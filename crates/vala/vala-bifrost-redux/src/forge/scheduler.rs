use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::compact::{
    ForgeCore, ForgeTableKey, ForgeTickOutcome, run_compaction_bins_for_table,
    run_compaction_reconciliation_for_table,
};
use super::error::ForgeError;
use super::expire::{discover_tables, run_snapshot_expiry_for_table};
use super::lease::{ForgeLease, forge_lease_key};
use super::orphan_gc::{build_live_set, run_orphan_gc_for_table};
use super::rewrite::ForgeRewriteRuntime;
use crate::catalog::TenantTableBinding;
use crate::maintenance::StagingFileInbox;

/// Construction-time dependency graph for one Forge handle.
pub struct ForgeBuildConfig {
    /// SQL handle used by Forge stages.
    pub vala: vala_sql::ValaPostgres,
    /// Cross-tenant operator pool.
    pub operator_pool: vala_sql::OperatorPool,
    /// Iceberg catalog.
    pub catalog: Arc<dyn iceberg::Catalog>,
    /// Raw staging operator.
    pub staging: Arc<opendal::Operator>,
    /// Object-store seam used by rewrites and GC.
    pub object_store: Arc<dyn super::compact::ForgeObjectStore>,
    /// DataFusion runtime owned by the Forge process.
    pub rewrite_runtime: ForgeRewriteRuntime,
    /// Bounded advisory Scribe wake-up inbox.
    pub hints: StagingFileInbox,
    /// Forge limits.
    pub config: super::compact::ForgeConfig,
    /// Delayed periodic maintenance interval.
    pub maintenance_interval: Duration,
}

/// The single stateful Forge maintenance handle.
pub struct Forge {
    context: Arc<ForgeCore>,
    interval: Duration,
    hints: tokio::sync::Mutex<StagingFileInbox>,
    tick: tokio::sync::Mutex<()>,
    running: AtomicBool,
}

impl Forge {
    /// Construct Forge and validate its complete dependency graph.
    pub fn new(config: ForgeBuildConfig) -> Result<Self, ForgeError> {
        if config.maintenance_interval.is_zero() {
            return Err(ForgeError::InvalidConfig {
                detail: "Forge scheduler interval must be positive".to_owned(),
            });
        }
        config.config.validate()?;
        if config.rewrite_runtime.spill_limit_bytes() != config.config.spill_limit_bytes {
            return Err(ForgeError::InvalidConfig {
                detail: "rewrite runtime spill limit must match Forge config".to_owned(),
            });
        }
        let core = ForgeCore::new(
            config.vala,
            config.operator_pool,
            config.catalog,
            config.staging,
            config.config,
        )?
        .with_object_store(config.object_store);
        Ok(Self {
            context: Arc::new(core),
            interval: config.maintenance_interval,
            hints: tokio::sync::Mutex::new(config.hints),
            tick: tokio::sync::Mutex::new(()),
            running: AtomicBool::new(false),
        })
    }

    /// Run until `shutdown` is cancelled.
    pub async fn run(&self, shutdown: CancellationToken) -> Result<(), ForgeError> {
        if self
            .running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(ForgeError::AlreadyRunning);
        }
        let _guard = RunGuard(&self.running);
        let mut ticker =
            tokio::time::interval_at(tokio::time::Instant::now() + self.interval, self.interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                () = shutdown.cancelled() => return Ok(()),
                _ = ticker.tick() => {
                    match self.run_tick(&shutdown).await {
                        Ok(outcome) => tracing::debug!(
                            groups_seen = outcome.groups_seen,
                            bins_committed = outcome.bins_committed,
                            reconciliation_recovered = outcome.reconciliation_recovered,
                            expiry_reconciled = outcome.expiry_reconciled,
                            gc_reconciled = outcome.gc_reconciled,
                            gc_deleted = outcome.gc_deleted,
                            gc_skipped = outcome.gc_skipped,
                            budget_skips = outcome.budget_skips,
                            lease_contention = outcome.lease_contention,
                            fence_losses = outcome.fence_losses,
                            stage_failures = outcome.stage_failures,
                            tables_succeeded = outcome.tables_succeeded,
                            tables_skipped = outcome.tables_skipped,
                            tables_failed = outcome.tables_failed,
                            "Forge maintenance tick completed"
                        ),
                        Err(error) => tracing::error!(error = %error, "Forge maintenance tick failed"),
                    }
                }
            }
        }
    }

    /// Execute one serialized full maintenance tick.
    pub async fn run_once(&self) -> Result<ForgeTickOutcome, ForgeError> {
        self.run_tick(&CancellationToken::new()).await
    }

    async fn run_tick(&self, stop: &CancellationToken) -> Result<ForgeTickOutcome, ForgeError> {
        let _tick = self.tick.lock().await;
        run_maintenance_tick_with_stop(&self.context, stop).await
    }
}

