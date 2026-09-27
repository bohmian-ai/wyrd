//! Domain-separated authority for private Scribe live-tail tickets.

use crate::oracle::peer_keyring::PeerTicketKeyring;
use chrono::{DateTime, Utc};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use vala_bifrost_redux::scribe::tail_rpc::{
    TailReadError, TailSecurityAudit, TailTicketAudience, TailTicketBinding, TailTicketClaims,
    TailTicketMinter, TailTicketVerifier,
};
use wyrd_spec::DataTenantId;

const DOMAIN: &[u8] = b"wyrd.scribe.tail.v1\0";
const MAX_TICKET_TTL: chrono::Duration = chrono::Duration::seconds(30);
const MAX_CLAIMS_BYTES: usize = 16 * 1024;
/// Maximum unexpired ticket nonces retained for replay rejection.
///
/// Every nonce expires within [`MAX_TICKET_TTL`], so this bounds concurrent
/// discovery tickets in flight rather than total history.
const MAX_REPLAY_NONCES: usize = 256;

#[derive(Debug, Serialize, Deserialize)]
/// Signed wire representation of a short-lived active-stream discovery ticket.
struct TicketWire {
    /// Narrow operation audience.
    audience: u8,
    /// Query owning the operation.
    query_id: uuid::Uuid,
    /// Tenant owning the operation.
    tenant_id: uuid::Uuid,
    /// Canonical table identity.
    canonical_table: String,
    /// Scribe node selected for the operation.
    node_id: uuid::Uuid,
    /// Writer epoch selected for the operation.
    writer_epoch: u64,
    /// Unchanged query execution deadline bound to the exact request.
    deadline_ms: i64,
    /// Ticket acceptance expiry, no later than the query deadline or mint + 30s.
    expires_ms: i64,
    /// Single-use replay nonce.
    nonce: Vec<u8>,
}

