//! Shared infrastructure for SLO-gated Criterion benches.

pub mod cluster;
pub mod lane;
pub mod recorder;
pub mod report;
pub mod slo;
pub mod workload;

pub use cluster::{
    CLUSTER_WORKLOAD_VERSION, ClientTrialMetrics, ClusterBenchmarkError, ClusterBenchmarkScenario,
    ClusterBenchmarkTrial, ClusterTopology, EvidenceStatus, ProductionTelemetryEvidence,
    TenantStageRows, TrafficMix, TrialDistribution, jain_fairness,
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
