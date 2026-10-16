//! Request-scoped runtime context.

use std::fmt;

use chrono::{DateTime, Utc};
use wyrd_spec::auth::Attribution;
use wyrd_spec::ids::IdempotencyKey;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::AuditDelegationStep;

use crate::principal::{Principal, PrincipalRef};

/// One flattened RFC 8693 actor-chain step.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DelegationStep {
    /// Self-describing delegating principal projection.
    pub principal: PrincipalRef,
}

impl DelegationStep {
    /// Projects one verified delegation hop into its durable audit wire form.
    ///
    /// The conversion lives here rather than in `wyrd-spec` because the
    /// runtime owns [`PrincipalRef`]; the spec crate stays free of runtime
    /// types. Only verified identity crosses: the principal's id, its
    /// card-free kind tag, and the card authority it acted under. No
    /// credential material is representable in the result.
    #[must_use]
    pub fn audit_projection(&self) -> AuditDelegationStep {
        AuditDelegationStep {
            principal_id: self.principal.id,
            principal_kind: self.principal.kind.tag(),
            card_ref: self.principal.card_ref().cloned(),
            card_ref_scope: self
                .principal
                .card_ref_scope()
                .map(|scope| scope.as_slice().to_vec())
                .unwrap_or_default(),
        }
    }
}

/// Projects an ordered delegation chain into its durable audit wire form.
///
/// Order is preserved exactly: the chain is initiator-first, and that order is
/// the delegation itself rather than an incidental encoding.
#[must_use]
pub fn audit_delegation_chain(chain: &[DelegationStep]) -> Vec<AuditDelegationStep> {
    chain.iter().map(DelegationStep::audit_projection).collect()
}

/// The subject/actor pair of a verified principal and its delegation chain.
///
/// The subject is always the verified top-level principal. The chain is
/// initiator-first, so its last step is the current (outermost `act`) actor;
/// with no chain the principal acts for itself. Older actors stay history in
/// audit detail and never become the current actor.
#[must_use]
pub fn delegated_attribution(principal: &Principal, chain: &[DelegationStep]) -> Attribution {
    Attribution::new(
        principal.id,
        chain.last().map_or(principal.id, |step| step.principal.id),
    )
}

/// The verified effective authority one accepted operation runs under.
///
/// Captured once when the server accepts deferred work, such as a queued
/// Verifier run, and stored with it so claim, retry, and restart reuse exactly
/// what was accepted: the effective principal (for a delegated request, the
/// subject/actor intersection authentication already produced) and the
/// initiator-first delegation chain kept as audit history. It is never
/// re-derived from a bare principal ID, holds no bearer token, API key, or
/// credential ID, and is server-only state that no run view exposes.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptedAuthority {
    /// Effective principal every protected downstream call is checked against.
    pub principal: Principal,
    /// Verified initiator-first delegation chain; empty for direct work.
    pub delegation_chain: Vec<DelegationStep>,
}

impl AcceptedAuthority {
    /// Capture `principal` and its verified `chain` as accepted authority.
    ///
    /// Drops the credential ID: which credential authenticated the accepting
    /// request is that request's audit fact, not part of the authority later
    /// work runs under.
    #[must_use]
    pub fn new(principal: &Principal, chain: &[DelegationStep]) -> Self {
        Self {
            principal: Principal {
                credential_id: None,
                ..principal.clone()
            },
            delegation_chain: chain.to_vec(),
        }
    }

    /// The subject/actor pair the accepting request carried.
    #[must_use]
    pub fn attribution(&self) -> Attribution {
        delegated_attribution(&self.principal, &self.delegation_chain)
    }
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
    use super::{AcceptedAuthority, DelegationStep, RequestContext, TraceParent};
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
                card_ref: Some(card_ref.clone()),
                card_ref_scope: CardRefScope::own(&card_ref),
            },
            tenant_id: wyrd_spec::DataTenantId::new_v7(),
            roles: Vec::new(),
            effective_permissions: PermissionSet::new(),
            credential_id: None,
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

    /// Accepted authority keeps the effective principal and chain, survives a
    /// JSON round trip unchanged, never carries the accepting credential, and
    /// projects the accepting request's subject/actor pair.
    ///
    /// # Panics
    /// Panics when the authority does not serialize or deserialize, or when
    /// the round trip changes it, keeps the credential, or projects another
    /// pair.
    #[test]
    fn accepted_authority_round_trips_without_credential() {
        let mut subject = service_principal("subject");
        subject.credential_id = Some(uuid::Uuid::now_v7());
        let actor = service_principal("actor");
        let chain = vec![DelegationStep {
            principal: PrincipalRef::from_principal(&actor),
        }];

        let accepted = AcceptedAuthority::new(&subject, &chain);
        let json = serde_json::to_value(&accepted).expect("serialize accepted authority");
        let parsed: AcceptedAuthority =
            serde_json::from_value(json).expect("deserialize accepted authority");

        assert_eq!(parsed, accepted);
        assert_eq!(parsed.principal.credential_id, None);
        assert_eq!(parsed.delegation_chain, chain);
        assert_eq!(parsed.attribution().sub_principal_id, subject.id);
        assert_eq!(parsed.attribution().act_principal_id, actor.id);
        assert_eq!(
            AcceptedAuthority::new(&subject, &[])
                .attribution()
                .act_principal_id,
            subject.id
        );
    }

    /// Accepted authority keeps a delegated exact tool grant: the effective
    /// subject/actor meet stays exactly the one tool after a JSON round trip,
    /// with subject and delegation history unchanged.
    ///
    /// # Panics
    /// Panics when the round trip widens, drops, or rewrites the tool scope,
    /// subject, or chain.
    #[test]
    fn accepted_authority_keeps_exact_tool_scope() {
        use crate::permission::Permission;
        use wyrd_spec::card::agent::BuiltinTool;
        let query = Permission::tool_invoke(BuiltinTool::BifrostQuery);
        let mut subject = service_principal("subject");
        subject.effective_permissions = [Permission::tool_invoke_any()].into_iter().collect();
        let actor_grants: PermissionSet = [query.clone()].into_iter().collect();
        subject.effective_permissions = subject
            .effective_permissions
            .intersection(&actor_grants)
            .expect("representable meet");
        let chain = vec![DelegationStep {
            principal: PrincipalRef::from_principal(&service_principal("actor")),
        }];

        let accepted = AcceptedAuthority::new(&subject, &chain);
        let json = serde_json::to_value(&accepted).expect("serialize accepted authority");
        let parsed: AcceptedAuthority =
            serde_json::from_value(json).expect("deserialize accepted authority");

        assert_eq!(parsed, accepted);
        assert_eq!(parsed.principal.id, subject.id);
        assert_eq!(parsed.delegation_chain, chain);
        let grants = &parsed.principal.effective_permissions;
        assert_eq!(grants.iter().collect::<Vec<_>>(), vec![&query]);
        assert!(!grants.contains(&Permission::tool_invoke(BuiltinTool::FsRead)));
        assert!(!grants.contains(&Permission::bifrost_query_read()));
    }
}
