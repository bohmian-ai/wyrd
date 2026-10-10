//! The process's one Scribe-bound outbox: audit decisions, gateway capture,
//! and Verifier results.
//!
//! Producers stage logical [`ScribeWrite`] values on [`ScribeOutbox`] and
//! return at once. The outbox writer hands each tenant's pending slice to
//! [`ScribeSink`], which projects every write into its fixed destination rows,
//! groups those rows by destination, encodes each group as Arrow IPC frames
//! under content-derived batch ids, splitting any frame larger than a quarter
//! of the configured Scribe request ceiling by rows, and submits the frames
//! in-process to this pod's Scribe or over the peer plane to a live one. A
//! peer call that has not answered within [`PEER_SUBMIT_TIMEOUT`] fails as
//! unavailable, so the retry rotates to the next ready Scribe. Scribe ingress alone
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
use std::sync::{Arc, Mutex};
use std::time::Duration;

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
use vala_bifrost_redux::gate::attribution::native_card_uids;
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

use crate::components::gateway::{CallCapture, CaptureDrop};
use crate::verification::results::ResultPayload;

/// The process's one non-blocking outbox for every Scribe-bound write.
pub type ScribeOutbox = Outbox<ScribeSink>;

/// Most tenants whose slices are written at once.
const SCRIBE_WRITERS: usize = 4;

/// Longest a peer Scribe may take to acknowledge one frame.
///
/// Matches the other Bifrost peer call budgets. Expiry surfaces as
/// `DeadlineExceeded`, which classifies as [`ScribeRefusal::Unavailable`].
const PEER_SUBMIT_TIMEOUT: Duration = Duration::from_secs(30);

/// Frame budget for a Scribe request ceiling of `request_bytes`.
///
/// A tenant slice can hold a long outage's backlog, so each frame's rows, in
/// memory and as encoded Arrow IPC, stay within a quarter of the ceiling. The
/// quarter leaves room for IPC framing and keeps Scribe's decoded and expanded
/// request bounds, which scale from the same ceiling, out of reach.
const fn frame_budget(request_bytes: usize) -> usize {
    request_bytes / 4
}

