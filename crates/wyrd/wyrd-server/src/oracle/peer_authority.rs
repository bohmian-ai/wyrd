//! Receiver-side authority for typed Oracle peer contexts.
//!
//! Peer RPCs travel only over the mTLS peer plane, which proves that the caller
//! is a cluster member. The typed context each request carries is unsigned: it
//! names the tenant, query, audience, fences, digests, and deadlines the caller
//! intends. This authority checks those fields against the receiver's own
//! trusted state — its node identity and fence, the exact bytes received, and
//! bounded expiry — before any claims-driven decode or storage IO, and commits
//! a durable audit row before returning any refusal.
//!
//! The digests prove consistency between the context and the body, not origin;
//! origin is the mTLS cluster identity.

use chrono::{DateTime, Utc};
use std::fmt;
use std::sync::Arc;
use vala_bifrost_redux::oracle::peer::{
    AuthorizedStage, OracleStageAuthority, PeerSecurityAudit, PeerSecurityError, PeerTicketClaims,
    PeerTicketVerifier, ReservationBinding, ReservationTicketClaims, StageBinding,
    StageTicketClaims, VerifiedClaimsBytes, reservation_body_digest, stage_body_digest,
};
use vala_bifrost_redux::oracle::telemetry::{
    AnalyticalStageAuthorityOutcome, record_stage_authority,
};
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{
    BifrostQueryRequest, BifrostSecurityViolationKind, NodeId, PeerContext,
};
use wyrd_tonic::prost::Message;

/// Hard cap applied before any fragment claims bytes are decoded.
const MAX_CLAIMS_BYTES: usize = vala_bifrost_redux::oracle::peer::MAX_FRAGMENT_CONTEXT_BYTES;
/// Hard cap applied to reservation claims before decoding.
const MAX_RESERVATION_CLAIMS_BYTES: usize =
    vala_bifrost_redux::oracle::peer::MAX_RESERVATION_CONTEXT_BYTES;
/// Hard cap applied to stage claims before decoding.
const MAX_STAGE_CLAIMS_BYTES: usize = vala_bifrost_redux::oracle::peer::MAX_STAGE_CONTEXT_BYTES;
/// Query envelopes include the bounded public SQL request and its authenticated context.
const MAX_FORWARD_QUERY_BYTES: usize = 128 * 1024;
/// Maximum lifetime accepted for a newly presented context.
const MAX_CONTEXT_TTL: chrono::Duration = chrono::Duration::seconds(30);

/// Authenticated ingress state forwarded once to a selected ready Oracle.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ForwardQueryClaims {
    /// Closed forwarding-envelope protocol version.
    pub protocol_version: u32,
    /// Exact selected Oracle audience.
    pub audience: NodeId,
    /// Exact selected Oracle role-incarnation fence.
    pub worker_fence: u64,
    /// Short envelope acceptance expiry, distinct from the query deadline.
    pub expires_at_ms: i64,
    /// Original verified public caller context without bearer credentials.
    pub context: vala_bifrost_redux::oracle::AuthorizedQueryContext,
    /// Original validated public query request.
    pub request: BifrostQueryRequest,
    /// Exact absolute query deadline, repeated for fail-closed envelope validation.
    pub absolute_deadline_ms: i64,
}

impl ForwardQueryClaims {
    /// Encodes these claims as one bounded forwarding context.
    ///
    /// # Errors
    /// Returns [`PeerSecurityError::Encoding`] when the envelope cannot be
    /// serialized or exceeds the forwarding bound.
    pub fn to_context(&self) -> Result<PeerContext, PeerSecurityError> {
        let claims_bytes = serde_json::to_vec(self).map_err(|_| PeerSecurityError::Encoding)?;
        if claims_bytes.is_empty() || claims_bytes.len() > MAX_FORWARD_QUERY_BYTES {
            return Err(PeerSecurityError::Encoding);
        }
        Ok(PeerContext { claims_bytes })
    }
}

/// Server-owned receiver authority for the private peer protocol.
///
/// Holds only the durable audit collaborator: every check reads the presented
/// context and the receiver's own identity, fence, clock, and body bytes.
pub struct OraclePeerAuthority {
    /// Durable audit collaborator required before returning any rejection.
    security_audit: Arc<dyn PeerSecurityAudit>,
}

impl fmt::Debug for OraclePeerAuthority {
    /// Formats no audit state; the authority owns no secret material.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OraclePeerAuthority")
            .finish_non_exhaustive()
    }
}

/// Returns whether `expires_at_ms` is still live and within the context window.
fn context_expiry_valid(expires_at_ms: i64, now: DateTime<Utc>) -> bool {
    let Some(max_expiry) = now.checked_add_signed(MAX_CONTEXT_TTL) else {
        return false;
    };
    expires_at_ms > now.timestamp_millis() && expires_at_ms <= max_expiry.timestamp_millis()
}

impl OraclePeerAuthority {
    /// Composes the authority over the durable peer-security audit writer.
    #[must_use]
    pub fn new(security_audit: Arc<dyn PeerSecurityAudit>) -> Self {
        Self { security_audit }
    }

