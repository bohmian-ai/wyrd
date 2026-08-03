//! Shared infrastructure for SLO-gated Criterion benches.

pub mod cluster;
pub mod lane;
pub mod recorder;
pub mod report;
pub mod slo;
pub mod workload;

pub use cluster::{
    BenchmarkEnvironment, BifrostBenchmarkComparison, BifrostReferenceProfile, BifrostSloEnvelope,
    CALIBRATION_RATE_CAP, CLUSTER_REPORT_VERSION, CLUSTER_WORKLOAD_VERSION, ClientTrialMetrics,
    ClusterBenchmarkError, ClusterBenchmarkScenario, ClusterBenchmarkTrial, ClusterScenarioReport,
    ClusterTopology, EvidenceStatus, FIRST_PROBE_RATE, FLUSH_CADENCE_SECONDS, KneeProvenance,
    MedianMetrics, MetricRegression, PLANNED_FLUSHES_PER_TRIAL, ProductionTelemetryEvidence,
    ProfileCompatibility, TrafficMix, TrialDistribution, calibration_probe_seconds,
    classify_cpu_vendor, compare_cluster_profiles, derive_trial_median, extract_linux_cpu_identity,
    extract_macos_cpu_identity, jain_fairness, measured_flush_offsets_seconds, median_three_f64,
    median_three_u64, normalize_cpu_model,
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
