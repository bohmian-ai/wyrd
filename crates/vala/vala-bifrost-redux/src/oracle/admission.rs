//! Pod-local Oracle query admission and stream-owned resource cleanup.
//!
//! Admission is deliberately local to one process and owns only queue order:
//! one tenant-fair FIFO per class, per-tenant fairness counts, and one bounded
//! set of waiting places. Execution capacity belongs to the pod governor; a
//! waiter is granted only when the governor grants its slot charge, and that
//! query's final resource release returns the slot and the tenant charge
//! together before one capacity wake reconsiders the queue. Asynchronous
//! callers wait only on their own one-shot channel.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::*;

/// Waiting places per Oracle node, shared by both query classes.
///
/// Running queries hold execution slots, not places; only waiters count here.
pub(crate) const DEFAULT_QUEUE_CAPACITY: u32 = 1_000;

/// Longest time a query of either class may wait for an execution slot.
///
/// A waiter also stops at its total query deadline when that comes first.
pub(crate) const DEFAULT_MAX_QUEUE_WAIT: Duration = Duration::from_hours(1);

/// Private pod-local admission limits computed by server boot.
#[derive(Debug, Clone, Copy)]
pub(crate) struct OracleAdmissionConfig {
    /// Protected Interactive slot units; also the Interactive class floor.
    pub(crate) interactive_slots: u32,
    /// Maximum Analytical slot units; zero disables the class entirely.
    pub(crate) analytical_slots: u32,
    /// Fixed pod-local per-tenant Interactive slot-unit cap.
    pub(crate) tenant_interactive_slots: u32,
    /// Fixed pod-local per-tenant Analytical slot-unit cap.
    pub(crate) tenant_analytical_slots: u32,
    /// Total queued waiters across both classes.
    pub(crate) queue_capacity: u32,
    /// Absolute maximum time a waiter may remain queued.
    pub(crate) max_queue_wait: Duration,
}

impl Default for OracleAdmissionConfig {
    fn default() -> Self {
        Self {
            interactive_slots: 8,
            analytical_slots: 4,
            tenant_interactive_slots: 12,
            tenant_analytical_slots: 4,
            queue_capacity: DEFAULT_QUEUE_CAPACITY,
            max_queue_wait: DEFAULT_MAX_QUEUE_WAIT,
        }
    }
}

impl From<&OracleConfig> for OracleAdmissionConfig {
    /// Projects the engine-wide Oracle limits onto the pod-local admission subset.
    ///
    /// Admission consumes only the class, tenant, and queue limits; every other
    /// engine value belongs to planning or execution. Deriving the subset here
    /// keeps one authoritative projection instead of restating the field list at
    /// each construction site.
    fn from(config: &OracleConfig) -> Self {
        Self {
            interactive_slots: config.interactive_slots,
            analytical_slots: config.analytical_slots,
            tenant_interactive_slots: config.tenant_interactive_slots,
            tenant_analytical_slots: config.tenant_analytical_slots,
            queue_capacity: config.queue_capacity,
            max_queue_wait: config.max_queue_wait,
        }
    }
}

/// Aggregate lifecycle report returned by bounded Oracle shutdown.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OracleShutdownReport {
    /// Queries that still own class or tenant capacity at the deadline.
    pub active_queries: u64,
    /// Waiters that remained queued at the deadline.
    pub queued_queries: u64,
    /// Aggregate governed bytes the shared Oracle memory root still holds.
    ///
    /// Actual `DataFusion` reservation, not an admission quantum: an admitted
    /// query that never grew a consumer contributes nothing here.
    pub reserved_memory_bytes: u64,
    /// Running peer reservations observed at shutdown.
    pub peer_running: u64,
}

/// One queued admission request and its absolute grant deadline.
struct Waiter {
    /// Monotonic queue identity used to remove a canceled waiter.
    id: u64,
    /// Fixed deadline computed at enqueue time.
    deadline: Instant,
    /// Fraction of pinned sealed bytes available from the local hot tier.
    local_ratio: f64,
    /// One-shot notification carrying the granted query resources.
    tx: tokio::sync::oneshot::Sender<crate::resources::OracleQueryResources>,
}

/// One winner notification held until the admission mutex is unlocked.
///
/// The resources already carry the tenant charge, so a failed send needs no
/// rollback: dropping them returns the slot and the charge together.
type GrantNotification = (
    tokio::sync::oneshot::Sender<crate::resources::OracleQueryResources>,
    crate::resources::OracleQueryResources,
);

/// FIFO waiters and active count for one locally contending tenant.
struct TenantQueue {
    /// Arrival-ordered requests awaiting a grant.
    waiters: VecDeque<Waiter>,
    /// Number of active grants held by the tenant.
    active: u32,
}

/// Mutex-owned tenant ring and fairness counts for one query class.
///
/// Holds no class slot count: whether a class can seat another query is the
/// pod governor's answer, asked at grant time.
struct ClassState {
    /// Fixed pod-local per-tenant slot-unit cap inside this class.
    tenant_slot_limit: u32,
    /// Stable first-enqueue tenant ring.
    tenants: Vec<(DataTenantId, TenantQueue)>,
    /// Next tenant index visited by round-robin grants.
    cursor: usize,
}

impl ClassState {
    /// Creates an empty tenant ring for one class.
    ///
    /// The tenant cap is clamped to the class maximum `capacity`, because a
    /// tenant can never hold more of a class than the governor lets the class
    /// hold. A zero `capacity` is a valid disabled class.
    fn new(capacity: u32, tenant_slot_limit: u32) -> Self {
        Self {
            tenant_slot_limit: tenant_slot_limit.min(capacity),
            tenants: Vec::new(),
            cursor: 0,
        }
    }

    /// Returns or creates the stable ring position for one tenant.
    fn tenant_index(&mut self, tenant: DataTenantId) -> usize {
        if let Some(index) = self.tenants.iter().position(|(id, _)| *id == tenant) {
            return index;
        }
        self.tenants.push((
            tenant,
            TenantQueue {
                waiters: VecDeque::new(),
                active: 0,
            },
        ));
        self.tenants.len() - 1
    }

    /// Returns the next rotation position whose FIFO head may be granted.
    ///
    /// Rotation starts at the cursor and visits every tenant once, so tenants
    /// take grants with equal weight. A tenant is eligible only when its FIFO
    /// has a head and its fixed slot cap can absorb one more query; this is the
    /// whole tenant-fairness rule, with no contention-dependent ceiling.
    fn next_eligible(&self) -> Option<usize> {
        if self.tenants.is_empty() {
            return None;
        }
        (0..self.tenants.len()).find_map(|step| {
            let index = (self.cursor + step) % self.tenants.len();
            let (_, queue) = &self.tenants[index];
            (!queue.waiters.is_empty() && queue.active < self.tenant_slot_limit).then_some(index)
        })
    }

    /// Returns the monotonic waiter identity at one tenant's FIFO head.
    fn head_waiter_id(&self, index: usize) -> u64 {
        self.tenants[index]
            .1
            .waiters
            .front()
            .map_or(u64::MAX, |waiter| waiter.id)
    }

    /// Drops every FIFO head whose absolute deadline already passed.
    ///
    /// Returns the number removed so the caller can decrement the shared queued
    /// counter once. Only heads are examined, which keeps one call proportional
    /// to the tenants it actually clears rather than to the whole queue; an
    /// entry behind a live head keeps its place until that head leaves. The
    /// grant loop therefore calls this before every grant decision, so an
    /// expired entry exposed by a prior pop is removed on the next iteration
    /// instead of being granted.
    fn prune_expired(&mut self, now: Instant) -> u32 {
        let mut removed = 0;
        for (_, queue) in &mut self.tenants {
            while queue
                .waiters
                .front()
                .is_some_and(|waiter| waiter.deadline <= now)
            {
                queue.waiters.pop_front();
                removed += 1;
            }
        }
        removed
    }
}

/// Complete mutable admission state protected by one synchronous mutex.
struct AdmissionState {
    /// Interactive tenant ring.
    interactive: ClassState,
    /// Analytical tenant ring.
    analytical: ClassState,
    /// Maximum number of queued waiters.
    queue_capacity: u32,
    /// Current queued waiter count.
    queued: u32,
    /// Refresh generation applied to future grants.
    generation: u64,
    /// Whether new admissions are closed.
    closed: bool,
    /// Whether a live Oracle membership exists for new admissions.
    membership_available: bool,
    /// Number of leader-local queries whose tenant charge is still held.
    active_queries: u64,
}

impl AdmissionState {
    /// Builds the mutex-owned queue state from one pod-local configuration.
    ///
    /// Interactive may borrow the whole local total, so its tenant cap is
    /// clamped to that total; Analytical is clamped to its own maximum. The
    /// slot limits themselves are enforced by the governor, not here.
    fn new(config: OracleAdmissionConfig, membership_available: bool) -> Self {
        let total_slot_units = config
            .interactive_slots
            .saturating_add(config.analytical_slots);
        Self {
            interactive: ClassState::new(total_slot_units, config.tenant_interactive_slots),
            analytical: ClassState::new(config.analytical_slots, config.tenant_analytical_slots),
            queue_capacity: config.queue_capacity.max(1),
            queued: 0,
            generation: 0,
            closed: false,
            membership_available,
            active_queries: 0,
        }
    }

    /// Returns the mutable class state charged by one admission class.
    fn class_mut(&mut self, kind: QueryClass) -> &mut ClassState {
        match kind {
            QueryClass::Interactive => &mut self.interactive,
            QueryClass::Analytical => &mut self.analytical,
        }
    }

    /// Closes the queue and drops every waiter's grant channel.
    ///
    /// A dropped channel is what a waiting future observes as refusal, so this
    /// needs no per-waiter wake.
    fn close_queue(&mut self) {
        self.closed = true;
        for (_, tenant) in &mut self.interactive.tenants {
            tenant.waiters.clear();
        }
        for (_, tenant) in &mut self.analytical.tenants {
            tenant.waiters.clear();
        }
        self.queued = 0;
    }
}

/// Shared admission owner state used by guards and waiter futures.
pub(super) struct AdmissionShared {
    /// Synchronous grant/release state lock.
    state: Mutex<AdmissionState>,
    /// Wakeup channel for release and refresh transitions.
    notify: tokio::sync::Notify,
    /// Root cancellation shared with queued waiters.
    root_cancel: CancellationToken,
    /// Absolute queue wait cap.
    max_queue_wait: Duration,
    /// Monotonic waiter identity source.
    next_waiter: AtomicU64,
    /// Narrow Oracle capability that grants complete query envelopes.
    resources: crate::resources::OracleResources,
}

impl AdmissionShared {
    /// Re-runs the grant pass under the admission lock and notifies winners.
    ///
    /// The single capacity waker calls this once per governor capacity change,
    /// whichever pod-local owner — leader or remote follower — returned slots.
    /// Tenant FIFO order and the rotation cursor are untouched: this is the
    /// same pass an enqueue runs, not a second admission path.
    fn regrant(self: &Arc<Self>) {
        let notifications = self
            .state
            .lock()
            .map_or_else(|_| Vec::new(), |mut state| grant_waiters(self, &mut state));
        notify_grants(notifications);
    }

    /// Wakes queued admission once for every governor capacity change.
    ///
    /// This is the queue's only capacity wake. It holds the owner weakly and
    /// stops on admission close, so it never keeps a dropped owner alive. A
    /// poisoned governor can grant nothing again, so the queue is closed and
    /// every waiter is refused rather than left to its deadline. `epoch` is
    /// read before the task is spawned, so a release that lands before the
    /// task's first poll is still observed as a change.
    async fn wake_on_capacity(
        shared: std::sync::Weak<Self>,
        resources: crate::resources::OracleResources,
        mut epoch: u64,
        stop: CancellationToken,
    ) {
        loop {
            let changed = tokio::select! {
                () = stop.cancelled() => return,
                changed = resources.wait_for_capacity_change(epoch) => changed,
            };
            let Some(owner) = shared.upgrade() else {
                return;
            };
            match changed {
                Ok(next) => {
                    epoch = next;
                    owner.regrant();
                }
                Err(error) => {
                    tracing::error!(%error, "Oracle admission closed its queue on a poisoned governor");
                    if let Ok(mut state) = owner.state.lock() {
                        state.close_queue();
                    }
                    owner.notify.notify_waiters();
                    return;
                }
            }
        }
    }
}

/// One leader-local query's tenant fairness charge.
///
/// Attached to the query's [`crate::resources::OracleQueryResources`] at grant
/// time, so it is returned by that owner's final release — after its slot
/// units and before the governor's capacity wake — wherever the owner has
/// moved, including into an Analytical graph. It is the only thing admission
/// charges a running query; slot capacity stays with the governor.
struct TenantCharge {
    /// Admission owner whose tenant ring holds the charge.
    shared: Arc<AdmissionShared>,
    /// Class whose tenant ring is charged.
    class: QueryClass,
    /// Tenant charged by the grant.
    tenant: DataTenantId,
}

impl Drop for TenantCharge {
    /// Returns the tenant count and the active-query count exactly once.
    ///
    /// Runs no grant pass: the governor's capacity wake that follows this drop
    /// is the one reconsideration. An underflow is accounting corruption and
    /// is logged rather than panicking inside a drop.
    fn drop(&mut self) {
        let Ok(mut state) = self.shared.state.lock() else {
            tracing::error!("Oracle admission state lock poisoned during tenant release");
            return;
        };
        let class = state.class_mut(self.class);
        if let Some((_, queue)) = class.tenants.iter_mut().find(|(id, _)| *id == self.tenant) {
            if let Some(active) = queue.active.checked_sub(1) {
                queue.active = active;
            } else {
                tracing::error!("Oracle admission tenant release underflow");
            }
        }
        if let Some(active) = state.active_queries.checked_sub(1) {
            state.active_queries = active;
        } else {
            tracing::error!("Oracle admission active-query release underflow");
        }
        drop(state);
        self.shared.notify.notify_waiters();
    }
}

/// Immutable caller-owned inputs needed to prepare one local admission.
pub(crate) struct PreparedAdmission {
    /// Authenticated tenant identity charged by the local scheduler.
    pub(crate) tenant: DataTenantId,
    /// Closed query class selected by the planner.
    pub(crate) query_class: QueryClass,
    /// Fraction of pinned sealed bytes available from the local hot tier.
    pub(crate) local_ratio: f64,
    /// Absolute query deadline.
    pub(crate) deadline: Instant,
    /// Caller/request cancellation authority.
    pub(crate) cancellation: CancellationToken,
}

