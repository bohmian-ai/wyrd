//! Typed peer-context claims and the receiver-side binding checks.
//!
//! The private plane authenticates a trusted cluster process with mTLS. The
//! claims here are unsigned typed context: each receiver compares them with
//! its own trusted state before it decodes a plan or touches tenant storage.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use thiserror::Error;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::BifrostSecurityViolationKind;
use wyrd_spec::vala::api::NodeId;
use wyrd_spec::vala::api::PeerContext;
use wyrd_tonic::prost::Message;

/// Typed context for one worker attempt, checked against receiver state.
#[derive(Clone, PartialEq, Message)]
pub struct PeerTicketClaims {
    /// Fixed private protocol version.
    #[prost(uint32, tag = "1")]
    pub protocol_version: u32,
    /// Worker audience node UUID bytes.
    #[prost(bytes, tag = "2")]
    pub audience: Vec<u8>,
    /// Worker role fence preventing restart replay.
    #[prost(uint64, tag = "3")]
    pub worker_fence: u64,
    /// Leader node UUID bytes used for reservation ownership.
    #[prost(bytes, tag = "4")]
    pub leader_node_id: Vec<u8>,
    /// Leader fence used for reservation ownership.
    #[prost(uint64, tag = "5")]
    pub leader_fence: u64,
    /// Query UUID bytes used for reservation ownership.
    #[prost(bytes, tag = "6")]
    pub query_id: Vec<u8>,
    /// Authenticated data-tenant UUID bytes.
    #[prost(bytes, tag = "7")]
    pub tenant_id: Vec<u8>,
    /// Context acceptance expiry as Unix milliseconds.
    #[prost(int64, tag = "9")]
    pub expires_at_ms: i64,
    /// Tenant-qualified binding.
    #[prost(string, tag = "10")]
    pub binding: String,
    /// Fragment digest.
    #[prost(string, tag = "11")]
    pub fragment_digest: String,
    /// Pinned snapshot or manifest digest.
    #[prost(string, tag = "12")]
    pub manifest_digest: String,
    /// Digest of the authorized projection.
    #[prost(string, tag = "13")]
    pub projection_digest: String,
    /// Digest of the leader-authorized permissions.
    #[prost(string, tag = "14")]
    pub permission_digest: String,
    /// Canonical assignment-authority digest (see
    /// [`wyrd_spec::vala::assignment_authority`]) binding every dispatched
    /// [`wyrd_spec::vala::api::FollowerScanAssignment`]'s identity, files,
    /// schema fingerprint, and closed predicate/projection closure into one
    /// value. The follower recomputes this over its actual received
    /// assignments and rejects any mismatch before resolving a provider or
    /// issuing object I/O.
    #[prost(string, tag = "15")]
    pub assignment_authority_digest: String,
    /// Admitted query execution deadline, independent of context acceptance expiry.
    #[prost(int64, tag = "16")]
    pub execution_deadline_unix_ms: i64,
}

impl PeerTicketClaims {
    /// Encodes these claims as one bounded private-plane context.
    ///
    /// # Errors
    /// Returns [`PeerSecurityError::Encoding`] when the claims exceed
    /// [`MAX_FRAGMENT_CONTEXT_BYTES`].
    pub fn to_context(&self) -> Result<PeerContext, PeerSecurityError> {
        encode_context(self, MAX_FRAGMENT_CONTEXT_BYTES)
    }
}

/// The closed set of private stage operations on the Analytical path.
///
/// Distributed execution has exactly two coordinator-to-follower operations,
/// and they are not interchangeable: `SetPlan` installs a stage's subplan and
/// opens its metrics channel, while `ExecuteTask` asks for a partition range of
/// an already-installed plan. The receiver compares the operation in the
/// context with the entry point it implements, so a context built for one can
/// never authorize the other even if every other bound field matches.
///
/// The enum is deliberately closed. A third operation is a protocol change, not
/// a value a peer may present.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StageOperationV1 {
    /// Install one stage subplan on a follower and open its metrics channel.
    SetPlan,
    /// Execute a partition range of an already-installed stage subplan.
    ExecuteTask,
}

impl StageOperationV1 {
    /// Returns the wire discriminant bound into the context claims.
    ///
    /// Zero is deliberately unused so a zero-valued protobuf field — the value a
    /// truncated or forged message decodes to — never names a real operation.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        match self {
            Self::SetPlan => 1,
            Self::ExecuteTask => 2,
        }
    }

    /// Recovers an operation from its wire discriminant.
    ///
    /// Returns `None` for any other value, including zero, so an unknown
    /// operation is refused at the parsing boundary rather than defaulted.
    #[must_use]
    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::SetPlan),
            2 => Some(Self::ExecuteTask),
            _ => None,
        }
    }

    /// Returns the closed telemetry label for this operation.
    #[must_use]
    pub const fn telemetry(self) -> crate::oracle::telemetry::AnalyticalStageOperation {
        match self {
            Self::SetPlan => crate::oracle::telemetry::AnalyticalStageOperation::SetPlan,
            Self::ExecuteTask => crate::oracle::telemetry::AnalyticalStageOperation::ExecuteTask,
        }
    }
}

