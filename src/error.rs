use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};

/// Development mode: internal error details are returned to the client
/// instead of a generic message. Set once at startup from ENVIRONMENT.
static DEV_MODE: AtomicBool = AtomicBool::new(false);

pub fn set_dev_mode(enabled: bool) {
    DEV_MODE.store(enabled, Ordering::Relaxed);
}

pub fn dev_mode() -> bool {
    DEV_MODE.load(Ordering::Relaxed)
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    BadRequest(String),
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("unauthorized")]
    Unauthorized,
    #[error("not found")]
    NotFound,
    #[error("conflict: {0}")]
    Conflict(String),
    // Internal detail is logged, never sent to the client.
    #[error("internal error")]
    Internal(String),
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        tracing::error!(error = %e, "database error");
        ApiError::Internal("database error".into())
    }
}

impl From<reqwest::Error> for ApiError {
    fn from(e: reqwest::Error) -> Self {
        tracing::error!(error = %e, "llm upstream request error");
        ApiError::Internal("llm upstream error".into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
            ApiError::InvalidCredentials => {
                (StatusCode::UNAUTHORIZED, "invalid email or password".into())
            }
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized".into()),
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not found".into()),
            ApiError::Conflict(m) => (StatusCode::CONFLICT, m.clone()),
            // Production: generic message, detail only in server logs.
            // Development: the real error goes back to the client.
            ApiError::Internal(detail) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                if dev_mode() {
                    detail.clone()
                } else {
                    "internal server error".into()
                },
            ),
        };
        (status, Json(json!({ "error": msg }))).into_response()
    }
}
