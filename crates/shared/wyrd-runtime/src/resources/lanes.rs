//! Fair, non-preemptive admission to the process's managed CPU lanes.
//!
//! There are `C` lanes, one per effective CPU, shared by every workload's
//! major CPU phases. A lane is held only while work runs: an async future
//! holds one for a single poll and returns it on `Ready` or `Pending`, and a
//! blocking job holds one on its own CPU worker. Idle workloads hold none, so
//! one demanding workload can borrow every lane.
//!
//! A freed lane goes to the demanding workload with the least normalized
//! service, then to that workload's tenant with the least service, then to
//! that tenant's oldest waiter. Service is busy-lane time divided by the
//! (equal) workload weight, including the elapsed time of running leases. A
//! workload or tenant that returns from idle is rebased to the current
//! service floor, so idle time earns no credit. Nothing is preempted: a long
//! phase delays others until it returns its lane.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll, Waker};
use std::thread::Thread;
use std::time::Duration;

use tokio::time::Instant;
use wyrd_spec::ids::DataTenantId;

use super::workload::{WorkContext, Workload};

thread_local! {
    /// The work running on this thread's held lane, and its lanes, if any.
    static RUNNING: RefCell<Option<Inherited>> = const { RefCell::new(None) };
}

impl WorkContext {
    /// The context of the laned work running on this thread, if any.
    ///
    /// Spawn hooks capture it so spawned work inherits its parent's
    /// attribution.
    #[must_use]
    pub fn current() -> Option<Self> {
        RUNNING.with_borrow(|running| running.as_ref().map(|running| running.context))
    }
}

/// The laned work running on this thread, captured by a spawn hook so the
/// work it spawns runs on the same lanes as the same work: polled through
/// [`Self::laned`], or run blocking through [`Self::run_blocking`].
#[derive(Debug, Clone)]
pub struct Inherited {
    /// The lanes the parent runs on.
    lanes: CpuLanes,
    /// The parent's work context.
    context: WorkContext,
}

impl Inherited {
    /// The laned work running on this thread, if any.
    #[must_use]
    pub fn current() -> Option<Self> {
        RUNNING.with_borrow(Clone::clone)
    }

    /// Wraps spawned `future` so each poll runs on a lane as the parent's
    /// work; see [`CpuLanes::laned`].
    pub fn laned<F: Future>(self, future: F) -> Laned<F> {
        self.lanes.laned(self.context, future)
    }

    /// Runs spawned blocking `job` on a lane as the parent's work; see
    /// [`CpuLanes::run_blocking`].
    pub fn run_blocking<T>(&self, job: impl FnOnce() -> T) -> T {
        self.lanes.run_blocking(self.context, job)
    }
}

/// How a waiting acquirer is woken.
#[derive(Debug)]
enum Wake {
    /// An async acquirer's task.
    Task(Waker),
    /// A blocking acquirer's thread.
    Thread(Thread),
}

/// Where one acquisition stands.
#[derive(Debug)]
enum WaitState {
    /// Queued; woken through `Wake` when granted.
    Waiting(Wake),
    /// Granted a lane that the acquirer has not yet taken.
    Granted(RunId),
    /// Taken by the acquirer or withdrawn; nothing left to do.
    Done,
}

/// One queued acquisition.
#[derive(Debug)]
struct Waiter {
    /// The workload the acquisition queues under.
    workload: Workload,
    /// The tenant the acquisition queues under.
    tenant: Option<DataTenantId>,
    /// The acquirer's grant state.
    state: Mutex<WaitState>,
}

