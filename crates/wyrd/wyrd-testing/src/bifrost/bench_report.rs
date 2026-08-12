//! Typed, causally-bound Bifrost capacity report contracts.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use wyrd_bench::{BenchmarkReadiness, BifrostReportEnvelope};

/// Closed schema identifier emitted by every canonical capacity report.
const CAPACITY_SCHEMA_VERSION: &str = "wyrd.bifrost.capacity/v1";
/// Floating-point tolerance for recomputed report ratios.
const RATIO_TOLERANCE: f64 = 1e-9;

/// Tier at which a report was captured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkTier {
    /// Short bounded smoke run; never promotable.
    Smoke,
    /// Authoritative qualification run.
    Qualification,
    /// Optional representative scale run.
    Scale,
}

/// Canonical telemetry window identity captured by T17.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TelemetryWindowRef {
    /// Stable run-local window identity.
    pub window_id: String,
    /// Inclusive start in Unix microseconds.
    pub started_at_micros: i64,
    /// Exclusive end in Unix microseconds.
    pub ended_at_micros: i64,
}

/// Live execution evidence required to construct a report stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageExecution {
    /// Benchmark run identity.
    pub run_id: String,
    /// Stable ordinal within the report.
    pub ordinal: u16,
    /// Canonical T17 capture window.
    pub capture_window: TelemetryWindowRef,
    /// SHA-256 digests of source observations used by this stage.
    pub observation_digests: Vec<String>,
}

/// Closed load-control shape for one stage.
///
/// The runner interprets each variant under one fixed rule: `value` is always a
/// ladder rung the fixture selects, and the `unit` names the offer law plus the
/// single controlling-throughput dimension the report attributes to the rung
/// (and, for the distributed family, the `efficiency(n)` numerator). Two offer
/// laws exist. Open-loop laws ([`Self::Qps`], [`Self::RequestsPerSec`]) offer
/// `value` operations per second; the controlling dimension is operations per
/// second. Closed-loop laws ([`Self::Concurrency`], [`Self::LogicalBytesPerSec`],
/// [`Self::ReturnedBytesPerSec`], [`Self::Streams`]) hold `value` operations in
/// flight for the measured window (a fixed-concurrency saturating loop); the
/// unit selects only which throughput the measured completions are reported as —
/// operations per second, logical scanned bytes per second, or terminal returned
/// bytes per second respectively. The rung `value` is what the saturation and
/// recommendation formulas compare across stages; the unit-selected throughput is
/// what the distributed body records per pod count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "unit", rename_all = "snake_case", deny_unknown_fields)]
pub enum StageControl {
    /// Open-loop query rate: offer `value` queries per second, bounding driver
    /// dispatch at `max_in_flight`. Controlling dimension: operations per second.
    Qps { value: u64, max_in_flight: u32 },
    /// Closed-loop concurrency: hold `value` operations in flight for the
    /// measured window. Controlling dimension: operations per second.
    Concurrency { value: u64 },
    /// Closed-loop result streams: hold `value` result streams in flight.
    /// Controlling dimension: operations per second.
    Streams { value: u64 },
    /// Closed-loop scan pressure: hold `value` queries in flight. Controlling
    /// dimension: logical scanned bytes per second summed over completions.
    LogicalBytesPerSec { value: u64 },
    /// Closed-loop return pressure: hold `value` queries in flight. Controlling
    /// dimension: terminal returned bytes per second summed over completions.
    ReturnedBytesPerSec { value: u64 },
    /// Open-loop write rate used by ingest: offer `value` writes per second.
    /// Controlling dimension: operations per second.
    RequestsPerSec { value: u64 },
}

/// Work offered, admitted, and completed in one stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct WorkCounts {
    /// Number of operations.
    pub operations: u64,
    /// Number of rows.
    pub rows: u64,
    /// Number of bytes.
    pub bytes: u64,
}

/// Latency percentile measurements in microseconds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct LatencyPercentiles {
    /// Admission latency.
    pub admission: Option<u64>,
    /// Queue latency.
    pub queue: Option<u64>,
    /// Planning latency.
    pub planning: Option<u64>,
    /// Time to first byte.
    pub ttfb: Option<u64>,
    /// Execution latency.
    pub execution: Option<u64>,
    /// Total latency.
    pub total: Option<u64>,
    /// Non-empty explanation required when any metric is unavailable.
    pub unavailable_reason: Option<String>,
}

/// Resource measurements associated with a stage.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ResourceSample {
    /// CPU seconds.
    pub cpu_seconds: Option<f64>,
    /// Final resident bytes.
    pub resident_bytes: Option<u64>,
    /// Peak query memory bytes.
    pub peak_query_memory_bytes: Option<u64>,
    /// Spill bytes.
    pub spill_bytes: Option<u64>,
    /// Peer/network bytes.
    pub network_bytes: Option<u64>,
}

/// Audit relay evidence associated with a stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct AuditRelaySample {
    /// Records waiting at the end of the stage.
    pub backlog_records: u64,
    /// Bytes waiting at the end of the stage.
    pub backlog_bytes: u64,
    /// Relay lag in microseconds.
    pub lag_micros: u64,
}

/// Exact correctness verdict for a stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum CorrectnessVerdict {
    /// All expected-result and isolation checks passed.
    Passed,
    /// One or more checks failed.
    Failed { reason: String },
}

/// Closed saturation cause set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SaturationCause {
    /// Controlled admission backpressure.
    ControlledAdmission,
    /// Latency exceeded the stage budget.
    Latency,
    /// Resource or spill ceiling.
    Resources,
    /// Driver missed operations or unexpected errors.
    Driver,
    /// Correctness or isolation failure.
    Correctness,
}

/// Result of replaying the highest healthy stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ReplayVerdict {
    /// Replay passed all required checks.
    Passed,
    /// Replay failed and the report is not promotable.
    Failed { reason: String },
}

/// One measured stage with required causal execution evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BifrostCapacityStage {
    /// Fixture workload/profile identity.
    pub profile_id: String,
    /// Stable stage ordinal.
    pub ordinal: u16,
    /// Offered load control.
    pub control: StageControl,
    /// Offered work.
    pub offered: WorkCounts,
    /// Admitted work.
    pub admitted: WorkCounts,
    /// Completed work.
    pub completed: WorkCounts,
    /// Controlled admission rejections.
    pub rejected: u64,
    /// Unexpected failures.
    pub failed: u64,
    /// Stage latency observations.
    pub latency: LatencyPercentiles,
    /// Logical bytes scanned or written.
    pub logical_bytes: u64,
    /// Physical bytes when the backend exposes them.
    pub physical_bytes: Option<u64>,
    /// Explanation required when physical bytes are absent.
    pub unavailable_reason: Option<String>,
    /// Rows returned by the stage.
    pub rows_returned: u64,
    /// Bytes returned by the stage.
    pub bytes_returned: u64,
    /// Resource evidence.
    pub resources: ResourceSample,
    /// Audit relay evidence.
    pub audit: AuditRelaySample,
    /// Jain fairness when an eligible denominator exists.
    pub fairness: Option<f64>,
    /// Exact correctness verdict.
    pub correctness: CorrectnessVerdict,
    /// Whether the closed healthy-stage rule passed.
    pub healthy: bool,
    /// Saturation classification, if any.
    pub saturation: Option<SaturationCause>,
    /// Recovery replay result, if this stage is a replay.
    pub recovery_replay: Option<ReplayVerdict>,
    /// Required live execution binding (D72).
    pub stage_execution: StageExecution,
}

impl BifrostCapacityStage {
    /// Construct a stage from a live [`StageExecution`] record.
    ///
    /// # Errors
    /// Returns [`CapacityReportError::InvalidExecution`] when the execution
    /// ordinal does not match the stage ordinal or has no observation digest.
    pub fn from_execution(
        profile_id: String,
        control: StageControl,
        ordinal: u16,
        execution: StageExecution,
    ) -> Result<Self, CapacityReportError> {
        validate_execution(ordinal, &execution)?;
        Ok(Self {
            profile_id,
            ordinal,
            control,
            offered: WorkCounts::default(),
            admitted: WorkCounts::default(),
            completed: WorkCounts::default(),
            rejected: 0,
            failed: 0,
            latency: LatencyPercentiles::default(),
            logical_bytes: 0,
            physical_bytes: None,
            unavailable_reason: Some("not recorded".to_owned()),
            rows_returned: 0,
            bytes_returned: 0,
            resources: ResourceSample::default(),
            audit: AuditRelaySample::default(),
            fairness: None,
            correctness: CorrectnessVerdict::Failed {
                reason: "not recorded".to_owned(),
            },
            healthy: false,
            saturation: None,
            recovery_replay: None,
            stage_execution: execution,
        })
    }

    /// Returns the stage's controlling load value for recommendation math.
    #[must_use]
    fn controlling_value(&self) -> u64 {
        match self.control {
            StageControl::Qps { value, .. }
            | StageControl::Concurrency { value }
            | StageControl::Streams { value }
            | StageControl::LogicalBytesPerSec { value }
            | StageControl::ReturnedBytesPerSec { value }
            | StageControl::RequestsPerSec { value } => value,
        }
    }

