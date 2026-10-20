//! `/auth/browser` — the browser-session HTTP boundary.
//!
//! A browser signs in to a tenant and keeps one opaque session per tenant in
//! a host-only HttpOnly cookie named `wyrd_session_<tenant route key>`. Rust
//! alone creates, resolves and revokes the session
//! ([`wyrd_auth::browser_sessions::BrowserSessions`]); these handlers only
//! move its cookie. Every cookie-only mutation requires the request's
//! `Origin` to equal the origin the request arrived at exactly; the provider
//! callback relies on its bound one-use login transaction instead. Every
//! response is `no-store`.

use axum::Json;
use axum::extract::{Extension, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use secrecy::{ExposeSecret as _, SecretString};
use url::Url;
use wyrd_auth::browser_sessions::{BROWSER_SESSION_TTL, BrowserSessions};
use wyrd_spec::auth::{
    BrowserApiKeyRequest, BrowserLogoutRequest, BrowserSessionQuery, BrowserSessionView,
    BrowserSessionsResponse,
};
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::ids::TenantSlug;
use wyrd_spec::request_id::RequestId;

use crate::auth::{RequestOrigin, auth_not_configured};
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Prefix of every tenant browser-session cookie name; the rest is the
/// tenant's route key.
pub(crate) const SESSION_COOKIE_PREFIX: &str = "wyrd_session_";

/// The browser-session owner over this server's issuer and human
/// connections.
///
/// # Errors
/// Returns `WYRD_SPEC_500_INTERNAL` when no signing key or human-connection
/// owner is configured.
pub(crate) fn browser_sessions(state: &AppState) -> Result<BrowserSessions, WyrdErrorResponse> {
    let issuer = state
        .auth
        .tenant_issuer(&state.scribe_outbox)
        .ok_or_else(auth_not_configured)?;
    let connections = state
        .auth
        .human_connections
        .clone()
        .ok_or_else(auth_not_configured)?;
    Ok(BrowserSessions::new(issuer, connections))
}

/// Every `wyrd_session_*` cookie a request presents, as (tenant route key
/// label, value) pairs in presentation order.
///
/// Cookies are split per RFC 6265 §5.4 across every `Cookie` header; a pair
/// whose label is not a valid route key is still returned, with no label, so
/// a caller counting sessions cannot be confused into ignoring it.
pub(crate) fn session_cookies(headers: &HeaderMap) -> Vec<(Option<TenantSlug>, String)> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .filter_map(|(name, value)| {
            let label = name.trim().strip_prefix(SESSION_COOKIE_PREFIX)?;
            Some((
                TenantSlug::new(label.to_owned()).ok(),
                value.trim().to_owned(),
            ))
        })
        .collect()
}

/// Refuse a cookie-authenticated mutation unless its `Origin` header equals
/// the request's own origin exactly.
///
/// # Errors
/// Returns `WYRD_AUTH_403_ORIGIN_REFUSED` when `Origin` is absent, not
/// UTF-8, `null`, or any other origin.
pub(crate) fn require_same_origin(headers: &HeaderMap, origin: &Url) -> Result<(), WyrdError> {
    let expected = origin.origin().ascii_serialization();
    match headers.get(header::ORIGIN).map(HeaderValue::to_str) {
        Some(Ok(presented)) if presented == expected => Ok(()),
        _ => Err(WyrdError::OriginRefused {
            message: "the request's Origin is not this Wyrd deployment's origin".to_owned(),
            details: serde_json::json!({}),
        }),
    }
}

