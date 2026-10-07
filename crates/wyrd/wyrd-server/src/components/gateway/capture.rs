//! Opt-in analytical capture of terminal gateway calls into Bifrost.
//!
//! After a call's accounting settles, [`CallCapture`] projects its terminal
//! facts under the policy admitted with the call into one
//! `vala.gateway.calls` row and one linked GenAI span per attempt, applying
//! mandatory secret redaction and binary-content references to any selected
//! payload. [`GatewayCapture`], the process's one capture writer, then submits
//! both batches as a server-internal write: in-process to this pod's Scribe,
//! or over the mutually authenticated peer plane to a live Scribe when this
//! pod runs none. Capture holds no token, evaluates no permission, and writes
//! no audit decision. Delivery is bounded by the call's deadline, so a slow,
//! saturated, or unavailable Bifrost drops the capture with a counted reason
//! and never changes the call. A disabled policy never reaches this module.

use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use arrow::array::{ArrayRef, RecordBatch};
use arrow::datatypes::Schema;
use arrow::ipc::writer::StreamWriter;
use arrow::json::ReaderBuilder;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::time::Instant;
use uuid::Uuid;
use vala_bifrost_redux::catalog::TableRef;
use vala_bifrost_redux::cluster::ClusterRegistry;
use vala_bifrost_redux::contracts::{IngressPayload, Scribe, ScribeError, ScribeIngressFrame};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::oracle::dispatcher::BifrostPeerTls;
use vala_bifrost_redux::tables::gateway::{CallsTable, REQUEST_PAYLOAD, RESPONSE_PAYLOAD};
use vala_bifrost_redux::tables::signal::correlation_fields;
use vala_bifrost_redux::tables::traces::project_resource_spans;
use vala_bifrost_redux::tables::{DomainTable, SpansTable};
use wyrd_gateway::{AttemptRecord, IngressDialect, MediaRequest, UploadContent};
use wyrd_queue::variant::{EncodedVariant, VariantColumnBuilder};
use wyrd_runtime::{PermissionSet, Principal, PrincipalKind};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{GATEWAY_CAPTURE_PRINCIPAL, PrincipalId};
use wyrd_spec::gateway::{
    GATEWAY_JSON_MAX_BYTES, GatewayAccountingEntryV1, GatewayAttemptSpanFieldsV1, GatewayCallId,
    GatewayCallOutcome, GatewayCallPayloadV1, GatewayCaptureMode, GatewayCapturePolicy,
    GatewayOperation, GatewayPayloadField, GatewayPayloadObjectRefV1, ModelRef,
};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::NodeId;
use wyrd_storage::StorageError;
use wyrd_storage::tenant_path::{self, ValidatedPath};
use wyrd_tonic::otlp::common::v1::any_value::Value as AnyValueKind;
use wyrd_tonic::otlp::common::v1::{AnyValue, InstrumentationScope, KeyValue};
use wyrd_tonic::otlp::resource::v1::Resource;
use wyrd_tonic::otlp::trace::v1::span::SpanKind;
use wyrd_tonic::otlp::trace::v1::status::StatusCode;
use wyrd_tonic::otlp::trace::v1::{ResourceSpans, ScopeSpans, Span, Status};
use wyrd_tonic::tonic::Code;
use wyrd_tonic::tonic::transport::Channel;
use wyrd_tonic::wyrd::v1::IngestCaptureRequest;
use wyrd_tonic::wyrd::v1::scribe_capture_peer_service_client::ScribeCapturePeerServiceClient;

use crate::state::{AppState, Bifrost};

/// Value substituted for a redacted secret.
const REDACTED: &str = "[REDACTED]";

/// Service and instrumentation-scope name of published attempt spans.
const GATEWAY_SERVICE: &str = "wyrd.gateway";

/// First pause before resubmitting a batch Scribe refused retryably.
const RETRY_INITIAL: Duration = Duration::from_millis(10);

/// Longest pause between resubmissions; the call's deadline bounds the total.
const RETRY_MAX: Duration = Duration::from_secs(1);

/// Normalized key fragments whose values are always credentials.
///
/// Keys are lowercased and stripped to ASCII alphanumerics first, so
/// `x-goog-api-key`, `apiKey`, and `api_key` all match `apikey`.
const SECRET_KEY_FRAGMENTS: &[&str] = &[
    "authorization",
    "apikey",
    "secret",
    "password",
    "passwd",
    "credential",
    "cookie",
    "privatekey",
    "bearer",
    "accesstoken",
    "refreshtoken",
    "idtoken",
    "sessiontoken",
];

/// Why one call's capture was not delivered; the call itself is unaffected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CaptureDrop {
    /// A selected payload could not be canonicalized within the fixed ceiling,
    /// so it is dropped rather than persisted unreviewed.
    Payload,
    /// The projected record violated its public contract.
    Invalid,
    /// The record could not be assembled into its Arrow batch or encoded.
    Projection,
    /// No Scribe acknowledged before the call's deadline: none is reachable
    /// from this pod, it is closed or failing, or the deadline elapsed.
    Unavailable,
    /// Scribe refused the batch for backpressure until the call's deadline.
    Saturated,
    /// Scribe refused the batch terminally, for example as malformed.
    Rejected,
    /// A selected binary object could not be written to or verified in object
    /// storage, so the whole capture is dropped rather than referencing bytes
    /// that may not be retrievable.
    Storage,
}

impl CaptureDrop {
    /// Closed metric label and diagnostic reason of this drop.
    pub(crate) const fn reason(self) -> &'static str {
        match self {
            Self::Payload => "payload",
            Self::Invalid => "invalid",
            Self::Projection => "projection",
            Self::Unavailable => "unavailable",
            Self::Saturated => "saturated",
            Self::Rejected => "rejected",
            Self::Storage => "storage",
        }
    }

    /// Whether a later resubmission of the same batch may be acknowledged.
    const fn is_retryable(self) -> bool {
        matches!(self, Self::Saturated | Self::Unavailable)
    }

    /// Classifies an in-process Scribe refusal.
    ///
    /// Backpressure and a full WAL are saturation; a closed, failing, or
    /// object-store-blocked writer is unavailable; anything else refuses this
    /// batch for good.
    pub(crate) fn from_scribe(error: &ScribeError) -> Self {
        match error {
            ScribeError::IngestBusy { .. } | ScribeError::WalDiskFull => Self::Saturated,
            ScribeError::IngressClosed
            | ScribeError::ObjectStorePutFailed(_)
            | ScribeError::Internal { .. } => Self::Unavailable,
            _ => Self::Rejected,
        }
    }

    /// Classifies a peer Scribe's gRPC refusal by status code.
    ///
    /// The peer service projects Scribe errors through Gate's one ingest
    /// status mapping, so saturation arrives as `ResourceExhausted`, and a
    /// closed or failing writer, like an unreachable peer, as `Unavailable`
    /// or `Internal`.
    fn from_code(code: Code) -> Self {
        match code {
            Code::ResourceExhausted => Self::Saturated,
            Code::Unavailable | Code::Internal | Code::DeadlineExceeded | Code::Unknown => {
                Self::Unavailable
            }
            _ => Self::Rejected,
        }
    }
}

/// One of the only two destinations gateway capture writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CaptureTable {
    /// `vala.gateway.calls`, one row per captured call.
    Calls,
    /// `vala.traces.spans`, one span per captured attempt.
    Spans,
}

impl CaptureTable {
    /// Fully qualified table name, which the peer RPC carries.
    pub(crate) const fn fqn(self) -> &'static str {
        match self {
            Self::Calls => "vala.gateway.calls",
            Self::Spans => "vala.traces.spans",
        }
    }

    /// Resolves a fully qualified name to a capture destination, refusing any
    /// other table.
    pub(crate) fn from_fqn(fqn: &str) -> Option<Self> {
        [Self::Calls, Self::Spans]
            .into_iter()
            .find(|table| table.fqn() == fqn)
    }

    /// Logical Bifrost table this destination names.
    fn table_ref(self) -> TableRef {
        match self {
            Self::Calls => TableRef::new(BifrostNamespace::Gateway, CallsTable::NAME),
            Self::Spans => TableRef::new(BifrostNamespace::Traces, SpansTable::NAME),
        }
    }
}

/// One encoded capture batch bound for one tenant's capture destination.
///
/// The local writer and the peer service both submit through
/// [`Self::into_frame`], so a batch is identical in-process and over the peer
/// plane. Cloning shares the encoded bytes.
#[derive(Debug, Clone)]
pub(crate) struct CaptureBatch {
    /// Tenant the rows belong to.
    pub(crate) tenant: DataTenantId,
    /// Capture destination.
    pub(crate) table: CaptureTable,
    /// Deterministic identity Scribe deduplicates resubmissions on.
    pub(crate) batch_id: Uuid,
    /// Request that admitted the call.
    pub(crate) request_id: RequestId,
    /// The batch as one Arrow IPC stream.
    pub(crate) ipc: Bytes,
}