/// Queued waiter state transferred from enqueueing into the async grant wait.
struct PreparedWaiter {
    /// Class charged by the eventual grant and cancellation cleanup.
    query_class: QueryClass,
    /// Tenant removed from the queue when cancellation wins.
    tenant: DataTenantId,
    /// Monotonic queue identity used by cancellation cleanup.
    waiter_id: u64,
    /// Fixed absolute deadline selected before queue insertion.
    wait_deadline: Instant,
    /// One-shot grant notification owned by the waiting future.
    receiver: tokio::sync::oneshot::Receiver<crate::resources::OracleQueryResources>,
}

/// Owns one queued place until the waiting future takes its grant.
///
/// Dropping the guard while armed removes the waiter from its tenant FIFO and
/// returns any grant already sent to it. This is what makes a dropped request
/// future — an HTTP client that disconnected while queued — release its place
/// immediately instead of holding it until its deadline or its turn.
struct QueuedWaiter<'a> {
    /// Admission owner whose queue holds the place.
    admission: &'a OracleAdmission,
    /// Queue identity and grant channel of the place.
    waiter: PreparedWaiter,
    /// False once the grant was received and ownership moved to the caller.
    armed: bool,
}

impl Drop for QueuedWaiter<'_> {
    /// Removes the unclaimed place and rolls back a grant that raced the exit.
    fn drop(&mut self) {
        if self.armed {
            self.admission.rollback_waiter(
                self.waiter.query_class,
                self.waiter.tenant,
                self.waiter.waiter_id,
                &mut self.waiter.receiver,
            );
        }
    }
}

/// Admission owner for bounded local capacity and refreshed membership state.
pub struct OracleAdmission {
    /// Local role identity and fence used for peer dispatch authorization.
    pub(super) local_role: RegisteredRole,
    /// Mutex-owned class, tenant, queue, and byte accounting state.
    shared: Arc<AdmissionShared>,
}

/// Leader identity attached to a query while it dispatches peer work.
#[derive(Debug, Clone, Copy)]
pub(super) struct AdmittedLeader {
    /// Physical node identity selected for the query.
    pub(super) node_id: NodeId,
    /// Membership fence observed when admission was granted.
    pub(super) fencing_token: u64,
}

#[cfg(feature = "test-support")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct QueryResourceSnapshot {
    /// Local running slot units retained by this query.
    pub admission_slots: u64,
    /// Query-owned memory bytes retained after live-tail drain.
    ///
    /// The drain moves the tail into this query's own admitted pool, so the
    /// figure is read from that pool rather than from a separate tally: a
    /// second counter could only report zero for a query that is demonstrably
    /// holding bytes.
    pub memory_bytes: u64,
    /// Local peer-worker slot units retained by this query.
    pub peer_slots: u64,
    /// Memory ceiling admission issued this exact query.
    ///
    /// Fixed at admission and never renegotiated, so it is the bound every
    /// other reservation figure here is meaningful against.
    pub granted_memory_bytes: u64,
    /// Bytes this query's own `DataFusion` pool currently holds.
    pub pool_current_bytes: u64,
    /// Largest reservation this query's own pool has ever held.
    pub pool_peak_bytes: u64,
}

#[cfg(feature = "test-support")]
#[derive(Debug)]
pub struct QueryResourceProbe {
    /// Existing Oracle query identity naming this observation.
    query_id: QueryId,
    /// Latest exact resource ownership and notification channel.
    snapshot: tokio::sync::watch::Sender<QueryResourceSnapshot>,
    /// This query's own admitted pool, read live rather than sampled on release.
    pool: Option<Arc<dyn datafusion::execution::memory_pool::MemoryPool>>,
    /// Peak counter the admitted pool writes after each successful growth.
    memory_peak_bytes: Option<Arc<std::sync::atomic::AtomicUsize>>,
    /// Park this query claimed at admission, taken once by its frame loop.
    park: std::sync::Mutex<Option<Arc<crate::resources::OracleQueryPark>>>,
    /// Ceiling admission granted this query's own pool.
    granted_memory_bytes: usize,
}

#[cfg(feature = "test-support")]
impl QueryResourceProbe {
    /// Creates the probe at admission, while the guard still holds its resources.
    ///
    /// Created before any Analytical transfer so the probe names the query's
    /// own pool and peak counter for its whole life. `resources` is absent
    /// only for a synthetic guard that never held an admitted envelope, and
    /// the grant, current, and peak observations are then all zero.
    #[must_use]
    fn new(query_id: QueryId, resources: Option<&crate::resources::OracleQueryResources>) -> Self {
        let slot_units = u64::from(resources.is_some());
        let (snapshot, _) = tokio::sync::watch::channel(QueryResourceSnapshot {
            admission_slots: slot_units,
            memory_bytes: 0,
            peer_slots: slot_units,
            granted_memory_bytes: resources.map_or(0, |resources| {
                u64::try_from(resources.execution().granted_memory_bytes()).unwrap_or(u64::MAX)
            }),
            pool_current_bytes: 0,
            pool_peak_bytes: 0,
        });
        Self {
            query_id,
            snapshot,
            pool: resources.map(|resources| Arc::clone(resources.execution().memory_pool())),
            memory_peak_bytes: resources
                .map(crate::resources::OracleQueryResources::memory_peak_bytes),
            park: std::sync::Mutex::new(resources.and_then(|resources| resources.park.clone())),
            granted_memory_bytes: resources
                .map_or(0, |resources| resources.execution().granted_memory_bytes()),
        }
    }

    /// Parks after this query's first batch frame when a journey armed a park.
    ///
    /// Takes the park once, so only the first batch parks. When the journey
    /// resolves it to exhaust, this grows a fresh consumer on the query's own
    /// admitted pool by one byte past its grant and returns the pool's real
    /// refusal, which the frame loop then settles exactly as a refused
    /// `DataFusion` allocation. Returns `None` when no park was armed or the
    /// journey resumed the query.
    pub(super) async fn park_after_rows(&self) -> Option<datafusion::error::DataFusionError> {
        let park = self.park.lock().ok()?.take()?;
        if !park.park().await {
            return None;
        }
        let pool = self.pool.as_ref()?;
        datafusion::execution::memory_pool::MemoryConsumer::new("oracle-test-exhaustion")
            .register(pool)
            .try_grow(self.granted_memory_bytes.saturating_add(1))
            .err()
    }
    /// Returns the existing Oracle query identity.
    #[must_use]
    pub fn query_id(&self) -> QueryId {
        self.query_id
    }
    /// Returns current exact ownership for this query only.
    ///
    /// Pool current and peak are read from the live owners rather than from the
    /// watch channel, so an observation taken while the query is still running
    /// reports what it holds now rather than what it held at admission.
    #[must_use]
    pub fn snapshot(&self) -> QueryResourceSnapshot {
        let mut snapshot = *self.snapshot.borrow();
        snapshot.pool_current_bytes = self
            .pool
            .as_ref()
            .map_or(0, |pool| u64::try_from(pool.reserved()).unwrap_or(u64::MAX));
        // Released ownership is reported by the watch channel; a live query's
        // held bytes are whatever its own pool holds right now.
        if snapshot.admission_slots > 0 {
            snapshot.memory_bytes = snapshot.pool_current_bytes;
        }
        snapshot.pool_peak_bytes = self.memory_peak_bytes.as_ref().map_or(0, |peak| {
            u64::try_from(peak.load(Ordering::Acquire)).unwrap_or(u64::MAX)
        });
        snapshot
    }
    /// Subscribes to ownership transitions.
    #[must_use]
    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<QueryResourceSnapshot> {
        self.snapshot.subscribe()
    }
    fn release_local(&self) {
        self.snapshot.send_modify(|snapshot| {
            snapshot.admission_slots = 0;
            snapshot.memory_bytes = 0;
            snapshot.peer_slots = 0;
        });
    }
}

impl std::fmt::Debug for OracleAdmission {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OracleAdmission")
            .finish_non_exhaustive()
    }
}

impl OracleAdmission {
    /// Creates a process-local owner and starts its single capacity waker.
    ///
    /// The waker is the queue's only reaction to returned capacity; it runs
    /// until [`Self::close`] and holds the owner weakly.
    ///
    /// # Errors
    /// Returns [`BifrostError::Internal`] when called outside a Tokio runtime.
    pub(super) fn with_config(
        local_role: RegisteredRole,
        membership_available: bool,
        config: OracleAdmissionConfig,
        resources: crate::resources::OracleResources,
    ) -> Result<Self, BifrostError> {
        let runtime =
            tokio::runtime::Handle::try_current().map_err(|_| BifrostError::Internal {
                detail: "Oracle admission requires an active Tokio runtime".to_owned(),
            })?;
        let root_cancel = CancellationToken::new();
        // Admission and the governor must agree on one class split. Installing
        // it here makes that true by construction: the ledger that both leader
        // admission and follower acquisition charge is bounded by exactly the
        // capacity this owner schedules against. The install is idempotent, so
        // boot's earlier install of the same values wins unchanged.
        resources.install_class_split(crate::resources::OracleClassSplit {
            interactive_floor_units: config.interactive_slots,
            analytical_max_units: config.analytical_slots,
        });
        let state = AdmissionState::new(config, membership_available);
        let shared = Arc::new(AdmissionShared {
            state: Mutex::new(state),
            notify: tokio::sync::Notify::new(),
            root_cancel,
            max_queue_wait: config.max_queue_wait,
            next_waiter: AtomicU64::new(1),
            resources,
        });
        runtime.spawn(AdmissionShared::wake_on_capacity(
            Arc::downgrade(&shared),
            shared.resources.clone(),
            shared.resources.capacity_epoch(),
            shared.root_cancel.clone(),
        ));
        Ok(Self { local_role, shared })
    }

    /// Starts the lifecycle task without reconstructing query state.
    ///
    /// # Errors
    /// Returns an internal error when called outside a Tokio runtime.
    pub(super) fn start_maintenance(
        shutdown: CancellationToken,
        ready: Arc<AtomicBool>,
    ) -> Result<(JoinHandle<()>, StartupResultReceiver), BifrostError> {
        let runtime =
            tokio::runtime::Handle::try_current().map_err(|_| BifrostError::Internal {
                detail: "Oracle construction requires an active Tokio runtime".to_owned(),
            })?;
        let (startup_tx, startup_rx) = tokio::sync::oneshot::channel();
        ready.store(true, Ordering::Release);
        let task = runtime.spawn(async move {
            let _ = startup_tx.send(Ok(()));
            shutdown.cancelled().await;
            ready.store(false, Ordering::Release);
        });
        Ok((task, startup_rx))
    }

    /// Returns the target partitions a query with `local_ratio` plans with.
    ///
    /// Derived from this pod's effective CPU and the pinned locality before
    /// admission, so the retained physical root is planned exactly once.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryAdmissionRejected`] when the locality is
    /// outside `[0, 1]` or the pod's CPU count cannot be represented.
    pub(super) fn session_partitions(&self, local_ratio: f64) -> Result<usize, BifrostError> {
        self.shared
            .resources
            .target_partitions(local_ratio)
            .map_err(|_| BifrostError::QueryAdmissionRejected)
    }

    /// Reports whether a live Oracle role and local class capacity are available.
    #[must_use]
    pub fn is_available(&self) -> bool {
        self.shared.resources.class_split().total_units() > 0
            && self
                .shared
                .state
                .lock()
                .is_ok_and(|state| state.membership_available && !state.closed)
    }

    /// Applies a membership/config generation to future grants without revoking guards.
    pub(crate) fn refresh(&self, membership: &crate::cluster::ClusterSnapshot) {
        let notifications = if let Ok(mut state) = self.shared.state.lock() {
            state.membership_available = !membership.live_oracles().is_empty();
            state.generation = state.generation.wrapping_add(1);
            grant_waiters(&self.shared, &mut state)
        } else {
            Vec::new()
        };
        notify_grants(notifications);
        self.shared.notify.notify_waiters();
    }

    /// Closes future admissions, refuses queued waiters, and stops the waker.
    pub(crate) fn close(&self) {
        if let Ok(mut state) = self.shared.state.lock() {
            state.close_queue();
        }
        self.shared.root_cancel.cancel();
        self.shared.notify.notify_waiters();
    }

    /// Closes admission, cancels queued work, and reports residual local state by a deadline.
    pub(crate) async fn shutdown(&self, deadline: Instant) -> OracleShutdownReport {
        self.close();
        loop {
            let done = self
                .shared
                .state
                .lock()
                .map_or(true, |state| state.active_queries == 0);
            if done || Instant::now() >= deadline {
                break;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            let _ = tokio::time::timeout(remaining, self.shared.notify.notified()).await;
        }
        let state = self
            .shared
            .state
            .lock()
            .expect("Oracle admission state invariant");
        OracleShutdownReport {
            active_queries: state.active_queries,
            queued_queries: u64::from(state.queued),
            reserved_memory_bytes: u64::try_from(self.shared.resources.shared_memory_reserved())
                .unwrap_or(u64::MAX),
            peer_running: self.shared.resources.live_slot_units(),
        }
    }

    /// Captures current local admission and peer reservations without waiting.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn runtime_inspection(&self) -> OracleRuntimeInspection {
        let state = self
            .shared
            .state
            .lock()
            .expect("Oracle admission state invariant");
        OracleRuntimeInspection {
            active_queries: state.active_queries,
            queued_queries: u64::from(state.queued),
            reserved_memory_bytes: u64::try_from(self.shared.resources.shared_memory_reserved())
                .unwrap_or(u64::MAX),
            peer_running: self.shared.resources.live_slot_units(),
        }
    }

    /// Waits in the tenant-fair queue for one governor slot grant.
    ///
    /// # Errors
    /// Returns [`BifrostError::QueryQueueFull`] when every waiting place is
    /// taken, admission rejection when closed or cancelled, and
    /// [`BifrostError::QueryTimeout`] when the absolute wait deadline expires
    /// before a grant.
    #[cfg(test)]
    #[tracing::instrument(name = "bifrost.oracle.admission", skip_all)]
    pub(super) async fn admit(
        self: &Arc<Self>,
        request: PreparedAdmission,
    ) -> Result<AdmittedQueryGuard, BifrostError> {
        self.admit_with_attempt_id(request, None).await
    }

    /// Acquires admission while preserving an ingress-fixed participant-cut identity.
    ///
    /// # Errors
    /// Returns the same refusals as the common admission path.
    pub(super) async fn admit_for_attempt(
        self: &Arc<Self>,
        request: PreparedAdmission,
        attempt_id: QueryId,
    ) -> Result<AdmittedQueryGuard, BifrostError> {
        self.admit_with_attempt_id(request, Some(attempt_id)).await
    }

