//! Immutable, role-fenced membership snapshots for Scribe and Oracle runtime roles.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use chrono::Utc;
use thiserror::Error;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use vala_sql::queries::cluster_nodes::ClusterNodes;
use vala_sql::row_types::cluster_nodes::{RoleMutation, RoleRegistration};
use vala_sql::{OperatorPool, SqlError};
use wyrd_spec::vala::api::{
    ClusterCapabilities, ClusterNodeKey, ClusterRole, ClusterRoleLease, NodeId,
    OracleCapabilitiesV1, ScribeCapabilitiesV1,
};

/// Production heartbeat period for independently fenced runtime roles.
pub const ROLE_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
/// Maximum age of a heartbeat included in a live cluster snapshot.
pub const ROLE_LIVENESS_CUTOFF: Duration = Duration::from_secs(15);

/// Cluster membership operation failure.
#[derive(Debug, Error)]
pub enum ClusterError {
    /// The role registry could not complete its operator-owned SQL operation.
    #[error("cluster membership operation failed: {0}")]
    Sql(#[from] SqlError),
    /// A role instance lost its independent fence before a lifecycle update.
    #[error("cluster role fence is stale")]
    StaleFence,
}

/// One role registration retained by its lifecycle owner.
#[derive(Debug, Clone)]
pub struct RegisteredRole {
    /// Composite node and role identity.
    pub key: ClusterNodeKey,
    /// Independent role fence returned by registration.
    pub fencing_token: u64,
    /// Capability document repeated on every fenced heartbeat.
    pub capabilities: ClusterCapabilities,
}

/// Immutable projection of ready, live cluster role leases.
#[derive(Debug, Clone, Default)]
pub struct ClusterSnapshot {
    /// Ready, fresh leases keyed by their composite node and closed role values.
    roles: HashMap<SnapshotKey, ClusterRoleLease>,
}

/// Hashable in-memory representation of the durable `(NodeId, ClusterRole)` key.
///
/// The T1 wire key intentionally does not commit to a collection trait. Snapshot
/// publication needs immutable constant-time lookup, so this private equivalent
/// preserves the same closed role values without widening the wire contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct SnapshotKey {
    /// Physical pod identity.
    node_id: NodeId,
    /// Closed independently fenced role encoded for the immutable map.
    role: SnapshotRole,
}

/// Closed role values used only as the local snapshot-map key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum SnapshotRole {
    /// WAL and live-tail owner.
    Scribe,
    /// Query leader and sealed-scan worker.
    Oracle,
}

impl From<ClusterNodeKey> for SnapshotKey {
    /// Converts the durable composite identity without changing its role meaning.
    fn from(value: ClusterNodeKey) -> Self {
        Self {
            node_id: value.node_id,
            role: match value.role {
                ClusterRole::Scribe => SnapshotRole::Scribe,
                ClusterRole::Oracle => SnapshotRole::Oracle,
            },
        }
    }
}

impl ClusterSnapshot {
    /// Builds an immutable snapshot from already validated live leases.
    #[must_use]
    pub fn new(roles: Vec<ClusterRoleLease>) -> Self {
        Self {
            roles: roles
                .into_iter()
                .map(|lease| (SnapshotKey::from(lease.key), lease))
                .collect(),
        }
    }

    /// Returns the live Scribe projection without relying on a positional role order.
    #[must_use]
    pub fn live_scribes(&self) -> Vec<&ClusterRoleLease> {
        self.roles
            .values()
            .filter(|lease| lease.key.role == ClusterRole::Scribe)
            .collect()
    }

    /// Returns the live Oracle projection without relying on a positional role order.
    #[must_use]
    pub fn live_oracles(&self) -> Vec<&ClusterRoleLease> {
        self.roles
            .values()
            .filter(|lease| lease.key.role == ClusterRole::Oracle)
            .collect()
    }
}

