//! Route table only — middleware lives in `crate::middleware`, handlers in
//! their modules (auth = controllers for auth, rag = controllers for RAG).

use axum::middleware::from_fn_with_state;
use axum::routing::{delete, get, post, put};
use axum::Router;

use crate::state::AppState;
use crate::{auth, rag};

pub fn router(state: AppState) -> Router {
    let trust_proxy = state.cfg.trust_proxy;

    // Credential endpoints get a tight budget (blunts brute force);
    // everything else gets a budget sized for normal app traffic.
    let auth_public = crate::middleware::rate_limit(
        Router::new()
            .route("/auth/register", post(auth::handlers::register))
            .route("/auth/login", post(auth::handlers::login))
            .route("/auth/refresh", post(auth::handlers::refresh))
            .route("/auth/logout", post(auth::handlers::logout)),
        1,  // refill 1 req/s
        10, // burst 10
        trust_proxy,
    );

    let public = Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(auth_public);

    let protected = Router::new()
        .route("/auth/me", get(auth::handlers::me))
        .route("/documents", post(rag::handlers::ingest_document))
        .route("/documents", get(rag::handlers::list_documents))
        .route("/documents/{id}", put(rag::handlers::update_document))
        .route("/documents/{id}", delete(rag::handlers::delete_document))
        .route("/chat", post(rag::handlers::chat))
        .route("/files", post(crate::files::upload))
        .route("/files", get(crate::files::list))
        .route("/files/{id}", get(crate::files::download))
        .route("/files/{id}/ingest", post(crate::files::ingest))
        .route("/files/{id}", delete(crate::files::delete))
        .route_layer(from_fn_with_state(
            state.clone(),
            auth::middleware::require_auth,
        ));
    let protected = crate::middleware::rate_limit(protected, 5, 50, trust_proxy);

    crate::middleware::apply(public.merge(protected)).with_state(state)
}
