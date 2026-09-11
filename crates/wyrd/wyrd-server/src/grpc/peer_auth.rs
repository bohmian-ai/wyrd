//! The one authentication boundary in front of the private peer listener.
//!
//! Every service mounted on the peer listener is wrapped here, and this is the
//! only place a peer request's identity is established. The layer runs to a
//! verdict on connection and metadata alone: the request body is not polled,
//! buffered, decompressed, or decoded until an [`AuthenticatedPeerContext`]
//! exists for it. That ordering is what makes a refused caller cost the
//! process nothing beyond a header parse and one bounded audit record.
//!
//! The layer answers three separate questions in order, and conflating them is
//! the failure mode it exists to prevent:
//!
//! 1. did a certificate the peer CA issued carry this connection? (transport
//!    admission, already settled by mTLS before the request arrives here);
//! 2. is the workload credential on this request the one configured peer
//!    Service principal? (this layer); and
//! 3. is this exact operation authorized? (a purpose ticket, verified by the
//!    handler after the context exists).

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use sha2::{Digest as _, Sha256};
use vala_bifrost_redux::oracle::dispatcher::OraclePeerCredentials;
use vala_bifrost_redux::oracle::peer::{AuthenticatedPeerContext, PeerSecurityAudit};
use wyrd_runtime::{Permission, Principal, PrincipalKind};
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::BifrostSecurityViolationKind;
use wyrd_tonic::tonic::Status;
use wyrd_tonic::tonic::body::Body;
use wyrd_tonic::tonic::codegen::http::{Request, Response};
use wyrd_tonic::tonic::metadata::MetadataMap;
use wyrd_tonic::tonic::server::NamedService;

use crate::state::WyrdTokenVerifier;

/// The exact peer Service principal this process admits on its peer listener.
///
/// Resolved once at boot by exchanging and verifying the process's own
/// configured peer credential. Deriving it from the credential rather than
/// from a second configuration input means the identity a process presents and
/// the identity it accepts cannot drift apart: they are the same principal by
/// construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerWorkloadIdentity {
    /// Stable identity of the peer Service principal.
    principal_id: wyrd_spec::auth::PrincipalId,
    /// Service card the peer principal is bound to.
    service_card: wyrd_spec::reference::CardRef,
}

impl PeerWorkloadIdentity {
    /// Resolves the configured peer identity by verifying this process's own
    /// credential.
    ///
    /// # Errors
    ///
    /// Returns a redacted message when the credential cannot be exchanged,
    /// fails verification, is not a platform Service principal, does not
    /// belong to the control tenant, or lacks `bifrost.peer.invoke`. Every one
    /// of those is a boot failure: a process that cannot prove its own peer
    /// identity must not serve a peer listener.
    pub async fn resolve(
        credentials: &dyn OraclePeerCredentials,
        verifier: &WyrdTokenVerifier,
    ) -> Result<Self, String> {
        let bearer = credentials
            .bearer(false)
            .await
            .map_err(|_| "Bifrost peer credential exchange failed".to_owned())?;
        let mut metadata = MetadataMap::new();
        let value = format!("Bearer {bearer}")
            .parse()
            .map_err(|_| "Bifrost peer access token is invalid".to_owned())?;
        metadata.insert("x-wyrd-access-token", value);
        let authenticated = vala_bifrost_redux::gate::auth::authenticate(verifier, &metadata)
            .await
            .map_err(|_| "Bifrost peer access token was rejected".to_owned())?;
        Self::from_principal(&authenticated.principal)
    }

    /// Projects one verified principal into the configured peer identity.
    ///
    /// # Errors
    ///
    /// Returns a redacted message when the principal is not a control-tenant
    /// Service principal holding the peer permission.
    fn from_principal(principal: &Principal) -> Result<Self, String> {
        let PrincipalKind::Service { card_ref, .. } = &principal.kind else {
            return Err("Bifrost peer credential is not a platform service".to_owned());
        };
        if principal.tenant_id != DataTenantId::SYSTEM_OWNER
            || !principal
                .effective_permissions
                .contains(&Permission::bifrost_peer_invoke())
        {
            return Err("Bifrost peer credential lacks platform service authority".to_owned());
        }
        Ok(Self {
            principal_id: principal.id,
            service_card: card_ref.clone(),
        })
    }