/// `Set-Cookie` for a browser cookie on `origin`: host-only, HttpOnly,
/// SameSite=Lax on `path`, Secure except for plain HTTP to a loopback host
/// (the local development exception), living `max_age_seconds`.
///
/// # Panics
/// Never: every component is a header-safe ASCII token by construction.
pub(crate) fn cookie_header(
    origin: &Url,
    name: &str,
    value: &str,
    path: &str,
    max_age_seconds: u64,
) -> HeaderValue {
    let loopback_http = origin.scheme() == "http"
        && match origin.host() {
            Some(url::Host::Domain(host)) => host == "localhost",
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            None => false,
        };
    let secure = if loopback_http { "" } else { "; Secure" };
    HeaderValue::from_str(&format!(
        "{name}={value}; Path={path}; Max-Age={max_age_seconds}; HttpOnly; SameSite=Lax{secure}"
    ))
    .expect("invariant: cookie components are header-safe ASCII")
}

/// `Set-Cookie` that installs a tenant's new session cookie.
fn set_session_cookie(origin: &Url, tenant: &TenantSlug, cookie: &SecretString) -> HeaderValue {
    cookie_header(
        origin,
        &format!("{SESSION_COOKIE_PREFIX}{tenant}"),
        cookie.expose_secret(),
        "/",
        BROWSER_SESSION_TTL.as_secs(),
    )
}

/// `Set-Cookie` that removes the session cookie under `label`.
fn clear_session_cookie(origin: &Url, label: &str) -> HeaderValue {
    cookie_header(
        origin,
        &format!("{SESSION_COOKIE_PREFIX}{label}"),
        "",
        "/",
        0,
    )
}

/// A `no-store` response carrying `cookies` as separate `Set-Cookie` headers.
fn with_cookies(
    status: StatusCode,
    cookies: Vec<HeaderValue>,
    body: impl IntoResponse,
) -> Response {
    let mut response = crate::auth::oauth::no_store(status, body);
    for cookie in cookies {
        response.headers_mut().append(header::SET_COOKIE, cookie);
    }
    response
}

/// The request id string for audit correlation, minting one when the
/// request-id layer did not run.
fn request_id(request_id: Option<Extension<RequestId>>) -> RequestId {
    request_id.map_or_else(RequestId::now_v7, |Extension(id)| id)
}

/// `POST /auth/browser/api-key` — recover tenant access with an API key.
///
/// The key establishes its tenant; `tenant` must name it. On success a new
/// session is created for the key's principal with exactly the key's
/// authority, its cookie is set, and its metadata returned. The key is never
/// stored or echoed.
///
/// # Errors
/// `403 WYRD_AUTH_403_ORIGIN_REFUSED` without this deployment's exact
/// `Origin`; `401 WYRD_AUTH_401_API_KEY_INVALID` for every unusable key or a
/// mismatching tenant; `503 WYRD_AUTH_503_VERIFY_UNAVAILABLE` when the store
/// fails.
#[utoipa::path(
    post,
    path = "/auth/browser/api-key",
    request_body = BrowserApiKeyRequest,
    responses(
        (status = 200, description = "Session created; its cookie is set",
          body = BrowserSessionView,
          headers(("Set-Cookie" = String, description = "The tenant session cookie"))),
        (status = 401, description = "The key is unusable or belongs to another tenant \
          (WYRD_AUTH_401_API_KEY_INVALID)", body = WyrdProblem),
        (status = 403, description = "Origin absent or foreign \
          (WYRD_AUTH_403_ORIGIN_REFUSED)", body = WyrdProblem),
        (status = 503, description = "The store is unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    security(()),
    tag = "Auth"
)]
#[tracing::instrument(level = "debug", skip_all)]
pub async fn api_key(
    State(state): State<AppState>,
    RequestOrigin(origin): RequestOrigin,
    headers: HeaderMap,
    id: Option<Extension<RequestId>>,
    Json(request): Json<BrowserApiKeyRequest>,
) -> Result<Response, WyrdErrorResponse> {
    require_same_origin(&headers, &origin)?;
    let sessions = browser_sessions(&state)?;
    let api_key = request.api_key.into_secret_string();
    let created = sessions
        .recover(&request.tenant, &api_key, request_id(id).as_str())
        .await?;
    let view = sessions.view(&created.session).await?;
    Ok(with_cookies(
        StatusCode::OK,
        vec![set_session_cookie(&origin, &view.tenant, &created.cookie)],
        Json(view),
    ))
}

