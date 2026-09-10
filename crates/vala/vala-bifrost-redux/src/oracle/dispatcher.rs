//! Bounded local and tonic sealed-fragment dispatch.

use std::collections::HashMap;
use std::fmt;
use std::pin::Pin;
use std::sync::atomic::Ordering;
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
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::api::{
    AnalyticalGraphRef, BifrostSecurityViolationKind, ExecuteFragmentRequest, FencingToken, NodeId,
    OracleRoleFence, PendingNodeReservation, QueryAuditDigest, QueryClass, QueryId,
    ReleaseNodeSlotsRequest, ReservationId, ReservationRejected, ReserveNodeSlotsRequest,
    ReserveNodeSlotsResponse, WorkerAttemptFrame, WorkerFooter, WorkerScanStats,
};
use wyrd_tonic::prost::Message;
use wyrd_tonic::tonic::metadata::MetadataValue;
use wyrd_tonic::tonic::transport::Channel;
use wyrd_tonic::tonic::{Request, Status};
use wyrd_tonic::wyrd::v1::oracle_peer_service_client::OraclePeerServiceClient;

use super::OracleSlotManager;
use super::attempt::{AttemptBuffer, AttemptError, PartialAttempt, ValidatedAttempt};
use super::follower::{
    AuthenticatedFollowerContext, FollowerSessionFactory, FollowerSourceResolver,
    PhysicalPlanFollower, PhysicalPlanFollowerError,
};
use super::peer::{
    PeerSecurityAudit, PeerSecurityError, PeerTicketClaims, PeerTicketMinter, PeerTicketVerifier,
    ReservationBinding, ReservationOperationV1, ReservationTicketClaims, ReservationTicketMinter,
    reservation_body_digest,
};
#[cfg(feature = "test-support")]
use super::reader_pins::OracleReaderAuthority;
use super::telemetry::{
    FragmentLocality, FragmentOutcome, FragmentTelemetry, PeerErrorClass, SecurityEventClass,
    SlotOutcome, record_peer_attempt, record_security, record_slot,
};
use crate::cluster::{ClusterRegistry, ClusterSnapshot, ROLE_LIVENESS_CUTOFF};

/// Fixed private peer protocol version carried in signed peer ticket claims.
///
/// The minter stamps this value into [`PeerTicketClaims::protocol_version`] and
/// the verifier requires it exactly, so a ticket minted by a binary speaking a
/// different peer wire is rejected instead of being decoded against the wrong
/// claim encoding. Both sides read this one constant, so the check cannot
/// desynchronize within a build.
///
/// Protocol v4 signs a separate execution deadline so accepted followers can
/// outlive ticket acceptance expiry. This homogeneous cutover rejects older
/// claims through [`validated_claim_identifiers`] without a deadline fallback.
pub const PEER_PROTOCOL_VERSION: u32 = 4;
/// Pending reservation time to live.
///
/// Shared with the Analytical leader's retained-release bound so a leader that
/// cannot confirm a release waits out exactly the window the follower grants a
/// pending reservation, rather than a second copy of the same duration.
pub(super) const PENDING_TTL: ChronoDuration = ChronoDuration::seconds(2);
/// Stable peer rejection hint.
const RESERVATION_RETRY_MS: u32 = 1_000;

/// Closed transport failure classification used by terminal dispatch policy.
#[derive(Debug, Error)]
pub enum DispatchError {
    /// Pinned partial outcome with every previously decoded batch retained.
    #[error("peer attempt completed partially")]
    Partial {
        /// Delivered batch payloads, absent when setup failed before delivery.
        attempt: Option<PartialAttempt>,
        /// Stable partition-local reason selected at the failing boundary.
        reason: DispatchPartialReason,
    },
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
    /// The tenant tripwire refused a physically scanned foreign-tenant row.
    ///
    /// Kept distinct from [`DispatchError::Terminal`] so the leader reports
    /// the tenant-isolation reason rather than a generic peer-security or
    /// retryable-worker outcome. It is never retried on another candidate:
    /// the refusal is a property of the data, not of the worker.
    #[error("peer fragment refused a foreign-tenant row")]
    TenantInvariant,
}

impl DispatchError {
    /// Whether the leader's partition classification depends on this failure
    /// keeping its own identity.
    ///
    /// Three failures make
    /// [`classify_partition_attempt`](super::exec::classify_partition_attempt)
    /// fail the partition outright: [`Self::Terminal`] is a contract or
    /// peer-security violation, [`Self::TenantInvariant`] is a physically
    /// scanned foreign-tenant row, and [`Self::StaleObject`] means the pinned
    /// cut moved and the query owes a replan. Reporting any of them as a
    /// partial converts a refusal into a degraded success — for the tenant
    /// tripwire that is a silent isolation breach, because the leader would
    /// return the surviving participants' rows and blame a timeout.
    ///
    /// Every other failure only degrades the partition, so a boundary is free
    /// to soften it into whichever [`DispatchPartialReason`] describes where it
    /// happened.
    ///
    /// This is the single authority for that split. Both dispatch boundaries —
    /// stream open and mid-stream frame delivery — ask here rather than each
    /// carrying its own list, because the two lists previously disagreed and
    /// the tenant refusal fell through the gap.
    const fn must_reach_leader_unchanged(&self) -> bool {
        match self {
            Self::Terminal | Self::TenantInvariant | Self::StaleObject => true,
            Self::Partial { .. }
            | Self::Unavailable
            | Self::EligibleSourceLoss { .. }
            | Self::Capacity
            | Self::FileNotFound => false,
        }
    }
}

/// Longest a peer waits out a saturated running-slot pool before refusing.
///
/// Sized far below the query deadline so peer backpressure never becomes a
/// caller-visible timeout — the failure mode where an uncoordinated worker-side
/// queue outlives the dispatch RPC and surfaces as a transport error instead of
/// a clean refusal. Long enough to absorb the brief contention that a fan-out
/// across several peers otherwise turns into a failed query.
const PEER_SLOT_WAIT: std::time::Duration = std::time::Duration::from_millis(50);

/// Interval between running-slot retries inside [`PEER_SLOT_WAIT`].
const PEER_SLOT_POLL: std::time::Duration = std::time::Duration::from_millis(5);

/// Closed reasons accompanying a partial peer attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchPartialReason {
    /// Ticket, request, channel, or stream-open construction failed.
    Setup,
    /// Deadline or cancellation selected before normal footer completion.
    Timeout,
    /// Delivered frame or payload decoding ended after prior batches.
    Decoder,
}

/// Closed causes that permit explicit degraded source completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EligibleSourceLossCause {
    /// The authenticated role-local provider could not resolve its pinned source.
    ProviderResolution,
}

/// Bounded role-fence-scoped pending reservation.
#[derive(Debug)]
struct PendingReservation {
    /// Query identity bound to the reservation.
    query_id: QueryId,
    /// Leader identity bound to the reservation.
    leader_node_id: NodeId,
    /// Leader fence preventing stale release.
    leader_fencing_token: FencingToken,
    /// Admission class used by closed slot telemetry.
    query_class: QueryClass,
    /// Pending expiry used for eager reclamation.
    expires_at: DateTime<Utc>,
    /// Resources this node charged when it accepted the reservation, and the
    /// purpose it charged them for.
    ///
    /// Reservation is the whole capacity gate. The charged envelope or worker
    /// quantum holds this node's aggregate slot units in the shared governor
    /// ledger and its Oracle memory, so a reserved fragment can always execute:
    /// a leader never dispatches to a node that has not already seated it.
    /// Leader-local work reuses the admitted query's own envelope and keeps this
    /// empty.
    capacity: Option<ReservedCapacity>,
    /// Graph this reservation may only ever be leased to, when it names one.
    graph: Option<AnalyticalGraphRef>,
}

/// What a follower charged when it accepted one reservation.
///
/// The two purposes are different quantities of the same budget, and neither
/// may be spent as the other: a fragment charges one worker quantum released
/// when its attempt stream ends, while a distributed Analytical graph charges a
/// whole query envelope — pool, partitions, and scratch — owned by the graph
/// lease for as long as the graph lives.
#[derive(Debug)]
pub(crate) enum ReservedCapacity {
    /// Worker quantum a single dispatched fragment executes under.
    Fragment(FollowerWorkerResources),
    /// Query envelope one distributed Analytical graph executes under.
    Graph(Box<crate::resources::OracleQueryResources>),
}

/// Everything a follower must prove before one reservation becomes a graph.
///
/// A graph lease is the largest thing a peer can be talked into charging, so
/// the reservation must have been taken for exactly this query and exactly this
/// graph before its envelope changes owner. Nothing here is read from the wire
/// framing; the caller projects every field from verified stage claims.
///
/// The reservation's leader identity and fence are deliberately not re-derived
/// here. They were checked when the reservation was accepted, and the
/// coordinator that presents the first stage message for a graph is not always
/// the leader — a follower running a middle stage is itself a coordinator and
/// legitimately signs under its own identity. The stage ticket independently
/// binds the presenting principal, this follower's node identity, and its
/// current role fence before this is ever reached. The activator retains that
/// reserving identity separately, in its own immutable graph binding.
#[derive(Debug, Clone, Copy)]
pub struct GraphLeaseRequest {
    /// Reservation the leader took on this node for this graph.
    pub reservation_id: ReservationId,
    /// Exact graph the reservation was taken for.
    pub graph: AnalyticalGraphRef,
    /// Query identity the reservation was bound to.
    pub query_id: QueryId,
}

/// One follower's reservation held across a fallible graph activation.
///
/// This is the transaction that replaced a destructive transfer. The pending
/// entry — envelope, reserving leader and fence, and above all its
/// *original* expiry — is removed from the registry and retained here
/// unchanged while the activator does the fallible work: building the query
/// runtime and registering the graph with its supervisor. Exactly one of
/// [`PendingGraphActivation::commit`] or [`PendingGraphActivation::rollback`]
/// then decides whether the reservation became a graph or goes back on the
/// shelf, so no failure path can leave the follower charged for a graph that
/// does not exist, and no failure path can destroy a reservation that is still
/// valid.
pub struct PendingGraphActivation {
    /// Registry this reservation is restored into when activation fails.
    registry: Arc<ReservationRegistry>,
    /// Identity of the reservation held open by this activation.
    reservation_id: ReservationId,
    /// Graph the reservation may only ever become.
    graph: AnalyticalGraphRef,
    /// The removed pending entry, retained verbatim until commit or rollback.
    ///
    /// Cleared by whichever of the two runs, so the drop guard can tell an
    /// abandoned activation from a settled one.
    entry: Option<PendingReservation>,
}

impl fmt::Debug for PendingGraphActivation {
    /// Reports the activation's identity without rendering retained ownership.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PendingGraphActivation")
            .field("reservation_id", &self.reservation_id.as_uuid())
            .field("settled", &self.entry.is_none())
            .finish_non_exhaustive()
    }
}

impl PendingGraphActivation {
    /// Returns the leader node and fence the reservation was accepted under.
    ///
    /// This is reservation ownership, not per-message stage authority. The
    /// activator retains it so a later coordinator can be authorized as either
    /// this exact pair or an exact member of the immutable destination cut.
    ///
    /// # Panics
    ///
    /// Panics when the activation has already committed or rolled back, which
    /// is unreachable: both consume `self`.
    #[must_use]
    pub(crate) fn reserving_leader(&self) -> (NodeId, FencingToken) {
        let entry = self
            .entry
            .as_ref()
            .expect("a live activation owns its entry");
        (entry.leader_node_id, entry.leader_fencing_token)
    }

    /// Returns the reservation's original expiry, which activation never extends.
    ///
    /// # Panics
    ///
    /// Panics when the activation has already committed or rolled back.
    #[must_use]
    pub(crate) fn expires_at(&self) -> DateTime<Utc> {
        self.entry
            .as_ref()
            .expect("a live activation owns its entry")
            .expires_at
    }

    /// Borrows the reserved query envelope without taking ownership of it.
    ///
    /// Building the graph's bounded runtime needs only the envelope's pool and
    /// scratch share, and it can fail. Lending rather than taking is what keeps
    /// a failed runtime build recoverable: the envelope is still here, so the
    /// rollback restores a reservation a later activator can still use.
    ///
    /// # Panics
    ///
    /// Panics when the activation has already committed or rolled back, or when
    /// the retained entry is not a graph reservation. `begin_graph_activation`
    /// refuses every other shape before this is reachable.
    #[must_use]
    pub(crate) fn envelope(&self) -> &crate::resources::OracleQueryResources {
        match self
            .entry
            .as_ref()
            .and_then(|entry| entry.capacity.as_ref())
        {
            Some(ReservedCapacity::Graph(resources)) => resources,
            _ => unreachable!("a graph activation always retains a graph envelope"),
        }
    }

    /// Moves the reserved envelope into `register`, keeping it on failure.
    ///
    /// `register` is the single fallible act that changes the envelope's owner:
    /// it takes the admitted resources and returns whatever owns them from then
    /// on — in production, the supervisor's graph guard. A registration that
    /// fails must hand the resources back, because the reservation this
    /// activation restores is only usable again if it is restored complete.
    ///
    /// On success the residue — the admission class — is returned for the lease
    /// to own, and the activation's cumulative counter is advanced exactly once.
    ///
    /// # Errors
    ///
    /// Returns the unchanged activation alongside `register`'s error, so the
    /// caller can still roll back under the original expiry.
    pub(crate) fn commit<T, F>(
        mut self,
        register: F,
    ) -> Result<(CommittedGraphActivation, T), (Box<Self>, BifrostError)>
    where
        F: FnOnce(
            crate::resources::OracleQueryResources,
        )
            -> Result<T, (Box<crate::resources::OracleQueryResources>, BifrostError)>,
    {
        let mut entry = self.entry.take().expect("a live activation owns its entry");
        let Some(ReservedCapacity::Graph(resources)) = entry.capacity.take() else {
            unreachable!("a graph activation always retains a graph envelope")
        };
        match register(*resources) {
            Ok(owner) => {
                let committed = CommittedGraphActivation {
                    reservation_id: self.reservation_id,
                    graph: self.graph,
                    query_class: entry.query_class,
                };
                #[cfg(any(test, feature = "test-support"))]
                self.registry
                    .graph_leases_activated_total
                    .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                tracing::debug!(
                    public_query_id = %self.graph.public_query_id,
                    datafusion_query_id = %self.graph.datafusion_query_id,
                    "Oracle graph lease activated from its reservation"
                );
                Ok((committed, owner))
            }
            Err((resources, error)) => {
                entry.capacity = Some(ReservedCapacity::Graph(resources));
                self.entry = Some(entry);
                Err((Box::new(self), error))
            }
        }
    }

    /// Returns the unchanged reservation to the registry, or drops it.
    ///
    /// Restoration is conditional on the reservation's *own* original expiry,
    /// never on a fresh one: a failed activation may not buy the leader more
    /// time than it was granted. An expired or displaced reservation releases
    /// its exact envelope instead, which returns this follower to
    /// baseline rather than stranding capacity for a graph that never existed.
    pub(crate) fn rollback(mut self, now: DateTime<Utc>) {
        let Some(entry) = self.entry.take() else {
            return;
        };
        self.registry.restore(self.reservation_id, entry, now);
    }
}

impl Drop for PendingGraphActivation {
    /// Rolls back an activation abandoned by cancellation, panic, or early return.
    ///
    /// The settled paths clear the entry and leave nothing to do here. This
    /// covers the activator that simply went away, where the alternative is an
    /// envelope no owner can ever return.
    fn drop(&mut self) {
        let Some(entry) = self.entry.take() else {
            return;
        };
        self.registry
            .restore(self.reservation_id, entry, Utc::now());
    }
}

/// The reservation residue one activated graph lease owns for the graph's life.
///
/// Everything else the reservation held has changed owner: the envelope — and
/// with it this node's aggregate slot units — moved into the supervisor's graph
/// state, and the pending entry is gone. What remains is the class the graph was
/// charged under, which its telemetry and settlement still name.
#[derive(Debug)]
pub struct CommittedGraphActivation {
    /// Reservation this graph was activated from.
    reservation_id: ReservationId,
    /// Graph this residue belongs to.
    graph: AnalyticalGraphRef,
    /// Admission class the graph's envelope was charged under.
    query_class: QueryClass,
}

impl CommittedGraphActivation {
    /// Returns the reservation this graph was activated from.
    #[must_use]
    pub fn reservation_id(&self) -> ReservationId {
        self.reservation_id
    }

    /// Returns the graph this residue belongs to.
    #[must_use]
    pub fn graph(&self) -> AnalyticalGraphRef {
        self.graph
    }

    /// Returns the admission class the graph's envelope was charged under.
    #[must_use]
    pub fn query_class(&self) -> QueryClass {
        self.query_class
    }
}

/// Running worker reservation retained through attempt-stream completion.
#[derive(Debug)]
pub struct RunningReservation {
    /// Authenticated query class carried into worker-owned scan telemetry.
    pub(crate) query_class: QueryClass,
    /// Remote-worker resources transferred from the reservation.
    ///
    /// Retained for the whole attempt stream so the bounded `DataFusion` pool
    /// this fragment executes under stays charged until the stream completes,
    /// fails, or is dropped. Leader-local execution leaves this empty and uses
    /// the admitted query's own pool. Dropping it returns this node's slot
    /// units to the shared governor ledger and wakes queued leaders.
    pub(crate) worker_resources: Option<FollowerWorkerResources>,
}

