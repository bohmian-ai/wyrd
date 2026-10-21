//! The process's one Scribe-bound outbox: audit decisions, gateway capture,
//! Verifier execution inputs and results, and tenant copies of server spans.
//!
//! Producers stage logical [`ScribeWrite`] values on [`ScribeOutbox`] and
//! return at once. The outbox writer hands each tenant's pending slice to
//! [`ScribeSink`], which projects every write into its fixed destination rows,
//! groups those rows by destination, encodes each group as Arrow IPC frames
//! under content-derived batch ids, splitting any group larger than a quarter
//! of the configured Scribe request ceiling by rows, encoding on the blocking
//! pool, and submits up to [`FRAMES_IN_FLIGHT`] chunks of frames at once
//! in-process to this pod's Scribe or over
//! the peer plane to a live one. A
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
//!
//! A submitted execution's inputs row carries its [`Submission`]; once the
//! write that holds it returns with the row acknowledged, the sink stages the
//! submission on the [`SubmissionOutbox`] that accepts it into the durable
//! queue, so no execution is queued before its inputs are durable.
//!
//! A sink given the Wyrd Postgres owner also schedules grouped verification:
//! once a slice's frames are acknowledged, every grouped inputs or result row
//! that was not lost records its group's activity in one tenant transaction
//! through [`VerifierExecutionQueue::record_group_activity`], before any
//! submission is handed off. A failed transaction fails the write, so the
//! outbox retries the identical slice. Activity acknowledged by Scribe but
//! not yet recorded when the process dies is lost with the rest of the
//! outbox.

use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use arrow::array::{Array as _, AsArray as _, RecordBatch};
use arrow::compute::concat_batches;
use arrow::ipc::writer::StreamWriter;
use bytes::Bytes;
use chrono::Utc;
use futures_util::StreamExt as _;
use futures_util::stream::FuturesUnordered;
use sha2::{Digest, Sha256};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use uuid::Uuid;
use vala_bifrost_redux::catalog::TableRef;
use vala_bifrost_redux::cluster::ClusterRegistry;
use vala_bifrost_redux::contracts::{IngressPayload, Scribe, ScribeError, ScribeIngressFrame};
use vala_bifrost_redux::gate::attribution::native_card_uids;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::oracle::dispatcher::BifrostPeerTls;
use vala_bifrost_redux::tables::audit::projection::project_audit_event;
use vala_bifrost_redux::tables::gateway::CallsTable;
use vala_bifrost_redux::tables::traces::project_resource_spans;
use vala_bifrost_redux::tables::{
    AuditLogTable, DomainTable, ExecutionInputsTable, ResultFeaturesTable, ResultsTable, SpansTable,
};
use wyrd_runtime::outbox::{Outbox, OutboxSink, Reservation, count_lost};
use wyrd_runtime::{PermissionSet, Principal, PrincipalKind};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{Attribution, PLATFORM_AUDIT_PRINCIPAL};
use wyrd_spec::ids::{CardUid, GroupId};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{AuditEvent, NodeId};
use wyrd_sql::WyrdPostgres;
use wyrd_sql::queries::verifier_executions::{GroupActivity, Submission, VerifierExecutionQueue};
use wyrd_tonic::otlp::trace::v1::ResourceSpans;
use wyrd_tonic::tonic::Code;
use wyrd_tonic::tonic::transport::Channel;
use wyrd_tonic::wyrd::v1::IngestCaptureRequest;
use wyrd_tonic::wyrd::v1::scribe_capture_peer_service_client::ScribeCapturePeerServiceClient;

use crate::components::gateway::{CallCapture, CaptureDrop};
use crate::verification::results::{ResultBatch, ResultPayload};
use crate::verification::submissions::SubmissionSink;

/// The process's one non-blocking outbox for every Scribe-bound write.
pub type ScribeOutbox = Outbox<ScribeSink>;

/// Most tenants whose slices are written at once.
const SCRIBE_WRITERS: usize = 4;

/// Default of `scribe.outbox_capacity`: the most writes the process Scribe
/// outbox admits at once, queued, retrying, or in flight. Every write's rows
/// are bounded by its producer's input limits, so this also bounds the
/// outbox's memory.
pub(crate) const SCRIBE_CAPACITY: usize = 65_536;

/// Longest a peer Scribe may take to acknowledge one frame.
///
/// Matches the other Bifrost peer call budgets. Expiry surfaces as
/// `DeadlineExceeded`, which classifies as [`ScribeRefusal::Unavailable`].
const PEER_SUBMIT_TIMEOUT: Duration = Duration::from_secs(30);

/// Most frames of one tenant slice awaiting Scribe's acknowledgement at once.
///
/// Each frame waits for its own durable acknowledgement, so submitting them
/// one at a time made a slice's write time the sum of its acknowledgements.
/// Concurrent frames share Scribe's WAL commits instead. The bound keeps one
/// slice's encoded frames, at most this many times the frame budget, within
/// the memory a large backlog may take, and leaves Scribe admission room for
/// the other writers' slices.
const FRAMES_IN_FLIGHT: usize = 8;

/// Frame budget for a Scribe request ceiling of `request_bytes`.
///
/// A tenant slice can hold a long outage's backlog, so each frame's rows, as
/// retained Arrow bytes and as encoded Arrow IPC, stay within a quarter of the ceiling. The
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
    /// One authorization decision bound for retained audit history,
    /// projected into its `vala.system.audit_log` row when it is staged, so
    /// the staging caller, not the one writer every tenant write waits on,
    /// pays for the projection; `None` when the decision could not be
    /// projected, which the sink counts lost.
    Audit(Option<RecordBatch>),
    /// One gateway call's validated analytical record, its objects already
    /// persisted.
    Capture(Box<CallCapture>),
    /// One Verifier judgment's summary and detail rows.
    Result {
        /// The result's rows, details before the summary, each naming the
        /// Card verified as its `card_uid`.
        payload: ResultPayload,
        /// The run's subject and actor, which Scribe stamps on every row.
        attribution: Attribution,
    },
    /// One Verifier execution's pinned inputs row.
    ExecutionInputs {
        /// The `vala.verification.execution_inputs` row.
        row: ResultBatch,
        /// The execution's subject and actor, which Scribe stamps on the row.
        attribution: Attribution,
        /// A submitted execution and its reserved slot on the outbox that
        /// queues it once Scribe acknowledges the row, so the handoff is never
        /// refused; `None` for a direct execution.
        queued: Option<(Reservation<SubmissionSink>, Box<Submission>)>,
    },
    /// One server span's tenant copy, attributed to the operation that
    /// emitted it.
    ServerSpans {
        /// The emitting operation's subject and actor, which Scribe stamps on
        /// the row.
        attribution: Attribution,
        /// The span as one OTLP resource span, already reduced to the fields
        /// its tenant may see; boxed so the enum stays as small as its
        /// pointer variants.
        spans: Box<ResourceSpans>,
        /// The span's share of [`TENANT_SPAN_PERMITS`], held while the copy
        /// is staged or retried and released once it is written, consumed,
        /// or abandoned.
        permit: OwnedSemaphorePermit,
    },
}

