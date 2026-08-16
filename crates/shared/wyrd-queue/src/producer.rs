//! The client byte owner and one bounded background producer.

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
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
/// One pending control is allowed for each producer's dedicated command channel.
const CONTROL_SLOTS_PER_PRODUCER: usize = 1;

/// Charged queue slots selected before one producer allocates its fixed buffers.
#[derive(Debug, Clone, Copy)]
struct ProducerCapacities {
    /// Tokio data-channel slots retaining [`Row`] values.
    data_slots: usize,
    /// Fixed crossbeam staging slots retaining [`Row`] values.
    staging_slots: usize,
    /// Tokio control-channel slots retaining [`Ctrl`] values.
    control_slots: usize,
    /// Repository-owned slot storage charged for the producer lifetime.
    charged_bytes: usize,
}

impl ProducerCapacities {
    /// Derives a per-producer fixed queue allocation from configured cardinality.
    ///
    /// Half of each producer partition is charged to concrete
    /// repository-owned [`Row`] and [`Ctrl`] slots, leaving the other half for
    /// dynamic row, sealed-batch, and retry ownership. Tokio node headers,
    /// `Arc` control blocks, allocator slack, TLS, and authentication state
    /// remain cardinality-bounded T7R residual rather than guessed layout
    /// bytes.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdQueueError::Backpressure`] when the configured producer
    /// ceiling is zero, the handle cannot reserve at least one data and one
    /// staging slot per accepted producer, or a checked capacity calculation
    /// overflows.
    fn for_handle(config: QueueConfig, budget: &ClientByteBudget) -> Result<Self, WyrdQueueError> {
        let producer_count = config.max_producers();
        if producer_count == 0 {
            return Err(WyrdQueueError::Backpressure);
        }
        let per_producer_bytes = budget.limit() / producer_count;
        // Fixed slots cannot consume the entire partition: every accepted
        // producer must still be able to reserve dynamic row ownership. Half
        // of each partition remains available to rows, sealed batches, and
        // retry retention under the same handle owner.
        let fixed_storage_bytes = per_producer_bytes / 2;
        let control_bytes = CONTROL_SLOTS_PER_PRODUCER
            .checked_mul(std::mem::size_of::<Ctrl>())
            .ok_or(WyrdQueueError::Backpressure)?;
        let row_slot_bytes = std::mem::size_of::<Row>();
        let available_row_bytes = fixed_storage_bytes
            .checked_sub(control_bytes)
            .ok_or(WyrdQueueError::Backpressure)?;
        let row_slots = available_row_bytes / row_slot_bytes;
        if row_slots < 2 {
            return Err(WyrdQueueError::Backpressure);
        }
        let data_slots = config.channel_capacity().min(row_slots - 1);
        let staging_slots = config
            .staging_capacity()
            .min(row_slots.saturating_sub(data_slots));
        if data_slots == 0 || staging_slots == 0 {
            return Err(WyrdQueueError::Backpressure);
        }
        let charged_bytes = data_slots
            .checked_add(staging_slots)
            .and_then(|slots| slots.checked_mul(row_slot_bytes))
            .and_then(|rows| rows.checked_add(control_bytes))
            .ok_or(WyrdQueueError::Backpressure)?;
        Ok(Self {
            data_slots,
            staging_slots,
            control_slots: CONTROL_SLOTS_PER_PRODUCER,
            charged_bytes,
        })
    }
}

/// One shared handle-wide byte and cardinality budget.
#[derive(Debug, Clone)]
pub struct ClientByteBudget {
    state: Arc<ClientBudgetState>,
}

/// Shared allocation and cardinality state for one client handle.
#[derive(Debug)]
struct ClientBudgetState {
    /// Immutable byte ceiling covering fixed and dynamic client ownership.
    limit: usize,
    /// Bytes currently reserved under the handle-wide owner.
    used: AtomicUsize,
    /// Sealed batches awaiting terminal settlement.
    live_batches: AtomicUsize,
    /// Ambiguous batches retained for later retry.
    retry_entries: AtomicUsize,
    /// Fixed data, staging, and control slot charges for live producers.
    fixed_storage: AtomicUsize,
    /// Shared producer admission state guarded as one cardinality transition.
    producer_permits: Mutex<ProducerPermitState>,
}

