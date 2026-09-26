//! Opt-in analytical capture of terminal gateway calls into Bifrost.
//!
//! After a call's accounting settles, [`CallCapture`] projects its terminal
//! facts under the policy admitted with the call into one
//! `vala.gateway.calls` row and one linked GenAI span per attempt, applying
//! mandatory secret redaction and binary-content references to any selected
//! payload. [`GatewayCapture`] then hands both batches to the tenant's
//! embedded `wyrd-client` producer with a non-waiting enqueue: the ordinary
//! queue owns encoding, sealing, retry, and publication, so a slow, saturated,
//! or unavailable Bifrost never changes the call. A disabled policy never
//! reaches this module.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use arrow::array::RecordBatch;
use arrow::datatypes::Schema;
use arrow::json::ReaderBuilder;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::{DateTime, TimeDelta, Utc};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use uuid::Uuid;
use vala_bifrost_redux::tables::gateway::CallsTable;
use vala_bifrost_redux::tables::signal::correlation_fields;
use vala_bifrost_redux::tables::traces::project_resource_spans;
use vala_bifrost_redux::tables::{DomainTable, builtin_table};
use wyrd_auth_issue::IssuingKey;
use wyrd_auth_verify::GATEWAY_CAPTURE_TOKEN_MAX_TTL_SECONDS;
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::bifrost::BifrostClientError;
use wyrd_client::config::ClientConfig;
use wyrd_client::error::WyrdClientError;
use wyrd_client::transport::HttpTransport;
use wyrd_client::transport::ResolvedCredential;
use wyrd_client::transport::credential::{AccessTokenSource, MintedAccessToken};
use wyrd_client::{Bifrost as BifrostClient, WyrdClient};
use wyrd_gateway::{AttemptRecord, IngressDialect, MediaRequest, UploadContent};
use wyrd_queue::{QueueConfig, WyrdQueueError};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, SecretBearer};
use wyrd_spec::gateway::{
    GATEWAY_JSON_MAX_BYTES, GatewayAccountingEntryV1, GatewayAttemptSpanFieldsV1, GatewayCallId,
    GatewayCallOutcome, GatewayCallPayloadV1, GatewayCaptureMode, GatewayCapturePolicy,
    GatewayOperation, GatewayPayloadField, GatewayPayloadObjectRefV1, ModelRef,
};
use wyrd_spec::request_id::RequestId;
use wyrd_storage::StorageError;
use wyrd_storage::tenant_path::{self, ValidatedPath};
use wyrd_tonic::otlp::common::v1::any_value::Value as AnyValueKind;
use wyrd_tonic::otlp::common::v1::{AnyValue, InstrumentationScope, KeyValue};
use wyrd_tonic::otlp::resource::v1::Resource;
use wyrd_tonic::otlp::trace::v1::span::SpanKind;
use wyrd_tonic::otlp::trace::v1::status::StatusCode;
use wyrd_tonic::otlp::trace::v1::{ResourceSpans, ScopeSpans, Span, Status};

use crate::state::AppState;

/// Destination of the logical-call row.
const CALLS_TABLE: &str = "vala.gateway.calls";

/// Destination of the linked attempt spans.
const SPANS_TABLE: &str = "vala.traces.spans";

/// Value substituted for a redacted secret.
const REDACTED: &str = "[REDACTED]";

/// Service and instrumentation-scope name of published attempt spans.
const GATEWAY_SERVICE: &str = "wyrd.gateway";

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

/// Why one call's capture was not enqueued; the call itself is unaffected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CaptureDrop {
    /// A selected payload could not be canonicalized within the fixed ceiling,
    /// so it is dropped rather than persisted unreviewed.
    Payload,
    /// The projected record violated its public contract.
    Invalid,
    /// The record could not be assembled into its Arrow batch.
    Projection,
    /// The tenant's capture producer could not be constructed.
    Unavailable,
    /// The bounded queue or byte budget refused the batch.
    Saturated,
    /// The producer refused the batch for another reason, such as shutdown.
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

    /// Classifies a producer refusal as saturation or another rejection.
    fn from_client(error: &BifrostClientError) -> Self {
        match error {
            BifrostClientError::Queue(WyrdQueueError::QueueFull | WyrdQueueError::Backpressure) => {
                Self::Saturated
            }
            _ => Self::Rejected,
        }
    }
}

