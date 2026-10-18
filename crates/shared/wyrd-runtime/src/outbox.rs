//! Generic non-blocking, per-tenant batched outbox.
//!
//! A server surface that must record derived work without delaying the request
//! that produced it hands each item to [`Outbox::stage`], which only enqueues
//! it. One background writer per outbox groups queued items by tenant and
//! writes each tenant's whole backlog through its [`OutboxSink`] in one
//! all-or-nothing call. At most `concurrency` tenants are written at once and
//! each tenant has at most one write in flight, so a slow tenant delays only
//! its own items.
//!
//! A failed write is never dropped. A write that returns an error or panics
//! keeps its exact items as that tenant's next write, apart from anything that
//! arrived meanwhile, and the tenant is retried after a backoff that starts at
//! [`INITIAL_BACKOFF`] and doubles up to [`MAX_BACKOFF`]; other tenants keep
//! writing. Later arrivals wait for the write after the retry succeeds, so a
//! sink deriving identity from its slice presents the same identity on every
//! attempt. Because a write may be retried after an unknown commit outcome,
//! every sink write must be safe to repeat.
//!
//! Every outbox has a finite `capacity` of admitted items: items queued,
//! waiting, retrying, or being written, plus slots reserved and not yet
//! staged. An item stays admitted until it is written or lost, never merely
//! dequeued, and a retried slice is admitted once however often it is
//! retried. The capacity is split in two shares. A producer that must refuse
//! its caller rather than lose an item takes a [`Reservation`] first;
//! [`Outbox::reserve`] refuses with [`OutboxFull`] once reserved slots and
//! the unwritten items staged through them would exceed [`RESERVED_SHARE`]
//! of the capacity. Unreserved items from [`Outbox::stage`] hold the rest,
//! so neither share can crowd out the other. `stage` never fails or waits:
//! past its share an item is counted lost and logged, so a full outbox never
//! blocks or fails the operation staging it.
//!
//! Nothing is preallocated. Items are lost when the process stops abruptly,
//! when graceful shutdown reaches its deadline, when an item is staged after
//! shutdown begins, or when an unreserved item finds the outbox full; each is
//! counted in `outbox_events_lost_total{outbox}`. Every failed write attempt
//! is counted in `outbox_write_failures_total{outbox}`, and the items not yet
//! written are exported as the `outbox_pending{outbox}` gauge.

use std::any::Any;
use std::collections::HashMap;
use std::fmt::Display;
use std::future::{Future, pending, poll_fn};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, PoisonError, RwLock};
use std::task::Poll;
use std::time::{Duration, Instant};

use tokio::sync::{Notify, mpsc};
use tokio::task::{Id, JoinError, JoinSet};
use tokio::time::{sleep_until, timeout_at};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use wyrd_spec::DataTenantId;

/// Delay before the first retry of a tenant whose write failed.
pub const INITIAL_BACKOFF: Duration = Duration::from_millis(50);

/// Longest delay between retries of a tenant whose writes keep failing.
pub const MAX_BACKOFF: Duration = Duration::from_secs(5);

/// Share of an outbox's capacity, as a fraction `(numerator, denominator)`,
/// that reservations may hold; the rest is unreserved staging's, so a
/// refusing producer's burst cannot crowd out audit decisions and their
/// traffic cannot refuse the producer.
pub const RESERVED_SHARE: (usize, usize) = (3, 4);

/// Refusal of a [`Reservation`] because the outbox holds its reserved share.
///
/// Nothing was admitted, so the caller may retry the whole operation later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("outbox {outbox} is full: {admitted} of {limit} reservable items admitted")]
pub struct OutboxFull {
    /// The refusing outbox's [`OutboxSink::NAME`].
    pub outbox: &'static str,
    /// Reserved slots and unwritten reserved items when the reservation was
    /// refused.
    pub admitted: usize,
    /// Items reservations may hold in this outbox.
    pub limit: usize,
}

