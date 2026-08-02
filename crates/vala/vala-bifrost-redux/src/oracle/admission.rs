//! Durable and local Oracle admission ownership.
//!
//! The owner aligns immutable membership planning, tenant-scoped lease
//! transactions, local slot permits, renewal, recovery, and shutdown cleanup so
//! admitted capacity cannot outlive its fenced durable authority.

use super::*;

/// Admission owner for bounded local capacity and immutable membership snapshots.
pub struct OracleAdmission {
    /// Immutable membership snapshots used for one admission calculation.
    pub(super) cluster: Arc<ClusterRegistry>,
    /// Local pending and running capacity guards.
    pub(super) slots: Arc<OracleSlotManager>,
    /// Durable cluster/class/tenant lease transaction owner.
    pub(super) leases: Arc<vala_sql::queries::oracle_admission::OracleAdmissionLeases>,
    /// Platform-admin pool used only during startup recovery.
    pub(super) operator_pool: vala_sql::OperatorPool,
    /// Local role identity and fence authorizing lease mutation.
    pub(super) local_role: RegisteredRole,
    /// Validated tenant ceilings and lifecycle configuration.
    pub(super) config: OracleConfig,
    /// Bounded tenant scopes queued for periodic authoritative repair.
    pub(super) tenant_reconcile_queue: Arc<Mutex<TenantReconcileQueue>>,
    /// Bounded durable leases queued when a public query stream is dropped.
    pub(super) lease_release_queue: Arc<Mutex<LeaseReleaseQueue>>,
}

/// Durable admission lease awaiting asynchronous cleanup by the admission owner.
pub(super) struct PendingLeaseRelease {
    /// Tenant whose transaction contains the lease row.
    pub(super) data_tenant_id: DataTenantId,
    /// Query identity used by the fenced release mutation.
    pub(super) query_id: QueryId,
    /// Leader fence that originally admitted the query.
    pub(super) leader: RoleFence,
}

/// FIFO for lease releases initiated from synchronous `Drop` paths.
///
/// The map is naturally bounded by admitted guards: each active query identity
/// contributes at most one pending release, while the queue preserves FIFO
/// maintenance order and deduplicates retries.
#[derive(Default)]
pub(super) struct LeaseReleaseQueue {
    /// Pending release requests in insertion order.
    pub(super) order: VecDeque<QueryId>,
    /// Query-keyed requests preventing duplicate cleanup entries.
    pub(super) pending: HashMap<QueryId, PendingLeaseRelease>,
}

impl LeaseReleaseQueue {
    /// Enqueues one release request without performing durable IO.
    ///
    /// Returns `false` when the query is already pending; this preserves one
    /// cleanup entry per admitted lease without imposing an arbitrary cap.
    fn enqueue(&mut self, release: PendingLeaseRelease) -> bool {
        if self.pending.contains_key(&release.query_id) {
            return false;
        }
        self.order.push_back(release.query_id);
        self.pending.insert(release.query_id, release);
        true
    }

    /// Takes at most `limit` requests for one maintenance tick.
    fn take(&mut self, limit: usize) -> Vec<PendingLeaseRelease> {
        let mut releases = Vec::with_capacity(self.order.len().min(limit));
        while releases.len() < limit {
            let Some(query_id) = self.order.pop_front() else {
                break;
            };
            if let Some(release) = self.pending.remove(&query_id) {
                releases.push(release);
            }
        }
        releases
    }

    /// Restores failed requests at the FIFO head in their original order.
    fn requeue(&mut self, releases: impl IntoIterator<Item = PendingLeaseRelease>) {
        let releases = releases.into_iter().collect::<Vec<_>>();
        for release in releases.into_iter().rev() {
            let query_id = release.query_id;
            if self.pending.insert(query_id, release).is_none() {
                self.order.push_front(query_id);
            }
        }
    }
}

/// Bounded deduplicated tenant scopes repaired by admission maintenance.
#[derive(Default)]
pub(super) struct TenantReconcileQueue {
    /// FIFO order for bounded maintenance work.
    order: VecDeque<(DataTenantId, QueryClass)>,
    /// Exact set preventing duplicate hot-path insertions.
    members: HashSet<(DataTenantId, QueryClass)>,
}

/// Closed insertion outcome for the bounded tenant reconciliation queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TenantReconcileInsert {
    /// A new distinct scope entered the FIFO.
    Queued,
    /// The scope was already pending and required no second entry.
    Coalesced,
    /// The fixed 64-scope queue had no capacity for a new scope.
    Saturated,
}

