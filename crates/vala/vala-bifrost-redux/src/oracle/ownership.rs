//! Role-fenced local ownership of delegated Oracle policy capacity.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use thiserror::Error;
use tokio::sync::mpsc;
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
    /// Maximum units this node may grant ahead of durable capacity.
    ///
    /// Bounds how far local admission may run in front of the background refill
    /// while a `PostgreSQL` allocation is outstanding. Exhausting it refuses the
    /// query immediately rather than parking it until the next refill lands.
    pub queue_capacity: usize,
}

impl Default for DelegatedOracleAdmissionConfig {
    fn default() -> Self {
        Self {
            allocation_units: DEFAULT_DELEGATED_ALLOCATION_UNITS,
            renewal_interval: ROLE_HEARTBEAT_INTERVAL,
            validity: ROLE_LIVENESS_CUTOFF,
            queue_capacity: 64,
        }
    }
}

/// Units requested per durable allocation round trip.
///
/// Each `PostgreSQL` allocation carries a fixed round-trip cost regardless of
/// how many units it returns, so a single-unit block forces one round trip per
/// query and cannot keep pace with concurrent readers. Requesting a batch
/// amortizes that cost across many admissions.
pub const DEFAULT_DELEGATED_ALLOCATION_UNITS: u32 = 32;

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

/// Coalescing identity shared by local demand and background allocation work.
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
    /// Whether background maintenance owns durable retirement of this idle block.
    retiring: bool,
}