/// Typed context for exactly one Analytical stage operation.
///
/// The receiver checks every field
/// against state it derived itself rather than against anything in the
/// presented message. Both query identities appear because they name different
/// lifecycles: a sibling distributed graph under the same public query, or a
/// replayed graph identity under a different public query, must both fail.
#[derive(Clone, PartialEq, Message)]
pub struct StageTicketClaims {
    /// Fixed private protocol version.
    #[prost(uint32, tag = "1")]
    pub protocol_version: u32,
    /// Stage operation discriminant; see [`StageOperationV1::as_u32`].
    #[prost(uint32, tag = "2")]
    pub operation: u32,
    /// Coordinator node UUID bytes that issued this operation.
    #[prost(bytes, tag = "3")]
    pub source_node_id: Vec<u8>,
    /// Coordinator role-incarnation fence at issue time.
    #[prost(uint64, tag = "4")]
    pub source_fence: u64,
    /// Follower node UUID bytes this operation is addressed to.
    #[prost(bytes, tag = "5")]
    pub destination_node_id: Vec<u8>,
    /// Follower role-incarnation fence preventing restart replay.
    #[prost(uint64, tag = "6")]
    pub destination_fence: u64,
    /// Authenticated data-tenant UUID bytes.
    #[prost(bytes, tag = "7")]
    pub tenant_id: Vec<u8>,
    /// Client-visible query UUID bytes owning the complete lifecycle.
    #[prost(bytes, tag = "8")]
    pub public_query_id: Vec<u8>,
    /// Private distributed-graph UUID bytes naming exactly one physical plan.
    #[prost(bytes, tag = "9")]
    pub datafusion_query_id: Vec<u8>,
    /// Pinned catalog snapshot or manifest digest for this attempt's cut.
    #[prost(string, tag = "10")]
    pub snapshot_digest: String,
    /// Digest of the exact bounded raw body this context authorizes.
    #[prost(string, tag = "11")]
    pub body_digest: String,
    /// Graph-local stage identifier, resolvable only under both parents.
    #[prost(uint32, tag = "12")]
    pub stage_id: u32,
    /// Whether [`Self::task_id`] names a task; `SetPlan` carries none.
    #[prost(bool, tag = "13")]
    pub has_task: bool,
    /// Graph-local task identifier, meaningful only when `has_task` is set.
    #[prost(uint32, tag = "14")]
    pub task_id: u32,
    /// Attempt ordinal; zero, or the one permitted pre-egress retry.
    #[prost(uint32, tag = "15")]
    pub attempt: u32,
    /// Follower reservation this operation charges its work against.
    #[prost(string, tag = "16")]
    pub reservation_id: String,
    /// Digest of the leader-authorized permissions for this query.
    #[prost(string, tag = "17")]
    pub permission_digest: String,
    /// Absolute query deadline as Unix milliseconds, identical across attempts.
    #[prost(int64, tag = "19")]
    pub absolute_deadline_ms: i64,
    /// Short context acceptance expiry, distinct from the query deadline.
    #[prost(int64, tag = "20")]
    pub expires_at_ms: i64,
    /// The attempt's frozen participant cut, carried in the context.
    ///
    /// The receiver cannot derive this the way it derives every other bound
    /// field, so it is adopted rather than compared: a follower that is itself
    /// a coordinator addresses exactly these destinations and no others. That
    /// is what keeps membership churn from moving an in-flight participant —
    /// the set was frozen by the leader and travels with every
    /// operation, so no node re-reads live membership mid-attempt.
    #[prost(message, repeated, tag = "21")]
    pub participants: Vec<StageParticipantV1>,
}

/// One frozen destination a stage context authorizes its holder to address.
///
/// Carried inside the context so a follower acting as a coordinator
/// inherits the leader's cut verbatim. The fence is part of the identity: a
/// destination that restarts under a new fence is a different incarnation and
/// is not in this attempt's cut, which is what makes a frozen destination fail
/// rather than silently redirect to its replacement.
#[derive(Clone, PartialEq, Message)]
pub struct StageParticipantV1 {
    /// Participant node UUID bytes.
    #[prost(bytes, tag = "1")]
    pub node_id: Vec<u8>,
    /// Role-incarnation fence this participant was frozen at.
    #[prost(uint64, tag = "2")]
    pub fence: u64,
    /// Private peer endpoint the participant advertised at freeze time.
    #[prost(string, tag = "3")]
    pub address: String,
    /// Reservation the leader took on this participant for the whole graph.
    ///
    /// Carried with the rest of the cut, so a coordinator can charge a follower
    /// only against the reservation that follower's own leader granted. It
    /// travels per participant rather than per context because each node grants
    /// its own reservation, and a follower that becomes a coordinator must
    /// address its peers under their reservations, not its own.
    #[prost(string, tag = "4")]
    pub reservation_id: String,
}

/// Hard cap on the participants one stage context may carry.
///
/// A cut larger than this is not a Bifrost topology, and the cap is checked
/// before the list is adopted so a forged claim cannot grow a follower's
/// destination table without bound.
pub const MAX_STAGE_PARTICIPANTS: usize = 64;