impl From<AuditEvent> for ScribeWrite {
    /// Stages one decision projected into its row, stamped with the instant
    /// it is staged, which is the instant the boundary decided.
    fn from(event: AuditEvent) -> Self {
        Self::Audit(project_audit_event(&event, Utc::now()).ok())
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
    /// Subject and actor Scribe stamps on the rows; the unresolved sentinel
    /// pair for audit rows, which carry their own attribution as content.
    attribution: Attribution,
    /// Request the rows were admitted under, when the write has one.
    request: Option<&'a RequestId>,
    /// The rows.
    batch: RecordBatch,
}

/// Projects `spans` into one canonical `vala.traces.spans` batch.
///
/// Gateway capture and server-span tenant copies both project their spans
/// here. `None` when the projection fails or rejects any span, since every
/// span of a write lands or none does.
pub(crate) fn project_spans(spans: &ResourceSpans) -> Option<RecordBatch> {
    project_resource_spans(std::slice::from_ref(spans), None, usize::MAX)
        .ok()
        .filter(|(_, outcome)| outcome.rejected_spans == 0)
        .map(|(batch, _)| batch)
}

impl ScribeWrite {
    /// Projects this write into its destination rows, in write order.
    ///
    /// # Errors
    ///
    /// Returns the projection failure's reason when the rows cannot be built.
    fn rows(&self) -> Result<Vec<Rows<'_>>, &'static str> {
        match self {
            Self::Audit(batch) => Ok(vec![Rows {
                table: ScribeTable::AuditLog,
                attribution: Attribution::unresolved(),
                request: None,
                batch: batch.clone().ok_or("projection")?,
            }]),
            Self::Capture(capture) => Ok(capture
                .batches()
                .map_err(CaptureDrop::reason)?
                .into_iter()
                .map(|(table, batch)| Rows {
                    table,
                    attribution: capture.attribution(),
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
                        attribution: *attribution,
                        request: None,
                        batch: batch.batch.clone(),
                    })
                })
                .collect(),
            Self::ExecutionInputs {
                row, attribution, ..
            } => Ok(vec![Rows {
                table: ScribeTable::from_fqn(&row.table)
                    .filter(|table| *table == ScribeTable::ExecutionInputs)
                    .ok_or("projection")?,
                attribution: *attribution,
                request: None,
                batch: row.batch.clone(),
            }]),
            Self::ServerSpans {
                attribution, spans, ..
            } => Ok(vec![Rows {
                table: ScribeTable::Spans,
                attribution: *attribution,
                request: None,
                batch: project_spans(spans).ok_or("projection")?,
            }]),
        }
    }

    /// The grouped source activity this write records: the owner, subject,
    /// Verifier, and group of an inputs row or a result's summary row, or
    /// `None` when the write is no grouped Verifier evidence or names no
    /// owner.
    fn group_activity(&self) -> Option<GroupActivity> {
        let batch = match self {
            Self::ExecutionInputs { row, .. } => &row.batch,
            Self::Result { payload, .. } => payload
                .batches()
                .iter()
                .find(|batch| ScribeTable::from_fqn(&batch.table) == Some(ScribeTable::Results))
                .map(|batch| &batch.batch)?,
            Self::Audit(_) | Self::Capture(_) | Self::ServerSpans { .. } => return None,
        };
        let cell = |name: &str| {
            let column = batch.column_by_name(name)?.as_string_opt::<i32>()?;
            (batch.num_rows() > 0 && column.is_valid(0)).then(|| column.value(0))
        };
        let group_id = GroupId::new(cell("group_id")?).ok()?;
        Some(GroupActivity {
            owner_card_uid: CardUid::new(cell("owner_card_uid")?).ok()?,
            card_uid: CardUid::new(cell("card_uid")?).ok()?,
            verifier_card_uid: CardUid::new(cell("verifier_card_uid")?).ok()?,
            group_id,
        })
    }
}

/// The destinations the Scribe outbox writes: retained audit history, the two
/// gateway capture tables, the two Verifier result tables, and the execution
/// inputs table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScribeTable {
    /// `vala.system.audit_log`, one row per authorization decision.
    AuditLog,
    /// `vala.gateway.calls`, one row per captured call.
    Calls,
    /// `vala.traces.spans`, one span per captured attempt or server span.
    Spans,
    /// `vala.verification.results`, one canonical summary row per judgment.
    Results,
    /// `vala.drift.result_features`, a result's drift feature rows.
    DriftFeatures,
    /// `vala.verification.execution_inputs`, one row per execution's inputs.
    ExecutionInputs,
}

impl ScribeTable {
    /// Every destination, in a fixed order.
    const ALL: [Self; 6] = [
        Self::AuditLog,
        Self::Calls,
        Self::Spans,
        Self::Results,
        Self::DriftFeatures,
        Self::ExecutionInputs,
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
            Self::ExecutionInputs => "vala.verification.execution_inputs",
        }
    }

    /// Resolves a fully qualified name to a destination, refusing any other
    /// table.
    pub(crate) fn from_fqn(fqn: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|table| table.fqn() == fqn)
    }

    /// Whether this destination is a Verifier result table.
    pub(crate) const fn is_result(self) -> bool {
        matches!(self, Self::Results | Self::DriftFeatures)
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
            Self::ExecutionInputs => {
                TableRef::new(BifrostNamespace::Verification, ExecutionInputsTable::NAME)
            }
        }
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
    /// Scribe acknowledged every frame, but recording grouped activity
    /// failed; the slice is retried.
    #[error("group scheduling failed")]
    Scheduling,
}

