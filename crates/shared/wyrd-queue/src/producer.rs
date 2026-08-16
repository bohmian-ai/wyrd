//! The client byte owner and one bounded background producer.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering};
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

/// Running producer state.
const RUNNING: u8 = 0;
/// Draining producer state.
const DRAINING: u8 = 1;
/// Fully drained producer state.
const DRAINED: u8 = 2;

/// One shared handle-wide byte and cardinality budget.
#[derive(Debug, Clone)]
pub struct ClientByteBudget {
    state: Arc<ClientBudgetState>,
}

/// Atomics that make every scalable client allocation and live entry visible.
#[derive(Debug)]
struct ClientBudgetState {
    limit: usize,
    used: AtomicUsize,
    live_batches: AtomicUsize,
    retry_entries: AtomicUsize,
}

impl ClientByteBudget {
    /// Creates the single budget used by all producers in one handle.
    #[must_use]
    pub fn new(limit: usize) -> Self {
        Self {
            state: Arc::new(ClientBudgetState {
                limit,
                used: AtomicUsize::new(0),
                live_batches: AtomicUsize::new(0),
                retry_entries: AtomicUsize::new(0),
            }),
        }
    }

    /// Reserves bytes before a row, builder, or frame allocation can grow.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdQueueError::Backpressure`] when the complete handle-wide
    /// live set would exceed its accepted byte ceiling.
    pub fn reserve(&self, bytes: usize) -> Result<ClientByteGuard, WyrdQueueError> {
        loop {
            let used = self.state.used.load(Ordering::Acquire);
            let Some(next) = used.checked_add(bytes) else {
                return Err(WyrdQueueError::Backpressure);
            };
            if next > self.state.limit {
                return Err(WyrdQueueError::Backpressure);
            }
            if self
                .state
                .used
                .compare_exchange(used, next, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return Ok(ClientByteGuard {
                    budget: self.clone(),
                    bytes,
                    batch_permit: None,
                });
            }
        }
    }

    /// Reserves bytes and one live-batch slot for a manually constructed batch.
    ///
    /// # Errors
    ///
    /// Returns typed backpressure before the batch can be constructed when its
    /// bytes or the 64-live-batch envelope cannot fit.
    pub fn reserve_sealed(&self, bytes: usize) -> Result<ClientByteGuard, WyrdQueueError> {
        self.reserve(bytes)?.attach_batch()
    }

    /// Reserves one globally bounded retry entry before retaining a batch.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdQueueError::Backpressure`] before retry-container growth
    /// when 64 retained retry/status entries are already live.
    pub(crate) fn reserve_retry(&self) -> Result<RetryPermit, WyrdQueueError> {
        self.reserve_cardinality(&self.state.retry_entries)
            .map(|()| RetryPermit {
                budget: self.clone(),
            })
    }

    /// Atomically reserves one globally bounded sealed-batch slot.
    fn reserve_batch(&self) -> Result<BatchPermit, WyrdQueueError> {
        self.reserve_cardinality(&self.state.live_batches)
            .map(|()| BatchPermit {
                budget: self.clone(),
            })
    }

    /// Reserves one cardinality slot without allocating a container entry first.
    fn reserve_cardinality(&self, counter: &AtomicUsize) -> Result<(), WyrdQueueError> {
        loop {
            let current = counter.load(Ordering::Acquire);
            if current >= QueueConfig::MAX_LIVE_ENTRIES {
                return Err(WyrdQueueError::Backpressure);
            }
            if counter
                .compare_exchange(current, current + 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return Ok(());
            }
        }
    }

    /// Returns currently owned client bytes for allocation assertions and telemetry.
    #[must_use]
    pub fn used_bytes(&self) -> usize {
        self.state.used.load(Ordering::Acquire)
    }
}

/// The exclusive reservation that follows bytes through every ownership state.
#[derive(Debug)]
pub struct ClientByteGuard {
    budget: ClientByteBudget,
    bytes: usize,
    batch_permit: Option<BatchPermit>,
}

impl ClientByteGuard {
    /// Shrinks an existing reservation after materialization reveals its exact size.
    #[must_use]
    pub(crate) fn resize(mut self, bytes: usize) -> Self {
        if bytes < self.bytes {
            self.budget
                .state
                .used
                .fetch_sub(self.bytes - bytes, Ordering::AcqRel);
            self.bytes = bytes;
        }
        self
    }

