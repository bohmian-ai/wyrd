//! Runtime principal identity model.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
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
}

/// Kind of authenticated identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum PrincipalKind {
    /// Human user identity.
    User,
    /// Card-bound service identity.
    Service {
        /// Bound Service card.
        card_ref: CardRef,
        /// Transitive card authorization set; always contains `card_ref`.
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
        }
    }

    /// Returns the card ref for service and agent principals.
    #[must_use]
    pub fn card_ref(&self) -> Option<&CardRef> {
        match &self.kind {
            PrincipalKind::Service { card_ref, .. } | PrincipalKind::Agent { card_ref, .. } => {
                Some(card_ref)
            }
            PrincipalKind::User => None,
        }
    }

    /// Returns the card scope for service and agent principals.
    #[must_use]
    pub fn card_ref_scope(&self) -> Option<&CardRefScope> {
        match &self.kind {
            PrincipalKind::Service { card_ref_scope, .. }
            | PrincipalKind::Agent { card_ref_scope, .. } => Some(card_ref_scope),
            PrincipalKind::User => None,
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

    /// Returns the card ref for service and agent principals.
    #[must_use]
    pub fn card_ref(&self) -> Option<&CardRef> {
        match &self.kind {
            PrincipalKind::Service { card_ref, .. } | PrincipalKind::Agent { card_ref, .. } => {
                Some(card_ref)
            }
            PrincipalKind::User => None,
        }
    }

    /// Returns the card scope for service and agent principals.
    #[must_use]
    pub fn card_ref_scope(&self) -> Option<&CardRefScope> {
        match &self.kind {
            PrincipalKind::Service { card_ref_scope, .. }
            | PrincipalKind::Agent { card_ref_scope, .. } => Some(card_ref_scope),
            PrincipalKind::User => None,
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
                card_ref: card_ref.clone(),
                card_ref_scope: CardRefScope::own(&card_ref),
            },
            tenant_id: wyrd_spec::DataTenantId::new_v7(),
            roles: vec![RoleRef::new("agent").expect("static role is valid")],
            effective_permissions: PermissionSet::from_iter([Permission::card_read()]),
        };

        assert_eq!(principal.card_ref(), Some(&card_ref));
    }

    #[test]
    fn principal_ref_keeps_card_ref_projection() {
        let card_ref = service_card_ref();
        let principal = Principal {
            id: PrincipalId::new(uuid::Uuid::now_v7()),
            kind: PrincipalKind::Service {
                card_ref: card_ref.clone(),
                card_ref_scope: CardRefScope::own(&card_ref),
            },
            tenant_id: wyrd_spec::DataTenantId::new_v7(),
            roles: vec![RoleRef::new("agent").expect("static role is valid")],
            effective_permissions: PermissionSet::new(),
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
                card_ref: own.clone(),
                card_ref_scope: CardRefScope::from_root_and_members(&own, [composed.clone()]),
            },
            tenant_id: wyrd_spec::DataTenantId::new_v7(),
            roles: Vec::new(),
            effective_permissions: PermissionSet::new(),
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
        };

        assert!(!principal.authorizes_card(&service_card_ref()));
    }
}
