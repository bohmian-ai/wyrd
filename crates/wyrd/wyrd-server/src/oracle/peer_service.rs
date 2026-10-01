//! Generated-tonic adapter for private Oracle peer execution.
//!
//! Mounted only on the mTLS peer listener, which admits cluster members by
//! certificate. Each handler checks its typed context against this receiver's
//! own identity, fence, clock, and received bytes before any decode or IO.

use std::pin::Pin;
use std::sync::Arc;

use arrow::datatypes::SchemaRef;
use datafusion::error::DataFusionError;
use futures_util::{Stream, StreamExt};
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::oracle::dispatcher::{
    AttemptEncoder, DispatchError, EligibleSourceLossCause, LiveFrame, NativeOutputTally,
    OraclePeerTransport, PEER_PROTOCOL_VERSION, WorkerAttemptStream,
};
use vala_bifrost_redux::oracle::follower::{
    AuthenticatedFollowerContext, FollowerResolutionError, PhysicalPlanFollowerError,
    authenticated_preflight,
};
use vala_bifrost_redux::oracle::peer::{
    PeerSecurityAudit, PeerTicketClaims, PeerTicketVerifier, ReservationBinding,
    ReservationOperationV1,
};
use wyrd_spec::vala::api::BifrostSecurityViolationKind;
use wyrd_spec::vala::api::{ClusterRole, ExecuteFragmentRequest};
use wyrd_tonic::private_conversion::PrivateConversionError;
use wyrd_tonic::prost::Message;
use wyrd_tonic::tonic::{Request, Response, Status};
use wyrd_tonic::wyrd::v1::oracle_peer_service_server::{
    OraclePeerService, OraclePeerServiceServer,
};
use wyrd_tonic::wyrd::v1::{
    self as proto, ForwardQueryRequest, ReleaseNodeSlotsRequest, ReserveNodeSlotsRequest,
};

use crate::state::{Bifrost, Scribe};

/// Private tonic service retaining one fenced worker runtime.
pub struct OraclePeerGrpc {
    /// One published Bifrost facade owning every selectable peer capability.
    bifrost: Arc<Bifrost>,
    /// The server's shutdown token; cancelling it ends open fragment streams.
    ///
    /// Graceful serving waits for in-flight streams, and a paused or slow
    /// reader would otherwise hold a stopping pod's fragment open until the
    /// leader's deadline. Ending it as unavailable lets the leader fail or
    /// degrade the read at once, as it would for a pod that died.
    shutdown: CancellationToken,
}

/// One fault a test-tier journey injects into the next Scribe fragment.
///
/// Each drives a live-read failure at the Scribe boundary that owns it, so
/// Oracle's terminal decision is observed against the real frames a peer
/// would send rather than a leader-side simulation.
#[cfg(feature = "test-support")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ScribeFragmentFault {
    /// Refuse the fragment as unavailable before it produces any frame.
    UnavailableBeforeRows = 1,
    /// Fail the stream as unavailable right after its first batch frame.
    UnavailableAfterFirstBatch = 2,
    /// End an otherwise complete stream without its footer.
    OmitFooter = 3,
    /// Reject the fragment's peer ticket as a trust-boundary failure.
    RejectTicket = 4,
    /// Refuse the fragment's follower lease as a capacity fault.
    CapacityRefused = 5,
}

/// The armed fault, `0` when none; consumed by exactly one Scribe fragment.
#[cfg(feature = "test-support")]
static SCRIBE_FRAGMENT_FAULT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Arms `fault` for the next Scribe fragment executed in this process.
///
/// The arming is process-wide and consumed once, so a journey arms it only
/// while a single Scribe serves the queried table.
#[cfg(feature = "test-support")]
pub fn arm_scribe_fragment_fault_for_test(fault: ScribeFragmentFault) {
    SCRIBE_FRAGMENT_FAULT.store(fault as u8, std::sync::atomic::Ordering::Release);
}

