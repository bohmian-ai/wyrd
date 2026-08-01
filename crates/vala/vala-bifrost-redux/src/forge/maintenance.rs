//! Ordered Iceberg metadata maintenance for one claimed Forge task.

use std::collections::BTreeSet;
use std::sync::Arc;

use iceberg::table::Table;
use iceberg::transaction::{
    ExpiredFileSet, ManifestRewriteLimits, ManifestRewriteOutcome, ManifestRewriteSelection,
    rewrite_manifests,
};
use tokio_util::sync::CancellationToken;

use super::Forge;
use super::compact::ForgeTableKey;
use super::error::ForgeError;
use super::expire::PendingExpiryTerminal;
use super::lease::ForgeLease;
use super::orphan_gc::{GcProtectedPaths, OrphanGcOutcome};
use crate::catalog::TenantTableBinding;

/// Deterministic one-shot catalog boundaries for Forge integration tests.
#[cfg(feature = "test-support")]
#[derive(Clone, Default)]
pub struct MaintenanceTestControls {
    /// Manifest submission boundary.
    manifest: Arc<MaintenanceTestGate>,
    /// Expiry-lifecycle boundary before its first catalog submission.
    expiry_submit: Arc<MaintenanceTestGate>,
    /// Expiry accepted-response boundary.
    expiry_accepted: Arc<MaintenanceTestGate>,
}

/// One armed boundary with lossless arrival and release notifications.
#[cfg(feature = "test-support")]
#[derive(Default)]
struct MaintenanceTestGate {
    /// Whether the next boundary must pause.
    armed: std::sync::atomic::AtomicBool,
    /// Whether the armed boundary has arrived.
    arrived: std::sync::atomic::AtomicBool,
    /// Whether the test released the boundary.
    released: std::sync::atomic::AtomicBool,
    /// Wakeup for arrival waiters.
    arrival: tokio::sync::Notify,
    /// Wakeup for the paused production future.
    release: tokio::sync::Notify,
}

#[cfg(feature = "test-support")]
impl MaintenanceTestGate {
    /// Arms this boundary for one production arrival.
    fn arm(&self) {
        self.arrived
            .store(false, std::sync::atomic::Ordering::Release);
        self.released
            .store(false, std::sync::atomic::Ordering::Release);
        self.armed.store(true, std::sync::atomic::Ordering::Release);
    }

    /// Waits until the armed production future reaches this boundary.
    async fn wait(&self) {
        while !self.arrived.load(std::sync::atomic::Ordering::Acquire) {
            self.arrival.notified().await;
        }
    }

    /// Releases the production future held at this boundary.
    fn release(&self) {
        self.released
            .store(true, std::sync::atomic::Ordering::Release);
        self.release.notify_waiters();
    }

    /// Pauses one armed production arrival until release or cancellation.
    async fn pause(&self, stop: &CancellationToken) -> bool {
        if !self.armed.swap(false, std::sync::atomic::Ordering::AcqRel) {
            return false;
        }
        self.arrived
            .store(true, std::sync::atomic::Ordering::Release);
        self.arrival.notify_waiters();
        loop {
            if self.released.load(std::sync::atomic::Ordering::Acquire) {
                return false;
            }
            tokio::select! {
                () = self.release.notified() => {}
                () = stop.cancelled() => return true,
            }
        }
    }
}

#[cfg(feature = "test-support")]
impl MaintenanceTestControls {
    /// Pauses the expiry lifecycle before its first catalog submission when armed.
    pub(super) async fn pause_expiry_submission(&self, stop: &CancellationToken) -> bool {
        self.expiry_submit.pause(stop).await
    }

    /// Pauses after an accepted expiry response when armed.
    pub(super) async fn pause_expiry_accepted(&self, stop: &CancellationToken) -> bool {
        self.expiry_accepted.pause(stop).await
    }

    /// Arms and waits for the next manifest catalog submission.
    pub fn arm_manifest_submission(&self) {
        self.manifest.arm();
    }
    /// Waits for the armed manifest catalog submission.
    pub async fn wait_manifest_submission(&self) {
        self.manifest.wait().await;
    }
    /// Releases the armed manifest submission.
    pub fn release_manifest_submission(&self) {
        self.manifest.release();
    }
    /// Arms the next expiry lifecycle before its first catalog submission.
    pub fn arm_expiry_submission(&self) {
        self.expiry_submit.arm();
    }
    /// Waits for the armed expiry lifecycle boundary.
    pub async fn wait_expiry_submission(&self) {
        self.expiry_submit.wait().await;
    }
    /// Releases the armed expiry lifecycle boundary.
    pub fn release_expiry_submission(&self) {
        self.expiry_submit.release();
    }
    /// Arms the boundary after the catalog accepts an expiry commit.
    pub fn arm_expiry_accepted(&self) {
        self.expiry_accepted.arm();
    }
    /// Waits for the accepted expiry response.
    pub async fn wait_expiry_accepted(&self) {
        self.expiry_accepted.wait().await;
    }
    /// Releases the accepted expiry response boundary.
    pub fn release_expiry_accepted(&self) {
        self.expiry_accepted.release();
    }
}

/// Cohesive owner for ordered manifest, expiry, and orphan maintenance.
pub(super) struct ForgeMaintenance {
    /// Shared Forge dependencies used by every lifecycle stage.
    forge: Arc<Forge>,
}

/// Exact result returned before any expired object is deleted.
pub(super) struct ForgeMaintenanceResult {
    /// Current table after metadata commits.
    pub(super) table: Table,
    /// Metadata-derived candidates proven unreachable after expiry.
    pub(super) expired_files: ExpiredFileSet,
    /// Expiry operations closed only after task Prepared evidence commits.
    pub(super) expiry_terminals: Vec<PendingExpiryTerminal>,
}

