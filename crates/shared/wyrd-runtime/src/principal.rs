//! Runtime principal identity model.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PrincipalKindTag;
use wyrd_spec::reference::{CardRef, CardRefScope};

use crate::permission::PermissionSet;

pub use wyrd_spec::auth::PrincipalId;

/// The authenticated identity behind a request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Principal {
    /// Stable principal id.
    pub id: PrincipalId,
    /// Principal kind.
    pub kind: PrincipalKind,
    /// Tenant isolation key.
    pub tenant_id: DataTenantId,
    /// Role names carried by the verified token.
    pub roles: Vec<RoleRef>,
    /// Permissions resolved from roles at verify time.
    pub effective_permissions: PermissionSet,
    /// Non-secret id of the credential the request authenticated with, when one
    /// was presented.
    ///
    /// Audit records it so a decision names which of a principal's several live
    /// credentials made it. `None` for a federated human session and for an
    /// identity the server minted for itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_id: Option<Uuid>,
}

/// Kind of authenticated identity.
///
/// Every variant here is **tenant-scoped**: [`Principal`] carries a required
/// tenant, so this enum is the tenant-plane projection of a principal.
/// Platform-scope principals are a separate projection and are never
/// representable as a [`Principal`], which is what keeps the two control planes
/// from collapsing into one another by accident.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum PrincipalKind {
    /// Tenant administrative identity created during tenant provisioning.
    ///
    /// The tenant's headless root of trust. It binds no Card — an
    /// administrative identity is not a registered AI-system component — and it
    /// coexists with any human administrator the tenant later federates.
    TenantAdmin,
    /// Human user identity.
    User,
    /// Machine service identity, optionally bound to a Service card.
    ///
    /// A deployed workload carries its Service card and the emit scope derived
    /// from it. Tenant automation created by a tenant administrator carries no
    /// Card and therefore no emit scope, because Card binding is a property of
    /// a machine principal rather than a precondition for holding a credential.
    Service {
        /// Bound Service card, absent for a Card-free automation identity.
        card_ref: Option<CardRef>,
        /// Transitive card authorization set; contains `card_ref` when one is
        /// bound and is empty otherwise.
        card_ref_scope: CardRefScope,
    },
    /// Card-bound agent identity.
    Agent {
        /// Bound Agent card.
        card_ref: CardRef,
        /// Transitive card authorization set; always contains `card_ref`.
        card_ref_scope: CardRefScope,
    },
}

impl PrincipalKind {
    /// Projects to the card-free wire discriminator.
    #[must_use]
    pub fn tag(&self) -> PrincipalKindTag {
        match self {
            Self::TenantAdmin => PrincipalKindTag::TenantAdmin,
            Self::User => PrincipalKindTag::User,
            Self::Service { .. } => PrincipalKindTag::Service,
            Self::Agent { .. } => PrincipalKindTag::Agent,
        }
    }
}

/// Reference to a role by name.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RoleRef(String);

impl RoleRef {
    /// Build a validated role reference.
    ///
    /// # Errors
    /// Returns an error when the name does not match `^[a-z0-9_]{1,64}$`.
    pub fn new(value: &str) -> Result<Self, InvalidRoleName> {
        if value.is_empty()
            || value.len() > 64
            || !value
                .bytes()
                .all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'_'))
        {
            return Err(InvalidRoleName);
        }
        Ok(Self(value.to_owned()))
    }

    /// Borrow as a string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RoleRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for RoleRef {
    type Err = InvalidRoleName;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

/// Invalid role name.
#[derive(Debug, thiserror::Error)]
#[error("role name must match ^[a-z0-9_]{{1,64}}$")]
pub struct InvalidRoleName;

impl Principal {
    /// Construct a principal.
    ///
    /// The authenticating credential is attached separately with
    /// [`Principal::with_credential_id`], because most callers — internal
    /// service identities, federated humans, test fixtures — have no credential
    /// to name and would otherwise all pass `None`.
    #[must_use]
    pub fn new(
        id: PrincipalId,
        kind: PrincipalKind,
        tenant_id: DataTenantId,
        roles: Vec<RoleRef>,
        effective_permissions: PermissionSet,
    ) -> Self {
        Self {
            id,
            kind,
            tenant_id,
            roles,
            effective_permissions,
            credential_id: None,
        }
    }

