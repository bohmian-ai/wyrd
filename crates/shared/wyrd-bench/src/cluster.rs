//! Versioned full-cluster Bifrost reference benchmark contracts.
//!
//! This module owns report identity and qualification math only. The real
//! Gate-to-Oracle workload remains in `wyrd-testing` so this crate stays free
//! of server, SQL, storage, and runtime dependencies.

use std::collections::{BTreeMap, BTreeSet};

use num_traits::ToPrimitive;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Closed schema identifier for full-cluster reference reports.
pub const CLUSTER_REPORT_VERSION: &str = "wyrd.bifrost.cluster-report/v1";
/// Stable workload identifier shared by captures and comparisons.
pub const CLUSTER_WORKLOAD_VERSION: &str = "wyrd.bifrost.cluster-workload/v1";
/// First open-loop calibration probe in public operations per second.
pub const FIRST_PROBE_RATE: u64 = 8;
/// Maximum calibration probe in public operations per second.
pub const CALIBRATION_RATE_CAP: u64 = 4_096;
/// Planned measured flush cadence in seconds.
pub const FLUSH_CADENCE_SECONDS: u64 = 2;
/// Number of planned flushes in one 20-second measured trial.
pub const PLANNED_FLUSHES_PER_TRIAL: u64 = 10;

/// Physical cluster topology exercised by one scenario.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClusterTopology {
    /// One Server process owns Gate, Scribe, Forge, and Oracle.
    OnePod,
    /// Three Server processes and three dedicated `ForgeWorker` processes.
    ThreeServersThreeForgeWorkers,
}

/// Public-operation mix exercised by one scenario.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrafficMix {
    /// Ninety-percent durable writes and ten-percent strict queries.
    WriteHeavy,
    /// Ten-percent durable writes and ninety-percent strict queries.
    ReadHeavy,
    /// Simultaneous durable writes and strict reads.
    Balanced,
}

/// Provenance for a calibrated saturation knee.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KneeProvenance {
    /// The first failing probe bounded the last passing absolute rate.
    Discovered,
    /// The 4,096 request/second cap passed, so the knee is a lower bound.
    CensoredAtCap,
}

/// Exact workload identity required for compatible comparison.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClusterBenchmarkScenario {
    /// Stable scenario identifier.
    pub scenario_id: String,
    /// Workload contract version.
    pub workload_version: String,
    /// Physical process topology.
    pub topology: ClusterTopology,
    /// Number of isolated data tenants.
    pub tenants: u32,
    /// Public-operation mix.
    pub traffic: TrafficMix,
    /// Rows carried by each durable write.
    pub rows_per_batch: u32,
    /// Maximum rows returned by each bounded strict query.
    pub query_row_limit: u32,
    /// Absolute public operations offered each second.
    pub offered_requests_per_second: u64,
    /// Percent of the reference knee represented by the absolute rate.
    pub offered_load_percent: u8,
    /// Reference calibration provenance.
    pub knee_provenance: KneeProvenance,
    /// Warmup duration in seconds.
    pub warmup_seconds: u32,
    /// Measured duration in seconds.
    pub measured_seconds: u32,
    /// Independent trial count.
    pub trials: u8,
    /// Minimum write and query samples required in each relevant trial.
    pub minimum_samples: u64,
    /// Stable workload seed.
    pub seed: u64,
}

impl ClusterBenchmarkScenario {
    /// Validate the closed reference-scenario contract.
    ///
    /// # Errors
    /// Returns [`ClusterBenchmarkError::Invalid`] when cardinality, timing,
    /// load, or version values depart from the controlled reference contract.
    pub fn validate(&self) -> Result<(), ClusterBenchmarkError> {
        if self.workload_version != CLUSTER_WORKLOAD_VERSION
            || self.tenants == 0
            || self.rows_per_batch != 64
            || self.query_row_limit != 64
            || self.offered_requests_per_second == 0
            || !matches!(self.offered_load_percent, 50 | 75 | 100)
            || self.warmup_seconds != 5
            || self.measured_seconds != 20
            || self.trials != 3
            || self.minimum_samples != 200
            || self.seed != 0xB1_F057
        {
            return Err(ClusterBenchmarkError::Invalid(format!(
                "scenario {} violates the v1 reference contract",
                self.scenario_id
            )));
        }
        Ok(())
    }
}

/// Closed, machine-readable environment identity for reference qualification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkEnvironment {
    /// CPU architecture, such as `aarch64` or `x86_64`.
    pub architecture: String,
    /// Classified CPU vendor: `intel`, `amd`, or `apple`.
    pub cpu_vendor: String,
    /// Deterministically normalized CPU model.
    pub cpu_model: String,
    /// Logical CPU count visible to the host.
    pub logical_cores: u32,
    /// Total host memory in bytes.
    pub host_memory_bytes: u64,
    /// Exact CPU limit assigned to benchmark containers, in milli-CPUs.
    pub container_cpu_millis: u32,
    /// Exact memory limit assigned to benchmark containers, in bytes.
    pub container_memory_bytes: u64,
    /// Exact Postgres image reference.
    pub postgres_image: String,
    /// Exact Postgres server version.
    pub postgres_version: String,
    /// Stable Postgres configuration identity.
    pub postgres_config: String,
    /// Exact storage image or implementation identity.
    pub storage_image: String,
    /// Exact storage version.
    pub storage_version: String,
    /// Stable storage configuration identity.
    pub storage_config: String,
    /// Rust compiler major/minor pair.
    pub rust_major_minor: String,
    /// Locked Arrow version.
    pub arrow_version: String,
    /// Locked `DataFusion` version.
    pub datafusion_version: String,
    /// Locked Iceberg source revision/version.
    pub iceberg_version: String,
    /// Operating-system identity recorded for diagnostics.
    pub os: String,
    /// Kernel identity recorded for diagnostics.
    pub kernel: String,
    /// Git revision recorded for diagnostics.
    pub git_sha: String,
    /// Whether tracked or untracked source changes existed during capture.
    pub dirty_worktree: bool,
    /// RFC3339 capture timestamp recorded for diagnostics.
    pub captured_at: String,
}

impl BenchmarkEnvironment {
    /// Validate that every required identity field is classified and bounded.
    ///
    /// # Errors
    /// Returns [`ClusterBenchmarkError::Unsupported`] when an identity needed
    /// for controlled comparison is absent or unclassifiable.
    pub fn validate(&self) -> Result<(), ClusterBenchmarkError> {
        let required = [
            self.architecture.as_str(),
            self.cpu_model.as_str(),
            self.postgres_image.as_str(),
            self.postgres_version.as_str(),
            self.postgres_config.as_str(),
            self.storage_image.as_str(),
            self.storage_version.as_str(),
            self.storage_config.as_str(),
            self.rust_major_minor.as_str(),
            self.arrow_version.as_str(),
            self.datafusion_version.as_str(),
            self.iceberg_version.as_str(),
            self.os.as_str(),
            self.kernel.as_str(),
            self.git_sha.as_str(),
            self.captured_at.as_str(),
        ];
        if required.iter().any(|value| value.trim().is_empty())
            || !matches!(self.cpu_vendor.as_str(), "intel" | "amd" | "apple")
            || self.logical_cores == 0
            || self.host_memory_bytes == 0
            || self.container_cpu_millis == 0
            || self.container_memory_bytes == 0
        {
            return Err(ClusterBenchmarkError::Unsupported(
                "reference environment identity is incomplete or unclassifiable".to_owned(),
            ));
        }
        Ok(())
    }

