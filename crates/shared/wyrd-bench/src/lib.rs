//! Shared infrastructure for SLO-gated Criterion benches.

pub mod compare;
pub mod report;
pub mod slo;
pub mod workload;

pub use compare::{BenchmarkStage, ComparisonError, ComparisonReport, Regression, compare_stages};
pub use report::{
    BacklogSample, BenchmarkReport, ForgeMeasurements, LatencyPercentiles, MachineMetadata,
    PhaseMeasurement, PodMetadata, QueryMeasurements, ReportError, ScribeMatrixCase,
    StageMeasurements, StorageMeasurements, VerificationMeasurements, required_scribe_matrix,
};
pub use slo::{SloError, SloGate, SloMeasurement, SloResult, SloThreshold};
pub use workload::{
    FaultPoint, SchemaWidth, TrafficShape, WorkloadCase, WorkloadError, WorkloadSpec,
};
