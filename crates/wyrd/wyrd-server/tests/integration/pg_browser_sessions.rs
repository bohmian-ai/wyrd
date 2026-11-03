//! Rust-owned opaque browser sessions through `/auth/browser`.
//!
//! Drives recovery, session resolution, and logout against the in-process
//! server and the repository-managed Postgres with real API keys: the stored
//! row holds only the cookie's digest and a Postgres-owned twelve-hour
//! absolute expiry; tampered, other-tenant, mislabeled, expired, revoked and
//! inactive-authority sessions all refuse; logout ends only its own session;
//! and a store outage is unavailable rather than any fallback.

use std::collections::BTreeSet;

use axum::body::{Body, to_bytes};
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use secrecy::{ExposeSecret as _, SecretString};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use uuid::Uuid;
use wyrd_testing::{Bootstrap, WyrdTestServer, WyrdTestServerBuilder};

/// The origin every browser request in this module arrives at.
const ORIGIN: &str = "http://wyrd.test";

/// One browser request: method, path, optional `Origin`, cookies and JSON
/// body.
struct BrowserRequest<'a> {
    /// HTTP method.
    method: Method,
    /// Request path and query.
    uri: &'a str,
    /// `Origin` header, when the browser sends one.
    origin: Option<&'a str>,
    /// `Cookie` header pairs.
    cookies: Vec<(String, String)>,
    /// JSON body, when there is one.
    body: Option<Value>,
}

/// A browser answer: status, headers and (possibly empty) JSON body.
struct BrowserResponse {
    /// Response status.
    status: StatusCode,
    /// Response headers.
    headers: HeaderMap,
    /// Parsed JSON body, `Null` when empty.
    body: Value,
}

impl BrowserResponse {
    /// The `Set-Cookie` header for cookie `name`, when one was sent.
    fn set_cookie(&self, name: &str) -> Option<String> {
        self.headers
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .find(|value| value.starts_with(&format!("{name}=")))
            .map(str::to_owned)
    }

    /// The value cookie `name` was set to.
    ///
    /// # Panics
    /// Panics when the cookie was not set.
    fn cookie_value(&self, name: &str) -> String {
        let header = self.set_cookie(name).expect("cookie is set");
        header[name.len() + 1..]
            .split(';')
            .next()
            .expect("cookie has a value")
            .to_owned()
    }
}

/// Send one browser request to the in-process router.
///
/// # Panics
/// As [`send_with`].
async fn send(server: &WyrdTestServer, request: BrowserRequest<'_>) -> BrowserResponse {
    send_with(server, request, None).await
}

/// Send one browser request, with `bearer` as `X-Wyrd-Access-Token` when
/// given, to the in-process router.
///
/// A non-empty body that is not JSON is kept as a JSON string.
///
/// # Panics
/// Panics when the request cannot be built or the route fails to respond.
async fn send_with(
    server: &WyrdTestServer,
    request: BrowserRequest<'_>,
    bearer: Option<&str>,
) -> BrowserResponse {
    let mut builder = Request::builder()
        .method(request.method)
        .uri(request.uri)
        .header(header::HOST, "wyrd.test");
    if let Some(bearer) = bearer {
        builder = builder.header("x-wyrd-access-token", format!("Bearer {bearer}"));
    }
    if let Some(origin) = request.origin {
        builder = builder.header(header::ORIGIN, origin);
    }
    if !request.cookies.is_empty() {
        let cookies = request
            .cookies
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ");
        builder = builder.header(header::COOKIE, cookies);
    }
    let body = match request.body {
        Some(body) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(body.to_string())
        }
        None => Body::empty(),
    };
    let response = server
        .oneshot(builder.body(body).expect("request builds"))
        .await
        .expect("request responds");
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("response body reads");
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()))
    };
    BrowserResponse {
        status,
        headers,
        body,
    }
}

/// Recover a session for `tenant` with `api_key` from `origin`.
async fn recover(
    server: &WyrdTestServer,
    tenant: &str,
    api_key: &str,
    origin: Option<&str>,
) -> BrowserResponse {
    send(
        server,
        BrowserRequest {
            method: Method::POST,
            uri: "/auth/browser/api-key",
            origin,
            cookies: Vec::new(),
            body: Some(json!({ "tenant": tenant, "api_key": api_key })),
        },
    )
    .await
}

/// Read `GET /auth/browser/session` (optionally filtered) with `cookies`.
async fn read(
    server: &WyrdTestServer,
    query: &str,
    cookies: Vec<(String, String)>,
) -> BrowserResponse {
    send(
        server,
        BrowserRequest {
            method: Method::GET,
            uri: &format!("/auth/browser/session{query}"),
            origin: None,
            cookies,
            body: None,
        },
    )
    .await
}

/// The session cookie name for a tenant route key.
fn cookie_name(tenant: &str) -> String {
    format!("wyrd_session_{tenant}")
}

/// Bootstrap an `admin` Service in `tenant` and return its id and API key.
///
/// # Panics
/// As [`service_with`].
async fn service(
    server: &WyrdTestServer,
    tenant: wyrd_spec::DataTenantId,
    name: &str,
) -> (Uuid, SecretString) {
    service_with(server, tenant, name, &["admin"]).await
}

/// Bootstrap a Service holding `roles` in `tenant` and return its id and API
/// key.
///
/// # Panics
/// Panics when bootstrapping fails or returns a user.
async fn service_with(
    server: &WyrdTestServer,
    tenant: wyrd_spec::DataTenantId,
    name: &str,
    roles: &[&str],
) -> (Uuid, SecretString) {
    match server
        .bootstrap_service_in_tenant(tenant, name, roles)
        .await
        .expect("service bootstraps")
    {
        Bootstrap::Machine { id, api_key, .. } => (id.as_uuid(), api_key),
        Bootstrap::User { .. } => panic!("service bootstrap returned a user"),
    }
}

/// Run one statement as the fixture superuser.
///
/// # Panics
/// Panics when the pool or the statement fails.
async fn superuser(server: &WyrdTestServer, sql: &'static str, bind: Uuid) {
    let pool = server
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    sqlx::query(sql)
        .bind(bind)
        .execute(&pool)
        .await
        .expect("superuser statement runs");
}