    /// Checks audience, fence, and expiry of a forwarding context before the
    /// forwarded query is admitted.
    ///
    /// # Errors
    /// Returns an audited closed security error for every invalid envelope.
    pub fn verify_forward_query(
        &self,
        context: &PeerContext,
        expected_worker: NodeId,
        expected_fence: u64,
        now: DateTime<Utc>,
    ) -> Result<ForwardQueryClaims, PeerSecurityError> {
        if context.claims_bytes.is_empty() || context.claims_bytes.len() > MAX_FORWARD_QUERY_BYTES {
            return Err(self.forwarding_rejection(
                BifrostSecurityViolationKind::PeerSignature,
                PeerSecurityError::Malformed,
            ));
        }
        let claims: ForwardQueryClaims = match serde_json::from_slice(&context.claims_bytes) {
            Ok(claims) => claims,
            Err(_) => {
                return Err(self.forwarding_rejection(
                    BifrostSecurityViolationKind::PeerSignature,
                    PeerSecurityError::Malformed,
                ));
            }
        };
        let violation = if claims.protocol_version != 1 || claims.audience != expected_worker {
            Some((
                BifrostSecurityViolationKind::PeerAudience,
                PeerSecurityError::Audience,
            ))
        } else if claims.worker_fence != expected_fence {
            Some((
                BifrostSecurityViolationKind::PeerFence,
                PeerSecurityError::Fence,
            ))
        } else if !context_expiry_valid(claims.expires_at_ms, now) {
            Some((
                BifrostSecurityViolationKind::PeerReplay,
                PeerSecurityError::Expired,
            ))
        } else {
            None
        };
        if let Some((kind, error)) = violation {
            return Err(self.forwarding_rejection(kind, error));
        }
        Ok(claims)
    }

    /// Stages one forwarding rejection on the system chain and returns the
    /// rejection unchanged.
    ///
    /// A forwarding envelope names its tenant in unsigned claims that no
    /// receiver state has bound yet, so a refusal is never attributed to that
    /// tenant: a connected peer must not be able to write into a foreign
    /// tenant's audit chain by forging one.
    fn forwarding_rejection(
        &self,
        violation: BifrostSecurityViolationKind,
        error: PeerSecurityError,
    ) -> PeerSecurityError {
        self.security_audit
            .append_unverified_ticket_rejection(violation);
        error
    }

    /// Checks one fragment context against this receiver before plan decode.
    ///
    /// Order: bounds, claims decode, tenant, audience (this node), fence (this
    /// role incarnation), then bounded expiry. The caller still validates query,
    /// reservation, digests, and assignments against its own reservation state
    /// before any provider or storage IO. Every refusal here is audited on the
    /// system chain, because no receiver state has bound the claimed tenant.
    ///
    /// # Errors
    /// Returns a closed, durably audited security error for a malformed,
    /// misaddressed, stale-fence, or expired context.
    pub fn verify_before_decode(
        &self,
        context: &PeerContext,
        expected_worker: NodeId,
        expected_fence: u64,
        now: DateTime<Utc>,
    ) -> Result<VerifiedClaimsBytes, PeerSecurityError> {
        if context.claims_bytes.is_empty() || context.claims_bytes.len() > MAX_CLAIMS_BYTES {
            return self.reject_unverified(
                BifrostSecurityViolationKind::PeerSignature,
                PeerSecurityError::Malformed,
            );
        }
        let Ok(claims) = PeerTicketClaims::decode(context.claims_bytes.as_slice()) else {
            return self.reject_unverified(
                BifrostSecurityViolationKind::PeerSignature,
                PeerSecurityError::Malformed,
            );
        };
        if uuid::Uuid::from_slice(&claims.tenant_id)
            .ok()
            .and_then(|tenant| DataTenantId::new(tenant).ok())
            .is_none()
        {
            return self.reject_unverified(
                BifrostSecurityViolationKind::PeerTenant,
                PeerSecurityError::Claims,
            );
        }
        let violation = if claims.audience != expected_worker.as_uuid().as_bytes() {
            Some((
                BifrostSecurityViolationKind::PeerAudience,
                PeerSecurityError::Audience,
            ))
        } else if claims.worker_fence != expected_fence {
            Some((
                BifrostSecurityViolationKind::PeerFence,
                PeerSecurityError::Fence,
            ))
        } else if !context_expiry_valid(claims.expires_at_ms, now) {
            Some((
                BifrostSecurityViolationKind::PeerReplay,
                PeerSecurityError::Expired,
            ))
        } else {
            None
        };
        if let Some((violation, error)) = violation {
            return self.reject_unverified(violation, error);
        }
        Ok(VerifiedClaimsBytes(context.claims_bytes.clone()))
    }

    /// Stages a system-chain audit before returning a pre-binding rejection.
    ///
    /// Every fragment and forwarding refusal lands here: until the caller
    /// matches the context against its own query and reservation state, the
    /// tenant the context names is an unsigned claim, not an attribution.
    ///
    /// # Errors
    /// Always returns the original closed rejection.
    fn reject_unverified<T>(
        &self,
        violation: BifrostSecurityViolationKind,
        error: PeerSecurityError,
    ) -> Result<T, PeerSecurityError> {
        self.security_audit
            .append_unverified_ticket_rejection(violation);
        Err(error)
    }

