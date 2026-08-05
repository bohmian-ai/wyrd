//! Versioned full-cluster Bifrost reference benchmark contracts.
//!
//! This module owns report identity and qualification math only. The real
//! Gate-to-Oracle workload remains in `wyrd-testing` so this crate stays free
//! of server, SQL, storage, and runtime dependencies.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::time::Duration;

use num_traits::ToPrimitive;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Closed schema identifier for full-cluster reference reports.
pub const CLUSTER_REPORT_VERSION: &str = "wyrd.bifrost.cluster-report/v2";
/// Stable workload identifier shared by captures and comparisons.
pub const CLUSTER_WORKLOAD_VERSION: &str = "wyrd.bifrost.cluster-workload/v2";
/// Closed schema identifier for reviewed qualification-rate profiles.
pub const QUALIFICATION_PROFILE_VERSION: &str = "wyrd.bifrost.qualification-profile/v2";
/// Canonical bounded capacity stages. Qualification replays reviewed values
/// from this sequence as absolute rates, never as a percentage of a knee.
pub const CANONICAL_CAPACITY_RATES: [u64; 11] =
    [25, 50, 75, 100, 200, 300, 500, 750, 1_000, 1_500, 2_000];
/// Optional continuation rates used only when every canonical stage is healthy.
pub const OPTIONAL_CAPACITY_RATES: [u64; 3] = [2_500, 3_000, 4_000];
/// Shortened smoke sequence; it makes no SLO or capacity claim.
pub const SMOKE_CAPACITY_RATES: [u64; 4] = [100, 300, 500, 1_000];
/// Maximum wall-clock duration for a six-scenario matrix, including lifecycle.
pub const MATRIX_DURATION_CAP: Duration = Duration::from_mins(40);
/// Production authentication-cache TTL compared by every v2 report.
pub const AUTH_CACHE_TTL_SECONDS: u64 = 5;
/// Production listener-backed revocation behavior compared by every v2 report.
pub const AUTH_INVALIDATION_MODE: &str = "listener-backed";
/// Production writer lifecycle compared by every v2 report.
pub const FLUSH_POLICY: &str = "staggered-production-writer-v1";
/// Public Gate routing contract compared by every v2 report.
pub const ROUTING_POLICY: &str = "public-gate-live-route-v1";
/// Public Arrow batching contract compared by every v2 report.
pub const BATCHING_POLICY: &str = "arrow-64-rows-max-64-kib-v1";
/// Deterministic phase-table allocation contract compared by every v2 report.
pub const ALLOCATION_POLICY: &str = "per-phase-preprovisioned-v1";
/// Exact capacity allocation including warmup, base, optional, confirmation,
/// and recovery slots.
pub const CAPACITY_TABLES_PER_TENANT: u32 = 17;
/// Exact capacity conditioning duration before each measured stage.
pub const CAPACITY_CONDITIONING_SECONDS: u32 = 3;
/// Exact capacity initial warmup duration.
pub const CAPACITY_WARMUP_SECONDS: u32 = 10;
/// Exact capacity measured-stage duration.
pub const CAPACITY_MEASURED_SECONDS: u32 = 20;

/// Return the canonical bounded rate sequence in execution order.
#[must_use]
pub fn capacity_rate_sequence(include_optional: bool) -> Vec<u64> {
    let mut rates = CANONICAL_CAPACITY_RATES.to_vec();
    if include_optional {
        rates.extend(OPTIONAL_CAPACITY_RATES);
    }
    rates
}

/// Classify whether a stage failure requires one confirmation stage.
#[must_use]
pub fn requires_failure_confirmation(consecutive_failures: u8) -> bool {
    consecutive_failures == 1
}

/// Build a deterministic capacity plan from stage outcomes.
///
/// The base sequence is always attempted. Optional rates are appended only
/// when all base stages pass; the first failure receives one confirming stage,
/// and the highest healthy rate is replayed as a recovery stage.
#[must_use]
pub fn capacity_stage_rates(outcomes: &[bool]) -> Vec<u64> {
    let base = CANONICAL_CAPACITY_RATES;
    let mut rates = base.to_vec();
    let first_failure = outcomes.iter().position(|passed| !passed);
    if first_failure.is_none() {
        rates.extend(OPTIONAL_CAPACITY_RATES);
        return rates;
    }
    let failure = first_failure.unwrap_or(0);
    rates.truncate((failure + 1).min(rates.len()));
    rates.push(base[failure.min(base.len() - 1)]);
    if let Some(healthy) = outcomes[..failure].iter().rposition(|passed| *passed) {
        rates.push(base[healthy]);
    }
    rates
}

/// Execution role assigned to a deterministic capacity stage slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapacityStageKind {
    /// One canonical or optional discovery rate.
    Discovery,
    /// A repeat of the first failed discovery rate.
    Confirmation,
    /// A fresh-identity replay of the highest healthy rate.
    Recovery,
}

/// Evidence-aware result consumed by the capacity transition owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapacityStageOutcome {
    /// Every qualification gate passed with zero backpressure.
    Passing,
    /// The only failing condition was exact controlled service backpressure.
    ControlledBackpressure,
    /// Correctness, evidence, latency, fairness, resource, audit, or cleanup failed.
    Invalid,
}

/// Next stage selected by the capacity state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapacityStagePlan {
    /// Stable zero-based measured-table slot.
    pub slot: u16,
    /// Absolute offered request rate.
    pub offered_requests_per_second: u64,
    /// Role of this stage in saturation classification.
    pub kind: CapacityStageKind,
}

/// Benchmark-only capacity lifecycle that preserves every discovery,
/// confirmation, and recovery transition without provisioning decisions.
#[derive(Debug)]
pub struct CapacityStateMachine {
    /// Ordered absolute discovery rates; confirmation and recovery reuse entries.
    rates: Vec<u64>,
    /// Index of the next undispatched discovery rate.
    next_rate: usize,
    /// Monotonic measured-stage slot used for deterministic table allocation.
    slot: u16,
    /// First failed discovery rate awaiting or consumed by confirmation.
    first_failed_rate: Option<u64>,
    /// Greatest passing discovery rate eligible for the terminal recovery replay.
    highest_healthy_rate: Option<u64>,
    /// Single-use confirmation transition state for the first controlled boundary.
    confirmation: ConfirmationState,
    /// Whether the next plan must replay the highest healthy discovery rate.
    recovery_pending: bool,
    /// Whether the bounded curve has emitted its final permitted stage.
    finished: bool,
}

/// Single-use confirmation lifecycle for the bounded capacity curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfirmationState {
    /// No failure has consumed the one confirmation.
    Available,
    /// The next stage repeats the first failed rate.
    Pending,
    /// The one permitted confirmation has completed.
    Used,
}

impl CapacityStateMachine {
    /// Create the canonical bounded v2 capacity state machine.
    #[must_use]
    pub fn canonical() -> Self {
        let mut rates = CANONICAL_CAPACITY_RATES.to_vec();
        rates.extend(OPTIONAL_CAPACITY_RATES);
        Self::for_rates(rates)
    }

    /// Create a bounded diagnostic curve over caller-selected absolute rates.
    #[must_use]
    pub fn for_rates(rates: Vec<u64>) -> Self {
        Self {
            rates,
            next_rate: 0,
            slot: 0,
            first_failed_rate: None,
            highest_healthy_rate: None,
            confirmation: ConfirmationState::Available,
            recovery_pending: false,
            finished: false,
        }
    }

    /// Return the next deterministic stage plan, if the curve is complete.
    #[must_use]
    pub fn next_plan(&mut self) -> Option<CapacityStagePlan> {
        if self.finished || usize::from(self.slot) >= self.rates.len().saturating_add(2) {
            return None;
        }
        let (rate, kind) = if self.confirmation == ConfirmationState::Pending {
            self.confirmation = ConfirmationState::Used;
            (self.first_failed_rate?, CapacityStageKind::Confirmation)
        } else if self.recovery_pending {
            self.recovery_pending = false;
            self.finished = true;
            (self.highest_healthy_rate?, CapacityStageKind::Recovery)
        } else {
            let rate = *self.rates.get(self.next_rate)?;
            self.next_rate += 1;
            (rate, CapacityStageKind::Discovery)
        };
        let plan = CapacityStagePlan {
            slot: self.slot,
            offered_requests_per_second: rate,
            kind,
        };
        self.slot += 1;
        Some(plan)
    }

