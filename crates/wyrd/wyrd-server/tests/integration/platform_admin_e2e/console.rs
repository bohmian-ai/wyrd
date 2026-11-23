//! Platform console journeys: a registered platform operator signs in to an
//! isolated platform browser session, against a real server and repository
//! Postgres.

use std::net::{IpAddr, SocketAddr};

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use serde_json::{Value, json};
use wyrd_testing::WyrdTestServer;

use super::{SOCIAL_HOST, initialize_platform_root, platform_session, social_call, social_request};

/// The trusted ingress every journey here sits behind.
const INGRESS: &str = "10.0.0.0/8";

/// The header that ingress names the visitor in.
const CLIENT_HEADER: &str = "CF-Connecting-IP";

/// The platform operator's password: fifteen characters, not blocklisted.
const OPERATOR_PASSWORD: &str = "quartz heron 15";

/// The platform browser-session cookie's name.
const PLATFORM_COOKIE: &str = "wyrd_platform_session";

/// A browser response: status, every `Set-Cookie` header and the JSON body
/// (`Null` when there is none).
struct BrowserReply {
    /// The response status.
    status: StatusCode,
    /// Every `Set-Cookie` header, verbatim.
    set_cookies: Vec<String>,
    /// The decoded body.
    body: Value,
}

impl BrowserReply {
    /// The `name=value` pair of the `Set-Cookie` named `name`, when set.
    fn cookie(&self, name: &str) -> Option<String> {
        let prefix = format!("{name}=");
        self.set_cookies
            .iter()
            .find(|value| value.starts_with(&prefix))
            .and_then(|value| value.split(';').next())
            .map(str::to_owned)
    }
}

/// A browser request to `uri` through the ingress at [`SOCIAL_HOST`],
/// carrying `cookies`, this deployment's `Origin` when `same_origin`, and
/// `body` as JSON when given.
async fn browser(
    srv: &WyrdTestServer,
    method: Method,
    uri: &str,
    cookies: &[&str],
    same_origin: bool,
    body: Option<&Value>,
) -> BrowserReply {
    let peer: IpAddr = "10.7.0.1".parse().expect("peer address");
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, SOCIAL_HOST)
        .header(CLIENT_HEADER, "203.0.113.9")
        .extension(axum::extract::ConnectInfo(SocketAddr::new(peer, 40_000)));
    if same_origin {
        builder = builder.header(header::ORIGIN, format!("http://{SOCIAL_HOST}"));
    }
    if !cookies.is_empty() {
        builder = builder.header(header::COOKIE, cookies.join("; "));
    }
    let request = match body {
        Some(body) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(body).expect("serializes"))),
        None => builder.body(Body::empty()),
    }
    .expect("request builds");
    let resp = srv.oneshot(request).await.expect("route responds");
    let status = resp.status();
    let set_cookies = resp
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok().map(str::to_owned))
        .collect();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
        .await
        .expect("read body");
    BrowserReply {
        status,
        set_cookies,
        body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    }
}

/// A server behind [`INGRESS`], its root credential and that credential's
/// platform API session.
///
/// # Panics
/// Panics when the server or the root session cannot be set up.
async fn console_server() -> (WyrdTestServer, String, String) {
    let srv = WyrdTestServer::builder()
        .with_trusted_ingress_for_test(&[INGRESS], CLIENT_HEADER)
        .start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let root = secrecy::ExposeSecret::expose_secret(&root).to_owned();
    let session = platform_session(&srv, &root).await;
    (srv, root, session)
}

/// Sign `name` in to the platform console with `password`.
async fn password_login(srv: &WyrdTestServer, name: &str, password: &str) -> BrowserReply {
    browser(
        srv,
        Method::POST,
        "/platform/browser/password/login",
        &[],
        true,
        Some(&json!({ "username": name, "password": password })),
    )
    .await
}