/// One logical write bound for Scribe, staged by its domain owner.
///
/// Each variant projects into a fixed destination set; [`ScribeSink`] owns
/// the Arrow encoding, grouping, identity, and delivery of those rows.
#[derive(Debug)]
pub enum ScribeWrite {
    /// One authorization decision bound for retained audit history.
    Audit {
        /// The decision, boxed so the enum stays as small as its pointer
        /// variants.
        event: Box<AuditEvent>,
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
            event: Box::new(event),
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
                .map_err(CaptureDrop::reason)?
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
/// and the Verifier UID as `card_uid`, and refuses a row whose `card_uid`
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
    /// or `Internal`. A call that outlived its channel deadline arrives as
    /// `Cancelled` from tonic's client timeout or `DeadlineExceeded` from the
    /// peer, and is retried like an unreachable peer.
    fn from_code(code: Code) -> Self {
        match code {
            Code::ResourceExhausted => Self::Saturated,
            Code::Unavailable
            | Code::Internal
            | Code::DeadlineExceeded
            | Code::Cancelled
            | Code::Unknown => Self::Unavailable,
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
    /// capture frame under the reserved capture principal, attributed to the
    /// UID-bearing Cards its rows name, which the gateway authorized against
    /// the caller before the call; a result frame
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
        let attributed_cards = match self.verifier {
            None => Some(native_card_uids(&self.ipc)).filter(|uids| !uids.is_empty()),
            Some(_) => None,
        };
        ScribeIngressFrame {
            attributed_cards,
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
pub(crate) struct ScribePeers {
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
    /// Deadline of every call on a channel built here;
    /// [`PEER_SUBMIT_TIMEOUT`] outside tests.
    timeout: Duration,
}

impl ScribePeers {
    /// Returns the channel of the Scribe that serves write `attempt`.
    ///
    /// Ready Scribes are ordered by node id and attempts rotate across them,
    /// so a retry after a refusal is redirected to the next Scribe. A cached
    /// channel is reused while its endpoint matches; otherwise a lazy channel
    /// is built, so a connect failure surfaces on the RPC itself. Every call
    /// on the channel carries [`Self::timeout`], so a silent peer cannot hold
    /// a tenant's slice.
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
            .timeout(self.timeout)
            .connect_lazy();
        channels.insert(node_id, (endpoint.to_owned(), channel.clone()));
        Ok(channel)
    }

    /// Submits `batch` to the Scribe serving `attempt` and waits for its ACK.
    ///
    /// # Errors
    ///
    /// Returns the [`ScribeRefusal`] the peer's status code classifies, or
    /// [`ScribeRefusal::Unavailable`] when no Scribe can be dialed or it did
    /// not answer before the channel's deadline.
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
pub(crate) enum ScribeRoute {
    /// Scribe runs in this pod and acknowledges in-process.
    Local(Arc<dyn Scribe>),
    /// This pod runs no Scribe and submits over the peer plane.
    Peer(ScribePeers),
}

impl ScribeRoute {
    /// Selects the route from this process's own dependencies.
    ///
    /// A local Scribe wins. Otherwise a pod with a peer-plane identity
    /// submits to the ready Scribes in `cluster` membership, whatever other
    /// roles it runs. A process with neither reaches no Scribe.
    pub(crate) fn select(
        local: Option<&Arc<vala_bifrost_redux::scribe::ScribeImpl>>,
        cluster: &Arc<ClusterRegistry>,
        tls: Option<&BifrostPeerTls>,
    ) -> Option<Self> {
        match (local, tls) {
            (Some(scribe), _) => Some(Self::Local(Arc::clone(scribe) as Arc<dyn Scribe>)),
            (None, Some(tls)) => Some(Self::Peer(ScribePeers {
                cluster: Arc::clone(cluster),
                tls: tls.clone(),
                channels: Mutex::new(HashMap::new()),
                timeout: PEER_SUBMIT_TIMEOUT,
            })),
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

/// Groups, encodes, identifies, and submits one tenant's logical writes.
pub struct ScribeSink {
    /// Submission route selected at boot; `None` reaches no Scribe.
    route: Option<ScribeRoute>,
    /// Write attempts so far; every frame of one attempt goes to the same
    /// peer Scribe, and a retry rotates to the next.
    attempts: AtomicUsize,
    /// Largest rows, in memory and encoded, one frame carries; see
    /// [`frame_budget`].
    frame_budget: usize,
}

impl ScribeSink {
    /// Starts an outbox submitting along `route`, or dropping every write
    /// when `route` is `None`.
    ///
    /// `request_bytes` is the deployment's configured Scribe request ceiling
    /// (`scribe.ingest_request_bytes`), which every Scribe a frame can reach
    /// enforces.
    #[must_use]
    pub(crate) fn outbox(route: Option<ScribeRoute>, request_bytes: usize) -> Arc<ScribeOutbox> {
        Outbox::new(
            Self {
                route,
                attempts: AtomicUsize::new(0),
                frame_budget: frame_budget(request_bytes),
            },
            SCRIBE_WRITERS,
        )
    }

    /// Starts an outbox submitting in-process to `scribe`, standing in for a
    /// pod's own Scribe at the default request ceiling.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn local_outbox(scribe: Arc<dyn Scribe>) -> Arc<ScribeOutbox> {
        Self::outbox(
            Some(ScribeRoute::Local(scribe)),
            vala_bifrost_redux::gate::limits::BIFROST_INGEST_REQUEST_LIMIT_BYTES,
        )
    }

    /// Whether this sink reaches any Scribe, in-process or over the peer plane.
    #[must_use]
    pub const fn reaches_scribe(&self) -> bool {
        self.route.is_some()
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
    /// request, and schema with while it stays within `budget` bytes in
    /// memory, and otherwise open a new one, so the frames of an identical slice are
    /// identical. Gateway capture carries its admitting request, which Scribe
    /// stamps as `wyrd_request_id`, so captures group per request. Result summary frames are moved after every detail frame,
    /// so a visible summary's details were submitted first. Positions of
    /// writes that cannot be projected are added to `lost`.
    fn frames<'a>(
        items: &'a [ScribeWrite],
        budget: usize,
        lost: &mut BTreeSet<usize>,
    ) -> Vec<Frame<'a>> {
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
                    .filter(|frame| frame.bytes + size <= budget)
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

    /// Encodes `frame` as Arrow IPC streams of at most `budget` bytes, each
    /// under its content-derived identity, in row order.
    ///
    /// A single row larger than `budget` is still encoded alone; Scribe
    /// decides whether its ceiling admits it.
    ///
    /// # Errors
    ///
    /// Returns the Arrow error when the rows cannot be concatenated or encoded.
    fn encode(
        tenant: DataTenantId,
        frame: &Frame<'_>,
        budget: usize,
    ) -> Result<Vec<ScribeBatch>, arrow::error::ArrowError> {
        let rows = concat_batches(&frame.batches[0].schema(), &frame.batches)?;
        let mut streams = Vec::new();
        Self::split(&rows, budget, &mut streams)?;
        Ok(streams
            .into_iter()
            .map(|ipc| Self::identify(tenant, frame, ipc))
            .collect())
    }

    /// Appends `rows` to `streams` as one Arrow IPC stream, or, when that
    /// stream exceeds `budget` bytes and holds more than one row, as the
    /// streams of each half in turn.
    ///
    /// Halving depends only on the rows, so an identical slice always splits
    /// into identical streams, whatever its row sizes.
    ///
    /// # Errors
    ///
    /// Returns the Arrow error when the rows cannot be encoded.
    fn split(
        rows: &RecordBatch,
        budget: usize,
        streams: &mut Vec<Vec<u8>>,
    ) -> Result<(), arrow::error::ArrowError> {
        let mut writer = StreamWriter::try_new(Vec::new(), &rows.schema())?;
        writer.write(rows)?;
        let ipc = writer.into_inner()?;
        let count = rows.num_rows();
        if ipc.len() <= budget || count < 2 {
            streams.push(ipc);
            return Ok(());
        }
        Self::split(&rows.slice(0, count / 2), budget, streams)?;
        Self::split(&rows.slice(count / 2, count - count / 2), budget, streams)
    }

    /// Wraps one encoded stream of `frame` under its content-derived identity.
    ///
    /// The batch id hashes the tenant, destination, attribution, request, and
    /// encoded rows, so a retry of the identical slice resubmits the same
    /// identity and Scribe's batch-id dedup absorbs frames it already
    /// accepted. A frame with no originating request is admitted under a
    /// request id derived from the batch id, so it too is stable across
    /// retries. The digest carries `UUIDv7` version and variant bits.
    ///
    /// # Panics
    ///
    /// Panics only if a `UUIDv7` batch id failed to parse as a request id,
    /// which its construction rules out.
    fn identify(tenant: DataTenantId, frame: &Frame<'_>, ipc: Vec<u8>) -> ScribeBatch {
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
        ScribeBatch {
            tenant,
            table: frame.table,
            batch_id,
            request_id,
            ipc: Bytes::from(ipc),
            verifier: frame.attribution.cloned(),
        }
    }
}

impl OutboxSink for ScribeSink {
    type Item = ScribeWrite;
    type Error = ScribeRefusal;
    const NAME: &'static str = "scribe";

    /// Writes one tenant's slice as destination frames, in order.
    ///
    /// Every frame of this attempt is submitted to the same Scribe, so a
    /// retry after a peer refusal or timeout moves the whole slice to the
    /// next ready one. A frame over the budget is submitted as its row-split
    /// streams. Every
    /// frame is submitted even after a terminal refusal; a retryable refusal
    /// stops the write so the outbox retries the identical slice.
    /// Writes that were projected away or whose frame was refused terminally
    /// are counted lost once the slice completes, never on a retried
    /// attempt. A process that reaches no Scribe drops the whole slice.
    ///
    /// # Errors
    ///
    /// Returns the retryable refusal of the first frame Scribe could not take.
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
        let Some(route) = &self.route else {
            count_lost::<Self>(items.len());
            tracing::warn!(outbox = Self::NAME, %tenant, lost = items.len(), "this process reaches no Scribe; writes are dropped");
            return Ok(());
        };
        let attempt = self.attempts.fetch_add(1, Ordering::Relaxed);
        let mut lost = BTreeSet::new();
        for frame in Self::frames(items, self.frame_budget, &mut lost) {
            let batches = match Self::encode(tenant, &frame, self.frame_budget) {
                Ok(batches) => batches,
                Err(error) => {
                    tracing::warn!(outbox = Self::NAME, %tenant, table = frame.table.fqn(), %error, "a Scribe frame could not be encoded; its writes are dropped");
                    lost.extend(&frame.items);
                    continue;
                }
            };
            for batch in batches {
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
        }
        count_lost::<Self>(lost.len());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use arrow::array::{Array as _, AsArray as _};
    use arrow::ipc::reader::StreamReader;
    use chrono::Utc;
    use vala_bifrost_redux::catalog::{TableRef, TableUid};
    use vala_bifrost_redux::cluster::{ClusterRegistry, ClusterSnapshot};
    use vala_bifrost_redux::contracts::{FrameAdmission, Scribe, ScribeError, ScribeIngressFrame};
    use vala_bifrost_redux::gate::limits::BIFROST_INGEST_REQUEST_LIMIT_BYTES;
    use vala_bifrost_redux::oracle::dispatcher::BifrostPeerTls;
    use vala_eval::executor::{EvalReport, TaskRunOutcome};
    use wyrd_runtime::audit::AuditStage;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::{
        GATEWAY_CAPTURE_PRINCIPAL, PLATFORM_AUDIT_PRINCIPAL, PrincipalId, PrincipalKindTag,
    };
    use wyrd_spec::gateway::GatewayCaptureMode;
    use wyrd_spec::ids::{CardUid, VerificationResultId};
    use wyrd_spec::reference::CardRef;
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{
        AuditEvent, AuditOutcome, ClusterCapabilities, ClusterNodeKey, ClusterRole,
        ClusterRoleLease, NodeId, ScribeCapabilitiesV1,
    };
    use wyrd_spec::vala::eval::ids::TaskId;
    use wyrd_spec::vala::eval::operator::ComparisonOperator;
    use wyrd_spec::vala::eval::result::AssertionResult;
    use wyrd_spec::verification::{DriftWindow, VerificationVerdict};
    use wyrd_sql::queries::verifier_runs::RunInput;
    use wyrd_testing::bifrost::peer_ca::BifrostPeerCa;
    use wyrd_tonic::tonic::{Code, Request, Response, Status};
    use wyrd_tonic::wyrd::v1::scribe_capture_peer_service_server::{
        ScribeCapturePeerService, ScribeCapturePeerServiceServer,
    };
    use wyrd_tonic::wyrd::v1::{IngestCaptureRequest, IngestCaptureResponse};

    use super::{
        Outbox, SCRIBE_WRITERS, ScribeOutbox, ScribePeers, ScribeRefusal, ScribeRoute, ScribeSink,
        ScribeWrite, VerifierAttribution, frame_budget,
    };
    use crate::components::gateway::CallCapture;
    use crate::components::gateway::capture_tests::{facts, policy};
    use crate::components::gateway::recording::{Received, RecordingScribe};
    use crate::grpc::capture_peer::ScribeCapturePeerGrpc;
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
            attribution
                .verifier
                .uid
                .as_ref()
                .expect("the fixture Verifier carries its UID"),
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
        assert_eq!(received[2].card_scope, attribution.verifier.uid.as_slice());
        assert!(received.iter().all(|frame| frame.tenant == tenant));
        assert!(
            received
                .iter()
                .all(|frame| frame.batch_id.get_version_num() == 7)
        );
    }

    /// One allowed decision for `operation`, attributed to a fresh principal.
    fn decision(operation: &str) -> AuditEvent {
        AuditEvent::new(
            RequestId::now_v7(),
            None,
            operation.to_owned(),
            "bifrost".to_owned(),
            None,
            PrincipalId::new(uuid::Uuid::now_v7()),
            PrincipalKindTag::User,
            operation.to_owned(),
            AuditOutcome::Allowed,
        )
    }

    /// Proves audit decisions stage through the [`AuditStage`] seam and frame
    /// like every other write: each tenant slice's decisions share one
    /// `vala.system.audit_log` frame under the platform audit principal with
    /// no Card scope, the reserved system owner's decisions travel in their
    /// own frame, and every frame carries a v7 content identity.
    ///
    /// # Panics
    ///
    /// Panics when the frames, their rows, or attribution differ.
    #[tokio::test]
    async fn audit_decisions_frame_under_the_platform_audit_principal() {
        let scribe = Arc::new(RecordingScribe::default());
        let outbox = ScribeSink::local_outbox(Arc::clone(&scribe) as _);
        let audit: &dyn AuditStage = &*outbox;
        let tenant = DataTenantId::new_v7();
        audit.stage(tenant, decision("bifrost.query.read_decision"));
        audit.stage(tenant, decision("bifrost.record.write"));
        audit.stage(DataTenantId::SYSTEM_OWNER, decision("platform.authorize"));
        settle(&outbox).await;

        let mut frames = scribe
            .audit_frames()
            .into_iter()
            .map(|frame| {
                assert_eq!(frame.table, "vala.system.audit_log");
                assert_eq!(frame.principal, PLATFORM_AUDIT_PRINCIPAL);
                assert!(frame.card_scope.is_empty());
                assert_eq!(frame.batch_id.get_version_num(), 7);
                (frame.tenant, frame.rows)
            })
            .collect::<Vec<_>>();
        frames.sort_unstable();
        let mut expected = vec![(tenant, 2), (DataTenantId::SYSTEM_OWNER, 1)];
        expected.sort_unstable();
        assert_eq!(frames, expected);
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

    /// Proves a process that reaches no Scribe consumes its writes.
    ///
    /// # Panics
    ///
    /// Panics when the outbox claims a route or does not settle.
    #[tokio::test]
    async fn an_unreachable_route_drops() {
        let outbox = ScribeSink::outbox(None, BIFROST_INGEST_REQUEST_LIMIT_BYTES);
        assert!(!outbox.sink().reaches_scribe());
        let tenant = DataTenantId::new_v7();
        outbox.stage(tenant, capture(tenant));
        settle(&outbox).await;
    }

    /// Proves a pod without a local Scribe routes over the peer plane from
    /// membership and its peer identity alone, with no Oracle runtime, and
    /// keeps writes pending while membership names no ready Scribe.
    ///
    /// # Panics
    ///
    /// Panics when no peer route is selected or a write settles without a
    /// Scribe to take it.
    #[tokio::test]
    async fn a_peer_route_needs_no_oracle() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://unused.invalid/none")
            .expect("lazy pool builds without connecting");
        let cluster = Arc::new(ClusterRegistry::new(
            vala_sql::postgres::ValaPostgres::from_pool(pool),
            NodeId::new(uuid::Uuid::now_v7()),
        ));
        let tls = BifrostPeerTls::new(
            Vec::new(),
            "bifrost-peer".to_owned(),
            Vec::new(),
            secrecy::SecretString::from(String::new()),
        );
        let route = ScribeRoute::select(None, &cluster, Some(&tls));
        assert!(matches!(route, Some(ScribeRoute::Peer(_))));
        let outbox = ScribeSink::outbox(route, BIFROST_INGEST_REQUEST_LIMIT_BYTES);
        assert!(outbox.sink().reaches_scribe());
        let tenant = DataTenantId::new_v7();
        outbox.stage(tenant, capture(tenant));
        let soon = Instant::now() + Duration::from_millis(200);
        assert_eq!(
            outbox.settle(soon).await,
            1,
            "a write waits for a ready Scribe"
        );
    }

    /// A Scribe request ceiling well below the default, so a modest Eval
    /// detail payload needs several frames.
    const SMALL_REQUEST_BYTES: usize = 64 * 1024;

    /// Starts an outbox submitting along `route` under a `request_bytes`
    /// Scribe ceiling.
    fn route_outbox(route: ScribeRoute, request_bytes: usize) -> Arc<ScribeOutbox> {
        Outbox::new(
            ScribeSink {
                route: Some(route),
                attempts: AtomicUsize::new(0),
                frame_budget: frame_budget(request_bytes),
            },
            SCRIBE_WRITERS,
        )
    }

    /// One Eval judgment with `tasks` passed items, task ids `t0`, `t1`, ...,
    /// each carrying an expected value of `expected_bytes` characters.
    ///
    /// # Panics
    ///
    /// Panics when the payload does not build.
    fn eval_result(
        attribution: &VerifierAttribution,
        tasks: usize,
        expected_bytes: usize,
    ) -> ScribeWrite {
        let subject = CardUid::from_uuid(uuid::Uuid::now_v7()).expect("uid");
        let now = Utc::now();
        let input = RunInput::EvalRecord {
            record_id: "record-1".to_owned(),
            event_time: now,
        };
        let outcomes = (0..tasks)
            .map(|task| {
                TaskRunOutcome::Ran(Box::new(AssertionResult {
                    task_id: TaskId::new(format!("t{task}")).expect("task id"),
                    passed: true,
                    actual: None,
                    expected: serde_json::Value::String("x".repeat(expected_bytes)),
                    operator: ComparisonOperator::Equals,
                    message: None,
                    stage: 0,
                    started_at: now,
                    duration_ms: 1,
                }))
            })
            .collect();
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
            attribution
                .verifier
                .uid
                .as_ref()
                .expect("the fixture Verifier carries its UID"),
            VerificationResultId::new_v7(),
            now,
            now,
            now,
        )
        .build(&VerifierReport::Eval {
            report: EvalReport { outcomes },
            verdict: VerificationVerdict::Passed,
            run_id: None,
        })
        .expect("an Eval result builds");
        ScribeWrite::Result {
            payload,
            attribution: attribution.clone(),
        }
    }

    /// A fresh attribution under the test Verifier.
    fn attribution() -> VerifierAttribution {
        VerifierAttribution {
            verifier: verifier(),
            principal: PrincipalId::new(uuid::Uuid::now_v7()),
        }
    }

    /// Each distinct acknowledged frame once, in first-acknowledged order,
    /// as Scribe's batch-id dedup would retain them.
    fn retained(received: Vec<Received>) -> Vec<Received> {
        let mut seen = HashSet::new();
        received
            .into_iter()
            .filter(|frame| seen.insert(frame.batch_id))
            .collect()
    }

    /// The task ids of every Eval item row in `frames`, in order.
    ///
    /// # Panics
    ///
    /// Panics when an item frame does not decode.
    fn item_task_ids(frames: &[Received]) -> Vec<String> {
        frames
            .iter()
            .filter(|frame| frame.table == "vala.eval.result_items")
            .flat_map(|frame| {
                StreamReader::try_new(std::io::Cursor::new(frame.ipc.clone()), None)
                    .expect("an item frame opens")
                    .flat_map(|batch| {
                        let batch = batch.expect("an item batch decodes");
                        let ids = batch
                            .column_by_name("task_id")
                            .expect("task_id column")
                            .as_string::<i32>()
                            .clone();
                        (0..ids.len())
                            .map(|row| ids.value(row).to_owned())
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Asserts `frames` carry every one of `tasks` item rows exactly once and
    /// in order, in more than one item frame within the small ceiling's
    /// budget, and end with the one summary.
    ///
    /// # Panics
    ///
    /// Panics when a row is missing, repeated, misordered, or a frame is over
    /// budget or out of order.
    fn assert_bounded_detail(frames: &[Received], tasks: usize) {
        let tables = frames
            .iter()
            .map(|frame| frame.table.as_str())
            .collect::<Vec<_>>();
        let (summary, items) = tables.split_last().expect("frames were written");
        assert_eq!(*summary, "vala.verification.results", "the summary is last");
        assert!(
            items.len() > 1 && items.iter().all(|table| *table == "vala.eval.result_items"),
            "the detail splits into several item frames before the summary: {tables:?}"
        );
        let budget = frame_budget(SMALL_REQUEST_BYTES);
        assert!(
            frames.iter().all(|frame| frame.ipc.len() <= budget),
            "every frame fits the budget"
        );
        let expected = (0..tasks)
            .map(|task| format!("t{task}"))
            .collect::<Vec<_>>();
        assert_eq!(item_task_ids(frames), expected, "every row once, in order");
    }

    /// A Scribe that acknowledges through `inner` but reports the
    /// `lose`-th submission (zero-based) as failed, as when its ACK is lost.
    struct LostAck {
        /// Records every acknowledged frame.
        inner: Arc<RecordingScribe>,
        /// Submission whose ACK is lost.
        lose: usize,
        /// Submissions so far.
        seen: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl Scribe for LostAck {
        /// Always ready.
        fn is_ready(&self) -> bool {
            true
        }

        /// Acknowledges `frame` through the recording Scribe, then reports
        /// the scripted submission as unavailable.
        ///
        /// # Errors
        ///
        /// Returns [`ScribeError::IngressClosed`] for the lost submission and
        /// the recording Scribe's refusal otherwise.
        async fn ingest_frame(
            &self,
            frame: ScribeIngressFrame,
        ) -> Result<FrameAdmission, ScribeError> {
            let admission = self.inner.ingest_frame(frame).await?;
            if self.seen.fetch_add(1, Ordering::Relaxed) == self.lose {
                return Err(ScribeError::IngressClosed);
            }
            Ok(admission)
        }

        /// Resolves through the recording Scribe.
        ///
        /// # Errors
        ///
        /// Returns the recording Scribe's resolution error.
        async fn resolve_write_table(
            &self,
            tenant: DataTenantId,
            table: &TableRef,
        ) -> Result<TableUid, ScribeError> {
            self.inner.resolve_write_table(tenant, table).await
        }
    }

    /// Proves a multi-row Eval detail larger than one frame under a lowered
    /// Scribe ceiling reaches the local Scribe as bounded frames carrying
    /// every row once, in order, before the summary, and that after a lost
    /// ACK the identical slice resubmits identical frame ids.
    ///
    /// # Panics
    ///
    /// Panics when a frame is over budget, a row is lost or repeated, or the
    /// retry changes a frame's identity.
    #[tokio::test]
    async fn an_oversized_detail_reaches_scribe_in_bounded_frames_that_replay_identically() {
        let recording = Arc::new(RecordingScribe::default());
        let scribe = LostAck {
            inner: Arc::clone(&recording),
            lose: 1,
            seen: AtomicUsize::new(0),
        };
        let outbox = route_outbox(ScribeRoute::Local(Arc::new(scribe)), SMALL_REQUEST_BYTES);
        outbox.stage(
            DataTenantId::new_v7(),
            eval_result(&attribution(), 64, 1024),
        );
        settle(&outbox).await;

        let submitted = recording.submitted();
        assert_eq!(
            submitted[2..4],
            submitted[..2],
            "the retry resubmits the identical frames"
        );
        let frames = retained(recording.received());
        assert_eq!(
            frames.len(),
            submitted.len() - 2,
            "every frame after the retry is new"
        );
        assert_bounded_detail(&frames, 64);
    }

    /// Proves a single detail row larger than the budget is still submitted
    /// alone and whole, and a terminal Scribe size refusal consumes it
    /// without retry while the summary is written.
    ///
    /// # Panics
    ///
    /// Panics when the row is split, retried, or the summary is missing.
    #[tokio::test]
    async fn an_unsplittable_row_is_submitted_whole_and_terminal_refusal_consumes_it() {
        let scribe = Arc::new(RecordingScribe::default());
        scribe.refuse_next(ScribeError::PayloadTooLarge {
            bytes: SMALL_REQUEST_BYTES + 1,
            limit: SMALL_REQUEST_BYTES,
        });
        let outbox = route_outbox(
            ScribeRoute::Local(Arc::clone(&scribe) as _),
            SMALL_REQUEST_BYTES,
        );
        outbox.stage(
            DataTenantId::new_v7(),
            eval_result(&attribution(), 1, SMALL_REQUEST_BYTES),
        );
        settle(&outbox).await;

        assert_eq!(scribe.attempts(), 2, "one item frame and one summary");
        let tables = scribe
            .received()
            .iter()
            .map(|frame| frame.table.clone())
            .collect::<Vec<_>>();
        assert_eq!(tables, ["vala.verification.results"]);
    }

    /// A peer Scribe that accepts every capture call and never answers,
    /// recording the batch id it holds.
    #[derive(Clone, Default)]
    struct SilentPeer {
        /// Batch ids of every call received.
        held: Arc<Mutex<Vec<String>>>,
    }

    #[wyrd_tonic::tonic::async_trait]
    impl ScribeCapturePeerService for SilentPeer {
        /// Records the call's batch id and never returns.
        ///
        /// # Errors
        ///
        /// Never returns.
        async fn ingest_capture(
            &self,
            request: Request<IngestCaptureRequest>,
        ) -> Result<Response<IngestCaptureResponse>, Status> {
            self.held
                .lock()
                .expect("held")
                .push(request.into_inner().batch_id);
            std::future::pending().await
        }
    }

    /// Serves `service` over mutual TLS under `ca` and `leaf` on a loopback
    /// port, returning its address.
    ///
    /// # Panics
    ///
    /// Panics when the listener or TLS server cannot start.
    async fn serve_peer<S>(
        ca: &BifrostPeerCa,
        leaf: &wyrd_testing::bifrost::peer_ca::BifrostPeerLeaf,
        service: ScribeCapturePeerServiceServer<S>,
    ) -> String
    where
        S: ScribeCapturePeerService,
    {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("peer listener binds");
        let port = listener.local_addr().expect("listener address").port();
        let incoming = async_stream::stream! {
            loop {
                yield listener.accept().await.map(|(stream, _)| stream);
            }
        };
        let server = wyrd_tonic::server::mutual_tls_server(
            wyrd_tonic::server::MutualTlsServerConfig::from_pem(
                leaf.certificate_pem().as_bytes(),
                leaf.private_key_pem().as_bytes(),
                ca.ca_certificate_pem().as_bytes(),
            ),
        )
        .expect("mutual TLS server builds");
        tokio::spawn(
            server
                .clone()
                .add_service(service)
                .serve_with_incoming(incoming),
        );
        format!("https://127.0.0.1:{port}")
    }

    /// A ready Scribe lease for `node` answering at `address`.
    fn scribe_lease(node: u128, address: String) -> ClusterRoleLease {
        let now = Utc::now();
        ClusterRoleLease {
            key: ClusterNodeKey {
                node_id: NodeId::new(uuid::Uuid::from_u128(node)),
                role: ClusterRole::Scribe,
            },
            address,
            fencing_token: 1,
            capability_version: 1,
            capabilities: ClusterCapabilities::ScribeV1(ScribeCapabilitiesV1 {
                tail_protocol_version: 1,
            }),
            ready: true,
            started_at: now,
            heartbeat_at: now,
        }
    }

    /// Proves a silent peer cannot hold a tenant: the first ready Scribe
    /// accepts every call and never answers, each held frame times out and
    /// its slice is retried under the same batch ids on the next ready
    /// Scribe, where an oversized Eval detail arrives as bounded frames with
    /// every row once and another tenant's capture is written too.
    ///
    /// # Panics
    ///
    /// Panics when a held frame is not retried under its id, a row is lost
    /// or repeated, a frame is over budget, or the other tenant's capture is
    /// missing.
    #[tokio::test]
    async fn a_silent_peer_times_out_and_the_slice_retries_on_the_next_scribe() {
        let ca = BifrostPeerCa::generate("scribe.peer.test").expect("peer CA");
        let leaf = ca.issue_leaf("scribe").expect("peer leaf");
        let silent = SilentPeer::default();
        let silent_address = serve_peer(
            &ca,
            &leaf,
            ScribeCapturePeerServiceServer::new(silent.clone()),
        )
        .await;
        let recording = Arc::new(RecordingScribe::default());
        let ready_address = serve_peer(
            &ca,
            &leaf,
            ScribeCapturePeerGrpc::new(Arc::clone(&recording) as _).into_server(),
        )
        .await;
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://unused.invalid/unused")
            .expect("a lazy pool never connects");
        let cluster = ClusterRegistry::new(
            vala_sql::postgres::ValaPostgres::from_pool(pool),
            NodeId::new(uuid::Uuid::now_v7()),
        );
        cluster.publish_snapshot_for_test(ClusterSnapshot::new(vec![
            scribe_lease(1, silent_address),
            scribe_lease(2, ready_address),
        ]));
        let peers = ScribePeers {
            cluster: Arc::new(cluster),
            tls: BifrostPeerTls::new(
                ca.ca_certificate_pem().as_bytes().to_vec(),
                ca.server_name().to_owned(),
                leaf.certificate_pem().as_bytes().to_vec(),
                secrecy::SecretString::from(leaf.private_key_pem().to_owned()),
            ),
            channels: Mutex::new(HashMap::new()),
            timeout: Duration::from_millis(300),
        };
        let outbox = route_outbox(ScribeRoute::Peer(peers), SMALL_REQUEST_BYTES);
        let tenant = DataTenantId::new_v7();
        let other = DataTenantId::new_v7();
        outbox.stage(tenant, eval_result(&attribution(), 64, 1024));
        outbox.stage(other, capture(other));
        settle(&outbox).await;

        let held = silent.held.lock().expect("held").clone();
        assert!(!held.is_empty(), "the silent peer held a frame");
        let received = recording.received();
        let acknowledged = received
            .iter()
            .map(|frame| frame.batch_id.to_string())
            .collect::<HashSet<_>>();
        assert!(
            held.iter().all(|id| acknowledged.contains(id)),
            "every held frame is retried under its batch id on the ready Scribe"
        );
        let (mine, others): (Vec<_>, Vec<_>) = retained(received)
            .into_iter()
            .partition(|frame| frame.tenant == tenant);
        assert_bounded_detail(&mine, 64);
        assert_eq!(
            others
                .iter()
                .map(|frame| frame.table.as_str())
                .collect::<Vec<_>>(),
            ["vala.gateway.calls", "vala.traces.spans"],
            "the other tenant's capture is written"
        );
    }

    /// Proves in-process refusals and peer status codes classify into the
    /// same retryable and terminal refusals, and an expired peer call, by
    /// either deadline code, retries.
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
        for code in [Code::DeadlineExceeded, Code::Cancelled] {
            assert_eq!(ScribeRefusal::from_code(code), ScribeRefusal::Unavailable);
        }
    }
}