/// Takes the armed fault, leaving none armed.
#[cfg(feature = "test-support")]
fn take_scribe_fragment_fault() -> Option<ScribeFragmentFault> {
    match SCRIBE_FRAGMENT_FAULT.swap(0, std::sync::atomic::Ordering::AcqRel) {
        1 => Some(ScribeFragmentFault::UnavailableBeforeRows),
        2 => Some(ScribeFragmentFault::UnavailableAfterFirstBatch),
        3 => Some(ScribeFragmentFault::OmitFooter),
        4 => Some(ScribeFragmentFault::RejectTicket),
        5 => Some(ScribeFragmentFault::CapacityRefused),
        _ => None,
    }
}

/// Starts one Scribe attempt before polling any result batch, including an empty stream.
fn start_scribe_attempt(
    encoder: &mut AttemptEncoder,
    schema: arrow::datatypes::SchemaRef,
) -> Result<wyrd_spec::vala::api::WorkerAttemptFrame, DispatchError> {
    encoder.start(schema).map_err(|_| DispatchError::Terminal)
}

impl OraclePeerGrpc {
    /// Creates the adapter around the retained worker runtime.
    ///
    /// `shutdown` is the server's shutdown token: once cancelled, every open
    /// fragment stream this adapter serves ends as unavailable.
    #[must_use]
    pub const fn new(bifrost: Arc<Bifrost>, shutdown: CancellationToken) -> Self {
        Self { bifrost, shutdown }
    }

    /// Returns the generated tonic server wrapper.
    #[must_use]
    pub fn into_server(self) -> OraclePeerServiceServer<Self> {
        OraclePeerServiceServer::new(self)
    }

    /// Appends one scrubbed denial and fails closed if audit storage is unavailable.
    ///
    /// # Errors
    ///
    /// Returns `Unavailable` when the record cannot be persisted, because a
    /// refusal Wyrd cannot account for is not a refusal it may forget.
    async fn audit_denial(&self, violation: BifrostSecurityViolationKind) -> Result<(), Status> {
        self.security_audit()?
            .append_unverified_ticket_rejection(violation)
            .await
            .map_err(|_| Status::unavailable("Bifrost peer security audit unavailable"))
    }

    /// Selects the exact role-owned peer security audit without constructing an aggregate.
    fn security_audit(&self) -> Result<Arc<dyn PeerSecurityAudit>, Status> {
        if let Some(oracle) = self.bifrost.oracle() {
            return Ok(oracle.peer().security_audit());
        }
        self.bifrost
            .scribe()
            .map(|scribe| scribe.fragment_security_audit())
            .ok_or_else(|| Status::unavailable("Bifrost peer capability is not configured"))
    }

    /// Authorizes one reservation operation before any capacity state changes.
    ///
    /// The context is detached from the request first, so the digest is taken
    /// over exactly the encoding the leader bound: the request with its context
    /// field cleared. Everything the follower compares against — its own node
    /// identity, its own current fence, the leader identity the cluster
    /// confirmed live — is derived here rather than read from the message.
    ///
    /// # Errors
    ///
    /// Returns `FailedPrecondition` when no Oracle role owns this node's
    /// reservation authority, `Unauthenticated` when no context is presented,
    /// and `PermissionDenied` for a context that is malformed, misbound,
    /// or expired. The refusal is durably audited before it returns;
    /// an audit that cannot commit surfaces as `Unavailable`.
    async fn authorize_reservation<T: Message>(
        &self,
        operation: ReservationOperationV1,
        context_free: &T,
        context: Option<proto::PeerContext>,
        leader_node_id: wyrd_spec::vala::api::NodeId,
        leader_fence: u64,
        query_id: uuid::Uuid,
    ) -> Result<(), Status> {
        let oracle = self
            .bifrost
            .oracle()
            .ok_or_else(|| Status::failed_precondition("Oracle role is not configured"))?;
        let Some(context) = context else {
            self.audit_denial(BifrostSecurityViolationKind::PeerSignature)
                .await?;
            return Err(Status::unauthenticated(
                "Bifrost peer reservation context is absent",
            ));
        };
        let context = wyrd_spec::vala::api::PeerContext::try_from(context).map_err(|_| {
            Status::permission_denied("Bifrost peer reservation context is invalid")
        })?;
        let registered = oracle.registered_role();
        let binding = ReservationBinding {
            operation,
            source_node_id: leader_node_id,
            source_fence: leader_fence,
            destination_node_id: registered.key.node_id,
            destination_fence: registered.fencing_token,
            query_id,
        };
        oracle
            .peer()
            .authority()
            .verify_reservation(
                &context,
                &binding,
                &context_free.encode_to_vec(),
                chrono::Utc::now(),
            )
            .await
            .map(|_| ())
            .map_err(|error| match error {
                vala_bifrost_redux::oracle::peer::PeerSecurityError::AuditUnavailable => {
                    Status::unavailable("Bifrost peer security audit unavailable")
                }
                _ => Status::permission_denied("Bifrost peer reservation is not authorized"),
            })
    }
}

