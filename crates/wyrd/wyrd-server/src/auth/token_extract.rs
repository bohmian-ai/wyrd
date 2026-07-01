//! Shared bearer-token extraction helpers for Wyrd auth handlers.
//!
//! `WYRD_ACCESS_TOKEN_HEADER` and `tenant_from_unverified_access_token` are owned
//! by `wyrd-auth-verify` (so the gRPC ingest interceptor can reach them without
//! depending on `wyrd-server`); this module re-exports the header and wraps the
//! tenant helper to keep the HTTP handlers' `WyrdErrorResponse` contract intact.

use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;

pub(crate) use wyrd_auth_verify::WYRD_ACCESS_TOKEN_HEADER;

use crate::error::WyrdErrorResponse;

pub(crate) fn tenant_from_unverified_access_token(
    token: &str,
) -> Result<DataTenantId, WyrdErrorResponse> {
    wyrd_auth_verify::tenant_from_unverified_access_token(token).map_err(|_| bad_token_format())
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