/// Immutable durable capacity request computed from one membership snapshot.
struct AdmissionPlan {
    /// Candidate lease bound to the local leader fence.
    lease: OracleAdmissionLease,
    /// Cluster-wide usable slot ceiling.
    cluster_limit: u32,
    /// Independent class ceiling.
    class_limit: u32,
    /// Tenant/class ceiling.
    tenant_limit: u32,
    /// Bounded durable acquisition deadline.
    deadline: Instant,
}

/// Shared terminal reason and cancellation-bound renewal task.
type LeaseRenewal = (Arc<Mutex<Option<QueryTerminalErrorCode>>>, JoinHandle<()>);

impl TenantReconcileQueue {
    /// Enqueues one tenant/class scope without performing durable IO.
    fn enqueue(
        &mut self,
        data_tenant_id: DataTenantId,
        query_class: QueryClass,
    ) -> TenantReconcileInsert {
        let key = (data_tenant_id, query_class);
        if self.members.contains(&key) {
            TenantReconcileInsert::Coalesced
        } else if self.order.len() < 64 {
            self.order.push_back(key);
            self.members.insert(key);
            TenantReconcileInsert::Queued
        } else {
            TenantReconcileInsert::Saturated
        }
    }

    /// Removes at most `limit` scopes for one maintenance transaction.
    fn take(&mut self, limit: usize) -> Vec<AdmissionReconcileScope> {
        let mut scopes = Vec::with_capacity(self.order.len().min(limit));
        while scopes.len() < limit {
            let Some((data_tenant_id, query_class)) = self.order.pop_front() else {
                break;
            };
            scopes.push(AdmissionReconcileScope::Tenant {
                data_tenant_id,
                query_class,
            });
        }
        scopes
    }

    /// Completes successful scopes so a future rejection may enqueue them.
    fn complete(&mut self, scopes: &[AdmissionReconcileScope]) {
        for scope in scopes {
            if let AdmissionReconcileScope::Tenant {
                data_tenant_id,
                query_class,
            } = scope
            {
                self.members.remove(&(*data_tenant_id, *query_class));
            }
        }
    }

    /// Restores a failed maintenance batch at the FIFO head in original order.
    fn requeue(&mut self, scopes: &[AdmissionReconcileScope]) {
        for scope in scopes.iter().rev() {
            if let AdmissionReconcileScope::Tenant {
                data_tenant_id,
                query_class,
            } = scope
            {
                self.order.push_front((*data_tenant_id, *query_class));
            }
        }
    }
}

impl std::fmt::Debug for OracleAdmission {
    /// Formats the admission owner without exposing database handles, role state,
    /// tenant queues, or capacity internals that could leak operational details.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OracleAdmission")
            .finish_non_exhaustive()
    }
}

impl OracleAdmission {
    /// Computes one fenced durable admission plan from the current immutable snapshot.
    ///
    /// # Errors
    ///
    /// Returns role-unavailable, timeout, internal-duration, or capacity errors
    /// when the snapshot cannot produce a valid local lease.
    fn plan_admission(
        &self,
        tenant: DataTenantId,
        query_class: QueryClass,
        deadline: Instant,
    ) -> Result<(AdmissionPlan, chrono::Duration), BifrostError> {
        let snapshot = self.cluster.snapshot();
        let live = snapshot.live_oracles();
        let local_node = self.local_role.key.node_id;
        if !live.iter().any(|role| role.key.node_id == local_node) {
            return Err(BifrostError::OracleRoleUnavailable);
        }
        let usable_slots = live
            .iter()
            .filter_map(|role| match &role.capabilities {
                ClusterCapabilities::OracleV1(capabilities) => Some(capabilities.usable_slots),
                ClusterCapabilities::ScribeV1(_) => None,
            })
            .try_fold(0_u32, u32::checked_add)
            .ok_or(BifrostError::QueryAdmissionRejected)?;
        let (class_limit, demand) = admission_limits(usable_slots, query_class);
        let tenant_limit = match query_class {
            QueryClass::Interactive => self.config.tenant_interactive_slots,
            QueryClass::Analytical => self.config.tenant_analytical_slots,
        };
        let lease_ttl = chrono::Duration::from_std(self.config.lease_ttl).map_err(|_| {
            BifrostError::Internal {
                detail: "Oracle lease TTL exceeds chrono bounds".to_owned(),
            }
        })?;
        if deadline <= Instant::now() {
            return Err(BifrostError::QueryTimeout);
        }
        let now = chrono::Utc::now();
        Ok((
            AdmissionPlan {
                lease: OracleAdmissionLease {
                    query_id: QueryId::new(uuid::Uuid::now_v7()),
                    data_tenant_id: tenant,
                    query_class,
                    slot_units: demand,
                    selected_node_ids: vec![local_node],
                    leader_node_id: local_node,
                    leader_fencing_token: self.local_role.fencing_token,
                    acquired_at: now,
                    expires_at: now + lease_ttl,
                },
                cluster_limit: usable_slots,
                class_limit,
                tenant_limit,
                deadline: deadline.min(Instant::now() + Duration::from_millis(250)),
            },
            lease_ttl,
        ))
    }

