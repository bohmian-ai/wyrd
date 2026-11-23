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

/// Register platform `user` `name` through the API-token channel and give
/// it [`OPERATOR_PASSWORD`]. Returns its principal id.
///
/// # Panics
/// Panics when registration or the password write is refused.
async fn register_password_operator(srv: &WyrdTestServer, api: &str, name: &str) -> String {
    let (status, registered) = social_call(
        srv,
        social_request(
            Method::POST,
            "/platform/admins",
            Some(api),
            Some(json!({ "name": name })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{registered}");
    let principal = registered["principal_id"]
        .as_str()
        .expect("principal id")
        .to_owned();
    set_operator_password(srv, api, &principal, OPERATOR_PASSWORD).await;
    principal
}

/// Set platform `user` `principal`'s password through the API-token channel.
///
/// # Panics
/// Panics when the write is refused.
async fn set_operator_password(srv: &WyrdTestServer, api: &str, principal: &str, password: &str) {
    let (status, body) = social_call(
        srv,
        social_request(
            Method::PUT,
            &format!("/platform/admins/{principal}/password"),
            Some(api),
            Some(json!({ "password": password })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
}

/// Sign `name` in with `password` and return the platform cookie pair.
///
/// # Panics
/// Panics when sign-in is refused or sets no cookie.
async fn signed_in(srv: &WyrdTestServer, name: &str, password: &str) -> String {
    let reply = password_login(srv, name, password).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    reply.cookie(PLATFORM_COOKIE).expect("platform cookie")
}

/// The status of `GET /platform/browser/session` with `cookie` on `srv`.
async fn session_status(srv: &WyrdTestServer, cookie: &str) -> StatusCode {
    browser(
        srv,
        Method::GET,
        "/platform/browser/session",
        &[cookie],
        false,
        None,
    )
    .await
    .status
}

/// Move every timestamp of the session `cookie` names `elapsed` into the
/// past, as if that much time had passed since its last request.
///
/// # Panics
/// Panics when the update fails or matches no session.
async fn age_session(srv: &WyrdTestServer, cookie: &str, elapsed: &str) {
    let value = cookie.split_once('=').expect("cookie pair").1;
    let aged = sqlx::query(
        "UPDATE platform.browser_sessions
            SET created_at = created_at - $2::interval,
                last_active_at = last_active_at - $2::interval,
                fresh_at = fresh_at - $2::interval,
                idle_expires_at = idle_expires_at - $2::interval,
                expires_at = expires_at - $2::interval
          WHERE session_hash = sha256(convert_to($1, 'UTF8'))",
    )
    .bind(value)
    .bind(elapsed)
    .execute(srv.operator_pool().pool())
    .await
    .expect("session ages");
    assert_eq!(aged.rows_affected(), 1, "the cookie names one session");
}

/// Platform console sessions end everywhere under the same rules:
///   1. a session signed in on one replica serves the other, and ends on
///      both 15 minutes after its last request;
///   2. activity moves the idle deadline but never past the eight-hour
///      absolute one, after which the session ends;
///   3. logout on one replica ends the session on the other;
///   4. suspension ends the operator's sessions, and reactivation does not
///      revive them;
///   5. a password change ends sessions from the earlier password, and only
///      the new password signs in;
///   6. when the session store cannot be written, a platform cookie is
///      refused as unavailable, never accepted or read as signed out.
///
/// # Panics
/// Panics when any session outlives its rule or the store failure reads
/// differently.
#[tokio::test(flavor = "multi_thread")]
async fn platform_console_sessions_end_on_every_replica() {
    let (srv, _root, api) = console_server().await;
    let replica = srv
        .start_replica(
            WyrdTestServer::builder().with_trusted_ingress_for_test(&[INGRESS], CLIENT_HEADER),
        )
        .await
        .expect("replica starts");
    let principal = register_password_operator(&srv, &api, "ops").await;

    // 1. Idle expiry, observed on both replicas.
    let idle = signed_in(&srv, "ops", OPERATOR_PASSWORD).await;
    assert_eq!(session_status(&replica, &idle).await, StatusCode::OK);
    age_session(&srv, &idle, "14 minutes").await;
    assert_eq!(
        session_status(&replica, &idle).await,
        StatusCode::OK,
        "activity within the window"
    );
    age_session(&srv, &idle, "16 minutes").await;
    assert_eq!(
        session_status(&replica, &idle).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(session_status(&srv, &idle).await, StatusCode::UNAUTHORIZED);

    // 2. Activity cannot extend the absolute deadline.
    let long = signed_in(&srv, "ops", OPERATOR_PASSWORD).await;
    // Seven hours 59 minutes since sign-in, the last request just now.
    let value = long.split_once('=').expect("cookie pair").1;
    sqlx::query(
        "UPDATE platform.browser_sessions
            SET created_at = created_at - interval '7 hours 59 minutes',
                expires_at = expires_at - interval '7 hours 59 minutes',
                idle_expires_at = LEAST(idle_expires_at,
                                        expires_at - interval '7 hours 59 minutes')
          WHERE session_hash = sha256(convert_to($1, 'UTF8'))",
    )
    .bind(value)
    .execute(srv.operator_pool().pool())
    .await
    .expect("session ages");
    let view = browser(
        &replica,
        Method::GET,
        "/platform/browser/session",
        &[&long],
        false,
        None,
    )
    .await;
    assert_eq!(view.status, StatusCode::OK, "{}", view.body);
    assert_eq!(
        view.body["idle_expires_at"], view.body["expires_at"],
        "activity stops at the absolute deadline: {}",
        view.body
    );
    age_session(&srv, &long, "2 minutes").await;
    assert_eq!(
        session_status(&replica, &long).await,
        StatusCode::UNAUTHORIZED
    );

    // 3. Logout on one replica ends the session on the other.
    let left = signed_in(&replica, "ops", OPERATOR_PASSWORD).await;
    let reply = browser(
        &replica,
        Method::POST,
        "/platform/browser/logout",
        &[&left],
        true,
        None,
    )
    .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{}", reply.body);
    assert_eq!(session_status(&srv, &left).await, StatusCode::UNAUTHORIZED);

    // 4. Suspension ends the session for good.
    let suspended = signed_in(&srv, "ops", OPERATOR_PASSWORD).await;
    for status in ["suspended", "active"] {
        let (code, body) = social_call(
            &srv,
            social_request(
                Method::PUT,
                &format!("/platform/admins/{principal}/status"),
                Some(&api),
                Some(json!({ "status": status })),
            ),
        )
        .await;
        assert_eq!(code, StatusCode::NO_CONTENT, "{body}");
        assert_eq!(
            session_status(&replica, &suspended).await,
            StatusCode::UNAUTHORIZED,
            "after {status}"
        );
    }

    // 5. A password change ends sessions from the earlier password.
    let before = signed_in(&srv, "ops", OPERATOR_PASSWORD).await;
    set_operator_password(&srv, &api, &principal, "copper finch 2026").await;
    assert_eq!(
        session_status(&replica, &before).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        password_login(&replica, "ops", OPERATOR_PASSWORD)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    let after = signed_in(&replica, "ops", "copper finch 2026").await;

    // 6. An unwritable store refuses rather than accepts or signs out.
    let pool = srv.operator_pool();
    super::fail_writes_to(
        pool.pool(),
        "console",
        "platform.browser_sessions",
        "UPDATE",
    )
    .await;
    assert_eq!(
        session_status(&replica, &after).await,
        StatusCode::SERVICE_UNAVAILABLE
    );
    let reply = browser(
        &srv,
        Method::GET,
        "/platform/tenants",
        &[&after],
        false,
        None,
    )
    .await;
    assert_eq!(
        reply.status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{}",
        reply.body
    );
    super::stop_failing_writes(pool.pool(), "console", "platform.browser_sessions").await;
    assert_eq!(session_status(&replica, &after).await, StatusCode::OK);
}

/// The platform login-binding cookie's name.
const LOGIN_COOKIE: &str = "wyrd_platform_login";

/// A begun platform browser OIDC login: its binding cookie pair, its state,
/// the nonce the provider must echo and the authorization URL.
struct BegunOidc {
    /// The `wyrd_platform_login` cookie pair.
    binding: String,
    /// The opaque state the provider returns.
    state: String,
    /// The nonce the ID token must carry.
    nonce: String,
    /// The provider authorization URL.
    url: url::Url,
}

/// Begin a platform browser OIDC login from this deployment's origin.
///
/// # Panics
/// Panics when begin is refused or sets no binding cookie.
async fn begin_oidc(srv: &WyrdTestServer) -> BegunOidc {
    let reply = browser(
        srv,
        Method::POST,
        "/platform/browser/oidc/login",
        &[],
        true,
        None,
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    let set = reply
        .set_cookies
        .iter()
        .find(|value| value.starts_with(&format!("{LOGIN_COOKIE}=")))
        .expect("a login binding cookie");
    for attribute in [
        "; Path=/platform;",
        "; HttpOnly",
        "; SameSite=Lax",
        "; Secure",
    ] {
        assert!(set.contains(attribute), "{attribute} is set: {set}");
    }
    let url = url::Url::parse(
        reply.body["authorization_url"]
            .as_str()
            .expect("authorization url"),
    )
    .expect("authorization url parses");
    let query = |name: &str| {
        url.query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
            .expect("authorization parameter")
    };
    BegunOidc {
        binding: reply.cookie(LOGIN_COOKIE).expect("binding pair"),
        state: query("state"),
        nonce: query("nonce"),
        url: url.clone(),
    }
}

/// Present the provider's return for `state` to the platform browser
/// callback with `cookies`.
async fn oidc_callback(
    srv: &WyrdTestServer,
    cookies: &[&str],
    state: &str,
    issuer: &str,
) -> BrowserReply {
    browser(
        srv,
        Method::POST,
        "/platform/browser/oidc/callback",
        cookies,
        true,
        Some(&json!({ "code": "provider-code", "state": state, "iss": issuer })),
    )
    .await
}

/// Configure the platform OIDC connection to `issuer` as a public client.
///
/// # Panics
/// Panics when configuration is refused.
async fn configure_platform_oidc(srv: &WyrdTestServer, api: &str, issuer: &str) {
    let resp = srv
        .oneshot(super::platform_request(
            Method::PUT,
            "/platform/oidc/connection",
            api,
            Some(json!({
                "issuer_url": issuer,
                "client_id": super::PLATFORM_CLIENT,
                "client_auth": { "method": "public" },
            })),
        ))
        .await
        .expect("configure route responds");
    assert_eq!(resp.status(), StatusCode::OK, "connection configures");
}

/// A pre-registered platform operator signs in to the platform console
/// through the platform OIDC connection:
///   1. begin requires this deployment's Origin, binds the login to this
///      browser with a platform-scoped cookie, and returns to the fixed
///      console callback;
///   2. a return without that browser's binding creates no session;
///   3. an authenticated but unregistered subject creates no session;
///   4. the registered subject gets an opaque `oidc` session whose metadata
///      and cookies carry no provider token;
///   5. the same return cannot be replayed;
///   6. an API-token login's state cannot finish a browser login, nor the
///      reverse;
///   7. replacing the connection ends the sessions it created, and the
///      pinned subject signs in again.
///
/// # Panics
/// Panics when any step signs in, refuses or reveals differently.
#[tokio::test(flavor = "multi_thread")]
async fn a_registered_operator_signs_in_to_the_console_through_platform_oidc() {
    let (srv, _root, api) = console_server().await;
    let provider = wiremock::MockServer::start().await;
    let issuer = provider.uri();
    let mount =
        |reply| super::mount_platform_provider(&provider, super::platform_key_set("mock-1"), reply);
    mount(wiremock::ResponseTemplate::new(500)).await;
    configure_platform_oidc(&srv, &api, &issuer).await;
    let (status, body) = social_call(
        &srv,
        social_request(
            Method::POST,
            "/platform/admins",
            Some(&api),
            Some(json!({ "name": "ops-lead", "match_claim": "ops@example.com" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // 1. Begin is same-origin, browser-bound and returns to the console.
    let reply = browser(
        &srv,
        Method::POST,
        "/platform/browser/oidc/login",
        &[],
        false,
        None,
    )
    .await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN, "{}", reply.body);
    let begun = begin_oidc(&srv).await;
    let redirect = begun
        .url
        .query_pairs()
        .find(|(key, _)| key == "redirect_uri")
        .map(|(_, value)| value.into_owned());
    assert_eq!(
        redirect.as_deref(),
        Some("http://wyrd.test/platform/ui/callback")
    );

    // 2. Another browser's return, or one with no binding, is refused.
    mount(super::platform_token_reply(
        &issuer,
        &begun.nonce,
        super::PLATFORM_CLIENT,
        "mock-1",
    ))
    .await;
    let other = begin_oidc(&srv).await;
    for cookies in [vec![], vec![other.binding.as_str()]] {
        let reply = oidc_callback(&srv, &cookies, &begun.state, &issuer).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{}", reply.body);
        assert!(reply.cookie(PLATFORM_COOKIE).is_none());
    }

    // 3. An unregistered subject gets nothing.
    mount(super::identity_token_reply(
        &issuer,
        &other.nonce,
        super::PLATFORM_CLIENT,
        "mock-1",
        "provider-subject-9",
        "stranger@example.com",
    ))
    .await;
    let reply = oidc_callback(&srv, &[&other.binding], &other.state, &issuer).await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{}", reply.body);
    assert!(reply.cookie(PLATFORM_COOKIE).is_none());

    // 4. The registered subject signs in to an oidc session.
    mount(super::platform_token_reply(
        &issuer,
        &begun.nonce,
        super::PLATFORM_CLIENT,
        "mock-1",
    ))
    .await;
    let reply = oidc_callback(&srv, &[&begun.binding], &begun.state, &issuer).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(reply.body["method"], "oidc");
    assert_eq!(reply.body["name"], "ops-lead");
    let revealed = format!("{} {:?}", reply.body, reply.set_cookies);
    assert!(
        !revealed.contains("provider-access") && !revealed.contains("eyJ"),
        "{revealed}"
    );
    let session = reply.cookie(PLATFORM_COOKIE).expect("platform cookie");
    let tenants = browser(
        &srv,
        Method::GET,
        "/platform/tenants",
        &[&session],
        false,
        None,
    )
    .await;
    assert_eq!(tenants.status, StatusCode::OK, "{}", tenants.body);

    // 5. The return is single-use.
    let replay = oidc_callback(&srv, &[&begun.binding], &begun.state, &issuer).await;
    assert_eq!(replay.status, StatusCode::UNAUTHORIZED, "{}", replay.body);

    // 6. API-token and browser logins cannot finish each other.
    let (token_state, token_nonce) = super::begin_platform_login(&srv).await;
    mount(super::platform_token_reply(
        &issuer,
        &token_nonce,
        super::PLATFORM_CLIENT,
        "mock-1",
    ))
    .await;
    let fresh = begin_oidc(&srv).await;
    let reply = oidc_callback(&srv, &[&fresh.binding], &token_state, &issuer).await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{}", reply.body);
    let (status, body) = super::platform_callback(&srv, &fresh.state, Some(&issuer)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");

    // 7. Replacing the connection ends the sessions it created.
    configure_platform_oidc(&srv, &api, &issuer).await;
    assert_eq!(
        session_status(&srv, &session).await,
        StatusCode::UNAUTHORIZED
    );
    let again = begin_oidc(&srv).await;
    mount(super::platform_token_reply(
        &issuer,
        &again.nonce,
        super::PLATFORM_CLIENT,
        "mock-1",
    ))
    .await;
    let reply = oidc_callback(&srv, &[&again.binding], &again.state, &issuer).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
}
