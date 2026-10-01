//! Bounded local and tonic sealed-fragment dispatch.

use std::collections::HashMap;
use std::fmt;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use futures_util::{Stream, StreamExt};
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::api::{
    AnalyticalGraphRef, ExecuteFragmentRequest, FencingToken, NodeId, OracleRoleFence, PeerContext,
    PendingNodeReservation, QueryAuditDigest, QueryClass, QueryId, ReservationId,
    ReservationRejected, ReserveNodeSlotsRequest, ReserveNodeSlotsResponse, WorkerAttemptFrame,
    WorkerFooter, WorkerScanStats,
};
use wyrd_tonic::tonic::Status;
use wyrd_tonic::tonic::transport::Channel;
use wyrd_tonic::wyrd::v1::oracle_peer_service_client::OraclePeerServiceClient;

use super::peer::{
    PeerSecurityError, PeerTicketClaims, ReservationBinding, ReservationOperationV1,
    ReservationTicketClaims, reservation_body_digest,
};
use super::telemetry::{FragmentOutcome, PeerErrorClass, record_peer_attempt};
use crate::cluster::{ClusterRegistry, ClusterSnapshot, ROLE_LIVENESS_CUTOFF};

/// Fixed private peer protocol version carried in every peer context.
///
/// The leader stamps this value into [`PeerTicketClaims::protocol_version`] and
/// the receiver requires it exactly, so a context built by a binary speaking a
/// different peer wire is rejected instead of being decoded against the wrong
/// claim encoding. Both sides read this one constant, so the check cannot
/// desynchronize within a build.
///
/// Protocol v5 replaces signed purpose tickets with unsigned typed contexts
/// carried over the mTLS peer channel; receivers validate every context field
/// against their own trusted state. A v4 peer is refused, never downgraded.
pub const PEER_PROTOCOL_VERSION: u32 = 5;
/// Stable peer rejection hint.
const RESERVATION_RETRY_MS: u32 = 1_000;
/// Acceptance window of one Scribe fragment's peer context.
///
/// Opening a fragment is a single round trip, so the window only has to cover
/// it; the fragment's own deadline still bounds execution separately.
const FRAGMENT_CONTEXT_ACCEPTANCE: ChronoDuration = ChronoDuration::seconds(2);

/// Waits out one explicit stream-accept peer capacity refusal before a leader
/// retries placement.
///
/// The wait is the refusing peer's own `retry_after_ms` hint, bounded by the
/// leader's absolute `deadline` and cut short by `cancel`. A hint that would
/// carry the retry past the deadline is not waited at all: the query could
/// not use the slot, so the leader stops now instead of sleeping into its own
/// timeout. Stateless by design — the leader owns the deadline, cancellation,
/// and every held grant it must close before calling this.
///
/// Returns `true` when the leader should retry placement, and `false` when
/// cancellation or the deadline ends the retry.
pub(super) async fn wait_for_peer_capacity(
    rejected: ReservationRejected,
    deadline: Instant,
    cancel: &CancellationToken,
) -> bool {
    let wake =
        Instant::now() + std::time::Duration::from_millis(u64::from(rejected.retry_after_ms));
    if wake >= deadline {
        return false;
    }
    tokio::select! {
        biased;
        () = cancel.cancelled() => false,
        () = tokio::time::sleep_until(wake) => true,
    }
}

/// Closed transport failure classification used by terminal dispatch policy.
#[derive(Debug, Error)]
pub enum DispatchError {
    /// Worker, transport, deadline, or cancellation made this source unavailable.
    #[error("peer attempt unavailable")]
    Unavailable,
    /// A role-local provider reported an authenticated source absence before rows.
    #[error("eligible follower source is unavailable: {cause:?}")]
    EligibleSourceLoss {
        /// Closed provider-side reason retained for terminal classification.
        cause: EligibleSourceLossCause,
    },
    /// The selected attempt could not reserve bounded parent memory.
    #[error("peer attempt capacity unavailable")]
    Capacity,
    /// A pinned immutable object disappeared and requires a whole-query replan.
    #[error("peer fragment references a stale object")]
    StaleObject,
    /// A delivered follower reported the pinned Parquet file-not-found partial.
    #[error("search parquet file not found")]
    FileNotFound,
    /// Ticket or fragment contract failed terminally.
    #[error("peer security or fragment contract rejected")]
    Terminal,
    /// A peer refused a scanned file whose footer tenant was missing or foreign.
    ///
    /// Kept distinct from [`DispatchError::Terminal`] so the leader reports
    /// the tenant-isolation reason rather than a generic peer-security or
    /// retryable-worker outcome. It is never retried on another candidate:
    /// the refusal is a property of the data, not of the worker.
    #[error("peer fragment refused a foreign-tenant file")]
    TenantInvariant,
}

/// Closed causes that permit explicit degraded source completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EligibleSourceLossCause {
    /// The authenticated role-local provider could not resolve its pinned source.
    ProviderResolution,
}

/// One follower grant, alive exactly as long as its leader's grant stream.
///
/// There is no expiry. The entry is created when this node accepts a leader's
/// grant stream and removed when that stream ends — the leader closed or
/// dropped it, the query deadline passed, or the node is shutting down. Until
/// a graph activates from it, the entry owns the charged query envelope;
/// afterwards the graph owns the envelope and the entry keeps only the signal
/// that ends the graph when the stream does.
#[derive(Debug)]
struct HeldGrant {
    /// Query identity bound to the grant.
    query_id: QueryId,
    /// Leader identity bound to the grant.
    leader_node_id: NodeId,
    /// Leader fence the grant was accepted under.
    leader_fencing_token: FencingToken,
    /// Query envelope this node charged when it accepted the grant stream.
    ///
    /// The envelope holds this node's slot unit in the shared governor ledger
    /// and its Oracle memory, so a granted graph can always execute: a leader
    /// never plans over a node that has not already seated it. `None` once a
    /// graph has taken it, which also makes a grant single-use.
    envelope: Option<Box<crate::resources::OracleQueryResources>>,
    /// Graph this grant may only ever be leased to.
    graph: AnalyticalGraphRef,
    /// Cancelled when the grant stream ends, which ends any graph built on it.
    closed: CancellationToken,
}

/// Everything a follower must prove before one grant becomes a graph.
///
/// A graph lease is the largest thing a peer can be talked into charging, so
/// the grant must have been taken for exactly this query and exactly this
/// graph before its envelope changes owner. Nothing here is read from the wire
/// framing; the caller projects every field from verified stage claims.
///
/// The grant's leader identity and fence are deliberately not re-derived
/// here. They were checked when the grant stream was accepted, and the
/// coordinator that presents the first stage message for a graph is not always
/// the leader — a follower running a middle stage is itself a coordinator and
/// legitimately signs under its own identity. The stage ticket independently
/// binds the presenting principal, this follower's node identity, and its
/// current role fence before this is ever reached. The activator retains that
/// granting identity separately, in its own immutable graph binding.
#[derive(Debug, Clone, Copy)]
pub struct GraphLeaseRequest {
    /// Grant the leader holds open on this node for this graph.
    pub reservation_id: ReservationId,
    /// Exact graph the grant was taken for.
    pub graph: AnalyticalGraphRef,
    /// Query identity the grant was bound to.
    pub query_id: QueryId,
}

/// One follower grant's envelope held across a fallible graph activation.
///
/// The envelope is taken out of the grant and retained here unchanged while
/// the activator does the fallible work: building the query runtime and
/// registering the graph with its supervisor. Exactly one of
/// [`PendingGraphActivation::commit`] or [`PendingGraphActivation::rollback`]
/// then decides whether the envelope became a graph or goes back to its grant,
/// so no failure path can leave the follower charged for a graph that does not
/// exist, and no failure path can destroy a grant whose stream is still open.
pub struct PendingGraphActivation {
    /// Registry the envelope is returned to when activation fails.
    registry: Arc<ReservationRegistry>,
    /// Identity of the grant this activation draws from.
    reservation_id: ReservationId,
    /// Graph the grant may only ever become.
    graph: AnalyticalGraphRef,
    /// Leader node and fence the grant was accepted under.
    granting_leader: (NodeId, FencingToken),
    /// Signal the grant stream's end cancels.
    closed: CancellationToken,
    /// The grant's envelope, retained verbatim until commit or rollback.
    ///
    /// Cleared by whichever of the two runs, so the drop guard can tell an
    /// abandoned activation from a settled one.
    envelope: Option<Box<crate::resources::OracleQueryResources>>,
}

impl fmt::Debug for PendingGraphActivation {
    /// Reports the activation's identity without rendering retained ownership.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PendingGraphActivation")
            .field("reservation_id", &self.reservation_id.as_uuid())
            .field("settled", &self.envelope.is_none())
            .finish_non_exhaustive()
    }
}

impl PendingGraphActivation {
    /// Returns the leader node and fence the grant was accepted under.
    ///
    /// This is grant ownership, not per-message stage authority. The activator
    /// retains it so a later coordinator can be authorized as either this
    /// exact pair or an exact member of the immutable destination cut.
    #[must_use]
    pub(crate) const fn reserving_leader(&self) -> (NodeId, FencingToken) {
        self.granting_leader
    }

    /// Returns the signal cancelled when the grant's leader stream ends.
    ///
    /// The activated graph keeps it, so the follower's settlement driver can
    /// end the graph the moment its leader's stream does.
    #[must_use]
    pub(crate) fn closed(&self) -> CancellationToken {
        self.closed.clone()
    }

    /// Borrows the granted query envelope without taking ownership of it.
    ///
    /// Building the graph's bounded runtime needs only the envelope's pool and
    /// scratch share, and it can fail. Lending rather than taking is what keeps
    /// a failed runtime build recoverable: the envelope is still here, so the
    /// rollback returns it to a grant a later activator can still use.
    ///
    /// # Panics
    ///
    /// Panics when the activation has already committed or rolled back, which
    /// is unreachable: both consume `self`.
    #[must_use]
    pub(crate) fn envelope(&self) -> &crate::resources::OracleQueryResources {
        self.envelope
            .as_ref()
            .expect("a live activation owns its envelope")
    }

    /// Moves the granted envelope into `register`, keeping it on failure.
    ///
    /// `register` is the single fallible act that changes the envelope's owner:
    /// it takes the admitted resources and returns whatever owns them from then
    /// on — in production, the supervisor's graph guard. A registration that
    /// fails must hand the resources back, because the grant this activation
    /// returns them to is only usable again if it is restored complete.
    ///
    /// On success the activation's cumulative counter is advanced exactly once.
    ///
    /// # Errors
    ///
    /// Returns the unchanged activation alongside `register`'s error, so the
    /// caller can still roll back.
    ///
    /// # Panics
    ///
    /// Panics when the activation has already settled, which is unreachable:
    /// both settling paths consume `self`.
    pub(crate) fn commit<T, F>(mut self, register: F) -> Result<T, (Box<Self>, BifrostError)>
    where
        F: FnOnce(
            crate::resources::OracleQueryResources,
        )
            -> Result<T, (Box<crate::resources::OracleQueryResources>, BifrostError)>,
    {
        let envelope = self
            .envelope
            .take()
            .expect("a live activation owns its envelope");
        match register(*envelope) {
            Ok(owner) => {
                #[cfg(any(test, feature = "test-support"))]
                self.registry
                    .graph_leases_activated_total
                    .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                tracing::debug!(
                    public_query_id = %self.graph.public_query_id,
                    datafusion_query_id = %self.graph.datafusion_query_id,
                    "Oracle graph lease activated from its held grant"
                );
                Ok(owner)
            }
            Err((resources, error)) => {
                self.envelope = Some(resources);
                Err((Box::new(self), error))
            }
        }
    }

    /// Returns the unchanged envelope to its grant, or drops it.
    ///
    /// The envelope goes back only while the grant's stream is still open; a
    /// grant whose stream ended meanwhile is already gone, and dropping the
    /// envelope then returns this follower to baseline rather than stranding
    /// capacity for a graph that never existed.
    pub(crate) fn rollback(mut self) {
        if let Some(envelope) = self.envelope.take() {
            self.registry.restore(self.reservation_id, envelope);
        }
    }
}

impl Drop for PendingGraphActivation {
    /// Rolls back an activation abandoned by cancellation, panic, or early return.
    ///
    /// The settled paths clear the envelope and leave nothing to do here. This
    /// covers the activator that simply went away, where the alternative is an
    /// envelope no owner can ever return.
    fn drop(&mut self) {
        if let Some(envelope) = self.envelope.take() {
            self.registry.restore(self.reservation_id, envelope);
        }
    }
}

