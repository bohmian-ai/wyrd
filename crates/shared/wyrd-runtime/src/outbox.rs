//! Generic non-blocking, per-tenant batched outbox.
//!
//! A server surface that must record derived work without delaying the request
//! that produced it hands each item to [`Outbox::stage`], which only enqueues
//! it. One background writer per outbox groups queued items by tenant and by
//! the sink's [`OutboxSink::lane`], and writes each lane's whole backlog
//! through its [`OutboxSink`] in one all-or-nothing call. Each lane has at
//! most one write in flight, so items of one lane are written in arrival
//! order; a tenant's lanes write concurrently. At most `concurrency` writes
//! run at once, and the tenant with the fewest writes in flight is
//! dispatched first, so a busy tenant cannot hold every slot while another
//! waits, and a slow tenant delays only its own items.
//!
//! A sink may set [`OutboxSink::LINGER`], as a Kafka producer sets
//! `linger.ms`: a lane's backlog then waits until its oldest item is that old,
//! or until it holds [`OutboxSink::BATCH_ITEMS`] items or
//! [`OutboxSink::BATCH_BYTES`] accounted bytes, before it is written, so items
//! staged one at a time reach the sink as one write instead of one write each.
//! One write takes at most [`OutboxSink::BATCH_BYTES`] of a lane's backlog, so
//! a large backlog is written, and its capacity released, a bounded slice at a
//! time. Linger delays only when an item is written, never its
//! producer, and a retry or a closed outbox writes at once.
//!
//! A failed write is never dropped. A write that returns an error or panics
//! keeps its exact items as that lane's next write, apart from anything that
//! arrived meanwhile, and the lane is retried after a backoff that starts at
//! [`INITIAL_BACKOFF`] and doubles up to [`MAX_BACKOFF`]; other lanes keep
//! writing. Later arrivals wait for the write after the retry succeeds, and
//! every attempt of a slice carries the key the slice was given when it was
//! first written, so a sink deriving identity from that key presents the same
//! identity on every attempt. Because a write may be retried after an unknown
//! commit outcome, every sink write must be safe to repeat.
//!
//! Every outbox has finite [`OutboxLimits`]: a most admitted items and a
//! most admitted bytes, as each item's [`OutboxSink::weight`]. Admitted
//! items are those queued, waiting, retrying, or being written, plus slots
//! reserved and not yet staged. An item stays admitted until it is written
//! or lost, never merely dequeued, and a retried slice is admitted once
//! however often it is retried. Both limits are split in two shares. A
//! producer that must refuse its caller rather than lose an item takes a
//! [`Reservation`] first; [`Outbox::reserve`] refuses with [`OutboxFull`]
//! once reserved slots and the unwritten items staged through them would
//! exceed [`RESERVED_SHARE`] of the item limit, or the bytes of those items
//! already reach that share of the byte limit. Unreserved items from
//! [`Outbox::stage`] hold the rest of both, so neither share can crowd out
//! the other. [`Outbox::reserve_until`] instead waits for capacity in arrival
//! order, woken whenever admitted items are released. `stage` never fails or
//! waits: past either unreserved limit an
//! item is shed, counted lost, and logged at most once per tenant every
//! [`SHED_LOG_INTERVAL`] with the count shed since the last record, so a
//! full outbox never blocks or fails the operation staging it.
//!
//! Nothing is preallocated. Every lost item is counted once in
//! `outbox_events_lost_total{outbox, reason}` with its [`LossReason`]: shed at
//! an unreserved limit, staged after shutdown began or unwritten at its
//! deadline, or consumed unwritten by the sink. Items lost when the process
//! stops abruptly are never counted. Written items and their bytes are
//! counted in `outbox_items_written_total{outbox}` and
//! `outbox_bytes_written_total{outbox}`, every failed write attempt in
//! `outbox_write_failures_total{outbox}`, and the items not yet written are
//! exported as the `outbox_pending{outbox}` gauge.

use std::any::Any;
use std::collections::{HashMap, VecDeque};
use std::fmt::Display;
use std::future::{Future, pending, poll_fn};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::pin;
use std::sync::atomic::{AtomicUsize, Ordering, fence};
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::task::Poll;
use std::time::{Duration, Instant};

use tokio::sync::{Notify, mpsc};
use tokio::task::{Id, JoinError, JoinSet};
use tokio::time::{sleep_until, timeout_at};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use uuid::Uuid;
use wyrd_spec::DataTenantId;

/// Delay before the first retry of a lane whose write failed.
pub const INITIAL_BACKOFF: Duration = Duration::from_millis(50);

/// Longest delay between retries of a tenant whose writes keep failing.
pub const MAX_BACKOFF: Duration = Duration::from_secs(5);

/// Share of an outbox's capacity, as a fraction `(numerator, denominator)`,
/// that reservations may hold; the rest is unreserved staging's, so a
/// refusing producer's burst cannot crowd out audit decisions and their
/// traffic cannot refuse the producer.
pub const RESERVED_SHARE: (usize, usize) = (3, 4);

/// Shortest time between two shed records of one outbox and tenant.
///
/// A saturated producer sheds every item it stages; one record per interval
/// carrying the count keeps the log readable and its cost flat.
pub const SHED_LOG_INTERVAL: Duration = Duration::from_secs(1);

/// The most an [`Outbox`] admits at once, by item count and by accounted
/// bytes; reaching either limit of a share sheds or refuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutboxLimits {
    /// Most items admitted, including unspent reserved slots.
    pub items: usize,
    /// Most bytes admitted, as the sum of [`OutboxSink::weight`] over the
    /// admitted items.
    pub bytes: usize,
}

/// Why an outbox item was lost, the `reason` label of
/// `outbox_events_lost_total`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LossReason {
    /// Refused at an unreserved staging limit.
    Shed,
    /// Consumed by the sink without being written: a terminal refusal or an
    /// item that could not be encoded, which no retry could cure.
    Rejected,
    /// The process reaches no destination to write to.
    Unreachable,
    /// Staged after shutdown began, or still unwritten at its deadline.
    Shutdown,
}

impl LossReason {
    /// The fixed metric label value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Shed => "shed",
            Self::Rejected => "rejected",
            Self::Unreachable => "unreachable",
            Self::Shutdown => "shutdown",
        }
    }
}

/// Refusal of a [`Reservation`] because the outbox holds its reserved share.
///
/// Nothing was admitted, so the caller may retry the whole operation later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("outbox {outbox} is full: {admitted} of {limit} reservable {unit} admitted")]
pub struct OutboxFull {
    /// The refusing outbox's [`OutboxSink::NAME`].
    pub outbox: &'static str,
    /// Reserved slots and unwritten reserved items, or the bytes of those
    /// items, when the reservation was refused.
    pub admitted: usize,
    /// Items or bytes reservations may hold in this outbox.
    pub limit: usize,
    /// Which limit refused: `"items"` or `"bytes"`.
    pub unit: &'static str,
}

