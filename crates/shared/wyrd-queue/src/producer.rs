//! The client byte owner and one background producer per table.

use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use arrow::record_batch::RecordBatch;
use arrow_schema::SchemaRef;
use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;
use wyrd_spec::reference::CardRef;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::ids::RunId;

use crate::batch_builder::{PreparedRows, RowPreflight};
use crate::config::QueueConfig;
use crate::error::WyrdQueueError;
use crate::queue::{Entry, FlushOutcome, InFlight, OwnedBatch, RecordQueue, Row, Settled};
use crate::sink::BatchSink;

/// Running producer state.
const RUNNING: u8 = 0;
/// Draining producer state.
const DRAINING: u8 = 1;
/// Fully drained producer state.
const DRAINED: u8 = 2;
/// Pause before a dropped handle's drain retries work no retry due time covers.
const DROPPED_DRAIN_PAUSE: Duration = Duration::from_millis(10);

/// One shared handle-wide byte budget.
///
/// The budget is the only bound on buffered client memory: every producer of
/// one handle charges its rows, sealed frames, and retained batches here.
/// Nothing is preallocated, so an idle producer holds zero bytes and the
/// number of producers is unbounded.
#[derive(Debug, Clone)]
pub struct ClientByteBudget {
    state: Arc<ClientBudgetState>,
}

/// Shared allocation state for one client handle.
#[derive(Debug)]
struct ClientBudgetState {
    /// Immutable byte ceiling covering every buffered client byte.
    limit: usize,
    /// Bytes below `limit` that only sealing may reserve, so admitted rows
    /// can never take the frame space needed to send them.
    sealing_headroom: usize,
    /// Bytes currently reserved under the handle-wide owner.
    used: AtomicUsize,
    /// Sealed batches awaiting terminal settlement, reported as a metric.
    live_batches: AtomicUsize,
    /// Ambiguous batches retained for later retry, reported as a metric.
    retry_entries: AtomicUsize,
    /// Observer told the row count of every loss a queue of this handle settles.
    loss_observer: OnceLock<LossObserver>,
}

/// Callback receiving the row count of each settled post-admission loss.
struct LossObserver(Box<dyn Fn(u64) + Send + Sync>);

impl fmt::Debug for LossObserver {
    /// Names the observer without exposing the callback.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LossObserver")
    }
}

impl ClientByteBudget {
    /// Creates the single budget used by all producers in one handle, with no
    /// sealing headroom.
    #[must_use]
    pub fn new(limit: usize) -> Self {
        Self::with_sealing_headroom(limit, 0)
    }

    /// Creates the budget a handle built from `config` shares across its
    /// producers.
    ///
    /// One message ceiling of `max_message_bytes`, capped at half the byte
    /// limit, is kept back from admission for sealing. Rows can then never
    /// fill the budget so completely that the frame needed to send them
    /// cannot be reserved; a seal waits only for frames already in flight to
    /// settle.
    #[must_use]
    pub fn for_config(config: &QueueConfig) -> Self {
        let limit = config.client_byte_limit_bytes;
        Self::with_sealing_headroom(limit, config.max_message_bytes.min(limit / 2))
    }

    /// Creates a budget whose admission stops `sealing_headroom` bytes below
    /// `limit`.
    fn with_sealing_headroom(limit: usize, sealing_headroom: usize) -> Self {
        Self {
            state: Arc::new(ClientBudgetState {
                limit,
                sealing_headroom,
                used: AtomicUsize::new(0),
                live_batches: AtomicUsize::new(0),
                retry_entries: AtomicUsize::new(0),
                loss_observer: OnceLock::new(),
            }),
        }
    }

    /// Returns the most bytes admission can ever hold: the limit less the
    /// sealing headroom. A record charging more can never be admitted.
    #[must_use]
    pub fn admission_limit(&self) -> usize {
        self.state.limit - self.state.sealing_headroom
    }

    /// Reserves bytes for admitted work before a row, builder, or batch
    /// allocation can grow.
    ///
    /// Admission stops at [`Self::admission_limit`].
    ///
    /// # Errors
    ///
    /// Returns [`WyrdQueueError::Backpressure`] when the complete handle-wide
    /// live set would exceed the admission ceiling.
    pub fn reserve(&self, bytes: usize) -> Result<ClientByteGuard, WyrdQueueError> {
        self.reserve_within(bytes, self.admission_limit())
    }

    /// Reserves bytes for a frame being sealed, which may use the sealing
    /// headroom up to the full byte limit.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdQueueError::Backpressure`] when the complete handle-wide
    /// live set would exceed the byte limit.
    pub(crate) fn reserve_sealing(&self, bytes: usize) -> Result<ClientByteGuard, WyrdQueueError> {
        self.reserve_within(bytes, self.state.limit)
    }