    /// Marks this existing byte owner as one of the 64 live sealed batches.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdQueueError::Backpressure`] before constructing a batch when
    /// the simultaneous live-batch envelope is full.
    pub(crate) fn attach_batch(mut self) -> Result<Self, WyrdQueueError> {
        self.batch_permit = Some(self.budget.reserve_batch()?);
        Ok(self)
    }
}

impl Drop for ClientByteGuard {
    fn drop(&mut self) {
        self.budget
            .state
            .used
            .fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

/// Releases one live sealed-batch slot with its owning batch.
#[derive(Debug)]
struct BatchPermit {
    budget: ClientByteBudget,
}

impl Drop for BatchPermit {
    fn drop(&mut self) {
        self.budget
            .state
            .live_batches
            .fetch_sub(1, Ordering::AcqRel);
    }
}

/// Releases one retry/status slot when a retained batch settles.
#[derive(Debug)]
pub(crate) struct RetryPermit {
    budget: ClientByteBudget,
}

impl Drop for RetryPermit {
    fn drop(&mut self) {
        self.budget
            .state
            .retry_entries
            .fetch_sub(1, Ordering::AcqRel);
    }
}

/// Shared accepted and dropped counts.
#[derive(Debug, Default)]
pub(crate) struct Counters {
    /// Rows accepted into stage one.
    pub(crate) accepted: AtomicU64,
    /// Rows refused or terminally discarded with visible accounting.
    pub(crate) dropped: AtomicU64,
}

/// Snapshot of producer occupancy and visible outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProducerMetrics {
    /// Accepted rows.
    pub accepted: u64,
    /// Refused or discarded rows.
    pub dropped: u64,
    /// Rows in the fixed stage-one and stage-two queues.
    pub queue_depth: usize,
}

/// One bounded producer control command.
enum Ctrl {
    /// Flushes buffered work and returns durable identities.
    Flush(oneshot::Sender<Result<Vec<[u8; 16]>, WyrdQueueError>>),
    /// Drains and stops the task.
    Shutdown(oneshot::Sender<Result<(), WyrdQueueError>>),
}

/// The caller-facing bounded producer for one table.
pub struct Producer {
    tx: mpsc::Sender<Row>,
    ctrl_tx: mpsc::Sender<Ctrl>,
    control_pending: Arc<AtomicBool>,
    staging: Arc<ArrayQueue<Row>>,
    channel_depth: Arc<AtomicUsize>,
    counters: Arc<Counters>,
    state: Arc<AtomicU8>,
    budget: ClientByteBudget,
}

impl Producer {
    /// Creates a standalone producer with its own accepted 32 MiB budget.
    #[must_use]
    pub fn new(
        table: String,
        schema: SchemaRef,
        sink: Arc<dyn BatchSink<ClientByteGuard>>,
        config: QueueConfig,
    ) -> Self {
        Self::with_budget(
            table,
            schema,
            sink,
            config,
            ClientByteBudget::new(config.client_byte_limit()),
        )
    }

    /// Creates a producer sharing the supplied handle-wide budget.
    #[must_use]
    pub fn with_budget(
        table: String,
        schema: SchemaRef,
        sink: Arc<dyn BatchSink<ClientByteGuard>>,
        config: QueueConfig,
        budget: ClientByteBudget,
    ) -> Self {
        let (tx, rx) = mpsc::channel(config.channel_capacity());
        let (ctrl_tx, ctrl_rx) = mpsc::channel(config.max_producers().max(1));
        let staging = Arc::new(ArrayQueue::new(config.staging_capacity()));
        let channel_depth = Arc::new(AtomicUsize::new(0));
        let counters = Arc::new(Counters::default());
        let state = Arc::new(AtomicU8::new(RUNNING));
        let control_pending = Arc::new(AtomicBool::new(false));
        let queue = RecordQueue::new(
            table,
            schema,
            Arc::clone(&staging),
            sink,
            config,
            budget.clone(),
            Arc::clone(&counters),
        );
        wyrd_runtime::runtime().spawn(
            Task {
                queue,
                rx,
                ctrl_rx,
                control_pending: Arc::clone(&control_pending),
                channel_depth: Arc::clone(&channel_depth),
                state: Arc::clone(&state),
                flush_max_rows: config.flush_max_rows(),
                flush_interval_ms: config.flush_interval_ms,
            }
            .run(),
        );
        Self {
            tx,
            ctrl_tx,
            control_pending,
            staging,
            channel_depth,
            counters,
            state,
            budget,
        }
    }