/// One destination an outbox writes to.
///
/// Implementations write everything one lane of a tenant has queued in one
/// call: in one transaction, or as idempotent writes whose repeat is
/// absorbed. The outbox owns queueing, grouping, concurrency, retry, and
/// shutdown.
pub trait OutboxSink: Send + Sync + 'static {
    /// The item callers stage.
    type Item: Send + 'static;
    /// The failure a write reports; logged, counted, and retried.
    type Error: Display + Send + 'static;
    /// Label for this outbox's metrics and logs.
    const NAME: &'static str;
    /// Lanes each tenant's items are spread over. Lanes of one tenant write
    /// concurrently; the default single lane writes a tenant one slice at a
    /// time.
    const LANES: usize = 1;
    /// Longest a lane's oldest waiting item waits for more to join its write,
    /// as Kafka's `linger.ms`. The default of zero writes as soon as a lane is
    /// free.
    const LINGER: Duration = Duration::ZERO;
    /// Waiting items at which a lane writes without lingering further, as
    /// Kafka's `batch.size`.
    const BATCH_ITEMS: usize = usize::MAX;
    /// Accounted bytes, as [`Self::weight`], at which a lane writes without
    /// lingering further, and the most one write takes of its backlog, as
    /// Kafka's `batch.size`. An item heavier than this is written alone. The
    /// rest of the backlog stays queued in order and is written next, so a
    /// slice's capacity returns to producers when that slice is written, not
    /// when the whole backlog is.
    const BATCH_BYTES: usize = usize::MAX;

    /// The lane `item` is written in, taken modulo [`Self::LANES`].
    ///
    /// Items of one tenant and lane are written in arrival order, one write
    /// at a time, so items whose writes must observe each other in arrival
    /// order, such as requests sharing an idempotency key, must share a lane.
    #[must_use]
    fn lane(item: &Self::Item) -> u64 {
        let _ = item;
        0
    }

    /// The bytes `item` is accounted as against [`OutboxLimits::bytes`].
    ///
    /// At least the length of every payload the item holds, such as captured
    /// request and response bodies; encoding overhead need not be counted.
    /// The default of zero suits items bounded by their producer's input
    /// limits, which the item limit then bounds.
    #[must_use]
    fn weight(item: &Self::Item) -> usize {
        let _ = item;
        0
    }

    /// Writes one lane's items of one tenant, in order, in one transaction.
    ///
    /// A write may be retried after its commit outcome is unknown, so writing
    /// the same items twice must leave the same durable state as writing them
    /// once. `slice` is the slice's idempotency key: a `UUIDv7` assigned when
    /// the slice is first written and passed again with every retry of the
    /// identical items, so a sink derives durable identities from it without
    /// reading the items.
    ///
    /// # Errors
    ///
    /// Returns the sink's error when any part of the write fails; nothing of
    /// the batch may then be durable unless a repeat would skip it.
    fn write(
        &self,
        tenant: DataTenantId,
        slice: Uuid,
        items: &[Self::Item],
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

/// One queued item: its tenant, the item, and its admitted [`Load`].
type Queued<S> = (DataTenantId, <S as OutboxSink>::Item, Load);

/// Sending half of an outbox queue.
type QueueSender<S> = mpsc::UnboundedSender<Queued<S>>;

/// The handle callers stage through; owns the queue and its writer task.
pub struct Outbox<S: OutboxSink> {
    /// Sending half of the unbounded queue the writer drains, taken by
    /// shutdown. `stage` sends under the read lock and shutdown takes it under
    /// the write lock, so every item is either queued before the fence and
    /// drained, or refused after it.
    queue: RwLock<Option<QueueSender<S>>>,
    /// Items queued, waiting, or being written, not yet written or lost.
    pending: Arc<AtomicUsize>,
    /// Admission counts the writer releases written and lost items from.
    admission: Arc<Admission>,
    /// Most items and bytes this outbox admits at once.
    limits: OutboxLimits,
    /// Per tenant, items shed since the last shed record and when that record
    /// was written; see [`SHED_LOG_INTERVAL`].
    // ponytail: one small entry per tenant that ever shed, never pruned;
    // prune idle entries if tenant counts reach the millions.
    shed: Mutex<HashMap<DataTenantId, ShedLog>>,
    /// Signalled whenever a settled write or abandonment leaves nothing
    /// pending.
    idle: Arc<Notify>,
    /// [`Self::reserve_until`] callers waiting for capacity, in arrival
    /// order.
    waiters: Mutex<Waiters>,
    /// Asks the writer to give up at the shutdown deadline.
    abandon: CancellationToken,
    /// Tracks the writer so shutdown can wait for it.
    writer: TaskTracker,
    /// The sink the writer submits through, shared for inspection.
    sink: Arc<S>,
}

/// Names the outbox and its pending count, so values carrying a handle — a
/// staged write that hands off to a second outbox — stay debuggable.
impl<S: OutboxSink> std::fmt::Debug for Outbox<S> {
    /// Write the sink's name and a relaxed snapshot of the pending count;
    /// queued items and the sink itself are never formatted.
    ///
    /// # Errors
    /// Returns the formatter's error when it rejects a write.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Outbox")
            .field("name", &S::NAME)
            .field("pending", &self.pending.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl<S: OutboxSink> Outbox<S> {
    /// Creates the outbox around `sink` and starts its writer.
    ///
    /// `concurrency` is the most writes run at once across every tenant and
    /// lane, which bounds the connections the sink may hold; `limits` are the
    /// most items and bytes admitted at once, which bound the memory queued,
    /// waiting, retrying, and in-flight work may hold.
    ///
    /// # Panics
    ///
    /// Panics when called outside a Tokio runtime, because the writer task is
    /// spawned immediately.
    #[must_use]
    pub fn new(sink: S, concurrency: usize, limits: OutboxLimits) -> Arc<Self> {
        // The channel is unbounded because `admission` already bounds it.
        let (queue, requests) = mpsc::unbounded_channel();
        let pending = Arc::new(AtomicUsize::new(0));
        let admission = Arc::new(Admission::default());
        let idle = Arc::new(Notify::new());
        let abandon = CancellationToken::new();
        let writer = TaskTracker::new();
        let sink = Arc::new(sink);
        writer.spawn(
            OutboxWriter {
                sink: Arc::clone(&sink),
                requests,
                waiting: HashMap::new(),
                since: HashMap::new(),
                closed: false,
                failed: HashMap::new(),
                writing: JoinSet::new(),
                in_flight: HashMap::new(),
                retry_at: HashMap::new(),
                concurrency: concurrency.max(1),
                pending: Arc::clone(&pending),
                admission: Arc::clone(&admission),
                idle: Arc::clone(&idle),
                abandon: abandon.clone(),
            }
            .run(),
        );
        writer.close();
        Arc::new(Self {
            queue: RwLock::new(Some(queue)),
            pending,
            admission,
            limits,
            shed: Mutex::default(),
            idle,
            waiters: Mutex::default(),
            abandon,
            writer,
            sink,
        })
    }

    /// Borrows the sink this outbox writes through.
    #[must_use]
    pub fn sink(&self) -> &S {
        &self.sink
    }

    /// Queues one item for `tenant` without waiting and never fails.
    ///
    /// The item is admitted and counted pending before it is sent, so the
    /// writer can never release it first. Admission happens under the queue's
    /// read lock, so nothing is admitted once shutdown has taken the queue.
    /// When either unreserved limit would be passed the item is shed: counted
    /// lost with [`LossReason::Shed`] and recorded by [`Self::shed`]. Once
    /// shutdown has begun, or if the writer is gone, the item is counted lost
    /// with [`LossReason::Shutdown`] and logged.
    pub fn stage(&self, tenant: DataTenantId, item: impl Into<S::Item>) {
        let item = item.into();
        let bytes = S::weight(&item);
        let queue = self.queue.read().unwrap_or_else(PoisonError::into_inner);
        let Some(sender) = queue.as_ref() else {
            drop(queue);
            lose_late::<S>(tenant);
            return;
        };
        let admission = &self.admission;
        if admit(
            &admission.unreserved,
            1,
            self.limits.items - self.reservable(),
        )
        .is_err()
        {
            drop(queue);
            self.shed(tenant);
            return;
        }
        let byte_limit = self.limits.bytes - self.reservable_bytes();
        if admit(&admission.unreserved_bytes, bytes, byte_limit).is_err() {
            admission.unreserved.fetch_sub(1, Ordering::AcqRel);
            drop(queue);
            self.shed(tenant);
            return;
        }
        self.enqueue(sender, tenant, item, Load::one(false, bytes));
    }

    /// Counts one shed item of `tenant` and writes a shed record when the
    /// tenant's last one is at least [`SHED_LOG_INTERVAL`] old; the record
    /// carries every item shed since the previous one.
    fn shed(&self, tenant: DataTenantId) {
        count_lost::<S>(1, LossReason::Shed);
        let now = Instant::now();
        let shed = {
            let mut logs = self.shed.lock().unwrap_or_else(PoisonError::into_inner);
            let log = logs.entry(tenant).or_default();
            log.unlogged += 1;
            if log
                .logged_at
                .is_some_and(|at| now.duration_since(at) < SHED_LOG_INTERVAL)
            {
                return;
            }
            log.logged_at = Some(now);
            std::mem::take(&mut log.unlogged)
        };
        self.log_shed(tenant, shed);
    }

    /// Writes one shed record for `shed` items of `tenant`.
    fn log_shed(&self, tenant: DataTenantId, shed: usize) {
        tracing::error!(
            outbox = S::NAME,
            %tenant,
            shed,
            max_items = self.limits.items,
            max_bytes = self.limits.bytes,
            "outbox is full; staged items were shed"
        );
    }

    /// Reserves `slots` items of capacity for a producer that must refuse its
    /// caller instead of losing what it stages.
    ///
    /// The reservation is admitted at once and holds its slots until each is
    /// staged through it or it is dropped, which releases the unused rest. An
    /// item staged through it keeps its slot until it is written or lost.
    /// Unreserved items never count against reservations.
    ///
    /// # Errors
    ///
    /// Returns [`OutboxFull`], admitting nothing, when the reservation would
    /// take reservations past [`RESERVED_SHARE`] of the capacity.
    pub fn reserve(self: &Arc<Self>, slots: usize) -> Result<Reservation<S>, OutboxFull> {
        let byte_limit = self.reservable_bytes();
        let bytes = self.admission.reserved_bytes.load(Ordering::Acquire);
        if bytes >= byte_limit {
            return Err(OutboxFull {
                outbox: S::NAME,
                admitted: bytes,
                limit: byte_limit,
                unit: "bytes",
            });
        }
        let limit = self.reservable();
        admit(&self.admission.reserved, slots, limit).map_err(|admitted| OutboxFull {
            outbox: S::NAME,
            admitted,
            limit,
            unit: "items",
        })?;
        Ok(Reservation {
            outbox: Arc::clone(self),
            slots: AtomicUsize::new(slots),
        })
    }

    /// Reserves `slots` like [`Self::reserve`], waiting until `deadline` for
    /// capacity instead of refusing at once.
    ///
    /// Waiters are admitted in arrival order. Under one short lock a caller
    /// either reserves, when no one is waiting and the reservation fits, or
    /// joins the back of the queue; only a queued caller suspends, so callers
    /// that find capacity never wait on one another. Only the head of the
    /// queue reserves, retrying whenever admitted items or unspent slots are
    /// released, so it never sleeps past freed capacity and no later caller
    /// takes capacity freed for it. A caller that leaves the queue, admitted,
    /// timed out, or cancelled, wakes the next head.
    ///
    /// # Errors
    ///
    /// Returns the latest [`OutboxFull`] the queue's head was refused with,
    /// admitting nothing, when `deadline` passes before the reservation fits.
    pub async fn reserve_until(
        self: &Arc<Self>,
        slots: usize,
        deadline: Instant,
    ) -> Result<Reservation<S>, OutboxFull> {
        let ticket = {
            let mut waiters = self.waiters();
            if waiters.queue.is_empty() {
                match self.reserve(slots) {
                    Ok(reservation) => return Ok(reservation),
                    Err(full) if Instant::now() >= deadline => return Err(full),
                    Err(full) => waiters.refused = Some(full),
                }
            }
            waiters.join(&self.admission)
        };
        let _place = Place {
            outbox: self,
            ticket,
        };
        loop {
            let freed = self.admission.freed.notified();
            tokio::pin!(freed);
            freed.as_mut().enable();
            fence(Ordering::SeqCst);
            let refused = {
                let mut waiters = self.waiters();
                if waiters.queue.front() == Some(&ticket) {
                    match self.reserve(slots) {
                        Ok(reservation) => return Ok(reservation),
                        Err(full) => waiters.refused = Some(full),
                    }
                }
                waiters.refused
            };
            if timeout_at(deadline.into(), freed).await.is_err() {
                return Err(refused.unwrap_or(OutboxFull {
                    outbox: S::NAME,
                    admitted: self.admission.reserved.load(Ordering::Acquire),
                    limit: self.reservable(),
                    unit: "items",
                }));
            }
        }
    }

    /// Locks the [`Self::reserve_until`] queue.
    fn waiters(&self) -> std::sync::MutexGuard<'_, Waiters> {
        self.waiters.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The most slots reservations may hold: [`RESERVED_SHARE`] of the
    /// item limit, rounded down.
    const fn reservable(&self) -> usize {
        self.limits.items * RESERVED_SHARE.0 / RESERVED_SHARE.1
    }

    /// The bytes at which reservations are refused: [`RESERVED_SHARE`] of
    /// the byte limit, rounded down, computed without overflowing.
    const fn reservable_bytes(&self) -> usize {
        self.limits.bytes / RESERVED_SHARE.1 * RESERVED_SHARE.0
    }

    /// Sends one item on a spent reserved slot to the writer.
    ///
    /// Its bytes join the reserved share unconditionally, because the slot
    /// already promised the producer its item is kept; [`Self::reserve`]
    /// refuses new reservations once that share's bytes reach their limit.
    /// Once shutdown has begun the item is released from its slot, counted
    /// lost with [`LossReason::Shutdown`], and logged.
    fn send_reserved(&self, tenant: DataTenantId, item: S::Item) {
        let bytes = S::weight(&item);
        let queue = self.queue.read().unwrap_or_else(PoisonError::into_inner);
        let Some(sender) = queue.as_ref() else {
            drop(queue);
            self.admission.release(Load::one(true, 0));
            lose_late::<S>(tenant);
            return;
        };
        self.admission
            .reserved_bytes
            .fetch_add(bytes, Ordering::AcqRel);
        self.enqueue(sender, tenant, item, Load::one(true, bytes));
    }

    /// Sends one admitted item to the writer, counting it pending first.
    ///
    /// Called under the queue's read lock. If the writer is gone, the item is
    /// released from its shares, counted lost with [`LossReason::Shutdown`],
    /// and logged.
    fn enqueue(&self, sender: &QueueSender<S>, tenant: DataTenantId, item: S::Item, load: Load) {
        let gauge = metrics::gauge!("outbox_pending", "outbox" => S::NAME);
        self.pending.fetch_add(1, Ordering::AcqRel);
        self.admission
            .staged_reserved
            .fetch_add(load.reserved, Ordering::AcqRel);
        gauge.increment(1.0);
        if sender.send((tenant, item, load)).is_ok() {
            return;
        }
        self.pending.fetch_sub(1, Ordering::AcqRel);
        self.admission
            .staged_reserved
            .fetch_sub(load.reserved, Ordering::AcqRel);
        gauge.decrement(1.0);
        self.admission.release(load);
        lose_late::<S>(tenant);
    }

    /// Returns the number of items not yet written.
    #[must_use]
    pub fn pending(&self) -> usize {
        self.pending.load(Ordering::Acquire)
    }

    /// Waits until `deadline` for everything staged so far to be written,
    /// without closing the outbox, and returns how many items remain.
    ///
    /// The outbox keeps accepting items throughout; callers that read the
    /// sink's destination right after staging use this to observe their items.
    pub async fn settle(&self, deadline: Instant) -> usize {
        loop {
            let idle = self.idle.notified();
            tokio::pin!(idle);
            idle.as_mut().enable();
            if self.pending() == 0 {
                return 0;
            }
            if timeout_at(deadline.into(), idle).await.is_err() {
                return self.pending();
            }
        }
    }

    /// Stops taking items, keeps writing and retrying until `deadline`, and
    /// returns how many items were lost at the deadline.
    ///
    /// The staging sender is dropped before the first await, which is a
    /// one-way fence: everything staged before it drains, and every later
    /// [`Outbox::stage`] is refused and counted lost without entering the
    /// queue. Shed counts not yet recorded are recorded at the fence.
    /// Items still unwritten at the deadline are abandoned with the
    /// writer, which cancels any in-flight write; once the writer is gone
    /// they are counted in `outbox_events_lost_total` with
    /// [`LossReason::Shutdown`], logged, and released
    /// from `pending()` and `outbox_pending`, which both end at zero, and
    /// released from their admission shares. When shutdown is called
    /// concurrently, exactly one caller reports the loss and settles both
    /// shares, and the others report zero. Unspent [`Reservation`] slots stay
    /// held until their reservation drops.
    pub async fn shutdown(&self, deadline: Instant) -> usize {
        drop(
            self.queue
                .write()
                .unwrap_or_else(PoisonError::into_inner)
                .take(),
        );
        let unlogged: Vec<_> = self
            .shed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter_mut()
            .filter(|(_, log)| log.unlogged > 0)
            .map(|(tenant, log)| (*tenant, std::mem::take(&mut log.unlogged)))
            .collect();
        for (tenant, shed) in unlogged {
            self.log_shed(tenant, shed);
        }
        if timeout_at(deadline.into(), self.writer.wait())
            .await
            .is_err()
        {
            self.abandon.cancel();
            self.writer.wait().await;
        }
        // With the queue closed and the writer gone nothing else moves these
        // counts, and admission only happens under the queue lock, so the
        // admitted bytes are exactly the abandoned items'. The caller that
        // takes a nonzero `pending` owns its reserved part and settles every
        // share alone.
        let lost = self.pending.swap(0, Ordering::AcqRel);
        if lost == 0 {
            return 0;
        }
        let admission = &self.admission;
        let reserved = admission.staged_reserved.swap(0, Ordering::AcqRel);
        let reserved_bytes = admission.reserved_bytes.swap(0, Ordering::AcqRel);
        let bytes = admission.unreserved_bytes.swap(0, Ordering::AcqRel) + reserved_bytes;
        admission.release(Load {
            items: lost,
            reserved,
            bytes,
            reserved_bytes,
        });
        count_lost::<S>(lost, LossReason::Shutdown);
        metrics::gauge!("outbox_pending", "outbox" => S::NAME).decrement(lost as f64);
        tracing::error!(
            outbox = S::NAME,
            lost,
            "outbox shutdown deadline passed with items unwritten"
        );
        self.idle.notify_waiters();
        lost
    }
}

/// Capacity an [`Outbox`] admitted ahead of staging, from [`Outbox::reserve`].
///
/// Each [`Reservation::stage`] spends one slot; dropping the reservation
/// releases the slots it never spent. A value carrying a reservation, such as
/// a write that hands off to a second outbox, holds that capacity until the
/// value is staged or dropped.
pub struct Reservation<S: OutboxSink> {
    /// The outbox the slots were admitted in.
    outbox: Arc<Outbox<S>>,
    /// Slots admitted and not yet staged.
    slots: AtomicUsize,
}

impl<S: OutboxSink> Reservation<S> {
    /// Stages one item for `tenant` on a reserved slot without waiting.
    ///
    /// With no slot left, the item is staged like [`Outbox::stage`]. Once
    /// shutdown has begun the item is counted lost, as any late stage is.
    pub fn stage(&self, tenant: DataTenantId, item: impl Into<S::Item>) {
        let spent = self
            .slots
            .try_update(Ordering::AcqRel, Ordering::Acquire, |slots| {
                slots.checked_sub(1)
            });
        if spent.is_ok() {
            self.outbox.send_reserved(tenant, item.into());
        } else {
            self.outbox.stage(tenant, item);
        }
    }
}

/// Releases the slots this reservation never spent.
impl<S: OutboxSink> Drop for Reservation<S> {
    /// Returns the unspent slots to the reserved share.
    fn drop(&mut self) {
        self.outbox
            .admission
            .reserved
            .fetch_sub(*self.slots.get_mut(), Ordering::AcqRel);
        if *self.slots.get_mut() > 0 {
            self.outbox.admission.wake();
        }
    }
}

/// Names the outbox and the unspent slots, never the outbox's contents.
impl<S: OutboxSink> std::fmt::Debug for Reservation<S> {
    /// Write the sink's name and a relaxed snapshot of the unspent slots.
    ///
    /// # Errors
    /// Returns the formatter's error when it rejects a write.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Reservation")
            .field("outbox", &S::NAME)
            .field("slots", &self.slots.load(Ordering::Relaxed))
            .finish()
    }
}

/// The two shares an [`Outbox`] admits items and bytes into, shared with its
/// writer.
#[derive(Debug, Default)]
struct Admission {
    /// Unreserved items not yet written or lost.
    unreserved: AtomicUsize,
    /// Unspent reserved slots plus reserved items not yet written or lost.
    reserved: AtomicUsize,
    /// Reserved items pending in the writer, which shutdown releases from
    /// `reserved` when it abandons them.
    staged_reserved: AtomicUsize,
    /// Bytes of unreserved items not yet written or lost.
    unreserved_bytes: AtomicUsize,
    /// Bytes of reserved items not yet written or lost; unspent slots hold
    /// none.
    reserved_bytes: AtomicUsize,
    /// Signalled whenever items or slots are released while a caller
    /// waits, waking the first [`Outbox::reserve_until`] waiter.
    freed: Notify,
    /// Length of the [`Outbox::reserve_until`] queue; zero lets releases
    /// skip [`Self::freed`].
    waiting: AtomicUsize,
}

impl Admission {
    /// Releases written or lost items from their shares.
    fn release(&self, load: Load) {
        self.reserved.fetch_sub(load.reserved, Ordering::AcqRel);
        self.unreserved
            .fetch_sub(load.items - load.reserved, Ordering::AcqRel);
        self.reserved_bytes
            .fetch_sub(load.reserved_bytes, Ordering::AcqRel);
        self.unreserved_bytes
            .fetch_sub(load.bytes - load.reserved_bytes, Ordering::AcqRel);
        self.wake();
    }

    /// Wakes the first [`Outbox::reserve_until`] waiter after a release, if
    /// any caller waits.
    ///
    /// The fence pairs with the one a waiter takes between registering for
    /// [`Self::freed`] and retrying, so either the waiter sees the released
    /// capacity or this sees the waiter.
    fn wake(&self) {
        fence(Ordering::SeqCst);
        if self.waiting.load(Ordering::SeqCst) > 0 {
            self.freed.notify_waiters();
        }
    }
}

/// The [`Outbox::reserve_until`] queue.
#[derive(Debug, Default)]
struct Waiters {
    /// Ticket the next caller to join takes.
    next: u64,
    /// Tickets of the waiting callers, head first.
    queue: VecDeque<u64>,
    /// The latest refusal of a caller that joined or headed the queue.
    refused: Option<OutboxFull>,
}

impl Waiters {
    /// Adds a caller to the back of the queue and returns its ticket.
    fn join(&mut self, admission: &Admission) -> u64 {
        let ticket = self.next;
        self.next += 1;
        self.queue.push_back(ticket);
        admission.waiting.store(self.queue.len(), Ordering::SeqCst);
        ticket
    }
}

/// One caller's place in the [`Outbox::reserve_until`] queue, left when the
/// caller returns or is cancelled.
struct Place<'a, S: OutboxSink> {
    /// The outbox whose queue the caller joined.
    outbox: &'a Outbox<S>,
    /// The caller's ticket.
    ticket: u64,
}

impl<S: OutboxSink> Drop for Place<'_, S> {
    /// Leaves the queue and, when this caller was its head, wakes the next
    /// head to retry.
    fn drop(&mut self) {
        let mut waiters = self.outbox.waiters();
        let head = waiters.queue.front() == Some(&self.ticket);
        waiters.queue.retain(|ticket| *ticket != self.ticket);
        let admission = &self.outbox.admission;
        admission
            .waiting
            .store(waiters.queue.len(), Ordering::SeqCst);
        if waiters.queue.is_empty() {
            waiters.refused = None;
        } else if head {
            admission.freed.notify_waiters();
        }
    }
}

