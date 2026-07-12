use axum::routing::{delete, get, post, put};
use axum::Router;

use crate::handler::document_handler;
use crate::state::app_state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/documents", post(document_handler::ingest_document))
        .route("/documents", get(document_handler::list_documents))
        .route("/documents/{id}", put(document_handler::update_document))
        .route("/documents/{id}", delete(document_handler::delete_document))
}
