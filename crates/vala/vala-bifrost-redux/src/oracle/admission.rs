//! Pod-local Oracle query admission and stream-owned resource cleanup.
//!
//! Admission is deliberately local to one process.  Class, tenant, memory,
//! and spill accounting share one synchronous mutex so a grant is one atomic
//! decision; asynchronous callers wait only on their own one-shot channel.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::*;

/// Private pod-local admission limits computed by server boot.
#[derive(Debug, Clone, Copy)]
pub(crate) struct OracleAdmissionConfig {
    /// Interactive class capacity.
    pub(crate) interactive_slots: u32,
    /// Analytical class capacity.
    pub(crate) analytical_slots: u32,
    /// Ceiling for a single locally contending tenant.
    pub(crate) single_tenant_ceiling: u32,
    /// Ceiling once two or more tenants contend locally.
    pub(crate) multi_tenant_ceiling: u32,
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
            single_tenant_ceiling: 8,
            multi_tenant_ceiling: 4,
            queue_capacity: 64,
            max_queue_wait: Duration::from_millis(250),
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
    /// Memory bytes still reserved at the deadline.
    pub reserved_memory_bytes: u64,
    /// Spill bytes still reserved at the deadline.
    pub reserved_spill_bytes: u64,
    /// Pending peer reservations observed at shutdown.
    pub peer_pending: u64,
    /// Running peer reservations observed at shutdown.
    pub peer_running: u64,
}

/// Closed query classes independently scheduled by the pod-local owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum AdmissionClass {
    /// Latency-sensitive interactive work.
    Interactive,
    /// Scan-heavy analytical work.
    Analytical,
}

impl From<QueryClass> for AdmissionClass {
    fn from(value: QueryClass) -> Self {
        match value {
            QueryClass::Interactive => Self::Interactive,
            QueryClass::Analytical => Self::Analytical,
        }
    }
}

/// One queued admission request and its absolute grant deadline.
struct Waiter {
    /// Monotonic queue identity used to remove a canceled waiter.
    id: u64,
    /// Fixed deadline computed at enqueue time.
    deadline: Instant,
    /// Fraction of pinned sealed bytes available from the local hot tier.
    local_ratio: f64,
    /// One-shot notification carrying the granted resource amounts.
    tx: tokio::sync::oneshot::Sender<Grant>,
}

/// Resource amounts atomically granted while the admission mutex is held.
struct Grant {
    /// Class counter charged by the grant.
    class: AdmissionClass,
    /// Tenant counter charged by the grant.
    tenant: DataTenantId,
    /// Memory bytes reserved by the grant.
    memory: u64,
    /// Spill bytes reserved by the grant.
    spill: u64,
    /// Complete memory and scratch envelope transferred to the stream guard.
    resources: Option<crate::resources::OracleQueryResources>,
}

/// One winner notification held until the admission mutex is unlocked.
type GrantNotification = (tokio::sync::oneshot::Sender<Grant>, Grant);

/// FIFO waiters and active count for one locally contending tenant.
struct TenantQueue {
    /// Arrival-ordered requests awaiting a grant.
    waiters: VecDeque<Waiter>,
    /// Number of active grants held by the tenant.
    active: u32,
}

/// Mutex-owned counters and tenant ring for one query class.
struct ClassState {
    /// Total class permits available to local queries.
    capacity: u32,
    /// Number of class permits currently granted.
    used: u32,
    /// Single-contender tenant ceiling.
    tenant_ceiling_single: u32,
    /// Multi-contender tenant ceiling.
    tenant_ceiling_multi: u32,
    /// Memory bytes currently reserved.
    memory_used: u64,
    /// Stable first-enqueue tenant ring.
    tenants: Vec<(DataTenantId, TenantQueue)>,
    /// Next tenant index visited by round-robin grants.
    cursor: usize,
}