/// The accepted producer count and the tightest configuration ceiling seen by a handle.
#[derive(Debug)]
struct ProducerPermitState {
    /// Producers admitted before fixed storage or queue construction.
    live: usize,
    /// Never-increasing producer ceiling shared by every direct caller.
    ceiling: usize,
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
                fixed_storage: AtomicUsize::new(0),
                producer_permits: Mutex::new(ProducerPermitState {
                    live: 0,
                    ceiling: QueueConfig::MAX_LIVE_ENTRIES,
                }),
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

    /// Returns one point-in-time view of the handle-wide ownership budget.
    ///
    /// The byte and batch values are independent atomic observations intended
    /// for telemetry and settlement assertions, rather than a transactional
    /// snapshot. Producer cardinality is read under its narrow admission lock.
    ///
    /// # Panics
    ///
    /// Panics if producer admission state is poisoned, indicating an
    /// invariant-breaking panic during producer construction or release.
    #[must_use]
    pub fn metrics(&self) -> ClientByteMetrics {
        let total_reserved_bytes = self.used_bytes();
        let fixed_storage_bytes = self.state.fixed_storage.load(Ordering::Acquire);
        let live_producers = self
            .state
            .producer_permits
            .lock()
            .expect("producer permit lock poisoned")
            .live;
        ClientByteMetrics {
            owned_bytes: total_reserved_bytes.saturating_sub(fixed_storage_bytes),
            fixed_storage_bytes,
            total_reserved_bytes,
            live_producers,
            live_batches: self.state.live_batches.load(Ordering::Acquire),
            retry_entries: self.state.retry_entries.load(Ordering::Acquire),
        }
    }

    /// Returns the immutable byte limit used to partition fixed producer storage.
    #[must_use]
    fn limit(&self) -> usize {
        self.state.limit
    }

    /// Reserves fixed producer queue storage before its buffers are constructed.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdQueueError::Backpressure`] before allocation when the
    /// fixed producer slots cannot fit the one handle-wide budget.
    fn reserve_fixed_storage(&self, bytes: usize) -> Result<FixedStorageGuard, WyrdQueueError> {
        let guard = self.reserve(bytes)?;
        self.state.fixed_storage.fetch_add(bytes, Ordering::AcqRel);
        Ok(FixedStorageGuard {
            guard,
            budget: self.clone(),
        })
    }

    /// Constructs one producer under a shared provisional admission slot.
    ///
    /// The narrow admission lock spans the construction closure. It prevents a
    /// concurrent direct caller from crossing a lower configured ceiling while
    /// that producer is still reserving fixed storage. The tighter ceiling is
    /// committed only after construction succeeds; an error restores the live
    /// count and leaves the prior ceiling unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdQueueError::Backpressure`] before construction when the
    /// configured ceiling is zero or the shared configured ceiling is occupied,
    /// and otherwise propagates a closure failure after rolling its provisional
    /// admission back.
    ///
    /// # Panics
    ///
    /// Panics if producer admission state is poisoned, indicating an
    /// invariant-breaking panic in another producer lifecycle operation.
    fn construct_producer<T>(
        &self,
        configured_ceiling: usize,
        construct: impl FnOnce() -> Result<T, WyrdQueueError>,
    ) -> Result<(T, ProducerPermit), WyrdQueueError> {
        if configured_ceiling == 0 {
            return Err(WyrdQueueError::Backpressure);
        }
        let mut state = self
            .state
            .producer_permits
            .lock()
            .expect("producer permit lock poisoned");
        let provisional_ceiling = state
            .ceiling
            .min(configured_ceiling.clamp(1, QueueConfig::MAX_LIVE_ENTRIES));
        if state.live >= provisional_ceiling {
            return Err(WyrdQueueError::Backpressure);
        }
        state.live += 1;
        match construct() {
            Ok(value) => {
                state.ceiling = provisional_ceiling;
                Ok((
                    value,
                    ProducerPermit {
                        budget: self.clone(),
                    },
                ))
            }
            Err(error) => {
                state.live = state
                    .live
                    .checked_sub(1)
                    .expect("provisional producer admission must be live");
                Err(error)
            }
        }
    }
}

