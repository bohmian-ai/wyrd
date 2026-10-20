//! Rust-owned opaque browser sessions through `/auth/browser`.
//!
//! Drives recovery, session resolution, and logout against the in-process
//! server and the repository-managed Postgres with real API keys: the stored
//! row holds only the cookie's digest and a Postgres-owned twelve-hour
//! absolute expiry; tampered, other-tenant, mislabeled, expired, revoked and
//! inactive-authority sessions all refuse; logout ends only its own session;
//! and a store outage is unavailable rather than any fallback.

use axum::body::{Body, to_bytes};
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use secrecy::{ExposeSecret as _, SecretString};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use uuid::Uuid;
use wyrd_testing::{Bootstrap, WyrdTestServer};

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
/// Panics when the request cannot be built, the route fails to respond, or a
/// non-empty body is not JSON.
async fn send(server: &WyrdTestServer, request: BrowserRequest<'_>) -> BrowserResponse {
    let mut builder = Request::builder()
        .method(request.method)
        .uri(request.uri)
        .header(header::HOST, "wyrd.test");
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
        serde_json::from_slice(&bytes).expect("response body is JSON")
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
/// Panics when bootstrapping fails or returns a user.
async fn service(
    server: &WyrdTestServer,
    tenant: wyrd_spec::DataTenantId,
    name: &str,
) -> (Uuid, SecretString) {
    match server
        .bootstrap_service_in_tenant(tenant, name, &["admin"])
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
            .map(|v| v.as_bytes()),
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
    superuser(
        &server,
        "UPDATE platform.tenants SET status = 'suspended' WHERE data_tenant_id = $1",
        beta_id.as_uuid(),
    )
    .await;
    assert_eq!(
        read(&server, "", beta_cookie).await.status,
        StatusCode::UNAUTHORIZED
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
