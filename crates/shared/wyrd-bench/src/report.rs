//! Structured Bifrost benchmark reports and percentile math.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::recorder::BenchmarkMetricSnapshot;
use crate::workload::WorkloadSpec;

/// Latency percentiles in microseconds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct LatencyPercentiles {
    /// 50th percentile.
    pub p50_us: u64,
    /// 95th percentile.
    pub p95_us: u64,
    /// 99th percentile.
    pub p99_us: u64,
    /// 99.9th percentile.
    pub p999_us: u64,
}

impl LatencyPercentiles {
    /// Calculate nearest-rank percentiles from non-negative microseconds.
    #[must_use]
    pub fn from_samples(samples: &[u64]) -> Self {
        if samples.is_empty() {
            return Self::default();
        }
        let mut sorted = samples.to_vec();
        sorted.sort_unstable();
        Self {
            p50_us: nearest_rank(&sorted, 0.50),
            p95_us: nearest_rank(&sorted, 0.95),
            p99_us: nearest_rank(&sorted, 0.99),
            p999_us: nearest_rank(&sorted, 0.999),
        }
    }
}

fn nearest_rank(sorted: &[u64], percentile: f64) -> u64 {
    let (numerator, denominator) = match percentile {
        0.50 => (1_u128, 2_u128),
        0.95 => (19_u128, 20_u128),
        0.99 => (99_u128, 100_u128),
        0.999 => (999_u128, 1_000_u128),
        _ => unreachable!("percentile is one of the report's fixed values"),
    };
    let length = u128::try_from(sorted.len()).expect("usize always fits in u128");
    let rank = usize::try_from((length * numerator).div_ceil(denominator))
        .expect("nearest rank fits in usize");
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

/// Machine and pod metadata normalized for cross-stage comparison.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachineMetadata {
    /// Git SHA that produced the report.
    pub git_sha: String,
    /// Whether the worktree had uncommitted changes.
    pub dirty_worktree: bool,
    /// Operating-system identifier.
    pub operating_system: String,
    /// CPU count when available.
    pub cpu_count: Option<u32>,
    /// Memory value supplied by the runner, when available.
    pub memory_bytes: Option<u64>,
}

/// Pod and process metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PodMetadata {
    /// Number of independently bound pods.
    pub pod_count: u32,
    /// Stable pod IDs used for the run.
    pub pod_ids: Vec<String>,
}

/// Storage and queue measurements shared by all Bifrost lanes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StorageMeasurements {
    /// Rows accepted per second.
    pub rows_per_second: f64,
    /// MiB accepted per second.
    pub mib_per_second: f64,
    /// Queue time in microseconds.
    pub queue_us: u64,
    /// WAL fsync time in microseconds.
    pub fsync_us: u64,
    /// Active memory bytes.
    pub active_memory_bytes: u64,
    /// Immutable memory bytes.
    pub immutable_memory_bytes: u64,
    /// Queued memory bytes.
    pub queued_memory_bytes: u64,
    /// WAL growth bytes.
    pub wal_growth_bytes: u64,
    /// WAL replay time in microseconds.
    pub wal_replay_us: u64,
    /// Number of visible Parquet files.
    pub file_count: u64,
    /// Average Parquet file size in bytes.
    pub average_file_size_bytes: u64,
    /// Compaction bytes-written / bytes-read ratio.
    pub compaction_amplification: f64,
}

/// Query and audit measurements.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct QueryMeasurements {
    /// Time to the first query result byte in microseconds.
    pub first_byte_us: u64,
    /// Write-to-query visibility freshness in microseconds.
    pub freshness_us: u64,
    /// Audit visibility lag in microseconds.
    pub audit_visibility_lag_us: u64,
}

/// Measurements for one real Bifrost pipeline stage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageMeasurements {
    /// Stable stage name used in reports and comparisons.
    pub stage: String,
    /// Stage duration percentiles.
    pub latency: LatencyPercentiles,
    /// Number of stage invocations.
    pub count: u64,
    /// Rows processed by the stage.
    pub rows: u64,
    /// Bytes processed or written by the stage.
    pub bytes: u64,
    /// Number of failed stage invocations.
    pub errors: u64,
}

/// One phase of a bounded workload run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhaseMeasurement {
    /// Stable phase name.
    pub phase: String,
    /// Target rate for the phase.
    pub target_rows_per_second: u64,
    /// Phase duration in milliseconds.
    pub duration_ms: u64,
    /// Rows submitted during the phase.
    pub submitted_rows: u64,
    /// Rows accepted by Scribe during the phase.
    pub accepted_rows: u64,
    /// Append failures during the phase.
    pub errors: u64,
}

