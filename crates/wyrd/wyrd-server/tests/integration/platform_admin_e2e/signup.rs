//! Public workspace signup journeys: source admission, proof, completion and
//! creation, against a real server and repository Postgres.

use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use serde_json::{Value, json};
use wyrd_kms::{KmsBackendKind, KmsConfig};
use wyrd_testing::WyrdTestServer;

use super::{
    SOCIAL_HOST, begin_google_login, begin_social_at, body_json, complete_signup, enable_google,
    finish_google_login, identity_token_reply, identity_token_reply_with, initialize_platform_root,
    local_records, mail_to, mailpit, mount_platform_provider, platform_key_set, platform_session,
    prove_signup, prove_signup_with, social_call, social_callback, social_request,
};

/// The trusted ingress every journey here sits behind.
const INGRESS: &str = "10.0.0.0/8";

/// The header that ingress names the visitor in.
const CLIENT_HEADER: &str = "CF-Connecting-IP";

/// The wrapping key every journey server starts with.
const OLD_KEY: &str = "k1";

/// The wrapping key a rotation activates.
const NEW_KEY: &str = "k2";

/// A wrapping key no journey server ever held.
const FOREIGN_KEY: &str = "k3";

/// A directory of owner-only file wrapping keys [`OLD_KEY`], [`NEW_KEY`] and
/// [`FOREIGN_KEY`], kept for the whole test process so every restart and
/// replica can read it.
///
/// # Panics
/// Panics when the directory or a key file cannot be written.
fn wrapping_keys() -> PathBuf {
    use std::os::unix::fs::PermissionsExt as _;

    use base64::Engine as _;
    let dir = tempfile::tempdir().expect("key directory").keep();
    for (name, byte) in [(OLD_KEY, 0x11_u8), (NEW_KEY, 0x22), (FOREIGN_KEY, 0x33)] {
        let path = dir.join(name);
        std::fs::write(
            &path,
            base64::engine::general_purpose::STANDARD.encode([byte; 32]),
        )
        .expect("key file writes");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .expect("key file is owner-only");
    }
    dir
}

/// A file `[kms]` over `keys` wrapping under `active` and still opening
/// envelopes wrapped under `retained`.
fn kms(keys: &Path, active: &str, retained: &[&str]) -> KmsConfig {
    KmsConfig {
        backend: Some(KmsBackendKind::File),
        active_key: Some(active.to_owned()),
        retained_keys: retained.iter().map(|key| (*key).to_owned()).collect(),
        key_dir: Some(keys.to_path_buf()),
        ..KmsConfig::default()
    }
}

/// A server behind [`INGRESS`] with Google enabled for signup, and its
/// operator session. Its `[kms]` wraps under [`OLD_KEY`] and already
/// retains [`NEW_KEY`], the first step of a documented rotation, over the
/// key directory [`WyrdTestServer::kms_config_for_test`] names.
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
    let (srv, session, _) = rooted_signup_server(google, social).await;
    (srv, session)
}

