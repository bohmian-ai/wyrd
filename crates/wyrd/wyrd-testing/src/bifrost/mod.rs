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
pub mod cluster;
pub mod forge_harness;
pub mod harness;
pub mod telemetry;

pub use cluster::{BifrostTopology, ClusterError, WyrdTestCluster, full_bifrost_topology};
pub use forge_harness::{
    CommitUncertaintyCatalog, ForgeFixture, ForgeObjectStoreControl, StandaloneForgeFixture,
    seed_forge_group, seed_forge_group_for_tenant, seed_forge_group_for_tenant_with_schema,
    seed_forge_group_for_tenant_with_schema_and_days,
};
pub use harness::{BifrostHarness, HarnessError};
pub use telemetry::{
    BifrostQueryTelemetryReport, ForgeMaintenanceTelemetryReport, ForgeTelemetryCapture,
    ForgeTelemetryReportError,
};
