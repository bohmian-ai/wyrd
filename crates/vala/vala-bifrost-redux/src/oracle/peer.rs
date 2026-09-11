//! Narrow peer-ticket authority boundary and replay protection.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;
use thiserror::Error;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::BifrostSecurityViolationKind;
use wyrd_spec::vala::api::NodeId;
use wyrd_spec::vala::api::SignedPeerTicket;
use wyrd_tonic::prost::Message;

/// The verified workload identity behind one admitted private-plane request.
///
/// Produced once per request by the server's peer authentication layer, before
/// the request body is polled, and attached to the request extensions as the
/// only identity input an admitted peer handler may read. Both private
/// adapters — the Oracle peer service and the upstream worker service — take
/// their caller identity from this one value, so neither can grow a second
/// authentication path or a synthetic principal of its own.
///
/// The context is deliberately narrow. It answers "which configured platform
/// service is calling, proved how" and nothing else. A data tenant, a space, a
/// source `NodeId`, and a fence are *operation* authority: they are carried by
/// a purpose ticket and resolved after this context exists, never asserted by
/// the caller alongside its credential.
#[derive(Clone)]
pub struct AuthenticatedPeerContext {
    /// Control-plane tenant the peer principal belongs to; always the owner.
    control_tenant: DataTenantId,
    /// Stable identity of the verified peer Service principal.
    principal_id: wyrd_spec::auth::PrincipalId,
    /// Service card the verified principal is bound to.
    service_card: wyrd_spec::reference::CardRef,
    /// Peer permissions the verified token resolved to.
    permissions: wyrd_runtime::PermissionSet,
    /// Stable digest of the presented workload credential.
    credential_digest: String,
    /// Stable digest of the accepted peer certificate, when the transport
    /// exposed one.
    certificate_digest: Option<String>,
    /// Correlator carried or minted for this request.
    request_id: wyrd_spec::request_id::RequestId,
    /// When the credential was verified.
    authenticated_at: DateTime<Utc>,
}

impl std::fmt::Debug for AuthenticatedPeerContext {
    /// Renders identity without rendering the credential it was proved with.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AuthenticatedPeerContext")
            .field("principal_id", &self.principal_id)
            .field("service_card", &self.service_card)
            .field("request_id", &self.request_id)
            .finish_non_exhaustive()
    }
}

impl AuthenticatedPeerContext {
    /// Builds one context from values the authentication layer has verified.
    ///
    /// Every argument is already proved: the caller must not construct this
    /// from wire-asserted values.
    #[must_use]
    #[expect(
        clippy::too_many_arguments,
        reason = "each field is a distinct verified fact and collapsing them \
                  into a struct literal would only move the same arity"
    )]
    pub fn new(
        control_tenant: DataTenantId,
        principal_id: wyrd_spec::auth::PrincipalId,
        service_card: wyrd_spec::reference::CardRef,
        permissions: wyrd_runtime::PermissionSet,
        credential_digest: String,
        certificate_digest: Option<String>,
        request_id: wyrd_spec::request_id::RequestId,
        authenticated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            control_tenant,
            principal_id,
            service_card,
            permissions,
            credential_digest,
            certificate_digest,
            request_id,
            authenticated_at,
        }
    }

    /// Returns the control-plane tenant this peer principal belongs to.
    #[must_use]
    pub const fn control_tenant(&self) -> DataTenantId {
        self.control_tenant
    }

    /// Returns the verified peer Service principal's stable identity.
    #[must_use]
    pub const fn principal_id(&self) -> wyrd_spec::auth::PrincipalId {
        self.principal_id
    }

    /// Returns the Service card the verified principal is bound to.
    #[must_use]
    pub const fn service_card(&self) -> &wyrd_spec::reference::CardRef {
        &self.service_card
    }

    /// Returns the peer permissions the verified token resolved to.
    #[must_use]
    pub const fn permissions(&self) -> &wyrd_runtime::PermissionSet {
        &self.permissions
    }

    /// Returns the stable digest of the presented workload credential.
    #[must_use]
    pub fn credential_digest(&self) -> &str {
        &self.credential_digest
    }

    /// Returns the accepted peer certificate's digest, when one was exposed.
    #[must_use]
    pub fn certificate_digest(&self) -> Option<&str> {
        self.certificate_digest.as_deref()
    }

    /// Returns the correlator this request is audited and traced under.
    #[must_use]
    pub const fn request_id(&self) -> &wyrd_spec::request_id::RequestId {
        &self.request_id
    }

    /// Returns when the credential behind this request was verified.
    #[must_use]
    pub const fn authenticated_at(&self) -> DateTime<Utc> {
        self.authenticated_at
    }
}