/// What a set of admitted items holds of the [`Admission`] shares.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Load {
    /// Items.
    items: usize,
    /// Items admitted on reserved slots.
    reserved: usize,
    /// Accounted bytes of every item.
    bytes: usize,
    /// Accounted bytes of the reserved items.
    reserved_bytes: usize,
}

impl Load {
    /// The load of one item of `bytes`, `reserved` or not.
    const fn one(reserved: bool, bytes: usize) -> Self {
        Self {
            items: 1,
            reserved: reserved as usize,
            bytes,
            reserved_bytes: if reserved { bytes } else { 0 },
        }
    }

    /// Adds `other` into this load.
    const fn add(&mut self, other: Self) {
        self.items += other.items;
        self.reserved += other.reserved;
        self.bytes += other.bytes;
        self.reserved_bytes += other.reserved_bytes;
    }

    /// Removes `part`, a load added into this one, from it.
    const fn sub(&mut self, part: Self) {
        self.items -= part.items;
        self.reserved -= part.reserved;
        self.bytes -= part.bytes;
        self.reserved_bytes -= part.reserved_bytes;
    }
}

/// When a tenant's last shed record was written and how many items it has
/// shed since.
#[derive(Debug, Default)]
struct ShedLog {
    /// When the last record was written; `None` before the first.
    logged_at: Option<Instant>,
    /// Items shed since that record.
    unlogged: usize,
}

/// Admits `count` items into `share` unless that takes it past `limit`.
///
/// # Errors
///
/// Returns the share's count, admitting nothing, when `count` more would
/// exceed `limit`.
fn admit(share: &AtomicUsize, count: usize, limit: usize) -> Result<(), usize> {
    share
        .try_update(Ordering::AcqRel, Ordering::Acquire, |admitted| {
            admitted.checked_add(count).filter(|total| *total <= limit)
        })
        .map(|_| ())
}

/// One lane's items in arrival order and their admitted [`Load`].
type Backlog<I> = (Vec<I>, Load);

/// One lane's received items in arrival order, each with its own admitted
/// [`Load`] so a bounded prefix can be written, and their total.
type Waiting<I> = (Vec<(I, Load)>, Load);

/// A tenant and one of its [`OutboxSink::LANES`]: the unit written in arrival
/// order, one write at a time.
type Lane = (DataTenantId, usize);

/// Outcome of one spawned lane write: the lane, the slice's key, its items,
/// and the result, whose error is the sink's error or the message of a
/// contained panic. The items come back only from a failed write, for its
/// retry under the same key; a written slice's items are freed off the writer
/// loop and come back empty.
type Written<S> = (Lane, Uuid, Vec<<S as OutboxSink>::Item>, Result<(), String>);

