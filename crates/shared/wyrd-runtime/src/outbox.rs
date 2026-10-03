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
//! A failed write is never dropped. Its items go back to the front of that
//! tenant's queue, ahead of anything that arrived meanwhile, and the tenant is
//! retried after a backoff that starts at [`INITIAL_BACKOFF`] and doubles up to
//! [`MAX_BACKOFF`]; other tenants keep writing. Because a write may be retried
//! after an unknown commit outcome, every sink write must be safe to repeat.
//!
//! The queue has no count limit and nothing is preallocated. Items are lost
//! only when the process stops abruptly, when graceful shutdown reaches its
//! deadline, or when an item is staged after shutdown; shutdown and late
//! staging count them in `outbox_events_lost_total{outbox}`. Every failed write
//! attempt is counted in `outbox_write_failures_total{outbox}`, and the items
//! not yet written are exported as the `outbox_pending{outbox}` gauge.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::fmt::Display;
use std::future::{Future, pending};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
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

/// One destination an outbox writes to.
///
/// Implementations make exactly one durable call per write: everything a
/// tenant has queued, in one transaction, all or nothing. The outbox owns
/// queueing, grouping, concurrency, retry, and shutdown.
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

/// The handle callers stage through; owns the queue and its writer task.
pub struct Outbox<S: OutboxSink> {
    /// Sending half of the unbounded queue the writer drains.
    queue: mpsc::UnboundedSender<(DataTenantId, S::Item)>,
    /// Items queued, waiting, or being written, not yet written or lost.
    pending: Arc<AtomicUsize>,
    /// Signalled by the writer whenever a settled write leaves nothing pending.
    idle: Arc<Notify>,
    /// Asks the writer to stop taking items and finish what it holds.
    stop: CancellationToken,
    /// Asks the writer to give up at the shutdown deadline.
    abandon: CancellationToken,
    /// Tracks the writer so shutdown can wait for it.
    writer: TaskTracker,
}

impl<S: OutboxSink> Outbox<S> {
    /// Creates the outbox around `sink` and starts its writer.
    ///
    /// `concurrency` is the most tenants written at once, which bounds the
    /// connections the sink may hold.
    ///
    /// # Panics
    ///
    /// Panics when called outside a Tokio runtime, because the writer task is
    /// spawned immediately.
    #[must_use]
    pub fn new(sink: S, concurrency: usize) -> Arc<Self> {
        let (queue, requests) = mpsc::unbounded_channel();
        let pending = Arc::new(AtomicUsize::new(0));
        let idle = Arc::new(Notify::new());
        let stop = CancellationToken::new();
        let abandon = CancellationToken::new();
        let writer = TaskTracker::new();
        writer.spawn(
            OutboxWriter {
                sink: Arc::new(sink),
                requests,
                waiting: HashMap::new(),
                writing: JoinSet::new(),
                in_flight: HashMap::new(),
                retry_at: HashMap::new(),
                concurrency: concurrency.max(1),
                pending: Arc::clone(&pending),
                idle: Arc::clone(&idle),
                stop: stop.clone(),
                abandon: abandon.clone(),
            }
            .run(),
        );
        writer.close();
        Arc::new(Self {
            queue,
            pending,
            idle,
            stop,
            abandon,
            writer,
        })
    }

