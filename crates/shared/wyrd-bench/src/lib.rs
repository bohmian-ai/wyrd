//! Shared infrastructure for SLO-gated Criterion benches.

pub mod compare;
pub mod recorder;
pub mod report;
pub mod slo;
pub mod workload;

pub use compare::{BenchmarkStage, ComparisonError, ComparisonReport, Regression, compare_stages};
pub use recorder::{BenchmarkMetricSnapshot, BenchmarkRecorder, HistogramSnapshot};
pub use report::{
    BacklogSample, BenchmarkReport, ForgeMeasurements, LatencyPercentiles, MachineMetadata,
    PhaseMeasurement, PodMetadata, QueryMeasurements, ReportError, ScribeBenchmarkConfig,
    ScribeBenchmarkReport, ScribeCaseReport, ScribeCaseVerification, ScribeCompactCase,
    ScribeComparisonStatus, ScribeComponentReport, ScribeDistribution, ScribeMatrixCase,
    ScribeTenantReport, ScribeTopologyEvidence, StageMeasurements, StorageMeasurements,
    VerificationMeasurements, compact_scribe_matrix, required_scribe_components,
    required_scribe_matrix, required_scribe_metric_families,
};
pub use slo::{SloError, SloGate, SloMeasurement, SloResult, SloThreshold};
pub use workload::{
    FaultPoint, SchemaWidth, TrafficShape, WorkloadCase, WorkloadError, WorkloadSpec,
};