/// In-memory owner of this follower's held grants; entries are never durable.
///
/// Every entry belongs to one open leader grant stream and is removed when
/// that stream ends. Nothing here expires on a timer and nothing waits for a
/// later request to reclaim it.
#[derive(Debug)]
pub struct ReservationRegistry {
    /// Held grants keyed by their unguessable identities.
    entries: Mutex<HashMap<ReservationId, HeldGrant>>,
    /// Hard bound on retained held grants for this worker role.
    capacity: usize,
    /// Immutable local slot-unit total, used only for placement and bounds.
    ///
    /// A capacity figure, never a gate: the shared governor ledger decides
    /// whether a unit is free. A full node refuses before accepting work and
    /// the leader owns any retry.
    total_slot_units: usize,
    /// Wakes the follower settlement driver when any grant stream ends.
    ///
    /// A stored permit, never a lost wake: a close that lands while the driver
    /// is busy is observed on its next pass.
    grant_closed: Arc<tokio::sync::Notify>,
    /// Cumulative count of graph leases this node activated from a grant.
    ///
    /// Incremented only on the first activation for a graph, never on reuse, so
    /// an integration test can assert the exactness the lease claims: one
    /// distributed plan charges one envelope on each follower regardless of how
    /// many stage messages, tasks, or retries address it. `test-support`-gated;
    /// no field, cost, or behavior exists on the production path.
    #[cfg(any(test, feature = "test-support"))]
    graph_leases_activated_total: core::sync::atomic::AtomicU64,
}

impl ReservationRegistry {
    /// Creates a bounded registry for one fenced Oracle role.
    #[must_use]
    pub fn new(total_slot_units: usize, capacity: usize) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            capacity,
            total_slot_units,
            grant_closed: Arc::new(tokio::sync::Notify::new()),
            #[cfg(any(test, feature = "test-support"))]
            graph_leases_activated_total: core::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Records one held grant and returns the guard its stream owns.
    ///
    /// `envelope` was already charged against the shared governor ledger, so
    /// there is no second local semaphore to clamp leader-supplied demand
    /// against here. The returned guard is the grant's only owner: dropping it
    /// removes the entry, returns an unused envelope, and ends any graph built
    /// on it.
    ///
    /// # Errors
    ///
    /// Returns terminal for an already-passed query deadline, and retryable
    /// when the bounded registry is unavailable or full. A refused `envelope`
    /// is dropped, returning its capacity.
    pub(crate) fn hold(
        self: &Arc<Self>,
        request: &ReserveNodeSlotsRequest,
        now: DateTime<Utc>,
        envelope: Box<crate::resources::OracleQueryResources>,
    ) -> Result<HeldGraphGrant, DispatchError> {
        if request.expires_at <= now {
            return Err(DispatchError::Terminal);
        }
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| DispatchError::Unavailable)?;
        if entries.len() >= self.capacity {
            return Err(DispatchError::Unavailable);
        }
        let reservation_id = loop {
            let candidate = ReservationId::new(uuid::Uuid::now_v7());
            if !entries.contains_key(&candidate) {
                break candidate;
            }
        };
        entries.insert(
            reservation_id,
            HeldGrant {
                query_id: request.query_id,
                leader_node_id: request.leader_node_id,
                leader_fencing_token: request.leader_fencing_token,
                envelope: Some(envelope),
                graph: request.graph,
                closed: CancellationToken::new(),
            },
        );
        Ok(HeldGraphGrant {
            registry: Arc::clone(self),
            reservation_id,
        })
    }

    /// Ends one grant because its leader stream ended.
    ///
    /// Removes the entry, drops an envelope no graph took, and cancels the
    /// grant's signal so a graph built on it is drained at once. Called only by
    /// [`HeldGraphGrant`]'s `Drop`, so it runs exactly once per grant.
    fn close(&self, reservation_id: ReservationId) {
        let removed = self
            .entries
            .lock()
            .ok()
            .and_then(|mut entries| entries.remove(&reservation_id));
        if let Some(grant) = removed {
            grant.closed.cancel();
            self.grant_closed.notify_one();
        }
    }

    /// Ends every held grant, returning each unused envelope's capacity.
    ///
    /// Shutdown calls this after new work is refused: no leader can still use
    /// a grant on a node that is going away, so its envelope is returned now
    /// rather than when the leader's stream happens to close. Graphs built on
    /// the grants are signalled exactly as a closed stream would signal them.
    /// Returns how many grants were ended.
    pub fn close_all(&self) -> usize {
        let Ok(mut entries) = self.entries.lock() else {
            return 0;
        };
        let closed = entries.len();
        for (_, grant) in entries.drain() {
            grant.closed.cancel();
        }
        if closed != 0 {
            self.grant_closed.notify_one();
        }
        closed
    }

    /// Opens one rollback-capable activation of a held grant into its graph.
    ///
    /// This is the only path from a grant to a graph envelope. The complete
    /// ownership tuple is checked *before* the envelope moves anywhere, and
    /// what the caller receives is a transaction rather than a lease: until it
    /// commits, the envelope still belongs to the grant.
    ///
    /// Activation refuses before any worker, cache, or provider IO when the
    /// grant is missing, already spent, or was taken for a different query or
    /// graph.
    ///
    /// # Errors
    ///
    /// Returns [`DispatchError::Terminal`] for a missing, spent, or mismatched
    /// grant, and [`DispatchError::Unavailable`] when the registry lock is
    /// poisoned.
    #[tracing::instrument(name = "bifrost.oracle.graph_lease", skip_all)]
    pub(crate) fn begin_graph_activation(
        self: &Arc<Self>,
        request: &GraphLeaseRequest,
    ) -> Result<PendingGraphActivation, DispatchError> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| DispatchError::Unavailable)?;
        let grant = entries
            .get_mut(&request.reservation_id)
            .ok_or(DispatchError::Terminal)?;
        if grant.query_id != request.query_id || grant.graph != request.graph {
            return Err(DispatchError::Terminal);
        }
        let envelope = grant.envelope.take().ok_or(DispatchError::Terminal)?;
        Ok(PendingGraphActivation {
            registry: Arc::clone(self),
            reservation_id: request.reservation_id,
            graph: request.graph,
            granting_leader: (grant.leader_node_id, grant.leader_fencing_token),
            closed: grant.closed.clone(),
            envelope: Some(envelope),
        })
    }

    /// Returns one unchanged envelope to its grant, or drops it.
    ///
    /// Centralized here rather than in the activator because the grant's
    /// lifetime is the registry's own invariant: an envelope may only return
    /// to a grant whose stream is still open and that holds no envelope.
    /// Otherwise it is dropped, which is the honest outcome — the grant the
    /// leader held is simply over.
    fn restore(
        &self,
        reservation_id: ReservationId,
        envelope: Box<crate::resources::OracleQueryResources>,
    ) {
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        if let Some(grant) = entries.get_mut(&reservation_id)
            && grant.envelope.is_none()
        {
            grant.envelope = Some(envelope);
        }
    }

    /// Returns the signal the follower settlement driver waits on for closes.
    #[must_use]
    pub(crate) fn grant_closed(&self) -> Arc<tokio::sync::Notify> {
        Arc::clone(&self.grant_closed)
    }

    /// Returns how many grant streams this node is currently holding open.
    ///
    /// Test-tier visibility for the rule that a follower holds nothing once
    /// its leader's stream has ended.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn held_grants(&self) -> usize {
        self.entries.lock().map_or(0, |entries| entries.len())
    }

    /// Returns this node's immutable local slot-unit total.
    ///
    /// Test-tier readiness inspection reads capacity here because the registry
    /// owns the figure.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub(crate) fn total_slot_units(&self) -> usize {
        self.total_slot_units
    }

    /// Returns the greatest number of graphs this node may own at one time.
    ///
    /// Every graph holds one slot unit, so this is the pod's immutable local
    /// slot-unit total: a bounded queue sized from it cannot exceed what the
    /// shared governor ledger could ever admit. Never zero: a node that can
    /// admit one graph must be able to settle it.
    #[must_use]
    pub(crate) fn max_concurrent_graphs(&self) -> usize {
        self.total_slot_units.max(1)
    }

    /// Returns the cumulative count of graph leases activated on this node.
    ///
    /// Integration-only observable. Counts activations, never reuse, so a test
    /// can assert that one distributed plan charged one envelope per follower.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn graph_leases_activated_total(&self) -> u64 {
        self.graph_leases_activated_total
            .load(core::sync::atomic::Ordering::Relaxed)
    }
}

/// Follower-side owner of one held grant, carried by the leader's grant stream.
///
/// Dropping it is the release: the follower's stream handler drops it when
/// the leader closes or drops the stream, when the query deadline passes, or
/// when the node shuts down. Nothing else releases a grant.
#[derive(Debug)]
pub struct HeldGraphGrant {
    /// Registry the grant lives in.
    registry: Arc<ReservationRegistry>,
    /// Identity of the held grant.
    reservation_id: ReservationId,
}

impl HeldGraphGrant {
    /// Returns the accepted grant the leader is told about.
    #[must_use]
    pub const fn reservation(&self) -> PendingNodeReservation {
        PendingNodeReservation {
            reservation_id: self.reservation_id,
        }
    }
}

impl Drop for HeldGraphGrant {
    /// Ends the grant with its stream, synchronously and without blocking.
    fn drop(&mut self) {
        self.registry.close(self.reservation_id);
    }
}

/// Leader-side handle to one grant a participant accepted.
///
/// It owns the open grant stream, so dropping it is how the leader releases
/// the participant: the follower observes the stream end and frees the grant
/// and anything built on it at once. There is no release request to send,
/// retry, or lose.
pub struct ParticipantGrant {
    /// Grant identity the participant minted; bound into stage claims.
    reservation_id: ReservationId,
    /// The open stream; dropping it ends the grant on the participant.
    _stream: Box<dyn Send>,
}

impl fmt::Debug for ParticipantGrant {
    /// Reports the grant identity without rendering the stream.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ParticipantGrant")
            .field("reservation_id", &self.reservation_id.as_uuid())
            .finish_non_exhaustive()
    }
}

impl ParticipantGrant {
    /// Wraps one accepted grant and the stream that keeps it open.
    #[must_use]
    pub fn new(reservation_id: ReservationId, stream: Box<dyn Send>) -> Self {
        Self {
            reservation_id,
            _stream: stream,
        }
    }

    /// Returns the participant-minted grant identity.
    #[must_use]
    pub const fn reservation_id(&self) -> ReservationId {
        self.reservation_id
    }
}

/// One item of a Scribe fragment's output as the leader receives it.
///
/// A remote Scribe sends encoded attempt frames; a Scribe in the leader's own
/// process hands over its Arrow batches and completion directly, so nothing
/// is encoded, decoded, or hashed on that path.
pub enum LiveFrame {
    /// One attempt frame from a remote peer.
    Wire(WorkerAttemptFrame),
    /// One in-process result batch.
    Batch(RecordBatch),
    /// Successful in-process completion; nothing may follow it.
    Complete(NativeCompletion),
}

/// Output evidence closing one in-process Scribe fragment.
///
/// It is the native counterpart of a remote [`WorkerFooter`]: the leader
/// reconciles the totals against what it received before accepting success,
/// without any encoding or hashing on either side. Bytes follow the native
/// convention, the sum of each batch's [`RecordBatch::get_array_memory_size`];
/// it is transfer accounting, not a memory charge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeCompletion {
    /// Authenticated plan fingerprint of the fragment that produced the output.
    pub plan_fingerprint: String,
    /// Rows the producer delivered.
    pub rows: u64,
    /// Native Arrow bytes the producer delivered.
    pub bytes: u64,
    /// Follower's finalized scan evidence.
    pub scan_stats: WorkerScanStats,
}

/// Producer-side running totals for one in-process fragment's output.
///
/// The Scribe executor records every batch it hands over and closes the
/// fragment with [`NativeOutputTally::complete`], so the completion carries
/// exactly what was delivered.
#[derive(Debug, Default)]
pub struct NativeOutputTally {
    /// Rows delivered so far.
    rows: u64,
    /// Native Arrow bytes delivered so far.
    bytes: u64,
}

impl NativeOutputTally {
    /// Adds one delivered batch's rows and native bytes.
    ///
    /// # Errors
    /// Returns [`DispatchError::Terminal`] when either total would overflow.
    pub fn record(&mut self, batch: &RecordBatch) -> Result<(), DispatchError> {
        let add = |total: u64, amount: usize| {
            u64::try_from(amount)
                .ok()
                .and_then(|amount| total.checked_add(amount))
                .ok_or(DispatchError::Terminal)
        };
        self.rows = add(self.rows, batch.num_rows())?;
        self.bytes = add(self.bytes, batch.get_array_memory_size())?;
        Ok(())
    }

    /// Closes the fragment with its delivered totals and scan evidence.
    #[must_use]
    pub fn complete(self, plan_fingerprint: String, scan_stats: WorkerScanStats) -> LiveFrame {
        LiveFrame::Complete(NativeCompletion {
            plan_fingerprint,
            rows: self.rows,
            bytes: self.bytes,
            scan_stats,
        })
    }
}

