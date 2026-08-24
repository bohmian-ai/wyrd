//! Private Oracle peer authority owned by the server boot boundary.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::cluster::{ClusterError, ClusterRegistry, RegisteredRole};
use vala_bifrost_redux::oracle::dispatcher::OraclePeerWorker;
use wyrd_spec::vala::api::{NodeId, OracleAdmissionContinuityLost};

mod audit_wal;
mod forwarding;
mod lifecycle_controls;
/// Private owner-local lifecycle transport over the canonical query runtime.
pub(crate) mod lifecycle_service;
mod lifecycle_transport;
mod peer_audit;
mod peer_authority;
mod peer_credentials;
mod peer_service;
mod query_audit;
mod tail_audit;
mod tail_authority;
mod tail_discovery;

pub use forwarding::{ReadyOracleForwarder, ReadyOracleForwarderInputs};
pub use lifecycle_controls::RunningQueryControls;
pub use lifecycle_service::OracleLifecycleGrpc;
pub use lifecycle_transport::{
    OracleLifecycleNodeOutcome, OracleLifecycleOutcome, OracleLifecycleTransport,
};
pub use peer_audit::PostgresPeerSecurityAudit;
pub use peer_authority::OraclePeerAuthority;
pub use peer_credentials::ServerOraclePeerCredentials;
pub use peer_service::OraclePeerGrpc;
#[cfg(feature = "test-support")]
pub use query_audit::{
    AuditRelayPauseGuard, AuditTelemetryLabelDomains, audit_telemetry_label_domains,
};
pub use query_audit::{AuditShutdownReport, OracleAuditPublisher};
pub use tail_audit::PostgresTailSecurityAudit;
pub use tail_authority::ScribeTailAuthority;
pub use tail_discovery::RegistryTailStreamDiscovery;

/// Closes the local serving latch for one delegated-continuity notification.
///
/// The latch closes and the shared Oracle cancellation token fires even when a
/// malformed signal does not match the expected fence. The boolean reports the
/// exact match so the lifecycle owner can diagnose corruption while remaining
/// fail-closed.
pub(crate) fn close_local_delegated_continuity(
    expected_node: NodeId,
    expected_fence: u64,
    loss: OracleAdmissionContinuityLost,
    advertise_ready: &AtomicBool,
    shutdown: &CancellationToken,
) -> bool {
    advertise_ready.store(false, Ordering::Release);
    shutdown.cancel();
    loss.holder_node_id == expected_node && loss.holder_fencing_token == expected_fence
}

/// Removes one retained Oracle fence from durable and local routing discovery.
///
/// # Errors
///
/// Returns [`ClusterError`] when the exact fenced readiness mutation or the
/// following immutable snapshot refresh cannot complete.
pub(crate) async fn deactivate_delegated_continuity(
    cluster: &ClusterRegistry,
    registered_role: &RegisteredRole,
) -> Result<(), ClusterError> {
    cluster.deactivate(registered_role).await?;
    cluster.refresh_snapshot().await?;
    Ok(())
}

/// Oracle-owned private peer service retained by the Oracle runtime.
#[derive(Clone)]
pub struct OraclePeerRuntime {
    /// Fenced worker whose authority uses the retained verified audit writer.
    worker: Arc<OraclePeerWorker>,
    /// Verified writer retained for the full peer-service lifetime.
    security_audit: Arc<PostgresPeerSecurityAudit>,
    /// Canonical authenticated client retained for lifecycle control fanout.
    lifecycle_transport: Arc<OracleLifecycleTransport>,
}

impl OraclePeerRuntime {
    /// Binds a worker to the exact audit writer used during its construction.
    #[must_use]
    pub fn new(
        worker: Arc<OraclePeerWorker>,
        security_audit: Arc<PostgresPeerSecurityAudit>,
        lifecycle_transport: Arc<OracleLifecycleTransport>,
    ) -> Self {
        Self {
            worker,
            security_audit,
            lifecycle_transport,
        }
    }
}

impl OraclePeerRuntime {
    /// Returns the fenced worker mounted by the generated tonic service.
    #[must_use]
    pub fn worker(&self) -> Arc<OraclePeerWorker> {
        Arc::clone(&self.worker)
    }

