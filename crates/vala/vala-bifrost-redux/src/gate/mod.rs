//! Bifrost Gate — the server-independent auth, transport-limit, and routing boundary.

pub mod auth;
pub mod error;
pub mod limits;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tracing::Instrument;
use wyrd_auth_oidc::IssuerConfigResolver;
use wyrd_auth_verify::PermissionResolver;
use wyrd_runtime::PermissionCheck;
use wyrd_tonic::otlp::logs_service::ExportLogsServiceRequest;
use wyrd_tonic::otlp::metrics_service::ExportMetricsServiceRequest;
use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;
use wyrd_tonic::tonic::metadata::MetadataMap;
use wyrd_tonic::tonic::{Request, Response, Status};
use wyrd_tonic::wyrd::v1::bifrost_ingest_service_server::{
    BifrostIngestService, BifrostIngestServiceServer,
};
use wyrd_tonic::wyrd::v1::{InsertBatchRequest, InsertBatchResponse};

use crate::catalog::TableRef;
use crate::contracts::{
    CanonicalIngress, DecodedOtlp, IngressPayload, OracleQueryDispatch, OtlpDecodeOwner, Scribe,
    ScribeIngressFrame,
};
pub use crate::gate::auth::{AuthContext, IngestAuthInterceptor, WYRD_REQUEST_ID_METADATA};
pub use crate::gate::error::IngestError;
pub use crate::gate::limits::{IngestLimits, OtlpWireLimits};
use crate::namespaces::BifrostNamespace;
use crate::oracle::{AuthorizedQueryContext, OracleQueryStream, QueryStreamLifecycle};
pub use crate::otlp_contract::{IngestOutcome, LogsOutcome, MetricsOutcome};
use wyrd_spec::vala::api::BifrostQueryRequest;
use wyrd_spec::vala::error::BifrostError;

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

/// Records one refused native write against the one Gate rejection taxonomy.
///
/// The projection lives on [`IngestError::rejection_reason`], so no call site
/// restates it. A refusal the projection declines to classify is a Wyrd
/// internal failure and is deliberately left out of the rejection family: a
/// server defect must not inflate the caller-attributed rejection rate.
fn record_write_rejection(error: &IngestError) {
    if let Some(reason) = error.rejection_reason() {
        record_gate_rejection("write", reason);
    }
}