/// Typed claims signed for one worker attempt.
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
    /// Single-use random nonce.
    #[prost(bytes, tag = "8")]
    pub nonce: Vec<u8>,
    /// Ticket acceptance and replay-cache expiry as Unix milliseconds.
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
    /// signed value. The follower recomputes this over its actual received
    /// assignments and rejects any mismatch before resolving a provider or
    /// issuing object I/O.
    #[prost(string, tag = "15")]
    pub assignment_authority_digest: String,
    /// Admitted query execution deadline, independent of ticket acceptance expiry.
    #[prost(int64, tag = "16")]
    pub execution_deadline_unix_ms: i64,
}

impl PeerTicketClaims {
    /// Validates the signed execution deadline without extending ticket acceptance.
    ///
    /// # Errors
    ///
    /// Rejects absent, unrepresentable, or contradictory deadline timestamps.
    pub fn execution_deadline(&self) -> Result<DateTime<Utc>, PeerSecurityError> {
        if self.execution_deadline_unix_ms <= 0
            || self.execution_deadline_unix_ms < self.expires_at_ms
            || DateTime::from_timestamp_millis(self.expires_at_ms).is_none()
        {
            return Err(PeerSecurityError::Claims);
        }
        DateTime::from_timestamp_millis(self.execution_deadline_unix_ms)
            .ok_or(PeerSecurityError::Claims)
    }
}

/// The closed set of private stage operations on the Analytical path.
///
/// Distributed execution has exactly two coordinator-to-follower operations,
/// and they are not interchangeable: `SetPlan` installs a stage's subplan and
/// opens its metrics channel, while `ExecuteTask` asks for a partition range of
/// an already-installed plan. They carry different signing domains and separate
/// single-use nonces, so a ticket minted for one can never authorize the other
/// even if every other bound field matches.
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
    /// Returns the wire discriminant bound into the signed claims.
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

    /// Returns this operation's distinct signature domain separator.
    ///
    /// Domain separation is what makes the two operations cryptographically
    /// distinct: a signature produced over the `SetPlan` domain does not verify
    /// under the `ExecuteTask` domain, so the receiver's own expectation — not
    /// anything in the presented message — selects which domain is checked.
    #[must_use]
    pub const fn domain(self) -> &'static [u8] {
        match self {
            Self::SetPlan => b"wyrd.oracle.stage.set-plan.v1\0",
            Self::ExecuteTask => b"wyrd.oracle.stage.execute-task.v1\0",
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

/// Typed claims signed for exactly one Analytical stage operation.
///
/// Every field is bound by the signature, and the receiver checks each one
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
    /// Digest of the exact bounded raw body this ticket authorizes.
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
    /// Single-use random nonce, distinct per operation.
    #[prost(bytes, tag = "18")]
    pub nonce: Vec<u8>,
    /// Absolute query deadline as Unix milliseconds, identical across attempts.
    #[prost(int64, tag = "19")]
    pub absolute_deadline_ms: i64,
    /// Short ticket acceptance expiry, distinct from the query deadline.
    #[prost(int64, tag = "20")]
    pub expires_at_ms: i64,
    /// The attempt's frozen participant cut, signed as part of the ticket.
    ///
    /// The receiver cannot derive this the way it derives every other bound
    /// field, so it is adopted rather than compared: a follower that is itself
    /// a coordinator addresses exactly these destinations and no others. That
    /// is what keeps membership churn from moving an in-flight participant —
    /// the set was frozen by the leader and travels signed with every
    /// operation, so no node re-reads live membership mid-attempt.
    #[prost(message, repeated, tag = "21")]
    pub participants: Vec<StageParticipantV1>,
}