impl Waiter {
    /// The grant state, recovered from poisoning because every write leaves
    /// it consistent.
    fn state(&self) -> std::sync::MutexGuard<'_, WaitState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Process-local identity of one running lease.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RunId(u64);

/// One running lease.
#[derive(Debug)]
struct Running {
    /// The lease identity.
    id: RunId,
    /// Who the lane runs for.
    workload: Workload,
    /// The tenant the lane runs for, if attributed.
    tenant: Option<DataTenantId>,
    /// When the lane was granted.
    started: Instant,
}

/// One tenant's queue and service inside a workload.
#[derive(Debug, Default)]
struct TenantLane {
    /// Completed busy time.
    service: Duration,
    /// Waiting acquisitions, oldest first.
    waiters: VecDeque<Arc<Waiter>>,
}

/// One workload's tenants and service.
#[derive(Debug, Default)]
struct WorkloadLane {
    /// Completed busy time divided by the workload's (unit) weight.
    service: Duration,
    /// Per-tenant queues; `None` is unattributed work.
    tenants: HashMap<Option<DataTenantId>, TenantLane>,
}

/// Lane occupancy, queues and service accounting.
#[derive(Debug)]
struct LaneState {
    /// Lanes not held by a running lease.
    free: usize,
    /// Next lease id.
    next: u64,
    /// Running leases.
    running: Vec<Running>,
    /// Per-workload queues and service, in [`Workload::ALL`] order.
    workloads: [WorkloadLane; 3],
}

/// The process's managed CPU lanes.
///
/// Cloning shares the lanes.
#[derive(Debug, Clone)]
pub struct CpuLanes {
    /// The shared lane state.
    inner: Arc<Mutex<LaneState>>,
}

/// One held lane, returned when dropped.
#[derive(Debug)]
pub struct LaneLease {
    /// The lanes this lease returns to.
    lanes: CpuLanes,
    /// The running lease it ends.
    id: RunId,
}

impl Drop for LaneLease {
    fn drop(&mut self) {
        self.lanes.release(self.id);
    }
}

impl CpuLanes {
    /// `count` lanes, at least one.
    pub(crate) fn new(count: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(LaneState {
                free: count.max(1),
                next: 0,
                running: Vec::new(),
                workloads: Default::default(),
            })),
        }
    }

    /// Lanes no running work holds.
    #[must_use]
    pub fn free(&self) -> usize {
        self.state().free
    }

    /// Acquisitions waiting for a lane.
    #[must_use]
    pub fn waiting(&self) -> usize {
        self.state()
            .workloads
            .iter()
            .flat_map(|workload| workload.tenants.values())
            .map(|tenant| tenant.waiters.len())
            .sum()
    }

    /// Waits for a lane for `context`'s workload and tenant.
    ///
    /// Hold the lease only while CPU work runs; drop it before awaiting IO,
    /// memory or downstream admission. Dropping the future withdraws the
    /// request, returning a lane granted to it exactly once.
    pub async fn acquire(&self, context: WorkContext) -> LaneLease {
        Acquire {
            lanes: self.clone(),
            context,
            waiter: None,
        }
        .await
    }

    /// Runs the blocking CPU job `job` on the calling CPU worker thread once a
    /// lane is free, inheriting the thread's lane when it already holds one.
    ///
    /// Call only from a dedicated CPU or blocking worker, never a Tokio async
    /// worker.
    pub fn run_blocking<T>(&self, context: WorkContext, job: impl FnOnce() -> T) -> T {
        if RUNNING.with_borrow(Option::is_some) {
            return job();
        }
        let _lease = self.acquire_blocking(context);
        let _running = Scope::enter(self, context);
        job()
    }

    /// Wraps `future` so each poll runs on a lane for `context`, returning the
    /// lane on every `Ready` or `Pending`. Polls nested inside a laned poll on
    /// the same thread inherit its lane instead of acquiring another.
    pub fn laned<F: Future>(&self, context: WorkContext, future: F) -> Laned<F> {
        Laned {
            lanes: self.clone(),
            context,
            waiter: None,
            future: Box::pin(future),
        }
    }

    /// Takes a lane now or queues `waker` for one.
    fn try_acquire(&self, context: WorkContext, wake: Wake) -> Result<RunId, Arc<Waiter>> {
        let mut state = self.state();
        state.rebase(context.workload, context.tenant);
        if state.free > 0 && !state.queued() {
            return Ok(state.grant(context.workload, context.tenant));
        }
        let waiter = Arc::new(Waiter {
            workload: context.workload,
            tenant: context.tenant,
            state: Mutex::new(WaitState::Waiting(wake)),
        });
        state.enqueue(Arc::clone(&waiter));
        Err(waiter)
    }

    /// Blocks the calling thread until a lane is free for `context`.
    fn acquire_blocking(&self, context: WorkContext) -> LaneLease {
        let id = match self.try_acquire(context, Wake::Thread(std::thread::current())) {
            Ok(id) => id,
            Err(waiter) => loop {
                let mut state = waiter.state();
                if let WaitState::Granted(id) = *state {
                    *state = WaitState::Done;
                    break id;
                }
                drop(state);
                std::thread::park();
            },
        };
        LaneLease {
            lanes: self.clone(),
            id,
        }
    }

    /// Ends running lease `id`, accounts its busy time and grants freed lanes.
    fn release(&self, id: RunId) {
        let mut state = self.state();
        state.finish(id);
        state.dispatch();
    }

    /// Withdraws `waiter`, returning a lane it was granted but never took.
    fn withdraw(&self, waiter: &Arc<Waiter>) {
        let mut state = self.state();
        let previous = std::mem::replace(&mut *waiter.state(), WaitState::Done);
        match previous {
            WaitState::Waiting(_) => state.dequeue(waiter),
            WaitState::Granted(id) => {
                state.finish(id);
                state.dispatch();
            }
            WaitState::Done => {}
        }
    }

    /// The lane state, recovered from poisoning because every write leaves
    /// it consistent.
    fn state(&self) -> std::sync::MutexGuard<'_, LaneState> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl LaneState {
    /// Starts a running lease on a free lane.
    fn grant(&mut self, workload: Workload, tenant: Option<DataTenantId>) -> RunId {
        self.free -= 1;
        let id = RunId(self.next);
        self.next += 1;
        self.running.push(Running {
            id,
            workload,
            tenant,
            started: Instant::now(),
        });
        id
    }

    /// Ends running lease `id` and adds its busy time to its workload and
    /// tenant service.
    fn finish(&mut self, id: RunId) {
        let Some(position) = self.running.iter().position(|running| running.id == id) else {
            return;
        };
        let running = self.running.swap_remove(position);
        let busy = running.started.elapsed();
        let workload = &mut self.workloads[running.workload.index()];
        workload.service += busy;
        workload.tenants.entry(running.tenant).or_default().service += busy;
        self.free += 1;
        self.forget_idle(running.workload, running.tenant);
    }

    /// Drops an idle tenant's entry so the map holds only demanding tenants;
    /// a returning tenant is rebased to the service floor anyway.
    fn forget_idle(&mut self, workload: Workload, tenant: Option<DataTenantId>) {
        if !self.tenant_demanding(workload, tenant) {
            self.workloads[workload.index()].tenants.remove(&tenant);
        }
    }

    /// Whether any acquisition is waiting.
    fn queued(&self) -> bool {
        self.workloads
            .iter()
            .flat_map(|workload| workload.tenants.values())
            .any(|tenant| !tenant.waiters.is_empty())
    }

    /// Busy time of running leases matching `filter`, so far.
    fn running_service(&self, filter: impl Fn(&Running) -> bool) -> Duration {
        let now = Instant::now();
        self.running
            .iter()
            .filter(|running| filter(running))
            .map(|running| now.saturating_duration_since(running.started))
            .sum()
    }

    /// `workload`'s completed plus running service.
    fn workload_service(&self, workload: Workload) -> Duration {
        self.workloads[workload.index()].service
            + self.running_service(|running| running.workload == workload)
    }

    /// `tenant`'s completed plus running service within `workload`.
    fn tenant_service(&self, workload: Workload, tenant: Option<DataTenantId>) -> Duration {
        self.workloads[workload.index()]
            .tenants
            .get(&tenant)
            .map_or(Duration::ZERO, |lane| lane.service)
            + self
                .running_service(|running| running.workload == workload && running.tenant == tenant)
    }

    /// Whether `workload` has waiting or running work.
    fn workload_demanding(&self, workload: Workload) -> bool {
        self.workloads[workload.index()]
            .tenants
            .values()
            .any(|lane| !lane.waiters.is_empty())
            || self
                .running
                .iter()
                .any(|running| running.workload == workload)
    }

    /// Whether `tenant` has waiting or running work within `workload`.
    fn tenant_demanding(&self, workload: Workload, tenant: Option<DataTenantId>) -> bool {
        self.workloads[workload.index()]
            .tenants
            .get(&tenant)
            .is_some_and(|lane| !lane.waiters.is_empty())
            || self
                .running
                .iter()
                .any(|running| running.workload == workload && running.tenant == tenant)
    }

    /// Lifts a workload or tenant returning from idle to the least service
    /// of its demanding peers, so idle time earns no credit.
    fn rebase(&mut self, workload: Workload, tenant: Option<DataTenantId>) {
        if !self.workload_demanding(workload) {
            let floor = Workload::ALL
                .into_iter()
                .filter(|peer| *peer != workload && self.workload_demanding(*peer))
                .map(|peer| self.workload_service(peer))
                .min();
            if let Some(floor) = floor {
                let lane = &mut self.workloads[workload.index()];
                lane.service = lane.service.max(floor);
            }
        }
        if !self.tenant_demanding(workload, tenant) {
            let floor = self.workloads[workload.index()]
                .tenants
                .keys()
                .filter(|peer| **peer != tenant && self.tenant_demanding(workload, **peer))
                .map(|peer| self.tenant_service(workload, *peer))
                .min();
            if let Some(floor) = floor {
                let lane = self.workloads[workload.index()]
                    .tenants
                    .entry(tenant)
                    .or_default();
                lane.service = lane.service.max(floor);
            }
        }
    }

    /// Queues `waiter` under its workload and tenant.
    fn enqueue(&mut self, waiter: Arc<Waiter>) {
        self.workloads[waiter.workload.index()]
            .tenants
            .entry(waiter.tenant)
            .or_default()
            .waiters
            .push_back(waiter);
    }

    /// Removes a withdrawn `waiter` from its queue.
    fn dequeue(&mut self, waiter: &Arc<Waiter>) {
        if let Some(lane) = self.workloads[waiter.workload.index()]
            .tenants
            .get_mut(&waiter.tenant)
        {
            lane.waiters.retain(|queued| !Arc::ptr_eq(queued, waiter));
        }
        self.forget_idle(waiter.workload, waiter.tenant);
    }

    /// The waiting workload, then tenant, with the least service.
    fn next(&self) -> Option<(Workload, Option<DataTenantId>)> {
        let workload = Workload::ALL
            .into_iter()
            .filter(|workload| {
                self.workloads[workload.index()]
                    .tenants
                    .values()
                    .any(|lane| !lane.waiters.is_empty())
            })
            .min_by_key(|workload| self.workload_service(*workload))?;
        let tenant = self.workloads[workload.index()]
            .tenants
            .iter()
            .filter(|(_, lane)| !lane.waiters.is_empty())
            .map(|(tenant, _)| *tenant)
            .min_by_key(|tenant| (self.tenant_service(workload, *tenant), *tenant))?;
        Some((workload, tenant))
    }

    /// Grants free lanes to the next waiters.
    fn dispatch(&mut self) {
        while self.free > 0 {
            let Some((workload, tenant)) = self.next() else {
                return;
            };
            let Some(waiter) = self.workloads[workload.index()]
                .tenants
                .get_mut(&tenant)
                .and_then(|lane| lane.waiters.pop_front())
            else {
                return;
            };
            let id = self.grant(workload, tenant);
            let previous = std::mem::replace(&mut *waiter.state(), WaitState::Granted(id));
            match previous {
                WaitState::Waiting(Wake::Task(waker)) => waker.wake(),
                WaitState::Waiting(Wake::Thread(thread)) => thread.unpark(),
                WaitState::Granted(_) | WaitState::Done => {}
            }
        }
    }
}