    /// Reserves `bytes` unless the handle-wide live set would pass `ceiling`.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdQueueError::Backpressure`] when it would.
    fn reserve_within(
        &self,
        bytes: usize,
        ceiling: usize,
    ) -> Result<ClientByteGuard, WyrdQueueError> {
        loop {
            let used = self.state.used.load(Ordering::Acquire);
            let Some(next) = used.checked_add(bytes) else {
                return Err(WyrdQueueError::Backpressure);
            };
            if next > ceiling {
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

    /// Reserves bytes for a manually constructed batch and counts it live.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdQueueError::Backpressure`] before the batch can be
    /// constructed when its bytes do not fit.
    pub fn reserve_sealed(&self, bytes: usize) -> Result<ClientByteGuard, WyrdQueueError> {
        Ok(self.reserve(bytes)?.attach_batch())
    }

    /// Counts one retained batch until its permit drops.
    pub(crate) fn reserve_retry(&self) -> RetryPermit {
        self.state.retry_entries.fetch_add(1, Ordering::AcqRel);
        RetryPermit {
            budget: self.clone(),
        }
    }

    /// Registers `observer` to receive the row count of every loss this
    /// handle's queues settle, synchronously on the settling task, so a loss
    /// is observable without a later enqueue or polling. The lost owner's
    /// bytes and retry slot are already released when the observer runs, so
    /// [`Self::metrics`] read there reports the settled ownership.
    ///
    /// The observer must not block. Only the first registration takes effect.
    pub fn observe_losses(&self, observer: impl Fn(u64) + Send + Sync + 'static) {
        let _ = self
            .state
            .loss_observer
            .set(LossObserver(Box::new(observer)));
    }

    /// Tells the registered observer, if any, that `rows` were settled as lost.
    pub(crate) fn report_loss(&self, rows: u64) {
        if let Some(LossObserver(observer)) = self.state.loss_observer.get() {
            observer(rows);
        }
    }

    /// Returns currently owned client bytes for allocation assertions and telemetry.
    #[must_use]
    pub fn used_bytes(&self) -> usize {
        self.state.used.load(Ordering::Acquire)
    }

    /// Returns one point-in-time view of the handle-wide ownership budget.
    ///
    /// The values are independent atomic observations intended for telemetry
    /// and settlement assertions, rather than a transactional snapshot.
    #[must_use]
    pub fn metrics(&self) -> ClientByteMetrics {
        ClientByteMetrics {
            owned_bytes: self.used_bytes(),
            live_batches: self.state.live_batches.load(Ordering::Acquire),
            retry_entries: self.state.retry_entries.load(Ordering::Acquire),
        }
    }
}

/// Point-in-time accounting for the ownership held by one client handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientByteMetrics {
    /// Bytes currently reserved by queued rows, sealed frames, and retained batches.
    pub owned_bytes: usize,
    /// Sealed batch owners that have not reached terminal settlement.
    pub live_batches: usize,
    /// Retained ambiguous batches awaiting a later retry attempt.
    pub retry_entries: usize,
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

    /// Sets this reservation to exactly `bytes`, reserving any growth from
    /// the sealing headroom before it is owned.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdQueueError::Backpressure`] when growth does not fit the
    /// handle-wide budget; the original reservation is then released with
    /// this guard.
    pub(crate) fn fit(mut self, bytes: usize) -> Result<Self, WyrdQueueError> {
        if bytes <= self.bytes {
            return Ok(self.resize(bytes));
        }
        let mut growth = self.budget.reserve_sealing(bytes - self.bytes)?;
        // The growth bytes transfer into `self`, so the temporary guard must
        // not return them when it drops.
        growth.bytes = 0;
        self.bytes = bytes;
        Ok(self)
    }

    /// Counts this byte owner as one live sealed batch until it drops.
    #[must_use]
    pub(crate) fn attach_batch(mut self) -> Self {
        self.budget
            .state
            .live_batches
            .fetch_add(1, Ordering::AcqRel);
        self.batch_permit = Some(BatchPermit {
            budget: self.budget.clone(),
        });
        self
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

/// Uncounts one live sealed batch with its owning batch.
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

/// Uncounts one retained batch when it settles.
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

/// Shared accepted, dropped, and staged counts.
#[derive(Debug, Default)]
pub(crate) struct Counters {
    /// Rows accepted by admission.
    pub(crate) accepted: AtomicU64,
    /// Rows refused or terminally discarded with visible accounting.
    pub(crate) dropped: AtomicU64,
    /// Rows waiting in staging for a seal.
    pub(crate) staged: AtomicUsize,
}

/// Snapshot of producer occupancy and visible outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProducerMetrics {
    /// Accepted rows.
    pub accepted: u64,
    /// Refused or discarded rows.
    pub dropped: u64,
    /// Admitted rows not yet sealed: in the channel or in staging.
    pub queue_depth: usize,
}

/// One producer control command.
enum Ctrl {
    /// Flushes buffered work and returns durable identities.
    Flush(oneshot::Sender<Result<Vec<[u8; 16]>, WyrdQueueError>>),
    /// Drains and stops the task.
    Shutdown(oneshot::Sender<Result<(), WyrdQueueError>>),
}

/// The caller-facing producer for one table.
///
/// Admission never waits: a whole record is charged to the shared byte
/// budget and handed to the background owner as one message, or refused.
pub struct Producer {
    preflight: RowPreflight,
    tx: mpsc::UnboundedSender<Entry>,
    ctrl_tx: mpsc::Sender<Ctrl>,
    control_pending: Arc<AtomicBool>,
    channel_depth: Arc<AtomicUsize>,
    counters: Arc<Counters>,
    state: Arc<AtomicU8>,
    budget: ClientByteBudget,
}

impl Producer {
    /// Creates a standalone producer with its own budget sized by `config`.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdQueueError::ConfigInvalid`] when `config` fails
    /// [`QueueConfig::validate`].
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
            ClientByteBudget::for_config(&config),
        )
    }

    /// Creates a producer sharing the supplied handle-wide budget.
    ///
    /// Construction allocates no buffer: the data channel, staging, and
    /// outbox all grow only with admitted bytes, so any number of producers
    /// can share one budget and an idle one holds nothing.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdQueueError::ConfigInvalid`] when `config` fails
    /// [`QueueConfig::validate`].
    pub fn with_budget(
        table: &str,
        schema: SchemaRef,
        sink: Arc<dyn BatchSink<ClientByteGuard>>,
        config: QueueConfig,
        budget: ClientByteBudget,
    ) -> Result<Self, WyrdQueueError> {
        config.validate()?;
        let preflight = RowPreflight::new(&schema);
        let (tx, rx) = mpsc::unbounded_channel();
        let (ctrl_tx, ctrl_rx) = mpsc::channel(1);
        let channel_depth = Arc::new(AtomicUsize::new(0));
        let counters = Arc::new(Counters::default());
        let state = Arc::new(AtomicU8::new(RUNNING));
        let control_pending = Arc::new(AtomicBool::new(false));
        let task = Task {
            queue: RecordQueue::new(
                table.to_owned(),
                Arc::clone(preflight.output_schema()),
                sink,
                config,
                budget.clone(),
                Arc::clone(&counters),
            ),
            rx,
            ctrl_rx,
            control_pending: Arc::clone(&control_pending),
            channel_depth: Arc::clone(&channel_depth),
            state: Arc::clone(&state),
            linger: config.linger(),
            max_in_flight: config.max_in_flight,
            in_flight: FuturesUnordered::new(),
            linger_deadline: None,
            deferred_error: None,
        };
        wyrd_runtime::runtime().spawn(async move {
            if let Some((reply, result)) = task.run().await {
                let _ = reply.send(result);
            }
        });
        Ok(Self {
            preflight,
            tx,
            ctrl_tx,
            control_pending,
            channel_depth,
            counters,
            state,
            budget,
        })
    }

    /// Admits one row without waiting.
    ///
    /// `card_ref` is optional because Card correlation is: an omitted value
    /// becomes a null in the sealed batch's `card_ref` column, which the server
    /// stores against the authenticated principal with a null `card_uid`.
    ///
    /// # Errors
    ///
    /// As [`Self::enqueue_rows`] for one row.
    pub fn enqueue(
        &self,
        json: Vec<u8>,
        card_ref: Option<CardRef>,
        run_id: Option<RunId>,
    ) -> Result<(), WyrdQueueError> {
        self.enqueue_rows(vec![json], card_ref, run_id)
    }

    /// Prepares and admits every row of one logical record, or none of them,
    /// without waiting.
    ///
    /// The complete record goes through this producer's [`RowPreflight`]
    /// first, so a refused row leaves queue, budget, and counters untouched;
    /// admission then follows [`Self::enqueue_prepared`]. Every row carries
    /// the same correlation.
    ///
    /// # Errors
    ///
    /// Returns any [`RowPreflight::prepare`] refusal without counting a row,
    /// then any [`Self::enqueue_prepared`] refusal.
    pub fn enqueue_rows(
        &self,
        rows: Vec<Vec<u8>>,
        card_ref: Option<CardRef>,
        run_id: Option<RunId>,
    ) -> Result<(), WyrdQueueError> {
        let prepared = self
            .preflight
            .prepare(&rows, card_ref.as_ref(), run_id.as_ref())?;
        drop(rows);
        self.enqueue_prepared(prepared)
    }

    /// Admits one prepared record, all or none, without waiting.
    ///
    /// The record's exact charge is reserved once and the record is handed
    /// over as one message of one-row slices sharing that reservation; nothing
    /// after the reservation can fail, and a refusal releases it, so a caller
    /// may resubmit the whole record without duplicating an accepted prefix.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdQueueError::PayloadTooLarge`] when the record charges
    /// more than the whole admission budget, [`WyrdQueueError::QueueFull`]
    /// when the producer is draining, and [`WyrdQueueError::Backpressure`]
    /// when the handle budget cannot cover the record now. Each refusal counts
    /// every row as dropped.
    pub fn enqueue_prepared(&self, prepared: PreparedRows) -> Result<(), WyrdQueueError> {
        let (batch, charge) = prepared.into_parts();
        let count = batch.num_rows();
        let admitted = self.admission_check(charge).and_then(|()| {
            let guard = Arc::new(self.budget.reserve(charge)?);
            let bytes = charge / count.max(1);
            Ok(Entry::Rows(
                (0..count)
                    .map(|row| Row {
                        batch: batch.slice(row, 1),
                        bytes,
                        _guard: Arc::clone(&guard),
                    })
                    .collect(),
            ))
        });
        self.hand_over(count as u64, admitted)
    }

    /// Admits one owned Arrow batch without waiting for encoding or publication.
    ///
    /// The batch's array memory is charged to the handle-wide budget and the
    /// batch is handed over as one message. The background owner later seals
    /// it alone under a stable UUIDv7 and settles it through the same retry,
    /// flush, and shutdown lifecycle as JSON rows. Its schema is sent
    /// unchanged; the destination contract is enforced by the server. A
    /// supplied `request_id` travels with the sealed frame, so every transport
    /// attempt, including retries, publishes under that originating request.
    ///
    /// # Errors
    ///
    /// As [`Self::enqueue_rows`], charged the batch's array memory.
    pub fn enqueue_batch(
        &self,
        batch: RecordBatch,
        request_id: Option<RequestId>,
    ) -> Result<(), WyrdQueueError> {
        let rows = batch.num_rows() as u64;
        let bytes = batch.get_array_memory_size();
        let admitted = self.admission_check(bytes).and_then(|()| {
            Ok(Entry::Batch(OwnedBatch {
                guard: self.budget.reserve(bytes)?,
                batch,
                request_id,
            }))
        });
        self.hand_over(rows, admitted)
    }

    /// Refuses a record this producer can never or can no longer admit.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdQueueError::QueueFull`] once the producer is draining and
    /// [`WyrdQueueError::PayloadTooLarge`] when `bytes` exceeds the whole
    /// admission budget.
    fn admission_check(&self, bytes: usize) -> Result<(), WyrdQueueError> {
        if self.state.load(Ordering::Acquire) != RUNNING {
            return Err(WyrdQueueError::QueueFull);
        }
        if bytes > self.budget.admission_limit() {
            return Err(WyrdQueueError::PayloadTooLarge);
        }
        Ok(())
    }

    /// Hands one admitted entry to the background owner, counting `rows` as
    /// accepted, or counts them as dropped when admission or the hand-over
    /// failed.
    ///
    /// # Errors
    ///
    /// Returns the admission error, or [`WyrdQueueError::QueueFull`] when the
    /// background owner has stopped.
    fn hand_over(
        &self,
        rows: u64,
        admitted: Result<Entry, WyrdQueueError>,
    ) -> Result<(), WyrdQueueError> {
        let sent = admitted.and_then(|entry| {
            let depth = entry.rows();
            self.channel_depth.fetch_add(depth, Ordering::AcqRel);
            self.tx.send(entry).map_err(|_| {
                self.channel_depth.fetch_sub(depth, Ordering::AcqRel);
                WyrdQueueError::QueueFull
            })
        });
        let counter = match sent {
            Ok(()) => &self.counters.accepted,
            Err(_) => &self.counters.dropped,
        };
        counter.fetch_add(rows, Ordering::AcqRel);
        sent
    }

    /// Flushes all currently buffered work through its durable acknowledgement.
    ///
    /// # Errors
    ///
    /// Returns typed backpressure when another control command is outstanding,
    /// plus sealing and transport errors from the background owner, including
    /// a terminal refusal of a batch a background seal sent since the last
    /// flush or shutdown reply.
    pub fn flush(&self) -> Result<Vec<[u8; 16]>, WyrdQueueError> {
        self.control(Ctrl::Flush)
    }

    /// Drains the producer and rejects later rows after terminal settlement.
    ///
    /// # Errors
    ///
    /// Returns typed backpressure when another control command is outstanding,
    /// the first durable transport error encountered while draining, or an
    /// earlier terminal refusal of a background-sent batch not yet reported.
    pub fn shutdown(&self) -> Result<(), WyrdQueueError> {
        self.control(Ctrl::Shutdown).map(|_| ())
    }

    /// Returns a lightweight occupancy snapshot.
    #[must_use]
    pub fn metrics(&self) -> ProducerMetrics {
        ProducerMetrics {
            accepted: self.counters.accepted.load(Ordering::Acquire),
            dropped: self.counters.dropped.load(Ordering::Acquire),
            queue_depth: self.channel_depth.load(Ordering::Acquire)
                + self.counters.staged.load(Ordering::Acquire),
        }
    }

    /// Reports whether this producer currently owns its one control slot.
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

    /// Sends one control command after taking this producer's one control
    /// slot, then blocks for its reply.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdQueueError::Backpressure`] when a control is already
    /// outstanding, [`WyrdQueueError::QueueFull`] when the background owner
    /// has stopped, and otherwise the command's own result.
    fn control<T>(
        &self,
        make: impl FnOnce(oneshot::Sender<Result<T, WyrdQueueError>>) -> Ctrl,
    ) -> Result<T, WyrdQueueError> {
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
///
/// One task owns staging, sealing, and up to `max_in_flight` concurrent sink
/// attempts. Intake never awaits the network: while attempts are in flight
/// the task keeps taking admitted records into staging and sealing batches
/// into the outbox. Staging seals when its JSON bytes reach the message
/// ceiling, or when the linger since its first row has elapsed and a send
/// slot is free, so batches grow while every slot is busy. Batches carry no
/// order relative to one another. After a shutdown that could not settle
/// everything, the task keeps retrying retained batches and serving controls
/// until a later shutdown succeeds or the handle drops.
struct Task {
    /// Staging, outbox, and retry owner.
    queue: RecordQueue,
    /// Admitted records, one message each.
    rx: mpsc::UnboundedReceiver<Entry>,
    /// Control receiver.
    ctrl_rx: mpsc::Receiver<Ctrl>,
    /// One command-slot occupancy flag shared with the producer handle.
    control_pending: Arc<AtomicBool>,
    /// Admitted rows not yet taken from the channel, shared with the handle.
    channel_depth: Arc<AtomicUsize>,
    /// Lifecycle state shared with the producer handle.
    state: Arc<AtomicU8>,
    /// How long staged rows may wait for more before a seal.
    linger: Duration,
    /// Most sink attempts this producer runs at once.
    max_in_flight: usize,
    /// The sink attempts in flight, each owning its batch until it settles.
    in_flight: FuturesUnordered<InFlight>,
    /// When the oldest staged row's linger elapses, while rows are staged.
    linger_deadline: Option<Instant>,
    /// First loss of a send or seal no caller was waiting on.
    ///
    /// A background seal or send can be refused terminally (for example an
    /// RBAC denial) with no control command to carry the error, and the
    /// refused batch is already consumed. The error waits here and is
    /// reported, exactly once, by the next flush or shutdown reply.
    deferred_error: Option<WyrdQueueError>,
}

impl Task {
    /// Runs the receiver loop until terminal drain or every sender is dropped.
    ///
    /// A successful shutdown returns its reply sender, with the result to send,
    /// only after this method consumes `self`; the outer spawned future sends
    /// that reply after all task-owned queue state has dropped. The result is
    /// `Ok` unless a background send was refused since the last control reply.
    async fn run(
        mut self,
    ) -> Option<(
        oneshot::Sender<Result<(), WyrdQueueError>>,
        Result<(), WyrdQueueError>,
    )> {
        loop {
            let retry_due = self.queue.next_retry_due();
            let can_send = self.in_flight.len() < self.max_in_flight;
            let open = self.state.load(Ordering::Acquire) == RUNNING;
            tokio::select! {
                biased;
                control = self.ctrl_rx.recv() => match control {
                    Some(Ctrl::Flush(reply)) => {
                        let mut outcome = FlushOutcome::default();
                        let result = self.drain(false, &mut outcome).await;
                        let result = self.report_deferred(result.map(|()| outcome.batch_ids));
                        self.control_pending.store(false, Ordering::Release);
                        let _ = reply.send(result);
                    }
                    Some(Ctrl::Shutdown(reply)) => {
                        self.state.store(DRAINING, Ordering::Release);
                        self.rx.close();
                        let result = self.drain(true, &mut FlushOutcome::default()).await;
                        let result = self.report_deferred(result);
                        self.control_pending.store(false, Ordering::Release);
                        if result.is_ok() {
                            self.state.store(DRAINED, Ordering::Release);
                            return Some((reply, result));
                        }
                        let _ = reply.send(result);
                    }
                    None => {
                        self.drain_dropped_handle().await;
                        return None;
                    }
                },
                Some(attempt) = self.in_flight.next(), if !self.in_flight.is_empty() => {
                    let settled = self.queue.settle(attempt, &mut FlushOutcome::default());
                    self.settle_background(settled);
                    self.pump();
                },
                entry = self.rx.recv(), if open => match entry {
                    Some(entry) => {
                        self.take(entry);
                        self.pump();
                    }
                    None => {
                        self.drain_dropped_handle().await;
                        return None;
                    }
                },
                () = sleep_until(self.linger_deadline), if can_send && self.linger_deadline.is_some() => {
                    self.pump();
                },
                () = sleep_until(retry_due), if can_send && retry_due.is_some() => {
                    self.pump();
                },
            }
        }
    }

    /// Places one admitted entry into staging or the outbox, deferring an
    /// Arrow batch's sealing loss to the next control reply.
    fn take(&mut self, entry: Entry) {
        self.channel_depth.fetch_sub(entry.rows(), Ordering::AcqRel);
        if let Err(error) = self.queue.place(entry) {
            self.deferred_error.get_or_insert(error);
        }
    }

    /// Seals staging that is due and starts every send a slot allows.
    ///
    /// Staging seals once its bytes reach the message ceiling, or once its
    /// linger has elapsed (or is zero) and a send slot is free. While every
    /// slot is busy an elapsed linger stays armed and staging keeps growing,
    /// so the next settle seals the accumulated rows as one larger batch;
    /// otherwise the linger starts with the first staged row. A seal refused for frame space restores its rows and is
    /// retried on the next pass. Sends start, retained batches whose backoff
    /// elapsed first, until `max_in_flight` attempts are running.
    fn pump(&mut self) {
        if self.queue.staging_len() > 0 {
            let now = Instant::now();
            let lingered = self.linger.is_zero()
                || self.linger_deadline.is_some_and(|deadline| deadline <= now);
            if self.queue.staging_full() || (lingered && self.in_flight.len() < self.max_in_flight)
            {
                self.linger_deadline = None;
                match self.queue.seal() {
                    Ok(()) | Err(WyrdQueueError::Backpressure) => {}
                    Err(error) => {
                        self.deferred_error.get_or_insert(error);
                    }
                }
            }
            if self.queue.staging_len() > 0 && self.linger_deadline.is_none() {
                self.linger_deadline = Some(now + self.linger);
            }
        }
        let now = Instant::now();
        while self.in_flight.len() < self.max_in_flight {
            match self.queue.start(now) {
                Some(attempt) => self.in_flight.push(attempt),
                None => break,
            }
        }
    }

    /// Settles all work admitted before a control command.
    ///
    /// Every entry in the channel when the command arrived is placed (with
    /// `closed`, the channel was closed by shutdown and is drained to its
    /// end). Staging is sealed, and every sealed batch and every retained
    /// batch whose backoff elapsed when the pass began is sent once, up to
    /// `max_in_flight` at once, until nothing is in flight. A batch retained
    /// again during the pass waits for the next pass, so several ambiguous
    /// batches cannot keep re-arming one another and hold the pass open. A seal refused for frame space is retried
    /// as attempts settle. Acknowledged identities are appended to `outcome`.
    ///
    /// # Errors
    ///
    /// Returns the first seal or send error in settlement order once
    /// everything sendable has been attempted. With no such error, returns
    /// [`WyrdQueueError::FlushTimeout`] when an earlier retained batch still
    /// awaits its backoff, and [`WyrdQueueError::Backpressure`] when staged
    /// rows could not be sealed. Retained batches keep their stable identity.
    async fn drain(
        &mut self,
        closed: bool,
        outcome: &mut FlushOutcome,
    ) -> Result<(), WyrdQueueError> {
        let mut first = self.deferred_error.take();
        if closed {
            while let Some(entry) = self.rx.recv().await {
                self.take(entry);
            }
        } else {
            for _ in 0..self.rx.len() {
                let Ok(entry) = self.rx.try_recv() else {
                    break;
                };
                self.take(entry);
            }
        }
        if let Some(error) = self.deferred_error.take() {
            first.get_or_insert(error);
        }
        self.linger_deadline = None;
        let pass_start = Instant::now();
        loop {
            match self.queue.seal() {
                Ok(()) | Err(WyrdQueueError::Backpressure) => {}
                Err(error) => {
                    first.get_or_insert(error);
                }
            }
            while self.in_flight.len() < self.max_in_flight {
                match self.queue.start(pass_start) {
                    Some(attempt) => self.in_flight.push(attempt),
                    None => break,
                }
            }
            let Some(attempt) = self.in_flight.next().await else {
                break;
            };
            if let Err(error) = self.queue.settle(attempt, outcome).into_result() {
                first.get_or_insert(error);
            }
        }
        match first {
            Some(error) => Err(error),
            None if self.queue.has_retained() => Err(WyrdQueueError::FlushTimeout),
            None if self.queue.staging_len() > 0 => Err(WyrdQueueError::Backpressure),
            None => Ok(()),
        }
    }

    /// Settles a send no flush or shutdown caller is waiting on.
    ///
    /// A retained batch retries on its own due time; a lost batch cannot be
    /// retried, so the first such error is kept for [`Self::report_deferred`]
    /// instead of being dropped.
    fn settle_background(&mut self, settled: Settled) {
        if let Settled::Lost(error) = settled {
            self.deferred_error.get_or_insert(error);
        }
    }

    /// Replaces a control reply with the earliest deferred background failure.
    ///
    /// The deferred error happened before this control command's own seal, so
    /// it is reported first and cleared; a later control reply sees only its
    /// own result.
    ///
    /// # Errors
    ///
    /// Returns the deferred background error when one is waiting, otherwise
    /// `result` unchanged.
    fn report_deferred<T>(
        &mut self,
        result: Result<T, WyrdQueueError>,
    ) -> Result<T, WyrdQueueError> {
        match self.deferred_error.take() {
            Some(error) => Err(error),
            None => result,
        }
    }

    /// Resolves a dropped handle's ambiguous work before the task ends.
    ///
    /// Each pass drains everything; while retained or unsealed work remains
    /// the task keeps its bytes charged and waits for the next retry due time
    /// before resending the same stable batch identities. A terminal result
    /// that consumed the final owner leaves no pending work and ends the task.
    async fn drain_dropped_handle(&mut self) {
        self.state.store(DRAINING, Ordering::Release);
        loop {
            let _ = self.drain(true, &mut FlushOutcome::default()).await;
            if !self.queue.has_pending() {
                break;
            }
            match self.queue.next_retry_due() {
                Some(due) => tokio::time::sleep_until(due).await,
                None => tokio::time::sleep(DROPPED_DRAIN_PAUSE).await,
            }
        }
        self.state.store(DRAINED, Ordering::Release);
    }
}

/// Sleeps until `deadline`, or forever when there is none; cancellation-safe
/// in `select!`.
async fn sleep_until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending::<()>().await,
    }
}

#[cfg(test)]
mod tests {
    //! Shared-budget ownership tests for the ordinary Rust queue.

    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use arrow::array::{ArrayRef, BinaryArray, Int64Array, ListArray, StructArray};
    use arrow::datatypes::Int32Type;
    use arrow::ipc::reader::StreamReader;
    use arrow::record_batch::RecordBatch;
    use arrow_schema::{DataType, Field, Schema, SchemaRef};
    use async_trait::async_trait;
    use tokio::sync::Notify;
    use wyrd_spec::reference::CardRef;
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::BifrostError;

