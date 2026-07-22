//! Server-owned Bifrost Gate application service.

use std::pin::Pin;
use std::sync::Arc;

use async_stream::try_stream;
use futures_util::Stream;
use vala_bifrost::WyrdCatalog;
use vala_bifrost_redux::contracts::Scribe;
use vala_bifrost_redux::scribe::ScribeImpl;
use vala_ingest::{
    IngestAuthInterceptor, IngestError, IngestLimits, IngestOutcome, LogsOutcome, MetricsOutcome,
    StreamSemaphores, auth::WYRD_REQUEST_ID_METADATA, orchestrator::run_ingest_frame_to_scribe,
};
use wyrd_auth_oidc::IssuerConfigResolver;
use wyrd_auth_verify::PermissionResolver;
use wyrd_spec::error::WyrdError;
use wyrd_tonic::otlp::logs_service::logs_service_server::LogsService;
use wyrd_tonic::otlp::logs_service::{ExportLogsServiceRequest, ExportLogsServiceResponse};
use wyrd_tonic::otlp::metrics_service::metrics_service_server::MetricsService;
use wyrd_tonic::otlp::metrics_service::{
    ExportMetricsServiceRequest, ExportMetricsServiceResponse,
};
use wyrd_tonic::otlp::trace_service::trace_service_server::TraceService;
use wyrd_tonic::otlp::trace_service::{ExportTraceServiceRequest, ExportTraceServiceResponse};
use wyrd_tonic::tonic::{Request, Response, Status, Streaming};
use wyrd_tonic::wyrd::v1::bifrost_ingest_service_server::{
    BifrostIngestService, BifrostIngestServiceServer,
};
use wyrd_tonic::wyrd::v1::{InsertBatchRequest, InsertBatchResponse};

/// Narrow future read capability owned by the server Gate.
///
/// The read engine is intentionally not implemented by this task. Keeping the
/// slot here prevents read handlers from silently reaching into storage while
/// the Oracle task supplies the concrete implementation.
pub trait Oracle: Send + Sync {}

fn oracle_unavailable() -> WyrdError {
    WyrdError::ServiceUnavailable {
        message: "Bifrost Oracle is not available".to_owned(),
        details: serde_json::Value::Null,
    }
}

/// The concrete server boundary for native Bifrost writes.
///
/// Gate owns authentication, stream bounds, tenant concurrency, and transport
/// response ordering. The Scribe dependency is mandatory at construction.
#[derive(Clone)]
pub struct Gate<R: PermissionResolver + 'static, I: IssuerConfigResolver + 'static> {
    catalog: Arc<WyrdCatalog>,
    scribe: Arc<dyn Scribe>,
    oracle: Option<Arc<dyn Oracle>>,
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
            oracle: None,
            limits,
            semaphores,
            auth,
        }
    }

    /// Construct a Gate with an explicit future Oracle capability.
    #[must_use]
    pub fn with_scribe_and_oracle(
        catalog: Arc<WyrdCatalog>,
        scribe: Arc<ScribeImpl>,
        oracle: Arc<dyn Oracle>,
        auth: IngestAuthInterceptor<R, I>,
        limits: IngestLimits,
    ) -> Self {
        let mut gate = Self::with_scribe(catalog, scribe, auth, limits);
        gate.oracle = Some(oracle);
        gate
    }

    /// Dispatch the read slot without bypassing the future Oracle.
    ///
    /// Until an Oracle implementation is supplied, all read dispatches fail
    /// with the stable service-unavailable contract.
    pub fn dispatch_read(&self) -> Result<(), WyrdError> {
        let _oracle = self.oracle.as_ref().ok_or_else(oracle_unavailable)?;
        Err(WyrdError::ServiceUnavailable {
            message: "Bifrost Oracle read dispatch is not implemented".to_owned(),
            details: serde_json::Value::Null,
        })
    }

    /// Mount the Gate on the shared tonic router.
    #[must_use]
    pub fn into_server(self) -> BifrostIngestServiceServer<Self> {
        let size = self.limits.max_decoding_message_size;
        BifrostIngestServiceServer::new(self).max_decoding_message_size(size)
    }

    /// Project one OTLP trace export through the Gate-owned Scribe boundary.
    pub async fn ingest_resource_spans(
        &self,
        auth: &vala_ingest::AuthContext,
        request: ExportTraceServiceRequest,
    ) -> Result<IngestOutcome, IngestError> {
        vala_ingest::ingest_resource_spans_to_scribe(
            &self.catalog,
            self.scribe.as_ref(),
            auth,
            request,
        )
        .await
    }

    /// Project one OTLP metrics export through the Gate-owned Scribe boundary.
    pub async fn ingest_resource_metrics(
        &self,
        auth: &vala_ingest::AuthContext,
        request: ExportMetricsServiceRequest,
    ) -> Result<MetricsOutcome, IngestError> {
        vala_ingest::ingest_resource_metrics_to_scribe(
            &self.catalog,
            self.scribe.as_ref(),
            auth,
            request,
        )
        .await
    }

    /// Project one OTLP logs export through the Gate-owned Scribe boundary.
    pub async fn ingest_resource_logs(
        &self,
        auth: &vala_ingest::AuthContext,
        request: ExportLogsServiceRequest,
    ) -> Result<LogsOutcome, IngestError> {
        vala_ingest::ingest_resource_logs_to_scribe(
            &self.catalog,
            self.scribe.as_ref(),
            auth,
            request,
        )
        .await
    }
}