/// The receiver-derived expectation one stage operation must match exactly.
///
/// Nothing here comes from the presented message. The follower assembles it
/// from its own node identity and fence, the tenant its transport
/// authenticated, the graph it has an authorized reservation for, and the exact
/// bytes it received. Verification is then a field-by-field comparison against
/// the context claims, which is why a mismatch in any single identity is
/// independently rejectable.
#[derive(Debug, Clone)]
pub struct StageBinding {
    /// The operation the receiving entry point implements.
    pub operation: StageOperationV1,
    /// Coordinator node the receiver expects to be talking to.
    pub source_node_id: NodeId,
    /// Coordinator fence the receiver expects.
    pub source_fence: u64,
    /// This follower's own node identity.
    pub destination_node_id: NodeId,
    /// This follower's own current role fence.
    pub destination_fence: u64,
    /// The tenant the transport authenticated.
    pub tenant_id: DataTenantId,
    /// The client-visible query identity carried on the operation.
    pub public_query_id: uuid::Uuid,
    /// The private graph identity carried on the operation.
    pub datafusion_query_id: uuid::Uuid,
    /// The pinned snapshot digest the follower resolved for this cut.
    pub snapshot_digest: String,
    /// Graph-local stage identifier being addressed.
    pub stage_id: u32,
    /// Graph-local task identifier, absent only for a stage-scoped operation
    /// that addresses no single task.
    pub task_id: Option<u32>,
    /// Attempt ordinal being addressed.
    pub attempt: u32,
    /// Reservation the follower resolved for this graph.
    pub reservation_id: String,
    /// Permission digest the follower resolved for this query.
    pub permission_digest: String,
}

/// Hard cap on a stage operation's raw body before any digest or decode.
///
/// A distributed subplan is protobuf, not user data, and a coordinator that
/// needs more than this is misbehaving. The cap is checked before the digest is
/// computed, so an oversized body is refused without hashing attacker-chosen
/// bytes of unbounded length.
pub const MAX_STAGE_BODY_BYTES: usize = 8 * 1024 * 1024;

/// Computes the canonical digest of one stage operation's raw body.
///
/// Both the coordinator (at mint time) and the follower (at verification time)
/// call this over the same bytes, so a matching digest proves the follower is
/// about to decode exactly the message the coordinator described.
///
/// # Errors
///
/// Returns [`PeerSecurityError::Body`] when the body is empty or exceeds
/// [`MAX_STAGE_BODY_BYTES`], before any hashing occurs.
pub fn stage_body_digest(body: &[u8]) -> Result<String, PeerSecurityError> {
    if body.is_empty() || body.len() > MAX_STAGE_BODY_BYTES {
        return Err(PeerSecurityError::Body);
    }
    let mut hash = Sha256::new();
    hash.update(b"wyrd.oracle.stage.body.v1\0");
    hash.update((body.len() as u64).to_be_bytes());
    hash.update(body);
    Ok(hex::encode(hash.finalize()))
}

impl StageTicketClaims {
    /// Builds context claims from a receiver-shaped binding.
    ///
    /// The coordinator constructs the same [`StageBinding`] the follower will
    /// derive, so both sides agree by construction on which fields are bound
    /// rather than by two hand-maintained field lists that can drift apart.
    #[must_use]
    pub fn for_binding(
        binding: &StageBinding,
        body_digest: String,
        absolute_deadline_ms: i64,
        expires_at_ms: i64,
        participants: Vec<StageParticipantV1>,
    ) -> Self {
        Self {
            protocol_version: STAGE_PROTOCOL_VERSION,
            operation: binding.operation.as_u32(),
            source_node_id: audience_bytes(binding.source_node_id),
            source_fence: binding.source_fence,
            destination_node_id: audience_bytes(binding.destination_node_id),
            destination_fence: binding.destination_fence,
            tenant_id: binding.tenant_id.as_uuid().as_bytes().to_vec(),
            public_query_id: binding.public_query_id.as_bytes().to_vec(),
            datafusion_query_id: binding.datafusion_query_id.as_bytes().to_vec(),
            snapshot_digest: binding.snapshot_digest.clone(),
            body_digest,
            stage_id: binding.stage_id,
            has_task: binding.task_id.is_some(),
            task_id: binding.task_id.unwrap_or_default(),
            attempt: binding.attempt,
            reservation_id: binding.reservation_id.clone(),
            permission_digest: binding.permission_digest.clone(),
            absolute_deadline_ms,
            expires_at_ms,
            participants,
        }
    }

    /// Encodes these claims as one bounded context for `operation`.
    ///
    /// # Errors
    /// Returns [`PeerSecurityError::Operation`] when the claims name a
    /// different operation and [`PeerSecurityError::Encoding`] when they exceed
    /// [`MAX_STAGE_CONTEXT_BYTES`].
    pub fn to_context(
        &self,
        operation: StageOperationV1,
    ) -> Result<PeerContext, PeerSecurityError> {
        if StageOperationV1::from_u32(self.operation) != Some(operation) {
            return Err(PeerSecurityError::Operation);
        }
        encode_context(self, MAX_STAGE_CONTEXT_BYTES)
    }

