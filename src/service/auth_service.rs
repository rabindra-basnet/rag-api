use argon2::password_hash::{
    rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use argon2::Argon2;
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::database::Db;
use crate::entity::user::{Session, User};
use crate::error::auth_error::AuthError;
use crate::error::ApiError;

const ISSUER: &str = "rag-backend";
const AUDIENCE: &str = "rag-backend-api";

// --- Password ---

pub fn hash_password(password: &str) -> Result<String, ApiError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| {
            tracing::error!(error = %e, "argon2 hash failure");
            ApiError::Internal("hash failure".into())
        })
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|parsed| {
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        })
        .unwrap_or(false)
}

pub fn validate_password_strength(pw: &str) -> Result<(), ApiError> {
    if pw.len() < 8 {
        return Err(ApiError::BadRequest(
            "password must be at least 8 characters".into(),
        ));
    }
    if pw.len() > 128 {
        return Err(ApiError::BadRequest("password too long".into()));
    }
    Ok(())
}

// --- JWT access token ---

#[derive(Debug, Serialize, Deserialize)]
pub struct AccessClaims {
    pub sub: String,
    pub email: Option<String>,
    pub typ: String,
    pub jti: String,
    pub iss: String,
    pub aud: String,
    pub iat: i64,
    pub nbf: i64,
    pub exp: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RefreshClaims {
    pub sub: String,
    pub sid: String,
    pub typ: String,
    pub jti: String,
    pub iss: String,
    pub aud: String,
    pub iat: i64,
    pub nbf: i64,
    pub exp: i64,
}

pub fn issue_access_token(
    secret: &str,
    user_id: &str,
    email: &Option<String>,
    ttl_minutes: i64,
) -> Result<String, ApiError> {
    let now = Utc::now();
    let claims = AccessClaims {
        sub: user_id.to_string(),
        email: email.clone(),
        typ: "access".into(),
        jti: Uuid::new_v4().to_string(),
        iss: ISSUER.to_string(),
        aud: AUDIENCE.to_string(),
        iat: now.timestamp(),
        nbf: now.timestamp(),
        exp: (now + Duration::minutes(ttl_minutes)).timestamp(),
    };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| {
        tracing::error!(error = %e, "jwt encode failure");
        ApiError::Internal("token failure".into())
    })
}

pub fn verify_access_token(secret: &str, token: &str) -> Result<AccessClaims, ApiError> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_issuer(&[ISSUER]);
    validation.set_audience(&[AUDIENCE]);
    validation.set_required_spec_claims(&["exp", "nbf"]);
    validation.leeway = 30;

    decode::<AccessClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map(|d| d.claims)
    .map_err(|_| ApiError::Auth(AuthError::Unauthorized))
}

pub fn issue_refresh_token(
    secret: &str,
    user_id: &str,
    session_id: &str,
    ttl_days: i64,
) -> Result<String, ApiError> {
    let now = Utc::now();
    let claims = RefreshClaims {
        sub: user_id.to_string(),
        sid: session_id.to_string(),
        typ: "refresh".into(),
        jti: Uuid::new_v4().to_string(),
        iss: ISSUER.to_string(),
        aud: AUDIENCE.to_string(),
        iat: now.timestamp(),
        nbf: now.timestamp(),
        exp: (now + Duration::days(ttl_days)).timestamp(),
    };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| {
        tracing::error!(error = %e, "jwt encode failure");
        ApiError::Internal("token failure".into())
    })
}

pub fn verify_refresh_token(secret: &str, token: &str) -> Result<RefreshClaims, ApiError> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_issuer(&[ISSUER]);
    validation.set_audience(&[AUDIENCE]);
    validation.set_required_spec_claims(&["exp", "nbf"]);
    validation.leeway = 30;

    decode::<RefreshClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map(|d| d.claims)
    .map_err(|_| ApiError::Auth(AuthError::Unauthorized))
}

// --- Token revocation (jti denylist) ---

pub async fn revoke_token(db: &Db, jti: &str, expires_at: &str) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM revoked_tokens WHERE expires_at < ?")
        .bind(Utc::now().to_rfc3339())
        .execute(db)
        .await?;

    sqlx::query("INSERT OR IGNORE INTO revoked_tokens (jti, expires_at) VALUES (?, ?)")
        .bind(jti)
        .bind(expires_at)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn is_token_revoked(db: &Db, jti: &str) -> Result<bool, ApiError> {
    let found: Option<(String,)> =
        sqlx::query_as("SELECT jti FROM revoked_tokens WHERE jti = ?")
            .bind(jti)
            .fetch_optional(db)
            .await?;
    Ok(found.is_some())
}

// --- Session token helpers (reserved for future stateful refresh) ---

#[allow(dead_code)]
pub fn hash_session_token(token: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(token.as_bytes()))
}

