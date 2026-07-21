//! Shared-resource, independently bound Bifrost test cluster.

use std::sync::Arc;

use opendal::Operator;
use thiserror::Error;
use vala_bifrost::catalog::WyrdCatalog;
use wyrd_auth::seed::seed_builtin_roles_for_tenant;
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_spec::DataTenantId;
use wyrd_storage::{BackendConfig, StorageHandle, StorageSettings};

use crate::server::{WyrdTestServer, WyrdTestServerBuilder, WyrdTestServerError, test_catalog};

/// Supported role topology for a Bifrost cluster journey.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BifrostTopology {
    /// Run one full Gate, Scribe, Forge, and Oracle pod.
    OnePod,
    /// Run three full Gate, Scribe, Forge, and Oracle pods.
    ThreePod,
}

impl BifrostTopology {
    fn validate(self, pods: usize) -> Result<(), ClusterError> {
        let expected = match self {
            Self::OnePod => 1,
            Self::ThreePod => 3,
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
}

impl WyrdTestCluster {
    /// Start a one- or three-pod cluster over shared real dependencies.
    ///
    /// # Errors
    /// Returns an error when Postgres, object storage, Iceberg, a WAL
    /// directory, or an independently bound server cannot be created.
    pub async fn start(pods: usize, topology: BifrostTopology) -> Result<Self, ClusterError> {
        topology.validate(pods)?;
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

        let mut wal_dirs = Vec::with_capacity(pods);
        let mut servers = Vec::with_capacity(pods);
        for _ in 0..pods {
            let wal_dir =
                tempfile::tempdir().map_err(|error| ClusterError::Resource(error.to_string()))?;
            let server = WyrdTestServerBuilder::default()
                .start_with_resources(
                    Arc::clone(&fixture),
                    Arc::clone(&storage),
                    Arc::clone(&catalog),
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
