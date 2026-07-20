//! Policy hook seam for authz-check evaluation.

use async_trait::async_trait;
#[cfg(feature = "test-helpers")]
use std::sync::Mutex;
use wyrd_spec::card::policy::PolicyDecision;

use crate::context::AuthzCheckContext;

/// Pluggable policy hook used by the authz-check route.
#[async_trait]
pub trait PolicyHook: Send + Sync {
    /// Evaluate an authz-check context.
    async fn evaluate(&self, ctx: &AuthzCheckContext) -> PolicyDecision;

    /// Whether this hook is the placeholder default unsuitable for production.
    fn is_stub_default(&self) -> bool {
        false
    }
}

/// Pre-v1 policy hook that allows every context after mechanism-layer guards pass.
#[derive(Debug, Default, Clone, Copy)]
pub struct StubAllowPolicyHook;

#[async_trait]
impl PolicyHook for StubAllowPolicyHook {
    async fn evaluate(&self, _: &AuthzCheckContext) -> PolicyDecision {
        PolicyDecision::Allow
    }

    fn is_stub_default(&self) -> bool {
        true
    }
}

/// Test helper policy hook that records every observed context and allows it.
#[cfg(feature = "test-helpers")]
#[derive(Debug, Default)]
pub struct RecordingPolicyHook {
    captured: Mutex<Vec<AuthzCheckContext>>,
}

#[cfg(feature = "test-helpers")]
impl RecordingPolicyHook {
    /// Return every captured context in call order.
    #[must_use]
    pub fn calls(&self) -> Vec<AuthzCheckContext> {
        self.captured
            .lock()
            .expect("captured contexts mutex is unpoisoned")
            .clone()
    }

    /// Return the most recent captured context.
    #[must_use]
    pub fn last(&self) -> Option<AuthzCheckContext> {
        self.captured
            .lock()
            .expect("captured contexts mutex is unpoisoned")
            .last()
            .cloned()
    }
}

#[cfg(feature = "test-helpers")]
#[async_trait]
impl PolicyHook for RecordingPolicyHook {
    async fn evaluate(&self, ctx: &AuthzCheckContext) -> PolicyDecision {
        self.captured
            .lock()
            .expect("captured contexts mutex is unpoisoned")
            .push(ctx.clone());
        PolicyDecision::Allow
    }
}

/// Test helper policy hook that denies every context with a fixed reason.
#[cfg(feature = "test-helpers")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DenyAllPolicyHook {
    /// Denial reason returned to callers.
    pub reason: String,
}

#[cfg(feature = "test-helpers")]
#[async_trait]
impl PolicyHook for DenyAllPolicyHook {
    async fn evaluate(&self, _: &AuthzCheckContext) -> PolicyDecision {
        PolicyDecision::Deny {
            reason: self.reason.clone(),
        }
    }
}

#[cfg(all(test, feature = "test-helpers"))]
mod test_helpers {
    use wyrd_auth_verify::VerifiedToken;
    use wyrd_runtime::{
        DelegationStep, PermissionSet, Principal, PrincipalId, PrincipalKind, PrincipalRef,
    };
    use wyrd_semver::VersionBlock;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::card::policy::PolicyDecision;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::{CardRef, CardRefScope};
    use wyrd_spec::request_id::RequestId;

    use crate::context::AuthzCheckContext;
    use crate::hook::{DenyAllPolicyHook, PolicyHook, RecordingPolicyHook, StubAllowPolicyHook};
    use crate::request::AuthzCheckRequest;

    fn card_ref(name: &str) -> CardRef {
        CardRef {
            kind: CardKind::Service,
            name: CardName::new(name).expect("static card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: Some(SpaceName::new("prod").expect("static space is valid")),
            uid: None,
        }
    }

    fn principal(name: &str) -> Principal {
        let card_ref = card_ref(name);
        Principal {
            id: PrincipalId::new(uuid::Uuid::now_v7()),
            kind: PrincipalKind::Service {
                card_ref: card_ref.clone(),
                card_ref_scope: CardRefScope::own(&card_ref),
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
            iat: chrono::Utc::now(),
        };
        let request = AuthzCheckRequest {
            target: card_ref("callee"),
            action: "card_write".to_owned(),
            context: serde_json::json!({}),
        };
        let request_id = RequestId::parse(&uuid::Uuid::now_v7().to_string())
            .expect("generated UUIDv7 is a valid request id");

        AuthzCheckContext::from_verified(&verified, request, None, request_id)
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

    #[tokio::test]
    async fn recording_hook_captures_calls() {
        let hook = RecordingPolicyHook::default();
        let first = context();
        let second = context();

        assert_eq!(hook.evaluate(&first).await, PolicyDecision::Allow);
        assert_eq!(hook.evaluate(&second).await, PolicyDecision::Allow);

        assert_eq!(hook.calls(), vec![first, second.clone()]);
        assert_eq!(hook.last(), Some(second));
    }
}