impl CaptureBatch {
    /// Encodes `batch` as one IPC stream for `call_id`'s `table` capture.
    ///
    /// # Errors
    ///
    /// Returns [`CaptureDrop::Projection`] when Arrow cannot encode the batch.
    fn encode(
        tenant: DataTenantId,
        call_id: GatewayCallId,
        table: CaptureTable,
        request_id: RequestId,
        batch: &RecordBatch,
    ) -> Result<Self, CaptureDrop> {
        let mut writer = StreamWriter::try_new(Vec::new(), batch.schema().as_ref())
            .map_err(|_| CaptureDrop::Projection)?;
        writer.write(batch).map_err(|_| CaptureDrop::Projection)?;
        let ipc = writer.into_inner().map_err(|_| CaptureDrop::Projection)?;
        Ok(Self {
            tenant,
            table,
            batch_id: Self::derive_id(tenant, call_id, table),
            request_id,
            ipc: Bytes::from(ipc),
        })
    }

    /// Derives the batch identity from the tenant, call, and destination.
    ///
    /// The same call's batch for the same table always has the same id, so a
    /// retry, or a resubmission to another Scribe, is absorbed by Scribe's
    /// batch-id dedup instead of writing the rows twice. The digest carries
    /// UUIDv7 version and variant bits, as Gate's derived OTLP ids do.
    pub(crate) fn derive_id(
        tenant: DataTenantId,
        call_id: GatewayCallId,
        table: CaptureTable,
    ) -> Uuid {
        let mut digest = Sha256::new();
        digest.update(b"wyrd.gateway.capture.batch-id.v1");
        digest.update(tenant.as_uuid().as_bytes());
        digest.update(call_id.as_uuid().as_bytes());
        digest.update(table.fqn().as_bytes());
        let digest: [u8; 32] = digest.finalize().into();
        let mut bytes = [0_u8; 16];
        bytes.copy_from_slice(&digest[..16]);
        bytes[6] = (bytes[6] & 0x0f) | 0x70;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        Uuid::from_bytes(bytes)
    }

    /// Builds the Scribe frame for this batch under the capture principal.
    ///
    /// The principal carries no permission: capture is a server-internal
    /// write that evaluates none, exactly like audit publication, and its id
    /// is what Scribe stamps on the captured rows.
    pub(crate) fn into_frame(self) -> ScribeIngressFrame {
        ScribeIngressFrame {
            principal: Principal::new(
                GATEWAY_CAPTURE_PRINCIPAL,
                PrincipalKind::User,
                self.tenant,
                Vec::new(),
                PermissionSet::new(),
            ),
            authenticated_tenant: self.tenant,
            table: self.table.table_ref(),
            expected_schema_fingerprint: None,
            request_id: self.request_id,
            batch_id: self.batch_id,
            measured_wire_bytes: self.ipc.len(),
            payload: IngressPayload::ArrowIpc(self.ipc),
        }
    }
}

/// Terminal facts of one admitted call, owned so capture can outlive it.
pub(crate) struct CallFacts {
    /// Logical call.
    pub(crate) call_id: GatewayCallId,
    /// Verified tenant.
    pub(crate) tenant: DataTenantId,
    /// Verified caller whose invocation caused the call.
    pub(crate) caller: PrincipalId,
    /// Request that admitted the call, which both publications carry.
    pub(crate) request_id: RequestId,
    /// Requested operation.
    pub(crate) operation: GatewayOperation,
    /// Dialect the caller spoke.
    pub(crate) ingress: IngressDialect,
    /// Whether the caller asked for a streamed answer.
    pub(crate) streaming: bool,
    /// Model the caller requested.
    pub(crate) requested: ModelRef,
    /// Model that completed the call, if any.
    pub(crate) resolved: Option<ModelRef>,
    /// Admission time.
    pub(crate) started_at: DateTime<Utc>,
    /// Terminal time.
    pub(crate) terminal_at: DateTime<Utc>,
    /// Settled logical-call outcome, which is `failed` when accounting
    /// prevented the answer even though the provider attempt succeeded.
    pub(crate) outcome: GatewayCallOutcome,
    /// Stable error code of the settled call: the caller's actual error when
    /// the call failed after its attempts, otherwise the code of `outcome`.
    pub(crate) error_code: Option<String>,
    /// Every attempt in ordinal order.
    pub(crate) attempts: Vec<AttemptRecord>,
    /// Ledger entries this call's accounting appended; empty when accounting
    /// failed, which leaves usage and cost unknown.
    pub(crate) entries: Vec<GatewayAccountingEntryV1>,
    /// Capture policy admitted with the call.
    pub(crate) policy: GatewayCapturePolicy,
    /// Unredacted request content, present only when the policy selects it.
    pub(crate) request: Option<Value>,
    /// Response content, present only when the policy selects it: a
    /// buffered or streamed answer already scrubbed of the provider credential
    /// by the adapter, a refusal, or a binary content reference. An error
    /// drops the capture because selected content could not be represented
    /// within the capture ceiling.
    pub(crate) response: Result<Option<Value>, CaptureDrop>,
    /// Bytes of every selected binary object already substituted by a typed
    /// reference, awaiting persistence before the referencing row is delivered.
    pub(crate) objects: PayloadObjects,
}

/// One call's validated analytical record: its row and attempt spans.
#[derive(Debug)]
pub(crate) struct CallCapture {
    /// Tenant the record belongs to.
    tenant: DataTenantId,
    /// Operation, which names the GenAI spans.
    operation: GatewayOperation,
    /// The `vala.gateway.calls` row.
    payload: GatewayCallPayloadV1,
    /// Bytes of every object the row references, persisted before delivery.
    objects: PayloadObjects,
    /// One span's fields per attempt, in ordinal order.
    spans: Vec<GatewayAttemptSpanFieldsV1>,
    /// Request that admitted the call, published with both batches.
    request_id: RequestId,
}

impl CallCapture {
    /// Projects `facts` into the exact version-1 row and attempt spans.
    ///
    /// Usage, cost, and pricing come only from the appended ledger entries, so
    /// unknown values stay null. `Metadata` writes both payload columns null;
    /// `Payload` writes only the selected fields, each redacted and
    /// canonicalized.
    ///
    /// # Errors
    ///
    /// Returns [`CaptureDrop::Payload`] when a selected payload cannot be
    /// canonicalized within [`GATEWAY_JSON_MAX_BYTES`] or the facts carry that
    /// drop for the response, and
    /// [`CaptureDrop::Invalid`] when the row or a span violates its contract.
    pub(crate) fn from_facts(facts: CallFacts) -> Result<Self, CaptureDrop> {
        let selected = |field| selects(&facts.policy, field);
        let mut objects = facts.objects;
        let request_payload_json = selected(GatewayPayloadField::Request)
            .then_some(facts.request)
            .flatten()
            .map(|value| redacted_canonical_json(value, &mut objects))
            .transpose()?;
        let response_payload_json = selected(GatewayPayloadField::Response)
            .then_some(facts.response?)
            .flatten()
            .map(|value| redacted_canonical_json(value, &mut objects))
            .transpose()?;
        let (usage, cost, currency, pricing_versions) = facts
            .entries
            .iter()
            .find_map(|entry| match entry {
                GatewayAccountingEntryV1::CallAccounted {
                    normalized_usage,
                    cost,
                    currency,
                    pricing_versions,
                    ..
                } => Some((
                    normalized_usage.clone(),
                    cost.clone(),
                    currency.clone(),
                    pricing_versions.clone(),
                )),
                _ => None,
            })
            .unwrap_or_default();
        let spans = facts
            .attempts
            .iter()
            .map(|attempt| attempt_span(facts.call_id, attempt, &facts.entries))
            .collect::<Vec<_>>();
        let payload = GatewayCallPayloadV1 {
            schema_version: 1,
            call_id: facts.call_id,
            caller_principal_id: facts.caller,
            operation: facts.operation,
            ingress_dialect: ingress_dialect(facts.ingress, facts.operation),
            requested_model: facts.requested,
            resolved_deployment: facts
                .resolved
                .as_ref()
                .and(facts.attempts.last())
                .map(|attempt| attempt.deployment.clone()),
            resolved_model: facts.resolved,
            streaming: facts.streaming,
            started_at: facts.started_at,
            terminal_at: facts.terminal_at,
            outcome: facts.outcome,
            error_code: facts.error_code,
            attempt_count: u32::try_from(facts.attempts.len()).unwrap_or(u32::MAX),
            usage,
            cost,
            currency,
            pricing_versions,
            capture_policy_version: facts.policy.version,
            request_payload_json,
            response_payload_json,
            payload_object_refs: objects.refs(),
        };
        payload.validate().map_err(|_| CaptureDrop::Invalid)?;
        for span in &spans {
            span.validate().map_err(|_| CaptureDrop::Invalid)?;
        }
        Ok(Self {
            tenant: facts.tenant,
            operation: facts.operation,
            payload,
            objects,
            spans,
            request_id: facts.request_id,
        })
    }

