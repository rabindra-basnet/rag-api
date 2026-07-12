mod auth;
mod db;
mod error;
mod llm;
mod middleware;
mod rag;
mod routes;
mod state;
mod validation;

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

    let bind_addr = state.cfg.bind_addr.clone();
    let app = routes::router(state);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("listening on {bind_addr}");
    axum::serve(
        listener,
        // ConnectInfo gives the rate limiter the peer IP when no proxy
        // headers are present.
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
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
