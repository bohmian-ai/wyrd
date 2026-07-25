//! Reusable real Scribe harness over the shared Bifrost test cluster.

use std::sync::Arc;

use thiserror::Error;
use vala_bifrost_redux::scribe::ScribeImpl;
use vala_bifrost_redux::scribe::memtable::MemtableStats;
use vala_bifrost_redux::scribe::seal::PostCommitBatch;
use vala_bifrost_redux::scribe::telemetry::ScribeInspectionSnapshot;
use vala_bifrost_redux::scribe::wal::{WalConfig, WalWriter};
use vala_sql::TenantConn;
use wyrd_spec::DataTenantId;

use super::{BifrostTopology, ClusterError, WyrdTestCluster};

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
        if tenant_count == 0 {
            return Err(HarnessError::Configuration(
                "at least one tenant is required".to_owned(),
            ));
        }
        let topology = match pods {
            1 => BifrostTopology::OnePod,
            3 => BifrostTopology::ThreePod,
            other => {
                return Err(HarnessError::Configuration(format!(
                    "supported pod counts are 1 and 3, got {other}"
                )));
            }
        };
        let cluster = WyrdTestCluster::start(pods, topology).await?;
        match Self::start_with_cluster(&cluster, tenant_count).await {
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

    async fn start_with_cluster(
        cluster: &WyrdTestCluster,
        tenant_count: usize,
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
            let wal = Arc::new(
                WalWriter::new(
                    wal_dir,
                    *node_id.as_bytes(),
                    i64::try_from(index + 1)
                        .map_err(|error| HarnessError::Configuration(error.to_string()))?,
                    WalConfig::default(),
                )
                .map_err(|error| HarnessError::Scribe(error.to_string()))?,
            );
            let scribe = ScribeImpl::new_for_embedded_with_deps(
                Arc::clone(&operator),
                wal,
                node_id.to_string(),
                i64::try_from(index + 1)
                    .map_err(|error| HarnessError::Configuration(error.to_string()))?,
            );
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
            let mut commits: Vec<(&ScribeImpl, PostCommitBatch)> =
                Vec::with_capacity(self.scribes.len());
            for scribe in &self.scribes {
                match scribe.force_seal(&mut conn).await {
                    Ok(batch) => commits.push((scribe.as_ref(), batch)),
                    Err(error) => {
                        for (completed_scribe, batch) in commits {
                            let _ = completed_scribe.abort_post_commit(batch);
                        }
                        return Err(HarnessError::Scribe(error.to_string()));
                    }
                }
            }
            conn.commit()
                .await
                .map_err(|error| HarnessError::Database(error.to_string()))?;
            for (scribe, batch) in commits {
                scribe
                    .complete_post_commit(batch)
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
        BifrostTopology::ThreePod => 3,
    }
}