/// Recovery creates an opaque, digest-only, absolutely expiring session with
/// exactly the key's tenant; every invalid authority refuses; logout is
/// immediate and session-specific and leaves the key usable; and a store
/// outage is unavailable.
///
/// # Panics
/// Panics when the server fails to start or any expectation fails.
#[tokio::test(flavor = "current_thread")]
async fn browser_session_authority_is_tenant_bound_and_revocable() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let acme = server.pg_fixture().tenant_slug().to_owned();
    let beta_id = server.seed_tenant("beta").await.expect("tenant seeds");
    let (principal, key) = service(&server, server.data_tenant_id(), "bs-recovery").await;
    let (_, beta_key) = service(&server, beta_id, "bs-beta").await;
    let key_text = key.expose_secret();

    // Cookie-only mutations require the exact origin.
    for origin in [None, Some("http://evil.test"), Some("null")] {
        let refused = recover(&server, &acme, key_text, origin).await;
        assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.body);
        assert_eq!(refused.body["code"], json!("WYRD_AUTH_403_ORIGIN_REFUSED"));
    }
    // The key establishes the tenant; another route key is refused.
    let mismatched = recover(&server, "beta", key_text, Some(ORIGIN)).await;
    assert_eq!(mismatched.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        mismatched.body["code"],
        json!("WYRD_AUTH_401_API_KEY_INVALID")
    );
    let unknown = recover(&server, "nowhere", key_text, Some(ORIGIN)).await;
    assert_eq!(unknown.status, StatusCode::UNAUTHORIZED);

    // A recovered session: safe metadata, a hardened cookie, no stored secret.
    let created = recover(&server, &acme, key_text, Some(ORIGIN)).await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.body);
    assert_eq!(
        created
            .headers
            .get(header::CACHE_CONTROL)
            .map(axum::http::HeaderValue::as_bytes),
        Some(b"no-store".as_slice())
    );
    assert_eq!(created.body["tenant"], json!(acme));
    assert_eq!(created.body["principal_id"], json!(principal.to_string()));
    assert_eq!(created.body["credential"], json!("api_key"));
    assert!(!created.body.to_string().contains(key_text));
    let set = created.set_cookie(&cookie_name(&acme)).expect("cookie set");
    for attribute in [
        "Path=/",
        "Max-Age=43200",
        "HttpOnly",
        "SameSite=Lax",
        "Secure",
    ] {
        assert!(set.contains(attribute), "{set} lacks {attribute}");
    }
    assert!(!set.contains("Domain="), "{set} is host-only");
    let session = created.cookie_value(&cookie_name(&acme));
    assert!(!session.contains(key_text));
    let digest = Sha256::digest(session.as_bytes()).to_vec();
    let pool = server
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    let (hash, lifetime, api_key_id): (Vec<u8>, f64, Option<Uuid>) = sqlx::query_as(
        "SELECT session_hash, extract(epoch FROM expires_at - created_at)::float8, api_key_id \
           FROM wyrd.auth_browser_sessions WHERE principal_id = $1",
    )
    .bind(principal)
    .fetch_one(&pool)
    .await
    .expect("one stored session");
    assert_eq!(hash, digest, "only the cookie's digest is stored");
    assert!(
        (lifetime - 43_200.0).abs() < 1.0,
        "absolute 12h, got {lifetime}"
    );
    assert!(api_key_id.is_some(), "bound to the originating key");

    // The live session resolves, alone, filtered, and among dead cookies.
    let acme_cookie = |value: &str| vec![(cookie_name(&acme), value.to_owned())];
    let live = read(&server, "", acme_cookie(&session)).await;
    assert_eq!(live.status, StatusCode::OK, "{}", live.body);
    assert_eq!(live.body["sessions"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        live.body["sessions"][0]["tenant_id"],
        created.body["tenant_id"]
    );
    let filtered = read(&server, &format!("?tenant={acme}"), acme_cookie(&session)).await;
    assert_eq!(filtered.status, StatusCode::OK);
    let other = read(&server, "?tenant=beta", acme_cookie(&session)).await;
    assert_eq!(
        other.status,
        StatusCode::UNAUTHORIZED,
        "no fallback to acme"
    );
    let mixed = read(
        &server,
        "",
        vec![
            (cookie_name(&acme), session.clone()),
            (cookie_name("beta"), "garbage".to_owned()),
        ],
    )
    .await;
    assert_eq!(mixed.status, StatusCode::OK);
    assert_eq!(mixed.body["sessions"].as_array().map(Vec::len), Some(1));
    let cleared = mixed
        .set_cookie(&cookie_name("beta"))
        .expect("dead cookie cleared");
    assert!(cleared.contains("Max-Age=0"), "{cleared}");
    assert!(
        mixed.set_cookie(&cookie_name(&acme)).is_none(),
        "live cookie kept"
    );

    // Tampered, other-tenant and mislabeled values refuse and are cleared.
    let mut tampered = session.clone();
    let last = tampered.pop().expect("non-empty");
    tampered.push(if last == 'A' { 'B' } else { 'A' });
    let secret = session.split_once('.').expect("tenant prefix").1;
    for (label, value) in [
        (acme.as_str(), tampered),
        (acme.as_str(), format!("{beta_id}.{secret}")),
        ("beta", session.clone()),
    ] {
        let dead = read(&server, "", vec![(cookie_name(label), value)]).await;
        assert_eq!(dead.status, StatusCode::UNAUTHORIZED, "{}", dead.body);
        assert_eq!(dead.body["code"], json!("WYRD_AUTH_401_INVALID_TOKEN"));
        assert!(dead.set_cookie(&cookie_name(label)).is_some(), "cleared");
    }

    // Expiry is server-enforced from Postgres time.
    let expiring = recover(&server, &acme, key_text, Some(ORIGIN))
        .await
        .cookie_value(&cookie_name(&acme));
    sqlx::query(
        "UPDATE wyrd.auth_browser_sessions \
            SET created_at = created_at - interval '13 hours', \
                expires_at = expires_at - interval '13 hours' \
          WHERE session_hash = $1",
    )
    .bind(Sha256::digest(expiring.as_bytes()).to_vec())
    .execute(&pool)
    .await
    .expect("session ages");
    assert_eq!(
        read(&server, "", acme_cookie(&expiring)).await.status,
        StatusCode::UNAUTHORIZED
    );

    // An inactive principal or tenant refuses while inactive.
    superuser(
        &server,
        "UPDATE wyrd.auth_service_accounts SET status = 'suspended' WHERE id = $1",
        principal,
    )
    .await;
    assert_eq!(
        read(&server, "", acme_cookie(&session)).await.status,
        StatusCode::UNAUTHORIZED
    );
    superuser(
        &server,
        "UPDATE wyrd.auth_service_accounts SET status = 'active' WHERE id = $1",
        principal,
    )
    .await;
    let beta_session = recover(&server, "beta", beta_key.expose_secret(), Some(ORIGIN)).await;
    assert_eq!(beta_session.status, StatusCode::OK, "{}", beta_session.body);
    let beta_cookie = vec![(
        cookie_name("beta"),
        beta_session.cookie_value(&cookie_name("beta")),
    )];
    assert_eq!(
        read(&server, "", beta_cookie.clone()).await.status,
        StatusCode::OK
    );
    // Beta's value relabeled as Acme and logged out as Acme revokes neither
    // tenant's session; only the Acme label is cleared.
    let relabeled = send(
        &server,
        BrowserRequest {
            method: Method::POST,
            uri: "/auth/browser/logout",
            origin: Some(ORIGIN),
            cookies: acme_cookie(&beta_cookie[0].1),
            body: Some(json!({ "tenant": acme })),
        },
    )
    .await;
    assert_eq!(
        relabeled.status,
        StatusCode::NO_CONTENT,
        "{}",
        relabeled.body
    );
    assert!(
        relabeled
            .set_cookie(&cookie_name(&acme))
            .is_some_and(|cleared| cleared.contains("Max-Age=0"))
    );
    assert_eq!(
        read(&server, "", beta_cookie.clone()).await.status,
        StatusCode::OK,
        "the relabeled Beta session survives"
    );
    assert_eq!(
        read(&server, "", acme_cookie(&session)).await.status,
        StatusCode::OK,
        "Acme's own session survives"
    );
    let beta_control = vec![(
        cookie_name("beta"),
        recover(&server, "beta", beta_key.expose_secret(), Some(ORIGIN))
            .await
            .cookie_value(&cookie_name("beta")),
    )];
    superuser(
        &server,
        "UPDATE platform.tenants SET status = 'suspended' WHERE data_tenant_id = $1",
        beta_id.as_uuid(),
    )
    .await;
    assert_eq!(
        read(&server, "", beta_cookie.clone()).await.status,
        StatusCode::UNAUTHORIZED
    );
    // Logout while the tenant is suspended still revokes the selected
    // session, so it stays dead once the tenant resumes; the untouched
    // control session resumes with the tenant.
    let suspended_logout = send(
        &server,
        BrowserRequest {
            method: Method::POST,
            uri: "/auth/browser/logout",
            origin: Some(ORIGIN),
            cookies: beta_cookie.clone(),
            body: Some(json!({ "tenant": "beta" })),
        },
    )
    .await;
    assert_eq!(suspended_logout.status, StatusCode::NO_CONTENT);
    assert!(
        suspended_logout
            .set_cookie(&cookie_name("beta"))
            .is_some_and(|cleared| cleared.contains("Max-Age=0"))
    );
    superuser(
        &server,
        "UPDATE platform.tenants SET status = 'active' WHERE data_tenant_id = $1",
        beta_id.as_uuid(),
    )
    .await;
    assert_eq!(
        read(&server, "", beta_control).await.status,
        StatusCode::OK,
        "the control session resumes with its tenant"
    );
    assert_eq!(
        read(&server, "", beta_cookie).await.status,
        StatusCode::UNAUTHORIZED,
        "a session logged out while suspended stays revoked after resume"
    );

    // Logout ends only its own session, immediately, and keeps the key.
    let second = recover(&server, &acme, key_text, Some(ORIGIN))
        .await
        .cookie_value(&cookie_name(&acme));
    let logout = |origin| BrowserRequest {
        method: Method::POST,
        uri: "/auth/browser/logout",
        origin,
        cookies: acme_cookie(&second),
        body: Some(json!({ "tenant": acme })),
    };
    let refused = send(&server, logout(Some("http://evil.test"))).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    assert_eq!(
        read(&server, "", acme_cookie(&second)).await.status,
        StatusCode::OK,
        "a refused logout revokes nothing"
    );
    let ended = send(&server, logout(Some(ORIGIN))).await;
    assert_eq!(ended.status, StatusCode::NO_CONTENT);
    let cleared = ended
        .set_cookie(&cookie_name(&acme))
        .expect("cookie cleared");
    assert!(cleared.contains("Max-Age=0"), "{cleared}");
    assert_eq!(
        read(&server, "", acme_cookie(&second)).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(&server, logout(Some(ORIGIN))).await.status,
        StatusCode::NO_CONTENT,
        "logging out a revoked session again still clears"
    );
    assert_eq!(
        read(&server, "", acme_cookie(&session)).await.status,
        StatusCode::OK,
        "other sessions survive logout"
    );
    server
        .exchange_api_key(&key)
        .await
        .expect("the API key survives logout");

    // A revoked originating key ends the sessions it created.
    superuser(
        &server,
        "UPDATE wyrd.auth_api_keys SET revoked_at = now() WHERE id = $1",
        api_key_id.expect("bound to a key"),
    )
    .await;
    assert_eq!(
        read(&server, "", acme_cookie(&session)).await.status,
        StatusCode::UNAUTHORIZED
    );

    // Recovery is audited as a login of the key's principal.
    server
        .await_audit_retained()
        .await
        .expect("audit retention settles");
    let logins = server
        .retained_audit_records(
            server.data_tenant_id(),
            "outcome",
            &format!("operation = 'auth.login' AND sub_principal_id = '{principal}'"),
        )
        .await
        .expect("audit rows read");
    assert!(
        logins
            .iter()
            .all(|row| row[0].as_deref() == Some("allowed"))
            && logins.len() == 3,
        "{logins:?}"
    );

    // A store outage is unavailable before any cookie is judged dead.
    server.state().postgres.wyrd().app_pool().close().await;
    let outage = read(&server, "", acme_cookie(&session)).await;
    assert_eq!(
        outage.status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{}",
        outage.body
    );
    assert_eq!(
        outage.body["code"],
        json!("WYRD_AUTH_503_VERIFY_UNAVAILABLE")
    );
    assert!(
        outage.set_cookie(&cookie_name(&acme)).is_none(),
        "nothing cleared"
    );
}