impl ScribeRefusal {
    /// Closed metric label and diagnostic reason of this refusal.
    const fn reason(self) -> &'static str {
        match self {
            Self::Saturated => "saturated",
            Self::Unavailable => "unavailable",
            Self::Rejected => "rejected",
            Self::Scheduling => "scheduling",
        }
    }

    /// Classifies an in-process Scribe refusal.
    ///
    /// Backpressure and a full WAL are saturation; a closed or failing writer
    /// is unavailable; anything else refuses this frame for good.
    pub(crate) fn from_scribe(error: &ScribeError) -> Self {
        match error {
            ScribeError::IngestBusy { .. } | ScribeError::WalDiskFull => Self::Saturated,
            ScribeError::IngressClosed | ScribeError::Internal { .. } => Self::Unavailable,
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
/// so a frame is identical in-process and over the peer plane.
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
    /// Subject and actor Scribe stamps on the frame's rows.
    pub(crate) attribution: Attribution,
}

impl ScribeBatch {
    /// Builds the Scribe frame for this batch.
    ///
    /// Scribe stamps the batch's [`Attribution`] on every row and authorizes
    /// each row's `card_uid` against the UID-bearing Cards the rows name,
    /// which the producing owner authorized before staging. The transport
    /// principal only carries the frame: the platform audit principal, the
    /// one principal Scribe admits for the system owner's audit history, for
    /// an audit frame, and the stamped actor otherwise. It carries no
    /// permission, since every write here is server-internal and evaluates
    /// none.
    pub(crate) fn into_frame(self) -> ScribeIngressFrame {
        let transport = match self.table {
            ScribeTable::AuditLog => PLATFORM_AUDIT_PRINCIPAL,
            _ => self.attribution.act_principal_id,
        };
        ScribeIngressFrame {
            attributed_cards: Some(native_card_uids(&self.ipc)).filter(|uids| !uids.is_empty()),
            attribution: self.attribution,
            principal: Principal::new(
                transport,
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
                sub_principal_id: batch.attribution.sub_principal_id.to_string(),
                act_principal_id: batch.attribution.act_principal_id.to_string(),
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

/// Most tenant span copies one process holds unwritten at once.
///
/// The outbox admits at most [`SCRIBE_CAPACITY`] writes, a quarter of them
/// unreserved; this smaller cap on the span producer keeps a sustained Scribe
/// outage's span copies from filling that unreserved share ahead of audit and
/// capture writes.
const TENANT_SPAN_PERMITS: usize = 2_048;

/// Stages the tenant copies of server spans on the process Scribe outbox.
///
/// Telemetry hands each copy over as its span ends, already reduced to the
/// tenant's safe fields; staging returns at once, so a slow or unavailable
/// Scribe never reaches the request. Each staged copy holds one of
/// [`TENANT_SPAN_PERMITS`]; when none is free the copy is dropped and counted
/// lost instead of waiting.
pub(crate) struct ServerSpanSink {
    /// The process outbox every Scribe write is staged on.
    outbox: Arc<ScribeOutbox>,
    /// Permits for copies staged and not yet written, consumed, or abandoned.
    permits: Arc<Semaphore>,
}

impl ServerSpanSink {
    /// Stages copies on `outbox` under a full permit pool.
    pub(crate) fn new(outbox: Arc<ScribeOutbox>) -> Self {
        Self {
            outbox,
            permits: Arc::new(Semaphore::new(TENANT_SPAN_PERMITS)),
        }
    }
}

impl wyrd_telemetry::TenantSpanSink for ServerSpanSink {
    /// Stages one copy for the route's tenant under its attribution, or
    /// drops and counts it lost when every permit is held.
    fn stage(&self, route: wyrd_telemetry::TenantSpanRoute, span: ResourceSpans) {
        let Ok(permit) = Arc::clone(&self.permits).try_acquire_owned() else {
            count_lost::<ScribeSink>(1);
            return;
        };
        self.outbox.stage(
            route.tenant,
            ScribeWrite::ServerSpans {
                attribution: route.attribution,
                spans: Box::new(span),
                permit,
            },
        );
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
    /// Wyrd Postgres owner recording acknowledged grouped activity; `None`
    /// schedules no group.
    groups: Option<WyrdPostgres>,
}

impl ScribeSink {
    /// Starts an outbox submitting along `route`, or dropping every write
    /// when `route` is `None`, at the default capacity.
    ///
    /// `request_bytes` is the deployment's configured Scribe request ceiling
    /// (`scribe.ingest_request_bytes`), which every Scribe a frame can reach
    /// enforces.
    ///
    /// # Panics
    /// Panics when called outside a Tokio runtime, because the writer task is
    /// spawned immediately.
    #[must_use]
    pub(crate) fn outbox(route: Option<ScribeRoute>, request_bytes: usize) -> Arc<ScribeOutbox> {
        Self::bounded_outbox(route, request_bytes, SCRIBE_CAPACITY, None)
    }

    /// Starts an outbox like [`Self::outbox`] admitting at most `capacity`
    /// writes (`scribe.outbox_capacity`) and recording acknowledged grouped
    /// activity through `groups`, when given.
    ///
    /// # Panics
    /// Panics when called outside a Tokio runtime, because the writer task is
    /// spawned immediately.
    #[must_use]
    pub(crate) fn bounded_outbox(
        route: Option<ScribeRoute>,
        request_bytes: usize,
        capacity: usize,
        groups: Option<WyrdPostgres>,
    ) -> Arc<ScribeOutbox> {
        Outbox::new(
            Self {
                route,
                attempts: AtomicUsize::new(0),
                frame_budget: frame_budget(request_bytes),
                groups,
            },
            SCRIBE_WRITERS,
            capacity,
        )
    }

    /// Starts an outbox submitting in-process to `scribe`, standing in for a
    /// pod's own Scribe at the default request ceiling, and recording
    /// acknowledged grouped activity through `groups`, when given.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn local_outbox(
        scribe: Arc<dyn Scribe>,
        groups: Option<WyrdPostgres>,
    ) -> Arc<ScribeOutbox> {
        Self::bounded_outbox(
            Some(ScribeRoute::Local(scribe)),
            vala_bifrost_redux::gate::limits::BIFROST_INGEST_REQUEST_LIMIT_BYTES,
            SCRIBE_CAPACITY,
            groups,
        )
    }

    /// Record the grouped activity of every acknowledged, not lost write of
    /// `items` in one `tenant` transaction.
    ///
    /// Issues nothing when no write is grouped evidence or no Postgres owner
    /// was given.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeRefusal::Scheduling`] when the transaction fails, so
    /// the outbox retries the slice; Scribe's batch-id fence absorbs its
    /// resubmitted frames.
    async fn schedule_groups(
        &self,
        tenant: DataTenantId,
        items: &[ScribeWrite],
        lost: &BTreeSet<usize>,
    ) -> Result<(), ScribeRefusal> {
        let Some(postgres) = &self.groups else {
            return Ok(());
        };
        let activity: Vec<GroupActivity> = items
            .iter()
            .enumerate()
            .filter(|(index, _)| !lost.contains(index))
            .filter_map(|(_, item)| item.group_activity())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if activity.is_empty() {
            return Ok(());
        }
        let recorded = async {
            let mut conn = postgres.tenant_conn(tenant).await?;
            VerifierExecutionQueue::default()
                .record_group_activity(&mut conn, &activity)
                .await?;
            conn.commit().await?;
            Ok::<_, wyrd_sql::SqlError>(())
        };
        recorded.await.map_err(|error| {
            tracing::warn!(outbox = Self::NAME, %tenant, %error, "grouped activity could not be recorded; the slice will be retried");
            ScribeRefusal::Scheduling
        })
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
    /// Subject and actor Scribe stamps on every row.
    attribution: Attribution,
    /// Request every row was admitted under, when they share one.
    request: Option<&'a RequestId>,
    /// The rows, all of one schema.
    batches: Vec<RecordBatch>,
    /// Slice position of the write that contributed each row, in row order.
    owners: Vec<usize>,
}

impl Frame<'_> {
    /// Whether `rows` share this frame's destination, attribution, request,
    /// and schema, so one Scribe frame can carry both.
    fn shares(&self, rows: &Rows<'_>) -> bool {
        self.table == rows.table
            && self.attribution == rows.attribution
            && self.request == rows.request
            && {
                let (ours, theirs) = (self.batches[0].schema_ref(), rows.batch.schema_ref());
                Arc::ptr_eq(ours, theirs) || ours == theirs
            }
    }

    /// The [`Chunk`] of this frame's `rows` rows starting at row `first`,
    /// held by `batches`.
    fn chunk(&self, first: usize, rows: usize, batches: Vec<RecordBatch>) -> Chunk {
        Chunk {
            table: self.table,
            attribution: self.attribution,
            request: self.request.cloned(),
            batches,
            owners: self.owners[first..first + rows].to_vec(),
        }
    }
}

impl ScribeSink {
    /// Groups `items` into frames, in first-seen destination order.
    ///
    /// Rows join the latest frame they share a destination, attribution,
    /// request, and schema with, and otherwise open a new one, so the frames
    /// of an identical slice are identical. Nothing is sized per row: one
    /// row's batch allocates many times the bytes it holds, and measuring each
    /// row on the writer cost more than the write itself. [`Self::chunks`]
    /// cuts each frame into budget-sized chunks from one measured row. Gateway capture
    /// carries its admitting request, which Scribe stamps as
    /// `wyrd_request_id`, so captures group per request. Result summary frames
    /// are moved after every detail frame, so a visible summary's details
    /// were acknowledged first. Positions of writes that cannot be projected
    /// are added to `lost`.
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
                let owners = std::iter::repeat_n(index, rows.batch.num_rows());
                match frames.iter_mut().rev().find(|frame| frame.shares(&rows)) {
                    Some(frame) => {
                        frame.batches.push(rows.batch);
                        frame.owners.extend(owners);
                    }
                    None => frames.push(Frame {
                        table: rows.table,
                        attribution: rows.attribution,
                        request: rows.request,
                        batches: vec![rows.batch],
                        owners: owners.collect(),
                    }),
                }
            }
        }
        frames.sort_by_key(|frame| frame.table == ScribeTable::Results);
        frames
    }

    /// Cuts `frame`'s rows into consecutive [`Chunk`]s of about `budget`
    /// retained Arrow bytes each, without concatenating them.
    ///
    /// The frame's first batch is measured once and every row is taken to
    /// be its size, so cutting costs one measurement per frame and depends
    /// only on the rows: an identical slice always cuts identically. A
    /// staged batch is never divided; skewed row sizes are left to
    /// [`Chunk::encode`]'s split by encoded size.
    fn chunks(frame: &Frame<'_>, budget: usize) -> Vec<Chunk> {
        let sample = &frame.batches[0];
        let row_bytes = wyrd_queue::retained_bytes(sample)
            .unwrap_or_else(|| sample.get_array_memory_size())
            .div_ceil(sample.num_rows().max(1))
            .max(1);
        let rows_per_chunk = (budget / row_bytes).max(1);
        let mut chunks = Vec::new();
        let (mut first, mut rows, mut batches) = (0, 0, Vec::new());
        for batch in &frame.batches {
            if rows > 0 && rows + batch.num_rows() > rows_per_chunk {
                chunks.push(frame.chunk(first, rows, std::mem::take(&mut batches)));
                first += rows;
                rows = 0;
            }
            rows += batch.num_rows();
            batches.push(batch.clone());
        }
        if rows > 0 {
            chunks.push(frame.chunk(first, rows, batches));
        }
        chunks
    }
}

/// One run of a frame's consecutive rows, encoded and submitted together.
///
/// It owns everything its encoding reads, so the encoding runs on the
/// blocking pool while the slice's other chunks encode or await Scribe.
struct Chunk {
    /// Destination table.
    table: ScribeTable,
    /// Subject and actor Scribe stamps on every row.
    attribution: Attribution,
    /// Request every row was admitted under, when they share one.
    request: Option<RequestId>,
    /// The staged batches holding the chunk's rows, in row order.
    batches: Vec<RecordBatch>,
    /// Slice position of the write that contributed each row, in row order.
    owners: Vec<usize>,
}

/// One submitted stream: how Scribe answered, the stream, and the slice
/// positions of the writes its rows came from.
type Submitted = (Result<(), ScribeRefusal>, ScribeBatch, BTreeSet<usize>);

impl Chunk {
    /// Concatenates the chunk's rows and encodes them as Arrow IPC streams
    /// of at most `budget` bytes, each under its content-derived identity
    /// and with the slice positions of the writes its rows came from.
    ///
    /// A single row larger than `budget` is still encoded alone; Scribe
    /// decides whether its ceiling admits it.
    ///
    /// # Errors
    ///
    /// Returns the Arrow error when the rows cannot be concatenated or
    /// encoded.
    fn encode(
        &self,
        tenant: DataTenantId,
        budget: usize,
    ) -> Result<Vec<(ScribeBatch, BTreeSet<usize>)>, arrow::error::ArrowError> {
        let rows = match self.batches.as_slice() {
            [only] => only.clone(),
            batches => concat_batches(batches[0].schema_ref(), batches)?,
        };
        let mut streams = Vec::new();
        Self::split(&rows, 0, budget, &mut streams)?;
        Ok(streams
            .into_iter()
            .map(|(start, count, ipc)| {
                let owners = self.owners[start..start + count].iter().copied().collect();
                (self.identify(tenant, ipc), owners)
            })
            .collect())
    }

    /// Appends `rows`, whose first row is row `first` of its frame, to
    /// `streams` as one Arrow IPC stream with its first row and row count, or,
    /// when that stream exceeds `budget` bytes and holds more than one row, as
    /// the streams of each half in turn.
    ///
    /// Halving depends only on the rows, so an identical slice always splits
    /// into identical streams, whatever its row sizes.
    ///
    /// # Errors
    ///
    /// Returns the Arrow error when the rows cannot be encoded.
    fn split(
        rows: &RecordBatch,
        first: usize,
        budget: usize,
        streams: &mut Vec<(usize, usize, Vec<u8>)>,
    ) -> Result<(), arrow::error::ArrowError> {
        let mut writer = StreamWriter::try_new(Vec::new(), &rows.schema())?;
        writer.write(rows)?;
        let ipc = writer.into_inner()?;
        let count = rows.num_rows();
        if ipc.len() <= budget || count < 2 {
            streams.push((first, count, ipc));
            return Ok(());
        }
        let half = count / 2;
        Self::split(&rows.slice(0, half), first, budget, streams)?;
        Self::split(
            &rows.slice(half, count - half),
            first + half,
            budget,
            streams,
        )
    }

    /// Wraps one encoded stream of `frame` under its content-derived identity.
    ///
    /// The batch id hashes the tenant, destination, attribution, request,
    /// and encoded rows, so a retry of the identical slice resubmits the same
    /// identity and Scribe's batch-id dedup absorbs frames it already
    /// accepted. A frame with no originating request is admitted under a
    /// request id derived from the batch id, so it too is stable across
    /// retries. The digest carries `UUIDv7` version and variant bits.
    ///
    /// # Panics
    ///
    /// Panics only if a `UUIDv7` batch id failed to parse as a request id,
    /// which its construction rules out.
    fn identify(&self, tenant: DataTenantId, ipc: Vec<u8>) -> ScribeBatch {
        let mut digest = Sha256::new();
        digest.update(b"wyrd.scribe.outbox.batch-id.v1");
        digest.update(tenant.as_uuid().as_bytes());
        digest.update(self.table.fqn().as_bytes());
        digest.update(self.attribution.sub_principal_id.as_uuid().as_bytes());
        digest.update(self.attribution.act_principal_id.as_uuid().as_bytes());
        if let Some(request) = &self.request {
            digest.update(request.as_str().as_bytes());
        }
        digest.update(&ipc);
        let digest: [u8; 32] = digest.finalize().into();
        let mut bytes = [0_u8; 16];
        bytes.copy_from_slice(&digest[..16]);
        bytes[6] = (bytes[6] & 0x0f) | 0x70;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        let batch_id = Uuid::from_bytes(bytes);
        let request_id = self.request.clone().unwrap_or_else(|| {
            RequestId::parse(&batch_id.to_string()).expect("a UUIDv7 batch id is a request id")
        });
        ScribeBatch {
            tenant,
            table: self.table,
            batch_id,
            request_id,
            ipc: Bytes::from(ipc),
            attribution: self.attribution,
        }
    }
}

impl ScribeSink {
    /// Submits every frame of `frames` along `route` for write `attempt`,
    /// keeping up to [`FRAMES_IN_FLIGHT`] chunks encoding or awaiting
    /// acknowledgement.
    ///
    /// Each frame is cut into [`Chunk`]s, and each chunk is concatenated and
    /// encoded on the blocking pool by [`Self::deliver`] only as a slot
    /// frees, so the slice's encoding runs in parallel and at most
    /// [`FRAMES_IN_FLIGHT`] chunks' streams are held at once. Writes whose
    /// rows cannot be encoded, or whose stream Scribe rejects terminally,
    /// are added to `lost`. After a retryable refusal no further chunk is
    /// started; those in flight still finish.
    ///
    /// # Errors
    ///
    /// Returns the first retryable refusal once every chunk in flight has
    /// finished.
    ///
    /// # Cancellation
    ///
    /// Dropping the future drops the chunks in flight; Scribe may already
    /// hold some of their streams, and the retried slice resubmits them
    /// under the same batch ids.
    async fn submit(
        &self,
        route: &ScribeRoute,
        tenant: DataTenantId,
        frames: &[Frame<'_>],
        attempt: usize,
        lost: &mut BTreeSet<usize>,
    ) -> Result<(), ScribeRefusal> {
        let budget = self.frame_budget;
        let mut in_flight = FuturesUnordered::new();
        let mut refused = None;
        for chunk in frames.iter().flat_map(|frame| Self::chunks(frame, budget)) {
            if in_flight.len() == FRAMES_IN_FLIGHT
                && let Some(done) = in_flight.next().await
            {
                Self::tally(tenant, done, lost, &mut refused);
            }
            if refused.is_some() {
                break;
            }
            in_flight.push(Self::deliver(route, tenant, chunk, budget, attempt));
        }
        while let Some(done) = in_flight.next().await {
            Self::tally(tenant, done, lost, &mut refused);
        }
        refused.map_or(Ok(()), Err)
    }

    /// Encodes `chunk` on the blocking pool, then submits its streams along
    /// `route` in turn, stopping after a retryable refusal.
    ///
    /// Returns each submitted stream with its answer, and the slice positions
    /// of the chunk's writes when its rows could not be encoded, which is
    /// logged here.
    ///
    /// # Panics
    ///
    /// Re-raises a panic of the encoding. The encoding is not cancelled
    /// while this future awaits it, since a runtime shutting down drops this
    /// future first.
    async fn deliver(
        route: &ScribeRoute,
        tenant: DataTenantId,
        chunk: Chunk,
        budget: usize,
        attempt: usize,
    ) -> (Vec<Submitted>, Vec<usize>) {
        let encoding = tokio::task::spawn_blocking(move || match chunk.encode(tenant, budget) {
            Ok(streams) => Ok(streams),
            Err(error) => Err((error, chunk.table, chunk.owners)),
        });
        let streams = match encoding.await {
            Ok(Ok(streams)) => streams,
            Ok(Err((error, table, owners))) => {
                tracing::warn!(outbox = Self::NAME, %tenant, table = table.fqn(), %error, "a Scribe frame could not be encoded; its writes are dropped");
                return (Vec::new(), owners);
            }
            Err(panicked) => std::panic::resume_unwind(panicked.into_panic()),
        };
        let mut submitted = Vec::with_capacity(streams.len());
        for (batch, owners) in streams {
            let result = route.submit(&batch, attempt).await;
            let retryable = matches!(result, Err(refusal) if refusal != ScribeRefusal::Rejected);
            submitted.push((result, batch, owners));
            if retryable {
                break;
            }
        }
        (submitted, Vec::new())
    }

    /// Records one delivered chunk: writes that could not be encoded or
    /// whose stream was rejected terminally are added to `lost`, and the
    /// first retryable refusal is kept in `refused` unless an earlier one
    /// already is.
    fn tally(
        tenant: DataTenantId,
        (submitted, unencodable): (Vec<Submitted>, Vec<usize>),
        lost: &mut BTreeSet<usize>,
        refused: &mut Option<ScribeRefusal>,
    ) {
        lost.extend(unencodable);
        for (result, batch, owners) in submitted {
            match result {
                Ok(()) => {}
                Err(ScribeRefusal::Rejected) => {
                    tracing::error!(outbox = Self::NAME, %tenant, table = batch.table.fqn(), batch_id = %batch.batch_id, writes = owners.len(), "Scribe rejected a frame terminally; its writes are dropped");
                    lost.extend(owners);
                }
                Err(refusal) => {
                    tracing::warn!(outbox = Self::NAME, %tenant, table = batch.table.fqn(), reason = refusal.reason(), "Scribe refused a frame; the slice will be retried");
                    refused.get_or_insert(refusal);
                }
            }
        }
    }
}

impl OutboxSink for ScribeSink {
    type Item = ScribeWrite;
    type Error = ScribeRefusal;
    const NAME: &'static str = "scribe";

    /// Writes one tenant's slice as destination frames: every detail frame,
    /// then, once all are acknowledged, every result summary frame.
    ///
    /// Within each phase up to [`FRAMES_IN_FLIGHT`] frames await Scribe at
    /// once, so their acknowledgement order is not their row order. Every
    /// frame of this attempt is submitted to the same Scribe, so a retry
    /// after a peer refusal or timeout moves the whole slice to the next
    /// ready one. A frame over the budget is submitted as its row-split
    /// streams. Every frame is submitted even after a terminal refusal; a
    /// retryable refusal submits no further frame, lets those in flight
    /// finish, and fails the write so the outbox retries the identical slice,
    /// whose already acknowledged frames Scribe's batch-id fence absorbs.
    /// Writes that were projected away or whose frame was refused terminally
    /// are counted lost once the slice completes, never on a retried
    /// attempt. A process that reaches no Scribe drops the whole slice.
    /// Once the slice completes, the grouped activity of every write that was
    /// not lost is recorded, then every submission whose inputs row was not
    /// lost is staged on its [`SubmissionOutbox`]; a lost row queues nothing.
    /// A failed activity transaction retries the slice before any loss is
    /// counted or reservation consumed.
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
        let frames = Self::frames(items, &mut lost);
        let (details, summaries) =
            frames.split_at(frames.partition_point(|frame| frame.table != ScribeTable::Results));
        for phase in [details, summaries] {
            self.submit(route, tenant, phase, attempt, &mut lost)
                .await?;
        }
        self.schedule_groups(tenant, items, &lost).await?;
        count_lost::<Self>(lost.len());
        for (index, item) in items.iter().enumerate() {
            if let ScribeWrite::ExecutionInputs {
                queued: Some((reserved, submission)),
                ..
            } = item
                && !lost.contains(&index)
            {
                reserved.stage(tenant, (**submission).clone());
            }
        }
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
    use wyrd_runtime::audit::AuditStage;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::{Attribution, PLATFORM_AUDIT_PRINCIPAL, PrincipalId, PrincipalKindTag};
    use wyrd_spec::card::drift::DriftMethod;
    use wyrd_spec::card::verifier::Check;
    use wyrd_spec::gateway::GatewayCaptureMode;
    use wyrd_spec::ids::{CardUid, ExecutionId, FeatureName};
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{
        AuditEvent, AuditOutcome, ClusterCapabilities, ClusterNodeKey, ClusterRole,
        ClusterRoleLease, NodeId, ScribeCapabilitiesV1,
    };
    use wyrd_spec::verification::TaskId;
    use wyrd_spec::verification::execution::{
        RequirementDetail, RequirementOutcome, RequirementResult, VerificationOutcome,
    };
    use wyrd_spec::verification::{
        DriftReport, DriftVerdict, DriftWindow, FeatureDriftReport, VerificationVerdict,
    };
    use wyrd_testing::bifrost::peer_ca::BifrostPeerCa;
    use wyrd_tonic::tonic::{Code, Request, Response, Status};
    use wyrd_tonic::wyrd::v1::scribe_capture_peer_service_server::{
        ScribeCapturePeerService, ScribeCapturePeerServiceServer,
    };
    use wyrd_tonic::wyrd::v1::{IngestCaptureRequest, IngestCaptureResponse};

    use wyrd_telemetry::{TenantSpanRoute, TenantSpanSink as _};
    use wyrd_tonic::otlp::trace::v1::{ResourceSpans, ScopeSpans, Span};

    use super::{
        FRAMES_IN_FLIGHT, Outbox, SCRIBE_CAPACITY, SCRIBE_WRITERS, ScribeOutbox, ScribePeers,
        ScribeRefusal, ScribeRoute, ScribeSink, ScribeWrite, ServerSpanSink, TENANT_SPAN_PERMITS,
        frame_budget,
    };
    use crate::components::gateway::CallCapture;
    use crate::components::gateway::capture_tests::{facts, policy};
    use crate::components::gateway::recording::{Received, RecordingScribe};
    use crate::grpc::capture_peer::ScribeCapturePeerGrpc;
    use crate::verification::results::{ResultPayloadBuilder, ResultRun};

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

    /// A fresh Card UID.
    ///
    /// # Panics
    ///
    /// Panics when a v7 UUID is not a Card UID.
    fn uid() -> CardUid {
        CardUid::from_uuid(uuid::Uuid::now_v7()).expect("uid")
    }

    /// A fresh subject judged by the tenant SYSTEM principal.
    fn background() -> Attribution {
        Attribution::new(
            PrincipalId::new(uuid::Uuid::now_v7()),
            PrincipalId::new(uuid::Uuid::now_v7()),
        )
    }

    /// One unscored Drift judgment of `subject` by a fresh Verifier: its
    /// summary row only, attributed to `attribution`.
    ///
    /// # Panics
    ///
    /// Panics when the payload does not build.
    fn result(subject: &CardUid, attribution: Attribution) -> ScribeWrite {
        drift_result(subject, attribution, 0, 0)
    }

    /// One failed Drift judgment of `subject` by a fresh Verifier whose one
    /// requirement scored `features` features, named `f0000`, `f0001`, ...,
    /// and gives a reason of `reason_bytes` characters, attributed to
    /// `attribution`.
    ///
    /// With no features the judgment writes its summary row only.
    ///
    /// # Panics
    ///
    /// Panics when the payload does not build.
    fn drift_result(
        subject: &CardUid,
        attribution: Attribution,
        features: usize,
        reason_bytes: usize,
    ) -> ScribeWrite {
        let verifier = uid();
        let now = Utc::now();
        let window = DriftWindow {
            start: now - chrono::Duration::hours(1),
            end: now,
        };
        let features = (0..features)
            .map(|feature| {
                let name = FeatureName::new(format!("f{feature:04}")).expect("feature name");
                let report = FeatureDriftReport {
                    feature: name.clone(),
                    score: Some(0.4),
                    threshold: Some(0.2),
                    verdict: DriftVerdict::Drift,
                    evidence: None,
                };
                (name, report)
            })
            .collect();
        let outcome = VerificationOutcome {
            verdict: VerificationVerdict::Failed,
            requirements: vec![RequirementResult {
                id: TaskId::new("drift").expect("task id"),
                step: None,
                statement: "drift".to_owned(),
                outcome: RequirementOutcome::Failed,
                reason: "x".repeat(reason_bytes),
                actual: None,
                citations: Vec::new(),
                detail: Some(RequirementDetail::Drift(DriftReport {
                    method: DriftMethod::Psi,
                    features,
                    verdict: DriftVerdict::Drift,
                })),
                check: serde_json::from_value::<Check>(serde_json::json!({
                    "kind": "assert", "path": "inputs.x", "operator": "equals", "expected": true
                }))
                .expect("assert check decodes"),
                provenance: None,
            }],
            steps: Vec::new(),
            evidence: std::collections::BTreeMap::new(),
        };
        let payload = ResultPayloadBuilder::new(
            ResultRun {
                execution_id: ExecutionId::new_v7(),
                run_id: None,
                verifier_version: "1.0.0",
                card_uid: subject,
                owner_card_uid: None,
                group_id: None,
                binding_id: None,
                trigger: None,
                window: Some(&window),
            },
            &verifier,
            now,
            now,
        )
        .build(&outcome)
        .expect("a Drift result builds");
        ScribeWrite::Result {
            payload,
            attribution,
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
    /// results of one run attribution share a single summary frame stamped
    /// with that pair and scoped to the Cards verified, the capture writes its
    /// row and span frames under the call's pair and admitting request, and
    /// the summary frame is submitted last although it was staged first. The
    /// capture's two frames share a phase and arrive in either order.
    ///
    /// # Panics
    ///
    /// Panics when the frames, the summary's position, rows, or attribution
    /// differ.
    #[tokio::test]
    async fn writes_group_by_destination_under_their_attribution() {
        let scribe = Arc::new(RecordingScribe::default());
        let outbox = ScribeSink::local_outbox(Arc::clone(&scribe) as _, None);
        let tenant = DataTenantId::new_v7();
        let attribution = background();
        let subjects = [uid(), uid(), uid()];
        for subject in &subjects {
            outbox.stage(tenant, result(subject, attribution));
        }
        let captured = capture(tenant);
        let request = captured.request_id().clone();
        let call = captured.attribution();
        outbox.stage(tenant, captured);
        settle(&outbox).await;

        let received = scribe.received();
        let mut frames = received
            .iter()
            .map(|frame| (frame.table.as_str(), frame.rows))
            .collect::<Vec<_>>();
        frames[..2].sort_unstable();
        assert_eq!(
            frames,
            [
                ("vala.gateway.calls", 1),
                ("vala.traces.spans", 1),
                ("vala.verification.results", 3),
            ]
        );
        for frame in &received[..2] {
            assert_eq!(frame.attribution, call);
            assert_eq!(frame.request_id, request);
            assert!(frame.card_scope.is_empty());
        }
        assert_eq!(received[2].attribution, attribution);
        assert_eq!(
            received[2].card_scope.iter().collect::<HashSet<_>>(),
            subjects.iter().collect::<HashSet<_>>()
        );
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
            Attribution::direct(PrincipalId::new(uuid::Uuid::now_v7())),
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
        let outbox = ScribeSink::local_outbox(Arc::clone(&scribe) as _, None);
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

    /// Proves frames are sized by the bytes their rows hold, not by each
    /// one-row batch's allocated capacity: a slice of many small decisions
    /// travels as one frame, so it waits for one durable acknowledgement
    /// rather than one per few hundred rows.
    ///
    /// # Panics
    ///
    /// Panics when the slice is split into more than one frame.
    #[tokio::test]
    async fn a_slice_of_small_writes_travels_as_one_frame() {
        const DECISIONS: usize = 2_000;
        let scribe = Arc::new(RecordingScribe::default());
        let outbox = ScribeSink::local_outbox(Arc::clone(&scribe) as _, None);
        let audit: &dyn AuditStage = &*outbox;
        let tenant = DataTenantId::new_v7();
        for _ in 0..DECISIONS {
            audit.stage(tenant, decision("bifrost.record.write"));
        }
        settle(&outbox).await;

        let frames: Vec<_> = scribe
            .audit_frames()
            .into_iter()
            .map(|frame| frame.rows)
            .collect();
        assert_eq!(frames, vec![DECISIONS]);
    }

    /// Proves a retryable refusal replays the identical slice: the call and
    /// span frames are in flight together, so when the call frame is refused
    /// both are resubmitted under their original batch ids, and Scribe ends
    /// up holding each frame once.
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
        let outbox = ScribeSink::local_outbox(Arc::clone(&scribe) as _, None);
        let tenant = DataTenantId::new_v7();
        outbox.stage(tenant, capture(tenant));
        settle(&outbox).await;

        let submitted = scribe.submitted();
        assert_eq!(submitted.len(), 4, "the whole slice is submitted twice");
        // The two frames are in flight together, so each attempt submits
        // them in whichever order their encodings finish.
        let (mut first, mut retry) = (submitted[..2].to_vec(), submitted[2..].to_vec());
        first.sort_unstable();
        retry.sort_unstable();
        assert_eq!(first, retry, "the retry keeps every identity");
        let held = retained(scribe.received())
            .iter()
            .map(|frame| frame.batch_id)
            .collect::<HashSet<_>>();
        assert_eq!(held, submitted[..2].iter().copied().collect::<HashSet<_>>());
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
        let outbox = ScribeSink::local_outbox(Arc::clone(&scribe) as _, None);
        let tenant = DataTenantId::new_v7();
        outbox.stage(tenant, capture(tenant));
        settle(&outbox).await;
        outbox.stage(tenant, capture(tenant));
        settle(&outbox).await;

        // Either capture frame may arrive first and take the refusal, so the
        // first slice leaves one frame and the second writes both.
        let mut tables = scribe
            .received()
            .iter()
            .map(|frame| frame.table.clone())
            .collect::<Vec<_>>();
        assert_eq!(tables.len(), 3, "{tables:?}");
        tables[1..].sort_unstable();
        assert_eq!(tables[1..], ["vala.gateway.calls", "vala.traces.spans"]);
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
        let outbox = outage_outbox();
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

    /// An outbox routed over the peer plane while membership names no ready
    /// Scribe: a sustained outage in which every write stays pending.
    ///
    /// # Panics
    ///
    /// Panics when no peer route is selected.
    fn outage_outbox() -> Arc<ScribeOutbox> {
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
        ScribeSink::outbox(route, BIFROST_INGEST_REQUEST_LIMIT_BYTES)
    }

    /// One tenant span copy routed to `tenant`, and the span it carries.
    ///
    /// # Panics
    ///
    /// Panics when the fixed principal id does not parse.
    fn span_copy(tenant: DataTenantId) -> (TenantSpanRoute, ResourceSpans) {
        let span = Span {
            trace_id: vec![1; 16],
            span_id: vec![2; 8],
            name: "request".to_owned(),
            start_time_unix_nano: 1,
            end_time_unix_nano: 2,
            ..Span::default()
        };
        (
            TenantSpanRoute {
                tenant,
                attribution: Attribution::direct(
                    "01900000-0000-7000-8000-000000000007"
                        .parse()
                        .expect("a principal id"),
                ),
            },
            ResourceSpans {
                scope_spans: vec![ScopeSpans {
                    spans: vec![span],
                    ..ScopeSpans::default()
                }],
                ..ResourceSpans::default()
            },
        )
    }

    /// Proves tenant span staging is capped through a sustained Scribe
    /// outage without starving sibling writes: copies beyond the permit pool
    /// are dropped at once, audit and capture writes still stage, and
    /// abandoning the outbox at shutdown returns every permit. A copy
    /// written once Scribe answers returns its permit too.
    ///
    /// # Panics
    ///
    /// Panics when staging exceeds the cap, a sibling write is refused, or a
    /// permit is not returned.
    #[tokio::test]
    async fn sustained_scribe_outage_caps_tenant_spans() {
        let outbox = outage_outbox();
        let spans = ServerSpanSink::new(Arc::clone(&outbox));
        let tenant = DataTenantId::new_v7();
        for _ in 0..TENANT_SPAN_PERMITS + 16 {
            let (route, copy) = span_copy(tenant);
            spans.stage(route, copy);
        }
        assert_eq!(outbox.pending(), TENANT_SPAN_PERMITS, "the cap holds");
        assert_eq!(spans.permits.available_permits(), 0);
        outbox.stage(tenant, decision("cards.read"));
        outbox.stage(tenant, capture(tenant));
        assert_eq!(
            outbox.pending(),
            TENANT_SPAN_PERMITS + 2,
            "audit and capture take no span permit"
        );
        assert_eq!(
            outbox.shutdown(Instant::now()).await,
            TENANT_SPAN_PERMITS + 2
        );
        assert_eq!(spans.permits.available_permits(), TENANT_SPAN_PERMITS);

        let scribe = Arc::new(RecordingScribe::default());
        let outbox = ScribeSink::local_outbox(Arc::clone(&scribe) as _, None);
        let spans = ServerSpanSink::new(Arc::clone(&outbox));
        let (route, copy) = span_copy(tenant);
        spans.stage(route, copy);
        settle(&outbox).await;
        let received = scribe.received();
        assert_eq!(received.len(), 1, "{received:?}");
        assert_eq!(received[0].table, "vala.traces.spans");
        assert_eq!(received[0].attribution, route.attribution);
        assert_eq!(spans.permits.available_permits(), TENANT_SPAN_PERMITS);
    }

    /// A Scribe request ceiling well below the default, so a modest drift
    /// detail payload needs several frames.
    const SMALL_REQUEST_BYTES: usize = 64 * 1024;

    /// Features scored by the drift judgment whose detail splits into
    /// several frames under [`SMALL_REQUEST_BYTES`] while its summary fits.
    const DETAIL_FEATURES: usize = 128;

    /// Starts an outbox submitting along `route` under a `request_bytes`
    /// Scribe ceiling.
    fn route_outbox(route: ScribeRoute, request_bytes: usize) -> Arc<ScribeOutbox> {
        Outbox::new(
            ScribeSink {
                route: Some(route),
                attempts: AtomicUsize::new(0),
                frame_budget: frame_budget(request_bytes),
                groups: None,
            },
            SCRIBE_WRITERS,
            SCRIBE_CAPACITY,
        )
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

    /// The feature of every drift detail row in `frames`, in order.
    ///
    /// # Panics
    ///
    /// Panics when an item frame does not decode.
    fn detail_features(frames: &[Received]) -> Vec<String> {
        frames
            .iter()
            .filter(|frame| frame.table == "vala.drift.result_features")
            .flat_map(|frame| {
                StreamReader::try_new(std::io::Cursor::new(frame.ipc.clone()), None)
                    .expect("an item frame opens")
                    .flat_map(|batch| {
                        let batch = batch.expect("an item batch decodes");
                        let ids = batch
                            .column_by_name("feature")
                            .expect("feature column")
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

    /// Asserts `frames` carry every one of `features` item rows exactly once,
    /// in more than one item frame within the small ceiling's budget, and end
    /// with the one summary. Item frames are in flight together, so rows are
    /// compared without order.
    ///
    /// # Panics
    ///
    /// Panics when a row is missing or repeated, a frame is over budget, or
    /// the summary is not last.
    fn assert_bounded_detail(frames: &[Received], features: usize) {
        let tables = frames
            .iter()
            .map(|frame| frame.table.as_str())
            .collect::<Vec<_>>();
        let (summary, items) = tables.split_last().expect("frames were written");
        assert_eq!(*summary, "vala.verification.results", "the summary is last");
        assert!(
            items.len() > 1
                && items
                    .iter()
                    .all(|table| *table == "vala.drift.result_features"),
            "the detail splits into several item frames before the summary: {tables:?}"
        );
        let budget = frame_budget(SMALL_REQUEST_BYTES);
        assert!(
            frames.iter().all(|frame| frame.ipc.len() <= budget),
            "every frame fits the budget: {:?}",
            frames
                .iter()
                .map(|frame| (frame.table.as_str(), frame.ipc.len()))
                .collect::<Vec<_>>()
        );
        let expected = (0..features)
            .map(|feature| format!("f{feature:04}"))
            .collect::<Vec<_>>();
        let mut features = detail_features(frames);
        features.sort_unstable();
        assert_eq!(features, expected, "every row once");
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

    /// Proves a multi-row verification detail larger than one frame under a lowered
    /// Scribe ceiling reaches the local Scribe as bounded frames carrying
    /// every row once before the summary, and that after a lost
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
            drift_result(&uid(), background(), DETAIL_FEATURES, 0),
        );
        settle(&outbox).await;

        let submitted = recording.submitted();
        let (mut first, mut retry) = (submitted[..2].to_vec(), submitted[2..4].to_vec());
        first.sort_unstable();
        retry.sort_unstable();
        assert_eq!(retry, first, "the retry resubmits the identical frames");
        let frames = retained(recording.received());
        assert_eq!(
            frames.len(),
            submitted.len() - 2,
            "every frame after the retry is new"
        );
        assert_bounded_detail(&frames, DETAIL_FEATURES);
    }

    /// A Scribe that acknowledges through `inner` after yielding, recording
    /// the most submissions it held at once.
    #[derive(Default)]
    struct Overlapping {
        /// Records every acknowledged frame.
        inner: Arc<RecordingScribe>,
        /// Submissions currently held.
        active: AtomicUsize,
        /// Most submissions held at once.
        peak: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl Scribe for Overlapping {
        /// Always ready.
        fn is_ready(&self) -> bool {
            true
        }

        /// Holds `frame` across a few scheduler yields, so submissions the
        /// sink keeps in flight overlap here, then acknowledges it through
        /// the recording Scribe.
        ///
        /// # Errors
        ///
        /// Returns the recording Scribe's refusal.
        async fn ingest_frame(
            &self,
            frame: ScribeIngressFrame,
        ) -> Result<FrameAdmission, ScribeError> {
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak.fetch_max(active, Ordering::SeqCst);
            for _ in 0..4 {
                tokio::task::yield_now().await;
            }
            self.active.fetch_sub(1, Ordering::SeqCst);
            self.inner.ingest_frame(frame).await
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

    /// Proves a slice's frames await Scribe together rather than one at a
    /// time, never more than [`FRAMES_IN_FLIGHT`] at once, within the frame
    /// budget, and that every summary is acknowledged after every detail row.
    ///
    /// # Panics
    ///
    /// Panics when frames are submitted one at a time, exceed the bound or
    /// the budget, lose a row, or a summary precedes a detail frame.
    #[tokio::test]
    async fn a_slice_keeps_a_bounded_number_of_frames_in_flight() {
        const JUDGMENTS: usize = 8;
        let scribe = Arc::new(Overlapping::default());
        let outbox = route_outbox(
            ScribeRoute::Local(Arc::clone(&scribe) as _),
            SMALL_REQUEST_BYTES,
        );
        let tenant = DataTenantId::new_v7();
        let attribution = background();
        for _ in 0..JUDGMENTS {
            outbox.stage(
                tenant,
                drift_result(&uid(), attribution, DETAIL_FEATURES, 0),
            );
        }
        settle(&outbox).await;

        let frames = scribe.inner.received();
        assert!(
            frames.len() > FRAMES_IN_FLIGHT,
            "the slice needs more frames than may be in flight: {}",
            frames.len()
        );
        let peak = scribe.peak.load(Ordering::SeqCst);
        assert!(
            (2..=FRAMES_IN_FLIGHT).contains(&peak),
            "frames overlap within the bound: {peak}"
        );
        let budget = frame_budget(SMALL_REQUEST_BYTES);
        assert!(frames.iter().all(|frame| frame.ipc.len() <= budget));
        let summaries = frames
            .iter()
            .position(|frame| frame.table == "vala.verification.results")
            .expect("summaries were written");
        let rows = |table: &str| {
            frames
                .iter()
                .filter(|frame| frame.table == table)
                .map(|frame| frame.rows)
                .sum::<usize>()
        };
        assert!(
            frames[summaries..]
                .iter()
                .all(|frame| frame.table == "vala.verification.results"),
            "every summary follows every detail frame"
        );
        assert_eq!(
            rows("vala.drift.result_features"),
            JUDGMENTS * DETAIL_FEATURES
        );
        assert_eq!(rows("vala.verification.results"), JUDGMENTS);
    }

    /// Proves a single row larger than the budget is still submitted alone
    /// and whole, and a terminal Scribe size refusal consumes it without
    /// retry.
    ///
    /// # Panics
    ///
    /// Panics when the row is split, retried, or left staged.
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
            drift_result(&uid(), background(), 0, SMALL_REQUEST_BYTES),
        );
        settle(&outbox).await;

        assert_eq!(
            scribe.attempts(),
            1,
            "the summary row is submitted once, whole"
        );
        assert!(scribe.received().is_empty(), "the refused row is consumed");
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
    /// Scribe, where an oversized verification detail arrives as bounded frames with
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
        outbox.stage(
            tenant,
            drift_result(&uid(), background(), DETAIL_FEATURES, 0),
        );
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
        assert_bounded_detail(&mine, DETAIL_FEATURES);
        let mut others = others
            .iter()
            .map(|frame| frame.table.as_str())
            .collect::<Vec<_>>();
        others.sort_unstable();
        assert_eq!(
            others,
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