    /// Applies the common bounded admission path with an optional fixed identity.
    ///
    /// # Errors
    /// Returns the enqueue refusal, the waiting refusal or timeout, or
    /// [`BifrostError::OracleRoleUnavailable`] when the role was fenced out
    /// after the grant.
    async fn admit_with_attempt_id(
        self: &Arc<Self>,
        request: PreparedAdmission,
        attempt_id: Option<QueryId>,
    ) -> Result<AdmittedQueryGuard, BifrostError> {
        let PreparedAdmission {
            tenant,
            query_class,
            local_ratio,
            deadline,
            cancellation,
        } = request;
        let enqueue = Instant::now();
        let wait_deadline = deadline.min(enqueue + self.max_queue_wait());
        let waiter_telemetry = OracleTelemetry::start_admission_waiter(query_class);
        let waiter = self.enqueue_waiter(tenant, query_class, local_ratio, wait_deadline)?;
        let resources = self
            .wait_for_grant(waiter, cancellation.clone(), query_class)
            .await?;
        OracleTelemetry::record_admission(
            query_class,
            OracleAdmissionOutcome::Admitted,
            OracleAdmissionReason::ClassCapacity,
        );
        let mut waiter_telemetry = waiter_telemetry;
        waiter_telemetry.finish("class", "acquired");
        self.build_admitted_guard(tenant, resources, cancellation, attempt_id)
    }

    /// Enqueues one waiter and immediately grants any newly eligible requests.
    ///
    /// This is the one capacity refusal: a full queue returns
    /// [`BifrostError::QueryQueueFull`] immediately. Nothing ahead of it
    /// reserves or waits for a place.
    ///
    /// # Errors
    /// Returns an internal error when the admission state lock is poisoned,
    /// [`BifrostError::OracleRoleUnavailable`] without live membership,
    /// query-admission rejection when the class is disabled or admission is
    /// closed, and [`BifrostError::QueryQueueFull`] when every waiting place is
    /// taken.
    fn enqueue_waiter(
        &self,
        tenant: DataTenantId,
        query_class: QueryClass,
        local_ratio: f64,
        wait_deadline: Instant,
    ) -> Result<PreparedWaiter, BifrostError> {
        let (tx, receiver) = tokio::sync::oneshot::channel();
        let waiter_id = self.shared.next_waiter.fetch_add(1, Ordering::Relaxed);
        let notifications = {
            let mut state = self
                .shared
                .state
                .lock()
                .map_err(|_| BifrostError::Internal {
                    detail: "Oracle admission state lock poisoned".to_owned(),
                })?;
            if !state.membership_available {
                OracleTelemetry::record_admission(
                    query_class,
                    OracleAdmissionOutcome::Rejected,
                    OracleAdmissionReason::Membership,
                );
                tracing::warn!(
                    ?query_class,
                    "Oracle admission refused: this node holds no live Oracle membership"
                );
                return Err(BifrostError::OracleRoleUnavailable);
            }
            // A class that cannot seat one query is not a queue that is
            // momentarily full; it is a class this pod does not offer. Rejecting
            // here keeps a caller from waiting out its whole deadline, and keeps
            // a query view and every peer contact off the path.
            if query_class == QueryClass::Analytical
                && !self.shared.resources.class_split().admits_analytical()
            {
                OracleTelemetry::record_admission(
                    query_class,
                    OracleAdmissionOutcome::Rejected,
                    OracleAdmissionReason::ClassCapacity,
                );
                return Err(BifrostError::QueryAdmissionRejected);
            }
            if state.closed {
                OracleTelemetry::record_admission(
                    query_class,
                    OracleAdmissionOutcome::Rejected,
                    OracleAdmissionReason::Shutdown,
                );
                return Err(BifrostError::QueryAdmissionRejected);
            }
            if state.queued >= state.queue_capacity {
                OracleTelemetry::record_admission(
                    query_class,
                    OracleAdmissionOutcome::Rejected,
                    OracleAdmissionReason::QueueFull,
                );
                tracing::warn!(
                    reason = "queue_full",
                    queued = state.queued,
                    capacity = state.queue_capacity,
                    "Oracle refused a query because every waiting place is taken"
                );
                return Err(BifrostError::QueryQueueFull);
            }
            let class = state.class_mut(query_class);
            let index = class.tenant_index(tenant);
            class.tenants[index].1.waiters.push_back(Waiter {
                id: waiter_id,
                deadline: wait_deadline,
                local_ratio,
                tx,
            });
            state.queued += 1;
            grant_waiters(&self.shared, &mut state)
        };
        notify_grants(notifications);
        Ok(PreparedWaiter {
            query_class,
            tenant,
            waiter_id,
            wait_deadline,
            receiver,
        })
    }

    /// Waits for a grant while preserving absolute deadlines and rollback.
    ///
    /// The queued place is owned by a [`QueuedWaiter`] guard, so every exit
    /// that does not take the grant — deadline, cancellation, shutdown, or the
    /// caller dropping this future when its client disconnects — removes the
    /// waiter and returns any grant that raced the exit. Returned capacity is
    /// not watched here: the owner's single capacity waker runs the grant pass
    /// and this future only receives its outcome.
    ///
    /// # Errors
    /// Returns [`BifrostError::QueryTimeout`] when the earlier of the queue
    /// limit and the total deadline passes, and query-admission rejection when
    /// the grant channel closes or either cancellation token wins the race.
    async fn wait_for_grant(
        &self,
        waiter: PreparedWaiter,
        cancellation: CancellationToken,
        query_class: QueryClass,
    ) -> Result<crate::resources::OracleQueryResources, BifrostError> {
        let wait_deadline = waiter.wait_deadline;
        let mut queued = QueuedWaiter {
            admission: self,
            waiter,
            armed: true,
        };
        let (reason, error) = tokio::select! {
            grant = &mut queued.waiter.receiver => {
                queued.armed = false;
                return grant.map_err(|_| BifrostError::QueryAdmissionRejected);
            }
            () = tokio::time::sleep_until(tokio::time::Instant::from_std(wait_deadline)) => {
                // The earlier of the queue limit and the total deadline has
                // passed: a timeout, not a capacity refusal to retry.
                (OracleAdmissionReason::QueueDeadline, BifrostError::QueryTimeout)
            }
            () = cancellation.cancelled() => {
                (OracleAdmissionReason::Shutdown, BifrostError::QueryAdmissionRejected)
            }
            () = self.shared.root_cancel.cancelled() => {
                (OracleAdmissionReason::Shutdown, BifrostError::QueryAdmissionRejected)
            }
        };
        drop(queued);
        OracleTelemetry::record_admission(query_class, OracleAdmissionOutcome::Rejected, reason);
        Err(error)
    }

    /// Removes a canceled waiter and releases any raced grant.
    ///
    /// A raced grant is simply dropped: its resources carry the tenant charge,
    /// so the drop returns the slot and the charge and wakes the queue.
    fn rollback_waiter(
        &self,
        query_class: QueryClass,
        tenant: DataTenantId,
        waiter_id: u64,
        receiver: &mut tokio::sync::oneshot::Receiver<crate::resources::OracleQueryResources>,
    ) {
        self.cancel_waiter(query_class, tenant, waiter_id);
        drop(receiver.try_recv());
    }

    /// Converts one granted resource owner into the stream-owned lifecycle guard.
    ///
    /// # Errors
    /// Returns [`BifrostError::OracleRoleUnavailable`] when this role was fenced
    /// out after local admission; the resources are released on that path.
    fn build_admitted_guard(
        &self,
        tenant: DataTenantId,
        resources: crate::resources::OracleQueryResources,
        cancellation: CancellationToken,
        attempt_id: Option<QueryId>,
    ) -> Result<AdmittedQueryGuard, BifrostError> {
        let local_node = self.local_role.key.node_id;
        let membership_available = self
            .shared
            .state
            .lock()
            .is_ok_and(|state| state.membership_available);
        if !membership_available {
            drop(resources);
            return Err(BifrostError::OracleRoleUnavailable);
        }
        let query_id = attempt_id.unwrap_or_else(|| QueryId::new(uuid::Uuid::now_v7()));
        Ok(AdmittedQueryGuard {
            analytical: None,
            query_id,
            tenant,
            leader: AdmittedLeader {
                node_id: local_node,
                fencing_token: self.local_role.fencing_token,
            },
            physical_projections: Vec::new(),
            #[cfg(feature = "test-support")]
            resource_probe: Some(Arc::new(QueryResourceProbe::new(
                query_id,
                Some(&resources),
            ))),
            resources: Some(resources),
            live_reservations: Vec::new(),
            cancellation: self.shared.root_cancel.child_token(),
            request_cancellation: cancellation,
            distributed_settlement: Arc::new(DistributedQuerySettlement::default()),
        })
    }

    /// Returns the immutable absolute queue wait cap.
    fn max_queue_wait(&self) -> Duration {
        self.shared.max_queue_wait
    }

    /// Removes one canceled waiter from its tenant FIFO.
    ///
    /// Runs no grant pass: a departing waiter frees a queue place, not
    /// execution capacity.
    fn cancel_waiter(&self, query_class: QueryClass, tenant: DataTenantId, waiter_id: u64) {
        if let Ok(mut state) = self.shared.state.lock() {
            let class = state.class_mut(query_class);
            if let Some((_, queue)) = class.tenants.iter_mut().find(|(id, _)| *id == tenant)
                && let Some(index) = queue
                    .waiters
                    .iter()
                    .position(|waiter| waiter.id == waiter_id)
            {
                queue.waiters.remove(index);
                state.queued = state.queued.saturating_sub(1);
            }
        }
    }
}

/// Grants queued waiters under the pod-local fairness rules while state is held.
///
/// One grant is decided per iteration so tenant rotation and the class
/// comparison stay honest as capacity changes underneath them:
///
/// 1. every FIFO head whose absolute deadline passed is dropped, because a dead
///    head must never block a live tenant behind it and an expired entry must
///    never be granted; this runs on each iteration against a freshly read
///    instant, so both an entry uncovered by a prior grant and an entry that
///    expired while an earlier grant in the same pass was decided are checked;
/// 2. the older of the two eligible class heads is offered first, compared by
///    the monotonic waiter identity, so neither a continuously ready class
///    starves the other nor an idle Analytical queue strands capacity;
/// 3. the pod governor decides whether that class can run. It alone enforces
///    the Interactive floor, Interactive borrowing, and the Analytical maximum
///    for leaders and remote followers alike.
///
/// A class the governor refuses is not retried in this pass: the refusal is a
/// shared-ledger condition, not a queue-order condition, and the next capacity
/// wake re-runs the pass.
fn grant_waiters(
    shared: &Arc<AdmissionShared>,
    state: &mut AdmissionState,
) -> Vec<GrantNotification> {
    grant_waiters_with_clock(shared, state, Instant::now)
}

/// Runs one grant pass, reading the decision clock through `now`.
///
/// This is the whole body of [`grant_waiters`]; the only reason it is separate
/// is that `now` must be a real source rather than a single captured instant,
/// so a test can script a clock that advances between two decisions in one
/// pass. Production always passes [`Instant::now`], and nothing else about the
/// pass changes.
fn grant_waiters_with_clock(
    shared: &Arc<AdmissionShared>,
    state: &mut AdmissionState,
    mut now: impl FnMut() -> Instant,
) -> Vec<GrantNotification> {
    let _ = state.generation;
    let mut notifications = Vec::new();
    let mut refused = [false; 2];
    loop {
        // Deadline eligibility is re-established before every grant decision,
        // not once per pass, and against current monotonic time rather than the
        // instant the pass began. A request's absolute deadline is
        // `min(caller deadline, enqueue + max_queue_wait)`, so deadlines are
        // neither monotonic inside one tenant FIFO nor guaranteed to outlive the
        // grants decided ahead of them: popping a live head can expose an entry
        // that already expired, and an entry can expire while an earlier grant
        // in this same pass is being decided. Re-reading the clock and pruning
        // here removes both before they can be selected, acquire slot
        // ownership, or be notified.
        let now = now();
        let expired = state.interactive.prune_expired(now) + state.analytical.prune_expired(now);
        state.queued = state.queued.saturating_sub(expired);
        let interactive = (!refused[0])
            .then(|| state.interactive.next_eligible())
            .flatten();
        let analytical = (!refused[1])
            .then(|| state.analytical.next_eligible())
            .flatten();
        let (kind, index) = match (interactive, analytical) {
            (Some(index), Some(other)) => {
                if state.interactive.head_waiter_id(index) <= state.analytical.head_waiter_id(other)
                {
                    (QueryClass::Interactive, index)
                } else {
                    (QueryClass::Analytical, other)
                }
            }
            (Some(index), None) => (QueryClass::Interactive, index),
            (None, Some(index)) => (QueryClass::Analytical, index),
            (None, None) => break,
        };
        let local_ratio = state.class_mut(kind).tenants[index]
            .1
            .waiters
            .front()
            .expect("an eligible tenant has a FIFO head")
            .local_ratio;
        let Ok(resources) = acquire_waiter_resources(shared, kind, local_ratio) else {
            refused[usize::from(kind == QueryClass::Analytical)] = true;
            continue;
        };
        let waiter = state.class_mut(kind).tenants[index]
            .1
            .waiters
            .pop_front()
            .expect("an eligible tenant has a FIFO head");
        state.queued = state.queued.saturating_sub(1);
        let resources = charge_tenant(shared, state, kind, index, resources);
        let class = state.class_mut(kind);
        // Advance past the tenant that just took a grant, never past the whole
        // ring, so the next grant goes to the following eligible tenant.
        class.cursor = (index + 1) % class.tenants.len();
        notifications.push((waiter.tx, resources));
    }
    notifications
}

/// Charges one tenant for a governor grant and attaches that charge to it.
///
/// The charge is attached to the query's own resource owner, so its return is
/// the owner's final release and cannot happen early or twice.
fn charge_tenant(
    shared: &Arc<AdmissionShared>,
    state: &mut AdmissionState,
    kind: QueryClass,
    index: usize,
    mut resources: crate::resources::OracleQueryResources,
) -> crate::resources::OracleQueryResources {
    state.active_queries += 1;
    let class = state.class_mut(kind);
    class.tenants[index].1.active += 1;
    resources.attach_admission_charge(Box::new(TenantCharge {
        shared: Arc::clone(shared),
        class: kind,
        tenant: class.tenants[index].0,
    }));
    resources
}

/// Asks the pod governor for one queued admission class's slot charge.
///
/// What is acquired is the query's one slot unit; governed memory is charged
/// later, as the query's shared-pool consumers actually grow.
///
/// # Errors
///
/// Returns [`crate::resources::BifrostResourceError`] when the root governor
/// has no slot unit left for the class.
fn acquire_waiter_resources(
    shared: &AdmissionShared,
    kind: QueryClass,
    local_ratio: f64,
) -> Result<crate::resources::OracleQueryResources, crate::resources::BifrostResourceError> {
    shared
        .resources
        .try_acquire_query(crate::resources::OracleResourceRequest::for_class(
            kind,
            local_ratio,
        ))
}