    /// Builds the one-row `vala.gateway.calls` batch over the table's user
    /// fields followed by its null `card_ref` and `run_id` correlation inputs.
    ///
    /// The scalar columns decode from the serialized payload; the
    /// `request_payload` and `response_payload` Variant columns encode the
    /// payload's redacted canonical JSON texts, keeping their JSON types.
    ///
    /// # Errors
    ///
    /// Returns [`CaptureDrop::Projection`] when Arrow cannot decode the row
    /// and [`CaptureDrop::Payload`] when a selected payload exceeds a Variant
    /// limit.
    pub(crate) fn calls_batch(&self) -> Result<RecordBatch, CaptureDrop> {
        let mut fields = CallsTable::arrow_fields();
        fields.extend(correlation_fields());
        let schema = Arc::new(Schema::new(fields));
        let scalars = Schema::new(
            schema
                .fields()
                .iter()
                .filter(|field| !is_payload_column(field.name()))
                .cloned()
                .collect::<Vec<_>>(),
        );
        let mut decoder = ReaderBuilder::new(Arc::new(scalars))
            .build_decoder()
            .map_err(|_| CaptureDrop::Projection)?;
        decoder
            .serialize(std::slice::from_ref(&self.payload))
            .map_err(|_| CaptureDrop::Projection)?;
        let decoded = decoder
            .flush()
            .ok()
            .flatten()
            .ok_or(CaptureDrop::Projection)?;
        let columns = schema
            .fields()
            .iter()
            .map(|field| match field.name().as_str() {
                REQUEST_PAYLOAD => payload_column(self.payload.request_payload_json.as_deref()),
                RESPONSE_PAYLOAD => payload_column(self.payload.response_payload_json.as_deref()),
                name => decoded
                    .column_by_name(name)
                    .cloned()
                    .ok_or(CaptureDrop::Projection),
            })
            .collect::<Result<Vec<_>, _>>()?;
        RecordBatch::try_new(schema, columns).map_err(|_| CaptureDrop::Projection)
    }

    /// Builds the canonical `vala.traces.spans` batch of the attempt spans.
    ///
    /// Every span shares the call's trace id, so they link to each other and
    /// to the row by `gateway_call_id`. Each carries exactly the
    /// [`GatewayAttemptSpanFieldsV1`] attributes, nulls omitted, plus the
    /// standard GenAI operation, provider, and request-model attributes.
    /// Returns `None` for a call with no attempt.
    ///
    /// # Errors
    ///
    /// Returns [`CaptureDrop::Projection`] when the canonical projection fails
    /// or rejects a span.
    pub(crate) fn spans_batch(&self) -> Result<Option<RecordBatch>, CaptureDrop> {
        if self.spans.is_empty() {
            return Ok(None);
        }
        let trace_id = self.payload.call_id.as_uuid().as_bytes().to_vec();
        let operation = gen_ai_operation(self.operation);
        let spans = self
            .spans
            .iter()
            .map(|fields| otlp_span(&trace_id, operation, fields))
            .collect::<Result<Vec<_>, _>>()?;
        let resource = ResourceSpans {
            resource: Some(Resource {
                attributes: vec![attribute(
                    "service.name",
                    AnyValueKind::StringValue(GATEWAY_SERVICE.to_owned()),
                )],
                ..Default::default()
            }),
            scope_spans: vec![ScopeSpans {
                scope: Some(InstrumentationScope {
                    name: GATEWAY_SERVICE.to_owned(),
                    ..Default::default()
                }),
                spans,
                ..Default::default()
            }],
            ..Default::default()
        };
        let (batch, outcome) = project_resource_spans(&[resource], None, usize::MAX)
            .map_err(|_| CaptureDrop::Projection)?;
        if outcome.rejected_spans > 0 {
            return Err(CaptureDrop::Projection);
        }
        Ok(Some(batch))
    }

    /// Encodes the row batch and, when the call made attempts, the span batch
    /// under their deterministic identities and the call's request ID.
    ///
    /// # Errors
    ///
    /// Returns the projection drop of either batch.
    pub(crate) fn batches(&self) -> Result<Vec<CaptureBatch>, CaptureDrop> {
        let encode = |table, batch: &RecordBatch| {
            CaptureBatch::encode(
                self.tenant,
                self.payload.call_id,
                table,
                self.request_id.clone(),
                batch,
            )
        };
        let mut batches = vec![encode(CaptureTable::Calls, &self.calls_batch()?)?];
        if let Some(spans) = self.spans_batch()? {
            batches.push(encode(CaptureTable::Spans, &spans)?);
        }
        Ok(batches)
    }
}

/// Live, ready Scribes reachable over the peer plane, with one reusable
/// channel each.
///
/// The roster is read from cluster membership on every attempt, so a Scribe
/// that joins, leaves, or stops being ready is followed without restart.
struct CapturePeers {
    /// Authoritative cluster membership.
    cluster: Arc<ClusterRegistry>,
    /// Cluster mTLS identity presented to every peer Scribe.
    tls: BifrostPeerTls,
    /// Reusable channel per Scribe node, keyed with the endpoint it dials.
    ///
    /// An entry is replaced when membership names another endpoint for the
    /// node and dropped when the node leaves the roster. The lock is never
    /// held across an await.
    channels: Mutex<HashMap<NodeId, (String, Channel)>>,
}

impl CapturePeers {
    /// Returns the channel of the Scribe that serves submission `attempt`.
    ///
    /// Ready Scribes are ordered by node id and attempts rotate across them,
    /// so a retry after a refusal is redirected to the next Scribe. A cached
    /// channel is reused while its endpoint matches; otherwise a lazy channel
    /// is built, so a connect failure surfaces on the RPC itself.
    ///
    /// # Errors
    ///
    /// Returns [`CaptureDrop::Unavailable`] when no Scribe is live and ready
    /// or its endpoint is invalid.
    fn channel(&self, attempt: usize) -> Result<Channel, CaptureDrop> {
        let snapshot = self.cluster.snapshot();
        let mut scribes = snapshot
            .live_scribes()
            .into_iter()
            .filter(|lease| lease.ready)
            .map(|lease| (lease.key.node_id, lease.address.as_str()))
            .collect::<Vec<_>>();
        if scribes.is_empty() {
            return Err(CaptureDrop::Unavailable);
        }
        scribes.sort_by_key(|(node_id, _)| node_id.as_uuid());
        let (node_id, endpoint) = scribes[attempt % scribes.len()];
        let mut channels = self
            .channels
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        channels.retain(|cached, _| scribes.iter().any(|(node, _)| node == cached));
        if let Some((cached, channel)) = channels.get(&node_id)
            && cached == endpoint
        {
            return Ok(channel.clone());
        }
        let channel = self
            .tls
            .endpoint(endpoint.to_owned())
            .map_err(|_| CaptureDrop::Unavailable)?
            .connect_lazy();
        channels.insert(node_id, (endpoint.to_owned(), channel.clone()));
        Ok(channel)
    }

    /// Submits `batch` to the Scribe serving `attempt` and waits for its ACK.
    ///
    /// # Errors
    ///
    /// Returns the [`CaptureDrop`] the peer's status code classifies, or
    /// [`CaptureDrop::Unavailable`] when no Scribe can be dialed.
    async fn submit(&self, batch: &CaptureBatch, attempt: usize) -> Result<(), CaptureDrop> {
        let mut client = ScribeCapturePeerServiceClient::new(self.channel(attempt)?);
        client
            .ingest_capture(IngestCaptureRequest {
                tenant_id: batch.tenant.to_string(),
                table: batch.table.fqn().to_owned(),
                batch_id: batch.batch_id.to_string(),
                request_id: batch.request_id.as_str().to_owned(),
                arrow_ipc: batch.ipc.clone(),
            })
            .await
            .map(|_| ())
            .map_err(|status| CaptureDrop::from_code(status.code()))
    }
}

/// Where this process's capture writer submits.
enum CaptureRoute {
    /// Scribe runs in this pod and acknowledges in-process.
    Local(Arc<dyn Scribe>),
    /// This pod runs no Scribe and submits over the peer plane.
    Peer(CapturePeers),
    /// This process can reach no Scribe, so every capture is dropped.
    Unavailable,
}

/// The process's one gateway capture writer.
///
/// It is chosen once from pod topology: in-process when this pod runs Scribe,
/// otherwise the capture-only peer RPC to a live Scribe. It holds no tenant
/// state, queue, or backlog: each capture's batches are submitted while the
/// call's capture task runs and either acknowledged or dropped by the call's
/// deadline.
pub struct GatewayCapture {
    /// Submission path selected at construction.
    route: CaptureRoute,
}

impl GatewayCapture {
    /// Selects the writer for this process's Bifrost topology.
    ///
    /// A local Scribe wins; an Oracle-only pod uses its peer-plane identity
    /// and cluster membership; a process with neither drops every capture as
    /// unavailable.
    #[must_use]
    pub fn for_bifrost(bifrost: &Bifrost) -> Self {
        let route = match (bifrost.scribe(), bifrost.oracle()) {
            (Some(scribe), _) => {
                CaptureRoute::Local(Arc::clone(scribe.scribe()) as Arc<dyn Scribe>)
            }
            (None, Some(oracle)) => match oracle.lifecycle_transport().peer_tls() {
                Some(tls) => CaptureRoute::Peer(CapturePeers {
                    cluster: oracle.cluster(),
                    tls: tls.clone(),
                    channels: Mutex::new(HashMap::new()),
                }),
                None => CaptureRoute::Unavailable,
            },
            (None, None) => CaptureRoute::Unavailable,
        };
        Self { route }
    }

    /// Builds a writer submitting in-process to `scribe`, standing in for a
    /// pod's own Scribe.
    #[cfg(test)]
    pub(crate) fn local(scribe: Arc<dyn Scribe>) -> Self {
        Self {
            route: CaptureRoute::Local(scribe),
        }
    }

