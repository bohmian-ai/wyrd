//! Role-fenced local ownership of delegated Oracle policy capacity.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PrincipalId;
use wyrd_spec::vala::api::{
    NodeId, OracleAdmissionContinuityLost, OracleAdmissionDemand, QueryClass,
};

use crate::cluster::{ROLE_HEARTBEAT_INTERVAL, ROLE_LIVENESS_CUTOFF};

/// Validated lifecycle settings for delegated policy capacity.
#[derive(Debug, Clone, Copy)]
pub struct DelegatedOracleAdmissionConfig {
    /// Exact units requested when a complete local grant is unavailable.
    pub allocation_units: u32,
    /// Background renewal and demand-notification cadence.
    pub renewal_interval: Duration,
    /// Maximum absolute validity requested for durable blocks.
    pub validity: Duration,
    /// Maximum bounded local waiter count.
    pub queue_capacity: usize,
}

impl Default for DelegatedOracleAdmissionConfig {
    fn default() -> Self {
        Self {
            allocation_units: 1,
            renewal_interval: ROLE_HEARTBEAT_INTERVAL,
            validity: ROLE_LIVENESS_CUTOFF,
            queue_capacity: 64,
        }
    }
}

impl DelegatedOracleAdmissionConfig {
    /// Validates allocation and role-liveness-derived timing bounds.
    ///
    /// # Errors
    ///
    /// Returns [`DelegatedOracleAdmissionError::InvalidConfig`] for zero values,
    /// renewal not shorter than validity, or validity beyond role liveness.
    pub fn validate(self) -> Result<Self, DelegatedOracleAdmissionError> {
        if self.allocation_units == 0
            || self.renewal_interval.is_zero()
            || self.validity.is_zero()
            || self.queue_capacity == 0
            || self.renewal_interval >= self.validity
            || self.validity > ROLE_LIVENESS_CUTOFF
        {
            return Err(DelegatedOracleAdmissionError::InvalidConfig);
        }
        Ok(self)
    }
}

/// One cached three-scope allocation issued to the local role incarnation.
#[derive(Debug, Clone)]
pub struct DelegatedAdmissionBlock {
    /// Durable allocation identity shared by all accounting levels.
    pub allocation_id: uuid::Uuid,
    /// Tenant charged by the allocation.
    pub tenant_id: DataTenantId,
    /// Principal charged by the allocation.
    pub principal_id: PrincipalId,
    /// Query class charged by the allocation.
    pub query_class: QueryClass,
    /// Physical holder node.
    pub holder_node_id: NodeId,
    /// Exact Oracle role-incarnation fence.
    pub holder_fencing_token: u64,
    /// Database-time validity start.
    pub valid_from: DateTime<Utc>,
    /// Absolute database-time expiry.
    pub expires_at: DateTime<Utc>,
    /// Total equal units available at every accounting level.
    pub units: u32,
    /// Locally consumed complete three-scope units.
    used: u32,
}

impl DelegatedAdmissionBlock {
    /// Collapses the three durable accounting rows into one complete local unit pool.
    ///
    /// # Errors
    ///
    /// Returns [`DelegatedOracleAdmissionError::ContinuityLost`] unless there
    /// are exactly global, tenant, and principal rows with identical allocation,
    /// holder, validity, class, tenant, and unit values.
    pub fn try_from_rows(
        rows: &[vala_sql::row_types::oracle_admission::OracleAdmissionBlockRow],
    ) -> Result<Self, DelegatedOracleAdmissionError> {
        let first = rows
            .first()
            .ok_or(DelegatedOracleAdmissionError::ContinuityLost)?;
        let scopes = rows
            .iter()
            .map(|row| row.scope_kind.as_str())
            .collect::<HashSet<_>>();
        if rows.len() != 3
            || scopes != HashSet::from(["global", "tenant", "principal"])
            || rows.iter().any(|row| {
                row.allocation_id != first.allocation_id
                    || row.data_tenant_id != first.data_tenant_id
                    || row.query_class != first.query_class
                    || row.units != first.units
                    || row.holder_node_id != first.holder_node_id
                    || row.holder_fencing_token != first.holder_fencing_token
                    || row.valid_from != first.valid_from
                    || row.expires_at != first.expires_at
                    || row.closed_at.is_some()
            })
        {
            return Err(DelegatedOracleAdmissionError::ContinuityLost);
        }
        let principal = rows
            .iter()
            .find(|row| row.scope_kind == "principal")
            .and_then(|row| row.principal_id)
            .ok_or(DelegatedOracleAdmissionError::ContinuityLost)?;
        Ok(Self {
            allocation_id: first.allocation_id,
            tenant_id: DataTenantId::new(first.data_tenant_id)
                .map_err(|_| DelegatedOracleAdmissionError::ContinuityLost)?,
            principal_id: PrincipalId::new(principal),
            query_class: match first.query_class.as_str() {
                "interactive" => QueryClass::Interactive,
                "analytical" => QueryClass::Analytical,
                _ => return Err(DelegatedOracleAdmissionError::ContinuityLost),
            },
            holder_node_id: NodeId::from(first.holder_node_id),
            holder_fencing_token: u64::try_from(first.holder_fencing_token)
                .map_err(|_| DelegatedOracleAdmissionError::ContinuityLost)?,
            valid_from: first.valid_from,
            expires_at: first.expires_at,
            units: u32::try_from(first.units)
                .map_err(|_| DelegatedOracleAdmissionError::ContinuityLost)?,
            used: 0,
        })
    }
}

/// A local acquisition request that never contains a database handle.
#[derive(Debug, Clone, Copy)]
pub struct DelegatedAdmissionRequest {
    /// Authenticated tenant identity.
    pub tenant_id: DataTenantId,
    /// Authenticated principal identity.
    pub principal_id: PrincipalId,
    /// Server-selected query class.
    pub query_class: QueryClass,
}

/// Local delegated admission failure.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum DelegatedOracleAdmissionError {
    /// Lifecycle configuration violates a locked timing or bound invariant.
    #[error("invalid delegated Oracle admission configuration")]
    InvalidConfig,
    /// The role fence has lost allocation or renewal continuity.
    #[error("delegated Oracle admission continuity is unavailable")]
    ContinuityLost,
    /// The bounded local fairness queue is full.
    #[error("delegated Oracle admission queue is full")]
    QueueFull,
    /// Internal state ownership was poisoned.
    #[error("delegated Oracle admission state is unavailable")]
    StateUnavailable,
}

/// One queued principal request retained in deterministic FIFO order.
struct DelegatedWaiter {
    /// Monotonic local identity used for cancellation cleanup.
    waiter_id: u64,
    /// Request whose three scopes must become available together.
    request: DelegatedAdmissionRequest,
    /// Completion notification.
    sender: oneshot::Sender<DelegatedOracleAdmissionGrant>,
}

/// Coalescing identity shared by local waiters and background allocation work.
type AdmissionDemandKey = (DataTenantId, PrincipalId, QueryClass);

/// Internal scheduling phase for one coalesced admission demand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AdmissionDemandPhase {
    /// The initial notification is still buffered in the worker channel.
    Queued,
    /// The worker owns one allocation attempt for this exact key.
    Allocating,
    /// An empty result made the key eligible for the next maintenance cadence.
    Retryable,
}

