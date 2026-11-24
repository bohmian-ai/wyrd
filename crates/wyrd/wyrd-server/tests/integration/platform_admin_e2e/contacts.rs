//! Verified-contact journeys: a signed-in User proves a recovery address
//! by email and replaces it, against a real server, repository Postgres and
//! Mailpit.

use std::net::{IpAddr, SocketAddr};

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use serde_json::{Value, json};
use wyrd_testing::WyrdTestServer;
use wyrd_testing::principal::within_propagation;

use super::{
    SOCIAL_HOST, initialize_platform_root, mail_to, mailpit, platform_session, session_cookie,
    social_call, social_request,
};

/// The trusted ingress every journey here sits behind.
const INGRESS: &str = "10.0.0.0/8";

/// The header that ingress names the visitor in.
pub(super) const CLIENT_HEADER: &str = "CF-Connecting-IP";

/// The administrator's password.
pub(super) const PASSWORD: &str = "amber kestrel lantern";

/// A server behind [`INGRESS`] with tenant `slug`, whose `admin` signs in
/// with [`PASSWORD`], and its operator session.
///
/// # Panics
/// Panics when the server, operator session or tenant cannot be set up.
pub(super) async fn contact_server(slug: &str) -> (WyrdTestServer, String) {
    let (srv, session) = bare_server().await;
    add_tenant(&srv, &session, slug).await;
    (srv, session)
}

/// A server behind [`INGRESS`] with no tenant, and its operator session.
///
/// # Panics
/// Panics when the server or operator session cannot be set up.
async fn bare_server() -> (WyrdTestServer, String) {
    let srv = WyrdTestServer::builder()
        .with_trusted_ingress_for_test(&[INGRESS], CLIENT_HEADER)
        .start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;
    (srv, session)
}