/// Call a tenant API route as a browser carrying `cookies`, from `origin`,
/// optionally with an explicit bearer.
async fn api(
    server: &WyrdTestServer,
    method: Method,
    uri: &str,
    origin: Option<&str>,
    cookies: Vec<(String, String)>,
    bearer: Option<&str>,
) -> BrowserResponse {
    send_with(
        server,
        BrowserRequest {
            method,
            uri,
            origin,
            cookies,
            body: None,
        },
        bearer,
    )
    .await
}

/// The stored expiry of a session and its recovery key's last use.
///
/// # Panics
/// Panics when the read fails.
async fn session_state(server: &WyrdTestServer, cookie: &str) -> (String, Option<String>) {
    let pool = server
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    sqlx::query_as(
        "SELECT s.expires_at::text, k.last_used_at::text \
           FROM wyrd.auth_browser_sessions s \
           JOIN wyrd.auth_api_keys k ON k.id = s.api_key_id \
          WHERE s.session_hash = $1",
    )
    .bind(Sha256::digest(cookie.as_bytes()).to_vec())
    .fetch_one(&pool)
    .await
    .expect("session reads")
}

/// A selected browser session reaches the same tenant API permission checks
/// and audit as a bearer; mutations need the exact origin; an explicit bearer
/// never falls back to a cookie; ambiguous, dead, wrong-tenant and
/// least-privilege sessions stay refused; reads neither slide nor write; and
/// MCP, OTLP and platform surfaces stay bearer-only.
///
/// # Panics
/// Panics when the server fails to start or any expectation fails.
#[tokio::test(flavor = "current_thread")]
async fn browser_session_requests_enforce_origin_and_bearer_precedence() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let acme = server.pg_fixture().tenant_slug().to_owned();
    let beta_id = server.seed_tenant("beta").await.expect("tenant seeds");
    let (admin, admin_key) = service(&server, server.data_tenant_id(), "bs-admin").await;
    let (_, viewer_key) =
        service_with(&server, server.data_tenant_id(), "bs-viewer", &["viewer"]).await;
    let (target, _) = service_with(&server, server.data_tenant_id(), "bs-target", &[]).await;
    let (_, beta_key) = service(&server, beta_id, "bs-beta").await;
    let session_for = |tenant: String, key: SecretString| {
        let server = &server;
        async move {
            let created = recover(server, &tenant, key.expose_secret(), Some(ORIGIN)).await;
            assert_eq!(created.status, StatusCode::OK, "{}", created.body);
            vec![(
                cookie_name(&tenant),
                created.cookie_value(&cookie_name(&tenant)),
            )]
        }
    };
    let admin_cookie = session_for(acme.clone(), admin_key).await;
    let viewer_cookie = session_for(acme.clone(), viewer_key).await;
    let beta_cookie = session_for("beta".to_owned(), beta_key).await;
    let grant = format!("/v1/principals/{target}/roles/editor");

    // A read authenticates by the session and neither slides nor writes it.
    let before = session_state(&server, &admin_cookie[0].1).await;
    let listed = api(
        &server,
        Method::GET,
        "/v1/principals",
        None,
        admin_cookie.clone(),
        None,
    )
    .await;
    assert_eq!(listed.status, StatusCode::OK, "{}", listed.body);
    assert_eq!(session_state(&server, &admin_cookie[0].1).await, before);

    // A mutation requires the exact origin.
    for origin in [None, Some("http://evil.test"), Some("null")] {
        let refused = api(
            &server,
            Method::PUT,
            &grant,
            origin,
            admin_cookie.clone(),
            None,
        )
        .await;
        assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.body);
        assert_eq!(refused.body["code"], json!("WYRD_AUTH_403_ORIGIN_REFUSED"));
    }
    let granted = api(
        &server,
        Method::PUT,
        &grant,
        Some(ORIGIN),
        admin_cookie.clone(),
        None,
    )
    .await;
    assert_eq!(granted.status, StatusCode::OK, "{}", granted.body);
    assert_eq!(granted.body["changed"], json!(true));

    // An explicit bearer always wins; a refused one never falls back.
    let bad_bearer = api(
        &server,
        Method::GET,
        "/v1/principals",
        None,
        admin_cookie.clone(),
        Some("not-a-jwt"),
    )
    .await;
    assert_eq!(
        bad_bearer.status,
        StatusCode::BAD_REQUEST,
        "{}",
        bad_bearer.body
    );
    assert_eq!(
        bad_bearer.body["code"],
        json!("WYRD_AUTH_400_BAD_TOKEN_FORMAT")
    );

    // Exactly one session cookie; a dead one is refused, never anonymous.
    let both = [admin_cookie.clone(), beta_cookie.clone()].concat();
    let ambiguous = api(&server, Method::GET, "/v1/principals", None, both, None).await;
    assert_eq!(
        ambiguous.status,
        StatusCode::BAD_REQUEST,
        "{}",
        ambiguous.body
    );
    assert_eq!(
        ambiguous.body["details"]["reason"],
        json!("browser_session_ambiguous")
    );
    let dead = vec![(
        cookie_name(&acme),
        format!("{}.forged", server.data_tenant_id()),
    )];
    let refused = api(&server, Method::GET, "/v1/principals", None, dead, None).await;
    assert_eq!(refused.status, StatusCode::UNAUTHORIZED, "{}", refused.body);
    assert_eq!(refused.body["code"], json!("WYRD_AUTH_401_INVALID_TOKEN"));
    let none = api(
        &server,
        Method::GET,
        "/v1/principals",
        None,
        Vec::new(),
        None,
    )
    .await;
    assert_eq!(none.status, StatusCode::UNAUTHORIZED);

    // The session's tenant and the key's least privilege still bind.
    let foreign = api(
        &server,
        Method::GET,
        &format!("/v1/principals/{target}/roles"),
        None,
        beta_cookie.clone(),
        None,
    )
    .await;
    assert_eq!(foreign.status, StatusCode::NOT_FOUND, "{}", foreign.body);
    let denied = api(
        &server,
        Method::DELETE,
        &grant,
        Some(ORIGIN),
        viewer_cookie,
        None,
    )
    .await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN, "{}", denied.body);
    assert_eq!(
        denied.body["code"],
        json!("WYRD_PERMISSION_403_DENIED_RBAC")
    );

    // Other protocol surfaces stay bearer-only.
    for (method, uri) in [
        (Method::POST, "/mcp"),
        (Method::POST, "/v1/traces"),
        (Method::GET, "/platform/oidc/connection"),
    ] {
        let refused = api(
            &server,
            method,
            uri,
            Some(ORIGIN),
            admin_cookie.clone(),
            None,
        )
        .await;
        assert_eq!(
            refused.status,
            StatusCode::UNAUTHORIZED,
            "{uri}: {}",
            refused.body
        );
    }

    // The cookie-authenticated grant is audited as the session's principal.
    server
        .await_audit_retained()
        .await
        .expect("audit retention settles");
    let decisions = server
        .retained_audit_records(
            server.data_tenant_id(),
            "outcome",
            &format!("operation = 'auth.principal.role.grant' AND sub_principal_id = '{admin}'"),
        )
        .await
        .expect("audit rows read");
    assert_eq!(decisions, vec![vec![Some("allowed".to_owned())]]);
}

