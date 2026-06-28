//! Configure the default gRPC transport for a Wyrd client.
//!
//! Run with:
//!     cargo run -p wyrd-rust-examples --bin transport_grpc

use wyrd_client::transport::{GrpcConfig, TransportConfig};
use wyrd_spec::security::{SecretRef, TlsConfig};

fn main() -> anyhow::Result<()> {
    let grpc = GrpcConfig {
        endpoint: "https://wyrd-ingest.example.com:50051".to_string(),
        timeout_ms: 30_000,
        tls: Some(TlsConfig {
            ca_cert: Some(SecretRef::File {
                path: "/etc/ssl/ca.pem".to_string(),
            }),
            client_cert: None,
            client_key: None,
            server_name_override: None,
            insecure_skip_verify: false,
        }),
        connect_retries: 3,
        keepalive_interval_ms: 20_000,
        keepalive_timeout_ms: 5_000,
        max_message_bytes: 4 * 1024 * 1024,
    };
    grpc.validate()?;

    let transport = TransportConfig::Grpc(grpc);
    transport.validate()?;

    let json = serde_json::to_string_pretty(&transport)?;
    let round_trip: TransportConfig = serde_json::from_str(&json)?;
    anyhow::ensure!(
        round_trip == transport,
        "transport config did not round-trip"
    );
    println!("{json}");
    Ok(())
}