/// Marks the current thread as running laned work for one context.
struct Scope {
    /// The work running before this scope.
    previous: Option<Inherited>,
}

impl Scope {
    /// Marks the current thread as running `context`'s work on `lanes`.
    fn enter(lanes: &CpuLanes, context: WorkContext) -> Self {
        let running = Inherited {
            lanes: lanes.clone(),
            context,
        };
        Self {
            previous: RUNNING.replace(Some(running)),
        }
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        RUNNING.set(self.previous.take());
    }
}

/// The future [`CpuLanes::acquire`] awaits.
struct Acquire {
    /// The lanes acquired from.
    lanes: CpuLanes,
    /// Who the lane is for.
    context: WorkContext,
    /// The queued acquisition, once queued.
    waiter: Option<Arc<Waiter>>,
}

impl Future for Acquire {
    type Output = LaneLease;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<LaneLease> {
        let this = &mut *self;
        match poll_lane(&this.lanes, this.context, &mut this.waiter, cx) {
            Poll::Ready(id) => Poll::Ready(LaneLease {
                lanes: this.lanes.clone(),
                id,
            }),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Drop for Acquire {
    fn drop(&mut self) {
        if let Some(waiter) = self.waiter.take() {
            self.lanes.withdraw(&waiter);
        }
    }
}

/// Takes a granted lane, queues for one, or refreshes the queued waker.
fn poll_lane(
    lanes: &CpuLanes,
    context: WorkContext,
    waiter: &mut Option<Arc<Waiter>>,
    cx: &Context<'_>,
) -> Poll<RunId> {
    if let Some(queued) = waiter {
        let mut state = queued.state();
        match &mut *state {
            WaitState::Granted(id) => {
                let id = *id;
                *state = WaitState::Done;
                drop(state);
                *waiter = None;
                return Poll::Ready(id);
            }
            WaitState::Waiting(wake) => {
                *wake = Wake::Task(cx.waker().clone());
                return Poll::Pending;
            }
            WaitState::Done => {
                drop(state);
                *waiter = None;
            }
        }
    }
    match lanes.try_acquire(context, Wake::Task(cx.waker().clone())) {
        Ok(id) => Poll::Ready(id),
        Err(queued) => {
            *waiter = Some(queued);
            Poll::Pending
        }
    }
}

/// A future whose every poll runs on a managed CPU lane.
///
/// Created by [`CpuLanes::laned`]. Each poll takes a lane (or inherits the
/// one its thread already holds), runs the inner poll with the work context
/// current, then returns the lane, so `Pending` on IO or memory holds none.
pub struct Laned<F> {
    /// The lanes acquired from.
    lanes: CpuLanes,
    /// Who the work runs for.
    context: WorkContext,
    /// The queued acquisition while waiting for a lane.
    waiter: Option<Arc<Waiter>>,
    /// The wrapped work.
    future: Pin<Box<F>>,
}

impl<F: Future> Future for Laned<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        let this = &mut *self;
        if RUNNING.with_borrow(Option::is_some) {
            return this.future.as_mut().poll(cx);
        }
        let Poll::Ready(id) = poll_lane(&this.lanes, this.context, &mut this.waiter, cx) else {
            return Poll::Pending;
        };
        let _lease = LaneLease {
            lanes: this.lanes.clone(),
            id,
        };
        let _running = Scope::enter(&this.lanes, this.context);
        this.future.as_mut().poll(cx)
    }
}

impl<F> Drop for Laned<F> {
    fn drop(&mut self) {
        if let Some(waiter) = self.waiter.take() {
            self.lanes.withdraw(&waiter);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::pin::pin;
    use std::sync::mpsc;

    use super::*;
    use crate::resources::{ByteLedger, WorkloadManager};

    /// A work context for `workload` and `tenant`.
    fn context(workload: Workload, tenant: Option<DataTenantId>) -> WorkContext {
        WorkloadManager::new(ByteLedger::new(1), 1, 1)
            .begin(workload, tenant, None)
            .context()
    }

    /// Polls `future` once with a waker that does nothing.
    fn poll_once<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
        future.poll(&mut Context::from_waker(Waker::noop()))
    }

    /// Polls an acquisition once, expecting it to have been granted.
    fn granted<F: Future<Output = LaneLease>>(future: Pin<&mut F>, why: &str) -> LaneLease {
        match poll_once(future) {
            Poll::Ready(lease) => lease,
            Poll::Pending => panic!("{why}"),
        }
    }

    /// Polls an acquisition once, expecting it to wait.
    fn queue<F: Future>(future: Pin<&mut F>) {
        assert!(poll_once(future).is_pending(), "the acquisition waits");
    }

    /// One workload borrows every lane while peers idle; freed lanes then go
    /// to the waiting workloads with the least service, not to arrival order.
    ///
    /// # Panics
    /// Panics when borrowing is refused or a lane goes to the busiest workload.
    #[tokio::test(start_paused = true)]
    async fn idle_peers_lend_every_lane_and_least_service_goes_next() {
        let lanes = CpuLanes::new(2);
        let gateway = context(Workload::Gateway, None);
        let first = lanes.acquire(gateway).await;
        let second = lanes.acquire(gateway).await;
        assert_eq!(lanes.free(), 0, "one workload borrows every lane");

        let mut more_gateway = pin!(lanes.acquire(gateway));
        queue(more_gateway.as_mut());
        let mut verification = pin!(lanes.acquire(context(Workload::Verification, None)));
        queue(verification.as_mut());
        let mut bifrost = pin!(lanes.acquire(context(Workload::Bifrost, None)));
        queue(bifrost.as_mut());

        tokio::time::advance(Duration::from_secs(1)).await;
        drop(first);
        let _verification = granted(verification.as_mut(), "least service, not arrival");
        assert!(poll_once(more_gateway.as_mut()).is_pending());
        drop(second);
        let _bifrost = granted(bifrost.as_mut(), "granted");
        assert!(poll_once(more_gateway.as_mut()).is_pending());
    }

    /// A running lease's elapsed time counts as service before it ends, so a
    /// workload holding a long lease does not look idle to the scheduler.
    ///
    /// # Panics
    /// Panics when a workload with a long running lease wins the next lane.
    #[tokio::test(start_paused = true)]
    async fn running_leases_count_as_service() {
        let lanes = CpuLanes::new(2);
        let gateway = context(Workload::Gateway, None);
        let verification = context(Workload::Verification, None);
        let _long = lanes.acquire(gateway).await;
        let other = lanes.acquire(context(Workload::Bifrost, None)).await;
        let mut first = pin!(lanes.acquire(verification));
        queue(first.as_mut());
        tokio::time::advance(Duration::from_secs(2)).await;
        drop(other);
        let first = granted(first.as_mut(), "verification has no service yet");
        let mut next_verification = pin!(lanes.acquire(verification));
        queue(next_verification.as_mut());
        let mut next_gateway = pin!(lanes.acquire(gateway));
        queue(next_gateway.as_mut());
        tokio::time::advance(Duration::from_secs(3)).await;
        drop(first);
        let _next = granted(
            next_verification.as_mut(),
            "3s served beats gateway's 5s still running",
        );
        assert!(poll_once(next_gateway.as_mut()).is_pending());
    }

    /// A workload returning from idle is rebased to the service floor, so
    /// time spent idle earns no burst of consecutive lanes.
    ///
    /// # Panics
    /// Panics when the returning workload keeps winning on idle credit.
    #[tokio::test(start_paused = true)]
    async fn idle_time_earns_no_credit() {
        let lanes = CpuLanes::new(1);
        let gateway = context(Workload::Gateway, None);
        let verification = context(Workload::Verification, None);
        let lease = lanes.acquire(verification).await;
        tokio::time::advance(Duration::from_secs(10)).await;
        drop(lease);
        let lease = lanes.acquire(gateway).await;
        tokio::time::advance(Duration::from_secs(30)).await;
        drop(lease);

        let running = lanes.acquire(gateway).await;
        let mut next_gateway = pin!(lanes.acquire(gateway));
        queue(next_gateway.as_mut());
        let mut returning = pin!(lanes.acquire(verification));
        queue(returning.as_mut());
        tokio::time::advance(Duration::from_secs(5)).await;
        drop(running);
        let returned = granted(
            returning.as_mut(),
            "rebased to 30s, verification is below gateway's 35s",
        );
        let mut again = pin!(lanes.acquire(verification));
        queue(again.as_mut());
        tokio::time::advance(Duration::from_secs(10)).await;
        drop(returned);
        let _next_gateway = granted(next_gateway.as_mut(), "40s rebased beats no 35s");
        assert!(poll_once(again.as_mut()).is_pending());
    }

    /// Inside one workload, a freed lane goes to the tenant with the least
    /// service.
    ///
    /// # Panics
    /// Panics when the busier tenant wins.
    #[tokio::test(start_paused = true)]
    async fn tenants_share_their_workload_by_service() {
        let lanes = CpuLanes::new(1);
        let busy = context(Workload::Verification, Some(DataTenantId::new_v7()));
        let quiet = context(Workload::Verification, Some(DataTenantId::new_v7()));
        let lease = lanes.acquire(busy).await;
        tokio::time::advance(Duration::from_secs(10)).await;
        drop(lease);
        let running = lanes.acquire(busy).await;
        let mut busy_next = pin!(lanes.acquire(busy));
        queue(busy_next.as_mut());
        let mut quiet_next = pin!(lanes.acquire(quiet));
        queue(quiet_next.as_mut());
        tokio::time::advance(Duration::from_secs(1)).await;
        drop(running);
        let _quiet_next = granted(quiet_next.as_mut(), "granted");
        assert!(poll_once(busy_next.as_mut()).is_pending());
    }

    /// A nested laned poll inherits its parent's lane and context instead of
    /// deadlocking on a second permit, and work pending on memory holds no
    /// lane, so another workload runs meanwhile.
    ///
    /// # Panics
    /// Panics on a recursive-permit deadlock or a lane held while pending.
    #[tokio::test(start_paused = true)]
    async fn nested_polls_inherit_and_pending_returns_the_lane() {
        let lanes = CpuLanes::new(1);
        let verification = context(Workload::Verification, None);
        let inner = lanes.laned(verification, async { WorkContext::current() });
        let outer = lanes.laned(verification, inner);
        let observed = tokio::time::timeout(Duration::from_secs(5), outer)
            .await
            .expect("a nested poll does not acquire again");
        assert_eq!(observed, Some(verification));

        let memory = ByteLedger::new(1);
        let held = memory.try_charge(1).expect("fits");
        let mut waiting = pin!(lanes.laned(verification, async {
            memory.changed().await;
        }));
        assert!(poll_once(waiting.as_mut()).is_pending());
        assert_eq!(lanes.free(), 1, "awaiting memory holds no lane");
        let gateway = lanes.laned(context(Workload::Gateway, None), async {
            WorkContext::current().map(|current| current.workload)
        });
        let ran = tokio::time::timeout(Duration::from_secs(5), gateway)
            .await
            .expect("the free lane runs other work");
        assert_eq!(ran, Some(Workload::Gateway));
        drop(held);
    }

    /// Work spawned from laned work inherits its context and its lanes: a
    /// spawned future and a spawned blocking job each wait for a lane while
    /// every lane is held, then run as the parent's work.
    ///
    /// # Panics
    /// Panics when spawned work runs without a lane or as other work.
    #[tokio::test]
    async fn spawned_work_inherits_the_parents_lanes() {
        let lanes = CpuLanes::new(1);
        let bifrost = context(Workload::Bifrost, None);
        let inherited = lanes
            .laned(bifrost, async { Inherited::current() })
            .await
            .expect("laned work is inherited");
        assert!(
            Inherited::current().is_none(),
            "unlaned code inherits nothing"
        );

        let held = lanes.acquire(context(Workload::Gateway, None)).await;
        let spawned = tokio::spawn(inherited.clone().laned(async { WorkContext::current() }));
        let blocking =
            tokio::task::spawn_blocking(move || inherited.run_blocking(WorkContext::current));
        tokio::time::timeout(Duration::from_secs(5), async {
            while lanes.waiting() < 2 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("both spawned jobs queue for a lane");
        assert!(!spawned.is_finished(), "spawned work waits for a lane");
        drop(held);
        assert_eq!(spawned.await.expect("spawned future"), Some(bifrost));
        assert_eq!(blocking.await.expect("spawned job"), Some(bifrost));
    }

    /// Cancelling a waiter, before or after it is granted, returns nothing
    /// twice and leaks no lane.
    ///
    /// # Panics
    /// Panics when a lane is lost or double counted.
    #[tokio::test(start_paused = true)]
    async fn cancelled_waiters_return_lanes_exactly_once() {
        let lanes = CpuLanes::new(1);
        let held = lanes.acquire(context(Workload::Bifrost, None)).await;
        {
            let mut withdrawn = pin!(lanes.acquire(context(Workload::Gateway, None)));
            queue(withdrawn.as_mut());
            assert_eq!(lanes.waiting(), 1);
        }
        assert_eq!(lanes.waiting(), 0, "a dropped waiter leaves the queue");
        let mut granted = Box::pin(lanes.acquire(context(Workload::Gateway, None)));
        queue(granted.as_mut());
        drop(held);
        assert_eq!(lanes.free(), 0, "the lane went to the waiter");
        drop(granted);
        assert_eq!(lanes.free(), 1, "an untaken grant returns once");
        assert_eq!(lanes.waiting(), 0);
        let remembered: usize = lanes
            .state()
            .workloads
            .iter()
            .map(|workload| workload.tenants.len())
            .sum();
        assert_eq!(remembered, 0, "idle tenants are forgotten");
    }

    /// A blocking job waits on its own thread for a lane, and a nested
    /// blocking job on that thread inherits it.
    ///
    /// # Panics
    /// Panics when the job runs without a lane or nested jobs deadlock.
    #[test]
    fn blocking_jobs_wait_for_a_lane_and_nest() {
        let lanes = CpuLanes::new(1);
        let verification = context(Workload::Verification, None);
        let held = lanes.acquire_blocking(verification);
        let (sender, receiver) = mpsc::channel();
        let worker = {
            let lanes = lanes.clone();
            std::thread::spawn(move || {
                let value = lanes.run_blocking(verification, || {
                    lanes.run_blocking(verification, WorkContext::current)
                });
                sender.send(value).expect("the test is listening");
            })
        };
        assert!(
            receiver.recv_timeout(Duration::from_millis(50)).is_err(),
            "no lane is free yet"
        );
        drop(held);
        let value = receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("the released lane runs the nested job");
        assert_eq!(value, Some(verification));
        worker.join().expect("the worker finishes");
        assert_eq!(lanes.free(), 1);
    }
}