    /// Compare only D22's compatibility-defining environment fields.
    ///
    /// Git SHA, capture timestamp, OS, and patch-level kernel are deliberately
    /// diagnostic. Host memory may vary by at most five percent.
    #[must_use]
    pub fn compatible_with(&self, candidate: &Self) -> bool {
        let memory_delta = self.host_memory_bytes.abs_diff(candidate.host_memory_bytes);
        let memory_compatible =
            memory_delta.saturating_mul(100) <= self.host_memory_bytes.saturating_mul(5);
        self.architecture == candidate.architecture
            && self.cpu_vendor == candidate.cpu_vendor
            && self.cpu_model == candidate.cpu_model
            && self.logical_cores == candidate.logical_cores
            && memory_compatible
            && self.container_cpu_millis == candidate.container_cpu_millis
            && self.container_memory_bytes == candidate.container_memory_bytes
            && self.postgres_image == candidate.postgres_image
            && self.postgres_version == candidate.postgres_version
            && self.postgres_config == candidate.postgres_config
            && self.storage_image == candidate.storage_image
            && self.storage_version == candidate.storage_version
            && self.storage_config == candidate.storage_config
            && self.rust_major_minor == candidate.rust_major_minor
            && self.arrow_version == candidate.arrow_version
            && self.datafusion_version == candidate.datafusion_version
            && self.iceberg_version == candidate.iceberg_version
    }
}

/// One operation distribution from a single independent trial.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrialDistribution {
    /// Number of recorded samples.
    pub samples: u64,
    /// Median latency in microseconds.
    pub p50_us: u64,
    /// 95th percentile latency in microseconds.
    pub p95_us: u64,
    /// 99th percentile latency in microseconds.
    pub p99_us: u64,
    /// Maximum latency in microseconds.
    pub max_us: u64,
    /// Whether the HDR histogram overflowed.
    pub overflowed: bool,
}

/// Client-observed measurements from one trial.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientTrialMetrics {
    /// Durable write acknowledgement latency.
    pub durable_write: TrialDistribution,
    /// Explicit flush acknowledgement-to-strict-visibility latency.
    pub flush_to_visible: TrialDistribution,
    /// Strict query time to first frame.
    pub query_time_to_first_frame: TrialDistribution,
    /// Total strict query latency.
    pub total_query: TrialDistribution,
    /// Durable acknowledged rows per measured second.
    pub durable_rows_per_second: f64,
    /// Successful strict queries per measured second.
    pub queries_per_second: f64,
    /// Decoded strict-query rows per measured second.
    pub query_rows_per_second: f64,
    /// Stable public rejections divided by submitted operations.
    pub backpressure_ratio: f64,
    /// Client retries divided by accepted operations.
    pub retry_ratio: f64,
    /// Jain fairness over per-tenant durable rows.
    pub write_fairness: f64,
    /// Jain fairness over per-tenant successful queries.
    pub read_fairness: f64,
    /// Open-loop operations missed by more than one dispatch interval.
    pub missed_operations: u64,
    /// Explicit flushes whose fixed cadence was missed.
    pub missed_flushes: u64,
}

/// Production-pillar reconciliation evidence for one trial.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionTelemetryEvidence {
    /// Gate-accepted write rows.
    pub gate_accepted_rows: u64,
    /// Scribe-accepted rows.
    pub scribe_accepted_rows: u64,
    /// Client-acknowledged durable rows.
    pub client_acknowledged_rows: u64,
    /// Persisted unique Scribe rows after convergence.
    pub scribe_persisted_rows: u64,
    /// Published unique rows after Forge convergence.
    pub forge_published_rows: u64,
    /// Oracle decoded stream rows across successful attempts.
    pub oracle_stream_rows: u64,
    /// Client decoded rows across successful attempts.
    pub client_decoded_rows: u64,
    /// Completed Oracle stream terminal outcomes.
    pub oracle_terminal_outcomes: u64,
    /// Completed client queries.
    pub client_completed_queries: u64,
    /// Durable audit transitions expected by the scenario.
    pub expected_audit_rows: u64,
    /// Durable audit transitions observed by the scenario.
    pub observed_audit_rows: u64,
    /// Required production metric-family and span evidence.
    pub required_telemetry: EvidenceStatus,
    /// Monotonic production-counter integrity.
    pub counter_integrity: EvidenceStatus,
    /// Production span error-state evidence.
    pub spans_clean: EvidenceStatus,
    /// Final lease, slot, fence, claim, task, and listener evidence.
    pub cleanup: EvidenceStatus,
    /// Exact tenant and data correctness evidence.
    pub correctness: EvidenceStatus,
}

/// Closed result for one required benchmark evidence family.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceStatus {
    /// The evidence was absent, invalid, reset, or unreconciled.
    #[default]
    Failed,
    /// The evidence was present and reconciled.
    Complete,
}

impl ProductionTelemetryEvidence {
    /// Return whether client and production ledgers reconcile exactly.
    #[must_use]
    pub fn reconciles(&self) -> bool {
        self.gate_accepted_rows == self.scribe_accepted_rows
            && self.scribe_accepted_rows == self.client_acknowledged_rows
            && self.scribe_persisted_rows == self.client_acknowledged_rows
            && self.forge_published_rows == self.client_acknowledged_rows
            && self.oracle_stream_rows == self.client_decoded_rows
            && self.oracle_terminal_outcomes == self.client_completed_queries
            && self.expected_audit_rows == self.observed_audit_rows
            && self.required_telemetry == EvidenceStatus::Complete
            && self.counter_integrity == EvidenceStatus::Complete
            && self.spans_clean == EvidenceStatus::Complete
            && self.cleanup == EvidenceStatus::Complete
            && self.correctness == EvidenceStatus::Complete
    }
}

/// One independent measured trial at one absolute offered rate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClusterBenchmarkTrial {
    /// One-based trial number.
    pub trial: u8,
    /// Client-observed metrics.
    pub client: ClientTrialMetrics,
    /// Production telemetry reconciliation evidence.
    pub production: ProductionTelemetryEvidence,
}

/// Derived median-of-three metrics for one scenario/load.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MedianMetrics {
    /// Median durable rows per second.
    pub durable_rows_per_second: f64,
    /// Median queries per second.
    pub queries_per_second: f64,
    /// Median decoded query rows per second.
    pub query_rows_per_second: f64,
    /// Median durable-write p95 in microseconds.
    pub durable_write_p95_us: u64,
    /// Median durable-write p99 in microseconds.
    pub durable_write_p99_us: u64,
    /// Median flush-to-visible p95 in microseconds.
    pub flush_to_visible_p95_us: u64,
    /// Median flush-to-visible p99 in microseconds.
    pub flush_to_visible_p99_us: u64,
    /// Median query TTFB p95 in microseconds.
    pub query_ttfb_p95_us: u64,
    /// Median query TTFB p99 in microseconds.
    pub query_ttfb_p99_us: u64,
    /// Median total-query p95 in microseconds.
    pub total_query_p95_us: u64,
    /// Median total-query p99 in microseconds.
    pub total_query_p99_us: u64,
    /// Median retry ratio.
    pub retry_ratio: f64,
    /// Median backpressure ratio.
    pub backpressure_ratio: f64,
    /// Median write fairness.
    pub write_fairness: f64,
    /// Median read fairness.
    pub read_fairness: f64,
}

