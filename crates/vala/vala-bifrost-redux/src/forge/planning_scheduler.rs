//! Durable demand scheduling without rewrite or Iceberg commit execution.

use std::collections::{BTreeMap, BTreeSet};
#[cfg(feature = "test-support")]
use std::sync::Arc;
#[cfg(feature = "test-support")]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use chrono::Utc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use vala_sql::SqlError;
use vala_sql::queries::forge_operations::ForgeOperations;
use vala_sql::queries::forge_tasks::{ForgeEnqueueBatch, ForgeTasks};
use vala_sql::row_types::forge_operations::ForgeOperationFamily;
use vala_sql::row_types::forge_tasks::{
    ExpiredCleanupPayload, FORGE_TASK_PAYLOAD_VERSION, ForgePlanningDemand, ForgeTaskEstimates,
    ForgeTaskPlan, ForgeTaskStrategy, ForgeTaskTableIdentity, NewForgeTask,
    ORPHAN_CLEANUP_PAYLOAD_VERSION, OrphanCleanupPayload,
};
use wyrd_spec::DataTenantId;

use super::Forge;
use super::compact::ForgeGroupKey;
use super::error::ForgeError;
use super::identity::task_table_binding;
use super::managed::policy::ForgeTablePolicy;
use super::metrics::{ForgePendingTasks, ForgeTelemetry};
use super::path::catalog_path_to_object_key;
use super::planner::{ForgePlanCandidate, ForgeTableSnapshot, plan_hash, plan_table};
#[cfg(feature = "test-support")]
use super::worker::ForgeLifecycleEvent;
use crate::catalog::layout::forge_data_location;
use crate::maintenance::StagingFileCommitted;

/// Complete classification from one bounded durable scheduler pass.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ForgeScheduleOutcome {
    /// Whether another live scheduler owned the singleton planning lease.
    pub standby: bool,
    /// Durable demands observed in the bounded page.
    pub demands_seen: usize,
    /// Demands whose complete exact task set was acknowledged.
    pub demands_acknowledged: usize,
    /// Exact executable tasks offered to durable enqueue.
    pub tasks_enqueued: usize,
    /// Planned rows dropped by idempotent enqueue conflicts.
    pub tasks_not_inserted: usize,
    /// Maximum-minus-minimum admitted task count across this complete pass's tenants.
    pub fairness_lag_tasks: usize,
    /// Candidate input files observed in this cycle; only complete cycles publish gauges.
    pub compaction_debt_files: u64,
    /// Candidate input bytes observed in this cycle; only complete cycles publish gauges.
    pub compaction_debt_bytes: u64,
    /// Whether the bounded page or any demand remained incomplete.
    pub incomplete: bool,
}

/// Local accounting returned after one durable demand generation is handled.
#[derive(Debug, Default, Clone, Copy)]
struct DemandPlanningResult {
    /// Tasks passed to the atomic enqueue transaction.
    tasks_enqueued: usize,
    /// Planned rows not inserted because the exact task already existed.
    tasks_not_inserted: usize,
    /// Whether the exact observed demand generation was acknowledged.
    acknowledged: bool,
    /// Candidate input files represented by this exact demand generation.
    compaction_debt_files: u64,
    /// Candidate input bytes represented by this exact demand generation.
    compaction_debt_bytes: u64,
}

/// Disposable coverage of one traversal under an exact scheduler generation.
#[derive(Default)]
struct PlanningCycle {
    /// Generation whose authority bounds every observation in this cycle.
    fence: i64,
    /// Roster identities already durably requested, including acknowledged tables.
    seeded: BTreeSet<(DataTenantId, ForgeTaskTableIdentity)>,
    /// Demand identities already attempted, including failed or disappeared tables.
    attempted: BTreeSet<(DataTenantId, ForgeTaskTableIdentity)>,
    /// Latest successful file/byte debt observation for each table.
    debt: BTreeMap<(DataTenantId, ForgeTaskTableIdentity), (u64, u64)>,
    /// Sticky failure or generation race; page exhaustion never clears it.
    failed: bool,
}

/// Concrete owner of fenced durable Forge planning and demand convergence.
pub struct ForgeScheduler<'forge> {
    /// Forge dependency owner used only for catalog reads and deterministic discovery.
    forge: &'forge Forge,
    /// Durable task and demand owner.
    tasks: ForgeTasks,
    /// Stable scheduler lease owner for this process.
    owner: Uuid,
    /// Bounded demand page size.
    demand_cap: u32,
    /// Unfinished local traversal; dropped on cancellation, standby, or fence change.
    cycle: Option<PlanningCycle>,
    /// Deterministic pause before the post-roster renewal in scheduler tests.
    #[cfg(feature = "test-support")]
    renewal_gate: Arc<SchedulerRenewalGate>,
    /// Deterministic pause before demand acknowledgement in scheduler tests.
    #[cfg(feature = "test-support")]
    demand_ack_gate: Arc<SchedulerDemandAckGate>,
    /// Deterministic pause after a refreshed demand is read in scheduler tests.
    #[cfg(feature = "test-support")]
    demand_refresh_gate: Arc<SchedulerDemandRefreshGate>,
    /// Complete-only publication count observed by scheduler tests.
    #[cfg(feature = "test-support")]
    complete_publications: Arc<AtomicUsize>,
}

/// Deterministic coordination around the post-roster scheduler renewal.
#[cfg(feature = "test-support")]
#[derive(Default)]
struct SchedulerRenewalGate {
    /// Arms exactly one pause before demand discovery.
    armed: AtomicBool,
    /// Records that the pass is currently stopped at the renewal boundary.
    paused: AtomicBool,
    /// Signals that the scheduling pass reached the renewal boundary.
    reached: tokio::sync::Notify,
    /// Releases the paused pass after the test changes durable leadership.
    release: tokio::sync::Notify,
}

/// Deterministic coordination around acknowledgement of one demand generation.
#[cfg(feature = "test-support")]
#[derive(Default)]
struct SchedulerDemandAckGate {
    /// Arms exactly one pause after planning and before the atomic acknowledgement.
    armed: AtomicBool,
    /// Records that the pass is currently stopped before acknowledgement.
    paused: AtomicBool,
    /// Signals that planning reached the acknowledgement boundary.
    reached: tokio::sync::Notify,
    /// Releases the paused pass after the test replaces the demand generation.
    release: tokio::sync::Notify,
}

/// Deterministic coordination immediately after a demand refresh read.
#[cfg(feature = "test-support")]
#[derive(Default)]
struct SchedulerDemandRefreshGate {
    /// Arms exactly one pause after the scheduler reads a successor generation.
    armed: AtomicBool,
    /// Records that the pass is stopped before retry-side effects.
    paused: AtomicBool,
    /// Signals that the successor generation has been read.
    reached: tokio::sync::Notify,
    /// Releases the paused pass after the test cancels or permits the retry.
    release: tokio::sync::Notify,
}

impl<'forge> ForgeScheduler<'forge> {
    /// Constructs one scheduler over the established Forge dependency graph.
    ///
    /// # Errors
    /// Returns invalid configuration when the demand page bound is invalid.
    pub fn new(forge: &'forge Forge) -> Result<Self, ForgeError> {
        Ok(Self::with_owner(forge, Uuid::now_v7()))
    }