    /// Recomputes the closed healthy-stage predicate from measured evidence.
    #[must_use]
    fn derived_healthy(&self) -> bool {
        self.rejected == 0
            && self.failed == 0
            && self.offered.operations == self.admitted.operations
            && self.completed.operations == self.admitted.operations
            && matches!(self.correctness, CorrectnessVerdict::Passed)
            && self.audit.backlog_records == 0
            && self.audit.backlog_bytes == 0
            && self.latency.total.is_some()
    }

    /// Returns whether this nonhealthy stage differs from health solely by admission rejection.
    #[must_use]
    fn is_controlled_admission_only(&self) -> bool {
        self.rejected > 0
            && self.failed == 0
            && self.offered.operations == self.admitted.operations.saturating_add(self.rejected)
            && self.completed.operations == self.admitted.operations
            && matches!(self.correctness, CorrectnessVerdict::Passed)
            && self.audit.backlog_records == 0
            && self.audit.backlog_bytes == 0
            && self.latency.total.is_some()
    }

    /// Validates execution identity, computed health, and saturation classification.
    ///
    /// # Errors
    ///
    /// Returns a typed report error when the declared health or saturation does
    /// not match the evidence captured for this stage.
    fn validate(&self) -> Result<(), CapacityReportError> {
        validate_execution(self.ordinal, &self.stage_execution)?;
        if self.healthy != self.derived_healthy() {
            return Err(CapacityReportError::StageHealthMismatch);
        }
        match (&self.saturation, self.healthy) {
            (Some(SaturationCause::ControlledAdmission), false)
                if self.is_controlled_admission_only() =>
            {
                Ok(())
            }
            (Some(_), _) => Err(CapacityReportError::InvalidSaturation),
            (None, _) => Ok(()),
        }
    }
}

/// Validates live stage identity before it is bound into a capacity report.
///
/// # Errors
///
/// Returns [`CapacityReportError::InvalidExecution`] when the run/window
/// identity is empty or malformed, ordinals disagree, or a digest is absent.
fn validate_execution(ordinal: u16, execution: &StageExecution) -> Result<(), CapacityReportError> {
    if execution.ordinal != ordinal
        || execution.run_id.is_empty()
        || execution.capture_window.window_id.is_empty()
        || execution.capture_window.ended_at_micros <= execution.capture_window.started_at_micros
        || execution.observation_digests.is_empty()
        || execution.observation_digests.iter().any(|digest| {
            digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
    {
        return Err(CapacityReportError::InvalidExecution);
    }
    Ok(())
}

/// Recommended operating point derived from a saturated ladder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecommendedStage {
    /// Healthy stage ordinal.
    pub ordinal: u16,
    /// Controlling-unit value at the recommendation.
    pub controlling_value: u64,
    /// Fractional headroom below the first saturation stage.
    pub headroom_fraction: f64,
    /// First saturation stage ordinal.
    pub saturation_ordinal: u16,
    /// First saturation cause.
    pub saturation: SaturationCause,
}

/// Ingest report body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IngestBody {
    /// Workload/profile identity.
    pub profile_id: String,
    /// Whether a saturation boundary was observed.
    pub saturation_reached: bool,
    /// Recommendation, required iff saturation was reached.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommended: Option<RecommendedStage>,
}

/// Query report body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryBody {
    /// Query identifier (`q1` through `q5`).
    pub query_id: String,
    /// Whether a saturation boundary was observed.
    pub saturation_reached: bool,
    /// Recommendation, required iff saturation was reached.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommended: Option<RecommendedStage>,
}

/// Distributed report body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DistributedBody {
    /// Query identifier used for scale efficiency.
    pub query_id: String,
    /// Oracle pod count.
    pub oracle_pods: Vec<u16>,
    /// Measured controlling throughput X(n) at each pod count.
    pub throughput: Vec<u64>,
    /// Scale efficiency at each pod count.
    pub efficiency: Vec<f64>,
    /// Whether a saturation boundary was observed.
    pub saturation_reached: bool,
    /// Recommendation, required iff saturation was reached.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommended: Option<RecommendedStage>,
}

/// Mixed Scribe/Oracle/Forge report body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MixedBody {
    /// Diagonal workload fraction in percent.
    pub fractions: Vec<(u8, u8)>,
    /// Forge debt generations per tenant table.
    pub forge_debt_generations_per_tenant: u32,
    /// Whether a saturation boundary was observed.
    pub saturation_reached: bool,
    /// Recommendation, required iff saturation was reached.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommended: Option<RecommendedStage>,
}

/// Report envelope shared by all four benchmark families.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BifrostCapacityReport<Body> {
    /// Closed report schema identifier.
    pub schema_version: String,
    /// Qualification source commit.
    pub source_commit: String,
    /// Whether the source tree was clean.
    pub tree_clean: bool,
    /// Dataset manifest digest.
    pub dataset_digest: String,
    /// Workload fixture digest.
    pub workload_digest: String,
    /// Report tier.
    pub tier: BenchmarkTier,
    /// Topology identity.
    pub topology: TopologyIdentity,
    /// Environment identity.
    pub environment: EnvironmentIdentity,
    /// Causally-bound measured stages.
    pub stages: Vec<BifrostCapacityStage>,
    /// Family-specific report body.
    pub body: Body,
}

/// Immutable report identity supplied by a runner before stage construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapacityReportContext {
    /// Qualification source commit.
    pub source_commit: String,
    /// Whether the source tree was clean.
    pub tree_clean: bool,
    /// Dataset manifest digest.
    pub dataset_digest: String,
    /// Workload fixture digest.
    pub workload_digest: String,
    /// Report tier.
    pub tier: BenchmarkTier,
    /// Topology identity.
    pub topology: TopologyIdentity,
    /// Environment identity.
    pub environment: EnvironmentIdentity,
}

impl<Body> BifrostCapacityReport<Body>
where
    Body: Serialize + CapacityBody,
{
    /// Construct an immutable report from executed stages.
    ///
    /// # Errors
    /// Returns an error for missing execution evidence, invalid digest shape,
    /// or a recommendation whose presence disagrees with saturation.
    pub fn new(
        context: CapacityReportContext,
        stages: Vec<BifrostCapacityStage>,
        body: Body,
    ) -> Result<Self, CapacityReportError> {
        if context.source_commit.len() != 40
            || !context
                .source_commit
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(CapacityReportError::InvalidSourceCommit);
        }
        if !is_lowercase_sha256(&context.dataset_digest)
            || !is_lowercase_sha256(&context.workload_digest)
        {
            return Err(CapacityReportError::InvalidDigest);
        }
        for (index, stage) in stages.iter().enumerate() {
            if usize::from(stage.ordinal) != index {
                return Err(CapacityReportError::InvalidExecution);
            }
            stage.validate()?;
        }
        if body.saturation_reached() != body.recommended().is_some() {
            return Err(CapacityReportError::RecommendationMismatch);
        }
        validate_recommendation(&stages, body.saturation_reached(), body.recommended())?;
        body.validate()?;
        Ok(Self {
            schema_version: CAPACITY_SCHEMA_VERSION.to_owned(),
            source_commit: context.source_commit,
            tree_clean: context.tree_clean,
            dataset_digest: context.dataset_digest,
            workload_digest: context.workload_digest,
            tier: context.tier,
            topology: context.topology,
            environment: context.environment,
            stages,
            body,
        })
    }

    /// Serialize this report as sorted-key UTF-8 JSON with one trailing newline.
    ///
    /// # Errors
    /// Returns [`CapacityReportError::Serialization`] when a field cannot be
    /// represented by serde JSON.
    pub fn canonical_json(&self) -> Result<Vec<u8>, CapacityReportError> {
        let value = serde_json::to_value(self)
            .map_err(|error| CapacityReportError::Serialization(error.to_string()))?;
        let mut bytes = serde_json::to_vec(&sort_json(value))
            .map_err(|error| CapacityReportError::Serialization(error.to_string()))?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    /// Return the SHA-256 digest of canonical JSON bytes.
    ///
    /// # Errors
    /// Propagates canonical serialization failures.
    pub fn digest(&self) -> Result<String, CapacityReportError> {
        Ok(sha256_hex(&self.canonical_json()?))
    }

    /// Render a concise Markdown projection that states the JSON digest.
    ///
    /// # Errors
    /// Propagates canonical serialization failures.
    pub fn markdown(&self) -> Result<String, CapacityReportError> {
        Ok(format!(
            "# Bifrost capacity report\n\n- Tier: `{:?}`\n- JSON SHA-256: `{}`\n",
            self.tier,
            self.digest()?
        ))
    }

    /// Persist this report as canonical JSON under the tier-scoped run layout.
    ///
    /// Writes [`canonical_json`](Self::canonical_json) to
    /// `<output_root>/<run_id>/<family>-<tier>.json`, creating the run
    /// directory as needed. This inherent operation is the only sanctioned path
    /// from an in-memory report to a report file, so every downstream digest is
    /// recomputable from the same bytes via [`digest`](Self::digest). Creation
    /// uses `create_new`, so it is race-safe: an artifact already present for
    /// the same `(run_id, family, tier)` is never overwritten and instead
    /// yields [`ReportWriteError::AlreadyExists`].
    ///
    /// # Errors
    /// Returns [`ReportWriteError::RunId`] for a run id that is not a nonempty
    /// filesystem-safe token, [`ReportWriteError::Family`] for a family label
    /// that is not a nonempty lowercase `[a-z0-9-]` token,
    /// [`ReportWriteError::Serialize`] when canonical serialization fails,
    /// [`ReportWriteError::AlreadyExists`] when the target artifact already
    /// exists, and [`ReportWriteError::Io`] for a directory-creation or write
    /// failure.
    pub fn write_report(
        &self,
        output_root: &std::path::Path,
        run_id: &str,
        family: &str,
    ) -> Result<std::path::PathBuf, ReportWriteError> {
        use std::io::Write as _;
        if !is_run_id_token(run_id) {
            return Err(ReportWriteError::RunId(run_id.to_owned()));
        }
        if !is_family_token(family) {
            return Err(ReportWriteError::Family(family.to_owned()));
        }
        let bytes = self
            .canonical_json()
            .map_err(|error| ReportWriteError::Serialize(error.to_string()))?;
        let run_dir = output_root.join(run_id);
        let path = run_dir.join(format!("{family}-{}.json", tier_slug(self.tier)));
        std::fs::create_dir_all(&run_dir).map_err(|error| ReportWriteError::Io {
            path: run_dir.clone(),
            detail: error.to_string(),
        })?;
        let mut file = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(ReportWriteError::AlreadyExists(path));
            }
            Err(error) => {
                return Err(ReportWriteError::Io {
                    path,
                    detail: error.to_string(),
                });
            }
        };
        file.write_all(&bytes)
            .map_err(|error| ReportWriteError::Io {
                path: path.clone(),
                detail: error.to_string(),
            })?;
        Ok(path)
    }
}