    /// Authorizes exactly one reservation operation before it changes state.
    ///
    /// Order: bounds, the body digest over the exact bytes received, claims
    /// decode, every bound identity against the receiver-derived binding
    /// (operation, both nodes, both fences, query), then bounded expiry. No
    /// grant is taken before all of it passes. A replayed reserve can only open
    /// a second grant stream, and that grant lives no longer than the stream
    /// the replaying caller holds open.
    ///
    /// The body is the encoded request with its context field cleared, which
    /// is what both sides digest.
    ///
    /// Rejections audit on the system chain: a reservation is a control-plane
    /// operation between Oracles and binds no data tenant to attribute to.
    ///
    /// # Errors
    ///
    /// Returns the audited closed rejection for a malformed, wrong-body,
    /// misbound, or expired context.
    pub fn verify_reservation(
        &self,
        context: &PeerContext,
        binding: &ReservationBinding,
        body: &[u8],
        now: DateTime<Utc>,
    ) -> Result<ReservationTicketClaims, PeerSecurityError> {
        if context.claims_bytes.is_empty()
            || context.claims_bytes.len() > MAX_RESERVATION_CLAIMS_BYTES
        {
            return self.reject_unverified(
                BifrostSecurityViolationKind::PeerSignature,
                PeerSecurityError::Malformed,
            );
        }
        let body_digest = match reservation_body_digest(body) {
            Ok(digest) => digest,
            Err(error) => {
                return self.reject_unverified(BifrostSecurityViolationKind::PeerFragment, error);
            }
        };
        let Ok(claims) = ReservationTicketClaims::decode(context.claims_bytes.as_slice()) else {
            return self.reject_unverified(
                BifrostSecurityViolationKind::PeerSignature,
                PeerSecurityError::Malformed,
            );
        };
        if let Err(error) = claims.verify_binding(binding, &body_digest) {
            let violation = match error {
                PeerSecurityError::Audience => BifrostSecurityViolationKind::PeerAudience,
                PeerSecurityError::Fence => BifrostSecurityViolationKind::PeerFence,
                PeerSecurityError::Body => BifrostSecurityViolationKind::PeerFragment,
                _ => BifrostSecurityViolationKind::PeerStageBinding,
            };
            return self.reject_unverified(violation, error);
        }
        if !context_expiry_valid(claims.expires_at_ms, now) {
            return self.reject_unverified(
                BifrostSecurityViolationKind::PeerReplay,
                PeerSecurityError::Expired,
            );
        }
        Ok(claims)
    }

    /// Authorizes one stage operation before any decode, cache access, or I/O.
    ///
    /// The order is the security property:
    ///
    /// 1. the claims-byte bound — no attacker-controlled length reaches a parser;
    /// 2. the bounded raw body's digest, so the bytes about to be decoded are
    ///    exactly the bytes the context describes;
    /// 3. claims decode, then tenant extraction;
    /// 4. a field-by-field match against the receiver's own [`StageBinding`];
    /// 5. the absolute query deadline and the context's own short expiry.
    ///
    /// Every refusal stages an audit row before it returns, and emits
    /// closed-label stage-authority telemetry. Refusals up to and including a
    /// binding mismatch are audited on the system chain; only an expiry after
    /// the claims matched the receiver's binding is attributed to its tenant.
    ///
    /// # Errors
    ///
    /// Returns [`PeerSecurityError::Malformed`], `Body`, `Operation`,
    /// `Audience`, `Fence`, `Claims`, or `Expired`. A rejection never returns
    /// claims.
    fn authorize_stage_inner(
        &self,
        context: &PeerContext,
        binding: &StageBinding,
        body: &[u8],
        now: DateTime<Utc>,
    ) -> Result<AuthorizedStage, PeerSecurityError> {
        if context.claims_bytes.is_empty() || context.claims_bytes.len() > MAX_STAGE_CLAIMS_BYTES {
            return self.reject_stage_unverified(
                binding,
                BifrostSecurityViolationKind::PeerSignature,
                PeerSecurityError::Malformed,
                AnalyticalStageAuthorityOutcome::Malformed,
            );
        }
        let body_digest = match stage_body_digest(body) {
            Ok(digest) => digest,
            Err(error) => {
                return self.reject_stage_unverified(
                    binding,
                    BifrostSecurityViolationKind::PeerFragment,
                    error,
                    AnalyticalStageAuthorityOutcome::Body,
                );
            }
        };
        let Ok(claims) = StageTicketClaims::decode(context.claims_bytes.as_slice()) else {
            return self.reject_stage_unverified(
                binding,
                BifrostSecurityViolationKind::PeerSignature,
                PeerSecurityError::Malformed,
                AnalyticalStageAuthorityOutcome::Malformed,
            );
        };
        let Some(tenant_id) = uuid::Uuid::from_slice(&claims.tenant_id)
            .ok()
            .and_then(|tenant| DataTenantId::new(tenant).ok())
        else {
            return self.reject_stage_unverified(
                binding,
                BifrostSecurityViolationKind::PeerTenant,
                PeerSecurityError::Claims,
                AnalyticalStageAuthorityOutcome::Binding,
            );
        };
        if let Err(error) = claims.verify_binding(binding, &body_digest) {
            // The claimed tenant has not matched the receiver's binding yet, so
            // it cannot choose whose audit chain records this refusal.
            let (violation, outcome) = stage_binding_violation(&error);
            return self.reject_stage_unverified(binding, violation, error, outcome);
        }
        if claims.absolute_deadline_ms <= now.timestamp_millis()
            || !context_expiry_valid(claims.expires_at_ms, now)
        {
            return self.reject_stage_verified(
                binding,
                tenant_id,
                BifrostSecurityViolationKind::PeerStageBinding,
                PeerSecurityError::Expired,
                AnalyticalStageAuthorityOutcome::Expired,
            );
        }
        record_stage_authority(
            binding.operation.telemetry(),
            AnalyticalStageAuthorityOutcome::Authorized,
        );
        Ok(AuthorizedStage { claims, tenant_id })
    }