/// Point-in-time accounting for the bounded ownership held by one client handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientByteMetrics {
    /// Dynamic bytes currently reserved by queued rows or a sealed batch owner.
    pub owned_bytes: usize,
    /// Lifetime charges for constructed producer data, staging, and control slots.
    pub fixed_storage_bytes: usize,
    /// Total handle reservation: dynamic owned bytes plus fixed storage charges.
    pub total_reserved_bytes: usize,
    /// Producers admitted by the shared handle before fixed queue construction.
    pub live_producers: usize,
    /// Sealed batch owners that have not reached terminal settlement.
    pub live_batches: usize,
    /// Retained ambiguous batches awaiting a later retry attempt.
    pub retry_entries: usize,
}

/// Lifetime reservation for fixed producer queue storage.
#[derive(Debug)]
struct FixedStorageGuard {
    /// The one byte-budget guard covering fixed slot storage.
    guard: ClientByteGuard,
    /// Shared budget counter reduced with the fixed guard.
    budget: ClientByteBudget,
}

impl Drop for FixedStorageGuard {
    /// Releases the fixed-slot accounting before its byte guard returns capacity.
    fn drop(&mut self) {
        self.budget
            .state
            .fixed_storage
            .fetch_sub(self.guard.bytes, Ordering::AcqRel);
    }
}

/// Lifetime admission guard for one producer under a shared client budget.
#[derive(Debug)]
struct ProducerPermit {
    /// Shared admission state decremented when terminal settlement or drop ends the lifetime.
    budget: ClientByteBudget,
}

impl Drop for ProducerPermit {
    /// Returns one producer slot exactly once.
    ///
    /// # Panics
    ///
    /// Panics if producer admission state is poisoned or underflows, which
    /// would indicate a producer lifecycle accounting invariant violation.
    fn drop(&mut self) {
        let mut state = self
            .budget
            .state
            .producer_permits
            .lock()
            .expect("producer permit lock poisoned");
        state.live = state
            .live
            .checked_sub(1)
            .expect("live producer permit must be held before release");
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
    /// One shared producer admission slot until terminal drain or producer drop.
    producer_permit: Mutex<Option<ProducerPermit>>,
    fixed_storage: Mutex<Option<FixedStorageGuard>>,
}

impl Producer {
    /// Creates a standalone producer with its own accepted 32 MiB budget.
    ///
    /// The fixed data, staging, and control slots are charged before their
    /// queues are constructed, then remain charged until terminal shutdown or
    /// producer drop.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdQueueError::Backpressure`] before allocating fixed queue
    /// storage when the accepted envelope cannot fit the client budget.
    pub fn new(
        table: &str,
        schema: SchemaRef,
        sink: Arc<dyn BatchSink<ClientByteGuard>>,
        config: QueueConfig,
    ) -> Result<Self, WyrdQueueError> {
        Self::with_budget(
            table,
            schema,
            sink,
            config,
            ClientByteBudget::new(config.client_byte_limit()),
        )
    }

