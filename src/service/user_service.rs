use crate::config::database::Db;
use crate::entity::user::User;
use crate::error::ApiError;
use crate::error::auth_error::AuthError;
use crate::repository;

pub async fn find_by_email(db: &Db, email: &str) -> Result<Option<User>, ApiError> {
    repository::user_repository::find_by_email(db, email).await
}

pub async fn find_by_id(db: &Db, id: &str) -> Result<Option<User>, ApiError> {
    repository::user_repository::find_by_id(db, id).await
}

pub async fn find_by_username(db: &Db, username: &str) -> Result<Option<User>, ApiError> {
    repository::user_repository::find_by_username(db, username).await
}

pub async fn create_user(
    db: &Db,
    email: &str,
    password: &str,
    name: Option<&str>,
    username: Option<&str>,
) -> Result<User, ApiError> {
    let hash = crate::service::auth_service::hash_password(password)?;
    let id = uuid::Uuid::new_v4().to_string();
    repository::user_repository::create(db, &id, email, &hash, name, username).await
}

pub async fn update_profile(
    db: &Db,
    user_id: &str,
    name: Option<&str>,
    username: Option<&str>,
) -> Result<User, ApiError> {
    if let Some(u) = username {
        if let Some(existing) = repository::user_repository::find_by_username(db, u).await? {
            if existing.id != user_id {
                return Err(ApiError::Auth(AuthError::Conflict("username already taken".into())));
            }
        }
    }
    repository::user_repository::update_profile(db, user_id, name, username).await
}