/// Server-local renewable source of tenant-scoped capture access tokens.
///
/// The embedded client's [`AuthMiddleware`] caches, proactively refreshes,
/// and force-refreshes through it, so capture owns no refresh loop and never
/// persists a credential.
struct CaptureTokenSource {
    /// Server signing key; the only issuer of the reserved capture identity.
    key: Arc<IssuingKey>,
    /// The one tenant every minted token binds.
    tenant: DataTenantId,
    /// Registered UID of the tenant's `vala.gateway.calls` destination.
    calls_uid: Uuid,
    /// Registered UID of the tenant's `vala.traces.spans` destination.
    spans_uid: Uuid,
    /// Stable, non-secret pooling identity naming the tenant.
    identity: String,
}

impl CaptureTokenSource {
    /// Builds the source minting `tenant`'s capture tokens with `key`.
    ///
    /// `calls_uid` and `spans_uid` are the destination UIDs resolved when the
    /// producer is built; every minted token's permissions are scoped to them.
    fn new(key: Arc<IssuingKey>, tenant: DataTenantId, calls_uid: Uuid, spans_uid: Uuid) -> Self {
        Self {
            key,
            tenant,
            calls_uid,
            spans_uid,
            identity: format!("gateway_capture:{tenant}"),
        }
    }
}

impl AccessTokenSource for CaptureTokenSource {
    /// Names the capture identity of this source's tenant.
    fn identity(&self) -> &str {
        &self.identity
    }

    /// Mints one gateway capture access token at the maximum capture lifetime.
    ///
    /// The expiry is read before signing, so the cache refreshes no later than
    /// the token's real expiry.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdClientError::Config`] when the key refuses to issue.
    fn mint(&self) -> Result<MintedAccessToken, WyrdClientError> {
        let ttl =
            TimeDelta::seconds(i64::try_from(GATEWAY_CAPTURE_TOKEN_MAX_TTL_SECONDS).unwrap_or(0));
        let expires_at = Utc::now() + ttl;
        let token = self
            .key
            .issue_gateway_capture_access_token(self.tenant, self.calls_uid, self.spans_uid, ttl)
            .map_err(|error| WyrdClientError::Config {
                field: "gateway_capture.access_token".to_owned(),
                reason: error.to_string(),
            })?;
        Ok(MintedAccessToken {
            access_token: SecretBearer::new(token),
            expires_at,
        })
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
    /// reference, awaiting persistence before the referencing row is enqueued.
    pub(crate) objects: PayloadObjects,
}

/// One call's validated analytical record: its row and attempt spans.
#[derive(Debug)]
pub(crate) struct CallCapture {
    /// Tenant whose producer publishes the record.
    tenant: DataTenantId,
    /// Operation, which names the GenAI spans.
    operation: GatewayOperation,
    /// The `vala.gateway.calls` row.
    payload: GatewayCallPayloadV1,
    /// Bytes of every object the row references, persisted before enqueue.
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
    /// # Errors
    ///
    /// Returns [`CaptureDrop::Projection`] when Arrow cannot decode the row.
    pub(crate) fn calls_batch(&self) -> Result<RecordBatch, CaptureDrop> {
        let mut fields = CallsTable::arrow_fields();
        fields.extend(correlation_fields());
        let mut decoder = ReaderBuilder::new(Arc::new(Schema::new(fields)))
            .build_decoder()
            .map_err(|_| CaptureDrop::Projection)?;
        decoder
            .serialize(std::slice::from_ref(&self.payload))
            .map_err(|_| CaptureDrop::Projection)?;
        decoder
            .flush()
            .ok()
            .flatten()
            .ok_or(CaptureDrop::Projection)
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
        let (batch, outcome) =
            project_resource_spans(&[resource], None).map_err(|_| CaptureDrop::Projection)?;
        if outcome.rejected_spans > 0 {
            return Err(CaptureDrop::Projection);
        }
        Ok(Some(batch))
    }

