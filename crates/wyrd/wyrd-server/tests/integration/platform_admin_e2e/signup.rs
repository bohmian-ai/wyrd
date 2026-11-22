//! Public workspace signup journeys: source admission, proof, completion and
//! creation, against a real server and repository Postgres.

use std::net::{IpAddr, SocketAddr};

use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use serde_json::{Value, json};
use wyrd_testing::WyrdTestServer;

use super::{
    SOCIAL_HOST, begin_social_at, body_json, complete_signup, enable_google, identity_token_reply,
    initialize_platform_root, mail_to, mailpit, mount_platform_provider, platform_key_set,
    platform_session, prove_signup, social_call, social_callback, social_request,
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

/// The whole-second wait a throttled response states, asserted equal in its
/// `Retry-After` header and its problem details.
///
/// # Panics
/// Panics when either is missing or they differ.
fn stated_wait(headers: &HeaderMap, body: &Value) -> u64 {
    let header = headers
        .get(header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or_else(|| panic!("a throttle carries a whole-second Retry-After: {body}"));
    assert_eq!(
        Some(header),
        body["details"]["retry_after_seconds"].as_u64(),
        "Retry-After equals details.retry_after_seconds: {body}"
    );
    assert!(header > 0);
    header
}

/// Complete `proof` as `slug` from visitor `client` behind the ingress;
/// returns the status, headers and body.
async fn complete_from(
    srv: &WyrdTestServer,
    proof: &str,
    slug: &str,
    client: &str,
) -> (StatusCode, HeaderMap, Value) {
    let mut request = from_peer(
        Method::POST,
        "/auth/signup/complete",
        "10.1.0.1",
        Some(client),
        &json!({ "slug": slug, "display_name": "Throttled" }),
    );
    request.headers_mut().insert(
        header::COOKIE,
        axum::http::HeaderValue::from_str(proof).expect("cookie value"),
    );
    let resp = srv.oneshot(request).await.expect("completion responds");
    let status = resp.status();
    let headers = resp.headers().clone();
    (status, headers, body_json(resp).await)
}

/// Every signup throttle answers `429` with a whole-second `Retry-After`
/// equal to `details.retry_after_seconds`:
///   1. the start source window;
///   2. the completion identity window;
///   3. the completion source window;
///   4. unavailable admission state refuses start rather than admitting it.
///
/// # Panics
/// Panics when a throttle lacks or misstates its wait, or unavailable state
/// admits.
#[tokio::test(flavor = "multi_thread")]
async fn every_signup_throttle_carries_retry_after() {
    let google = wiremock::MockServer::start().await;
    let (srv, _session) = signup_server(&google).await;
    let taken = srv.pg_fixture().tenant_slug().to_owned();

    // 1. Start.
    starts_until_throttled(&srv, "10.1.0.1", Some("198.51.100.20")).await;
    let (status, headers, body) = start_signup(&srv, "10.1.0.1", Some("198.51.100.20")).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    stated_wait(&headers, &body);

    // 2. One proven identity retrying a taken slug exhausts its window.
    let proof = prove_signup(&srv, &google, "google-retrier").await;
    let throttled = loop {
        let (status, headers, body) = complete_from(&srv, &proof, &taken, "198.51.100.21").await;
        if status == StatusCode::TOO_MANY_REQUESTS {
            break (headers, body);
        }
        assert_eq!(status, StatusCode::CONFLICT, "a taken slug: {body}");
    };
    assert_eq!(throttled.1["code"], "WYRD_AUTH_429_SIGNUP_ATTEMPTS");
    stated_wait(&throttled.0, &throttled.1);

    // 3. Many identities from one visitor exhaust its source window.
    let mut source_throttle = None;
    for subject in 0..10 {
        let proof = prove_signup(&srv, &google, &format!("google-flood-{subject}")).await;
        for _ in 0..4 {
            let (status, headers, body) =
                complete_from(&srv, &proof, &taken, "198.51.100.22").await;
            if status == StatusCode::TOO_MANY_REQUESTS {
                source_throttle = Some((headers, body));
                break;
            }
            assert_eq!(status, StatusCode::CONFLICT, "a taken slug: {body}");
        }
        if source_throttle.is_some() {
            break;
        }
    }
    let (headers, body) = source_throttle.expect("the source window is exhausted");
    stated_wait(&headers, &body);

    // 4. Unavailable admission state refuses start.
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    sqlx::query("ALTER TABLE platform.signup_admissions RENAME TO signup_admissions_down")
        .execute(&superuser)
        .await
        .expect("admission state goes away");
    let (status, _, body) = start_signup(&srv, "10.1.0.1", Some("198.51.100.23")).await;
    sqlx::query("ALTER TABLE platform.signup_admissions_down RENAME TO signup_admissions")
        .execute(&superuser)
        .await
        .expect("admission state returns");
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["code"], "WYRD_AUTH_503_VERIFY_UNAVAILABLE", "{body}");
}

/// `(scope, stored key as text, key length in bytes)` for every admission
/// window.
async fn admission_keys(superuser: &sqlx::PgPool) -> Vec<(String, String, i32)> {
    sqlx::query_as("SELECT scope, key::text, octet_length(key) FROM platform.signup_admissions")
        .fetch_all(superuser)
        .await
        .expect("admission windows read")
}

/// The single count `sql` selects.
async fn count(superuser: &sqlx::PgPool, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql)
        .fetch_one(superuser)
        .await
        .expect("rows count")
}