/// Incremental dispatch stream shared by local and tonic transports.
pub type WorkerAttemptStream = Pin<Box<dyn Stream<Item = Result<LiveFrame, DispatchError>> + Send>>;

/// Worker-side owner of this node's Analytical graph grants.
///
/// A peer leader opens a grant stream here before it plans a distributed graph
/// over this node. Accepting the stream charges a whole query envelope; the
/// envelope belongs to that stream until the graph's first stage activates it,
/// and the graph belongs to it afterwards. The stream ending ends both.
pub struct OraclePeerWorker {
    /// Grants this node is holding for open leader streams.
    reservations: Arc<ReservationRegistry>,
    /// Root Oracle capability every granted graph envelope is charged against.
    oracle_resources: crate::resources::OracleResources,
}

impl OraclePeerWorker {
    /// Creates the worker over this node's grant registry and Oracle capability.
    #[must_use]
    pub const fn new(
        reservations: Arc<ReservationRegistry>,
        oracle_resources: crate::resources::OracleResources,
    ) -> Self {
        Self {
            reservations,
            oracle_resources,
        }
    }

    /// Admits one graph grant for one fenced leader's stream, or refuses at once.
    ///
    /// Accepting the stream is the single admission gate: it charges the query
    /// envelope the graph will later execute under, so a leader holding a grant
    /// on every participant knows every participant can run. A node whose
    /// capacity is full refuses with its retry hint before accepting any work.
    /// It never waits on the leader's behalf: several leaders may target this
    /// node, and only a leader knows its own deadline, so the leader owns the
    /// bounded retry.
    ///
    /// The caller's stream must own the returned guard; dropping it is the
    /// grant's release.
    ///
    /// # Errors
    ///
    /// Returns the refusal and its retry hint when the envelope cannot be
    /// charged or the grant cannot be recorded.
    pub fn reserve(
        &self,
        request: &ReserveNodeSlotsRequest,
    ) -> Result<HeldGraphGrant, ReservationRejected> {
        self.acquire_graph_envelope()
            .and_then(|envelope| self.reservations.hold(request, Utc::now(), envelope))
            .map_err(|_| {
                tracing::warn!(stage = "slot_reservation", "oracle peer capacity rejection");
                ReservationRejected {
                    retry_after_ms: RESERVATION_RETRY_MS,
                }
            })
    }

    /// Returns grant streams this node holds, for integration-only assertions.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn held_grants(&self) -> usize {
        self.reservations.held_grants()
    }

    /// Charges the query envelope one granted graph will execute under.
    ///
    /// A graph runs a whole distributed plan on this node — several stages,
    /// their exchanges, and their spill — so it charges a full query envelope.
    /// The local ratio is zero because none of the leader's own scan work runs
    /// here.
    ///
    /// # Errors
    ///
    /// Returns [`DispatchError::Capacity`] when the Oracle resources cannot
    /// admit the envelope.
    fn acquire_graph_envelope(
        &self,
    ) -> Result<Box<crate::resources::OracleQueryResources>, DispatchError> {
        self.oracle_resources
            .try_acquire_query(crate::resources::OracleResourceRequest::for_class(
                QueryClass::Analytical,
                0.0,
            ))
            .map(Box::new)
            .map_err(|_| DispatchError::Capacity)
    }
}

/// Builds the typed peer-context claims delivered to one dispatch candidate.
///
/// The context's acceptance window closes at the earlier of `accept_until` and
/// the query's own absolute deadline, so a peer can never accept work past
/// either bound. The fragment digest, manifest digest, and projection digest all carry
/// the same plan fingerprint: the follower validates one sealed fragment, and
/// splitting these into distinct digests would imply a per-stage binding the
/// protocol does not have.
///
/// The assignment-authority digest is computed here, over the exact assignments
/// this fragment will dispatch, so protocol v2 followers can recompute it from
/// what they physically received and refuse a closure that was altered after
/// the leader built it.
///
/// # Errors
///
/// Returns [`DispatchError::Terminal`] when the fragment's assignments cannot
/// produce a canonical authority digest; such a fragment must never be sent.
fn peer_ticket_claims(
    candidate: &DispatchCandidate,
    context: &DispatchContext,
    fragment: &PhysicalDispatchFragment,
    accept_until: DateTime<Utc>,
) -> Result<PeerTicketClaims, DispatchError> {
    Ok(PeerTicketClaims {
        protocol_version: PEER_PROTOCOL_VERSION,
        audience: candidate.node_id.as_uuid().as_bytes().to_vec(),
        worker_fence: candidate.worker_fence,
        leader_node_id: context.leader_node_id.as_uuid().as_bytes().to_vec(),
        leader_fence: context.leader_fence,
        query_id: context.query_id.as_uuid().as_bytes().to_vec(),
        tenant_id: context.tenant_id.as_bytes().to_vec(),
        expires_at_ms: accept_until
            .timestamp_millis()
            .min(fragment.deadline_unix_ms),
        execution_deadline_unix_ms: fragment.deadline_unix_ms,
        binding: format!("{}.{}", fragment.binding.namespace, fragment.binding.table),
        fragment_digest: fragment.plan_fingerprint.clone(),
        manifest_digest: fragment.plan_fingerprint.clone(),
        projection_digest: fragment.plan_fingerprint.clone(),
        permission_digest: context.permission_digest.clone(),
        assignment_authority_digest: super::peer::assignment_authority_digest_for(
            &fragment.assignments,
        )
        .map_err(|_| DispatchError::Terminal)?,
    })
}

/// Attempt-encoding failure raised while framing one follower batch stream.
///
/// The encoder owns only the Arrow IPC framing boundary, so its closed set is
/// narrower than a scan failure: every variant means the worker could not turn
/// already-decoded rows into the peer attempt protocol. Callers map the whole
/// enum onto [`DispatchError::Terminal`] because none of them is retryable on
/// another candidate.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AttemptEncodeError {
    /// The attempt produced no schema frame, so it has no footer to finalize.
    #[error("attempt produced no schema frame")]
    Empty,
    /// A later batch changed the immutable attempt schema.
    #[error("attempt batch schema changed mid-stream")]
    Schema,
    /// Arrow IPC encoding or a checked counter failed.
    #[error("attempt batch encoding failed")]
    Encode,
    /// A footer digest could not be constructed from its hex preimage.
    #[error("attempt footer digest is malformed")]
    Digest,
}

/// Stateful footer encoder for one incremental worker attempt.
///
/// The encoder is driven once per attempt: [`AttemptEncoder::start`] emits the
/// immutable schema frame, [`AttemptEncoder::encode`] emits one frame per
/// decoded batch while hashing the payload in stream order, and
/// [`AttemptEncoder::finish_physical`] seals the running row, byte, and payload
/// counters into the footer the leader validates. It holds no IO and no
/// reservation; the surrounding stream owns both.
#[derive(Debug, Default)]
pub struct AttemptEncoder {
    /// Schema emitted exactly once before the first batch.
    schema: Option<arrow::datatypes::SchemaRef>,
    /// Incremental hash over encoded batch payloads in stream order.
    payload_hash: Sha256,
    /// Total encoded schema and batch bytes.
    encoded_bytes: usize,
    /// Total rows emitted across batch frames.
    row_count: u64,
}

impl AttemptEncoder {
    /// Starts an attempt with its immutable output schema, including empty results.
    ///
    /// # Errors
    /// Returns [`AttemptEncodeError::Encode`] when Arrow IPC schema encoding fails.
    pub fn start(
        &mut self,
        schema: arrow::datatypes::SchemaRef,
    ) -> Result<WorkerAttemptFrame, AttemptEncodeError> {
        let mut bytes = Vec::new();
        StreamWriter::try_new(&mut bytes, &schema)
            .and_then(|mut writer| writer.finish())
            .map_err(|_| AttemptEncodeError::Encode)?;
        self.encoded_bytes = bytes.len();
        self.schema = Some(schema);
        Ok(WorkerAttemptFrame::Schema(bytes))
    }

    /// Encodes one batch and returns its optional first-schema frame plus batch frame.
    ///
    /// # Errors
    ///
    /// Returns [`AttemptEncodeError::Encode`] when Arrow IPC encoding or checked
    /// counters fail, and [`AttemptEncodeError::Schema`] if later batches change
    /// schema.
    pub fn encode(
        &mut self,
        batch: &RecordBatch,
    ) -> Result<(Option<WorkerAttemptFrame>, WorkerAttemptFrame), AttemptEncodeError> {
        let schema_frame = if let Some(schema) = &self.schema {
            if schema.as_ref() != batch.schema().as_ref() {
                return Err(AttemptEncodeError::Schema);
            }
            None
        } else {
            let schema = batch.schema();
            let mut bytes = Vec::new();
            StreamWriter::try_new(&mut bytes, &schema)
                .and_then(|mut writer| writer.finish())
                .map_err(|_| AttemptEncodeError::Encode)?;
            self.encoded_bytes = bytes.len();
            self.schema = Some(schema);
            Some(WorkerAttemptFrame::Schema(bytes))
        };
        let mut bytes = Vec::new();
        StreamWriter::try_new(&mut bytes, &batch.schema())
            .and_then(|mut writer| {
                writer.write(batch)?;
                writer.finish()
            })
            .map_err(|_| AttemptEncodeError::Encode)?;
        self.payload_hash.update(&bytes);
        self.encoded_bytes = self
            .encoded_bytes
            .checked_add(bytes.len())
            .ok_or(AttemptEncodeError::Encode)?;
        self.row_count = self
            .row_count
            .checked_add(u64::try_from(batch.num_rows()).map_err(|_| AttemptEncodeError::Encode)?)
            .ok_or(AttemptEncodeError::Encode)?;
        Ok((schema_frame, WorkerAttemptFrame::Batch(bytes)))
    }

    /// Finalizes a native physical-plan attempt under its immutable fingerprint.
    ///
    /// `scan_stats` is the follower's finalized physical read volume, which the
    /// leader sums across the participant cut because its own plan scans no
    /// storage.
    ///
    /// # Errors
    /// Returns a closed empty, digest, or checked byte-conversion failure.
    pub fn finish_physical(
        self,
        plan_fingerprint: &str,
        scan_stats: WorkerScanStats,
    ) -> Result<WorkerAttemptFrame, AttemptEncodeError> {
        if self.schema.is_none() {
            return Err(AttemptEncodeError::Empty);
        }
        let digest = QueryAuditDigest::new(plan_fingerprint.to_owned())
            .map_err(|_| AttemptEncodeError::Digest)?;
        let payload_digest = QueryAuditDigest::new(hex::encode(self.payload_hash.finalize()))
            .map_err(|_| AttemptEncodeError::Digest)?;
        Ok(WorkerAttemptFrame::Footer(WorkerFooter {
            fragment_id: plan_fingerprint.to_owned(),
            manifest_digest: digest,
            row_count: self.row_count,
            encoded_bytes: u64::try_from(self.encoded_bytes)
                .map_err(|_| AttemptEncodeError::Encode)?,
            payload_digest,
            completed: true,
            scan_stats,
        }))
    }
}

/// One adapter contract used identically by local and tonic dispatch.
#[async_trait]
pub trait OraclePeerTransport: Send + Sync {
    /// Opens one held graph grant on one worker.
    ///
    /// The inner result separates an accepted grant, which stays held until
    /// the returned handle is dropped, from an explicit stream-accept refusal
    /// carrying the worker's retry hint.
    ///
    /// # Errors
    /// Returns retryable availability or terminal contract failure.
    async fn reserve(
        &self,
        worker: NodeId,
        request: ReserveNodeSlotsRequest,
    ) -> Result<Result<ParticipantGrant, ReservationRejected>, DispatchError>;

    /// Executes one ticket-bound fragment and returns footer-terminated frames.
    ///
    /// # Errors
    /// Returns retryable availability or terminal security/contract failure.
    async fn execute(
        &self,
        worker: NodeId,
        request: ExecuteFragmentRequest,
    ) -> Result<WorkerAttemptStream, DispatchError>;
}

/// Acceptance window for one grant-stream context, in seconds.
///
/// Short by design: the context is checked once, when the follower accepts
/// the stream, so the window only has to cover that round trip. It does not
/// bound how long the grant is held.
const RESERVATION_CONTEXT_TTL_SECONDS: i64 = 10;

/// Real tonic client transport resolving Oracle peers from live membership.
///
/// Every call runs over the mTLS peer channel built from [`BifrostPeerTls`];
/// no per-call credential exists. Receivers validate the typed context against
/// their own trusted state.
pub struct TonicOraclePeerTransport {
    /// Existing registry publishing immutable ready/live membership cuts.
    topology: OraclePeerTopology,
    /// Immutable peer CA and client identity every channel is dialed with.
    tls: BifrostPeerTls,
    /// Established mTLS channel per peer, keyed with what it dialed.
    ///
    /// A channel is reused only while the candidate names the same role fence
    /// and endpoint, so a restarted or relocated peer is dialed and
    /// authenticated afresh instead of inheriting its predecessor's channel.
    channels: Mutex<HashMap<NodeId, PeerChannel>>,
}