    /// Record the completed stage and advance confirmation/recovery state.
    ///
    /// # Errors
    /// Returns [`ClusterBenchmarkError::Invalid`] when a recovery stage fails
    /// or a result is recorded after the state machine has completed.
    pub fn record(
        &mut self,
        plan: CapacityStagePlan,
        outcome: CapacityStageOutcome,
    ) -> Result<(), ClusterBenchmarkError> {
        if self.finished && plan.kind != CapacityStageKind::Recovery {
            return Err(ClusterBenchmarkError::Invalid(
                "capacity result arrived after terminal recovery".to_owned(),
            ));
        }
        if outcome == CapacityStageOutcome::Invalid {
            return Err(ClusterBenchmarkError::NotReady(
                "capacity stage failed a non-saturation qualification gate".to_owned(),
            ));
        }
        let passed = outcome == CapacityStageOutcome::Passing;
        match plan.kind {
            CapacityStageKind::Discovery if passed => {
                self.highest_healthy_rate = Some(plan.offered_requests_per_second);
                if self.next_rate == self.rates.len() {
                    self.recovery_pending = true;
                }
            }
            CapacityStageKind::Discovery => {
                self.first_failed_rate = Some(plan.offered_requests_per_second);
                if self.confirmation == ConfirmationState::Used {
                    self.recovery_pending = self.highest_healthy_rate.is_some();
                    self.finished = !self.recovery_pending;
                } else {
                    self.confirmation = ConfirmationState::Pending;
                }
            }
            CapacityStageKind::Confirmation if passed => {
                self.first_failed_rate = None;
                if self.next_rate == self.rates.len() {
                    self.recovery_pending = self.highest_healthy_rate.is_some();
                    self.finished = !self.recovery_pending;
                }
            }
            CapacityStageKind::Confirmation => {
                self.recovery_pending = self.highest_healthy_rate.is_some();
                self.finished = !self.recovery_pending;
            }
            CapacityStageKind::Recovery if passed => {}
            CapacityStageKind::Recovery => {
                return Err(ClusterBenchmarkError::NotReady(
                    "highest healthy rate did not recover after saturation".to_owned(),
                ));
            }
        }
        Ok(())
    }
}
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

/// Closed runtime role identity used for bounded resource attribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BifrostRuntimeRole {
    /// Public request admission and routing.
    Gate,
    /// WAL append and acknowledgement owner.
    Scribe,
    /// Compaction and snapshot publication owner.
    Forge,
    /// Strict query and admission owner.
    Oracle,
}

/// Closed benchmark operation identity used by trace manifests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkOperation {
    /// Durable public write.
    DurableWrite,
    /// Flush acknowledgement through strict visibility.
    FlushToVisible,
    /// Bounded strict query first frame.
    QueryTimeToFirstFrame,
    /// Bounded strict query terminal completion.
    QueryTotal,
}

/// Closed saturation reason retained on failed and censored stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapacityLimit {
    /// The service rejected work at its admission boundary.
    Backpressure,
    /// The benchmark driver exhausted its declared in-flight budget.
    InFlightCap,
    /// An open-loop dispatch missed its planned deadline.
    MissedDeadline,
    /// A production dependency exceeded its declared SLO.
    DependencySlo,
    /// Required production evidence was absent, malformed, or internally inconsistent.
    InvalidEvidence,
    /// Exact data, tenant, or audit reconciliation failed.
    Correctness,
    /// The stage cleanup or resource lifecycle did not converge.
    Cleanup,
}

/// Stable node identity retained for per-node evidence.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessId(
    /// Stable process/node value from the benchmark topology.
    pub String,
);

/// HDR-derived distribution for a critical-path span family.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpanDistribution {
    /// Span operation identity.
    pub operation: BenchmarkOperation,
    /// Number of spans represented by the distribution.
    pub samples: u64,
    /// p95 duration in microseconds.
    pub p95_us: u64,
    /// p99 duration in microseconds.
    pub p99_us: u64,
}

/// Bounded critical-path trace manifest for one operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceManifest {
    /// Operation represented by the selected traces.
    pub operation: BenchmarkOperation,
    /// Representative trace IDs; tenant/table values stay in trace baggage.
    pub representative_trace_ids: Vec<String>,
    /// Critical-path span distributions for attribution.
    pub critical_path_spans: Vec<SpanDistribution>,
}

/// Bounded pillar telemetry delta captured at stage boundaries.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PillarTelemetryDelta {
    /// Gate accepted public operations.
    pub gate_accepted: u64,
    /// Scribe WAL append and fsync bytes.
    pub scribe_wal_bytes: u64,
    /// Forge materialized rows.
    pub forge_publications: u64,
    /// Oracle decoded rows.
    pub oracle_decoded_rows: u64,
    /// Peak analytical slot units held concurrently by Oracle leaders.
    pub oracle_analytical_slots_peak: u64,
    /// Final configured local Oracle slot capacity exposed by the process.
    pub oracle_slots_total: u64,
    /// Queries classified interactive by the estimated-scan rule.
    pub oracle_interactive_scan_classifications: u64,
    /// Queries classified analytical by the predicted-scan rule.
    pub oracle_predicted_scan_classifications: u64,
    /// Queries classified analytical because an unbounded global operator remains.
    pub oracle_global_operator_classifications: u64,
    /// Interactive admission rejections from the local pending waiter bound.
    pub oracle_pending_limit_rejections: u64,
    /// Admission attempts that exhausted the bounded durable lease-acquisition window.
    pub oracle_lease_timeout_rejections: u64,
    /// Interactive admission rejections from the durable cluster ceiling.
    pub oracle_cluster_lease_rejections: u64,
    /// Interactive admission rejections from the durable class ceiling.
    pub oracle_class_lease_rejections: u64,
    /// Interactive admission rejections from the durable tenant ceiling.
    pub oracle_tenant_lease_rejections: u64,
    /// Interactive admission rejections from the selected leader's local slots.
    pub oracle_local_slot_rejections: u64,
    /// Validation status across the exact closed pillar bindings.
    pub status: EvidenceStatus,
    /// Bounded identifiers for required bindings that were absent.
    pub missing_required: Vec<String>,
    /// Bounded identifiers for bindings with invalid kind, labels, unit, or value.
    pub invalid: Vec<String>,
}

/// Dependency evidence used to explain a limiting stage.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyTelemetryEvidence {
    /// Postgres pool wait time in microseconds.
    pub postgres_pool_wait_us: u64,
    /// Postgres transaction count.
    pub postgres_transactions: u64,
    /// Storage bytes and operation failures.
    pub storage_bytes: u64,
    /// Storage operation latency p99 in microseconds.
    pub storage_p99_us: u64,
    /// WAL fsync latency p99 in microseconds.
    pub wal_fsync_p99_us: u64,
    /// Validation status across the exact closed dependency bindings.
    pub status: EvidenceStatus,
    /// Bounded identifiers for required bindings that were absent.
    pub missing_required: Vec<String>,
    /// Bounded identifiers for malformed dependency bindings.
    pub invalid: Vec<String>,
}

/// Workload resource evidence attributed to one node and its roles.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessResourceEvidence {
    /// Stable OS process identity.
    pub process_id: ProcessId,
    /// Explicit process epoch; replacement begins a new epoch instead of a counter reset.
    pub epoch: u64,
    /// Logical node identities hosted in this process.
    pub hosted_logical_nodes: Vec<String>,
    /// Roles hosted by this node.
    pub roles: Vec<BifrostRuntimeRole>,
    /// CPU seconds consumed by the process.
    pub cpu_seconds: f64,
    /// Peak resident set size in bytes.
    pub peak_rss_bytes: u64,
    /// Resident set size at the end of the capture window.
    pub current_rss_bytes: u64,
    /// Runtime busy seconds.
    pub runtime_busy_seconds: f64,
    /// Peak runtime queue depth.
    pub runtime_queue_peak: u64,
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

/// Identity for one measured capacity stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapacityStageIdentity {
    /// Stable scenario-local stage ID.
    pub stage_id: String,
    /// Strictly increasing stage ordinal.
    pub ordinal: u16,
    /// Non-overlapping tenant row ranges.
    pub tenant_rows: Vec<TenantStageRows>,
}

/// One retained capacity stage, including failed probes and recovery.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapacityStage {
    /// Stage identity and deterministic row ledger.
    pub identity: CapacityStageIdentity,
    /// Discovery, confirmation, or recovery role of this retained stage.
    pub kind: CapacityStageKind,
    /// Absolute offered rate.
    pub offered_requests_per_second: u64,
    /// Completed public operations per second.
    pub completed_requests_per_second: f64,
    /// Measured stage duration.
    pub duration: Duration,
    /// Whether the stage met its readiness predicates.
    pub passed: bool,
    /// Number of in-flight cap exhaustions.
    pub in_flight_cap_exhaustions: u64,
    /// Every stop predicate observed for this stage.
    pub stop_reasons: Vec<CapacityLimit>,
    /// Client measurements and production evidence.
    pub metrics: ClientTrialMetrics,
    /// Pillar telemetry delta.
    pub telemetry: PillarTelemetryDelta,
    /// Per-node resource evidence.
    pub resources: Vec<ProcessResourceEvidence>,
    /// Dependency evidence.
    pub dependencies: DependencyTelemetryEvidence,
    /// Representative traces.
    pub traces: Vec<TraceManifest>,
}

/// Closed semantic role assigned to one reviewed absolute qualification rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualificationRateRole {
    /// Lowest passing canonical rate in the source curve.
    Healthy,
    /// Highest passing canonical rate below the required headroom ceiling.
    TargetOperating,
    /// Highest passing canonical rate with a passing recovery replay.
    NearSaturation,
}

/// One absolute rate selected deterministically from a passing capacity curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewedQualificationRate {
    /// Absolute public requests offered per second.
    pub requests_per_second: u64,
    /// Qualification role derived from the source curve.
    pub role: QualificationRateRole,
}

/// Compact identity for one reviewed passing discovery stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewedStageRef {
    /// Scenario that owns the stage.
    pub scenario_id: String,
    /// Stable scenario-local stage identifier.
    pub stage_id: String,
    /// Strictly increasing scenario-local ordinal.
    pub ordinal: u16,
    /// Absolute offered request rate.
    pub requests_per_second: u64,
    /// Capacity-stage role.
    pub kind: CapacityStageKind,
    /// Explicit reviewed passing status.
    pub passed: bool,
}