    /// Checks every bound field against the receiver's own expectation.
    ///
    /// This is a pure comparison with no IO, no cache access, and no plan
    /// decoding, so an authority runs it before anything is decoded. Each mismatch is independently reachable, which is
    /// what lets the authority test reject one identity at a time.
    ///
    /// # Errors
    ///
    /// Returns [`PeerSecurityError::Operation`] for a protocol-version or
    /// operation mismatch, [`PeerSecurityError::Audience`] for the wrong
    /// coordinator or follower node, [`PeerSecurityError::Fence`] for a stale
    /// fence on either side, [`PeerSecurityError::Body`] for a body digest that
    /// does not match the presented bytes, and [`PeerSecurityError::Claims`] for
    /// any other bound mismatch — tenant, either query identity, snapshot,
    /// stage, task, attempt, reservation, or permission digest.
    pub fn verify_binding(
        &self,
        binding: &StageBinding,
        body_digest: &str,
    ) -> Result<(), PeerSecurityError> {
        if self.protocol_version != STAGE_PROTOCOL_VERSION
            || StageOperationV1::from_u32(self.operation) != Some(binding.operation)
        {
            return Err(PeerSecurityError::Operation);
        }
        if self.source_node_id != audience_bytes(binding.source_node_id)
            || self.destination_node_id != audience_bytes(binding.destination_node_id)
        {
            return Err(PeerSecurityError::Audience);
        }
        if self.source_fence != binding.source_fence
            || self.destination_fence != binding.destination_fence
        {
            return Err(PeerSecurityError::Fence);
        }
        if self.body_digest != body_digest {
            return Err(PeerSecurityError::Body);
        }
        // The cut is adopted, not derived, so its only receiver-side check is
        // that it is small enough to hold. Bounding it here keeps an oversized
        // claim from ever reaching the egress owner that records it.
        if self.participants.len() > MAX_STAGE_PARTICIPANTS {
            return Err(PeerSecurityError::Claims);
        }
        let task_matches = match binding.task_id {
            Some(task_id) => self.has_task && self.task_id == task_id,
            None => !self.has_task,
        };
        if self.tenant_id != binding.tenant_id.as_uuid().as_bytes()
            || self.public_query_id != binding.public_query_id.as_bytes()
            || self.datafusion_query_id != binding.datafusion_query_id.as_bytes()
            || self.snapshot_digest != binding.snapshot_digest
            || self.stage_id != binding.stage_id
            || !task_matches
            || self.attempt != binding.attempt
            || self.reservation_id != binding.reservation_id
            || self.permission_digest != binding.permission_digest
        {
            return Err(PeerSecurityError::Claims);
        }
        Ok(())
    }
}

/// The closed set of private reservation operations on the peer plane.
///
/// A leader opens one held grant on a follower and releases it by closing that
/// stream, so there is no release operation to replay. The receiver still
/// compares the operation in the context with its own entry point, exactly as
/// the two stage operations do, so a context minted for anything else is
/// refused.
///
/// The enum is deliberately closed. A second reservation operation is a
/// protocol change, not a value a peer may present.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReservationOperationV1 {
    /// Open one held grant on one follower for one graph.
    ReserveSlots,
}

impl ReservationOperationV1 {
    /// Returns the wire discriminant bound into the context claims.
    ///
    /// Zero is deliberately unused so a zero-valued protobuf field — the value
    /// a truncated or forged message decodes to — never names a real operation.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        match self {
            Self::ReserveSlots => 1,
        }
    }

    /// Recovers an operation from its wire discriminant.
    ///
    /// Returns `None` for any other value, including zero and the retired
    /// release discriminant, so an unknown operation is refused at the parsing
    /// boundary rather than defaulted.
    #[must_use]
    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::ReserveSlots),
            _ => None,
        }
    }
}

/// Typed context for exactly one reservation operation.
///
/// Every field is checked against state the receiver
/// derived itself. The follower's own node identity and fence appear because a
/// reservation is charged against one incarnation of one node: a context built
/// for a follower that has since restarted must not be honoured by its
/// successor.
#[derive(Clone, PartialEq, Message)]
pub struct ReservationTicketClaims {
    /// Fixed private protocol version.
    #[prost(uint32, tag = "1")]
    pub protocol_version: u32,
    /// Reservation operation discriminant; see [`ReservationOperationV1::as_u32`].
    #[prost(uint32, tag = "2")]
    pub operation: u32,
    /// Leader node UUID bytes that issued this operation.
    #[prost(bytes, tag = "3")]
    pub source_node_id: Vec<u8>,
    /// Leader role-incarnation fence at issue time.
    #[prost(uint64, tag = "4")]
    pub source_fence: u64,
    /// Follower node UUID bytes this operation is addressed to.
    #[prost(bytes, tag = "5")]
    pub destination_node_id: Vec<u8>,
    /// Follower role-incarnation fence preventing restart replay.
    #[prost(uint64, tag = "6")]
    pub destination_fence: u64,
    /// Client-visible query UUID bytes owning this reservation.
    #[prost(bytes, tag = "7")]
    pub query_id: Vec<u8>,
    /// Digest of the exact bounded raw request body this context authorizes.
    #[prost(string, tag = "8")]
    pub body_digest: String,
    /// Short context acceptance expiry.
    #[prost(int64, tag = "10")]
    pub expires_at_ms: i64,
}

