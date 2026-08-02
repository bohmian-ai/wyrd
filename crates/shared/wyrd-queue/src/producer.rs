//! The two-stage producer and its background flush task.
//!
//! [`Producer`] is the caller-facing handle. `enqueue` is the hot path: it does
//! a bounded, non-blocking hand-off onto a stage-1 `tokio::mpsc` bounded channel
//! and returns immediately — it never touches the sink, the schema, or Arrow.
//! A single background task (spawned once on the process-global
//! [`wyrd_runtime::runtime`], never an ad-hoc runtime) owns the stage-2
//! `crossbeam_queue::ArrayQueue` staging buffer via a [`RecordQueue`], moves
//! rows channel → staging, and seals-and-sends on the size trigger, the interval
//! timer, an explicit `flush`, or drain-on-`shutdown`.
//!
//! Backpressure is explicit and never silent: a saturated channel returns
//! [`WyrdQueueError::QueueFull`] and bumps the drop counter. `shutdown` walks the
//! `Running → Draining → Drained` state machine, refusing new rows once draining
//! and flushing the buffer to completion before it returns.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use arrow_schema::SchemaRef;
use crossbeam_queue::ArrayQueue;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{Interval, MissedTickBehavior};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::ids::RunId;

use crate::config::QueueConfig;
use crate::error::WyrdQueueError;
use crate::queue::{RecordQueue, Row};
use crate::sink::BatchSink;

/// `Running`: accepting rows. The steady state.
const RUNNING: u8 = 0;
/// `Draining`: shutdown observed; new rows are rejected while the buffer flushes.
const DRAINING: u8 = 1;
/// `Drained`: the buffer is empty and the background task has stopped.
const DRAINED: u8 = 2;

/// Bounded enqueue retries before a full channel yields `QueueFull`.
const ENQUEUE_ATTEMPTS: usize = 3;
/// Per-attempt backoff on a full channel. Small and bounded — never unbounded
/// blocking; the caller gets a fast `429` if the buffer stays saturated.
const ENQUEUE_BACKOFF: Duration = Duration::from_millis(1);

/// Accepted/dropped tallies shared by the producer and the flush path.
#[derive(Debug, Default)]
pub(crate) struct Counters {
    /// Rows accepted onto the stage-1 channel.
    pub(crate) accepted: AtomicU64,
    /// Rows dropped — a full channel (`429`) or a staging overflow on re-buffer.
    pub(crate) dropped: AtomicU64,
}

/// A point-in-time snapshot of producer counters and depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProducerMetrics {
    /// Total rows accepted onto the channel since construction.
    pub accepted: u64,
    /// Total rows dropped (channel saturation or staging overflow) — never silent.
    pub dropped: u64,
    /// Rows in flight: stage-1 channel depth plus stage-2 staging depth.
    pub queue_depth: usize,
}

/// A control message from a [`Producer`] handle to its background task.
enum Ctrl {
    /// Seal-and-send everything currently buffered; reply with the sealed ids.
    Flush(oneshot::Sender<Result<Vec<[u8; 16]>, WyrdQueueError>>),
    /// Drain to completion and stop the task; reply once drained.
    Shutdown(oneshot::Sender<Result<(), WyrdQueueError>>),
}

/// The caller-facing write handle over the two-stage buffered producer.
///
/// Cloneable-by-`Arc` at the surface layer; here it is the single owner of the
/// stage-1 sender and the control channel. Dropping the last handle closes both
/// channels, which the background task observes and drains before it exits.
pub struct Producer {
    tx: mpsc::Sender<Row>,
    ctrl_tx: mpsc::UnboundedSender<Ctrl>,
    staging: Arc<ArrayQueue<Row>>,
    channel_depth: Arc<AtomicUsize>,
    counters: Arc<Counters>,
    state: Arc<AtomicU8>,
}

