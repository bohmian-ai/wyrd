//! OTLP/gRPC trace collector: a drop-in `TraceService` that authenticates,
//! decodes `ExportTraceServiceRequest` to span rows, and writes them through the
//! live group-commit coordinator on the `traces.spans` table.
//!
//! The service is the gRPC edge; the reusable decode→write core is
//! [`OtlpTraceService::ingest_resource_spans`], which Task C's HTTP OTLP path
//! consumes without re-touching tonic. The `traces.spans` table is a
//! `SystemShared` multi-tenant physical table, so the writer opens with
//! [`TableScope::SystemShared`] and the token-derived data tenant is stamped
//! into `data_tenant_id` by the coordinator (never derived from the wire).
//!
//! Backpressure vs. bad data:
//! - `IngestBusy` (coordinator buffer full) → gRPC `UNAVAILABLE`, retryable, no
//!   `partial_success`; no rows from this request were written.
//! - Individual malformed spans → OTLP `partial_success` with `rejected_spans`
//!   and an `error_message`; the request as a whole still succeeds.
//! - Auth failure and a total decode failure → hard error status.

pub mod map;

use std::sync::Arc;

use uuid::Uuid;
use vala_bifrost::tables::logs::RecordsTable;
use vala_bifrost::tables::metrics::PointsTable;
use vala_bifrost::tables::traces::SpansTable;
use vala_bifrost::writer::BifrostWriteContext;
use vala_bifrost::{BifrostNamespace, TableScope, WyrdCatalog};
use vala_bifrost_redux::catalog::TableRef as ReduxTableRef;
use vala_bifrost_redux::contracts::{Scribe, ScribeAppend};
use vala_bifrost_redux::schema::fingerprint::SchemaFingerprint;
use vala_bifrost_redux::scribe::ScribeImpl;
use wyrd_auth_oidc::IssuerConfigResolver;
use wyrd_auth_verify::PermissionResolver;
use wyrd_runtime::{Permission, PermissionCheck, RbacCheck};
use wyrd_tonic::otlp::logs_service::logs_service_server::{LogsService, LogsServiceServer};
use wyrd_tonic::otlp::logs_service::{
    ExportLogsPartialSuccess, ExportLogsServiceRequest, ExportLogsServiceResponse,
};
use wyrd_tonic::otlp::metrics_service::metrics_service_server::{
    MetricsService, MetricsServiceServer,
};
use wyrd_tonic::otlp::metrics_service::{
    ExportMetricsPartialSuccess, ExportMetricsServiceRequest, ExportMetricsServiceResponse,
};
use wyrd_tonic::otlp::trace_service::trace_service_server::{TraceService, TraceServiceServer};
use wyrd_tonic::otlp::trace_service::{
    ExportTracePartialSuccess, ExportTraceServiceRequest, ExportTraceServiceResponse,
};
use wyrd_tonic::prost::Message;
use wyrd_tonic::tonic::{Request, Response, Status};

use crate::auth::{AuthContext, IngestAuthInterceptor};
use crate::error::IngestError;
use map::{
    MappedLogs, MappedMetrics, MappedSpans, logs_to_record_batch, map_resource_logs,
    map_resource_metrics, map_resource_spans, metrics_to_record_batch, spans_to_record_batch,
};

/// Audit origin recorded on every OTLP-sourced commit.
const OTLP_ORIGIN: &str = "otlp";

/// OTLP/gRPC trace receiver over `vala-bifrost`'s group-commit writer.
///
/// Generic over the resolver-backed verifier's `R` / `I`, static-dispatched like
/// [`crate::service::BifrostIngestGrpc`], so it shares the one auth seam.
pub struct OtlpTraceService<R: PermissionResolver + 'static, I: IssuerConfigResolver + 'static> {
    catalog: Arc<WyrdCatalog>,
    auth: IngestAuthInterceptor<R, I>,
    scribe: Option<Arc<ScribeImpl>>,
}

