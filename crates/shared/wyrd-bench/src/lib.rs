//! Shared infrastructure for SLO-gated Criterion benches.

pub mod cluster;
pub mod lane;
pub mod recorder;
pub mod report;
pub mod slo;
pub mod workload;

pub use cluster::{
    ALLOCATION_POLICY, AUTH_CACHE_TTL_SECONDS, AUTH_INVALIDATION_MODE, AttemptedProbe,
    BATCHING_POLICY, BenchmarkEnvironment, BenchmarkOperation, BifrostBenchmarkComparison,
    BifrostDiagnosticReport, BifrostReferenceProfile, BifrostRuntimeRole, BifrostSloEnvelope,
    CANONICAL_CAPACITY_RATES, CAPACITY_CONDITIONING_SECONDS, CAPACITY_MEASURED_SECONDS,
    CAPACITY_TABLES_PER_TENANT, CAPACITY_WARMUP_SECONDS, CLUSTER_REPORT_VERSION,
    CLUSTER_WORKLOAD_VERSION, CapacityLimit, CapacityStage, CapacityStageIdentity,
    CapacityStageKind, CapacityStageOutcome, CapacityStagePlan, CapacityStateMachine,
    ClientTrialMetrics, ClusterBenchmarkError, ClusterBenchmarkScenario, ClusterBenchmarkTrial,
    ClusterScenarioReport, ClusterTopology, ClusterTrialReport, ClusterWorkloadIdentity,
    DependencyTelemetryEvidence, DiagnosticStatus, EvidenceStatus, FLUSH_CADENCE_SECONDS,
    FLUSH_POLICY, MATRIX_DURATION_CAP, MedianMetrics, MetricRegression, OPTIONAL_CAPACITY_RATES,
    PLANNED_FLUSHES_PER_TRIAL, PillarTelemetryDelta, ProcessId, ProcessResourceEvidence,
    ProductionTelemetryEvidence, ProfileCompatibility, QUALIFICATION_PROFILE_VERSION,
    QualificationProfileV2, QualificationRateRole, ROUTING_POLICY, ReviewedCapacitySource,
    ReviewedQualificationRate, ReviewedScenarioProfile, ReviewedScenarioQualification,
    ReviewedStageRef, SMOKE_CAPACITY_RATES, SpanDistribution, TenantStageRows, TraceManifest,
    TrafficMix, TrialDistribution, capacity_rate_sequence, capacity_stage_rates,
    classify_cpu_vendor, compare_cluster_profiles, derive_trial_median, extract_linux_cpu_identity,
    extract_macos_cpu_identity, jain_fairness, measured_flush_offsets_seconds, median_three_f64,
    median_three_u64, normalize_cpu_model, qualification_preflight_seconds,
    requires_failure_confirmation,
};

pub use lane::{
    BenchmarkReadiness, BifrostFaultProfile, BifrostLane, BifrostPayloadShape,
    BifrostReportEnvelope, BifrostScenario, DurableAckReport, DurableAckSample, NegativeFlowReport,
    REPORT_SCHEMA_VERSION, ScenarioError, compare_bifrost_stages, summarize_durable_acks,
};
pub use recorder::{BenchmarkMetricSnapshot, BenchmarkRecorder, HistogramSnapshot};
pub use report::{
    ScribeCompactCase, ScribeComponentReport, ScribeDistribution, compact_scribe_matrix,
    required_scribe_components, select_compact_scribe_cases,
};
pub use slo::{SloError, SloGate, SloMeasurement, SloResult, SloThreshold};
pub use workload::{
    FaultPoint, SchemaWidth, TrafficShape, WorkloadCase, WorkloadError, WorkloadSpec,
};
