//! Fixed-cardinality operational telemetry for Forge maintenance.

use std::collections::BTreeMap;
use std::time::Duration;

use metrics::{Counter, Gauge, Histogram};
use num_traits::ToPrimitive;
use uuid::Uuid;
use wyrd_spec::vala::api::StoragePath;

use crate::catalog::TenantTableBinding;

use super::error::ForgeError;
use super::path::catalog_path_to_object_key;
use super::{Forge, ForgeScheduleOutcome};

/// Validated object-key contract for one rewrite source.
#[derive(Debug, Clone, Copy)]
pub(super) enum RewritePathContract<'binding> {
    /// Live Iceberg audit paths are catalog URIs rooted at the table location.
    Catalog {
        /// Validated physical tenant/table binding that owns the catalog root.
        binding: &'binding TenantTableBinding,
        /// Catalog table location used by the retained-manifest observation.
        table_location: &'binding str,
    },
}

impl RewritePathContract<'_> {
    /// Converts one typed rewrite path into a binding-validated object key.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when a staging path escapes its
    /// binding or a catalog URI does not belong to the validated table root.
    fn object_key(self, store: &opendal::Operator, path: &str) -> Result<String, ForgeError> {
        match self {
            Self::Catalog {
                binding,
                table_location,
            } => catalog_path_to_object_key(table_location, binding, store, path),
        }
    }
}

/// Durable-data source that produced a Forge operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum ForgeMetricSource {
    /// Server-side staging file-list state.
    Staging,
    /// Current Iceberg snapshot state.
    Iceberg,
}

impl ForgeMetricSource {
    /// Returns the only metric label value emitted for this source.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Staging => "staging",
            Self::Iceberg => "iceberg",
        }
    }
}

/// Closed maintenance stage inventory used by duration and failure series.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum ForgeMetricStage {
    /// Reconcile staging audit operations.
    ReconcileStaging,
    /// Reconcile current-snapshot replacement operations.
    ReconcileIceberg,
    /// Fold staging files into Iceberg.
    StagingFold,
    /// Discover current-snapshot rewrite groups.
    ManifestDiscovery,
    /// Replace one current-snapshot group.
    IcebergRewrite,
    /// Reconcile and expire old snapshots.
    SnapshotExpiry,
    /// Reconcile and remove proven orphan objects.
    OrphanGc,
}

impl ForgeMetricStage {
    /// Returns the only metric label value emitted for this stage.
    const fn as_str(self) -> &'static str {
        match self {
            Self::ReconcileStaging => "reconcile_staging",
            Self::ReconcileIceberg => "reconcile_iceberg",
            Self::StagingFold => "staging_fold",
            Self::ManifestDiscovery => "manifest_discovery",
            Self::IcebergRewrite => "iceberg_rewrite",
            Self::SnapshotExpiry => "snapshot_expiry",
            Self::OrphanGc => "orphan_gc",
        }
    }
}

/// Closed strategy inventory for authoritative Forge catalog commits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ForgeCatalogCommitStrategy {
    /// A staged-file fold publishes its replacement output.
    StagingFold,
    /// A live small-file rewrite replaces current Iceberg data files.
    SmallFiles,
}

impl ForgeCatalogCommitStrategy {
    /// Return the stable span value for this catalog commit strategy.
    const fn as_str(self) -> &'static str {
        match self {
            Self::StagingFold => "staging_fold",
            Self::SmallFiles => "small_files",
        }
    }
}

/// Closed terminal classification for staging and Iceberg operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum ForgeOperationResult {
    /// A new durable catalog effect committed.
    Committed,
    /// A previously uncertain effect was proven committed.
    Recovered,
    /// A prepared operation was proven abandoned and reset.
    Reset,
    /// No durable work was necessary.
    Noop,
    /// The tick-wide budget deferred work.
    Budget,
    /// The owner lost its lease fence.
    FenceLost,
    /// The owning operation returned an error.
    Failed,
}

impl ForgeOperationResult {
    /// Returns the only metric label value emitted for this result.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Committed => "committed",
            Self::Recovered => "recovered",
            Self::Reset => "reset",
            Self::Noop => "noop",
            Self::Budget => "budget",
            Self::FenceLost => "fence_lost",
            Self::Failed => "failed",
        }
    }
}

/// Closed lease-boundary result inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum ForgeLeaseResult {
    /// Another owner retains a live lease.
    Contention,
    /// An expired different owner was replaced.
    Takeover,
    /// A formerly held lease fence was lost.
    FenceLost,
}