/// Signup admission and creator accounting store keyed pseudonyms, and
/// expired signup state leaves only through bounded maintenance batches:
///   1. start, source and identity windows hold 32-byte HMACs, never an
///      address, issuer or subject;
///   2. the created tenant records a 32-byte creator pseudonym;
///   3. a public start purges nothing;
///   4. each maintenance batch removes at most 500 windows and 500 attempts,
///      expired ones only, leaving live windows;
///   5. the creator pseudonym survives its attempt's purge.
///
/// # Panics
/// Panics when a key is plain, the purge is unbounded or runs on a public
/// request, or creator accounting is lost.
#[tokio::test(flavor = "multi_thread")]
async fn signup_state_is_pseudonymous_and_purged_in_batches() {
    let google = wiremock::MockServer::start().await;
    let (srv, _session) = signup_server(&google).await;
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");

    // 1. Every window the journey opens is a pseudonym.
    let (status, _, body) = start_signup(&srv, "10.1.0.1", Some("198.51.100.40")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let proof = prove_signup(&srv, &google, "google-pseudonym").await;
    let (status, created, _) = complete_signup(&srv, &proof, "pseudonym-co", [192, 0, 2, 40]).await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let keys = admission_keys(&superuser).await;
    let scopes: std::collections::BTreeSet<&str> =
        keys.iter().map(|(scope, _, _)| scope.as_str()).collect();
    assert_eq!(scopes, ["identity", "source", "start"].into());
    let google_host = url::Url::parse(&google.uri())
        .expect("mock url")
        .host_str()
        .expect("mock host")
        .to_owned();
    for (scope, key, len) in &keys {
        assert_eq!(*len, 32, "{scope} key is an HMAC-SHA-256: {key}");
        for plain in [
            "198.51.100.40",
            "192.0.2.40",
            "127.0.0.1",
            "google-pseudonym",
        ] {
            assert!(!key.contains(plain), "{scope} key {key} holds {plain}");
        }
        assert!(
            !key.contains(&google_host),
            "{scope} key {key} holds the issuer"
        );
    }

    // 2. The tenant keeps its creator pseudonym.
    let creator: Option<i32> = sqlx::query_scalar(
        "SELECT octet_length(signup_creator_key) FROM platform.tenants WHERE slug = 'pseudonym-co'",
    )
    .fetch_one(&superuser)
    .await
    .expect("creator key reads");
    assert_eq!(creator, Some(32));

    // 3. Expired state survives a public start.
    sqlx::query(
        "INSERT INTO platform.signup_admissions (scope, key, window_start, attempts)
         SELECT 'source', sha256(convert_to(n::text, 'UTF8')),
                statement_timestamp() - interval '2 days', 1
           FROM generate_series(1, 600) AS n",
    )
    .execute(&superuser)
    .await
    .expect("expired windows insert");
    sqlx::query(
        "INSERT INTO platform.signup_attempts
             (id, proof_kind, proof_hash, browser_binding_hash, completion_hash, email,
              expires_at)
         SELECT gen_random_uuid(), 'email', sha256(convert_to('proof' || n, 'UTF8')),
                sha256(convert_to('binding' || n, 'UTF8')),
                sha256(convert_to('completion' || n, 'UTF8')), n || '@example.test',
                statement_timestamp() - interval '1 hour'
           FROM generate_series(1, 600) AS n",
    )
    .execute(&superuser)
    .await
    .expect("expired attempts insert");
    sqlx::query(
        "UPDATE platform.signup_attempts SET expires_at = statement_timestamp() - interval '1 minute'
          WHERE id = (SELECT signup_attempt_id FROM platform.tenants WHERE slug = 'pseudonym-co')",
    )
    .execute(&superuser)
    .await
    .expect("completed attempt expires");
    let live_windows = count(
        &superuser,
        "SELECT count(*) FROM platform.signup_admissions
          WHERE window_start > statement_timestamp() - interval '1 hour'",
    )
    .await;
    let (status, _, body) = start_signup(&srv, "10.1.0.1", Some("198.51.100.41")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let expired_attempts = "SELECT count(*) FROM platform.signup_attempts
                             WHERE expires_at <= statement_timestamp()";
    assert_eq!(
        count(&superuser, expired_attempts).await,
        601,
        "a public start deletes nothing"
    );

    // 4. Maintenance removes expired state one bounded batch at a time.
    let social = srv
        .state()
        .auth
        .social_sign_in
        .as_ref()
        .expect("social sign-in is built");
    let mut batches = Vec::new();
    loop {
        let removed = social.purge_expired_signups().await.expect("purge runs");
        batches.push(removed);
        if removed == 0 {
            break;
        }
    }
    assert_eq!(batches, [1000, 201, 0], "two bounded batches, then nothing");
    assert_eq!(count(&superuser, expired_attempts).await, 0);
    assert_eq!(
        count(
            &superuser,
            "SELECT count(*) FROM platform.signup_admissions"
        )
        .await,
        live_windows + 1,
        "live windows, and the new start's, remain"
    );
    assert_eq!(
        count(
            &superuser,
            "SELECT count(*) FROM platform.signup_attempts
              WHERE expires_at > statement_timestamp()"
        )
        .await,
        0,
        "starts stored no attempt to keep"
    );

    // 5. Creator accounting outlives the attempt.
    let (attempt, creator): (Option<uuid::Uuid>, Option<i32>) = sqlx::query_as(
        "SELECT signup_attempt_id, octet_length(signup_creator_key)
           FROM platform.tenants WHERE slug = 'pseudonym-co'",
    )
    .fetch_one(&superuser)
    .await
    .expect("tenant reads");
    assert_eq!((attempt, creator), (None, Some(32)));
}

/// The provider discovery requests `google` has received.
async fn discoveries(google: &wiremock::MockServer) -> usize {
    google
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path().ends_with("/openid-configuration"))
        .count()
}

/// A signup start keeps its whole provider round-trip in the browser:
///   1. a start writes no signup row; state, verifier and nonce ride a
///      sealed, HttpOnly cookie scoped to the social callbacks;
///   2. a tampered cookie proves nothing;
///   3. of two concurrent callbacks with one cookie exactly one records a
///      proof, and a replay afterwards records none;
///   4. a throttled start is refused before provider discovery;
///   5. unavailable admission state refuses a start before any work.
///
/// # Panics
/// Panics when a start writes a row, the cookie is readable, a callback
/// proves twice or a refused start reaches the provider.
#[tokio::test(flavor = "multi_thread")]
async fn signup_start_keeps_its_round_trip_in_a_sealed_cookie() {
    let google = wiremock::MockServer::start().await;
    let (srv, _session) = signup_server(&google).await;
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    let attempts = "SELECT count(*) FROM platform.signup_attempts";
    let start = json!({ "method": "social", "provider": "google" });

    // 1. Nothing is stored; the cookie is sealed.
    let begun = begin_social_at(&srv, "/auth/signup", start.clone()).await;
    assert_eq!(
        count(&superuser, attempts).await,
        0,
        "a start writes no row"
    );
    for attribute in [
        "HttpOnly",
        "Path=/auth/social",
        "Max-Age=300",
        "SameSite=Lax",
    ] {
        assert!(begun.set_cookie.contains(attribute), "{}", begun.set_cookie);
    }
    let (_, sealed) = begun.cookie.split_once('=').expect("cookie pair");
    for plain in [&begun.query["state"], &begun.query["nonce"]] {
        assert!(!sealed.contains(plain.as_str()), "the cookie hides {plain}");
    }

    // 2. A tampered cookie proves nothing.
    mount_platform_provider(
        &google,
        platform_key_set("google-key"),
        identity_token_reply(
            &google.uri(),
            &begun.query["nonce"],
            "wyrd-google",
            "google-key",
            "google-sealed",
            "sealed@example.test",
        ),
    )
    .await;
    let callback = |cookie: String| {
        let srv = &srv;
        let uri = social_callback(&begun.query["state"], &google.uri());
        async move {
            let resp = srv
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header(header::HOST, SOCIAL_HOST)
                        .header(header::COOKIE, cookie)
                        .body(Body::empty())
                        .expect("request builds"),
                )
                .await
                .expect("callback responds");
            resp.headers()
                .get_all(header::SET_COOKIE)
                .iter()
                .filter_map(|value| value.to_str().ok())
                .any(|value| {
                    value.starts_with("wyrd_signup=") && !value.starts_with("wyrd_signup=;")
                })
        }
    };
    let mut tampered = sealed.to_owned();
    let flipped = if tampered.ends_with('A') { 'B' } else { 'A' };
    tampered.pop();
    tampered.push(flipped);
    assert!(
        !callback(format!("wyrd_login={tampered}")).await,
        "tampered"
    );
    assert_eq!(
        count(&superuser, attempts).await,
        0,
        "a tampered cookie proves nothing"
    );

    // 3. One cookie proves once, however its callbacks race or replay.
    let (first, second) = tokio::join!(
        callback(begun.cookie.clone()),
        callback(begun.cookie.clone())
    );
    assert!(
        first ^ second,
        "exactly one concurrent callback proves: {first} {second}"
    );
    assert!(
        !callback(begun.cookie.clone()).await,
        "a replay proves nothing"
    );
    assert_eq!(count(&superuser, attempts).await, 1, "one proof per state");

    // 4. A throttled start does no provider work.
    starts_until_throttled(&srv, "10.6.0.1", Some("198.51.100.60")).await;
    let before = discoveries(&google).await;
    let (status, _, body) = start_signup(&srv, "10.6.0.1", Some("198.51.100.60")).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    assert_eq!(
        discoveries(&google).await,
        before,
        "refused before discovery"
    );

    // 5. Admission state that cannot be read refuses before any work.
    sqlx::query("ALTER TABLE platform.signup_admissions RENAME TO signup_admissions_away")
        .execute(&superuser)
        .await
        .expect("admission state goes away");
    let (status, _, body) = start_signup(&srv, "10.6.0.2", Some("198.51.100.61")).await;
    sqlx::query("ALTER TABLE platform.signup_admissions_away RENAME TO signup_admissions")
        .execute(&superuser)
        .await
        .expect("admission state returns");
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(
        discoveries(&google).await,
        before,
        "refused before discovery"
    );
    assert_eq!(count(&superuser, attempts).await, 1);
}

