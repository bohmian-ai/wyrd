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