/// The writer task behind one [`Outbox`].
///
/// Moved into its task by [`Outbox::new`]; it owns the receiving half of the
/// queue, the per-lane backlogs, the in-flight writes, and the retry state.
struct OutboxWriter<S: OutboxSink> {
    /// Destination every write goes to.
    sink: Arc<S>,
    /// Receiving half of the queue.
    requests: mpsc::UnboundedReceiver<Queued<S>>,
    /// Received items per lane, in arrival order, not being written.
    waiting: HashMap<Lane, Waiting<S::Item>>,
    /// When each waiting lane's oldest item arrived, which starts its
    /// [`OutboxSink::LINGER`].
    since: HashMap<Lane, Instant>,
    /// Whether the queue has closed, after which lanes write without
    /// lingering so shutdown drains promptly.
    closed: bool,
    /// The key and exact slice of each lane whose last write failed; it is
    /// that lane's next write, unmerged with `waiting`.
    failed: HashMap<Lane, (Uuid, Backlog<S::Item>)>,
    /// In-flight lane writes.
    writing: JoinSet<Written<S>>,
    /// Lane, admitted load, and dispatch time of each in-flight write, keyed
    /// by its task.
    in_flight: HashMap<Id, (Lane, Load, Instant)>,
    /// Earliest retry time and current backoff of each lane whose last
    /// write failed.
    retry_at: HashMap<Lane, (Instant, Duration)>,
    /// Most writes run at once.
    concurrency: usize,
    /// Count shared with the handle; decremented once items are written.
    pending: Arc<AtomicUsize>,
    /// Admission shares shared with the handle; released with `pending`.
    admission: Arc<Admission>,
    /// Woken whenever a settled write leaves nothing pending.
    idle: Arc<Notify>,
    /// Cancelled when shutdown reaches its deadline.
    abandon: CancellationToken,
}

impl<S: OutboxSink> OutboxWriter<S> {
    /// The writer loop: receives items, dispatches lane writes, settles
    /// finished ones, and wakes for retries, until shutdown has dropped the
    /// sender and nothing is left, or shutdown abandons it at the deadline.
    ///
    /// Abandoning returns at once; dropping the writer cancels its in-flight
    /// writes and discards their items, which shutdown then counts lost.
    async fn run(mut self) {
        let mut received = Vec::new();
        let mut open = true;
        loop {
            self.dispatch();
            if !open && self.waiting.is_empty() && self.failed.is_empty() && self.writing.is_empty()
            {
                return;
            }
            let retry = self.next_wake();
            tokio::select! {
                biased;
                () = self.abandon.cancelled() => return,
                Some(done) = self.writing.join_next_with_id(), if !self.writing.is_empty() => {
                    self.finish(done);
                }
                count = self.requests.recv_many(&mut received, usize::MAX), if open => {
                    open = count != 0;
                    self.closed = !open;
                    let lanes = S::LANES.max(1) as u64;
                    let now = Instant::now();
                    for (tenant, item, load) in received.drain(..) {
                        // The remainder is below `LANES`, a `usize`.
                        let lane = usize::try_from(S::lane(&item) % lanes).unwrap_or_default();
                        self.since.entry((tenant, lane)).or_insert(now);
                        let (items, backlog) = self.waiting.entry((tenant, lane)).or_default();
                        items.push((item, load));
                        backlog.add(load);
                    }
                }
                () = async {
                    match retry {
                        Some(at) => sleep_until(at.into()).await,
                        None => pending().await,
                    }
                } => {}
            }
        }
    }

    /// Starts one write for each lane that has items waiting, has no write
    /// in flight, is not backing off, and has lingered (see
    /// [`Self::is_ready`]), while fewer than `concurrency` writes
    /// run. Ready lanes are taken in order of their tenant's writes in flight
    /// plus the lane's rank among that tenant's ready lanes, so every tenant's
    /// first lane goes before any tenant's second. A lane with a failed slice
    /// writes exactly that slice again; otherwise a write takes the lane's
    /// backlog up to [`OutboxSink::BATCH_BYTES`] (see [`Self::take`]).
    ///
    /// The write task keeps its items through a panicking sink, including a
    /// panic while rendering the sink's error, and returns them with the panic
    /// as the error, so they are retried like any failed write.
    fn dispatch(&mut self) {
        if self.writing.len() >= self.concurrency {
            return;
        }
        let now = Instant::now();
        let mut busy: HashMap<DataTenantId, usize> = HashMap::new();
        for ((tenant, _), ..) in self.in_flight.values() {
            *busy.entry(*tenant).or_default() += 1;
        }
        let mut ready: Vec<(usize, Lane)> = self
            .waiting
            .keys()
            .chain(
                self.failed
                    .keys()
                    .filter(|lane| !self.waiting.contains_key(*lane)),
            )
            .filter(|lane| self.is_ready(**lane, now))
            .map(|lane| {
                let order = busy.entry(lane.0).or_default();
                *order += 1;
                (*order, *lane)
            })
            .collect();
        ready.sort_unstable_by_key(|(order, _)| *order);
        for (_, lane) in ready {
            if self.writing.len() >= self.concurrency {
                return;
            }
            let Some((slice, (items, load))) = self
                .failed
                .remove(&lane)
                .or_else(|| self.take(lane).map(|backlog| (Uuid::now_v7(), backlog)))
            else {
                continue;
            };
            let (tenant, _) = lane;
            let sink = Arc::clone(&self.sink);
            let task = self.writing.spawn(async move {
                // The constructing closure borrows `items`, so it is dropped
                // before awaiting, and the write before `items` is returned.
                let outcome = {
                    let write =
                        catch_unwind(AssertUnwindSafe(|| sink.write(tenant, slice, &items)));
                    match write {
                        Ok(write) => contain_panic(write).await,
                        Err(panic) => Err(panic),
                    }
                };
                // Rendering a sink error runs sink code too, so it is
                // contained as well and `items` is still returned.
                let result = catch_unwind(AssertUnwindSafe(|| match outcome {
                    Ok(written) => written.map_err(|error| error.to_string()),
                    Err(panic) => Err(format!("sink write panicked: {}", panic_message(&*panic))),
                }))
                .unwrap_or_else(|panic| {
                    Err(format!(
                        "sink error rendering panicked: {}",
                        panic_message(&*panic)
                    ))
                });
                if result.is_ok() {
                    // A large slice's items can take as long to free as to
                    // write; freeing them on the blocking pool keeps the
                    // writer loop free to receive and dispatch the next write.
                    drop(tokio::task::spawn_blocking(move || drop(items)));
                    return (lane, slice, Vec::new(), result);
                }
                (lane, slice, items, result)
            });
            self.in_flight
                .insert(task.id(), (lane, load, Instant::now()));
        }
    }

    /// Removes the next slice of `lane`'s backlog: the longest prefix within
    /// [`OutboxSink::BATCH_BYTES`], and at least its first item. A remainder
    /// keeps its place and the lane's linger start, so it is ready to write
    /// as soon as this slice is written.
    fn take(&mut self, lane: Lane) -> Option<Backlog<S::Item>> {
        let (queued, total) = self.waiting.get_mut(&lane)?;
        let mut load = Load::default();
        let count = queued
            .iter()
            .position(|(_, next)| {
                let over = load.items > 0 && load.bytes.saturating_add(next.bytes) > S::BATCH_BYTES;
                if !over {
                    load.add(*next);
                }
                over
            })
            .unwrap_or(queued.len());
        let items = if count == queued.len() {
            self.since.remove(&lane);
            self.waiting.remove(&lane)?.0
        } else {
            total.sub(load);
            queued.drain(..count).collect()
        };
        Some((items.into_iter().map(|(item, _)| item).collect(), load))
    }

    /// Whether `lane` has no write in flight, its backoff has elapsed, and
    /// its next write may go now: a failed slice retries at once, and a
    /// backlog goes once its oldest item has lingered [`OutboxSink::LINGER`],
    /// it holds [`OutboxSink::BATCH_ITEMS`] items or
    /// [`OutboxSink::BATCH_BYTES`] bytes, or the queue has closed.
    fn is_ready(&self, lane: Lane, now: Instant) -> bool {
        !self.in_flight.values().any(|(busy, ..)| *busy == lane)
            && self.retry_at.get(&lane).is_none_or(|(at, _)| *at <= now)
            && (self.closed || self.failed.contains_key(&lane) || self.lingered(lane, now))
    }

    /// Whether `lane`'s backlog has waited [`OutboxSink::LINGER`] or holds
    /// [`OutboxSink::BATCH_ITEMS`] items or [`OutboxSink::BATCH_BYTES`] bytes.
    fn lingered(&self, lane: Lane, now: Instant) -> bool {
        self.linger_until(lane).is_none_or(|at| at <= now)
            || self.waiting.get(&lane).is_some_and(|(items, load)| {
                items.len() >= S::BATCH_ITEMS || load.bytes >= S::BATCH_BYTES
            })
    }

    /// When `lane`'s oldest waiting item has lingered, or `None` when it has
    /// nothing waiting.
    fn linger_until(&self, lane: Lane) -> Option<Instant> {
        self.since.get(&lane).map(|since| *since + S::LINGER)
    }

    /// The earliest future time a failed lane may retry or a lingering lane
    /// may write, or `None` when no timer is needed because every write slot
    /// is busy or nothing waits on time.
    fn next_wake(&self) -> Option<Instant> {
        if self.writing.len() >= self.concurrency {
            return None;
        }
        let now = Instant::now();
        let retries = self
            .retry_at
            .iter()
            .filter(|(lane, (at, _))| *at > now && self.failed.contains_key(*lane))
            .map(|(_, (at, _))| *at);
        let lingering = self
            .since
            .keys()
            .filter_map(|lane| self.linger_until(*lane))
            .filter(|at| *at > now);
        retries.chain(lingering).min()
    }

    /// Settles one finished write.
    ///
    /// Every joined write, succeeded or failed, records its dispatch-to-settle
    /// time in `outbox_write_seconds`. Success clears the lane's backoff,
    /// counts the items and bytes written, and releases the slots of the
    /// items it wrote, which the write task already handed to the blocking
    /// pool to free. Failure,
    /// including a contained sink panic, keeps the items as the lane's next
    /// write, unmerged with later arrivals, doubles its backoff, and counts
    /// and logs the attempt. A task that failed to join lost its items, which
    /// are counted lost with [`LossReason::Rejected`]; with sink panics
    /// contained that needs a panic that escapes containment, such as one
    /// raised while dropping a panic payload.
    fn finish(&mut self, done: Result<(Id, Written<S>), JoinError>) {
        let ((lane, slice, items, result), load) = match done {
            Ok((id, written)) => {
                let load =
                    self.in_flight
                        .remove(&id)
                        .map_or_else(Load::default, |(_, load, started)| {
                            metrics::histogram!("outbox_write_seconds", "outbox" => S::NAME)
                                .record(started.elapsed().as_secs_f64());
                            load
                        });
                (written, load)
            }
            Err(error) => {
                if let Some(((tenant, lane), load, _)) = self.in_flight.remove(&error.id()) {
                    count_lost::<S>(load.items, LossReason::Rejected);
                    tracing::error!(outbox = S::NAME, %tenant, lane, %error, lost = load.items, "outbox write task failed");
                    self.release(load);
                }
                return;
            }
        };
        let error = match result {
            Ok(()) => {
                self.retry_at.remove(&lane);
                metrics::counter!("outbox_items_written_total", "outbox" => S::NAME)
                    .increment(load.items as u64);
                metrics::counter!("outbox_bytes_written_total", "outbox" => S::NAME)
                    .increment(load.bytes as u64);
                self.release(load);
                return;
            }
            Err(error) => error,
        };
        let backoff = self
            .retry_at
            .get(&lane)
            .map_or(INITIAL_BACKOFF, |(_, last)| (*last * 2).min(MAX_BACKOFF));
        self.retry_at
            .insert(lane, (Instant::now() + backoff, backoff));
        metrics::counter!("outbox_write_failures_total", "outbox" => S::NAME).increment(1);
        tracing::error!(
            outbox = S::NAME,
            tenant = %lane.0,
            lane = lane.1,
            %error,
            items = items.len(),
            retry_in_ms = backoff.as_millis(),
            "outbox write failed; retrying"
        );
        self.failed.insert(lane, (slice, (items, load)));
    }