/// Complete three-trial result for one scenario and absolute load.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClusterScenarioReport {
    /// Exact scenario and rate identity.
    pub scenario: ClusterBenchmarkScenario,
    /// Independently discovered candidate/reference knee.
    pub discovered_knee_requests_per_second: u64,
    /// Provenance of the independently discovered knee.
    pub discovered_knee_provenance: KneeProvenance,
    /// Three independent trials.
    pub trials: Vec<ClusterBenchmarkTrial>,
    /// Median of the three independent trial summaries.
    pub median: MedianMetrics,
}

impl ClusterScenarioReport {
    /// Validate sample, cadence, telemetry, and median cardinality invariants.
    ///
    /// # Errors
    /// Returns `Unsupported` for insufficient evidence and `NotReady` for a
    /// failed operation, cadence, telemetry, correctness, or cleanup invariant.
    pub fn validate(&self) -> Result<(), ClusterBenchmarkError> {
        self.scenario.validate()?;
        if self.trials.len() != usize::from(self.scenario.trials) {
            return Err(ClusterBenchmarkError::Unsupported(
                "scenario does not contain exactly three trials".to_owned(),
            ));
        }
        let needs_write = true;
        let needs_read = true;
        for trial in &self.trials {
            if (needs_write && trial.client.durable_write.samples < self.scenario.minimum_samples)
                || (needs_read
                    && (trial.client.query_time_to_first_frame.samples
                        < self.scenario.minimum_samples
                        || trial.client.total_query.samples < self.scenario.minimum_samples))
            {
                return Err(ClusterBenchmarkError::Unsupported(
                    "trial has fewer than 200 required operation samples".to_owned(),
                ));
            }
            if trial.client.flush_to_visible.samples != PLANNED_FLUSHES_PER_TRIAL
                || trial.client.missed_flushes != 0
            {
                return Err(ClusterBenchmarkError::NotReady(
                    "trial did not record exactly ten fixed-cadence flushes".to_owned(),
                ));
            }
            if trial.client.missed_operations != 0
                || trial.client.durable_write.overflowed
                || trial.client.flush_to_visible.overflowed
                || trial.client.query_time_to_first_frame.overflowed
                || trial.client.total_query.overflowed
                || !trial.production.reconciles()
            {
                return Err(ClusterBenchmarkError::NotReady(
                    "trial contains missed work, overflow, or unreconciled production evidence"
                        .to_owned(),
                ));
            }
        }
        if self.median != derive_trial_median(&self.trials)? {
            return Err(ClusterBenchmarkError::Invalid(
                "persisted median does not equal the median of three trial summaries".to_owned(),
            ));
        }
        let flush_samples = self
            .trials
            .iter()
            .map(|trial| trial.client.flush_to_visible.samples)
            .sum::<u64>();
        if flush_samples < 30 {
            return Err(ClusterBenchmarkError::Unsupported(
                "scenario/load has fewer than thirty explicit flush samples".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Derive the complete comparison summary from exactly three trials.
///
/// # Errors
/// Returns [`ClusterBenchmarkError::Unsupported`] when the trial cardinality
/// is not three or a floating measurement is not finite.
pub fn derive_trial_median(
    trials: &[ClusterBenchmarkTrial],
) -> Result<MedianMetrics, ClusterBenchmarkError> {
    let float = |field: fn(&ClientTrialMetrics) -> f64| {
        median_three_f64(
            &trials
                .iter()
                .map(|trial| field(&trial.client))
                .collect::<Vec<_>>(),
        )
    };
    let integer = |field: fn(&ClientTrialMetrics) -> u64| {
        median_three_u64(
            &trials
                .iter()
                .map(|trial| field(&trial.client))
                .collect::<Vec<_>>(),
        )
    };
    Ok(MedianMetrics {
        durable_rows_per_second: float(|metrics| metrics.durable_rows_per_second)?,
        queries_per_second: float(|metrics| metrics.queries_per_second)?,
        query_rows_per_second: float(|metrics| metrics.query_rows_per_second)?,
        durable_write_p95_us: integer(|metrics| metrics.durable_write.p95_us)?,
        durable_write_p99_us: integer(|metrics| metrics.durable_write.p99_us)?,
        flush_to_visible_p95_us: integer(|metrics| metrics.flush_to_visible.p95_us)?,
        flush_to_visible_p99_us: integer(|metrics| metrics.flush_to_visible.p99_us)?,
        query_ttfb_p95_us: integer(|metrics| metrics.query_time_to_first_frame.p95_us)?,
        query_ttfb_p99_us: integer(|metrics| metrics.query_time_to_first_frame.p99_us)?,
        total_query_p95_us: integer(|metrics| metrics.total_query.p95_us)?,
        total_query_p99_us: integer(|metrics| metrics.total_query.p99_us)?,
        retry_ratio: float(|metrics| metrics.retry_ratio)?,
        backpressure_ratio: float(|metrics| metrics.backpressure_ratio)?,
        write_fairness: float(|metrics| metrics.write_fairness)?,
        read_fairness: float(|metrics| metrics.read_fairness)?,
    })
}

/// Absolute product SLOs applied only to a controlled reference profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BifrostSloEnvelope {
    /// Maximum durable-write p99 in microseconds.
    pub durable_write_p99_us: u64,
    /// Maximum flush-to-strict-visibility p99 in microseconds.
    pub flush_to_visible_p99_us: u64,
    /// Maximum bounded-query TTFB p99 in microseconds.
    pub query_ttfb_p99_us: u64,
}

impl Default for BifrostSloEnvelope {
    fn default() -> Self {
        Self {
            durable_write_p99_us: 100_000,
            flush_to_visible_p99_us: 5_000_000,
            query_ttfb_p99_us: 500_000,
        }
    }
}

/// Checked-in or captured full-cluster reference envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BifrostReferenceProfile {
    /// Exact report schema version.
    pub schema_version: String,
    /// Closed environment identity.
    pub environment: BenchmarkEnvironment,
    /// Eighteen scenario/load results: six scenarios times three rates.
    pub scenarios: Vec<ClusterScenarioReport>,
    /// Absolute reference SLOs.
    pub slos: BifrostSloEnvelope,
}

impl BifrostReferenceProfile {
    /// Validate schema, environment, matrix identity, evidence, and absolute SLOs.
    ///
    /// # Errors
    /// Returns a typed invalid, unsupported, or not-ready result. Dirty source
    /// is explicitly unsupported for reference blessing.
    pub fn validate_reference(&self) -> Result<(), ClusterBenchmarkError> {
        if self.schema_version != CLUSTER_REPORT_VERSION {
            return Err(ClusterBenchmarkError::Incompatible(format!(
                "expected {CLUSTER_REPORT_VERSION}, got {}",
                self.schema_version
            )));
        }
        self.environment.validate()?;
        if self.environment.dirty_worktree {
            return Err(ClusterBenchmarkError::Unsupported(
                "dirty worktree captures cannot be blessed as a reference".to_owned(),
            ));
        }
        if self.scenarios.len() != 18 {
            return Err(ClusterBenchmarkError::Unsupported(
                "reference requires six scenarios at three absolute rates".to_owned(),
            ));
        }
        let mut identities = BTreeSet::new();
        for report in &self.scenarios {
            report.validate()?;
            validate_fixed_scenario(report)?;
            let key = (
                report.scenario.scenario_id.as_str(),
                report.scenario.offered_load_percent,
            );
            if !identities.insert(key) {
                return Err(ClusterBenchmarkError::Invalid(format!(
                    "duplicate scenario/load identity {}:{}",
                    key.0, key.1
                )));
            }
            let median = &report.median;
            if median.durable_write_p99_us > self.slos.durable_write_p99_us {
                return Err(ClusterBenchmarkError::NotReady(
                    "durable-write p99 exceeds the absolute reference SLO".to_owned(),
                ));
            }
            if median.flush_to_visible_p99_us > self.slos.flush_to_visible_p99_us {
                return Err(ClusterBenchmarkError::NotReady(
                    "flush-to-visible p99 exceeds the absolute reference SLO".to_owned(),
                ));
            }
            if median.query_ttfb_p99_us > self.slos.query_ttfb_p99_us {
                return Err(ClusterBenchmarkError::NotReady(
                    "query TTFB p99 exceeds the absolute reference SLO".to_owned(),
                ));
            }
        }
        for (id, topology, tenants, traffic) in fixed_scenarios() {
            let reports = self
                .scenarios
                .iter()
                .filter(|report| report.scenario.scenario_id == id)
                .collect::<Vec<_>>();
            if reports.len() != 3
                || reports.iter().any(|report| {
                    report.scenario.topology != topology
                        || report.scenario.tenants != tenants
                        || report.scenario.traffic != traffic
                })
            {
                return Err(ClusterBenchmarkError::Invalid(format!(
                    "reference matrix is missing exact scenario `{id}`"
                )));
            }
            let knee = reports[0].discovered_knee_requests_per_second;
            let provenance = reports[0].discovered_knee_provenance;
            if knee < FIRST_PROBE_RATE
                || (provenance == KneeProvenance::CensoredAtCap && knee != CALIBRATION_RATE_CAP)
            {
                return Err(ClusterBenchmarkError::Invalid(format!(
                    "scenario `{id}` has an invalid calibrated knee"
                )));
            }
            for report in reports {
                if report.discovered_knee_requests_per_second != knee
                    || report.discovered_knee_provenance != provenance
                    || report.scenario.knee_provenance != provenance
                {
                    return Err(ClusterBenchmarkError::Invalid(format!(
                        "scenario `{id}` has inconsistent knee or provenance"
                    )));
                }
            }
        }
        Ok(())
    }
}

/// Return D22's fixed scenario identities without runtime dependencies.
fn fixed_scenarios() -> [(&'static str, ClusterTopology, u32, TrafficMix); 6] {
    [
        (
            "balanced-one-pod-one-tenant",
            ClusterTopology::OnePod,
            1,
            TrafficMix::Balanced,
        ),
        (
            "balanced-one-pod-eight-tenants",
            ClusterTopology::OnePod,
            8,
            TrafficMix::Balanced,
        ),
        (
            "balanced-three-server-three-worker-eight-tenants",
            ClusterTopology::ThreeServersThreeForgeWorkers,
            8,
            TrafficMix::Balanced,
        ),
        (
            "balanced-three-server-three-worker-thirty-two-tenants",
            ClusterTopology::ThreeServersThreeForgeWorkers,
            32,
            TrafficMix::Balanced,
        ),
        (
            "write-heavy-three-server-three-worker-eight-tenants",
            ClusterTopology::ThreeServersThreeForgeWorkers,
            8,
            TrafficMix::WriteHeavy,
        ),
        (
            "read-heavy-three-server-three-worker-eight-tenants",
            ClusterTopology::ThreeServersThreeForgeWorkers,
            8,
            TrafficMix::ReadHeavy,
        ),
    ]
}

/// Validate one report against its named fixed scenario identity.
///
/// # Errors
/// Returns [`ClusterBenchmarkError::Invalid`] when the name and fixed shape do
/// not identify one of D22's six scenarios.
fn validate_fixed_scenario(report: &ClusterScenarioReport) -> Result<(), ClusterBenchmarkError> {
    let scenario = &report.scenario;
    if !fixed_scenarios()
        .iter()
        .any(|(id, topology, tenants, traffic)| {
            scenario.scenario_id == *id
                && scenario.topology == *topology
                && scenario.tenants == *tenants
                && scenario.traffic == *traffic
        })
    {
        return Err(ClusterBenchmarkError::Invalid(format!(
            "unknown or mismatched fixed scenario `{}`",
            scenario.scenario_id
        )));
    }
    Ok(())
}

/// Compatibility decision emitted even when comparison is refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileCompatibility {
    /// Every D22 identity field matches.
    Compatible,
    /// One or more identity fields differ or are incomplete.
    Incompatible,
}

/// Relative regression result for one metric.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricRegression {
    /// Stable scenario/rate/metric key.
    pub metric: String,
    /// Median baseline value.
    pub before: f64,
    /// Median candidate value.
    pub after: f64,
    /// Inclusive pass threshold.
    pub threshold: f64,
    /// Whether the candidate passes this metric.
    pub passed: bool,
}

/// Structured comparison written even when qualification fails.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BifrostBenchmarkComparison {
    /// Profile compatibility decision.
    pub compatibility: ProfileCompatibility,
    /// Metric-level relative and absolute results.
    pub metric_results: Vec<MetricRegression>,
    /// Human-readable refusal or regression reasons.
    pub failures: Vec<String>,
    /// Whether all compatibility, evidence, SLO, fairness, scaling, and
    /// regression rules passed.
    pub ready: bool,
}

/// Full-cluster report and comparison errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ClusterBenchmarkError {
    /// Static report or scenario values violate the closed schema.
    #[error("invalid cluster benchmark: {0}")]
    Invalid(String),
    /// Required environment or statistical evidence is unavailable.
    #[error("unsupported cluster benchmark: {0}")]
    Unsupported(String),
    /// Workload completed but failed a correctness or SLO gate.
    #[error("cluster benchmark not ready: {0}")]
    NotReady(String),
    /// Baseline and candidate cannot be compared.
    #[error("incompatible cluster benchmark: {0}")]
    Incompatible(String),
}