/// The receiver-derived expectation one reservation operation must match.
///
/// Nothing here comes from the presented message: the follower assembles it
/// from its own identity and fence, the leader identity the request names and
/// the cluster confirms live, and the exact bytes it received.
#[derive(Debug, Clone)]
pub struct ReservationBinding {
    /// The operation the receiving entry point implements.
    pub operation: ReservationOperationV1,
    /// Leader node the receiver expects to be talking to.
    pub source_node_id: NodeId,
    /// Leader fence the receiver expects.
    pub source_fence: u64,
    /// This follower's own node identity.
    pub destination_node_id: NodeId,
    /// This follower's own current role fence.
    pub destination_fence: u64,
    /// The client-visible query identity carried on the operation.
    pub query_id: uuid::Uuid,
}

/// Hard cap on a reservation request's raw body before any digest or decode.
///
/// A reservation request carries a handful of identifiers and no user data, so
/// this bound is generous by orders of magnitude; it exists so an oversized
/// body is refused without hashing attacker-chosen bytes of unbounded length.
pub const MAX_RESERVATION_BODY_BYTES: usize = 64 * 1024;

/// Computes the canonical digest of one reservation request's raw body.
///
/// Both the leader and the follower call this over the encoded request with
/// its context field cleared, so a matching digest proves the follower is
/// acting on exactly the request the leader described.
///
/// # Errors
///
/// Returns [`PeerSecurityError::Body`] when the body is empty or exceeds
/// [`MAX_RESERVATION_BODY_BYTES`], before any hashing occurs.
pub fn reservation_body_digest(body: &[u8]) -> Result<String, PeerSecurityError> {
    if body.is_empty() || body.len() > MAX_RESERVATION_BODY_BYTES {
        return Err(PeerSecurityError::Body);
    }
    let mut hash = Sha256::new();
    hash.update(b"wyrd.oracle.peer.reservation.body.v1\0");
    hash.update((body.len() as u64).to_be_bytes());
    hash.update(body);
    Ok(hex::encode(hash.finalize()))
}

impl ReservationTicketClaims {
    /// Builds context claims from a receiver-shaped binding.
    ///
    /// The leader constructs the same [`ReservationBinding`] the follower will
    /// derive, so both sides agree by construction on which fields are bound
    /// rather than through two hand-maintained field lists that can drift.
    #[must_use]
    pub fn for_binding(
        binding: &ReservationBinding,
        body_digest: String,
        expires_at_ms: i64,
    ) -> Self {
        Self {
            protocol_version: STAGE_PROTOCOL_VERSION,
            operation: binding.operation.as_u32(),
            source_node_id: audience_bytes(binding.source_node_id),
            source_fence: binding.source_fence,
            destination_node_id: audience_bytes(binding.destination_node_id),
            destination_fence: binding.destination_fence,
            query_id: binding.query_id.as_bytes().to_vec(),
            body_digest,
            expires_at_ms,
        }
    }

    /// Encodes these claims as one bounded context for `operation`.
    ///
    /// # Errors
    /// Returns [`PeerSecurityError::Operation`] when the claims name a
    /// different operation and [`PeerSecurityError::Encoding`] when they exceed
    /// [`MAX_RESERVATION_CONTEXT_BYTES`].
    pub fn to_context(
        &self,
        operation: ReservationOperationV1,
    ) -> Result<PeerContext, PeerSecurityError> {
        if ReservationOperationV1::from_u32(self.operation) != Some(operation) {
            return Err(PeerSecurityError::Operation);
        }
        encode_context(self, MAX_RESERVATION_CONTEXT_BYTES)
    }

    /// Checks every bound field against the receiver's own expectation.
    ///
    /// A pure comparison with no IO and no request decoding, so an authority
    /// runs it before any reservation state changes.
    ///
    /// # Errors
    ///
    /// Returns [`PeerSecurityError::Operation`] for a protocol-version or
    /// operation mismatch, [`PeerSecurityError::Audience`] for the wrong leader
    /// or follower node, [`PeerSecurityError::Fence`] for a stale fence on
    /// either side, [`PeerSecurityError::Body`] for a body digest that does not
    /// match the presented bytes, and [`PeerSecurityError::Claims`] for a
    /// query identity mismatch.
    pub fn verify_binding(
        &self,
        binding: &ReservationBinding,
        body_digest: &str,
    ) -> Result<(), PeerSecurityError> {
        if self.protocol_version != STAGE_PROTOCOL_VERSION
            || ReservationOperationV1::from_u32(self.operation) != Some(binding.operation)
        {
            return Err(PeerSecurityError::Operation);
        }
        if self.source_node_id != audience_bytes(binding.source_node_id)
            || self.destination_node_id != audience_bytes(binding.destination_node_id)
        {
            return Err(PeerSecurityError::Audience);
        }
        if self.source_fence != binding.source_fence
            || self.destination_fence != binding.destination_fence
        {
            return Err(PeerSecurityError::Fence);
        }
        if self.body_digest != body_digest {
            return Err(PeerSecurityError::Body);
        }
        if self.query_id != binding.query_id.as_bytes() {
            return Err(PeerSecurityError::Claims);
        }
        Ok(())
    }
}

