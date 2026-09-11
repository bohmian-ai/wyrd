//! Authenticated generated-tonic adapter for private Oracle peer execution.

use std::pin::Pin;
use std::sync::Arc;

use futures_util::{Stream, StreamExt};
use vala_bifrost_redux::oracle::dispatcher::{
    AttemptEncoder, DispatchError, PEER_PROTOCOL_VERSION, WorkerExecution,
};
use vala_bifrost_redux::oracle::follower::{
    AuthenticatedFollowerContext, PhysicalPlanFollowerError, authenticated_preflight,
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

use crate::state::Bifrost;

/// Private tonic service retaining one fenced worker runtime.
pub struct OraclePeerGrpc {
    /// One published Bifrost facade owning every selectable peer capability.
    bifrost: Arc<Bifrost>,
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
    #[must_use]
    pub const fn new(bifrost: Arc<Bifrost>) -> Self {
        Self { bifrost }
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
    /// The ticket is detached from the request first, so the digest is taken
    /// over exactly the encoding the leader signed: the request with its ticket
    /// field cleared. Everything the follower compares against — its own node
    /// identity, its own current fence, the leader identity the cluster
    /// confirmed live — is derived here rather than read from the message.
    ///
    /// # Errors
    ///
    /// Returns `FailedPrecondition` when no Oracle role owns this node's
    /// reservation authority, `Unauthenticated` when no ticket is presented,
    /// and `PermissionDenied` for a ticket that is malformed, misbound,
    /// expired, or replayed. The refusal is durably audited before it returns;
    /// an audit that cannot commit surfaces as `Unavailable`.
    async fn authorize_reservation<T: Message>(
        &self,
        operation: ReservationOperationV1,
        ticket_free: &T,
        ticket: Option<proto::SignedPeerTicket>,
        leader_node_id: wyrd_spec::vala::api::NodeId,
        leader_fence: u64,
        query_id: uuid::Uuid,
    ) -> Result<(), Status> {
        let oracle = self
            .bifrost
            .oracle()
            .ok_or_else(|| Status::failed_precondition("Oracle role is not configured"))?;
        let Some(ticket) = ticket else {
            self.audit_denial(BifrostSecurityViolationKind::PeerSignature)
                .await?;
            return Err(Status::unauthenticated(
                "Bifrost peer reservation ticket is absent",
            ));
        };
        let ticket = wyrd_spec::vala::api::SignedPeerTicket::try_from(ticket)
            .map_err(|_| Status::permission_denied("Bifrost peer reservation ticket is invalid"))?;
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
                &ticket,
                &binding,
                &ticket_free.encode_to_vec(),
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

    /// Executes one Scribe-targeted physical fragment under Scribe's own fence and resources.
    async fn execute_scribe_fragment(
        &self,
        request: ExecuteFragmentRequest,
    ) -> Result<WorkerExecution, DispatchError> {
        let scribe = self.bifrost.scribe().ok_or_else(|| {
            tracing::error!("Scribe fragment reached a process without the Scribe owner");
            DispatchError::Terminal
        })?;
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
        let verifier: Arc<dyn PeerTicketVerifier> = scribe.fragment_verifier();
        let verified = verifier
            .verify_peer_ticket(
                &request.ticket,
                local_role.key.node_id,
                local_role.fencing_token,
                chrono::Utc::now(),
            )
            .await
            .map_err(|error| {
                tracing::error!(?error, "Scribe peer ticket verification failed");
                DispatchError::Terminal
            })?;
        let claims = PeerTicketClaims::decode(verified.0.as_slice()).map_err(|error| {
            tracing::error!(?error, "Scribe peer ticket claims decode failed");
            DispatchError::Terminal
        })?;
        let tenant_id = uuid::Uuid::from_slice(&claims.tenant_id)
            .ok()
            .and_then(|tenant| wyrd_spec::DataTenantId::new(tenant).ok())
            .ok_or(DispatchError::Terminal)?;
        let query_id =
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
        // Recomputed last, before any provider or tail I/O: a valid
        // signature only proves the claims were not tampered with in
        // transit, not that the signed closed-predicate/projection closure
        // matches what this Scribe worker actually received.
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
        let request_id = wyrd_spec::request_id::RequestId::parse(&query_id.to_string())
            .map_err(|_| DispatchError::Terminal)?;
        let lease = scribe
            .resources()
            .try_acquire_follower(
                &request_id,
                vala_bifrost_redux::resources::ORACLE_PARTITION_MEMORY_BYTES,
            )
            .map_err(|_| DispatchError::Capacity)?;
        // The Scribe follower is shaped by the lease this node just charged:
        // one partition, because a hot-tail fragment is a single sequential
        // cut, and the batch size the granted bytes support.
        let sessions = vala_bifrost_redux::oracle::follower::FollowerSessionFactory::for_grant(
            lease.memory_pool(),
            lease.memory_bytes(),
            1,
        );
        let execution = scribe
            .fragment_follower()
            .execute(&request, authenticated, &sessions)
            .await
            .map_err(|error| match error {
                PhysicalPlanFollowerError::Preflight(_)
                | PhysicalPlanFollowerError::PostResolutionDecode(_)
                | PhysicalPlanFollowerError::AuthorityAlreadyInstalled => DispatchError::Terminal,
                PhysicalPlanFollowerError::Resolution(_) => DispatchError::EligibleSourceLoss {
                    cause: vala_bifrost_redux::oracle::dispatcher::EligibleSourceLossCause::ProviderResolution,
                },
                PhysicalPlanFollowerError::Execution(_) => DispatchError::Unavailable,
            })?;
        scribe.record_fragment_execution();
        // Split now, finalize after drain: the scan counters are written during
        // execution, and the leader has no physical scan of its own to report.
        let (mut batches, scan_evidence, reader_protection) = execution.split();
        let plan_fingerprint = request.plan_fingerprint;
        let scribe_owner = Arc::clone(scribe);
        let output = async_stream::stream! {
            let _lease = lease;
            // Retained through the whole attempt so a Scribe fragment that
            // happens to name a snapshot keeps it protected until it is done.
            let _reader_protection = reader_protection;
            let mut encoder = AttemptEncoder::default();
            match start_scribe_attempt(&mut encoder, batches.schema()) {
                Ok(schema) => yield Ok(schema),
                Err(_) => {
                    yield Err(DispatchError::Terminal);
                    return;
                }
            }
            while let Some(batch) = batches.next().await {
                let batch = batch.map_err(|error| {
                    // The classification below keeps only a closed outcome, so
                    // without this the cause of a failed hot-tail fragment is
                    // lost entirely and the leader sees a bare `Unavailable`.
                    // The Oracle follower stream logs its own cause the same
                    // way; this is the Scribe half of that pair.
                    tracing::warn!(?error, "Scribe follower execution stream failed");
                    if vala_bifrost_redux::oracle::is_stale_iceberg_object_error(&error) {
                        DispatchError::StaleObject
                    } else if vala_bifrost_redux::oracle::is_tenant_invariant_error(&error) {
                        DispatchError::TenantInvariant
                    } else {
                        DispatchError::Unavailable
                    }
                })?;
                let (schema, batch) = encoder.encode(&batch).map_err(|_| DispatchError::Terminal)?;
                if let Some(schema) = schema {
                    yield Ok(schema);
                }
                yield Ok(batch);
            }
            let footer = encoder
                .finish_physical(&plan_fingerprint, scan_evidence.finalize())
                .map_err(|_| DispatchError::Terminal);
            if footer.is_ok() {
                scribe_owner.record_fragment_footer();
            }
            yield footer;
        };
        Ok(WorkerExecution {
            stream: Box::pin(output),
        })
    }
}

/// Reads the peer identity the authentication layer established for a request.
///
/// The layer runs before the body is polled, so an admitted handler always
/// finds this present. Its absence means the service was mounted outside the
/// peer boundary, which is refused rather than reconstructed here: a handler
/// that could rebuild identity from metadata would be a second authentication
/// path, and the plane is required to have exactly one.
///
/// # Errors
///
/// Returns `Unauthenticated` when no context is attached.
fn peer_context<T>(
    request: &Request<T>,
) -> Result<&vala_bifrost_redux::oracle::peer::AuthenticatedPeerContext, Status> {
    request
        .extensions()
        .get::<vala_bifrost_redux::oracle::peer::AuthenticatedPeerContext>()
        .ok_or_else(|| Status::unauthenticated("Bifrost peer identity is absent"))
}

#[wyrd_tonic::tonic::async_trait]
impl OraclePeerService for OraclePeerGrpc {
    /// Worker attempt stream retaining the running slot until EOF or cancellation.
    type ExecuteFragmentStream =
        Pin<Box<dyn Stream<Item = Result<proto::WorkerAttemptFrame, Status>> + Send>>;
    /// Authenticated public-query frames proxied from the selected local Oracle.
    type ForwardQueryStream = crate::grpc::query::QueryGrpcStream;

    /// Reserves one bounded pending slot after workload authentication.
    ///
    /// # Errors
    /// Returns an authentication, conversion, or worker rejection status.
    async fn reserve_slots(
        &self,
        request: Request<ReserveNodeSlotsRequest>,
    ) -> Result<Response<proto::ReserveNodeSlotsResponse>, Status> {
        peer_context(&request)?;
        let mut wire = request.into_inner();
        let ticket = wire.ticket.take();
        let request = wyrd_spec::vala::api::ReserveNodeSlotsRequest::try_from(wire.clone())
            .map_err(conversion_status)?;
        // Fence liveness first, then the purpose ticket, and only then any
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
            ticket,
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
        Ok(Response::new(worker.reserve(&request).await.into()))
    }

    /// Releases one matching reservation idempotently after workload authentication.
    ///
    /// # Errors
    /// Returns an authentication or conversion status.
    async fn release_slots(
        &self,
        request: Request<ReleaseNodeSlotsRequest>,
    ) -> Result<Response<proto::ReleaseNodeSlotsResponse>, Status> {
        peer_context(&request)?;
        let mut wire = request.into_inner();
        let ticket = wire.ticket.take();
        let request = wyrd_spec::vala::api::ReleaseNodeSlotsRequest::try_from(wire.clone())
            .map_err(conversion_status)?;
        // A release is state-changing, so it is authorized on exactly the same
        // terms as a reserve: a replayed release must not cancel capacity the
        // leader has since re-taken.
        self.authorize_reservation(
            ReservationOperationV1::ReleaseSlots,
            &wire,
            ticket,
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

    /// Executes verified immutable work and streams a footer-terminated attempt.
    ///
    /// # Errors
    /// Returns an authentication, conversion, security, or execution status.
    async fn execute_fragment(
        &self,
        request: Request<proto::ExecuteFragmentRequest>,
    ) -> Result<Response<Self::ExecuteFragmentStream>, Status> {
        peer_context(&request)?;
        let request =
            ExecuteFragmentRequest::try_from(request.into_inner()).map_err(conversion_status)?;
        let WorkerExecution { mut stream } = match request.target_fence.role {
            ClusterRole::Oracle => match self.bifrost.oracle_peer_service() {
                Some(peer) => peer.worker().execute(request).await,
                None => Err(DispatchError::Terminal),
            },
            ClusterRole::Scribe => self.execute_scribe_fragment(request).await,
        }
        .map_err(dispatch_status)?;
        let output = async_stream::stream! {
            while let Some(frame) = stream.next().await {
                yield frame.map(Into::into).map_err(dispatch_status);
            }
        };
        Ok(Response::new(Box::pin(output)))
    }

    /// Verifies one authenticated forwarding envelope and executes its exact local leader cut.
    ///
    /// # Errors
    /// Returns authentication, signature, policy, fence, deadline, or query status before a
    /// response stream is published.
    async fn forward_query(
        &self,
        request: Request<ForwardQueryRequest>,
    ) -> Result<Response<Self::ForwardQueryStream>, Status> {
        peer_context(&request)?;
        let envelope = request
            .into_inner()
            .envelope
            .ok_or_else(|| Status::invalid_argument("forwarding envelope is required"))?;
        let ticket = wyrd_spec::vala::api::SignedPeerTicket {
            key_id: envelope.key_id,
            claims_bytes: envelope.claims_bytes,
            signature: envelope.signature,
        };
        self.bifrost
            .gate()
            .ensure_query_open()
            .map_err(|error| crate::grpc::query::query_status(error.into()))?;
        let stream = self
            .bifrost
            .query_forwarder()
            .ok_or(wyrd_spec::vala::error::BifrostError::OracleRoleUnavailable)
            .map_err(|error| crate::grpc::query::query_status(error.into()))?
            .accept(ticket)
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
        DispatchError::Partial { .. } => Status::deadline_exceeded(error.to_string()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::datatypes::{DataType, Field, Schema};
    use vala_bifrost_redux::oracle::attempt::AttemptBuffer;

    /// The Scribe follower session is shaped by the lease this node charged.
    ///
    /// A hot-tail fragment runs under the Scribe follower lease acquired above,
    /// so its session comes from that lease's granted bytes and the single
    /// partition a sequential cut offers — never from a fixed batch size or a
    /// value the leader supplied in the request.
    #[test]
    fn scribe_follower_session_shape_contract() {
        let granted = vala_bifrost_redux::resources::ORACLE_PARTITION_MEMORY_BYTES;
        let sessions = vala_bifrost_redux::oracle::follower::FollowerSessionFactory::for_grant(
            std::sync::Arc::new(datafusion::execution::memory_pool::GreedyMemoryPool::new(
                granted,
            )),
            granted,
            1,
        );
        let expected = vala_bifrost_redux::resources::OracleSessionShape::for_grant(granted, 1, 1);

        assert_eq!(sessions.shape(1), expected);
        assert_eq!(
            expected.target_partitions,
            vala_bifrost_redux::resources::oracle_partitions_for_work(1, 1),
            "a hot-tail cut is one sequential scan target under the shared floor"
        );
        assert_ne!(
            expected.batch_size, 1_024,
            "the removed fixed batch size is not the admitted shape"
        );

        let narrow = vala_bifrost_redux::resources::ORACLE_PARTITION_WORKING_MEMORY_BYTES;
        let smaller = vala_bifrost_redux::oracle::follower::FollowerSessionFactory::for_grant(
            std::sync::Arc::new(datafusion::execution::memory_pool::GreedyMemoryPool::new(
                narrow,
            )),
            narrow,
            1,
        );
        assert_ne!(
            smaller.shape(1).batch_size,
            expected.batch_size,
            "a smaller lease produces a smaller batch size"
        );
    }

    /// Proves the private tonic boundary preserves only stale-object failures as not-found.
    #[test]
    fn stale_object_dispatch_status_is_not_found() {
        let stale = dispatch_status(DispatchError::StaleObject);
        let outage = dispatch_status(DispatchError::Unavailable);

        assert_eq!(stale.code(), wyrd_tonic::tonic::Code::NotFound);
        assert_eq!(outage.code(), wyrd_tonic::tonic::Code::Unavailable);
    }

    /// An explicit-empty Scribe result still emits schema then a validated complete footer.
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

        let mut attempt = AttemptBuffer::new(16 * 1_024);
        attempt.push(schema_frame).expect("schema is first");
        attempt.push(footer_frame).expect("footer is last");
        let validated = attempt.finish().expect("zero-row footer validates");
        assert_eq!(validated.footer.row_count, 0);
        assert_eq!(validated.batches.count(), 0);
    }
}
