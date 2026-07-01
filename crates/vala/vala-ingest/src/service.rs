//! The `wyrd.v1.BifrostIngestService` gRPC server implementation.
//!
//! This crate owns no listener and binds no socket — S3.C2 mounts the service on
//! the shared tonic listener, wrapping it in the auth interceptor. Here we only
//! implement the generated trait: read the resolved [`AuthContext`] from the
//! request extensions (the interceptor populated them), take a per-tenant stream
//! slot, and drive the orchestrator.

use std::sync::Arc;

use vala_bifrost::WyrdCatalog;
use wyrd_tonic::tonic::{Request, Response, Status, Streaming};
use wyrd_tonic::wyrd::v1::bifrost_ingest_service_server::{
    BifrostIngestService, BifrostIngestServiceServer,
};
use wyrd_tonic::wyrd::v1::{InsertBatchRequest, InsertBatchResponse};

use crate::auth::{AuthContext, WYRD_REQUEST_ID_METADATA};
use crate::error::IngestError;
use crate::limits::{IngestLimits, StreamSemaphores};
use crate::orchestrator::run_ingest;

/// gRPC ingest service over `vala-bifrost`'s writer.
pub struct BifrostIngestGrpc {
    catalog: Arc<WyrdCatalog>,
    limits: IngestLimits,
    semaphores: Arc<StreamSemaphores>,
}

impl BifrostIngestGrpc {
    /// Construct from the engine catalog with default [`IngestLimits`].
    #[must_use]
    pub fn new(catalog: Arc<WyrdCatalog>) -> Self {
        Self::with_limits(catalog, IngestLimits::default())
    }

    /// Construct with explicit limits.
    #[must_use]
    pub fn with_limits(catalog: Arc<WyrdCatalog>, limits: IngestLimits) -> Self {
        let semaphores = Arc::new(StreamSemaphores::new(
            limits.max_concurrent_streams_per_tenant,
        ));
        Self {
            catalog,
            limits,
            semaphores,
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
impl BifrostIngestService for BifrostIngestGrpc {
    async fn insert_batch(
        &self,
        request: Request<Streaming<InsertBatchRequest>>,
    ) -> Result<Response<InsertBatchResponse>, Status> {
        let auth = request
            .extensions()
            .get::<AuthContext>()
            .cloned()
            .ok_or_else(|| {
                IngestError::Unauthenticated("request carries no auth context".to_owned())
            })?;

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
