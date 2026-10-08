//! The process's one Scribe-bound outbox: audit decisions, gateway capture,
//! and Verifier results.
//!
//! Producers stage logical [`ScribeWrite`] values on [`ScribeOutbox`] and
//! return at once. The outbox writer hands each tenant's pending slice to
//! [`ScribeSink`], which projects every write into its fixed destination rows,
//! groups those rows by destination, encodes each group as Arrow IPC frames
//! under content-derived batch ids, and submits the frames in-process to this
//! pod's Scribe or over the peer plane to a live one. Scribe ingress alone
//! admits, accounts, persists, and acknowledges them; nothing here reserves
//! Scribe capacity.
//!
//! A retryable refusal fails the write, so the outbox retries the identical
//! slice and every frame keeps its batch id; Scribe's batch-id dedup absorbs
//! the frames it already accepted. A terminal refusal, a projection failure,
//! or a write for a process that can reach no Scribe is logged, counted in
//! `outbox_events_lost_total{outbox="scribe"}`, and consumed, so later writes
//! for the tenant continue. Work held here is process memory: abrupt process
//! death or the shutdown deadline loses it without changing the operation
//! that produced it.

use std::collections::{BTreeSet, HashMap};
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use arrow::array::RecordBatch;
use arrow::compute::concat_batches;
use arrow::ipc::writer::StreamWriter;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use vala_bifrost_redux::catalog::TableRef;
use vala_bifrost_redux::cluster::ClusterRegistry;
use vala_bifrost_redux::contracts::{IngressPayload, Scribe, ScribeError, ScribeIngressFrame};
use vala_bifrost_redux::gate::limits::BIFROST_INGEST_REQUEST_LIMIT_BYTES;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::oracle::dispatcher::BifrostPeerTls;
use vala_bifrost_redux::tables::audit::projection::project_audit_event;
use vala_bifrost_redux::tables::gateway::CallsTable;
use vala_bifrost_redux::tables::{
    AuditLogTable, DomainTable, ResultFeaturesTable, ResultItemsTable, ResultsTable, SpansTable,
};
use wyrd_runtime::outbox::{Outbox, OutboxSink, count_lost};
use wyrd_runtime::{PermissionSet, Principal, PrincipalKind};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{GATEWAY_CAPTURE_PRINCIPAL, PLATFORM_AUDIT_PRINCIPAL, PrincipalId};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::CardUid;
use wyrd_spec::reference::{CardRef, CardRefScope};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{AuditEvent, NodeId};
use wyrd_tonic::tonic::Code;
use wyrd_tonic::tonic::transport::Channel;
use wyrd_tonic::wyrd::v1::scribe_capture_peer_service_client::ScribeCapturePeerServiceClient;
use wyrd_tonic::wyrd::v1::{IngestCaptureRequest, VerifierResultAttribution};

use crate::components::gateway::CallCapture;
use crate::verification::results::ResultPayload;

/// The process's one non-blocking outbox for every Scribe-bound write.
pub type ScribeOutbox = Outbox<ScribeSink>;

/// Most tenants whose slices are written at once.
const SCRIBE_WRITERS: usize = 4;

/// Largest in-memory size of the rows one frame carries.
///
/// A tenant slice can hold a long outage's backlog, so each destination group
/// is split into frames below Scribe's default request ceiling.
// ponytail: a fixed quarter of the default ingest ceiling; derive it from the
// configured Scribe limit if an operator ever lowers that ceiling below it.
const FRAME_BUDGET_BYTES: usize = BIFROST_INGEST_REQUEST_LIMIT_BYTES / 4;

/// One logical write bound for Scribe, staged by its domain owner.
///
/// Each variant projects into a fixed destination set; [`ScribeSink`] owns
/// the Arrow encoding, grouping, identity, and delivery of those rows.
#[derive(Debug)]
pub enum ScribeWrite {
    /// One authorization decision bound for retained audit history.
    Audit {
        /// The decision.
        event: AuditEvent,
        /// When the boundary decided, captured as the decision is staged.
        decided_at: DateTime<Utc>,
    },
    /// One gateway call's validated analytical record, its objects already
    /// persisted.
    Capture(Box<CallCapture>),
    /// One Verifier judgment's summary and detail rows.
    Result {
        /// The result's rows, details before the summary.
        payload: ResultPayload,
        /// The exact Verifier and SYSTEM principal the rows are written under.
        attribution: VerifierAttribution,
    },
}

impl From<AuditEvent> for ScribeWrite {
    /// Stages one decision, stamped with the instant it is staged, which is
    /// the instant the boundary decided.
    fn from(event: AuditEvent) -> Self {
        Self::Audit {
            event,
            decided_at: Utc::now(),
        }
    }
}