/// A registered platform operator signs in with a platform-user password
/// and holds an isolated platform browser session:
///   1. there is no anonymous enrollment; an authorized operator registers a
///      platform `user` without an IdP, and its password follows the
///      15-character and blocklist policy;
///   2. a wrong password, an unknown name, the bootstrap key, or a missing
///      Origin creates no session;
///   3. the right password sets a regenerated opaque host-only Secure,
///      HttpOnly, SameSite=Lax cookie on `/platform`, and neither it nor the
///      session metadata carries an API token or the bootstrap key;
///   4. the cookie authorizes platform reads, and its mutations require
///      this deployment's Origin;
///   5. a tenant cookie cannot reach the platform, and a replayed platform
///      cookie reaches no tenant route nor tenant session enumeration;
///   6. logout ends only the selected session, and the API-token consumer
///      keeps working.
///
/// # Panics
/// Panics when any step signs in, authorizes or reveals differently.
#[tokio::test(flavor = "multi_thread")]
async fn a_registered_operator_signs_in_with_a_platform_password() {
    let (srv, root, api) = console_server().await;

    // 1. Registration needs platform authority, and the password its policy.
    let (status, _) = social_call(
        &srv,
        social_request(
            Method::POST,
            "/platform/admins",
            None,
            Some(json!({ "name": "stranger" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "no anonymous enrollment");
    let (status, registered) = social_call(
        &srv,
        social_request(
            Method::POST,
            "/platform/admins",
            Some(&api),
            Some(json!({ "name": "ops" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{registered}");
    let principal = registered["principal_id"].as_str().expect("principal id");
    for refused in ["fourteen chars", "!qazxsw2#edcvfr4"] {
        let (status, body) = social_call(
            &srv,
            social_request(
                Method::PUT,
                &format!("/platform/admins/{principal}/password"),
                Some(&api),
                Some(json!({ "password": refused })),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    }
    let (status, body) = social_call(
        &srv,
        social_request(
            Method::PUT,
            &format!("/platform/admins/{principal}/password"),
            Some(&api),
            Some(json!({ "password": OPERATOR_PASSWORD })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");

    // 2. Nothing but the right password, from this origin, signs in.
    for (name, password) in [
        ("ops", "quartz heron 16"),
        ("nobody", OPERATOR_PASSWORD),
        ("ops", root.as_str()),
    ] {
        let reply = password_login(&srv, name, password).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{}", reply.body);
        assert!(reply.cookie(PLATFORM_COOKIE).is_none());
    }
    let reply = browser(
        &srv,
        Method::POST,
        "/platform/browser/password/login",
        &[],
        false,
        Some(&json!({ "username": "ops", "password": OPERATOR_PASSWORD })),
    )
    .await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN, "{}", reply.body);
    assert!(reply.cookie(PLATFORM_COOKIE).is_none());

    // 3. The session cookie is opaque, regenerated and platform-scoped.
    let first = password_login(&srv, "ops", OPERATOR_PASSWORD).await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.body);
    let set = first
        .set_cookies
        .iter()
        .find(|value| value.starts_with(&format!("{PLATFORM_COOKIE}=")))
        .expect("a platform session cookie");
    for attribute in [
        "; Path=/platform;",
        "; HttpOnly",
        "; SameSite=Lax",
        "; Secure",
    ] {
        assert!(set.contains(attribute), "{attribute} is set: {set}");
    }
    assert!(!set.contains("Domain"), "the cookie is host-only: {set}");
    let cookie = first.cookie(PLATFORM_COOKIE).expect("cookie pair");
    let value = cookie.split_once('=').expect("pair").1;
    assert!(
        value.len() >= 32 && !value.contains('.'),
        "the cookie is an opaque secret, not a token"
    );
    let text = first.body.to_string();
    assert!(!text.contains(value) && !text.contains(&root) && !text.contains("token"));
    assert_eq!(first.body["name"], "ops");
    assert_eq!(first.body["method"], "password");
    let second = password_login(&srv, "ops", OPERATOR_PASSWORD).await;
    let other = second.cookie(PLATFORM_COOKIE).expect("a second session");
    assert_ne!(other, cookie, "every sign-in regenerates the session");
    let read = browser(
        &srv,
        Method::GET,
        "/platform/browser/session",
        &[&cookie],
        false,
        None,
    )
    .await;
    assert_eq!(read.status, StatusCode::OK, "{}", read.body);
    assert_eq!(read.body["principal_id"], principal);
    assert!(!read.body.to_string().contains(value));

    // 4. The cookie reads the platform; its mutations need this origin.
    let tenants = browser(
        &srv,
        Method::GET,
        "/platform/tenants",
        &[&cookie],
        false,
        None,
    )
    .await;
    assert_eq!(tenants.status, StatusCode::OK, "{}", tenants.body);
    let create = json!({ "slug": "harbor", "display_name": "Harbor",
        "human_admin": { "username": "admin", "password": "amber kestrel lantern" } });
    let foreign = browser(
        &srv,
        Method::POST,
        "/platform/tenants",
        &[&cookie],
        false,
        Some(&create),
    )
    .await;
    assert_eq!(foreign.status, StatusCode::FORBIDDEN, "{}", foreign.body);
    let created = browser(
        &srv,
        Method::POST,
        "/platform/tenants",
        &[&cookie],
        true,
        Some(&create),
    )
    .await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.body);

    // 5. Neither plane accepts the other's cookie.
    let tenant_login = browser(
        &srv,
        Method::POST,
        "/auth/browser/password/login",
        &[],
        true,
        Some(&json!({ "tenant": "harbor", "username": "admin",
            "password": "amber kestrel lantern" })),
    )
    .await;
    assert_eq!(tenant_login.status, StatusCode::OK, "{}", tenant_login.body);
    let tenant_cookie = tenant_login
        .cookie("wyrd_session_harbor")
        .expect("tenant session");
    for uri in ["/platform/tenants", "/platform/browser/session"] {
        let refused = browser(&srv, Method::GET, uri, &[&tenant_cookie], false, None).await;
        assert_eq!(
            refused.status,
            StatusCode::UNAUTHORIZED,
            "{uri}: {}",
            refused.body
        );
    }
    for uri in ["/v1/principals/me", "/auth/browser/session"] {
        let refused = browser(&srv, Method::GET, uri, &[&cookie], false, None).await;
        assert_eq!(
            refused.status,
            StatusCode::UNAUTHORIZED,
            "{uri}: {}",
            refused.body
        );
    }
    let renamed = format!("wyrd_session_harbor={value}");
    let refused = browser(
        &srv,
        Method::GET,
        "/v1/principals/me",
        &[&renamed],
        false,
        None,
    )
    .await;
    assert_eq!(refused.status, StatusCode::UNAUTHORIZED, "{}", refused.body);

    // 6. Logout ends only this session; API tokens are untouched.
    let out = browser(
        &srv,
        Method::POST,
        "/platform/browser/logout",
        &[&cookie],
        true,
        None,
    )
    .await;
    assert_eq!(out.status, StatusCode::NO_CONTENT, "{}", out.body);
    let cleared = out
        .set_cookies
        .iter()
        .find(|value| value.starts_with(&format!("{PLATFORM_COOKIE}=;")))
        .expect("the cookie is cleared");
    assert!(cleared.contains("Max-Age=0"), "{cleared}");
    let gone = browser(
        &srv,
        Method::GET,
        "/platform/browser/session",
        &[&cookie],
        false,
        None,
    )
    .await;
    assert_eq!(gone.status, StatusCode::UNAUTHORIZED);
    let kept = browser(
        &srv,
        Method::GET,
        "/platform/browser/session",
        &[&other],
        false,
        None,
    )
    .await;
    assert_eq!(kept.status, StatusCode::OK, "{}", kept.body);
    let (status, _) = social_call(
        &srv,
        social_request(Method::GET, "/platform/tenants", Some(&api), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the API-token consumer still works");
}
