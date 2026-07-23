//! Bifrost Gate — the server-independent auth, admission, and dispatch boundary.

pub mod auth;
pub mod collector;
pub mod error;
pub mod limits;

use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use arrow::record_batch::RecordBatch;
use async_stream::try_stream;
use async_trait::async_trait;
use futures_util::Stream;
use wyrd_auth_oidc::IssuerConfigResolver;
use wyrd_auth_verify::PermissionResolver;
use wyrd_runtime::PermissionCheck;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::DataTenantId;
use wyrd_tonic::otlp::logs_service::logs_service_server::LogsService;
use wyrd_tonic::otlp::logs_service::{ExportLogsServiceRequest, ExportLogsServiceResponse};
use wyrd_tonic::otlp::metrics_service::metrics_service_server::MetricsService;
use wyrd_tonic::otlp::metrics_service::{
    ExportMetricsServiceRequest, ExportMetricsServiceResponse,
};
use wyrd_tonic::otlp::trace_service::trace_service_server::TraceService;
use wyrd_tonic::otlp::trace_service::{ExportTraceServiceRequest, ExportTraceServiceResponse};
use wyrd_tonic::tonic::metadata::MetadataMap;
use wyrd_tonic::tonic::{Request, Response, Status, Streaming};
use wyrd_tonic::wyrd::v1::bifrost_ingest_service_server::{
    BifrostIngestService, BifrostIngestServiceServer,
};
use wyrd_tonic::wyrd::v1::{InsertBatchRequest, InsertBatchResponse};

use crate::catalog::{BifrostCatalog, BifrostCatalogError, TableRef, TenantTableBinding};
use crate::contracts::{IngressPayload, Scribe, ScribeIngressFrame};
pub use crate::gate::auth::{AuthContext, IngestAuthInterceptor, WYRD_REQUEST_ID_METADATA};
pub use crate::gate::collector::{
    IngestOutcome, IngressCpuProjection, InlineProjectionExecutor, LogsOutcome, MetricsOutcome,
    ProjectionExecutor, project_resource_logs, project_resource_metrics, project_resource_spans,
    source_schema_fingerprint,
};
pub use crate::gate::error::{CatalogError, IngestError};
pub use crate::gate::limits::{IngestLimits, StreamSemaphores};
use crate::namespaces::BifrostNamespace;
use crate::schema::fingerprint::SchemaFingerprint;

/// Catalog lookup capability required by Gate. Server integrations provide the
/// adapter; Gate does not depend on a concrete catalog implementation.
#[async_trait]
pub trait Catalog: Send + Sync {
    async fn ensure_builtin_for_table(
        &self,
        _namespace: BifrostNamespace,
        _table: &str,
        _tenant: DataTenantId,
    ) -> Result<(), CatalogError> {
        Ok(())
    }

    async fn table_schema_fingerprint(
        &self,
        namespace: BifrostNamespace,
        table: &str,
        tenant: DataTenantId,
    ) -> Result<SchemaFingerprint, CatalogError>;
}

#[async_trait]
impl Catalog for BifrostCatalog {
    async fn ensure_builtin_for_table(
        &self,
        namespace: BifrostNamespace,
        table: &str,
        tenant: DataTenantId,
    ) -> Result<(), CatalogError> {
        let Some(definition) = crate::tables::builtin_table(
            namespace.as_str().strip_prefix("vala.").unwrap_or_default(),
            table,
        ) else {
            return Ok(());
        };
        self.ensure_builtin(tenant, definition)
            .await
            .map(|_| ())
            .map_err(|error| CatalogError::Internal(error.to_string()))
    }

    async fn table_schema_fingerprint(
        &self,
        namespace: BifrostNamespace,
        table: &str,
        tenant: DataTenantId,
    ) -> Result<SchemaFingerprint, CatalogError> {
        let table_ref = TableRef::new(namespace, table);
        BifrostCatalog::table_schema_fingerprint(self, &table_ref, tenant)
            .await
            .map_err(|error| match error {
                BifrostCatalogError::TableNotFound(table) => CatalogError::TableNotFound(table),
                BifrostCatalogError::FingerprintMismatch(table) => {
                    CatalogError::FingerprintMismatch(table)
                }
                other => CatalogError::Internal(other.to_string()),
            })
    }
}

/// Narrow future read capability owned by the Redux Gate.
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

fn record_gate_event(event: &'static str) {
    metrics::counter!("bifrost_gate_events_total", "stage" => event).increment(1);
}