/// Normalize a CPU model according to D22's closed comparison contract.
#[must_use]
pub fn normalize_cpu_model(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .replace("(r)", "")
        .replace("(tm)", "")
        .split_ascii_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Classify a normalized CPU brand as one supported vendor.
///
/// # Errors
/// Returns [`ClusterBenchmarkError::Unsupported`] when the brand cannot be
/// deterministically classified as Intel, AMD, or Apple.
pub fn classify_cpu_vendor(brand: &str) -> Result<String, ClusterBenchmarkError> {
    let normalized = normalize_cpu_model(brand);
    if normalized.contains("intel") || normalized == "genuineintel" {
        Ok("intel".to_owned())
    } else if normalized.contains("amd") || normalized == "authenticamd" {
        Ok("amd".to_owned())
    } else if normalized.contains("apple") {
        Ok("apple".to_owned())
    } else {
        Err(ClusterBenchmarkError::Unsupported(format!(
            "unclassifiable CPU brand `{brand}`"
        )))
    }
}

/// Extract and normalize Linux `vendor_id` and `model name` CPU identity.
///
/// # Errors
/// Returns [`ClusterBenchmarkError::Unsupported`] when either first-processor
/// field is absent or the vendor/model cannot be classified.
pub fn extract_linux_cpu_identity(
    cpuinfo: &str,
) -> Result<(String, String), ClusterBenchmarkError> {
    let fields = cpuinfo
        .lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.trim(), value.trim()))
        .collect::<BTreeMap<_, _>>();
    let vendor = fields.get("vendor_id").ok_or_else(|| {
        ClusterBenchmarkError::Unsupported("Linux cpuinfo has no vendor_id".to_owned())
    })?;
    let model = fields.get("model name").ok_or_else(|| {
        ClusterBenchmarkError::Unsupported("Linux cpuinfo has no model name".to_owned())
    })?;
    Ok((classify_cpu_vendor(vendor)?, normalize_cpu_model(model)))
}

