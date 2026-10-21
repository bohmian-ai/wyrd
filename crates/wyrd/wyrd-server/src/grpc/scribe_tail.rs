//! Private tonic adapter for Scribe live capture and release.
//!
//! Mounted only on the mutually authenticated peer listener, so every caller
//! already presented the cluster `wyrd-peer` certificate. That admits a
//! trusted cluster process, not a tenant. Listing returns partition metadata,
//! never rows; user and table authorization happen at Oracle before any
//! listing is issued, and each live fragment is checked against this Scribe's
//! own state by the Oracle peer service.

use std::sync::Arc;

use vala_bifrost_redux::scan_sizing::ScanScope;
use vala_bifrost_redux::scribe::tail_rpc::{FetchLiveTailService, TailReadError};
use wyrd_tonic::private_conversion::PrivateConversionError;
use wyrd_tonic::tonic::{Request, Response, Status};
use wyrd_tonic::wyrd::v1::scribe_tail_service_server::{
    ScribeTailService, ScribeTailServiceServer,
};
use wyrd_tonic::wyrd::v1::{
    self as proto, ListActiveStreamsRequest, ListActiveStreamsResponse, ReleaseTailCaptureRequest,
    ReleaseTailCaptureResponse,
};

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
    /// Captures one tenant table's live sources for the request's query and
    /// lists the captured event-day scopes with the bytes the request's scope
    /// reads in each; returns metadata and the capture id, never rows.
    async fn list_active_streams(
        &self,
        request: Request<ListActiveStreamsRequest>,
    ) -> Result<Response<ListActiveStreamsResponse>, Status> {
        let request = request.into_inner();
        let binding = wire_binding(request.binding)?;
        let query_id = uuid_field(&request.query_id, "query_id")?;
        let expires_at =
            std::time::UNIX_EPOCH + std::time::Duration::from_millis(request.expires_at_unix_ms);
        let scope = ScanScope::from_wire(request.columns, &request.filters);
        let capture = self
            .source
            .capture_active_streams(&binding, &scope, query_id, expires_at)
            .await
            .map_err(tail_status)?;
        Ok(Response::new(ListActiveStreamsResponse {
            capture_id: capture
                .capture_id
                .map(|capture_id| capture_id.as_bytes().to_vec())
                .unwrap_or_default(),
            streams: capture
                .streams
                .into_iter()
                .map(|stream| proto::ActiveTailStream {
                    time_partition: Some(stream.time_partition.into()),
                    stream: Some(stream.stream.into()),
                    bytes: stream.bytes,
                })
                .collect(),
            publication_revision: capture.publication_revision,
        }))
    }

    /// Releases one query's retained capture; an unknown capture is a no-op.
    async fn release_capture(
        &self,
        request: Request<ReleaseTailCaptureRequest>,
    ) -> Result<Response<ReleaseTailCaptureResponse>, Status> {
        let request = request.into_inner();
        let binding = wire_binding(request.binding)?;
        self.source
            .release_capture(
                &binding,
                uuid_field(&request.query_id, "query_id")?,
                uuid_field(&request.capture_id, "capture_id")?,
            )
            .map_err(tail_status)?;
        Ok(Response::new(ReleaseTailCaptureResponse {}))
    }
}

/// Decodes the required private tenant/table binding.
///
/// # Errors
/// Returns an invalid-argument status when it is absent or malformed.
fn wire_binding(
    binding: Option<proto::TenantTableBinding>,
) -> Result<wyrd_spec::vala::api::TenantTableBinding, Status> {
    let binding = binding.ok_or_else(|| Status::invalid_argument("tail binding is required"))?;
    wyrd_spec::vala::api::TenantTableBinding::try_from(binding).map_err(conversion_status)
}

/// Decodes one required 16-byte UUID field.
///
/// # Errors
/// Returns an invalid-argument status naming `field` when it is not a UUID.
fn uuid_field(value: &[u8], field: &str) -> Result<uuid::Uuid, Status> {
    uuid::Uuid::from_slice(value)
        .map_err(|_| Status::invalid_argument(format!("tail {field} is not a UUID")))
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
