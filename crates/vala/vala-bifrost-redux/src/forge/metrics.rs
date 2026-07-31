//! Fixed-cardinality operational telemetry for Forge maintenance.

use std::collections::BTreeMap;
use std::future::Future;
use std::time::{Duration, Instant};

use metrics::{Counter, Gauge, Histogram};
use wyrd_spec::vala::api::StoragePath;

use super::Forge;
use super::compact::ForgeTickOutcome;
use super::error::ForgeError;

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

    /// Returns the durable data source owned by this stage.
    const fn source(self) -> ForgeMetricSource {
        match self {
            Self::ReconcileStaging | Self::StagingFold => ForgeMetricSource::Staging,
            Self::ReconcileIceberg
            | Self::ManifestDiscovery
            | Self::IcebergRewrite
            | Self::SnapshotExpiry
            | Self::OrphanGc => ForgeMetricSource::Iceberg,
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

/// Closed convergence classification for complete periodic passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum ForgeConvergenceResult {
    /// The complete pass observed no work.
    Noop,
    /// The complete pass made terminal progress with no remaining blocker.
    Progress,
    /// The pass retained pending, failed, or incomplete evidence.
    Incomplete,
}

impl ForgeConvergenceResult {
    /// Returns the only metric label value emitted for this result.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Noop => "noop",
            Self::Progress => "progress",
            Self::Incomplete => "incomplete",
        }
    }
}

/// Complete-tick reconciliation backlog accumulated across every table.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct ForgeGaugeSnapshot {
    /// Latest prepared staging operations examined by the complete pass.
    pub(super) staging_prepared_operations: usize,
    /// Latest prepared Iceberg operations examined by the complete pass.
    pub(super) iceberg_prepared_operations: usize,
    /// Staging operations whose terminal state remains uncertain.
    pub(super) staging_uncertain_operations: usize,
    /// Iceberg operations whose terminal state remains uncertain.
    pub(super) iceberg_uncertain_operations: usize,
}

/// One bounded reconciliation observation before it is merged into a tick.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct ReconciliationGaugeObservation {
    /// Prepared operations examined, or the explicit overflow lower bound.
    pub(super) prepared_operations: usize,
    /// Prepared operations left without terminal proof.
    pub(super) uncertain_operations: usize,
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

impl ForgeGaugeSnapshot {
    /// Saturatingly merges one source-local reconciliation observation.
    pub(super) fn record_reconciliation(
        &mut self,
        source: ForgeMetricSource,
        observation: ReconciliationGaugeObservation,
    ) {
        let (prepared, uncertain) = match source {
            ForgeMetricSource::Staging => (
                &mut self.staging_prepared_operations,
                &mut self.staging_uncertain_operations,
            ),
            ForgeMetricSource::Iceberg => (
                &mut self.iceberg_prepared_operations,
                &mut self.iceberg_uncertain_operations,
            ),
        };
        *prepared = prepared.saturating_add(observation.prepared_operations);
        *uncertain = uncertain.saturating_add(observation.uncertain_operations);
    }
}

/// Registered Forge metric handles retained by one Forge owner.
pub(super) struct ForgeMetrics {
    /// Terminal operation counters for every source/result combination.
    operations: BTreeMap<(ForgeMetricSource, ForgeOperationResult), Counter>,
    /// Lease-boundary counters for every closed result.
    lease_events: BTreeMap<ForgeLeaseResult, Counter>,
    /// Complete-pass convergence counters.
    convergence: BTreeMap<ForgeConvergenceResult, Counter>,
    /// Failure counters for every owning stage.
    stage_failures: BTreeMap<ForgeMetricStage, Counter>,
    /// Duration histograms for every owning stage.
    stage_seconds: BTreeMap<ForgeMetricStage, Histogram>,
    /// Source-labelled pending-file gauges.
    pending_files: BTreeMap<ForgeMetricSource, Gauge>,
    /// Current-snapshot eligible-file gauge.
    eligible_iceberg_files: Gauge,
    /// Source-labelled prepared-operation gauges.
    prepared_operations: BTreeMap<ForgeMetricSource, Gauge>,
    /// Source-labelled uncertain-operation gauges.
    uncertain_operations: BTreeMap<ForgeMetricSource, Gauge>,
    /// Committed/recovered replacement input-file counters.
    rewrite_input_files: BTreeMap<ForgeMetricSource, Counter>,
    /// Committed/recovered replacement input-byte counters.
    rewrite_input_bytes: BTreeMap<ForgeMetricSource, Counter>,
    /// Committed/recovered replacement output-file counters.
    rewrite_output_files: BTreeMap<ForgeMetricSource, Counter>,
    /// Committed/recovered replacement output-byte counters.
    rewrite_output_bytes: BTreeMap<ForgeMetricSource, Counter>,
    /// Confirmed snapshot-expiry effects.
    snapshot_expired: Counter,
    /// Bounded orphan candidates examined.
    gc_candidates: Counter,
    /// Confirmed or idempotently absent orphan objects.
    gc_deleted: Counter,
}

