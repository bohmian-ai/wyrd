//! Versioned full-cluster Bifrost reference benchmark contracts.
//!
//! This module owns the reconciled scenario, trial, and production-telemetry
//! evidence shapes only. The real Gate-to-Oracle workload remains in
//! `wyrd-testing` so this crate stays free of server, SQL, storage, and
//! runtime dependencies.

use std::collections::BTreeMap;

use num_traits::ToPrimitive;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Stable workload identifier shared by captures and comparisons.
pub const CLUSTER_WORKLOAD_VERSION: &str = "wyrd.bifrost.cluster-workload/v2";

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

/// Deterministic row and batch identity range owned by one tenant/stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TenantStageRows {
    /// Zero-based tenant ordinal.
    pub tenant_ordinal: u32,
    /// Seed from which deterministic batch IDs derive.
    pub batch_id_seed: u64,
    /// Inclusive first row identity.
    pub row_id_start: u64,
    /// Exclusive final row identity.
    pub row_id_end_exclusive: u64,
    /// Exact accepted batch ordinals; gaps represent rejected scheduled writes.
    pub batch_ordinals: Vec<u64>,
    /// Exact accepted row identities.
    pub row_ids: Vec<u64>,
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
    /// Warmup duration in seconds.
    pub warmup_seconds: u32,
    /// Measured duration in seconds.
    pub measured_seconds: u32,
    /// Independent trial count.
    pub trials: u8,
    /// Minimum write and query samples required in each relevant trial.
    pub minimum_samples: u64,
    /// Profile-driven concurrent public operation cap.
    pub max_in_flight: usize,
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
            || self.warmup_seconds != 10
            || self.measured_seconds != 20
            || self.trials != 3
            || self.minimum_samples != 200
            || self.max_in_flight == 0
            || self.seed != 0xB1_F057
        {
            return Err(ClusterBenchmarkError::Invalid(format!(
                "scenario {} violates the v2 reference contract",
                self.scenario_id
            )));
        }
        Ok(())
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
    /// Public operations planned for the fixed measured window.
    pub planned_operations: u64,
    /// Public operations dispatched rather than missed by the open-loop scheduler.
    pub attempted_operations: u64,
    /// Public operations accepted by Gate or completed by Oracle.
    pub accepted_operations: u64,
    /// Stable public admission or capacity rejections.
    pub backpressure_operations: u64,
    /// Stable durable-write admission or capacity rejections.
    pub write_backpressure_operations: u64,
    /// Stable query and strict-visibility admission or capacity rejections.
    pub query_backpressure_operations: u64,
    /// Stable public error-code counts for all rejected operations.
    pub backpressure_by_code: BTreeMap<String, u64>,
    /// Transient public failures eligible for bounded retry.
    pub retry_operations: u64,
    /// Public operations rejected because the profile-driven in-flight cap was exhausted.
    pub in_flight_cap_exhaustions: u64,
    /// Declared profile-driven in-flight cap used by the driver.
    pub max_in_flight: u64,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fairness_uses_separate_vector_equation() {
        assert!((jain_fairness(&[10, 10, 10]) - 1.0).abs() < f64::EPSILON);
        assert!(jain_fairness(&[10, 1]) < 0.95);
        assert!(jain_fairness(&[]).abs() < f64::EPSILON);
    }
}