/// One durable allocation paired with its conservative local monotonic bound.
struct CachedDelegatedAdmissionBlock {
    /// Database-issued accounting and absolute validity evidence.
    block: DelegatedAdmissionBlock,
    /// Local deadline derived without trusting the pod wall clock.
    valid_until: Instant,
}

/// Mutable cached blocks, fairness queue, and lifecycle latches.
struct DelegatedState {
    /// Cached allocations issued to this exact role incarnation.
    blocks: Vec<CachedDelegatedAdmissionBlock>,
    /// Stable FIFO across tenant and principal request identities.
    waiters: VecDeque<DelegatedWaiter>,
    /// Coalesced background demand and its exact scheduling phase.
    demands: HashMap<AdmissionDemandKey, AdmissionDemandPhase>,
    /// Next monotonic waiter identity, never reused within this owner.
    next_waiter_id: u64,
    /// Whether new local grants remain valid.
    available: bool,
    /// Exactly-once continuity-loss notification latch.
    loss_notified: bool,
}

/// Concrete local owner of cached delegated policy capacity.
pub struct DelegatedOracleAdmission {
    /// Physical node identity bound to every accepted block.
    node_id: NodeId,
    /// Exact Oracle role-incarnation fence bound to every accepted block.
    fencing_token: u64,
    /// Validated local lifecycle settings.
    config: DelegatedOracleAdmissionConfig,
    /// Synchronous per-query state; it contains no SQL capability.
    state: Arc<Mutex<DelegatedState>>,
    /// Bounded background allocation demand channel.
    demand_tx: mpsc::Sender<OracleAdmissionDemand>,
    /// Typed cancellation handoff consumed by later query orchestration.
    loss_tx: mpsc::Sender<OracleAdmissionContinuityLost>,
}

impl DelegatedOracleAdmission {
    /// Constructs one local owner for an exact Oracle role incarnation.
    ///
    /// # Errors
    ///
    /// Returns [`DelegatedOracleAdmissionError::InvalidConfig`] when lifecycle
    /// settings violate allocation or role-liveness bounds.
    pub fn new(
        node_id: NodeId,
        fencing_token: u64,
        config: DelegatedOracleAdmissionConfig,
        demand_tx: mpsc::Sender<OracleAdmissionDemand>,
        loss_tx: mpsc::Sender<OracleAdmissionContinuityLost>,
    ) -> Result<Self, DelegatedOracleAdmissionError> {
        if fencing_token == 0 {
            return Err(DelegatedOracleAdmissionError::InvalidConfig);
        }
        Ok(Self {
            node_id,
            fencing_token,
            config: config.validate()?,
            state: Arc::new(Mutex::new(DelegatedState {
                blocks: Vec::new(),
                waiters: VecDeque::new(),
                demands: HashMap::new(),
                next_waiter_id: 1,
                available: true,
                loss_notified: false,
            })),
            demand_tx,
            loss_tx,
        })
    }

    /// Adds a database-issued block after exact holder and validity checks.
    ///
    /// # Errors
    ///
    /// Returns [`DelegatedOracleAdmissionError`] when the block belongs to a
    /// different incarnation, is already invalid, or state cannot be locked.
    pub fn install_block(
        &self,
        block: DelegatedAdmissionBlock,
        database_now: DateTime<Utc>,
        observed_before: Instant,
    ) -> Result<(), DelegatedOracleAdmissionError> {
        if block.holder_node_id != self.node_id
            || block.holder_fencing_token != self.fencing_token
            || block.units == 0
            || block.valid_from > database_now
            || block.expires_at <= database_now
        {
            return Err(DelegatedOracleAdmissionError::ContinuityLost);
        }
        let valid_until =
            conservative_local_deadline(database_now, block.expires_at, observed_before)?;
        if valid_until <= Instant::now() {
            return Err(DelegatedOracleAdmissionError::ContinuityLost);
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| DelegatedOracleAdmissionError::StateUnavailable)?;
        state
            .demands
            .remove(&(block.tenant_id, block.principal_id, block.query_class));
        state
            .blocks
            .push(CachedDelegatedAdmissionBlock { block, valid_until });
        Self::grant_waiters(&self.state, &mut state);
        self.queue_waiting_demands(&mut state)
    }

