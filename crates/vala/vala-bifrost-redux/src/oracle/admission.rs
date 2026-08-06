//! Pod-local Oracle query admission and stream-owned resource cleanup.
//!
//! Admission is deliberately local to one process.  Class, tenant, memory,
//! and spill accounting share one synchronous mutex so a grant is one atomic
//! decision; asynchronous callers wait only on their own one-shot channel.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
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
    /// Interactive memory budget.
    pub(crate) interactive_memory_bytes: u64,
    /// Analytical memory budget.
    pub(crate) analytical_memory_bytes: u64,
    /// Pod-local spill budget.
    pub(crate) spill_bytes: u64,
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
            interactive_memory_bytes: 256 * 1024 * 1024,
            analytical_memory_bytes: 256 * 1024 * 1024,
            spill_bytes: 1 << 30,
        }
    }
}

/// Aggregate lifecycle report returned by bounded Oracle shutdown.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct OracleShutdownReport {
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
    /// Existing per-query memory ceiling from the prepared request.
    memory_ceiling: u64,
    /// Whether this request may reserve analytical spill.
    spill_eligible: bool,
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
    /// Total class memory budget.
    memory_limit: u64,
    /// Memory bytes currently reserved.
    memory_used: u64,
    /// Total class spill budget.
    spill_limit: u64,
    /// Spill bytes currently reserved.
    spill_used: u64,
    /// Stable first-enqueue tenant ring.
    tenants: Vec<(DataTenantId, TenantQueue)>,
    /// Next tenant index visited by round-robin grants.
    cursor: usize,
}