impl ForgeLeaseResult {
    /// Returns the only metric label value emitted for this result.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Contention => "contention",
            Self::Takeover => "takeover",
            Self::FenceLost => "fence_lost",
        }
    }
}

/// Exact object counts and bytes for one terminal replacement.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct ForgeRewriteVolume {
    /// Exact replaced input object count.
    pub(super) input_files: usize,
    /// Exact replaced input bytes.
    pub(super) input_bytes: u64,
    /// Exact committed output object count.
    pub(super) output_files: usize,
    /// Exact committed output bytes.
    pub(super) output_bytes: u64,
}

/// Registered Forge metric handles retained by one Forge owner.
pub struct ForgeTelemetry {
    /// Complete scheduler-owned oldest-backlog publication.
    oldest_backlog: Gauge,
    /// Complete scheduler-owned fairness-lag publication.
    fairness_lag: Gauge,
    /// Count of complete scheduler gauge publications.
    complete_gauge_publications: Counter,
    /// Terminal operation counters for every source/result combination.
    operations: BTreeMap<(ForgeMetricSource, ForgeOperationResult), Counter>,
    /// Lease-boundary counters for every closed result.
    lease_events: BTreeMap<ForgeLeaseResult, Counter>,
    /// Failure counters for every owning stage.
    stage_failures: BTreeMap<ForgeMetricStage, Counter>,
    /// Duration histograms for every owning stage.
    stage_seconds: BTreeMap<ForgeMetricStage, Histogram>,
    /// Committed/recovered replacement input-file counters.
    rewrite_input_files: BTreeMap<ForgeMetricSource, Counter>,
    /// Committed/recovered replacement input-byte counters.
    rewrite_input_bytes: BTreeMap<ForgeMetricSource, Counter>,
    /// Committed/recovered replacement output-file counters.
    rewrite_output_files: BTreeMap<ForgeMetricSource, Counter>,
    /// Committed/recovered replacement output-byte counters.
    rewrite_output_bytes: BTreeMap<ForgeMetricSource, Counter>,
    /// One terminal duration series for each closed strategy/result pair.
    task_duration: BTreeMap<(&'static str, &'static str), Histogram>,
    /// Final spill observations partitioned by closed Forge strategy.
    task_spill: BTreeMap<&'static str, Histogram>,
    /// Conflict counters emitted exactly at durable rejection boundaries.
    conflicts: BTreeMap<&'static str, Counter>,
    /// Completed cleanup obligation duration by closed cleanup provenance.
    cleanup_duration: BTreeMap<&'static str, Histogram>,
    /// Scheduler pass duration retained by the injected production handle.
    scheduler_duration: Histogram,
}

impl ForgeTelemetry {
    /// Registers the complete fixed-cardinality Forge metric inventory.
    #[must_use]
    pub fn new() -> Self {
        Self {
            oldest_backlog: metrics::gauge!("bifrost_forge_oldest_backlog_seconds"),
            fairness_lag: metrics::gauge!("bifrost_forge_fairness_lag_tasks"),
            complete_gauge_publications: metrics::counter!(
                "bifrost_forge_complete_gauge_publications_total"
            ),
            operations: operation_counters(),
            lease_events: lease_counters(),
            stage_failures: stage_failure_counters(),
            stage_seconds: stage_histograms(),
            rewrite_input_files: source_counters("bifrost_forge_rewrite_input_files_total"),
            rewrite_input_bytes: source_counters("bifrost_forge_rewrite_input_bytes_total"),
            rewrite_output_files: source_counters("bifrost_forge_rewrite_output_files_total"),
            rewrite_output_bytes: source_counters("bifrost_forge_rewrite_output_bytes_total"),
            task_duration: task_duration_histograms(),
            task_spill: strategy_histograms("bifrost_forge_task_spill_bytes"),
            conflicts: conflict_counters(),
            cleanup_duration: cleanup_histograms(),
            scheduler_duration: metrics::histogram!("bifrost_forge_scheduler_duration_seconds"),
        }
    }

    /// Record one finished scheduler pass through the production metric owner.
    ///
    /// Complete-only gauges remain published at the scheduler's authoritative
    /// roster-status query rather than inferred from this aggregate outcome.
    pub(crate) fn record_scheduler_pass(&self, _outcome: &ForgeScheduleOutcome, elapsed: Duration) {
        self.scheduler_duration.record(elapsed.as_secs_f64());
    }

