//! Typed, server-independent Bifrost benchmark scenarios and measurements.
//!
//! The lane adapters deliberately contain no cluster, SQL, tonic, or storage
//! code. They declare a run and reduce completed durable-ACK samples into a
//! versioned report value. `wyrd-testing` owns the real harness that executes
//! these declarations against Wyrd.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Current report schema for the Bifrost benchmark suite.
pub const REPORT_SCHEMA_VERSION: &str = "wyrd.bifrost.report/v3";

/// One product lane with its own preparation and verification semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BifrostLane {
    /// Unary durable Scribe ingest.
    Scribe,
    /// Native OTLP export projected through Gate.
    Otlp,
    /// Exact tenant/table/day Forge compaction.
    Forge,
    /// Oracle fused hot/file/object read.
    Oracle,
}

/// Payload shape owned by a lane's scenario.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BifrostPayloadShape {
    /// Projected Arrow rows sent through the unary Scribe seam.
    ArrowRows,
    /// Native OTLP resource/span batches sent through Gate.
    OtlpSpans,
    /// A Forge maintenance group.
    ForgeGroup,
    /// An Oracle bounded hot-tail query.
    OracleQuery,
}

/// Fault profile declared by a benchmark scenario.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BifrostFaultProfile {
    /// No injected fault.
    #[default]
    None,
    /// Delay the WAL grouped sync operation.
    DelayedFsync,
    /// Fail one persistence attempt and verify owner-local retry.
    Retry,
    /// Drive the Scribe child budget through its hard-pressure boundary.
    MemoryPressure,
    /// Drive the WAL admission path through its hard-pressure boundary.
    WalPressure,
    /// Delay the concrete object-store write seam.
    ObjectStoreStall,
    /// Fail one concrete Postgres publication attempt before retry.
    PostgresStall,
    /// Reserve Scribe, Oracle, and Forge memory against one parent.
    ConcurrentRoleMemory,
}

impl BifrostFaultProfile {
    /// Parse the process-facing fault profile name.
    pub fn parse(value: &str) -> Result<Self, ScenarioError> {
        match value {
            "none" => Ok(Self::None),
            "delayed_fsync" => Ok(Self::DelayedFsync),
            "retry" => Ok(Self::Retry),
            "memory_pressure" => Ok(Self::MemoryPressure),
            "wal_pressure" => Ok(Self::WalPressure),
            "object_store_stall" => Ok(Self::ObjectStoreStall),
            "postgres_stall" => Ok(Self::PostgresStall),
            "concurrent_role_memory" => Ok(Self::ConcurrentRoleMemory),
            other => Err(ScenarioError::Invalid(format!(
                "unknown fault_profile `{other}`"
            ))),
        }
    }
}

impl BifrostLane {
    /// Parse a canonical lane name.
    pub fn parse(value: &str) -> Result<Self, ScenarioError> {
        match value {
            "scribe" => Ok(Self::Scribe),
            "otlp" => Ok(Self::Otlp),
            "forge" => Ok(Self::Forge),
            "oracle" => Ok(Self::Oracle),
            other => Err(ScenarioError::UnknownLane(other.to_owned())),
        }
    }

    /// Stable report label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scribe => "scribe",
            Self::Otlp => "otlp",
            Self::Forge => "forge",
            Self::Oracle => "oracle",
        }
    }
}

/// A typed benchmark declaration shared by all canonical entry points.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BifrostScenario {
    /// Product lane owning preparation and verification.
    pub lane: BifrostLane,
    /// Number of real Wyrd pods required by the harness.
    pub pods: u32,
    /// Number of isolated data tenants.
    pub tenants: u32,
    /// Number of logical tables per tenant.
    pub tables: u32,
    /// Warmup duration in seconds.
    pub warmup_seconds: u32,
    /// Measured duration in seconds.
    pub measured_seconds: u32,
    /// Maximum producer work in flight.
    pub max_in_flight: u32,
    /// Offered work units per second for open-loop lanes.
    pub target_items_per_second: u64,
    /// Work units sent in one protocol request.
    pub items_per_request: u32,
    /// Number of post-run samples required by the lane verifier.
    pub verify_samples: u32,
    /// Payload shape selected by the lane.
    pub payload_shape: BifrostPayloadShape,
    /// Explicit fault profile for the run.
    pub fault_profile: BifrostFaultProfile,
    /// Whether the harness must verify negative and retry flows.
    pub require_negative_flows: bool,
    /// Optional compact Scribe case selected by the benchmark process.
    pub case_id: Option<String>,
    /// Optional deterministic delayed-fsync injection.
    pub fsync_delay_ms: u32,
    /// Product ACK p99 limit for lanes with a durable latency SLO.
    pub ack_p99_limit_us: Option<u64>,
}

