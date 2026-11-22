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
    signup_server_with(
        google,
        wyrd_server::config::SocialConfig {
            google_issuer: Some(google.uri()),
            ..Default::default()
        },
    )
    .await
}

/// [`signup_server`] with the provider endpoints `social`.
///
/// # Panics
/// As [`signup_server`].
async fn signup_server_with(
    google: &wiremock::MockServer,
    social: wyrd_server::config::SocialConfig,
) -> (WyrdTestServer, String) {
    let srv = WyrdTestServer::builder()
        .with_social_endpoints_for_test(social)
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

/// Point the deployment's mail relay at the test Mailpit; returns its API.
async fn relay_to_mailpit(srv: &WyrdTestServer, session: &str) -> String {
    let (host, port, api) = mailpit();
    let (status, relay) = social_call(
        srv,
        social_request(
            Method::PUT,
            "/platform/mail",
            Some(session),
            Some(
                json!({ "host": host, "port": port, "tls": "none", "sender": "signup@wyrd.test" }),
            ),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{relay}");
    api
}

/// Prove `address` by email from visitor `client`; returns the completion
/// cookie pair and its `Set-Cookie` attributes.
async fn email_proof(
    srv: &WyrdTestServer,
    api: &str,
    address: &str,
    client: &str,
) -> (String, String) {
    let (status, flow, body) = email_step(
        srv,
        "/auth/signup",
        client,
        None,
        "wyrd_signup_email",
        &json!({ "method": "email", "email": address }),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    let flow = flow.expect("flow cookie");
    let flow = flow.split(';').next().expect("pair");
    let (proof, _) = mailed_proof(api, address).await;
    let (status, completion, body) = email_step(
        srv,
        "/auth/signup/verify",
        client,
        Some(flow),
        "wyrd_signup",
        &json!({ "proof": proof }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let attributes = completion.expect("completion cookie");
    (
        attributes.split(';').next().expect("pair").to_owned(),
        attributes,
    )
}

/// Complete with the completion cookie `proof` and `body` from `client`;
/// returns the status, the `slug` session cookie it set and the body.
async fn complete_with(
    srv: &WyrdTestServer,
    proof: &str,
    client: &str,
    body: &Value,
) -> (StatusCode, Option<String>, Value) {
    let slug = body["slug"].as_str().unwrap_or_default().to_owned();
    email_step(
        srv,
        "/auth/signup/complete",
        client,
        Some(proof),
        &format!("wyrd_session_{slug}"),
        body,
    )
    .await
}

/// The status of every directory row named `slug`.
async fn statuses(superuser: &sqlx::PgPool, slug: &str) -> Vec<String> {
    sqlx::query_scalar("SELECT status FROM platform.tenants WHERE slug = $1")
        .bind(slug)
        .fetch_all(superuser)
        .await
        .expect("directory reads")
}

/// Completion consumes either proof kind through the one claim path and
/// recovers only its own interrupted claim:
///   1. a completion cookie lasts the five-minute completion window;
///   2. an email proof needs local credentials inside policy, and a social
///      proof refuses them; each refusal creates nothing;
///   3. an email proof with credentials creates an active workspace whose
///      local User is its administrator, signs it in with a password
///      session, and clears the attempt's email;
///   4. replaying the spent completion names the same workspace, opens no
///      session and creates nothing; a resumed creation retried under
///      another username keeps the one original administrator;
///   5. a claim interrupted in `creating` beyond the two-minute staleness bound resumes
///      under its own tenant id only with its own completion cookie; another
///      proof cannot adopt it, and a recent one still reads as in progress;
///   6. an expired proof cannot restart its interrupted claim, which stays
///      unactivated.
///
/// # Panics
/// Panics when a completion creates, refuses or resumes differently.
#[tokio::test(flavor = "multi_thread")]
async fn completion_consumes_either_proof_and_recovers_its_own_claim() {
    let google = wiremock::MockServer::start().await;
    let (srv, session) = signup_server(&google).await;
    let api = relay_to_mailpit(&srv, &session).await;
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    let address = format!("owner-{}@example.test", uuid::Uuid::new_v4().simple());

    // 1. The completion window is five minutes.
    let (local, attributes) = email_proof(&srv, &api, &address, "198.51.100.90").await;
    assert!(attributes.contains("Max-Age=300"), "{attributes}");

    // 2. Credentials follow the proof kind and the password policy.
    let named = |extra: Value| {
        let mut body = json!({ "slug": "harbor", "display_name": "Harbor" });
        if let (Some(body), Some(extra)) = (body.as_object_mut(), extra.as_object()) {
            body.extend(extra.clone());
        }
        body
    };
    let credentials = json!({ "local_credentials": {
        "username": "ada", "password": "quiet harbor lantern" } });
    for (case, body) in [
        ("no credentials", named(json!({}))),
        (
            "short password",
            named(
                json!({ "local_credentials": { "username": "ada", "password": "amber kestrel9" } }),
            ),
        ),
    ] {
        let (status, cookie, body) = complete_with(&srv, &local, "198.51.100.90", &body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{case}: {body}");
        assert_eq!(body["code"], "WYRD_SPEC_400_VALIDATION", "{case}");
        assert!(cookie.is_none(), "{case}");
    }
    let social = prove_signup(&srv, &google, "google-harbor").await;
    let (status, _, body) =
        complete_with(&srv, &social, "198.51.100.91", &named(credentials.clone())).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "social with credentials: {body}"
    );
    assert_eq!(body["details"]["field"], "local_credentials", "{body}");
    assert!(
        statuses(&superuser, "harbor").await.is_empty(),
        "nothing was created"
    );

    // 3. The email proof creates a workspace its local User administers.
    let (status, cookie, body) =
        complete_with(&srv, &local, "198.51.100.90", &named(credentials.clone())).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, json!({ "tenant": "harbor" }));
    assert!(cookie.is_some(), "the creator is signed in");
    assert_eq!(statuses(&superuser, "harbor").await, ["active"]);
    let (status, signed_in, body) = email_step(
        &srv,
        "/auth/browser/password/login",
        "198.51.100.90",
        None,
        "wyrd_session_harbor",
        &json!({ "tenant": "harbor", "username": "ada", "password": "quiet harbor lantern" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(signed_in.is_some());
    assert_eq!(body["credential"], "password");
    assert_eq!(
        body["permissions"],
        json!(["wildcard:wildcard"]),
        "the creator administers the workspace"
    );
    let cleared: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT status, email FROM platform.signup_attempts WHERE proof_kind = 'email'",
    )
    .fetch_all(&superuser)
    .await
    .expect("attempt reads");
    assert_eq!(cleared, [("completed".to_owned(), None)]);

    // 4. A replay names the same workspace and creates nothing.
    let (status, again, body) =
        complete_with(&srv, &local, "198.51.100.90", &named(credentials.clone())).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["details"]["tenant"], "harbor", "{body}");
    assert!(again.is_none(), "a replay opens no session");
    assert_eq!(statuses(&superuser, "harbor").await, ["active"]);

    // 4b. A retry under another username keeps the original administrator.
    sqlx::query(
        "WITH a AS (UPDATE platform.signup_attempts SET status = 'failed', email = $1
                     WHERE proof_kind = 'email' RETURNING id)
         UPDATE platform.tenants SET status = 'failed', provisioning_failed_reason = 'interrupted'
          WHERE slug = 'harbor' AND signup_attempt_id IN (SELECT id FROM a)",
    )
    .bind(&address)
    .execute(&superuser)
    .await
    .expect("the completed creation is reopened as interrupted");
    let retry = json!({ "slug": "harbor", "display_name": "Harbor", "local_credentials": {
        "username": "grace", "password": "lantern quiet harbor" } });
    let (status, resumed, body) = complete_with(&srv, &local, "198.51.100.90", &retry).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(resumed.is_some());
    let users: Vec<String> = sqlx::query_scalar(
        "SELECT u.username FROM wyrd.auth_users u JOIN platform.tenants t
             ON t.data_tenant_id = u.data_tenant_id
          WHERE t.slug = 'harbor' AND u.password_hash IS NOT NULL",
    )
    .fetch_all(&superuser)
    .await
    .expect("users read");
    assert_eq!(users, ["ada"], "one administrator, under its original name");
    let (status, _, body) = email_step(
        &srv,
        "/auth/browser/password/login",
        "198.51.100.90",
        None,
        "wyrd_session_harbor",
        &json!({ "tenant": "harbor", "username": "ada", "password": "lantern quiet harbor" }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the retry's password signs in: {body}"
    );

    // 5. An interrupted claim resumes only for its own attempt.
    let erin = prove_signup(&srv, &google, "google-erin").await;
    let other = prove_signup(&srv, &google, "google-other").await;
    let interrupted = uuid::Uuid::now_v7();
    sqlx::query(
        "WITH a AS (UPDATE platform.signup_attempts
                       SET status = 'creating', updated_at = statement_timestamp() - interval '1 minute'
                     WHERE subject = 'google-erin' RETURNING id)
         INSERT INTO platform.tenants
             (data_tenant_id, slug, display_name, status, signup_attempt_id, signup_creator_key,
              updated_at)
         SELECT $1, 'erin', 'Erin', 'provisioning', id, sha256(convert_to('erin', 'UTF8')),
                now() - interval '1 minute' FROM a",
    )
    .bind(interrupted)
    .execute(&superuser)
    .await
    .expect("an interrupted creation is staged");
    let erin_body = json!({ "slug": "erin", "display_name": "Erin" });
    let (status, _, body) = complete_with(&srv, &erin, "198.51.100.92", &erin_body).await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "a recent claim is in progress: {body}"
    );
    assert_eq!(body["details"]["reason"], "signup_in_progress", "{body}");
    sqlx::raw_sql(
        "UPDATE platform.signup_attempts SET updated_at = statement_timestamp() - interval '3 minutes'
          WHERE subject = 'google-erin';
         UPDATE platform.tenants SET updated_at = now() - interval '3 minutes' WHERE slug = 'erin'",
    )
    .execute(&superuser)
    .await
    .expect("the claim goes stale");
    let (status, _, body) = complete_with(&srv, &other, "198.51.100.93", &erin_body).await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "another proof cannot adopt it: {body}"
    );
    assert_eq!(statuses(&superuser, "erin").await, ["provisioning"]);
    let (status, resumed, body) = complete_with(&srv, &erin, "198.51.100.92", &erin_body).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(resumed.is_some());
    let (id, status): (uuid::Uuid, String) =
        sqlx::query_as("SELECT data_tenant_id, status FROM platform.tenants WHERE slug = 'erin'")
            .fetch_one(&superuser)
            .await
            .expect("tenant reads");
    assert_eq!((id, status.as_str()), (interrupted, "active"));

    // 6. An expired proof cannot restart its claim.
    let finn = prove_signup(&srv, &google, "google-finn").await;
    sqlx::query(
        "WITH a AS (UPDATE platform.signup_attempts
                       SET status = 'creating',
                           updated_at = statement_timestamp() - interval '20 minutes',
                           expires_at = statement_timestamp() - interval '1 second'
                     WHERE subject = 'google-finn' RETURNING id)
         INSERT INTO platform.tenants
             (data_tenant_id, slug, display_name, status, signup_attempt_id, signup_creator_key,
              updated_at)
         SELECT gen_random_uuid(), 'finn', 'Finn', 'provisioning', id,
                sha256(convert_to('finn', 'UTF8')), now() - interval '20 minutes' FROM a",
    )
    .execute(&superuser)
    .await
    .expect("an expired interrupted creation is staged");
    let (status, _, body) = complete_with(
        &srv,
        &finn,
        "198.51.100.94",
        &json!({ "slug": "finn", "display_name": "Finn" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], "WYRD_AUTH_400_INVALID_STATE", "{body}");
    assert_eq!(
        statuses(&superuser, "finn").await,
        ["provisioning"],
        "never active ownerless"
    );
}

/// Complete a Google proof for `subject` as `slug` from `client`; returns
/// the status and body.
async fn create_as(
    srv: &WyrdTestServer,
    google: &wiremock::MockServer,
    subject: &str,
    slug: &str,
    client: &str,
) -> (StatusCode, Value) {
    let proof = prove_signup(srv, google, subject).await;
    let (status, _, body) = complete_with(
        srv,
        &proof,
        client,
        &json!({ "slug": slug, "display_name": slug }),
    )
    .await;
    (status, body)
}

/// Assert `body` is the owned-workspace cap refusal.
fn at_cap(status: StatusCode, body: &Value, case: &str) {
    assert_eq!(status, StatusCode::FORBIDDEN, "{case}: {body}");
    assert_eq!(
        body["code"], "WYRD_AUTH_403_SIGNUP_WORKSPACE_LIMIT",
        "{case}"
    );
}

/// Each proven identity owns at most the configured number of
/// signup-created workspaces that are not retired:
///   1. the cap is an operator login setting, default 3;
///   2. at the cap a new claim is refused with `403`, not `429`, and creates
///      nothing;
///   3. the count survives purging the identity's attempts, and a failed
///      claim still counts;
///   4. retiring a workspace frees capacity;
///   5. two concurrent claims across replicas at one slot left create one;
///   6. resuming an attempt's own claim at the cap is not a new claim;
///   7. an email identity at the same address is a different identity, and
///      operator creation is never capped.
///
/// # Panics
/// Panics when a claim is admitted or refused differently.
#[tokio::test(flavor = "multi_thread")]
async fn the_owned_workspace_cap_holds_atomically_across_purge() {
    let google = wiremock::MockServer::start().await;
    let (srv, session) = signup_server(&google).await;
    let api = relay_to_mailpit(&srv, &session).await;
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");

    // 1. The cap is configurable and defaults to three.
    let settings = |body: Option<Value>| {
        social_request(
            if body.is_some() {
                Method::PUT
            } else {
                Method::GET
            },
            "/platform/login-settings",
            Some(&session),
            body,
        )
    };
    let (status, current) = social_call(&srv, settings(None)).await;
    assert_eq!(status, StatusCode::OK, "{current}");
    assert_eq!(current["signup_workspace_cap"], 3, "{current}");
    let (status, changed) = social_call(
        &srv,
        settings(Some(
            json!({ "password_enabled": true, "signup_enabled": true,
                               "signup_workspace_cap": 1 }),
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{changed}");
    let (status, refused) = social_call(
        &srv,
        settings(Some(
            json!({ "password_enabled": true, "signup_enabled": true,
                               "signup_workspace_cap": 0 }),
        )),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a cap below one: {refused}"
    );

    // 2. At the cap a new claim is refused and creates nothing.
    let (status, body) = create_as(&srv, &google, "google-cap", "cap-a", "198.51.100.100").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = create_as(&srv, &google, "google-cap", "cap-b", "198.51.100.101").await;
    at_cap(status, &body, "second workspace");
    assert!(statuses(&superuser, "cap-b").await.is_empty());

    // 3. The count survives purge, and a failed claim counts.
    sqlx::query("DELETE FROM platform.signup_attempts WHERE subject = 'google-cap'")
        .execute(&superuser)
        .await
        .expect("attempts purge");
    let (status, body) = create_as(&srv, &google, "google-cap", "cap-c", "198.51.100.102").await;
    at_cap(status, &body, "after purge");
    sqlx::query("UPDATE platform.tenants SET status = 'failed', provisioning_failed_reason = 'interrupted' WHERE slug = 'cap-a'")
        .execute(&superuser)
        .await
        .expect("claim fails");
    let (status, body) = create_as(&srv, &google, "google-cap", "cap-d", "198.51.100.103").await;
    at_cap(status, &body, "a failed claim counts");

    // 4. Retirement frees capacity.
    sqlx::query("UPDATE platform.tenants SET status = 'deleted', provisioning_failed_reason = NULL WHERE slug = 'cap-a'")
        .execute(&superuser)
        .await
        .expect("tenant retires");
    let (status, body) = create_as(&srv, &google, "google-cap", "cap-e", "198.51.100.104").await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // 5. Concurrent claims across replicas at one slot create one.
    let replica = srv
        .start_replica(
            WyrdTestServer::builder().with_trusted_ingress_for_test(&[INGRESS], CLIENT_HEADER),
        )
        .await
        .expect("replica starts");
    let first = prove_signup(&srv, &google, "google-race").await;
    let second = prove_signup(&srv, &google, "google-race").await;
    let race_1 = json!({ "slug": "race-1", "display_name": "Race" });
    let race_2 = json!({ "slug": "race-2", "display_name": "Race" });
    let (one, two) = tokio::join!(
        complete_with(&srv, &first, "198.51.100.105", &race_1),
        complete_with(&replica, &second, "198.51.100.106", &race_2),
    );
    let mut outcomes = [one.0, two.0];
    outcomes.sort();
    assert_eq!(
        outcomes,
        [StatusCode::OK, StatusCode::FORBIDDEN],
        "{one:?} {two:?}"
    );
    let created: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM platform.tenants WHERE slug IN ('race-1', 'race-2')",
    )
    .fetch_one(&superuser)
    .await
    .expect("directory reads");
    assert_eq!(created, 1);

    // 6. Resuming an own claim at the cap is not a new claim.
    let resuming = prove_signup(&srv, &google, "google-race").await;
    sqlx::query(
        "WITH a AS (UPDATE platform.signup_attempts SET status = 'failed'
                     WHERE id = (SELECT id FROM platform.signup_attempts
                                  WHERE subject = 'google-race' AND status = 'proven'
                                  ORDER BY id DESC LIMIT 1) RETURNING id)
         INSERT INTO platform.tenants
             (data_tenant_id, slug, display_name, status, provisioning_failed_reason,
              signup_attempt_id, signup_creator_key)
         SELECT $1, 'race-own', 'Race', 'failed', 'interrupted', a.id,
                (SELECT signup_creator_key FROM platform.tenants
                  WHERE slug IN ('race-1', 'race-2'))
           FROM a",
    )
    .bind(uuid::Uuid::now_v7())
    .execute(&superuser)
    .await
    .expect("an own failed claim is staged");
    let (status, _, body) = complete_with(
        &srv,
        &resuming,
        "198.51.100.107",
        &json!({ "slug": "race-own", "display_name": "Race" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(statuses(&superuser, "race-own").await, ["active"]);

    // 7. Identities are never merged by email; operators are never capped.
    let (local, _) = email_proof(&srv, &api, "google-cap@example.test", "198.51.100.108").await;
    let (status, _, body) = complete_with(
        &srv,
        &local,
        "198.51.100.108",
        &json!({ "slug": "cap-mail", "display_name": "Mail",
                 "local_credentials": { "username": "cap", "password": "quiet harbor lantern" } }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "same address, other identity: {body}"
    );
    let (status, body) = social_call(
        &srv,
        social_request(
            Method::POST,
            "/platform/tenants",
            Some(&session),
            Some(json!({ "slug": "operator-made", "display_name": "Operator" })),
        ),
    )
    .await;
    assert!(status.is_success(), "operator creation: {body}");
}

/// Mount a Siteverify answer for the token `token`.
async fn siteverify(cloudflare: &wiremock::MockServer, token: &str, status: u16, answer: Value) {
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::body_string_contains(format!(
            "response={token}&"
        )))
        .respond_with(wiremock::ResponseTemplate::new(status).set_body_json(answer))
        .mount(cloudflare)
        .await;
}

/// With a Turnstile challenge configured, browser workspace creation needs a
/// server-validated token, checked after admission and before the claim:
///   1. the operator stores the site key and a sealed secret through the
///      login settings, which never return the secret;
///   2. a missing token, and tokens Siteverify refuses, reports as solved on
///      another hostname or for another action, or reports already used,
///      create no tenant and answer `WYRD_AUTH_403_SIGNUP_CHALLENGE`; a
///      missing token makes no Siteverify call;
///   3. an unavailable Siteverify refuses creation;
///   4. a valid token creates the workspace, posting the stored secret;
///   5. a completion refused by admission makes no Siteverify call;
///   6. operator creation is never challenged, and clearing the setting
///      removes the challenge.
///
/// # Panics
/// Panics when a completion is admitted or refused differently.
#[tokio::test(flavor = "multi_thread")]
async fn a_configured_challenge_gates_browser_creation() {
    let google = wiremock::MockServer::start().await;
    let cloudflare = wiremock::MockServer::start().await;
    let (srv, session) = signup_server_with(
        &google,
        wyrd_server::config::SocialConfig {
            google_issuer: Some(google.uri()),
            turnstile_siteverify_url: Some(cloudflare.uri()),
            ..Default::default()
        },
    )
    .await;
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    let settings = |body: Option<Value>| {
        social_request(
            if body.is_some() {
                Method::PUT
            } else {
                Method::GET
            },
            "/platform/login-settings",
            Some(&session),
            body,
        )
    };

    // 1. The secret is stored sealed and never returned.
    let (status, current) = social_call(&srv, settings(None)).await;
    assert_eq!(status, StatusCode::OK, "{current}");
    assert_eq!(current["signup_challenge"], Value::Null, "{current}");
    let configured = json!({ "password_enabled": true, "signup_enabled": true,
        "signup_challenge": { "site_key": "0x4AAA-site", "secret": "turnstile-secret" } });
    let (status, stored) = social_call(&srv, settings(Some(configured))).await;
    assert_eq!(status, StatusCode::OK, "{stored}");
    assert_eq!(
        stored["signup_challenge"],
        json!({ "site_key": "0x4AAA-site" })
    );
    let (_, read) = social_call(&srv, settings(None)).await;
    assert_eq!(
        read["signup_challenge"],
        json!({ "site_key": "0x4AAA-site" })
    );
    assert!(!read.to_string().contains("turnstile-secret"));
    let plain: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM platform.login_settings
          WHERE position('turnstile-secret' in encode(signup_challenge_secret, 'escape')) > 0",
    )
    .fetch_one(&superuser)
    .await
    .expect("settings read");
    assert_eq!(plain, 0, "the secret is sealed");

    // 2. Refused tokens create nothing.
    let solved = |hostname: &str, action: &str| json!({ "success": true, "hostname": hostname, "action": action, "error-codes": [] });
    siteverify(
        &cloudflare,
        "invalid",
        200,
        json!({ "success": false, "error-codes": ["invalid-input-response"] }),
    )
    .await;
    siteverify(
        &cloudflare,
        "reused",
        200,
        json!({ "success": false, "error-codes": ["timeout-or-duplicate"] }),
    )
    .await;
    siteverify(&cloudflare, "elsewhere", 200, solved("evil.test", "signup")).await;
    siteverify(&cloudflare, "login", 200, solved(SOCIAL_HOST, "login")).await;
    siteverify(&cloudflare, "down", 503, json!({})).await;
    siteverify(&cloudflare, "good", 200, solved(SOCIAL_HOST, "signup")).await;
    let attempt = |subject: &'static str, token: Option<&'static str>, client: &'static str| {
        let srv = &srv;
        let google = &google;
        async move {
            let proof = prove_signup(srv, google, subject).await;
            let mut body = json!({ "slug": subject, "display_name": subject });
            if let Some(token) = token {
                body["challenge_token"] = json!(token);
            }
            let (status, cookie, body) = complete_with(srv, &proof, client, &body).await;
            (status, cookie, body)
        }
    };
    let (status, _, body) = attempt("chal-missing", None, "198.51.100.120").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "WYRD_AUTH_403_SIGNUP_CHALLENGE", "{body}");
    let calls = || async {
        cloudflare
            .received_requests()
            .await
            .expect("recorded")
            .len()
    };
    assert_eq!(
        calls().await,
        0,
        "a missing token is refused without a call"
    );
    for (subject, token, client) in [
        ("chal-invalid", "invalid", "198.51.100.121"),
        ("chal-reused", "reused", "198.51.100.122"),
        ("chal-elsewhere", "elsewhere", "198.51.100.123"),
        ("chal-login", "login", "198.51.100.124"),
    ] {
        let (status, cookie, body) = attempt(subject, Some(token), client).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{token}: {body}");
        assert_eq!(body["code"], "WYRD_AUTH_403_SIGNUP_CHALLENGE", "{token}");
        assert!(cookie.is_none(), "{token}");
        assert!(
            statuses(&superuser, subject).await.is_empty(),
            "{token} created nothing"
        );
    }

    // 3. An unavailable provider refuses creation.
    let (status, _, body) = attempt("chal-down", Some("down"), "198.51.100.125").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["details"]["reason"], "challenge_unavailable", "{body}");
    assert!(statuses(&superuser, "chal-down").await.is_empty());

    // 4. A valid token creates the workspace with the stored secret.
    let (status, cookie, body) = attempt("chal-good", Some("good"), "198.51.100.126").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(cookie.is_some());
    assert_eq!(statuses(&superuser, "chal-good").await, ["active"]);
    let posted = cloudflare.received_requests().await.expect("recorded");
    let last = String::from_utf8_lossy(&posted.last().expect("a call").body).into_owned();
    assert!(
        last.starts_with("secret=turnstile-secret&response=good&"),
        "{last}"
    );

    // 5. Refused admission makes no call.
    let before = calls().await;
    let proof = prove_signup(&srv, &google, "chal-sourceless").await;
    let mut request = from_peer(
        Method::POST,
        "/auth/signup/complete",
        "10.1.0.1",
        None,
        &json!({ "slug": "chal-sourceless", "display_name": "x", "challenge_token": "good" }),
    );
    request
        .headers_mut()
        .insert(header::COOKIE, proof.parse().expect("cookie header"));
    let resp = srv.oneshot(request).await.expect("completion responds");
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        calls().await,
        before,
        "admission refused before the challenge"
    );

    // 6. Operators are not challenged; clearing removes the challenge.
    let (status, body) = social_call(
        &srv,
        social_request(
            Method::POST,
            "/platform/tenants",
            Some(&session),
            Some(json!({ "slug": "chal-operator", "display_name": "Operator" })),
        ),
    )
    .await;
    assert!(status.is_success(), "operator creation: {body}");
    let (status, cleared) = social_call(
        &srv,
        settings(Some(
            json!({ "password_enabled": true, "signup_enabled": true }),
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{cleared}");
    assert_eq!(cleared["signup_challenge"], Value::Null);
    let (status, _, body) = attempt("chal-cleared", None, "198.51.100.127").await;
    assert_eq!(status, StatusCode::OK, "{body}");
}
