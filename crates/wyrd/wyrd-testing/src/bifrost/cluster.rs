//! Shared-resource, independently bound Bifrost test cluster.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use metrics_exporter_prometheus::PrometheusHandle;
use opendal::Operator;
use sqlx::Row;
use thiserror::Error;
use vala_bifrost::catalog::WyrdCatalog;
use vala_bifrost_redux::catalog::BifrostCatalog;
use vala_bifrost_redux::forge::{ForgeConfig, ForgeWorkerCompletionObserver};
use vala_bifrost_redux::oracle::dispatcher::{
    OraclePeerCredentials, OraclePeerTls, TonicOraclePeerTransport,
};
use vala_bifrost_redux::scribe::admission::AdmissionConfig;
use wyrd_auth::seed::seed_builtin_roles_for_tenant;
use wyrd_dev_fixtures::pg::PgFixture;
#[cfg(test)]
use wyrd_runtime::{Permission, PermissionSet};
use wyrd_server::app::metrics::install_recorder;
use wyrd_server::config::BifrostRuntimeRole;
use wyrd_server::config::ForgeProcessRole;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{NodeId, TenantTableBinding};
use wyrd_storage::{BackendConfig, StorageHandle, StorageSettings};
use wyrd_telemetry::{CapturedSpan, TelemetryConfig, TelemetryGuard, TestTraceCapture};

use crate::bifrost::forge_harness::CommitUncertaintyCatalog;
use crate::bifrost::telemetry::BifrostTelemetryCapture;
use crate::server::{
    TestOraclePeerTls, WyrdTestServer, WyrdTestServerBuilder, WyrdTestServerError,
    provision_oracle_peer_credentials, test_catalog, test_redux_catalog,
};

/// Supported role topology for a Bifrost cluster journey.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BifrostTopology {
    /// Run one full Gate, Scribe, Forge, and Oracle pod.
    OnePod,
    /// Run three full Gate, Scribe, Forge, and Oracle pods.
    ThreePod,
    /// Run one ingest pod and two query pods.
    RoleSeparated,
    /// Run six mixed pods for capacity calibration.
    SixPod,
    /// Run one API/scheduler process with three dedicated Forge workers.
    DedicatedForgeWorkers,
    /// Run exactly three public Server processes and three dedicated workers.
    ThreeServersThreeForgeWorkers,
}

impl BifrostTopology {
    /// Return the concrete node descriptor for this named topology.
    fn spec(self) -> BifrostClusterSpec {
        match self {
            Self::OnePod => BifrostClusterSpec::one_mixed(),
            Self::ThreePod => BifrostClusterSpec::three_mixed(),
            Self::RoleSeparated => BifrostClusterSpec::role_separated(),
            Self::SixPod => BifrostClusterSpec::six_capacity(),
            Self::DedicatedForgeWorkers => BifrostClusterSpec::dedicated_forge_workers(),
            Self::ThreeServersThreeForgeWorkers => {
                BifrostClusterSpec::three_servers_three_forge_workers()
            }
        }
    }

    /// Validate that a legacy pod count agrees with the named topology.
    ///
    /// # Errors
    ///
    /// Returns [`ClusterError::Topology`] when the count and topology differ.
    fn validate(self, pods: usize) -> Result<(), ClusterError> {
        let expected = self.spec().nodes.len();
        if pods == expected {
            Ok(())
        } else {
            Err(ClusterError::Topology {
                topology: self,
                expected,
                actual: pods,
            })
        }
    }
}

/// Return the complete Bifrost role topology used by capacity journeys.
#[must_use]
pub const fn full_bifrost_topology() -> BifrostTopology {
    BifrostTopology::ThreePod
}

/// Resources allocated to a node's Oracle test runtime.
#[derive(Debug, Clone, Default)]
pub struct TestOracleResources {
    /// Optional parent directory for the retained local spill root.
    pub spill_root: Option<PathBuf>,
}

/// Concrete role and identity descriptor for one Bifrost pod.
#[derive(Debug, Clone)]
pub struct BifrostNodeSpec {
    /// Stable physical node identity retained across restarts.
    pub node_id: NodeId,
    /// Roles enabled on this pod.
    pub roles: BTreeSet<BifrostRuntimeRole>,
    /// Optional Oracle-local resource placement.
    pub oracle: Option<TestOracleResources>,
}

/// Non-empty concrete topology descriptor used by journeys and calibration.
#[derive(Debug, Clone)]
pub struct BifrostClusterSpec {
    /// Ordered node descriptors with unique stable identities.
    pub nodes: Vec<BifrostNodeSpec>,
}

impl BifrostClusterSpec {
    /// Construct one mixed node.
    #[must_use]
    pub fn one_mixed() -> Self {
        Self::mixed(1)
    }

    /// Construct two mixed nodes for direct peer-transport journeys.
    #[must_use]
    pub fn two_mixed() -> Self {
        Self::mixed(2)
    }

    /// Construct three mixed nodes.
    #[must_use]
    pub fn three_mixed() -> Self {
        Self::mixed(3)
    }

    /// Construct three full Server processes for the role-separated lane.
    #[must_use]
    pub fn role_separated() -> Self {
        let mut spec = Self::mixed(3);
        // Retain an explicit Oracle resource marker so legacy lane reporting
        // can distinguish this named topology from the ordinary three-pod
        // capacity lane without reintroducing component-partial roles.
        for node in &mut spec.nodes {
            node.oracle = Some(TestOracleResources::default());
        }
        spec
    }

    /// Construct six mixed capacity nodes.
    #[must_use]
    pub fn six_capacity() -> Self {
        Self::mixed(6)
    }

    /// Construct one server process and three worker-only Forge processes.
    #[must_use]
    pub fn dedicated_forge_workers() -> Self {
        let mut nodes = vec![Self::node(
            1,
            [
                BifrostRuntimeRole::Scribe,
                BifrostRuntimeRole::Forge,
                BifrostRuntimeRole::Oracle,
            ],
        )];
        nodes.extend((2..=4).map(|id| Self::node(id, [BifrostRuntimeRole::Forge])));
        Self { nodes }
    }

    /// Construct the deterministic three-Server/three-ForgeWorker matrix.
    ///
    /// The first three nodes carry the complete public Server role set. The
    /// remaining three nodes carry only Forge, which the server harness maps
    /// to the closed `ForgeWorker` process role. Keeping the descriptor
    /// explicit prevents a mixed role from accidentally satisfying this lane.
    #[must_use]
    pub fn three_servers_three_forge_workers() -> Self {
        let mut nodes = (1..=3)
            .map(|id| {
                Self::node(
                    id,
                    [
                        BifrostRuntimeRole::Scribe,
                        BifrostRuntimeRole::Forge,
                        BifrostRuntimeRole::Oracle,
                    ],
                )
            })
            .collect::<Vec<_>>();
        nodes.extend((4..=6).map(|id| Self::node(id, [BifrostRuntimeRole::Forge])));
        Self { nodes }
    }

    /// Build `count` mixed nodes with deterministic identities.
    fn mixed(count: usize) -> Self {
        Self {
            nodes: (1..=count)
                .map(|index| {
                    Self::node(
                        u128::try_from(index).expect("node index fits u128"),
                        [
                            BifrostRuntimeRole::Scribe,
                            BifrostRuntimeRole::Forge,
                            BifrostRuntimeRole::Oracle,
                        ],
                    )
                })
                .collect(),
        }
    }

    /// Build one descriptor from deterministic identity and closed roles.
    fn node<const N: usize>(id: u128, roles: [BifrostRuntimeRole; N]) -> BifrostNodeSpec {
        BifrostNodeSpec {
            node_id: NodeId::new(uuid::Uuid::from_u128(id)),
            roles: roles.into_iter().collect(),
            oracle: None,
        }
    }

    /// Validate non-empty unique identities and non-empty role sets.
    ///
    /// # Errors
    ///
    /// Returns [`ClusterError::Resource`] when the topology is empty, contains
    /// a duplicate node ID, or declares a node without a runtime role.
    fn validate(&self) -> Result<(), ClusterError> {
        if self.nodes.is_empty() {
            return Err(ClusterError::Resource(
                "cluster must contain at least one node".to_owned(),
            ));
        }
        let mut ids = BTreeSet::new();
        for node in &self.nodes {
            if node.roles.is_empty() {
                return Err(ClusterError::Resource(format!(
                    "node {} has no Bifrost role",
                    node.node_id.as_uuid()
                )));
            }
            if !ids.insert(node.node_id) {
                return Err(ClusterError::Resource(format!(
                    "duplicate node identity {}",
                    node.node_id.as_uuid()
                )));
            }
            let forge_worker =
                node.roles.len() == 1 && node.roles.contains(&BifrostRuntimeRole::Forge);
            if !forge_worker && !has_full_server_roles(&node.roles) {
                return Err(ClusterError::Resource(format!(
                    "node {} must declare all Server roles or Forge only",
                    node.node_id.as_uuid()
                )));
            }
        }
        Ok(())
    }
}

/// Transport fault kind retained by the test cluster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OracleFault {
    /// Reject matching peer or tail operations as unavailable.
    Drop,
    /// Delay matching operations by the bounded duration.
    Delay(Duration),
    /// Corrupt the matching response after transport receipt.
    Corrupt,
}

/// Scoped peer/tail operation key used by transport adapters and journeys.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct OracleFaultKey {
    /// Optional destination node; absent applies to every destination.
    pub node_id: Option<NodeId>,
    /// Closed adapter operation name such as `peer.execute` or `tail.page`.
    pub operation: String,
}

/// Shared fault controls consulted by test-tier transport adapters.
#[derive(Debug, Default)]
pub struct OracleFaultController {
    /// Active scoped faults.
    faults: Mutex<BTreeMap<OracleFaultKey, OracleFault>>,
}

