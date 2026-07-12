use axum::routing::{delete, get, post};
use axum::Router;

use crate::handler::file_handler;
use crate::state::app_state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/files", post(file_handler::upload))
        .route("/files", get(file_handler::list))
        .route("/files/{id}", get(file_handler::download))
        .route("/files/{id}/ingest", post(file_handler::ingest))
        .route("/files/{id}", delete(file_handler::delete))
}
