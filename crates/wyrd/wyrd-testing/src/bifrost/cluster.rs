//! Shared-resource, independently bound Bifrost test cluster.

use std::sync::{Arc, OnceLock};

use opendal::Operator;
use thiserror::Error;
use vala_bifrost::catalog::WyrdCatalog;
use vala_bifrost_redux::catalog::BifrostCatalog;
use vala_bifrost_redux::forge::ForgeConfig;
use vala_bifrost_redux::forge::ForgeWorkerCompletionObserver;
use vala_bifrost_redux::scribe::admission::AdmissionConfig;
use wyrd_auth::seed::seed_builtin_roles_for_tenant;
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_server::config::ForgeProcessRole;
use wyrd_server::{WyrdTelemetryRuntime, install_capture_runtime};
use wyrd_spec::DataTenantId;
use wyrd_storage::{BackendConfig, StorageHandle, StorageSettings};
use wyrd_telemetry::TelemetryConfig;

use crate::bifrost::forge_harness::CommitUncertaintyCatalog;
use crate::bifrost::telemetry::ForgeTelemetryCapture;
use crate::server::{
    WyrdTestServer, WyrdTestServerBuilder, WyrdTestServerError, test_catalog, test_redux_catalog,
};

/// Supported role topology for a Bifrost cluster journey.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BifrostTopology {
    /// Run one full Gate, Scribe, Forge, and Oracle pod.
    OnePod,
    /// Run three full Gate, Scribe, Forge, and Oracle pods.
    ThreePod,
    /// Run one API and scheduler pod with three worker-only Forge pods.
    DedicatedForgeWorkers,
}

impl BifrostTopology {
    fn validate(self, pods: usize) -> Result<(), ClusterError> {
        let expected = match self {
            Self::OnePod => 1,
            Self::ThreePod => 3,
            Self::DedicatedForgeWorkers => 4,
        };
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

/// Errors raised while constructing or stopping a shared test cluster.
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
    /// A shared Postgres, catalog, object-store, or WAL resource failed.
    #[error("cluster resource setup failed: {0}")]
    Resource(String),
    /// A server failed to start or bind.
    #[error("cluster server failed: {0}")]
    Server(#[from] WyrdTestServerError),
    /// A server failed during explicit teardown.
    #[error("cluster shutdown failed: {0}")]
    Shutdown(String),
}

/// A real multi-pod Bifrost test topology.
///
/// All pods share one fixture-owned Postgres database, one object store, and
/// one Iceberg catalog. HTTP and gRPC listeners are independently bound, and
/// each pod receives a durable local WAL directory for workload drivers.
pub struct WyrdTestCluster {
    servers: Vec<WyrdTestServer>,
    fixture: Arc<PgFixture>,
    storage: Arc<StorageHandle>,
    storage_root: Arc<tempfile::TempDir>,
    wal_dirs: Vec<tempfile::TempDir>,
    topology: BifrostTopology,
    /// Optional shared observer for real dedicated worker completions.
    forge_completion_observer: Option<ForgeWorkerCompletionObserver>,
    /// Optional post-commit uncertainty wrapper shared by every Forge role.
    commit_uncertainty_catalog: Option<Arc<CommitUncertaintyCatalog>>,
    /// Read-only capture over the process-global production telemetry runtime.
    telemetry: ForgeTelemetryCapture,
}

/// Process-global runtime retained before any test cluster role starts.
struct ProcessTelemetry {
    /// Production backend lifetime owner.
    runtime: Arc<WyrdTelemetryRuntime>,
    /// Read-only capture paired with that same runtime.
    capture: ForgeTelemetryCapture,
}

/// One production-shaped exporter installation for serialized cluster tests.
static PROCESS_TELEMETRY: OnceLock<ProcessTelemetry> = OnceLock::new();

/// Install or return the process runtime used by real test-cluster roles.
fn process_telemetry() -> Result<&'static ProcessTelemetry, ClusterError> {
    if let Some(runtime) = PROCESS_TELEMETRY.get() {
        return Ok(runtime);
    }
    let (runtime, traces) = install_capture_runtime(TelemetryConfig {
        service_name: Some("wyrd-test-cluster".to_owned()),
        ..TelemetryConfig::default()
    })
    .map_err(|error| ClusterError::Resource(error.to_string()))?;
    let runtime = Arc::new(runtime);
    let capture = ForgeTelemetryCapture::new(runtime.prometheus(), traces);
    let _ = PROCESS_TELEMETRY.set(ProcessTelemetry { runtime, capture });
    PROCESS_TELEMETRY
        .get()
        .ok_or_else(|| ClusterError::Resource("process telemetry installation raced".to_owned()))
}

/// Private role-specific inputs for one shared-resource cluster construction.
///
/// The settings keep the public fixture constructors focused on their named
/// topology while the one resource owner receives a coherent process-role
/// configuration.
struct ClusterRoleSettings {
    /// Optional test-tier delay applied to each role's Scribe WAL sync.
    wal_sync_delay: std::time::Duration,
    /// Optional test-tier Scribe admission limits shared by every role.
    scribe_admission: Option<AdmissionConfig>,
    /// One production role for every cluster process.
    roles: Vec<ForgeProcessRole>,
    /// Optional passive observer shared by supervised Forge workers.
    forge_completion_observer: Option<ForgeWorkerCompletionObserver>,
    /// Supervisor interval used by every role's real Forge loop.
    forge_interval: std::time::Duration,
    /// Whether Forge roles receive the deterministic post-commit catalog wrapper.
    inject_commit_uncertainty: bool,
    /// Optional complete Forge limits profile validated by normal role startup.
    forge_config: Option<ForgeConfig>,
}

impl WyrdTestCluster {
    /// Return the process production telemetry capture shared by this cluster.
    #[must_use]
    pub const fn telemetry(&self) -> &ForgeTelemetryCapture {
        &self.telemetry
    }