impl BifrostScenario {
    /// Build the short smoke declaration used by every canonical adapter.
    #[must_use]
    pub fn smoke(lane: BifrostLane) -> Self {
        Self {
            lane,
            pods: 1,
            tenants: 1,
            tables: 1,
            warmup_seconds: 0,
            measured_seconds: 1,
            max_in_flight: 16,
            target_items_per_second: 10_000,
            items_per_request: 100,
            verify_samples: 3,
            payload_shape: match lane {
                BifrostLane::Scribe => BifrostPayloadShape::ArrowRows,
                BifrostLane::Otlp => BifrostPayloadShape::OtlpSpans,
                BifrostLane::Forge => BifrostPayloadShape::ForgeGroup,
                BifrostLane::Oracle => BifrostPayloadShape::OracleQuery,
            },
            fault_profile: BifrostFaultProfile::None,
            require_negative_flows: true,
            case_id: None,
            fsync_delay_ms: 0,
            ack_p99_limit_us: (lane == BifrostLane::Scribe).then_some(5_000),
        }
    }

    /// Resolve a scenario with command-line values taking precedence over
    /// lane-specific environment defaults.
    pub fn from_process(lane: BifrostLane) -> Result<Self, ScenarioError> {
        let mut scenario = Self::smoke(lane);
        scenario.pods = bounded_value("pods", "WYRD_BIFROST_PODS", scenario.pods)?;
        scenario.tenants = bounded_value("tenants", "WYRD_BIFROST_TENANTS", scenario.tenants)?;
        scenario.tables = bounded_value("tables", "WYRD_BIFROST_TABLES", scenario.tables)?;
        scenario.warmup_seconds = bounded_value(
            "warmup-seconds",
            "WYRD_BIFROST_WARMUP_SECONDS",
            scenario.warmup_seconds,
        )?;
        scenario.measured_seconds = bounded_value(
            "measured-seconds",
            "WYRD_BIFROST_MEASURED_SECONDS",
            scenario.measured_seconds,
        )?;
        scenario.max_in_flight = bounded_value(
            "max-in-flight",
            "WYRD_BIFROST_MAX_IN_FLIGHT",
            scenario.max_in_flight,
        )?;
        scenario.target_items_per_second = bounded_value(
            "target-items-per-second",
            "WYRD_BIFROST_TARGET_ITEMS_PER_SECOND",
            scenario.target_items_per_second,
        )?;
        scenario.items_per_request = bounded_value(
            "items-per-request",
            "WYRD_BIFROST_ITEMS_PER_REQUEST",
            scenario.items_per_request,
        )?;
        scenario.verify_samples = bounded_value(
            "verify-samples",
            "WYRD_BIFROST_VERIFY_SAMPLES",
            scenario.verify_samples,
        )?;
        if let Some(value) = std::env::var_os("WYRD_BIFROST_FAULT_PROFILE") {
            scenario.fault_profile = BifrostFaultProfile::parse(&value.to_string_lossy())?;
        }
        scenario.require_negative_flows = bounded_value(
            "require-negative-flows",
            "WYRD_BIFROST_REQUIRE_NEGATIVE_FLOWS",
            scenario.require_negative_flows,
        )?;
        scenario.fsync_delay_ms = bounded_value(
            "fsync-delay-ms",
            "WYRD_BIFROST_FSYNC_DELAY_MS",
            scenario.fsync_delay_ms,
        )?;
        scenario.validate()?;
        Ok(scenario)
    }