/// Notifies winners after the admission mutex is released.
///
/// A winner whose future already left returns its grant by dropping it; the
/// resources' release returns the slot and tenant charge and wakes the queue.
fn notify_grants(notifications: Vec<GrantNotification>) {
    for (sender, resources) in notifications {
        drop(sender.send(resources));
    }
}

/// Stream-owned local capacity cleanup.
pub(super) struct AdmittedQueryGuard {
    /// Stable query identity used by peer dispatch and test probes.
    pub(super) query_id: QueryId,
    /// Fenced leader selected at admission time.
    pub(super) leader: AdmittedLeader,
    /// Tenant whose fairness charge the resources carry.
    tenant: DataTenantId,
    /// Physical table projections charged to this query's admitted pool.
    physical_projections: Vec<
        crate::catalog::PhysicalTableProjection<crate::resources::OracleQueryMemoryReservation>,
    >,
    /// The governor's slot charge with this query's tenant charge attached.
    ///
    /// `None` once an Analytical graph has taken it; the graph's final release
    /// then returns both charges.
    resources: Option<crate::resources::OracleQueryResources>,
    /// Parent reservations retaining drained live batches through stream cleanup.
    pub(super) live_reservations: Vec<crate::resources::OracleQueryMemoryReservation>,
    /// Analytical graph and attempt ownership retained until cleanup.
    ///
    /// Present only on an attempt that the distributed Analytical path leased
    /// a session for. Dropping this guard drops those owners, which
    /// is what returns the exchange-buffer and memory children to the query
    /// envelope and the envelope to the root capability.
    pub(super) analytical: Option<super::analytical::AnalyticalAttemptOwnership>,
    /// Cancellation shared with stream and peer dispatch.
    pub(super) cancellation: CancellationToken,
    /// Caller/request cancellation retained for admission ownership.
    pub(super) request_cancellation: CancellationToken,
    /// Query-scoped join owner for every started distributed partition.
    pub(super) distributed_settlement: Arc<DistributedQuerySettlement>,
    /// Query-keyed lifecycle observation retained through cleanup.
    #[cfg(feature = "test-support")]
    pub(super) resource_probe: Option<Arc<QueryResourceProbe>>,
}

/// Query-scoped join state preventing a distributed child from outliving admission.
#[derive(Debug, Default)]
pub(super) struct DistributedQuerySettlement {
    /// Active partition futures guarded with cancellation admission.
    active: std::sync::Mutex<usize>,
    /// Wakes the terminal owner after the last partition future settles.
    settled: tokio::sync::Notify,
}

impl DistributedQuerySettlement {
    /// Cancels admission for new work and waits until every started partition settles.
    pub(super) async fn cancel_and_join(&self, cancellation: &CancellationToken) {
        {
            let _active = self.active.lock();
            cancellation.cancel();
        }
        self.join().await;
    }

    /// Waits until no started partition future remains.
    ///
    /// The waiter is registered before the active count is observed.
    /// [`tokio::sync::Notify::notify_waiters`] wakes only waiters that are
    /// already registered, and a `Notified` future does not register until it
    /// is first polled. Observing the count first would therefore lose the
    /// wakeup from a guard that drops between the observation and the await,
    /// leaving this join parked forever while the query holds admission.
    pub(super) async fn join(&self) {
        loop {
            let notified = self.settled.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.active.lock().map_or(true, |active| *active == 0) {
                return;
            }
            notified.await;
        }
    }
}

impl Drop for AdmittedQueryGuard {
    /// Cancels the query and synchronously releases leader-local resources.
    fn drop(&mut self) {
        self.cancellation.cancel();
        self.live_reservations.clear();
        self.physical_projections.clear();
        self.analytical.take();
        drop(self.resources.take());
        #[cfg(feature = "test-support")]
        if let Some(probe) = &self.resource_probe {
            probe.release_local();
        }
    }
}

impl AdmittedQueryGuard {
    /// Waits, bounded, for every task-owned child of this failed query's
    /// envelope to drop before admission is released.
    ///
    /// Dropping a failed plan aborts its spawned `DataFusion` partition tasks,
    /// but the runtime drops an aborted task — and the memory
    /// reservation it holds — only on a later poll. Releasing the envelope in
    /// that window would poison the process governor for a teardown that is
    /// merely in progress. This first returns the reservations the guard itself
    /// retains, exactly as [`Self::release`] would, then polls the envelope on
    /// the same bounded schedule a follower graph drains on. A child still
    /// alive after the wait is a real leak, and the following release poisons
    /// as before.
    pub(super) async fn drain_children(&mut self) {
        self.live_reservations.clear();
        self.physical_projections.clear();
        for _ in 0..super::analytical::GRAPH_DRAIN_POLLS {
            if self.children_idle() {
                return;
            }
            tokio::time::sleep(super::analytical::GRAPH_DRAIN_INTERVAL).await;
        }
        tracing::warn!(
            query_id = ?self.query_id,
            "Oracle query children did not drain before admission release"
        );
    }

    /// Reports whether the admitted envelope has no live nested child.
    ///
    /// A released, moved, or poisoned envelope reports idle so the caller
    /// proceeds to release, which owns reporting those states.
    fn children_idle(&self) -> bool {
        self.resources
            .as_ref()
            .is_none_or(crate::resources::OracleQueryResources::nested_idle)
    }

    /// Takes the Analytical ownership so the stream can settle it.
    ///
    /// Taking rather than borrowing is deliberate: settlement consumes the two
    /// guards, and removing them here means a later drop of this admission
    /// cannot settle or release the same attempt a second time.
    pub(super) fn take_analytical(
        &mut self,
    ) -> Option<super::analytical::AnalyticalAttemptOwnership> {
        self.analytical.take()
    }

    /// Releases this query's physical projections before its Analytical graph
    /// retains the guard.
    ///
    /// The projections are children of the very envelope the graph returns on
    /// release, and the graph releases only once that envelope has no live
    /// children. A retained guard still holding them would make the graph wait
    /// on itself until its deadline and strand its capacity on this node.
    pub(super) fn release_physical_projections(&mut self) {
        self.physical_projections.clear();
    }

    /// Installs one already-admitted envelope for an ownership-transfer test.
    #[cfg(test)]
    pub(super) fn install_query_resources_for_test(
        &mut self,
        resources: crate::resources::OracleQueryResources,
    ) {
        self.resources = Some(resources);
    }

    /// Takes this query's admitted envelope so its Analytical graph can own it.
    ///
    /// An Analytical leader must not admit a second envelope for a query that
    /// already holds one, so the graph is registered with *this* envelope rather
    /// than a fresh acquisition. Taking rather than sharing keeps a single
    /// owner: from here the graph releases it, and this guard's own drop only
    /// carries nothing: the tenant charge travels inside the envelope.
    ///
    /// Returns `None` when the envelope was already taken, which is the
    /// refusal path — a query about to be rejected has no envelope to lend.
    pub(super) fn take_query_resources(
        &mut self,
    ) -> Option<crate::resources::OracleQueryResources> {
        self.resources.take()
    }

    /// Returns a taken envelope to the guard it was taken from.
    ///
    /// Only the graph-registration refusal uses this: registration hands the
    /// envelope back instead of consuming it, and the guard is still the
    /// query's owner at that point, so putting it back is what lets the
    /// ordinary terminal release return the grant. A guard that is already
    /// holding an envelope drops the returned one rather than overwriting a
    /// live owner.
    pub(super) fn restore_query_resources(
        &mut self,
        resources: crate::resources::OracleQueryResources,
    ) {
        if self.resources.is_none() {
            self.resources = Some(resources);
        }
    }

    /// Records that result data left this query, fencing any later retry.
    ///
    /// A retry is only sound while nothing has been handed to the client. The
    /// leader stream calls this immediately before its first batch frame is
    /// yielded, so the fence is set by the act of egress rather than by a
    /// caller remembering to set it.
    pub(super) fn record_analytical_egress(&self) {
        if let Some(ownership) = self.analytical.as_ref() {
            ownership.record_egress();
        }
    }

    /// Materializes every pinned physical table under this admitted query owner.
    ///
    /// Each table's exact fixed bytes are split from the live query pool before
    /// allocation. Completed projections remain with the stream guard and are
    /// released before its aggregate local permit on every terminal path.
    ///
    /// # Errors
    ///
    /// Returns query admission rejection when logical validation, checked byte
    /// planning, or an exact child split fails, or when the derived projection
    /// disagrees with the catalog-pinned physical binding.
    pub(super) fn retain_physical_projections(
        &mut self,
        cuts: &[crate::catalog::PinnedSealedTable],
    ) -> Result<(), BifrostError> {
        self.retain_physical_bindings(cuts.iter().map(|cut| &cut.binding))
    }

    /// Retains exact physical projections derived from bounded pinned bindings.
    ///
    /// The exact-size iterator ensures allocation cardinality is inherited from
    /// the already bounded query plan rather than introduced by this seam.
    ///
    /// # Errors
    ///
    /// Returns query admission rejection under the same validation, child
    /// split, and binding-consistency failures as [`Self::retain_physical_projections`].
    fn retain_physical_bindings<'a>(
        &mut self,
        bindings: impl ExactSizeIterator<Item = &'a crate::catalog::TenantTableBinding>,
    ) -> Result<(), BifrostError> {
        let resources = self
            .resources
            .as_ref()
            .ok_or(BifrostError::QueryAdmissionRejected)?;
        let mut projections = Vec::with_capacity(bindings.len());
        for binding in bindings {
            let logical = crate::catalog::LogicalTableIdentity::try_new(
                &self.tenant,
                &binding.tenant,
                &binding.table_ref,
            )
            .map_err(|_| BifrostError::QueryAdmissionRejected)?;
            let projection = logical
                .try_project(|facts| {
                    resources
                        .try_split_memory("oracle_physical_table_projection", facts.material_bytes)
                })
                .map_err(|_| BifrostError::QueryAdmissionRejected)?;
            if projection.object_prefix() != binding.object_prefix
                || projection.namespace() != binding.iceberg_namespace.to_string()
            {
                return Err(BifrostError::QueryAdmissionRejected);
            }
            projections.push(projection);
        }
        self.physical_projections.extend(projections);
        Ok(())
    }

    /// Returns the execution this query's exact envelope was issued with.
    ///
    /// `None` once the envelope has moved to an Analytical graph.
    #[must_use]
    pub(super) fn execution(&self) -> Option<&crate::resources::OracleExecution> {
        self.resources
            .as_ref()
            .map(crate::resources::OracleQueryResources::execution)
    }

    /// Builds the sole execution `TaskContext` for a graphless retained root.
    ///
    /// The admitted execution supplies the runtime, memory pool, partitions,
    /// and sort-merge reservation. Planning used the same partition function
    /// the grant did, so applying the grant's shape never reshapes a final plan.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryAdmissionRejected`] when the admitted
    /// envelope has already moved to an Analytical graph.
    pub(super) fn execution_session(
        &self,
        config: datafusion::prelude::SessionConfig,
    ) -> Result<datafusion::prelude::SessionContext, BifrostError> {
        let execution = self
            .execution()
            .ok_or(BifrostError::QueryAdmissionRejected)?;
        Ok(SessionContext::new_with_state(
            execution.session_state(config),
        ))
    }

    /// Returns the query-keyed ownership probe after live-tail drain completes.
    ///
    /// The probe was created at admission so it names this query's own pool
    /// even after an Analytical transfer; this records the live-tail bytes the
    /// guard retains now.
    #[cfg(feature = "test-support")]
    pub(super) fn attach_resource_probe(&mut self) -> Arc<QueryResourceProbe> {
        let memory_bytes = self
            .live_reservations
            .iter()
            .map(|reservation| reservation.bytes() as u64)
            .sum();
        let query_id = self.query_id;
        let probe = self
            .resource_probe
            .get_or_insert_with(|| Arc::new(QueryResourceProbe::new(query_id, None)));
        probe
            .snapshot
            .send_modify(|snapshot| snapshot.memory_bytes = memory_bytes);
        Arc::clone(probe)
    }

    /// Cancels the query and releases all local resources synchronously.
    pub(super) fn release(mut self) {
        self.cancellation.cancel();
        self.analytical.take();
        self.live_reservations.clear();
        self.physical_projections.clear();
        drop(self.resources.take());
        #[cfg(feature = "test-support")]
        if let Some(probe) = &self.resource_probe {
            probe.release_local();
        }
    }
}

#[cfg(test)]
/// Builds one real stream-owned guard for lifecycle tests without a cluster dependency.
///
/// The guard holds a real governor slot grant with its tenant charge attached,
/// exactly as a queued grant would, so releasing it drives the production
/// return path.
///
/// # Panics
/// Panics when the fixture governor cannot grant one Interactive slot.
pub(super) fn admitted_guard_for_test()
-> (AdmittedQueryGuard, Arc<AdmissionShared>, CancellationToken) {
    let shared = Arc::new(AdmissionShared {
        state: Mutex::new(AdmissionState::new(
            OracleAdmissionConfig {
                interactive_slots: 1,
                analytical_slots: 0,
                tenant_interactive_slots: 1,
                tenant_analytical_slots: 0,
                queue_capacity: 1,
                max_queue_wait: Duration::from_secs(1),
            },
            true,
        )),
        notify: tokio::sync::Notify::new(),
        root_cancel: CancellationToken::new(),
        max_queue_wait: Duration::from_secs(1),
        next_waiter: AtomicU64::new(1),
        resources: crate::resources::BifrostRuntimeResources::composed_for_test(
            1024 * 1024 * 1024,
            1024 * 1024 * 1024,
            [crate::resources::BifrostRole::Oracle],
        )
        .oracle()
        .expect("composition must enable the Oracle capability"),
    });
    let tenant = DataTenantId::new_v7();
    let granted = acquire_waiter_resources(&shared, QueryClass::Interactive, 1.0)
        .expect("the fixture governor grants one Interactive slot");
    let mut state = shared.state.lock().expect("state");
    let index = state.interactive.tenant_index(tenant);
    let resources = charge_tenant(&shared, &mut state, QueryClass::Interactive, index, granted);
    drop(state);
    let cancellation = shared.root_cancel.child_token();
    let request_cancellation = CancellationToken::new();
    let query_id = QueryId::new(uuid::Uuid::now_v7());
    (
        AdmittedQueryGuard {
            analytical: None,
            query_id,
            tenant,
            leader: AdmittedLeader {
                node_id: NodeId::new(uuid::Uuid::now_v7()),
                fencing_token: 1,
            },
            physical_projections: Vec::new(),
            #[cfg(feature = "test-support")]
            resource_probe: Some(Arc::new(QueryResourceProbe::new(
                query_id,
                Some(&resources),
            ))),
            resources: Some(resources),
            live_reservations: Vec::new(),
            cancellation: cancellation.clone(),
            request_cancellation: request_cancellation.clone(),
            distributed_settlement: Arc::new(DistributedQuerySettlement::default()),
        },
        shared,
        request_cancellation,
    )
}