/// In-memory worker reservation owner; entries are never durable.
#[derive(Debug)]
pub struct ReservationRegistry {
    /// Pending reservations keyed by their unguessable identities.
    entries: Mutex<HashMap<ReservationId, PendingReservation>>,
    /// Hard bound on retained pending entries for this worker role.
    capacity: usize,
    /// Role-scoped pending and running slot owner.
    slots: Arc<OracleSlotManager>,
    /// Cumulative count of successful pending-to-running admissions on this peer.
    ///
    /// Incremented only in `take_for_execute`'s success arm — the remote-worker
    /// admission site that charges this node's running semaphore — and never on
    /// the leader-local path, which charges no peer running capacity. It exists
    /// solely so a cross-pod integration test can assert that at least one query
    /// fragment was admitted to run on a non-leader peer, which is otherwise
    /// unobservable (the fragment span carries locality but no outcome, and the
    /// outcome metric carries no locality). It is `test-support`-gated: no
    /// field, cost, or behavior exists on the production path.
    #[cfg(feature = "test-support")]
    admitted_running_total: core::sync::atomic::AtomicU64,
    /// Cumulative count of graph leases this node activated from a reservation.
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
    pub fn new(slots: Arc<OracleSlotManager>, capacity: usize) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            capacity,
            slots,
            #[cfg(feature = "test-support")]
            admitted_running_total: core::sync::atomic::AtomicU64::new(0),
            #[cfg(any(test, feature = "test-support"))]
            graph_leases_activated_total: core::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Returns the cumulative count of successful running admissions on this peer.
    ///
    /// Integration-only observable for asserting that at least one fragment was
    /// admitted to run on a non-leader peer. Reflects only `take_for_execute`
    /// successes; the leader-local transition never contributes.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn admitted_running_total(&self) -> u64 {
        self.admitted_running_total
            .load(core::sync::atomic::Ordering::Relaxed)
    }

    /// Returns this registry's slot manager for waiter-bound admission.
    pub(crate) fn slots(&self) -> &Arc<OracleSlotManager> {
        &self.slots
    }

    /// Atomically reserves one pending worker slot and returns its generated identity.
    ///
    /// # Errors
    /// Returns a retryable failure when capacity, expiry, or local pending slots reject.
    pub(crate) fn reserve(
        &self,
        request: &ReserveNodeSlotsRequest,
        now: DateTime<Utc>,
        capacity: Option<ReservedCapacity>,
    ) -> Result<PendingNodeReservation, DispatchError> {
        if request.slot_units == 0 || request.expires_at <= now {
            return Err(DispatchError::Terminal);
        }
        // Slot units were already charged against the shared governor ledger by
        // whichever capability produced `capacity`, so there is no second local
        // semaphore to clamp leader-supplied demand against here.
        self.insert(request, now, capacity)
    }

    /// Reserves tuple-bound leader-local work under admitted query capacity.
    ///
    /// The local query already owns this process's admission budget. The
    /// reservation retains expiry, registry-capacity, and ownership checks
    /// without charging the shared governor slot ledger a second time.
    ///
    /// # Errors
    /// Returns terminal for invalid demand or expiry and retryable when the
    /// bounded registry is unavailable or full.
    fn reserve_local(
        &self,
        request: &ReserveNodeSlotsRequest,
        now: DateTime<Utc>,
    ) -> Result<PendingNodeReservation, DispatchError> {
        self.insert(request, now, None)
    }

    /// Inserts one validated reservation with its explicit capacity owner.
    ///
    /// # Errors
    /// Returns terminal for invalid demand or expiry and retryable when the
    /// bounded registry is unavailable or full.
    fn insert(
        &self,
        request: &ReserveNodeSlotsRequest,
        now: DateTime<Utc>,
        capacity: Option<ReservedCapacity>,
    ) -> Result<PendingNodeReservation, DispatchError> {
        if request.slot_units == 0 || request.expires_at <= now {
            return Err(DispatchError::Terminal);
        }
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| DispatchError::Unavailable)?;
        retain_live(&mut entries, now);
        if entries.len() >= self.capacity {
            return Err(DispatchError::Unavailable);
        }
        let reservation_id = loop {
            let candidate = ReservationId::new(uuid::Uuid::now_v7());
            if !entries.contains_key(&candidate) {
                break candidate;
            }
        };
        let expires_at = request.expires_at.min(now + PENDING_TTL);
        entries.insert(
            reservation_id,
            pending_reservation(request, expires_at, capacity),
        );
        Ok(PendingNodeReservation {
            reservation_id,
            expires_at,
        })
    }

    /// Removes a reservation only when the complete ownership tuple matches.
    #[must_use]
    pub fn release(&self, request: &ReleaseNodeSlotsRequest, now: DateTime<Utc>) -> bool {
        let Ok(mut entries) = self.entries.lock() else {
            return false;
        };
        retain_live(&mut entries, now);
        let matches = entries
            .get(&request.reservation_id)
            .is_none_or(|entry| reservation_matches(entry, request));
        if matches {
            entries.remove(&request.reservation_id);
        }
        matches
    }

    /// Converts a matching, unexpired pending reservation into a running guard.
    ///
    /// The ownership tuple is checked before removal, so a forged execute cannot
    /// destroy another query's pending reservation.
    ///
    /// This transition cannot fail on capacity. The slot units were charged
    /// and stored when the reservation was accepted, so they are transferred
    /// here rather than acquired: a peer that answered `Pending` has already
    /// committed the capacity this fragment executes under, and the leader can
    /// treat a completed fan-out reservation as a guarantee that every
    /// participant will run.
    ///
    /// # Errors
    /// Returns terminal for missing, expired, or mismatched ownership.
    #[tracing::instrument(name = "bifrost.oracle.slot_reservation", skip_all)]
    pub fn take_for_execute(
        &self,
        reservation_id: ReservationId,
        query_id: QueryId,
        leader_node_id: NodeId,
        leader_fencing_token: FencingToken,
        now: DateTime<Utc>,
    ) -> Result<RunningReservation, DispatchError> {
        let mut entries = self.entries.lock().map_err(|_| DispatchError::Terminal)?;
        retain_live(&mut entries, now);
        let entry = entries
            .get(&reservation_id)
            .ok_or(DispatchError::Terminal)?;
        if entry.query_id != query_id
            || entry.leader_node_id != leader_node_id
            || entry.leader_fencing_token != leader_fencing_token
        {
            return Err(DispatchError::Terminal);
        }
        let query_class = entry.query_class;
        // Transfer, do not acquire. The slot units were charged when this
        // reservation was accepted, so a reserved fragment can always execute and
        // this transition cannot fail on capacity.
        let mut entry = entries
            .remove(&reservation_id)
            .ok_or(DispatchError::Terminal)?;
        // A graph reservation is not spendable here. Its envelope belongs to
        // the graph lease, and letting a fragment consume it would leave the
        // graph executing on capacity nothing owns.
        let worker_resources = match entry.capacity.take() {
            Some(ReservedCapacity::Fragment(resources)) => Some(resources),
            Some(ReservedCapacity::Graph(_)) => return Err(DispatchError::Terminal),
            None => None,
        };
        let result = Ok(RunningReservation {
            query_class,
            worker_resources,
        });
        #[cfg(feature = "test-support")]
        self.admitted_running_total
            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        record_slot(
            query_class,
            if result.is_ok() {
                SlotOutcome::Running
            } else {
                SlotOutcome::Rejected
            },
        );
        result
    }

    /// Converts a matching local-leader reservation without charging its slot twice.
    ///
    /// This transition is restricted to the in-process transport. Its caller must
    /// retain the admitted query guard whose running permit covers the local
    /// fragment and subsequent leader-owned operators. The complete ownership
    /// tuple is still checked and consumed before fragment decoding or object IO.
    ///
    /// # Errors
    /// Returns terminal for missing, expired, or mismatched ownership.
    #[tracing::instrument(name = "bifrost.oracle.slot_reservation", skip_all)]
    fn take_for_local_leader_execute(
        &self,
        reservation_id: ReservationId,
        query_id: QueryId,
        leader_node_id: NodeId,
        leader_fencing_token: FencingToken,
        now: DateTime<Utc>,
    ) -> Result<RunningReservation, DispatchError> {
        let mut entries = self.entries.lock().map_err(|_| DispatchError::Terminal)?;
        retain_live(&mut entries, now);
        let entry = entries
            .get(&reservation_id)
            .ok_or(DispatchError::Terminal)?;
        if entry.query_id != query_id
            || entry.leader_node_id != leader_node_id
            || entry.leader_fencing_token != leader_fencing_token
        {
            return Err(DispatchError::Terminal);
        }
        let query_class = entry.query_class;
        entries
            .remove(&reservation_id)
            .ok_or(DispatchError::Terminal)?;
        record_slot(query_class, SlotOutcome::Running);
        Ok(RunningReservation {
            query_class,
            worker_resources: None,
        })
    }

    /// Opens one rollback-capable activation of a reservation into its graph.
    ///
    /// This is the only path from a reservation to a graph envelope, and it is
    /// deliberately not the transfer it replaced. The complete ownership tuple
    /// is checked *before* the pending entry moves anywhere, and what the caller
    /// receives is a transaction rather than a lease: until it commits, the
    /// reservation is still whole and still restorable under its own original
    /// expiry.
    ///
    /// Activation refuses before any worker, cache, or provider IO when the
    /// reservation is missing, expired, or was taken for a different query or
    /// graph — or for no graph at all, which is a fragment reservation whose
    /// worker quantum is far smaller than a graph envelope.
    ///
    /// # Errors
    ///
    /// Returns [`DispatchError::Terminal`] for a missing, expired, mismatched,
    /// or non-graph reservation, and [`DispatchError::Unavailable`] when the
    /// reservation lock is poisoned.
    #[tracing::instrument(name = "bifrost.oracle.graph_lease", skip_all)]
    pub(crate) fn begin_graph_activation(
        self: &Arc<Self>,
        request: &GraphLeaseRequest,
        now: DateTime<Utc>,
    ) -> Result<PendingGraphActivation, DispatchError> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| DispatchError::Unavailable)?;
        retain_live(&mut entries, now);
        let entry = entries
            .get(&request.reservation_id)
            .ok_or(DispatchError::Terminal)?;
        // Only a graph reservation may become a graph. A fragment reservation
        // charged one worker quantum, which cannot pay for a whole plan.
        if entry.query_id != request.query_id
            || entry.graph != Some(request.graph)
            || !matches!(entry.capacity, Some(ReservedCapacity::Graph(_)))
        {
            return Err(DispatchError::Terminal);
        }
        let entry = entries
            .remove(&request.reservation_id)
            .ok_or(DispatchError::Terminal)?;
        Ok(PendingGraphActivation {
            registry: Arc::clone(self),
            reservation_id: request.reservation_id,
            graph: request.graph,
            entry: Some(entry),
        })
    }

    /// Puts one unchanged reservation back, or releases it when it cannot be.
    ///
    /// Centralized here rather than in the activator because collision and
    /// expiry are the registry's own invariants: an entry may only return to a
    /// slot that is still vacant, and only while its own original expiry has
    /// not passed. Everything else drops the exact envelope, which is
    /// the honest outcome — the reservation the leader was promised is simply
    /// over.
    fn restore(
        &self,
        reservation_id: ReservationId,
        entry: PendingReservation,
        now: DateTime<Utc>,
    ) {
        if entry.expires_at <= now {
            return;
        }
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        retain_live(&mut entries, now);
        if entries.contains_key(&reservation_id) || entries.len() >= self.capacity {
            return;
        }
        entries.insert(reservation_id, entry);
    }

    /// Returns the greatest number of graphs this node may own at one time.
    ///
    /// Derived from this pod's immutable local slot-unit total against the
    /// per-graph slot demand, so a bounded queue sized from this cannot exceed
    /// what the shared governor ledger could ever admit. Never zero: a node
    /// that can admit one graph must be able to settle it.
    #[must_use]
    pub(crate) fn max_concurrent_graphs(&self) -> usize {
        let running = self.slots.total_slot_units().max(1);
        let units = usize::try_from(super::analytical::ANALYTICAL_GRAPH_SLOT_UNITS)
            .unwrap_or(1)
            .max(1)
            .min(running);
        (running / units).max(1)
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

    /// Reclaims expired pending reservations and returns the number still held.
    #[must_use]
    pub fn cleanup_expired(&self, now: DateTime<Utc>) -> usize {
        self.entries.lock().map_or(0, |mut entries| {
            retain_live(&mut entries, now);
            entries.len()
        })
    }
}

/// Converts a validated wire reservation into its capacity-owning registry entry.
fn pending_reservation(
    request: &ReserveNodeSlotsRequest,
    expires_at: DateTime<Utc>,
    capacity: Option<ReservedCapacity>,
) -> PendingReservation {
    PendingReservation {
        query_id: request.query_id,
        leader_node_id: request.leader_node_id,
        leader_fencing_token: request.leader_fencing_token,
        query_class: request.query_class,
        expires_at,
        capacity,
        graph: request.graph,
    }
}

/// Retains only unexpired pending entries, releasing their capacity immediately.
fn retain_live(entries: &mut HashMap<ReservationId, PendingReservation>, now: DateTime<Utc>) {
    entries.retain(|_, entry| entry.expires_at > now);
}

/// Compares the exact query and fenced leader release tuple.
fn reservation_matches(entry: &PendingReservation, request: &ReleaseNodeSlotsRequest) -> bool {
    entry.query_id == request.query_id
        && entry.leader_node_id == request.leader_node_id
        && entry.leader_fencing_token == request.leader_fencing_token
}

/// Incremental dispatch stream shared by local and tonic transports.
pub type WorkerAttemptStream =
    Pin<Box<dyn Stream<Item = Result<WorkerAttemptFrame, DispatchError>> + Send>>;

/// Worker execution whose stream owns its running reservation until drop.
pub struct WorkerExecution {
    /// Incremental footer-terminated frames with cancellation-bound ownership.
    pub stream: WorkerAttemptStream,
}

/// Exact role-root quantum retained by one remote follower stream.
#[derive(Debug)]
pub(crate) enum FollowerWorkerResources {
    /// Oracle floor/elastic ownership for persisted execution.
    Oracle(crate::resources::OracleWorkerResources),
}

impl FollowerWorkerResources {
    /// Returns the exact bounded `DataFusion` pool retained by this role lease.
    fn memory_pool(&self) -> Arc<dyn datafusion::execution::memory_pool::MemoryPool> {
        match self {
            Self::Oracle(resources) => resources.memory_pool(),
        }
    }

    /// Returns the trusted grant this role lease was charged for.
    fn granted_memory_bytes(&self) -> usize {
        match self {
            Self::Oracle(resources) => resources.granted_memory_bytes(),
        }
    }

    /// Returns the partition ceiling admitted alongside that grant.
    fn admitted_target_partitions(&self) -> usize {
        match self {
            Self::Oracle(resources) => resources.admitted_target_partitions(),
        }
    }
}

/// Worker-side owner for verify, reservation transition, fragment validation, and IO.
pub struct OraclePeerWorker {
    /// Node identity required by every ticket audience.
    worker_node_id: NodeId,
    /// Current role fence required by every ticket.
    oracle_fence: FencingToken,
    /// Raw-ticket authority used before fragment decoding.
    verifier: Arc<dyn PeerTicketVerifier>,
    /// Durable collaborator used before returning verified claim failures.
    security_audit: Arc<dyn PeerSecurityAudit>,
    /// Tuple-bound pending-to-running transition owner.
    reservations: Arc<ReservationRegistry>,
    /// Root Oracle capability used by every remote execution quantum.
    oracle_resources: crate::resources::OracleResources,
    /// Native physical-plan follower built only from injected process capabilities.
    physical_follower: Arc<PhysicalPlanFollower<Arc<dyn FollowerSourceResolver>>>,
    /// Test-tier observer attached to the exact production physical path.
    physical_observer: Arc<PhysicalWorkerObserver>,
}

/// Directly observed native follower activity for one production worker.
#[cfg(feature = "test-support")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalWorkerInspection {
    /// Physical process identity that executed the fragments.
    pub node_id: NodeId,
    /// Oracle catalog-backed follower executions begun.
    pub oracle_executions: u64,
    /// Footer frames emitted after complete physical execution.
    pub footers_emitted: u64,
    /// Rows handed to attempt encoding across every executed fragment.
    ///
    /// This is the row count that actually crosses the follower wire, so a
    /// signed predicate applied inside the source — the Scribe live tail
    /// among them — is observable here as strictly fewer encoded rows for
    /// the same final result.
    pub rows_encoded: u64,
}

/// Shared counters retained across the worker and its emitted streams.
#[derive(Debug, Default)]
struct PhysicalWorkerObserver {
    /// Oracle catalog-backed follower executions begun.
    oracle_executions: std::sync::atomic::AtomicU64,
    /// Footer frames emitted after complete physical execution.
    footers_emitted: std::sync::atomic::AtomicU64,
    /// Rows handed to attempt encoding across every executed fragment.
    rows_encoded: std::sync::atomic::AtomicU64,
}

/// Complete construction inputs for one production [`OraclePeerWorker`].
///
/// Boot, journey fixtures, and unit tests all build the same worker from the
/// same fixed identity, security, and resource dependencies. Naming them keeps
/// the two `Arc<dyn ...>` audit/verifier pairs from being transposable at a
/// call site.
pub struct OraclePeerWorkerConfig {
    /// Identity of the node this worker answers for.
    pub worker_node_id: NodeId,
    /// Role fence every accepted fragment must match.
    pub oracle_fence: FencingToken,
    /// Verifier for inbound peer tickets.
    pub verifier: Arc<dyn PeerTicketVerifier>,
    /// Sink for peer security audit records.
    pub security_audit: Arc<dyn PeerSecurityAudit>,
    /// Shared registry backing local fragment reservations.
    pub reservations: Arc<ReservationRegistry>,
    /// Root-issued Oracle capability bounding follower execution.
    pub oracle_resources: crate::resources::OracleResources,
    /// Resolver that binds assigned sources to concrete providers.
    pub resolver: Arc<dyn FollowerSourceResolver>,
    /// Sink for Oracle query audit records.
    pub audit: Arc<dyn super::OracleAudit>,
}

impl OraclePeerWorker {
    /// Creates the production worker with native physical-plan execution enabled.
    #[must_use]
    pub fn new_physical_with_resources(config: OraclePeerWorkerConfig) -> Self {
        let OraclePeerWorkerConfig {
            worker_node_id,
            oracle_fence,
            verifier,
            security_audit,
            reservations,
            oracle_resources,
            resolver,
            audit,
        } = config;
        let follower = PhysicalPlanFollower::new(resolver).with_audit(audit);
        Self {
            worker_node_id,
            oracle_fence,
            verifier,
            security_audit,
            reservations,
            oracle_resources,
            physical_follower: Arc::new(follower),
            physical_observer: Arc::new(PhysicalWorkerObserver::default()),
        }
    }

    /// Installs the one process reader authority on this worker's follower.
    ///
    /// The worker is constructed before the Oracle engine that owns the
    /// authority, so boot fills it here after `OracleEngine::new` and before
    /// startup, cluster activation, snapshot publication, or readiness. A
    /// snapshot-bearing assignment fails closed until this succeeds.
    ///
    /// # Errors
    ///
    /// Returns [`PhysicalPlanFollowerError::AuthorityAlreadyInstalled`] when an
    /// authority was already installed, so a repeated or late installation
    /// fails boot instead of permitting source IO under an unexpected epoch.
    pub fn install_reader_authority(
        &self,
        authority: Arc<crate::oracle::reader_pins::OracleReaderAuthority>,
    ) -> Result<(), PhysicalPlanFollowerError> {
        self.physical_follower.install_reader_authority(authority)
    }

