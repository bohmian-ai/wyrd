//! Durable demand scheduling without rewrite or Iceberg commit execution.

use std::collections::BTreeMap;
#[cfg(feature = "test-support")]
use std::sync::Arc;
#[cfg(feature = "test-support")]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use chrono::Utc;
use num_traits::ToPrimitive;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use vala_sql::SqlError;
use vala_sql::queries::forge_tasks::{ForgeClaimLimits, ForgeEnqueueBatch, ForgeTasks};
use vala_sql::row_types::forge_tasks::{
    ForgePlanningDemand, ForgeTaskLane, ForgeTaskStrategy, ForgeTaskTableIdentity, NewForgeTask,
};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

use super::Forge;
use super::error::ForgeError;
use super::identity::task_table_binding;
use super::metrics::{ForgeDemandTransitionResult, ForgeTaskMetricStrategy};
use super::planner::{
    ForgeCapacity, ForgePlanCandidate, ForgePlanCapacity, ForgePlanner, ForgeTableSnapshot,
};
use super::worker::ForgeLifecycleEvent;
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
    /// Plans classified terminally outside every lane.
    pub unschedulable: usize,
    /// Ready rows whose persisted envelope exceeds this pod's governor capacity.
    pub unclaimable_tasks: usize,
    /// Maximum-minus-minimum admitted task count across this complete pass's tenants.
    pub fairness_lag_tasks: usize,
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
    /// Tasks terminalized because no configured lane can execute them.
    unschedulable: usize,
    /// Whether the exact observed demand generation was acknowledged.
    acknowledged: bool,
}

