use crate::config::database::Db;
use crate::entity::provider::Provider;
use crate::entity::user::User;
use crate::error::ApiError;

pub async fn find_by_email(db: &Db, email: &str) -> Result<Option<User>, ApiError> {
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = ?")
        .bind(email)
        .fetch_optional(db)
        .await?;
    Ok(user)
}

pub async fn find_by_id(db: &Db, id: &str) -> Result<Option<User>, ApiError> {
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
        .bind(id)
        .fetch_optional(db)
        .await?;
    Ok(user)
}

pub async fn find_by_username(db: &Db, username: &str) -> Result<Option<User>, ApiError> {
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE username = ?")
        .bind(username)
        .fetch_optional(db)
        .await?;
    Ok(user)
}

pub async fn create(
    db: &Db,
    id: &str,
    email: &str,
    password_hash: &str,
    name: Option<&str>,
    username: Option<&str>,
) -> Result<User, ApiError> {
    let account_id = uuid::Uuid::new_v4().to_string();
    let display_username = username.map(|u| format!("@{u}"));
    let now = chrono::Utc::now().to_rfc3339();

    let mut tx = db.begin().await?;

    sqlx::query(
        "INSERT INTO users (id, email, email_verified, name, username, display_username, metadata, created_at, updated_at)
         VALUES (?, ?, 0, ?, ?, ?, '{}', ?, ?)",
    )
    .bind(id)
    .bind(email)
    .bind(name)
    .bind(username)
    .bind(&display_username)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO accounts (id, account_id, provider_id, user_id, password, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&account_id)
    .bind(email)
    .bind(Provider::Credential.as_str())
    .bind(id)
    .bind(password_hash)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let user = find_by_id(db, id)
        .await?
        .ok_or_else(|| ApiError::Internal("failed to fetch created user".into()))?;
    Ok(user)
}

pub async fn update_profile(
    db: &Db,
    user_id: &str,
    name: Option<&str>,
    username: Option<&str>,
) -> Result<User, ApiError> {
    let display_username = username.map(|u| format!("@{u}"));
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "UPDATE users SET name = COALESCE(?, name), username = COALESCE(?, username), display_username = COALESCE(?, display_username), updated_at = ? WHERE id = ?",
    )
    .bind(name)
    .bind(username)
    .bind(&display_username)
    .bind(&now)
    .bind(user_id)
    .execute(db)
    .await?;

    find_by_id(db, user_id)
        .await?
        .ok_or_else(|| ApiError::Internal("failed to fetch updated user".into()))
}

pub async fn get_account_password(db: &Db, user_id: &str) -> Result<Option<String>, ApiError> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT password FROM accounts WHERE user_id = ? AND provider_id = ?")
            .bind(user_id)
            .bind(Provider::Credential.as_str())
            .fetch_optional(db)
            .await?;
    Ok(row.and_then(|r| r.0))
}