    /// Classifies one inbound principal against this configured identity.
    ///
    /// Returns `None` when the principal is the configured peer Service
    /// principal, and otherwise the violation kind the refusal is audited as.
    fn violation(&self, principal: &Principal) -> Option<BifrostSecurityViolationKind> {
        let PrincipalKind::Service { card_ref, .. } = &principal.kind else {
            return Some(BifrostSecurityViolationKind::PeerAudience);
        };
        if principal.tenant_id != DataTenantId::SYSTEM_OWNER {
            return Some(BifrostSecurityViolationKind::PeerTenant);
        }
        if !principal
            .effective_permissions
            .contains(&Permission::bifrost_peer_invoke())
            || principal.id != self.principal_id
            || card_ref != &self.service_card
        {
            return Some(BifrostSecurityViolationKind::PeerAudience);
        }
        None
    }
}

/// Shared owner of the peer plane's one authentication decision.
///
/// Cloned into every mounted private service, so all of them consult the same
/// verifier, the same configured identity, the same audit sink, and the same
/// refusal budget.
#[derive(Clone)]
pub struct PeerWorkloadAuthLayer {
    /// Composed peer authentication state shared by every wrapped service.
    shared: Arc<PeerWorkloadAuthState>,
}

/// State one peer listener's authentication boundary owns.
struct PeerWorkloadAuthState {
    /// The verifier public ingest also uses; the peer plane adds no second one.
    verifier: Arc<WyrdTokenVerifier>,
    /// The one principal this listener admits.
    expected: PeerWorkloadIdentity,
    /// Sink for canonical refusal records.
    audit: Arc<dyn PeerSecurityAudit>,
    /// Bound on concurrent refusal audit work.
    ///
    /// Refusals are attacker-controlled in volume, so the audit they produce is
    /// budgeted. Exhausting the budget never admits a request: the refusal
    /// stands and only its record is dropped, with a warning.
    denials: tokio::sync::Semaphore,
}

impl PeerWorkloadAuthLayer {
    /// Composes the peer plane's authentication boundary.
    #[must_use]
    pub fn new(
        verifier: Arc<WyrdTokenVerifier>,
        expected: PeerWorkloadIdentity,
        audit: Arc<dyn PeerSecurityAudit>,
        denial_concurrency: usize,
    ) -> Self {
        Self {
            shared: Arc::new(PeerWorkloadAuthState {
                verifier,
                expected,
                audit,
                denials: tokio::sync::Semaphore::new(denial_concurrency),
            }),
        }
    }

    /// Wraps one mounted private service in this boundary.
    #[must_use]
    pub fn wrap<S>(&self, inner: S) -> PeerWorkloadAuth<S> {
        PeerWorkloadAuth {
            inner,
            shared: Arc::clone(&self.shared),
        }
    }
}

impl PeerWorkloadAuthState {
    /// Authenticates one request's metadata and connection identity.
    ///
    /// # Errors
    ///
    /// Returns the refusal status the caller answers with. The body has not
    /// been touched at any point on this path.
    async fn admit(
        &self,
        parts: &wyrd_tonic::tonic::codegen::http::request::Parts,
    ) -> Result<AuthenticatedPeerContext, Status> {
        let metadata = MetadataMap::from_headers(parts.headers.clone());
        let credential_digest = credential_digest(&metadata);
        let authenticated =
            match vala_bifrost_redux::gate::auth::authenticate(&self.verifier, &metadata).await {
                Ok(authenticated) => authenticated,
                Err(error) => {
                    self.audit_denial(BifrostSecurityViolationKind::PeerAudience)
                        .await;
                    return Err(Status::unauthenticated(error.to_string()));
                }
            };
        if let Some(violation) = self.expected.violation(&authenticated.principal) {
            tracing::error!(?violation, "Bifrost peer workload authority denied");
            self.audit_denial(violation).await;
            return Err(Status::permission_denied("Bifrost peer authority denied"));
        }
        let PrincipalKind::Service { card_ref, .. } = &authenticated.principal.kind else {
            // Unreachable: `violation` refuses every non-service principal.
            return Err(Status::permission_denied("Bifrost peer authority denied"));
        };
        Ok(AuthenticatedPeerContext::new(
            DataTenantId::SYSTEM_OWNER,
            authenticated.principal.id,
            card_ref.clone(),
            authenticated.principal.effective_permissions.clone(),
            credential_digest,
            certificate_digest(parts),
            authenticated.request_id,
            chrono::Utc::now(),
        ))
    }

