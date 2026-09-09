//! Shared-resource, independently bound Bifrost test cluster.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use metrics_exporter_prometheus::PrometheusHandle;
use opendal::Operator;
use sqlx::Row;
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::catalog::BifrostCatalog;
use vala_bifrost_redux::cluster::RoleTiming;
use vala_bifrost_redux::forge::{ForgeConfig, ForgeWorkerCompletionObserver};
use vala_bifrost_redux::oracle::dispatcher::{
    BifrostPeerTls, OraclePeerCredentials, TonicOraclePeerTransport,
};
use vala_bifrost_redux::resources::SystemResourceSnapshot;
use vala_bifrost_redux::scribe::admission::AdmissionConfig;
use wyrd_auth::seed::seed_builtin_roles_for_tenant;
use wyrd_dev_fixtures::pg::PgFixture;
#[cfg(test)]
use wyrd_runtime::{Permission, PermissionSet};
use wyrd_server::app::metrics::install_recorder;
use wyrd_server::config::BifrostRuntimeRole;
use wyrd_server::config::BifrostTarget;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{NodeId, TenantTableBinding};
use wyrd_storage::{BackendConfig, StorageHandle, StorageSettings};
use wyrd_telemetry::{CapturedSpan, TelemetryConfig, TelemetryGuard, TestTraceCapture};

use crate::bifrost::forge_harness::CommitUncertaintyCatalog;
use crate::bifrost::telemetry::BifrostTelemetryCapture;
use crate::server::{
    OracleRuntimeInspection, TestBifrostPeerTls, WyrdTestServer, WyrdTestServerBuilder,
    WyrdTestServerError, provision_oracle_peer_credentials, reserve_loopback_addr, test_catalog,
};