impl<R: PermissionResolver + 'static, I: IssuerConfigResolver + 'static> OtlpTraceService<R, I> {
    /// Construct from the engine catalog and the shared ingest auth interceptor.
    #[must_use]
    pub fn new(catalog: Arc<WyrdCatalog>, auth: IngestAuthInterceptor<R, I>) -> Self {
        Self {
            catalog,
            auth,
            scribe: None,
        }
    }

    /// Construct the OTLP service with the server-owned queued Scribe seam.
    #[must_use]
    pub fn with_scribe(
        catalog: Arc<WyrdCatalog>,
        scribe: Arc<ScribeImpl>,
        auth: IngestAuthInterceptor<R, I>,
    ) -> Self {
        Self {
            catalog,
            auth,
            scribe: Some(scribe),
        }
    }

    /// Wrap into the generated OTLP server type for mounting on the shared
    /// tonic listener.
    #[must_use]
    pub fn into_server(self) -> TraceServiceServer<Self> {
        TraceServiceServer::new(self)
    }

    /// Decode → write core, reusable by Task C's HTTP OTLP path.
    ///
    /// Delegates to the transport-agnostic [`ingest_resource_spans`] free
    /// function against this service's catalog, so the OTLP/gRPC edge and the
    /// OTLP/HTTP handler share one decode→RBAC→commit core.
    ///
    /// # Errors
    /// Returns an [`IngestError`] for auth/RBAC/engine failures (including
    /// [`IngestError::WriterBusy`] on backpressure). Per-span decode failures do
    /// not error here — they surface in [`IngestOutcome::rejected_spans`].
    pub async fn ingest_resource_spans(
        &self,
        auth: &AuthContext,
        request: ExportTraceServiceRequest,
    ) -> Result<IngestOutcome, IngestError> {
        match &self.scribe {
            Some(scribe) => {
                ingest_resource_spans_to_scribe(&self.catalog, scribe, auth, request).await
            }
            None => ingest_resource_spans(&self.catalog, auth, request).await,
        }
    }
}

/// Transport-agnostic OTLP trace decode→write core.
///
/// Authorizes the record-write capability, flattens the spans, writes the
/// accepted rows through the live coordinator on `traces.spans` as one commit
/// unit, and reports the per-span rejection count. Both the OTLP/gRPC collector
/// [`OtlpTraceService::export`] and Task C's OTLP/HTTP handler call this, so the
/// RBAC + map + commit logic lives in exactly one place and is independent of
/// the verifier generics `R`/`I`.
///
/// The `auth` is always token-derived ([`AuthContext::tenant`] is never taken
/// from the wire), so both transports preserve tenant isolation identically.
///
/// # Errors
/// Returns an [`IngestError`] for auth/RBAC/engine failures (including
/// [`IngestError::WriterBusy`] on backpressure). Per-span decode failures do not
/// error here — they surface in [`IngestOutcome::rejected_spans`].
pub async fn ingest_resource_spans(
    catalog: &WyrdCatalog,
    auth: &AuthContext,
    request: ExportTraceServiceRequest,
) -> Result<IngestOutcome, IngestError> {
    // Gate: record-write capability. Tenant isolation is the data partition,
    // stamped server-side; this is the coarse operation gate.
    RbacCheck
        .check(&auth.principal, &Permission::bifrost_record_write())
        .into_result()
        .map_err(IngestError::from_rbac)?;

    let MappedSpans { records, rejected } = map_resource_spans(&request.resource_spans);
    let rejected_spans = rejected.len() as i64;
    let rejection_message = rejected.first().map(|first| first.reason.clone());

    if records.is_empty() {
        return Ok(IngestOutcome {
            accepted_spans: 0,
            rejected_spans,
            rejection_message,
        });
    }

    let batch = spans_to_record_batch(&records).map_err(IngestError::Decode)?;
    let accepted_spans = records.len() as i64;

    // traces.spans is SystemShared: bind SYSTEM_OWNER for the registration /
    // commit rows, stamp the data tenant into data_tenant_id. typed_writer
    // carries the table's Sensitive payload class so the coordinator redacts
    // the `attributes` column at flush.
    let writer = catalog
        .typed_writer::<SpansTable>(TableScope::SystemShared, auth.tenant)
        .await
        .map_err(IngestError::from_engine)?;

    let ctx = BifrostWriteContext {
        batch_id: *uuid::Uuid::now_v7().as_bytes(),
        origin: OTLP_ORIGIN.to_owned(),
        actor: auth.principal.id.to_string(),
        request_id: auth.request_id.clone(),
        card_ref: auth.principal.card_ref().cloned(),
    };

    writer
        .commit_one(auth.tenant, vec![batch], ctx)
        .await
        .map_err(IngestError::from_engine)?;

    Ok(IngestOutcome {
        accepted_spans,
        rejected_spans,
        rejection_message,
    })
}

