//! Bifrost Gate — the server-independent auth, admission, and dispatch boundary.

pub mod auth;
pub mod collector;
pub mod error;
pub mod limits;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use tracing::Instrument;
use wyrd_auth_oidc::IssuerConfigResolver;
use wyrd_auth_verify::PermissionResolver;
use wyrd_runtime::PermissionCheck;
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
use wyrd_tonic::tonic::{Request, Response, Status};
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
pub use crate::gate::limits::IngestLimits;
use crate::namespaces::BifrostNamespace;
use crate::oracle::{
    AuthorizedQueryContext, Oracle, OracleQueryStream, QueryOptions, QueryStreamLifecycle,
};
use crate::schema::fingerprint::SchemaFingerprint;
use wyrd_spec::vala::api::BifrostQueryRequest;
use wyrd_spec::vala::error::BifrostError;

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

fn record_gate_event(event: &'static str) {
    metrics::counter!("bifrost_gate_events_total", "stage" => event).increment(1);
}

fn record_gate_rows(accepted: i64, rejected: i64) {
    metrics::counter!("bifrost_gate_rows_total", "status" => "accepted")
        .increment(u64::try_from(accepted).unwrap_or(0));
    metrics::counter!("bifrost_gate_rows_total", "status" => "rejected")
        .increment(u64::try_from(rejected).unwrap_or(0));
}

/// Record the bounded-cardinality Gate request families used by D24.
fn record_gate_request(operation: &'static str, outcome: &'static str, elapsed: Duration) {
    metrics::counter!("bifrost_gate_requests_total", "operation" => operation, "outcome" => outcome)
        .increment(1);
    metrics::histogram!(
        "bifrost_gate_request_duration_seconds",
        "operation" => operation,
        "outcome" => outcome
    )
    .record(elapsed.as_secs_f64());
}

/// Record one typed Gate rejection without exposing request identity.
fn record_gate_rejection(operation: &'static str, reason: &'static str) {
    metrics::counter!("bifrost_gate_rejections_total", "operation" => operation, "reason" => reason)
        .increment(1);
}

/// Owns exactly one terminal Gate request metric across return or cancellation.
struct GateRequestLifecycle {
    /// Closed D24 operation label for this request.
    operation: &'static str,
    /// Monotonic request start used by the duration owner.
    started: std::time::Instant,
    /// Whether a normal return already emitted the terminal metric.
    completed: bool,
    /// Active-request gauge drained on every terminal path.
    active: metrics::Gauge,
}

impl GateRequestLifecycle {
    /// Begin one request lifecycle at the transport entry point.
    fn begin(operation: &'static str) -> Self {
        let active = metrics::gauge!("bifrost_gate_active_requests", "operation" => operation);
        active.increment(1.0);
        Self {
            operation,
            started: std::time::Instant::now(),
            completed: false,
            active,
        }
    }

    /// Emit the normal terminal outcome and disarm cancellation-on-drop.
    fn complete(mut self, outcome: &'static str) {
        record_gate_request(self.operation, outcome, self.started.elapsed());
        self.completed = true;
    }
}

impl Drop for GateRequestLifecycle {
    /// Emit cancellation when the transport drops the in-flight service future.
    fn drop(&mut self) {
        if !self.completed {
            record_gate_request(self.operation, "cancelled", self.started.elapsed());
        }
        self.active.decrement(1.0);
    }
}

/// Describe and initialize the D24 Gate families at process boot.
fn initialize_gate_metrics() {
    metrics::describe_counter!(
        "bifrost_gate_requests_total",
        "Total Bifrost Gate requests by operation and terminal outcome."
    );
    metrics::describe_histogram!(
        "bifrost_gate_request_duration_seconds",
        metrics::Unit::Seconds,
        "Bifrost Gate request duration by operation and terminal outcome."
    );
    metrics::describe_counter!(
        "bifrost_gate_query_streams_total",
        "Total Bifrost Gate query streams by terminal outcome."
    );
    metrics::describe_histogram!(
        "bifrost_gate_query_stream_duration_seconds",
        metrics::Unit::Seconds,
        "Bifrost Gate query-stream duration by terminal outcome."
    );
    metrics::describe_gauge!(
        "bifrost_gate_active_streams",
        "Current authorized and admitted Bifrost Gate query streams."
    );
    metrics::describe_gauge!(
        "bifrost_gate_active_requests",
        "Current Bifrost Gate requests by closed operation."
    );
    metrics::describe_counter!(
        "bifrost_gate_rejections_total",
        "Total rejected Bifrost Gate requests by operation and closed reason."
    );
    for operation in ["write", "query"] {
        metrics::gauge!("bifrost_gate_active_requests", "operation" => operation).set(0.0);
        for outcome in ["success", "rejected", "failed", "cancelled"] {
            metrics::counter!(
                "bifrost_gate_requests_total",
                "operation" => operation,
                "outcome" => outcome
            )
            .increment(0);
        }
    }
    for outcome in ["success", "rejected", "failed", "cancelled"] {
        metrics::counter!("bifrost_gate_query_streams_total", "outcome" => outcome).increment(0);
    }
    metrics::gauge!("bifrost_gate_active_streams", "operation" => "query").set(0.0);
}