impl OracleFaultController {
    /// Install or replace one scoped fault.
    pub fn set(&self, key: OracleFaultKey, fault: OracleFault) {
        if let Ok(mut faults) = self.faults.lock() {
            faults.insert(key, fault);
        }
    }

    /// Remove all active peer and tail faults.
    pub fn clear(&self) {
        if let Ok(mut faults) = self.faults.lock() {
            faults.clear();
        }
    }

    /// Return the exact or wildcard fault for one destination operation.
    #[must_use]
    pub fn fault_for(&self, node_id: NodeId, operation: &str) -> Option<OracleFault> {
        self.faults.lock().ok().and_then(|faults| {
            faults
                .get(&OracleFaultKey {
                    node_id: Some(node_id),
                    operation: operation.to_owned(),
                })
                .or_else(|| {
                    faults.get(&OracleFaultKey {
                        node_id: None,
                        operation: operation.to_owned(),
                    })
                })
                .cloned()
        })
    }
}

/// One membership row returned by typed cluster inspection.
#[derive(Debug, Clone)]
pub struct OracleMembershipInspection {
    /// Stable node identity.
    pub node_id: NodeId,
    /// Independently fenced Bifrost role.
    pub role: BifrostRuntimeRole,
    /// Current role fencing token.
    pub fencing_token: u64,
    /// Whether the role advertised readiness.
    pub ready: bool,
}

/// Typed inspection snapshot exposed to operational journeys.
#[derive(Debug, Clone, Default)]
pub struct OracleInspection {
    /// Durable Scribe and Oracle membership rows.
    pub memberships: Vec<OracleMembershipInspection>,
    /// Active durable Oracle admission leases.
    pub active_leases: u64,
    /// Sum of durable admission accounting slots.
    pub slots_in_use: u64,
    /// Live-tail fences retained across every Scribe process.
    pub active_tail_fences: u64,
    /// Number of durable audit rows observed across tenants.
    pub audit_rows: u64,
    /// Durable Oracle read-decision audit rows observed across tenants.
    pub read_audit_rows: u64,
    /// Forge tasks still holding a durable claim.
    pub forge_active_claims: u64,
    /// Distinct Forge attempts still in a non-terminal claimed execution state.
    pub forge_active_attempts: u64,
    /// Forge tasks retaining historical attempt identifiers for diagnostics.
    pub forge_historical_attempts: u64,
    /// Forge tasks in a terminal state with durable evidence.
    pub forge_terminal_tasks: u64,
    /// Metric families present in the production recorder snapshot.
    pub metric_families: BTreeSet<String>,
    /// Finished span names present in the production tracing capture.
    pub span_names: BTreeSet<String>,
}

/// Concrete result of stopping every server/listener in a test cluster.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClusterShutdownInspection {
    /// Every server stop operation completed successfully.
    pub servers_stopped: bool,
    /// Every bound HTTP/gRPC listener was joined by its server stop operation.
    pub listeners_stopped: bool,
    /// Scribe queued commands remaining after every server drain completed.
    pub scribe_queued: u64,
    /// Scribe append commands remaining after every server drain completed.
    pub scribe_inflight: u64,
    /// Persistent WAL streams remaining after server shutdown.
    pub scribe_wal_streams: u64,
    /// Durable Oracle leases remaining at the final pre-stop drain checkpoint.
    pub oracle_leases: u64,
    /// Durable Oracle admission slots remaining at the final pre-stop drain checkpoint.
    pub oracle_slots: u64,
    /// Forge claims remaining at the final pre-stop drain checkpoint.
    pub forge_active_claims: u64,
    /// Forge attempts remaining at the final pre-stop drain checkpoint.
    pub forge_active_attempts: u64,
    /// Supervised server tasks retained after every server owner is dropped.
    pub supervised_tasks: u64,
}

/// One parsed production Prometheus series.
#[derive(Debug, Clone, PartialEq)]
pub struct OracleMetricSample {
    /// Metric family with histogram suffixes normalized to their owner family.
    pub family: String,
    /// Exact label keys and values emitted by production instrumentation.
    pub labels: BTreeMap<String, String>,
    /// Current sample value or delta value.
    pub value: f64,
}

/// Snapshot marker used to isolate one serialized journey's telemetry.
#[derive(Debug, Clone)]
pub struct OracleTelemetryCheckpoint {
    /// Production metric values keyed by rendered series.
    metrics: BTreeMap<String, f64>,
    /// Finished span count before the journey.
    spans: usize,
}

/// Production telemetry emitted after one checkpoint.
#[derive(Debug, Clone, Default)]
pub struct OracleTelemetryDelta {
    /// Changed production metric series.
    pub metrics: Vec<OracleMetricSample>,
    /// Finished production-pipeline spans.
    pub spans: Vec<CapturedSpan>,
}

/// Read-only handle over the one process-installed production telemetry stack.
#[derive(Debug, Clone)]
pub struct OracleTelemetryCapture {
    /// Existing server Prometheus recorder handle.
    metrics: PrometheusHandle,
    /// Finished-span handle from the production-shaped OTel provider.
    traces: TestTraceCapture,
}

impl OracleTelemetryCapture {
    /// Capture the production recorder and exporter positions before a journey.
    #[must_use]
    pub fn checkpoint(&self) -> OracleTelemetryCheckpoint {
        OracleTelemetryCheckpoint {
            metrics: rendered_values(&self.metrics.render()),
            spans: self.traces.checkpoint(),
        }
    }

    /// Return changed metric series and newly finished spans.
    ///
    /// # Errors
    ///
    /// Returns [`ClusterError::Telemetry`] when a changed Prometheus line has
    /// invalid label or numeric syntax.
    pub fn delta_since(
        &self,
        checkpoint: &OracleTelemetryCheckpoint,
    ) -> Result<OracleTelemetryDelta, ClusterError> {
        let rendered = self.metrics.render();
        let current = rendered_values(&rendered);
        let mut metrics = Vec::new();
        for (series, value) in current {
            let prior = checkpoint.metrics.get(&series).copied().unwrap_or_default();
            if value != prior {
                let mut sample = parse_metric_sample(&series, value)?;
                sample.value = value - prior;
                metrics.push(sample);
            }
        }
        Ok(OracleTelemetryDelta {
            metrics,
            spans: self.traces.finished_since(checkpoint.spans),
        })
    }

    /// Return the current production Prometheus series as absolute samples.
    ///
    /// Benchmark polling uses this while a query owns its admission and memory
    /// guards so peak gauges are observed before stream cleanup returns them to
    /// zero.
    ///
    /// # Errors
    ///
    /// Returns [`ClusterError::Telemetry`] when a rendered Prometheus series
    /// has invalid label or numeric syntax.
    pub fn snapshot(&self) -> Result<Vec<OracleMetricSample>, ClusterError> {
        rendered_values(&self.metrics.render())
            .into_iter()
            .map(|(series, value)| parse_metric_sample(&series, value))
            .collect()
    }
}

/// Process-global telemetry resources kept alive for every cluster node.
struct ProcessTelemetry {
    /// Guard retaining the one SDK tracer provider.
    guard: Arc<TelemetryGuard>,
    /// Forge report capture backed by the same process recorder and provider.
    forge_capture: BifrostTelemetryCapture,
}

/// One telemetry installation per test process.
static PROCESS_TELEMETRY: OnceLock<ProcessTelemetry> = OnceLock::new();
/// Serializes the check-and-install window for process telemetry.
static PROCESS_TELEMETRY_INIT: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Install or borrow the process production telemetry stack.
///
/// # Errors
///
/// Returns [`ClusterError::Telemetry`] if either process-global recorder or
/// subscriber was installed by an unrelated owner first.
fn process_telemetry() -> Result<&'static ProcessTelemetry, ClusterError> {
    if let Some(telemetry) = PROCESS_TELEMETRY.get() {
        return Ok(telemetry);
    }
    let _init_guard = PROCESS_TELEMETRY_INIT
        .lock()
        .map_err(|_| ClusterError::Telemetry("process telemetry lock poisoned".to_owned()))?;
    if let Some(telemetry) = PROCESS_TELEMETRY.get() {
        return Ok(telemetry);
    }
    let (guard, traces) = wyrd_telemetry::init_test_capture(TelemetryConfig {
        service_name: Some("wyrd-test-cluster".to_owned()),
        ..TelemetryConfig::default()
    })
    .map_err(|error| ClusterError::Telemetry(error.to_string()))?;
    let metrics = install_recorder().map_err(|error| ClusterError::Telemetry(error.to_string()))?;
    let forge_capture = BifrostTelemetryCapture::new(metrics.clone(), traces.clone());
    let _ = PROCESS_TELEMETRY.set(ProcessTelemetry {
        guard: Arc::new(guard),
        forge_capture,
    });
    PROCESS_TELEMETRY
        .get()
        .ok_or_else(|| ClusterError::Telemetry("process telemetry installation raced".to_owned()))
}

/// Borrow the one process-installed production telemetry runtime for Forge tests.
///
/// # Errors
/// Returns a telemetry error when process installation fails or another global
/// recorder/subscriber already owns the process.
pub fn shared_process_telemetry_for_test()
-> Result<(Arc<TelemetryGuard>, BifrostTelemetryCapture), ClusterError> {
    let process = process_telemetry()?;
    Ok((Arc::clone(&process.guard), process.forge_capture.clone()))
}

/// Persistent local resources for one restartable node slot.
struct NodeResources {
    /// Original node descriptor.
    spec: BifrostNodeSpec,
    /// Retained Scribe WAL root, absent on query-only nodes.
    wal_root: Option<Arc<tempfile::TempDir>>,
    /// Retained Oracle/Forge spill root, absent on Scribe-only nodes.
    spill_root: Option<Arc<tempfile::TempDir>>,
    /// Fixed public HTTP bind retained across a restart.
    http_addr: std::net::SocketAddr,
    /// Fixed private/public gRPC bind retained across a restart.
    grpc_addr: std::net::SocketAddr,
    /// Closed production process role derived from the component set.
    process_role: ForgeProcessRole,
}