/// In-process executor for fragments that target this process's own Scribe.
///
/// The private gRPC service and this process's own Oracle both run Scribe
/// fragments through it, so the fence, context, and claims checks exist once.
/// It yields Arrow batches and an in-process completion; only the gRPC
/// service encodes them into attempt frames for a remote leader.
pub struct ScribeFragmentExecutor {
    /// Local Scribe owner whose fence, verifier, and resources serve fragments.
    scribe: Arc<Scribe>,
}

impl ScribeFragmentExecutor {
    /// Creates the executor over this process's Scribe owner.
    #[must_use]
    pub fn new(scribe: Arc<Scribe>) -> Self {
        Self { scribe }
    }

    /// Executes one Scribe-targeted physical fragment under Scribe's own fence and resources.
    ///
    /// Checks the target fence, verifies the typed peer context, validates
    /// its claims and assignment-authority digest against the request, then
    /// charges a follower lease and returns the result schema with a stream
    /// of the result batches, closed by one [`LiveFrame::Complete`] carrying
    /// the authenticated plan fingerprint, the delivered row and native byte
    /// totals, and the follower's scan evidence.
    ///
    /// # Errors
    /// Returns [`DispatchError::Terminal`] for a fence, context, claims, or
    /// preflight mismatch, and the follower's start classification otherwise.
    pub async fn execute(
        &self,
        request: ExecuteFragmentRequest,
    ) -> Result<(SchemaRef, WorkerAttemptStream), DispatchError> {
        let scribe = &self.scribe;
        let local_role = scribe.scribe_registered_role();
        if request.target_fence.role != ClusterRole::Scribe
            || request.target_fence.node_id != local_role.key.node_id
            || request.target_fence.fencing_token != local_role.fencing_token
            || request.reservation_id.as_uuid() != uuid::Uuid::nil()
        {
            tracing::error!(
                target_role = ?request.target_fence.role,
                target_node = %request.target_fence.node_id.as_uuid(),
                local_node = %local_role.key.node_id.as_uuid(),
                target_fence = request.target_fence.fencing_token,
                local_fence = local_role.fencing_token,
                reservation_is_nil = request.reservation_id.as_uuid().is_nil(),
                "Scribe fragment target does not match the local owner"
            );
            return Err(DispatchError::Terminal);
        }
        #[cfg(feature = "test-support")]
        let fault = take_scribe_fragment_fault();
        #[cfg(feature = "test-support")]
        match fault {
            Some(ScribeFragmentFault::UnavailableBeforeRows) => {
                return Err(DispatchError::Unavailable);
            }
            Some(ScribeFragmentFault::RejectTicket) => {
                tracing::error!("Scribe peer ticket verification failed (injected)");
                return Err(DispatchError::Terminal);
            }
            Some(ScribeFragmentFault::CapacityRefused) => return Err(DispatchError::Capacity),
            _ => {}
        }
        let verifier: Arc<dyn PeerTicketVerifier> = scribe.fragment_verifier();
        let verified = verifier
            .verify_peer_ticket(
                &request.context,
                local_role.key.node_id,
                local_role.fencing_token,
                chrono::Utc::now(),
            )
            .await
            .map_err(|error| {
                tracing::error!(?error, "Scribe peer context verification failed");
                DispatchError::Terminal
            })?;
        let claims = PeerTicketClaims::decode(verified.0.as_slice()).map_err(|error| {
            tracing::error!(?error, "Scribe peer context claims decode failed");
            DispatchError::Terminal
        })?;
        let tenant_id = uuid::Uuid::from_slice(&claims.tenant_id)
            .ok()
            .and_then(|tenant| wyrd_spec::DataTenantId::new(tenant).ok())
            .ok_or(DispatchError::Terminal)?;
        uuid::Uuid::from_slice(&claims.query_id).map_err(|_| DispatchError::Terminal)?;
        if claims.protocol_version != PEER_PROTOCOL_VERSION
            || claims.leader_node_id.as_slice() != request.leader_fence.node_id.as_uuid().as_bytes()
            || claims.leader_fence != request.leader_fence.fencing_token
            || claims.worker_fence != local_role.fencing_token
            || claims.fragment_digest != request.plan_fingerprint
            || claims.manifest_digest != request.plan_fingerprint
            || claims.permission_digest.is_empty()
            || request.assignments.is_empty()
            || request.assignments.iter().any(|assignment| {
                assignment.binding.tenant_id != tenant_id
                    || format!(
                        "{}.{}",
                        assignment.binding.namespace, assignment.binding.table
                    ) != claims.binding
                    || !assignment.persisted.files.is_empty()
                    || assignment.scribe_provider_cut.is_none()
            })
        {
            tracing::error!("Scribe peer physical claims validation failed");
            return Err(DispatchError::Terminal);
        }
        // Recomputed last, before any provider or tail I/O: the context names
        // the closed-predicate/projection closure the leader intended, and
        // this proves it matches what this Scribe worker actually received.
        match vala_bifrost_redux::oracle::peer::assignment_authority_digest_for(
            &request.assignments,
        ) {
            Ok(recomputed) if recomputed == claims.assignment_authority_digest => {}
            _ => {
                tracing::error!("Scribe peer assignment-authority digest mismatch");
                return Err(DispatchError::Terminal);
            }
        }
        let binding = request
            .assignments
            .first()
            .map(|assignment| assignment.binding.clone())
            .ok_or(DispatchError::Terminal)?;
        let authenticated = AuthenticatedFollowerContext {
            tenant_id,
            table_binding: &binding,
            reservation_id: &request.reservation_id,
            leader_fence: request.leader_fence.clone(),
            local_fence: request.target_fence.clone(),
        };
        authenticated_preflight(&request, &authenticated).map_err(|error| {
            tracing::error!(?error, "Scribe physical follower rejected the request");
            DispatchError::Terminal
        })?;
        // The Scribe follower runs the local partition count its own pod plan
        // derives, under its pool ceiling. Bytes are charged only as it grows.
        // It never spills: a Scribe node owns no governed Oracle spill
        // directory.
        let follower = scribe
            .resources()
            .follower_execution(vala_bifrost_redux::resources::ORACLE_PARTITION_MEMORY_BYTES)
            .map_err(|error| {
                tracing::error!(%error, "Scribe follower execution could not be built");
                DispatchError::Terminal
            })?;
        let execution = scribe
            .fragment_follower()
            .execute(&request, authenticated, &follower)
            .await
            .map_err(|error| {
                tracing::warn!(?error, "Scribe physical follower could not start");
                scribe_start_error(&error)
            })?;
        scribe.record_fragment_execution();
        // Split now, finalize after drain: the scan counters are written during
        // execution, and the leader has no physical scan of its own to report.
        let (mut batches, scan_evidence) = execution.split();
        let schema = batches.schema();
        let scribe_owner = Arc::clone(scribe);
        let plan_fingerprint = request.plan_fingerprint;
        let output = async_stream::try_stream! {
            let mut tally = NativeOutputTally::default();
            while let Some(batch) = batches.next().await {
                let batch = batch.map_err(|error| {
                    // The classification below keeps only a closed outcome, so
                    // without this the cause of a failed hot-tail fragment is
                    // lost entirely and the leader sees a bare `Unavailable`.
                    // The Oracle follower stream logs its own cause the same
                    // way; this is the Scribe half of that pair.
                    tracing::warn!(?error, "Scribe follower execution stream failed");
                    scribe_stream_error(&error)
                })?;
                tally.record(&batch)?;
                yield LiveFrame::Batch(batch);
                #[cfg(feature = "test-support")]
                if fault == Some(ScribeFragmentFault::UnavailableAfterFirstBatch) {
                    Err(DispatchError::Unavailable)?;
                }
            }
            #[cfg(feature = "test-support")]
            if fault == Some(ScribeFragmentFault::OmitFooter) {
                return;
            }
            scribe_owner.record_fragment_footer();
            yield tally.complete(plan_fingerprint, scan_evidence.finalize());
        };
        Ok((schema, Box::pin(output)))
    }
}

