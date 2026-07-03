//! Shared bearer-token extraction helpers for Wyrd auth handlers.
//!
//! `WYRD_ACCESS_TOKEN_HEADER` and `tenant_from_unverified_access_token` are owned
//! by `wyrd-auth-verify` (so the gRPC ingest interceptor can reach them without
//! depending on `wyrd-server`); this module re-exports the header and wraps the
//! tenant helper to keep the HTTP handlers' `WyrdErrorResponse` contract intact.

use std::sync::Arc;

use axum::http::HeaderMap;
use secrecy::{ExposeSecret, SecretString};
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;

pub(crate) use wyrd_auth_verify::WYRD_ACCESS_TOKEN_HEADER;

use crate::error::WyrdErrorResponse;

pub(crate) fn extract_wyrd_access_token(
    headers: &HeaderMap,
) -> Result<SecretString, WyrdErrorResponse> {
    let raw = headers
        .get(WYRD_ACCESS_TOKEN_HEADER)
        .ok_or_else(|| {
            WyrdErrorResponse::from(WyrdError::Unauthenticated {
                message: "missing X-Wyrd-Access-Token header".to_owned(),
                details: serde_json::json!({ "header": "x-wyrd-access-token" }),
            })
        })?
        .to_str()
        .map_err(|_| {
            WyrdErrorResponse::from(WyrdError::BadTokenFormat {
                message: "X-Wyrd-Access-Token header is not valid UTF-8".to_owned(),
                details: serde_json::json!({ "header": "x-wyrd-access-token" }),
            })
        })?;
    let Some(token) = raw.strip_prefix("Bearer ") else {
        return Err(WyrdErrorResponse::from(WyrdError::BadTokenFormat {
            message: "X-Wyrd-Access-Token header must be a Bearer credential".to_owned(),
            details: serde_json::json!({ "header": "x-wyrd-access-token" }),
        }));
    };
    if token.is_empty() {
        return Err(WyrdErrorResponse::from(WyrdError::BadTokenFormat {
            message: "X-Wyrd-Access-Token bearer token is empty".to_owned(),
            details: serde_json::json!({ "header": "x-wyrd-access-token" }),
        }));
    }
    Ok(SecretString::from(token.to_owned()))
}

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

/// Run the full Wyrd bearer-token verification sequence and return an authenticated principal.
///
/// Performs five steps in order: (1) extract the bearer token from `X-Wyrd-Access-Token`,
/// (2) decode the unverified claims to derive the expected tenant, (3) require a configured
/// verifier — returning `503 WYRD_AUTH_503_VERIFY_UNAVAILABLE` with `retry_after_seconds: 1`
/// if none is set, (4) cryptographically verify the token against the tenant, and (5) build
/// the [`AuthenticatedPrincipal`](super::AuthenticatedPrincipal) from the verified claims.
///
/// This is the single verify pipeline shared by `require_authenticated` (the `/v1` default-deny
/// middleware) and `AuthenticatedPrincipal::from_request_parts` (the extractor fallback for
/// off-nest routes such as `/auth/issue-key`). Both call sites produce identical
/// `400`/`401`/`503` error responses because they share this function.
pub(crate) async fn verify_authenticated_principal(
    verifier: Option<Arc<crate::state::WyrdTokenVerifier>>,
    headers: &HeaderMap,
) -> Result<super::AuthenticatedPrincipal, WyrdErrorResponse> {
    let token = extract_wyrd_access_token(headers)?;
    let expected_tenant = tenant_from_unverified_access_token(token.expose_secret())?;
    let verifier = verifier.ok_or_else(auth_not_configured)?;
    let verified = verifier
        .verify(&token, &expected_tenant)
        .await
        .map_err(WyrdErrorResponse::from)?;
    Ok(super::AuthenticatedPrincipal {
        principal: verified.principal.clone(),
    })
}
