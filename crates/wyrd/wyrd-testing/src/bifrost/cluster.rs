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
use vala_bifrost_redux::oracle::dispatcher::OraclePeerCredentials;
use vala_bifrost_redux::scribe::admission::AdmissionConfig;
use vala_bifrost_redux::scribe::tail_rpc::TonicTailReadTransport;
use wyrd_auth::seed::seed_builtin_roles_for_tenant;
use wyrd_client::WyrdClient;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_dev_fixtures::pg::PgFixture;
#[cfg(test)]
use wyrd_runtime::{Permission, PermissionSet};
use wyrd_server::app::metrics::install_recorder;
use wyrd_server::config::BifrostRuntimeRole;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::NodeId;
use wyrd_storage::{BackendConfig, StorageHandle, StorageSettings};
use wyrd_telemetry::{CapturedSpan, TelemetryConfig, TelemetryGuard, TestTraceCapture};
use wyrd_tonic::wyrd::v1::scribe_tail_service_client::ScribeTailServiceClient;

use crate::Bootstrap;
use crate::server::{
    WyrdTestServer, WyrdTestServerBuilder, WyrdTestServerError, provision_oracle_peer_credentials,
    test_catalog, test_redux_catalog,
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
}

impl BifrostTopology {
    /// Return the concrete node descriptor for this named topology.
    fn spec(self) -> BifrostClusterSpec {
        match self {
            Self::OnePod => BifrostClusterSpec::one_mixed(),
            Self::ThreePod => BifrostClusterSpec::three_mixed(),
            Self::RoleSeparated => BifrostClusterSpec::role_separated(),
            Self::SixPod => BifrostClusterSpec::six_capacity(),
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

    /// Construct three mixed nodes.
    #[must_use]
    pub fn three_mixed() -> Self {
        Self::mixed(3)
    }

    /// Construct one Scribe/Forge node and two Oracle nodes.
    #[must_use]
    pub fn role_separated() -> Self {
        Self {
            nodes: vec![
                Self::node(1, [BifrostRuntimeRole::Scribe, BifrostRuntimeRole::Forge]),
                Self::node(2, [BifrostRuntimeRole::Oracle]),
                Self::node(3, [BifrostRuntimeRole::Oracle]),
            ],
        }
    }

    /// Construct six mixed capacity nodes.
    #[must_use]
    pub fn six_capacity() -> Self {
        Self::mixed(6)
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
    /// Number of durable audit rows observed across tenants.
    pub audit_rows: u64,
    /// Metric families present in the production recorder snapshot.
    pub metric_families: BTreeSet<String>,
    /// Finished span names present in the production tracing capture.
    pub span_names: BTreeSet<String>,
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

    /// Return every metric family currently rendered by the production recorder.
    #[must_use]
    fn families(&self) -> BTreeSet<String> {
        rendered_values(&self.metrics.render())
            .into_iter()
            .filter_map(|(series, value)| parse_metric_sample(&series, value).ok())
            .map(|sample| sample.family)
            .collect()
    }

    /// Return every finished span name currently held by the exporter.
    #[must_use]
    fn span_names(&self) -> BTreeSet<String> {
        self.traces
            .finished_since(0)
            .into_iter()
            .map(|span| span.name)
            .collect()
    }
}

/// Process-global telemetry resources kept alive for every cluster node.
struct ProcessTelemetry {
    /// Guard retaining the one SDK tracer provider.
    guard: Arc<TelemetryGuard>,
    /// Cloneable inspection handle.
    capture: OracleTelemetryCapture,
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
    let _ = PROCESS_TELEMETRY.set(ProcessTelemetry {
        guard: Arc::new(guard),
        capture: OracleTelemetryCapture { metrics, traces },
    });
    PROCESS_TELEMETRY
        .get()
        .ok_or_else(|| ClusterError::Telemetry("process telemetry installation raced".to_owned()))
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
    storage_root: Arc<tempfile::TempDir>,
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
    /// Scoped transport fault state.
    faults: OracleFaultController,
    /// Read-only process telemetry handle.
    telemetry: OracleTelemetryCapture,
    /// Valid SYSTEM_OWNER Service credential retained across node restarts.
    oracle_peer_credentials: Arc<dyn OraclePeerCredentials>,
}

impl WyrdTestCluster {
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
        Self::start_spec_with_options(spec, Duration::ZERO, None).await
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
        Self::start_spec_with_options(topology.spec(), wal_sync_delay, None).await
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
        Self::start_spec_with_options(topology.spec(), Duration::ZERO, Some(admission)).await
    }

    /// Build shared dependencies, retained roots, and every node slot.
    async fn start_spec_with_options(
        spec: BifrostClusterSpec,
        wal_sync_delay: Duration,
        scribe_admission: Option<AdmissionConfig>,
    ) -> Result<Self, ClusterError> {
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

        let storage_root = Arc::new(
            tempfile::tempdir().map_err(|error| ClusterError::Resource(error.to_string()))?,
        );
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
        let oracle_peer_credentials =
            provision_oracle_peer_credentials(Arc::clone(&fixture)).await?;
        let topology = classify_topology(&spec);
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
            nodes.insert(
                node.node_id,
                NodeResources {
                    spec: node,
                    wal_root,
                    spill_root,
                    http_addr: reserve_loopback_addr()?,
                    grpc_addr: reserve_loopback_addr()?,
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
            faults: OracleFaultController::default(),
            telemetry: process.capture.clone(),
            oracle_peer_credentials,
        };
        let node_ids = cluster.nodes.keys().copied().collect::<Vec<_>>();
        for node_id in node_ids {
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
        if let Some(admission) = self.scribe_admission {
            builder = builder.with_scribe_admission_for_test(admission);
        }
        Ok(builder
            .start_with_resources(
                Arc::clone(&self.fixture),
                Arc::clone(&self.storage),
                Arc::clone(&self.catalog),
                Arc::clone(&self.redux_catalog),
                Some(Arc::clone(&self.storage_root)),
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
    pub const fn telemetry(&self) -> &OracleTelemetryCapture {
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
        Ok(OracleInspection {
            memberships,
            active_leases: u64::try_from(active_leases)
                .map_err(|error| ClusterError::Resource(error.to_string()))?,
            slots_in_use: u64::try_from(slots_in_use)
                .map_err(|error| ClusterError::Resource(error.to_string()))?,
            audit_rows: u64::try_from(audit_rows)
                .map_err(|error| ClusterError::Resource(error.to_string()))?,
            metric_families: self.telemetry.families(),
            span_names: self.telemetry.span_names(),
        })
    }

    /// Register every running Scribe stream with every running Oracle via authenticated tonic.
    ///
    /// Journeys call this after registering a table and before a Fused query so
    /// the real Oracle fence/page workflow crosses the bound private gRPC service.
    ///
    /// # Errors
    ///
    /// Returns an error when service bootstrap, token exchange, tonic connect,
    /// stream inspection, or transport construction fails.
    pub async fn register_live_tail(
        &self,
        table: &str,
        event_day: wyrd_spec::vala::api::EventDay,
    ) -> Result<(), ClusterError> {
        self.register_live_tail_for_tenant(self.data_tenant_id(), table, event_day)
            .await
    }

    /// Register one tenant's running Scribe streams with every running Oracle.
    ///
    /// The private bearer and route are scoped to the same tenant so identical
    /// logical table names cannot cross tenant boundaries.
    ///
    /// # Errors
    ///
    /// Returns an error when service bootstrap, token exchange, tonic connect,
    /// stream inspection, or transport construction fails.
    pub async fn register_live_tail_for_tenant(
        &self,
        tenant: wyrd_spec::DataTenantId,
        table: &str,
        event_day: wyrd_spec::vala::api::EventDay,
    ) -> Result<(), ClusterError> {
        for (index, scribe_server) in self
            .servers()
            .filter(|server| server.bifrost_scribe().is_some())
            .enumerate()
        {
            let bootstrap = scribe_server
                .bootstrap_service_in_tenant(tenant, &format!("oracle-tail-{index}"), &["admin"])
                .await
                .map_err(|error| ClusterError::Resource(error.to_string()))?;
            let api_key = match bootstrap {
                Bootstrap::Machine { api_key, .. } => api_key,
                Bootstrap::User { .. } => {
                    return Err(ClusterError::Resource(
                        "tail service bootstrap returned a user".to_owned(),
                    ));
                }
            };
            let grpc_endpoint = scribe_server
                .grpc_url()
                .ok_or_else(|| ClusterError::Resource("Scribe gRPC endpoint missing".to_owned()))?;
            let http_endpoint = scribe_server
                .base_url()
                .ok_or_else(|| ClusterError::Resource("Scribe HTTP endpoint missing".to_owned()))?;
            let client = WyrdClient::with_config(ClientConfig {
                grpc: GrpcConfig {
                    endpoint: grpc_endpoint.clone(),
                    connect_retries: 0,
                    ..GrpcConfig::default()
                },
                http: HttpConfig {
                    base_url: http_endpoint.to_owned(),
                    ..HttpConfig::default()
                },
                api_key: Some(api_key),
                ..ClientConfig::default()
            })
            .map_err(|error| ClusterError::Resource(error.to_string()))?;
            let bearer = client
                .auth()
                .bearer()
                .await
                .map_err(|error| ClusterError::Resource(error.to_string()))?;
            let channel = wyrd_tonic::tonic::transport::Endpoint::from_shared(grpc_endpoint)
                .map_err(|error| ClusterError::Resource(error.to_string()))?
                .connect()
                .await
                .map_err(|error| ClusterError::Resource(error.to_string()))?;
            let transport = Arc::new(
                TonicTailReadTransport::new(ScribeTailServiceClient::new(channel), bearer.expose())
                    .map_err(|error| ClusterError::Resource(error.to_string()))?,
            );
            let stream = scribe_server
                .bifrost_scribe()
                .ok_or_else(|| ClusterError::Resource("Scribe disappeared".to_owned()))?
                .tail_service()
                .map_err(|error| ClusterError::Resource(error.to_string()))?
                .stream();
            let writer_epoch = u64::try_from(stream.writer_epoch.as_i64())
                .map_err(|error| ClusterError::Resource(error.to_string()))?;
            for query_server in self
                .servers()
                .filter(|server| server.state().bifrost_query().is_some())
            {
                query_server
                    .state()
                    .bifrost_query()
                    .expect("filtered Oracle runtime remains present")
                    .oracle()
                    .tail_transports()
                    .insert_live_stream_for_tenant(
                        tenant,
                        table,
                        NodeId::new(stream.node_id.as_uuid()),
                        writer_epoch,
                        event_day.clone(),
                        transport.clone(),
                    );
            }
        }
        Ok(())
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
    pub fn servers(&self) -> impl Iterator<Item = &WyrdTestServer> {
        self.servers.values().filter_map(Option::as_ref)
    }

    /// Return one running pod by stable order index.
    #[must_use]
    pub fn server(&self, index: usize) -> Option<&WyrdTestServer> {
        self.servers().nth(index)
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
    pub async fn shutdown(mut self) -> Result<(), ClusterError> {
        let mut first_error = None;
        let node_ids = self.servers.keys().copied().collect::<Vec<_>>();
        for node_id in node_ids {
            if self.servers.get(&node_id).is_some_and(Option::is_some)
                && let Err(error) = self.stop_node(node_id).await
            {
                first_error.get_or_insert(error.to_string());
            }
        }
        first_error.map_or(Ok(()), |error| Err(ClusterError::Shutdown(error)))
    }
}

/// Classify a validated concrete descriptor for legacy lane reporting.
fn classify_topology(spec: &BifrostClusterSpec) -> BifrostTopology {
    match spec.nodes.len() {
        1 => BifrostTopology::OnePod,
        3 if spec
            .nodes
            .iter()
            .any(|node| node.roles != BifrostClusterSpec::three_mixed().nodes[0].roles) =>
        {
            BifrostTopology::RoleSeparated
        }
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

    /// A stopped node retains its physical roots and restarts into the same slot.
    #[tokio::test]
    async fn restartable_node_slot_preserves_roots() {
        let mut cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed())
            .await
            .expect("cluster starts");
        let node_id = cluster.ready_query_nodes()[0];
        let wal_root = cluster.wal_dirs().next().expect("WAL root").to_path_buf();
        cluster.stop_node(node_id).await.expect("node stops");
        assert!(cluster.server_by_node(node_id).is_none());
        cluster.restart_node(node_id).await.expect("node restarts");
        let server = cluster.server_by_node(node_id).expect("node is running");
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

    /// Role-separated descriptors expose only matching live endpoint sets.
    #[tokio::test]
    async fn role_separated_readiness_matches_configured_roles() {
        let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::role_separated())
            .await
            .expect("role-separated cluster starts");
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