/// A password meeting the local policy, distinct per `n`.
fn strong(n: u32) -> String {
    format!("Quiet harbor lantern {n}")
}

/// Send `body` to `uri` as `method` with `bearer`, from this origin.
async fn as_bearer(
    server: &WyrdTestServer,
    method: Method,
    uri: &str,
    bearer: &str,
    body: Option<Value>,
) -> BrowserResponse {
    send_with(
        server,
        BrowserRequest {
            method,
            uri,
            origin: Some(ORIGIN),
            cookies: Vec::new(),
            body,
        },
        Some(bearer),
    )
    .await
}

/// Send `body` to `uri` as `method` with one tenant session `cookie`, from
/// this origin.
async fn as_browser(
    server: &WyrdTestServer,
    method: Method,
    uri: &str,
    tenant: &str,
    cookie: &str,
    body: Option<Value>,
) -> BrowserResponse {
    send(
        server,
        BrowserRequest {
            method,
            uri,
            origin: Some(ORIGIN),
            cookies: vec![(cookie_name(tenant), cookie.to_owned())],
            body,
        },
    )
    .await
}

/// POST an anonymous `/auth/browser/password/{operation}` request.
async fn password(server: &WyrdTestServer, operation: &str, body: Value) -> BrowserResponse {
    send(
        server,
        BrowserRequest {
            method: Method::POST,
            uri: &format!("/auth/browser/password/{operation}"),
            origin: Some(ORIGIN),
            cookies: Vec::new(),
            body: Some(body),
        },
    )
    .await
}

/// An administrator bearer for `tenant`.
///
/// # Panics
/// Panics when bootstrapping or the exchange fails.
async fn admin_bearer(server: &WyrdTestServer, tenant: wyrd_spec::DataTenantId) -> String {
    let (_, key) = service(server, tenant, &format!("admin-{}", Uuid::now_v7())).await;
    server
        .exchange_api_key(&key)
        .await
        .expect("admin exchanges")
}

/// Assert `response` carries a one-time secret no cache may keep.
///
/// # Panics
/// Panics when `Cache-Control: no-store` is absent.
fn assert_no_store(response: &BrowserResponse) {
    assert_eq!(
        response
            .headers
            .get(header::CACHE_CONTROL)
            .map(axum::http::HeaderValue::as_bytes),
        Some(&b"no-store"[..]),
        "a secret-bearing response must be no-store"
    );
}

/// Issue an invitation for the built-in `role` with `bearer`; returns the
/// response.
async fn invite(server: &WyrdTestServer, bearer: &str, role: &str) -> BrowserResponse {
    as_bearer(
        server,
        Method::POST,
        "/v1/principals/users/invitations",
        bearer,
        Some(json!({ "roles": [role] })),
    )
    .await
}

/// Enroll `username` with `secret` through a fresh `role` invitation in the
/// fixture tenant; returns the signup response.
///
/// # Panics
/// Panics when the invitation is refused.
async fn enroll(
    server: &WyrdTestServer,
    bearer: &str,
    role: &str,
    username: &str,
    secret: &str,
) -> BrowserResponse {
    let issued = invite(server, bearer, role).await;
    assert_eq!(issued.status, StatusCode::CREATED, "{}", issued.body);
    password(
        server,
        "signup",
        json!({
            "tenant": server.pg_fixture().tenant_slug(),
            "invitation_code": issued.body["invitation_code"],
            "username": username,
            "password": secret,
        }),
    )
    .await
}

/// Sign `username` in with `secret` to the fixture tenant.
async fn login(server: &WyrdTestServer, username: &str, secret: &str) -> BrowserResponse {
    password(
        server,
        "login",
        json!({
            "tenant": server.pg_fixture().tenant_slug(),
            "username": username,
            "password": secret,
        }),
    )
    .await
}