/// OTLP trace projection into queued Scribe. `encoded_len` is captured before
/// mapping and is carried unchanged as the source charge.
pub async fn ingest_resource_spans_to_scribe(
    catalog: &WyrdCatalog,
    scribe: &ScribeImpl,
    auth: &AuthContext,
    request: ExportTraceServiceRequest,
) -> Result<IngestOutcome, IngestError> {
    authorize_otlp(auth)?;
    let source_bytes = request.encoded_len();
    let MappedSpans { records, rejected } = map_resource_spans(&request.resource_spans);
    let rejected_spans = rejected.len() as i64;
    let rejection_message = rejected.first().map(|first| first.reason.clone());
    if records.is_empty() {
        return Ok(IngestOutcome {
            accepted_spans: 0,
            rejected_spans,
            rejection_message,
        });
    }
    let accepted_spans = records.len() as i64;
    let batch = spans_to_record_batch(&records).map_err(IngestError::Decode)?;
    enqueue_scribe(
        catalog,
        scribe,
        auth,
        "vala.traces.spans",
        batch,
        source_bytes,
    )
    .await?;
    Ok(IngestOutcome {
        accepted_spans,
        rejected_spans,
        rejection_message,
    })
}

/// Result of one accepted OTLP export: how many spans committed and how many the
/// receiver dropped for a per-span reason.
#[derive(Debug)]
pub struct IngestOutcome {
    /// Spans committed to `traces.spans`.
    pub accepted_spans: i64,
    /// Spans dropped for a per-span reason (surfaced as `partial_success`).
    pub rejected_spans: i64,
    /// First rejection reason, for the OTLP `error_message`.
    pub rejection_message: Option<String>,
}

impl IngestOutcome {
    /// Render the OTLP `partial_success` field, or `None` on a full success.
    ///
    /// Shared by both OTLP transports so the `rejected_spans` / `error_message`
    /// contract is identical on gRPC and HTTP.
    #[must_use]
    pub fn partial_success(&self) -> Option<ExportTracePartialSuccess> {
        if self.rejected_spans == 0 {
            return None;
        }
        Some(ExportTracePartialSuccess {
            rejected_spans: self.rejected_spans,
            error_message: self
                .rejection_message
                .clone()
                .unwrap_or_else(|| "one or more spans were rejected".to_owned()),
        })
    }
}

#[wyrd_tonic::tonic::async_trait]
impl<R: PermissionResolver + 'static, I: IssuerConfigResolver + 'static> TraceService
    for OtlpTraceService<R, I>
{
    #[tracing::instrument(
        name = "otlp.trace.export",
        skip_all,
        fields(tenant, accepted_spans, rejected_spans)
    )]
    async fn export(
        &self,
        request: Request<ExportTraceServiceRequest>,
    ) -> Result<Response<ExportTraceServiceResponse>, Status> {
        let auth = self
            .auth
            .authenticate(request.metadata())
            .await
            .map_err(Status::from)?;
        tracing::Span::current().record("tenant", tracing::field::display(auth.tenant));

        let payload = request.into_inner();
        let outcome = self
            .ingest_resource_spans(&auth, payload)
            .await
            .map_err(map_export_error)?;

        tracing::Span::current().record("accepted_spans", outcome.accepted_spans);
        tracing::Span::current().record("rejected_spans", outcome.rejected_spans);

        Ok(Response::new(ExportTraceServiceResponse {
            partial_success: outcome.partial_success(),
        }))
    }
}

