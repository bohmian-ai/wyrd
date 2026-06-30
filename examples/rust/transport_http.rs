//! Configure a Wyrd queue with the HTTP fallback transport.
//!
//! Run with:
//!     cargo run -p wyrd-rust-examples --bin transport_http

use wyrd_client::transport::{HttpConfig, QueueConfig, TransportConfig};
use wyrd_spec::security::{SecretRef, TlsConfig};

fn main() -> anyhow::Result<()> {
    let http = HttpConfig {
        base_url: "https://wyrd-ingest.example.com".to_string(),
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
        auth: Some(SecretRef::Env {
            name: "WYRD_API_KEY".to_string(),
        }),
        compression: true,
    };
    http.validate()?;

    let queue = QueueConfig {
        transport: TransportConfig::Http(http),
        ..QueueConfig::default()
    };
    queue.validate()?;

    let json = serde_json::to_string_pretty(&queue)?;
    let round_trip: QueueConfig = serde_json::from_str(&json)?;
    anyhow::ensure!(round_trip == queue, "queue config did not round-trip");
    println!("{json}");
    Ok(())
}
