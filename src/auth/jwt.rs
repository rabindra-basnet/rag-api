use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::ApiError;

const ISSUER: &str = "rag-backend";
const AUDIENCE: &str = "rag-backend-api";

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // user id
    pub email: String,
    pub jti: String, // unique token id
    pub iss: String,
    pub aud: String,
    pub typ: String, // "access" — refresh tokens are opaque, never JWTs
    pub iat: i64,
    pub nbf: i64,
    pub exp: i64,
}

pub fn issue_access_token(
    secret: &str,
    user_id: Uuid,
    email: &str,
    ttl_minutes: i64,
) -> Result<String, ApiError> {
    let now = Utc::now();
    let claims = Claims {
        sub: user_id.to_string(),
        email: email.to_string(),
        jti: Uuid::new_v4().to_string(),
        iss: ISSUER.to_string(),
        aud: AUDIENCE.to_string(),
        typ: "access".to_string(),
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

#[derive(Debug, Serialize, Deserialize)]
pub struct RefreshClaims {
    pub sub: String,
    pub jti: String,
    pub iss: String,
    pub aud: String,
    pub typ: String, // "refresh"
    pub iat: i64,
    pub nbf: i64,
    pub exp: i64,
}

/// Refresh tokens are JWTs too, but every one is also hashed into the DB so
/// rotation, revocation, and family reuse-detection still apply — the
/// signature alone is never enough to refresh.
pub fn issue_refresh_token(
    secret: &str,
    user_id: Uuid,
    ttl_days: i64,
) -> Result<String, ApiError> {
    let now = Utc::now();
    let claims = RefreshClaims {
        sub: user_id.to_string(),
        jti: Uuid::new_v4().to_string(),
        iss: ISSUER.to_string(),
        aud: AUDIENCE.to_string(),
        typ: "refresh".to_string(),
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
        tracing::error!(error = %e, "refresh jwt encode failure");
        ApiError::Internal("token failure".into())
    })
}

pub fn verify_refresh_token(secret: &str, token: &str) -> Result<RefreshClaims, ApiError> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_issuer(&[ISSUER]);
    validation.set_audience(&[AUDIENCE]);
    validation.set_required_spec_claims(&["exp", "nbf", "iss", "aud", "sub"]);
    validation.leeway = 30;

    let claims = decode::<RefreshClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map(|d| d.claims)
    .map_err(|_| ApiError::Unauthorized)?;

    if claims.typ != "refresh" {
        return Err(ApiError::Unauthorized);
    }
    Ok(claims)
}

pub fn verify_access_token(secret: &str, token: &str) -> Result<Claims, ApiError> {
    // Pin the algorithm (prevents alg-confusion), validate exp/nbf/iss/aud.
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_issuer(&[ISSUER]);
    validation.set_audience(&[AUDIENCE]);
    validation.set_required_spec_claims(&["exp", "nbf", "iss", "aud", "sub"]);
    validation.leeway = 30;

    let claims = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map(|d| d.claims)
    .map_err(|_| ApiError::Unauthorized)?;

    if claims.typ != "access" {
        return Err(ApiError::Unauthorized);
    }
    Ok(claims)
}