/// Optional Forge supervision controls shared by every node in one cluster.
struct ForgeHarnessOptions {
    /// Observer receiving successful worker completions.
    completion_observer: Option<ForgeWorkerCompletionObserver>,
    /// Complete Forge limit profile supplied by a journey.
    config: Option<ForgeConfig>,
    /// Enable the deterministic uncertain-commit catalog wrapper.
    inject_uncertainty: bool,
    /// Scheduler interval used by bound server roles.
    interval: Duration,
}

impl Default for ForgeHarnessOptions {
    fn default() -> Self {
        Self {
            completion_observer: None,
            config: None,
            inject_uncertainty: false,
            interval: Duration::from_secs(60),
        }
    }
}

/// Errors raised while constructing, inspecting, or stopping a test cluster.
#[derive(Debug, Error)]
pub enum ClusterError {
    /// The requested pod count does not match the selected topology.
    #[error("invalid {topology:?} topology: expected {expected} pods, got {actual}")]
    Topology {
        /// Selected topology.
        topology: BifrostTopology,
        /// Required pod count.
        expected: usize,
        /// Requested pod count.
        actual: usize,
    },
    /// A shared Postgres, catalog, object-store, WAL, or spill resource failed.
    #[error("cluster resource setup failed: {0}")]
    Resource(String),
    /// A server failed to start or bind.
    #[error("cluster server failed: {0}")]
    Server(#[from] WyrdTestServerError),
    /// A server failed during explicit teardown.
    #[error("cluster shutdown failed: {0}")]
    Shutdown(String),
    /// Production telemetry could not be installed or parsed.
    #[error("cluster telemetry failed: {0}")]
    Telemetry(String),
}

/// Configured local roots retained after an abrupt test-node termination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetainedNodeRoots {
    /// Scribe WAL root retained for replay.
    pub wal_root: Option<PathBuf>,
    /// Oracle/Forge spill root retained for replacement startup.
    pub spill_root: Option<PathBuf>,
    /// HTTP address proven closed by abrupt termination.
    pub previous_http_addr: std::net::SocketAddr,
    /// gRPC address proven closed by abrupt termination.
    pub previous_grpc_addr: std::net::SocketAddr,
    /// Writer epoch retained as the restart fence baseline.
    pub previous_writer_epoch: Option<i64>,
}

/// Identity evidence returned after a terminated node is rebound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeRestartEvidence {
    /// Stable logical node identity preserved across replacement.
    pub node_id: NodeId,
    /// New HTTP listener address.
    pub http_addr: std::net::SocketAddr,
    /// New gRPC listener address.
    pub grpc_addr: std::net::SocketAddr,
    /// Previous writer epoch, when the node owned Scribe.
    pub previous_writer_epoch: Option<i64>,
    /// Replacement writer epoch, strictly greater for Scribe nodes.
    pub writer_epoch: Option<i64>,
}

/// A real multi-pod Bifrost test topology with restartable node slots.
pub struct WyrdTestCluster {
    /// Stable node-keyed server slots; stopped nodes retain `None`.
    servers: BTreeMap<NodeId, Option<WyrdTestServer>>,
    /// Stable roots and role descriptors retained across server replacement.
    nodes: BTreeMap<NodeId, NodeResources>,
    /// Shared real Postgres fixture.
    fixture: Arc<PgFixture>,
    /// Shared real storage handle.
    storage: Arc<StorageHandle>,
    /// Shared local storage lifetime guard.
    storage_root: Arc<ClusterStorageRoot>,
    /// Shared legacy Iceberg catalog.
    catalog: Arc<WyrdCatalog>,
    /// Shared Redux Bifrost catalog.
    redux_catalog: Arc<BifrostCatalog>,
    /// Named topology retained for existing lane selection.
    topology: BifrostTopology,
    /// Builder fault configuration retained by the cluster.
    wal_sync_delay: Duration,
    /// Optional deterministic Scribe admission configuration.
    scribe_admission: Option<AdmissionConfig>,
    /// Optional node receiving the deterministic admission override.
    scribe_admission_node: Option<NodeId>,
    /// Scoped transport fault state.
    faults: OracleFaultController,
    /// Read-only process telemetry handle.
    telemetry: BifrostTelemetryCapture,
    /// Valid SYSTEM_OWNER Service credential retained across node restarts.
    oracle_peer_credentials: Arc<dyn OraclePeerCredentials>,
    /// Optional retained TLS fixture directory and paths for real peer transport.
    oracle_peer_tls: Option<(Arc<tempfile::TempDir>, TestOraclePeerTls)>,
    /// Shared observer for supervised Forge worker completions.
    forge_completion_observer: Option<ForgeWorkerCompletionObserver>,
    /// Shared uncertainty-injection catalog wrapper, when enabled.
    commit_uncertainty_catalog: Option<Arc<CommitUncertaintyCatalog>>,
    /// Optional complete Forge configuration for role fixtures.
    forge_config: Option<ForgeConfig>,
    /// Interval used by supervised scheduler roles.
    forge_interval: Duration,
}

/// Lifetime owner for temporary or caller-declared local cluster storage.
enum ClusterStorageRoot {
    /// Automatically removed test-only storage.
    Temporary(tempfile::TempDir),
    /// Persistent benchmark storage rooted at the declared absolute path.
    Dedicated(std::path::PathBuf),
}

impl ClusterStorageRoot {
    /// Return the exact runtime local-filesystem root.
    fn path(&self) -> &std::path::Path {
        match self {
            Self::Temporary(root) => root.path(),
            Self::Dedicated(root) => root,
        }
    }
}

/// Stable read-only view of all currently running server slots.
pub struct ServerView<'a> {
    /// Running servers in stable node order.
    items: Vec<&'a WyrdTestServer>,
    /// Iterator cursor for direct `for`/adapter use.
    cursor: usize,
}

impl<'a> std::ops::Deref for ServerView<'a> {
    type Target = [&'a WyrdTestServer];

    fn deref(&self) -> &Self::Target {
        &self.items
    }
}

impl<'a> Iterator for ServerView<'a> {
    type Item = &'a WyrdTestServer;

    fn next(&mut self) -> Option<Self::Item> {
        let server = self.items.get(self.cursor).copied();
        self.cursor = self.cursor.saturating_add(1);
        server
    }
}

impl WyrdTestCluster {
    /// Abruptly terminate one node while retaining its configured local roots.
    ///
    /// # Errors
    ///
    /// Returns an error when the node is unknown, already stopped, or its test
    /// supervisor cannot be terminated.
    pub async fn terminate_node_abruptly_for_test(
        &mut self,
        node_id: NodeId,
    ) -> Result<RetainedNodeRoots, ClusterError> {
        let server = self
            .servers
            .get_mut(&node_id)
            .ok_or_else(|| ClusterError::Resource(format!("unknown node {}", node_id.as_uuid())))?
            .take()
            .ok_or_else(|| {
                ClusterError::Resource(format!("node {} is already stopped", node_id.as_uuid()))
            })?;
        let previous_writer_epoch = server
            .bifrost_scribe()
            .map(|scribe| scribe.writer_epoch_for_test());
        server
            .terminate_abruptly_for_test()
            .await
            .map_err(|error| ClusterError::Shutdown(error.to_string()))?;
        let resources = self
            .nodes
            .get(&node_id)
            .ok_or_else(|| ClusterError::Resource(format!("unknown node {}", node_id.as_uuid())))?;
        Ok(RetainedNodeRoots {
            wal_root: resources
                .wal_root
                .as_ref()
                .map(|root| root.path().to_path_buf()),
            spill_root: resources
                .spill_root
                .as_ref()
                .map(|root| root.path().to_path_buf()),
            previous_http_addr: resources.http_addr,
            previous_grpc_addr: resources.grpc_addr,
            previous_writer_epoch,
        })
    }

    /// Restart an abruptly terminated node on fresh addresses and retained roots.
    ///
    /// # Errors
    ///
    /// Returns an error when the node is running, the retained roots no longer
    /// match the configured roots, or replacement startup fails.
    pub async fn restart_terminated_node_at_new_address(
        &mut self,
        node_id: NodeId,
        roots: RetainedNodeRoots,
    ) -> Result<NodeRestartEvidence, ClusterError> {
        let resources = self
            .nodes
            .get(&node_id)
            .ok_or_else(|| ClusterError::Resource(format!("unknown node {}", node_id.as_uuid())))?;
        let configured = RetainedNodeRoots {
            wal_root: resources
                .wal_root
                .as_ref()
                .map(|root| root.path().to_path_buf()),
            previous_http_addr: roots.previous_http_addr,
            previous_grpc_addr: roots.previous_grpc_addr,
            previous_writer_epoch: roots.previous_writer_epoch,
            spill_root: resources
                .spill_root
                .as_ref()
                .map(|root| root.path().to_path_buf()),
        };
        if configured != roots {
            return Err(ClusterError::Resource(
                "retained node roots do not match configured roots".to_owned(),
            ));
        }
        self.restart_node_at_new_address(node_id).await?;
        let resources = self
            .nodes
            .get(&node_id)
            .ok_or_else(|| ClusterError::Resource(format!("unknown node {}", node_id.as_uuid())))?;
        let replacement = self.servers.get(&node_id).and_then(Option::as_ref);
        let writer_epoch = replacement
            .and_then(WyrdTestServer::bifrost_scribe)
            .map(|scribe| scribe.writer_epoch_for_test());
        if resources.http_addr == roots.previous_http_addr
            || resources.grpc_addr == roots.previous_grpc_addr
            || writer_epoch
                .zip(roots.previous_writer_epoch)
                .is_some_and(|(new, old)| new <= old)
        {
            return Err(ClusterError::Resource(
                "replacement node did not advance address and writer fences".to_owned(),
            ));
        }
        Ok(NodeRestartEvidence {
            node_id,
            http_addr: resources.http_addr,
            grpc_addr: resources.grpc_addr,
            previous_writer_epoch: roots.previous_writer_epoch,
            writer_epoch,
        })
    }