impl ForgeMetrics {
    /// Registers the complete fixed-cardinality Forge metric inventory.
    pub(super) fn new() -> Self {
        const SOURCES: [ForgeMetricSource; 2] =
            [ForgeMetricSource::Staging, ForgeMetricSource::Iceberg];
        const RESULTS: [ForgeOperationResult; 7] = [
            ForgeOperationResult::Committed,
            ForgeOperationResult::Recovered,
            ForgeOperationResult::Reset,
            ForgeOperationResult::Noop,
            ForgeOperationResult::Budget,
            ForgeOperationResult::FenceLost,
            ForgeOperationResult::Failed,
        ];
        const LEASE_RESULTS: [ForgeLeaseResult; 3] = [
            ForgeLeaseResult::Contention,
            ForgeLeaseResult::Takeover,
            ForgeLeaseResult::FenceLost,
        ];
        const CONVERGENCE_RESULTS: [ForgeConvergenceResult; 3] = [
            ForgeConvergenceResult::Noop,
            ForgeConvergenceResult::Progress,
            ForgeConvergenceResult::Incomplete,
        ];
        const STAGES: [ForgeMetricStage; 7] = [
            ForgeMetricStage::ReconcileStaging,
            ForgeMetricStage::ReconcileIceberg,
            ForgeMetricStage::StagingFold,
            ForgeMetricStage::ManifestDiscovery,
            ForgeMetricStage::IcebergRewrite,
            ForgeMetricStage::SnapshotExpiry,
            ForgeMetricStage::OrphanGc,
        ];
        let mut operations = BTreeMap::new();
        for source in SOURCES {
            for result in RESULTS {
                operations.insert(
                    (source, result),
                    metrics::counter!(
                        "bifrost_forge_operations",
                        "source" => source.as_str(),
                        "result" => result.as_str()
                    ),
                );
            }
        }
        Self {
            operations,
            lease_events: LEASE_RESULTS
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
                .collect(),
            convergence: CONVERGENCE_RESULTS
                .into_iter()
                .map(|result| {
                    (
                        result,
                        metrics::counter!(
                            "bifrost_forge_convergence_passes_total",
                            "result" => result.as_str()
                        ),
                    )
                })
                .collect(),
            stage_failures: STAGES
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
                .collect(),
            stage_seconds: STAGES
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
                .collect(),
            pending_files: source_gauges("bifrost_forge_pending_files"),
            eligible_iceberg_files: metrics::gauge!("bifrost_forge_eligible_iceberg_files"),
            prepared_operations: source_gauges("bifrost_forge_prepared_operations"),
            uncertain_operations: source_gauges("bifrost_forge_uncertain_operations"),
            rewrite_input_files: source_counters("bifrost_forge_rewrite_input_files_total"),
            rewrite_input_bytes: source_counters("bifrost_forge_rewrite_input_bytes_total"),
            rewrite_output_files: source_counters("bifrost_forge_rewrite_output_files_total"),
            rewrite_output_bytes: source_counters("bifrost_forge_rewrite_output_bytes_total"),
            snapshot_expired: metrics::counter!("bifrost_forge_snapshot_expired_total"),
            gc_candidates: metrics::counter!("bifrost_forge_gc_candidates_total"),
            gc_deleted: metrics::counter!("bifrost_forge_gc_deleted_total"),
        }
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

    /// Measures one owning stage future and records its single return boundary.
    ///
    /// # Errors
    ///
    /// Returns the stage's original error after recording its duration and one
    /// failure event. Cancellation represented as an error is therefore also
    /// retained without changing the stage's control flow.
    pub(super) async fn observe_stage<T>(
        &self,
        stage: ForgeMetricStage,
        future: impl Future<Output = Result<T, super::error::ForgeError>>,
    ) -> Result<T, super::error::ForgeError> {
        let started = Instant::now();
        let result = future.await;
        self.record_stage(stage, started.elapsed(), result.is_err());
        if let Err(error) = &result {
            let operation_result = if matches!(error, super::error::ForgeError::FenceLost { .. }) {
                ForgeOperationResult::FenceLost
            } else {
                ForgeOperationResult::Failed
            };
            self.record_operation(stage.source(), operation_result, 1);
        }
        result
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

    /// Publishes backlog gauges and convergence only for a complete periodic pass.
    pub(super) fn record_complete_tick(
        &self,
        outcome: &ForgeTickOutcome,
        gauges: ForgeGaugeSnapshot,
    ) {
        if !outcome.tick_complete || outcome.tables_examined != outcome.tables_discovered {
            return;
        }
        self.pending_files[&ForgeMetricSource::Staging].set(outcome.staging_pending_files as f64);
        self.pending_files[&ForgeMetricSource::Iceberg].set(outcome.live_candidates as f64);
        self.eligible_iceberg_files
            .set(outcome.live_candidates as f64);
        self.prepared_operations[&ForgeMetricSource::Staging]
            .set(gauges.staging_prepared_operations as f64);
        self.prepared_operations[&ForgeMetricSource::Iceberg]
            .set(gauges.iceberg_prepared_operations as f64);
        self.uncertain_operations[&ForgeMetricSource::Staging]
            .set(gauges.staging_uncertain_operations as f64);
        self.uncertain_operations[&ForgeMetricSource::Iceberg]
            .set(gauges.iceberg_uncertain_operations as f64);
        self.convergence[&convergence_result(outcome)].increment(1);
    }

    /// Publishes exact aggregate counters at the periodic or hinted completion boundary.
    pub(super) fn record_outcome(&self, outcome: &ForgeTickOutcome) {
        self.record_operation(
            ForgeMetricSource::Staging,
            ForgeOperationResult::Committed,
            outcome.bins_committed,
        );
        self.record_operation(
            ForgeMetricSource::Iceberg,
            ForgeOperationResult::Committed,
            outcome.live_groups_committed,
        );
        if outcome.is_converged() {
            self.record_operation(ForgeMetricSource::Staging, ForgeOperationResult::Noop, 1);
            self.record_operation(ForgeMetricSource::Iceberg, ForgeOperationResult::Noop, 1);
        }
        self.record_rewrite_volume(
            ForgeMetricSource::Staging,
            outcome.staging_input_files,
            outcome.staging_input_bytes,
            outcome.staging_output_files,
            outcome.staging_output_bytes,
        );
        self.record_rewrite_volume(
            ForgeMetricSource::Iceberg,
            outcome.live_input_files,
            outcome.live_input_bytes,
            outcome.live_output_files,
            outcome.live_output_bytes,
        );
        self.snapshot_expired
            .increment(outcome.expiry_reconciled as u64);
        self.gc_candidates.increment(outcome.gc_candidates as u64);
        self.gc_deleted.increment(outcome.gc_deleted as u64);
    }
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
    /// Returns an object-store error when any exact path cannot be stated, or
    /// [`ForgeError::InvalidConfig`] when byte accumulation overflows.
    pub(super) async fn measure_rewrite_volume(
        &self,
        input_paths: &[StoragePath],
        output_paths: &[StoragePath],
    ) -> Result<ForgeRewriteVolume, ForgeError> {
        let input_bytes = self.measure_path_bytes(input_paths).await?;
        let output_bytes = self.measure_path_bytes(output_paths).await?;
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
    /// Returns an object-store error when metadata cannot be read, or
    /// [`ForgeError::InvalidConfig`] when the total exceeds `u64`.
    async fn measure_path_bytes(&self, paths: &[StoragePath]) -> Result<u64, ForgeError> {
        let mut bytes = 0_u64;
        for path in paths {
            let size = self
                .core
                .object_store
                .stat(path.as_str())
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

/// Registers one gauge handle for each closed source value.
fn source_gauges(name: &'static str) -> BTreeMap<ForgeMetricSource, Gauge> {
    [ForgeMetricSource::Staging, ForgeMetricSource::Iceberg]
        .into_iter()
        .map(|source| (source, metrics::gauge!(name, "source" => source.as_str())))
        .collect()
}

/// Registers one counter handle for each closed source value.
fn source_counters(name: &'static str) -> BTreeMap<ForgeMetricSource, Counter> {
    [ForgeMetricSource::Staging, ForgeMetricSource::Iceberg]
        .into_iter()
        .map(|source| (source, metrics::counter!(name, "source" => source.as_str())))
        .collect()
}

/// Classifies a complete returned periodic outcome with the approved predicates.
fn convergence_result(outcome: &ForgeTickOutcome) -> ForgeConvergenceResult {
    if outcome.is_converged() {
        return ForgeConvergenceResult::Noop;
    }
    let terminal_work = outcome.bins_committed > 0
        || outcome.reconciled > 0
        || outcome.reconciliation_recovered > 0
        || outcome.expiry_reconciled > 0
        || outcome.gc_reconciled > 0
        || outcome.gc_deleted > 0
        || outcome.live_groups_committed > 0
        || outcome.live_recovered > 0
        || outcome.live_reset > 0
        || outcome.outputs_committed > 0;
    let pending_or_failure = outcome.pending_work
        || outcome.tables_skipped > 0
        || outcome.tables_failed > 0
        || outcome.bins_skipped > 0
        || outcome.budget_skips > 0
        || outcome.lease_contention > 0
        || outcome.fence_losses > 0
        || outcome.stage_failures > 0
        || outcome.live_snapshot_changes > 0
        || outcome.live_pending > 0
        || outcome.live_unresolved > 0
        || outcome.open_operation_overflows > 0;
    let complete = outcome.tick_complete && outcome.tables_examined == outcome.tables_discovered;
    if complete && terminal_work && !pending_or_failure {
        ForgeConvergenceResult::Progress
    } else {
        ForgeConvergenceResult::Incomplete
    }
}

#[cfg(test)]
mod tests {
    use wyrd_bench::BenchmarkRecorder;

    use super::*;

    /// Proves the fixed inventory constructs without accepting arbitrary labels.
    #[test]
    fn forge_metrics_register_closed_inventory_once() {
        let recorder = BenchmarkRecorder::new();
        let _metrics = metrics::with_local_recorder(&recorder, ForgeMetrics::new);
        let snapshot = recorder.snapshot();
        assert_eq!(snapshot.series, 52);
        assert_eq!(snapshot.counters.len(), 38);
        assert_eq!(snapshot.gauges.len(), 7);
        assert_eq!(snapshot.histograms.len(), 7);
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
                assert!(snapshot.counters.contains_key(&format!(
                    "bifrost_forge_operations{{result=\"{}\",source=\"{}\"}}",
                    result.as_str(),
                    source.as_str()
                )));
            }
            for family in [
                "bifrost_forge_pending_files",
                "bifrost_forge_prepared_operations",
                "bifrost_forge_uncertain_operations",
            ] {
                assert!(
                    snapshot
                        .gauges
                        .contains_key(&format!("{family}{{source=\"{}\"}}", source.as_str()))
                );
            }
            for family in [
                "bifrost_forge_rewrite_input_files_total",
                "bifrost_forge_rewrite_input_bytes_total",
                "bifrost_forge_rewrite_output_files_total",
                "bifrost_forge_rewrite_output_bytes_total",
            ] {
                assert!(
                    snapshot
                        .counters
                        .contains_key(&format!("{family}{{source=\"{}\"}}", source.as_str()))
                );
            }
        }
        for key in snapshot
            .counters
            .keys()
            .chain(snapshot.gauges.keys())
            .chain(snapshot.histograms.keys())
        {
            assert!(
                ![
                    "tenant",
                    "table",
                    "day",
                    "path",
                    "operation",
                    "snapshot",
                    "owner",
                    "error",
                    "lease_key",
                ]
                .iter()
                .any(|label| key.contains(&format!("{label}=\"")))
            );
        }
    }

    /// Proves typed authoritative boundaries accept every closed result.
    #[test]
    fn forge_metrics_record_authoritative_boundaries() {
        let recorder = BenchmarkRecorder::new();
        let metrics = metrics::with_local_recorder(&recorder, ForgeMetrics::new);
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
                metrics.record_operation(source, result, 1);
            }
        }
        for result in [
            ForgeLeaseResult::Contention,
            ForgeLeaseResult::Takeover,
            ForgeLeaseResult::FenceLost,
        ] {
            metrics.record_lease(result);
        }
        metrics.snapshot_expired.increment(2);
        metrics.gc_candidates.increment(3);
        metrics.gc_deleted.increment(4);
        metrics.record_rewrite_volume(ForgeMetricSource::Iceberg, 5, 6, 7, 8);
        metrics.record_rewrite_volume(ForgeMetricSource::Staging, 9, 10, 11, 12);
        metrics.record_stage(
            ForgeMetricStage::StagingFold,
            Duration::from_millis(1),
            false,
        );
        metrics.record_stage(
            ForgeMetricStage::IcebergRewrite,
            Duration::from_millis(2),
            true,
        );
        let snapshot = recorder.snapshot();
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
                assert_eq!(
                    snapshot.counters[&format!(
                        "bifrost_forge_operations{{result=\"{}\",source=\"{}\"}}",
                        result.as_str(),
                        source.as_str()
                    )],
                    1
                );
            }
        }
        for result in [
            ForgeLeaseResult::Contention,
            ForgeLeaseResult::Takeover,
            ForgeLeaseResult::FenceLost,
        ] {
            assert_eq!(
                snapshot.counters[&format!(
                    "bifrost_forge_lease_events_total{{result=\"{}\"}}",
                    result.as_str()
                )],
                1
            );
        }
        assert_eq!(snapshot.counters["bifrost_forge_snapshot_expired_total"], 2);
        assert_eq!(snapshot.counters["bifrost_forge_gc_candidates_total"], 3);
        assert_eq!(snapshot.counters["bifrost_forge_gc_deleted_total"], 4);
        for (source, values) in [
            (ForgeMetricSource::Iceberg, [5, 6, 7, 8]),
            (ForgeMetricSource::Staging, [9, 10, 11, 12]),
        ] {
            for (family, expected) in [
                ("bifrost_forge_rewrite_input_files_total", values[0]),
                ("bifrost_forge_rewrite_input_bytes_total", values[1]),
                ("bifrost_forge_rewrite_output_files_total", values[2]),
                ("bifrost_forge_rewrite_output_bytes_total", values[3]),
            ] {
                assert_eq!(
                    snapshot.counters[&format!("{family}{{source=\"{}\"}}", source.as_str())],
                    expected
                );
            }
        }
        assert_eq!(
            snapshot.histograms["bifrost_forge_stage_seconds{stage=\"staging_fold\"}"].count,
            1
        );
        assert_eq!(
            snapshot.histograms["bifrost_forge_stage_seconds{stage=\"iceberg_rewrite\"}"].count,
            1
        );
        assert_eq!(
            snapshot.counters["bifrost_forge_stage_failures_total{stage=\"iceberg_rewrite\"}"],
            1
        );
        assert_eq!(
            snapshot.counters["bifrost_forge_stage_failures_total{stage=\"staging_fold\"}"],
            0
        );
    }

