//! Authenticated private tonic adapter for bounded Scribe tail fences.

use std::sync::Arc;

use arrow::ipc::writer::StreamWriter;
use vala_bifrost_redux::scribe::tail_rpc::{ScribeTailReader, TailReadError};
use wyrd_runtime::PrincipalKind;
use wyrd_tonic::private_conversion::PrivateConversionError;
use wyrd_tonic::tonic::{Request, Response, Status};
use wyrd_tonic::wyrd::v1::scribe_tail_service_server::{
    ScribeTailService, ScribeTailServiceServer,
};
use wyrd_tonic::wyrd::v1::{
    self as proto, AcquireTailFenceRequest, ReleaseTailFenceRequest, TailPageRequest,
};

use crate::AppState;

/// Private gRPC adapter that validates a workload caller before touching a tail fence.
pub struct ScribeTailGrpc {
    /// Server auth state used to derive the tenant from verified metadata.
    state: AppState,
    /// Scribe-owned fence state shared with local Oracle reads.
    reader: Arc<ScribeTailReader>,
}

impl ScribeTailGrpc {
    /// Creates the adapter around the server's retained tail reader.
    #[must_use]
    pub fn new(state: AppState, reader: Arc<ScribeTailReader>) -> Self {
        Self { state, reader }
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
            .await
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
    /// Acquires only fence metadata after authenticating and tenant-validating the binding.
    async fn acquire_fence(
        &self,
        request: Request<AcquireTailFenceRequest>,
    ) -> Result<Response<proto::TailReadFence>, Status> {
        let tenant = self.authenticated_tenant(request.metadata()).await?;
        let request = wyrd_spec::vala::api::AcquireTailFenceRequest::try_from(request.into_inner())
            .map_err(conversion_status)?;
        if request.binding.tenant_id != tenant {
            return Err(Status::permission_denied(
                "tail binding tenant does not match caller",
            ));
        }
        let fence = self
            .reader
            .acquire_fence(request)
            .await
            .map_err(tail_status)?;
        Ok(Response::new(fence.into()))
    }

    /// Reads one bounded owned-frame page only after authenticating the caller.
    async fn read_fence_page(
        &self,
        request: Request<TailPageRequest>,
    ) -> Result<Response<proto::TailPage>, Status> {
        let tenant = self.authenticated_tenant(request.metadata()).await?;
        let request = wyrd_spec::vala::api::TailPageRequest::try_from(request.into_inner())
            .map_err(conversion_status)?;
        let page = self
            .reader
            .read_page_for_tenant(tenant, request)
            .map_err(tail_status)?;
        let batches = page
            .batches
            .iter()
            .map(|batch| encode_batch(batch.as_ref()))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Response::new(proto::TailPage {
            arrow_ipc_batches: batches,
            next_cursor: page
                .next
                .map(Into::into)
                .map(proto::tail_page::NextCursor::Next),
            complete: page.complete,
        }))
    }

    /// Releases a retained fence idempotently after authenticating the caller.
    async fn release_fence(
        &self,
        request: Request<ReleaseTailFenceRequest>,
    ) -> Result<Response<proto::ReleaseTailFenceResponse>, Status> {
        let tenant = self.authenticated_tenant(request.metadata()).await?;
        let request = wyrd_spec::vala::api::ReleaseTailFenceRequest::try_from(request.into_inner())
            .map_err(conversion_status)?;
        self.reader
            .release_fence_for_tenant(tenant, request.fence_id)
            .map_err(tail_status)?;
        Ok(Response::new(proto::ReleaseTailFenceResponse {}))
    }
}

/// Maps malformed private wire fields to an unauthenticated-safe invalid argument status.
fn conversion_status(error: PrivateConversionError) -> Status {
    Status::invalid_argument(error.to_string())
}

/// Maps local tail failures without revealing fence existence to unauthenticated callers.
fn tail_status(error: TailReadError) -> Status {
    match error {
        TailReadError::DeadlineElapsed => Status::deadline_exceeded(error.to_string()),
        TailReadError::Capacity => Status::resource_exhausted(error.to_string()),
        TailReadError::UnsupportedProtocol { .. }
        | TailReadError::Binding
        | TailReadError::SchemaMismatch => Status::invalid_argument(error.to_string()),
        TailReadError::AccessDenied => Status::permission_denied(error.to_string()),
        TailReadError::WriterEpochMismatch | TailReadError::CursorOutOfRange => {
            Status::failed_precondition(error.to_string())
        }
        TailReadError::OversizeRow => Status::out_of_range(error.to_string()),
        TailReadError::Encode { .. } | TailReadError::State { .. } => {
            Status::internal(error.to_string())
        }
    }
}

/// Encodes one local shallow batch into the private transport's owned Arrow IPC bytes.
fn encode_batch(batch: &arrow::record_batch::RecordBatch) -> Result<Vec<u8>, Status> {
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, &batch.schema())
        .map_err(|error| Status::internal(error.to_string()))?;
    writer
        .write(batch)
        .map_err(|error| Status::internal(error.to_string()))?;
    writer
        .finish()
        .map_err(|error| Status::internal(error.to_string()))?;
    Ok(bytes)
}
