use std::time::Duration;

use skald_providers::{HttpTransport, TransportConfig};

#[test]
fn transport_defaults_match_plan() {
    let config = TransportConfig::default();

    assert_eq!(config.timeout, Duration::from_secs(30));
    assert_eq!(config.connect_timeout, Duration::from_secs(5));
    assert_eq!(config.pool_max_idle_per_host, 8);
    assert!(config.user_agent.contains("skald-providers"));
}

#[test]
fn transport_builds_reqwest_client() {
    let transport = HttpTransport::new(TransportConfig::default()).expect("transport builds");

    assert_eq!(transport.config().pool_max_idle_per_host, 8);
}
