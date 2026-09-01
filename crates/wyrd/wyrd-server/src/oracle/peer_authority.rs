//! Domain-separated Ed25519 authority for opaque Oracle peer tickets.

use chrono::{DateTime, Utc};
use secrecy::SecretString;
use std::fmt;
use std::sync::Arc;
use vala_bifrost_redux::oracle::peer::PeerReplayCache;
use vala_bifrost_redux::oracle::peer::{
    AuthorizedStage, OracleStageAuthority, PeerSecurityAudit, PeerSecurityError, PeerTicketClaims,
    PeerTicketMinter, PeerTicketVerifier, ReservationBinding, ReservationOperationV1,
    ReservationTicketClaims, StageBinding, StageOperationV1, StageTicketClaims,
    VerifiedClaimsBytes, reservation_body_digest, stage_body_digest,
};
use vala_bifrost_redux::oracle::telemetry::{
    AnalyticalStageAuthorityOutcome, record_stage_authority,
};
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{
    BifrostQueryRequest, BifrostSecurityViolationKind, NodeId, QueryClass, SignedPeerTicket,
};
use wyrd_tonic::prost::Message;

use crate::oracle::peer_keyring::PeerTicketKeyring;

/// Domain separator preventing peer signatures from crossing protocol boundaries.
const DOMAIN: &[u8] = b"wyrd.oracle.peer.v1\0";
/// Domain separator for authenticated public-query forwarding envelopes.
const FORWARD_QUERY_DOMAIN: &[u8] = b"wyrd.oracle.forward-query.v1\0";
/// Hard cap applied before any claims bytes are decoded.
const MAX_CLAIMS_BYTES: usize = 16 * 1024;
/// Hard cap applied to reservation claims before decoding; a reservation ticket
/// binds two fenced node identities, one query, and a body digest, so it is the
/// narrowest of the private claim shapes.
const MAX_RESERVATION_CLAIMS_BYTES: usize = 8 * 1024;
/// Hard cap applied to stage claims before decoding; stage claims are wider
/// than fragment claims because they bind two query identities, a stage, a
/// task, an attempt, and a reservation on top of the peer fields.
const MAX_STAGE_CLAIMS_BYTES: usize = 32 * 1024;
/// Query envelopes include the bounded public SQL request and immutable participant cut.
const MAX_FORWARD_QUERY_BYTES: usize = 128 * 1024;
/// Default bound on unexpired single-use ticket identities.
const DEFAULT_REPLAY_CAPACITY: usize = 1_024;
/// Maximum lifetime accepted for a newly presented ticket.
const DEFAULT_MAX_TICKET_TTL: chrono::Duration = chrono::Duration::seconds(30);

/// Signed authenticated ingress state consumed exactly once by a selected ready Oracle.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ForwardQueryClaims {
    /// Closed forwarding-envelope protocol version.
    pub protocol_version: u32,
    /// Exact selected Oracle audience.
    pub audience: NodeId,
    /// Exact selected Oracle role-incarnation fence.
    pub worker_fence: u64,
    /// Single-use replay identity.
    pub nonce: Vec<u8>,
    /// Short envelope acceptance expiry, distinct from the query deadline.
    pub expires_at_ms: i64,
    /// Original verified public caller context without bearer credentials.
    pub context: vala_bifrost_redux::oracle::AuthorizedQueryContext,
    /// Original validated public query request.
    pub request: BifrostQueryRequest,
    /// Server-derived class covered by the participant capability validation.
    pub query_class: QueryClass,
    /// Complete immutable participant cut used by every execution stage.
    pub participant_cut: vala_bifrost_redux::oracle::OracleQueryAttemptCut,
    /// Stable fingerprint redundantly bound for explicit receiver validation.
    pub participant_cut_fingerprint: String,
    /// Exact absolute query deadline, repeated for fail-closed envelope validation.
    pub absolute_deadline_ms: i64,
}

/// Server-owned Ed25519 authority for the private peer protocol.
pub struct OraclePeerAuthority {
    /// Independent peer-ticket keyring this authority signs and verifies with.
    ///
    /// Verification resolves the key the presented ticket names rather than
    /// pinning this process's own active key, which is what allows a rotation
    /// to publish a new key before switching issuance without refusing tickets
    /// a peer minted moments earlier under the retiring key.
    keyring: Arc<PeerTicketKeyring>,
    /// Role-local bounded single-use nonce owner.
    replay: Arc<PeerReplayCache>,
    /// Upper ticket lifetime bound checked after signature verification.
    max_ticket_ttl: chrono::Duration,
    /// Durable audit collaborator required before returning any rejection.
    security_audit: Arc<dyn PeerSecurityAudit>,
}

impl fmt::Debug for OraclePeerAuthority {
    /// Formats only the public key identifier; private, replay, and audit
    /// state is intentionally excluded from diagnostics and signing logs.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OraclePeerAuthority")
            .field("key_id", &self.keyring.active_key_id())
            .finish_non_exhaustive()
    }
}

impl OraclePeerAuthority {
    /// Signs one authenticated forwarding envelope without retaining a public bearer.
    ///
    /// # Errors
    /// Returns a closed encoding failure when the bounded envelope cannot be serialized.
    pub fn mint_forward_query(
        &self,
        claims: &ForwardQueryClaims,
    ) -> Result<SignedPeerTicket, PeerSecurityError> {
        let claims_bytes = serde_json::to_vec(claims).map_err(|_| PeerSecurityError::Encoding)?;
        if claims_bytes.is_empty() || claims_bytes.len() > MAX_FORWARD_QUERY_BYTES {
            return Err(PeerSecurityError::Encoding);
        }
        let signature = self.keyring.sign(&signing_input_for(
            FORWARD_QUERY_DOMAIN,
            self.keyring.active_key_id(),
            &claims_bytes,
        ));
        Ok(SignedPeerTicket {
            key_id: self.keyring.active_key_id().to_owned(),
            claims_bytes,
            signature,
        })
    }

