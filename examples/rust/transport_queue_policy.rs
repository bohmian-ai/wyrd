//! Tune queue policy for a Wyrd transport.
//!
//! Run with:
//!     cargo run -p wyrd-rust-examples --bin transport_queue_policy

use wyrd_client::transport::{MockConfig, QueueConfig, TransportConfig};

fn main() -> anyhow::Result<()> {
    let queue = QueueConfig {
        transport: TransportConfig::Mock(MockConfig {
            label: "queue-policy-demo".to_string(),
            fail_on_flush: None,
        }),
        flush_max_rows: 5_000,
        flush_interval_ms: 1_000,
        channel_capacity: 500,
        sample_ratio: Some(0.5),
    };
    queue.validate()?;

    let json = serde_json::to_string_pretty(&queue)?;
    let round_trip: QueueConfig = serde_json::from_str(&json)?;
    anyhow::ensure!(round_trip == queue, "queue config did not round-trip");
    println!("{json}");
    Ok(())
}
