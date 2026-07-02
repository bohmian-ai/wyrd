//! The `wyrd.v1.BifrostIngestService` gRPC server implementation.
//!
//! This crate owns no listener and binds no socket — S3.C2 mounts the service on
//! the shared tonic listener. Because tonic's sync `Interceptor` cannot run the
//! async token verification, auth completes **in the handler**: `insert_batch`
//! awaits `self.auth.authenticate(request.metadata())` as its first step, then
//! takes a per-tenant stream slot and drives the orchestrator.

use std::sync::Arc;

use vala_bifrost::WyrdCatalog;
use wyrd_auth_verify::PermissionResolver;
use wyrd_tonic::tonic::{Request, Response, Status, Streaming};
use wyrd_tonic::wyrd::v1::bifrost_ingest_service_server::{
    BifrostIngestService, BifrostIngestServiceServer,
};
use wyrd_tonic::wyrd::v1::{InsertBatchRequest, InsertBatchResponse};

use crate::auth::{IngestAuthInterceptor, WYRD_REQUEST_ID_METADATA};
use crate::limits::{IngestLimits, StreamSemaphores};
use crate::orchestrator::run_ingest;

/// gRPC ingest service over `vala-bifrost`'s writer.
///
/// Generic over the resolver-backed verifier `R` (static dispatch, no
/// `Box<dyn>`) so the same auth seam the HTTP extractor uses is threaded in at
/// mount time.
pub struct BifrostIngestGrpc<R: PermissionResolver + 'static> {
    catalog: Arc<WyrdCatalog>,
    limits: IngestLimits,
    semaphores: Arc<StreamSemaphores>,
    auth: IngestAuthInterceptor<R>,
}

impl<R: PermissionResolver + 'static> BifrostIngestGrpc<R> {
    /// Construct from the engine catalog with default [`IngestLimits`].
    #[must_use]
    pub fn new(catalog: Arc<WyrdCatalog>, auth: IngestAuthInterceptor<R>) -> Self {
        Self::with_limits(catalog, auth, IngestLimits::default())
    }

    /// Construct with explicit limits.
    #[must_use]
    pub fn with_limits(
        catalog: Arc<WyrdCatalog>,
        auth: IngestAuthInterceptor<R>,
        limits: IngestLimits,
    ) -> Self {
        let semaphores = Arc::new(StreamSemaphores::new(
            limits.max_concurrent_streams_per_tenant,
        ));
        Self {
            catalog,
            limits,
            semaphores,
            auth,
        }
    }

    /// Wrap into the generated server type, raising `max_decoding_message_size`
    /// above tonic's 4 MiB default (which silently drops large Arrow frames).
    #[must_use]
    pub fn into_server(self) -> BifrostIngestServiceServer<Self> {
        let size = self.limits.max_decoding_message_size;
        BifrostIngestServiceServer::new(self).max_decoding_message_size(size)
    }
}

#[wyrd_tonic::tonic::async_trait]
impl<R: PermissionResolver + 'static> BifrostIngestService for BifrostIngestGrpc<R> {
    async fn insert_batch(
        &self,
        request: Request<Streaming<InsertBatchRequest>>,
    ) -> Result<Response<InsertBatchResponse>, Status> {
        let auth = self
            .auth
            .authenticate(request.metadata())
            .await
            .map_err(Status::from)?;

        // Hold a per-tenant stream slot for the whole commit; dropped on return.
        let _permit = self.semaphores.acquire(auth.tenant)?;
        let request_id = auth.request_id.clone();

        let stream = request.into_inner();
        let rows = run_ingest(&self.catalog, &self.limits, &auth, stream).await?;

        let mut response = Response::new(InsertBatchResponse {
            rows_accepted: rows,
        });
        if let Ok(value) = request_id.as_str().parse() {
            response
                .metadata_mut()
                .insert(WYRD_REQUEST_ID_METADATA, value);
        }
        Ok(response)
    }
}