/// Returns the stable on-disk slug for a report tier.
#[must_use]
fn tier_slug(tier: BenchmarkTier) -> &'static str {
    match tier {
        BenchmarkTier::Smoke => "smoke",
        BenchmarkTier::Qualification => "qualification",
        BenchmarkTier::Scale => "scale",
    }
}

/// Returns whether a report family label is a nonempty lowercase `[a-z0-9-]` token.
#[must_use]
fn is_family_token(family: &str) -> bool {
    !family.is_empty()
        && family
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// Returns whether a run id is a nonempty filesystem-safe `[A-Za-z0-9._-]` token.
#[must_use]
fn is_run_id_token(run_id: &str) -> bool {
    !run_id.is_empty()
        && run_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

/// Returns whether an identity is exactly one lowercase hexadecimal SHA-256 digest.
#[must_use]
fn is_lowercase_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

/// Body capabilities required by the generic report constructor.
pub trait CapacityBody {
    /// Return whether a saturation boundary was observed.
    fn saturation_reached(&self) -> bool;
    /// Return the optional recommendation.
    fn recommended(&self) -> Option<&RecommendedStage>;
    /// Validates body-specific evidence retained with the generic report.
    ///
    /// # Errors
    ///
    /// Returns a typed report error when the body evidence contradicts its
    /// family-specific capacity contract.
    fn validate(&self) -> Result<(), CapacityReportError>;
}

/// Implements the shared capacity-body behavior for families without extra evidence.
macro_rules! impl_capacity_body {
    ($($type:ty),+ $(,)?) => {$ (
        impl CapacityBody for $type {
            /// Returns whether this family observed a saturation boundary.
            fn saturation_reached(&self) -> bool { self.saturation_reached }
            /// Borrows the family recommendation when the boundary is valid.
            fn recommended(&self) -> Option<&RecommendedStage> { self.recommended.as_ref() }
            /// Confirms this body needs no family-specific evidence validation.
            ///
            /// # Errors
            ///
            /// This implementation cannot fail because its family has no
            /// additional formula beyond the shared report validation.
            fn validate(&self) -> Result<(), CapacityReportError> { Ok(()) }
        }
    )+ };
}
impl_capacity_body!(IngestBody, QueryBody, MixedBody);

impl CapacityBody for DistributedBody {
    /// Returns whether a distributed saturation boundary was observed.
    fn saturation_reached(&self) -> bool {
        self.saturation_reached
    }

    /// Returns the distributed ladder recommendation when saturation occurred.
    fn recommended(&self) -> Option<&RecommendedStage> {
        self.recommended.as_ref()
    }

    /// Validates the locked pod ladder and recomputes every efficiency value.
    ///
    /// # Errors
    ///
    /// Returns [`CapacityReportError::InvalidDistributedEvidence`] when the
    /// observations cannot prove the locked efficiency formula.
    fn validate(&self) -> Result<(), CapacityReportError> {
        if self.oracle_pods != [1, 2, 3, 6]
            || self.oracle_pods.len() != self.throughput.len()
            || self.throughput.len() != self.efficiency.len()
            || self.throughput.first().copied().unwrap_or_default() == 0
        {
            return Err(CapacityReportError::InvalidDistributedEvidence);
        }
        let baseline = self.throughput[0] as f64;
        for ((pods, throughput), efficiency) in self
            .oracle_pods
            .iter()
            .zip(&self.throughput)
            .zip(&self.efficiency)
        {
            let expected = *throughput as f64 / (f64::from(*pods) * baseline);
            if !efficiency.is_finite() || (*efficiency - expected).abs() > RATIO_TOLERANCE {
                return Err(CapacityReportError::InvalidDistributedEvidence);
            }
        }
        Ok(())
    }
}

/// Validates the first controlled saturation boundary and its derived recommendation.
///
/// # Errors
///
/// Returns [`CapacityReportError::InvalidRecommendation`] when saturation or
/// headroom does not follow the measured, ordered stage ladder.
fn validate_recommendation(
    stages: &[BifrostCapacityStage],
    saturation_reached: bool,
    recommended: Option<&RecommendedStage>,
) -> Result<(), CapacityReportError> {
    let first_nonhealthy = stages.iter().find(|stage| !stage.healthy);
    let first_saturation = stages.iter().find(|stage| stage.saturation.is_some());
    if !saturation_reached {
        return if first_saturation.is_none() && recommended.is_none() {
            Ok(())
        } else {
            Err(CapacityReportError::InvalidRecommendation)
        };
    }
    let (Some(boundary), Some(recommended), Some(first_nonhealthy)) =
        (first_saturation, recommended, first_nonhealthy)
    else {
        return Err(CapacityReportError::InvalidRecommendation);
    };
    if boundary.ordinal != first_nonhealthy.ordinal
        || !matches!(
            boundary.saturation,
            Some(SaturationCause::ControlledAdmission)
        )
        || !boundary.is_controlled_admission_only()
        || recommended.saturation_ordinal != boundary.ordinal
        || recommended.saturation != SaturationCause::ControlledAdmission
    {
        return Err(CapacityReportError::InvalidRecommendation);
    }
    let saturation_value = boundary.controlling_value();
    if saturation_value == 0 || !recommended.headroom_fraction.is_finite() {
        return Err(CapacityReportError::InvalidRecommendation);
    }
    let expected = stages
        .iter()
        .filter(|stage| {
            stage.healthy
                && u128::from(stage.controlling_value()) * 10 <= u128::from(saturation_value) * 7
        })
        .max_by_key(|stage| stage.ordinal)
        .ok_or(CapacityReportError::InvalidRecommendation)?;
    let expected_headroom = 1.0 - expected.controlling_value() as f64 / saturation_value as f64;
    if recommended.ordinal != expected.ordinal
        || recommended.controlling_value != expected.controlling_value()
        || (recommended.headroom_fraction - expected_headroom).abs() > RATIO_TOLERANCE
    {
        return Err(CapacityReportError::InvalidRecommendation);
    }
    Ok(())
}

/// Topology identity captured with a report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TopologyIdentity {
    /// Stable topology identifier.
    pub topology_id: String,
    /// Number of Oracle pods.
    pub oracle_pods: u16,
}

/// Environment identity captured with a report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentIdentity {
    /// CPU identity, when probeable.
    pub cpu: Option<String>,
    /// Memory bytes, when probeable.
    pub memory_bytes: Option<u64>,
    /// Operating-system identity.
    pub os: String,
    /// Storage mode identity.
    pub storage_mode: String,
    /// Explanation for unavailable probes.
    pub unavailable_reason: Option<String>,
}