impl From<CallCapture> for ScribeWrite {
    /// Stages one call's capture.
    fn from(capture: CallCapture) -> Self {
        Self::Capture(Box::new(capture))
    }
}

/// One write's rows for one destination, before grouping.
struct Rows<'a> {
    /// Destination table.
    table: ScribeTable,
    /// Verifier attribution of result rows; `None` otherwise.
    attribution: Option<&'a VerifierAttribution>,
    /// Request the rows were admitted under, when the write has one.
    request: Option<&'a RequestId>,
    /// The rows.
    batch: RecordBatch,
}

impl ScribeWrite {
    /// Projects this write into its destination rows, in write order.
    ///
    /// # Errors
    ///
    /// Returns the projection failure's reason when the rows cannot be built.
    fn rows(&self) -> Result<Vec<Rows<'_>>, &'static str> {
        match self {
            Self::Audit { event, decided_at } => Ok(vec![Rows {
                table: ScribeTable::AuditLog,
                attribution: None,
                request: None,
                batch: project_audit_event(event, *decided_at).map_err(|_| "projection")?,
            }]),
            Self::Capture(capture) => Ok(capture
                .batches()
                .map_err(|drop| drop.reason())?
                .into_iter()
                .map(|(table, batch)| Rows {
                    table,
                    attribution: None,
                    request: Some(capture.request_id()),
                    batch,
                })
                .collect()),
            Self::Result {
                payload,
                attribution,
            } => payload
                .batches()
                .iter()
                .map(|batch| {
                    let table = ScribeTable::from_fqn(&batch.table)
                        .filter(|table| table.is_result())
                        .ok_or("projection")?;
                    Ok(Rows {
                        table,
                        attribution: Some(attribution),
                        request: None,
                        batch: batch.batch.clone(),
                    })
                })
                .collect(),
        }
    }
}

/// The destinations the Scribe outbox writes: retained audit history, the two
/// gateway capture tables, and the three Verifier result tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScribeTable {
    /// `vala.system.audit_log`, one row per authorization decision.
    AuditLog,
    /// `vala.gateway.calls`, one row per captured call.
    Calls,
    /// `vala.traces.spans`, one span per captured attempt.
    Spans,
    /// `vala.verification.results`, one canonical summary row per judgment.
    Results,
    /// `vala.drift.result_features`, the Drift result's feature rows.
    DriftFeatures,
    /// `vala.eval.result_items`, the Eval result's task rows.
    EvalItems,
}

impl ScribeTable {
    /// Every destination, in a fixed order.
    const ALL: [Self; 6] = [
        Self::AuditLog,
        Self::Calls,
        Self::Spans,
        Self::Results,
        Self::DriftFeatures,
        Self::EvalItems,
    ];

    /// Fully qualified table name, which the peer RPC carries.
    #[must_use]
    pub const fn fqn(self) -> &'static str {
        match self {
            Self::AuditLog => "vala.system.audit_log",
            Self::Calls => "vala.gateway.calls",
            Self::Spans => "vala.traces.spans",
            Self::Results => "vala.verification.results",
            Self::DriftFeatures => "vala.drift.result_features",
            Self::EvalItems => "vala.eval.result_items",
        }
    }

    /// Resolves a fully qualified name to a destination, refusing any other
    /// table.
    pub(crate) fn from_fqn(fqn: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|table| table.fqn() == fqn)
    }

    /// Whether this destination is a Verifier result table, which is written
    /// only under a [`VerifierAttribution`].
    pub(crate) const fn is_result(self) -> bool {
        matches!(self, Self::Results | Self::DriftFeatures | Self::EvalItems)
    }

    /// Logical Bifrost table this destination names.
    fn table_ref(self) -> TableRef {
        match self {
            Self::AuditLog => TableRef::new(BifrostNamespace::Audit, AuditLogTable::NAME),
            Self::Calls => TableRef::new(BifrostNamespace::Gateway, CallsTable::NAME),
            Self::Spans => TableRef::new(BifrostNamespace::Traces, SpansTable::NAME),
            Self::Results => TableRef::new(BifrostNamespace::Verification, ResultsTable::NAME),
            Self::DriftFeatures => {
                TableRef::new(BifrostNamespace::Drift, ResultFeaturesTable::NAME)
            }
            Self::EvalItems => TableRef::new(BifrostNamespace::Eval, ResultItemsTable::NAME),
        }
    }
}

/// The identity a Verifier result is written under.
///
/// The frame principal built from it is the tenant SYSTEM principal scoped to
/// exactly this Verifier, so Scribe stamps that principal as `principal_id`
/// and the Verifier UID as `card_uid`, and refuses a row whose `card_ref`
/// names another Card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifierAttribution {
    /// The exact Verifier Card, carrying its UID.
    pub verifier: CardRef,
    /// The tenant's SYSTEM principal.
    pub principal: PrincipalId,
}