    /// Performs the bounded one-retry durable lease transaction.
    ///
    /// Each attempt opens and commits its own tenant transaction. A retryable
    /// rejection sleeps only within the plan deadline; no transaction or lease
    /// survives between attempts. Cancellation drops the active SQL future and
    /// leaves its uncommitted transaction to roll back.
    ///
    /// # Errors
    ///
    /// Returns admission rejection on timeout or exhausted capacity and execution
    /// failure when opening, acquiring, or committing the tenant transaction fails.
    async fn acquire_plan(&self, plan: &AdmissionPlan) -> Result<AdmissionAcquire, BifrostError> {
        for attempt in 0_u8..=1 {
            let request = AdmissionRequest {
                lease: plan.lease.clone(),
                cluster_limit: plan.cluster_limit,
                class_limit: plan.class_limit,
                tenant_limit: plan.tenant_limit,
            };
            let remaining = plan
                .deadline
                .checked_duration_since(Instant::now())
                .ok_or(BifrostError::QueryAdmissionRejected)?;
            let operation = async {
                let mut conn = self
                    .leases
                    .postgres()
                    .tenant_conn(request.lease.data_tenant_id)
                    .await?;
                let outcome = self.leases.acquire(&mut conn, &request).await?;
                conn.commit().await?;
                Ok::<AdmissionAcquire, vala_sql::SqlError>(outcome)
            };
            let outcome = tokio::time::timeout(remaining, operation)
                .await
                .map_err(|_| BifrostError::QueryAdmissionRejected)?
                .map_err(|error| {
                    tracing::error!(error = %error, "Oracle durable admission failed");
                    BifrostError::QueryExecutionFailed
                })?;
            if matches!(outcome, AdmissionAcquire::Acquired(_)) || attempt == 1 {
                return Ok(outcome);
            }
            let jitter_ms = rand::thread_rng().gen_range(20_u64..=100);
            let remaining = plan
                .deadline
                .checked_duration_since(Instant::now())
                .ok_or(BifrostError::QueryAdmissionRejected)?;
            tokio::time::sleep(remaining.min(Duration::from_millis(jitter_ms))).await;
        }
        Err(BifrostError::QueryAdmissionRejected)
    }

    /// Renews one tenant-bound durable lease and commits the outer transaction.
    ///
    /// The mutation and its durable state become visible together at commit.
    /// Cancellation before commit rolls the transaction back; after commit the
    /// returned mutation reflects the durable renewal result.
    ///
    /// # Errors
    ///
    /// Returns [`vala_sql::SqlError`] when the tenant transaction, fenced
    /// renewal, or commit fails.
    async fn renew_lease(
        &self,
        data_tenant_id: DataTenantId,
        query_id: QueryId,
        leader: &RoleFence,
        expiry: chrono::DateTime<chrono::Utc>,
    ) -> Result<LeaseMutation, vala_sql::SqlError> {
        let mut conn = self.leases.postgres().tenant_conn(data_tenant_id).await?;
        let mutation = self
            .leases
            .renew(&mut conn, query_id, leader, expiry)
            .await?;
        conn.commit().await?;
        Ok(mutation)
    }

    /// Releases one tenant-bound durable lease and commits the outer transaction.
    ///
    /// The fenced release is not reported until its tenant transaction commits.
    /// Cancellation before commit leaves expiry or a later idempotent release as
    /// the recovery path and never reports successful cleanup.
    ///
    /// # Errors
    ///
    /// Returns [`vala_sql::SqlError`] when the tenant transaction, fenced
    /// release, or commit fails.
    async fn release_lease(
        &self,
        data_tenant_id: DataTenantId,
        query_id: QueryId,
        leader: &RoleFence,
    ) -> Result<LeaseMutation, vala_sql::SqlError> {
        let mut conn = self.leases.postgres().tenant_conn(data_tenant_id).await?;
        let mutation = self.leases.release(&mut conn, query_id, leader).await?;
        conn.commit().await?;
        Ok(mutation)
    }