/// A point-in-time backlog sample from Scribe and Forge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BacklogSample {
    /// Milliseconds since the run started.
    pub elapsed_ms: u64,
    /// WAL bytes retained on local pod disks.
    pub wal_bytes: u64,
    /// Writable memtable rows.
    pub active_rows: u64,
    /// Rows retained in immutable generations.
    pub immutable_rows: u64,
    /// Immutable generations awaiting post-commit completion.
    pub pending_generations: u64,
    /// Accepted request items still charged to pod admission.
    pub admitted_items: u64,
    /// Admission bytes charged to accepted requests.
    pub admitted_bytes: u64,
    /// Active logical tenant/table writers.
    pub active_writers: u64,
    /// Operations retained by the fixed blocking executor.
    pub executor_depth: u64,
    /// Executor submissions that waited for capacity.
    pub executor_saturation_events: u64,
    /// Writers that have entered terminal unhealthy state.
    pub unhealthy_writers: u64,
    /// Eligible Forge file-list candidates.
    pub forge_candidates: u64,
}

/// Durable output verification result.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationMeasurements {
    /// Rows accepted by producers.
    pub expected_rows: u64,
    /// Rows represented by file-list entries.
    pub file_list_rows: u64,
    /// File-list entries whose objects were found.
    pub parquet_files: u64,
    /// File-list entries whose objects were missing.
    pub missing_objects: u64,
    /// Eligible Forge candidates remaining at verification time.
    pub forge_candidates: u64,
    /// Whether all required checks passed.
    pub passed: bool,
}

/// Forge compaction counters collected across maintenance ticks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgeMeasurements {
    /// Maintenance ticks that returned successfully.
    pub ticks: u64,
    /// Candidate groups discovered by Forge.
    pub groups_seen: u64,
    /// Rewrite bins committed by Forge.
    pub bins_committed: u64,
    /// Rewrite bins skipped by a budget or lease boundary.
    pub bins_skipped: u64,
    /// Tables whose maintenance stages failed.
    pub tables_failed: u64,
    /// Lease-contention events observed by Forge.
    pub lease_contention: u64,
    /// Maximum eligible candidate count observed during the run.
    pub peak_candidates: u64,
}

/// One required Scribe workload configuration from the full benchmark matrix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScribeMatrixCase {
    /// Approximate canonical batch size. `1` represents the one-row case.
    pub batch_size_bytes: u64,
    /// Number of data tenants in the run.
    pub tenant_count: u32,
    /// Whether all appends share one seal-key or are dispersed.
    pub seal_key_pattern: String,
    /// Whether the batch spans more than one UTC event day.
    pub cross_day: bool,
    /// WAL sync condition.
    pub fsync_mode: String,
    /// Whether one tenant is intentionally noisy.
    pub noisy_tenant: bool,
}

/// One compact Scribe SLO workload declaration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScribeCompactCase {
    /// Stable case identifier.
    pub id: String,
    /// Target frame size used to construct the Arrow IPC payload.
    pub frame_size_bytes: u64,
    /// Concurrent public SDK streams used by the case.
    pub writers: u32,
    /// Requested tenant count.
    pub tenants: u32,
    /// Requested pod count.
    pub pods: u32,
    /// Number of tables receiving frames in the case.
    pub tables: u32,
    /// Table distribution: all streams share one table or are dispersed.
    pub table_distribution: String,
    /// Explicit writer routing mode.
    pub routing_mode: String,
    /// Fsync mode declared by the case.
    pub fsync_mode: String,
    /// Injected fsync delay in milliseconds.
    pub fsync_delay_ms: u64,
    /// Minimum measured frames after warmup.
    pub minimum_samples: u64,
}

type CompactCaseDefinition = (
    &'static str,
    u64,
    u32,
    u32,
    u32,
    u32,
    &'static str,
    &'static str,
    u64,
    u64,
);

const COMPACT_CASES: [CompactCaseDefinition; 12] = [
    (
        "tiny-w1-t1-p1-tbl1-same",
        1,
        1,
        1,
        1,
        1,
        "same",
        "normal",
        0,
        1_000,
    ),
    (
        "tiny-w64-t1-p1-tbl1-same",
        1,
        64,
        1,
        1,
        1,
        "same",
        "normal",
        0,
        1_000,
    ),
    (
        "64k-w1-t1-p1-tbl1-same",
        64 * 1024,
        1,
        1,
        1,
        1,
        "same",
        "normal",
        0,
        1_000,
    ),
    (
        "64k-w64-t1-p1-tbl1-same",
        64 * 1024,
        64,
        1,
        1,
        1,
        "same",
        "normal",
        0,
        1_000,
    ),
    (
        "64k-w64-t1-p3-tbl1-same",
        64 * 1024,
        64,
        1,
        3,
        1,
        "same",
        "normal",
        0,
        1_000,
    ),
    (
        "64k-w64-t10-p3-tbl1-same",
        64 * 1024,
        64,
        10,
        3,
        1,
        "same",
        "normal",
        0,
        1_000,
    ),
    (
        "64k-w64-t10-p3-tbl8-dispersed",
        64 * 1024,
        64,
        10,
        3,
        8,
        "dispersed",
        "normal",
        0,
        1_000,
    ),
    (
        "64k-w64-t10-p3-tbl1-delayed",
        64 * 1024,
        64,
        10,
        3,
        1,
        "same",
        "delayed_test_only",
        60,
        1_000,
    ),
    (
        "1m-w32-t4-p3-tbl1-same",
        1024 * 1024,
        32,
        4,
        3,
        1,
        "same",
        "normal",
        0,
        256,
    ),
    (
        "8m-w8-t4-p3-tbl8-dispersed",
        8 * 1024 * 1024,
        8,
        4,
        3,
        8,
        "dispersed",
        "normal",
        0,
        64,
    ),
    (
        "32m-w1-t1-p1-tbl1-same",
        32 * 1024 * 1024,
        1,
        1,
        1,
        1,
        "same",
        "normal",
        0,
        16,
    ),
    (
        "32m-w4-t4-p3-tbl4-dispersed",
        32 * 1024 * 1024,
        4,
        4,
        3,
        4,
        "dispersed",
        "delayed_test_only",
        60,
        16,
    ),
];

