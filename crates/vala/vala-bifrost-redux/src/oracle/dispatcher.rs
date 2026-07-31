//! Bounded local and tonic sealed-fragment dispatch.

use std::collections::{HashMap, HashSet};
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use futures_util::{Stream, StreamExt};
use prost::Message;
use thiserror::Error;
use tokio::sync::OwnedSemaphorePermit;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{
    BifrostSecurityViolationKind, ExecuteFragmentRequest, FencingToken, NodeId,
    PendingNodeReservation, QueryClass, QueryId, ReleaseNodeSlotsRequest, ReservationId,
    ReservationRejected, ReserveNodeSlotsRequest, ReserveNodeSlotsResponse, WorkerAttemptFrame,
};
use wyrd_tonic::tonic::metadata::MetadataValue;
use wyrd_tonic::tonic::transport::{Channel, Endpoint};
use wyrd_tonic::tonic::{Request, Status};
use wyrd_tonic::wyrd::v1::oracle_peer_service_client::OraclePeerServiceClient;

use super::OracleSlotManager;
use super::attempt::{AttemptBuffer, AttemptError, ValidatedAttempt};
use super::executor::{ExecutorError, SealedFragmentExecutor};
use super::fragment::SealedScanFragment;
use super::peer::{
    PeerSecurityAudit, PeerSecurityError, PeerTicketClaims, PeerTicketMinter, PeerTicketVerifier,
    projection_digest,
};
use super::telemetry::{
    FragmentLocality, FragmentOutcome, FragmentTelemetry, PeerErrorClass, SecurityEventClass,
    SlotOutcome, record_peer_attempt, record_security, record_slot,
};
use crate::scribe::memory::BifrostMemoryGovernor;

/// Fixed private peer protocol version.
pub const PEER_PROTOCOL_VERSION: u32 = 1;
/// Pending reservation time to live.
const PENDING_TTL: ChronoDuration = ChronoDuration::seconds(2);
/// Stable peer rejection hint.
const RESERVATION_RETRY_MS: u32 = 1_000;

/// Transport failure classification used by retry policy.
#[derive(Debug, Error)]
pub enum DispatchError {
    /// Worker or transport failed and may be retried.
    #[error("peer attempt unavailable")]
    Retryable,
    /// A pinned immutable object disappeared and requires a whole-query replan.
    #[error("peer fragment references a stale object")]
    StaleObject,
    /// Ticket or fragment contract failed and must not be retried.
    #[error("peer security or fragment contract rejected")]
    Terminal,
    /// All bounded distinct attempts failed.
    #[error("sealed fragment attempts exhausted")]
    Exhausted,
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
    /// Requested running-slot demand.
    slot_units: u32,
    /// Admission class used by closed slot telemetry.
    query_class: QueryClass,
    /// Pending expiry used for eager reclamation.
    expires_at: DateTime<Utc>,
    /// Pending slot permit held until execute or release.
    _permit: OwnedSemaphorePermit,
}

/// Running worker reservation retained through attempt-stream completion.
#[derive(Debug)]
pub struct RunningReservation {
    /// Remote-worker slot permit released on stream completion, failure, or drop.
    ///
    /// Leader-local execution leaves this empty because the admitted query guard
    /// already retains that node's running permit for the complete query stream.
    permit: Option<OwnedSemaphorePermit>,
}

impl Drop for RunningReservation {
    /// Releases the retained worker permit when an attempt stream completes or is dropped.
    fn drop(&mut self) {
        drop(self.permit.take());
    }
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
}