impl Producer {
    /// Build a producer for one destination table and spawn its background task.
    ///
    /// `schema` is the resolved **user** Arrow schema; the [`BatchBuilder`] adds
    /// the reserved `card_ref`/`run_id` correlation columns at seal. `sink` is the
    /// swap seam. The background task runs on the process-global
    /// [`wyrd_runtime::runtime`] — this constructor never creates a runtime.
    ///
    /// [`BatchBuilder`]: crate::batch_builder::BatchBuilder
    #[must_use]
    pub fn new(
        table: String,
        schema: SchemaRef,
        sink: Arc<dyn BatchSink>,
        config: QueueConfig,
    ) -> Self {
        let (tx, rx) = mpsc::channel(config.channel_capacity.max(1));
        let (ctrl_tx, ctrl_rx) = mpsc::unbounded_channel();
        let staging = Arc::new(ArrayQueue::new(config.staging_capacity.max(1)));
        let counters = Arc::new(Counters::default());
        let channel_depth = Arc::new(AtomicUsize::new(0));
        let state = Arc::new(AtomicU8::new(RUNNING));

        let queue = RecordQueue::new(
            table,
            schema,
            staging.clone(),
            sink,
            config,
            counters.clone(),
        );

        let task = Task {
            queue,
            rx,
            ctrl_rx,
            channel_depth: channel_depth.clone(),
            state: state.clone(),
            flush_max_rows: config.flush_max_rows.max(1),
            flush_interval_ms: config.flush_interval_ms,
        };
        wyrd_runtime::runtime().spawn(task.run());

        Self {
            tx,
            ctrl_tx,
            staging,
            channel_depth,
            counters,
            state,
        }
    }

    /// Hand one JSON row off to the buffer. The hot path: a bounded, non-blocking
    /// enqueue that never touches the sink or Arrow.
    ///
    /// `json` is one `model_dump_json()` payload; `card_ref` and `run_id` are the
    /// per-row correlation. Returns immediately on success.
    ///
    /// # Errors
    /// [`WyrdQueueError::QueueFull`] if the producer is draining, or if the
    /// stage-1 channel stays saturated across the bounded retry window. Every
    /// such rejection bumps the drop counter — a full queue is a visible `429`,
    /// never a silent drop.
    pub fn enqueue(
        &self,
        json: Vec<u8>,
        card_ref: CardRef,
        run_id: Option<RunId>,
    ) -> Result<(), WyrdQueueError> {
        if self.state.load(Ordering::SeqCst) != RUNNING {
            self.counters.dropped.fetch_add(1, Ordering::SeqCst);
            return Err(WyrdQueueError::QueueFull);
        }
        let mut row = Row {
            json,
            card_ref,
            run_id,
        };
        for attempt in 0..ENQUEUE_ATTEMPTS {
            match self.tx.try_send(row) {
                Ok(()) => {
                    self.counters.accepted.fetch_add(1, Ordering::SeqCst);
                    self.channel_depth.fetch_add(1, Ordering::SeqCst);
                    return Ok(());
                }
                Err(TrySendError::Full(returned)) => {
                    row = returned;
                    if attempt + 1 < ENQUEUE_ATTEMPTS {
                        std::thread::sleep(ENQUEUE_BACKOFF);
                    }
                }
                Err(TrySendError::Closed(_)) => break,
            }
        }
        self.counters.dropped.fetch_add(1, Ordering::SeqCst);
        Err(WyrdQueueError::QueueFull)
    }

    /// Seal-and-send everything currently buffered and block until it completes,
    /// returning the sealed `batch_id`s.
    ///
    /// # Errors
    /// The first seal/send error of the pass (see [`RecordQueue::seal_and_send`]),
    /// or [`WyrdQueueError::QueueFull`] if the background task is gone.
    pub fn flush(&self) -> Result<Vec<[u8; 16]>, WyrdQueueError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self.ctrl_tx.send(Ctrl::Flush(reply_tx)).is_err() {
            return Err(WyrdQueueError::QueueFull);
        }
        match reply_rx.blocking_recv() {
            Ok(result) => result,
            Err(_) => Err(WyrdQueueError::QueueFull),
        }
    }

    /// Transition `Running → Draining → Drained`: refuse new rows, flush the
    /// buffer to completion, and stop the background task. Blocks until drained.
    ///
    /// # Errors
    /// The terminal drain's seal/send error if the buffer could not be fully
    /// flushed (rows are re-buffered, not dropped), or [`WyrdQueueError::QueueFull`]
    /// if the task had already stopped.
    pub fn shutdown(&self) -> Result<(), WyrdQueueError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self.ctrl_tx.send(Ctrl::Shutdown(reply_tx)).is_err() {
            return Err(WyrdQueueError::QueueFull);
        }
        match reply_rx.blocking_recv() {
            Ok(result) => result,
            Err(_) => Err(WyrdQueueError::QueueFull),
        }
    }

    /// Snapshot the accepted/dropped counters and the live queue depth.
    #[must_use]
    pub fn metrics(&self) -> ProducerMetrics {
        ProducerMetrics {
            accepted: self.counters.accepted.load(Ordering::SeqCst),
            dropped: self.counters.dropped.load(Ordering::SeqCst),
            queue_depth: self.channel_depth.load(Ordering::SeqCst) + self.staging.len(),
        }
    }
}

