//! Point a Wyrd client's gRPC plane (Bifrost ingest) at an explicit endpoint.
//!
//! The endpoint settings are a field of the `ClientConfig` that
//! `WyrdClient::with_config` takes; an omitted endpoint derives from the
//! HTTP server URL. Building the config makes no network call.
//!
//! Run with:
//!     cargo run -p wyrd-rust-examples --bin transport_grpc

use wyrd_sdk::config::ClientConfig;
use wyrd_sdk::transport::GrpcConfig;

fn main() -> anyhow::Result<()> {
    let grpc = GrpcConfig {
        endpoint: "https://wyrd-ingest.example.com:50051".to_string(),
        timeout_ms: 30_000,
        connect_retries: 3,
        keepalive_interval_ms: 20_000,
        keepalive_timeout_ms: 5_000,
        max_message_bytes: 4 * 1024 * 1024,
    };
    grpc.validate()?;

    // Hand this to `WyrdClient::with_config` once a credential is set.
    let config = ClientConfig {
        grpc,
        ..ClientConfig::default()
    };
    println!("gRPC endpoint: {}", config.grpc.endpoint);
    Ok(())
}