/// Hard cap on fragment context bytes, checked before decoding.
pub const MAX_FRAGMENT_CONTEXT_BYTES: usize = 16 * 1024;
/// Hard cap on reservation context bytes; a reservation binds two fenced node
/// identities, one query, and a body digest, so it is the narrowest shape.
pub const MAX_RESERVATION_CONTEXT_BYTES: usize = 8 * 1024;
/// Hard cap on stage context bytes; stage claims bind two query identities, a
/// stage, a task, an attempt, a reservation, and a participant cut.
pub const MAX_STAGE_CONTEXT_BYTES: usize = 32 * 1024;

/// Encodes one typed claims message as a bounded private-plane context.
///
/// # Errors
/// Returns [`PeerSecurityError::Encoding`] when encoding fails or the encoded
/// claims are empty or exceed `max_bytes`.
fn encode_context<M: Message>(
    claims: &M,
    max_bytes: usize,
) -> Result<PeerContext, PeerSecurityError> {
    let mut claims_bytes = Vec::new();
    claims
        .encode(&mut claims_bytes)
        .map_err(|_| PeerSecurityError::Encoding)?;
    if claims_bytes.is_empty() || claims_bytes.len() > max_bytes {
        return Err(PeerSecurityError::Encoding);
    }
    Ok(PeerContext { claims_bytes })
}

/// Fixed private stage-protocol version bound into every stage and grant context.
///
/// Version 2 makes `ReserveSlots` a held server stream and removes
/// `ReleaseSlots`: a follower's graph grant, and every stage bound to it,
/// lives exactly as long as its leader's stream. A version-1 leader expects a
/// unary reserve and an explicit release, so its contexts are refused rather
/// than admitted into grants nothing would release.
pub const STAGE_PROTOCOL_VERSION: u32 = 2;

/// Claims bytes accepted after bound, audience, and fence checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedClaimsBytes(pub Vec<u8>);

/// Peer-context validation failure.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PeerSecurityError {
    /// Context is missing, oversized, or does not decode.
    #[error("peer context is malformed")]
    Malformed,
    /// Claims are not addressed to the expected worker.
    #[error("peer context audience is invalid")]
    Audience,
    /// Claims use a stale role fence.
    #[error("peer context fence is stale")]
    Fence,
    /// Context is expired or exceeds the configured window.
    #[error("peer context is expired")]
    Expired,
    /// A deterministic claims encoding failed.
    #[error("peer claims encoding failed")]
    Encoding,
    /// Claims do not bind all required attempt identities.
    #[error("peer context claims do not match the attempt")]
    Claims,
    /// The context names a different operation than the one presented.
    #[error("peer context names a different operation")]
    Operation,
    /// The presented raw body does not match the context digest or exceeds bounds.
    #[error("peer body does not match its context digest")]
    Body,
}

/// Failure composing the peer-security audit collaborator.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
#[error("peer security audit cannot be composed")]
pub struct PeerSecurityAuditError;

/// Server-owned, non-blocking audit capability for rejected peer requests.
///
/// Both methods only stage the rejection and return; the refusal never waits
/// for, or depends on, the audit commit.
pub trait PeerSecurityAudit: Send + Sync {
    /// Stages a rejection whose claims cannot select an audit tenant.
    fn stage_unverified_ticket_rejection(&self, violation: BifrostSecurityViolationKind);

    /// Stages a violation on the context-named tenant's audit chain.
    fn stage_verified_ticket_violation(
        &self,
        tenant_id: DataTenantId,
        violation: BifrostSecurityViolationKind,
    );
}

/// Narrow context verification capability implemented by the server authority.
#[async_trait]
pub trait PeerTicketVerifier: Send + Sync {
    /// Checks raw context bytes against receiver state before decoding work.
    ///
    /// # Errors
    /// Returns a closed peer-security failure without exposing key material.
    async fn verify_peer_ticket(
        &self,
        context: &PeerContext,
        expected_worker: NodeId,
        expected_worker_fence: u64,
        now: DateTime<Utc>,
    ) -> Result<VerifiedClaimsBytes, PeerSecurityError>;
}

/// Encodes a node identity for claims audience binding.
#[must_use]
pub fn audience_bytes(node: NodeId) -> Vec<u8> {
    node.as_uuid().as_bytes().to_vec()
}

/// Computes the canonical digest of an ordered authorized projection.
#[must_use]
pub fn projection_digest(projection: &[String]) -> String {
    let mut hash = Sha256::new();
    hash.update(b"wyrd.oracle.projection.v1\0");
    for column in projection {
        hash.update(column.as_bytes());
        hash.update([0]);
    }
    hex::encode(hash.finalize())
}