impl VerifierAttribution {
    /// Projects this attribution onto the peer RPC, the Verifier reference
    /// and its UID as separate fields.
    fn to_wire(&self) -> VerifierResultAttribution {
        VerifierResultAttribution {
            verifier_ref: CardRef {
                uid: None,
                ..self.verifier.clone()
            }
            .to_string(),
            verifier_uid: self
                .verifier
                .uid
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default(),
            principal_id: self.principal.to_string(),
        }
    }

    /// Parses a peer-submitted attribution.
    ///
    /// # Errors
    ///
    /// Returns a static reason when the principal id is malformed, the
    /// reference is not an exact Verifier Card reference, or the UID is
    /// malformed.
    pub(crate) fn from_wire(wire: &VerifierResultAttribution) -> Result<Self, &'static str> {
        let mut verifier = CardRef::from_str(&wire.verifier_ref)
            .map_err(|_| "result verifier_ref is not a Card reference")?;
        if verifier.kind != CardKind::Verifier || verifier.uid.is_some() {
            return Err("result verifier_ref does not name a Verifier version");
        }
        verifier.uid = Some(
            wire.verifier_uid
                .parse::<CardUid>()
                .map_err(|_| "result verifier_uid is not a Card UID")?,
        );
        let principal = wire
            .principal_id
            .parse::<PrincipalId>()
            .map_err(|_| "result principal_id is not a principal id")?;
        Ok(Self {
            verifier,
            principal,
        })
    }
}

/// Why Scribe did not acknowledge one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ScribeRefusal {
    /// Scribe refused the frame for backpressure.
    #[error("scribe saturated")]
    Saturated,
    /// No Scribe acknowledged: none is reachable yet, or it is closed or
    /// failing.
    #[error("scribe unavailable")]
    Unavailable,
    /// Scribe refused the frame terminally, for example as malformed.
    #[error("scribe rejected the frame")]
    Rejected,
}

impl ScribeRefusal {
    /// Closed metric label and diagnostic reason of this refusal.
    const fn reason(self) -> &'static str {
        match self {
            Self::Saturated => "saturated",
            Self::Unavailable => "unavailable",
            Self::Rejected => "rejected",
        }
    }

    /// Classifies an in-process Scribe refusal.
    ///
    /// Backpressure and a full WAL are saturation; a closed, failing, or
    /// object-store-blocked writer is unavailable; anything else refuses this
    /// frame for good.
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

/// One encoded frame bound for one tenant's destination.
///
/// The sink and the peer service both submit through [`Self::into_frame`],
/// so a frame is identical in-process and over the peer plane. A result
/// table's frame carries its [`VerifierAttribution`] and an audit or capture
/// table's carries none; the peer service refuses any other combination.
#[derive(Debug, Clone)]
pub(crate) struct ScribeBatch {
    /// Tenant the rows belong to.
    pub(crate) tenant: DataTenantId,
    /// Destination table.
    pub(crate) table: ScribeTable,
    /// Content-derived identity Scribe deduplicates resubmissions on.
    pub(crate) batch_id: Uuid,
    /// Request the frame is admitted under.
    pub(crate) request_id: RequestId,
    /// The rows as one Arrow IPC stream.
    pub(crate) ipc: Bytes,
    /// Verifier attribution of a result frame; `None` for audit and capture.
    pub(crate) verifier: Option<VerifierAttribution>,
}

