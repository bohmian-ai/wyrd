//! Shared infrastructure for SLO-gated Criterion benches.

pub mod compare;
pub mod report;
pub mod slo;
pub mod workload;

pub use compare::{BenchmarkStage, ComparisonError, ComparisonReport, Regression, compare_stages};
pub use report::{
    BenchmarkReport, LatencyPercentiles, MachineMetadata, PodMetadata, QueryMeasurements,
    ReportError, StorageMeasurements,
};
pub use slo::{SloError, SloGate, SloMeasurement, SloResult, SloThreshold};
pub use workload::{
    FaultPoint, SchemaWidth, TrafficShape, WorkloadCase, WorkloadError, WorkloadSpec,
};