/// Recomputes the canonical assignment-authority digest for one follower's
/// full set of dispatched scan assignments.
///
/// Both the leader and the follower call this over the same list, in the same
/// order, so a matching digest proves the follower's actual assignments are
/// exactly the ones the leader described — including every file, schema
/// fingerprint, and closed predicate.
///
/// # Errors
/// Returns [`PeerSecurityError::Encoding`] when a schema fingerprint is not
/// canonical 64-hex, or a field exceeds the digest's length domain.
pub fn assignment_authority_digest_for(
    assignments: &[wyrd_spec::vala::api::FollowerScanAssignment],
) -> Result<String, PeerSecurityError> {
    let inputs = assignments
        .iter()
        .map(
            |assignment| wyrd_spec::vala::assignment_authority::AssignmentDigestInput {
                scan_id: assignment.scan_id.as_str(),
                tenant_uuid: assignment.binding.tenant_id.as_uuid(),
                namespace: assignment.binding.namespace.as_str(),
                table: assignment.binding.table.as_str(),
                schema_fingerprint_hex: assignment.schema_fingerprint.as_str(),
                files: &assignment.persisted.files,
                scribe_cut: assignment.scribe_provider_cut.as_ref(),
                required_columns: &assignment.required_columns,
                predicates: &assignment.predicates,
                reader_cut: &assignment.reader_cut,
            },
        )
        .collect::<Vec<_>>();
    wyrd_spec::vala::assignment_authority::assignment_authority_digest(&inputs)
        .map_err(|_| PeerSecurityError::Encoding)
}

/// One stage operation that passed every authority check.
///
/// Holding this value is the receiver's proof that it may now decode the
/// operation's body, touch the task cache, construct providers, and issue I/O.
/// Nothing downstream re-derives the tenant: it is the checked context tenant,
/// carried here so a handler cannot accidentally resolve tenancy from an
/// unchecked field.
#[derive(Debug, Clone)]
pub struct AuthorizedStage {
    /// The verified claims, already matched field-by-field to the receiver's
    /// own [`StageBinding`].
    pub claims: StageTicketClaims,
    /// The tenant the checked context names.
    pub tenant_id: DataTenantId,
}

/// The server-owned authority every Analytical stage operation passes through.
///
/// The contract lives here, next to the claims and binding it operates on, so
/// the Oracle follower ingress can require authorization without depending on
/// the server crate that owns the security audit. The server
/// implements it on its existing peer authority.
#[async_trait]
pub trait OracleStageAuthority: Send + Sync {
    /// Authorizes one stage operation before its body may be decoded or used.
    ///
    /// Holding the returned [`AuthorizedStage`] is the receiver's proof that
    /// every check ran: bounds, exact body digest, field-by-field binding,
    /// deadline, and expiry. A caller that decodes, reads a cache, constructs a
    /// provider, or issues I/O before this returns has broken the contract.
    ///
    /// # Errors
    ///
    /// Returns the closed [`PeerSecurityError`] for the first failed check. A
    /// rejection never returns claims.
    async fn authorize_stage(
        &self,
        context: &PeerContext,
        binding: &StageBinding,
        body: &[u8],
        now: DateTime<Utc>,
    ) -> Result<AuthorizedStage, PeerSecurityError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one stage binding whose every field is distinguishable.
    fn stage_binding() -> StageBinding {
        StageBinding {
            operation: StageOperationV1::ExecuteTask,
            source_node_id: NodeId::new(uuid::Uuid::from_u128(1)),
            source_fence: 11,
            destination_node_id: NodeId::new(uuid::Uuid::from_u128(2)),
            destination_fence: 22,
            tenant_id: DataTenantId::new(uuid::Uuid::now_v7()).expect("a UUIDv7 fixture tenant"),
            public_query_id: uuid::Uuid::from_u128(4),
            datafusion_query_id: uuid::Uuid::from_u128(5),
            snapshot_digest: "snapshot".to_owned(),
            stage_id: 6,
            task_id: Some(7),
            attempt: 0,
            reservation_id: "reservation".to_owned(),
            permission_digest: "permission".to_owned(),
        }
    }

    /// One named single-field mutation and the closed error it must produce.
    type StageMutation = (
        &'static str,
        Box<dyn Fn(&mut StageTicketClaims)>,
        PeerSecurityError,
    );

