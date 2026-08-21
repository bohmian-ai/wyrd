//! Private Oracle peer authority owned by the server boot boundary.

use std::sync::Arc;
use vala_bifrost_redux::oracle::dispatcher::OraclePeerWorker;

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