    /// Verifies signature, audience, fence, expiry, replay, and tenant before decoding work.
    ///
    /// # Errors
    /// Returns a durably audited closed security error for every invalid envelope.
    pub async fn verify_forward_query(
        &self,
        ticket: &SignedPeerTicket,
        expected_worker: NodeId,
        expected_fence: u64,
        now: DateTime<Utc>,
    ) -> Result<ForwardQueryClaims, PeerSecurityError> {
        if ticket.signature.len() != 64
            || ticket.claims_bytes.is_empty()
            || ticket.claims_bytes.len() > MAX_FORWARD_QUERY_BYTES
        {
            return Err(self
                .forwarding_rejection(
                    None,
                    BifrostSecurityViolationKind::PeerSignature,
                    PeerSecurityError::InvalidSignature,
                )
                .await);
        }
        if self
            .keyring
            .verify(
                &ticket.key_id,
                &signing_input_for(FORWARD_QUERY_DOMAIN, &ticket.key_id, &ticket.claims_bytes),
                &ticket.signature,
                now,
            )
            .is_err()
        {
            return Err(self
                .forwarding_rejection(
                    None,
                    BifrostSecurityViolationKind::PeerSignature,
                    PeerSecurityError::InvalidSignature,
                )
                .await);
        }
        let claims: ForwardQueryClaims = match serde_json::from_slice(&ticket.claims_bytes) {
            Ok(claims) => claims,
            Err(_) => {
                return Err(self
                    .forwarding_rejection(
                        None,
                        BifrostSecurityViolationKind::PeerSignature,
                        PeerSecurityError::Claims,
                    )
                    .await);
            }
        };
        let tenant = claims.context.data_tenant_id;
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
        } else {
            None
        };
        if let Some((kind, error)) = violation {
            return Err(self.forwarding_rejection(Some(tenant), kind, error).await);
        }
        let max_expiry = now
            .checked_add_signed(self.max_ticket_ttl)
            .ok_or(PeerSecurityError::Expired)?;
        if claims.expires_at_ms <= now.timestamp_millis()
            || claims.expires_at_ms > max_expiry.timestamp_millis()
            || claims.nonce.len() < 16
        {
            return Err(self
                .forwarding_rejection(
                    Some(tenant),
                    BifrostSecurityViolationKind::PeerReplay,
                    PeerSecurityError::Expired,
                )
                .await);
        }
        let expires = DateTime::from_timestamp_millis(claims.expires_at_ms)
            .ok_or(PeerSecurityError::Expired)?;
        if let Err(error) = self
            .replay
            .consume(&ticket.key_id, &claims.nonce, expires, now)
        {
            return Err(self
                .forwarding_rejection(
                    Some(tenant),
                    BifrostSecurityViolationKind::PeerReplay,
                    error,
                )
                .await);
        }
        Ok(claims)
    }

    /// Audits one forwarding rejection and substitutes audit-unavailable on append failure.
    async fn forwarding_rejection(
        &self,
        tenant: Option<DataTenantId>,
        violation: BifrostSecurityViolationKind,
        error: PeerSecurityError,
    ) -> PeerSecurityError {
        let result = match tenant {
            Some(tenant) => {
                self.security_audit
                    .append_verified_ticket_violation(tenant, violation)
                    .await
            }
            None => {
                self.security_audit
                    .append_unverified_ticket_rejection(violation)
                    .await
            }
        };
        if result.is_err() {
            PeerSecurityError::AuditUnavailable
        } else {
            error
        }
    }

    /// Parses the configured Wyrd PKCS#8 key and derives the pinned key ID.
    ///
    /// # Errors
    /// Returns [`PeerSecurityError::InvalidSignature`] for malformed key material.
    pub fn from_pem(
        pem: &SecretString,
        security_audit: Arc<dyn PeerSecurityAudit>,
    ) -> Result<Self, PeerSecurityError> {
        Self::from_pem_with_limits(
            pem,
            DEFAULT_REPLAY_CAPACITY,
            DEFAULT_MAX_TICKET_TTL,
            security_audit,
        )
    }

    /// Parses the configured Wyrd key with explicit replay and ticket bounds.
    ///
    /// # Errors
    /// Returns a closed security failure for malformed key material or invalid bounds.
    pub fn from_pem_with_limits(
        pem: &SecretString,
        replay_capacity: usize,
        max_ticket_ttl: chrono::Duration,
        security_audit: Arc<dyn PeerSecurityAudit>,
    ) -> Result<Self, PeerSecurityError> {
        let keyring = PeerTicketKeyring::from_signing_key_pem(pem)
            .map_err(|_| PeerSecurityError::InvalidSignature)?;
        Self::from_keyring_with_limits(
            Arc::new(keyring),
            replay_capacity,
            max_ticket_ttl,
            security_audit,
        )
    }

    /// Composes the authority over this plane's independent ticket keyring.
    ///
    /// This is the production constructor. The keyring is loaded from the peer
    /// plane's own configured material, which is what keeps peer authority and
    /// north-south workload authority on separate keys.
    ///
    /// # Errors
    ///
    /// Returns [`PeerSecurityError::Claims`] when the replay or lifetime bound
    /// is not positive.
    pub fn from_keyring(
        keyring: Arc<PeerTicketKeyring>,
        security_audit: Arc<dyn PeerSecurityAudit>,
    ) -> Result<Self, PeerSecurityError> {
        Self::from_keyring_with_limits(
            keyring,
            DEFAULT_REPLAY_CAPACITY,
            DEFAULT_MAX_TICKET_TTL,
            security_audit,
        )
    }

    /// Composes the authority over a keyring with explicit bounds.
    ///
    /// # Errors
    ///
    /// Returns [`PeerSecurityError::Claims`] when the replay capacity is zero
    /// or the maximum ticket lifetime is not positive.
    pub fn from_keyring_with_limits(
        keyring: Arc<PeerTicketKeyring>,
        replay_capacity: usize,
        max_ticket_ttl: chrono::Duration,
        security_audit: Arc<dyn PeerSecurityAudit>,
    ) -> Result<Self, PeerSecurityError> {
        if replay_capacity == 0 || max_ticket_ttl <= chrono::Duration::zero() {
            return Err(PeerSecurityError::Claims);
        }
        Ok(Self {
            keyring,
            replay: Arc::new(PeerReplayCache::new(replay_capacity)),
            max_ticket_ttl,
            security_audit,
        })
    }

    /// Signs deterministic protobuf claims with a domain-separated input.
    ///
    /// # Errors
    /// Returns [`PeerSecurityError::Encoding`] when claims cannot be encoded.
    pub fn mint(&self, claims: &PeerTicketClaims) -> Result<SignedPeerTicket, PeerSecurityError> {
        let mut claims_bytes = Vec::new();
        Message::encode(claims, &mut claims_bytes).map_err(|_| PeerSecurityError::Encoding)?;
        let signature = self
            .keyring
            .sign(&signing_input(self.keyring.active_key_id(), &claims_bytes));
        Ok(SignedPeerTicket {
            key_id: self.keyring.active_key_id().to_owned(),
            claims_bytes,
            signature,
        })
    }

    /// Returns the public lowercase SHA-256 key identifier.
    #[must_use]
    pub fn key_id(&self) -> &str {
        self.keyring.active_key_id()
    }

    /// Verifies raw bytes before claims decoding or storage access.
    ///
    /// # Errors
    /// Returns a closed security error for key, signature, audience, fence, or expiry violations.
    pub async fn verify_before_decode(
        &self,
        ticket: &SignedPeerTicket,
        expected_worker: NodeId,
        expected_fence: u64,
        now: DateTime<Utc>,
    ) -> Result<VerifiedClaimsBytes, PeerSecurityError> {
        if ticket.signature.len() != 64 {
            return self
                .reject_unverified(
                    BifrostSecurityViolationKind::PeerSignature,
                    PeerSecurityError::InvalidSignature,
                )
                .await;
        }
        if ticket.claims_bytes.is_empty() || ticket.claims_bytes.len() > MAX_CLAIMS_BYTES {
            return self
                .reject_unverified(
                    BifrostSecurityViolationKind::PeerSignature,
                    PeerSecurityError::InvalidSignature,
                )
                .await;
        }
        if let Err(error) = self.keyring.verify(
            &ticket.key_id,
            &signing_input(&ticket.key_id, &ticket.claims_bytes),
            &ticket.signature,
            now,
        ) {
            let violation = match error {
                PeerSecurityError::UnknownKey => BifrostSecurityViolationKind::PeerUnknownKey,
                _ => BifrostSecurityViolationKind::PeerSignature,
            };
            return self.reject_unverified(violation, error).await;
        }
        let claims = match PeerTicketClaims::decode(ticket.claims_bytes.as_slice()) {
            Ok(claims) => claims,
            Err(_) => {
                return self
                    .reject_unverified(
                        BifrostSecurityViolationKind::PeerSignature,
                        PeerSecurityError::InvalidSignature,
                    )
                    .await;
            }
        };
        let tenant_id = match uuid::Uuid::from_slice(&claims.tenant_id)
            .ok()
            .and_then(|tenant| DataTenantId::new(tenant).ok())
        {
            Some(tenant_id) => tenant_id,
            None => {
                return self
                    .reject_unverified(
                        BifrostSecurityViolationKind::PeerTenant,
                        PeerSecurityError::Claims,
                    )
                    .await;
            }
        };
        if claims.audience != expected_worker.as_uuid().as_bytes() {
            return self
                .reject_verified(
                    tenant_id,
                    BifrostSecurityViolationKind::PeerAudience,
                    PeerSecurityError::Audience,
                )
                .await;
        }
        if claims.worker_fence != expected_fence {
            return self
                .reject_verified(
                    tenant_id,
                    BifrostSecurityViolationKind::PeerFence,
                    PeerSecurityError::Fence,
                )
                .await;
        }
        let max_expiry = now
            .checked_add_signed(self.max_ticket_ttl)
            .ok_or(PeerSecurityError::Expired)?;
        if claims.expires_at_ms <= now.timestamp_millis()
            || claims.expires_at_ms > max_expiry.timestamp_millis()
            || claims.nonce.len() < 16
        {
            return self
                .reject_verified(
                    tenant_id,
                    BifrostSecurityViolationKind::PeerReplay,
                    PeerSecurityError::Expired,
                )
                .await;
        }
        let expires = chrono::DateTime::from_timestamp_millis(claims.expires_at_ms)
            .ok_or(PeerSecurityError::Expired)?;
        if let Err(error) = self
            .replay
            .consume(&ticket.key_id, &claims.nonce, expires, now)
        {
            return self
                .reject_verified(tenant_id, BifrostSecurityViolationKind::PeerReplay, error)
                .await;
        }
        Ok(VerifiedClaimsBytes(ticket.claims_bytes.clone()))
    }

    /// Commits a platform/system audit before returning an untrusted-ticket rejection.
    ///
    /// # Errors
    /// Returns [`PeerSecurityError::AuditUnavailable`] when the required row cannot commit;
    /// otherwise returns the original closed rejection.
    async fn reject_unverified<T>(
        &self,
        violation: BifrostSecurityViolationKind,
        error: PeerSecurityError,
    ) -> Result<T, PeerSecurityError> {
        self.security_audit
            .append_unverified_ticket_rejection(violation)
            .await
            .map_err(|_| PeerSecurityError::AuditUnavailable)?;
        Err(error)
    }

    /// Commits a signed-tenant audit before returning a verified-ticket rejection.
    ///
    /// # Errors
    /// Returns [`PeerSecurityError::AuditUnavailable`] when the required row cannot commit;
    /// otherwise returns the original closed rejection.
    async fn reject_verified(
        &self,
        tenant_id: DataTenantId,
        violation: BifrostSecurityViolationKind,
        error: PeerSecurityError,
    ) -> Result<VerifiedClaimsBytes, PeerSecurityError> {
        self.security_audit
            .append_verified_ticket_violation(tenant_id, violation)
            .await
            .map_err(|_| PeerSecurityError::AuditUnavailable)?;
        Err(error)
    }
}