/// Map an ingest error onto its gRPC status. Backpressure is remapped to the
/// retryable OTLP `UNAVAILABLE` (rather than `RESOURCE_EXHAUSTED`) so an OTLP
/// SDK retries the whole export.
fn map_export_error(error: IngestError) -> Status {
    match error {
        IngestError::WriterBusy => Status::unavailable(error.to_string()),
        other => other.into_status(),
    }
}

// ─── metrics ───────────────────────────────────────────────────────────────────

/// OTLP/gRPC metrics receiver over `vala-bifrost`'s group-commit writer.
///
/// Mirror of [`OtlpTraceService`] for the metrics signal: authenticates, maps
/// `ExportMetricsServiceRequest` to `metrics.points` rows, and commits them
/// through the live coordinator. `metrics.points` is a `SystemShared` multi-tenant
/// physical table, so the writer opens with [`TableScope::SystemShared`] and the
/// token-derived data tenant is stamped into `data_tenant_id` (never from the wire).
pub struct OtlpMetricsService<R: PermissionResolver + 'static, I: IssuerConfigResolver + 'static> {
    catalog: Arc<WyrdCatalog>,
    auth: IngestAuthInterceptor<R, I>,
    scribe: Option<Arc<ScribeImpl>>,
}

impl<R: PermissionResolver + 'static, I: IssuerConfigResolver + 'static> OtlpMetricsService<R, I> {
    /// Construct from the engine catalog and the shared ingest auth interceptor.
    #[must_use]
    pub fn new(catalog: Arc<WyrdCatalog>, auth: IngestAuthInterceptor<R, I>) -> Self {
        Self {
            catalog,
            auth,
            scribe: None,
        }
    }

    /// Construct the OTLP metrics service with the queued Scribe seam.
    #[must_use]
    pub fn with_scribe(
        catalog: Arc<WyrdCatalog>,
        scribe: Arc<ScribeImpl>,
        auth: IngestAuthInterceptor<R, I>,
    ) -> Self {
        Self {
            catalog,
            auth,
            scribe: Some(scribe),
        }
    }

    /// Wrap into the generated OTLP server type for mounting on the shared tonic
    /// listener.
    #[must_use]
    pub fn into_server(self) -> MetricsServiceServer<Self> {
        MetricsServiceServer::new(self)
    }

    /// Decode → write core, reusable by the HTTP OTLP path.
    ///
    /// # Errors
    /// Returns an [`IngestError`] for auth/RBAC/engine failures (including
    /// [`IngestError::WriterBusy`] on backpressure). Per-point decode failures do
    /// not error here — they surface in [`MetricsOutcome::rejected_points`].
    pub async fn ingest_resource_metrics(
        &self,
        auth: &AuthContext,
        request: ExportMetricsServiceRequest,
    ) -> Result<MetricsOutcome, IngestError> {
        match &self.scribe {
            Some(scribe) => {
                ingest_resource_metrics_to_scribe(&self.catalog, scribe, auth, request).await
            }
            None => ingest_resource_metrics(&self.catalog, auth, request).await,
        }
    }
}

