//! Verified-contact journeys: a signed-in User proves a recovery address
//! by email and replaces it, against a real server, repository Postgres and
//! Mailpit.

use std::net::{IpAddr, SocketAddr};

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use serde_json::{Value, json};
use wyrd_testing::WyrdTestServer;

use super::{
    SOCIAL_HOST, initialize_platform_root, mail_to, mailpit, platform_session, session_cookie,
    social_call, social_request,
};

/// The trusted ingress every journey here sits behind.
const INGRESS: &str = "10.0.0.0/8";

/// The header that ingress names the visitor in.
const CLIENT_HEADER: &str = "CF-Connecting-IP";

/// The administrator's password.
const PASSWORD: &str = "amber kestrel lantern";

/// A server behind [`INGRESS`] with tenant `slug`, whose `admin` signs in
/// with [`PASSWORD`], and its operator session.
///
/// # Panics
/// Panics when the server, operator session or tenant cannot be set up.
async fn contact_server(slug: &str) -> (WyrdTestServer, String) {
    let srv = WyrdTestServer::builder()
        .with_trusted_ingress_for_test(&[INGRESS], CLIENT_HEADER)
        .start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;
    let (status, created) = social_call(
        &srv,
        social_request(
            Method::POST,
            "/platform/tenants",
            Some(&session),
            Some(json!({
                "slug": slug,
                "display_name": slug,
                "human_admin": { "username": "admin", "password": PASSWORD },
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    (srv, session)
}

/// Point the deployment's mail relay at the test Mailpit; returns its API.
async fn relay_to_mailpit(srv: &WyrdTestServer, session: &str) -> String {
    let (host, port, api) = mailpit();
    let (status, relay) = social_call(
        srv,
        social_request(
            Method::PUT,
            "/platform/mail",
            Some(session),
            Some(json!({ "host": host, "port": port, "tls": "none", "sender": "accounts@wyrd.test" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{relay}");
    api
}

/// The visitor address the browser helpers present unless told otherwise.
const VISITOR: &str = "203.0.113.9";

/// One request body: JSON, or an urlencoded form.
enum Payload<'a> {
    /// No body.
    Empty,
    /// A JSON body.
    Json(&'a Value),
    /// An `application/x-www-form-urlencoded` body.
    Form(String),
}

/// A same-origin browser request to `uri` through the ingress from the
/// visitor `client`, carrying `cookies`; returns the status, every
/// `Set-Cookie` value and the body (`null` when it is not JSON).
async fn ingress(
    srv: &WyrdTestServer,
    method: Method,
    uri: &str,
    client: &str,
    cookies: &[&str],
    payload: Payload<'_>,
) -> (StatusCode, Vec<String>, Value) {
    let peer: IpAddr = "10.7.0.1".parse().expect("peer address");
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, SOCIAL_HOST)
        .header(header::ORIGIN, format!("http://{SOCIAL_HOST}"))
        .header(CLIENT_HEADER, client)
        .extension(axum::extract::ConnectInfo(SocketAddr::new(peer, 40_000)));
    if !cookies.is_empty() {
        builder = builder.header(header::COOKIE, cookies.join("; "));
    }
    let body = match payload {
        Payload::Empty => Body::empty(),
        Payload::Json(body) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(serde_json::to_vec(body).expect("serializes"))
        }
        Payload::Form(form) => {
            builder = builder.header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
            Body::from(form)
        }
    };
    let resp = srv
        .oneshot(builder.body(body).expect("request builds"))
        .await
        .expect("route responds");
    let status = resp.status();
    let cookies = resp
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .map(str::to_owned)
        .collect();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
        .await
        .expect("read body");
    (
        status,
        cookies,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// A same-origin browser `POST` of `body` to `uri` through the ingress,
/// carrying `cookies`; returns the status, the value of the `Set-Cookie`
/// named `set`, and the body.
async fn browser(
    srv: &WyrdTestServer,
    uri: &str,
    cookies: &[&str],
    set: &str,
    body: &Value,
) -> (StatusCode, Option<String>, Value) {
    let (status, set_cookies, body) = ingress(
        srv,
        Method::POST,
        uri,
        VISITOR,
        cookies,
        Payload::Json(body),
    )
    .await;
    let prefix = format!("{set}=");
    let cookie = set_cookies
        .iter()
        .find(|value| value.starts_with(&prefix))
        .and_then(|value| value.split(';').next())
        .map(str::to_owned);
    (status, cookie, body)
}

/// Sign `admin` in to `slug` with [`PASSWORD`]; returns its session
/// cookie pair.
async fn sign_in(srv: &WyrdTestServer, slug: &str) -> String {
    sign_in_as(srv, slug, "admin", PASSWORD)
        .await
        .expect("a password session")
}

/// Sign `username` in to `slug` with `password`; returns its session
/// cookie pair, or the refusal's status.
async fn sign_in_as(
    srv: &WyrdTestServer,
    slug: &str,
    username: &str,
    password: &str,
) -> Result<String, StatusCode> {
    let resp = srv
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/browser/password/login")
                .header(header::HOST, SOCIAL_HOST)
                .header(header::ORIGIN, format!("http://{SOCIAL_HOST}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(CLIENT_HEADER, VISITOR)
                .extension(axum::extract::ConnectInfo(SocketAddr::new(
                    "10.7.0.1".parse().expect("peer"),
                    40_000,
                )))
                .body(Body::from(
                    json!({ "tenant": slug, "username": username, "password": password })
                        .to_string(),
                ))
                .expect("request builds"),
        )
        .await
        .expect("login responds");
    if resp.status() != StatusCode::OK {
        return Err(resp.status());
    }
    Ok(session_cookie(&resp, slug).expect("a password session"))
}

/// The proof the newest message to `address` carries in its link fragment,
/// and that message's text.
async fn mailed_proof(api: &str, address: &str) -> (String, String) {
    let mail = mail_to(api, address).await;
    let (_, _, _, text) = mail.first().expect("a message arrived");
    let proof = text
        .split("#proof=")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .expect("the message links its proof in a fragment")
        .to_owned();
    (proof, text.clone())
}

/// `slug`'s verified contacts: address and revision.
async fn contacts(superuser: &sqlx::PgPool, slug: &str) -> Vec<(String, i64)> {
    sqlx::query_as(
        "SELECT c.address, c.revision FROM wyrd.auth_user_contacts c
           JOIN platform.tenants t ON t.data_tenant_id = c.data_tenant_id
          WHERE t.slug = $1",
    )
    .bind(slug)
    .fetch_all(superuser)
    .await
    .expect("contacts read")
}

/// A signed-in User proves and replaces a verified recovery contact:
///   1. without a mail relay, verification is not offered and nothing is
///      mailed;
///   2. without a session nothing starts; a sign-in older than five minutes
///      must sign in again;
///   3. a fresh password session starts verification, which mails the
///      address a fragment link and sets a flow cookie; a wrong proof, or
///      the proof without that cookie, verifies nothing;
///   4. the right proof makes the address the contact at revision 1, and the
///      same browser repeating it, as after a lost response, changes nothing;
///   5. replacing it mails a proof to the new address; a flow begun before
///      the replacement no longer verifies, the old address is told of the
///      change, and the password still signs in.
///
/// # Panics
/// Panics when a step verifies, stores or mails differently.
#[tokio::test(flavor = "multi_thread")]
async fn a_signed_in_user_verifies_and_replaces_a_recovery_contact() {
    let (srv, operator) = contact_server("harbor").await;
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    let first = "ada.contact@example.test";
    let second = "bea.contact@example.test";
    let abandoned = "cy.contact@example.test";
    let request = |email: &str| json!({ "tenant": "harbor", "email": email });
    let session = sign_in(&srv, "harbor").await;

    // 1. No relay, no verification.
    let (status, _, body) = browser(
        &srv,
        "/auth/browser/email/request",
        &[&session],
        "wyrd_contact",
        &request(first),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "WYRD_AUTH_403_METHOD_UNAVAILABLE", "{body}");
    let api = relay_to_mailpit(&srv, &operator).await;
    assert!(mail_to(&api, first).await.is_empty(), "nothing was mailed");

    // 2. A session, and a recent one, are required.
    let (status, _, _) = browser(
        &srv,
        "/auth/browser/email/request",
        &[],
        "wyrd_contact",
        &request(first),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    sqlx::query(
        "UPDATE wyrd.auth_browser_sessions
            SET created_at = created_at - interval '10 minutes',
                expires_at = expires_at - interval '10 minutes'",
    )
    .execute(&superuser)
    .await
    .expect("sessions age");
    let (status, _, body) = browser(
        &srv,
        "/auth/browser/email/request",
        &[&session],
        "wyrd_contact",
        &request(first),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(
        body["code"], "WYRD_AUTH_403_RECENT_LOGIN_REQUIRED",
        "{body}"
    );
    assert!(mail_to(&api, first).await.is_empty(), "nothing was mailed");

    // 3. A fresh session starts it; only the right proof in this browser counts.
    let session = sign_in(&srv, "harbor").await;
    let (status, flow, body) = browser(
        &srv,
        "/auth/browser/email/request",
        &[&session],
        "wyrd_contact",
        &request(first),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert_eq!(body["status"], "check_email");
    let flow = flow.expect("a flow cookie");
    let (proof, _) = mailed_proof(&api, first).await;
    assert!(!flow.contains(&proof), "the cookie never holds the proof");
    let verify = |proof: &str| json!({ "tenant": "harbor", "proof": proof });
    let (status, _, _) = browser(
        &srv,
        "/auth/browser/email/verify",
        &[&session, &flow],
        "wyrd_contact",
        &verify("not-the-proof"),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _, _) = browser(
        &srv,
        "/auth/browser/email/verify",
        &[&session],
        "wyrd_contact",
        &verify(&proof),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(contacts(&superuser, "harbor").await.is_empty());

    // 4. The right proof verifies once; repeating it changes nothing.
    for _ in 0..2 {
        let (status, _, body) = browser(
            &srv,
            "/auth/browser/email/verify",
            &[&session, &flow],
            "wyrd_contact",
            &verify(&proof),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
        assert_eq!(
            contacts(&superuser, "harbor").await,
            vec![(first.to_owned(), 1)]
        );
    }

    // 5. Replacement advances the revision and tells the old address.
    let (status, stale_flow, _) = browser(
        &srv,
        "/auth/browser/email/request",
        &[&session],
        "wyrd_contact",
        &request(abandoned),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let (stale_proof, _) = mailed_proof(&api, abandoned).await;
    let (status, flow, _) = browser(
        &srv,
        "/auth/browser/email/request",
        &[&session],
        "wyrd_contact",
        &request(second),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let (proof, _) = mailed_proof(&api, second).await;
    let (status, _, body) = browser(
        &srv,
        "/auth/browser/email/verify",
        &[&session, &flow.expect("a flow cookie")],
        "wyrd_contact",
        &verify(&proof),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    assert_eq!(
        contacts(&superuser, "harbor").await,
        vec![(second.to_owned(), 2)]
    );
    let (status, _, _) = browser(
        &srv,
        "/auth/browser/email/verify",
        &[&session, &stale_flow.expect("a flow cookie")],
        "wyrd_contact",
        &verify(&stale_proof),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "a stale flow verifies nothing"
    );
    assert_eq!(
        contacts(&superuser, "harbor").await,
        vec![(second.to_owned(), 2)]
    );
    let notices = mail_to(&api, first).await;
    assert_eq!(notices.len(), 2, "the proof, then the change notice");
    assert!(!notices[0].3.contains("#proof="), "{}", notices[0].3);
    assert!(
        !notices[0].3.contains(second),
        "the notice names no new address"
    );
    sign_in(&srv, "harbor").await;
}

/// Make `address` the verified contact of the User signed in by
/// `session` to `slug`, through the mailed proof.
///
/// # Panics
/// Panics when the request, mail or verification fails.
async fn verify_contact(srv: &WyrdTestServer, api: &str, slug: &str, session: &str, address: &str) {
    let (status, flow, body) = browser(
        srv,
        "/auth/browser/email/request",
        &[session],
        "wyrd_contact",
        &json!({ "tenant": slug, "email": address }),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    let (proof, _) = mailed_proof(api, address).await;
    let (status, _, body) = browser(
        srv,
        "/auth/browser/email/verify",
        &[session, &flow.expect("a flow cookie")],
        "wyrd_contact",
        &json!({ "tenant": slug, "proof": proof }),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
}

/// The subject of a mailed reset code.
const RESET_SUBJECT: &str = "Reset your Wyrd password";

/// The subject of the notice that a reset took effect.
const RESET_DONE_SUBJECT: &str = "Your Wyrd password was reset";

/// Ask for a reset of `username` at `tenant` from the visitor `client`;
/// returns the status, the body and how long the answer took.
async fn request_reset(
    srv: &WyrdTestServer,
    tenant: &str,
    username: &str,
    client: &str,
) -> (StatusCode, Value, std::time::Duration) {
    let started = std::time::Instant::now();
    let (status, _, body) = ingress(
        srv,
        Method::POST,
        "/auth/browser/password/reset/request",
        client,
        &[],
        Payload::Json(&json!({ "tenant": tenant, "username": username })),
    )
    .await;
    (status, body, started.elapsed())
}

/// The texts of the messages with `subject` to `address`, newest first,
/// once there are at least `count`.
///
/// Reset mail is sent after the answer, so this polls Mailpit for up to ten
/// seconds.
///
/// # Panics
/// Panics when fewer than `count` arrive in time.
async fn await_mail(api: &str, address: &str, subject: &str, count: usize) -> Vec<String> {
    for _ in 0..100 {
        let texts: Vec<String> = mail_to(api, address)
            .await
            .into_iter()
            .filter(|(_, _, sent, _)| sent == subject)
            .map(|(_, _, _, text)| text)
            .collect();
        if texts.len() >= count {
            return texts;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("fewer than {count} messages titled {subject:?} arrived");
}

/// How many messages with `subject` Mailpit holds for `address`.
async fn mail_count(api: &str, address: &str, subject: &str) -> usize {
    mail_to(api, address)
        .await
        .iter()
        .filter(|(_, _, sent, _)| sent == subject)
        .count()
}

/// The pasteable reset code a reset message carries: its one 43-character
/// base64url word.
///
/// # Panics
/// Panics when the message has no such word, or more than one.
fn reset_code(text: &str) -> String {
    let codes: Vec<&str> = text
        .split_whitespace()
        .filter(|word| {
            word.len() == 43
                && word
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        })
        .collect();
    assert_eq!(codes.len(), 1, "one code to paste: {text}");
    codes[0].to_owned()
}

/// Redeem `code` at `tenant` with `new_password`; returns the status,
/// every `Set-Cookie` and the body.
async fn redeem(
    srv: &WyrdTestServer,
    tenant: &str,
    code: &str,
    new_password: &str,
) -> (StatusCode, Vec<String>, Value) {
    ingress(
        srv,
        Method::POST,
        "/auth/browser/password/reset",
        VISITOR,
        &[],
        Payload::Json(
            &json!({ "tenant": tenant, "reset_code": code, "new_password": new_password }),
        ),
    )
    .await
}

/// The median of `samples`.
fn median(mut samples: Vec<std::time::Duration>) -> std::time::Duration {
    samples.sort();
    samples[samples.len() / 2]
}

/// A real `wyrd-cli` device sign-in approved by `session` at `slug`;
/// returns the token response.
///
/// # Panics
/// Panics when a step of the device flow fails.
async fn device_sign_in(srv: &WyrdTestServer, slug: &str, session: &str) -> Value {
    let (status, _, device) = ingress(
        srv,
        Method::POST,
        "/auth/device_authorization",
        VISITOR,
        &[],
        Payload::Form(format!("client_id=wyrd-cli&tenant={slug}")),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{device}");
    let approve = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("tenant", slug)
        .append_pair(
            "user_code",
            device["user_code"].as_str().expect("user code"),
        )
        .append_pair("decision", "approve")
        .finish();
    let (status, _, _) = ingress(
        srv,
        Method::POST,
        "/auth/device",
        VISITOR,
        &[session],
        Payload::Form(approve),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the signed-in session approves");
    let poll = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("grant_type", "urn:ietf:params:oauth:grant-type:device_code")
        .append_pair(
            "device_code",
            device["device_code"].as_str().expect("device code"),
        )
        .append_pair("client_id", "wyrd-cli")
        .finish();
    let (status, _, token) = ingress(
        srv,
        Method::POST,
        "/auth/token",
        VISITOR,
        &[],
        Payload::Form(poll),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{token}");
    token
}

/// Renew a CLI sign-in with `refresh`; returns the status and body.
async fn renew(srv: &WyrdTestServer, refresh: &str) -> (StatusCode, Value) {
    let form = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("grant_type", "refresh_token")
        .append_pair("refresh_token", refresh)
        .append_pair("client_id", "wyrd-cli")
        .finish();
    let (status, _, body) = ingress(
        srv,
        Method::POST,
        "/auth/token",
        VISITOR,
        &[],
        Payload::Form(form),
    )
    .await;
    (status, body)
}

/// A local User who forgot a password resets it through a verified contact:
///   1. reset requests for a verified User, an unknown username, an
///      unverified User, a suspended User and an unknown workspace all get the
///      same generic `202` body with comparable timing, and only the
///      verified User's contact is mailed: a workspace reset link and a code
///      to paste, never a code in a URL;
///   2. redeeming the newest mailed code sets the password and signs nothing
///      in; the old password, browser session and CLI renewal stop working,
///      the personal API key keeps working and the new password signs in;
///   3. the contact is told the reset took effect, without the password, and
///      the same code cannot be used again.
///
/// # Panics
/// Panics when a step answers, mails or resets differently.
#[tokio::test(flavor = "multi_thread")]
async fn a_verified_contact_resets_a_forgotten_password() {
    let (srv, operator) = contact_server("quay").await;
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    let api = relay_to_mailpit(&srv, &operator).await;
    let ada = "ada.reset@example.test";
    let cy = "cy.reset@example.test";
    let session = sign_in(&srv, "quay").await;
    verify_contact(&srv, &api, "quay", &session, ada).await;
    for name in ["bea", "cyd"] {
        let (status, _, body) = browser(
            &srv,
            "/v1/principals/users",
            &[&session],
            "-",
            &json!({
                "username": name,
                "password": PASSWORD,
                "assignments": [{ "role": "viewer", "scope": "all" }],
            }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }
    let cy_session = sign_in_as(&srv, "quay", "cyd", PASSWORD)
        .await
        .expect("cyd signs in");
    verify_contact(&srv, &api, "quay", &cy_session, cy).await;
    sqlx::query("UPDATE wyrd.auth_users SET status = 'suspended' WHERE username = 'cyd'")
        .execute(&superuser)
        .await
        .expect("cyd is suspended");
    let (status, _, me) = ingress(
        &srv,
        Method::GET,
        "/v1/principals/me",
        VISITOR,
        &[&session],
        Payload::Empty,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{me}");
    let admin = me["principal_id"]
        .as_str()
        .expect("principal id")
        .to_owned();
    let (status, _, key) = browser(
        &srv,
        &format!("/v1/principals/{admin}/credentials"),
        &[&session],
        "-",
        &json!({ "name": "laptop", "permission_ceiling": ["cards:read"], "expires_in": 3600 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{key}");
    let key = secrecy::SecretString::from(key["credential"].as_str().expect("key").to_owned());
    let cli = device_sign_in(&srv, "quay", &session).await;
    let (status, cli) = renew(&srv, cli["refresh_token"].as_str().expect("refresh")).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the CLI renews before the reset: {cli}"
    );

    // 1. Every request gets the same answer; only the verified User is mailed.
    let mut known = Vec::new();
    let mut others = Vec::new();
    for _ in 0..3 {
        for (tenant, username) in [
            ("quay", "admin"),
            ("quay", "nobody"),
            ("quay", "bea"),
            ("quay", "cyd"),
            ("nowhere", "admin"),
        ] {
            let (status, body, took) = request_reset(&srv, tenant, username, VISITOR).await;
            assert_eq!(status, StatusCode::ACCEPTED, "{tenant}/{username}: {body}");
            assert_eq!(
                body,
                json!({ "status": "check_email" }),
                "{tenant}/{username}"
            );
            if username == "admin" && tenant == "quay" {
                known.push(took);
            } else {
                others.push(took);
            }
        }
    }
    let (known, others) = (median(known), median(others));
    assert!(
        known.abs_diff(others) < others.max(std::time::Duration::from_millis(25)),
        "known {known:?} against others {others:?}"
    );
    let texts = await_mail(&api, ada, RESET_SUBJECT, 3).await;
    for text in &texts {
        assert!(text.contains("http://wyrd.test/t/quay/reset"), "{text}");
        let code = reset_code(text);
        for line in text.lines().filter(|line| line.contains("http")) {
            assert!(!line.contains(&code), "the code is never in a URL: {line}");
        }
    }
    assert_eq!(
        mail_count(&api, cy, RESET_SUBJECT).await,
        0,
        "a suspended User gets nothing"
    );

    // 2. The newest code sets the password, signs nothing in and ends local sign-ins.
    let fresh = "quiet harbor lanterns";
    let code = reset_code(&texts[0]);
    let (status, cookies, body) = redeem(&srv, "quay", &code, fresh).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    assert!(
        !cookies
            .iter()
            .any(|cookie| cookie.starts_with("wyrd_session_")),
        "a reset signs nothing in: {cookies:?}"
    );
    let (status, _, _) = ingress(
        &srv,
        Method::GET,
        "/auth/browser/session?tenant=quay",
        VISITOR,
        &[&session],
        Payload::Empty,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "the old session ended");
    let (status, body) = renew(&srv, cli["refresh_token"].as_str().expect("refresh")).await;
    assert!(
        status.is_client_error(),
        "the CLI cannot renew: {status} {body}"
    );
    srv.exchange_api_key(&key)
        .await
        .expect("the personal key survives the reset");
    assert_eq!(
        sign_in_as(&srv, "quay", "admin", PASSWORD).await,
        Err(StatusCode::UNAUTHORIZED)
    );
    sign_in_as(&srv, "quay", "admin", fresh)
        .await
        .expect("the new password signs in");

    // 3. The contact hears of it, and the code is spent.
    let notices = await_mail(&api, ada, RESET_DONE_SUBJECT, 1).await;
    assert!(!notices[0].contains(fresh), "the notice omits the password");
    let (status, _, body) = redeem(&srv, "quay", &code, "another quiet harbor").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(
        body["code"], "WYRD_AUTH_401_ONE_TIME_CODE_INVALID",
        "{body}"
    );
}
