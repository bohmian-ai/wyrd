//! Multi-pod Bifrost test harness.

/// One fixed canonical trace/log/metric dataset as public Arrow batches.
pub mod canonical_signals;
pub mod cluster;
#[cfg(test)]
mod deployment_contract;
pub mod forge_harness;
/// One in-memory certificate authority minting dual-EKU Bifrost peer leaves.
pub mod peer_ca;
/// Peer ticket keyring material for rotation and independence journeys.
pub mod peer_keyring;
/// Multi-process peer network used by the Tier-2 peer-network journeys.
pub mod process_cluster;
pub mod query_fixture;
pub mod scribe_workload;
pub mod telemetry;
/// The two Bifrost write doors tests are allowed to use.
pub mod write;

pub use cluster::{
    BifrostClusterSpec, BifrostNodeSpec, BifrostTopology, ClusterError, ClusterShutdownInspection,
    RetainedNodeRoots, TestOracleResources, WyrdTestCluster, full_bifrost_topology,
    shared_process_telemetry_for_test,
};
pub use forge_harness::{
    CommitUncertaintyCatalog, ForgeFixture, ForgeObjectStoreControl, seed_forge_group,
    seed_forge_group_for_tenant, seed_forge_group_for_tenant_with_schema,
    seed_forge_group_for_tenant_with_schema_and_days,
};
pub use query_fixture::{SeededBifrostQuery, seed_query_fixture};
pub use scribe_workload::{
    SCRIBE_PRODUCTION_WORKLOAD_VERSION, ScribeCacheMode, ScribeCheckpointNameV1,
    ScribeDrainObservationV1, ScribeGeometryRecipeV1, ScribeLifecycleCheckpointV1,
    ScribeProductionEvidenceV1, ScribeProductionWorkloadV1, ScribePublishedHotFileV1,
    ScribeStorageDrainObservationV1, ScribeWorkloadError, ScribeWorkloadOperationV1,
    ScribeWorkloadRunV1, ScribeWorkloadTableKindV1, ScribeWorkloadTableV1,
    ScribeWorkloadTenantBinding, ScribeWorkloadTenantV1, scribe_workload_digest,
};
pub use telemetry::{
    BifrostQueryTelemetryReport, BifrostTelemetryCapture, BifrostTelemetryCheckpoint,
    BifrostTelemetryReportError,
};
pub use write::{BifrostWriter, RawIngest};