/// Supported role topology for a Bifrost cluster journey.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BifrostTopology {
    /// Run one full Gate, Scribe, Forge, and Oracle pod.
    OnePod,
    /// Run two full mixed pods for the distributed capacity ladder.
    TwoPod,
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
    ///
    /// Keeps topology validation and cluster construction on one canonical
    /// descriptor.
    pub(crate) fn spec(self) -> BifrostClusterSpec {
        match self {
            Self::OnePod => BifrostClusterSpec::one_mixed(),
            Self::TwoPod => BifrostClusterSpec::two_mixed(),
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
    /// Complete raw process observation retained across node restarts.
    pub system_resources: Option<SystemResourceSnapshot>,
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
    /// Forge compaction budget this pod admits plans against, when it is not
    /// the harness default.
    pub forge_compaction_memory_limit_bytes: Option<usize>,
    /// Optional accelerated role cadence applied only by test-support builders.
    pub role_timing: Option<RoleTiming>,
}

/// Non-empty concrete topology descriptor used by journeys and calibration.
#[derive(Debug, Clone)]
pub struct BifrostClusterSpec {
    /// Ordered node descriptors with unique stable identities.
    pub nodes: Vec<BifrostNodeSpec>,
    /// Optional projection of the production Scribe rotation inputs for journeys.
    scribe_geometry_for_test: Option<vala_bifrost_redux::scribe::geometry::ScribeGeometry>,
    /// Optional deterministic controls for the real Scribe publisher.
    scribe_persistence_faults_for_test:
        Option<vala_bifrost_redux::scribe::persistence::PersistenceFaults>,
    /// Storage I/O bounds every node in this cluster resolves its owner from.
    ///
    /// Carried on the spec rather than applied per start so a restarted node
    /// composes the same owner as its first boot; a cache mode that changed
    /// across a restart would make a parity comparison meaningless.
    storage_io: wyrd_server::config::BifrostStorageIoConfig,
}

impl BifrostClusterSpec {
    /// Construct one mixed node.
    #[must_use]
    pub fn one_mixed() -> Self {
        Self::mixed(1)
    }

    /// Apply one scaled Scribe geometry to every Scribe node.
    ///
    /// The cluster forwards the validated geometry unchanged to the
    /// server-owned Scribe graph, so a journey drives ordinary whole-shard
    /// rotation and ordinary target rolling rather than a substitute.
    #[must_use]
    pub fn with_scribe_geometry_for_test(
        mut self,
        geometry: vala_bifrost_redux::scribe::geometry::ScribeGeometry,
    ) -> Self {
        self.scribe_geometry_for_test = Some(geometry);
        self
    }

    /// Selects the decoded-metadata cache mode every node in this cluster boots with.
    ///
    /// The mode is a boot input, not a label: disabled resolves the production
    /// owner with an explicit zero budget and enabled resolves it with a small
    /// nonzero one, leaving every other storage bound at its default. Both are
    /// values the production policy validates, so a parity run differs from its
    /// twin in exactly one configured scalar.
    #[must_use]
    pub fn with_metadata_cache_mode(mut self, mode: crate::bifrost::ScribeCacheMode) -> Self {
        self.storage_io.metadata_cache_bytes = Some(match mode {
            crate::bifrost::ScribeCacheMode::Disabled => 0,
            crate::bifrost::ScribeCacheMode::Enabled => 32 * 1024 * 1024,
        });
        self
    }

    /// Installs deterministic controls in each real Scribe persistence graph.
    #[must_use]
    pub fn with_scribe_persistence_faults_for_test(
        mut self,
        faults: vala_bifrost_redux::scribe::persistence::PersistenceFaults,
    ) -> Self {
        self.scribe_persistence_faults_for_test = Some(faults);
        self
    }

    /// Applies one raw process observation to every Oracle node.
    ///
    /// Restarted nodes retain this observation and rerun the production policy;
    /// the cluster never derives a desired query grant.
    #[must_use]
    pub fn with_system_resources(mut self, snapshot: SystemResourceSnapshot) -> Self {
        for node in &mut self.nodes {
            if node.roles.contains(&BifrostRuntimeRole::Oracle) {
                let oracle = node.oracle.get_or_insert_with(TestOracleResources::default);
                oracle.system_resources = Some(snapshot);
            }
        }
        self
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

    /// Construct one Oracle leader, one Oracle follower, and one Scribe follower.
    #[must_use]
    pub fn role_separated() -> Self {
        Self {
            nodes: vec![
                Self::node(1, [BifrostRuntimeRole::Oracle]),
                Self::node(2, [BifrostRuntimeRole::Oracle]),
                Self::node(3, [BifrostRuntimeRole::Scribe]),
            ],
            scribe_geometry_for_test: None,
            scribe_persistence_faults_for_test: None,
            storage_io: wyrd_server::config::BifrostStorageIoConfig::default(),
        }
    }

    /// Construct three Oracle-only nodes and one Scribe-only node.
    ///
    /// The smallest topology that puts real distributed stages on real peers:
    /// a coordinator is not a participant in its own worker set, so two remote
    /// Oracles are required, and separating the publisher keeps the query nodes
    /// carrying nothing but the read path whose capacity is under test.
    #[must_use]
    pub fn three_oracles_one_scribe() -> Self {
        Self {
            nodes: vec![
                Self::node(1, [BifrostRuntimeRole::Oracle]),
                Self::node(2, [BifrostRuntimeRole::Oracle]),
                Self::node(3, [BifrostRuntimeRole::Oracle]),
                Self::node(4, [BifrostRuntimeRole::Scribe]),
            ],
            scribe_geometry_for_test: None,
            scribe_persistence_faults_for_test: None,
            storage_io: wyrd_server::config::BifrostStorageIoConfig::default(),
        }
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
                BifrostRuntimeRole::ForgeCoordinator,
                BifrostRuntimeRole::Oracle,
            ],
        )];
        nodes.extend((2..=4).map(|id| Self::node(id, [BifrostRuntimeRole::ForgeWorker])));
        Self {
            nodes,
            scribe_geometry_for_test: None,
            scribe_persistence_faults_for_test: None,
            storage_io: wyrd_server::config::BifrostStorageIoConfig::default(),
        }
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
                        BifrostRuntimeRole::ForgeCoordinator,
                        BifrostRuntimeRole::Oracle,
                    ],
                )
            })
            .collect::<Vec<_>>();
        nodes.extend((4..=6).map(|id| Self::node(id, [BifrostRuntimeRole::ForgeWorker])));
        Self {
            nodes,
            scribe_geometry_for_test: None,
            scribe_persistence_faults_for_test: None,
            storage_io: wyrd_server::config::BifrostStorageIoConfig::default(),
        }
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
                            BifrostRuntimeRole::ForgeCoordinator,
                            BifrostRuntimeRole::ForgeWorker,
                            BifrostRuntimeRole::Oracle,
                        ],
                    )
                })
                .collect(),
            scribe_geometry_for_test: None,
            scribe_persistence_faults_for_test: None,
            storage_io: wyrd_server::config::BifrostStorageIoConfig::default(),
        }
    }

    /// Build one descriptor from deterministic identity and closed roles.
    fn node<const N: usize>(id: u128, roles: [BifrostRuntimeRole; N]) -> BifrostNodeSpec {
        BifrostNodeSpec {
            node_id: NodeId::new(uuid::Uuid::from_u128(id)),
            roles: roles.into_iter().collect(),
            oracle: None,
            forge_compaction_memory_limit_bytes: None,
            role_timing: None,
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
    /// Live-tail fences retained across every Scribe process.
    pub active_tail_fences: u64,
    /// Number of durable audit rows observed across tenants.
    pub audit_rows: u64,
    /// Durable Oracle read-decision audit rows observed across tenants.
    pub read_audit_rows: u64,
    /// Active local Oracle queries across running pods.
    pub active_queries: u64,
    /// Queued local Oracle queries across running pods.
    pub queued_queries: u64,
    /// Memory reservations retained by Oracle queries.
    pub reserved_memory_bytes: u64,
    /// Spill reservations retained by Oracle queries.
    pub reserved_spill_bytes: u64,
    /// Active Oracle-owned process/query scratch directories.
    pub spill_directories: u64,
    /// Regular files beneath all running Oracle-owned scratch prefixes.
    pub spill_files: u64,
    /// Exact regular-file bytes beneath all running Oracle-owned scratch prefixes.
    pub spill_file_bytes: u64,
    /// Peer pending reservations across Oracle pods.
    pub peer_pending: u64,
    /// Peer running reservations across Oracle pods.
    pub peer_running: u64,
    /// Accepted audit records retained in pod-local WALs.
    pub audit_wal_records: u64,
    /// Bytes retained in pod-local WALs.
    pub audit_wal_bytes: u64,
    /// Oldest accepted audit record retained in any pod WAL.
    pub audit_oldest_age: Option<Duration>,
    /// Forge tasks still holding a durable claim.
    pub forge_active_claims: u64,
    /// Distinct Forge attempts still in a non-terminal claimed execution state.
    pub forge_active_attempts: u64,
    /// Forge tasks eligible for claim (`ready` or `retryable`, with `ready_at`
    /// reached) that no worker has claimed yet. These are excluded from
    /// `forge_active_claims`, so a quiesce check that ignores them can still
    /// race a task claimed between the observation and shutdown.
    pub forge_claimable_tasks: u64,
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Forge claims remaining after every server shutdown completes.
    pub forge_active_claims: u64,
    /// Forge attempts remaining after every server shutdown completes.
    pub forge_active_attempts: u64,
    /// Supervised server tasks retained after every server owner is dropped.
    pub supervised_tasks: u64,
    /// Each stopped node's terminal storage-owner snapshot, in stop order.
    ///
    /// Kept per node rather than summed: the storage owner is a per-node
    /// resource, and adding two nodes' counts together would turn a per-node
    /// reconciliation into a cluster-wide one that no single owner ever has to
    /// satisfy.
    pub storage: Vec<vala_bifrost_redux::storage::MetadataCacheSnapshot>,
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
    /// Test polling uses this while a query owns its admission and memory
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
    install_failure_diagnostics_hook(forge_capture.clone());
    let _ = PROCESS_TELEMETRY.set(ProcessTelemetry {
        guard: Arc::new(guard),
        forge_capture,
    });
    PROCESS_TELEMETRY
        .get()
        .ok_or_else(|| ClusterError::Telemetry("process telemetry installation raced".to_owned()))
}

/// Print the installed telemetry's view of the world whenever a test panics.
///
/// The production span tree and metric exposition live in memory for the whole
/// process, and a panicking journey would otherwise discard them — leaving the
/// bare assertion message as the only evidence for a distributed failure. This
/// chains ahead of the existing hook, so the normal panic message and backtrace
/// still print; libtest captures the whole thing and shows it for the failing
/// test only.
///
/// Rendering runs inside `catch_unwind` because a panic raised by a panic hook
/// aborts the process, which would replace a readable test failure with a
/// signal. A diagnostic that cannot be produced is simply omitted.
///
/// # Panics
///
/// Does not panic. Called once, under the process telemetry installation lock.
fn install_failure_diagnostics_hook(capture: BifrostTelemetryCapture) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        previous(info);
        let rendered = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            capture.failure_diagnostics()
        }));
        match rendered {
            Ok(report) => eprintln!("\n--- bifrost telemetry at panic ---\n{report}"),
            Err(_) => eprintln!("\n--- bifrost telemetry at panic: unavailable ---"),
        }
    }));
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
    /// Retained Oracle audit WAL root, absent on query-only nodes.
    audit_wal_root: Option<Arc<tempfile::TempDir>>,
    /// Fixed public HTTP bind retained across a restart.
    http_addr: std::net::SocketAddr,
    /// Fixed private/public gRPC bind retained across a restart.
    grpc_addr: std::net::SocketAddr,
    /// Closed production process role derived from the component set.
    process_role: BifrostTarget,
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
    /// Oracle audit WAL root retained for relay recovery.
    pub audit_wal_root: Option<PathBuf>,
    /// HTTP address proven closed by abrupt termination.
    pub previous_http_addr: std::net::SocketAddr,
    /// gRPC address proven closed by abrupt termination.
    pub previous_grpc_addr: std::net::SocketAddr,
    /// Writer epoch retained as the restart fence baseline.
    pub previous_writer_epoch: Option<i64>,
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
    /// Named topology retained for existing lane selection.
    topology: BifrostTopology,
    /// Builder fault configuration retained by the cluster.
    wal_sync_delay: Duration,
    /// Optional deterministic Scribe admission configuration.
    scribe_admission: Option<AdmissionConfig>,
    /// Optional node receiving the deterministic admission override.
    scribe_admission_node: Option<NodeId>,
    /// Optional test-only production rotation projection retained across restarts.
    scribe_geometry_for_test: Option<vala_bifrost_redux::scribe::geometry::ScribeGeometry>,
    /// Optional test-only persistence controls retained across restarts.
    scribe_persistence_faults_for_test:
        Option<vala_bifrost_redux::scribe::persistence::PersistenceFaults>,
    /// Storage I/O bounds every node start and restart resolves its owner from.
    storage_io: wyrd_server::config::BifrostStorageIoConfig,
    /// Scoped transport fault state.
    faults: OracleFaultController,
    /// Read-only process telemetry handle.
    telemetry: BifrostTelemetryCapture,
    /// Valid SYSTEM_OWNER Service credential retained across node restarts.
    oracle_peer_credentials: Arc<dyn OraclePeerCredentials>,
    /// Optional retained TLS fixture directory and paths for real peer transport.
    oracle_peer_tls: Option<(Arc<tempfile::TempDir>, TestBifrostPeerTls)>,
    /// Peer ticket keyring material every replica in this cluster loads.
    ///
    /// Peer tickets verify against a published manifest, so replicas that must
    /// accept one another's tickets share one keyring; a per-node keyring would
    /// make every east-west call an unknown-key refusal.
    oracle_peer_keyring: Option<crate::bifrost::peer_keyring::TestPeerKeyringPaths>,
    /// Shared observer for supervised Forge worker completions.
    forge_completion_observer: Option<ForgeWorkerCompletionObserver>,
    /// Shared uncertainty-injection catalog wrapper, when enabled.
    commit_uncertainty_catalog: Option<Arc<CommitUncertaintyCatalog>>,
    /// Optional complete Forge configuration for role fixtures.
    forge_config: Option<ForgeConfig>,
    /// Interval used by supervised scheduler roles.
    forge_interval: Duration,
    /// Per-node public-request lifetime cancelled by abrupt process termination.
    abrupt_request_lifetimes: BTreeMap<NodeId, CancellationToken>,
    /// Exact attempt identities returned by production panic reclaim and awaiting clock advance.
    reclaimed_panic_attempts: BTreeMap<uuid::Uuid, uuid::Uuid>,
}

