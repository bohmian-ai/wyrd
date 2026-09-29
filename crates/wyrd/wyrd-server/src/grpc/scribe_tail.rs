//! Private tonic adapter for Scribe active-stream discovery.
//!
//! Mounted only on the mutually authenticated peer listener, so every caller
//! already presented the cluster `wyrd-peer` certificate. That admits a
//! trusted cluster process, not a tenant. Listing returns partition metadata,
//! never rows; user and table authorization happen at Oracle before any
//! listing is issued, and each live fragment is checked against this Scribe's
//! own state by the Oracle peer service.

use std::sync::Arc;

use vala_bifrost_redux::scribe::tail_rpc::{FetchLiveTailService, TailReadError};
use wyrd_tonic::private_conversion::PrivateConversionError;
use wyrd_tonic::tonic::{Request, Response, Status};
use wyrd_tonic::wyrd::v1::scribe_tail_service_server::{
    ScribeTailService, ScribeTailServiceServer,
};
use wyrd_tonic::wyrd::v1::{self as proto, ListActiveStreamsRequest, ListActiveStreamsResponse};

/// Private gRPC adapter listing this process's live Scribe streams.
pub struct ScribeTailGrpc {
    /// Scribe-owned live source whose active partitions are listed.
    source: Arc<FetchLiveTailService>,
}

impl ScribeTailGrpc {
    /// Creates the adapter around the server's Scribe live source.
    #[must_use]
    pub fn new(source: Arc<FetchLiveTailService>) -> Self {
        Self { source }
    }

    /// Returns the generated service wrapper for application-router mounting.
    #[must_use]
    pub fn into_server(self) -> ScribeTailServiceServer<Self> {
        ScribeTailServiceServer::new(self)
    }
}

#[wyrd_tonic::tonic::async_trait]
impl ScribeTailService for ScribeTailGrpc {
    /// Lists active tenant/table event-day scopes; returns metadata, never rows.
    async fn list_active_streams(
        &self,
        request: Request<ListActiveStreamsRequest>,
    ) -> Result<Response<ListActiveStreamsResponse>, Status> {
        let binding = request
            .into_inner()
            .binding
            .ok_or_else(|| Status::invalid_argument("tail binding is required"))?;
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
