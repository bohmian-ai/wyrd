//! Authenticated private tonic adapter for Scribe active-stream discovery.
//!
//! Listing is metadata-only and trusts the authenticated internal peer: a
//! workload service bearer over the mutually authenticated peer channel. The
//! shared system-owner workload may list any tenant's table; any other
//! workload may list only its own tenant. No ticket or replay record exists.

use std::sync::Arc;

use vala_bifrost_redux::scribe::tail_rpc::{FetchLiveTailService, TailReadError};
use wyrd_runtime::PrincipalKind;
use wyrd_tonic::private_conversion::PrivateConversionError;
use wyrd_tonic::tonic::{Request, Response, Status};
use wyrd_tonic::wyrd::v1::scribe_tail_service_server::{
    ScribeTailService, ScribeTailServiceServer,
};
use wyrd_tonic::wyrd::v1::{self as proto, ListActiveStreamsRequest, ListActiveStreamsResponse};

use crate::AppState;

/// Private gRPC adapter that validates a workload caller before listing live streams.
pub struct ScribeTailGrpc {
    /// Server auth state used to derive the tenant from verified metadata.
    state: AppState,
    /// Scribe-owned live source whose active partitions are listed.
    source: Arc<FetchLiveTailService>,
}

impl ScribeTailGrpc {
    /// Creates the adapter around the server's Scribe live source.
    #[must_use]
    pub fn new(state: AppState, source: Arc<FetchLiveTailService>) -> Self {
        Self { state, source }
    }

    /// Returns the generated service wrapper for application-router mounting.
    #[must_use]
    pub fn into_server(self) -> ScribeTailServiceServer<Self> {
        ScribeTailServiceServer::new(self)
    }

    /// Authenticates a service workload and returns its server-derived tenant.
    async fn authenticated_tenant(
        &self,
        metadata: &wyrd_tonic::tonic::metadata::MetadataMap,
    ) -> Result<wyrd_spec::DataTenantId, Status> {
        let verifier = self
            .state
            .auth
            .token_verifier
            .as_ref()
            .ok_or_else(|| Status::unavailable("auth backend not configured"))?;
        let auth = vala_bifrost_redux::gate::auth::authenticate(verifier.as_ref(), metadata)
            .map_err(|error| Status::unauthenticated(error.to_string()))?;
        if !matches!(auth.principal.kind, PrincipalKind::Service { .. }) {
            return Err(Status::permission_denied(
                "private Scribe tail requires a workload service identity",
            ));
        }
        Ok(auth.tenant)
    }
}

#[wyrd_tonic::tonic::async_trait]
impl ScribeTailService for ScribeTailGrpc {
    /// Lists active tenant/table event-day scopes; returns metadata, never rows.
    async fn list_active_streams(
        &self,
        request: Request<ListActiveStreamsRequest>,
    ) -> Result<Response<ListActiveStreamsResponse>, Status> {
        let tenant = self.authenticated_tenant(request.metadata()).await?;
        let binding = request
            .into_inner()
            .binding
            .ok_or_else(|| Status::invalid_argument("tail binding is required"))?;
        if tenant != wyrd_spec::DataTenantId::SYSTEM_OWNER
            && binding.tenant_id != tenant.as_uuid().to_string()
        {
            return Err(Status::permission_denied(
                "tail binding tenant does not match caller",
            ));
        }
        let binding = wyrd_spec::vala::api::TenantTableBinding::try_from(binding)
            .map_err(conversion_status)?;
        let streams = self
            .source
            .list_active_streams(&binding)
            .map_err(tail_status)?;
        Ok(Response::new(ListActiveStreamsResponse {
            streams: streams
                .into_iter()
                .map(|(time_partition, stream)| proto::ActiveTailStream {
                    time_partition: Some(time_partition.into()),
                    stream: Some(stream.into()),
                })
                .collect(),
        }))
    }
}

/// Maps malformed private wire fields to an unauthenticated-safe invalid argument status.
fn conversion_status(error: PrivateConversionError) -> Status {
    Status::invalid_argument(error.to_string())
}

/// Maps local discovery failures to tonic statuses at the private boundary.
fn tail_status(error: TailReadError) -> Status {
    match error {
        TailReadError::DeadlineElapsed => Status::deadline_exceeded(error.to_string()),
        TailReadError::Binding => Status::invalid_argument(error.to_string()),
        TailReadError::Authorization { .. } => Status::permission_denied(error.to_string()),
        TailReadError::State { .. } => Status::internal(error.to_string()),
        TailReadError::Unavailable { .. } => Status::unavailable(error.to_string()),
        TailReadError::StaleIdentity => Status::failed_precondition(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use wyrd_tonic::tonic::Code;

    use vala_bifrost_redux::scribe::tail_rpc::TailReadError;

    use super::tail_status;

    /// Proves Scribe state failures become opaque internal statuses only at the boundary.
    ///
    /// # Panics
    ///
    /// Panics when the boundary mapper exposes a non-internal tonic status.
    #[test]
    fn tail_state_error_maps_to_internal_status() {
        let status = tail_status(TailReadError::State {
            detail: "fixture failure".to_owned(),
        });

        assert_eq!(status.code(), Code::Internal);
    }
}