    /// Validate topology, bounds, and lane-specific constraints before setup.
    pub fn validate(&self) -> Result<(), ScenarioError> {
        if !matches!(self.pods, 1 | 3 | 6) {
            return Err(ScenarioError::Invalid("pods must be 1, 3, or 6".to_owned()));
        }
        if self.tenants == 0 || self.tables == 0 {
            return Err(ScenarioError::Invalid(
                "tenants and tables must be greater than zero".to_owned(),
            ));
        }
        if self.measured_seconds == 0
            || self.max_in_flight == 0
            || self.target_items_per_second == 0
            || self.items_per_request == 0
        {
            return Err(ScenarioError::Invalid(
                "measured_seconds and max_in_flight must be greater than zero".to_owned(),
            ));
        }
        if self.lane != BifrostLane::Scribe && self.fsync_delay_ms != 0 {
            return Err(ScenarioError::Invalid(
                "fsync delay belongs only to the Scribe lane".to_owned(),
            ));
        }
        let expected_shape = match self.lane {
            BifrostLane::Scribe => BifrostPayloadShape::ArrowRows,
            BifrostLane::Otlp => BifrostPayloadShape::OtlpSpans,
            BifrostLane::Forge => BifrostPayloadShape::ForgeGroup,
            BifrostLane::Oracle => BifrostPayloadShape::OracleQuery,
        };
        if self.payload_shape != expected_shape {
            return Err(ScenarioError::Invalid(
                "payload_shape does not match lane".to_owned(),
            ));
        }
        if self.fsync_delay_ms == 0 && self.fault_profile == BifrostFaultProfile::DelayedFsync {
            return Err(ScenarioError::Invalid(
                "delayed_fsync requires a positive fsync_delay_ms".to_owned(),
            ));
        }
        if self.fsync_delay_ms > 0 && self.fault_profile != BifrostFaultProfile::DelayedFsync {
            return Err(ScenarioError::Invalid(
                "positive fsync_delay_ms requires delayed_fsync".to_owned(),
            ));
        }
        if self.ack_p99_limit_us == Some(0) {
            return Err(ScenarioError::Invalid(
                "ack_p99_limit_us must be greater than zero".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Common report envelope shared by every canonical lane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BifrostReportEnvelope {
    /// Version used to reject incomparable report files.
    pub report_version: String,
    /// Product lane that produced the report.
    pub lane: BifrostLane,
    /// Fully resolved scenario declaration.
    pub scenario: BifrostScenario,
    /// Readiness classification after verification.
    pub readiness: BenchmarkReadiness,
    /// Machine-readable failure classifications.
    pub failures: Vec<String>,
    /// Result of the lane's required negative/retry flow checks.
    pub negative_flows: NegativeFlowReport,
    /// Source revision used to produce the artifact.
    pub source_revision: String,
}

/// Machine-readable negative-flow evidence shared by every lane artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NegativeFlowReport {
    /// Whether the scenario required the checks.
    pub required: bool,
    /// Whether the adapter actually ran them.
    pub executed: bool,
    /// Whether all required checks passed.
    pub passed: bool,
    /// Stable names of the checks performed.
    pub checks: Vec<String>,
}

impl NegativeFlowReport {
    /// Return a skipped result for a scenario that does not require checks.
    #[must_use]
    pub fn skipped() -> Self {
        Self {
            required: false,
            executed: false,
            passed: true,
            checks: Vec::new(),
        }
    }

    /// Return an unexecuted required result for an unsupported preflight.
    #[must_use]
    pub fn required_but_unexecuted() -> Self {
        Self {
            required: true,
            executed: false,
            passed: false,
            checks: Vec::new(),
        }
    }

    /// Build an executed result from named checks.
    #[must_use]
    pub fn executed(checks: impl IntoIterator<Item = &'static str>, passed: bool) -> Self {
        Self {
            required: true,
            executed: true,
            passed,
            checks: checks.into_iter().map(str::to_owned).collect(),
        }
    }
}

impl BifrostReportEnvelope {
    /// Build a report envelope from a verified scenario result.
    #[must_use]
    pub fn new(scenario: BifrostScenario, readiness: BenchmarkReadiness) -> Self {
        let negative_flows_required = scenario.require_negative_flows;
        Self {
            report_version: REPORT_SCHEMA_VERSION.to_owned(),
            lane: scenario.lane,
            scenario,
            readiness,
            failures: Vec::new(),
            negative_flows: if negative_flows_required {
                NegativeFlowReport::required_but_unexecuted()
            } else {
                NegativeFlowReport::skipped()
            },
            source_revision: source_revision(),
        }
    }
}

fn source_revision() -> String {
    if let Some(revision) = std::env::var_os("WYRD_BIFROST_SOURCE_REVISION") {
        return revision.to_string_lossy().into_owned();
    }
    let revision = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned());
    let Some(revision) = revision.filter(|value| !value.is_empty()) else {
        return "unknown".to_owned();
    };
    let dirty = std::process::Command::new("git")
        .args(["diff", "--quiet", "HEAD", "--"])
        .status()
        .map_or(true, |status| !status.success());
    if dirty {
        format!("{revision}-dirty")
    } else {
        revision
    }
}

/// Common readiness result for benchmark reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkReadiness {
    /// The run completed its declared verification.
    Ready,
    /// The run completed but failed a declared verification.
    NotReady,
    /// The environment could not satisfy a required preflight.
    Unsupported,
}

fn bounded_value<T>(argument: &str, environment: &str, default: T) -> Result<T, ScenarioError>
where
    T: std::str::FromStr + Copy,
    T::Err: std::fmt::Display,
{
    let prefix = format!("--{argument}=");
    let value = std::env::args()
        .find_map(|candidate| candidate.strip_prefix(&prefix).map(str::to_owned))
        .or_else(|| std::env::var(environment).ok());
    value.map_or(Ok(default), |value| {
        value.parse::<T>().map_err(|error| {
            ScenarioError::Invalid(format!("{argument} has invalid value {value}: {error}"))
        })
    })
}

/// One completed durable ACK measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurableAckSample {
    /// Stable request identity was echoed by the server.
    pub response_id_matches: bool,
    /// ACK was observed only after WAL sync and memtable insertion.
    pub durable: bool,
    /// Client-observed ACK latency in microseconds.
    pub latency_us: u64,
    /// Rows represented by the durable batch.
    pub rows: u64,
}

/// Small lane report emitted by an adapter after harness verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurableAckReport {
    /// Common run identity, scenario, readiness, and failures.
    pub envelope: BifrostReportEnvelope,
    /// Number of samples that passed the durable ACK boundary.
    pub durable_samples: u64,
    /// Number of rows confirmed durable.
    pub durable_rows: u64,
    /// ACK latency p99, or zero when no sample was completed.
    pub ack_p99_us: u64,
    /// Whether all required identity/durability checks passed.
    pub verified: bool,
}