#[async_trait::async_trait]
impl OracleStageAuthority for OraclePeerAuthority {
    /// Signs one stage ticket through the server-owned Ed25519 authority.
    ///
    /// # Errors
    ///
    /// Returns the closed operation-mismatch or encoding failure.
    fn mint_stage(
        &self,
        operation: StageOperationV1,
        claims: &StageTicketClaims,
    ) -> Result<SignedPeerTicket, PeerSecurityError> {
        self.mint_stage_inner(operation, claims)
    }

    /// Authorizes one stage operation before any decode, cache read, or I/O.
    ///
    /// # Errors
    ///
    /// Returns the closed failure for the first check that did not pass, or an
    /// audit-unavailable refusal when the required durable row cannot commit.
    async fn authorize_stage(
        &self,
        ticket: &SignedPeerTicket,
        binding: &StageBinding,
        body: &[u8],
        now: DateTime<Utc>,
    ) -> Result<AuthorizedStage, PeerSecurityError> {
        self.authorize_stage_inner(ticket, binding, body, now).await
    }
}

impl vala_bifrost_redux::oracle::peer::ReservationTicketMinter for OraclePeerAuthority {
    /// Signs one reservation ticket through the authority's own keyring.
    ///
    /// # Errors
    ///
    /// Returns the closed encoding or operation rejection from
    /// [`OraclePeerAuthority::mint_reservation`].
    fn mint_reservation_ticket(
        &self,
        operation: ReservationOperationV1,
        claims: &ReservationTicketClaims,
    ) -> Result<SignedPeerTicket, PeerSecurityError> {
        self.mint_reservation(operation, claims)
    }
}

#[async_trait::async_trait]
impl PeerTicketVerifier for OraclePeerAuthority {
    /// Verifies one raw ticket through the server-owned Ed25519 authority.
    ///
    /// # Errors
    /// Returns a closed signature, claim, expiry, fence, audience, or replay failure.
    async fn verify_peer_ticket(
        &self,
        ticket: &SignedPeerTicket,
        expected_worker: NodeId,
        expected_worker_fence: u64,
        now: DateTime<Utc>,
    ) -> Result<VerifiedClaimsBytes, PeerSecurityError> {
        self.verify_before_decode(ticket, expected_worker, expected_worker_fence, now)
            .await
    }
}

impl PeerTicketMinter for OraclePeerAuthority {
    /// Implements the narrow Redux ticket-minting capability.
    ///
    /// # Errors
    /// Returns [`PeerSecurityError::Encoding`] when claims cannot be encoded.
    fn mint_peer_ticket(
        &self,
        claims: &PeerTicketClaims,
    ) -> Result<SignedPeerTicket, PeerSecurityError> {
        self.mint(claims)
    }
}