/// [`signup_server_with`], also returning the platform root credential.
///
/// # Panics
/// As [`signup_server`].
async fn rooted_signup_server(
    google: &wiremock::MockServer,
    social: wyrd_server::config::SocialConfig,
) -> (WyrdTestServer, String, secrecy::SecretString) {
    let srv = WyrdTestServer::builder()
        .with_social_endpoints_for_test(social)
        .with_trusted_ingress_for_test(&[INGRESS], CLIENT_HEADER)
        .with_kms_for_test(kms(&wrapping_keys(), OLD_KEY, &[NEW_KEY]))
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
    (srv, session, root)
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

/// A self-service signup cannot claim a reserved workspace slug such as
/// `admin`, `platform` or `support`: completion answers the same `409` as
/// a taken slug and no tenant row is written, so nobody can pose as the
/// service operator.
///
/// # Panics
/// Panics when a reserved slug is claimed or refused differently from a
/// taken one.
#[tokio::test(flavor = "multi_thread")]
async fn signup_cannot_claim_a_reserved_slug() {
    let google = wiremock::MockServer::start().await;
    let (srv, _session) = signup_server(&google).await;
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    let proof = prove_signup(&srv, &google, "google-squatter").await;

    for slug in ["admin", "platform", "support"] {
        let body = json!({ "slug": slug, "display_name": "Official" });
        let (status, session, body) = complete_with(&srv, &proof, "198.51.100.140", &body).await;
        assert_eq!(status, StatusCode::CONFLICT, "{slug} is reserved: {body}");
        assert!(session.is_none(), "{slug}: no session is issued");
        assert!(
            statuses(&superuser, slug).await.is_empty(),
            "{slug}: no tenant row"
        );
    }
}

/// Every signup throttle answers `429` with a whole-second `Retry-After`
/// equal to `details.retry_after_seconds`:
///   1. the start source window;
///   2. the completion identity window;
///   3. the completion source window, for a provider or a local proof;
///   4. unavailable admission state refuses start rather than admitting it.
///
/// # Panics
/// Panics when a throttle lacks or misstates its wait, or unavailable state
/// admits.
#[tokio::test(flavor = "multi_thread")]
async fn every_signup_throttle_carries_retry_after() {
    let google = wiremock::MockServer::start().await;
    let (srv, session) = signup_server(&google).await;
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

    // 3b. A local completion from that visitor is refused too, and claims
    // nothing.
    let api = relay_to_mailpit(&srv, &session).await;
    let address = format!("throttled-{}@example.test", uuid::Uuid::new_v4().simple());
    let (local, _) = email_proof(&srv, &api, &address, "198.51.100.22").await;
    let (status, cookie, body) = complete_with(
        &srv,
        &local,
        "198.51.100.22",
        &json!({ "slug": "throttled-local", "display_name": "Throttled", "local_credentials": {
            "username": "ada", "password": "quiet harbor lantern" } }),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    assert_eq!(body["code"], "WYRD_AUTH_429_SIGNUP_ATTEMPTS", "{body}");
    assert!(cookie.is_none());
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    assert!(statuses(&superuser, "throttled-local").await.is_empty());

    // 4. Unavailable admission state refuses start.
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
        "UPDATE platform.signup_attempts SET expires_at = statement_timestamp() - interval '11 minutes'
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
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "WYRD_AUTH_403_METHOD_UNAVAILABLE", "{body}");
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

    // 5b. Once the completion window ends, cleanup keeps the spent proof for
    // as long as its email link lives, so the asking browser replaying it
    // gains no new completion; only later does cleanup remove it.
    let social = srv
        .state()
        .auth
        .social_sign_in
        .as_ref()
        .expect("social sign-in is built");
    let expire = |ago: &'static str| {
        sqlx::query(
            "UPDATE platform.signup_attempts
                SET expires_at = statement_timestamp() - $1::interval",
        )
        .bind(ago)
        .execute(&superuser)
    };
    expire("1 second")
        .await
        .expect("the completion window ends");
    while social.purge_expired_signups().await.expect("purge runs") > 0 {}
    let (status, replayed, body) = email_step(
        &srv,
        "/auth/signup/verify",
        "198.51.100.70",
        Some(&flow),
        "wyrd_signup",
        &verify(&proof),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert!(replayed.is_none(), "a spent proof yields no completion");
    assert_eq!(
        count(&superuser, attempts).await,
        1,
        "the spent proof outlives its completion window"
    );
    expire("11 minutes")
        .await
        .expect("the email link has expired too");
    while social.purge_expired_signups().await.expect("purge runs") > 0 {}
    assert_eq!(
        count(&superuser, attempts).await,
        0,
        "then cleanup removes it"
    );

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
///   4. repeating the successful completion returns the same workspace,
///      opens no session and creates nothing; a resumed creation retried
///      under another username keeps the one original administrator;
///   5. a claim interrupted in `creating` beyond the two-minute staleness bound resumes
///      under its own tenant id only with its own completion cookie; another
///      proof cannot adopt it, and a recent one still reads as in progress;
///   6. an expired proof cannot restart its interrupted claim, which stays
///      unactivated;
///   7. operator creation of that slug cannot adopt the expired signup
///      claim, stale or failed, before or after its attempt is purged;
///   8. removing the mail relay refuses new email starts but not a proof
///      already delivered: it still verifies and completes.
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

    // 4. Repeating the successful completion, as after a lost response,
    // returns the same workspace and creates nothing.
    let (status, again, body) =
        complete_with(&srv, &local, "198.51.100.90", &named(credentials.clone())).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, json!({ "tenant": "harbor" }));
    assert!(
        again.is_none(),
        "the creator signs in; a retry opens no session"
    );
    assert_eq!(statuses(&superuser, "harbor").await, ["active"]);
    let admins: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wyrd.auth_users u JOIN platform.tenants t
             ON t.data_tenant_id = u.data_tenant_id WHERE t.slug = 'harbor'",
    )
    .fetch_one(&superuser)
    .await
    .expect("users count");
    assert_eq!(admins, 1, "one administrator");

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

    // 7. Operator creation cannot adopt the expired signup claim.
    for (case, stage, status) in [
        ("stale provisioning", "SELECT 1", "provisioning"),
        (
            "failed",
            "UPDATE platform.tenants SET status = 'failed',
                    provisioning_failed_reason = 'interrupted' WHERE slug = 'finn'",
            "failed",
        ),
        (
            "failed after purge",
            "DELETE FROM platform.signup_attempts WHERE subject = 'google-finn'",
            "failed",
        ),
        (
            "stale provisioning after purge",
            "UPDATE platform.tenants SET status = 'provisioning', provisioning_failed_reason = NULL,
                    updated_at = now() - interval '20 minutes' WHERE slug = 'finn'",
            "provisioning",
        ),
    ] {
        sqlx::raw_sql(stage).execute(&superuser).await.expect(case);
        let (code, body) = social_call(
            &srv,
            social_request(
                Method::POST,
                "/platform/tenants",
                Some(&session),
                Some(json!({ "slug": "finn", "display_name": "Finn" })),
            ),
        )
        .await;
        assert_eq!(code, StatusCode::CONFLICT, "{case}: {body}");
        assert_eq!(statuses(&superuser, "finn").await, [status], "{case}");
        let owners: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM wyrd.auth_users u JOIN platform.tenants t
                 ON t.data_tenant_id = u.data_tenant_id WHERE t.slug = 'finn'",
        )
        .fetch_one(&superuser)
        .await
        .expect("users count");
        assert_eq!(owners, 0, "{case}: no operator owner was initialized");
    }

    // 8. Removing the relay withdraws new email starts, not a delivered proof.
    let late = format!("late-{}@example.test", uuid::Uuid::new_v4().simple());
    let start = json!({ "method": "email", "email": late });
    let (status, flow, body) = email_step(
        &srv,
        "/auth/signup",
        "198.51.100.95",
        None,
        "wyrd_signup_email",
        &start,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    let flow = flow.expect("flow cookie");
    let flow = flow.split(';').next().expect("pair");
    let (proof, _) = mailed_proof(&api, &late).await;
    sqlx::query(
        "UPDATE platform.login_settings
            SET smtp_host = NULL, smtp_port = NULL, smtp_tls = NULL, smtp_username = NULL,
                smtp_password_enc = NULL, smtp_sender = NULL",
    )
    .execute(&superuser)
    .await
    .expect("the relay is removed");
    let (status, _, body) = email_step(
        &srv,
        "/auth/signup",
        "198.51.100.96",
        None,
        "wyrd_signup_email",
        &start,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "a new start: {body}");
    let (status, completion, body) = email_step(
        &srv,
        "/auth/signup/verify",
        "198.51.100.95",
        Some(flow),
        "wyrd_signup",
        &json!({ "proof": proof }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the delivered proof verifies: {body}"
    );
    let completion = completion.expect("completion cookie");
    let (status, signed_in, body) = complete_with(
        &srv,
        completion.split(';').next().expect("pair"),
        "198.51.100.95",
        &json!({ "slug": "beacon", "display_name": "Beacon", "local_credentials": {
            "username": "lin", "password": "quiet harbor lantern" } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "and completes: {body}");
    assert!(signed_in.is_some());
    assert_eq!(statuses(&superuser, "beacon").await, ["active"]);
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
///   0. two replicas deriving for the first time together store one
///      derivation key, and a flow begun on one verifies on the other;
///   3. the count survives purging the identity's attempts, activating a
///      new wrapping key with the old one retained, a routine maintenance
///      pass and retiring the old key: the retired replica still opens the
///      provider, SMTP and challenge secrets, completes an in-flight signup
///      proven before retirement, and refuses the same identity at the cap;
///      a failed claim still counts;
///   4. retiring a workspace frees capacity;
///   5. two concurrent claims at one slot left, across an old-key and a
///      rotated replica, create one;
///   6. resuming an attempt's own claim at the cap is not a new claim;
///   7. an email identity at the same address is a different identity, and
///      operator creation is never capped.
///
/// # Panics
/// Panics when a claim is admitted or refused differently.
#[tokio::test(flavor = "multi_thread")]
async fn the_owned_workspace_cap_holds_atomically_across_purge() {
    let google = wiremock::MockServer::start().await;
    let cloudflare = wiremock::MockServer::start().await;
    let (srv, session) = signup_server(&google).await;
    let api = relay_to_mailpit(&srv, &session).await;
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    let keys = srv
        .kms_config_for_test()
        .key_dir
        .clone()
        .expect("the journey server uses a file key directory");
    let derivation_keys = || async {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM platform.derivation_key")
            .fetch_one(&superuser)
            .await
            .expect("derivation keys count")
    };

    // 0. Two replicas deriving for the first time together keep one
    // derivation key: a flow begun on one verifies on the other.
    assert_eq!(derivation_keys().await, 0, "no key before first use");
    let twin = srv
        .start_replica(
            WyrdTestServer::builder()
                .with_trusted_ingress_for_test(&[INGRESS], CLIENT_HEADER)
                .with_kms_for_test(kms(&keys, OLD_KEY, &[NEW_KEY])),
        )
        .await
        .expect("a second replica starts");
    let (here, there) = (
        format!("here-{}@example.test", uuid::Uuid::new_v4().simple()),
        format!("there-{}@example.test", uuid::Uuid::new_v4().simple()),
    );
    let (body_here, body_there) = (
        json!({ "method": "email", "email": here }),
        json!({ "method": "email", "email": there }),
    );
    let (begun_here, begun_there) = tokio::join!(
        email_step(
            &srv,
            "/auth/signup",
            "198.51.100.95",
            None,
            "wyrd_signup_email",
            &body_here,
        ),
        email_step(
            &twin,
            "/auth/signup",
            "198.51.100.96",
            None,
            "wyrd_signup_email",
            &body_there,
        ),
    );
    for (status, _, body) in [&begun_here, &begun_there] {
        assert_eq!(*status, StatusCode::ACCEPTED, "{body}");
    }
    assert_eq!(derivation_keys().await, 1, "one derivation key");
    let flow = begun_there.1.expect("flow cookie");
    let (proof, _) = mailed_proof(&api, &there).await;
    let (status, _, body) = email_step(
        &srv,
        "/auth/signup/verify",
        "198.51.100.96",
        Some(flow.split(';').next().expect("pair")),
        "wyrd_signup",
        &json!({ "proof": proof }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the other replica's flow verifies: {body}"
    );

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

    // 3b. Activating a new wrapping key, the old one retained, keeps the
    // count: a replica wrapping under the new key opens the old replica's
    // proof and still refuses.
    let rotated = srv
        .start_replica(
            WyrdTestServer::builder()
                .with_trusted_ingress_for_test(&[INGRESS], CLIENT_HEADER)
                .with_kms_for_test(kms(&keys, NEW_KEY, &[OLD_KEY])),
        )
        .await
        .expect("rotated replica starts");
    let proof = prove_signup(&srv, &google, "google-cap").await;
    let (status, _, body) = complete_with(
        &rotated,
        &proof,
        "198.51.100.109",
        &json!({ "slug": "cap-r", "display_name": "cap-r" }),
    )
    .await;
    at_cap(status, &body, "after rotation");
    assert!(statuses(&superuser, "cap-r").await.is_empty());

    // 3c. A routine pass moves every stored envelope, the platform
    // derivation key's included, onto the new key; after the old key is
    // retired the derivation key is unchanged, so the in-flight proof and the
    // recorded creator key still match.
    sqlx::query("DELETE FROM platform.signup_admissions")
        .execute(&superuser)
        .await
        .expect("admission windows expire");
    let (host, port, _) = mailpit();
    for (uri, body) in [
        (
            "/platform/mail",
            json!({ "host": host, "port": port, "tls": "none", "sender": "signup@wyrd.test",
                    "username": "relay", "password": "relay-password" }),
        ),
        (
            "/platform/login-settings",
            json!({ "password_enabled": true, "signup_enabled": true, "signup_workspace_cap": 1,
                    "signup_challenge": { "site_key": "0x4AAA-site", "secret": "rotated-secret" } }),
        ),
    ] {
        let (status, stored) = social_call(
            &srv,
            social_request(Method::PUT, uri, Some(&session), Some(body)),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{uri}: {stored}");
    }
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::body_string_contains("response=good&"))
        .and(wiremock::matchers::body_string_contains("secret=rotated-secret"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
            json!({ "success": true, "hostname": SOCIAL_HOST, "action": "signup", "error-codes": [] }),
        ))
        .mount(&cloudflare)
        .await;
    let in_flight = prove_signup(&srv, &google, "google-cap").await;
    let (status, report) = social_call(
        &rotated,
        social_request(
            Method::POST,
            "/platform/stored-secrets/maintenance",
            Some(&session),
            Some(json!({ "mode": "routine" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["remaining"], 0, "every envelope moved: {report}");
    let retired = srv
        .start_replica(
            WyrdTestServer::builder()
                .with_social_endpoints_for_test(wyrd_server::config::SocialConfig {
                    google_issuer: Some(google.uri()),
                    turnstile_siteverify_url: Some(cloudflare.uri()),
                    ..Default::default()
                })
                .with_trusted_ingress_for_test(&[INGRESS], CLIENT_HEADER)
                .with_kms_for_test(kms(&keys, NEW_KEY, &[])),
        )
        .await
        .expect("a replica without the old key starts");
    let (status, _, body) = complete_with(
        &retired,
        &in_flight,
        "198.51.100.110",
        &json!({ "slug": "cap-i", "display_name": "cap-i", "challenge_token": "good" }),
    )
    .await;
    at_cap(status, &body, "an in-flight proof after retirement");
    let proof = prove_signup(&retired, &google, "google-cap").await;
    let (status, _, body) = complete_with(
        &retired,
        &proof,
        "198.51.100.111",
        &json!({ "slug": "cap-n", "display_name": "cap-n", "challenge_token": "good" }),
    )
    .await;
    at_cap(status, &body, "a provider sign-in after retirement");
    let stored: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM platform.derivation_key
          WHERE length(envelope) > 32
            AND position(decode(repeat('11', 32), 'hex') in envelope) = 0
            AND position(decode(repeat('22', 32), 'hex') in envelope) = 0",
    )
    .fetch_one(&superuser)
    .await
    .expect("the derivation key reads");
    assert_eq!(stored, 1, "one derivation key, stored only as an envelope");
    assert_eq!(
        cloudflare
            .received_requests()
            .await
            .expect("recorded")
            .len(),
        2,
        "each completion posted the original challenge secret"
    );
    let retired_mail = format!("retired-{}@example.test", uuid::Uuid::new_v4().simple());
    email_proof(&retired, &api, &retired_mail, "198.51.100.112").await;
    let (status, cleared) = social_call(
        &srv,
        settings(Some(
            json!({ "password_enabled": true, "signup_enabled": true, "signup_workspace_cap": 1 }),
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{cleared}");
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

    // 5. Concurrent claims at one slot, on an old-key and a rotated replica
    // mid-rollout, create one.
    let replica = &rotated;
    let first = prove_signup(&srv, &google, "google-race").await;
    let second = prove_signup(&srv, &google, "google-race").await;
    let race_1 = json!({ "slug": "race-1", "display_name": "Race" });
    let race_2 = json!({ "slug": "race-2", "display_name": "Race" });
    let (one, two) = tokio::join!(
        complete_with(&srv, &first, "198.51.100.105", &race_1),
        complete_with(replica, &second, "198.51.100.106", &race_2),
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

/// Signup and social sign-in derive their state from the platform
/// derivation key; a replica that cannot open it refuses them rather than
/// using plain state, while password sign-in keeps working:
///   1. on a replica whose `[kms]` holds none of the keys the stored
///      envelopes name, a social signup start, an email signup start and an
///      email proof's verification each answer
///      `WYRD_AUTH_503_VERIFY_UNAVAILABLE` and record no admission window,
///      and a Google sign-in to an existing workspace refuses alike at its
///      start, setting no cookie and creating no User, identity, session or
///      grant;
///   2. a workspace administrator still signs in there with a password.
///
/// # Panics
/// Panics when a refusal, a recorded window or the password sign-in differs.
#[tokio::test(flavor = "multi_thread")]
async fn signup_refuses_without_its_derivation_key() {
    let google = wiremock::MockServer::start().await;
    let (srv, session) = signup_server(&google).await;
    let api = relay_to_mailpit(&srv, &session).await;
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    let address = format!("keyless-{}@example.test", uuid::Uuid::new_v4().simple());
    let (completion, _) = email_proof(&srv, &api, &address, "198.51.100.140").await;
    let (status, _, body) = complete_with(
        &srv,
        &completion,
        "198.51.100.140",
        &json!({ "slug": "keyless", "display_name": "Keyless", "local_credentials": {
            "username": "ada", "password": "quiet harbor lantern" } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let late = format!("late-{}@example.test", uuid::Uuid::new_v4().simple());
    let (status, flow, body) = email_step(
        &srv,
        "/auth/signup",
        "198.51.100.141",
        None,
        "wyrd_signup_email",
        &json!({ "method": "email", "email": late }),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    let flow = flow.expect("flow cookie");
    let flow = flow.split(';').next().expect("pair").to_owned();
    let (proof, _) = mailed_proof(&api, &late).await;
    let keys = srv
        .kms_config_for_test()
        .key_dir
        .clone()
        .expect("the journey server uses a file key directory");
    let keyless = srv
        .start_replica(
            WyrdTestServer::builder()
                .with_trusted_ingress_for_test(&[INGRESS], CLIENT_HEADER)
                .with_kms_for_test(kms(&keys, FOREIGN_KEY, &[])),
        )
        .await
        .expect("a replica without the wrapping keys starts");
    let windows = || async {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM platform.signup_admissions")
            .fetch_one(&superuser)
            .await
            .expect("windows count")
    };
    let before = windows().await;

    // 1. Each signup step refuses and records nothing.
    for (case, cookie, uri, body) in [
        (
            "social start",
            None,
            "/auth/signup",
            json!({ "method": "social", "provider": "google" }),
        ),
        (
            "email start",
            None,
            "/auth/signup",
            json!({ "method": "email", "email": format!("another-{late}") }),
        ),
        (
            "email verification",
            Some(flow.as_str()),
            "/auth/signup/verify",
            json!({ "proof": proof }),
        ),
    ] {
        let (status, set, body) = email_step(
            &keyless,
            uri,
            "198.51.100.142",
            cookie,
            "wyrd_signup",
            &body,
        )
        .await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{case}: {body}");
        assert_eq!(body["code"], "WYRD_AUTH_503_VERIFY_UNAVAILABLE", "{case}");
        assert!(set.is_none(), "{case} sets no cookie");
        assert_eq!(windows().await, before, "{case} records no window");
    }

    // A tenant Google sign-in start is admitted by a pseudonymous window,
    // which needs the derivation key, so it refuses and records nothing.
    let records = local_records(&superuser).await;
    let (status, headers, body) = start_sign_in(
        &keyless,
        "/auth/browser/login",
        Some("198.51.100.142"),
        &json!({ "tenant": "keyless", "provider": "google" }),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["code"], "WYRD_AUTH_503_VERIFY_UNAVAILABLE");
    assert!(headers.get(header::SET_COOKIE).is_none(), "no login cookie");
    assert_eq!(
        local_records(&superuser).await,
        records,
        "nothing is created"
    );
    assert_eq!(windows().await, before, "the start records no window");

    // 2. Password sign-in does not need the derivation key.
    let (status, signed_in, body) = email_step(
        &keyless,
        "/auth/browser/password/login",
        "198.51.100.143",
        None,
        "wyrd_session_keyless",
        &json!({ "tenant": "keyless", "username": "ada", "password": "quiet harbor lantern" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(signed_in.is_some());
}

/// One JSON sign-in start at `uri` through [`INGRESS`] naming `client`, or
/// naming nobody when `None`; returns the status, headers and body.
async fn start_sign_in(
    srv: &WyrdTestServer,
    uri: &str,
    client: Option<&str>,
    body: &Value,
) -> (StatusCode, HeaderMap, Value) {
    let resp = srv
        .oneshot(from_peer(Method::POST, uri, "10.0.0.9", client, body))
        .await
        .expect("sign-in start responds");
    let status = resp.status();
    let headers = resp.headers().clone();
    (status, headers, body_json(resp).await)
}

/// One CLI device authorization start for `tenant` through [`INGRESS`]
/// naming `client`; returns the status, headers and RFC 6749 body.
async fn start_device(
    srv: &WyrdTestServer,
    tenant: &str,
    client: &str,
) -> (StatusCode, HeaderMap, Value) {
    let resp = srv
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/device_authorization")
                .header(header::HOST, SOCIAL_HOST)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(CLIENT_HEADER, client)
                .extension(axum::extract::ConnectInfo(SocketAddr::new(
                    "10.0.0.9".parse().expect("peer address"),
                    40_000,
                )))
                .body(Body::from(format!("client_id=wyrd-cli&tenant={tenant}")))
                .expect("request builds"),
        )
        .await
        .expect("device start responds");
    let status = resp.status();
    let headers = resp.headers().clone();
    (status, headers, body_json(resp).await)
}

/// Pending sign-in rows across every tenant: login transactions and device
/// authorizations.
async fn pending_sign_ins(superuser: &sqlx::PgPool) -> i64 {
    count(
        superuser,
        "SELECT (SELECT count(*) FROM wyrd.auth_login_state) \
              + (SELECT count(*) FROM wyrd.auth_device_authorizations)",
    )
    .await
}

/// A flood of unauthenticated sign-in starts from one source records only a
/// counter:
///   1. company SSO, social and device starts, including for an unknown
///      tenant, share one window per source; the 301st is refused on every
///      start route with `429` and a whole-second `Retry-After`
///      (`WYRD_AUTH_429_SIGN_IN_ATTEMPTS`, or `temporarily_unavailable` on
///      the device endpoint), and adds no pending row, cookie or provider
///      request;
///   2. another replica counts in the same window;
///   3. another source still starts and completes a Google sign-in;
///   4. the refused source may still start a signup, and a source refused by
///      signup may still start a sign-in;
///   5. a start whose source cannot be derived, or on a replica without the
///      derivation key, refuses with `503` and records nothing, while
///      password and platform root sign-in still work there.
///
/// # Panics
/// Panics when a start is admitted or refused differently, or records state.
#[tokio::test(flavor = "multi_thread")]
async fn sign_in_starts_are_admitted_per_source() {
    let google = wiremock::MockServer::start().await;
    let (srv, _, root) = rooted_signup_server(
        &google,
        wyrd_server::config::SocialConfig {
            google_issuer: Some(google.uri()),
            ..Default::default()
        },
    )
    .await;
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    let creator = prove_signup(&srv, &google, "google-flood").await;
    let (status, body, _) = complete_signup(&srv, &creator, "flood", [198, 51, 100, 150]).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let flooder = Some("203.0.113.5");

    // 1. Unknown-tenant starts are counted like any other.
    for start in 0..300 {
        let (status, _, body) = if start % 3 == 2 {
            start_device(&srv, "no-such-tenant", "203.0.113.5").await
        } else {
            start_sign_in(
                &srv,
                "/auth/browser/login",
                flooder,
                &json!({ "tenant": "no-such-tenant" }),
            )
            .await
        };
        assert_ne!(
            status,
            StatusCode::TOO_MANY_REQUESTS,
            "start {start}: {body}"
        );
    }
    let pending = pending_sign_ins(&superuser).await;
    let discoveries = google
        .received_requests()
        .await
        .map_or(0, |seen| seen.len());
    for (case, uri, body) in [
        (
            "company SSO",
            "/auth/browser/login",
            json!({ "tenant": "flood" }),
        ),
        (
            "social",
            "/auth/browser/login",
            json!({ "tenant": "flood", "provider": "google" }),
        ),
        (
            "platform OIDC",
            "/auth/platform/login",
            json!({ "redirect_uri": format!("http://{SOCIAL_HOST}/platform/callback") }),
        ),
    ] {
        let (status, headers, body) = start_sign_in(&srv, uri, flooder, &body).await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{case}: {body}");
        assert_eq!(body["code"], "WYRD_AUTH_429_SIGN_IN_ATTEMPTS", "{case}");
        assert!(stated_wait(&headers, &body) > 0, "{case} states its wait");
        assert!(
            headers.get(header::SET_COOKIE).is_none(),
            "{case} sets no cookie"
        );
    }
    let (status, headers, body) = start_device(&srv, "flood", "203.0.113.5").await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "device: {body}");
    assert_eq!(body["error"], "temporarily_unavailable");
    assert_eq!(body["code"], "WYRD_AUTH_429_SIGN_IN_ATTEMPTS");
    let wait: u64 = headers
        .get(header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
        .expect("the device refusal states its wait");
    assert!((1..=3600).contains(&wait), "{wait}");
    assert_eq!(
        pending_sign_ins(&superuser).await,
        pending,
        "refusals record nothing"
    );
    assert_eq!(
        google
            .received_requests()
            .await
            .map_or(0, |seen| seen.len()),
        discoveries,
        "refusals contact no provider"
    );

    // 2. Another replica shares the count.
    let twin = srv
        .start_replica(
            WyrdTestServer::builder()
                .with_trusted_ingress_for_test(&[INGRESS], CLIENT_HEADER)
                .with_kms_for_test(srv.kms_config_for_test().clone()),
        )
        .await
        .expect("a replica starts");
    let (status, _, body) = start_sign_in(
        &twin,
        "/auth/browser/login",
        flooder,
        &json!({ "tenant": "flood" }),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "replica: {body}");

    // 3. Another source signs in to the same workspace.
    let begun = begin_google_login(&srv, "flood").await;
    mount_platform_provider(
        &google,
        platform_key_set("google-key"),
        identity_token_reply_with(
            &google.uri(),
            &begun.query["nonce"],
            "wyrd-google",
            "google-key",
            "google-flood",
            "google-flood@example.test",
            true,
        ),
    )
    .await;
    let (status, location) =
        finish_google_login(&srv, &begun, Some(&begun.cookie), &google.uri()).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let location = location.expect("the sign-in redirects");
    assert!(!location.contains("login="), "signed in: {location}");

    // 4. Sign-in and signup windows are separate.
    let (status, _, body) = start_signup(&srv, "10.0.0.9", flooder).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the flooder may still sign up: {body}"
    );
    let signup_flooder = Some("203.0.113.77");
    starts_until_throttled(&srv, "10.0.0.9", signup_flooder).await;
    let (status, _, body) = start_sign_in(
        &srv,
        "/auth/browser/login",
        signup_flooder,
        &json!({ "tenant": "flood", "provider": "google" }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a signup flooder may still sign in: {body}"
    );

    // 5. Without a source or the derivation key, starts refuse.
    let (status, _, body) = start_sign_in(
        &srv,
        "/auth/browser/login",
        None,
        &json!({ "tenant": "flood" }),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "no source: {body}");
    assert_eq!(body["code"], "WYRD_AUTH_503_VERIFY_UNAVAILABLE");
    let keys = srv
        .kms_config_for_test()
        .key_dir
        .clone()
        .expect("the journey server uses a file key directory");
    let keyless = srv
        .start_replica(
            WyrdTestServer::builder()
                .with_trusted_ingress_for_test(&[INGRESS], CLIENT_HEADER)
                .with_kms_for_test(kms(&keys, FOREIGN_KEY, &[])),
        )
        .await
        .expect("a replica without the wrapping keys starts");
    let pending = pending_sign_ins(&superuser).await;
    let fresh = Some("203.0.113.99");
    for (case, uri, body) in [
        (
            "company SSO",
            "/auth/browser/login",
            json!({ "tenant": "flood" }),
        ),
        (
            "platform OIDC",
            "/auth/platform/login",
            json!({ "redirect_uri": format!("http://{SOCIAL_HOST}/platform/callback") }),
        ),
    ] {
        let (status, headers, body) = start_sign_in(&keyless, uri, fresh, &body).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{case}: {body}");
        assert_eq!(body["code"], "WYRD_AUTH_503_VERIFY_UNAVAILABLE", "{case}");
        assert_eq!(
            headers
                .get(header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok()),
            Some("1"),
            "{case} is retryable"
        );
    }
    let (status, headers, body) = start_device(&keyless, "flood", "203.0.113.99").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "device: {body}");
    assert_eq!(body["error"], "temporarily_unavailable");
    assert!(headers.get(header::RETRY_AFTER).is_none());
    assert_eq!(
        pending_sign_ins(&superuser).await,
        pending,
        "nothing is recorded"
    );
    platform_session(&keyless, secrecy::ExposeSecret::expose_secret(&root)).await;
}

/// With a Turnstile challenge configured, browser workspace creation needs a
/// server-validated token, checked after admission and before the claim:
///   1. the operator stores the site key and a sealed secret through the
///      login settings, which never return the secret;
///   2. a missing token, and tokens Siteverify refuses, reports as solved on
///      another hostname or for another action, or reports already used,
///      create no tenant and answer `WYRD_AUTH_403_SIGNUP_CHALLENGE`; a
///      missing token makes no Siteverify call;
///   3. an unavailable Siteverify, or a stored secret that cannot be
///      opened, refuses creation;
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

    // 3b. A configured but unreadable challenge secret refuses creation; it
    // never counts as no challenge.
    let stored: Vec<u8> =
        sqlx::query_scalar("SELECT signup_challenge_secret FROM platform.login_settings")
            .fetch_one(&superuser)
            .await
            .expect("the challenge secret reads");
    sqlx::query("UPDATE platform.login_settings SET signup_challenge_secret = '\\x00'")
        .execute(&superuser)
        .await
        .expect("the challenge secret is corrupted");
    let (status, _, body) = attempt("chal-unreadable", Some("good"), "198.51.100.127").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["code"], "WYRD_AUTH_503_VERIFY_UNAVAILABLE", "{body}");
    assert!(statuses(&superuser, "chal-unreadable").await.is_empty());
    sqlx::query("UPDATE platform.login_settings SET signup_challenge_secret = $1")
        .bind(&stored)
        .execute(&superuser)
        .await
        .expect("the challenge secret is restored");

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

/// The verified contact of every User of workspace `slug`: username or
/// external subject, address, and whether the User has a password.
async fn contacts(superuser: &sqlx::PgPool, slug: &str) -> Vec<(String, bool)> {
    sqlx::query_as(
        "SELECT c.address, u.password_hash IS NOT NULL
           FROM wyrd.auth_user_contacts c
           JOIN wyrd.auth_users u ON u.data_tenant_id = c.data_tenant_id AND u.id = c.user_id
           JOIN platform.tenants t ON t.data_tenant_id = c.data_tenant_id
          WHERE t.slug = $1 AND c.verified_at IS NOT NULL",
    )
    .bind(slug)
    .fetch_all(superuser)
    .await
    .expect("contacts read")
}

/// Return from GitHub for a signup begun from a fresh browser, GitHub
/// reporting `emails`; returns the authorization query and the completion
/// cookie pair, if the return set one.
async fn github_signup(
    srv: &WyrdTestServer,
    github: &wiremock::MockServer,
    subject: u64,
    emails: Value,
) -> (std::collections::HashMap<String, String>, Option<String>) {
    let begun = super::begin_social_at(
        srv,
        "/auth/signup",
        json!({ "method": "social", "provider": "github" }),
    )
    .await;
    super::mount_github(
        github,
        "gho_signup",
        json!({ "id": subject, "login": "octo" }),
    )
    .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/user/emails"))
        .and(wiremock::matchers::header(
            "authorization",
            "Bearer gho_signup",
        ))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(emails))
        .mount(github)
        .await;
    let resp = srv
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(super::github_callback(&begun.query["state"]))
                .header(header::HOST, SOCIAL_HOST)
                .header(header::COOKIE, &begun.cookie)
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("callback responds");
    let cookie = resp
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with("wyrd_signup="))
        .map(|value| value.split(';').next().expect("pair").to_owned());
    (begun.query, cookie)
}

/// Every signup creator gets exactly one verified contact, proven by the
/// provider or by Wyrd's own email challenge:
///   1. without a mail relay, a Google email `email_verified` proves the
///      contact, and the password-free social admin records it;
///   2. a Google email not `email_verified` creates nothing: completion
///      answers `contact_required`, and without a relay the email branch is
///      not offered;
///   3. with a relay, that same browser proves a mailbox bound to its
///      provider attempt, then creates the workspace with the mailed
///      address as contact and still no password;
///   4. an email proof's local administrator gets the proven address as
///      contact and a password session that invites a teammate;
///   5. a GitHub signup asks for `user:email` only and takes a verified
///      address from the email API, never an unverified primary; with none
///      verified it is `contact_required`.
///
/// # Panics
/// Panics when a creator's contact, password or creation differs.
#[tokio::test(flavor = "multi_thread")]
async fn every_signup_creator_gets_a_verified_contact() {
    let google = wiremock::MockServer::start().await;
    let github = wiremock::MockServer::start().await;
    let (srv, session) = signup_server_with(
        &google,
        wyrd_server::config::SocialConfig {
            google_issuer: Some(google.uri()),
            github_url: Some(github.uri()),
            github_api_url: Some(github.uri()),
            turnstile_siteverify_url: None,
        },
    )
    .await;
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");

    // 1. Provider-verified contact needs no relay.
    let proof = prove_signup_with(&srv, &google, "verified-contact", true).await;
    let (status, _, created) = complete_with(
        &srv,
        &proof,
        "203.0.113.70",
        &json!({ "slug": "verified-contact", "display_name": "Verified" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(
        contacts(&superuser, "verified-contact").await,
        vec![("verified-contact@example.test".to_owned(), false)]
    );

    // 2. An unverified provider email is no contact.
    let proof = prove_signup_with(&srv, &google, "unverified-contact", false).await;
    let (status, _, refused) = complete_with(
        &srv,
        &proof,
        "203.0.113.71",
        &json!({ "slug": "unverified-contact", "display_name": "Unverified" }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    assert_eq!(
        refused["details"]["reason"], "contact_required",
        "{refused}"
    );
    assert!(statuses(&superuser, "unverified-contact").await.is_empty());
    let (status, _, refused) = email_step(
        &srv,
        "/auth/signup",
        "203.0.113.71",
        Some(&proof),
        "wyrd_signup_email",
        &json!({ "method": "email", "email": "owner@example.test" }),
    )
    .await;
    assert!(
        status.is_client_error(),
        "no relay, no email branch: {refused}"
    );

    // 3. The same browser proves a mailbox bound to its provider attempt.
    let api = relay_to_mailpit(&srv, &session).await;
    let (status, flow, body) = email_step(
        &srv,
        "/auth/signup",
        "203.0.113.71",
        Some(&proof),
        "wyrd_signup_email",
        &json!({ "method": "email", "email": "Owner@Example.test" }),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    let flow = flow.expect("flow cookie");
    let flow = flow.split(';').next().expect("pair");
    let (mailed, _) = mailed_proof(&api, "owner@example.test").await;
    let (status, completion, body) = email_step(
        &srv,
        "/auth/signup/verify",
        "203.0.113.71",
        Some(&format!("{flow}; {proof}")),
        "wyrd_signup",
        &json!({ "proof": mailed }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let completion = completion.expect("completion cookie");
    let completion = completion.split(';').next().expect("pair");
    let (status, _, created) = complete_with(
        &srv,
        completion,
        "203.0.113.71",
        &json!({ "slug": "unverified-contact", "display_name": "Unverified" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(
        contacts(&superuser, "unverified-contact").await,
        vec![("Owner@example.test".to_owned(), false)],
        "the canonical address keeps its local part"
    );
    let identities: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wyrd.auth_user_identities i
           JOIN platform.tenants t ON t.data_tenant_id = i.data_tenant_id
          WHERE t.slug = 'unverified-contact' AND i.subject = 'unverified-contact'",
    )
    .fetch_one(&superuser)
    .await
    .expect("identities read");
    assert_eq!(identities, 1, "the creator signs in with Google");

    // 4. A local administrator's contact is its proven address.
    let (proof, _) = email_proof(&srv, &api, "local@example.test", "203.0.113.72").await;
    let (status, session_cookie, created) = complete_with(
        &srv,
        &proof,
        "203.0.113.72",
        &json!({
            "slug": "local-contact",
            "display_name": "Local",
            "local_credentials": { "username": "founder", "password": "amber kestrel lantern" },
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(
        contacts(&superuser, "local-contact").await,
        vec![("local@example.test".to_owned(), true)]
    );
    let session_cookie = session_cookie.expect("password session");
    let session_cookie = session_cookie.split(';').next().expect("pair");
    let resp = srv
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/principals/users/invitations")
                .header(header::HOST, SOCIAL_HOST)
                .header(header::ORIGIN, format!("http://{SOCIAL_HOST}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, session_cookie)
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "recipients": ["teammate@example.test"],
                        "assignments": [{ "role": "viewer", "scope": "all" }],
                    }))
                    .expect("serializes"),
                ))
                .expect("request builds"),
        )
        .await
        .expect("invitation responds");
    let status = resp.status();
    assert_eq!(status, StatusCode::CREATED, "{}", body_json(resp).await);

    // 5. GitHub's verified address, never its unverified primary.
    super::enable_github(&srv, &session, &github).await;
    let (query, proof) = github_signup(
        &srv,
        &github,
        4242,
        json!([
            { "email": "primary@example.test", "primary": true, "verified": false },
            { "email": "octo@example.test", "primary": false, "verified": true },
        ]),
    )
    .await;
    assert_eq!(query.get("scope").map(String::as_str), Some("user:email"));
    let proof = proof.expect("completion cookie");
    let (status, _, created) = complete_with(
        &srv,
        &proof,
        "203.0.113.73",
        &json!({ "slug": "github-contact", "display_name": "GitHub" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(
        contacts(&superuser, "github-contact").await,
        vec![("octo@example.test".to_owned(), false)]
    );
    let (_, proof) = github_signup(
        &srv,
        &github,
        4343,
        json!([{ "email": "primary@example.test", "primary": true, "verified": false }]),
    )
    .await;
    let (status, _, refused) = complete_with(
        &srv,
        &proof.expect("completion cookie"),
        "203.0.113.74",
        &json!({ "slug": "github-unverified", "display_name": "GitHub" }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    assert_eq!(refused["details"]["reason"], "contact_required");
}

/// Read `GET /auth/signup/proof` from `client` with `cookie`; returns the
/// status and body.
async fn proof_method(srv: &WyrdTestServer, cookie: Option<&str>) -> (StatusCode, Value) {
    let peer: IpAddr = "10.7.0.1".parse().expect("peer address");
    let mut builder = Request::builder()
        .method(Method::GET)
        .uri("/auth/signup/proof")
        .header(header::HOST, SOCIAL_HOST)
        .header(CLIENT_HEADER, "203.0.113.80")
        .extension(axum::extract::ConnectInfo(SocketAddr::new(peer, 40_000)));
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    let resp = srv
        .oneshot(builder.body(Body::empty()).expect("request builds"))
        .await
        .expect("proof read responds");
    let status = resp.status();
    (status, body_json(resp).await)
}

/// The proof step reports how this browser proved itself, so the workspace
/// form knows whether to ask for local credentials:
///   1. without a completion cookie, or with a forged one, the read answers
///      `401 WYRD_AUTH_401_ONE_TIME_CODE_INVALID`;
///   2. a Google, GitHub or email proof reads back as exactly
///      `{"method": "google" | "github" | "email"}`, naming no address;
///   3. reading consumes nothing: it repeats, and the proof still creates
///      its workspace;
///   4. an expired proof reads as no proof.
///
/// # Panics
/// Panics when a proof reads, refuses or is consumed differently.
#[tokio::test(flavor = "multi_thread")]
async fn the_proof_step_reports_its_method() {
    let google = wiremock::MockServer::start().await;
    let github = wiremock::MockServer::start().await;
    let (srv, session) = signup_server_with(
        &google,
        wyrd_server::config::SocialConfig {
            google_issuer: Some(google.uri()),
            github_url: Some(github.uri()),
            github_api_url: Some(github.uri()),
            turnstile_siteverify_url: None,
        },
    )
    .await;
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");

    // 1. No live proof, no method.
    for cookie in [None, Some("wyrd_signup=forged")] {
        let (status, body) = proof_method(&srv, cookie).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{cookie:?}: {body}");
        assert_eq!(
            body["code"], "WYRD_AUTH_401_ONE_TIME_CODE_INVALID",
            "{body}"
        );
    }

    // 2 and 3. Each proof names its method, repeatably, and still creates.
    let google_proof = prove_signup(&srv, &google, "google-method").await;
    for _ in 0..2 {
        let (status, body) = proof_method(&srv, Some(&google_proof)).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body, json!({ "method": "google" }));
    }
    let (status, _, created) = complete_with(
        &srv,
        &google_proof,
        "203.0.113.81",
        &json!({ "slug": "google-method", "display_name": "Google" }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the read consumed nothing: {created}"
    );

    super::enable_github(&srv, &session, &github).await;
    let (_, github_proof) = github_signup(
        &srv,
        &github,
        5151,
        json!([{ "email": "octo-method@example.test", "primary": true, "verified": true }]),
    )
    .await;
    let github_proof = github_proof.expect("completion cookie");
    let (status, body) = proof_method(&srv, Some(&github_proof)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, json!({ "method": "github" }));

    let api = relay_to_mailpit(&srv, &session).await;
    let address = format!("method-{}@example.test", uuid::Uuid::new_v4().simple());
    let (email_proof_cookie, _) = email_proof(&srv, &api, &address, "203.0.113.82").await;
    let (status, body) = proof_method(&srv, Some(&email_proof_cookie)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, json!({ "method": "email" }));

    // 4. An expired proof is no proof.
    sqlx::query(
        "UPDATE platform.signup_attempts SET expires_at = statement_timestamp() - interval '1 second'
          WHERE subject = '5151'",
    )
    .execute(&superuser)
    .await
    .expect("the GitHub proof expires");
    let (status, body) = proof_method(&srv, Some(&github_proof)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(
        body["code"], "WYRD_AUTH_401_ONE_TIME_CODE_INVALID",
        "{body}"
    );
}