/// Digest-bound provenance shared by every scenario entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewedCapacitySource {
    /// Exact schema of the source capacity report.
    pub report_schema_version: String,
    /// Lowercase SHA-256 of the complete source artifact bytes.
    pub artifact_sha256: String,
}

/// One independently derived qualification curve.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewedScenarioQualification {
    /// Stable scenario identity used for exact routing.
    pub scenario_id: String,
    /// Exact capacity workload from which this entry was derived.
    pub source_workload: ClusterWorkloadIdentity,
    /// Exact three-rate workload permitted for qualification.
    pub qualification_workload: ClusterWorkloadIdentity,
    /// One absolute rate for each closed qualification role.
    pub rates: Vec<ReviewedQualificationRate>,
    /// Compact reviewed identities for the selected discovery stages.
    pub selected_stages: Vec<ReviewedStageRef>,
}

/// Human-reviewable per-scenario profile generated from canonical capacity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationProfileV2 {
    /// Closed qualification-profile schema identifier.
    pub schema_version: String,
    /// Exact compatible environment identity.
    pub environment: BenchmarkEnvironment,
    /// Digest-bound source capacity evidence.
    pub source_capacity: ReviewedCapacitySource,
    /// Independently derived scenario qualification entries.
    pub scenarios: Vec<ReviewedScenarioQualification>,
}

impl QualificationProfileV2 {
    /// Generate independent qualification entries for every capacity scenario.
    ///
    /// # Errors
    /// Returns an error when any scenario lacks a qualifying curve.
    pub fn from_capacity_report(
        report: &BifrostReferenceProfile,
        artifact_sha256: String,
    ) -> Result<Self, ClusterBenchmarkError> {
        let scenarios = report
            .scenarios
            .iter()
            .map(Self::scenario_from_capacity)
            .collect::<Result<Vec<_>, _>>()?;
        let candidate = Self {
            schema_version: QUALIFICATION_PROFILE_VERSION.to_owned(),
            environment: report.environment.clone(),
            source_capacity: ReviewedCapacitySource {
                report_schema_version: report.schema_version.clone(),
                artifact_sha256,
            },
            scenarios,
        };
        candidate.validate()?;
        Ok(candidate)
    }

    /// Validate all per-scenario identities, curves, roles, and digest shape.
    ///
    /// # Errors
    /// Returns an error for malformed or cross-scenario data.
    pub fn validate(&self) -> Result<(), ClusterBenchmarkError> {
        if self.schema_version != QUALIFICATION_PROFILE_VERSION
            || self.source_capacity.report_schema_version != CLUSTER_REPORT_VERSION
            || self.source_capacity.artifact_sha256.len() != 64
            || !self
                .source_capacity
                .artifact_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || self.scenarios.is_empty()
        {
            return Err(ClusterBenchmarkError::Incompatible(
                "qualification profile schema, digest, or scenario set is invalid".to_owned(),
            ));
        }
        let mut ids = HashSet::new();
        for scenario in &self.scenarios {
            if !ids.insert(scenario.scenario_id.as_str()) {
                return Err(ClusterBenchmarkError::Incompatible(
                    "qualification profile contains duplicate scenarios".to_owned(),
                ));
            }
            scenario.validate()?;
        }
        Ok(())
    }

    /// Require exact coverage of the six canonical matrix scenarios.
    ///
    /// # Errors
    /// Returns an error unless exactly the canonical six are present.
    pub fn validate_matrix(&self) -> Result<(), ClusterBenchmarkError> {
        self.validate()?;
        let expected = fixed_scenarios()
            .into_iter()
            .map(|(id, _, _, _)| id)
            .collect::<HashSet<_>>();
        let actual = self
            .scenarios
            .iter()
            .map(|scenario| scenario.scenario_id.as_str())
            .collect::<HashSet<_>>();
        if actual != expected || self.scenarios.len() != expected.len() {
            return Err(ClusterBenchmarkError::Unsupported(
                "qualification matrix requires exactly the six canonical scenarios".to_owned(),
            ));
        }
        Ok(())
    }

    /// Resolve one reviewed scenario entry.
    ///
    /// # Errors
    /// Returns an error when the scenario is missing or duplicated.
    pub fn scenario(
        &self,
        scenario_id: &str,
    ) -> Result<&ReviewedScenarioQualification, ClusterBenchmarkError> {
        let matches = self
            .scenarios
            .iter()
            .filter(|scenario| scenario.scenario_id == scenario_id)
            .collect::<Vec<_>>();
        let [scenario] = matches.as_slice() else {
            return Err(ClusterBenchmarkError::Incompatible(format!(
                "qualification profile does not contain exactly one entry for {scenario_id}"
            )));
        };
        Ok(scenario)
    }

    /// Resolve T19's distributed target-operating rate.
    ///
    /// # Errors
    /// Returns an error when the distributed scenario or role is unavailable.
    pub fn distributed_target_operating_rate(&self) -> Result<u64, ClusterBenchmarkError> {
        self.scenario("balanced-three-server-three-worker-eight-tenants")?
            .rate(QualificationRateRole::TargetOperating)
    }

    /// Validate compact stage references against the digest-verified report.
    ///
    /// # Errors
    /// Returns an error for any source or selected-stage mismatch.
    pub fn validate_capacity_source(
        &self,
        report: &BifrostReferenceProfile,
    ) -> Result<(), ClusterBenchmarkError> {
        self.validate()?;
        if report.schema_version != self.source_capacity.report_schema_version
            || report.environment != self.environment
            || report.scenarios.len() != self.scenarios.len()
        {
            return Err(ClusterBenchmarkError::Incompatible(
                "capacity source schema, environment, or scenarios differ".to_owned(),
            ));
        }
        for reviewed in &self.scenarios {
            let matches = report
                .scenarios
                .iter()
                .filter(|scenario| scenario.workload == reviewed.source_workload)
                .collect::<Vec<_>>();
            let [source] = matches.as_slice() else {
                return Err(ClusterBenchmarkError::Incompatible(
                    "capacity source must contain exactly one matching workload".to_owned(),
                ));
            };
            for selected in &reviewed.selected_stages {
                let stages = source
                    .reports
                    .iter()
                    .flat_map(|report| &report.capacity_stages)
                    .filter(|stage| {
                        stage.identity.stage_id == selected.stage_id
                            && stage.identity.ordinal == selected.ordinal
                    })
                    .collect::<Vec<_>>();
                let [stage] = stages.as_slice() else {
                    return Err(ClusterBenchmarkError::Incompatible(
                        "reviewed stage is absent or duplicated".to_owned(),
                    ));
                };
                if selected.scenario_id != reviewed.scenario_id
                    || selected.requests_per_second != stage.offered_requests_per_second
                    || selected.kind != stage.kind
                    || selected.passed != stage.passed
                    || stage.kind != CapacityStageKind::Discovery
                    || !stage.passed
                {
                    return Err(ClusterBenchmarkError::Incompatible(
                        "reviewed stage is not an exact passing discovery stage".to_owned(),
                    ));
                }
                stage.validate_evidence()?;
            }
        }
        Ok(())
    }

    /// Derive one scenario-local three-role entry.
    ///
    /// # Errors
    /// Returns an error when capacity evidence cannot supply the three roles.
    fn scenario_from_capacity(
        scenario: &ReviewedScenarioProfile,
    ) -> Result<ReviewedScenarioQualification, ClusterBenchmarkError> {
        let stages = scenario
            .reports
            .iter()
            .flat_map(|report| &report.capacity_stages)
            .collect::<Vec<_>>();
        let recovery = stages
            .iter()
            .rev()
            .find(|stage| stage.kind == CapacityStageKind::Recovery && stage.passed)
            .map(|stage| stage.offered_requests_per_second)
            .ok_or_else(|| {
                ClusterBenchmarkError::Unsupported(
                    "capacity source lacks passing recovery".to_owned(),
                )
            })?;
        let passing = CANONICAL_CAPACITY_RATES
            .iter()
            .filter_map(|rate| {
                stages
                    .iter()
                    .find(|stage| {
                        stage.kind == CapacityStageKind::Discovery
                            && stage.passed
                            && stage.offered_requests_per_second == *rate
                    })
                    .copied()
            })
            .collect::<Vec<_>>();
        let healthy = passing.first().copied().ok_or_else(|| {
            ClusterBenchmarkError::Unsupported("capacity source lacks passing discovery".to_owned())
        })?;
        let near = passing.last().copied().ok_or_else(|| {
            ClusterBenchmarkError::Unsupported("capacity source lacks passing discovery".to_owned())
        })?;
        if passing.len() < 3 || near.offered_requests_per_second != recovery {
            return Err(ClusterBenchmarkError::Unsupported(
                "capacity curve or recovery is insufficient".to_owned(),
            ));
        }
        let ceiling = near.offered_requests_per_second.saturating_mul(70) / 100;
        let target = passing
            .iter()
            .rev()
            .find(|stage| {
                stage.offered_requests_per_second > healthy.offered_requests_per_second
                    && stage.offered_requests_per_second < near.offered_requests_per_second
                    && stage.offered_requests_per_second <= ceiling
            })
            .copied()
            .ok_or_else(|| {
                ClusterBenchmarkError::Unsupported(
                    "capacity source lacks target headroom".to_owned(),
                )
            })?;
        let selected = [healthy, target, near];
        for stage in selected {
            stage.validate_evidence()?;
        }
        let rates = [
            QualificationRateRole::Healthy,
            QualificationRateRole::TargetOperating,
            QualificationRateRole::NearSaturation,
        ]
        .into_iter()
        .zip(selected)
        .map(|(role, stage)| ReviewedQualificationRate {
            requests_per_second: stage.offered_requests_per_second,
            role,
        })
        .collect::<Vec<_>>();
        let mut qualification_workload = scenario.workload.clone();
        qualification_workload.ordered_rates =
            rates.iter().map(|rate| rate.requests_per_second).collect();
        qualification_workload.trial_count = 3;
        qualification_workload.tables_per_tenant = 18;
        Ok(ReviewedScenarioQualification {
            scenario_id: scenario.workload.scenario_id.clone(),
            source_workload: scenario.workload.clone(),
            qualification_workload,
            rates,
            selected_stages: selected
                .into_iter()
                .map(|stage| ReviewedStageRef {
                    scenario_id: scenario.workload.scenario_id.clone(),
                    stage_id: stage.identity.stage_id.clone(),
                    ordinal: stage.identity.ordinal,
                    requests_per_second: stage.offered_requests_per_second,
                    kind: stage.kind,
                    passed: stage.passed,
                })
                .collect(),
        })
    }
}