fn map_otlp_error(error: IngestError) -> Status {
    if matches!(error, IngestError::WriterBusy) {
        Status::unavailable(error.to_string())
    } else {
        error.into_status()
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
            let mut stream_table: Option<String> = None;
            let mut stream_batch_id: Option<Vec<u8>> = None;
            while let Some(frame) = tokio::time::timeout(limits.idle_deadline, stream.message())
                .await
                .map_err(|_| IngestError::StreamIdle)?
                .map_err(|status| IngestError::StreamProtocolViolation(status.to_string()))? {
                if started.elapsed() > limits.total_deadline {
                    Err(IngestError::StreamIdle)?;
                }
                let (next_sequence, next_count, next_bytes) = validate_frame(
                    &frame,
                    &limits,
                    expected_sequence,
                    frame_count,
                    total_bytes,
                    &mut stream_table,
                    &mut stream_batch_id,
                )?;
                expected_sequence = next_sequence;
                frame_count = next_count;
                total_bytes = next_bytes;
                let rows = run_ingest_frame_to_scribe(
                    &catalog,
                    scribe.as_ref(),
                    &limits,
                    &auth_context,
                    frame.clone(),
                    total_rows,
                ).await?;
                total_rows = total_rows.saturating_add(rows);
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

fn validate_frame(
    frame: &InsertBatchRequest,
    limits: &IngestLimits,
    expected_sequence: u64,
    frame_count: u64,
    total_bytes: u64,
    stream_table: &mut Option<String>,
    stream_batch_id: &mut Option<Vec<u8>>,
) -> Result<(u64, u64, u64), IngestError> {
    if frame.frame_sequence != expected_sequence {
        return Err(IngestError::StreamProtocolViolation(format!(
            "frame_sequence must be {expected_sequence}, got {}",
            frame.frame_sequence
        )));
    }
    let next_count = frame_count.saturating_add(1);
    if next_count > limits.max_stream_frames {
        return Err(IngestError::StreamProtocolViolation(format!(
            "stream exceeded {} frames",
            limits.max_stream_frames
        )));
    }
    if frame.table.is_empty() || frame.wyrd_batch_id.len() != 16 {
        return Err(IngestError::StreamProtocolViolation(
            "table and exactly 16-byte wyrd_batch_id are required on every frame".to_owned(),
        ));
    }
    if let Some(table) = stream_table {
        if table != &frame.table {
            return Err(IngestError::StreamProtocolViolation(
                "table changed mid-stream; every frame must repeat the established table"
                    .to_owned(),
            ));
        }
    } else {
        *stream_table = Some(frame.table.clone());
    }
    if let Some(batch_id) = stream_batch_id {
        if batch_id != &frame.wyrd_batch_id {
            return Err(IngestError::StreamProtocolViolation(
                "wyrd_batch_id changed mid-stream; every frame must repeat the established batch"
                    .to_owned(),
            ));
        }
    } else {
        *stream_batch_id = Some(frame.wyrd_batch_id.clone());
    }
    let batch_id = uuid::Uuid::from_bytes(
        frame
            .wyrd_batch_id
            .as_slice()
            .try_into()
            .map_err(|_| IngestError::StreamProtocolViolation("invalid batch id".to_owned()))?,
    );
    if batch_id.get_version() != Some(uuid::Version::SortRand) {
        return Err(IngestError::StreamProtocolViolation(
            "wyrd_batch_id must be UUIDv7".to_owned(),
        ));
    }
    if frame.arrow_ipc.len() > limits.max_frame_bytes {
        return Err(IngestError::PayloadTooLarge {
            bytes: frame.arrow_ipc.len() as u64,
            limit: limits.max_frame_bytes as u64,
        });
    }
    let next_bytes = total_bytes.saturating_add(frame.arrow_ipc.len() as u64);
    if next_bytes > limits.max_stream_bytes {
        return Err(IngestError::BatchTooLarge {
            bytes: next_bytes,
            limit: limits.max_stream_bytes,
        });
    }
    Ok((expected_sequence.saturating_add(1), next_count, next_bytes))
}

#[wyrd_tonic::tonic::async_trait]
impl<R: PermissionResolver + 'static, I: IssuerConfigResolver + 'static> TraceService
    for Gate<R, I>
{
    async fn export(
        &self,
        request: Request<ExportTraceServiceRequest>,
    ) -> Result<Response<ExportTraceServiceResponse>, Status> {
        let auth = self
            .auth
            .authenticate(request.metadata())
            .await
            .map_err(Status::from)?;
        let outcome = self
            .ingest_resource_spans(&auth, request.into_inner())
            .await
            .map_err(map_otlp_error)?;
        Ok(Response::new(ExportTraceServiceResponse {
            partial_success: outcome.partial_success(),
        }))
    }
}

#[wyrd_tonic::tonic::async_trait]
impl<R: PermissionResolver + 'static, I: IssuerConfigResolver + 'static> MetricsService
    for Gate<R, I>
{
    async fn export(
        &self,
        request: Request<ExportMetricsServiceRequest>,
    ) -> Result<Response<ExportMetricsServiceResponse>, Status> {
        let auth = self
            .auth
            .authenticate(request.metadata())
            .await
            .map_err(Status::from)?;
        let outcome = self
            .ingest_resource_metrics(&auth, request.into_inner())
            .await
            .map_err(map_otlp_error)?;
        Ok(Response::new(ExportMetricsServiceResponse {
            partial_success: outcome.partial_success(),
        }))
    }
}

#[wyrd_tonic::tonic::async_trait]
impl<R: PermissionResolver + 'static, I: IssuerConfigResolver + 'static> LogsService
    for Gate<R, I>
{
    async fn export(
        &self,
        request: Request<ExportLogsServiceRequest>,
    ) -> Result<Response<ExportLogsServiceResponse>, Status> {
        let auth = self
            .auth
            .authenticate(request.metadata())
            .await
            .map_err(Status::from)?;
        let outcome = self
            .ingest_resource_logs(&auth, request.into_inner())
            .await
            .map_err(map_otlp_error)?;
        Ok(Response::new(ExportLogsServiceResponse {
            partial_success: outcome.partial_success(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::{oracle_unavailable, validate_frame};
    use vala_ingest::IngestLimits;
    use wyrd_tonic::wyrd::v1::InsertBatchRequest;

    #[test]
    fn gate_oracle_placeholder_fails_service_unavailable() {
        assert_eq!(
            oracle_unavailable().code(),
            "WYRD_SERVER_503_SERVICE_UNAVAILABLE"
        );
    }

    fn frame(sequence: u64, table: &str, batch_id: Vec<u8>, bytes: usize) -> InsertBatchRequest {
        InsertBatchRequest {
            table: table.to_owned(),
            arrow_ipc: vec![0; bytes],
            wyrd_batch_id: batch_id,
            frame_sequence: sequence,
        }
    }

    #[test]
    fn frame_requires_repeated_table_and_uuidv7_batch_id() {
        let batch_id = uuid::Uuid::now_v7().as_bytes().to_vec();
        let mut table = None;
        let mut identity = None;
        validate_frame(
            &frame(0, "vala.bifrost.events", batch_id.clone(), 1),
            &IngestLimits::default(),
            0,
            0,
            0,
            &mut table,
            &mut identity,
        )
        .expect("first frame");
        let error = validate_frame(
            &frame(1, "vala.bifrost.other", batch_id, 1),
            &IngestLimits::default(),
            1,
            1,
            1,
            &mut table,
            &mut identity,
        )
        .expect_err("changed table");
        assert!(matches!(
            error,
            vala_ingest::IngestError::StreamProtocolViolation(_)
        ));
        let error = validate_frame(
            &frame(
                1,
                "vala.bifrost.events",
                uuid::Uuid::now_v7().as_bytes().to_vec(),
                1,
            ),
            &IngestLimits::default(),
            1,
            1,
            1,
            &mut table,
            &mut identity,
        )
        .expect_err("changed batch id");
        assert!(matches!(
            error,
            vala_ingest::IngestError::StreamProtocolViolation(_)
        ));
    }

    #[test]
    fn frame_sequence_must_be_contiguous() {
        let mut table = None;
        let mut identity = None;
        let error = validate_frame(
            &frame(
                2,
                "vala.bifrost.events",
                uuid::Uuid::now_v7().as_bytes().to_vec(),
                1,
            ),
            &IngestLimits::default(),
            0,
            0,
            0,
            &mut table,
            &mut identity,
        )
        .expect_err("sequence gap");
        assert!(matches!(
            error,
            vala_ingest::IngestError::StreamProtocolViolation(_)
        ));
    }

    #[test]
    fn frame_limit_accepts_32_mib_and_rejects_32_mib_plus_one() {
        let limits = IngestLimits::default();
        let mut table = None;
        let mut identity = None;
        let id = uuid::Uuid::now_v7().as_bytes().to_vec();
        validate_frame(
            &frame(0, "vala.bifrost.events", id.clone(), limits.max_frame_bytes),
            &limits,
            0,
            0,
            0,
            &mut table,
            &mut identity,
        )
        .expect("aggregate validator accepts the exact stream budget");
        assert_eq!(limits.max_frame_bytes, 32 * 1024 * 1024);
        let error = validate_frame(
            &frame(1, "vala.bifrost.events", id, limits.max_frame_bytes + 1),
            &limits,
            1,
            1,
            limits.max_frame_bytes as u64,
            &mut table,
            &mut identity,
        )
        .expect_err("one byte over the frame cap");
        assert!(matches!(
            error,
            vala_ingest::IngestError::PayloadTooLarge { .. }
        ));
    }
}