/// One frozen destination a stage ticket authorizes its holder to address.
///
/// Carried inside the signed claims so a follower acting as a coordinator
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
    /// Signed with the rest of the cut, so a coordinator can charge a follower
    /// only against the reservation that follower's own leader granted. It
    /// travels per participant rather than per ticket because each node grants
    /// its own reservation, and a follower that becomes a coordinator must
    /// address its peers under their reservations, not its own.
    #[prost(string, tag = "4")]
    pub reservation_id: String,
}

/// Hard cap on the participants one stage ticket may carry.
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
/// the signed claims, which is why a mismatch in any single identity is
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
/// about to decode exactly the message the coordinator signed for.
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
    /// Builds signable claims from a receiver-shaped binding.
    ///
    /// The coordinator constructs the same [`StageBinding`] the follower will
    /// derive, so both sides agree by construction on which fields are bound
    /// rather than by two hand-maintained field lists that can drift apart.
    #[must_use]
    pub fn for_binding(
        binding: &StageBinding,
        body_digest: String,
        nonce: Vec<u8>,
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
            nonce,
            absolute_deadline_ms,
            expires_at_ms,
            participants,
        }
    }

    /// Checks every bound field against the receiver's own expectation.
    ///
    /// This is a pure comparison with no IO, no cache access, and no plan
    /// decoding, so an authority can run it between signature verification and
    /// nonce consumption. Each mismatch is independently reachable, which is
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
/// A leader reserves capacity on a follower and later releases it. The two are
/// not interchangeable: a replayed release must never cancel a reservation the
/// leader has since re-taken, and a replayed reserve must never charge a
/// follower twice. They therefore carry different signing domains and separate
/// single-use nonces, exactly as the two stage operations do.
///
/// The enum is deliberately closed. A third reservation operation is a protocol
/// change, not a value a peer may present.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReservationOperationV1 {
    /// Take bounded pending capacity on one follower for one query.
    ReserveSlots,
    /// Release one previously taken reservation on the same follower.
    ReleaseSlots,
}

impl ReservationOperationV1 {
    /// Returns the wire discriminant bound into the signed claims.
    ///
    /// Zero is deliberately unused so a zero-valued protobuf field — the value
    /// a truncated or forged message decodes to — never names a real operation.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        match self {
            Self::ReserveSlots => 1,
            Self::ReleaseSlots => 2,
        }
    }

    /// Recovers an operation from its wire discriminant.
    ///
    /// Returns `None` for any other value, including zero, so an unknown
    /// operation is refused at the parsing boundary rather than defaulted.
    #[must_use]
    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::ReserveSlots),
            2 => Some(Self::ReleaseSlots),
            _ => None,
        }
    }

    /// Returns this operation's distinct signature domain separator.
    ///
    /// Domain separation is what makes the two operations cryptographically
    /// distinct: a signature produced over the reserve domain does not verify
    /// under the release domain, so the receiver's own expectation — not
    /// anything in the presented message — selects which domain is checked.
    #[must_use]
    pub const fn domain(self) -> &'static [u8] {
        match self {
            Self::ReserveSlots => b"wyrd.oracle.peer.reserve-slots.v1\0",
            Self::ReleaseSlots => b"wyrd.oracle.peer.release-slots.v1\0",
        }
    }
}