/// The owned state of the single background flush task.
struct Task {
    queue: RecordQueue,
    rx: mpsc::Receiver<Row>,
    ctrl_rx: mpsc::UnboundedReceiver<Ctrl>,
    channel_depth: Arc<AtomicUsize>,
    state: Arc<AtomicU8>,
    flush_max_rows: usize,
    flush_interval_ms: u64,
}

impl Task {
    /// The task loop: control messages take priority (`biased`), then rows, then
    /// the interval timer. Exits on shutdown or once every sender is dropped.
    ///
    /// The interval is built here (inside the runtime), never in the constructor —
    /// `tokio::time::interval` requires an active timer driver.
    async fn run(mut self) {
        let mut ticker = make_ticker(self.flush_interval_ms);
        loop {
            tokio::select! {
                biased;

                ctrl = self.ctrl_rx.recv() => {
                    match ctrl {
                        Some(Ctrl::Flush(reply)) => {
                            self.drain_channel().await;
                            let out = self.queue.seal_and_send().await.map(|o| o.batch_ids);
                            let _ = reply.send(out);
                        }
                        Some(Ctrl::Shutdown(reply)) => {
                            self.state.store(DRAINING, Ordering::SeqCst);
                            self.drain_channel().await;
                            let out = self.drain_to_completion().await;
                            self.state.store(DRAINED, Ordering::SeqCst);
                            let _ = reply.send(out);
                            break;
                        }
                        None => {
                            self.drain_channel().await;
                            let _ = self.drain_to_completion().await;
                            break;
                        }
                    }
                }

                maybe_row = self.rx.recv() => {
                    match maybe_row {
                        Some(row) => {
                            self.channel_depth.fetch_sub(1, Ordering::SeqCst);
                            self.queue.ingest(row).await;
                            if self.queue.staging_len() >= self.flush_max_rows {
                                let _ = self.queue.seal_and_send().await;
                            }
                        }
                        None => {
                            let _ = self.drain_to_completion().await;
                            break;
                        }
                    }
                }

                () = tick(&mut ticker) => {
                    if self.queue.staging_len() > 0 {
                        let _ = self.queue.seal_and_send().await;
                    }
                }
            }
        }
    }

    /// Pull every row currently queued on the channel into staging. Called before
    /// a flush/shutdown seal so the pass captures all in-flight rows deterministically.
    async fn drain_channel(&mut self) {
        while let Ok(row) = self.rx.try_recv() {
            self.channel_depth.fetch_sub(1, Ordering::SeqCst);
            self.queue.ingest(row).await;
        }
    }

    /// Seal repeatedly until staging is empty. Stops and surfaces the error on the
    /// first failed pass (rows stay re-buffered, never dropped).
    async fn drain_to_completion(&self) -> Result<(), WyrdQueueError> {
        while self.queue.has_pending() {
            self.queue.seal_and_send().await?;
        }
        Ok(())
    }
}

/// Build the interval timer, or `None` when the time trigger is disabled (`0`).
fn make_ticker(interval_ms: u64) -> Option<Interval> {
    if interval_ms == 0 {
        return None;
    }
    let mut interval = tokio::time::interval(Duration::from_millis(interval_ms));
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    Some(interval)
}

/// Await the next tick, or park forever when the timer is disabled.
async fn tick(ticker: &mut Option<Interval>) {
    match ticker {
        Some(interval) => {
            interval.tick().await;
        }
        None => std::future::pending::<()>().await,
    }
}