    /// Expires and reconciles one tenant/class maintenance scope atomically.
    ///
    /// Expiry and all three aggregate scopes share one tenant transaction. The
    /// method commits only after every reconciliation succeeds; cancellation or
    /// any error rolls the complete maintenance unit back.
    ///
    /// # Errors
    ///
    /// Returns [`vala_sql::SqlError`] when the tenant transaction, expiry,
    /// reconciliation, or commit fails.
    async fn maintain_scope(
        &self,
        scope: &AdmissionReconcileScope,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), vala_sql::SqlError> {
        let AdmissionReconcileScope::Tenant {
            data_tenant_id,
            query_class,
        } = scope
        else {
            return Err(vala_sql::SqlError::InvariantViolation {
                detail: "maintenance requires a tenant scope".to_owned(),
            });
        };
        let mut conn = self.leases.postgres().tenant_conn(*data_tenant_id).await?;
        self.leases.expire_batch(&mut conn, now, 128).await?;
        self.leases
            .reconcile_scopes(
                &mut conn,
                &[
                    AdmissionReconcileScope::Cluster,
                    AdmissionReconcileScope::Class {
                        query_class: *query_class,
                    },
                    scope.clone(),
                ],
                now,
            )
            .await?;
        conn.commit().await
    }

    /// Reclaims expired shared admission state before this role advertises readiness.
    ///
    /// Recovery runs through the operator-owned transaction boundary so expiry,
    /// aggregate repair, and the matching recovery audit commit atomically. The
    /// caller must not mark Oracle ready until this future completes successfully.
    /// Cancellation before completion rolls back the recovery transaction and
    /// leaves readiness false.
    ///
    /// # Errors
    ///
    /// Returns [`vala_sql::SqlError`] when the operator transaction cannot begin,
    /// recover shared scopes, append its audit record, or commit.
    async fn recover_startup(&self) -> Result<(), vala_sql::SqlError> {
        self.leases
            .recover_shared_scopes(&self.operator_pool, chrono::Utc::now())
            .await
    }