    /// Record one terminal worker attempt with closed strategy and result labels.
    pub(crate) fn record_task_terminal(&self, strategy: &str, result: &str, elapsed: Duration) {
        let Some(strategy) = closed_strategy(strategy) else {
            return;
        };
        let Some(result) = closed_task_result(result) else {
            return;
        };
        if let Some(histogram) = self.task_duration.get(&(strategy, result)) {
            histogram.record(elapsed.as_secs_f64());
        }
    }

    /// Record final `DataFusion` spill accounting for one completed Forge attempt.
    pub(crate) fn record_task_spill(&self, strategy: &str, bytes: u64) {
        if let Some(strategy) = closed_strategy(strategy)
            && let Some(histogram) = self.task_spill.get(strategy)
        {
            histogram.record(bytes.to_f64().unwrap_or(f64::MAX));
        }
    }

    /// Record one exact durable conflict before its caller returns the rejection.
    pub(crate) fn record_conflict(&self, kind: &'static str) {
        if let Some(counter) = self.conflicts.get(kind) {
            counter.increment(1);
        }
    }

    /// Record completion of one durable cleanup obligation.
    pub(crate) fn record_cleanup(&self, kind: &'static str, elapsed: Duration) {
        if let Some(histogram) = self.cleanup_duration.get(kind) {
            histogram.record(elapsed.as_secs_f64());
        }
    }

    /// Publish the complete scheduler-owned backlog and fairness state.
    ///
    /// This boundary is called only after the scheduler has read a complete
    /// durable status page, so neither gauge can expose a partial roster.
    pub(crate) fn record_planning_status(&self, backlog_age: Duration, fairness_lag: usize) {
        self.oldest_backlog.set(backlog_age.as_secs_f64());
        self.fairness_lag
            .set(fairness_lag.to_f64().unwrap_or(f64::MAX));
        self.complete_gauge_publications.increment(1);
    }

    /// Construct the shared closed-schema span for one catalog commit future.
    ///
    /// Only the strategy, result, role, and scrubbed durable task UUIDs are
    /// owner-authored. Tenant, table, SQL, object-path, and error details must
    /// never be added by callers.
    pub(super) fn catalog_commit_span(
        strategy: ForgeCatalogCommitStrategy,
        task_identity: Option<(Uuid, Uuid)>,
    ) -> tracing::Span {
        let span = tracing::info_span!(
            "bifrost.forge.catalog.commit",
            strategy = strategy.as_str(),
            result = tracing::field::Empty,
            role = "forge_worker",
            task_id = tracing::field::Empty,
            attempt_id = tracing::field::Empty,
        );
        if let Some((task_id, attempt_id)) = task_identity {
            span.record("task_id", tracing::field::display(task_id));
            span.record("attempt_id", tracing::field::display(attempt_id));
        }
        span
    }

    /// Records one typed terminal operation event.
    pub(super) fn record_operation(
        &self,
        source: ForgeMetricSource,
        result: ForgeOperationResult,
        count: usize,
    ) {
        self.operations[&(source, result)].increment(count as u64);
    }

    /// Records one typed lease-boundary event.
    pub(super) fn record_lease(&self, result: ForgeLeaseResult) {
        self.lease_events[&result].increment(1);
    }

    /// Records one owning-stage return and whether it failed.
    pub(super) fn record_stage(&self, stage: ForgeMetricStage, elapsed: Duration, failed: bool) {
        self.stage_seconds[&stage].record(elapsed.as_secs_f64());
        if failed {
            self.stage_failures[&stage].increment(1);
        }
    }

    /// Records committed replacement volume from an authoritative outcome.
    pub(super) fn record_rewrite_volume(
        &self,
        source: ForgeMetricSource,
        input_files: usize,
        input_bytes: u64,
        output_files: usize,
        output_bytes: u64,
    ) {
        self.rewrite_input_files[&source].increment(input_files as u64);
        self.rewrite_input_bytes[&source].increment(input_bytes);
        self.rewrite_output_files[&source].increment(output_files as u64);
        self.rewrite_output_bytes[&source].increment(output_bytes);
    }
}

impl Default for ForgeTelemetry {
    /// Register the standard production Forge metric inventory.
    fn default() -> Self {
        Self::new()
    }
}