    /// Name the credential this principal authenticated with.
    #[must_use]
    pub fn with_credential_id(mut self, credential_id: Option<Uuid>) -> Self {
        self.credential_id = credential_id;
        self
    }

    /// Returns the bound card ref, when this principal has one.
    ///
    /// An agent always binds a Card. A service binds one only when it is a
    /// deployed workload; Card-free tenant automation, tenant administrators,
    /// and humans return `None`.
    #[must_use]
    pub fn card_ref(&self) -> Option<&CardRef> {
        match &self.kind {
            PrincipalKind::Service { card_ref, .. } => card_ref.as_ref(),
            PrincipalKind::Agent { card_ref, .. } => Some(card_ref),
            PrincipalKind::TenantAdmin | PrincipalKind::User => None,
        }
    }

    /// Returns the card scope for card-carrying machine principals.
    ///
    /// A Card-free service still reports its (empty) scope, so callers can
    /// distinguish "no emit authority" from "not a machine principal".
    #[must_use]
    pub fn card_ref_scope(&self) -> Option<&CardRefScope> {
        match &self.kind {
            PrincipalKind::Service { card_ref_scope, .. }
            | PrincipalKind::Agent { card_ref_scope, .. } => Some(card_ref_scope),
            PrincipalKind::TenantAdmin | PrincipalKind::User => None,
        }
    }

    /// True when this principal is authorized to emit for `card`.
    #[must_use]
    pub fn authorizes_card(&self, card: &CardRef) -> bool {
        self.card_ref_scope()
            .is_some_and(|scope| scope.authorizes(card))
    }
}

/// Self-describing projection of a principal for request delegation chains.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrincipalRef {
    /// Stable principal id.
    pub id: PrincipalId,
    /// Principal kind.
    pub kind: PrincipalKind,
}

impl PrincipalRef {
    /// Project a full principal into a delegation-chain reference.
    #[must_use]
    pub fn from_principal(principal: &Principal) -> Self {
        Self {
            id: principal.id,
            kind: principal.kind.clone(),
        }
    }

    /// Returns the bound card ref, when this principal has one.
    ///
    /// Mirrors [`Principal::card_ref`] for the delegation-chain projection.
    #[must_use]
    pub fn card_ref(&self) -> Option<&CardRef> {
        match &self.kind {
            PrincipalKind::Service { card_ref, .. } => card_ref.as_ref(),
            PrincipalKind::Agent { card_ref, .. } => Some(card_ref),
            PrincipalKind::TenantAdmin | PrincipalKind::User => None,
        }
    }