/// The concrete Bifrost write boundary.
///
/// Gate owns authentication, request bounds, and transport response ordering.
/// The Scribe dependency is mandatory at construction.
#[derive(Clone)]
pub struct Gate<
    C: Catalog + 'static,
    R: PermissionResolver + 'static,
    I: IssuerConfigResolver + 'static,
> {
    catalog: Arc<C>,
    scribe: Option<Arc<dyn Scribe>>,
    /// Optional retained Oracle used by the server's stable local query dispatch.
    oracle: Option<Arc<Oracle>>,
    limits: IngestLimits,
    projection: Arc<dyn ProjectionExecutor>,
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
        initialize_gate_metrics();
        Self {
            catalog,
            scribe: Some(scribe),
            oracle: None,
            limits,
            projection: Arc::new(InlineProjectionExecutor),
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

    /// Constructs a stable Gate whose ingest role is intentionally unavailable.
    ///
    /// Authentication remains active on this Gate. Authorized ingest reaches the
    /// closed role check and receives the stable unavailable response without a
    /// WAL allocation.
    #[must_use]
    pub fn without_scribe(
        catalog: Arc<C>,
        auth: IngestAuthInterceptor<R, I>,
        limits: IngestLimits,
    ) -> Self {
        initialize_gate_metrics();
        Self {
            catalog,
            scribe: None,
            oracle: None,
            limits,
            projection: Arc::new(InlineProjectionExecutor),
            auth,
            closed: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Attaches the independently optional retained Oracle query owner.
    #[must_use]
    pub fn with_oracle(mut self, oracle: Arc<Oracle>) -> Self {
        self.oracle = Some(oracle);
        self
    }

    /// Stop accepting new ingest requests.
    pub fn close(&self) {
        if !self.closed.swap(true, Ordering::AcqRel) {
            record_gate_event("close");
            tracing::info!("bifrost gate closed");
        }
    }

    /// Reports whether lifecycle shutdown has closed new Gate work.
    ///
    /// This test-tier probe observes the same atomic checked by ingest request
    /// admission without exposing a second mutable shutdown path.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn is_closed_for_test(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    fn ensure_open(&self) -> Result<(), IngestError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(IngestError::IngressClosed);
        }
        if !self.scribe.as_ref().is_some_and(|scribe| scribe.is_ready()) {
            return Err(IngestError::IngressClosed);
        }
        Ok(())
    }

    async fn authenticate(&self, metadata: &MetadataMap) -> Result<AuthContext, IngestError> {
        let started = std::time::Instant::now();
        record_gate_event("auth_attempt");
        match self.auth.authenticate(metadata).await {
            Ok(auth) => {
                metrics::histogram!("bifrost_gate_resolution_seconds", "stage" => "auth")
                    .record(started.elapsed().as_secs_f64());
                self.ensure_open()?;
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

    /// Dispatches authorized public SQL only to a ready local Oracle.
    ///
    /// # Errors
    ///
    /// Returns role unavailable before planning when this Gate has no ready
    /// Oracle, otherwise returns the retained Oracle's stable query errors.
    #[tracing::instrument(
        name = "bifrost.gate.role_dispatch",
        skip_all,
        fields(required_role = "oracle", operation = "query_sql")
    )]
    pub async fn query_sql(
        &self,
        context: AuthorizedQueryContext,
        request: BifrostQueryRequest,
    ) -> Result<OracleQueryStream, BifrostError> {
        let request_lifecycle = GateRequestLifecycle::begin("query");
        let Some(oracle) = &self.oracle else {
            metrics::counter!(
                "bifrost_gate_role_unavailable_total",
                "required_role" => "oracle",
                "reason" => "not_configured"
            )
            .increment(1);
            record_gate_rejection("query", "role_unavailable");
            request_lifecycle.complete("rejected");
            return Err(BifrostError::OracleRoleUnavailable);
        };
        if !oracle.is_ready() {
            metrics::counter!(
                "bifrost_gate_role_unavailable_total",
                "required_role" => "oracle",
                "reason" => "not_ready"
            )
            .increment(1);
            record_gate_rejection("query", "role_unavailable");
            request_lifecycle.complete("rejected");
            return Err(BifrostError::OracleRoleUnavailable);
        }
        metrics::gauge!("bifrost_gate_active_streams", "operation" => "query").increment(1.0);
        let lifecycle = Arc::new(QueryStreamLifecycle::new(|outcome, elapsed| {
            metrics::counter!("bifrost_gate_query_streams_total", "outcome" => outcome)
                .increment(1);
            metrics::histogram!("bifrost_gate_query_stream_duration_seconds", "outcome" => outcome)
                .record(elapsed.as_secs_f64());
            metrics::gauge!("bifrost_gate_active_streams", "operation" => "query").decrement(1.0);
        }));
        let result = oracle
            .query_sql_with_gate_lifecycle(context, request, Some(Arc::clone(&lifecycle)))
            .instrument(tracing::info_span!(
                "bifrost.gate.query",
                operation = "query"
            ))
            .await;
        if matches!(&result, Err(BifrostError::QueryAdmissionRejected)) {
            record_gate_rejection("query", "oracle_admission");
            lifecycle.finish("rejected");
            request_lifecycle.complete("rejected");
        } else if result.is_err() {
            lifecycle.finish("failed");
            request_lifecycle.complete("failed");
        } else {
            request_lifecycle.complete("success");
        }
        result
    }

    /// Dispatches an authorized typed logical plan only to a ready local Oracle.
    ///
    /// # Errors
    ///
    /// Returns role unavailable before admission when this Gate has no ready
    /// Oracle, otherwise returns the retained Oracle's stable query errors.
    #[tracing::instrument(
        name = "bifrost.gate.role_dispatch",
        skip_all,
        fields(required_role = "oracle", operation = "query_plan")
    )]
    pub async fn query_plan(
        &self,
        context: AuthorizedQueryContext,
        plan: datafusion::logical_expr::LogicalPlan,
        options: QueryOptions,
    ) -> Result<OracleQueryStream, BifrostError> {
        let Some(oracle) = &self.oracle else {
            metrics::counter!(
                "bifrost_gate_role_unavailable_total",
                "required_role" => "oracle",
                "reason" => "not_configured"
            )
            .increment(1);
            return Err(BifrostError::OracleRoleUnavailable);
        };
        if !oracle.is_ready() {
            metrics::counter!(
                "bifrost_gate_role_unavailable_total",
                "required_role" => "oracle",
                "reason" => "not_ready"
            )
            .increment(1);
            return Err(BifrostError::OracleRoleUnavailable);
        }
        oracle.query_plan(context, plan, options).await
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
        if namespace == BifrostNamespace::Audit {
            return Err(IngestError::ReservedBuiltinWriteDenied {
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
        let scribe = self.scribe.as_ref().ok_or(IngestError::IngressClosed)?;
        scribe
            .ingest_frame(ScribeIngressFrame {
                principal: auth.principal.clone(),
                binding,
                expected_schema_fingerprint: registered_fingerprint,
                request_id: auth.request_id.clone(),
                batch_id: uuid::Uuid::now_v7(),
                audit_event,
                measured_wire_bytes,
                payload: IngressPayload::ProjectedArrow(vec![rows]),
            })
            .await
            .map_err(|error| {
                record_gate_event("scribe_failure");
                IngestError::from_scribe(error)
            })
            .map(|_| ())
    }

    /// Resolve, authorize, stamp, and dispatch one native batch to Scribe.
    ///
    /// Gate owns permission, catalog, binding, audit, and batch dispatch
    /// decisions; the server only supplies the catalog adapter and mounts this
    /// service.
    #[tracing::instrument(
        skip_all,
        fields(tenant = %auth.tenant, table = %frame.table, request_id = %auth.request_id)
    )]
    async fn dispatch_native_frame(
        &self,
        limits: &IngestLimits,
        auth: &AuthContext,
        frame: InsertBatchRequest,
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
            IngestError::RequestValidation("wyrd_batch_id must be exactly 16 bytes".to_owned())
        })?;
        let batch_id = uuid::Uuid::from_bytes(batch_id);
        if batch_id.get_version() != Some(uuid::Version::SortRand) {
            return Err(IngestError::RequestValidation(
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
        if namespace == BifrostNamespace::Audit {
            return Err(IngestError::ReservedBuiltinWriteDenied { table: frame.table });
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
            operation: "bifrost.ingest_batch".to_owned(),
            resource: table.fqn(),
            card_ref: auth.principal.card_ref().cloned(),
            principal_id: auth.principal.id,
            principal_kind: auth.principal.kind.tag(),
            auth_method: wyrd_spec::vala::api::AuthMethod::Jwt,
            permission: "bifrost:record:write".to_owned(),
            decision: wyrd_spec::vala::api::AuditDecision::Allow,
            result: wyrd_spec::vala::api::AuditResult::Success,
            payload_summary: "one bounded native batch".to_owned(),
            detail: None,
        };
        let scribe = self.scribe.as_ref().ok_or(IngestError::IngressClosed)?;
        let admission = scribe
            .ingest_frame(ScribeIngressFrame {
                principal: auth.principal.clone(),
                binding,
                expected_schema_fingerprint: registered_fingerprint,
                request_id: auth.request_id.clone(),
                batch_id,
                audit_event,
                measured_wire_bytes: frame.arrow_ipc.len(),
                payload: IngressPayload::ArrowIpc(frame.arrow_ipc),
            })
            .await
            .map_err(|error| {
                record_gate_event("scribe_failure");
                metrics::counter!("bifrost_gate_frames_total", "status" => "rejected").increment(1);
                IngestError::from_scribe(error)
            })?;
        metrics::counter!("bifrost_gate_frames_total", "status" => "accepted").increment(1);
        record_gate_rows(
            i64::try_from(admission.rows_accepted).unwrap_or(i64::MAX),
            0,
        );
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
    BifrostNamespace::split_fqn(fqn)
        .ok_or_else(|| IngestError::RequestValidation(format!("unrecognized table fqn: {fqn}")))
}

fn map_otlp_error(error: IngestError) -> Status {
    error.into_status()
}

#[wyrd_tonic::tonic::async_trait]
impl<C: Catalog + 'static, R: PermissionResolver + 'static, I: IssuerConfigResolver + 'static>
    BifrostIngestService for Gate<C, R, I>
{
    #[tracing::instrument(name = "bifrost.gate.write", skip_all, fields(operation = "write"))]
    async fn insert_batch(
        &self,
        request: Request<InsertBatchRequest>,
    ) -> Result<Response<InsertBatchResponse>, Status> {
        let lifecycle = GateRequestLifecycle::begin("write");
        let result: Result<Response<InsertBatchResponse>, Status> = async {
            let auth = self
                .authenticate(request.metadata())
                .await
                .inspect_err(|_| record_gate_rejection("write", "auth"))
                .map_err(Status::from)?;
            let frame = request.into_inner();
            validate_batch(&frame, &self.limits)
                .inspect_err(|error| {
                    record_gate_rejection(
                        "write",
                        match error {
                            IngestError::PayloadTooLarge { .. } => "payload_limit",
                            _ => "validation",
                        },
                    );
                })
                .map_err(Status::from)?;
            self.dispatch_native_frame(&self.limits, &auth, frame.clone())
                .await
                .inspect_err(|error| {
                    record_gate_event("native_rejection");
                    let reason = match error {
                        IngestError::RbacDenied { .. }
                        | IngestError::ReservedBuiltinWriteDenied { .. }
                        | IngestError::CardScopeDenied { .. }
                        | IngestError::CardUnresolved { .. } => "permission",
                        IngestError::PayloadTooLarge { .. } => "payload_limit",
                        IngestError::RequestValidation(_)
                        | IngestError::Decode(_)
                        | IngestError::EventTimeOutOfRange { .. }
                        | IngestError::TooManyRows { .. } => "validation",
                        IngestError::TableNotFound { .. } | IngestError::SchemaMismatch { .. } => {
                            "catalog"
                        }
                        IngestError::IngressClosed => "role_unavailable",
                        IngestError::IngestBusy { .. } | IngestError::WalDiskFull => {
                            "scribe_admission"
                        }
                        IngestError::Unauthenticated(_) | IngestError::PrincipalUnresolved => {
                            "auth"
                        }
                        IngestError::Internal(_) => return,
                    };
                    record_gate_rejection("write", reason);
                })
                .map_err(Status::from)?;
            let mut response = Response::new(InsertBatchResponse {
                wyrd_batch_id: frame.wyrd_batch_id,
            });
            if let Ok(value) = auth.request_id.as_str().parse() {
                response
                    .metadata_mut()
                    .insert(WYRD_REQUEST_ID_METADATA, value);
            }
            Ok(response)
        }
        .await;
        // Scribe admission pressure is projected as ResourceExhausted (429)
        // by the canonical ingest error mapping. Count that response as one
        // rejected write request at this outer lifecycle owner, immediately
        // before returning to the transport. Keeping the accounting here
        // avoids a second increment in the Gate→Scribe seam.
        let outcome = ingest_request_outcome(&result);
        lifecycle.complete(outcome);
        result
    }
}

/// Classify one completed native write request for the bounded D24 metric.
///
/// The transport status is already the canonical projection of the Gate
/// taxonomy. Resource exhaustion includes Scribe's explicit admission/busy
/// response, so it is a rejection rather than an internal failure.
fn ingest_request_outcome(result: &Result<Response<InsertBatchResponse>, Status>) -> &'static str {
    match result {
        Ok(_) => "success",
        Err(status) if status.code() == wyrd_tonic::tonic::Code::Cancelled => "cancelled",
        Err(status)
            if matches!(
                status.code(),
                wyrd_tonic::tonic::Code::PermissionDenied
                    | wyrd_tonic::tonic::Code::ResourceExhausted
            ) =>
        {
            "rejected"
        }
        Err(_) => "failed",
    }
}

fn validate_batch(frame: &InsertBatchRequest, limits: &IngestLimits) -> Result<(), IngestError> {
    if frame.table.is_empty() || frame.wyrd_batch_id.len() != 16 {
        return Err(IngestError::RequestValidation(
            "table and exactly 16-byte wyrd_batch_id are required on every frame".to_owned(),
        ));
    }
    let batch_id = uuid::Uuid::from_bytes(
        frame
            .wyrd_batch_id
            .as_ref()
            .try_into()
            .map_err(|_| IngestError::RequestValidation("invalid batch id".to_owned()))?,
    );
    if batch_id.get_version() != Some(uuid::Version::SortRand) {
        return Err(IngestError::RequestValidation(
            "wyrd_batch_id must be UUIDv7".to_owned(),
        ));
    }
    if frame.arrow_ipc.len() > limits.max_frame_bytes {
        return Err(IngestError::PayloadTooLarge {
            bytes: frame.arrow_ipc.len() as u64,
            limit: limits.max_frame_bytes as u64,
        });
    }
    Ok(())
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
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::collector::{IngestOutcome, ProjectedExport, ProjectionExecutor};
    use super::error::CatalogError;
    use super::limits::IngestLimits;
    use super::{AuthContext, Catalog, Gate, IngestError};
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

    /// One exact Gate family description captured during owner initialization.
    type GateMetricDescription = (String, String, Option<metrics::Unit>, String);

    /// Recorder that observes boot descriptions while retaining normal metric behavior.
    #[derive(Debug, Default)]
    struct GateBootRecorder {
        /// Standard recorder used to verify family registration and initial values.
        metrics: wyrd_bench::BenchmarkRecorder,
        /// Exact kind, family, unit, and help text emitted during initialization.
        descriptions: Mutex<Vec<GateMetricDescription>>,
    }

    impl metrics::Recorder for GateBootRecorder {
        /// Retain one counter description exactly as emitted at boot.
        fn describe_counter(
            &self,
            key: metrics::KeyName,
            unit: Option<metrics::Unit>,
            description: metrics::SharedString,
        ) {
            self.descriptions
                .lock()
                .expect("description registry")
                .push((
                    "counter".to_owned(),
                    key.as_str().to_owned(),
                    unit,
                    description.to_string(),
                ));
        }

        /// Retain one gauge description exactly as emitted at boot.
        fn describe_gauge(
            &self,
            key: metrics::KeyName,
            unit: Option<metrics::Unit>,
            description: metrics::SharedString,
        ) {
            self.descriptions
                .lock()
                .expect("description registry")
                .push((
                    "gauge".to_owned(),
                    key.as_str().to_owned(),
                    unit,
                    description.to_string(),
                ));
        }

        /// Retain one histogram description exactly as emitted at boot.
        fn describe_histogram(
            &self,
            key: metrics::KeyName,
            unit: Option<metrics::Unit>,
            description: metrics::SharedString,
        ) {
            self.descriptions
                .lock()
                .expect("description registry")
                .push((
                    "histogram".to_owned(),
                    key.as_str().to_owned(),
                    unit,
                    description.to_string(),
                ));
        }

        /// Delegate counter registration to the behavioral recorder.
        fn register_counter(
            &self,
            key: &metrics::Key,
            metadata: &metrics::Metadata<'_>,
        ) -> metrics::Counter {
            metrics::Recorder::register_counter(&self.metrics, key, metadata)
        }

        /// Delegate gauge registration to the behavioral recorder.
        fn register_gauge(
            &self,
            key: &metrics::Key,
            metadata: &metrics::Metadata<'_>,
        ) -> metrics::Gauge {
            metrics::Recorder::register_gauge(&self.metrics, key, metadata)
        }

        /// Delegate histogram registration to the behavioral recorder.
        fn register_histogram(
            &self,
            key: &metrics::Key,
            metadata: &metrics::Metadata<'_>,
        ) -> metrics::Histogram {
            metrics::Recorder::register_histogram(&self.metrics, key, metadata)
        }
    }

    /// D24 describes every Gate family at boot and registers the active gauge.
    ///
    /// # Panics
    ///
    /// Panics when a required description, canonical help text, terminal label,
    /// or the initialized query gauge is absent.
    #[test]
    fn d24_gate_metric_families_are_described_and_registered_at_boot() {
        let recorder = GateBootRecorder::default();
        metrics::with_local_recorder(&recorder, super::initialize_gate_metrics);
        assert_eq!(
            *recorder.descriptions.lock().expect("description registry"),
            [
                (
                    "counter".to_owned(),
                    "bifrost_gate_requests_total".to_owned(),
                    None,
                    "Total Bifrost Gate requests by operation and terminal outcome.".to_owned(),
                ),
                (
                    "histogram".to_owned(),
                    "bifrost_gate_request_duration_seconds".to_owned(),
                    Some(metrics::Unit::Seconds),
                    "Bifrost Gate request duration by operation and terminal outcome.".to_owned(),
                ),
                (
                    "counter".to_owned(),
                    "bifrost_gate_query_streams_total".to_owned(),
                    None,
                    "Total Bifrost Gate query streams by terminal outcome.".to_owned(),
                ),
                (
                    "histogram".to_owned(),
                    "bifrost_gate_query_stream_duration_seconds".to_owned(),
                    Some(metrics::Unit::Seconds),
                    "Bifrost Gate query-stream duration by terminal outcome.".to_owned(),
                ),
                (
                    "gauge".to_owned(),
                    "bifrost_gate_active_streams".to_owned(),
                    None,
                    "Current authorized and admitted Bifrost Gate query streams.".to_owned(),
                ),
                (
                    "gauge".to_owned(),
                    "bifrost_gate_active_requests".to_owned(),
                    None,
                    "Current Bifrost Gate requests by closed operation.".to_owned(),
                ),
                (
                    "counter".to_owned(),
                    "bifrost_gate_rejections_total".to_owned(),
                    None,
                    "Total rejected Bifrost Gate requests by operation and closed reason."
                        .to_owned(),
                ),
            ]
        );
        assert_eq!(
            recorder
                .metrics
                .snapshot()
                .gauges
                .get("bifrost_gate_active_streams{operation=\"query\"}"),
            Some(&0.0)
        );
        let snapshot = recorder.metrics.snapshot();
        for operation in ["write", "query"] {
            for outcome in ["success", "rejected", "failed", "cancelled"] {
                assert_eq!(
                    snapshot.counters.get(&format!(
                        "bifrost_gate_requests_total{{operation=\"{operation}\",outcome=\"{outcome}\"}}"
                    )),
                    Some(&0)
                );
            }
        }
        for outcome in ["success", "rejected", "failed", "cancelled"] {
            assert_eq!(
                snapshot.counters.get(&format!(
                    "bifrost_gate_query_streams_total{{outcome=\"{outcome}\"}}"
                )),
                Some(&0)
            );
        }
    }

    /// Admission pressure is counted once as a rejected write request.
    #[test]
    fn admission_status_maps_to_one_rejected_write_outcome() {
        let status = wyrd_tonic::tonic::Status::new(
            wyrd_tonic::tonic::Code::ResourceExhausted,
            "ingest writer busy",
        );
        let result: Result<
            wyrd_tonic::tonic::Response<wyrd_tonic::wyrd::v1::InsertBatchResponse>,
            wyrd_tonic::tonic::Status,
        > = Err(status);
        assert_eq!(super::ingest_request_outcome(&result), "rejected");
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
            Ok(crate::contracts::FrameAdmission {
                batch_id: uuid::Uuid::now_v7(),
                rows_accepted: 0,
            })
        }
    }

    struct NotReadyScribe;

    #[async_trait]
    impl crate::contracts::Scribe for NotReadyScribe {
        fn is_ready(&self) -> bool {
            false
        }

        async fn ingest_frame(
            &self,
            _frame: crate::contracts::ScribeIngressFrame,
        ) -> Result<crate::contracts::FrameAdmission, crate::contracts::ScribeError> {
            panic!("a not-ready Scribe must be rejected by Gate first");
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
            Ok(crate::contracts::FrameAdmission {
                batch_id: uuid::Uuid::now_v7(),
                rows_accepted: 1,
            })
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
        let _gate = Gate::<TestCatalog, TestPermissionResolver, TestIssuerResolver>::with_scribe(
            Arc::new(TestCatalog),
            Arc::new(TestScribe),
            crate::gate::auth::ingest_auth_interceptor(Arc::new(verifier)),
            IngestLimits::default(),
        );
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
        assert!(matches!(error, IngestError::IngressClosed));
        assert_eq!(projection_calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn gate_rejects_ingest_when_scribe_recovery_is_incomplete() {
        let projection_calls = Arc::new(AtomicUsize::new(0));
        let gate = Gate::with_scribe_and_projection(
            Arc::new(TestCatalog),
            Arc::new(NotReadyScribe),
            test_interceptor(),
            IngestLimits::default(),
            Arc::new(FailingProjection {
                calls: Arc::clone(&projection_calls),
            }),
        );

        let error = gate
            .ingest_resource_spans(&auth_context(true), ExportTraceServiceRequest::default())
            .await
            .expect_err("Gate must fail closed while Scribe recovery is incomplete");
        assert!(matches!(error, IngestError::IngressClosed));
        assert_eq!(projection_calls.load(Ordering::Relaxed), 0);
    }

    /// Query request lifecycles emit one exact terminal and drain active work.
    #[test]
    fn query_request_lifecycle_reconciles_exact_terminal_outcomes() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        metrics::with_local_recorder(&recorder, || {
            for outcome in ["success", "rejected", "failed"] {
                let lifecycle = super::GateRequestLifecycle::begin("query");
                while lifecycle.started.elapsed().as_micros() == 0 {
                    std::hint::spin_loop();
                }
                lifecycle.complete(outcome);
            }
            let cancelled = super::GateRequestLifecycle::begin("query");
            while cancelled.started.elapsed().as_micros() == 0 {
                std::hint::spin_loop();
            }
            drop(cancelled);
        });
        let snapshot = recorder.snapshot();
        for outcome in ["success", "rejected", "failed", "cancelled"] {
            assert_eq!(
                snapshot.counters.get(&format!(
                    "bifrost_gate_requests_total{{operation=\"query\",outcome=\"{outcome}\"}}"
                )),
                Some(&1)
            );
            assert_eq!(
                snapshot
                    .histograms
                    .get(&format!(
                        "bifrost_gate_request_duration_seconds{{operation=\"query\",outcome=\"{outcome}\"}}"
                    ))
                    .map(|histogram| histogram.count),
                Some(1)
            );
        }
        assert_eq!(
            snapshot
                .gauges
                .get("bifrost_gate_active_requests{operation=\"query\"}"),
            Some(&0.0)
        );
    }
}
