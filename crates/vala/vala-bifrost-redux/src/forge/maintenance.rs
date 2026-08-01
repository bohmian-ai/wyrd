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
    pub(super) async fn execute(
        &self,
        lease: &mut ForgeLease,
        key: &ForgeTableKey,
        binding: &TenantTableBinding,
        table: Table,
        manifest_paths: &[String],
        _stop: &CancellationToken,
    ) -> Result<ForgeMaintenanceResult, ForgeError> {
        lease.require_fence(&self.forge.core.operator_pool).await?;
        if !lease.commit_window_fits(self.forge.core.config.commit_window()) {
            return Err(ForgeError::FenceLost {
                lease_key: lease.lease_key.clone(),
            });
        }
        let rewritten = tokio::time::timeout(
            self.forge.core.config.iceberg_total_retry_timeout,
            rewrite_manifests(
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
            ),
        )
        .await
        .map_err(|_| ForgeError::Timeout {
            operation: "Iceberg manifest rewrite commit",
        })?
        .map_err(ForgeError::Catalog)?;
        if rewritten.outcome == ManifestRewriteOutcome::Stale {
            tracing::debug!("manifest rewrite selection became stale; reconciling expiry state");
        }
        lease.require_fence(&self.forge.core.operator_pool).await?;
        let expiry = self
            .forge
            .run_snapshot_expiry_for_table(lease, key, binding, self.forge.core.clock.now()?)
            .await?;
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
    pub(super) async fn collect_never_published(
        &self,
        lease: &mut ForgeLease,
        key: &ForgeTableKey,
        binding: &TenantTableBinding,
        stop: &CancellationToken,
    ) -> Result<(), ForgeError> {
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