impl ClassState {
    /// Creates empty class counters with positive internal floors.
    fn new(capacity: u32, single: u32, multi: u32) -> Self {
        Self {
            capacity: capacity.max(1),
            used: 0,
            tenant_ceiling_single: single.max(1),
            tenant_ceiling_multi: multi.max(1),
            memory_used: 0,
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

    /// Counts tenants with active permits or queued requests.
    fn contender_count(&self) -> u32 {
        u32::try_from(
            self.tenants
                .iter()
                .filter(|(_, queue)| queue.active > 0 || !queue.waiters.is_empty())
                .count()
                .max(1),
        )
        .unwrap_or(u32::MAX)
    }

    /// Computes the current tenant ceiling from local contention.
    fn hard_ceiling(&self) -> u32 {
        let ceiling = if self.contender_count() == 1 {
            self.tenant_ceiling_single
        } else {
            self.tenant_ceiling_multi
        };
        ceiling.min(self.capacity)
    }
}

/// Complete mutable admission state protected by one synchronous mutex.
struct AdmissionState {
    /// Interactive class state.
    interactive: ClassState,
    /// Analytical class state.
    analytical: ClassState,
    /// Pod-global spill bytes retained by all active query grants.
    spill_used: u64,
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
    /// Number of active local query grants.
    active_queries: u64,
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

/// Synchronous local permit released by the aggregate query guard.
struct LocalPermit {
    /// Shared counters released by this permit.
    shared: Arc<AdmissionShared>,
    /// Class counter charged by this permit.
    class: AdmissionClass,
    /// Tenant counter charged by this permit.
    tenant: DataTenantId,
    /// Memory bytes charged by this permit.
    memory: u64,
    /// Spill bytes charged by this permit.
    spill: u64,
    /// Complete resource envelope released before queued work is reconsidered.
    resources: Mutex<Option<crate::resources::OracleQueryResources>>,
    /// Read-only projection of the envelope, retained after it moves away.
    shape: Option<RetainedQuerySessionShape>,
    /// Exactly-once release latch.
    released: AtomicBool,
}

/// The parts of an admitted envelope a query still needs after it moves.
///
/// An Analytical lease transfers this query's [`OracleQueryResources`] into the
/// graph that owns it, because exactly one owner may return the envelope to the
/// governor. Dispatch, session construction, and tail drain still have to name
/// the same memory pool and the same admitted shape afterwards, so the permit
/// keeps this borrow-equivalent copy. Holding the pool handle charges nothing:
/// it is the same tracked pool the envelope owns, not a second reservation.
#[derive(Clone)]
struct RetainedQuerySessionShape {
    /// The tracked pool every operator of this query allocates from.
    pool: Arc<dyn datafusion::execution::memory_pool::MemoryPool>,
    /// The peak counter that pool records into, shared with the envelope.
    memory_peak_bytes: Arc<AtomicUsize>,
    /// The memory ceiling admission granted this query.
    granted_memory_bytes: usize,
    /// Adaptive partition count derived from that grant.
    target_partitions: usize,
}

impl LocalPermit {
    /// Releases all local counters exactly once and wakes queued waiters.
    fn release_inner(&self) {
        if self.released.swap(true, Ordering::AcqRel) {
            return;
        }
        let Ok(mut state) = self.shared.state.lock() else {
            tracing::error!("Oracle admission state lock poisoned during release");
            return;
        };
        let release = Grant {
            class: self.class,
            tenant: self.tenant,
            memory: self.memory,
            spill: self.spill,
            resources: None,
        };
        if let Err(error) = rollback_counts(&mut state, &release) {
            tracing::error!(%error, "Oracle admission permit release failed");
            return;
        }
        let resources = self
            .resources
            .lock()
            .map_or(None, |mut resources| resources.take());
        drop(resources);
        let notifications = grant_waiters(&self.shared, &mut state);
        drop(state);
        notify_grants(&self.shared, notifications);
        self.shared.notify.notify_waiters();
    }
}

impl Drop for LocalPermit {
    /// Performs synchronous release when the aggregate guard is dropped.
    fn drop(&mut self) {
        self.release_inner();
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
    class_kind: AdmissionClass,
    /// Tenant removed from the queue when cancellation wins.
    tenant: DataTenantId,
    /// Monotonic queue identity used by cancellation cleanup.
    waiter_id: u64,
    /// Fixed absolute deadline selected before queue insertion.
    wait_deadline: Instant,
    /// One-shot grant notification owned by the waiting future.
    receiver: tokio::sync::oneshot::Receiver<Grant>,
}

/// Admission owner for bounded local capacity and refreshed membership state.
pub struct OracleAdmission {
    /// Existing peer pending/running slot owner.
    pub(super) slots: Arc<OracleSlotManager>,
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
    pub memory_bytes: u64,
    /// Local peer-worker slot units retained by this query.
    pub peer_slots: u64,
    /// Live-tail fences retained by this query.
    pub tail_fences: u64,
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
    memory_peak_bytes: Option<Arc<AtomicUsize>>,
}

#[cfg(feature = "test-support")]
impl QueryResourceProbe {
    /// Creates the probe after local admission and before stream construction.
    ///
    /// `shape` is the query's retained envelope projection; it is absent only
    /// for a synthetic guard that never held an admitted envelope, and the
    /// grant, current, and peak observations are then all zero.
    #[must_use]
    fn new(
        query_id: QueryId,
        memory_bytes: u64,
        slot_units: u64,
        shape: Option<&RetainedQuerySessionShape>,
    ) -> Self {
        let (snapshot, _) = tokio::sync::watch::channel(QueryResourceSnapshot {
            admission_slots: slot_units,
            memory_bytes,
            peer_slots: slot_units,
            tail_fences: 0,
            granted_memory_bytes: shape.map_or(0, |shape| {
                u64::try_from(shape.granted_memory_bytes).unwrap_or(u64::MAX)
            }),
            pool_current_bytes: 0,
            pool_peak_bytes: 0,
        });
        Self {
            query_id,
            snapshot,
            pool: shape.map(|shape| Arc::clone(&shape.pool)),
            memory_peak_bytes: shape.map(|shape| Arc::clone(&shape.memory_peak_bytes)),
        }
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
    /// Creates a process-local owner with explicit class, tenant, and byte budgets.
    #[must_use]
    pub(super) fn with_config(
        slots: Arc<OracleSlotManager>,
        local_role: RegisteredRole,
        membership_available: bool,
        config: OracleAdmissionConfig,
        resources: crate::resources::OracleResources,
    ) -> Self {
        let root_cancel = CancellationToken::new();
        let state = AdmissionState {
            interactive: ClassState::new(
                config.interactive_slots,
                config.single_tenant_ceiling,
                config.multi_tenant_ceiling,
            ),
            analytical: ClassState::new(
                config.analytical_slots,
                config.single_tenant_ceiling,
                config.multi_tenant_ceiling,
            ),
            spill_used: 0,
            queue_capacity: config.queue_capacity.max(1),
            queued: 0,
            generation: 0,
            closed: false,
            membership_available,
            active_queries: 0,
        };
        Self {
            slots,
            local_role,
            shared: Arc::new(AdmissionShared {
                state: Mutex::new(state),
                notify: tokio::sync::Notify::new(),
                root_cancel,
                max_queue_wait: config.max_queue_wait,
                next_waiter: AtomicU64::new(1),
                resources,
            }),
        }
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

    /// Reports whether a live Oracle role and local class capacity are available.
    #[must_use]
    pub fn is_available(&self) -> bool {
        self.shared.state.lock().is_ok_and(|state| {
            state.membership_available
                && !state.closed
                && (state.interactive.capacity > 0 || state.analytical.capacity > 0)
        })
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
        notify_grants(&self.shared, notifications);
        self.shared.notify.notify_waiters();
    }

    /// Closes future admissions and cancels queued waiters synchronously.
    pub(crate) fn close(&self) {
        if let Ok(mut state) = self.shared.state.lock() {
            state.closed = true;
            for (_, tenant) in &mut state.interactive.tenants {
                tenant.waiters.clear();
            }
            for (_, tenant) in &mut state.analytical.tenants {
                tenant.waiters.clear();
            }
            state.queued = 0;
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
            reserved_memory_bytes: state.interactive.memory_used + state.analytical.memory_used,
            reserved_spill_bytes: state.spill_used,
            peer_pending: self.slots.pending_in_use(),
            peer_running: self.slots.running_in_use(),
        }
    }

    /// Captures current local admission and peer reservations without waiting.
    #[cfg(feature = "test-support")]
    pub(crate) fn runtime_inspection(&self) -> OracleRuntimeInspection {
        let state = self
            .shared
            .state
            .lock()
            .expect("Oracle admission state invariant");
        OracleRuntimeInspection {
            active_queries: state.active_queries,
            queued_queries: u64::from(state.queued),
            reserved_memory_bytes: state.interactive.memory_used + state.analytical.memory_used,
            reserved_spill_bytes: state.spill_used,
            peer_pending: self.slots.pending_in_use(),
            peer_running: self.slots.running_in_use(),
        }
    }

    /// Acquires one class/tenant/memory/spill aggregate with a fixed absolute wait deadline.
    ///
    /// # Errors
    /// Returns admission rejection when the queue is full, closed, cancelled, or the absolute
    /// deadline expires before a grant.
    #[tracing::instrument(name = "bifrost.oracle.admission", skip_all)]
    pub(super) async fn admit(
        self: &Arc<Self>,
        request: PreparedAdmission,
    ) -> Result<AdmittedQueryGuard, BifrostError> {
        self.admit_with_attempt_id(request, None).await
    }

    /// Acquires admission while preserving an ingress-fixed participant-cut identity.
    pub(super) async fn admit_for_attempt(
        self: &Arc<Self>,
        request: PreparedAdmission,
        attempt_id: QueryId,
    ) -> Result<AdmittedQueryGuard, BifrostError> {
        self.admit_with_attempt_id(request, Some(attempt_id)).await
    }

    /// Applies the common bounded admission path with an optional fixed identity.
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
        let class_kind = AdmissionClass::from(query_class);
        let enqueue = Instant::now();
        let wait_deadline = deadline.min(enqueue + self.max_queue_wait());
        let waiter_telemetry = OracleTelemetry::start_admission_waiter(query_class);
        let waiter =
            self.enqueue_waiter(tenant, class_kind, local_ratio, wait_deadline, query_class)?;
        let grant = self
            .wait_for_grant(waiter, cancellation.clone(), query_class)
            .await?;
        OracleTelemetry::record_admission(
            query_class,
            OracleAdmissionOutcome::Admitted,
            OracleAdmissionReason::ClassCapacity,
        );
        let mut waiter_telemetry = waiter_telemetry;
        waiter_telemetry.finish("class", "acquired");
        self.build_admitted_guard(grant, cancellation, attempt_id)
    }

    /// Enqueues one waiter and immediately grants any newly eligible requests.
    ///
    /// # Errors
    /// Returns an internal error when the admission state lock is poisoned, or
    /// a query-admission rejection when shutdown or queue capacity prevents
    /// insertion.
    fn enqueue_waiter(
        &self,
        tenant: DataTenantId,
        class_kind: AdmissionClass,
        local_ratio: f64,
        wait_deadline: Instant,
        query_class: QueryClass,
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
                return Err(BifrostError::OracleRoleUnavailable);
            }
            if state.closed || state.queued >= state.queue_capacity {
                let reason = if state.closed {
                    OracleAdmissionReason::Shutdown
                } else {
                    OracleAdmissionReason::QueueFull
                };
                OracleTelemetry::record_admission(
                    query_class,
                    OracleAdmissionOutcome::Rejected,
                    reason,
                );
                return Err(BifrostError::QueryAdmissionRejected);
            }
            let class = match class_kind {
                AdmissionClass::Interactive => &mut state.interactive,
                AdmissionClass::Analytical => &mut state.analytical,
            };
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
        notify_grants(&self.shared, notifications);
        Ok(PreparedWaiter {
            class_kind,
            tenant,
            waiter_id,
            wait_deadline,
            receiver,
        })
    }

    /// Waits for a grant while preserving absolute deadlines and rollback.
    ///
    /// # Errors
    /// Returns query-admission rejection when the grant channel closes, the
    /// deadline expires, or either cancellation token wins the race.
    async fn wait_for_grant(
        &self,
        waiter: PreparedWaiter,
        cancellation: CancellationToken,
        query_class: QueryClass,
    ) -> Result<Grant, BifrostError> {
        let PreparedWaiter {
            class_kind,
            tenant,
            waiter_id,
            wait_deadline,
            mut receiver,
        } = waiter;
        tokio::select! {
            grant = &mut receiver => grant.map_err(|_| BifrostError::QueryAdmissionRejected),
            () = tokio::time::sleep_until(tokio::time::Instant::from_std(wait_deadline)) => {
                self.rollback_waiter(class_kind, tenant, waiter_id, &mut receiver);
                OracleTelemetry::record_admission(
                    query_class,
                    OracleAdmissionOutcome::Rejected,
                    OracleAdmissionReason::QueueDeadline,
                );
                Err(BifrostError::QueryAdmissionRejected)
            }
            () = cancellation.cancelled() => {
                self.rollback_waiter(class_kind, tenant, waiter_id, &mut receiver);
                OracleTelemetry::record_admission(
                    query_class,
                    OracleAdmissionOutcome::Rejected,
                    OracleAdmissionReason::Shutdown,
                );
                Err(BifrostError::QueryAdmissionRejected)
            }
            () = self.shared.root_cancel.cancelled() => {
                self.rollback_waiter(class_kind, tenant, waiter_id, &mut receiver);
                OracleTelemetry::record_admission(
                    query_class,
                    OracleAdmissionOutcome::Rejected,
                    OracleAdmissionReason::Shutdown,
                );
                Err(BifrostError::QueryAdmissionRejected)
            }
        }
    }

    /// Removes a canceled waiter and returns any raced grant to the pool.
    fn rollback_waiter(
        &self,
        class_kind: AdmissionClass,
        tenant: DataTenantId,
        waiter_id: u64,
        receiver: &mut tokio::sync::oneshot::Receiver<Grant>,
    ) {
        self.cancel_waiter(class_kind, tenant, waiter_id);
        if let Ok(grant) = receiver.try_recv() {
            self.rollback_grant(grant);
        }
    }

    /// Converts one granted aggregate into the stream-owned lifecycle guard.
    ///
    /// # Errors
    /// Returns [`BifrostError::OracleRoleUnavailable`] when this role was fenced
    /// out after local admission; the aggregate permit is released on that path.
    fn build_admitted_guard(
        &self,
        mut grant: Grant,
        cancellation: CancellationToken,
        attempt_id: Option<QueryId>,
    ) -> Result<AdmittedQueryGuard, BifrostError> {
        let permit = LocalPermit {
            shared: Arc::clone(&self.shared),
            class: grant.class,
            tenant: grant.tenant,
            memory: grant.memory,
            spill: grant.spill,
            shape: grant
                .resources
                .as_ref()
                .map(|resources| RetainedQuerySessionShape {
                    pool: resources.memory_pool(),
                    memory_peak_bytes: resources.memory_peak_bytes(),
                    granted_memory_bytes: resources.granted_memory_bytes,
                    target_partitions: resources.target_partitions,
                }),
            resources: Mutex::new(grant.resources.take()),
            released: AtomicBool::new(false),
        };
        let local_node = self.local_role.key.node_id;
        let membership_available = self
            .shared
            .state
            .lock()
            .is_ok_and(|state| state.membership_available);
        if !membership_available {
            drop(permit);
            return Err(BifrostError::OracleRoleUnavailable);
        }
        let query_id = attempt_id.unwrap_or_else(|| QueryId::new(uuid::Uuid::now_v7()));
        Ok(AdmittedQueryGuard {
            analytical: None,
            query_id,
            leader: AdmittedLeader {
                node_id: local_node,
                fencing_token: self.local_role.fencing_token,
            },
            physical_projections: Vec::new(),
            local_permit: Some(permit),
            delegated_grant: None,
            live_reservations: Vec::new(),
            cancellation: self.shared.root_cancel.child_token(),
            request_cancellation: cancellation,
            distributed_settlement: Arc::new(DistributedQuerySettlement::default()),
            #[cfg(feature = "test-support")]
            resource_probe: None,
        })
    }

    /// Returns the immutable absolute queue wait cap.
    fn max_queue_wait(&self) -> Duration {
        self.shared.max_queue_wait
    }

    /// Removes one canceled waiter and immediately re-runs grant scheduling.
    fn cancel_waiter(&self, class_kind: AdmissionClass, tenant: DataTenantId, waiter_id: u64) {
        let notifications = if let Ok(mut state) = self.shared.state.lock() {
            let class = match class_kind {
                AdmissionClass::Interactive => &mut state.interactive,
                AdmissionClass::Analytical => &mut state.analytical,
            };
            if let Some((_, queue)) = class.tenants.iter_mut().find(|(id, _)| *id == tenant)
                && let Some(index) = queue
                    .waiters
                    .iter()
                    .position(|waiter| waiter.id == waiter_id)
            {
                queue.waiters.remove(index);
                state.queued = state.queued.saturating_sub(1);
            }
            grant_waiters(&self.shared, &mut state)
        } else {
            Vec::new()
        };
        notify_grants(&self.shared, notifications);
        self.shared.notify.notify_waiters();
    }

    /// Returns a raced notification grant to the mutex-owned counters.
    fn rollback_grant(&self, mut grant: Grant) {
        drop(grant.resources.take());
        let notifications = if let Ok(mut state) = self.shared.state.lock() {
            if let Err(error) = rollback_counts(&mut state, &grant) {
                tracing::error!(%error, "Oracle admission rollback failed");
                return;
            }
            grant_waiters(&self.shared, &mut state)
        } else {
            Vec::new()
        };
        notify_grants(&self.shared, notifications);
        self.shared.notify.notify_waiters();
    }
}

/// Grants queued waiters in class-local weighted rounds while holding admission state.
fn grant_waiters(
    shared: &Arc<AdmissionShared>,
    state: &mut AdmissionState,
) -> Vec<GrantNotification> {
    let _ = state.generation;
    let mut notifications = Vec::new();
    for (kind, class) in [
        (AdmissionClass::Interactive, &mut state.interactive),
        (AdmissionClass::Analytical, &mut state.analytical),
    ] {
        if class.tenants.is_empty() {
            continue;
        }
        let rounds = class.tenants.len();
        let mut made_progress = true;
        while class.used < class.capacity && made_progress {
            made_progress = false;
            for _ in 0..rounds {
                if class.used >= class.capacity {
                    break;
                }
                let index = class.cursor % class.tenants.len();
                class.cursor = (class.cursor + 1) % class.tenants.len();
                let contenders = class.contender_count();
                let guarantee = (class.capacity / contenders)
                    .max(1)
                    .min(class.hard_ceiling());
                let borrowed = class
                    .tenants
                    .iter()
                    .enumerate()
                    .all(|(other, (_, q))| other == index || q.waiters.is_empty());
                let hard_ceiling = class.hard_ceiling();
                let waiter = {
                    let (_, queue) = &mut class.tenants[index];
                    if queue.waiters.front().is_none()
                        || (queue.active >= guarantee && !borrowed)
                        || queue.active >= hard_ceiling
                    {
                        continue;
                    }
                    queue.waiters.pop_front().expect("checked waiter exists")
                };
                if waiter.deadline <= Instant::now() {
                    state.queued = state.queued.saturating_sub(1);
                    made_progress = true;
                    continue;
                }
                let Ok(resources) = acquire_waiter_resources(shared, kind, waiter.local_ratio)
                else {
                    class.tenants[index].1.waiters.push_front(waiter);
                    continue;
                };
                let memory = u64::try_from(resources.memory_bytes).unwrap_or(u64::MAX);
                let spill = resources.scratch_bytes;
                let tenant = class.tenants[index].0;
                class.used += 1;
                class.memory_used += memory;
                state.spill_used += spill;
                class.tenants[index].1.active += 1;
                state.queued = state.queued.saturating_sub(1);
                state.active_queries += 1;
                made_progress = true;
                notifications.push((
                    waiter.tx,
                    Grant {
                        class: kind,
                        tenant,
                        memory,
                        spill,
                        resources: Some(resources),
                    },
                ));
            }
        }
    }
    notifications
}

/// Acquires the exact root resource quantum for one queued admission class.
///
/// # Errors
///
/// Returns [`crate::resources::BifrostResourceError`] when the root governor
/// cannot cover the class quantum without violating Task 01 accounting.
fn acquire_waiter_resources(
    shared: &AdmissionShared,
    kind: AdmissionClass,
    local_ratio: f64,
) -> Result<crate::resources::OracleQueryResources, crate::resources::BifrostResourceError> {
    let query_class = match kind {
        AdmissionClass::Interactive => QueryClass::Interactive,
        AdmissionClass::Analytical => QueryClass::Analytical,
    };
    shared
        .resources
        .try_acquire_query(crate::resources::OracleResourceRequest::for_class(
            query_class,
            local_ratio,
        ))
}

/// Notifies winners after releasing the admission mutex and repairs canceled sends.
fn notify_grants(shared: &Arc<AdmissionShared>, mut notifications: Vec<GrantNotification>) {
    let mut pending: VecDeque<GrantNotification> = notifications.drain(..).collect();
    while let Some((sender, grant)) = pending.pop_front() {
        if let Err(mut grant) = sender.send(grant) {
            drop(grant.resources.take());
            let next = if let Ok(mut state) = shared.state.lock() {
                match rollback_counts(&mut state, &grant) {
                    Ok(()) => grant_waiters(shared, &mut state),
                    Err(error) => {
                        tracing::error!(%error, "Oracle admission notification rollback failed");
                        Vec::new()
                    }
                }
            } else {
                Vec::new()
            };
            pending.extend(next);
        }
    }
}

/// Reverses one grant's class, tenant, memory, spill, and active counters.
fn rollback_counts(state: &mut AdmissionState, grant: &Grant) -> Result<(), BifrostError> {
    state.spill_used =
        state
            .spill_used
            .checked_sub(grant.spill)
            .ok_or_else(|| BifrostError::Internal {
                detail: "Oracle admission spill rollback underflow".to_owned(),
            })?;
    let class = match grant.class {
        AdmissionClass::Interactive => &mut state.interactive,
        AdmissionClass::Analytical => &mut state.analytical,
    };
    class.memory_used =
        class
            .memory_used
            .checked_sub(grant.memory)
            .ok_or_else(|| BifrostError::Internal {
                detail: "Oracle admission memory rollback underflow".to_owned(),
            })?;
    if let Some((_, tenant)) = class.tenants.iter_mut().find(|(id, _)| *id == grant.tenant) {
        tenant.active = tenant
            .active
            .checked_sub(1)
            .ok_or_else(|| BifrostError::Internal {
                detail: "Oracle admission tenant rollback underflow".to_owned(),
            })?;
    }
    class.used = class
        .used
        .checked_sub(1)
        .ok_or_else(|| BifrostError::Internal {
            detail: "Oracle admission class rollback underflow".to_owned(),
        })?;
    state.active_queries =
        state
            .active_queries
            .checked_sub(1)
            .ok_or_else(|| BifrostError::Internal {
                detail: "Oracle admission active-query rollback underflow".to_owned(),
            })?;
    Ok(())
}

/// Stream-owned local capacity cleanup.
pub(super) struct AdmittedQueryGuard {
    /// Stable query identity used by peer dispatch and test probes.
    pub(super) query_id: QueryId,
    /// Fenced leader selected at admission time.
    pub(super) leader: AdmittedLeader,
    /// Physical table projections charged to this query's admitted pool.
    physical_projections: Vec<
        crate::catalog::PhysicalTableProjection<crate::resources::OracleQueryMemoryReservation>,
    >,
    /// Aggregate leader-local class, tenant, memory, and spill permit.
    local_permit: Option<LocalPermit>,
    /// Delegated global, tenant, and principal policy unit retained for the attempt.
    delegated_grant: Option<super::DelegatedOracleAdmissionGrant>,
    /// Canonical local slot-use gauge retained with the running permit.
    /// Parent reservations retaining drained live batches through stream cleanup.
    pub(super) live_reservations: Vec<AccountedMemoryReservation>,
    /// Inactive Analytical graph and attempt ownership retained until cleanup.
    ///
    /// Present only on an attempt that the production-unreachable Analytical
    /// path leased a session for. Dropping this guard drops those owners, which
    /// is what returns the exchange-buffer and scratch children to the query
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
    /// Starts one partition only while query cancellation remains open.
    pub(super) fn start(
        self: &Arc<Self>,
        cancellation: &CancellationToken,
    ) -> Option<DistributedPartitionGuard> {
        let mut active = self.active.lock().ok()?;
        if cancellation.is_cancelled() {
            return None;
        }
        *active = active.checked_add(1)?;
        Some(DistributedPartitionGuard {
            settlement: Arc::clone(self),
        })
    }

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

/// RAII terminal for one started distributed partition future.
pub(super) struct DistributedPartitionGuard {
    /// Shared query settlement updated on every future terminal or drop.
    settlement: Arc<DistributedQuerySettlement>,
}

impl Drop for DistributedPartitionGuard {
    fn drop(&mut self) {
        if let Ok(mut active) = self.settlement.active.lock() {
            *active = active.saturating_sub(1);
        }
        self.settlement.settled.notify_waiters();
    }
}

impl Drop for AdmittedQueryGuard {
    /// Cancels the query and synchronously releases leader-local resources.
    fn drop(&mut self) {
        self.cancellation.cancel();
        self.live_reservations.clear();
        self.physical_projections.clear();
        // Before the permit: an Analytical graph owns this query's envelope, and
        // the permit's release wakes queued waiters. Returning the envelope
        // second would let the next query be admitted while this one's capacity
        // is still charged to the governor.
        self.analytical.take();
        if let Some(permit) = self.local_permit.take() {
            permit.release_inner();
        }
        self.delegated_grant.take();
        #[cfg(feature = "test-support")]
        if let Some(probe) = &self.resource_probe {
            probe.release_local();
        }
    }
}

impl AdmittedQueryGuard {
    /// Takes the inactive Analytical ownership so the stream can settle it.
    ///
    /// Taking rather than borrowing is deliberate: settlement consumes the two
    /// guards, and removing them here means a later drop of this admission
    /// cannot settle or release the same attempt a second time.
    ///
    /// This query's physical projections are released first because they are
    /// children of the very envelope the graph is about to return. Settling
    /// with them still live makes the graph refuse to release, which strands
    /// the graph on this node instead of returning its capacity.
    pub(super) fn take_analytical(
        &mut self,
    ) -> Option<super::analytical::AnalyticalAttemptOwnership> {
        self.physical_projections.clear();
        self.analytical.take()
    }

    /// Installs one already-admitted envelope for an ownership-transfer test.
    #[cfg(test)]
    pub(super) fn install_query_resources_for_test(
        &mut self,
        resources: crate::resources::OracleQueryResources,
    ) {
        let permit = self
            .local_permit
            .as_ref()
            .expect("a test guard always holds its local permit");
        *permit
            .resources
            .lock()
            .expect("fixture permit resources lock is uncontended") = Some(resources);
    }

    /// Takes this query's admitted envelope so its Analytical graph can own it.
    ///
    /// An Analytical leader must not admit a second envelope for a query that
    /// already holds one, so the graph is registered with *this* envelope rather
    /// than a fresh acquisition. Taking rather than sharing keeps a single
    /// owner: from here the graph releases it, and this guard's own drop only
    /// returns the admission counters.
    ///
    /// Returns `None` when the query has no live local permit, which is the
    /// refusal path — a query about to be rejected has no envelope to lend.
    pub(super) fn take_query_resources(
        &mut self,
    ) -> Option<crate::resources::OracleQueryResources> {
        self.local_permit
            .as_ref()
            .and_then(|permit| permit.resources.lock().ok())
            .and_then(|mut resources| resources.take())
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

    /// Retains the already-acquired delegated policy unit through stream settlement.
    pub(super) fn retain_delegated_grant(&mut self, grant: super::DelegatedOracleAdmissionGrant) {
        self.delegated_grant = Some(grant);
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
        let permit = self
            .local_permit
            .as_ref()
            .ok_or(BifrostError::QueryAdmissionRejected)?;
        let resources = permit
            .resources
            .lock()
            .map_err(|_| BifrostError::QueryAdmissionRejected)?;
        let resources = resources
            .as_ref()
            .ok_or(BifrostError::QueryAdmissionRejected)?;
        let mut projections = Vec::with_capacity(bindings.len());
        for binding in bindings {
            let logical = crate::catalog::LogicalTableIdentity::try_new(
                &permit.tenant,
                &binding.tenant,
                &binding.table_ref,
            )
            .map_err(|_| BifrostError::QueryAdmissionRejected)?;
            let projection = logical
                .try_project(crate::catalog::PhysicalProjectionRole::Oracle, |facts| {
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

    /// Returns the immutable spill share retained by this admitted query.
    #[must_use]
    pub(super) fn spill_limit_bytes(&self) -> u64 {
        self.local_permit.as_ref().map_or(0, |permit| permit.spill)
    }

    /// Builds the tracked greedy pool owned by this query's exact envelope.
    #[must_use]
    pub(super) fn memory_pool(
        &self,
    ) -> Option<Arc<dyn datafusion::execution::memory_pool::MemoryPool>> {
        self.local_permit
            .as_ref()
            .and_then(|permit| permit.shape.as_ref())
            .map(|shape| Arc::clone(&shape.pool))
    }

    /// Returns the memory ceiling admission granted this query.
    ///
    /// Falls back to the working-memory floor when the permit is absent, which
    /// is the same conservative direction every other accessor here takes: a
    /// query with no live permit is about to be refused, and a floor-sized
    /// session shape cannot over-commit memory on its way out.
    #[must_use]
    pub(super) fn granted_memory_bytes(&self) -> usize {
        self.local_permit
            .as_ref()
            .and_then(|permit| permit.shape.as_ref())
            .map_or(
                crate::resources::ORACLE_PARTITION_WORKING_MEMORY_BYTES,
                |shape| shape.granted_memory_bytes,
            )
    }

    /// Returns adaptive target partitions calculated with the admitted grant.
    #[must_use]
    pub(super) fn target_partitions(&self) -> usize {
        self.local_permit
            .as_ref()
            .and_then(|permit| permit.shape.as_ref())
            .map_or(1, |shape| shape.target_partitions)
    }

    /// Attaches exact query-keyed ownership after live-tail drain completes.
    #[cfg(feature = "test-support")]
    pub(super) fn attach_resource_probe(&mut self) -> Arc<QueryResourceProbe> {
        let memory_bytes = self
            .live_reservations
            .iter()
            .map(|reservation| reservation.bytes as u64)
            .sum();
        let slot_units = self.local_permit.as_ref().map_or(0_u64, |_| 1_u64);
        let probe = Arc::new(QueryResourceProbe::new(
            self.query_id,
            memory_bytes,
            slot_units,
            self.local_permit
                .as_ref()
                .and_then(|permit| permit.shape.as_ref()),
        ));
        self.resource_probe = Some(Arc::clone(&probe));
        probe
    }

    /// Cancels the query and releases all local resources synchronously.
    pub(super) fn release(mut self) {
        self.cancellation.cancel();
        // Before the permit: the Analytical graph holds this query's envelope,
        // and returning admission counters first would let a queued waiter be
        // admitted while the envelope it needs is still charged.
        self.analytical.take();
        self.live_reservations.clear();
        self.physical_projections.clear();
        if let Some(permit) = self.local_permit.take() {
            permit.release_inner();
        }
        self.delegated_grant.take();
        #[cfg(feature = "test-support")]
        if let Some(probe) = &self.resource_probe {
            probe.release_local();
        }
    }
}

#[cfg(test)]
/// Builds one real stream-owned guard for lifecycle tests without a cluster dependency.
pub(super) fn admitted_guard_for_test()
-> (AdmittedQueryGuard, Arc<AdmissionShared>, CancellationToken) {
    let shared = Arc::new(AdmissionShared {
        state: Mutex::new(AdmissionState {
            interactive: ClassState::new(1, 1, 1),
            analytical: ClassState::new(1, 1, 1),
            spill_used: 0,
            queue_capacity: 1,
            queued: 0,
            generation: 0,
            closed: false,
            membership_available: true,
            active_queries: 1,
        }),
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
    let mut state = shared.state.lock().expect("state");
    let index = state.interactive.tenant_index(tenant);
    state.interactive.used = 1;
    state.interactive.memory_used = 1024;
    state.interactive.tenants[index].1.active = 1;
    drop(state);
    let cancellation = shared.root_cancel.child_token();
    let request_cancellation = CancellationToken::new();
    (
        AdmittedQueryGuard {
            analytical: None,
            query_id: QueryId::new(uuid::Uuid::now_v7()),
            leader: AdmittedLeader {
                node_id: NodeId::new(uuid::Uuid::now_v7()),
                fencing_token: 1,
            },
            physical_projections: Vec::new(),
            local_permit: Some(LocalPermit {
                shared: Arc::clone(&shared),
                class: AdmissionClass::Interactive,
                tenant,
                memory: 1024,
                spill: 0,
                resources: Mutex::new(None),
                shape: None,
                released: AtomicBool::new(false),
            }),
            delegated_grant: None,
            live_reservations: Vec::new(),
            cancellation: cancellation.clone(),
            request_cancellation: request_cancellation.clone(),
            distributed_settlement: Arc::new(DistributedQuerySettlement::default()),
            #[cfg(feature = "test-support")]
            resource_probe: None,
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
        capabilities: ClusterCapabilities::OracleV1(wyrd_spec::vala::api::OracleCapabilitiesV1 {
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
    Arc::new(OracleAdmission::with_config(
        Arc::new(OracleSlotManager::new(1, 1)),
        role,
        true,
        config,
        resources,
    ))
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
        Arc::new(AdmissionShared {
            state: Mutex::new(AdmissionState {
                interactive: ClassState::new(
                    config.interactive_slots,
                    config.single_tenant_ceiling,
                    config.multi_tenant_ceiling,
                ),
                analytical: ClassState::new(
                    config.analytical_slots,
                    config.single_tenant_ceiling,
                    config.multi_tenant_ceiling,
                ),
                spill_used: 0,
                queue_capacity: config.queue_capacity,
                queued: 0,
                generation: 0,
                closed: false,
                membership_available: true,
                active_queries: 0,
            }),
            notify: tokio::sync::Notify::new(),
            root_cancel: CancellationToken::new(),
            max_queue_wait: config.max_queue_wait,
            next_waiter: AtomicU64::new(1),
            resources: test_resources(),
        })
    }

    /// Builds the production admission owner without a database-backed registry.
    /// Builds an Oracle capability whose shared slot pool holds two units.
    ///
    /// Contention between the classes is expressed through the one shared slot
    /// ceiling rather than through a memory budget. Admission charges a
    /// slot-unit quantum now, so a memory-sized fixture no longer forces the
    /// interactive-blocks-analytical ordering this covers; a two-unit ceiling
    /// does, because one interactive query holds a unit and an analytical query
    /// needs both.
    ///
    /// # Panics
    ///
    /// Panics when the deterministic plan or role composition fails.
    fn two_slot_resources() -> crate::resources::OracleResources {
        let mut policy = crate::resources::BifrostResourcePolicy {
            roles: [crate::resources::BifrostRole::Oracle]
                .into_iter()
                .collect(),
            memory_limit_bytes: None,
            unmanaged_reserve_bytes: None,
            scratch_limit_bytes: None,
            effective_cpu: None,
            oracle_query_slot_limit: None,
            scratch_root: std::path::PathBuf::new(),
            volume_roots: None,
        };
        policy.oracle_query_slot_limit = Some(2);
        crate::resources::BifrostRuntimeResources::from_snapshot(
            crate::resources::SystemResourceSnapshot {
                memory_limit_bytes: 1024 * 1024 * 1024,
                effective_cpu: 8,
                scratch_capacity_bytes: 2 * 1024 * 1024 * 1024,
                scratch_available_bytes: 2 * 1024 * 1024 * 1024,
                memory_source: crate::resources::ResourceSource::Injected,
                cpu_source: crate::resources::ResourceSource::Injected,
            },
            policy,
        )
        .expect("two-slot Oracle plan")
        .compose_roles()
        .expect("two-slot Oracle composition")
        .oracle()
        .expect("composition must enable the Oracle capability")
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
        Arc::new(OracleAdmission::with_config(
            Arc::new(OracleSlotManager::new(1, 1)),
            role,
            true,
            config,
            test_resources(),
        ))
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
                .projection_facts(crate::catalog::PhysicalProjectionRole::Oracle)
                .expect("checked projection facts")
                .material_bytes;
        let pool = admitted.memory_pool().expect("admitted query memory pool");
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
        let pool = admitted.memory_pool().expect("admitted query memory pool");

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
        assert_eq!(state.interactive.used, 1);
        drop(state);
        notify_grants(&shared, notifications);
        assert!(rx.try_recv().is_ok());
    }

    /// Tenant contention switches to the multi-tenant ceiling without revoking active permits.
    #[test]
    fn local_admission_enforces_tenant_budget() {
        let shared = shared(OracleAdmissionConfig {
            interactive_slots: 4,
            single_tenant_ceiling: 4,
            multi_tenant_ceiling: 1,
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
        assert_eq!(state.interactive.used, 2);
        assert!(
            state
                .interactive
                .tenants
                .iter()
                .all(|(_, queue)| queue.active <= 1)
        );
        drop(state);
        notify_grants(&shared, notifications);
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
            single_tenant_ceiling: 3,
            multi_tenant_ceiling: 3,
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
        assert_eq!(state.interactive.used, 3);
        assert_eq!(state.interactive.tenants[0].1.active, 2);
        assert_eq!(state.interactive.tenants[1].1.active, 1);
        drop(state);
        notify_grants(&shared, notifications);
        assert_eq!(
            receivers
                .into_iter()
                .map(|mut rx| usize::from(rx.try_recv().is_ok()))
                .sum::<usize>(),
            3
        );
    }

    /// A canceled winner rolls back every charged counter before the next grant pass.
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
        notify_grants(&shared, notifications);
        let _ = live_rx.try_recv().expect("replacement waiter grant");
        let state = shared.state.lock().expect("state");
        assert_eq!(state.interactive.used, 1);
        assert_eq!(state.active_queries, 1);
        assert!(state.interactive.memory_used > 0);
        assert!(state.spill_used > 0);
    }

    /// The admission queue never grants beyond its configured waiter bound.
    #[tokio::test]
    async fn local_admission_queue_capacity_is_bounded() {
        let owner = owner(OracleAdmissionConfig {
            interactive_slots: 1,
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
            Err(BifrostError::QueryAdmissionRejected)
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
        assert!(matches!(result, Err(BifrostError::QueryAdmissionRejected)));
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
        let owner = owner_with_resources(OracleAdmissionConfig::default(), two_slot_resources());
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
        assert!(matches!(timeout, Err(BifrostError::QueryAdmissionRejected)));
        drop(held);
    }

    /// Production admission releases local ownership across every terminal edge.
    #[tokio::test]
    async fn production_admission_release_matrix() {
        production_release_prefix().await;

        let request_owner = owner(OracleAdmissionConfig {
            interactive_slots: 1,
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

    /// A failed aggregate grant restores every local counter synchronously.
    #[test]
    fn local_admission_partial_acquire_rolls_back() {
        let shared = shared(OracleAdmissionConfig::default());
        let tenant = DataTenantId::new_v7();
        let mut state = shared.state.lock().expect("state");
        let index = state.interactive.tenant_index(tenant);
        state.interactive.used = 1;
        state.interactive.memory_used = 1;
        state.interactive.tenants[index].1.active = 1;
        state.active_queries = 1;
        rollback_counts(
            &mut state,
            &Grant {
                class: AdmissionClass::Interactive,
                tenant,
                memory: 1,
                spill: 0,
                resources: None,
            },
        )
        .expect("seeded grant counters must roll back exactly");
        assert_eq!(state.interactive.used, 0);
        assert_eq!(state.interactive.memory_used, 0);
        assert_eq!(state.interactive.tenants[index].1.active, 0);
        assert_eq!(state.active_queries, 0);
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
        let state = owner.shared.state.lock().expect("state");
        assert_eq!(state.analytical.used, 0);
        assert_eq!(state.analytical.memory_used, 0);
        assert_eq!(state.spill_used, 0);
        assert_eq!(state.active_queries, 0);
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
    #[test]
    fn membership_refresh_creates_future_generation() {
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

    /// Queues one waiter directly so several requests contend in a single pass.
    ///
    /// Production enqueues run a grant pass on every arrival. Tenant rotation and
    /// the multi-tenant ceiling are only observable once the queue already holds
    /// the competing requests, so this seeds the queue and the caller runs one
    /// pass by hand.
    fn push_waiter(
        shared: &Arc<AdmissionShared>,
        class_kind: AdmissionClass,
        tenant: DataTenantId,
        id: u64,
        deadline: Instant,
    ) -> tokio::sync::oneshot::Receiver<Grant> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let mut state = shared.state.lock().expect("state");
        let class = match class_kind {
            AdmissionClass::Interactive => &mut state.interactive,
            AdmissionClass::Analytical => &mut state.analytical,
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
        notify_grants(shared, notifications);
    }

    /// A saturated Analytical class never consumes Interactive capacity, per-tenant
    /// FIFO holds, and the tenant cursor grants a waiting peer tenant before it
    /// returns to the first tenant.
    ///
    /// Rejection is checked against the same process root so a refused request is
    /// proven to leave class, tenant, memory, scratch, and slot ownership exactly
    /// where it found it.
    #[test]
    fn analytical_saturation_preserves_interactive_floor_and_rotates_tenants() {
        let resources = test_resources();
        let config = OracleAdmissionConfig {
            interactive_slots: 2,
            analytical_slots: 1,
            single_tenant_ceiling: 2,
            multi_tenant_ceiling: 1,
            ..Default::default()
        };
        let owner = owner_with_resources(config, resources.clone());
        let shared = Arc::clone(&owner.shared);
        let baseline = resources.snapshot().expect("root baseline");
        let tenant_a = DataTenantId::new_v7();
        let tenant_b = DataTenantId::new_v7();
        let deadline = Instant::now() + Duration::from_secs(30);

        let mut analytical =
            push_waiter(&shared, AdmissionClass::Analytical, tenant_a, 1, deadline);
        let mut first_a = push_waiter(&shared, AdmissionClass::Interactive, tenant_a, 2, deadline);
        let mut second_a = push_waiter(&shared, AdmissionClass::Interactive, tenant_a, 3, deadline);
        let mut first_b = push_waiter(&shared, AdmissionClass::Interactive, tenant_b, 4, deadline);
        drain_grants(&shared);

        let analytical_grant = analytical.try_recv().expect("analytical class grant");
        let first_a_grant = first_a
            .try_recv()
            .expect("interactive tenant A is grantable while the analytical class is full");
        let first_b_grant = first_b
            .try_recv()
            .expect("the tenant cursor reaches peer tenant B before tenant A repeats");
        assert!(
            second_a.try_recv().is_err(),
            "the multi-tenant ceiling holds tenant A's second interactive request"
        );
        {
            let state = shared.state.lock().expect("state");
            assert_eq!(state.analytical.used, 1);
            assert_eq!(state.interactive.used, 2);
            assert_eq!(state.interactive.tenants[0].1.active, 1);
            assert_eq!(state.interactive.tenants[1].1.active, 1);
            assert_eq!(state.queued, 1);
        }

        owner.rollback_grant(first_b_grant);
        let second_a_grant = second_a
            .try_recv()
            .expect("tenant A's queued request is granted once the peer tenant releases");
        assert_eq!(shared.state.lock().expect("state").queued, 0);

        owner.rollback_grant(first_a_grant);
        owner.rollback_grant(second_a_grant);
        owner.rollback_grant(analytical_grant);
        {
            let state = shared.state.lock().expect("state");
            assert_eq!(state.interactive.used, 0);
            assert_eq!(state.analytical.used, 0);
            assert_eq!(state.interactive.memory_used, 0);
            assert_eq!(state.analytical.memory_used, 0);
            assert_eq!(state.spill_used, 0);
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

        let rejecting = owner_with_resources(
            OracleAdmissionConfig {
                interactive_slots: 1,
                queue_capacity: 1,
                ..config
            },
            resources.clone(),
        );
        let mut held = rejecting
            .enqueue_waiter(
                tenant_a,
                AdmissionClass::Interactive,
                0.0,
                deadline,
                QueryClass::Interactive,
            )
            .expect("first interactive request is admitted");
        let mut waiting = rejecting
            .enqueue_waiter(
                tenant_a,
                AdmissionClass::Interactive,
                0.0,
                deadline,
                QueryClass::Interactive,
            )
            .expect("second interactive request occupies the finite queue");
        let before_rejection = resources.snapshot().expect("pre-rejection root");
        let rejected = rejecting.enqueue_waiter(
            tenant_a,
            AdmissionClass::Interactive,
            0.0,
            deadline,
            QueryClass::Interactive,
        );
        assert!(matches!(
            rejected,
            Err(BifrostError::QueryAdmissionRejected)
        ));
        {
            let state = rejecting.shared.state.lock().expect("state");
            assert_eq!(state.interactive.used, 1);
            assert_eq!(state.queued, 1);
            assert_eq!(state.active_queries, 1);
        }
        assert_eq!(
            resources.snapshot().expect("post-rejection root"),
            before_rejection,
            "a refused request charges no class, tenant, memory, scratch, or slot ownership"
        );

        let held_grant = held.receiver.try_recv().expect("held interactive grant");
        rejecting.rollback_grant(held_grant);
        let waiting_grant = waiting
            .receiver
            .try_recv()
            .expect("the queued request is granted by the release");
        rejecting.rollback_grant(waiting_grant);
        assert_eq!(
            resources.snapshot().expect("final root"),
            baseline,
            "rejection and release leave the process root at its exact baseline"
        );
    }

    /// Local probe release remains idempotent through its watch state.
    #[test]
    fn admission_module_compiles() {}
}