/// Errors raised while constructing or serializing a capacity report.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CapacityReportError {
    /// Source commit is not a 40-character hexadecimal revision.
    #[error("source commit must be 40 hexadecimal characters")]
    InvalidSourceCommit,
    /// A dataset or workload digest is not a SHA-256 shape.
    #[error("dataset and workload digests must be 64 hexadecimal characters")]
    InvalidDigest,
    /// Stage execution evidence is absent or inconsistent.
    #[error("stage execution evidence is missing or inconsistent")]
    InvalidExecution,
    /// Recommendation presence disagrees with saturation state.
    #[error("recommended must be present if and only if saturation_reached is true")]
    RecommendationMismatch,
    /// A caller-supplied health flag disagrees with measured stage evidence.
    #[error("stage health does not match its measured evidence")]
    StageHealthMismatch,
    /// A saturation marker is not the sole controlled-admission failure.
    #[error("saturation must be a controlled-admission-only nonhealthy stage")]
    InvalidSaturation,
    /// Recommendation fields do not follow the ordered healthy/saturated ladder.
    #[error("recommendation does not match the first controlled-admission saturation boundary")]
    InvalidRecommendation,
    /// Distributed throughput observations cannot prove the locked efficiency formula.
    #[error("distributed throughput evidence is invalid")]
    InvalidDistributedEvidence,
    /// Serde could not serialize a report.
    #[error("capacity report serialization failed: {0}")]
    Serialization(String),
}

/// Errors raised while persisting a capacity report to its on-disk artifact.
#[derive(Debug, Error)]
pub enum ReportWriteError {
    /// The run id is not a nonempty filesystem-safe token.
    #[error("run id must be a nonempty filesystem-safe token: {0:?}")]
    RunId(String),
    /// The report family label is not a nonempty lowercase `[a-z0-9-]` token.
    #[error("report family must be a nonempty lowercase [a-z0-9-] token: {0:?}")]
    Family(String),
    /// Canonical serialization of the report failed.
    #[error("capacity report serialization failed: {0}")]
    Serialize(String),
    /// An artifact already exists for the same run id, family, and tier.
    #[error("refusing to overwrite existing report artifact at {0}")]
    AlreadyExists(std::path::PathBuf),
    /// A directory-creation or file-write operation failed.
    #[error("report io failure at {path}: {detail}")]
    Io {
        /// Path whose creation or write failed.
        path: std::path::PathBuf,
        /// Rendered underlying IO error.
        detail: String,
    },
}

/// Recursively sorts report JSON object keys while preserving array order.
fn sort_json(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut sorted = serde_json::Map::new();
            for (key, value) in map {
                sorted.insert(key, sort_json(value));
            }
            serde_json::Value::Object(sorted)
        }
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(sort_json).collect())
        }
        value => value,
    }
}

/// Computes a lowercase SHA-256 digest for canonical report bytes.
///
/// Exposed at crate scope so the family runners can derive observation and
/// workload digests from the same dependency-free implementation the report
/// uses, keeping every digest recomputable from identical bytes.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let mut state = Sha256::new();
    state.update(bytes);
    state.finish()
}

/// Incremental SHA-256 state for deterministic report digests.
struct Sha256 {
    /// Eight SHA-256 chaining words.
    state: [u32; 8],
    /// Partial block retained between updates.
    buffer: [u8; 64],
    /// Number of valid partial-block bytes.
    buffered: usize,
    /// Total absorbed bytes before final padding.
    length: u64,
}

impl Sha256 {
    /// Initializes the standard SHA-256 initial vector.
    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buffer: [0; 64],
            buffered: 0,
            length: 0,
        }
    }
    /// Absorbs bytes into complete compression blocks plus one partial block.
    fn update(&mut self, bytes: &[u8]) {
        self.length = self.length.saturating_add(bytes.len() as u64);
        let mut input = bytes;
        if self.buffered > 0 {
            let take = (64 - self.buffered).min(input.len());
            self.buffer[self.buffered..self.buffered + take].copy_from_slice(&input[..take]);
            self.buffered += take;
            input = &input[take..];
            if self.buffered == 64 {
                let block = self.buffer;
                self.compress(&block);
                self.buffered = 0;
            }
        }
        while input.len() >= 64 {
            self.compress(&input[..64]);
            input = &input[64..];
        }
        self.buffer[..input.len()].copy_from_slice(input);
        self.buffered = input.len();
    }
    /// Finalizes padding and returns lowercase hexadecimal output.
    fn finish(mut self) -> String {
        let bits = self.length.saturating_mul(8);
        self.buffer[self.buffered] = 0x80;
        self.buffered += 1;
        if self.buffered > 56 {
            self.buffer[self.buffered..].fill(0);
            let block = self.buffer;
            self.compress(&block);
            self.buffered = 0;
        }
        self.buffer[self.buffered..56].fill(0);
        self.buffer[56..].copy_from_slice(&bits.to_be_bytes());
        let block = self.buffer;
        self.compress(&block);
        self.state
            .into_iter()
            .map(|word| format!("{word:08x}"))
            .collect()
    }
    /// Applies the fixed SHA-256 compression permutation to one block.
    fn compress(&mut self, block: &[u8]) {
        /// SHA-256 round constants in the compression schedule.
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
            0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
            0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
            0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
            0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
            0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
            0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
            0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
            0xc67178f2,
        ];
        let mut words = [0_u32; 64];
        for (index, chunk) in block.chunks_exact(4).take(16).enumerate() {
            words[index] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        for index in 16..64 {
            let a = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let b = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(a)
                .wrapping_add(words[index - 7])
                .wrapping_add(b);
        }
        let mut s = self.state;
        for index in 0..64 {
            let ch = (s[4] & s[5]) ^ (!s[4] & s[6]);
            let e = s[4].rotate_right(6) ^ s[4].rotate_right(11) ^ s[4].rotate_right(25);
            let t1 = s[7]
                .wrapping_add(e)
                .wrapping_add(ch)
                .wrapping_add(K[index])
                .wrapping_add(words[index]);
            let maj = (s[0] & s[1]) ^ (s[0] & s[2]) ^ (s[1] & s[2]);
            let a = s[0].rotate_right(2) ^ s[0].rotate_right(13) ^ s[0].rotate_right(22);
            let t2 = a.wrapping_add(maj);
            s = [
                t1.wrapping_add(t2),
                s[0],
                s[1],
                s[2],
                s[3].wrapping_add(t1),
                s[4],
                s[5],
                s[6],
            ];
        }
        for (slot, value) in self.state.iter_mut().zip(s) {
            *slot = slot.wrapping_add(value);
        }
    }
}

/// Existing benchmark adapter report retained for legacy bench helpers.
#[derive(Debug, Serialize)]
pub struct LaneExecutionReport {
    /// Existing shared envelope.
    pub envelope: BifrostReportEnvelope,
    /// Elapsed benchmark time.
    pub elapsed_us: u64,
    /// Existing readiness result.
    pub verified: bool,
    /// Rows observed by the adapter.
    pub rows: u64,
}

/// Emit the retained adapter report to its task-local output path.
///
/// # Errors
/// Returns filesystem or JSON serialization errors.
pub fn emit_report(
    report: &LaneExecutionReport,
    lane: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let path = std::env::var_os("WYRD_BIFROST_REPORT").map_or_else(
        || repository_report_path(&format!("{lane}.json")),
        std::path::PathBuf::from,
    );
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, format!("{}\n", serde_json::to_string_pretty(report)?))?;
    Ok(())
}

/// Derives the repository-local legacy adapter output path for a lane name.
fn repository_report_path(file_name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join("target/bifrost-benchmarks/production-readiness")
        .join(file_name)
}

/// Map a boolean verification result to the retained readiness enum.
#[must_use]
pub fn readiness(verified: bool) -> BenchmarkReadiness {
    if verified {
        BenchmarkReadiness::Ready
    } else {
        BenchmarkReadiness::NotReady
    }
}

/// Pure unit tests for report construction, schema closure, and digest invariants.
#[cfg(test)]
mod tests {
    use super::*;

    /// Creates one valid healthy stage for report-construction tests.
    fn stage(ordinal: u16, value: u64) -> BifrostCapacityStage {
        BifrostCapacityStage {
            profile_id: "q1".to_owned(),
            ordinal,
            control: StageControl::Qps {
                value,
                max_in_flight: 256,
            },
            offered: WorkCounts {
                operations: 100,
                rows: 100,
                bytes: 100,
            },
            admitted: WorkCounts {
                operations: 100,
                rows: 100,
                bytes: 100,
            },
            completed: WorkCounts {
                operations: 100,
                rows: 100,
                bytes: 100,
            },
            rejected: 0,
            failed: 0,
            latency: LatencyPercentiles {
                total: Some(1),
                ..LatencyPercentiles::default()
            },
            logical_bytes: 100,
            physical_bytes: Some(100),
            unavailable_reason: None,
            rows_returned: 100,
            bytes_returned: 100,
            resources: ResourceSample::default(),
            audit: AuditRelaySample::default(),
            fairness: Some(1.0),
            correctness: CorrectnessVerdict::Passed,
            healthy: true,
            saturation: None,
            recovery_replay: None,
            stage_execution: StageExecution {
                run_id: "run-1".to_owned(),
                ordinal,
                capture_window: TelemetryWindowRef {
                    window_id: format!("window-{ordinal}"),
                    started_at_micros: 1,
                    ended_at_micros: 2,
                },
                observation_digests: vec!["a".repeat(64)],
            },
        }
    }

