use crate::response::api_response::ApiErrorResponse;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("unauthorized")]
    Unauthorized,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("bad request: {0}")]
    BadRequest(String),
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        match &self {
            AuthError::InvalidCredentials => {
                ApiErrorResponse::send(StatusCode::UNAUTHORIZED.as_u16(), Some("invalid email or password".into()))
            }
            AuthError::Unauthorized => {
                ApiErrorResponse::send(StatusCode::UNAUTHORIZED.as_u16(), Some("unauthorized".into()))
            }
            AuthError::Conflict(m) => {
                ApiErrorResponse::send(StatusCode::CONFLICT.as_u16(), Some(m.clone()))
            }
            AuthError::BadRequest(m) => {
                ApiErrorResponse::send(StatusCode::BAD_REQUEST.as_u16(), Some(m.clone()))
            }
        }
    }
}