fn record_gate_rows(accepted: i64, rejected: i64) {
    metrics::counter!("bifrost_gate_rows_total", "status" => "accepted")
        .increment(u64::try_from(accepted).unwrap_or(0));
    metrics::counter!("bifrost_gate_rows_total", "status" => "rejected")
        .increment(u64::try_from(rejected).unwrap_or(0));
}

/// The concrete Bifrost write boundary.
///
/// Gate owns authentication, stream bounds, tenant concurrency, and transport
/// response ordering. The Scribe dependency is mandatory at construction.
#[derive(Clone)]
pub struct Gate<
    C: Catalog + 'static,
    R: PermissionResolver + 'static,
    I: IssuerConfigResolver + 'static,
> {
    catalog: Arc<C>,
    scribe: Arc<dyn Scribe>,
    oracle: Option<Arc<dyn Oracle>>,
    limits: IngestLimits,
    projection: Arc<dyn ProjectionExecutor>,
    semaphores: Arc<StreamSemaphores>,
    auth: IngestAuthInterceptor<R, I>,
    closed: Arc<AtomicBool>,
}

impl<C: Catalog + 'static, R: PermissionResolver + 'static, I: IssuerConfigResolver + 'static>
    Gate<C, R, I>
{
    /// Construct a Gate with a required Scribe capability.
    #[must_use]
    pub fn with_scribe(
        catalog: Arc<C>,
        scribe: Arc<dyn Scribe>,
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
            projection: Arc::new(InlineProjectionExecutor),
            semaphores,
            auth,
            closed: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Construct a Gate whose protocol projection runs on an injected bounded
    /// ingress CPU executor.
    #[must_use]
    pub fn with_scribe_and_projection(
        catalog: Arc<C>,
        scribe: Arc<dyn Scribe>,
        auth: IngestAuthInterceptor<R, I>,
        limits: IngestLimits,
        projection: Arc<dyn ProjectionExecutor>,
    ) -> Self {
        let mut gate = Self::with_scribe(catalog, scribe, auth, limits);
        gate.projection = projection;
        gate
    }

    /// Construct a Gate with an explicit future Oracle capability.
    #[must_use]
    pub fn with_scribe_and_oracle(
        catalog: Arc<C>,
        scribe: Arc<dyn Scribe>,
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

    /// Stop accepting new streams and frames.
    pub fn close(&self) {
        if !self.closed.swap(true, Ordering::AcqRel) {
            record_gate_event("close");
            tracing::info!("bifrost gate closed");
        }
    }

    fn ensure_open(&self) -> Result<(), IngestError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(IngestError::WriterClosed);
        }
        Ok(())
    }

    async fn authenticate(&self, metadata: &MetadataMap) -> Result<AuthContext, IngestError> {
        self.ensure_open()?;
        let started = std::time::Instant::now();
        record_gate_event("auth_attempt");
        match self.auth.authenticate(metadata).await {
            Ok(auth) => {
                metrics::histogram!("bifrost_gate_resolution_seconds", "stage" => "auth")
                    .record(started.elapsed().as_secs_f64());
                Ok(auth)
            }
            Err(error) => {
                record_gate_event("auth_rejection");
                metrics::counter!("bifrost_gate_frames_total", "status" => "rejected").increment(1);
                Err(error)
            }
        }
    }

    /// Mount the Gate on the shared tonic router.
    #[must_use]
    pub fn into_server(self) -> BifrostIngestServiceServer<Self> {
        let size = self.limits.max_decoding_message_size;
        BifrostIngestServiceServer::new(self).max_decoding_message_size(size)
    }

    /// Project one OTLP trace export through the Gate-owned Scribe boundary.
    #[tracing::instrument(skip_all, fields(tenant = %auth.tenant, request_id = %auth.request_id))]
    pub async fn ingest_resource_spans(
        &self,
        auth: &AuthContext,
        request: ExportTraceServiceRequest,
    ) -> Result<IngestOutcome, IngestError> {
        self.ensure_open()?;
        record_gate_event("otlp_export");
        if let Err(error) = authorize_record_write(auth) {
            record_gate_event("otlp_rejection");
            return Err(error);
        }
        let projected = match self.projection.project_spans(request).await {
            Ok(projected) => projected,
            Err(error) => {
                record_gate_event("projection_failure");
                record_gate_event("otlp_rejection");
                return Err(error);
            }
        };
        record_gate_rows(
            projected.outcome.accepted_spans,
            projected.outcome.rejected_spans,
        );
        if let Some(batch) = projected.batch
            && let Err(error) = self
                .dispatch_projected(auth, "vala.traces.spans", batch, projected.source_bytes)
                .await
        {
            record_gate_event("otlp_rejection");
            return Err(error);
        }
        Ok(projected.outcome)
    }

    /// Project one OTLP metrics export through the Gate-owned Scribe boundary.
    #[tracing::instrument(skip_all, fields(tenant = %auth.tenant, request_id = %auth.request_id))]
    pub async fn ingest_resource_metrics(
        &self,
        auth: &AuthContext,
        request: ExportMetricsServiceRequest,
    ) -> Result<MetricsOutcome, IngestError> {
        self.ensure_open()?;
        record_gate_event("otlp_export");
        if let Err(error) = authorize_record_write(auth) {
            record_gate_event("otlp_rejection");
            return Err(error);
        }
        let projected = match self.projection.project_metrics(request).await {
            Ok(projected) => projected,
            Err(error) => {
                record_gate_event("projection_failure");
                record_gate_event("otlp_rejection");
                return Err(error);
            }
        };
        record_gate_rows(
            projected.outcome.accepted_points,
            projected.outcome.rejected_points,
        );
        if let Some(batch) = projected.batch
            && let Err(error) = self
                .dispatch_projected(auth, "vala.metrics.points", batch, projected.source_bytes)
                .await
        {
            record_gate_event("otlp_rejection");
            return Err(error);
        }
        Ok(projected.outcome)
    }

    /// Project one OTLP logs export through the Gate-owned Scribe boundary.
    #[tracing::instrument(skip_all, fields(tenant = %auth.tenant, request_id = %auth.request_id))]
    pub async fn ingest_resource_logs(
        &self,
        auth: &AuthContext,
        request: ExportLogsServiceRequest,
    ) -> Result<LogsOutcome, IngestError> {
        self.ensure_open()?;
        record_gate_event("otlp_export");
        if let Err(error) = authorize_record_write(auth) {
            record_gate_event("otlp_rejection");
            return Err(error);
        }
        let projected = match self.projection.project_logs(request).await {
            Ok(projected) => projected,
            Err(error) => {
                record_gate_event("projection_failure");
                record_gate_event("otlp_rejection");
                return Err(error);
            }
        };
        record_gate_rows(
            projected.outcome.accepted_records,
            projected.outcome.rejected_records,
        );
        if let Some(batch) = projected.batch
            && let Err(error) = self
                .dispatch_projected(auth, "vala.logs.records", batch, projected.source_bytes)
                .await
        {
            record_gate_event("otlp_rejection");
            return Err(error);
        }
        Ok(projected.outcome)
    }

    #[tracing::instrument(
        skip_all,
        fields(tenant = %auth.tenant, table = table_fqn, request_id = %auth.request_id,
               rows = rows.num_rows(), wire_bytes = measured_wire_bytes)
    )]
    async fn dispatch_projected(
        &self,
        auth: &AuthContext,
        table_fqn: &str,
        rows: RecordBatch,
        measured_wire_bytes: usize,
    ) -> Result<(), IngestError> {
        self.ensure_open()?;
        let (namespace, name) = resolve_fqn(table_fqn)?;
        if namespace == BifrostNamespace::System {
            return Err(IngestError::SystemTableWriteDenied {
                table: table_fqn.to_owned(),
            });
        }
        self.catalog
            .ensure_builtin_for_table(namespace, &name, auth.tenant)
            .await
            .map_err(|error| {
                record_gate_event("catalog_failure");
                IngestError::from_catalog(error)
            })?;
        let registered_fingerprint = self
            .catalog
            .table_schema_fingerprint(namespace, &name, auth.tenant)
            .await
            .map_err(|error| {
                record_gate_event("catalog_failure");
                IngestError::from_catalog(error)
            })?;
        let table = TableRef::new(namespace, name);
        let binding = TenantTableBinding::resolve((auth.tenant, table.clone()))
            .map_err(|_| IngestError::Internal("invalid tenant/table binding".to_owned()))?;
        let audit_event = wyrd_spec::vala::api::AuditEvent {
            request_id: auth.request_id.clone(),
            trace_id: None,
            operation: "bifrost.otlp".to_owned(),
            resource: table.fqn(),
            card_ref: auth.principal.card_ref().cloned(),
            principal_id: auth.principal.id,
            principal_kind: auth.principal.kind.tag(),
            auth_method: wyrd_spec::vala::api::AuthMethod::Jwt,
            permission: "bifrost:record:write".to_owned(),
            decision: wyrd_spec::vala::api::AuditDecision::Allow,
            result: wyrd_spec::vala::api::AuditResult::Success,
            payload_summary: "one projected OTLP frame".to_owned(),
            detail: None,
        };
        self.scribe
            .ingest_frame(ScribeIngressFrame {
                principal: auth.principal.clone(),
                binding,
                expected_schema_fingerprint: registered_fingerprint,
                request_id: auth.request_id.clone(),
                batch_id: uuid::Uuid::now_v7(),
                frame_sequence: 0,
                audit_event,
                measured_wire_bytes,
                stream_rows_before: 0,
                stream_rows_limit: u64::MAX,
                payload: IngressPayload::ProjectedArrow(vec![rows]),
            })
            .await
            .map_err(|error| {
                record_gate_event("scribe_failure");
                IngestError::from_scribe(error)
            })
            .map(|_| ())
    }

    /// Resolve, authorize, stamp, and dispatch one native frame to Scribe.
    ///
    /// Gate owns permission, catalog, binding, audit, and whole-stream dispatch
    /// decisions; the server only supplies the catalog adapter and mounts this
    /// service.
    #[tracing::instrument(
        skip_all,
        fields(tenant = %auth.tenant, table = %frame.table, request_id = %auth.request_id,
               frame_sequence = frame.frame_sequence)
    )]
    async fn dispatch_native_frame(
        &self,
        limits: &IngestLimits,
        auth: &AuthContext,
        frame: InsertBatchRequest,
        stream_rows_before: u64,
    ) -> Result<u64, IngestError> {
        self.ensure_open()?;
        let resolution_started = std::time::Instant::now();
        record_gate_event("native_frame");
        metrics::counter!("bifrost_gate_frame_bytes_total")
            .increment(u64::try_from(frame.arrow_ipc.len()).unwrap_or(u64::MAX));
        if frame.arrow_ipc.len() > limits.max_frame_bytes {
            metrics::counter!("bifrost_gate_frames_total", "status" => "rejected").increment(1);
            return Err(IngestError::PayloadTooLarge {
                bytes: frame.arrow_ipc.len() as u64,
                limit: limits.max_frame_bytes as u64,
            });
        }
        let batch_id: [u8; 16] = frame.wyrd_batch_id.as_ref().try_into().map_err(|_| {
            IngestError::StreamProtocolViolation(
                "wyrd_batch_id must be exactly 16 bytes".to_owned(),
            )
        })?;
        let batch_id = uuid::Uuid::from_bytes(batch_id);
        if batch_id.get_version() != Some(uuid::Version::SortRand) {
            return Err(IngestError::StreamProtocolViolation(
                "wyrd_batch_id must be UUIDv7".to_owned(),
            ));
        }
        wyrd_runtime::RbacCheck
            .check(
                &auth.principal,
                &wyrd_runtime::Permission::bifrost_record_write(),
            )
            .into_result()
            .map_err(IngestError::from_rbac)?;
        let (namespace, name) = resolve_fqn(&frame.table)?;
        if namespace == BifrostNamespace::System {
            return Err(IngestError::SystemTableWriteDenied { table: frame.table });
        }
        self.catalog
            .ensure_builtin_for_table(namespace, &name, auth.tenant)
            .await
            .map_err(|error| {
                record_gate_event("catalog_failure");
                metrics::counter!("bifrost_gate_frames_total", "status" => "rejected").increment(1);
                IngestError::from_catalog(error)
            })?;
        let registered_fingerprint = self
            .catalog
            .table_schema_fingerprint(namespace, &name, auth.tenant)
            .await
            .map_err(|error| {
                record_gate_event("catalog_failure");
                metrics::counter!("bifrost_gate_frames_total", "status" => "rejected").increment(1);
                IngestError::from_catalog(error)
            })?;
        let table = TableRef::new(namespace, name);
        let binding = TenantTableBinding::resolve((auth.tenant, table.clone()))
            .map_err(|_| IngestError::Internal("invalid tenant/table binding".to_owned()))?;
        let audit_event = wyrd_spec::vala::api::AuditEvent {
            request_id: auth.request_id.clone(),
            trace_id: None,
            operation: "bifrost.ingest_frame".to_owned(),
            resource: table.fqn(),
            card_ref: auth.principal.card_ref().cloned(),
            principal_id: auth.principal.id,
            principal_kind: auth.principal.kind.tag(),
            auth_method: wyrd_spec::vala::api::AuthMethod::Jwt,
            permission: "bifrost:record:write".to_owned(),
            decision: wyrd_spec::vala::api::AuditDecision::Allow,
            result: wyrd_spec::vala::api::AuditResult::Success,
            payload_summary: "one bounded native frame".to_owned(),
            detail: None,
        };
        let admission = self
            .scribe
            .ingest_frame(ScribeIngressFrame {
                principal: auth.principal.clone(),
                binding,
                expected_schema_fingerprint: registered_fingerprint,
                request_id: auth.request_id.clone(),
                batch_id,
                frame_sequence: frame.frame_sequence,
                audit_event,
                measured_wire_bytes: frame.arrow_ipc.len(),
                stream_rows_before,
                stream_rows_limit: limits.max_stream_rows,
                payload: IngressPayload::ArrowIpc(frame.arrow_ipc),
            })
            .await
            .map_err(|error| {
                record_gate_event("scribe_failure");
                metrics::counter!("bifrost_gate_frames_total", "status" => "rejected").increment(1);
                IngestError::from_scribe(error)
            })?;
        metrics::counter!("bifrost_gate_frames_total", "status" => "accepted").increment(1);
        metrics::histogram!("bifrost_gate_resolution_seconds")
            .record(resolution_started.elapsed().as_secs_f64());
        Ok(admission.rows_accepted)
    }
}