/// Return the twelve-case compact Scribe matrix required by the closeout.
#[must_use]
pub fn compact_scribe_matrix() -> Vec<ScribeCompactCase> {
    COMPACT_CASES
        .into_iter()
        .map(
            |(
                id,
                frame_size_bytes,
                writers,
                tenants,
                pods,
                tables,
                table_distribution,
                fsync_mode,
                fsync_delay_ms,
                minimum_samples,
            )| ScribeCompactCase {
                id: id.to_owned(),
                frame_size_bytes,
                writers,
                tenants,
                pods,
                tables,
                table_distribution: table_distribution.to_owned(),
                routing_mode: if table_distribution == "same" {
                    "same_logical_table".to_owned()
                } else {
                    "dispersed_tenant_table_pod".to_owned()
                },
                fsync_mode: fsync_mode.to_owned(),
                fsync_delay_ms,
                minimum_samples,
            },
        )
        .collect()
}

/// A distribution captured from a production metric series.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScribeDistribution {
    /// Number of observations.
    pub count: u64,
    /// 50th percentile in microseconds or metric units.
    pub p50: u64,
    /// 95th percentile in microseconds or metric units.
    pub p95: u64,
    /// 99th percentile when at least 100 observations exist.
    pub p99: Option<u64>,
    /// Maximum observed value.
    pub max: u64,
}

/// Requested and observed writer placement for one case.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScribeTopologyEvidence {
    /// Requested writer count.
    pub requested_writers: u32,
    /// Writers actually launched.
    pub actual_writers: u32,
    /// Requested tenant count.
    pub requested_tenants: u32,
    /// Tenants actually provisioned.
    pub actual_tenants: u32,
    /// Requested pod count.
    pub requested_pods: u32,
    /// Pods actually used.
    pub actual_pods: u32,
    /// Requested logical table count.
    pub requested_logical_tables: u32,
    /// Logical tables actually routed.
    pub actual_logical_tables: u32,
    /// Physical tables observed after provisioning.
    pub actual_physical_tables: u32,
    /// Explicit routing mode.
    pub routing_mode: String,
    /// Writer counts by pod.
    pub writers_by_pod: BTreeMap<String, u32>,
    /// Writer counts by tenant.
    pub writers_by_tenant: BTreeMap<String, u32>,
    /// Writer counts by logical table.
    pub writers_by_table: BTreeMap<String, u32>,
}

/// Per-case absolute verification facts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScribeCaseVerification {
    /// Overall absolute checks that were exercised by this case.
    pub passed: bool,
    /// Whether all accepted frames became durable before clean shutdown.
    pub drain_zero_gap: bool,
    /// Whether replay identity was checked without duplicate rows/audits.
    pub replay_exact_identity: Option<bool>,
    /// Whether the case stayed within retained item and byte ceilings.
    pub retained_within_ceiling: bool,
    /// Exact 429 capacity behavior when a saturation probe ran.
    pub exact_429: Option<bool>,
    /// Exact 507 WAL exhaustion behavior when a disk probe ran.
    pub exact_507: Option<bool>,
}