    /// Constructs a scheduler with a stable fixture owner across bounded passes.
    ///
    /// # Errors
    /// Returns invalid configuration when the demand page bound is invalid.
    #[cfg(feature = "test-support")]
    pub fn with_owner_for_test(forge: &'forge Forge, owner: Uuid) -> Result<Self, ForgeError> {
        Ok(Self::with_owner(forge, owner))
    }

    /// Constructs the scheduler dependency graph for one explicit lease owner.
    ///
    /// The demand page bound saturates rather than refusing: an out-of-range
    /// configured value can only mean "more hints than one wake will ever
    /// page", which the maximum already expresses.
    fn with_owner(forge: &'forge Forge, owner: Uuid) -> Self {
        let config = &forge.core.config;
        Self {
            forge,
            cycle: None,
            tasks: ForgeTasks::new(forge.core.operator_pool.clone()),
            owner,
            demand_cap: u32::try_from(config.max_hints_per_wake).unwrap_or(u32::MAX),
            #[cfg(feature = "test-support")]
            renewal_gate: Arc::new(SchedulerRenewalGate::default()),
            #[cfg(feature = "test-support")]
            demand_ack_gate: Arc::new(SchedulerDemandAckGate::default()),
            #[cfg(feature = "test-support")]
            demand_refresh_gate: Arc::new(SchedulerDemandRefreshGate::default()),
            #[cfg(feature = "test-support")]
            complete_publications: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Arms a deterministic pause before demand discovery for a fence-loss test.
    #[cfg(feature = "test-support")]
    pub fn pause_before_demand_renewal_for_test(&self) {
        self.renewal_gate.armed.store(true, Ordering::Release);
    }

    /// Waits until a scheduling pass reaches the armed renewal boundary.
    #[cfg(feature = "test-support")]
    pub async fn wait_for_demand_renewal_pause_for_test(&self) {
        while !self.renewal_gate.paused.load(Ordering::Acquire) {
            self.renewal_gate.reached.notified().await;
        }
    }

    /// Releases one scheduling pass paused before exact fence renewal.
    #[cfg(feature = "test-support")]
    pub fn release_demand_renewal_pause_for_test(&self) {
        self.renewal_gate.release.notify_one();
    }

    /// Arms a deterministic pause before one demand acknowledgement transaction.
    #[cfg(feature = "test-support")]
    pub fn pause_before_demand_acknowledgement_for_test(&self) {
        self.demand_ack_gate.armed.store(true, Ordering::Release);
    }

    /// Waits until a scheduling pass reaches the armed acknowledgement boundary.
    #[cfg(feature = "test-support")]
    pub async fn wait_for_demand_acknowledgement_pause_for_test(&self) {
        while !self.demand_ack_gate.paused.load(Ordering::Acquire) {
            self.demand_ack_gate.reached.notified().await;
        }
    }

    /// Releases one scheduling pass paused before demand acknowledgement.
    #[cfg(feature = "test-support")]
    pub fn release_demand_acknowledgement_pause_for_test(&self) {
        self.demand_ack_gate.release.notify_one();
    }

    /// Arms a deterministic pause immediately after one successor generation read.
    #[cfg(feature = "test-support")]
    pub fn pause_after_demand_refresh_for_test(&self) {
        self.demand_refresh_gate
            .armed
            .store(true, Ordering::Release);
    }

    /// Waits until a scheduling pass reads the successor generation while paused.
    #[cfg(feature = "test-support")]
    pub async fn wait_for_demand_refresh_pause_for_test(&self) {
        while !self.demand_refresh_gate.paused.load(Ordering::Acquire) {
            self.demand_refresh_gate.reached.notified().await;
        }
    }

    /// Releases one scheduling pass paused after a successor generation read.
    #[cfg(feature = "test-support")]
    pub fn release_demand_refresh_pause_for_test(&self) {
        self.demand_refresh_gate.release.notify_one();
    }

    /// Returns complete-only status and telemetry publications by this scheduler.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn complete_publications_for_test(&self) -> usize {
        self.complete_publications.load(Ordering::Acquire)
    }

    /// Durably records an advisory Scribe hint without planning or executing it.
    ///
    /// # Errors
    /// Returns identity or SQL errors. A failed transaction leaves no partial demand.
    pub async fn record_hint(&self, hint: StagingFileCommitted) -> Result<(), ForgeError> {
        let (binding, _) = hint.into_parts();
        let table = ForgeTaskTableIdentity::new(
            crate::catalog::BIFROST_CATALOG_NAME,
            binding.logical_namespace.clone(),
            binding.table_ref.name.clone(),
        )
        .map_err(ForgeError::Sql)?;
        let mut conn = self
            .forge
            .core
            .vala
            .tenant_conn(binding.tenant)
            .await
            .map_err(ForgeError::Sql)?;
        self.tasks
            .upsert_hint(&mut conn, binding.tenant, &table)
            .await
            .map_err(ForgeError::Sql)?;
        conn.commit().await.map_err(ForgeError::Sql)?;
        Ok(())
    }

    /// Advances one bounded page of a roster-aware planning cycle.
    ///
    /// Every wake discovers new registered tables without re-requesting already
    /// seeded identities. Failed demand stays durable but is attempted only once
    /// per cycle. Inventory is published only after a successful full traversal.
    ///
    /// # Errors
    /// Returns fence or SQL failures, retaining an invalidated cycle for traversal
    /// when authority remains valid. Individual discovery/planning failures make
    /// the cycle incomplete and retain demand for the next cycle.
    ///
    /// # Cancellation
    /// Local progress is taken before the first await, so dropping this future
    /// discards it. Cancellation, standby, and fence loss never publish inventory.
    pub async fn schedule_once(
        &mut self,
        stop: &CancellationToken,
    ) -> Result<ForgeScheduleOutcome, ForgeError> {
        let previous = self.cycle.take();
        let Some(fence) = self.acquire_fence().await? else {
            return Ok(ForgeScheduleOutcome {
                standby: true,
                ..ForgeScheduleOutcome::default()
            });
        };
        self.renew_fence(fence).await?;
        let mut cycle = previous
            .filter(|cycle| cycle.fence == fence)
            .unwrap_or_else(|| PlanningCycle {
                fence,
                ..PlanningCycle::default()
            });
        let result = self.schedule_cycle(&mut cycle, stop).await;
        match result {
            Ok((mut outcome, remaining)) => {
                if remaining && !stop.is_cancelled() {
                    self.cycle = Some(cycle);
                }
                // Traversal has closed before publication reads. A failed final
                // read must not keep an exhausted failed cycle open another wake.
                self.publish_status(&mut outcome, fence, stop).await?;
                #[cfg(feature = "test-support")]
                if !outcome.incomplete {
                    self.complete_publications.fetch_add(1, Ordering::AcqRel);
                }
                Ok(outcome)
            }
            Err(error) => {
                cycle.failed = true;
                if !stop.is_cancelled() && !matches!(error, ForgeError::FenceLost { .. }) {
                    self.cycle = Some(cycle);
                }
                Err(error)
            }
        }
    }

    /// Discovers the roster and attempts the next eligible page in this cycle.
    ///
    /// Returns whether eligible demand remains; retained failed or successor
    /// generations for attempted identities do not keep a cycle open.
    ///
    /// # Errors
    /// Returns fence, selection, or demand traversal failures.
    ///
    /// # Cancellation
    /// Stops before the next demand boundary and skips publication.
    async fn schedule_cycle(
        &self,
        cycle: &mut PlanningCycle,
        stop: &CancellationToken,
    ) -> Result<(ForgeScheduleOutcome, bool), ForgeError> {
        let fence = cycle.fence;
        let mut outcome = ForgeScheduleOutcome::default();
        match self.repair_roster(stop, cycle).await {
            Ok(incomplete) => cycle.failed |= incomplete,
            Err(error @ ForgeError::FenceLost { .. }) => return Err(error),
            Err(error) => {
                cycle.failed = true;
                tracing::warn!(error = %error, "Forge roster discovery failed; cycle cannot publish inventory");
            }
        }
        let (demands, _) = self
            .tasks
            .planning_demands_excluding(self.owner, fence, self.demand_cap, &cycle.attempted)
            .await
            .map_err(ForgeError::Sql)?;
        #[cfg(feature = "test-support")]
        self.pause_before_demand_renewal_if_armed().await;
        self.renew_fence(fence).await?;
        outcome.demands_seen = demands.len();
        let mut admitted_by_tenant = BTreeMap::<_, usize>::new();
        self.plan_demands(demands, cycle, stop, &mut outcome, &mut admitted_by_tenant)
            .await?;
        cycle.failed |= outcome.incomplete;
        outcome.fairness_lag_tasks = admitted_by_tenant
            .values()
            .min()
            .zip(admitted_by_tenant.values().max())
            .map_or(0, |(minimum, maximum)| maximum.saturating_sub(*minimum));
        if stop.is_cancelled() {
            outcome.incomplete = true;
            return Ok((outcome, false));
        }
        let (eligible, _) = self
            .tasks
            .planning_demands_excluding(self.owner, fence, 1, &cycle.attempted)
            .await
            .map_err(ForgeError::Sql)?;
        let remaining = !eligible.is_empty();
        outcome.incomplete = cycle.failed || remaining;
        (outcome.compaction_debt_files, outcome.compaction_debt_bytes) = cycle.debt.values().fold(
            (0_u64, 0_u64),
            |(files, bytes), (next_files, next_bytes)| {
                (
                    files.saturating_add(*next_files),
                    bytes.saturating_add(*next_bytes),
                )
            },
        );
        self.renew_fence(fence).await?;
        Ok((outcome, remaining))
    }

    /// Plans a bounded demand page while retaining raced or failed demands for retry.
    ///
    /// The method updates the pass outcome and per-tenant admission counts in
    /// demand order. A generation race is refreshed once, and cancellation
    /// stops before the next demand or retry.
    ///
    /// # Errors
    ///
    /// Returns fence-renewal or durable SQL failures. Individual planning
    /// failures are logged and retained without aborting the remaining page.
    async fn plan_demands(
        &self,
        demands: Vec<ForgePlanningDemand>,
        cycle: &mut PlanningCycle,
        stop: &CancellationToken,
        outcome: &mut ForgeScheduleOutcome,
        admitted_by_tenant: &mut BTreeMap<DataTenantId, usize>,
    ) -> Result<(), ForgeError> {
        let fence = cycle.fence;
        for mut demand in demands {
            if stop.is_cancelled() {
                outcome.incomplete = true;
                break;
            }
            let identity = (demand.data_tenant_id, demand.table_ref.clone());
            cycle.attempted.insert(identity.clone());
            let mut generation_retries = 0_u8;
            loop {
                if stop.is_cancelled() {
                    outcome.incomplete = true;
                    break;
                }
                self.renew_fence(fence).await?;
                match self.plan_demand(&demand, fence).await {
                    Ok(planned) => {
                        outcome.tasks_enqueued = outcome
                            .tasks_enqueued
                            .saturating_add(planned.tasks_enqueued);
                        outcome.tasks_not_inserted = outcome
                            .tasks_not_inserted
                            .saturating_add(planned.tasks_not_inserted);
                        if cycle.seeded.contains(&identity) {
                            cycle.debt.insert(
                                identity,
                                (planned.compaction_debt_files, planned.compaction_debt_bytes),
                            );
                        }
                        outcome.demands_acknowledged = outcome
                            .demands_acknowledged
                            .saturating_add(usize::from(planned.acknowledged));
                        if planned.acknowledged {
                            admitted_by_tenant
                                .entry(demand.data_tenant_id)
                                .and_modify(|count| {
                                    *count = count.saturating_add(planned.tasks_enqueued);
                                })
                                .or_insert(planned.tasks_enqueued);
                        }
                        outcome.incomplete |= !planned.acknowledged;
                        break;
                    }
                    Err(ForgeError::Sql(SqlError::ForgeDemandGenerationChanged))
                        if generation_retries == 0 && !stop.is_cancelled() =>
                    {
                        generation_retries = generation_retries.saturating_add(1);
                        outcome.incomplete = true;
                        let Some(refreshed) = self
                            .tasks
                            .refresh_demand(self.owner, fence, &demand)
                            .await
                            .map_err(ForgeError::Sql)?
                        else {
                            break;
                        };
                        #[cfg(feature = "test-support")]
                        self.pause_after_demand_refresh_if_armed().await;
                        if stop.is_cancelled() {
                            outcome.incomplete = true;
                            break;
                        }
                        demand = refreshed;
                    }
                    Err(error) => {
                        outcome.incomplete = true;
                        tracing::warn!(data_tenant_id = %demand.data_tenant_id, table = %demand.table_ref.table, error = %error, "Forge demand planning failed; retaining demand and continuing tenant page");
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    /// Pauses once at the deterministic post-roster renewal boundary when armed.
    #[cfg(feature = "test-support")]
    async fn pause_before_demand_renewal_if_armed(&self) {
        if self.renewal_gate.armed.swap(false, Ordering::AcqRel) {
            self.renewal_gate.paused.store(true, Ordering::Release);
            self.renewal_gate.reached.notify_waiters();
            self.renewal_gate.release.notified().await;
            self.renewal_gate.paused.store(false, Ordering::Release);
        }
    }

    /// Acquires the singleton planning fence for this scheduler owner.
    ///
    /// # Errors
    ///
    /// Returns invalid configuration, SQL, or fence-loss errors.
    async fn acquire_fence(&self) -> Result<Option<i64>, ForgeError> {
        let lease_seconds =
            u32::try_from(self.forge.core.config.lease_ttl.as_secs()).map_err(|_| {
                ForgeError::InvalidConfig {
                    detail: "scheduler lease TTL exceeds u32 seconds".to_owned(),
                }
            })?;
        self.tasks
            .acquire_scheduler(self.owner, lease_seconds)
            .await
            .map_err(ForgeError::Sql)
    }

    /// Renews the exact scheduler generation without minting a replacement token.
    ///
    /// # Errors
    ///
    /// Returns invalid configuration when the lease does not fit the SQL domain,
    /// or a stale-fence/SQL error when this owner no longer holds the generation.
    ///
    /// # Cancellation
    ///
    /// The single renewal statement either extends the exact generation or
    /// returns without durable partial progress.
    async fn renew_fence(&self, fence: i64) -> Result<(), ForgeError> {
        match self
            .tasks
            .renew_scheduler(self.owner, fence, self.forge.core.config.lease_ttl)
            .await
        {
            Ok(()) => Ok(()),
            Err(SqlError::Conflict { .. }) => Err(ForgeError::FenceLost {
                lease_key: "forge:scheduler:v1".to_owned(),
            }),
            Err(error) => Err(ForgeError::Sql(error)),
        }
    }

    /// Repairs periodic demand from the authoritative registered-table roster.
    ///
    /// The returned flag is true when discovery was partial or cancellation
    /// interrupted the roster before every table was upserted.
    ///
    /// # Errors
    ///
    /// Returns roster, identity, or SQL errors without acknowledging demand.
    async fn repair_roster(
        &self,
        stop: &CancellationToken,
        cycle: &mut PlanningCycle,
    ) -> Result<bool, ForgeError> {
        let (tables, failures) = self.forge.discover_tables().await?;
        let mut roster = BTreeSet::new();
        for key in tables {
            let identity = ForgeTaskTableIdentity::new(
                crate::catalog::BIFROST_CATALOG_NAME,
                key.table_ref.namespace.as_str(),
                key.table_ref.name,
            )
            .map_err(ForgeError::Sql)?;
            roster.insert((key.tenant, identity));
        }
        if failures == 0 {
            cycle.debt.retain(|identity, _| roster.contains(identity));
        }
        for identity in roster {
            if stop.is_cancelled() {
                return Ok(true);
            }
            if cycle.seeded.contains(&identity) {
                continue;
            }
            self.renew_fence(cycle.fence).await?;
            self.tasks
                .upsert_periodic(identity.0, &identity.1)
                .await
                .map_err(ForgeError::Sql)?;
            cycle.seeded.insert(identity);
        }
        Ok(failures > 0)
    }

    /// Plans and atomically acknowledges one exact demand generation.
    ///
    /// # Errors
    ///
    /// Returns identity, catalog, discovery, planning, or SQL errors. A
    /// failure leaves the demand generation available to a later scheduler.
    async fn plan_demand(
        &self,
        demand: &ForgePlanningDemand,
        fence: i64,
    ) -> Result<DemandPlanningResult, ForgeError> {
        let (snapshot, compaction_debt_files, compaction_debt_bytes, orphan_scan_prefix) =
            self.discover_snapshot(demand).await?;
        let ForgeDemandArbitration { executable } = self
            .arbitrate_demand(demand, &snapshot, orphan_scan_prefix)
            .await?;
        let inserted = {
            #[cfg(feature = "test-support")]
            self.pause_before_demand_acknowledgement_if_armed().await;
            self.tasks
                .enqueue_and_acknowledge(
                    self.owner,
                    fence,
                    demand,
                    ForgeEnqueueBatch {
                        executable: &executable,
                    },
                )
                .await
                .map_err(ForgeError::Sql)?
        };
        let result = DemandPlanningResult {
            tasks_enqueued: inserted.len(),
            tasks_not_inserted: executable.len().saturating_sub(inserted.len()),
            acknowledged: true,
            compaction_debt_files,
            compaction_debt_bytes,
        };
        for task_type in &inserted {
            ForgeTelemetry::record_task_created(*task_type);
        }
        #[cfg(feature = "test-support")]
        if result.acknowledged
            && let Some(observer) = &self.forge.core.completion_observer
        {
            // A batch holds at most one task per strategy, so the inserted
            // strategies identify exactly the rows that gained a task id.
            for task in executable
                .iter()
                .filter(|task| inserted.contains(&task.strategy))
            {
                observer.record_lifecycle(ForgeLifecycleEvent::Planned {
                    task_id: self
                        .tasks
                        .task_id_for_plan(task)
                        .await
                        .map_err(ForgeError::Sql)?,
                    tenant: demand.data_tenant_id,
                    table: demand.table_ref.table.clone(),
                    inputs: task.plan.inputs.clone(),
                });
            }
        }
        Ok(result)
    }

    /// Pauses once after planning and before the exact demand-generation CAS.
    #[cfg(feature = "test-support")]
    async fn pause_before_demand_acknowledgement_if_armed(&self) {
        if self.demand_ack_gate.armed.swap(false, Ordering::AcqRel) {
            self.demand_ack_gate.paused.store(true, Ordering::Release);
            self.demand_ack_gate.reached.notify_waiters();
            self.demand_ack_gate.release.notified().await;
            self.demand_ack_gate.paused.store(false, Ordering::Release);
        }
    }

    /// Pauses once after a successor generation refresh and before retry effects.
    #[cfg(feature = "test-support")]
    async fn pause_after_demand_refresh_if_armed(&self) {
        if self.demand_refresh_gate.armed.swap(false, Ordering::AcqRel) {
            self.demand_refresh_gate
                .paused
                .store(true, Ordering::Release);
            self.demand_refresh_gate.reached.notify_waiters();
            self.demand_refresh_gate.release.notified().await;
            self.demand_refresh_gate
                .paused
                .store(false, Ordering::Release);
        }
    }

    /// Reconstructs the deterministic task candidates for one current table snapshot.
    ///
    /// Promotion and rewrite candidates both contribute debt from the same
    /// discovery reads, even when promotion takes admission priority. Expiration
    /// and cleanup never contribute file or byte compaction debt.
    ///
    /// # Errors
    ///
    /// Returns identity, clock, catalog, discovery, or candidate-bound errors.
    async fn discover_snapshot(
        &self,
        demand: &ForgePlanningDemand,
    ) -> Result<(ForgeTableSnapshot, u64, u64, String), ForgeError> {
        let binding = task_table_binding(
            demand.data_tenant_id,
            demand.data_tenant_id,
            &demand.table_ref,
        )?;
        let table = self.forge.load_table(&binding.table_ident()).await?;
        // The orphan scan prefix is this table's current Forge recipe root,
        // normalized to an object key. It is derived here, from the same table
        // load the rest of discovery uses, so a fallback orphan task and the
        // worker that later executes it agree on the prefix by construction.
        let table_location = table.metadata().location().to_owned();
        let orphan_scan_prefix = catalog_path_to_object_key(
            &table_location,
            &binding,
            &self.forge.core.staging,
            &forge_data_location(&table_location),
        )?;
        // Snapshot expiration is the one plan-encoded metadata effect. Open
        // live-rewrite reconciliation shares its candidate because both are
        // resolved by the same fenced expiry pass against current metadata.
        let snapshot_expiry_due = self.snapshot_expiry_due(&table, demand)?;
        let reconciliation_due = self.open_rewrite_requires_reconciliation(&binding).await?;
        let maintenance_candidate = if snapshot_expiry_due || reconciliation_due {
            self.maintenance_candidate(&table, snapshot_expiry_due, reconciliation_due)
                .await?
        } else {
            None
        };
        // Discovery runs to completion above so the due predicates and the
        // candidate bound stay exercised and their debt stays recorded;
        // admission is where the phase boundary applies.
        let maintenance_candidate = maintenance_candidate
            .filter(|candidate| super::phase::admits_new_effect(candidate.strategy));
        let promotion_candidate = self.promotion_candidate(&binding).await?;
        // The rewrite candidate is derived unconditionally so its debt is
        // recorded even on a pass that will not admit it, but it is ordered
        // behind promotion below: a table that still owes Scribe a publication
        // must not start a rewrite against a live set that is about to change.
        let rewrite_candidate = self
            .rewrite_candidate(&table)
            .await?
            .filter(|candidate| super::phase::admits_new_effect(candidate.strategy));
        let (compaction_debt_files, compaction_debt_bytes) = promotion_candidate
            .iter()
            .chain(rewrite_candidate.iter())
            .fold((0_u64, 0_u64), |(files, bytes), candidate| {
                (
                    files.saturating_add(candidate.inputs.len() as u64),
                    bytes.saturating_add(candidate.bytes),
                )
            });
        // Promotion precedes every other demand for the same table: an object
        // Scribe already published must reach the catalog before any pass that
        // reasons about the catalog's contents runs against it.
        let candidates = promotion_candidate
            .into_iter()
            .chain(rewrite_candidate)
            .chain(maintenance_candidate)
            .collect::<Vec<_>>();
        Ok((
            ForgeTableSnapshot {
                snapshot_id: table
                    .metadata()
                    .current_snapshot()
                    .map_or(0, |snapshot| snapshot.snapshot_id()),
                candidates,
            },
            compaction_debt_files,
            compaction_debt_bytes,
            orphan_scan_prefix,
        ))
    }

    /// Builds the exact promotion candidate one table currently owes, if any.
    ///
    /// The candidate is derived entirely from durable Scribe evidence, so it is
    /// deterministic across schedulers: the same eligible rows in the same
    /// production order produce the same inputs, the same parameters, and the
    /// same promoted-file-set digest, which the idempotent enqueue then
    /// collapses into one durable task.
    ///
    /// Promotion never opens the objects it publishes, so its admission working
    /// set is the fixed metadata cost of one commit rather than the promoted
    /// byte total; the byte total is still carried as the candidate's estimate
    /// so telemetry and durable estimates report the real published volume.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Sql`] when the tenant-scoped demand read fails,
    /// [`ForgeError::Invariant`] when a row's promotion evidence is absent or
    /// contradictory, and [`ForgeError::Capacity`] when the fixed promotion
    /// envelope does not fit this scheduler's ceilings.
    async fn promotion_candidate(
        &self,
        binding: &crate::catalog::TenantTableBinding,
    ) -> Result<Option<ForgePlanCandidate>, ForgeError> {
        let mut conn = self
            .forge
            .core
            .vala
            .tenant_conn(binding.tenant)
            .await
            .map_err(ForgeError::Sql)?;
        let demand = super::scribe_promotion::read_promotion_demand(
            &mut conn,
            binding,
            super::scribe_promotion::PROMOTION_BRANCH,
        )
        .await?;
        conn.commit().await.map_err(ForgeError::Sql)?;
        let Some(super::scribe_promotion::ScribePromotionDemand {
            plan, total_bytes, ..
        }) = demand
        else {
            return Ok(None);
        };
        let mut inputs = plan
            .files()
            .iter()
            .map(|file| file.path().as_str().to_owned())
            .collect::<Vec<_>>();
        inputs.sort_unstable();
        Ok(Some(ForgePlanCandidate {
            strategy: ForgeTaskStrategy::ScribePromotion,
            input_bytes: vec![1; inputs.len()],
            inputs,
            bytes: total_bytes.max(1),
            parameters: plan.to_parameters(),
        }))
    }

    /// Builds the small-file rewrite candidate one table currently owes, if any.
    ///
    /// Candidacy is decided from the base snapshot's own live data files and
    /// the configured small-file threshold, so the same snapshot always yields
    /// the same inputs, the same parameters, and therefore the same plan hash —
    /// which is what lets the idempotent enqueue collapse repeated passes over
    /// an unchanged table into one durable task rather than a queue of them.
    ///
    /// A single small file is not debt: rewriting one file into one file
    /// changes nothing a reader can observe and would spend a commit to do it.
    /// The candidate therefore requires at least two, which is also the
    /// smallest input set the managed core can produce a smaller live set from.
    ///
    /// Selection here is a *bound*, not the execution plan. The managed core
    /// performs its own selection and reports what it actually consumed; this
    /// candidate exists to bind the durable task to one immutable base and to
    /// name the inputs recovery will look for.
    ///
    /// The threshold comes from the same [`ForgeTablePolicy`] the worker
    /// extracts, so a table whose declared geometry cannot hold the operator
    /// threshold (for example an undeclared 512 MiB table under a 768 MiB
    /// threshold) is refused here, with a warning, instead of being enqueued as
    /// a task every worker attempt must refuse. Only the rewrite is withheld;
    /// promotion and maintenance for the same table still plan.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Catalog`] when the base snapshot's manifest list
    /// or one of its manifests cannot be read.
    async fn rewrite_candidate(
        &self,
        table: &iceberg::table::Table,
    ) -> Result<Option<ForgePlanCandidate>, ForgeError> {
        let Some(snapshot) = table.metadata().current_snapshot() else {
            return Ok(None);
        };
        let threshold = match ForgeTablePolicy::extract(table.metadata(), &self.forge.core.config) {
            Ok(policy) => policy.small_file_threshold_bytes,
            Err(error) => {
                tracing::warn!(table = %table.identifier(), error = %error, "Forge rewrite policy is impossible for this table; no rewrite is planned");
                return Ok(None);
            }
        };
        let manifests = table
            .manifest_list_reader(snapshot)
            .load()
            .await
            .map_err(ForgeError::Catalog)?;
        let mut selected = BTreeMap::new();
        for manifest_file in manifests.entries() {
            let manifest = manifest_file
                .load_manifest(table.file_io())
                .await
                .map_err(ForgeError::Catalog)?;
            for entry in manifest.entries().iter().filter(|entry| entry.is_alive()) {
                let file = entry.data_file();
                if file.content_type() != iceberg::spec::DataContentType::Data
                    || file.file_size_in_bytes() >= threshold
                {
                    continue;
                }
                selected.insert(file.file_path().to_owned(), file.file_size_in_bytes());
            }
        }
        if selected.len() < 2 {
            return Ok(None);
        }
        let inputs = selected.keys().cloned().collect::<Vec<_>>();
        let input_bytes = selected.values().copied().collect::<Vec<_>>();
        let bytes = input_bytes.iter().copied().fold(0_u64, u64::saturating_add);
        Ok(Some(ForgePlanCandidate {
            strategy: ForgeTaskStrategy::SmallFiles,
            input_bytes,
            inputs,
            bytes: bytes.max(1),
            parameters: serde_json::json!({ "kind": super::worker::LIVE_REWRITE_PARAMETER_KIND }),
        }))
    }

    /// Evaluates the independent snapshot-expiry trigger for one table.
    ///
    /// Maintenance is due when accrued commits past `retain_last` reach
    /// `maintenance_trigger_snapshot_count`, or when the oldest retained
    /// snapshot is older than `maintenance_trigger_interval` and at least one
    /// commit exists past `retain_last`. It reads only in-memory table metadata
    /// plus the process clock, so it is cheap enough to evaluate on every
    /// planning tick regardless of compaction backlog. Commit accrual is
    /// derived from the retained snapshot count rather than a persisted marker
    /// because each executed expiry shrinks that count, resetting the count arm
    /// without additional durable state.
    ///
    /// # Errors
    ///
    /// Returns a clock error when the current time cannot be read.
    fn snapshot_expiry_due(
        &self,
        table: &iceberg::table::Table,
        demand: &ForgePlanningDemand,
    ) -> Result<bool, ForgeError> {
        if !self.forge.core.config.snapshot_expiry_enabled {
            return Ok(false);
        }
        let retained = table.metadata().snapshots().count();
        let commits = retained.saturating_sub(self.forge.core.config.retain_last);
        if commits == 0 {
            return Ok(false);
        }
        if demand.acknowledged_snapshot_id == table.metadata().current_snapshot_id()
            && demand
                .acknowledged_commit_count
                .is_some_and(|acknowledged| commits as u64 <= acknowledged)
        {
            return Ok(false);
        }
        let now_ms = self.forge.core.clock.now()?.timestamp_millis();
        let oldest_age = table
            .metadata()
            .snapshots()
            .map(|snapshot| snapshot.timestamp_ms())
            .min()
            .map(|oldest_ms| {
                Duration::from_millis(u64::try_from(now_ms.saturating_sub(oldest_ms)).unwrap_or(0))
            });
        Ok(maintenance_trigger_due(
            commits,
            oldest_age,
            self.forge.core.config.maintenance_trigger_snapshot_count,
            self.forge.core.config.maintenance_trigger_interval,
            self.forge.core.config.snapshot_retention,
        ))
    }

    /// Reports whether one table has retained rewrite evidence requiring a worker pass.
    ///
    /// Open rewrite state is correctness work, not a periodic trigger. It must
    /// therefore enqueue maintenance even when snapshot retention is not due;
    /// otherwise a crash after Prepared can remain stranded indefinitely.
    ///
    /// # Errors
    /// Returns identity or SQL errors while validating and reading the bounded
    /// operation-state projection.
    async fn open_rewrite_requires_reconciliation(
        &self,
        binding: &crate::catalog::TenantTableBinding,
    ) -> Result<bool, ForgeError> {
        let resource = ForgeGroupKey::table_audit_resource(binding.tenant, &binding.table_ref);
        let operations = ForgeOperations::new(&resource, ForgeOperationFamily::IcebergRewrite)
            .map_err(ForgeError::Sql)?;
        let mut conn = self
            .forge
            .core
            .vala
            .tenant_conn(binding.tenant)
            .await
            .map_err(ForgeError::Sql)?;
        let page = operations
            .list_open(&mut conn, 1, None)
            .await
            .map_err(ForgeError::Sql)?;
        conn.commit().await.map_err(ForgeError::Sql)?;
        Ok(!page.operations.is_empty() || page.overflowed)
    }

    /// Bounds the maintenance inputs to one tick and returns their byte estimate.
    ///
    /// The file ceiling is applied first, then the byte ceiling stops accrual —
    /// but never below one input, so a single oversized manifest still makes
    /// progress instead of producing an empty plan forever. Paths are sorted and
    /// deduplicated so the resulting plan hash is stable for the same manifest
    /// set regardless of manifest-list order. Each recorded size is floored at
    /// one byte so a zero-length manifest still contributes a distinguishable
    /// term.
    ///
    /// Returns the bounded paths, their per-input byte sizes, and the summed
    /// estimate.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when a manifest reports a negative
    /// length or when the byte estimate overflows.
    fn bound_maintenance_inputs(
        maintenance_inputs: Vec<&iceberg::spec::ManifestFile>,
    ) -> Result<(Vec<String>, Vec<u64>, u64), ForgeError> {
        let mut inputs = Vec::new();
        let mut input_bytes = Vec::new();
        let mut bytes = 0_u64;
        for manifest in maintenance_inputs {
            let size =
                u64::try_from(manifest.manifest_length).map_err(|_| ForgeError::Invariant {
                    detail: "Iceberg manifest length is negative".to_owned(),
                })?;
            let Some(next) = bytes.checked_add(size) else {
                return Err(ForgeError::Invariant {
                    detail: "manifest maintenance byte estimate overflowed".to_owned(),
                });
            };
            bytes = next;
            inputs.push(manifest.manifest_path.clone());
            input_bytes.push(size.max(1));
        }
        let mut input_terms = inputs.into_iter().zip(input_bytes).collect::<Vec<_>>();
        input_terms.sort_by(|left, right| left.0.cmp(&right.0));
        input_terms.dedup_by(|left, right| left.0 == right.0);
        let (inputs, input_bytes): (Vec<_>, Vec<_>) = input_terms.into_iter().unzip();
        Ok((inputs, input_bytes, bytes))
    }

    /// Builds one bounded lifecycle task from the current manifest list.
    ///
    /// The scheduler owns this catalog IO because selection uses its validated
    /// process configuration and participates in one fenced planning pass.
    ///
    /// # Errors
    ///
    /// Returns catalog or invariant failures when current metadata cannot be
    /// read or selected manifest sizes exceed representable bounds.
    ///
    /// # Cancellation
    ///
    /// Cancellation during the catalog read produces no durable scheduler state.
    async fn maintenance_candidate(
        &self,
        table: &iceberg::table::Table,
        snapshot_expiry_due: bool,
        reconciliation_due: bool,
    ) -> Result<Option<ForgePlanCandidate>, ForgeError> {
        let Some(snapshot) = table.metadata().current_snapshot() else {
            return Ok(None);
        };
        let manifests = table
            .manifest_list_reader(snapshot)
            .load()
            .await
            .map_err(ForgeError::Catalog)?;
        if !snapshot_expiry_due && !reconciliation_due {
            return Ok(None);
        }
        let maintenance_inputs = if snapshot_expiry_due {
            manifests.entries().iter().collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let (mut inputs, mut input_bytes, mut bytes) =
            Self::bound_maintenance_inputs(maintenance_inputs)?;
        if inputs.is_empty() && reconciliation_due {
            inputs.push(format!("forge://reconcile/{}", snapshot.snapshot_id()));
            input_bytes.push(1);
            bytes = 1;
        } else if inputs.is_empty() {
            return Ok(None);
        }
        let estimate = bytes.max(1);
        Ok(Some(ForgePlanCandidate {
            strategy: ForgeTaskStrategy::SnapshotExpiry,
            inputs,
            input_bytes,
            bytes: estimate,
            parameters: serde_json::json!({
                "kind":"maintenance",
                "trigger_commit_count": table.metadata().snapshots().count()
                    .saturating_sub(self.forge.core.config.retain_last),
                "snapshot_expiry_due": snapshot_expiry_due,
                "reconciliation_due": reconciliation_due,
            }),
        }))
    }

    /// Fills this table's single active-task slot in the fixed strategy order.
    ///
    /// Expired cleanup outranks fresh planning because it consumes a handoff an
    /// expiration already committed. The existing planner's own candidate order
    /// is preserved next. Orphan cleanup is always due, so it comes last and
    /// only when nothing a reader can still observe claimed the slot.
    ///
    /// # Errors
    ///
    /// Returns the handoff, planner, and orphan-projection errors of the
    /// branch that ran. A failure leaves the demand unacknowledged.
    async fn arbitrate_demand(
        &self,
        demand: &ForgePlanningDemand,
        snapshot: &ForgeTableSnapshot,
        orphan_scan_prefix: String,
    ) -> Result<ForgeDemandArbitration, ForgeError> {
        let mut executable = Vec::new();
        // Expired cleanup outranks fresh planning for this table: it consumes a
        // handoff an expiration already committed, and the one-active-task
        // index would refuse a second task anyway. Draining the handoff first
        // is what keeps unreachable objects from accumulating behind new work.
        if let Some(cleanup) = self.expired_cleanup_task(demand).await? {
            executable.push(cleanup);
        }
        let planned = if executable.is_empty() {
            plan_table(snapshot)?
        } else {
            Vec::new()
        };
        for task in planned.into_iter().take(1) {
            executable.push(NewForgeTask {
                data_tenant_id: demand.data_tenant_id,
                table_ref: demand.table_ref.clone(),
                strategy: task.strategy,
                base_snapshot_id: task.base_snapshot_id,
                plan: task.plan,
                plan_hash: task.plan_hash,
                estimates: task.estimates,
                ready_at: Utc::now(),
            });
        }
        // Orphan cleanup is deliberately last. It reclaims objects no metadata
        // references, so it must never displace a compaction, expiration, or
        // cleanup effect that a reader can still observe. It fills the table's
        // single active-task slot only when nothing else claimed it.
        if executable.is_empty() {
            executable.push(self.orphan_cleanup_task(
                demand,
                snapshot.snapshot_id,
                orphan_scan_prefix,
            )?);
        }
        Ok(ForgeDemandArbitration { executable })
    }

    /// Runs one production arbitration for an integration fixture.
    ///
    /// # Errors
    ///
    /// Returns the errors of [`Self::discover_snapshot`] and
    /// [`Self::arbitrate_demand`].
    #[cfg(feature = "test-support")]
    pub async fn arbitrate_demand_for_test(
        &self,
        demand: &ForgePlanningDemand,
    ) -> Result<Vec<NewForgeTask>, ForgeError> {
        let (snapshot, _, _, prefix) = self.discover_snapshot(demand).await?;
        let arbitration = self.arbitrate_demand(demand, &snapshot, prefix).await?;
        Ok(arbitration.executable)
    }

    /// Builds the one cleanup task an unconsumed expiration handoff demands.
    ///
    /// The projection is fixed and bounded: the candidates are already known,
    /// so nothing is discovered, sized against data, or re-derived from the
    /// catalog. The estimates exist only to satisfy admission, and the single
    /// reader permit reflects that the task performs one deletion at a time.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Sql`] when the handoff read or its payload
    /// validation fails, [`ForgeError::Capacity`] when the topology cannot
    /// supply an envelope, and [`ForgeError::Invariant`] when the candidate
    /// count exceeds the durable estimate bounds.
    async fn expired_cleanup_task(
        &self,
        demand: &ForgePlanningDemand,
    ) -> Result<Option<NewForgeTask>, ForgeError> {
        if !super::phase::admits_new_effect(ForgeTaskStrategy::ExpiredCleanup) {
            return Ok(None);
        }
        self.unconsumed_cleanup_projection(demand).await
    }

    /// Builds the one orphan-cleanup task a table's idle planning pass projects.
    ///
    /// The cutoff is computed exactly once here, from the demand generation's
    /// own `last_requested_at`, so the task carries an immutable planning cut
    /// that no later worker re-derives from a moving clock. The scan prefix and
    /// base snapshot are the ones this same planning pass already observed.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when the configured TTL cannot be
    /// converted or the cutoff underflows the representable timestamp range.
    fn orphan_cleanup_task(
        &self,
        demand: &ForgePlanningDemand,
        base_snapshot_id: i64,
        scan_prefix: String,
    ) -> Result<NewForgeTask, ForgeError> {
        let ttl =
            chrono::Duration::from_std(self.forge.core.config.orphan_gc_ttl).map_err(|_| {
                ForgeError::Invariant {
                    detail: "Forge orphan GC TTL exceeds the representable range".to_owned(),
                }
            })?;
        let age_cutoff_ms = demand
            .last_requested_at
            .checked_sub_signed(ttl)
            .ok_or_else(|| ForgeError::Invariant {
                detail: "Forge orphan cleanup cutoff underflows the representable range".to_owned(),
            })?
            .timestamp_millis();
        let payload = OrphanCleanupPayload {
            version: ORPHAN_CLEANUP_PAYLOAD_VERSION,
            age_cutoff_ms,
        };
        let plan = ForgeTaskPlan {
            version: FORGE_TASK_PAYLOAD_VERSION,
            inputs: vec![scan_prefix],
            parameters: payload.to_value(),
        };
        let plan_hash = plan_hash(&plan)?;
        Ok(NewForgeTask {
            data_tenant_id: demand.data_tenant_id,
            table_ref: demand.table_ref.clone(),
            strategy: ForgeTaskStrategy::OrphanCleanup,
            base_snapshot_id,
            plan,
            plan_hash,
            estimates: ForgeTaskEstimates { files: 1, bytes: 1 },
            ready_at: Utc::now(),
        })
    }

    /// Builds the cleanup task for an unconsumed handoff without the phase gate.
    ///
    /// The gate is the caller's, so the projection itself stays provable while
    /// production routing is still owned by a later task.
    ///
    /// # Errors
    ///
    /// Returns the same failures as [`Self::expired_cleanup_task`].
    pub(super) async fn unconsumed_cleanup_projection(
        &self,
        demand: &ForgePlanningDemand,
    ) -> Result<Option<NewForgeTask>, ForgeError> {
        let Some(payload) = self
            .tasks
            .unconsumed_expiration_handoff(demand.data_tenant_id, &demand.table_ref)
            .await
            .map_err(ForgeError::Sql)?
        else {
            return Ok(None);
        };
        cleanup_projection(demand, &payload).map(Some)
    }

    /// Builds the production cleanup projection for one integration fixture.
    ///
    /// Only phase activation is bypassed; the handoff read, validation, and
    /// bounded projection are the production owners.
    ///
    /// # Errors
    ///
    /// Returns the same failures as [`Self::expired_cleanup_task`].
    #[cfg(feature = "test-support")]
    pub async fn expired_cleanup_task_for_test(
        &self,
        demand: &ForgePlanningDemand,
    ) -> Result<Option<NewForgeTask>, ForgeError> {
        self.unconsumed_cleanup_projection(demand).await
    }

    /// Publishes complete-only planning backlog gauges.
    ///
    /// # Errors
    ///
    /// Returns SQL errors while reading the full durable status aggregates.
    /// Fence loss after the read suppresses every complete-only gauge update.
    ///
    /// # Cancellation
    ///
    /// Cancellation during status or renewal IO produces no durable write and
    /// publishes no authoritative gauge update.
    async fn publish_status(
        &self,
        outcome: &mut ForgeScheduleOutcome,
        fence: i64,
        stop: &CancellationToken,
    ) -> Result<(), ForgeError> {
        if outcome.incomplete || outcome.demands_acknowledged != outcome.demands_seen {
            return Ok(());
        }
        self.renew_fence(fence).await?;
        let demands = self
            .tasks
            .planning_demand_status()
            .await
            .map_err(ForgeError::Sql)?;
        let pending = self
            .tasks
            .pending_task_status()
            .await
            .map_err(ForgeError::Sql)?;
        self.renew_fence(fence).await?;
        if stop.is_cancelled() {
            outcome.incomplete = true;
            return Ok(());
        }
        self.forge.core.telemetry.publish_planning_status(
            demands.demands,
            demands
                .oldest_requested_at
                .map_or(0, |time| time.timestamp()),
        );
        let observations: Vec<_> = pending
            .into_iter()
            .map(|status| ForgePendingTasks {
                task_type: status.strategy,
                count: status.pending,
                oldest_ready_at_unix: status.oldest_ready_at.map_or(0, |time| time.timestamp()),
            })
            .collect();
        self.forge
            .core
            .telemetry
            .publish_pending_tasks(&observations);
        self.forge
            .core
            .telemetry
            .record_compaction_debt(outcome.compaction_debt_files, outcome.compaction_debt_bytes);
        Ok(())
    }
}

/// Pure retention-gated count-OR-interval snapshot-expiry trigger predicate.
///
/// `commits` is the count of retained snapshots past `retain_last`, and
/// `oldest_age` is the age of the oldest retained snapshot when the table has
/// any. The oldest snapshot must first cross the configured retention cutoff;
/// otherwise expiry would be a successful no-op and successor demands could
/// starve compaction forever. Once eligible, maintenance is due when
/// `commits >= count_threshold`, or when `oldest_age` has reached `interval`.
/// A table with zero accrued commits is never due. Extracted as a free function
/// so trigger and retention outcomes are exercised without catalog or clock IO.
#[must_use]
pub(super) fn maintenance_trigger_due(
    commits: usize,
    oldest_age: Option<Duration>,
    count_threshold: usize,
    interval: Duration,
    retention: Duration,
) -> bool {
    if commits == 0 {
        return false;
    }
    if oldest_age.is_none_or(|age| age < retention) {
        return false;
    }
    if commits >= count_threshold {
        return true;
    }
    oldest_age.is_some_and(|age| age >= interval)
}

/// The one active-task slot this table's planning pass filled.
struct ForgeDemandArbitration {
    /// At most one enqueueable task, in arbitration order.
    executable: Vec<NewForgeTask>,
}

/// Builds the fixed bounded durable task one cleanup handoff projects to.
///
/// The projection is deterministic in the payload alone: candidate count is the
/// file estimate, the canonical serialization length is the byte estimate, and
/// the base snapshot is the identity the expiration committed.
///
/// # Errors
///
/// Returns [`ForgeError::Invariant`] when the candidate count exceeds the
/// durable estimate bound.
pub(super) fn cleanup_projection(
    demand: &ForgePlanningDemand,
    payload: &ExpiredCleanupPayload,
) -> Result<NewForgeTask, ForgeError> {
    let count = payload.cleanup_candidates.len();
    let files = u32::try_from(count).map_err(|_| ForgeError::Invariant {
        detail: "expired cleanup candidate count exceeds u32".to_owned(),
    })?;
    let bytes = payload.serialized_candidate_bytes().max(1);
    let plan = ForgeTaskPlan {
        version: FORGE_TASK_PAYLOAD_VERSION,
        inputs: Vec::new(),
        parameters: payload.to_value(),
    };
    let plan_hash = plan_hash(&plan)?;
    Ok(NewForgeTask {
        data_tenant_id: demand.data_tenant_id,
        table_ref: demand.table_ref.clone(),
        strategy: ForgeTaskStrategy::ExpiredCleanup,
        base_snapshot_id: payload.committed_snapshot_id,
        plan,
        plan_hash,
        estimates: ForgeTaskEstimates { files, bytes },
        ready_at: Utc::now(),
    })
}

#[cfg(test)]
mod source_tests {
    use std::time::Duration;

    use super::maintenance_trigger_due;
    use vala_sql::row_types::forge_tasks::ForgeTaskStrategy;

    /// The count arm fires at threshold once the oldest snapshot is eligible.
    #[test]
    fn count_arm_fires_at_threshold() {
        assert!(maintenance_trigger_due(
            32,
            Some(Duration::from_secs(1)),
            32,
            Duration::from_hours(1),
            Duration::from_secs(1),
        ));
    }

    /// The interval arm fires when the oldest snapshot ages past the interval
    /// while at least one commit exists but the count arm has not tripped.
    #[test]
    fn interval_arm_fires_with_commits_below_count() {
        assert!(maintenance_trigger_due(
            1,
            Some(Duration::from_hours(2)),
            32,
            Duration::from_hours(1),
            Duration::from_hours(1),
        ));
    }

    /// Neither arm fires for a below-threshold, recently committed table.
    #[test]
    fn not_due_below_count_and_interval() {
        assert!(!maintenance_trigger_due(
            5,
            Some(Duration::from_mins(10)),
            32,
            Duration::from_hours(1),
            Duration::from_hours(1),
        ));
    }

    /// Zero accrued commits is never due, so the interval arm cannot fire on an
    /// empty or freshly maintained table regardless of the reported age.
    #[test]
    fn not_due_without_commits() {
        assert!(!maintenance_trigger_due(
            0,
            Some(Duration::from_hours(100)),
            1,
            Duration::from_hours(1),
            Duration::from_hours(1),
        ));
    }

    /// Count pressure cannot schedule expiry before retention makes work eligible.
    #[test]
    fn count_arm_waits_for_retention_cutoff() {
        assert!(!maintenance_trigger_due(
            200,
            Some(Duration::from_hours(24)),
            32,
            Duration::from_hours(1),
            Duration::from_hours(24 * 7),
        ));
    }

    /// Every production route the scheduler can plan is phase-admitted.
    ///
    /// The scheduler builds exactly four durable strategies: Scribe promotion,
    /// the small-file live rewrite, snapshot expiration, and the two cleanup
    /// routes. Production routing is only real if each of them crosses the
    /// activation boundary, so this pins the closed admitted set rather than a
    /// single arm, and the source assertion keeps the filter on the discovery
    /// path so an inadmissible candidate can never become a durable task.
    #[test]
    fn every_planned_strategy_is_phase_admitted() {
        for strategy in [
            ForgeTaskStrategy::ScribePromotion,
            ForgeTaskStrategy::SmallFiles,
            ForgeTaskStrategy::SnapshotExpiry,
            ForgeTaskStrategy::ExpiredCleanup,
            ForgeTaskStrategy::OrphanCleanup,
        ] {
            assert!(
                crate::forge::phase::admits_new_effect(strategy),
                "{strategy:?} is a production route and must be admitted"
            );
        }

        let source = include_str!("planning_scheduler.rs");
        assert!(
            source.contains("super::phase::admits_new_effect(candidate.strategy)"),
            "discovery filters the candidate through the phase boundary"
        );
    }

    /// The supervised scheduler path contains no direct execution or commit call.
    #[test]
    fn scheduler_source_has_no_datafusion_or_iceberg_execution() {
        let source = include_str!("planning_scheduler.rs");
        assert!(!source.contains(concat!("run_one_live_", "replacement(")));
        assert!(!source.contains(concat!("run_targeted_", "compaction_for_table(")));
        assert!(!source.contains(concat!("commit_", "rewrite(")));
        assert!(!source.contains(concat!(".", "rewrite(")));
    }
}
