//! Reusable real Scribe harness over the shared Bifrost test cluster.

use std::sync::Arc;

use thiserror::Error;
use vala_bifrost_redux::forge::Forge;
use vala_bifrost_redux::maintenance::StagingFilePublisher;
use vala_bifrost_redux::scribe::admission::AdmissionConfig;
use vala_bifrost_redux::scribe::execution_lanes::{
    ScribeIngressCpuPool, ScribePersistenceCpuPool, ScribeWalIoPool,
};
use vala_bifrost_redux::scribe::memtable::MemtableStats;
use vala_bifrost_redux::scribe::persistence::PersistenceFaults;
use vala_bifrost_redux::scribe::stream_identity::{NodeId, StreamIdentity, WriterEpoch};
use vala_bifrost_redux::scribe::telemetry::ScribeInspectionSnapshot;
use vala_bifrost_redux::scribe::wal::{WalConfig, WalWriter};
use vala_bifrost_redux::scribe::{
    ScribeBuildConfig, ScribeExecutionPools, ScribeImpl, ScribePersistenceConfig,
};
use vala_sql::TenantConn;
use wyrd_spec::DataTenantId;

use super::{BifrostTopology, ClusterError, WyrdTestCluster};
use crate::WyrdTestServer;

/// Errors raised while constructing or draining the real harness.
#[derive(Debug, Error)]
pub enum HarnessError {
    /// The shared cluster could not be created.
    #[error("cluster setup failed: {0}")]
    Cluster(#[from] ClusterError),
    /// The harness configuration is invalid.
    #[error("invalid harness configuration: {0}")]
    Configuration(String),
    /// A Scribe operation failed.
    #[error("scribe operation failed: {0}")]
    Scribe(String),
    /// A tenant connection could not be acquired.
    #[error("tenant connection failed: {0}")]
    Database(String),
}

/// Reusable real Scribe setup for journeys and bounded workload runs.
pub struct BifrostHarness {
    cluster: WyrdTestCluster,
    scribes: Vec<Arc<ScribeImpl>>,
    tenants: Vec<DataTenantId>,
}

impl BifrostHarness {
    /// Start a one- or three-pod real Scribe harness with seeded tenants.
    pub async fn start(pods: usize, tenant_count: usize) -> Result<Self, HarnessError> {
        Self::start_with_wal_sync_delay(pods, tenant_count, std::time::Duration::ZERO).await
    }

    /// Start a Scribe harness with an explicit test-only WAL sync delay.
    pub async fn start_with_wal_sync_delay(
        pods: usize,
        tenant_count: usize,
        wal_sync_delay: std::time::Duration,
    ) -> Result<Self, HarnessError> {
        Self::start_with_faults(
            pods,
            tenant_count,
            wal_sync_delay,
            PersistenceFaults::default(),
        )
        .await
    }

    /// Start a Scribe harness with concrete persistence fault controls.
    pub async fn start_with_faults(
        pods: usize,
        tenant_count: usize,
        wal_sync_delay: std::time::Duration,
        persistence_faults: PersistenceFaults,
    ) -> Result<Self, HarnessError> {
        if tenant_count == 0 {
            return Err(HarnessError::Configuration(
                "at least one tenant is required".to_owned(),
            ));
        }
        let topology = match pods {
            1 => BifrostTopology::OnePod,
            3 => BifrostTopology::ThreePod,
            6 => BifrostTopology::SixPod,
            other => {
                return Err(HarnessError::Configuration(format!(
                    "supported pod counts are 1, 3, and 6, got {other}"
                )));
            }
        };
        let cluster =
            WyrdTestCluster::start_with_wal_sync_delay(pods, topology, wal_sync_delay).await?;
        let actual_scribes = wal_sync_delay.is_zero();
        let setup = if actual_scribes {
            let tenants = {
                let mut tenants = vec![cluster.data_tenant_id()];
                for index in 1..tenant_count {
                    tenants.push(cluster.add_tenant(&format!("bench-tenant-{index}")).await?);
                }
                tenants
            };
            let scribes = cluster
                .servers()
                .filter_map(WyrdTestServer::bifrost_scribe)
                .collect();
            Ok((scribes, tenants))
        } else {
            Self::start_with_cluster(&cluster, tenant_count, wal_sync_delay, persistence_faults)
                .await
        };
        match setup {
            Ok((scribes, tenants)) => Ok(Self {
                cluster,
                scribes,
                tenants,
            }),
            Err(error) => {
                let _ = cluster.shutdown().await;
                Err(error)
            }
        }
    }

