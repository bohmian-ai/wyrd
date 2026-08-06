//! Pod-local Oracle query admission and stream-owned resource cleanup.
//!
//! Admission is intentionally process-local. Membership and role fences are
//! used to select the serving leader, while semaphores and memory reservations
//! own every transient query resource without writing per-query SQL state.

use super::*;

/// Admission owner for bounded local capacity and immutable membership snapshots.
pub struct OracleAdmission {
    /// Immutable membership snapshots used for leader selection.
    pub(super) cluster: Arc<ClusterRegistry>,
    /// Local pending and running capacity guards.
    pub(super) slots: Arc<OracleSlotManager>,
    /// Local role identity and fence used for peer dispatch authorization.
    pub(super) local_role: RegisteredRole,
}

/// Leader identity attached to a query while it dispatches peer work.
#[derive(Debug, Clone, Copy)]
pub(super) struct AdmittedLeader {
    /// Physical node identity selected for the query.
    pub(super) node_id: NodeId,
    /// Membership fence observed when admission was granted.
    pub(super) fencing_token: u64,
}

/// Exact resources retained by one query identity for test-tier lifecycle proof.
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

/// Query-identity-keyed lifecycle observation shared with test-tier transports.
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

    /// Returns the current exact ownership for this query only.
    #[must_use]
    pub fn snapshot(&self) -> QueryResourceSnapshot {
        *self.snapshot.borrow()
    }

    /// Subscribes to this query's ownership transitions.
    #[must_use]
    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<QueryResourceSnapshot> {
        self.snapshot.subscribe()
    }

    /// Records synchronous release of query-owned resources.
    fn release_local(&self) {
        self.snapshot.send_modify(|snapshot| {
            snapshot.admission_slots = 0;
            snapshot.memory_bytes = 0;
            snapshot.peer_slots = 0;
        });
    }
}

impl std::fmt::Debug for OracleAdmission {
    /// Formats the admission owner without exposing capacity internals.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OracleAdmission")
            .finish_non_exhaustive()
    }
}

impl OracleAdmission {
    /// Creates a process-local admission owner.
    #[must_use]
    pub(super) fn new(
        cluster: Arc<ClusterRegistry>,
        slots: Arc<OracleSlotManager>,
        local_role: RegisteredRole,
    ) -> Self {
        Self {
            cluster,
            slots,
            local_role,
        }
    }

    /// Starts the lifecycle task without reconstructing query state.
    ///
    /// Readiness is local and immediate once membership has been composed. The
    /// task only waits for shutdown, so no SQL maintenance or recovery work can
    /// keep a query admission path blocked.
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

    /// Reports whether at least one Oracle role and local slot are available.
    #[must_use]
    pub fn is_available(&self) -> bool {
        !self.cluster.snapshot().live_oracles().is_empty() && self.slots.running_capacity() > 0
    }

    /// Acquires local pending/running capacity for one query.
    ///
    /// The selected leader and fence come from the current immutable membership
    /// snapshot. All permits move into the returned guard and are released by
    /// its synchronous terminal/drop cleanup.
    ///
    /// # Errors
    /// Returns a stable admission rejection when the bounded local wait expires,
    /// cancellation interrupts the wait, or no Oracle role is live.
    #[tracing::instrument(
        name = "bifrost.oracle.admission",
        skip_all,
        fields(query_class = query_class_label(query_class))
    )]
    pub(super) async fn admit(
        self: &Arc<Self>,
        _tenant: DataTenantId,
        query_class: QueryClass,
        deadline: Instant,
        cancellation: CancellationToken,
    ) -> Result<AdmittedQueryGuard, BifrostError> {
        let mut waiter = OracleTelemetry::start_admission_waiter(query_class);
        let pending = self.slots.try_pending().inspect_err(|_| {
            waiter.finish("cluster", "rejected");
        })?;
        let local_node = self.local_role.key.node_id;
        if !self
            .cluster
            .snapshot()
            .live_oracles()
            .iter()
            .any(|role| role.key.node_id == local_node)
        {
            waiter.finish("cluster", "rejected");
            return Err(BifrostError::OracleRoleUnavailable);
        }
        let capacity = u32::try_from(self.slots.running_capacity()).unwrap_or(u32::MAX);
        let demand = admission_limits(capacity, query_class).1;
        let running = self
            .slots
            .acquire_running(demand, deadline, &cancellation)
            .await
            .map_err(|_| BifrostError::QueryAdmissionRejected)?;
        drop(pending);
        waiter.finish("cluster", "acquired");
        let query_id = QueryId::new(uuid::Uuid::now_v7());
        Ok(AdmittedQueryGuard {
            query_id,
            leader: AdmittedLeader {
                node_id: local_node,
                fencing_token: self.local_role.fencing_token,
            },
            running: Some(running),
            slot_telemetry: Some(OracleTelemetry::start_slot_use(query_class, demand)),
            live_reservations: Vec::new(),
            cancellation,
            #[cfg(feature = "test-support")]
            resource_probe: None,
        })
    }
}

/// Stream-owned local capacity cleanup.
pub(super) struct AdmittedQueryGuard {
    /// Stable query identity used by peer dispatch and test probes.
    pub(super) query_id: QueryId,
    /// Fenced leader selected at admission time.
    pub(super) leader: AdmittedLeader,
    /// Local running capacity retained until stream completion/drop.
    pub(super) running: Option<OwnedSemaphorePermit>,
    /// Canonical local slot-use gauge retained with the running permit.
    pub(super) slot_telemetry: Option<SlotTelemetryGuard>,
    /// Parent reservations retaining drained live batches through stream cleanup.
    pub(super) live_reservations: Vec<AccountedMemoryReservation>,
    /// Cancellation shared with stream and peer dispatch.
    pub(super) cancellation: CancellationToken,
    /// Query-keyed lifecycle observation retained through cleanup.
    #[cfg(feature = "test-support")]
    pub(super) resource_probe: Option<Arc<QueryResourceProbe>>,
}

impl Drop for AdmittedQueryGuard {
    /// Releases every local resource synchronously and exactly once.
    fn drop(&mut self) {
        self.cancellation.cancel();
        self.live_reservations.clear();
        self.running.take();
        self.slot_telemetry.take();
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
        let slot_units = self.running.as_ref().map_or(0, |permit| {
            u64::try_from(permit.num_permits())
                .expect("invariant: admitted u32 slot demand fits u64")
        });
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
        self.running.take();
        self.slot_telemetry.take();
        #[cfg(feature = "test-support")]
        if let Some(probe) = &self.resource_probe {
            probe.release_local();
        }
    }
}

/// Startup result receiver shared by the Oracle lifecycle owner.
type StartupResultReceiver = tokio::sync::oneshot::Receiver<Result<(), BifrostError>>;

#[cfg(test)]
mod tests {
    #[cfg(feature = "test-support")]
    use super::*;

    /// Query probes release local ownership without a durable mutation.
    #[cfg(feature = "test-support")]
    #[test]
    fn query_resource_probe_releases_local_resources() {
        let probe = QueryResourceProbe::new(QueryId::new(uuid::Uuid::now_v7()), 128, 1);
        probe.release_local();
        assert_eq!(probe.snapshot(), QueryResourceSnapshot::default());
    }
}