impl TicketWire {
    /// Validate the signed acceptance window independently of the query budget.
    ///
    /// # Errors
    /// Returns authorization failure for invalid timestamps, expired acceptance,
    /// or an acceptance window exceeding the query deadline or maximum TTL.
    fn validate_deadlines(
        &self,
        now: DateTime<Utc>,
    ) -> Result<(DateTime<Utc>, DateTime<Utc>), TailReadError> {
        let deadline = DateTime::from_timestamp_millis(self.deadline_ms);
        let expiry = DateTime::from_timestamp_millis(self.expires_ms);
        match (deadline, expiry) {
            (Some(deadline), Some(expiry))
                if expiry > now && expiry <= deadline && expiry - now <= MAX_TICKET_TTL =>
            {
                Ok((deadline, expiry))
            }
            _ => Err(TailReadError::Authorization {
                detail: "tail ticket deadline or acceptance expiry is invalid".to_owned(),
            }),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
/// Envelope signed by the server-owned Ed25519 key.
struct SignedWire {
    /// Signing-key identifier used for rotation and domain separation.
    key_id: String,
    /// Serialized operation claims.
    payload: Vec<u8>,
    /// Ed25519 signature over the domain, key id, and payload.
    signature: Vec<u8>,
}

/// Server-owned Scribe-tail signer/verifier with bounded replay retention.
pub struct ScribeTailAuthority {
    /// Independent peer-ticket keyring this authority signs and verifies with.
    ///
    /// Tail tickets travel the same private plane as every other peer purpose
    /// ticket, so they are signed by the same independent keyring and never by
    /// the north-south workload key, and they rotate with it.
    keyring: Arc<PeerTicketKeyring>,
    /// Nonce replay set retained only through ticket expiry.
    replay: Mutex<HashMap<Vec<u8>, DateTime<Utc>>>,
    /// Maximum unexpired replay entries retained at once.
    replay_capacity: usize,
    /// Durable audit sink required before authorization denials escape.
    audit: Arc<dyn TailSecurityAudit>,
}

impl std::fmt::Debug for ScribeTailAuthority {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ScribeTailAuthority")
            .field("key_id", &self.keyring.active_key_id())
            .finish_non_exhaustive()
    }
}

impl ScribeTailAuthority {
    /// Creates the tail authority from the server's existing signing key.
    ///
    /// # Errors
    /// Returns an authorization error for malformed key material.
    pub fn from_pem(
        pem: &SecretString,
        audit: Arc<dyn TailSecurityAudit>,
    ) -> Result<Self, TailReadError> {
        let keyring = PeerTicketKeyring::from_signing_key_pem(pem).map_err(|_| {
            TailReadError::Authorization {
                detail: "tail signing key is invalid".to_owned(),
            }
        })?;
        Ok(Self::from_keyring(Arc::new(keyring), audit))
    }

    /// Composes the tail authority over this plane's independent keyring.
    ///
    /// This is the production constructor; [`Self::from_pem`] remains for a
    /// caller holding only single-key material.
    #[must_use]
    pub fn from_keyring(
        keyring: Arc<PeerTicketKeyring>,
        audit: Arc<dyn TailSecurityAudit>,
    ) -> Self {
        Self {
            keyring,
            replay: Mutex::new(HashMap::new()),
            replay_capacity: MAX_REPLAY_NONCES,
            audit,
        }
    }

    /// Clears nonce replay state during ordered Scribe shutdown.
    pub(crate) fn clear_replay_state(&self) {
        if let Ok(mut replay) = self.replay.lock() {
            replay.clear();
        }
    }

    /// Signs an already encoded domain payload.
    ///
    /// # Errors
    /// Returns authorization failure when the signed envelope cannot be encoded.
    fn sign(&self, payload: Vec<u8>) -> Result<Vec<u8>, TailReadError> {
        let key_id = self.keyring.active_key_id().to_owned();
        let signature = self
            .keyring
            .sign(&[DOMAIN, key_id.as_bytes(), &payload].concat());
        serde_json::to_vec(&SignedWire {
            key_id,
            payload,
            signature,
        })
        .map_err(|_| TailReadError::Authorization {
            detail: "tail ticket encoding failed".to_owned(),
        })
    }

    /// Verifies and decodes a signed domain payload.
    ///
    /// # Errors
    /// Returns authorization failure for malformed envelopes, unknown keys,
    /// invalid signatures, or invalid claims.
    fn decode<T: for<'de> Deserialize<'de>>(&self, encoded: &[u8]) -> Result<T, TailReadError> {
        let wire: SignedWire =
            serde_json::from_slice(encoded).map_err(|_| TailReadError::Authorization {
                detail: "tail ticket encoding is invalid".to_owned(),
            })?;
        if wire.payload.len() > MAX_CLAIMS_BYTES || wire.signature.len() != 64 {
            return Err(TailReadError::Authorization {
                detail: "tail ticket signature is invalid".to_owned(),
            });
        }
        self.keyring
            .verify(
                &wire.key_id,
                &[DOMAIN, wire.key_id.as_bytes(), &wire.payload].concat(),
                &wire.signature,
                Utc::now(),
            )
            .map_err(|_| TailReadError::Authorization {
                detail: "tail ticket signature is invalid".to_owned(),
            })?;
        serde_json::from_slice(&wire.payload).map_err(|_| TailReadError::Authorization {
            detail: "tail ticket claims are invalid".to_owned(),
        })
    }

    /// Converts a typed audience into its stable wire discriminator.
    fn audience(value: TailTicketAudience) -> u8 {
        match value {
            TailTicketAudience::List => 1,
        }
    }

    /// Converts a stable wire discriminator into a typed audience.
    ///
    /// # Errors
    /// Returns authorization failure for an unknown discriminator.
    fn parse_audience(value: u8) -> Result<TailTicketAudience, TailReadError> {
        match value {
            1 => Ok(TailTicketAudience::List),
            _ => Err(TailReadError::Authorization {
                detail: "tail ticket audience is invalid".to_owned(),
            }),
        }
    }

    /// Projects transport-neutral claims into their signed wire representation.
    fn wire(claims: &TailTicketClaims, now: DateTime<Utc>) -> TicketWire {
        TicketWire {
            audience: Self::audience(claims.audience),
            query_id: claims.query_id,
            tenant_id: claims.tenant_id.as_uuid(),
            canonical_table: claims.canonical_table.clone(),
            node_id: claims.node_id,
            writer_epoch: claims.writer_epoch,
            deadline_ms: claims.deadline.timestamp_millis(),
            expires_ms: claims.deadline.min(now + MAX_TICKET_TTL).timestamp_millis(),
            nonce: claims.nonce.clone(),
        }
    }
}

impl TailTicketMinter for ScribeTailAuthority {
    /// Signs one short-lived active-stream discovery claim set.
    ///
    /// The signed acceptance expiry is bounded independently; the query's exact
    /// deadline remains unchanged for request binding.
    ///
    /// # Errors
    /// Returns authorization failure for expired queries, short nonces, or
    /// envelope encoding/signing failures.
    fn mint_tail_ticket(&self, claims: &TailTicketClaims) -> Result<Vec<u8>, TailReadError> {
        let now = Utc::now();
        if claims.deadline <= now || claims.nonce.len() < 16 {
            return Err(TailReadError::Authorization {
                detail: "tail ticket lifetime or nonce is invalid".to_owned(),
            });
        }
        serde_json::to_vec(&Self::wire(claims, now))
            .map_err(|_| TailReadError::Authorization {
                detail: "tail ticket encoding failed".to_owned(),
            })
            .and_then(|payload| self.sign(payload))
    }
}

#[async_trait::async_trait]
impl TailTicketVerifier for ScribeTailAuthority {
    /// Appends a rejection whose signed tenant tuple is not trusted.
    async fn audit_unverified_rejection(&self, reason: &str) -> Result<(), TailReadError> {
        self.audit.append_unverified_tail_rejection(reason).await
    }

    /// Appends a rejection after the signed tenant tuple has been decoded.
    async fn audit_verified_violation(
        &self,
        tenant_id: DataTenantId,
        reason: &str,
    ) -> Result<(), TailReadError> {
        self.audit
            .append_verified_tail_violation(tenant_id, reason)
            .await
    }

    /// Validates a ticket's exact request tuple and audits any verified denial.
    async fn verify_tail_ticket_binding(
        &self,
        claims: &TailTicketClaims,
        binding: &TailTicketBinding,
    ) -> Result<(), TailReadError> {
        if let Err(error) = claims.validate_binding(
            binding.query_id,
            binding.tenant_id,
            &binding.canonical_table,
            binding.node_id,
            binding.writer_epoch,
            binding.deadline,
        ) {
            self.audit
                .append_verified_tail_violation(claims.tenant_id, "binding")
                .await?;
            return Err(error);
        }
        Ok(())
    }

    /// Verifies a ticket before operation-specific binding checks and consumes
    /// its nonce even when the later check rejects the request.
    ///
    /// # Errors
    /// Returns authorization failure for malformed, expired, wrong-audience, or
    /// replayed tickets; audit failure also closes the operation.
    async fn verify_tail_ticket_unbound(
        &self,
        encoded: &[u8],
        expected_audience: TailTicketAudience,
    ) -> Result<TailTicketClaims, TailReadError> {
        let wire: TicketWire = match self.decode(encoded) {
            Ok(wire) => wire,
            Err(error) => {
                self.audit
                    .append_unverified_tail_rejection("signature")
                    .await?;
                return Err(error);
            }
        };
        let audience = match Self::parse_audience(wire.audience) {
            Ok(audience) => audience,
            Err(error) => {
                if let Ok(tenant) = DataTenantId::new(wire.tenant_id) {
                    self.audit
                        .append_verified_tail_violation(tenant, "audience")
                        .await?;
                } else {
                    self.audit
                        .append_unverified_tail_rejection("audience")
                        .await?;
                }
                return Err(error);
            }
        };
        if audience != expected_audience {
            if let Ok(tenant) = DataTenantId::new(wire.tenant_id) {
                self.audit
                    .append_verified_tail_violation(tenant, "audience")
                    .await?;
            }
            return Err(TailReadError::Authorization {
                detail: "tail ticket audience is invalid".to_owned(),
            });
        }
        let (deadline, expiry) = match wire.validate_deadlines(Utc::now()) {
            Ok(deadlines) => deadlines,
            Err(error) => {
                if let Ok(tenant) = DataTenantId::new(wire.tenant_id) {
                    self.audit
                        .append_verified_tail_violation(tenant, "expiry")
                        .await?;
                } else {
                    self.audit
                        .append_unverified_tail_rejection("expiry")
                        .await?;
                }
                return Err(error);
            }
        };
        let replay_rejected = {
            let mut replay = self
                .replay
                .lock()
                .map_err(|_| TailReadError::Authorization {
                    detail: "tail replay state is unavailable".to_owned(),
                })?;
            replay.retain(|_, expiry| *expiry > Utc::now());
            replay.len() >= self.replay_capacity
                || replay.insert(wire.nonce.clone(), expiry).is_some()
        };
        if replay_rejected {
            self.audit
                .append_verified_tail_violation(
                    DataTenantId::new(wire.tenant_id).map_err(|_| {
                        TailReadError::Authorization {
                            detail: "tail tenant is invalid".to_owned(),
                        }
                    })?,
                    "replay",
                )
                .await?;
            return Err(TailReadError::Authorization {
                detail: "tail ticket replay detected".to_owned(),
            });
        }
        Ok(TailTicketClaims {
            query_id: wire.query_id,
            tenant_id: DataTenantId::new(wire.tenant_id).map_err(|_| {
                TailReadError::Authorization {
                    detail: "tail tenant is invalid".to_owned(),
                }
            })?,
            canonical_table: wire.canonical_table,
            node_id: wire.node_id,
            writer_epoch: wire.writer_epoch,
            deadline,
            audience: expected_audience,
            nonce: wire.nonce,
        })
    }

    /// Verifies a single-use ticket and consumes its nonce before Scribe state.
    ///
    /// # Errors
    /// Returns authorization failure for malformed, expired, mismatched, or
    /// replayed tickets; audit failure also closes the operation.
    async fn verify_tail_ticket(
        &self,
        encoded: &[u8],
        expected: &TailTicketClaims,
    ) -> Result<(), TailReadError> {
        let wire: TicketWire = match self.decode(encoded) {
            Ok(wire) => wire,
            Err(error) => {
                self.audit
                    .append_unverified_tail_rejection("signature")
                    .await?;
                return Err(error);
            }
        };
        let audience = Self::parse_audience(wire.audience)?;
        let expected_wire = Self::wire(expected, Utc::now());
        if wire.query_id != expected_wire.query_id
            || wire.tenant_id != expected_wire.tenant_id
            || wire.canonical_table != expected_wire.canonical_table
            || wire.node_id != expected_wire.node_id
            || wire.writer_epoch != expected_wire.writer_epoch
            || wire.deadline_ms != expected_wire.deadline_ms
            || wire.audience != expected_wire.audience
            || wire.nonce != expected_wire.nonce
        {
            self.audit
                .append_verified_tail_violation(expected.tenant_id, "claims")
                .await?;
            return Err(TailReadError::Authorization {
                detail: "tail ticket claims do not match request".to_owned(),
            });
        }
        let now = Utc::now();
        let (_, expiry) = match wire.validate_deadlines(now) {
            Ok(deadlines) => deadlines,
            Err(error) => {
                self.audit
                    .append_verified_tail_violation(expected.tenant_id, "expiry")
                    .await?;
                return Err(error);
            }
        };
        let replay_rejected = {
            let mut replay = self
                .replay
                .lock()
                .map_err(|_| TailReadError::Authorization {
                    detail: "tail replay state is unavailable".to_owned(),
                })?;
            replay.retain(|_, expiry| *expiry > now);
            replay.len() >= self.replay_capacity || replay.insert(wire.nonce, expiry).is_some()
        };
        if replay_rejected {
            self.audit
                .append_verified_tail_violation(expected.tenant_id, "replay")
                .await?;
            return Err(TailReadError::Authorization {
                detail: "tail ticket replay detected".to_owned(),
            });
        }
        let _ = audience;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_TICKET_TTL, ScribeTailAuthority, TicketWire};
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use secrecy::SecretString;
    use vala_bifrost_redux::scribe::tail_rpc::{
        TailReadError, TailSecurityAudit, TailTicketAudience, TailTicketBinding, TailTicketClaims,
        TailTicketMinter, TailTicketVerifier,
    };
    use wyrd_spec::DataTenantId;

    const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";

    #[derive(Default)]
    struct RecordingAudit {
        reasons: Mutex<Vec<String>>,
        fail: bool,
    }

    #[async_trait]
    impl TailSecurityAudit for RecordingAudit {
        async fn append_unverified_tail_rejection(
            &self,
            reason: &str,
        ) -> Result<(), TailReadError> {
            self.reasons
                .lock()
                .expect("audit lock")
                .push(reason.to_owned());
            if self.fail {
                return Err(TailReadError::Authorization {
                    detail: "injected audit outage".to_owned(),
                });
            }
            Ok(())
        }

        async fn append_verified_tail_violation(
            &self,
            _tenant_id: DataTenantId,
            reason: &str,
        ) -> Result<(), TailReadError> {
            self.reasons
                .lock()
                .expect("audit lock")
                .push(reason.to_owned());
            if self.fail {
                return Err(TailReadError::Authorization {
                    detail: "injected audit outage".to_owned(),
                });
            }
            Ok(())
        }
    }

    fn claims(audience: TailTicketAudience, nonce: u8) -> TailTicketClaims {
        TailTicketClaims {
            query_id: uuid::Uuid::new_v4(),
            tenant_id: DataTenantId::new_v7(),
            canonical_table: "vala.bifrost.events".to_owned(),
            node_id: uuid::Uuid::new_v4(),
            writer_epoch: 1,
            deadline: chrono::Utc::now() + chrono::Duration::seconds(5),
            audience,
            nonce: vec![nonce; 16],
        }
    }

    fn authority(audit: Arc<RecordingAudit>) -> ScribeTailAuthority {
        ScribeTailAuthority::from_pem(&SecretString::from(PRIVATE_KEY_PEM.to_owned()), audit)
            .expect("fixture key parses")
    }

    /// Long query deadlines retain short-lived, single-use List authorization
    /// without shortening the bound request's execution budget.
    ///
    /// # Panics
    /// Panics if a healthy long query cannot mint or use its ticket, or replay
    /// is accepted without an audit denial.
    #[tokio::test]
    async fn replayed_list_tickets_are_audited() {
        let audit = Arc::new(RecordingAudit::default());
        let authority = authority(Arc::clone(&audit));
        let (audience, nonce) = (TailTicketAudience::List, 1);
        let mut claims = claims(audience, nonce);
        claims.deadline = chrono::Utc::now() + chrono::Duration::seconds(60);
        let ticket = authority.mint_tail_ticket(&claims).expect("ticket signs");
        let wire: TicketWire = authority.decode(&ticket).expect("signed claims decode");
        let now = chrono::Utc::now();
        let (deadline, expiry) = wire.validate_deadlines(now).expect("ticket is usable");
        assert_eq!(
            deadline.timestamp_millis(),
            claims.deadline.timestamp_millis()
        );
        assert!(expiry - now <= MAX_TICKET_TTL);
        assert!(expiry < deadline);
        let decoded = authority
            .verify_tail_ticket_unbound(&ticket, audience)
            .await
            .expect("first use succeeds");
        assert_eq!(
            decoded.deadline.timestamp_millis(),
            claims.deadline.timestamp_millis()
        );
        assert!(matches!(
            authority
                .verify_tail_ticket_unbound(&ticket, audience)
                .await,
            Err(TailReadError::Authorization { .. })
        ));
        authority.clear_replay_state();
        authority
            .verify_tail_ticket_unbound(&ticket, audience)
            .await
            .expect("shutdown clears bounded replay state");
        let reasons = audit.reasons.lock().expect("audit lock");
        assert_eq!(
            reasons.iter().filter(|reason| *reason == "replay").count(),
            1
        );
    }

    /// Both verification entry points reject expired acceptance independently
    /// of the still-live query deadline; exact authority and replay stay fenced.
    ///
    /// # Panics
    /// Panics if an expired, rebound, or replayed ticket authorizes Scribe IO.
    #[tokio::test]
    async fn long_query_tickets_keep_expiry_and_exact_binding() {
        let authority = authority(Arc::new(RecordingAudit::default()));
        let (audience, nonce) = (TailTicketAudience::List, 11);
        let mut claims = claims(audience, nonce);
        claims.deadline = chrono::Utc::now() + chrono::Duration::seconds(60);
        let ticket = authority
            .mint_tail_ticket(&claims)
            .expect("long query signs");
        let mut changed = claims.clone();
        changed.query_id = uuid::Uuid::new_v4();
        assert!(
            authority
                .verify_tail_ticket(&ticket, &changed)
                .await
                .is_err()
        );
        authority
            .verify_tail_ticket(&ticket, &claims)
            .await
            .expect("exact request verifies");
        assert!(
            authority
                .verify_tail_ticket(&ticket, &claims)
                .await
                .is_err()
        );

        let mut wire: TicketWire = authority.decode(&ticket).expect("signed ticket decodes");
        let expiry = chrono::DateTime::from_timestamp_millis(wire.expires_ms)
            .expect("signed expiry is valid");
        assert!(wire.validate_deadlines(expiry).is_err());
        wire.expires_ms =
            (chrono::Utc::now() - chrono::Duration::milliseconds(1)).timestamp_millis();
        let expired = authority
            .sign(serde_json::to_vec(&wire).expect("claims encode"))
            .expect("expired fixture signs");
        authority.clear_replay_state();
        assert!(matches!(
            authority
                .verify_tail_ticket_unbound(&expired, audience)
                .await,
            Err(TailReadError::Authorization { .. })
        ));
        assert!(matches!(
            authority.verify_tail_ticket(&expired, &claims).await,
            Err(TailReadError::Authorization { .. })
        ));
    }

    /// A verified binding denial fails closed when its audit append is unavailable.
    #[tokio::test]
    async fn binding_audit_outage_denies_before_mutation() {
        let audit = Arc::new(RecordingAudit {
            fail: true,
            ..RecordingAudit::default()
        });
        let authority = authority(audit);
        let claims = claims(TailTicketAudience::List, 3);
        let binding = TailTicketBinding {
            query_id: uuid::Uuid::new_v4(),
            tenant_id: claims.tenant_id,
            canonical_table: claims.canonical_table.clone(),
            node_id: claims.node_id,
            writer_epoch: claims.writer_epoch,
            deadline: claims.deadline,
        };
        assert!(matches!(
            authority.verify_tail_ticket_binding(&claims, &binding).await,
            Err(TailReadError::Authorization { detail }) if detail == "injected audit outage"
        ));
    }
}