    /// Constructs every Scribe pod and tenant fixture for the retained cluster.
    ///
    /// # Errors
    ///
    /// Returns [`HarnessError`] when tenant creation, writer-epoch conversion,
    /// WAL/Scribe construction, catalog registration, or recovery startup
    /// fails.
    async fn start_with_cluster(
        cluster: &WyrdTestCluster,
        tenant_count: usize,
        wal_sync_delay: std::time::Duration,
        persistence_faults: PersistenceFaults,
    ) -> Result<(Vec<Arc<ScribeImpl>>, Vec<DataTenantId>), HarnessError> {
        let mut tenants = Vec::with_capacity(tenant_count);
        tenants.push(cluster.data_tenant_id());
        for index in 1..tenant_count {
            tenants.push(cluster.add_tenant(&format!("bench-tenant-{index}")).await?);
        }

        let operator = Arc::new(cluster.storage_operator());
        let mut scribes = Vec::with_capacity(pods_for(cluster));
        for (index, wal_dir) in cluster.wal_dirs().enumerate() {
            let node_id = uuid::Uuid::now_v7();
            let writer_epoch = i64::try_from(index + 1)
                .map_err(|error| HarnessError::Configuration(error.to_string()))?;
            let wal = Arc::new(
                WalWriter::new(
                    wal_dir,
                    *node_id.as_bytes(),
                    writer_epoch,
                    WalConfig::default(),
                )
                .map_err(|error| HarnessError::Scribe(error.to_string()))?,
            );
            let scribe_resources = cluster
                .server(index)
                .and_then(|server| server.state().bifrost_resources.as_ref())
                .and_then(vala_bifrost_redux::resources::BifrostRoleResources::scribe)
                .ok_or_else(|| {
                    HarnessError::Configuration(
                        "Bifrost test server did not provision shared memory".to_owned(),
                    )
                })?;
            let server = cluster.server(index).ok_or_else(|| {
                HarnessError::Configuration("missing Bifrost test server".to_owned())
            })?;
            let scribe = ScribeImpl::new_with_execution_pools(ScribeBuildConfig {
                catalog: Some(Arc::clone(&server.state().bifrost)),
                operator: Arc::clone(&operator),
                wal,
                stream: StreamIdentity::new(NodeId::new(node_id), WriterEpoch::new(writer_epoch)),
                admission: AdmissionConfig::default(),
                coordination_runtime: tokio::runtime::Handle::current(),
                execution_pools: ScribeExecutionPools::new(
                    ScribeIngressCpuPool::new_with_capacity(2, 256),
                    ScribePersistenceCpuPool::new_with_capacity(2, 64),
                    ScribeWalIoPool::try_new_with_capacity_and_delay(2, 256, wal_sync_delay)
                        .map_err(|error| HarnessError::Configuration(error.to_string()))?,
                ),
                persistence: Some(
                    ScribePersistenceConfig::new(
                        Arc::new(server.state().postgres.vala().clone()),
                        64,
                        2,
                    )
                    .with_operator_pool(server.state().postgres.operator_pool().ok_or_else(
                        || {
                            HarnessError::Configuration(
                                "Bifrost test server has no operator pool".to_owned(),
                            )
                        },
                    )?)
                    .with_output_scratch(
                        server
                            .state()
                            .bifrost_resources
                            .as_ref()
                            .and_then(|resources| resources.scribe())
                            .and_then(|resources| resources.volume_capabilities())
                            .map(|(_, scratch)| scratch)
                            .ok_or_else(|| {
                                HarnessError::Configuration(
                                    "Bifrost test server has no Scribe output scratch".to_owned(),
                                )
                            })?,
                    )
                    .with_test_faults(persistence_faults.clone()),
                ),
                resources: scribe_resources,
                ingest_limits: vala_bifrost_redux::gate::limits::IngestLimits::default(),
                staging_file_publisher: None,
            });
            scribes.push(Arc::new(scribe));
        }

        Ok((scribes, tenants))
    }

    /// Return the shared real cluster and its servers.
    #[must_use]
    pub fn cluster(&self) -> &WyrdTestCluster {
        &self.cluster
    }

    /// Return all tenant IDs in deterministic setup order.
    #[must_use]
    pub fn tenants(&self) -> &[DataTenantId] {
        &self.tenants
    }

    /// Return all Scribe pods in pod order.
    #[must_use]
    pub fn scribes(&self) -> &[Arc<ScribeImpl>] {
        &self.scribes
    }

