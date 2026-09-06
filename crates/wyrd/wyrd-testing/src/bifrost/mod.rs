//! Multi-pod Bifrost test harness.

#[cfg(feature = "bench")]
pub mod bench_cluster;
#[cfg(feature = "bench")]
pub mod bench_dataset;
#[cfg(feature = "bench")]
pub mod bench_families;
#[cfg(feature = "bench")]
pub mod bench_materializer;
#[cfg(feature = "bench")]
pub mod bench_oracle;
#[cfg(feature = "bench")]
pub mod bench_otlp;
#[cfg(feature = "bench")]
pub mod bench_qualification;
#[cfg(feature = "bench")]
pub mod bench_qualification_driver;
#[cfg(feature = "bench")]
pub mod bench_report;
#[cfg(feature = "bench")]
pub mod bench_runner;
#[cfg(feature = "bench")]
pub mod bench_scribe;
pub mod calibration;
pub mod cluster;
#[cfg(test)]
mod deployment_contract;
pub mod forge_harness;
pub mod harness;
pub mod query_fixture;
pub mod scribe_workload;
pub mod telemetry;

pub use cluster::{
    BifrostClusterSpec, BifrostNodeSpec, BifrostTopology, ClusterError, ClusterShutdownInspection,
    OracleFollowerPauses, RetainedNodeRoots, TestOracleResources, WyrdTestCluster,
    full_bifrost_topology, shared_process_telemetry_for_test,
};
pub use forge_harness::{
    CommitUncertaintyCatalog, ForgeFixture, ForgeObjectStoreControl, seed_forge_group,
    seed_forge_group_for_tenant, seed_forge_group_for_tenant_with_schema,
    seed_forge_group_for_tenant_with_schema_and_days,
};
pub use harness::{BifrostHarness, HarnessError};
pub use query_fixture::{SeededBifrostQuery, seed_query_fixture};
pub use scribe_workload::{
    SCRIBE_PRODUCTION_WORKLOAD_VERSION, ScribeCacheMode, ScribeCheckpointNameV1,
    ScribeGeometryRecipeV1, ScribeLifecycleCheckpointV1, ScribeProductionEvidenceV1,
    ScribeProductionWorkloadV1, ScribePublishedHotFileV1, ScribeWorkloadError,
    ScribeWorkloadOperationV1, ScribeWorkloadRunV1, ScribeWorkloadTableKindV1,
    ScribeWorkloadTableV1, ScribeWorkloadTenantBinding, ScribeWorkloadTenantV1,
    scribe_workload_digest,
};
pub use telemetry::{
    BifrostQueryTelemetryReport, BifrostTelemetryCapture, BifrostTelemetryCheckpoint,
    BifrostTelemetryReportError,
};
