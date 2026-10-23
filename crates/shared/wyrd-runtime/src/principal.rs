//! Runtime principal identity model.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{CardAccess, PrincipalKindTag, WorkloadFamily};
use wyrd_spec::ids::CardUid;

use crate::permission::PermissionSet;

pub use wyrd_spec::auth::PrincipalId;

/// The authenticated identity behind a request.
///
/// Account identity (`id`, `kind`, `tenant_id`) is stable across releases.
/// Which exact Service or Agent version the request runs as is execution
/// context carried beside it in `workload_uid`, and which Cards it may
/// attribute evidence to is the signed `card_access` limit; neither changes
/// the account.
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
    /// The token's permission snapshot: resolved from the principal's roles
    /// once at issuance and carried unchanged until the token expires.
    pub effective_permissions: PermissionSet,
    /// Non-secret id of the credential the request authenticated with, when one
    /// was presented.
    ///
    /// Audit records it so a decision names which of a principal's several live
    /// credentials made it. `None` for a federated human session and for an
    /// identity the server minted for itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_id: Option<Uuid>,
    /// Effective limit on which Cards this request may attribute evidence to.
    ///
    /// Independent of `effective_permissions`: Card-attributed work needs both.
    pub card_access: CardAccess,
    /// The subject's exact Service or Agent root, when one was selected.
    ///
    /// Present exactly when the subject's own context is a validated root.
    /// For delegated work the owning root is the current actor's instead; see
    /// [`crate::request_context::owning_root`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workload_uid: Option<CardUid>,
}

/// Kind of authenticated account.
///
/// Every variant here is **tenant-scoped**: [`Principal`] carries a required
/// tenant, so this enum is the tenant-plane projection of a principal.
/// Platform-scope principals are a separate projection and are never
/// representable as a [`Principal`], which is what keeps the two control planes
/// from collapsing into one another by accident. The kind names the account
/// only; execution context lives on [`Principal::workload_uid`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalKind {
    /// Tenant administrative identity created during tenant provisioning.
    TenantAdmin,
    /// Human user identity.
    User,
    /// Stable machine Service account, independent of every Card version.
    Service,
    /// Internal credentialless verification-result writer.
    ///
    /// Exactly one exists per tenant. It holds no roles or credential and has
    /// no public lifecycle; the server mints it a short-lived token scoped to
    /// the single Verifier whose results it writes.
    System,
}