    /// Stages and counts a stage rejection with no attributable tenant.
    ///
    /// # Errors
    /// Always returns the original closed rejection.
    fn reject_stage_unverified(
        &self,
        binding: &StageBinding,
        violation: BifrostSecurityViolationKind,
        error: PeerSecurityError,
        outcome: AnalyticalStageAuthorityOutcome,
    ) -> Result<AuthorizedStage, PeerSecurityError> {
        record_stage_authority(binding.operation.telemetry(), outcome);
        self.security_audit
            .append_unverified_ticket_rejection(violation);
        Err(error)
    }

    /// Stages and counts a stage rejection against the context tenant chain.
    ///
    /// # Errors
    /// Always returns the original closed rejection.
    fn reject_stage_verified(
        &self,
        binding: &StageBinding,
        tenant_id: DataTenantId,
        violation: BifrostSecurityViolationKind,
        error: PeerSecurityError,
        outcome: AnalyticalStageAuthorityOutcome,
    ) -> Result<AuthorizedStage, PeerSecurityError> {
        record_stage_authority(binding.operation.telemetry(), outcome);
        self.security_audit
            .append_verified_ticket_violation(tenant_id, violation);
        Err(error)
    }
}

#[async_trait::async_trait]
impl OracleStageAuthority for OraclePeerAuthority {
    /// Authorizes one stage operation before any decode, cache read, or I/O.
    ///
    /// # Errors
    ///
    /// Returns the closed failure for the first check that did not pass.
    async fn authorize_stage(
        &self,
        context: &PeerContext,
        binding: &StageBinding,
        body: &[u8],
        now: DateTime<Utc>,
    ) -> Result<AuthorizedStage, PeerSecurityError> {
        self.authorize_stage_inner(context, binding, body, now)
    }
}

#[async_trait::async_trait]
impl PeerTicketVerifier for OraclePeerAuthority {
    /// Checks one raw fragment context against this receiver.
    ///
    /// # Errors
    /// Returns a closed malformed, claim, expiry, fence, or audience failure.
    async fn verify_peer_ticket(
        &self,
        context: &PeerContext,
        expected_worker: NodeId,
        expected_worker_fence: u64,
        now: DateTime<Utc>,
    ) -> Result<VerifiedClaimsBytes, PeerSecurityError> {
        self.verify_before_decode(context, expected_worker, expected_worker_fence, now)
    }
}

