//! Role-fenced local ownership of delegated Oracle policy capacity.

use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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
    /// Request whose three scopes must become available together.
    request: DelegatedAdmissionRequest,
    /// Completion notification.
    sender: oneshot::Sender<DelegatedOracleAdmissionGrant>,
}

/// Mutable cached blocks, fairness queue, and lifecycle latches.
struct DelegatedState {
    /// Cached allocations issued to this exact role incarnation.
    blocks: Vec<DelegatedAdmissionBlock>,
    /// Stable FIFO across tenant and principal request identities.
    waiters: VecDeque<DelegatedWaiter>,
    /// Coalesced outstanding background demands.
    outstanding: HashSet<(DataTenantId, PrincipalId, QueryClass)>,
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
                outstanding: HashSet::new(),
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
    ) -> Result<(), DelegatedOracleAdmissionError> {
        if block.holder_node_id != self.node_id
            || block.holder_fencing_token != self.fencing_token
            || block.units == 0
            || block.valid_from > database_now
            || block.expires_at <= database_now
        {
            return Err(DelegatedOracleAdmissionError::ContinuityLost);
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| DelegatedOracleAdmissionError::StateUnavailable)?;
        state
            .outstanding
            .remove(&(block.tenant_id, block.principal_id, block.query_class));
        state.blocks.push(block);
        Self::grant_waiters(&self.state, &mut state);
        Ok(())
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
        let receiver = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| DelegatedOracleAdmissionError::StateUnavailable)?;
            if !state.available {
                return Err(DelegatedOracleAdmissionError::ContinuityLost);
            }
            if let Some(index) = available_block(&state.blocks, request, Utc::now()) {
                state.blocks[index].used += 1;
                return Ok(DelegatedOracleAdmissionGrant::new(self, index));
            }
            if state.waiters.len() >= self.config.queue_capacity {
                return Err(DelegatedOracleAdmissionError::QueueFull);
            }
            let (sender, receiver) = oneshot::channel();
            state.waiters.push_back(DelegatedWaiter { request, sender });
            if state.outstanding.insert((
                request.tenant_id,
                request.principal_id,
                request.query_class,
            )) && self
                .demand_tx
                .try_send(OracleAdmissionDemand {
                    tenant_id: request.tenant_id,
                    principal_id: request.principal_id,
                    query_class: request.query_class,
                    requested_units: self.config.allocation_units,
                    holder_node_id: self.node_id,
                    holder_fencing_token: self.fencing_token,
                })
                .is_err()
            {
                state.outstanding.remove(&(
                    request.tenant_id,
                    request.principal_id,
                    request.query_class,
                ));
                state.waiters.pop_back();
                return Err(DelegatedOracleAdmissionError::ContinuityLost);
            }
            receiver
        };
        receiver
            .await
            .map_err(|_| DelegatedOracleAdmissionError::ContinuityLost)
    }

    /// Closes new grants and emits exactly one typed continuity-loss notification.
    pub fn close_continuity(&self) {
        let notify = self.state.lock().is_ok_and(|mut state| {
            state.available = false;
            state.waiters.clear();
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

    /// Returns allocation identities currently cached for background renewal.
    fn allocation_ids(&self) -> Result<Vec<uuid::Uuid>, DelegatedOracleAdmissionError> {
        self.state
            .lock()
            .map(|state| {
                state
                    .blocks
                    .iter()
                    .map(|block| block.allocation_id)
                    .collect()
            })
            .map_err(|_| DelegatedOracleAdmissionError::StateUnavailable)
    }

    /// Grants queued requests in stable arrival order while complete units exist.
    fn grant_waiters(owner: &Arc<Mutex<DelegatedState>>, state: &mut DelegatedState) {
        let mut retained = VecDeque::new();
        while let Some(waiter) = state.waiters.pop_front() {
            if let Some(index) = available_block(&state.blocks, waiter.request, Utc::now()) {
                state.blocks[index].used += 1;
                let grant = DelegatedOracleAdmissionGrant {
                    owner: Arc::clone(owner),
                    block_index: index,
                    released: false,
                };
                if waiter.sender.send(grant).is_err() {
                    state.blocks[index].used = state.blocks[index].used.saturating_sub(1);
                }
            } else {
                retained.push_back(waiter);
            }
        }
        state.waiters = retained;
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
                () = self.shutdown.cancelled() => break,
                demand = self.demand_rx.recv() => match demand {
                    Some(demand) => self.allocate(demand).await,
                    None => Err(DelegatedOracleAdmissionError::ContinuityLost),
                },
                _ = renewal.tick() => self.renew().await,
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
        let rows = blocks
            .allocate(
                demand,
                chrono::Duration::from_std(self.admission.config.validity)
                    .map_err(|_| DelegatedOracleAdmissionError::InvalidConfig)?,
            )
            .await
            .map_err(|_| DelegatedOracleAdmissionError::ContinuityLost)?;
        if rows.is_empty() {
            tracing::debug!("delegated Oracle admission capacity unavailable or fragmented");
            return Ok(());
        }
        let block = DelegatedAdmissionBlock::try_from_rows(&rows)?;
        self.admission.install_block(block, Utc::now())
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
            let renewed = blocks
                .renew(
                    allocation_id,
                    self.admission.node_id.as_uuid(),
                    self.admission.fencing_token,
                    validity,
                )
                .await
                .map_err(|_| DelegatedOracleAdmissionError::ContinuityLost)?;
            if !renewed {
                return Err(DelegatedOracleAdmissionError::ContinuityLost);
            }
        }
        Ok(())
    }
}

/// Finds one valid complete allocation for a request.
fn available_block(
    blocks: &[DelegatedAdmissionBlock],
    request: DelegatedAdmissionRequest,
    now: DateTime<Utc>,
) -> Option<usize> {
    blocks.iter().position(|block| {
        block.tenant_id == request.tenant_id
            && block.principal_id == request.principal_id
            && block.query_class == request.query_class
            && block.expires_at > now
            && block.used < block.units
    })
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
            && let Some(block) = state.blocks.get_mut(self.block_index)
        {
            block.used = block.used.saturating_sub(1);
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
        let (demand_tx, demand_rx) = mpsc::channel(8);
        let (loss_tx, loss_rx) = mpsc::channel(8);
        let owner = DelegatedOracleAdmission::new(
            NodeId::new(uuid::Uuid::from_u128(1)),
            3,
            DelegatedOracleAdmissionConfig::default(),
            demand_tx,
            loss_tx,
        )
        .expect("valid owner");
        (Arc::new(owner), demand_rx, loss_rx)
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
            )
            .expect("matching block");
        let grant = owner.acquire(request).await.expect("cached grant");
        assert_eq!(owner.state.lock().expect("state").blocks[0].used, 1);
        drop(grant);
        assert_eq!(owner.state.lock().expect("state").blocks[0].used, 0);
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
