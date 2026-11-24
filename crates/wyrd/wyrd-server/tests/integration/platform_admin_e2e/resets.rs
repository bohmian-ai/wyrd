//! Self-service password reset journeys: a User with a verified contact
//! resets a forgotten password from mailed codes, against a real server,
//! repository Postgres and Mailpit.

use std::net::{IpAddr, SocketAddr};

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use serde_json::{Value, json};
use wyrd_testing::WyrdTestServer;

use super::contacts::{
    CLIENT_HEADER, PASSWORD, browser, contact_server, mailed_proof, relay_to_mailpit, sign_in,
    sign_in_as,
};
use super::{SOCIAL_HOST, mail_to, social_call, social_request};

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

/// Every stale or foreign use of a reset code refuses, and the offline paths
/// stay usable:
///   1. a mailed code replaced by a newer request, then the newer one once
///      expired, both refuse;
///   2. a contact proof is not a reset code, and a code mailed to a contact
///      that has since been replaced refuses;
///   3. while company SSO is required, reset is neither requested nor
///      redeemed; a social-only User gets no reset code from an operator;
///   4. without a mail relay reset is no longer requested, but a code
///      already mailed and an operator-issued code both still reset.
///
/// # Panics
/// Panics when a code resets, or a request answers, differently.
#[tokio::test(flavor = "multi_thread")]
async fn a_reset_code_refuses_every_stale_or_foreign_use() {
    let (srv, operator) = contact_server("pier").await;
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    let api = relay_to_mailpit(&srv, &operator).await;
    let old = "ada.stale@example.test";
    let new = "ada.renewed@example.test";
    let dee = "dee.offline@example.test";
    let fresh = "quiet harbor lanterns";
    let session = sign_in(&srv, "pier").await;
    verify_contact(&srv, &api, "pier", &session, old).await;
    let mut users = std::collections::BTreeMap::new();
    for name in ["bea", "dee"] {
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
        users.insert(name, body["principal_id"].as_str().expect("id").to_owned());
    }
    let dee_session = sign_in_as(&srv, "pier", "dee", PASSWORD)
        .await
        .expect("dee signs in");
    verify_contact(&srv, &api, "pier", &dee_session, dee).await;
    let refused = |status: StatusCode, body: &Value, case: &str| {
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{case}: {body}");
        assert_eq!(
            body["code"], "WYRD_AUTH_401_ONE_TIME_CODE_INVALID",
            "{case}: {body}"
        );
    };

    // 1. A newer request replaces a code; a code expires.
    for _ in 0..2 {
        assert_eq!(
            request_reset(&srv, "pier", "admin", VISITOR).await.0,
            StatusCode::ACCEPTED
        );
    }
    let texts = await_mail(&api, old, RESET_SUBJECT, 2).await;
    let (newer, older) = (reset_code(&texts[0]), reset_code(&texts[1]));
    let (status, _, body) = redeem(&srv, "pier", &older, fresh).await;
    refused(status, &body, "replaced");
    sqlx::query(
        "UPDATE wyrd.auth_password_resets
            SET created_at = created_at - interval '16 minutes',
                expires_at = expires_at - interval '16 minutes'
          WHERE consumed_at IS NULL",
    )
    .execute(&superuser)
    .await
    .expect("codes age");
    let (status, _, body) = redeem(&srv, "pier", &newer, fresh).await;
    refused(status, &body, "expired");

    // 2. A contact proof is no reset code; replacing the contact voids its codes.
    assert_eq!(
        request_reset(&srv, "pier", "admin", VISITOR).await.0,
        StatusCode::ACCEPTED
    );
    let bound = reset_code(&await_mail(&api, old, RESET_SUBJECT, 3).await[0]);
    let (status, flow, body) = browser(
        &srv,
        "/auth/browser/email/request",
        &[&session],
        "wyrd_contact",
        &json!({ "tenant": "pier", "email": new }),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    let (proof, _) = mailed_proof(&api, new).await;
    let (status, _, body) = redeem(&srv, "pier", &proof, fresh).await;
    refused(status, &body, "contact proof");
    let (status, _, body) = browser(
        &srv,
        "/auth/browser/email/verify",
        &[&session, &flow.expect("a flow cookie")],
        "wyrd_contact",
        &json!({ "tenant": "pier", "proof": proof }),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    let (status, _, body) = redeem(&srv, "pier", &bound, fresh).await;
    refused(status, &body, "old contact");
    sign_in_as(&srv, "pier", "admin", PASSWORD)
        .await
        .expect("no refused code changed the password");

    // 3. Required company SSO leaves no reset path; social-only Users get no code.
    assert_eq!(
        request_reset(&srv, "pier", "dee", VISITOR).await.0,
        StatusCode::ACCEPTED
    );
    let mailed = reset_code(&await_mail(&api, dee, RESET_SUBJECT, 1).await[0]);
    let require_sso = |required: bool| {
        sqlx::query(
            "INSERT INTO wyrd.auth_login_policies (data_tenant_id, require_company_sso)
             SELECT data_tenant_id, $1 FROM platform.tenants WHERE slug = 'pier'
             ON CONFLICT (data_tenant_id) DO UPDATE
                SET require_company_sso = EXCLUDED.require_company_sso,
                    revision = wyrd.auth_login_policies.revision + 1",
        )
        .bind(required)
        .execute(&superuser)
    };
    require_sso(true).await.expect("SSO required");
    let (status, body, _) = request_reset(&srv, "pier", "dee", VISITOR).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "WYRD_AUTH_403_METHOD_UNAVAILABLE", "{body}");
    let (status, _, body) = redeem(&srv, "pier", &mailed, fresh).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    require_sso(false).await.expect("SSO no longer required");
    let eve = uuid::Uuid::now_v7();
    sqlx::query(
        "INSERT INTO wyrd.auth_users (id, data_tenant_id, email, auth_type, status)
         SELECT $1, data_tenant_id, 'eve@example.test', 'oidc', 'active'
           FROM platform.tenants WHERE slug = 'pier'",
    )
    .bind(eve)
    .execute(&superuser)
    .await
    .expect("a social-only User");
    let admin_session = sign_in(&srv, "pier").await;
    let (status, _, body) = browser(
        &srv,
        &format!("/v1/principals/users/{eve}/password-reset"),
        &[&admin_session],
        "-",
        &json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    // 4. Without a relay nothing is requested, but issued codes still reset.
    let (status, _, issued) = browser(
        &srv,
        &format!("/v1/principals/users/{}/password-reset", users["bea"]),
        &[&admin_session],
        "-",
        &json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{issued}");
    sqlx::query(
        "UPDATE platform.login_settings
            SET smtp_host = NULL, smtp_port = NULL, smtp_tls = NULL, smtp_username = NULL,
                smtp_password_enc = NULL, smtp_sender = NULL",
    )
    .execute(&superuser)
    .await
    .expect("the relay is removed");
    let (status, body, _) = request_reset(&srv, "pier", "dee", VISITOR).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "WYRD_AUTH_403_METHOD_UNAVAILABLE", "{body}");
    let (status, _, body) = redeem(&srv, "pier", &mailed, fresh).await;
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "the mailed code survives: {body}"
    );
    let (status, _, body) = redeem(
        &srv,
        "pier",
        issued["reset_code"].as_str().expect("code"),
        fresh,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "the operator code works offline: {body}"
    );
    for name in ["dee", "bea"] {
        sign_in_as(&srv, "pier", name, fresh)
            .await
            .expect("the reset password signs in");
    }
}

/// Reset requests are throttled without locking or disclosing delivery:
///   1. past its account window a User's requests still get the generic
///      `202`, but only the first three are mailed, and the password still
///      signs in;
///   2. one source is refused with `429` once its window is spent;
///   3. a relay that refuses the message changes nothing in the answer, and
///      the lost code is never resent: once the relay works again only a new
///      request mails one.
///
/// # Panics
/// Panics when a request answers, mails or locks differently.
#[tokio::test(flavor = "multi_thread")]
async fn reset_requests_are_throttled_without_locking_or_disclosing_delivery() {
    let (srv, operator) = contact_server("dock").await;
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    let api = relay_to_mailpit(&srv, &operator).await;
    let ada = "ada.throttle@example.test";
    let bea = "bea.throttle@example.test";
    let session = sign_in(&srv, "dock").await;
    verify_contact(&srv, &api, "dock", &session, ada).await;
    let (status, _, body) = browser(
        &srv,
        "/v1/principals/users",
        &[&session],
        "-",
        &json!({
            "username": "bea",
            "password": PASSWORD,
            "assignments": [{ "role": "viewer", "scope": "all" }],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let bea_session = sign_in_as(&srv, "dock", "bea", PASSWORD)
        .await
        .expect("bea signs in");
    verify_contact(&srv, &api, "dock", &bea_session, bea).await;

    // 1. The account window silences mail, not the answer, and locks nothing.
    for _ in 0..5 {
        let (status, body, _) = request_reset(&srv, "dock", "admin", VISITOR).await;
        assert_eq!(status, StatusCode::ACCEPTED, "{body}");
        assert_eq!(body, json!({ "status": "check_email" }));
    }
    let counted = || async {
        sqlx::query_scalar::<_, i32>(
            "SELECT attempts FROM platform.signup_admissions WHERE scope = 'reset_account'",
        )
        .fetch_all(&superuser)
        .await
        .expect("account windows read")
    };
    for _ in 0..100 {
        if counted().await == vec![5] {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(counted().await, vec![5], "every request was counted");
    await_mail(&api, ada, RESET_SUBJECT, 3).await;
    assert_eq!(
        mail_count(&api, ada, RESET_SUBJECT).await,
        3,
        "three were mailed"
    );
    sign_in_as(&srv, "dock", "admin", PASSWORD)
        .await
        .expect("requests never lock the account");

    // 2. One source's window ends in 429.
    let source = "198.51.100.7";
    for _ in 0..30 {
        let (status, body, _) = request_reset(&srv, "dock", "nobody", source).await;
        assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    }
    let (status, body, _) = request_reset(&srv, "dock", "nobody", source).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    assert_eq!(body["code"], "WYRD_AUTH_429_PASSWORD_ATTEMPTS", "{body}");
    assert!(
        body["details"]["retry_after_seconds"].as_u64() > Some(0),
        "{body}"
    );
    let (status, _, _) = request_reset(&srv, "dock", "nobody", VISITOR).await;
    assert_eq!(status, StatusCode::ACCEPTED, "other sources are unaffected");

    // 3. A refusing relay is invisible to the caller, and its loss is final.
    let refusing = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a refusing relay binds");
    let port = refusing.local_addr().expect("bound").port();
    let (attempted, mut attempts) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = refusing.accept().await {
            use tokio::io::AsyncWriteExt as _;
            let _ = stream.write_all(b"554 5.3.0 no service\r\n").await;
            let _ = attempted.send(());
        }
    });
    let (status, relay) = social_call(
        &srv,
        social_request(
            Method::PUT,
            "/platform/mail",
            Some(&operator),
            Some(json!({ "host": "127.0.0.1", "port": port, "tls": "none", "sender": "accounts@wyrd.test" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{relay}");
    let (status, body, _) = request_reset(&srv, "dock", "bea", VISITOR).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert_eq!(body, json!({ "status": "check_email" }));
    tokio::time::timeout(std::time::Duration::from_secs(10), attempts.recv())
        .await
        .expect("the send reached the refusing relay")
        .expect("the relay reports");
    relay_to_mailpit(&srv, &operator).await;
    assert_eq!(
        request_reset(&srv, "dock", "bea", VISITOR).await.0,
        StatusCode::ACCEPTED
    );
    let texts = await_mail(&api, bea, RESET_SUBJECT, 1).await;
    assert_eq!(texts.len(), 1, "the lost code was never resent");
    let (status, _, body) = redeem(
        &srv,
        "dock",
        &reset_code(&texts[0]),
        "quiet harbor lanterns",
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    assert!(
        attempts.try_recv().is_err(),
        "nothing retried the refused send"
    );
}
