//! Private tonic adapter for commit notices addressed to the Forge leader.
//!
//! Mounted on the mutually authenticated peer listener of a pod that runs the
//! Forge coordinator, so every caller already presented the cluster
//! `wyrd-peer` certificate. A notice names the fencing token its sender read
//! from the live election row; a coordinator that does not hold that exact
//! term refuses it, so a replaced leader never counts a commit. Compactor
//! pulls and reports are fenced the same way.

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
use wyrd_tonic::wyrd::v1::{
    NotifyForgePromotionRequest, NotifyForgePromotionResponse, PullForgeCompactionRequest,
    PullForgeCompactionResponse, ReportForgeCompactionRequest, ReportForgeCompactionResponse,
};

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

    /// Maps a leader-side failure to the status a remote caller sees.
    ///
    /// A replaced or never-elected term is `FailedPrecondition`, which every
    /// caller treats as "no current leader here".
    fn refusal(error: ForgeError) -> Status {
        match error {
            ForgeError::FenceLost { .. } => Status::failed_precondition(
                "this coordinator does not hold the named Forge leader term",
            ),
            error => Status::internal(error.to_string()),
        }
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
    ///
    /// # Errors
    ///
    /// Returns `InvalidArgument` for a malformed tenant, table identity or
    /// table setting, `FailedPrecondition` when this coordinator does not hold
    /// the named leader term, and `Internal` for any other leader-side failure.
    async fn notify_promotion(
        &self,
        request: Request<NotifyForgePromotionRequest>,
    ) -> Result<Response<NotifyForgePromotionResponse>, Status> {
        let request = request.into_inner();
        let fencing_token = request.fencing_token;
        let notice = Self::notice(request)?;
        self.forge
            .accept_commit_notice(fencing_token, notice)
            .map(|()| Response::new(NotifyForgePromotionResponse {}))
            .map_err(Self::refusal)
    }

    /// Answers one remote compactor's capacity pull from the held term.
    ///
    /// The response is the pull's acknowledgement; a coordinator that does
    /// not hold the named term answers `FailedPrecondition`.
    ///
    /// # Errors
    ///
    /// Returns `FailedPrecondition` when this coordinator does not hold the
    /// named leader term and `Internal` for any other leader-side failure.
    async fn pull_compaction(
        &self,
        request: Request<PullForgeCompactionRequest>,
    ) -> Result<Response<PullForgeCompactionResponse>, Status> {
        let request = request.into_inner();
        let limit = usize::try_from(request.limit).unwrap_or(usize::MAX);
        match self
            .forge
            .serve_compaction_pull(request.fencing_token, limit)
        {
            Ok(dispatches) => Ok(Response::new(PullForgeCompactionResponse {
                tasks: dispatches
                    .iter()
                    .map(vala_bifrost_redux::forge::dispatch_to_wire)
                    .collect(),
            })),
            Err(error) => Err(Self::refusal(error)),
        }
    }

    /// Applies one remote compactor's report to the held term.
    ///
    /// A stale task identity is answered with `matched = false`.
    ///
    /// # Errors
    ///
    /// Returns `InvalidArgument` for a malformed table key, task id or outcome,
    /// `FailedPrecondition` when this coordinator does not hold the named
    /// leader term, and `Internal` for any other leader-side failure.
    async fn report_compaction(
        &self,
        request: Request<ReportForgeCompactionRequest>,
    ) -> Result<Response<ReportForgeCompactionResponse>, Status> {
        let request = request.into_inner();
        let key = vala_bifrost_redux::forge::table_key_from_wire(
            &request.tenant_id,
            request.namespace,
            request.table,
        )
        .map_err(Status::invalid_argument)?;
        let task_id = request
            .task_id
            .parse()
            .map_err(|_| Status::invalid_argument("task_id is not a UUID"))?;
        let outcome = vala_bifrost_redux::forge::outcome_from_wire(request.outcome)
            .map_err(Status::invalid_argument)?;
        match self
            .forge
            .serve_compaction_report(request.fencing_token, &key, task_id, outcome)
        {
            Ok(matched) => Ok(Response::new(ReportForgeCompactionResponse { matched })),
            Err(error) => Err(Self::refusal(error)),
        }
    }
}