/// Extract and normalize macOS `machdep.cpu.brand_string` identity.
///
/// # Errors
/// Returns [`ClusterBenchmarkError::Unsupported`] for an empty or
/// unclassifiable brand string.
pub fn extract_macos_cpu_identity(brand: &str) -> Result<(String, String), ClusterBenchmarkError> {
    let model = normalize_cpu_model(brand);
    if model.is_empty() {
        return Err(ClusterBenchmarkError::Unsupported(
            "macOS CPU brand string is empty".to_owned(),
        ));
    }
    Ok((classify_cpu_vendor(brand)?, model))
}

/// Return the median of exactly three independently derived integer values.
///
/// # Errors
/// Returns [`ClusterBenchmarkError::Unsupported`] for any other cardinality.
pub fn median_three_u64(values: &[u64]) -> Result<u64, ClusterBenchmarkError> {
    if values.len() != 3 {
        return Err(ClusterBenchmarkError::Unsupported(
            "median selection requires exactly three trials".to_owned(),
        ));
    }
    let mut values = [values[0], values[1], values[2]];
    values.sort_unstable();
    Ok(values[1])
}

/// Return the median of exactly three independently derived floating values.
///
/// # Errors
/// Returns [`ClusterBenchmarkError::Unsupported`] for wrong cardinality or a
/// non-finite value.
pub fn median_three_f64(values: &[f64]) -> Result<f64, ClusterBenchmarkError> {
    if values.len() != 3 || values.iter().any(|value| !value.is_finite()) {
        return Err(ClusterBenchmarkError::Unsupported(
            "median selection requires exactly three finite trials".to_owned(),
        ));
    }
    let mut values = [values[0], values[1], values[2]];
    values.sort_by(f64::total_cmp);
    Ok(values[1])
}

/// Return the fixed probe duration needed to reach thirty attempted samples.
///
/// The result grows from five seconds in five-second increments and never
/// exceeds forty-five seconds.
#[must_use]
pub fn calibration_probe_seconds(rate: u64, operations_per_request: u64) -> Option<u32> {
    (5..=45).step_by(5).find(|seconds| {
        rate.saturating_mul(u64::from(*seconds))
            .saturating_mul(operations_per_request)
            >= 30
    })
}

/// Return the ten immutable flush offsets in a measured 20-second trial.
#[must_use]
pub fn measured_flush_offsets_seconds() -> [u64; 10] {
    [2, 4, 6, 8, 10, 12, 14, 16, 18, 20]
}