    /// Enumerates one mutation per bound claims field.
    ///
    /// Kept out of the assertion loop so binding a new field is a one-line
    /// addition here rather than an edit inside a long test body.
    fn stage_field_mutations() -> Vec<StageMutation> {
        vec![
            (
                "protocol",
                Box::new(|c: &mut StageTicketClaims| c.protocol_version += 1),
                PeerSecurityError::Operation,
            ),
            (
                "operation",
                Box::new(|c: &mut StageTicketClaims| {
                    c.operation = StageOperationV1::SetPlan.as_u32();
                }),
                PeerSecurityError::Operation,
            ),
            (
                "source node",
                Box::new(|c: &mut StageTicketClaims| c.source_node_id[0] ^= 1),
                PeerSecurityError::Audience,
            ),
            (
                "destination node",
                Box::new(|c: &mut StageTicketClaims| c.destination_node_id[0] ^= 1),
                PeerSecurityError::Audience,
            ),
            (
                "source fence",
                Box::new(|c: &mut StageTicketClaims| c.source_fence += 1),
                PeerSecurityError::Fence,
            ),
            (
                "destination fence",
                Box::new(|c: &mut StageTicketClaims| c.destination_fence += 1),
                PeerSecurityError::Fence,
            ),
            (
                "body digest",
                Box::new(|c: &mut StageTicketClaims| c.body_digest.push('0')),
                PeerSecurityError::Body,
            ),
            (
                "tenant",
                Box::new(|c: &mut StageTicketClaims| c.tenant_id[0] ^= 1),
                PeerSecurityError::Claims,
            ),
            (
                "public query",
                Box::new(|c: &mut StageTicketClaims| c.public_query_id[0] ^= 1),
                PeerSecurityError::Claims,
            ),
            (
                "datafusion query",
                Box::new(|c: &mut StageTicketClaims| c.datafusion_query_id[0] ^= 1),
                PeerSecurityError::Claims,
            ),
            (
                "snapshot",
                Box::new(|c: &mut StageTicketClaims| c.snapshot_digest.push('0')),
                PeerSecurityError::Claims,
            ),
            (
                "stage",
                Box::new(|c: &mut StageTicketClaims| c.stage_id += 1),
                PeerSecurityError::Claims,
            ),
            (
                "task",
                Box::new(|c: &mut StageTicketClaims| c.task_id += 1),
                PeerSecurityError::Claims,
            ),
            (
                "task presence",
                Box::new(|c: &mut StageTicketClaims| c.has_task = false),
                PeerSecurityError::Claims,
            ),
            (
                "attempt",
                Box::new(|c: &mut StageTicketClaims| c.attempt += 1),
                PeerSecurityError::Claims,
            ),
            (
                "reservation",
                Box::new(|c: &mut StageTicketClaims| c.reservation_id.push('0')),
                PeerSecurityError::Claims,
            ),
            (
                "permission",
                Box::new(|c: &mut StageTicketClaims| c.permission_digest.push('0')),
                PeerSecurityError::Claims,
            ),
        ]
    }

    /// Every bound field is independently load-bearing.
    ///
    /// Context claims are only as strong as the weakest field the receiver
    /// actually compares, so this walks one mutation per field and asserts each
    /// is refused with its own closed error.
    ///
    /// # Panics
    ///
    /// Panics when any single-field mutation is accepted.
    #[test]
    fn oracle_stage_claims_reject_every_single_field_mutation() {
        let binding = stage_binding();
        let digest = stage_body_digest(b"body").expect("a bounded body digests");
        let claims =
            StageTicketClaims::for_binding(&binding, digest.clone(), 1_000, 2_000, Vec::new());
        claims
            .verify_binding(&binding, &digest)
            .expect("an unmutated binding verifies");

        for (name, mutate, expected) in stage_field_mutations() {
            let mut mutated = claims.clone();
            mutate(&mut mutated);
            assert_eq!(
                mutated.verify_binding(&binding, &digest),
                Err(expected),
                "mutating the {name} field must be refused"
            );
        }
    }

    /// A `SetPlan` context cannot authorize an `ExecuteTask` operation.
    ///
    /// The operation field is compared against the receiving entry point's own
    /// expectation rather than against anything in the message, and encoding
    /// refuses a context for an operation the claims do not name.
    ///
    /// # Panics
    ///
    /// Panics when a cross-operation context is accepted.
    #[test]
    fn oracle_stage_operations_are_not_interchangeable() {
        assert_eq!(StageOperationV1::from_u32(0), None);
        assert_eq!(StageOperationV1::from_u32(3), None);

        let mut binding = stage_binding();
        binding.operation = StageOperationV1::SetPlan;
        binding.task_id = None;
        let digest = stage_body_digest(b"plan").expect("a bounded body digests");
        let claims =
            StageTicketClaims::for_binding(&binding, digest.clone(), 1_000, 2_000, Vec::new());

        let mut executing = binding.clone();
        executing.operation = StageOperationV1::ExecuteTask;
        assert_eq!(
            claims.verify_binding(&executing, &digest),
            Err(PeerSecurityError::Operation)
        );
        assert_eq!(
            claims.to_context(StageOperationV1::ExecuteTask),
            Err(PeerSecurityError::Operation)
        );
    }

    /// The raw body is bounded before it is hashed.
    ///
    /// # Panics
    ///
    /// Panics when an empty or oversized body produces a digest.
    #[test]
    fn oracle_stage_body_digest_is_bounded_and_length_committed() {
        assert_eq!(stage_body_digest(&[]), Err(PeerSecurityError::Body));
        assert_eq!(
            stage_body_digest(&vec![0_u8; MAX_STAGE_BODY_BYTES + 1]),
            Err(PeerSecurityError::Body)
        );
        assert_ne!(
            stage_body_digest(b"ab").expect("a bounded body digests"),
            stage_body_digest(b"abc").expect("a bounded body digests")
        );
    }

    /// Projection digest is ordered and domain-separated.
    #[test]
    fn oracle_peer_projection_digest_is_order_sensitive() {
        assert_ne!(
            projection_digest(&["a".to_owned(), "b".to_owned()]),
            projection_digest(&["b".to_owned(), "a".to_owned()])
        );
    }
}
