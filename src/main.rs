mod auth;
mod db;
mod error;
mod llm;
mod rag;
mod state;

use axum::middleware;
use axum::routing::{delete, get, post};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
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
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .layer(RequestBodyLimitLayer::new(2 * 1024 * 1024)) // 2 MiB
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind(&state.cfg.bind_addr).await?;
    tracing::info!("listening on {}", state.cfg.bind_addr);
    axum::serve(listener, app).await?;
    Ok(())
}
