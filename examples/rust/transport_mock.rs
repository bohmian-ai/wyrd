//! Configure a Wyrd queue with the in-memory mock transport.
//!
//! Run with:
//!     cargo run -p wyrd-rust-examples --bin transport_mock

use wyrd_client::transport::{MockConfig, QueueConfig, TransportConfig};

fn main() -> anyhow::Result<()> {
    let mock = MockConfig {
        label: "demo".to_string(),
        fail_on_flush: None,
    };
    let queue = QueueConfig {
        transport: TransportConfig::Mock(mock),
        flush_max_rows: 100,
        flush_interval_ms: 100,
        channel_capacity: 10,
        sample_ratio: None,
    };
    queue.validate()?;

    let json = serde_json::to_string_pretty(&queue)?;
    let round_trip: QueueConfig = serde_json::from_str(&json)?;
    anyhow::ensure!(round_trip == queue, "queue config did not round-trip");
    println!("{json}");
    Ok(())
}