    /// Creates a producer sharing the supplied handle-wide budget.
    ///
    /// The fixed slot partition is derived from the configured producer
    /// cardinality before constructing Tokio or crossbeam buffers. This keeps
    /// every accepted producer's material slot capacity inside one handle-wide
    /// byte owner. A shared producer permit also prevents direct callers from
    /// constructing more than the accepted configured producer cardinality.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdQueueError::Backpressure`] before queue construction when
    /// the configured producer ceiling is zero, the shared producer ceiling is
    /// occupied, or the fixed storage charge cannot fit the shared client
    /// budget.
    pub fn with_budget(
        table: &str,
        schema: SchemaRef,
        sink: Arc<dyn BatchSink<ClientByteGuard>>,
        config: QueueConfig,
        budget: ClientByteBudget,
    ) -> Result<Self, WyrdQueueError> {
        let capacities = ProducerCapacities::for_handle(config, &budget)?;
        let (mut producer, producer_permit) =
            budget.construct_producer(config.max_producers(), || {
                let fixed_storage = budget.reserve_fixed_storage(capacities.charged_bytes)?;
                let (tx, rx) = mpsc::channel(capacities.data_slots);
                let (ctrl_tx, ctrl_rx) = mpsc::channel(capacities.control_slots);
                let staging = Arc::new(ArrayQueue::new(capacities.staging_slots));
                let channel_depth = Arc::new(AtomicUsize::new(0));
                let counters = Arc::new(Counters::default());
                let state = Arc::new(AtomicU8::new(RUNNING));
                let control_pending = Arc::new(AtomicBool::new(false));
                let queue = RecordQueue::new(
                    table.to_owned(),
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
                Ok(Self {
                    tx,
                    ctrl_tx,
                    control_pending,
                    staging,
                    channel_depth,
                    counters,
                    state,
                    budget: budget.clone(),
                    producer_permit: Mutex::new(None),
                    fixed_storage: Mutex::new(Some(fixed_storage)),
                })
            })?;
        *producer
            .producer_permit
            .get_mut()
            .expect("new producer permit mutex is not poisoned") = Some(producer_permit);
        Ok(producer)
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
        let result = reply_rx
            .blocking_recv()
            .unwrap_or(Err(WyrdQueueError::QueueFull));
        if self.state.load(Ordering::Acquire) == DRAINED {
            self.release_fixed_storage();
            self.release_producer_permit();
        }
        result
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

    /// Reports whether this producer currently owns its one bounded control slot.
    ///
    /// A `true` result means a flush or shutdown command is awaiting completion;
    /// it does not imply that a batch has been accepted by the sink.
    #[must_use]
    pub fn has_pending_control(&self) -> bool {
        self.control_pending.load(Ordering::Acquire)
    }

    /// Returns whether the background owner has terminally drained all retained work.
    #[must_use]
    pub fn is_drained(&self) -> bool {
        self.state.load(Ordering::Acquire) == DRAINED
    }

    /// Releases the lifetime charge after the producer task has terminally drained.
    ///
    /// # Panics
    ///
    /// Panics if the producer fixed-storage mutex is poisoned, indicating an
    /// invariant-breaking panic in another producer operation.
    fn release_fixed_storage(&self) {
        drop(
            self.fixed_storage
                .lock()
                .expect("producer fixed-storage lock poisoned")
                .take(),
        );
    }

    /// Releases the shared producer slot after terminal drain completes.
    ///
    /// # Panics
    ///
    /// Panics if the producer permit mutex is poisoned, indicating an
    /// invariant-breaking panic in another producer lifecycle operation.
    fn release_producer_permit(&self) {
        drop(
            self.producer_permit
                .lock()
                .expect("producer permit lock poisoned")
                .take(),
        );
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
                        if result.is_ok() {
                            self.state.store(DRAINED, Ordering::Release);
                        }
                        self.control_pending.store(false, Ordering::Release);
                        let should_stop = result.is_ok();
                        let _ = reply.send(result);
                        if should_stop {
                            break;
                        }
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

    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use arrow_schema::{DataType, Field, Schema, SchemaRef};
    use async_trait::async_trait;
    use tokio::sync::Notify;
    use wyrd_spec::reference::CardRef;

    use super::ClientByteBudget;
    use crate::{
        BatchSink, ClientByteGuard, DurableBatchAck, MockSink, Producer, QueueConfig, SealedBatch,
        SinkError, WyrdQueueError,
    };

    /// Builds a one-column user schema for sealed-batch tests.
    fn schema() -> SchemaRef {
        Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]))
    }

    /// Builds the card correlation required by a queue row.
    fn card() -> CardRef {
        "prod/Service/queue@1.0.0".parse().expect("valid test card")
    }