/// `GET /auth/browser/session` — the live sessions this browser presents.
///
/// Every `wyrd_session_*` cookie (only the selected tenant's with `tenant`)
/// is resolved by its value; one is live only when it resolves and names the
/// tenant its label does. Invalid, expired, revoked and mislabeled cookies
/// are cleared. Each live session is returned separately, never merged.
/// Resolution writes nothing and never extends a session.
///
/// # Errors
/// `401 WYRD_AUTH_401_INVALID_TOKEN` (clearing the dead cookies) when no
/// presented session is live; `503 WYRD_AUTH_503_VERIFY_UNAVAILABLE` when the
/// store fails, before any cookie is judged dead.
#[utoipa::path(
    get,
    path = "/auth/browser/session",
    params(BrowserSessionQuery),
    responses(
        (status = 200, description = "Live tenant sessions", body = BrowserSessionsResponse),
        (status = 401, description = "No presented session is live \
          (WYRD_AUTH_401_INVALID_TOKEN)", body = WyrdProblem),
        (status = 503, description = "The store is unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    security(()),
    tag = "Auth"
)]
#[tracing::instrument(level = "debug", skip_all)]
pub async fn session(
    State(state): State<AppState>,
    RequestOrigin(origin): RequestOrigin,
    headers: HeaderMap,
    Query(query): Query<BrowserSessionQuery>,
) -> Result<Response, WyrdErrorResponse> {
    let sessions = browser_sessions(&state)?;
    let mut live = Vec::new();
    let mut dead = Vec::new();
    for (label, value) in session_cookies(&headers) {
        if query.tenant.is_some() && label != query.tenant {
            continue;
        }
        match live_view(&sessions, label.as_ref(), &value).await? {
            Some(view) => live.push(view),
            None => dead.push(label),
        }
    }
    let cleared = dead
        .iter()
        .flatten()
        .map(|label| clear_session_cookie(&origin, label.as_str()))
        .collect();
    if live.is_empty() {
        let refused = WyrdErrorResponse::from(WyrdError::InvalidToken {
            message: "no live browser session; sign in again".to_owned(),
            details: serde_json::json!({ "reason": "browser_session_invalid" }),
        });
        return Ok(with_cookies(StatusCode::UNAUTHORIZED, cleared, refused));
    }
    Ok(with_cookies(
        StatusCode::OK,
        cleared,
        Json(BrowserSessionsResponse { sessions: live }),
    ))
}

/// The metadata of the session `value` names when it is live and belongs to
/// the tenant `label` names; `None` when it is dead.
///
/// # Errors
/// Returns store failures; every authority refusal is `None`.
async fn live_view(
    sessions: &BrowserSessions,
    label: Option<&TenantSlug>,
    value: &str,
) -> Result<Option<BrowserSessionView>, WyrdError> {
    let Some(label) = label else {
        return Ok(None);
    };
    let session = match sessions.resolve(value).await {
        Ok(session) => session,
        Err(error @ WyrdError::AuthVerifyUnavailable { .. }) => return Err(error),
        Err(_) => return Ok(None),
    };
    let view = match sessions.view(&session).await {
        Ok(view) => view,
        Err(error @ WyrdError::AuthVerifyUnavailable { .. }) => return Err(error),
        Err(_) => return Ok(None),
    };
    Ok((&view.tenant == label).then_some(view))
}