/// Compute Jain fairness for one non-empty per-tenant vector.
#[must_use]
pub fn jain_fairness(values: &[u64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let sum = values.iter().filter_map(ToPrimitive::to_f64).sum::<f64>();
    let squares = values
        .iter()
        .filter_map(ToPrimitive::to_f64)
        .map(|value| value.powi(2))
        .sum::<f64>();
    if squares == 0.0 {
        0.0
    } else {
        sum.powi(2) / (values.len().to_f64().unwrap_or(f64::INFINITY) * squares)
    }
}

/// Compare two complete compatible profiles using D22's exact boundaries.
#[must_use]
pub fn compare_cluster_profiles(
    before: &BifrostReferenceProfile,
    after: &BifrostReferenceProfile,
) -> BifrostBenchmarkComparison {
    let mut comparison = BifrostBenchmarkComparison {
        compatibility: ProfileCompatibility::Compatible,
        metric_results: Vec::new(),
        failures: Vec::new(),
        ready: false,
    };
    if let Err(error) = before.validate_reference() {
        comparison.failures.push(format!("baseline: {error}"));
    }
    if let Err(error) = after.validate_reference() {
        comparison.failures.push(format!("candidate: {error}"));
    }
    let identities_match = before.schema_version == after.schema_version
        && before.environment.compatible_with(&after.environment)
        && scenario_map(before).keys().eq(scenario_map(after).keys());
    if !identities_match {
        comparison.compatibility = ProfileCompatibility::Incompatible;
        comparison
            .failures
            .push("profile identity or exact absolute offered rates differ".to_owned());
        return comparison;
    }
    let after_map = scenario_map(after);
    for (key, before_report) in scenario_map(before) {
        let Some(after_report) = after_map.get(&key) else {
            comparison.compatibility = ProfileCompatibility::Incompatible;
            comparison.failures.push(format!("missing scenario {key}"));
            continue;
        };
        compare_scenario_metrics(&mut comparison, &key, before_report, after_report);
    }
    evaluate_scaling(after, &mut comparison);
    comparison.ready = comparison.compatibility == ProfileCompatibility::Compatible
        && comparison.failures.is_empty()
        && comparison.metric_results.iter().all(|result| result.passed);
    comparison
}

/// Append every D22 relative metric for one compatible scenario/rate pair.
fn compare_scenario_metrics(
    comparison: &mut BifrostBenchmarkComparison,
    key: &str,
    before_report: &ClusterScenarioReport,
    after_report: &ClusterScenarioReport,
) {
    push_lower_bound(
        comparison,
        &format!("{key}.durable_rows_per_second"),
        before_report.median.durable_rows_per_second,
        after_report.median.durable_rows_per_second,
        0.90,
    );
    push_lower_bound(
        comparison,
        &format!("{key}.queries_per_second"),
        before_report.median.queries_per_second,
        after_report.median.queries_per_second,
        0.90,
    );
    for (name, before_value, after_value) in [
        (
            "durable_write_p95_us",
            before_report.median.durable_write_p95_us,
            after_report.median.durable_write_p95_us,
        ),
        (
            "durable_write_p99_us",
            before_report.median.durable_write_p99_us,
            after_report.median.durable_write_p99_us,
        ),
        (
            "query_ttfb_p95_us",
            before_report.median.query_ttfb_p95_us,
            after_report.median.query_ttfb_p95_us,
        ),
        (
            "query_ttfb_p99_us",
            before_report.median.query_ttfb_p99_us,
            after_report.median.query_ttfb_p99_us,
        ),
        (
            "total_query_p95_us",
            before_report.median.total_query_p95_us,
            after_report.median.total_query_p95_us,
        ),
        (
            "total_query_p99_us",
            before_report.median.total_query_p99_us,
            after_report.median.total_query_p99_us,
        ),
    ] {
        push_upper_bound(
            comparison,
            &format!("{key}.{name}"),
            before_value.to_f64().unwrap_or(f64::INFINITY),
            after_value.to_f64().unwrap_or(f64::INFINITY),
            1.15,
        );
    }
    for (name, before_value, after_value) in [
        (
            "retry_ratio",
            before_report.median.retry_ratio,
            after_report.median.retry_ratio,
        ),
        (
            "backpressure_ratio",
            before_report.median.backpressure_ratio,
            after_report.median.backpressure_ratio,
        ),
    ] {
        push_absolute_upper(
            comparison,
            &format!("{key}.{name}"),
            before_value,
            after_value,
            before_value + 0.02,
        );
    }
    for (name, value) in [
        ("write_fairness", after_report.median.write_fairness),
        ("read_fairness", after_report.median.read_fairness),
    ] {
        if value > 0.0 {
            push_absolute_lower(comparison, &format!("{key}.{name}"), value, 0.95);
        }
    }
}

/// Build the exact scenario/rate compatibility key map.
fn scenario_map(profile: &BifrostReferenceProfile) -> BTreeMap<String, &ClusterScenarioReport> {
    profile
        .scenarios
        .iter()
        .map(|report| {
            let scenario = &report.scenario;
            (
                format!(
                    "{}:{}:{:?}:{}:{:?}:{}:{}:{}:{}:{:?}:{}:{}:{}:{}:{}",
                    scenario.scenario_id,
                    scenario.workload_version,
                    scenario.topology,
                    scenario.tenants,
                    scenario.traffic,
                    scenario.rows_per_batch,
                    scenario.query_row_limit,
                    scenario.offered_requests_per_second,
                    scenario.offered_load_percent,
                    scenario.knee_provenance,
                    scenario.warmup_seconds,
                    scenario.measured_seconds,
                    scenario.trials,
                    scenario.minimum_samples,
                    scenario.seed,
                ),
                report,
            )
        })
        .collect()
}

/// Append one relative lower-bound comparison.
fn push_lower_bound(
    comparison: &mut BifrostBenchmarkComparison,
    metric: &str,
    before: f64,
    after: f64,
    factor: f64,
) {
    if before == 0.0 && after == 0.0 {
        return;
    }
    let threshold = before * factor;
    comparison.metric_results.push(MetricRegression {
        metric: metric.to_owned(),
        before,
        after,
        threshold,
        passed: after >= threshold,
    });
}

/// Append one relative upper-bound comparison.
fn push_upper_bound(
    comparison: &mut BifrostBenchmarkComparison,
    metric: &str,
    before: f64,
    after: f64,
    factor: f64,
) {
    if before == 0.0 && after == 0.0 {
        return;
    }
    push_absolute_upper(comparison, metric, before, after, before * factor);
}

/// Append one absolute upper-bound comparison.
fn push_absolute_upper(
    comparison: &mut BifrostBenchmarkComparison,
    metric: &str,
    before: f64,
    after: f64,
    threshold: f64,
) {
    comparison.metric_results.push(MetricRegression {
        metric: metric.to_owned(),
        before,
        after,
        threshold,
        passed: after <= threshold,
    });
}

/// Append one absolute lower-bound comparison.
fn push_absolute_lower(
    comparison: &mut BifrostBenchmarkComparison,
    metric: &str,
    after: f64,
    threshold: f64,
) {
    comparison.metric_results.push(MetricRegression {
        metric: metric.to_owned(),
        before: threshold,
        after,
        threshold,
        passed: after >= threshold,
    });
}

/// Evaluate the balanced eight-tenant topology scaling rule.
fn evaluate_scaling(
    profile: &BifrostReferenceProfile,
    comparison: &mut BifrostBenchmarkComparison,
) {
    let balanced = profile.scenarios.iter().filter(|report| {
        report.scenario.traffic == TrafficMix::Balanced
            && report.scenario.tenants == 8
            && report.scenario.offered_load_percent == 100
    });
    let mut one = None;
    let mut three = None;
    for report in balanced {
        if report.discovered_knee_provenance == KneeProvenance::CensoredAtCap {
            comparison
                .failures
                .push("censored knee cannot claim scaling qualification".to_owned());
            return;
        }
        match report.scenario.topology {
            ClusterTopology::OnePod => one = Some(report.discovered_knee_requests_per_second),
            ClusterTopology::ThreeServersThreeForgeWorkers => {
                three = Some(report.discovered_knee_requests_per_second);
            }
        }
    }
    if let (Some(one), Some(three)) = (one, three) {
        let efficiency =
            three.to_f64().unwrap_or(0.0) / (one.to_f64().unwrap_or(f64::INFINITY) * 3.0);
        push_absolute_lower(comparison, "balanced_scaling_efficiency", efficiency, 0.75);
    } else {
        comparison.failures.push(
            "balanced eight-tenant scaling pair is absent from the candidate profile".to_owned(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Proves D22 CPU marker and whitespace normalization is deterministic.
    #[test]
    fn cpu_normalization_removes_only_markers_and_whitespace() {
        assert_eq!(
            normalize_cpu_model(" Intel(R)  Xeon(TM)   Gold  "),
            "intel xeon gold"
        );
        assert_eq!(normalize_cpu_model("Apple M3 Pro"), "apple m3 pro");
    }

    /// Proves supported CPU fixtures classify and unknown vendors refuse.
    #[test]
    fn cpu_vendor_classification_is_closed() {
        assert_eq!(classify_cpu_vendor("GenuineIntel").unwrap(), "intel");
        assert_eq!(classify_cpu_vendor("AuthenticAMD").unwrap(), "amd");
        assert_eq!(classify_cpu_vendor("Apple M3 Pro").unwrap(), "apple");
        assert!(matches!(
            classify_cpu_vendor("mystery processor"),
            Err(ClusterBenchmarkError::Unsupported(_))
        ));
    }

    /// Proves exact Linux and macOS extraction fixtures produce normalized identity.
    #[test]
    fn platform_cpu_extraction_uses_declared_fields() {
        let linux = "processor: 0\nvendor_id: GenuineIntel\nmodel name: Intel(R) Xeon(TM) Gold\n";
        assert_eq!(
            extract_linux_cpu_identity(linux).unwrap(),
            ("intel".to_owned(), "intel xeon gold".to_owned())
        );
        assert_eq!(
            extract_macos_cpu_identity("Apple M3 Pro").unwrap(),
            ("apple".to_owned(), "apple m3 pro".to_owned())
        );
        assert!(extract_linux_cpu_identity("model name: Unknown").is_err());
    }

    /// Proves comparison percentiles use the median rather than the best run.
    #[test]
    fn median_selection_uses_middle_trial() {
        assert_eq!(median_three_u64(&[9, 1, 5]).unwrap(), 5);
        assert!((median_three_f64(&[9.0, 1.0, 5.0]).unwrap() - 5.0).abs() < f64::EPSILON);
        assert!(median_three_u64(&[1, 2]).is_err());
    }

    /// Proves calibration extends only as far as required and caps at 45s.
    #[test]
    fn calibration_sample_extension_is_bounded() {
        assert_eq!(calibration_probe_seconds(8, 1), Some(5));
        assert_eq!(calibration_probe_seconds(1, 1), Some(30));
        assert_eq!(calibration_probe_seconds(0, 1), None);
    }

    /// Proves measured flush cadence is fixed and never shifted.
    #[test]
    fn flush_cadence_contains_exact_ten_offsets() {
        assert_eq!(
            measured_flush_offsets_seconds(),
            [2, 4, 6, 8, 10, 12, 14, 16, 18, 20]
        );
    }

    /// Proves Jain fairness uses the canonical equation and empty input fails low.
    #[test]
    fn fairness_uses_separate_vector_equation() {
        assert!((jain_fairness(&[10, 10, 10]) - 1.0).abs() < f64::EPSILON);
        assert!(jain_fairness(&[10, 1]) < 0.95);
        assert!(jain_fairness(&[]).abs() < f64::EPSILON);
    }

    /// Build one complete controlled profile for comparison-boundary tests.
    fn profile() -> BifrostReferenceProfile {
        let mut scenarios = Vec::new();
        for (id, topology, tenants, traffic) in fixed_scenarios() {
            let knee = if topology == ClusterTopology::OnePod {
                100
            } else {
                240
            };
            for percent in [50, 75, 100] {
                scenarios.push(fixture_scenario(
                    id, topology, tenants, traffic, percent, knee,
                ));
            }
        }
        BifrostReferenceProfile {
            schema_version: CLUSTER_REPORT_VERSION.to_owned(),
            environment: fixture_environment(),
            scenarios,
            slos: BifrostSloEnvelope::default(),
        }
    }

    /// Build the closed environment fixture used by comparison tests.
    fn fixture_environment() -> BenchmarkEnvironment {
        BenchmarkEnvironment {
            architecture: "aarch64".to_owned(),
            cpu_vendor: "apple".to_owned(),
            cpu_model: "apple m3 pro".to_owned(),
            logical_cores: 11,
            host_memory_bytes: 19_327_352_832,
            container_cpu_millis: 4_000,
            container_memory_bytes: 4_294_967_296,
            postgres_image: "postgres:16".to_owned(),
            postgres_version: "16.10".to_owned(),
            postgres_config: "wyrd-reference-v1".to_owned(),
            storage_image: "in-process-memory".to_owned(),
            storage_version: "wyrd-storage/0.0.1".to_owned(),
            storage_config: "memory-loopback-v1".to_owned(),
            rust_major_minor: "1.95".to_owned(),
            arrow_version: "58.4.0".to_owned(),
            datafusion_version: "53.1.0".to_owned(),
            iceberg_version: "1422e94ac570ed99abbf530403dd40de67d37dbf".to_owned(),
            os: "macos".to_owned(),
            kernel: "Darwin 25".to_owned(),
            git_sha: "0123456789abcdef".to_owned(),
            dirty_worktree: false,
            captured_at: "2026-08-03T12:00:00Z".to_owned(),
        }
    }

    /// Build one complete scenario/load fixture with three identical trials.
    fn fixture_scenario(
        id: &str,
        topology: ClusterTopology,
        tenants: u32,
        traffic: TrafficMix,
        percent: u8,
        knee: u64,
    ) -> ClusterScenarioReport {
        let client = fixture_client();
        let production = fixture_production();
        ClusterScenarioReport {
            scenario: ClusterBenchmarkScenario {
                scenario_id: id.to_owned(),
                workload_version: CLUSTER_WORKLOAD_VERSION.to_owned(),
                topology,
                tenants,
                traffic,
                rows_per_batch: 64,
                query_row_limit: 64,
                offered_requests_per_second: knee * u64::from(percent) / 100,
                offered_load_percent: percent,
                knee_provenance: KneeProvenance::Discovered,
                warmup_seconds: 5,
                measured_seconds: 20,
                trials: 3,
                minimum_samples: 200,
                seed: 0xB1_F057,
            },
            discovered_knee_requests_per_second: knee,
            discovered_knee_provenance: KneeProvenance::Discovered,
            trials: (1..=3)
                .map(|trial| ClusterBenchmarkTrial {
                    trial,
                    client: client.clone(),
                    production: production.clone(),
                })
                .collect(),
            median: MedianMetrics {
                durable_rows_per_second: 1_000.0,
                queries_per_second: 100.0,
                query_rows_per_second: 500.0,
                durable_write_p95_us: 20_000,
                durable_write_p99_us: 30_000,
                flush_to_visible_p95_us: 200_000,
                flush_to_visible_p99_us: 300_000,
                query_ttfb_p95_us: 20_000,
                query_ttfb_p99_us: 30_000,
                total_query_p95_us: 20_000,
                total_query_p99_us: 30_000,
                retry_ratio: 0.01,
                backpressure_ratio: 0.01,
                write_fairness: 1.0,
                read_fairness: 1.0,
            },
        }
    }

    /// Build exact production reconciliation evidence for one fixture trial.
    fn fixture_production() -> ProductionTelemetryEvidence {
        ProductionTelemetryEvidence {
            gate_accepted_rows: 1_000,
            scribe_accepted_rows: 1_000,
            client_acknowledged_rows: 1_000,
            scribe_persisted_rows: 1_000,
            forge_published_rows: 1_000,
            oracle_stream_rows: 500,
            client_decoded_rows: 500,
            oracle_terminal_outcomes: 200,
            client_completed_queries: 200,
            expected_audit_rows: 200,
            observed_audit_rows: 200,
            required_telemetry: EvidenceStatus::Complete,
            counter_integrity: EvidenceStatus::Complete,
            spans_clean: EvidenceStatus::Complete,
            cleanup: EvidenceStatus::Complete,
            correctness: EvidenceStatus::Complete,
        }
    }

    /// Build exact client measurements for one fixture trial.
    fn fixture_client() -> ClientTrialMetrics {
        let distribution = TrialDistribution {
            samples: 200,
            p50_us: 10_000,
            p95_us: 20_000,
            p99_us: 30_000,
            max_us: 40_000,
            overflowed: false,
        };
        ClientTrialMetrics {
            durable_write: distribution,
            flush_to_visible: TrialDistribution {
                samples: 10,
                p50_us: 100_000,
                p95_us: 200_000,
                p99_us: 300_000,
                max_us: 400_000,
                overflowed: false,
            },
            query_time_to_first_frame: distribution,
            total_query: distribution,
            durable_rows_per_second: 1_000.0,
            queries_per_second: 100.0,
            query_rows_per_second: 500.0,
            backpressure_ratio: 0.01,
            retry_ratio: 0.01,
            write_fairness: 1.0,
            read_fairness: 1.0,
            missed_operations: 0,
            missed_flushes: 0,
        }
    }

    /// Replace one fixed scenario's knee and derived absolute loads consistently.
    fn set_knee(
        profile: &mut BifrostReferenceProfile,
        scenario_id: &str,
        knee: u64,
        provenance: KneeProvenance,
    ) {
        for report in &mut profile.scenarios {
            if report.scenario.scenario_id == scenario_id {
                report.discovered_knee_requests_per_second = knee;
                report.discovered_knee_provenance = provenance;
                report.scenario.knee_provenance = provenance;
            }
        }
    }

    /// Copy one report median into all three identical fixture trials.
    fn synchronize_fixture_trials(report: &mut ClusterScenarioReport) {
        for trial in &mut report.trials {
            trial.client.durable_rows_per_second = report.median.durable_rows_per_second;
            trial.client.queries_per_second = report.median.queries_per_second;
            trial.client.query_rows_per_second = report.median.query_rows_per_second;
            trial.client.durable_write.p95_us = report.median.durable_write_p95_us;
            trial.client.durable_write.p99_us = report.median.durable_write_p99_us;
            trial.client.flush_to_visible.p95_us = report.median.flush_to_visible_p95_us;
            trial.client.flush_to_visible.p99_us = report.median.flush_to_visible_p99_us;
            trial.client.query_time_to_first_frame.p95_us = report.median.query_ttfb_p95_us;
            trial.client.query_time_to_first_frame.p99_us = report.median.query_ttfb_p99_us;
            trial.client.total_query.p95_us = report.median.total_query_p95_us;
            trial.client.total_query.p99_us = report.median.total_query_p99_us;
            trial.client.retry_ratio = report.median.retry_ratio;
            trial.client.backpressure_ratio = report.median.backpressure_ratio;
            trial.client.write_fairness = report.median.write_fairness;
            trial.client.read_fairness = report.median.read_fairness;
        }
    }

    /// Proves old or unknown report schemas fail strict decoding/validation.
    #[test]
    fn report_versioning_refuses_old_and_unknown_shapes() {
        let mut report = profile();
        report.schema_version = "wyrd.bifrost.cluster-report/v0".to_owned();
        assert!(matches!(
            report.validate_reference(),
            Err(ClusterBenchmarkError::Incompatible(_))
        ));
        let mut json = serde_json::to_value(profile()).unwrap();
        json.as_object_mut()
            .unwrap()
            .insert("unknown".to_owned(), serde_json::Value::Bool(true));
        assert!(serde_json::from_value::<BifrostReferenceProfile>(json).is_err());
    }

    /// Proves exact compatibility permits only five-percent host-memory drift.
    #[test]
    fn environment_compatibility_is_closed() {
        let before = profile().environment;
        let mut candidate = before.clone();
        candidate.host_memory_bytes = before.host_memory_bytes * 105 / 100;
        assert!(before.compatible_with(&candidate));
        candidate.host_memory_bytes = before.host_memory_bytes * 106 / 100;
        assert!(!before.compatible_with(&candidate));
        candidate = before.clone();
        candidate.container_cpu_millis += 1;
        assert!(!before.compatible_with(&candidate));
    }

    /// Proves a missing required operation sample is unsupported, never ready.
    #[test]
    fn insufficient_samples_are_unsupported() {
        let mut report = profile();
        report.scenarios[0].trials[0].client.durable_write.samples = 199;
        assert!(matches!(
            report.validate_reference(),
            Err(ClusterBenchmarkError::Unsupported(_))
        ));
    }

    /// Proves first-probe failure is a not-ready condition represented in evidence.
    #[test]
    fn first_probe_failure_cannot_become_reference() {
        let mut report = profile();
        report.scenarios[0].trials[0].production.correctness = EvidenceStatus::Failed;
        assert!(matches!(
            report.validate_reference(),
            Err(ClusterBenchmarkError::NotReady(_))
        ));
        assert_eq!(FIRST_PROBE_RATE, 8);
    }

    /// Proves exact-rate identity mismatches refuse comparison.
    #[test]
    fn candidate_must_replay_exact_reference_rate() {
        let before = profile();
        let mut after = profile();
        after.scenarios[0].scenario.offered_requests_per_second += 1;
        let comparison = compare_cluster_profiles(&before, &after);
        assert_eq!(comparison.compatibility, ProfileCompatibility::Incompatible);
        assert!(!comparison.ready);
    }

    /// Proves the profile refuses duplicate, missing, and mismatched fixed identities.
    #[test]
    fn fixed_matrix_is_closed_and_duplicate_free() {
        let mut duplicate = profile();
        duplicate.scenarios[1] = duplicate.scenarios[0].clone();
        assert!(matches!(
            duplicate.validate_reference(),
            Err(ClusterBenchmarkError::Invalid(_))
        ));

        let mut mismatched = profile();
        mismatched.scenarios[0].scenario.traffic = TrafficMix::ReadHeavy;
        assert!(matches!(
            mismatched.validate_reference(),
            Err(ClusterBenchmarkError::Invalid(_))
        ));

        let mut forged_median = profile();
        forged_median.scenarios[0].median.queries_per_second += 1.0;
        assert!(matches!(
            forged_median.validate_reference(),
            Err(ClusterBenchmarkError::Invalid(_))
        ));
    }

    /// Proves every workload-shape field participates in comparison identity.
    #[test]
    fn workload_shape_drift_is_incompatible() {
        let before = profile();
        let mut after = profile();
        after.scenarios[0].scenario.seed += 1;
        let comparison = compare_cluster_profiles(&before, &after);
        assert_eq!(comparison.compatibility, ProfileCompatibility::Incompatible);
        assert!(!comparison.ready);
    }

    /// Proves a censored knee cannot claim scaling qualification.
    #[test]
    fn censored_knee_is_unsupported_for_scaling() {
        let mut before = profile();
        let mut after = profile();
        set_knee(
            &mut before,
            "balanced-one-pod-eight-tenants",
            CALIBRATION_RATE_CAP,
            KneeProvenance::CensoredAtCap,
        );
        set_knee(
            &mut after,
            "balanced-one-pod-eight-tenants",
            CALIBRATION_RATE_CAP,
            KneeProvenance::CensoredAtCap,
        );
        let comparison = compare_cluster_profiles(&before, &after);
        assert!(!comparison.ready);
        assert!(
            comparison
                .failures
                .iter()
                .any(|failure| failure.contains("censored"))
        );
    }

    /// Proves all exact relative thresholds pass at the boundary and fail beyond it.
    #[test]
    fn regression_threshold_boundaries_are_inclusive() {
        let before = profile();
        let mut boundary = profile();
        for report in &mut boundary.scenarios {
            report.median.durable_rows_per_second = 900.0;
            report.median.queries_per_second = 90.0;
            report.median.durable_write_p95_us = 23_000;
            report.median.durable_write_p99_us = 34_500;
            report.median.query_ttfb_p95_us = 23_000;
            report.median.query_ttfb_p99_us = 34_500;
            report.median.total_query_p95_us = 23_000;
            report.median.total_query_p99_us = 34_500;
            report.median.retry_ratio = 0.03;
            report.median.backpressure_ratio = 0.03;
            report.median.write_fairness = 0.95;
            report.median.read_fairness = 0.95;
            synchronize_fixture_trials(report);
        }
        assert!(compare_cluster_profiles(&before, &boundary).ready);
        boundary.scenarios[0].median.durable_rows_per_second = 899.9;
        synchronize_fixture_trials(&mut boundary.scenarios[0]);
        assert!(!compare_cluster_profiles(&before, &boundary).ready);
    }

    /// Proves three-pod scaling must reach at least 75 percent efficiency.
    #[test]
    fn scaling_efficiency_boundary_is_enforced() {
        let before = profile();
        let mut after = profile();
        set_knee(
            &mut after,
            "balanced-three-server-three-worker-eight-tenants",
            225,
            KneeProvenance::Discovered,
        );
        assert!(compare_cluster_profiles(&before, &after).ready);
        set_knee(
            &mut after,
            "balanced-three-server-three-worker-eight-tenants",
            224,
            KneeProvenance::Discovered,
        );
        assert!(!compare_cluster_profiles(&before, &after).ready);
    }
}
