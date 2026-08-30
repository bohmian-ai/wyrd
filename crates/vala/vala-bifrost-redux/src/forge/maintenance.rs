//! Ordered Iceberg metadata maintenance for one claimed Forge task.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

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
use super::metrics::ForgeMetricStage;
use crate::catalog::TenantTableBinding;

/// One already-filtered data manifest considered by the shared rewrite planner.
///
/// This deliberately carries only the manifest-rewrite planning
/// inputs so scheduling and execution cannot drift in their completed-bin
/// predicate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ManifestRewriteCandidate {
    /// Immutable manifest object identity.
    pub(super) path: String,
    /// Manifest object size in bytes.
    pub(super) size_bytes: u64,
    /// Partition-spec boundary for independent bins.
    pub(super) partition_spec_id: i32,
    /// Manifest-list order used to prefer oldest work.
    pub(super) sequence_number: i64,
}

/// Selects at most one worthwhile oldest-first manifest bin per partition spec.
///
/// This is the synchronous Wyrd form of
/// `gc.rs::plan_manifest_rewrite`: a completed bin and a final bin both must
/// reduce at least two manifests to one. A final bin is useful only when it
/// reaches the target bytes or the configured minimum count.
#[must_use]
pub(super) fn select_manifest_rewrite_paths(
    candidates: &[ManifestRewriteCandidate],
    target_size_bytes: u64,
    min_count_to_merge: usize,
) -> Vec<String> {
    debug_assert!(target_size_bytes > 0, "manifest target must be positive");
    debug_assert!(
        min_count_to_merge > 0,
        "manifest minimum count must be positive"
    );

    let mut by_spec = BTreeMap::<i32, Vec<&ManifestRewriteCandidate>>::new();
    for candidate in candidates {
        if candidate.size_bytes < target_size_bytes {
            by_spec
                .entry(candidate.partition_spec_id)
                .or_default()
                .push(candidate);
        }
    }

    let mut selected = Vec::new();
    for candidates in by_spec.values_mut() {
        candidates.sort_by_key(|candidate| candidate.sequence_number);
        let mut current_bin = Vec::new();
        let mut current_bytes = 0_u64;
        let mut completed_bin = None;
        for candidate in candidates {
            let next_bytes = current_bytes.saturating_add(candidate.size_bytes);
            if !current_bin.is_empty() && next_bytes > target_size_bytes {
                if current_bin.len() >= 2 {
                    completed_bin = Some(std::mem::take(&mut current_bin));
                    break;
                }
                current_bin.clear();
                current_bytes = 0;
            }
            current_bin.push(*candidate);
            current_bytes = current_bytes.saturating_add(candidate.size_bytes);
        }
        let chosen = if let Some(completed) = completed_bin {
            completed
        } else if current_bin.len() >= 2
            && (current_bytes >= target_size_bytes || current_bin.len() >= min_count_to_merge)
        {
            current_bin
        } else {
            continue;
        };
        selected.extend(chosen.into_iter().map(|candidate| candidate.path.clone()));
    }
    selected.sort_unstable();
    selected
}

/// Selects one complete worthwhile bin that fits the task's file and byte bounds.
///
/// Preserving the bin as the unit of admission prevents an interleaved manifest
/// list from truncating two valid per-spec bins into two non-rewritable
/// singletons. When no whole bin fits, the scheduler must leave demand pending.
#[must_use]
pub(super) fn select_bounded_manifest_rewrite_paths(
    candidates: &[ManifestRewriteCandidate],
    target_size_bytes: u64,
    min_count_to_merge: usize,
    max_files: usize,
    max_bytes: u64,
) -> Vec<String> {
    let selected = select_manifest_rewrite_paths(candidates, target_size_bytes, min_count_to_merge);
    let selected = selected
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    let mut by_spec = BTreeMap::<i32, Vec<&ManifestRewriteCandidate>>::new();
    for candidate in candidates {
        if selected.contains(&candidate.path) {
            by_spec
                .entry(candidate.partition_spec_id)
                .or_default()
                .push(candidate);
        }
    }
    for bin in by_spec.values_mut() {
        bin.sort_by_key(|candidate| candidate.sequence_number);
        let bytes = bin.iter().try_fold(0_u64, |sum, candidate| {
            sum.checked_add(candidate.size_bytes)
        });
        if bin.len() <= max_files && bytes.is_some_and(|bytes| bytes <= max_bytes) {
            return bin.iter().map(|candidate| candidate.path.clone()).collect();
        }
    }
    Vec::new()
}