    /// Records one refusal within the plane's bounded audit budget.
    ///
    /// Deliberately infallible for the caller: the request is already refused,
    /// so neither a full budget nor an audit-store failure can change the
    /// outcome. Both are logged rather than converted into a different status,
    /// which would tell an unauthenticated caller about internal state.
    async fn audit_denial(&self, violation: BifrostSecurityViolationKind) {
        let Ok(_permit) = self.denials.try_acquire() else {
            tracing::warn!(
                ?violation,
                "Bifrost peer refusal audit budget is saturated; record dropped"
            );
            return;
        };
        if self
            .audit
            .append_unverified_ticket_rejection(violation)
            .await
            .is_err()
        {
            tracing::error!(?violation, "Bifrost peer refusal audit append failed");
        }
    }
}

/// Digests the presented workload credential without retaining it.
///
/// The digest is what audit and telemetry correlate on: it identifies which
/// credential was used across records without ever storing the credential.
/// An absent credential digests to the empty string, which no token produces.
fn credential_digest(metadata: &MetadataMap) -> String {
    metadata
        .get("x-wyrd-access-token")
        .and_then(|value| value.to_str().ok())
        .map_or_else(String::new, |value| {
            let digest = Sha256::digest(value.as_bytes());
            hex::encode(&digest[..16])
        })
}

/// Digests the accepted peer certificate, when the transport exposed one.
///
/// The certificate proves the connection, never the runtime node: this value is
/// carried for audit correlation and is deliberately never compared to a Wyrd
/// `NodeId`.
fn certificate_digest(parts: &wyrd_tonic::tonic::codegen::http::request::Parts) -> Option<String> {
    let info = parts
        .extensions
        .get::<wyrd_tonic::tonic::transport::server::TlsConnectInfo<
            wyrd_tonic::tonic::transport::server::TcpConnectInfo,
        >>()?;
    let certs = info.peer_certs()?;
    let leaf = certs.first()?;
    Some(hex::encode(Sha256::digest(leaf.as_ref())))
}

/// One private service that only sees authenticated requests.
#[derive(Clone)]
pub struct PeerWorkloadAuth<S> {
    /// Service invoked only after an [`AuthenticatedPeerContext`] exists.
    inner: S,
    /// Shared authentication state for this listener.
    shared: Arc<PeerWorkloadAuthState>,
}

impl<S> NamedService for PeerWorkloadAuth<S>
where
    S: NamedService,
{
    /// The boundary is transparent to routing: it serves the inner name.
    const NAME: &'static str = S::NAME;
}

impl<S> wyrd_tonic::tonic::codegen::Service<Request<Body>> for PeerWorkloadAuth<S>
where
    S: wyrd_tonic::tonic::codegen::Service<
            Request<Body>,
            Response = Response<Body>,
            Error = std::convert::Infallible,
        > + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
{
    type Response = Response<Body>;
    type Error = std::convert::Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    /// Defers readiness to the wrapped service; the check itself is per request.
    fn poll_ready(&mut self, context: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(context)
    }

    /// Establishes peer identity, then forwards the untouched request.
    fn call(&mut self, request: Request<Body>) -> Self::Future {
        // Same Tower readiness contract the transport-admission wrapper keeps:
        // the future owns the value that was polled ready, and an unreadied
        // clone takes its place.
        let unreadied = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, unreadied);
        let shared = Arc::clone(&self.shared);
        Box::pin(async move {
            let (mut parts, body) = request.into_parts();
            let context = match shared.admit(&parts).await {
                Ok(context) => context,
                Err(status) => return Ok(status.into_http()),
            };
            parts.extensions.insert(context);
            inner.call(Request::from_parts(parts, body)).await
        })
    }
}