    /// Enqueues both batches on `producer` without waiting for publication,
    /// each under the call's originating request ID.
    ///
    /// # Errors
    ///
    /// Returns the projection drop of either batch, [`CaptureDrop::Saturated`]
    /// when the bounded queue refuses one, and [`CaptureDrop::Rejected`] for any
    /// other refusal. A refused span batch leaves the row enqueued.
    pub(crate) fn enqueue(&self, producer: &BifrostClient) -> Result<(), CaptureDrop> {
        let calls = self.calls_batch()?;
        let spans = self.spans_batch()?;
        producer
            .enqueue_batch(CALLS_TABLE, calls, Some(self.request_id.clone()))
            .map_err(|error| CaptureDrop::from_client(&error))?;
        if let Some(spans) = spans {
            producer
                .enqueue_batch(SPANS_TABLE, spans, Some(self.request_id.clone()))
                .map_err(|error| CaptureDrop::from_client(&error))?;
        }
        Ok(())
    }
}

/// Pause between shutdown requests while a producer's retained batch awaits
/// its retry.
const CAPTURE_DRAIN_PAUSE: Duration = Duration::from_millis(10);

/// Per-tenant owner of embedded capture producers.
///
/// A producer exists only for a tenant that has captured at least one call,
/// so a disabled tenant creates nothing and readiness never depends on
/// Bifrost.
#[derive(Default)]
pub struct GatewayCapture {
    /// Embedded client of each capturing tenant.
    ///
    /// Shared weakly with each producer's loss observer so a settled loss
    /// republishes the gauges from every producer's ownership. The lock is
    /// never held across an await.
    producers: Arc<ProducerMap>,
    /// Serializes first-use producer construction and shutdown across the
    /// construction's awaited catalog, database, and connect IO.
    construction: tokio::sync::Mutex<()>,
}

/// Embedded capture client of each capturing tenant.
type ProducerMap = std::sync::Mutex<HashMap<DataTenantId, Arc<BifrostClient>>>;

impl GatewayCapture {
    /// Enqueues `capture` on its tenant's producer and records the outcome.
    ///
    /// Never awaits publication: the only awaited work is persisting the
    /// call's referenced objects, first-use producer construction, and the
    /// non-waiting enqueue, and callers run this after the call's answer and
    /// accounting. Every referenced object is written and verified *before*
    /// the row naming it is enqueued, so a published reference always had
    /// bytes behind it; a storage failure drops the whole capture instead.
    /// The converse is deliberate: bytes that persist while enqueue fails are
    /// left for the bucket lifecycle to expire. Every drop is counted and
    /// logged without payload content.
    ///
    /// # Errors
    ///
    /// Returns the [`CaptureDrop`] that prevented enqueue, for tests; the call
    /// is unaffected either way.
    pub(crate) async fn publish(
        &self,
        state: &AppState,
        capture: &CallCapture,
    ) -> Result<(), CaptureDrop> {
        let result = match capture.objects.persist(state, capture.tenant).await {
            Ok(()) => match self.producer(state, capture.tenant).await {
                Ok(producer) => capture.enqueue(&producer),
                Err(drop) => Err(drop),
            },
            Err(drop) => Err(drop),
        };
        Self::record(capture.payload.call_id, result);
        Self::record_backlog(&self.producers);
        result
    }

    /// Counts and logs one capture outcome.
    pub(crate) fn record(call_id: GatewayCallId, result: Result<(), CaptureDrop>) {
        let outcome = result.err().map_or("enqueued", CaptureDrop::reason);
        metrics::counter!("wyrd_gateway_capture_total", "outcome" => outcome).increment(1);
        if outcome != "enqueued" {
            tracing::warn!(
                call_id = %call_id.as_uuid(),
                reason = outcome,
                "gateway call capture dropped; the call is unaffected"
            );
        }
    }

    /// Publishes producer, backlog, and retry gauges across all `producers`.
    ///
    /// Sampling and publishing happen under the map lock, so concurrent
    /// callers publish in the order they sampled and the last one wins with
    /// the newest ownership.
    fn record_backlog(producers: &ProducerMap) {
        let producers = Self::lock(producers);
        Self::record_gauges(producers.len(), producers.values());
    }

