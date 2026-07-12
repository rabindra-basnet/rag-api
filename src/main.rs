use rag_backend::state::{http_client, AppState, Config};
use rag_backend::{db, routes};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let cfg = Config::from_env();
    let dev = cfg.environment == "development";
    rag_backend::error::set_dev_mode(dev);
    if dev && std::env::var("RUST_BACKTRACE").is_err() {
        // Full tracebacks on panics during development.
        std::env::set_var("RUST_BACKTRACE", "1");
    }

    // Development: verbose, human-readable logs with file:line. Production:
    // compact info-level (override either with RUST_LOG).
    let default_filter = if dev {
        "rag_backend=trace,tower_http=debug,sqlx=debug"
    } else {
        "rag_backend=info,tower_http=info"
    };
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| default_filter.into());
    if dev {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .pretty()
            .with_file(true)
            .with_line_number(true)
            .init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }
    tracing::info!(environment = cfg.environment, "starting");
    let pool = db::init(&cfg.database_url).await?;

    let state = AppState {
        db: pool,
        http: http_client(),
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