    /// Delivers `capture` by `deadline` and records the outcome.
    ///
    /// Every referenced object is written and verified *before* the row
    /// naming it is submitted, so a published reference always had bytes
    /// behind it; a storage failure drops the whole capture instead. Bytes
    /// that persist while delivery fails are left for the bucket lifecycle to
    /// expire. The row batch is delivered before the span batch, each retried
    /// while Scribe refuses retryably and the deadline allows. Every drop is
    /// counted and logged without payload content.
    ///
    /// # Errors
    ///
    /// Returns the [`CaptureDrop`] that prevented delivery, for tests; the
    /// call is unaffected either way.
    pub(crate) async fn publish(
        &self,
        state: &AppState,
        capture: &CallCapture,
        deadline: Instant,
    ) -> Result<(), CaptureDrop> {
        let result = self.deliver(state, capture, deadline).await;
        Self::record(capture.payload.call_id, result);
        result
    }

    /// Persists `capture`'s objects, then delivers each of its batches.
    ///
    /// # Errors
    ///
    /// Returns the storage, projection, or delivery drop that stopped it.
    async fn deliver(
        &self,
        state: &AppState,
        capture: &CallCapture,
        deadline: Instant,
    ) -> Result<(), CaptureDrop> {
        capture.objects.persist(state, capture.tenant).await?;
        for batch in capture.batches()? {
            until_deadline(deadline, |attempt| self.submit(&batch, attempt)).await?;
        }
        Ok(())
    }

    /// Submits `batch` once along this writer's route.
    ///
    /// # Errors
    ///
    /// Returns the classified Scribe refusal, or [`CaptureDrop::Unavailable`]
    /// when this process reaches no Scribe.
    async fn submit(&self, batch: &CaptureBatch, attempt: usize) -> Result<(), CaptureDrop> {
        match &self.route {
            CaptureRoute::Local(scribe) => scribe
                .ingest_frame(batch.clone().into_frame())
                .await
                .map(|_| ())
                .map_err(|error| CaptureDrop::from_scribe(&error)),
            CaptureRoute::Peer(peers) => peers.submit(batch, attempt).await,
            CaptureRoute::Unavailable => Err(CaptureDrop::Unavailable),
        }
    }

    /// Counts and logs one capture outcome.
    pub(crate) fn record(call_id: GatewayCallId, result: Result<(), CaptureDrop>) {
        let outcome = result.err().map_or("delivered", CaptureDrop::reason);
        metrics::counter!("wyrd_gateway_capture_total", "outcome" => outcome).increment(1);
        if outcome != "delivered" {
            tracing::warn!(
                call_id = %call_id.as_uuid(),
                reason = outcome,
                "gateway call capture dropped; the call is unaffected"
            );
        }
    }
}

/// Runs `attempt` until it is acknowledged, refused terminally, or
/// `deadline` passes.
///
/// Each attempt receives its ordinal and is cancelled at `deadline`. A
/// retryable refusal waits an exponentially growing pause, from
/// [`RETRY_INITIAL`] up to [`RETRY_MAX`], before the next attempt; a pause
/// that would reach the deadline ends delivery with that refusal instead.
///
/// # Errors
///
/// Returns a terminal refusal at once, and otherwise the latest retryable
/// refusal once the deadline leaves no room for another attempt, or
/// [`CaptureDrop::Unavailable`] when the first attempt itself outlives the
/// deadline.
async fn until_deadline<F, Fut>(deadline: Instant, mut attempt: F) -> Result<(), CaptureDrop>
where
    F: FnMut(usize) -> Fut,
    Fut: Future<Output = Result<(), CaptureDrop>>,
{
    let mut pause = RETRY_INITIAL;
    let mut latest = CaptureDrop::Unavailable;
    for ordinal in 0.. {
        latest = match tokio::time::timeout_at(deadline, attempt(ordinal)).await {
            Ok(Ok(())) => return Ok(()),
            Ok(Err(refusal)) if refusal.is_retryable() => refusal,
            Ok(Err(refusal)) => return Err(refusal),
            Err(_) => return Err(latest),
        };
        if Instant::now() + pause >= deadline {
            break;
        }
        tokio::time::sleep(pause).await;
        pause = (pause * 2).min(RETRY_MAX);
    }
    Err(latest)
}

/// Request content of a call as capture sees it: the operation body, plus a
/// reference for each uploaded file.
///
/// # Errors
///
/// Returns the IO error of a spooled upload that cannot be reread.
pub(crate) async fn request_content(
    body: &Value,
    media: Option<&MediaRequest>,
    objects: &mut PayloadObjects,
) -> std::io::Result<Value> {
    let Some(media) = media else {
        return Ok(body.clone());
    };
    let mut files = Vec::with_capacity(media.files.len());
    for file in &media.files {
        // ponytail: the upload is buffered whole so its exact bytes can be
        // persisted and verified; stream through `operator().writer` if
        // captured uploads ever outgrow one call's memory budget.
        let bytes = match &file.content {
            UploadContent::Bytes(bytes) => bytes.to_vec(),
            UploadContent::Spooled { file: spooled, len } => {
                let mut reader = tokio::fs::File::from_std(spooled.try_clone()?);
                reader.seek(std::io::SeekFrom::Start(0)).await?;
                let mut buffer = Vec::with_capacity(usize::try_from(*len).unwrap_or(0));
                reader.take(*len).read_to_end(&mut buffer).await?;
                buffer
            }
        };
        let mut entry = objects.reference(&file.content_type, bytes);
        if let Some(members) = entry.as_object_mut() {
            members.insert("field".to_owned(), Value::String(file.field.clone()));
        }
        files.push(entry);
    }
    Ok(json!({ "body": body, "files": files }))
}

/// Whether `policy` persists the `field` payload column.
pub(crate) fn selects(policy: &GatewayCapturePolicy, field: GatewayPayloadField) -> bool {
    policy.mode == GatewayCaptureMode::Payload && policy.payload_fields.contains(&field)
}

/// Reserved owner segment of every captured payload object.
///
/// Captured objects belong to a tenant's capture stream rather than to any
/// Card, so they share one fixed owner segment inside the existing tenant-path
/// grammar instead of widening that shared contract with an optional owner.
const CAPTURED_OBJECT_OWNER: Uuid = Uuid::from_u128(0x0191_f3a6_7c41_7c9d_9f2e_4b61_0a83_d5e7);

/// Tenant-scoped storage key of the object named by `digest`, or `None` when
/// the digest is not the canonical `sha256:` form.
///
/// The key is derived from the verified tenant and the digest alone, and is
/// never returned to a caller or written into a row, so one tenant's digest
/// can never address another tenant's object and a published reference reveals
/// nothing about where its bytes live.
pub(crate) fn object_path(tenant: DataTenantId, digest: &str) -> Option<ValidatedPath> {
    let hex = digest.strip_prefix("sha256:")?;
    if hex.len() != 64
        || !hex
            .chars()
            .all(|character| character.is_ascii_hexdigit() && !character.is_uppercase())
    {
        return None;
    }
    let full = tenant_path::build(
        tenant,
        &CAPTURED_OBJECT_OWNER.to_string(),
        &format!("gateway/payload-objects/{hex}"),
    );
    tenant_path::validate(&full, tenant).ok()
}

/// Bytes of every selected binary object one capture must persist, keyed by
/// the typed reference that stands in for them.
///
/// Response projection, request-content resolution, and redaction all
/// substitute references through [`Self::reference`], so this collects exactly
/// the objects the row ends up naming. The `BTreeMap` supplies the contract's
/// ordering and duplicate-freedom directly: repeated identical bytes inside one
/// call converge on a single entry, which is also what makes a replayed call
/// write idempotently rather than storing the same bytes twice.
#[derive(Default)]
pub(crate) struct PayloadObjects {
    /// Exact bytes behind each reference.
    objects: BTreeMap<GatewayPayloadObjectRefV1, Vec<u8>>,
}

impl std::fmt::Debug for PayloadObjects {
    /// Prints only how many objects are held.
    ///
    /// The bytes are captured call content, so they never reach a diagnostic
    /// even through a derived `Debug` on an enclosing type.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PayloadObjects")
            .field("objects", &self.objects.len())
            .finish()
    }
}

impl PayloadObjects {
    /// Records `bytes` under their canonical digest and returns the reference
    /// that replaces them in the captured payload.
    ///
    /// The returned value is the exact serialization of
    /// [`GatewayPayloadObjectRefV1`], so the payload JSON and the row's own
    /// reference list carry one shape and no bytes, base64, storage path, or
    /// backend locator ever reaches Bifrost.
    pub(crate) fn reference(&mut self, content_type: &str, bytes: Vec<u8>) -> Value {
        let reference = GatewayPayloadObjectRefV1 {
            digest: format!("sha256:{}", hex::encode(Sha256::digest(&bytes))),
            content_type: content_type.to_owned(),
            size_bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        };
        let projection = json!({
            "digest": reference.digest,
            "content_type": reference.content_type,
            "size_bytes": reference.size_bytes,
        });
        self.objects.insert(reference, bytes);
        projection
    }

    /// The sorted, duplicate-free references the row publishes.
    fn refs(&self) -> Vec<GatewayPayloadObjectRefV1> {
        self.objects.keys().cloned().collect()
    }