/// `POST /auth/browser/logout` — end this browser's session for one tenant.
///
/// Revokes the session the tenant's cookie names, immediately and only that
/// session (never its API key, other sessions or the provider session), and
/// clears the cookie. A missing or dead cookie revokes nothing and still
/// clears.
///
/// # Errors
/// `403 WYRD_AUTH_403_ORIGIN_REFUSED` without this deployment's exact
/// `Origin`; `503 WYRD_AUTH_503_VERIFY_UNAVAILABLE` when the store fails.
#[utoipa::path(
    post,
    path = "/auth/browser/logout",
    request_body = BrowserLogoutRequest,
    responses(
        (status = 204, description = "Session revoked and its cookie cleared",
          headers(("Set-Cookie" = String, description = "The cleared session cookie"))),
        (status = 403, description = "Origin absent or foreign \
          (WYRD_AUTH_403_ORIGIN_REFUSED)", body = WyrdProblem),
        (status = 503, description = "The store is unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    security(()),
    tag = "Auth"
)]
#[tracing::instrument(level = "debug", skip_all)]
pub async fn logout(
    State(state): State<AppState>,
    RequestOrigin(origin): RequestOrigin,
    headers: HeaderMap,
    Json(request): Json<BrowserLogoutRequest>,
) -> Result<Response, WyrdErrorResponse> {
    require_same_origin(&headers, &origin)?;
    let sessions = browser_sessions(&state)?;
    for (label, value) in session_cookies(&headers) {
        if label.as_ref() == Some(&request.tenant) {
            sessions.logout(&value).await?;
        }
    }
    Ok(with_cookies(
        StatusCode::NO_CONTENT,
        vec![clear_session_cookie(&origin, request.tenant.as_str())],
        (),
    ))
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue, header};
    use url::Url;

    use super::{cookie_header, require_same_origin, session_cookies};

    /// Session cookies are found across headers, labelled by route key, and
    /// other cookies are ignored.
    ///
    /// # Panics
    /// Panics when the parsed cookies differ.
    #[test]
    fn session_cookies_are_parsed_across_headers() {
        let mut headers = HeaderMap::new();
        headers.append(
            header::COOKIE,
            HeaderValue::from_static("theme=dark; wyrd_session_acme=a.1"),
        );
        headers.append(
            header::COOKIE,
            HeaderValue::from_static("wyrd_session_Bad!=b.2"),
        );
        let found = session_cookies(&headers);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].0.as_ref().map(|slug| slug.as_str()), Some("acme"));
        assert_eq!(found[0].1, "a.1");
        assert!(found[1].0.is_none());
    }

    /// Only the exact origin passes; absence, `null` and lookalikes refuse.
    ///
    /// # Panics
    /// Panics when an origin is judged wrongly.
    #[test]
    fn same_origin_is_exact() {
        let origin = Url::parse("https://wyrd.example").expect("origin parses");
        let with = |value: &'static str| {
            let mut headers = HeaderMap::new();
            headers.insert(header::ORIGIN, HeaderValue::from_static(value));
            headers
        };
        assert!(require_same_origin(&with("https://wyrd.example"), &origin).is_ok());
        for refused in ["null", "http://wyrd.example", "https://wyrd.example.evil"] {
            assert!(require_same_origin(&with(refused), &origin).is_err());
        }
        assert!(require_same_origin(&HeaderMap::new(), &origin).is_err());
    }

    /// Cookies are Secure except for plain HTTP to a loopback host.
    ///
    /// # Panics
    /// Panics when the Secure attribute is wrong.
    #[test]
    fn cookies_are_secure_except_on_loopback_http() {
        let header = |origin: &str| {
            let origin = Url::parse(origin).expect("origin parses");
            cookie_header(&origin, "n", "v", "/", 1)
                .to_str()
                .expect("ascii")
                .to_owned()
        };
        assert!(header("https://wyrd.example").ends_with("; Secure"));
        assert!(header("http://wyrd.example").ends_with("; Secure"));
        assert!(!header("http://localhost:8080").contains("Secure"));
        assert!(!header("http://127.0.0.1:8080").contains("Secure"));
        assert!(header("https://wyrd.example").contains("HttpOnly; SameSite=Lax"));
    }
}