/// Builds the stable domain-separated signing preimage.
///
/// The byte order is fixed as `DOMAIN || key_id || protobuf claims`; changing
/// it invalidates every issued peer ticket and must therefore be versioned.
fn signing_input(key_id: &str, claims: &[u8]) -> Vec<u8> {
    signing_input_for(DOMAIN, key_id, claims)
}

impl OraclePeerAuthority {
    /// Signs one single-use ticket for exactly one reservation operation.
    ///
    /// The signature is produced over the operation's own domain separator, so
    /// a reserve ticket cannot be presented as a release ticket even with
    /// identical claims bytes: the receiver checks the domain its own entry
    /// point implements, not one named in the message.
    ///
    /// # Errors
    ///
    /// Returns [`PeerSecurityError::Operation`] when the claims name a
    /// different operation than the one being signed, and
    /// [`PeerSecurityError::Encoding`] when the claims cannot be encoded or
    /// exceed [`MAX_RESERVATION_CLAIMS_BYTES`].
    pub fn mint_reservation(
        &self,
        operation: ReservationOperationV1,
        claims: &ReservationTicketClaims,
    ) -> Result<SignedPeerTicket, PeerSecurityError> {
        if ReservationOperationV1::from_u32(claims.operation) != Some(operation) {
            return Err(PeerSecurityError::Operation);
        }
        let mut claims_bytes = Vec::new();
        Message::encode(claims, &mut claims_bytes).map_err(|_| PeerSecurityError::Encoding)?;
        if claims_bytes.is_empty() || claims_bytes.len() > MAX_RESERVATION_CLAIMS_BYTES {
            return Err(PeerSecurityError::Encoding);
        }
        let signature = self.keyring.sign(&signing_input_for(
            operation.domain(),
            self.keyring.active_key_id(),
            &claims_bytes,
        ));
        Ok(SignedPeerTicket {
            key_id: self.keyring.active_key_id().to_owned(),
            claims_bytes,
            signature,
        })
    }

    /// Authorizes exactly one reservation operation before it changes state.
    ///
    /// The order is deliberate and is what makes the check meaningful: bounds,
    /// then signature under the receiver's own operation domain, then the body
    /// digest over the exact bytes received, then every bound identity against
    /// the receiver-derived binding, then expiry, then single-use nonce
    /// consumption. No reservation is taken or released before all of it
    /// passes, so a replayed release cannot cancel capacity and a replayed
    /// reserve cannot charge a follower twice.
    ///
    /// The body is the encoded request with its ticket field cleared, which is
    /// what both sides digest; a substituted request carrying a valid ticket
    /// therefore fails on the digest rather than on any downstream field.
    ///
    /// Rejections audit on the system chain: a reservation is a control-plane
    /// operation between Oracles and binds no data tenant to attribute to.
    ///
    /// # Errors
    ///
    /// Returns the audited closed rejection for malformed, unknown-key,
    /// wrong-domain, wrong-body, misbound, expired, or replayed tickets, and
    /// [`PeerSecurityError::AuditUnavailable`] when the rejection itself cannot
    /// be recorded.
    pub async fn verify_reservation(
        &self,
        ticket: &SignedPeerTicket,
        binding: &ReservationBinding,
        body: &[u8],
        now: DateTime<Utc>,
    ) -> Result<ReservationTicketClaims, PeerSecurityError> {
        if ticket.signature.len() != 64
            || ticket.claims_bytes.is_empty()
            || ticket.claims_bytes.len() > MAX_RESERVATION_CLAIMS_BYTES
        {
            return self
                .reject_unverified(
                    BifrostSecurityViolationKind::PeerSignature,
                    PeerSecurityError::InvalidSignature,
                )
                .await;
        }
        if let Err(error) = self.keyring.verify(
            &ticket.key_id,
            &signing_input_for(
                binding.operation.domain(),
                &ticket.key_id,
                &ticket.claims_bytes,
            ),
            &ticket.signature,
            now,
        ) {
            let violation = match error {
                PeerSecurityError::UnknownKey => BifrostSecurityViolationKind::PeerUnknownKey,
                _ => BifrostSecurityViolationKind::PeerSignature,
            };
            return self.reject_unverified(violation, error).await;
        }
        let body_digest = match reservation_body_digest(body) {
            Ok(digest) => digest,
            Err(error) => {
                return self
                    .reject_unverified(BifrostSecurityViolationKind::PeerFragment, error)
                    .await;
            }
        };
        let Ok(claims) = ReservationTicketClaims::decode(ticket.claims_bytes.as_slice()) else {
            return self
                .reject_unverified(
                    BifrostSecurityViolationKind::PeerSignature,
                    PeerSecurityError::InvalidSignature,
                )
                .await;
        };
        if let Err(error) = claims.verify_binding(binding, &body_digest) {
            let violation = match error {
                PeerSecurityError::Audience => BifrostSecurityViolationKind::PeerAudience,
                PeerSecurityError::Fence => BifrostSecurityViolationKind::PeerFence,
                PeerSecurityError::Body => BifrostSecurityViolationKind::PeerFragment,
                _ => BifrostSecurityViolationKind::PeerStageBinding,
            };
            return self.reject_unverified(violation, error).await;
        }
        let max_expiry = now
            .checked_add_signed(self.max_ticket_ttl)
            .ok_or(PeerSecurityError::Expired)?;
        if claims.expires_at_ms <= now.timestamp_millis()
            || claims.expires_at_ms > max_expiry.timestamp_millis()
            || claims.nonce.len() < 16
        {
            return self
                .reject_unverified(
                    BifrostSecurityViolationKind::PeerReplay,
                    PeerSecurityError::Expired,
                )
                .await;
        }
        let expires = chrono::DateTime::from_timestamp_millis(claims.expires_at_ms)
            .ok_or(PeerSecurityError::Expired)?;
        if let Err(error) = self
            .replay
            .consume(&ticket.key_id, &claims.nonce, expires, now)
        {
            return self
                .reject_unverified(BifrostSecurityViolationKind::PeerReplay, error)
                .await;
        }
        Ok(claims)
    }