/// Concrete owner of fenced durable Forge planning and demand convergence.
pub struct ForgeScheduler<'forge> {
    /// Forge dependency owner used only for catalog reads and deterministic discovery.
    forge: &'forge Forge,
    /// Single pure planner used by hints and periodic roster repair.
    planner: ForgePlanner,
    /// Live governor-clamped capacity shared by candidate sizing and admission.
    capacity: ForgeCapacity,
    /// Durable task and demand owner.
    tasks: ForgeTasks,
    /// Stable scheduler lease owner for this process.
    owner: Uuid,
    /// Bounded demand page size.
    demand_cap: u32,
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
    /// Returns invalid configuration when derived capacity is not positive.
    pub fn new(forge: &'forge Forge) -> Result<Self, ForgeError> {
        Self::with_owner(forge, Uuid::now_v7())
    }

    /// Constructs a scheduler with a stable fixture owner across bounded passes.
    ///
    /// # Errors
    /// Returns invalid configuration when derived capacity is not positive.
    #[cfg(feature = "test-support")]
    pub fn with_owner_for_test(forge: &'forge Forge, owner: Uuid) -> Result<Self, ForgeError> {
        Self::with_owner(forge, owner)
    }

    /// Constructs the scheduler dependency graph for one explicit lease owner.
    ///
    /// # Errors
    /// Returns invalid configuration when derived capacity is not positive.
    fn with_owner(forge: &'forge Forge, owner: Uuid) -> Result<Self, ForgeError> {
        let config = &forge.core.config;
        let configured = ForgeCapacity::try_from(config)?;
        let governor = forge
            .core
            .resources
            .snapshot()
            .map_err(|error| ForgeError::Capacity {
                detail: error.to_string(),
            })?;
        let capacity = ForgeCapacity {
            max_files: configured.max_files,
            max_bytes: configured.max_bytes,
            max_parallelism: configured
                .max_parallelism
                .min(u16::try_from(governor.plan.effective_cpu).unwrap_or(u16::MAX)),
            max_memory_bytes: configured
                .max_memory_bytes
                .min(u64::try_from(governor.plan.elastic_memory_bytes).unwrap_or(u64::MAX)),
            max_spill_bytes: configured
                .max_spill_bytes
                .min(governor.plan.scratch_limit_bytes),
            max_large_task_bytes: configured.max_large_task_bytes,
        };
        Ok(Self {
            forge,
            planner: ForgePlanner::new(capacity),
            capacity,
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
        })
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
            "wyrd-redux",
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
        metrics::counter!("bifrost_forge_planning_demand_total", "source" => "hint").increment(1);
        Ok(())
    }

    /// Repairs lost hints from the authoritative roster and plans one bounded demand page.
    ///
    /// This method performs metadata reads and `PostgreSQL` writes only. It never
    /// invokes `DataFusion`, rewrites objects, or commits an Iceberg transaction.
    ///
    /// # Errors
    /// Returns scheduler lease, roster, catalog, planning, or durable SQL errors.
    /// Planning and cancellation failures retain the observed demand generation.
    /// A generation replaced during its atomic acknowledgement is refreshed and
    /// retried once, while keeping the pass incomplete so complete-only status
    /// is never published from a raced view.
    ///
    /// # Cancellation
    ///
    /// Caller cancellation returns an incomplete outcome before the next bounded
    /// roster or demand unit and skips later fence renewal or publication.
    /// Fence loss returns immediately without later enqueue, status, or metrics.
    pub async fn schedule_once(
        &self,
        stop: &CancellationToken,
    ) -> Result<ForgeScheduleOutcome, ForgeError> {
        let started = std::time::Instant::now();
        let Some(fence) = self.acquire_fence().await? else {
            return Ok(ForgeScheduleOutcome {
                standby: true,
                ..ForgeScheduleOutcome::default()
            });
        };
        self.renew_fence(fence).await?;
        let mut outcome = ForgeScheduleOutcome {
            incomplete: self.repair_roster(stop, fence).await?,
            ..ForgeScheduleOutcome::default()
        };
        #[cfg(feature = "test-support")]
        self.pause_before_demand_renewal_if_armed().await;
        self.renew_fence(fence).await?;
        let (demands, overflowed) = self
            .tasks
            .planning_demands(self.owner, fence, self.demand_cap)
            .await
            .map_err(ForgeError::Sql)?;
        outcome.incomplete |= overflowed;
        outcome.demands_seen = demands.len();
        let mut admitted_by_tenant = BTreeMap::<_, usize>::new();
        self.plan_demands(demands, fence, stop, &mut outcome, &mut admitted_by_tenant)
            .await?;
        outcome.fairness_lag_tasks = admitted_by_tenant
            .values()
            .min()
            .zip(admitted_by_tenant.values().max())
            .map_or(0, |(minimum, maximum)| maximum.saturating_sub(*minimum));
        if stop.is_cancelled() {
            outcome.incomplete = true;
            return Ok(outcome);
        }
        self.renew_fence(fence).await?;
        self.publish_status(&mut outcome, fence).await?;
        metrics::counter!("bifrost_forge_scheduling_total", "result" => if outcome.incomplete { "incomplete" } else { "complete" }).increment(1);
        metrics::counter!("bifrost_forge_unschedulable_total")
            .increment(outcome.unschedulable as u64);
        metrics::histogram!("bifrost_forge_scheduling_duration_seconds")
            .record(started.elapsed().as_secs_f64());
        #[cfg(feature = "test-support")]
        if !outcome.incomplete {
            self.complete_publications.fetch_add(1, Ordering::AcqRel);
        }
        Ok(outcome)
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
        fence: i64,
        stop: &CancellationToken,
        outcome: &mut ForgeScheduleOutcome,
        admitted_by_tenant: &mut BTreeMap<DataTenantId, usize>,
    ) -> Result<(), ForgeError> {
        for mut demand in demands {
            if stop.is_cancelled() {
                outcome.incomplete = true;
                break;
            }
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
                        outcome.unschedulable =
                            outcome.unschedulable.saturating_add(planned.unschedulable);
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
                        self.forge.core.telemetry.record_demand_transition(
                            ForgeDemandTransitionResult::GenerationChanged,
                        );
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
                        self.forge
                            .core
                            .telemetry
                            .record_demand_transition(ForgeDemandTransitionResult::Failed);
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
        fence: i64,
    ) -> Result<bool, ForgeError> {
        let (tables, failures) = self.forge.discover_tables().await?;
        for key in tables {
            if stop.is_cancelled() {
                return Ok(true);
            }
            self.renew_fence(fence).await?;
            let identity = ForgeTaskTableIdentity::new(
                "wyrd-redux",
                key.table_ref.namespace.as_str(),
                key.table_ref.name,
            )
            .map_err(ForgeError::Sql)?;
            self.tasks
                .upsert_periodic(key.tenant, &identity)
                .await
                .map_err(ForgeError::Sql)?;
        }
        Ok(failures > 0)
    }

    /// Plans and atomically acknowledges one exact demand generation.
    ///
    /// # Errors
    ///
    /// Returns identity, catalog, discovery, planning, lane, or SQL errors. A
    /// failure leaves the demand generation available to a later scheduler.
    async fn plan_demand(
        &self,
        demand: &ForgePlanningDemand,
        fence: i64,
    ) -> Result<DemandPlanningResult, ForgeError> {
        let snapshot = self.discover_snapshot(demand).await?;
        for candidate in &snapshot.candidates {
            self.forge.core.telemetry.record_discovered_candidate(
                ForgeTaskMetricStrategy::try_from(candidate.strategy).map_err(|strategy| {
                    ForgeError::Invariant {
                        detail: format!(
                            "Forge candidate strategy lacks a metric mapping: {strategy:?}"
                        ),
                    }
                })?,
                candidate.inputs.len(),
                candidate.bytes,
            );
        }
        let planned = self.planner.plan_table(&snapshot)?;
        let mut executable = Vec::new();
        let mut unschedulable = Vec::new();
        for task in planned.into_iter().take(1) {
            let terminal = task.capacity == ForgePlanCapacity::Unschedulable;
            let durable = NewForgeTask {
                data_tenant_id: demand.data_tenant_id,
                table_ref: demand.table_ref.clone(),
                strategy: task.strategy,
                lane: if terminal {
                    ForgeTaskLane::Ordinary
                } else {
                    task.lane()?
                },
                base_snapshot_id: task.base_snapshot_id,
                plan: task.plan,
                plan_hash: task.plan_hash,
                estimates: task.estimates,
                ready_at: Utc::now(),
            };
            if terminal {
                unschedulable.push(durable);
            } else {
                executable.push(durable);
            }
        }
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
                        unschedulable: &unschedulable,
                    },
                    unschedulable_event,
                )
                .await
                .map_err(ForgeError::Sql)?
        };
        let result = DemandPlanningResult {
            tasks_enqueued: usize::try_from(inserted).unwrap_or(usize::MAX),
            tasks_not_inserted: executable
                .len()
                .saturating_add(unschedulable.len())
                .saturating_sub(usize::try_from(inserted).unwrap_or(usize::MAX)),
            unschedulable: unschedulable.len(),
            acknowledged: true,
        };
        if result.acknowledged && result.tasks_enqueued == 0 {
            self.forge
                .core
                .telemetry
                .record_demand_transition(ForgeDemandTransitionResult::Drained);
        }
        if result.acknowledged
            && let Some(observer) = &self.forge.core.completion_observer
        {
            for task in &executable {
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
    /// Staging work retains priority. Live candidates are materialized only
    /// when no staging fold is ready, preserving one mutation per base snapshot.
    ///
    /// # Errors
    ///
    /// Returns identity, clock, catalog, discovery, or candidate-bound errors.
    async fn discover_snapshot(
        &self,
        demand: &ForgePlanningDemand,
    ) -> Result<ForgeTableSnapshot, ForgeError> {
        let binding = task_table_binding(
            demand.data_tenant_id,
            demand.data_tenant_id,
            &demand.table_ref,
        )?;
        let table = self.forge.load_table(&binding.table_ident()).await?;
        let current_day = self.forge.core.clock.now()?.date_naive();
        let discovered = self
            .forge
            .discover_live_rewrites(&binding, &table, current_day)
            .await?;
        let mut candidates = self
            .forge
            .discover_staging_task_candidates(&binding, current_day, self.capacity)
            .await?;
        if candidates.is_empty() {
            candidates = discovered
                .groups()
                .iter()
                .map(|group| {
                    ForgePlanCandidate::from_live_group(
                        group,
                        self.forge.core.config.max_concurrent_reads,
                        self.capacity,
                    )
                })
                .collect::<Result<Vec<_>, ForgeError>>()?;
        }
        // Independent per-table maintenance trigger. Evaluated every tick from
        // the current metadata, not from the presence of compaction work, so
        // snapshot expiry can never be starved by sustained compaction load.
        // When maintenance is due it leads the candidate list, taking this
        // tick's single planned slot ahead of compaction. Retention eligibility
        // is part of the due predicate so a successful no-op expiry cannot
        // create an endless successor-demand loop ahead of compaction.
        let maintenance_due = self.maintenance_due(&table, demand)?;
        if maintenance_due && let Some(candidate) = self.maintenance_candidate(&table).await? {
            candidates.insert(0, candidate);
        }
        Ok(ForgeTableSnapshot {
            snapshot_id: discovered.base_snapshot_id(),
            candidates,
        })
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
    fn maintenance_due(
        &self,
        table: &iceberg::table::Table,
        demand: &ForgePlanningDemand,
    ) -> Result<bool, ForgeError> {
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
    ) -> Result<Option<ForgePlanCandidate>, ForgeError> {
        let Some(snapshot) = table.metadata().current_snapshot() else {
            return Ok(None);
        };
        let manifests = table
            .manifest_list_reader(snapshot)
            .load()
            .await
            .map_err(ForgeError::Catalog)?;
        let mut inputs = Vec::new();
        let mut input_bytes = Vec::new();
        let mut bytes = 0_u64;
        for manifest in manifests
            .entries()
            .iter()
            .take(self.forge.core.config.max_files_per_tick)
        {
            let size =
                u64::try_from(manifest.manifest_length).map_err(|_| ForgeError::Invariant {
                    detail: "Iceberg manifest length is negative".to_owned(),
                })?;
            let Some(next) = bytes.checked_add(size) else {
                return Err(ForgeError::Invariant {
                    detail: "manifest maintenance byte estimate overflowed".to_owned(),
                });
            };
            if !inputs.is_empty() && next > self.forge.core.config.max_bytes_per_tick {
                break;
            }
            bytes = next;
            inputs.push(manifest.manifest_path.clone());
            input_bytes.push(size.max(1));
        }
        let mut input_terms = inputs.into_iter().zip(input_bytes).collect::<Vec<_>>();
        input_terms.sort_by(|left, right| left.0.cmp(&right.0));
        input_terms.dedup_by(|left, right| left.0 == right.0);
        let (inputs, input_bytes): (Vec<_>, Vec<_>) = input_terms.into_iter().unzip();
        if inputs.is_empty() {
            return Ok(None);
        }
        let estimate = bytes.max(1);
        // Parallelism is the concurrent-read width of the expiry, not the count
        // of manifests to retire, so bound it by `max_concurrent_reads`. Leaving
        // it equal to the manifest count would push any table whose retained
        // history exceeds `max_concurrent_reads` into the Unschedulable lane
        // (its input count is not one, so the large-singleton path never
        // applies), wedging deep-history tables. The full manifest window still
        // drives `inputs`, so successive expiries retire bounded slices and the
        // plan hash advances as history shrinks.
        let envelope = super::planner::ForgeTaskEnvelope::for_rewrite(
            estimate,
            inputs
                .len()
                .min(self.forge.core.config.max_concurrent_reads),
            self.capacity,
        );
        Ok(Some(ForgePlanCandidate {
            strategy: ForgeTaskStrategy::SnapshotExpiry,
            inputs,
            input_bytes,
            bytes: estimate,
            parallelism: envelope.reader_permits,
            memory_bytes: envelope.memory_bytes(),
            spill_bytes: envelope.scratch_bytes,
            parameters: serde_json::json!({
                "kind":"maintenance",
                "trigger_commit_count": table.metadata().snapshots().count()
                    .saturating_sub(self.forge.core.config.retain_last),
            }),
        }))
    }

    /// Publishes complete-only planning backlog gauges.
    ///
    /// # Errors
    ///
    /// Returns SQL errors while reading the bounded durable status page.
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
    ) -> Result<(), ForgeError> {
        if outcome.incomplete || outcome.demands_acknowledged != outcome.demands_seen {
            return Ok(());
        }
        let (backlog, oldest, overflowed) = self
            .tasks
            .planning_status(self.demand_cap)
            .await
            .map_err(ForgeError::Sql)?;
        let governor =
            self.forge
                .core
                .resources
                .snapshot()
                .map_err(|error| ForgeError::Capacity {
                    detail: error.to_string(),
                })?;
        let capacity = ForgeCapacity::try_from(&self.forge.core.config)?;
        let limits = ForgeClaimLimits {
            max_active_per_tenant: u32::MAX,
            lease_seconds: 1,
            max_files: capacity.max_files,
            max_bytes: capacity.max_bytes,
            max_parallelism: capacity
                .max_parallelism
                .min(u16::try_from(governor.plan.effective_cpu).unwrap_or(u16::MAX)),
            max_memory_bytes: capacity
                .max_memory_bytes
                .min(u64::try_from(governor.plan.elastic_memory_bytes).unwrap_or(u64::MAX)),
            max_spill_bytes: capacity
                .max_spill_bytes
                .min(governor.plan.scratch_limit_bytes),
            max_large_task_bytes: capacity.max_large_task_bytes,
        };
        let unclaimable_task_ids = self
            .tasks
            .unclaimable_ready_task_ids(limits)
            .await
            .map_err(ForgeError::Sql)?;
        outcome.unclaimable_tasks = unclaimable_task_ids.len();
        for task_id in unclaimable_task_ids {
            tracing::warn!(
                %task_id,
                "Forge ready tasks exceed the live pod governor envelope"
            );
        }
        self.renew_fence(fence).await?;
        if should_publish_gauges(outcome, overflowed) {
            metrics::gauge!("bifrost_forge_planning_backlog").set(exact_gauge(backlog));
            let age = oldest.map_or(0.0, |time| {
                Utc::now()
                    .signed_duration_since(time)
                    .to_std()
                    .unwrap_or_default()
                    .as_secs_f64()
            });
            metrics::gauge!("bifrost_forge_oldest_planning_demand_seconds").set(age);
            self.forge
                .core
                .telemetry
                .record_planning_status(Duration::from_secs_f64(age), outcome.fairness_lag_tasks);
        } else {
            outcome.incomplete = true;
        }
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
fn maintenance_trigger_due(
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

/// Converts a durable backlog count into an exact, monotonic gauge value.
#[must_use]
fn exact_gauge(value: u64) -> f64 {
    const MAX_EXACT_GAUGE_INTEGER: u64 = 1_u64 << 53;
    value
        .min(MAX_EXACT_GAUGE_INTEGER)
        .to_f64()
        .unwrap_or(9_007_199_254_740_992.0)
}

/// Returns whether one status observation may replace authoritative gauges.
#[must_use]
fn should_publish_gauges(outcome: &ForgeScheduleOutcome, status_overflowed: bool) -> bool {
    !outcome.incomplete
        && outcome.demands_acknowledged == outcome.demands_seen
        && !status_overflowed
}

/// Builds the exact audited terminal envelope after SQL chooses the task UUID.
fn unschedulable_event(task_id: Uuid) -> AuditEvent {
    AuditEvent::new(
        RequestId::now_v7(),
        None,
        "forge.task.unschedulable".to_owned(),
        format!("forge-task:{task_id}"),
        None,
        PrincipalId::new(Uuid::nil()),
        PrincipalKindTag::Service,
        AuthMethod::Internal,
        "bifrost:forge".to_owned(),
        AuditDecision::Allow,
        AuditResult::Success,
        "Forge plan exceeds configured capacity".to_owned(),
    )
}

#[cfg(test)]
mod source_tests {
    use std::time::Duration;

    use super::{ForgeScheduleOutcome, maintenance_trigger_due, should_publish_gauges};

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

    /// The supervised scheduler path contains no direct execution or commit call.
    #[test]
    fn scheduler_source_has_no_datafusion_or_iceberg_execution() {
        let source = include_str!("planning_scheduler.rs");
        assert!(!source.contains(concat!("run_one_live_", "replacement(")));
        assert!(!source.contains(concat!("run_targeted_", "compaction_for_table(")));
        assert!(!source.contains(concat!("commit_", "rewrite(")));
        assert!(!source.contains(concat!(".", "rewrite(")));
    }

    /// An incomplete pass cannot replace previously published authoritative gauges.
    #[test]
    fn incomplete_pass_preserves_previous_gauges() {
        let mut published = (7_u64, 11_u64);
        let incomplete = ForgeScheduleOutcome {
            demands_seen: 2,
            demands_acknowledged: 1,
            incomplete: true,
            ..ForgeScheduleOutcome::default()
        };
        if should_publish_gauges(&incomplete, false) {
            published = (0, 0);
        }
        assert_eq!(published, (7, 11));
        let complete = ForgeScheduleOutcome {
            demands_seen: 2,
            demands_acknowledged: 2,
            ..ForgeScheduleOutcome::default()
        };
        assert!(should_publish_gauges(&complete, false));
        assert!(!should_publish_gauges(&complete, true));
    }
}
