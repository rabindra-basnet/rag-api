//! Route table only — middleware lives in `crate::middleware`, handlers in
//! their modules (auth = controllers for auth, rag = controllers for RAG).

use axum::middleware::from_fn_with_state;
use axum::routing::{delete, get, post, put};
use axum::Router;

use crate::state::AppState;
use crate::{auth, rag};

pub fn router(state: AppState) -> Router {
    let public = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/auth/register", post(auth::handlers::register))
        .route("/auth/login", post(auth::handlers::login))
        .route("/auth/refresh", post(auth::handlers::refresh))
        .route("/auth/logout", post(auth::handlers::logout));

    let protected = Router::new()
        .route("/auth/me", get(auth::handlers::me))
        .route("/documents", post(rag::handlers::ingest_document))
        .route("/documents", get(rag::handlers::list_documents))
        .route("/documents/{id}", put(rag::handlers::update_document))
        .route("/documents/{id}", delete(rag::handlers::delete_document))
        .route("/chat", post(rag::handlers::chat))
        .route_layer(from_fn_with_state(
            state.clone(),
            auth::middleware::require_auth,
        ));

    crate::middleware::apply(public.merge(protected)).with_state(state)
}
