//! Multi-pod Bifrost test harness.

#[cfg(feature = "bench")]
pub mod bench_forge;
#[cfg(feature = "bench")]
pub mod bench_oracle;
#[cfg(feature = "bench")]
pub mod bench_otlp;
#[cfg(feature = "bench")]
pub(crate) mod bench_report;
#[cfg(feature = "bench")]
pub mod bench_scribe;
pub mod calibration;
pub mod cluster;
#[cfg(test)]
mod deployment_contract;
pub mod forge_harness;
pub mod harness;

pub use cluster::{
    BifrostClusterSpec, BifrostNodeSpec, BifrostTopology, ClusterError, TestOracleResources,
    WyrdTestCluster, full_bifrost_topology,
};
pub use forge_harness::{
    CommitUncertaintyCatalog, ForgeFixture, ForgeObjectStoreControl, seed_forge_group,
    seed_forge_group_for_tenant, seed_forge_group_for_tenant_with_schema,
    seed_forge_group_for_tenant_with_schema_and_days,
};
pub use harness::{BifrostHarness, HarnessError};
