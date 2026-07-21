//! Structured Bifrost benchmark reports and percentile math.

use std::path::Path;

use serde::{Deserialize, Serialize};

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
    /// Per-stage measurements from the real data path.
    pub stages: Vec<StageMeasurements>,
    /// Phase-level workload measurements.
    pub phases: Vec<PhaseMeasurement>,
    /// Backlog samples collected during the run.
    pub backlog: Vec<BacklogSample>,
    /// Durable output verification.
    pub verification: VerificationMeasurements,
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
            stages: Vec::new(),
            phases: Vec::new(),
            backlog: Vec::new(),
            verification: VerificationMeasurements::default(),
            complete: true,
        };
        let json = report.to_json().expect("report serializes");
        let decoded: BenchmarkReport = serde_json::from_str(&json).expect("report parses");
        assert_eq!(decoded, report);
    }
}
