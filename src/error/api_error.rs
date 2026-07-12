use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};

use super::auth_error::AuthError;
use super::db_error::DbError;
use super::llm_error::LlmError;

static DEV_MODE: AtomicBool = AtomicBool::new(false);

pub fn set_dev_mode(enabled: bool) {
    DEV_MODE.store(enabled, Ordering::Relaxed);
}

fn dev_mode() -> bool {
    DEV_MODE.load(Ordering::Relaxed)
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    BadRequest(String),
    #[error("not found")]
    NotFound,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    Llm(#[from] LlmError),
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
        tracing::error!(error = %e, "upstream request error");
        ApiError::Internal("upstream error".into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            ApiError::Auth(e) => e.into_response(),
            ApiError::Db(e) => e.into_response(),
            ApiError::Llm(e) => e.into_response(),
            other => {
                let (status, msg) = match &other {
                    ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
                    ApiError::NotFound => (StatusCode::NOT_FOUND, "not found".into()),
                    ApiError::Conflict(m) => (StatusCode::CONFLICT, m.clone()),
                    ApiError::Internal(detail) => (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        if dev_mode() {
                            detail.clone()
                        } else {
                            "internal server error".into()
                        },
                    ),
                    _ => unreachable!(),
                };
                (status, Json(json!({ "error": msg }))).into_response()
            }
        }
    }
}
