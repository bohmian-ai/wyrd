//! Durable demand scheduling without rewrite or Iceberg commit execution.

use chrono::Utc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use vala_sql::queries::forge_tasks::ForgeTasks;
use vala_sql::row_types::forge_tasks::{
    ForgeTaskLane, ForgeTaskStrategy, ForgeTaskTableIdentity, NewForgeTask,
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
    /// Whether the bounded page or any demand remained incomplete.
    pub incomplete: bool,
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
            max_memory_bytes: config.max_bytes_per_tick,
            max_spill_bytes: config.spill_limit_bytes,
            max_large_task_bytes: config.max_large_task_bytes,
        }
        .validate()?;
        Ok(Self {
            forge,
            planner: ForgePlanner::new(),
            tasks: ForgeTasks::new(),
            capacity,
            owner: Uuid::now_v7(),
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
    /// This method performs metadata reads and PostgreSQL writes only. It never
    /// invokes DataFusion, rewrites objects, or commits an Iceberg transaction.
    ///
    /// # Errors
    /// Returns scheduler lease, roster, catalog, planning, or durable SQL errors.
    /// Planning and cancellation failures retain the observed demand generation.
    pub async fn schedule_once(
        &self,
        stop: &CancellationToken,
    ) -> Result<ForgeScheduleOutcome, ForgeError> {
        let started = std::time::Instant::now();
        let lease_seconds =
            u32::try_from(self.forge.core.config.lease_ttl.as_secs()).map_err(|_| {
                ForgeError::InvalidConfig {
                    detail: "scheduler lease TTL exceeds u32 seconds".to_owned(),
                }
            })?;
        let fence = self
            .tasks
            .acquire_scheduler(&self.forge.core.operator_pool, self.owner, lease_seconds)
            .await
            .map_err(ForgeError::Sql)?
            .ok_or_else(|| ForgeError::FenceLost {
                lease_key: "forge:scheduler:v1".to_owned(),
            })?;
        let (tables, failures) = self.forge.discover_tables().await?;
        let mut outcome = ForgeScheduleOutcome {
            incomplete: failures > 0,
            ..ForgeScheduleOutcome::default()
        };
        for key in tables {
            if stop.is_cancelled() {
                outcome.incomplete = true;
                return Ok(outcome);
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
        for demand in demands {
            if stop.is_cancelled() {
                outcome.incomplete = true;
                break;
            }
            let demand_result: Result<(), ForgeError> = async {
                let binding = task_table_binding(
                    demand.data_tenant_id,
                    demand.data_tenant_id,
                    &demand.table_ref,
                )?;
                let table = self.forge.load_table(&binding.table_ident()).await?;
                let mut tenant_conn = self
                    .forge
                    .core
                    .vala
                    .tenant_conn(demand.data_tenant_id)
                    .await
                    .map_err(ForgeError::Sql)?;
                let staged = vala_sql::queries::file_list::list_nonterminal_files(
                    &mut tenant_conn,
                    &demand.table_ref.namespace,
                    &demand.table_ref.table,
                )
                .await
                .map_err(ForgeError::Sql)?;
                tenant_conn.commit().await.map_err(ForgeError::Sql)?;
                let discovered = self
                    .forge
                    .discover_live_rewrites(
                        &binding,
                        &table,
                        self.forge.core.clock.now()?.date_naive(),
                    )
                    .await?;
                let mut candidates = Vec::new();
                if !staged.is_empty() {
                    let bytes = staged
                        .iter()
                        .try_fold(0_u64, |total, (_, size)| total.checked_add(*size))
                        .ok_or_else(|| ForgeError::Invariant {
                            detail: "Forge staging bytes overflow".to_owned(),
                        })?;
                    candidates.push(ForgePlanCandidate {
                        strategy: ForgeTaskStrategy::StagingFold,
                        inputs: staged.into_iter().map(|(path, _)| path).collect(),
                        bytes,
                        parallelism: 1,
                        memory_bytes: bytes,
                        spill_bytes: bytes,
                        parameters: serde_json::json!({"kind":"staging_fold"}),
                    });
                }
                candidates.extend(
                    discovered
                        .groups()
                        .iter()
                        .map(|group| {
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
                                parallelism: u16::try_from(group.files().len()).map_err(|_| {
                                    ForgeError::Invariant {
                                        detail: "Forge planned parallelism exceeds u16".to_owned(),
                                    }
                                })?,
                                memory_bytes: bytes,
                                spill_bytes: bytes,
                                inputs,
                                bytes,
                                parameters: serde_json::json!({"kind":"live_rewrite"}),
                            })
                        })
                        .collect::<Result<Vec<_>, ForgeError>>()?,
                );
                let planned = self.planner.plan_table(
                    &ForgeTableSnapshot {
                        snapshot_id: discovered.base_snapshot_id(),
                        candidates,
                    },
                    &self.capacity,
                )?;
                outcome.unschedulable = outcome.unschedulable.saturating_add(
                    planned
                        .iter()
                        .filter(|task| task.capacity == ForgePlanCapacity::Unschedulable)
                        .count(),
                );
                let mut executable = Vec::new();
                let mut unschedulable = Vec::new();
                for task in planned {
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
                outcome.tasks_enqueued = outcome
                    .tasks_enqueued
                    .saturating_add(executable.len())
                    .saturating_add(unschedulable.len());
                if self
                    .tasks
                    .enqueue_and_acknowledge(
                        &self.forge.core.operator_pool,
                        self.owner,
                        fence,
                        &demand,
                        &executable,
                        &unschedulable,
                        unschedulable_event,
                    )
                    .await
                    .map_err(ForgeError::Sql)?
                {
                    outcome.demands_acknowledged = outcome.demands_acknowledged.saturating_add(1);
                } else {
                    outcome.incomplete = true;
                }
                Ok(())
            }
            .await;
            if let Err(error) = demand_result {
                outcome.incomplete = true;
                tracing::warn!(data_tenant_id = %demand.data_tenant_id, table = %demand.table_ref.table, error = %error, "Forge demand planning failed; retaining demand and continuing tenant page");
            }
        }
        if !outcome.incomplete && outcome.demands_acknowledged == outcome.demands_seen {
            let (backlog, oldest, overflowed) = self
                .tasks
                .planning_status(&self.forge.core.operator_pool, self.demand_cap)
                .await
                .map_err(ForgeError::Sql)?;
            if !should_publish_gauges(&outcome, overflowed) {
                outcome.incomplete = true;
            } else {
                metrics::gauge!("bifrost_forge_planning_backlog").set(backlog as f64);
                let age = oldest.map_or(0.0, |time| {
                    Utc::now()
                        .signed_duration_since(time)
                        .to_std()
                        .unwrap_or_default()
                        .as_secs_f64()
                });
                metrics::gauge!("bifrost_forge_oldest_planning_demand_seconds").set(age);
            }
        }
        metrics::counter!("bifrost_forge_scheduling_total", "result" => if outcome.incomplete { "incomplete" } else { "complete" }).increment(1);
        metrics::counter!("bifrost_forge_unschedulable_total")
            .increment(outcome.unschedulable as u64);
        metrics::histogram!("bifrost_forge_scheduling_duration_seconds")
            .record(started.elapsed().as_secs_f64());
        Ok(outcome)
    }
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