    /// Persists and verifies every held object under `tenant`.
    ///
    /// An object already stored with identical bytes is left untouched, so a
    /// replayed call converges on the one object instead of rewriting it;
    /// otherwise the bytes are written and read back, and any disagreement in
    /// content or length fails closed rather than publishing a reference whose
    /// bytes may differ. Keys are tenant-derived, so identical bytes in two
    /// tenants remain two independent objects.
    ///
    /// # Errors
    ///
    /// Returns [`CaptureDrop::Storage`] when a key cannot be derived or any
    /// read, write, or verification fails. The caller's own call, its first
    /// byte, streaming, terminal outcome, and accounting are unaffected.
    async fn persist(&self, state: &AppState, tenant: DataTenantId) -> Result<(), CaptureDrop> {
        for (reference, bytes) in &self.objects {
            let path = object_path(tenant, &reference.digest).ok_or(CaptureDrop::Storage)?;
            match state.storage.get_object(&path).await {
                Ok(stored) if stored == *bytes => continue,
                Ok(_) => return Err(CaptureDrop::Storage),
                Err(StorageError::ObjectNotFound { .. }) => {}
                Err(_) => return Err(CaptureDrop::Storage),
            }
            state
                .storage
                .put_object(&path, bytes.clone())
                .await
                .map_err(|_| CaptureDrop::Storage)?;
            let stored = state
                .storage
                .get_object(&path)
                .await
                .map_err(|_| CaptureDrop::Storage)?;
            if stored != *bytes {
                return Err(CaptureDrop::Storage);
            }
        }
        Ok(())
    }
}

/// Whether `name` is one of the two Variant payload columns of
/// `vala.gateway.calls`, which the JSON row decoder cannot build.
fn is_payload_column(name: &str) -> bool {
    name == REQUEST_PAYLOAD || name == RESPONSE_PAYLOAD
}

/// Encodes one optional canonical JSON payload text as a one-row Variant
/// column; an unselected payload is a null row.
///
/// # Errors
///
/// Returns [`CaptureDrop::Payload`] when the text is not JSON or exceeds a
/// Variant limit, so the capture is dropped rather than stored partially.
fn payload_column(text: Option<&str>) -> Result<ArrayRef, CaptureDrop> {
    VariantColumnBuilder::encode("payload", [text], EncodedVariant::from_json_text)
        .map_err(|_| CaptureDrop::Payload)
}

/// Redacts `value`, collecting substituted binary content into `objects`, and
/// returns its RFC 8785 canonical JSON.
///
/// # Errors
///
/// Returns [`CaptureDrop::Payload`] when inline binary content is malformed,
/// or when canonicalization fails or exceeds [`GATEWAY_JSON_MAX_BYTES`].
fn redacted_canonical_json(
    mut value: Value,
    objects: &mut PayloadObjects,
) -> Result<String, CaptureDrop> {
    redact(&mut value, objects)?;
    serde_jcs::to_string(&value)
        .ok()
        .filter(|text| text.len() <= GATEWAY_JSON_MAX_BYTES)
        .ok_or(CaptureDrop::Payload)
}

/// Replaces credentials and inline binary content throughout `value`.
///
/// A member whose normalized key contains a [`SECRET_KEY_FRAGMENTS`] entry,
/// or is exactly `token`, is replaced by [`REDACTED`]. Base64 data URLs,
/// `b64_json` members, and `data` members beside a media-type member become
/// typed references through `objects`, so the decoded bytes are kept for
/// persistence while no base64 content is persisted in the row.
///
/// # Errors
///
/// Returns [`CaptureDrop::Payload`] when any inline binary content is not
/// valid base64, so encoded text is never stored as if it were the content.
// ponytail: key-name and inline-base64 shape rules only; the adapter already
// removes the attempt's exact provider credential from response content and
// drops a projection encoding it, so add value-pattern scanning only for
// secrets the gateway never resolved.
fn redact(value: &mut Value, objects: &mut PayloadObjects) -> Result<(), CaptureDrop> {
    match value {
        Value::Object(map) => redact_object(map, objects)?,
        Value::Array(values) => {
            for member in values {
                redact(member, objects)?;
            }
        }
        Value::String(text) => {
            if let Some(reference) = data_url_reference(text, objects)? {
                *value = reference;
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
    Ok(())
}

/// Applies [`redact`] to one object's members.
///
/// # Errors
///
/// Returns [`CaptureDrop::Payload`] when an inline binary member is not valid
/// base64.
fn redact_object(
    map: &mut Map<String, Value>,
    objects: &mut PayloadObjects,
) -> Result<(), CaptureDrop> {
    let media_type = ["mime_type", "mimeType", "media_type", "format"]
        .iter()
        .find_map(|key| map.get(*key).and_then(Value::as_str))
        .map(str::to_owned);
    for (key, member) in map.iter_mut() {
        let normalized: String = key
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_lowercase())
            .collect();
        if normalized == "token"
            || SECRET_KEY_FRAGMENTS
                .iter()
                .any(|fragment| normalized.contains(fragment))
        {
            *member = Value::String(REDACTED.to_owned());
            continue;
        }
        let inline = match (key.as_str(), member.as_str(), &media_type) {
            ("b64_json", Some(encoded), _) => Some(("application/octet-stream", encoded)),
            ("data", Some(encoded), Some(kind)) => Some((kind.as_str(), encoded)),
            _ => None,
        };
        match inline {
            Some((kind, encoded)) => {
                *member = objects.reference(kind, decode_base64(encoded)?);
            }
            None => redact(member, objects)?,
        }
    }
    Ok(())
}

/// Reference for a `data:<type>;base64,<content>` URL, or `None` for any
/// other text.
///
/// # Errors
///
/// Returns [`CaptureDrop::Payload`] when the URL's content is not valid base64.
fn data_url_reference(
    text: &str,
    objects: &mut PayloadObjects,
) -> Result<Option<Value>, CaptureDrop> {
    let Some((kind, encoded)) = text
        .strip_prefix("data:")
        .and_then(|rest| rest.split_once(";base64,"))
    else {
        return Ok(None);
    };
    Ok(Some(objects.reference(kind, decode_base64(encoded)?)))
}

/// Decoded bytes of `encoded`.
///
/// # Errors
///
/// Returns [`CaptureDrop::Payload`] when `encoded` is not valid standard
/// base64; the encoded text is never substituted for the content.
fn decode_base64(encoded: &str) -> Result<Vec<u8>, CaptureDrop> {
    STANDARD.decode(encoded).map_err(|_| CaptureDrop::Payload)
}

/// Wire name of the ingress dialect a call used.
fn ingress_dialect(ingress: IngressDialect, operation: GatewayOperation) -> String {
    match ingress {
        IngressDialect::OpenAi => format!("openai_{}", operation_name(operation)),
        IngressDialect::AnthropicMessages => "anthropic_messages".to_owned(),
        IngressDialect::GeminiGenerateContent => "gemini_generate_content".to_owned(),
        IngressDialect::VertexGenerateContent => "vertex_generate_content".to_owned(),
    }
}

/// Snake-case wire name of `operation`.
pub(crate) const fn operation_name(operation: GatewayOperation) -> &'static str {
    match operation {
        GatewayOperation::ChatCompletions => "chat_completions",
        GatewayOperation::Responses => "responses",
        GatewayOperation::Embeddings => "embeddings",
        GatewayOperation::Images => "images",
        GatewayOperation::Audio => "audio",
        GatewayOperation::Batches => "batches",
    }
}

/// OpenTelemetry `gen_ai.operation.name` of `operation`, using the standard
/// value where one exists.
const fn gen_ai_operation(operation: GatewayOperation) -> &'static str {
    match operation {
        GatewayOperation::ChatCompletions | GatewayOperation::Responses => "chat",
        other => operation_name(other),
    }
}

/// Stable error code the caller received for a non-successful `outcome`.
pub(crate) fn error_code(outcome: GatewayCallOutcome) -> Option<String> {
    wyrd_gateway::outcome_error_code(outcome).map(str::to_owned)
}

/// Span fields of `attempt`, priced from its ledger entry when one exists.
fn attempt_span(
    call_id: GatewayCallId,
    attempt: &AttemptRecord,
    entries: &[GatewayAccountingEntryV1],
) -> GatewayAttemptSpanFieldsV1 {
    let accounted = entries.iter().find_map(|entry| match entry {
        GatewayAccountingEntryV1::AttemptAccounted {
            attempt_ordinal,
            provider_usage_json,
            normalized_usage,
            cost,
            currency,
            pricing_version,
            ..
        } if *attempt_ordinal == attempt.ordinal => Some((
            provider_usage_json.clone(),
            normalized_usage.clone(),
            cost.clone(),
            currency.clone(),
            pricing_version.clone(),
        )),
        _ => None,
    });
    let (provider_usage_json, normalized_usage, cost, currency, pricing_version) =
        accounted.unwrap_or_default();
    GatewayAttemptSpanFieldsV1 {
        gateway_call_id: call_id,
        attempt_ordinal: attempt.ordinal,
        deployment: attempt.deployment.clone(),
        model: attempt.model.clone(),
        outcome: attempt.outcome,
        error_code: error_code(attempt.outcome),
        started_at: attempt.started_at,
        terminal_at: attempt.terminal_at,
        provider_usage_json,
        normalized_usage,
        cost,
        currency,
        pricing_version,
    }
}

