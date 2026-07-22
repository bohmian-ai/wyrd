//! Server-owned Bifrost Gate mount.

use std::sync::Arc;

use vala_bifrost::WyrdCatalog;
use vala_bifrost_redux::scribe::ScribeImpl;
use vala_ingest::{BifrostIngestGrpc, IngestAuthInterceptor, IngestLimits};
use wyrd_auth_oidc::IssuerConfigResolver;
use wyrd_auth_verify::PermissionResolver;
use wyrd_tonic::wyrd::v1::bifrost_ingest_service_server::BifrostIngestServiceServer;

/// The concrete server application boundary for native Bifrost writes.
///
/// The wrapped transport adapter is private to this module; callers mount the
/// returned server through Gate, so production routing has one write boundary.
pub struct Gate<R: PermissionResolver + 'static, I: IssuerConfigResolver + 'static> {
    service: BifrostIngestGrpc<R, I>,
}

impl<R: PermissionResolver + 'static, I: IssuerConfigResolver + 'static> Gate<R, I> {
    /// Construct a Gate with a required server-owned Scribe.
    #[must_use]
    pub fn with_scribe(
        catalog: Arc<WyrdCatalog>,
        scribe: Arc<ScribeImpl>,
        auth: IngestAuthInterceptor<R, I>,
        limits: IngestLimits,
    ) -> Self {
        Self {
            service: BifrostIngestGrpc::with_scribe(catalog, scribe, auth, limits),
        }
    }

    /// Mount the Gate on the shared tonic router.
    #[must_use]
    pub fn into_server(self) -> BifrostIngestServiceServer<BifrostIngestGrpc<R, I>> {
        self.service.into_server()
    }
}