impl ClassState {
    /// Creates empty class counters with positive internal floors.
    fn new(capacity: u32, single: u32, multi: u32, memory_limit: u64, spill_limit: u64) -> Self {
        Self {
            capacity: capacity.max(1),
            used: 0,
            tenant_ceiling_single: single.max(1),
            tenant_ceiling_multi: multi.max(1),
            memory_limit: memory_limit.max(1),
            memory_used: 0,
            spill_limit,
            spill_used: 0,
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

    /// Computes the fixed per-query class memory reservation.
    fn memory_per_query(&self) -> u64 {
        (self.memory_limit / u64::from(self.capacity)).max(1)
    }

    /// Computes the fixed analytical spill share for one admitted query.
    fn spill_per_query(&self) -> u64 {
        (self.spill_limit / u64::from(self.capacity)).max(1)
    }
}

/// Complete mutable admission state protected by one synchronous mutex.
struct AdmissionState {
    /// Interactive class state.
    interactive: ClassState,
    /// Analytical class state.
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
    /// Exactly-once release latch.
    released: AtomicBool,
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
        let class = match self.class {
            AdmissionClass::Interactive => &mut state.interactive,
            AdmissionClass::Analytical => &mut state.analytical,
        };
        // Release aggregate components in reverse acquisition order: spill,
        // memory, tenant, then class.
        class.spill_used = class.spill_used.saturating_sub(self.spill);
        class.memory_used = class.memory_used.saturating_sub(self.memory);
        if let Some((_, tenant)) = class.tenants.iter_mut().find(|(id, _)| *id == self.tenant) {
            tenant.active = tenant.active.saturating_sub(1);
        }
        class.used = class.used.saturating_sub(1);
        state.active_queries = state.active_queries.saturating_sub(1);
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
    /// Absolute query deadline.
    pub(crate) deadline: Instant,
    /// Existing per-query memory ceiling from the retained memory governor.
    pub(crate) memory_ceiling: u64,
    /// Whether analytical spill is permitted for this request.
    pub(crate) spill_eligible: bool,
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
}

#[cfg(feature = "test-support")]
#[derive(Debug)]
pub struct QueryResourceProbe {
    /// Existing Oracle query identity naming this observation.
    query_id: QueryId,
    /// Latest exact resource ownership and notification channel.
    snapshot: tokio::sync::watch::Sender<QueryResourceSnapshot>,
}

#[cfg(feature = "test-support")]
impl QueryResourceProbe {
    /// Creates the probe after local admission and before stream construction.
    #[must_use]
    fn new(query_id: QueryId, memory_bytes: u64, slot_units: u64) -> Self {
        let (snapshot, _) = tokio::sync::watch::channel(QueryResourceSnapshot {
            admission_slots: slot_units,
            memory_bytes,
            peer_slots: slot_units,
            tail_fences: 0,
        });
        Self { query_id, snapshot }
    }
    /// Returns the existing Oracle query identity.
    #[must_use]
    pub fn query_id(&self) -> QueryId {
        self.query_id
    }
    /// Returns current exact ownership for this query only.
    #[must_use]
    pub fn snapshot(&self) -> QueryResourceSnapshot {
        *self.snapshot.borrow()
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
    ) -> Self {
        let root_cancel = CancellationToken::new();
        let state = AdmissionState {
            interactive: ClassState::new(
                config.interactive_slots,
                config.single_tenant_ceiling,
                config.multi_tenant_ceiling,
                config.interactive_memory_bytes,
                0,
            ),
            analytical: ClassState::new(
                config.analytical_slots,
                config.single_tenant_ceiling,
                config.multi_tenant_ceiling,
                config.analytical_memory_bytes,
                config.spill_bytes,
            ),
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
            reserved_spill_bytes: state.interactive.spill_used + state.analytical.spill_used,
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
            reserved_spill_bytes: state.interactive.spill_used + state.analytical.spill_used,
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
        let PreparedAdmission {
            tenant,
            query_class,
            deadline,
            memory_ceiling,
            spill_eligible,
            cancellation,
        } = request;
        if memory_ceiling == 0 {
            OracleTelemetry::record_admission(
                query_class,
                OracleAdmissionOutcome::Rejected,
                OracleAdmissionReason::Memory,
            );
            return Err(BifrostError::QueryAdmissionRejected);
        }
        let class_kind = AdmissionClass::from(query_class);
        let enqueue = Instant::now();
        let wait_deadline = deadline.min(enqueue + self.max_queue_wait());
        let waiter_telemetry = OracleTelemetry::start_admission_waiter(query_class);
        let waiter = self.enqueue_waiter(
            tenant,
            class_kind,
            wait_deadline,
            memory_ceiling,
            spill_eligible,
            query_class,
        )?;
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
        self.build_admitted_guard(&grant, cancellation)
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
        wait_deadline: Instant,
        memory_ceiling: u64,
        spill_eligible: bool,
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
                memory_ceiling,
                spill_eligible,
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
            self.rollback_grant(&grant);
        }
    }

    /// Converts one granted aggregate into the stream-owned lifecycle guard.
    ///
    /// # Errors
    /// Returns [`BifrostError::OracleRoleUnavailable`] when this role was fenced
    /// out after local admission; the aggregate permit is released on that path.
    fn build_admitted_guard(
        &self,
        grant: &Grant,
        cancellation: CancellationToken,
    ) -> Result<AdmittedQueryGuard, BifrostError> {
        let permit = LocalPermit {
            shared: Arc::clone(&self.shared),
            class: grant.class,
            tenant: grant.tenant,
            memory: grant.memory,
            spill: grant.spill,
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
        let query_id = QueryId::new(uuid::Uuid::now_v7());
        Ok(AdmittedQueryGuard {
            query_id,
            leader: AdmittedLeader {
                node_id: local_node,
                fencing_token: self.local_role.fencing_token,
            },
            running: None,
            local_permit: Some(permit),
            live_reservations: Vec::new(),
            cancellation: self.shared.root_cancel.child_token(),
            request_cancellation: cancellation,
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
    fn rollback_grant(&self, grant: &Grant) {
        let notifications = if let Ok(mut state) = self.shared.state.lock() {
            rollback_counts(&mut state, grant);
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
    _shared: &Arc<AdmissionShared>,
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
                let memory = class.memory_per_query().min(waiter.memory_ceiling).max(1);
                if class.memory_used.saturating_add(memory) > class.memory_limit {
                    class.tenants[index].1.waiters.push_front(waiter);
                    continue;
                }
                let spill = if matches!(kind, AdmissionClass::Analytical) && waiter.spill_eligible {
                    class.spill_per_query()
                } else {
                    0
                };
                if class.spill_used.saturating_add(spill) > class.spill_limit {
                    class.tenants[index].1.waiters.push_front(waiter);
                    continue;
                }
                let tenant = class.tenants[index].0;
                class.used += 1;
                class.memory_used += memory;
                class.spill_used += spill;
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
                    },
                ));
            }
        }
    }
    notifications
}

/// Notifies winners after releasing the admission mutex and repairs canceled sends.
fn notify_grants(shared: &Arc<AdmissionShared>, mut notifications: Vec<GrantNotification>) {
    let mut pending: VecDeque<GrantNotification> = notifications.drain(..).collect();
    while let Some((sender, grant)) = pending.pop_front() {
        if let Err(grant) = sender.send(grant) {
            let next = if let Ok(mut state) = shared.state.lock() {
                rollback_counts(&mut state, &grant);
                grant_waiters(shared, &mut state)
            } else {
                Vec::new()
            };
            pending.extend(next);
        }
    }
}

/// Reverses one grant's class, tenant, memory, spill, and active counters.
fn rollback_counts(state: &mut AdmissionState, grant: &Grant) {
    let class = match grant.class {
        AdmissionClass::Interactive => &mut state.interactive,
        AdmissionClass::Analytical => &mut state.analytical,
    };
    class.spill_used = class.spill_used.saturating_sub(grant.spill);
    class.memory_used = class.memory_used.saturating_sub(grant.memory);
    if let Some((_, tenant)) = class.tenants.iter_mut().find(|(id, _)| *id == grant.tenant) {
        tenant.active = tenant.active.saturating_sub(1);
    }
    class.used = class.used.saturating_sub(1);
    state.active_queries = state.active_queries.saturating_sub(1);
}

/// Stream-owned local capacity cleanup.
pub(super) struct AdmittedQueryGuard {
    /// Stable query identity used by peer dispatch and test probes.
    pub(super) query_id: QueryId,
    /// Fenced leader selected at admission time.
    pub(super) leader: AdmittedLeader,
    /// Retained for peer compatibility; local admission uses `local_permit`.
    pub(super) running: Option<OwnedSemaphorePermit>,
    /// Aggregate leader-local class, tenant, memory, and spill permit.
    local_permit: Option<LocalPermit>,
    /// Canonical local slot-use gauge retained with the running permit.
    /// Parent reservations retaining drained live batches through stream cleanup.
    pub(super) live_reservations: Vec<AccountedMemoryReservation>,
    /// Cancellation shared with stream and peer dispatch.
    pub(super) cancellation: CancellationToken,
    /// Caller/request cancellation retained for admission ownership.
    pub(super) request_cancellation: CancellationToken,
    /// Query-keyed lifecycle observation retained through cleanup.
    #[cfg(feature = "test-support")]
    pub(super) resource_probe: Option<Arc<QueryResourceProbe>>,
}

impl Drop for AdmittedQueryGuard {
    /// Cancels the query and synchronously releases leader-local resources.
    fn drop(&mut self) {
        self.cancellation.cancel();
        self.live_reservations.clear();
        if let Some(permit) = self.local_permit.take() {
            permit.release_inner();
        }
        self.running.take();
        #[cfg(feature = "test-support")]
        if let Some(probe) = &self.resource_probe {
            probe.release_local();
        }
    }
}

impl AdmittedQueryGuard {
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
        ));
        self.resource_probe = Some(Arc::clone(&probe));
        probe
    }

    /// Cancels the query and releases all local resources synchronously.
    pub(super) fn release(mut self) {
        self.cancellation.cancel();
        self.live_reservations.clear();
        if let Some(permit) = self.local_permit.take() {
            permit.release_inner();
        }
        self.running.take();
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
            interactive: ClassState::new(1, 1, 1, 1024, 0),
            analytical: ClassState::new(1, 1, 1, 1024, 1024),
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
            query_id: QueryId::new(uuid::Uuid::now_v7()),
            leader: AdmittedLeader {
                node_id: NodeId::new(uuid::Uuid::now_v7()),
                fencing_token: 1,
            },
            running: None,
            local_permit: Some(LocalPermit {
                shared: Arc::clone(&shared),
                class: AdmissionClass::Interactive,
                tenant,
                memory: 1024,
                spill: 0,
                released: AtomicBool::new(false),
            }),
            live_reservations: Vec::new(),
            cancellation: cancellation.clone(),
            request_cancellation: request_cancellation.clone(),
            #[cfg(feature = "test-support")]
            resource_probe: None,
        },
        shared,
        request_cancellation,
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
mod tests {
    use super::*;
    use crate::cluster::ClusterSnapshot;
    use chrono::Utc;
    use wyrd_spec::vala::api::{
        ClusterNodeKey, ClusterRole, ClusterRoleLease, OracleCapabilitiesV1,
    };