    /// Proves incomplete publication cannot overwrite a complete snapshot.
    #[test]
    fn forge_metrics_gauges_preserve_complete_tick_snapshot() {
        let recorder = BenchmarkRecorder::new();
        let metrics = metrics::with_local_recorder(&recorder, ForgeMetrics::new);
        let complete = ForgeTickOutcome {
            staging_pending_files: 3,
            live_candidates: 5,
            tick_complete: true,
            ..ForgeTickOutcome::default()
        };
        metrics.record_complete_tick(
            &complete,
            ForgeGaugeSnapshot {
                staging_prepared_operations: 7,
                iceberg_prepared_operations: 11,
                staging_uncertain_operations: 2,
                iceberg_uncertain_operations: 4,
            },
        );
        metrics.record_complete_tick(
            &ForgeTickOutcome::default(),
            ForgeGaugeSnapshot {
                staging_prepared_operations: 1,
                ..ForgeGaugeSnapshot::default()
            },
        );
        let snapshot = recorder.snapshot();
        assert_eq!(
            snapshot.gauges["bifrost_forge_pending_files{source=\"staging\"}"],
            3.0
        );
        assert_eq!(
            snapshot.gauges["bifrost_forge_prepared_operations{source=\"staging\"}"],
            7.0
        );
        assert_eq!(
            snapshot.gauges["bifrost_forge_pending_files{source=\"iceberg\"}"],
            5.0
        );
        assert_eq!(snapshot.gauges["bifrost_forge_eligible_iceberg_files"], 5.0);
        assert_eq!(
            snapshot.gauges["bifrost_forge_prepared_operations{source=\"iceberg\"}"],
            11.0
        );
        assert_eq!(
            snapshot.gauges["bifrost_forge_uncertain_operations{source=\"staging\"}"],
            2.0
        );
        assert_eq!(
            snapshot.gauges["bifrost_forge_uncertain_operations{source=\"iceberg\"}"],
            4.0
        );
        assert_eq!(snapshot.gauges.len(), 7);
    }

