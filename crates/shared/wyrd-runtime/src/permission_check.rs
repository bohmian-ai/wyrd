//! Runtime permission check contract.

use crate::permission::Permission;
use crate::principal::{Principal, PrincipalId};

/// Authorization decision contract.
///
/// This is synchronous on purpose: RBAC is a pure function over an in-memory
/// `PermissionSet`. DB-backed or network-backed policy checks belong behind a
/// separate contract.
pub trait PermissionCheck: Send + Sync + std::fmt::Debug {
    /// Check whether `principal` has `permission`.
    fn check(&self, principal: &Principal, permission: &Permission) -> PermissionVerdict;
}

/// Authorization verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionVerdict {
    /// The principal is allowed to perform the action.
    Allow,
    /// The principal is denied.
    Deny {
        /// Denial reason.
        reason: PermissionDenyReason,
    },
}

/// Why the check denied.
///
/// This enum intentionally has no ABAC/CEL variant. Attribute and policy
/// decisions belong to the Policy plane, not to the runtime RBAC check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionDenyReason {
    /// The principal's effective RBAC permissions did not cover the required
    /// permission.
    Rbac {
        /// Required permission.
        required: Permission,
        /// Principal that failed the check.
        principal: PrincipalId,
    },
}

impl PermissionVerdict {
    /// Convert a verdict to a result at a handler boundary.
    pub fn into_result(self) -> Result<(), PermissionDenyReason> {
        match self {
            Self::Allow => Ok(()),
            Self::Deny { reason } => Err(reason),
        }
    }
}

/// Pure RBAC permission checker.
///
/// Returns `Allow` when the principal's effective permission set covers the
/// required permission.
#[derive(Debug, Default, Clone, Copy)]
pub struct RbacCheck;

impl PermissionCheck for RbacCheck {
    fn check(&self, principal: &Principal, permission: &Permission) -> PermissionVerdict {
        if principal.effective_permissions.contains(permission) {
            PermissionVerdict::Allow
        } else {
            PermissionVerdict::Deny {
                reason: PermissionDenyReason::Rbac {
                    required: permission.clone(),
                    principal: principal.id,
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PermissionCheck, PermissionDenyReason, PermissionVerdict, RbacCheck};
    use crate::permission::{Permission, PermissionSet};
    use crate::principal::{Principal, PrincipalId, PrincipalKind, RoleRef};

    fn principal_with_permissions(permissions: impl IntoIterator<Item = Permission>) -> Principal {
        Principal {
            id: PrincipalId::new(uuid::Uuid::now_v7()),
            kind: PrincipalKind::User,
            tenant_id: wyrd_spec::DataTenantId::new_v7(),
            roles: vec![RoleRef::new("runtime_admin").expect("static role is valid")],
            effective_permissions: PermissionSet::from_iter(permissions),
        }
    }

    #[test]
    fn rbac_allows_match() {
        let principal = principal_with_permissions([Permission::card_write()]);

        let verdict = RbacCheck.check(&principal, &Permission::card_write());

        assert_eq!(verdict, PermissionVerdict::Allow);
    }

    #[test]
    fn rbac_denies_missing() {
        let principal = principal_with_permissions([]);

        let verdict = RbacCheck.check(&principal, &Permission::card_write());

        assert_eq!(
            verdict,
            PermissionVerdict::Deny {
                reason: PermissionDenyReason::Rbac {
                    required: Permission::card_write(),
                    principal: principal.id,
                },
            }
        );
    }

    #[test]
    fn rbac_resolves_through_wildcard() {
        let principal = principal_with_permissions([Permission::wildcard()]);

        assert_eq!(
            RbacCheck.check(&principal, &Permission::card_write()),
            PermissionVerdict::Allow
        );
        assert_eq!(
            RbacCheck.check(&principal, &Permission::delegation_issue()),
            PermissionVerdict::Allow
        );
    }

    #[test]
    fn rbac_allows_delegation_for_runtime_admin() {
        let principal = principal_with_permissions([Permission::delegation_issue()]);

        let verdict = RbacCheck.check(&principal, &Permission::delegation_issue());

        assert_eq!(verdict, PermissionVerdict::Allow);
    }

    #[test]
    fn verdict_into_result_round_trips() {
        assert_eq!(PermissionVerdict::Allow.into_result(), Ok(()));

        let reason = PermissionDenyReason::Rbac {
            required: Permission::card_write(),
            principal: PrincipalId::new(uuid::Uuid::now_v7()),
        };
        let verdict = PermissionVerdict::Deny {
            reason: reason.clone(),
        };

        assert_eq!(verdict.into_result(), Err(reason));
    }

    #[test]
    fn deny_reason_carries_required_and_principal() {
        let reason = PermissionDenyReason::Rbac {
            required: Permission::card_write(),
            principal: PrincipalId::new(uuid::Uuid::now_v7()),
        };

        let debug = format!("{reason:?}");

        assert!(debug.contains("required:"));
        assert!(debug.contains("principal:"));
    }
}
