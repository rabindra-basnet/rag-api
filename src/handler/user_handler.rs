use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::dto::auth_dto::UpdateProfileReq;
use crate::dto::user_dto::UserReadDto;
use crate::error::ApiError;
use crate::error::request_error::ValidatedJson;
use crate::middleware::auth::AuthUser;
use crate::state::app_state::AppState;

use crate::service::user_service;

pub async fn me(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Value>, ApiError> {
    let db_user = user_service::find_by_id(&state.db, &user.id).await?
        .ok_or(ApiError::NotFound)?;
    let profile = UserReadDto::from(&db_user);
    Ok(Json(json!({ "user": profile })))
}

pub async fn update_profile(
    State(state): State<AppState>,
    user: AuthUser,
    ValidatedJson(req): ValidatedJson<UpdateProfileReq>,
) -> Result<Json<Value>, ApiError> {
    let name = req.name.as_deref().map(|n| n.trim()).filter(|n| !n.is_empty());
    let username = req.username.as_deref();

    let updated = user_service::update_profile(&state.db, &user.id, name, username).await?;
    let profile = UserReadDto::from(&updated);
    Ok(Json(json!({ "user": profile })))
}