    /// Returns the scrubbed security writer retained by the peer service.
    #[must_use]
    pub fn security_audit(&self) -> Arc<PostgresPeerSecurityAudit> {
        Arc::clone(&self.security_audit)
    }

    /// Returns the one authenticated lifecycle transport built at boot.
    #[must_use]
    pub fn lifecycle_transport(&self) -> Arc<OracleLifecycleTransport> {
        Arc::clone(&self.lifecycle_transport)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::vala::api::{OracleCapabilitiesV1, QueryClass, ScribeCapabilitiesV1};

    /// Delegated continuity loss closes readiness and settles cancellation work.
    ///
    /// # Panics
    ///
    /// Panics when the exact fence is not recognized, local readiness remains
    /// open, or an affected task does not observe cancellation and settle.
    #[tokio::test]
    async fn delegated_continuity_loss_deactivates_exact_oracle_fence_and_settles_queries() {
        let fixture = PgFixture::start().await.expect("PostgreSQL fixture starts");
        let node_id = NodeId::new(uuid::Uuid::from_u128(41));
        let cluster = ClusterRegistry::new(fixture.vala_postgres().clone(), node_id);
        let oracle = cluster
            .register_oracle(
                "http://oracle.test",
                OracleCapabilitiesV1 {
                    peer_protocol_version: 2,
                    storage_protocol_version: 1,
                    cpu_cores: 1.0,
                    memory_budget_bytes: 1024,
                    cpu_cores_per_slot: 1.0,
                    memory_bytes_per_slot: 1024,
                    raw_slots: 1,
                    usable_slots: 1,
                    supported_classes: vec![QueryClass::Interactive],
                    max_workers_per_query: 1,
                },
            )
            .await
            .expect("Oracle registers");
        let scribe = cluster
            .register_scribe(
                "http://scribe.test",
                ScribeCapabilitiesV1 {
                    tail_protocol_version: 1,
                },
            )
            .await
            .expect("Scribe registers");
        cluster.refresh_snapshot().await.expect("ready snapshot");
        let shutdown = CancellationToken::new();
        let affected_query = shutdown.child_token();
        let settled = Arc::new(AtomicBool::new(false));
        let task_settled = Arc::clone(&settled);
        let task = tokio::spawn(async move {
            affected_query.cancelled().await;
            task_settled.store(true, Ordering::Release);
        });
        let ready = AtomicBool::new(true);
        let loss = OracleAdmissionContinuityLost {
            holder_node_id: node_id,
            holder_fencing_token: oracle.fencing_token,
        };
        assert!(close_local_delegated_continuity(
            oracle.key.node_id,
            oracle.fencing_token,
            loss,
            &ready,
            &shutdown,
        ));
        deactivate_delegated_continuity(&cluster, &oracle)
            .await
            .expect("continuity event deactivates exact role");
        task.await.expect("affected query settles");
        assert!(!ready.load(Ordering::Acquire));
        assert!(settled.load(Ordering::Acquire));
        let snapshot = cluster.snapshot();
        assert!(
            snapshot
                .live_oracle_at_fence(node_id, oracle.fencing_token)
                .is_none()
        );
        assert!(
            snapshot
                .live_scribe_at_fence(node_id, scribe.fencing_token)
                .is_some()
        );
        let durable_ready: bool = sqlx::query_scalar(
            "SELECT ready FROM vala.cluster_nodes WHERE data_tenant_id=$1 \
             AND node_id=$2 AND role='oracle' AND fencing_token=$3",
        )
        .bind(wyrd_spec::DataTenantId::SYSTEM_OWNER.as_uuid())
        .bind(node_id.as_uuid())
        .bind(i64::try_from(oracle.fencing_token).expect("test fence fits i64"))
        .fetch_one(fixture.operator_pool().pool())
        .await
        .expect("durable Oracle readiness");
        assert!(!durable_ready);

        let mismatch_shutdown = CancellationToken::new();
        let mismatch_ready = AtomicBool::new(true);
        assert!(!close_local_delegated_continuity(
            node_id,
            oracle.fencing_token,
            OracleAdmissionContinuityLost {
                holder_node_id: node_id,
                holder_fencing_token: oracle.fencing_token + 1,
            },
            &mismatch_ready,
            &mismatch_shutdown,
        ));
        assert!(!mismatch_ready.load(Ordering::Acquire));
        assert!(mismatch_shutdown.is_cancelled());
    }
}
