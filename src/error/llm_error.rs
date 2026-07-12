use crate::response::api_response::ApiErrorResponse;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("llm upstream error: {0}")]
    UpstreamError(String),
    #[error("embeddings count mismatch")]
    EmbeddingsCountMismatch,
    #[error("no query embedding")]
    NoQueryEmbedding,
    #[error("llm returned no content")]
    NoContent,
    #[error("api key not configured")]
    ApiKeyNotConfigured,
}

impl IntoResponse for LlmError {
    fn into_response(self) -> Response {
        tracing::error!(error = %self, "llm error");
        match &self {
            LlmError::ApiKeyNotConfigured => {
                ApiErrorResponse::send(StatusCode::INTERNAL_SERVER_ERROR.as_u16(), Some("llm not configured".into()))
            }
            _ => {
                ApiErrorResponse::send(StatusCode::INTERNAL_SERVER_ERROR.as_u16(), Some("llm upstream error".into()))
            }
        }
    }
}