fn authorize_record_write(auth: &AuthContext) -> Result<(), IngestError> {
    wyrd_runtime::RbacCheck
        .check(
            &auth.principal,
            &wyrd_runtime::Permission::bifrost_record_write(),
        )
        .into_result()
        .map_err(IngestError::from_rbac)
}

fn resolve_fqn(fqn: &str) -> Result<(BifrostNamespace, String), IngestError> {
    BifrostNamespace::split_fqn(fqn).ok_or_else(|| {
        IngestError::StreamProtocolViolation(format!("unrecognized table fqn: {fqn}"))
    })
}

fn map_otlp_error(error: IngestError) -> Status {
    error.into_status()
}

#[wyrd_tonic::tonic::async_trait]
impl<C: Catalog + 'static, R: PermissionResolver + 'static, I: IssuerConfigResolver + 'static>
    BifrostIngestService for Gate<C, R, I>
{
    type InsertBatchStream =
        Pin<Box<dyn Stream<Item = Result<InsertBatchResponse, Status>> + Send>>;

    async fn insert_batch(
        &self,
        request: Request<Streaming<InsertBatchRequest>>,
    ) -> Result<Response<Self::InsertBatchStream>, Status> {
        let auth = self
            .authenticate(request.metadata())
            .await
            .map_err(Status::from)?;
        let _permit = self.semaphores.acquire(auth.tenant)?;
        let mut stream = request.into_inner();
        let gate = Self {
            catalog: Arc::clone(&self.catalog),
            scribe: Arc::clone(&self.scribe),
            oracle: self.oracle.clone(),
            limits: self.limits.clone(),
            projection: Arc::clone(&self.projection),
            semaphores: Arc::clone(&self.semaphores),
            auth: self.auth.clone(),
            closed: Arc::clone(&self.closed),
        };
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
                let rows = gate.dispatch_native_frame(
                    &limits,
                    &auth_context,
                    frame.clone(),
                    total_rows,
                ).await.inspect_err(|_error| {
                    record_gate_event("native_rejection");
                })?;
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
        if batch_id.as_slice() != frame.wyrd_batch_id.as_ref() {
            return Err(IngestError::StreamProtocolViolation(
                "wyrd_batch_id changed mid-stream; every frame must repeat the established batch"
                    .to_owned(),
            ));
        }
    } else {
        *stream_batch_id = Some(frame.wyrd_batch_id.to_vec());
    }
    let batch_id = uuid::Uuid::from_bytes(
        frame
            .wyrd_batch_id
            .as_ref()
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
impl<C: Catalog + 'static, R: PermissionResolver + 'static, I: IssuerConfigResolver + 'static>
    TraceService for Gate<C, R, I>
{
    async fn export(
        &self,
        request: Request<ExportTraceServiceRequest>,
    ) -> Result<Response<ExportTraceServiceResponse>, Status> {
        let auth = self
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
impl<C: Catalog + 'static, R: PermissionResolver + 'static, I: IssuerConfigResolver + 'static>
    MetricsService for Gate<C, R, I>
{
    async fn export(
        &self,
        request: Request<ExportMetricsServiceRequest>,
    ) -> Result<Response<ExportMetricsServiceResponse>, Status> {
        let auth = self
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
impl<C: Catalog + 'static, R: PermissionResolver + 'static, I: IssuerConfigResolver + 'static>
    LogsService for Gate<C, R, I>
{
    async fn export(
        &self,
        request: Request<ExportLogsServiceRequest>,
    ) -> Result<Response<ExportLogsServiceResponse>, Status> {
        let auth = self
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
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::collector::{IngestOutcome, ProjectedExport, ProjectionExecutor};
    use super::error::CatalogError;
    use super::limits::IngestLimits;
    use super::{AuthContext, Catalog, Gate, IngestError, oracle_unavailable, validate_frame};
    use crate::catalog::{TableRef, TenantTableBinding, TenantTableBindingError};
    use crate::namespaces::BifrostNamespace;
    use crate::schema::fingerprint::SchemaFingerprint;
    use arrow::array::Int64Array;
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::record_batch::RecordBatch;
    use async_trait::async_trait;
    use wyrd_auth_oidc::IssuerConfigResolver;
    use wyrd_auth_verify::PermissionResolver;
    use wyrd_runtime::{Permission, PermissionSet, Principal, PrincipalKind};
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::ids::DataTenantId;
    use wyrd_spec::request_id::RequestId;
    use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;
    use wyrd_tonic::wyrd::v1::InsertBatchRequest;

    #[test]
    fn gate_oracle_placeholder_fails_service_unavailable() {
        assert_eq!(
            oracle_unavailable().code(),
            "WYRD_SERVER_503_SERVICE_UNAVAILABLE"
        );
    }

    fn frame(sequence: u64, table: &str, batch_id: &[u8], bytes: usize) -> InsertBatchRequest {
        InsertBatchRequest {
            table: table.to_owned(),
            arrow_ipc: bytes::Bytes::from(vec![0; bytes]),
            wyrd_batch_id: bytes::Bytes::copy_from_slice(batch_id),
            frame_sequence: sequence,
        }
    }

    #[test]
    fn frame_requires_repeated_table_and_uuidv7_batch_id() {
        let batch_id = uuid::Uuid::now_v7().as_bytes().to_vec();
        let mut table = None;
        let mut identity = None;
        validate_frame(
            &frame(0, "vala.bifrost.events", &batch_id, 1),
            &IngestLimits::default(),
            0,
            0,
            0,
            &mut table,
            &mut identity,
        )
        .expect("first frame");
        let error = validate_frame(
            &frame(1, "vala.bifrost.other", &batch_id, 1),
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
            super::error::IngestError::StreamProtocolViolation(_)
        ));
        let error = validate_frame(
            &frame(1, "vala.bifrost.events", uuid::Uuid::now_v7().as_bytes(), 1),
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
            super::error::IngestError::StreamProtocolViolation(_)
        ));
    }

    #[test]
    fn frame_sequence_must_be_contiguous() {
        let mut table = None;
        let mut identity = None;
        let error = validate_frame(
            &frame(2, "vala.bifrost.events", uuid::Uuid::now_v7().as_bytes(), 1),
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
            super::error::IngestError::StreamProtocolViolation(_)
        ));
    }

    #[test]
    fn frame_limit_accepts_32_mib_and_rejects_32_mib_plus_one() {
        let limits = IngestLimits::default();
        let mut table = None;
        let mut identity = None;
        let id = uuid::Uuid::now_v7().as_bytes().to_vec();
        validate_frame(
            &frame(0, "vala.bifrost.events", &id, limits.max_frame_bytes),
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
            &frame(1, "vala.bifrost.events", &id, limits.max_frame_bytes + 1),
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
            super::error::IngestError::PayloadTooLarge { .. }
        ));
    }

    #[derive(Debug)]
    struct TestCatalog;

    #[async_trait]
    impl Catalog for TestCatalog {
        async fn table_schema_fingerprint(
            &self,
            _namespace: BifrostNamespace,
            _table: &str,
            _tenant: DataTenantId,
        ) -> Result<SchemaFingerprint, CatalogError> {
            Ok(SchemaFingerprint([0; 32]))
        }
    }

    #[derive(Debug)]
    struct TestPermissionResolver;

    impl PermissionResolver for TestPermissionResolver {
        async fn resolve(
            &self,
            _tenant_id: &DataTenantId,
            _roles: &[wyrd_runtime::RoleRef],
        ) -> Result<wyrd_runtime::PermissionSet, wyrd_auth_verify::ResolveError> {
            Ok(wyrd_runtime::PermissionSet::new())
        }
    }

    #[derive(Debug)]
    struct TestIssuerResolver;

    impl IssuerConfigResolver for TestIssuerResolver {
        async fn trusted_issuers(
            &self,
            _tenant: &DataTenantId,
        ) -> Result<Vec<wyrd_auth_oidc::TrustedIssuer>, wyrd_auth_oidc::OidcError> {
            Ok(Vec::new())
        }
    }

    struct TestScribe;

    #[async_trait]
    impl crate::contracts::Scribe for TestScribe {
        async fn ingest_frame(
            &self,
            _frame: crate::contracts::ScribeIngressFrame,
        ) -> Result<crate::contracts::FrameAdmission, crate::contracts::ScribeError> {
            Ok(crate::contracts::FrameAdmission { rows_accepted: 0 })
        }
    }

    struct CountingScribe {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl crate::contracts::Scribe for CountingScribe {
        async fn ingest_frame(
            &self,
            _frame: crate::contracts::ScribeIngressFrame,
        ) -> Result<crate::contracts::FrameAdmission, crate::contracts::ScribeError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            Ok(crate::contracts::FrameAdmission { rows_accepted: 1 })
        }
    }

    struct FailingCatalog;

    #[async_trait]
    impl Catalog for FailingCatalog {
        async fn table_schema_fingerprint(
            &self,
            _namespace: BifrostNamespace,
            table: &str,
            _tenant: DataTenantId,
        ) -> Result<SchemaFingerprint, CatalogError> {
            Err(CatalogError::TableNotFound(table.to_owned()))
        }
    }

    struct FailingProjection {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl ProjectionExecutor for FailingProjection {
        async fn project_spans(
            &self,
            _request: ExportTraceServiceRequest,
        ) -> Result<ProjectedExport<IngestOutcome>, IngestError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            Err(IngestError::Internal(
                "projection should not run".to_owned(),
            ))
        }

        async fn project_metrics(
            &self,
            _request: wyrd_tonic::otlp::metrics_service::ExportMetricsServiceRequest,
        ) -> Result<ProjectedExport<super::collector::MetricsOutcome>, IngestError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            Err(IngestError::Internal(
                "projection should not run".to_owned(),
            ))
        }

        async fn project_logs(
            &self,
            _request: wyrd_tonic::otlp::logs_service::ExportLogsServiceRequest,
        ) -> Result<ProjectedExport<super::collector::LogsOutcome>, IngestError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            Err(IngestError::Internal(
                "projection should not run".to_owned(),
            ))
        }
    }

    struct BatchProjection;

    #[async_trait]
    impl ProjectionExecutor for BatchProjection {
        async fn project_spans(
            &self,
            _request: ExportTraceServiceRequest,
        ) -> Result<ProjectedExport<IngestOutcome>, IngestError> {
            let schema = Arc::new(Schema::new(vec![Field::new(
                "value",
                DataType::Int64,
                false,
            )]));
            let batch = RecordBatch::try_new(schema, vec![Arc::new(Int64Array::from(vec![1]))])
                .map_err(|error| IngestError::Internal(error.to_string()))?;
            Ok(ProjectedExport {
                outcome: IngestOutcome {
                    accepted_spans: 1,
                    rejected_spans: 0,
                    rejection_message: None,
                },
                batch: Some(batch),
                source_bytes: 1,
            })
        }

        async fn project_metrics(
            &self,
            _request: wyrd_tonic::otlp::metrics_service::ExportMetricsServiceRequest,
        ) -> Result<ProjectedExport<super::collector::MetricsOutcome>, IngestError> {
            Err(IngestError::Internal("unused projection".to_owned()))
        }

        async fn project_logs(
            &self,
            _request: wyrd_tonic::otlp::logs_service::ExportLogsServiceRequest,
        ) -> Result<ProjectedExport<super::collector::LogsOutcome>, IngestError> {
            Err(IngestError::Internal("unused projection".to_owned()))
        }
    }

    fn auth_context(with_permission: bool) -> AuthContext {
        let tenant = DataTenantId::new_v7();
        let permissions = if with_permission {
            PermissionSet::from_iter([Permission::bifrost_record_write()])
        } else {
            PermissionSet::new()
        };
        let principal = Principal::new(
            PrincipalId::new(uuid::Uuid::now_v7()),
            PrincipalKind::User,
            tenant,
            Vec::new(),
            permissions,
        );
        AuthContext {
            principal,
            tenant,
            request_id: RequestId::now_v7(),
        }
    }

    fn test_interceptor()
    -> super::auth::IngestAuthInterceptor<TestPermissionResolver, TestIssuerResolver> {
        let mut keys = std::collections::HashMap::new();
        keys.insert(
            wyrd_auth_verify::Kid::new("test").expect("test kid is valid"),
            Arc::new(
                wyrd_auth_verify::public_key_from_pem(
                    b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n",
                )
                .expect("test key is valid"),
            ),
        );
        let verifier = wyrd_auth_verify::TokenVerifier::new(
            keys,
            "wyrd",
            Arc::new(TestPermissionResolver),
            wyrd_auth_verify::WyrdAuthVerifySettings::default(),
        );
        super::auth::ingest_auth_interceptor(Arc::new(verifier))
    }

    #[test]
    fn gate_constructs_with_injected_seams_without_server_boot() {
        let mut keys = std::collections::HashMap::new();
        keys.insert(
            wyrd_auth_verify::Kid::new("test").expect("test kid is valid"),
            Arc::new(
                wyrd_auth_verify::public_key_from_pem(
                    b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n",
                )
                .expect("test key is valid"),
            ),
        );
        let verifier: wyrd_auth_verify::TokenVerifier<TestPermissionResolver, TestIssuerResolver> =
            wyrd_auth_verify::TokenVerifier::new(
                keys,
                "wyrd",
                Arc::new(TestPermissionResolver),
                wyrd_auth_verify::WyrdAuthVerifySettings::default(),
            );
        let gate = Gate::<TestCatalog, TestPermissionResolver, TestIssuerResolver>::with_scribe(
            Arc::new(TestCatalog),
            Arc::new(TestScribe),
            crate::gate::auth::ingest_auth_interceptor(Arc::new(verifier)),
            IngestLimits::default(),
        );
        assert!(gate.dispatch_read().is_err());
    }

    #[tokio::test]
    async fn gate_enforces_bifrost_record_write() {
        let projection_calls = Arc::new(AtomicUsize::new(0));
        let scribe_calls = Arc::new(AtomicUsize::new(0));
        let gate = Gate::with_scribe_and_projection(
            Arc::new(TestCatalog),
            Arc::new(CountingScribe {
                calls: Arc::clone(&scribe_calls),
            }),
            test_interceptor(),
            IngestLimits::default(),
            Arc::new(FailingProjection {
                calls: Arc::clone(&projection_calls),
            }),
        );

        let error = gate
            .ingest_resource_spans(&auth_context(false), ExportTraceServiceRequest::default())
            .await
            .expect_err("permission must be denied");
        assert!(matches!(error, IngestError::RbacDenied { .. }));
        assert_eq!(projection_calls.load(Ordering::Relaxed), 0);
        assert_eq!(scribe_calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn gate_authenticates_before_reading_frames() {
        let gate = Gate::with_scribe(
            Arc::new(TestCatalog),
            Arc::new(TestScribe),
            test_interceptor(),
            IngestLimits::default(),
        );
        let metadata = wyrd_tonic::tonic::metadata::MetadataMap::new();
        assert!(gate.authenticate(&metadata).await.is_err());
    }

    #[test]
    fn gate_resolves_principal_tenant_table_and_binding() {
        let tenant = DataTenantId::new_v7();
        let table = TableRef::new(BifrostNamespace::Bifrost, "events");
        let binding = TenantTableBinding::resolve((tenant, table.clone())).expect("binding");
        assert_eq!(binding.tenant, tenant);
        assert_eq!(binding.table_ref, table);
        assert_eq!(binding.table_name, "events");
        assert!(
            binding
                .object_prefix
                .starts_with(&format!("tenants/{tenant}/"))
        );
    }

    #[test]
    fn gate_rejects_tenant_binding_mismatch() {
        let binding = TenantTableBinding::resolve((
            DataTenantId::new_v7(),
            TableRef::new(BifrostNamespace::Bifrost, "events"),
        ))
        .expect("binding");
        let error = binding
            .validate_authenticated_tenant(DataTenantId::new_v7())
            .expect_err("a binding must not cross tenant boundaries");
        assert!(matches!(
            error,
            TenantTableBindingError::TenantMismatch { .. }
        ));
    }

    #[tokio::test]
    async fn catalog_failure_does_not_call_scribe() {
        let scribe_calls = Arc::new(AtomicUsize::new(0));
        let gate = Gate::with_scribe_and_projection(
            Arc::new(FailingCatalog),
            Arc::new(CountingScribe {
                calls: Arc::clone(&scribe_calls),
            }),
            test_interceptor(),
            IngestLimits::default(),
            Arc::new(BatchProjection),
        );

        let error = gate
            .ingest_resource_spans(&auth_context(true), ExportTraceServiceRequest::default())
            .await
            .expect_err("catalog failure must reject the write");
        assert!(matches!(error, IngestError::TableNotFound { .. }));
        assert_eq!(scribe_calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn close_rejects_new_work_before_projection() {
        let projection_calls = Arc::new(AtomicUsize::new(0));
        let gate = Gate::with_scribe_and_projection(
            Arc::new(TestCatalog),
            Arc::new(TestScribe),
            test_interceptor(),
            IngestLimits::default(),
            Arc::new(FailingProjection {
                calls: Arc::clone(&projection_calls),
            }),
        );
        gate.close();

        let error = gate
            .ingest_resource_spans(&auth_context(true), ExportTraceServiceRequest::default())
            .await
            .expect_err("closed Gate must reject new work");
        assert!(matches!(error, IngestError::WriterClosed));
        assert_eq!(projection_calls.load(Ordering::Relaxed), 0);
    }
}