#[async_trait::async_trait]
impl OraclePeerTransport for ScribeFragmentExecutor {
    /// Scribe fragments are never reserved; the leader skips reservation.
    ///
    /// # Errors
    /// Always returns [`DispatchError::Terminal`].
    async fn reserve(
        &self,
        _worker: wyrd_spec::vala::api::NodeId,
        _request: wyrd_spec::vala::api::ReserveNodeSlotsRequest,
    ) -> Result<wyrd_spec::vala::api::ReserveNodeSlotsResponse, DispatchError> {
        Err(DispatchError::Terminal)
    }

    /// Nothing is reserved, so release is a no-op.
    ///
    /// # Errors
    /// Never fails.
    async fn release(
        &self,
        _worker: wyrd_spec::vala::api::NodeId,
        _request: wyrd_spec::vala::api::ReleaseNodeSlotsRequest,
    ) -> Result<(), DispatchError> {
        Ok(())
    }

    /// Executes the fragment in-process through [`Self::execute`].
    ///
    /// # Errors
    /// Returns the same failures as [`Self::execute`].
    async fn execute(
        &self,
        _worker: wyrd_spec::vala::api::NodeId,
        request: ExecuteFragmentRequest,
    ) -> Result<WorkerAttemptStream, DispatchError> {
        ScribeFragmentExecutor::execute(self, request)
            .await
            .map(|(_, stream)| stream)
    }
}