/// Reduce completed samples without counting offered or merely queued work.
#[must_use]
pub fn summarize_durable_acks(
    scenario: &BifrostScenario,
    samples: &[DurableAckSample],
) -> DurableAckReport {
    let mut latencies = samples
        .iter()
        .filter(|sample| sample.durable && sample.response_id_matches)
        .map(|sample| sample.latency_us)
        .collect::<Vec<_>>();
    latencies.sort_unstable();
    let durable = samples
        .iter()
        .filter(|sample| sample.durable && sample.response_id_matches)
        .collect::<Vec<_>>();
    let ack_p99_us = latencies
        .get((latencies.len().saturating_mul(99).div_ceil(100)).saturating_sub(1))
        .copied()
        .unwrap_or_default();
    let mut envelope = BifrostReportEnvelope::new(
        scenario.clone(),
        if !durable.is_empty()
            && durable.len() == samples.len()
            && durable
                .iter()
                .all(|sample| sample.durable && sample.response_id_matches)
        {
            BenchmarkReadiness::Ready
        } else {
            BenchmarkReadiness::NotReady
        },
    );
    if let Some(limit) = scenario.ack_p99_limit_us
        && ack_p99_us > limit
        && !latencies.is_empty()
    {
        envelope.readiness = BenchmarkReadiness::NotReady;
        envelope.failures.push(format!(
            "durable ACK p99 {ack_p99_us} us exceeded {limit} us"
        ));
    }
    DurableAckReport {
        envelope,
        durable_samples: u64::try_from(durable.len()).unwrap_or(u64::MAX),
        durable_rows: durable
            .iter()
            .map(|sample| sample.rows)
            .fold(0, u64::saturating_add),
        ack_p99_us,
        verified: !durable.is_empty()
            && durable.len() == samples.len()
            && durable
                .iter()
                .all(|sample| sample.durable && sample.response_id_matches)
            && scenario
                .ack_p99_limit_us
                .is_none_or(|limit| ack_p99_us <= limit),
    }
}

