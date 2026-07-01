//! Authentication extractors for Wyrd HTTP handlers.

pub mod audit_writer;
pub mod callback;
pub mod caller_extractor;
pub mod card_scope;
pub mod exchange_api_key;
pub mod issue_api_key;
pub mod jwt_bearer;
pub mod login;
pub mod permission_resolver;
pub mod policy_hook;
pub mod principal_extractor;
pub mod refresh;
pub mod repo;
pub mod revocation_listener;
pub mod revocation_resolver;
pub mod revoke;
pub mod roles;
pub mod routes;
pub mod seed;
pub(crate) mod token_extract;

pub use caller_extractor::Caller;
pub use principal_extractor::AuthenticatedPrincipal;

use crate::error::WyrdErrorResponse;
use wyrd_runtime::{Permission, Principal};
use wyrd_spec::error::WyrdError;

/// Require `service_accounts:write` on `principal`.
///
/// Shared gate for the credential-administration routes (issue-key and principal
/// revoke) so the two write paths cannot drift. `action` names the attempted
/// operation for the RBAC denial message.
///
/// # Errors
/// Returns [`WyrdError::PermissionDeniedRbac`] when the principal lacks the
/// `service_accounts:write` permission.
pub(crate) fn require_service_accounts_write(
    principal: &Principal,
    action: &str,
) -> Result<(), WyrdErrorResponse> {
    if principal
        .effective_permissions
        .contains(&Permission::service_accounts_write())
    {
        return Ok(());
    }
    Err(WyrdErrorResponse::from(WyrdError::PermissionDeniedRbac {
        message: format!("service_accounts:write permission required to {action}"),
        details: serde_json::json!({ "required": "service_accounts:write" }),
    }))
}