impl PrincipalKind {
    /// Projects to the wire discriminator.
    #[must_use]
    pub const fn tag(self) -> PrincipalKindTag {
        match self {
            Self::TenantAdmin => PrincipalKindTag::TenantAdmin,
            Self::User => PrincipalKindTag::User,
            Self::Service => PrincipalKindTag::Service,
            Self::System => PrincipalKindTag::System,
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
    /// Construct a rootless principal.
    ///
    /// The Card limit defaults to `any_registered` (the rootless User,
    /// administrator, and unrestricted Service state), except the SYSTEM
    /// writer, which starts with an empty scope. Context and credential are
    /// attached separately with [`Principal::with_card_access`] and
    /// [`Principal::with_credential_id`].
    #[must_use]
    pub fn new(
        id: PrincipalId,
        kind: PrincipalKind,
        tenant_id: DataTenantId,
        roles: Vec<RoleRef>,
        effective_permissions: PermissionSet,
    ) -> Self {
        let card_access = match kind {
            PrincipalKind::System => CardAccess::Scoped { cards: Vec::new() },
            PrincipalKind::TenantAdmin | PrincipalKind::User | PrincipalKind::Service => {
                CardAccess::AnyRegistered
            }
        };
        Self {
            id,
            kind,
            tenant_id,
            roles,
            effective_permissions,
            credential_id: None,
            card_access,
            workload_uid: None,
        }
    }

    /// Name the credential this principal authenticated with.
    #[must_use]
    pub fn with_credential_id(mut self, credential_id: Option<Uuid>) -> Self {
        self.credential_id = credential_id;
        self
    }

    /// Attach the signed Card limit and the subject's selected root.
    #[must_use]
    pub fn with_card_access(
        mut self,
        card_access: CardAccess,
        workload_uid: Option<CardUid>,
    ) -> Self {
        self.card_access = card_access;
        self.workload_uid = workload_uid;
        self
    }

    /// The exact signed Card UID scope, when access is `scoped`.
    #[must_use]
    pub fn card_scope(&self) -> Option<&[CardUid]> {
        match &self.card_access {
            CardAccess::Scoped { cards } => Some(cards),
            CardAccess::AnyRegistered | CardAccess::RootRequired { .. } => None,
        }
    }

    /// The Cards this principal may attribute observations and verifications to.
    #[must_use]
    pub fn card_attribution(&self) -> CardAttribution<'_> {
        match &self.card_access {
            CardAccess::Scoped { cards } => CardAttribution::Scoped(cards),
            CardAccess::AnyRegistered => CardAttribution::AnyRegistered,
            CardAccess::RootRequired { restriction } => {
                CardAttribution::RootRequired(restriction.family())
            }
        }
    }

    /// True when the signed scope names `card`.
    #[must_use]
    pub fn authorizes_card_uid(&self, card: &CardUid) -> bool {
        self.card_scope().is_some_and(|cards| cards.contains(card))
    }
}

/// Which Cards a principal may attribute observations and verifications to.
///
/// Returned by [`Principal::card_attribution`]. The distinction is explicit so
/// an unbound principal is never mistaken for one with an empty bounded scope,
/// and an unresolved family-restricted account is never mistaken for either.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardAttribution<'a> {
    /// Only the members of this signed Card UID scope.
    Scoped(&'a [CardUid]),
    /// Any registered observation-target Card in the principal's tenant.
    AnyRegistered,
    /// A family-restricted account with no validated root: no Card-attributed
    /// authority until a root of this family is selected.
    RootRequired(&'a WorkloadFamily),
}

/// Projection of one delegation participant.
///
/// Account identity plus the participant's own root and non-secret credential
/// provenance. Historical participants confer no authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrincipalRef {
    /// Stable principal id.
    pub id: PrincipalId,
    /// Principal kind.
    pub kind: PrincipalKind,
    /// The participant's exact root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workload_uid: Option<CardUid>,
    /// Non-secret id of the credential the participant authenticated with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_id: Option<Uuid>,
}

impl PrincipalRef {
    /// Project a full principal into a delegation-chain reference.
    #[must_use]
    pub fn from_principal(principal: &Principal) -> Self {
        Self {
            id: principal.id,
            kind: principal.kind,
            workload_uid: principal.workload_uid.clone(),
            credential_id: principal.credential_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CardAttribution, Principal, PrincipalId, PrincipalKind, PrincipalRef, RoleRef};
    use crate::permission::PermissionSet;
    use wyrd_spec::auth::{CardAccess, FamilyRestriction, WorkloadFamily, WorkloadKind};
    use wyrd_spec::ids::{CardName, CardUid};

    fn uid() -> CardUid {
        CardUid::from_uuid(uuid::Uuid::now_v7()).expect("v7 Card UID")
    }

    fn principal(kind: PrincipalKind) -> Principal {
        Principal::new(
            PrincipalId::new(uuid::Uuid::now_v7()),
            kind,
            wyrd_spec::DataTenantId::new_v7(),
            Vec::new(),
            PermissionSet::new(),
        )
    }

    #[test]
    fn role_ref_accepts_locked_name_shape() {
        let role = RoleRef::new("credential_admin").expect("role name is valid");

        assert_eq!(role.as_str(), "credential_admin");
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

    /// Rootless accounts attribute to any registered Card, SYSTEM starts
    /// empty, a scope authorizes exactly its UIDs, and an unresolved family
    /// account is neither.
    ///
    /// # Panics
    /// Panics when a state projects the wrong attribution.
    #[test]
    fn card_attribution_follows_the_signed_access_state() {
        for kind in [
            PrincipalKind::TenantAdmin,
            PrincipalKind::User,
            PrincipalKind::Service,
        ] {
            assert_eq!(
                principal(kind).card_attribution(),
                CardAttribution::AnyRegistered
            );
        }
        assert_eq!(
            principal(PrincipalKind::System).card_attribution(),
            CardAttribution::Scoped(&[])
        );
        let (root, member, outside) = (uid(), uid(), uid());
        let scoped = principal(PrincipalKind::Service).with_card_access(
            CardAccess::Scoped {
                cards: vec![root.clone(), member.clone()],
            },
            Some(root.clone()),
        );
        assert!(scoped.authorizes_card_uid(&member));
        assert!(!scoped.authorizes_card_uid(&outside));
        let projected = PrincipalRef::from_principal(&scoped);
        assert_eq!(projected.workload_uid, Some(root));
        let family = WorkloadFamily {
            kind: WorkloadKind::Service,
            name: CardName::new("checkout").expect("static name"),
        };
        let unresolved = principal(PrincipalKind::Service).with_card_access(
            CardAccess::RootRequired {
                restriction: FamilyRestriction::Family(family.clone()),
            },
            None,
        );
        assert_eq!(
            unresolved.card_attribution(),
            CardAttribution::RootRequired(&family)
        );
        assert!(!unresolved.authorizes_card_uid(&member));
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