    /// Returns the exact installed epoch plus preflight and resolver-entry counts.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn authority_inspection_for_test(
        &self,
    ) -> (Option<Arc<OracleReaderAuthority>>, usize, usize) {
        self.physical_follower.authority_inspection_for_test()
    }

    /// Reconstructs boot's uninstalled worker while retaining its real dependencies.
    ///
    /// The new follower has an empty authority cell and fresh effect counters;
    /// reservations, security, resources, and the catalog resolver remain shared.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn without_reader_authority_for_test(&self) -> Self {
        Self {
            worker_node_id: self.worker_node_id,
            oracle_fence: self.oracle_fence,
            verifier: Arc::clone(&self.verifier),
            security_audit: Arc::clone(&self.security_audit),
            reservations: Arc::clone(&self.reservations),
            oracle_resources: self.oracle_resources.clone(),
            physical_follower: Arc::new(self.physical_follower.without_reader_authority_for_test()),
            physical_observer: Arc::new(PhysicalWorkerObserver::default()),
        }
    }

    /// Captures exact production follower and footer activity for journeys.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn physical_inspection(&self) -> PhysicalWorkerInspection {
        PhysicalWorkerInspection {
            node_id: self.worker_node_id,
            oracle_executions: self
                .physical_observer
                .oracle_executions
                .load(Ordering::Acquire),
            footers_emitted: self
                .physical_observer
                .footers_emitted
                .load(Ordering::Acquire),
            rows_encoded: self.physical_observer.rows_encoded.load(Ordering::Acquire),
        }
    }

    /// Reserves bounded running capacity for one fenced leader.
    ///
    /// Reservation is the single admission gate: accepting here grants the
    /// slot units the fragment will later execute under, so a leader that
    /// completes its fan-out reservation knows every participant can run.
    /// A saturated pool is waited out for at most [`PEER_SLOT_WAIT`] before
    /// refusing, which converts a momentary instant of contention into a
    /// slightly delayed fragment instead of a failed query, while still
    /// refusing sustained overload promptly enough that the leader can retry
    /// well inside the query deadline.
    pub async fn reserve(&self, request: &ReserveNodeSlotsRequest) -> ReserveNodeSlotsResponse {
        let query_class = request.query_class;
        let rejected = || {
            record_slot(query_class, SlotOutcome::Rejected);
            ReserveNodeSlotsResponse::Rejected(ReservationRejected {
                retry_after_ms: RESERVATION_RETRY_MS,
            })
        };
        // The waiter bound is taken before the first attempt and held for the
        // whole wait, so a saturated node sheds new arrivals immediately instead
        // of accumulating an unbounded set of sleepers behind one running gate.
        let Ok(_waiter) = self.reservations.slots().try_pending() else {
            return rejected();
        };
        let deadline = std::time::Instant::now() + PEER_SLOT_WAIT;
        loop {
            // Charge the governor here — memory and slot units together — so a
            // node already saturated by its own leader-side queries refuses
            // before the leader commits to this participant rather than after.
            let attempt = match self.acquire_reserved_capacity(request) {
                Ok(capacity) => self
                    .reservations
                    .reserve(request, Utc::now(), Some(capacity)),
                Err(error) => {
                    tracing::warn!(
                        stage = "slot_reservation",
                        query_class = ?request.query_class,
                        "oracle peer capacity rejection"
                    );
                    Err(error)
                }
            };
            match attempt {
                Ok(pending) => {
                    record_slot(query_class, SlotOutcome::Pending);
                    return ReserveNodeSlotsResponse::Pending(pending);
                }
                Err(DispatchError::Capacity) if std::time::Instant::now() < deadline => {
                    tokio::time::sleep(PEER_SLOT_POLL).await;
                }
                Err(_) => return rejected(),
            }
        }
    }

    /// Releases one matching reservation idempotently.
    pub fn release(&self, request: &ReleaseNodeSlotsRequest) {
        let _released = self.reservations.release(request, Utc::now());
    }

    /// Returns live pending reservations for integration-only capacity assertions.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn pending_reservations(&self) -> usize {
        self.reservations.cleanup_expired(Utc::now())
    }

    /// Returns cumulative successful running admissions for integration assertions.
    ///
    /// Counts only fragments this peer admitted to run via `take_for_execute`;
    /// a non-zero value proves at least one fragment executed on this non-leader
    /// peer rather than falling back to the leader.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn admitted_running_total(&self) -> u64 {
        self.reservations.admitted_running_total()
    }

    /// Verifies and executes one ticket-bound fragment.
    ///
    /// Signature, configured key ID, audience, worker fence, and replay are
    /// validated over raw claims bytes before claims or fragment decoding.
    /// Reservation ownership is then converted before fragment decoding and IO.
    ///
    /// # Errors
    /// Returns terminal security/contract failures or retryable capacity/storage failures.
    pub async fn execute(
        &self,
        request: ExecuteFragmentRequest,
    ) -> Result<WorkerExecution, DispatchError> {
        self.execute_with_capacity(request, WorkerCapacity::ReserveRunning, None)
            .await
    }

    /// Verifies and executes a leader-local fragment under its admitted query slot.
    ///
    /// The in-process dispatcher retains the admitted query guard while this
    /// operation runs, so this path validates and consumes the pending
    /// reservation without charging duplicate local slot units.
    ///
    /// # Errors
    /// Returns terminal security/contract failures or retryable storage failures.
    async fn execute_local(
        &self,
        request: ExecuteFragmentRequest,
        admitted_grant: LeaderAdmittedGrant,
    ) -> Result<WorkerExecution, DispatchError> {
        self.execute_with_capacity(
            request,
            WorkerCapacity::LeaderAdmitted,
            Some(admitted_grant),
        )
        .await
    }

    /// Executes the shared verification and fragment workflow with explicit capacity ownership.
    ///
    /// # Errors
    /// Returns terminal security/contract failures or retryable capacity/storage failures.
    /// Runs the last pre-execution refusals for an already-authenticated fragment.
    ///
    /// Both checks happen after the ticket verified and before
    /// `follower.execute` resolves a provider or issues any object I/O:
    /// the physical claims must still describe this exact request, and the
    /// assignment-authority digest recomputed over the assignments this
    /// follower physically received must equal the digest signed into the
    /// verified claims. A valid signature only proves the claims bytes were
    /// not altered in transit, not that the dispatched closure matches what
    /// the leader signed, so the digest is recomputed here rather than
    /// trusted.
    ///
    /// # Errors
    ///
    /// Returns [`DispatchError::Terminal`] when either check refuses, after
    /// appending the matching verified security violation. Propagates the
    /// audit append failure unchanged when the chain itself cannot record the
    /// refusal, so the worker fails closed rather than serving unattributably.
    async fn admit_verified_fragment(
        &self,
        request: &ExecuteFragmentRequest,
        claims: &PeerTicketClaims,
        tenant_id: DataTenantId,
        running: &RunningReservation,
    ) -> Result<(), DispatchError> {
        if let Err(violation) = validate_physical_claims(claims, request) {
            tracing::error!(?violation, "Oracle peer physical claims validation failed");
            self.audit_verified(tenant_id, violation).await?;
            return Err(DispatchError::Terminal);
        }
        // The authenticated class decided this fragment's quantum at reservation
        // time; emitting it here is what lets an operator tie a slow fragment
        // back to the admission decision that sized its memory pool.
        tracing::debug!(
            query_class = ?running.query_class,
            reservation = %request.reservation_id.as_uuid(),
            "Oracle peer fragment admitted to execute"
        );
        match super::peer::assignment_authority_digest_for(&request.assignments) {
            Ok(recomputed) if recomputed == claims.assignment_authority_digest => {}
            _ => {
                tracing::error!("Oracle peer assignment-authority digest mismatch");
                self.audit_verified(
                    tenant_id,
                    BifrostSecurityViolationKind::PeerAssignmentAuthority,
                )
                .await?;
                return Err(DispatchError::Terminal);
            }
        }
        Ok(())
    }

    /// Selects the follower session grant for one fragment's admitted capacity.
    ///
    /// Both capacities shape the session from a grant this process admitted:
    /// the leader's own envelope in-process, or the worker quantum this node
    /// charged for the remote fragment. Neither reads a caller-supplied hint.
    ///
    /// # Errors
    ///
    /// Returns [`DispatchError::Capacity`] when the capacity this fragment
    /// claims has no retained grant to shape its session from.
    fn admitted_sessions(
        capacity: WorkerCapacity,
        admitted_grant: Option<LeaderAdmittedGrant>,
        worker_resources: Option<&FollowerWorkerResources>,
    ) -> Result<FollowerSessionFactory, DispatchError> {
        Ok(match capacity {
            WorkerCapacity::LeaderAdmitted => {
                let grant = admitted_grant.ok_or(DispatchError::Capacity)?;
                FollowerSessionFactory::for_grant(
                    grant.memory_pool,
                    grant.granted_memory_bytes,
                    grant.admitted_target_partitions,
                )
            }
            WorkerCapacity::ReserveRunning => {
                let resources = worker_resources.ok_or(DispatchError::Capacity)?;
                FollowerSessionFactory::for_grant(
                    resources.memory_pool(),
                    resources.granted_memory_bytes(),
                    resources.admitted_target_partitions(),
                )
            }
        })
    }

    /// Accepts signed authority and transfers reservation ownership into the timed stream.
    ///
    /// Dropping the future or returned stream releases its capacity and reader protection.
    ///
    /// # Errors
    ///
    /// Refuses invalid or elapsed authority, unavailable capacity, and follower failures.
    async fn execute_with_capacity(
        &self,
        request: ExecuteFragmentRequest,
        capacity: WorkerCapacity,
        admitted_grant: Option<LeaderAdmittedGrant>,
    ) -> Result<WorkerExecution, DispatchError> {
        let (claims, tenant_id) = self.verify_fragment_authority(&request).await?;
        let monotonic_now = Instant::now();
        let execution_deadline = claims
            .execution_deadline()
            .map_err(|_| DispatchError::Terminal)?;
        let remaining = (execution_deadline - Utc::now())
            .to_std()
            .map_err(|_| DispatchError::Terminal)?;
        if remaining.is_zero() {
            return Err(DispatchError::Terminal);
        }
        let follower_deadline = monotonic_now
            .checked_add(remaining)
            .ok_or(DispatchError::Terminal)?;
        let mut running = self
            .claim_running_reservation(&request, capacity, &claims, tenant_id)
            .await?;
        // The lease was charged at reservation; take it here so the attempt
        // stream, not the reservation entry, owns it for the rest of execution.
        let worker_resources = running.worker_resources.take();
        self.admit_verified_fragment(&request, &claims, tenant_id, &running)
            .await?;
        let follower = &self.physical_follower;
        let sessions =
            Self::admitted_sessions(capacity, admitted_grant, worker_resources.as_ref())?;
        let binding = request
            .assignments
            .first()
            .map(|assignment| assignment.binding.clone())
            .ok_or(DispatchError::Terminal)?;
        let stream = follower
            .execute(
                &request,
                AuthenticatedFollowerContext {
                    tenant_id,
                    table_binding: &binding,
                    reservation_id: &request.reservation_id,
                    leader_fence: request.leader_fence.clone(),
                    local_fence: request.target_fence.clone(),
                },
                &sessions,
            )
            .await;
        let stream = match stream {
            Ok(stream) => stream,
            Err(error) => {
                return Err(match error {
                    // A plan whose bytes do not match the signed digest, or
                    // whose fences, bindings, or assignments contradict the
                    // verified ticket, is a contract refusal of an
                    // authenticated peer's request. It is detected here before
                    // any object I/O, and it must be attributable afterwards,
                    // so it joins the tenant's security chain rather than only
                    // the local trace.
                    PhysicalPlanFollowerError::Preflight(_) => {
                        tracing::error!(?error, "Oracle physical follower rejected the request");
                        self.audit_verified(tenant_id, BifrostSecurityViolationKind::PeerFragment)
                            .await?;
                        DispatchError::Terminal
                    }
                    // Boot installs the reader authority before this worker
                    // can serve, so reaching this arm at execution time means
                    // the process is misconfigured rather than the peer.
                    PhysicalPlanFollowerError::PostResolutionDecode(_)
                    | PhysicalPlanFollowerError::AuthorityAlreadyInstalled => {
                        tracing::error!(?error, "Oracle physical follower rejected the request");
                        DispatchError::Terminal
                    }
                    PhysicalPlanFollowerError::Resolution(_) => {
                        tracing::warn!(
                            ?error,
                            "Oracle physical follower could not resolve a pinned source"
                        );
                        DispatchError::EligibleSourceLoss {
                            cause: EligibleSourceLossCause::ProviderResolution,
                        }
                    }
                    PhysicalPlanFollowerError::Execution(_) => {
                        tracing::warn!(
                            ?error,
                            "Oracle physical follower execution could not start"
                        );
                        DispatchError::Unavailable
                    }
                });
            }
        };
        self.physical_observer
            .oracle_executions
            .fetch_add(1, Ordering::AcqRel);
        let (stream, scan_evidence, reader_protection) = stream.split();
        let output = encode_attempt_frames(
            stream,
            scan_evidence,
            reader_protection,
            running,
            follower_deadline,
            request.plan_fingerprint.clone(),
            Arc::clone(&self.physical_observer),
        );
        Ok(WorkerExecution {
            stream: retain_worker_resources(output, worker_resources),
        })
    }

    /// Converts this fragment's pending reservation into a running one.
    ///
    /// The transition is tuple-bound: the reservation must already be held for
    /// exactly this query, leader, and leader fence. A remote worker consumes a
    /// slot units; the leader-local path reuses the query's own admitted
    /// slot instead of taking a duplicate. A tuple mismatch is a fence violation
    /// rather than an ordinary refusal, so it is audited before it is returned.
    ///
    /// # Errors
    ///
    /// Returns [`DispatchError::Terminal`] when a claim identifier is malformed
    /// or the reservation is missing, expired, or bound to different ownership,
    /// and propagates any other transition failure unchanged. Audit-append
    /// failures propagate unchanged.
    async fn claim_running_reservation(
        &self,
        request: &ExecuteFragmentRequest,
        capacity: WorkerCapacity,
        claims: &PeerTicketClaims,
        tenant_id: DataTenantId,
    ) -> Result<RunningReservation, DispatchError> {
        let query_id = QueryId::new(uuid_from(&claims.query_id)?);
        let leader = NodeId::new(uuid_from(&claims.leader_node_id)?);
        let transition = match capacity {
            WorkerCapacity::ReserveRunning => self.reservations.take_for_execute(
                request.reservation_id,
                query_id,
                leader,
                claims.leader_fence,
                Utc::now(),
            ),
            WorkerCapacity::LeaderAdmitted => self.reservations.take_for_local_leader_execute(
                request.reservation_id,
                query_id,
                leader,
                claims.leader_fence,
                Utc::now(),
            ),
        };
        match transition {
            Ok(running) => Ok(running),
            Err(DispatchError::Terminal) => {
                tracing::error!(
                    reservation = %request.reservation_id.as_uuid(),
                    "Oracle peer reservation transition was not tuple-bound"
                );
                self.audit_verified(tenant_id, BifrostSecurityViolationKind::PeerFence)
                    .await?;
                Err(DispatchError::Terminal)
            }
            Err(error) => Err(error),
        }
    }

    /// Proves one inbound fragment is addressed to this worker by an authorized leader.
    ///
    /// Runs the complete security preamble in fixed order: target role, target
    /// fence identity, peer-ticket signature, claim decoding, and claim identity
    /// validation. Every refusal is audited before it is returned, so no
    /// follower work, reservation transition, or resource acquisition can be
    /// reached by an unauthenticated or misaddressed request.
    ///
    /// # Errors
    /// Returns [`DispatchError::Terminal`] for a wrong role, a fence mismatch,
    /// an unverifiable ticket, undecodable claims, or claim identifiers that
    /// fail validation. Audit-append failures propagate unchanged.
    async fn verify_fragment_authority(
        &self,
        request: &ExecuteFragmentRequest,
    ) -> Result<(PeerTicketClaims, DataTenantId), DispatchError> {
        if request.target_fence.role != wyrd_spec::vala::api::ClusterRole::Oracle {
            tracing::error!(role = ?request.target_fence.role, "Oracle peer received a non-Oracle target role");
            return Err(DispatchError::Terminal);
        }
        let expected_fence = self.oracle_fence;
        if request.target_fence.node_id != self.worker_node_id
            || request.target_fence.fencing_token != expected_fence
        {
            tracing::error!(
                target = %request.target_fence.node_id.as_uuid(),
                worker = %self.worker_node_id.as_uuid(),
                requested_fence = request.target_fence.fencing_token,
                expected_fence,
                "Oracle peer target fence mismatch"
            );
            // Refused before ticket verification, so no tenant is established
            // and this joins the unverified rejection chain. Recording it is
            // what makes a leader replaying a superseded fence visible to an
            // operator instead of only to this pod's trace.
            self.audit_unverified(BifrostSecurityViolationKind::PeerFence)
                .await?;
            return Err(DispatchError::Terminal);
        }
        let verified = self
            .verifier
            .verify_peer_ticket(
                &request.ticket,
                self.worker_node_id,
                expected_fence,
                Utc::now(),
            )
            .await
            .map_err(|error| {
                tracing::error!(?error, "Oracle peer ticket verification failed");
                record_security(SecurityEventClass::Ticket);
                DispatchError::Terminal
            })?;
        let Ok(claims) = PeerTicketClaims::decode(verified.0.as_slice()) else {
            self.audit_unverified(BifrostSecurityViolationKind::PeerSignature)
                .await?;
            return Err(DispatchError::Terminal);
        };
        let tenant_validation = validated_claim_identifiers(&claims);
        if let Err(violation) = tenant_validation.as_ref() {
            self.audit_unverified(*violation).await?;
        }
        let tenant_id = tenant_validation.map_err(|_| DispatchError::Terminal)?;
        Ok((claims, tenant_id))
    }

    /// Acquires the one remote-worker memory root backing a peer reservation.
    ///
    /// Called from the reservation path only. The leader-local path never
    /// reaches here because it reuses the admitted query's own envelope and pool
    /// rather than taking a second, duplicate worker quantum.
    ///
    /// # Errors
    ///
    /// Returns [`DispatchError::Capacity`] when the configured Oracle resources
    /// cannot admit the remote worker quantum.
    fn acquire_reserved_capacity(
        &self,
        request: &ReserveNodeSlotsRequest,
    ) -> Result<ReservedCapacity, DispatchError> {
        if request.graph.is_some() {
            // A graph runs a whole distributed plan on this node — several
            // stages, their exchanges, and their spill — so it charges a query
            // envelope, not the single-fragment quantum below. The local ratio
            // is zero because none of the leader's own scan work runs here.
            return self
                .oracle_resources
                .try_acquire_query(crate::resources::OracleResourceRequest::for_class(
                    request.query_class,
                    0.0,
                ))
                .map(|envelope| ReservedCapacity::Graph(Box::new(envelope)))
                .map_err(|_| DispatchError::Capacity);
        }
        let class = match request.query_class {
            QueryClass::Interactive => crate::resources::OracleWorkerClass::Interactive,
            QueryClass::Analytical => crate::resources::OracleWorkerClass::Analytical,
        };
        self.oracle_resources
            .try_acquire_worker(class)
            .map(FollowerWorkerResources::Oracle)
            .map(ReservedCapacity::Fragment)
            .map_err(|_| DispatchError::Capacity)
    }

    /// Commits a system-chain audit before rejecting claims that lack a trusted tenant.
    ///
    /// # Errors
    /// Returns terminal rejection whether the security audit succeeds or fails.
    async fn audit_unverified(
        &self,
        violation: BifrostSecurityViolationKind,
    ) -> Result<(), DispatchError> {
        record_security(match violation {
            BifrostSecurityViolationKind::PeerSignature
            | BifrostSecurityViolationKind::PeerUnknownKey => SecurityEventClass::Ticket,
            _ => SecurityEventClass::Claims,
        });
        self.security_audit
            .append_unverified_ticket_rejection(violation)
            .await
            .map_err(|_| DispatchError::Terminal)
    }

    /// Commits a verified-tenant audit before rejecting fragment-bound claims.
    ///
    /// # Errors
    /// Returns terminal rejection whether the security audit succeeds or fails.
    async fn audit_verified(
        &self,
        tenant_id: DataTenantId,
        violation: BifrostSecurityViolationKind,
    ) -> Result<(), DispatchError> {
        record_security(SecurityEventClass::Fragment);
        self.security_audit
            .append_verified_ticket_violation(tenant_id, violation)
            .await
            .map_err(|_| DispatchError::Terminal)
    }
}

