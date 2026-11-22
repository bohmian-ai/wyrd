//! Public workspace signup journeys: source admission, proof, completion and
//! creation, against a real server and repository Postgres.

use std::net::{IpAddr, SocketAddr};

use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use serde_json::{Value, json};
use wyrd_testing::WyrdTestServer;

use super::{
    SOCIAL_HOST, body_json, complete_signup, enable_google, initialize_platform_root,
    mount_platform_provider, platform_key_set, platform_session, prove_signup, social_call,
    social_request,
};

/// The trusted ingress every journey here sits behind.
const INGRESS: &str = "10.0.0.0/8";

/// The header that ingress names the visitor in.
const CLIENT_HEADER: &str = "CF-Connecting-IP";

/// A server behind [`INGRESS`] with Google enabled for signup, and its
/// operator session.
///
/// # Panics
/// Panics when the server, operator session or provider setup fails.
async fn signup_server(google: &wiremock::MockServer) -> (WyrdTestServer, String) {
    let srv = WyrdTestServer::builder()
        .with_social_endpoints_for_test(wyrd_server::config::SocialConfig {
            google_issuer: Some(google.uri()),
            ..Default::default()
        })
        .with_trusted_ingress_for_test(&[INGRESS], CLIENT_HEADER)
        .start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;
    mount_platform_provider(
        google,
        platform_key_set("google-key"),
        wiremock::ResponseTemplate::new(500),
    )
    .await;
    enable_google(&srv, &session, google).await;
    let (status, changed) = social_call(
        &srv,
        social_request(
            Method::PUT,
            "/platform/login-settings",
            Some(&session),
            Some(json!({ "password_enabled": true, "signup_enabled": true })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "signup enables: {changed}");
    (srv, session)
}

/// A browser request from TCP `peer`, naming `client` in the ingress header
/// when given.
fn from_peer(
    method: Method,
    uri: &str,
    peer: &str,
    client: Option<&str>,
    body: &Value,
) -> Request<Body> {
    let peer: IpAddr = peer.parse().expect("peer address");
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, SOCIAL_HOST)
        .header(header::ORIGIN, format!("http://{SOCIAL_HOST}"))
        .header(header::CONTENT_TYPE, "application/json")
        .extension(axum::extract::ConnectInfo(SocketAddr::new(peer, 40_000)));
    if let Some(client) = client {
        builder = builder.header(CLIENT_HEADER, client);
    }
    builder
        .body(Body::from(serde_json::to_vec(body).expect("serializes")))
        .expect("request builds")
}

/// Begin a Google signup from `peer` naming `client`; returns the status,
/// response headers and body.
async fn start_signup(
    srv: &WyrdTestServer,
    peer: &str,
    client: Option<&str>,
) -> (StatusCode, HeaderMap, Value) {
    let resp = srv
        .oneshot(from_peer(
            Method::POST,
            "/auth/signup",
            peer,
            client,
            &json!({ "method": "social", "provider": "google" }),
        ))
        .await
        .expect("signup start responds");
    let status = resp.status();
    let headers = resp.headers().clone();
    (status, headers, body_json(resp).await)
}

/// Begin signups from `peer` naming `client` until one is throttled; returns
/// how many were admitted.
///
/// # Panics
/// Panics when a start fails other than by the throttle, or none is
/// throttled within a hundred.
async fn starts_until_throttled(srv: &WyrdTestServer, peer: &str, client: Option<&str>) -> usize {
    for admitted in 0..100 {
        let (status, _, body) = start_signup(srv, peer, client).await;
        if status == StatusCode::TOO_MANY_REQUESTS {
            assert_eq!(body["code"], "WYRD_AUTH_429_SIGNUP_ATTEMPTS", "{body}");
            return admitted;
        }
        assert_eq!(
            status,
            StatusCode::OK,
            "start {admitted} from {client:?}: {body}"
        );
    }
    panic!("no start from {client:?} was throttled");
}

/// Each visitor behind the trusted ingress is admitted in its own source
/// window:
///   1. two visitors named by the ingress exhaust separate windows;
///   2. a forwarding header from an untrusted peer is ignored: the peer is
///      the source, so naming an exhausted visitor neither borrows nor
///      spends its window;
///   3. IPv4-mapped IPv6 counts as its IPv4 visitor;
///   4. two addresses in one IPv6 `/64` share a window, another `/64` does
///      not;
///   5. with no determinable source, start and completion both refuse and
///      create nothing;
///   6. password sign-in failures are keyed by the same derivation in their
///      own windows.
///
/// # Panics
/// Panics when any step's response or stored records differ.
#[tokio::test(flavor = "multi_thread")]
async fn admission_keys_the_trusted_source() {
    let google = wiremock::MockServer::start().await;
    let (srv, _session) = signup_server(&google).await;
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");

    // 1. Two visitors behind the ingress get separate windows.
    let window = starts_until_throttled(&srv, "10.1.0.1", Some("198.51.100.1")).await;
    assert!(window > 0, "a fresh window admits starts");
    let (status, _, body) = start_signup(&srv, "10.1.0.2", Some("198.51.100.2")).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "another visitor is admitted: {body}"
    );

    // 2. An untrusted peer's header is ignored.
    let (status, _, body) = start_signup(&srv, "203.0.113.5", Some("198.51.100.2")).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the untrusted peer is its own source: {body}"
    );
    assert_eq!(
        starts_until_throttled(&srv, "203.0.113.5", Some("198.51.100.1")).await,
        window - 1,
        "the untrusted peer spends only its own window"
    );

    // 3. Mapped IPv4 is the IPv4 visitor.
    let (status, _, _) = start_signup(&srv, "10.1.0.1", Some("::ffff:198.51.100.1")).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);

    // 4. One IPv6 /64 shares a window.
    assert_eq!(
        starts_until_throttled(&srv, "10.1.0.1", Some("2001:db8:1:2::1")).await,
        window
    );
    let (status, _, _) = start_signup(&srv, "10.1.0.1", Some("2001:db8:1:2:ffff::9")).await;
    assert_eq!(
        status,
        StatusCode::TOO_MANY_REQUESTS,
        "the same /64 is throttled"
    );
    let (status, _, body) = start_signup(&srv, "10.1.0.1", Some("2001:db8:1:3::1")).await;
    assert_eq!(status, StatusCode::OK, "another /64 is admitted: {body}");

    // 5. No determinable source refuses start and completion.
    for client in [None, Some("not-an-address")] {
        let (status, _, body) = start_signup(&srv, "10.1.0.1", client).await;
        assert_eq!(
            status,
            StatusCode::SERVICE_UNAVAILABLE,
            "{client:?}: {body}"
        );
        assert_eq!(body["details"]["reason"], "source_unknown", "{body}");
    }
    let proof = prove_signup(&srv, &google, "google-nosource").await;
    let (status, body, cookie) = complete_signup(&srv, &proof, "nosource", [10, 9, 9, 9]).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["details"]["reason"], "source_unknown", "{body}");
    assert!(cookie.is_none());
    let created: i64 =
        sqlx::query_scalar("SELECT count(*) FROM platform.tenants WHERE slug = 'nosource'")
            .fetch_one(&superuser)
            .await
            .expect("directory reads");
    assert_eq!(created, 0, "an unknown source creates no tenant");
    let (status, body, _) = complete_signup(&srv, &proof, "nosource", [192, 0, 2, 44]).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the same proof completes from a known source: {body}"
    );

    // 6. Password failures use the same derivation, in their own windows.
    let tenant = srv.pg_fixture().tenant_slug().to_owned();
    for (peer, client) in [
        ("10.1.0.1", Some("198.51.100.1")),
        ("10.1.0.2", Some("198.51.100.2")),
        ("203.0.113.5", Some("198.51.100.1")),
        ("10.1.0.1", Some("2001:db8:1:2::1")),
        ("10.1.0.1", Some("2001:db8:1:2:ffff::9")),
    ] {
        let resp = srv
            .oneshot(from_peer(
                Method::POST,
                "/auth/browser/password/login",
                peer,
                client,
                &json!({ "tenant": tenant, "username": "nobody", "password": "not the password at all" }),
            ))
            .await
            .expect("login responds");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "{peer} {client:?}");
    }
    let sources: Vec<(String, i32)> = sqlx::query_as(
        "SELECT subject, failures FROM wyrd.auth_password_attempts \
         WHERE scope = 'source' ORDER BY subject",
    )
    .fetch_all(&superuser)
    .await
    .expect("attempts read");
    assert_eq!(
        sources,
        vec![
            ("198.51.100.1".to_owned(), 1),
            ("198.51.100.2".to_owned(), 1),
            ("2001:db8:1:2::/64".to_owned(), 2),
            ("203.0.113.5".to_owned(), 1),
        ],
        "password source windows follow the trusted source and /64"
    );
}
