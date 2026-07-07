//! Backpressure proof: a saturated channel yields `WYRD_CLIENT_429_QUEUE_FULL`
//! with a drop-counter bump — every enqueue is accounted, none silently dropped.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use arrow_schema::{DataType, Field, Schema, SchemaRef};
use wyrd_queue::sink::{BatchSink, SealedBatch};
use wyrd_queue::{Producer, QueueConfig};
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::CardRef;

/// A sink whose `send` never returns — it parks forever after signalling that it
/// has started, so the background task stalls and the channel cannot drain.
#[derive(Default)]
struct StallSink {
    started: AtomicBool,
}

#[async_trait::async_trait]
impl BatchSink for StallSink {
    async fn send(&self, _batch: SealedBatch) -> Result<u64, WyrdError> {
        self.started.store(true, Ordering::SeqCst);
        std::future::pending::<()>().await;
        unreachable!()
    }
}

fn user_schema() -> SchemaRef {
    Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]))
}

fn card() -> CardRef {
    "prod/Service/alpha@1.0.0".parse().expect("valid card ref")
}

#[test]
fn full_channel_returns_429_and_counts_the_drop() {
    let sink = Arc::new(StallSink::default());
    let config = QueueConfig {
        channel_capacity: 2,
        staging_capacity: 8,
        flush_max_rows: 1,
        flush_interval_ms: 0,
        flush_timeout_ms: 60_000,
        max_message_bytes: 4 * 1024 * 1024,
    };
    let producer = Producer::new("ns.tbl".to_owned(), user_schema(), sink.clone(), config);

    // First row is pulled, sealed, and handed to the stalling sink — parking the
    // background task so nothing further drains the channel.
    producer
        .enqueue(br#"{"id": 0}"#.to_vec(), card(), None)
        .expect("first enqueue accepted");
    let deadline = Instant::now() + Duration::from_secs(3);
    while !sink.started.load(Ordering::SeqCst) {
        assert!(Instant::now() < deadline, "sink never started");
        std::thread::sleep(Duration::from_millis(5));
    }

    // With the task parked, keep enqueueing until the bounded channel rejects.
    let attempts = 20;
    let mut rejected = 0;
    for i in 0..attempts {
        let payload = format!(r#"{{"id": {}}}"#, i + 1).into_bytes();
        if let Err(err) = producer.enqueue(payload, card(), None) {
            assert_eq!(err.code(), "WYRD_CLIENT_429_QUEUE_FULL");
            rejected += 1;
        }
    }

    assert!(rejected > 0, "a saturated channel must reject");

    let metrics = producer.metrics();
    assert_eq!(
        metrics.dropped, rejected,
        "every rejection bumps the drop counter"
    );
    assert_eq!(
        metrics.accepted + metrics.dropped,
        attempts as u64 + 1,
        "every enqueue is accounted — no silent drop"
    );
}