/// Typed claims signed for exactly one reservation operation.
///
/// Every field is bound by the signature and checked against state the receiver
/// derived itself. The follower's own node identity and fence appear because a
/// reservation is charged against one incarnation of one node: a ticket minted
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
    /// Digest of the exact bounded raw request body this ticket authorizes.
    #[prost(string, tag = "8")]
    pub body_digest: String,
    /// Single-use random nonce, distinct per operation.
    #[prost(bytes, tag = "9")]
    pub nonce: Vec<u8>,
    /// Short ticket acceptance expiry.
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
/// Both the leader (at mint time) and the follower (at verification time) call
/// this over the encoded request with its ticket field cleared, so a matching
/// digest proves the follower is acting on exactly the request the leader
/// signed for and not on a substituted one carrying a valid ticket.
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
    /// Builds signable claims from a receiver-shaped binding.
    ///
    /// The leader constructs the same [`ReservationBinding`] the follower will
    /// derive, so both sides agree by construction on which fields are bound
    /// rather than through two hand-maintained field lists that can drift.
    #[must_use]
    pub fn for_binding(
        binding: &ReservationBinding,
        body_digest: String,
        nonce: Vec<u8>,
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
            nonce,
            expires_at_ms,
        }
    }

    /// Checks every bound field against the receiver's own expectation.
    ///
    /// A pure comparison with no IO and no request decoding, so an authority
    /// runs it between signature verification and nonce consumption.
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

/// Server-owned capability minting one reservation purpose ticket.
///
/// The transport that dials a follower does not own signing material, and the
/// authority that owns it does not own routing. This narrow seam is how a
/// leader stamps an authorization onto a reservation call without the transport
/// holding a key or the authority learning about endpoints.
pub trait ReservationTicketMinter: Send + Sync {
    /// Signs one single-use ticket for exactly one reservation operation.
    ///
    /// # Errors
    ///
    /// Returns [`PeerSecurityError::Operation`] when the claims name a
    /// different operation than the one requested and
    /// [`PeerSecurityError::Encoding`] when the claims cannot be encoded within
    /// their bound.
    fn mint_reservation_ticket(
        &self,
        operation: ReservationOperationV1,
        claims: &ReservationTicketClaims,
    ) -> Result<SignedPeerTicket, PeerSecurityError>;
}

/// Fixed private stage-protocol version bound into every stage ticket.
pub const STAGE_PROTOCOL_VERSION: u32 = 1;

/// Claims bytes accepted after signature and fence checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedClaimsBytes(pub Vec<u8>);

/// Peer-ticket validation failure.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PeerSecurityError {
    /// Ticket names a key other than the one configured deployment key.
    #[error("peer ticket key identifier is unknown")]
    UnknownKey,
    /// Ticket is malformed or does not verify.
    #[error("peer ticket signature is invalid")]
    InvalidSignature,
    /// Claims are not addressed to the expected worker.
    #[error("peer ticket audience is invalid")]
    Audience,
    /// Claims use a stale leader fence.
    #[error("peer ticket fence is stale")]
    Fence,
    /// Ticket is expired or exceeds the configured window.
    #[error("peer ticket is expired")]
    Expired,
    /// A nonce was already consumed.
    #[error("peer ticket replay detected")]
    Replay,
    /// A deterministic claims encoding failed.
    #[error("peer claims encoding failed")]
    Encoding,
    /// Verified claims do not bind all required attempt identities.
    #[error("peer ticket claims do not match the attempt")]
    Claims,
    /// Replay protection cannot safely retain another unexpired nonce.
    #[error("peer ticket replay cache is full")]
    ReplayCapacity,
    /// A required durable security audit could not commit.
    #[error("peer security audit is unavailable")]
    AuditUnavailable,
    /// The ticket authorizes a different stage operation than the one presented.
    #[error("peer ticket authorizes a different stage operation")]
    Operation,
    /// The presented raw body does not match the signed digest or exceeds bounds.
    #[error("peer stage body does not match its signed digest")]
    Body,
}

/// Failure returned by the narrow durable peer-security audit collaborator.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
#[error("peer security audit append failed")]
pub struct PeerSecurityAuditError;

