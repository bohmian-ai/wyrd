//! Authz-check context derived from a verified delegated token.

use thiserror::Error;
use wyrd_auth_verify::VerifiedToken;
use wyrd_runtime::{Principal, PrincipalKind, PrincipalRef};
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;

use crate::request::{AuthzCheckRequest, AuthzCheckRequestMetadata};

/// Authz-check evaluation context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthzCheckContext {
    /// Principal the call is being made as.
    pub callee: Principal,
    /// Immediate delegator for this hop.
    pub caller: PrincipalRef,
    /// Initiator-first delegation chain. The callee is not included.
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
    /// The principal kind is not Service or Agent.
    #[error("authz check requires a Service or Agent principal kind")]
    RequiresServiceOrAgentKind,
    /// The delegation chain is empty (direct token).
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
    /// # Errors
    /// Returns a specific [`AuthzCheckContextError`] variant for each failure condition:
    /// wrong principal kind or empty delegation chain.
    pub fn from_verified(
        verified: &VerifiedToken,
        request: AuthzCheckRequest,
        metadata: Option<AuthzCheckRequestMetadata>,
        request_id: RequestId,
    ) -> Result<Self, AuthzCheckContextError> {
        if !matches!(
            verified.principal.kind,
            PrincipalKind::Service { .. } | PrincipalKind::Agent { .. }
        ) {
            return Err(AuthzCheckContextError::RequiresServiceOrAgentKind);
        }

        let callee = verified.principal.clone();
        let chain: Vec<_> = verified
            .delegation_chain
            .iter()
            .map(|step| step.principal.clone())
            .collect();
        let Some(caller) = chain.last().cloned() else {
            return Err(AuthzCheckContextError::RequiresDelegationChain);
        };

        Ok(Self {
            callee,
            caller,
            chain,
            request,
            metadata,
            request_id,
        })
    }
}

/// Return true when a verified token is eligible for `/v1/authz/check`.
#[must_use]
pub fn is_delegated_token(verified: &VerifiedToken) -> bool {
    matches!(
        verified.principal.kind,
        PrincipalKind::Service { .. } | PrincipalKind::Agent { .. }
    ) && !verified.delegation_chain.is_empty()
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

    use crate::context::{AuthzCheckContext, AuthzCheckContextError, is_delegated_token};
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
        }
    }

    /// Build a scoped service principal kind for tests.
    fn service_kind(name: &str) -> PrincipalKind {
        let card_ref = card_ref(CardKind::Service, name);
        PrincipalKind::Service {
            card_ref: card_ref.clone(),
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
            iat: chrono::Utc::now(),
        }
    }

    #[test]
    fn delegated_service_token_builds_initiator_first_context() {
        let initiator = principal(service_kind("initiator"));
        let immediate = principal(service_kind("caller"));
        let callee = principal(service_kind("callee"));
        let verified = verified(callee.clone(), vec![initiator.clone(), immediate.clone()]);

        let ctx = AuthzCheckContext::from_verified(&verified, request(), None, request_id())
            .expect("delegated token is accepted");

        assert_eq!(ctx.callee, callee);
        assert_eq!(ctx.caller, PrincipalRef::from_principal(&immediate));
        assert_eq!(
            ctx.chain,
            vec![
                PrincipalRef::from_principal(&initiator),
                PrincipalRef::from_principal(&immediate)
            ]
        );
    }

    #[test]
    fn direct_service_token_is_rejected_with_delegation_chain_error() {
        let callee = principal(service_kind("callee"));
        let verified = verified(callee, Vec::new());

        assert!(!is_delegated_token(&verified));
        assert_eq!(
            AuthzCheckContext::from_verified(&verified, request(), None, request_id()),
            Err(AuthzCheckContextError::RequiresDelegationChain)
        );
    }

    #[test]
    fn delegated_user_token_is_rejected_with_kind_error() {
        let user = principal(PrincipalKind::User);
        let initiator = principal(service_kind("initiator"));
        let verified = verified(user, vec![initiator]);

        assert!(!is_delegated_token(&verified));
        assert_eq!(
            AuthzCheckContext::from_verified(&verified, request(), None, request_id()),
            Err(AuthzCheckContextError::RequiresServiceOrAgentKind)
        );
    }

    #[test]
    fn delegated_agent_token_is_accepted() {
        let initiator = principal(service_kind("initiator"));
        let callee = principal(agent_kind("agent"));
        let verified = verified(callee, vec![initiator]);

        assert!(is_delegated_token(&verified));
    }
}