struct RunGuard<'a>(&'a AtomicBool);
impl Drop for RunGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// Run all Forge maintenance stages once.
/// Discover and process one complete maintenance tick.
///
/// Discovery and deterministic ordering happen once per tick. Each discovered
/// table is then handed to [`process_table`], so binding errors, lease
/// contention, stage failures, and release failures affect only that table.
/// A database failure while discovering the table set remains a tick-level
/// error because no safe ordered work set exists in that case.
async fn run_maintenance_tick_with_stop(
    context: &ForgeCore,
    stopped: &CancellationToken,
) -> Result<ForgeTickOutcome, ForgeError> {
    let owner = Uuid::now_v7();
    let mut outcome = ForgeTickOutcome::default();
    let (mut tables, discovery_failures) = discover_tables(context).await?;
    outcome.tables_failed = outcome.tables_failed.saturating_add(discovery_failures);
    tables.sort_by(|left, right| {
        left.tenant
            .cmp(&right.tenant)
            .then_with(|| left.table_ref.fqn().cmp(&right.table_ref.fqn()))
    });
    for key in tables {
        if stopped.is_cancelled() {
            break;
        }
        match process_table(context, &key, owner, stopped, &mut outcome).await {
            TableRunStatus::Succeeded => outcome.tables_succeeded += 1,
            TableRunStatus::Skipped => {
                outcome.tables_skipped += 1;
                if stopped.is_cancelled() {
                    break;
                }
            }
            TableRunStatus::Failed => outcome.tables_failed += 1,
        }
    }
    Ok(outcome)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TableRunStatus {
    Succeeded,
    Skipped,
    Failed,
}

/// Resolve, lease, execute, and release one physical table.
///
/// This is the table-level fault-isolation boundary. Every path after lease
/// acquisition attempts a fenced release, including cancellation and stage
/// failure; callers receive a small status instead of a partially updated
/// outcome so the tick can continue with the next ordered table.
async fn process_table(
    context: &ForgeCore,
    key: &ForgeTableKey,
    owner: Uuid,
    stopped: &CancellationToken,
    outcome: &mut ForgeTickOutcome,
) -> TableRunStatus {
    let binding = match TenantTableBinding::resolve((key.tenant, key.table_ref.clone())) {
        Ok(binding) => binding,
        Err(error) => {
            tracing::error!(
                tenant = %key.tenant,
                table = %key.table_ref.fqn(),
                error = %error,
                "Forge table binding failed"
            );
            return TableRunStatus::Failed;
        }
    };
    let lease_key = forge_lease_key(key.tenant, &binding.logical_namespace, &binding.table_name);
    let mut lease = match ForgeLease::acquire(
        &context.operator_pool,
        lease_key,
        owner,
        context.config.lease_ttl,
    )
    .await
    {
        Ok(Some(lease)) => lease,
        Ok(None) => {
            outcome.lease_contention += 1;
            return TableRunStatus::Skipped;
        }
        Err(error) => {
            tracing::error!(
                tenant = %key.tenant,
                table = %key.table_ref.fqn(),
                error = %error,
                "Forge table lease acquisition failed"
            );
            return TableRunStatus::Failed;
        }
    };

    // Keep release outside the stage future so every post-acquisition path
    // converges through the same cleanup decision.
    let table_result = run_table_stages(context, &mut lease, key, &binding, stopped, outcome).await;
    let release_result = release_lease(context, &lease).await;
    match (table_result, release_result) {
        (Ok(stop_after), Ok(())) => {
            if stop_after {
                TableRunStatus::Skipped
            } else {
                TableRunStatus::Succeeded
            }
        }
        (Err(ForgeError::Shutdown), Ok(())) => TableRunStatus::Skipped,
        (Err(error), Ok(())) => {
            outcome.stage_failures += 1;
            if matches!(error, ForgeError::FenceLost { .. }) {
                outcome.fence_losses += 1;
            }
            tracing::error!(
                tenant = %key.tenant,
                table = %key.table_ref.fqn(),
                error = %error,
                "Forge table maintenance failed; continuing"
            );
            TableRunStatus::Failed
        }
        (Ok(_), Err(release_error)) => {
            tracing::error!(
                tenant = %key.tenant,
                table = %key.table_ref.fqn(),
                error = %release_error,
                "Forge table lease release failed; continuing"
            );
            TableRunStatus::Failed
        }
        (Err(error), Err(release_error)) => {
            outcome.stage_failures += 1;
            if matches!(error, ForgeError::FenceLost { .. }) {
                outcome.fence_losses += 1;
            }
            tracing::warn!(
                error = %release_error,
                lease_key = %lease.lease_key,
                "Forge stage failed and lease release also failed"
            );
            tracing::error!(error = %error, "Forge table maintenance failed");
            TableRunStatus::Failed
        }
    }
}

/// Execute the ordered maintenance stages for one leased physical table.
///
/// The sequence is reconciliation, compaction, snapshot expiry, one complete
/// retained live-set build, and orphan GC. Cancellation is checked between
/// stages so no new stage begins after shutdown is requested. The live set is
/// passed into GC rather than rebuilt there, while GC still performs its final
/// per-object reference recheck before deletion.
///
/// This function deliberately does not release `lease`: the caller owns the
/// release boundary so success, cancellation, stage failure, and release
/// failure all converge through the same conditional cleanup path.
async fn run_table_stages(
    context: &ForgeCore,
    lease: &mut ForgeLease,
    key: &super::compact::ForgeTableKey,
    binding: &TenantTableBinding,
    stopped: &CancellationToken,
    outcome: &mut ForgeTickOutcome,
) -> Result<bool, ForgeError> {
    // One fence covers reconciliation -> compaction -> expiry -> live-set
    // rebuild -> GC for this physical table.
    let reconciliation =
        run_compaction_reconciliation_for_table(context, lease, key, binding).await?;
    outcome.reconciliation_recovered += reconciliation;
    outcome.reconciled += reconciliation;
    if stopped.is_cancelled() {
        return Ok(true);
    }

    let compaction = run_compaction_bins_for_table(context, lease, key, binding).await?;
    outcome.groups_seen += compaction.groups_seen;
    outcome.bins_committed += compaction.bins_committed;
    outcome.bins_skipped += compaction.bins_skipped;
    outcome.budget_skips += compaction.budget_skips;
    outcome.reconciled += compaction.reconciled;
    if stopped.is_cancelled() {
        return Ok(true);
    }

    let expiry = run_snapshot_expiry_for_table(context, lease, key, binding).await?;
    outcome.expiry_reconciled += expiry;
    outcome.reconciled += expiry;
    if stopped.is_cancelled() {
        return Ok(true);
    }

    let table = super::compact::load_table(context, &binding.table_ident()).await?;
    let live_set = build_live_set(context, key, binding, &table).await?;
    if stopped.is_cancelled() {
        return Ok(true);
    }
    let gc = run_orphan_gc_for_table(context, lease, key, binding, &live_set).await?;
    outcome.gc_reconciled += gc.recovered;
    outcome.gc_deleted += gc.deleted;
    outcome.gc_skipped += gc.skipped;
    outcome.reconciled += gc.recovered;
    Ok(false)
}

async fn release_lease(context: &ForgeCore, lease: &ForgeLease) -> Result<(), ForgeError> {
    let released = tokio::time::timeout(
        context.config.catalog_request_timeout,
        lease.release(&context.operator_pool),
    )
    .await
    .map_err(|_| ForgeError::Timeout {
        operation: "Forge lease release",
    })??;
    if !released {
        tracing::warn!(lease_key = %lease.lease_key, "Forge lease release lost its fence");
    }
    Ok(())
}
