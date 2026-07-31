//! Private Oracle peer authority owned by the server boot boundary.

use std::sync::Arc;
use vala_bifrost_redux::oracle::dispatcher::OraclePeerWorker;

mod peer_audit;
mod peer_authority;
mod peer_service;
mod query_audit;

pub use peer_audit::PostgresPeerSecurityAudit;
pub use peer_authority::OraclePeerAuthority;
pub use peer_service::OraclePeerGrpc;
pub use query_audit::ServerOracleAudit;

/// Readiness-qualified private peer runtime retained by [`crate::AppState`].
pub struct OraclePeerRuntime {
    /// Fenced worker whose authority uses the retained verified audit writer.
    worker: Arc<OraclePeerWorker>,
    /// Verified writer retained for the full peer-service lifetime.
    _security_audit: Arc<PostgresPeerSecurityAudit>,
}

impl OraclePeerRuntime {
    /// Binds a worker to the exact audit writer used during its construction.
    #[must_use]
    pub fn new(
        worker: Arc<OraclePeerWorker>,
        security_audit: Arc<PostgresPeerSecurityAudit>,
    ) -> Self {
        Self {
            worker,
            _security_audit: security_audit,
        }
    }

    /// Returns the fenced worker mounted by the generated tonic service.
    #[must_use]
    pub fn worker(&self) -> Arc<OraclePeerWorker> {
        Arc::clone(&self.worker)
    }
}