impl ScribeBatch {
    /// Builds the Scribe frame for this batch.
    ///
    /// An audit frame is submitted under the platform audit principal, the
    /// one principal Scribe admits for the system owner's audit history; a
    /// capture frame under the reserved capture principal; a result frame
    /// under the tenant SYSTEM principal whose Card scope is exactly the
    /// Verifier, from which Scribe stamps `card_uid`. The principal carries no
    /// permission: all are server-internal writes that evaluate none, and its
    /// id is what Scribe stamps as `principal_id`.
    pub(crate) fn into_frame(self) -> ScribeIngressFrame {
        let (id, kind) = match &self.verifier {
            None if self.table == ScribeTable::AuditLog => {
                (PLATFORM_AUDIT_PRINCIPAL, PrincipalKind::User)
            }
            None => (GATEWAY_CAPTURE_PRINCIPAL, PrincipalKind::User),
            Some(attribution) => (
                attribution.principal,
                PrincipalKind::System {
                    card_ref_scope: CardRefScope::own(&attribution.verifier),
                },
            ),
        };
        ScribeIngressFrame {
            attributed_cards: None,
            principal: Principal::new(id, kind, self.tenant, Vec::new(), PermissionSet::new()),
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

/// Live, ready Scribes reachable over the peer plane, with one reusable
/// channel each.
///
/// The roster is read from cluster membership on every attempt, so a Scribe
/// that joins, leaves, or stops being ready is followed without restart.
struct ScribePeers {
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

impl ScribePeers {
    /// Returns the channel of the Scribe that serves submission `attempt`.
    ///
    /// Ready Scribes are ordered by node id and attempts rotate across them,
    /// so a retry after a refusal is redirected to the next Scribe. A cached
    /// channel is reused while its endpoint matches; otherwise a lazy channel
    /// is built, so a connect failure surfaces on the RPC itself.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeRefusal::Unavailable`] when no Scribe is live and
    /// ready or its endpoint is invalid.
    fn channel(&self, attempt: usize) -> Result<Channel, ScribeRefusal> {
        let snapshot = self.cluster.snapshot();
        let mut scribes = snapshot
            .live_scribes()
            .into_iter()
            .filter(|lease| lease.ready)
            .map(|lease| (lease.key.node_id, lease.address.as_str()))
            .collect::<Vec<_>>();
        if scribes.is_empty() {
            return Err(ScribeRefusal::Unavailable);
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
            .map_err(|_| ScribeRefusal::Unavailable)?
            .connect_lazy();
        channels.insert(node_id, (endpoint.to_owned(), channel.clone()));
        Ok(channel)
    }

    /// Submits `batch` to the Scribe serving `attempt` and waits for its ACK.
    ///
    /// # Errors
    ///
    /// Returns the [`ScribeRefusal`] the peer's status code classifies, or
    /// [`ScribeRefusal::Unavailable`] when no Scribe can be dialed.
    async fn submit(&self, batch: &ScribeBatch, attempt: usize) -> Result<(), ScribeRefusal> {
        let mut client = ScribeCapturePeerServiceClient::new(self.channel(attempt)?);
        client
            .ingest_capture(IngestCaptureRequest {
                tenant_id: batch.tenant.to_string(),
                table: batch.table.fqn().to_owned(),
                batch_id: batch.batch_id.to_string(),
                request_id: batch.request_id.as_str().to_owned(),
                arrow_ipc: batch.ipc.clone(),
                verifier: batch.verifier.as_ref().map(VerifierAttribution::to_wire),
            })
            .await
            .map(|_| ())
            .map_err(|status| ScribeRefusal::from_code(status.code()))
    }
}

/// Where the sink submits.
enum ScribeRoute {
    /// Scribe runs in this pod and acknowledges in-process.
    Local(Arc<dyn Scribe>),
    /// This pod runs no Scribe and submits over the peer plane.
    Peer(ScribePeers),
}

impl ScribeRoute {
    /// Selects the route for this process's Bifrost roles.
    ///
    /// A local Scribe wins; an Oracle-only pod uses its peer-plane identity
    /// and cluster membership; a process with neither reaches no Scribe.
    fn select(
        scribe: Option<&Arc<crate::state::Scribe>>,
        oracle: Option<&Arc<crate::state::Oracle>>,
    ) -> Option<Self> {
        match (scribe, oracle) {
            (Some(scribe), _) => Some(Self::Local(Arc::clone(scribe.scribe()) as Arc<dyn Scribe>)),
            (None, Some(oracle)) => oracle.lifecycle_transport().peer_tls().map(|tls| {
                Self::Peer(ScribePeers {
                    cluster: oracle.cluster(),
                    tls: tls.clone(),
                    channels: Mutex::new(HashMap::new()),
                })
            }),
            (None, None) => None,
        }
    }

    /// Submits `batch` once along this route.
    ///
    /// # Errors
    ///
    /// Returns the classified Scribe refusal.
    async fn submit(&self, batch: &ScribeBatch, attempt: usize) -> Result<(), ScribeRefusal> {
        match self {
            Self::Local(scribe) => scribe
                .ingest_frame(batch.clone().into_frame())
                .await
                .map(|_| ())
                .map_err(|error| ScribeRefusal::from_scribe(&error)),
            Self::Peer(peers) => peers.submit(batch, attempt).await,
        }
    }
}

/// The route slot a [`ScribeSink`] reads, bound once Bifrost is assembled.
///
/// Gate, Oracle, and the peer security audit need the outbox before the
/// Scribe and Oracle runtimes exist, so the outbox starts unbound and the
/// assembled composition binds its route. `None` once bound means this
/// process reaches no Scribe.
type RouteSlot = Arc<OnceLock<Option<ScribeRoute>>>;

/// Binds the route of the process's [`ScribeOutbox`] once Bifrost's roles
/// exist.
pub struct ScribeRouteBinding(RouteSlot);

impl ScribeRouteBinding {
    /// Binds the route this process's roles select; a second bind is ignored.
    pub(crate) fn bind(
        self,
        scribe: Option<&Arc<crate::state::Scribe>>,
        oracle: Option<&Arc<crate::state::Oracle>>,
    ) {
        let _ = self.0.set(ScribeRoute::select(scribe, oracle));
    }
}

/// Whether a process composing `scribe` and `oracle` reaches any Scribe,
/// in-process or over the peer plane.
pub(crate) fn reaches_scribe(
    scribe: Option<&Arc<crate::state::Scribe>>,
    oracle: Option<&Arc<crate::state::Oracle>>,
) -> bool {
    scribe.is_some()
        || oracle.is_some_and(|oracle| oracle.lifecycle_transport().peer_tls().is_some())
}

/// Groups, encodes, identifies, and submits one tenant's logical writes.
pub struct ScribeSink {
    /// Submission route, bound after Bifrost assembly.
    route: RouteSlot,
    /// Submissions so far; rotates peer Scribes across retries.
    attempts: AtomicUsize,
}

impl ScribeSink {
    /// Starts the process outbox with an unbound route, returning it with the
    /// binding Bifrost assembly completes.
    #[must_use]
    pub fn outbox() -> (Arc<ScribeOutbox>, ScribeRouteBinding) {
        let route = RouteSlot::default();
        let outbox = Outbox::new(
            Self {
                route: Arc::clone(&route),
                attempts: AtomicUsize::new(0),
            },
            SCRIBE_WRITERS,
        );
        (outbox, ScribeRouteBinding(route))
    }

    /// Starts an outbox submitting in-process to `scribe`, standing in for a
    /// pod's own Scribe.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn local_outbox(scribe: Arc<dyn Scribe>) -> Arc<ScribeOutbox> {
        Outbox::new(
            Self {
                route: Arc::new(OnceLock::from(Some(ScribeRoute::Local(scribe)))),
                attempts: AtomicUsize::new(0),
            },
            SCRIBE_WRITERS,
        )
    }
}

/// One frame being assembled from consecutive rows of one destination.
struct Frame<'a> {
    /// Destination table.
    table: ScribeTable,
    /// Verifier attribution of result rows.
    attribution: Option<&'a VerifierAttribution>,
    /// Request every row was admitted under, when they share one.
    request: Option<&'a RequestId>,
    /// The rows, all of one schema.
    batches: Vec<RecordBatch>,
    /// In-memory size of `batches`.
    bytes: usize,
    /// Slice positions of the writes contributing rows.
    items: BTreeSet<usize>,
}

impl Frame<'_> {
    /// Whether `rows` share this frame's destination, attribution, request,
    /// and schema, so one Scribe frame can carry both.
    fn shares(&self, rows: &Rows<'_>) -> bool {
        self.table == rows.table
            && self.attribution == rows.attribution
            && self.request == rows.request
            && self.batches[0].schema() == rows.batch.schema()
    }
}

impl ScribeSink {
    /// Groups `items` into frames, in first-seen destination order.
    ///
    /// Rows join the latest frame they share a destination, attribution,
    /// request, and schema with while it stays within [`FRAME_BUDGET_BYTES`],
    /// and otherwise open a new one, so the frames of an identical slice are
    /// identical. Gateway capture carries its admitting request, which Scribe
    /// stamps as `wyrd_request_id`, so captures group per request. Result summary frames are moved after every detail frame,
    /// so a visible summary's details were submitted first. Positions of
    /// writes that cannot be projected are added to `lost`.
    fn frames<'a>(items: &'a [ScribeWrite], lost: &mut BTreeSet<usize>) -> Vec<Frame<'a>> {
        let mut frames: Vec<Frame<'a>> = Vec::new();
        for (index, item) in items.iter().enumerate() {
            let rows = match item.rows() {
                Ok(rows) => rows,
                Err(reason) => {
                    tracing::warn!(
                        outbox = Self::NAME,
                        reason,
                        "a Scribe write could not be projected; it is dropped"
                    );
                    lost.insert(index);
                    continue;
                }
            };
            for rows in rows {
                let size = rows.batch.get_array_memory_size();
                match frames
                    .iter_mut()
                    .rev()
                    .find(|frame| frame.shares(&rows))
                    .filter(|frame| frame.bytes + size <= FRAME_BUDGET_BYTES)
                {
                    Some(frame) => {
                        frame.batches.push(rows.batch);
                        frame.bytes += size;
                        frame.items.insert(index);
                    }
                    None => frames.push(Frame {
                        table: rows.table,
                        attribution: rows.attribution,
                        request: rows.request,
                        batches: vec![rows.batch],
                        bytes: size,
                        items: BTreeSet::from([index]),
                    }),
                }
            }
        }
        frames.sort_by_key(|frame| frame.table == ScribeTable::Results);
        frames
    }