/// Create tenant `slug`, displayed as `Slug Team`, whose `admin` signs in
/// with [`PASSWORD`].
///
/// # Panics
/// Panics when the tenant is not created.
async fn add_tenant(srv: &WyrdTestServer, operator: &str, slug: &str) {
    let (status, created) = social_call(
        srv,
        social_request(
            Method::POST,
            "/platform/tenants",
            Some(operator),
            Some(json!({
                "slug": slug,
                "display_name": format!("{slug} team"),
                "human_admin": { "username": "admin", "password": PASSWORD },
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
}

/// Point the deployment's mail relay at the test Mailpit; returns its API.
pub(super) async fn relay_to_mailpit(srv: &WyrdTestServer, session: &str) -> String {
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
pub(super) async fn browser(
    srv: &WyrdTestServer,
    uri: &str,
    cookies: &[&str],
    set: &str,
    body: &Value,
) -> (StatusCode, Option<String>, Value) {
    let (status, _, cookie, body) = browser_from(srv, "203.0.113.9", uri, cookies, set, body).await;
    (status, cookie, body)
}

/// [`browser`] from the visitor `client` the ingress names, also returning
/// the `Retry-After` header.
async fn browser_from(
    srv: &WyrdTestServer,
    client: &str,
    uri: &str,
    cookies: &[&str],
    set: &str,
    body: &Value,
) -> (StatusCode, Option<String>, Option<String>, Value) {
    let peer: IpAddr = "10.7.0.1".parse().expect("peer address");
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(header::HOST, SOCIAL_HOST)
        .header(header::ORIGIN, format!("http://{SOCIAL_HOST}"))
        .header(header::CONTENT_TYPE, "application/json")
        .header(CLIENT_HEADER, client)
        .extension(axum::extract::ConnectInfo(SocketAddr::new(peer, 40_000)));
    if !cookies.is_empty() {
        builder = builder.header(header::COOKIE, cookies.join("; "));
    }
    let request = builder
        .body(Body::from(serde_json::to_vec(body).expect("serializes")))
        .expect("request builds");
    let resp = srv.oneshot(request).await.expect("route responds");
    let status = resp.status();
    let retry_after = resp
        .headers()
        .get(header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
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
        retry_after,
        cookie,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// Sign `admin` in to `slug` with [`PASSWORD`]; returns its session
/// cookie pair.
pub(super) async fn sign_in(srv: &WyrdTestServer, slug: &str) -> String {
    sign_in_as(srv, slug, "admin", PASSWORD)
        .await
        .expect("a password session")
}

/// Sign `username` in to `slug` with `password`; returns its session
/// cookie pair, or the refusal's status.
pub(super) async fn sign_in_as(
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
                .header(CLIENT_HEADER, "203.0.113.9")
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
pub(super) async fn mailed_proof(api: &str, address: &str) -> (String, String) {
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

/// The proof and text of the message to `address` after its `before`
/// earlier ones. Discovery mails after its answer, so this polls Mailpit for
/// up to ten seconds.
///
/// # Panics
/// Panics when no further message arrives in time.
async fn next_proof(api: &str, address: &str, before: usize) -> (String, String) {
    for _ in 0..100 {
        if mail_to(api, address).await.len() > before {
            return mailed_proof(api, address).await;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("no further message to {address} arrived");
}

/// `slug`'s verified contacts: address and revision.
pub(super) async fn contacts(superuser: &sqlx::PgPool, slug: &str) -> Vec<(String, i64)> {
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

/// Make `email` the verified contact of `slug`'s `admin` through a fresh
/// sign-in and the mailed proof.
///
/// # Panics
/// Panics when any step is refused.
async fn verify_admin_contact(srv: &WyrdTestServer, api: &str, slug: &str, email: &str) {
    let session = sign_in(srv, slug).await;
    let (status, flow, body) = browser(
        srv,
        "/auth/browser/email/request",
        &[&session],
        "wyrd_contact",
        &json!({ "tenant": slug, "email": email }),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    let (proof, _) = mailed_proof(api, email).await;
    let (status, _, body) = browser(
        srv,
        "/auth/browser/email/verify",
        &[&session, &flow.expect("a flow cookie")],
        "wyrd_contact",
        &json!({ "tenant": slug, "proof": proof }),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
}

/// How many directory entries the deployment holds.
async fn directory_entries(superuser: &sqlx::PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM platform.workspace_directory")
        .fetch_one(superuser)
        .await
        .expect("directory counts")
}

/// Ask for the workspaces of `email` from a new browser, read the mailed
/// proof and verify it there; returns the verify status, its body and the
/// discovery read cookie.
///
/// # Panics
/// Panics when the request is not accepted or no proof arrives.
async fn discover(srv: &WyrdTestServer, api: &str, email: &str) -> (StatusCode, Value, String) {
    let before = mail_to(api, email).await.len();
    let (status, binding, body) = browser(
        srv,
        "/auth/browser/workspaces/request",
        &[],
        "wyrd_discovery",
        &json!({ "email": email }),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert_eq!(body, json!({ "status": "check_email" }));
    let binding = binding.expect("a discovery binding cookie");
    let (proof, _) = next_proof(api, email, before).await;
    let (status, read, body) = browser(
        srv,
        "/auth/browser/workspaces/verify",
        &[&binding],
        "wyrd_discovery",
        &json!({ "proof": proof }),
    )
    .await;
    (status, body, read.unwrap_or_default())
}

/// The `(tenant, display_name, login_url)` of each workspace in a discovery
/// page, refusing any other field.
fn workspaces(page: &Value) -> Vec<(String, String, String)> {
    page["workspaces"]
        .as_array()
        .expect("a workspace list")
        .iter()
        .map(|workspace| {
            let fields = workspace.as_object().expect("a workspace object");
            assert_eq!(
                fields.len(),
                3,
                "only tenant, name and login link: {workspace}"
            );
            let text = |key: &str| workspace[key].as_str().expect(key).to_owned();
            (text("tenant"), text("display_name"), text("login_url"))
        })
        .collect()
}

/// A visitor discovers only the live workspaces whose User verified the
/// address, and still signs in to each:
///   1. without a mail relay, discovery is not offered;
///   2. known and unknown addresses get the same `202` and a mail with a
///      fragment proof and no workspace name;
///   3. the proof in another browser reads nothing; in its own browser it
///      returns the two workspaces whose admin verified the address, as
///      fixed same-origin login links, sets no session, and an unknown
///      address gets an empty list;
///   4. the same browser repeating the verification gets the same result;
///   5. replacing one contact, suspending the other User and retiring a
///      tenant each remove a workspace at once, before any reconciliation;
///   6. a listed login link still needs that workspace's own sign-in.
///
/// # Panics
/// Panics when discovery discloses, omits or mails differently.
#[tokio::test(flavor = "multi_thread")]
async fn a_visitor_discovers_only_live_workspaces_for_a_verified_address() {
    let (srv, operator) = bare_server().await;
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    for slug in ["north", "south", "east", "west"] {
        add_tenant(&srv, &operator, slug).await;
    }
    let shared = "dana.discover@example.test";
    let other = "eli.discover@example.test";
    let unknown = "fay.discover@example.test";

    // 1. No relay, no discovery.
    let (status, _, body) = browser(
        &srv,
        "/auth/browser/workspaces/request",
        &[],
        "wyrd_discovery",
        &json!({ "email": shared }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "WYRD_AUTH_403_METHOD_UNAVAILABLE", "{body}");

    let api = relay_to_mailpit(&srv, &operator).await;
    for slug in ["north", "south", "east"] {
        verify_admin_contact(&srv, &api, slug, shared).await;
    }
    verify_admin_contact(&srv, &api, "west", other).await;
    assert_eq!(
        within_propagation(&4, || directory_entries(&superuser)).await,
        4,
        "every verified contact reaches the directory"
    );

    // 2. and 3. A proof read in another browser discloses nothing.
    let before = mail_to(&api, shared).await.len();
    let (status, binding, _) = browser(
        &srv,
        "/auth/browser/workspaces/request",
        &[],
        "wyrd_discovery",
        &json!({ "email": shared }),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert!(binding.is_some());
    let (proof, text) = next_proof(&api, shared, before).await;
    for name in ["north", "south", "east", "west"] {
        assert!(!text.contains(name), "the mail names no workspace: {text}");
    }
    let (status, _, _) = browser(
        &srv,
        "/auth/browser/workspaces/verify",
        &[],
        "wyrd_discovery",
        &json!({ "proof": proof }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, page, read) = discover(&srv, &api, shared).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(!read.is_empty(), "a discovery read cookie");
    let login = |slug: &str| {
        (
            slug.to_owned(),
            format!("{slug} team"),
            format!("http://{SOCIAL_HOST}/t/{slug}/login"),
        )
    };
    assert_eq!(
        workspaces(&page),
        vec![login("east"), login("north"), login("south")]
    );
    assert!(page.get("next_cursor").is_none_or(Value::is_null), "{page}");
    let (status, page, _) = discover(&srv, &api, unknown).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(workspaces(&page).is_empty());

    // 4. The same browser repeating its verification reads the same page.
    let before = mail_to(&api, shared).await.len();
    let (status, binding, _) = browser(
        &srv,
        "/auth/browser/workspaces/request",
        &[],
        "wyrd_discovery",
        &json!({ "email": shared }),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let binding = binding.expect("a binding");
    let (proof, _) = next_proof(&api, shared, before).await;
    for _ in 0..2 {
        let (status, _, page) = browser(
            &srv,
            "/auth/browser/workspaces/verify",
            &[&binding],
            "wyrd_discovery",
            &json!({ "proof": proof }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert_eq!(workspaces(&page).len(), 3);
    }

    // 5. Revalidation removes a workspace before reconciliation catches up.
    verify_admin_contact(&srv, &api, "south", other).await;
    sqlx::query(
        "UPDATE wyrd.auth_users u SET status = 'suspended'
           FROM platform.tenants t
          WHERE t.data_tenant_id = u.data_tenant_id AND t.slug = 'east'",
    )
    .execute(&superuser)
    .await
    .expect("east's admin is suspended");
    let (_, page, _) = discover(&srv, &api, shared).await;
    assert_eq!(workspaces(&page), vec![login("north")]);
    sqlx::query("UPDATE platform.tenants SET status = 'deleted' WHERE slug = 'north'")
        .execute(&superuser)
        .await
        .expect("north is retired");
    let (_, page, _) = discover(&srv, &api, shared).await;
    assert!(workspaces(&page).is_empty(), "{page}");
    let (_, page, _) = discover(&srv, &api, other).await;
    assert_eq!(workspaces(&page), vec![login("south"), login("west")]);

    // 6. A listed workspace still needs its own sign-in.
    let (status, _) = social_call(
        &srv,
        social_request(Method::GET, "/auth/browser/session?tenant=west", None, None),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// Assert `(status, retry_after, body)` is the email admission refusal, with
/// a `Retry-After` equal to its whole-second problem field.
fn assert_throttled(status: StatusCode, retry_after: Option<&str>, body: &Value) {
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    assert_eq!(body["code"], "WYRD_AUTH_429_EMAIL_ATTEMPTS", "{body}");
    let seconds = body["details"]["retry_after_seconds"]
        .as_u64()
        .expect("whole-second wait");
    assert!(seconds > 0);
    assert_eq!(retry_after, Some(seconds.to_string().as_str()));
}

/// Email requests are counted before any mail is sent:
///   1. one address asked about again and again still gets the generic
///      `202`, but past its budget no further mail goes to it;
///   2. one visitor asking about many addresses is refused with `429`
///      and `Retry-After` once its source budget is spent, while another
///      visitor is still served;
///   3. a signed-in User asking to verify address after address is refused
///      with `429` once the account budget is spent, and nothing more is
///      mailed.
///
/// # Panics
/// Panics when a budget admits, refuses or mails differently.
#[tokio::test(flavor = "multi_thread")]
async fn email_requests_are_admitted_before_any_mail_is_sent() {
    let (srv, operator) = contact_server("harbor").await;
    let api = relay_to_mailpit(&srv, &operator).await;
    let discover_from = |client: &'static str, email: String| {
        let srv = &srv;
        async move {
            browser_from(
                srv,
                client,
                "/auth/browser/workspaces/request",
                &[],
                "wyrd_discovery",
                &json!({ "email": email }),
            )
            .await
        }
    };

    // 1. The address budget never shows in the response.
    let asked = "gus.budget@example.test";
    for _ in 0..7 {
        let (status, _, _, body) = discover_from("198.51.100.1", asked.to_owned()).await;
        assert_eq!(status, StatusCode::ACCEPTED, "{body}");
        assert_eq!(body, json!({ "status": "check_email" }));
    }
    assert_eq!(
        within_propagation(&5, || async { mail_to(&api, asked).await.len() }).await,
        5,
        "the address budget stops mail"
    );

    // 2. The source budget refuses with Retry-After.
    let mut refused = None;
    for n in 0..22 {
        let (status, retry_after, _, body) =
            discover_from("198.51.100.2", format!("hal{n}.budget@example.test")).await;
        if status != StatusCode::ACCEPTED {
            refused = Some((n, status, retry_after, body));
            break;
        }
    }
    let (n, status, retry_after, body) = refused.expect("the source budget refuses");
    assert_eq!(n, 20, "twenty requests per source are admitted");
    assert_throttled(status, retry_after.as_deref(), &body);
    assert!(mail_to(&api, "hal20.budget@example.test").await.is_empty());
    let (status, _, _, _) =
        discover_from("198.51.100.3", "ivy.budget@example.test".to_owned()).await;
    assert_eq!(status, StatusCode::ACCEPTED, "another source is unaffected");

    // 3. The account budget refuses a signed-in User.
    let session = sign_in(&srv, "harbor").await;
    let mut refused = None;
    for n in 0..7 {
        let (status, retry_after, _, body) = browser_from(
            &srv,
            "198.51.100.4",
            "/auth/browser/email/request",
            &[&session],
            "wyrd_contact",
            &json!({ "tenant": "harbor", "email": format!("jo{n}.budget@example.test") }),
        )
        .await;
        if status != StatusCode::ACCEPTED {
            refused = Some((n, status, retry_after, body));
            break;
        }
    }
    let (n, status, retry_after, body) = refused.expect("the account budget refuses");
    assert_eq!(n, 5, "five contact requests per account are admitted");
    assert_throttled(status, retry_after.as_deref(), &body);
    assert!(mail_to(&api, "jo5.budget@example.test").await.is_empty());
}

/// Point the deployment's mail relay at `port` on this host, without TLS.
async fn relay_to(srv: &WyrdTestServer, session: &str, port: u16) {
    let (status, relay) = social_call(
        srv,
        social_request(
            Method::PUT,
            "/platform/mail",
            Some(session),
            Some(json!({ "host": "127.0.0.1", "port": port, "tls": "none", "sender": "accounts@wyrd.test" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{relay}");
}

/// A discovery answer shows nothing of its address's mail, whatever the
/// relay does:
///   1. with a relay that accepts connections and never answers, an address
///      below its budget and one past it both get the generic `202` at once;
///   2. with a relay that refuses connections, both still get the generic
///      `202`;
///   3. with a working relay, the address below its budget is mailed once
///      and the one past it is not mailed.
///
/// # Panics
/// Panics when an answer differs between the two addresses, waits on the
/// relay, or mail goes where its budget forbids.
#[tokio::test(flavor = "multi_thread")]
async fn discovery_answers_alike_whatever_the_relay_does() {
    let (srv, operator) = contact_server("harbor").await;
    let discover_from = |client: &'static str, email: &'static str| {
        let srv = &srv;
        async move {
            let started = std::time::Instant::now();
            let (status, _, flow, body) = browser_from(
                srv,
                client,
                "/auth/browser/workspaces/request",
                &[],
                "wyrd_discovery",
                &json!({ "email": email }),
            )
            .await;
            (status, flow.is_some(), body, started.elapsed())
        }
    };
    let generic = (
        StatusCode::ACCEPTED,
        true,
        json!({ "status": "check_email" }),
    );
    let answer = |reply: &(StatusCode, bool, Value, std::time::Duration)| {
        (reply.0, reply.1, reply.2.clone())
    };
    // The relay's ten-second SMTP timeout would show in a waited answer.
    let at_once = std::time::Duration::from_secs(3);

    // 1. A relay that never answers delays neither address.
    let stalled = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a stalling relay binds");
    let stalled_port = stalled.local_addr().expect("relay address").port();
    let held = tokio::spawn(async move {
        let mut open = Vec::new();
        while let Ok((socket, _)) = stalled.accept().await {
            open.push(socket);
        }
    });
    relay_to(&srv, &operator, stalled_port).await;
    let suppressed = "kit.suppressed@example.test";
    for _ in 0..5 {
        discover_from("198.51.100.21", suppressed).await;
    }
    for (client, email) in [
        ("198.51.100.22", "lou.below@example.test"),
        ("198.51.100.23", suppressed),
    ] {
        let reply = discover_from(client, email).await;
        assert_eq!(answer(&reply), generic, "{email}");
        assert!(
            reply.3 < at_once,
            "{email} waited {:?} on the relay",
            reply.3
        );
    }

    // 2. A relay that refuses connections changes neither answer.
    let closed = std::net::TcpListener::bind("127.0.0.1:0").expect("a port binds");
    let closed_port = closed.local_addr().expect("port").port();
    drop(closed);
    relay_to(&srv, &operator, closed_port).await;
    for (client, email) in [
        ("198.51.100.24", "max.below@example.test"),
        ("198.51.100.25", suppressed),
    ] {
        let reply = discover_from(client, email).await;
        assert_eq!(answer(&reply), generic, "{email}");
        assert!(
            reply.3 < at_once,
            "{email} waited {:?} on the relay",
            reply.3
        );
    }
    held.abort();

    // 3. A working relay mails the address below its budget only.
    let api = relay_to_mailpit(&srv, &operator).await;
    let below = "ned.below@example.test";
    for (client, email) in [("198.51.100.26", below), ("198.51.100.27", suppressed)] {
        assert_eq!(
            answer(&discover_from(client, email).await),
            generic,
            "{email}"
        );
    }
    next_proof(&api, below, 0).await;
    assert!(
        mail_to(&api, suppressed).await.is_empty(),
        "the address past its budget is never mailed"
    );
}

/// A contact flow belongs to the User and tenant that began it:
///   1. another User of the same tenant, signed in and recent, presenting
///      the flow cookie and its proof is refused and nothing is stored;
///   2. the User who began it still completes it afterwards;
///   3. a session for one tenant cannot ask for another tenant's contact.
///
/// # Panics
/// Panics when another User's session verifies or stores a contact.
#[tokio::test(flavor = "multi_thread")]
async fn a_contact_flow_belongs_to_the_user_who_began_it() {
    let (srv, operator) = contact_server("harbor").await;
    add_tenant(&srv, &operator, "cove").await;
    let api = relay_to_mailpit(&srv, &operator).await;
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    let tenant: uuid::Uuid =
        sqlx::query_scalar("SELECT data_tenant_id FROM platform.tenants WHERE slug = 'harbor'")
            .fetch_one(&superuser)
            .await
            .expect("tenant id");
    let admin = srv
        .bootstrap_service_in_tenant(
            wyrd_spec::ids::DataTenantId::new(tenant).expect("a UUIDv7 tenant"),
            "contacts-admin",
            &["admin"],
        )
        .await
        .expect("tenant admin bootstraps");
    let admin = srv
        .exchange_api_key(admin.api_key().expect("admin key"))
        .await
        .expect("admin key exchanges");
    let other_password = "quiet harbor lantern";
    let (status, created) = social_call(
        &srv,
        social_request(
            Method::POST,
            "/v1/principals/users",
            Some(&admin),
            Some(json!({
                "username": "ada", "password": other_password,
                "assignments": [{ "role": "viewer", "scope": "all" }],
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let address = "owner.flow@example.test";
    let owner = sign_in(&srv, "harbor").await;
    let (status, flow, body) = browser(
        &srv,
        "/auth/browser/email/request",
        &[&owner],
        "wyrd_contact",
        &json!({ "tenant": "harbor", "email": address }),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    let flow = flow.expect("a flow cookie");
    let (proof, _) = mailed_proof(&api, address).await;
    let verify = json!({ "tenant": "harbor", "proof": proof });

    // 1. Another User's session cannot take the flow over.
    let other = sign_in_as(&srv, "harbor", "ada", other_password)
        .await
        .expect("a password session");
    let (status, _, body) = browser(
        &srv,
        "/auth/browser/email/verify",
        &[&other, &flow],
        "wyrd_contact",
        &verify,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(
        body["code"], "WYRD_AUTH_401_ONE_TIME_CODE_INVALID",
        "{body}"
    );
    assert!(
        contacts(&superuser, "harbor").await.is_empty(),
        "nothing stored"
    );

    // 2. The User who began it completes it.
    let (status, _, body) = browser(
        &srv,
        "/auth/browser/email/verify",
        &[&owner, &flow],
        "wyrd_contact",
        &verify,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    assert_eq!(contacts(&superuser, "harbor").await.len(), 1);

    // 3. A session for one tenant asks nothing of another.
    let (status, _, _) = browser(
        &srv,
        "/auth/browser/email/request",
        &[&owner],
        "wyrd_contact",
        &json!({ "tenant": "cove", "email": "cove.flow@example.test" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(mail_to(&api, "cove.flow@example.test").await.is_empty());
    assert!(contacts(&superuser, "cove").await.is_empty());
}

/// A discovery read pages through every live workspace:
///   1. an address that reaches 23 workspaces reads the first 20 in
///      route-key order with a `next_cursor`;
///   2. the read cookie and that cursor return the last 3 and no cursor;
///   3. the next page without the read cookie is refused.
///
/// # Panics
/// Panics when a page is missing, repeats or reorders a workspace.
#[tokio::test(flavor = "multi_thread")]
async fn a_discovery_read_pages_through_every_workspace() {
    let (srv, operator) = bare_server().await;
    let api = relay_to_mailpit(&srv, &operator).await;
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    let slugs: Vec<String> = (0..23).map(|n| format!("page-{n:02}")).collect();
    for slug in &slugs {
        add_tenant(&srv, &operator, slug).await;
    }
    let address = "pager@example.test";
    sqlx::query(
        "INSERT INTO wyrd.auth_user_contacts (data_tenant_id, user_id, address)
         SELECT u.data_tenant_id, u.id, $1
           FROM wyrd.auth_users u JOIN platform.tenants t USING (data_tenant_id)
          WHERE t.slug LIKE 'page-%'",
    )
    .bind(address)
    .execute(&superuser)
    .await
    .expect("contacts seed");
    assert_eq!(
        within_propagation(&23, || directory_entries(&superuser)).await,
        23
    );

    // 1. The first page.
    let (status, first, read) = discover(&srv, &api, address).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    let listed = |page: &Value| {
        workspaces(page)
            .into_iter()
            .map(|(slug, _, _)| slug)
            .collect::<Vec<_>>()
    };
    assert_eq!(listed(&first), slugs[..20]);
    let cursor = first["next_cursor"].as_str().expect("a next cursor");

    // 2. and 3. The next page needs the read cookie.
    let next = |cookie: Option<String>| {
        let mut request = Request::builder()
            .method(Method::GET)
            .uri(format!("/auth/browser/workspaces?cursor={cursor}"))
            .header(header::HOST, SOCIAL_HOST);
        if let Some(cookie) = cookie {
            request = request.header(header::COOKIE, cookie);
        }
        let request = request.body(Body::empty()).expect("request builds");
        let srv = &srv;
        async move {
            let resp = srv.oneshot(request).await.expect("page responds");
            let status = resp.status();
            let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .expect("page body");
            (
                status,
                serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null),
            )
        }
    };
    let (status, rest) = next(Some(read)).await;
    assert_eq!(status, StatusCode::OK, "{rest}");
    assert_eq!(listed(&rest), slugs[20..]);
    assert!(rest.get("next_cursor").is_none_or(Value::is_null), "{rest}");
    let (status, _) = next(None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// A discovery read is granted only by a live discovery proof:
///   1. a contact proof and its flow cookie, presented as a discovery
///      flow, read nothing;
///   2. a discovery proof and its flow cookie, presented to contact
///      verification by a signed-in User, verify nothing;
///   3. once the read a proof granted has expired, the same browser
///      repeating that proof reads nothing.
///
/// # Panics
/// Panics when a proof of another purpose or past its read reads a page.
#[tokio::test(flavor = "multi_thread")]
async fn only_a_live_discovery_proof_reads_workspaces() {
    let (srv, operator) = contact_server("harbor").await;
    let api = relay_to_mailpit(&srv, &operator).await;
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    let known = "known.purpose@example.test";
    verify_admin_contact(&srv, &api, "harbor", known).await;
    assert_eq!(
        within_propagation(&1, || directory_entries(&superuser)).await,
        1
    );
    let renamed = |cookie: &str, to: &str| {
        let (_, value) = cookie.split_once('=').expect("a cookie pair");
        format!("{to}={value}")
    };

    // 1. A contact proof is not a discovery proof.
    let session = sign_in(&srv, "harbor").await;
    let candidate = "candidate.purpose@example.test";
    let (status, flow, body) = browser(
        &srv,
        "/auth/browser/email/request",
        &[&session],
        "wyrd_contact",
        &json!({ "tenant": "harbor", "email": candidate }),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    let contact_flow = flow.expect("a contact flow cookie");
    let (contact_proof, _) = mailed_proof(&api, candidate).await;
    let (status, read, body) = browser(
        &srv,
        "/auth/browser/workspaces/verify",
        &[&renamed(&contact_flow, "wyrd_discovery")],
        "wyrd_discovery",
        &json!({ "proof": contact_proof }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert!(read.is_none(), "no read cookie");

    // 2. A discovery proof is not a contact proof.
    let before = mail_to(&api, known).await.len();
    let (status, flow, _) = browser(
        &srv,
        "/auth/browser/workspaces/request",
        &[],
        "wyrd_discovery",
        &json!({ "email": known }),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let discovery_flow = flow.expect("a discovery flow cookie");
    let (discovery_proof, _) = next_proof(&api, known, before).await;
    let (status, _, body) = browser(
        &srv,
        "/auth/browser/email/verify",
        &[&session, &renamed(&discovery_flow, "wyrd_contact")],
        "wyrd_contact",
        &json!({ "tenant": "harbor", "proof": discovery_proof }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(
        contacts(&superuser, "harbor").await,
        [(known.to_owned(), 1)],
        "the contact is unchanged"
    );

    // 3. A spent proof past its read deadline reads nothing.
    let verify = |proof: String| {
        let (srv, discovery_flow) = (&srv, &discovery_flow);
        async move {
            browser(
                srv,
                "/auth/browser/workspaces/verify",
                &[discovery_flow],
                "wyrd_discovery",
                &json!({ "proof": proof }),
            )
            .await
        }
    };
    let (status, _, page) = verify(discovery_proof.clone()).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(workspaces(&page).len(), 1);
    sqlx::query(
        "UPDATE platform.discovery_proofs
            SET read_until = statement_timestamp() - interval '1 second'",
    )
    .execute(&superuser)
    .await
    .expect("the read expires");
    let (status, read, body) = verify(discovery_proof).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert!(read.is_none(), "no read cookie");
}

/// The tenants the directory holds entries for, in slug order.
async fn directory_tenants(superuser: &sqlx::PgPool) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT t.slug FROM platform.workspace_directory d
           JOIN platform.tenants t USING (data_tenant_id) ORDER BY t.slug",
    )
    .fetch_all(superuser)
    .await
    .expect("directory reads")
}

/// The directory heals after missed notifications, and discovery discloses
/// nothing it cannot revalidate:
///   1. with a lost entry and a stale one left behind, interrupting the
///      directory listener makes it resubscribe and resynchronize every
///      live tenant, restoring the lost entry and removing the stale one;
///   2. while the tenant contact cannot be read, discovery lists no
///      workspace, and lists it again once the read works.
///
/// # Panics
/// Panics when the directory does not heal or an unvalidated workspace is
/// listed.
#[tokio::test(flavor = "multi_thread")]
async fn the_directory_heals_and_discloses_only_what_it_revalidates() {
    let (srv, operator) = contact_server("harbor").await;
    add_tenant(&srv, &operator, "cove").await;
    let api = relay_to_mailpit(&srv, &operator).await;
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    let address = "healing@example.test";
    verify_admin_contact(&srv, &api, "harbor", address).await;
    let healed = vec!["harbor".to_owned()];
    assert_eq!(
        within_propagation(&healed, || directory_tenants(&superuser)).await,
        healed
    );

    // 1. Lose harbor's entry, leave a stale one for cove, interrupt the listener.
    sqlx::query("DELETE FROM platform.workspace_directory")
        .execute(&superuser)
        .await
        .expect("the entry is lost");
    sqlx::query(
        "INSERT INTO platform.workspace_directory
             (data_tenant_id, user_id, address_key, contact_revision)
         SELECT u.data_tenant_id, u.id, sha256('stale'::bytea), 1
           FROM wyrd.auth_users u JOIN platform.tenants t USING (data_tenant_id)
          WHERE t.slug = 'cove'",
    )
    .execute(&superuser)
    .await
    .expect("a stale entry stays");
    assert_eq!(directory_tenants(&superuser).await, ["cove"]);
    let interrupted: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM (
             SELECT pg_terminate_backend(pid) FROM pg_stat_activity
              WHERE datname = current_database() AND pid <> pg_backend_pid()
                AND query ILIKE '%LISTEN%wyrd_contact%'
         ) terminated",
    )
    .fetch_one(&superuser)
    .await
    .expect("the listener is interrupted");
    assert_eq!(interrupted, 1, "one directory listener");
    assert_eq!(
        within_propagation(&healed, || directory_tenants(&superuser)).await,
        healed,
        "resubscription resynchronizes"
    );
    let (status, page, _) = discover(&srv, &api, address).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(workspaces(&page).len(), 1);

    // 2. Revalidation that cannot read the contact lists nothing.
    sqlx::query("REVOKE SELECT ON wyrd.auth_user_contacts FROM wyrd_tenant")
        .execute(&superuser)
        .await
        .expect("contact reads fail");
    let (status, page, _) = discover(&srv, &api, address).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(workspaces(&page).is_empty(), "{page}");
    sqlx::query("GRANT SELECT ON wyrd.auth_user_contacts TO wyrd_tenant")
        .execute(&superuser)
        .await
        .expect("contact reads work");
    let (status, page, _) = discover(&srv, &api, address).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(workspaces(&page).len(), 1);
}