#[cfg(test)]
mod tests {
    use wyrd_runtime::permission::PermissionSet;
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::{CardRef, CardRefScope};

    use super::*;

    /// Builds the Service card reference the configured peer principal is bound
    /// to, or a differently named one for the near-miss case.
    fn service_card(name: &str) -> CardRef {
        CardRef {
            kind: CardKind::Service,
            name: CardName::new(name).expect("static card name is valid"),
            version: wyrd_semver::VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: SpaceName::new("system").expect("static space name is valid"),
            uid: None,
        }
    }

    /// Builds one principal with the exact kind, tenant, and permissions a case
    /// needs, leaving every other field constant.
    fn principal(
        id: PrincipalId,
        kind: PrincipalKind,
        tenant_id: DataTenantId,
        permissions: PermissionSet,
    ) -> Principal {
        Principal {
            id,
            kind,
            tenant_id,
            roles: Vec::new(),
            effective_permissions: permissions,
        }
    }

    /// Builds the Service principal kind bound to `card`.
    fn service_kind(card: &CardRef) -> PrincipalKind {
        PrincipalKind::Service {
            card_ref: card.clone(),
            card_ref_scope: CardRefScope::own(card),
        }
    }

    /// The layer admits exactly the configured peer Service principal.
    ///
    /// Every near miss is classified rather than admitted: a human principal, a
    /// data-tenant principal, a platform service without the peer permission, a
    /// different principal id, and a different Service card. The identity is
    /// resolved from this process's own credential, so a divergence in any of
    /// those fields means the caller is not this plane's one workload.
    #[test]
    fn only_the_configured_peer_service_principal_is_admitted() {
        let card = service_card("bifrost-peer");
        let expected = PeerWorkloadIdentity {
            principal_id: PrincipalId::new(uuid::Uuid::from_u128(1)),
            service_card: card.clone(),
        };
        let peer = PermissionSet::from_iter([Permission::bifrost_peer_invoke()]);

        assert_eq!(
            expected.violation(&principal(
                expected.principal_id,
                PrincipalKind::User,
                DataTenantId::SYSTEM_OWNER,
                peer.clone(),
            )),
            Some(BifrostSecurityViolationKind::PeerAudience),
            "a human principal is never this plane's workload"
        );
        assert_eq!(
            expected.violation(&principal(
                expected.principal_id,
                service_kind(&card),
                DataTenantId::new_v7(),
                peer.clone(),
            )),
            Some(BifrostSecurityViolationKind::PeerTenant),
            "a data-tenant service is refused as a tenant violation"
        );
        assert_eq!(
            expected.violation(&principal(
                expected.principal_id,
                service_kind(&card),
                DataTenantId::SYSTEM_OWNER,
                PermissionSet::new(),
            )),
            Some(BifrostSecurityViolationKind::PeerAudience),
            "the peer permission is required"
        );
        assert_eq!(
            expected.violation(&principal(
                PrincipalId::new(uuid::Uuid::from_u128(2)),
                service_kind(&card),
                DataTenantId::SYSTEM_OWNER,
                peer.clone(),
            )),
            Some(BifrostSecurityViolationKind::PeerAudience),
            "a different platform service holding the permission is still refused"
        );
        assert_eq!(
            expected.violation(&principal(
                expected.principal_id,
                service_kind(&service_card("bifrost-peer-alternate")),
                DataTenantId::SYSTEM_OWNER,
                peer.clone(),
            )),
            Some(BifrostSecurityViolationKind::PeerAudience),
            "the exact Service card is part of the identity"
        );
        assert_eq!(
            expected.violation(&principal(
                expected.principal_id,
                service_kind(&card),
                DataTenantId::SYSTEM_OWNER,
                peer,
            )),
            None,
            "the configured peer Service principal is admitted"
        );
    }
}
