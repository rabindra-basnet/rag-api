use chrono::{Duration, Utc};
use sha2::{Digest, Sha256};
use crate::db::Db;
use uuid::Uuid;

use crate::error::ApiError;

use super::jwt::{issue_refresh_token, verify_refresh_token};

/// Refresh tokens are signed JWTs (typ = "refresh"), but the SHA-256 hash of
/// every issued token is also stored server-side. A refresh therefore needs
/// BOTH a valid signature AND a live DB row — so tokens stay revocable, are
/// rotated on every use, and replaying a rotated token revokes its whole
/// family (reuse detection).
pub struct IssuedRefresh {
    pub token: String,
}

fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

pub async fn issue(
    db: &Db,
    jwt_secret: &str,
    user_id: Uuid,
    family_id: Option<Uuid>,
    ttl_days: i64,
) -> Result<IssuedRefresh, ApiError> {
    let token = issue_refresh_token(jwt_secret, user_id, ttl_days)?;
    let now = Utc::now();
    sqlx::query(
        "INSERT INTO refresh_tokens (id, user_id, token_hash, family_id, expires_at, created_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(user_id.to_string())
    .bind(hash_token(&token))
    .bind(family_id.unwrap_or_else(Uuid::new_v4).to_string())
    .bind((now + Duration::days(ttl_days)).to_rfc3339())
    .bind(now.to_rfc3339())
    .execute(db)
    .await?;
    Ok(IssuedRefresh { token })
}

pub struct RotatedRefresh {
    pub user_id: Uuid,
    pub new_token: String,
}

/// Validate + rotate a refresh token. Reuse of a revoked token kills the family.
pub async fn rotate(
    db: &Db,
    jwt_secret: &str,
    token: &str,
    ttl_days: i64,
) -> Result<RotatedRefresh, ApiError> {
    // Signature, exp, iss/aud, and typ checked before we ever touch the DB.
    let claims = verify_refresh_token(jwt_secret, token)?;

    let hash = hash_token(token);
    let row: Option<crate::models::RefreshToken> =
        sqlx::query_as("SELECT * FROM refresh_tokens WHERE token_hash = ?")
            .bind(&hash)
            .fetch_optional(db)
            .await?;

    let Some(row) = row else {
        return Err(ApiError::Unauthorized);
    };

    // The signed sub must match the row we found for that token.
    if claims.sub != row.user_id {
        return Err(ApiError::Unauthorized);
    }

    if row.revoked_at.is_some() {
        // Token reuse — revoke the entire family.
        tracing::warn!(user_id = row.user_id, "refresh token reuse detected; revoking family");
        sqlx::query("UPDATE refresh_tokens SET revoked_at = ? WHERE family_id = ? AND revoked_at IS NULL")
            .bind(Utc::now().to_rfc3339())
            .bind(&row.family_id)
            .execute(db)
            .await?;
        return Err(ApiError::Unauthorized);
    }

    // DB-side expiry check (belt to the JWT exp's suspenders).
    let expired = chrono::DateTime::parse_from_rfc3339(&row.expires_at)
        .map(|t| t < Utc::now())
        .unwrap_or(true);
    if expired {
        return Err(ApiError::Unauthorized);
    }

    sqlx::query("UPDATE refresh_tokens SET revoked_at = ? WHERE id = ?")
        .bind(Utc::now().to_rfc3339())
        .bind(&row.id)
        .execute(db)
        .await?;

    let user_uuid = Uuid::parse_str(&row.user_id).map_err(|_| ApiError::Unauthorized)?;
    let family_uuid = Uuid::parse_str(&row.family_id).ok();
    let issued = issue(db, jwt_secret, user_uuid, family_uuid, ttl_days).await?;

    Ok(RotatedRefresh {
        user_id: user_uuid,
        new_token: issued.token,
    })
}

pub async fn revoke(db: &Db, token: &str) -> Result<(), ApiError> {
    sqlx::query("UPDATE refresh_tokens SET revoked_at = ? WHERE token_hash = ? AND revoked_at IS NULL")
        .bind(Utc::now().to_rfc3339())
        .bind(hash_token(token))
        .execute(db)
        .await?;
    Ok(())
}