    /// Returns the card scope for card-carrying machine principals.
    ///
    /// Mirrors [`Principal::card_ref_scope`] for the delegation-chain
    /// projection.
    #[must_use]
    pub fn card_ref_scope(&self) -> Option<&CardRefScope> {
        match &self.kind {
            PrincipalKind::Service { card_ref_scope, .. }
            | PrincipalKind::Agent { card_ref_scope, .. } => Some(card_ref_scope),
            PrincipalKind::TenantAdmin | PrincipalKind::User => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Principal, PrincipalId, PrincipalKind, PrincipalRef, RoleRef};
    use crate::permission::{Permission, PermissionSet};
    use wyrd_semver::VersionBlock;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::{CardRef, CardRefScope};

    fn service_card_ref() -> CardRef {
        CardRef {
            kind: CardKind::Service,
            name: CardName::new("billing").expect("static card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: Some(SpaceName::new("prod").expect("static space is valid")),
            uid: None,
        }
    }

    #[test]
    fn role_ref_accepts_locked_name_shape() {
        let role = RoleRef::new("runtime_admin").expect("role name is valid");

        assert_eq!(role.as_str(), "runtime_admin");
    }

    #[test]
    fn role_ref_rejects_punctuation_and_uppercase() {
        assert!(RoleRef::new("RuntimeAdmin").is_err());
        assert!(RoleRef::new("runtime-admin").is_err());
    }

    #[test]
    fn principal_id_roundtrips_as_uuid() {
        let uuid = uuid::Uuid::now_v7();
        let id = PrincipalId::new(uuid);

        assert_eq!(id.to_string(), uuid.to_string());
        assert_eq!(id.as_uuid(), uuid);
    }

    #[test]
    fn service_kind_carries_card_ref() {
        let card_ref = service_card_ref();
        let principal = Principal {
            id: PrincipalId::new(uuid::Uuid::now_v7()),
            kind: PrincipalKind::Service {
                card_ref: Some(card_ref.clone()),
                card_ref_scope: CardRefScope::own(&card_ref),
            },
            tenant_id: wyrd_spec::DataTenantId::new_v7(),
            roles: vec![RoleRef::new("agent").expect("static role is valid")],
            effective_permissions: PermissionSet::from_iter([Permission::card_read()]),
            credential_id: None,
        };

        assert_eq!(principal.card_ref(), Some(&card_ref));
    }

    #[test]
    fn principal_ref_keeps_card_ref_projection() {
        let card_ref = service_card_ref();
        let principal = Principal {
            id: PrincipalId::new(uuid::Uuid::now_v7()),
            kind: PrincipalKind::Service {
                card_ref: Some(card_ref.clone()),
                card_ref_scope: CardRefScope::own(&card_ref),
            },
            tenant_id: wyrd_spec::DataTenantId::new_v7(),
            roles: vec![RoleRef::new("agent").expect("static role is valid")],
            effective_permissions: PermissionSet::new(),
            credential_id: None,
        };

        let principal_ref = PrincipalRef::from_principal(&principal);

        assert_eq!(principal_ref.id, principal.id);
        assert_eq!(principal_ref.card_ref(), Some(&card_ref));
        assert_eq!(
            principal_ref.card_ref_scope(),
            Some(&CardRefScope::own(&card_ref))
        );
        assert_eq!(principal_ref.kind, principal.kind);
    }

    #[test]
    fn principal_authorizes_cards_from_scope() {
        let own = service_card_ref();
        let mut composed = service_card_ref();
        composed.name = CardName::new("composed").expect("static card name is valid");
        let principal = Principal {
            id: PrincipalId::new(uuid::Uuid::now_v7()),
            kind: PrincipalKind::Service {
                card_ref: Some(own.clone()),
                card_ref_scope: CardRefScope::from_root_and_members(&own, [composed.clone()]),
            },
            tenant_id: wyrd_spec::DataTenantId::new_v7(),
            roles: Vec::new(),
            effective_permissions: PermissionSet::new(),
            credential_id: None,
        };

        assert!(principal.authorizes_card(&own));
        assert!(principal.authorizes_card(&composed));

        let mut unrelated = service_card_ref();
        unrelated.name = CardName::new("unrelated").expect("static card name is valid");
        assert!(!principal.authorizes_card(&unrelated));
    }

    #[test]
    fn user_principal_authorizes_no_cards() {
        let principal = Principal {
            id: PrincipalId::new(uuid::Uuid::now_v7()),
            kind: PrincipalKind::User,
            tenant_id: wyrd_spec::DataTenantId::new_v7(),
            roles: Vec::new(),
            effective_permissions: PermissionSet::new(),
            credential_id: None,
        };

        assert!(!principal.authorizes_card(&service_card_ref()));
    }
}

/// Platform-scope authenticated identity.
///
/// Deliberately carries **no tenant**. A platform principal operates the
/// tenant directory — creating, inspecting, suspending, and recovering
/// administration for tenants — and is never implicitly authorized over the
/// resources inside one. Because the type has no tenant to offer, a handler
/// holding one cannot open a tenant-scoped connection at all; the control-plane
/// boundary is enforced by what is representable rather than by a check a
/// caller could forget.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformPrincipal {
    /// Stable principal id.
    pub id: PrincipalId,
    /// The kind the platform directory stores for this principal.
    ///
    /// `GlobalAdmin` for a machine root, `User` for a human registered for
    /// federated sign-in. Audit reads it, so a human's decision is never
    /// recorded as a machine's. It is read from the store at session
    /// verification, never inferred from what the principal may do.
    pub kind: PrincipalKindTag,
    /// Permissions resolved from the principal's platform grant.
    ///
    /// Absence of a grant is denial: a platform principal with an empty set
    /// authenticates but authorizes nothing.
    pub effective_permissions: PermissionSet,
    /// Non-secret id of the credential the session was minted from, when one
    /// was presented. `None` for a federated login.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_id: Option<Uuid>,
}

impl PlatformPrincipal {
    /// Construct a platform-scope principal.
    #[must_use]
    pub const fn new(
        id: PrincipalId,
        kind: PrincipalKindTag,
        effective_permissions: PermissionSet,
    ) -> Self {
        Self {
            id,
            kind,
            effective_permissions,
            credential_id: None,
        }
    }

