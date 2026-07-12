use std::time::Duration;

use axum::Router;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::SmartIpKeyExtractor;
use tower_governor::GovernorLayer;

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
        governed!(GovernorConfigBuilder::default())
    }
}