    /// Connect to one real Oracle server through the cluster's configured TLS trust.
    ///
    /// # Errors
    /// Returns a resource error when TLS is disabled, the node is absent, or
    /// certificate/DNS-authenticated channel establishment fails.
    pub async fn oracle_peer_channel(
        &self,
        index: usize,
    ) -> Result<wyrd_tonic::tonic::transport::Channel, ClusterError> {
        let (_, tls) = self
            .oracle_peer_tls
            .as_ref()
            .ok_or_else(|| ClusterError::Resource("Oracle peer TLS is not enabled".to_owned()))?;
        let address = self
            .server(index)
            .and_then(WyrdTestServer::grpc_url)
            .ok_or_else(|| ClusterError::Resource("Oracle peer endpoint is absent".to_owned()))?;
        let ca = std::fs::read(&tls.ca_path)
            .map_err(|error| ClusterError::Resource(error.to_string()))?;
        wyrd_tonic::transport::authenticated_tls_endpoint(address, &ca, tls.server_name.clone())
            .map_err(|error| ClusterError::Resource(error.to_string()))?
            .connect()
            .await
            .map_err(|error| ClusterError::Resource(error.to_string()))
    }

    /// Return the cluster-owned least-privilege Oracle Service bearer.
    ///
    /// # Errors
    /// Returns a resource error when token exchange fails.
    pub async fn oracle_peer_bearer(&self, force_refresh: bool) -> Result<String, ClusterError> {
        self.oracle_peer_credentials
            .bearer(force_refresh)
            .await
            .map_err(|error| ClusterError::Resource(error.to_string()))
    }

    /// Builds the same registry-backed TLS peer transport used by server boot.
    ///
    /// # Errors
    /// Returns a resource error when TLS is disabled, the leader is absent, or
    /// the retained CA fixture cannot be read.
    pub fn oracle_peer_transport(
        &self,
        leader_index: usize,
    ) -> Result<TonicOraclePeerTransport, ClusterError> {
        let (_, tls) = self
            .oracle_peer_tls
            .as_ref()
            .ok_or_else(|| ClusterError::Resource("Oracle peer TLS is not enabled".to_owned()))?;
        let server = self
            .server(leader_index)
            .ok_or_else(|| ClusterError::Resource("Oracle leader is absent".to_owned()))?;
        let peer =
            server.state().oracle_peer.as_ref().ok_or_else(|| {
                ClusterError::Resource("Oracle leader runtime is absent".to_owned())
            })?;
        let ca = std::fs::read(&tls.ca_path)
            .map_err(|error| ClusterError::Resource(error.to_string()))?;
        Ok(TonicOraclePeerTransport::with_credentials_and_tls(
            peer.cluster(),
            Arc::clone(&self.oracle_peer_credentials),
            OraclePeerTls::new(ca, tls.server_name.clone()),
        ))
    }

    /// Refreshes every running node's authoritative immutable membership cut.
    ///
    /// # Errors
    /// Returns a resource error when any registry refresh fails.
    pub async fn refresh_oracle_snapshots(&self) -> Result<(), ClusterError> {
        for server in self.servers.values().flatten() {
            if let Some(peer) = &server.state().oracle_peer {
                peer.cluster()
                    .refresh_snapshot()
                    .await
                    .map_err(|error| ClusterError::Resource(error.to_string()))?;
            }
        }
        Ok(())
    }

    /// Start a legacy named topology over shared real dependencies.
    ///
    /// # Errors
    ///
    /// Returns an error when the count disagrees with the topology or any
    /// shared resource and independently bound server cannot start.
    pub async fn start(pods: usize, topology: BifrostTopology) -> Result<Self, ClusterError> {
        topology.validate(pods)?;
        Self::start_spec(topology.spec()).await
    }

    /// Start a concrete role topology over shared real dependencies.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid descriptor, resource failure, telemetry
    /// installation failure, or node bind failure. Nodes already started are
    /// shut down before the error returns.
    pub async fn start_spec(spec: BifrostClusterSpec) -> Result<Self, ClusterError> {
        Self::start_spec_with_options(spec, Duration::ZERO, None, false, false).await
    }

    /// Start real Wyrd servers whose Oracle membership and gRPC listeners use TLS.
    ///
    /// # Errors
    /// Returns the same topology, resource, TLS-fixture, or server boot errors as
    /// [`Self::start_spec`].
    pub async fn start_spec_with_oracle_peer_tls(
        spec: BifrostClusterSpec,
    ) -> Result<Self, ClusterError> {
        Self::start_spec_with_options(spec, Duration::ZERO, None, true, false).await
    }

    /// Starts a TLS topology while retaining the final node as an unbooted slot.
    ///
    /// # Errors
    /// Returns the same resource, topology, and boot errors as
    /// [`Self::start_spec_with_oracle_peer_tls`].
    pub async fn start_spec_with_oracle_peer_tls_delayed_last(
        spec: BifrostClusterSpec,
    ) -> Result<Self, ClusterError> {
        Self::start_spec_with_options(spec, Duration::ZERO, None, true, true).await
    }

    /// Start a named topology with an explicit WAL fsync delay.
    ///
    /// # Errors
    ///
    /// Returns the same setup and bind errors as [`Self::start_spec`].
    pub async fn start_with_wal_sync_delay(
        pods: usize,
        topology: BifrostTopology,
        wal_sync_delay: Duration,
    ) -> Result<Self, ClusterError> {
        topology.validate(pods)?;
        Self::start_spec_with_options(topology.spec(), wal_sync_delay, None, false, false).await
    }

    /// Start a named topology with deterministic Scribe admission bounds.
    ///
    /// # Errors
    ///
    /// Returns the same setup and bind errors as [`Self::start_spec`].
    pub async fn start_with_admission(
        pods: usize,
        topology: BifrostTopology,
        admission: AdmissionConfig,
    ) -> Result<Self, ClusterError> {
        topology.validate(pods)?;
        Self::start_spec_with_options(
            topology.spec(),
            Duration::ZERO,
            Some(admission),
            false,
            false,
        )
        .await
    }

    /// Start an explicit descriptor with admission pressure on one Server.
    ///
    /// # Errors
    /// Returns a topology, resource, or role-supervision error when the
    /// selected node cannot be booted with the requested bounds.
    pub async fn start_spec_with_admission_on_node(
        spec: BifrostClusterSpec,
        node_index: usize,
        admission: AdmissionConfig,
    ) -> Result<Self, ClusterError> {
        Self::start_spec_with_all_options(
            spec,
            Duration::ZERO,
            Some(admission),
            Some(node_index),
            false,
            false,
            (ForgeHarnessOptions::default(), None),
        )
        .await
    }

    /// Start an explicit descriptor with a shared Forge completion observer.
    ///
    /// The observer is passive production-worker evidence used by deterministic
    /// publication waits; it does not alter Forge scheduling or task state.
    ///
    /// # Errors
    /// Returns the same topology, resource, and role-supervision errors as
    /// [`Self::start_spec`].
    pub async fn start_spec_with_forge_completion_observer(
        spec: BifrostClusterSpec,
    ) -> Result<Self, ClusterError> {
        Self::start_spec_with_all_options(
            spec,
            Duration::ZERO,
            None,
            None,
            false,
            false,
            (
                ForgeHarnessOptions {
                    completion_observer: Some(ForgeWorkerCompletionObserver::new()),
                    ..ForgeHarnessOptions::default()
                },
                None,
            ),
        )
        .await
    }

    /// Start an observed topology against one caller-declared filesystem root.
    ///
    /// # Errors
    /// Returns the same topology, resource, and role-supervision errors as
    /// [`Self::start_spec_with_forge_completion_observer`], plus filesystem
    /// creation or canonicalization failures for the dedicated root.
    pub async fn start_spec_with_forge_observer_and_storage_root(
        spec: BifrostClusterSpec,
        storage_root: std::path::PathBuf,
    ) -> Result<Self, ClusterError> {
        Self::start_spec_with_all_options(
            spec,
            Duration::ZERO,
            None,
            None,
            false,
            false,
            (
                ForgeHarnessOptions {
                    completion_observer: Some(ForgeWorkerCompletionObserver::new()),
                    ..ForgeHarnessOptions::default()
                },
                Some(storage_root),
            ),
        )
        .await
    }

    /// Return the canonical local-filesystem root used by the live cluster.
    #[must_use]
    pub fn storage_root(&self) -> &std::path::Path {
        self.storage_root.path()
    }

    /// Start an explicit descriptor with admission pressure and Forge observer.
    ///
    /// # Errors
    /// Returns the same topology, resource, and role-supervision errors as
    /// [`Self::start_spec_with_admission_on_node`].
    pub async fn start_spec_with_admission_and_forge_completion_observer_on_node(
        spec: BifrostClusterSpec,
        node_index: usize,
        admission: AdmissionConfig,
    ) -> Result<Self, ClusterError> {
        Self::start_spec_with_all_options(
            spec,
            Duration::ZERO,
            Some(admission),
            Some(node_index),
            false,
            false,
            (
                ForgeHarnessOptions {
                    completion_observer: Some(ForgeWorkerCompletionObserver::new()),
                    ..ForgeHarnessOptions::default()
                },
                None,
            ),
        )
        .await
    }

    /// Start one server process with three dedicated Forge workers.
    ///
    /// # Errors
    /// Returns a topology, resource, or role-supervision error.
    pub async fn start_with_dedicated_forge_workers() -> Result<Self, ClusterError> {
        Self::start_with_dedicated_forge_workers_with_options(None, None, None).await
    }

    /// Start dedicated workers with deterministic uncertain-commit injection.
    ///
    /// # Errors
    /// Returns a topology, resource, or role-supervision error.
    pub async fn start_with_dedicated_forge_workers_with_uncertainty_for_test()
    -> Result<Self, ClusterError> {
        Self::start_with_dedicated_forge_workers_with_options(None, None, Some(true)).await
    }