    /// Releases written or lost items from pending and their shares, and
    /// wakes settlers when nothing remains pending.
    fn release(&self, load: Load) {
        metrics::gauge!("outbox_pending", "outbox" => S::NAME).decrement(load.items as f64);
        self.admission
            .staged_reserved
            .fetch_sub(load.reserved, Ordering::AcqRel);
        self.admission.release(load);
        if self.pending.fetch_sub(load.items, Ordering::AcqRel) == load.items {
            self.idle.notify_waiters();
        }
    }
}

/// Polls `future` to completion and returns its output, or the payload of a
/// panic raised while polling it.
///
/// Containing the panic inside the polled task, rather than letting it unwind
/// the task, keeps the caller's owned state, such as a write's items, intact.
///
/// # Errors
///
/// Returns the panic payload when polling `future` panics; the future is then
/// dropped without being polled again.
async fn contain_panic<F: Future>(future: F) -> Result<F::Output, Box<dyn Any + Send>> {
    let mut future = pin!(future);
    poll_fn(
        |context| match catch_unwind(AssertUnwindSafe(|| future.as_mut().poll(context))) {
            Ok(poll) => poll.map(Ok),
            Err(panic) => Poll::Ready(Err(panic)),
        },
    )
    .await
}

/// The message of a panic payload, when it is a string.
fn panic_message(panic: &(dyn Any + Send)) -> &str {
    panic
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("non-string panic payload")
}

/// Counts `count` items of outbox `S` as lost in
/// `outbox_events_lost_total{outbox, reason}`.
///
/// The outbox counts shed, shutdown, and late-staging loss itself; a sink
/// calls this for items it consumes without writing, such as a terminal
/// rejection that a retry could never cure, and a producer for items it
/// sheds at its own limit before staging.
pub fn count_lost<S: OutboxSink>(count: usize, reason: LossReason) {
    metrics::counter!("outbox_events_lost_total", "outbox" => S::NAME, "reason" => reason.as_str())
        .increment(count as u64);
}

/// Counts and logs one item of `tenant` lost because it was staged after
/// shutdown began or the writer is gone.
fn lose_late<S: OutboxSink>(tenant: DataTenantId) {
    count_lost::<S>(1, LossReason::Shutdown);
    tracing::error!(outbox = S::NAME, %tenant, "outbox item staged after shutdown was lost");
}

#[cfg(test)]
mod tests {
    //! The generic outbox against an in-memory sink whose tenants can be made
    //! to fail on demand.

    use std::collections::{HashMap, HashSet};
    use std::fmt::{self, Display, Formatter};
    use std::future::{Future, poll_fn};
    use std::pin::pin;
    use std::sync::atomic::Ordering;
    use std::sync::{Arc, Mutex, MutexGuard};
    use std::task::Poll;
    use std::time::{Duration, Instant};

    use metrics::atomics::AtomicU64;
    use metrics::{
        Counter, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit,
    };
    use tokio::sync::Notify;
    use uuid::Uuid;
    use wyrd_spec::DataTenantId;

    use super::{Outbox, OutboxLimits, OutboxSink};

    /// In-memory sink recording each tenant's written items in order.
    #[derive(Default)]
    struct MemorySink {
        /// Shared record of written items and failing tenants.
        state: Arc<Mutex<MemoryState>>,
        /// Writes of hanging tenants block until notified, simulating a hang.
        hang: Arc<Notify>,
        /// Notified as each write starts, so a test can wait for a dispatch
        /// without polling.
        started: Arc<Notify>,
    }

    /// What the memory sink has written and which tenants fail or hang.
    #[derive(Default)]
    struct MemoryState {
        /// Written items per tenant, in write order.
        written: HashMap<DataTenantId, Vec<u32>>,
        /// Tenants whose writes fail.
        failing: HashSet<DataTenantId>,
        /// Tenants whose writes hang.
        hanging: HashSet<DataTenantId>,
        /// Tenants whose next write panics once.
        panicking: HashSet<DataTenantId>,
        /// Tenants whose next write fails once with an error whose `Display`
        /// panics.
        display_panicking: HashSet<DataTenantId>,
        /// Failed write attempts.
        failures: usize,
        /// Every write attempt's tenant, slice key, and items, in attempt
        /// order.
        attempts: Vec<(DataTenantId, Uuid, Vec<u32>)>,
    }

    /// Failure the memory sink reports for an injected write failure.
    #[derive(Debug)]
    struct MemoryError {
        /// Whether rendering this error panics, simulating a sink error type
        /// whose `Display` implementation is faulty.
        panics_on_display: bool,
    }