    /// Cancellable sink that acknowledges only after the test releases it.
    #[derive(Default)]
    struct TimeoutSink {
        /// Identities observed before each borrowed send attempt.
        attempts: Mutex<Vec<[u8; 16]>>,
        /// Whether later attempts may complete with a durable acknowledgement.
        released: AtomicBool,
        /// Wakes a retained attempt after the test chooses to resolve it.
        wake: Notify,
        /// Borrowed send futures cancelled by the queue deadline.
        cancelled: AtomicUsize,
    }

    /// Records whether one borrowed sink attempt reached an acknowledgement.
    struct BorrowedAttempt<'a> {
        /// Counter updated only when cancellation drops the borrowed future.
        cancelled: &'a AtomicUsize,
        /// Set after the sink has produced a durable acknowledgement.
        acknowledged: bool,
    }

    impl Drop for BorrowedAttempt<'_> {
        /// Counts deadline cancellation without touching the queue-owned batch.
        fn drop(&mut self) {
            if !self.acknowledged {
                self.cancelled.fetch_add(1, Ordering::AcqRel);
            }
        }
    }

    impl TimeoutSink {
        /// Releases all future borrowed attempts for durable acknowledgement.
        fn release(&self) {
            self.released.store(true, Ordering::Release);
            self.wake.notify_waiters();
        }
    }

    #[async_trait]
    impl BatchSink<ClientByteGuard> for TimeoutSink {
        /// Waits for release without ever taking ownership from the queue.
        ///
        /// # Errors
        ///
        /// This test sink does not return an error; a queue deadline cancels
        /// its borrowed future and preserves the batch outside this method.
        ///
        /// # Panics
        ///
        /// Panics if the test-only attempt recorder mutex is poisoned.
        async fn send(
            &self,
            batch: &SealedBatch<ClientByteGuard>,
        ) -> Result<DurableBatchAck, SinkError> {
            let mut attempt = BorrowedAttempt {
                cancelled: &self.cancelled,
                acknowledged: false,
            };
            self.attempts
                .lock()
                .expect("timeout sink attempt lock poisoned")
                .push(batch.batch_id);
            while !self.released.load(Ordering::Acquire) {
                let notified = self.wake.notified();
                if self.released.load(Ordering::Acquire) {
                    break;
                }
                notified.await;
            }
            attempt.acknowledged = true;
            Ok(DurableBatchAck {
                batch_id: batch.batch_id,
                rows: batch.rows,
            })
        }
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

    /// Applies the shared producer ceiling before direct callers construct queue storage.
    #[test]
    fn shared_budget_refuses_sixty_fifth_producer_before_construction() {
        let sink = Arc::new(MockSink::new());
        let budget = ClientByteBudget::new(QueueConfig::MAX_CLIENT_BYTE_LIMIT);
        let config = QueueConfig {
            flush_interval_ms: 0,
            ..QueueConfig::default()
        };
        let mut producers = Vec::with_capacity(QueueConfig::MAX_LIVE_ENTRIES);
        let first_table = "vala.bifrost.direct_capacity_0";
        producers.push(
            Producer::with_budget(first_table, schema(), sink.clone(), config, budget.clone())
                .expect("first direct producer fits the shared envelope"),
        );
        assert!(matches!(
            Producer::with_budget(
                "vala.bifrost.direct_capacity_lower_failed",
                schema(),
                sink.clone(),
                QueueConfig {
                    max_producers: 1,
                    flush_interval_ms: 0,
                    ..QueueConfig::default()
                },
                budget.clone(),
            ),
            Err(WyrdQueueError::Backpressure)
        ));
        for index in 1..QueueConfig::MAX_LIVE_ENTRIES {
            let table = format!("vala.bifrost.direct_capacity_{index}");
            producers.push(
                Producer::with_budget(&table, schema(), sink.clone(), config, budget.clone())
                    .expect("accepted direct producer fits the shared envelope"),
            );
        }
        let admitted = budget.metrics();
        assert_eq!(
            admitted.live_producers,
            QueueConfig::MAX_LIVE_ENTRIES,
            "shared admission records every direct producer"
        );
        assert!(
            admitted.total_reserved_bytes <= QueueConfig::MAX_CLIENT_BYTE_LIMIT,
            "fixed storage remains inside the one byte owner: {admitted:?}"
        );
        assert!(matches!(
            Producer::with_budget(
                "vala.bifrost.direct_capacity_overflow",
                schema(),
                sink.clone(),
                config,
                budget.clone(),
            ),
            Err(WyrdQueueError::Backpressure)
        ));
        assert_eq!(
            budget.metrics(),
            admitted,
            "the refused producer cannot grow fixed accounting or permits"
        );
        for producer in &producers {
            producer
                .shutdown()
                .expect("empty direct producer shuts down");
        }
        let settled = budget.metrics();
        assert_eq!(settled.live_producers, 0, "terminal drain releases permits");
        assert_eq!(
            settled.fixed_storage_bytes, 0,
            "terminal drain releases slots"
        );
        assert_eq!(
            settled.total_reserved_bytes, 0,
            "terminal drain settles owner"
        );

        let dropped = Producer::with_budget(
            "vala.bifrost.direct_capacity_drop",
            schema(),
            sink,
            config,
            budget.clone(),
        )
        .expect("released permit allows a replacement producer");
        assert_eq!(
            budget.metrics().live_producers,
            1,
            "replacement owns one permit"
        );
        drop(dropped);
        assert_eq!(
            budget.metrics().live_producers,
            0,
            "producer drop releases permit"
        );
    }

    /// Rejects zero producer configuration without changing shared permit state.
    #[test]
    fn zero_producer_config_refuses_without_permit_or_ceiling_mutation() {
        let sink = Arc::new(MockSink::new());
        let budget = ClientByteBudget::new(QueueConfig::MAX_CLIENT_BYTE_LIMIT);
        let before = budget.metrics();
        assert!(matches!(
            Producer::with_budget(
                "vala.bifrost.direct_capacity_zero",
                schema(),
                sink.clone(),
                QueueConfig {
                    max_producers: 0,
                    flush_interval_ms: 0,
                    ..QueueConfig::default()
                },
                budget.clone(),
            ),
            Err(WyrdQueueError::Backpressure)
        ));
        assert_eq!(
            budget.metrics(),
            before,
            "zero configuration cannot reserve bytes, permits, or tighten the ceiling"
        );
        let config = QueueConfig {
            flush_interval_ms: 0,
            ..QueueConfig::default()
        };
        let first = Producer::with_budget(
            "vala.bifrost.direct_capacity_after_zero_1",
            schema(),
            sink.clone(),
            config,
            budget.clone(),
        )
        .expect("zero refusal leaves default capacity available");
        let second = Producer::with_budget(
            "vala.bifrost.direct_capacity_after_zero_2",
            schema(),
            sink,
            config,
            budget.clone(),
        )
        .expect("zero refusal did not tighten the shared ceiling to one");
        first
            .shutdown()
            .expect("first zero-cap regression producer drains");
        second
            .shutdown()
            .expect("second zero-cap regression producer drains");
    }

    /// Rolls producer admission back when fixed byte reservation fails after permit acquisition.
    #[test]
    fn shared_producer_permit_rolls_back_after_fixed_admission_failure() {
        let sink = Arc::new(MockSink::new());
        let budget = ClientByteBudget::new(QueueConfig::MAX_CLIENT_BYTE_LIMIT);
        let blocker = budget
            .reserve(QueueConfig::MAX_CLIENT_BYTE_LIMIT)
            .expect("test reserves the complete byte owner");
        assert!(matches!(
            Producer::with_budget(
                "vala.bifrost.direct_capacity_blocked",
                schema(),
                sink,
                QueueConfig {
                    max_producers: 1,
                    flush_interval_ms: 0,
                    ..QueueConfig::default()
                },
                budget.clone(),
            ),
            Err(WyrdQueueError::Backpressure)
        ));
        let refused = budget.metrics();
        assert_eq!(
            refused.live_producers, 0,
            "failed fixed admission drops the already-acquired permit"
        );
        assert_eq!(
            refused.fixed_storage_bytes, 0,
            "failed fixed admission creates no queue storage charge"
        );
        drop(blocker);
        let config = QueueConfig {
            flush_interval_ms: 0,
            ..QueueConfig::default()
        };
        let first = Producer::with_budget(
            "vala.bifrost.direct_capacity_after_fixed_failure_1",
            schema(),
            Arc::new(MockSink::new()),
            config,
            budget.clone(),
        )
        .expect("failed lower-cap admission leaves default capacity available");
        let second = Producer::with_budget(
            "vala.bifrost.direct_capacity_after_fixed_failure_2",
            schema(),
            Arc::new(MockSink::new()),
            config,
            budget,
        )
        .expect("failed fixed admission did not tighten the shared ceiling to one");
        first
            .shutdown()
            .expect("first post-failure producer drains");
        second
            .shutdown()
            .expect("second post-failure producer drains");
    }

    /// Retries one exact sealed owner identity after an ambiguous sink result.
    #[test]
    fn bounded_retry_retains_the_same_batch_identity() {
        let sink = Arc::new(MockSink::new());
        sink.fail_next(1);
        let budget = ClientByteBudget::new(QueueConfig::MAX_CLIENT_BYTE_LIMIT);
        let producer = Producer::with_budget(
            "vala.bifrost.queue",
            schema(),
            sink.clone(),
            QueueConfig {
                flush_interval_ms: 0,
                ..QueueConfig::default()
            },
            budget.clone(),
        )
        .expect("fixed queue storage fits the default budget");
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
        let settled = budget.metrics();
        assert_eq!(
            settled.owned_bytes, 0,
            "durable ACK settles dynamic ownership"
        );
        assert_eq!(
            settled.live_batches, 0,
            "durable ACK releases the batch slot"
        );
        assert_eq!(
            settled.retry_entries, 0,
            "durable ACK releases the retry slot"
        );
        assert!(
            settled.fixed_storage_bytes > 0,
            "the live producer retains its fixed queue storage charge"
        );
        producer.shutdown().expect("settled producer shutdown");
        assert_eq!(
            budget.used_bytes(),
            0,
            "shutdown releases fixed queue storage"
        );
    }

    /// Preserves one exact owner when the bounded queue deadline cancels a borrow.
    #[test]
    fn flush_timeout_retains_batch_until_later_ack() {
        let sink = Arc::new(TimeoutSink::default());
        let budget = ClientByteBudget::new(QueueConfig::MAX_CLIENT_BYTE_LIMIT);
        let producer = Producer::with_budget(
            "vala.bifrost.timeout",
            schema(),
            sink.clone(),
            QueueConfig {
                flush_interval_ms: 0,
                flush_timeout_ms: 5,
                ..QueueConfig::default()
            },
            budget.clone(),
        )
        .expect("fixed queue storage fits the default budget");
        producer
            .enqueue(br#"{"id": 7}"#.to_vec(), card(), None)
            .expect("row accepted");
        assert!(matches!(
            producer.flush(),
            Err(WyrdQueueError::FlushTimeout)
        ));
        let retained = budget.metrics();
        assert!(retained.owned_bytes > 0, "timeout retains the byte owner");
        assert_eq!(retained.live_batches, 1, "timeout retains the batch slot");
        assert_eq!(retained.retry_entries, 1, "timeout retains the retry slot");
        assert_eq!(
            sink.cancelled.load(Ordering::Acquire),
            1,
            "deadline cancelled only the borrowed sink future"
        );
        sink.release();
        producer.flush().expect("retained timeout batch resolves");
        let attempts = sink
            .attempts
            .lock()
            .expect("timeout sink attempt lock poisoned")
            .clone();
        assert_eq!(attempts.len(), 2, "one timeout then one resolving retry");
        assert_eq!(attempts[0], attempts[1], "timeout retains the same UUIDv7");
        let settled = budget.metrics();
        assert_eq!(settled.owned_bytes, 0, "ACK releases dynamic bytes");
        assert_eq!(settled.live_batches, 0, "ACK releases batch slot");
        assert_eq!(settled.retry_entries, 0, "ACK releases retry slot");
        producer.shutdown().expect("timeout producer shutdown");
        assert_eq!(budget.used_bytes(), 0, "shutdown releases fixed storage");
    }
}