    /// Starts cancellation-bound renewal for one acquired durable lease.
    ///
    /// The task renews at the configured cadence until query cancellation. A
    /// stale fence, missing/released lease, or SQL failure records one terminal
    /// code before cancelling the query. Aborting the task may leave the last
    /// committed renewal durable; explicit release or expiry owns final cleanup.
    fn start_lease_renewal(
        self: &Arc<Self>,
        lease: &OracleAdmissionLease,
        cancellation: &CancellationToken,
        lease_ttl: chrono::Duration,
    ) -> LeaseRenewal {
        let renewal_cancel = cancellation.clone();
        let renewal_terminal = Arc::new(Mutex::new(None));
        let terminal = Arc::clone(&renewal_terminal);
        let admission = Arc::clone(self);
        let data_tenant_id = lease.data_tenant_id;
        let query_id = lease.query_id;
        let interval_duration = self.config.lease_renew_interval;
        let leader = RoleFence {
            node_id: self.local_role.key.node_id,
            fencing_token: self.local_role.fencing_token,
        };
        let task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(interval_duration);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            interval.tick().await;
            loop {
                tokio::select! {
                    () = renewal_cancel.cancelled() => break,
                    _ = interval.tick() => {
                        let expiry = chrono::Utc::now() + lease_ttl;
                        let result = admission
                            .renew_lease(data_tenant_id, query_id, &leader, expiry)
                            .await;
                        if let Some((code, outcome)) = renewal_failure(&result) {
                            if let Ok(mut reason) = terminal.lock() {
                                *reason = Some(code);
                            }
                            metrics::counter!(
                                "bifrost_oracle_lease_renewals_total",
                                "outcome" => outcome
                            ).increment(1);
                            renewal_cancel.cancel();
                            break;
                        }
                        metrics::counter!(
                            "bifrost_oracle_lease_renewals_total",
                            "outcome" => "renewed"
                        ).increment(1);
                    }
                }
            }
        });
        (renewal_terminal, task)
    }

    /// Creates a local admission owner.
    #[must_use]
    pub(super) fn new(
        cluster: Arc<ClusterRegistry>,
        slots: Arc<OracleSlotManager>,
        leases: Arc<vala_sql::queries::oracle_admission::OracleAdmissionLeases>,
        local_role: RegisteredRole,
        config: OracleConfig,
        operator_pool: vala_sql::OperatorPool,
    ) -> Self {
        Self {
            cluster,
            slots,
            leases,
            operator_pool,
            local_role,
            config,
            tenant_reconcile_queue: Arc::new(Mutex::new(TenantReconcileQueue::default())),
            lease_release_queue: Arc::new(Mutex::new(LeaseReleaseQueue::default())),
        }
    }

    /// Queues one dropped stream's durable lease for lifecycle-owned cleanup.
    fn enqueue_lease_release(&self, release: PendingLeaseRelease) {
        if let Ok(mut queue) = self.lease_release_queue.lock() {
            if !queue.enqueue(release) {
                tracing::debug!("Oracle dropped-lease cleanup request was already pending");
            }
        } else {
            tracing::error!(
                "Oracle dropped-lease cleanup queue lock poisoned; lease expiry remains authoritative"
            );
            metrics::counter!(
                "bifrost_oracle_lease_release_queue_total",
                "outcome" => "queue_poisoned"
            )
            .increment(1);
        }
    }

    /// Drains queued dropped-stream leases before Oracle shutdown completes.
    ///
    /// The deadline prevents shutdown from hanging on a degraded database. A
    /// failed or unfinished release is restored to the queue so
    /// lease expiry remains the explicit crash-safe fallback rather than a
    /// silently discarded cleanup request.
    /// Cancellation of the owning shutdown future may interrupt an active SQL
    /// release; the uncommitted transaction rolls back and durable expiry remains
    /// authoritative. Releases committed before cancellation remain complete.
    pub(super) async fn drain_lease_releases(&self, deadline: Instant) {
        loop {
            let releases = self
                .lease_release_queue
                .lock()
                .map(|mut queue| queue.take(64))
                .unwrap_or_default();
            if releases.is_empty() {
                return;
            }
            let mut pending = VecDeque::from(releases);
            while let Some(release) = pending.pop_front() {
                let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                    pending.push_front(release);
                    if let Ok(mut queue) = self.lease_release_queue.lock() {
                        queue.requeue(pending);
                    }
                    return;
                };
                match tokio::time::timeout(
                    remaining,
                    self.release_lease(release.data_tenant_id, release.query_id, &release.leader),
                )
                .await
                {
                    Ok(Ok(_)) => {
                        metrics::counter!(
                            "bifrost_oracle_lease_release_queue_total",
                            "outcome" => "released"
                        )
                        .increment(1);
                    }
                    Ok(Err(error)) => {
                        pending.push_front(release);
                        if let Ok(mut queue) = self.lease_release_queue.lock() {
                            queue.requeue(pending);
                        }
                        tracing::error!(error = %error, "Oracle shutdown lease cleanup failed; requests requeued");
                        return;
                    }
                    Err(_) => {
                        pending.push_front(release);
                        if let Ok(mut queue) = self.lease_release_queue.lock() {
                            queue.requeue(pending);
                        }
                        tracing::error!(
                            "Oracle shutdown lease cleanup timed out; requests requeued"
                        );
                        return;
                    }
                }
            }
        }
    }

    /// Starts startup reconciliation followed by cancellation-bound maintenance.
    ///
    /// The returned startup receiver resolves only after recovery commits. The
    /// maintenance task then owns dropped-lease release and tenant reconciliation
    /// until shutdown cancellation, at which point readiness is cleared. Individual
    /// maintenance failures are requeued; already committed work is not replayed.
    ///
    /// # Errors
    ///
    /// Returns an internal error when construction occurs outside a Tokio runtime.
    pub(super) fn start_maintenance(
        self: &Arc<Self>,
        shutdown: CancellationToken,
        ready: Arc<AtomicBool>,
    ) -> Result<(JoinHandle<()>, StartupResultReceiver), BifrostError> {
        let runtime =
            tokio::runtime::Handle::try_current().map_err(|_| BifrostError::Internal {
                detail: "Oracle construction requires an active Tokio runtime".to_owned(),
            })?;
        let admission = Arc::clone(self);
        let queue = Arc::clone(&self.tenant_reconcile_queue);
        let lease_release_queue = Arc::clone(&self.lease_release_queue);
        let maintenance_interval = self.config.maintenance_interval;
        let (startup_tx, startup_rx) = tokio::sync::oneshot::channel();
        let task = runtime.spawn(async move {
            if let Err(error) = admission.recover_startup().await {
                tracing::error!(error = %error, "Oracle startup admission recovery failed");
                let _ = startup_tx.send(Err(BifrostError::Internal {
                    detail: format!("Oracle startup admission recovery failed: {error}"),
                }));
                return;
            }
            ready.store(true, Ordering::Release);
            let _ = startup_tx.send(Ok(()));
            let mut interval = tokio::time::interval(maintenance_interval);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            interval.tick().await;
            loop {
                tokio::select! {
                    () = shutdown.cancelled() => break,
                    _ = interval.tick() => {
                        let releases = lease_release_queue
                            .lock()
                            .map(|mut queue| queue.take(64))
                            .unwrap_or_default();
                        for release in releases {
                            match admission
                                .release_lease(
                                    release.data_tenant_id,
                                    release.query_id,
                                    &release.leader,
                                )
                                .await
                            {
                                Ok(_) => {
                                    metrics::counter!(
                                        "bifrost_oracle_lease_release_queue_total",
                                        "outcome" => "released"
                                    )
                                    .increment(1);
                                }
                                Err(error) => {
                                    if let Ok(mut queue) = lease_release_queue.lock() {
                                        queue.requeue(std::iter::once(release));
                                    }
                                    tracing::error!(error = %error, "Oracle dropped-lease cleanup failed; request requeued");
                                }
                            }
                        }
                        let scopes = queue
                            .lock()
                            .map(|mut queue| queue.take(64))
                            .unwrap_or_default();
                        for scope in scopes {
                            match admission.maintain_scope(&scope, chrono::Utc::now()).await {
                                Ok(()) => {
                                    if let Ok(mut queue) = queue.lock() {
                                        queue.complete(std::slice::from_ref(&scope));
                                    }
                                }
                                Err(error) => {
                                    if let Ok(mut queue) = queue.lock() {
                                        queue.requeue(std::slice::from_ref(&scope));
                                    }
                                    metrics::counter!(
                                        "bifrost_oracle_admission_reconcile_total",
                                        "outcome" => "requeued"
                                    )
                                    .increment(1);
                                    tracing::error!(error = %error, "Oracle tenant admission reconciliation failed; scopes requeued");
                                }
                            }
                        }
                    }
                }
            }
            ready.store(false, Ordering::Release);
        });
        Ok((task, startup_rx))
    }

    /// Reports whether at least one Oracle role and local slot are available.
    #[must_use]
    pub fn is_available(&self) -> bool {
        !self.cluster.snapshot().live_oracles().is_empty() && self.slots.running_capacity() > 0
    }

    /// Acquires cluster/class/tenant capacity and the matching local running guard.
    ///
    /// The calculation uses one immutable membership snapshot. This single-node
    /// task selects the local leader and never downgrades analytical work. The
    /// supplied cancellation is a child of Oracle's lifecycle token and remains
    /// owned by the returned stream guard so role shutdown cancels accepted work.
    ///
    /// # Errors
    ///
    /// Returns stable admission rejection for bounded waiter, durable capacity,
    /// local-capacity, or placement failure; SQL failures fail the query closed.
    /// Cancellation before durable acquisition returns without a lease. After
    /// acquisition, local-slot failure awaits fenced durable release; success
    /// transfers the lease, renewal task, cancellation token, and local permits to
    /// the returned guard so no admitted capacity is detached from stream cleanup.
    #[tracing::instrument(
        name = "bifrost.oracle.admission",
        skip_all,
        fields(query_class = query_class_label(query_class))
    )]
    pub(super) async fn admit(
        self: &Arc<Self>,
        tenant: DataTenantId,
        query_class: QueryClass,
        deadline: Instant,
        cancellation: CancellationToken,
    ) -> Result<AdmittedQueryGuard, BifrostError> {
        let mut waiter = OracleTelemetry::start_admission_waiter(query_class);
        let pending = match self.slots.try_pending() {
            Ok(pending) => pending,
            Err(error) => {
                OracleTelemetry::record_admission_rejection("cluster", query_class);
                waiter.finish("cluster", "rejected");
                return Err(error);
            }
        };
        let (plan, lease_ttl) = self.plan_admission(tenant, query_class, deadline)?;
        let demand = plan.lease.slot_units;
        let local_node = plan.lease.leader_node_id;
        let acquired = self.acquire_plan(&plan).await?;
        match acquired {
            AdmissionAcquire::Rejected {
                scope,
                retry_after_ms,
            } => {
                debug_assert_eq!(retry_after_ms, 1_000);
                if scope == AdmissionScope::Tenant {
                    let insert = self
                        .tenant_reconcile_queue
                        .lock()
                        .map_or(TenantReconcileInsert::Saturated, |mut queue| {
                            queue.enqueue(tenant, query_class)
                        });
                    if insert == TenantReconcileInsert::Saturated {
                        metrics::counter!(
                            "bifrost_oracle_admission_reconcile_queue_total",
                            "outcome" => "saturated"
                        )
                        .increment(1);
                    }
                }
                let scope = admission_scope_label(scope);
                OracleTelemetry::record_admission_rejection(scope, query_class);
                waiter.finish(scope, "rejected");
                Err(BifrostError::QueryAdmissionRejected)
            }
            AdmissionAcquire::Acquired(lease) => {
                waiter.finish("cluster", "acquired");
                drop(pending);
                let _slot_span = tracing::info_span!(
                    "bifrost.oracle.slot_reservation",
                    role = "leader",
                    query_class = query_class_label(query_class),
                    slot_units = demand
                );
                let running = match self.slots.try_running(demand) {
                    Ok(running) => {
                        OracleTelemetry::record_slot_reservation(query_class, "acquired");
                        running
                    }
                    Err(error) => {
                        OracleTelemetry::record_slot_reservation(query_class, "rejected");
                        OracleTelemetry::record_admission_rejection("cluster", query_class);
                        let leader = RoleFence {
                            node_id: local_node,
                            fencing_token: self.local_role.fencing_token,
                        };
                        let _ = self
                            .release_lease(lease.data_tenant_id, lease.query_id, &leader)
                            .await;
                        return Err(error);
                    }
                };
                let slot_telemetry = OracleTelemetry::start_slot_use(query_class, demand);
                let (renewal_terminal, renewal) =
                    self.start_lease_renewal(&lease, &cancellation, lease_ttl);
                Ok(AdmittedQueryGuard {
                    data_tenant_id: lease.data_tenant_id,
                    query_id: lease.query_id,
                    leader: RoleFence {
                        node_id: local_node,
                        fencing_token: self.local_role.fencing_token,
                    },
                    admission: Arc::clone(self),
                    running: Some(running),
                    slot_telemetry: Some(slot_telemetry),
                    live_reservations: Vec::new(),
                    cancellation,
                    renewal_terminal,
                    renewal: Some(renewal),
                    release_pending: true,
                })
            }
        }
    }
}

