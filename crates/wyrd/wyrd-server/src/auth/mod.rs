//! Authentication extractors for Wyrd HTTP handlers.

pub mod authorize;
pub mod browser;
pub mod callback;
pub mod cli_login;
pub mod jwt_bearer;
pub mod oauth;
pub mod revoke;

pub(crate) use wyrd_auth::card_scope;
pub use wyrd_auth::seed;
pub(crate) use wyrd_auth::{exchange_api_key, issue_api_key, pg_resolvers, refresh, roles};

// --------------------------------------------------------------------------
// Shared host-header and error helpers used by multiple token-issuing paths.
// --------------------------------------------------------------------------

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::uri::Authority;
use axum::http::{HeaderMap, header};
use url::Url;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::TenantSlug;

use crate::http::error::WyrdErrorResponse;

/// The origin a sign-in request arrived at: the address the browser or CLI
/// itself used, which is where login redirects and callbacks must return.
///
/// The host is the first `X-Forwarded-Host` value a proxy in front sets —
/// the BFF forwards the browser's this way — else the request's `Host` (or
/// its HTTP/2 authority). The scheme is the first `X-Forwarded-Proto` value,
/// `https` only when it says so and `http` otherwise. The official image's
/// nginx keeps `Host` and forwards the scheme, so the deployment works at
/// whatever address it is reached on with nothing configured. A forged
/// `Host` only redirects its own sender, and an identity provider accepts
/// only the redirect URIs registered with it.
#[derive(Debug, Clone)]
pub struct RequestOrigin(
    /// The origin as a URL with an empty path.
    pub Url,
);

impl RequestOrigin {
    /// Read the origin from `headers`, falling back to `authority` when
    /// neither `X-Forwarded-Host` nor `Host` is present.
    ///
    /// Returns `None` when there is no host, or the host carries anything
    /// beyond a host and port (a path, user information, a query).
    pub(crate) fn from_parts(headers: &HeaderMap, authority: Option<&str>) -> Option<Url> {
        let first = |name| {
            headers
                .get(name)
                .map(|value| {
                    value
                        .to_str()
                        .ok()
                        .and_then(|value| value.split(',').next())
                })
                .map(|value| value.map(str::trim))
        };
        let host = match first("x-forwarded-host").or_else(|| first(header::HOST.as_str())) {
            Some(value) => value?,
            None => authority?,
        };
        let scheme = match first("x-forwarded-proto").flatten() {
            Some(proto) if proto.eq_ignore_ascii_case("https") => "https",
            _ => "http",
        };
        let url = Url::parse(&format!("{scheme}://{host}")).ok()?;
        (url.path() == "/"
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none())
        .then_some(url)
    }
}

impl<S: Send + Sync> FromRequestParts<S> for RequestOrigin {
    /// A request without a usable host is refused as problem JSON.
    type Rejection = WyrdErrorResponse;

    /// Derive the origin with [`RequestOrigin::from_parts`].
    ///
    /// # Errors
    /// Returns `WYRD_SPEC_400_VALIDATION` when the request names no usable
    /// host. Derivation completes before the returned future is polled.
    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let authority = parts.uri.authority().map(Authority::as_str);
        let origin = Self::from_parts(&parts.headers, authority)
            .map(Self)
            .ok_or_else(|| {
                WyrdErrorResponse::from(WyrdError::Validation {
                    message: "the request names no usable Host".to_owned(),
                    details: serde_json::json!({ "reason": "request_origin_invalid" }),
                })
            });
        std::future::ready(origin)
    }
}

/// Extract the tenant slug from the leftmost host label (e.g. `acme` in
/// `acme.wyrd.cloud`). Returns `None` for `localhost`-style hosts.
pub(crate) fn tenant_slug_from_host(headers: &HeaderMap) -> Option<TenantSlug> {
    let host = headers.get(header::HOST)?.to_str().ok()?;
    let host = host.split(':').next().unwrap_or(host);
    let mut segments = host.split('.').filter(|segment| !segment.is_empty());
    let first = segments.next()?;
    let second = segments.next()?;
    if first == "localhost" || second == "localhost" {
        return None;
    }
    TenantSlug::new(first.to_owned()).ok()
}

pub(crate) fn invalid_token(message: &str) -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::InvalidToken {
        message: message.to_owned(),
        details: serde_json::json!({}),
    })
}

pub(crate) fn auth_not_configured() -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::Internal {
        message: "auth signing or verification handle is not configured".to_owned(),
        details: serde_json::json!({}),
    })
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue, header};

    use super::RequestOrigin;

    /// Headers carrying `Host: host` and, when given, `X-Forwarded-Proto`.
    ///
    /// # Panics
    /// Panics when a value is not a valid header value.
    fn headers(host: &str, proto: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::HOST,
            HeaderValue::from_str(host).expect("host value"),
        );
        if let Some(proto) = proto {
            headers.insert(
                "x-forwarded-proto",
                HeaderValue::from_str(proto).expect("proto value"),
            );
        }
        headers
    }

    /// The origin is the request's host with the forwarded scheme, `http`
    /// without one, the authority without a `Host`, and nothing for a host
    /// that smuggles more than a host and port.
    ///
    /// # Panics
    /// Panics when an origin is derived differently.
    #[test]
    fn origin_is_the_host_with_the_forwarded_scheme() {
        let origin = |headers: &HeaderMap, authority| {
            RequestOrigin::from_parts(headers, authority)
                .map(|url| url.origin().ascii_serialization())
        };
        assert_eq!(
            origin(&headers("localhost:8080", None), None).as_deref(),
            Some("http://localhost:8080")
        );
        assert_eq!(
            origin(&headers("wyrd.example.com", Some("https")), None).as_deref(),
            Some("https://wyrd.example.com")
        );
        assert_eq!(
            origin(&headers("wyrd.example.com", Some("HTTPS, http")), None).as_deref(),
            Some("https://wyrd.example.com")
        );
        assert_eq!(
            origin(&headers("wyrd.example.com", Some("gopher")), None).as_deref(),
            Some("http://wyrd.example.com")
        );
        assert_eq!(
            origin(&HeaderMap::new(), Some("10.0.0.5:8080")).as_deref(),
            Some("http://10.0.0.5:8080")
        );
        let mut forwarded = headers("127.0.0.1:8080", Some("https"));
        forwarded.insert(
            "x-forwarded-host",
            HeaderValue::from_static("wyrd.example.com"),
        );
        assert_eq!(
            origin(&forwarded, None).as_deref(),
            Some("https://wyrd.example.com")
        );
        assert_eq!(origin(&HeaderMap::new(), None), None);
        for host in [
            "evil.example/path",
            "user@wyrd.example.com",
            "wyrd.example.com?x=1",
        ] {
            assert_eq!(origin(&headers(host, None), None), None, "{host}");
        }
    }
}