/// One established peer channel and the exact identity it was dialed for.
#[derive(Clone)]
struct PeerChannel {
    /// Role fence of the peer incarnation this channel authenticated.
    fence: FencingToken,
    /// Endpoint the channel dialed.
    address: String,
    /// Multiplexed tonic channel shared by every call to that incarnation.
    channel: Channel,
}

/// Membership source used by production and feature-gated transport fixtures.
enum OraclePeerTopology {
    /// Production's continuously refreshed authoritative registry.
    Registry(Arc<ClusterRegistry>),
    /// Immutable fixture projection used only by transport-focused tests.
    #[cfg(feature = "test-support")]
    TestAddresses(HashMap<NodeId, String>),
}

/// Safe closed reason why a selected peer cannot be routed from one snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PeerTopologyMismatch {
    /// The selected Oracle identity is absent from the snapshot.
    Missing,
    /// The selected role is present but has not advertised readiness.
    Unready,
    /// The selected role heartbeat is outside the live cutoff.
    Expired,
    /// The selected role has been replaced by a newer fence.
    Fence,
    /// Production trust forbids the advertised plaintext address.
    PlaintextAddress,
}

/// Maps one peer-address resolution mismatch onto its dispatch disposition.
///
/// Address resolution runs before any RPC is opened, so no frames can have been
/// delivered and the classes are separated by cause rather than sharing one
/// terminal outcome:
///
/// - A plaintext address is a trust-policy refusal, never an availability
///   problem, so it stays terminal and is never downgraded.
/// - A newer fence means the pinned participant cut moved under the query. That
///   is precisely the stale-cut condition the single pre-output replan exists to
///   serve, so it keeps the stale-object class.
/// - Absence, unreadiness, and an expired heartbeat are ordinary pre-`do_get`
///   transport failures. They degrade only their own partition and leave the
///   rest of the cut free to answer. Classifying them stale instead let one
///   participant's heartbeat gap fail an entire query.
const fn dispatch_error_for_topology_mismatch(mismatch: PeerTopologyMismatch) -> DispatchError {
    match mismatch {
        PeerTopologyMismatch::PlaintextAddress => DispatchError::Terminal,
        PeerTopologyMismatch::Fence => DispatchError::StaleObject,
        PeerTopologyMismatch::Missing
        | PeerTopologyMismatch::Unready
        | PeerTopologyMismatch::Expired => DispatchError::Unavailable,
    }
}

/// Resolves one exact candidate from a single immutable membership cut.
///
/// # Errors
/// Returns a scrub-safe topology class for absence, readiness, expiry, fence,
/// or HTTPS policy mismatch.
///
/// # Panics
/// Panics only if the fixed repository-owned liveness cutoff cannot fit a
/// `chrono::Duration`, which is an invariant of its seconds-scale value.
fn resolve_snapshot_candidate<'a>(
    snapshot: &'a ClusterSnapshot,
    candidate: &DispatchCandidate,
    tls_required: bool,
    now: DateTime<Utc>,
) -> Result<&'a str, PeerTopologyMismatch> {
    let lease = match candidate.role {
        wyrd_spec::vala::api::ClusterRole::Oracle => snapshot.live_oracle(candidate.node_id),
        wyrd_spec::vala::api::ClusterRole::Scribe => snapshot
            .live_scribes()
            .into_iter()
            .find(|lease| lease.key.node_id == candidate.node_id),
    }
    .ok_or(PeerTopologyMismatch::Missing)?;
    if !lease.ready {
        return Err(PeerTopologyMismatch::Unready);
    }
    let cutoff = ChronoDuration::from_std(ROLE_LIVENESS_CUTOFF)
        .expect("invariant: role liveness cutoff fits chrono duration");
    if lease.heartbeat_at < now - cutoff {
        return Err(PeerTopologyMismatch::Expired);
    }
    if lease.fencing_token != candidate.worker_fence {
        return Err(PeerTopologyMismatch::Fence);
    }
    if tls_required && !lease.address.starts_with("https://") {
        return Err(PeerTopologyMismatch::PlaintextAddress);
    }
    Ok(lease.address.as_str())
}

/// Immutable role-neutral trust and client identity for the Bifrost peer plane.
#[derive(Clone)]
pub struct BifrostPeerTls {
    /// PEM-encoded dedicated Bifrost peer CA accepted for peer servers.
    ca_certificate_pem: Vec<u8>,
    /// DNS name required on the authenticated peer certificate.
    server_name: String,
    /// PEM leaf chain this process presents as its client identity.
    client_certificate_chain_pem: Vec<u8>,
    /// PEM private key paired with `client_certificate_chain_pem`.
    client_private_key_pem: secrecy::SecretString,
}

impl std::fmt::Debug for BifrostPeerTls {
    /// Formats only non-secret trust identity; key bytes never reach logs.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BifrostPeerTls")
            .field("server_name", &self.server_name)
            .field("ca_certificate_bytes", &self.ca_certificate_pem.len())
            .field(
                "client_certificate_bytes",
                &self.client_certificate_chain_pem.len(),
            )
            .field("client_private_key", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

impl BifrostPeerTls {
    /// Creates one immutable role-neutral peer identity from boot-loaded PEM bytes.
    ///
    /// The same material serves both directions of the peer plane: the private
    /// listener presents it to accept inbound connections, and every outbound
    /// dial presents it as the client identity. There is no role-specific
    /// variant, so a Scribe-only peer cannot skip a check an Oracle applies.
    #[must_use]
    pub fn new(
        ca_certificate_pem: Vec<u8>,
        server_name: String,
        client_certificate_chain_pem: Vec<u8>,
        client_private_key_pem: secrecy::SecretString,
    ) -> Self {
        Self {
            ca_certificate_pem,
            server_name,
            client_certificate_chain_pem,
            client_private_key_pem,
        }
    }

    /// Builds a peer identity that carries no usable certificate material.
    ///
    /// Only for in-crate unit fixtures that construct an owner requiring a peer
    /// identity but never dial through it. Any endpoint built from it fails to
    /// connect, which is the correct outcome for a fixture that must not reach
    /// the network.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn unreachable_for_test() -> Self {
        Self {
            ca_certificate_pem: Vec::new(),
            server_name: "unreachable.invalid".to_owned(),
            client_certificate_chain_pem: Vec::new(),
            client_private_key_pem: secrecy::SecretString::from(String::new()),
        }
    }

    /// Returns the immutable CA trust bytes for sibling private services.
    #[must_use]
    pub fn ca_certificate_pem(&self) -> &[u8] {
        &self.ca_certificate_pem
    }

    /// Returns the certificate DNS identity shared by sibling private services.
    #[must_use]
    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    /// Returns the leaf chain this process presents on the peer plane.
    #[must_use]
    pub fn client_certificate_chain_pem(&self) -> &[u8] {
        &self.client_certificate_chain_pem
    }

    /// Builds the one mutually authenticated peer endpoint for `address`.
    ///
    /// Every private caller — Oracle dispatch, query forwarding, Scribe tail
    /// discovery, and lifecycle control — resolves its channel here, so no peer
    /// role can reach a weaker or plaintext transport.
    ///
    /// # Errors
    ///
    /// Returns [`wyrd_tonic::transport::EndpointBuildError`] when the process
    /// Rustls provider conflicts or the address, trust root, or client identity
    /// is rejected by tonic.
    pub fn endpoint(
        &self,
        address: String,
    ) -> Result<wyrd_tonic::tonic::transport::Endpoint, wyrd_tonic::transport::EndpointBuildError>
    {
        use secrecy::ExposeSecret as _;
        wyrd_tonic::transport::mutually_authenticated_tls_endpoint(
            address,
            &self.ca_certificate_pem,
            self.server_name.clone(),
            &self.client_certificate_chain_pem,
            self.client_private_key_pem.expose_secret().as_bytes(),
        )
    }
}

impl TonicOraclePeerTransport {
    /// Creates a production transport over the live cluster registry and the
    /// mTLS peer identity.
    #[must_use]
    pub fn with_tls(registry: Arc<ClusterRegistry>, tls: BifrostPeerTls) -> Self {
        Self {
            topology: OraclePeerTopology::Registry(registry),
            tls,
            channels: Mutex::default(),
        }
    }

    /// Creates an mTLS transport over an immutable endpoint fixture.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn with_test_tls(addresses: HashMap<NodeId, String>, tls: BifrostPeerTls) -> Self {
        Self {
            topology: OraclePeerTopology::TestAddresses(addresses),
            tls,
            channels: Mutex::default(),
        }
    }

    /// Resolves an exact candidate from one current immutable membership cut.
    ///
    /// # Errors
    /// Returns stale-object when the node is absent or its current role fence
    /// differs, and terminal when a TLS transport is configured with plaintext.
    fn resolve_candidate(&self, candidate: &DispatchCandidate) -> Result<String, DispatchError> {
        // Prefer the endpoint the immutable cut already authenticated. Trust
        // policy is still enforced here because it is a property of the address
        // itself and needs no membership lookup. A participant that has since
        // stopped serving surfaces as an ordinary connect failure, which is the
        // pre-`do_get` transport-failure path, not a membership verdict.
        if let Some(endpoint) = &candidate.endpoint {
            if !endpoint.starts_with("https://") {
                return Err(DispatchError::Terminal);
            }
            return Ok(endpoint.clone());
        }
        #[cfg(feature = "test-support")]
        if let OraclePeerTopology::TestAddresses(addresses) = &self.topology {
            let address = addresses
                .get(&candidate.node_id)
                .ok_or(DispatchError::StaleObject)?;
            if !address.starts_with("https://") {
                return Err(DispatchError::Terminal);
            }
            return Ok(address.clone());
        }
        let snapshot = self.snapshot();
        match resolve_snapshot_candidate(&snapshot, candidate, true, Utc::now()) {
            Ok(address) => Ok(address.to_owned()),
            Err(mismatch) => {
                tracing::warn!(
                    worker = %candidate.node_id.as_uuid(),
                    expected_fence = candidate.worker_fence,
                    mismatch = ?mismatch,
                    "Oracle peer topology target is stale"
                );
                Err(dispatch_error_for_topology_mismatch(mismatch))
            }
        }
    }

    /// Resolves the currently live fence for public trait compatibility.
    ///
    /// Production directory dispatch does not use this projection because it
    /// must preserve the planning candidate's exact fence.
    ///
    /// # Errors
    /// Returns stale-object when the worker has no current ready/live lease.
    fn current_candidate(&self, worker: NodeId) -> Result<DispatchCandidate, DispatchError> {
        #[cfg(feature = "test-support")]
        if let OraclePeerTopology::TestAddresses(addresses) = &self.topology {
            return addresses
                .contains_key(&worker)
                .then_some(DispatchCandidate {
                    node_id: worker,
                    role: wyrd_spec::vala::api::ClusterRole::Oracle,
                    worker_fence: 0,
                    endpoint: None,
                })
                .ok_or(DispatchError::StaleObject);
        }
        let snapshot = self.snapshot();
        let lease = snapshot
            .live_oracle(worker)
            .ok_or(DispatchError::StaleObject)?;
        Ok(DispatchCandidate {
            node_id: worker,
            role: wyrd_spec::vala::api::ClusterRole::Oracle,
            worker_fence: lease.fencing_token,
            endpoint: None,
        })
    }

    /// Loads exactly one immutable topology cut for one outbound resolution.
    fn snapshot(&self) -> Arc<crate::cluster::ClusterSnapshot> {
        match &self.topology {
            OraclePeerTopology::Registry(registry) => registry.snapshot(),
            #[cfg(feature = "test-support")]
            OraclePeerTopology::TestAddresses(_) => {
                unreachable!("test address fixtures resolve before registry snapshot access")
            }
        }
    }