    /// Queues one item for `tenant` without waiting and never fails.
    ///
    /// After shutdown the item is counted lost and logged instead.
    pub fn stage(&self, tenant: DataTenantId, item: impl Into<S::Item>) {
        self.pending.fetch_add(1, Ordering::AcqRel);
        if self.queue.send((tenant, item.into())).is_err() {
            self.pending.fetch_sub(1, Ordering::AcqRel);
            count_lost::<S>(1);
            tracing::error!(outbox = S::NAME, %tenant, "outbox item staged after shutdown was lost");
            return;
        }
        metrics::gauge!("outbox_pending", "outbox" => S::NAME).increment(1.0);
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
    /// returns how many items were lost.
    ///
    /// Items still unwritten at the deadline are abandoned, counted in
    /// `outbox_events_lost_total`, and logged; an in-flight write is cancelled.
    pub async fn shutdown(&self, deadline: Instant) -> usize {
        self.stop.cancel();
        if timeout_at(deadline.into(), self.writer.wait())
            .await
            .is_err()
        {
            self.abandon.cancel();
            self.writer.wait().await;
        }
        self.pending()
    }
}

/// Outcome of one spawned tenant write: the tenant, its items, and the result.
type Written<S> = (
    DataTenantId,
    Vec<<S as OutboxSink>::Item>,
    Result<(), <S as OutboxSink>::Error>,
);

/// The writer task behind one [`Outbox`].
///
/// Moved into its task by [`Outbox::new`]; it owns the receiving half of the
/// queue, the per-tenant backlogs, the in-flight writes, and the retry state.
struct OutboxWriter<S: OutboxSink> {
    /// Destination every write goes to.
    sink: Arc<S>,
    /// Receiving half of the queue.
    requests: mpsc::UnboundedReceiver<(DataTenantId, S::Item)>,
    /// Received items per tenant, in arrival order, not being written.
    waiting: HashMap<DataTenantId, Vec<S::Item>>,
    /// In-flight tenant writes.
    writing: JoinSet<Written<S>>,
    /// Tenant and item count of each in-flight write, keyed by its task.
    in_flight: HashMap<Id, (DataTenantId, usize)>,
    /// Earliest retry time and current backoff of each tenant whose last
    /// write failed.
    retry_at: HashMap<DataTenantId, (Instant, Duration)>,
    /// Most tenants written at once.
    concurrency: usize,
    /// Count shared with the handle; decremented once items are written.
    pending: Arc<AtomicUsize>,
    /// Woken whenever a settled write leaves nothing pending.
    idle: Arc<Notify>,
    /// Cancelled by shutdown to close the queue.
    stop: CancellationToken,
    /// Cancelled when shutdown reaches its deadline.
    abandon: CancellationToken,
}

impl<S: OutboxSink> OutboxWriter<S> {
    /// The writer loop: receives items, dispatches tenant writes, settles
    /// finished ones, and wakes for retries, until stopped with nothing left or
    /// abandoned at the shutdown deadline.
    async fn run(mut self) {
        let mut received = Vec::new();
        let mut open = true;
        let mut closing = false;
        loop {
            self.dispatch();
            if !open && self.waiting.is_empty() && self.writing.is_empty() {
                return;
            }
            let retry = self.next_retry();
            tokio::select! {
                biased;
                () = self.abandon.cancelled() => {
                    self.abandon_remaining();
                    return;
                }
                Some(done) = self.writing.join_next_with_id(), if !self.writing.is_empty() => {
                    self.finish(done);
                }
                count = self.requests.recv_many(&mut received, usize::MAX), if open => {
                    open = count != 0;
                    for (tenant, item) in received.drain(..) {
                        self.waiting.entry(tenant).or_default().push(item);
                    }
                }
                () = self.stop.cancelled(), if !closing => {
                    closing = true;
                    self.requests.close();
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
    /// run. A write takes the tenant's whole backlog.
    fn dispatch(&mut self) {
        let now = Instant::now();
        let ready: Vec<DataTenantId> = self
            .waiting
            .keys()
            .filter(|tenant| self.is_ready(**tenant, now))
            .copied()
            .collect();
        for tenant in ready {
            if self.writing.len() >= self.concurrency {
                return;
            }
            let Some(items) = self.waiting.remove(&tenant) else {
                continue;
            };
            let count = items.len();
            let sink = Arc::clone(&self.sink);
            let task = self.writing.spawn(async move {
                let result = sink.write(tenant, &items).await;
                (tenant, items, result)
            });
            self.in_flight.insert(task.id(), (tenant, count));
        }
    }

    /// Whether `tenant` has no write in flight and its backoff has elapsed.
    fn is_ready(&self, tenant: DataTenantId, now: Instant) -> bool {
        !self.in_flight.values().any(|(busy, _)| *busy == tenant)
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
            .filter(|(tenant, (at, _))| *at > now && self.waiting.contains_key(*tenant))
            .map(|(_, (at, _))| *at)
            .min()
    }

    /// Settles one finished write.
    ///
    /// Success clears the tenant's backoff and releases its items. Failure puts
    /// the items back at the front of the tenant's backlog, doubles its
    /// backoff, and counts and logs the attempt. A write task that panicked
    /// lost its items, which are counted lost.
    fn finish(&mut self, done: Result<(Id, Written<S>), JoinError>) {
        let (tenant, items, result) = match done {
            Ok((id, written)) => {
                self.in_flight.remove(&id);
                written
            }
            Err(error) => {
                if let Some((tenant, count)) = self.in_flight.remove(&error.id()) {
                    count_lost::<S>(count);
                    tracing::error!(outbox = S::NAME, %tenant, %error, lost = count, "outbox write task failed");
                    self.release(count);
                }
                return;
            }
        };
        let error = match result {
            Ok(()) => {
                self.retry_at.remove(&tenant);
                self.release(items.len());
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
        match self.waiting.entry(tenant) {
            Entry::Occupied(mut queued) => {
                let later = std::mem::replace(queued.get_mut(), items);
                queued.get_mut().extend(later);
            }
            Entry::Vacant(slot) => {
                slot.insert(items);
            }
        }
    }

    /// Releases `count` written or lost items from the pending total and wakes
    /// settlers when nothing remains.
    fn release(&self, count: usize) {
        metrics::gauge!("outbox_pending", "outbox" => S::NAME).decrement(count as f64);
        if self.pending.fetch_sub(count, Ordering::AcqRel) == count {
            self.idle.notify_waiters();
        }
    }

    /// Counts and logs every item still unwritten at the shutdown deadline.
    ///
    /// The items stay in the pending total, which shutdown reports; returning
    /// afterwards drops the in-flight writes, cancelling them.
    fn abandon_remaining(&self) {
        let lost = self.pending.load(Ordering::Acquire);
        if lost > 0 {
            count_lost::<S>(lost);
            tracing::error!(
                outbox = S::NAME,
                lost,
                "outbox shutdown deadline passed with items unwritten"
            );
        }
    }
}

/// Counts `count` items of outbox `S` as lost.
fn count_lost<S: OutboxSink>(count: usize) {
    metrics::counter!("outbox_events_lost_total", "outbox" => S::NAME).increment(count as u64);
}

#[cfg(test)]
mod tests {
    //! The generic outbox against an in-memory sink whose tenants can be made
    //! to fail on demand.

    use std::collections::{HashMap, HashSet};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use tokio::sync::Notify;
    use wyrd_spec::DataTenantId;

    use super::{Outbox, OutboxSink};

    /// In-memory sink recording each tenant's written items in order.
    #[derive(Default)]
    struct MemorySink {
        /// Shared record of written items and failing tenants.
        state: Arc<Mutex<MemoryState>>,
        /// Writes of these tenants block until notified, simulating a hang.
        hang: Arc<Notify>,
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
        /// Failed write attempts.
        failures: usize,
    }

    impl OutboxSink for MemorySink {
        type Item = u32;
        type Error = String;
        const NAME: &'static str = "test";

        async fn write(&self, tenant: DataTenantId, items: &[u32]) -> Result<(), String> {
            let hangs = self.lock().hanging.contains(&tenant);
            if hangs {
                self.hang.notified().await;
            }
            let mut state = self.lock();
            if state.failing.contains(&tenant) {
                state.failures += 1;
                return Err("injected failure".to_owned());
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
        fn lock(&self) -> std::sync::MutexGuard<'_, MemoryState> {
            self.state
                .lock()
                .expect("memory sink state is never poisoned")
        }
    }

    /// Starts an outbox over a fresh memory sink and returns both.
    fn outbox(concurrency: usize) -> (Arc<Outbox<MemorySink>>, Arc<Mutex<MemoryState>>) {
        let sink = MemorySink::default();
        let state = Arc::clone(&sink.state);
        (Outbox::new(sink, concurrency), state)
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

    /// The queue accepts far more items than any former limit, all written.
    #[tokio::test]
    async fn the_queue_has_no_count_limit() {
        let (outbox, state) = outbox(4);
        let tenant = DataTenantId::new_v7();
        for item in 0..50_000_u32 {
            outbox.stage(tenant, item);
        }
        assert_eq!(outbox.settle(within(30)).await, 0);
        let written = state.lock().expect("unpoisoned").written[&tenant].clone();
        assert_eq!(written, (0..50_000_u32).collect::<Vec<_>>());
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

    /// Shutdown gives up at its deadline and reports failing and hanging
    /// items as lost.
    #[tokio::test]
    async fn shutdown_counts_items_unwritten_at_the_deadline_as_lost() {
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
    }
}
