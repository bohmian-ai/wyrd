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
use wyrd_auth::browser_sessions::{BROWSER_SESSION_TTL, BrowserSessions, CreatedBrowserSession};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{
    BrowserApiKeyRequest, BrowserLoginRequest, BrowserLoginResponse, BrowserLogoutRequest,
    BrowserSessionQuery, BrowserSessionView, BrowserSessionsResponse,
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
    cookie_pairs(headers)
        .filter_map(|(name, value)| {
            let label = name.trim().strip_prefix(SESSION_COOKIE_PREFIX)?;
            Some((
                TenantSlug::new(label.to_owned()).ok(),
                value.trim().to_owned(),
            ))
        })
        .collect()
}

/// Every cookie (name, value) pair a request presents, split per RFC 6265
/// §5.4 across every `Cookie` header, in presentation order.
fn cookie_pairs(headers: &HeaderMap) -> impl Iterator<Item = (&str, &str)> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .map(|(name, value)| (name.trim(), value.trim()))
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

/// `POST /auth/browser/login` — begin a provider sign-in for one tenant.
///
/// Begins a login transaction ([`BrowserSessions::begin`]) and sets its
/// random binding in the `wyrd_login` cookie, scoped to `/auth/callback` and
/// living as long as the login state. The provider callback completes the
/// sign-in only for the browser presenting that binding. The response
/// carries the provider authorization URL the browser navigates to.
///
/// # Errors
/// `403 WYRD_AUTH_403_ORIGIN_REFUSED` without this deployment's exact
/// `Origin`; `401 WYRD_AUTH_401_INVALID_TOKEN` for an unknown tenant or one
/// without an Active connection, indistinguishably; `503
/// WYRD_AUTH_503_DISCOVERY_UNAVAILABLE` or `WYRD_AUTH_503_VERIFY_UNAVAILABLE`
/// when the provider or store is unavailable.
#[utoipa::path(
    post,
    path = "/auth/browser/login",
    request_body = BrowserLoginRequest,
    responses(
        (status = 200, description = "Sign-in begun; the login-transaction cookie is set",
          body = BrowserLoginResponse,
          headers(("Set-Cookie" = String, description = "The `wyrd_login` transaction cookie"))),
        (status = 401, description = "SSO sign-in is not available for this tenant \
          (WYRD_AUTH_401_INVALID_TOKEN)", body = WyrdProblem),
        (status = 403, description = "Origin absent or foreign \
          (WYRD_AUTH_403_ORIGIN_REFUSED)", body = WyrdProblem),
        (status = 503, description = "The identity provider or store is unavailable \
          (WYRD_AUTH_503_DISCOVERY_UNAVAILABLE, WYRD_AUTH_503_VERIFY_UNAVAILABLE)",
          body = WyrdProblem)
    ),
    security(()),
    tag = "Auth"
)]
#[tracing::instrument(level = "debug", skip_all)]
pub async fn login(
    State(state): State<AppState>,
    RequestOrigin(origin): RequestOrigin,
    headers: HeaderMap,
    Json(request): Json<BrowserLoginRequest>,
) -> Result<Response, WyrdErrorResponse> {
    require_same_origin(&headers, &origin)?;
    let begun = browser_sessions(&state)?
        .begin(&request.tenant, &origin)
        .await?;
    Ok(with_cookies(
        StatusCode::OK,
        vec![cookie_header(
            &origin,
            LOGIN_COOKIE,
            begun.binding.expose_secret(),
            LOGIN_COOKIE_PATH,
            LOGIN_COOKIE_MAX_AGE_SECONDS,
        )],
        Json(BrowserLoginResponse {
            authorization_url: begun.authorization_url,
        }),
    ))
}

/// Name of the cookie holding a browser sign-in's login-transaction binding.
const LOGIN_COOKIE: &str = "wyrd_login";

/// The only path the login-transaction cookie is sent to.
const LOGIN_COOKIE_PATH: &str = "/auth/callback";