    /// Start a one- or three-pod cluster over shared real dependencies.
    ///
    /// # Errors
    /// Returns an error when Postgres, object storage, Iceberg, a WAL
    /// directory, or an independently bound server cannot be created.
    pub async fn start(pods: usize, topology: BifrostTopology) -> Result<Self, ClusterError> {
        Self::start_with_wal_sync_delay(pods, topology, std::time::Duration::ZERO).await
    }

    /// Start one real scheduler/server and three real worker-only processes.
    ///
    /// All four processes share the fixture Postgres, Iceberg catalog, and
    /// object store. The worker-only processes use the production
    /// `ForgeWorker` role and therefore must not bind a public Wyrd listener.
    ///
    /// # Errors
    ///
    /// Returns [`ClusterError`] when a shared dependency or any role-specific
    /// process cannot start.
    pub async fn start_with_dedicated_forge_workers() -> Result<Self, ClusterError> {
        Self::start_with_dedicated_forge_workers_with_interval_for_test(
            std::time::Duration::from_secs(60),
        )
        .await
    }

    /// Start dedicated Forge roles with an explicit supervised scheduler interval.
    ///
    /// This test-tier constructor is for topology journeys that must observe
    /// real scheduler lease takeover without invoking a scheduler directly.
    ///
    /// # Errors
    ///
    /// Returns [`ClusterError`] when a shared dependency or any role-specific
    /// process cannot start.
    pub async fn start_with_dedicated_forge_workers_with_interval_for_test(
        forge_interval: std::time::Duration,
    ) -> Result<Self, ClusterError> {
        let observer = ForgeWorkerCompletionObserver::new();
        Self::start_with_wal_sync_delay_and_admission_and_roles(
            4,
            BifrostTopology::DedicatedForgeWorkers,
            ClusterRoleSettings {
                wal_sync_delay: std::time::Duration::ZERO,
                scribe_admission: None,
                roles: vec![
                    ForgeProcessRole::Server,
                    ForgeProcessRole::ForgeWorker,
                    ForgeProcessRole::ForgeWorker,
                    ForgeProcessRole::ForgeWorker,
                ],
                forge_completion_observer: Some(observer),
                forge_interval,
                inject_commit_uncertainty: false,
                forge_config: None,
            },
        )
        .await
    }

