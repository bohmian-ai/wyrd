//! Server-owned Bifrost Gate application service.

use std::pin::Pin;
use std::sync::Arc;

use async_stream::try_stream;
use futures_util::Stream;
use vala_bifrost::WyrdCatalog;
use vala_bifrost_redux::scribe::ScribeImpl;
use vala_ingest::{
    IngestAuthInterceptor, IngestError, IngestLimits, StreamSemaphores,
    auth::WYRD_REQUEST_ID_METADATA, orchestrator::run_ingest_frame_to_scribe,
};
use wyrd_auth_oidc::IssuerConfigResolver;
use wyrd_auth_verify::PermissionResolver;
use wyrd_tonic::tonic::{Request, Response, Status, Streaming};
use wyrd_tonic::wyrd::v1::bifrost_ingest_service_server::{
    BifrostIngestService, BifrostIngestServiceServer,
};
use wyrd_tonic::wyrd::v1::{InsertBatchRequest, InsertBatchResponse};

/// The concrete server boundary for native Bifrost writes.
///
/// Gate owns authentication, stream bounds, tenant concurrency, and transport
/// response ordering. The Scribe dependency is mandatory at construction.
#[derive(Clone)]
pub struct Gate<R: PermissionResolver + 'static, I: IssuerConfigResolver + 'static> {
    catalog: Arc<WyrdCatalog>,
    scribe: Arc<ScribeImpl>,
    limits: IngestLimits,
    semaphores: Arc<StreamSemaphores>,
    auth: IngestAuthInterceptor<R, I>,
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
        let semaphores = Arc::new(StreamSemaphores::new(
            limits.max_concurrent_streams_per_tenant,
        ));
        Self {
            catalog,
            scribe,
            limits,
            semaphores,
            auth,
        }
    }

    /// Mount the Gate on the shared tonic router.
    #[must_use]
    pub fn into_server(self) -> BifrostIngestServiceServer<Self> {
        let size = self.limits.max_decoding_message_size;
        BifrostIngestServiceServer::new(self).max_decoding_message_size(size)
    }
}

#[wyrd_tonic::tonic::async_trait]
impl<R: PermissionResolver + 'static, I: IssuerConfigResolver + 'static> BifrostIngestService
    for Gate<R, I>
{
    type InsertBatchStream =
        Pin<Box<dyn Stream<Item = Result<InsertBatchResponse, Status>> + Send>>;

    async fn insert_batch(
        &self,
        request: Request<Streaming<InsertBatchRequest>>,
    ) -> Result<Response<Self::InsertBatchStream>, Status> {
        let auth = self
            .auth
            .authenticate(request.metadata())
            .await
            .map_err(Status::from)?;
        let _permit = self.semaphores.acquire(auth.tenant)?;
        let mut stream = request.into_inner();
        let catalog = Arc::clone(&self.catalog);
        let scribe = Arc::clone(&self.scribe);
        let limits = self.limits.clone();
        let auth_context = auth.clone();
        let output = try_stream! {
            let started = std::time::Instant::now();
            let mut expected_sequence = 0_u64;
            let mut frame_count = 0_u64;
            let mut total_bytes = 0_u64;
            let mut total_rows = 0_u64;
            while let Some(frame) = tokio::time::timeout(limits.idle_deadline, stream.message())
                .await
                .map_err(|_| IngestError::StreamIdle)?
                .map_err(|status| IngestError::StreamProtocolViolation(status.to_string()))? {
                if started.elapsed() > limits.total_deadline {
                    Err(IngestError::StreamIdle)?;
                }
                if frame.frame_sequence != expected_sequence {
                    Err(IngestError::StreamProtocolViolation(format!(
                        "frame_sequence must be {expected_sequence}, got {}",
                        frame.frame_sequence
                    )))?;
                }
                expected_sequence = expected_sequence.saturating_add(1);
                frame_count = frame_count.saturating_add(1);
                if frame_count > limits.max_stream_frames {
                    Err(IngestError::StreamProtocolViolation(format!(
                        "stream exceeded {} frames",
                        limits.max_stream_frames
                    )))?;
                }
                if frame.table.is_empty() || frame.wyrd_batch_id.len() != 16 {
                    Err(IngestError::StreamProtocolViolation(
                        "table and exactly 16-byte wyrd_batch_id are required on every frame".to_owned()
                    ))?;
                }
                let batch_id = uuid::Uuid::from_bytes(frame.wyrd_batch_id.as_slice().try_into().map_err(|_| IngestError::StreamProtocolViolation("invalid batch id".to_owned()))?);
                if batch_id.get_version() != Some(uuid::Version::SortRand) {
                    Err(IngestError::StreamProtocolViolation(
                        "wyrd_batch_id must be UUIDv7".to_owned()
                    ))?;
                }
                total_bytes = total_bytes.saturating_add(frame.arrow_ipc.len() as u64);
                if total_bytes > limits.max_stream_bytes {
                    Err(IngestError::BatchTooLarge { bytes: total_bytes, limit: limits.max_stream_bytes })?;
                }
                let rows = run_ingest_frame_to_scribe(
                    &catalog,
                    scribe.as_ref(),
                    &limits,
                    &auth_context,
                    frame.clone(),
                ).await?;
                total_rows = total_rows.saturating_add(rows);
                if total_rows > limits.max_stream_rows {
                    Err(IngestError::TooManyRows { rows: total_rows, limit: limits.max_stream_rows })?;
                }
                yield InsertBatchResponse {
                    wyrd_batch_id: frame.wyrd_batch_id,
                    frame_sequence: frame.frame_sequence,
                    rows_accepted: rows,
                };
            }
        };
        let mut response = Response::new(Box::pin(output) as Self::InsertBatchStream);
        if let Ok(value) = auth.request_id.as_str().parse() {
            response
                .metadata_mut()
                .insert(WYRD_REQUEST_ID_METADATA, value);
        }
        Ok(response)
    }
}