/// Projects one binding failure onto its audit class and telemetry outcome.
///
/// Kept as a free function because it is a pure closed-set mapping with no
/// authority state; the audit kind and the metric label are chosen together so
/// the two surfaces can never disagree about why a stage was refused.
fn stage_binding_violation(
    error: &PeerSecurityError,
) -> (
    BifrostSecurityViolationKind,
    AnalyticalStageAuthorityOutcome,
) {
    match *error {
        PeerSecurityError::Audience => (
            BifrostSecurityViolationKind::PeerAudience,
            AnalyticalStageAuthorityOutcome::Binding,
        ),
        PeerSecurityError::Fence => (
            BifrostSecurityViolationKind::PeerFence,
            AnalyticalStageAuthorityOutcome::Binding,
        ),
        PeerSecurityError::Body => (
            BifrostSecurityViolationKind::PeerFragment,
            AnalyticalStageAuthorityOutcome::Body,
        ),
        _ => (
            BifrostSecurityViolationKind::PeerStageBinding,
            AnalyticalStageAuthorityOutcome::Binding,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use vala_bifrost_redux::oracle::peer::{
        PeerSecurityAudit, ReservationOperationV1, StageOperationV1,
    };

    /// One captured audit call, where `None` denotes the system chain.
    type AuditCall = (Option<DataTenantId>, BifrostSecurityViolationKind);

    /// Captures the two audit paths without replacing authority verification.
    #[derive(Default)]
    struct RecordingPeerAudit {
        /// Ordered audit calls.
        calls: Mutex<Vec<AuditCall>>,
    }

    impl PeerSecurityAudit for RecordingPeerAudit {
        /// Captures an unattributable rejection as a system-chain call.
        fn append_unverified_ticket_rejection(&self, violation: BifrostSecurityViolationKind) {
            self.calls
                .lock()
                .expect("audit mutex")
                .push((None, violation));
        }

        /// Captures a rejection with its context tenant.
        fn append_verified_ticket_violation(
            &self,
            tenant_id: DataTenantId,
            violation: BifrostSecurityViolationKind,
        ) {
            self.calls
                .lock()
                .expect("audit mutex")
                .push((Some(tenant_id), violation));
        }
    }

    /// Creates an authority and its recording audit collaborator.
    fn authority() -> (OraclePeerAuthority, Arc<RecordingPeerAudit>) {
        let audit = Arc::new(RecordingPeerAudit::default());
        (OraclePeerAuthority::new(audit.clone()), audit)
    }

    /// Builds the reservation binding every reservation test starts from.
    fn reservation_binding() -> ReservationBinding {
        ReservationBinding {
            operation: ReservationOperationV1::ReserveSlots,
            source_node_id: NodeId::new(uuid::Uuid::from_u128(31)),
            source_fence: 7,
            destination_node_id: NodeId::new(uuid::Uuid::from_u128(32)),
            destination_fence: 9,
            query_id: uuid::Uuid::from_u128(33),
        }
    }

    /// Builds one correct reservation context over `body`.
    fn reservation_context(binding: &ReservationBinding, body: &[u8]) -> PeerContext {
        ReservationTicketClaims::for_binding(
            binding,
            reservation_body_digest(body).expect("reservation body digest"),
            (Utc::now() + chrono::Duration::seconds(5)).timestamp_millis(),
        )
        .to_context(binding.operation)
        .expect("reservation context")
    }

    /// A reservation context authorizes exactly one operation over one body.
    ///
    /// A context naming another operation does not authorize a reserve even
    /// with every identity matching, and a substituted request body is refused
    /// while every identity still matches.
    #[tokio::test]
    async fn reservation_contexts_are_operation_and_body_exact() {
        let (authority, audit) = authority();
        let binding = reservation_binding();
        let body = b"reserve-request".as_slice();
        let context = reservation_context(&binding, body);

        authority
            .verify_reservation(&context, &binding, body, Utc::now())
            .expect("a correct reservation context is authorized");
        assert_eq!(
            authority
                .verify_reservation(&context, &binding, b"substituted-request", Utc::now())
                .expect_err("a substituted body is refused"),
            PeerSecurityError::Body
        );

        let mut foreign = ReservationTicketClaims::for_binding(
            &binding,
            reservation_body_digest(body).expect("reservation body digest"),
            (Utc::now() + chrono::Duration::seconds(5)).timestamp_millis(),
        );
        foreign.operation = ReservationOperationV1::ReserveSlots.as_u32() + 1;
        assert_eq!(
            authority
                .verify_reservation(
                    &PeerContext {
                        claims_bytes: foreign.encode_to_vec(),
                    },
                    &binding,
                    body,
                    Utc::now()
                )
                .expect_err("a context for another operation does not authorize a reserve"),
            PeerSecurityError::Operation
        );
        let stale = ReservationBinding {
            destination_fence: 10,
            ..reservation_binding()
        };
        assert_eq!(
            authority
                .verify_reservation(&context, &stale, body, Utc::now())
                .expect_err("a restarted follower refuses the old fence"),
            PeerSecurityError::Fence
        );
        assert_eq!(audit.calls.lock().expect("audit mutex").len(), 3);
    }

    /// Builds one fully bound fragment claim for authority tests.
    fn claims(
        worker: NodeId,
        worker_fence: u64,
        tenant_id: DataTenantId,
        now: DateTime<Utc>,
    ) -> PeerTicketClaims {
        PeerTicketClaims {
            protocol_version: vala_bifrost_redux::oracle::dispatcher::PEER_PROTOCOL_VERSION,
            audience: worker.as_uuid().as_bytes().to_vec(),
            worker_fence,
            leader_node_id: uuid::Uuid::from_u128(2).as_bytes().to_vec(),
            leader_fence: 3,
            query_id: uuid::Uuid::from_u128(4).as_bytes().to_vec(),
            tenant_id: tenant_id.as_uuid().as_bytes().to_vec(),
            expires_at_ms: (now + chrono::Duration::seconds(10)).timestamp_millis(),
            execution_deadline_unix_ms: (now + chrono::Duration::seconds(30)).timestamp_millis(),
            binding: "binding".to_owned(),
            fragment_digest: "fragment".to_owned(),
            manifest_digest: "manifest".to_owned(),
            projection_digest: "projection".to_owned(),
            permission_digest: "permission".to_owned(),
            assignment_authority_digest: "assignment-authority".to_owned(),
        }
    }

    /// Records the three post-authorization effects a stage operation has.
    ///
    /// The ordering property `authorize_stage` exists to guarantee is that a
    /// refused operation performs none of these. The probe stands in for the
    /// real follower entry point, which decodes the subplan, reads or writes
    /// the upstream task cache, and then issues object I/O — in that order,
    /// and only after the authority returns.
    #[derive(Debug, Default)]
    struct StageEffectProbe {
        /// Subplan decodes performed.
        decoded: AtomicUsize,
        /// Task-cache accesses performed.
        cache_hits: AtomicUsize,
        /// Object-store reads issued.
        io_reads: AtomicUsize,
    }

    impl StageEffectProbe {
        /// Runs the authority, then the effects it gates, in production order.
        ///
        /// # Errors
        /// Returns the authority's closed rejection, having performed no effect.
        async fn authorize_then_execute(
            &self,
            authority: &OraclePeerAuthority,
            context: &PeerContext,
            binding: &StageBinding,
            body: &[u8],
            now: DateTime<Utc>,
        ) -> Result<AuthorizedStage, PeerSecurityError> {
            let authorized = authority
                .authorize_stage(context, binding, body, now)
                .await?;
            self.decoded.fetch_add(1, Ordering::SeqCst);
            self.cache_hits.fetch_add(1, Ordering::SeqCst);
            self.io_reads.fetch_add(1, Ordering::SeqCst);
            Ok(authorized)
        }

        /// Returns the three effect counts as one comparable tuple.
        fn counts(&self) -> (usize, usize, usize) {
            (
                self.decoded.load(Ordering::SeqCst),
                self.cache_hits.load(Ordering::SeqCst),
                self.io_reads.load(Ordering::SeqCst),
            )
        }
    }

    /// One named single-field claims mutation and the closed error it forces.
    type StageMutation = (
        &'static str,
        Box<dyn Fn(&mut StageTicketClaims)>,
        PeerSecurityError,
    );

    /// Builds the follower-derived binding every stage test compares against.
    fn stage_binding(tenant_id: DataTenantId) -> StageBinding {
        StageBinding {
            operation: StageOperationV1::ExecuteTask,
            source_node_id: NodeId::new(uuid::Uuid::from_u128(11)),
            source_fence: 3,
            destination_node_id: NodeId::new(uuid::Uuid::from_u128(12)),
            destination_fence: 5,
            tenant_id,
            public_query_id: uuid::Uuid::from_u128(21),
            datafusion_query_id: uuid::Uuid::from_u128(22),
            snapshot_digest: "snapshot".to_owned(),
            stage_id: 2,
            task_id: Some(1),
            attempt: 0,
            reservation_id: "reservation".to_owned(),
            permission_digest: "permission".to_owned(),
        }
    }

    /// Builds claims for one binding over `body`.
    fn stage_claims(binding: &StageBinding, body: &[u8], now: DateTime<Utc>) -> StageTicketClaims {
        StageTicketClaims::for_binding(
            binding,
            stage_body_digest(body).expect("a bounded fixture body digests"),
            (now + chrono::Duration::seconds(20)).timestamp_millis(),
            (now + chrono::Duration::seconds(10)).timestamp_millis(),
            Vec::new(),
        )
    }

    /// Encodes `claims` as the raw context bytes a coordinator would send.
    fn raw_stage_context(claims: &StageTicketClaims) -> PeerContext {
        PeerContext {
            claims_bytes: claims.encode_to_vec(),
        }
    }

    /// A stage operation is refused before any decode, cache access, or I/O.
    ///
    /// This is the ordering gate for the whole Analytical path. Each negative
    /// binding is exercised independently — including the public and DataFusion
    /// query identities separately, which is what stops a sibling distributed
    /// graph under the same public query from borrowing another graph's context.
    ///
    /// # Panics
    ///
    /// Panics when any negative binding is accepted or when a refusal performs
    /// a gated effect.
    #[tokio::test]
    async fn stage_authority_rejects_before_decode_cache_or_io() {
        let (authority, audit) = authority();
        let tenant_id = DataTenantId::new_v7();
        let binding = stage_binding(tenant_id);
        let body = b"stage-operation-body".as_slice();
        let now = Utc::now();
        let probe = StageEffectProbe::default();

        let foreign_tenant = DataTenantId::new_v7();
        let mutations: Vec<StageMutation> = vec![
            (
                "tenant",
                Box::new(move |c: &mut StageTicketClaims| {
                    c.tenant_id = foreign_tenant.as_uuid().as_bytes().to_vec();
                }),
                PeerSecurityError::Claims,
            ),
            (
                "public query identity",
                Box::new(|c: &mut StageTicketClaims| {
                    c.public_query_id = uuid::Uuid::from_u128(99).as_bytes().to_vec();
                }),
                PeerSecurityError::Claims,
            ),
            (
                "datafusion query identity",
                Box::new(|c: &mut StageTicketClaims| {
                    c.datafusion_query_id = uuid::Uuid::from_u128(98).as_bytes().to_vec();
                }),
                PeerSecurityError::Claims,
            ),
            (
                "coordinator node",
                Box::new(|c: &mut StageTicketClaims| c.source_node_id[0] ^= 1),
                PeerSecurityError::Audience,
            ),
            (
                "follower fence",
                Box::new(|c: &mut StageTicketClaims| c.destination_fence += 1),
                PeerSecurityError::Fence,
            ),
            (
                "snapshot digest",
                Box::new(|c: &mut StageTicketClaims| c.snapshot_digest.push('x')),
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
                "attempt",
                Box::new(|c: &mut StageTicketClaims| c.attempt += 1),
                PeerSecurityError::Claims,
            ),
            (
                "reservation",
                Box::new(|c: &mut StageTicketClaims| c.reservation_id.push('x')),
                PeerSecurityError::Claims,
            ),
            (
                "permission digest",
                Box::new(|c: &mut StageTicketClaims| c.permission_digest.push('x')),
                PeerSecurityError::Claims,
            ),
            (
                "operation",
                Box::new(|c: &mut StageTicketClaims| {
                    c.operation = StageOperationV1::SetPlan.as_u32();
                }),
                PeerSecurityError::Operation,
            ),
            (
                "expired deadline",
                Box::new(|c: &mut StageTicketClaims| c.absolute_deadline_ms = 0),
                PeerSecurityError::Expired,
            ),
            (
                "expired context",
                Box::new(|c: &mut StageTicketClaims| c.expires_at_ms = 0),
                PeerSecurityError::Expired,
            ),
        ];

        for (name, mutate, expected) in mutations {
            let mut claims = stage_claims(&binding, body, now);
            mutate(&mut claims);
            assert_eq!(
                probe
                    .authorize_then_execute(
                        &authority,
                        &raw_stage_context(&claims),
                        &binding,
                        body,
                        now
                    )
                    .await
                    .err(),
                Some(expected),
                "a stage operation with a wrong {name} must be refused"
            );
            assert_eq!(
                probe.counts(),
                (0, 0, 0),
                "a refused {name} must not decode, touch the cache, or issue I/O"
            );
        }

        // A body that does not match the context digest is refused even though
        // every claims field is correct.
        let context = stage_claims(&binding, body, now)
            .to_context(StageOperationV1::ExecuteTask)
            .expect("the fixture coordinator encodes its own context");
        assert_eq!(
            probe
                .authorize_then_execute(&authority, &context, &binding, b"other-body", now)
                .await
                .err(),
            Some(PeerSecurityError::Body),
            "a body that does not match its context digest must be refused"
        );
        assert_eq!(probe.counts(), (0, 0, 0));

        // An oversized body is refused before it is hashed or decoded.
        assert_eq!(
            probe
                .authorize_then_execute(
                    &authority,
                    &context,
                    &binding,
                    &vec![0_u8; 8 * 1024 * 1024 + 1],
                    now
                )
                .await
                .err(),
            Some(PeerSecurityError::Body)
        );
        assert_eq!(probe.counts(), (0, 0, 0));

        // Undecodable and oversized contexts are refused before any decode.
        for malformed in [
            PeerContext {
                claims_bytes: vec![0xff],
            },
            PeerContext {
                claims_bytes: vec![0; MAX_STAGE_CLAIMS_BYTES + 1],
            },
        ] {
            assert_eq!(
                probe
                    .authorize_then_execute(&authority, &malformed, &binding, body, now)
                    .await
                    .err(),
                Some(PeerSecurityError::Malformed)
            );
        }
        assert_eq!(probe.counts(), (0, 0, 0));

        // Every refusal above committed exactly one durable audit row. Only
        // the two expiries, which follow a successful binding match, name the
        // bound tenant; every earlier refusal, including the forged foreign
        // tenant, is recorded on the system chain.
        let calls = audit.calls.lock().expect("audit mutex").clone();
        assert_eq!(calls.len(), 18, "each refusal audits exactly once");
        let attributed: Vec<_> = calls.iter().filter_map(|(tenant, _)| *tenant).collect();
        assert_eq!(attributed, vec![tenant_id, tenant_id]);

        let authorized = probe
            .authorize_then_execute(&authority, &context, &binding, body, now)
            .await
            .expect("a correct stage operation authorizes");
        assert_eq!(authorized.tenant_id, tenant_id);
        assert_eq!(probe.counts(), (1, 1, 1));
    }

    /// Every fragment-context refusal audits to the system chain: before the
    /// caller binds the context to its own query state, the named tenant is an
    /// unsigned claim that must not select a tenant audit chain.
    #[tokio::test]
    async fn oracle_peer_authority_audits_rejections_to_the_right_chain() {
        let (authority, audit) = authority();
        let worker = NodeId::new(uuid::Uuid::from_u128(1));
        let tenant_id = DataTenantId::new_v7();
        let now = Utc::now();
        let context = claims(worker, 7, tenant_id, now)
            .to_context()
            .expect("context");

        assert_eq!(
            authority.verify_before_decode(
                &PeerContext {
                    claims_bytes: vec![0xff]
                },
                worker,
                7,
                now
            ),
            Err(PeerSecurityError::Malformed)
        );
        assert_eq!(
            authority.verify_before_decode(
                &PeerContext {
                    claims_bytes: vec![0; MAX_CLAIMS_BYTES + 1]
                },
                worker,
                7,
                now
            ),
            Err(PeerSecurityError::Malformed)
        );
        assert_eq!(
            authority.verify_before_decode(&context, NodeId::new(uuid::Uuid::from_u128(9)), 7, now),
            Err(PeerSecurityError::Audience)
        );
        assert_eq!(
            authority.verify_before_decode(&context, worker, 8, now),
            Err(PeerSecurityError::Fence)
        );
        let expired = claims(worker, 7, tenant_id, now - chrono::Duration::seconds(20))
            .to_context()
            .expect("expired context");
        assert_eq!(
            authority.verify_before_decode(&expired, worker, 7, now),
            Err(PeerSecurityError::Expired)
        );
        let mut distant = claims(worker, 7, tenant_id, now);
        distant.expires_at_ms = (now + chrono::Duration::minutes(5)).timestamp_millis();
        assert_eq!(
            authority.verify_before_decode(
                &distant.to_context().expect("distant context"),
                worker,
                7,
                now
            ),
            Err(PeerSecurityError::Expired),
            "a context outliving the bounded window is refused"
        );
        authority
            .verify_before_decode(&context, worker, 7, now)
            .expect("a correctly addressed context is accepted");
        assert_eq!(
            *audit.calls.lock().expect("audit mutex"),
            vec![
                (None, BifrostSecurityViolationKind::PeerSignature),
                (None, BifrostSecurityViolationKind::PeerSignature),
                (None, BifrostSecurityViolationKind::PeerAudience),
                (None, BifrostSecurityViolationKind::PeerFence),
                (None, BifrostSecurityViolationKind::PeerReplay),
                (None, BifrostSecurityViolationKind::PeerReplay),
            ]
        );
    }

    /// Builds one fully bound v1 forwarding envelope for authority tests.
    fn forward_claims(
        worker: NodeId,
        worker_fence: u64,
        tenant_id: DataTenantId,
        now: DateTime<Utc>,
    ) -> ForwardQueryClaims {
        use wyrd_runtime::permission::PermissionSet;
        use wyrd_runtime::{Permission, Principal, PrincipalKind};
        use wyrd_spec::auth::PrincipalId;
        use wyrd_spec::request_id::RequestId;

        let principal = Principal::new(
            PrincipalId::new(uuid::Uuid::now_v7()),
            PrincipalKind::User,
            tenant_id,
            Vec::new(),
            PermissionSet::from_iter([Permission::bifrost_query_read()]),
        );
        let context = vala_bifrost_redux::oracle::AuthorizedQueryContext::try_new(
            principal,
            tenant_id,
            RequestId::now_v7(),
            None,
            wyrd_spec::vala::api::AuthMethod::Internal,
            Permission::bifrost_query_read(),
        )
        .expect("tenant-bound query context");
        ForwardQueryClaims {
            protocol_version: 1,
            audience: worker,
            worker_fence,
            expires_at_ms: (now + chrono::Duration::seconds(5)).timestamp_millis(),
            context,
            request: BifrostQueryRequest {
                sql: "SELECT value FROM vala.bifrost.events".to_owned(),
                deadline_ms: Some(5_000),
            },
            absolute_deadline_ms: (now + chrono::Duration::seconds(30)).timestamp_millis(),
        }
    }

    /// A forwarding context round-trips and is refused for a wrong audience,
    /// protocol version, stale fence, or expired envelope, each refusal audited
    /// on the system chain rather than the tenant the envelope claims.
    #[tokio::test]
    async fn forward_query_context_is_checked_against_the_receiver() {
        let (authority, audit) = authority();
        let worker = NodeId::new(uuid::Uuid::from_u128(71));
        let tenant = DataTenantId::new_v7();
        let now = Utc::now();
        let claims = forward_claims(worker, 11, tenant, now);

        let verified = authority
            .verify_forward_query(&claims.to_context().expect("context"), worker, 11, now)
            .expect("a correct forwarding envelope is authorized");
        assert_eq!(verified.context.data_tenant_id, tenant);
        assert_eq!(verified.context.request_id, claims.context.request_id);
        assert_eq!(verified.request, claims.request);
        assert_eq!(verified.absolute_deadline_ms, claims.absolute_deadline_ms);

        let refusals = [
            (
                forward_claims(NodeId::new(uuid::Uuid::from_u128(72)), 11, tenant, now),
                PeerSecurityError::Audience,
            ),
            (
                ForwardQueryClaims {
                    protocol_version: 2,
                    ..forward_claims(worker, 11, tenant, now)
                },
                PeerSecurityError::Audience,
            ),
            (
                forward_claims(worker, 10, tenant, now),
                PeerSecurityError::Fence,
            ),
            (
                ForwardQueryClaims {
                    expires_at_ms: (now - chrono::Duration::milliseconds(1)).timestamp_millis(),
                    ..forward_claims(worker, 11, tenant, now)
                },
                PeerSecurityError::Expired,
            ),
        ];
        for (refused, expected) in refusals {
            assert_eq!(
                authority
                    .verify_forward_query(&refused.to_context().expect("context"), worker, 11, now)
                    .expect_err("a misaddressed or expired envelope is refused"),
                expected
            );
        }
        assert_eq!(
            authority
                .verify_forward_query(
                    &PeerContext {
                        claims_bytes: b"{".to_vec()
                    },
                    worker,
                    11,
                    now
                )
                .expect_err("an undecodable envelope is refused"),
            PeerSecurityError::Malformed
        );
        let calls = audit.calls.lock().expect("audit mutex").clone();
        assert_eq!(calls.len(), 5, "each refusal audits exactly once");
        assert!(
            calls.iter().all(|(tenant, _)| tenant.is_none()),
            "a forged forwarding tenant never selects a tenant audit chain"
        );
    }
}
