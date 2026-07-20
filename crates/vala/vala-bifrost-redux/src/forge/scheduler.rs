use std::time::Duration;

use tokio::sync::watch;
use tokio::task::JoinHandle;
use uuid::Uuid;

use super::compact::{
    ForgeContext, ForgeTickOutcome, run_compaction_bins_for_table,
    run_compaction_reconciliation_for_table,
};
use super::error::ForgeError;
use super::expire::{discover_tables, run_snapshot_expiry_for_table};
use super::lease::{ForgeLease, forge_lease_key};
use super::orphan_gc::{build_live_set, run_orphan_gc_for_table};
use crate::catalog::TenantTableBinding;

pub struct ForgeScheduler {
    stop: watch::Sender<bool>,
    task: JoinHandle<()>,
}

impl ForgeScheduler {
    pub fn start(context: ForgeContext, interval: Duration) -> Result<Self, ForgeError> {
        if interval.is_zero() {
            return Err(ForgeError::InvalidConfig {
                detail: "Forge scheduler interval must be positive".to_owned(),
            });
        }
        let (stop, mut stopped) = watch::channel(false);
        let task = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        if let Err(error) = run_maintenance_tick_with_stop(&context, &stopped).await {
                            tracing::error!(error = %error, "Forge maintenance tick failed");
                        }
                    }
                    changed = stopped.changed() => {
                        if changed.is_err() || *stopped.borrow() {
                            break;
                        }
                    }
                }
            }
        });
        Ok(Self { stop, task })
    }

    pub async fn shutdown(self) -> Result<(), ForgeError> {
        self.stop.send(true).map_err(|_| ForgeError::Shutdown)?;
        self.task.await.map_err(|error| ForgeError::Reconciliation {
            detail: error.to_string(),
        })
    }
}

/// Run all Forge maintenance stages once.
pub async fn run_maintenance_tick(context: &ForgeContext) -> Result<ForgeTickOutcome, ForgeError> {
    let (_sender, stopped) = watch::channel(false);
    run_maintenance_tick_with_stop(context, &stopped).await
}

async fn run_maintenance_tick_with_stop(
    context: &ForgeContext,
    stopped: &watch::Receiver<bool>,
) -> Result<ForgeTickOutcome, ForgeError> {
    let owner = Uuid::now_v7();
    let mut outcome = ForgeTickOutcome::default();
    let mut tables = discover_tables(context).await?;
    tables.sort_by(|left, right| {
        left.tenant
            .cmp(&right.tenant)
            .then_with(|| left.table_ref.fqn().cmp(&right.table_ref.fqn()))
    });
    for key in tables {
        if *stopped.borrow() {
            break;
        }
        let binding =
            TenantTableBinding::resolve((key.tenant, key.table_ref.clone())).map_err(|error| {
                ForgeError::Group {
                    detail: error.to_string(),
                }
            })?;
        let lease_key =
            forge_lease_key(key.tenant, &binding.logical_namespace, &binding.table_name);
        let Some(mut lease) = ForgeLease::acquire(
            &context.operator_pool,
            lease_key,
            owner,
            context.config.lease_ttl,
        )
        .await?
        else {
            outcome.bins_skipped += 1;
            continue;
        };

        // One fence covers reconciliation -> compaction -> expiry -> live-set
        // rebuild -> GC for this physical table.
        outcome.reconciled +=
            run_compaction_reconciliation_for_table(context, &mut lease, &key, &binding).await?;
        if *stopped.borrow() {
            release_lease(context, &lease).await?;
            break;
        }

        let compaction = run_compaction_bins_for_table(context, &mut lease, &key, &binding).await?;
        outcome.groups_seen += compaction.groups_seen;
        outcome.bins_committed += compaction.bins_committed;
        outcome.bins_skipped += compaction.bins_skipped;
        outcome.reconciled += compaction.reconciled;
        if *stopped.borrow() {
            release_lease(context, &lease).await?;
            break;
        }

        outcome.reconciled +=
            run_snapshot_expiry_for_table(context, &mut lease, &key, &binding).await?;
        if *stopped.borrow() {
            release_lease(context, &lease).await?;
            break;
        }

        let table = super::compact::load_table(context, &binding.table_ident()).await?;
        let _live_set = build_live_set(context, &key, &binding, &table).await?;
        if *stopped.borrow() {
            release_lease(context, &lease).await?;
            break;
        }
        outcome.reconciled += run_orphan_gc_for_table(context, &mut lease, &key, &binding).await?;
        release_lease(context, &lease).await?;
    }
    Ok(outcome)
}

async fn release_lease(context: &ForgeContext, lease: &ForgeLease) -> Result<(), ForgeError> {
    if !lease.release(&context.operator_pool).await? {
        tracing::warn!(lease_key = %lease.lease_key, "Forge lease release lost its fence");
    }
    Ok(())
}