impl ForgeMaintenance {
    /// Constructs a lifecycle owner over the worker's existing Forge graph.
    pub(super) fn new(forge: Arc<Forge>) -> Self {
        Self { forge }
    }

    /// Runs manifest rewrite, watermarked expiry, and never-published cleanup in order.
    ///
    /// Each externally visible stage retains the same table lease. A stale
    /// manifest selection causes no commit; expiry reloads current metadata and
    /// validates all active task watermarks before committing.
    ///
    /// # Errors
    ///
    /// Returns fencing, catalog, expiry, cleanup, object-store, SQL, audit, or
    /// cancellation failures. Completed earlier stages remain durable and a
    /// successor task safely resumes from current metadata.
    ///
    /// # Cancellation
    ///
    /// Cancellation stops before the next stage. Cancellation or timeout racing
    /// manifest submission requires a successor to reload metadata before retry;
    /// expiry commit uncertainty remains represented by its Prepared operation.
    pub(super) async fn execute(
        &self,
        lease: &mut ForgeLease,
        key: &ForgeTableKey,
        binding: &TenantTableBinding,
        table: Table,
        manifest_paths: &[String],
        stop: &CancellationToken,
    ) -> Result<ForgeMaintenanceResult, ForgeError> {
        require_running(stop)?;
        lease.require_fence(&self.forge.core.operator_pool).await?;
        if !lease.commit_window_fits(self.forge.core.config.commit_window()) {
            return Err(ForgeError::FenceLost {
                lease_key: lease.lease_key.clone(),
            });
        }
        #[cfg(feature = "test-support")]
        if self
            .forge
            .core
            .maintenance_controls
            .pause_expiry_submission(stop)
            .await
        {
            return Err(ForgeError::Reconciliation {
                detail:
                    "Iceberg expiry lifecycle was cancelled before its first catalog submission"
                        .to_owned(),
            });
        }
        let rewrite = rewrite_manifests(
            self.forge.core.catalog.as_ref(),
            &table,
            ManifestRewriteSelection {
                manifest_paths: manifest_paths.to_vec(),
            },
            ManifestRewriteLimits {
                max_manifests: self.forge.core.config.max_files_per_tick,
                max_entries: self.forge.core.config.max_files_per_tick,
                max_bytes: self.forge.core.config.max_bytes_per_tick,
            },
        );
        #[cfg(feature = "test-support")]
        if self
            .forge
            .core
            .maintenance_controls
            .manifest
            .pause(stop)
            .await
        {
            return Err(ForgeError::Reconciliation {
                detail: "Iceberg manifest rewrite was cancelled at submission with unknown acceptance; reload metadata before retry".to_owned(),
            });
        }
        tokio::pin!(rewrite);
        let rewritten = tokio::select! {
            response = tokio::time::timeout(
                self.forge.core.config.iceberg_total_retry_timeout,
                &mut rewrite,
            ) => match response {
                Ok(Ok(rewritten)) => rewritten,
                Ok(Err(error)) => return Err(ForgeError::Catalog(error)),
                Err(_) => return Err(ForgeError::Reconciliation {
                    detail: "Iceberg manifest rewrite timed out with unknown acceptance; reload metadata before retry"
                        .to_owned(),
                }),
            },
            () = stop.cancelled() => return Err(ForgeError::Reconciliation {
                detail: "Iceberg manifest rewrite was cancelled with unknown acceptance; reload metadata before retry"
                    .to_owned(),
            }),
        };
        if rewritten.outcome == ManifestRewriteOutcome::Stale {
            tracing::debug!("manifest rewrite selection became stale; reconciling expiry state");
        }
        lease.require_fence(&self.forge.core.operator_pool).await?;
        require_running(stop)?;
        let expiry = self
            .forge
            .run_snapshot_expiry_for_table(lease, key, binding, self.forge.core.clock.now()?, stop)
            .await?;
        require_running(stop)?;
        lease.require_fence(&self.forge.core.operator_pool).await?;
        let table = self.forge.load_table(&binding.table_ident()).await?;
        Ok(ForgeMaintenanceResult {
            table,
            expired_files: expiry.expired_files,
            expiry_terminals: expiry.terminals,
        })
    }

    /// Runs the disjoint never-published generation collector after expired cleanup.
    ///
    /// # Errors
    ///
    /// Returns lease, catalog, SQL, audit, object-store, or cancellation failures.
    ///
    /// # Cancellation
    ///
    /// Cancellation before collection starts no new scan or deletion effect.
    pub(super) async fn collect_never_published(
        &self,
        lease: &mut ForgeLease,
        key: &ForgeTableKey,
        binding: &TenantTableBinding,
        stop: &CancellationToken,
    ) -> Result<(), ForgeError> {
        require_running(stop)?;
        lease.require_fence(&self.forge.core.operator_pool).await?;
        let staging = BTreeSet::new();
        let live = BTreeSet::new();
        let _: OrphanGcOutcome = self
            .forge
            .run_orphan_gc_for_table(
                lease,
                key,
                binding,
                self.forge.core.clock.now()?,
                stop,
                GcProtectedPaths {
                    staging: &staging,
                    live: &live,
                },
            )
            .await?;
        Ok(())
    }
}

/// Rejects a maintenance stage before it starts when shared authority is cancelled.
///
/// # Errors
///
/// Returns [`ForgeError::Shutdown`] when claim, table-lease, or process authority was lost.
fn require_running(stop: &CancellationToken) -> Result<(), ForgeError> {
    if stop.is_cancelled() {
        Err(ForgeError::Shutdown)
    } else {
        Ok(())
    }
}