    /// Start one embedded `all` process with a real supervised completion observer.
    ///
    /// This is the baseline topology for comparisons with dedicated worker
    /// processes. It shares the same cluster resource construction and only
    /// changes the production role roster.
    ///
    /// # Errors
    ///
    /// Returns [`ClusterError`] when the shared dependencies or embedded
    /// server supervisor cannot start.
    pub async fn start_with_embedded_forge_observer() -> Result<Self, ClusterError> {
        let observer = ForgeWorkerCompletionObserver::new();
        Self::start_with_wal_sync_delay_and_admission_and_roles(
            1,
            BifrostTopology::OnePod,
            ClusterRoleSettings {
                wal_sync_delay: std::time::Duration::ZERO,
                scribe_admission: None,
                roles: vec![ForgeProcessRole::All],
                forge_completion_observer: Some(observer),
                forge_interval: std::time::Duration::from_secs(60),
                inject_commit_uncertainty: false,
                forge_config: None,
            },
        )
        .await
    }

    /// Start dedicated Forge roles sharing one deterministic post-commit catalog wrapper.
    ///
    /// The public server/query catalog remains the normal catalog. Only the
    /// real supervised Forge roles receive the wrapper so recovery exercises a
    /// production worker's uncertain-commit path.
    ///
    /// # Errors
    ///
    /// Returns [`ClusterError`] when shared dependencies or a role supervisor
    /// cannot start.
    pub async fn start_with_dedicated_forge_workers_with_uncertainty_for_test()
    -> Result<Self, ClusterError> {
        let observer = ForgeWorkerCompletionObserver::new();
        Self::start_with_wal_sync_delay_and_admission_and_roles(
            4,
            BifrostTopology::DedicatedForgeWorkers,
            ClusterRoleSettings {
                wal_sync_delay: std::time::Duration::ZERO,
                scribe_admission: None,
                roles: vec![
                    ForgeProcessRole::Server,
                    ForgeProcessRole::ForgeWorker,
                    ForgeProcessRole::ForgeWorker,
                    ForgeProcessRole::ForgeWorker,
                ],
                forge_completion_observer: Some(observer),
                forge_interval: std::time::Duration::from_secs(60),
                inject_commit_uncertainty: true,
                forge_config: None,
            },
        )
        .await
    }

    /// Start dedicated Forge roles with one complete deterministic limit profile.
    ///
    /// The profile is validated by the normal production [`Forge::new`] path
    /// for every server and worker role; no planner or worker behavior is
    /// replaced by this test constructor.
    ///
    /// # Errors
    ///
    /// Returns [`ClusterError`] when the configuration or a shared role
    /// dependency cannot start.
    pub async fn start_with_dedicated_forge_workers_with_config_for_test(
        config: ForgeConfig,
    ) -> Result<Self, ClusterError> {
        let observer = ForgeWorkerCompletionObserver::new();
        Self::start_with_wal_sync_delay_and_admission_and_roles(
            4,
            BifrostTopology::DedicatedForgeWorkers,
            ClusterRoleSettings {
                wal_sync_delay: std::time::Duration::ZERO,
                scribe_admission: None,
                roles: vec![
                    ForgeProcessRole::Server,
                    ForgeProcessRole::ForgeWorker,
                    ForgeProcessRole::ForgeWorker,
                    ForgeProcessRole::ForgeWorker,
                ],
                forge_completion_observer: Some(observer),
                forge_interval: std::time::Duration::from_secs(60),
                inject_commit_uncertainty: false,
                forge_config: Some(config),
            },
        )
        .await
    }

