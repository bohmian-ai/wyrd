//! Authz-check context derived from a verified delegated token.

use thiserror::Error;
use wyrd_auth_verify::VerifiedToken;
use wyrd_runtime::{Principal, PrincipalKind, PrincipalRef};
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;

use crate::request::{AuthzCheckRequest, AuthzCheckRequestMetadata};

/// Authz-check evaluation context in RFC 8693 subject/actor terms.
///
/// Built from a verified delegated token for `/v1/authz/check`, and directly
/// by the token exchange to ask whether an actor may act for a subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthzCheckContext {
    /// The party being acted for, carrying the token's attenuated authority.
    pub subject: Principal,
    /// The current actor: the Service or Agent performing the call.
    pub actor: PrincipalRef,
    /// Actor chain earliest first; its last entry is [`Self::actor`].
    pub chain: Vec<PrincipalRef>,
    /// Header-derived request metadata.
    pub request: AuthzCheckRequest,
    /// Mesh-projected request metadata, when present.
    pub metadata: Option<AuthzCheckRequestMetadata>,
    /// Request correlator for audit and response headers.
    pub request_id: RequestId,
}

/// Context construction failures.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AuthzCheckContextError {
    /// The current actor's kind is not Service or Agent.
    #[error("authz check requires a Service or Agent actor")]
    RequiresServiceOrAgentKind,
    /// The actor chain is empty (direct token).
    #[error("authz check requires a non-empty delegation chain (direct token not accepted)")]
    RequiresDelegationChain,
}

impl From<AuthzCheckContextError> for WyrdError {
    fn from(error: AuthzCheckContextError) -> Self {
        let condition = match &error {
            AuthzCheckContextError::RequiresServiceOrAgentKind => "requires_service_or_agent_kind",
            AuthzCheckContextError::RequiresDelegationChain => "requires_delegation_chain",
        };
        WyrdError::AuthzRequiresDelegatedToken {
            message: error.to_string(),
            details: serde_json::json!({ "condition": condition }),
        }
    }
}