    impl Display for MemoryError {
        /// Renders the injected failure message.
        ///
        /// # Panics
        ///
        /// Panics when `panics_on_display` is set, so a test can prove the
        /// writer contains a panic raised while rendering a sink error.
        fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
            assert!(!self.panics_on_display, "injected error display panic");
            formatter.write_str("injected failure")
        }
    }

    impl OutboxSink for MemorySink {
        type Item = u32;
        type Error = MemoryError;
        const NAME: &'static str = "test";

        /// Accounts each item as its own value in bytes, so a test sizes
        /// items by choosing their values.
        fn weight(item: &u32) -> usize {
            *item as usize
        }

        /// Writes `items` for `tenant`, applying the injections configured in
        /// [`MemoryState`] in a fixed order the tests depend on.
        ///
        /// The order is: notify `started` that a write was dispatched; if the
        /// tenant is hanging, wait for `hang`; if the tenant has a one-shot
        /// panic injected, consume it and panic; if the tenant has a one-shot
        /// panicking-display failure injected, consume it, count a failure,
        /// and fail; if the tenant is failing, count a failure and fail;
        /// otherwise append `items` to the tenant's written record.
        ///
        /// # Errors
        ///
        /// Returns a [`MemoryError`] when the tenant is failing or has a
        /// panicking-display failure injected; that error's `Display` panics
        /// only in the latter case.
        ///
        /// # Panics
        ///
        /// Panics once, after the hang stage, when a panic is injected for the
        /// tenant.
        ///
        /// # Cancellation
        ///
        /// Dropping the future while it waits for `hang` leaves the state
        /// unchanged, so the batch is neither written nor counted as failed.
        async fn write(
            &self,
            tenant: DataTenantId,
            slice: Uuid,
            items: &[u32],
        ) -> Result<(), MemoryError> {
            self.started.notify_one();
            let hangs = self.lock().hanging.contains(&tenant);
            if hangs {
                self.hang.notified().await;
            }
            let panics = self.lock().panicking.remove(&tenant);
            assert!(!panics, "injected sink panic");
            let mut state = self.lock();
            state.attempts.push((tenant, slice, items.to_vec()));
            if state.display_panicking.remove(&tenant) {
                state.failures += 1;
                return Err(MemoryError {
                    panics_on_display: true,
                });
            }
            if state.failing.contains(&tenant) {
                state.failures += 1;
                return Err(MemoryError {
                    panics_on_display: false,
                });
            }
            state
                .written
                .entry(tenant)
                .or_default()
                .extend_from_slice(items);
            Ok(())
        }
    }

    impl MemorySink {
        /// Locks the shared state, which no test holds across an await.
        fn lock(&self) -> MutexGuard<'_, MemoryState> {
            self.state
                .lock()
                .expect("memory sink state is never poisoned")
        }
    }

    /// An item whose drop blocks until its gate opens or ten seconds pass,
    /// standing in for a slice that is slow to free.
    struct SlowDrop(Mutex<std::sync::mpsc::Receiver<()>>);

    impl Drop for SlowDrop {
        /// Blocks until the gate opens, its sender is dropped, or ten
        /// seconds pass.
        fn drop(&mut self) {
            if let Ok(gate) = self.0.lock() {
                let _ = gate.recv_timeout(Duration::from_secs(10));
            }
        }
    }

    /// A sink that accepts every write of [`SlowDrop`] items.
    struct SlowDropSink;

    impl OutboxSink for SlowDropSink {
        type Item = SlowDrop;
        type Error = MemoryError;
        const NAME: &'static str = "slow_drop";

        /// Accepts `items` without inspecting them.
        ///
        /// # Errors
        ///
        /// Never fails.
        fn write(
            &self,
            _tenant: DataTenantId,
            _slice: Uuid,
            _items: &[SlowDrop],
        ) -> impl Future<Output = Result<(), MemoryError>> + Send {
            std::future::ready(Ok(()))
        }
    }

    /// Proves freeing a written slice does not hold up the writer: while the
    /// first slice's item is still blocked in its drop, a slice staged after
    /// it is written and settles.
    ///
    /// # Panics
    ///
    /// Panics when the second slice waits for the first slice's drop. Two
    /// workers keep a blocked writer loop from also blocking the test.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn freeing_a_written_slice_does_not_hold_up_the_next_write() {
        let outbox = Outbox::new(SlowDropSink, 1, ROOMY);
        let tenant = DataTenantId::new_v7();
        let (gate, held) = std::sync::mpsc::channel();
        outbox.stage(tenant, SlowDrop(Mutex::new(held)));
        let first = outbox.settle(Instant::now() + Duration::from_secs(2)).await;
        let (_, open) = std::sync::mpsc::channel();
        outbox.stage(tenant, SlowDrop(Mutex::new(open)));
        let second = outbox.settle(Instant::now() + Duration::from_secs(2)).await;
        drop(gate);
        assert_eq!((first, second), (0, 0), "the next write waited for a drop");
    }

    /// A sink that lingers [`LingerSink::LINGER`] and writes at three items,
    /// recording each written slice.
    #[derive(Default)]
    struct LingerSink {
        /// Every written slice, in completion order.
        written: Mutex<Vec<Vec<u32>>>,
    }

    impl OutboxSink for LingerSink {
        type Item = u32;
        type Error = MemoryError;
        const NAME: &'static str = "linger";
        const LINGER: Duration = Duration::from_millis(300);
        const BATCH_ITEMS: usize = 3;

        /// Records `items` once written.
        ///
        /// # Errors
        ///
        /// Never fails.
        ///
        /// # Panics
        ///
        /// Panics if the record's lock is poisoned.
        fn write(
            &self,
            _tenant: DataTenantId,
            _slice: Uuid,
            items: &[u32],
        ) -> impl Future<Output = Result<(), MemoryError>> + Send {
            self.written
                .lock()
                .expect("unpoisoned")
                .push(items.to_vec());
            std::future::ready(Ok(()))
        }
    }

    /// Proves a lingering lane writes items staged one at a time as one
    /// write once its oldest has lingered, and writes at once on reaching
    /// `BATCH_ITEMS`.
    ///
    /// # Panics
    ///
    /// Panics when the trickled items are split across writes, a full batch
    /// waits out the linger, or the outbox does not settle.
    #[tokio::test]
    async fn a_lingering_lane_batches_trickled_items_and_writes_a_full_batch_at_once() {
        let outbox = Outbox::new(LingerSink::default(), 1, ROOMY);
        let tenant = DataTenantId::new_v7();
        outbox.stage(tenant, 1_u32);
        outbox.stage(tenant, 2_u32);
        assert_eq!(
            outbox.settle(within(5)).await,
            0,
            "the lingering lane settles"
        );
        let started = Instant::now();
        for item in 3_u32..=5 {
            outbox.stage(tenant, item);
        }
        assert_eq!(outbox.settle(within(5)).await, 0, "the full batch settles");
        assert!(
            started.elapsed() < LingerSink::LINGER,
            "a full batch waited out the linger"
        );
        let written = outbox.sink().written.lock().expect("unpoisoned").clone();
        assert_eq!(written, [vec![1, 2], vec![3, 4, 5]]);
    }

    /// A two-lane sink, odd items in lane 1 and even in lane 0, that records
    /// each written slice and holds the slice `[0]` until `gate` is notified.
    #[derive(Default)]
    struct LaneSink {
        /// Releases the held `[0]` write.
        gate: Notify,
        /// Every written slice, in completion order.
        written: Mutex<Vec<Vec<u32>>>,
    }

    impl OutboxSink for LaneSink {
        type Item = u32;
        type Error = MemoryError;
        const NAME: &'static str = "lanes";
        const LANES: usize = 2;

        /// An item's parity is its lane.
        fn lane(item: &u32) -> u64 {
            u64::from(item % 2)
        }

        /// Records `items` once written, first waiting for `gate` when the
        /// slice is exactly `[0]`.
        ///
        /// # Errors
        ///
        /// Never fails.
        ///
        /// # Panics
        ///
        /// Panics if the record's lock is poisoned.
        async fn write(
            &self,
            _tenant: DataTenantId,
            _slice: Uuid,
            items: &[u32],
        ) -> Result<(), MemoryError> {
            if items == [0] {
                self.gate.notified().await;
            }
            self.written
                .lock()
                .expect("unpoisoned")
                .push(items.to_vec());
            Ok(())
        }
    }

    /// One tenant's lanes write concurrently while each lane keeps arrival
    /// order: lane 1 commits while lane 0's first write hangs, and lane 0's
    /// later items wait for that write instead of passing it.
    ///
    /// # Panics
    ///
    /// Panics when lane 1 waits for lane 0, or lane 0 writes out of order.
    #[tokio::test]
    async fn a_tenants_lanes_write_concurrently_and_each_keeps_order() {
        let outbox = Outbox::new(LaneSink::default(), 2, ROOMY);
        let tenant = DataTenantId::new_v7();
        outbox.stage(tenant, 0_u32);
        outbox.stage(tenant, 1_u32);
        let deadline = within(10);
        while outbox.sink().written.lock().expect("unpoisoned").is_empty() {
            assert!(Instant::now() < deadline, "lane 1 waited for lane 0");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        outbox.stage(tenant, 2_u32);
        outbox.stage(tenant, 4_u32);
        outbox.sink().gate.notify_one();

        assert_eq!(outbox.settle(within(10)).await, 0);
        assert_eq!(
            *outbox.sink().written.lock().expect("unpoisoned"),
            vec![vec![1], vec![0], vec![2, 4]]
        );
    }

    /// A sink whose items weigh their value, written at most
    /// [`ByteSink::BATCH_BYTES`] at a time, that records each written slice
    /// and holds the slice `[0]` until `gate` is notified.
    #[derive(Default)]
    struct ByteSink {
        /// Notified when the held `[0]` write starts.
        started: Notify,
        /// Releases the held `[0]` write.
        gate: Notify,
        /// Every written slice, in completion order.
        written: Mutex<Vec<Vec<u32>>>,
    }

    impl OutboxSink for ByteSink {
        type Item = u32;
        type Error = MemoryError;
        const NAME: &'static str = "bytes";
        const BATCH_BYTES: usize = 10;

        /// An item weighs its value.
        fn weight(item: &u32) -> usize {
            *item as usize
        }

        /// Records `items` once written, first waiting for `gate` when the
        /// slice is exactly `[0]`.
        ///
        /// # Errors
        ///
        /// Never fails.
        ///
        /// # Panics
        ///
        /// Panics if the record's lock is poisoned.
        async fn write(
            &self,
            _tenant: DataTenantId,
            _slice: Uuid,
            items: &[u32],
        ) -> Result<(), MemoryError> {
            if items == [0] {
                self.started.notify_one();
                self.gate.notified().await;
            }
            self.written
                .lock()
                .expect("unpoisoned")
                .push(items.to_vec());
            Ok(())
        }
    }

    /// A backlog heavier than `BATCH_BYTES` is written in arrival order as
    /// consecutive slices of at most that many bytes, an item heavier than
    /// the bound alone.
    ///
    /// # Panics
    ///
    /// Panics when a slice exceeds the bound with more than one item, items
    /// leave arrival order, or the outbox does not settle.
    #[tokio::test]
    async fn a_heavy_backlog_is_written_in_bounded_slices_in_order() {
        let outbox = Outbox::new(ByteSink::default(), 1, ROOMY);
        let tenant = DataTenantId::new_v7();
        outbox.stage(tenant, 0_u32);
        outbox.sink().started.notified().await;
        for item in [4_u32, 5, 1, 20, 3] {
            outbox.stage(tenant, item);
        }
        outbox.sink().gate.notify_one();

        assert_eq!(outbox.settle(within(10)).await, 0);
        assert_eq!(
            *outbox.sink().written.lock().expect("unpoisoned"),
            vec![vec![0], vec![4, 5, 1], vec![20], vec![3]]
        );
    }

    /// Capacity of test outboxes that do not exercise the bound.
    const ROOMY: OutboxLimits = items(1_024);

    /// Limits of `items` items and unlimited bytes.
    const fn items(items: usize) -> OutboxLimits {
        OutboxLimits {
            items,
            bytes: usize::MAX,
        }
    }

    /// Starts an outbox over a fresh memory sink and returns both.
    fn outbox(concurrency: usize) -> (Arc<Outbox<MemorySink>>, Arc<Mutex<MemoryState>>) {
        let sink = MemorySink::default();
        let state = Arc::clone(&sink.state);
        (Outbox::new(sink, concurrency, ROOMY), state)
    }

    /// Starts an outbox over `sink` and returns it with the sink's state and
    /// write-start signal.
    fn outbox_over(
        sink: MemorySink,
    ) -> (
        Arc<Outbox<MemorySink>>,
        Arc<Mutex<MemoryState>>,
        Arc<Notify>,
    ) {
        let state = Arc::clone(&sink.state);
        let started = Arc::clone(&sink.started);
        (Outbox::new(sink, 4, ROOMY), state, started)
    }

    /// Thread-local metrics recorder keeping every counter and gauge by name,
    /// and by name and `reason` label when it has one.
    ///
    /// Installed with `metrics::set_default_local_recorder` in a
    /// current-thread test, it also sees the outbox writer's updates because
    /// every task runs on the test thread.
    #[derive(Default)]
    struct TestMetrics {
        /// Raw value of each metric, keyed `name` or `name{reason=...}`;
        /// gauges store `f64` bits.
        values: Mutex<HashMap<String, Arc<AtomicU64>>>,
    }

    impl TestMetrics {
        /// The shared value cell of the metric `key` names, one per `reason`
        /// label value.
        fn cell(&self, key: &Key) -> Arc<AtomicU64> {
            let name = key
                .labels()
                .find(|label| label.key() == "reason")
                .map_or_else(
                    || key.name().to_owned(),
                    |reason| format!("{}{{reason={}}}", key.name(), reason.value()),
                );
            let mut values = self.values.lock().expect("test metrics are never poisoned");
            Arc::clone(values.entry(name).or_default())
        }

        /// The raw value of metric `name`, summed over its `reason` labels
        /// unless `name` names one as `name{reason=...}`; zero when it was
        /// never touched.
        fn raw(&self, name: &str) -> u64 {
            let labelled = format!("{name}{{");
            self.values
                .lock()
                .expect("test metrics are never poisoned")
                .iter()
                .filter(|(key, _)| *key == name || key.starts_with(&labelled))
                .map(|(_, value)| value.load(Ordering::Acquire))
                .sum()
        }

        /// The value of counter `name`.
        fn counter(&self, name: &str) -> u64 {
            self.raw(name)
        }

        /// The value of gauge `name`.
        fn gauge(&self, name: &str) -> f64 {
            f64::from_bits(self.raw(name))
        }
    }

    impl Recorder for TestMetrics {
        /// Intentionally ignores counter descriptions; tests assert values
        /// only.
        fn describe_counter(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}

        /// Intentionally ignores gauge descriptions; tests assert values only.
        fn describe_gauge(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}

        /// Intentionally ignores histogram descriptions; histograms are not
        /// recorded.
        fn describe_histogram(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}

        /// Records the counter by metric name, ignoring labels, so every
        /// registration of one name shares a single value cell.
        fn register_counter(&self, key: &Key, _: &Metadata<'_>) -> Counter {
            Counter::from_arc(self.cell(key))
        }

        /// Records the gauge by metric name, ignoring labels, storing its
        /// `f64` value as bits in a shared cell.
        fn register_gauge(&self, key: &Key, _: &Metadata<'_>) -> Gauge {
            Gauge::from_arc(self.cell(key))
        }

        /// Intentionally records nothing: the outbox emits no histogram the
        /// tests assert on.
        fn register_histogram(&self, _: &Key, _: &Metadata<'_>) -> Histogram {
            Histogram::noop()
        }
    }

    /// A deadline `seconds` from now.
    fn within(seconds: u64) -> Instant {
        Instant::now() + Duration::from_secs(seconds)
    }

    /// Waits until the sink has recorded at least `failures` failed writes.
    async fn await_failures(state: &Mutex<MemoryState>, failures: usize) {
        let deadline = within(10);
        while state.lock().expect("unpoisoned").failures < failures {
            assert!(Instant::now() < deadline, "the write never failed");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    /// A failed write is retried and committed exactly once, in order, ahead of
    /// the tenant's later items, and `pending` returns to zero.
    #[tokio::test]
    async fn a_failed_write_is_retried_once_in_order_ahead_of_later_items() {
        let (outbox, state) = outbox(4);
        let tenant = DataTenantId::new_v7();
        state.lock().expect("unpoisoned").failing.insert(tenant);
        outbox.stage(tenant, 1_u32);
        outbox.stage(tenant, 2_u32);
        await_failures(&state, 2).await;
        outbox.stage(tenant, 3_u32);
        assert_eq!(outbox.pending(), 3, "failed items stay pending");
        state.lock().expect("unpoisoned").failing.clear();

        assert_eq!(outbox.settle(within(10)).await, 0, "the retry drains");
        assert_eq!(
            state.lock().expect("unpoisoned").written.get(&tenant),
            Some(&vec![1, 2, 3]),
            "retried items commit once, ahead of later items"
        );
        assert_eq!(outbox.pending(), 0);
    }

    /// A retry writes exactly the failed slice under its key: every attempt
    /// sees the same items and key, and an item staged after the failure is
    /// written separately under a new key once the retry succeeds, so a sink
    /// deriving identity from the key presents one identity across attempts.
    #[tokio::test]
    async fn a_retry_writes_the_identical_failed_slice_without_later_items() {
        let (outbox, state) = outbox(4);
        let tenant = DataTenantId::new_v7();
        state.lock().expect("unpoisoned").failing.insert(tenant);
        outbox.stage(tenant, 1_u32);
        outbox.stage(tenant, 2_u32);
        await_failures(&state, 1).await;
        outbox.stage(tenant, 3_u32);
        await_failures(&state, 2).await;
        state.lock().expect("unpoisoned").failing.clear();

        assert_eq!(outbox.settle(within(10)).await, 0, "the retry drains");
        let state = state.lock().expect("unpoisoned");
        let (last, retries) = state
            .attempts
            .split_last()
            .expect("the later item was written");
        assert_eq!(
            (last.0, &last.2),
            (tenant, &vec![3]),
            "the later item is its own write"
        );
        assert!(retries.len() >= 3, "two failures and one successful retry");
        let slice = retries[0].1;
        assert!(
            retries
                .iter()
                .all(|attempt| attempt == &(tenant, slice, vec![1, 2])),
            "every retry is the identical failed slice under one key: {retries:?}"
        );
        assert_ne!(last.1, slice, "the later write has its own key");
        assert_eq!(state.written.get(&tenant), Some(&vec![1, 2, 3]));
    }

    /// A tenant whose writes keep failing does not delay another tenant.
    #[tokio::test]
    async fn a_failing_tenant_does_not_delay_another_tenant() {
        let (outbox, state) = outbox(1);
        let failing = DataTenantId::new_v7();
        let healthy = DataTenantId::new_v7();
        state.lock().expect("unpoisoned").failing.insert(failing);
        outbox.stage(failing, 1_u32);
        await_failures(&state, 1).await;
        outbox.stage(healthy, 2_u32);

        let deadline = within(10);
        while !state
            .lock()
            .expect("unpoisoned")
            .written
            .contains_key(&healthy)
        {
            assert!(Instant::now() < deadline, "the healthy tenant was delayed");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(outbox.pending(), 1, "only the failing tenant's item waits");
        state.lock().expect("unpoisoned").failing.clear();
        assert_eq!(outbox.settle(within(10)).await, 0);
    }

    /// The bound covers in-flight and retrying work, not just the queue: with
    /// one tenant's write hanging and another's retrying, a reservation past
    /// the reserved share is refused, retries do not charge their slice
    /// again, and capacity returns once the work is written.
    ///
    /// # Panics
    /// Panics when a reservation within the share is refused, one past it is
    /// admitted, the refusal reports other counts, or the work is not written
    /// once in order.
    #[tokio::test]
    async fn bounded_staging_covers_retrying_and_inflight_work() {
        let sink = MemorySink::default();
        let (hang, started) = (Arc::clone(&sink.hang), Arc::clone(&sink.started));
        let state = Arc::clone(&sink.state);
        // Reservations may hold three of these four items.
        let outbox = Outbox::new(sink, 4, items(4));
        let (hanging, failing) = (DataTenantId::new_v7(), DataTenantId::new_v7());
        {
            let mut state = state.lock().expect("unpoisoned");
            state.hanging.insert(hanging);
            state.failing.insert(failing);
        }
        let reserved = outbox.reserve(1).expect("room for one reservation");
        reserved.stage(hanging, 1_u32);
        started.notified().await;
        let retried = outbox.reserve(2).expect("room for a two-item reservation");
        retried.stage(failing, 2_u32);
        retried.stage(failing, 3_u32);
        await_failures(&state, 3).await;
        assert_eq!(outbox.pending(), 3, "a retried slice is admitted once");

        let refused = outbox
            .reserve(1)
            .expect_err("in-flight and retrying work fill the share");
        assert_eq!(
            (refused.admitted, refused.limit, refused.unit),
            (3, 3, "items")
        );
        outbox.stage(failing, 4_u32);
        outbox.stage(failing, 5_u32);
        assert_eq!(
            outbox.pending(),
            4,
            "unreserved staging past capacity is lost"
        );

        {
            let mut state = state.lock().expect("unpoisoned");
            state.hanging.clear();
            state.failing.clear();
        }
        hang.notify_one();
        assert_eq!(
            outbox.settle(within(10)).await,
            0,
            "the bounded work drains"
        );
        drop((reserved, retried));
        let recovered = outbox
            .reserve(3)
            .expect("written work releases its capacity");
        recovered.stage(hanging, 6_u32);
        assert_eq!(outbox.settle(within(10)).await, 0);
        let state = state.lock().expect("unpoisoned");
        assert_eq!(state.written[&hanging], vec![1, 6]);
        assert_eq!(state.written[&failing], vec![2, 3, 4]);
    }

    /// Unreserved staging on a full outbox, as audit decisions use it, returns
    /// at once and counts the item lost; reservations cannot take the share
    /// kept for it, unreserved items never count against reservations, and a
    /// dropped reservation returns its unspent slots.
    ///
    /// # Panics
    /// Panics when a full outbox blocks or keeps an unreserved item, the lost
    /// count differs, or reservations are admitted or refused against the
    /// wrong share.
    #[tokio::test(flavor = "current_thread")]
    async fn bounded_audit_staging_remains_nonblocking() {
        let metrics = TestMetrics::default();
        let _metrics = metrics::set_default_local_recorder(&metrics);
        let sink = MemorySink::default();
        let state = Arc::clone(&sink.state);
        let outbox = Outbox::new(sink, 4, items(4));
        let tenant = DataTenantId::new_v7();
        state.lock().expect("unpoisoned").hanging.insert(tenant);

        let unspent = outbox.reserve(3).expect("the reserved share");
        outbox
            .reserve(1)
            .expect_err("reservations hold their whole share");
        outbox.stage(tenant, 1_u32);
        assert_eq!(
            outbox.pending(),
            1,
            "the kept share admits unreserved staging"
        );
        outbox.stage(tenant, 2_u32);
        assert_eq!(
            outbox.pending(),
            1,
            "a full unreserved share refuses without waiting"
        );
        assert_eq!(
            metrics.counter("outbox_events_lost_total{reason=shed}"),
            1,
            "the refusal is counted as shed"
        );

        drop(unspent);
        outbox
            .reserve(3)
            .expect("a dropped reservation returns its slots despite the hanging item");
    }

    /// A full outbox admits waiting reservations in arrival order as capacity
    /// is released, and refuses one whose deadline has passed.
    #[tokio::test(flavor = "current_thread")]
    async fn waiting_reservations_are_admitted_in_arrival_order() {
        let outbox = Outbox::new(MemorySink::default(), 4, items(4));
        let mut held = (0..3)
            .map(|_| outbox.reserve(1).expect("the reserved share"))
            .collect::<Vec<_>>();
        outbox
            .reserve_until(1, Instant::now())
            .await
            .expect_err("a passed deadline refuses at once");

        let waiter = |outbox: &Arc<Outbox<MemorySink>>| {
            let outbox = Arc::clone(outbox);
            tokio::spawn(async move { outbox.reserve_until(1, within(10)).await })
        };
        let first = waiter(&outbox);
        tokio::task::yield_now().await;
        let second = waiter(&outbox);
        tokio::task::yield_now().await;

        drop(held.pop());
        let first = first
            .await
            .expect("joined")
            .expect("the first waiter takes the freed slot");
        tokio::task::yield_now().await;
        assert!(!second.is_finished(), "the second waiter queues behind it");

        drop(held.pop());
        let _second = second
            .await
            .expect("joined")
            .expect("the next freed slot admits the second waiter");
        drop(first);
    }

    /// Capacity freed for the head of the queue is never taken by a later
    /// caller, even one asking for less, and a waiter that times out leaves
    /// the queue so the next one is admitted.
    #[tokio::test(flavor = "current_thread")]
    async fn a_later_caller_never_takes_capacity_freed_for_the_head() {
        let outbox = Outbox::new(MemorySink::default(), 4, items(4));
        let mut held = (0..3)
            .map(|_| outbox.reserve(1).expect("the reserved share"))
            .collect::<Vec<_>>();
        let head = {
            let outbox = Arc::clone(&outbox);
            tokio::spawn(async move { outbox.reserve_until(2, within(10)).await })
        };
        tokio::task::yield_now().await;

        drop(held.pop());
        tokio::task::yield_now().await;
        outbox
            .reserve_until(1, Instant::now() + Duration::from_millis(50))
            .await
            .expect_err("a later caller does not take the head's freed slot");
        assert!(!head.is_finished(), "the head still needs a second slot");

        let later = {
            let outbox = Arc::clone(&outbox);
            tokio::spawn(async move { outbox.reserve_until(1, within(10)).await })
        };
        tokio::task::yield_now().await;
        drop(held.pop());
        let head = head
            .await
            .expect("joined")
            .expect("the head is admitted once both slots are free");
        tokio::task::yield_now().await;
        assert!(!later.is_finished(), "the later caller still queues");
        drop(head);
        later
            .await
            .expect("joined")
            .expect("the head's departure admits the next caller");
    }

    /// Shutdown keeps retrying until the deadline and writes what recovers.
    #[tokio::test]
    async fn shutdown_flushes_items_that_recover_before_the_deadline() {
        let (outbox, state) = outbox(4);
        let tenant = DataTenantId::new_v7();
        state.lock().expect("unpoisoned").failing.insert(tenant);
        outbox.stage(tenant, 1_u32);
        await_failures(&state, 1).await;
        let shutdown = {
            let outbox = Arc::clone(&outbox);
            tokio::spawn(async move { outbox.shutdown(within(10)).await })
        };
        await_failures(&state, 2).await;
        state.lock().expect("unpoisoned").failing.clear();

        assert_eq!(shutdown.await.expect("shutdown joins"), 0, "nothing lost");
        assert_eq!(
            state.lock().expect("unpoisoned").written.get(&tenant),
            Some(&vec![1])
        );
        outbox.stage(tenant, 2_u32);
        assert_eq!(outbox.pending(), 0, "a shut-down outbox accepts nothing");
    }

    /// Shutdown gives up at its deadline, reports failing and hanging items as
    /// lost exactly once, and leaves nothing pending in the count or gauge.
    #[tokio::test(flavor = "current_thread")]
    async fn shutdown_counts_items_unwritten_at_the_deadline_as_lost() {
        let metrics = TestMetrics::default();
        let _metrics = metrics::set_default_local_recorder(&metrics);
        let (outbox, state) = outbox(4);
        let failing = DataTenantId::new_v7();
        let hanging = DataTenantId::new_v7();
        {
            let mut state = state.lock().expect("unpoisoned");
            state.failing.insert(failing);
            state.hanging.insert(hanging);
        }
        outbox.stage(failing, 1_u32);
        outbox.stage(failing, 2_u32);
        outbox.stage(hanging, 3_u32);
        await_failures(&state, 1).await;

        let lost = outbox
            .shutdown(Instant::now() + Duration::from_millis(200))
            .await;
        assert_eq!(lost, 3, "every unwritten item is reported lost");
        assert!(state.lock().expect("unpoisoned").written.is_empty());
        assert_eq!(
            metrics.counter("outbox_events_lost_total{reason=shutdown}"),
            3,
            "lost once, at shutdown"
        );
        assert_eq!(outbox.pending(), 0, "abandoned items are not pending");
        assert!(
            metrics.gauge("outbox_pending").abs() < f64::EPSILON,
            "the pending gauge returns to zero"
        );
    }

    /// Concurrent shutdowns over reserved and unreserved items unwritten at
    /// the deadline report the loss once, settle both shares exactly, and
    /// leave an unspent reservation holding its slot until it drops.
    ///
    /// # Panics
    /// Panics when a reservation is refused, the loss is reported other than
    /// once, or a share or count does not settle.
    #[tokio::test(flavor = "current_thread")]
    async fn concurrent_shutdown_releases_reserved_losses_once() {
        let metrics = TestMetrics::default();
        let _metrics = metrics::set_default_local_recorder(&metrics);
        let (outbox, state) = outbox(4);
        let hanging = DataTenantId::new_v7();
        state.lock().expect("unpoisoned").hanging.insert(hanging);
        let held = outbox.reserve(1).expect("an unspent slot is admitted");
        let spent = outbox.reserve(2).expect("two reserved slots are admitted");
        spent.stage(hanging, 1_u32);
        spent.stage(hanging, 2_u32);
        drop(spent);
        outbox.stage(hanging, 3_u32);

        let deadline = Instant::now() + Duration::from_millis(200);
        let (first, second) = tokio::join!(outbox.shutdown(deadline), outbox.shutdown(deadline));
        assert_eq!(first + second, 3, "the loss is reported by one caller");
        assert_eq!(first.min(second), 0, "the other caller reports zero");
        assert_eq!(
            metrics.counter("outbox_events_lost_total{reason=shutdown}"),
            3,
            "lost once, at shutdown"
        );
        assert_eq!(outbox.pending(), 0, "abandoned items are not pending");
        assert_eq!(outbox.admission.staged_reserved.load(Ordering::Acquire), 0);
        assert_eq!(outbox.admission.unreserved.load(Ordering::Acquire), 0);
        assert_eq!(
            outbox.admission.reserved.load(Ordering::Acquire),
            1,
            "only the unspent reservation still holds a slot"
        );
        drop(held);
        assert_eq!(outbox.admission.reserved.load(Ordering::Acquire), 0);
    }

    /// A sink panic is retried like a failed write: the batch stays pending,
    /// commits once ahead of a later item, the writer keeps running, nothing is
    /// counted lost, and `pending` returns to zero.
    #[tokio::test(flavor = "current_thread")]
    async fn a_panicking_write_is_retried_once_in_order_without_loss() {
        let metrics = TestMetrics::default();
        let _metrics = metrics::set_default_local_recorder(&metrics);
        let (outbox, state, started) = outbox_over(MemorySink::default());
        let tenant = DataTenantId::new_v7();
        state.lock().expect("unpoisoned").panicking.insert(tenant);
        outbox.stage(tenant, 1_u32);
        started.notified().await;
        outbox.stage(tenant, 2_u32);
        assert_eq!(outbox.pending(), 2, "the panicked batch stays pending");

        assert_eq!(outbox.settle(within(10)).await, 0, "the retry drains");
        assert_eq!(
            state.lock().expect("unpoisoned").written.get(&tenant),
            Some(&vec![1, 2]),
            "the panicked batch commits once, ahead of the later item"
        );
        assert_eq!(metrics.counter("outbox_write_failures_total"), 1);
        assert_eq!(metrics.counter("outbox_events_lost_total"), 0);
        assert!(metrics.gauge("outbox_pending").abs() < f64::EPSILON);
    }

    /// A sink error whose `Display` panics is retried like a failed write: the
    /// batch stays pending, commits once ahead of a later item, the writer
    /// keeps running, the failure is counted once, nothing is counted lost,
    /// and `pending` and its gauge return to zero.
    #[tokio::test(flavor = "current_thread")]
    async fn a_panicking_error_display_is_retried_once_in_order_without_loss() {
        let metrics = TestMetrics::default();
        let _metrics = metrics::set_default_local_recorder(&metrics);
        let (outbox, state, started) = outbox_over(MemorySink::default());
        let tenant = DataTenantId::new_v7();
        state
            .lock()
            .expect("unpoisoned")
            .display_panicking
            .insert(tenant);
        outbox.stage(tenant, 1_u32);
        started.notified().await;
        outbox.stage(tenant, 2_u32);
        assert_eq!(outbox.pending(), 2, "the failed batch stays pending");

        assert_eq!(outbox.settle(within(10)).await, 0, "the retry drains");
        assert_eq!(
            state.lock().expect("unpoisoned").written.get(&tenant),
            Some(&vec![1, 2]),
            "the failed batch commits once, ahead of the later item"
        );
        assert_eq!(metrics.counter("outbox_write_failures_total"), 1);
        assert_eq!(metrics.counter("outbox_events_lost_total"), 0);
        assert!(metrics.gauge("outbox_pending").abs() < f64::EPSILON);
    }

    /// Shutdown fences admission when it begins: items staged before it drain
    /// even behind a blocked write, and a later stage is refused and counted
    /// lost without reaching the sink.
    #[tokio::test(flavor = "current_thread")]
    async fn shutdown_refuses_items_staged_after_it_begins() {
        let metrics = TestMetrics::default();
        let _metrics = metrics::set_default_local_recorder(&metrics);
        let sink = MemorySink::default();
        let hang = Arc::clone(&sink.hang);
        let (outbox, state, started) = outbox_over(sink);
        let tenant = DataTenantId::new_v7();
        state.lock().expect("unpoisoned").hanging.insert(tenant);
        outbox.stage(tenant, 1_u32);
        started.notified().await;
        outbox.stage(tenant, 2_u32);

        let mut shutdown = pin!(outbox.shutdown(within(10)));
        let begun = poll_fn(|context| Poll::Ready(shutdown.as_mut().poll(context).is_pending()));
        assert!(begun.await, "shutdown waits for the blocked write");
        outbox.stage(tenant, 3_u32);
        assert_eq!(outbox.pending(), 2, "the late item never entered the queue");
        assert_eq!(
            metrics.counter("outbox_events_lost_total{reason=shutdown}"),
            1
        );

        state.lock().expect("unpoisoned").hanging.clear();
        hang.notify_one();
        assert_eq!(shutdown.await, 0, "pre-fence items drain");
        assert_eq!(
            state.lock().expect("unpoisoned").written.get(&tenant),
            Some(&vec![1, 2]),
            "only pre-fence items reach the sink"
        );
        assert_eq!(outbox.pending(), 0);
        assert!(metrics.gauge("outbox_pending").abs() < f64::EPSILON);
    }

    /// Large items shed on bytes before the item limit is reached, small ones
    /// are still admitted in the bytes left, written items release their
    /// bytes, and reservations are refused on bytes once reserved items hold
    /// their byte share, while a spent slot always keeps its item.
    ///
    /// # Panics
    /// Panics when an item is admitted past an unreserved byte limit, shed
    /// within it, counted under another reason, or a reservation is admitted
    /// or refused against the wrong byte share.
    #[tokio::test(flavor = "current_thread")]
    async fn large_items_shed_on_bytes_before_items() {
        let metrics = TestMetrics::default();
        let _metrics = metrics::set_default_local_recorder(&metrics);
        let sink = MemorySink::default();
        let (hang, state) = (Arc::clone(&sink.hang), Arc::clone(&sink.state));
        // Unreserved staging holds 100 bytes; reservations 300.
        let outbox = Outbox::new(
            sink,
            4,
            OutboxLimits {
                items: 1_024,
                bytes: 400,
            },
        );
        let tenant = DataTenantId::new_v7();
        state.lock().expect("unpoisoned").hanging.insert(tenant);

        outbox.stage(tenant, 60_u32);
        outbox.stage(tenant, 60_u32);
        assert_eq!(
            outbox.pending(),
            1,
            "the second large item is shed on bytes"
        );
        outbox.stage(tenant, 40_u32);
        assert_eq!(outbox.pending(), 2, "a small item fits the bytes left");
        assert_eq!(metrics.counter("outbox_events_lost_total{reason=shed}"), 1);

        let reservation = outbox.reserve(2).expect("reserved bytes are free");
        reservation.stage(tenant, 300_u32);
        let full = outbox
            .reserve(1)
            .expect_err("reserved items hold the reserved byte share");
        assert_eq!((full.admitted, full.limit, full.unit), (300, 300, "bytes"));
        reservation.stage(tenant, 50_u32);
        assert_eq!(
            outbox.pending(),
            4,
            "a spent slot keeps its item past the share"
        );
        drop(reservation);

        state.lock().expect("unpoisoned").hanging.clear();
        hang.notify_one();
        assert_eq!(outbox.settle(within(10)).await, 0);
        assert_eq!(metrics.counter("outbox_items_written_total"), 4);
        assert_eq!(metrics.counter("outbox_bytes_written_total"), 450);
        assert_eq!(outbox.admission.unreserved_bytes.load(Ordering::Acquire), 0);
        assert_eq!(outbox.admission.reserved_bytes.load(Ordering::Acquire), 0);
        outbox.stage(tenant, 100_u32);
        outbox
            .reserve(1)
            .expect("written items release their bytes");
        assert_eq!(outbox.settle(within(10)).await, 0);
        assert_eq!(
            metrics.counter("outbox_events_lost_total"),
            1,
            "nothing else was lost"
        );
    }

    /// Shed records of one tenant collected by [`ShedRecords`]: the `shed`
    /// count of every record, in order.
    #[derive(Default)]
    struct ShedRecords {
        /// The `shed` field of each record.
        counts: Mutex<Vec<u64>>,
    }

    /// Reads the `shed` field of an event.
    struct ShedField<'a>(&'a mut Option<u64>);

    impl tracing::field::Visit for ShedField<'_> {
        /// Keeps the value of the `shed` field.
        fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
            if field.name() == "shed" {
                *self.0 = Some(value);
            }
        }

        /// Ignores every field not recorded as a `u64`.
        fn record_debug(&mut self, _: &tracing::field::Field, _: &dyn fmt::Debug) {}
    }

    impl tracing::Subscriber for ShedRecords {
        /// Sees every event.
        fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
            true
        }

        /// Assigns every span the same id; spans are not inspected.
        fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            tracing::span::Id::from_u64(1)
        }

        /// Ignores span records.
        fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}

        /// Ignores span links.
        fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}

        /// Keeps the `shed` count of every event carrying one.
        fn event(&self, event: &tracing::Event<'_>) {
            let mut shed = None;
            event.record(&mut ShedField(&mut shed));
            if let Some(shed) = shed {
                self.counts.lock().expect("unpoisoned").push(shed);
            }
        }

        /// Ignores span entry.
        fn enter(&self, _: &tracing::span::Id) {}

        /// Ignores span exit.
        fn exit(&self, _: &tracing::span::Id) {}
    }

    /// A tenant shedding continuously writes one shed record per interval,
    /// and shutdown records the rest, so the records' counts sum to the
    /// `shed` loss counter.
    ///
    /// # Panics
    /// Panics when more than one record is written within the interval, a
    /// shed item is left unrecorded after shutdown, or the counts disagree.
    #[tokio::test(flavor = "current_thread")]
    async fn shed_records_are_aggregated_per_tenant_and_interval() {
        let metrics = TestMetrics::default();
        let _metrics = metrics::set_default_local_recorder(&metrics);
        let records = Arc::new(ShedRecords::default());
        let _records = tracing::subscriber::set_default(Arc::clone(&records));
        let sink = MemorySink::default();
        let state = Arc::clone(&sink.state);
        let outbox = Outbox::new(sink, 4, items(4));
        let (tenant, other) = (DataTenantId::new_v7(), DataTenantId::new_v7());
        state.lock().expect("unpoisoned").hanging.insert(tenant);
        outbox.stage(tenant, 1_u32);
        for _ in 0..5 {
            outbox.stage(tenant, 2_u32);
        }
        outbox.stage(other, 3_u32);
        assert_eq!(
            *records.counts.lock().expect("unpoisoned"),
            vec![1, 1],
            "one record per tenant within the interval"
        );

        outbox.shutdown(Instant::now()).await;
        let counts = records.counts.lock().expect("unpoisoned").clone();
        assert_eq!(counts, vec![1, 1, 4], "shutdown records the rest");
        assert_eq!(
            counts.iter().sum::<u64>(),
            metrics.counter("outbox_events_lost_total{reason=shed}")
        );
    }
}