    /// Proves every complete-pass convergence classification is exhaustive.
    #[test]
    fn forge_metrics_convergence_table_is_exhaustive() {
        let noop = ForgeTickOutcome {
            tick_complete: true,
            ..ForgeTickOutcome::default()
        };
        assert_eq!(convergence_result(&noop), ForgeConvergenceResult::Noop);
        let progress = ForgeTickOutcome {
            bins_committed: 1,
            tick_complete: true,
            ..ForgeTickOutcome::default()
        };
        assert_eq!(
            convergence_result(&progress),
            ForgeConvergenceResult::Progress
        );
        for terminal in [
            ForgeTickOutcome {
                bins_committed: 1,
                tick_complete: true,
                ..ForgeTickOutcome::default()
            },
            ForgeTickOutcome {
                reconciled: 1,
                tick_complete: true,
                ..ForgeTickOutcome::default()
            },
            ForgeTickOutcome {
                reconciliation_recovered: 1,
                tick_complete: true,
                ..ForgeTickOutcome::default()
            },
            ForgeTickOutcome {
                expiry_reconciled: 1,
                tick_complete: true,
                ..ForgeTickOutcome::default()
            },
            ForgeTickOutcome {
                gc_reconciled: 1,
                tick_complete: true,
                ..ForgeTickOutcome::default()
            },
            ForgeTickOutcome {
                gc_deleted: 1,
                tick_complete: true,
                ..ForgeTickOutcome::default()
            },
            ForgeTickOutcome {
                live_groups_committed: 1,
                tick_complete: true,
                ..ForgeTickOutcome::default()
            },
            ForgeTickOutcome {
                live_recovered: 1,
                tick_complete: true,
                ..ForgeTickOutcome::default()
            },
            ForgeTickOutcome {
                live_reset: 1,
                tick_complete: true,
                ..ForgeTickOutcome::default()
            },
            ForgeTickOutcome {
                outputs_committed: 1,
                tick_complete: true,
                ..ForgeTickOutcome::default()
            },
        ] {
            assert_eq!(
                convergence_result(&terminal),
                ForgeConvergenceResult::Progress
            );
        }
        for incomplete in [
            ForgeTickOutcome {
                pending_work: true,
                tick_complete: true,
                ..progress
            },
            ForgeTickOutcome {
                lease_contention: 1,
                tick_complete: true,
                ..progress
            },
            ForgeTickOutcome {
                tables_skipped: 1,
                tick_complete: true,
                ..progress
            },
            ForgeTickOutcome {
                tables_failed: 1,
                tick_complete: true,
                ..progress
            },
            ForgeTickOutcome {
                bins_skipped: 1,
                tick_complete: true,
                ..progress
            },
            ForgeTickOutcome {
                budget_skips: 1,
                tick_complete: true,
                ..progress
            },
            ForgeTickOutcome {
                fence_losses: 1,
                tick_complete: true,
                ..progress
            },
            ForgeTickOutcome {
                stage_failures: 1,
                tick_complete: true,
                ..progress
            },
            ForgeTickOutcome {
                live_snapshot_changes: 1,
                tick_complete: true,
                ..progress
            },
            ForgeTickOutcome {
                live_pending: 1,
                tick_complete: true,
                ..progress
            },
            ForgeTickOutcome {
                live_unresolved: 1,
                tick_complete: true,
                ..progress
            },
            ForgeTickOutcome {
                open_operation_overflows: 1,
                tick_complete: true,
                ..progress
            },
            ForgeTickOutcome {
                tables_discovered: 2,
                tables_examined: 1,
                tick_complete: true,
                ..progress
            },
            ForgeTickOutcome {
                tick_complete: false,
                ..progress
            },
        ] {
            assert_eq!(
                convergence_result(&incomplete),
                ForgeConvergenceResult::Incomplete
            );
        }
    }
}
