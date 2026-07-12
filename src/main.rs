mod auth;
mod db;
mod error;
mod llm;
mod rag;
mod state;
mod validation;

use std::time::Duration;

use axum::http::header::AUTHORIZATION;
use axum::middleware;
use axum::routing::{delete, get, post, put};
use axum::Router;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::sensitive_headers::SetSensitiveRequestHeadersLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use state::{openai_client, AppState, Config};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rag_backend=debug,tower_http=info".into()),
        )
        .init();

    let cfg = Config::from_env();
    let pool = db::init(&cfg.database_url).await?;

    let state = AppState {
        db: pool,
        llm: openai_client(&cfg.llm_base_url, &cfg.llm_api_key),
        embeddings: openai_client(&cfg.embeddings_base_url, &cfg.embeddings_api_key),
        cfg,
    };

    let protected = Router::new()
        .route("/auth/me", get(auth::handlers::me))
        .route("/documents", post(rag::handlers::ingest_document))
        .route("/documents", get(rag::handlers::list_documents))
        .route("/documents/{id}", put(rag::handlers::update_document))
        .route("/documents/{id}", delete(rag::handlers::delete_document))
        .route("/chat", post(rag::handlers::chat))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::middleware::require_auth,
        ));

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/auth/register", post(auth::handlers::register))
        .route("/auth/login", post(auth::handlers::login))
        .route("/auth/refresh", post(auth::handlers::refresh))
        .route("/auth/logout", post(auth::handlers::logout))
        .merge(protected)
        // Layers run top-to-bottom on the request, bottom-to-top on the response.
        .layer(CatchPanicLayer::new()) // panic in a handler -> 500, not a dropped connection
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetSensitiveRequestHeadersLayer::new([AUTHORIZATION])) // redact from traces
        .layer(TraceLayer::new_for_http())
        // Generous timeout: /documents and /chat wait on upstream LLM calls.
        .layer(TimeoutLayer::new(Duration::from_secs(120)))
        .layer(CorsLayer::permissive())
        .layer(CompressionLayer::new())
        // Documents are capped at 10 MB each in the handler; this is the
        // whole-request cap so a JSON batch of several documents still fits.
        .layer(RequestBodyLimitLayer::new(50 * 1024 * 1024)) // 50 MiB
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind(&state.cfg.bind_addr).await?;
    tracing::info!("listening on {}", state.cfg.bind_addr);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received, draining connections");
}