    /// Acquires one complete local unit or queues and notifies background demand.
    ///
    /// This operation touches only the synchronous cache and bounded channels;
    /// it has no `PostgreSQL` capability and cannot perform per-query SQL.
    ///
    /// # Errors
    ///
    /// Returns [`DelegatedOracleAdmissionError`] when continuity is closed, the
    /// queue is full, demand notification fails, or local state is unavailable.
    pub async fn acquire(
        self: &Arc<Self>,
        request: DelegatedAdmissionRequest,
    ) -> Result<DelegatedOracleAdmissionGrant, DelegatedOracleAdmissionError> {
        let (receiver, cleanup) = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| DelegatedOracleAdmissionError::StateUnavailable)?;
            if !state.available {
                return Err(DelegatedOracleAdmissionError::ContinuityLost);
            }
            if let Some(index) = available_block(&state.blocks, request, Instant::now()) {
                state.blocks[index].block.used += 1;
                return Ok(DelegatedOracleAdmissionGrant::new(self, index));
            }
            if state.waiters.len() >= self.config.queue_capacity {
                return Err(DelegatedOracleAdmissionError::QueueFull);
            }
            let (sender, receiver) = oneshot::channel();
            let waiter_id = state.next_waiter_id;
            state.next_waiter_id = state
                .next_waiter_id
                .checked_add(1)
                .ok_or(DelegatedOracleAdmissionError::StateUnavailable)?;
            state.waiters.push_back(DelegatedWaiter {
                waiter_id,
                request,
                sender,
            });
            if self.queue_demand(&mut state, request).is_err() {
                state.waiters.pop_back();
                return Err(DelegatedOracleAdmissionError::ContinuityLost);
            }
            (
                receiver,
                DelegatedWaiterCleanup::new(&self.state, waiter_id, Self::request_key(request)),
            )
        };
        let result = receiver
            .await
            .map_err(|_| DelegatedOracleAdmissionError::ContinuityLost);
        cleanup.disarm();
        result
    }

    /// Closes new grants and emits exactly one typed continuity-loss notification.
    pub fn close_continuity(&self) {
        let notify = self.state.lock().is_ok_and(|mut state| {
            state.available = false;
            state.waiters.clear();
            state.demands.clear();
            if state.loss_notified {
                false
            } else {
                state.loss_notified = true;
                true
            }
        });
        if notify {
            let _ = self.loss_tx.try_send(OracleAdmissionContinuityLost {
                holder_node_id: self.node_id,
                holder_fencing_token: self.fencing_token,
            });
        }
    }

    /// Stops local admission and settles all queued scheduling state at shutdown.
    ///
    /// Normal server shutdown does not emit a continuity-loss notification,
    /// but no waiter or demand phase may survive after its worker exits.
    fn stop(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.available = false;
            state.waiters.clear();
            state.demands.clear();
        }
    }

    /// Returns the coalescing key for one local request.
    fn request_key(request: DelegatedAdmissionRequest) -> AdmissionDemandKey {
        (request.tenant_id, request.principal_id, request.query_class)
    }

    /// Returns the coalescing key for one background notification.
    fn demand_key(demand: OracleAdmissionDemand) -> AdmissionDemandKey {
        (demand.tenant_id, demand.principal_id, demand.query_class)
    }

    /// Builds one role-fenced notification from a local request.
    fn demand_for(&self, request: DelegatedAdmissionRequest) -> OracleAdmissionDemand {
        OracleAdmissionDemand {
            tenant_id: request.tenant_id,
            principal_id: request.principal_id,
            query_class: request.query_class,
            requested_units: self.config.allocation_units,
            holder_node_id: self.node_id,
            holder_fencing_token: self.fencing_token,
        }
    }

    /// Enqueues one initial notification unless its key already has work.
    ///
    /// # Errors
    ///
    /// Returns [`DelegatedOracleAdmissionError::ContinuityLost`] when the
    /// bounded worker channel cannot accept the notification.
    fn queue_demand(
        &self,
        state: &mut DelegatedState,
        request: DelegatedAdmissionRequest,
    ) -> Result<(), DelegatedOracleAdmissionError> {
        let key = Self::request_key(request);
        if state.demands.contains_key(&key) {
            return Ok(());
        }
        state.demands.insert(key, AdmissionDemandPhase::Queued);
        if self.demand_tx.try_send(self.demand_for(request)).is_err() {
            state.demands.remove(&key);
            return Err(DelegatedOracleAdmissionError::ContinuityLost);
        }
        Ok(())
    }

    /// Enqueues exactly one notification for each unsatisfied waiter key.
    ///
    /// # Errors
    ///
    /// Returns [`DelegatedOracleAdmissionError::ContinuityLost`] when any
    /// residual waiter cannot notify the bounded worker channel.
    fn queue_waiting_demands(
        &self,
        state: &mut DelegatedState,
    ) -> Result<(), DelegatedOracleAdmissionError> {
        let mut projected = HashSet::new();
        let requests = state
            .waiters
            .iter()
            .map(|waiter| waiter.request)
            .filter(|request| projected.insert(Self::request_key(*request)))
            .collect::<Vec<_>>();
        for request in requests {
            self.queue_demand(state, request)?;
        }
        Ok(())
    }

    /// Returns allocation identities currently cached for background renewal.
    fn allocation_ids(&self) -> Result<Vec<uuid::Uuid>, DelegatedOracleAdmissionError> {
        self.state
            .lock()
            .map(|state| {
                state
                    .blocks
                    .iter()
                    .map(|cached| cached.block.allocation_id)
                    .collect()
            })
            .map_err(|_| DelegatedOracleAdmissionError::StateUnavailable)
    }

    /// Claims retry-eligible demand for one cadence-bounded allocation pass.
    ///
    /// Waiter order defines retry order, identical keys remain coalesced, and
    /// cancelled demands without a replacement waiter are omitted.
    ///
    /// # Errors
    ///
    /// Returns [`DelegatedOracleAdmissionError::StateUnavailable`] when local
    /// admission state cannot be inspected.
    fn begin_retryable_demands(
        &self,
    ) -> Result<Vec<OracleAdmissionDemand>, DelegatedOracleAdmissionError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| DelegatedOracleAdmissionError::StateUnavailable)?;
        let mut projected = HashSet::new();
        let requests = state
            .waiters
            .iter()
            .map(|waiter| waiter.request)
            .filter(|request| projected.insert(Self::request_key(*request)))
            .filter(|request| {
                state.demands.get(&Self::request_key(*request))
                    == Some(&AdmissionDemandPhase::Retryable)
            })
            .collect::<Vec<_>>();
        state.demands.retain(|key, phase| {
            *phase != AdmissionDemandPhase::Retryable || projected.contains(key)
        });
        for request in &requests {
            state.demands.insert(
                Self::request_key(*request),
                AdmissionDemandPhase::Allocating,
            );
        }
        Ok(requests
            .into_iter()
            .map(|request| self.demand_for(request))
            .collect())
    }

    /// Claims a buffered initial notification for one allocation attempt.
    ///
    /// Stale notifications whose waiters were already satisfied or cancelled
    /// are settled without issuing SQL.
    ///
    /// # Errors
    ///
    /// Returns [`DelegatedOracleAdmissionError::StateUnavailable`] when local
    /// scheduling state cannot be updated.
    fn begin_queued_allocation(
        &self,
        demand: OracleAdmissionDemand,
    ) -> Result<bool, DelegatedOracleAdmissionError> {
        let key = Self::demand_key(demand);
        let mut state = self
            .state
            .lock()
            .map_err(|_| DelegatedOracleAdmissionError::StateUnavailable)?;
        if state.demands.get(&key) != Some(&AdmissionDemandPhase::Queued) {
            return Ok(false);
        }
        if !state
            .waiters
            .iter()
            .any(|waiter| Self::request_key(waiter.request) == key)
        {
            state.demands.remove(&key);
            return Ok(false);
        }
        state.demands.insert(key, AdmissionDemandPhase::Allocating);
        Ok(true)
    }

    /// Settles an empty allocation attempt without starving a live waiter.
    ///
    /// A matching waiter retains the coalescing key for the next background
    /// cadence. When every waiter was cancelled after notification delivery,
    /// the consumed notification is forgotten so a future request can notify.
    ///
    /// # Errors
    ///
    /// Returns [`DelegatedOracleAdmissionError::StateUnavailable`] when local
    /// admission state cannot be updated.
    fn settle_empty_allocation(
        &self,
        demand: OracleAdmissionDemand,
    ) -> Result<(), DelegatedOracleAdmissionError> {
        let key = Self::demand_key(demand);
        let mut state = self
            .state
            .lock()
            .map_err(|_| DelegatedOracleAdmissionError::StateUnavailable)?;
        if state.demands.get(&key) != Some(&AdmissionDemandPhase::Allocating) {
            return Err(DelegatedOracleAdmissionError::ContinuityLost);
        }
        if state
            .waiters
            .iter()
            .any(|waiter| Self::request_key(waiter.request) == key)
        {
            state.demands.insert(key, AdmissionDemandPhase::Retryable);
        } else {
            state.demands.remove(&key);
        }
        Ok(())
    }

    /// Applies authoritative database-time renewal evidence to one cached block.
    ///
    /// The local expiry advances under the same state lock used by acquisition,
    /// while the consumed-unit count remains unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`DelegatedOracleAdmissionError::ContinuityLost`] when the
    /// renewed rows do not exactly match one cached allocation, holder fence,
    /// immutable accounting identity, or strictly later live expiry.
    fn apply_renewal(
        &self,
        renewal: &vala_sql::queries::oracle_admission::OracleAdmissionRenewal,
        observed_before: Instant,
    ) -> Result<(), DelegatedOracleAdmissionError> {
        let renewed = DelegatedAdmissionBlock::try_from_rows(&renewal.rows)?;
        if renewed.holder_node_id != self.node_id
            || renewed.holder_fencing_token != self.fencing_token
            || renewed.expires_at <= renewal.database_now
        {
            return Err(DelegatedOracleAdmissionError::ContinuityLost);
        }
        let valid_until =
            conservative_local_deadline(renewal.database_now, renewed.expires_at, observed_before)?;
        if valid_until <= Instant::now() {
            return Err(DelegatedOracleAdmissionError::ContinuityLost);
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| DelegatedOracleAdmissionError::StateUnavailable)?;
        let mut matching = state
            .blocks
            .iter_mut()
            .filter(|cached| cached.block.allocation_id == renewed.allocation_id);
        let current = matching
            .next()
            .ok_or(DelegatedOracleAdmissionError::ContinuityLost)?;
        if matching.next().is_some()
            || current.block.tenant_id != renewed.tenant_id
            || current.block.principal_id != renewed.principal_id
            || current.block.query_class != renewed.query_class
            || current.block.holder_node_id != renewed.holder_node_id
            || current.block.holder_fencing_token != renewed.holder_fencing_token
            || current.block.valid_from != renewed.valid_from
            || current.block.units != renewed.units
            || renewed.expires_at <= current.block.expires_at
            || valid_until <= current.valid_until
        {
            return Err(DelegatedOracleAdmissionError::ContinuityLost);
        }
        current.block.expires_at = renewed.expires_at;
        current.valid_until = valid_until;
        Ok(())
    }

    /// Grants queued requests in stable arrival order while complete units exist.
    fn grant_waiters(owner: &Arc<Mutex<DelegatedState>>, state: &mut DelegatedState) {
        let mut retained = VecDeque::new();
        while let Some(waiter) = state.waiters.pop_front() {
            if let Some(index) = available_block(&state.blocks, waiter.request, Instant::now()) {
                state.blocks[index].block.used += 1;
                let grant = DelegatedOracleAdmissionGrant {
                    owner: Arc::clone(owner),
                    block_index: index,
                    released: false,
                };
                if waiter.sender.send(grant).is_err() {
                    state.blocks[index].block.used =
                        state.blocks[index].block.used.saturating_sub(1);
                }
            } else {
                retained.push_back(waiter);
            }
        }
        state.waiters = retained;
    }
}

