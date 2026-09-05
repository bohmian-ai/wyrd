//! Domain-separated Ed25519 authority for opaque Oracle peer tickets.

use chrono::{DateTime, Utc};
use ed25519_dalek::pkcs8::DecodePrivateKey;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use secrecy::ExposeSecret;
use secrecy::SecretString;
use sha2::{Digest, Sha256};
use std::fmt;
use std::sync::Arc;
use vala_bifrost_redux::oracle::peer::PeerReplayCache;
use vala_bifrost_redux::oracle::peer::{
    PeerSecurityAudit, PeerSecurityError, PeerTicketClaims, PeerTicketMinter, PeerTicketVerifier,
    VerifiedClaimsBytes,
};
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{
    BifrostQueryRequest, BifrostSecurityViolationKind, NodeId, QueryClass, SignedPeerTicket,
};
use wyrd_tonic::prost::Message;

/// Domain separator preventing peer signatures from crossing protocol boundaries.
const DOMAIN: &[u8] = b"wyrd.oracle.peer.v1\0";
/// Domain separator for authenticated public-query forwarding envelopes.
const FORWARD_QUERY_DOMAIN: &[u8] = b"wyrd.oracle.forward-query.v1\0";
/// Hard cap applied before any claims bytes are decoded.
const MAX_CLAIMS_BYTES: usize = 16 * 1024;
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
    /// Pinned private key retained only by the server authority.
    signing: Arc<SigningKey>,
    /// Public key used to authenticate raw claim bytes.
    verifying: VerifyingKey,
    /// Lowercase SHA-256 digest of the raw public key.
    key_id: String,
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
            .field("key_id", &self.key_id)
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
        let signature = self.signing.sign(&signing_input_for(
            FORWARD_QUERY_DOMAIN,
            &self.key_id,
            &claims_bytes,
        ));
        Ok(SignedPeerTicket {
            key_id: self.key_id.clone(),
            claims_bytes,
            signature: signature.to_bytes().to_vec(),
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
        if ticket.key_id != self.key_id
            || ticket.signature.len() != 64
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
        let signature = match Signature::from_slice(&ticket.signature) {
            Ok(signature) => signature,
            Err(_) => {
                return Err(self
                    .forwarding_rejection(
                        None,
                        BifrostSecurityViolationKind::PeerSignature,
                        PeerSecurityError::InvalidSignature,
                    )
                    .await);
            }
        };
        if self
            .verifying
            .verify(
                &signing_input_for(FORWARD_QUERY_DOMAIN, &ticket.key_id, &ticket.claims_bytes),
                &signature,
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
        if replay_capacity == 0 || max_ticket_ttl <= chrono::Duration::zero() {
            return Err(PeerSecurityError::Claims);
        }
        let signing = SigningKey::from_pkcs8_pem(pem.expose_secret())
            .map_err(|_| PeerSecurityError::InvalidSignature)?;
        let verifying = signing.verifying_key();
        let key_id = hex::encode(Sha256::digest(verifying.to_bytes()));
        Ok(Self {
            signing: Arc::new(signing),
            verifying,
            key_id,
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
            .signing
            .sign(&signing_input(&self.key_id, &claims_bytes));
        Ok(SignedPeerTicket {
            key_id: self.key_id.clone(),
            claims_bytes,
            signature: signature.to_bytes().to_vec(),
        })
    }

    /// Returns the public lowercase SHA-256 key identifier.
    #[must_use]
    pub fn key_id(&self) -> &str {
        &self.key_id
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
        if ticket.key_id != self.key_id {
            return self
                .reject_unverified(
                    BifrostSecurityViolationKind::PeerUnknownKey,
                    PeerSecurityError::UnknownKey,
                )
                .await;
        }
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
        let signature = match Signature::from_slice(&ticket.signature) {
            Ok(signature) => signature,
            Err(_) => {
                return self
                    .reject_unverified(
                        BifrostSecurityViolationKind::PeerSignature,
                        PeerSecurityError::InvalidSignature,
                    )
                    .await;
            }
        };
        if self
            .verifying
            .verify(
                &signing_input(&ticket.key_id, &ticket.claims_bytes),
                &signature,
            )
            .is_err()
        {
            return self
                .reject_unverified(
                    BifrostSecurityViolationKind::PeerSignature,
                    PeerSecurityError::InvalidSignature,
                )
                .await;
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
    async fn reject_unverified(
        &self,
        violation: BifrostSecurityViolationKind,
        error: PeerSecurityError,
    ) -> Result<VerifiedClaimsBytes, PeerSecurityError> {
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

/// Builds a signing preimage for one closed private protocol domain.
fn signing_input_for(domain: &[u8], key_id: &str, claims: &[u8]) -> Vec<u8> {
    [domain, key_id.as_bytes(), claims].concat()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
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
            execution_deadline_unix_ms: (now + chrono::Duration::seconds(30)).timestamp_millis(),
            binding: "binding".to_owned(),
            fragment_digest: "fragment".to_owned(),
            manifest_digest: "manifest".to_owned(),
            projection_digest: "projection".to_owned(),
            permission_digest: "permission".to_owned(),
            assignment_authority_digest: "assignment-authority".to_owned(),
        }
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

        let mut deadline_tamper = primary
            .mint(&claims(worker, 9, tenant, now))
            .expect("ticket");
        let mut changed =
            PeerTicketClaims::decode(deadline_tamper.claims_bytes.as_slice()).expect("claims");
        changed.execution_deadline_unix_ms += 1;
        deadline_tamper.claims_bytes = changed.encode_to_vec();
        assert_eq!(
            primary
                .verify_before_decode(&deadline_tamper, worker, 9, now)
                .await,
            Err(PeerSecurityError::InvalidSignature),
            "execution deadline is signed independently of acceptance expiry",
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