    /// Encodes `frame` as one Arrow IPC stream of one batch under its
    /// content-derived identity.
    ///
    /// The batch id hashes the tenant, destination, attribution, request, and
    /// encoded rows, so a retry of the identical slice resubmits the same
    /// identity and Scribe's batch-id dedup absorbs frames it already
    /// accepted. A frame with no originating request is admitted under a
    /// request id derived from the batch id, so it too is stable across
    /// retries. The digest carries `UUIDv7` version and variant bits.
    ///
    /// # Errors
    ///
    /// Returns the Arrow error when the rows cannot be concatenated or encoded.
    fn encode(
        tenant: DataTenantId,
        frame: &Frame<'_>,
    ) -> Result<ScribeBatch, arrow::error::ArrowError> {
        let schema = frame.batches[0].schema();
        let rows = concat_batches(&schema, &frame.batches)?;
        let mut writer = StreamWriter::try_new(Vec::new(), &schema)?;
        writer.write(&rows)?;
        let ipc = writer.into_inner()?;
        let mut digest = Sha256::new();
        digest.update(b"wyrd.scribe.outbox.batch-id.v1");
        digest.update(tenant.as_uuid().as_bytes());
        digest.update(frame.table.fqn().as_bytes());
        if let Some(attribution) = frame.attribution {
            digest.update(attribution.principal.to_string().as_bytes());
            digest.update(attribution.verifier.to_string().as_bytes());
        }
        if let Some(request) = frame.request {
            digest.update(request.as_str().as_bytes());
        }
        digest.update(&ipc);
        let digest: [u8; 32] = digest.finalize().into();
        let mut bytes = [0_u8; 16];
        bytes.copy_from_slice(&digest[..16]);
        bytes[6] = (bytes[6] & 0x0f) | 0x70;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        let batch_id = Uuid::from_bytes(bytes);
        let request_id = frame.request.cloned().unwrap_or_else(|| {
            RequestId::parse(&batch_id.to_string()).expect("a UUIDv7 batch id is a request id")
        });
        Ok(ScribeBatch {
            tenant,
            table: frame.table,
            batch_id,
            request_id,
            ipc: Bytes::from(ipc),
            verifier: frame.attribution.cloned(),
        })
    }
}