    /// Returns a client over the exact selected worker's authenticated channel.
    ///
    /// The first call for a peer incarnation connects eagerly, so an
    /// unreachable peer still fails here as a pre-`do_get` transport failure,
    /// and caches the channel under that peer's fence and endpoint. Later
    /// calls for the same fence and endpoint multiplex over it; a different
    /// fence or endpoint replaces it. Tonic re-establishes a dropped
    /// connection on the cached channel with the same TLS identity checks.
    ///
    /// The client lifts tonic's 4 MiB default decode cap: one worker frame is
    /// one batch the authenticated worker already materialized under its own
    /// grants, and a batch of accepted rows can exceed 4 MiB. A codec cap here
    /// would refuse acknowledged data after ACK instead of bounding memory.
    ///
    /// # Errors
    /// Returns retryable failure for absent, invalid, or unreachable endpoints
    /// and terminal failure when the channel map lock is poisoned.
    async fn client(
        &self,
        candidate: &DispatchCandidate,
    ) -> Result<OraclePeerServiceClient<Channel>, DispatchError> {
        let address = self.resolve_candidate(candidate)?;
        let cached = self
            .channels
            .lock()
            .map_err(|_| DispatchError::Terminal)?
            .get(&candidate.node_id)
            .filter(|cached| cached.fence == candidate.worker_fence && cached.address == address)
            .map(|cached| cached.channel.clone());
        let channel = if let Some(channel) = cached {
            channel
        } else {
            let started = std::time::Instant::now();
            let channel = self
                .tls
                .endpoint(address.clone())
                .map_err(|_| DispatchError::Unavailable)?
                .connect()
                .await
                .map_err(|_| DispatchError::Unavailable)?;
            super::QueryPhase::PeerConnect.record(started);
            self.channels
                .lock()
                .map_err(|_| DispatchError::Terminal)?
                .insert(
                    candidate.node_id,
                    PeerChannel {
                        fence: candidate.worker_fence,
                        address,
                        channel: channel.clone(),
                    },
                );
            channel
        };
        Ok(OraclePeerServiceClient::new(channel).max_decoding_message_size(usize::MAX))
    }

    /// Builds the typed reservation context over an already context-free
    /// request encoding.
    ///
    /// The body digest is taken over the encoding with the context field
    /// cleared, which is exactly what the receiver recomputes, so the context
    /// covers every routed identity in the request.
    ///
    /// # Errors
    ///
    /// Returns [`DispatchError::Terminal`] when the body exceeds its bound or
    /// the claims cannot be encoded; a reservation is never sent without its
    /// context.
    fn reservation_context<T: wyrd_tonic::prost::Message>(
        context_free: &T,
        binding: &ReservationBinding,
    ) -> Result<wyrd_tonic::wyrd::v1::PeerContext, DispatchError> {
        let body_digest = reservation_body_digest(&context_free.encode_to_vec())
            .map_err(|_| DispatchError::Terminal)?;
        let expires_at_ms = (Utc::now() + ChronoDuration::seconds(RESERVATION_CONTEXT_TTL_SECONDS))
            .timestamp_millis();
        ReservationTicketClaims::for_binding(binding, body_digest, expires_at_ms)
            .to_context(binding.operation)
            .map(Into::into)
            .map_err(|_| DispatchError::Terminal)
    }

    /// Opens one held grant on one exact planned node/fence target.
    ///
    /// The grant is the stream: its first message admits or refuses, and an
    /// admitted grant stays held on the follower until the returned handle,
    /// which owns the stream, is dropped.
    ///
    /// # Errors
    /// Returns stale-object for a changed lease, retryable for transport
    /// failure, or terminal for invalid transport and response contracts,
    /// including a stream that ends before its first message.
    async fn reserve_candidate(
        &self,
        candidate: &DispatchCandidate,
        request: ReserveNodeSlotsRequest,
    ) -> Result<Result<ParticipantGrant, ReservationRejected>, DispatchError> {
        let leader_node_id = request.leader_node_id;
        let leader_fence = request.leader_fencing_token;
        let query_id = request.query_id.as_uuid();
        let mut wire: wyrd_tonic::wyrd::v1::ReserveNodeSlotsRequest = request.into();
        let binding = ReservationBinding {
            operation: ReservationOperationV1::ReserveSlots,
            source_node_id: leader_node_id,
            source_fence: leader_fence,
            destination_node_id: candidate.node_id,
            destination_fence: candidate.worker_fence,
            query_id,
        };
        wire.context = None;
        wire.context = Some(Self::reservation_context(&wire, &binding)?);
        let mut client = self.client(candidate).await?;
        let mut stream = client
            .reserve_slots(wire)
            .await
            .map_err(|status| status_error(&status))?
            .into_inner();
        let first = stream
            .message()
            .await
            .map_err(|status| status_error(&status))?
            .ok_or(DispatchError::Terminal)?;
        let outcome: ReserveNodeSlotsResponse = first.try_into().map_err(|error| {
            tracing::warn!(?error, "Oracle leader could not decode a grant response");
            DispatchError::Terminal
        })?;
        Ok(match outcome {
            ReserveNodeSlotsResponse::Pending(pending) => Ok(ParticipantGrant::new(
                pending.reservation_id,
                Box::new(stream),
            )),
            ReserveNodeSlotsResponse::Rejected(rejected) => Err(rejected),
        })
    }

    /// Executes a fragment for one exact planned node/fence target.
    ///
    /// # Errors
    /// Returns stale-object for a changed lease, retryable for transport
    /// failure, or terminal for invalid transport and stream contracts.
    async fn execute_candidate(
        &self,
        candidate: &DispatchCandidate,
        request: ExecuteFragmentRequest,
    ) -> Result<WorkerAttemptStream, DispatchError> {
        let mut client = self.client(candidate).await?;
        let wire: wyrd_tonic::wyrd::v1::ExecuteFragmentRequest = request.into();
        let opened = std::time::Instant::now();
        let response = client
            .execute_fragment(wire)
            .await
            .inspect(|_| super::QueryPhase::PeerOpen.record(opened))
            .map_err(|status| {
                tracing::warn!(code = ?status.code(), "Oracle peer execute rejected");
                if candidate.role == wyrd_spec::vala::api::ClusterRole::Scribe {
                    live_execution_status_error(&status)
                } else {
                    execution_status_error(&status)
                }
            })?;
        let mut stream = response.into_inner();
        let output = async_stream::stream! {
            let streaming = std::time::Instant::now();
            let mut first = true;
            while let Some(frame) = stream.next().await {
                if std::mem::take(&mut first) {
                    super::QueryPhase::PeerFirstFrame.record(streaming);
                }
                yield frame.map_err(|status| {
                    tracing::warn!(code = ?status.code(), message = status.message(), "Oracle peer worker stream failed");
                    stream_status_error(&status)
                }).and_then(|frame| frame.try_into().map(LiveFrame::Wire).map_err(|error| {
                    tracing::warn!(?error, "Oracle leader could not decode a worker frame");
                    DispatchError::Terminal
                }));
            }
            super::QueryPhase::PeerTerminal.record(streaming);
        };
        Ok(Box::pin(output))
    }
}

#[async_trait]
impl OraclePeerTransport for TonicOraclePeerTransport {
    /// Opens one held grant through the generated tonic client.
    ///
    /// # Errors
    /// Returns retryable transport or terminal conversion failure.
    async fn reserve(
        &self,
        worker: NodeId,
        request: ReserveNodeSlotsRequest,
    ) -> Result<Result<ParticipantGrant, ReservationRejected>, DispatchError> {
        let candidate = self.current_candidate(worker)?;
        self.reserve_candidate(&candidate, request).await
    }

    /// Adapts one footer-terminated tonic stream without collecting frames.
    ///
    /// # Errors
    /// Returns retryable transport or terminal frame-conversion failure.
    async fn execute(
        &self,
        worker: NodeId,
        request: ExecuteFragmentRequest,
    ) -> Result<WorkerAttemptStream, DispatchError> {
        let candidate = self.current_candidate(worker)?;
        self.execute_candidate(&candidate, request).await
    }
}

/// Routes peer reservation and live-fragment traffic from this node's leader.
///
/// Remote peers are reached through tonic. This node's own Oracle is never a
/// peer: a leader reserves only remote graph participants and opens fragments
/// only on Scribes, so a local Oracle candidate is a contract failure.
pub struct OraclePeerTransportDirectory {
    /// Physical node identity this directory's leader runs on.
    local_node_id: NodeId,
    /// Closed remote route separating live production resolution from injection.
    remote: RemoteOraclePeerTransport,
    /// In-process executor for this node's own Scribe; its fragments never
    /// cross a peer channel, so their batches stay in memory.
    local_scribe: Option<Arc<dyn OraclePeerTransport>>,
}

/// Private remote dispatch variants preserving the public transport contract.
enum RemoteOraclePeerTransport {
    /// Production tonic owner that resolves the exact planned candidate.
    Production(Arc<TonicOraclePeerTransport>),
    /// Process-local mode: no peer plane exists, so no remote candidate is reachable.
    Unavailable,
    /// Test-only adapter retaining isolated transport injection.
    #[cfg(test)]
    Injected(Arc<dyn OraclePeerTransport>),
}

impl OraclePeerTransportDirectory {
    /// Creates the production directory over the tonic transport.
    ///
    /// This node's own Scribe fragments always run through `local_scribe`.
    /// `remote` is absent for a process-local node, which serves no peer
    /// plane; any non-local candidate then fails terminally rather than being
    /// dialed.
    #[must_use]
    pub fn new(
        local_node_id: NodeId,
        remote: Option<Arc<TonicOraclePeerTransport>>,
        local_scribe: Option<Arc<dyn OraclePeerTransport>>,
    ) -> Self {
        Self {
            local_node_id,
            remote: remote.map_or(
                RemoteOraclePeerTransport::Unavailable,
                RemoteOraclePeerTransport::Production,
            ),
            local_scribe,
        }
    }

    /// Creates a directory from injectable transports for isolated owner tests.
    #[cfg(test)]
    #[must_use]
    pub(super) fn new_for_test(
        local_node_id: NodeId,
        remote: Arc<dyn OraclePeerTransport>,
    ) -> Self {
        Self {
            local_node_id,
            remote: RemoteOraclePeerTransport::Injected(remote),
            local_scribe: None,
        }
    }

    /// Returns whether `node_id` is the exact in-process Oracle identity.
    #[must_use]
    pub fn is_local(&self, node_id: NodeId) -> bool {
        node_id == self.local_node_id
    }

    /// Opens one participant's held grant for a distributed graph.
    ///
    /// This is the Analytical leader's only admission entry point. The grant
    /// stays held on the participant until the returned handle is dropped,
    /// which is how the leader releases it.
    ///
    /// # Errors
    ///
    /// The outer result carries transport or contract failure, including
    /// [`DispatchError::Terminal`] for this node's own identity or when no peer
    /// plane exists. The inner one separates acceptance from an explicit
    /// stream-accept refusal, which keeps the participant's `retry_after_ms`
    /// so the leader can own a bounded retry. Only that inner refusal is proof
    /// the participant accepted no work.
    pub async fn reserve_graph(
        &self,
        candidate: &DispatchCandidate,
        request: ReserveNodeSlotsRequest,
    ) -> Result<Result<ParticipantGrant, ReservationRejected>, DispatchError> {
        if self.is_local(candidate.node_id) {
            return Err(DispatchError::Terminal);
        }
        match &self.remote {
            RemoteOraclePeerTransport::Production(remote) => {
                remote.reserve_candidate(candidate, request).await
            }
            RemoteOraclePeerTransport::Unavailable => Err(DispatchError::Terminal),
            #[cfg(test)]
            RemoteOraclePeerTransport::Injected(remote) => {
                remote.reserve(candidate.node_id, request).await
            }
        }
    }

    /// Opens one fragment on a Scribe, in this process when the candidate is
    /// this node's own Scribe and remotely otherwise.
    ///
    /// # Errors
    ///
    /// Returns [`DispatchError::Terminal`] when no route reaches the
    /// candidate, and otherwise the selected transport's failure.
    async fn execute(
        &self,
        candidate: &DispatchCandidate,
        request: ExecuteFragmentRequest,
    ) -> Result<WorkerAttemptStream, DispatchError> {
        if let Some(scribe) = &self.local_scribe
            && self.is_local(candidate.node_id)
            && candidate.role == wyrd_spec::vala::api::ClusterRole::Scribe
        {
            return scribe.execute(candidate.node_id, request).await;
        }
        match &self.remote {
            RemoteOraclePeerTransport::Production(remote) => {
                remote.execute_candidate(candidate, request).await
            }
            RemoteOraclePeerTransport::Unavailable => Err(DispatchError::Terminal),
            #[cfg(test)]
            RemoteOraclePeerTransport::Injected(remote) => {
                remote.execute(candidate.node_id, request).await
            }
        }
    }
}

/// Classifies tonic status without retrying security or malformed-contract failures.
///
/// `ResourceExhausted` is the code a healthy peer returns when it is momentarily
/// out of admission capacity. It is transient backpressure and classifies as
/// [`DispatchError::Unavailable`] so the leader can place the fragment elsewhere
/// rather than failing the query.
///
/// `Unavailable` deliberately stays terminal. A peer uses it for a genuine
/// storage or role outage, where silently continuing would return an incomplete
/// answer; Bifrost fails closed on completeness instead. That is why capacity
/// refusal must not reuse `Unavailable` — see the peer reservation path.
fn status_error(status: &Status) -> DispatchError {
    match status.code() {
        wyrd_tonic::tonic::Code::DeadlineExceeded
        | wyrd_tonic::tonic::Code::Cancelled
        | wyrd_tonic::tonic::Code::ResourceExhausted => DispatchError::Unavailable,
        _ => DispatchError::Terminal,
    }
}

