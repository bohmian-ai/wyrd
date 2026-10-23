//! Request-scoped runtime context.

use std::fmt;

use chrono::{DateTime, Utc};
use wyrd_spec::auth::Attribution;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardUid, IdempotencyKey};
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
    /// types. Only verified identity crosses: the principal's id, its kind
    /// tag, and the exact root it acted under. No credential material is
    /// representable in the result.
    #[must_use]
    pub fn audit_projection(&self) -> AuditDelegationStep {
        AuditDelegationStep {
            principal_id: self.principal.id,
            principal_kind: self.principal.kind.tag(),
            workload_uid: self.principal.workload_uid.clone(),
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

/// The exact root that owns work done by `principal` under `chain`.
///
/// The current (outermost) actor's root when delegated, otherwise the
/// subject's own selected root; `None` for rootless work.
#[must_use]
pub fn owning_root<'a>(
    principal: &'a Principal,
    chain: &'a [DelegationStep],
) -> Option<&'a CardUid> {
    chain
        .last()
        .map_or(principal.workload_uid.as_ref(), |step| {
            step.principal.workload_uid.as_ref()
        })
}

/// Whether a request's `selected` workload root must still be resolved for a
/// verified bearer.
///
/// A bearer whose owning root ([`owning_root`]) is already signed is
/// immutable: an equal selector is an idempotent `Ok(false)` and a different
/// one is refused, never unpinned. A rootless bearer answers `Ok(true)`, and
/// the selector is resolved under its signed Card access. Every ingress
/// routes a selector through here, so none silently drops or rebinds one.
///
/// # Errors
/// Returns [`WyrdError::SignedWorkloadMismatch`] when `selected` differs from
/// the signed root.
pub fn needs_root_resolution(
    principal: &Principal,
    chain: &[DelegationStep],
    selected: &CardUid,
) -> Result<bool, WyrdError> {
    match owning_root(principal, chain) {
        Some(signed) if signed == selected => Ok(false),
        Some(signed) => Err(WyrdError::signed_workload_mismatch(signed, selected)),
        None => Ok(true),
    }
}

/// The verified effective authority one accepted operation runs under.
///
/// Captured once when the server accepts deferred work, such as a queued
/// Verifier run, and stored with it so claim, retry, and restart reuse exactly
/// what was accepted: the effective principal (for a delegated request, the
/// subject/actor intersection authentication already produced, with its
/// signed Card limit and root) and the initiator-first delegation chain with
/// each participant's root and non-secret credential provenance. It is never
/// re-derived from a bare principal ID or the account's current roles, holds
/// no bearer token or secret, and is server-only state that no run view
/// exposes.
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
    /// Keeps the non-secret credential ID as provenance; it never
    /// authorizes later work.
    #[must_use]
    pub fn new(principal: &Principal, chain: &[DelegationStep]) -> Self {
        Self {
            principal: principal.clone(),
            delegation_chain: chain.to_vec(),
        }
    }

    /// The exact root that owns the accepted work.
    #[must_use]
    pub fn owning_root(&self) -> Option<&CardUid> {
        owning_root(&self.principal, &self.delegation_chain)
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
    use super::{
        AcceptedAuthority, DelegationStep, RequestContext, TraceParent, needs_root_resolution,
    };
    use crate::permission::PermissionSet;
    use crate::principal::{Principal, PrincipalId, PrincipalKind, PrincipalRef};
    use chrono::Utc;
    use wyrd_spec::auth::CardAccess;
    use wyrd_spec::ids::{CardUid, IdempotencyKey};
    use wyrd_spec::request_id::RequestId;

    fn request_id() -> RequestId {
        RequestId::parse(&uuid::Uuid::now_v7().to_string())
            .expect("generated UUIDv7 is a valid request id")
    }

    fn service_principal(_name: &str) -> Principal {
        let root = CardUid::from_uuid(uuid::Uuid::now_v7()).expect("v7 Card UID");
        Principal::new(
            PrincipalId::new(uuid::Uuid::now_v7()),
            PrincipalKind::Service,
            wyrd_spec::DataTenantId::new_v7(),
            Vec::new(),
            PermissionSet::new(),
        )
        .with_card_access(
            CardAccess::Scoped {
                cards: vec![root.clone()],
            },
            Some(root),
        )
    }

    /// A signed root accepts only an equal selector; a rootless bearer
    /// resolves any selector.
    #[test]
    fn selector_never_rebinds_a_signed_root() {
        let rooted = service_principal("rooted");
        let signed = rooted.workload_uid.clone().expect("signed root");
        let other = CardUid::from_uuid(uuid::Uuid::now_v7()).expect("v7 Card UID");
        assert!(!needs_root_resolution(&rooted, &[], &signed).expect("equal selector"));
        assert!(matches!(
            needs_root_resolution(&rooted, &[], &other),
            Err(wyrd_spec::error::WyrdError::SignedWorkloadMismatch { .. })
        ));
        let rootless = Principal::new(
            PrincipalId::new(uuid::Uuid::now_v7()),
            PrincipalKind::User,
            wyrd_spec::DataTenantId::new_v7(),
            Vec::new(),
            PermissionSet::new(),
        );
        assert!(needs_root_resolution(&rootless, &[], &other).expect("rootless"));
        let delegated = [DelegationStep {
            principal: PrincipalRef::from_principal(&rooted),
        }];
        assert!(matches!(
            needs_root_resolution(&rootless, &delegated, &other),
            Err(wyrd_spec::error::WyrdError::SignedWorkloadMismatch { .. })
        ));
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
        assert_eq!(step.principal.workload_uid, principal.workload_uid);
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

    /// Accepted authority keeps the effective principal, Card limit, roots,
    /// chain, and credential provenance across a JSON round trip, and
    /// projects the accepting request's subject/actor pair and owning root.
    ///
    /// # Panics
    /// Panics when the authority does not serialize or deserialize, or when
    /// the round trip changes it or projects another pair or owner.
    #[test]
    fn accepted_authority_round_trips_with_provenance() {
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
        assert_eq!(parsed.principal.credential_id, subject.credential_id);
        assert_eq!(parsed.principal.card_access, subject.card_access);
        assert_eq!(parsed.delegation_chain, chain);
        assert_eq!(parsed.owning_root(), actor.workload_uid.as_ref());
        assert_eq!(
            AcceptedAuthority::new(&subject, &[]).owning_root(),
            subject.workload_uid.as_ref()
        );
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
