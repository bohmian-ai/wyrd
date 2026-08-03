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
use vala_sql::queries::forge_tasks::{ForgeEnqueueBatch, ForgeTasks};
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
use super::planner::{
    ForgeCapacity, ForgePlanCandidate, ForgePlanCapacity, ForgePlanner, ForgeTableSnapshot,
};
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
    /// Plans classified terminally outside every lane.
    pub unschedulable: usize,
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
        let capacity = ForgeCapacity::try_from(config)?;
        Ok(Self {
            forge,
            planner: ForgePlanner::new(capacity),
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
        let result = DemandPlanningResult {
            tasks_enqueued: executable.len().saturating_add(unschedulable.len()),
            unschedulable: unschedulable.len(),
            #[cfg(feature = "test-support")]
            acknowledged: {
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
            },
            #[cfg(not(feature = "test-support"))]
            acknowledged: self
                .tasks
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
                .map_err(ForgeError::Sql)?,
        };
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
            .discover_staging_task_candidates(&binding, current_day)
            .await?;
        if candidates.is_empty() {
            candidates = discovered
                .groups()
                .iter()
                .map(live_candidate)
                .collect::<Result<Vec<_>, ForgeError>>()?;
        }
        if candidates.is_empty()
            && let Some(candidate) = self.maintenance_candidate(&table).await?
        {
            candidates.push(candidate);
        }
        Ok(ForgeTableSnapshot {
            snapshot_id: discovered.base_snapshot_id(),
            candidates,
        })
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
        }
        inputs.sort();
        inputs.dedup();
        if inputs.is_empty() {
            return Ok(None);
        }
        let files = u16::try_from(inputs.len()).map_err(|_| ForgeError::Invariant {
            detail: "manifest maintenance parallelism exceeds u16".to_owned(),
        })?;
        let estimate = bytes.max(1);
        Ok(Some(ForgePlanCandidate {
            strategy: ForgeTaskStrategy::SnapshotExpiry,
            inputs,
            bytes: estimate,
            parallelism: files.max(1),
            memory_bytes: estimate,
            spill_bytes: estimate,
            parameters: serde_json::json!({"kind":"maintenance"}),
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

/// Maps one exact live rewrite group into the durable planner contract.
///
/// # Errors
///
/// Returns invariant errors when byte or parallelism estimates exceed their
/// typed bounds.
fn live_candidate(
    group: &super::right_size::IcebergRewriteGroup,
) -> Result<ForgePlanCandidate, ForgeError> {
    let mut inputs = group
        .files()
        .iter()
        .map(|file| file.catalog_path().to_owned())
        .collect::<Vec<_>>();
    inputs.sort();
    inputs.dedup();
    let bytes = group
        .files()
        .iter()
        .try_fold(0_u64, |total, file| {
            total.checked_add(file.file_size_bytes())
        })
        .ok_or_else(|| ForgeError::Invariant {
            detail: "Forge group bytes overflow".to_owned(),
        })?;
    Ok(ForgePlanCandidate {
        strategy: ForgeTaskStrategy::SmallFiles,
        parallelism: u16::try_from(group.files().len()).map_err(|_| ForgeError::Invariant {
            detail: "Forge planned parallelism exceeds u16".to_owned(),
        })?,
        memory_bytes: bytes,
        spill_bytes: bytes,
        inputs,
        bytes,
        parameters: serde_json::json!({"kind":"live_rewrite"}),
    })
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
    use super::{ForgeScheduleOutcome, should_publish_gauges};

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