impl AuthzCheckContext {
    /// Build an authz-check context from a verified delegated token.
    ///
    /// The token's principal is the subject; the last verified actor is the
    /// current actor, which must be a Service or Agent.
    ///
    /// # Errors
    /// Returns [`AuthzCheckContextError::RequiresDelegationChain`] for a
    /// direct token and [`AuthzCheckContextError::RequiresServiceOrAgentKind`]
    /// when the current actor is neither a Service nor an Agent.
    pub fn from_verified(
        verified: &VerifiedToken,
        request: AuthzCheckRequest,
        metadata: Option<AuthzCheckRequestMetadata>,
        request_id: RequestId,
    ) -> Result<Self, AuthzCheckContextError> {
        let chain: Vec<_> = verified
            .delegation_chain
            .iter()
            .map(|step| step.principal.clone())
            .collect();
        let Some(actor) = chain.last().cloned() else {
            return Err(AuthzCheckContextError::RequiresDelegationChain);
        };
        if !matches!(
            actor.kind,
            PrincipalKind::Service { .. } | PrincipalKind::Agent { .. }
        ) {
            return Err(AuthzCheckContextError::RequiresServiceOrAgentKind);
        }

        Ok(Self {
            subject: verified.principal.clone(),
            actor,
            chain,
            request,
            metadata,
            request_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use wyrd_auth_verify::VerifiedToken;
    use wyrd_runtime::{
        DelegationStep, PermissionSet, Principal, PrincipalId, PrincipalKind, PrincipalRef, RoleRef,
    };
    use wyrd_semver::VersionBlock;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::{CardRef, CardRefScope};
    use wyrd_spec::request_id::RequestId;

    use crate::context::{AuthzCheckContext, AuthzCheckContextError};
    use crate::request::AuthzCheckRequest;

    fn card_ref(kind: CardKind, name: &str) -> CardRef {
        CardRef {
            kind,
            name: CardName::new(name).expect("static card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: Some(SpaceName::new("prod").expect("static space is valid")),
            uid: None,
        }
    }

    fn principal(kind: PrincipalKind) -> Principal {
        Principal {
            id: PrincipalId::new(uuid::Uuid::now_v7()),
            kind,
            tenant_id: DataTenantId::new_v7(),
            roles: vec![RoleRef::new("service").expect("static role is valid")],
            effective_permissions: PermissionSet::new(),
            credential_id: None,
        }
    }

    /// Build a scoped service principal kind for tests.
    fn service_kind(name: &str) -> PrincipalKind {
        let card_ref = card_ref(CardKind::Service, name);
        PrincipalKind::Service {
            card_ref: Some(card_ref.clone()),
            card_ref_scope: CardRefScope::own(&card_ref),
        }
    }

    /// Build a scoped agent principal kind for tests.
    fn agent_kind(name: &str) -> PrincipalKind {
        let card_ref = card_ref(CardKind::Agent, name);
        PrincipalKind::Agent {
            card_ref: card_ref.clone(),
            card_ref_scope: CardRefScope::own(&card_ref),
        }
    }

    fn request() -> AuthzCheckRequest {
        AuthzCheckRequest {
            target: card_ref(CardKind::Service, "callee"),
            action: "card_write".to_owned(),
            context: serde_json::json!({}),
        }
    }

    fn request_id() -> RequestId {
        RequestId::parse(&uuid::Uuid::now_v7().to_string())
            .expect("generated UUIDv7 is a valid request id")
    }

    fn verified(principal: Principal, chain: Vec<Principal>) -> VerifiedToken {
        VerifiedToken {
            principal,
            delegation_chain: chain
                .into_iter()
                .map(|principal| DelegationStep {
                    principal: PrincipalRef::from_principal(&principal),
                })
                .collect(),
            exp: chrono::Utc::now(),
        }
    }

    /// The subject stays the context's principal and the last actor is the
    /// current actor, with the chain earliest first.
    ///
    /// # Panics
    /// Panics when the delegated token is refused or the context's subject,
    /// current actor, or chain order differs.
    #[test]
    fn delegated_token_builds_subject_and_current_actor_context() {
        let earlier = principal(service_kind("earlier-actor"));
        let actor = principal(service_kind("actor"));
        let subject = principal(service_kind("subject"));
        let verified = verified(subject.clone(), vec![earlier.clone(), actor.clone()]);

        let ctx = AuthzCheckContext::from_verified(&verified, request(), None, request_id())
            .expect("delegated token is accepted");

        assert_eq!(ctx.subject, subject);
        assert_eq!(ctx.actor, PrincipalRef::from_principal(&actor));
        assert_eq!(
            ctx.chain,
            vec![
                PrincipalRef::from_principal(&earlier),
                PrincipalRef::from_principal(&actor)
            ]
        );
    }

    /// A direct token has no actor and is refused.
    ///
    /// # Panics
    /// Panics when the context is not refused with
    /// `RequiresDelegationChain`.
    #[test]
    fn direct_token_is_rejected_with_delegation_chain_error() {
        let verified = verified(principal(service_kind("subject")), Vec::new());

        assert_eq!(
            AuthzCheckContext::from_verified(&verified, request(), None, request_id()),
            Err(AuthzCheckContextError::RequiresDelegationChain)
        );
    }

    /// A human actor is refused; only a Service or Agent may act.
    ///
    /// # Panics
    /// Panics when the context is not refused with
    /// `RequiresServiceOrAgentKind`.
    #[test]
    fn user_actor_is_rejected_with_kind_error() {
        let verified = verified(
            principal(service_kind("subject")),
            vec![principal(PrincipalKind::User)],
        );

        assert_eq!(
            AuthzCheckContext::from_verified(&verified, request(), None, request_id()),
            Err(AuthzCheckContextError::RequiresServiceOrAgentKind)
        );
    }

    /// A human subject is served by an Agent actor.
    ///
    /// # Panics
    /// Panics when the Agent actor is refused.
    #[test]
    fn agent_actor_for_user_subject_is_accepted() {
        let verified = verified(
            principal(PrincipalKind::User),
            vec![principal(agent_kind("agent"))],
        );

        assert!(AuthzCheckContext::from_verified(&verified, request(), None, request_id()).is_ok());
    }
}