    /// Reserves row capacity before handing it to the bounded channel.
    ///
    /// # Errors
    ///
    /// Returns typed backpressure when the producer is draining, its row queue
    /// is full, or the one handle-wide client budget cannot cover the row.
    pub fn enqueue(
        &self,
        json: Vec<u8>,
        card_ref: CardRef,
        run_id: Option<RunId>,
    ) -> Result<(), WyrdQueueError> {
        if self.state.load(Ordering::Acquire) != RUNNING {
            self.counters.dropped.fetch_add(1, Ordering::AcqRel);
            return Err(WyrdQueueError::QueueFull);
        }
        let guard = match self.budget.reserve(json.capacity()) {
            Ok(guard) => guard,
            Err(error) => {
                self.counters.dropped.fetch_add(1, Ordering::AcqRel);
                return Err(error);
            }
        };
        let row = Row {
            json,
            card_ref,
            run_id,
            _guard: guard,
        };
        match self.tx.try_send(row) {
            Ok(()) => {
                self.counters.accepted.fetch_add(1, Ordering::AcqRel);
                self.channel_depth.fetch_add(1, Ordering::AcqRel);
                Ok(())
            }
            Err(TrySendError::Full(_)) | Err(TrySendError::Closed(_)) => {
                self.counters.dropped.fetch_add(1, Ordering::AcqRel);
                Err(WyrdQueueError::QueueFull)
            }
        }
    }

    /// Flushes all currently buffered work through its durable acknowledgement.
    ///
    /// # Errors
    ///
    /// Returns typed backpressure when another control command is outstanding,
    /// plus sealing and transport errors from the background owner.
    pub fn flush(&self) -> Result<Vec<[u8; 16]>, WyrdQueueError> {
        self.control(Ctrl::Flush)
    }

    /// Drains the producer and rejects later rows after terminal settlement.
    ///
    /// # Errors
    ///
    /// Returns typed backpressure when another control command is outstanding,
    /// or the first durable transport error encountered while draining.
    pub fn shutdown(&self) -> Result<(), WyrdQueueError> {
        if self.control_pending.swap(true, Ordering::AcqRel) {
            return Err(WyrdQueueError::Backpressure);
        }
        let (reply_tx, reply_rx) = oneshot::channel();
        if self.ctrl_tx.try_send(Ctrl::Shutdown(reply_tx)).is_err() {
            self.control_pending.store(false, Ordering::Release);
            return Err(WyrdQueueError::Backpressure);
        }
        reply_rx
            .blocking_recv()
            .unwrap_or(Err(WyrdQueueError::QueueFull))
    }

    /// Returns a lightweight occupancy snapshot.
    #[must_use]
    pub fn metrics(&self) -> ProducerMetrics {
        ProducerMetrics {
            accepted: self.counters.accepted.load(Ordering::Acquire),
            dropped: self.counters.dropped.load(Ordering::Acquire),
            queue_depth: self.channel_depth.load(Ordering::Acquire) + self.staging.len(),
        }
    }

    /// Sends a flush control command after reserving this producer's one slot.
    fn control(
        &self,
        make: impl FnOnce(oneshot::Sender<Result<Vec<[u8; 16]>, WyrdQueueError>>) -> Ctrl,
    ) -> Result<Vec<[u8; 16]>, WyrdQueueError> {
        if self.control_pending.swap(true, Ordering::AcqRel) {
            return Err(WyrdQueueError::Backpressure);
        }
        let (reply_tx, reply_rx) = oneshot::channel();
        if self.ctrl_tx.try_send(make(reply_tx)).is_err() {
            self.control_pending.store(false, Ordering::Release);
            return Err(WyrdQueueError::Backpressure);
        }
        reply_rx
            .blocking_recv()
            .unwrap_or(Err(WyrdQueueError::QueueFull))
    }
}

/// The owned background receiver that serializes staging and control transitions.
struct Task {
    queue: RecordQueue,
    rx: mpsc::Receiver<Row>,
    ctrl_rx: mpsc::Receiver<Ctrl>,
    control_pending: Arc<AtomicBool>,
    channel_depth: Arc<AtomicUsize>,
    state: Arc<AtomicU8>,
    flush_max_rows: usize,
    flush_interval_ms: u64,
}

impl Task {
    /// Runs the one receiver loop until shutdown or every sender is dropped.
    async fn run(mut self) {
        let mut ticker = make_ticker(self.flush_interval_ms);
        loop {
            tokio::select! {
                biased;
                control = self.ctrl_rx.recv() => match control {
                    Some(Ctrl::Flush(reply)) => {
                        self.drain_channel().await;
                        let result = self.queue.seal_and_send().await.map(|outcome| outcome.batch_ids);
                        self.control_pending.store(false, Ordering::Release);
                        let _ = reply.send(result);
                    }
                    Some(Ctrl::Shutdown(reply)) => {
                        self.state.store(DRAINING, Ordering::Release);
                        self.drain_channel().await;
                        let result = self.drain_to_completion().await;
                        self.state.store(DRAINED, Ordering::Release);
                        self.control_pending.store(false, Ordering::Release);
                        let _ = reply.send(result);
                        break;
                    }
                    None => {
                        self.state.store(DRAINING, Ordering::Release);
                        let _ = self.drain_to_completion().await;
                        self.state.store(DRAINED, Ordering::Release);
                        break;
                    }
                },
                row = self.rx.recv() => match row {
                    Some(row) => {
                        self.channel_depth.fetch_sub(1, Ordering::AcqRel);
                        self.queue.ingest(row).await;
                        if self.queue.staging_len() >= self.flush_max_rows {
                            let _ = self.queue.seal_and_send().await;
                        }
                    }
                    None => {
                        self.state.store(DRAINING, Ordering::Release);
                        let _ = self.drain_to_completion().await;
                        self.state.store(DRAINED, Ordering::Release);
                        break;
                    }
                },
                () = tick(&mut ticker) => {
                    if self.queue.staging_len() > 0 {
                        let _ = self.queue.seal_and_send().await;
                    }
                },
            }
        }
    }