/// Concrete Redux owner for role registration, fenced lifecycle updates, and snapshots.
pub struct ClusterRegistry {
    /// The one role-fenced SQL owner used by every lifecycle operation.
    nodes: ClusterNodes,
    /// Physical pod identity shared by the independently fenced roles.
    node_id: NodeId,
    /// Atomically published immutable live-role snapshot.
    snapshot: ArcSwap<ClusterSnapshot>,
}

impl ClusterRegistry {
    /// Creates a registry for the server-owned operator pool and pod node identity.
    #[must_use]
    pub fn new(pool: OperatorPool, node_id: NodeId) -> Self {
        Self {
            nodes: ClusterNodes::new(pool),
            node_id,
            snapshot: ArcSwap::from_pointee(ClusterSnapshot::default()),
        }
    }

    /// Registers one Scribe role and advances only the Scribe fence for this node.
    ///
    /// # Errors
    ///
    /// Returns [`ClusterError`] when registration or the initial fenced ready
    /// heartbeat cannot complete.
    pub async fn register_scribe(
        &self,
        address: &str,
        capabilities: ScribeCapabilitiesV1,
    ) -> Result<RegisteredRole, ClusterError> {
        let capabilities = ClusterCapabilities::ScribeV1(capabilities);
        self.register_role(ClusterRole::Scribe, address, capabilities)
            .await
    }

    /// Registers one Oracle role and advances only the Oracle fence for this node.
    ///
    /// # Errors
    ///
    /// Returns [`ClusterError`] when registration or the initial fenced ready
    /// heartbeat cannot complete.
    pub async fn register_oracle(
        &self,
        address: &str,
        capabilities: OracleCapabilitiesV1,
    ) -> Result<RegisteredRole, ClusterError> {
        let capabilities = ClusterCapabilities::OracleV1(capabilities);
        self.register_role(ClusterRole::Oracle, address, capabilities)
            .await
    }

    /// Returns the most recently atomically published live-role snapshot.
    #[must_use]
    pub fn snapshot(&self) -> Arc<ClusterSnapshot> {
        self.snapshot.load_full()
    }

    /// Refreshes both role projections from durable membership and publishes one snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`ClusterError`] if either closed role projection cannot be decoded
    /// or read from membership.
    pub async fn refresh_snapshot(&self) -> Result<(), ClusterError> {
        let cutoff = Utc::now()
            - chrono::Duration::from_std(ROLE_LIVENESS_CUTOFF).map_err(|error| {
                ClusterError::Sql(SqlError::InvariantViolation {
                    detail: error.to_string(),
                })
            })?;
        let scribes = self.nodes.list_live(ClusterRole::Scribe, cutoff).await?;
        let oracles = self.nodes.list_live(ClusterRole::Oracle, cutoff).await?;
        self.snapshot.store(Arc::new(ClusterSnapshot::new(
            scribes.into_iter().chain(oracles).collect(),
        )));
        metrics::gauge!("bifrost_cluster_roles_live", "role" => "scribe")
            .set(self.snapshot().live_scribes().len() as f64);
        metrics::gauge!("bifrost_cluster_roles_live", "role" => "oracle")
            .set(self.snapshot().live_oracles().len() as f64);
        Ok(())
    }