/// Cancellation guard that removes one queued waiter without duplicating demand.
struct DelegatedWaiterCleanup {
    /// Shared local state containing the queued waiter.
    owner: Arc<Mutex<DelegatedState>>,
    /// Exact waiter identity to remove without disturbing FIFO peers.
    waiter_id: u64,
    /// Coalesced demand identity used to settle retry-only work on cancellation.
    demand_key: AdmissionDemandKey,
    /// Whether successful completion already removed the waiter.
    armed: bool,
}

impl DelegatedWaiterCleanup {
    /// Arms cancellation cleanup for one newly queued waiter.
    fn new(
        owner: &Arc<Mutex<DelegatedState>>,
        waiter_id: u64,
        demand_key: AdmissionDemandKey,
    ) -> Self {
        Self {
            owner: Arc::clone(owner),
            waiter_id,
            demand_key,
            armed: true,
        }
    }

    /// Disarms cleanup after the waiter completes normally.
    fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for DelegatedWaiterCleanup {
    /// Reclaims queue capacity while retaining the already-enqueued demand.
    ///
    /// The worker channel cannot retract a sent notification. Keeping its
    /// coalescing key prevents a replacement waiter from enqueueing a duplicate
    /// allocation before the worker receives the original notification.
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if let Ok(mut state) = self.owner.lock() {
            state
                .waiters
                .retain(|waiter| waiter.waiter_id != self.waiter_id);
            let has_replacement = state.waiters.iter().any(|waiter| {
                DelegatedOracleAdmission::request_key(waiter.request) == self.demand_key
            });
            if !has_replacement
                && state.demands.get(&self.demand_key) == Some(&AdmissionDemandPhase::Retryable)
            {
                state.demands.remove(&self.demand_key);
            }
        }
    }
}

/// Background `PostgreSQL` owner for allocation demand and renewal continuity.
pub struct DelegatedOracleAdmissionWorker {
    /// Cross-tenant SQL capability absent from every per-query operation.
    pool: vala_sql::OperatorPool,
    /// Exact local role owner receiving validated blocks.
    admission: Arc<DelegatedOracleAdmission>,
    /// Coalesced bounded demand notifications from local acquire.
    demand_rx: mpsc::Receiver<OracleAdmissionDemand>,
    /// Server lifecycle cancellation.
    shutdown: CancellationToken,
}

impl DelegatedOracleAdmissionWorker {
    /// Constructs the sole background SQL collaborator for one admission owner.
    #[must_use]
    pub fn new(
        pool: vala_sql::OperatorPool,
        admission: Arc<DelegatedOracleAdmission>,
        demand_rx: mpsc::Receiver<OracleAdmissionDemand>,
        shutdown: CancellationToken,
    ) -> Self {
        Self {
            pool,
            admission,
            demand_rx,
            shutdown,
        }
    }