/// One OTLP client span carrying `fields` under the call's `trace_id`.
///
/// # Errors
///
/// Returns [`CaptureDrop::Projection`] when the fields cannot be encoded.
fn otlp_span(
    trace_id: &[u8],
    operation: &str,
    fields: &GatewayAttemptSpanFieldsV1,
) -> Result<Span, CaptureDrop> {
    let Value::Object(members) =
        serde_json::to_value(fields).map_err(|_| CaptureDrop::Projection)?
    else {
        return Err(CaptureDrop::Projection);
    };
    let mut attributes = BTreeMap::new();
    for (key, member) in members {
        let value = match member {
            Value::Null => continue,
            Value::String(text) => AnyValueKind::StringValue(text),
            Value::Number(number) => match number.as_i64() {
                Some(integer) => AnyValueKind::IntValue(integer),
                None => AnyValueKind::StringValue(number.to_string()),
            },
            Value::Bool(flag) => AnyValueKind::BoolValue(flag),
            nested @ (Value::Array(_) | Value::Object(_)) => AnyValueKind::StringValue(
                serde_jcs::to_string(&nested).map_err(|_| CaptureDrop::Projection)?,
            ),
        };
        attributes.insert(key, value);
    }
    for (key, value) in [
        ("gen_ai.operation.name", operation),
        ("gen_ai.provider.name", fields.model.provider.as_str()),
        ("gen_ai.request.model", fields.model.model.as_str()),
    ] {
        attributes.insert(key.to_owned(), AnyValueKind::StringValue(value.to_owned()));
    }
    let nanos = |time: DateTime<Utc>| {
        time.timestamp_nanos_opt()
            .and_then(|nanos| u64::try_from(nanos).ok())
            .unwrap_or_default()
    };
    Ok(Span {
        trace_id: trace_id.to_vec(),
        span_id: u64::from(fields.attempt_ordinal.get())
            .to_be_bytes()
            .to_vec(),
        name: operation.to_owned(),
        kind: SpanKind::Client as i32,
        start_time_unix_nano: nanos(fields.started_at),
        end_time_unix_nano: nanos(fields.terminal_at),
        attributes: attributes
            .into_iter()
            .map(|(key, value)| attribute(&key, value))
            .collect(),
        status: Some(Status {
            code: if fields.outcome == GatewayCallOutcome::Succeeded {
                StatusCode::Ok
            } else {
                StatusCode::Error
            } as i32,
            ..Default::default()
        }),
        ..Default::default()
    })
}