impl ReviewedScenarioQualification {
    /// Validate this scenario-local curve and workload relationship.
    ///
    /// # Errors
    /// Returns an error for malformed roles, workloads, or stage references.
    pub fn validate(&self) -> Result<(), ClusterBenchmarkError> {
        let roles = [
            QualificationRateRole::Healthy,
            QualificationRateRole::TargetOperating,
            QualificationRateRole::NearSaturation,
        ];
        let values = self
            .rates
            .iter()
            .map(|rate| rate.requests_per_second)
            .collect::<Vec<_>>();
        let mut expected = self.source_workload.clone();
        expected.ordered_rates.clone_from(&values);
        expected.trial_count = 3;
        expected.tables_per_tenant = 18;
        if self.scenario_id != self.source_workload.scenario_id
            || self.scenario_id != self.qualification_workload.scenario_id
            || self.rates.len() != 3
            || self.selected_stages.len() != 3
            || self.rates.iter().map(|rate| rate.role).ne(roles)
            || values.windows(2).any(|window| window[0] >= window[1])
            || self.qualification_workload != expected
        {
            return Err(ClusterBenchmarkError::Incompatible(
                "scenario qualification is invalid".to_owned(),
            ));
        }
        self.qualification_workload.validate()?;
        for (rate, stage) in self.rates.iter().zip(&self.selected_stages) {
            if stage.scenario_id != self.scenario_id
                || stage.kind != CapacityStageKind::Discovery
                || !stage.passed
                || stage.requests_per_second != rate.requests_per_second
            {
                return Err(ClusterBenchmarkError::Incompatible(
                    "qualification rate has a cross-scenario or invalid stage".to_owned(),
                ));
            }
        }
        Ok(())
    }

    /// Resolve one scenario-local role.
    ///
    /// # Errors
    /// Returns an error when the role is missing or duplicated.
    pub fn rate(&self, role: QualificationRateRole) -> Result<u64, ClusterBenchmarkError> {
        let matches = self
            .rates
            .iter()
            .filter(|rate| rate.role == role)
            .collect::<Vec<_>>();
        let [rate] = matches.as_slice() else {
            return Err(ClusterBenchmarkError::Incompatible(
                "qualification role is absent or duplicated".to_owned(),
            ));
        };
        Ok(rate.requests_per_second)
    }
}

impl CapacityStage {
    /// Validate that a retained stage can explain readiness or saturation.
    ///
    /// # Errors
    /// Returns [`ClusterBenchmarkError::NotReady`] when required pillar,
    /// dependency, node/resource, or four-operation trace evidence is absent.
    pub fn validate_evidence(&self) -> Result<(), ClusterBenchmarkError> {
        let operations = self
            .traces
            .iter()
            .map(|trace| trace.operation)
            .collect::<HashSet<_>>();
        if self.telemetry.status != EvidenceStatus::Complete
            || self.dependencies.status != EvidenceStatus::Complete
            || !self.telemetry.missing_required.is_empty()
            || !self.telemetry.invalid.is_empty()
            || !self.dependencies.missing_required.is_empty()
            || !self.dependencies.invalid.is_empty()
            || self.resources.is_empty()
            || self
                .resources
                .iter()
                .any(|resource| resource.roles.is_empty())
            || operations.len() != 4
            || self.traces.iter().any(|trace| {
                trace.representative_trace_ids.is_empty()
                    || trace.critical_path_spans.is_empty()
                    || trace
                        .critical_path_spans
                        .iter()
                        .any(|span| span.samples == 0)
            })
        {
            return Err(ClusterBenchmarkError::NotReady(
                "capacity stage lacks complete bounded telemetry, resource, dependency, or trace evidence"
                    .to_owned(),
            ));
        }
        Ok(())
    }
}

/// A qualification workload identity and its reviewed absolute rates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClusterWorkloadIdentity {
    /// Stable scenario identity.
    pub scenario_id: String,
    /// Topology, traffic, and protocol shape identity.
    pub topology: ClusterTopology,
    /// Tenant cardinality.
    pub tenants: u32,
    /// Traffic mix.
    pub traffic: TrafficMix,
    /// Rows per write and bounded query limit.
    pub rows_per_batch: u32,
    /// Query row limit.
    pub query_row_limit: u32,
    /// Exact production authentication-cache TTL.
    pub auth_cache_ttl_seconds: u64,
    /// Exact production revocation invalidation mechanism.
    pub auth_invalidation_mode: String,
    /// Exact production writer flush lifecycle.
    pub flush_policy: String,
    /// Exact public Gate routing policy.
    pub routing_policy: String,
    /// Exact public Arrow frame batching policy.
    pub batching_policy: String,
    /// Runtime-inspected storage backend identity.
    pub storage_runtime_identity: String,
    /// Deterministic phase-table allocation policy.
    pub allocation_policy: String,
    /// Exact number of boot-provisioned tables for each tenant.
    pub tables_per_tenant: u32,
    /// Closed absolute offered-rate order.
    pub ordered_rates: Vec<u64>,
    /// Exact independent trial count.
    pub trial_count: u8,
    /// Exact warmup duration before a qualification measurement.
    pub warmup_seconds: u32,
    /// Exact conditioning duration before a measured stage or trial.
    pub conditioning_seconds: u32,
    /// Exact measured duration.
    pub measured_seconds: u32,
}

impl ClusterWorkloadIdentity {
    /// Validate the closed v2 production-faithfulness identity.
    ///
    /// # Errors
    /// Returns [`ClusterBenchmarkError::Incompatible`] when any production
    /// policy, allocation count, ordered rate, or duration is absent or does
    /// not match the report shape.
    pub fn validate(&self) -> Result<(), ClusterBenchmarkError> {
        let expected_tables = self
            .ordered_rates
            .len()
            .checked_mul(usize::from(self.trial_count))
            .and_then(|pairs| pairs.checked_mul(2))
            .and_then(|tables| u32::try_from(tables).ok())
            .ok_or_else(|| {
                ClusterBenchmarkError::Incompatible(
                    "qualification allocation count overflowed".to_owned(),
                )
            })?;
        if self.auth_cache_ttl_seconds != AUTH_CACHE_TTL_SECONDS
            || self.auth_invalidation_mode != AUTH_INVALIDATION_MODE
            || self.flush_policy != FLUSH_POLICY
            || self.routing_policy != ROUTING_POLICY
            || self.batching_policy != BATCHING_POLICY
            || self.storage_runtime_identity.trim().is_empty()
            || self.allocation_policy != ALLOCATION_POLICY
            || self.tables_per_tenant != expected_tables
            || self.ordered_rates.is_empty()
            || self
                .ordered_rates
                .windows(2)
                .any(|rates| rates[0] >= rates[1])
            || self.trial_count != 3
            || self.warmup_seconds != 10
            || self.conditioning_seconds != 3
            || self.measured_seconds != 20
        {
            return Err(ClusterBenchmarkError::Incompatible(
                "workload identity does not match the closed v2 production profile".to_owned(),
            ));
        }
        Ok(())
    }
}

/// One reviewed qualification scenario and its exact reports by rate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewedScenarioProfile {
    /// Stable fixed scenario identifier.
    pub scenario_id: String,
    /// Shape identity excluding run-specific telemetry.
    pub workload: ClusterWorkloadIdentity,
    /// Strictly increasing reviewed absolute rates.
    pub offered_rates: Vec<u64>,
    /// Exactly one report per offered rate.
    pub reports: Vec<ClusterScenarioReport>,
}