    /// Start dedicated workers with a complete validated Forge config.
    ///
    /// # Errors
    /// Returns a topology, resource, or role-supervision error.
    pub async fn start_with_dedicated_forge_workers_with_config_for_test(
        config: ForgeConfig,
    ) -> Result<Self, ClusterError> {
        Self::start_with_dedicated_forge_workers_with_options(Some(config), None, None).await
    }

    /// Start an embedded `all` process with a completion observer.
    ///
    /// # Errors
    /// Returns a topology, resource, or role-supervision error.
    pub async fn start_with_embedded_forge_observer() -> Result<Self, ClusterError> {
        let observer = ForgeWorkerCompletionObserver::new();
        Self::start_spec_with_all_options(
            BifrostClusterSpec::one_mixed(),
            Duration::ZERO,
            None,
            None,
            false,
            false,
            (
                ForgeHarnessOptions {
                    completion_observer: Some(observer),
                    ..ForgeHarnessOptions::default()
                },
                None,
            ),
        )
        .await
    }

    async fn start_with_dedicated_forge_workers_with_options(
        forge_config: Option<ForgeConfig>,
        forge_interval: Option<Duration>,
        uncertainty: Option<bool>,
    ) -> Result<Self, ClusterError> {
        let observer = ForgeWorkerCompletionObserver::new();
        Self::start_spec_with_all_options(
            BifrostClusterSpec::dedicated_forge_workers(),
            Duration::ZERO,
            None,
            None,
            false,
            false,
            (
                ForgeHarnessOptions {
                    completion_observer: Some(observer),
                    config: forge_config,
                    inject_uncertainty: uncertainty.unwrap_or(false),
                    interval: forge_interval.unwrap_or(Duration::from_secs(60)),
                },
                None,
            ),
        )
        .await
    }

    /// Return the shared Forge completion observer, when configured.
    #[must_use]
    pub fn forge_completion_observer(&self) -> Option<ForgeWorkerCompletionObserver> {
        self.forge_completion_observer.clone()
    }

    /// Wake the configured production Forge scheduler without manufacturing work.
    pub fn request_forge_scheduler_pass_for_test(&self) {
        for server in self.servers() {
            server.request_forge_scheduler_pass_for_test();
        }
    }

    /// Return the uncertainty catalog control, when configured.
    #[must_use]
    pub fn commit_uncertainty_catalog(&self) -> Option<Arc<CommitUncertaintyCatalog>> {
        self.commit_uncertainty_catalog.clone()
    }

    /// Build shared dependencies, retained roots, and every node slot.
    async fn start_spec_with_options(
        spec: BifrostClusterSpec,
        wal_sync_delay: Duration,
        scribe_admission: Option<AdmissionConfig>,
        enable_oracle_peer_tls: bool,
        delay_last_node: bool,
    ) -> Result<Self, ClusterError> {
        Self::start_spec_with_all_options(
            spec,
            wal_sync_delay,
            scribe_admission,
            None,
            enable_oracle_peer_tls,
            delay_last_node,
            (ForgeHarnessOptions::default(), None),
        )
        .await
    }

    /// Build a topology with production Forge role supervision controls.
    async fn start_spec_with_all_options(
        spec: BifrostClusterSpec,
        wal_sync_delay: Duration,
        scribe_admission: Option<AdmissionConfig>,
        scribe_admission_node: Option<usize>,
        enable_oracle_peer_tls: bool,
        delay_last_node: bool,
        harness: (ForgeHarnessOptions, Option<std::path::PathBuf>),
    ) -> Result<Self, ClusterError> {
        let (options, dedicated_storage_root) = harness;
        spec.validate()?;
        let process = process_telemetry()?;
        let fixture = Arc::new(
            PgFixture::start()
                .await
                .map_err(|error| ClusterError::Resource(error.to_string()))?,
        );
        let tenant = fixture.data_tenant_id();
        let mut conn = fixture
            .tenant_conn()
            .await
            .map_err(|error| ClusterError::Resource(error.to_string()))?;
        seed_builtin_roles_for_tenant(&mut conn, tenant)
            .await
            .map_err(|error| ClusterError::Resource(error.to_string()))?;
        conn.commit()
            .await
            .map_err(|error| ClusterError::Resource(error.to_string()))?;

        let storage_root = Arc::new(match dedicated_storage_root {
            Some(root) => {
                std::fs::create_dir_all(&root)
                    .map_err(|error| ClusterError::Resource(error.to_string()))?;
                ClusterStorageRoot::Dedicated(
                    root.canonicalize()
                        .map_err(|error| ClusterError::Resource(error.to_string()))?,
                )
            }
            None => ClusterStorageRoot::Temporary(
                tempfile::tempdir().map_err(|error| ClusterError::Resource(error.to_string()))?,
            ),
        });
        let storage = StorageHandle::from_settings(StorageSettings {
            backend: BackendConfig::Local {
                root: storage_root.path().to_path_buf(),
            },
            require_encryption: false,
            presign_ttl: Duration::from_secs(600),
            part_size_bytes: 16 * 1024 * 1024,
            multipart_threshold_bytes: 100 * 1024 * 1024,
            public_base_url: Some("https://wyrd.test".to_owned()),
        })
        .await
        .map_err(|error| ClusterError::Resource(error.to_string()))?;
        let catalog: Arc<WyrdCatalog> = test_catalog(&fixture, &storage).await?;
        let redux_catalog: Arc<BifrostCatalog> = test_redux_catalog(&fixture, &storage).await?;
        let commit_uncertainty_catalog = options
            .inject_uncertainty
            .then(|| CommitUncertaintyCatalog::new(redux_catalog.iceberg_catalog()));
        let oracle_peer_credentials =
            provision_oracle_peer_credentials(Arc::clone(&fixture)).await?;
        let oracle_peer_tls = if enable_oracle_peer_tls {
            let root = Arc::new(
                tempfile::tempdir().map_err(|error| ClusterError::Resource(error.to_string()))?,
            );
            let certificate_path = root.path().join("oracle-peer-cert.pem");
            let private_key_path = root.path().join("oracle-peer-key.pem");
            let ca_path = root.path().join("oracle-peer-ca.pem");
            std::fs::write(
                &certificate_path,
                include_bytes!("fixtures/oracle-peer-cert.pem"),
            )
            .map_err(|error| ClusterError::Resource(error.to_string()))?;
            std::fs::write(
                &private_key_path,
                include_bytes!("fixtures/oracle-peer-key.pem"),
            )
            .map_err(|error| ClusterError::Resource(error.to_string()))?;
            std::fs::write(&ca_path, include_bytes!("fixtures/oracle-peer-ca.pem"))
                .map_err(|error| ClusterError::Resource(error.to_string()))?;
            Some((
                root,
                TestOraclePeerTls {
                    certificate_path,
                    private_key_path,
                    ca_path,
                    server_name: "localhost".to_owned(),
                },
            ))
        } else {
            None
        };
        let topology = classify_topology(&spec);
        let scribe_admission_node =
            scribe_admission_node.and_then(|index| spec.nodes.get(index).map(|node| node.node_id));
        let node_count = spec.nodes.len();
        let mut nodes = BTreeMap::new();
        for node in spec.nodes {
            let wal_root = if node.roles.contains(&BifrostRuntimeRole::Scribe) {
                Some(Arc::new(tempfile::tempdir().map_err(|error| {
                    ClusterError::Resource(error.to_string())
                })?))
            } else {
                None
            };
            let spill_root = if node.roles.contains(&BifrostRuntimeRole::Oracle)
                || node.roles.contains(&BifrostRuntimeRole::Forge)
            {
                Some(Arc::new(create_spill_root(node.oracle.as_ref())?))
            } else {
                None
            };
            let process_role =
                if node.roles.len() == 1 && node.roles.contains(&BifrostRuntimeRole::Forge) {
                    ForgeProcessRole::ForgeWorker
                } else if has_full_server_roles(&node.roles) {
                    if node_count == 1 {
                        ForgeProcessRole::All
                    } else {
                        ForgeProcessRole::Server
                    }
                } else {
                    return Err(ClusterError::Resource(format!(
                        "node {} must declare all Server roles or Forge only",
                        node.node_id.as_uuid()
                    )));
                };
            nodes.insert(
                node.node_id,
                NodeResources {
                    spec: node,
                    wal_root,
                    spill_root,
                    http_addr: reserve_loopback_addr()?,
                    grpc_addr: reserve_loopback_addr()?,
                    process_role,
                },
            );
        }
        let mut cluster = Self {
            servers: nodes.keys().copied().map(|node| (node, None)).collect(),
            nodes,
            fixture,
            storage,
            storage_root,
            catalog,
            redux_catalog,
            topology,
            wal_sync_delay,
            scribe_admission,
            scribe_admission_node,
            faults: OracleFaultController::default(),
            telemetry: process.forge_capture.clone(),
            oracle_peer_credentials,
            oracle_peer_tls,
            forge_completion_observer: options.completion_observer.clone(),
            commit_uncertainty_catalog: commit_uncertainty_catalog.clone(),
            forge_config: options.config.clone(),
            forge_interval: options.interval,
        };
        let node_ids = cluster.nodes.keys().copied().collect::<Vec<_>>();
        let delayed = delay_last_node.then(|| node_ids.last().copied()).flatten();
        for node_id in node_ids.into_iter().filter(|node| Some(*node) != delayed) {
            if let Err(error) = cluster.restart_node(node_id).await {
                let _ = cluster.shutdown().await;
                return Err(error);
            }
        }
        Ok(cluster)
    }

