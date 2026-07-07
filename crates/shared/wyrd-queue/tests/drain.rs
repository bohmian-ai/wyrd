//! Drain proof: `shutdown()` walks `Running → Draining → Drained` (flush then
//! reject), and a stalled sink past the deadline yields `WYRD_CLIENT_504_FLUSH_TIMEOUT`.

use std::sync::Arc;

use arrow_schema::{DataType, Field, Schema, SchemaRef};
use wyrd_queue::sink::{BatchSink, SealedBatch};
use wyrd_queue::{MockSink, Producer, QueueConfig};
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::CardRef;

/// A sink whose `send` never returns, to force the flush deadline to elapse.
#[derive(Default)]
struct StallSink;

#[async_trait::async_trait]
impl BatchSink for StallSink {
    async fn send(&self, _batch: SealedBatch) -> Result<u64, WyrdError> {
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

fn no_auto() -> QueueConfig {
    QueueConfig {
        flush_interval_ms: 0,
        flush_max_rows: 512,
        ..QueueConfig::default()
    }
}

#[test]
fn shutdown_drains_then_rejects_enqueue() {
    let sink = Arc::new(MockSink::new());
    let producer = Producer::new("ns.tbl".to_owned(), user_schema(), sink.clone(), no_auto());

    producer
        .enqueue(br#"{"id": 1}"#.to_vec(), card(), None)
        .expect("enqueue");
    producer
        .enqueue(br#"{"id": 2}"#.to_vec(), card(), None)
        .expect("enqueue");

    producer.shutdown().expect("shutdown drains");

    let received = sink.received();
    let rows: u64 = received.iter().map(|b| b.rows).sum();
    assert_eq!(rows, 2, "shutdown flushes buffered rows before stopping");

    let err = producer
        .enqueue(br#"{"id": 3}"#.to_vec(), card(), None)
        .unwrap_err();
    assert_eq!(
        err.code(),
        "WYRD_CLIENT_429_QUEUE_FULL",
        "draining rejects new rows"
    );
}

#[test]
fn stalled_sink_flush_times_out() {
    let sink = Arc::new(StallSink);
    let config = QueueConfig {
        flush_interval_ms: 0,
        flush_max_rows: 512,
        flush_timeout_ms: 50,
        ..QueueConfig::default()
    };
    let producer = Producer::new("ns.tbl".to_owned(), user_schema(), sink, config);

    producer
        .enqueue(br#"{"id": 1}"#.to_vec(), card(), None)
        .expect("enqueue");

    let err = producer.flush().unwrap_err();
    assert_eq!(err.code(), "WYRD_CLIENT_504_FLUSH_TIMEOUT");
}