    /// Signs one single-use ticket for exactly one Analytical stage operation.
    ///
    /// The signature is produced over the operation's own domain separator, so
    /// a `SetPlan` ticket cannot be presented as an `ExecuteTask` ticket even
    /// with identical claims bytes: the receiver checks the domain its own
    /// entry point implements, not one named in the message.
    ///
    /// # Errors
    ///
    /// Returns [`PeerSecurityError::Operation`] when the claims name a
    /// different operation than the one being signed, and
    /// [`PeerSecurityError::Encoding`] when the claims cannot be encoded or
    /// exceed [`MAX_STAGE_CLAIMS_BYTES`].
    fn mint_stage_inner(
        &self,
        operation: StageOperationV1,
        claims: &StageTicketClaims,
    ) -> Result<SignedPeerTicket, PeerSecurityError> {
        if StageOperationV1::from_u32(claims.operation) != Some(operation) {
            return Err(PeerSecurityError::Operation);
        }
        let mut claims_bytes = Vec::new();
        Message::encode(claims, &mut claims_bytes).map_err(|_| PeerSecurityError::Encoding)?;
        if claims_bytes.is_empty() || claims_bytes.len() > MAX_STAGE_CLAIMS_BYTES {
            return Err(PeerSecurityError::Encoding);
        }
        let signature = self.keyring.sign(&signing_input_for(
            operation.domain(),
            self.keyring.active_key_id(),
            &claims_bytes,
        ));
        Ok(SignedPeerTicket {
            key_id: self.keyring.active_key_id().to_owned(),
            claims_bytes,
            signature,
        })
    }

    /// Authorizes one stage operation before any decode, cache access, or I/O.
    ///
    /// The order here is the security property, not an implementation detail:
    ///
    /// 1. key identity, signature length, and the claims-byte bound — no
    ///    attacker-controlled length reaches a parser;
    /// 2. the Ed25519 signature over this operation's own domain;
    /// 3. the bounded raw body's digest, so the bytes about to be decoded are
    ///    exactly the bytes that were signed for;
    /// 4. claims decode, then tenant extraction from the *verified* bytes;
    /// 5. a field-by-field match against the receiver's own [`StageBinding`];
    /// 6. the absolute query deadline and the ticket's own short expiry; and
    /// 7. single-use nonce consumption, last, so a request that fails any
    ///    earlier check cannot burn a nonce a legitimate retry still needs.
    ///
    /// Only after all seven does the caller learn the operation is authorized.
    /// Every refusal commits a durable audit row before it returns, and emits
    /// closed-label stage-authority telemetry.
    ///
    /// # Errors
    ///
    /// Returns [`PeerSecurityError::UnknownKey`], `InvalidSignature`, `Body`,
    /// `Operation`, `Audience`, `Fence`, `Claims`, `Expired`, `Replay`,
    /// `ReplayCapacity`, or [`PeerSecurityError::AuditUnavailable`] when the
    /// required audit row cannot commit. A rejection never returns claims.
    async fn authorize_stage_inner(
        &self,
        ticket: &SignedPeerTicket,
        binding: &StageBinding,
        body: &[u8],
        now: DateTime<Utc>,
    ) -> Result<AuthorizedStage, PeerSecurityError> {
        if ticket.signature.len() != 64
            || ticket.claims_bytes.is_empty()
            || ticket.claims_bytes.len() > MAX_STAGE_CLAIMS_BYTES
        {
            return self
                .reject_stage_unverified(
                    binding.operation,
                    BifrostSecurityViolationKind::PeerSignature,
                    PeerSecurityError::InvalidSignature,
                    AnalyticalStageAuthorityOutcome::Signature,
                )
                .await;
        }
        if let Err(error) = self.keyring.verify(
            &ticket.key_id,
            &signing_input_for(
                binding.operation.domain(),
                &ticket.key_id,
                &ticket.claims_bytes,
            ),
            &ticket.signature,
            now,
        ) {
            let violation = match error {
                PeerSecurityError::UnknownKey => BifrostSecurityViolationKind::PeerUnknownKey,
                _ => BifrostSecurityViolationKind::PeerSignature,
            };
            return self
                .reject_stage_unverified(
                    binding.operation,
                    violation,
                    error,
                    AnalyticalStageAuthorityOutcome::Signature,
                )
                .await;
        }
        let body_digest = match stage_body_digest(body) {
            Ok(digest) => digest,
            Err(error) => {
                return self
                    .reject_stage_unverified(
                        binding.operation,
                        BifrostSecurityViolationKind::PeerFragment,
                        error,
                        AnalyticalStageAuthorityOutcome::Body,
                    )
                    .await;
            }
        };
        let Ok(claims) = StageTicketClaims::decode(ticket.claims_bytes.as_slice()) else {
            return self
                .reject_stage_unverified(
                    binding.operation,
                    BifrostSecurityViolationKind::PeerSignature,
                    PeerSecurityError::InvalidSignature,
                    AnalyticalStageAuthorityOutcome::Signature,
                )
                .await;
        };
        let Some(tenant_id) = uuid::Uuid::from_slice(&claims.tenant_id)
            .ok()
            .and_then(|tenant| DataTenantId::new(tenant).ok())
        else {
            return self
                .reject_stage_unverified(
                    binding.operation,
                    BifrostSecurityViolationKind::PeerTenant,
                    PeerSecurityError::Claims,
                    AnalyticalStageAuthorityOutcome::Binding,
                )
                .await;
        };
        if let Err(error) = claims.verify_binding(binding, &body_digest) {
            let (violation, outcome) = stage_binding_violation(&error);
            return self
                .reject_stage_verified(binding.operation, tenant_id, violation, error, outcome)
                .await;
        }
        if claims.absolute_deadline_ms <= now.timestamp_millis() {
            return self
                .reject_stage_verified(
                    binding.operation,
                    tenant_id,
                    BifrostSecurityViolationKind::PeerStageBinding,
                    PeerSecurityError::Expired,
                    AnalyticalStageAuthorityOutcome::Expired,
                )
                .await;
        }
        let max_expiry = now
            .checked_add_signed(self.max_ticket_ttl)
            .ok_or(PeerSecurityError::Expired)?;
        if claims.expires_at_ms <= now.timestamp_millis()
            || claims.expires_at_ms > max_expiry.timestamp_millis()
            || claims.nonce.len() < 16
        {
            return self
                .reject_stage_verified(
                    binding.operation,
                    tenant_id,
                    BifrostSecurityViolationKind::PeerReplay,
                    PeerSecurityError::Expired,
                    AnalyticalStageAuthorityOutcome::Expired,
                )
                .await;
        }
        let expires = chrono::DateTime::from_timestamp_millis(claims.expires_at_ms)
            .ok_or(PeerSecurityError::Expired)?;
        if let Err(error) = self
            .replay
            .consume(&ticket.key_id, &claims.nonce, expires, now)
        {
            return self
                .reject_stage_verified(
                    binding.operation,
                    tenant_id,
                    BifrostSecurityViolationKind::PeerReplay,
                    error,
                    AnalyticalStageAuthorityOutcome::Replay,
                )
                .await;
        }
        record_stage_authority(
            binding.operation.telemetry(),
            AnalyticalStageAuthorityOutcome::Authorized,
        );
        Ok(AuthorizedStage { claims, tenant_id })
    }

