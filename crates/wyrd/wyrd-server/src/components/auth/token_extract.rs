//! Shared bearer-token extraction helpers for Wyrd auth handlers.

use std::sync::Arc;

use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderName};
use base64::Engine;
use secrecy::{ExposeSecret, SecretString};
use wyrd_auth_verify::AccessTokenClaims;
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;

use crate::http::error::WyrdErrorResponse;

pub(crate) const WYRD_ACCESS_TOKEN_HEADER: HeaderName =
    HeaderName::from_static("x-wyrd-access-token");

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

/// Read the tenant a token claims, before the token is verified, so the
/// verifier can be asked about the right tenant.
///
/// A structurally broken token is the caller's mistake and says so. A
/// well-formed JWT whose claims are not tenant access-token claims is a
/// different thing: the clearest case is a platform session, which travels on
/// this same header and names no tenant. That is not malformed input — it is a
/// credential that confers no tenant access — so it is refused as
/// unauthenticated, the same way the tenant plane refuses any token it will not
/// accept. Reporting it as a format error would both misdescribe it and tell a
/// caller which kind of token it holds.
///
/// # Errors
/// Returns [`WyrdError::BadTokenFormat`] when the value is not a compact JWT
/// with a base64 payload, and [`WyrdError::Unauthenticated`] when the payload
/// carries no tenant access-token claims.
pub(crate) fn tenant_from_unverified_access_token(
    token: &str,
) -> Result<DataTenantId, WyrdErrorResponse> {
    let payload = token.split('.').nth(1).ok_or_else(bad_token_format)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| bad_token_format())?;
    let claims: AccessTokenClaims = serde_json::from_slice(&bytes).map_err(|_| {
        WyrdErrorResponse::from(WyrdError::Unauthenticated {
            message: "token is not a tenant access token".to_owned(),
            details: serde_json::json!({ "plane": "tenant" }),
        })
    })?;
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

/// Run the full Wyrd bearer-token verification sequence and return an authenticated principal.
///
/// Performs five steps in order: (1) extract the bearer token from `X-Wyrd-Access-Token`,
/// (2) decode the unverified claims to derive the expected tenant, (3) require a configured
/// verifier — returning `503 WYRD_AUTH_503_VERIFY_UNAVAILABLE` with `retry_after_seconds: 1`
/// if none is set, (4) cryptographically verify the token against the tenant, and (5) build
/// the [`AuthenticatedPrincipal`](super::AuthenticatedPrincipal) from the verified claims.
///
/// `surface` is the audience of the route being served: every surface accepts
/// `wyrd`, and a Bifrost route additionally accepts a `bifrost`-audience
/// delegated token, so a token minted for Bifrost cannot be replayed against
/// any other Wyrd surface.
///
/// This is the single verify pipeline shared by the `/v1` default-deny
/// middlewares and `AuthenticatedPrincipal::from_request_parts` (the extractor fallback for
/// off-nest routes such as `/auth/issue-key`). Every call site produces identical
/// `400`/`401`/`503` error responses because they share this function.
///
/// # Errors
/// Returns `401` when the header is missing or verification fails, `400` when
/// the token is malformed, and `503` when no verifier is configured.
pub(crate) fn verify_authenticated_principal(
    verifier: Option<&wyrd_auth_verify::TokenVerifier>,
    headers: &HeaderMap,
    surface: wyrd_auth_verify::TokenAudience,
) -> Result<super::AuthenticatedPrincipal, WyrdErrorResponse> {
    let token = extract_wyrd_access_token(headers)?;
    let expected_tenant = tenant_from_unverified_access_token(token.expose_secret())?;
    let verifier = verifier.ok_or_else(auth_not_configured)?;
    let verified = verifier
        .verify_on(&token, &expected_tenant, surface)
        .map_err(WyrdErrorResponse::from)?;
    Ok(super::AuthenticatedPrincipal::from_verified(Arc::new(
        verified,
    )))
}

/// Read the per-request [`RequestId`] the request-id layer inserted.
///
/// Every authenticated extractor needs the correlator for audit and for the
/// response header, and every one of them fails the same way when the layer is
/// absent: the request reached a handler without passing through the layer that
/// mints it, which is a server wiring fault rather than a caller error. Owning
/// that read once is what keeps the tenant and platform extractors from
/// disagreeing about how it is reported.
///
/// # Errors
/// Returns [`WyrdError::Internal`] when the extension is missing.
pub(crate) fn request_id(parts: &Parts) -> Result<RequestId, WyrdErrorResponse> {
    parts.extensions.get::<RequestId>().cloned().ok_or_else(|| {
        WyrdErrorResponse::from(WyrdError::Internal {
            message: "missing RequestId extension".to_owned(),
            details: serde_json::json!({ "extension": "RequestId" }),
        })
    })
}
