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
    let peer: IpAddr = "10.7.0.1".parse().expect("peer address");
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(header::HOST, SOCIAL_HOST)
        .header(header::ORIGIN, format!("http://{SOCIAL_HOST}"))
        .header(header::CONTENT_TYPE, "application/json")
        .header(CLIENT_HEADER, "203.0.113.9")
        .extension(axum::extract::ConnectInfo(SocketAddr::new(peer, 40_000)));
    if !cookies.is_empty() {
        builder = builder.header(header::COOKIE, cookies.join("; "));
    }
    let request = builder
        .body(Body::from(serde_json::to_vec(body).expect("serializes")))
        .expect("request builds");
    let resp = srv.oneshot(request).await.expect("route responds");
    let status = resp.status();
    let prefix = format!("{set}=");
    let cookie = resp
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with(&prefix))
        .and_then(|value| value.split(';').next())
        .map(str::to_owned);
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
        .await
        .expect("read body");
    (
        status,
        cookie,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// Sign `admin` in to `slug` with [`PASSWORD`]; returns its session
/// cookie pair.
async fn sign_in(srv: &WyrdTestServer, slug: &str) -> String {
    let resp = srv
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/browser/password/login")
                .header(header::HOST, SOCIAL_HOST)
                .header(header::ORIGIN, format!("http://{SOCIAL_HOST}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(CLIENT_HEADER, "203.0.113.9")
                .extension(axum::extract::ConnectInfo(SocketAddr::new(
                    "10.7.0.1".parse().expect("peer"),
                    40_000,
                )))
                .body(Body::from(
                    json!({ "tenant": slug, "username": "admin", "password": PASSWORD })
                        .to_string(),
                ))
                .expect("request builds"),
        )
        .await
        .expect("login responds");
    assert_eq!(resp.status(), StatusCode::OK);
    session_cookie(&resp, slug).expect("a password session")
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
