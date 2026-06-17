//! Policy hook seam for authz-check evaluation.

use async_trait::async_trait;
use wyrd_spec::card::policy::PolicyDecision;

use crate::context::AuthzCheckContext;

/// Pluggable policy hook used by the authz-check route.
#[async_trait]
pub trait PolicyHook: Send + Sync {
    /// Evaluate an authz-check context.
    async fn evaluate(&self, ctx: &AuthzCheckContext) -> PolicyDecision;
}

/// Pre-v1 policy hook that allows every context after mechanism-layer guards pass.
#[cfg(any(test, feature = "test-helpers"))]
#[derive(Debug, Default, Clone, Copy)]
pub struct StubAllowPolicyHook;

#[cfg(any(test, feature = "test-helpers"))]
#[async_trait]
impl PolicyHook for StubAllowPolicyHook {
    async fn evaluate(&self, _: &AuthzCheckContext) -> PolicyDecision {
        PolicyDecision::Allow
    }
}

/// Test helper policy hook that denies every context with a fixed reason.
#[cfg(any(test, feature = "test-helpers"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DenyAllPolicyHook {
    /// Denial reason returned to callers.
    pub reason: String,
}

#[cfg(any(test, feature = "test-helpers"))]
#[async_trait]
impl PolicyHook for DenyAllPolicyHook {
    async fn evaluate(&self, _: &AuthzCheckContext) -> PolicyDecision {
        PolicyDecision::Deny {
            reason: self.reason.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use wyrd_auth_verify::VerifiedToken;
    use wyrd_runtime::{
        DelegationStep, PermissionSet, Principal, PrincipalId, PrincipalKind, PrincipalRef,
    };
    use wyrd_spec::DataTenantId;
    use wyrd_spec::card::policy::PolicyDecision;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::version::VersionBlock;

    use crate::context::AuthzCheckContext;
    use crate::hook::{DenyAllPolicyHook, PolicyHook, StubAllowPolicyHook};
    use crate::request::AuthzCheckRequest;

    fn card_ref(name: &str) -> CardRef {
        CardRef {
            kind: CardKind::Service,
            name: CardName::new(name).expect("static card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: SpaceName::new("prod").expect("static space is valid"),
            uid: None,
        }
    }

    fn principal(name: &str) -> Principal {
        Principal {
            id: PrincipalId::new(uuid::Uuid::now_v7()),
            kind: PrincipalKind::Service {
                card_ref: card_ref(name),
            },
            tenant_id: DataTenantId::new_v7(),
            roles: Vec::new(),
            effective_permissions: PermissionSet::new(),
        }
    }

    fn context() -> AuthzCheckContext {
        let caller = principal("caller");
        let callee = principal("callee");
        let verified = VerifiedToken {
            principal: callee,
            delegation_chain: vec![DelegationStep {
                principal: PrincipalRef::from_principal(&caller),
            }],
            exp: chrono::Utc::now(),
        };
        let request = AuthzCheckRequest {
            method: "POST".to_owned(),
            path: "/v1/cards".to_owned(),
            host: "service.wyrd".to_owned(),
        };
        let request_id = RequestId::parse(&uuid::Uuid::now_v7().to_string())
            .expect("generated UUIDv7 is a valid request id");

        AuthzCheckContext::from_verified(&verified, request, request_id)
            .expect("test context is delegated")
    }

    #[tokio::test]
    async fn stub_allow_hook_allows_after_mechanism_guards() {
        let decision = StubAllowPolicyHook.evaluate(&context()).await;

        assert_eq!(decision, PolicyDecision::Allow);
    }

    #[tokio::test]
    async fn deny_all_hook_returns_fixed_reason() {
        let hook = DenyAllPolicyHook {
            reason: "test-deny".to_owned(),
        };

        let decision = hook.evaluate(&context()).await;

        assert_eq!(
            decision,
            PolicyDecision::Deny {
                reason: "test-deny".to_owned()
            }
        );
    }
}