#[cfg(test)]
/// Builds one production admission owner over an explicit resource capability.
///
/// Shared with the Analytical lifecycle tests, which need a real admission
/// owner rather than a fabricated guard: proving that a retained graph keeps a
/// queued waiter blocked requires the queue, the class ceiling, and the
/// active-query counter to be the production ones.
pub(super) fn admission_owner_for_test(
    config: OracleAdmissionConfig,
    resources: crate::resources::OracleResources,
) -> Arc<OracleAdmission> {
    let role = RegisteredRole {
        key: wyrd_spec::vala::api::ClusterNodeKey {
            node_id: NodeId::new(uuid::Uuid::now_v7()),
            role: wyrd_spec::vala::api::ClusterRole::Oracle,
        },
        fencing_token: 1,
        capabilities: wyrd_spec::vala::api::ClusterCapabilities::OracleV1(
            wyrd_spec::vala::api::OracleCapabilitiesV1 {
                storage_protocol_version: 1,
                cpu_cores: 1.0,
                memory_budget_bytes: 1024,
                cpu_cores_per_slot: 1.0,
                memory_bytes_per_slot: 1024,
                raw_slots: 1,
                usable_slots: 1,
                supported_classes: vec![QueryClass::Interactive, QueryClass::Analytical],
                max_workers_per_query: 1,
            },
        ),
    };
    Arc::new(
        OracleAdmission::with_config(role, true, config, resources)
            .expect("admission tests run inside a Tokio runtime"),
    )
}

#[cfg(test)]
/// Reads active query ownership for a stream lifecycle assertion.
pub(super) fn active_queries_for_test(shared: &Arc<AdmissionShared>) -> u64 {
    shared.state.lock().expect("state").active_queries
}

/// Startup result receiver shared by the Oracle lifecycle owner.
type StartupResultReceiver = tokio::sync::oneshot::Receiver<Result<(), BifrostError>>;

#[cfg(test)]
pub(in crate::oracle) mod tests {
    use super::*;
    use crate::cluster::ClusterSnapshot;
    use chrono::Utc;
    use wyrd_spec::vala::api::ClusterCapabilities;
    use wyrd_spec::vala::api::{
        ClusterNodeKey, ClusterRole, ClusterRoleLease, OracleCapabilitiesV1,
    };

    /// Composes the deterministic Oracle capability shared by admission tests.
    pub(in crate::oracle) fn test_resources() -> crate::resources::OracleResources {
        crate::resources::BifrostRuntimeResources::composed_for_test(
            1024 * 1024 * 1024,
            1024 * 1024 * 1024,
            [crate::resources::BifrostRole::Oracle],
        )
        .oracle()
        .expect("composition must enable the Oracle capability")
    }

    fn shared(config: OracleAdmissionConfig) -> Arc<AdmissionShared> {
        let resources = test_resources();
        resources.install_class_split(crate::resources::OracleClassSplit {
            interactive_floor_units: config.interactive_slots,
            analytical_max_units: config.analytical_slots,
        });
        Arc::new(AdmissionShared {
            state: Mutex::new(AdmissionState::new(config, true)),
            notify: tokio::sync::Notify::new(),
            root_cancel: CancellationToken::new(),
            max_queue_wait: config.max_queue_wait,
            next_waiter: AtomicU64::new(1),
            resources,
        })
    }

    /// Returns the governor slot units each class holds: `(interactive, analytical)`.
    ///
    /// Slot capacity is the governor's alone, so tests read it from the
    /// governor rather than from any admission counter. Every query holds one
    /// unit, so the live class counters are the charge.
    fn charged_units(shared: &AdmissionShared) -> (u32, u32) {
        let snapshot = shared.resources.snapshot().expect("resource snapshot");
        (
            snapshot.oracle_interactive_queries,
            snapshot.oracle_analytical_queries,
        )
    }

    /// Builds an admission owner over a specific Oracle resource capability.
    fn owner_with_resources(
        config: OracleAdmissionConfig,
        resources: crate::resources::OracleResources,
    ) -> Arc<OracleAdmission> {
        super::admission_owner_for_test(config, resources)
    }

    fn owner(config: OracleAdmissionConfig) -> Arc<OracleAdmission> {
        let role = RegisteredRole {
            key: ClusterNodeKey {
                node_id: NodeId::new(uuid::Uuid::now_v7()),
                role: ClusterRole::Oracle,
            },
            fencing_token: 1,
            capabilities: ClusterCapabilities::OracleV1(OracleCapabilitiesV1 {
                storage_protocol_version: 1,
                cpu_cores: 1.0,
                memory_budget_bytes: 1024,
                cpu_cores_per_slot: 1.0,
                memory_bytes_per_slot: 1024,
                raw_slots: 1,
                usable_slots: 1,
                supported_classes: vec![QueryClass::Interactive, QueryClass::Analytical],
                max_workers_per_query: 1,
            }),
        };
        Arc::new(
            OracleAdmission::with_config(role, true, config, test_resources())
                .expect("the test admission owner starts"),
        )
    }

    /// Creates an immutable live Oracle snapshot for owner refresh tests.
    fn live_snapshot() -> ClusterSnapshot {
        let node_id = NodeId::new(uuid::Uuid::now_v7());
        ClusterSnapshot::new(vec![ClusterRoleLease {
            key: ClusterNodeKey {
                node_id,
                role: ClusterRole::Oracle,
            },
            address: "http://127.0.0.1:1".to_owned(),
            fencing_token: 1,
            capability_version: 1,
            capabilities: ClusterCapabilities::OracleV1(OracleCapabilitiesV1 {
                storage_protocol_version: 1,
                cpu_cores: 1.0,
                memory_budget_bytes: 1024,
                cpu_cores_per_slot: 1.0,
                memory_bytes_per_slot: 1024,
                raw_slots: 1,
                usable_slots: 1,
                supported_classes: vec![QueryClass::Interactive, QueryClass::Analytical],
                max_workers_per_query: 1,
            }),
            ready: true,
            started_at: Utc::now(),
            heartbeat_at: Utc::now(),
        }])
    }

    /// Oracle materializes physical identity inside its admitted query pool and
    /// releases the exact child before the aggregate query owner.
    #[tokio::test]
    async fn oracle_physical_projection_uses_admitted_query_owner() {
        let owner = owner(OracleAdmissionConfig::default());
        let tenant = DataTenantId::new_v7();
        let mut admitted = owner
            .admit(PreparedAdmission {
                tenant,
                query_class: QueryClass::Interactive,
                local_ratio: 0.0,
                deadline: Instant::now() + Duration::from_secs(1),
                cancellation: CancellationToken::new(),
            })
            .await
            .expect("query admission");
        let table =
            crate::catalog::TableRef::new(crate::namespaces::BifrostNamespace::Traces, "spans");
        let binding = crate::catalog::TenantTableBinding::resolve((tenant, table.clone()))
            .expect("tenant table binding");
        let expected_bytes =
            crate::catalog::LogicalTableIdentity::try_new(&tenant, &tenant, &table)
                .expect("logical identity")
                .projection_facts()
                .expect("checked projection facts")
                .material_bytes;
        let pool = Arc::clone(
            admitted
                .execution()
                .expect("admitted query execution")
                .memory_pool(),
        );
        let baseline = pool.reserved();

        admitted
            .retain_physical_bindings(std::iter::once(&binding))
            .expect("projection child split from admitted query");

        assert_eq!(admitted.physical_projections.len(), 1);
        assert_eq!(pool.reserved(), baseline + expected_bytes);
        drop(admitted);
        assert_eq!(pool.reserved(), 0);
    }

    /// Explicit release drops physical projection children before the query root.
    #[tokio::test]
    async fn oracle_explicit_release_clears_projection_without_poisoning_root() {
        let owner = owner(OracleAdmissionConfig::default());
        let tenant = DataTenantId::new_v7();
        let mut admitted = owner
            .admit(PreparedAdmission {
                tenant,
                query_class: QueryClass::Interactive,
                local_ratio: 0.0,
                deadline: Instant::now() + Duration::from_secs(1),
                cancellation: CancellationToken::new(),
            })
            .await
            .expect("query admission");
        let table =
            crate::catalog::TableRef::new(crate::namespaces::BifrostNamespace::Traces, "spans");
        let binding = crate::catalog::TenantTableBinding::resolve((tenant, table))
            .expect("tenant table binding");
        let pool = Arc::clone(
            admitted
                .execution()
                .expect("admitted query execution")
                .memory_pool(),
        );

        admitted
            .retain_physical_bindings(std::iter::once(&binding))
            .expect("projection child split from admitted query");
        assert!(pool.reserved() > 0);
        admitted.release();

        assert_eq!(pool.reserved(), 0);
        assert_eq!(
            owner
                .shared
                .resources
                .snapshot()
                .expect("explicit release leaves root accounting healthy")
                .oracle_memory_used_bytes,
            0
        );
    }

    /// Local admission capacity never exceeds the independent class ceiling.
    #[test]
    fn local_admission_enforces_class_capacity() {
        let shared = shared(OracleAdmissionConfig {
            interactive_slots: 1,
            ..Default::default()
        });
        let tenant = DataTenantId::new_v7();
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        let mut state = shared.state.lock().expect("state");
        let index = state.interactive.tenant_index(tenant);
        state.interactive.tenants[index]
            .1
            .waiters
            .push_back(Waiter {
                id: 1,
                deadline: Instant::now() + Duration::from_secs(1),
                local_ratio: 0.0,
                tx,
            });
        state.queued = 1;
        let notifications = grant_waiters(&shared, &mut state);
        assert_eq!(charged_units(&shared).0, 1);
        drop(state);
        notify_grants(notifications);
        assert!(rx.try_recv().is_ok());
    }

    /// The fixed per-tenant slot cap bounds one tenant without revoking active permits.
    #[test]
    fn local_admission_enforces_tenant_budget() {
        let shared = shared(OracleAdmissionConfig {
            interactive_slots: 4,
            tenant_interactive_slots: 1,
            ..Default::default()
        });
        let first = DataTenantId::new_v7();
        let second = DataTenantId::new_v7();
        let mut state = shared.state.lock().expect("state");
        let mut receivers = Vec::new();
        for (id, number) in [(first, 1_u64), (second, 2_u64)] {
            let (tx, rx) = tokio::sync::oneshot::channel();
            receivers.push(rx);
            let index = state.interactive.tenant_index(id);
            state.interactive.tenants[index]
                .1
                .waiters
                .push_back(Waiter {
                    id: number,
                    deadline: Instant::now() + Duration::from_secs(1),
                    local_ratio: 0.0,
                    tx,
                });
            state.queued += 1;
        }
        let notifications = grant_waiters(&shared, &mut state);
        assert_eq!(charged_units(&shared).0, 2);
        assert!(
            state
                .interactive
                .tenants
                .iter()
                .all(|(_, queue)| queue.active <= 1)
        );
        drop(state);
        notify_grants(notifications);
        assert_eq!(
            receivers
                .into_iter()
                .map(|mut rx| usize::from(rx.try_recv().is_ok()))
                .sum::<usize>(),
            2
        );
    }

    /// Refresh changes only future grant generation and leaves active counts intact.
    #[tokio::test]
    async fn membership_refresh_affects_only_new_admission() {
        let owner = owner(OracleAdmissionConfig::default());
        let request = || PreparedAdmission {
            tenant: DataTenantId::new_v7(),
            query_class: QueryClass::Interactive,
            local_ratio: 0.0,
            deadline: Instant::now() + Duration::from_secs(1),
            cancellation: CancellationToken::new(),
        };
        let active = owner.admit(request()).await.expect("active admission");
        owner.refresh(&ClusterSnapshot::default());
        assert!(!owner.is_available());
        assert_eq!(owner.shared.state.lock().expect("state").active_queries, 1);
        assert!(matches!(
            owner.admit(request()).await,
            Err(BifrostError::OracleRoleUnavailable)
        ));
        owner.refresh(&live_snapshot());
        assert!(owner.is_available());
        drop(active);
        let future = owner.admit(request()).await.expect("future admission");
        drop(future);
    }

    /// Weighted rounds grant each tenant before borrowing the remaining idle capacity.
    #[test]
    fn local_admission_weighted_round_robin_and_borrowing() {
        let shared = shared(OracleAdmissionConfig {
            interactive_slots: 3,
            tenant_interactive_slots: 3,
            ..Default::default()
        });
        let first = DataTenantId::new_v7();
        let second = DataTenantId::new_v7();
        let mut receivers = Vec::new();
        let mut state = shared.state.lock().expect("state");
        for number in 1_u64..=2 {
            let (tx, rx) = tokio::sync::oneshot::channel();
            receivers.push(rx);
            let index = state.interactive.tenant_index(first);
            state.interactive.tenants[index]
                .1
                .waiters
                .push_back(Waiter {
                    id: number,
                    deadline: Instant::now() + Duration::from_secs(1),
                    local_ratio: 0.0,
                    tx,
                });
            state.queued += 1;
        }
        let (tx, rx) = tokio::sync::oneshot::channel();
        receivers.push(rx);
        let index = state.interactive.tenant_index(second);
        state.interactive.tenants[index]
            .1
            .waiters
            .push_back(Waiter {
                id: 3,
                deadline: Instant::now() + Duration::from_secs(1),
                local_ratio: 0.0,
                tx,
            });
        state.queued += 1;
        let notifications = grant_waiters(&shared, &mut state);
        assert_eq!(charged_units(&shared).0, 3);
        assert_eq!(state.interactive.tenants[0].1.active, 2);
        assert_eq!(state.interactive.tenants[1].1.active, 1);
        drop(state);
        notify_grants(notifications);
        assert_eq!(
            receivers
                .into_iter()
                .map(|mut rx| usize::from(rx.try_recv().is_ok()))
                .sum::<usize>(),
            3
        );
    }

    /// A winner that left before its grant returns every charge by drop, and the
    /// next grant pass seats the waiter behind it.
    #[test]
    fn local_admission_failed_notification_rolls_back_and_regrants() {
        let shared = shared(OracleAdmissionConfig {
            interactive_slots: 1,
            ..Default::default()
        });
        let tenant = DataTenantId::new_v7();
        let (dropped_tx, dropped_rx) = tokio::sync::oneshot::channel();
        drop(dropped_rx);
        let (live_tx, mut live_rx) = tokio::sync::oneshot::channel();
        let mut state = shared.state.lock().expect("state");
        let index = state.interactive.tenant_index(tenant);
        for (id, tx) in [(1, dropped_tx), (2, live_tx)] {
            state.interactive.tenants[index]
                .1
                .waiters
                .push_back(Waiter {
                    id,
                    deadline: Instant::now() + Duration::from_secs(1),
                    local_ratio: 0.0,
                    tx,
                });
            state.queued += 1;
        }
        let notifications = grant_waiters(&shared, &mut state);
        drop(state);
        notify_grants(notifications);
        shared.regrant();
        let grant = live_rx.try_recv().expect("replacement waiter grant");
        assert_eq!(charged_units(&shared).0, 1);
        assert_eq!(shared.state.lock().expect("state").active_queries, 1);
        drop(grant);
    }