/// One destination an outbox writes to.
///
/// Implementations write everything a tenant has queued in one call: in one
/// transaction, or as idempotent writes whose repeat is absorbed. The outbox
/// owns queueing, grouping, concurrency, retry, and shutdown.
pub trait OutboxSink: Send + Sync + 'static {
    /// The item callers stage.
    type Item: Send + 'static;
    /// The failure a write reports; logged, counted, and retried.
    type Error: Display + Send + 'static;
    /// Label for this outbox's metrics and logs.
    const NAME: &'static str;

    /// Writes one tenant's items, in order, in one transaction.
    ///
    /// A write may be retried after its commit outcome is unknown, so writing
    /// the same items twice must leave the same durable state as writing them
    /// once.
    ///
    /// # Errors
    ///
    /// Returns the sink's error when any part of the write fails; nothing of
    /// the batch may then be durable unless a repeat would skip it.
    fn write(
        &self,
        tenant: DataTenantId,
        items: &[Self::Item],
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

/// One queued item: its tenant, the item, and whether it was staged on a
/// reserved slot.
type Queued<S> = (DataTenantId, <S as OutboxSink>::Item, bool);

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
    /// Most items this outbox admits at once.
    capacity: usize,
    /// Signalled whenever a settled write or abandonment leaves nothing
    /// pending.
    idle: Arc<Notify>,
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
    /// `concurrency` is the most tenants written at once, which bounds the
    /// connections the sink may hold; `capacity` is the most items admitted
    /// at once, which bounds the memory queued, waiting, retrying, and
    /// in-flight work may hold.
    ///
    /// # Panics
    ///
    /// Panics when called outside a Tokio runtime, because the writer task is
    /// spawned immediately.
    #[must_use]
    pub fn new(sink: S, concurrency: usize, capacity: usize) -> Arc<Self> {
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
            capacity,
            idle,
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
    /// writer can never release it first. When the unreserved share is full,
    /// once shutdown has begun, or if the writer is gone, the item never
    /// enters the queue; it is counted lost and logged instead.
    pub fn stage(&self, tenant: DataTenantId, item: impl Into<S::Item>) {
        let limit = self.capacity - self.reservable();
        if admit(&self.admission.unreserved, 1, limit).is_err() {
            count_lost::<S>(1);
            tracing::error!(outbox = S::NAME, %tenant, capacity = self.capacity, "outbox is full; the staged item was lost");
            return;
        }
        self.send(tenant, item.into(), false);
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
        let limit = self.reservable();
        admit(&self.admission.reserved, slots, limit).map_err(|admitted| OutboxFull {
            outbox: S::NAME,
            admitted,
            limit,
        })?;
        Ok(Reservation {
            outbox: Arc::clone(self),
            slots: AtomicUsize::new(slots),
        })
    }

    /// The most slots reservations may hold: [`RESERVED_SHARE`] of the
    /// capacity, rounded down.
    const fn reservable(&self) -> usize {
        self.capacity * RESERVED_SHARE.0 / RESERVED_SHARE.1
    }

    /// Sends one admitted item to the writer, counting it pending first;
    /// `reserved` says which share admitted it.
    ///
    /// Once shutdown has begun, or if the writer is gone, the item is released
    /// from its share, counted lost, and logged.
    fn send(&self, tenant: DataTenantId, item: S::Item, reserved: bool) {
        let queue = self.queue.read().unwrap_or_else(PoisonError::into_inner);
        if let Some(queue) = queue.as_ref() {
            let gauge = metrics::gauge!("outbox_pending", "outbox" => S::NAME);
            let reserved_count = usize::from(reserved);
            self.pending.fetch_add(1, Ordering::AcqRel);
            self.admission
                .staged_reserved
                .fetch_add(reserved_count, Ordering::AcqRel);
            gauge.increment(1.0);
            if queue.send((tenant, item, reserved)).is_ok() {
                return;
            }
            self.pending.fetch_sub(1, Ordering::AcqRel);
            self.admission
                .staged_reserved
                .fetch_sub(reserved_count, Ordering::AcqRel);
            gauge.decrement(1.0);
        }
        self.admission.release(1, usize::from(reserved));
        count_lost::<S>(1);
        tracing::error!(outbox = S::NAME, %tenant, "outbox item staged after shutdown was lost");
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
    /// queue. Items still unwritten at the deadline are abandoned with the
    /// writer, which cancels any in-flight write; once the writer is gone
    /// they are counted in `outbox_events_lost_total`, logged, and released
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
        if timeout_at(deadline.into(), self.writer.wait())
            .await
            .is_err()
        {
            self.abandon.cancel();
            self.writer.wait().await;
        }
        // With the queue closed and the writer gone nothing else moves these
        // counts, so the caller that takes a nonzero `pending` also owns its
        // reserved part and settles both shares alone.
        let lost = self.pending.swap(0, Ordering::AcqRel);
        if lost == 0 {
            return 0;
        }
        let reserved = self.admission.staged_reserved.swap(0, Ordering::AcqRel);
        self.admission.release(lost, reserved);
        count_lost::<S>(lost);
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
            self.outbox.send(tenant, item.into(), true);
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

/// The two shares an [`Outbox`] admits items into, shared with its writer.
#[derive(Debug, Default)]
struct Admission {
    /// Unreserved items not yet written or lost.
    unreserved: AtomicUsize,
    /// Unspent reserved slots plus reserved items not yet written or lost.
    reserved: AtomicUsize,
    /// Reserved items pending in the writer, which shutdown releases from
    /// `reserved` when it abandons them.
    staged_reserved: AtomicUsize,
}

impl Admission {
    /// Releases `count` written or lost items, `reserved` of them reserved,
    /// from their shares.
    fn release(&self, count: usize, reserved: usize) {
        self.reserved.fetch_sub(reserved, Ordering::AcqRel);
        self.unreserved
            .fetch_sub(count - reserved, Ordering::AcqRel);
    }
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

/// One tenant's items in arrival order and how many of them are reserved.
type Backlog<I> = (Vec<I>, usize);

/// Outcome of one spawned tenant write: the tenant, its items, and the result,
/// whose error is the sink's error or the message of a contained panic.
type Written<S> = (
    DataTenantId,
    Vec<<S as OutboxSink>::Item>,
    Result<(), String>,
);

/// The writer task behind one [`Outbox`].
///
/// Moved into its task by [`Outbox::new`]; it owns the receiving half of the
/// queue, the per-tenant backlogs, the in-flight writes, and the retry state.
struct OutboxWriter<S: OutboxSink> {
    /// Destination every write goes to.
    sink: Arc<S>,
    /// Receiving half of the queue.
    requests: mpsc::UnboundedReceiver<Queued<S>>,
    /// Received items per tenant, in arrival order, not being written.
    waiting: HashMap<DataTenantId, Backlog<S::Item>>,
    /// The exact slice of each tenant whose last write failed; it is that
    /// tenant's next write, unmerged with `waiting`.
    failed: HashMap<DataTenantId, Backlog<S::Item>>,
    /// In-flight tenant writes.
    writing: JoinSet<Written<S>>,
    /// Tenant, item count, and reserved item count of each in-flight write,
    /// keyed by its task.
    in_flight: HashMap<Id, (DataTenantId, usize, usize)>,
    /// Earliest retry time and current backoff of each tenant whose last
    /// write failed.
    retry_at: HashMap<DataTenantId, (Instant, Duration)>,
    /// Most tenants written at once.
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
    /// The writer loop: receives items, dispatches tenant writes, settles
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
            let retry = self.next_retry();
            tokio::select! {
                biased;
                () = self.abandon.cancelled() => return,
                Some(done) = self.writing.join_next_with_id(), if !self.writing.is_empty() => {
                    self.finish(done);
                }
                count = self.requests.recv_many(&mut received, usize::MAX), if open => {
                    open = count != 0;
                    for (tenant, item, reserved) in received.drain(..) {
                        let (items, reserved_count) = self.waiting.entry(tenant).or_default();
                        items.push(item);
                        *reserved_count += usize::from(reserved);
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

    /// Starts one write for each tenant that has items waiting, has no write
    /// in flight, and is not backing off, while fewer than `concurrency` writes
    /// run. A tenant with a failed slice writes exactly that slice again;
    /// otherwise a write takes the tenant's whole backlog.
    ///
    /// The write task keeps its items through a panicking sink, including a
    /// panic while rendering the sink's error, and returns them with the panic
    /// as the error, so they are retried like any failed write.
    fn dispatch(&mut self) {
        let now = Instant::now();
        let ready: Vec<DataTenantId> = self
            .waiting
            .keys()
            .chain(
                self.failed
                    .keys()
                    .filter(|tenant| !self.waiting.contains_key(*tenant)),
            )
            .filter(|tenant| self.is_ready(**tenant, now))
            .copied()
            .collect();
        for tenant in ready {
            if self.writing.len() >= self.concurrency {
                return;
            }
            let Some((items, reserved)) = self
                .failed
                .remove(&tenant)
                .or_else(|| self.waiting.remove(&tenant))
            else {
                continue;
            };
            let count = items.len();
            let sink = Arc::clone(&self.sink);
            let task = self.writing.spawn(async move {
                // The constructing closure borrows `items`, so it is dropped
                // before awaiting, and the write before `items` is returned.
                let outcome = {
                    let write = catch_unwind(AssertUnwindSafe(|| sink.write(tenant, &items)));
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
                (tenant, items, result)
            });
            self.in_flight.insert(task.id(), (tenant, count, reserved));
        }
    }

    /// Whether `tenant` has no write in flight and its backoff has elapsed.
    fn is_ready(&self, tenant: DataTenantId, now: Instant) -> bool {
        !self.in_flight.values().any(|(busy, ..)| *busy == tenant)
            && self.retry_at.get(&tenant).is_none_or(|(at, _)| *at <= now)
    }

    /// The earliest future retry time of a waiting tenant, or `None` when no
    /// timer is needed because every write slot is busy or nobody backs off.
    fn next_retry(&self) -> Option<Instant> {
        if self.writing.len() >= self.concurrency {
            return None;
        }
        let now = Instant::now();
        self.retry_at
            .iter()
            .filter(|(tenant, (at, _))| *at > now && self.failed.contains_key(*tenant))
            .map(|(_, (at, _))| *at)
            .min()
    }

    /// Settles one finished write.
    ///
    /// Success clears the tenant's backoff and releases its items. Failure,
    /// including a contained sink panic, keeps the items as the tenant's next
    /// write, unmerged with later arrivals, doubles its backoff, and counts
    /// and logs the attempt. A task that failed to join lost its items, which are counted
    /// lost; with sink panics contained that needs a panic that escapes
    /// containment, such as one raised while dropping a panic payload.
    fn finish(&mut self, done: Result<(Id, Written<S>), JoinError>) {
        let ((tenant, items, result), reserved) = match done {
            Ok((id, written)) => {
                let reserved = self
                    .in_flight
                    .remove(&id)
                    .map_or(0, |(.., reserved)| reserved);
                (written, reserved)
            }
            Err(error) => {
                if let Some((tenant, count, reserved)) = self.in_flight.remove(&error.id()) {
                    count_lost::<S>(count);
                    tracing::error!(outbox = S::NAME, %tenant, %error, lost = count, "outbox write task failed");
                    self.release(count, reserved);
                }
                return;
            }
        };
        let error = match result {
            Ok(()) => {
                self.retry_at.remove(&tenant);
                self.release(items.len(), reserved);
                return;
            }
            Err(error) => error,
        };
        let backoff = self
            .retry_at
            .get(&tenant)
            .map_or(INITIAL_BACKOFF, |(_, last)| (*last * 2).min(MAX_BACKOFF));
        self.retry_at
            .insert(tenant, (Instant::now() + backoff, backoff));
        metrics::counter!("outbox_write_failures_total", "outbox" => S::NAME).increment(1);
        tracing::error!(
            outbox = S::NAME,
            %tenant,
            %error,
            items = items.len(),
            retry_in_ms = backoff.as_millis(),
            "outbox write failed; retrying"
        );
        self.failed.insert(tenant, (items, reserved));
    }

    /// Releases `count` written or lost items, `reserved` of them reserved,
    /// from pending and their shares, and wakes settlers when nothing remains
    /// pending.
    fn release(&self, count: usize, reserved: usize) {
        metrics::gauge!("outbox_pending", "outbox" => S::NAME).decrement(count as f64);
        self.admission
            .staged_reserved
            .fetch_sub(reserved, Ordering::AcqRel);
        self.admission.release(count, reserved);
        if self.pending.fetch_sub(count, Ordering::AcqRel) == count {
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
/// `outbox_events_lost_total{outbox}`.
///
/// The outbox counts shutdown and late-staging loss itself; a sink calls this
/// for items it consumes without writing, such as a terminal rejection that a
/// retry could never cure.
pub fn count_lost<S: OutboxSink>(count: usize) {
    metrics::counter!("outbox_events_lost_total", "outbox" => S::NAME).increment(count as u64);
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
    use wyrd_spec::DataTenantId;

    use super::{Outbox, OutboxSink};

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
        /// Every write attempt's tenant and slice, in attempt order.
        attempts: Vec<(DataTenantId, Vec<u32>)>,
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
        async fn write(&self, tenant: DataTenantId, items: &[u32]) -> Result<(), MemoryError> {
            self.started.notify_one();
            let hangs = self.lock().hanging.contains(&tenant);
            if hangs {
                self.hang.notified().await;
            }
            let panics = self.lock().panicking.remove(&tenant);
            assert!(!panics, "injected sink panic");
            let mut state = self.lock();
            state.attempts.push((tenant, items.to_vec()));
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

    /// Capacity of test outboxes that do not exercise the bound.
    const ROOMY: usize = 1_024;

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

    /// Thread-local metrics recorder keeping every counter and gauge by name.
    ///
    /// Installed with `metrics::set_default_local_recorder` in a
    /// current-thread test, it also sees the outbox writer's updates because
    /// every task runs on the test thread.
    #[derive(Default)]
    struct TestMetrics {
        /// Raw value of each metric; gauges store `f64` bits.
        values: Mutex<HashMap<String, Arc<AtomicU64>>>,
    }

    impl TestMetrics {
        /// The shared value cell of the metric named by `key`.
        fn cell(&self, key: &Key) -> Arc<AtomicU64> {
            let mut values = self.values.lock().expect("test metrics are never poisoned");
            Arc::clone(values.entry(key.name().to_owned()).or_default())
        }

        /// The raw value of metric `name`, zero when it was never touched.
        fn raw(&self, name: &str) -> u64 {
            self.values
                .lock()
                .expect("test metrics are never poisoned")
                .get(name)
                .map_or(0, |value| value.load(Ordering::Acquire))
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

    /// A retry writes exactly the failed slice: every attempt sees the same
    /// items, and an item staged after the failure is written separately once
    /// the retry succeeds, so a sink deriving identity from its slice presents
    /// one identity across attempts.
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
        assert_eq!(last, &(tenant, vec![3]), "the later item is its own write");
        assert!(retries.len() >= 3, "two failures and one successful retry");
        assert!(
            retries
                .iter()
                .all(|attempt| attempt == &(tenant, vec![1, 2])),
            "every retry is the identical failed slice: {retries:?}"
        );
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
    #[tokio::test]
    async fn bounded_staging_covers_retrying_and_inflight_work() {
        let sink = MemorySink::default();
        let (hang, started) = (Arc::clone(&sink.hang), Arc::clone(&sink.started));
        let state = Arc::clone(&sink.state);
        // Reservations may hold three of these four items.
        let outbox = Outbox::new(sink, 4, 4);
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
        assert_eq!((refused.admitted, refused.limit), (3, 3));
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
    #[tokio::test(flavor = "current_thread")]
    async fn bounded_audit_staging_remains_nonblocking() {
        let metrics = TestMetrics::default();
        let _metrics = metrics::set_default_local_recorder(&metrics);
        let sink = MemorySink::default();
        let state = Arc::clone(&sink.state);
        let outbox = Outbox::new(sink, 4, 4);
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
            metrics.counter("outbox_events_lost_total"),
            1,
            "the refusal is counted"
        );

        drop(unspent);
        outbox
            .reserve(3)
            .expect("a dropped reservation returns its slots despite the hanging item");
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
        assert_eq!(metrics.counter("outbox_events_lost_total"), 3, "lost once");
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
        assert_eq!(metrics.counter("outbox_events_lost_total"), 3, "lost once");
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
        assert_eq!(metrics.counter("outbox_events_lost_total"), 1);

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
}