impl ReviewedScenarioProfile {
    /// Validate exact rate ordering and one report per reviewed absolute rate.
    ///
    /// # Errors
    /// Returns an incompatible error when rates are empty, duplicated,
    /// reordered, or do not match the report identities.
    pub fn validate(&self) -> Result<(), ClusterBenchmarkError> {
        self.workload.validate()?;
        if self.offered_rates.is_empty()
            || self
                .offered_rates
                .windows(2)
                .any(|window| window[0] >= window[1])
            || self.reports.len() != self.offered_rates.len()
        {
            return Err(ClusterBenchmarkError::Incompatible(
                "reviewed qualification rates must be strictly increasing and complete".to_owned(),
            ));
        }
        if self.workload.scenario_id != self.scenario_id {
            return Err(ClusterBenchmarkError::Invalid(
                "reviewed workload identity does not match its scenario".to_owned(),
            ));
        }
        if self.workload.ordered_rates != self.offered_rates {
            return Err(ClusterBenchmarkError::Incompatible(
                "workload ordered rates differ from reviewed rates".to_owned(),
            ));
        }
        for (rate, report) in self.offered_rates.iter().zip(&self.reports) {
            if *rate != report.scenario.offered_requests_per_second
                || report.scenario.scenario_id != self.scenario_id
            {
                return Err(ClusterBenchmarkError::Incompatible(
                    "candidate must replay every reviewed absolute rate exactly".to_owned(),
                ));
            }
            if report.scenario.topology != self.workload.topology
                || report.scenario.tenants != self.workload.tenants
                || report.scenario.traffic != self.workload.traffic
                || report.scenario.rows_per_batch != self.workload.rows_per_batch
                || report.scenario.query_row_limit != self.workload.query_row_limit
            {
                return Err(ClusterBenchmarkError::Invalid(
                    "reviewed workload identity does not match its reports".to_owned(),
                ));
            }
        }
        Ok(())
    }
}

/// Per-trial report with complete client, role, dependency, and trace evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClusterTrialReport {
    /// Absolute offered rate and trial ordinal.
    pub offered_requests_per_second: u64,
    /// One-based trial number.
    pub trial_index: u8,
    /// Client-derived metrics.
    pub metrics: ClientTrialMetrics,
    /// Pillar telemetry delta.
    pub telemetry: PillarTelemetryDelta,
    /// Per-node resource evidence.
    pub resources: Vec<ProcessResourceEvidence>,
    /// Dependency evidence.
    pub dependencies: DependencyTelemetryEvidence,
    /// Trace manifest.
    pub traces: Vec<TraceManifest>,
}

impl ClusterTrialReport {
    /// Validate complete bounded evidence for one qualification trial.
    ///
    /// # Errors
    /// Returns [`ClusterBenchmarkError::NotReady`] when production telemetry,
    /// dependencies, node resources, or any critical public-operation trace is
    /// absent or contains an empty distribution.
    pub fn validate_evidence(&self) -> Result<(), ClusterBenchmarkError> {
        let operations = self
            .traces
            .iter()
            .map(|trace| trace.operation)
            .collect::<HashSet<_>>();
        if self.telemetry.status != EvidenceStatus::Complete
            || self.dependencies.status != EvidenceStatus::Complete
            || !self.telemetry.missing_required.is_empty()
            || !self.telemetry.invalid.is_empty()
            || !self.dependencies.missing_required.is_empty()
            || !self.dependencies.invalid.is_empty()
            || self.resources.is_empty()
            || self
                .resources
                .iter()
                .any(|resource| resource.roles.is_empty())
            || operations.len() != 4
            || self.traces.iter().any(|trace| {
                trace.representative_trace_ids.is_empty()
                    || trace.critical_path_spans.is_empty()
                    || trace
                        .critical_path_spans
                        .iter()
                        .any(|span| span.samples == 0)
            })
        {
            return Err(ClusterBenchmarkError::NotReady(
                "qualification trial lacks complete bounded telemetry, resource, dependency, or trace evidence"
                    .to_owned(),
            ));
        }
        Ok(())
    }
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
    /// Runtime storage backend kind (must be local-filesystem for qualifying runs).
    #[serde(default)]
    pub storage_backend_kind: String,
    /// Dedicated benchmark storage root, scrubbed to a repository-relative path.
    #[serde(default)]
    pub storage_root: String,
    /// Filesystem/device class observed for the storage root.
    #[serde(default)]
    pub storage_device_class: String,
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
            self.storage_backend_kind.as_str(),
            self.storage_root.as_str(),
            self.storage_device_class.as_str(),
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
            || self.storage_image.contains("memory")
            || self.storage_backend_kind.contains("memory")
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
            && self.storage_backend_kind == candidate.storage_backend_kind
            && self.storage_root == candidate.storage_root
            && self.storage_device_class == candidate.storage_device_class
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
    /// Three independent trials.
    pub trials: Vec<ClusterBenchmarkTrial>,
    /// Complete bounded evidence corresponding one-for-one with `trials`.
    #[serde(default)]
    pub trial_evidence: Vec<ClusterTrialReport>,
    /// Median of the three independent trial summaries.
    pub median: MedianMetrics,
    /// Capacity stages retained for bounded capacity mode. Qualification
    /// reports may leave this empty; omission never makes a report promotable.
    #[serde(default)]
    pub capacity_stages: Vec<CapacityStage>,
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
        if self.capacity_stages.is_empty() {
            if self.trial_evidence.len() != self.trials.len() {
                return Err(ClusterBenchmarkError::Unsupported(
                    "qualification report lacks one complete evidence record per trial".to_owned(),
                ));
            }
            for evidence in &self.trial_evidence {
                evidence.validate_evidence()?;
            }
        }
        let needs_write = true;
        let needs_read = true;
        for trial in &self.trials {
            if trial.client.attempted_operations > trial.client.planned_operations
                || trial.client.accepted_operations > trial.client.attempted_operations
                || trial.client.in_flight_cap_exhaustions > trial.client.planned_operations
            {
                return Err(ClusterBenchmarkError::NotReady(
                    "open-loop operation ledger is not monotonic".to_owned(),
                ));
            }
            if (needs_write && trial.client.durable_write.samples < self.scenario.minimum_samples)
                || (needs_read
                    && (trial.client.query_time_to_first_frame.samples
                        < self.scenario.minimum_samples
                        || trial.client.total_query.samples < self.scenario.minimum_samples))
            {
                return Err(ClusterBenchmarkError::Unsupported(format!(
                    "scenario={} offered_rps={} trial={} has insufficient samples: required={}, planned={}, attempted={}, accepted={}, writes={}, query_ttfb={}, query_total={}, retries={}, backpressure={}, missed={}",
                    self.scenario.scenario_id,
                    self.scenario.offered_requests_per_second,
                    trial.trial,
                    self.scenario.minimum_samples,
                    trial.client.planned_operations,
                    trial.client.attempted_operations,
                    trial.client.accepted_operations,
                    trial.client.durable_write.samples,
                    trial.client.query_time_to_first_frame.samples,
                    trial.client.total_query.samples,
                    trial.client.retry_operations,
                    trial.client.backpressure_operations,
                    trial.client.missed_operations,
                )));
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
    /// Six reviewed scenarios, each containing its exact absolute-rate reports.
    pub scenarios: Vec<ReviewedScenarioProfile>,
    /// Absolute reference SLOs.
    pub slos: BifrostSloEnvelope,
}

/// Closed terminal status for a non-promotable v2 diagnostic capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticStatus {
    /// Required host, runtime, or dependency identity was unavailable.
    Unsupported,
    /// A probe, stage, correctness gate, or cleanup predicate failed.
    NotReady,
}

/// One attempted capacity or calibration probe retained on failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptedProbe {
    /// Stable scenario identifier, when startup reached scenario selection.
    pub scenario_id: Option<String>,
    /// Absolute rate offered by the attempted probe.
    pub offered_requests_per_second: u64,
    /// Whether the probe completed its measurement window.
    pub completed: bool,
    /// Every simultaneously observed stop reason.
    pub stop_reasons: Vec<CapacityLimit>,
}

/// Strict v2 diagnostic emitted by every unsuccessful benchmark invocation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BifrostDiagnosticReport {
    /// Exact cluster report schema version; diagnostics never invent a side schema.
    pub schema_version: String,
    /// Diagnostics are never eligible for baseline promotion.
    pub promotable: bool,
    /// Typed terminal readiness state.
    pub status: DiagnosticStatus,
    /// Complete terminal error text.
    pub error: String,
    /// Environment identity when discovery completed.
    pub environment: Option<BenchmarkEnvironment>,
    /// Every attempted probe in execution order, including the first failure.
    pub attempted_probes: Vec<AttemptedProbe>,
    /// Complete scenario reports produced before failure.
    pub scenarios: Vec<ReviewedScenarioProfile>,
}

impl BifrostDiagnosticReport {
    /// Build a strict, explicitly non-promotable v2 diagnostic.
    #[must_use]
    pub fn failure(
        status: DiagnosticStatus,
        error: String,
        environment: Option<BenchmarkEnvironment>,
        attempted_probes: Vec<AttemptedProbe>,
        scenarios: Vec<ReviewedScenarioProfile>,
    ) -> Self {
        Self {
            schema_version: CLUSTER_REPORT_VERSION.to_owned(),
            promotable: false,
            status,
            error,
            environment,
            attempted_probes,
            scenarios,
        }
    }
}