#[wyrd_tonic::tonic::async_trait]
impl OraclePeerService for OraclePeerGrpc {
    /// Worker attempt stream retaining the running slot until EOF or cancellation.
    type ExecuteFragmentStream =
        Pin<Box<dyn Stream<Item = Result<proto::WorkerAttemptFrame, Status>> + Send>>;
    /// Public-query frames proxied from the selected local Oracle.
    type ForwardQueryStream = crate::grpc::query::QueryGrpcStream;

    /// Reserves one bounded pending slot after its context is checked.
    ///
    /// # Errors
    /// Returns a context, conversion, or worker rejection status.
    async fn reserve_slots(
        &self,
        request: Request<ReserveNodeSlotsRequest>,
    ) -> Result<Response<proto::ReserveNodeSlotsResponse>, Status> {
        let mut wire = request.into_inner();
        let context = wire.context.take();
        let request = wyrd_spec::vala::api::ReserveNodeSlotsRequest::try_from(wire.clone())
            .map_err(conversion_status)?;
        // Fence liveness first, then the typed context, and only then any
        // capacity change: an unauthorized reserve must not charge the
        // follower even transiently.
        if self
            .bifrost
            .oracle()
            .ok_or_else(|| Status::failed_precondition("Oracle role is not configured"))?
            .cluster()
            .validate_live_oracle(request.leader_node_id, request.leader_fencing_token)
            .await
            .is_err()
        {
            self.audit_denial(BifrostSecurityViolationKind::PeerFence)
                .await?;
            return Err(Status::permission_denied("Oracle peer fence is not live"));
        }
        self.authorize_reservation(
            ReservationOperationV1::ReserveSlots,
            &wire,
            context,
            request.leader_node_id,
            request.leader_fencing_token,
            request.query_id.as_uuid(),
        )
        .await?;
        let worker = self
            .bifrost
            .oracle_peer_service()
            .ok_or_else(|| Status::failed_precondition("Oracle role is not configured"))?
            .worker();
        Ok(Response::new(worker.reserve(&request).into()))
    }

