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
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::SmartIpKeyExtractor;
use tower_governor::GovernorLayer;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::sensitive_headers::SetSensitiveRequestHeadersLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

/// Generous: /documents and /chat wait on upstream LLM calls.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Documents are capped at 10 MB each in the handler; this is the
/// whole-request cap so a JSON batch of several documents still fits.
const MAX_BODY_BYTES: usize = 50 * 1024 * 1024;

/// Per-IP rate limit for a sub-router. `trust_proxy` selects the key
/// source: behind nginx (which overwrites X-Forwarded-For) use the proxy
/// headers; exposed directly, use the peer address so the header can't be
/// spoofed to dodge the limit.
pub fn rate_limit<S: Clone + Send + Sync + 'static>(
    router: Router<S>,
    per_second: u64,
    burst: u32,
    trust_proxy: bool,
) -> Router<S> {
    macro_rules! governed {
        ($builder:expr) => {{
            let config = std::sync::Arc::new(
                $builder
                    .per_second(per_second)
                    .burst_size(burst)
                    .finish()
                    .expect("valid governor config"),
            );
            // Evict idle per-IP buckets so limiter memory stays bounded.
            let gc = config.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    gc.limiter().retain_recent();
                }
            });
            router.layer(GovernorLayer::new(config))
        }};
    }
    if trust_proxy {
        governed!(GovernorConfigBuilder::default().key_extractor(SmartIpKeyExtractor))
    } else {
        governed!(GovernorConfigBuilder::default()) // PeerIpKeyExtractor
    }
}

pub fn apply<S: Clone + Send + Sync + 'static>(router: Router<S>) -> Router<S> {
    router
        // innermost (closest to the handler)
        .layer(CatchPanicLayer::new()) // panic in a handler -> 500, not a dropped connection
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