/// Mutable cached blocks, overdraft accounting, and lifecycle latches.
struct DelegatedState {
    /// Cached allocations issued to this exact role incarnation.
    blocks: Vec<CachedDelegatedAdmissionBlock>,
    /// Coalesced background demand and its exact scheduling phase.
    demands: HashMap<AdmissionDemandKey, AdmissionDemandPhase>,
    /// Units granted ahead of durable capacity while a refill is outstanding.
    ///
    /// Durable capacity is refilled by a background worker that must perform a
    /// `PostgreSQL` round trip. Blocking a read on that round trip puts database
    /// latency — and, when capacity is momentarily fragmented, a full renewal
    /// cadence — directly on the query hot path. Overdraft lets a query proceed
    /// against the local per-tenant ceilings while the refill lands, and bounds
    /// how far ahead of durable capacity this node may run.
    overdraft_used: usize,
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
                demands: HashMap::new(),
                overdraft_used: 0,
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
        state.blocks.push(CachedDelegatedAdmissionBlock {
            block,
            valid_until,
            retiring: false,
        });
        Ok(())
    }

    /// Charges one local unit against cached capacity or bounded overdraft.
    ///
    /// This is on the query hot path, so it never waits. A cache hit charges a
    /// durable unit directly. A miss notifies the background worker to refill and
    /// grants against overdraft so the query proceeds under the local per-tenant
    /// ceilings instead of parking until a `PostgreSQL` round trip lands.
    /// Exhausted overdraft refuses immediately; a caller that must wait is better
    /// served by a fast refusal it can retry than by an unbounded stall.
    ///
    /// This operation touches only the synchronous cache and bounded channels; it
    /// has no `PostgreSQL` capability and cannot perform per-query SQL.
    ///
    /// # Errors
    ///
    /// Returns [`DelegatedOracleAdmissionError::ContinuityLost`] when continuity
    /// is closed, [`DelegatedOracleAdmissionError::QueueFull`] when overdraft is
    /// exhausted, and [`DelegatedOracleAdmissionError::StateUnavailable`] when
    /// local state cannot be locked.
    pub fn acquire(
        self: &Arc<Self>,
        request: DelegatedAdmissionRequest,
    ) -> Result<DelegatedOracleAdmissionGrant, DelegatedOracleAdmissionError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| DelegatedOracleAdmissionError::StateUnavailable)?;
        if !state.available {
            return Err(DelegatedOracleAdmissionError::ContinuityLost);
        }
        if let Some(index) = available_block(&state.blocks, request, Instant::now()) {
            state.blocks[index].block.used += 1;
            let allocation_id = state.blocks[index].block.allocation_id;
            return Ok(DelegatedOracleAdmissionGrant::new(
                self,
                Some(allocation_id),
            ));
        }
        if state.overdraft_used >= self.config.queue_capacity {
            return Err(DelegatedOracleAdmissionError::QueueFull);
        }
        // A failed notification only means the refill is not scheduled by this
        // caller; it must not deny a query that overdraft can still cover.
        let _ = self.queue_demand(&mut state, request);
        state.overdraft_used = state.overdraft_used.saturating_add(1);
        Ok(DelegatedOracleAdmissionGrant::new(self, None))
    }

    /// Closes new grants and emits exactly one typed continuity-loss notification.
    pub fn close_continuity(&self) {
        let notify = self.state.lock().is_ok_and(|mut state| {
            state.available = false;
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
    /// but no demand phase may survive after its worker exits.
    fn stop(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.available = false;
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

    /// Claims every idle allocation for fenced background retirement.
    fn begin_idle_retirements(&self) -> Result<Vec<uuid::Uuid>, DelegatedOracleAdmissionError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| DelegatedOracleAdmissionError::StateUnavailable)?;
        let mut allocations = Vec::new();
        for cached in &mut state.blocks {
            if cached.block.used == 0 && !cached.retiring {
                cached.retiring = true;
                allocations.push(cached.block.allocation_id);
            }
        }
        Ok(allocations)
    }

    /// Removes one exact allocation after its durable rows close successfully.
    fn finish_idle_retirement(
        &self,
        allocation_id: uuid::Uuid,
    ) -> Result<(), DelegatedOracleAdmissionError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| DelegatedOracleAdmissionError::StateUnavailable)?;
        let index = state
            .blocks
            .iter()
            .position(|cached| cached.block.allocation_id == allocation_id)
            .ok_or(DelegatedOracleAdmissionError::ContinuityLost)?;
        if !state.blocks[index].retiring || state.blocks[index].block.used != 0 {
            return Err(DelegatedOracleAdmissionError::ContinuityLost);
        }
        state.blocks.remove(index);
        Ok(())
    }

    /// Returns non-retiring allocation identities cached for background renewal.
    fn renewal_allocation_ids(&self) -> Result<Vec<uuid::Uuid>, DelegatedOracleAdmissionError> {
        self.state
            .lock()
            .map(|state| {
                state
                    .blocks
                    .iter()
                    .filter(|cached| !cached.retiring)
                    .map(|cached| cached.block.allocation_id)
                    .collect()
            })
            .map_err(|_| DelegatedOracleAdmissionError::StateUnavailable)
    }

    /// Claims every retry-eligible demand for one cadence-bounded allocation pass.
    ///
    /// Demand is coalesced by key, so one pass issues at most one allocation per
    /// (tenant, principal, class). A key stays eligible until an allocation
    /// installs capacity for it, which is what lets overdraft drain back to
    /// durable units without any query waiting on the round trip.
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
        let keys = state
            .demands
            .iter()
            .filter(|(_, phase)| **phase == AdmissionDemandPhase::Retryable)
            .map(|(key, _)| *key)
            .collect::<Vec<_>>();
        for key in &keys {
            state.demands.insert(*key, AdmissionDemandPhase::Allocating);
        }
        Ok(keys
            .into_iter()
            .map(
                |(tenant_id, principal_id, query_class)| OracleAdmissionDemand {
                    tenant_id,
                    principal_id,
                    query_class,
                    requested_units: self.config.allocation_units,
                    holder_node_id: self.node_id,
                    holder_fencing_token: self.fencing_token,
                },
            )
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
        state.demands.insert(key, AdmissionDemandPhase::Allocating);
        Ok(true)
    }

    /// Settles an empty allocation attempt so the next cadence retries it.
    ///
    /// Durable capacity may be momentarily fragmented. The key stays eligible
    /// so the background worker keeps trying; queries continue against overdraft
    /// in the meantime rather than parking on this retry.
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
        state.demands.insert(key, AdmissionDemandPhase::Retryable);
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
        self.retire_idle().await?;
        self.renew().await?;
        for demand in self.admission.begin_retryable_demands()? {
            self.allocate(demand).await?;
        }
        Ok(())
    }

    /// Closes and removes every locally idle allocation before renewing active work.
    async fn retire_idle(&self) -> Result<(), DelegatedOracleAdmissionError> {
        let blocks = vala_sql::queries::oracle_admission::OracleAdmissionBlocks::new(&self.pool);
        for allocation_id in self.admission.begin_idle_retirements()? {
            let closed = blocks
                .close_allocation(
                    allocation_id,
                    self.admission.node_id.as_uuid(),
                    self.admission.fencing_token,
                )
                .await
                .map_err(|_| DelegatedOracleAdmissionError::ContinuityLost)?;
            if closed != 3 {
                return Err(DelegatedOracleAdmissionError::ContinuityLost);
            }
            self.admission.finish_idle_retirement(allocation_id)?;
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
        for allocation_id in self.admission.renewal_allocation_ids()? {
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
        !cached.retiring
            && cached.block.tenant_id == request.tenant_id
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
    /// Cached allocation charged at grant time, or `None` for an overdraft unit.
    allocation_id: Option<uuid::Uuid>,
    /// Exactly-once local release latch.
    released: bool,
}

impl DelegatedOracleAdmissionGrant {
    /// Builds a grant over one charged cached allocation or overdraft unit.
    ///
    /// `allocation_id` is `None` when the unit was granted ahead of durable
    /// capacity; release then returns overdraft rather than a cached block.
    fn new(owner: &Arc<DelegatedOracleAdmission>, allocation_id: Option<uuid::Uuid>) -> Self {
        Self {
            owner: Arc::clone(&owner.state),
            allocation_id,
            released: false,
        }
    }

    /// Restores the exact accounting level this local unit was charged against.
    fn release_inner(&mut self) {
        if self.released {
            return;
        }
        self.released = true;
        let Ok(mut state) = self.owner.lock() else {
            return;
        };
        let Some(allocation_id) = self.allocation_id else {
            state.overdraft_used = state.overdraft_used.saturating_sub(1);
            return;
        };
        if let Some(cached) = state
            .blocks
            .iter_mut()
            .find(|cached| cached.block.allocation_id == allocation_id)
        {
            cached.block.used = cached.block.used.saturating_sub(1);
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
    #[test]
    fn acquire_coalesces_identical_demand() {
        let (owner, mut demand_rx, _) = owner();
        let request = DelegatedAdmissionRequest {
            tenant_id: DataTenantId::new_v7(),
            principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
            query_class: QueryClass::Interactive,
        };
        let first = owner.acquire(request).expect("first overdraft grant");
        let second = owner.acquire(request).expect("second overdraft grant");
        assert_eq!(
            demand_rx.try_recv().expect("one demand").requested_units,
            DEFAULT_DELEGATED_ALLOCATION_UNITS
        );
        assert!(demand_rx.try_recv().is_err());
        drop(first);
        drop(second);
    }

    /// A cache miss grants immediately instead of waiting for durable capacity.
    ///
    /// Blocking here put a `PostgreSQL` round trip, and on fragmented capacity a
    /// full renewal cadence, directly on the query hot path. Overdraft keeps the
    /// query moving under the local per-tenant ceilings while the background
    /// refill lands.
    ///
    /// # Panics
    ///
    /// Panics when a cache miss fails to grant or does not charge overdraft.
    #[test]
    fn cache_miss_grants_from_overdraft_without_waiting() {
        let (owner, _demand_rx, _) = owner();
        let request = DelegatedAdmissionRequest {
            tenant_id: DataTenantId::new_v7(),
            principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
            query_class: QueryClass::Interactive,
        };
        let grant = owner
            .acquire(request)
            .expect("cache miss must grant from overdraft");
        assert_eq!(
            owner.state.lock().expect("charged state").overdraft_used,
            1,
            "an overdraft grant must be charged against the bounded ceiling"
        );
        drop(grant);
        assert_eq!(
            owner.state.lock().expect("released state").overdraft_used,
            0,
            "releasing an overdraft grant must return its unit"
        );
    }

    /// Exhausted overdraft refuses immediately rather than parking the query.
    ///
    /// # Panics
    ///
    /// Panics when the ceiling admits more units than configured or refuses with
    /// something other than a capacity rejection.
    #[test]
    fn exhausted_overdraft_refuses_instead_of_blocking() {
        let (owner, _demand_rx, _) = owner_with_queue_capacity(2);
        let request = DelegatedAdmissionRequest {
            tenant_id: DataTenantId::new_v7(),
            principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
            query_class: QueryClass::Interactive,
        };
        let first = owner.acquire(request).expect("first overdraft unit");
        let second = owner.acquire(request).expect("second overdraft unit");
        assert!(matches!(
            owner.acquire(request),
            Err(DelegatedOracleAdmissionError::QueueFull)
        ));
        drop(first);
        owner
            .acquire(request)
            .expect("a returned overdraft unit is immediately reusable");
        drop(second);
    }

    /// Installed durable capacity is charged ahead of overdraft.
    ///
    /// # Panics
    ///
    /// Panics when a cached block is bypassed or overdraft is charged while
    /// durable capacity remains.
    #[test]
    fn cached_capacity_is_charged_before_overdraft() {
        let (owner, mut demand_rx, _) = owner();
        let request = DelegatedAdmissionRequest {
            tenant_id: DataTenantId::new_v7(),
            principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
            query_class: QueryClass::Interactive,
        };
        let priming = owner.acquire(request).expect("priming overdraft grant");
        let demand = demand_rx.try_recv().expect("initial demand");
        assert!(
            owner
                .begin_queued_allocation(demand)
                .expect("worker claims demand")
        );
        install_test_block(&owner, request, demand, 4);
        drop(priming);

        let grant = owner.acquire(request).expect("cached grant");
        let state = owner.state.lock().expect("charged state");
        assert_eq!(
            state.overdraft_used, 0,
            "durable capacity must be spent before overdraft"
        );
        assert_eq!(state.blocks[0].block.used, 1);
        drop(state);
        drop(grant);
        assert_eq!(
            owner.state.lock().expect("released state").blocks[0]
                .block
                .used,
            0,
            "releasing a cached grant must return its durable unit"
        );
    }

    /// Empty allocation stays coalesced until a cadence retry returns capacity.
    ///
    /// # Panics
    ///
    /// Panics when empty settlement loses or duplicates demand.
    #[test]
    fn empty_allocation_retries_on_cadence() {
        let (owner, mut demand_rx, _) = owner();
        let request = DelegatedAdmissionRequest {
            tenant_id: DataTenantId::new_v7(),
            principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
            query_class: QueryClass::Interactive,
        };
        let grant = owner.acquire(request).expect("overdraft grant");
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
        assert!(
            owner
                .begin_retryable_demands()
                .expect("settled demand")
                .is_empty(),
            "installed capacity clears the retry key"
        );
        drop(grant);
    }

    /// Worker cadence cannot claim a demand whose initial notice is buffered.
    ///
    /// # Panics
    ///
    /// Panics when cadence scheduling duplicates the queued allocation or
    /// installs more than one block for the demand.
    #[test]
    fn worker_cadence_before_initial_dequeue_allocates_at_most_once() {
        let (owner, mut demand_rx, _) = owner();
        let request = DelegatedAdmissionRequest {
            tenant_id: DataTenantId::new_v7(),
            principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
            query_class: QueryClass::Interactive,
        };
        let grant = owner.acquire(request).expect("overdraft grant");
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

    /// Continuity loss and shutdown settle every demand phase without reuse.
    ///
    /// # Panics
    ///
    /// Panics when either terminal transition retains demand or permits a later
    /// local acquisition.
    #[test]
    fn terminal_transitions_clear_all_demand_phases() {
        for shutdown in [false, true] {
            let (owner, _demand_rx, _) = owner();
            let request = DelegatedAdmissionRequest {
                tenant_id: DataTenantId::new_v7(),
                principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
                query_class: QueryClass::Interactive,
            };
            let grant = owner.acquire(request).expect("overdraft grant");
            if shutdown {
                owner.stop();
            } else {
                owner.close_continuity();
            }
            {
                let state = owner.state.lock().expect("terminal scheduling state");
                assert!(state.demands.is_empty());
                assert!(!state.available);
            }
            assert!(matches!(
                owner.acquire(request),
                Err(DelegatedOracleAdmissionError::ContinuityLost)
            ));
            drop(grant);
        }
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
        let grant = owner.acquire(request).expect("cached grant");
        assert_eq!(owner.state.lock().expect("state").blocks[0].block.used, 1);
        drop(grant);
        assert_eq!(owner.state.lock().expect("state").blocks[0].block.used, 0);
    }

    /// Stable grant identity survives removal of an earlier idle allocation.
    ///
    /// # Panics
    ///
    /// Panics when vector compaction redirects release to another allocation.
    #[tokio::test]
    async fn grant_release_uses_allocation_identity_after_earlier_retirement() {
        let (owner, _, _) = owner();
        let request = |tenant_id| DelegatedAdmissionRequest {
            tenant_id,
            principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
            query_class: QueryClass::Interactive,
        };
        let first = request(DataTenantId::new_v7());
        let second = request(DataTenantId::new_v7());
        for current in [first, second] {
            install_test_block(&owner, current, owner.demand_for(current), 1);
        }
        let grant = owner.acquire(second).expect("second allocation grant");
        let first_id = owner.state.lock().expect("state").blocks[0]
            .block
            .allocation_id;
        assert_eq!(
            owner.begin_idle_retirements().expect("idle claims"),
            vec![first_id]
        );
        owner
            .finish_idle_retirement(first_id)
            .expect("first allocation retires");
        drop(grant);
        let state = owner.state.lock().expect("compacted state");
        assert_eq!(state.blocks.len(), 1);
        assert_eq!(state.blocks[0].block.used, 0);
    }

    /// Retirement excludes an idle block from both local grants and renewal.
    ///
    /// # Panics
    ///
    /// Panics when a retiring allocation remains usable or renewable.
    #[tokio::test]
    async fn retiring_block_cannot_grant_or_renew() {
        let (owner, mut demand_rx, _) = owner();
        let request = DelegatedAdmissionRequest {
            tenant_id: DataTenantId::new_v7(),
            principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
            query_class: QueryClass::Interactive,
        };
        install_test_block(&owner, request, owner.demand_for(request), 1);
        let allocation_id = owner.state.lock().expect("state").blocks[0]
            .block
            .allocation_id;
        assert_eq!(
            owner.begin_idle_retirements().expect("idle claims"),
            vec![allocation_id]
        );
        assert!(
            owner
                .renewal_allocation_ids()
                .expect("renewal identities")
                .is_empty()
        );
        // A retiring block cannot serve the request, so admission falls through
        // to overdraft and asks the background worker for replacement capacity.
        let replacement = owner
            .acquire(request)
            .expect("retiring capacity falls through to overdraft");
        assert_eq!(
            owner.state.lock().expect("charged state").overdraft_used,
            1,
            "a retiring block must not be charged as durable capacity"
        );
        assert_eq!(
            demand_rx.try_recv().expect("replacement demand").tenant_id,
            request.tenant_id
        );
        drop(replacement);
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
        let grant = owner.acquire(request).expect("initial grant");
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