    /// Releases one matching reservation idempotently after its context is checked.
    ///
    /// # Errors
    /// Returns a context or conversion status.
    async fn release_slots(
        &self,
        request: Request<ReleaseNodeSlotsRequest>,
    ) -> Result<Response<proto::ReleaseNodeSlotsResponse>, Status> {
        let mut wire = request.into_inner();
        let context = wire.context.take();
        let request = wyrd_spec::vala::api::ReleaseNodeSlotsRequest::try_from(wire.clone())
            .map_err(conversion_status)?;
        // A release is state-changing, so it is authorized on exactly the same
        // terms as a reserve: a replayed release must not cancel capacity the
        // leader has since re-taken.
        self.authorize_reservation(
            ReservationOperationV1::ReleaseSlots,
            &wire,
            context,
            request.leader_node_id,
            request.leader_fencing_token,
            request.query_id.as_uuid(),
        )
        .await?;
        self.bifrost
            .oracle_peer_service()
            .ok_or_else(|| Status::failed_precondition("Oracle role is not configured"))?
            .worker()
            .release(&request);
        Ok(Response::new(proto::ReleaseNodeSlotsResponse {}))
    }

    /// Executes one verified Scribe fragment and streams a footer-terminated attempt.
    ///
    /// Only Scribe owns fragment work: Oracle peers exchange Analytical stages,
    /// so an Oracle-target fragment is refused before any decode. The
    /// executor's batches are encoded here, and only here, into Arrow IPC
    /// attempt frames: the schema first, each batch in order, then the footer
    /// built from the in-process completion. Server shutdown ends an open
    /// stream with an unavailable status instead of waiting for its reader.
    ///
    /// # Errors
    /// Returns a conversion, security, or execution status, and a
    /// permission-denied status for an Oracle-target fragment.
    async fn execute_fragment(
        &self,
        request: Request<proto::ExecuteFragmentRequest>,
    ) -> Result<Response<Self::ExecuteFragmentStream>, Status> {
        let request =
            ExecuteFragmentRequest::try_from(request.into_inner()).map_err(conversion_status)?;
        let plan_fingerprint = request.plan_fingerprint.clone();
        let (schema, mut stream) = match request.target_fence.role {
            // Oracle peers exchange Analytical stages, never fragments.
            ClusterRole::Oracle => {
                tracing::warn!("Oracle-target fragment refused: Oracle peers run no fragments");
                Err(DispatchError::Terminal)
            }
            ClusterRole::Scribe => match self.bifrost.scribe() {
                Some(scribe) => {
                    ScribeFragmentExecutor::new(Arc::clone(scribe))
                        .execute(request)
                        .await
                }
                None => {
                    tracing::error!("Scribe fragment reached a process without the Scribe owner");
                    Err(DispatchError::Terminal)
                }
            },
        }
        .map_err(dispatch_status)?;
        let shutdown = self.shutdown.clone();
        let output = async_stream::try_stream! {
            let mut encoder = AttemptEncoder::default();
            yield start_scribe_attempt(&mut encoder, schema).map_err(dispatch_status)?.into();
            loop {
                let next = tokio::select! {
                    () = shutdown.cancelled() => None,
                    frame = stream.next() => Some(frame),
                };
                let frame = next.ok_or_else(|| dispatch_status(DispatchError::Unavailable))?;
                let Some(frame) = frame else { break };
                match frame.map_err(dispatch_status)? {
                    LiveFrame::Batch(batch) => {
                        let (schema, batch) = encoder
                            .encode(&batch)
                            .map_err(|_| dispatch_status(DispatchError::Terminal))?;
                        if let Some(schema) = schema {
                            yield schema.into();
                        }
                        yield batch.into();
                    }
                    LiveFrame::Complete(completion) => {
                        yield encoder
                            .finish_physical(&plan_fingerprint, completion.scan_stats)
                            .map_err(|_| dispatch_status(DispatchError::Terminal))?
                            .into();
                        return;
                    }
                    LiveFrame::Wire(_) => Err(dispatch_status(DispatchError::Terminal))?,
                }
            }
        };
        Ok(Response::new(Box::pin(output)))
    }