/// Lifetime of the login-transaction cookie: the five-minute login state.
const LOGIN_COOKIE_MAX_AGE_SECONDS: u64 = 300;

/// The login-transaction binding a provider callback request presents, when
/// it presents exactly one `wyrd_login` cookie.
pub(crate) fn login_binding(headers: &HeaderMap) -> Option<String> {
    let mut bindings = cookie_pairs(headers).filter(|(name, _)| *name == LOGIN_COOKIE);
    match (bindings.next(), bindings.next()) {
        (Some((_, value)), None) => Some(value.to_owned()),
        _ => None,
    }
}

/// The callback response for a completed browser sign-in: the tenant's
/// session cookie is set, the login-transaction cookie cleared, and the
/// browser redirected (`303`) to the tenant home.
///
/// # Errors
/// Returns the refusals of [`BrowserSessions::view`].
pub(crate) async fn signed_in(
    state: &AppState,
    origin: &Url,
    created: &CreatedBrowserSession,
) -> Result<Response, WyrdErrorResponse> {
    let view = browser_sessions(state)?.view(&created.session).await?;
    Ok(see_other(
        &format!("/t/{}", view.tenant),
        vec![
            set_session_cookie(origin, &view.tenant, &created.cookie),
            clear_login_cookie(origin),
        ],
    ))
}

/// The callback response for a refused browser sign-in whose login state was
/// consumed: the login-transaction cookie is cleared and the browser
/// redirected (`303`) to the tenant's sign-in page with `login=failed`. The
/// refusal itself was audited by the exchange and is only logged here.
///
/// # Errors
/// Returns `error` itself when the tenant has left the directory, and
/// `WYRD_AUTH_503_VERIFY_UNAVAILABLE` when the directory read fails.
pub(crate) async fn login_refused(
    state: &AppState,
    origin: &Url,
    tenant_id: DataTenantId,
    error: WyrdError,
) -> Result<Response, WyrdErrorResponse> {
    tracing::info!(code = error.code(), "browser sign-in refused");
    let (tenant, _) = state
        .postgres
        .wyrd()
        .tenant_directory_entry(tenant_id)
        .await
        .map_err(|cause| {
            tracing::warn!(error = %cause, "tenant directory unavailable");
            WyrdError::AuthVerifyUnavailable {
                message: "auth backend unavailable".to_owned(),
                details: serde_json::json!({}),
            }
        })?
        .ok_or(error)?;
    Ok(see_other(
        &format!("/t/{tenant}/login?login=failed"),
        vec![clear_login_cookie(origin)],
    ))
}

/// `Set-Cookie` that removes the login-transaction cookie.
fn clear_login_cookie(origin: &Url) -> HeaderValue {
    cookie_header(origin, LOGIN_COOKIE, "", LOGIN_COOKIE_PATH, 0)
}

/// A `no-store` `303 See Other` to the same-origin `location`, carrying
/// `cookies`.
///
/// # Panics
/// Never: `location` is a path built from a route key, which is header-safe.
fn see_other(location: &str, cookies: Vec<HeaderValue>) -> Response {
    let mut response = with_cookies(StatusCode::SEE_OTHER, cookies, ());
    response.headers_mut().insert(
        header::LOCATION,
        HeaderValue::from_str(location).expect("invariant: route-key paths are header-safe"),
    );
    response
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
        assert_eq!(found[0].0.as_ref().map(wyrd_spec::TenantSlug::as_str), Some("acme"));
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

    /// The callback reads the login binding only from exactly one
    /// `wyrd_login` cookie; none or two bind nothing.
    ///
    /// # Panics
    /// Panics when a binding is read wrongly.
    #[test]
    fn the_login_binding_is_exactly_one_cookie() {
        let with = |values: &[&'static str]| {
            let mut headers = HeaderMap::new();
            for value in values {
                headers.append(header::COOKIE, HeaderValue::from_static(value));
            }
            headers
        };
        assert_eq!(
            super::login_binding(&with(&["theme=dark; wyrd_login=abc"])).as_deref(),
            Some("abc")
        );
        assert!(super::login_binding(&with(&["theme=dark"])).is_none());
        assert!(super::login_binding(&with(&["wyrd_login=a", "wyrd_login=b"])).is_none());
    }
}

