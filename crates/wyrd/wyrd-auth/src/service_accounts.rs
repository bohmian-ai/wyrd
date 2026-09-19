//! Shared permission gates for service-account administration flows.

use wyrd_runtime::{Permission, Principal};
use wyrd_spec::error::WyrdError;

/// Require `service_accounts:write` on `principal`.
///
/// Shared by credential-administration flows so API-key issuance, trusted-issuer
/// management, workload-binding management, and principal revocation cannot drift.
pub fn require_service_accounts_write(
    principal: &Principal,
    action: &str,
) -> Result<(), WyrdError> {
    if principal
        .effective_permissions
        .contains(&Permission::service_accounts_write())
    {
        return Ok(());
    }
    Err(WyrdError::PermissionDeniedRbac {
        message: format!("service_accounts:write permission required to {action}"),
        details: serde_json::json!({ "required": "service_accounts:write" }),
    })
}

#[cfg(test)]
mod tests {
    use wyrd_runtime::permission::PermissionSet;
    use wyrd_runtime::principal::{PrincipalId, PrincipalKind, RoleRef};
    use wyrd_runtime::{Permission, Principal};
    use wyrd_spec::error::WyrdError;

    use super::require_service_accounts_write;

    fn principal_with_permissions(permissions: impl IntoIterator<Item = Permission>) -> Principal {
        Principal {
            id: PrincipalId::new(uuid::Uuid::now_v7()),
            kind: PrincipalKind::User,
            tenant_id: wyrd_spec::DataTenantId::new_v7(),
            roles: vec![RoleRef::new("viewer").expect("static role is valid")],
            effective_permissions: PermissionSet::from_iter(permissions),
            credential_id: None,
        }
    }

    #[test]
    fn allows_principal_with_service_accounts_write() {
        let principal = principal_with_permissions([Permission::service_accounts_write()]);
        assert!(require_service_accounts_write(&principal, "manage issuers").is_ok());
    }

    #[test]
    fn denies_principal_without_permission() {
        let principal = principal_with_permissions([]);
        let result = require_service_accounts_write(&principal, "manage issuers");
        assert!(
            matches!(result, Err(WyrdError::PermissionDeniedRbac { .. })),
            "empty permission set should be denied, got: {result:?}"
        );
    }
}