#[cfg(test)]
mod producer_tests {
    //! Producer proof: manual/size/timer flush, stable `batch_id`s, `fail_next`
    //! re-buffer (no loss), metrics accounting, and one-batch mixed correlation.

    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use crate::sink::SealedBatch;
    use crate::{MockSink, Producer, QueueConfig};
    use arrow::array::{Array, StringArray};
    use arrow::ipc::reader::StreamReader;
    use arrow_schema::{DataType, Field, Schema, SchemaRef};
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
        let producer = Producer::new(
            "ns.tbl".to_owned(),
            user_schema(),
            sink.clone(),
            config_no_auto(),
        );

        for i in 0..3 {
            producer
                .enqueue(row(i, "x"), card("alpha"), None)
                .expect("enqueue");
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

        producer
            .enqueue(row(1, "a"), card("alpha"), None)
            .expect("enqueue");
        producer
            .enqueue(row(2, "b"), card("alpha"), None)
            .expect("enqueue");

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

        producer
            .enqueue(row(1, "a"), card("alpha"), None)
            .expect("enqueue");

        wait_until(|| total_rows(&sink.received()) >= 1);
        assert_eq!(total_rows(&sink.received()), 1);
    }

    #[test]
    fn fail_next_rebuffers_with_no_loss() {
        let sink = Arc::new(MockSink::new());
        let producer = Producer::new(
            "ns.tbl".to_owned(),
            user_schema(),
            sink.clone(),
            config_no_auto(),
        );
        sink.fail_next(1);

        producer
            .enqueue(row(1, "a"), card("alpha"), None)
            .expect("enqueue");
        producer
            .enqueue(row(2, "b"), card("alpha"), None)
            .expect("enqueue");

        // First flush hits the forced sink failure; rows are re-buffered, not lost.
        assert!(producer.flush().is_err(), "forced sink failure surfaces");
        assert!(sink.received().is_empty(), "a failed send records nothing");

        // Second flush drains the re-buffered rows successfully.
        producer.flush().expect("second flush");
        assert_eq!(
            total_rows(&sink.received()),
            2,
            "no rows lost across re-buffer"
        );
    }

    #[test]
    fn metrics_account_every_enqueue() {
        let sink = Arc::new(MockSink::new());
        let producer = Producer::new(
            "ns.tbl".to_owned(),
            user_schema(),
            sink.clone(),
            config_no_auto(),
        );

        for i in 0..5 {
            producer
                .enqueue(row(i, "x"), card("alpha"), None)
                .expect("enqueue");
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
        let producer = Producer::new(
            "ns.tbl".to_owned(),
            user_schema(),
            sink.clone(),
            config_no_auto(),
        );

        producer
            .enqueue(
                row(1, "a"),
                card("alpha"),
                Some(RunId::from_string("run-a".to_owned())),
            )
            .expect("enqueue");
        producer
            .enqueue(
                row(2, "b"),
                card("beta"),
                Some(RunId::from_string("run-b".to_owned())),
            )
            .expect("enqueue");
        producer
            .enqueue(row(3, "c"), card("gamma"), None)
            .expect("enqueue");

        producer.flush().expect("flush");

        let received = sink.received();
        assert_eq!(received.len(), 1, "mixed cards/runs are not partitioned");

        let mut reader =
            StreamReader::try_new(received[0].frames.as_slice(), None).expect("reader");
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
}

#[cfg(test)]
mod backpressure {
    //! Backpressure proof: a saturated channel yields `WYRD_CLIENT_429_QUEUE_FULL`
    //! with a drop-counter bump — every enqueue is accounted, none silently dropped.

    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    use crate::sink::{BatchSink, SealedBatch};
    use crate::{Producer, QueueConfig};
    use arrow_schema::{DataType, Field, Schema, SchemaRef};
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
}

#[cfg(test)]
mod drain {
    //! Drain proof: `shutdown()` walks `Running → Draining → Drained` (flush then
    //! reject), and a stalled sink past the deadline yields `WYRD_CLIENT_504_FLUSH_TIMEOUT`.

    use std::sync::Arc;

    use crate::sink::{BatchSink, SealedBatch};
    use crate::{MockSink, Producer, QueueConfig};
    use arrow_schema::{DataType, Field, Schema, SchemaRef};
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
}
