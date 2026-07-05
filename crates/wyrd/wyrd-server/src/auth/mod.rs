//! Authentication extractors for Wyrd HTTP handlers.

pub mod callback;
pub(crate) mod card_scope;
pub mod exchange_api_key;
pub mod issue_api_key;
pub mod jwt_bearer;
pub mod login;
pub mod permission_resolver;
pub mod pg_resolvers;
pub mod refresh;
pub mod repo;
pub mod revocation_listener;
pub mod revocation_resolver;
pub mod revoke;
pub mod roles;
pub mod seed;

use crate::components::auth::AuthenticatedPrincipal;
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;
use wyrd_auth_oidc::{IssuerConfigResolver, TrustedIssuer};
use wyrd_runtime::{Permission, Principal};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::IssuerUrl;
use wyrd_spec::error::WyrdError;

/// Resolve the tenant's trusted issuer matching `issuer`, failing closed.
///
/// Single owner of the "is this issuer trusted for this tenant" decision used
/// by the human login and OIDC callback paths. The workload jwt-bearer path does
/// **not** call this fn — it drives its own `verify_external` call and then resolves
/// the workload binding directly via `WorkloadBindingResolver`; it never needs a
/// `TrustedIssuer` domain value. Consolidating the login/callback decision here
/// keeps the fail-closed error mapping from drifting across those entry points.
///
/// # Errors
/// - [`WyrdError::Internal`] when no issuer resolver is configured (server
///   misconfiguration).
/// - [`WyrdError::AuthVerifyUnavailable`] when the resolver cannot reach its
///   backing store (fail closed on outage, not a silent untrusted result).
/// - [`WyrdError::InvalidToken`] when the issuer is not trusted for this tenant
///   (untrusted or cross-tenant).
pub(crate) async fn trusted_issuer(
    state: &AppState,
    tenant_id: DataTenantId,
    issuer: &IssuerUrl,
) -> Result<TrustedIssuer, WyrdErrorResponse> {
    let resolver = state.auth.trusted_issuer_resolver.as_ref().ok_or_else(|| {
        WyrdErrorResponse::from(WyrdError::Internal {
            message: "trusted issuer resolver is not configured".to_owned(),
            details: serde_json::json!({}),
        })
    })?;
    let issuers = resolver
        .trusted_issuers(&tenant_id)
        .await
        .map_err(|error| {
            tracing::warn!(
                error = %error,
                tenant_id = %tenant_id,
                "trusted issuer resolution failed"
            );
            WyrdErrorResponse::from(WyrdError::AuthVerifyUnavailable {
                message: "trusted issuer resolution unavailable".to_owned(),
                details: serde_json::json!({ "retry_after_seconds": 1 }),
            })
        })?;
    issuers
        .into_iter()
        .find(|candidate| candidate.issuer == *issuer)
        .ok_or_else(|| {
            WyrdErrorResponse::from(WyrdError::InvalidToken {
                message: "issuer is not trusted for the resolved tenant".to_owned(),
                details: serde_json::json!({}),
            })
        })
}

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