/// Derives the deterministic OTLP batch identity used for retry suppression.
///
/// OTLP is at-least-once and carries no request idempotency key, so a retried
/// export must be recognized by what it accepted rather than by transport
/// metadata. The identity is the SHA-256 of a domain separator, the
/// authenticated tenant, the logical table FQN, and the canonical batch's
/// logical Arrow digest from [`crate::scribe::preprocess::logical_data_identity`],
/// which deliberately excludes the per-request managed columns (request id,
/// ingest time) that change across a legitimate retry. Scoping by tenant and
/// table keeps identity from correlating across either boundary.
///
/// The first sixteen digest bytes are stamped with the RFC 4122 variant and
/// UUID version 7 so the value satisfies the same `wyrd_batch_id` contract the
/// native ingest path enforces. Scribe's WAL and Postgres commit fence then own
/// the actual duplicate decision: a repeat returns `AlreadyCommitted` and a
/// digest collision carrying a different durable identity stays fail-closed.
///
/// # Errors
///
/// Returns the mapped Scribe failure when the canonical batch has no
/// representable logical identity.
fn otlp_batch_id(
    tenant: wyrd_spec::ids::DataTenantId,
    table: &TableRef,
    batch: &arrow::record_batch::RecordBatch,
) -> Result<uuid::Uuid, IngestError> {
    use sha2::{Digest as _, Sha256};

    let (logical_digest, _) = crate::scribe::preprocess::logical_data_identity(batch)
        .map_err(IngestError::from_scribe)?;
    let mut digest = Sha256::new();
    digest.update(b"wyrd.otlp.batch-id.v1");
    digest.update(tenant.as_uuid().as_bytes());
    digest.update(table.fqn().as_bytes());
    digest.update(logical_digest);
    let digest: [u8; 32] = digest.finalize().into();
    let mut bytes: [u8; 16] = digest[..16].try_into().expect("sixteen digest bytes");
    bytes[6] = (bytes[6] & 0x0f) | 0x70;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Ok(uuid::Uuid::from_bytes(bytes))
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

/// Describe and initialize the closed Gate metric families at process boot.
///
/// Every series this owner can ever emit is published at zero here. A
/// Prometheus counter appears in a render only after a handle exists for its
/// exact label set, so a terminal or refusal that never occurred in a scrape
/// window would otherwise leave a hole rather than a zero. Qualification reads
/// each family as a closed cross product, so an absent series and a zero series
/// must not be distinguishable. The server calls this immediately after the
/// global recorder is live; registering earlier would emit into the no-op
/// recorder and be lost.
pub fn initialize_gate_metrics() {
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
        for reason in crate::gate::error::GATE_REJECTION_REASONS {
            metrics::counter!(
                "bifrost_gate_rejections_total",
                "operation" => operation,
                "reason" => reason
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
pub struct Gate<R: PermissionResolver + 'static, I: IssuerConfigResolver + 'static> {
    /// Optional durable write capability; absent on a query-only replica.
    scribe: Option<Arc<dyn Scribe>>,
    /// Optional SQL dispatch seam reaching an Oracle this Gate does not own.
    query: Option<Arc<dyn OracleQueryDispatch>>,
    /// Immutable transport and typed-ingress bounds.
    limits: IngestLimits,
    /// Shared bearer-token verification adapter for every public transport.
    auth: IngestAuthInterceptor<R, I>,
    /// Shared admission closure observed by ingest and query alike.
    closed: Arc<AtomicBool>,
}

impl<R: PermissionResolver + 'static, I: IssuerConfigResolver + 'static> Gate<R, I> {
    /// Requests an exact root-backed decode child from Scribe for the adapter.
    ///
    /// # Errors
    ///
    /// Returns [`IngestError::IngressClosed`] when lifecycle shutdown already
    /// closed this Gate or Scribe is absent, and a stable ingress refusal when
    /// the ingress envelope is occupied or Scribe resource accounting fails
    /// while reserving the preflighted typed-request capacity. The closure
    /// check runs before any reservation, so a draining replica never holds
    /// Scribe memory for a request it will refuse.
    pub fn reserve_otlp_decode(&self, bytes: usize) -> Result<OtlpDecodeOwner, IngestError> {
        if self.is_closed() {
            return Err(IngestError::IngressClosed);
        }
        self.scribe
            .as_ref()
            .ok_or(IngestError::IngressClosed)?
            .reserve_otlp_decode(bytes)
            .map_err(IngestError::from_scribe)
    }

    /// Returns the immutable OTLP limits shared with the server decode adapter.
    #[must_use]
    pub const fn otlp_wire_limits(&self) -> OtlpWireLimits {
        self.limits.otlp
    }

    /// Returns the tonic frame ceiling derived from the same boot snapshot.
    #[must_use]
    pub const fn otlp_decoding_message_size(&self) -> usize {
        self.limits.max_decoding_message_size
    }

    /// Construct a Gate with a required Scribe capability.
    #[must_use]
    pub fn with_scribe(
        scribe: Arc<crate::scribe::ScribeImpl>,
        auth: IngestAuthInterceptor<R, I>,
        limits: IngestLimits,
    ) -> Self {
        initialize_gate_metrics();
        Self {
            scribe: Some(scribe),
            query: None,
            limits,
            auth,
            closed: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Constructs a Gate around one crate-local ingress test double.
    #[cfg(test)]
    fn with_test_scribe(
        scribe: Arc<dyn Scribe>,
        auth: IngestAuthInterceptor<R, I>,
        limits: IngestLimits,
    ) -> Self {
        initialize_gate_metrics();
        Self {
            scribe: Some(scribe),
            query: None,
            limits,
            auth,
            closed: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Constructs a stable Gate whose ingest role is intentionally unavailable.
    ///
    /// Authentication remains active on this Gate. Authorized ingest reaches the
    /// closed role check and receives the stable unavailable response without a
    /// WAL allocation.
    #[must_use]
    pub fn without_scribe(auth: IngestAuthInterceptor<R, I>, limits: IngestLimits) -> Self {
        initialize_gate_metrics();
        Self {
            scribe: None,
            query: None,
            limits,
            auth,
            closed: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Attaches the seam through which this Gate dispatches public SQL.
    ///
    /// The dispatcher owns role selection. A Gate without one refuses every
    /// query with [`BifrostError::OracleRoleUnavailable`], which is how an
    /// ingest-only replica is expressed.
    #[must_use]
    pub fn with_query_dispatch(mut self, query: Arc<dyn OracleQueryDispatch>) -> Self {
        self.query = Some(query);
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
        self.is_closed()
    }

    /// Reads the one shared admission-closure flag.
    ///
    /// Ingest admission, query admission, and OTLP decode reservation all
    /// consult this single atomic so `close()` cannot leave one public surface
    /// admitting while another refuses.
    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    /// Refuses new ingest once the Gate is closed or Scribe is not ready.
    ///
    /// # Errors
    ///
    /// Returns [`IngestError::IngressClosed`] after `close()` and while the
    /// local Scribe is absent or still recovering.
    fn ensure_open(&self) -> Result<(), IngestError> {
        if self.is_closed() {
            return Err(IngestError::IngressClosed);
        }
        if !self.scribe.as_ref().is_some_and(|scribe| scribe.is_ready()) {
            return Err(IngestError::IngressClosed);
        }
        Ok(())
    }

    /// Refuses new query admission once lifecycle shutdown has closed the Gate.
    ///
    /// Unlike [`Self::ensure_open`], this consults only the shared closure
    /// flag: a query-serving replica has no Scribe and must still admit reads
    /// until it begins draining.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::OracleRoleUnavailable`] after `close()`.
    pub fn ensure_query_open(&self) -> Result<(), BifrostError> {
        if self.is_closed() {
            return Err(BifrostError::OracleRoleUnavailable);
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

    /// Authenticates one server-owned OTLP adapter request before tonic codec work.
    ///
    /// The outer service invokes this before `Grpc::unary`, preflight, decode
    /// reservation, or typed construction. Gate retains authentication and
    /// readiness authority while the adapter owns protobuf framing.
    ///
    /// # Errors
    ///
    /// Returns the stable authentication or ingress-closed refusal produced by
    /// the ordinary generated-service path.
    pub async fn authenticate_otlp_metadata(
        &self,
        metadata: &MetadataMap,
    ) -> Result<AuthContext, IngestError> {
        self.authenticate(metadata).await
    }

    /// Mount the Gate on the shared tonic router.
    #[must_use]
    pub fn into_server(self) -> BifrostIngestServiceServer<Self> {
        let size = self.limits.max_decoding_message_size;
        BifrostIngestServiceServer::new(self).max_decoding_message_size(size)
    }

    /// Dispatches one authorized public SQL request through the query seam.
    ///
    /// Gate owns the closed request lifecycle, admission closure, and stream
    /// accounting; the attached [`OracleQueryDispatch`] owns role selection and
    /// may execute locally, forward to a fenced peer, or refuse. The Gate
    /// stream lifecycle is attached to the returned stream after dispatch, so
    /// local and forwarded execution are accounted identically.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::OracleRoleUnavailable`] before any accounting
    /// when this Gate is closed or has no query dispatcher, otherwise returns
    /// the dispatcher's stable query errors.
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
        self.ensure_query_open()?;
        let request_lifecycle = GateRequestLifecycle::begin("query");
        let Some(dispatch) = &self.query else {
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
        metrics::gauge!("bifrost_gate_active_streams", "operation" => "query").increment(1.0);
        let lifecycle = Arc::new(QueryStreamLifecycle::new(|outcome, elapsed| {
            metrics::counter!("bifrost_gate_query_streams_total", "outcome" => outcome)
                .increment(1);
            metrics::histogram!("bifrost_gate_query_stream_duration_seconds", "outcome" => outcome)
                .record(elapsed.as_secs_f64());
            metrics::gauge!("bifrost_gate_active_streams", "operation" => "query").decrement(1.0);
        }));
        let result = dispatch
            .dispatch_sql(context, request)
            .instrument(tracing::info_span!(
                "bifrost.gate.query",
                operation = "query"
            ))
            .await
            .map(|stream| stream.with_gate_lifecycle(Arc::clone(&lifecycle)));
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

    /// Routes one adapter-decoded trace export and its move-only owner.
    ///
    /// # Errors
    ///
    /// Returns authorization, request-limit, Scribe, or outcome-shape errors.
    #[tracing::instrument(skip_all, fields(tenant = %auth.tenant, request_id = %auth.request_id))]
    pub async fn ingest_decoded_resource_spans(
        &self,
        auth: &AuthContext,
        decoded: DecodedOtlp<ExportTraceServiceRequest>,
    ) -> Result<IngestOutcome, IngestError> {
        self.ensure_open()?;
        record_gate_event("otlp_export");
        if let Err(error) = authorize_record_write(auth) {
            record_gate_event("otlp_rejection");
            return Err(error);
        }
        crate::otlp_limits::enforce_trace_limits(&decoded.request, decoded.wire_bytes, self.limits)
            .map_err(IngestError::from_scribe)?;
        let (batch, outcome) = crate::tables::traces::project_resource_spans(
            &decoded.request.resource_spans,
            auth.principal.card_ref_scope(),
        )
        .map_err(|error| IngestError::Internal(format!("trace projection failed: {error}")))?;
        self.dispatch_canonical(
            auth,
            TableRef::new(BifrostNamespace::Traces, "spans"),
            decoded.wire_bytes,
            batch,
            decoded.owner,
        )
        .await?;
        record_gate_rows(outcome.accepted_spans, outcome.rejected_spans);
        Ok(outcome)
    }

    /// Routes one adapter-decoded metrics export and its move-only owner.
    ///
    /// # Errors
    ///
    /// Returns authorization, request-limit, Scribe, or outcome-shape errors.
    #[tracing::instrument(skip_all, fields(tenant = %auth.tenant, request_id = %auth.request_id))]
    pub async fn ingest_decoded_resource_metrics(
        &self,
        auth: &AuthContext,
        decoded: DecodedOtlp<ExportMetricsServiceRequest>,
    ) -> Result<MetricsOutcome, IngestError> {
        self.ensure_open()?;
        record_gate_event("otlp_export");
        if let Err(error) = authorize_record_write(auth) {
            record_gate_event("otlp_rejection");
            return Err(error);
        }
        crate::otlp_limits::enforce_metric_limits(
            &decoded.request,
            decoded.wire_bytes,
            self.limits,
        )
        .map_err(IngestError::from_scribe)?;
        let (batch, outcome) = crate::tables::metrics::project_resource_metrics(
            &decoded.request.resource_metrics,
            auth.principal.card_ref_scope(),
        )
        .map_err(|error| IngestError::Internal(format!("metric projection failed: {error}")))?;
        self.dispatch_canonical(
            auth,
            TableRef::new(BifrostNamespace::Metrics, "points"),
            decoded.wire_bytes,
            batch,
            decoded.owner,
        )
        .await?;
        record_gate_rows(outcome.accepted_points, outcome.rejected_points);
        Ok(outcome)
    }

    /// Routes one adapter-decoded log export and its move-only owner.
    ///
    /// # Errors
    ///
    /// Returns authorization, request-limit, Scribe, or outcome-shape errors.
    #[tracing::instrument(skip_all, fields(tenant = %auth.tenant, request_id = %auth.request_id))]
    pub async fn ingest_decoded_resource_logs(
        &self,
        auth: &AuthContext,
        decoded: DecodedOtlp<ExportLogsServiceRequest>,
    ) -> Result<LogsOutcome, IngestError> {
        self.ensure_open()?;
        record_gate_event("otlp_export");
        if let Err(error) = authorize_record_write(auth) {
            record_gate_event("otlp_rejection");
            return Err(error);
        }
        crate::otlp_limits::enforce_log_limits(&decoded.request, decoded.wire_bytes, self.limits)
            .map_err(IngestError::from_scribe)?;
        let (batch, outcome) = crate::tables::logs::project_resource_logs(
            &decoded.request.resource_logs,
            auth.principal.card_ref_scope(),
        )
        .map_err(|error| IngestError::Internal(format!("log projection failed: {error}")))?;
        self.dispatch_canonical(
            auth,
            TableRef::new(BifrostNamespace::Logs, "records"),
            decoded.wire_bytes,
            batch,
            decoded.owner,
        )
        .await?;
        record_gate_rows(outcome.accepted_records, outcome.rejected_records);
        Ok(outcome)
    }

    #[tracing::instrument(
        skip_all,
        fields(tenant = %auth.tenant, table = %table, request_id = %auth.request_id,
               wire_bytes = measured_wire_bytes)
    )]
    async fn dispatch_canonical(
        &self,
        auth: &AuthContext,
        table: TableRef,
        measured_wire_bytes: usize,
        batch: arrow::record_batch::RecordBatch,
        owner: Option<OtlpDecodeOwner>,
    ) -> Result<(), IngestError> {
        self.ensure_open()?;
        if measured_wire_bytes > self.limits.max_frame_bytes {
            return Err(IngestError::PayloadTooLarge {
                bytes: u64::try_from(measured_wire_bytes).unwrap_or(u64::MAX),
                limit: u64::try_from(self.limits.max_frame_bytes).unwrap_or(u64::MAX),
            });
        }
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
            payload_summary: "one bounded OTLP frame".to_owned(),
            detail: None,
        };
        if batch.num_rows() == 0 {
            return Ok(());
        }
        let batch_id = otlp_batch_id(auth.tenant, &table, &batch)?;
        let payload = IngressPayload::Canonical(match owner {
            Some(owner) => CanonicalIngress::new(vec![batch], owner),
            None => CanonicalIngress::unreserved(vec![batch]),
        });
        let scribe = self.scribe.as_ref().ok_or(IngestError::IngressClosed)?;
        scribe
            .ingest_frame(ScribeIngressFrame {
                principal: auth.principal.clone(),
                authenticated_tenant: auth.tenant,
                table,
                expected_schema_fingerprint: None,
                request_id: auth.request_id.clone(),
                batch_id,
                audit_event,
                measured_wire_bytes,
                payload,
            })
            .await
            .map_err(|error| {
                record_gate_event("scribe_failure");
                IngestError::from_scribe(error)
            })?;
        Ok(())
    }

    /// Authorizes and routes one bounded native batch to Scribe.
    ///
    /// Once validation and admission reach Scribe, the durable append runs in
    /// its own task so a transport deadline cannot cancel the WAL decision.
    /// The caller still awaits that decision when connected; after cancellation,
    /// a retry with the same batch ID observes Scribe's durable dedup result.
    ///
    /// # Errors
    ///
    /// Returns a stable validation, authorization, admission, persistence, or
    /// task-failure error. Transport cancellation can make the result ambiguous
    /// to the caller, but does not revoke an already-dispatched durable append.
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
        let table = TableRef::new(namespace, name);
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
        let ingress = ScribeIngressFrame {
            principal: auth.principal.clone(),
            authenticated_tenant: auth.tenant,
            table,
            expected_schema_fingerprint: None,
            request_id: auth.request_id.clone(),
            batch_id,
            audit_event,
            measured_wire_bytes: frame.arrow_ipc.len(),
            payload: IngressPayload::ArrowIpc(frame.arrow_ipc),
        };
        // The durable Scribe write stays inside this request future on purpose.
        // Detaching it onto its own task would orphan the admission owner when a
        // transport drops the handler: the spawned task keeps its admission slot
        // and ingress bytes while nothing observes its terminal. Awaiting inline
        // makes the admission guard drop with the cancelled request.
        let admission = scribe.ingest_frame(ingress).await.map_err(|error| {
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

/// Resolves one validated public table name into its closed namespace and local name.
///
/// # Errors
/// Returns a stable request-validation error for an unknown namespace.
pub fn resolve_fqn(fqn: &str) -> Result<(BifrostNamespace, String), IngestError> {
    BifrostNamespace::split_fqn(fqn)
        .ok_or_else(|| IngestError::RequestValidation(format!("unrecognized table fqn: {fqn}")))
}

#[wyrd_tonic::tonic::async_trait]
impl<R: PermissionResolver + 'static, I: IssuerConfigResolver + 'static> BifrostIngestService
    for Gate<R, I>
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
                .inspect_err(record_write_rejection)
                .map_err(Status::from)?;
            self.dispatch_native_frame(&self.limits, &auth, frame.clone())
                .await
                .inspect_err(|error| {
                    record_gate_event("native_rejection");
                    record_write_rejection(error);
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

/// Validates one native Arrow ingress envelope before provider or Scribe IO.
///
/// # Errors
/// Returns a stable request-validation or payload-limit error.
pub fn validate_batch(
    frame: &InsertBatchRequest,
    limits: &IngestLimits,
) -> Result<(), IngestError> {
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

#[cfg(test)]
mod tests {
    use std::str::FromStr as _;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::limits::IngestLimits;
    use super::{AuthContext, Gate, IngestError};
    use crate::contracts::DecodedOtlp;
    use arrow::array::Array as _;
    use arrow::record_batch::RecordBatch;
    use async_trait::async_trait;
    use futures_util::StreamExt as _;
    use wyrd_auth_oidc::IssuerConfigResolver;
    use wyrd_auth_verify::PermissionResolver;
    use wyrd_runtime::{Permission, PermissionSet, Principal, PrincipalKind};
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::ids::DataTenantId;
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::QueryStreamFrame;
    use wyrd_spec::vala::error::BifrostError;
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
        fn is_ready(&self) -> bool {
            true
        }

        /// Records one owner-backed frame accepted by the Gate test double.
        ///
        /// # Errors
        ///
        /// This test implementation is infallible after its frame assertions;
        /// assertion failures panic rather than returning [`ScribeError`].
        async fn ingest_frame(
            &self,
            frame: crate::contracts::ScribeIngressFrame,
        ) -> Result<crate::contracts::FrameAdmission, crate::contracts::ScribeError> {
            let _ = frame;
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
        /// Row count of every canonical batch Gate handed to Scribe, in order.
        rows: Arc<Mutex<Vec<usize>>>,
        /// `card_ref` value of every handed row, in accepted order.
        card_refs: Arc<Mutex<Vec<Option<String>>>>,
    }

    impl CountingScribe {
        /// Builds one counting double sharing `calls` and its own row log.
        fn new(calls: Arc<AtomicUsize>) -> Self {
            Self {
                calls,
                rows: Arc::new(Mutex::new(Vec::new())),
                card_refs: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    #[async_trait]
    impl crate::contracts::Scribe for CountingScribe {
        fn is_ready(&self) -> bool {
            true
        }

        /// Records one owner-backed frame accepted by the counting test double.
        ///
        /// # Errors
        ///
        /// This implementation returns no error after the frame invariants
        /// succeed; invariant violations panic in the test that owns it.
        async fn ingest_frame(
            &self,
            frame: crate::contracts::ScribeIngressFrame,
        ) -> Result<crate::contracts::FrameAdmission, crate::contracts::ScribeError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            assert_eq!(frame.authenticated_tenant, frame.principal.tenant_id);
            assert_eq!(frame.table.fqn(), "vala.traces.spans");
            assert!(frame.expected_schema_fingerprint.is_none());
            let crate::contracts::IngressPayload::Canonical(canonical) = &frame.payload else {
                panic!("Gate must hand Scribe validated canonical batches");
            };
            assert!(canonical.owner.is_some(), "Gate must move its decode owner");
            self.rows
                .lock()
                .expect("row log is not poisoned")
                .extend(canonical.batches.iter().map(RecordBatch::num_rows));
            let scope = frame.principal.card_ref_scope();
            let mut handed = self.card_refs.lock().expect("card log is not poisoned");
            for batch in &canonical.batches {
                let column = batch
                    .column_by_name("card_ref")
                    .expect("a canonical signal batch carries its correlation column")
                    .as_any()
                    .downcast_ref::<arrow::array::StringArray>()
                    .expect("card_ref is Utf8");
                for row in 0..column.len() {
                    if column.is_null(row) {
                        handed.push(None);
                        continue;
                    }
                    let raw = column.value(row);
                    let card = wyrd_spec::reference::CardRef::from_str(raw)
                        .expect("Gate must only hand Scribe parseable references");
                    assert!(
                        scope.is_some_and(|scope| scope.authorizes(&card)),
                        "Gate must not hand Scribe a reference outside the signed scope"
                    );
                    handed.push(Some(raw.to_owned()));
                }
            }
            Ok(crate::contracts::FrameAdmission {
                batch_id: uuid::Uuid::now_v7(),
                rows_accepted: 0,
            })
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
            delegation_chain: Vec::new(),
        }
    }

    /// One authenticated context whose principal carries the shared signed scope.
    ///
    /// Card correlation is only assertable from a signed scope, so a Gate test
    /// that exercises `wyrd.card_ref` needs a service principal rather than the
    /// plain user `auth_context` builds.
    fn scoped_auth_context() -> AuthContext {
        let tenant = DataTenantId::new_v7();
        let scope = crate::tables::signal::correlation_fixture::scope();
        let principal = Principal::new(
            PrincipalId::new(uuid::Uuid::now_v7()),
            PrincipalKind::Service {
                card_ref: scope.as_slice()[0].clone(),
                card_ref_scope: scope,
            },
            tenant,
            Vec::new(),
            PermissionSet::from_iter([Permission::bifrost_record_write()]),
        );
        AuthContext {
            principal,
            tenant,
            request_id: RequestId::now_v7(),
            delegation_chain: Vec::new(),
        }
    }

    /// One valid span carrying `card_ref` as its record correlation attribute.
    fn span_resource_with_card(
        index: u8,
        card_ref: &str,
    ) -> wyrd_tonic::otlp::trace::v1::ResourceSpans {
        let mut resource = span_resource(index, true);
        resource.scope_spans[0].spans[0].attributes =
            vec![wyrd_tonic::otlp::common::v1::KeyValue {
                key: "wyrd.card_ref".to_owned(),
                value: Some(wyrd_tonic::otlp::common::v1::AnyValue {
                    value: Some(wyrd_tonic::otlp::common::v1::any_value::Value::StringValue(
                        card_ref.to_owned(),
                    )),
                }),
            }];
        resource
    }

    /// Couples one trace fixture to a real root-backed decode owner.
    fn decoded_trace(request: ExportTraceServiceRequest) -> DecodedOtlp<ExportTraceServiceRequest> {
        DecodedOtlp::new(request, 0, crate::scribe::otlp_decode_owner_for_test(1))
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
        let _gate = Gate::<TestPermissionResolver, TestIssuerResolver>::with_test_scribe(
            Arc::new(TestScribe),
            crate::gate::auth::ingest_auth_interceptor(Arc::new(verifier)),
            IngestLimits::default(),
        );
    }

    #[tokio::test]
    async fn gate_enforces_bifrost_record_write() {
        let scribe_calls = Arc::new(AtomicUsize::new(0));
        let gate = Gate::with_test_scribe(
            Arc::new(CountingScribe::new(Arc::clone(&scribe_calls))),
            test_interceptor(),
            IngestLimits::default(),
        );

        let error = gate
            .ingest_decoded_resource_spans(
                &auth_context(false),
                decoded_trace(ExportTraceServiceRequest::default()),
            )
            .await
            .expect_err("permission must be denied");
        assert!(matches!(error, IngestError::RbacDenied { .. }));
        assert_eq!(scribe_calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn gate_authenticates_before_reading_frames() {
        let gate = Gate::with_test_scribe(
            Arc::new(TestScribe),
            test_interceptor(),
            IngestLimits::default(),
        );
        let metadata = wyrd_tonic::tonic::metadata::MetadataMap::new();
        assert!(gate.authenticate(&metadata).await.is_err());
    }

    /// One valid span in `resource_spans` position `index`, or an invalid one.
    ///
    /// An invalid span carries a truncated trace id, which the canonical trace
    /// projection rejects whole while accepting its siblings.
    fn span_resource(index: u8, valid: bool) -> wyrd_tonic::otlp::trace::v1::ResourceSpans {
        wyrd_tonic::otlp::trace::v1::ResourceSpans {
            scope_spans: vec![wyrd_tonic::otlp::trace::v1::ScopeSpans {
                spans: vec![wyrd_tonic::otlp::trace::v1::Span {
                    trace_id: if valid {
                        vec![index; 16]
                    } else {
                        vec![index; 3]
                    },
                    span_id: vec![index; 8],
                    name: format!("span-{index}"),
                    start_time_unix_nano: 1,
                    end_time_unix_nano: 2,
                    ..wyrd_tonic::otlp::trace::v1::Span::default()
                }],
                ..wyrd_tonic::otlp::trace::v1::ScopeSpans::default()
            }],
            ..wyrd_tonic::otlp::trace::v1::ResourceSpans::default()
        }
    }

    /// Gate projects a mixed OTLP export into exactly the accepted rows (S1).
    ///
    /// Scribe receives one canonical batch whose row count equals the accepted
    /// span count, so the ordinals it stamps form one contiguous range starting
    /// at zero with no gap left by a rejected span. A span whose `wyrd.card_ref`
    /// lies outside the principal's signed scope, or names a signed member the
    /// mint left without a UID, is one more rejected sibling rather than a
    /// whole-request refusal: its valid siblings still store, and the returned
    /// partial-success counts stay exact. Scribe stamps `card_uid` from the
    /// matching signed member alone, which its own resolution tests pin.
    #[tokio::test]
    async fn mixed_otlp_projection_assigns_only_accepted_contiguous_ordinals() {
        use crate::tables::signal::correlation_fixture;

        let scribe_calls = Arc::new(AtomicUsize::new(0));
        let scribe = Arc::new(CountingScribe::new(Arc::clone(&scribe_calls)));
        let rows = Arc::clone(&scribe.rows);
        let card_refs = Arc::clone(&scribe.card_refs);
        let gate = Gate::with_test_scribe(scribe, test_interceptor(), IngestLimits::default());

        let outcome = gate
            .ingest_decoded_resource_spans(
                &scoped_auth_context(),
                decoded_trace(ExportTraceServiceRequest {
                    resource_spans: vec![
                        span_resource(1, true),
                        span_resource(2, false),
                        span_resource_with_card(3, correlation_fixture::IN_SCOPE),
                        span_resource_with_card(4, correlation_fixture::OUT_OF_SCOPE),
                        span_resource_with_card(5, correlation_fixture::WITHOUT_UID),
                        span_resource(6, true),
                    ],
                }),
            )
            .await
            .expect("Gate must route without consulting its catalog adapter");

        assert_eq!(
            (outcome.accepted_spans, outcome.rejected_spans),
            (3, 3),
            "an unauthorized reference rejects only its own span"
        );
        assert!(outcome.rejection_message.is_some());
        assert_eq!(scribe_calls.load(Ordering::Relaxed), 1);
        assert_eq!(*rows.lock().expect("row log is not poisoned"), vec![3]);
        assert_eq!(
            *card_refs.lock().expect("card log is not poisoned"),
            vec![None, Some(correlation_fixture::IN_SCOPE.to_owned()), None,],
            "only accepted rows reach Scribe, in request order"
        );
    }

    /// An export with no accepted span never reaches Scribe (S1).
    ///
    /// Gate still returns the projection's own partial-success outcome, so the
    /// OTLP caller observes its rejections unchanged.
    #[tokio::test]
    async fn all_invalid_otlp_returns_existing_outcome_without_scribe() {
        let scribe_calls = Arc::new(AtomicUsize::new(0));
        let gate = Gate::with_test_scribe(
            Arc::new(CountingScribe::new(Arc::clone(&scribe_calls))),
            test_interceptor(),
            IngestLimits::default(),
        );

        let outcome = gate
            .ingest_decoded_resource_spans(
                &auth_context(true),
                decoded_trace(ExportTraceServiceRequest {
                    resource_spans: vec![span_resource(1, false), span_resource(2, false)],
                }),
            )
            .await
            .expect("a fully rejected export is a successful partial response");

        assert_eq!((outcome.accepted_spans, outcome.rejected_spans), (0, 2));
        assert!(outcome.rejection_message.is_some());
        assert_eq!(scribe_calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn close_rejects_new_work_before_scribe_handoff() {
        let scribe_calls = Arc::new(AtomicUsize::new(0));
        let gate = Gate::with_test_scribe(
            Arc::new(CountingScribe::new(Arc::clone(&scribe_calls))),
            test_interceptor(),
            IngestLimits::default(),
        );
        gate.close();

        let error = gate
            .ingest_decoded_resource_spans(
                &auth_context(true),
                decoded_trace(ExportTraceServiceRequest::default()),
            )
            .await
            .expect_err("closed Gate must reject new work");
        assert!(matches!(error, IngestError::IngressClosed));
        assert_eq!(scribe_calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn gate_rejects_ingest_when_scribe_recovery_is_incomplete() {
        let gate = Gate::with_test_scribe(
            Arc::new(NotReadyScribe),
            test_interceptor(),
            IngestLimits::default(),
        );

        let error = gate
            .ingest_decoded_resource_spans(
                &auth_context(true),
                decoded_trace(ExportTraceServiceRequest::default()),
            )
            .await
            .expect_err("Gate must fail closed while Scribe recovery is incomplete");
        assert!(matches!(error, IngestError::IngressClosed));
    }

    /// Test double standing in for the server-tier SQL dispatch owner.
    ///
    /// It records that Gate reached the seam and returns either a synthetic
    /// stream that emits one successful terminal, or a stable refusal, so the
    /// Gate-side accounting can be asserted without an Oracle.
    struct TestQueryDispatch {
        /// Number of times Gate handed a request across the seam.
        calls: Arc<AtomicUsize>,
        /// Refusal returned instead of a stream, when set.
        refusal: Option<BifrostError>,
    }

    #[async_trait]
    impl crate::contracts::OracleQueryDispatch for TestQueryDispatch {
        /// Records one dispatch and returns the configured stream or refusal.
        ///
        /// # Errors
        ///
        /// Returns the configured [`BifrostError`] when this double was built
        /// to refuse.
        async fn dispatch_sql(
            &self,
            _context: crate::oracle::AuthorizedQueryContext,
            _request: wyrd_spec::vala::api::BifrostQueryRequest,
        ) -> Result<crate::oracle::OracleQueryStream, BifrostError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            if let Some(error) = &self.refusal {
                return Err(error.clone());
            }
            let frames = futures_util::stream::iter([Ok(QueryStreamFrame::Terminal(
                wyrd_spec::vala::api::QueryTerminalFrame {
                    outcome: wyrd_spec::vala::api::QueryTerminalOutcome::Success,
                    freshness: wyrd_spec::vala::api::QueryFreshness::Complete,
                    execution_path: wyrd_spec::vala::api::QueryExecutionPath::Interactive,
                    row_count: 0,
                    warnings: Vec::new(),
                    source_completion: Vec::new(),
                    error: None,
                    arrow_ipc_eos: vec![0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0],
                },
            ))]);
            Ok(crate::oracle::OracleQueryStream::test_new(
                "seam".to_owned(),
                Box::pin(frames),
                tokio_util::sync::CancellationToken::new(),
            ))
        }
    }

    /// Builds one authorized query context without a server or Postgres.
    fn query_context() -> crate::oracle::AuthorizedQueryContext {
        let auth = auth_context(true);
        crate::oracle::AuthorizedQueryContext {
            principal: auth.principal,
            data_tenant_id: auth.tenant,
            request_id: auth.request_id,
            trace_id: None,
            auth_method: wyrd_spec::vala::api::AuthMethod::Jwt,
            permission: "bifrost:record:read".to_owned(),
            delegation_chain: auth.delegation_chain,
        }
    }

    /// Builds one minimal well-formed public SQL request.
    fn query_request() -> wyrd_spec::vala::api::BifrostQueryRequest {
        wyrd_spec::vala::api::BifrostQueryRequest {
            sql: "SELECT 1".to_owned(),
            visibility: wyrd_spec::vala::api::VisibilityMode::PublishedOnly,
            freshness: wyrd_spec::vala::api::FreshnessPolicy::Strict,
            deadline_ms: None,
        }
    }

    /// Builds a query-only Gate: no Scribe, optionally one dispatch seam.
    fn query_gate(
        dispatch: Option<Arc<TestQueryDispatch>>,
    ) -> Gate<TestPermissionResolver, TestIssuerResolver> {
        let gate = Gate::without_scribe(test_interceptor(), IngestLimits::default());
        match dispatch {
            Some(dispatch) => gate.with_query_dispatch(dispatch),
            None => gate,
        }
    }

    /// Gate owns the whole query request lifecycle around the dispatch seam.
    ///
    /// # Panics
    ///
    /// Panics when a refusal reaches stream accounting, when a dispatched
    /// stream does not carry the Gate lifecycle, or when either path fails to
    /// settle exactly one terminal request outcome.
    #[test]
    fn query_dispatch_seam_owns_the_request_lifecycle() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        let calls = Arc::new(AtomicUsize::new(0));

        let unconfigured = query_gate(None);
        let error = metrics::with_local_recorder(&recorder, || {
            wyrd_runtime::runtime()
                .block_on(unconfigured.query_sql(query_context(), query_request()))
        })
        .expect_err("a Gate with no dispatch seam must refuse");
        assert!(matches!(error, BifrostError::OracleRoleUnavailable));
        let snapshot = recorder.snapshot();
        assert_eq!(
            snapshot
                .counters
                .get("bifrost_gate_requests_total{operation=\"query\",outcome=\"rejected\"}"),
            Some(&1)
        );
        assert_eq!(
            snapshot.counters.get(
                "bifrost_gate_rejections_total{operation=\"query\",reason=\"role_unavailable\"}"
            ),
            Some(&1)
        );
        assert_eq!(
            snapshot
                .gauges
                .get("bifrost_gate_active_streams{operation=\"query\"}"),
            None,
            "a refusal before dispatch must not touch stream accounting"
        );

        let dispatched = query_gate(Some(Arc::new(TestQueryDispatch {
            calls: Arc::clone(&calls),
            refusal: None,
        })));
        metrics::with_local_recorder(&recorder, || {
            wyrd_runtime::runtime().block_on(async {
                let stream = dispatched
                    .query_sql(query_context(), query_request())
                    .await
                    .expect("the seam returns an admitted stream");
                let mut frames = stream.frames;
                while frames.next().await.is_some() {}
            });
        });
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        let snapshot = recorder.snapshot();
        assert_eq!(
            snapshot
                .counters
                .get("bifrost_gate_requests_total{operation=\"query\",outcome=\"success\"}"),
            Some(&1)
        );
        assert_eq!(
            snapshot
                .counters
                .get("bifrost_gate_query_streams_total{outcome=\"success\"}"),
            Some(&1),
            "the dispatched stream must carry the Gate stream lifecycle"
        );
    }

    /// A closed Gate refuses queries even with no Scribe present.
    #[tokio::test]
    async fn closed_gate_refuses_queries_without_a_scribe() {
        let calls = Arc::new(AtomicUsize::new(0));
        let gate = query_gate(Some(Arc::new(TestQueryDispatch {
            calls: Arc::clone(&calls),
            refusal: None,
        })));
        gate.close();

        assert!(matches!(
            gate.ensure_query_open(),
            Err(BifrostError::OracleRoleUnavailable)
        ));
        let error = gate
            .query_sql(query_context(), query_request())
            .await
            .expect_err("a closed Gate must refuse queries");
        assert!(matches!(error, BifrostError::OracleRoleUnavailable));
        assert_eq!(
            calls.load(Ordering::Relaxed),
            0,
            "a closed Gate must refuse before reaching the dispatch seam"
        );
    }

    /// A closed Gate refuses OTLP decode before reserving Scribe memory.
    #[test]
    fn closed_gate_refuses_otlp_decode_before_reserving() {
        let gate = Gate::with_test_scribe(
            Arc::new(NotReadyScribe),
            test_interceptor(),
            IngestLimits::default(),
        );
        gate.close();

        let error = gate
            .reserve_otlp_decode(1024)
            .expect_err("a closed Gate must refuse decode reservation");
        assert!(matches!(error, IngestError::IngressClosed));
    }

    /// Every write refusal records exactly the label the one taxonomy projects.
    ///
    /// # Panics
    ///
    /// Panics when a variant records a different label than
    /// [`IngestError::rejection_reason`] projects, or when an internal failure
    /// is attributed to the caller.
    #[test]
    fn insert_batch_uses_the_one_rejection_taxonomy() {
        let cases = [
            IngestError::Unauthenticated("no bearer".to_owned()),
            IngestError::PrincipalUnresolved,
            IngestError::RbacDenied {
                detail: "bifrost:record:write denied".to_owned(),
            },
            IngestError::ReservedBuiltinWriteDenied {
                table: "vala.audit.events".to_owned(),
            },
            IngestError::RequestValidation("bad batch id".to_owned()),
            IngestError::Decode("truncated frame".to_owned()),
            IngestError::PayloadTooLarge { bytes: 2, limit: 1 },
            IngestError::TableNotFound {
                table: "vala.traces.absent".to_owned(),
            },
            IngestError::IngressClosed,
            IngestError::WalDiskFull,
        ];
        for error in cases {
            let recorder = wyrd_bench::BenchmarkRecorder::default();
            metrics::with_local_recorder(&recorder, || super::record_write_rejection(&error));
            let reason = error
                .rejection_reason()
                .expect("every case is a caller-attributed rejection");
            assert_eq!(
                recorder.snapshot().counters.get(&format!(
                    "bifrost_gate_rejections_total{{operation=\"write\",reason=\"{reason}\"}}"
                )),
                Some(&1),
                "{error} must record exactly its projected reason"
            );
        }

        let recorder = wyrd_bench::BenchmarkRecorder::default();
        let internal = IngestError::Internal("wyrd defect".to_owned());
        metrics::with_local_recorder(&recorder, || super::record_write_rejection(&internal));
        assert!(
            recorder.snapshot().counters.is_empty(),
            "a server defect must not be attributed to the caller"
        );
    }

    /// One refusal advances only its own reason label.
    ///
    /// # Panics
    ///
    /// Panics when the projected series does not advance by exactly one or an
    /// unrelated reason moves.
    #[test]
    fn gate_rejection_increments_only_the_projected_reason() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        let error = IngestError::TableNotFound {
            table: "vala.bifrost/absent".to_owned(),
        };
        let reason = error
            .rejection_reason()
            .expect("a table-not-found refusal is a caller-attributed rejection");
        metrics::with_local_recorder(&recorder, || {
            super::initialize_gate_metrics();
            super::record_write_rejection(&error);
        });

        let snapshot = recorder.snapshot();
        assert_eq!(
            snapshot.counters.get(&format!(
                "bifrost_gate_rejections_total{{operation=\"write\",reason=\"{reason}\"}}"
            )),
            Some(&1),
            "the projected reason must advance by exactly one"
        );
        for other in crate::gate::error::GATE_REJECTION_REASONS {
            if other == reason {
                continue;
            }
            assert_eq!(
                snapshot.counters.get(&format!(
                    "bifrost_gate_rejections_total{{operation=\"write\",reason=\"{other}\"}}"
                )),
                Some(&0),
                "an unrelated reason must not move"
            );
        }
    }

    /// Boot publishes the whole closed rejection table at zero.
    ///
    /// # Panics
    ///
    /// Panics when any `operation` x `reason` pair is absent from the
    /// initialized registry, which would make an absent series and a zero
    /// series indistinguishable to a scrape.
    #[test]
    fn initialize_gate_metrics_zero_registers_the_closed_rejection_table() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        metrics::with_local_recorder(&recorder, super::initialize_gate_metrics);

        let snapshot = recorder.snapshot();
        for operation in ["write", "query"] {
            for reason in crate::gate::error::GATE_REJECTION_REASONS {
                assert_eq!(
                    snapshot.counters.get(&format!(
                        "bifrost_gate_rejections_total{{operation=\"{operation}\",reason=\"{reason}\"}}"
                    )),
                    Some(&0),
                    "missing seeded rejection series for {operation}/{reason}"
                );
            }
        }
    }

    /// Native batch identity acceptance is unchanged in both directions.
    ///
    /// # Panics
    ///
    /// Panics when a 16-byte non-UUIDv7 identity is admitted, a short identity
    /// is admitted, or a valid `UUIDv7` is refused.
    #[test]
    fn batch_identity_acceptance_is_unchanged() {
        let limits = IngestLimits::default();
        let frame = |batch_id: Vec<u8>| wyrd_tonic::wyrd::v1::InsertBatchRequest {
            table: "vala.traces.spans".to_owned(),
            wyrd_batch_id: batch_id.into(),
            arrow_ipc: Vec::new().into(),
        };

        let uuid_v4 = uuid::Uuid::new_v4();
        assert!(matches!(
            super::validate_batch(&frame(uuid_v4.as_bytes().to_vec()), &limits),
            Err(IngestError::RequestValidation(_)),
        ));
        assert!(matches!(
            super::validate_batch(&frame(vec![0_u8; 15]), &limits),
            Err(IngestError::RequestValidation(_))
        ));
        assert!(
            super::validate_batch(&frame(uuid::Uuid::now_v7().as_bytes().to_vec()), &limits)
                .is_ok()
        );
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

#[cfg(test)]
mod otlp_batch_id_tests {
    use std::sync::Arc;

    use arrow::array::Int32Array;
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::record_batch::RecordBatch;
    use wyrd_spec::ids::DataTenantId;

    use super::{TableRef, otlp_batch_id};
    use crate::namespaces::BifrostNamespace;

    /// Builds one canonical-shaped batch carrying `values` in order.
    fn batch(values: &[i32]) -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![Field::new("v", DataType::Int32, false)]));
        RecordBatch::try_new(schema, vec![Arc::new(Int32Array::from(values.to_vec()))])
            .expect("the fixture batch matches its schema")
    }

    /// The derived OTLP identity is stable per tenant, table, and payload.
    ///
    /// Retry suppression depends on a repeated export converging on one
    /// `wyrd_batch_id`, and on any other tenant, table, row order, or payload
    /// diverging so identity never correlates across those boundaries. The
    /// value must also remain a `UUIDv7`, which is the contract the native
    /// ingest path validates.
    ///
    /// # Panics
    ///
    /// Panics when a repeat diverges, when a distinct input converges, or when
    /// the derived value is not a `UUIDv7`.
    #[test]
    fn derived_identity_is_stable_per_tenant_table_and_payload() {
        let tenant = DataTenantId::new_v7();
        let other_tenant = DataTenantId::new_v7();
        let spans = TableRef::new(BifrostNamespace::Traces, "spans");
        let records = TableRef::new(BifrostNamespace::Logs, "records");
        let id = |tenant, table: &TableRef, values: &[i32]| {
            otlp_batch_id(tenant, table, &batch(values)).expect("the fixture batch has an identity")
        };

        let baseline = id(tenant, &spans, &[1, 2, 3]);
        assert_eq!(
            baseline,
            id(tenant, &spans, &[1, 2, 3]),
            "a replayed logical batch keeps one identity"
        );
        assert_eq!(
            baseline.get_version(),
            Some(uuid::Version::SortRand),
            "the derived identity satisfies the UUIDv7 batch contract"
        );
        for divergent in [
            id(other_tenant, &spans, &[1, 2, 3]),
            id(tenant, &records, &[1, 2, 3]),
            id(tenant, &spans, &[3, 2, 1]),
            id(tenant, &spans, &[1, 2, 4]),
        ] {
            assert_ne!(
                baseline, divergent,
                "a different tenant, table, row order, or payload is a different batch"
            );
        }
    }
}