/// Transport-agnostic OTLP metrics decode→write core.
///
/// Authorizes the record-write capability, flattens the data points, writes the
/// accepted rows through the live coordinator on `metrics.points` as one commit
/// unit, and reports the per-point rejection count. Both the OTLP/gRPC collector
/// and the OTLP/HTTP handler call this.
///
/// # Errors
/// Returns an [`IngestError`] for auth/RBAC/engine failures (including
/// [`IngestError::WriterBusy`] on backpressure). Per-point decode failures do not
/// error here — they surface in [`MetricsOutcome::rejected_points`].
pub async fn ingest_resource_metrics(
    catalog: &WyrdCatalog,
    auth: &AuthContext,
    request: ExportMetricsServiceRequest,
) -> Result<MetricsOutcome, IngestError> {
    RbacCheck
        .check(&auth.principal, &Permission::bifrost_record_write())
        .into_result()
        .map_err(IngestError::from_rbac)?;

    let MappedMetrics { records, rejected } = map_resource_metrics(&request.resource_metrics);
    let rejected_points = rejected.len() as i64;
    let rejection_message = rejected.first().map(|first| first.reason.clone());

    if records.is_empty() {
        return Ok(MetricsOutcome {
            accepted_points: 0,
            rejected_points,
            rejection_message,
        });
    }

    let batch = metrics_to_record_batch(&records).map_err(IngestError::Decode)?;
    let accepted_points = records.len() as i64;

    let writer = catalog
        .typed_writer::<PointsTable>(TableScope::SystemShared, auth.tenant)
        .await
        .map_err(IngestError::from_engine)?;

    let ctx = BifrostWriteContext {
        batch_id: *uuid::Uuid::now_v7().as_bytes(),
        origin: OTLP_ORIGIN.to_owned(),
        actor: auth.principal.id.to_string(),
        request_id: auth.request_id.clone(),
        card_ref: auth.principal.card_ref().cloned(),
    };

    writer
        .commit_one(auth.tenant, vec![batch], ctx)
        .await
        .map_err(IngestError::from_engine)?;

    Ok(MetricsOutcome {
        accepted_points,
        rejected_points,
        rejection_message,
    })
}

/// OTLP metrics projection into queued Scribe using the full source request
/// size for the admission charge.
pub async fn ingest_resource_metrics_to_scribe(
    catalog: &WyrdCatalog,
    scribe: &ScribeImpl,
    auth: &AuthContext,
    request: ExportMetricsServiceRequest,
) -> Result<MetricsOutcome, IngestError> {
    authorize_otlp(auth)?;
    let source_bytes = request.encoded_len();
    let MappedMetrics { records, rejected } = map_resource_metrics(&request.resource_metrics);
    let rejected_points = rejected.len() as i64;
    let rejection_message = rejected.first().map(|first| first.reason.clone());
    if records.is_empty() {
        return Ok(MetricsOutcome {
            accepted_points: 0,
            rejected_points,
            rejection_message,
        });
    }
    let accepted_points = records.len() as i64;
    let batch = metrics_to_record_batch(&records).map_err(IngestError::Decode)?;
    enqueue_scribe(
        catalog,
        scribe,
        auth,
        "vala.metrics.points",
        batch,
        source_bytes,
    )
    .await?;
    Ok(MetricsOutcome {
        accepted_points,
        rejected_points,
        rejection_message,
    })
}

/// Result of one accepted OTLP metrics export: committed vs. dropped data points.
#[derive(Debug)]
pub struct MetricsOutcome {
    /// Data points committed to `metrics.points`.
    pub accepted_points: i64,
    /// Data points dropped for a per-point reason (surfaced as `partial_success`).
    pub rejected_points: i64,
    /// First rejection reason, for the OTLP `error_message`.
    pub rejection_message: Option<String>,
}

impl MetricsOutcome {
    /// Render the OTLP `partial_success` field, or `None` on a full success.
    #[must_use]
    pub fn partial_success(&self) -> Option<ExportMetricsPartialSuccess> {
        if self.rejected_points == 0 {
            return None;
        }
        Some(ExportMetricsPartialSuccess {
            rejected_data_points: self.rejected_points,
            error_message: self
                .rejection_message
                .clone()
                .unwrap_or_else(|| "one or more data points were rejected".to_owned()),
        })
    }
}