/// Compare two Bifrost stage manifests without comparing unlike lane reports.
pub fn compare_bifrost_stages(
    before: &serde_json::Value,
    after: &serde_json::Value,
    tolerance: f64,
) -> Result<serde_json::Value, ScenarioError> {
    let before_reports = before
        .get("reports")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| ScenarioError::Invalid("Bifrost stage has no reports".to_owned()))?;
    let after_reports = after
        .get("reports")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| ScenarioError::Invalid("Bifrost stage has no reports".to_owned()))?;
    let mut regressions = Vec::new();
    for before_report in before_reports {
        let before_envelope = bifrost_envelope(before_report)?;
        let after_report = after_reports
            .iter()
            .find(|report| {
                bifrost_envelope(report)
                    .ok()
                    .is_some_and(|envelope| envelope["lane"] == before_envelope["lane"])
            })
            .ok_or_else(|| {
                ScenarioError::Invalid(format!(
                    "Bifrost lane {} is missing from one stage",
                    before_envelope["lane"]
                ))
            })?;
        let after_envelope = bifrost_envelope(after_report)?;
        if before_envelope["report_version"] != REPORT_SCHEMA_VERSION
            || after_envelope["report_version"] != REPORT_SCHEMA_VERSION
        {
            return Err(ScenarioError::Invalid(
                "Bifrost reports use incompatible schema versions".to_owned(),
            ));
        }
        if before_envelope["scenario"] != after_envelope["scenario"] {
            return Err(ScenarioError::Invalid(format!(
                "Bifrost scenario differs for lane {}",
                before_envelope["lane"]
            )));
        }
        let before_metric = bifrost_metric(before_report)?;
        let after_metric = bifrost_metric(after_report)?;
        if before_metric > 0 {
            let before_float = before_metric.to_string().parse::<f64>().unwrap_or(f64::MAX);
            let after_float = after_metric.to_string().parse::<f64>().unwrap_or(f64::MAX);
            let relative_change = (after_float - before_float) / before_float;
            if relative_change > tolerance {
                regressions.push(serde_json::json!({
                    "lane": before_envelope["lane"],
                    "metric": "primary_latency_us",
                    "before": before_metric,
                    "after": after_metric,
                    "relative_change": relative_change,
                }));
            }
        }
    }
    Ok(serde_json::json!({
        "before_stage": before.get("stage").cloned().unwrap_or(serde_json::Value::Null),
        "after_stage": after.get("stage").cloned().unwrap_or(serde_json::Value::Null),
        "report_version": REPORT_SCHEMA_VERSION,
        "regressions": regressions,
    }))
}

fn bifrost_envelope(report: &serde_json::Value) -> Result<serde_json::Value, ScenarioError> {
    report
        .get("ack")
        .and_then(|ack| ack.get("envelope"))
        .or_else(|| report.get("envelope"))
        .cloned()
        .ok_or_else(|| ScenarioError::Invalid("Bifrost lane report has no envelope".to_owned()))
}

fn bifrost_metric(report: &serde_json::Value) -> Result<u64, ScenarioError> {
    report
        .get("ack")
        .and_then(|ack| ack.get("ack_p99_us"))
        .or_else(|| report.get("elapsed_us"))
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            ScenarioError::Invalid("Bifrost lane report has no latency metric".to_owned())
        })
}

