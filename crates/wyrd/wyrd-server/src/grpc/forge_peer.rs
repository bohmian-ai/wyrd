//! Private tonic adapter for commit notices addressed to the Forge leader.
//!
//! Mounted on the mutually authenticated peer listener of a pod that runs the
//! Forge coordinator, so every caller already presented the cluster
//! `wyrd-peer` certificate. A notice names the fencing token its sender read
//! from the live election row; a coordinator that does not hold that exact
//! term refuses it, so a replaced leader never counts a commit.

use std::sync::Arc;

use vala_bifrost_redux::forge::{
    Forge, ForgeCommitNotice, ForgeError, ForgeTableKey, ForgeTableSettings,
};
use vala_sql::row_types::forge_tasks::ForgeTaskTableIdentity;
use wyrd_spec::DataTenantId;
use wyrd_tonic::tonic::{Request, Response, Status};
use wyrd_tonic::wyrd::v1::forge_leader_peer_service_server::{
    ForgeLeaderPeerService, ForgeLeaderPeerServiceServer,
};
use wyrd_tonic::wyrd::v1::{NotifyForgePromotionRequest, NotifyForgePromotionResponse};

/// Private gRPC adapter delivering peer commit notices to this coordinator.
pub struct ForgeLeaderPeerGrpc {
    /// This pod's Forge coordinator.
    forge: Arc<Forge>,
}

impl ForgeLeaderPeerGrpc {
    /// Creates the adapter around this pod's coordinator.
    #[must_use]
    pub fn new(forge: Arc<Forge>) -> Self {
        Self { forge }
    }

    /// Returns the generated service wrapper for peer-router mounting.
    #[must_use]
    pub fn into_server(self) -> ForgeLeaderPeerServiceServer<Self> {
        ForgeLeaderPeerServiceServer::new(self)
    }

    /// Validates one request into the notice the in-process path would build.
    ///
    /// # Errors
    ///
    /// Returns `InvalidArgument` for a malformed tenant, table identity or
    /// table setting.
    fn notice(request: NotifyForgePromotionRequest) -> Result<ForgeCommitNotice, Status> {
        let tenant = request
            .tenant_id
            .parse::<DataTenantId>()
            .map_err(|_| Status::invalid_argument("tenant_id is not a tenant id"))?;
        let table = ForgeTaskTableIdentity::new(
            vala_bifrost_redux::catalog::BIFROST_CATALOG_NAME,
            request.namespace,
            request.table,
        )
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let settings = ForgeTableSettings::from_properties(&request.properties)
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        Ok(ForgeCommitNotice {
            key: ForgeTableKey { tenant, table },
            snapshot_id: request.snapshot_id,
            settings,
        })
    }
}

#[wyrd_tonic::tonic::async_trait]
impl ForgeLeaderPeerService for ForgeLeaderPeerGrpc {
    /// Applies one promotion commit notice to the held leader term.
    ///
    /// A coordinator that does not hold the named term answers
    /// `FailedPrecondition`; the sender drops the notice.
    async fn notify_promotion(
        &self,
        request: Request<NotifyForgePromotionRequest>,
    ) -> Result<Response<NotifyForgePromotionResponse>, Status> {
        let request = request.into_inner();
        let fencing_token = request.fencing_token;
        let notice = Self::notice(request)?;
        match self.forge.accept_commit_notice(fencing_token, notice) {
            Ok(()) => Ok(Response::new(NotifyForgePromotionResponse {})),
            Err(ForgeError::FenceLost { .. }) => Err(Status::failed_precondition(
                "this coordinator does not hold the named Forge leader term",
            )),
            Err(error) => Err(Status::internal(error.to_string())),
        }
    }
}