impl OutboxSink for ScribeSink {
    type Item = ScribeWrite;
    type Error = ScribeRefusal;
    const NAME: &'static str = "scribe";

    /// Writes one tenant's slice as destination frames, in order.
    ///
    /// Every frame is submitted even after a terminal refusal; a retryable
    /// refusal stops the write so the outbox retries the identical slice.
    /// Writes that were projected away or whose frame was refused terminally
    /// are counted lost once the slice completes, never on a retried
    /// attempt. A process that reaches no Scribe drops the whole slice.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeRefusal::Unavailable`] while the route is unbound, and
    /// the retryable refusal of the first frame Scribe could not take.
    ///
    /// # Panics
    ///
    /// Panics only if a `UUIDv7` batch id failed to parse as a request id,
    /// which its construction rules out.
    async fn write(
        &self,
        tenant: DataTenantId,
        items: &[ScribeWrite],
    ) -> Result<(), ScribeRefusal> {
        let Some(route) = self.route.get() else {
            return Err(ScribeRefusal::Unavailable);
        };
        let Some(route) = route else {
            count_lost::<Self>(items.len());
            tracing::warn!(outbox = Self::NAME, %tenant, lost = items.len(), "this process reaches no Scribe; writes are dropped");
            return Ok(());
        };
        let mut lost = BTreeSet::new();
        for frame in Self::frames(items, &mut lost) {
            let batch = match Self::encode(tenant, &frame) {
                Ok(batch) => batch,
                Err(error) => {
                    tracing::warn!(outbox = Self::NAME, %tenant, table = frame.table.fqn(), %error, "a Scribe frame could not be encoded; its writes are dropped");
                    lost.extend(&frame.items);
                    continue;
                }
            };
            let attempt = self.attempts.fetch_add(1, Ordering::Relaxed);
            match route.submit(&batch, attempt).await {
                Ok(()) => {}
                Err(ScribeRefusal::Rejected) => {
                    tracing::error!(outbox = Self::NAME, %tenant, table = frame.table.fqn(), batch_id = %batch.batch_id, writes = frame.items.len(), "Scribe rejected a frame terminally; its writes are dropped");
                    lost.extend(&frame.items);
                }
                Err(refusal) => {
                    tracing::warn!(outbox = Self::NAME, %tenant, table = frame.table.fqn(), reason = refusal.reason(), "Scribe refused a frame; the slice will be retried");
                    return Err(refusal);
                }
            }
        }
        count_lost::<Self>(lost.len());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use chrono::Utc;
    use vala_bifrost_redux::contracts::ScribeError;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::{GATEWAY_CAPTURE_PRINCIPAL, PrincipalId};
    use wyrd_spec::gateway::GatewayCaptureMode;
    use wyrd_spec::ids::{CardUid, VerificationResultId};
    use wyrd_spec::reference::CardRef;
    use wyrd_spec::verification::DriftWindow;
    use wyrd_sql::queries::verifier_runs::RunInput;
    use wyrd_tonic::tonic::Code;

    use super::{ScribeOutbox, ScribeRefusal, ScribeSink, ScribeWrite, VerifierAttribution};
    use crate::components::gateway::CallCapture;
    use crate::components::gateway::capture_tests::{facts, policy};
    use crate::components::gateway::recording::RecordingScribe;
    use crate::verification::engines::VerifierReport;
    use crate::verification::results::{ResultPayloadBuilder, ResultRun};

    /// Verifier reference every test result is written by.
    const VERIFIER: &str = "default/Verifier/drift-check@1.0.0";

    /// One validated `Metadata` capture for `tenant`: a call row and one
    /// attempt span under its own admitting request.
    ///
    /// # Panics
    ///
    /// Panics when the fixed facts do not project.
    fn capture(tenant: DataTenantId) -> CallCapture {
        let mut facts = facts(policy(GatewayCaptureMode::Metadata, &[]));
        facts.tenant = tenant;
        CallCapture::from_facts(facts).expect("metadata projects")
    }

    /// The exact Verifier, with its UID, the result attribution names.
    ///
    /// # Panics
    ///
    /// Panics when the fixed reference does not parse.
    fn verifier() -> CardRef {
        let mut verifier: CardRef = VERIFIER.parse().expect("a Verifier reference");
        verifier.uid = Some(CardUid::from_uuid(uuid::Uuid::now_v7()).expect("uid"));
        verifier
    }

    /// One unscored Drift judgment of a fresh subject: its summary row only,
    /// attributed to `attribution`.
    ///
    /// # Panics
    ///
    /// Panics when the payload does not build.
    fn result(attribution: &VerifierAttribution) -> ScribeWrite {
        let subject = CardUid::from_uuid(uuid::Uuid::now_v7()).expect("uid");
        let now = Utc::now();
        let input = RunInput::DriftWindow(DriftWindow {
            start: now - chrono::Duration::hours(1),
            end: now,
        });
        let payload = ResultPayloadBuilder::new(
            ResultRun {
                run_id: None,
                verifier_version: "1.0.0",
                subject_card_uid: &subject,
                owner_card_uid: None,
                binding_id: None,
                trigger: None,
                input: &input,
            },
            VERIFIER,
            VerificationResultId::new_v7(),
            now,
            now,
            now,
        )
        .build(&VerifierReport::Drift(None))
        .expect("an unscored Drift result builds");
        ScribeWrite::Result {
            payload,
            attribution: attribution.clone(),
        }
    }

    /// Waits for everything staged on `outbox` to be written or consumed.
    ///
    /// # Panics
    ///
    /// Panics when items remain after ten seconds.
    async fn settle(outbox: &ScribeOutbox) {
        let deadline = Instant::now() + Duration::from_secs(10);
        assert_eq!(outbox.settle(deadline).await, 0, "the outbox settles");
    }

    /// Proves one tenant slice becomes one frame per destination: three
    /// results by one Verifier share a single summary frame under the scoped
    /// SYSTEM principal, the capture writes its row and span frames under the
    /// capture principal and admitting request, and the summary frame is
    /// submitted last although it was staged first.
    ///
    /// # Panics
    ///
    /// Panics when the frames, their order, rows, or attribution differ.
    #[tokio::test]
    async fn writes_group_by_destination_under_their_attribution() {
        let scribe = Arc::new(RecordingScribe::default());
        let outbox = ScribeSink::local_outbox(Arc::clone(&scribe) as _);
        let tenant = DataTenantId::new_v7();
        let attribution = VerifierAttribution {
            verifier: verifier(),
            principal: PrincipalId::new(uuid::Uuid::now_v7()),
        };
        for _ in 0..3 {
            outbox.stage(tenant, result(&attribution));
        }
        let captured = capture(tenant);
        let request = captured.request_id().clone();
        outbox.stage(tenant, captured);
        settle(&outbox).await;

        let received = scribe.received();
        let frames = received
            .iter()
            .map(|frame| (frame.table.as_str(), frame.rows))
            .collect::<Vec<_>>();
        assert_eq!(
            frames,
            [
                ("vala.gateway.calls", 1),
                ("vala.traces.spans", 1),
                ("vala.verification.results", 3),
            ]
        );
        for frame in &received[..2] {
            assert_eq!(frame.principal, GATEWAY_CAPTURE_PRINCIPAL);
            assert_eq!(frame.request_id, request);
            assert!(frame.card_scope.is_empty());
        }
        assert_eq!(received[2].principal, attribution.principal);
        assert_eq!(received[2].card_scope, [attribution.verifier.clone()]);
        assert!(received.iter().all(|frame| frame.tenant == tenant));
        assert!(
            received
                .iter()
                .all(|frame| frame.batch_id.get_version_num() == 7)
        );
    }

    /// Proves a retryable refusal replays the identical slice: the refused
    /// frame is resubmitted under the same batch id, and only then does the
    /// next frame follow.
    ///
    /// # Panics
    ///
    /// Panics when a resubmission changes identity or a frame is missing.
    #[tokio::test]
    async fn a_retry_resubmits_identical_frames() {
        let scribe = Arc::new(RecordingScribe::default());
        scribe.refuse_next(ScribeError::IngestBusy {
            table: "vala.gateway.calls".to_owned(),
        });
        let outbox = ScribeSink::local_outbox(Arc::clone(&scribe) as _);
        let tenant = DataTenantId::new_v7();
        outbox.stage(tenant, capture(tenant));
        settle(&outbox).await;

        let submitted = scribe.submitted();
        let acknowledged = scribe
            .received()
            .iter()
            .map(|frame| frame.batch_id)
            .collect::<Vec<_>>();
        assert_eq!(submitted.len(), 3, "the refused frame is submitted twice");
        assert_eq!(submitted[0], submitted[1], "the retry keeps its identity");
        assert_eq!(acknowledged, submitted[1..]);
    }

    /// Proves a terminal refusal is consumed: the rejected frame is dropped,
    /// the slice's other frames are still written, and a later write for the
    /// tenant is not blocked behind it.
    ///
    /// # Panics
    ///
    /// Panics when the rejection blocks or retries any write.
    #[tokio::test]
    async fn a_terminal_rejection_never_blocks_later_writes() {
        let scribe = Arc::new(RecordingScribe::default());
        scribe.refuse_next(ScribeError::InvalidFrame);
        let outbox = ScribeSink::local_outbox(Arc::clone(&scribe) as _);
        let tenant = DataTenantId::new_v7();
        outbox.stage(tenant, capture(tenant));
        settle(&outbox).await;
        outbox.stage(tenant, capture(tenant));
        settle(&outbox).await;

        let tables = scribe
            .received()
            .iter()
            .map(|frame| frame.table.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            tables,
            [
                "vala.traces.spans",
                "vala.gateway.calls",
                "vala.traces.spans"
            ]
        );
        assert_eq!(scribe.attempts(), 4, "the rejected frame is never retried");
    }

    /// Proves an unbound route keeps writes pending for retry, and binding it
    /// for a process that reaches no Scribe consumes them.
    ///
    /// # Panics
    ///
    /// Panics when the unbound outbox settles or the bound one does not.
    #[tokio::test]
    async fn an_unbound_route_waits_and_an_unreachable_one_drops() {
        let (outbox, route) = ScribeSink::outbox();
        let tenant = DataTenantId::new_v7();
        outbox.stage(tenant, capture(tenant));
        let soon = Instant::now() + Duration::from_millis(200);
        assert_eq!(outbox.settle(soon).await, 1, "an unbound write waits");
        route.bind(None, None);
        settle(&outbox).await;
    }

    /// Proves in-process refusals and peer status codes classify into the
    /// same retryable and terminal refusals.
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
                ScribeRefusal::Saturated,
            ),
            (ScribeError::WalDiskFull, ScribeRefusal::Saturated),
            (ScribeError::IngressClosed, ScribeRefusal::Unavailable),
            (
                ScribeError::Internal {
                    detail: String::new(),
                },
                ScribeRefusal::Unavailable,
            ),
            (ScribeError::InvalidFrame, ScribeRefusal::Rejected),
        ];
        for (error, refusal) in cases {
            assert_eq!(ScribeRefusal::from_scribe(&error), refusal, "{error}");
            let status = vala_bifrost_redux::gate::IngestError::from_scribe(error).into_status();
            assert_eq!(ScribeRefusal::from_code(status.code()), refusal, "{status}");
        }
        assert_eq!(
            ScribeRefusal::from_code(Code::DeadlineExceeded),
            ScribeRefusal::Unavailable
        );
    }
}