    /// Audits and counts a stage rejection with no cryptographically known tenant.
    ///
    /// # Errors
    /// Returns [`PeerSecurityError::AuditUnavailable`] when the system-chain row
    /// cannot commit; otherwise returns the original closed rejection.
    async fn reject_stage_unverified(
        &self,
        operation: StageOperationV1,
        violation: BifrostSecurityViolationKind,
        error: PeerSecurityError,
        outcome: AnalyticalStageAuthorityOutcome,
    ) -> Result<AuthorizedStage, PeerSecurityError> {
        record_stage_authority(operation.telemetry(), outcome);
        self.security_audit
            .append_unverified_ticket_rejection(violation)
            .await
            .map_err(|_| PeerSecurityError::AuditUnavailable)?;
        Err(error)
    }

    /// Audits and counts a stage rejection against the verified tenant chain.
    ///
    /// # Errors
    /// Returns [`PeerSecurityError::AuditUnavailable`] when the tenant-scoped row
    /// cannot commit; otherwise returns the original closed rejection.
    async fn reject_stage_verified(
        &self,
        operation: StageOperationV1,
        tenant_id: DataTenantId,
        violation: BifrostSecurityViolationKind,
        error: PeerSecurityError,
        outcome: AnalyticalStageAuthorityOutcome,
    ) -> Result<AuthorizedStage, PeerSecurityError> {
        record_stage_authority(operation.telemetry(), outcome);
        self.security_audit
            .append_verified_ticket_violation(tenant_id, violation)
            .await
            .map_err(|_| PeerSecurityError::AuditUnavailable)?;
        Err(error)
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

/// Builds a signing preimage for one closed private protocol domain.
fn signing_input_for(domain: &[u8], key_id: &str, claims: &[u8]) -> Vec<u8> {
    vala_bifrost_redux::oracle::peer::peer_signing_input(domain, key_id, claims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use vala_bifrost_redux::oracle::peer::{PeerSecurityAudit, PeerSecurityAuditError};

    /// Fixed PKCS#8 fixture used to lock the public-key digest vector.
    const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
    /// Expected lowercase SHA-256 digest of the fixture public key.
    const KEY_ID: &str = "22b9898f3a934b6287f51261c9de3ed2fd096d798c32f9fdd37f77f37cf744e3";

    /// One captured audit call, where `None` denotes the system chain.
    type AuditCall = (Option<DataTenantId>, BifrostSecurityViolationKind);

    /// Captures the two audit paths without replacing authority verification.
    #[derive(Default)]
    struct RecordingPeerAudit {
        /// Ordered audit calls.
        calls: Mutex<Vec<AuditCall>>,
    }

    /// Refuses every append to prove security rejection remains fail-closed.
    struct FailingPeerAudit;

    #[async_trait::async_trait]
    impl PeerSecurityAudit for FailingPeerAudit {
        /// Refuses the system-chain append.
        ///
        /// # Errors
        /// Always returns [`PeerSecurityAuditError`] to exercise fail-closed behavior.
        async fn append_unverified_ticket_rejection(
            &self,
            _violation: BifrostSecurityViolationKind,
        ) -> Result<(), PeerSecurityAuditError> {
            Err(PeerSecurityAuditError)
        }

        /// Refuses the tenant-chain append.
        ///
        /// # Errors
        /// Always returns [`PeerSecurityAuditError`] to exercise fail-closed behavior.
        async fn append_verified_ticket_violation(
            &self,
            _tenant_id: DataTenantId,
            _violation: BifrostSecurityViolationKind,
        ) -> Result<(), PeerSecurityAuditError> {
            Err(PeerSecurityAuditError)
        }
    }

    #[async_trait::async_trait]
    impl PeerSecurityAudit for RecordingPeerAudit {
        /// Captures an unverified rejection as a system-chain call.
        ///
        /// # Errors
        /// This recorder never fails.
        async fn append_unverified_ticket_rejection(
            &self,
            violation: BifrostSecurityViolationKind,
        ) -> Result<(), PeerSecurityAuditError> {
            self.calls
                .lock()
                .expect("audit mutex")
                .push((None, violation));
            Ok(())
        }

        /// Captures a verified rejection with its trusted tenant.
        ///
        /// # Errors
        /// This recorder never fails.
        async fn append_verified_ticket_violation(
            &self,
            tenant_id: DataTenantId,
            violation: BifrostSecurityViolationKind,
        ) -> Result<(), PeerSecurityAuditError> {
            self.calls
                .lock()
                .expect("audit mutex")
                .push((Some(tenant_id), violation));
            Ok(())
        }
    }

    /// Creates an authority and its recording audit collaborator.
    fn authority() -> (OraclePeerAuthority, Arc<RecordingPeerAudit>) {
        let audit = Arc::new(RecordingPeerAudit::default());
        let authority =
            OraclePeerAuthority::from_pem(&SecretString::from(PRIVATE_KEY_PEM), audit.clone())
                .expect("authority");
        (authority, audit)
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

    /// Mints one correct reservation ticket over `body`.
    fn reservation_ticket(
        authority: &OraclePeerAuthority,
        binding: &ReservationBinding,
        body: &[u8],
    ) -> SignedPeerTicket {
        let claims = ReservationTicketClaims::for_binding(
            binding,
            vala_bifrost_redux::oracle::peer::reservation_body_digest(body)
                .expect("reservation body digest"),
            uuid::Uuid::new_v4().as_bytes().to_vec(),
            (Utc::now() + chrono::Duration::seconds(5)).timestamp_millis(),
        );
        authority
            .mint_reservation(binding.operation, &claims)
            .expect("reservation ticket")
    }

    /// A reservation ticket authorizes one operation, one body, and one use.
    ///
    /// Locks the three properties a capacity change depends on: the release
    /// domain does not verify at the reserve entry point even with identical
    /// claims, a substituted request body is refused while every identity still
    /// matches, and the same correct ticket is accepted exactly once.
    #[tokio::test]
    async fn reservation_tickets_are_operation_body_and_use_exact() {
        let (authority, _audit) = authority();
        let binding = reservation_binding();
        let body = b"reserve-request".as_slice();
        let ticket = reservation_ticket(&authority, &binding, body);

        authority
            .verify_reservation(&ticket, &binding, body, Utc::now())
            .await
            .expect("a correct reservation ticket is authorized");
        assert_eq!(
            authority
                .verify_reservation(&ticket, &binding, body, Utc::now())
                .await
                .expect_err("a replayed reservation ticket is refused"),
            PeerSecurityError::Replay
        );

        let fresh = reservation_ticket(&authority, &binding, body);
        assert_eq!(
            authority
                .verify_reservation(&fresh, &binding, b"substituted-request", Utc::now())
                .await
                .expect_err("a substituted body is refused"),
            PeerSecurityError::Body
        );

        let release = ReservationBinding {
            operation: ReservationOperationV1::ReleaseSlots,
            ..reservation_binding()
        };
        let released = reservation_ticket(&authority, &release, body);
        assert_eq!(
            authority
                .verify_reservation(&released, &binding, body, Utc::now())
                .await
                .expect_err("a release ticket does not authorize a reserve"),
            PeerSecurityError::InvalidSignature
        );
    }

    /// Builds one fully bound v2 claim for authority tests.
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
            nonce: uuid::Uuid::from_u128(6).as_bytes().to_vec(),
            expires_at_ms: (now + chrono::Duration::seconds(10)).timestamp_millis(),
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
            ticket: &SignedPeerTicket,
            binding: &StageBinding,
            body: &[u8],
            now: DateTime<Utc>,
        ) -> Result<AuthorizedStage, PeerSecurityError> {
            let authorized = authority
                .authorize_stage(ticket, binding, body, now)
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

    /// Builds claims for one binding with a fresh single-use nonce.
    fn stage_claims(
        binding: &StageBinding,
        body: &[u8],
        nonce: uuid::Uuid,
        now: DateTime<Utc>,
    ) -> StageTicketClaims {
        StageTicketClaims::for_binding(
            binding,
            stage_body_digest(body).expect("a bounded fixture body digests"),
            nonce.as_bytes().to_vec(),
            (now + chrono::Duration::seconds(20)).timestamp_millis(),
            (now + chrono::Duration::seconds(10)).timestamp_millis(),
            Vec::new(),
        )
    }

    /// A stage operation is refused before any decode, cache access, or I/O.
    ///
    /// This is the ordering gate for the whole Analytical path. Each negative
    /// binding is exercised independently — including the public and DataFusion
    /// query identities separately, which is what stops a sibling distributed
    /// graph under the same public query from borrowing another graph's ticket.
    ///
    /// Nonce consumption is asserted to happen *last* by a production-observable
    /// route rather than by inspecting private state: after every refusal, a
    /// correct ticket reusing that same nonce still authorizes. If nonce
    /// consumption moved ahead of the signature, body, or binding checks, a
    /// single forged request would burn a nonce a legitimate operation needs,
    /// and this assertion would fail.
    ///
    /// # Panics
    ///
    /// Panics when any negative binding is accepted, when a refusal performs a
    /// gated effect, or when a refusal consumes the nonce.
    #[tokio::test]
    async fn stage_authority_rejects_before_decode_cache_or_io() {
        let (authority, audit) = authority();
        let tenant_id = DataTenantId::new_v7();
        let binding = stage_binding(tenant_id);
        let body = b"stage-operation-body".as_slice();
        let now = Utc::now();
        let probe = StageEffectProbe::default();

        let mutations: Vec<StageMutation> = vec![
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
                "expired deadline",
                Box::new(|c: &mut StageTicketClaims| c.absolute_deadline_ms = 0),
                PeerSecurityError::Expired,
            ),
        ];

        // The nonce is shared across every refusal below, so a refusal that
        // consumed it would break the final authorization.
        let nonce = uuid::Uuid::new_v4();
        for (name, mutate, expected) in mutations {
            let mut claims = stage_claims(&binding, body, nonce, now);
            mutate(&mut claims);
            let ticket = authority
                .mint_stage(StageOperationV1::ExecuteTask, &claims)
                .expect("the fixture coordinator signs its own claims");
            assert_eq!(
                probe
                    .authorize_then_execute(&authority, &ticket, &binding, body, now)
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

        // A body that does not match the signed digest is refused even though
        // every claims field is correct.
        let claims = stage_claims(&binding, body, nonce, now);
        let ticket = authority
            .mint_stage(StageOperationV1::ExecuteTask, &claims)
            .expect("the fixture coordinator signs its own claims");
        assert_eq!(
            probe
                .authorize_then_execute(&authority, &ticket, &binding, b"other-body", now)
                .await
                .err(),
            Some(PeerSecurityError::Body),
            "a body that does not match its signed digest must be refused"
        );
        assert_eq!(probe.counts(), (0, 0, 0));

        // An oversized body is refused before it is hashed or decoded.
        assert_eq!(
            probe
                .authorize_then_execute(
                    &authority,
                    &ticket,
                    &binding,
                    &vec![0_u8; 8 * 1024 * 1024 + 1],
                    now
                )
                .await
                .err(),
            Some(PeerSecurityError::Body)
        );
        assert_eq!(probe.counts(), (0, 0, 0));

        // A ticket minted for the sibling operation does not verify here: the
        // receiver checks the domain its own entry point implements.
        let mut set_plan_binding = binding.clone();
        set_plan_binding.operation = StageOperationV1::SetPlan;
        set_plan_binding.task_id = None;
        let set_plan_claims = stage_claims(&set_plan_binding, body, nonce, now);
        let set_plan_ticket = authority
            .mint_stage(StageOperationV1::SetPlan, &set_plan_claims)
            .expect("the fixture coordinator signs its own claims");
        assert_eq!(
            probe
                .authorize_then_execute(&authority, &set_plan_ticket, &binding, body, now)
                .await
                .err(),
            Some(PeerSecurityError::InvalidSignature),
            "a set-plan ticket must not authorize an execute-task operation"
        );
        assert_eq!(probe.counts(), (0, 0, 0));

        // A tampered signature is refused before anything is decoded.
        let mut tampered = ticket.clone();
        tampered.signature[0] ^= 1;
        assert_eq!(
            probe
                .authorize_then_execute(&authority, &tampered, &binding, body, now)
                .await
                .err(),
            Some(PeerSecurityError::InvalidSignature)
        );
        assert_eq!(probe.counts(), (0, 0, 0));

        // Every refusal above committed exactly one durable audit row.
        let rejections = audit.calls.lock().expect("audit mutex").len();
        assert_eq!(rejections, 15, "each refusal audits exactly once");

        // The shared nonce survived every refusal: consumption is last.
        let authorized = probe
            .authorize_then_execute(&authority, &ticket, &binding, body, now)
            .await
            .expect("a correct stage operation authorizes on the reused nonce");
        assert_eq!(authorized.tenant_id, tenant_id);
        assert_eq!(probe.counts(), (1, 1, 1));

        // And it is single-use: the same ticket cannot be replayed.
        assert_eq!(
            probe
                .authorize_then_execute(&authority, &ticket, &binding, body, now)
                .await
                .err(),
            Some(PeerSecurityError::Replay)
        );
        assert_eq!(
            probe.counts(),
            (1, 1, 1),
            "a replayed operation must not decode, touch the cache, or issue I/O"
        );
    }

    /// The pinned fixture derives the exact lowercase raw-public-key digest.
    #[test]
    fn oracle_peer_authority_derives_exact_key_id_vector() {
        let (authority, _) = authority();
        assert_eq!(authority.key_id(), KEY_ID);
    }

    /// Claims/signature tamper and unknown keys audit only to the system chain.
    #[tokio::test]
    async fn oracle_peer_authority_audits_untrusted_ticket_without_claimed_tenant() {
        let (authority, audit) = authority();
        let worker = NodeId::new(uuid::Uuid::from_u128(1));
        let tenant_id = DataTenantId::new_v7();
        let now = Utc::now();
        let ticket = authority
            .mint(&claims(worker, 7, tenant_id, now))
            .expect("ticket");

        let mut claims_tamper = ticket.clone();
        claims_tamper.claims_bytes = vec![0xff];
        assert_eq!(
            authority
                .verify_before_decode(&claims_tamper, worker, 7, now)
                .await,
            Err(PeerSecurityError::InvalidSignature)
        );

        let mut signature_tamper = ticket.clone();
        signature_tamper.signature[0] ^= 1;
        assert_eq!(
            authority
                .verify_before_decode(&signature_tamper, worker, 7, now)
                .await,
            Err(PeerSecurityError::InvalidSignature)
        );

        let mut unknown = ticket;
        unknown.key_id = "00".repeat(32);
        assert_eq!(
            authority
                .verify_before_decode(&unknown, worker, 7, now)
                .await,
            Err(PeerSecurityError::UnknownKey)
        );
        assert_eq!(
            *audit.calls.lock().expect("audit mutex"),
            vec![
                (None, BifrostSecurityViolationKind::PeerSignature),
                (None, BifrostSecurityViolationKind::PeerSignature),
                (None, BifrostSecurityViolationKind::PeerUnknownKey),
            ]
        );
    }

    /// Audience, worker fence, and replay audit to the signed tenant chain.
    #[tokio::test]
    async fn oracle_peer_authority_audits_verified_violations_to_signed_tenant() {
        let (authority, audit) = authority();
        let worker = NodeId::new(uuid::Uuid::from_u128(1));
        let tenant_id = DataTenantId::new_v7();
        let now = Utc::now();
        let ticket = authority
            .mint(&claims(worker, 7, tenant_id, now))
            .expect("ticket");
        assert_eq!(
            authority
                .verify_before_decode(&ticket, NodeId::new(uuid::Uuid::from_u128(9)), 7, now)
                .await,
            Err(PeerSecurityError::Audience)
        );
        assert_eq!(
            authority
                .verify_before_decode(&ticket, worker, 8, now)
                .await,
            Err(PeerSecurityError::Fence)
        );
        authority
            .verify_before_decode(&ticket, worker, 7, now)
            .await
            .expect("first consume");
        assert_eq!(
            authority
                .verify_before_decode(&ticket, worker, 7, now)
                .await,
            Err(PeerSecurityError::Replay)
        );

        let restarted =
            OraclePeerAuthority::from_pem(&SecretString::from(PRIVATE_KEY_PEM), audit.clone())
                .expect("restart");
        assert_eq!(
            restarted
                .verify_before_decode(&ticket, worker, 8, now)
                .await,
            Err(PeerSecurityError::Fence)
        );

        let expired = restarted
            .mint(&claims(
                worker,
                8,
                tenant_id,
                now - chrono::Duration::seconds(20),
            ))
            .expect("expired ticket");
        assert_eq!(
            restarted
                .verify_before_decode(&expired, worker, 8, now)
                .await,
            Err(PeerSecurityError::Expired)
        );
        assert_eq!(
            *audit.calls.lock().expect("audit mutex"),
            vec![
                (Some(tenant_id), BifrostSecurityViolationKind::PeerAudience,),
                (Some(tenant_id), BifrostSecurityViolationKind::PeerFence),
                (Some(tenant_id), BifrostSecurityViolationKind::PeerReplay),
                (Some(tenant_id), BifrostSecurityViolationKind::PeerFence),
                (Some(tenant_id), BifrostSecurityViolationKind::PeerReplay),
            ]
        );
    }

    /// A security audit outage masks rejection details and never permits the ticket.
    #[tokio::test]
    async fn oracle_peer_authority_fails_closed_when_security_audit_is_unavailable() {
        let authority = OraclePeerAuthority::from_pem(
            &SecretString::from(PRIVATE_KEY_PEM),
            Arc::new(FailingPeerAudit),
        )
        .expect("authority");
        let worker = NodeId::new(uuid::Uuid::from_u128(1));
        let tenant_id = DataTenantId::new_v7();
        let now = Utc::now();
        let mut ticket = authority
            .mint(&claims(worker, 7, tenant_id, now))
            .expect("ticket");
        ticket.key_id = "00".repeat(32);

        assert_eq!(
            authority
                .verify_before_decode(&ticket, worker, 7, now)
                .await,
            Err(PeerSecurityError::AuditUnavailable)
        );
    }

    /// Tamper, malformed bytes, expiry, replay, and restart fencing all fail closed.
    #[tokio::test]
    async fn oracle_peer_authority_rejects_tamper_replay_and_restart_fence() {
        let (primary, _audit) = authority();
        let worker = NodeId::new(uuid::Uuid::from_u128(70));
        let tenant = DataTenantId::new(uuid::Uuid::now_v7()).expect("tenant");
        let now = Utc::now();

        let mut claim_tamper = primary
            .mint(&claims(worker, 9, tenant, now))
            .expect("ticket");
        claim_tamper.claims_bytes[0] ^= 1;
        assert_eq!(
            primary
                .verify_before_decode(&claim_tamper, worker, 9, now)
                .await,
            Err(PeerSecurityError::InvalidSignature),
        );

        let mut signature_tamper = primary
            .mint(&claims(worker, 9, tenant, now))
            .expect("ticket");
        signature_tamper.signature[0] ^= 1;
        assert_eq!(
            primary
                .verify_before_decode(&signature_tamper, worker, 9, now)
                .await,
            Err(PeerSecurityError::InvalidSignature),
        );

        let mut malformed = primary
            .mint(&claims(worker, 9, tenant, now))
            .expect("ticket");
        malformed.signature.truncate(1);
        assert_eq!(
            primary
                .verify_before_decode(&malformed, worker, 9, now)
                .await,
            Err(PeerSecurityError::InvalidSignature),
        );

        let mut expired_claims = claims(worker, 9, tenant, now);
        expired_claims.expires_at_ms = (now - chrono::Duration::milliseconds(1)).timestamp_millis();
        let expired = primary.mint(&expired_claims).expect("ticket");
        assert_eq!(
            primary.verify_before_decode(&expired, worker, 9, now).await,
            Err(PeerSecurityError::Expired),
        );

        let valid = primary
            .mint(&claims(worker, 9, tenant, now))
            .expect("ticket");
        primary
            .verify_before_decode(&valid, worker, 9, now)
            .await
            .expect("first use");
        assert_eq!(
            primary.verify_before_decode(&valid, worker, 9, now).await,
            Err(PeerSecurityError::Replay),
        );

        let (restarted, _audit) = authority();
        assert_eq!(
            restarted
                .verify_before_decode(&valid, worker, 10, now)
                .await,
            Err(PeerSecurityError::Fence),
        );
    }
}
