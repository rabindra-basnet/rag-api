use axum::extract::State;
use axum::Json;
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use time::Duration as CookieDuration;
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;

use super::jwt::issue_access_token;
use super::middleware::AuthUser;
use super::password::{hash_password, validate_password_strength, verify_password};
use super::tokens;

pub const REFRESH_COOKIE: &str = "refresh_token";

#[derive(Deserialize, validator::Validate)]
pub struct RegisterReq {
    #[validate(email(message = "must be a valid email address"))]
    #[validate(length(max = 254, message = "email too long"))]
    pub email: String,
    #[validate(length(min = 8, max = 128, message = "password must be 8-128 characters"))]
    pub password: String,
}

#[derive(Deserialize, validator::Validate)]
pub struct LoginReq {
    #[validate(length(min = 1, max = 254, message = "email required"))]
    pub email: String,
    #[validate(length(min = 1, max = 128, message = "password required"))]
    pub password: String,
}

/// Body is optional: browser clients rely on the HttpOnly cookie instead.
#[derive(Deserialize, Default)]
pub struct RefreshReq {
    pub refresh_token: Option<String>,
}

fn valid_email(email: &str) -> bool {
    let email = email.trim();
    email.len() <= 254 && email.contains('@') && !email.starts_with('@') && !email.ends_with('@')
}

fn refresh_token_from(jar: &CookieJar, body: Option<Json<RefreshReq>>) -> Result<String, ApiError> {
    body.and_then(|Json(b)| b.refresh_token)
        .or_else(|| jar.get(REFRESH_COOKIE).map(|c| c.value().to_string()))
        .ok_or(ApiError::Unauthorized)
}

fn refresh_cookie(state: &AppState, token: String) -> Cookie<'static> {
    Cookie::build((REFRESH_COOKIE, token))
        .http_only(true)
        .secure(state.cfg.cookie_secure)
        .same_site(SameSite::Strict)
        .path("/auth") // only sent to /auth/refresh and /auth/logout
        .max_age(CookieDuration::days(state.cfg.refresh_ttl_days))
        .build()
}

fn clear_refresh_cookie(state: &AppState) -> Cookie<'static> {
    let mut c = refresh_cookie(state, String::new());
    c.set_max_age(CookieDuration::ZERO);
    c
}

pub async fn register(
    State(state): State<AppState>,
    jar: CookieJar,
    crate::validation::ValidatedJson(req): crate::validation::ValidatedJson<RegisterReq>,
) -> Result<(CookieJar, Json<Value>), ApiError> {
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

    session_response(&state, jar, id, &email).await
}

pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    crate::validation::ValidatedJson(req): crate::validation::ValidatedJson<LoginReq>,
) -> Result<(CookieJar, Json<Value>), ApiError> {
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

    session_response(&state, jar, id, &email).await
}

pub async fn refresh(
    State(state): State<AppState>,
    jar: CookieJar,
    body: Option<Json<RefreshReq>>,
) -> Result<(CookieJar, Json<Value>), ApiError> {
    let token = refresh_token_from(&jar, body)?;
    let rotated = tokens::rotate(
        &state.db,
        &state.cfg.jwt_secret,
        &token,
        state.cfg.refresh_ttl_days,
    )
    .await?;

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

    let jar = jar.add(refresh_cookie(&state, rotated.new_token.clone()));
    Ok((
        jar,
        Json(json!({
            "access_token": access,
            "refresh_token": rotated.new_token,
            "token_type": "Bearer",
            "expires_in": state.cfg.access_ttl_minutes * 60,
        })),
    ))
}

pub async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
    body: Option<Json<RefreshReq>>,
) -> Result<(CookieJar, Json<Value>), ApiError> {
    if let Ok(token) = refresh_token_from(&jar, body) {
        tokens::revoke(&state.db, &token).await?;
    }
    let jar = jar.remove(clear_refresh_cookie(&state));
    Ok((jar, Json(json!({ "ok": true }))))
}

pub async fn me(user: AuthUser) -> Json<Value> {
    Json(json!({ "id": user.id, "email": user.email }))
}

async fn session_response(
    state: &AppState,
    jar: CookieJar,
    user_id: Uuid,
    email: &str,
) -> Result<(CookieJar, Json<Value>), ApiError> {
    let access = issue_access_token(
        &state.cfg.jwt_secret,
        user_id,
        email,
        state.cfg.access_ttl_minutes,
    )?;
    let refresh = tokens::issue(
        &state.db,
        &state.cfg.jwt_secret,
        user_id,
        None,
        state.cfg.refresh_ttl_days,
    )
    .await?;

    let jar = jar.add(refresh_cookie(state, refresh.token.clone()));
    Ok((
        jar,
        Json(json!({
            "access_token": access,
            "refresh_token": refresh.token,
            "token_type": "Bearer",
            "expires_in": state.cfg.access_ttl_minutes * 60,
        })),
    ))
}
