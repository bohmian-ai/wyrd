//! Durable demand scheduling without rewrite or Iceberg commit execution.

use std::collections::BTreeMap;

use chrono::Utc;
use num_traits::ToPrimitive;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use vala_sql::queries::forge_tasks::{ForgeEnqueueBatch, ForgeTasks};
use vala_sql::row_types::forge_tasks::{
    ForgePlanningDemand, ForgeTaskLane, ForgeTaskStrategy, ForgeTaskTableIdentity, NewForgeTask,
};
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
    /// Complete hard capacity declaration.
    capacity: ForgeCapacity,
    /// Stable scheduler lease owner for this process.
    owner: Uuid,
    /// Bounded demand page size.
    demand_cap: u32,
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
        let capacity = ForgeCapacity {
            max_files: u32::try_from(config.max_files_per_tick).map_err(|_| {
                ForgeError::InvalidConfig {
                    detail: "Forge file capacity exceeds u32".to_owned(),
                }
            })?,
            max_bytes: config.max_bytes_per_tick,
            max_parallelism: u16::try_from(config.max_concurrent_reads).map_err(|_| {
                ForgeError::InvalidConfig {
                    detail: "Forge parallelism exceeds u16".to_owned(),
                }
            })?,
            max_memory_bytes: config.max_memory_bytes,
            max_spill_bytes: config.spill_limit_bytes,
            max_large_task_bytes: config.max_large_task_bytes,
        }
        .validate()?;
        Ok(Self {
            forge,
            planner: ForgePlanner::new(),
            tasks: ForgeTasks::new(),
            capacity,
            owner,
            demand_cap: u32::try_from(config.max_hints_per_wake).unwrap_or(u32::MAX),
        })
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
    pub async fn schedule_once(
        &self,
        stop: &CancellationToken,
    ) -> Result<ForgeScheduleOutcome, ForgeError> {
        let started = std::time::Instant::now();
        let fence = self.acquire_fence().await?;
        let mut outcome = ForgeScheduleOutcome {
            incomplete: self.repair_roster(stop).await?,
            ..ForgeScheduleOutcome::default()
        };
        let (demands, overflowed) = self
            .tasks
            .planning_demands(
                &self.forge.core.operator_pool,
                self.owner,
                fence,
                self.demand_cap,
            )
            .await
            .map_err(ForgeError::Sql)?;
        outcome.incomplete |= overflowed;
        outcome.demands_seen = demands.len();
        let mut admitted_by_tenant = BTreeMap::<_, usize>::new();
        for demand in demands {
            if stop.is_cancelled() {
                outcome.incomplete = true;
                break;
            }
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
                }
                Err(error) => {
                    outcome.incomplete = true;
                    tracing::warn!(data_tenant_id = %demand.data_tenant_id, table = %demand.table_ref.table, error = %error, "Forge demand planning failed; retaining demand and continuing tenant page");
                }
            }
        }
        outcome.fairness_lag_tasks = admitted_by_tenant
            .values()
            .min()
            .zip(admitted_by_tenant.values().max())
            .map_or(0, |(minimum, maximum)| maximum.saturating_sub(*minimum));
        self.publish_status(&mut outcome).await?;
        metrics::counter!("bifrost_forge_scheduling_total", "result" => if outcome.incomplete { "incomplete" } else { "complete" }).increment(1);
        metrics::counter!("bifrost_forge_unschedulable_total")
            .increment(outcome.unschedulable as u64);
        metrics::histogram!("bifrost_forge_scheduling_duration_seconds")
            .record(started.elapsed().as_secs_f64());
        Ok(outcome)
    }

    /// Acquires the singleton planning fence for this scheduler owner.
    ///
    /// # Errors
    ///
    /// Returns invalid configuration, SQL, or fence-loss errors.
    async fn acquire_fence(&self) -> Result<i64, ForgeError> {
        let lease_seconds =
            u32::try_from(self.forge.core.config.lease_ttl.as_secs()).map_err(|_| {
                ForgeError::InvalidConfig {
                    detail: "scheduler lease TTL exceeds u32 seconds".to_owned(),
                }
            })?;
        self.tasks
            .acquire_scheduler(&self.forge.core.operator_pool, self.owner, lease_seconds)
            .await
            .map_err(ForgeError::Sql)?
            .ok_or_else(|| ForgeError::FenceLost {
                lease_key: "forge:scheduler:v1".to_owned(),
            })
    }

    /// Repairs periodic demand from the authoritative registered-table roster.
    ///
    /// The returned flag is true when discovery was partial or cancellation
    /// interrupted the roster before every table was upserted.
    ///
    /// # Errors
    ///
    /// Returns roster, identity, or SQL errors without acknowledging demand.
    async fn repair_roster(&self, stop: &CancellationToken) -> Result<bool, ForgeError> {
        let (tables, failures) = self.forge.discover_tables().await?;
        for key in tables {
            if stop.is_cancelled() {
                return Ok(true);
            }
            let identity = ForgeTaskTableIdentity::new(
                "wyrd-redux",
                key.table_ref.namespace.as_str(),
                key.table_ref.name,
            )
            .map_err(ForgeError::Sql)?;
            self.tasks
                .upsert_periodic(&self.forge.core.operator_pool, key.tenant, &identity)
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
        let planned = self.planner.plan_table(&snapshot, &self.capacity)?;
        let mut executable = Vec::new();
        let mut unschedulable = Vec::new();
        for task in planned.into_iter().take(1) {
            let terminal = task.capacity == ForgePlanCapacity::Unschedulable;
            let durable = NewForgeTask {
                data_tenant_id: demand.data_tenant_id,
                table_ref: demand.table_ref.clone(),
                strategy: task.strategy,
                lane: if terminal {
                    ForgeTaskLane::LargeSingleton
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
            acknowledged: self
                .tasks
                .enqueue_and_acknowledge(
                    &self.forge.core.operator_pool,
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
            && let Some(candidate) = maintenance_candidate(
                &table,
                self.forge.core.config.max_files_per_tick,
                self.forge.core.config.max_bytes_per_tick,
            )
            .await?
        {
            candidates.push(candidate);
        }
        Ok(ForgeTableSnapshot {
            snapshot_id: discovered.base_snapshot_id(),
            candidates,
        })
    }

    /// Publishes complete-only planning backlog gauges.
    ///
    /// # Errors
    ///
    /// Returns SQL errors while reading the bounded durable status page.
    async fn publish_status(&self, outcome: &mut ForgeScheduleOutcome) -> Result<(), ForgeError> {
        if outcome.incomplete || outcome.demands_acknowledged != outcome.demands_seen {
            return Ok(());
        }
        let (backlog, oldest, overflowed) = self
            .tasks
            .planning_status(&self.forge.core.operator_pool, self.demand_cap)
            .await
            .map_err(ForgeError::Sql)?;
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
            metrics::gauge!("bifrost_forge_oldest_backlog_seconds").set(age);
            metrics::gauge!("bifrost_forge_fairness_lag_tasks")
                .set(exact_gauge(outcome.fairness_lag_tasks as u64));
            metrics::counter!("bifrost_forge_complete_gauge_publications_total").increment(1);
        } else {
            outcome.incomplete = true;
        }
        Ok(())
    }
}

/// Builds one bounded lifecycle task from the current manifest list.
///
/// The exact manifest identities are the rewrite selection. Expiry and cleanup
/// remain useful even when only one manifest exists, so every non-empty table
/// receives one deterministic lifecycle task per base snapshot.
///
/// # Errors
///
/// Returns catalog or invariant failures when current metadata cannot be read
/// or its selected manifest sizes exceed representable bounds.
async fn maintenance_candidate(
    table: &iceberg::table::Table,
    max_manifests: usize,
    max_bytes: u64,
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
    for manifest in manifests.entries().iter().take(max_manifests) {
        let size = u64::try_from(manifest.manifest_length).map_err(|_| ForgeError::Invariant {
            detail: "Iceberg manifest length is negative".to_owned(),
        })?;
        let Some(next) = bytes.checked_add(size) else {
            return Err(ForgeError::Invariant {
                detail: "manifest maintenance byte estimate overflowed".to_owned(),
            });
        };
        if !inputs.is_empty() && next > max_bytes {
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
