//! Fixed-cardinality operational telemetry for Forge maintenance.

use std::collections::BTreeMap;
use std::future::Future;
use std::time::{Duration, Instant};

use metrics::{Counter, Gauge, Histogram};
use num_traits::ToPrimitive;
use wyrd_spec::vala::api::StoragePath;

use crate::catalog::TenantTableBinding;

use super::Forge;
use super::compact::ForgeTickOutcome;
use super::error::ForgeError;
use super::path::catalog_path_to_object_key;

/// Validated object-key contract for one rewrite source.
#[derive(Debug, Clone, Copy)]
pub(super) enum RewritePathContract<'binding> {
    /// Staging audit paths are already relative but must remain within binding.
    Staging {
        /// Physical tenant/table binding that owns every relative key.
        binding: &'binding TenantTableBinding,
    },
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
            Self::Staging { binding } => {
                binding
                    .validate_object_path(path)
                    .ok_or_else(|| ForgeError::Invariant {
                        detail: format!("staging path escaped table binding: {path}"),
                    })
            }
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
    /// Complete staging reconciliation observation.
    staging: ReconciliationGaugeObservation,
    /// Complete Iceberg reconciliation observation.
    iceberg: ReconciliationGaugeObservation,
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
        let aggregate = match source {
            ForgeMetricSource::Staging => &mut self.staging,
            ForgeMetricSource::Iceberg => &mut self.iceberg,
        };
        aggregate.prepared_operations = aggregate
            .prepared_operations
            .saturating_add(observation.prepared_operations);
        aggregate.uncertain_operations = aggregate
            .uncertain_operations
            .saturating_add(observation.uncertain_operations);
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
        Self {
            operations: operation_counters(),
            lease_events: lease_counters(),
            convergence: convergence_counters(),
            stage_failures: stage_failure_counters(),
            stage_seconds: stage_histograms(),
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
        self.pending_files[&ForgeMetricSource::Staging]
            .set(gauge_value(outcome.staging_pending_files));
        self.pending_files[&ForgeMetricSource::Iceberg].set(gauge_value(outcome.live_candidates));
        self.eligible_iceberg_files
            .set(gauge_value(outcome.live_candidates));
        self.prepared_operations[&ForgeMetricSource::Staging]
            .set(gauge_value(gauges.staging.prepared_operations));
        self.prepared_operations[&ForgeMetricSource::Iceberg]
            .set(gauge_value(gauges.iceberg.prepared_operations));
        self.uncertain_operations[&ForgeMetricSource::Staging]
            .set(gauge_value(gauges.staging.uncertain_operations));
        self.uncertain_operations[&ForgeMetricSource::Iceberg]
            .set(gauge_value(gauges.iceberg.uncertain_operations));
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

/// Registers every closed convergence-result counter.
fn convergence_counters() -> BTreeMap<ForgeConvergenceResult, Counter> {
    [
        ForgeConvergenceResult::Noop,
        ForgeConvergenceResult::Progress,
        ForgeConvergenceResult::Incomplete,
    ]
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

/// Largest integer that an `f64` metrics gauge represents exactly.
const MAX_EXACT_GAUGE_INTEGER: u64 = 1_u64 << 53;

/// Converts an in-memory count into the metrics gauge representation.
///
/// Counts through `2^53` remain exact. Larger counts saturate monotonically at
/// `2^53`, which is a truthful lower-bound sentinel that never rounds the
/// observed count upward. The saturation matches Forge's existing saturating
/// outcome contract for impossible or extreme in-memory counts.
fn gauge_value(value: usize) -> f64 {
    value
        .to_u64()
        .map_or(MAX_EXACT_GAUGE_INTEGER, |count| {
            count.min(MAX_EXACT_GAUGE_INTEGER)
        })
        .to_f64()
        .unwrap_or(9_007_199_254_740_992.0)
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
    use wyrd_bench::{BenchmarkMetricSnapshot, BenchmarkRecorder};
    use wyrd_spec::DataTenantId;

    use super::*;
    use crate::catalog::table_ref::TableRef;
    use crate::namespaces::BifrostNamespace;

    /// Builds one tenant-qualified binding for rewrite-path tests.
    fn rewrite_binding() -> TenantTableBinding {
        TenantTableBinding::resolve((
            DataTenantId::new_v7(),
            TableRef::new(BifrostNamespace::Traces, "spans"),
        ))
        .expect("test binding resolves")
    }

    /// Builds a filesystem operator for authority-free catalog URI tests.
    fn rewrite_store(root: &std::path::Path) -> opendal::Operator {
        let service = opendal::services::Fs::default()
            .root(root.to_str().expect("temporary path is valid UTF-8"));
        opendal::Operator::new(service)
            .expect("filesystem backend initializes")
            .finish()
    }

    /// Proves staging measurement preserves a validated relative object key.
    #[test]
    fn rewrite_path_contract_preserves_relative_staging_key() {
        let binding = rewrite_binding();
        let root = tempfile::tempdir().expect("temporary root initializes");
        let store = rewrite_store(root.path());
        let expected = format!("{}/data/part.parquet", binding.object_prefix);
        let actual = RewritePathContract::Staging { binding: &binding }
            .object_key(&store, &expected)
            .expect("binding-owned staging key validates");
        assert_eq!(actual, expected);
    }

    /// Proves live measurement converts a valid catalog URI to its object key.
    #[test]
    fn rewrite_path_contract_normalizes_catalog_uri() {
        let binding = rewrite_binding();
        let root = tempfile::tempdir().expect("temporary root initializes");
        let store = rewrite_store(root.path());
        let location = format!("file:///{}", binding.object_prefix);
        let catalog_path = format!("{location}/data/part.parquet");
        let expected = format!("{}/data/part.parquet", binding.object_prefix);
        let actual = RewritePathContract::Catalog {
            binding: &binding,
            table_location: &location,
        }
        .object_key(&store, &catalog_path)
        .expect("binding-owned catalog URI validates");
        assert_eq!(actual, expected);
    }

    /// Proves live measurement rejects a catalog URI outside the table root.
    #[test]
    fn rewrite_path_contract_rejects_foreign_catalog_uri() {
        let binding = rewrite_binding();
        let root = tempfile::tempdir().expect("temporary root initializes");
        let store = rewrite_store(root.path());
        let location = format!("file:///{}", binding.object_prefix);
        let foreign = format!(
            "file:///{}-foreign/data/part.parquet",
            binding.object_prefix
        );
        assert!(
            RewritePathContract::Catalog {
                binding: &binding,
                table_location: &location,
            }
            .object_key(&store, &foreign)
            .is_err()
        );
    }

    /// Proves malformed staging paths fail before object-store metadata IO.
    #[test]
    fn rewrite_path_contract_rejects_malformed_staging_path() {
        let binding = rewrite_binding();
        let root = tempfile::tempdir().expect("temporary root initializes");
        let store = rewrite_store(root.path());
        let malformed = format!("{}/../foreign/part.parquet", binding.object_prefix);
        assert!(
            RewritePathContract::Staging { binding: &binding }
                .object_key(&store, &malformed)
                .is_err()
        );
    }

    /// Records every typed operation, lease, volume, and stage boundary once.
    fn record_authoritative_boundaries(metrics: &ForgeMetrics) {
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
    }

    /// Asserts every source/result operation and lease series changed once.
    fn assert_operation_and_lease_boundaries(snapshot: &BenchmarkMetricSnapshot) {
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
    }

    /// Asserts exact rewrite volume, expiry, GC, duration, and failure deltas.
    fn assert_volume_and_stage_boundaries(snapshot: &BenchmarkMetricSnapshot) {
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

    /// Asserts one gauge value without exact floating-point comparison.
    fn assert_gauge(snapshot: &BenchmarkMetricSnapshot, key: &str, expected: f64) {
        let actual = snapshot.gauges[key];
        assert!(
            (actual - expected).abs() < f64::EPSILON,
            "{key} expected {expected}, observed {actual}"
        );
    }

    /// Proves gauge conversion is exact through `2^53` and saturates above it.
    #[test]
    fn gauge_value_saturates_without_overstatement() {
        let boundaries: [(u64, f64); 3] = [
            (MAX_EXACT_GAUGE_INTEGER - 1, 9_007_199_254_740_991.0),
            (MAX_EXACT_GAUGE_INTEGER, 9_007_199_254_740_992.0),
            (MAX_EXACT_GAUGE_INTEGER + 1, 9_007_199_254_740_992.0),
        ];
        for (input, expected) in boundaries {
            if let Ok(input) = usize::try_from(input) {
                let actual = gauge_value(input);
                assert_eq!(actual.to_bits(), expected.to_bits());
                assert!(actual <= input.to_f64().unwrap_or(f64::INFINITY));
            }
        }
        let maximum = usize::MAX;
        let actual = gauge_value(maximum);
        let expected = maximum
            .to_u64()
            .map_or(MAX_EXACT_GAUGE_INTEGER, |count| {
                count.min(MAX_EXACT_GAUGE_INTEGER)
            })
            .to_f64()
            .unwrap_or(9_007_199_254_740_992.0);
        assert_eq!(actual.to_bits(), expected.to_bits());
        assert!(actual <= maximum.to_f64().unwrap_or(f64::INFINITY));
    }

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
        record_authoritative_boundaries(&metrics);
        let snapshot = recorder.snapshot();
        assert_operation_and_lease_boundaries(&snapshot);
        assert_volume_and_stage_boundaries(&snapshot);
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
                staging: ReconciliationGaugeObservation {
                    prepared_operations: 7,
                    uncertain_operations: 2,
                },
                iceberg: ReconciliationGaugeObservation {
                    prepared_operations: 11,
                    uncertain_operations: 4,
                },
            },
        );
        metrics.record_complete_tick(
            &ForgeTickOutcome::default(),
            ForgeGaugeSnapshot {
                staging: ReconciliationGaugeObservation {
                    prepared_operations: 1,
                    uncertain_operations: 1,
                },
                ..ForgeGaugeSnapshot::default()
            },
        );
        let snapshot = recorder.snapshot();
        assert_gauge(
            &snapshot,
            "bifrost_forge_pending_files{source=\"staging\"}",
            3.0,
        );
        assert_gauge(
            &snapshot,
            "bifrost_forge_prepared_operations{source=\"staging\"}",
            7.0,
        );
        assert_gauge(
            &snapshot,
            "bifrost_forge_pending_files{source=\"iceberg\"}",
            5.0,
        );
        assert_gauge(&snapshot, "bifrost_forge_eligible_iceberg_files", 5.0);
        assert_gauge(
            &snapshot,
            "bifrost_forge_prepared_operations{source=\"iceberg\"}",
            11.0,
        );
        assert_gauge(
            &snapshot,
            "bifrost_forge_uncertain_operations{source=\"staging\"}",
            2.0,
        );
        assert_gauge(
            &snapshot,
            "bifrost_forge_uncertain_operations{source=\"iceberg\"}",
            4.0,
        );
        assert_eq!(snapshot.gauges.len(), 7);
    }

    /// Builds one complete tick for every terminal-work signal.
    fn terminal_outcomes() -> [ForgeTickOutcome; 10] {
        [
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
        ]
    }

    /// Builds one tick for every signal that prevents convergence.
    fn incomplete_outcomes(progress: &ForgeTickOutcome) -> [ForgeTickOutcome; 14] {
        [
            ForgeTickOutcome {
                pending_work: true,
                tick_complete: true,
                ..*progress
            },
            ForgeTickOutcome {
                lease_contention: 1,
                tick_complete: true,
                ..*progress
            },
            ForgeTickOutcome {
                tables_skipped: 1,
                tick_complete: true,
                ..*progress
            },
            ForgeTickOutcome {
                tables_failed: 1,
                tick_complete: true,
                ..*progress
            },
            ForgeTickOutcome {
                bins_skipped: 1,
                tick_complete: true,
                ..*progress
            },
            ForgeTickOutcome {
                budget_skips: 1,
                tick_complete: true,
                ..*progress
            },
            ForgeTickOutcome {
                fence_losses: 1,
                tick_complete: true,
                ..*progress
            },
            ForgeTickOutcome {
                stage_failures: 1,
                tick_complete: true,
                ..*progress
            },
            ForgeTickOutcome {
                live_snapshot_changes: 1,
                tick_complete: true,
                ..*progress
            },
            ForgeTickOutcome {
                live_pending: 1,
                tick_complete: true,
                ..*progress
            },
            ForgeTickOutcome {
                live_unresolved: 1,
                tick_complete: true,
                ..*progress
            },
            ForgeTickOutcome {
                open_operation_overflows: 1,
                tick_complete: true,
                ..*progress
            },
            ForgeTickOutcome {
                tables_discovered: 2,
                tables_examined: 1,
                tick_complete: true,
                ..*progress
            },
            ForgeTickOutcome {
                tick_complete: false,
                ..*progress
            },
        ]
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
        for terminal in terminal_outcomes() {
            assert_eq!(
                convergence_result(&terminal),
                ForgeConvergenceResult::Progress
            );
        }
        for incomplete in incomplete_outcomes(&progress) {
            assert_eq!(
                convergence_result(&incomplete),
                ForgeConvergenceResult::Incomplete
            );
        }
    }
}