impl BifrostReferenceProfile {
    /// Return the reviewed scenario groups stored in this profile.
    #[must_use]
    pub fn reviewed_scenarios(&self) -> &[ReviewedScenarioProfile] {
        &self.scenarios
    }

    /// Iterate over every nested rate report without changing the persisted
    /// reviewed-scenario shape.
    pub fn reports(&self) -> impl Iterator<Item = &ClusterScenarioReport> {
        self.scenarios
            .iter()
            .flat_map(|scenario| scenario.reports.iter())
    }

    /// Validate one selected qualification scenario without requiring the
    /// six-scenario promotion matrix.
    ///
    /// # Errors
    /// Returns an incompatible or unsupported error when the selected report
    /// is absent, malformed, dirty, or lacks complete trial evidence.
    pub fn validate_selected(&self, scenario_id: &str) -> Result<(), ClusterBenchmarkError> {
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
        let selected = self
            .scenarios
            .iter()
            .find(|scenario| scenario.scenario_id == scenario_id)
            .ok_or_else(|| {
                ClusterBenchmarkError::Invalid(format!(
                    "selected scenario `{scenario_id}` has no reports"
                ))
            })?;
        selected.validate()?;
        if selected.reports.is_empty() {
            return Err(ClusterBenchmarkError::Invalid(format!(
                "selected scenario `{scenario_id}` has no reports"
            )));
        }
        for report in &selected.reports {
            report.validate()?;
            validate_fixed_scenario(report)?;
        }
        Ok(())
    }

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
        if self.environment.storage_backend_kind != "local-filesystem"
            || self.environment.storage_root.is_empty()
            || self.environment.storage_device_class.is_empty()
        {
            return Err(ClusterBenchmarkError::Unsupported(
                "qualification requires a declared local-filesystem storage identity".to_owned(),
            ));
        }
        if self.environment.dirty_worktree {
            return Err(ClusterBenchmarkError::Unsupported(
                "dirty worktree captures cannot be blessed as a reference".to_owned(),
            ));
        }
        if self.scenarios.len() != 6 {
            return Err(ClusterBenchmarkError::Unsupported(
                "reference requires six scenarios at three absolute rates".to_owned(),
            ));
        }
        validate_report_matrix(self)?;
        Ok(())
    }
}