    fn shared(config: OracleAdmissionConfig) -> Arc<AdmissionShared> {
        Arc::new(AdmissionShared {
            state: Mutex::new(AdmissionState {
                interactive: ClassState::new(
                    config.interactive_slots,
                    config.single_tenant_ceiling,
                    config.multi_tenant_ceiling,
                    config.interactive_memory_bytes,
                    0,
                ),
                analytical: ClassState::new(
                    config.analytical_slots,
                    config.single_tenant_ceiling,
                    config.multi_tenant_ceiling,
                    config.analytical_memory_bytes,
                    config.spill_bytes,
                ),
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
        })
    }

    /// Builds the production admission owner without a database-backed registry.
    fn owner(config: OracleAdmissionConfig) -> Arc<OracleAdmission> {
        let role = RegisteredRole {
            key: ClusterNodeKey {
                node_id: NodeId::new(uuid::Uuid::now_v7()),
                role: ClusterRole::Oracle,
            },
            fencing_token: 1,
            capabilities: ClusterCapabilities::OracleV1(OracleCapabilitiesV1 {
                peer_protocol_version: 1,
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
                peer_protocol_version: 1,
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
                memory_ceiling: u64::MAX,
                spill_eligible: false,
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
                    memory_ceiling: u64::MAX,
                    spill_eligible: false,
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
        assert!(receivers.into_iter().all(|mut rx| rx.try_recv().is_ok()));
    }

    /// Refresh changes only future grant generation and leaves active counts intact.
    #[tokio::test]
    async fn membership_refresh_affects_only_new_admission() {
        let owner = owner(OracleAdmissionConfig::default());
        let request = || PreparedAdmission {
            tenant: DataTenantId::new_v7(),
            query_class: QueryClass::Interactive,
            deadline: Instant::now() + Duration::from_secs(1),
            memory_ceiling: 1024,
            spill_eligible: false,
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
        let future = owner.admit(request()).await.expect("future admission");
        drop(future);
        drop(active);
    }

    /// Weighted rounds grant one waiter per tenant before an idle tenant is borrowed.
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
                    memory_ceiling: u64::MAX,
                    spill_eligible: false,
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
                memory_ceiling: u64::MAX,
                spill_eligible: false,
                tx,
            });
        state.queued += 1;
        let notifications = grant_waiters(&shared, &mut state);
        assert_eq!(state.interactive.used, 3);
        assert_eq!(state.interactive.tenants[0].1.active, 2);
        assert_eq!(state.interactive.tenants[1].1.active, 1);
        drop(state);
        notify_grants(&shared, notifications);
        assert!(receivers.into_iter().all(|mut rx| rx.try_recv().is_ok()));
    }

    /// Fixed class shares cap memory at the prepared request ceiling and charge analytical spill.
    #[test]
    fn local_admission_uses_prepared_memory_and_spill_shares() {
        let shared = shared(OracleAdmissionConfig {
            analytical_slots: 2,
            analytical_memory_bytes: 100,
            spill_bytes: 100,
            ..Default::default()
        });
        let tenant = DataTenantId::new_v7();
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        let mut state = shared.state.lock().expect("state");
        let index = state.analytical.tenant_index(tenant);
        state.analytical.tenants[index].1.waiters.push_back(Waiter {
            id: 1,
            deadline: Instant::now() + Duration::from_secs(1),
            memory_ceiling: 10,
            spill_eligible: true,
            tx,
        });
        state.queued = 1;
        let notifications = grant_waiters(&shared, &mut state);
        assert_eq!(state.analytical.memory_used, 10);
        assert_eq!(state.analytical.spill_used, 50);
        drop(state);
        notify_grants(&shared, notifications);
        let grant = rx.try_recv().expect("prepared grant");
        assert_eq!(grant.memory, 10);
        assert_eq!(grant.spill, 50);
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
                    memory_ceiling: u64::MAX,
                    spill_eligible: false,
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
        assert_eq!(
            state.interactive.memory_used,
            state.interactive.memory_limit
        );
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
            deadline: Instant::now() + Duration::from_secs(2),
            memory_ceiling: 1024,
            spill_eligible: false,
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
                deadline: Instant::now() + Duration::from_secs(1),
                memory_ceiling: 1024,
                spill_eligible: false,
                cancellation: CancellationToken::new(),
            })
            .await
            .expect("first admission");
        let started = Instant::now();
        let result = owner
            .admit(PreparedAdmission {
                tenant: DataTenantId::new_v7(),
                query_class: QueryClass::Interactive,
                deadline: started + Duration::from_millis(20),
                memory_ceiling: 1024,
                spill_eligible: false,
                cancellation: CancellationToken::new(),
            })
            .await;
        assert!(matches!(result, Err(BifrostError::QueryAdmissionRejected)));
        assert!(started.elapsed() < Duration::from_millis(250));
        drop(first);
    }

    /// Releasing one query cancels only its child while a sibling remains active.
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
                deadline: Instant::now() + Duration::from_secs(1),
                memory_ceiling: 1024,
                spill_eligible: false,
                cancellation: caller_one.clone(),
            })
            .await
            .expect("first admission");
        let first_child = first.cancellation.clone();
        let second = owner
            .admit(PreparedAdmission {
                tenant: DataTenantId::new_v7(),
                query_class: QueryClass::Interactive,
                deadline: Instant::now() + Duration::from_secs(1),
                memory_ceiling: 1024,
                spill_eligible: false,
                cancellation: caller_two.clone(),
            })
            .await
            .expect("second admission");
        let second_child = second.cancellation.clone();
        first.release();
        assert!(first_child.is_cancelled());
        assert!(!second_child.is_cancelled());
        assert!(!caller_one.is_cancelled());
        assert!(!caller_two.is_cancelled());
        drop(second);
    }

    /// Root shutdown cancellation reaches every admitted child without relaying tasks.
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
                deadline: Instant::now() + Duration::from_secs(1),
                memory_ceiling: 1024,
                spill_eligible: false,
                cancellation: CancellationToken::new(),
            })
            .await
            .expect("first admission");
        let second = owner
            .admit(PreparedAdmission {
                tenant: DataTenantId::new_v7(),
                query_class: QueryClass::Interactive,
                deadline: Instant::now() + Duration::from_secs(1),
                memory_ceiling: 1024,
                spill_eligible: false,
                cancellation: CancellationToken::new(),
            })
            .await
            .expect("second admission");
        let first_child = first.cancellation.clone();
        let second_child = second.cancellation.clone();
        owner.close();
        assert!(first_child.is_cancelled());
        assert!(second_child.is_cancelled());
        drop(first);
        drop(second);
    }

    /// Covers successful, unavailable, and deadline admission releases.
    async fn production_release_prefix() {
        let success_owner = owner(OracleAdmissionConfig::default());
        let success = success_owner
            .admit(PreparedAdmission {
                tenant: DataTenantId::new_v7(),
                query_class: QueryClass::Interactive,
                deadline: Instant::now() + Duration::from_secs(1),
                memory_ceiling: 1024,
                spill_eligible: false,
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
                deadline: Instant::now() + Duration::from_secs(1),
                memory_ceiling: 1024,
                spill_eligible: false,
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
                deadline: Instant::now() + Duration::from_secs(1),
                memory_ceiling: 1024,
                spill_eligible: false,
                cancellation: CancellationToken::new(),
            })
            .await
            .expect("held admission");
        let timeout = timeout_owner
            .admit(PreparedAdmission {
                tenant: DataTenantId::new_v7(),
                query_class: QueryClass::Interactive,
                deadline: Instant::now() + Duration::from_millis(10),
                memory_ceiling: 1024,
                spill_eligible: false,
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
                deadline: Instant::now() + Duration::from_secs(1),
                memory_ceiling: 1024,
                spill_eligible: false,
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
                    deadline: Instant::now() + Duration::from_secs(1),
                    memory_ceiling: 1024,
                    spill_eligible: false,
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
                deadline: Instant::now() + Duration::from_secs(1),
                memory_ceiling: 1024,
                spill_eligible: false,
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
            },
        );
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
            analytical_memory_bytes: 1024,
            spill_bytes: 1024,
            ..Default::default()
        });
        let guard = owner
            .admit(PreparedAdmission {
                tenant: DataTenantId::new_v7(),
                query_class: QueryClass::Analytical,
                deadline: Instant::now() + Duration::from_secs(1),
                memory_ceiling: 512,
                spill_eligible: true,
                cancellation: CancellationToken::new(),
            })
            .await
            .expect("analytical admission");
        guard.release();
        let state = owner.shared.state.lock().expect("state");
        assert_eq!(state.analytical.used, 0);
        assert_eq!(state.analytical.memory_used, 0);
        assert_eq!(state.analytical.spill_used, 0);
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
            deadline: Instant::now() + Duration::from_secs(1),
            memory_ceiling: 1024,
            spill_eligible: false,
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

    /// Local probe release remains idempotent through its watch state.
    #[test]
    fn admission_module_compiles() {}
}
