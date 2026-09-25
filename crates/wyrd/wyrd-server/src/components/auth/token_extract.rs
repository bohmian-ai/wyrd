//! Shared bearer-token extraction helpers for Wyrd auth handlers.

use std::sync::Arc;

use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderName};
use base64::Engine;
use secrecy::{ExposeSecret, SecretString};
use wyrd_auth_verify::AccessTokenClaims;
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;

use super::AuthenticatedPrincipal;
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
) -> Result<AuthenticatedPrincipal, WyrdErrorResponse> {
    verify_access_token(verifier, extract_wyrd_access_token(headers)?, surface)
}

/// Verifies an already extracted Wyrd access `token` and builds the
/// authenticated principal.
///
/// The token's unverified tenant claim selects the verification tenant; the
/// verifier then checks signature, expiry, and claims against it.
///
/// # Errors
/// Returns `BadTokenFormat` for a token that is not a compact Wyrd JWT,
/// `AuthVerifyUnavailable` when no verifier is configured, and the mapped
/// verification error.
pub(crate) fn verify_access_token(
    verifier: Option<&wyrd_auth_verify::TokenVerifier>,
    token: SecretString,
    surface: wyrd_auth_verify::TokenAudience,
) -> Result<AuthenticatedPrincipal, WyrdErrorResponse> {
    let expected_tenant = tenant_from_unverified_access_token(token.expose_secret())?;
    let verifier = verifier.ok_or_else(auth_not_configured)?;
    let verified = verifier
        .verify_on(&token, &expected_tenant, surface)
        .map_err(WyrdErrorResponse::from)?;
    Ok(AuthenticatedPrincipal::from_verified(Arc::new(verified)))
}

/// Header an Anthropic SDK carries its credential in.
pub(crate) const ANTHROPIC_API_KEY_HEADER: HeaderName = HeaderName::from_static("x-api-key");

/// Header a Google GenAI SDK carries its credential in.
pub(crate) const GOOGLE_API_KEY_HEADER: HeaderName = HeaderName::from_static("x-goog-api-key");

/// Every header a public gateway ingress request may carry a credential in.
const GATEWAY_CREDENTIAL_HEADERS: [HeaderName; 4] = [
    WYRD_ACCESS_TOKEN_HEADER,
    AUTHORIZATION,
    ANTHROPIC_API_KEY_HEADER,
    GOOGLE_API_KEY_HEADER,
];