/// Server-owned durable audit capability used before a rejected peer request returns.
#[async_trait]
pub trait PeerSecurityAudit: Send + Sync {
    /// Appends a rejection whose claims cannot select an audit tenant.
    ///
    /// # Errors
    /// Returns [`PeerSecurityAuditError`] when the system-tenant row cannot commit.
    async fn append_unverified_ticket_rejection(
        &self,
        violation: BifrostSecurityViolationKind,
    ) -> Result<(), PeerSecurityAuditError>;

    /// Appends a violation to the cryptographically verified tenant's audit chain.
    ///
    /// # Errors
    /// Returns [`PeerSecurityAuditError`] when the tenant-scoped row cannot commit.
    async fn append_verified_ticket_violation(
        &self,
        tenant_id: DataTenantId,
        violation: BifrostSecurityViolationKind,
    ) -> Result<(), PeerSecurityAuditError>;
}

/// Explicit no-op peer audit used only by isolated Redux tests.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopPeerSecurityAudit;

#[async_trait]
impl PeerSecurityAudit for NoopPeerSecurityAudit {
    /// Accepts an unverified rejection without persistence in isolated tests.
    ///
    /// # Errors
    /// This isolated implementation never fails.
    async fn append_unverified_ticket_rejection(
        &self,
        _violation: BifrostSecurityViolationKind,
    ) -> Result<(), PeerSecurityAuditError> {
        Ok(())
    }

    /// Accepts a verified rejection without persistence in isolated tests.
    ///
    /// # Errors
    /// This isolated implementation never fails.
    async fn append_verified_ticket_violation(
        &self,
        _tenant_id: DataTenantId,
        _violation: BifrostSecurityViolationKind,
    ) -> Result<(), PeerSecurityAuditError> {
        Ok(())
    }
}

/// Server-owned signing capability consumed by Redux dispatch.
pub trait PeerTicketMinter: Send + Sync {
    /// Mints one typed, single-use ticket without exposing key material.
    ///
    /// # Errors
    /// Returns a closed encoding or authority failure.
    fn mint_peer_ticket(
        &self,
        claims: &PeerTicketClaims,
    ) -> Result<SignedPeerTicket, PeerSecurityError>;
}

/// Narrow raw-ticket verification capability implemented by the server authority.
#[async_trait]
pub trait PeerTicketVerifier: Send + Sync {
    /// Verifies raw ticket bytes before decoding claims.
    ///
    /// # Errors
    /// Returns a closed peer-security failure without exposing key material.
    async fn verify_peer_ticket(
        &self,
        ticket: &SignedPeerTicket,
        expected_worker: NodeId,
        expected_worker_fence: u64,
        now: DateTime<Utc>,
    ) -> Result<VerifiedClaimsBytes, PeerSecurityError>;
}

/// One key identifier and nonce pair retained until ticket expiry.
type ReplayIdentity = (String, Vec<u8>);

/// Bounded worker-local nonce cache.
#[derive(Debug)]
pub struct PeerReplayCache {
    /// Unexpired key-and-nonce identities consumed by this worker role.
    entries: Mutex<HashMap<ReplayIdentity, DateTime<Utc>>>,
    /// Hard maximum number of unexpired identities retained.
    capacity: usize,
}

impl PeerReplayCache {
    /// Creates a bounded replay cache.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            capacity,
        }
    }
    /// Atomically consumes a nonce until its expiry.
    ///
    /// # Errors
    /// Returns [`PeerSecurityError::Replay`] for duplicates or when bounded insertion is full.
    pub fn consume(
        &self,
        key_id: &str,
        nonce: &[u8],
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<(), PeerSecurityError> {
        let mut entries = self.entries.lock().map_err(|_| PeerSecurityError::Replay)?;
        entries.retain(|_, expiry| *expiry > now);
        let key = (key_id.to_owned(), nonce.to_vec());
        if entries.contains_key(&key) {
            return Err(PeerSecurityError::Replay);
        }
        if entries.len() >= self.capacity {
            return Err(PeerSecurityError::ReplayCapacity);
        }
        entries.insert(key, expires_at);
        Ok(())
    }

    /// Returns the number of retained unexpired nonce identities.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries
            .lock()
            .map_or(self.capacity, |entries| entries.len())
    }

    /// Returns whether no replay identities are currently retained.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries
            .lock()
            .map_or(self.capacity == 0, |entries| entries.is_empty())
    }
}