/// Convert a durable strategy tag to the fixed telemetry label vocabulary.
fn closed_strategy(strategy: &str) -> Option<&'static str> {
    match strategy {
        "staging_fold" => Some("staging_fold"),
        "small_files" => Some("small_files"),
        "snapshot_expiry" => Some("snapshot_expiry"),
        _ => None,
    }
}

/// Convert an execution result to the fixed telemetry label vocabulary.
fn closed_task_result(result: &str) -> Option<&'static str> {
    match result {
        "succeeded" => Some("succeeded"),
        "retryable" => Some("retryable"),
        "failed" => Some("failed"),
        "cancelled" => Some("cancelled"),
        "unschedulable" => Some("unschedulable"),
        _ => None,
    }
}

/// Registers every closed strategy/result task-duration series eagerly.
fn task_duration_histograms() -> BTreeMap<(&'static str, &'static str), Histogram> {
    let mut histograms = BTreeMap::new();
    for strategy in ["staging_fold", "small_files", "snapshot_expiry"] {
        for result in [
            "succeeded",
            "retryable",
            "failed",
            "cancelled",
            "unschedulable",
        ] {
            histograms.insert(
                (strategy, result),
                metrics::histogram!(
                    "bifrost_forge_task_duration_seconds",
                    "strategy" => strategy,
                    "result" => result
                ),
            );
        }
    }
    histograms
}

/// Register one histogram for every strategy accepted by the v1 worker.
fn strategy_histograms(name: &'static str) -> BTreeMap<&'static str, Histogram> {
    ["staging_fold", "small_files", "snapshot_expiry"]
        .into_iter()
        .map(|strategy| (strategy, metrics::histogram!(name, "strategy" => strategy)))
        .collect()
}

/// Register one counter for every stable Forge conflict classification.
fn conflict_counters() -> BTreeMap<&'static str, Counter> {
    ["lease_contention", "fence_lost", "snapshot_changed"]
        .into_iter()
        .map(|kind| {
            (
                kind,
                metrics::counter!("bifrost_forge_conflicts_total", "kind" => kind),
            )
        })
        .collect()
}

/// Register cleanup duration histograms for the closed cleanup provenance set.
fn cleanup_histograms() -> BTreeMap<&'static str, Histogram> {
    ["expired", "orphan", "spill"]
        .into_iter()
        .map(|kind| {
            (
                kind,
                metrics::histogram!("bifrost_forge_cleanup_duration_seconds", "kind" => kind),
            )
        })
        .collect()
}

impl Forge {
    /// Measures exact durable object volume before appending a recovery terminal.
    ///
    /// Reading metadata before the terminal audit makes replay deduplication
    /// exact: a successful terminal append removes the operation from the open
    /// reconciliation page, while a metadata failure leaves it retryable.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when a staging or catalog path is
    /// malformed, foreign, or outside its tenant/table binding; an object-store
    /// error when metadata stat fails; or [`ForgeError::InvalidConfig`] when
    /// byte accumulation overflows.
    pub(super) async fn measure_rewrite_volume(
        &self,
        contract: RewritePathContract<'_>,
        input_paths: &[StoragePath],
        output_paths: &[StoragePath],
    ) -> Result<ForgeRewriteVolume, ForgeError> {
        let input_bytes = self.measure_path_bytes(contract, input_paths).await?;
        let output_bytes = self.measure_path_bytes(contract, output_paths).await?;
        Ok(ForgeRewriteVolume {
            input_files: input_paths.len(),
            input_bytes,
            output_files: output_paths.len(),
            output_bytes,
        })
    }

    /// Sums exact object sizes for one validated path set.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when a staging or catalog path is
    /// malformed, foreign, or outside its tenant/table binding; an object-store
    /// error when metadata stat fails; or [`ForgeError::InvalidConfig`] when
    /// the total exceeds `u64`.
    async fn measure_path_bytes(
        &self,
        contract: RewritePathContract<'_>,
        paths: &[StoragePath],
    ) -> Result<u64, ForgeError> {
        let mut bytes = 0_u64;
        for path in paths {
            let object_key = contract.object_key(&self.core.staging, path.as_str())?;
            let size = self
                .core
                .object_store
                .stat(&object_key)
                .await
                .map_err(ForgeError::ObjectStore)?
                .content_length();
            bytes = bytes
                .checked_add(size)
                .ok_or_else(|| ForgeError::InvalidConfig {
                    detail: "recovered rewrite volume exceeds u64".to_owned(),
                })?;
        }
        Ok(bytes)
    }
}