    /// Construct and bind one replacement server from retained node resources.
    async fn build_node(&self, node_id: NodeId) -> Result<WyrdTestServer, ClusterError> {
        let resources = self
            .nodes
            .get(&node_id)
            .ok_or_else(|| ClusterError::Resource(format!("unknown node {}", node_id.as_uuid())))?;
        let mut builder = WyrdTestServerBuilder::default()
            .with_wal_sync_delay(self.wal_sync_delay)
            .with_bifrost_node(node_id, resources.spec.roles.clone())
            .with_bifrost_roots(
                resources.wal_root.as_ref().map(Arc::clone),
                resources.spill_root.as_ref().map(Arc::clone),
            )
            .with_bind_addrs(resources.http_addr, resources.grpc_addr)
            .with_oracle_peer_credentials(Arc::clone(&self.oracle_peer_credentials))
            .with_telemetry(Arc::clone(&process_telemetry()?.guard));
        builder = builder.with_forge_process_role_for_test(resources.process_role);
        builder = builder.with_forge_interval(self.forge_interval);
        if let Some(observer) = &self.forge_completion_observer {
            builder = builder.with_forge_completion_observer_for_test(observer.clone());
        }
        if let Some(config) = &self.forge_config {
            builder = builder.with_forge_config_for_test(config.clone());
        }
        if let Some(catalog) = &self.commit_uncertainty_catalog {
            builder = builder.with_forge_catalog_for_test(catalog.clone());
        }
        if let Some(admission) = self.scribe_admission
            && (self.scribe_admission_node.is_none() || self.scribe_admission_node == Some(node_id))
        {
            builder = builder.with_scribe_admission_for_test(admission);
        }
        if let Some((_, tls)) = &self.oracle_peer_tls {
            builder = builder.with_oracle_peer_tls(tls.clone());
        }
        Ok(builder
            .start_with_resources(
                Arc::clone(&self.fixture),
                Arc::clone(&self.storage),
                Arc::clone(&self.catalog),
                Arc::clone(&self.redux_catalog),
                None,
            )
            .await?
            .bind()
            .await?)
    }

    /// Return the selected named topology.
    #[must_use]
    pub const fn topology(&self) -> BifrostTopology {
        self.topology
    }

    /// Return node IDs currently bound with an ingest-capable role.
    #[must_use]
    pub fn ready_ingest_nodes(&self) -> Vec<NodeId> {
        self.ready_nodes_for(BifrostRuntimeRole::Scribe)
    }

    /// Return node IDs currently bound with an Oracle-capable role.
    #[must_use]
    pub fn ready_query_nodes(&self) -> Vec<NodeId> {
        self.ready_nodes_for(BifrostRuntimeRole::Oracle)
    }

    /// Returns every configured node identity, including delayed or stopped slots.
    #[must_use]
    pub fn configured_node_ids(&self) -> Vec<NodeId> {
        self.nodes.keys().copied().collect()
    }

    /// Filter live node slots by one configured role.
    fn ready_nodes_for(&self, role: BifrostRuntimeRole) -> Vec<NodeId> {
        self.servers
            .iter()
            .filter_map(|(node_id, server)| {
                server.as_ref().and_then(|_| {
                    self.nodes
                        .get(node_id)
                        .filter(|resources| resources.spec.roles.contains(&role))
                        .map(|_| *node_id)
                })
            })
            .collect()
    }

    /// Return the scoped transport fault controller.
    #[must_use]
    pub const fn fault_controller(&self) -> &OracleFaultController {
        &self.faults
    }

    /// Return the process production telemetry capture handle.
    #[must_use]
    pub const fn telemetry(&self) -> &BifrostTelemetryCapture {
        &self.telemetry
    }

