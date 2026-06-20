//! Shared bearer-token extraction helpers for Wyrd auth handlers.

use axum::http::HeaderName;
use base64::Engine;
use wyrd_auth_verify::AccessTokenClaims;
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;

use crate::error::WyrdErrorResponse;

pub(crate) const WYRD_ACCESS_TOKEN_HEADER: HeaderName =
    HeaderName::from_static("x-wyrd-access-token");

pub(crate) fn tenant_from_unverified_access_token(
    token: &str,
) -> Result<DataTenantId, WyrdErrorResponse> {
    let payload = token.split('.').nth(1).ok_or_else(bad_token_format)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| bad_token_format())?;
    let claims: AccessTokenClaims =
        serde_json::from_slice(&bytes).map_err(|_| bad_token_format())?;
    Ok(claims.principal.tenant_id)
}

pub(crate) fn bad_token_format() -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::BadTokenFormat {
        message: "X-Wyrd-Access-Token is not a compact Wyrd JWT".to_owned(),
        details: serde_json::json!({ "header": "x-wyrd-access-token" }),
    })
}

pub(crate) fn auth_not_configured() -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::AuthVerifyUnavailable {
        message: "auth backend not configured".to_owned(),
        details: serde_json::json!({ "retry_after_seconds": 1 }),
    })
}