/// Extracts the one Wyrd access token of a public gateway ingress request.
///
/// A token travels in `X-Wyrd-Access-Token` or `Authorization`, each as a
/// `Bearer` credential, or as the bare value of the route's `native` SDK
/// header, if any. Exactly one credential header value must be present: presenting
/// several, repeating one, or using a provider header the route does not
/// accept fails closed. Every query parameter must be named in `query_names`,
/// so a token placed in the URL is refused rather than ignored. The value is
/// always a Wyrd access token, never a provider key.
///
/// # Errors
/// Returns `BadTokenFormat` for a query parameter outside `query_names`,
/// several credential header values, a non-UTF-8, non-`Bearer`, or empty
/// value, and `Unauthenticated` when no accepted credential header is present.
pub(crate) fn extract_gateway_access_token(
    headers: &HeaderMap,
    native: Option<&HeaderName>,
    query: Option<&str>,
    query_names: &[&str],
) -> Result<SecretString, WyrdError> {
    let bad = |message: &str, header: &str| WyrdError::BadTokenFormat {
        message: message.to_owned(),
        details: serde_json::json!({ "header": header }),
    };
    let query = query.unwrap_or_default().as_bytes();
    if let Some((name, _)) =
        url::form_urlencoded::parse(query).find(|(name, _)| !query_names.contains(&name.as_ref()))
    {
        return Err(WyrdError::BadTokenFormat {
            message: "gateway ingress accepts credentials only in headers and refuses \
                      unexpected query parameters"
                .to_owned(),
            details: serde_json::json!({ "query_parameter": name }),
        });
    }
    let carriers = GATEWAY_CREDENTIAL_HEADERS;
    let mut presented = carriers
        .iter()
        .flat_map(|name| headers.get_all(name).iter().map(move |value| (name, value)));
    let Some((name, value)) = presented.next() else {
        return Err(WyrdError::Unauthenticated {
            message: "missing Wyrd access token".to_owned(),
            details: serde_json::json!({ "header": "authorization" }),
        });
    };
    if presented.next().is_some() {
        return Err(bad(
            "exactly one credential header must carry the Wyrd access token",
            name.as_str(),
        ));
    }
    let bearer = *name == WYRD_ACCESS_TOKEN_HEADER || *name == AUTHORIZATION;
    if !bearer && native != Some(name) {
        return Err(bad(
            "this gateway route does not accept that credential header",
            name.as_str(),
        ));
    }
    let raw = value
        .to_str()
        .map_err(|_| bad("credential header is not valid UTF-8", name.as_str()))?;
    let token = if bearer {
        raw.strip_prefix("Bearer ").ok_or_else(|| {
            bad(
                "credential header must be a Bearer credential",
                name.as_str(),
            )
        })?
    } else {
        raw
    };
    if token.is_empty() {
        return Err(bad("credential header carries no token", name.as_str()));
    }
    Ok(SecretString::from(token.to_owned()))
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

/// Credential-carrier cases of [`extract_gateway_access_token`]: each accepted
/// header alone, and the missing, repeated, mixed, non-bearer, empty,
/// foreign-native, and URL-borne credentials it refuses before verification.
#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderName, HeaderValue};
    use secrecy::ExposeSecret;
    use wyrd_spec::error::WyrdError;

    use super::{ANTHROPIC_API_KEY_HEADER, extract_gateway_access_token};

    /// Extracts from `headers` on an Anthropic route whose only accepted query
    /// parameter is `alt`.
    fn extract(
        headers: &[(&'static str, &'static str)],
        query: Option<&str>,
    ) -> Result<String, WyrdError> {
        let mut map = HeaderMap::new();
        for (name, value) in headers {
            map.append(
                HeaderName::from_static(name),
                HeaderValue::from_static(value),
            );
        }
        extract_gateway_access_token(&map, Some(&ANTHROPIC_API_KEY_HEADER), query, &["alt"])
            .map(|token| token.expose_secret().to_owned())
    }

    /// Exactly one carrier yields the token: a `Bearer` in
    /// `X-Wyrd-Access-Token` or `Authorization`, or the route's bare native
    /// header. Missing, repeated, mixed, non-bearer, foreign-native, empty, and
    /// URL-borne credentials are refused before verification.
    #[test]
    fn gateway_access_token_needs_exactly_one_accepted_header() {
        for headers in [
            [("x-wyrd-access-token", "Bearer jwt")],
            [("authorization", "Bearer jwt")],
            [("x-api-key", "jwt")],
        ] {
            assert_eq!(
                extract(&headers, Some("alt=sse")).ok().as_deref(),
                Some("jwt")
            );
        }
        assert!(matches!(
            extract(&[], None),
            Err(WyrdError::Unauthenticated { .. })
        ));
        for (headers, query) in [
            (
                &[("x-api-key", "jwt"), ("authorization", "Bearer jwt")][..],
                None,
            ),
            (&[("x-api-key", "jwt"), ("x-api-key", "jwt")][..], None),
            (&[("authorization", "jwt")][..], None),
            (&[("x-wyrd-access-token", "Bearer ")][..], None),
            (&[("x-goog-api-key", "jwt")][..], None),
            (&[("x-api-key", "jwt")][..], Some("key=jwt")),
        ] {
            assert!(
                matches!(
                    extract(headers, query),
                    Err(WyrdError::BadTokenFormat { .. })
                ),
                "{headers:?} {query:?}"
            );
        }
    }
}