    /// Checks one forwarding context and executes its exact local leader cut.
    ///
    /// # Errors
    /// Returns context, policy, fence, deadline, or query status before a
    /// response stream is published.
    async fn forward_query(
        &self,
        request: Request<ForwardQueryRequest>,
    ) -> Result<Response<Self::ForwardQueryStream>, Status> {
        let context = request
            .into_inner()
            .context
            .ok_or_else(|| Status::invalid_argument("forwarding context is required"))?;
        let context = wyrd_spec::vala::api::PeerContext::try_from(context)
            .map_err(|_| Status::invalid_argument("forwarding context is invalid"))?;
        self.bifrost
            .gate()
            .ensure_query_open()
            .map_err(|error| crate::grpc::query::query_status(error.into()))?;
        let forwarder = self
            .bifrost
            .query_forwarder()
            .ok_or(wyrd_spec::vala::error::BifrostError::OracleRoleUnavailable)
            .map_err(|error| crate::grpc::query::query_status(error.into()))?;
        #[cfg(feature = "test-support")]
        forwarder.silent_peer_for_test().hold_if_armed().await;
        let stream = forwarder
            .accept(context)
            .await
            .map_err(|error| crate::grpc::query::query_status(error.into()))?;
        Ok(crate::grpc::query::query_stream_response(stream))
    }
}

/// Maps malformed private fields without revealing runtime state.
fn conversion_status(error: PrivateConversionError) -> Status {
    Status::invalid_argument(error.to_string())
}

/// Maps peer pressure, retryable failures, and terminal contract failures separately.
fn dispatch_status(error: DispatchError) -> Status {
    match error {
        DispatchError::Unavailable => Status::unavailable(error.to_string()),
        DispatchError::EligibleSourceLoss { .. } => Status::failed_precondition(error.to_string()),
        DispatchError::Capacity => Status::resource_exhausted(error.to_string()),
        DispatchError::StaleObject | DispatchError::FileNotFound => {
            Status::not_found(error.to_string())
        }
        DispatchError::Terminal => Status::permission_denied(error.to_string()),
        // Distinct from `permission_denied` so the leader can recover the
        // tenant-isolation reason; see `execution_status_error`.
        DispatchError::TenantInvariant => Status::aborted(error.to_string()),
    }
}

/// Maps a Scribe follower failure before its stream exists to its dispatch outcome.
///
/// Only a source gone from this incarnation is live-source loss, which the
/// leader may degrade before rows. A local bound refusal is capacity. Schema,
/// projection, predicate, integrity, preflight, decode, and local execution
/// faults are terminal and fail the query.
fn scribe_start_error(error: &PhysicalPlanFollowerError) -> DispatchError {
    match error {
        PhysicalPlanFollowerError::Resolution(FollowerResolutionError::SourceLoss(_)) => {
            DispatchError::EligibleSourceLoss {
                cause: EligibleSourceLossCause::ProviderResolution,
            }
        }
        PhysicalPlanFollowerError::Resolution(FollowerResolutionError::Capacity(_)) => {
            DispatchError::Capacity
        }
        PhysicalPlanFollowerError::Resolution(FollowerResolutionError::Fault(_))
        | PhysicalPlanFollowerError::Execution(_)
        | PhysicalPlanFollowerError::Preflight(_)
        | PhysicalPlanFollowerError::PostResolutionDecode(_) => DispatchError::Terminal,
    }
}

