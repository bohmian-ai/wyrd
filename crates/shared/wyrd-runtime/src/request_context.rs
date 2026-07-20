//! Request-scoped runtime context.

use std::fmt;

use chrono::{DateTime, Utc};
use wyrd_spec::ids::IdempotencyKey;
use wyrd_spec::request_id::RequestId;

use crate::principal::{Principal, PrincipalRef};

/// One flattened RFC 8693 actor-chain step.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DelegationStep {
    /// Self-describing delegating principal projection.
    pub principal: PrincipalRef,
}

/// Per-request context carried through runtime boundaries.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct RequestContext {
    /// Request correlation id.
    pub request_id: RequestId,
    /// Authenticated principal once auth middleware has populated it.
    pub principal: Option<Principal>,
    /// Flattened delegation chain.
    pub delegation_chain: Vec<DelegationStep>,
    /// Optional W3C traceparent.
    pub traceparent: Option<TraceParent>,
    /// Optional idempotency key. Redacted from debug output.
    pub idempotency_key: Option<IdempotencyKey>,
    /// Request start time.
    pub started_at: DateTime<Utc>,
}

impl RequestContext {
    /// Construct an unauthenticated request context.
    #[must_use]
    pub fn new(request_id: RequestId, started_at: DateTime<Utc>) -> Self {
        Self {
            request_id,
            principal: None,
            delegation_chain: Vec::new(),
            traceparent: None,
            idempotency_key: None,
            started_at,
        }
    }

    /// Borrow the authenticated principal, if populated.
    #[must_use]
    pub fn principal(&self) -> Option<&Principal> {
        self.principal.as_ref()
    }

    /// True when this request was made under delegation.
    #[must_use]
    pub fn is_delegated(&self) -> bool {
        !self.delegation_chain.is_empty()
    }
}

impl fmt::Debug for RequestContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RequestContext")
            .field("request_id", &self.request_id)
            .field(
                "principal",
                &self.principal.as_ref().map(|principal| principal.id),
            )
            .field("delegation_chain_depth", &self.delegation_chain.len())
            .field("traceparent", &self.traceparent)
            .field(
                "idempotency_key",
                &self.idempotency_key.as_ref().map(|_| "<redacted>"),
            )
            .field("started_at", &self.started_at)
            .finish()
    }
}

/// W3C traceparent header value carried unchanged.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct TraceParent(String);

impl TraceParent {
    /// Build from an already accepted traceparent header value.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Borrow the underlying header value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::{DelegationStep, RequestContext, TraceParent};
    use crate::permission::PermissionSet;
    use crate::principal::{Principal, PrincipalId, PrincipalKind, PrincipalRef};
    use chrono::Utc;
    use wyrd_semver::VersionBlock;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, IdempotencyKey, SpaceName};
    use wyrd_spec::reference::{CardRef, CardRefScope};
    use wyrd_spec::request_id::RequestId;

    fn request_id() -> RequestId {
        RequestId::parse(&uuid::Uuid::now_v7().to_string())
            .expect("generated UUIDv7 is a valid request id")
    }

    fn card_ref(kind: CardKind, name: &str) -> CardRef {
        CardRef {
            kind,
            name: CardName::new(name).expect("static card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: Some(SpaceName::new("prod").expect("static space is valid")),
            uid: None,
        }
    }

    fn service_principal(name: &str) -> Principal {
        let card_ref = card_ref(CardKind::Service, name);
        Principal {
            id: PrincipalId::new(uuid::Uuid::now_v7()),
            kind: PrincipalKind::Service {
                card_ref: card_ref.clone(),
                card_ref_scope: CardRefScope::own(&card_ref),
            },
            tenant_id: wyrd_spec::DataTenantId::new_v7(),
            roles: Vec::new(),
            effective_permissions: PermissionSet::new(),
        }
    }

    #[test]
    fn delegation_chain_default_empty() {
        let context = RequestContext::new(request_id(), Utc::now());

        assert!(context.delegation_chain.is_empty());
        assert!(!context.is_delegated());
    }

    #[test]
    fn delegation_step_carries_self_describing_projection() {
        let principal = service_principal("billing");
        let step = DelegationStep {
            principal: PrincipalRef::from_principal(&principal),
        };

        assert_eq!(step.principal.id, principal.id);
        assert_eq!(step.principal.kind, principal.kind);
        assert!(step.principal.card_ref().is_some());
    }

    #[test]
    fn delegation_chain_round_trips() {
        let mut context = RequestContext::new(request_id(), Utc::now());
        context.principal = Some(service_principal("current"));
        context.traceparent = Some(TraceParent::new(
            "00-0123456789abcdef0123456789abcdef-0123456789abcdef-01",
        ));
        context.delegation_chain = vec![
            DelegationStep {
                principal: PrincipalRef::from_principal(&service_principal("first")),
            },
            DelegationStep {
                principal: PrincipalRef::from_principal(&service_principal("second")),
            },
        ];

        let json = serde_json::to_string(&context).expect("serialize request context");
        let parsed: RequestContext =
            serde_json::from_str(&json).expect("deserialize request context");

        assert_eq!(parsed.delegation_chain, context.delegation_chain);
        assert_eq!(parsed.traceparent, context.traceparent);
        assert!(parsed.is_delegated());
    }

    #[test]
    fn debug_redacts_idempotency_key() {
        let mut context = RequestContext::new(request_id(), Utc::now());
        context.idempotency_key =
            Some(IdempotencyKey::new("client-private-retry-key").expect("valid opaque key"));

        let debug = format!("{context:?}");

        assert!(debug.contains(r#"idempotency_key: Some("<redacted>")"#));
        assert!(!debug.contains("client-private-retry-key"));
    }

    #[test]
    fn debug_redacts_delegation_chain_to_depth() {
        let mut context = RequestContext::new(request_id(), Utc::now());
        context.delegation_chain = vec![
            DelegationStep {
                principal: PrincipalRef::from_principal(&service_principal("first")),
            },
            DelegationStep {
                principal: PrincipalRef::from_principal(&service_principal("second")),
            },
            DelegationStep {
                principal: PrincipalRef::from_principal(&service_principal("third")),
            },
        ];

        let debug = format!("{context:?}");

        assert!(debug.contains("delegation_chain_depth: 3"));
        assert!(!debug.contains("first"));
        assert!(!debug.contains("second"));
        assert!(!debug.contains("third"));
    }
}