/// Maps a non-renewed lease result to its terminal code and closed metric label.
fn renewal_failure(
    result: &Result<LeaseMutation, vala_sql::SqlError>,
) -> Option<(QueryTerminalErrorCode, &'static str)> {
    match result {
        Ok(LeaseMutation::Renewed(_)) => None,
        Ok(LeaseMutation::StaleLeaderFence) => Some((
            QueryTerminalErrorCode::QueryPeerSecurity,
            "stale_leader_fence",
        )),
        Ok(LeaseMutation::Missing) => {
            Some((QueryTerminalErrorCode::QueryExecutionFailed, "missing"))
        }
        Ok(LeaseMutation::Released | LeaseMutation::AlreadyReleased) => {
            Some((QueryTerminalErrorCode::QueryExecutionFailed, "released"))
        }
        Err(error) => {
            tracing::error!(error = %error, "Oracle admission renewal failed");
            Some((QueryTerminalErrorCode::QueryExecutionFailed, "sql_error"))
        }
    }
}

/// Stream-owned durable/local capacity cleanup.
pub(super) struct AdmittedQueryGuard {
    /// Tenant whose transaction owns the durable lease and cleanup.
    pub(super) data_tenant_id: DataTenantId,
    /// Durable lease identity released on every completion path.
    pub(super) query_id: QueryId,
    /// Fenced leader authorized to release the durable lease.
    pub(super) leader: RoleFence,
    /// Durable admission workflow owner retained through asynchronous cleanup.
    pub(super) admission: Arc<OracleAdmission>,
    /// Local running capacity retained until stream completion/drop.
    pub(super) running: Option<OwnedSemaphorePermit>,
    /// Canonical local slot-use gauge retained with the running permit.
    pub(super) slot_telemetry: Option<SlotTelemetryGuard>,
    /// Parent reservations retaining drained live batches through stream cleanup.
    pub(super) live_reservations: Vec<AccountedMemoryReservation>,
    /// Cancellation shared with the 20-second lease renewer and stream.
    pub(super) cancellation: CancellationToken,
    /// Typed late terminal selected by a failed durable lease renewal.
    pub(super) renewal_terminal: Arc<Mutex<Option<QueryTerminalErrorCode>>>,
    /// Renewal task aborted when the stream completes or drops.
    pub(super) renewal: Option<JoinHandle<()>>,
    /// Whether explicit awaited release still owns durable cleanup.
    pub(super) release_pending: bool,
}

