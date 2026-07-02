//! Producer proof: manual/size/timer flush, stable `batch_id`s, `fail_next`
//! re-buffer (no loss), metrics accounting, and one-batch mixed correlation.

use std::sync::Arc;
use std::time::{Duration, Instant};

use arrow::array::{Array, StringArray};
use arrow::ipc::reader::StreamReader;
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use wyrd_queue::sink::SealedBatch;
use wyrd_queue::{MockSink, Producer, QueueConfig};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::ids::RunId;

fn user_schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("name", DataType::Utf8, true),
    ]))
}

fn card(name: &str) -> CardRef {
    format!("prod/Service/{name}@1.0.0")
        .parse()
        .expect("valid card ref")
}

fn row(id: i64, name: &str) -> Vec<u8> {
    format!(r#"{{"id": {id}, "name": "{name}"}}"#).into_bytes()
}

fn wait_until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if predicate() {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("condition not met before deadline");
}

fn total_rows(batches: &[SealedBatch]) -> u64 {
    batches.iter().map(|b| b.rows).sum()
}

fn config_no_auto() -> QueueConfig {
    QueueConfig {
        flush_interval_ms: 0,
        flush_max_rows: 512,
        ..QueueConfig::default()
    }
}

#[test]
fn manual_flush_returns_stable_batch_ids() {
    let sink = Arc::new(MockSink::new());
    let producer = Producer::new("ns.tbl".to_owned(), user_schema(), sink.clone(), config_no_auto());

    for i in 0..3 {
        producer.enqueue(row(i, "x"), card("alpha"), None).expect("enqueue");
    }
    let ids = producer.flush().expect("flush");

    assert_eq!(ids.len(), 1, "one batch for three rows under the size cap");
    let received = sink.received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].rows, 3);
    assert_eq!(received[0].table, "ns.tbl");
    // The id flush() reports is exactly the id sealed onto the batch — the
    // producer never regenerates it (retry-stable by construction).
    assert_eq!(received[0].batch_id, ids[0]);
}

#[test]
fn size_trigger_flushes_without_manual_call() {
    let sink = Arc::new(MockSink::new());
    let config = QueueConfig {
        flush_interval_ms: 0,
        flush_max_rows: 2,
        ..QueueConfig::default()
    };
    let producer = Producer::new("ns.tbl".to_owned(), user_schema(), sink.clone(), config);

    producer.enqueue(row(1, "a"), card("alpha"), None).expect("enqueue");
    producer.enqueue(row(2, "b"), card("alpha"), None).expect("enqueue");

    wait_until(|| total_rows(&sink.received()) >= 2);
    assert_eq!(total_rows(&sink.received()), 2);
}

#[test]
fn timer_trigger_flushes() {
    let sink = Arc::new(MockSink::new());
    let config = QueueConfig {
        flush_interval_ms: 25,
        flush_max_rows: 512,
        ..QueueConfig::default()
    };
    let producer = Producer::new("ns.tbl".to_owned(), user_schema(), sink.clone(), config);

    producer.enqueue(row(1, "a"), card("alpha"), None).expect("enqueue");

    wait_until(|| total_rows(&sink.received()) >= 1);
    assert_eq!(total_rows(&sink.received()), 1);
}

#[test]
fn fail_next_rebuffers_with_no_loss() {
    let sink = Arc::new(MockSink::new());
    let producer = Producer::new("ns.tbl".to_owned(), user_schema(), sink.clone(), config_no_auto());
    sink.fail_next(1);

    producer.enqueue(row(1, "a"), card("alpha"), None).expect("enqueue");
    producer.enqueue(row(2, "b"), card("alpha"), None).expect("enqueue");

    // First flush hits the forced sink failure; rows are re-buffered, not lost.
    assert!(producer.flush().is_err(), "forced sink failure surfaces");
    assert!(sink.received().is_empty(), "a failed send records nothing");

    // Second flush drains the re-buffered rows successfully.
    producer.flush().expect("second flush");
    assert_eq!(total_rows(&sink.received()), 2, "no rows lost across re-buffer");
}

#[test]
fn metrics_account_every_enqueue() {
    let sink = Arc::new(MockSink::new());
    let producer = Producer::new("ns.tbl".to_owned(), user_schema(), sink.clone(), config_no_auto());

    for i in 0..5 {
        producer.enqueue(row(i, "x"), card("alpha"), None).expect("enqueue");
    }
    producer.flush().expect("flush");

    let metrics = producer.metrics();
    assert_eq!(metrics.accepted, 5);
    assert_eq!(metrics.dropped, 0);
    assert_eq!(metrics.queue_depth, 0, "drained after flush");
}

#[test]
fn mixed_card_and_run_flush_to_one_batch() {
    let sink = Arc::new(MockSink::new());
    let producer = Producer::new("ns.tbl".to_owned(), user_schema(), sink.clone(), config_no_auto());

    producer
        .enqueue(row(1, "a"), card("alpha"), Some(RunId::from_string("run-a".to_owned())))
        .expect("enqueue");
    producer
        .enqueue(row(2, "b"), card("beta"), Some(RunId::from_string("run-b".to_owned())))
        .expect("enqueue");
    producer
        .enqueue(row(3, "c"), card("gamma"), None)
        .expect("enqueue");

    producer.flush().expect("flush");

    let received = sink.received();
    assert_eq!(received.len(), 1, "mixed cards/runs are not partitioned");

    let mut reader = StreamReader::try_new(received[0].frames.as_slice(), None).expect("reader");
    let batch = reader.next().expect("one batch").expect("ok");
    assert_eq!(batch.num_rows(), 3);

    let card_col = batch
        .column_by_name("card_ref")
        .expect("card_ref column")
        .as_any()
        .downcast_ref::<StringArray>()
        .expect("utf8");
    assert_eq!(card_col.value(0), "prod/Service/alpha@1.0.0");
    assert_eq!(card_col.value(1), "prod/Service/beta@1.0.0");
    assert_eq!(card_col.value(2), "prod/Service/gamma@1.0.0");

    let run_col = batch
        .column_by_name("run_id")
        .expect("run_id column")
        .as_any()
        .downcast_ref::<StringArray>()
        .expect("utf8");
    assert_eq!(run_col.value(0), "run-a");
    assert_eq!(run_col.value(1), "run-b");
    assert!(run_col.is_null(2), "row with no run_id stays null");
}