/// Validate every reviewed scenario and its six-scenario matrix identity.
fn validate_report_matrix(profile: &BifrostReferenceProfile) -> Result<(), ClusterBenchmarkError> {
    let mut identities = BTreeSet::new();
    let mut scenario_ids = BTreeSet::new();
    for reviewed in &profile.scenarios {
        if !scenario_ids.insert(reviewed.scenario_id.as_str()) {
            return Err(ClusterBenchmarkError::Invalid(format!(
                "duplicate reviewed scenario identity {}",
                reviewed.scenario_id
            )));
        }
        reviewed.validate()?;
        for report in &reviewed.reports {
            report.validate()?;
            validate_fixed_scenario(report)?;
            let key = (
                report.scenario.scenario_id.as_str(),
                report.scenario.offered_requests_per_second,
            );
            if !identities.insert(key) {
                return Err(ClusterBenchmarkError::Invalid(format!(
                    "duplicate scenario/rate identity {}:{}",
                    key.0, key.1
                )));
            }
            let median = &report.median;
            if median.durable_write_p99_us > profile.slos.durable_write_p99_us {
                return Err(ClusterBenchmarkError::NotReady(
                    "durable-write p99 exceeds the absolute reference SLO".to_owned(),
                ));
            }
            if median.flush_to_visible_p99_us > profile.slos.flush_to_visible_p99_us {
                return Err(ClusterBenchmarkError::NotReady(
                    "flush-to-visible p99 exceeds the absolute reference SLO".to_owned(),
                ));
            }
            if median.query_ttfb_p99_us > profile.slos.query_ttfb_p99_us {
                return Err(ClusterBenchmarkError::NotReady(
                    "query TTFB p99 exceeds the absolute reference SLO".to_owned(),
                ));
            }
        }
    }
    for (id, topology, tenants, traffic) in fixed_scenarios() {
        let reports = profile
            .scenarios
            .iter()
            .filter(|reviewed| reviewed.scenario_id == id)
            .flat_map(|reviewed| reviewed.reports.iter())
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
        let mut rates = reports
            .iter()
            .map(|report| report.scenario.offered_requests_per_second)
            .collect::<Vec<_>>();
        rates.sort_unstable();
        rates.dedup();
        if rates.len() != 3 {
            return Err(ClusterBenchmarkError::Invalid(format!(
                "scenario `{id}` must contain three distinct absolute offered rates"
            )));
        }
    }
    Ok(())
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

/// Return the ten immutable flush offsets in a measured 20-second trial.
#[must_use]
pub fn measured_flush_offsets_seconds() -> [u64; 10] {
    [2, 4, 6, 8, 10, 12, 14, 16, 18, 20]
}

/// Compute the exact qualification invocation bound before cluster startup.
///
/// The caller supplies the reviewed rate count, three-trial measurement
/// duration, and the declared aggregate lifecycle overhead. Each rate/trial
/// pair owns ten seconds of warmup and three seconds of conditioning.
///
/// # Errors
/// Returns [`ClusterBenchmarkError::Unsupported`] when arithmetic overflows or
/// the declared invocation cap cannot contain the complete formula.
pub fn qualification_preflight_seconds(
    reviewed_rate_count: usize,
    measured_seconds: u64,
    lifecycle_overhead_seconds: u64,
    invocation_cap_seconds: u64,
) -> Result<u64, ClusterBenchmarkError> {
    let rates = u64::try_from(reviewed_rate_count).map_err(|_| {
        ClusterBenchmarkError::Unsupported("reviewed rate count overflowed".to_owned())
    })?;
    let required = rates
        .checked_mul(3)
        .and_then(|pairs| pairs.checked_mul(13_u64.checked_add(measured_seconds)?))
        .and_then(|windows| windows.checked_add(lifecycle_overhead_seconds))
        .ok_or_else(|| {
            ClusterBenchmarkError::Unsupported(
                "qualification duration preflight overflowed".to_owned(),
            )
        })?;
    if required > invocation_cap_seconds {
        return Err(ClusterBenchmarkError::Unsupported(format!(
            "qualification requires {required}s but invocation cap is {invocation_cap_seconds}s"
        )));
    }
    Ok(required)
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
        .reports()
        .map(|report| {
            let scenario = &report.scenario;
            (
                format!(
                    "{}:{}:{:?}:{}:{:?}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
                    scenario.scenario_id,
                    scenario.workload_version,
                    scenario.topology,
                    scenario.tenants,
                    scenario.traffic,
                    scenario.rows_per_batch,
                    scenario.query_row_limit,
                    scenario.offered_requests_per_second,
                    scenario.warmup_seconds,
                    scenario.measured_seconds,
                    scenario.trials,
                    scenario.minimum_samples,
                    scenario.max_in_flight,
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
    let balanced = profile.reports().filter(|report| {
        report.scenario.traffic == TrafficMix::Balanced && report.scenario.tenants == 8
    });
    let mut one = None;
    let mut three = None;
    for report in balanced {
        let highest_rate = report.scenario.offered_requests_per_second;
        match report.scenario.topology {
            ClusterTopology::OnePod => one = Some(one.unwrap_or(0).max(highest_rate)),
            ClusterTopology::ThreeServersThreeForgeWorkers => {
                three = Some(three.unwrap_or(0).max(highest_rate));
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

    /// Build one evidence-complete retained stage for qualification selection tests.
    fn qualification_stage(rate: u64, kind: CapacityStageKind, passed: bool) -> CapacityStage {
        let operations = [
            BenchmarkOperation::DurableWrite,
            BenchmarkOperation::FlushToVisible,
            BenchmarkOperation::QueryTimeToFirstFrame,
            BenchmarkOperation::QueryTotal,
        ];
        CapacityStage {
            identity: CapacityStageIdentity {
                stage_id: format!("stage-{kind:?}-{rate}"),
                ordinal: u16::try_from(rate).unwrap_or(u16::MAX),
                tenant_rows: Vec::new(),
            },
            kind,
            offered_requests_per_second: rate,
            completed_requests_per_second: rate.to_f64().unwrap_or(f64::INFINITY),
            duration: Duration::from_secs(20),
            passed,
            in_flight_cap_exhaustions: 0,
            stop_reasons: if passed {
                Vec::new()
            } else {
                vec![CapacityLimit::Backpressure]
            },
            metrics: fixture_client(),
            telemetry: PillarTelemetryDelta {
                status: EvidenceStatus::Complete,
                ..Default::default()
            },
            resources: vec![ProcessResourceEvidence {
                process_id: ProcessId("pid-test".to_owned()),
                epoch: 0,
                hosted_logical_nodes: vec!["server-0".to_owned()],
                roles: vec![BifrostRuntimeRole::Oracle],
                cpu_seconds: 1.0,
                peak_rss_bytes: 1,
                current_rss_bytes: 1,
                runtime_busy_seconds: 1.0,
                runtime_queue_peak: 1,
            }],
            dependencies: DependencyTelemetryEvidence {
                status: EvidenceStatus::Complete,
                ..Default::default()
            },
            traces: operations
                .into_iter()
                .map(|operation| TraceManifest {
                    operation,
                    representative_trace_ids: vec![format!("trace-{operation:?}")],
                    critical_path_spans: vec![SpanDistribution {
                        operation,
                        samples: 1,
                        p95_us: 1,
                        p99_us: 1,
                    }],
                })
                .collect(),
        }
    }

    /// Proves capacity stages use the exact absolute-rate sequence and one
    /// confirmation after the first failed stage.
    #[test]
    fn capacity_sequence_is_absolute_and_bounded() {
        assert_eq!(
            capacity_rate_sequence(false),
            vec![25, 50, 75, 100, 200, 300, 500, 750, 1_000, 1_500, 2_000]
        );
        assert_eq!(
            capacity_rate_sequence(true),
            vec![
                25, 50, 75, 100, 200, 300, 500, 750, 1_000, 1_500, 2_000, 2_500, 3_000, 4_000
            ]
        );
        assert!(requires_failure_confirmation(1));
        assert!(!requires_failure_confirmation(0));
        assert!(!requires_failure_confirmation(2));
        assert_eq!(
            capacity_stage_rates(&[true, true]),
            OPTIONAL_CAPACITY_RATES.iter().fold(
                CANONICAL_CAPACITY_RATES.to_vec(),
                |mut rates, rate| {
                    rates.push(*rate);
                    rates
                }
            )
        );
        assert_eq!(capacity_stage_rates(&[true, false]), vec![25, 50, 50, 25]);
    }

    /// Proves every scenario derives and retains its own reviewed rate curve.
    #[test]
    fn qualification_profile_is_per_scenario_and_compact() {
        let mut source = profile();
        for (index, scenario) in source.scenarios.iter_mut().enumerate() {
            let rates = if index == 1 {
                vec![25, 50, 75]
            } else {
                vec![25, 50, 75, 100, 200]
            };
            let mut stages = rates
                .into_iter()
                .map(|rate| qualification_stage(rate, CapacityStageKind::Discovery, true))
                .collect::<Vec<_>>();
            let recovery = stages
                .last()
                .expect("passing fixture has a stage")
                .offered_requests_per_second;
            stages.push(qualification_stage(
                recovery,
                CapacityStageKind::Recovery,
                true,
            ));
            scenario.reports[0].capacity_stages = stages;
        }
        let candidate = QualificationProfileV2::from_capacity_report(&source, "a".repeat(64))
            .expect("all scenarios qualify");
        candidate
            .validate_matrix()
            .expect("canonical six are covered");
        let mut duplicate = candidate.clone();
        duplicate.scenarios[5] = duplicate.scenarios[0].clone();
        assert!(matches!(
            duplicate.validate(),
            Err(ClusterBenchmarkError::Incompatible(_))
        ));
        let mut missing = candidate.clone();
        missing.scenarios.pop();
        assert!(matches!(
            missing.validate_matrix(),
            Err(ClusterBenchmarkError::Unsupported(_))
        ));
        let mut seventh = candidate.clone();
        let mut unexpected = seventh.scenarios[0].clone();
        unexpected.scenario_id = "unexpected-seventh".to_owned();
        unexpected.source_workload.scenario_id = unexpected.scenario_id.clone();
        unexpected.qualification_workload.scenario_id = unexpected.scenario_id.clone();
        for stage in &mut unexpected.selected_stages {
            stage.scenario_id = unexpected.scenario_id.clone();
        }
        seventh.scenarios.push(unexpected);
        assert!(matches!(
            seventh.validate_matrix(),
            Err(ClusterBenchmarkError::Unsupported(_))
        ));
        assert_eq!(
            candidate.scenarios[0]
                .rate(QualificationRateRole::TargetOperating)
                .expect("target"),
            100
        );
        assert_eq!(
            candidate.scenarios[1]
                .rate(QualificationRateRole::TargetOperating)
                .expect("target"),
            50
        );
        assert_eq!(
            candidate
                .distributed_target_operating_rate()
                .expect("distributed target"),
            100
        );
        candidate
            .validate_capacity_source(&source)
            .expect("digest-bound entries match source");
        let json = serde_json::to_string(&candidate).expect("profile serializes");
        assert!(!json.contains("tenant_rows"));
        assert!(!json.contains("row_ids"));

        let mut crossed = candidate.clone();
        crossed.scenarios[0].selected_stages[0].scenario_id =
            crossed.scenarios[1].scenario_id.clone();
        assert!(matches!(
            crossed.validate(),
            Err(ClusterBenchmarkError::Incompatible(_))
        ));

        let mut selected_only = candidate;
        selected_only.scenarios.truncate(1);
        selected_only
            .validate()
            .expect("one selected scenario is valid");
        assert!(matches!(
            selected_only.validate_matrix(),
            Err(ClusterBenchmarkError::Unsupported(_))
        ));
    }

    /// Proves insufficient capacity never receives fallback qualification rates.
    #[test]
    fn qualification_profile_refuses_insufficient_sources() {
        let mut source = profile();
        for scenario in &mut source.scenarios {
            scenario.reports[0].capacity_stages = vec![
                qualification_stage(25, CapacityStageKind::Discovery, true),
                qualification_stage(50, CapacityStageKind::Discovery, true),
                qualification_stage(50, CapacityStageKind::Recovery, true),
            ];
        }
        assert!(matches!(
            QualificationProfileV2::from_capacity_report(&source, "b".repeat(64)),
            Err(ClusterBenchmarkError::Unsupported(_))
        ));
    }

    /// Proves the concrete state machine confirms the first failure and then
    /// replays the highest healthy rate under a fresh stage identity.
    #[test]
    fn capacity_state_machine_confirms_and_recovers() {
        let mut machine = CapacityStateMachine::canonical();
        let first = machine.next_plan().expect("first discovery exists");
        assert_eq!(first.offered_requests_per_second, 25);
        machine
            .record(first, CapacityStageOutcome::Passing)
            .expect("healthy stage records");
        let second = machine.next_plan().expect("second discovery exists");
        assert_eq!(second.offered_requests_per_second, 50);
        machine
            .record(second, CapacityStageOutcome::ControlledBackpressure)
            .expect("failure records");
        let confirmation = machine.next_plan().expect("confirmation exists");
        assert_eq!(confirmation.kind, CapacityStageKind::Confirmation);
        assert_eq!(confirmation.offered_requests_per_second, 50);
        machine
            .record(confirmation, CapacityStageOutcome::ControlledBackpressure)
            .expect("confirmation records");
        let recovery = machine.next_plan().expect("recovery exists");
        assert_eq!(recovery.kind, CapacityStageKind::Recovery);
        assert_eq!(recovery.offered_requests_per_second, 25);
        assert_ne!(recovery.slot, first.slot);
        machine
            .record(recovery, CapacityStageOutcome::Passing)
            .expect("recovery records");
        assert!(machine.next_plan().is_none());
    }

    /// Proves a passing confirmation resumes discovery at the next rate.
    #[test]
    fn capacity_passing_confirmation_resumes_discovery() {
        let mut machine = CapacityStateMachine::canonical();
        let first = machine.next_plan().unwrap();
        machine
            .record(first, CapacityStageOutcome::Passing)
            .unwrap();
        let failed = machine.next_plan().unwrap();
        machine
            .record(failed, CapacityStageOutcome::ControlledBackpressure)
            .unwrap();
        let confirmation = machine.next_plan().unwrap();
        machine
            .record(confirmation, CapacityStageOutcome::Passing)
            .unwrap();
        let resumed = machine.next_plan().unwrap();
        assert_eq!(resumed.kind, CapacityStageKind::Discovery);
        assert_eq!(resumed.offered_requests_per_second, 75);
    }

    /// Proves an all-passing curve still ends with a fresh recovery identity.
    #[test]
    fn capacity_all_pass_curve_replays_highest_rate() {
        let mut machine = CapacityStateMachine::canonical();
        let mut discoveries = Vec::new();
        loop {
            let plan = machine.next_plan().unwrap();
            if plan.kind == CapacityStageKind::Recovery {
                assert_eq!(plan.offered_requests_per_second, 4_000);
                assert_eq!(plan.slot, 14);
                machine.record(plan, CapacityStageOutcome::Passing).unwrap();
                break;
            }
            discoveries.push(plan.offered_requests_per_second);
            machine.record(plan, CapacityStageOutcome::Passing).unwrap();
        }
        assert_eq!(discoveries.len(), 14);
        assert!(machine.next_plan().is_none());
    }

    /// Proves repeated transient failures consume only one confirmation and stay bounded.
    #[test]
    fn capacity_repeated_transient_failure_has_single_confirmation() {
        let mut machine = CapacityStateMachine::canonical();
        let mut plans = Vec::new();
        while let Some(plan) = machine.next_plan() {
            let passed = match plan.kind {
                CapacityStageKind::Discovery => plans.len() != 1 && plans.len() != 4,
                CapacityStageKind::Confirmation | CapacityStageKind::Recovery => true,
            };
            plans.push(plan);
            machine
                .record(
                    plan,
                    if passed {
                        CapacityStageOutcome::Passing
                    } else {
                        CapacityStageOutcome::ControlledBackpressure
                    },
                )
                .unwrap();
        }
        assert_eq!(
            plans
                .iter()
                .filter(|plan| plan.kind == CapacityStageKind::Confirmation)
                .count(),
            1
        );
        assert!(plans.len() <= 13);
        assert_eq!(
            plans.last().map(|plan| plan.kind),
            Some(CapacityStageKind::Recovery)
        );
    }

    /// Proves qualification rejects an invocation cap that cannot contain all
    /// pair-local warmup, conditioning, measurement, and lifecycle work.
    #[test]
    fn qualification_preflight_uses_complete_formula() {
        assert_eq!(qualification_preflight_seconds(3, 20, 60, 400), Ok(357));
        assert!(matches!(
            qualification_preflight_seconds(3, 20, 60, 356),
            Err(ClusterBenchmarkError::Unsupported(_))
        ));
    }

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
        let mut grouped = BTreeMap::<String, Vec<ClusterScenarioReport>>::new();
        for scenario in scenarios {
            grouped
                .entry(scenario.scenario.scenario_id.clone())
                .or_default()
                .push(scenario);
        }
        let scenarios = grouped
            .into_iter()
            .map(|(scenario_id, mut reports)| {
                reports.sort_by_key(|report| report.scenario.offered_requests_per_second);
                let first = &reports[0];
                ReviewedScenarioProfile {
                    scenario_id: scenario_id.clone(),
                    workload: ClusterWorkloadIdentity {
                        scenario_id,
                        topology: first.scenario.topology,
                        tenants: first.scenario.tenants,
                        traffic: first.scenario.traffic,
                        rows_per_batch: first.scenario.rows_per_batch,
                        query_row_limit: first.scenario.query_row_limit,
                        auth_cache_ttl_seconds: AUTH_CACHE_TTL_SECONDS,
                        auth_invalidation_mode: AUTH_INVALIDATION_MODE.to_owned(),
                        flush_policy: FLUSH_POLICY.to_owned(),
                        routing_policy: ROUTING_POLICY.to_owned(),
                        batching_policy: BATCHING_POLICY.to_owned(),
                        storage_runtime_identity:
                            "local-filesystem:target/bifrost-benchmarks/storage:local".to_owned(),
                        allocation_policy: ALLOCATION_POLICY.to_owned(),
                        tables_per_tenant: u32::try_from(reports.len() * 3 * 2).unwrap_or(u32::MAX),
                        ordered_rates: reports
                            .iter()
                            .map(|report| report.scenario.offered_requests_per_second)
                            .collect(),
                        trial_count: 3,
                        warmup_seconds: 10,
                        conditioning_seconds: 3,
                        measured_seconds: 20,
                    },
                    offered_rates: reports
                        .iter()
                        .map(|report| report.scenario.offered_requests_per_second)
                        .collect(),
                    reports,
                }
            })
            .collect();
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
            storage_image: "local-filesystem".to_owned(),
            storage_version: "wyrd-storage/0.0.1".to_owned(),
            storage_config: "memory-loopback-v1".to_owned(),
            storage_backend_kind: "local-filesystem".to_owned(),
            storage_root: "target/bifrost-benchmarks/storage".to_owned(),
            storage_device_class: "local".to_owned(),
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
                warmup_seconds: 10,
                measured_seconds: 20,
                trials: 3,
                minimum_samples: 200,
                max_in_flight: 4096,
                seed: 0xB1_F057,
            },
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
            trial_evidence: (1..=3).map(fixture_trial_evidence).collect(),
            capacity_stages: Vec::new(),
        }
    }

    /// Build complete bounded evidence for one fixture qualification trial.
    fn fixture_trial_evidence(trial_index: u8) -> ClusterTrialReport {
        let operations = [
            BenchmarkOperation::DurableWrite,
            BenchmarkOperation::FlushToVisible,
            BenchmarkOperation::QueryTimeToFirstFrame,
            BenchmarkOperation::QueryTotal,
        ];
        ClusterTrialReport {
            offered_requests_per_second: 100,
            trial_index,
            metrics: ClientTrialMetrics::default(),
            telemetry: PillarTelemetryDelta {
                status: EvidenceStatus::Complete,
                ..Default::default()
            },
            resources: vec![ProcessResourceEvidence {
                process_id: ProcessId("fixture".to_owned()),
                epoch: 0,
                hosted_logical_nodes: vec!["server-0".to_owned()],
                roles: vec![BifrostRuntimeRole::Gate],
                cpu_seconds: 1.0,
                peak_rss_bytes: 1,
                current_rss_bytes: 1,
                runtime_busy_seconds: 1.0,
                runtime_queue_peak: 1,
            }],
            dependencies: DependencyTelemetryEvidence {
                status: EvidenceStatus::Complete,
                ..Default::default()
            },
            traces: operations
                .into_iter()
                .map(|operation| TraceManifest {
                    operation,
                    representative_trace_ids: vec!["trace".to_owned()],
                    critical_path_spans: vec![SpanDistribution {
                        operation,
                        samples: 1,
                        p95_us: 1,
                        p99_us: 1,
                    }],
                })
                .collect(),
        }
    }

    /// Proves every qualification evidence family gates report validation.
    #[test]
    fn qualification_missing_evidence_families_are_non_promotable() {
        let fixture = || {
            fixture_scenario(
                "balanced-one-pod-one-tenant",
                ClusterTopology::OnePod,
                1,
                TrafficMix::Balanced,
                50,
                100,
            )
        };
        let mut report = fixture();
        report.trial_evidence[0].telemetry.status = EvidenceStatus::Failed;
        assert!(matches!(
            report.validate(),
            Err(ClusterBenchmarkError::NotReady(_))
        ));
        let mut report = fixture();
        report.trial_evidence[0].resources.clear();
        assert!(matches!(
            report.validate(),
            Err(ClusterBenchmarkError::NotReady(_))
        ));
        let mut report = fixture();
        report.trial_evidence[0].dependencies.status = EvidenceStatus::Failed;
        assert!(matches!(
            report.validate(),
            Err(ClusterBenchmarkError::NotReady(_))
        ));
        let mut report = fixture();
        report.trial_evidence[0].traces.clear();
        assert!(matches!(
            report.validate(),
            Err(ClusterBenchmarkError::NotReady(_))
        ));
        let mut report = fixture();
        report.trial_evidence.clear();
        assert!(matches!(
            report.validate(),
            Err(ClusterBenchmarkError::Unsupported(_))
        ));
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
            planned_operations: 400,
            attempted_operations: 400,
            accepted_operations: 400,
            backpressure_operations: 0,
            write_backpressure_operations: 0,
            query_backpressure_operations: 0,
            backpressure_by_code: BTreeMap::new(),
            retry_operations: 0,
            in_flight_cap_exhaustions: 0,
            max_in_flight: 256,
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
        assert!(json["scenarios"][0].get("reports").is_some());
        assert!(json["scenarios"][0].get("scenario").is_none());
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

    /// Proves in-memory/mock storage cannot qualify as a capacity reference.
    #[test]
    fn memory_storage_is_unsupported_for_qualification() {
        let mut environment = profile().environment;
        environment.storage_image = "in-process-memory".to_owned();
        assert!(matches!(
            environment.validate(),
            Err(ClusterBenchmarkError::Unsupported(_))
        ));
    }

    /// Proves a missing required operation sample is unsupported, never ready.
    #[test]
    fn insufficient_samples_are_unsupported() {
        let mut report = profile();
        report.scenarios[0].reports[0].trials[0]
            .client
            .durable_write
            .samples = 199;
        assert!(matches!(
            report.validate_reference(),
            Err(ClusterBenchmarkError::Unsupported(_))
        ));
    }

    /// Proves first-probe failure is a not-ready condition represented in evidence.
    #[test]
    fn first_probe_failure_cannot_become_reference() {
        let mut report = profile();
        report.scenarios[0].reports[0].trials[0]
            .production
            .correctness = EvidenceStatus::Failed;
        assert!(matches!(
            report.validate_reference(),
            Err(ClusterBenchmarkError::NotReady(_))
        ));
    }

    /// Proves exact-rate identity mismatches refuse comparison.
    #[test]
    fn candidate_must_replay_exact_reference_rate() {
        let before = profile();
        let mut after = profile();
        after.scenarios[0].reports[0]
            .scenario
            .offered_requests_per_second += 1;
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
        mismatched.scenarios[0].reports[0].scenario.traffic = TrafficMix::ReadHeavy;
        assert!(matches!(
            mismatched.validate_reference(),
            Err(ClusterBenchmarkError::Invalid(_))
        ));

        let mut forged_median = profile();
        forged_median.scenarios[0].reports[0]
            .median
            .queries_per_second += 1.0;
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
        after.scenarios[0].reports[0].scenario.seed += 1;
        let comparison = compare_cluster_profiles(&before, &after);
        assert_eq!(comparison.compatibility, ProfileCompatibility::Incompatible);
        assert!(!comparison.ready);
    }

    /// Proves all exact relative thresholds pass at the boundary and fail beyond it.
    #[test]
    fn regression_threshold_boundaries_are_inclusive() {
        let before = profile();
        let mut boundary = profile();
        for reviewed in &mut boundary.scenarios {
            for report in &mut reviewed.reports {
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
        }
        assert!(compare_cluster_profiles(&before, &boundary).ready);
        boundary.scenarios[0].reports[0]
            .median
            .durable_rows_per_second = 899.9;
        synchronize_fixture_trials(&mut boundary.scenarios[0].reports[0]);
        assert!(!compare_cluster_profiles(&before, &boundary).ready);
    }
}