/// Browser sign-in initiation against a mock provider, proving the callback
/// and login-transaction cookie come from the request origin and that a
/// foreign origin or an unconfigured tenant begins nothing.
#[cfg(test)]
mod pg_tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{Request, StatusCode, header};
    use axum::response::Response;
    use secrecy::SecretString;
    use tower::ServiceExt;
    use url::Url;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wyrd_auth::connections::HumanConnections;
    use wyrd_auth_issue::IssuingKey;
    use wyrd_auth_oidc::ScreenedHttp;
    use wyrd_auth_verify::Kid;
    use wyrd_dev_fixtures::pg::{PgFixture, seed_active_human_connection};
    use wyrd_spec::auth::{BrowserLoginResponse, Sha256Hex};
    use wyrd_storage::{LocalSigner, StorageHandle};

    use crate::components::auth::ServerAuth;
    use crate::state::AppState;

    /// The Ed25519 test issuing key the login state needs to build the
    /// browser-session owner.
    const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";

    /// App state over `fixture` whose tenant's Active connection points at a
    /// mock provider.
    ///
    /// # Panics
    /// Panics when the fixture cannot be seeded or the state cannot be built.
    async fn login_state(fixture: &PgFixture, provider: &MockServer) -> AppState {
        let issuer = provider.uri();
        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issuer": issuer,
                "authorization_endpoint": format!("{issuer}/authorize"),
                "token_endpoint": format!("{issuer}/token"),
                "jwks_uri": format!("{issuer}/jwks"),
                "response_types_supported": ["code"],
                "subject_types_supported": ["public"],
                "id_token_signing_alg_values_supported": ["EdDSA"],
            })))
            .mount(provider)
            .await;
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "keys": [] })),
            )
            .mount(provider)
            .await;
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let binding = seed_active_human_connection(&mut conn)
            .await
            .expect("connection seeds");
        conn.commit().await.expect("seed commits");
        sqlx::query(
            "UPDATE wyrd.auth_human_connections SET issuer_url = $1 WHERE connection_id = $2",
        )
        .bind(&issuer)
        .bind(binding.connection_id)
        .execute(&fixture.superuser_pool().expect("superuser pool opens"))
        .await
        .expect("connection points at the mock provider");
        let tempdir = tempfile::tempdir().expect("login storage tempdir");
        let signer = LocalSigner::new(tempdir.keep()).expect("local signer creates");
        let state = crate::test_support::test_app_state(
            Arc::new(crate::postgres::ServerPostgres::from_parts(
                fixture.wyrd_postgres().clone(),
                fixture.vala_postgres().clone(),
            )),
            Arc::new(StorageHandle::new(signer)),
            crate::test_support::test_catalog().await,
        );
        let audit = Arc::clone(&state.scribe_outbox);
        let issuing_key = IssuingKey::from_ed_pem(
            SecretString::from(PRIVATE_KEY_PEM.to_owned()),
            Kid::new("k1").expect("kid is valid"),
            "wyrd",
        )
        .expect("issuing key parses");
        state.with_auth(ServerAuth {
            issuing_key: Some(Arc::new(issuing_key)),
            human_connections: Some(HumanConnections::new(
                fixture.wyrd_postgres().clone(),
                None,
                ScreenedHttp::allowing_internal(),
                audit,
            )),
            ..ServerAuth::default()
        })
    }

    /// POST `/auth/browser/login` for `tenant` arriving at
    /// `https://wyrd.example.com` through a TLS proxy with `Origin: origin`,
    /// through the handler alone.
    ///
    /// # Panics
    /// Panics when the request cannot be built or answered.
    async fn begin(state: AppState, tenant: &str, origin: &str) -> Response {
        axum::Router::new()
            .route("/auth/browser/login", axum::routing::post(super::login))
            .with_state(state)
            .oneshot(
                Request::post("/auth/browser/login")
                    .header(header::HOST, "wyrd.example.com")
                    .header("x-forwarded-proto", "https")
                    .header(header::ORIGIN, origin)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::json!({ "tenant": tenant }).to_string(),
                    ))
                    .expect("request builds"),
            )
            .await
            .expect("router answers")
    }

    /// Number of login-state rows in the fixture tenant.
    ///
    /// # Panics
    /// Panics when the count query fails.
    async fn state_rows(fixture: &PgFixture) -> i64 {
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        sqlx::query_scalar("SELECT COUNT(*) FROM wyrd.auth_login_state")
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("state count runs")
    }

    /// A same-origin sign-in returns the tenant's provider URL with the
    /// callback on the request origin, sets the login-transaction cookie
    /// scoped to the callback for the login state's lifetime, and stores only
    /// that binding's digest.
    ///
    /// # Panics
    /// Panics when the sign-in is refused or its URL, cookie, or stored
    /// binding differ.
    #[tokio::test]
    async fn login_derives_the_callback_and_binds_the_browser() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let provider = MockServer::start().await;
        let state = login_state(&fixture, &provider).await;

        let response = begin(state, fixture.tenant_slug(), "https://wyrd.example.com").await;

        let (parts, body) = response.into_parts();
        let body = axum::body::to_bytes(body, usize::MAX)
            .await
            .expect("body reads");
        assert_eq!(
            parts.status,
            StatusCode::OK,
            "{}",
            String::from_utf8_lossy(&body)
        );
        let cookie = parts
            .headers
            .get(header::SET_COOKIE)
            .expect("the login cookie is set")
            .to_str()
            .expect("ascii")
            .to_owned();
        let (pair, attributes) = cookie.split_once("; ").expect("cookie has attributes");
        let binding = pair.strip_prefix("wyrd_login=").expect("login cookie");
        assert_eq!(
            attributes,
            "Path=/auth/callback; Max-Age=300; HttpOnly; SameSite=Lax; Secure"
        );
        let begun: BrowserLoginResponse = serde_json::from_slice(&body).expect("body decodes");
        let location = Url::parse(begun.authorization_url.as_str()).expect("url parses");
        assert!(location.as_str().starts_with(&provider.uri()));
        let redirect = location
            .query_pairs()
            .find(|(key, _)| key == "redirect_uri")
            .map(|(_, value)| value.into_owned());
        assert_eq!(
            redirect.as_deref(),
            Some("https://wyrd.example.com/auth/callback")
        );
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let stored: Vec<u8> =
            sqlx::query_scalar("SELECT browser_binding_hash FROM wyrd.auth_login_state")
                .fetch_one(&mut **conn.transaction())
                .await
                .expect("binding reads");
        assert_eq!(
            stored,
            Sha256Hex::digest(binding.as_bytes()).as_bytes().to_vec()
        );
    }

    /// A foreign or absent origin and a tenant without a connection begin
    /// nothing and set no cookie.
    ///
    /// # Panics
    /// Panics when either sign-in is begun.
    #[tokio::test]
    async fn refused_logins_begin_nothing() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let provider = MockServer::start().await;
        let state = login_state(&fixture, &provider).await;

        let foreign = begin(
            state.clone(),
            fixture.tenant_slug(),
            "https://attacker.example.net",
        )
        .await;
        assert_eq!(foreign.status(), StatusCode::FORBIDDEN);
        assert!(foreign.headers().get(header::SET_COOKIE).is_none());

        let unknown = begin(state, "no-such-tenant", "https://wyrd.example.com").await;
        assert_eq!(unknown.status(), StatusCode::UNAUTHORIZED);
        assert!(unknown.headers().get(header::SET_COOKIE).is_none());
        assert_eq!(state_rows(&fixture).await, 0);
    }
}