/// A browser request from `peer` carrying `cookie`; returns the status, the
/// `name` cookie it set and the body.
async fn email_step(
    srv: &WyrdTestServer,
    uri: &str,
    peer: &str,
    cookie: Option<&str>,
    name: &str,
    body: &Value,
) -> (StatusCode, Option<String>, Value) {
    let mut request = from_peer(Method::POST, uri, "10.7.0.1", Some(peer), body);
    if let Some(cookie) = cookie {
        request
            .headers_mut()
            .insert(header::COOKIE, cookie.parse().expect("cookie header"));
    }
    let resp = srv.oneshot(request).await.expect("signup step responds");
    let status = resp.status();
    let prefix = format!("{name}=");
    let set = resp
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with(&prefix))
        .map(str::to_owned);
    (status, set, body_json(resp).await)
}

/// The proof the newest signup message to `address` carries in its link
/// fragment, and that message's text.
async fn mailed_proof(api: &str, address: &str) -> (String, String) {
    let mail = mail_to(api, address).await;
    let (_, _, _, text) = mail.first().expect("a signup message arrived");
    let proof = text
        .split("#proof=")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .expect("the message links its proof in a fragment")
        .to_owned();
    (proof, text.clone())
}

/// A visitor proves a mailbox in the browser that asked, and nowhere else:
///   1. without a mail relay, email signup is not offered and nothing is
///      sent;
///   2. a start answers `check_email`, stores nothing, sets a sealed
///      HttpOnly flow cookie for `/auth/signup`, and mails the trimmed
///      address a fragment link holding a proof the cookie does not hold;
///   3. a wrong proof, or the right proof without the flow cookie, proves
///      nothing;
///   4. the right proof in the asking browser records one email proof under
///      the canonical address and sets the completion cookie;
///   5. the same browser repeating it, as after a lost response, gets a
///      fresh completion cookie and no second proof; a later flow for the
///      address cannot use the spent proof;
///   6. an address's own budget refuses further starts from fresh sources
///      before any mail is sent.
///
/// # Panics
/// Panics when a step proves, stores or mails differently.
#[tokio::test(flavor = "multi_thread")]
async fn email_signup_proves_a_mailbox_in_the_browser_that_asked() {
    let google = wiremock::MockServer::start().await;
    let (srv, session) = signup_server(&google).await;
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    let attempts = "SELECT count(*) FROM platform.signup_attempts";
    let local = format!("Ada.Lovelace+{}", uuid::Uuid::new_v4().simple());
    let typed = format!("  {local}@Example.TEST ");
    let canonical = format!("{local}@example.test");
    let start = json!({ "method": "email", "email": typed });
    let (host, port, api) = mailpit();

    // 1. No relay, no email signup.
    let (status, cookie, body) = email_step(
        &srv,
        "/auth/signup",
        "198.51.100.70",
        None,
        "wyrd_signup_email",
        &start,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert!(cookie.is_none());
    assert!(
        mail_to(&api, &canonical).await.is_empty(),
        "nothing is sent"
    );
    let (status, relay) = social_call(
        &srv,
        social_request(
            Method::PUT,
            "/platform/mail",
            Some(&session),
            Some(
                json!({ "host": host, "port": port, "tls": "none", "sender": "signup@wyrd.test" }),
            ),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{relay}");

    // 2. A start stores nothing and mails a fragment-linked proof.
    let (status, flow, body) = email_step(
        &srv,
        "/auth/signup",
        "198.51.100.70",
        None,
        "wyrd_signup_email",
        &start,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert_eq!(body, json!({ "status": "check_email" }));
    let flow = flow.expect("the flow cookie is set");
    for attribute in [
        "HttpOnly",
        "Path=/auth/signup",
        "Max-Age=900",
        "SameSite=Lax",
    ] {
        assert!(flow.contains(attribute), "{flow}");
    }
    let flow = flow.split(';').next().expect("cookie pair").to_owned();
    assert_eq!(
        count(&superuser, attempts).await,
        0,
        "a start writes no row"
    );
    let (proof, text) = mailed_proof(&api, &canonical).await;
    assert_eq!(
        mail_to(&api, &canonical).await[0].0,
        [format!("{local}@Example.TEST")],
        "delivery keeps the address as typed, trimmed"
    );
    assert!(
        text.contains("http://wyrd.test/signup/verify#proof="),
        "{text}"
    );
    assert!(!flow.contains(&proof), "the cookie does not hold the proof");

    // 3. A wrong proof, or no flow cookie, proves nothing.
    let verify = |proof: &str| json!({ "proof": proof });
    for (cookie, sent) in [
        (Some(flow.as_str()), "not-the-proof"),
        (None, proof.as_str()),
    ] {
        let (status, completion, body) = email_step(
            &srv,
            "/auth/signup/verify",
            "198.51.100.70",
            cookie,
            "wyrd_signup",
            &verify(sent),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
        assert_eq!(
            body["code"], "WYRD_AUTH_401_ONE_TIME_CODE_INVALID",
            "{body}"
        );
        assert!(completion.is_none());
    }
    assert_eq!(count(&superuser, attempts).await, 0);

    // 4. The asking browser proves the mailbox once.
    let (status, completion, body) = email_step(
        &srv,
        "/auth/signup/verify",
        "198.51.100.70",
        Some(&flow),
        "wyrd_signup",
        &verify(&proof),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, json!({ "status": "proven" }));
    let completion = completion.expect("the completion cookie is set");
    assert!(
        completion.contains("Path=/auth/signup") && completion.contains("HttpOnly"),
        "{completion}"
    );
    let proven: Vec<(String, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT proof_kind, email, provider FROM platform.signup_attempts")
            .fetch_all(&superuser)
            .await
            .expect("attempts read");
    assert_eq!(
        proven,
        [("email".to_owned(), Some(canonical.clone()), None)]
    );

    // 5. A lost response resumes in the same browser; the spent proof is
    // useless to a later flow.
    let (status, again, body) = email_step(
        &srv,
        "/auth/signup/verify",
        "198.51.100.70",
        Some(&flow),
        "wyrd_signup",
        &verify(&proof),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_ne!(again.expect("a fresh completion cookie"), completion);
    assert_eq!(count(&superuser, attempts).await, 1, "no second proof");
    let (status, later, body) = email_step(
        &srv,
        "/auth/signup",
        "198.51.100.71",
        None,
        "wyrd_signup_email",
        &start,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    let later = later
        .expect("flow cookie")
        .split(';')
        .next()
        .expect("pair")
        .to_owned();
    let (status, _, body) = email_step(
        &srv,
        "/auth/signup/verify",
        "198.51.100.71",
        Some(&later),
        "wyrd_signup",
        &verify(&proof),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");

    // 6. The address's budget holds across sources, before SMTP.
    let mut admitted = 2;
    let throttled = loop {
        let (status, _, body) = email_step(
            &srv,
            "/auth/signup",
            &format!("198.51.100.{}", 80 + admitted),
            None,
            "wyrd_signup_email",
            &start,
        )
        .await;
        if status == StatusCode::TOO_MANY_REQUESTS {
            break body;
        }
        assert_eq!(status, StatusCode::ACCEPTED, "{body}");
        admitted += 1;
        assert!(admitted < 50, "the address is never throttled");
    };
    assert_eq!(
        throttled["code"], "WYRD_AUTH_429_SIGNUP_ATTEMPTS",
        "{throttled}"
    );
    assert_eq!(
        mail_to(&api, &canonical).await.len(),
        admitted,
        "every admitted start mailed once, the refused one not at all"
    );
}