    /// Locks `producers`, recovering the map if a holder panicked.
    ///
    /// No holder mutates the map partially, so a poisoned map is still whole.
    fn lock(
        producers: &ProducerMap,
    ) -> std::sync::MutexGuard<'_, HashMap<DataTenantId, Arc<BifrostClient>>> {
        producers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Sets the producer gauge to `producers` and the backlog and retry gauges
    /// to the ownership `clients` still hold.
    fn record_gauges<'a>(producers: usize, clients: impl Iterator<Item = &'a Arc<BifrostClient>>) {
        let (mut bytes, mut retries) = (0_usize, 0_usize);
        for client in clients {
            let metrics = client.metrics();
            bytes += metrics.owned_bytes;
            retries += metrics.retry_entries;
        }
        metrics::gauge!("wyrd_gateway_capture_producers").set(producers as f64);
        metrics::gauge!("wyrd_gateway_capture_backlog_bytes").set(bytes as f64);
        metrics::gauge!("wyrd_gateway_capture_retry_entries").set(retries as f64);
    }

    /// Counts every capture row `producer` settles as lost after admission
    /// and republishes the backlog and retry gauges.
    ///
    /// The queue reports each loss synchronously once the lost owner's bytes
    /// and retry slot are released, so a terminal refusal or unretainable
    /// retry is observable, with settled gauges, without a later call. The
    /// observer holds the map weakly, so it never keeps producers alive.
    fn observe(&self, producer: &BifrostClient) {
        let producers = Arc::downgrade(&self.producers);
        producer.observe_losses(move |rows| {
            if let Some(producers) = producers.upgrade() {
                Self::record_backlog(&producers);
            }
            metrics::counter!("wyrd_gateway_capture_publication_dropped_rows_total")
                .increment(rows);
        });
    }

    /// Number of tenants with a constructed capture producer.
    #[cfg(test)]
    pub(crate) async fn producer_count(&self) -> usize {
        Self::lock(&self.producers).len()
    }

    /// Returns `tenant`'s producer, constructing it on first use.
    ///
    /// Construction ensures both built-in destinations, captures their table
    /// UIDs into the renewable capture credential, and connects an embedded
    /// client from the server's ordinary client configuration and
    /// `WYRD_GRPC_URL`. A failure is retried on the next captured call. The
    /// UIDs are fixed for the producer's life: a destination recreated under a
    /// new UID stays denied until a server restart rebuilds the producer.
    ///
    /// # Errors
    ///
    /// Returns [`CaptureDrop::Saturated`], constructing nothing, when
    /// [`QueueConfig::MAX_LIVE_ENTRIES`] tenants already own producers, and
    /// [`CaptureDrop::Unavailable`] when this replica has no catalog or
    /// signing key, or when a catalog, storage, or client step fails.
    // ponytail: one lock serializes first-use construction across tenants;
    // per-tenant init cells if many tenants enable capture at once.
    async fn producer(
        &self,
        state: &AppState,
        tenant: DataTenantId,
    ) -> Result<Arc<BifrostClient>, CaptureDrop> {
        let _construction = self.construction.lock().await;
        {
            let producers = Self::lock(&self.producers);
            if let Some(producer) = producers.get(&tenant) {
                return Ok(Arc::clone(producer));
            }
            if producers.len() >= QueueConfig::MAX_LIVE_ENTRIES {
                return Err(CaptureDrop::Saturated);
            }
        }
        let producer = Arc::new(Self::connect(state, tenant).await.map_err(|error| {
            tracing::warn!(%tenant, %error, "gateway capture producer unavailable");
            CaptureDrop::Unavailable
        })?);
        self.observe(&producer);
        Self::lock(&self.producers).insert(tenant, Arc::clone(&producer));
        Ok(producer)
    }

    /// Provisions `tenant`'s capture authority and connects its client.
    ///
    /// # Errors
    ///
    /// Returns a redacted description of the failed step.
    async fn connect(state: &AppState, tenant: DataTenantId) -> Result<BifrostClient, String> {
        let catalog = state
            .bifrost
            .catalog()
            .ok_or("this replica has no Bifrost catalog")?;
        let key = state
            .auth
            .issuing_key
            .clone()
            .ok_or("this replica has no signing key")?;
        let mut uids = [Uuid::nil(); 2];
        for (uid, (namespace, name)) in uids
            .iter_mut()
            .zip([("gateway", "calls"), ("traces", "spans")])
        {
            let definition = builtin_table(namespace, name).ok_or("built-in table missing")?;
            let table = catalog
                .ensure_builtin(tenant, definition)
                .await
                .map_err(|error| error.to_string())?;
            *uid = Uuid::from_bytes(*table.as_bytes());
        }
        let config = ClientConfig::from_env();
        let source = CaptureTokenSource::new(key, tenant, uids[0], uids[1]);
        let auth = AuthMiddleware::new(&config, ResolvedCredential::Renewable(Arc::new(source)))
            .map_err(|error| error.to_string())?;
        let http = HttpTransport::new(&config.http, Arc::clone(&auth))
            .map_err(|error| error.to_string())?;
        let client = WyrdClient::from_parts(auth, http, config.grpc);
        BifrostClient::connect(&client)
            .await
            .map_err(|error| error.to_string())
    }

