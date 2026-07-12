use axum::extract::{FromRequestParts, State};
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::Response;
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;

use super::jwt::verify_access_token;

/// Authenticated user, inserted into request extensions by `require_auth`.
#[derive(Clone, Debug)]
pub struct AuthUser {
    pub id: Uuid,
    pub email: String,
}

impl<S: Send + Sync> FromRequestParts<S> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthUser>()
            .cloned()
            .ok_or(ApiError::Unauthorized)
    }
}

pub async fn require_auth(
    State(state): State<AppState>,
    mut req: axum::extract::Request,
    next: Next,
) -> Result<Response, ApiError> {
    let token = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(ApiError::Unauthorized)?;

    let claims = verify_access_token(&state.cfg.jwt_secret, token)?;
    let id = Uuid::parse_str(&claims.sub).map_err(|_| ApiError::Unauthorized)?;

    req.extensions_mut().insert(AuthUser {
        id,
        email: claims.email,
    });
    Ok(next.run(req).await)
}
