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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};
    use std::thread;

    use super::*;

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