    /// Moves every currently queued row into fixed staging before a control seal.
    async fn drain_channel(&mut self) {
        while let Ok(row) = self.rx.try_recv() {
            self.channel_depth.fetch_sub(1, Ordering::AcqRel);
            self.queue.ingest(row).await;
        }
    }

    /// Repeats seal attempts until no row or ambiguous batch owner remains.
    async fn drain_to_completion(&mut self) -> Result<(), WyrdQueueError> {
        self.drain_channel().await;
        while self.queue.has_pending() {
            self.queue.seal_and_send().await?;
        }
        Ok(())
    }
}

/// Creates the optional interval only after entering the shared Tokio runtime.
fn make_ticker(interval_ms: u64) -> Option<Interval> {
    if interval_ms == 0 {
        return None;
    }
    let mut interval = tokio::time::interval(Duration::from_millis(interval_ms));
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    Some(interval)
}

/// Waits for one interval tick or remains pending when interval flushing is disabled.
async fn tick(ticker: &mut Option<Interval>) {
    match ticker {
        Some(interval) => {
            interval.tick().await;
        }
        None => std::future::pending::<()>().await,
    }
}

#[cfg(test)]
mod tests {
    //! Bounded ownership tests for the ordinary Rust queue.

    use std::sync::Arc;

    use arrow_schema::{DataType, Field, Schema, SchemaRef};
    use wyrd_spec::reference::CardRef;

    use super::ClientByteBudget;
    use crate::{MockSink, Producer, QueueConfig, WyrdQueueError};

    /// Builds a one-column user schema for sealed-batch tests.
    fn schema() -> SchemaRef {
        Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]))
    }

    /// Builds the card correlation required by a queue row.
    fn card() -> CardRef {
        "prod/Service/queue@1.0.0".parse().expect("valid test card")
    }

    /// Refuses a byte reservation before a payload owner can grow beyond 32 MiB.
    #[test]
    fn bounded_client_bytes_refuse_before_allocation() {
        let budget = ClientByteBudget::new(8);
        assert!(matches!(
            budget.reserve(9),
            Err(WyrdQueueError::Backpressure)
        ));
        assert_eq!(budget.used_bytes(), 0);
    }

    /// Retries one exact sealed owner identity after an ambiguous sink result.
    #[test]
    fn bounded_retry_retains_the_same_batch_identity() {
        let sink = Arc::new(MockSink::new());
        sink.fail_next(1);
        let budget = ClientByteBudget::new(QueueConfig::MAX_CLIENT_BYTE_LIMIT);
        let producer = Producer::with_budget(
            "vala.bifrost.queue".to_owned(),
            schema(),
            sink.clone(),
            QueueConfig {
                flush_interval_ms: 0,
                ..QueueConfig::default()
            },
            budget.clone(),
        );
        producer
            .enqueue(br#"{"id": 1}"#.to_vec(), card(), None)
            .expect("row accepted");
        assert!(
            producer.flush().is_err(),
            "first transport result is ambiguous"
        );
        producer.flush().expect("retained batch retries");
        let attempts = sink.attempted();
        assert_eq!(attempts.len(), 2);
        assert_eq!(attempts[0], attempts[1]);
        assert_eq!(budget.used_bytes(), 0, "durable ACK settles the owner once");
    }
}
