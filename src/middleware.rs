//! Application-wide tower middleware, kept out of the router so routes
//! stay clean.
//!
//! NOTE: with `Router::layer`, the LAST layer added is the outermost, so
//! requests flow bottom-to-top through this chain. Request-id must be set
//! before it can be propagated, and headers must be marked sensitive
//! before TraceLayer logs them — hence those sit at the bottom.

use std::time::Duration;

use axum::http::header::AUTHORIZATION;
use axum::Router;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::sensitive_headers::SetSensitiveRequestHeadersLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::SmartIpKeyExtractor;
use tower_governor::GovernorLayer;

/// Generous: /documents and /chat wait on upstream LLM calls.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Documents are capped at 10 MB each in the handler; this is the
/// whole-request cap so a JSON batch of several documents still fits.
const MAX_BODY_BYTES: usize = 50 * 1024 * 1024;

pub fn apply<S: Clone + Send + Sync + 'static>(router: Router<S>) -> Router<S> {
    // Per-IP rate limit: bucket refills 5 req/s, bursts up to 50.
    // SmartIpKeyExtractor honors Forwarded/X-Forwarded-For behind a proxy,
    // falling back to the peer address.
    let governor = GovernorConfigBuilder::default()
        .key_extractor(SmartIpKeyExtractor)
        .per_second(5)
        .burst_size(50)
        .finish()
        .expect("valid governor config");
    let governor = std::sync::Arc::new(governor);
    // Evict idle per-IP buckets so the limiter's memory stays bounded.
    let gc = governor.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            gc.limiter().retain_recent();
        }
    });

    router
        // innermost (closest to the handler)
        .layer(CatchPanicLayer::new()) // panic in a handler -> 500, not a dropped connection
        .layer(GovernorLayer::new(governor))
        .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES))
        .layer(CompressionLayer::new())
        .layer(CorsLayer::permissive())
        .layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            REQUEST_TIMEOUT,
        ))
        .layer(TraceLayer::new_for_http())
        .layer(SetSensitiveRequestHeadersLayer::new([AUTHORIZATION])) // redact before tracing
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
    // outermost (first to see the request)
}