/// Returns whether one manifest-rewrite pass has publishable work.
///
/// The scheduler uses this before considering snapshot expiry, so the
/// maintenance trigger follows the same selection rules as the eventual
/// catalog rewrite instead of borrowing expiry's retained-snapshot predicate.
#[must_use]
pub(super) fn manifest_rewrite_is_due(
    candidates: &[ManifestRewriteCandidate],
    target_size_bytes: u64,
    min_count_to_merge: usize,
) -> bool {
    !select_manifest_rewrite_paths(candidates, target_size_bytes, min_count_to_merge).is_empty()
}

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
    ///
    /// The `Notified` future is pinned and enabled (registered against the
    /// `arrival` notify) BEFORE the final `arrived` re-check, so a `pause` that
    /// stores `arrived` and calls `notify_waiters` in the window between the
    /// check and the await cannot be a lost wakeup.
    async fn wait(&self) {
        loop {
            let notified = self.arrival.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.arrived.load(std::sync::atomic::Ordering::Acquire) {
                return;
            }
            notified.await;
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

/// The exact table, plan inputs, and due-work flags for one maintenance pass.
///
/// The dispatcher derives all six values from one validated claim, so they
/// travel together rather than as six positional parameters in which the two
/// adjacent booleans would be silently transposable.
pub(super) struct ForgeMaintenanceRequest<'a> {
    /// Identity of the table being maintained.
    pub(super) key: &'a ForgeTableKey,
    /// Tenant and table binding authorizing this pass.
    pub(super) binding: &'a TenantTableBinding,
    /// Loaded Iceberg table this pass commits against.
    pub(super) table: Table,
    /// Manifest paths selected by the claimed plan.
    pub(super) manifest_paths: &'a [String],
    /// Whether this pass must rewrite manifests.
    pub(super) manifest_rewrite_due: bool,
    /// Whether this pass must expire snapshots.
    pub(super) snapshot_expiry_due: bool,
}

impl ForgeMaintenance {
    /// Constructs a lifecycle owner over the worker's existing Forge graph.
    pub(super) fn new(forge: Arc<Forge>) -> Self {
        Self { forge }
    }

    /// Selects the bounded set of manifests this pass may rewrite.
    ///
    /// Only data manifests the claimed plan already named are eligible, so a
    /// pass can never widen its own scope from live table metadata. A table with
    /// no current snapshot selects nothing. The final bounding applies the
    /// tick's file and byte ceilings, so one pass cannot rewrite an unbounded
    /// amount of metadata.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Catalog`] when the current snapshot's manifest list
    /// cannot be loaded.
    ///
    /// # Panics
    ///
    /// Panics if a manifest reports a negative length after being filtered to
    /// nonnegative lengths, which would mean the metadata changed mid-read.
    async fn select_rewrite_paths(
        &self,
        table: &Table,
        manifest_paths: &[String],
    ) -> Result<Vec<String>, ForgeError> {
        let Some(snapshot) = table.metadata().current_snapshot() else {
            return Ok(Vec::new());
        };
        let manifests = table
            .manifest_list_reader(snapshot)
            .load()
            .await
            .map_err(ForgeError::Catalog)?;
        let candidates = manifests
            .entries()
            .iter()
            .filter(|manifest| {
                manifest.content == iceberg::spec::ManifestContentType::Data
                    && manifest_paths.contains(&manifest.manifest_path)
                    && manifest.manifest_length >= 0
            })
            .map(|manifest| ManifestRewriteCandidate {
                path: manifest.manifest_path.clone(),
                size_bytes: u64::try_from(manifest.manifest_length)
                    .expect("nonnegative manifest length fits u64"),
                partition_spec_id: manifest.partition_spec_id,
                sequence_number: manifest.sequence_number,
            })
            .collect::<Vec<_>>();
        Ok(select_bounded_manifest_rewrite_paths(
            &candidates,
            self.forge.core.config.manifest_rewrite_target_size_bytes,
            self.forge.core.config.manifest_rewrite_min_count,
        ))
    }

    /// Submits one bounded manifest rewrite under the tick's retry timeout.
    ///
    /// Submission acceptance is unknowable once the request is in flight, so
    /// both the timeout and cancellation paths return a reconciliation error
    /// naming that uncertainty: a successor must reload metadata before it
    /// retries rather than assuming the rewrite did not land. A stale selection
    /// is not an error — the expiry stage that follows reconciles it.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Catalog`] when the catalog rejects the rewrite, and
    /// [`ForgeError::Reconciliation`] when the submission times out or is
    /// cancelled with unknown acceptance.
    ///
    /// # Cancellation
    ///
    /// Cancellation racing submission leaves acceptance unknown and is reported
    /// as a reconciliation error, never as a clean stop.
    async fn submit_manifest_rewrite(
        &self,
        table: &Table,
        rewrite_paths: Vec<String>,
        stop: &CancellationToken,
    ) -> Result<(), ForgeError> {
        let rewrite = rewrite_manifests(
            self.forge.core.catalog.as_ref(),
            table,
            ManifestRewriteSelection {
                manifest_paths: rewrite_paths,
            },
            ManifestRewriteLimits {
                max_bytes: self.forge.core.config.manifest_rewrite_target_size_bytes,
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
        Ok(())
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
    /// The `stop` token is the caller's authority-only token, not its shutdown
    /// token: it is cancelled solely on genuine authority loss (claim-heartbeat
    /// failure or lease-renew loss), so cancellation observed here means fence
    /// loss and nothing else. A graceful shutdown does not cancel this operation
    /// — it runs to its own outcome, bounded by `iceberg_total_retry_timeout`.
    /// Cancellation stops before the next stage. Cancellation or timeout racing
    /// manifest submission requires a successor to reload metadata before retry;
    /// expiry commit uncertainty remains represented by its Prepared operation.
    pub(super) async fn execute(
        &self,
        lease: &mut ForgeLease,
        request: ForgeMaintenanceRequest<'_>,
        stop: &CancellationToken,
    ) -> Result<ForgeMaintenanceResult, ForgeError> {
        let ForgeMaintenanceRequest {
            key,
            binding,
            table,
            manifest_paths,
            manifest_rewrite_due,
            snapshot_expiry_due,
        } = request;
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
        if manifest_rewrite_due && self.forge.core.config.manifest_rewrite_enabled {
            let rewrite_paths = self.select_rewrite_paths(&table, manifest_paths).await?;
            if !rewrite_paths.is_empty() {
                self.submit_manifest_rewrite(&table, rewrite_paths, stop)
                    .await?;
            }
        }
        lease.require_fence(&self.forge.core.operator_pool).await?;
        require_running(stop)?;
        let (expired_files, expiry_terminals) =
            if snapshot_expiry_due && self.forge.core.config.snapshot_expiry_enabled {
                let expiry = self
                    .forge
                    .run_snapshot_expiry_for_table(
                        lease,
                        key,
                        binding,
                        self.forge.core.clock.now()?,
                        stop,
                    )
                    .await?;
                (expiry.expired_files, expiry.terminals)
            } else {
                (ExpiredFileSet::default(), Vec::new())
            };
        require_running(stop)?;
        lease.require_fence(&self.forge.core.operator_pool).await?;
        let table = self.forge.load_table(&binding.table_ident()).await?;
        Ok(ForgeMaintenanceResult {
            table,
            expired_files,
            expiry_terminals,
        })
    }

    /// Runs the disjoint never-published generation collector after expired cleanup.
    ///
    /// The bounded orphan-GC run's outcome is consumed for telemetry: the run's
    /// wall-clock duration and success/failure are recorded against the
    /// `OrphanGc` maintenance stage, and its deleted, retained, and Partial
    /// accounting is recorded through the orphan cleanup surface. Recording
    /// happens on both the success and error paths so a failed run still moves
    /// its stage-failure series before the error propagates.
    ///
    /// # Errors
    ///
    /// Returns lease, catalog, SQL, audit, object-store, or cancellation failures.
    ///
    /// # Cancellation
    ///
    /// Cancellation before collection starts no new scan or deletion effect.
    #[tracing::instrument(skip_all, fields(tenant = %key.tenant))]
    pub(super) async fn collect_never_published(
        &self,
        lease: &mut ForgeLease,
        key: &ForgeTableKey,
        binding: &TenantTableBinding,
        stop: &CancellationToken,
    ) -> Result<(), ForgeError> {
        require_running(stop)?;
        lease.require_fence(&self.forge.core.operator_pool).await?;
        let now = self.forge.core.clock.now()?;
        let started = Instant::now();
        let result = self
            .forge
            .run_orphan_gc_for_table(lease, key, binding, now, stop)
            .await;
        let elapsed = started.elapsed();
        self.forge.core.telemetry.record_stage(
            ForgeMetricStage::OrphanGc,
            elapsed,
            result.is_err(),
        );
        let outcome = result?;
        self.forge
            .core
            .telemetry
            .record_orphan_gc(&outcome, elapsed);
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

#[cfg(test)]
mod tests {
    use super::{
        ManifestRewriteCandidate, manifest_rewrite_is_due, select_bounded_manifest_rewrite_paths,
        select_manifest_rewrite_paths,
    };

    fn candidate(
        path: &str,
        size_bytes: u64,
        spec: i32,
        sequence: i64,
    ) -> ManifestRewriteCandidate {
        ManifestRewriteCandidate {
            path: path.to_owned(),
            size_bytes,
            partition_spec_id: spec,
            sequence_number: sequence,
        }
    }

    /// Applies the completed-bin and final-bin reduction predicates.
    #[test]
    fn manifest_selector_requires_two_manifests_and_selects_completed_bins() {
        assert!(select_manifest_rewrite_paths(&[candidate("only", 99, 0, 0)], 100, 1).is_empty());
        assert!(
            select_manifest_rewrite_paths(
                &[candidate("a", 99, 0, 0), candidate("b", 99, 0, 1)],
                100,
                100,
            )
            .is_empty()
        );
        assert_eq!(
            select_manifest_rewrite_paths(
                &[candidate("a", 50, 0, 0), candidate("b", 50, 0, 1)],
                100,
                100,
            ),
            ["a", "b"]
        );
        assert_eq!(
            select_manifest_rewrite_paths(
                &[
                    candidate("old-a", 40, 0, 1),
                    candidate("old-b", 40, 0, 2),
                    candidate("overflow", 40, 0, 3),
                ],
                100,
                100,
            ),
            ["old-a", "old-b"]
        );
    }

    /// Each partition spec independently retains one oldest worthwhile bin.
    #[test]
    fn manifest_selector_groups_specs_and_observes_minimum_count() {
        assert_eq!(
            select_manifest_rewrite_paths(
                &[
                    candidate("a", 40, 0, 2),
                    candidate("b", 40, 0, 1),
                    candidate("other-a", 40, 1, 1),
                    candidate("other-b", 40, 1, 2),
                ],
                100,
                2,
            ),
            ["a", "b", "other-a", "other-b"]
        );
        assert!(
            select_manifest_rewrite_paths(&[candidate("underfilled", 40, 0, 1)], 100, 2,)
                .is_empty()
        );
    }

    /// Interleaved specs retain one whole bin across task file and byte bounds.
    #[test]
    fn bounded_manifest_selector_never_splits_interleaved_specs() {
        let candidates = [
            candidate("spec-zero-old", 40, 0, 1),
            candidate("spec-one-old", 40, 1, 1),
            candidate("spec-zero-new", 40, 0, 2),
            candidate("spec-one-new", 40, 1, 2),
        ];
        assert_eq!(
            select_bounded_manifest_rewrite_paths(&candidates, 100, 2, 2, 100),
            ["spec-zero-old", "spec-zero-new"]
        );
        assert!(
            select_bounded_manifest_rewrite_paths(&candidates, 100, 2, 1, 100).is_empty(),
            "demand remains unacknowledged when no complete bin fits"
        );
    }

    /// Manifest eligibility is independent of retained-snapshot expiry state.
    #[test]
    fn manifest_rewrite_due_uses_its_own_completed_bin_predicate() {
        let candidates = [candidate("old-a", 50, 0, 1), candidate("old-b", 50, 0, 2)];
        assert!(manifest_rewrite_is_due(&candidates, 100, 2));
        assert!(!manifest_rewrite_is_due(&candidates[..1], 100, 2));
    }
}