/// Invitation signup creates a verifier-only local User with its Roles and a
/// password session; login, change and operator reset follow the password
/// revision, ending every local session at every replica and spending stale
/// codes.
///
/// # Panics
/// Panics when the server fails to start or any expectation fails.
#[tokio::test(flavor = "current_thread")]
async fn local_signup_login_and_password_lifecycle() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let acme = server.pg_fixture().tenant_slug().to_owned();
    let admin = admin_bearer(&server, server.data_tenant_id()).await;
    let pool = server
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");

    // The invitation code is returned once, no-store, beside a fixed URL.
    let issued = invite(&server, &admin, "viewer").await;
    assert_eq!(issued.status, StatusCode::CREATED, "{}", issued.body);
    assert_eq!(
        issued
            .headers
            .get(header::CACHE_CONTROL)
            .map(axum::http::HeaderValue::as_bytes),
        Some(b"no-store".as_slice())
    );
    let code = issued.body["invitation_code"]
        .as_str()
        .expect("code")
        .to_owned();
    assert_eq!(code.len(), 43);
    assert_eq!(
        issued.body["signup_url"],
        json!(format!("{ORIGIN}/t/{acme}/signup"))
    );
    let signup = |username: &str, secret: &str| json!({ "tenant": acme, "invitation_code": code, "username": username, "password": secret });

    // Policy refusals spend nothing.
    for (username, secret) in [
        ("x", strong(1)),
        ("ada", "short".to_owned()),
        ("ada", "password123456789".to_owned()),
        ("ada", format!("{}\u{7}", strong(1))),
    ] {
        let refused = password(&server, "signup", signup(username, &secret)).await;
        assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{}", refused.body);
        assert_eq!(refused.body["code"], json!("WYRD_SPEC_400_VALIDATION"));
    }

    // Signup enrolls and signs in; the username is canonical.
    let created = password(&server, "signup", signup("  Ada.Lovelace ", &strong(1))).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    assert_eq!(created.body["credential"], json!("password"));
    assert_eq!(created.body["principal_kind"], json!("user"));
    assert!(!created.body.to_string().contains(&strong(1)));
    let user = Uuid::parse_str(created.body["principal_id"].as_str().expect("id")).expect("uuid");
    let first = created.cookie_value(&cookie_name(&acme));
    let (username, verifier, revision): (String, String, i64) = sqlx::query_as(
        "SELECT username, password_hash, password_revision FROM wyrd.auth_users WHERE id = $1",
    )
    .bind(user)
    .fetch_one(&pool)
    .await
    .expect("user stored");
    assert_eq!(username, "ada.lovelace");
    assert!(
        verifier.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"),
        "{verifier}"
    );
    assert!(!verifier.contains(&strong(1)));
    assert_eq!(revision, 0);
    let roles: Vec<(String,)> = sqlx::query_as(
        "SELECT r.name FROM wyrd.auth_user_roles ur JOIN wyrd.auth_roles r ON r.id = ur.role_id \
          WHERE ur.user_id = $1",
    )
    .bind(user)
    .fetch_all(&pool)
    .await
    .expect("roles read");
    assert_eq!(roles, vec![("viewer".to_owned(),)]);

    // The code is single-use.
    let reused = password(&server, "signup", signup("grace", &strong(2))).await;
    assert_eq!(reused.status, StatusCode::UNAUTHORIZED, "{}", reused.body);
    assert_eq!(
        reused.body["code"],
        json!("WYRD_AUTH_401_ONE_TIME_CODE_INVALID")
    );

    // The profile is the local account.
    let me = as_browser(
        &server,
        Method::GET,
        "/v1/principals/me",
        &acme,
        &first,
        None,
    )
    .await;
    assert_eq!(me.status, StatusCode::OK, "{}", me.body);
    assert_eq!(me.body["username"], json!("ada.lovelace"));
    assert_eq!(me.body["authentication"], json!("password"));
    let permissions = me.body["permissions"].as_array().expect("permissions");
    for held in [
        "cards:read",
        "personal_credentials:read",
        "personal_credentials:write",
    ] {
        assert!(
            permissions.contains(&json!(held)),
            "{held} missing: {permissions:?}"
        );
    }

    // Login: one refusal for every bad pair, then a fresh session.
    for (name, secret) in [("ada.lovelace", strong(9)), ("nobody", strong(1))] {
        let refused = login(&server, name, &secret).await;
        assert_eq!(refused.status, StatusCode::UNAUTHORIZED, "{}", refused.body);
        assert_eq!(
            refused.body["code"],
            json!("WYRD_AUTH_401_PASSWORD_INVALID")
        );
    }
    let second = login(&server, "ADA.LOVELACE", &strong(1)).await;
    assert_eq!(second.status, StatusCode::OK, "{}", second.body);
    let second = second.cookie_value(&cookie_name(&acme));
    assert_ne!(second, first);

    // A second replica over the same database serves the same sessions.
    let replica = server
        .start_replica(WyrdTestServerBuilder::default())
        .await
        .expect("replica starts");
    let live = read(&replica, "", vec![(cookie_name(&acme), second.clone())]).await;
    assert_eq!(live.status, StatusCode::OK, "{}", live.body);
    let key = as_browser(
        &server,
        Method::POST,
        &format!("/v1/principals/{user}/credentials"),
        &acme,
        &second,
        Some(json!({ "name": "laptop", "permission_ceiling": ["cards:read"], "expires_in": 3600 })),
    )
    .await;
    assert_eq!(key.status, StatusCode::OK, "{}", key.body);
    let key = SecretString::from(key.body["credential"].as_str().expect("key").to_owned());

    // Change needs the current password and ends every local session.
    let change = |current: String, next: String| json!({ "tenant": acme, "current_password": current, "new_password": next });
    let wrong = as_browser(
        &server,
        Method::POST,
        "/auth/browser/password/change",
        &acme,
        &second,
        Some(change(strong(9), strong(3))),
    )
    .await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED, "{}", wrong.body);
    let changed = as_browser(
        &server,
        Method::POST,
        "/auth/browser/password/change",
        &acme,
        &second,
        Some(change(strong(1), strong(3))),
    )
    .await;
    assert_eq!(changed.status, StatusCode::NO_CONTENT, "{}", changed.body);
    assert!(
        changed
            .set_cookie(&cookie_name(&acme))
            .is_some_and(|cookie| cookie.contains("Max-Age=0"))
    );
    for at in [&server, &replica] {
        for dead in [&first, &second] {
            let read = read(at, "", vec![(cookie_name(&acme), dead.clone())]).await;
            assert_eq!(read.status, StatusCode::UNAUTHORIZED, "{}", read.body);
        }
    }
    assert_eq!(
        login(&replica, "ada.lovelace", &strong(1)).await.status,
        StatusCode::UNAUTHORIZED
    );
    let third = login(&replica, "ada.lovelace", &strong(3)).await;
    assert_eq!(third.status, StatusCode::OK, "{}", third.body);
    let third = third.cookie_value(&cookie_name(&acme));

    // A reset issued before a change is spent by it.
    let reset_uri = format!("/v1/principals/users/{user}/password-reset");
    let stale = as_bearer(&server, Method::POST, &reset_uri, &admin, None).await;
    assert_eq!(stale.status, StatusCode::CREATED, "{}", stale.body);
    assert_eq!(
        stale.body["reset_url"],
        json!(format!("{ORIGIN}/t/{acme}/reset"))
    );
    let changed = as_browser(
        &server,
        Method::POST,
        "/auth/browser/password/change",
        &acme,
        &third,
        Some(change(strong(3), strong(4))),
    )
    .await;
    assert_eq!(changed.status, StatusCode::NO_CONTENT, "{}", changed.body);
    let reset = |code: &Value, next: String| json!({ "tenant": acme, "reset_code": code, "new_password": next });
    let refused = password(
        &server,
        "reset",
        reset(&stale.body["reset_code"], strong(5)),
    )
    .await;
    assert_eq!(refused.status, StatusCode::UNAUTHORIZED, "{}", refused.body);

    // A current reset sets the password, signs nothing in and is single-use.
    let session = login(&server, "ada.lovelace", &strong(4)).await;
    let session = session.cookie_value(&cookie_name(&acme));
    let issued = as_bearer(&server, Method::POST, &reset_uri, &admin, None).await;
    let done = password(
        &server,
        "reset",
        reset(&issued.body["reset_code"], strong(6)),
    )
    .await;
    assert_eq!(done.status, StatusCode::NO_CONTENT, "{}", done.body);
    assert!(done.set_cookie(&cookie_name(&acme)).is_none());
    let again = password(
        &server,
        "reset",
        reset(&issued.body["reset_code"], strong(7)),
    )
    .await;
    assert_eq!(again.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        read(&replica, "", vec![(cookie_name(&acme), session)])
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login(&replica, "ada.lovelace", &strong(6)).await.status,
        StatusCode::OK
    );
    replica
        .exchange_api_key(&key)
        .await
        .expect("the personal key survives change and reset");

    // A suspended User cannot sign in.
    superuser(
        &server,
        "UPDATE wyrd.auth_users SET status = 'suspended' WHERE id = $1",
        user,
    )
    .await;
    assert_eq!(
        login(&server, "ada.lovelace", &strong(6)).await.status,
        StatusCode::UNAUTHORIZED
    );
}