/// Production-owner telemetry sensitivity tests.
mod tests {
    #[cfg(test)]
    use std::collections::BTreeMap;
    #[cfg(test)]
    use std::sync::Arc;
    #[cfg(test)]
    use std::time::Duration;

    #[cfg(test)]
    use datafusion::execution::memory_pool::{MemoryConsumer, MemoryPool};
    #[cfg(test)]
    use wyrd_bench::{BenchmarkMetricSnapshot, BenchmarkRecorder};

    #[cfg(test)]
    use super::{ForgeMetricSource, ForgeTelemetry};
    #[cfg(test)]
    use crate::scribe::memory::{BifrostDataFusionMemoryPool, BifrostMemoryGovernor};

    /// Report families whose values are projected by the Forge maintenance report.
    #[cfg(test)]
    const REPORT_FAMILIES: [&str; 10] = [
        "bifrost_forge_rewrite_output_bytes_total",
        "bifrost_forge_task_duration_seconds",
        "bifrost_forge_oldest_backlog_seconds",
        "bifrost_memory_reserved_bytes",
        "bifrost_forge_task_spill_bytes",
        "bifrost_forge_conflicts_total",
        "bifrost_forge_fairness_lag_tasks",
        "bifrost_forge_cleanup_duration_seconds",
        "bifrost_forge_role_processes",
        "bifrost_forge_role_process_started_total",
    ];

    /// Return whether one report family changed between recorder snapshots.
    #[cfg(test)]
    fn family_changed(
        before: &BenchmarkMetricSnapshot,
        after: &BenchmarkMetricSnapshot,
        family: &str,
    ) -> bool {
        let counters = |snapshot: &BenchmarkMetricSnapshot| {
            snapshot
                .counters
                .iter()
                .filter(|(name, _)| {
                    name.as_str() == family || name.starts_with(&format!("{family}{{"))
                })
                .map(|(name, value)| (name.clone(), *value))
                .collect::<BTreeMap<_, _>>()
        };
        let gauges = |snapshot: &BenchmarkMetricSnapshot| {
            snapshot
                .gauges
                .iter()
                .filter(|(name, _)| {
                    name.as_str() == family || name.starts_with(&format!("{family}{{"))
                })
                .map(|(name, value)| (name.clone(), *value))
                .collect::<BTreeMap<_, _>>()
        };
        let histograms = |snapshot: &BenchmarkMetricSnapshot| {
            snapshot
                .histograms
                .iter()
                .filter(|(name, _)| {
                    name.as_str() == family || name.starts_with(&format!("{family}{{"))
                })
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect::<BTreeMap<_, _>>()
        };
        counters(before) != counters(after)
            || gauges(before) != gauges(after)
            || histograms(before) != histograms(after)
    }

    /// Assert one authoritative owner transition changes only its mapped report family.
    #[cfg(test)]
    fn assert_owner_transition(
        recorder: &BenchmarkRecorder,
        expected: &str,
        transition: impl FnOnce(),
    ) {
        let before = recorder.snapshot();
        transition();
        let after = recorder.snapshot();
        let changed = REPORT_FAMILIES
            .into_iter()
            .filter(|family| family_changed(&before, &after, family))
            .collect::<Vec<_>>();
        assert_eq!(changed, vec![expected]);
    }