#[wyrd_tonic::tonic::async_trait]
impl<R: PermissionResolver + 'static, I: IssuerConfigResolver + 'static> MetricsService
    for OtlpMetricsService<R, I>
{
    #[tracing::instrument(
        name = "otlp.metrics.export",
        skip_all,
        fields(tenant, accepted_points, rejected_points)
    )]
    async fn export(
        &self,
        request: Request<ExportMetricsServiceRequest>,
    ) -> Result<Response<ExportMetricsServiceResponse>, Status> {
        let auth = self
            .auth
            .authenticate(request.metadata())
            .await
            .map_err(Status::from)?;
        tracing::Span::current().record("tenant", tracing::field::display(auth.tenant));

        let payload = request.into_inner();
        let outcome = self
            .ingest_resource_metrics(&auth, payload)
            .await
            .map_err(map_export_error)?;

        tracing::Span::current().record("accepted_points", outcome.accepted_points);
        tracing::Span::current().record("rejected_points", outcome.rejected_points);

        Ok(Response::new(ExportMetricsServiceResponse {
            partial_success: outcome.partial_success(),
        }))
    }
}

// ─── logs ──────────────────────────────────────────────────────────────────────

/// OTLP/gRPC logs receiver over `vala-bifrost`'s group-commit writer.
///
/// Mirror of [`OtlpTraceService`] for the logs signal: authenticates, maps
/// `ExportLogsServiceRequest` to `logs.records` rows, and commits them through the
/// live coordinator. `logs.records` is a `SystemShared` multi-tenant physical
/// table (and `PayloadClass::Sensitive`), so the writer opens with
/// [`TableScope::SystemShared`] and the coordinator redacts `body`/`attributes` at
/// flush.
pub struct OtlpLogsService<R: PermissionResolver + 'static, I: IssuerConfigResolver + 'static> {
    catalog: Arc<WyrdCatalog>,
    auth: IngestAuthInterceptor<R, I>,
    scribe: Option<Arc<ScribeImpl>>,
}

impl<R: PermissionResolver + 'static, I: IssuerConfigResolver + 'static> OtlpLogsService<R, I> {
    /// Construct from the engine catalog and the shared ingest auth interceptor.
    #[must_use]
    pub fn new(catalog: Arc<WyrdCatalog>, auth: IngestAuthInterceptor<R, I>) -> Self {
        Self {
            catalog,
            auth,
            scribe: None,
        }
    }

    /// Construct the OTLP logs service with the queued Scribe seam.
    #[must_use]
    pub fn with_scribe(
        catalog: Arc<WyrdCatalog>,
        scribe: Arc<ScribeImpl>,
        auth: IngestAuthInterceptor<R, I>,
    ) -> Self {
        Self {
            catalog,
            auth,
            scribe: Some(scribe),
        }
    }

    /// Wrap into the generated OTLP server type for mounting on the shared tonic
    /// listener.
    #[must_use]
    pub fn into_server(self) -> LogsServiceServer<Self> {
        LogsServiceServer::new(self)
    }

    /// Decode → write core, reusable by the HTTP OTLP path.
    ///
    /// # Errors
    /// Returns an [`IngestError`] for auth/RBAC/engine failures (including
    /// [`IngestError::WriterBusy`] on backpressure). Per-record decode failures do
    /// not error here — they surface in [`LogsOutcome::rejected_records`].
    pub async fn ingest_resource_logs(
        &self,
        auth: &AuthContext,
        request: ExportLogsServiceRequest,
    ) -> Result<LogsOutcome, IngestError> {
        match &self.scribe {
            Some(scribe) => {
                ingest_resource_logs_to_scribe(&self.catalog, scribe, auth, request).await
            }
            None => ingest_resource_logs(&self.catalog, auth, request).await,
        }
    }
}