/// Maps a failure inside an open Scribe follower stream to its dispatch outcome.
///
/// The stream reads this node's own memtable and staged runs, so a failure is
/// a local fault rather than loss of a remote source: it is terminal at any
/// row. Stale-object and tenant-invariant failures keep their own classes.
fn scribe_stream_error(error: &DataFusionError) -> DispatchError {
    if vala_bifrost_redux::oracle::is_stale_iceberg_object_error(error) {
        DispatchError::StaleObject
    } else if vala_bifrost_redux::oracle::is_tenant_invariant_error(error) {
        DispatchError::TenantInvariant
    } else {
        DispatchError::Terminal
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::datatypes::{DataType, Field, Schema};

    /// Only source loss degrades a Scribe fragment; every other cause fails.
    ///
    /// Resolution classes decided at the Scribe open survive to the dispatch
    /// outcome: a changed incarnation is eligible source loss, a bounded
    /// snapshot refusal is capacity, and a schema, projection, or local
    /// execution fault is terminal. A failure inside the open local stream is
    /// terminal rather than unavailable.
    ///
    /// # Panics
    ///
    /// Panics when any class maps to a different dispatch outcome.
    #[test]
    fn scribe_fragment_failure_classes_survive_to_dispatch() {
        let resolution = |class| scribe_start_error(&PhysicalPlanFollowerError::Resolution(class));
        assert!(matches!(
            resolution(FollowerResolutionError::SourceLoss("gone".to_owned())),
            DispatchError::EligibleSourceLoss { .. }
        ));
        assert!(matches!(
            resolution(FollowerResolutionError::Capacity("bound".to_owned())),
            DispatchError::Capacity
        ));
        assert!(matches!(
            resolution(FollowerResolutionError::Fault("schema".to_owned())),
            DispatchError::Terminal
        ));
        assert!(matches!(
            scribe_start_error(&PhysicalPlanFollowerError::Execution("open".to_owned())),
            DispatchError::Terminal
        ));
        assert!(matches!(
            scribe_stream_error(&DataFusionError::Execution("staged decode".to_owned())),
            DispatchError::Terminal
        ));
    }

    /// Proves the private tonic boundary preserves only stale-object failures as not-found.
    #[test]
    fn stale_object_dispatch_status_is_not_found() {
        let stale = dispatch_status(DispatchError::StaleObject);
        let outage = dispatch_status(DispatchError::Unavailable);

        assert_eq!(stale.code(), wyrd_tonic::tonic::Code::NotFound);
        assert_eq!(outage.code(), wyrd_tonic::tonic::Code::Unavailable);
    }

    /// An explicit-empty Scribe result still emits schema then a complete zero-row footer.
    #[test]
    fn empty_scribe_attempt_emits_schema_and_complete_footer() {
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Utf8,
            false,
        )]));
        let mut encoder = AttemptEncoder::default();
        let schema_frame = start_scribe_attempt(&mut encoder, schema).expect("schema starts");
        let footer_frame = encoder
            .finish_physical(
                "empty-scribe-plan",
                wyrd_spec::vala::api::WorkerScanStats::default(),
            )
            .expect("empty result completes");
        assert!(matches!(
            schema_frame,
            wyrd_spec::vala::api::WorkerAttemptFrame::Schema(_)
        ));
        assert!(matches!(
            &footer_frame,
            wyrd_spec::vala::api::WorkerAttemptFrame::Footer(footer)
                if footer.completed && footer.row_count == 0
        ));
    }
}