    /// Drive each real telemetry owner boundary and pin independent report sensitivity.
    #[test]
    fn forge_production_telemetry_contract() {
        let recorder = BenchmarkRecorder::new()
            .install()
            .expect("production metrics recorder installs once");
        let telemetry = ForgeTelemetry::new();
        telemetry.record_rewrite_volume(ForgeMetricSource::Staging, 0, 0, 0, 0);
        telemetry.record_task_terminal("staging_fold", "succeeded", Duration::ZERO);
        telemetry.record_task_spill("staging_fold", 0);
        for kind in ["lease_contention", "fence_lost", "snapshot_changed"] {
            telemetry.record_conflict(kind);
        }
        telemetry.record_cleanup("expired", Duration::ZERO);
        telemetry.record_planning_status(Duration::from_secs(1), 1);
        let governor = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("valid memory");
        let pool: Arc<dyn MemoryPool> = Arc::new(BifrostDataFusionMemoryPool::new(governor));
        let reservation = MemoryConsumer::new("forge-owner-proof").register(&pool);
        reservation.try_grow(0).expect("zero-sized Forge reserve");

        assert_owner_transition(
            &recorder,
            "bifrost_forge_rewrite_output_bytes_total",
            || telemetry.record_rewrite_volume(ForgeMetricSource::Staging, 1, 64, 1, 32),
        );
        assert_owner_transition(&recorder, "bifrost_forge_task_duration_seconds", || {
            telemetry.record_task_terminal("staging_fold", "succeeded", Duration::from_millis(2));
        });
        assert_owner_transition(&recorder, "bifrost_forge_oldest_backlog_seconds", || {
            telemetry.record_planning_status(Duration::from_secs(2), 1);
        });
        assert_owner_transition(&recorder, "bifrost_forge_fairness_lag_tasks", || {
            telemetry.record_planning_status(Duration::from_secs(2), 2);
        });
        assert_owner_transition(&recorder, "bifrost_forge_task_spill_bytes", || {
            telemetry.record_task_spill("staging_fold", 4096);
        });
        for kind in ["lease_contention", "fence_lost", "snapshot_changed"] {
            assert_owner_transition(&recorder, "bifrost_forge_conflicts_total", || {
                telemetry.record_conflict(kind);
            });
        }
        assert_owner_transition(&recorder, "bifrost_forge_cleanup_duration_seconds", || {
            telemetry.record_cleanup("expired", Duration::from_millis(3));
        });

        assert_owner_transition(&recorder, "bifrost_memory_reserved_bytes", || {
            reservation.try_grow(4096).expect("Forge memory reserve");
        });
    }
}

/// Registers every closed source/result operation counter.
fn operation_counters() -> BTreeMap<(ForgeMetricSource, ForgeOperationResult), Counter> {
    let mut counters = BTreeMap::new();
    for source in [ForgeMetricSource::Staging, ForgeMetricSource::Iceberg] {
        for result in [
            ForgeOperationResult::Committed,
            ForgeOperationResult::Recovered,
            ForgeOperationResult::Reset,
            ForgeOperationResult::Noop,
            ForgeOperationResult::Budget,
            ForgeOperationResult::FenceLost,
            ForgeOperationResult::Failed,
        ] {
            counters.insert(
                (source, result),
                metrics::counter!(
                    "bifrost_forge_operations",
                    "source" => source.as_str(),
                    "result" => result.as_str()
                ),
            );
        }
    }
    counters
}

/// Registers every closed lease-result counter.
fn lease_counters() -> BTreeMap<ForgeLeaseResult, Counter> {
    [
        ForgeLeaseResult::Contention,
        ForgeLeaseResult::Takeover,
        ForgeLeaseResult::FenceLost,
    ]
    .into_iter()
    .map(|result| {
        (
            result,
            metrics::counter!(
                "bifrost_forge_lease_events_total",
                "result" => result.as_str()
            ),
        )
    })
    .collect()
}

/// Returns every closed owning-stage label.
fn stages() -> [ForgeMetricStage; 7] {
    [
        ForgeMetricStage::ReconcileStaging,
        ForgeMetricStage::ReconcileIceberg,
        ForgeMetricStage::StagingFold,
        ForgeMetricStage::ManifestDiscovery,
        ForgeMetricStage::IcebergRewrite,
        ForgeMetricStage::SnapshotExpiry,
        ForgeMetricStage::OrphanGc,
    ]
}

/// Registers failure counters for every closed owning stage.
fn stage_failure_counters() -> BTreeMap<ForgeMetricStage, Counter> {
    stages()
        .into_iter()
        .map(|stage| {
            (
                stage,
                metrics::counter!(
                    "bifrost_forge_stage_failures_total",
                    "stage" => stage.as_str()
                ),
            )
        })
        .collect()
}

/// Registers duration histograms for every closed owning stage.
fn stage_histograms() -> BTreeMap<ForgeMetricStage, Histogram> {
    stages()
        .into_iter()
        .map(|stage| {
            (
                stage,
                metrics::histogram!(
                    "bifrost_forge_stage_seconds",
                    "stage" => stage.as_str()
                ),
            )
        })
        .collect()
}

/// Registers one counter handle for each closed source value.
fn source_counters(name: &'static str) -> BTreeMap<ForgeMetricSource, Counter> {
    [ForgeMetricSource::Staging, ForgeMetricSource::Iceberg]
        .into_iter()
        .map(|source| (source, metrics::counter!(name, "source" => source.as_str())))
        .collect()
}