impl Drop for AdmittedQueryGuard {
    /// Releases local capacity and queues bounded durable cleanup.
    ///
    /// Drop never spawns a task or performs remote IO. The retained admission
    /// owner drains this request from its lifecycle maintenance task, while
    /// the explicit stream cancellation path continues to await [`Self::release`].
    fn drop(&mut self) {
        self.cancellation.cancel();
        if let Some(renewal) = self.renewal.take() {
            renewal.abort();
        }
        self.running.take();
        self.slot_telemetry.take();
        if self.release_pending {
            self.release_pending = false;
            self.admission.enqueue_lease_release(PendingLeaseRelease {
                data_tenant_id: self.data_tenant_id,
                query_id: self.query_id,
                leader: RoleFence {
                    node_id: self.leader.node_id,
                    fencing_token: self.leader.fencing_token,
                },
            });
        }
    }
}

impl AdmittedQueryGuard {
    /// Cancels renewal, releases local resources, and durably releases the lease.
    ///
    /// The query stream awaits this method after emitting its terminal frame or
    /// while servicing explicit cancellation, making cleanup observable before
    /// the caller regains control. Local permits and memory are released before
    /// durable IO. If caller cancellation interrupts the durable release, `Drop`
    /// requeues the lease and expiry remains the final recovery boundary.
    pub(super) async fn release(mut self) {
        self.cancellation.cancel();
        if let Some(renewal) = self.renewal.take() {
            renewal.abort();
        }
        self.live_reservations.clear();
        self.running.take();
        self.slot_telemetry.take();
        let result = self
            .admission
            .release_lease(self.data_tenant_id, self.query_id, &self.leader)
            .await;
        if let Err(error) = result {
            tracing::error!(error = %error, "Oracle admission cleanup failed");
        } else {
            self.release_pending = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tenant repair insertion coalesces duplicates and saturates at 64 scopes.
    #[test]
    fn oracle_tenant_reconcile_queue_coalesces_and_saturates_without_io() {
        let mut queue = TenantReconcileQueue::default();
        let first = DataTenantId::new_v7();
        assert_eq!(
            queue.enqueue(first, QueryClass::Interactive),
            TenantReconcileInsert::Queued
        );
        assert_eq!(
            queue.enqueue(first, QueryClass::Interactive),
            TenantReconcileInsert::Coalesced
        );
        for _ in 1..64 {
            assert_eq!(
                queue.enqueue(DataTenantId::new_v7(), QueryClass::Interactive),
                TenantReconcileInsert::Queued
            );
        }
        assert_eq!(queue.order.len(), 64);
        assert_eq!(queue.members.len(), 64);
        assert_eq!(
            queue.enqueue(DataTenantId::new_v7(), QueryClass::Analytical),
            TenantReconcileInsert::Saturated
        );
    }

    /// Failed maintenance restores FIFO order and success releases membership.
    #[test]
    fn oracle_tenant_reconcile_queue_requeues_failed_batch_in_order() {
        let mut queue = TenantReconcileQueue::default();
        let tenants = [DataTenantId::new_v7(), DataTenantId::new_v7()];
        for tenant in tenants {
            assert_eq!(
                queue.enqueue(tenant, QueryClass::Analytical),
                TenantReconcileInsert::Queued
            );
        }
        let scopes = queue.take(64);
        assert!(queue.order.is_empty());
        assert_eq!(queue.members.len(), 2);
        queue.requeue(&scopes);
        assert_eq!(queue.take(64), scopes);
        queue.complete(&scopes);
        assert!(queue.members.is_empty());
        assert_eq!(
            queue.enqueue(tenants[0], QueryClass::Analytical),
            TenantReconcileInsert::Queued
        );
    }

    /// Pending lease cleanup has no arbitrary cap and deduplicates one query identity.
    #[test]
    fn oracle_lease_release_queue_retains_distinct_entries() {
        let mut queue = LeaseReleaseQueue::default();
        let tenant = DataTenantId::new_v7();
        let leader = wyrd_spec::vala::api::NodeId::new(uuid::Uuid::now_v7());
        let mut query_ids = Vec::new();
        for _ in 0..300 {
            let query_id = QueryId::new(uuid::Uuid::now_v7());
            query_ids.push(query_id);
            assert!(queue.enqueue(PendingLeaseRelease {
                data_tenant_id: tenant,
                query_id,
                leader: RoleFence {
                    node_id: leader,
                    fencing_token: 1,
                },
            }));
        }
        assert_eq!(queue.order.len(), 300);
        assert_eq!(queue.pending.len(), 300);
        assert!(!queue.enqueue(PendingLeaseRelease {
            data_tenant_id: tenant,
            query_id: query_ids[0],
            leader: RoleFence {
                node_id: leader,
                fencing_token: 1,
            },
        }));
        assert_eq!(queue.take(usize::MAX).len(), 300);
        assert!(queue.order.is_empty());
        assert!(queue.pending.is_empty());
    }
}