    /// Name the credential this session was minted from.
    #[must_use]
    pub const fn with_credential_id(mut self, credential_id: Option<Uuid>) -> Self {
        self.credential_id = credential_id;
        self
    }
}

/// The authenticated identity behind a request, in exactly one control plane.
///
/// One authentication pipeline produces this, whichever entry path the caller
/// used — a machine credential exchanged for a token, or a human federated
/// login. Everything downstream authorizes against it and never branches on how
/// authentication happened.
///
/// The two variants are closed and carry different payloads, so a platform
/// identity is not representable where a tenant identity is required and the
/// reverse. That mirrors the database tier's `OperatorPool` / `TenantConn`
/// split at the identity tier.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "scope")]
pub enum AuthContext {
    /// Platform control plane: tenant lifecycle and administrative recovery.
    Platform(PlatformPrincipal),
    /// Tenant control plane: the resources of exactly one tenant.
    Tenant(Box<Principal>),
}

impl AuthContext {
    /// Borrow the platform principal, when this context is platform-scoped.
    #[must_use]
    pub fn platform(&self) -> Option<&PlatformPrincipal> {
        match self {
            Self::Platform(principal) => Some(principal),
            Self::Tenant(_) => None,
        }
    }

    /// Borrow the tenant principal, when this context is tenant-scoped.
    #[must_use]
    pub fn tenant(&self) -> Option<&Principal> {
        match self {
            Self::Tenant(principal) => Some(principal),
            Self::Platform(_) => None,
        }
    }

    /// The authenticated principal id, whichever plane this context names.
    ///
    /// Audit uses this so every privileged operation is attributable to a
    /// principal rather than to a credential.
    #[must_use]
    pub fn principal_id(&self) -> PrincipalId {
        match self {
            Self::Platform(principal) => principal.id,
            Self::Tenant(principal) => principal.id,
        }
    }

    /// The kind of principal behind the request, whichever plane it names.
    ///
    /// Audit records this, so it has to be what the store holds rather than a
    /// per-plane constant: a platform plane carrying both machine roots and
    /// registered humans would otherwise attribute every human decision to a
    /// machine.
    #[must_use]
    pub fn principal_kind(&self) -> PrincipalKindTag {
        match self {
            Self::Platform(principal) => principal.kind,
            Self::Tenant(principal) => principal.kind.tag(),
        }
    }

    /// The credential the request authenticated with, on either plane.
    ///
    /// Audit records it beside the principal, because a principal may hold
    /// several live credentials at once and the recorded decision has to say
    /// which one was used.
    #[must_use]
    pub const fn credential_id(&self) -> Option<Uuid> {
        match self {
            Self::Platform(principal) => principal.credential_id,
            Self::Tenant(principal) => principal.credential_id,
        }
    }

    /// The permissions authorization is decided against.
    ///
    /// One vocabulary and one checker serve both planes; only the set differs.
    #[must_use]
    pub fn effective_permissions(&self) -> &PermissionSet {
        match self {
            Self::Platform(principal) => &principal.effective_permissions,
            Self::Tenant(principal) => &principal.effective_permissions,
        }
    }