    /// Runs demand allocation and uninterrupted renewal until shutdown or loss.
    ///
    /// Database failure, late renewal, invalid returned rows, or channel closure
    /// closes new grants and emits the owner's exactly-once typed notification.
    /// Cancellation stops the worker without attempting an early capacity return;
    /// durable capacity remains non-reusable until expiry and role non-liveness.
    pub async fn run(mut self) {
        let mut renewal = tokio::time::interval(self.admission.config.renewal_interval);
        renewal.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            let result = tokio::select! {
                () = self.shutdown.cancelled() => {
                    self.admission.stop();
                    break;
                },
                demand = self.demand_rx.recv() => match demand {
                    Some(demand) => self.allocate_queued(demand).await,
                    None => Err(DelegatedOracleAdmissionError::ContinuityLost),
                },
                _ = renewal.tick() => self.maintain().await,
            };
            if let Err(error) = result {
                tracing::warn!(%error, "delegated Oracle admission continuity closed");
                self.admission.close_continuity();
                let blocks =
                    vala_sql::queries::oracle_admission::OracleAdmissionBlocks::new(&self.pool);
                let _ = blocks
                    .close_holder(
                        self.admission.node_id.as_uuid(),
                        self.admission.fencing_token,
                    )
                    .await;
                break;
            }
        }
    }

    /// Claims and allocates one notification received from the worker channel.
    ///
    /// A stale buffered notification is a successful no-op because its demand
    /// was already satisfied or cancelled before dequeue.
    ///
    /// # Errors
    ///
    /// Returns a continuity error when scheduling state or allocation fails.
    async fn allocate_queued(
        &self,
        demand: OracleAdmissionDemand,
    ) -> Result<(), DelegatedOracleAdmissionError> {
        if !self.admission.begin_queued_allocation(demand)? {
            return Ok(());
        }
        self.allocate(demand).await
    }

    /// Allocates and installs one exact demand in a short transaction.
    ///
    /// # Errors
    ///
    /// Returns a continuity error for SQL, malformed rows, or invalid holder data.
    async fn allocate(
        &self,
        demand: OracleAdmissionDemand,
    ) -> Result<(), DelegatedOracleAdmissionError> {
        let blocks = vala_sql::queries::oracle_admission::OracleAdmissionBlocks::new(&self.pool);
        let observed_before = Instant::now();
        let allocation = blocks
            .allocate(
                demand,
                chrono::Duration::from_std(self.admission.config.validity)
                    .map_err(|_| DelegatedOracleAdmissionError::InvalidConfig)?,
            )
            .await
            .map_err(|_| DelegatedOracleAdmissionError::ContinuityLost)?;
        if allocation.rows.is_empty() {
            tracing::debug!("delegated Oracle admission capacity unavailable or fragmented");
            return self.admission.settle_empty_allocation(demand);
        }
        let block = DelegatedAdmissionBlock::try_from_rows(&allocation.rows)?;
        self.admission
            .install_block(block, allocation.database_now, observed_before)
    }

    /// Renews live blocks and retries coalesced demand once per owner cadence.
    ///
    /// Empty allocations remain eligible for a later cadence without feeding
    /// the worker channel or creating a busy loop.
    ///
    /// # Errors
    ///
    /// Returns a continuity error when renewal, retry allocation, or local
    /// state inspection fails.
    async fn maintain(&self) -> Result<(), DelegatedOracleAdmissionError> {
        self.renew().await?;
        for demand in self.admission.begin_retryable_demands()? {
            self.allocate(demand).await?;
        }
        Ok(())
    }

    /// Renews every cached allocation while continuity remains uninterrupted.
    ///
    /// # Errors
    ///
    /// Returns a continuity error when any allocation is late or SQL is uncertain.
    async fn renew(&self) -> Result<(), DelegatedOracleAdmissionError> {
        let blocks = vala_sql::queries::oracle_admission::OracleAdmissionBlocks::new(&self.pool);
        let validity = chrono::Duration::from_std(self.admission.config.validity)
            .map_err(|_| DelegatedOracleAdmissionError::InvalidConfig)?;
        for allocation_id in self.admission.allocation_ids()? {
            let observed_before = Instant::now();
            let renewal = blocks
                .renew(
                    allocation_id,
                    self.admission.node_id.as_uuid(),
                    self.admission.fencing_token,
                    validity,
                )
                .await
                .map_err(|_| DelegatedOracleAdmissionError::ContinuityLost)?;
            let renewal = renewal.ok_or(DelegatedOracleAdmissionError::ContinuityLost)?;
            self.admission.apply_renewal(&renewal, observed_before)?;
        }
        Ok(())
    }
}

/// Finds one valid complete allocation for a request.
fn available_block(
    blocks: &[CachedDelegatedAdmissionBlock],
    request: DelegatedAdmissionRequest,
    now: Instant,
) -> Option<usize> {
    blocks.iter().position(|cached| {
        cached.block.tenant_id == request.tenant_id
            && cached.block.principal_id == request.principal_id
            && cached.block.query_class == request.query_class
            && cached.valid_until > now
            && cached.block.used < cached.block.units
    })
}

/// Maps a database-issued validity interval onto a conservative local deadline.
///
/// The local anchor is captured before the SQL request. Network and scheduling
/// delay therefore shorten usable capacity rather than extending database time.
///
/// # Errors
///
/// Returns [`DelegatedOracleAdmissionError::ContinuityLost`] when the database
/// interval is empty, cannot convert to monotonic duration, or overflows.
fn conservative_local_deadline(
    database_now: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    observed_before: Instant,
) -> Result<Instant, DelegatedOracleAdmissionError> {
    let remaining = (expires_at - database_now)
        .to_std()
        .map_err(|_| DelegatedOracleAdmissionError::ContinuityLost)?;
    observed_before
        .checked_add(remaining)
        .ok_or(DelegatedOracleAdmissionError::ContinuityLost)
}

/// RAII ownership of one locally consumed global/tenant/principal unit.
pub struct DelegatedOracleAdmissionGrant {
    /// Shared local state restored by release.
    owner: Arc<Mutex<DelegatedState>>,
    /// Cached allocation index charged atomically at grant time.
    block_index: usize,
    /// Exactly-once local release latch.
    released: bool,
}

impl DelegatedOracleAdmissionGrant {
    /// Builds a grant over one atomically charged cached allocation.
    fn new(owner: &Arc<DelegatedOracleAdmission>, block_index: usize) -> Self {
        Self {
            owner: Arc::clone(&owner.state),
            block_index,
            released: false,
        }
    }

    /// Restores all three accounting levels represented by this local unit.
    fn release_inner(&mut self) {
        if self.released {
            return;
        }
        self.released = true;
        if let Ok(mut state) = self.owner.lock()
            && let Some(cached) = state.blocks.get_mut(self.block_index)
        {
            cached.block.used = cached.block.used.saturating_sub(1);
            DelegatedOracleAdmission::grant_waiters(&self.owner, &mut state);
        }
    }
}