/// One OTLP attribute.
fn attribute(key: &str, value: AnyValueKind) -> KeyValue {
    KeyValue {
        key: key.to_owned(),
        value: Some(AnyValue { value: Some(value) }),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::num::{NonZeroU32, NonZeroU64};
    use std::sync::Arc;

    use arrow::array::Array;
    use chrono::Utc;
    use serde_json::json;
    use tokio::time::{Duration, Instant};
    use vala_bifrost_redux::contracts::ScribeError;
    use wyrd_gateway::{
        AttemptRecord, AttemptUsage, IngressDialect, MediaRequest, OpenAiMediaRoute, UploadContent,
        UploadFile,
    };
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::{GATEWAY_CAPTURE_PRINCIPAL, PrincipalId};
    use wyrd_spec::gateway::{
        GATEWAY_JSON_MAX_BYTES, GatewayCallId, GatewayCallOutcome, GatewayCaptureMode,
        GatewayCapturePolicy, GatewayOperation, GatewayPayloadField, ModelRef,
    };
    use wyrd_spec::ids::ProviderDeploymentName;
    use wyrd_tonic::tonic::Code;

    use super::recording::RecordingScribe;
    use super::{
        CallCapture, CallFacts, CaptureBatch, CaptureDrop, CaptureRoute, CaptureTable,
        GatewayCapture, PayloadObjects, object_path, request_content, until_deadline,
    };

    /// Secret planted under credential-shaped keys; must never be persisted.
    const CANARY: &str = "sk-capture-canary-7f3a";

    /// Base64 image content that must become a reference.
    const IMAGE_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

    /// Capture policy of `mode` selecting `fields`.
    fn policy(mode: GatewayCaptureMode, fields: &[GatewayPayloadField]) -> GatewayCapturePolicy {
        GatewayCapturePolicy {
            mode,
            payload_fields: fields.iter().copied().collect::<BTreeSet<_>>(),
            version: NonZeroU64::new(3).expect("nonzero"),
        }
    }

    /// Terminal facts of one succeeded single-attempt chat call under `policy`
    /// whose request and response both carry the secret canary.
    fn facts(policy: GatewayCapturePolicy) -> CallFacts {
        let model = ModelRef::from_projection("acme/a").expect("model");
        let now = Utc::now();
        CallFacts {
            call_id: GatewayCallId::new_v7(),
            tenant: DataTenantId::new_v7(),
            caller: PrincipalId::new(uuid::Uuid::now_v7()),
            request_id: wyrd_spec::request_id::RequestId::now_v7(),
            operation: GatewayOperation::ChatCompletions,
            ingress: IngressDialect::OpenAi,
            streaming: false,
            requested: model.clone(),
            resolved: Some(model.clone()),
            started_at: now,
            terminal_at: now,
            outcome: GatewayCallOutcome::Succeeded,
            error_code: None,
            attempts: vec![AttemptRecord {
                ordinal: NonZeroU32::MIN,
                deployment: ProviderDeploymentName::new("dep-a").expect("name"),
                model,
                outcome: GatewayCallOutcome::Succeeded,
                billable: true,
                usage: AttemptUsage::default(),
                started_at: now,
                terminal_at: now,
            }],
            entries: Vec::new(),
            policy,
            request: Some(json!({
                "model": "acme/a",
                "api_key": CANARY,
                "headers": {"Authorization": format!("Bearer {CANARY}")},
                "messages": [{"role": "user", "content": [
                    {"type": "image_url", "image_url": {"url": format!("data:image/png;base64,{IMAGE_B64}")}},
                    {"inline_data": {"mime_type": "image/png", "data": IMAGE_B64}},
                ]}],
            })),
            response: Ok(Some(json!({"id": "chatcmpl-1", "token": CANARY}))),
            objects: PayloadObjects::default(),
        }
    }

    /// Proves `Metadata` writes a row with null payload and correlation
    /// columns plus one linked attempt span whose trace id is the call id.
    #[test]
    fn metadata_capture_projects_row_and_linked_spans() {
        let capture = CallCapture::from_facts(facts(policy(GatewayCaptureMode::Metadata, &[])))
            .expect("metadata projects");
        assert_eq!(capture.payload.request_payload_json, None);
        assert_eq!(capture.payload.response_payload_json, None);
        assert_eq!(capture.payload.capture_policy_version.get(), 3);
        assert_eq!(capture.payload.ingress_dialect, "openai_chat_completions");
        assert_eq!(capture.payload.attempt_count, 1);
        assert_eq!(capture.payload.usage, None, "unknown usage stays null");

        let calls = capture.calls_batch().expect("calls batch");
        assert_eq!(calls.num_rows(), 1);
        for column in ["card_ref", "run_id", "request_payload", "response_payload"] {
            let array = calls.column_by_name(column).expect(column);
            assert_eq!(array.null_count(), 1, "{column} is null");
        }

        assert_eq!(capture.spans.len(), 1);
        assert_eq!(capture.spans[0].gateway_call_id, capture.payload.call_id);
        let spans = capture
            .spans_batch()
            .expect("spans project")
            .expect("one attempt yields spans");
        assert_eq!(spans.num_rows(), 1);
    }

    /// Proves an unresolved call nulls both `resolved_model` children.
    ///
    /// DataFusion `get_field` returns a child without its parent's nulls, so
    /// `resolved_model['provider']` reads SQL null only when the child slot
    /// itself is null; the nullable children keep that null through Parquet.
    #[test]
    fn unresolved_call_nulls_resolved_model_children() {
        let mut facts = facts(policy(GatewayCaptureMode::Metadata, &[]));
        facts.resolved = None;
        let calls = CallCapture::from_facts(facts)
            .expect("metadata projects")
            .calls_batch()
            .expect("calls batch");
        let resolved = calls
            .column_by_name("resolved_model")
            .and_then(|column| column.as_any().downcast_ref::<arrow::array::StructArray>())
            .expect("resolved_model Struct");
        assert!(resolved.is_null(0));
        for child in ["provider", "model"] {
            let child = resolved.column_by_name(child).expect(child);
            assert!(child.is_null(0), "an absent model has no child value");
        }
    }

    /// Proves a selected payload is redacted, binary content becomes a
    /// digest reference with no base64, and the unselected field stays null.
    #[test]
    fn payload_capture_redacts_secrets_and_references_binary_content() {
        let capture = CallCapture::from_facts(facts(policy(
            GatewayCaptureMode::Payload,
            &[GatewayPayloadField::Request],
        )))
        .expect("payload projects");
        let request = capture
            .payload
            .request_payload_json
            .as_deref()
            .expect("request selected");
        assert!(!request.contains(CANARY), "{request}");
        assert!(!request.contains(IMAGE_B64), "{request}");
        assert!(request.contains("image/png"), "{request}");
        assert_eq!(
            request.matches("\"digest\"").count(),
            2,
            "both inline images are replaced by the typed reference shape: {request}"
        );
        let refs = &capture.payload.payload_object_refs;
        assert_eq!(
            refs.len(),
            1,
            "the two inline images are the same bytes, so one call converges on one object: {refs:?}"
        );
        let reference = &refs[0];
        reference.validate().expect("the reference is canonical");
        assert_eq!(reference.content_type, "image/png");
        assert!(reference.size_bytes > 0, "{reference:?}");
        assert!(
            request.contains(&reference.digest),
            "the row's root list names the same object the payload references: {request}"
        );
        assert!(
            !request.contains("payload-objects") && !request.contains(&capture.tenant.to_string()),
            "no storage path or backend locator reaches the row: {request}"
        );
        assert_eq!(capture.payload.response_payload_json, None);
    }

    /// Proves a selected upload becomes the typed reference shape while its
    /// exact bytes are collected for persistence.
    ///
    /// The form field survives beside the reference so a reader can still tell
    /// which part an object came from, but no upload content is left inline.
    #[tokio::test]
    async fn selected_uploads_become_typed_references_carrying_their_bytes() {
        const UPLOAD: &[u8] = b"upload-object-bytes";
        let media = MediaRequest {
            route: OpenAiMediaRoute::ImageEdits,
            files: vec![UploadFile {
                field: "image".to_owned(),
                filename: "canvas.png".to_owned(),
                content_type: "image/png".to_owned(),
                content: UploadContent::Bytes(UPLOAD.to_vec()),
            }],
        };
        let mut objects = PayloadObjects::default();
        let content = request_content(&json!({"prompt": "draw"}), Some(&media), &mut objects)
            .await
            .expect("the upload resolves");
        let files = content["files"].as_array().expect("files");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0]["field"], json!("image"));
        assert_eq!(files[0]["content_type"], json!("image/png"));
        assert_eq!(files[0]["size_bytes"], json!(UPLOAD.len()));
        let refs = objects.refs();
        assert_eq!(refs.len(), 1, "the upload is one captured object: {refs:?}");
        refs[0].validate().expect("canonical");
        assert_eq!(files[0]["digest"], json!(refs[0].digest));
        assert!(
            !content.to_string().contains("upload-object-bytes"),
            "no upload content stays inline: {content}"
        );
    }

    /// Proves the derived object key is tenant-scoped and that only the
    /// canonical digest grammar derives one at all.
    ///
    /// The key is the whole of the isolation story for captured objects: the
    /// same digest under two tenants must name two different objects, and a
    /// caller-supplied digest must never be able to escape its tenant prefix.
    #[test]
    fn object_keys_are_tenant_scoped_and_reject_non_canonical_digests() {
        let digest = format!("sha256:{}", "a".repeat(64));
        let one = DataTenantId::new_v7();
        let two = DataTenantId::new_v7();
        let first = object_path(one, &digest).expect("a canonical digest derives a key");
        let second = object_path(two, &digest).expect("a canonical digest derives a key");
        assert_eq!(first.data_tenant_id, one);
        assert!(first.full.starts_with(&one.to_string()), "{first:?}");
        assert_ne!(
            first.full, second.full,
            "identical bytes in two tenants are two objects: {first:?} {second:?}"
        );
        for rejected in [
            format!("sha256:{}", "A".repeat(64)),
            "sha256:abc".to_owned(),
            "a".repeat(64),
            format!("sha1:{}", "a".repeat(64)),
            format!("sha256:{}/../escape", "a".repeat(63)),
        ] {
            assert!(
                object_path(one, &rejected).is_none(),
                "non-canonical digest derives no key: {rejected}"
            );
        }
    }

    /// Proves repeated identical content converges on one reference while
    /// distinct content keeps its own, in the contract's order.
    ///
    /// This convergence inside one call is the same property that makes a
    /// replayed call write the object once instead of storing it twice.
    #[test]
    fn repeated_content_converges_on_one_object_reference() {
        let mut objects = PayloadObjects::default();
        let first = objects.reference("image/png", b"one".to_vec());
        let again = objects.reference("image/png", b"one".to_vec());
        let other = objects.reference("text/plain", b"two".to_vec());
        assert_eq!(
            first, again,
            "identical bytes and media type yield one reference"
        );
        assert_ne!(first, other);
        let refs = objects.refs();
        assert_eq!(refs.len(), 2, "{refs:?}");
        assert!(
            refs.windows(2).all(|pair| pair[0] < pair[1]),
            "the published list is sorted and duplicate-free: {refs:?}"
        );
        for reference in &refs {
            reference.validate().expect("canonical");
            assert_eq!(reference.size_bytes, 3, "{reference:?}");
        }
    }

    /// Proves a payload over the fixed ceiling drops the capture.
    #[test]
    fn oversized_payload_drops_capture() {
        let mut facts = facts(policy(
            GatewayCaptureMode::Payload,
            &[GatewayPayloadField::Request],
        ));
        facts.request = Some(json!({"input": "x".repeat(GATEWAY_JSON_MAX_BYTES)}));
        assert_eq!(
            CallCapture::from_facts(facts).err(),
            Some(CaptureDrop::Payload)
        );
    }

    /// Proves malformed inline base64 — a `b64_json` member, a media `data`
    /// member, and a data URL — drops the whole capture instead of storing the
    /// encoded text as the object's bytes.
    ///
    /// # Panics
    ///
    /// Panics when any malformed encoding still projects a capture.
    #[test]
    fn malformed_inline_base64_drops_capture() {
        for response in [
            json!({"data": [{"b64_json": "not base64!"}]}),
            json!({"audio": {"format": "wav", "data": "%%%"}}),
            json!({"image": "data:image/png;base64,***"}),
        ] {
            let mut facts = facts(policy(
                GatewayCaptureMode::Payload,
                &[GatewayPayloadField::Response],
            ));
            facts.response = Ok(Some(response.clone()));
            let dropped = CallCapture::from_facts(facts);
            assert_eq!(dropped.err(), Some(CaptureDrop::Payload), "{response}");
        }
    }

    /// Proves a capture encodes its row batch before its span batch, each
    /// under the admitting request and an identity derived from tenant, call,
    /// and destination alone.
    ///
    /// # Panics
    ///
    /// Panics when the batches, their order, or their identities differ.
    #[test]
    fn batches_carry_deterministic_ids_under_the_admitting_request() {
        let capture = CallCapture::from_facts(facts(policy(GatewayCaptureMode::Metadata, &[])))
            .expect("metadata projects");
        let batches = capture.batches().expect("batches encode");
        let tables = batches.iter().map(|batch| batch.table).collect::<Vec<_>>();
        assert_eq!(tables, [CaptureTable::Calls, CaptureTable::Spans]);
        let call = capture.payload.call_id;
        for batch in &batches {
            assert_eq!(batch.tenant, capture.tenant);
            assert_eq!(batch.request_id, capture.request_id);
            assert_eq!(
                batch.batch_id,
                CaptureBatch::derive_id(capture.tenant, call, batch.table)
            );
            assert_eq!(batch.batch_id.get_version_num(), 7);
        }
        assert_eq!(
            capture.batches().expect("batches re-encode")[0].batch_id,
            batches[0].batch_id,
            "a re-encoded capture resubmits under the same identity"
        );
        let other_call =
            CaptureBatch::derive_id(capture.tenant, GatewayCallId::new_v7(), CaptureTable::Calls);
        let other_tenant =
            CaptureBatch::derive_id(DataTenantId::new_v7(), call, CaptureTable::Calls);
        assert!(
            ![batches[1].batch_id, other_call, other_tenant].contains(&batches[0].batch_id),
            "destination, call, and tenant each separate identities"
        );
    }

    /// Proves the in-process writer submits each batch under the reserved
    /// capture principal, scoped to the call's tenant, with no permission.
    ///
    /// # Panics
    ///
    /// Panics when a batch is not acknowledged or its frame differs.
    #[tokio::test]
    async fn local_writer_submits_under_the_capture_principal() {
        let capture = CallCapture::from_facts(facts(policy(GatewayCaptureMode::Metadata, &[])))
            .expect("metadata projects");
        let scribe = Arc::new(RecordingScribe::default());
        let writer = GatewayCapture::local(Arc::clone(&scribe) as _);
        let deadline = Instant::now() + Duration::from_secs(5);
        for batch in capture.batches().expect("batches encode") {
            until_deadline(deadline, |attempt| writer.submit(&batch, attempt))
                .await
                .expect("the batch is acknowledged");
        }
        let received = scribe.received();
        let tables = received
            .iter()
            .map(|frame| frame.table.as_str())
            .collect::<Vec<_>>();
        assert_eq!(tables, ["vala.gateway.calls", "vala.traces.spans"]);
        assert_eq!(scribe.rows(), 2, "one call row and one attempt span");
        for frame in &received {
            assert_eq!(frame.principal, GATEWAY_CAPTURE_PRINCIPAL);
            assert_eq!(frame.tenant, capture.tenant);
            assert_eq!(frame.request_id, capture.request_id);
        }
    }

    /// Proves a saturated Scribe is retried with backoff until it
    /// acknowledges, and that saturation lasting to the deadline drops the
    /// batch as saturated without another attempt past it.
    ///
    /// # Panics
    ///
    /// Panics when the retried batch is not acknowledged or the drop differs.
    #[tokio::test(start_paused = true)]
    async fn saturation_retries_until_acknowledged_or_the_deadline() {
        let capture = CallCapture::from_facts(facts(policy(GatewayCaptureMode::Metadata, &[])))
            .expect("metadata projects");
        let batch = capture.batches().expect("batches encode").remove(0);
        let busy = || ScribeError::IngestBusy {
            table: "vala.gateway.calls".to_owned(),
        };

        let recovering = Arc::new(RecordingScribe::default());
        recovering.refuse_next(busy());
        recovering.refuse_next(busy());
        let writer = GatewayCapture::local(Arc::clone(&recovering) as _);
        let deadline = Instant::now() + Duration::from_secs(5);
        assert_eq!(
            until_deadline(deadline, |attempt| writer.submit(&batch, attempt)).await,
            Ok(())
        );
        assert_eq!(recovering.attempts(), 3, "two refusals, then the ACK");
        assert_eq!(recovering.received().len(), 1);

        let saturated = Arc::new(RecordingScribe::default());
        for _ in 0..64 {
            saturated.refuse_next(busy());
        }
        let writer = GatewayCapture::local(Arc::clone(&saturated) as _);
        let started = Instant::now();
        let deadline = started + Duration::from_millis(100);
        assert_eq!(
            until_deadline(deadline, |attempt| writer.submit(&batch, attempt)).await,
            Err(CaptureDrop::Saturated)
        );
        assert!(
            Instant::now() <= deadline,
            "no attempt outlives the deadline"
        );
        assert!(saturated.received().is_empty());
        assert!(
            (2..64).contains(&saturated.attempts()),
            "backoff paces the retries: {}",
            saturated.attempts()
        );
    }

    /// Proves a terminal refusal drops at once, a Scribe that never answers
    /// drops as unavailable at the deadline, and a process with no Scribe
    /// drops as unavailable without waiting.
    ///
    /// # Panics
    ///
    /// Panics when any drop, attempt count, or wait differs.
    #[tokio::test(start_paused = true)]
    async fn terminal_parked_and_absent_scribes_drop_without_retry() {
        let capture = CallCapture::from_facts(facts(policy(GatewayCaptureMode::Metadata, &[])))
            .expect("metadata projects");
        let batch = capture.batches().expect("batches encode").remove(0);
        let deadline = Instant::now() + Duration::from_secs(5);

        let refusing = Arc::new(RecordingScribe::default());
        refusing.refuse_next(ScribeError::InvalidFrame);
        let writer = GatewayCapture::local(Arc::clone(&refusing) as _);
        assert_eq!(
            until_deadline(deadline, |attempt| writer.submit(&batch, attempt)).await,
            Err(CaptureDrop::Rejected)
        );
        assert_eq!(
            refusing.attempts(),
            1,
            "a terminal refusal is never retried"
        );

        let parked = Arc::new(RecordingScribe::default());
        parked.park();
        let writer = GatewayCapture::local(Arc::clone(&parked) as _);
        assert_eq!(
            until_deadline(deadline, |attempt| writer.submit(&batch, attempt)).await,
            Err(CaptureDrop::Unavailable)
        );
        assert_eq!(
            Instant::now(),
            deadline,
            "the parked attempt ends at the deadline"
        );

        let absent = GatewayCapture {
            route: CaptureRoute::Unavailable,
        };
        let later = Instant::now() + Duration::from_millis(1);
        assert_eq!(
            until_deadline(later, |attempt| absent.submit(&batch, attempt)).await,
            Err(CaptureDrop::Unavailable)
        );
    }

    /// Proves in-process refusals and peer status codes classify into the
    /// same retryable and terminal drops.
    ///
    /// # Panics
    ///
    /// Panics when a refusal classifies differently.
    #[test]
    fn scribe_refusals_and_peer_codes_classify_alike() {
        let cases = [
            (
                ScribeError::IngestBusy {
                    table: String::new(),
                },
                CaptureDrop::Saturated,
            ),
            (ScribeError::WalDiskFull, CaptureDrop::Saturated),
            (ScribeError::IngressClosed, CaptureDrop::Unavailable),
            (
                ScribeError::Internal {
                    detail: String::new(),
                },
                CaptureDrop::Unavailable,
            ),
            (ScribeError::InvalidFrame, CaptureDrop::Rejected),
        ];
        for (error, drop) in cases {
            assert_eq!(CaptureDrop::from_scribe(&error), drop, "{error}");
            let status = vala_bifrost_redux::gate::IngestError::from_scribe(error).into_status();
            assert_eq!(CaptureDrop::from_code(status.code()), drop, "{status}");
        }
        assert_eq!(
            CaptureDrop::from_code(Code::DeadlineExceeded),
            CaptureDrop::Unavailable
        );
    }
}