    /// Returns the fixed report environment used by pure contract tests.
    fn environment() -> EnvironmentIdentity {
        EnvironmentIdentity {
            cpu: Some("test".to_owned()),
            memory_bytes: Some(1),
            os: "test".to_owned(),
            storage_mode: "local".to_owned(),
            unavailable_reason: None,
        }
    }

    /// Returns the fixed single-pod topology used by pure contract tests.
    fn topology() -> TopologyIdentity {
        TopologyIdentity {
            topology_id: "one-pod".to_owned(),
            oracle_pods: 1,
        }
    }

    /// Returns the immutable report context used by all report constructors.
    fn context() -> CapacityReportContext {
        CapacityReportContext {
            source_commit: "a".repeat(40),
            tree_clean: true,
            dataset_digest: "b".repeat(64),
            workload_digest: "c".repeat(64),
            tier: BenchmarkTier::Smoke,
            topology: topology(),
            environment: environment(),
        }
    }

    /// Returns a valid first controlled-admission boundary and its healthy predecessor.
    fn saturated_stages() -> Vec<BifrostCapacityStage> {
        let healthy = stage(0, 100);
        let mut saturated = stage(1, 200);
        saturated.offered.operations = 100;
        saturated.admitted.operations = 80;
        saturated.completed.operations = 80;
        saturated.rejected = 20;
        saturated.healthy = false;
        saturated.saturation = Some(SaturationCause::ControlledAdmission);
        vec![healthy, saturated]
    }

    /// Returns the mathematically required recommendation for `saturated_stages`.
    fn recommendation() -> RecommendedStage {
        RecommendedStage {
            ordinal: 0,
            controlling_value: 100,
            headroom_fraction: 0.5,
            saturation_ordinal: 1,
            saturation: SaturationCause::ControlledAdmission,
        }
    }

    /// Proves stage construction refuses an absent or mismatched live execution record.
    #[test]
    fn stage_requires_live_execution() {
        assert!(
            BifrostCapacityStage::from_execution(
                "q1".to_owned(),
                StageControl::Qps {
                    value: 100,
                    max_in_flight: 256
                },
                1,
                StageExecution {
                    run_id: "run".to_owned(),
                    ordinal: 0,
                    capture_window: TelemetryWindowRef {
                        window_id: "w".to_owned(),
                        started_at_micros: 0,
                        ended_at_micros: 1
                    },
                    observation_digests: Vec::new()
                },
            )
            .is_err()
        );
    }

    /// Proves tier and stage execution are emitted in canonical sorted JSON.
    #[test]
    fn canonical_json_and_digest_are_stable() {
        let report = BifrostCapacityReport::new(
            context(),
            vec![stage(0, 100)],
            IngestBody {
                profile_id: "ingest-small".to_owned(),
                saturation_reached: false,
                recommended: None,
            },
        )
        .expect("report");
        let bytes = report.canonical_json().expect("canonical json");
        assert_eq!(bytes.last(), Some(&b'\n'));
        let text = std::str::from_utf8(&bytes).expect("utf8");
        assert!(text.find("\"body\"").unwrap() < text.find("\"dataset_digest\"").unwrap());
        assert!(!text.contains("\"recommended\":null"));
        assert_eq!(report.digest().expect("digest").len(), 64);
        assert!(
            report
                .markdown()
                .expect("markdown")
                .contains("JSON SHA-256")
        );
    }

    /// Proves the disk writer honors the tier layout and never overwrites.
    #[test]
    fn write_report_persists_and_refuses_overwrite() {
        let report = BifrostCapacityReport::new(
            context(),
            vec![stage(0, 100)],
            IngestBody {
                profile_id: "ingest-small".to_owned(),
                saturation_reached: false,
                recommended: None,
            },
        )
        .expect("report");
        let root = std::env::temp_dir().join(format!(
            "wyrd-bench-report-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or_default()
        ));

        let path = report
            .write_report(&root, "run-xyz", "ingest")
            .expect("first write");
        assert!(path.ends_with("run-xyz/ingest-smoke.json"));
        assert_eq!(
            std::fs::read(&path).expect("read artifact"),
            report.canonical_json().expect("canonical json"),
        );

        match report.write_report(&root, "run-xyz", "ingest") {
            Err(ReportWriteError::AlreadyExists(existing)) => assert_eq!(existing, path),
            other => panic!("expected AlreadyExists, got {other:?}"),
        }

        assert!(matches!(
            report.write_report(&root, "run-xyz", "Ingest"),
            Err(ReportWriteError::Family(_))
        ));
        assert!(matches!(
            report.write_report(&root, "../evil", "ingest"),
            Err(ReportWriteError::RunId(_))
        ));

        std::fs::remove_dir_all(&root).ok();
    }

    /// Proves recommendation presence and content are derived from saturation evidence.
    #[test]
    fn recommended_requires_saturation() {
        let body = IngestBody {
            profile_id: "ingest-small".to_owned(),
            saturation_reached: false,
            recommended: Some(RecommendedStage {
                ordinal: 0,
                controlling_value: 1,
                headroom_fraction: 0.5,
                saturation_ordinal: 1,
                saturation: SaturationCause::ControlledAdmission,
            }),
        };
        assert!(matches!(
            BifrostCapacityReport::new(context(), vec![stage(0, 100)], body),
            Err(CapacityReportError::RecommendationMismatch)
        ));
        let report = BifrostCapacityReport::new(
            context(),
            saturated_stages(),
            IngestBody {
                profile_id: "ingest-small".to_owned(),
                saturation_reached: true,
                recommended: Some(recommendation()),
            },
        )
        .expect("derived recommendation");
        assert!(
            std::str::from_utf8(&report.canonical_json().expect("json"))
                .expect("utf8")
                .contains("\"recommended\"")
        );
    }

    /// Proves constructor digest validation exactly matches the schema's lowercase SHA-256 domain.
    #[test]
    fn report_rejects_noncanonical_digest_identities() {
        for invalid in [
            "A".repeat(64),
            "g".repeat(64),
            "a".repeat(63),
            "a".repeat(65),
        ] {
            let mut dataset_context = context();
            dataset_context.dataset_digest = invalid.clone();
            assert!(matches!(
                BifrostCapacityReport::new(
                    dataset_context,
                    vec![stage(0, 100)],
                    IngestBody {
                        profile_id: "ingest".to_owned(),
                        saturation_reached: false,
                        recommended: None,
                    },
                ),
                Err(CapacityReportError::InvalidDigest)
            ));
            let mut workload_context = context();
            workload_context.workload_digest = invalid;
            assert!(matches!(
                BifrostCapacityReport::new(
                    workload_context,
                    vec![stage(0, 100)],
                    IngestBody {
                        profile_id: "ingest".to_owned(),
                        saturation_reached: false,
                        recommended: None,
                    },
                ),
                Err(CapacityReportError::InvalidDigest)
            ));
        }
    }

    /// Proves each healthy predicate and recommendation field rejects mutation.
    #[test]
    fn report_semantics_reject_mutations() {
        let mut mutations = Vec::new();
        let mut rejected = stage(0, 100);
        rejected.rejected = 1;
        mutations.push(rejected);
        let mut failed = stage(0, 100);
        failed.failed = 1;
        mutations.push(failed);
        let mut unaccounted = stage(0, 100);
        unaccounted.admitted.operations = 99;
        mutations.push(unaccounted);
        let mut incomplete = stage(0, 100);
        incomplete.completed.operations = 99;
        mutations.push(incomplete);
        let mut incorrect = stage(0, 100);
        incorrect.correctness = CorrectnessVerdict::Failed {
            reason: "mutation".to_owned(),
        };
        mutations.push(incorrect);
        let mut audit = stage(0, 100);
        audit.audit.backlog_records = 1;
        mutations.push(audit);
        let mut latency = stage(0, 100);
        latency.latency.total = None;
        mutations.push(latency);
        for mutated in mutations {
            assert!(matches!(
                BifrostCapacityReport::new(
                    context(),
                    vec![mutated],
                    IngestBody {
                        profile_id: "ingest-small".to_owned(),
                        saturation_reached: false,
                        recommended: None,
                    },
                ),
                Err(CapacityReportError::StageHealthMismatch)
            ));
        }
        for recommended in [
            RecommendedStage {
                ordinal: 1,
                ..recommendation()
            },
            RecommendedStage {
                controlling_value: 99,
                ..recommendation()
            },
            RecommendedStage {
                headroom_fraction: 0.25,
                ..recommendation()
            },
            RecommendedStage {
                saturation_ordinal: 0,
                ..recommendation()
            },
            RecommendedStage {
                saturation: SaturationCause::Latency,
                ..recommendation()
            },
        ] {
            assert!(matches!(
                BifrostCapacityReport::new(
                    context(),
                    saturated_stages(),
                    IngestBody {
                        profile_id: "ingest-small".to_owned(),
                        saturation_reached: true,
                        recommended: Some(recommended.clone()),
                    },
                ),
                Err(CapacityReportError::InvalidRecommendation)
            ));
        }
    }

