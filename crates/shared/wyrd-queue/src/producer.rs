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
        while self.queue.staging_len() > 0 {
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
