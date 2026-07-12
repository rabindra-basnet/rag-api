use std::time::Duration;

use axum::http::header::AUTHORIZATION;
use axum::http::{HeaderName, HeaderValue};
use std::str::FromStr;
use axum::middleware::from_fn_with_state;
use axum::routing::get;
use axum::Router;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, AllowOrigin, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::sensitive_headers::SetSensitiveRequestHeadersLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::middleware::auth::require_auth;
use crate::state::app_state::AppState;

use super::{auth, chat, document, file, user};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_BODY_BYTES: usize = 50 * 1024 * 1024;

fn build_cors(allow_origin: &str, expose_headers: &str) -> CorsLayer {
    let allow_any = allow_origin
        .split(',')
        .map(|s| s.trim())
        .any(|s| s == "*");

    let origins: Vec<HeaderValue> = allow_origin
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && *s != "*")
        .filter_map(|s| HeaderValue::from_str(s).ok())
        .collect();

    let cors = CorsLayer::new()
        .allow_origin(if allow_any || origins.is_empty() {
            AllowOrigin::any()
        } else {
            AllowOrigin::list(origins)
        })
        .allow_methods(Any)
        .allow_headers(Any);

    let headers: Vec<HeaderName> = expose_headers
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .filter_map(|s| HeaderName::from_str(s).ok())
        .collect();

    if headers.is_empty() {
        cors
    } else {
        cors.expose_headers(headers)
    }
}

async fn health() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }))
}

pub fn routes(state: AppState) -> Router {
    let trust_proxy = state.cfg.trust_proxy;

    let cors = build_cors(&state.cfg.cors_allow_origin, &state.cfg.cors_expose_headers);

    let auth_public = crate::middleware::rate_limit::rate_limit(auth::routes(), 1, 10, trust_proxy);

    let public = Router::new()
        .route("/health", get(health))
        .merge(auth_public);

    let protected = user::routes()
        .merge(document::routes())
        .merge(chat::routes())
        .merge(file::routes())
        .route_layer(from_fn_with_state(state.clone(), require_auth));
    let protected = crate::middleware::rate_limit::rate_limit(protected, 5, 50, trust_proxy);

    public
        .merge(protected)
        .with_state(state)
        .layer(CatchPanicLayer::new())
        .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES))
        .layer(CompressionLayer::new())
        .layer(cors)
        .layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            REQUEST_TIMEOUT,
        ))
        .layer(TraceLayer::new_for_http())
        .layer(SetSensitiveRequestHeadersLayer::new([AUTHORIZATION]))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
}