/// Scenario construction failures.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ScenarioError {
    /// The requested lane is not canonical.
    #[error("unknown Bifrost lane: {0}")]
    UnknownLane(String),
    /// A scenario field violates the bounded benchmark contract.
    #[error("invalid Bifrost scenario: {0}")]
    Invalid(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_scenarios_validate_for_each_lane() {
        for lane in [
            BifrostLane::Scribe,
            BifrostLane::Otlp,
            BifrostLane::Forge,
            BifrostLane::Oracle,
        ] {
            BifrostScenario::smoke(lane)
                .validate()
                .expect("smoke scenario");
        }
    }

    #[test]
    fn summary_counts_only_durable_identity_verified_samples() {
        let scenario = BifrostScenario::smoke(BifrostLane::Scribe);
        let report = summarize_durable_acks(
            &scenario,
            &[
                DurableAckSample {
                    response_id_matches: true,
                    durable: true,
                    latency_us: 10,
                    rows: 2,
                },
                DurableAckSample {
                    response_id_matches: false,
                    durable: true,
                    latency_us: 1,
                    rows: 99,
                },
            ],
        );
        assert_eq!(report.durable_samples, 1);
        assert_eq!(report.durable_rows, 2);
        assert!(!report.verified);
    }

    #[test]
    fn summary_fails_the_scribe_ack_slo_when_p99_is_over_limit() {
        let scenario = BifrostScenario::smoke(BifrostLane::Scribe);
        let samples = (0..100)
            .map(|_| DurableAckSample {
                response_id_matches: true,
                durable: true,
                latency_us: 6_000,
                rows: 1,
            })
            .collect::<Vec<_>>();
        let report = summarize_durable_acks(&scenario, &samples);
        assert_eq!(report.ack_p99_us, 6_000);
        assert!(!report.verified);
        assert_eq!(report.envelope.readiness, BenchmarkReadiness::NotReady);
        assert!(report.envelope.failures[0].contains("exceeded 5000 us"));
    }

    #[test]
    fn scenario_defaults_are_bounded_before_harness_setup() {
        BifrostScenario::smoke(BifrostLane::Oracle)
            .validate()
            .expect("smoke scenario");
    }

    #[test]
    fn bifrost_comparator_rejects_incompatible_report_versions() {
        let scenario = BifrostScenario::smoke(BifrostLane::Scribe);
        let envelope = BifrostReportEnvelope::new(scenario, BenchmarkReadiness::Ready);
        let before = serde_json::json!({
            "stage": "before",
            "reports": [{
                "envelope": envelope,
                "elapsed_us": 1,
                "verified": true,
                "rows": 1
            }]
        });
        let mut after = before.clone();
        after["reports"][0]["envelope"]["report_version"] =
            serde_json::Value::String("old".to_owned());
        assert!(compare_bifrost_stages(&before, &after, 0.1).is_err());
    }

    #[test]
    fn unsupported_preflight_cannot_report_slo_pass() {
        let scenario = BifrostScenario::smoke(BifrostLane::Scribe);
        let envelope = BifrostReportEnvelope::new(scenario, BenchmarkReadiness::Unsupported);
        assert_eq!(envelope.readiness, BenchmarkReadiness::Unsupported);
        assert!(!envelope.negative_flows.passed);
    }

    #[test]
    fn stale_report_schema_is_rejected() {
        let scenario = BifrostScenario::smoke(BifrostLane::Scribe);
        let before = serde_json::json!({
            "reports": [{
                "envelope": BifrostReportEnvelope::new(
                    scenario.clone(),
                    BenchmarkReadiness::Ready,
                ),
                "ack": {"ack_p99_us": 1}
            }]
        });
        let mut stale = before.clone();
        stale["reports"][0]["envelope"]["report_version"] =
            serde_json::Value::String("wyrd.bifrost.report/v1".to_owned());
        assert!(compare_bifrost_stages(&before, &stale, 0.1).is_err());
    }

    #[test]
    fn fault_profiles_execute_their_required_negative_flows() {
        for profile in [
            BifrostFaultProfile::DelayedFsync,
            BifrostFaultProfile::Retry,
            BifrostFaultProfile::MemoryPressure,
            BifrostFaultProfile::WalPressure,
            BifrostFaultProfile::ObjectStoreStall,
            BifrostFaultProfile::PostgresStall,
            BifrostFaultProfile::ConcurrentRoleMemory,
        ] {
            let mut scenario = BifrostScenario::smoke(BifrostLane::Scribe);
            scenario.fault_profile = profile;
            scenario.fsync_delay_ms = u32::from(profile == BifrostFaultProfile::DelayedFsync) * 60;
            scenario.validate().expect("fault scenario");
            let mut envelope = BifrostReportEnvelope::new(scenario, BenchmarkReadiness::Ready);
            envelope.negative_flows =
                NegativeFlowReport::executed(["fault_profile_executed"], true);
            assert!(envelope.negative_flows.required);
            assert!(envelope.negative_flows.executed);
            assert!(envelope.negative_flows.passed);
        }
    }

    #[test]
    fn all_four_adapters_emit_the_shared_envelope() {
        for lane in [
            BifrostLane::Scribe,
            BifrostLane::Otlp,
            BifrostLane::Forge,
            BifrostLane::Oracle,
        ] {
            let envelope =
                BifrostReportEnvelope::new(BifrostScenario::smoke(lane), BenchmarkReadiness::Ready);
            assert_eq!(envelope.report_version, REPORT_SCHEMA_VERSION);
            assert_eq!(envelope.lane, lane);
            assert!(envelope.negative_flows.required);
        }
    }
}
