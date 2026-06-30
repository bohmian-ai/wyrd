//! Configure mTLS and bearer-token secret references for a Wyrd transport.
//!
//! Run with:
//!     cargo run -p wyrd-rust-examples --bin transport_secrets

use wyrd_client::transport::{GrpcConfig, QueueConfig, TransportConfig};
use wyrd_spec::security::{SecretRef, TlsConfig};

fn main() -> anyhow::Result<()> {
    let grpc = GrpcConfig {
        endpoint: "https://wyrd-ingest.internal:50051".to_string(),
        timeout_ms: 30_000,
        tls: Some(TlsConfig {
            ca_cert: Some(SecretRef::File {
                path: "/var/run/secrets/wyrd/ca.pem".to_string(),
            }),
            client_cert: Some(SecretRef::File {
                path: "/var/run/secrets/wyrd/client.crt".to_string(),
            }),
            client_key: Some(SecretRef::File {
                path: "/var/run/secrets/wyrd/client.key".to_string(),
            }),
            server_name_override: Some("ingest.internal".to_string()),
            insecure_skip_verify: false,
        }),
        auth: Some(SecretRef::Vault {
            key: "secret/data/wyrd/api".to_string(),
        }),
        connect_retries: 3,
    };
    grpc.validate()?;

    let queue = QueueConfig {
        transport: TransportConfig::Grpc(grpc),
        ..QueueConfig::default()
    };
    queue.validate()?;

    let json = serde_json::to_string_pretty(&queue)?;
    let round_trip: QueueConfig = serde_json::from_str(&json)?;
    anyhow::ensure!(round_trip == queue, "queue config did not round-trip");
    println!("{json}");
    Ok(())
}
