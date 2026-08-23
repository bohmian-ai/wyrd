//! Authenticated generated-tonic adapter for private Oracle peer execution.

use std::pin::Pin;
use std::sync::Arc;

use futures_util::{Stream, StreamExt};
use vala_bifrost_redux::oracle::dispatcher::PEER_PROTOCOL_VERSION;
use vala_bifrost_redux::oracle::dispatcher::{DispatchError, WorkerExecution};
use vala_bifrost_redux::oracle::executor::AttemptEncoder;
use vala_bifrost_redux::oracle::follower::{
    AuthenticatedFollowerContext, PhysicalPlanFollowerError, authenticated_preflight,
};
use vala_bifrost_redux::oracle::peer::{PeerSecurityAudit, PeerTicketClaims, PeerTicketVerifier};
use wyrd_runtime::{Permission, Principal, PrincipalKind};
use wyrd_spec::vala::api::BifrostSecurityViolationKind;
use wyrd_spec::vala::api::{ClusterRole, PhysicalExecuteFragmentRequest};
use wyrd_tonic::private_conversion::PrivateConversionError;
use wyrd_tonic::prost::Message;
use wyrd_tonic::tonic::{Request, Response, Status};
use wyrd_tonic::wyrd::v1::oracle_peer_service_server::{
    OraclePeerService, OraclePeerServiceServer,
};
use wyrd_tonic::wyrd::v1::{
    self as proto, ExecuteFragmentRequest, ForwardQueryRequest, ReleaseNodeSlotsRequest,
    ReserveNodeSlotsRequest,
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

    /// Requires a verified service workload before peer state is touched.
    ///
    /// # Errors
    /// Returns an authentication or authorization status for missing/invalid workload identity.
    async fn authenticate(
        &self,
        metadata: &wyrd_tonic::tonic::metadata::MetadataMap,
    ) -> Result<Principal, Status> {
        let auth = match self.bifrost.gate().authenticate_peer(metadata).await {
            Ok(auth) => auth,
            Err(error) => {
                self.audit_denial(BifrostSecurityViolationKind::PeerAudience)
                    .await?;
                return Err(Status::unauthenticated(error.to_string()));
            }
        };
        if let Some(violation) = peer_authority_violation(&auth.principal) {
            tracing::error!(?violation, "Oracle peer workload authority denied");
            self.audit_denial(violation).await?;
            return Err(Status::permission_denied("Oracle peer authority denied"));
        }
        Ok(auth.principal)
    }

    /// Appends one scrubbed denial and fails closed if audit storage is unavailable.
    async fn audit_denial(&self, violation: BifrostSecurityViolationKind) -> Result<(), Status> {
        self.security_audit()?
            .append_unverified_ticket_rejection(violation)
            .await
            .map_err(|_| Status::unavailable("Oracle peer security audit unavailable"))
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

    /// Executes one Scribe-targeted physical fragment under Scribe's own fence and resources.
    async fn execute_scribe_fragment(
        &self,
        request: PhysicalExecuteFragmentRequest,
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
        let execution = scribe
            .fragment_follower()
            .execute(&request, authenticated, lease.memory_pool())
            .await
            .map_err(|error| match error {
                PhysicalPlanFollowerError::Preflight(_)
                | PhysicalPlanFollowerError::PostResolutionDecode(_) => DispatchError::Terminal,
                PhysicalPlanFollowerError::Resolution(_) => DispatchError::EligibleSourceLoss {
                    cause: vala_bifrost_redux::oracle::dispatcher::EligibleSourceLossCause::ProviderResolution,
                },
                PhysicalPlanFollowerError::Execution(_) => DispatchError::Unavailable,
            })?;
        scribe.record_fragment_execution();
        // Split now, finalize after drain: the scan counters are written during
        // execution, and the leader has no physical scan of its own to report.
        let (mut batches, scan_evidence) = execution.split();
        let plan_fingerprint = request.plan_fingerprint;
        let scribe_owner = Arc::clone(scribe);
        let output = async_stream::stream! {
            let _lease = lease;
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
                    if vala_bifrost_redux::oracle::is_stale_iceberg_object_error(&error) {
                        DispatchError::StaleObject
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

/// Classifies platform-service peer authority without exposing identity detail.
fn peer_authority_violation(principal: &Principal) -> Option<BifrostSecurityViolationKind> {
    if !matches!(principal.kind, PrincipalKind::Service { .. }) {
        return Some(BifrostSecurityViolationKind::PeerAudience);
    }
    if principal.tenant_id != wyrd_spec::DataTenantId::SYSTEM_OWNER {
        return Some(BifrostSecurityViolationKind::PeerTenant);
    }
    (!principal
        .effective_permissions
        .contains(&Permission::bifrost_oracle_peer_invoke()))
    .then_some(BifrostSecurityViolationKind::PeerAudience)
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
        self.authenticate(request.metadata()).await?;
        let request = wyrd_spec::vala::api::ReserveNodeSlotsRequest::try_from(request.into_inner())
            .map_err(conversion_status)?;
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
        self.authenticate(request.metadata()).await?;
        let request = wyrd_spec::vala::api::ReleaseNodeSlotsRequest::try_from(request.into_inner())
            .map_err(conversion_status)?;
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
        request: Request<ExecuteFragmentRequest>,
    ) -> Result<Response<Self::ExecuteFragmentStream>, Status> {
        self.authenticate(request.metadata()).await?;
        let request =
            wyrd_spec::vala::api::PhysicalExecuteFragmentRequest::try_from(request.into_inner())
                .map_err(conversion_status)?;
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
        self.authenticate(request.metadata()).await?;
        let envelope = request
            .into_inner()
            .envelope
            .ok_or_else(|| Status::invalid_argument("forwarding envelope is required"))?;
        let ticket = wyrd_spec::vala::api::SignedPeerTicket {
            key_id: envelope.key_id,
            claims_bytes: envelope.claims_bytes,
            signature: envelope.signature,
        };
        let stream = self
            .bifrost
            .gate()
            .accept_forwarded_query(ticket)
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::datatypes::{DataType, Field, Schema};
    use vala_bifrost_redux::oracle::attempt::AttemptBuffer;
    use wyrd_runtime::permission::PermissionSet;
    use wyrd_semver::VersionBlock;
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::{CardRef, CardRefScope};

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

    /// Peer authority rejects users, tenant mismatch, and missing permission before mutation.
    #[test]
    fn reserve_requires_oracle_peer_authority() {
        let card_ref = CardRef {
            kind: CardKind::Service,
            name: CardName::new("oracle-peer").expect("name"),
            version: VersionBlock::parse("1.0.0").expect("version"),
            space: SpaceName::new("system").expect("space"),
            uid: None,
        };
        let service_kind = PrincipalKind::Service {
            card_ref: card_ref.clone(),
            card_ref_scope: CardRefScope::own(&card_ref),
        };
        let principal = |kind, tenant_id, permissions| Principal {
            id: PrincipalId::new(uuid::Uuid::now_v7()),
            kind,
            tenant_id,
            roles: Vec::new(),
            effective_permissions: permissions,
        };
        assert_eq!(
            peer_authority_violation(&principal(
                PrincipalKind::User,
                wyrd_spec::DataTenantId::SYSTEM_OWNER,
                PermissionSet::from_iter([Permission::bifrost_oracle_peer_invoke()]),
            )),
            Some(BifrostSecurityViolationKind::PeerAudience)
        );
        assert_eq!(
            peer_authority_violation(&principal(
                service_kind.clone(),
                wyrd_spec::DataTenantId::new_v7(),
                PermissionSet::from_iter([Permission::bifrost_oracle_peer_invoke()]),
            )),
            Some(BifrostSecurityViolationKind::PeerTenant)
        );
        assert_eq!(
            peer_authority_violation(&principal(
                service_kind.clone(),
                wyrd_spec::DataTenantId::SYSTEM_OWNER,
                PermissionSet::new(),
            )),
            Some(BifrostSecurityViolationKind::PeerAudience)
        );
        assert_eq!(
            peer_authority_violation(&principal(
                service_kind,
                wyrd_spec::DataTenantId::SYSTEM_OWNER,
                PermissionSet::from_iter([Permission::bifrost_oracle_peer_invoke()]),
            )),
            None
        );
    }
}