    /// Inspect durable membership, admission, audit, and production telemetry.
    ///
    /// # Errors
    ///
    /// Returns a database or conversion error when a typed control-plane row
    /// cannot be read safely.
    pub async fn oracle_inspection(&self) -> Result<OracleInspection, ClusterError> {
        let owner = self
            .fixture
            .superuser_pool()
            .await
            .map_err(|error| ClusterError::Resource(error.to_string()))?;
        let pool = &owner;
        let rows = sqlx::query(
            "SELECT node_id, role, fencing_token, ready FROM vala.cluster_nodes \
             ORDER BY node_id, role",
        )
        .fetch_all(pool)
        .await
        .map_err(|error| ClusterError::Resource(error.to_string()))?;
        let memberships = rows
            .into_iter()
            .map(|row| {
                let role: String = row.get("role");
                let role = match role.as_str() {
                    "scribe" => BifrostRuntimeRole::Scribe,
                    "oracle" => BifrostRuntimeRole::Oracle,
                    other => {
                        return Err(ClusterError::Resource(format!(
                            "unknown membership role {other}"
                        )));
                    }
                };
                let fencing_token: i64 = row.get("fencing_token");
                Ok(OracleMembershipInspection {
                    node_id: NodeId::new(row.get("node_id")),
                    role,
                    fencing_token: u64::try_from(fencing_token).map_err(|error| {
                        ClusterError::Resource(format!("negative fencing token: {error}"))
                    })?,
                    ready: row.get("ready"),
                })
            })
            .collect::<Result<Vec<_>, ClusterError>>()?;
        let active_leases: i64 =
            sqlx::query_scalar("SELECT COUNT(*)::bigint FROM vala.oracle_admission_leases")
                .fetch_one(pool)
                .await
                .map_err(|error| ClusterError::Resource(error.to_string()))?;
        let slots_in_use: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(used_slots), 0)::bigint \
             FROM vala.oracle_admission_accounting",
        )
        .fetch_one(pool)
        .await
        .map_err(|error| ClusterError::Resource(error.to_string()))?;
        let audit_rows: i64 = sqlx::query_scalar("SELECT COUNT(*)::bigint FROM vala.audit_outbox")
            .fetch_one(pool)
            .await
            .map_err(|error| ClusterError::Resource(error.to_string()))?;
        let read_audit_rows: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)::bigint FROM vala.audit_outbox WHERE operation = 'bifrost.query.read_decision'",
        )
        .fetch_one(pool)
        .await
        .map_err(|error| ClusterError::Resource(error.to_string()))?;
        let (
            forge_active_claims,
            forge_active_attempts,
            forge_historical_attempts,
            forge_terminal_tasks,
        ): (i64, i64, i64, i64) =
            sqlx::query_as(
                "SELECT
                    COUNT(*) FILTER (WHERE state IN ('claimed', 'running', 'prepared'))::bigint,
                    COUNT(DISTINCT attempt_id) FILTER (WHERE state IN ('claimed', 'running', 'prepared'))::bigint,
                    COUNT(*) FILTER (WHERE attempt_id IS NOT NULL)::bigint,
                    COUNT(*) FILTER (WHERE state IN ('succeeded', 'failed', 'cancelled', 'unschedulable') AND evidence IS NOT NULL)::bigint
                 FROM vala.forge_tasks",
            )
            .fetch_one(pool)
            .await
            .map_err(|error| ClusterError::Resource(error.to_string()))?;
        let active_tail_fences = self
            .servers
            .values()
            .filter_map(Option::as_ref)
            .filter(|server| server.bifrost_scribe().is_some())
            .map(|server| server.active_bifrost_tail_fences())
            .try_fold(0_u64, |total, count| {
                count
                    .map(|count| total.saturating_add(count))
                    .map_err(|error| ClusterError::Resource(error.to_string()))
            })?;
        Ok(OracleInspection {
            memberships,
            active_leases: u64::try_from(active_leases)
                .map_err(|error| ClusterError::Resource(error.to_string()))?,
            slots_in_use: u64::try_from(slots_in_use)
                .map_err(|error| ClusterError::Resource(error.to_string()))?,
            active_tail_fences,
            audit_rows: u64::try_from(audit_rows)
                .map_err(|error| ClusterError::Resource(error.to_string()))?,
            read_audit_rows: u64::try_from(read_audit_rows)
                .map_err(|error| ClusterError::Resource(error.to_string()))?,
            forge_active_claims: u64::try_from(forge_active_claims)
                .map_err(|error| ClusterError::Resource(error.to_string()))?,
            forge_active_attempts: u64::try_from(forge_active_attempts)
                .map_err(|error| ClusterError::Resource(error.to_string()))?,
            forge_historical_attempts: u64::try_from(forge_historical_attempts)
                .map_err(|error| ClusterError::Resource(error.to_string()))?,
            forge_terminal_tasks: u64::try_from(forge_terminal_tasks)
                .map_err(|error| ClusterError::Resource(error.to_string()))?,
            metric_families: self.telemetry.families(),
            span_names: self.telemetry.span_names(),
        })
    }

    /// Observe that every running Scribe exposes the requested live stream.
    ///
    /// Journeys call this after registering a table and before a Fused query so
    /// the real Oracle fence/page workflow crosses the bound private gRPC service.
    ///
    /// # Errors
    ///
    /// Returns an error when service bootstrap, token exchange, tonic connect,
    /// stream inspection, or transport construction fails.
    pub async fn observe_live_tail(
        &self,
        table: &str,
        event_day: wyrd_spec::vala::api::EventDay,
    ) -> Result<(), ClusterError> {
        self.observe_live_tail_for_tenant(self.data_tenant_id(), table, event_day)
            .await
    }

    /// Observe one tenant's running Scribe streams without creating a route.
    ///
    /// The private bearer and route are scoped to the same tenant so identical
    /// logical table names cannot cross tenant boundaries.
    ///
    /// # Errors
    ///
    /// Returns an error when service bootstrap, token exchange, tonic connect,
    /// stream inspection, or transport construction fails.
    pub async fn observe_live_tail_for_tenant(
        &self,
        tenant: wyrd_spec::DataTenantId,
        table: &str,
        event_day: wyrd_spec::vala::api::EventDay,
    ) -> Result<(), ClusterError> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            match self.observe_live_tail_once(tenant, table, &event_day) {
                Ok(()) => return Ok(()),
                Err(error) if tokio::time::Instant::now() < deadline => {
                    tokio::task::yield_now().await;
                    let _ = error;
                }
                Err(error) => return Err(error),
            }
        }
    }

    /// Checks the current Scribe-owned stream snapshot without mutating routes.
    fn observe_live_tail_once(
        &self,
        tenant: wyrd_spec::DataTenantId,
        table: &str,
        event_day: &wyrd_spec::vala::api::EventDay,
    ) -> Result<(), ClusterError> {
        let canonical = table.strip_prefix("vala.").unwrap_or(table);
        let (namespace, table_name) = canonical.split_once('.').ok_or_else(|| {
            ClusterError::Resource("tail observation table must be namespace-qualified".to_owned())
        })?;
        let binding = TenantTableBinding {
            tenant_id: tenant,
            namespace: namespace.to_owned(),
            table: table_name.to_owned(),
        };
        let mut found = false;
        for scribe_server in self
            .servers()
            .filter(|server| server.bifrost_scribe().is_some())
        {
            let reader = scribe_server
                .state()
                .bifrost_tail_reader_for_test()
                .ok_or_else(|| ClusterError::Resource("Scribe tail reader missing".to_owned()))?;
            let streams = reader
                .list_active_streams(&binding)
                .map_err(|error| ClusterError::Resource(error.to_string()))?;
            if streams.iter().any(|(day, _)| day == event_day) {
                found = true;
            }
        }
        found.then_some(()).ok_or_else(|| {
            ClusterError::Resource(format!(
                "no Scribe has an active stream for {}",
                event_day.as_str()
            ))
        })
    }

    /// Stop one node while retaining its stable identity and local roots.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown/already-stopped node or failed shutdown.
    pub async fn stop_node(&mut self, node_id: NodeId) -> Result<(), ClusterError> {
        let slot = self
            .servers
            .get_mut(&node_id)
            .ok_or_else(|| ClusterError::Resource(format!("unknown node {}", node_id.as_uuid())))?;
        let server = slot.take().ok_or_else(|| {
            ClusterError::Resource(format!("node {} is already stopped", node_id.as_uuid()))
        })?;
        server
            .shutdown()
            .await
            .map_err(|error| ClusterError::Shutdown(error.to_string()))
    }

    /// Restart one stopped node against the same Postgres, storage, WAL, and spill roots.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown/running node or replacement boot failure.
    pub async fn restart_node(&mut self, node_id: NodeId) -> Result<(), ClusterError> {
        let slot = self
            .servers
            .get(&node_id)
            .ok_or_else(|| ClusterError::Resource(format!("unknown node {}", node_id.as_uuid())))?;
        if slot.is_some() {
            return Err(ClusterError::Resource(format!(
                "node {} is already running",
                node_id.as_uuid()
            )));
        }
        let server = self.build_node(node_id).await?;
        self.servers.insert(node_id, Some(server));
        Ok(())
    }

    /// Restarts one stopped node on newly reserved HTTP and gRPC addresses.
    ///
    /// # Errors
    /// Returns the same unknown/running, address-allocation, or boot errors as
    /// [`Self::restart_node`].
    pub async fn restart_node_at_new_address(
        &mut self,
        node_id: NodeId,
    ) -> Result<(), ClusterError> {
        let resources = self
            .nodes
            .get_mut(&node_id)
            .ok_or_else(|| ClusterError::Resource(format!("unknown node {}", node_id.as_uuid())))?;
        resources.http_addr = reserve_loopback_addr()?;
        resources.grpc_addr = reserve_loopback_addr()?;
        self.restart_node(node_id).await
    }

    /// Return the shared fixture tenant.
    #[must_use]
    pub fn data_tenant_id(&self) -> DataTenantId {
        self.fixture.data_tenant_id()
    }

    /// Seed an additional tenant with roles needed by a real Bifrost run.
    ///
    /// # Errors
    ///
    /// Returns an error when tenant creation, role seeding, or commit fails.
    pub async fn add_tenant(&self, slug: &str) -> Result<DataTenantId, ClusterError> {
        let tenant = self
            .fixture
            .seed_additional_tenant(slug)
            .await
            .map_err(|error| ClusterError::Resource(error.to_string()))?;
        let mut conn = self
            .fixture
            .tenant_conn_for(tenant)
            .await
            .map_err(|error| ClusterError::Resource(error.to_string()))?;
        seed_builtin_roles_for_tenant(&mut conn, tenant)
            .await
            .map_err(|error| ClusterError::Resource(error.to_string()))?;
        conn.commit()
            .await
            .map_err(|error| ClusterError::Resource(error.to_string()))?;
        Ok(tenant)
    }

    /// Return the shared Postgres fixture used by every pod.
    #[must_use]
    pub fn pg_fixture(&self) -> &PgFixture {
        &self.fixture
    }

    /// Return the shared object-store operator used by all pods.
    #[must_use]
    pub fn storage_operator(&self) -> Operator {
        self.storage.operator().clone()
    }

    /// Iterate independently bound running servers in stable node order.
    pub fn servers(&self) -> ServerView<'_> {
        ServerView {
            items: self.servers.values().filter_map(Option::as_ref).collect(),
            cursor: 0,
        }
    }

    /// Count currently retained bound supervisor tasks across running servers.
    #[must_use]
    pub fn supervised_task_count_for_test(&self) -> u64 {
        self.servers()
            .map(WyrdTestServer::supervised_task_count_for_test)
            .map(|count| count as u64)
            .sum()
    }

    /// Return one running pod by stable order index.
    #[must_use]
    pub fn server(&self, index: usize) -> Option<&WyrdTestServer> {
        self.servers().get(index).copied()
    }

    /// Return one running pod by stable physical node identity.
    #[must_use]
    pub fn server_by_node(&self, node_id: NodeId) -> Option<&WyrdTestServer> {
        self.servers.get(&node_id).and_then(Option::as_ref)
    }

    /// Return real HTTP endpoints for all running pods.
    #[must_use]
    pub fn base_urls(&self) -> Vec<&str> {
        self.servers()
            .filter_map(WyrdTestServer::base_url)
            .collect()
    }

    /// Iterate Scribe WAL roots in stable node order.
    pub fn wal_dirs(&self) -> impl Iterator<Item = &Path> {
        self.nodes
            .values()
            .filter_map(|resources| resources.wal_root.as_ref().map(|root| root.path()))
    }

    /// Explicitly stop every running server and release shared resources.
    ///
    /// # Errors
    ///
    /// Returns the first teardown error after attempting every running node.
    pub async fn shutdown(self) -> Result<(), ClusterError> {
        self.shutdown_and_inspect().await.map(|_| ())
    }

    /// Stop every server and return concrete listener/server teardown evidence.
    ///
    /// # Errors
    /// Returns the first teardown error after attempting every running node.
    pub async fn shutdown_and_inspect(mut self) -> Result<ClusterShutdownInspection, ClusterError> {
        let mut first_error = None;
        let mut scribe_queued = 0_u64;
        let mut scribe_inflight = 0_u64;
        let mut scribe_wal_streams = 0_u64;
        let mut listeners_stopped = true;
        let mut supervised_tasks = 0_u64;
        for server in self.servers.values().filter_map(Option::as_ref) {
            if server.bifrost_scribe().is_some() {
                if let Err(error) = server.flush_bifrost().await {
                    first_error.get_or_insert(error.to_string());
                }
                if let Some(scribe) = server.bifrost_scribe() {
                    scribe_inflight =
                        scribe_inflight.saturating_add(scribe.inflight_items_for_test() as u64);
                }
            }
        }
        let inspection = self.oracle_inspection().await?;
        let node_ids = self.servers.keys().copied().collect::<Vec<_>>();
        let expected_servers = node_ids.len();
        let mut stopped_servers = 0_usize;
        for node_id in node_ids {
            if let Some(slot) = self.servers.get_mut(&node_id)
                && let Some(server) = slot.take()
            {
                match server.shutdown_and_inspect().await {
                    Ok(server_inspection) => {
                        stopped_servers = stopped_servers.saturating_add(1);
                        listeners_stopped &= server_inspection.listeners_stopped;
                        supervised_tasks =
                            supervised_tasks.saturating_add(server_inspection.supervised_tasks);
                        if let Some(snapshot) = server_inspection.scribe {
                            scribe_queued =
                                scribe_queued.saturating_add(snapshot.queued_items as u64);
                            scribe_wal_streams = scribe_wal_streams
                                .saturating_add(snapshot.open_wal_stream_count as u64);
                        }
                    }
                    Err(error) => {
                        first_error.get_or_insert(error.to_string());
                    }
                }
            }
        }
        if let Some(error) = first_error {
            Err(ClusterError::Shutdown(error))
        } else {
            Ok(ClusterShutdownInspection {
                servers_stopped: stopped_servers == expected_servers,
                listeners_stopped,
                scribe_queued,
                scribe_inflight,
                scribe_wal_streams,
                oracle_leases: inspection.active_leases,
                oracle_slots: inspection.slots_in_use,
                forge_active_claims: inspection.forge_active_claims,
                forge_active_attempts: inspection.forge_active_attempts,
                supervised_tasks,
            })
        }
    }
}

/// Return whether a node carries the complete public Server component roster.
fn has_full_server_roles(roles: &BTreeSet<BifrostRuntimeRole>) -> bool {
    roles.len() == 3
        && roles.contains(&BifrostRuntimeRole::Scribe)
        && roles.contains(&BifrostRuntimeRole::Forge)
        && roles.contains(&BifrostRuntimeRole::Oracle)
}