/// Transport-agnostic OTLP logs decode→write core.
///
/// Authorizes the record-write capability, flattens the log records, writes the
/// accepted rows through the live coordinator on `logs.records` as one commit
/// unit, and reports the per-record rejection count. Both the OTLP/gRPC collector
/// and the OTLP/HTTP handler call this.
///
/// # Errors
/// Returns an [`IngestError`] for auth/RBAC/engine failures (including
/// [`IngestError::WriterBusy`] on backpressure). Per-record decode failures do not
/// error here — they surface in [`LogsOutcome::rejected_records`].
pub async fn ingest_resource_logs(
    catalog: &WyrdCatalog,
    auth: &AuthContext,
    request: ExportLogsServiceRequest,
) -> Result<LogsOutcome, IngestError> {
    RbacCheck
        .check(&auth.principal, &Permission::bifrost_record_write())
        .into_result()
        .map_err(IngestError::from_rbac)?;

    let MappedLogs { records, rejected } = map_resource_logs(&request.resource_logs);
    let rejected_records = rejected.len() as i64;
    let rejection_message = rejected.first().map(|first| first.reason.clone());

    if records.is_empty() {
        return Ok(LogsOutcome {
            accepted_records: 0,
            rejected_records,
            rejection_message,
        });
    }

    let batch = logs_to_record_batch(&records).map_err(IngestError::Decode)?;
    let accepted_records = records.len() as i64;

    let writer = catalog
        .typed_writer::<RecordsTable>(TableScope::SystemShared, auth.tenant)
        .await
        .map_err(IngestError::from_engine)?;

    let ctx = BifrostWriteContext {
        batch_id: *uuid::Uuid::now_v7().as_bytes(),
        origin: OTLP_ORIGIN.to_owned(),
        actor: auth.principal.id.to_string(),
        request_id: auth.request_id.clone(),
        card_ref: auth.principal.card_ref().cloned(),
    };

    writer
        .commit_one(auth.tenant, vec![batch], ctx)
        .await
        .map_err(IngestError::from_engine)?;

    Ok(LogsOutcome {
        accepted_records,
        rejected_records,
        rejection_message,
    })
}

/// OTLP logs projection into queued Scribe using the full source request size
/// for the admission charge.
pub async fn ingest_resource_logs_to_scribe(
    catalog: &WyrdCatalog,
    scribe: &ScribeImpl,
    auth: &AuthContext,
    request: ExportLogsServiceRequest,
) -> Result<LogsOutcome, IngestError> {
    authorize_otlp(auth)?;
    let source_bytes = request.encoded_len();
    let MappedLogs { records, rejected } = map_resource_logs(&request.resource_logs);
    let rejected_records = rejected.len() as i64;
    let rejection_message = rejected.first().map(|first| first.reason.clone());
    if records.is_empty() {
        return Ok(LogsOutcome {
            accepted_records: 0,
            rejected_records,
            rejection_message,
        });
    }
    let accepted_records = records.len() as i64;
    let batch = logs_to_record_batch(&records).map_err(IngestError::Decode)?;
    enqueue_scribe(
        catalog,
        scribe,
        auth,
        "vala.logs.records",
        batch,
        source_bytes,
    )
    .await?;
    Ok(LogsOutcome {
        accepted_records,
        rejected_records,
        rejection_message,
    })
}

fn authorize_otlp(auth: &AuthContext) -> Result<(), IngestError> {
    RbacCheck
        .check(&auth.principal, &Permission::bifrost_record_write())
        .into_result()
        .map_err(IngestError::from_rbac)
}