/// Deterministic test signer implementing the same opaque contract.
#[derive(Debug, Clone)]
pub struct DeterministicTestSigner {
    /// Fixed key identifier.
    pub key_id: String,
}

impl PeerTicketMinter for DeterministicTestSigner {
    /// Encodes claims without pretending to provide production key custody.
    ///
    /// # Errors
    /// Returns [`PeerSecurityError::Encoding`] when protobuf encoding fails.
    fn mint_peer_ticket(
        &self,
        claims: &PeerTicketClaims,
    ) -> Result<SignedPeerTicket, PeerSecurityError> {
        let mut bytes = Vec::new();
        claims
            .encode(&mut bytes)
            .map_err(|_| PeerSecurityError::Encoding)?;
        Ok(SignedPeerTicket {
            key_id: self.key_id.clone(),
            claims_bytes: bytes.clone(),
            signature: bytes,
        })
    }
}

/// Builds the stable domain-separated signing preimage for one peer ticket.
///
/// The byte order is fixed as `DOMAIN || key_id || claims`. It lives here, next
/// to the claim shapes, because both the signing authority and any harness that
/// must produce a ticket under a retired or unpublished key have to agree on it
/// exactly; two copies of this format would drift silently and only show up as
/// an unexplained signature refusal.
#[must_use]
pub fn peer_signing_input(domain: &[u8], key_id: &str, claims: &[u8]) -> Vec<u8> {
    [domain, key_id.as_bytes(), claims].concat()
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
/// Both the leader (at mint time) and the follower (at verification time)
/// call this over the same list, in the same order, so a matching digest
/// proves the follower's actual assignments are exactly the ones the leader
/// signed — including every file, schema fingerprint, and closed predicate.
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
/// Nothing downstream re-derives the tenant: it is the cryptographically
/// verified one, carried here so a handler cannot accidentally resolve tenancy
/// from an unverified field.
#[derive(Debug, Clone)]
pub struct AuthorizedStage {
    /// The verified claims, already matched field-by-field to the receiver's
    /// own [`StageBinding`].
    pub claims: StageTicketClaims,
    /// The tenant the signature actually bound.
    pub tenant_id: DataTenantId,
}

/// The server-owned authority every Analytical stage operation passes through.
///
/// The contract lives here, next to the claims and binding it operates on, so
/// the Oracle follower ingress can require authorization without depending on
/// the server crate that owns the signing key. The server implements it on its
/// existing peer authority; nothing else may.
///
/// Both directions are on one trait because they are one protocol: the leader
/// mints exactly the ticket the follower will re-derive and check.
#[async_trait]
pub trait OracleStageAuthority: Send + Sync {
    /// Signs one single-use ticket for exactly one stage operation.
    ///
    /// # Errors
    ///
    /// Returns [`PeerSecurityError::Operation`] when `claims` does not carry
    /// `operation`, and [`PeerSecurityError::Encoding`] when the claims cannot
    /// be encoded within the implementation's bound.
    fn mint_stage(
        &self,
        operation: StageOperationV1,
        claims: &StageTicketClaims,
    ) -> Result<SignedPeerTicket, PeerSecurityError>;

    /// Authorizes one stage operation before its body may be decoded or used.
    ///
    /// Holding the returned [`AuthorizedStage`] is the receiver's proof that
    /// every check ran: signature over the operation's own domain, exact
    /// body digest, field-by-field binding, deadline, expiry, and single-use
    /// nonce consumption. A caller that decodes, reads a cache, constructs a
    /// provider, or issues I/O before this returns has broken the contract.
    ///
    /// # Errors
    ///
    /// Returns the closed [`PeerSecurityError`] for the first failed check, or
    /// [`PeerSecurityError::AuditUnavailable`] when the required audit row
    /// cannot commit. A rejection never returns claims.
    async fn authorize_stage(
        &self,
        ticket: &SignedPeerTicket,
        binding: &StageBinding,
        body: &[u8],
        now: DateTime<Utc>,
    ) -> Result<AuthorizedStage, PeerSecurityError>;
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};
    use std::thread;

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
    /// Signed claims are only as strong as the weakest field the receiver
    /// actually compares, so this walks one mutation per field and asserts each
    /// is refused with its own closed error. A field that stops being compared
    /// would let a peer substitute that value while keeping a valid signature.
    ///
    /// # Panics
    ///
    /// Panics when any single-field mutation is accepted.
    #[test]
    fn oracle_stage_claims_reject_every_single_field_mutation() {
        let binding = stage_binding();
        let digest = stage_body_digest(b"body").expect("a bounded body digests");
        let claims = StageTicketClaims::for_binding(
            &binding,
            digest.clone(),
            vec![0; 16],
            1_000,
            2_000,
            Vec::new(),
        );
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

    /// A `SetPlan` ticket cannot authorize an `ExecuteTask` operation.
    ///
    /// The two operations carry distinct signing domains, so this is belt and
    /// braces at the claims layer: even if a signature somehow verified, the
    /// operation field is compared against the receiving entry point's own
    /// expectation rather than against anything in the message.
    ///
    /// # Panics
    ///
    /// Panics when a cross-operation ticket is accepted.
    #[test]
    fn oracle_stage_operations_have_distinct_domains_and_are_not_interchangeable() {
        assert_ne!(
            StageOperationV1::SetPlan.domain(),
            StageOperationV1::ExecuteTask.domain()
        );
        assert_eq!(StageOperationV1::from_u32(0), None);
        assert_eq!(StageOperationV1::from_u32(3), None);

        let mut binding = stage_binding();
        binding.operation = StageOperationV1::SetPlan;
        binding.task_id = None;
        let digest = stage_body_digest(b"plan").expect("a bounded body digests");
        let claims = StageTicketClaims::for_binding(
            &binding,
            digest.clone(),
            vec![0; 16],
            1_000,
            2_000,
            Vec::new(),
        );

        let mut executing = binding.clone();
        executing.operation = StageOperationV1::ExecuteTask;
        assert_eq!(
            claims.verify_binding(&executing, &digest),
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

    /// Concurrent duplicate nonce consumption admits exactly one caller.
    #[test]
    fn oracle_peer_replay_cache_consumes_nonce_atomically() {
        let cache = Arc::new(PeerReplayCache::new(4));
        let barrier = Arc::new(Barrier::new(3));
        let now = Utc::now();
        let handles = (0..2)
            .map(|_| {
                let cache = Arc::clone(&cache);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    cache.consume(
                        "kid",
                        b"0123456789abcdef",
                        now + chrono::Duration::seconds(1),
                        now,
                    )
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        let results = handles
            .into_iter()
            .map(|handle| handle.join().expect("replay thread"))
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(result, Err(PeerSecurityError::Replay)))
                .count(),
            1
        );
    }

    /// Capacity fails closed while expiry reclaims a consumed nonce.
    #[test]
    fn oracle_peer_replay_cache_is_bounded_and_expiry_reclaims() {
        let cache = PeerReplayCache::new(1);
        let now = Utc::now();
        cache
            .consume("kid", b"first", now + chrono::Duration::seconds(1), now)
            .expect("first nonce");
        assert_eq!(
            cache.consume("kid", b"second", now + chrono::Duration::seconds(1), now),
            Err(PeerSecurityError::ReplayCapacity)
        );
        cache
            .consume(
                "kid",
                b"second",
                now + chrono::Duration::seconds(2),
                now + chrono::Duration::seconds(1),
            )
            .expect("expired nonce reclaimed");
        assert_eq!(cache.len(), 1);
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