impl ReservationRegistry {
    /// Creates a bounded registry for one fenced Oracle role.
    #[must_use]
    pub fn new(slots: Arc<OracleSlotManager>, capacity: usize) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            capacity,
            slots,
        }
    }

    /// Atomically reserves one pending worker slot and returns its generated identity.
    ///
    /// # Errors
    /// Returns a retryable failure when capacity, expiry, or local pending slots reject.
    pub(crate) fn reserve(
        &self,
        request: &ReserveNodeSlotsRequest,
        now: DateTime<Utc>,
    ) -> Result<PendingNodeReservation, DispatchError> {
        if request.slot_units == 0 || request.expires_at <= now {
            return Err(DispatchError::Terminal);
        }
        let permit = self
            .slots
            .try_pending()
            .map_err(|_| DispatchError::Retryable)?;
        let mut entries = self.entries.lock().map_err(|_| DispatchError::Retryable)?;
        retain_live(&mut entries, now);
        if entries.len() >= self.capacity {
            return Err(DispatchError::Retryable);
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
            pending_reservation(request, expires_at, permit),
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
    /// # Errors
    /// Returns terminal for missing/mismatched ownership and retryable for a
    /// concurrent running-capacity change.
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
        let demand = entry.slot_units;
        let query_class = entry.query_class;
        let entry = entries
            .remove(&reservation_id)
            .ok_or(DispatchError::Terminal)?;
        drop(entry);
        let result = self
            .slots
            .try_running(demand)
            .map(|permit| RunningReservation {
                permit: Some(permit),
            })
            .map_err(|_| DispatchError::Retryable);
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
        Ok(RunningReservation { permit: None })
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

/// Converts a validated wire reservation into its permit-owning registry entry.
fn pending_reservation(
    request: &ReserveNodeSlotsRequest,
    expires_at: DateTime<Utc>,
    permit: OwnedSemaphorePermit,
) -> PendingReservation {
    PendingReservation {
        query_id: request.query_id,
        leader_node_id: request.leader_node_id,
        leader_fencing_token: request.leader_fencing_token,
        slot_units: request.slot_units,
        query_class: request.query_class,
        expires_at,
        _permit: permit,
    }
}

/// Retains only unexpired pending entries, dropping their permits immediately.
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

/// Maps executor failures through the durable verified-tenant audit boundary.
struct WorkerErrorMapper {
    /// Durable audit collaborator retained by the in-flight stream.
    security_audit: Arc<dyn PeerSecurityAudit>,
    /// Cryptographically verified tenant selected before fragment decoding.
    tenant_id: DataTenantId,
}

impl WorkerErrorMapper {
    /// Classifies one worker execution failure and commits required security audit.
    ///
    /// # Errors
    ///
    /// Returns terminal when the failure is a contract/security violation or
    /// its mandatory audit cannot commit; availability and capacity failures
    /// remain retryable.
    async fn classify(&self, error: ExecutorError) -> DispatchError {
        tracing::warn!(error = ?error, "Oracle sealed-fragment execution rejected");
        let violation = match error {
            ExecutorError::StaleObject => return DispatchError::StaleObject,
            ExecutorError::Storage | ExecutorError::Deadline | ExecutorError::Capacity => {
                return DispatchError::Retryable;
            }
            ExecutorError::Path => BifrostSecurityViolationKind::PeerTenant,
            ExecutorError::Size => BifrostSecurityViolationKind::PeerManifest,
            ExecutorError::Digest
            | ExecutorError::Empty
            | ExecutorError::Schema
            | ExecutorError::Decode
            | ExecutorError::Predicate => BifrostSecurityViolationKind::PeerFragment,
        };
        record_security(SecurityEventClass::Fragment);
        let _ = self
            .security_audit
            .append_verified_ticket_violation(self.tenant_id, violation)
            .await;
        DispatchError::Terminal
    }
}

/// Worker-side owner for verify, reservation transition, fragment validation, and IO.
pub struct OraclePeerWorker {
    /// Node identity required by every ticket audience.
    worker_node_id: NodeId,
    /// Current role fence required by every ticket.
    worker_fence: FencingToken,
    /// Raw-ticket authority used before fragment decoding.
    verifier: Arc<dyn PeerTicketVerifier>,
    /// Durable collaborator used before returning verified claim failures.
    security_audit: Arc<dyn PeerSecurityAudit>,
    /// Tuple-bound pending-to-running transition owner.
    reservations: Arc<ReservationRegistry>,
    /// Shared immutable fragment validator and reader.
    executor: SealedFragmentExecutor,
}

impl OraclePeerWorker {
    /// Creates one worker runtime scoped to a single Oracle role fence.
    #[must_use]
    pub fn new(
        worker_node_id: NodeId,
        worker_fence: FencingToken,
        verifier: Arc<dyn PeerTicketVerifier>,
        security_audit: Arc<dyn PeerSecurityAudit>,
        reservations: Arc<ReservationRegistry>,
        executor: SealedFragmentExecutor,
    ) -> Self {
        Self {
            worker_node_id,
            worker_fence,
            verifier,
            security_audit,
            reservations,
            executor,
        }
    }

    /// Reserves bounded pending capacity for one fenced leader.
    #[must_use]
    pub fn reserve(&self, request: &ReserveNodeSlotsRequest) -> ReserveNodeSlotsResponse {
        let query_class = request.query_class;
        if let Ok(pending) = self.reservations.reserve(request, Utc::now()) {
            record_slot(query_class, SlotOutcome::Pending);
            ReserveNodeSlotsResponse::Pending(pending)
        } else {
            record_slot(query_class, SlotOutcome::Rejected);
            ReserveNodeSlotsResponse::Rejected(ReservationRejected {
                retry_after_ms: RESERVATION_RETRY_MS,
            })
        }
    }

    /// Releases one matching reservation idempotently.
    pub fn release(&self, request: &ReleaseNodeSlotsRequest) {
        let _released = self.reservations.release(request, Utc::now());
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
        self.execute_with_capacity(request, WorkerCapacity::ReserveRunning)
            .await
    }

    /// Verifies and executes a leader-local fragment under its admitted query slot.
    ///
    /// The in-process dispatcher retains the admitted query guard while this
    /// operation runs, so this path validates and consumes the pending
    /// reservation without acquiring a duplicate local running permit.
    ///
    /// # Errors
    /// Returns terminal security/contract failures or retryable storage failures.
    async fn execute_local(
        &self,
        request: ExecuteFragmentRequest,
    ) -> Result<WorkerExecution, DispatchError> {
        self.execute_with_capacity(request, WorkerCapacity::LeaderAdmitted)
            .await
    }

    /// Executes the shared verification and fragment workflow with explicit capacity ownership.
    ///
    /// # Errors
    /// Returns terminal security/contract failures or retryable capacity/storage failures.
    async fn execute_with_capacity(
        &self,
        request: ExecuteFragmentRequest,
        capacity: WorkerCapacity,
    ) -> Result<WorkerExecution, DispatchError> {
        let verified = self
            .verifier
            .verify_peer_ticket(
                &request.ticket,
                self.worker_node_id,
                self.worker_fence,
                Utc::now(),
            )
            .await
            .map_err(|_| {
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
        let running = match transition {
            Ok(running) => running,
            Err(DispatchError::Terminal) => {
                self.audit_verified(tenant_id, BifrostSecurityViolationKind::PeerFence)
                    .await?;
                return Err(DispatchError::Terminal);
            }
            Err(error) => return Err(error),
        };
        let Ok(fragment) = SealedScanFragment::decode(&request.fragment_bytes) else {
            self.audit_verified(tenant_id, BifrostSecurityViolationKind::PeerFragment)
                .await?;
            return Err(DispatchError::Terminal);
        };
        if let Err(violation) = validate_claims(&claims, &fragment) {
            self.audit_verified(tenant_id, violation).await?;
            return Err(DispatchError::Terminal);
        }
        let mapper = WorkerErrorMapper {
            security_audit: Arc::clone(&self.security_audit),
            tenant_id,
        };
        let frames = match self.executor.execute(&fragment) {
            Ok(frames) => frames,
            Err(error) => return Err(mapper.classify(error).await),
        };
        let output = async_stream::stream! {
            let _running = running;
            let mut frames = frames;
            while let Some(frame) = frames.next().await {
                match frame {
                    Ok(frame) => yield Ok(frame),
                    Err(error) => {
                        yield Err(mapper.classify(error).await);
                        return;
                    }
                }
            }
        };
        Ok(WorkerExecution {
            stream: Box::pin(output),
        })
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

/// Source of the running capacity retained while one worker attempt streams.
#[derive(Clone, Copy)]
enum WorkerCapacity {
    /// A remote worker acquires its own local running permit.
    ReserveRunning,
    /// The in-process leader reuses the permit retained by query admission.
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

/// Matches every fragment-bound verified claim before storage access.
///
/// # Errors
/// Returns terminal rejection for any binding, digest, projection, or deadline mismatch.
fn validate_claims(
    claims: &PeerTicketClaims,
    fragment: &SealedScanFragment,
) -> Result<(), BifrostSecurityViolationKind> {
    if claims.binding != fragment.binding {
        return Err(BifrostSecurityViolationKind::PeerTenant);
    }
    if claims.fragment_digest != fragment.fragment_id {
        return Err(BifrostSecurityViolationKind::PeerFragment);
    }
    if claims.manifest_digest != fragment.pinned_digest {
        return Err(BifrostSecurityViolationKind::PeerManifest);
    }
    if claims.projection_digest != projection_digest(&fragment.projection)
        || claims.expires_at_ms > fragment.deadline_unix_ms
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
    /// Reserves through the same registry used by the tonic path.
    ///
    /// # Errors
    /// This adapter returns the worker's typed reservation outcome.
    async fn reserve(
        &self,
        _worker: NodeId,
        request: ReserveNodeSlotsRequest,
    ) -> Result<ReserveNodeSlotsResponse, DispatchError> {
        Ok(self.worker.reserve(&request))
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
    ) -> Result<WorkerAttemptStream, DispatchError> {
        Ok(self.worker.execute_local(request).await?.stream)
    }
}

/// Real tonic client transport keyed by immutable Oracle-node addresses.
pub struct TonicOraclePeerTransport {
    /// Frozen worker-address snapshot keyed by node identity.
    addresses: HashMap<NodeId, String>,
    /// Optional authenticated service credential attached to private calls.
    authorization: Option<MetadataValue<wyrd_tonic::tonic::metadata::Ascii>>,
}

impl TonicOraclePeerTransport {
    /// Creates a transport from one immutable membership snapshot.
    ///
    /// # Errors
    /// Returns terminal rejection when the bearer value is invalid metadata.
    pub fn new(
        addresses: HashMap<NodeId, String>,
        bearer: Option<&str>,
    ) -> Result<Self, DispatchError> {
        let authorization = bearer
            .map(|token| format!("Bearer {token}").parse())
            .transpose()
            .map_err(|_| DispatchError::Terminal)?;
        Ok(Self {
            addresses,
            authorization,
        })
    }

    /// Connects to the exact selected worker from the frozen snapshot.
    ///
    /// # Errors
    /// Returns retryable failure for absent, invalid, or unreachable endpoints.
    async fn client(
        &self,
        worker: NodeId,
    ) -> Result<OraclePeerServiceClient<Channel>, DispatchError> {
        let address = self
            .addresses
            .get(&worker)
            .ok_or(DispatchError::Retryable)?;
        let endpoint =
            Endpoint::from_shared(address.clone()).map_err(|_| DispatchError::Retryable)?;
        let channel = endpoint
            .connect()
            .await
            .map_err(|_| DispatchError::Retryable)?;
        Ok(OraclePeerServiceClient::new(channel))
    }

    /// Adds workload authorization metadata when configured.
    fn authenticated<T>(&self, value: T) -> Request<T> {
        let mut request = Request::new(value);
        if let Some(value) = &self.authorization {
            request
                .metadata_mut()
                .insert("authorization", value.clone());
        }
        request
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
        let mut client = self.client(worker).await?;
        let response = client
            .reserve_slots(self.authenticated(request.into()))
            .await
            .map_err(|status| status_error(&status))?
            .into_inner();
        response.try_into().map_err(|_| DispatchError::Terminal)
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
        let mut client = self.client(worker).await?;
        client
            .release_slots(self.authenticated(request.into()))
            .await
            .map_err(|status| status_error(&status))?;
        Ok(())
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
        let mut client = self.client(worker).await?;
        let mut stream = client
            .execute_fragment(self.authenticated(request.into()))
            .await
            .map_err(|status| status_error(&status))?
            .into_inner();
        let output = async_stream::stream! {
            while let Some(frame) = stream.next().await {
                yield frame
                    .map_err(|status| status_error(&status))
                    .and_then(|frame| frame.try_into().map_err(|_| DispatchError::Terminal));
            }
        };
        Ok(Box::pin(output))
    }
}

/// Routes the local Oracle identity in-process and every remote identity through tonic.
pub struct OraclePeerTransportDirectory {
    /// Physical node identity that must never traverse the network transport.
    local_node_id: NodeId,
    /// Shared in-process adapter backed by the same fenced worker as the gRPC service.
    local: Arc<dyn OraclePeerTransport>,
    /// Remote adapter whose immutable endpoint map excludes local dispatch decisions.
    remote: Arc<dyn OraclePeerTransport>,
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
            remote,
        }
    }

    /// Creates a directory from injectable transports for isolated owner tests.
    #[cfg(test)]
    #[must_use]
    fn new_for_test(
        local_node_id: NodeId,
        local: Arc<dyn OraclePeerTransport>,
        remote: Arc<dyn OraclePeerTransport>,
    ) -> Self {
        Self {
            local_node_id,
            local,
            remote,
        }
    }

    /// Returns whether `node_id` is the exact in-process Oracle identity.
    #[must_use]
    pub fn is_local(&self, node_id: NodeId) -> bool {
        node_id == self.local_node_id
    }

    /// Selects the only permitted adapter for one candidate identity.
    fn transport(&self, node_id: NodeId) -> &dyn OraclePeerTransport {
        if self.is_local(node_id) {
            self.local.as_ref()
        } else {
            self.remote.as_ref()
        }
    }

    /// Reserves through the identity-selected local or remote adapter.
    ///
    /// # Errors
    ///
    /// Returns the selected adapter's retryable or terminal failure.
    async fn reserve(
        &self,
        worker: NodeId,
        request: ReserveNodeSlotsRequest,
    ) -> Result<ReserveNodeSlotsResponse, DispatchError> {
        self.transport(worker).reserve(worker, request).await
    }

    /// Releases through the same identity-selected adapter used for reserve.
    ///
    /// # Errors
    ///
    /// Returns the selected adapter's retryable or terminal failure.
    async fn release(
        &self,
        worker: NodeId,
        request: ReleaseNodeSlotsRequest,
    ) -> Result<(), DispatchError> {
        self.transport(worker).release(worker, request).await
    }

    /// Executes through the same identity-selected adapter used for reserve.
    ///
    /// # Errors
    ///
    /// Returns the selected adapter's retryable or terminal failure.
    async fn execute(
        &self,
        worker: NodeId,
        request: ExecuteFragmentRequest,
    ) -> Result<WorkerAttemptStream, DispatchError> {
        self.transport(worker).execute(worker, request).await
    }
}

/// Classifies tonic status without retrying security or malformed-contract failures.
fn status_error(status: &Status) -> DispatchError {
    match status.code() {
        wyrd_tonic::tonic::Code::Unavailable
        | wyrd_tonic::tonic::Code::DeadlineExceeded
        | wyrd_tonic::tonic::Code::ResourceExhausted
        | wyrd_tonic::tonic::Code::Cancelled => DispatchError::Retryable,
        wyrd_tonic::tonic::Code::NotFound => DispatchError::StaleObject,
        _ => DispatchError::Terminal,
    }
}

/// Immutable worker candidate with its role fence.
#[derive(Debug, Clone, Copy)]
pub struct DispatchCandidate {
    /// Worker node identity.
    pub node_id: NodeId,
    /// Current Oracle-role fence.
    pub worker_fence: FencingToken,
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
}

/// Owns claims construction, reserve/execute/release, and distinct-worker retry.
pub struct FragmentDispatcher {
    /// Narrow server-owned authority used to mint a fresh ticket per attempt.
    ticket_minter: Arc<dyn PeerTicketMinter>,
    /// Node-aware directory enforcing in-process leader and tonic remote routing.
    transports: OraclePeerTransportDirectory,
    /// Optional production parent governor charged before transport frames are decoded.
    memory_governor: Option<BifrostMemoryGovernor>,
}

impl FragmentDispatcher {
    /// Creates a dispatcher from narrow authority and transport capabilities.
    #[must_use]
    pub fn new(
        ticket_minter: Arc<dyn PeerTicketMinter>,
        transports: OraclePeerTransportDirectory,
    ) -> Self {
        Self {
            ticket_minter,
            transports,
            memory_governor: None,
        }
    }

    /// Attaches the process-wide parent governor used by production dispatch.
    #[must_use]
    pub fn with_memory_governor(mut self, governor: BifrostMemoryGovernor) -> Self {
        self.memory_governor = Some(governor);
        self
    }

    /// Executes on at most three distinct candidates, preserving candidate order.
    ///
    /// Every retry gets a new reservation, nonce, and ticket. Any accepted
    /// pending reservation is explicitly released after failure; TTL remains
    /// crash recovery only. Attempt bytes become visible only after footer
    /// validation succeeds.
    ///
    /// # Errors
    /// Returns terminal contract errors immediately or exhaustion after three retryable failures.
    pub async fn execute(
        &self,
        context: &DispatchContext,
        fragment: SealedScanFragment,
        candidates: &[DispatchCandidate],
    ) -> Result<ValidatedAttempt, DispatchError> {
        if !self.transports.is_local(context.leader_node_id) {
            return Err(DispatchError::Terminal);
        }
        let fragment_bytes = fragment.encode().map_err(|_| DispatchError::Terminal)?;
        let mut attempted = HashSet::new();
        for candidate in candidates {
            if attempted.len() == 3 {
                break;
            }
            if !attempted.insert(candidate.node_id) {
                continue;
            }
            let expires_at = Utc::now() + PENDING_TTL;
            let reserve = ReserveNodeSlotsRequest {
                query_id: context.query_id,
                leader_node_id: context.leader_node_id,
                leader_fencing_token: context.leader_fence,
                query_class: context.query_class,
                slot_units: context.slot_units,
                expires_at,
            };
            let pending = match self.transports.reserve(candidate.node_id, reserve).await {
                Err(DispatchError::Retryable) | Ok(ReserveNodeSlotsResponse::Rejected(_)) => {
                    continue;
                }
                Err(error) => return Err(error),
                Ok(ReserveNodeSlotsResponse::Pending(pending)) => pending,
            };
            let release = ReleaseNodeSlotsRequest {
                reservation_id: pending.reservation_id,
                query_id: context.query_id,
                leader_node_id: context.leader_node_id,
                leader_fencing_token: context.leader_fence,
            };
            let claims = PeerTicketClaims {
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
                binding: fragment.binding.clone(),
                fragment_digest: fragment.fragment_id.clone(),
                manifest_digest: fragment.pinned_digest.clone(),
                projection_digest: projection_digest(&fragment.projection),
                permission_digest: context.permission_digest.clone(),
            };
            let Ok(ticket) = self.ticket_minter.mint_peer_ticket(&claims) else {
                self.release_pending(candidate.node_id, release).await;
                return Err(DispatchError::Terminal);
            };
            let request = ExecuteFragmentRequest {
                ticket,
                fragment_bytes: fragment_bytes.clone(),
                reservation_id: pending.reservation_id,
            };
            let result = self
                .execute_attempt(candidate.node_id, request, context, &fragment)
                .await;
            if result.is_ok() {
                return result;
            }
            self.release_pending(candidate.node_id, release).await;
            if matches!(
                result,
                Err(DispatchError::Terminal | DispatchError::StaleObject)
            ) {
                return result;
            }
        }
        Err(DispatchError::Exhausted)
    }

    /// Attempts immediate tuple-bound cleanup after any accepted-attempt failure.
    async fn release_pending(&self, worker: NodeId, release: ReleaseNodeSlotsRequest) {
        if let Err(error) = self.transports.release(worker, release).await {
            tracing::warn!(
                worker = %worker.as_uuid(),
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
        fields(locality = if worker == context.leader_node_id { "local" } else { "remote" })
    )]
    async fn execute_attempt(
        &self,
        worker: NodeId,
        request: ExecuteFragmentRequest,
        context: &DispatchContext,
        fragment: &SealedScanFragment,
    ) -> Result<ValidatedAttempt, DispatchError> {
        let locality = if worker == context.leader_node_id {
            FragmentLocality::Local
        } else {
            FragmentLocality::Remote
        };
        let mut telemetry = FragmentTelemetry::start(locality);
        let mut buffer = match &self.memory_governor {
            Some(governor) => AttemptBuffer::with_memory_governor(
                context.attempt_bytes,
                context.attempt_memory_bytes,
                governor,
            )
            .map_err(attempt_error)?,
            None => {
                AttemptBuffer::with_spill_limit(context.attempt_bytes, context.attempt_memory_bytes)
            }
        };
        let mut frames = match self.transports.execute(worker, request).await {
            Ok(frames) => frames,
            Err(error) => {
                record_peer_attempt(FragmentOutcome::Failed, dispatch_error_label(&error));
                telemetry.finish(FragmentOutcome::Failed, 0);
                return Err(error);
            }
        };
        while let Some(frame) = frames.next().await {
            buffer
                .push(frame.inspect_err(|error| {
                    record_peer_attempt(FragmentOutcome::Failed, dispatch_error_label(error));
                })?)
                .map_err(attempt_error)?;
        }
        let attempt = buffer.finish().map_err(attempt_error)?;
        if attempt.footer.fragment_id != fragment.fragment_id
            || attempt.footer.manifest_digest.as_str() != fragment.pinned_digest
        {
            record_peer_attempt(FragmentOutcome::Failed, PeerErrorClass::Footer);
            telemetry.finish(FragmentOutcome::Failed, 0);
            return Err(DispatchError::Retryable);
        }
        record_peer_attempt(FragmentOutcome::Success, PeerErrorClass::None);
        telemetry.finish(FragmentOutcome::Success, attempt.footer.encoded_bytes);
        Ok(attempt)
    }
}

/// Invalid or incomplete attempts are retryable because no bytes were admitted.
fn attempt_error(_error: AttemptError) -> DispatchError {
    record_peer_attempt(FragmentOutcome::Failed, PeerErrorClass::Attempt);
    DispatchError::Retryable
}

/// Maps internal retry classes to closed metric labels.
fn dispatch_error_label(error: &DispatchError) -> PeerErrorClass {
    match error {
        DispatchError::Retryable | DispatchError::StaleObject => PeerErrorClass::Availability,
        DispatchError::Terminal => PeerErrorClass::Security,
        DispatchError::Exhausted => PeerErrorClass::Exhausted,
    }
}

impl From<PeerSecurityError> for DispatchError {
    fn from(_: PeerSecurityError) -> Self {
        Self::Terminal
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::super::fragment::{SealedScanFile, SealedSourceTier};
    use super::super::peer::DeterministicTestSigner;
    use super::*;

    /// Transport that rejects the first reserve transiently and reaches the second candidate.
    struct RetryReserveTransport {
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
        ) -> Result<WorkerAttemptStream, DispatchError> {
            self.execute_calls.fetch_add(1, Ordering::SeqCst);
            Err(DispatchError::Terminal)
        }
    }

    #[async_trait]
    impl OraclePeerTransport for RetryReserveTransport {
        /// Returns retryable once, then one accepted pending reservation.
        ///
        /// # Errors
        ///
        /// Returns [`DispatchError::Retryable`] on the injected first call.
        async fn reserve(
            &self,
            _worker: NodeId,
            request: ReserveNodeSlotsRequest,
        ) -> Result<ReserveNodeSlotsResponse, DispatchError> {
            if self.reserve_calls.fetch_add(1, Ordering::SeqCst) == 0 {
                return Err(DispatchError::Retryable);
            }
            Ok(ReserveNodeSlotsResponse::Pending(PendingNodeReservation {
                reservation_id: ReservationId::new(uuid::Uuid::now_v7()),
                expires_at: request.expires_at,
            }))
        }

        /// Accepts cleanup for the injected second-candidate execution failure.
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

        /// Injects a terminal execute failure after the second reserve succeeds.
        ///
        /// # Errors
        ///
        /// Always returns [`DispatchError::Terminal`] for the focused retry test.
        async fn execute(
            &self,
            _worker: NodeId,
            _request: ExecuteFragmentRequest,
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
        drop(running);
    }

    /// Leader-local execution consumes its reservation while reusing one admitted slot.
    #[test]
    fn oracle_peer_local_transition_does_not_double_charge_leader_slot() {
        let slots = Arc::new(OracleSlotManager::new(1, 1));
        let leader_slot = slots.try_running(1).expect("admitted leader slot");
        let registry = ReservationRegistry::new(Arc::clone(&slots), 1);
        let now = Utc::now();
        let query = QueryId::new(uuid::Uuid::now_v7());
        let leader = NodeId::new(uuid::Uuid::now_v7());
        let pending = registry
            .reserve(
                &reserve_request(query, leader, 11, now + ChronoDuration::seconds(2)),
                now,
            )
            .expect("pending local reservation");

        let running = registry
            .take_for_local_leader_execute(pending.reservation_id, query, leader, 11, now)
            .expect("leader-local transition");

        assert!(running.permit.is_none());
        assert!(slots.try_running(1).is_err());
        drop(leader_slot);
        assert!(slots.try_running(1).is_ok());
    }

    /// Expiry cleanup releases pending capacity and release is fenced and idempotent.
    #[test]
    fn oracle_peer_reservation_expiry_and_release_cleanup() {
        let registry = ReservationRegistry::new(Arc::new(OracleSlotManager::new(1, 1)), 1);
        let now = Utc::now();
        let query = QueryId::new(uuid::Uuid::now_v7());
        let leader = NodeId::new(uuid::Uuid::now_v7());
        let pending = registry
            .reserve(
                &reserve_request(query, leader, 9, now + ChronoDuration::milliseconds(1)),
                now,
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

    /// A retryable reserve failure advances to the next distinct candidate.
    #[tokio::test]
    async fn oracle_dispatch_retries_transient_reserve_on_next_candidate() {
        let leader = NodeId::new(uuid::Uuid::from_u128(1));
        let transport = Arc::new(RetryReserveTransport {
            reserve_calls: AtomicUsize::new(0),
        });
        let dispatcher = FragmentDispatcher::new(
            Arc::new(DeterministicTestSigner {
                key_id: "test".to_owned(),
            }),
            OraclePeerTransportDirectory::new_for_test(
                leader,
                transport.clone(),
                transport.clone(),
            ),
        );
        let first = NodeId::new(uuid::Uuid::from_u128(2));
        let second = NodeId::new(uuid::Uuid::from_u128(3));
        let fragment = SealedScanFragment {
            fragment_id: "fragment".to_owned(),
            binding: "binding".to_owned(),
            tier: SealedSourceTier::HotSealed,
            pinned_digest: "manifest".to_owned(),
            files: vec![SealedScanFile {
                location: "binding/data.parquet".to_owned(),
                row_groups: Vec::new(),
                size_bytes: 1,
                estimated_rows: 1,
            }],
            projection: Vec::new(),
            predicates: Vec::new(),
            schema_fingerprint: "schema".to_owned(),
            estimated_rows: 1,
            estimated_bytes: 1,
            deadline_unix_ms: i64::MAX,
        };
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
        };
        let error = dispatcher
            .execute(
                &context,
                fragment,
                &[
                    DispatchCandidate {
                        node_id: first,
                        worker_fence: 2,
                    },
                    DispatchCandidate {
                        node_id: second,
                        worker_fence: 3,
                    },
                ],
            )
            .await
            .expect_err("second candidate reaches injected terminal execute");
        assert!(matches!(error, DispatchError::Terminal));
        assert_eq!(transport.reserve_calls.load(Ordering::SeqCst), 2);
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
            OraclePeerTransportDirectory::new_for_test(
                leader,
                transport.clone(),
                transport.clone(),
            ),
        );
        let fragment = SealedScanFragment {
            fragment_id: "fragment".to_owned(),
            binding: "binding".to_owned(),
            tier: SealedSourceTier::HotSealed,
            pinned_digest: "manifest".to_owned(),
            files: vec![SealedScanFile {
                location: "binding/data.parquet".to_owned(),
                row_groups: Vec::new(),
                size_bytes: 1,
                estimated_rows: 1,
            }],
            projection: Vec::new(),
            predicates: Vec::new(),
            schema_fingerprint: "schema".to_owned(),
            estimated_rows: 1,
            estimated_bytes: 1,
            deadline_unix_ms: i64::MAX,
        };
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
        };

        let error = dispatcher
            .execute(
                &context,
                fragment,
                &[DispatchCandidate {
                    node_id: leader,
                    worker_fence: 5,
                }],
            )
            .await
            .expect_err("mint failure is terminal");

        assert!(matches!(error, DispatchError::Terminal));
        assert_eq!(transport.release_calls.load(Ordering::SeqCst), 1);
        assert_eq!(transport.execute_calls.load(Ordering::SeqCst), 0);
    }

    /// Tonic not-found preserves the stale-object signal while outages stay retryable.
    #[test]
    fn oracle_tonic_status_preserves_stale_object_classification() {
        assert!(matches!(
            status_error(&Status::not_found("stale pinned object")),
            DispatchError::StaleObject
        ));
        assert!(matches!(
            status_error(&Status::unavailable("storage outage")),
            DispatchError::Retryable
        ));
    }
}