/// Malformed and schema-invalid bodies on every password route are the
/// canonical validation problem and never echo the submitted secret.
///
/// # Panics
/// Panics when the server fails to start or any expectation fails.
#[tokio::test(flavor = "current_thread")]
async fn password_routes_refuse_invalid_bodies() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let secret = "invalid-body-secret-0451";
    for operation in ["signup", "login", "change", "reset"] {
        let malformed = server
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/auth/browser/password/{operation}"))
                    .header(header::HOST, "wyrd.test")
                    .header(header::ORIGIN, ORIGIN)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(format!("{{\"password\":\"{secret}\"")))
                    .expect("request builds"),
            )
            .await
            .expect("request responds");
        let status = malformed.status();
        let kind = malformed.headers().get(header::CONTENT_TYPE).cloned();
        let bytes = to_bytes(malformed.into_body(), 1 << 20)
            .await
            .expect("response body reads");
        let body: Value = serde_json::from_slice(&bytes).expect("problem is JSON");
        let invalid = password(&server, operation, json!({ "password": secret })).await;
        for (status, kind, body) in [
            (status, kind, body),
            (
                invalid.status,
                invalid.headers.get(header::CONTENT_TYPE).cloned(),
                invalid.body,
            ),
        ] {
            assert_eq!(status, StatusCode::BAD_REQUEST, "{operation}: {body}");
            assert_eq!(body["code"], "WYRD_SPEC_400_VALIDATION", "{operation}");
            assert_eq!(
                kind.as_ref().map(axum::http::HeaderValue::as_bytes),
                Some(&b"application/problem+json"[..]),
                "{operation}"
            );
            assert!(!body.to_string().contains(secret), "{operation} echoes");
        }
    }
}