    /// Starts the bounded cadence heartbeat for one independently fenced role.
    ///
    /// The task exits with the provided server lifecycle token. A stale fence is
    /// terminal for this registration because a replacement now owns the role.
    #[must_use]
    pub fn start_heartbeat(
        self: Arc<Self>,
        registered: RegisteredRole,
        shutdown: CancellationToken,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(ROLE_HEARTBEAT_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            interval.tick().await;
            loop {
                tokio::select! {
                    _ = shutdown.cancelled() => return,
                    _ = interval.tick() => match self.heartbeat(&registered).await {
                        Ok(()) => {},
                        Err(ClusterError::StaleFence) => {
                            tracing::warn!(role = ?registered.key.role, "cluster role heartbeat lost its fence");
                            return;
                        }
                        Err(error) => tracing::warn!(role = ?registered.key.role, %error, "cluster role heartbeat failed"),
                    },
                }
            }
        })
    }

    /// Starts the membership poller that publishes immutable role snapshots.
    ///
    /// The task exits with the provided server lifecycle token and leaves the
    /// last valid snapshot installed when one transient refresh fails.
    #[must_use]
    pub fn start_snapshot_poller(self: Arc<Self>, shutdown: CancellationToken) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(ROLE_HEARTBEAT_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    _ = shutdown.cancelled() => return,
                    _ = interval.tick() => {
                        if let Err(error) = self.refresh_snapshot().await {
                            tracing::warn!(%error, "cluster membership snapshot refresh failed");
                        }
                    }
                }
            }
        })
    }

    /// Applies one fenced ready heartbeat for a role this registry registered.
    ///
    /// # Errors
    ///
    /// Returns [`ClusterError::StaleFence`] if a replacement role already owns
    /// the composite key, or [`ClusterError::Sql`] for membership failures.
    pub async fn heartbeat(&self, registered: &RegisteredRole) -> Result<(), ClusterError> {
        match self
            .nodes
            .heartbeat(
                &registered.key,
                registered.fencing_token,
                true,
                &registered.capabilities,
            )
            .await?
        {
            RoleMutation::Applied => Ok(()),
            RoleMutation::StaleFence => Err(ClusterError::StaleFence),
        }
    }

    /// Unregisters one exact role fence without touching another role on this node.
    ///
    /// # Errors
    ///
    /// Returns [`ClusterError::StaleFence`] if the role was replaced, or
    /// [`ClusterError::Sql`] if durable membership cannot be updated.
    pub async fn shutdown_role(&self, registered: RegisteredRole) -> Result<(), ClusterError> {
        match self
            .nodes
            .unregister(&registered.key, registered.fencing_token)
            .await?
        {
            RoleMutation::Applied => Ok(()),
            RoleMutation::StaleFence => Err(ClusterError::StaleFence),
        }
    }

    /// Registers one closed role and marks it ready through its new independent fence.
    async fn register_role(
        &self,
        role: ClusterRole,
        address: &str,
        capabilities: ClusterCapabilities,
    ) -> Result<RegisteredRole, ClusterError> {
        let key = ClusterNodeKey {
            node_id: self.node_id,
            role,
        };
        let registration = RoleRegistration {
            key,
            address: address.to_owned(),
            capabilities: capabilities.clone(),
            started_at: Utc::now(),
        };
        let registered = self.nodes.register(&registration).await?;
        let role = RegisteredRole {
            key,
            fencing_token: registered.lease.fencing_token,
            capabilities,
        };
        self.heartbeat(&role).await?;
        Ok(role)
    }
}

#[cfg(test)]
mod tests {
    use super::ClusterSnapshot;
    use chrono::Utc;
    use wyrd_spec::vala::api::{
        ClusterCapabilities, ClusterNodeKey, ClusterRole, ClusterRoleLease, ScribeCapabilitiesV1,
    };

    /// Projects each role by its composite key when both roles share a physical node.
    #[test]
    fn snapshot_projects_roles_by_composite_key() {
        let node_id = wyrd_spec::vala::api::NodeId::new(uuid::Uuid::now_v7());
        let lease = |role| ClusterRoleLease {
            key: ClusterNodeKey { node_id, role },
            address: "127.0.0.1:9000".to_owned(),
            fencing_token: 1,
            capability_version: 1,
            capabilities: ClusterCapabilities::ScribeV1(ScribeCapabilitiesV1 {
                tail_protocol_version: 1,
            }),
            ready: true,
            started_at: Utc::now(),
            heartbeat_at: Utc::now(),
        };
        let snapshot =
            ClusterSnapshot::new(vec![lease(ClusterRole::Oracle), lease(ClusterRole::Scribe)]);
        assert_eq!(snapshot.live_scribes().len(), 1);
        assert_eq!(snapshot.live_oracles().len(), 1);
    }
}