/// Measurements emitted for one compact Scribe case.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ScribeCaseReport {
    /// Workload declaration.
    pub case: ScribeCompactCase,
    /// Requested and observed topology evidence.
    pub topology: ScribeTopologyEvidence,
    /// Frames sent after warmup.
    pub measured_frames: u64,
    /// Rows accepted by Gate/Scribe.
    pub admitted_rows: u64,
    /// Rows confirmed durable after drain.
    pub durable_rows: u64,
    /// Wall-clock duration in microseconds.
    pub elapsed_us: u64,
    /// Accepted frames per second.
    pub admitted_frames_per_second: f64,
    /// Durable frames per second.
    pub durable_frames_per_second: f64,
    /// Durable rows per second.
    pub rows_per_second: f64,
    /// Durable MiB per second.
    pub mib_per_second: f64,
    /// ACK latency distribution in microseconds.
    pub ack: ScribeDistribution,
    /// Gate resolution distribution in microseconds.
    pub resolution: ScribeDistribution,
    /// Ingress CPU distribution in microseconds.
    pub ingress: ScribeDistribution,
    /// WAL append distribution in microseconds.
    pub append: ScribeDistribution,
    /// WAL sync distribution in microseconds.
    pub sync: ScribeDistribution,
    /// Number of opportunistic groups completed.
    pub groups: u64,
    /// Frames per group distribution.
    pub frames_per_group: ScribeDistribution,
    /// Bytes per group distribution.
    pub bytes_per_group: ScribeDistribution,
    /// Number of fsync calls.
    pub fsync_calls: u64,
    /// Fsync calls divided by admitted frames.
    pub fsync_per_frame: f64,
    /// Accepted minus durable frame count.
    pub accepted_durable_frame_gap: u64,
    /// Accepted minus durable row count.
    pub accepted_durable_row_gap: u64,
    /// Current retained items at case end.
    pub retained_items_current: u64,
    /// Peak retained items.
    pub retained_items_peak: u64,
    /// Current retained bytes at case end.
    pub retained_bytes_current: u64,
    /// Peak retained bytes.
    pub retained_bytes_peak: u64,
    /// Peak queue depth by execution lane.
    pub lane_queue_peaks: BTreeMap<String, u64>,
    /// Peak active jobs by execution lane.
    pub lane_active_peaks: BTreeMap<String, u64>,
    /// Failed jobs by execution lane.
    pub lane_failures: BTreeMap<String, u64>,
    /// Panicked jobs by execution lane.
    pub lane_panics: BTreeMap<String, u64>,
    /// Writer unhealthy transitions observed in the interval.
    pub writer_unhealthy_transitions: u64,
    /// Required production metric names observed in the interval.
    pub required_metrics_observed: Vec<String>,
    /// Number of recorder series at case end.
    pub metric_series: usize,
    /// Whether the recorder cardinality cap was exceeded.
    pub metric_series_limit_exceeded: bool,
    /// Absolute verification facts.
    pub verification: ScribeCaseVerification,
}

/// Component benchmark measurement from a real production seam.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ScribeComponentReport {
    /// Stable component name.
    pub name: String,
    /// Production path used by the measurement.
    pub path: String,
    /// Explicit measurement provenance for the production seam.
    pub provenance: String,
    /// Warmup operations excluded from the measured sample.
    pub warmup_operations: u64,
    /// Fixture payload bytes represented by one operation.
    pub fixture_bytes: u64,
    /// Measured operations per second.
    pub operations_per_second: f64,
    /// Measured mebibytes per second.
    pub mib_per_second: f64,
    /// Number of measured operations.
    pub operations: u64,
    /// Processed bytes.
    pub bytes: u64,
    /// Elapsed wall-clock microseconds.
    pub elapsed_us: u64,
    /// Captured distribution.
    pub distribution: ScribeDistribution,
}

/// Configuration fingerprint for one benchmark stage.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScribeBenchmarkConfig {
    /// Number of bound server pods.
    pub pods: u32,
    /// Number of tenants available to the run.
    pub tenants: u32,
    /// Queue and admission configuration as resolved by the server.
    pub resolved: BTreeMap<String, String>,
    /// Compact matrix identifier.
    pub matrix: String,
}

/// Explicit comparison state for environments without a pre-repair baseline.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScribeComparisonStatus {
    /// Whether a comparable Stage A report was available.
    pub baseline_available: bool,
    /// `unavailable`, `passed`, or `failed`.
    pub status: String,
    /// Human-readable reason for an unavailable comparison.
    pub reason: String,
    /// Role of this artifact when no comparable baseline exists.
    pub baseline_role: String,
}

/// Complete post-repair Scribe benchmark artifact.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ScribeBenchmarkReport {
    /// Report schema version.
    pub report_version: String,
    /// `pre-throughput` or `post-throughput`.
    pub stage: String,
    /// Benchmark lane name.
    pub lane: String,
    /// Git and machine identity.
    pub machine: MachineMetadata,
    /// Human-readable environment fingerprint.
    pub environment_fingerprint: String,
    /// Configuration fingerprint used for future comparisons.
    pub configuration_fingerprint: String,
    /// Resolved benchmark configuration.
    pub configuration: ScribeBenchmarkConfig,
    /// Component measurements.
    pub components: Vec<ScribeComponentReport>,
    /// Compact matrix measurements.
    pub cases: Vec<ScribeCaseReport>,
    /// Full production metric snapshot.
    pub metrics: BenchmarkMetricSnapshot,
    /// Names required by the task contract.
    pub required_metric_families: Vec<String>,
    /// Absolute verification summary.
    pub verification: ScribeCaseVerification,
    /// Comparison status; relative throughput is never inferred.
    pub comparison: ScribeComparisonStatus,
    /// Report errors.
    pub errors: u64,
    /// Whether all required post-change measurements completed.
    pub complete: bool,
}