/// Enrollment and sign-in are bound to one tenant and one live invitation,
/// concurrent use enrolls once with no partial User, administration is
/// required to invite, and password failures are counted per account until
/// the window refuses with its retry time.
///
/// # Panics
/// Panics when the server fails to start or any expectation fails.
#[tokio::test(flavor = "current_thread")]
async fn local_signup_is_tenant_bound_and_rate_limited() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let acme = server.pg_fixture().tenant_slug().to_owned();
    let beta_id = server.seed_tenant("beta").await.expect("tenant seeds");
    let admin = admin_bearer(&server, server.data_tenant_id()).await;
    let pool = server
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    let users_named = |name: &'static str| {
        let pool = pool.clone();
        async move {
            let (count,): (i64,) =
                sqlx::query_as("SELECT count(*) FROM wyrd.auth_users WHERE username = $1")
                    .bind(name)
                    .fetch_one(&pool)
                    .await
                    .expect("count reads");
            count
        }
    };

    // An invitation names its own tenant only.
    let issued = invite(&server, &admin, "viewer").await;
    let code = issued.body["invitation_code"].clone();
    let elsewhere = password(
        &server,
        "signup",
        json!({ "tenant": "beta", "invitation_code": code, "username": "mallory", "password": strong(1) }),
    )
    .await;
    assert_eq!(
        elsewhere.status,
        StatusCode::UNAUTHORIZED,
        "{}",
        elsewhere.body
    );
    assert_eq!(users_named("mallory").await, 0, "no partial User");

    // Malformed, revoked and expired invitations refuse.
    let malformed = password(
        &server,
        "signup",
        json!({ "tenant": acme, "invitation_code": "short", "username": "mallory", "password": strong(1) }),
    )
    .await;
    assert_eq!(
        malformed.body["code"],
        json!("WYRD_AUTH_401_ONE_TIME_CODE_INVALID")
    );
    let revocable = invite(&server, &admin, "viewer").await;
    let revoke_uri = format!(
        "/v1/principals/users/invitations/{}",
        revocable.body["invitation"]["id"].as_str().expect("id")
    );
    assert_eq!(
        as_bearer(&server, Method::DELETE, &revoke_uri, &admin, None)
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    let expiring = invite(&server, &admin, "viewer").await;
    sqlx::query(
        "UPDATE wyrd.auth_user_invitations \
            SET created_at = created_at - interval '25 hours', \
                expires_at = expires_at - interval '25 hours' WHERE id = $1",
    )
    .bind(Uuid::parse_str(expiring.body["invitation"]["id"].as_str().expect("id")).expect("uuid"))
    .execute(&pool)
    .await
    .expect("invitation ages");
    for dead in [&revocable, &expiring] {
        let refused = password(
            &server,
            "signup",
            json!({ "tenant": acme, "invitation_code": dead.body["invitation_code"], "username": "mallory", "password": strong(1) }),
        )
        .await;
        assert_eq!(refused.status, StatusCode::UNAUTHORIZED, "{}", refused.body);
    }
    assert_eq!(users_named("mallory").await, 0);

    // Two concurrent enrollments with one code: exactly one User.
    let body = |name: &str| json!({ "tenant": acme, "invitation_code": code, "username": name, "password": strong(1) });
    let (left, right) = tokio::join!(
        password(&server, "signup", body("racer-a")),
        password(&server, "signup", body("racer-b")),
    );
    let statuses = [left.status, right.status];
    assert!(
        statuses.contains(&StatusCode::CREATED) && statuses.contains(&StatusCode::UNAUTHORIZED),
        "{statuses:?}"
    );
    assert_eq!(
        users_named("racer-a").await + users_named("racer-b").await,
        1
    );
    let winner = if left.status == StatusCode::CREATED {
        "racer-a"
    } else {
        "racer-b"
    };

    // A taken username refuses without consuming the invitation.
    let fresh = invite(&server, &admin, "viewer").await;
    let taken = password(
        &server,
        "signup",
        json!({ "tenant": acme, "invitation_code": fresh.body["invitation_code"], "username": winner, "password": strong(2) }),
    )
    .await;
    assert_eq!(taken.status, StatusCode::BAD_REQUEST, "{}", taken.body);
    let reused = password(
        &server,
        "signup",
        json!({ "tenant": acme, "invitation_code": fresh.body["invitation_code"], "username": "late", "password": strong(2) }),
    )
    .await;
    assert_eq!(reused.status, StatusCode::CREATED, "{}", reused.body);

    // Accounts are tenant-local; a viewer cannot invite.
    let other_tenant = password(
        &server,
        "login",
        json!({ "tenant": "beta", "username": winner, "password": strong(1) }),
    )
    .await;
    assert_eq!(other_tenant.status, StatusCode::UNAUTHORIZED);
    let viewer = reused.cookie_value(&cookie_name(&acme));
    let forbidden = as_browser(
        &server,
        Method::POST,
        "/v1/principals/users/invitations",
        &acme,
        &viewer,
        Some(json!({ "roles": ["viewer"] })),
    )
    .await;
    assert_eq!(
        forbidden.status,
        StatusCode::FORBIDDEN,
        "{}",
        forbidden.body
    );
    let beta_admin = admin_bearer(&server, beta_id).await;
    let unknown_role = invite(&server, &beta_admin, "no-such-role").await;
    assert_eq!(
        unknown_role.status,
        StatusCode::BAD_REQUEST,
        "{}",
        unknown_role.body
    );

    // Oversized input refuses before derivation and is not counted.
    let oversized = login(&server, winner, &"x".repeat(2000)).await;
    assert_eq!(oversized.status, StatusCode::UNAUTHORIZED);

    // Ten failures per account, then the window refuses even the right
    // password, with its retry time.
    for _ in 0..10 {
        assert_eq!(
            login(&server, winner, &strong(9)).await.status,
            StatusCode::UNAUTHORIZED
        );
    }
    let limited = login(&server, winner, &strong(1)).await;
    assert_eq!(
        limited.status,
        StatusCode::TOO_MANY_REQUESTS,
        "{}",
        limited.body
    );
    assert_eq!(
        limited.body["code"],
        json!("WYRD_AUTH_429_PASSWORD_ATTEMPTS")
    );
    let retry: u64 = limited
        .headers
        .get(header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
        .expect("Retry-After seconds");
    assert!((1..=900).contains(&retry), "{retry}");
    assert_eq!(
        login(&server, "late", &strong(2)).await.status,
        StatusCode::OK,
        "another account is unaffected"
    );
}

/// A User key has one User owner, a stored sorted ceiling and an expiry;
/// creating one for oneself needs a recent password session; its exchange
/// carries the User's current grants narrowed to the ceiling; own listing
/// pages with a bound cursor; and revocation and suspension end it.
///
/// # Panics
/// Panics when the server fails to start or any expectation fails.
#[tokio::test(flavor = "current_thread")]
async fn user_keys_preserve_owner_scope_and_credential_lifecycle() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let acme = server.pg_fixture().tenant_slug().to_owned();
    let admin = admin_bearer(&server, server.data_tenant_id()).await;
    let pool = server
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    let created = enroll(&server, &admin, "viewer", "keyholder", &strong(1)).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let user = created.body["principal_id"]
        .as_str()
        .expect("id")
        .to_owned();
    let cookie = created.cookie_value(&cookie_name(&acme));
    let keys = format!("/v1/principals/{user}/credentials");
    let key_body = |ceiling: Value| json!({ "name": " laptop ", "permission_ceiling": ceiling, "expires_in": 3600 });

    // A User target needs a valid body within both parties' authority.
    for (body, status) in [
        (None, StatusCode::BAD_REQUEST),
        (Some(key_body(json!([]))), StatusCode::BAD_REQUEST),
        (
            Some(key_body(json!(["cards:read", "cards:read"]))),
            StatusCode::BAD_REQUEST,
        ),
        (
            Some(key_body(json!(["service_accounts:write"]))),
            StatusCode::BAD_REQUEST,
        ),
        (
            Some(
                json!({ "name": "x", "permission_ceiling": ["cards:read"], "expires_in": 31_536_001 }),
            ),
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let refused = as_browser(&server, Method::POST, &keys, &acme, &cookie, body).await;
        assert_eq!(refused.status, status, "{}", refused.body);
    }

    // A recent password session creates the key; the stored row is User-owned.
    let issued = as_browser(
        &server,
        Method::POST,
        &keys,
        &acme,
        &cookie,
        Some(key_body(json!(["personal_credentials:read", "cards:read"]))),
    )
    .await;
    assert_eq!(issued.status, StatusCode::OK, "{}", issued.body);
    assert_no_store(&issued);
    let key = SecretString::from(issued.body["credential"].as_str().expect("key").to_owned());
    let key_id = Uuid::parse_str(issued.body["id"].as_str().expect("id")).expect("uuid");
    let (owner, account, name, ceiling, expires): (
        Option<Uuid>,
        Option<Uuid>,
        String,
        Value,
        bool,
    ) = sqlx::query_as(
        "SELECT user_id, service_account_id, name, permission_ceiling, expires_at IS NOT NULL \
               FROM wyrd.auth_api_keys WHERE id = $1",
    )
    .bind(key_id)
    .fetch_one(&pool)
    .await
    .expect("key stored");
    assert_eq!(owner.map(|id| id.to_string()), Some(user.clone()));
    assert_eq!(account, None);
    assert_eq!(name, "laptop");
    assert_eq!(ceiling, json!(["cards:read", "personal_credentials:read"]));
    assert!(expires);

    // The key acts as the User, narrowed to its ceiling.
    let token = server
        .exchange_api_key(&key)
        .await
        .expect("user key exchanges");
    let me = as_bearer(&server, Method::GET, "/v1/principals/me", &token, None).await;
    assert_eq!(me.status, StatusCode::OK, "{}", me.body);
    assert_eq!(me.body["principal_id"], json!(user));
    assert_eq!(
        me.body["permissions"],
        json!(["cards:read", "personal_credentials:read"])
    );
    let unpaged = as_bearer(&server, Method::GET, &keys, &token, None).await;
    assert_eq!(unpaged.status, StatusCode::BAD_REQUEST, "{}", unpaged.body);
    let paged = as_bearer(
        &server,
        Method::GET,
        &format!("{keys}?limit=10"),
        &token,
        None,
    )
    .await;
    assert_eq!(paged.status, StatusCode::OK, "{}", paged.body);
    assert_eq!(paged.body["credentials"][0]["name"], json!("laptop"));
    let revoke_uri = format!("{keys}/{key_id}");
    assert_eq!(
        as_bearer(&server, Method::DELETE, &revoke_uri, &token, None)
            .await
            .status,
        StatusCode::FORBIDDEN,
        "the ceiling withholds personal_credentials:write"
    );

    // A later role never widens the key.
    let editor = wyrd_runtime::builtin_roles::builtin_role_uuid(server.data_tenant_id(), "editor");
    sqlx::query(
        "INSERT INTO wyrd.auth_user_roles (data_tenant_id, user_id, role_id, source) \
         SELECT data_tenant_id, id, $2, 'direct' FROM wyrd.auth_users WHERE id = $1",
    )
    .bind(Uuid::parse_str(&user).expect("uuid"))
    .bind(editor)
    .execute(&pool)
    .await
    .expect("role granted");
    let token = server.exchange_api_key(&key).await.expect("exchanges");
    let me = as_bearer(&server, Method::GET, "/v1/principals/me", &token, None).await;
    assert_eq!(
        me.body["permissions"],
        json!(["cards:read", "personal_credentials:read"])
    );

    // Recovery with the key and a stale session are not recent proof.
    let recovered = recover(&server, &acme, key.expose_secret(), Some(ORIGIN)).await;
    assert_eq!(recovered.status, StatusCode::OK, "{}", recovered.body);
    let recovered = recovered.cookie_value(&cookie_name(&acme));
    let body = Some(key_body(json!(["cards:read"])));
    let refused = as_browser(
        &server,
        Method::POST,
        &keys,
        &acme,
        &recovered,
        body.clone(),
    )
    .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.body);
    sqlx::query(
        "UPDATE wyrd.auth_browser_sessions SET created_at = created_at - interval '6 minutes', \
                expires_at = expires_at - interval '6 minutes' WHERE session_hash = $1",
    )
    .bind(Sha256::digest(cookie.as_bytes()).to_vec())
    .execute(&pool)
    .await
    .expect("session ages");
    let stale = as_browser(&server, Method::POST, &keys, &acme, &cookie, body.clone()).await;
    assert_eq!(stale.status, StatusCode::FORBIDDEN, "{}", stale.body);
    assert_eq!(
        stale.body["code"],
        json!("WYRD_AUTH_403_RECENT_LOGIN_REQUIRED")
    );

    // Pages are newest first and complete; a cursor is bound to its principal.
    let fresh = login(&server, "keyholder", &strong(1))
        .await
        .cookie_value(&cookie_name(&acme));
    for _ in 0..2 {
        let made = as_browser(&server, Method::POST, &keys, &acme, &fresh, body.clone()).await;
        assert_eq!(made.status, StatusCode::OK, "{}", made.body);
    }
    let first = as_browser(
        &server,
        Method::GET,
        &format!("{keys}?limit=2"),
        &acme,
        &fresh,
        None,
    )
    .await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.body);
    assert_eq!(first.body["credentials"].as_array().map(Vec::len), Some(2));
    let cursor = first.body["next_cursor"].as_str().expect("more").to_owned();
    let rest = as_browser(
        &server,
        Method::GET,
        &format!("{keys}?limit=2&cursor={cursor}"),
        &acme,
        &fresh,
        None,
    )
    .await;
    assert_eq!(rest.body["credentials"].as_array().map(Vec::len), Some(1));
    assert_eq!(rest.body["credentials"][0]["id"], json!(key_id.to_string()));
    assert!(rest.body.get("next_cursor").is_none(), "{}", rest.body);
    let (service_id, _) = service(&server, server.data_tenant_id(), "cursor-target").await;
    let foreign = as_bearer(
        &server,
        Method::GET,
        &format!("/v1/principals/{service_id}/credentials?cursor={cursor}"),
        &admin,
        None,
    )
    .await;
    assert_eq!(foreign.status, StatusCode::BAD_REQUEST, "{}", foreign.body);
    let overflow = as_browser(
        &server,
        Method::GET,
        &format!("{keys}?limit=101"),
        &acme,
        &fresh,
        None,
    )
    .await;
    assert_eq!(overflow.status, StatusCode::BAD_REQUEST);

    // More than a full page of keys created at one instant pages exactly once.
    let mut expected: BTreeSet<String> = server
        .seed_user_keys(
            wyrd_runtime::PrincipalId::new(Uuid::parse_str(&user).expect("uuid")),
            105,
        )
        .await
        .expect("keys seed")
        .iter()
        .map(Uuid::to_string)
        .collect();
    let mut seen = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let uri = match &cursor {
            Some(cursor) => format!("{keys}?limit=100&cursor={cursor}"),
            None => format!("{keys}?limit=100"),
        };
        let page = as_browser(&server, Method::GET, &uri, &acme, &fresh, None).await;
        assert_eq!(page.status, StatusCode::OK, "{}", page.body);
        let rows = page.body["credentials"].as_array().expect("credentials");
        assert!(rows.len() <= 100);
        seen.extend(
            rows.iter()
                .map(|row| row["id"].as_str().expect("id").to_owned()),
        );
        match page.body.get("next_cursor").and_then(Value::as_str) {
            Some(next) => cursor = Some(next.to_owned()),
            None => break,
        }
    }
    let unique: BTreeSet<String> = seen.iter().cloned().collect();
    assert_eq!(unique.len(), seen.len(), "a key was listed twice");
    expected.extend(
        first.body["credentials"]
            .as_array()
            .expect("credentials")
            .iter()
            .chain(rest.body["credentials"].as_array().expect("credentials"))
            .map(|row| row["id"].as_str().expect("id").to_owned()),
    );
    assert_eq!(unique, expected);

    // Administrators keep the complete listing and the account shapes.
    let complete = as_bearer(&server, Method::GET, &keys, &admin, None).await;
    assert_eq!(complete.status, StatusCode::OK, "{}", complete.body);
    let listed: BTreeSet<String> = complete.body["credentials"]
        .as_array()
        .expect("credentials")
        .iter()
        .map(|row| row["id"].as_str().expect("id").to_owned())
        .collect();
    assert_eq!(listed, expected);
    let account_keys = format!("/v1/principals/{service_id}/credentials");
    let with_body = as_bearer(&server, Method::POST, &account_keys, &admin, body.clone()).await;
    assert_eq!(
        with_body.status,
        StatusCode::BAD_REQUEST,
        "{}",
        with_body.body
    );
    let plain = as_bearer(&server, Method::POST, &account_keys, &admin, None).await;
    assert_eq!(plain.status, StatusCode::OK, "{}", plain.body);
    assert_no_store(&plain);
    let listed = as_bearer(&server, Method::GET, &account_keys, &admin, None).await;
    assert!(
        listed.body["credentials"][0].get("name").is_none(),
        "{}",
        listed.body
    );
    let for_user = as_bearer(&server, Method::POST, &keys, &admin, body.clone()).await;
    assert_eq!(for_user.status, StatusCode::OK, "{}", for_user.body);
    assert_no_store(&for_user);
    let service_me = as_bearer(&server, Method::GET, "/v1/principals/me", &admin, None).await;
    assert_eq!(service_me.status, StatusCode::FORBIDDEN);

    // Own revocation ends the key; another User's keys stay out of reach.
    let revoked = as_browser(&server, Method::DELETE, &revoke_uri, &acme, &fresh, None).await;
    assert_eq!(revoked.status, StatusCode::NO_CONTENT, "{}", revoked.body);
    assert!(server.exchange_api_key(&key).await.is_err());
    let other = enroll(&server, &admin, "viewer", "neighbor", &strong(2)).await;
    let other = other.cookie_value(&cookie_name(&acme));
    let probe = as_browser(
        &server,
        Method::GET,
        &format!("{keys}?limit=5"),
        &acme,
        &other,
        None,
    )
    .await;
    assert_eq!(probe.status, StatusCode::FORBIDDEN, "{}", probe.body);

    // Suspension ends every key of the User.
    let live = SecretString::from(
        for_user.body["credential"]
            .as_str()
            .expect("key")
            .to_owned(),
    );
    superuser(
        &server,
        "UPDATE wyrd.auth_users SET status = 'suspended' WHERE id = $1",
        Uuid::parse_str(&user).expect("uuid"),
    )
    .await;
    assert!(server.exchange_api_key(&live).await.is_err());
}
