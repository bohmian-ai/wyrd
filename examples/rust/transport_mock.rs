//! Configure the in-memory mock transport for a Wyrd client.
//!
//! Run with:
//!     cargo run -p wyrd-rust-examples --bin transport_mock

use wyrd_client::transport::{MockConfig, TransportConfig};

fn main() -> anyhow::Result<()> {
    let mock = MockConfig {
        label: "demo".to_string(),
        fail_on_drain: None,
    };
    let transport = TransportConfig::Mock(mock);
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
