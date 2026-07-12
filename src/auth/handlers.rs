use axum::extract::State;
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;

use super::jwt::issue_access_token;
use super::middleware::AuthUser;
use super::password::{hash_password, validate_password_strength, verify_password};
use super::tokens;

#[derive(Deserialize)]
pub struct RegisterReq {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct LoginReq {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct RefreshReq {
    pub refresh_token: String,
}

fn valid_email(email: &str) -> bool {
    let email = email.trim();
    email.len() <= 254 && email.contains('@') && !email.starts_with('@') && !email.ends_with('@')
}

pub async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterReq>,
) -> Result<Json<Value>, ApiError> {
    let email = req.email.trim().to_lowercase();
    if !valid_email(&email) {
        return Err(ApiError::BadRequest("invalid email".into()));
    }
    validate_password_strength(&req.password)?;

    let existing: Option<(String,)> = sqlx::query_as("SELECT id FROM users WHERE email = ?")
        .bind(&email)
        .fetch_optional(&state.db)
        .await?;
    if existing.is_some() {
        return Err(ApiError::Conflict("email already registered".into()));
    }

    let id = Uuid::new_v4();
    let hash = hash_password(&req.password)?;
    sqlx::query("INSERT INTO users (id, email, password_hash, created_at) VALUES (?, ?, ?, ?)")
        .bind(id.to_string())
        .bind(&email)
        .bind(&hash)
        .bind(Utc::now().to_rfc3339())
        .execute(&state.db)
        .await?;

    session_response(&state, id, &email).await
}

pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginReq>,
) -> Result<Json<Value>, ApiError> {
    let email = req.email.trim().to_lowercase();
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT id, password_hash FROM users WHERE email = ?")
            .bind(&email)
            .fetch_optional(&state.db)
            .await?;

    // Verify against a dummy hash when the user doesn't exist to keep
    // timing consistent (no user-enumeration via response latency).
    const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$AAAAAAAAAAAAAAAAAAAAAA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let (id, hash) = match &row {
        Some((id, hash)) => (Some(id), hash.as_str()),
        None => (None, DUMMY_HASH),
    };
    let ok = verify_password(&req.password, hash);
    let Some(id) = id.filter(|_| ok) else {
        return Err(ApiError::InvalidCredentials);
    };
    let id = Uuid::parse_str(id).map_err(|_| ApiError::Internal("bad user id".into()))?;

    session_response(&state, id, &email).await
}

pub async fn refresh(
    State(state): State<AppState>,
    Json(req): Json<RefreshReq>,
) -> Result<Json<Value>, ApiError> {
    let rotated = tokens::rotate(&state.db, &req.refresh_token, state.cfg.refresh_ttl_days).await?;

    let (email,): (String,) = sqlx::query_as("SELECT email FROM users WHERE id = ?")
        .bind(rotated.user_id.to_string())
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::Unauthorized)?;

    let access = issue_access_token(
        &state.cfg.jwt_secret,
        rotated.user_id,
        &email,
        state.cfg.access_ttl_minutes,
    )?;

    Ok(Json(json!({
        "access_token": access,
        "refresh_token": rotated.new_token,
        "token_type": "Bearer",
        "expires_in": state.cfg.access_ttl_minutes * 60,
    })))
}

pub async fn logout(
    State(state): State<AppState>,
    Json(req): Json<RefreshReq>,
) -> Result<Json<Value>, ApiError> {
    tokens::revoke(&state.db, &req.refresh_token).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn me(user: AuthUser) -> Json<Value> {
    Json(json!({ "id": user.id, "email": user.email }))
}

async fn session_response(
    state: &AppState,
    user_id: Uuid,
    email: &str,
) -> Result<Json<Value>, ApiError> {
    let access = issue_access_token(
        &state.cfg.jwt_secret,
        user_id,
        email,
        state.cfg.access_ttl_minutes,
    )?;
    let refresh = tokens::issue(&state.db, user_id, None, state.cfg.refresh_ttl_days).await?;

    Ok(Json(json!({
        "access_token": access,
        "refresh_token": refresh.token,
        "token_type": "Bearer",
        "expires_in": state.cfg.access_ttl_minutes * 60,
    })))
}
