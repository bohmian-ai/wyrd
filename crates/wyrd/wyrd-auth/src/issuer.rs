//! Trusted issuer resolution for server-tier auth flows.

use wyrd_auth_oidc::{IssuerConfigResolver, TrustedIssuer};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::IssuerUrl;
use wyrd_spec::error::WyrdError;

/// Resolve the tenant's trusted issuer matching `issuer`, failing closed.
///
/// This is the shared decision used by the human login and OIDC callback paths.
/// The workload jwt-bearer path verifies externally and resolves workload
/// bindings directly.
pub async fn trusted_issuer<R>(
    resolver: Option<&R>,
    tenant_id: DataTenantId,
    issuer: &IssuerUrl,
) -> Result<TrustedIssuer, WyrdError>
where
    R: IssuerConfigResolver,
{
    let resolver = resolver.ok_or_else(|| WyrdError::Internal {
        message: "trusted issuer resolver is not configured".to_owned(),
        details: serde_json::json!({}),
    })?;
    let issuers = resolver.trusted_issuers(&tenant_id).await.map_err(|error| {
        tracing::warn!(
            error = %error,
            tenant_id = %tenant_id,
            "trusted issuer resolution failed"
        );
        WyrdError::AuthVerifyUnavailable {
            message: "trusted issuer resolution unavailable".to_owned(),
            details: serde_json::json!({ "retry_after_seconds": 1 }),
        }
    })?;
    issuers
        .into_iter()
        .find(|candidate| candidate.issuer == *issuer)
        .ok_or_else(|| WyrdError::InvalidToken {
            message: "issuer is not trusted for the resolved tenant".to_owned(),
            details: serde_json::json!({}),
        })
}