/// Classifies the authenticated execute stream's typed stale-object wire signal.
fn execution_status_error(status: &Status) -> DispatchError {
    match status.code() {
        wyrd_tonic::tonic::Code::NotFound => DispatchError::FileNotFound,
        // The private peer protocol reserves `Aborted` for the footer tenant
        // proof so a foreign-tenant refusal on a remote worker reaches the
        // leader as a tenant-isolation outcome instead of a generic
        // security or retryable failure.
        wyrd_tonic::tonic::Code::Aborted => DispatchError::TenantInvariant,
        wyrd_tonic::tonic::Code::FailedPrecondition => DispatchError::EligibleSourceLoss {
            cause: EligibleSourceLossCause::ProviderResolution,
        },
        _ => status_error(status),
    }
}

/// Classifies a live Scribe fragment's open refusal.
///
/// Live coverage is best effort, so a Scribe that answers `Unavailable` is an
/// availability loss the caller may degrade before any row — unlike a
/// published worker, which fails closed. A capacity refusal is a resource
/// fault and never becomes a live omission. Every other code keeps the
/// published classification.
fn live_execution_status_error(status: &Status) -> DispatchError {
    match status.code() {
        wyrd_tonic::tonic::Code::Unavailable => DispatchError::Unavailable,
        wyrd_tonic::tonic::Code::ResourceExhausted => DispatchError::Capacity,
        _ => execution_status_error(status),
    }
}

/// Classifies errors after a stream was delivered as partition-local decoder partials.
///
/// `Aborted` stays the footer tenant refusal here too: the proof runs when the
/// stream is first polled, after the schema frame, so a refusal can end an
/// already-open stream and must not degrade into an availability loss.
fn stream_status_error(status: &Status) -> DispatchError {
    match status.code() {
        wyrd_tonic::tonic::Code::Aborted => DispatchError::TenantInvariant,
        wyrd_tonic::tonic::Code::Unauthenticated
        | wyrd_tonic::tonic::Code::PermissionDenied
        | wyrd_tonic::tonic::Code::InvalidArgument => DispatchError::Terminal,
        _ => DispatchError::Unavailable,
    }
}

/// Immutable worker candidate with its role fence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchCandidate {
    /// Worker node identity.
    pub node_id: NodeId,
    /// Exact role served by this candidate.
    pub role: wyrd_spec::vala::api::ClusterRole,
    /// Current role fence.
    pub worker_fence: FencingToken,
    /// Private endpoint carried from the authenticated participant cut.
    ///
    /// The cut already selected and authenticated this endpoint when the
    /// participant was chosen, and the cut is immutable for the whole attempt.
    /// Re-deriving the address from live membership at dispatch time would
    /// reintroduce exactly the membership refresh the cut exists to prevent: a
    /// participant admitted by the cut can disappear from a later, independently
    /// refreshed snapshot and be reported absent even though it is serving.
    ///
    /// `None` falls back to snapshot resolution for callers that have no cut.
    pub endpoint: Option<String>,
}

/// Immutable query and authorization bindings every live fragment of one query is opened under.
#[derive(Debug, Clone)]
pub struct DispatchContext {
    /// Query identity.
    pub query_id: QueryId,
    /// Leader node identity.
    pub leader_node_id: NodeId,
    /// Leader role fence.
    pub leader_fence: FencingToken,
    /// Authenticated data-tenant UUID bytes.
    pub tenant_id: uuid::Uuid,
    /// Admission class, carried into open-failure diagnostics.
    pub query_class: QueryClass,
    /// Server-derived permission digest.
    pub permission_digest: String,
    /// Admission-owned cancellation propagated to every stream open.
    pub cancellation: CancellationToken,
    /// Absolute deadline bounding every stream open and read.
    pub deadline: Instant,
}

/// One immutable native physical subtree and its role-local scan assignments.
#[derive(Debug, Clone)]
pub struct PhysicalDispatchFragment {
    /// Accepted codec bytes for the follower subtree.
    pub physical_plan_bytes: Vec<u8>,
    /// Complete role-local provider assignments referenced by the subtree.
    pub assignments: Vec<wyrd_spec::vala::api::FollowerScanAssignment>,
    /// Authenticated table binding shared by every assignment.
    pub binding: wyrd_spec::vala::api::TenantTableBinding,
    /// Closed target role selected for this subtree.
    pub target_role: wyrd_spec::vala::api::ClusterRole,
    /// Immutable codec fingerprint bound into ticket and footer.
    pub plan_fingerprint: String,
    /// Absolute request deadline in Unix milliseconds.
    pub deadline_unix_ms: i64,
}

/// Builds the execute request one minted ticket authorizes for one candidate.
///
/// The leader and target fences, assignments, plan bytes, and fingerprint are
/// the exact values the ticket claims were minted over, so the worker's
/// recomputation of the claim binding matches.
fn fragment_request(
    peer_context: PeerContext,
    candidate: &DispatchCandidate,
    context: &DispatchContext,
    fragment: &PhysicalDispatchFragment,
) -> ExecuteFragmentRequest {
    ExecuteFragmentRequest {
        context: peer_context,
        physical_plan_bytes: fragment.physical_plan_bytes.clone(),
        // Scribe fragments are never granted: the receiver requires nil.
        reservation_id: ReservationId::new(uuid::Uuid::nil()),
        leader_fence: OracleRoleFence {
            node_id: context.leader_node_id,
            role: wyrd_spec::vala::api::ClusterRole::Oracle,
            fencing_token: context.leader_fence,
        },
        target_fence: OracleRoleFence {
            node_id: candidate.node_id,
            role: fragment.target_role,
            fencing_token: candidate.worker_fence,
        },
        assignments: fragment.assignments.clone(),
        plan_fingerprint: fragment.plan_fingerprint.clone(),
    }
}

/// Opens live Scribe fragments for one leader under signed, fenced peer contexts.
pub struct FragmentDispatcher {
    /// Node-aware directory enforcing in-process leader and tonic remote routing.
    ///
    /// Shared rather than owned: the Analytical leader reserves its graph
    /// participants through the same directory, so both paths route through one
    /// identity-selected set of adapters.
    transports: Arc<OraclePeerTransportDirectory>,
}

impl FragmentDispatcher {
    /// Creates a dispatcher over the shared node-aware transport directory.
    #[must_use]
    pub fn new(transports: Arc<OraclePeerTransportDirectory>) -> Self {
        Self { transports }
    }

    /// Opens one Scribe live fragment and hands back its unbuffered frames.
    ///
    /// Nothing is buffered or retried: the caller validates frames as they
    /// arrive and owns the stream's lifetime, so dropping it cancels the
    /// fragment on the Scribe. The peer context names exactly this candidate's
    /// node and writer epoch; a Scribe that restarted or advanced its epoch
    /// refuses it. The open is bounded by both the query's remaining deadline
    /// and its cancellation token.
    ///
    /// # Errors
    ///
    /// Returns [`DispatchError::Terminal`] when this node is not the local
    /// leader, the candidate is not the fragment's Scribe target, or the peer
    /// context cannot be encoded; [`DispatchError::Unavailable`] when the
    /// deadline has passed, the query was cancelled, or the open timed out;
    /// and otherwise the transport's own open failure unchanged.
    pub async fn open_stream(
        &self,
        context: &DispatchContext,
        fragment: &PhysicalDispatchFragment,
        candidate: &DispatchCandidate,
    ) -> Result<WorkerAttemptStream, DispatchError> {
        if !self.transports.is_local(context.leader_node_id)
            || candidate.role != wyrd_spec::vala::api::ClusterRole::Scribe
            || fragment.target_role != candidate.role
        {
            return Err(DispatchError::Terminal);
        }
        let claims = peer_ticket_claims(
            candidate,
            context,
            fragment,
            Utc::now() + FRAGMENT_CONTEXT_ACCEPTANCE,
        )?;
        let peer_context = claims.to_context().map_err(|_| {
            tracing::error!("Oracle live Scribe peer context encoding failed");
            DispatchError::Terminal
        })?;
        let request = fragment_request(peer_context, candidate, context, fragment);
        let remaining = context
            .deadline
            .checked_duration_since(Instant::now())
            .ok_or(DispatchError::Unavailable)?;
        match tokio::select! {
            () = context.cancellation.cancelled() => Err(DispatchError::Unavailable),
            result = tokio::time::timeout(remaining, self.transports.execute(candidate, request)) =>
                result.map_err(|_| DispatchError::Unavailable).and_then(std::convert::identity),
        } {
            Ok(frames) => Ok(frames),
            Err(error) => {
                tracing::warn!(
                    stage = "remote_execute_open",
                    query_class = ?context.query_class,
                    candidate_node = %candidate.node_id.as_uuid(),
                    ?error,
                    "oracle peer remote execute open failed"
                );
                record_peer_attempt(FragmentOutcome::Failed, dispatch_error_label(&error));
                Err(error)
            }
        }
    }
}

/// Maps internal retry classes to closed metric labels.
fn dispatch_error_label(error: &DispatchError) -> PeerErrorClass {
    match error {
        DispatchError::Unavailable
        | DispatchError::EligibleSourceLoss { .. }
        | DispatchError::StaleObject
        | DispatchError::FileNotFound
        | DispatchError::Capacity => PeerErrorClass::Availability,
        DispatchError::Terminal | DispatchError::TenantInvariant => PeerErrorClass::Security,
    }
}