/// Builds the signed peer-ticket claims delivered to one dispatch candidate.
///
/// The ticket expires at the earlier of the candidate's pending reservation and
/// the query's own absolute deadline, so a peer can never hold work past either
/// bound. The fragment digest, manifest digest, and projection digest all carry
/// the same plan fingerprint: the follower validates one sealed fragment, and
/// splitting these into distinct digests would imply a per-stage binding the
/// protocol does not have.
///
/// The assignment-authority digest is computed here, over the exact assignments
/// this fragment will dispatch, so protocol v2 followers can recompute it from
/// what they physically received and refuse a closure that was altered after
/// the leader signed it.
///
/// # Errors
///
/// Returns [`DispatchError::Terminal`] when the fragment's assignments cannot
/// produce a canonical authority digest; such a fragment must never be signed.
fn peer_ticket_claims(
    candidate: &DispatchCandidate,
    context: &DispatchContext,
    fragment: &PhysicalDispatchFragment,
    pending: &PendingNodeReservation,
) -> Result<PeerTicketClaims, DispatchError> {
    Ok(PeerTicketClaims {
        protocol_version: PEER_PROTOCOL_VERSION,
        audience: candidate.node_id.as_uuid().as_bytes().to_vec(),
        worker_fence: candidate.worker_fence,
        leader_node_id: context.leader_node_id.as_uuid().as_bytes().to_vec(),
        leader_fence: context.leader_fence,
        query_id: context.query_id.as_uuid().as_bytes().to_vec(),
        tenant_id: context.tenant_id.as_bytes().to_vec(),
        nonce: uuid::Uuid::new_v4().as_bytes().to_vec(),
        expires_at_ms: pending
            .expires_at
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

/// Encodes one follower record-batch stream into the peer attempt frame protocol.
///
/// Emits the schema frame first, then one encoded frame per batch, then the
/// physical footer. The returned stream owns `running` for its whole life, so
/// the reservation is released exactly when the last frame has been produced
/// or the consumer drops the stream.
///
/// `scan_evidence` is finalized after the last batch and carried on the footer
/// so the leader can report physical read volume for a plan whose own leaves
/// are all remote.
///
/// The absolute `follower_deadline` is enforced between batches rather than
/// around the whole stream: a deadline reached after frames have already been
/// delivered yields a [`DispatchPartialReason::Timeout`] partial so the leader
/// keeps the batches it received, while a decode or footer failure yields a
/// terminal because those indicate a contract violation rather than a bound.
fn encode_attempt_frames(
    stream: datafusion::execution::SendableRecordBatchStream,
    scan_evidence: super::follower::FollowerScanEvidence,
    reader_protection: Option<super::follower::FollowerReaderProtection>,
    running: RunningReservation,
    follower_deadline: tokio::time::Instant,
    plan_fingerprint: String,
    physical_observer: Arc<PhysicalWorkerObserver>,
) -> WorkerAttemptStream {
    Box::pin(async_stream::stream! {
        let _running = running;
        // Retained for the whole attempt, footer and error paths included: the
        // fragment's snapshots stay protected until this stream is finished or
        // dropped, never merely until its plan was built.
        let _reader_protection = reader_protection;
        let mut stream = stream;
        let mut encoder = AttemptEncoder::default();
        match encoder.start(stream.schema()) {
            Ok(schema) => yield Ok(schema),
            Err(_) => {
                yield Err(DispatchError::Terminal);
                return;
            }
        }
        while let Some(batch) = tokio::select! {
            () = tokio::time::sleep_until(follower_deadline) => {
                yield Err(DispatchError::Partial {
                    attempt: None,
                    reason: DispatchPartialReason::Timeout,
                });
                return;
            }
            batch = stream.next() => batch,
        } {
            let batch = match batch {
                Ok(batch) => batch,
                Err(error) => {
                    tracing::warn!(?error, "Oracle follower execution stream failed");
                    // A foreign-tenant row refused by the physical tripwire is a
                    // property of the scanned data, not of this worker, so it is
                    // reported as a tenant-isolation outcome that the leader will
                    // not retry on another candidate.
                    if super::exec::is_tenant_invariant_error(&error) {
                        yield Err(DispatchError::TenantInvariant);
                    } else {
                        yield Err(DispatchError::Unavailable);
                    }
                    return;
                }
            };
            physical_observer
                .rows_encoded
                .fetch_add(
                    u64::try_from(batch.num_rows()).unwrap_or(u64::MAX),
                    Ordering::AcqRel,
                );
            match encoder.encode(&batch) {
                Ok((schema, batch)) => {
                    if let Some(schema) = schema {
                        yield Ok(schema);
                    }
                    yield Ok(batch);
                }
                Err(error) => {
                    tracing::warn!(%error, "Oracle follower could not encode a fragment batch");
                    yield Err(DispatchError::Terminal);
                    return;
                }
            }
        }
        // Finalize only here: the scan counters are written during execution,
        // so reading them before the stream is exhausted under-reports the scan.
        let footer = encoder
            .finish_physical(&plan_fingerprint, scan_evidence.finalize())
            .map_err(|error| {
                tracing::warn!(%error, "Oracle follower could not finalize a fragment footer");
                DispatchError::Terminal
            });
        if footer.is_ok() {
            physical_observer.footers_emitted.fetch_add(1, Ordering::AcqRel);
        }
        yield footer;
    })
}

/// Retains one coarse root quantum until the remote attempt stream terminates.
fn retain_worker_resources(
    mut stream: WorkerAttemptStream,
    resources: Option<FollowerWorkerResources>,
) -> WorkerAttemptStream {
    Box::pin(async_stream::stream! {
        let _resources = resources;
        while let Some(frame) = stream.next().await {
            yield frame;
        }
    })
}

#[cfg(test)]
mod resource_tests {
    use super::*;
    use crate::resources::{
        BifrostResourcePolicy, BifrostRole, BifrostRuntimeResources,
        ORACLE_PARTITION_WORKING_MEMORY_BYTES, ResourceSource, SystemResourceSnapshot,
    };
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    /// Production stream ownership retains and releases the advertised root quantum.
    ///
    /// # Panics
    ///
    /// Panics when the deterministic resource topology or assertions fail.
    #[test]
    fn remote_worker_stream_retains_root_quantum_until_terminal_drop() {
        let roles = BifrostRuntimeResources::from_snapshot(
            SystemResourceSnapshot {
                memory_limit_bytes: 576 * 1024 * 1024,
                effective_cpu: 1,
                scratch_capacity_bytes: 1024 * 1024 * 1024,
                scratch_available_bytes: 1024 * 1024 * 1024,
                memory_source: ResourceSource::Injected,
                cpu_source: ResourceSource::Injected,
            },
            BifrostResourcePolicy {
                roles: BTreeSet::from([BifrostRole::Oracle, BifrostRole::Forge]),
                memory_limit_bytes: None,
                unmanaged_reserve_bytes: None,
                scratch_limit_bytes: None,
                effective_cpu: None,
                oracle_query_slot_limit: None,
                // The unclamped production default cannot sit beside the
                // Oracle floor; this topology names one rewrite working set,
                // exactly as its co-located deployment configures one.
                forge_compaction_memory_limit_bytes: Some(
                    crate::resources::FORGE_TEST_BUDGET_BYTES,
                ),
                scratch_root: PathBuf::new(),
                volume_roots: None,
            },
        )
        .expect("exact Oracle/Forge topology")
        .compose_roles()
        .expect("role composition");
        let oracle = roles.oracle().expect("Oracle capability");
        let resources = oracle
            .try_acquire_worker(crate::resources::OracleWorkerClass::Interactive)
            .expect("advertised worker quantum");
        assert_eq!(
            resources.memory_bytes(),
            ORACLE_PARTITION_WORKING_MEMORY_BYTES,
            "a worker charges the slot-unit admission quantum, not the grant cap"
        );
        let stream: WorkerAttemptStream = Box::pin(futures_util::stream::pending());
        let retained =
            retain_worker_resources(stream, Some(FollowerWorkerResources::Oracle(resources)));
        assert_eq!(
            oracle
                .snapshot()
                .expect("retained snapshot")
                .oracle_memory_used_bytes,
            ORACLE_PARTITION_WORKING_MEMORY_BYTES
        );
        // Admission charges one slot-unit quantum rather than a whole grant cap,
        // so the budget holds several workers. Drain it to prove the retained
        // stream's quantum is genuinely held rather than merely accounted.
        let mut drained = Vec::new();
        while let Ok(worker) =
            oracle.try_acquire_worker(crate::resources::OracleWorkerClass::Interactive)
        {
            drained.push(worker);
        }
        assert!(
            oracle
                .try_acquire_worker(crate::resources::OracleWorkerClass::Interactive)
                .is_err()
        );
        drop(drained);
        drop(retained);
        assert_eq!(
            oracle
                .snapshot()
                .expect("released snapshot")
                .oracle_memory_used_bytes,
            0
        );
        oracle
            .try_acquire_worker(crate::resources::OracleWorkerClass::Interactive)
            .expect("capacity returns after terminal stream drop");
    }
}

/// Source of the running capacity retained while one worker attempt streams.
#[derive(Clone, Copy)]
enum WorkerCapacity {
    /// A remote worker charges its own local slot units.
    ReserveRunning,
    /// The in-process leader reuses the units retained by query admission.
    LeaderAdmitted,
}

/// Validates fixed-width and non-empty claims before constructing typed IDs.
///
/// # Errors
/// Returns terminal rejection for malformed verified claims.
fn validated_claim_identifiers(
    claims: &PeerTicketClaims,
) -> Result<DataTenantId, BifrostSecurityViolationKind> {
    if claims.protocol_version != PEER_PROTOCOL_VERSION
        || claims.query_id.len() != 16
        || claims.leader_node_id.len() != 16
        || claims.permission_digest.is_empty()
        || claims.execution_deadline().is_err()
    {
        return Err(BifrostSecurityViolationKind::PeerFragment);
    }
    let tenant_uuid = uuid::Uuid::from_slice(&claims.tenant_id)
        .map_err(|_| BifrostSecurityViolationKind::PeerTenant)?;
    DataTenantId::new(tenant_uuid).map_err(|_| BifrostSecurityViolationKind::PeerTenant)
}

/// Parses one exact UUID claim.
///
/// # Errors
/// Returns terminal rejection for malformed UUID bytes.
fn uuid_from(bytes: &[u8]) -> Result<uuid::Uuid, DispatchError> {
    uuid::Uuid::from_slice(bytes).map_err(|_| DispatchError::Terminal)
}

/// Matches every native physical-plan claim before provider resolution.
///
/// # Errors
/// Returns terminal rejection for any tenant binding, plan digest, or fence mismatch.
fn validate_physical_claims(
    claims: &PeerTicketClaims,
    request: &ExecuteFragmentRequest,
) -> Result<(), BifrostSecurityViolationKind> {
    if request.leader_fence.node_id.as_uuid().as_bytes() != claims.leader_node_id.as_slice()
        || request.leader_fence.fencing_token != claims.leader_fence
        || request.target_fence.fencing_token != claims.worker_fence
    {
        return Err(BifrostSecurityViolationKind::PeerFence);
    }
    if request.assignments.is_empty()
        || request.assignments.iter().any(|assignment| {
            format!(
                "{}.{}",
                assignment.binding.namespace, assignment.binding.table
            ) != claims.binding
        })
    {
        return Err(BifrostSecurityViolationKind::PeerTenant);
    }
    if claims.fragment_digest != request.plan_fingerprint
        || claims.manifest_digest != request.plan_fingerprint
    {
        return Err(BifrostSecurityViolationKind::PeerFragment);
    }
    Ok(())
}

/// One adapter contract used identically by local and tonic dispatch.
#[async_trait]
pub trait OraclePeerTransport: Send + Sync {
    /// Reserves pending slots on one worker.
    ///
    /// # Errors
    /// Returns retryable availability or terminal contract failure.
    async fn reserve(
        &self,
        worker: NodeId,
        request: ReserveNodeSlotsRequest,
    ) -> Result<ReserveNodeSlotsResponse, DispatchError>;

    /// Releases one matching pending reservation idempotently.
    ///
    /// # Errors
    /// Returns retryable availability or terminal contract failure.
    async fn release(
        &self,
        worker: NodeId,
        request: ReleaseNodeSlotsRequest,
    ) -> Result<(), DispatchError>;

    /// Executes one ticket-bound fragment and returns footer-terminated frames.
    ///
    /// # Errors
    /// Returns retryable availability or terminal security/contract failure.
    async fn execute(
        &self,
        worker: NodeId,
        request: ExecuteFragmentRequest,
        admitted_grant: Option<LeaderAdmittedGrant>,
    ) -> Result<WorkerAttemptStream, DispatchError>;
}

/// Zero-serialization adapter used when the selected worker is local.
pub struct LocalOraclePeerTransport {
    /// Shared worker used by the leader-local execution path.
    worker: Arc<OraclePeerWorker>,
}

impl LocalOraclePeerTransport {
    /// Creates the local adapter around the same worker owner used by tonic.
    #[must_use]
    pub fn new(worker: Arc<OraclePeerWorker>) -> Self {
        Self { worker }
    }
}

#[async_trait]
impl OraclePeerTransport for LocalOraclePeerTransport {
    /// Reserves tuple-bound local work under the leader's admitted query capacity.
    ///
    /// # Errors
    /// This adapter returns the worker's typed reservation outcome.
    async fn reserve(
        &self,
        _worker: NodeId,
        request: ReserveNodeSlotsRequest,
    ) -> Result<ReserveNodeSlotsResponse, DispatchError> {
        Ok(
            if let Ok(pending) = self.worker.reservations.reserve_local(&request, Utc::now()) {
                record_slot(request.query_class, SlotOutcome::Pending);
                ReserveNodeSlotsResponse::Pending(pending)
            } else {
                record_slot(request.query_class, SlotOutcome::Rejected);
                ReserveNodeSlotsResponse::Rejected(ReservationRejected {
                    retry_after_ms: RESERVATION_RETRY_MS,
                })
            },
        )
    }

    /// Releases through the same tuple check used by the tonic path.
    ///
    /// # Errors
    /// This in-process adapter never fails.
    async fn release(
        &self,
        _worker: NodeId,
        request: ReleaseNodeSlotsRequest,
    ) -> Result<(), DispatchError> {
        self.worker.release(&request);
        Ok(())
    }

    /// Executes through the shared worker without serializing or collecting frames.
    ///
    /// # Errors
    /// Returns the worker's retryable or terminal failure.
    async fn execute(
        &self,
        _worker: NodeId,
        request: ExecuteFragmentRequest,
        admitted_grant: Option<LeaderAdmittedGrant>,
    ) -> Result<WorkerAttemptStream, DispatchError> {
        let admitted_grant = admitted_grant.ok_or(DispatchError::Capacity)?;
        Ok(self
            .worker
            .execute_local(request, admitted_grant)
            .await?
            .stream)
    }
}

/// Real tonic client transport resolving Oracle peers from live membership.
/// Acceptance window for one reservation purpose ticket, in seconds.
///
/// Short by design: a reservation call is a single round trip on a local
/// network, so the window only has to cover it. Anything longer widens the
/// interval in which a captured ticket is still presentable.
const RESERVATION_TICKET_TTL_SECONDS: i64 = 10;

pub struct TonicOraclePeerTransport {
    /// Existing registry publishing immutable ready/live membership cuts.
    topology: OraclePeerTopology,
    /// Optional authenticated service credential attached to private calls.
    credentials: Arc<dyn OraclePeerCredentials>,
    /// Optional immutable CA and DNS identity; absent only for local development tests.
    tls: Option<BifrostPeerTls>,
    /// Server-owned signer stamping a purpose ticket onto reservation calls.
    ///
    /// Absent only where no reservation authority has been injected, in which
    /// case reserving and releasing capacity fail closed rather than travelling
    /// unauthorized.
    reservation_minter: Option<Arc<dyn ReservationTicketMinter>>,
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

/// Supplies short-lived authorization for private Oracle peer RPCs.
///
/// The server implementation owns durable credentials and refresh policy; the
/// Redux transport only requests a current bearer at the network boundary.
#[async_trait]
pub trait OraclePeerCredentials: Send + Sync {
    /// Returns a current access bearer, refreshing when `force_refresh` is true.
    ///
    /// # Errors
    /// Returns a terminal dispatch failure when credentials cannot be exchanged.
    async fn bearer(&self, force_refresh: bool) -> Result<String, DispatchError>;
}

/// Deterministic credential provider used by local and transport tests.
#[derive(Debug)]
pub struct StaticOraclePeerCredentials {
    /// Redacted bearer value retained only for deterministic private calls.
    bearer: secrecy::SecretString,
}

impl StaticOraclePeerCredentials {
    /// Creates deterministic credentials from one bearer.
    #[must_use]
    pub fn new(bearer: secrecy::SecretString) -> Self {
        Self { bearer }
    }
}

#[async_trait]
impl OraclePeerCredentials for StaticOraclePeerCredentials {
    async fn bearer(&self, _force_refresh: bool) -> Result<String, DispatchError> {
        use secrecy::ExposeSecret;
        Ok(self.bearer.expose_secret().to_owned())
    }
}

impl TonicOraclePeerTransport {
    /// Creates a development transport over the live cluster registry.
    ///
    /// # Errors
    /// Returns terminal rejection when the bearer value is invalid metadata.
    #[cfg(feature = "test-support")]
    pub fn new(
        addresses: HashMap<NodeId, String>,
        bearer: Option<&str>,
    ) -> Result<Self, DispatchError> {
        let bearer = bearer.unwrap_or_default();
        Ok(Self {
            topology: OraclePeerTopology::TestAddresses(addresses),
            credentials: Arc::new(StaticOraclePeerCredentials::new(
                secrecy::SecretString::from(bearer.to_owned()),
            )),
            tls: None,
            reservation_minter: None,
        })
    }

    /// Creates a production transport backed by a refreshing credential owner.
    #[must_use]
    pub fn with_credentials(
        registry: Arc<ClusterRegistry>,
        credentials: Arc<dyn OraclePeerCredentials>,
    ) -> Self {
        Self {
            topology: OraclePeerTopology::Registry(registry),
            credentials,
            tls: None,
            reservation_minter: None,
        }
    }

    /// Creates a production transport with CA-authenticated TLS.
    #[must_use]
    pub fn with_credentials_and_tls(
        registry: Arc<ClusterRegistry>,
        credentials: Arc<dyn OraclePeerCredentials>,
        tls: BifrostPeerTls,
    ) -> Self {
        Self {
            topology: OraclePeerTopology::Registry(registry),
            credentials,
            tls: Some(tls),
            reservation_minter: None,
        }
    }

    /// Attaches the server-owned signer for reservation purpose tickets.
    ///
    /// Kept a separate step because signing authority is owned by the server
    /// and routing is owned here: a transport composed without it can still
    /// dial peers, but every reservation call it makes fails closed.
    #[must_use]
    pub fn with_reservation_minter(mut self, minter: Arc<dyn ReservationTicketMinter>) -> Self {
        self.reservation_minter = Some(minter);
        self
    }

    /// Creates a TLS transport over an immutable endpoint fixture.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn with_test_credentials_and_tls(
        addresses: HashMap<NodeId, String>,
        credentials: Arc<dyn OraclePeerCredentials>,
        tls: BifrostPeerTls,
    ) -> Self {
        Self {
            topology: OraclePeerTopology::TestAddresses(addresses),
            credentials,
            tls: Some(tls),
            reservation_minter: None,
        }
    }

    /// Resolves an exact candidate from one current immutable membership cut.
    ///
    /// # Errors
    /// Returns stale-object when the node is absent or its current role fence
    /// differs, and terminal when a TLS transport is configured with plaintext.
    fn resolve_candidate(&self, candidate: &DispatchCandidate) -> Result<String, DispatchError> {
        #[cfg(feature = "test-support")]
        if let OraclePeerTopology::TestAddresses(addresses) = &self.topology {
            let address = addresses
                .get(&candidate.node_id)
                .ok_or(DispatchError::StaleObject)?;
            if self.tls.is_some() && !address.starts_with("https://") {
                return Err(DispatchError::Terminal);
            }
            return Ok(address.clone());
        }
        // Prefer the endpoint the immutable cut already authenticated. Trust
        // policy is still enforced here because it is a property of the address
        // itself and needs no membership lookup. A participant that has since
        // stopped serving surfaces as an ordinary connect failure, which is the
        // pre-`do_get` transport-failure path, not a membership verdict.
        if let Some(endpoint) = &candidate.endpoint {
            if self.tls.is_some() && !endpoint.starts_with("https://") {
                return Err(DispatchError::Terminal);
            }
            return Ok(endpoint.clone());
        }
        let snapshot = self.snapshot();
        match resolve_snapshot_candidate(&snapshot, candidate, self.tls.is_some(), Utc::now()) {
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

    /// Connects to the exact selected worker after live fence resolution.
    ///
    /// # Errors
    /// Returns retryable failure for absent, invalid, or unreachable endpoints.
    async fn client(
        &self,
        candidate: &DispatchCandidate,
    ) -> Result<OraclePeerServiceClient<Channel>, DispatchError> {
        let address = self.resolve_candidate(candidate)?;
        let endpoint = self
            .tls
            .as_ref()
            .ok_or(DispatchError::Unavailable)?
            .endpoint(address)
            .map_err(|_| DispatchError::Unavailable)?;
        let channel = endpoint
            .connect()
            .await
            .map_err(|_| DispatchError::Unavailable)?;
        Ok(OraclePeerServiceClient::new(channel))
    }

    /// Stamps a freshly minted reserve ticket onto one request copy.
    ///
    /// The digest is taken over the encoding with the ticket field cleared,
    /// which is exactly what the follower recomputes, so the signed value
    /// covers every routed identity in the request and nothing about the
    /// signature itself.
    ///
    /// # Errors
    ///
    /// Returns [`DispatchError::Terminal`] when no reservation authority is
    /// attached or the ticket cannot be minted; neither is retryable.
    fn ticketed_reserve(
        &self,
        wire: &wyrd_tonic::wyrd::v1::ReserveNodeSlotsRequest,
        binding: &ReservationBinding,
    ) -> Result<wyrd_tonic::wyrd::v1::ReserveNodeSlotsRequest, DispatchError> {
        let mut stamped = wire.clone();
        stamped.ticket = None;
        stamped.ticket = Some(self.reservation_ticket(&stamped, binding)?.into());
        Ok(stamped)
    }

    /// Stamps a freshly minted release ticket onto one request copy.
    ///
    /// # Errors
    ///
    /// Returns [`DispatchError::Terminal`] when no reservation authority is
    /// attached or the ticket cannot be minted.
    fn ticketed_release(
        &self,
        wire: &wyrd_tonic::wyrd::v1::ReleaseNodeSlotsRequest,
        binding: &ReservationBinding,
    ) -> Result<wyrd_tonic::wyrd::v1::ReleaseNodeSlotsRequest, DispatchError> {
        let mut stamped = wire.clone();
        stamped.ticket = None;
        stamped.ticket = Some(self.reservation_ticket(&stamped, binding)?.into());
        Ok(stamped)
    }

    /// Mints one single-use ticket over an already ticket-free encoding.
    ///
    /// # Errors
    ///
    /// Returns [`DispatchError::Terminal`] when no reservation authority is
    /// attached, the body exceeds its bound, or signing fails. A reservation
    /// that cannot be authorized is never sent unauthorized.
    fn reservation_ticket<T: wyrd_tonic::prost::Message>(
        &self,
        ticket_free: &T,
        binding: &ReservationBinding,
    ) -> Result<wyrd_spec::vala::api::SignedPeerTicket, DispatchError> {
        let minter = self
            .reservation_minter
            .as_ref()
            .ok_or(DispatchError::Terminal)?;
        let body_digest = reservation_body_digest(&ticket_free.encode_to_vec())
            .map_err(|_| DispatchError::Terminal)?;
        let expires_at_ms = (Utc::now() + ChronoDuration::seconds(RESERVATION_TICKET_TTL_SECONDS))
            .timestamp_millis();
        let claims = ReservationTicketClaims::for_binding(
            binding,
            body_digest,
            uuid::Uuid::new_v4().as_bytes().to_vec(),
            expires_at_ms,
        );
        minter
            .mint_reservation_ticket(binding.operation, &claims)
            .map_err(|_| DispatchError::Terminal)
    }

    /// Reserves capacity for one exact planned node/fence target.
    ///
    /// # Errors
    /// Returns stale-object for a changed lease, retryable for transport
    /// failure, or terminal for invalid transport and response contracts.
    async fn reserve_candidate(
        &self,
        candidate: &DispatchCandidate,
        request: ReserveNodeSlotsRequest,
    ) -> Result<ReserveNodeSlotsResponse, DispatchError> {
        let leader_node_id = request.leader_node_id;
        let leader_fence = request.leader_fencing_token;
        let query_id = request.query_id.as_uuid();
        let wire: wyrd_tonic::wyrd::v1::ReserveNodeSlotsRequest = request.into();
        let binding = ReservationBinding {
            operation: ReservationOperationV1::ReserveSlots,
            source_node_id: leader_node_id,
            source_fence: leader_fence,
            destination_node_id: candidate.node_id,
            destination_fence: candidate.worker_fence,
            query_id,
        };
        let mut client = self.client(candidate).await?;
        // A ticket is single-use, so the one credential retry mints its own
        // rather than replaying the first attempt's nonce.
        let response = match client
            .reserve_slots(
                self.authenticated(self.ticketed_reserve(&wire, &binding)?, false)
                    .await?,
            )
            .await
        {
            Err(status) if status.code() == wyrd_tonic::tonic::Code::Unauthenticated => client
                .reserve_slots(
                    self.authenticated(self.ticketed_reserve(&wire, &binding)?, true)
                        .await?,
                )
                .await
                .map_err(|status| status_error(&status))?,
            result => result.map_err(|status| status_error(&status))?,
        }
        .into_inner();
        response.try_into().map_err(|error| {
            tracing::warn!(
                ?error,
                "Oracle leader could not decode a reservation response"
            );
            DispatchError::Terminal
        })
    }

    /// Releases capacity for one exact planned node/fence target.
    ///
    /// # Errors
    /// Returns stale-object for a changed lease, retryable for transport
    /// failure, or terminal for invalid transport and response contracts.
    async fn release_candidate(
        &self,
        candidate: &DispatchCandidate,
        request: ReleaseNodeSlotsRequest,
    ) -> Result<(), DispatchError> {
        let mut client = self.client(candidate).await?;
        let binding = ReservationBinding {
            operation: ReservationOperationV1::ReleaseSlots,
            source_node_id: request.leader_node_id,
            source_fence: request.leader_fencing_token,
            destination_node_id: candidate.node_id,
            destination_fence: candidate.worker_fence,
            query_id: request.query_id.as_uuid(),
        };
        let wire: wyrd_tonic::wyrd::v1::ReleaseNodeSlotsRequest = request.into();
        // Same single-use discipline as reserve: the retry mints a fresh nonce.
        match client
            .release_slots(
                self.authenticated(self.ticketed_release(&wire, &binding)?, false)
                    .await?,
            )
            .await
        {
            Err(status) if status.code() == wyrd_tonic::tonic::Code::Unauthenticated => client
                .release_slots(
                    self.authenticated(self.ticketed_release(&wire, &binding)?, true)
                        .await?,
                )
                .await
                .map_err(|status| execution_status_error(&status))?,
            result => result.map_err(|status| execution_status_error(&status))?,
        };
        Ok(())
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
        let response = client
            .execute_fragment(self.authenticated(wire, false).await?)
            .await
            .map_err(|status| {
                tracing::warn!(code = ?status.code(), "Oracle peer execute rejected");
                execution_status_error(&status)
            })?;
        let mut stream = response.into_inner();
        let output = async_stream::stream! {
            while let Some(frame) = stream.next().await {
                yield frame.map_err(|status| stream_status_error(&status)).and_then(|frame| frame.try_into().map_err(|error| {
                    tracing::warn!(?error, "Oracle leader could not decode a worker frame");
                    DispatchError::Terminal
                }));
            }
        };
        Ok(Box::pin(output))
    }

    /// Adds workload authorization metadata when configured.
    async fn authenticated<T>(
        &self,
        value: T,
        force_refresh: bool,
    ) -> Result<Request<T>, DispatchError> {
        let mut request = Request::new(value);
        let bearer = self
            .credentials
            .bearer(force_refresh)
            .await
            .map_err(|error| {
                tracing::error!(
                    ?error,
                    force_refresh,
                    "Oracle peer bearer acquisition failed"
                );
                error
            })?;
        let value: MetadataValue<wyrd_tonic::tonic::metadata::Ascii> = format!("Bearer {bearer}")
            .parse()
            .map_err(|_| DispatchError::Unavailable)?;
        request.metadata_mut().insert("x-wyrd-access-token", value);
        Ok(request)
    }
}

#[async_trait]
impl OraclePeerTransport for TonicOraclePeerTransport {
    /// Reserves pending capacity through the generated tonic client.
    ///
    /// # Errors
    /// Returns retryable transport or terminal conversion failure.
    async fn reserve(
        &self,
        worker: NodeId,
        request: ReserveNodeSlotsRequest,
    ) -> Result<ReserveNodeSlotsResponse, DispatchError> {
        let candidate = self.current_candidate(worker)?;
        self.reserve_candidate(&candidate, request).await
    }

    /// Releases one tuple-bound reservation through the generated tonic client.
    ///
    /// # Errors
    /// Returns retryable transport or terminal conversion failure.
    async fn release(
        &self,
        worker: NodeId,
        request: ReleaseNodeSlotsRequest,
    ) -> Result<(), DispatchError> {
        let candidate = self.current_candidate(worker)?;
        self.release_candidate(&candidate, request).await
    }

    /// Adapts one footer-terminated tonic stream without collecting frames.
    ///
    /// # Errors
    /// Returns retryable transport or terminal frame-conversion failure.
    async fn execute(
        &self,
        worker: NodeId,
        request: ExecuteFragmentRequest,
        _admitted_grant: Option<LeaderAdmittedGrant>,
    ) -> Result<WorkerAttemptStream, DispatchError> {
        let candidate = self.current_candidate(worker)?;
        self.execute_candidate(&candidate, request).await
    }
}

/// Routes the local Oracle identity in-process and every remote identity through tonic.
pub struct OraclePeerTransportDirectory {
    /// Physical node identity that must never traverse the network transport.
    local_node_id: NodeId,
    /// Shared in-process adapter backed by the same fenced worker as the gRPC service.
    local: Arc<dyn OraclePeerTransport>,
    /// Closed remote route separating live production resolution from injection.
    remote: RemoteOraclePeerTransport,
}

/// Private remote dispatch variants preserving the public transport contract.
enum RemoteOraclePeerTransport {
    /// Production tonic owner that resolves the exact planned candidate.
    Production(Arc<TonicOraclePeerTransport>),
    /// Test-only adapter retaining isolated transport injection.
    #[cfg(test)]
    Injected(Arc<dyn OraclePeerTransport>),
}

impl OraclePeerTransportDirectory {
    /// Creates an unambiguous production directory from concrete local and tonic adapters.
    #[must_use]
    pub fn new(
        local_node_id: NodeId,
        local: Arc<LocalOraclePeerTransport>,
        remote: Arc<TonicOraclePeerTransport>,
    ) -> Self {
        Self {
            local_node_id,
            local,
            remote: RemoteOraclePeerTransport::Production(remote),
        }
    }

    /// Creates a directory from injectable transports for isolated owner tests.
    #[cfg(test)]
    #[must_use]
    pub(super) fn new_for_test(
        local_node_id: NodeId,
        local: Arc<dyn OraclePeerTransport>,
        remote: Arc<dyn OraclePeerTransport>,
    ) -> Self {
        Self {
            local_node_id,
            local,
            remote: RemoteOraclePeerTransport::Injected(remote),
        }
    }

    /// Returns whether `node_id` is the exact in-process Oracle identity.
    #[must_use]
    pub fn is_local(&self, node_id: NodeId) -> bool {
        node_id == self.local_node_id
    }

    /// Reserves through the identity-selected local or remote adapter.
    ///
    /// # Errors
    ///
    /// Returns the selected adapter's retryable or terminal failure.
    async fn reserve(
        &self,
        candidate: &DispatchCandidate,
        request: ReserveNodeSlotsRequest,
    ) -> Result<ReserveNodeSlotsResponse, DispatchError> {
        if self.is_local(candidate.node_id)
            && candidate.role == wyrd_spec::vala::api::ClusterRole::Oracle
        {
            self.local.reserve(candidate.node_id, request).await
        } else {
            match &self.remote {
                RemoteOraclePeerTransport::Production(remote) => {
                    remote.reserve_candidate(candidate, request).await
                }
                #[cfg(test)]
                RemoteOraclePeerTransport::Injected(remote) => {
                    remote.reserve(candidate.node_id, request).await
                }
            }
        }
    }

    /// Reserves one participant's whole query envelope for a distributed graph.
    ///
    /// This is the Analytical leader's only reservation entry point. It refuses
    /// a request that names no graph rather than silently taking a fragment's
    /// worker quantum, because the two charges are different sizes and a graph
    /// admitted on a fragment's quantum would execute a whole plan on capacity
    /// sized for one leaf.
    ///
    /// # Errors
    ///
    /// Returns [`DispatchError::Terminal`] when `request` names no graph,
    /// [`DispatchError::Capacity`] when the participant declined, and the
    /// selected adapter's failure otherwise.
    pub async fn reserve_graph(
        &self,
        candidate: &DispatchCandidate,
        request: ReserveNodeSlotsRequest,
    ) -> Result<PendingNodeReservation, DispatchError> {
        if request.graph.is_none() {
            return Err(DispatchError::Terminal);
        }
        match self.reserve(candidate, request).await? {
            ReserveNodeSlotsResponse::Pending(pending) => Ok(pending),
            ReserveNodeSlotsResponse::Rejected(_) => Err(DispatchError::Capacity),
        }
    }

    /// Releases one graph reservation this node took on a participant.
    ///
    /// Idempotent by construction: a participant that has already leased the
    /// reservation into graph ownership no longer holds the pending entry and
    /// answers successfully, which is what lets a leader release every
    /// participant it reserved without knowing which ones the plan reached.
    ///
    /// # Errors
    ///
    /// Returns the selected adapter's retryable or terminal failure.
    pub async fn release_graph_reservation(
        &self,
        candidate: &DispatchCandidate,
        request: ReleaseNodeSlotsRequest,
    ) -> Result<(), DispatchError> {
        self.release(candidate, request).await
    }

    /// Releases through the same identity-selected adapter used for reserve.
    ///
    /// # Errors
    ///
    /// Returns the selected adapter's retryable or terminal failure.
    async fn release(
        &self,
        candidate: &DispatchCandidate,
        request: ReleaseNodeSlotsRequest,
    ) -> Result<(), DispatchError> {
        if self.is_local(candidate.node_id)
            && candidate.role == wyrd_spec::vala::api::ClusterRole::Oracle
        {
            self.local.release(candidate.node_id, request).await
        } else {
            match &self.remote {
                RemoteOraclePeerTransport::Production(remote) => {
                    remote.release_candidate(candidate, request).await
                }
                #[cfg(test)]
                RemoteOraclePeerTransport::Injected(remote) => {
                    remote.release(candidate.node_id, request).await
                }
            }
        }
    }

    /// Executes through the same identity-selected adapter used for reserve.
    ///
    /// # Errors
    ///
    /// Returns the selected adapter's retryable or terminal failure.
    async fn execute(
        &self,
        candidate: &DispatchCandidate,
        request: ExecuteFragmentRequest,
        admitted_grant: LeaderAdmittedGrant,
    ) -> Result<WorkerAttemptStream, DispatchError> {
        if self.is_local(candidate.node_id)
            && candidate.role == wyrd_spec::vala::api::ClusterRole::Oracle
        {
            self.local
                .execute(candidate.node_id, request, Some(admitted_grant))
                .await
        } else {
            match &self.remote {
                RemoteOraclePeerTransport::Production(remote) => {
                    remote.execute_candidate(candidate, request).await
                }
                #[cfg(test)]
                RemoteOraclePeerTransport::Injected(remote) => {
                    remote.execute(candidate.node_id, request, None).await
                }
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
        // The private peer protocol reserves `Aborted` for the tenant
        // tripwire so a foreign-tenant refusal on a remote worker reaches the
        // leader as a tenant-isolation outcome instead of a generic
        // security or retryable failure.
        wyrd_tonic::tonic::Code::Aborted => DispatchError::TenantInvariant,
        wyrd_tonic::tonic::Code::FailedPrecondition => DispatchError::EligibleSourceLoss {
            cause: EligibleSourceLossCause::ProviderResolution,
        },
        _ => status_error(status),
    }
}

/// Classifies errors after a stream was delivered as partition-local decoder partials.
fn stream_status_error(status: &Status) -> DispatchError {
    match status.code() {
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

/// Leader-admitted execution grant handed to a leader-local fragment.
///
/// A leader-local fragment runs inside the leader's own admitted envelope, so
/// it must be shaped by that admission rather than by a fresh worker quantum.
/// Carrying the grant with the pool keeps the three session knobs derived from
/// one admission decision.
#[derive(Clone)]
pub struct LeaderAdmittedGrant {
    /// Shared pool from the leader's complete admitted query envelope.
    pub memory_pool: Arc<dyn datafusion::execution::memory_pool::MemoryPool>,
    /// Trusted grant bytes backing that pool.
    pub granted_memory_bytes: usize,
    /// Partition ceiling admitted for the leader's query.
    pub admitted_target_partitions: usize,
}

impl std::fmt::Debug for LeaderAdmittedGrant {
    /// Formats only the non-sensitive admitted execution bounds.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LeaderAdmittedGrant")
            .field("granted_memory_bytes", &self.granted_memory_bytes)
            .field(
                "admitted_target_partitions",
                &self.admitted_target_partitions,
            )
            .finish_non_exhaustive()
    }
}

/// Immutable query and authorization bindings used for all fragment attempts.
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
    /// Admission class and worker slot demand.
    pub query_class: QueryClass,
    /// Worker slots charged by one fragment.
    pub slot_units: u32,
    /// Server-derived permission digest.
    pub permission_digest: String,
    /// Hard attempt-buffer byte ceiling.
    pub attempt_bytes: usize,
    /// In-memory threshold before query-scoped spill.
    pub attempt_memory_bytes: usize,
    /// Shared pool from the leader's complete admitted query envelope.
    pub query_memory_pool: Arc<dyn datafusion::execution::memory_pool::MemoryPool>,
    /// Trusted grant bytes backing `query_memory_pool`.
    pub granted_memory_bytes: usize,
    /// Partition ceiling admitted for this query by that same grant.
    pub admitted_target_partitions: usize,
    /// Admission-owned cancellation propagated to every attempt await.
    pub cancellation: CancellationToken,
    /// Absolute deadline shared by reserve, execute, reads, and cleanup.
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

/// Owns claims construction and one ambiguity-terminal reserve/execute/release cut.
pub struct FragmentDispatcher {
    /// Narrow server-owned authority used to mint a fresh ticket per attempt.
    ticket_minter: Arc<dyn PeerTicketMinter>,
    /// Node-aware directory enforcing in-process leader and tonic remote routing.
    ///
    /// Shared rather than owned: the Analytical leader reserves its graph
    /// participants through the same directory, so both paths route through one
    /// identity-selected set of adapters.
    transports: Arc<OraclePeerTransportDirectory>,
}

impl FragmentDispatcher {
    /// Creates a dispatcher from narrow authority and transport capabilities.
    #[must_use]
    pub fn new(
        ticket_minter: Arc<dyn PeerTicketMinter>,
        transports: Arc<OraclePeerTransportDirectory>,
    ) -> Self {
        Self {
            ticket_minter,
            transports,
        }
    }

    /// Reserves one candidate's slots, or reports that it declined.
    ///
    /// Scribe candidates hold no slot registry, so they are admitted with a nil
    /// reservation without a round trip. An Oracle candidate is reserved under
    /// the query's remaining deadline and its cancellation token, so a cancelled
    /// or expired query stops reserving rather than continuing down the
    /// candidate list.
    ///
    /// # Errors
    ///
    /// Returns [`DispatchError::Unavailable`] when the deadline has already
    /// passed, the query was cancelled, or the reserve call timed out, and
    /// propagates a terminal reservation failure unchanged. `Ok(None)` means the
    /// candidate rejected the reservation and the caller should try the next one.
    async fn reserve_candidate(
        &self,
        candidate: &DispatchCandidate,
        reserve: ReserveNodeSlotsRequest,
        context: &DispatchContext,
    ) -> Result<Option<PendingNodeReservation>, DispatchError> {
        if candidate.role == wyrd_spec::vala::api::ClusterRole::Scribe {
            return Ok(Some(PendingNodeReservation {
                reservation_id: ReservationId::new(uuid::Uuid::nil()),
                expires_at: reserve.expires_at,
            }));
        }
        let remaining = context
            .deadline
            .checked_duration_since(Instant::now())
            .ok_or(DispatchError::Unavailable)?;
        match tokio::select! {
            () = context.cancellation.cancelled() => Err(DispatchError::Unavailable),
            result = tokio::time::timeout(remaining, self.transports.reserve(candidate, reserve)) =>
                result.map_err(|_| DispatchError::Unavailable).and_then(std::convert::identity),
        } {
            Ok(ReserveNodeSlotsResponse::Rejected(_)) => Ok(None),
            Ok(ReserveNodeSlotsResponse::Pending(pending)) => Ok(Some(pending)),
            Err(error) => {
                tracing::error!(?error, "Oracle peer reservation failed terminally");
                Err(error)
            }
        }
    }

    /// Executes one candidate, advancing only after an authenticated capacity rejection.
    ///
    /// A timeout, cancellation, transport error, follower error, or accepted
    /// reservation failure is terminal because delivery may be ambiguous. A
    /// proven pre-delivery `Rejected` response may advance to the next ordered
    /// candidate. Attempt bytes become visible only after footer validation.
    ///
    /// # Errors
    /// Returns the first ambiguity-terminal failure or capacity when every
    /// candidate explicitly rejects before delivery.
    pub async fn execute(
        &self,
        context: &DispatchContext,
        fragment: PhysicalDispatchFragment,
        candidates: &[DispatchCandidate],
    ) -> Result<ValidatedAttempt, DispatchError> {
        if !self.transports.is_local(context.leader_node_id) {
            return Err(DispatchError::Terminal);
        }
        // Last retryable attempt failure, kept so an exhausted candidate list
        // reports the real cause instead of a bare admission failure.
        let mut last_retryable: Option<Result<ValidatedAttempt, DispatchError>> = None;
        for candidate in candidates {
            if candidate.role != fragment.target_role {
                return Err(DispatchError::Terminal);
            }
            let expires_at = Utc::now() + PENDING_TTL;
            let reserve = ReserveNodeSlotsRequest {
                query_id: context.query_id,
                leader_node_id: context.leader_node_id,
                leader_fencing_token: context.leader_fence,
                query_class: context.query_class,
                slot_units: context.slot_units,
                expires_at,
                // Fragment dispatch names no graph: it charges a worker
                // quantum, not the whole query envelope a graph lease owns.
                graph: None,
            };
            let Some(pending) = self.reserve_candidate(candidate, reserve, context).await? else {
                continue;
            };
            let release = ReleaseNodeSlotsRequest {
                reservation_id: pending.reservation_id,
                query_id: context.query_id,
                leader_node_id: context.leader_node_id,
                leader_fencing_token: context.leader_fence,
            };
            let claims = peer_ticket_claims(candidate, context, &fragment, &pending)?;
            let Ok(ticket) = self.ticket_minter.mint_peer_ticket(&claims) else {
                tracing::error!("Oracle peer ticket mint failed");
                if candidate.role == wyrd_spec::vala::api::ClusterRole::Oracle {
                    self.release_pending(candidate, release, context).await;
                }
                return Err(DispatchError::Partial {
                    attempt: None,
                    reason: DispatchPartialReason::Setup,
                });
            };
            let request = ExecuteFragmentRequest {
                ticket,
                physical_plan_bytes: fragment.physical_plan_bytes.clone(),
                reservation_id: pending.reservation_id,
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
            };
            let result = self
                .execute_attempt(candidate, request, context, &fragment)
                .await;
            if result.is_ok() {
                return result;
            }
            if candidate.role == wyrd_spec::vala::api::ClusterRole::Oracle {
                self.release_pending(candidate, release, context).await;
            }
            // A transient loss on one worker — a restarting peer, a reset
            // connection, a source that briefly could not be opened — must not
            // fail the query while another candidate can still serve it. Only a
            // failure that would recur or must not be retried elsewhere ends the
            // dispatch here: a contract or security refusal, a foreign-tenant
            // row, a pinned object that no longer exists, and admission
            // pressure that the next candidate would also hit.
            match result {
                Err(DispatchError::Unavailable | DispatchError::EligibleSourceLoss { .. }) => {
                    last_retryable = Some(result);
                }
                _ => return result,
            }
        }
        // Reaching here means every candidate either rejected its reservation
        // or failed retryably. Report the last real failure when there was one
        // so the caller sees why, and admission pressure otherwise.
        last_retryable.unwrap_or(Err(DispatchError::Capacity))
    }

    /// Attempts immediate tuple-bound cleanup after any accepted-attempt failure.
    async fn release_pending(
        &self,
        candidate: &DispatchCandidate,
        release: ReleaseNodeSlotsRequest,
        context: &DispatchContext,
    ) {
        let result = tokio::select! {
            biased;
            result = self.transports.release(candidate, release) => result,
            () = tokio::time::sleep_until(context.deadline) => Err(DispatchError::Unavailable),
        };
        if let Err(error) = result {
            tracing::warn!(
                worker = %candidate.node_id.as_uuid(),
                ?error,
                "Oracle pending reservation release failed; worker TTL remains fallback"
            );
        }
    }

    /// Buffers and validates one complete remote or local attempt.
    ///
    /// # Errors
    /// Returns retryable for incomplete/invalid footer or transport outcomes.
    #[tracing::instrument(
        name = "bifrost.oracle.fragment",
        skip_all,
        fields(locality = if candidate.node_id == context.leader_node_id { "local" } else { "remote" })
    )]

    /// Opens one authenticated peer stream and classifies an open failure.
    ///
    /// The open is bounded by both the query's remaining deadline and its
    /// cancellation token, so neither an unresponsive peer nor an abandoned
    /// query can hold the attempt open. Classification of a failure here is the
    /// partial/terminal boundary: a terminal, stale-object, or missing-file
    /// failure keeps its own meaning because the leader must react to each
    /// differently, while an unavailable, source-loss, or capacity failure
    /// becomes a setup partial — no frames were delivered, so the leader may
    /// keep what other participants produced instead of failing the query.
    ///
    /// # Errors
    ///
    /// Returns [`DispatchError::Unavailable`] when the deadline has already
    /// passed, [`DispatchError::Terminal`], [`DispatchError::StaleObject`], and
    /// [`DispatchError::FileNotFound`] unchanged, [`DispatchError::TenantInvariant`]
    /// unchanged, and otherwise a [`DispatchPartialReason::Setup`] partial
    /// carrying no attempt.
    async fn open_attempt_frames(
        &self,
        candidate: &DispatchCandidate,
        request: ExecuteFragmentRequest,
        context: &DispatchContext,
    ) -> Result<WorkerAttemptStream, DispatchError> {
        let remaining = context
            .deadline
            .checked_duration_since(Instant::now())
            .ok_or(DispatchError::Unavailable)?;
        match tokio::select! {
            () = context.cancellation.cancelled() => Err(DispatchError::Unavailable),
            result = tokio::time::timeout(
                remaining,
                self.transports.execute(
                    candidate,
                    request,
                    LeaderAdmittedGrant {
                        memory_pool: Arc::clone(&context.query_memory_pool),
                        granted_memory_bytes: context.granted_memory_bytes,
                        admitted_target_partitions: context.admitted_target_partitions,
                    },
                ),
            ) =>
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
                Err(open_failure(error))
            }
        }
    }

    async fn execute_attempt(
        &self,
        candidate: &DispatchCandidate,
        request: ExecuteFragmentRequest,
        context: &DispatchContext,
        fragment: &PhysicalDispatchFragment,
    ) -> Result<ValidatedAttempt, DispatchError> {
        let locality = if candidate.node_id == context.leader_node_id {
            FragmentLocality::Local
        } else {
            FragmentLocality::Remote
        };
        let mut telemetry = FragmentTelemetry::start(locality);
        let mut buffer = AttemptBuffer::with_memory_pool(
            context.attempt_bytes,
            context.attempt_memory_bytes,
            &context.query_memory_pool,
        )
        .map_err(attempt_error)?;
        let mut frames = match self.open_attempt_frames(candidate, request, context).await {
            Ok(frames) => frames,
            Err(error) => {
                telemetry.finish(FragmentOutcome::Failed, 0);
                return Err(error);
            }
        };
        while let Some(frame) = tokio::select! {
            () = context.cancellation.cancelled() => Some(Err(DispatchError::Unavailable)),
            () = tokio::time::sleep_until(context.deadline) => Some(Err(DispatchError::Unavailable)),
            frame = frames.next() => frame,
        } {
            let frame = match frame {
                Ok(frame) => frame,
                Err(error) => {
                    tracing::warn!(?error, "Oracle follower frame stream completed partially");
                    record_peer_attempt(FragmentOutcome::Failed, dispatch_error_label(&error));
                    return Err(mid_stream_failure(error, buffer));
                }
            };
            if let Err(error) = buffer.push(frame) {
                return if error == AttemptError::Schema {
                    Err(DispatchError::Terminal)
                } else {
                    Err(DispatchError::Partial {
                        attempt: buffer.finish_partial().ok(),
                        reason: DispatchPartialReason::Decoder,
                    })
                };
            }
        }
        if !buffer.has_footer() {
            return Err(DispatchError::Partial {
                attempt: buffer.finish_partial().ok(),
                reason: DispatchPartialReason::Decoder,
            });
        }
        let attempt = buffer.finish().map_err(attempt_error)?;
        if attempt.footer.fragment_id != fragment.plan_fingerprint
            || attempt.footer.manifest_digest.as_str() != fragment.plan_fingerprint
        {
            record_peer_attempt(FragmentOutcome::Failed, PeerErrorClass::Footer);
            telemetry.finish(FragmentOutcome::Failed, 0);
            return Err(DispatchError::Unavailable);
        }
        record_peer_attempt(FragmentOutcome::Success, PeerErrorClass::None);
        telemetry.finish(FragmentOutcome::Success, attempt.footer.encoded_bytes);
        Ok(attempt)
    }
}

/// Maps one stream-open failure to the outcome the leader must classify.
///
/// No frame was delivered, so a softened failure carries no attempt: the
/// leader keeps whatever the other participants produced and records that this
/// one never started. A failure
/// [`DispatchError::must_reach_leader_unchanged`] identifies is returned as-is,
/// and an already-softened [`DispatchError::Partial`] keeps the reason and
/// payload its own boundary chose rather than being relabelled `Setup`.
fn open_failure(error: DispatchError) -> DispatchError {
    if error.must_reach_leader_unchanged() || matches!(error, DispatchError::Partial { .. }) {
        return error;
    }
    DispatchError::Partial {
        attempt: None,
        reason: DispatchPartialReason::Setup,
    }
}

/// Maps one mid-stream frame failure to the outcome the leader must classify.
///
/// The attempt already opened and may have delivered batches, so a softened
/// failure carries whatever `buffer` decoded before the stream died. Keeping
/// those batches is the point of the partial: the leader folds them into the
/// degraded result instead of discarding delivered rows. A failure
/// [`DispatchError::must_reach_leader_unchanged`] identifies is returned as-is,
/// and the buffered batches are dropped with it — the leader fails that
/// partition, so there is nothing to fold them into.
fn mid_stream_failure(error: DispatchError, buffer: AttemptBuffer) -> DispatchError {
    if error.must_reach_leader_unchanged() {
        return error;
    }
    DispatchError::Partial {
        attempt: buffer.finish_partial().ok(),
        reason: DispatchPartialReason::Timeout,
    }
}

/// Invalid or incomplete attempts are retryable because no bytes were admitted.
fn attempt_error(error: AttemptError) -> DispatchError {
    tracing::error!(error = %error, "Oracle fragment attempt buffer failed");
    record_peer_attempt(FragmentOutcome::Failed, PeerErrorClass::Attempt);
    if error == AttemptError::ParentCapacity {
        DispatchError::Capacity
    } else {
        DispatchError::Unavailable
    }
}

/// Maps internal retry classes to closed metric labels.
fn dispatch_error_label(error: &DispatchError) -> PeerErrorClass {
    match error {
        DispatchError::Partial { .. }
        | DispatchError::Unavailable
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

    /// Builds one empty attempt buffer for classification-only proofs.
    ///
    /// No frame is ever pushed, so `finish_partial` reports a missing schema
    /// and the partial carries `None`. That is deliberate: this fixture exists
    /// to observe which [`DispatchError`] variant is selected, not to prove
    /// what a partial retains.
    fn classification_buffer() -> AttemptBuffer {
        let pool: Arc<dyn datafusion::execution::memory_pool::MemoryPool> = Arc::new(
            datafusion::execution::memory_pool::GreedyMemoryPool::new(1 << 20),
        );
        AttemptBuffer::with_memory_pool(1 << 20, 1 << 20, &pool)
            .expect("a fresh buffer reserves inside a 1 MiB pool")
    }

    /// Neither dispatch boundary may soften a partition-failing refusal.
    ///
    /// `Terminal`, `TenantInvariant`, and `StaleObject` each make the leader
    /// fail the partition outright, so each must reach it unchanged from both
    /// the stream open and a mid-stream frame death; folding one into a partial
    /// turns a refusal into a degraded success, which for the tenant tripwire
    /// is a silent isolation breach.
    #[test]
    fn neither_boundary_softens_a_partition_failing_refusal() {
        for error in [
            DispatchError::Terminal,
            DispatchError::TenantInvariant,
            DispatchError::StaleObject,
        ] {
            let label = format!("{error:?}");
            assert!(
                error.must_reach_leader_unchanged(),
                "{label} must be preserved"
            );
            assert!(
                open_failure(error).must_reach_leader_unchanged(),
                "{label} must survive the open boundary"
            );
        }
    }

    /// A stream that never opened degrades with no attempt, and an already
    /// softened partial keeps the reason its own boundary chose.
    #[test]
    fn open_failure_degrades_recoverable_losses_without_an_attempt() {
        assert!(matches!(
            open_failure(DispatchError::Unavailable),
            DispatchError::Partial {
                attempt: None,
                reason: DispatchPartialReason::Setup,
            }
        ));
        assert!(matches!(
            open_failure(DispatchError::Partial {
                attempt: None,
                reason: DispatchPartialReason::Decoder,
            }),
            DispatchError::Partial {
                reason: DispatchPartialReason::Decoder,
                ..
            }
        ));
    }

    /// A frame stream that dies mid-attempt must not soften a refusal.
    #[test]
    fn mid_stream_failure_preserves_partition_failing_refusals() {
        assert!(matches!(
            mid_stream_failure(DispatchError::Terminal, classification_buffer()),
            DispatchError::Terminal
        ));
        assert!(matches!(
            mid_stream_failure(DispatchError::TenantInvariant, classification_buffer()),
            DispatchError::TenantInvariant
        ));
        assert!(matches!(
            mid_stream_failure(DispatchError::StaleObject, classification_buffer()),
            DispatchError::StaleObject
        ));
    }

    /// Failures the leader is allowed to degrade become timeout partials.
    #[test]
    fn mid_stream_failure_degrades_recoverable_losses() {
        for error in [
            DispatchError::Unavailable,
            DispatchError::Capacity,
            DispatchError::FileNotFound,
            DispatchError::EligibleSourceLoss {
                cause: EligibleSourceLossCause::ProviderResolution,
            },
            DispatchError::Partial {
                attempt: None,
                reason: DispatchPartialReason::Setup,
            },
        ] {
            let label = format!("{error:?}");
            assert!(
                matches!(
                    mid_stream_failure(error, classification_buffer()),
                    DispatchError::Partial {
                        reason: DispatchPartialReason::Timeout,
                        ..
                    }
                ),
                "{label} must degrade to a timeout partial"
            );
        }
    }

    /// Builds one leader-admitted grant for in-process worker tests.
    ///
    /// Mirrors what the leader hands a local fragment: the admitted pool plus
    /// the grant bytes and partition ceiling that admission produced.
    fn test_admitted_grant(granted_memory_bytes: usize) -> LeaderAdmittedGrant {
        LeaderAdmittedGrant {
            memory_pool: Arc::new(datafusion::execution::memory_pool::GreedyMemoryPool::new(
                granted_memory_bytes,
            )),
            granted_memory_bytes,
            admitted_target_partitions: 1,
        }
    }
    use std::sync::atomic::{AtomicUsize, Ordering};

    use arrow::array::Int64Array;
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::record_batch::RecordBatch;

    use super::super::peer::{
        DeterministicTestSigner, NoopPeerSecurityAudit, VerifiedClaimsBytes, projection_digest,
    };
    use super::*;
    use datafusion::execution::memory_pool::GreedyMemoryPool;
    use wyrd_spec::vala::api::{
        ClusterCapabilities, ClusterNodeKey, ClusterRole, ClusterRoleLease, FollowerScanAssignment,
        OracleCapabilitiesV1, PersistedFileAssignment, ScribeProviderCut, TenantTableBinding,
        TimeGranularityWire,
    };

    /// Deterministic verifier that preserves the already encoded claims bytes.
    struct ClaimsPassthroughVerifier;

    /// Deterministic role-local provider used by worker ownership tests.
    struct TestFollowerResolver;

    #[async_trait]
    impl FollowerSourceResolver for TestFollowerResolver {
        /// Resolves one empty physical source with the fixture schema.
        async fn resolve(
            &self,
            _target_role: ClusterRole,
            assignment: &FollowerScanAssignment,
            _session: &datafusion::execution::session_state::SessionState,
            _reader_io_permit: Option<&crate::oracle::reader_pins::ReaderIoPermit>,
        ) -> Result<super::super::follower::ResolvedFollowerSource, String> {
            let schema = Arc::new(Schema::new(vec![
                Field::new("value", DataType::Int64, false),
                Field::new(
                    wyrd_spec::vala::managed_columns::DATA_TENANT_ID,
                    DataType::Utf8,
                    false,
                ),
            ]));
            let batch = RecordBatch::try_new(
                Arc::clone(&schema),
                vec![
                    Arc::new(Int64Array::from(vec![1_i64])),
                    Arc::new(arrow::array::StringArray::from(vec![
                        assignment.binding.tenant_id.to_string(),
                    ])),
                ],
            )
            .map_err(|error| error.to_string())?;
            let required_schema = super::super::exec::select_schema_by_name(
                schema.as_ref(),
                &assignment.required_columns,
            )
            .map(|(projected, _)| projected)
            .map_err(|error| error.to_string())?;
            let batch = batch
                .project(
                    &required_schema
                        .fields()
                        .iter()
                        .map(|field| {
                            batch
                                .schema()
                                .index_of(field.name())
                                .map_err(|error| error.to_string())
                        })
                        .collect::<Result<Vec<_>, String>>()?,
                )
                .map_err(|error| error.to_string())?;
            datafusion::datasource::memory::MemorySourceConfig::try_new_exec(
                &[vec![batch]],
                Arc::clone(&required_schema),
                None,
            )
            .map(|plan| super::super::follower::ResolvedFollowerSource {
                plan: plan as Arc<dyn datafusion::physical_plan::ExecutionPlan>,
                full_schema: schema,
            })
            .map_err(|error| error.to_string())
        }
    }

    /// Accepting audit collaborator for worker ownership tests.
    struct TestOracleAudit;

    #[async_trait]
    impl super::super::OracleAudit for TestOracleAudit {
        /// Accepts the immutable read decision in this ownership-only test.
        async fn append_read_decision(
            &self,
            _context: &super::super::AuthorizedQueryContext,
            _decision: super::super::BifrostQueryReadDecision,
        ) -> Result<(), super::super::BifrostError> {
            Ok(())
        }

        /// Accepts a security event in this ownership-only test.
        async fn append_security_violation(
            &self,
            _context: super::super::VerifiedSecurityContext,
            _violation: super::super::BifrostSecurityViolation,
        ) -> Result<(), super::super::BifrostError> {
            Ok(())
        }
    }

    #[async_trait]
    impl PeerTicketVerifier for ClaimsPassthroughVerifier {
        /// Returns the ticket claims after the test constructs matching audience and fence values.
        ///
        /// # Errors
        ///
        /// This deterministic verifier does not fail; worker-side typed claim
        /// validation still runs before reservation transition and fragment IO.
        async fn verify_peer_ticket(
            &self,
            ticket: &wyrd_spec::vala::api::SignedPeerTicket,
            _expected_worker: NodeId,
            _expected_worker_fence: u64,
            _now: DateTime<Utc>,
        ) -> Result<VerifiedClaimsBytes, PeerSecurityError> {
            Ok(VerifiedClaimsBytes(ticket.claims_bytes.clone()))
        }
    }

    /// Immutable scan closure every dispatcher physical-plan fixture shares.
    ///
    /// The dispatcher tests exercise ticket minting, reservation accounting,
    /// role fencing, and attempt framing over a serialized physical plan, so
    /// the only per-request source facts they need are the pinned schema
    /// fingerprint the placeholder leaf and assignment both carry, the
    /// projection the ticket digests, and the absolute deadline the ticket
    /// expires at. No object is opened, so no Parquet file is written.
    struct DispatcherFixture {
        /// Pinned fingerprint shared by the placeholder leaf and the assignment.
        schema_fingerprint: String,
        /// Authorized projection covered by the ticket's projection digest.
        projection: Vec<String>,
        /// Absolute ticket expiry expressed as Unix milliseconds.
        deadline_unix_ms: i64,
    }

    /// Builds the deterministic closure shared by every dispatcher request fixture.
    fn dispatcher_fixture() -> DispatcherFixture {
        let schema = Arc::new(Schema::new(vec![
            Field::new("value", DataType::Int64, false),
            Field::new(
                wyrd_spec::vala::managed_columns::DATA_TENANT_ID,
                DataType::Utf8,
                false,
            ),
        ]));
        DispatcherFixture {
            schema_fingerprint: crate::oracle::assignment_schema_fingerprint(&schema),
            projection: vec!["value".to_owned()],
            deadline_unix_ms: Utc::now().timestamp_millis() + 60_000,
        }
    }

    /// Encodes one matching worker request for a retained reservation.
    /// Hourly Scribe provider cut every protocol-v3 dispatcher fixture carries.
    ///
    /// The digest only covers the partition bounds when a cut is present, so
    /// the authority contract needs a fixture cut to tamper with. The two
    /// bounds are adjacent hours, which keeps [`ScribeProviderCut::is_valid`]
    /// satisfied while leaving both granularity and start free to mutate.
    /// `writer_epoch` must equal the target role fence, which follower
    /// preflight compares before it will admit the cut at all.
    fn fixture_scribe_cut(writer_epoch: u64) -> ScribeProviderCut {
        ScribeProviderCut {
            writer_epoch,
            start_partition: fixture_partition(TimeGranularityWire::Hour, 1_787_493_600_000_000),
            end_partition: fixture_partition(TimeGranularityWire::Hour, 1_787_497_200_000_000),
            maximum_batch_count: 16,
            maximum_retained_bytes: 1_048_576,
        }
    }

    /// Builds one canonical partition from epoch microseconds.
    ///
    /// # Panics
    ///
    /// Panics when `start_micros` is not the exact boundary of `granularity`;
    /// every call site passes a boundary literal.
    fn fixture_partition(
        granularity: TimeGranularityWire,
        start_micros: i64,
    ) -> wyrd_spec::vala::api::TimePartitionWire {
        wyrd_spec::vala::api::TimePartitionWire::new(
            granularity,
            chrono::DateTime::from_timestamp_micros(start_micros).expect("fixture instant"),
        )
        .expect("fixture instant is an exact partition boundary")
    }

    fn worker_request(
        fragment: &DispatcherFixture,
        reservation_id: ReservationId,
        node: NodeId,
        fence: FencingToken,
        query_id: QueryId,
        tenant: DataTenantId,
    ) -> ExecuteFragmentRequest {
        worker_request_with_cut(
            fragment,
            reservation_id,
            node,
            fence,
            query_id,
            tenant,
            None,
        )
    }

    /// Builds one signed worker request, optionally carrying a Scribe cut.
    ///
    /// A cut is only legal on a Scribe target fence, so the target role is
    /// derived from its presence rather than passed separately; the leader
    /// fence stays Oracle either way. The signed
    /// `assignment_authority_digest` is minted over the finished assignment,
    /// so a caller that mutates the request afterwards is exactly the tamper
    /// case the current peer protocol must reject.
    ///
    /// # Panics
    ///
    /// Panics when plan encoding, digest computation, or ticket minting fails,
    /// all of which are deterministic for these fixtures.
    fn worker_request_with_cut(
        fragment: &DispatcherFixture,
        reservation_id: ReservationId,
        node: NodeId,
        fence: FencingToken,
        query_id: QueryId,
        tenant: DataTenantId,
        scribe_provider_cut: Option<ScribeProviderCut>,
    ) -> ExecuteFragmentRequest {
        let target_role = if scribe_provider_cut.is_some() {
            ClusterRole::Scribe
        } else {
            ClusterRole::Oracle
        };
        let schema = Arc::new(Schema::new(vec![
            Field::new("value", DataType::Int64, false),
            Field::new(
                wyrd_spec::vala::managed_columns::DATA_TENANT_ID,
                DataType::Utf8,
                false,
            ),
        ]));
        let physical_plan_bytes =
            datafusion_proto::bytes::physical_plan_to_bytes_with_extension_codec(
                Arc::new(super::super::codec::RemoteSourcePlaceholderExec::new(
                    "dispatcher-test-scan",
                    &fragment.schema_fingerprint,
                    schema,
                )),
                &super::super::codec::OraclePhysicalExtensionCodec::encoder(),
            )
            .expect("native physical plan encoding")
            .to_vec();
        let plan_fingerprint = super::super::codec::physical_plan_fingerprint(&physical_plan_bytes);
        // An Oracle fragment always carries at least one dispatched file: the
        // leader marks a file-less Oracle partition empty instead of sending
        // it, and the follower refuses a fragment with no scannable work.
        let persisted = if scribe_provider_cut.is_some() {
            PersistedFileAssignment { files: Vec::new() }
        } else {
            PersistedFileAssignment {
                files: vec![crate::oracle::test_persisted_descriptor(
                    "dispatcher-test-file-0.parquet",
                )],
            }
        };
        let assignments = vec![FollowerScanAssignment {
            scan_id: "dispatcher-test-scan".to_owned(),
            binding: TenantTableBinding {
                tenant_id: tenant,
                namespace: "vala.bifrost".to_owned(),
                table: "events".to_owned(),
            },
            persisted,
            scribe_provider_cut,
            schema_fingerprint: fragment.schema_fingerprint.clone(),
            required_columns: vec![
                "value".to_owned(),
                wyrd_spec::vala::managed_columns::DATA_TENANT_ID.to_owned(),
            ],
            predicates: Vec::new(),
            reader_cut: wyrd_spec::vala::api::FollowerReaderCut::no_snapshot(uuid::Uuid::nil(), 1),
        }];
        let claims = PeerTicketClaims {
            protocol_version: PEER_PROTOCOL_VERSION,
            audience: node.as_uuid().as_bytes().to_vec(),
            worker_fence: fence,
            leader_node_id: node.as_uuid().as_bytes().to_vec(),
            leader_fence: fence,
            query_id: query_id.as_uuid().as_bytes().to_vec(),
            tenant_id: tenant.as_uuid().as_bytes().to_vec(),
            nonce: uuid::Uuid::now_v7().as_bytes().to_vec(),
            expires_at_ms: fragment.deadline_unix_ms,
            execution_deadline_unix_ms: fragment.deadline_unix_ms,
            binding: "vala.bifrost.events".to_owned(),
            fragment_digest: plan_fingerprint.clone(),
            manifest_digest: plan_fingerprint.clone(),
            projection_digest: projection_digest(&fragment.projection),
            permission_digest: "permission".to_owned(),
            assignment_authority_digest: crate::oracle::peer::assignment_authority_digest_for(
                &assignments,
            )
            .expect("deterministic fixture digest"),
        };
        let ticket = DeterministicTestSigner {
            key_id: "test".to_owned(),
        }
        .mint_peer_ticket(&claims)
        .expect("deterministic ticket");
        ExecuteFragmentRequest {
            ticket,
            physical_plan_bytes,
            reservation_id,
            leader_fence: OracleRoleFence {
                node_id: node,
                role: ClusterRole::Oracle,
                fencing_token: fence,
            },
            target_fence: OracleRoleFence {
                node_id: node,
                role: target_role,
                fencing_token: fence,
            },
            assignments,
            plan_fingerprint,
        }
    }

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
                required_columns: vec![
                    "value".to_owned(),
                    wyrd_spec::vala::managed_columns::DATA_TENANT_ID.to_owned(),
                ],
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
                DispatchError::Partial { .. } => "partial",
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

    /// Transport that injects one ambiguity-terminal reservation failure.
    struct AmbiguousReserveTransport {
        /// Number of reserve calls observed across distinct candidates.
        reserve_calls: AtomicUsize,
    }

    /// Ticket minter that fails after a worker has accepted pending capacity.
    struct FailingTicketMinter;

    impl PeerTicketMinter for FailingTicketMinter {
        /// Injects one deterministic signing failure.
        ///
        /// # Errors
        ///
        /// Always returns [`PeerSecurityError::Encoding`].
        fn mint_peer_ticket(
            &self,
            _claims: &PeerTicketClaims,
        ) -> Result<wyrd_spec::vala::api::SignedPeerTicket, PeerSecurityError> {
            Err(PeerSecurityError::Encoding)
        }
    }

    /// Transport probe recording cleanup after an accepted pending reservation.
    struct PendingCleanupTransport {
        /// Number of tuple-bound release calls observed.
        release_calls: AtomicUsize,
        /// Number of execute calls, which must remain zero when minting fails.
        execute_calls: AtomicUsize,
    }

    /// Transport that accepts capacity and then stalls its execute stream forever.
    struct StalledExecuteTransport {
        /// Number of releases observed after the shared deadline expires.
        release_calls: AtomicUsize,
    }

    #[async_trait]
    impl OraclePeerTransport for StalledExecuteTransport {
        async fn reserve(
            &self,
            _worker: NodeId,
            request: ReserveNodeSlotsRequest,
        ) -> Result<ReserveNodeSlotsResponse, DispatchError> {
            Ok(ReserveNodeSlotsResponse::Pending(PendingNodeReservation {
                reservation_id: ReservationId::new(uuid::Uuid::now_v7()),
                expires_at: request.expires_at,
            }))
        }

        async fn release(
            &self,
            _worker: NodeId,
            _request: ReleaseNodeSlotsRequest,
        ) -> Result<(), DispatchError> {
            self.release_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn execute(
            &self,
            _worker: NodeId,
            _request: ExecuteFragmentRequest,
            _admitted_grant: Option<LeaderAdmittedGrant>,
        ) -> Result<WorkerAttemptStream, DispatchError> {
            Ok(Box::pin(futures_util::stream::pending()))
        }
    }

    #[async_trait]
    impl OraclePeerTransport for PendingCleanupTransport {
        /// Accepts one pending reservation for cleanup verification.
        ///
        /// # Errors
        ///
        /// This deterministic reserve path never fails.
        async fn reserve(
            &self,
            _worker: NodeId,
            request: ReserveNodeSlotsRequest,
        ) -> Result<ReserveNodeSlotsResponse, DispatchError> {
            Ok(ReserveNodeSlotsResponse::Pending(PendingNodeReservation {
                reservation_id: ReservationId::new(uuid::Uuid::now_v7()),
                expires_at: request.expires_at,
            }))
        }

        /// Records the immediate release of the accepted reservation.
        ///
        /// # Errors
        ///
        /// This deterministic release path never fails.
        async fn release(
            &self,
            _worker: NodeId,
            _request: ReleaseNodeSlotsRequest,
        ) -> Result<(), DispatchError> {
            self.release_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        /// Records an erroneous execute call if pre-execute cleanup regresses.
        ///
        /// # Errors
        ///
        /// Always returns terminal because this path must be unreachable.
        async fn execute(
            &self,
            _worker: NodeId,
            _request: ExecuteFragmentRequest,
            _admitted_grant: Option<LeaderAdmittedGrant>,
        ) -> Result<WorkerAttemptStream, DispatchError> {
            self.execute_calls.fetch_add(1, Ordering::SeqCst);
            Err(DispatchError::Terminal)
        }
    }

    #[async_trait]
    impl OraclePeerTransport for AmbiguousReserveTransport {
        /// Returns unavailable once; a correct dispatcher never makes a second call.
        ///
        /// # Errors
        ///
        /// Returns [`DispatchError::Unavailable`] on the injected first call.
        async fn reserve(
            &self,
            _worker: NodeId,
            request: ReserveNodeSlotsRequest,
        ) -> Result<ReserveNodeSlotsResponse, DispatchError> {
            if self.reserve_calls.fetch_add(1, Ordering::SeqCst) == 0 {
                return Err(DispatchError::Unavailable);
            }
            Ok(ReserveNodeSlotsResponse::Pending(PendingNodeReservation {
                reservation_id: ReservationId::new(uuid::Uuid::now_v7()),
                expires_at: request.expires_at,
            }))
        }

        /// Accepts cleanup if an accepted reservation must be released.
        ///
        /// # Errors
        ///
        /// This deterministic transport never fails release.
        async fn release(
            &self,
            _worker: NodeId,
            _request: ReleaseNodeSlotsRequest,
        ) -> Result<(), DispatchError> {
            Ok(())
        }

        /// Injects a terminal execute failure if an invalid second call reaches execution.
        ///
        /// # Errors
        ///
        /// Always returns [`DispatchError::Terminal`] for the focused ambiguity test.
        async fn execute(
            &self,
            _worker: NodeId,
            _request: ExecuteFragmentRequest,
            _admitted_grant: Option<LeaderAdmittedGrant>,
        ) -> Result<WorkerAttemptStream, DispatchError> {
            Err(DispatchError::Terminal)
        }
    }

    /// Creates one exact reservation request.
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
            query_class: QueryClass::Interactive,
            slot_units: 1,
            expires_at,
            graph: None,
        }
    }

    /// A mismatched execute cannot remove another leader's pending reservation.
    #[test]
    fn oracle_peer_reservation_transition_is_tuple_bound() {
        let registry = ReservationRegistry::new(Arc::new(OracleSlotManager::new(2, 2)), 2);
        let now = Utc::now();
        let query = QueryId::new(uuid::Uuid::now_v7());
        let leader = NodeId::new(uuid::Uuid::now_v7());
        let pending = registry
            .reserve(
                &reserve_request(query, leader, 7, now + ChronoDuration::seconds(2)),
                now,
                None,
            )
            .expect("pending reservation");
        assert!(matches!(
            registry.take_for_execute(
                pending.reservation_id,
                query,
                NodeId::new(uuid::Uuid::now_v7()),
                7,
                now
            ),
            Err(DispatchError::Terminal)
        ));
        let running = registry
            .take_for_execute(pending.reservation_id, query, leader, 7, now)
            .expect("matching transition");
        assert_eq!(running.query_class, QueryClass::Interactive);
        drop(running);
    }

    /// Leader-local execution reuses admitted waiter and slot capacity.
    ///
    /// The leader's own query envelope already holds its slot units in the
    /// shared governor ledger, so the leader-local transition must charge
    /// neither a second follower quantum nor a peer-waiter slot.
    #[test]
    fn oracle_peer_local_transition_does_not_double_charge_leader_slot() {
        let oracle = slot_limited_oracle(1);
        let slots = Arc::new(OracleSlotManager::new(1, 1));
        let pending_slot = slots.try_pending().expect("admitted leader pending slot");
        let leader_slot = worker_capacity(&oracle);
        let registry = ReservationRegistry::new(Arc::clone(&slots), 1);
        let now = Utc::now();
        let query = QueryId::new(uuid::Uuid::now_v7());
        let leader = NodeId::new(uuid::Uuid::now_v7());
        let pending = registry
            .reserve_local(
                &reserve_request(query, leader, 11, now + ChronoDuration::seconds(2)),
                now,
            )
            .expect("pending local reservation");

        let running = registry
            .take_for_local_leader_execute(pending.reservation_id, query, leader, 11, now)
            .expect("leader-local transition");

        assert_eq!(running.query_class, QueryClass::Interactive);
        assert!(
            running.worker_resources.is_none(),
            "the leader-local transition charges no follower quantum"
        );
        assert!(slots.try_pending().is_err());
        assert_eq!(oracle.live_slot_units(), 1);
        drop(pending_slot);
        drop(leader_slot);
        assert!(slots.try_pending().is_ok());
        assert_eq!(oracle.live_slot_units(), 0);
    }

    /// A follower whose received assignment was tampered with after the
    /// leader signed the ticket is rejected before any provider resolution
    /// or object I/O, even though the ticket's own signature still verifies.
    ///
    /// The signature only proves the claims bytes were not altered in
    /// transit; it says nothing about whether the assignments physically
    /// dispatched alongside the ticket match what was signed. Recomputing
    /// and comparing the assignment-authority digest is what catches a
    /// tampered `required_columns`/predicate/file list here.
    #[tokio::test]
    async fn tampered_assignment_authority_digest_is_rejected_before_execution() {
        let roles = crate::resources::BifrostRuntimeResources::composed_for_test(
            1024 * 1024 * 1024,
            1024 * 1024 * 1024,
            [crate::resources::BifrostRole::Oracle],
        );
        let oracle = roles.oracle().expect("Oracle capability");
        let node = NodeId::new(uuid::Uuid::now_v7());
        let query_id = QueryId::new(uuid::Uuid::now_v7());
        let tenant = DataTenantId::new_v7();
        let fence = 41;
        let reservations = Arc::new(ReservationRegistry::new(
            Arc::new(OracleSlotManager::new(1, 1)),
            1,
        ));
        let fragment = dispatcher_fixture();
        let worker = OraclePeerWorker::new_physical_with_resources(OraclePeerWorkerConfig {
            worker_node_id: node,
            oracle_fence: fence,
            verifier: Arc::new(ClaimsPassthroughVerifier),
            security_audit: Arc::new(NoopPeerSecurityAudit),
            reservations: Arc::clone(&reservations),
            oracle_resources: oracle.clone(),
            resolver: Arc::new(TestFollowerResolver),
            audit: Arc::new(TestOracleAudit),
        });
        let now = Utc::now();
        let pending = reservations
            .reserve_local(
                &reserve_request(query_id, node, fence, now + ChronoDuration::seconds(2)),
                now,
            )
            .expect("leader-local pending reservation");
        let mut request = worker_request(
            &fragment,
            pending.reservation_id,
            node,
            fence,
            query_id,
            tenant,
        );
        // Tamper with the dispatched assignment after the ticket was signed
        // over the original closure: this must be caught even though the
        // ticket signature itself still verifies cleanly.
        request.assignments[0].required_columns = vec!["tampered_column".to_owned()];

        let result = worker
            .execute_local(request, test_admitted_grant(2 * 1024 * 1024))
            .await;
        let Err(error) = result else {
            panic!("tampered assignment closure must be rejected before execution");
        };
        assert!(matches!(error, DispatchError::Terminal));
    }

    /// One digest-covered mutation applied to a dispatched request.
    ///
    /// Named so the tamper table stays readable; the boxed closure is what
    /// lets each case mutate a different field of the same fixture request.
    type TamperCase = Box<dyn Fn(&mut ExecuteFragmentRequest)>;

    /// Counts resolver invocations so a test can prove a rejected request
    /// never reaches provider resolution or object I/O.
    struct CountingFollowerResolver {
        /// Number of times [`FollowerSourceResolver::resolve`] was called.
        calls: AtomicUsize,
    }

    #[async_trait]
    impl FollowerSourceResolver for CountingFollowerResolver {
        /// Records the call, then delegates to the fixed empty test schema.
        async fn resolve(
            &self,
            target_role: ClusterRole,
            assignment: &FollowerScanAssignment,
            session: &datafusion::execution::session_state::SessionState,
            reader_io_permit: Option<&crate::oracle::reader_pins::ReaderIoPermit>,
        ) -> Result<super::super::follower::ResolvedFollowerSource, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            TestFollowerResolver
                .resolve(target_role, assignment, session, reader_io_permit)
                .await
        }
    }

    /// Builds a counting-resolver worker plus one valid request for it.
    ///
    /// The v3 authority proofs each need an isolated worker, its own
    /// reservation registry, and a request already reserved against it; the
    /// only thing they vary is the fence and how they then tamper with the
    /// request, so the identical setup is built once here. The returned
    /// resolver is the same instance the worker holds, so a caller can assert
    /// on how many times provider resolution was reached.
    fn counting_worker_request(
        oracle: &crate::resources::OracleResources,
        fragment: &DispatcherFixture,
        tenant: DataTenantId,
        fence: FencingToken,
    ) -> (
        OraclePeerWorker,
        Arc<CountingFollowerResolver>,
        ExecuteFragmentRequest,
    ) {
        let node = NodeId::new(uuid::Uuid::now_v7());
        let query_id = QueryId::new(uuid::Uuid::now_v7());
        let reservations = Arc::new(ReservationRegistry::new(
            Arc::new(OracleSlotManager::new(1, 1)),
            1,
        ));
        let resolver = Arc::new(CountingFollowerResolver {
            calls: AtomicUsize::new(0),
        });
        let worker = OraclePeerWorker::new_physical_with_resources(OraclePeerWorkerConfig {
            worker_node_id: node,
            oracle_fence: fence,
            verifier: Arc::new(ClaimsPassthroughVerifier),
            security_audit: Arc::new(NoopPeerSecurityAudit),
            reservations: Arc::clone(&reservations),
            oracle_resources: oracle.clone(),
            resolver: Arc::clone(&resolver) as Arc<dyn FollowerSourceResolver>,
            audit: Arc::new(TestOracleAudit),
        });
        let now = Utc::now();
        let pending = reservations
            .reserve_local(
                &reserve_request(query_id, node, fence, now + ChronoDuration::seconds(2)),
                now,
            )
            .expect("leader-local pending reservation");
        let request = worker_request(
            fragment,
            pending.reservation_id,
            node,
            fence,
            query_id,
            tenant,
        );
        (worker, resolver, request)
    }

    /// Proves every Scribe-cut partition component is covered by the v3
    /// assignment-authority digest.
    ///
    /// A cut-bearing assignment can never reach `execute_local`: an Oracle peer
    /// worker refuses a non-Oracle target role, and follower preflight refuses
    /// an Oracle assignment that carries a cut. The partition contract is
    /// therefore proven where the dispatcher actually mints and compares it,
    /// over the same `assignment_authority_digest_for` seam the ticket claims
    /// are built from.
    ///
    /// # Panics
    ///
    /// Panics when mutating either bound's granularity or start instant leaves
    /// the digest unchanged, or when an identical cut fails to reproduce it.
    fn assert_partition_components_are_digest_covered(
        tenant: DataTenantId,
        fragment: &DispatcherFixture,
    ) {
        let cut_assignment = |cut: ScribeProviderCut| FollowerScanAssignment {
            scan_id: "dispatcher-test-scan".to_owned(),
            binding: TenantTableBinding {
                tenant_id: tenant,
                namespace: "vala.bifrost".to_owned(),
                table: "events".to_owned(),
            },
            persisted: PersistedFileAssignment { files: Vec::new() },
            scribe_provider_cut: Some(cut),
            schema_fingerprint: fragment.schema_fingerprint.clone(),
            required_columns: vec![
                "value".to_owned(),
                wyrd_spec::vala::managed_columns::DATA_TENANT_ID.to_owned(),
            ],
            predicates: Vec::new(),
            reader_cut: wyrd_spec::vala::api::FollowerReaderCut::no_snapshot(uuid::Uuid::nil(), 1),
        };
        let digest_of = |cut: ScribeProviderCut| {
            crate::oracle::peer::assignment_authority_digest_for(&[cut_assignment(cut)])
                .expect("deterministic fixture digest")
        };
        let baseline = digest_of(fixture_scribe_cut(7));

        let mut start_granularity = fixture_scribe_cut(7);
        start_granularity.start_partition =
            fixture_partition(TimeGranularityWire::Day, 1_787_443_200_000_000);
        start_granularity.end_partition =
            fixture_partition(TimeGranularityWire::Day, 1_787_443_200_000_000);

        let mut start_micros = fixture_scribe_cut(7);
        start_micros.start_partition =
            fixture_partition(TimeGranularityWire::Hour, 1_787_490_000_000_000);

        let mut end_micros = fixture_scribe_cut(7);
        end_micros.end_partition =
            fixture_partition(TimeGranularityWire::Hour, 1_787_500_800_000_000);

        for (label, mutated) in [
            ("start granularity", start_granularity),
            ("start micros", start_micros),
            ("end micros", end_micros),
        ] {
            assert_ne!(
                digest_of(mutated),
                baseline,
                "{label} must change the v3 assignment-authority digest"
            );
        }

        // The digest a leader mints is exactly what a worker recomputes, so
        // a matching cut reproduces the baseline byte for byte.
        assert_eq!(digest_of(fixture_scribe_cut(7)), baseline);
    }

    /// Protocol-v3 exact-partition assignment-authority contract, proven as
    /// one seam:
    ///
    /// - a valid v3 request whose recomputed digest matches the signed claims
    ///   executes and reaches the resolver exactly once;
    /// - every digest-covered tamper class is rejected terminally before the
    ///   resolver is ever called, including each Scribe-cut partition
    ///   granularity and start mutated independently;
    /// - an explicit v2 `protocol_version` ticket is rejected by the same gate
    ///   protocol v3 replaced, proving there is no dual decoder.
    #[tokio::test]
    async fn follower_assignment_v3_partition_authority_contract() {
        let roles = crate::resources::BifrostRuntimeResources::composed_for_test(
            1024 * 1024 * 1024,
            1024 * 1024 * 1024,
            [crate::resources::BifrostRole::Oracle],
        );
        let oracle = roles.oracle().expect("Oracle capability");
        let fragment = dispatcher_fixture();
        let tenant = DataTenantId::new_v7();

        // A valid v3 request executes and reaches the resolver exactly once.
        {
            let (worker, resolver, request) =
                counting_worker_request(&oracle, &fragment, tenant, 51);
            worker
                .execute_local(request, test_admitted_grant(2 * 1024 * 1024))
                .await
                .expect("valid v3 assignment authority digest executes");
            assert_eq!(resolver.calls.load(Ordering::SeqCst), 1);
        }

        // Every digest-covered tamper is rejected before the resolver runs.
        let tamper_cases: Vec<TamperCase> = vec![
            Box::new(|request| {
                request.assignments[0].required_columns = vec!["tampered_column".to_owned()];
            }),
            Box::new(|request| {
                request.assignments[0].persisted.files.push(
                    crate::oracle::test_persisted_descriptor("s3://bucket/tampered.parquet"),
                );
            }),
            Box::new(|request| {
                request.assignments[0].schema_fingerprint = "f".repeat(64);
            }),
        ];
        for tamper in tamper_cases {
            let (worker, resolver, mut request) =
                counting_worker_request(&oracle, &fragment, tenant, 52);
            tamper(&mut request);
            let result = worker
                .execute_local(request, test_admitted_grant(2 * 1024 * 1024))
                .await;
            assert!(matches!(result, Err(DispatchError::Terminal)));
            assert_eq!(
                resolver.calls.load(Ordering::SeqCst),
                0,
                "a tampered assignment-authority digest must never reach the resolver"
            );
        }

        // Each Scribe-cut partition component is digest-covered.
        assert_partition_components_are_digest_covered(tenant, &fragment);

        // An explicit v2 `protocol_version` ticket is rejected: protocol v3
        // fully replaced v2 rather than accepting both.
        {
            let (worker, resolver, mut request) =
                counting_worker_request(&oracle, &fragment, tenant, 53);
            let mut claims = PeerTicketClaims::decode(request.ticket.claims_bytes.as_slice())
                .expect("decode fixture claims");
            claims.protocol_version = 2;
            let mut bytes = Vec::new();
            claims.encode(&mut bytes).expect("encode v2 claims");
            request.ticket.claims_bytes = bytes.clone();
            request.ticket.signature = bytes;
            let result = worker
                .execute_local(request, test_admitted_grant(2 * 1024 * 1024))
                .await;
            assert!(matches!(result, Err(DispatchError::Terminal)));
            assert_eq!(
                resolver.calls.load(Ordering::SeqCst),
                0,
                "an explicit v2 protocol_version ticket must never reach the resolver"
            );
        }
    }

    /// Leader-admitted execution reaches a frame while its exact query lease is active.
    #[tokio::test]
    async fn oracle_peer_leader_admitted_executes_under_active_query_owner() {
        let roles = crate::resources::BifrostRuntimeResources::composed_for_test(
            1024 * 1024 * 1024,
            1024 * 1024 * 1024,
            [crate::resources::BifrostRole::Oracle],
        );
        let oracle = roles.oracle().expect("Oracle capability");
        let query_owner = oracle
            .try_acquire_query(crate::resources::OracleResourceRequest::for_class(
                QueryClass::Interactive,
                1.0,
            ))
            .expect("exact query owner");
        let node = NodeId::new(uuid::Uuid::now_v7());
        let query_id = QueryId::new(uuid::Uuid::now_v7());
        let tenant = DataTenantId::new_v7();
        let fence = 31;
        let reservations = Arc::new(ReservationRegistry::new(
            Arc::new(OracleSlotManager::new(1, 1)),
            1,
        ));
        let fragment = dispatcher_fixture();
        let worker = OraclePeerWorker::new_physical_with_resources(OraclePeerWorkerConfig {
            worker_node_id: node,
            oracle_fence: fence,
            verifier: Arc::new(ClaimsPassthroughVerifier),
            security_audit: Arc::new(NoopPeerSecurityAudit),
            reservations: Arc::clone(&reservations),
            oracle_resources: oracle.clone(),
            resolver: Arc::new(TestFollowerResolver),
            audit: Arc::new(TestOracleAudit),
        });
        let now = Utc::now();
        let pending = reservations
            .reserve_local(
                &reserve_request(query_id, node, fence, now + ChronoDuration::seconds(2)),
                now,
            )
            .expect("leader-local pending reservation");
        let request = worker_request(
            &fragment,
            pending.reservation_id,
            node,
            fence,
            query_id,
            tenant,
        );

        let mut execution = worker
            .execute_local(request, test_admitted_grant(2 * 1024 * 1024))
            .await
            .expect("leader-admitted execution");
        let frame = execution
            .stream
            .next()
            .await
            .expect("leader-admitted stream reaches a frame")
            .expect("leader-admitted frame succeeds");
        assert!(matches!(frame, WorkerAttemptFrame::Schema(_)));
        drop(execution);
        drop(query_owner);
        assert_eq!(
            oracle
                .snapshot()
                .expect("healthy root after leader-admitted execution")
                .oracle_memory_used_bytes,
            0
        );
    }

    /// Signed execution time remains distinct from the pending acceptance window.
    ///
    /// # Panics
    ///
    /// Panics if minting clips the execution budget or malformed claims are accepted.
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
            slot_units: 1,
            permission_digest: "permission".to_owned(),
            attempt_bytes: 1_024,
            attempt_memory_bytes: 1_024,
            query_memory_pool: Arc::new(GreedyMemoryPool::new(1_024)),
            granted_memory_bytes: 1_024,
            admitted_target_partitions: 1,
            cancellation: CancellationToken::new(),
            deadline: Instant::now() + std::time::Duration::from_secs(30),
        };
        let candidate = DispatchCandidate {
            node_id: node,
            role: ClusterRole::Oracle,
            worker_fence: 1,
            endpoint: None,
        };
        let pending = PendingNodeReservation {
            reservation_id: ReservationId::new(uuid::Uuid::now_v7()),
            expires_at: now + PENDING_TTL,
        };
        let mut fragment = physical_dispatch_fragment("deadline");
        fragment.deadline_unix_ms = (now + ChronoDuration::seconds(30)).timestamp_millis();
        let claims =
            peer_ticket_claims(&candidate, &context, &fragment, &pending).expect("signed claims");
        assert_eq!(claims.expires_at_ms, pending.expires_at.timestamp_millis());
        assert_eq!(claims.execution_deadline_unix_ms, fragment.deadline_unix_ms);
        assert!(validated_claim_identifiers(&claims).is_ok());
        let mut old = claims.clone();
        old.protocol_version = 3;
        assert_eq!(
            validated_claim_identifiers(&old),
            Err(BifrostSecurityViolationKind::PeerFragment)
        );
        for deadline in [0, -1, i64::MAX, claims.expires_at_ms - 1] {
            let mut invalid = claims.clone();
            invalid.execution_deadline_unix_ms = deadline;
            assert_eq!(
                validated_claim_identifiers(&invalid),
                Err(BifrostSecurityViolationKind::PeerFragment)
            );
        }
        let mut invalid_expiry = claims.clone();
        invalid_expiry.expires_at_ms = i64::MIN;
        assert_eq!(
            validated_claim_identifiers(&invalid_expiry),
            Err(BifrostSecurityViolationKind::PeerFragment)
        );
        fragment.deadline_unix_ms = (now + ChronoDuration::milliseconds(500)).timestamp_millis();
        let short = peer_ticket_claims(&candidate, &context, &fragment, &pending)
            .expect("short query claims");
        assert_eq!(short.expires_at_ms, fragment.deadline_unix_ms);
        assert_eq!(short.execution_deadline_unix_ms, fragment.deadline_unix_ms);
        assert!(validated_claim_identifiers(&short).is_ok());
    }

    /// Remote worker execution retains exactly one root quantum until stream drop.
    #[tokio::test]
    async fn oracle_peer_remote_execution_owns_one_worker_quantum() {
        let roles = crate::resources::BifrostRuntimeResources::composed_for_test(
            1024 * 1024 * 1024,
            1024 * 1024 * 1024,
            [crate::resources::BifrostRole::Oracle],
        );
        let oracle = roles.oracle().expect("Oracle capability");
        let node = NodeId::new(uuid::Uuid::now_v7());
        let query_id = QueryId::new(uuid::Uuid::now_v7());
        let tenant = DataTenantId::new_v7();
        let fence = 37;
        let reservations = Arc::new(ReservationRegistry::new(
            Arc::new(OracleSlotManager::new(1, 1)),
            1,
        ));
        let fragment = dispatcher_fixture();
        let worker = OraclePeerWorker::new_physical_with_resources(OraclePeerWorkerConfig {
            worker_node_id: node,
            oracle_fence: fence,
            verifier: Arc::new(ClaimsPassthroughVerifier),
            security_audit: Arc::new(NoopPeerSecurityAudit),
            reservations: Arc::clone(&reservations),
            oracle_resources: oracle.clone(),
            resolver: Arc::new(TestFollowerResolver),
            audit: Arc::new(TestOracleAudit),
        });
        let now = Utc::now();
        // Drive the production reservation path: the worker quantum is charged
        // at reservation, so a test that inserted a registry entry directly
        // would exercise an admission state the server can never produce.
        let ReserveNodeSlotsResponse::Pending(pending) = worker
            .reserve(&reserve_request(
                query_id,
                node,
                fence,
                now + ChronoDuration::seconds(2),
            ))
            .await
        else {
            panic!("remote pending reservation");
        };
        assert_eq!(
            oracle
                .snapshot()
                .expect("healthy snapshot after reservation")
                .oracle_memory_used_bytes,
            crate::resources::ORACLE_PARTITION_WORKING_MEMORY_BYTES,
            "reservation charges the worker quantum up front"
        );
        let request = worker_request(
            &fragment,
            pending.reservation_id,
            node,
            fence,
            query_id,
            tenant,
        );

        let mut execution = worker.execute(request).await.expect("remote execution");
        assert_eq!(
            oracle
                .snapshot()
                .expect("healthy remote worker snapshot")
                .oracle_memory_used_bytes,
            crate::resources::ORACLE_PARTITION_WORKING_MEMORY_BYTES
        );
        while let Some(frame) = execution.stream.next().await {
            frame.expect("remote worker frame");
        }
        drop(execution);
        assert_eq!(
            oracle
                .snapshot()
                .expect("healthy root after remote stream completion")
                .oracle_memory_used_bytes,
            0
        );
    }

    /// Expiry cleanup releases pending capacity and release is fenced and idempotent.
    #[test]
    fn peer_pending_reservation_expires() {
        let registry = ReservationRegistry::new(Arc::new(OracleSlotManager::new(1, 1)), 1);
        let now = Utc::now();
        let query = QueryId::new(uuid::Uuid::now_v7());
        let leader = NodeId::new(uuid::Uuid::now_v7());
        let pending = registry
            .reserve(
                &reserve_request(query, leader, 9, now + ChronoDuration::milliseconds(1)),
                now,
                None,
            )
            .expect("pending reservation");
        assert_eq!(
            registry.cleanup_expired(now + ChronoDuration::milliseconds(2)),
            0
        );
        let replacement = registry
            .reserve(
                &reserve_request(query, leader, 9, now + ChronoDuration::seconds(1)),
                now,
                None,
            )
            .expect("replacement reservation");
        let request = ReleaseNodeSlotsRequest {
            reservation_id: replacement.reservation_id,
            query_id: query,
            leader_node_id: leader,
            leader_fencing_token: 9,
        };
        assert!(registry.release(&request, now));
        assert!(registry.release(&request, now));
        assert_ne!(pending.reservation_id, replacement.reservation_id);
    }

    /// Builds an Oracle capability whose shared slot ledger holds `units` units.
    ///
    /// The dispatcher's follower path no longer owns a running semaphore, so a
    /// saturation test must saturate the one authority that decides: the shared
    /// governor slot ledger. An explicit slot limit is what makes that ledger a
    /// known size.
    ///
    /// # Panics
    ///
    /// Panics when the deterministic plan or role composition fails.
    fn slot_limited_oracle(units: usize) -> crate::resources::OracleResources {
        crate::resources::BifrostRuntimeResources::from_snapshot(
            crate::resources::SystemResourceSnapshot {
                memory_limit_bytes: 1024 * 1024 * 1024,
                effective_cpu: 8,
                scratch_capacity_bytes: 1024 * 1024 * 1024,
                scratch_available_bytes: 1024 * 1024 * 1024,
                memory_source: crate::resources::ResourceSource::Injected,
                cpu_source: crate::resources::ResourceSource::Injected,
            },
            crate::resources::BifrostResourcePolicy {
                roles: std::collections::BTreeSet::from([crate::resources::BifrostRole::Oracle]),
                memory_limit_bytes: None,
                unmanaged_reserve_bytes: None,
                scratch_limit_bytes: None,
                effective_cpu: None,
                oracle_query_slot_limit: Some(units),
                forge_compaction_memory_limit_bytes: None,
                scratch_root: std::path::PathBuf::new(),
                volume_roots: None,
            },
        )
        .expect("slot-limited Oracle plan")
        .compose_roles()
        .expect("slot-limited Oracle composition")
        .oracle()
        .expect("composition must enable the Oracle capability")
    }

    /// Acquires one Interactive follower quantum as a reservation's capacity.
    ///
    /// # Panics
    ///
    /// Panics when the shared ledger refuses the quantum.
    fn worker_capacity(oracle: &crate::resources::OracleResources) -> ReservedCapacity {
        ReservedCapacity::Fragment(FollowerWorkerResources::Oracle(
            oracle
                .try_acquire_worker(crate::resources::OracleWorkerClass::Interactive)
                .expect("one follower quantum"),
        ))
    }

    /// A reservation is a guarantee: once accepted, execution cannot be refused.
    ///
    /// Capacity is decided once, when the follower quantum is charged against
    /// the shared governor ledger. A saturated peer refuses there — before the
    /// leader has committed to dispatching this participant — and an accepted
    /// reservation carries the charged quantum its fragment will execute under,
    /// so `take_for_execute` cannot turn a negotiated fan-out into a failed
    /// query. Releasing the reservation returns the units, which is what lets
    /// the next reservation through.
    #[test]
    fn accepted_reservation_guarantees_execution_and_saturation_refuses_up_front() {
        let oracle = slot_limited_oracle(1);
        let registry = ReservationRegistry::new(Arc::new(OracleSlotManager::new(1, 1)), 2);
        let now = Utc::now();
        let query = QueryId::new(uuid::Uuid::now_v7());
        let leader = NodeId::new(uuid::Uuid::now_v7());
        let expires = now + ChronoDuration::seconds(2);
        let pending = registry
            .reserve(
                &reserve_request(query, leader, 13, expires),
                now,
                Some(worker_capacity(&oracle)),
            )
            .expect("pending reservation");
        // The single slot unit is committed by the reservation itself, so a
        // second concurrent follower quantum is refused before any reservation
        // is attempted rather than at execute.
        assert!(
            oracle
                .try_acquire_worker(crate::resources::OracleWorkerClass::Interactive)
                .is_err(),
            "the shared ledger refuses a second quantum up front"
        );
        let running = registry
            .take_for_execute(pending.reservation_id, query, leader, 13, now)
            .expect("an accepted reservation always executes");
        assert_eq!(running.query_class, QueryClass::Interactive);
        assert!(
            running.worker_resources.is_some(),
            "the charged quantum is transferred"
        );
        assert_eq!(
            registry.cleanup_expired(now),
            0,
            "the claimed reservation left the registry"
        );
        drop(running);
        drop(worker_capacity(&oracle));
    }

    /// An unclaimed reservation returns its charged slot units at expiry.
    ///
    /// Because reservation charges the shared governor ledger, a leader that
    /// abandons a fan-out mid-negotiation would strand capacity without expiry
    /// reclaim.
    #[test]
    fn expired_reservation_returns_its_running_permit() {
        let oracle = slot_limited_oracle(1);
        let registry = ReservationRegistry::new(Arc::new(OracleSlotManager::new(1, 1)), 2);
        let now = Utc::now();
        let query = QueryId::new(uuid::Uuid::now_v7());
        let leader = NodeId::new(uuid::Uuid::now_v7());
        registry
            .reserve(
                &reserve_request(query, leader, 13, now + ChronoDuration::seconds(2)),
                now,
                Some(worker_capacity(&oracle)),
            )
            .expect("pending reservation");
        assert_eq!(oracle.live_slot_units(), 1);
        let expired = now + ChronoDuration::seconds(3);
        assert_eq!(
            registry.cleanup_expired(expired),
            0,
            "expiry reclaims the abandoned reservation"
        );
        assert_eq!(
            oracle.live_slot_units(),
            0,
            "expiry returns the charged slot units"
        );
        drop(worker_capacity(&oracle));
    }

    /// Worker execution streams release slot units on completion, cancel, and drop.
    #[tokio::test]
    async fn worker_execution_stream_releases_running_capacity() {
        let oracle = slot_limited_oracle(1);
        let quantum = |oracle: &crate::resources::OracleResources| {
            FollowerWorkerResources::Oracle(
                oracle
                    .try_acquire_worker(crate::resources::OracleWorkerClass::Interactive)
                    .expect("one follower quantum"),
            )
        };

        let completion_quantum = quantum(&oracle);
        let completion_stream = async_stream::stream! {
            let _running = RunningReservation {
                query_class: QueryClass::Interactive,
                worker_resources: Some(completion_quantum),
            };
            if false {
                yield Err(DispatchError::Unavailable);
            }
        };
        let mut completion = WorkerExecution {
            stream: Box::pin(completion_stream),
        };
        assert_eq!(oracle.live_slot_units(), 1);
        assert!(completion.stream.next().await.is_none());
        assert_eq!(oracle.live_slot_units(), 0);

        let cancellation = CancellationToken::new();
        let cancellation_quantum = quantum(&oracle);
        let observed = cancellation.clone();
        let cancellation_stream = async_stream::stream! {
            let _running = RunningReservation {
                query_class: QueryClass::Interactive,
                worker_resources: Some(cancellation_quantum),
            };
            observed.cancelled().await;
        };
        let mut cancellation_stream = Box::pin(cancellation_stream);
        let waiter = tokio::spawn(async move { cancellation_stream.next().await });
        tokio::task::yield_now().await;
        assert_eq!(oracle.live_slot_units(), 1);
        cancellation.cancel();
        assert!(waiter.await.expect("cancellation stream joins").is_none());
        assert_eq!(oracle.live_slot_units(), 0);

        let drop_quantum = quantum(&oracle);
        let drop_stream = async_stream::stream! {
            let _running = RunningReservation {
                query_class: QueryClass::Interactive,
                worker_resources: Some(drop_quantum),
            };
            futures_util::future::pending::<()>().await;
            yield Err(DispatchError::Unavailable);
        };
        let execution = WorkerExecution {
            stream: Box::pin(drop_stream),
        };
        assert_eq!(oracle.live_slot_units(), 1);
        assert!(
            oracle
                .try_acquire_worker(crate::resources::OracleWorkerClass::Interactive)
                .is_err()
        );
        drop(execution);
        assert_eq!(oracle.live_slot_units(), 0);
        drop(quantum(&oracle));
    }

    /// An ambiguous reserve failure is terminal to the selected candidate sequence.
    #[tokio::test]
    async fn oracle_dispatch_does_not_retry_ambiguous_reserve() {
        let leader = NodeId::new(uuid::Uuid::from_u128(1));
        let transport = Arc::new(AmbiguousReserveTransport {
            reserve_calls: AtomicUsize::new(0),
        });
        let dispatcher = FragmentDispatcher::new(
            Arc::new(DeterministicTestSigner {
                key_id: "test".to_owned(),
            }),
            Arc::new(OraclePeerTransportDirectory::new_for_test(
                leader,
                transport.clone(),
                transport.clone(),
            )),
        );
        let first = NodeId::new(uuid::Uuid::from_u128(2));
        let second = NodeId::new(uuid::Uuid::from_u128(3));
        let fragment = physical_dispatch_fragment("fragment");
        let context = DispatchContext {
            query_id: QueryId::new(uuid::Uuid::now_v7()),
            leader_node_id: leader,
            leader_fence: 1,
            tenant_id: uuid::Uuid::now_v7(),
            query_class: QueryClass::Interactive,
            slot_units: 1,
            permission_digest: "permission".to_owned(),
            attempt_bytes: 1_024,
            attempt_memory_bytes: 1_024,
            query_memory_pool: Arc::new(GreedyMemoryPool::new(1_024)),
            granted_memory_bytes: 1_024,
            admitted_target_partitions: 1,
            cancellation: CancellationToken::new(),
            deadline: Instant::now() + std::time::Duration::from_secs(5),
        };
        let error = dispatcher
            .execute(
                &context,
                fragment,
                &[
                    DispatchCandidate {
                        node_id: first,
                        role: ClusterRole::Oracle,
                        worker_fence: 2,
                        endpoint: None,
                    },
                    DispatchCandidate {
                        node_id: second,
                        role: ClusterRole::Oracle,
                        worker_fence: 3,
                        endpoint: None,
                    },
                ],
            )
            .await
            .expect_err("ambiguous first reserve is terminal");
        assert!(matches!(error, DispatchError::Unavailable));
        assert_eq!(transport.reserve_calls.load(Ordering::SeqCst), 1);
    }

    /// A post-reserve ticket-mint failure releases pending capacity before returning.
    #[tokio::test]
    async fn oracle_dispatch_releases_pending_when_ticket_mint_fails() {
        let leader = NodeId::new(uuid::Uuid::from_u128(11));
        let transport = Arc::new(PendingCleanupTransport {
            release_calls: AtomicUsize::new(0),
            execute_calls: AtomicUsize::new(0),
        });
        let dispatcher = FragmentDispatcher::new(
            Arc::new(FailingTicketMinter),
            Arc::new(OraclePeerTransportDirectory::new_for_test(
                leader,
                transport.clone(),
                transport.clone(),
            )),
        );
        let fragment = physical_dispatch_fragment("fragment");
        let context = DispatchContext {
            query_id: QueryId::new(uuid::Uuid::now_v7()),
            leader_node_id: leader,
            leader_fence: 4,
            tenant_id: uuid::Uuid::now_v7(),
            query_class: QueryClass::Interactive,
            slot_units: 1,
            permission_digest: "permission".to_owned(),
            attempt_bytes: 1_024,
            attempt_memory_bytes: 1_024,
            query_memory_pool: Arc::new(GreedyMemoryPool::new(1_024)),
            granted_memory_bytes: 1_024,
            admitted_target_partitions: 1,
            cancellation: CancellationToken::new(),
            deadline: Instant::now() + std::time::Duration::from_secs(5),
        };

        let error = dispatcher
            .execute(
                &context,
                fragment,
                &[DispatchCandidate {
                    node_id: leader,
                    role: ClusterRole::Oracle,
                    worker_fence: 5,
                    endpoint: None,
                }],
            )
            .await
            .expect_err("mint failure is terminal");

        // A mint failure is request *setup*, not a rejection by the peer, and the
        // ported design degrades setup failures rather than failing the query:
        // when it cannot build the authenticated client for a node it substitutes
        // an empty stream and records a partial error, reserving a terminal for a
        // peer that answered and refused. Classifying this as terminal would fail
        // whole queries over one node's transient credential problem.
        assert!(matches!(
            error,
            DispatchError::Partial {
                attempt: None,
                reason: DispatchPartialReason::Setup,
            }
        ));
        // The reservation must still be surrendered, and no fragment may be sent
        // to a peer whose request was never successfully signed.
        assert_eq!(transport.release_calls.load(Ordering::SeqCst), 1);
        assert_eq!(transport.execute_calls.load(Ordering::SeqCst), 0);
    }

    /// A peer that never emits a frame is bounded by the admitted deadline and released.
    #[tokio::test]
    async fn stalled_peer_honors_deadline_and_releases_slot() {
        let leader = NodeId::new(uuid::Uuid::from_u128(21));
        let transport = Arc::new(StalledExecuteTransport {
            release_calls: AtomicUsize::new(0),
        });
        let dispatcher = FragmentDispatcher::new(
            Arc::new(DeterministicTestSigner {
                key_id: "test".to_owned(),
            }),
            Arc::new(OraclePeerTransportDirectory::new_for_test(
                leader,
                transport.clone(),
                transport.clone(),
            )),
        );
        let fragment = physical_dispatch_fragment("stalled");
        let context = DispatchContext {
            query_id: QueryId::new(uuid::Uuid::now_v7()),
            leader_node_id: leader,
            leader_fence: 1,
            tenant_id: uuid::Uuid::now_v7(),
            query_class: QueryClass::Interactive,
            slot_units: 1,
            permission_digest: "permission".to_owned(),
            attempt_bytes: 1_024,
            attempt_memory_bytes: 1_024,
            query_memory_pool: Arc::new(GreedyMemoryPool::new(1_024)),
            granted_memory_bytes: 1_024,
            admitted_target_partitions: 1,
            cancellation: CancellationToken::new(),
            deadline: Instant::now() + std::time::Duration::from_millis(10),
        };

        let error = dispatcher
            .execute(
                &context,
                fragment,
                &[DispatchCandidate {
                    node_id: leader,
                    role: ClusterRole::Oracle,
                    worker_fence: 1,
                    endpoint: None,
                }],
            )
            .await
            .expect_err("stalled peer times out");
        assert!(matches!(
            error,
            DispatchError::Partial { attempt: None, .. }
        ));
        assert_eq!(transport.release_calls.load(Ordering::SeqCst), 1);
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

    /// Parent-memory pressure remains distinct from a malformed or oversized attempt.
    #[test]
    fn oracle_attempt_capacity_preserves_admission_classification() {
        assert!(matches!(
            attempt_error(AttemptError::ParentCapacity),
            DispatchError::Capacity
        ));
        assert!(matches!(
            attempt_error(AttemptError::Capacity),
            DispatchError::Unavailable
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
}
