//! Authenticated generated-tonic adapter for private Oracle peer execution.

use std::pin::Pin;
use std::sync::Arc;

use futures_util::{Stream, StreamExt};
use vala_bifrost_redux::cluster::ClusterRegistry;
use vala_bifrost_redux::oracle::dispatcher::{DispatchError, OraclePeerWorker, WorkerExecution};
use vala_bifrost_redux::oracle::peer::PeerSecurityAudit;
use wyrd_runtime::{Permission, Principal, PrincipalKind};
use wyrd_spec::vala::api::BifrostSecurityViolationKind;
use wyrd_tonic::private_conversion::PrivateConversionError;
use wyrd_tonic::tonic::{Request, Response, Status};
use wyrd_tonic::wyrd::v1::oracle_peer_service_server::{
    OraclePeerService, OraclePeerServiceServer,
};
use wyrd_tonic::wyrd::v1::{
    self as proto, ExecuteFragmentRequest, ReleaseNodeSlotsRequest, ReserveNodeSlotsRequest,
};

use crate::AppState;

/// Private tonic service retaining one fenced worker runtime.
pub struct OraclePeerGrpc {
    /// Server auth state used to authenticate workload peers.
    state: AppState,
    /// Redux worker owner shared with the local transport.
    worker: Arc<OraclePeerWorker>,
    /// Authoritative membership owner used before reservation mutation.
    cluster: Arc<ClusterRegistry>,
    /// Scrubbed durable audit used for rejected peer authority.
    security_audit: Arc<dyn PeerSecurityAudit>,
}

impl OraclePeerGrpc {
    /// Creates the adapter around the retained worker runtime.
    #[must_use]
    pub fn new(
        state: AppState,
        worker: Arc<OraclePeerWorker>,
        cluster: Arc<ClusterRegistry>,
        security_audit: Arc<dyn PeerSecurityAudit>,
    ) -> Self {
        Self {
            state,
            worker,
            cluster,
            security_audit,
        }
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
        let verifier = self
            .state
            .auth
            .token_verifier
            .as_ref()
            .ok_or_else(|| Status::unavailable("auth backend not configured"))?;
        let auth =
            match vala_bifrost_redux::gate::auth::authenticate(verifier.as_ref(), metadata).await {
                Ok(auth) => auth,
                Err(error) => {
                    self.audit_denial(BifrostSecurityViolationKind::PeerAudience)
                        .await?;
                    return Err(Status::unauthenticated(error.to_string()));
                }
            };
        if let Some(violation) = peer_authority_violation(&auth.principal) {
            self.audit_denial(violation).await?;
            return Err(Status::permission_denied("Oracle peer authority denied"));
        }
        Ok(auth.principal)
    }

    /// Appends one scrubbed denial and fails closed if audit storage is unavailable.
    async fn audit_denial(&self, violation: BifrostSecurityViolationKind) -> Result<(), Status> {
        self.security_audit
            .append_unverified_ticket_rejection(violation)
            .await
            .map_err(|_| Status::unavailable("Oracle peer security audit unavailable"))
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
            .cluster
            .validate_live_oracle(request.leader_node_id, request.leader_fencing_token)
            .await
            .is_err()
        {
            self.audit_denial(BifrostSecurityViolationKind::PeerFence)
                .await?;
            return Err(Status::permission_denied("Oracle peer fence is not live"));
        }
        Ok(Response::new(self.worker.reserve(&request).into()))
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
        self.worker.release(&request);
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
        let request = wyrd_spec::vala::api::ExecuteFragmentRequest::try_from(request.into_inner())
            .map_err(conversion_status)?;
        let WorkerExecution { mut stream } = self
            .worker
            .execute(request)
            .await
            .map_err(dispatch_status)?;
        let output = async_stream::stream! {
            while let Some(frame) = stream.next().await {
                yield frame.map(Into::into).map_err(dispatch_status);
            }
        };
        Ok(Response::new(Box::pin(output)))
    }
}

/// Maps malformed private fields without revealing runtime state.
fn conversion_status(error: PrivateConversionError) -> Status {
    Status::invalid_argument(error.to_string())
}

/// Maps retryable peer failures separately from terminal security/contract failures.
fn dispatch_status(error: DispatchError) -> Status {
    match error {
        DispatchError::Retryable => Status::unavailable(error.to_string()),
        DispatchError::StaleObject => Status::not_found(error.to_string()),
        DispatchError::Terminal => Status::permission_denied(error.to_string()),
        DispatchError::Exhausted => Status::aborted(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let outage = dispatch_status(DispatchError::Retryable);

        assert_eq!(stale.code(), wyrd_tonic::tonic::Code::NotFound);
        assert_eq!(outage.code(), wyrd_tonic::tonic::Code::Unavailable);
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