impl From<PeerSecurityError> for DispatchError {
    fn from(_: PeerSecurityError) -> Self {
        Self::Terminal
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::Int64Array;
    use arrow::datatypes::{DataType, Field, Schema};
    use std::sync::atomic::Ordering;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::vala::api::{
        AnalyticalGraphRef, ClusterCapabilities, ClusterNodeKey, ClusterRole, ClusterRoleLease,
        FollowerScanAssignment, OracleCapabilitiesV1, PersistedFileAssignment, TenantTableBinding,
    };
    use wyrd_tonic::tonic::Request;

    /// Builds one Oracle lease for deterministic topology resolution cases.
    fn topology_lease(
        node_id: NodeId,
        address: &str,
        fence: u64,
        ready: bool,
        heartbeat_at: DateTime<Utc>,
    ) -> ClusterRoleLease {
        ClusterRoleLease {
            key: ClusterNodeKey {
                node_id,
                role: ClusterRole::Oracle,
            },
            address: address.to_owned(),
            fencing_token: fence,
            capability_version: 1,
            capabilities: ClusterCapabilities::OracleV1(OracleCapabilitiesV1 {
                storage_protocol_version: 1,
                cpu_cores: 1.0,
                memory_budget_bytes: 1,
                cpu_cores_per_slot: 1.0,
                memory_bytes_per_slot: 1,
                raw_slots: 1,
                usable_slots: 1,
                supported_classes: vec![QueryClass::Interactive],
                max_workers_per_query: 1,
            }),
            ready,
            started_at: heartbeat_at,
            heartbeat_at,
        }
    }

    /// Builds one native physical dispatch fixture for transport-state tests.
    fn physical_dispatch_fragment(id: &str) -> PhysicalDispatchFragment {
        let binding = TenantTableBinding {
            tenant_id: DataTenantId::SYSTEM_OWNER,
            namespace: "vala.bifrost".to_owned(),
            table: "events".to_owned(),
        };
        PhysicalDispatchFragment {
            physical_plan_bytes: id.as_bytes().to_vec(),
            assignments: vec![FollowerScanAssignment {
                scan_id: format!("{id}-scan"),
                binding: binding.clone(),
                persisted: PersistedFileAssignment { files: Vec::new() },
                scribe_provider_cut: None,
                // Canonical 64-lowercase-hex placeholder: the assignment-
                // authority digest requires this shape even in fixtures that
                // never exercise object storage.
                schema_fingerprint: "0".repeat(64),
                required_columns: vec!["value".to_owned()],
                predicates: Vec::new(),
                reader_cut: wyrd_spec::vala::api::FollowerReaderCut::no_snapshot(
                    uuid::Uuid::nil(),
                    1,
                ),
            }],
            binding,
            target_role: ClusterRole::Oracle,
            plan_fingerprint: id.to_owned(),
            deadline_unix_ms: i64::MAX,
        }
    }

    /// Classifies every live-routing outcome without IO, sleeps, or secret detail.
    #[test]
    fn oracle_peer_snapshot_resolver_matrix_is_exact_and_safe() {
        let now = Utc::now();
        let node = NodeId::new(uuid::Uuid::now_v7());
        let candidate = DispatchCandidate {
            node_id: node,
            role: ClusterRole::Oracle,
            worker_fence: 7,
            endpoint: None,
        };
        let matching = ClusterSnapshot::new(vec![topology_lease(
            node,
            "https://worker-a.example:50052",
            7,
            true,
            now,
        )]);
        assert_eq!(
            resolve_snapshot_candidate(&matching, &candidate, true, now),
            Ok("https://worker-a.example:50052")
        );
        assert_eq!(
            resolve_snapshot_candidate(&ClusterSnapshot::default(), &candidate, true, now),
            Err(PeerTopologyMismatch::Missing)
        );
        let unready = ClusterSnapshot::new(vec![topology_lease(
            node,
            "https://worker-a.example:50052",
            7,
            false,
            now,
        )]);
        assert_eq!(
            resolve_snapshot_candidate(&unready, &candidate, true, now),
            Err(PeerTopologyMismatch::Unready)
        );
        let expired = ClusterSnapshot::new(vec![topology_lease(
            node,
            "https://worker-a.example:50052",
            7,
            true,
            now - ChronoDuration::seconds(16),
        )]);
        assert_eq!(
            resolve_snapshot_candidate(&expired, &candidate, true, now),
            Err(PeerTopologyMismatch::Expired)
        );
        let replaced = ClusterSnapshot::new(vec![topology_lease(
            node,
            "https://worker-b.example:50053",
            8,
            true,
            now,
        )]);
        assert_eq!(
            resolve_snapshot_candidate(&replaced, &candidate, true, now),
            Err(PeerTopologyMismatch::Fence)
        );
        assert_eq!(
            resolve_snapshot_candidate(
                &replaced,
                &DispatchCandidate {
                    node_id: node,
                    role: ClusterRole::Oracle,
                    worker_fence: 8,
                    endpoint: None,
                },
                true,
                now,
            ),
            Ok("https://worker-b.example:50053")
        );
        let plaintext = ClusterSnapshot::new(vec![topology_lease(
            node,
            "http://worker.example:50052",
            7,
            true,
            now,
        )]);
        assert_eq!(
            resolve_snapshot_candidate(&plaintext, &candidate, true, now),
            Err(PeerTopologyMismatch::PlaintextAddress)
        );
    }

    /// Every peer-address mismatch maps to the disposition its cause earns.
    ///
    /// This matrix is the pre-RPC half of the partition disposition contract.
    /// It is pinned exhaustively because collapsing any row into a terminal
    /// outcome fails a whole query for a condition that only one participant
    /// experienced, and collapsing the fence row into a degrade would silently
    /// answer from a cut that has already moved.
    #[test]
    fn peer_topology_mismatch_maps_to_its_exact_disposition() {
        for (mismatch, expected) in [
            (PeerTopologyMismatch::PlaintextAddress, "terminal"),
            (PeerTopologyMismatch::Fence, "stale"),
            (PeerTopologyMismatch::Missing, "unavailable"),
            (PeerTopologyMismatch::Unready, "unavailable"),
            (PeerTopologyMismatch::Expired, "unavailable"),
        ] {
            let observed = match dispatch_error_for_topology_mismatch(mismatch) {
                DispatchError::Terminal => "terminal",
                DispatchError::StaleObject => "stale",
                DispatchError::Unavailable => "unavailable",
                DispatchError::FileNotFound => "file_not_found",
                DispatchError::Capacity => "capacity",
                DispatchError::EligibleSourceLoss { .. } => "source_loss",
                DispatchError::TenantInvariant => "tenant_invariant",
            };
            assert_eq!(
                observed, expected,
                "{mismatch:?} must resolve to {expected}"
            );
        }
    }

    /// An unavailable participant degrades only its own partition.
    ///
    /// Proves the end of the chain the mapping above feeds: a peer that is
    /// absent, unready, or past its heartbeat cutoff must leave the remaining
    /// participants free to answer instead of failing the query.
    #[test]
    fn absent_participant_degrades_only_its_partition() {
        for mismatch in [
            PeerTopologyMismatch::Missing,
            PeerTopologyMismatch::Unready,
            PeerTopologyMismatch::Expired,
        ] {
            assert!(
                !matches!(
                    dispatch_error_for_topology_mismatch(mismatch),
                    DispatchError::Terminal | DispatchError::StaleObject
                ),
                "{mismatch:?} must not fail the whole query"
            );
        }
    }

    /// Creates one exact graph reservation request.
    fn reserve_request(
        query_id: QueryId,
        leader: NodeId,
        fence: FencingToken,
        expires_at: DateTime<Utc>,
    ) -> ReserveNodeSlotsRequest {
        ReserveNodeSlotsRequest {
            query_id,
            leader_node_id: leader,
            leader_fencing_token: fence,
            expires_at,
            graph: AnalyticalGraphRef {
                public_query_id: query_id.as_uuid(),
                datafusion_query_id: uuid::Uuid::now_v7(),
            },
        }
    }

    /// Signed execution time remains distinct from the context acceptance window.
    ///
    /// # Panics
    ///
    /// Panics if minting clips the execution budget to the acceptance window or
    /// lets a short query outlive its own deadline.
    #[test]
    fn peer_deadlines_keep_acceptance_and_execution_distinct() {
        let now = Utc::now();
        let node = NodeId::new(uuid::Uuid::now_v7());
        let context = DispatchContext {
            query_id: QueryId::new(uuid::Uuid::now_v7()),
            leader_node_id: node,
            leader_fence: 1,
            tenant_id: uuid::Uuid::now_v7(),
            query_class: QueryClass::Interactive,
            permission_digest: "permission".to_owned(),
            cancellation: CancellationToken::new(),
            deadline: Instant::now() + std::time::Duration::from_secs(30),
        };
        let candidate = DispatchCandidate {
            node_id: node,
            role: ClusterRole::Oracle,
            worker_fence: 1,
            endpoint: None,
        };
        let accept_until = now + FRAGMENT_CONTEXT_ACCEPTANCE;
        let mut fragment = physical_dispatch_fragment("deadline");
        fragment.deadline_unix_ms = (now + ChronoDuration::seconds(30)).timestamp_millis();
        let claims = peer_ticket_claims(&candidate, &context, &fragment, accept_until)
            .expect("signed claims");
        assert_eq!(claims.expires_at_ms, accept_until.timestamp_millis());
        assert_eq!(claims.execution_deadline_unix_ms, fragment.deadline_unix_ms);
        fragment.deadline_unix_ms = (now + ChronoDuration::milliseconds(500)).timestamp_millis();
        let short = peer_ticket_claims(&candidate, &context, &fragment, accept_until)
            .expect("short query claims");
        assert_eq!(short.expires_at_ms, fragment.deadline_unix_ms);
        assert_eq!(short.execution_deadline_unix_ms, fragment.deadline_unix_ms);
    }

    /// Builds an Oracle capability whose shared slot ledger holds `units` units.
    ///
    /// A reservation's capacity is the graph envelope it holds, charged against
    /// the shared governor slot ledger, so a saturation test must saturate that
    /// one authority. An explicit slot limit is what makes the ledger a known
    /// size.
    ///
    /// # Panics
    ///
    /// Panics when the deterministic plan or role composition fails.
    fn slot_limited_oracle(units: usize) -> crate::resources::OracleResources {
        crate::resources::BifrostRuntimeResources::from_snapshot(
            crate::resources::SystemResourceSnapshot {
                memory_limit_bytes: 2 * 1024 * 1024 * 1024,
                effective_cpu: 8,
                scratch_capacity_bytes: 1024 * 1024 * 1024,
                scratch_available_bytes: 1024 * 1024 * 1024,
                memory_source: crate::resources::ResourceSource::Injected,
                cpu_source: crate::resources::ResourceSource::Injected,
            },
            crate::resources::BifrostResourcePolicy {
                roles: std::collections::BTreeSet::from([crate::resources::BifrostRole::Oracle]),
                server_memory_min_bytes: None,
                bifrost_memory_limit_bytes: None,
                scratch_limit_bytes: None,
                effective_cpu: None,
                oracle_query_slot_limit: Some(units),
                scratch_root: None,
                volume_roots: None,
            },
        )
        .expect("slot-limited Oracle plan")
        .compose_roles()
        .expect("slot-limited Oracle composition")
        .oracle()
        .expect("composition must enable the Oracle capability")
    }

    /// Acquires one Interactive graph envelope as a reservation's capacity.
    ///
    /// # Errors
    ///
    /// Returns the shared ledger's refusal when no slot unit is free.
    fn graph_envelope(
        oracle: &crate::resources::OracleResources,
    ) -> Result<Box<crate::resources::OracleQueryResources>, crate::resources::BifrostResourceError>
    {
        oracle
            .try_acquire_query(crate::resources::OracleResourceRequest::for_class(
                QueryClass::Interactive,
                0.0,
            ))
            .map(Box::new)
    }

    /// A held grant returns its envelope the moment its stream's guard drops.
    ///
    /// Nothing expires and nothing waits for a later request: the follower is
    /// back at baseline as soon as the leader's stream ends.
    ///
    /// # Panics
    ///
    /// Panics when the grant is not held while its guard lives or any charge
    /// survives the guard.
    #[test]
    fn held_grant_returns_its_envelope_when_its_stream_ends() {
        let oracle = slot_limited_oracle(2);
        let registry = Arc::new(ReservationRegistry::new(2, 2));
        let now = Utc::now();
        let query = QueryId::new(uuid::Uuid::now_v7());
        let leader = NodeId::new(uuid::Uuid::now_v7());
        let grant = registry
            .hold(
                &reserve_request(query, leader, 9, now + ChronoDuration::seconds(30)),
                now,
                graph_envelope(&oracle).expect("envelope"),
            )
            .expect("held grant");
        assert_eq!(registry.held_grants(), 1);
        assert_eq!(oracle.live_slot_units(), 1);
        drop(grant);
        assert_eq!(
            registry.held_grants(),
            0,
            "the stream's end removes the grant"
        );
        assert_eq!(
            oracle.live_slot_units(),
            0,
            "the stream's end returns the charged units"
        );
    }

    /// A saturated follower refuses at stream accept, before any work.
    ///
    /// Capacity is decided once, when the graph envelope is charged against the
    /// shared governor ledger, and a query whose deadline already passed is
    /// refused rather than held.
    ///
    /// # Panics
    ///
    /// Panics when a second envelope is admitted past saturation, a past
    /// deadline is held, or the stream's end strands the charged unit.
    #[test]
    fn graph_grant_saturation_refuses_up_front() {
        let oracle = slot_limited_oracle(1);
        let registry = Arc::new(ReservationRegistry::new(1, 2));
        let now = Utc::now();
        let query = QueryId::new(uuid::Uuid::now_v7());
        let leader = NodeId::new(uuid::Uuid::now_v7());
        let grant = registry
            .hold(
                &reserve_request(query, leader, 13, now + ChronoDuration::seconds(2)),
                now,
                graph_envelope(&oracle).expect("envelope"),
            )
            .expect("held grant");
        assert!(
            graph_envelope(&oracle).is_err(),
            "the shared ledger refuses a second envelope up front"
        );
        drop(grant);
        assert!(matches!(
            registry.hold(
                &reserve_request(query, leader, 13, now),
                now,
                graph_envelope(&oracle).expect("freed envelope"),
            ),
            Err(DispatchError::Terminal)
        ));
        assert_eq!(oracle.live_slot_units(), 0, "nothing stays charged");
    }

    /// A grant's stream ending signals the graph built on it and spends it.
    ///
    /// Activation moves the envelope to the graph and leaves the grant spent,
    /// so no second graph can activate from it. Ending the stream cancels the
    /// signal the graph keeps, and an activation still in flight when the
    /// stream ends drops its envelope instead of restoring a dead grant.
    ///
    /// # Panics
    ///
    /// Panics when a spent grant activates again, the close signal is not
    /// raised, or an envelope outlives its grant's stream.
    #[test]
    fn grant_close_signals_its_graph_and_never_restores_a_dead_grant() {
        let oracle = slot_limited_oracle(2);
        let registry = Arc::new(ReservationRegistry::new(2, 4));
        let now = Utc::now();
        let query = QueryId::new(uuid::Uuid::now_v7());
        let leader = NodeId::new(uuid::Uuid::now_v7());
        let request = reserve_request(query, leader, 3, now + ChronoDuration::seconds(30));
        let grant = registry
            .hold(&request, now, graph_envelope(&oracle).expect("envelope"))
            .expect("held grant");
        let lease = GraphLeaseRequest {
            reservation_id: grant.reservation().reservation_id,
            graph: request.graph,
            query_id: query,
        };
        let activation = registry
            .begin_graph_activation(&lease)
            .expect("first activation");
        let closed = activation.closed();
        let graph_envelope_owner = activation
            .commit(Ok::<_, (Box<_>, BifrostError)>)
            .expect("commit");
        assert!(matches!(
            registry.begin_graph_activation(&lease),
            Err(DispatchError::Terminal)
        ));
        assert!(!closed.is_cancelled());
        drop(grant);
        assert!(closed.is_cancelled(), "the stream's end signals the graph");
        drop(graph_envelope_owner);
        assert_eq!(oracle.live_slot_units(), 0);

        let second = registry
            .hold(&request, now, graph_envelope(&oracle).expect("envelope"))
            .expect("second grant");
        let in_flight = registry
            .begin_graph_activation(&GraphLeaseRequest {
                reservation_id: second.reservation().reservation_id,
                graph: request.graph,
                query_id: query,
            })
            .expect("in-flight activation");
        drop(second);
        in_flight.rollback();
        assert_eq!(registry.held_grants(), 0);
        assert_eq!(
            oracle.live_slot_units(),
            0,
            "an activation that outlived its stream returns nothing to it"
        );
    }

    /// Only authenticated delivered execution not-found preserves file-loss partiality.
    #[test]
    fn oracle_tonic_status_preserves_stale_object_classification() {
        assert!(matches!(
            execution_status_error(&Status::not_found("stale pinned object")),
            DispatchError::FileNotFound
        ));
        assert!(matches!(
            status_error(&Status::not_found("missing peer")),
            DispatchError::Terminal
        ));
        assert!(matches!(
            status_error(&Status::unavailable("storage outage")),
            DispatchError::Terminal
        ));
        assert!(matches!(
            status_error(&Status::cancelled("cancelled")),
            DispatchError::Unavailable
        ));
        assert!(matches!(
            status_error(&Status::deadline_exceeded("deadline")),
            DispatchError::Unavailable
        ));
    }

    /// A live Scribe open refusal separates availability from resource and trust faults.
    ///
    /// Only `Unavailable` may become a degradable live loss; capacity stays a
    /// resource fault and a denied ticket stays terminal.
    #[test]
    fn live_scribe_open_status_separates_availability_from_faults() {
        assert!(matches!(
            live_execution_status_error(&Status::unavailable("scribe outage")),
            DispatchError::Unavailable
        ));
        assert!(matches!(
            live_execution_status_error(&Status::resource_exhausted("follower lease")),
            DispatchError::Capacity
        ));
        assert!(matches!(
            live_execution_status_error(&Status::permission_denied("ticket")),
            DispatchError::Terminal
        ));
        assert!(matches!(
            live_execution_status_error(&Status::aborted("foreign tenant")),
            DispatchError::TenantInvariant
        ));
    }

    /// A status that ends an already-open worker stream keeps the tenant class.
    ///
    /// The footer tenant proof runs when the stream is first polled, after the
    /// schema frame, so its refusal arrives here rather than at open. Folding
    /// `Aborted` into availability would let a live read degrade past a
    /// tenant refusal and skip the leader's refusal audit.
    #[test]
    fn open_stream_status_preserves_tenant_refusal() {
        assert!(matches!(
            stream_status_error(&Status::aborted("foreign tenant")),
            DispatchError::TenantInvariant
        ));
        assert!(matches!(
            stream_status_error(&Status::unavailable("scribe outage")),
            DispatchError::Unavailable
        ));
        assert!(matches!(
            stream_status_error(&Status::permission_denied("ticket")),
            DispatchError::Terminal
        ));
    }

    /// The attempt encoder emits one schema frame, hashes batch payloads in
    /// stream order, and seals its running counters into the physical footer.
    ///
    /// The footer's row and byte counters are what the leader validates against
    /// the delivered frames, and a mid-stream schema change is the contract
    /// violation the leader cannot reconcile, so both are pinned here.
    #[test]
    fn dispatcher_attempt_encoder_frames_one_schema_then_a_counted_footer() {
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![Arc::new(Int64Array::from(vec![1, 2, 3]))],
        )
        .expect("fixture batch");

        let mut encoder = AttemptEncoder::default();
        assert!(matches!(
            encoder.start(Arc::clone(&schema)).expect("schema frame"),
            WorkerAttemptFrame::Schema(_)
        ));
        let (repeated_schema, first) = encoder.encode(&batch).expect("first batch encodes");
        assert!(repeated_schema.is_none());
        assert!(matches!(first, WorkerAttemptFrame::Batch(_)));
        encoder.encode(&batch).expect("second batch encodes");

        let widened = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            true,
        )]));
        let widened_batch =
            RecordBatch::try_new(widened, vec![Arc::new(Int64Array::from(vec![Some(4_i64)]))])
                .expect("widened batch");
        assert_eq!(
            encoder
                .encode(&widened_batch)
                .expect_err("schema is immutable"),
            AttemptEncodeError::Schema
        );

        let footer = encoder
            .finish_physical("plan-fingerprint", WorkerScanStats::default())
            .expect("physical footer");
        let WorkerAttemptFrame::Footer(footer) = footer else {
            panic!("finish_physical yields a footer frame");
        };
        assert_eq!(footer.fragment_id, "plan-fingerprint");
        assert_eq!(footer.manifest_digest.as_str(), "plan-fingerprint");
        assert_eq!(footer.row_count, 6);
        assert!(footer.encoded_bytes > 0);
        assert!(footer.completed);
    }

    /// An attempt that never produced a schema frame has no footer to finalize.
    #[test]
    fn dispatcher_attempt_encoder_refuses_a_footer_without_a_schema() {
        assert_eq!(
            AttemptEncoder::default()
                .finish_physical("plan-fingerprint", WorkerScanStats::default())
                .expect_err("an unstarted attempt has no footer"),
            AttemptEncodeError::Empty
        );
    }

    /// Issues one throwaway CA and the leaf both the peer and dialer present.
    ///
    /// Returns the CA PEM, the leaf certificate PEM, and the leaf key PEM, with
    /// the leaf carrying [`PEER_SERVER_NAME`] as its DNS name.
    fn peer_pki() -> (String, String, String) {
        use rcgen::{
            BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
        };
        let ca_key = KeyPair::generate().expect("CA key generates");
        let mut ca = CertificateParams::new(Vec::<String>::new()).expect("CA params build");
        ca.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let ca_pem = ca.self_signed(&ca_key).expect("CA self-signs").pem();
        let issuer = Issuer::new(ca, ca_key);
        let leaf_key = KeyPair::generate().expect("leaf key generates");
        let mut leaf =
            CertificateParams::new(vec![PEER_SERVER_NAME.to_owned()]).expect("leaf params build");
        leaf.extended_key_usages = vec![
            ExtendedKeyUsagePurpose::ServerAuth,
            ExtendedKeyUsagePurpose::ClientAuth,
        ];
        let leaf_pem = leaf
            .signed_by(&leaf_key, &issuer)
            .expect("leaf is signed")
            .pem();
        (ca_pem, leaf_pem, leaf_key.serialize_pem())
    }

    /// DNS identity every test peer presents and every dial verifies.
    const PEER_SERVER_NAME: &str = "peer.bifrost.test";

    /// Peer service that refuses every grant at once, which holds no state.
    struct RefusingPeer;

    #[async_trait]
    impl wyrd_tonic::wyrd::v1::oracle_peer_service_server::OraclePeerService for RefusingPeer {
        /// Grant stream type; every grant is refused in one message.
        type ReserveSlotsStream = Pin<
            Box<
                dyn Stream<Item = Result<wyrd_tonic::wyrd::v1::ReserveNodeSlotsResponse, Status>>
                    + Send,
            >,
        >;
        /// Unused worker stream type.
        type ExecuteFragmentStream = Pin<
            Box<dyn Stream<Item = Result<wyrd_tonic::wyrd::v1::WorkerAttemptFrame, Status>> + Send>,
        >;
        /// Unused forwarded-query stream type.
        type ForwardQueryStream = Pin<
            Box<dyn Stream<Item = Result<wyrd_tonic::wyrd::v1::QueryStreamFrame, Status>> + Send>,
        >;

        /// Refuses every grant stream with its retry hint and ends it.
        async fn reserve_slots(
            &self,
            _: Request<wyrd_tonic::wyrd::v1::ReserveNodeSlotsRequest>,
        ) -> Result<wyrd_tonic::tonic::Response<Self::ReserveSlotsStream>, Status> {
            let refused = wyrd_tonic::wyrd::v1::ReserveNodeSlotsResponse::from(
                ReserveNodeSlotsResponse::Rejected(ReservationRejected { retry_after_ms: 1 }),
            );
            Ok(wyrd_tonic::tonic::Response::new(Box::pin(
                futures_util::stream::once(async move { Ok(refused) }),
            )))
        }

        /// Refuses execution; the proof never opens a fragment.
        async fn execute_fragment(
            &self,
            _: Request<wyrd_tonic::wyrd::v1::ExecuteFragmentRequest>,
        ) -> Result<wyrd_tonic::tonic::Response<Self::ExecuteFragmentStream>, Status> {
            Err(Status::unimplemented("execute"))
        }

        /// Refuses forwarding; the proof never forwards.
        async fn forward_query(
            &self,
            _: Request<wyrd_tonic::wyrd::v1::ForwardQueryRequest>,
        ) -> Result<wyrd_tonic::tonic::Response<Self::ForwardQueryStream>, Status> {
            Err(Status::unimplemented("forward"))
        }
    }

    /// Serves [`RefusingPeer`] over mutual TLS and counts accepted connections.
    ///
    /// Returns the `https` endpoint and the accepted-connection counter.
    async fn counting_peer(
        ca_pem: &str,
        leaf_pem: &str,
        key_pem: &str,
    ) -> (String, Arc<std::sync::atomic::AtomicUsize>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("peer listener binds");
        let port = listener
            .local_addr()
            .expect("listener has an address")
            .port();
        let accepted = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = Arc::clone(&accepted);
        let incoming = async_stream::stream! {
            loop {
                let accepted = listener.accept().await.map(|(stream, _)| stream);
                counter.fetch_add(1, Ordering::SeqCst);
                yield accepted;
            }
        };
        let mut server = wyrd_tonic::server::mutual_tls_server(
            wyrd_tonic::server::MutualTlsServerConfig::from_pem(
                leaf_pem.as_bytes(),
                key_pem.as_bytes(),
                ca_pem.as_bytes(),
            ),
        )
        .expect("mutual TLS server builds");
        tokio::spawn(
            server
                .add_service(
                    wyrd_tonic::wyrd::v1::oracle_peer_service_server::OraclePeerServiceServer::new(
                        RefusingPeer,
                    ),
                )
                .serve_with_incoming(incoming),
        );
        (format!("https://127.0.0.1:{port}"), accepted)
    }

    /// Repeated calls to one ready peer incarnation share one authenticated
    /// connection, while a new fence or a new endpoint dials afresh.
    ///
    /// Each step completes a real mutually authenticated RPC, so a reused
    /// channel is proven usable rather than merely cached. The TCP accept
    /// count on each peer is the connection evidence.
    #[tokio::test]
    async fn peer_transport_reuses_tls_channel_for_same_ready_node() {
        let (ca_pem, leaf_pem, key_pem) = peer_pki();
        let (first, first_accepts) = counting_peer(&ca_pem, &leaf_pem, &key_pem).await;
        let (second, second_accepts) = counting_peer(&ca_pem, &leaf_pem, &key_pem).await;
        let node = NodeId::new(uuid::Uuid::now_v7());
        let transport = TonicOraclePeerTransport::with_test_tls(
            HashMap::new(),
            BifrostPeerTls::new(
                ca_pem.into_bytes(),
                PEER_SERVER_NAME.to_owned(),
                leaf_pem.into_bytes(),
                secrecy::SecretString::from(key_pem),
            ),
        );
        let candidate = |fence: FencingToken, endpoint: &str| DispatchCandidate {
            node_id: node,
            role: wyrd_spec::vala::api::ClusterRole::Oracle,
            worker_fence: fence,
            endpoint: Some(endpoint.to_owned()),
        };
        let round_trip = |candidate: DispatchCandidate| {
            let transport = &transport;
            async move {
                let refused = transport
                    .client(&candidate)
                    .await
                    .expect("peer is reachable")
                    .reserve_slots(wyrd_tonic::wyrd::v1::ReserveNodeSlotsRequest::default())
                    .await
                    .expect("authenticated grant stream opens")
                    .into_inner()
                    .message()
                    .await
                    .expect("authenticated refusal completes");
                assert!(refused.is_some(), "the refusal is one real message");
            }
        };

        for _ in 0..3 {
            round_trip(candidate(1, &first)).await;
        }
        assert_eq!(
            first_accepts.load(Ordering::SeqCst),
            1,
            "one ready incarnation shares one connection"
        );

        round_trip(candidate(2, &first)).await;
        assert_eq!(
            first_accepts.load(Ordering::SeqCst),
            2,
            "a new fence dials a new connection"
        );

        round_trip(candidate(2, &second)).await;
        round_trip(candidate(2, &second)).await;
        assert_eq!(
            second_accepts.load(Ordering::SeqCst),
            1,
            "a new endpoint dials once and reuses"
        );
        assert_eq!(
            first_accepts.load(Ordering::SeqCst),
            2,
            "the relocated peer never reuses the old endpoint"
        );
    }
}