    /// Drains every producer concurrently, publishing accepted capture evidence.
    ///
    /// All producers drain together under the server's one shutdown deadline,
    /// which cancels this future, so a tenant whose publication never settles
    /// cannot keep another tenant from draining. Evidence still buffered when
    /// a producer fails terminally is lost, which the capture contract permits
    /// before Scribe acknowledgement. Once every producer has drained, the
    /// producer, backlog, and retry gauges settle to what the drained
    /// producers still hold.
    pub(crate) async fn shutdown(&self) {
        let producers = {
            let _construction = self.construction.lock().await;
            std::mem::take(&mut *Self::lock(&self.producers))
                .into_values()
                .collect::<Vec<_>>()
        };
        futures_util::future::join_all(producers.iter().map(|producer| Self::drain(producer)))
            .await;
        Self::record_gauges(0, producers.iter());
    }

    /// Shuts `producer` down, asking again while a retained batch awaits its
    /// retry.
    ///
    /// A producer holding a batch whose ambiguous send awaits its retry answers
    /// shutdown with `FlushTimeout` while its background owner keeps that
    /// batch. The loop has no deadline of its own; cancelling the future ends
    /// it. Any other failure is logged and ends the drain.
    async fn drain(producer: &BifrostClient) {
        loop {
            match producer.shutdown().await {
                Ok(()) => break,
                // ponytail: fixed pause, since the queue exposes no retry-settled signal; await one if drain latency matters.
                Err(BifrostClientError::Queue(WyrdQueueError::FlushTimeout)) => {
                    tokio::time::sleep(CAPTURE_DRAIN_PAUSE).await;
                }
                Err(error) => {
                    tracing::warn!(%error, "gateway capture producer did not drain");
                    break;
                }
            }
        }
    }

    /// Installs `producer` for `tenant`, standing in for construction.
    #[cfg(test)]
    pub(crate) async fn install(&self, tenant: DataTenantId, producer: Arc<BifrostClient>) {
        self.observe(&producer);
        Self::lock(&self.producers).insert(tenant, producer);
    }
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
    use secrecy::SecretString;
    use serde_json::json;
    use wyrd_auth_issue::IssuingKey;
    use wyrd_auth_verify::Kid;
    use wyrd_client::WyrdClient;
    use wyrd_client::config::ClientConfig;
    use wyrd_client::transport::credential::AccessTokenSource;
    use wyrd_gateway::{
        AttemptRecord, AttemptUsage, IngressDialect, MediaRequest, OpenAiMediaRoute, UploadContent,
        UploadFile,
    };
    use wyrd_queue::{MockSink, QueueConfig};
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::gateway::{
        GATEWAY_JSON_MAX_BYTES, GatewayCallId, GatewayCallOutcome, GatewayCaptureMode,
        GatewayCapturePolicy, GatewayOperation, GatewayPayloadField, ModelRef,
    };
    use wyrd_spec::ids::ProviderDeploymentName;

