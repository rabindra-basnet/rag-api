use crate::response::api_response::ApiErrorResponse;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error")]
    QueryError(String),
    #[error("unique constraint violation")]
    UniqueConstraintViolation(String),
}

impl From<sqlx::Error> for DbError {
    fn from(e: sqlx::Error) -> Self {
        tracing::error!(error = %e, "database error");
        DbError::QueryError(e.to_string())
    }
}

impl IntoResponse for DbError {
    fn into_response(self) -> Response {
        match &self {
            DbError::QueryError(_) => {
                ApiErrorResponse::send(StatusCode::INTERNAL_SERVER_ERROR.as_u16(), Some("database error".into()))
            }
            DbError::UniqueConstraintViolation(_) => {
                ApiErrorResponse::send(StatusCode::CONFLICT.as_u16(), Some(self.to_string()))
            }
        }
    }
}