    /// The admission queue never grants beyond its configured waiter bound.
    #[tokio::test]
    async fn local_admission_queue_capacity_is_bounded() {
        let owner = owner(OracleAdmissionConfig {
            interactive_slots: 1,
            analytical_slots: 0,
            tenant_analytical_slots: 0,
            queue_capacity: 1,
            ..Default::default()
        });
        let request = || PreparedAdmission {
            tenant: DataTenantId::new_v7(),
            query_class: QueryClass::Interactive,
            local_ratio: 0.0,
            deadline: Instant::now() + Duration::from_secs(2),
            cancellation: CancellationToken::new(),
        };
        let first = owner.admit(request()).await.expect("first admission");
        let queued_owner = Arc::clone(&owner);
        let queued = tokio::spawn(async move { queued_owner.admit(request()).await });
        tokio::task::yield_now().await;
        assert_eq!(owner.shared.state.lock().expect("state").queued, 1);
        assert!(matches!(
            owner.admit(request()).await,
            Err(BifrostError::QueryQueueFull)
        ));
        drop(first);
        owner.close();
        queued.abort();
        let _ = queued.await;
    }

    /// A queued owner honors the original request deadline without extension.
    #[tokio::test]
    async fn production_admission_absolute_deadline_is_bounded() {
        let owner = owner(OracleAdmissionConfig {
            interactive_slots: 1,
            analytical_slots: 0,
            tenant_analytical_slots: 0,
            max_queue_wait: Duration::from_secs(1),
            ..Default::default()
        });
        let first = owner
            .admit(PreparedAdmission {
                tenant: DataTenantId::new_v7(),
                query_class: QueryClass::Interactive,
                local_ratio: 0.0,
                deadline: Instant::now() + Duration::from_secs(1),
                cancellation: CancellationToken::new(),
            })
            .await
            .expect("first admission");
        let started = Instant::now();
        let result = owner
            .admit(PreparedAdmission {
                tenant: DataTenantId::new_v7(),
                query_class: QueryClass::Interactive,
                local_ratio: 0.0,
                deadline: started + Duration::from_millis(20),
                cancellation: CancellationToken::new(),
            })
            .await;
        assert!(matches!(result, Err(BifrostError::QueryTimeout)));
        assert!(started.elapsed() < Duration::from_millis(250));
        drop(first);
    }

    /// Releasing one concurrent owner cancels only that query's child.
    #[tokio::test]
    async fn production_admission_two_query_cancellation_isolated() {
        let owner = owner(OracleAdmissionConfig {
            interactive_slots: 2,
            ..Default::default()
        });
        let caller_one = CancellationToken::new();
        let caller_two = CancellationToken::new();
        let first = owner
            .admit(PreparedAdmission {
                tenant: DataTenantId::new_v7(),
                query_class: QueryClass::Interactive,
                local_ratio: 0.0,
                deadline: Instant::now() + Duration::from_secs(1),
                cancellation: caller_one.clone(),
            })
            .await
            .expect("first admission");
        let first_child = first.cancellation.clone();
        let queued_owner = Arc::clone(&owner);
        let second_task = tokio::spawn(async move {
            queued_owner
                .admit(PreparedAdmission {
                    tenant: DataTenantId::new_v7(),
                    query_class: QueryClass::Interactive,
                    local_ratio: 0.0,
                    deadline: Instant::now() + Duration::from_secs(1),
                    cancellation: caller_two.clone(),
                })
                .await
        });
        tokio::task::yield_now().await;
        assert!(second_task.is_finished());
        first.release();
        assert!(first_child.is_cancelled());
        let second = second_task
            .await
            .expect("concurrent task")
            .expect("second concurrent admission");
        let second_child = second.cancellation.clone();
        assert!(!second_child.is_cancelled());
        assert!(!caller_one.is_cancelled());
        drop(second);
    }

    /// Both scheduling classes contend for one allocator without resource silos.
    #[tokio::test]
    async fn oracle_classes_share_allocator_without_resource_silos() {
        // One total unit expresses the contention this covers: an interactive
        // query holds it and an analytical query needs it, so the classes must
        // queue behind one shared ceiling rather than behind private silos.
        let owner = owner(OracleAdmissionConfig {
            interactive_slots: 0,
            analytical_slots: 1,
            tenant_interactive_slots: 1,
            tenant_analytical_slots: 1,
            ..Default::default()
        });
        let interactive = owner
            .admit(PreparedAdmission {
                tenant: DataTenantId::new_v7(),
                query_class: QueryClass::Interactive,
                local_ratio: 1.0,
                deadline: Instant::now() + Duration::from_secs(1),
                cancellation: CancellationToken::new(),
            })
            .await
            .expect("interactive query owns the global envelope");
        let queued_owner = Arc::clone(&owner);
        let analytical = tokio::spawn(async move {
            queued_owner
                .admit(PreparedAdmission {
                    tenant: DataTenantId::new_v7(),
                    query_class: QueryClass::Analytical,
                    local_ratio: 0.0,
                    deadline: Instant::now() + Duration::from_secs(1),
                    cancellation: CancellationToken::new(),
                })
                .await
        });
        tokio::task::yield_now().await;
        assert!(!analytical.is_finished());
        assert!(
            owner
                .shared
                .resources
                .snapshot()
                .expect("resource snapshot")
                .oracle_query_active
        );
        interactive.release();
        let analytical = analytical
            .await
            .expect("analytical task")
            .expect("analytical query wakes after shared release");
        analytical.release();
        assert!(
            !owner
                .shared
                .resources
                .snapshot()
                .expect("released snapshot")
                .oracle_query_active
        );
    }

    /// Root shutdown cancels the sole active child and rejects queued work.
    #[tokio::test]
    async fn production_admission_shutdown_cancels_children() {
        let owner = owner(OracleAdmissionConfig {
            interactive_slots: 2,
            ..Default::default()
        });
        let first = owner
            .admit(PreparedAdmission {
                tenant: DataTenantId::new_v7(),
                query_class: QueryClass::Interactive,
                local_ratio: 0.0,
                deadline: Instant::now() + Duration::from_secs(1),
                cancellation: CancellationToken::new(),
            })
            .await
            .expect("first admission");
        let queued_owner = Arc::clone(&owner);
        let second = tokio::spawn(async move {
            queued_owner
                .admit(PreparedAdmission {
                    tenant: DataTenantId::new_v7(),
                    query_class: QueryClass::Interactive,
                    local_ratio: 0.0,
                    deadline: Instant::now() + Duration::from_secs(1),
                    cancellation: CancellationToken::new(),
                })
                .await
        });
        let first_child = first.cancellation.clone();
        owner.close();
        assert!(first_child.is_cancelled());
        assert!(matches!(
            second.await.expect("queued task"),
            Err(BifrostError::QueryAdmissionRejected)
        ));
        drop(first);
    }

    /// Covers successful, unavailable, and deadline admission releases.
    async fn production_release_prefix() {
        let success_owner = owner(OracleAdmissionConfig::default());
        let success = success_owner
            .admit(PreparedAdmission {
                tenant: DataTenantId::new_v7(),
                query_class: QueryClass::Interactive,
                local_ratio: 0.0,
                deadline: Instant::now() + Duration::from_secs(1),
                cancellation: CancellationToken::new(),
            })
            .await
            .expect("success admission");
        drop(success);
        assert_eq!(
            success_owner
                .shared
                .state
                .lock()
                .expect("state")
                .active_queries,
            0
        );

        let error_owner = owner(OracleAdmissionConfig::default());
        error_owner.refresh(&ClusterSnapshot::default());
        let error = error_owner
            .admit(PreparedAdmission {
                tenant: DataTenantId::new_v7(),
                query_class: QueryClass::Interactive,
                local_ratio: 0.0,
                deadline: Instant::now() + Duration::from_secs(1),
                cancellation: CancellationToken::new(),
            })
            .await;
        assert!(matches!(error, Err(BifrostError::OracleRoleUnavailable)));
        assert_eq!(
            error_owner
                .shared
                .state
                .lock()
                .expect("state")
                .active_queries,
            0
        );

        let timeout_owner = owner(OracleAdmissionConfig {
            interactive_slots: 1,
            analytical_slots: 0,
            tenant_analytical_slots: 0,
            max_queue_wait: Duration::from_secs(1),
            ..Default::default()
        });
        let held = timeout_owner
            .admit(PreparedAdmission {
                tenant: DataTenantId::new_v7(),
                query_class: QueryClass::Interactive,
                local_ratio: 0.0,
                deadline: Instant::now() + Duration::from_secs(1),
                cancellation: CancellationToken::new(),
            })
            .await
            .expect("held admission");
        let timeout = timeout_owner
            .admit(PreparedAdmission {
                tenant: DataTenantId::new_v7(),
                query_class: QueryClass::Interactive,
                local_ratio: 0.0,
                deadline: Instant::now() + Duration::from_millis(10),
                cancellation: CancellationToken::new(),
            })
            .await;
        assert!(matches!(timeout, Err(BifrostError::QueryTimeout)));
        drop(held);
    }

    /// Production admission releases local ownership across every terminal edge.
    #[tokio::test]
    async fn production_admission_release_matrix() {
        production_release_prefix().await;

        let request_owner = owner(OracleAdmissionConfig {
            interactive_slots: 1,
            analytical_slots: 0,
            tenant_analytical_slots: 0,
            ..Default::default()
        });
        let request_held = request_owner
            .admit(PreparedAdmission {
                tenant: DataTenantId::new_v7(),
                query_class: QueryClass::Interactive,
                local_ratio: 0.0,
                deadline: Instant::now() + Duration::from_secs(1),
                cancellation: CancellationToken::new(),
            })
            .await
            .expect("request-held admission");
        let request_cancel = CancellationToken::new();
        let request_cancel_for_task = request_cancel.clone();
        let request_owner_task = Arc::clone(&request_owner);
        let request_task = tokio::spawn(async move {
            request_owner_task
                .admit(PreparedAdmission {
                    tenant: DataTenantId::new_v7(),
                    query_class: QueryClass::Interactive,
                    local_ratio: 0.0,
                    deadline: Instant::now() + Duration::from_secs(1),
                    cancellation: request_cancel_for_task,
                })
                .await
        });
        tokio::task::yield_now().await;
        request_cancel.cancel();
        assert!(matches!(
            request_task.await.expect("request task"),
            Err(BifrostError::QueryAdmissionRejected)
        ));
        drop(request_held);

        let root_owner = owner(OracleAdmissionConfig::default());
        let root = root_owner
            .admit(PreparedAdmission {
                tenant: DataTenantId::new_v7(),
                query_class: QueryClass::Interactive,
                local_ratio: 0.0,
                deadline: Instant::now() + Duration::from_secs(1),
                cancellation: CancellationToken::new(),
            })
            .await
            .expect("root admission");
        let root_child = root.cancellation.clone();
        root_owner.close();
        assert!(root_child.is_cancelled());
        drop(root);
    }

    /// A granted query's tenant charge returns exactly once, with its slot.
    ///
    /// The charge rides on the governor's resource owner, so dropping that
    /// owner is the only release and returns both the slot and the tenant
    /// counts together.
    #[test]
    fn tenant_charge_returns_with_its_resources() {
        let shared = shared(OracleAdmissionConfig::default());
        let tenant = DataTenantId::new_v7();
        let resources = acquire_waiter_resources(&shared, QueryClass::Interactive, 0.0)
            .expect("the governor seats one Interactive query");
        let resources = {
            let mut state = shared.state.lock().expect("state");
            let index = state.interactive.tenant_index(tenant);
            charge_tenant(
                &shared,
                &mut state,
                QueryClass::Interactive,
                index,
                resources,
            )
        };
        {
            let state = shared.state.lock().expect("state");
            assert_eq!(state.interactive.tenants[0].1.active, 1);
            assert_eq!(state.active_queries, 1);
        }
        assert_eq!(charged_units(&shared), (1, 0));
        drop(resources);
        let state = shared.state.lock().expect("state");
        assert_eq!(state.interactive.tenants[0].1.active, 0);
        assert_eq!(state.active_queries, 0);
        drop(state);
        assert_eq!(charged_units(&shared), (0, 0));
    }

    /// A permit release is idempotent and immediately converges queued work.
    #[tokio::test]
    async fn local_admission_releases_once() {
        let owner = owner(OracleAdmissionConfig {
            analytical_slots: 1,
            ..Default::default()
        });
        let guard = owner
            .admit(PreparedAdmission {
                tenant: DataTenantId::new_v7(),
                query_class: QueryClass::Analytical,
                local_ratio: 0.0,
                deadline: Instant::now() + Duration::from_secs(1),
                cancellation: CancellationToken::new(),
            })
            .await
            .expect("analytical admission");
        guard.release();
        assert_eq!(charged_units(&owner.shared).1, 0);
        assert_eq!(owner.shared.state.lock().expect("state").active_queries, 0);
    }

    /// The queue stores the precomputed absolute deadline rather than extending it on wakeups.
    #[test]
    fn local_admission_queue_deadline_is_absolute() {
        let enqueue = Instant::now();
        let request_deadline = enqueue + Duration::from_secs(5);
        let cap = enqueue + Duration::from_millis(250);
        assert_eq!(request_deadline.min(cap), cap);
    }

    /// Generation increments are the only refresh effect on running local state.
    #[tokio::test]
    async fn membership_refresh_creates_future_generation() {
        let owner = owner(OracleAdmissionConfig::default());
        let before = owner.shared.state.lock().expect("state").generation;
        owner.refresh(&ClusterSnapshot::default());
        let state = owner.shared.state.lock().expect("state");
        assert_eq!(state.generation, before + 1);
        assert!(!state.membership_available);
    }

    /// Closing twice leaves queued work canceled and reports active residuals truthfully.
    #[tokio::test]
    async fn oracle_shutdown_is_bounded_and_idempotent() {
        let owner = owner(OracleAdmissionConfig::default());
        let request = PreparedAdmission {
            tenant: DataTenantId::new_v7(),
            query_class: QueryClass::Interactive,
            local_ratio: 0.0,
            deadline: Instant::now() + Duration::from_secs(1),
            cancellation: CancellationToken::new(),
        };
        let active = owner.admit(request).await.expect("active admission");
        let forced = owner.shutdown(Instant::now()).await;
        assert_eq!(forced.active_queries, 1);
        assert_eq!(forced.queued_queries, 0);
        assert!(owner.shared.state.lock().expect("state").closed);
        let repeated = owner.shutdown(Instant::now()).await;
        assert_eq!(repeated.active_queries, 1);
        drop(active);
        let clean = owner
            .shutdown(Instant::now() + Duration::from_secs(1))
            .await;
        assert_eq!(clean.active_queries, 0);
        assert_eq!(clean.queued_queries, 0);
    }

