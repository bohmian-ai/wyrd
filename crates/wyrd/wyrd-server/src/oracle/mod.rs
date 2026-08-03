//! Private Oracle peer authority owned by the server boot boundary.

use std::sync::Arc;
use vala_bifrost_redux::cluster::ClusterRegistry;
use vala_bifrost_redux::oracle::dispatcher::OraclePeerWorker;

mod peer_audit;
mod peer_authority;
mod peer_credentials;
mod peer_service;
mod query_audit;
mod tail_audit;
mod tail_authority;
mod tail_discovery;

pub use peer_audit::PostgresPeerSecurityAudit;
pub use peer_authority::OraclePeerAuthority;
pub use peer_credentials::ServerOraclePeerCredentials;
pub use peer_service::OraclePeerGrpc;
pub use query_audit::ServerOracleAudit;
pub use tail_audit::PostgresTailSecurityAudit;
pub use tail_authority::ScribeTailAuthority;
pub use tail_discovery::RegistryTailStreamDiscovery;

/// Readiness-qualified private peer runtime retained by [`crate::AppState`].
pub struct OraclePeerRuntime {
    /// Fenced worker whose authority uses the retained verified audit writer.
    worker: Arc<OraclePeerWorker>,
    /// Verified writer retained for the full peer-service lifetime.
    security_audit: Arc<PostgresPeerSecurityAudit>,
    cluster: Arc<ClusterRegistry>,
}

impl OraclePeerRuntime {
    /// Binds a worker to the exact audit writer used during its construction.
    #[must_use]
    pub fn new(
        worker: Arc<OraclePeerWorker>,
        security_audit: Arc<PostgresPeerSecurityAudit>,
        cluster: Arc<ClusterRegistry>,
    ) -> Self {
        Self {
            worker,
            security_audit,
            cluster,
        }
    }

    /// Returns the fenced worker mounted by the generated tonic service.
    #[must_use]
    pub fn worker(&self) -> Arc<OraclePeerWorker> {
        Arc::clone(&self.worker)
    }

    /// Returns the durable membership owner used for peer authorization.
    #[must_use]
    pub fn cluster(&self) -> Arc<ClusterRegistry> {
        Arc::clone(&self.cluster)
    }

    /// Returns the scrubbed security writer retained by the peer service.
    #[must_use]
    pub fn security_audit(&self) -> Arc<PostgresPeerSecurityAudit> {
        Arc::clone(&self.security_audit)
    }
}