async fn enqueue_scribe(
    catalog: &WyrdCatalog,
    scribe: &ScribeImpl,
    auth: &AuthContext,
    table: &str,
    rows: arrow::record_batch::RecordBatch,
    measured_wire_bytes: usize,
) -> Result<(), IngestError> {
    let table = ReduxTableRef::parse_fqn(table)
        .ok_or_else(|| IngestError::Internal(format!("invalid OTLP Scribe table: {table}")))?;
    catalog
        .table_schema_fingerprint(
            BifrostNamespace::from_wire(table.namespace.as_str())
                .ok_or_else(|| IngestError::Internal("invalid OTLP namespace".to_owned()))?,
            &table.name,
            auth.tenant,
        )
        .await
        .map_err(IngestError::from_engine)?;
    let batch_id = Uuid::now_v7();
    let schema_fingerprint = SchemaFingerprint::from_arrow_schema(rows.schema().as_ref());
    scribe
        .append(ScribeAppend {
            principal: auth.principal.clone(),
            table,
            rows,
            schema_fingerprint,
            request_id: auth.request_id.clone(),
            batch_id,
            measured_wire_bytes,
        })
        .await
        .map_err(IngestError::from_scribe)
}

/// Result of one accepted OTLP logs export: committed vs. dropped records.
#[derive(Debug)]
pub struct LogsOutcome {
    /// Records committed to `logs.records`.
    pub accepted_records: i64,
    /// Records dropped for a per-record reason (surfaced as `partial_success`).
    pub rejected_records: i64,
    /// First rejection reason, for the OTLP `error_message`.
    pub rejection_message: Option<String>,
}

impl LogsOutcome {
    /// Render the OTLP `partial_success` field, or `None` on a full success.
    #[must_use]
    pub fn partial_success(&self) -> Option<ExportLogsPartialSuccess> {
        if self.rejected_records == 0 {
            return None;
        }
        Some(ExportLogsPartialSuccess {
            rejected_log_records: self.rejected_records,
            error_message: self
                .rejection_message
                .clone()
                .unwrap_or_else(|| "one or more log records were rejected".to_owned()),
        })
    }
}

#[wyrd_tonic::tonic::async_trait]
impl<R: PermissionResolver + 'static, I: IssuerConfigResolver + 'static> LogsService
    for OtlpLogsService<R, I>
{
    #[tracing::instrument(
        name = "otlp.logs.export",
        skip_all,
        fields(tenant, accepted_records, rejected_records)
    )]
    async fn export(
        &self,
        request: Request<ExportLogsServiceRequest>,
    ) -> Result<Response<ExportLogsServiceResponse>, Status> {
        let auth = self
            .auth
            .authenticate(request.metadata())
            .await
            .map_err(Status::from)?;
        tracing::Span::current().record("tenant", tracing::field::display(auth.tenant));

        let payload = request.into_inner();
        let outcome = self
            .ingest_resource_logs(&auth, payload)
            .await
            .map_err(map_export_error)?;

        tracing::Span::current().record("accepted_records", outcome.accepted_records);
        tracing::Span::current().record("rejected_records", outcome.rejected_records);

        Ok(Response::new(ExportLogsServiceResponse {
            partial_success: outcome.partial_success(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::map_export_error;
    use crate::error::IngestError;
    use wyrd_tonic::tonic::Code;

    // Backpressure is a coordinator-local, cross-boundary-state-free condition
    // (channel-full). Forcing genuine channel saturation end-to-end is flaky and
    // materially harder than asserting the load-bearing mapping directly, so the
    // `IngestBusy → UNAVAILABLE` contract is pinned here per the testing doctrine.
    #[test]
    fn ingest_busy_maps_to_unavailable_not_resource_exhausted() {
        let status = map_export_error(IngestError::WriterBusy);
        assert_eq!(
            status.code(),
            Code::Unavailable,
            "OTLP backpressure must be retryable UNAVAILABLE, not RESOURCE_EXHAUSTED"
        );
    }

    #[test]
    fn other_errors_keep_their_ingest_status() {
        let status = map_export_error(IngestError::TableNotFound {
            table: "vala.traces.spans".to_owned(),
        });
        assert_eq!(status.code(), Code::NotFound);
    }
}