/// A recording stand-in for a pod's own Scribe, shared by every capture test.
#[cfg(test)]
pub(crate) mod recording {
    use std::collections::VecDeque;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use arrow::ipc::reader::StreamReader;
    use bytes::Bytes;
    use vala_bifrost_redux::catalog::{TableRef, TableUid};
    use vala_bifrost_redux::contracts::{
        FrameAdmission, IngressPayload, Scribe, ScribeError, ScribeIngressFrame,
    };
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::request_id::RequestId;

    /// One frame the recording Scribe acknowledged.
    #[derive(Debug, Clone)]
    pub(crate) struct Received {
        /// Authenticated tenant of the frame.
        pub(crate) tenant: DataTenantId,
        /// Principal the frame was submitted under.
        pub(crate) principal: PrincipalId,
        /// Fully qualified destination table.
        pub(crate) table: String,
        /// Request the frame was admitted under.
        pub(crate) request_id: RequestId,
        /// Rows the frame's IPC stream carries.
        pub(crate) rows: usize,
        /// The frame's IPC stream.
        pub(crate) ipc: Bytes,
    }

    /// Scribe that records acknowledged frames, refuses as scripted, and can
    /// park every submission so it never answers.
    #[derive(Default)]
    pub(crate) struct RecordingScribe {
        /// Frames acknowledged, in arrival order.
        received: Mutex<Vec<Received>>,
        /// Refusals answered to the next submissions, in order.
        refusals: Mutex<VecDeque<ScribeError>>,
        /// Submissions seen, acknowledged or not.
        attempts: AtomicUsize,
        /// Whether every submission parks forever.
        parked: AtomicBool,
    }

    impl RecordingScribe {
        /// Refuses the next unscripted submission with `error`.
        ///
        /// # Panics
        ///
        /// Panics when the refusal script lock is poisoned.
        pub(crate) fn refuse_next(&self, error: ScribeError) {
            self.refusals.lock().expect("refusals").push_back(error);
        }

        /// Parks every later submission so it never answers.
        pub(crate) fn park(&self) {
            self.parked.store(true, Ordering::Release);
        }

        /// Frames acknowledged so far.
        ///
        /// # Panics
        ///
        /// Panics when the received lock is poisoned.
        pub(crate) fn received(&self) -> Vec<Received> {
            self.received.lock().expect("received").clone()
        }

        /// Submissions seen so far, acknowledged or not.
        pub(crate) fn attempts(&self) -> usize {
            self.attempts.load(Ordering::Acquire)
        }

        /// Rows across every acknowledged frame.
        pub(crate) fn rows(&self) -> usize {
            self.received().iter().map(|frame| frame.rows).sum()
        }

        /// Every acknowledged IPC stream as lossy text, for content checks.
        pub(crate) fn published(&self) -> String {
            self.received()
                .iter()
                .map(|frame| String::from_utf8_lossy(&frame.ipc).into_owned())
                .collect()
        }
    }

    #[async_trait::async_trait]
    impl Scribe for RecordingScribe {
        /// Parks, refuses as scripted, or records `frame` and acknowledges it.
        ///
        /// # Errors
        ///
        /// Returns the next scripted refusal.
        ///
        /// # Panics
        ///
        /// Panics when the frame is not one readable Arrow IPC stream.
        async fn ingest_frame(
            &self,
            frame: ScribeIngressFrame,
        ) -> Result<FrameAdmission, ScribeError> {
            self.attempts.fetch_add(1, Ordering::AcqRel);
            if self.parked.load(Ordering::Acquire) {
                std::future::pending::<()>().await;
            }
            if let Some(error) = self.refusals.lock().expect("refusals").pop_front() {
                return Err(error);
            }
            let IngressPayload::ArrowIpc(ipc) = frame.payload else {
                panic!("capture submits Arrow IPC");
            };
            let rows = StreamReader::try_new(std::io::Cursor::new(ipc.clone()), None)
                .expect("the capture stream opens")
                .map(|batch| batch.expect("the capture batch decodes").num_rows())
                .sum::<usize>();
            self.received.lock().expect("received").push(Received {
                tenant: frame.authenticated_tenant,
                principal: frame.principal.id,
                table: frame.table.fqn(),
                request_id: frame.request_id,
                rows,
                ipc,
            });
            Ok(FrameAdmission {
                batch_id: frame.batch_id,
                rows_accepted: u64::try_from(rows).unwrap_or(u64::MAX),
                receipt_micros: 0,
                first_commit: true,
            })
        }

        /// Capture never resolves a table, so this always refuses.
        ///
        /// # Errors
        ///
        /// Always returns [`ScribeError::Internal`].
        async fn resolve_write_table(
            &self,
            _tenant: DataTenantId,
            _table: &TableRef,
        ) -> Result<TableUid, ScribeError> {
            Err(ScribeError::Internal {
                detail: "the recording Scribe resolves no table".to_owned(),
            })
        }
    }
}