    /// Start a cluster with an explicit WAL fsync delay for benchmark cases.
    pub async fn start_with_wal_sync_delay(
        pods: usize,
        topology: BifrostTopology,
        wal_sync_delay: std::time::Duration,
    ) -> Result<Self, ClusterError> {
        Self::start_with_wal_sync_delay_and_admission(pods, topology, wal_sync_delay, None).await
    }

    /// Start a cluster with explicit test-tier Scribe admission limits.
    pub async fn start_with_admission(
        pods: usize,
        topology: BifrostTopology,
        admission: AdmissionConfig,
    ) -> Result<Self, ClusterError> {
        Self::start_with_wal_sync_delay_and_admission(
            pods,
            topology,
            std::time::Duration::ZERO,
            Some(admission),
        )
        .await
    }

    async fn start_with_wal_sync_delay_and_admission(
        pods: usize,
        topology: BifrostTopology,
        wal_sync_delay: std::time::Duration,
        scribe_admission: Option<AdmissionConfig>,
    ) -> Result<Self, ClusterError> {
        Self::start_with_wal_sync_delay_and_admission_and_roles(
            pods,
            topology,
            ClusterRoleSettings {
                wal_sync_delay,
                scribe_admission,
                roles: vec![ForgeProcessRole::All; pods],
                forge_completion_observer: None,
                forge_interval: std::time::Duration::from_secs(60),
                inject_commit_uncertainty: false,
                forge_config: None,
            },
        )
        .await
    }

    /// Construct a shared-resource cluster with one production role per pod.
    ///
    /// This private constructor centralizes the resource graph so every
    /// topology exercises the same Postgres, catalog, object store, and bound
    /// `WyrdServer` supervisor. Only the real production process role differs.
    ///
    /// # Errors
    ///
    /// Returns [`ClusterError`] when the selected topology, role roster, or a
    /// shared dependency is invalid, or when any bound server fails to start.
    async fn start_with_wal_sync_delay_and_admission_and_roles(
        pods: usize,
        topology: BifrostTopology,
        settings: ClusterRoleSettings,
    ) -> Result<Self, ClusterError> {
        topology.validate(pods)?;
        let telemetry = process_telemetry()?;
        if settings.roles.len() != pods {
            return Err(ClusterError::Resource(format!(
                "cluster role roster has {} entries for {pods} pods",
                settings.roles.len()
            )));
        }
        if pods == 0 {
            return Err(ClusterError::Resource(
                "cluster must contain at least one pod".to_owned(),
            ));
        }

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
            presign_ttl: std::time::Duration::from_secs(600),
            part_size_bytes: 16 * 1024 * 1024,
            multipart_threshold_bytes: 100 * 1024 * 1024,
            public_base_url: Some("https://wyrd.test".to_owned()),
        })
        .await
        .map_err(|error| ClusterError::Resource(error.to_string()))?;
        let catalog: Arc<WyrdCatalog> = test_catalog(&fixture, &storage).await?;
        let redux_catalog: Arc<BifrostCatalog> = test_redux_catalog(&fixture, &storage).await?;
        let commit_uncertainty_catalog = settings
            .inject_commit_uncertainty
            .then(|| CommitUncertaintyCatalog::new(redux_catalog.iceberg_catalog()));