#[allow(dead_code)]
pub async fn create_session(
    db: &Db,
    user_id: &str,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
    ttl_days: i64,
) -> Result<(String, String), ApiError> {
    let session_id = Uuid::new_v4().to_string();
    let token = Uuid::new_v4().to_string();
    let token_hash = hash_session_token(&token);
    let now = Utc::now();
    let expires_at = (now + Duration::days(ttl_days)).to_rfc3339();

    sqlx::query(
        "INSERT INTO sessions (id, expires_at, token, ip_address, user_agent, user_id, active, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, 1, ?, ?)",
    )
    .bind(&session_id)
    .bind(&expires_at)
    .bind(&token_hash)
    .bind(ip_address)
    .bind(user_agent)
    .bind(user_id)
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .execute(db)
    .await?;

    Ok((session_id, token))
}

#[allow(dead_code)]
pub async fn validate_session(db: &Db, token: &str) -> Result<(String, User), ApiError> {
    let token_hash = hash_session_token(token);
    let now = Utc::now().to_rfc3339();

    let session: Session =
        sqlx::query_as("SELECT * FROM sessions WHERE token = ? AND active = 1 AND expires_at > ?")
            .bind(&token_hash)
            .bind(&now)
            .fetch_optional(db)
            .await?
            .ok_or(ApiError::Auth(AuthError::Unauthorized))?;

    let user: User = sqlx::query_as("SELECT * FROM users WHERE id = ?")
        .bind(&session.user_id)
        .fetch_optional(db)
        .await?
        .ok_or(ApiError::Auth(AuthError::Unauthorized))?;

    Ok((session.id, user))
}

#[allow(dead_code)]
pub async fn validate_session_by_id(db: &Db, session_id: &str) -> Result<User, ApiError> {
    let now = Utc::now().to_rfc3339();
    let session: Session = sqlx::query_as(
        "SELECT * FROM sessions WHERE id = ? AND active = 1 AND expires_at > ?",
    )
    .bind(session_id)
    .bind(&now)
    .fetch_optional(db)
    .await?
    .ok_or(ApiError::Auth(AuthError::Unauthorized))?;

    let user: User = sqlx::query_as("SELECT * FROM users WHERE id = ?")
        .bind(&session.user_id)
        .fetch_optional(db)
        .await?
        .ok_or(ApiError::Auth(AuthError::Unauthorized))?;

    Ok(user)
}

#[allow(dead_code)]
pub async fn rotate_session(db: &Db, old_token: &str, ttl_days: i64) -> Result<String, ApiError> {
    let token_hash = hash_session_token(old_token);
    let now = Utc::now();

    let session: Session = sqlx::query_as("SELECT * FROM sessions WHERE token = ? AND active = 1")
        .bind(&token_hash)
        .fetch_optional(db)
        .await?
        .ok_or(ApiError::Auth(AuthError::Unauthorized))?;

    let expired = chrono::DateTime::parse_from_rfc3339(&session.expires_at)
        .map(|t| t < now)
        .unwrap_or(true);
    if expired {
        return Err(ApiError::Auth(AuthError::Unauthorized));
    }

    sqlx::query("UPDATE sessions SET active = 0, updated_at = ? WHERE id = ?")
        .bind(now.to_rfc3339())
        .bind(&session.id)
        .execute(db)
        .await?;

    let (_sid, _token) = create_session(db, &session.user_id, None, None, ttl_days).await?;
    Ok(_token)
}

#[allow(dead_code)]
pub async fn revoke_session(db: &Db, token: &str) -> Result<(), ApiError> {
    let token_hash = hash_session_token(token);
    sqlx::query("UPDATE sessions SET active = 0, updated_at = ? WHERE token = ? AND active = 1")
        .bind(Utc::now().to_rfc3339())
        .bind(&token_hash)
        .execute(db)
        .await?;
    Ok(())
}

#[allow(dead_code)]
pub async fn revoke_session_by_id(db: &Db, session_id: &str) -> Result<(), ApiError> {
    sqlx::query("UPDATE sessions SET active = 0, updated_at = ? WHERE id = ? AND active = 1")
        .bind(Utc::now().to_rfc3339())
        .bind(session_id)
        .execute(db)
        .await?;
    Ok(())
}

#[allow(dead_code)]
pub async fn revoke_all_user_sessions(db: &Db, user_id: &str) -> Result<(), ApiError> {
    sqlx::query("UPDATE sessions SET active = 0, updated_at = ? WHERE user_id = ? AND active = 1")
        .bind(Utc::now().to_rfc3339())
        .bind(user_id)
        .execute(db)
        .await?;
    Ok(())
}

#[allow(dead_code)]
pub async fn find_session_owner(db: &Db, token: &str) -> Result<Option<String>, ApiError> {
    let token_hash = hash_session_token(token);
    let session: Option<Session> = sqlx::query_as("SELECT * FROM sessions WHERE token = ?")
        .bind(&token_hash)
        .fetch_optional(db)
        .await?;
    Ok(session.map(|s| s.user_id))
}