/// Lifetime owner for temporary or caller-declared local cluster storage.
enum ClusterStorageRoot {
    /// Automatically removed test-only storage.
    Temporary(tempfile::TempDir),
    /// Persistent test storage rooted at the declared absolute path.
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

/// Where a starting cluster sources its fixture, storage root, and peer
/// credentials.
///
/// Threaded into [`WyrdTestCluster::start_spec_with_all_options`] as the second
/// element of its harness tuple, this unifies the two storage-provenance
/// choices the start path branches on: an owning cluster that provisions its own
/// resources, versus a family cluster that reuses run-owned shared resources.
enum ClusterResourceSource {
    /// The cluster owns fresh resources: a new [`PgFixture`], freshly
    /// provisioned peer credentials, and either a caller-declared dedicated
    /// storage root or, when `None`, a temporary tempdir root that cluster
    /// shutdown removes.
    Owned {
        /// Caller-declared dedicated local storage root, or `None` for a
        /// temporary root owned by the cluster.
        dedicated_root: Option<PathBuf>,
    },
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
    /// Returns the public-request lifetime tied to one bound test process.
    ///
    /// Axum connection tasks can outlive a dropped in-process listener
    /// supervisor, unlike real process death. Public client journeys select on
    /// this lifetime so abrupt cluster termination also ends the real request.
    ///
    /// # Errors
    ///
    /// Returns [`ClusterError`] when `node_id` is unknown.
    pub fn abrupt_request_lifetime(
        &self,
        node_id: NodeId,
    ) -> Result<CancellationToken, ClusterError> {
        self.abrupt_request_lifetimes
            .get(&node_id)
            .cloned()
            .ok_or_else(|| ClusterError::Resource(format!("unknown node {}", node_id.as_uuid())))
    }

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
        self.abrupt_request_lifetimes
            .get(&node_id)
            .expect("known server slot has an abrupt request lifetime")
            .cancel();
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
            audit_wal_root: resources
                .audit_wal_root
                .as_ref()
                .map(|root| root.path().to_path_buf()),
            previous_http_addr: resources.http_addr,
            previous_grpc_addr: resources.grpc_addr,
            previous_writer_epoch,
        })
    }

    /// Restart an abruptly terminated node on fresh addresses and retained roots.
    ///
    /// This is the replacement-pod half of an abrupt-termination journey, so it
    /// proves the two fences a replacement must advance before any assertion
    /// about replay can mean anything: the node is reachable only at addresses
    /// the terminated process never held, and a Scribe owner comes back at a
    /// strictly greater writer epoch. Both addresses are reserved and compared
    /// before the replacement boots, so a reused address leaves the node stopped
    /// rather than running behind an error, and a boot failure restores the
    /// addresses the node was configured with.
    ///
    /// # Errors
    ///
    /// Returns [`ClusterError::Resource`] when the node is unknown or still
    /// running, when `roots` does not match the stopped node's actual retained
    /// roots and addresses, when either freshly reserved address repeats a
    /// previous one, or when the replacement's writer epoch does not advance
    /// exactly as `None -> None` or `Some(old) -> Some(new > old)`. Returns the
    /// underlying error when address reservation or replacement startup fails.
    pub async fn restart_terminated_node_at_new_address(
        &mut self,
        node_id: NodeId,
        roots: RetainedNodeRoots,
    ) -> Result<(), ClusterError> {
        let unknown = || ClusterError::Resource(format!("unknown node {}", node_id.as_uuid()));
        if self.servers.get(&node_id).ok_or_else(unknown)?.is_some() {
            return Err(ClusterError::Resource(format!(
                "node {} is still running",
                node_id.as_uuid()
            )));
        }
        let resources = self.nodes.get(&node_id).ok_or_else(unknown)?;
        let previous_http_addr = resources.http_addr;
        let previous_grpc_addr = resources.grpc_addr;
        let observed = RetainedNodeRoots {
            wal_root: resources
                .wal_root
                .as_ref()
                .map(|root| root.path().to_path_buf()),
            spill_root: resources
                .spill_root
                .as_ref()
                .map(|root| root.path().to_path_buf()),
            audit_wal_root: resources
                .audit_wal_root
                .as_ref()
                .map(|root| root.path().to_path_buf()),
            previous_http_addr,
            previous_grpc_addr,
            previous_writer_epoch: roots.previous_writer_epoch,
        };
        if observed != roots {
            return Err(ClusterError::Resource(
                "retained node roots do not match the stopped node".to_owned(),
            ));
        }

        // Reserved before anything is mutated: a replacement that reused an
        // address would prove nothing about the terminated listener, and the
        // node must stay stopped rather than run behind this error.
        let http_addr = reserve_loopback_addr()?;
        let grpc_addr = reserve_loopback_addr()?;
        if http_addr == previous_http_addr || grpc_addr == previous_grpc_addr {
            return Err(ClusterError::Resource(
                "replacement node did not advance its listener addresses".to_owned(),
            ));
        }
        let resources = self.nodes.get_mut(&node_id).ok_or_else(unknown)?;
        resources.http_addr = http_addr;
        resources.grpc_addr = grpc_addr;
        let server = match self.build_node(node_id).await {
            Ok(server) => server,
            Err(error) => {
                let resources = self.nodes.get_mut(&node_id).ok_or_else(unknown)?;
                resources.http_addr = previous_http_addr;
                resources.grpc_addr = previous_grpc_addr;
                return Err(error);
            }
        };
        let writer_epoch = server
            .bifrost_scribe()
            .map(|scribe| scribe.writer_epoch_for_test());
        self.servers.insert(node_id, Some(server));
        self.abrupt_request_lifetimes
            .insert(node_id, CancellationToken::new());
        // The replacement is registered before this check so a cluster shutdown
        // still drains it; an unadvanced fence is a failed journey, not a leak.
        match (roots.previous_writer_epoch, writer_epoch) {
            (None, None) => Ok(()),
            (Some(previous), Some(replacement)) if replacement > previous => Ok(()),
            (previous, replacement) => Err(ClusterError::Resource(format!(
                "replacement node did not advance its writer fence: {previous:?} -> {replacement:?}"
            ))),
        }
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
        peer_tls_from_paths(tls)?
            .endpoint(address)
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
        // The leader's own peer authority signs its reservation tickets, exactly
        // as boot wires it, so a journey driving this transport exercises the
        // production authorization path rather than an unticketed one.
        let minter = server
            .state()
            .bifrost
            .oracle_peer_service()
            .ok_or_else(|| ClusterError::Resource("Oracle peer runtime is absent".to_owned()))?;
        Ok(TonicOraclePeerTransport::with_credentials_and_tls(
            server
                .state()
                .oracle_cluster()
                .ok_or_else(|| ClusterError::Resource("Oracle cluster is absent".to_owned()))?,
            Arc::clone(&self.oracle_peer_credentials),
            peer_tls_from_paths(tls)?,
        )
        .with_reservation_minter(Arc::clone(minter.authority())
            as Arc<dyn vala_bifrost_redux::oracle::peer::ReservationTicketMinter>))
    }

    /// Refreshes every running node's authoritative immutable membership cut.
    ///
    /// # Errors
    /// Returns a resource error when any registry refresh fails.
    pub async fn refresh_oracle_snapshots(&self) -> Result<(), ClusterError> {
        for server in self.servers.values().flatten() {
            if let Some(cluster) = server.state().oracle_cluster() {
                cluster
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
        Self::start_spec_with_options(spec, Duration::ZERO, None, false).await
    }

    /// Starts a topology while retaining the final node as an unbooted slot.
    ///
    /// The reserved slot lets a journey observe the cluster before the last
    /// replica joins, then boot it through [`Self::restart_node`].
    ///
    /// # Errors
    /// Returns the same resource, topology, and boot errors as
    /// [`Self::start_spec`].
    pub async fn start_spec_delayed_last(spec: BifrostClusterSpec) -> Result<Self, ClusterError> {
        Self::start_spec_with_options(spec, Duration::ZERO, None, true).await
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
        Self::start_spec_with_options(topology.spec(), wal_sync_delay, None, false).await
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
        Self::start_spec_with_options(topology.spec(), Duration::ZERO, Some(admission), false).await
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
            (
                ForgeHarnessOptions::default(),
                ClusterResourceSource::Owned {
                    dedicated_root: None,
                },
            ),
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
            (
                ForgeHarnessOptions {
                    completion_observer: Some(ForgeWorkerCompletionObserver::new()),
                    ..ForgeHarnessOptions::default()
                },
                ClusterResourceSource::Owned {
                    dedicated_root: None,
                },
            ),
        )
        .await
    }

    /// Start an explicit descriptor with a selected Forge configuration and observer.
    ///
    /// When `delay_last_node` is true, retain the highest node identity for later
    /// startup through `restart_node`, allowing ingestion before maintenance.
    ///
    /// When `inject_uncertainty` is true every Forge process wraps its catalog
    /// in the shared commit seam, which is inert until a journey arms it. That
    /// is what lets one journey pause or definitively refuse a real publication
    /// while the sibling journeys on the same topology see the plain catalog.
    ///
    /// # Errors
    /// Returns the same topology, resource, configuration, and role-supervision
    /// errors as [`Self::start_spec_with_forge_completion_observer`].
    pub async fn start_spec_with_forge_config_and_completion_observer(
        spec: BifrostClusterSpec,
        config: ForgeConfig,
        delay_last_node: bool,
        inject_uncertainty: bool,
    ) -> Result<Self, ClusterError> {
        Self::start_spec_with_all_options(
            spec,
            Duration::ZERO,
            None,
            None,
            delay_last_node,
            (
                ForgeHarnessOptions {
                    completion_observer: Some(ForgeWorkerCompletionObserver::new()),
                    config: Some(config),
                    inject_uncertainty,
                    ..ForgeHarnessOptions::default()
                },
                ClusterResourceSource::Owned {
                    dedicated_root: None,
                },
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
            (
                ForgeHarnessOptions {
                    completion_observer: Some(ForgeWorkerCompletionObserver::new()),
                    ..ForgeHarnessOptions::default()
                },
                ClusterResourceSource::Owned {
                    dedicated_root: Some(storage_root),
                },
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
            (
                ForgeHarnessOptions {
                    completion_observer: Some(ForgeWorkerCompletionObserver::new()),
                    ..ForgeHarnessOptions::default()
                },
                ClusterResourceSource::Owned {
                    dedicated_root: None,
                },
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
            (
                ForgeHarnessOptions {
                    completion_observer: Some(observer),
                    ..ForgeHarnessOptions::default()
                },
                ClusterResourceSource::Owned {
                    dedicated_root: None,
                },
            ),
        )
        .await
    }

    /// Start one embedded `all` process with catalog-panic controls and a completion observer.
    ///
    /// # Errors
    /// Returns a topology, resource, configuration, or role-supervision error.
    pub async fn start_with_embedded_forge_panic_recovery_for_test(
        config: ForgeConfig,
    ) -> Result<Self, ClusterError> {
        Self::start_spec_with_all_options(
            BifrostClusterSpec::one_mixed(),
            Duration::ZERO,
            None,
            None,
            false,
            (
                ForgeHarnessOptions {
                    completion_observer: Some(ForgeWorkerCompletionObserver::new()),
                    config: Some(config),
                    inject_uncertainty: true,
                    interval: Duration::from_secs(1),
                },
                ClusterResourceSource::Owned {
                    dedicated_root: None,
                },
            ),
        )
        .await
    }

    /// Start one embedded bound `all` pod with uncertainty and no ambient pass.
    ///
    /// The interval is explicit and long because a journey that drives the
    /// server-owned scheduler itself cannot also be racing an ambient tick: a
    /// pass it did not request could plan or claim the very task it is about to
    /// observe. Everything else is the production composition — one bound pod
    /// serving public HTTP and gRPC, with Scribe, Oracle, and the server-owned
    /// Forge scheduler and worker roles.
    ///
    /// # Errors
    /// Returns a topology, resource, or role-supervision error.
    pub async fn start_embedded_forge_uncertainty_for_test(
        interval: Duration,
    ) -> Result<Self, ClusterError> {
        Self::start_spec_with_all_options(
            BifrostClusterSpec::one_mixed(),
            Duration::ZERO,
            None,
            None,
            false,
            (
                ForgeHarnessOptions {
                    completion_observer: Some(ForgeWorkerCompletionObserver::new()),
                    config: None,
                    inject_uncertainty: true,
                    interval,
                },
                ClusterResourceSource::Owned {
                    dedicated_root: None,
                },
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
            (
                ForgeHarnessOptions {
                    completion_observer: Some(observer),
                    config: forge_config,
                    inject_uncertainty: uncertainty.unwrap_or(false),
                    interval: forge_interval.unwrap_or(Duration::from_secs(60)),
                },
                ClusterResourceSource::Owned {
                    dedicated_root: None,
                },
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
        delay_last_node: bool,
    ) -> Result<Self, ClusterError> {
        Self::start_spec_with_all_options(
            spec,
            wal_sync_delay,
            scribe_admission,
            None,
            delay_last_node,
            (
                ForgeHarnessOptions::default(),
                ClusterResourceSource::Owned {
                    dedicated_root: None,
                },
            ),
        )
        .await
    }

    /// Build a topology with production Forge role supervision controls.
    ///
    /// The harness tuple selects a temporary or caller-declared storage root.
    /// The cluster owns a fresh fixture and freshly provisioned peer credentials.
    ///
    /// # Errors
    /// Returns [`ClusterError::Topology`] when the descriptor fails validation,
    /// [`ClusterError::Resource`] when an owned [`PgFixture`] start, built-in
    /// role seeding, storage-root creation or canonicalization, catalog
    /// construction, or peer-credential provisioning fails,
    /// [`ClusterError::Server`] when a node fails to start or bind, and
    /// [`ClusterError::Telemetry`] when production telemetry cannot be installed.
    async fn start_spec_with_all_options(
        spec: BifrostClusterSpec,
        wal_sync_delay: Duration,
        scribe_admission: Option<AdmissionConfig>,
        scribe_admission_node: Option<usize>,
        delay_last_node: bool,
        harness: (ForgeHarnessOptions, ClusterResourceSource),
    ) -> Result<Self, ClusterError> {
        let (options, resource_source) = harness;
        spec.validate()?;
        let scribe_geometry_for_test = spec.scribe_geometry_for_test;
        let scribe_persistence_faults_for_test = spec.scribe_persistence_faults_for_test.clone();
        let storage_io = spec.storage_io;
        let process = process_telemetry()?;
        // `explicit_root` is the storage root to reuse (a shared or a
        // caller-declared dedicated root); `None` selects a temporary root.
        let (fixture, shared_credentials, explicit_root) = match resource_source {
            ClusterResourceSource::Owned { dedicated_root } => {
                let fixture = Arc::new(
                    PgFixture::start()
                        .await
                        .map_err(|error| ClusterError::Resource(error.to_string()))?,
                );
                (fixture, None, dedicated_root)
            }
        };
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

        // A shared storage root reuses the run's dedicated tree; both the
        // shared and caller-declared dedicated roots resolve to a non-deleting
        // `Dedicated` guard so cluster shutdown never removes shared data.
        let storage_root = Arc::new(match explicit_root {
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
        let storage_settings = StorageSettings {
            backend: BackendConfig::Local {
                root: storage_root.path().to_path_buf(),
            },
            require_encryption: false,
            presign_ttl: Duration::from_secs(600),
            part_size_bytes: 16 * 1024 * 1024,
            multipart_threshold_bytes: 100 * 1024 * 1024,
            public_base_url: Some("https://wyrd.test".to_owned()),
        };
        // The filesystem service resumes a listing from `start_after` correctly
        // but does not advertise the capability, and Forge workers refuse to
        // start on a staging backend that cannot resume a bounded orphan scan.
        // This shared cluster root stands in for a production object store, so
        // it declares the support it actually has.
        let operator = wyrd_storage::factory::build_operator(&storage_settings.backend)
            .map_err(|error| ClusterError::Resource(error.to_string()))?
            .layer(opendal::layers::CapabilityOverrideLayer::new(
                |mut capability| {
                    capability.list_with_start_after = true;
                    capability
                },
            ));
        let storage = StorageHandle::from_settings_with_operator(storage_settings, operator)
            .await
            .map_err(|error| ClusterError::Resource(error.to_string()))?;
        // Only the uncertainty wrapper needs a cluster-level catalog handle;
        // every node otherwise builds its own from its own storage owner, so
        // constructing one unconditionally would create a catalog no node uses.
        let commit_uncertainty_catalog = match options.inject_uncertainty {
            false => None,
            true => {
                let catalog: Arc<BifrostCatalog> =
                    test_catalog(&fixture, crate::server::test_storage_owner(&storage)).await?;
                Some(CommitUncertaintyCatalog::new(catalog.iceberg_catalog()))
            }
        };
        // Reuse the once-provisioned shared credentials when booting over shared
        // resources; their provisioning path is non-idempotent plain inserts, so
        // a fresh provision only runs when this cluster owns its fixture.
        let oracle_peer_credentials = match shared_credentials {
            Some(credentials) => credentials,
            None => provision_oracle_peer_credentials(Arc::clone(&fixture)).await?,
        };
        // Every replica in one topology must chain to the same peer CA, so the
        // authority is minted once per cluster and shared. The peer plane is
        // mandatory for a Scribe- or Oracle-bearing target, so this is
        // unconditional rather than gated on a per-test flag.
        let oracle_peer_tls = {
            let root = Arc::new(
                tempfile::tempdir().map_err(|error| ClusterError::Resource(error.to_string()))?,
            );
            let authority = crate::bifrost::peer_ca::BifrostPeerCa::generate("localhost")
                .map_err(|error| ClusterError::Resource(error.to_string()))?;
            let tls = authority
                .materialize(root.path(), "cluster")
                .map_err(|error| ClusterError::Resource(error.to_string()))?;
            Some((root, tls))
        };
        // Materialized beside the peer PEMs, in the same once-per-cluster root,
        // for the same reason: one topology, one published peer authority.
        let oracle_peer_keyring = match &oracle_peer_tls {
            Some((root, _)) => Some(
                crate::bifrost::peer_keyring::TestPeerKeyring::generate()
                    .materialize(root.path(), "cluster")
                    .map_err(|error| ClusterError::Resource(error.to_string()))?,
            ),
            None => None,
        };
        let topology = classify_topology(&spec);
        let scribe_admission_node =
            scribe_admission_node.and_then(|index| spec.nodes.get(index).map(|node| node.node_id));
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
                || node.roles.contains(&BifrostRuntimeRole::ForgeCoordinator)
                || node.roles.contains(&BifrostRuntimeRole::ForgeWorker)
            {
                Some(Arc::new(create_spill_root(node.oracle.as_ref())?))
            } else {
                None
            };
            let audit_wal_root = if node.roles.contains(&BifrostRuntimeRole::Oracle) {
                Some(Arc::new(tempfile::tempdir().map_err(|error| {
                    ClusterError::Resource(error.to_string())
                })?))
            } else {
                None
            };
            let process_role = process_target_for_roles(&node.roles).ok_or_else(|| {
                ClusterError::Resource(format!(
                    "node {} has no exact public process target",
                    node.node_id.as_uuid()
                ))
            })?;
            nodes.insert(
                node.node_id,
                NodeResources {
                    spec: node,
                    wal_root,
                    spill_root,
                    audit_wal_root,
                    http_addr: reserve_loopback_addr()?,
                    grpc_addr: reserve_loopback_addr()?,
                    process_role,
                },
            );
        }
        let abrupt_request_lifetimes = nodes
            .keys()
            .copied()
            .map(|node| (node, CancellationToken::new()))
            .collect();
        let mut cluster = Self {
            servers: nodes.keys().copied().map(|node| (node, None)).collect(),
            nodes,
            fixture,
            storage,
            storage_root,
            topology,
            wal_sync_delay,
            scribe_admission,
            scribe_admission_node,
            scribe_geometry_for_test,
            scribe_persistence_faults_for_test,
            storage_io,
            faults: OracleFaultController::default(),
            telemetry: process.forge_capture.clone(),
            oracle_peer_credentials,
            oracle_peer_tls,
            oracle_peer_keyring,
            forge_completion_observer: options.completion_observer.clone(),
            commit_uncertainty_catalog: commit_uncertainty_catalog.clone(),
            forge_config: options.config.clone(),
            forge_interval: options.interval,
            abrupt_request_lifetimes,
            reclaimed_panic_attempts: BTreeMap::new(),
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
                resources.audit_wal_root.as_ref().map(Arc::clone),
            )
            .with_bind_addrs(resources.http_addr, resources.grpc_addr)
            .with_oracle_peer_credentials(Arc::clone(&self.oracle_peer_credentials))
            .with_bifrost_storage_io_for_test(self.storage_io)
            .with_telemetry(Arc::clone(&process_telemetry()?.guard));
        if let Some(timing) = resources.spec.role_timing {
            builder = builder.with_role_timing_for_test(timing);
        }
        if let Some(snapshot) = resources
            .spec
            .oracle
            .as_ref()
            .and_then(|oracle| oracle.system_resources)
        {
            builder = builder.with_system_resources_for_test(snapshot);
        }
        if let Some(bytes) = resources.spec.forge_compaction_memory_limit_bytes {
            builder = builder.with_forge_compaction_memory_limit_for_test(bytes);
        }
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
        if let Some(geometry) = self.scribe_geometry_for_test {
            builder = builder.with_scribe_geometry_for_test(geometry);
        }
        if let Some(faults) = &self.scribe_persistence_faults_for_test {
            builder = builder.with_scribe_persistence_faults_for_test(faults.clone());
        }
        if let Some((_, tls)) = &self.oracle_peer_tls {
            builder = builder.with_peer_tls(tls.clone());
        }
        if let Some(keyring) = &self.oracle_peer_keyring {
            builder = builder.with_peer_keyring_paths(keyring.clone());
        }
        Ok(builder
            .start_with_resources(Arc::clone(&self.fixture), Arc::clone(&self.storage), None)
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

    /// Inspect durable membership, audit, and production telemetry.
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
            forge_claimable_tasks,
            forge_historical_attempts,
            forge_terminal_tasks,
        ): (i64, i64, i64, i64, i64) =
            sqlx::query_as(
                "SELECT
                    COUNT(*) FILTER (WHERE state IN ('claimed', 'running', 'prepared'))::bigint,
                    COUNT(DISTINCT attempt_id) FILTER (WHERE state IN ('claimed', 'running', 'prepared'))::bigint,
                    COUNT(*) FILTER (WHERE state IN ('ready', 'retryable') AND ready_at <= statement_timestamp())::bigint,
                    COUNT(*) FILTER (WHERE attempt_id IS NOT NULL)::bigint,
                    COUNT(*) FILTER (WHERE state IN ('succeeded', 'failed', 'cancelled') AND evidence IS NOT NULL)::bigint
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
        let mut runtime = OracleRuntimeInspection::default();
        for server in self.servers.values().flatten() {
            if let Ok(snapshot) = server.oracle_runtime_inspection() {
                runtime.active_queries = runtime
                    .active_queries
                    .saturating_add(snapshot.active_queries);
                runtime.queued_queries = runtime
                    .queued_queries
                    .saturating_add(snapshot.queued_queries);
                runtime.reserved_memory_bytes = runtime
                    .reserved_memory_bytes
                    .saturating_add(snapshot.reserved_memory_bytes);
                runtime.reserved_spill_bytes = runtime
                    .reserved_spill_bytes
                    .saturating_add(snapshot.reserved_spill_bytes);
                runtime.peer_pending = runtime.peer_pending.saturating_add(snapshot.peer_pending);
                runtime.peer_running = runtime.peer_running.saturating_add(snapshot.peer_running);
                runtime.audit_wal_records = runtime
                    .audit_wal_records
                    .saturating_add(snapshot.audit_wal_records);
                runtime.audit_wal_bytes = runtime
                    .audit_wal_bytes
                    .saturating_add(snapshot.audit_wal_bytes);
                runtime.spill_directories = runtime
                    .spill_directories
                    .saturating_add(snapshot.spill_directories);
                runtime.spill_files = runtime.spill_files.saturating_add(snapshot.spill_files);
                runtime.spill_file_bytes = runtime
                    .spill_file_bytes
                    .saturating_add(snapshot.spill_file_bytes);
                runtime.audit_oldest_age =
                    match (runtime.audit_oldest_age, snapshot.audit_oldest_age) {
                        (None, age) => age,
                        (age, None) => age,
                        (Some(left), Some(right)) => Some(left.max(right)),
                    };
            }
        }
        Ok(OracleInspection {
            memberships,
            active_tail_fences,
            audit_rows: u64::try_from(audit_rows)
                .map_err(|error| ClusterError::Resource(error.to_string()))?,
            read_audit_rows: u64::try_from(read_audit_rows)
                .map_err(|error| ClusterError::Resource(error.to_string()))?,
            active_queries: runtime.active_queries,
            queued_queries: runtime.queued_queries,
            reserved_memory_bytes: runtime.reserved_memory_bytes,
            reserved_spill_bytes: runtime.reserved_spill_bytes,
            spill_directories: runtime.spill_directories,
            spill_files: runtime.spill_files,
            spill_file_bytes: runtime.spill_file_bytes,
            peer_pending: runtime.peer_pending,
            peer_running: runtime.peer_running,
            audit_wal_records: runtime.audit_wal_records,
            audit_wal_bytes: runtime.audit_wal_bytes,
            audit_oldest_age: runtime.audit_oldest_age,
            forge_active_claims: u64::try_from(forge_active_claims)
                .map_err(|error| ClusterError::Resource(error.to_string()))?,
            forge_active_attempts: u64::try_from(forge_active_attempts)
                .map_err(|error| ClusterError::Resource(error.to_string()))?,
            forge_claimable_tasks: u64::try_from(forge_claimable_tasks)
                .map_err(|error| ClusterError::Resource(error.to_string()))?,
            forge_historical_attempts: u64::try_from(forge_historical_attempts)
                .map_err(|error| ClusterError::Resource(error.to_string()))?,
            forge_terminal_tasks: u64::try_from(forge_terminal_tasks)
                .map_err(|error| ClusterError::Resource(error.to_string()))?,
            metric_families: self.telemetry.families(),
            span_names: self.telemetry.span_names(),
        })
    }

    /// Projects every Oracle node's live root resource ownership.
    ///
    /// One entry per node that composed an Oracle, in cluster order. The
    /// snapshot is the production root's own, so a journey asserting that
    /// Scribe, Oracle, and Forge together stay inside the managed budget is
    /// reading the same accounting admission refuses against.
    ///
    /// # Errors
    ///
    /// Returns [`ClusterError::Resource`] when a node's root accounting is
    /// poisoned and cannot produce a trustworthy snapshot.
    pub fn oracle_resource_snapshots(
        &self,
    ) -> Result<Vec<vala_bifrost_redux::resources::ResourceSnapshot>, ClusterError> {
        self.servers
            .values()
            .flatten()
            .filter_map(|server| server.state().bifrost_resources())
            .filter(|resources| resources.oracle().is_some())
            .map(|resources| {
                resources
                    .snapshot()
                    .map_err(|error| ClusterError::Resource(error.to_string()))
            })
            .collect()
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
        time_partition: wyrd_spec::vala::api::TimePartitionWire,
    ) -> Result<(), ClusterError> {
        self.observe_live_tail_for_tenant(self.data_tenant_id(), table, time_partition)
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
        time_partition: wyrd_spec::vala::api::TimePartitionWire,
    ) -> Result<(), ClusterError> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            match self.observe_live_tail_once(tenant, table, &time_partition) {
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
        time_partition: &wyrd_spec::vala::api::TimePartitionWire,
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
            if streams
                .iter()
                .any(|(partition, _)| partition == time_partition)
            {
                found = true;
            }
        }
        found.then_some(()).ok_or_else(|| {
            ClusterError::Resource(format!(
                "no Scribe has an active stream for {}",
                time_partition.start_utc().to_rfc3339()
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
            .shutdown_and_inspect()
            .await
            .map(|_| ())
            .map_err(|error| ClusterError::Shutdown(error.to_string()))
    }

    /// Await one node's real terminal supervisor result and retain its restart slot.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown/stopped node, timeout, join failure, or
    /// unexpected clean shutdown.
    pub async fn await_node_terminal_failure_for_test(
        &mut self,
        node_id: NodeId,
        deadline: Duration,
    ) -> Result<String, ClusterError> {
        let slot = self
            .servers
            .get_mut(&node_id)
            .ok_or_else(|| ClusterError::Resource(format!("unknown node {}", node_id.as_uuid())))?;
        let server = slot.take().ok_or_else(|| {
            ClusterError::Resource(format!("node {} is already stopped", node_id.as_uuid()))
        })?;
        server
            .await_terminal_failure_for_test(deadline)
            .await
            .map_err(ClusterError::Server)
    }

    /// Advance only the exact unclassified row produced by panic reclaim.
    ///
    /// # Errors
    ///
    /// Returns an error unless the row is retryable, unowned, has exactly one
    /// consumed attempt, has no failure class, and its last reclaimed attempt
    /// matches `expected_attempt` in the caller's retained evidence.
    pub async fn advance_reclaimed_panic_task_for_test(
        &mut self,
        task_id: uuid::Uuid,
        expected_attempt: uuid::Uuid,
    ) -> Result<(), ClusterError> {
        if self.reclaimed_panic_attempts.get(&task_id) != Some(&expected_attempt) {
            return Err(ClusterError::Resource(format!(
                "panic recovery attempt {expected_attempt} is not the exact reclaimed evidence for task {task_id}"
            )));
        }
        let row: (String, i32, Option<String>, bool, bool) = sqlx::query_as(
            "SELECT state,attempt_count,failure_class,attempt_id IS NULL,claimed_by IS NULL FROM vala.forge_tasks WHERE task_id=$1",
        )
        .bind(task_id)
        .fetch_one(self.fixture.operator_pool().pool())
        .await
        .map_err(|error| ClusterError::Resource(error.to_string()))?;
        if !is_exact_reclaimed_panic_row(&row) {
            return Err(ClusterError::Resource(format!(
                "panic recovery row {task_id} is not exact reclaimed state for attempt {expected_attempt}: {row:?}"
            )));
        }
        let changed = sqlx::query(
            "UPDATE vala.forge_tasks SET next_eligible_at=statement_timestamp(),ready_at=statement_timestamp() WHERE task_id=$1 AND state='retryable' AND attempt_count=1 AND failure_class IS NULL AND attempt_id IS NULL AND claimed_by IS NULL",
        )
        .bind(task_id)
        .execute(self.fixture.operator_pool().pool())
        .await
        .map_err(|error| ClusterError::Resource(error.to_string()))?
        .rows_affected();
        if changed != 1 {
            return Err(ClusterError::Resource(format!(
                "panic recovery eligibility update lost exact row {task_id} from attempt {expected_attempt}"
            )));
        }
        self.reclaimed_panic_attempts.remove(&task_id);
        Ok(())
    }

    /// Retain the exact panic-owned attempt observed before restarting its node.
    ///
    /// The subsequent eligibility seam accepts only this pair after production
    /// reclaim has cleared the durable ownership columns.
    pub fn retain_panic_attempt_for_test(&mut self, task_id: uuid::Uuid, attempt_id: uuid::Uuid) {
        self.reclaimed_panic_attempts.insert(task_id, attempt_id);
    }

    /// Return the canonical audit-outbox row count for panic-clock assertions.
    ///
    /// # Errors
    ///
    /// Returns a fixture or SQL error when the privileged assertion connection
    /// cannot read the canonical outbox.
    pub async fn audit_outbox_count_for_test(&self) -> Result<i64, ClusterError> {
        let pool = self
            .fixture
            .superuser_pool()
            .await
            .map_err(|error| ClusterError::Resource(error.to_string()))?;
        sqlx::query_scalar("SELECT count(*) FROM vala.audit_outbox")
            .fetch_one(&pool)
            .await
            .map_err(|error| ClusterError::Resource(error.to_string()))
    }

    /// Reclaim expired Forge attempts through production and retain their exact evidence.
    ///
    /// # Errors
    ///
    /// Returns an error when the node is stopped, reclaim fails, or duplicate
    /// evidence for one task disagrees with the production result.
    pub async fn reclaim_expired_forge_attempts_for_test(
        &mut self,
        node_id: NodeId,
        cap: u32,
    ) -> Result<Vec<(uuid::Uuid, uuid::Uuid)>, ClusterError> {
        let reclaimed = self
            .server_by_node(node_id)
            .ok_or_else(|| {
                ClusterError::Resource(format!("node {} is stopped", node_id.as_uuid()))
            })?
            .reclaim_expired_forge_attempts_for_test(cap)
            .await
            .map_err(ClusterError::Server)?;
        for (task_id, attempt_id) in &reclaimed {
            if self.reclaimed_panic_attempts.get(task_id) != Some(attempt_id) {
                return Err(ClusterError::Resource(format!(
                    "panic reclaim returned untracked attempt {attempt_id} for task {task_id}"
                )));
            }
        }
        Ok(reclaimed)
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
        self.abrupt_request_lifetimes
            .insert(node_id, CancellationToken::new());
        Ok(())
    }

    /// Seeds crash residue and an unrelated sibling beneath one node's Oracle spill root.
    ///
    /// The owned `oracle-runtime-*` child models files left by a pod crash. The
    /// unrelated sibling proves restart cleanup remains prefix-scoped.
    ///
    /// # Errors
    ///
    /// Returns a resource error for an unknown node, a node without an Oracle
    /// spill root, or any fixture-directory or marker-write failure.
    pub fn seed_oracle_spill_restart_fixture(&self, node_id: NodeId) -> Result<(), ClusterError> {
        let root = self
            .nodes
            .get(&node_id)
            .and_then(|resources| resources.spill_root.as_ref())
            .ok_or_else(|| ClusterError::Resource("Oracle spill root missing".to_owned()))?;
        let oracle = root.path().join("oracle-spill");
        for child in ["oracle-runtime-stale", "unrelated-sibling"] {
            let directory = oracle.join(child);
            std::fs::create_dir_all(&directory)
                .map_err(|error| ClusterError::Resource(error.to_string()))?;
            std::fs::write(directory.join("marker"), b"restart-fixture")
                .map_err(|error| ClusterError::Resource(error.to_string()))?;
        }
        Ok(())
    }

    /// Reports whether restart residue and its unrelated sibling still exist.
    ///
    /// The tuple is `(owned_stale_exists, unrelated_sibling_exists)` and never
    /// exposes filesystem paths outside the test harness.
    ///
    /// # Errors
    ///
    /// Returns a resource error for an unknown node or missing Oracle spill root.
    pub fn oracle_spill_restart_fixture_state(
        &self,
        node_id: NodeId,
    ) -> Result<(bool, bool), ClusterError> {
        let root = self
            .nodes
            .get(&node_id)
            .and_then(|resources| resources.spill_root.as_ref())
            .ok_or_else(|| ClusterError::Resource("Oracle spill root missing".to_owned()))?;
        let oracle = root.path().join("oracle-spill");
        Ok((
            oracle.join("oracle-runtime-stale").exists(),
            oracle.join("unrelated-sibling").exists(),
        ))
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
        let mut storage = Vec::new();
        let node_ids = self.servers.keys().copied().collect::<Vec<_>>();
        let expected_servers = node_ids.len();
        let mut stopped_servers = 0_usize;
        for node_id in node_ids {
            if let Some(slot) = self.servers.get_mut(&node_id)
                && let Some(server) = slot.take()
            {
                // No Scribe seal or flush may run while this node's forge worker
                // is still live. Cancellation only signals the worker, which stays
                // live inside its claim loop until it is joined, so a pre-shutdown
                // flush would plant a fresh generation the live worker claims but
                // cannot finish, stranding a non-terminal Forge claim. There is
                // therefore no pre-shutdown flush here: `server.shutdown_and_inspect`
                // cancels, joins the serve task (which runs the production
                // drain-then-seal to completion, ending worker liveness), and only
                // then drains the Scribe. The inflight read below is a pure
                // observation of the admission owner and plants nothing.
                if let Some(scribe) = server.bifrost_scribe() {
                    scribe_inflight =
                        scribe_inflight.saturating_add(scribe.inflight_items_for_test() as u64);
                }
                match server.shutdown_and_inspect().await {
                    Ok(server_inspection) => {
                        stopped_servers = stopped_servers.saturating_add(1);
                        listeners_stopped &= server_inspection.listeners_stopped;
                        supervised_tasks =
                            supervised_tasks.saturating_add(server_inspection.supervised_tasks);
                        storage.extend(server_inspection.storage);
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
        let inspection = self.oracle_inspection().await;
        match (first_error, inspection) {
            (Some(shutdown), Err(inspection)) => Err(ClusterError::Shutdown(format!(
                "{shutdown}; post-shutdown inspection failed: {inspection}"
            ))),
            (Some(shutdown), Ok(_)) => Err(ClusterError::Shutdown(shutdown)),
            (None, Err(error)) => Err(error),
            (None, Ok(inspection)) => Ok(ClusterShutdownInspection {
                servers_stopped: stopped_servers == expected_servers,
                listeners_stopped,
                scribe_queued,
                scribe_inflight,
                scribe_wal_streams,
                forge_active_claims: inspection.forge_active_claims,
                forge_active_attempts: inspection.forge_active_attempts,
                supervised_tasks,
                storage,
            }),
        }
    }
}

/// Return whether a durable row is the sole shape eligible for panic clock advance.
fn is_exact_reclaimed_panic_row(row: &(String, i32, Option<String>, bool, bool)) -> bool {
    row == &("retryable".to_owned(), 1, None, true, true)
}

/// Maps one exact concrete role set to its public process target.
fn process_target_for_roles(roles: &BTreeSet<BifrostRuntimeRole>) -> Option<BifrostTarget> {
    [
        BifrostTarget::All,
        BifrostTarget::Server,
        BifrostTarget::Oracle,
        BifrostTarget::Scribe,
        BifrostTarget::ForgeWorker,
    ]
    .into_iter()
    .find(|target| {
        wyrd_server::config::BifrostRoles::for_target(*target)
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            == *roles
    })
}

/// Classify a validated concrete descriptor for legacy lane reporting.
fn classify_topology(spec: &BifrostClusterSpec) -> BifrostTopology {
    match spec.nodes.len() {
        1 => BifrostTopology::OnePod,
        2 => BifrostTopology::TwoPod,
        6 if spec.nodes[..3].iter().all(|node| {
            node.roles.contains(&BifrostRuntimeRole::Scribe)
                && node.roles.contains(&BifrostRuntimeRole::ForgeCoordinator)
                && node.roles.contains(&BifrostRuntimeRole::Oracle)
                && node.roles.len() == 3
        }) && spec.nodes[3..].iter().all(|node| {
            node.roles.len() == 1 && node.roles.contains(&BifrostRuntimeRole::ForgeWorker)
        }) =>
        {
            BifrostTopology::ThreeServersThreeForgeWorkers
        }
        4 if spec.nodes.first().is_some_and(|node| {
            node.roles.contains(&BifrostRuntimeRole::Scribe)
                && node.roles.contains(&BifrostRuntimeRole::ForgeCoordinator)
                && node.roles.contains(&BifrostRuntimeRole::Oracle)
                && node.roles.len() == 3
        }) && spec.nodes[1..].iter().all(|node| {
            node.roles.len() == 1 && node.roles.contains(&BifrostRuntimeRole::ForgeWorker)
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

/// Loads one runtime peer identity from the harness-written PEM paths.
///
/// Tests hold the material as files because that is how a deployment supplies
/// it; this is the single place the harness turns those paths back into the
/// production [`BifrostPeerTls`] owner.
///
/// # Errors
///
/// Returns [`ClusterError::Resource`] when a PEM file is unreadable or the
/// private key is not valid PEM text.
fn peer_tls_from_paths(tls: &TestBifrostPeerTls) -> Result<BifrostPeerTls, ClusterError> {
    let read = |path: &std::path::Path| {
        std::fs::read(path).map_err(|error| ClusterError::Resource(error.to_string()))
    };
    let key = String::from_utf8(read(&tls.private_key_path)?)
        .map_err(|error| ClusterError::Resource(error.to_string()))?;
    Ok(BifrostPeerTls::new(
        read(&tls.ca_path)?,
        tls.server_name.clone(),
        read(&tls.certificate_path)?,
        secrecy::SecretString::from(key),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Durable cleanup inspection remains ordered after every server shutdown
    /// attempt and preserves shutdown-error precedence.
    #[test]
    fn shutdown_durable_inspection_is_post_server_loop() {
        let source = include_str!("cluster.rs");
        let start = source
            .find("pub async fn shutdown_and_inspect(mut self)")
            .expect("cluster shutdown owner exists");
        let end = source[start..]
            .find("fn process_target_for_roles")
            .map(|offset| start + offset)
            .expect("shutdown owner ends before topology helper");
        let function = &source[start..end];
        let server_shutdown = function
            .find("server.shutdown_and_inspect().await")
            .expect("server shutdown is awaited");
        let durable_inspection = function
            .rfind("self.oracle_inspection().await")
            .expect("durable state is inspected once");

        assert!(server_shutdown < durable_inspection);
        assert_eq!(
            function.matches("self.oracle_inspection().await").count(),
            1
        );
        assert!(function.contains("match (first_error, inspection)"));
        assert!(function.contains("(Some(shutdown), Err(inspection))"));
    }

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
                .all(|node| process_target_for_roles(&node.roles) == Some(BifrostTarget::Server))
        );
        assert!(
            spec.nodes[3..]
                .iter()
                .all(|node| node.roles == BTreeSet::from([BifrostRuntimeRole::ForgeWorker]))
        );
        assert_eq!(
            classify_topology(&spec),
            BifrostTopology::ThreeServersThreeForgeWorkers
        );
    }

    /// The dedicated-worker descriptor keeps the serving coordinator out of the worker pool.
    ///
    /// # Panics
    ///
    /// Panics when validation, process-target mapping, worker isolation, or
    /// legacy topology classification diverges from the four-node contract.
    #[test]
    fn dedicated_forge_workers_descriptor_is_exact() {
        let spec = BifrostClusterSpec::dedicated_forge_workers();
        spec.validate().expect("dedicated descriptor validates");
        assert_eq!(spec.nodes.len(), 4);
        assert_eq!(
            process_target_for_roles(&spec.nodes[0].roles),
            Some(BifrostTarget::Server)
        );
        assert!(
            spec.nodes[1..]
                .iter()
                .all(|node| node.roles == BTreeSet::from([BifrostRuntimeRole::ForgeWorker]))
        );
        assert_eq!(
            classify_topology(&spec),
            BifrostTopology::DedicatedForgeWorkers
        );
    }

    /// A stopped node retains raw observations and re-derives the same clean plan.
    #[tokio::test]
    async fn cluster_restart_rederives_same_plan_from_retained_snapshot() {
        let observation = SystemResourceSnapshot {
            memory_limit_bytes: 1024 * 1024 * 1024,
            effective_cpu: 3,
            scratch_capacity_bytes: 1280 * 1024 * 1024,
            scratch_available_bytes: 1280 * 1024 * 1024,
            memory_source: vala_bifrost_redux::resources::ResourceSource::Injected,
            cpu_source: vala_bifrost_redux::resources::ResourceSource::Injected,
        };
        let mut cluster = WyrdTestCluster::start_spec(
            BifrostClusterSpec::one_mixed().with_system_resources(observation),
        )
        .await
        .expect("cluster starts");
        let node_id = cluster.ready_query_nodes()[0];
        let original = cluster.server_by_node(node_id).expect("running node");
        let original_identity = original.postgres_pool_identity();
        let original_resources = original
            .state()
            .bifrost_resources()
            .expect("original global resources")
            .snapshot()
            .expect("original resource snapshot");
        let wal_root = cluster.wal_dirs().next().expect("WAL root").to_path_buf();
        cluster.stop_node(node_id).await.expect("node stops");
        assert!(cluster.server_by_node(node_id).is_none());
        cluster.restart_node(node_id).await.expect("node restarts");
        let server = cluster.server_by_node(node_id).expect("node is running");
        assert_ne!(server.postgres_pool_identity(), original_identity);
        assert_eq!(cluster.wal_dirs().next(), Some(wal_root.as_path()));
        let restarted_resources = server
            .state()
            .bifrost_resources()
            .expect("restarted global resources")
            .snapshot()
            .expect("restarted resource snapshot");
        assert_eq!(restarted_resources.plan, original_resources.plan);
        assert_eq!(restarted_resources.scribe_memory_used_bytes, 0);
        assert_eq!(restarted_resources.elastic_memory_used_bytes, 0);
        assert_eq!(restarted_resources.scratch_used_bytes, 0);
        assert!(!restarted_resources.oracle_query_active);
        let mut system_conn = cluster
            .fixture
            .tenant_conn_for(DataTenantId::SYSTEM_OWNER)
            .await
            .expect("system tenant connection");
        let assigned: Vec<(String, serde_json::Value)> = sqlx::query_as(
            "SELECT r.name, r.permissions FROM wyrd.auth_service_accounts sa \
             JOIN wyrd.auth_service_account_roles sar ON sar.data_tenant_id = sa.data_tenant_id AND sar.service_account_id = sa.id \
             JOIN wyrd.auth_roles r ON r.data_tenant_id = sar.data_tenant_id AND r.id = sar.role_id \
             WHERE sa.name = 'bifrost-peer' ORDER BY r.name",
        )
        .fetch_all(&mut **system_conn.transaction())
        .await
        .expect("Oracle peer role reads");
        drop(system_conn);
        assert_eq!(assigned.len(), 1);
        assert_eq!(assigned[0].0, "bifrost_peer");
        let stored_permissions: Vec<Permission> =
            serde_json::from_value(assigned[0].1.clone()).expect("permissions decode");
        let expected = vec![Permission::bifrost_peer_invoke()];
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
        // The spec binds two Oracle-only nodes and one Scribe-only node, so the
        // two endpoint sets are disjoint. Asserting the configured node count on
        // both sides would pass only if every node ran every role, which is the
        // opposite of what role separation means.
        assert_eq!(cluster.ready_ingest_nodes().len(), 1);
        assert_eq!(cluster.ready_query_nodes().len(), 2);
        assert!(cluster.ready_ingest_nodes().iter().all(|node| {
            cluster
                .server_by_node(*node)
                .and_then(WyrdTestServer::bifrost_scribe)
                .is_some()
        }));
        assert!(cluster.ready_query_nodes().iter().all(|node| {
            cluster
                .server_by_node(*node)
                .is_some_and(|server| server.bifrost_scribe().is_none())
        }));
        cluster.shutdown().await.expect("cluster shuts down");
    }

    /// Prometheus parsing preserves closed label keys and normalizes histograms.
    #[test]
    fn metric_parser_preserves_exact_labels() {
        let sample = parse_metric_sample(
            "oracle_query_duration_seconds_bucket{class=\"interactive\",outcome=\"success\",le=\"1\"}",
            2.0,
        )
        .expect("metric parses");
        assert_eq!(sample.family, "oracle_query_duration_seconds");
        assert_eq!(sample.labels["class"], "interactive");
        assert_eq!(sample.labels["outcome"], "success");
    }

    /// Panic recovery clock admission accepts only the exact reclaimed row shape.
    #[test]
    fn panic_recovery_clock_requires_exact_reclaimed_row() {
        let exact = ("retryable".to_owned(), 1, None, true, true);
        assert!(is_exact_reclaimed_panic_row(&exact));
        for rejected in [
            ("running".to_owned(), 1, None, false, false),
            ("claimed".to_owned(), 1, None, false, false),
            (
                "retryable".to_owned(),
                1,
                Some("storage_health".to_owned()),
                true,
                true,
            ),
            ("retryable".to_owned(), 0, None, true, true),
            ("retryable".to_owned(), 2, None, true, true),
            ("retryable".to_owned(), 1, None, false, true),
            ("retryable".to_owned(), 1, None, true, false),
            ("failed".to_owned(), 1, None, true, true),
        ] {
            assert!(!is_exact_reclaimed_panic_row(&rejected));
        }
    }
}