    /// Prove the dependency-free digest helper matches the SHA-256 standard vector.
    #[test]
    fn sha256_matches_standard_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    /// Proves distributed evidence validates the locked ladder and formula.
    #[test]
    fn distributed_efficiency_rejects_mutations() {
        let body = DistributedBody {
            query_id: "q1".to_owned(),
            oracle_pods: vec![1, 2, 3, 6],
            throughput: vec![100, 180, 240, 420],
            efficiency: vec![1.0, 0.9, 0.8, 0.7],
            saturation_reached: false,
            recommended: None,
        };
        assert!(BifrostCapacityReport::new(context(), vec![stage(0, 100)], body.clone()).is_ok());
        for invalid in [
            DistributedBody {
                oracle_pods: vec![1, 2],
                ..body.clone()
            },
            DistributedBody {
                throughput: vec![0, 180, 240, 420],
                ..body.clone()
            },
            DistributedBody {
                efficiency: vec![1.0, 0.8, 0.8, 0.7],
                ..body.clone()
            },
        ] {
            assert!(matches!(
                BifrostCapacityReport::new(context(), vec![stage(0, 100)], invalid.clone()),
                Err(CapacityReportError::InvalidDistributedEvidence)
            ));
        }
    }

    /// Proves serde rejects top-level and nested unknown report fields.
    #[test]
    fn report_schema_rejects_unknown_fields() {
        let report = BifrostCapacityReport::new(
            context(),
            vec![stage(0, 100)],
            IngestBody {
                profile_id: "ingest-small".to_owned(),
                saturation_reached: false,
                recommended: None,
            },
        )
        .expect("report");
        let mut top_level = serde_json::to_value(&report).expect("json");
        top_level["unexpected"] = serde_json::Value::Null;
        assert!(serde_json::from_value::<BifrostCapacityReport<IngestBody>>(top_level).is_err());
        let mut nested = serde_json::to_value(report).expect("json");
        nested["stages"][0]["stage_execution"]["unexpected"] = serde_json::Value::Null;
        assert!(serde_json::from_value::<BifrostCapacityReport<IngestBody>>(nested).is_err());
    }

    /// Evaluates every validation keyword emitted by the checked-in report schema.
    ///
    /// # Errors
    ///
    /// Returns a JSON-pointer diagnostic when the instance violates the
    /// fixture, a local reference is unresolved or cyclic, or the fixture
    /// introduces a keyword this deliberately narrow evaluator does not own.
    fn fixture_schema_accepts(
        schema: &serde_json::Value,
        instance: &serde_json::Value,
    ) -> Result<(), String> {
        audit_schema_keywords(schema, "#")?;
        evaluate_schema(schema, schema, instance, "", &mut Vec::new())
    }

    /// Rejects an unsupported validation keyword before instance evaluation can skip it.
    ///
    /// # Errors
    ///
    /// Returns a schema-pointer diagnostic for a keyword or nested schema shape
    /// that this fixture evaluator cannot interpret exhaustively.
    fn audit_schema_keywords(schema: &serde_json::Value, pointer: &str) -> Result<(), String> {
        let Some(object) = schema.as_object() else {
            return if schema.is_boolean() {
                Ok(())
            } else {
                Err(format!("{pointer}: schema must be an object or boolean"))
            };
        };
        for key in object.keys() {
            if !matches!(
                key.as_str(),
                "$schema"
                    | "$id"
                    | "$defs"
                    | "$ref"
                    | "title"
                    | "description"
                    | "type"
                    | "properties"
                    | "required"
                    | "additionalProperties"
                    | "items"
                    | "prefixItems"
                    | "minItems"
                    | "minLength"
                    | "enum"
                    | "const"
                    | "pattern"
                    | "minimum"
                    | "oneOf"
                    | "allOf"
                    | "if"
                    | "then"
                    | "else"
                    | "not"
            ) {
                return Err(format!("{pointer}: unsupported schema keyword `{key}`"));
            }
        }
        for container in ["$defs", "properties"] {
            if let Some(entries) = object.get(container).and_then(serde_json::Value::as_object) {
                for (name, nested) in entries {
                    audit_schema_keywords(nested, &format!("{pointer}/{container}/{name}"))?;
                }
            }
        }
        for key in ["additionalProperties", "items", "if", "then", "else", "not"] {
            if let Some(nested) = object.get(key) {
                audit_schema_keywords(nested, &format!("{pointer}/{key}"))?;
            }
        }
        for key in ["prefixItems", "oneOf", "allOf"] {
            if let Some(items) = object.get(key).and_then(serde_json::Value::as_array) {
                for (index, nested) in items.iter().enumerate() {
                    audit_schema_keywords(nested, &format!("{pointer}/{key}/{index}"))?;
                }
            }
        }
        Ok(())
    }

    /// Recursively evaluates a local schema node against one JSON value.
    ///
    /// # Errors
    ///
    /// Returns a JSON-pointer diagnostic for validation failure, malformed
    /// local reference, or cyclic `$ref` resolution.
    fn evaluate_schema(
        root: &serde_json::Value,
        schema: &serde_json::Value,
        instance: &serde_json::Value,
        pointer: &str,
        refs: &mut Vec<String>,
    ) -> Result<(), String> {
        if let Some(allowed) = schema.as_bool() {
            return allowed
                .then_some(())
                .ok_or_else(|| format!("{pointer}: false schema rejects instance"));
        }
        let object = schema
            .as_object()
            .ok_or_else(|| format!("{pointer}: schema must be an object"))?;
        if let Some(reference) = object.get("$ref").and_then(serde_json::Value::as_str) {
            let local = reference
                .strip_prefix('#')
                .ok_or_else(|| format!("{pointer}: non-local ref `{reference}`"))?;
            if refs.iter().any(|seen| seen == reference) {
                return Err(format!("{pointer}: cyclic ref `{reference}`"));
            }
            let target = root
                .pointer(local)
                .ok_or_else(|| format!("{pointer}: unresolved ref `{reference}`"))?;
            refs.push(reference.to_owned());
            let result = evaluate_schema(root, target, instance, pointer, refs);
            refs.pop();
            return result;
        }
        if let Some(types) = object.get("type") {
            let valid = if let Some(kind) = types.as_str() {
                instance_has_type(instance, kind)
            } else if let Some(kinds) = types.as_array() {
                kinds
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .any(|kind| instance_has_type(instance, kind))
            } else {
                false
            };
            if !valid {
                return Err(format!("{pointer}: type mismatch"));
            }
        }
        if let Some(expected) = object.get("const")
            && instance != expected
        {
            return Err(format!("{pointer}: const mismatch"));
        }
        if let Some(values) = object.get("enum").and_then(serde_json::Value::as_array)
            && !values.iter().any(|value| value == instance)
        {
            return Err(format!("{pointer}: enum mismatch"));
        }
        if let Some(pattern) = object.get("pattern").and_then(serde_json::Value::as_str) {
            let string = instance
                .as_str()
                .ok_or_else(|| format!("{pointer}: pattern requires string"))?;
            if !matches_pattern(pattern, string) {
                return Err(format!("{pointer}: pattern mismatch `{pattern}`"));
            }
        }
        if let Some(minimum) = object.get("minLength").and_then(serde_json::Value::as_u64)
            && instance
                .as_str()
                .is_none_or(|string| string.len() < minimum as usize)
        {
            return Err(format!("{pointer}: shorter than {minimum} characters"));
        }
        if let Some(minimum) = object.get("minimum").and_then(serde_json::Value::as_f64)
            && instance.as_f64().is_none_or(|number| number < minimum)
        {
            return Err(format!("{pointer}: below minimum {minimum}"));
        }
        if let Some(minimum) = object.get("minItems").and_then(serde_json::Value::as_u64)
            && instance
                .as_array()
                .is_none_or(|items| items.len() < minimum as usize)
        {
            return Err(format!("{pointer}: fewer than {minimum} items"));
        }
        if let Some(required) = object.get("required").and_then(serde_json::Value::as_array) {
            let fields = instance
                .as_object()
                .ok_or_else(|| format!("{pointer}: required requires object"))?;
            for field in required.iter().filter_map(serde_json::Value::as_str) {
                if !fields.contains_key(field) {
                    return Err(format!("{pointer}/{field}: required property missing"));
                }
            }
        }
        if let Some(properties) = object
            .get("properties")
            .and_then(serde_json::Value::as_object)
        {
            let fields = instance
                .as_object()
                .ok_or_else(|| format!("{pointer}: properties require object"))?;
            if matches!(
                object.get("additionalProperties"),
                Some(serde_json::Value::Bool(false))
            ) {
                for field in fields.keys() {
                    if !properties.contains_key(field) {
                        return Err(format!("{pointer}/{field}: additional property"));
                    }
                }
            }
            for (field, property_schema) in properties {
                if let Some(value) = fields.get(field) {
                    evaluate_schema(
                        root,
                        property_schema,
                        value,
                        &format!("{pointer}/{field}"),
                        refs,
                    )?;
                }
            }
        }
        let prefix_length = object
            .get("prefixItems")
            .and_then(serde_json::Value::as_array)
            .map_or(0, Vec::len);
        if let Some(items_schema) = object.get("items") {
            let items = instance
                .as_array()
                .ok_or_else(|| format!("{pointer}: items require array"))?;
            for (index, value) in items.iter().enumerate().skip(prefix_length) {
                evaluate_schema(
                    root,
                    items_schema,
                    value,
                    &format!("{pointer}/{index}"),
                    refs,
                )?;
            }
        }
        if let Some(prefix) = object
            .get("prefixItems")
            .and_then(serde_json::Value::as_array)
        {
            let items = instance
                .as_array()
                .ok_or_else(|| format!("{pointer}: prefixItems require array"))?;
            for (index, item_schema) in prefix.iter().enumerate() {
                let value = items
                    .get(index)
                    .ok_or_else(|| format!("{pointer}/{index}: prefix item missing"))?;
                evaluate_schema(
                    root,
                    item_schema,
                    value,
                    &format!("{pointer}/{index}"),
                    refs,
                )?;
            }
        }
        for key in ["allOf"] {
            if let Some(schemas) = object.get(key).and_then(serde_json::Value::as_array) {
                for nested in schemas {
                    evaluate_schema(root, nested, instance, pointer, refs)?;
                }
            }
        }
        if let Some(schemas) = object.get("oneOf").and_then(serde_json::Value::as_array) {
            let mut matches = 0;
            let mut last_error = None;
            for nested in schemas {
                match evaluate_schema(root, nested, instance, pointer, refs) {
                    Ok(()) => matches += 1,
                    Err(diagnostic) => last_error = Some(diagnostic),
                }
            }
            if matches != 1 {
                return Err(last_error
                    .unwrap_or_else(|| format!("{pointer}: oneOf matched {matches} branches")));
            }
        }
        if let Some(condition) = object.get("if") {
            let branch = if evaluate_schema(root, condition, instance, pointer, refs).is_ok() {
                object.get("then")
            } else {
                object.get("else")
            };
            if let Some(branch) = branch {
                evaluate_schema(root, branch, instance, pointer, refs)?;
            }
        }
        if let Some(negated) = object.get("not")
            && evaluate_schema(root, negated, instance, pointer, refs).is_ok()
        {
            return Err(format!("{pointer}: not schema matched"));
        }
        Ok(())
    }