impl Drop for DelegatedOracleAdmissionGrant {
    /// Restores the cached complete unit exactly once.
    fn drop(&mut self) {
        self.release_inner();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vala_sql::row_types::oracle_admission::OracleAdmissionBlockRow;

    /// Constructs a valid local owner and its observable background channels.
    ///
    /// # Panics
    ///
    /// Panics when the locked default configuration is invalid.
    fn owner() -> (
        Arc<DelegatedOracleAdmission>,
        mpsc::Receiver<OracleAdmissionDemand>,
        mpsc::Receiver<OracleAdmissionContinuityLost>,
    ) {
        owner_with_queue_capacity(DelegatedOracleAdmissionConfig::default().queue_capacity)
    }

    /// Constructs a local owner with one exact test queue capacity.
    ///
    /// # Panics
    ///
    /// Panics when the requested queue capacity violates lifecycle invariants.
    fn owner_with_queue_capacity(
        queue_capacity: usize,
    ) -> (
        Arc<DelegatedOracleAdmission>,
        mpsc::Receiver<OracleAdmissionDemand>,
        mpsc::Receiver<OracleAdmissionContinuityLost>,
    ) {
        let (demand_tx, demand_rx) = mpsc::channel(8);
        let (loss_tx, loss_rx) = mpsc::channel(8);
        let owner = DelegatedOracleAdmission::new(
            NodeId::new(uuid::Uuid::from_u128(1)),
            3,
            DelegatedOracleAdmissionConfig {
                queue_capacity,
                ..DelegatedOracleAdmissionConfig::default()
            },
            demand_tx,
            loss_tx,
        )
        .expect("valid owner");
        (Arc::new(owner), demand_rx, loss_rx)
    }

    /// Projects one complete cached allocation into its three durable rows.
    ///
    /// # Panics
    ///
    /// Panics when test-only unit or fence values exceed their SQL projections.
    fn renewal_rows(
        block: &DelegatedAdmissionBlock,
        expires_at: DateTime<Utc>,
    ) -> Vec<OracleAdmissionBlockRow> {
        ["global", "tenant", "principal"]
            .into_iter()
            .map(|scope_kind| OracleAdmissionBlockRow {
                block_id: uuid::Uuid::now_v7(),
                allocation_id: block.allocation_id,
                scope_kind: scope_kind.to_owned(),
                data_tenant_id: block.tenant_id.into(),
                principal_id: (scope_kind == "principal").then_some(block.principal_id.as_uuid()),
                query_class: match block.query_class {
                    QueryClass::Interactive => "interactive",
                    QueryClass::Analytical => "analytical",
                }
                .to_owned(),
                units: i32::from(u16::try_from(block.units).expect("test block units fit u16")),
                holder_node_id: block.holder_node_id.as_uuid(),
                holder_fencing_token: i64::try_from(block.holder_fencing_token)
                    .expect("test holder fence fits i64"),
                valid_from: block.valid_from,
                expires_at,
                closed_at: None,
            })
            .collect()
    }

    /// Reads one exact internal demand phase for scheduling assertions.
    ///
    /// # Panics
    ///
    /// Panics when the test owner state cannot be locked.
    fn demand_phase(
        owner: &DelegatedOracleAdmission,
        request: DelegatedAdmissionRequest,
    ) -> Option<AdmissionDemandPhase> {
        owner
            .state
            .lock()
            .expect("test demand state")
            .demands
            .get(&DelegatedOracleAdmission::request_key(request))
            .copied()
    }

    /// Installs one live test allocation for an exact waiting demand.
    ///
    /// # Panics
    ///
    /// Panics when the block violates local holder or validity invariants.
    fn install_test_block(
        owner: &DelegatedOracleAdmission,
        request: DelegatedAdmissionRequest,
        demand: OracleAdmissionDemand,
        units: u32,
    ) {
        let now = Utc::now();
        owner
            .install_block(
                DelegatedAdmissionBlock {
                    allocation_id: uuid::Uuid::now_v7(),
                    tenant_id: request.tenant_id,
                    principal_id: request.principal_id,
                    query_class: request.query_class,
                    holder_node_id: demand.holder_node_id,
                    holder_fencing_token: demand.holder_fencing_token,
                    valid_from: now,
                    expires_at: now + chrono::Duration::seconds(10),
                    units,
                    used: 0,
                },
                now,
                Instant::now(),
            )
            .expect("test allocation installs");
    }

    /// Missing capacity coalesces identical background demand notifications.
    ///
    /// # Panics
    ///
    /// Panics when demand is not emitted exactly once.
    #[tokio::test]
    async fn acquire_coalesces_identical_demand() {
        let (owner, mut demand_rx, _) = owner();
        let request = DelegatedAdmissionRequest {
            tenant_id: DataTenantId::new_v7(),
            principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
            query_class: QueryClass::Interactive,
        };
        let first_owner = Arc::clone(&owner);
        let first = tokio::spawn(async move { first_owner.acquire(request).await });
        let second_owner = Arc::clone(&owner);
        let second = tokio::spawn(async move { second_owner.acquire(request).await });
        tokio::task::yield_now().await;
        assert_eq!(demand_rx.try_recv().expect("one demand").requested_units, 1);
        assert!(demand_rx.try_recv().is_err());
        first.abort();
        second.abort();
    }

    /// Dynamic tenant demand reaches only the background allocator channel.
    ///
    /// # Panics
    ///
    /// Panics when cached-only acquisition loses tenant identity, merges two
    /// tenants, or grants capacity before background allocation installs it.
    #[tokio::test]
    async fn background_allocator_uses_dynamic_tenant_default_without_request_sql() {
        let (owner, mut demand_rx, _) = owner();
        let principal_id = PrincipalId::new(uuid::Uuid::now_v7());
        let first_request = DelegatedAdmissionRequest {
            tenant_id: DataTenantId::new_v7(),
            principal_id,
            query_class: QueryClass::Interactive,
        };
        let second_request = DelegatedAdmissionRequest {
            tenant_id: DataTenantId::new_v7(),
            principal_id,
            query_class: QueryClass::Interactive,
        };
        let first_owner = Arc::clone(&owner);
        let first = tokio::spawn(async move { first_owner.acquire(first_request).await });
        let second_owner = Arc::clone(&owner);
        let second = tokio::spawn(async move { second_owner.acquire(second_request).await });
        tokio::task::yield_now().await;
        let demands = [
            demand_rx.try_recv().expect("first tenant demand"),
            demand_rx.try_recv().expect("second tenant demand"),
        ];
        assert!(
            demands
                .iter()
                .any(|demand| demand.tenant_id == first_request.tenant_id)
        );
        assert!(
            demands
                .iter()
                .any(|demand| demand.tenant_id == second_request.tenant_id)
        );
        assert!(demand_rx.try_recv().is_err());
        assert!(!first.is_finished());
        assert!(!second.is_finished());
        for (request, demand) in [first_request, second_request].into_iter().map(|request| {
            let demand = demands
                .iter()
                .copied()
                .find(|demand| demand.tenant_id == request.tenant_id)
                .expect("tenant demand is present");
            (request, demand)
        }) {
            assert!(
                owner
                    .begin_queued_allocation(demand)
                    .expect("background worker claims demand")
            );
            install_test_block(&owner, request, demand, 1);
        }
        drop(
            first
                .await
                .expect("first request joins")
                .expect("first grant"),
        );
        drop(
            second
                .await
                .expect("second request joins")
                .expect("second grant"),
        );
    }

    /// Empty allocation stays coalesced until a cadence retry returns capacity.
    ///
    /// # Panics
    ///
    /// Panics when empty settlement loses or duplicates demand, or later
    /// capacity cannot complete the original waiting request.
    #[tokio::test]
    async fn empty_allocation_retries_on_cadence_and_later_grants() {
        let (owner, mut demand_rx, _) = owner();
        let request = DelegatedAdmissionRequest {
            tenant_id: DataTenantId::new_v7(),
            principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
            query_class: QueryClass::Interactive,
        };
        let waiting_owner = Arc::clone(&owner);
        let waiting = tokio::spawn(async move { waiting_owner.acquire(request).await });
        tokio::task::yield_now().await;
        let demand = demand_rx.try_recv().expect("initial demand");
        assert!(
            owner
                .begin_queued_allocation(demand)
                .expect("initial allocation claims queued phase")
        );
        owner
            .settle_empty_allocation(demand)
            .expect("empty allocation stays retryable");
        assert_eq!(
            owner.begin_retryable_demands().expect("cadence demand"),
            [demand]
        );
        assert!(demand_rx.try_recv().is_err(), "retry does not use channel");

        install_test_block(&owner, request, demand, demand.requested_units);
        let grant = waiting.await.expect("waiter joins").expect("waiter grants");
        assert!(
            owner
                .begin_retryable_demands()
                .expect("settled demand")
                .is_empty()
        );
        drop(grant);
    }

    /// Worker cadence cannot claim a demand whose initial notice is buffered.
    ///
    /// # Panics
    ///
    /// Panics when cadence scheduling duplicates the queued allocation, loses
    /// the waiter, or installs more than one block for the demand.
    #[tokio::test]
    async fn worker_cadence_before_initial_dequeue_allocates_at_most_once() {
        let (owner, mut demand_rx, _) = owner();
        let request = DelegatedAdmissionRequest {
            tenant_id: DataTenantId::new_v7(),
            principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
            query_class: QueryClass::Interactive,
        };
        let waiting_owner = Arc::clone(&owner);
        let waiting = tokio::spawn(async move { waiting_owner.acquire(request).await });
        tokio::task::yield_now().await;

        assert_eq!(
            demand_phase(&owner, request),
            Some(AdmissionDemandPhase::Queued)
        );
        assert!(
            owner
                .begin_retryable_demands()
                .expect("cadence inspection")
                .is_empty(),
            "buffered initial notification is not cadence eligible"
        );
        let demand = demand_rx.try_recv().expect("buffered initial notification");
        assert!(
            owner
                .begin_queued_allocation(demand)
                .expect("worker dequeues initial notification")
        );
        install_test_block(&owner, request, demand, 1);
        let grant = waiting.await.expect("waiter joins").expect("waiter grants");
        assert!(
            !owner
                .begin_queued_allocation(demand)
                .expect("stale dequeue")
        );
        let state = owner.state.lock().expect("settled scheduling state");
        assert_eq!(state.blocks.len(), 1);
        assert!(state.demands.is_empty());
        drop(state);
        drop(grant);
    }

    /// Last-waiter cancellation settles retry-only demand and permits reuse.
    ///
    /// # Panics
    ///
    /// Panics when cancellation strands retry state or a replacement cannot
    /// enqueue one fresh notification.
    #[tokio::test]
    async fn retryable_last_waiter_cancellation_reopens_notification_slot() {
        let (owner, mut demand_rx, _) = owner();
        let request = DelegatedAdmissionRequest {
            tenant_id: DataTenantId::new_v7(),
            principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
            query_class: QueryClass::Analytical,
        };
        let cancelled_owner = Arc::clone(&owner);
        let cancelled = tokio::spawn(async move { cancelled_owner.acquire(request).await });
        tokio::task::yield_now().await;
        let demand = demand_rx.try_recv().expect("initial notification");
        assert!(
            owner
                .begin_queued_allocation(demand)
                .expect("initial claim")
        );
        owner
            .settle_empty_allocation(demand)
            .expect("empty allocation becomes retryable");
        assert_eq!(
            demand_phase(&owner, request),
            Some(AdmissionDemandPhase::Retryable)
        );
        cancelled.abort();
        let cancelled_result = cancelled.await;
        assert!(matches!(cancelled_result, Err(error) if error.is_cancelled()));
        assert_eq!(demand_phase(&owner, request), None);

        let replacement_owner = Arc::clone(&owner);
        let replacement = tokio::spawn(async move { replacement_owner.acquire(request).await });
        tokio::task::yield_now().await;
        assert_eq!(
            demand_rx.try_recv().expect("replacement notification"),
            demand
        );
        assert!(demand_rx.try_recv().is_err());
        replacement.abort();
    }

    /// Continuity loss and shutdown settle every demand phase without reuse.
    ///
    /// # Panics
    ///
    /// Panics when either terminal transition retains a waiter or demand, or
    /// permits a later local acquisition.
    #[tokio::test]
    async fn terminal_transitions_clear_all_demand_phases() {
        for shutdown in [false, true] {
            let (owner, _demand_rx, _) = owner();
            let request = DelegatedAdmissionRequest {
                tenant_id: DataTenantId::new_v7(),
                principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
                query_class: QueryClass::Interactive,
            };
            let waiting_owner = Arc::clone(&owner);
            let waiting = tokio::spawn(async move { waiting_owner.acquire(request).await });
            tokio::task::yield_now().await;
            if shutdown {
                owner.stop();
            } else {
                owner.close_continuity();
            }
            {
                let state = owner.state.lock().expect("terminal scheduling state");
                assert!(state.waiters.is_empty());
                assert!(state.demands.is_empty());
                assert!(!state.available);
            }
            assert!(matches!(
                owner.acquire(request).await,
                Err(DelegatedOracleAdmissionError::ContinuityLost)
            ));
            assert!(matches!(
                waiting.await.expect("terminal waiter joins"),
                Err(DelegatedOracleAdmissionError::ContinuityLost)
            ));
        }
    }

    /// Cancellation reclaims queue capacity without duplicating queued demand.
    ///
    /// # Panics
    ///
    /// Panics when cancellation leaves its waiter behind, forgets the original
    /// notification, duplicates demand, or changes replacement FIFO order.
    #[tokio::test]
    async fn cancelled_waiter_preserves_enqueued_demand_and_fifo_queue_reuse() {
        let (owner, mut demand_rx, _) = owner_with_queue_capacity(2);
        let request = DelegatedAdmissionRequest {
            tenant_id: DataTenantId::new_v7(),
            principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
            query_class: QueryClass::Interactive,
        };
        let cancelled_owner = Arc::clone(&owner);
        let cancelled = tokio::spawn(async move { cancelled_owner.acquire(request).await });
        tokio::task::yield_now().await;
        assert_eq!(demand_rx.len(), 1, "worker has not received first demand");
        cancelled.abort();
        let cancelled_result = cancelled.await;
        assert!(matches!(cancelled_result, Err(error) if error.is_cancelled()));
        {
            let state = owner.state.lock().expect("state after cancellation");
            assert!(state.waiters.is_empty());
            assert_eq!(state.demands.len(), 1);
            assert_eq!(
                state.demands.values().copied().collect::<Vec<_>>(),
                vec![AdmissionDemandPhase::Queued]
            );
        }

        let first_owner = Arc::clone(&owner);
        let first = tokio::spawn(async move { first_owner.acquire(request).await });
        tokio::task::yield_now().await;
        let second_owner = Arc::clone(&owner);
        let second = tokio::spawn(async move { second_owner.acquire(request).await });
        tokio::task::yield_now().await;
        {
            let state = owner.state.lock().expect("replacement state");
            assert_eq!(
                state
                    .waiters
                    .iter()
                    .map(|waiter| waiter.waiter_id)
                    .collect::<Vec<_>>(),
                vec![2, 3]
            );
            assert_eq!(state.demands.len(), 1);
        }
        assert_eq!(demand_rx.len(), 1, "replacement must reuse original demand");
        let original_demand = demand_rx.try_recv().expect("original demand remains");
        assert_eq!(original_demand.tenant_id, request.tenant_id);
        assert!(
            owner
                .begin_queued_allocation(original_demand)
                .expect("worker claims original notification")
        );
        assert!(
            demand_rx.try_recv().is_err(),
            "no duplicate demand is queued"
        );
        assert!(matches!(
            owner.acquire(request).await,
            Err(DelegatedOracleAdmissionError::QueueFull)
        ));
        install_test_block(&owner, request, original_demand, 1);
        let first_grant = first
            .await
            .expect("first replacement joins")
            .expect("first grant");
        tokio::task::yield_now().await;
        assert!(
            !second.is_finished(),
            "second replacement remains FIFO queued"
        );
        assert_eq!(
            owner
                .state
                .lock()
                .expect("state after first grant")
                .waiters
                .front()
                .expect("second waiter remains")
                .waiter_id,
            3
        );
        drop(first_grant);
        let second_grant = second
            .await
            .expect("second replacement joins")
            .expect("second grant");
        let residual_demand = demand_rx.try_recv().expect("residual FIFO demand");
        assert!(
            !owner
                .begin_queued_allocation(residual_demand)
                .expect("satisfied residual notification is stale")
        );
        let state = owner.state.lock().expect("completed replacement state");
        assert!(state.waiters.is_empty());
        assert!(state.demands.is_empty());
        drop(state);
        drop(second_grant);
    }

    /// Cached grants consume and restore complete local units without SQL.
    ///
    /// # Panics
    ///
    /// Panics when block installation, acquisition, or exact release fails.
    #[tokio::test]
    async fn cached_grant_releases_atomically() {
        let (owner, _, _) = owner();
        let now = Utc::now();
        let request = DelegatedAdmissionRequest {
            tenant_id: DataTenantId::new_v7(),
            principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
            query_class: QueryClass::Interactive,
        };
        owner
            .install_block(
                DelegatedAdmissionBlock {
                    allocation_id: uuid::Uuid::now_v7(),
                    tenant_id: request.tenant_id,
                    principal_id: request.principal_id,
                    query_class: request.query_class,
                    holder_node_id: NodeId::new(uuid::Uuid::from_u128(1)),
                    holder_fencing_token: 3,
                    valid_from: now,
                    expires_at: now + chrono::Duration::seconds(10),
                    units: 1,
                    used: 0,
                },
                now,
                Instant::now(),
            )
            .expect("matching block");
        let grant = owner.acquire(request).await.expect("cached grant");
        assert_eq!(owner.state.lock().expect("state").blocks[0].block.used, 1);
        drop(grant);
        assert_eq!(owner.state.lock().expect("state").blocks[0].block.used, 0);
    }

    /// Database wall-clock skew cannot extend or prematurely reject local use.
    ///
    /// # Panics
    ///
    /// Panics when local validity follows the pod wall clock instead of the
    /// authoritative database interval mapped onto monotonic time.
    #[test]
    fn database_time_skew_uses_conservative_monotonic_validity() {
        for database_now in [
            DateTime::parse_from_rfc3339("2000-01-01T00:00:00Z")
                .expect("past database time")
                .to_utc(),
            DateTime::parse_from_rfc3339("2100-01-01T00:00:00Z")
                .expect("future database time")
                .to_utc(),
        ] {
            let (owner, _, _) = owner();
            let request = DelegatedAdmissionRequest {
                tenant_id: DataTenantId::new_v7(),
                principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
                query_class: QueryClass::Interactive,
            };
            let observed_before = Instant::now();
            owner
                .install_block(
                    DelegatedAdmissionBlock {
                        allocation_id: uuid::Uuid::now_v7(),
                        tenant_id: request.tenant_id,
                        principal_id: request.principal_id,
                        query_class: request.query_class,
                        holder_node_id: NodeId::new(uuid::Uuid::from_u128(1)),
                        holder_fencing_token: 3,
                        valid_from: database_now,
                        expires_at: database_now + chrono::Duration::milliseconds(40),
                        units: 1,
                        used: 0,
                    },
                    database_now,
                    observed_before,
                )
                .expect("skewed database interval installs");
            let state = owner.state.lock().expect("skewed state");
            assert_eq!(
                available_block(&state.blocks, request, observed_before),
                Some(0)
            );
            drop(state);
            std::thread::sleep(Duration::from_millis(50));
            let state = owner.state.lock().expect("expired skewed state");
            assert_eq!(
                available_block(&state.blocks, request, Instant::now()),
                None
            );
        }
    }

    /// Authoritative renewal advances cached expiry without resetting usage.
    ///
    /// # Panics
    ///
    /// Panics when renewal evidence fails or changes local consumption.
    #[tokio::test]
    async fn renewal_advances_cached_expiry_and_preserves_usage() {
        let (owner, _, _) = owner();
        let now = Utc::now();
        let block = DelegatedAdmissionBlock {
            allocation_id: uuid::Uuid::now_v7(),
            tenant_id: DataTenantId::new_v7(),
            principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
            query_class: QueryClass::Interactive,
            holder_node_id: NodeId::new(uuid::Uuid::from_u128(1)),
            holder_fencing_token: 3,
            valid_from: now,
            expires_at: now + chrono::Duration::seconds(2),
            units: 1,
            used: 0,
        };
        owner
            .install_block(block.clone(), now, Instant::now())
            .expect("initial block installs");
        let request = DelegatedAdmissionRequest {
            tenant_id: block.tenant_id,
            principal_id: block.principal_id,
            query_class: block.query_class,
        };
        let grant = owner.acquire(request).await.expect("initial grant");
        let renewed_expiry = now + chrono::Duration::seconds(10);
        let renewal = vala_sql::queries::oracle_admission::OracleAdmissionRenewal {
            rows: renewal_rows(&block, renewed_expiry),
            database_now: now + chrono::Duration::seconds(1),
        };
        owner
            .apply_renewal(&renewal, Instant::now())
            .expect("renewal applies");
        let state = owner.state.lock().expect("renewed state");
        assert_eq!(state.blocks[0].block.expires_at, renewed_expiry);
        assert_eq!(state.blocks[0].block.used, 1);
        drop(state);
        drop(grant);
    }

    /// Mismatched renewal evidence fails closed and notifies exactly once.
    ///
    /// # Panics
    ///
    /// Panics when mismatched evidence applies or continuity notification duplicates.
    #[test]
    fn mismatched_renewal_closes_continuity_once() {
        let (owner, _, mut loss_rx) = owner();
        let now = Utc::now();
        let block = DelegatedAdmissionBlock {
            allocation_id: uuid::Uuid::now_v7(),
            tenant_id: DataTenantId::new_v7(),
            principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
            query_class: QueryClass::Interactive,
            holder_node_id: NodeId::new(uuid::Uuid::from_u128(1)),
            holder_fencing_token: 3,
            valid_from: now,
            expires_at: now + chrono::Duration::seconds(2),
            units: 1,
            used: 0,
        };
        owner
            .install_block(block.clone(), now, Instant::now())
            .expect("initial block installs");
        let mut rows = renewal_rows(&block, now + chrono::Duration::seconds(10));
        rows[0].holder_fencing_token = 4;
        let renewal = vala_sql::queries::oracle_admission::OracleAdmissionRenewal {
            rows,
            database_now: now + chrono::Duration::seconds(1),
        };
        assert!(owner.apply_renewal(&renewal, Instant::now()).is_err());
        owner.close_continuity();
        owner.close_continuity();
        assert_eq!(
            loss_rx.try_recv().expect("one loss").holder_fencing_token,
            3
        );
        assert!(loss_rx.try_recv().is_err());
    }

    /// Repeated continuity loss emits exactly one typed notification.
    ///
    /// # Panics
    ///
    /// Panics when notification delivery is absent or duplicated.
    #[test]
    fn continuity_loss_notifies_once() {
        let (owner, _, mut loss_rx) = owner();
        owner.close_continuity();
        owner.close_continuity();
        assert_eq!(
            loss_rx.try_recv().expect("one loss").holder_fencing_token,
            3
        );
        assert!(loss_rx.try_recv().is_err());
    }
}