        let mut wal_dirs = Vec::with_capacity(pods);
        let mut servers = Vec::with_capacity(pods);
        for role in settings.roles {
            let wal_dir =
                tempfile::tempdir().map_err(|error| ClusterError::Resource(error.to_string()))?;
            let builder = WyrdTestServerBuilder::default()
                .with_wal_sync_delay(settings.wal_sync_delay)
                .with_forge_interval(settings.forge_interval)
                .with_forge_process_role_for_test(role)
                .with_telemetry_for_test(telemetry.runtime.guard());
            let builder = if let Some(config) = &settings.forge_config {
                builder.with_forge_config_for_test(config.clone())
            } else {
                builder
            };
            let builder = if let Some(observer) = &settings.forge_completion_observer {
                builder.with_forge_completion_observer_for_test(observer.clone())
            } else {
                builder
            };
            let builder = if let Some(catalog) = &commit_uncertainty_catalog {
                let forge_catalog: Arc<dyn iceberg::Catalog> = catalog.clone();
                builder.with_forge_catalog_for_test(forge_catalog)
            } else {
                builder
            };
            let builder = if let Some(admission) = settings.scribe_admission {
                builder.with_scribe_admission_for_test(admission)
            } else {
                builder
            };
            let server = builder
                .start_with_resources(
                    Arc::clone(&fixture),
                    Arc::clone(&storage),
                    Arc::clone(&catalog),
                    Arc::clone(&redux_catalog),
                    Some(Arc::clone(&storage_root)),
                )
                .await?
                .bind()
                .await?;
            wal_dirs.push(wal_dir);
            servers.push(server);
        }

        Ok(Self {
            servers,
            fixture,
            storage,
            storage_root,
            wal_dirs,
            topology,
            forge_completion_observer: settings.forge_completion_observer,
            commit_uncertainty_catalog,
            telemetry: telemetry.capture.clone(),
        })
    }

    /// Return the selected topology.
    #[must_use]
    pub const fn topology(&self) -> BifrostTopology {
        self.topology
    }

    /// Return the shared fixture tenant.
    #[must_use]
    pub fn data_tenant_id(&self) -> DataTenantId {
        self.fixture.data_tenant_id()
    }

    /// Return the shared completion observer for a dedicated worker topology.
    #[must_use]
    pub fn forge_completion_observer(&self) -> Option<ForgeWorkerCompletionObserver> {
        self.forge_completion_observer.clone()
    }

    /// Return the shared deterministic catalog control when this topology enabled it.
    #[must_use]
    pub fn commit_uncertainty_catalog(&self) -> Option<Arc<CommitUncertaintyCatalog>> {
        self.commit_uncertainty_catalog.clone()
    }

    /// Seed an additional tenant with the roles needed by a real Bifrost run.
    ///
    /// The tenant row and role seed are created through the same fixture-owned
    /// Postgres path as the initial tenant; benchmarks must not construct a
    /// second pool or schema to represent tenancy.
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

    /// Return all independently bound servers in pod order.
    #[must_use]
    pub fn servers(&self) -> &[WyrdTestServer] {
        &self.servers
    }

    /// Return one pod by index.
    #[must_use]
    pub fn server(&self, index: usize) -> Option<&WyrdTestServer> {
        self.servers.get(index)
    }

    /// Return the real HTTP endpoints for all pods.
    #[must_use]
    pub fn base_urls(&self) -> Vec<&str> {
        self.servers
            .iter()
            .filter_map(WyrdTestServer::base_url)
            .collect()
    }

    /// Return each pod's local WAL directory.
    pub fn wal_dirs(&self) -> impl Iterator<Item = &std::path::Path> {
        self.wal_dirs.iter().map(tempfile::TempDir::path)
    }

    /// Explicitly stop all bound servers and release cluster resources.
    ///
    /// # Errors
    /// Returns the first server teardown error, after attempting all pods.
    pub async fn shutdown(self) -> Result<(), ClusterError> {
        let mut first_error = None;
        for server in self.servers {
            if let Err(error) = server.shutdown().await {
                first_error.get_or_insert(error.to_string());
            }
        }
        let _ = self.storage_root;
        let _ = self.fixture;
        let _ = self.wal_dirs;
        first_error.map_or(Ok(()), |error| Err(ClusterError::Shutdown(error)))
    }
}