    /// Classifies the subset of JSON types used by the checked-in fixture.
    #[must_use]
    fn instance_has_type(value: &serde_json::Value, kind: &str) -> bool {
        match kind {
            "null" => value.is_null(),
            "boolean" => value.is_boolean(),
            "object" => value.is_object(),
            "array" => value.is_array(),
            "string" => value.is_string(),
            "number" => value.is_number(),
            "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
            _ => false,
        }
    }

    /// Evaluates the exact anchored hexadecimal patterns emitted by this fixture.
    #[must_use]
    fn matches_pattern(pattern: &str, value: &str) -> bool {
        match pattern {
            "^[0-9a-f]{64}$" => is_lowercase_sha256(value),
            "^[0-9a-fA-F]{40}$" => {
                value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
            }
            "^[0-9a-fA-F]{64}$" => {
                value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
            }
            _ => false,
        }
    }

    /// Resolves one exact schema pointer for a deliberate evaluator-regression mutation.
    ///
    /// # Panics
    ///
    /// Panics when the checked-in fixture no longer exposes `pointer`, because
    /// silently creating a mutation target would not prove evaluator coverage.
    fn schema_mutation_target<'schema>(
        schema: &'schema mut serde_json::Value,
        pointer: &str,
    ) -> &'schema mut serde_json::Value {
        schema
            .pointer_mut(pointer)
            .unwrap_or_else(|| panic!("schema mutation target must exist: {pointer}"))
    }

    /// Asserts that a mutated fixture rejects an instance at its expected JSON pointer.
    fn assert_mutated_schema_rejects(
        schema: &serde_json::Value,
        instance: &serde_json::Value,
        expected_pointer: &str,
    ) {
        let result = fixture_schema_accepts(schema, instance);
        assert!(
            result
                .as_ref()
                .is_err_and(|diagnostic| diagnostic.contains(expected_pointer)),
            "expected evaluator rejection at {expected_pointer}, got {result:?}"
        );
    }

    /// Proves the checked-in schema accepts every canonical body form and rejects contract mutations.
    #[test]
    fn schema_fixture_validates_body_forms_and_negatives() {
        let schema: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../fixtures/bifrost/qualification/capacity-report-schema.json"
        ))
        .expect("schema fixture");
        let unsaturated = [
            serde_json::to_value(
                BifrostCapacityReport::new(
                    context(),
                    vec![stage(0, 100)],
                    IngestBody {
                        profile_id: "ingest".to_owned(),
                        saturation_reached: false,
                        recommended: None,
                    },
                )
                .expect("ingest"),
            )
            .expect("json"),
            serde_json::to_value(
                BifrostCapacityReport::new(
                    context(),
                    vec![stage(0, 100)],
                    QueryBody {
                        query_id: "q1".to_owned(),
                        saturation_reached: false,
                        recommended: None,
                    },
                )
                .expect("query"),
            )
            .expect("json"),
            serde_json::to_value(
                BifrostCapacityReport::new(
                    context(),
                    vec![stage(0, 100)],
                    DistributedBody {
                        query_id: "q1".to_owned(),
                        oracle_pods: vec![1, 2, 3, 6],
                        throughput: vec![100, 180, 240, 420],
                        efficiency: vec![1.0, 0.9, 0.8, 0.7],
                        saturation_reached: false,
                        recommended: None,
                    },
                )
                .expect("distributed"),
            )
            .expect("json"),
            serde_json::to_value(
                BifrostCapacityReport::new(
                    context(),
                    vec![stage(0, 100)],
                    MixedBody {
                        fractions: vec![(25, 25)],
                        forge_debt_generations_per_tenant: 64,
                        saturation_reached: false,
                        recommended: None,
                    },
                )
                .expect("mixed"),
            )
            .expect("json"),
        ];
        for instance in &unsaturated {
            let result = fixture_schema_accepts(&schema, instance);
            assert!(result.is_ok(), "unsaturated schema result: {result:?}");
        }
        let saturated = [
            serde_json::to_value(
                BifrostCapacityReport::new(
                    context(),
                    saturated_stages(),
                    IngestBody {
                        profile_id: "ingest".to_owned(),
                        saturation_reached: true,
                        recommended: Some(recommendation()),
                    },
                )
                .expect("saturated ingest"),
            )
            .expect("json"),
            serde_json::to_value(
                BifrostCapacityReport::new(
                    context(),
                    saturated_stages(),
                    QueryBody {
                        query_id: "q1".to_owned(),
                        saturation_reached: true,
                        recommended: Some(recommendation()),
                    },
                )
                .expect("saturated query"),
            )
            .expect("json"),
            serde_json::to_value(
                BifrostCapacityReport::new(
                    context(),
                    saturated_stages(),
                    DistributedBody {
                        query_id: "q1".to_owned(),
                        oracle_pods: vec![1, 2, 3, 6],
                        throughput: vec![100, 180, 240, 420],
                        efficiency: vec![1.0, 0.9, 0.8, 0.7],
                        saturation_reached: true,
                        recommended: Some(recommendation()),
                    },
                )
                .expect("saturated distributed"),
            )
            .expect("json"),
            serde_json::to_value(
                BifrostCapacityReport::new(
                    context(),
                    saturated_stages(),
                    MixedBody {
                        fractions: vec![(25, 25)],
                        forge_debt_generations_per_tenant: 64,
                        saturation_reached: true,
                        recommended: Some(recommendation()),
                    },
                )
                .expect("saturated mixed"),
            )
            .expect("json"),
        ];
        for instance in &saturated {
            let result = fixture_schema_accepts(&schema, instance);
            assert!(result.is_ok(), "saturated schema result: {result:?}");
        }
        let mut missing_tier = saturated[0].clone();
        missing_tier
            .as_object_mut()
            .expect("report object")
            .remove("tier");
        assert!(fixture_schema_accepts(&schema, &missing_tier).is_err());
        let mut malformed_execution = saturated[0].clone();
        malformed_execution["stages"][0]["stage_execution"] = serde_json::json!({"run_id": "run"});
        assert!(fixture_schema_accepts(&schema, &malformed_execution).is_err());
        let mut nested_unknown = saturated[0].clone();
        nested_unknown["stages"][0]["stage_execution"]["unexpected"] = serde_json::Value::Null;
        assert!(fixture_schema_accepts(&schema, &nested_unknown).is_err());
        let mut uppercase_digest = saturated[0].clone();
        uppercase_digest["dataset_digest"] = serde_json::Value::String("A".repeat(64));
        assert!(fixture_schema_accepts(&schema, &uppercase_digest).is_err());
        let mut nonhex_digest = saturated[0].clone();
        nonhex_digest["workload_digest"] = serde_json::Value::String("g".repeat(64));
        assert!(fixture_schema_accepts(&schema, &nonhex_digest).is_err());
        let mut invalid_tier = saturated[0].clone();
        invalid_tier["tier"] = serde_json::Value::String("invalid".to_owned());
        assert!(fixture_schema_accepts(&schema, &invalid_tier).is_err());
        let mut malformed_control = saturated[0].clone();
        malformed_control["stages"][0]["control"] =
            serde_json::json!({"unit": "qps", "value": 100});
        assert!(fixture_schema_accepts(&schema, &malformed_control).is_err());
        let mut malformed_window = saturated[0].clone();
        malformed_window["stages"][0]["stage_execution"]["capture_window"] =
            serde_json::json!({"window_id": "window"});
        assert!(fixture_schema_accepts(&schema, &malformed_window).is_err());
        let mut empty_execution = saturated[0].clone();
        empty_execution["stages"][0]["stage_execution"]["observation_digests"] =
            serde_json::json!([]);
        assert!(fixture_schema_accepts(&schema, &empty_execution).is_err());
        let mut wrong_body = saturated[0].clone();
        wrong_body["body"]["query_id"] = serde_json::Value::String("q1".to_owned());
        assert!(fixture_schema_accepts(&schema, &wrong_body).is_err());
        let mut false_with_recommended = unsaturated[0].clone();
        false_with_recommended["body"]["recommended"] =
            serde_json::to_value(recommendation()).expect("recommendation");
        assert!(fixture_schema_accepts(&schema, &false_with_recommended).is_err());
        let mut true_without_recommended = saturated[0].clone();
        true_without_recommended["body"]
            .as_object_mut()
            .expect("body object")
            .remove("recommended");
        assert!(fixture_schema_accepts(&schema, &true_without_recommended).is_err());

        let mut enum_weakened = schema.clone();
        enum_weakened["properties"]["tier"]["enum"] = serde_json::json!(["broken"]);
        assert!(fixture_schema_accepts(&enum_weakened, &saturated[0]).is_err());
        let mut pattern_weakened = schema.clone();
        pattern_weakened["properties"]["dataset_digest"]["pattern"] =
            serde_json::json!("unsupported");
        assert!(fixture_schema_accepts(&pattern_weakened, &saturated[0]).is_err());
        let mut minimum_weakened = schema.clone();
        minimum_weakened["$defs"]["stage"]["properties"]["ordinal"]["minimum"] =
            serde_json::json!(1);
        assert!(fixture_schema_accepts(&minimum_weakened, &saturated[0]).is_err());
    }

    /// Proves every fixture keyword family has a mutation-sensitive evaluator regression.
    #[test]
    fn schema_evaluator_rejects_every_keyword_family_mutation() {
        let schema: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../fixtures/bifrost/qualification/capacity-report-schema.json"
        ))
        .expect("schema fixture");
        let ingest = serde_json::to_value(
            BifrostCapacityReport::new(
                context(),
                vec![stage(0, 100)],
                IngestBody {
                    profile_id: "ingest".to_owned(),
                    saturation_reached: false,
                    recommended: None,
                },
            )
            .expect("ingest report"),
        )
        .expect("ingest json");
        let mixed = serde_json::to_value(
            BifrostCapacityReport::new(
                context(),
                vec![stage(0, 100)],
                MixedBody {
                    fractions: vec![(25, 25)],
                    forge_debt_generations_per_tenant: 64,
                    saturation_reached: false,
                    recommended: None,
                },
            )
            .expect("mixed report"),
        )
        .expect("mixed json");

        let mut local_ref = schema.clone();
        *schema_mutation_target(&mut local_ref, "/properties/stages/items/$ref") =
            serde_json::json!("#/$defs/missing");
        assert_mutated_schema_rejects(&local_ref, &ingest, "/stages/0");

        let mut scalar_type = schema.clone();
        *schema_mutation_target(&mut scalar_type, "/properties/tree_clean/type") =
            serde_json::json!("string");
        assert_mutated_schema_rejects(&scalar_type, &ingest, "/tree_clean");

        let mut nullable_type = schema.clone();
        *schema_mutation_target(&mut nullable_type, "/$defs/environment/properties/cpu/type") =
            serde_json::json!(["null"]);
        assert_mutated_schema_rejects(&nullable_type, &ingest, "/environment/cpu");

        let mut properties_and_closure = schema.clone();
        schema_mutation_target(&mut properties_and_closure, "/$defs/stage/properties")
            .as_object_mut()
            .expect("stage properties object")
            .remove("profile_id")
            .expect("stage profile_id property");
        assert_mutated_schema_rejects(&properties_and_closure, &ingest, "/stages/0/profile_id");

        let mut required = schema.clone();
        schema_mutation_target(&mut required, "/required")
            .as_array_mut()
            .expect("root required array")
            .push(serde_json::json!("missing"));
        assert_mutated_schema_rejects(&required, &ingest, "/missing");

        let mut items = schema.clone();
        *schema_mutation_target(&mut items, "/properties/stages/items") =
            serde_json::Value::Bool(false);
        assert_mutated_schema_rejects(&items, &ingest, "/stages/0");

        let mut prefix_items = schema.clone();
        *schema_mutation_target(
            &mut prefix_items,
            "/$defs/mixed_body/properties/fractions/items/prefixItems/0/type",
        ) = serde_json::json!("string");
        assert_mutated_schema_rejects(&prefix_items, &mixed, "/body/fractions/0/0");

        let mut min_items = schema.clone();
        *schema_mutation_target(
            &mut min_items,
            "/$defs/execution/properties/observation_digests/minItems",
        ) = serde_json::json!(2);
        assert_mutated_schema_rejects(
            &min_items,
            &ingest,
            "/stages/0/stage_execution/observation_digests",
        );

        let mut min_length = schema.clone();
        *schema_mutation_target(
            &mut min_length,
            "/$defs/window/properties/window_id/minLength",
        ) = serde_json::json!(9);
        assert_mutated_schema_rejects(
            &min_length,
            &ingest,
            "/stages/0/stage_execution/capture_window/window_id",
        );

        let mut enumeration = schema.clone();
        *schema_mutation_target(&mut enumeration, "/properties/tier/enum") =
            serde_json::json!(["broken"]);
        assert_mutated_schema_rejects(&enumeration, &ingest, "/tier");

        let mut constant = schema.clone();
        *schema_mutation_target(&mut constant, "/properties/schema_version/const") =
            serde_json::json!("broken");
        assert_mutated_schema_rejects(&constant, &ingest, "/schema_version");

        let mut pattern = schema.clone();
        *schema_mutation_target(&mut pattern, "/properties/dataset_digest/pattern") =
            serde_json::json!("unsupported");
        assert_mutated_schema_rejects(&pattern, &ingest, "/dataset_digest");

        let mut minimum = schema.clone();
        *schema_mutation_target(&mut minimum, "/$defs/stage/properties/ordinal/minimum") =
            serde_json::json!(1);
        assert_mutated_schema_rejects(&minimum, &ingest, "/stages/0/ordinal");

        let mut one_of = schema.clone();
        *schema_mutation_target(&mut one_of, "/properties/body/oneOf") = serde_json::json!([]);
        assert_mutated_schema_rejects(&one_of, &ingest, "/body");

        let mut all_of = schema.clone();
        *schema_mutation_target(&mut all_of, "/$defs/ingest_body/allOf") =
            serde_json::json!([false]);
        assert_mutated_schema_rejects(&all_of, &ingest, "/body");

        let mut conditional = schema.clone();
        *schema_mutation_target(
            &mut conditional,
            "/$defs/ingest_body/allOf/0/if/properties/saturation_reached/const",
        ) = serde_json::json!(false);
        audit_schema_keywords(&conditional, "#").expect("mutated fixture remains supported");
        let conditional_result = evaluate_schema(
            &conditional,
            &conditional["$defs"]["ingest_body"],
            &ingest["body"],
            "/body",
            &mut Vec::new(),
        );
        assert!(
            conditional_result
                .as_ref()
                .is_err_and(|diagnostic| diagnostic.contains("/body/recommended")),
            "expected conditional rejection at /body/recommended, got {conditional_result:?}"
        );

        let mut negated = schema;
        *schema_mutation_target(&mut negated, "/$defs/ingest_body/allOf/0/else/not") =
            serde_json::Value::Bool(true);
        assert_mutated_schema_rejects(&negated, &ingest, "/body");
    }

    /// Proves the checked-in report schema is closed and requires tier plus execution evidence.
    #[test]
    fn report_fixture_requires_tier_and_stage_execution() {
        let schema: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../fixtures/bifrost/qualification/capacity-report-schema.json"
        ))
        .expect("schema fixture");
        let required = schema["required"].as_array().expect("required fields");
        assert!(required.iter().any(|field| field == "tier"));
        let stage_required = schema["$defs"]["stage"]["required"]
            .as_array()
            .expect("stage required fields");
        assert!(
            stage_required
                .iter()
                .any(|field| field == "stage_execution")
        );
        assert_eq!(
            schema["properties"]["body"]["oneOf"]
                .as_array()
                .map(Vec::len),
            Some(4)
        );
    }
}