/// Classify a validated concrete descriptor for legacy lane reporting.
fn classify_topology(spec: &BifrostClusterSpec) -> BifrostTopology {
    match spec.nodes.len() {
        1 => BifrostTopology::OnePod,
        6 if spec.nodes[..3]
            .iter()
            .all(|node| has_full_server_roles(&node.roles))
            && spec.nodes[3..].iter().all(|node| {
                node.roles.len() == 1 && node.roles.contains(&BifrostRuntimeRole::Forge)
            }) =>
        {
            BifrostTopology::ThreeServersThreeForgeWorkers
        }
        4 if spec.nodes.first().is_some_and(|node| {
            node.roles.contains(&BifrostRuntimeRole::Scribe)
                && node.roles.contains(&BifrostRuntimeRole::Forge)
                && node.roles.contains(&BifrostRuntimeRole::Oracle)
        }) && spec.nodes[1..].iter().all(|node| {
            node.roles.len() == 1 && node.roles.contains(&BifrostRuntimeRole::Forge)
        }) =>
        {
            BifrostTopology::DedicatedForgeWorkers
        }
        3 if spec.nodes.iter().any(|node| node.oracle.is_some()) => BifrostTopology::RoleSeparated,
        3 => BifrostTopology::ThreePod,
        6 => BifrostTopology::SixPod,
        _ => BifrostTopology::RoleSeparated,
    }
}

/// Create a retained spill directory under an optional caller-selected parent.
///
/// # Errors
///
/// Returns [`ClusterError::Resource`] when the parent or temp directory cannot
/// be created.
fn create_spill_root(
    resources: Option<&TestOracleResources>,
) -> Result<tempfile::TempDir, ClusterError> {
    if let Some(parent) = resources.and_then(|resources| resources.spill_root.as_deref()) {
        std::fs::create_dir_all(parent)
            .map_err(|error| ClusterError::Resource(error.to_string()))?;
        tempfile::Builder::new()
            .prefix("oracle-spill-")
            .tempdir_in(parent)
            .map_err(|error| ClusterError::Resource(error.to_string()))
    } else {
        tempfile::tempdir().map_err(|error| ClusterError::Resource(error.to_string()))
    }
}

/// Ask the OS for one currently free loopback port used by the cluster binder.
///
/// # Errors
///
/// Returns [`ClusterError::Resource`] when loopback binding or address lookup fails.
fn reserve_loopback_addr() -> Result<std::net::SocketAddr, ClusterError> {
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .map_err(|error| ClusterError::Resource(error.to_string()))?;
    listener
        .local_addr()
        .map_err(|error| ClusterError::Resource(error.to_string()))
}

/// Parse rendered Prometheus values without interpreting comments.
fn rendered_values(rendered: &str) -> BTreeMap<String, f64> {
    rendered
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let (series, value) = line.rsplit_once(' ')?;
            value
                .parse::<f64>()
                .ok()
                .map(|value| (series.to_owned(), value))
        })
        .collect()
}

/// Parse one Prometheus series name and its exact label set.
///
/// # Errors
///
/// Returns [`ClusterError::Telemetry`] when braces, labels, or values are malformed.
fn parse_metric_sample(series: &str, value: f64) -> Result<OracleMetricSample, ClusterError> {
    let (name, labels) = if let Some((name, labels)) = series.split_once('{') {
        (
            name,
            Some(labels.strip_suffix('}').ok_or_else(|| {
                ClusterError::Telemetry(format!("metric labels are not closed: {series}"))
            })?),
        )
    } else {
        (series, None)
    };
    let family = name
        .strip_suffix("_bucket")
        .or_else(|| name.strip_suffix("_sum"))
        .or_else(|| name.strip_suffix("_count"))
        .unwrap_or(name)
        .to_owned();
    let mut parsed_labels = BTreeMap::new();
    if let Some(labels) = labels {
        for label in labels.split(',').filter(|label| !label.is_empty()) {
            let (key, raw_value) = label.split_once('=').ok_or_else(|| {
                ClusterError::Telemetry(format!("metric label is malformed: {label}"))
            })?;
            let label_value = raw_value
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .ok_or_else(|| {
                    ClusterError::Telemetry(format!("metric label is not quoted: {label}"))
                })?;
            parsed_labels.insert(key.to_owned(), label_value.to_owned());
        }
    }
    Ok(OracleMetricSample {
        family,
        labels: parsed_labels,
        value,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The six-process matrix contains exactly three complete Servers and
    /// three Forge-only workers, with no mixed or partial role descriptors.
    #[test]
    fn three_servers_three_workers_descriptor_is_exact() {
        let spec = BifrostClusterSpec::three_servers_three_forge_workers();
        spec.validate().expect("matrix descriptor validates");
        assert_eq!(spec.nodes.len(), 6);
        assert!(
            spec.nodes[..3]
                .iter()
                .all(|node| has_full_server_roles(&node.roles))
        );
        assert!(
            spec.nodes[3..]
                .iter()
                .all(|node| node.roles == BTreeSet::from([BifrostRuntimeRole::Forge]))
        );
        assert_eq!(
            classify_topology(&spec),
            BifrostTopology::ThreeServersThreeForgeWorkers
        );
    }

    /// A stopped node retains its physical roots and restarts into the same slot.
    #[tokio::test]
    async fn restartable_node_slot_preserves_roots() {
        let mut cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed())
            .await
            .expect("cluster starts");
        let node_id = cluster.ready_query_nodes()[0];
        let original_identity = cluster
            .server_by_node(node_id)
            .expect("running node")
            .postgres_pool_identity();
        let wal_root = cluster.wal_dirs().next().expect("WAL root").to_path_buf();
        cluster.stop_node(node_id).await.expect("node stops");
        assert!(cluster.server_by_node(node_id).is_none());
        cluster.restart_node(node_id).await.expect("node restarts");
        let server = cluster.server_by_node(node_id).expect("node is running");
        assert_ne!(server.postgres_pool_identity(), original_identity);
        assert_eq!(cluster.wal_dirs().next(), Some(wal_root.as_path()));
        let mut system_conn = cluster
            .fixture
            .tenant_conn_for(DataTenantId::SYSTEM_OWNER)
            .await
            .expect("system tenant connection");
        let assigned: Vec<(String, serde_json::Value)> = sqlx::query_as(
            "SELECT r.name, r.permissions FROM wyrd.auth_service_accounts sa \
             JOIN wyrd.auth_service_account_roles sar ON sar.data_tenant_id = sa.data_tenant_id AND sar.service_account_id = sa.id \
             JOIN wyrd.auth_roles r ON r.data_tenant_id = sar.data_tenant_id AND r.id = sar.role_id \
             WHERE sa.name = 'bifrost-oracle-peer' ORDER BY r.name",
        )
        .fetch_all(&mut **system_conn.transaction())
        .await
        .expect("Oracle peer role reads");
        drop(system_conn);
        assert_eq!(assigned.len(), 1);
        assert_eq!(assigned[0].0, "bifrost_oracle_peer");
        let stored_permissions: Vec<Permission> =
            serde_json::from_value(assigned[0].1.clone()).expect("permissions decode");
        let expected = vec![Permission::bifrost_oracle_peer_invoke()];
        assert_eq!(stored_permissions, expected);
        let bearer = cluster
            .oracle_peer_credentials
            .bearer(false)
            .await
            .expect("Oracle credential exchanges");
        let effective = server
            .oracle_peer_permissions_for_test(bearer)
            .await
            .expect("Oracle bearer verifies");
        assert_eq!(effective, PermissionSet::from_iter(expected));
        cluster.shutdown().await.expect("cluster shuts down");
    }

    /// Stopping one process closes only its fresh pools while another process
    /// remains queryable, and restart allocates a new pool graph.
    #[tokio::test]
    async fn process_pool_lifecycle_isolated_across_restart() {
        let mut cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::three_mixed())
            .await
            .expect("cluster starts");
        let ids = cluster.configured_node_ids();
        let before = ids
            .iter()
            .map(|id| {
                cluster
                    .server_by_node(*id)
                    .expect("running node")
                    .postgres_pool_identity()
            })
            .collect::<Vec<_>>();
        cluster.stop_node(ids[0]).await.expect("node stops");
        let survivor = cluster.server_by_node(ids[1]).expect("survivor remains");
        sqlx::query("SELECT 1")
            .execute(survivor.state().postgres.app_pool())
            .await
            .expect("survivor pool remains usable");
        cluster.restart_node(ids[0]).await.expect("node restarts");
        let after = ids
            .iter()
            .map(|id| {
                cluster
                    .server_by_node(*id)
                    .expect("running replacement")
                    .postgres_pool_identity()
            })
            .collect::<Vec<_>>();
        assert_ne!(after[0], before[0]);
        assert_eq!(after[1..], before[1..]);
        cluster.shutdown().await.expect("cluster shuts down");
    }

    /// Role-separated descriptors expose only matching live endpoint sets.
    #[tokio::test]
    async fn role_separated_readiness_matches_configured_roles() {
        let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::role_separated())
            .await
            .expect("role-separated cluster starts");
        assert_eq!(cluster.ready_ingest_nodes().len(), 3);
        assert_eq!(cluster.ready_query_nodes().len(), 3);
        assert!(cluster.ready_ingest_nodes().iter().all(|node| {
            cluster
                .server_by_node(*node)
                .and_then(WyrdTestServer::bifrost_scribe)
                .is_some()
        }));
        assert!(cluster.ready_query_nodes().iter().all(|node| {
            cluster
                .server_by_node(*node)
                .is_some_and(|server| server.bifrost_scribe().is_some())
        }));
        cluster.shutdown().await.expect("cluster shuts down");
    }

    /// Prometheus parsing preserves closed label keys and normalizes histograms.
    #[test]
    fn metric_parser_preserves_exact_labels() {
        let sample = parse_metric_sample(
            "bifrost_oracle_query_duration_seconds_bucket{visibility=\"fused\",query_class=\"interactive\",outcome=\"success\",le=\"1\"}",
            2.0,
        )
        .expect("metric parses");
        assert_eq!(sample.family, "bifrost_oracle_query_duration_seconds");
        assert_eq!(sample.labels["visibility"], "fused");
        assert_eq!(sample.labels["query_class"], "interactive");
        assert_eq!(sample.labels["outcome"], "success");
    }
}