    use super::{
        BifrostClient, CallCapture, CallFacts, CaptureDrop, CaptureTokenSource, GatewayCapture,
        PayloadObjects, object_path, request_content,
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

    /// Embedded client whose batches land in `sink` instead of gRPC.
    fn producer(sink: Arc<MockSink>, config: QueueConfig) -> BifrostClient {
        let client = WyrdClient::with_config(ClientConfig {
            credential: Some(SecretString::from("secret")),
            ..ClientConfig::default()
        })
        .expect("client assembles without IO");
        BifrostClient::with_sink(&client, None, sink, config)
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
        for column in ["card_ref", "run_id", "request_payload_json"] {
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

    /// Proves the token source mints per-tenant tokens within the capture
    /// lifetime ceiling.
    #[test]
    fn token_source_mints_bounded_tenant_tokens() {
        let key = IssuingKey::from_ed_pem(
            IssuingKey::generate_ephemeral_pem().expect("pem"),
            Kid::new("k1").expect("kid"),
            "wyrd",
        )
        .expect("key");
        let tenant = DataTenantId::new_v7();
        let source =
            CaptureTokenSource::new(Arc::new(key), tenant, uuid::Uuid::nil(), uuid::Uuid::nil());
        assert_eq!(source.identity(), format!("gateway_capture:{tenant}"));
        let before = Utc::now();
        let minted = source.mint().expect("mints");
        let lifetime = (minted.expires_at - before).num_seconds();
        assert!((899..=900).contains(&lifetime), "{lifetime}");
    }

    /// Proves enqueue hands both batches to the queue without waiting, and a
    /// refused byte budget drops the capture as saturated.
    #[tokio::test]
    async fn enqueue_is_non_waiting_and_saturation_drops() {
        let capture = CallCapture::from_facts(facts(policy(GatewayCaptureMode::Metadata, &[])))
            .expect("metadata projects");

        let sink = Arc::new(MockSink::new());
        let open = producer(Arc::clone(&sink), QueueConfig::default());
        assert_eq!(capture.enqueue(&open), Ok(()));
        open.shutdown().await.expect("producer drains");
        let rows: u64 = sink.received().iter().map(|receipt| receipt.rows).sum();
        assert_eq!(rows, 2, "one call row and one attempt span publish");

        let full = producer(
            Arc::new(MockSink::new()),
            QueueConfig {
                client_byte_limit_bytes: 1,
                ..QueueConfig::default()
            },
        );
        assert_eq!(capture.enqueue(&full), Err(CaptureDrop::Saturated));
    }

    /// Log output of the process-global test subscriber.
    static LOGS: std::sync::Mutex<Vec<u8>> = std::sync::Mutex::new(Vec::new());

    /// Appends formatted log output to [`LOGS`].
    struct LogWriter;

    impl std::io::Write for LogWriter {
        /// Appends `bytes` to [`LOGS`].
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            LOGS.lock().expect("logs").extend_from_slice(bytes);
            Ok(bytes.len())
        }

        /// Nothing is buffered outside [`LOGS`].
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Process-global recorder keeping each counter and gauge cell by name,
    /// since producers report losses from their own runtime threads.
    #[derive(Default)]
    struct NameRecorder(
        std::sync::Mutex<std::collections::HashMap<String, Arc<metrics::atomics::AtomicU64>>>,
    );

    impl NameRecorder {
        /// The cell of metric `name`, created on first use.
        fn cell(&self, name: &str) -> Arc<metrics::atomics::AtomicU64> {
            Arc::clone(
                self.0
                    .lock()
                    .expect("metric cells")
                    .entry(name.to_owned())
                    .or_default(),
            )
        }

        /// The raw value of counter or gauge `name`; a gauge stores `f64` bits.
        fn raw(&self, name: &str) -> u64 {
            self.cell(name).load(std::sync::atomic::Ordering::Acquire)
        }
    }

    impl metrics::Recorder for NameRecorder {
        /// Ignores descriptions.
        fn describe_counter(
            &self,
            _: metrics::KeyName,
            _: Option<metrics::Unit>,
            _: metrics::SharedString,
        ) {
        }
        /// Ignores descriptions.
        fn describe_gauge(
            &self,
            _: metrics::KeyName,
            _: Option<metrics::Unit>,
            _: metrics::SharedString,
        ) {
        }
        /// Ignores descriptions.
        fn describe_histogram(
            &self,
            _: metrics::KeyName,
            _: Option<metrics::Unit>,
            _: metrics::SharedString,
        ) {
        }
        /// Counts into the cell of the counter's name.
        fn register_counter(
            &self,
            key: &metrics::Key,
            _: &metrics::Metadata<'_>,
        ) -> metrics::Counter {
            metrics::Counter::from_arc(self.cell(key.name()))
        }
        /// Stores into the cell of the gauge's name.
        fn register_gauge(&self, key: &metrics::Key, _: &metrics::Metadata<'_>) -> metrics::Gauge {
            metrics::Gauge::from_arc(self.cell(key.name()))
        }
        /// Discards histograms.
        fn register_histogram(
            &self,
            _: &metrics::Key,
            _: &metrics::Metadata<'_>,
        ) -> metrics::Histogram {
            metrics::Histogram::noop()
        }
    }

    /// Proves a capture batch the sink terminally refuses after admission
    /// settles once, with no later call: its row is counted on the gateway
    /// loss counter and the producer, one payload-free warning carries the
    /// admitting request, and shutdown settles the producer, backlog, and
    /// retry gauges.
    ///
    /// Installs the process-global recorder and subscriber, which nextest's
    /// process-per-test isolation keeps to this test.
    ///
    /// # Panics
    ///
    /// Panics when a global is already installed or an assertion differs.
    #[tokio::test]
    async fn terminal_publication_loss_is_counted_logged_and_settled() {
        let recorder: &'static NameRecorder = Box::leak(Box::default());
        metrics::set_global_recorder(recorder).expect("this test process owns the recorder");
        tracing::subscriber::set_global_default(
            tracing_subscriber::fmt()
                .with_writer(|| LogWriter)
                .with_ansi(false)
                .finish(),
        )
        .expect("this test process owns the subscriber");
        let capture = CallCapture::from_facts(facts(policy(GatewayCaptureMode::Metadata, &[])))
            .expect("metadata projects");
        let sink = Arc::new(MockSink::new());
        sink.terminal_next(1);
        let producer = Arc::new(producer(Arc::clone(&sink), QueueConfig::default()));
        let owner = GatewayCapture::default();
        owner.install(capture.tenant, Arc::clone(&producer)).await;
        GatewayCapture::record_backlog(&owner.producers);
        assert_eq!(
            f64::from_bits(recorder.raw("wyrd_gateway_capture_producers")),
            1.0
        );

        assert_eq!(capture.enqueue(&producer), Ok(()));
        let lost = "wyrd_gateway_capture_publication_dropped_rows_total";
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while recorder.raw(lost) == 0 || sink.received().is_empty() {
            assert!(std::time::Instant::now() < deadline, "the refusal settles");
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        assert_eq!(producer.metrics().dropped_rows, 1);

        owner.shutdown().await;
        assert_eq!(
            recorder.raw(lost),
            1,
            "the refused row settles exactly once"
        );
        for gauge in [
            "wyrd_gateway_capture_producers",
            "wyrd_gateway_capture_backlog_bytes",
            "wyrd_gateway_capture_retry_entries",
        ] {
            assert_eq!(recorder.raw(gauge), 0, "{gauge} settles at shutdown");
        }
        let logs = String::from_utf8(LOGS.lock().expect("logs").clone()).expect("utf-8 logs");
        let warnings = logs
            .lines()
            .filter(|line| line.contains("bifrost rows lost"))
            .collect::<Vec<_>>();
        assert_eq!(warnings.len(), 1, "{logs}");
        assert!(
            warnings[0].contains(capture.request_id.as_str())
                && warnings[0].contains("WYRD_SPEC_400_VALIDATION")
                && !warnings[0].contains("acme/a"),
            "{logs}"
        );
    }

    /// Proves a terminal refusal of a fresh batch and of a batch retained for
    /// retry, with no later call and no shutdown, returns the backlog and
    /// retry gauges to the producer's settled ownership as the loss settles.
    ///
    /// Each case first publishes a sample taken while ownership was held, so
    /// only the loss notification can settle the gauges. Installs the
    /// process-global recorder, which nextest's process-per-test isolation
    /// keeps to this test.
    ///
    /// # Panics
    ///
    /// Panics when the recorder is already installed, a loss does not settle,
    /// or a gauge still reports released ownership.
    #[tokio::test]
    async fn terminal_losses_settle_gauges_without_a_later_call() {
        let recorder: &'static NameRecorder = Box::leak(Box::default());
        metrics::set_global_recorder(recorder).expect("this test process owns the recorder");
        let capture = CallCapture::from_facts(facts(policy(GatewayCaptureMode::Metadata, &[])))
            .expect("metadata projects");
        let sink = Arc::new(MockSink::new());
        let producer = Arc::new(producer(Arc::clone(&sink), QueueConfig::default()));
        let owner = GatewayCapture::default();
        owner.install(capture.tenant, Arc::clone(&producer)).await;
        let lost = "wyrd_gateway_capture_publication_dropped_rows_total";

        for (settled, retained) in [(1, false), (2, true)] {
            if retained {
                sink.fail_next(1);
            }
            sink.terminal_next(1);
            metrics::gauge!("wyrd_gateway_capture_backlog_bytes").set(1024.0);
            metrics::gauge!("wyrd_gateway_capture_retry_entries").set(1.0);
            producer
                .enqueue_batch(
                    super::CALLS_TABLE,
                    capture.calls_batch().expect("calls batch"),
                    None,
                )
                .expect("batch admitted");
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while recorder.raw(lost) < settled {
                assert!(std::time::Instant::now() < deadline, "the refusal settles");
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            assert_eq!(recorder.raw(lost), settled, "one loss event per refusal");
            let metrics = producer.metrics();
            assert_eq!((metrics.owned_bytes, metrics.retry_entries), (0, 0));
            for gauge in [
                "wyrd_gateway_capture_backlog_bytes",
                "wyrd_gateway_capture_retry_entries",
            ] {
                assert_eq!(
                    f64::from_bits(recorder.raw(gauge)),
                    0.0,
                    "{gauge} settles with the loss (retained: {retained})"
                );
            }
        }
        assert_eq!(owner.producer_count().await, 1, "no shutdown ran");
    }

    /// Proves shutdown drains tenants concurrently under the one server
    /// deadline: a tenant whose sink never recovers cannot keep a healthy
    /// tenant from publishing and stopping before that deadline.
    #[tokio::test]
    async fn unavailable_tenant_cannot_delay_a_healthy_tenant_drain() {
        let capture = CallCapture::from_facts(facts(policy(GatewayCaptureMode::Metadata, &[])))
            .expect("metadata projects");
        let unavailable_sink = Arc::new(MockSink::new());
        unavailable_sink.fail_next(usize::MAX);
        let unavailable = Arc::new(producer(
            unavailable_sink,
            QueueConfig {
                flush_timeout_ms: 100,
                ..QueueConfig::default()
            },
        ));
        let healthy_sink = Arc::new(MockSink::new());
        let healthy = Arc::new(producer(Arc::clone(&healthy_sink), QueueConfig::default()));
        let owner = GatewayCapture::default();
        owner
            .install(DataTenantId::new_v7(), Arc::clone(&unavailable))
            .await;
        owner.install(capture.tenant, Arc::clone(&healthy)).await;
        assert_eq!(capture.enqueue(&unavailable), Ok(()));
        assert_eq!(capture.enqueue(&healthy), Ok(()));

        let drained =
            tokio::time::timeout(std::time::Duration::from_secs(2), owner.shutdown()).await;
        assert!(drained.is_err(), "the unavailable tenant never drains");
        let rows: u64 = healthy_sink
            .received()
            .iter()
            .map(|receipt| receipt.rows)
            .sum();
        assert_eq!(rows, 2, "the healthy tenant publishes before the deadline");
        assert!(
            capture.enqueue(&healthy).is_err(),
            "the healthy tenant's producer stopped before the deadline"
        );
    }

    /// Proves shutdown publishes capture evidence accepted before it,
    /// retrying a transient publication failure rather than dropping rows.
    #[tokio::test]
    async fn shutdown_publishes_accepted_evidence_through_retry() {
        let capture = CallCapture::from_facts(facts(policy(GatewayCaptureMode::Metadata, &[])))
            .expect("metadata projects");
        let sink = Arc::new(MockSink::new());
        sink.fail_next(1);
        let producer = Arc::new(producer(Arc::clone(&sink), QueueConfig::default()));
        let owner = GatewayCapture::default();
        owner.install(capture.tenant, Arc::clone(&producer)).await;
        assert_eq!(capture.enqueue(&producer), Ok(()));

        owner.shutdown().await;
        let rows: u64 = sink.received().iter().map(|receipt| receipt.rows).sum();
        assert_eq!(rows, 2, "accepted row and span publish during shutdown");
        assert!(
            sink.attempted().len() > sink.received().len(),
            "the failed send was retried"
        );
        assert_eq!(producer.metrics().dropped_rows, 0);
        assert_eq!(
            owner.producer_count().await,
            0,
            "shutdown releases producers"
        );
    }
}