    /// One grant pass is fair and work-conserving.
    ///
    /// Pins the whole local scheduling rule in a single pass: the older class
    /// head is offered first, the governor alone decides whether it runs and
    /// still leaves the Interactive floor to Interactive work, eligible tenants
    /// rotate one grant at a time with equal weight, and a tenant's own
    /// requests stay in arrival order. Releasing one grant must then hand that
    /// capacity to the queued waiter through the single capacity wake rather
    /// than leave it idle.
    ///
    /// # Panics
    ///
    /// Panics when a grant is misordered, withheld while capacity is free, or
    /// issued beyond the governor's capacity.
    #[tokio::test]
    async fn local_admission_is_fair_and_work_conserving() {
        // Four slot units of memory must actually exist, or a memory refusal
        // would masquerade as a scheduling decision.
        let owner = owner_with_resources(
            OracleAdmissionConfig {
                interactive_slots: 2,
                analytical_slots: 2,
                tenant_interactive_slots: 4,
                tenant_analytical_slots: 2,
                ..Default::default()
            },
            crate::resources::BifrostRuntimeResources::composed_for_test(
                8 * 1024 * 1024 * 1024,
                8 * 1024 * 1024 * 1024,
                [crate::resources::BifrostRole::Oracle],
            )
            .oracle()
            .expect("composition must enable the Oracle capability"),
        );
        let shared = Arc::clone(&owner.shared);
        let deadline = Instant::now() + Duration::from_secs(30);
        let analytical_tenant = DataTenantId::new_v7();
        let tenant_b = DataTenantId::new_v7();
        let tenant_c = DataTenantId::new_v7();

        // Arrival order is the waiter identity: the two analytical requests are
        // the oldest work in the queue, so they are offered first and fill the
        // Analytical maximum, leaving the Interactive floor to the rest.
        let mut analytical_head = push_waiter(
            &shared,
            QueryClass::Analytical,
            analytical_tenant,
            1,
            deadline,
        );
        let mut analytical_tail = push_waiter(
            &shared,
            QueryClass::Analytical,
            analytical_tenant,
            2,
            deadline,
        );
        let mut first_tenant_head =
            push_waiter(&shared, QueryClass::Interactive, tenant_b, 3, deadline);
        let mut peer_tenant_head =
            push_waiter(&shared, QueryClass::Interactive, tenant_c, 4, deadline);
        let mut first_tenant_tail =
            push_waiter(&shared, QueryClass::Interactive, tenant_b, 5, deadline);
        drain_grants(&shared);

        let first_head_grant = first_tenant_head
            .try_recv()
            .expect("the protected interactive floor seats interactive work");
        let peer_head_grant = peer_tenant_head
            .try_recv()
            .expect("the tenant cursor rotates to a peer tenant before it repeats one");
        assert!(
            first_tenant_tail.try_recv().is_err(),
            "a tenant's second request must wait behind its peers, not jump the rotation"
        );
        let analytical_head_grant = analytical_head
            .try_recv()
            .expect("the older analytical head outranks a younger interactive waiter");
        let analytical_tail_grant = analytical_tail
            .try_recv()
            .expect("remaining capacity keeps going to the oldest eligible class head");
        assert_eq!(
            charged_units(&shared),
            (2, 2),
            "the floor is exactly two units and each analytical grant charges one"
        );
        assert_eq!(
            shared.state.lock().expect("state").queued,
            1,
            "only the rotation-deferred request waits"
        );

        // Work conservation: the release's capacity wake alone must hand the
        // freed units to the queued waiter, without a second arrival.
        drop(analytical_tail_grant);
        let first_tail_grant = await_grant(&mut first_tenant_tail).await;
        assert_eq!(shared.state.lock().expect("state").queued, 0);

        drop(first_head_grant);
        drop(peer_head_grant);
        drop(first_tail_grant);
        drop(analytical_head_grant);
        assert_eq!(charged_units(&shared), (0, 0));
        assert_eq!(shared.state.lock().expect("state").active_queries, 0);
    }

    /// A graph follower envelope release wakes the queued leader in arrival order.
    ///
    /// Leader admission and a peer's graph envelope charge one shared governor
    /// ledger, so a follower that finishes is the only event that can free the
    /// leader's capacity. The queued leader must observe that release through
    /// its existing bounded wait and the queue must keep its arrival order.
    ///
    /// # Panics
    ///
    /// Panics when the follower charge is not exclusive, when a release fails to
    /// wake the queue, or when the younger waiter is granted first.
    #[tokio::test]
    async fn follower_release_wakes_waiting_leader_without_reordering() {
        let owner = owner(OracleAdmissionConfig {
            interactive_slots: 1,
            analytical_slots: 0,
            tenant_analytical_slots: 0,
            max_queue_wait: Duration::from_secs(30),
            ..Default::default()
        });
        let worker = owner
            .shared
            .resources
            .try_acquire_query(crate::resources::OracleResourceRequest::for_class(
                QueryClass::Interactive,
                0.0,
            ))
            .expect("a follower graph envelope charges the one local slot unit");
        let request = || PreparedAdmission {
            tenant: DataTenantId::new_v7(),
            query_class: QueryClass::Interactive,
            local_ratio: 0.0,
            deadline: Instant::now() + Duration::from_secs(30),
            cancellation: CancellationToken::new(),
        };
        let first_owner = Arc::clone(&owner);
        let first = tokio::spawn(async move { first_owner.admit(request()).await });
        wait_for_queued(&owner, 1).await;
        let second_owner = Arc::clone(&owner);
        let second = tokio::spawn(async move { second_owner.admit(request()).await });
        wait_for_queued(&owner, 2).await;

        drop(worker);
        let granted = first
            .await
            .expect("first admission task")
            .expect("the follower release wakes the oldest waiter");
        assert!(
            !second.is_finished(),
            "one freed unit must not grant two queries"
        );
        granted.release();
        let followed = second
            .await
            .expect("second admission task")
            .expect("the younger waiter is granted only after the older one releases");
        followed.release();
        let state = owner.shared.state.lock().expect("state");
        assert_eq!(state.queued, 0);
        assert_eq!(state.active_queries, 0);
    }

