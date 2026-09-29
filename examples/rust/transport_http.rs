//! Configure the HTTP fallback transport for a Wyrd client.
//!
//! Run with:
//!     cargo run -p wyrd-rust-examples --bin transport_http

use wyrd_sdk::transport::{HttpConfig, TransportConfig};

fn main() -> anyhow::Result<()> {
    let http = HttpConfig {
        base_url: "https://wyrd-ingest.example.com".to_string(),
        timeout_ms: 30_000,
        compression: true,
    };
    http.validate()?;

    let transport = TransportConfig::Http(http);
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
