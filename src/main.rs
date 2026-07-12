use rag_backend::config;
use rag_backend::error;
use rag_backend::routes;
use rag_backend::state::app_state::AppState;
use tracing_subscriber::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    config::parameter::init();

    let dev = config::parameter::get().environment == "development";
    error::api_error::set_dev_mode(dev);
    if dev && std::env::var("RUST_BACKTRACE").is_err() {
        std::env::set_var("RUST_BACKTRACE", "1");
    }

    let default_filter = if dev {
        "rag_backend=trace,tower_http=debug,sqlx=debug"
    } else {
        "rag_backend=info,tower_http=info"
    };
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| default_filter.into());

    let logs_dir = std::path::Path::new("logs");
    if !logs_dir.exists() {
        std::fs::create_dir_all(logs_dir).ok();
    }
    let file_appender = tracing_appender::rolling::daily("logs", "app.log");
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(file_appender)
        .with_ansi(false)
        .with_target(true)
        .with_filter(filter.clone());

    let console_layer = tracing_subscriber::fmt::layer()
        .with_file(dev)
        .with_line_number(dev)
        .with_filter(filter);

    tracing_subscriber::registry()
        .with(file_layer)
        .with(console_layer)
        .init();

    let cfg = config::parameter::get();
    tracing::info!(environment = cfg.environment, "starting");

    let pool = config::database::init(&cfg.database_url).await?;

    let state = AppState {
        db: pool,
        cfg: cfg.clone(),
    };

    let bind_addr = cfg.bind_addr.clone();
    let app = routes::root::routes(state);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("listening on {bind_addr}");
    axum::serve(
        listener,
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