    /// Yields until the admission queue holds exactly `expected` waiters.
    ///
    /// Spawned admission tasks reach their queued state on a later poll, so a
    /// bounded yield loop replaces a sleep and still fails loudly if the queue
    /// never converges.
    ///
    /// # Panics
    ///
    /// Panics when the queue does not reach `expected` within the bound.
    async fn wait_for_queued(owner: &Arc<OracleAdmission>, expected: u32) {
        for _ in 0..1_000 {
            if owner.shared.state.lock().expect("state").queued == expected {
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("admission queue never reached {expected} waiters");
    }

    /// Queues one waiter directly so several requests contend in a single pass.
    ///
    /// Production enqueues run a grant pass on every arrival. Tenant rotation and
    /// the multi-tenant ceiling are only observable once the queue already holds
    /// the competing requests, so this seeds the queue and the caller runs one
    /// pass by hand.
    fn push_waiter(
        shared: &Arc<AdmissionShared>,
        query_class: QueryClass,
        tenant: DataTenantId,
        id: u64,
        deadline: Instant,
    ) -> tokio::sync::oneshot::Receiver<crate::resources::OracleQueryResources> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let mut state = shared.state.lock().expect("state");
        let class = match query_class {
            QueryClass::Interactive => &mut state.interactive,
            QueryClass::Analytical => &mut state.analytical,
        };
        let index = class.tenant_index(tenant);
        class.tenants[index].1.waiters.push_back(Waiter {
            id,
            deadline,
            local_ratio: 0.0,
            tx,
        });
        state.queued += 1;
        rx
    }

    /// Runs one complete grant pass exactly as a release or arrival would.
    fn drain_grants(shared: &Arc<AdmissionShared>) {
        let notifications = {
            let mut state = shared.state.lock().expect("state");
            grant_waiters(shared, &mut state)
        };
        notify_grants(notifications);
    }

    /// Awaits a queued waiter's grant delivered by the owner's capacity wake.
    ///
    /// # Panics
    ///
    /// Panics when no grant arrives within five seconds.
    async fn await_grant(
        receiver: &mut tokio::sync::oneshot::Receiver<crate::resources::OracleQueryResources>,
    ) -> crate::resources::OracleQueryResources {
        tokio::time::timeout(Duration::from_secs(5), receiver)
            .await
            .expect("the capacity wake grants the queued waiter")
            .expect("the grant channel stays open")
    }

    /// A waiter that expired behind a live FIFO head is never granted, even when
    /// the same pass has capacity left after granting that head.
    ///
    /// Each request's absolute deadline is `min(caller deadline, enqueue +
    /// max_queue_wait)`, so deadlines are not monotonic inside one tenant FIFO.
    /// This seeds an older live head in front of a later already-expired entry,
    /// leaves two Interactive units free so one pass can issue two grants, and
    /// proves the exposed expired entry is dropped rather than given slot
    /// ownership. Live work queued afterwards still progresses.
    #[tokio::test]
    async fn expired_waiter_behind_a_live_head_is_never_granted() {
        let resources = test_resources();
        let config = OracleAdmissionConfig {
            interactive_slots: 2,
            tenant_interactive_slots: 2,
            ..Default::default()
        };
        let owner = owner_with_resources(config, resources.clone());
        let shared = Arc::clone(&owner.shared);
        let baseline = resources.snapshot().expect("root baseline");
        let tenant = DataTenantId::new_v7();

        let mut live_head = push_waiter(
            &shared,
            QueryClass::Interactive,
            tenant,
            1,
            Instant::now() + Duration::from_secs(30),
        );
        // An instant read before the grant pass reads its own is already in the
        // past by the time the pass compares them, so this waiter is
        // deterministically expired without any clock arithmetic.
        let mut expired_tail =
            push_waiter(&shared, QueryClass::Interactive, tenant, 2, Instant::now());
        drain_grants(&shared);

        let head_grant = live_head.try_recv().expect("the live head must be granted");
        assert!(
            expired_tail.try_recv().is_err(),
            "an expired waiter must never receive a grant"
        );
        {
            let state = shared.state.lock().expect("state");
            assert_eq!(state.queued, 0, "both waiters left the queue");
            assert_eq!(state.active_queries, 1, "only the live head is active");
            assert_eq!(charged_units(&shared).0, 1, "only one slot unit is charged");
            assert_eq!(
                state.interactive.tenants[0].1.active, 1,
                "the tenant owns exactly the live head's units"
            );
            assert!(
                state.interactive.tenants[0].1.waiters.is_empty(),
                "the expired entry is removed from the FIFO"
            );
        }
        let after = resources
            .snapshot()
            .expect("root while the head owns its grant");
        assert_eq!(
            after.oracle_active_queries - baseline.oracle_active_queries,
            1,
            "exactly one Interactive slot grant is outstanding"
        );
        drop(head_grant);
        assert_eq!(
            resources
                .snapshot()
                .expect("root after the head releases")
                .oracle_active_queries,
            baseline.oracle_active_queries,
            "no slot is stranded by the rejected expired waiter"
        );

        let mut next_live = push_waiter(
            &shared,
            QueryClass::Interactive,
            tenant,
            3,
            Instant::now() + Duration::from_secs(30),
        );
        drain_grants(&shared);
        assert!(
            next_live.try_recv().is_ok(),
            "live work queued after the expired entry still progresses"
        );
    }

    /// A waiter that is still live when a pass begins but expires before its own
    /// grant decision is rejected rather than granted.
    ///
    /// This is the case a single pass-start instant cannot see: the head is live
    /// at both reads, while the tail is live at the first decision and dead at
    /// the second. The clock is scripted rather than measured, so the two
    /// decision times are fixed and the test never depends on how long the
    /// intervening grant actually takes. It proves the expired tail acquires no
    /// slot, active-query, or tenant ownership and receives no
    /// notification, and that live work queued afterwards still progresses.
    #[tokio::test]
    async fn waiter_expiring_between_grant_decisions_is_never_granted() {
        let resources = test_resources();
        let config = OracleAdmissionConfig {
            interactive_slots: 2,
            tenant_interactive_slots: 2,
            ..Default::default()
        };
        let owner = owner_with_resources(config, resources.clone());
        let shared = Arc::clone(&owner.shared);
        let baseline = resources.snapshot().expect("root baseline");
        let tenant = DataTenantId::new_v7();
        let start = Instant::now();

        let mut live_head = push_waiter(
            &shared,
            QueryClass::Interactive,
            tenant,
            1,
            start + Duration::from_secs(30),
        );
        let mut expiring_tail = push_waiter(
            &shared,
            QueryClass::Interactive,
            tenant,
            2,
            start + Duration::from_secs(1),
        );

        // Decision one sees `start`, where both waiters are live; decision two
        // sees `start + 2s`, by which the tail's absolute deadline has passed.
        let mut decisions = 0_u32;
        let notifications = {
            let mut state = shared.state.lock().expect("state");
            grant_waiters_with_clock(&shared, &mut state, || {
                decisions += 1;
                if decisions == 1 {
                    start
                } else {
                    start + Duration::from_secs(2)
                }
            })
        };
        notify_grants(notifications);

        let head_grant = live_head.try_recv().expect("the live head must be granted");
        assert!(
            expiring_tail.try_recv().is_err(),
            "a waiter that expired mid-pass must never receive a grant"
        );
        {
            let state = shared.state.lock().expect("state");
            assert_eq!(state.queued, 0, "both waiters left the queue");
            assert_eq!(state.active_queries, 1, "only the live head is active");
            assert_eq!(charged_units(&shared).0, 1, "only one slot unit is charged");
            assert_eq!(
                state.interactive.tenants[0].1.active, 1,
                "the tenant owns exactly the live head's units"
            );
            assert!(
                state.interactive.tenants[0].1.waiters.is_empty(),
                "the expired entry is removed from the FIFO"
            );
        }
        let after = resources
            .snapshot()
            .expect("root while the head owns its grant");
        assert_eq!(
            after.oracle_active_queries - baseline.oracle_active_queries,
            1,
            "exactly one Interactive slot grant is outstanding"
        );
        drop(head_grant);
        assert_eq!(
            resources
                .snapshot()
                .expect("root after the head releases")
                .oracle_active_queries,
            baseline.oracle_active_queries,
            "no slot is stranded by the rejected waiter"
        );

        let mut next_live = push_waiter(
            &shared,
            QueryClass::Interactive,
            tenant,
            3,
            Instant::now() + Duration::from_secs(30),
        );
        drain_grants(&shared);
        assert!(
            next_live.try_recv().is_ok(),
            "live work queued after the rejected waiter still progresses"
        );
        assert_eq!(
            decisions, 2,
            "the pass read the clock once per grant decision, not once per pass"
        );
    }

    /// A saturated Analytical class never consumes Interactive capacity, per-tenant
    /// FIFO holds, and the tenant cursor grants a waiting peer tenant before it
    /// returns to the first tenant.
    ///
    /// Rejection is checked against the same process root so a refused request is
    /// proven to leave class, tenant, memory, and slot ownership exactly
    /// where it found it.
    #[tokio::test]
    async fn analytical_saturation_preserves_interactive_floor_and_rotates_tenants() {
        let resources = test_resources();
        let config = OracleAdmissionConfig {
            interactive_slots: 2,
            analytical_slots: 1,
            tenant_interactive_slots: 3,
            tenant_analytical_slots: 1,
            ..Default::default()
        };
        let owner = owner_with_resources(config, resources.clone());
        let shared = Arc::clone(&owner.shared);
        let baseline = resources.snapshot().expect("root baseline");
        let tenant_a = DataTenantId::new_v7();
        let tenant_b = DataTenantId::new_v7();
        let deadline = Instant::now() + Duration::from_secs(30);

        let mut analytical = push_waiter(&shared, QueryClass::Analytical, tenant_a, 1, deadline);
        let mut tenant_a_head =
            push_waiter(&shared, QueryClass::Interactive, tenant_a, 2, deadline);
        let mut tenant_a_tail =
            push_waiter(&shared, QueryClass::Interactive, tenant_a, 3, deadline);
        let mut peer_head = push_waiter(&shared, QueryClass::Interactive, tenant_b, 4, deadline);
        drain_grants(&shared);

        let analytical_grant = analytical.try_recv().expect("analytical class grant");
        let head_grant = tenant_a_head
            .try_recv()
            .expect("interactive tenant A is grantable while the analytical class is full");
        let peer_grant = peer_head
            .try_recv()
            .expect("the tenant cursor reaches peer tenant B before tenant A repeats");
        assert!(
            tenant_a_tail.try_recv().is_err(),
            "the shared slot ceiling holds tenant A's second interactive request"
        );
        assert_eq!(charged_units(&shared), (2, 1));
        {
            let state = shared.state.lock().expect("state");
            assert_eq!(state.interactive.tenants[0].1.active, 1);
            assert_eq!(state.interactive.tenants[1].1.active, 1);
            assert_eq!(state.queued, 1);
        }

        drop(peer_grant);
        let tail_grant = await_grant(&mut tenant_a_tail).await;
        assert_eq!(shared.state.lock().expect("state").queued, 0);

        drop(head_grant);
        drop(tail_grant);
        drop(analytical_grant);
        assert_eq!(charged_units(&shared), (0, 0));
        {
            let state = shared.state.lock().expect("state");
            assert_eq!(state.active_queries, 0);
            assert!(
                state
                    .interactive
                    .tenants
                    .iter()
                    .chain(state.analytical.tenants.iter())
                    .all(|(_, queue)| queue.active == 0 && queue.waiters.is_empty())
            );
        }
        assert_eq!(
            resources.snapshot().expect("released root"),
            baseline,
            "every granted envelope returns to the process root"
        );

        prove_rejection_charges_nothing(config, &resources, tenant_a, deadline).await;
        assert_eq!(
            resources.snapshot().expect("final root"),
            baseline,
            "rejection and release leave the process root at its exact baseline"
        );
    }

    /// Proves a queue-full refusal mutates nothing the process root owns.
    ///
    /// Deliberately run against the same root the granting owner used: a
    /// refusal that quietly charged the root would be invisible to an owner
    /// checked only against its own class counters.
    async fn prove_rejection_charges_nothing(
        config: OracleAdmissionConfig,
        resources: &crate::resources::OracleResources,
        tenant: DataTenantId,
        deadline: Instant,
    ) {
        let rejecting = owner_with_resources(
            OracleAdmissionConfig {
                interactive_slots: 1,
                analytical_slots: 0,
                tenant_analytical_slots: 0,
                queue_capacity: 1,
                ..config
            },
            resources.clone(),
        );
        let mut held = rejecting
            .enqueue_waiter(tenant, QueryClass::Interactive, 0.0, deadline)
            .expect("first interactive request is admitted");
        let mut waiting = rejecting
            .enqueue_waiter(tenant, QueryClass::Interactive, 0.0, deadline)
            .expect("second interactive request occupies the finite queue");
        let before_rejection = resources.snapshot().expect("pre-rejection root");
        let rejected = rejecting.enqueue_waiter(tenant, QueryClass::Interactive, 0.0, deadline);
        assert!(matches!(rejected, Err(BifrostError::QueryQueueFull)));
        assert_eq!(charged_units(&rejecting.shared).0, 1);
        {
            let state = rejecting.shared.state.lock().expect("state");
            assert_eq!(state.queued, 1);
            assert_eq!(state.active_queries, 1);
        }
        assert_eq!(
            resources.snapshot().expect("post-rejection root"),
            before_rejection,
            "a refused request charges no class, tenant, memory, or slot ownership"
        );

        let held_grant = held.receiver.try_recv().expect("held interactive grant");
        drop(held_grant);
        drop(await_grant(&mut waiting.receiver).await);
    }

    /// Places in the production queue, shared by both query classes.
    const PRODUCTION_QUEUE_PLACES: usize = 1_000;

    /// Builds one waiting request for `class` whose total deadline is `total`.
    fn waiting_request(
        class: QueryClass,
        total: Duration,
        cancellation: CancellationToken,
    ) -> PreparedAdmission {
        PreparedAdmission {
            tenant: DataTenantId::new_v7(),
            query_class: class,
            local_ratio: 1.0,
            deadline: Instant::now() + total,
            cancellation,
        }
    }

    /// Admits one-unit Interactive queries until every slot unit is held, so
    /// every later query of either class has to wait in the queue.
    async fn saturate(owner: &Arc<OracleAdmission>) -> Vec<AdmittedQueryGuard> {
        let mut held = Vec::new();
        for _ in 0..2 {
            held.push(
                owner
                    .admit(waiting_request(
                        QueryClass::Interactive,
                        Duration::from_mins(1),
                        CancellationToken::new(),
                    ))
                    .await
                    .expect("a holder takes one free slot unit"),
            );
        }
        held
    }

    /// Admission config with two slot units that Interactive may borrow.
    fn saturable(queue_capacity: u32, max_queue_wait: Duration) -> OracleAdmissionConfig {
        OracleAdmissionConfig {
            interactive_slots: 0,
            analytical_slots: 2,
            tenant_interactive_slots: 2,
            tenant_analytical_slots: 2,
            queue_capacity,
            max_queue_wait,
        }
    }

    /// Waits for the queue to report `expected` waiters without sleeping.
    ///
    /// Bounded by wall time rather than a yield count: a thousand spawned
    /// waiters on two workers need an unpredictable number of yields to all
    /// enqueue when the host is busy, so a fixed iteration budget fails a
    /// correct queue.
    ///
    /// # Panics
    /// Panics when the queue does not reach `expected` within ten seconds.
    async fn await_queued(owner: &Arc<OracleAdmission>, expected: u32) {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            if owner.shared.state.lock().expect("state").queued == expected {
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("the queue never reached {expected} waiters");
    }

    /// Both query classes share one queue-wait limit and one total deadline;
    /// the earlier limit ends a wait as a query timeout that frees its place.
    ///
    /// # Panics
    ///
    /// Panics when a default, bound, or timeout claim fails.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn queued_queries_obey_queue_and_total_deadlines() {
        let defaults = OracleAdmissionConfig::default();
        assert_eq!(defaults.queue_capacity, 1_000);
        assert_eq!(defaults.max_queue_wait, Duration::from_hours(1));
        let engine = OracleConfig::default();
        assert_eq!(engine.queue_capacity, 1_000);
        assert_eq!(engine.max_queue_wait, Duration::from_hours(1));
        assert_eq!(engine.default_deadline, Duration::from_hours(2));

        // A waiter under the production queue limit outlives the old 250 ms
        // timer, then runs once capacity frees.
        let waiting = owner(saturable(defaults.queue_capacity, defaults.max_queue_wait));
        let held = saturate(&waiting).await;
        let waiting_owner = Arc::clone(&waiting);
        let mut waiter = tokio::spawn(async move {
            waiting_owner
                .admit(waiting_request(
                    QueryClass::Analytical,
                    Duration::from_mins(1),
                    CancellationToken::new(),
                ))
                .await
        });
        assert!(
            tokio::time::timeout(Duration::from_millis(400), &mut waiter)
                .await
                .is_err(),
            "a saturated waiter must remain queued beyond 250 ms"
        );
        drop(held);
        waiter
            .await
            .expect("waiter task")
            .expect("the waiter runs once capacity frees")
            .release();

        // The earlier limit wins for both classes, and each is a query timeout
        // that leaves no queued place behind.
        for class in [QueryClass::Interactive, QueryClass::Analytical] {
            for (queue_limit, total) in [
                (Duration::from_millis(50), Duration::from_secs(30)),
                (Duration::from_secs(30), Duration::from_millis(50)),
            ] {
                let expiring = owner(saturable(4, queue_limit));
                let held = saturate(&expiring).await;
                let started = Instant::now();
                let result = expiring
                    .admit(waiting_request(class, total, CancellationToken::new()))
                    .await;
                assert!(
                    matches!(result, Err(BifrostError::QueryTimeout)),
                    "{class:?} queue {queue_limit:?} total {total:?} did not time out: {:?}",
                    result.err()
                );
                assert!(started.elapsed() < Duration::from_secs(10));
                assert_eq!(expiring.shared.state.lock().expect("state").queued, 0);
                drop(held);
            }
        }
    }

    /// Both query classes share one 1,000-place queue: the 1,001st waiter is
    /// the retryable queue-full overload, and a cancelled or dropped waiter
    /// frees its place.
    ///
    /// # Panics
    ///
    /// Panics when a queue bound or release claim fails.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn queued_queries_share_one_thousand_places() {
        let defaults = OracleAdmissionConfig::default();
        // One thousand waiters of both classes fit; the next is the retryable
        // overload, and cancelling one waiter returns its place.
        let full = owner(saturable(defaults.queue_capacity, defaults.max_queue_wait));
        let held = saturate(&full).await;
        let mut cancellations = Vec::with_capacity(PRODUCTION_QUEUE_PLACES);
        let mut waiters = Vec::with_capacity(PRODUCTION_QUEUE_PLACES);
        for index in 0..PRODUCTION_QUEUE_PLACES {
            let class = if index % 2 == 0 {
                QueryClass::Interactive
            } else {
                QueryClass::Analytical
            };
            let cancellation = CancellationToken::new();
            cancellations.push(cancellation.clone());
            let waiting_owner = Arc::clone(&full);
            waiters.push(tokio::spawn(async move {
                waiting_owner
                    .admit(waiting_request(class, Duration::from_mins(1), cancellation))
                    .await
            }));
        }
        await_queued(&full, 1_000).await;
        assert!(matches!(
            full.admit(waiting_request(
                QueryClass::Interactive,
                Duration::from_mins(1),
                CancellationToken::new(),
            ))
            .await,
            Err(BifrostError::QueryQueueFull)
        ));
        cancellations[0].cancel();
        await_queued(&full, 999).await;
        let replacement_owner = Arc::clone(&full);
        let replacement = tokio::spawn(async move {
            replacement_owner
                .admit(waiting_request(
                    QueryClass::Analytical,
                    Duration::from_mins(1),
                    CancellationToken::new(),
                ))
                .await
        });
        await_queued(&full, 1_000).await;
        // Dropping the waiting future — what a disconnected HTTP client does —
        // frees the place without any cancellation token firing.
        replacement.abort();
        assert!(replacement.await.is_err(), "the replacement was aborted");
        await_queued(&full, 999).await;

        for cancellation in &cancellations {
            cancellation.cancel();
        }
        drop(held);
        for waiter in waiters {
            let _ = waiter.await;
        }
    }

    /// Only a full queue is the queue-full refusal.
    ///
    /// A disabled class and a closed queue keep the admission rejection, and a
    /// queued waiter that reaches its deadline is a timeout; none of them may be
    /// reported as [`BifrostError::QueryQueueFull`].
    ///
    /// # Panics
    ///
    /// Panics when any refusal carries the wrong error.
    #[tokio::test]
    async fn queue_full_is_distinct_from_timeout() {
        let owner = owner(OracleAdmissionConfig {
            interactive_slots: 1,
            analytical_slots: 0,
            tenant_analytical_slots: 0,
            queue_capacity: 1,
            max_queue_wait: Duration::from_secs(30),
            ..Default::default()
        });
        let request = |query_class: QueryClass, wait: Duration| PreparedAdmission {
            tenant: DataTenantId::new_v7(),
            query_class,
            local_ratio: 0.0,
            deadline: Instant::now() + wait,
            cancellation: CancellationToken::new(),
        };
        let long = Duration::from_secs(30);
        let held = owner
            .admit(request(QueryClass::Interactive, long))
            .await
            .expect("the one slot unit is granted");
        assert!(
            matches!(
                owner.admit(request(QueryClass::Analytical, long)).await,
                Err(BifrostError::QueryAdmissionRejected)
            ),
            "a disabled class is not a full queue"
        );

        let queued_owner = Arc::clone(&owner);
        let queued = tokio::spawn(async move {
            queued_owner
                .admit(request(QueryClass::Interactive, long))
                .await
        });
        wait_for_queued(&owner, 1).await;
        assert!(
            matches!(
                owner.admit(request(QueryClass::Interactive, long)).await,
                Err(BifrostError::QueryQueueFull)
            ),
            "the next query after the last waiting place is queue-full"
        );
        queued.abort();
        assert!(queued.await.is_err(), "the queued waiter was aborted");
        wait_for_queued(&owner, 0).await;

        assert!(
            matches!(
                owner
                    .admit(request(QueryClass::Interactive, Duration::from_millis(20)))
                    .await,
                Err(BifrostError::QueryTimeout)
            ),
            "a waiter that reaches its deadline times out"
        );
        owner.close();
        assert!(
            matches!(
                owner.admit(request(QueryClass::Interactive, long)).await,
                Err(BifrostError::QueryAdmissionRejected)
            ),
            "a closed queue is shutdown, not a full queue"
        );
        drop(held);
    }

    /// Local probe release remains idempotent through its watch state.
    #[test]
    fn admission_module_compiles() {}
}