    /// The tenant this context is scoped to, when it is tenant-scoped.
    ///
    /// Returns `None` for a platform context because a platform principal has
    /// no tenant to return — not because one was withheld.
    #[must_use]
    pub fn tenant_id(&self) -> Option<DataTenantId> {
        match self {
            Self::Tenant(principal) => Some(principal.tenant_id),
            Self::Platform(_) => None,
        }
    }
}

impl From<Principal> for AuthContext {
    /// Lift a verified tenant principal into the shared context.
    ///
    /// Boxed because a tenant principal carries its roles and resolved
    /// permissions, and the platform variant carries far less; keeping the
    /// enum small matters where it is passed by value on every request.
    fn from(principal: Principal) -> Self {
        Self::Tenant(Box::new(principal))
    }
}

impl From<PlatformPrincipal> for AuthContext {
    /// Lift a verified platform principal into the shared context.
    ///
    /// The platform arm is the one that carries no tenant, which is what
    /// downstream tenancy checks read the context for.
    fn from(principal: PlatformPrincipal) -> Self {
        Self::Platform(principal)
    }
}

/// The plane an [`AuthContext`] reports, and the tenancy that follows from it.
#[cfg(test)]
mod auth_context_tests {
    use wyrd_spec::DataTenantId;

    use wyrd_spec::auth::PrincipalKindTag;

    use super::{AuthContext, PlatformPrincipal, Principal, PrincipalId, PrincipalKind};
    use crate::permission::{Permission, PermissionSet};

    /// A platform principal holding tenant-lifecycle authority.
    fn platform() -> PlatformPrincipal {
        PlatformPrincipal::new(
            PrincipalId::new(uuid::Uuid::now_v7()),
            PrincipalKindTag::GlobalAdmin,
            {
                let mut set = PermissionSet::new();
                set.insert(Permission::tenant_create());
                set
            },
        )
    }

    /// A tenant administrative principal in some tenant.
    fn tenant() -> Principal {
        Principal::new(
            PrincipalId::new(uuid::Uuid::now_v7()),
            PrincipalKind::TenantAdmin,
            DataTenantId::new_v7(),
            Vec::new(),
            {
                let mut set = PermissionSet::new();
                set.insert(Permission::card_read());
                set
            },
        )
    }

    /// A platform context has no tenant to offer, so no handler holding one can
    /// reach tenant-scoped data even by mistake.
    #[test]
    fn platform_context_carries_no_tenant() {
        let context = AuthContext::from(platform());

        assert_eq!(context.tenant_id(), None);
        assert!(context.tenant().is_none());
        assert!(context.platform().is_some());
    }

    /// A tenant context names exactly one tenant and never satisfies a
    /// platform-scoped accessor.
    #[test]
    fn tenant_context_carries_exactly_its_tenant() {
        let principal = tenant();
        let expected = principal.tenant_id;
        let context = AuthContext::from(principal);

        assert_eq!(context.tenant_id(), Some(expected));
        assert!(context.platform().is_none());
        assert!(context.tenant().is_some());
    }

    /// Authorization reads one permission set whichever plane produced the
    /// context, so the checker needs no plane-specific branch.
    #[test]
    fn permissions_resolve_from_whichever_plane_authenticated() {
        let platform_context = AuthContext::from(platform());
        let tenant_context = AuthContext::from(tenant());

        assert!(
            platform_context
                .effective_permissions()
                .contains(&Permission::tenant_create())
        );
        assert!(
            !tenant_context
                .effective_permissions()
                .contains(&Permission::tenant_create()),
            "a tenant principal never holds platform authority"
        );
        assert!(
            tenant_context
                .effective_permissions()
                .contains(&Permission::card_read())
        );
    }

    /// Every privileged operation is attributable to a principal, on either
    /// plane.
    #[test]
    fn principal_id_is_available_on_both_planes() {
        let principal = platform();
        let expected = principal.id;
        assert_eq!(AuthContext::from(principal).principal_id(), expected);

        let principal = tenant();
        let expected = principal.id;
        assert_eq!(AuthContext::from(principal).principal_id(), expected);
    }
}
