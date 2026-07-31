//! Authenticated generated-tonic adapter for private Oracle peer execution.

use std::pin::Pin;
use std::sync::Arc;

use futures_util::{Stream, StreamExt};
use vala_bifrost_redux::oracle::dispatcher::{DispatchError, OraclePeerWorker, WorkerExecution};
use wyrd_runtime::PrincipalKind;
use wyrd_tonic::private_conversion::PrivateConversionError;
use wyrd_tonic::tonic::{Request, Response, Status};
use wyrd_tonic::wyrd::v1::oracle_peer_service_server::{
    OraclePeerService, OraclePeerServiceServer,
};
use wyrd_tonic::wyrd::v1::{
    self as proto, ExecuteFragmentRequest, ReleaseNodeSlotsRequest, ReserveNodeSlotsRequest,
};

use crate::AppState;

/// Private tonic service retaining one fenced worker runtime.
pub struct OraclePeerGrpc {
    /// Server auth state used to authenticate workload peers.
    state: AppState,
    /// Redux worker owner shared with the local transport.
    worker: Arc<OraclePeerWorker>,
}

impl OraclePeerGrpc {
    /// Creates the adapter around the retained worker runtime.
    #[must_use]
    pub fn new(state: AppState, worker: Arc<OraclePeerWorker>) -> Self {
        Self { state, worker }
    }

    /// Returns the generated tonic server wrapper.
    #[must_use]
    pub fn into_server(self) -> OraclePeerServiceServer<Self> {
        OraclePeerServiceServer::new(self)
    }

    /// Requires a verified service workload before peer state is touched.
    ///
    /// # Errors
    /// Returns an authentication or authorization status for missing/invalid workload identity.
    async fn authenticate(
        &self,
        metadata: &wyrd_tonic::tonic::metadata::MetadataMap,
    ) -> Result<(), Status> {
        let verifier = self
            .state
            .auth
            .token_verifier
            .as_ref()
            .ok_or_else(|| Status::unavailable("auth backend not configured"))?;
        let auth = vala_bifrost_redux::gate::auth::authenticate(verifier.as_ref(), metadata)
            .await
            .map_err(|error| Status::unauthenticated(error.to_string()))?;
        if !matches!(auth.principal.kind, PrincipalKind::Service { .. }) {
            return Err(Status::permission_denied(
                "private Oracle peer requires a workload service identity",
            ));
        }
        Ok(())
    }
}

#[wyrd_tonic::tonic::async_trait]
impl OraclePeerService for OraclePeerGrpc {
    /// Worker attempt stream retaining the running slot until EOF or cancellation.
    type ExecuteFragmentStream =
        Pin<Box<dyn Stream<Item = Result<proto::WorkerAttemptFrame, Status>> + Send>>;

    /// Reserves one bounded pending slot after workload authentication.
    ///
    /// # Errors
    /// Returns an authentication, conversion, or worker rejection status.
    async fn reserve_slots(
        &self,
        request: Request<ReserveNodeSlotsRequest>,
    ) -> Result<Response<proto::ReserveNodeSlotsResponse>, Status> {
        self.authenticate(request.metadata()).await?;
        let request = wyrd_spec::vala::api::ReserveNodeSlotsRequest::try_from(request.into_inner())
            .map_err(conversion_status)?;
        Ok(Response::new(self.worker.reserve(&request).into()))
    }

    /// Releases one matching reservation idempotently after workload authentication.
    ///
    /// # Errors
    /// Returns an authentication or conversion status.
    async fn release_slots(
        &self,
        request: Request<ReleaseNodeSlotsRequest>,
    ) -> Result<Response<proto::ReleaseNodeSlotsResponse>, Status> {
        self.authenticate(request.metadata()).await?;
        let request = wyrd_spec::vala::api::ReleaseNodeSlotsRequest::try_from(request.into_inner())
            .map_err(conversion_status)?;
        self.worker.release(&request);
        Ok(Response::new(proto::ReleaseNodeSlotsResponse {}))
    }

    /// Executes verified immutable work and streams a footer-terminated attempt.
    ///
    /// # Errors
    /// Returns an authentication, conversion, security, or execution status.
    async fn execute_fragment(
        &self,
        request: Request<ExecuteFragmentRequest>,
    ) -> Result<Response<Self::ExecuteFragmentStream>, Status> {
        self.authenticate(request.metadata()).await?;
        let request = wyrd_spec::vala::api::ExecuteFragmentRequest::try_from(request.into_inner())
            .map_err(conversion_status)?;
        let WorkerExecution { mut stream } = self
            .worker
            .execute(request)
            .await
            .map_err(dispatch_status)?;
        let output = async_stream::stream! {
            while let Some(frame) = stream.next().await {
                yield frame.map(Into::into).map_err(dispatch_status);
            }
        };
        Ok(Response::new(Box::pin(output)))
    }
}

/// Maps malformed private fields without revealing runtime state.
fn conversion_status(error: PrivateConversionError) -> Status {
    Status::invalid_argument(error.to_string())
}

/// Maps retryable peer failures separately from terminal security/contract failures.
fn dispatch_status(error: DispatchError) -> Status {
    match error {
        DispatchError::Retryable => Status::unavailable(error.to_string()),
        DispatchError::StaleObject => Status::not_found(error.to_string()),
        DispatchError::Terminal => Status::permission_denied(error.to_string()),
        DispatchError::Exhausted => Status::aborted(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Proves the private tonic boundary preserves only stale-object failures as not-found.
    #[test]
    fn stale_object_dispatch_status_is_not_found() {
        let stale = dispatch_status(DispatchError::StaleObject);
        let outage = dispatch_status(DispatchError::Retryable);

        assert_eq!(stale.code(), wyrd_tonic::tonic::Code::NotFound);
        assert_eq!(outage.code(), wyrd_tonic::tonic::Code::Unavailable);
    }
}