    /// Return the Forge handles retained by each real bound server pod.
    #[must_use]
    pub fn server_forges(&self) -> Vec<Arc<Forge>> {
        self.cluster
            .servers()
            .filter_map(|server| server.state().forge().cloned())
            .collect()
    }

    /// Return publishers paired with the real server-owned Forge inboxes.
    #[must_use]
    pub fn server_forge_publishers(&self) -> Vec<StagingFilePublisher> {
        self.cluster
            .servers()
            .map(WyrdTestServer::forge_publisher)
            .collect()
    }

    /// Acquire a tenant-scoped connection from the shared fixture.
    pub async fn tenant_conn(&self, tenant: DataTenantId) -> Result<TenantConn<'_>, HarnessError> {
        self.cluster
            .pg_fixture()
            .tenant_conn_for(tenant)
            .await
            .map_err(|error| HarnessError::Database(error.to_string()))
    }

    /// Seal every pod's writable and pending generations and commit file-list
    /// transactions through the tenant-scoped fixture connection.
    pub async fn force_seal_all(&self) -> Result<(), HarnessError> {
        for tenant in self.tenants.iter().copied() {
            let mut conn = self.tenant_conn(tenant).await?;
            let mut commits = Vec::with_capacity(self.scribes.len());
            for scribe in &self.scribes {
                match scribe.force_seal(&mut conn).await {
                    Ok(batch) => commits.push((scribe.as_ref(), batch)),
                    Err(error) => {
                        for (completed_scribe, batch) in commits {
                            let _ = completed_scribe.abort_commit_attempts(batch).await;
                        }
                        return Err(HarnessError::Scribe(error.to_string()));
                    }
                }
            }
            let commit_result = conn.commit().await;
            for (scribe, batch) in commits {
                scribe
                    .settle_commit_attempts(batch, &commit_result)
                    .await
                    .map_err(|error| HarnessError::Scribe(error.to_string()))?;
            }
        }
        Ok(())
    }

    /// Return whether no writable rows or pending post-commit generations remain.
    pub fn is_drained(&self) -> Result<bool, HarnessError> {
        self.scribes
            .iter()
            .map(|scribe| scribe.memtable_stats())
            .collect::<Result<Vec<_>, _>>()
            .map(|stats| {
                stats
                    .iter()
                    .all(|state| state.writable_rows == 0 && state.pending_generations == 0)
            })
            .map_err(|error| HarnessError::Scribe(error.to_string()))
    }

    /// Return the aggregate current WAL footprint.
    #[must_use]
    pub fn wal_bytes(&self) -> u64 {
        self.scribes
            .iter()
            .map(|scribe| scribe.wal_bytes_on_disk())
            .sum()
    }

    /// Return one bounded ownership snapshot per real Scribe pod.
    pub fn inspection_snapshots(&self) -> Result<Vec<ScribeInspectionSnapshot>, HarnessError> {
        self.scribes
            .iter()
            .map(|scribe| {
                scribe
                    .inspection_snapshot()
                    .map_err(|error| HarnessError::Scribe(error.to_string()))
            })
            .collect()
    }

    /// Return aggregate memtable state across pods.
    pub fn memtable_stats(&self) -> Result<MemtableStats, HarnessError> {
        self.scribes
            .iter()
            .try_fold(MemtableStats::default(), |mut total, scribe| {
                let stats = scribe
                    .memtable_stats()
                    .map_err(|error| HarnessError::Scribe(error.to_string()))?;
                total.writable_rows += stats.writable_rows;
                total.writable_bytes += stats.writable_bytes;
                total.immutable_rows += stats.immutable_rows;
                total.immutable_bytes += stats.immutable_bytes;
                total.immutable_generations += stats.immutable_generations;
                total.pending_generations += stats.pending_generations;
                Ok(total)
            })
    }

    /// Shut down all real servers and release fixture resources.
    pub async fn shutdown(self) -> Result<(), HarnessError> {
        self.cluster.shutdown().await?;
        Ok(())
    }
}

fn pods_for(cluster: &WyrdTestCluster) -> usize {
    match cluster.topology() {
        BifrostTopology::OnePod => 1,
        BifrostTopology::TwoPod => 2,
        BifrostTopology::ThreePod | BifrostTopology::RoleSeparated => 3,
        BifrostTopology::SixPod => 6,
        BifrostTopology::DedicatedForgeWorkers => 4,
        BifrostTopology::ThreeServersThreeForgeWorkers => 6,
    }
}