    use super::{ClientByteBudget, ProducerMetrics};
    use crate::{
        BatchSink, ClientByteGuard, DurableBatchAck, MockSink, Producer, QueueConfig, Row,
        RowPreflight, SealedBatch, SinkError, WyrdQueueError,
    };

    /// Builds a one-column user schema for sealed-batch tests.
    fn schema() -> SchemaRef {
        Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]))
    }

    /// Builds the card correlation carried by a queue row.
    fn card() -> CardRef {
        "prod/Service/queue@1.0.0".parse().expect("valid test card")
    }

    /// Builds `count` single-column JSON rows with sequential ids.
    fn rows(count: usize) -> Vec<Vec<u8>> {
        (0..count)
            .map(|id| format!(r#"{{"id":{id}}}"#).into_bytes())
            .collect()
    }

    /// Returns the bytes admission charges for `rows` as one card-correlated
    /// record: its prepared arrays plus one row owner per row.
    ///
    /// # Panics
    ///
    /// Panics when the rows do not prepare against [`schema`].
    fn charge(rows: &[Vec<u8>]) -> usize {
        RowPreflight::new(&schema())
            .prepare(rows, Some(&card()), None)
            .expect("rows prepare")
            .charge()
    }

    /// Cancellable sink that acknowledges only after the test releases it.
    #[derive(Default)]
    struct TimeoutSink {
        /// Identities observed before each borrowed send attempt.
        attempts: Mutex<Vec<[u8; 16]>>,
        /// Frame allocation addresses observed before each borrowed send attempt.
        allocations: Mutex<Vec<usize>>,
        /// Whether later attempts may complete with a durable acknowledgement.
        released: AtomicBool,
        /// Wakes waiting attempts after the test chooses to resolve them.
        wake: Notify,
        /// Borrowed send futures cancelled by the queue deadline.
        cancelled: AtomicUsize,
        /// Attempts currently waiting inside the sink.
        waiting: AtomicUsize,
        /// Most attempts ever waiting inside the sink at once.
        peak: AtomicUsize,
    }

    /// Records whether one borrowed sink attempt reached an acknowledgement.
    struct BorrowedAttempt<'a> {
        /// The sink whose counters this attempt updates.
        sink: &'a TimeoutSink,
        /// Set after the sink has produced a durable acknowledgement.
        acknowledged: bool,
    }

    impl Drop for BorrowedAttempt<'_> {
        /// Counts deadline cancellation without touching the queue-owned batch.
        fn drop(&mut self) {
            self.sink.waiting.fetch_sub(1, Ordering::AcqRel);
            if !self.acknowledged {
                self.sink.cancelled.fetch_add(1, Ordering::AcqRel);
            }
        }
    }

    impl TimeoutSink {
        /// Releases all current and future borrowed attempts for durable acknowledgement.
        fn release(&self) {
            self.released.store(true, Ordering::Release);
            self.wake.notify_waiters();
        }

        /// Returns how many attempts reached the sink.
        ///
        /// # Panics
        ///
        /// Panics if the attempt recorder mutex is poisoned.
        fn attempted(&self) -> usize {
            self.attempts.lock().expect("attempt lock").len()
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
        /// Panics if a test-only recorder mutex is poisoned.
        async fn send(
            &self,
            batch: &SealedBatch<ClientByteGuard>,
        ) -> Result<DurableBatchAck, SinkError> {
            let waiting = self.waiting.fetch_add(1, Ordering::AcqRel) + 1;
            self.peak.fetch_max(waiting, Ordering::AcqRel);
            let mut attempt = BorrowedAttempt {
                sink: self,
                acknowledged: false,
            };
            self.attempts
                .lock()
                .expect("timeout sink attempt lock poisoned")
                .push(batch.batch_id);
            self.allocations
                .lock()
                .expect("timeout sink allocation lock poisoned")
                .push(batch.bytes().as_ptr() as usize);
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

    /// Waits until `condition` holds, panicking with `what` after five seconds.
    fn wait_until(what: &str, condition: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !condition() {
            assert!(Instant::now() < deadline, "{what}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Counts the rows a sink durably received across every receipt.
    fn received_rows(sink: &MockSink) -> u64 {
        sink.received().iter().map(|receipt| receipt.rows).sum()
    }

    /// Refuses a byte reservation before a payload owner can grow past the budget.
    #[test]
    fn bounded_client_bytes_refuse_before_allocation() {
        let budget = ClientByteBudget::new(8);
        assert!(matches!(
            budget.reserve(9),
            Err(WyrdQueueError::Backpressure)
        ));
        assert_eq!(budget.used_bytes(), 0);
    }

    /// A configuration that cannot work is refused before any task starts.
    #[test]
    fn invalid_config_is_refused_before_any_task() {
        let sink = Arc::new(MockSink::new());
        for config in [
            QueueConfig {
                max_in_flight: 0,
                ..QueueConfig::default()
            },
            QueueConfig {
                max_message_bytes: 0,
                ..QueueConfig::default()
            },
            QueueConfig {
                client_byte_limit_bytes: 2 * QueueConfig::default().max_message_bytes - 1,
                ..QueueConfig::default()
            },
        ] {
            assert!(matches!(
                Producer::new("vala.bifrost.invalid", schema(), sink.clone(), config),
                Err(WyrdQueueError::ConfigInvalid { .. })
            ));
        }
        assert!(sink.attempted().is_empty());
    }

    /// One budget serves a thousand table producers: none is refused, an idle
    /// one holds zero bytes, and every table's row publishes.
    #[test]
    fn a_thousand_tables_share_one_budget_and_idle_producers_hold_nothing() {
        let config = QueueConfig::default();
        let budget = ClientByteBudget::for_config(&config);
        let sink = Arc::new(MockSink::new());
        let producers = (0..1_000)
            .map(|index| {
                Producer::with_budget(
                    &format!("vala.bifrost.table_{index}"),
                    schema(),
                    sink.clone(),
                    config,
                    budget.clone(),
                )
                .expect("no producer count is refused")
            })
            .collect::<Vec<_>>();
        assert_eq!(budget.used_bytes(), 0, "idle producers hold no bytes");
        for producer in &producers {
            producer
                .enqueue(br#"{"id":1}"#.to_vec(), Some(card()), None)
                .expect("each table admits its row");
        }
        for producer in &producers {
            producer.shutdown().expect("each table drains");
        }
        assert_eq!(received_rows(&sink), 1_000);
        assert_eq!(budget.used_bytes(), 0);
    }

    /// A dropped handle's task keeps its retained batch and bytes, retries it
    /// under one identity and allocation, and releases everything on ACK.
    #[test]
    fn dropped_handle_keeps_retrying_its_retained_batch() {
        let sink = Arc::new(TimeoutSink::default());
        let budget = ClientByteBudget::new(QueueConfig::DEFAULT_CLIENT_BYTE_LIMIT);
        let config = QueueConfig {
            flush_timeout_ms: 50,
            ..QueueConfig::default()
        };
        let producer = Producer::with_budget(
            "vala.bifrost.drop_timeout",
            schema(),
            sink.clone(),
            config,
            budget.clone(),
        )
        .expect("producer starts");
        producer
            .enqueue(br#"{"id": 9}"#.to_vec(), Some(card()), None)
            .expect("row accepted before timeout");
        assert!(matches!(
            producer.flush(),
            Err(WyrdQueueError::FlushTimeout)
        ));
        let second_started = Instant::now();
        assert!(matches!(
            producer.shutdown(),
            Err(WyrdQueueError::FlushTimeout)
        ));
        assert!(
            second_started.elapsed() < Duration::from_millis(250),
            "later shutdown control is serviced within its bounded deadline"
        );
        drop(producer);
        let retained = budget.metrics();
        assert!(retained.owned_bytes > 0, "task retains the frame owner");
        assert_eq!(retained.live_batches, 1, "task retains the sealed batch");
        assert_eq!(retained.retry_entries, 1, "task retains the retry entry");
        sink.release();
        wait_until("the retained batch settles", || {
            budget.metrics().owned_bytes == 0
        });
        let resolved = budget.metrics();
        assert_eq!((resolved.live_batches, resolved.retry_entries), (0, 0));
        let attempts = sink.attempts.lock().expect("attempt lock").clone();
        let allocations = sink.allocations.lock().expect("allocation lock").clone();
        assert!(attempts.len() >= 2, "dropped task retries the batch");
        assert!(attempts.iter().all(|batch_id| *batch_id == attempts[0]));
        assert!(allocations.iter().all(|address| *address == allocations[0]));
    }

    /// Ambiguous sends retain one identity, uncapped, until a retry ACKs it.
    #[test]
    fn ambiguous_sends_retry_one_identity_until_acked() {
        let sink = Arc::new(MockSink::new());
        sink.fail_next(2);
        let budget = ClientByteBudget::new(QueueConfig::DEFAULT_CLIENT_BYTE_LIMIT);
        let producer = Producer::with_budget(
            "vala.bifrost.queue",
            schema(),
            sink.clone(),
            QueueConfig::default(),
            budget.clone(),
        )
        .expect("producer starts");
        producer
            .enqueue(br#"{"id": 1}"#.to_vec(), Some(card()), None)
            .expect("row accepted");
        assert!(
            producer.flush().is_err(),
            "first transport result is ambiguous"
        );
        wait_until("the retained batch ACKs", || received_rows(&sink) == 1);
        let attempts = sink.attempted();
        assert_eq!(attempts.len(), 3);
        assert!(attempts.iter().all(|batch_id| *batch_id == attempts[0]));
        producer.flush().expect("nothing remains");
        let settled = budget.metrics();
        assert_eq!(
            (
                settled.owned_bytes,
                settled.live_batches,
                settled.retry_entries
            ),
            (0, 0, 0)
        );
        producer.shutdown().expect("settled producer shutdown");
    }

    /// Preserves one exact owner when the queue deadline cancels a borrow.
    #[test]
    fn flush_timeout_retains_batch_until_later_ack() {
        let sink = Arc::new(TimeoutSink::default());
        let budget = ClientByteBudget::new(QueueConfig::DEFAULT_CLIENT_BYTE_LIMIT);
        let producer = Producer::with_budget(
            "vala.bifrost.timeout",
            schema(),
            sink.clone(),
            QueueConfig {
                flush_timeout_ms: 5,
                ..QueueConfig::default()
            },
            budget.clone(),
        )
        .expect("producer starts");
        producer
            .enqueue(br#"{"id": 7}"#.to_vec(), Some(card()), None)
            .expect("row accepted");
        assert!(matches!(
            producer.flush(),
            Err(WyrdQueueError::FlushTimeout)
        ));
        let retained = budget.metrics();
        assert!(retained.owned_bytes > 0, "timeout retains the byte owner");
        assert_eq!(retained.live_batches, 1, "timeout retains the batch");
        assert_eq!(retained.retry_entries, 1, "timeout retains the retry entry");
        assert!(sink.cancelled.load(Ordering::Acquire) >= 1);
        sink.release();
        wait_until("scheduled retry settles the retained batch", || {
            budget.metrics().live_batches == 0
        });
        producer
            .flush()
            .expect("flush succeeds after the scheduled retry settles");
        let attempts = sink.attempts.lock().expect("attempt lock").clone();
        assert!(attempts.len() >= 2, "timeout then a resolving retry");
        assert!(attempts.iter().all(|batch_id| *batch_id == attempts[0]));
        assert_eq!(budget.used_bytes(), 0, "ACK releases every byte");
        producer.shutdown().expect("timeout producer shutdown");
    }

    /// A shutdown that cannot settle everything keeps serving controls and
    /// retrying; a later shutdown then drains the producer.
    #[test]
    fn failed_shutdown_keeps_serving_controls_and_retries() {
        let sink = Arc::new(MockSink::new());
        sink.fail_next(1);
        let budget = ClientByteBudget::new(QueueConfig::DEFAULT_CLIENT_BYTE_LIMIT);
        let producer = Producer::with_budget(
            "vala.bifrost.failed_shutdown",
            schema(),
            sink.clone(),
            QueueConfig::default(),
            budget.clone(),
        )
        .expect("producer starts");
        producer
            .enqueue(br#"{"id":1}"#.to_vec(), Some(card()), None)
            .expect("row accepted");
        assert!(producer.shutdown().is_err(), "the first send is ambiguous");
        assert!(matches!(
            producer.enqueue(br#"{"id":2}"#.to_vec(), Some(card()), None),
            Err(WyrdQueueError::QueueFull)
        ));
        wait_until("the retained batch ACKs in the background", || {
            budget.metrics().retry_entries == 0
        });
        producer.shutdown().expect("nothing remains");
        assert!(producer.is_drained());
        let attempts = sink.attempted();
        assert!(attempts.iter().all(|batch_id| *batch_id == attempts[0]));
        assert_eq!(received_rows(&sink), 1);
    }

    /// Sink that terminally refuses every batch and signals each attempt.
    struct RefusingSink {
        /// Tells the test thread a background seal has reached the sink.
        attempted: Mutex<std::sync::mpsc::Sender<()>>,
    }

    #[async_trait]
    impl BatchSink<ClientByteGuard> for RefusingSink {
        /// Refuses the borrowed batch so the queue consumes it without retry.
        ///
        /// # Errors
        ///
        /// Always returns [`SinkError::Terminal`].
        ///
        /// # Panics
        ///
        /// Panics if the test-only signal mutex is poisoned.
        async fn send(
            &self,
            _batch: &SealedBatch<ClientByteGuard>,
        ) -> Result<DurableBatchAck, SinkError> {
            let _ = self
                .attempted
                .lock()
                .expect("refusing sink signal lock poisoned")
                .send(());
            Err(SinkError::Terminal(
                wyrd_spec::error::WyrdError::ServiceUnavailable {
                    message: "refusing sink".to_owned(),
                    details: serde_json::json!({ "refusing": true }),
                },
            ))
        }
    }

    /// A background send's terminal refusal reaches the next flush, once.
    ///
    /// A zero linger makes the task seal and send the row as soon as it
    /// arrives, before any flush command exists. The refused batch is consumed,
    /// so the explicit flush has nothing of its own to send; it must still
    /// report the refusal, and neither a second flush nor shutdown may report
    /// it again.
    ///
    /// # Panics
    ///
    /// Panics when the refusal is dropped, reported twice, or the sink is never
    /// reached.
    #[test]
    fn background_terminal_refusal_is_reported_by_the_next_flush_once() {
        let (signal, attempted) = std::sync::mpsc::channel();
        let sink = Arc::new(RefusingSink {
            attempted: Mutex::new(signal),
        });
        let producer = Producer::new(
            "vala.bifrost.background_refusal",
            schema(),
            sink,
            QueueConfig {
                linger_ms: 0,
                ..QueueConfig::default()
            },
        )
        .expect("producer starts");
        producer
            .enqueue(br#"{"id":1}"#.to_vec(), Some(card()), None)
            .expect("row admitted");
        attempted
            .recv()
            .expect("the immediate seal reaches the sink");

        let first = producer.flush();
        assert!(
            matches!(first, Err(WyrdQueueError::Sink(_))),
            "the refusal must reach the caller: {first:?}"
        );
        let second = producer.flush();
        assert!(second.is_ok(), "a refusal is reported once: {second:?}");
        let shutdown = producer.shutdown();
        assert!(shutdown.is_ok(), "nothing remains to report: {shutdown:?}");
    }

    /// Builds a two-row batch whose binary and nested columns the JSON row path cannot express.
    fn nested_batch() -> RecordBatch {
        let tags = ListArray::from_iter_primitive::<Int32Type, _, _>(vec![
            Some(vec![Some(1), Some(2)]),
            None,
        ]);
        let usage = StructArray::from(vec![(
            Arc::new(Field::new("tokens", DataType::Int64, false)),
            Arc::new(Int64Array::from(vec![7, 9])) as ArrayRef,
        )]);
        let columns: Vec<ArrayRef> = vec![
            Arc::new(BinaryArray::from(vec![
                b"\x00\xff".as_slice(),
                b"".as_slice(),
            ])),
            Arc::new(tags),
            Arc::new(usage),
        ];
        RecordBatch::try_from_iter(vec![
            ("body", columns[0].clone()),
            ("tags", columns[1].clone()),
            ("usage", columns[2].clone()),
        ])
        .expect("nested test batch is well formed")
    }

    /// Terminal refusal of a fresh or a retained batch settles the batch's
    /// rows as lost exactly once, tells the loss observer as they settle
    /// without a later enqueue, and releases every owner before the observer
    /// reads the handle's ownership. Shutdown still drains and reports the
    /// first background refusal exactly once.
    #[test]
    fn accepted_batch_losses_settle_exactly_once() {
        let sink = Arc::new(MockSink::new());
        let budget = ClientByteBudget::new(QueueConfig::DEFAULT_CLIENT_BYTE_LIMIT);
        let lost = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let observed = Arc::new(std::sync::Mutex::new(Vec::new()));
        budget.observe_losses({
            let (lost, observed, budget) =
                (Arc::clone(&lost), Arc::clone(&observed), budget.clone());
            move |rows| {
                let metrics = budget.metrics();
                observed
                    .lock()
                    .expect("observation lock")
                    .push((metrics.owned_bytes, metrics.retry_entries));
                lost.fetch_add(rows, Ordering::AcqRel);
            }
        });
        let producer = Producer::with_budget(
            "vala.gateway.calls",
            schema(),
            sink.clone(),
            QueueConfig::default(),
            budget.clone(),
        )
        .expect("producer starts");
        let settle = |expected: u64| {
            wait_until("lost rows settle", || {
                lost.load(Ordering::Acquire) >= expected
            });
        };

        sink.terminal_next(1);
        producer
            .enqueue_batch(nested_batch(), None)
            .expect("fresh batch admitted");
        settle(2);
        sink.fail_next(1);
        sink.terminal_next(1);
        producer
            .enqueue_batch(nested_batch(), None)
            .expect("retained batch admitted");
        settle(4);
        assert_eq!(
            *observed.lock().expect("observation lock"),
            vec![(0, 0), (0, 0)],
            "each observer sees the lost owner's bytes and retry entry released"
        );

        let deferred = producer
            .shutdown()
            .expect_err("the drained producer reports its unreported background refusal");
        assert!(
            matches!(deferred, WyrdQueueError::Sink(_)),
            "the first terminal refusal is the one reported: {deferred:?}"
        );
        assert_eq!(lost.load(Ordering::Acquire), 4, "each loss settles once");
        assert_eq!(producer.metrics().dropped, 4);
        assert!(sink.received().is_empty(), "no refused batch publishes");
        let settled = budget.metrics();
        assert_eq!((settled.live_batches, settled.retry_entries), (0, 0));
        assert_eq!(budget.used_bytes(), 0);
    }

    /// Decodes every batch carried by one sink receipt.
    fn decode(bytes: &[u8]) -> Vec<RecordBatch> {
        StreamReader::try_new(std::io::Cursor::new(bytes), None)
            .expect("receipt is an IPC stream")
            .map(|batch| batch.expect("receipt batch decodes"))
            .collect()
    }

    /// An owned Arrow batch is sealed by the background owner under one stable retried identity.
    #[test]
    fn arrow_batch_enqueue_seals_nested_columns_through_the_retry_owner() {
        let sink = Arc::new(MockSink::new());
        sink.fail_next(1);
        let budget = ClientByteBudget::new(QueueConfig::DEFAULT_CLIENT_BYTE_LIMIT);
        let producer = Producer::with_budget(
            "vala.gateway.calls",
            schema(),
            sink.clone(),
            QueueConfig::default(),
            budget.clone(),
        )
        .expect("producer starts");
        let batch = nested_batch();
        let request_id = RequestId::now_v7();
        producer
            .enqueue_batch(batch.clone(), Some(request_id.clone()))
            .expect("nested batch admitted");
        wait_until("retained batch settles on retry", || {
            !sink.received().is_empty()
        });
        let attempts = sink.attempted();
        assert_eq!(attempts.len(), 2, "one ambiguity then one resolving retry");
        assert_eq!(attempts[0], attempts[1], "retry keeps the sealed UUIDv7");
        let receipt = &sink.received()[0];
        assert_eq!(receipt.table, "vala.gateway.calls");
        assert_eq!(receipt.rows, 2);
        assert_eq!(
            receipt.request_id,
            Some(request_id),
            "the retry keeps the request"
        );
        assert_eq!(decode(&receipt.bytes), vec![batch], "columns sent verbatim");
        producer.flush().expect("nothing remains after the ACK");
        assert_eq!(producer.metrics().accepted, 2);
        assert_eq!(producer.metrics().dropped, 0);
        producer.shutdown().expect("settled producer drains");
        assert_eq!(budget.used_bytes(), 0, "every batch reservation settles");
    }

    /// Admission returns at once while a sink stalls, and refuses at once when bytes are exhausted.
    #[test]
    fn arrow_batch_enqueue_never_waits_for_publication() {
        let sink = Arc::new(TimeoutSink::default());
        let budget = ClientByteBudget::new(QueueConfig::DEFAULT_CLIENT_BYTE_LIMIT);
        let producer = Producer::with_budget(
            "vala.traces.spans",
            schema(),
            sink.clone(),
            QueueConfig {
                flush_timeout_ms: 10_000,
                ..QueueConfig::default()
            },
            budget.clone(),
        )
        .expect("producer starts");
        producer
            .enqueue_batch(nested_batch(), None)
            .expect("first batch admitted");
        wait_until("background owner starts sending", || sink.attempted() == 1);
        let started = Instant::now();
        producer
            .enqueue_batch(nested_batch(), None)
            .expect("second batch admitted while the sink stalls");
        assert!(
            started.elapsed() < Duration::from_millis(50),
            "admission never awaits publication"
        );
        wait_until("intake seals the second batch", || {
            budget.metrics().live_batches == 2
        });
        let blocker = budget
            .reserve(budget.admission_limit() - budget.used_bytes())
            .expect("test occupies the remaining byte owner");
        assert!(matches!(
            producer.enqueue_batch(nested_batch(), None),
            Err(WyrdQueueError::Backpressure)
        ));
        assert_eq!(producer.metrics().dropped, 2, "refused rows are counted");
        drop(blocker);
        sink.release();
        producer
            .shutdown()
            .expect("released sink drains both batches");
        assert_eq!(sink.attempted(), 2);
        assert_eq!(budget.used_bytes(), 0, "shutdown settles every reservation");
    }

    /// While sends stall, at most `max_in_flight` run at once and intake
    /// keeps admitting far more rows than any fixed slot count. A due linger
    /// does not seal while every slot is busy, so the rows admitted behind
    /// the stalled sends accumulate and leave as one batch on release.
    #[test]
    fn stalled_sends_bound_concurrency_and_grow_the_next_batch() {
        let sink = Arc::new(TimeoutSink::default());
        let budget = ClientByteBudget::new(QueueConfig::DEFAULT_CLIENT_BYTE_LIMIT);
        let producer = Producer::with_budget(
            "vala.bifrost.in_flight_intake",
            schema(),
            sink.clone(),
            QueueConfig {
                linger_ms: 0,
                max_in_flight: 2,
                flush_timeout_ms: 10_000,
                ..QueueConfig::default()
            },
            budget.clone(),
        )
        .expect("producer starts");
        for record in 0..2 {
            producer
                .enqueue_rows(rows(1), Some(card()), None)
                .unwrap_or_else(|error| panic!("record {record} admitted: {error}"));
            wait_until("each record seals while a slot is free", || {
                sink.attempted() == record + 1
            });
        }
        for _ in 0..1_000 {
            producer
                .enqueue_rows(rows(1), Some(card()), None)
                .expect("admission never waits on stalled sends");
        }
        wait_until("rows behind busy slots stay staged", || {
            producer.counters.staged.load(Ordering::Acquire) == 1_000
        });
        assert_eq!(
            budget.metrics().live_batches,
            2,
            "nothing sealed while slots are busy"
        );
        sink.release();
        producer
            .shutdown()
            .expect("released sink drains every batch");
        assert_eq!(sink.attempted(), 3, "the staged rows leave as one batch");
        assert_eq!(producer.metrics().accepted, 1_002);
        assert_eq!(producer.metrics().dropped, 0);
        assert_eq!(
            sink.peak.load(Ordering::Acquire),
            2,
            "never above max_in_flight"
        );
        assert_eq!(budget.used_bytes(), 0, "shutdown settles every reservation");
    }

    /// A complete record is prepared before admission: when only its final
    /// row is unstorable, the exact first refusal names that row, and queue
    /// depth, budget, counters, and the sink stay untouched. Dropping a
    /// prepared record before hand-over (cancellation) changes nothing; once
    /// handed over, the record settles whole through the ordinary flush.
    #[test]
    fn prepared_rows_reject_atomically_before_reservation() {
        let config = QueueConfig {
            linger_ms: 60_000,
            ..QueueConfig::default()
        };
        let budget = ClientByteBudget::for_config(&config);
        let sink = Arc::new(MockSink::new());
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            crate::variant::variant_field("payload", true),
        ]));
        let producer = Producer::with_budget(
            "vala.bifrost.prepared",
            Arc::clone(&schema),
            sink.clone(),
            config,
            budget.clone(),
        )
        .expect("producer starts");
        let untouched = |case: &str| {
            assert_eq!(
                producer.metrics(),
                ProducerMetrics {
                    accepted: 0,
                    dropped: 0,
                    queue_depth: 0
                },
                "{case}"
            );
            assert_eq!(budget.used_bytes(), 0, "{case}");
        };
        let deep = format!("{}{}", "[".repeat(65), "]".repeat(65));
        let large = format!(r#""{}""#, "x".repeat(8 * 1024 * 1024));
        let cases = [
            (r#"{"id":2,"extra":1}"#.to_owned(), "undeclared"),
            (format!(r#"{{"id":2,"payload":{deep}}}"#), "deep"),
            (format!(r#"{{"id":2,"payload":{large}}}"#), "large"),
            (
                r#"{"id":2,"payload":99999999999999999999999}"#.to_owned(),
                "range",
            ),
            (
                format!(r#"{{"id":2,"extra":1,"payload":{deep}}}"#),
                "undeclared",
            ),
        ];
        for (last, expected) in cases {
            let rows = vec![
                br#"{"id":0,"payload":{"ok":true}}"#.to_vec(),
                br#"{"id":1,"payload":"text"}"#.to_vec(),
                last.into_bytes(),
            ];
            let error = producer
                .enqueue_rows(rows, Some(card()), None)
                .expect_err("the final row refuses the whole record");
            let matched = match (&error, expected) {
                (
                    WyrdQueueError::Contract(BifrostError::UndeclaredField { field, row: 2 }),
                    "undeclared",
                ) => field == "extra",
                (
                    WyrdQueueError::Contract(BifrostError::VariantTooDeep {
                        field, row: 2, ..
                    }),
                    "deep",
                )
                | (
                    WyrdQueueError::Contract(BifrostError::VariantTooLarge {
                        field, row: 2, ..
                    }),
                    "large",
                )
                | (
                    WyrdQueueError::Contract(BifrostError::VariantNumericOutOfRange {
                        field,
                        row: 2,
                        ..
                    }),
                    "range",
                ) => field == "payload",
                _ => false,
            };
            assert!(matched, "{expected}: {error:?}");
            untouched(expected);
        }

        let preflight = RowPreflight::new(&schema);
        let rows = [br#"{"id":0}"#, br#"{"id":1}"#];
        drop(preflight.prepare(&rows, None, None).expect("rows prepare"));
        untouched("cancelled before hand-over");
        producer
            .enqueue_prepared(preflight.prepare(&rows, None, None).expect("rows prepare"))
            .expect("the record is admitted whole");
        assert_eq!(producer.metrics().accepted, 2);
        producer.flush().expect("the admitted record settles");
        assert_eq!(received_rows(&sink), 2);
        assert_eq!(sink.received().len(), 1, "one record, one batch");
        producer.shutdown().expect("nothing remains");
        assert_eq!(budget.used_bytes(), 0);
    }

    /// A multi-row record is admitted whole or not at all: a record larger
    /// than the whole admission budget is refused as too large, a byte
    /// refusal releases every row's reservation, and the sink receives
    /// exactly the admitted rows.
    #[test]
    fn multi_row_admission_is_all_or_none() {
        let config = QueueConfig {
            client_byte_limit_bytes: 16 * 1024,
            max_message_bytes: 4 * 1024,
            ..QueueConfig::default()
        };
        let budget = ClientByteBudget::for_config(&config);
        let sink = Arc::new(MockSink::new());
        let producer = Producer::with_budget(
            "vala.bifrost.atomic_admission",
            schema(),
            sink.clone(),
            config,
            budget.clone(),
        )
        .expect("producer starts");
        let oversized = rows(budget.admission_limit() / std::mem::size_of::<Row>() + 1);
        let oversized_count = oversized.len() as u64;
        assert!(charge(&oversized) > budget.admission_limit());
        assert!(matches!(
            producer.enqueue_rows(oversized, Some(card()), None),
            Err(WyrdQueueError::PayloadTooLarge)
        ));
        let refused = rows(9);
        let blocker = budget
            .reserve(budget.admission_limit() - charge(&refused) + 1)
            .expect("test leaves room for fewer than nine rows");
        let blocked = budget.used_bytes();
        assert!(matches!(
            producer.enqueue_rows(refused, Some(card()), None),
            Err(WyrdQueueError::Backpressure)
        ));
        assert_eq!(budget.used_bytes(), blocked, "no row kept its bytes");
        drop(blocker);
        assert_eq!(
            producer.metrics().accepted,
            0,
            "refused records admit no prefix"
        );
        assert_eq!(producer.metrics().dropped, oversized_count + 9);
        producer
            .enqueue_rows(rows(9), Some(card()), None)
            .expect("a record that fits is admitted whole");
        producer.shutdown().expect("admitted rows drain");
        assert_eq!(received_rows(&sink), 9, "exactly the admitted rows publish");
        assert_eq!(budget.used_bytes(), 0);
    }

    /// Staged rows seal on their own, with no flush, once their JSON bytes
    /// reach the message ceiling, and once the linger elapses.
    #[test]
    fn byte_and_linger_triggers_seal_without_a_flush() {
        let sink = Arc::new(MockSink::new());
        let by_bytes = Producer::new(
            "vala.bifrost.byte_trigger",
            schema(),
            sink.clone(),
            QueueConfig {
                linger_ms: 60_000,
                max_message_bytes: 2 * 1024,
                ..QueueConfig::default()
            },
        )
        .expect("producer starts");
        let batch = rows(300);
        let count = batch.len() as u64;
        assert!(batch.iter().map(Vec::len).sum::<usize>() >= 2 * 1024);
        by_bytes
            .enqueue_rows(batch, None, None)
            .expect("record admitted");
        wait_until("staging reaching the byte ceiling seals", || {
            received_rows(&sink) == count
        });
        let by_linger = Producer::new(
            "vala.bifrost.linger_trigger",
            schema(),
            sink.clone(),
            QueueConfig {
                linger_ms: 20,
                ..QueueConfig::default()
            },
        )
        .expect("producer starts");
        by_linger
            .enqueue(br#"{"id":1}"#.to_vec(), None, None)
            .expect("row admitted");
        wait_until("an elapsed linger seals", || {
            received_rows(&sink) == count + 1
        });
        by_bytes.shutdown().expect("nothing remains");
        by_linger.shutdown().expect("nothing remains");
    }

    /// Admitted rows cannot take the sealing headroom: with the admission
    /// ceiling exhausted, a flush still reserves its frame and publishes the
    /// staged rows.
    #[test]
    fn admitted_rows_cannot_take_the_sealing_headroom() {
        let config = QueueConfig {
            linger_ms: 60_000,
            ..QueueConfig::default()
        };
        let budget = ClientByteBudget::for_config(&config);
        let sink = Arc::new(MockSink::new());
        let producer = Producer::with_budget(
            "vala.bifrost.sealing_headroom",
            schema(),
            sink.clone(),
            config,
            budget.clone(),
        )
        .expect("producer starts");
        let row = rows(1);
        let blocker = budget
            .reserve(budget.admission_limit() - budget.used_bytes() - charge(&row))
            .expect("test exhausts admission but for one row");
        producer
            .enqueue_rows(row, Some(card()), None)
            .expect("the last admissible row is admitted");
        assert!(matches!(
            budget.reserve(1),
            Err(WyrdQueueError::Backpressure)
        ));
        producer
            .flush()
            .expect("the frame comes from the sealing headroom");
        assert_eq!(received_rows(&sink), 1);
        drop(blocker);
        producer.shutdown().expect("nothing remains");
    }
}