impl ScribeBenchmarkReport {
    /// Current Scribe report schema version.
    pub const VERSION: &'static str = "wyrd.bifrost.scribe.report/v1";

    /// Serialize the report as stable pretty JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Validate the closeout evidence without applying machine-dependent
    /// throughput floors.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        if !self.components.is_empty() {
            errors.extend(self.validate_components().err().unwrap_or_default());
        }
        if self.comparison.baseline_available
            || self.comparison.status != "unavailable"
            || self.comparison.reason != "no valid pre-repair baseline exists"
            || self.comparison.baseline_role != "initial_valid_post_repair"
        {
            errors.push("comparison metadata is not the explicit no-baseline state".to_owned());
        }
        let required = self
            .required_metric_families
            .iter()
            .collect::<std::collections::BTreeSet<_>>();
        for case in &self.cases {
            if case.topology.requested_writers != case.topology.actual_writers
                || case.topology.requested_tenants != case.topology.actual_tenants
                || case.topology.requested_pods != case.topology.actual_pods
                || case.topology.requested_logical_tables != case.topology.actual_logical_tables
                || case.topology.actual_physical_tables == 0
                || case.topology.actual_physical_tables
                    > case
                        .topology
                        .actual_tenants
                        .saturating_mul(case.topology.requested_logical_tables)
            {
                errors.push(format!("topology mismatch in case {}", case.case.id));
            }
            if !required.is_subset(
                &case
                    .required_metrics_observed
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>(),
            ) {
                errors.push(format!(
                    "required metric families missing in case {}",
                    case.case.id
                ));
            }
            if case.measured_frames < case.case.minimum_samples
                || case.metric_series_limit_exceeded
                || !case.verification.passed
                || !case.verification.drain_zero_gap
                || !case.verification.retained_within_ceiling
            {
                errors.push(format!("verification incomplete in case {}", case.case.id));
            }
            if case.case.fsync_mode == "delayed_test_only"
                && case.case.frame_size_bytes == 64 * 1024
                && case.fsync_per_frame > 0.25
            {
                errors.push(format!(
                    "delayed fsync grouping exceeded the ceiling in {}",
                    case.case.id
                ));
            }
            if case.case.frame_size_bytes <= 64 * 1024
                && case.ack.p99.is_some_and(|p99| p99 >= 5_000)
            {
                errors.push(format!("ACK p99 exceeded 5 ms in {}", case.case.id));
            }
        }
        let negative_flow_required = !self.lane.ends_with(":sustained");
        if negative_flow_required
            && (self.verification.replay_exact_identity != Some(true)
                || self.verification.exact_429 != Some(true)
                || self.verification.exact_507 != Some(true))
        {
            errors.push("top-level negative-flow evidence is incomplete".to_owned());
        }
        if self
            .required_metric_families
            .iter()
            .any(|metric| !self.metrics.contains_family(metric))
        {
            errors.push("report metric snapshot is missing a required family".to_owned());
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Validate the independent component evidence without applying
    /// machine-dependent throughput floors.
    pub fn validate_components(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        if std::env::var_os("WYRD_BIFROST_CLOSEOUT").is_some() && self.machine.dirty_worktree {
            errors.push("component report was generated from a dirty worktree".to_owned());
        }
        let expected = required_scribe_components()
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        let actual = self
            .components
            .iter()
            .map(|component| component.name.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        if actual != expected || actual.len() != self.components.len() {
            errors.push(format!(
                "component set must contain exactly {expected:?} without duplicates"
            ));
        }
        for component in &self.components {
            if !expected.contains(component.name.as_str()) {
                errors.push(format!("unknown component {}", component.name));
            }
            if component.path.trim().is_empty() || component.provenance.trim().is_empty() {
                errors.push(format!(
                    "component {} is missing measurement provenance",
                    component.name
                ));
            }
            if component.operations < 1_000 {
                errors.push(format!(
                    "component {} has fewer than 1000 operations",
                    component.name
                ));
            }
            if component.distribution.count != component.operations {
                errors.push(format!(
                    "component {} distribution count mismatch",
                    component.name
                ));
            }
            if component.fixture_bytes == 0 || component.bytes == 0 {
                errors.push(format!(
                    "component {} has no measured bytes",
                    component.name
                ));
            }
            if component.operations_per_second <= 0.0 || component.mib_per_second <= 0.0 {
                errors.push(format!("component {} has no measured rate", component.name));
            }
            if component.elapsed_us == 0 {
                errors.push(format!(
                    "component {} has no measured elapsed time",
                    component.name
                ));
            }
            if component.operations >= 100 && component.distribution.p99.is_none() {
                errors.push(format!("component {} is missing p99", component.name));
            }
            if component.operations < 100 && component.distribution.p99.is_some() {
                errors.push(format!(
                    "component {} reports p99 without 100 samples",
                    component.name
                ));
            }
            if component.name == "64_frame_append_one_sync"
                && !component.provenance.contains("frames=64")
            {
                errors.push("64_frame_append_one_sync is missing exact group evidence".to_owned());
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Write JSON and its adjacent concise Markdown summary.
    pub fn write_artifacts(&self, json_path: impl AsRef<Path>) -> Result<(), ReportError> {
        let json_path = json_path.as_ref();
        std::fs::write(
            json_path,
            format!(
                "{}\n",
                self.to_json()
                    .map_err(|error| ReportError::Serialize(error.to_string()))?
            ),
        )
        .map_err(|error| ReportError::Write {
            path: json_path.display().to_string(),
            message: error.to_string(),
        })?;
        let markdown_path = json_path.with_extension("md");
        std::fs::write(&markdown_path, self.to_markdown()).map_err(|error| ReportError::Write {
            path: markdown_path.display().to_string(),
            message: error.to_string(),
        })
    }

    /// Render the human-readable summary without dropping machine fields.
    #[must_use]
    pub fn to_markdown(&self) -> String {
        let mut output = format!(
            "# Bifrost Scribe benchmark\n\n- Stage: `{}`\n- Lane: `{}`\n- Complete: `{}`\n- Comparison: `{}` ({})\n- Environment: `{}`\n- Configuration: `{}`\n\n",
            self.stage,
            self.lane,
            self.complete,
            self.comparison.status,
            self.comparison.reason,
            self.environment_fingerprint,
            self.configuration_fingerprint,
        );
        output.push_str("| Case | Frames | Durable fps | ACK p99 (us) | Fsync/frame | Groups |\n|---|---:|---:|---:|---:|---:|\n");
        for case in &self.cases {
            let p99 = case
                .ack
                .p99
                .map_or_else(|| "unavailable".to_owned(), |value| value.to_string());
            let _ = writeln!(
                output,
                "| {} | {} | {:.2} | {} | {:.4} | {} |",
                case.case.id,
                case.measured_frames,
                case.durable_frames_per_second,
                p99,
                case.fsync_per_frame,
                case.groups,
            );
        }
        output.push_str("\n## Required metric families\n\n");
        for metric in &self.required_metric_families {
            let _ = writeln!(output, "- `{metric}`");
        }
        if !self.components.is_empty() {
            output.push_str("\n## Independent components\n\n");
            output.push_str("| Component | Operations | Bytes | Elapsed (us) | Ops/s | MiB/s | p50 | p95 | p99 | Max |\n|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n");
            for component in &self.components {
                let p99 = component
                    .distribution
                    .p99
                    .map_or_else(|| "unavailable".to_owned(), |value| value.to_string());
                let _ = writeln!(
                    output,
                    "| {} | {} | {} | {} | {:.2} | {:.2} | {} | {} | {} | {} |",
                    component.name,
                    component.operations,
                    component.bytes,
                    component.elapsed_us,
                    component.operations_per_second,
                    component.mib_per_second,
                    component.distribution.p50,
                    component.distribution.p95,
                    p99,
                    component.distribution.max,
                );
            }
            output.push_str("\n`p99` is unavailable when fewer than 100 measured samples exist.\n");
        }
        output
    }
}

/// Exact production seams required by the component closeout report.
#[must_use]
pub fn required_scribe_components() -> [&'static str; 7] {
    [
        "wal_prepare_crc_no_io",
        "vectored_append_no_sync",
        "sync_alone",
        "one_frame_append_sync",
        "64_frame_append_one_sync",
        "gate_ack",
        "sdk_gate_scribe",
    ]
}

/// Every production metric family required by the closeout.
#[must_use]
pub fn required_scribe_metric_families() -> Vec<String> {
    [
        "bifrost_gate_frames_total",
        "bifrost_gate_frame_bytes_total",
        "bifrost_gate_resolution_seconds",
        "bifrost_scribe_ack_seconds",
        "bifrost_scribe_admission_rejections_total",
        "bifrost_scribe_retained_items",
        "bifrost_scribe_retained_bytes",
        "bifrost_scribe_lane_queued",
        "bifrost_scribe_lane_active",
        "bifrost_scribe_lane_jobs_total",
        "bifrost_scribe_lane_job_seconds",
        "bifrost_scribe_writer_queue_depth",
        "bifrost_scribe_writer_groups_total",
        "bifrost_scribe_writer_group_frames",
        "bifrost_scribe_writer_group_bytes",
        "bifrost_scribe_wal_append_seconds",
        "bifrost_scribe_wal_sync_seconds",
        "bifrost_scribe_wal_fsync_total",
        "bifrost_scribe_wal_bytes_total",
        "bifrost_scribe_frames_total",
        "bifrost_scribe_rows_total",
        "bifrost_scribe_writer_unhealthy_total",
        "bifrost_scribe_replay_seconds",
        "bifrost_scribe_shutdown_seconds",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

/// Return the complete required Scribe matrix (240 configurations).
#[must_use]
pub fn required_scribe_matrix() -> Vec<ScribeMatrixCase> {
    [
        1_u64,
        64 * 1024,
        1024 * 1024,
        8 * 1024 * 1024,
        32 * 1024 * 1024,
    ]
    .into_iter()
    .flat_map(|batch_size_bytes| {
        [1_u32, 100, 1_000]
            .into_iter()
            .flat_map(move |tenant_count| {
                ["same", "dispersed"]
                    .into_iter()
                    .flat_map(move |seal_key_pattern| {
                        [false, true].into_iter().flat_map(move |cross_day| {
                            ["normal", "delayed"]
                                .into_iter()
                                .flat_map(move |fsync_mode| {
                                    [false, true].into_iter().map(move |noisy_tenant| {
                                        ScribeMatrixCase {
                                            batch_size_bytes,
                                            tenant_count,
                                            seal_key_pattern: seal_key_pattern.to_owned(),
                                            cross_day,
                                            fsync_mode: fsync_mode.to_owned(),
                                            noisy_tenant,
                                        }
                                    })
                                })
                        })
                    })
            })
    })
    .collect()
}

/// One complete machine-readable Bifrost benchmark report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkReport {
    /// Report schema version.
    pub report_version: String,
    /// Named lane that produced this report.
    pub lane: String,
    /// Reproducible workload declaration.
    pub workload: WorkloadSpec,
    /// Batch size used by the run.
    pub batch_size: u64,
    /// Topology metadata.
    pub pods: PodMetadata,
    /// Normalized machine metadata.
    pub machine: MachineMetadata,
    /// Write latency percentiles.
    pub write_latency: LatencyPercentiles,
    /// Query latency percentiles.
    pub query_latency: LatencyPercentiles,
    /// Storage measurements.
    pub storage: StorageMeasurements,
    /// Query and audit measurements.
    pub query: QueryMeasurements,
    /// Forge compaction counters and backlog peak.
    pub forge: ForgeMeasurements,
    /// Required Scribe matrix configurations for this benchmark family.
    pub scribe_matrix: Vec<ScribeMatrixCase>,
    /// Per-stage measurements from the real data path.
    pub stages: Vec<StageMeasurements>,
    /// Phase-level workload measurements.
    pub phases: Vec<PhaseMeasurement>,
    /// Backlog samples collected during the run.
    pub backlog: Vec<BacklogSample>,
    /// Durable output verification.
    pub verification: VerificationMeasurements,
    /// Total producer, sealing, or Forge errors observed during the run.
    pub errors: u64,
    /// False when a timeout or verification failure ended the run.
    pub complete: bool,
}

impl BenchmarkReport {
    /// Current structured report version.
    pub const VERSION: &'static str = "wyrd.bifrost.report/v2";

    /// Serialize a report as stable, pretty JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Write a report to a caller-selected artifact path.
    pub fn write_json(&self, path: impl AsRef<Path>) -> Result<(), ReportError> {
        let bytes = self
            .to_json()
            .map_err(|error| ReportError::Serialize(error.to_string()))?;
        std::fs::write(path.as_ref(), format!("{bytes}\n")).map_err(|error| ReportError::Write {
            path: path.as_ref().display().to_string(),
            message: error.to_string(),
        })
    }
}

/// Report serialization or artifact-write failure.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReportError {
    /// JSON serialization failed.
    #[error("failed to serialize benchmark report: {0}")]
    Serialize(String),
    /// Report artifact could not be written.
    #[error("failed to write benchmark report {path}: {message}")]
    Write {
        /// Artifact path.
        path: String,
        /// IO error.
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workload::{SchemaWidth, TrafficShape};

    #[test]
    fn percentile_math_is_deterministic() {
        let percentiles = LatencyPercentiles::from_samples(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        assert_eq!(percentiles.p50_us, 5);
        assert_eq!(percentiles.p95_us, 10);
        assert_eq!(percentiles.p999_us, 10);
    }

    #[test]
    fn report_round_trips_structured_json() {
        let report = BenchmarkReport {
            report_version: BenchmarkReport::VERSION.to_owned(),
            lane: "bench:bifrost:scribe:slo".to_owned(),
            workload: WorkloadSpec::required("test", 7, TrafficShape::Steady, SchemaWidth::Narrow),
            batch_size: 1_000,
            pods: PodMetadata {
                pod_count: 1,
                pod_ids: vec!["pod-0".to_owned()],
            },
            machine: MachineMetadata::default(),
            write_latency: LatencyPercentiles::default(),
            query_latency: LatencyPercentiles::default(),
            storage: StorageMeasurements::default(),
            query: QueryMeasurements::default(),
            forge: ForgeMeasurements::default(),
            scribe_matrix: Vec::new(),
            stages: Vec::new(),
            phases: Vec::new(),
            backlog: Vec::new(),
            verification: VerificationMeasurements::default(),
            errors: 0,
            complete: true,
        };
        let json = report.to_json().expect("report serializes");
        let decoded: BenchmarkReport = serde_json::from_str(&json).expect("report parses");
        assert_eq!(decoded, report);
    }

    #[test]
    fn scribe_matrix_covers_every_required_dimension() {
        let matrix = required_scribe_matrix();
        assert_eq!(matrix.len(), 240);
        assert!(matrix.iter().any(|case| {
            case.batch_size_bytes == 32 * 1024 * 1024
                && case.tenant_count == 1_000
                && case.seal_key_pattern == "dispersed"
                && case.cross_day
                && case.fsync_mode == "delayed"
                && case.noisy_tenant
        }));
    }

    #[test]
    fn compact_scribe_matrix_has_the_locked_twelve_cases() {
        let matrix = compact_scribe_matrix();
        assert_eq!(matrix.len(), 12);
        assert_eq!(matrix[7].fsync_delay_ms, 60);
        assert_eq!(matrix[7].minimum_samples, 1_000);
        assert_eq!(matrix[10].frame_size_bytes, 32 * 1024 * 1024);
        assert_eq!(matrix[11].writers, 4);
        assert_eq!(matrix[11].tables, 4);
    }

    #[test]
    fn scribe_report_renders_json_and_markdown_without_comparison() {
        let report = ScribeBenchmarkReport {
            report_version: ScribeBenchmarkReport::VERSION.to_owned(),
            stage: "post-throughput".to_owned(),
            lane: "bench:bifrost:scribe:slo".to_owned(),
            comparison: ScribeComparisonStatus {
                status: "unavailable".to_owned(),
                reason: "pre-repair baseline was not captured".to_owned(),
                baseline_role: "initial_valid_post_repair".to_owned(),
                ..ScribeComparisonStatus::default()
            },
            ..ScribeBenchmarkReport::default()
        };
        let json = report.to_json().expect("scribe report serializes");
        assert!(json.contains("post-throughput"));
        assert!(report.to_markdown().contains("unavailable"));
    }

    #[test]
    fn scribe_report_validation_rejects_missing_topology_metrics_and_negative_flows() {
        let report = ScribeBenchmarkReport {
            cases: vec![ScribeCaseReport {
                case: compact_scribe_matrix()
                    .into_iter()
                    .next()
                    .expect("compact case"),
                ..ScribeCaseReport::default()
            }],
            required_metric_families: vec!["bifrost_gate_frames_total".to_owned()],
            ..ScribeBenchmarkReport::default()
        };
        let errors = report
            .validate()
            .expect_err("incomplete evidence must fail closed");
        assert!(errors.iter().any(|error| error.contains("topology")));
        assert!(errors.iter().any(|error| error.contains("metric")));
        assert!(errors.iter().any(|error| error.contains("negative-flow")));
    }

    fn valid_component(name: &str) -> ScribeComponentReport {
        ScribeComponentReport {
            name: name.to_owned(),
            path: "production seam".to_owned(),
            provenance: if name == "64_frame_append_one_sync" {
                "frames=64; segments=1; fsyncs=1".to_owned()
            } else {
                "independent wall-clock samples".to_owned()
            },
            warmup_operations: 100,
            fixture_bytes: 64,
            operations: 1_000,
            bytes: 64_000,
            elapsed_us: 1_000,
            operations_per_second: 1_000_000.0,
            mib_per_second: 61.0,
            distribution: ScribeDistribution {
                count: 1_000,
                p50: 1,
                p95: 2,
                p99: Some(3),
                max: 4,
            },
        }
    }

    #[test]
    fn component_report_validator_requires_exact_independent_set() {
        let components = required_scribe_components()
            .into_iter()
            .map(valid_component)
            .collect::<Vec<_>>();
        let report = ScribeBenchmarkReport {
            components,
            ..ScribeBenchmarkReport::default()
        };
        assert!(report.validate_components().is_ok());

        let mut duplicate = report.clone();
        duplicate.components[0].name = duplicate.components[1].name.clone();
        let errors = duplicate
            .validate_components()
            .expect_err("duplicate component names must fail");
        assert!(errors.iter().any(|error| error.contains("exactly")));

        let mut derived = report;
        derived.components[0].elapsed_us =
            derived.components[0].distribution.count * derived.components[0].distribution.p50;
        derived.components[0].provenance.clear();
        let errors = derived
            .validate_components()
            .expect_err("missing provenance must fail");
        assert!(errors.iter().any(|error| error.contains("provenance")));
    }
}
