use chrono::{Duration, Utc};
use rand::RngCore;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::error::ApiError;

/// Opaque refresh token: 32 random bytes, hex to the client, only the
/// SHA-256 hash stored. Rotation is tracked per family so a replayed
/// (already-rotated) token revokes the whole family.
pub struct IssuedRefresh {
    pub token: String,
}

fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

fn new_opaque_token() -> String {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    hex::encode(buf)
}

pub async fn issue(
    db: &SqlitePool,
    user_id: Uuid,
    family_id: Option<Uuid>,
    ttl_days: i64,
) -> Result<IssuedRefresh, ApiError> {
    let token = new_opaque_token();
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
    db: &SqlitePool,
    token: &str,
    ttl_days: i64,
) -> Result<RotatedRefresh, ApiError> {
    let hash = hash_token(token);
    let row: Option<(String, String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, user_id, family_id, expires_at, revoked_at
         FROM refresh_tokens WHERE token_hash = ?",
    )
    .bind(&hash)
    .fetch_optional(db)
    .await?;

    let Some((id, user_id, family_id, expires_at, revoked_at)) = row else {
        return Err(ApiError::Unauthorized);
    };

    if revoked_at.is_some() {
        // Token reuse — revoke the entire family.
        tracing::warn!(user_id, "refresh token reuse detected; revoking family");
        sqlx::query("UPDATE refresh_tokens SET revoked_at = ? WHERE family_id = ? AND revoked_at IS NULL")
            .bind(Utc::now().to_rfc3339())
            .bind(&family_id)
            .execute(db)
            .await?;
        return Err(ApiError::Unauthorized);
    }

    let expired = chrono::DateTime::parse_from_rfc3339(&expires_at)
        .map(|t| t < Utc::now())
        .unwrap_or(true);
    if expired {
        return Err(ApiError::Unauthorized);
    }

    sqlx::query("UPDATE refresh_tokens SET revoked_at = ? WHERE id = ?")
        .bind(Utc::now().to_rfc3339())
        .bind(&id)
        .execute(db)
        .await?;

    let user_uuid = Uuid::parse_str(&user_id).map_err(|_| ApiError::Unauthorized)?;
    let family_uuid = Uuid::parse_str(&family_id).ok();
    let issued = issue(db, user_uuid, family_uuid, ttl_days).await?;

    Ok(RotatedRefresh {
        user_id: user_uuid,
        new_token: issued.token,
    })
}

pub async fn revoke(db: &SqlitePool, token: &str) -> Result<(), ApiError> {
    sqlx::query("UPDATE refresh_tokens SET revoked_at = ? WHERE token_hash = ? AND revoked_at IS NULL")
        .bind(Utc::now().to_rfc3339())
        .bind(hash_token(token))
        .execute(db)
        .await?;
    Ok(())
}
