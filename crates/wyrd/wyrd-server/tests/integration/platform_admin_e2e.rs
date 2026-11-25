//! The complete headless operator journey, against a real server.
//!
//! Proves the path an operator actually walks: initialize the deployment, take
//! a platform session, create a tenant, and use the tenant's own credential to
//! act inside it — with no database access at any step after initialization.
//!
//! It also proves the boundary in both directions, which is the property no
//! unit test can establish: a platform session cannot reach tenant resources
//! and a tenant token cannot reach the tenant directory.
//!
//! Runs in the Postgres-backed `test:wyrd` lane like every other server suite.

use secrecy::SecretString;
use sqlx::PgPool;
use std::env;
use std::io::{Result as IoResult, Write};
use std::process::Command;
use std::sync::{Arc, Mutex};

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, Response, StatusCode, header};
use serde_json::{Value, json};
use wyrd_server::boot::init::InitError;
use wyrd_spec::DataTenantId;
use wyrd_testing::WyrdTestServer;
use wyrd_testing::principal::within_propagation;

mod company_sso;
mod console;
mod contacts;
mod methods;
mod resets;
mod setup;
mod signup;

/// Wrapping-key lifecycle and stored-secret maintenance journeys.
mod stored_secrets;

/// The one header every Wyrd plane authenticates on.
///
/// Spelled here rather than imported because the server keeps it private: a test
/// that hard-codes it fails if the wire name ever changes, which is the point.
const WYRD_ACCESS_TOKEN_HEADER: &str = "x-wyrd-access-token";

/// Establish the deployment's root and hand back the credential it disclosed.
///
/// Initialization writes the plaintext to a caller-supplied sink and returns
/// nothing, because the disclosure is the only place the secret exists. This
/// reads it back out of an in-memory sink the way an operator reads it off a
/// terminal, so every journey below stays a journey rather than a plumbing
/// exercise.
///
/// # Errors
/// Returns the [`InitError`] initialization produced, notably when the
/// platform root already exists — initialization is idempotent only in the
/// sense that it refuses rather than mints a second root.
///
/// # Panics
/// Panics when initialization succeeded but disclosed no credential line.
async fn initialize_platform_root(srv: &WyrdTestServer) -> Result<SecretString, InitError> {
    let mut disclosure = Vec::new();
    wyrd_server::boot::init::initialize_platform_root(&srv.operator_pool(), &mut disclosure)
        .await?;
    let disclosed = String::from_utf8(disclosure).expect("the disclosure is UTF-8");
    let line = disclosed
        .lines()
        .find(|line| line.starts_with("wyrd_global_"))
        .expect("initialization discloses the credential it minted");
    Ok(secrecy::SecretString::from(line.to_owned()))
}

/// Decode a JSON response body.
async fn body_json(resp: Response<Body>) -> Value {
    let bytes = to_bytes(resp.into_body(), 1 << 20)
        .await
        .expect("read body");
    serde_json::from_slice(&bytes).expect("decode body")
}

/// Build a JSON POST carrying a platform session.
fn platform_post(uri: &str, session: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header(WYRD_ACCESS_TOKEN_HEADER, format!("Bearer {session}"))
        .body(Body::from(serde_json::to_vec(&body).expect("serializes")))
        .expect("request builds")
}

/// Build a JSON POST with no credential at all.
fn anonymous_post(uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&body).expect("serializes")))
        .expect("request builds")
}

/// Build an RFC 8693 token-exchange form POST presenting `credential` as an
/// API-key subject token to `uri`, with no credential header.
fn token_exchange(uri: &str, credential: &str) -> Request<Body> {
    let form = url::form_urlencoded::Serializer::new(String::new())
        .append_pair(
            "grant_type",
            "urn:ietf:params:oauth:grant-type:token-exchange",
        )
        .append_pair("subject_token", credential)
        .append_pair("subject_token_type", "urn:wyrd:oauth:token-type:api_key")
        .finish();
    Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(form))
        .expect("request builds")
}

/// Exchange a platform credential for a session token.
async fn platform_session(srv: &WyrdTestServer, credential: &str) -> String {
    let resp = srv
        .oneshot(token_exchange("/auth/platform/token", credential))
        .await
        .expect("token route responds");
    let status = resp.status();
    let body = body_json(resp).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "platform credential exchanges: {body}"
    );
    assert_eq!(body["token_type"], "Bearer");
    body["access_token"]
        .as_str()
        .expect("session token is a string")
        .to_owned()
}

/// Initialize → session → create tenant → act in the tenant, with no database
/// access after the first step.
#[tokio::test]
async fn operator_provisions_a_usable_tenant_without_touching_the_database() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");

    // 1. Initialization is the one step that runs against the database.
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let root_credential = secrecy::ExposeSecret::expose_secret(&root).to_owned();
    assert!(root_credential.starts_with("wyrd_global_"));

    // 2. Everything after it is an ordinary authenticated API call.
    let session = platform_session(&srv, &root_credential).await;

    let resp = srv
        .oneshot(platform_post(
            "/platform/tenants",
            &session,
            json!({ "slug": "acme", "display_name": "Acme" }),
        ))
        .await
        .expect("tenant route responds");
    assert_eq!(resp.status(), StatusCode::OK, "tenant provisions");
    let created = body_json(resp).await;

    assert_eq!(created["tenant"]["slug"], "acme");
    assert_eq!(
        created["tenant"]["status"], "active",
        "a returned tenant is usable, not half-provisioned"
    );
    let admin_credential = created["admin"]["credential"]
        .as_str()
        .expect("credential is returned once");
    assert!(admin_credential.starts_with("wyrd_sk_"));
    assert!(
        created["admin"]["principal_id"].is_string(),
        "the tenant administrator is a durable principal, not a bare credential"
    );

    // 3. The returned credential is immediately usable inside the tenant, which
    //    is what "usable tenant" has to mean.
    let resp = srv
        .oneshot(token_exchange("/auth/token", admin_credential))
        .await
        .expect("token exchange responds");
    let status = resp.status();
    let body = body_json(resp).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the tenant administrator can authenticate with no further setup: {body}"
    );
}

/// Re-running initialization refuses rather than minting a second root.
#[tokio::test]
async fn initialization_happens_at_most_once() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");

    initialize_platform_root(&srv)
        .await
        .expect("first initialization succeeds");

    let second = initialize_platform_root(&srv).await;

    assert!(
        matches!(second, Err(InitError::AlreadyInitialized)),
        "a second initialization refuses instead of creating another root"
    );
}

/// Point a child `wyrd-server` operator command at this test's database.
///
/// The binary resolves both its runtime and platform DSNs from the
/// environment, so the child gets the lane's `WYRD_DATABASE_URL` and
/// `WYRD_PLATFORM_DATABASE_URL` with the database swapped for the fixture's.
/// Leaving either at the lane database would let `init` write the platform
/// root into the shared database. Every other credential is inherited.
///
/// # Panics
///
/// Panics when the lane did not set either DSN, or when a value is not a
/// Postgres DSN naming its database after the last slash.
fn operator_command(subcommand: &str, database: &str) -> Command {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_wyrd-server"));
    command.arg(subcommand);
    for name in ["WYRD_DATABASE_URL", "WYRD_PLATFORM_DATABASE_URL"] {
        let base = env::var(name).unwrap_or_else(|_| panic!("the journey lane sets {name}"));
        let (prefix, _) = base
            .rsplit_once('/')
            .expect("a Postgres DSN names its database after the last slash");
        command.env(name, format!("{prefix}/{database}"));
    }
    command
}

/// The operator initializes a deployment by running the shipped binary.
///
/// Every other initialization journey calls the library function directly,
/// which proves the writes but not the process an operator actually runs: that
/// `init` parses as a subcommand, that the credential reaches the terminal and
/// nothing else, that a second run refuses instead of minting another root, and
/// that the exit status says which happened. A credential printed to stderr, or
/// printed again on the second run, is a disclosed secret in a log pipeline.
///
/// # Panics
///
/// Panics when the binary cannot be run, misreports its status, or discloses
/// the credential anywhere but the first run's stdout.
#[tokio::test]
async fn an_operator_initializes_the_deployment_through_the_shipped_binary() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let database = srv.pg_fixture().database_name().to_owned();

    let first = operator_command("init", &database)
        .output()
        .expect("init runs");
    let stdout = String::from_utf8(first.stdout).expect("init prints UTF-8");
    let stderr = String::from_utf8(first.stderr).expect("init prints UTF-8");
    assert!(
        first.status.success(),
        "init failed: status={:?} stdout={stdout} stderr={stderr}",
        first.status.code()
    );

    let credential = stdout
        .lines()
        .find(|line| line.starts_with("wyrd_global_"))
        .expect("the credential is printed to stdout")
        .to_owned();
    assert!(
        !stderr.contains(&credential),
        "the credential reached stderr, which is where a log pipeline reads"
    );

    // The credential the terminal received is the one the deployment accepts:
    // one-time delivery is only delivery if what was printed actually works.
    let session = platform_session(&srv, &credential).await;
    let resp = srv
        .oneshot(platform_request(
            Method::GET,
            "/platform/admins",
            &session,
            None,
        ))
        .await
        .expect("listing responds");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "the printed credential administers the platform"
    );

    let second = operator_command("init", &database)
        .output()
        .expect("init runs again");
    let repeat_stdout = String::from_utf8(second.stdout).expect("init prints UTF-8");
    let repeat_stderr = String::from_utf8(second.stderr).expect("init prints UTF-8");
    assert!(
        !second.status.success(),
        "a second init reported success: stdout={repeat_stdout}"
    );
    assert!(
        !repeat_stdout.contains("wyrd_global_") && !repeat_stderr.contains("wyrd_global_"),
        "the refused run disclosed credential material: stdout={repeat_stdout} \
         stderr={repeat_stderr}"
    );
    assert!(
        repeat_stderr.contains("wyrd-server:"),
        "the refusal is reported on stderr rather than silently: {repeat_stderr}"
    );
}

/// Losing every platform credential is recoverable through the shipped binary.
///
/// A platform credential cannot be read back — only its verifier is stored — so
/// without an operator-side recovery, losing them all would end administration
/// of the deployment permanently: every way to obtain a platform session needs a
/// credential, and issuing one needs a session. Recovery is available to whoever
/// already holds this machine's database access, which is the authority that ran
/// initialization, and it reissues for the existing root rather than creating a
/// second one.
///
/// # Panics
///
/// Panics when recovery does not restore full platform administration without
/// manual SQL.
#[tokio::test]
async fn an_operator_recovers_from_losing_every_platform_credential() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let database = srv.pg_fixture().database_name().to_owned();

    let initialized = operator_command("init", &database)
        .output()
        .expect("init runs");
    assert!(initialized.status.success(), "init failed");
    let root = printed_credential(&initialized.stdout);
    let session = platform_session(&srv, &root).await;

    let resp = srv
        .oneshot(platform_request(
            Method::GET,
            "/platform/admins",
            &session,
            None,
        ))
        .await
        .expect("listing responds");
    assert_eq!(resp.status(), StatusCode::OK);
    let principal_id = body_json(resp).await["principals"][0]["principal_id"]
        .as_str()
        .expect("the deployment root is listed")
        .to_owned();

    // Lose every platform credential the deployment has, through the platform
    // plane itself — no SQL.
    let resp = srv
        .oneshot(platform_request(
            Method::GET,
            &format!("/platform/admins/{principal_id}/credentials"),
            &session,
            None,
        ))
        .await
        .expect("listing responds");
    let listing = body_json(resp).await;
    for credential in listing["credentials"]
        .as_array()
        .expect("credentials are listed")
    {
        let id = credential["id"]
            .as_str()
            .expect("credential ids are strings");
        let resp = srv
            .oneshot(platform_request(
                Method::DELETE,
                &format!("/platform/admins/{principal_id}/credentials/{id}"),
                &session,
                None,
            ))
            .await
            .expect("revoke responds");
        assert_eq!(
            resp.status(),
            StatusCode::NO_CONTENT,
            "credential {id} retires"
        );
    }

    let resp = srv
        .oneshot(token_exchange("/auth/platform/token", &root))
        .await
        .expect("token exchange responds");
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "the deployment really has no usable platform credential left"
    );

    let recovered = operator_command("recover-root", &database)
        .output()
        .expect("recover-root runs");
    let stdout = String::from_utf8(recovered.stdout).expect("recover-root prints UTF-8");
    let stderr = String::from_utf8(recovered.stderr).expect("recover-root prints UTF-8");
    assert!(
        recovered.status.success(),
        "recover-root failed: stdout={stdout} stderr={stderr}"
    );
    let replacement = printed_credential(stdout.as_bytes());
    assert!(
        !stderr.contains(&replacement),
        "the replacement reached stderr, which is where a log pipeline reads"
    );

    // Recovery is only recovery if the whole operator journey resumes: the
    // replacement administers tenants and configures who may sign in.
    let recovered_session = platform_session(&srv, &replacement).await;
    let resp = srv
        .oneshot(platform_post(
            "/platform/tenants",
            &recovered_session,
            json!({ "slug": "after-recovery", "display_name": "After recovery" }),
        ))
        .await
        .expect("tenant route responds");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "the recovered credential provisions tenants"
    );

    let provider = wyrd_testing::DiscoveryFixture::start().await;
    let resp = srv
        .oneshot(platform_request(
            Method::PUT,
            "/platform/oidc/connection",
            &recovered_session,
            Some(json!({
                "issuer_url": provider.issuer(),
                "client_id": "wyrd-platform",
                "client_auth": { "method": "secret_post", "secret": "super-secret-client-value" },
            })),
        ))
        .await
        .expect("configure route responds");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "the recovered credential configures platform sign-in"
    );

    // The recovered root is the one the deployment already had, not a second.
    let resp = srv
        .oneshot(platform_request(
            Method::GET,
            "/platform/admins",
            &recovered_session,
            None,
        ))
        .await
        .expect("listing responds");
    let admins = body_json(resp).await;
    let roots = admins["principals"]
        .as_array()
        .expect("principals are listed")
        .iter()
        .filter(|principal| principal["principal_id"] == principal_id.as_str())
        .count();
    assert_eq!(
        roots, 1,
        "recovery reissued for the existing root rather than creating another: {admins}"
    );
}

/// Extract the one credential a `wyrd-server` operator command printed.
///
/// # Panics
///
/// Panics when the output is not UTF-8 or names no credential.
fn printed_credential(stdout: &[u8]) -> String {
    std::str::from_utf8(stdout)
        .expect("operator output is UTF-8")
        .lines()
        .find(|line| line.starts_with("wyrd_global_"))
        .expect("the credential is printed to stdout")
        .to_owned()
}

/// Neither plane can act on the other, in both directions.
#[tokio::test]
async fn the_two_control_planes_cannot_reach_each_other() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;

    // A tenant token cannot reach the tenant directory. It has to be presented
    // in the header the platform plane actually reads: sending it in the tenant
    // header would only prove that a request with no platform credential is
    // refused, which is true of an anonymous request too.
    let tenant = srv
        .bootstrap_service("journey-admin", &["admin"])
        .await
        .expect("tenant principal seeds");
    let tenant_jwt = srv
        .exchange_api_key(tenant.api_key().expect("machine key"))
        .await
        .expect("tenant token exchanges");
    let create_tenant = |authorization: Option<String>| {
        let mut builder = Request::builder()
            .method(Method::POST)
            .uri("/platform/tenants")
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(value) = authorization {
            builder = builder.header(WYRD_ACCESS_TOKEN_HEADER, value);
        }
        builder
            .body(Body::from(
                serde_json::to_vec(&json!({ "slug": "sneaky", "display_name": "Sneaky" }))
                    .expect("serializes"),
            ))
            .expect("request builds")
    };

    let resp = srv
        .oneshot(create_tenant(Some(format!("Bearer {tenant_jwt}"))))
        .await
        .expect("route responds");
    let presented_status = resp.status();
    let presented_body = body_json(resp).await;
    assert_eq!(
        presented_status,
        StatusCode::UNAUTHORIZED,
        "a tenant token cannot create tenants even with full tenant authority"
    );

    // ...and the refusal is the same one an anonymous request gets, so it tells
    // a caller nothing about whether its token was recognized.
    let resp = srv
        .oneshot(create_tenant(None))
        .await
        .expect("route responds");
    let anonymous_status = resp.status();
    let anonymous_body = body_json(resp).await;
    assert_eq!(anonymous_status, presented_status);
    assert_eq!(
        anonymous_body["code"], presented_body["code"],
        "a rejected tenant token is indistinguishable from no credential at all"
    );

    // A platform session cannot reach a tenant's resources.
    let resp = srv
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/v1/cards")
                .header(WYRD_ACCESS_TOKEN_HEADER, format!("Bearer {session}"))
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("route responds");
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "a platform session confers no access inside any tenant"
    );
}

/// A platform session dies with the credential that minted it.
///
/// The platform plane revalidates current state instead of trusting a token
/// snapshot: every request re-reads the minting credential. That claim is load-bearing, and only a real
/// request against a real route can show it holds.
#[tokio::test]
async fn revoking_a_platform_credential_ends_its_live_sessions() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;

    let resp = srv
        .oneshot(platform_post(
            "/platform/tenants",
            &session,
            json!({ "slug": "before-revocation", "display_name": "Before" }),
        ))
        .await
        .expect("tenant route responds");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "the session administers the platform while its credential is live"
    );

    revoke_every_platform_credential(&srv).await;

    let resp = srv
        .oneshot(platform_post(
            "/platform/tenants",
            &session,
            json!({ "slug": "after-revocation", "display_name": "After" }),
        ))
        .await
        .expect("tenant route responds");
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "the same session stops administering the platform once its credential is revoked"
    );
}

/// Revoke every platform credential, simulating an operator retiring a leaked
/// root credential.
async fn revoke_every_platform_credential(srv: &WyrdTestServer) {
    sqlx::query("UPDATE platform.credentials SET revoked_at = now() WHERE revoked_at IS NULL")
        .execute(srv.operator_pool().pool())
        .await
        .expect("credentials revoke");
}

/// A live platform session observes grant and principal changes immediately.
///
/// Every platform request re-reads the session's principal and its grant, so
/// the same session is forbidden on the request after its grant is withdrawn,
/// regains authority when the grant returns, and is unauthenticated on the
/// request after its principal is suspended. Nothing is cached to invalidate.
#[tokio::test]
async fn a_live_platform_session_observes_grant_withdrawal_and_suspension_on_its_next_request() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;
    let operator = srv.operator_pool();
    let create = |slug: &str| {
        platform_post(
            "/platform/tenants",
            &session,
            json!({ "slug": slug, "display_name": slug }),
        )
    };

    let withheld: serde_json::Value =
        sqlx::query_scalar("DELETE FROM platform.principal_grants RETURNING permissions")
            .fetch_one(operator.pool())
            .await
            .expect("grant withdraws");
    let resp = srv
        .oneshot(create("grant-withdrawn"))
        .await
        .expect("route responds");
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "the request after withdrawal holds no platform authority"
    );

    sqlx::query(
        "INSERT INTO platform.principal_grants (principal_id, permissions) \
         SELECT id, $1 FROM platform.principals",
    )
    .bind(&withheld)
    .execute(operator.pool())
    .await
    .expect("grant restores");
    let resp = srv
        .oneshot(create("grant-restored"))
        .await
        .expect("route responds");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "the request after restoration administers the platform again"
    );

    sqlx::query("UPDATE platform.principals SET status = 'suspended'")
        .execute(operator.pool())
        .await
        .expect("principal suspends");
    let resp = srv
        .oneshot(create("principal-suspended"))
        .await
        .expect("route responds");
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "the request after suspension is no longer a platform session"
    );
}

/// Losing every tenant credential does not cost the tenant: the platform plane
/// issues a replacement for the same principal.
#[tokio::test]
async fn tenant_administration_survives_losing_every_credential() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;

    let created = body_json(
        srv.oneshot(platform_post(
            "/platform/tenants",
            &session,
            json!({ "slug": "recoverable", "display_name": "Recoverable" }),
        ))
        .await
        .expect("tenant route responds"),
    )
    .await;
    let tenant_id = created["tenant"]["id"].as_str().expect("tenant id");
    let original_principal = created["admin"]["principal_id"]
        .as_str()
        .expect("principal id")
        .to_owned();

    let resp = srv
        .oneshot(platform_post(
            "/platform/tenants/admin/credentials",
            &session,
            json!({ "tenant_id": tenant_id }),
        ))
        .await
        .expect("recovery route responds");
    assert_eq!(resp.status(), StatusCode::OK, "recovery succeeds");
    let recovered = body_json(resp).await;

    assert_eq!(
        recovered["principal_id"].as_str().expect("principal id"),
        original_principal,
        "recovery restores the existing principal rather than creating a second"
    );
    let replacement = recovered["credential"].as_str().expect("credential");
    let resp = srv
        .oneshot(token_exchange("/auth/token", replacement))
        .await
        .expect("token exchange responds");
    let status = resp.status();
    let body = body_json(resp).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the replacement credential restores programmatic administration: {body}"
    );
}

/// Build a JSON request carrying a tenant access token.
fn tenant_request(method: Method, uri: &str, body: Option<Value>) -> Request<Body> {
    let builder = Request::builder().method(method).uri(uri);
    match body {
        Some(body) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("serializes")))
            .expect("request builds"),
        None => builder.body(Body::empty()).expect("request builds"),
    }
}

/// Exchange a tenant credential for an access token, or report the refusal.
///
/// # Errors
/// Returns the refusal's [`StatusCode`] when the exchange is not accepted, so
/// a journey can assert the status a real operator would see.
async fn tenant_token(srv: &WyrdTestServer, credential: &str) -> Result<String, StatusCode> {
    let resp = srv
        .oneshot(token_exchange("/auth/token", credential))
        .await
        .expect("token route responds");
    let status = resp.status();
    if status != StatusCode::OK {
        return Err(status);
    }
    let body = body_json(resp).await;
    Ok(body["access_token"]
        .as_str()
        .expect("access token is a string")
        .to_owned())
}

/// Provision a tenant and return its administrator's access token.
async fn provisioned_tenant_admin(srv: &WyrdTestServer, slug: &str) -> String {
    let root = initialize_platform_root(srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(srv, secrecy::ExposeSecret::expose_secret(&root)).await;
    let created = body_json(
        srv.oneshot(platform_post(
            "/platform/tenants",
            &session,
            json!({ "slug": slug, "display_name": slug }),
        ))
        .await
        .expect("tenant route responds"),
    )
    .await;
    let credential = created["admin"]["credential"]
        .as_str()
        .expect("admin credential");
    tenant_token(srv, credential)
        .await
        .expect("the tenant administrator authenticates")
}

/// A tenant administrator creates its own automation principal and rotates its
/// credential with no gap and no platform involvement.
///
/// This is the whole point of separating credentials from identity: the
/// principal, its roles, and everything it owns survive the rotation, and the
/// old credential mints nothing once the operator retires it. Revocation ends
/// renewal and does not withdraw a token the credential already minted, which
/// spends the account's current authority until its five-minute expiry, while
/// the surviving credential exchanges and spends immediately — no ordering
/// between the two tokens is involved.
#[tokio::test]
async fn a_tenant_rotates_an_automation_credential_without_an_outage() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let admin = provisioned_tenant_admin(&srv, "rotating").await;

    // 1. The tenant creates automation that is narrower than itself.
    let resp = srv
        .oneshot_authenticated(
            &admin,
            tenant_request(
                Method::POST,
                "/v1/principals",
                Some(json!({ "name": "ci-runner", "assignments": [{ "role": "viewer", "scope": "all" }] })),
            ),
        )
        .await
        .expect("principal route responds");
    let status = resp.status();
    let created = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "principal is created: {created}");
    let principal_id = created["principal_id"]
        .as_str()
        .expect("principal id")
        .to_owned();
    let first = created["credential"]
        .as_str()
        .expect("credential")
        .to_owned();

    // 2. Rotation is overlap: the replacement is live before the old one dies.
    let resp = srv
        .oneshot_authenticated(
            &admin,
            tenant_request(
                Method::POST,
                &format!("/v1/principals/{principal_id}/credentials"),
                None,
            ),
        )
        .await
        .expect("issue route responds");
    let status = resp.status();
    let issued = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "second credential issues: {issued}");
    let second = issued["credential"]
        .as_str()
        .expect("credential")
        .to_owned();
    let second_id = issued["id"].as_str().expect("credential id").to_owned();

    assert!(
        tenant_token(&srv, &first).await.is_ok(),
        "the original credential is still live during the overlap"
    );
    assert!(
        tenant_token(&srv, &second).await.is_ok(),
        "the replacement credential is live before anything is revoked"
    );

    // 3. Retiring the old one leaves the principal and the new credential alone.
    let credentials = body_json(
        srv.oneshot_authenticated(
            &admin,
            tenant_request(
                Method::GET,
                &format!("/v1/principals/{principal_id}/credentials"),
                None,
            ),
        )
        .await
        .expect("list route responds"),
    )
    .await;
    let listed = credentials["credentials"]
        .as_array()
        .expect("credential list");
    assert_eq!(
        listed.len(),
        2,
        "both credentials are visible: {credentials}"
    );
    assert!(
        listed.iter().all(|entry| entry.get("credential").is_none()),
        "a listing never returns secret material: {credentials}"
    );
    let first_id = listed
        .iter()
        .find(|entry| entry["id"] != second_id.as_str())
        .expect("the original credential is listed")["id"]
        .as_str()
        .expect("credential id")
        .to_owned();

    // A token minted by the credential about to be revoked.
    let live_token = tenant_token(&srv, &first)
        .await
        .expect("the original credential still mints a token");
    let resp = srv
        .oneshot_authenticated(&live_token, tenant_request(Method::GET, "/v1/cards", None))
        .await
        .expect("cards route responds");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "the token authorizes before revocation"
    );

    let resp = srv
        .oneshot_authenticated(
            &admin,
            tenant_request(
                Method::DELETE,
                &format!("/v1/principals/{principal_id}/credentials/{first_id}"),
                None,
            ),
        )
        .await
        .expect("revoke route responds");
    assert_eq!(resp.status(), StatusCode::NO_CONTENT, "revocation succeeds");

    assert_eq!(
        tenant_token(&srv, &first).await.unwrap_err(),
        StatusCode::BAD_REQUEST,
        "the retired credential mints no further token"
    );
    let resp = srv
        .oneshot_authenticated(&live_token, tenant_request(Method::GET, "/v1/cards", None))
        .await
        .expect("cards route responds");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "revoking a credential does not withdraw a token it already minted"
    );
    let surviving = tenant_token(&srv, &second)
        .await
        .expect("the principal's other credential still authenticates, so rotation has no gap");
    let resp = srv
        .oneshot_authenticated(&surviving, tenant_request(Method::GET, "/v1/cards", None))
        .await
        .expect("cards route responds");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "the surviving credential's fresh token spends immediately"
    );
}

/// Replaying a revoke changes nothing.
///
/// A retried DELETE — an SDK retry after a timed-out 204, a re-run pipeline, a
/// second operator — finds nothing left to retire and must leave the
/// surviving credential working.
#[tokio::test]
async fn replaying_a_revoke_does_not_disturb_the_surviving_credential() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let admin = provisioned_tenant_admin(&srv, "replayed").await;

    let created = body_json(
        srv.oneshot_authenticated(
            &admin,
            tenant_request(
                Method::POST,
                "/v1/principals",
                Some(json!({ "name": "ci-runner", "assignments": [{ "role": "viewer", "scope": "all" }] })),
            ),
        )
        .await
        .expect("principal route responds"),
    )
    .await;
    let principal_id = created["principal_id"].as_str().expect("principal id");

    let issued = body_json(
        srv.oneshot_authenticated(
            &admin,
            tenant_request(
                Method::POST,
                &format!("/v1/principals/{principal_id}/credentials"),
                None,
            ),
        )
        .await
        .expect("issue route responds"),
    )
    .await;
    let second = issued["credential"]
        .as_str()
        .expect("credential")
        .to_owned();

    let listed = body_json(
        srv.oneshot_authenticated(
            &admin,
            tenant_request(
                Method::GET,
                &format!("/v1/principals/{principal_id}/credentials"),
                None,
            ),
        )
        .await
        .expect("list route responds"),
    )
    .await;
    let first_id = listed["credentials"]
        .as_array()
        .expect("credential list")
        .iter()
        .find(|entry| entry["id"] != issued["id"])
        .expect("the original credential is listed")["id"]
        .as_str()
        .expect("credential id")
        .to_owned();

    let revoke = |credential: String| {
        tenant_request(
            Method::DELETE,
            &format!("/v1/principals/{principal_id}/credentials/{credential}"),
            None,
        )
    };
    let resp = srv
        .oneshot_authenticated(&admin, revoke(first_id.clone()))
        .await
        .expect("revoke route responds");
    assert_eq!(
        resp.status(),
        StatusCode::NO_CONTENT,
        "the first revoke acts"
    );

    let resp = srv
        .oneshot_authenticated(&admin, revoke(first_id))
        .await
        .expect("revoke route responds");
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "a replayed revoke reports that there was nothing left to retire"
    );

    // And the surviving credential is still a working way in.
    assert!(
        tenant_token(&srv, &second).await.is_ok(),
        "the surviving credential still authenticates after the replay"
    );
}

/// Revoking tenant credential A refuses A's next exchange at once, while the
/// token A already minted keeps authorizing until its expiry and surviving
/// credential B exchanges and spends immediately.
///
/// The server issues short-lived tokens with no clock-skew allowance, so the
/// lapse of A's token is observed directly instead of waiting out the
/// five-minute production lifetime. Tenant verification reads no auth store,
/// so there is nothing to invalidate and no ordering between A's and B's
/// tokens.
#[tokio::test]
async fn a_revoked_credential_mints_nothing_and_its_token_lapses_at_expiry() {
    let srv = wyrd_testing::WyrdTestServerBuilder::default()
        .with_access_ttl(chrono::Duration::seconds(4))
        .with_auth_verify_settings(wyrd_auth_verify::WyrdAuthVerifySettings {
            allowed_clock_skew: std::time::Duration::ZERO,
        })
        .start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;
    let created = body_json(
        srv.oneshot(platform_post(
            "/platform/tenants",
            &session,
            json!({ "slug": "revoked-admin", "display_name": "Revoked admin" }),
        ))
        .await
        .expect("tenant route responds"),
    )
    .await;
    let admin_id = created["admin"]["principal_id"]
        .as_str()
        .expect("admin principal id")
        .to_owned();
    let first = created["admin"]["credential"]
        .as_str()
        .expect("admin credential")
        .to_owned();

    let token_a = tenant_token(&srv, &first)
        .await
        .expect("the administrator authenticates");
    let cards = async |token: &str| {
        srv.oneshot_authenticated(token, tenant_request(Method::GET, "/v1/cards", None))
            .await
            .expect("cards route responds")
            .status()
    };
    assert_eq!(
        cards(&token_a).await,
        StatusCode::OK,
        "credential A's token authorizes"
    );

    let issued = body_json(
        srv.oneshot_authenticated(
            &token_a,
            tenant_request(
                Method::POST,
                &format!("/v1/principals/{admin_id}/credentials"),
                None,
            ),
        )
        .await
        .expect("issue route responds"),
    )
    .await;
    let second = issued["credential"]
        .as_str()
        .expect("replacement credential")
        .to_owned();
    let second_id = issued["id"].as_str().expect("credential id").to_owned();
    let listed = body_json(
        srv.oneshot_authenticated(
            &token_a,
            tenant_request(
                Method::GET,
                &format!("/v1/principals/{admin_id}/credentials"),
                None,
            ),
        )
        .await
        .expect("list route responds"),
    )
    .await;
    let first_id = listed["credentials"]
        .as_array()
        .expect("credential list")
        .iter()
        .find(|entry| entry["id"] != second_id.as_str())
        .expect("the original credential is listed")["id"]
        .as_str()
        .expect("credential id")
        .to_owned();

    let resp = srv
        .oneshot_authenticated(
            &token_a,
            tenant_request(
                Method::DELETE,
                &format!("/v1/principals/{admin_id}/credentials/{first_id}"),
                None,
            ),
        )
        .await
        .expect("revoke route responds");
    assert_eq!(resp.status(), StatusCode::NO_CONTENT, "revocation succeeds");

    assert_eq!(
        tenant_token(&srv, &first).await.unwrap_err(),
        StatusCode::BAD_REQUEST,
        "revoked credential A cannot exchange again"
    );
    assert_eq!(
        cards(&token_a).await,
        StatusCode::OK,
        "revoking A does not withdraw a token A already minted"
    );
    let token_b = tenant_token(&srv, &second)
        .await
        .expect("surviving credential B exchanges immediately");
    assert_eq!(
        cards(&token_b).await,
        StatusCode::OK,
        "B's fresh token spends immediately"
    );

    // Past the short lifetime, A's token lapses on its own.
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    assert_eq!(
        cards(&token_a).await,
        StatusCode::UNAUTHORIZED,
        "A's token is refused once it expires"
    );
}

/// A credential can only be revoked through the principal that owns it.
#[tokio::test]
async fn a_credential_cannot_be_revoked_through_another_principal() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let admin = provisioned_tenant_admin(&srv, "misaddressed").await;

    let mut principals = Vec::new();
    for name in ["owner", "bystander"] {
        let created = body_json(
            srv.oneshot_authenticated(
                &admin,
                tenant_request(
                    Method::POST,
                    "/v1/principals",
                    Some(json!({ "name": name, "assignments": [{ "role": "viewer", "scope": "all" }] })),
                ),
            )
            .await
            .expect("principal route responds"),
        )
        .await;
        principals.push((
            created["principal_id"]
                .as_str()
                .expect("principal id")
                .to_owned(),
            created["credential"]
                .as_str()
                .expect("credential")
                .to_owned(),
        ));
    }
    let (owner_id, owner_credential) = principals[0].clone();
    let (bystander_id, _) = principals[1].clone();

    let listed = body_json(
        srv.oneshot_authenticated(
            &admin,
            tenant_request(
                Method::GET,
                &format!("/v1/principals/{owner_id}/credentials"),
                None,
            ),
        )
        .await
        .expect("list route responds"),
    )
    .await;
    let owner_credential_id = listed["credentials"][0]["id"]
        .as_str()
        .expect("credential id")
        .to_owned();

    let resp = srv
        .oneshot_authenticated(
            &admin,
            tenant_request(
                Method::DELETE,
                &format!("/v1/principals/{bystander_id}/credentials/{owner_credential_id}"),
                None,
            ),
        )
        .await
        .expect("revoke route responds");
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "naming the wrong principal does not revoke another principal's credential"
    );
    assert!(
        tenant_token(&srv, &owner_credential).await.is_ok(),
        "the owner's credential is untouched"
    );
}

/// Automation created by a tenant administrator is genuinely narrower than the
/// administrator that created it.
///
/// The point of creating a restricted principal is that a leaked automation
/// credential cannot become a tenant administrator. That is a property of the
/// roles it was granted, so it needs a real request from a real token.
#[tokio::test]
async fn automation_cannot_escalate_itself_to_an_administrator() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let admin = provisioned_tenant_admin(&srv, "restricted").await;

    let created = body_json(
        srv.oneshot_authenticated(
            &admin,
            tenant_request(
                Method::POST,
                "/v1/principals",
                Some(json!({ "name": "ci-runner", "assignments": [{ "role": "viewer", "scope": "all" }] })),
            ),
        )
        .await
        .expect("principal route responds"),
    )
    .await;
    let credential = created["credential"].as_str().expect("credential");
    let automation = tenant_token(&srv, credential)
        .await
        .expect("automation authenticates");

    let resp = srv
        .oneshot_authenticated(
            &automation,
            tenant_request(
                Method::POST,
                "/v1/principals",
                Some(json!({ "name": "escalated", "assignments": [{ "role": "admin", "scope": "all" }] })),
            ),
        )
        .await
        .expect("principal route responds");
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "a reader principal cannot mint itself an administrator"
    );
}

/// Build a JSON request with a platform session and an explicit method.
fn platform_request(
    method: Method,
    uri: &str,
    session: &str,
    body: Option<Value>,
) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(WYRD_ACCESS_TOKEN_HEADER, format!("Bearer {session}"));
    match body {
        Some(body) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("serializes")))
            .expect("request builds"),
        None => builder.body(Body::empty()).expect("request builds"),
    }
}

/// Ed25519 key the mock platform provider signs ID tokens with.
const PLATFORM_SIGNING_KEY: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
/// Public half of [`PLATFORM_SIGNING_KEY`] as a JWK `x`.
const PLATFORM_SIGNING_X: &str = "WhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ-DZ8Vw";
/// Wyrd's client identifier at the mock platform provider, and so the only
/// audience its ID tokens may name.
const PLATFORM_CLIENT: &str = "wyrd-platform";
/// Discovery path of the mock platform provider.
const PLATFORM_DISCOVERY: &str = "/.well-known/openid-configuration";

/// Reset `server` to a platform provider that advertises RFC 9207 response
/// issuers, answers its key set with `key_set`, and answers its token
/// endpoint with `token_response`.
async fn mount_platform_provider(
    server: &wiremock::MockServer,
    key_set: wiremock::ResponseTemplate,
    token_response: wiremock::ResponseTemplate,
) {
    server.reset().await;
    mount_platform_discovery(server, key_set, &format!("{}/token", server.uri())).await;
    mount_platform_token(server, token_response).await;
}

/// Answer `server`'s discovery, advertising RFC 9207 response issuers and
/// `token_endpoint`, and its key set with `key_set`.
async fn mount_platform_discovery(
    server: &wiremock::MockServer,
    key_set: wiremock::ResponseTemplate,
    token_endpoint: &str,
) {
    let issuer = server.uri();
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(PLATFORM_DISCOVERY))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "issuer": issuer,
            "authorization_endpoint": format!("{issuer}/authorize"),
            "token_endpoint": token_endpoint,
            "jwks_uri": format!("{issuer}/jwks"),
            "response_types_supported": ["code"],
            "subject_types_supported": ["public"],
            "id_token_signing_alg_values_supported": ["EdDSA"],
            "authorization_response_iss_parameter_supported": true,
        })))
        .mount(server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/jwks"))
        .respond_with(key_set)
        .mount(server)
        .await;
}

/// A key set publishing [`PLATFORM_SIGNING_KEY`] under `kid`.
fn platform_key_set(kid: &str) -> wiremock::ResponseTemplate {
    wiremock::ResponseTemplate::new(200).set_body_json(json!({
        "keys": [{ "kty": "OKP", "crv": "Ed25519", "kid": kid, "x": PLATFORM_SIGNING_X }]
    }))
}

/// Reset `server` to a platform provider whose discovery and key set are
/// down, while its token endpoint still answers with `token_response`.
async fn mount_platform_outage(
    server: &wiremock::MockServer,
    token_response: wiremock::ResponseTemplate,
) {
    server.reset().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(wiremock::ResponseTemplate::new(503))
        .mount(server)
        .await;
    mount_platform_token(server, token_response).await;
}

/// Answer `server`'s token endpoint with `token_response`.
async fn mount_platform_token(
    server: &wiremock::MockServer,
    token_response: wiremock::ResponseTemplate,
) {
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/token"))
        .respond_with(token_response)
        .mount(server)
        .await;
}

/// A token-endpoint reply carrying an ID token from `issuer` for `nonce`,
/// addressed to `audience` and signed under `kid`, asserting the verified
/// email the registered administrator was matched on.
///
/// # Panics
/// Panics when the test key does not load or the token does not sign.
fn platform_token_reply(
    issuer: &str,
    nonce: &str,
    audience: &str,
    kid: &str,
) -> wiremock::ResponseTemplate {
    identity_token_reply(
        issuer,
        nonce,
        audience,
        kid,
        "provider-subject-1",
        "ops@example.com",
    )
}

/// A token-endpoint reply as [`platform_token_reply`], for the provider
/// identity `subject` asserting `email`.
///
/// # Panics
/// Panics when the test key does not load or the token does not sign.
fn identity_token_reply(
    issuer: &str,
    nonce: &str,
    audience: &str,
    kid: &str,
    subject: &str,
    email: &str,
) -> wiremock::ResponseTemplate {
    identity_token_reply_with(issuer, nonce, audience, kid, subject, email, true)
}

/// [`identity_token_reply`] whose `email_verified` claim is `email_verified`.
fn identity_token_reply_with(
    issuer: &str,
    nonce: &str,
    audience: &str,
    kid: &str,
    subject: &str,
    email: &str,
    email_verified: bool,
) -> wiremock::ResponseTemplate {
    wiremock::ResponseTemplate::new(200).set_body_json(identity_token_body(
        issuer,
        nonce,
        audience,
        kid,
        subject,
        email,
        email_verified,
    ))
}

/// The token-endpoint body [`identity_token_reply_with`] answers.
///
/// # Panics
/// Panics when the test key does not load or the token does not sign.
fn identity_token_body(
    issuer: &str,
    nonce: &str,
    audience: &str,
    kid: &str,
    subject: &str,
    email: &str,
    email_verified: bool,
) -> Value {
    signed_identity_body(
        kid,
        &identity_claims(issuer, nonce, audience, subject, email, email_verified),
    )
}

/// [`identity_token_reply`] carrying the standard `name` claim when `name`
/// is given.
fn identity_token_reply_named(
    issuer: &str,
    nonce: &str,
    audience: &str,
    kid: &str,
    subject: &str,
    email: &str,
    name: Option<&str>,
) -> wiremock::ResponseTemplate {
    let mut claims = identity_claims(issuer, nonce, audience, subject, email, true);
    if let Some(name) = name {
        claims["name"] = json!(name);
    }
    signed_identity_reply(kid, &claims)
}

/// The ID-token claims a provider at `issuer` asserts for `subject`.
fn identity_claims(
    issuer: &str,
    nonce: &str,
    audience: &str,
    subject: &str,
    email: &str,
    email_verified: bool,
) -> Value {
    let now = chrono::Utc::now().timestamp();
    json!({
        "iss": issuer,
        "sub": subject,
        "aud": audience,
        "exp": now + 3600,
        "iat": now,
        "nonce": nonce,
        "email": email,
        "email_verified": email_verified,
    })
}

/// A token-endpoint reply whose ID token carries `claims`, signed under `kid`.
///
/// # Panics
/// Panics when the test key does not load or the token does not sign.
fn signed_identity_reply(kid: &str, claims: &Value) -> wiremock::ResponseTemplate {
    wiremock::ResponseTemplate::new(200).set_body_json(signed_identity_body(kid, claims))
}

/// The token-endpoint body whose ID token carries `claims`, signed under `kid`.
///
/// # Panics
/// Panics when the test key does not load or the token does not sign.
fn signed_identity_body(kid: &str, claims: &Value) -> Value {
    let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::EdDSA);
    header.kid = Some(kid.to_owned());
    let key = jsonwebtoken::EncodingKey::from_ed_pem(PLATFORM_SIGNING_KEY.as_bytes())
        .expect("test key loads");
    let id_token = jsonwebtoken::encode(&header, claims, &key).expect("token signs");
    json!({
        "access_token": "provider-access",
        "token_type": "Bearer",
        "id_token": id_token,
    })
}

/// Begin a platform login and return its state and the nonce the provider
/// must echo.
///
/// # Panics
/// Panics when begin is refused or its authorization URL carries no nonce.
async fn begin_platform_login(srv: &WyrdTestServer) -> (String, String) {
    let resp = srv
        .oneshot(anonymous_post(
            "/auth/platform/login",
            json!({ "redirect_uri": "https://wyrd.example/callback" }),
        ))
        .await
        .expect("login route responds");
    let status = resp.status();
    let begun = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "platform login begins: {begun}");
    let authorization_url = url::Url::parse(
        begun["authorization_url"]
            .as_str()
            .expect("authorization url"),
    )
    .expect("authorization url parses");
    let nonce = authorization_url
        .query_pairs()
        .find(|(name, _)| name == "nonce")
        .map(|(_, value)| value.into_owned())
        .expect("the authorization request carries a nonce");
    let state = begun["state"].as_str().expect("state").to_owned();
    (state, nonce)
}

/// Present the provider's return for `state` to the served platform callback,
/// with `iss` when the provider sent one.
async fn platform_callback(
    srv: &WyrdTestServer,
    state: &str,
    iss: Option<&str>,
) -> (StatusCode, Value) {
    let mut body = json!({ "code": "provider-code", "state": state });
    if let Some(iss) = iss {
        body["iss"] = json!(iss);
    }
    let resp = srv
        .oneshot(anonymous_post("/auth/platform/callback", body))
        .await
        .expect("callback route responds");
    let status = resp.status();
    (status, body_json(resp).await)
}

/// Requests `server` recorded at `route` since its last reset.
///
/// # Panics
/// Panics when the mock server does not record requests.
async fn platform_provider_hits(server: &wiremock::MockServer, route: &str) -> usize {
    server
        .received_requests()
        .await
        .expect("requests are recorded")
        .iter()
        .filter(|request| request.url.path() == route)
        .count()
}

/// Federated platform sign-in runs end to end through the served begin and
/// `/auth/platform/callback` routes against a provider that advertises RFC
/// 9207 response issuers:
///   1. the connection takes no audience of its own; its client ID is the
///      audience, and a separately supplied one is refused;
///   2. a return without `iss`, or with another issuer's, is refused before
///      the token endpoint sees the code;
///   3. an ID token addressed to another client is refused;
///   4. a return with the exact `iss` yields a session with platform
///      authority and none inside a tenant;
///   5. with discovery and the key set down, a callback still completes from
///      the provider state its begin cached, and a token naming an unknown key
///      makes exactly one forced re-discovery and fails closed;
///   6. configuration discovers through the same relying party: a key set
///      that is unavailable or undecodable is refused without replacing the
///      stored connection, and a same-issuer reconfiguration refreshes the
///      provider the next begin and callback use.
///
/// # Panics
/// Panics when any step deviates from the contract above.
#[tokio::test]
async fn federated_platform_sign_in_runs_through_the_served_callback() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;
    let provider = wiremock::MockServer::start().await;
    let issuer = provider.uri();
    mount_platform_provider(
        &provider,
        platform_key_set("mock-1"),
        wiremock::ResponseTemplate::new(500),
    )
    .await;

    // 1. The client ID is the audience; there is no second one to configure.
    let configure = |extra: Option<(&str, &str)>| {
        let mut body = json!({
            "issuer_url": issuer,
            "client_id": PLATFORM_CLIENT,
            "client_auth": { "method": "public" },
        });
        if let Some((field, value)) = extra {
            body[field] = json!(value);
        }
        platform_request(
            Method::PUT,
            "/platform/oidc/connection",
            &session,
            Some(body),
        )
    };
    let resp = srv
        .oneshot(configure(Some(("expected_audience", "another-client"))))
        .await
        .expect("configure route responds");
    assert!(
        resp.status().is_client_error(),
        "a separate human audience is not an input: {}",
        resp.status()
    );
    let resp = srv
        .oneshot(configure(None))
        .await
        .expect("configure route responds");
    let status = resp.status();
    let view = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "connection configures: {view}");
    assert!(
        view.get("expected_audience").is_none(),
        "the connection view carries no separate audience: {view}"
    );

    // A provider whose key set cannot be loaded is refused, and the stored
    // connection is left as it was.
    let reconfigure = json!({
        "issuer_url": issuer,
        "client_id": "wyrd-platform-two",
        "client_auth": { "method": "public" },
    });
    for (label, key_set) in [
        ("unavailable key set", wiremock::ResponseTemplate::new(503)),
        (
            "undecodable key set",
            wiremock::ResponseTemplate::new(200).set_body_string("not a key set"),
        ),
    ] {
        mount_platform_provider(&provider, key_set, wiremock::ResponseTemplate::new(500)).await;
        let resp = srv
            .oneshot(platform_request(
                Method::PUT,
                "/platform/oidc/connection",
                &session,
                Some(reconfigure.clone()),
            ))
            .await
            .expect("configure route responds");
        let status = resp.status();
        let body = body_json(resp).await;
        assert_eq!(
            status,
            StatusCode::SERVICE_UNAVAILABLE,
            "{label} is refused: {body}"
        );
        assert_eq!(
            body["code"], "WYRD_AUTH_503_DISCOVERY_UNAVAILABLE",
            "{label} carries the stable discovery code: {body}"
        );
        let resp = srv
            .oneshot(platform_request(
                Method::GET,
                "/platform/oidc/connection",
                &session,
                None,
            ))
            .await
            .expect("connection route responds");
        let stored = body_json(resp).await;
        assert_eq!(
            stored["client_id"], PLATFORM_CLIENT,
            "{label}: the stored connection is unchanged: {stored}"
        );
    }
    mount_platform_provider(
        &provider,
        platform_key_set("mock-1"),
        wiremock::ResponseTemplate::new(500),
    )
    .await;
    let resp = srv
        .oneshot(platform_request(
            Method::POST,
            "/platform/admins",
            &session,
            Some(json!({ "name": "ops-lead", "match_claim": "ops@example.com" })),
        ))
        .await
        .expect("register route responds");
    assert_eq!(resp.status(), StatusCode::OK, "administrator registers");

    // 2. RFC 9207: an advertised issuer must come back, and exactly.
    for (label, iss) in [
        ("missing iss", None),
        ("another issuer's iss", Some("https://evil.example.com")),
    ] {
        let (state, nonce) = begin_platform_login(&srv).await;
        mount_platform_provider(
            &provider,
            platform_key_set("mock-1"),
            platform_token_reply(&issuer, &nonce, PLATFORM_CLIENT, "mock-1"),
        )
        .await;
        let (status, body) = platform_callback(&srv, &state, iss).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{label}: {body}");
        assert_eq!(
            platform_provider_hits(&provider, "/token").await,
            0,
            "{label}: the code never reaches the token endpoint"
        );
    }

    // 3. The ID token must be addressed to this client.
    let (state, nonce) = begin_platform_login(&srv).await;
    mount_platform_provider(
        &provider,
        platform_key_set("mock-1"),
        platform_token_reply(&issuer, &nonce, "another-client", "mock-1"),
    )
    .await;
    let (status, body) = platform_callback(&srv, &state, Some(&issuer)).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "another client's token: {body}"
    );

    // 4. The exact issuer completes, with platform authority only.
    let (state, nonce) = begin_platform_login(&srv).await;
    mount_platform_provider(
        &provider,
        platform_key_set("mock-1"),
        platform_token_reply(&issuer, &nonce, PLATFORM_CLIENT, "mock-1"),
    )
    .await;
    let (status, body) = platform_callback(&srv, &state, Some(&issuer)).await;
    assert_eq!(status, StatusCode::OK, "the exact issuer completes: {body}");
    let human = body["access_token"]
        .as_str()
        .expect("a platform session")
        .to_owned();
    let resp = srv
        .oneshot(platform_post(
            "/platform/tenants",
            &human,
            json!({ "slug": "federated-provisioned", "display_name": "Federated" }),
        ))
        .await
        .expect("tenant route responds");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "the signed-in administrator holds platform authority"
    );
    let resp = srv
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/v1/cards")
                .header(WYRD_ACCESS_TOKEN_HEADER, format!("Bearer {human}"))
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("route responds");
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "the federated platform session confers no access inside any tenant"
    );

    // 5. A discovery outage after begin does not break a callback whose
    //    provider state is cached.
    let (state, nonce) = begin_platform_login(&srv).await;
    mount_platform_outage(
        &provider,
        platform_token_reply(&issuer, &nonce, PLATFORM_CLIENT, "mock-1"),
    )
    .await;
    let (status, body) = platform_callback(&srv, &state, Some(&issuer)).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the cached provider completes: {body}"
    );
    assert_eq!(
        platform_provider_hits(&provider, PLATFORM_DISCOVERY).await
            + platform_provider_hits(&provider, "/jwks").await,
        0,
        "the callback fetched no discovery or key set"
    );

    // An unknown key forces one re-discovery, which the outage refuses.
    let (state, nonce) = begin_platform_login(&srv).await;
    mount_platform_outage(
        &provider,
        platform_token_reply(&issuer, &nonce, PLATFORM_CLIENT, "rotated-away"),
    )
    .await;
    let (status, body) = platform_callback(&srv, &state, Some(&issuer)).await;
    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "an unknown key during an outage fails closed: {body}"
    );
    assert_eq!(
        platform_provider_hits(&provider, PLATFORM_DISCOVERY).await,
        1,
        "exactly one forced re-discovery"
    );

    // 6. Reconfiguring the same issuer refreshes this process's provider: a
    //    key published only since the last discovery verifies afterwards
    //    without another discovery.
    mount_platform_provider(
        &provider,
        platform_key_set("mock-2"),
        wiremock::ResponseTemplate::new(500),
    )
    .await;
    let resp = srv
        .oneshot(configure(None))
        .await
        .expect("configure route responds");
    assert_eq!(resp.status(), StatusCode::OK, "same-issuer reconfiguration");
    let (state, nonce) = begin_platform_login(&srv).await;
    mount_platform_outage(
        &provider,
        platform_token_reply(&issuer, &nonce, PLATFORM_CLIENT, "mock-2"),
    )
    .await;
    let (status, body) = platform_callback(&srv, &state, Some(&issuer)).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the reconfigured provider's key verifies: {body}"
    );
    assert_eq!(
        platform_provider_hits(&provider, PLATFORM_DISCOVERY).await,
        0,
        "the callback used the provider reconfiguration discovered"
    );

    srv.shutdown().await.expect("server shuts down");
}

/// Client secret of the confidential platform connection the Transit journey
/// configures.
const PLATFORM_CLIENT_SECRET: &str = "platform-client-secret";

/// Complete one federated platform sign-in on `srv` against `provider`,
/// asserting the token endpoint received [`PLATFORM_CLIENT_SECRET`] in the
/// code exchange's body.
///
/// # Panics
/// Panics when the sign-in is refused or the exchange carried another secret.
async fn confidential_platform_sign_in(srv: &WyrdTestServer, provider: &wiremock::MockServer) {
    let issuer = provider.uri();
    let (state, nonce) = begin_platform_login(srv).await;
    mount_platform_provider(
        provider,
        platform_key_set("mock-1"),
        platform_token_reply(&issuer, &nonce, PLATFORM_CLIENT, "mock-1"),
    )
    .await;
    let (status, body) = platform_callback(srv, &state, Some(&issuer)).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the confidential sign-in completes: {body}"
    );
    let exchanges: Vec<String> = provider
        .received_requests()
        .await
        .expect("requests are recorded")
        .iter()
        .filter(|request| request.url.path() == "/token")
        .map(|request| String::from_utf8_lossy(&request.body).into_owned())
        .collect();
    assert_eq!(exchanges.len(), 1, "one code exchange: {exchanges:?}");
    assert!(
        exchanges[0].contains(&format!("client_secret={PLATFORM_CLIENT_SECRET}")),
        "the exchange authenticates with the opened secret: {}",
        exchanges[0]
    );
}

/// A confidential platform OIDC connection's client secret is sealed through
/// the deployment's remote wrapping backend and opened through it for the
/// code exchange, before and after a restart:
///   1. configuring the connection seals the secret with one Transit
///      encrypt, and the view never returns it;
///   2. a sign-in authenticates to the provider's token endpoint with the
///      opened secret, which took a Transit decrypt;
///   3. the restarted process opens the committed envelope through Transit
///      again and the next sign-in authenticates the same way.
///
/// # Panics
/// Panics when any step deviates from the contract above.
#[tokio::test]
async fn platform_sign_in_opens_its_client_secret_through_vault_transit_across_restart() {
    let transit = wyrd_testing::kms::TransitFixture::start().await;
    let srv = Box::pin(
        WyrdTestServer::builder()
            .with_kms_for_test(transit.config("platform", &[]))
            .start_bound(),
    )
    .await
    .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;
    let provider = wiremock::MockServer::start().await;
    mount_platform_provider(
        &provider,
        platform_key_set("mock-1"),
        wiremock::ResponseTemplate::new(500),
    )
    .await;

    // 1. Configuring seals through Transit; the view is redacted.
    let resp = srv
        .oneshot(platform_request(
            Method::PUT,
            "/platform/oidc/connection",
            &session,
            Some(json!({
                "issuer_url": provider.uri(),
                "client_id": PLATFORM_CLIENT,
                "client_auth": { "method": "secret_post", "secret": PLATFORM_CLIENT_SECRET },
            })),
        ))
        .await
        .expect("configure route responds");
    let status = resp.status();
    let view = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "connection configures: {view}");
    assert!(
        !view.to_string().contains(PLATFORM_CLIENT_SECRET),
        "the view never returns the secret: {view}"
    );
    assert_eq!(
        transit.requests("encrypt").await,
        1,
        "the secret seals once"
    );
    let resp = srv
        .oneshot(platform_request(
            Method::POST,
            "/platform/admins",
            &session,
            Some(json!({ "name": "ops-lead", "match_claim": "ops@example.com" })),
        ))
        .await
        .expect("register route responds");
    assert_eq!(resp.status(), StatusCode::OK, "administrator registers");

    // 2. The code exchange uses the secret Transit opened.
    confidential_platform_sign_in(&srv, &provider).await;
    let opened = transit.requests("decrypt").await;
    assert!(opened >= 1, "the sign-in opened the secret through Transit");
    // The first sign-in start also seals the platform derivation key once.
    let sealed = transit.requests("encrypt").await;

    // 3. The restarted process opens the committed envelope again.
    let srv = Box::pin(srv.restart_bound(WyrdTestServer::builder()))
        .await
        .expect("server restarts");
    confidential_platform_sign_in(&srv, &provider).await;
    assert!(
        transit.requests("decrypt").await > opened,
        "the restarted process opened the secret through Transit"
    );
    assert_eq!(
        transit.requests("encrypt").await,
        sealed,
        "nothing was resealed"
    );

    srv.shutdown().await.expect("server shuts down");
}

/// Renewing the platform connection's encryption reseals its unchanged client
/// secret under a fresh data key: one more Transit encrypt, a redacted view,
/// and the next sign-in authenticates with the same secret, with no
/// reconfiguration, through the route and the client handle. A key outage
/// refuses with nothing resealed; a removal racing a renewal wins and the
/// renewal conflicts; a removed connection has nothing to renew, and removal
/// is never undone by a renewal.
///
/// # Panics
/// Panics when any step deviates from the contract above.
#[tokio::test]
async fn renewing_platform_connection_encryption_keeps_its_sign_in() {
    let transit =
        wyrd_testing::kms::TransitFixture::start_delayed(std::time::Duration::from_millis(300))
            .await;
    let srv = Box::pin(
        WyrdTestServer::builder()
            .with_kms_for_test(transit.config("platform", &[]))
            .start_bound(),
    )
    .await
    .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;
    let provider = wiremock::MockServer::start().await;
    mount_platform_provider(
        &provider,
        platform_key_set("mock-1"),
        wiremock::ResponseTemplate::new(500),
    )
    .await;
    let resp = srv
        .oneshot(platform_request(
            Method::PUT,
            "/platform/oidc/connection",
            &session,
            Some(json!({
                "issuer_url": provider.uri(),
                "client_id": PLATFORM_CLIENT,
                "client_auth": { "method": "secret_post", "secret": PLATFORM_CLIENT_SECRET },
            })),
        ))
        .await
        .expect("configure route responds");
    assert_eq!(resp.status(), StatusCode::OK, "connection configures");
    let configured = body_json(resp).await;
    let resp = srv
        .oneshot(platform_request(
            Method::POST,
            "/platform/admins",
            &session,
            Some(json!({ "name": "ops-lead", "match_claim": "ops@example.com" })),
        ))
        .await
        .expect("register route responds");
    assert_eq!(resp.status(), StatusCode::OK, "administrator registers");

    let renew = || {
        srv.oneshot(platform_request(
            Method::POST,
            "/platform/oidc/connection/renew-encryption",
            &session,
            None,
        ))
    };
    let resp = renew().await.expect("renew route responds");
    let status = resp.status();
    let renewed = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "renewal succeeds: {renewed}");
    assert_eq!(
        renewed, configured,
        "the connection is unchanged and redacted"
    );
    assert_eq!(
        transit.requests("encrypt").await,
        2,
        "the unchanged secret is resealed once under a fresh data key"
    );
    confidential_platform_sign_in(&srv, &provider).await;

    // A key outage refuses the renewal and changes nothing. The sign-in
    // start above sealed the platform derivation key once.
    let sealed = transit.requests("encrypt").await;
    transit.set_available(false);
    let resp = renew().await.expect("renew route responds");
    transit.set_available(true);
    assert_eq!(
        resp.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "an outage refuses"
    );
    assert_eq!(
        transit.requests("encrypt").await,
        sealed,
        "the outage resealed nothing"
    );
    confidential_platform_sign_in(&srv, &provider).await;

    // A removal landing while the renewal reseals wins; the renewal conflicts.
    let encrypts = transit.requests("encrypt").await;
    let remove = async {
        while transit.requests("encrypt").await <= encrypts {
            tokio::task::yield_now().await;
        }
        srv.oneshot(platform_request(
            Method::DELETE,
            "/platform/oidc/connection",
            &session,
            None,
        ))
        .await
    };
    let (lost, removed) = tokio::join!(renew(), remove);
    assert_eq!(
        removed.expect("remove route responds").status(),
        StatusCode::NO_CONTENT,
        "connection removes"
    );
    assert_eq!(
        lost.expect("renew route responds").status(),
        StatusCode::CONFLICT,
        "the renewal loses to the removal"
    );
    let resp = renew().await.expect("renew route responds");
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "a removed connection has nothing to renew"
    );
    let resp = srv
        .oneshot(platform_request(
            Method::GET,
            "/platform/oidc/connection",
            &session,
            None,
        ))
        .await
        .expect("read route responds");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND, "removal stands");
    srv.shutdown().await.expect("server shuts down");
}

/// An operator configures federated sign-in, registers an administrator, and
/// can take it all away again without losing the deployment.
///
/// The provider secret is the thing to watch: it goes in once and must never
/// come back out of any read. And because federated login is additive, removing
/// the connection has to leave the global credential administering the platform
/// — otherwise a provider outage or a fat-fingered delete would lock the
/// operator out of their own deployment.
#[tokio::test]
async fn an_operator_configures_and_removes_federated_platform_sign_in() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;
    let provider = wyrd_testing::DiscoveryFixture::start().await;
    let issuer = provider.issuer();

    // Before any connection exists, a login attempt has nothing to resolve.
    let resp = srv
        .oneshot(anonymous_post(
            "/auth/platform/login",
            json!({ "redirect_uri": "https://wyrd.example/callback" }),
        ))
        .await
        .expect("login route responds");
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "federated login is unavailable until it is configured"
    );

    // Registering an administrator before the connection would create a
    // principal that could never sign in.
    let resp = srv
        .oneshot(platform_request(
            Method::POST,
            "/platform/admins",
            &session,
            Some(json!({ "name": "ops-lead", "match_claim": "ops@example.com" })),
        ))
        .await
        .expect("register route responds");
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "an administrator cannot be registered against no connection"
    );

    let secret = "super-secret-client-value";
    let resp = srv
        .oneshot(platform_request(
            Method::PUT,
            "/platform/oidc/connection",
            &session,
            Some(json!({
                "issuer_url": issuer,
                "client_id": "wyrd-platform",
                "client_auth": { "method": "secret_post", "secret": secret },
            })),
        ))
        .await
        .expect("configure route responds");
    let status = resp.status();
    let configured = body_json(resp).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "connection configures: {configured}"
    );

    let resp = srv
        .oneshot(platform_request(
            Method::GET,
            "/platform/oidc/connection",
            &session,
            None,
        ))
        .await
        .expect("read route responds");
    assert_eq!(resp.status(), StatusCode::OK, "connection reads back");
    let view = body_json(resp).await;
    assert_eq!(view["issuer_url"], issuer.as_str());
    assert_eq!(
        view["jwks_uri"],
        provider.jwks_uri(),
        "the JWKS endpoint comes from discovery, never from the request"
    );
    assert_eq!(view["client_auth"], "SecretPost");
    assert!(
        !serde_json::to_string(&view)
            .expect("view serializes")
            .contains(secret),
        "the provider secret never appears in a read: {view}"
    );

    let resp = srv
        .oneshot(platform_request(
            Method::POST,
            "/platform/admins",
            &session,
            Some(json!({ "name": "ops-lead", "match_claim": "ops@example.com" })),
        ))
        .await
        .expect("register route responds");
    let status = resp.status();
    let registered = body_json(resp).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "administrator registers: {registered}"
    );
    assert!(
        registered["principal_id"].is_string(),
        "a registered administrator is a durable principal"
    );

    // The same claim cannot be registered twice, or two principals would
    // compete to be pinned by one person's first login.
    let resp = srv
        .oneshot(platform_request(
            Method::POST,
            "/platform/admins",
            &session,
            Some(json!({ "name": "ops-lead-again", "match_claim": "ops@example.com" })),
        ))
        .await
        .expect("register route responds");
    assert_eq!(
        resp.status(),
        StatusCode::CONFLICT,
        "one matching claim registers at most one administrator"
    );

    // Registration is what confers authority, so the registered human can
    // administer the platform as soon as a provider verifies them — not after
    // some further grant step that does not exist.
    let registered_id: uuid::Uuid = registered["principal_id"]
        .as_str()
        .expect("principal id")
        .parse()
        .expect("principal id is a UUID");
    let human = srv
        .federated_platform_session(registered_id)
        .await
        .expect("a verified administrator takes a session");
    let human = secrecy::ExposeSecret::expose_secret(&human).to_owned();
    let resp = srv
        .oneshot(platform_post(
            "/platform/tenants",
            &human,
            json!({ "slug": "human-provisioned", "display_name": "Human" }),
        ))
        .await
        .expect("tenant route responds");
    let status = resp.status();
    let provisioned = body_json(resp).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the registered administrator holds the fixed platform grant: {provisioned}"
    );

    // And that authority is the platform's alone: the tenant it just created
    // cannot register a platform administrator or hand one a grant.
    let tenant_admin = tenant_token(
        &srv,
        provisioned["admin"]["credential"]
            .as_str()
            .expect("admin credential"),
    )
    .await
    .expect("the tenant administrator authenticates");
    let resp = srv
        .oneshot(platform_request(
            Method::POST,
            "/platform/admins",
            &tenant_admin,
            Some(json!({ "name": "escalation", "match_claim": "attacker@example.com" })),
        ))
        .await
        .expect("register route responds");
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "tenant authority never reaches the platform plane"
    );

    let resp = srv
        .oneshot(platform_request(
            Method::DELETE,
            "/platform/oidc/connection",
            &session,
            None,
        ))
        .await
        .expect("delete route responds");
    assert_eq!(resp.status(), StatusCode::NO_CONTENT, "connection removes");

    // The whole point: losing federated sign-in costs the deployment nothing
    // else. The global credential still administers the platform.
    let resp = srv
        .oneshot(platform_post(
            "/platform/tenants",
            &session,
            json!({ "slug": "after-oidc-removal", "display_name": "Still Working" }),
        ))
        .await
        .expect("tenant route responds");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "the global credential administers the platform with federated login gone"
    );

    // Both principals performed the same operation on the same plane, so the
    // retained kind is the only thing separating the machine root's decision
    // from the registered human's. A federated sign-in presents a provider
    // token, not a credential, so its decisions name none.
    let root_id: String =
        sqlx::query_scalar("SELECT id::text FROM platform.principals WHERE name = $1")
            .bind(wyrd_server::boot::init::PLATFORM_ROOT_NAME)
            .fetch_one(srv.operator_pool().pool())
            .await
            .expect("the root principal exists");
    let registered_id = registered_id.to_string();
    let decisions = system_audit_decisions(&srv).await;
    for (principal, expected) in [
        (root_id.as_str(), "global_admin"),
        (registered_id.as_str(), "user"),
    ] {
        let mut kinds = decisions
            .iter()
            .filter(|row| {
                row["operation"].as_deref() == Some("platform.authz")
                    && row["sub_principal_id"].as_deref() == Some(principal)
            })
            .inspect(|row| {
                assert_eq!(
                    row["act_principal_id"].as_deref(),
                    Some(principal),
                    "a platform principal acts for itself"
                );
            })
            .map(|row| row["sub_principal_kind"].clone())
            .collect::<Vec<_>>();
        kinds.sort_unstable();
        kinds.dedup();
        assert_eq!(
            kinds,
            vec![Some(expected.to_owned())],
            "decisions by {principal} were retained as {kinds:?}, expected only {expected}"
        );
    }
    assert!(
        decisions
            .iter()
            .filter(|row| row["sub_principal_id"].as_deref() == Some(registered_id.as_str()))
            .all(|row| row["credential_id"].is_none()),
        "a federated session names no credential, so its decisions must not claim one"
    );
}

/// A tenant administrator cannot configure who signs in to the platform.
#[tokio::test]
async fn a_tenant_administrator_cannot_configure_platform_sign_in() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let admin = provisioned_tenant_admin(&srv, "outsider").await;

    let resp = srv
        .oneshot(platform_request(
            Method::PUT,
            "/platform/oidc/connection",
            &admin,
            Some(json!({
                "issuer_url": "https://attacker.example.com/",
                "client_id": "wyrd-platform",
                "client_auth": { "method": "public" },
            })),
        ))
        .await
        .expect("configure route responds");
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "a tenant token cannot point the platform plane at another issuer"
    );
}

/// A connection cannot point the server's key fetches at a blocked address.
///
/// Configuring a connection is the only place an operator names an outbound
/// host, and the anonymous login route then drives fetches to it. Without
/// screening this route would be an SSRF primitive.
#[tokio::test]
async fn a_connection_cannot_name_an_unresolvable_issuer() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;

    let resp = srv
        .oneshot(platform_request(
            Method::PUT,
            "/platform/oidc/connection",
            &session,
            Some(json!({
                "issuer_url": "https://wyrd-invalid.invalid/realms/platform",
                "client_id": "wyrd-platform",
                "client_auth": { "method": "public" },
            })),
        ))
        .await
        .expect("configure route responds");
    assert_ne!(
        resp.status(),
        StatusCode::OK,
        "an issuer that cannot be screened and discovered is not stored"
    );

    let resp = srv
        .oneshot(platform_request(
            Method::GET,
            "/platform/oidc/connection",
            &session,
            None,
        ))
        .await
        .expect("read route responds");
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "a refused configuration leaves no connection behind"
    );
}

/// An operator can see who administers the platform, and stop one of them.
///
/// Without this the active-status check every platform request performs is a
/// dormant guard: nothing in the deployment could make a platform principal
/// inactive, so an administrator whose identity was pinned in error would be
/// unremovable without direct database access.
#[tokio::test]
async fn an_operator_lists_and_suspends_platform_administrators() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;
    let provider = wyrd_testing::DiscoveryFixture::start().await;

    srv.oneshot(platform_request(
        Method::PUT,
        "/platform/oidc/connection",
        &session,
        Some(json!({
            "issuer_url": provider.issuer(),
            "client_id": "wyrd-platform",
            "client_auth": { "method": "public" },
        })),
    ))
    .await
    .expect("configure route responds");

    let registered = body_json(
        srv.oneshot(platform_request(
            Method::POST,
            "/platform/admins",
            &session,
            Some(json!({ "name": "ops-lead", "match_claim": "ops@example.com" })),
        ))
        .await
        .expect("register route responds"),
    )
    .await;
    let admin_id = registered["principal_id"].as_str().expect("principal id");

    let listed = body_json(
        srv.oneshot(platform_request(
            Method::GET,
            "/platform/admins",
            &session,
            None,
        ))
        .await
        .expect("list route responds"),
    )
    .await;
    let principals = listed["principals"].as_array().expect("principal list");
    let registered_row = principals
        .iter()
        .find(|row| row["principal_id"] == admin_id)
        .expect("the registered administrator is listed");
    assert_eq!(registered_row["status"], "active");
    assert_eq!(registered_row["match_claim"], "ops@example.com");
    assert!(
        registered_row["subject"].is_null(),
        "an administrator who has never signed in has no pinned subject"
    );

    let suspend = |target: &str, status: &str| {
        platform_request(
            Method::PUT,
            &format!("/platform/admins/{target}/status"),
            &session,
            Some(json!({ "status": status })),
        )
    };
    let resp = srv
        .oneshot(suspend(admin_id, "suspended"))
        .await
        .expect("status route responds");
    assert_eq!(resp.status(), StatusCode::NO_CONTENT, "suspension succeeds");

    let listed = body_json(
        srv.oneshot(platform_request(
            Method::GET,
            "/platform/admins",
            &session,
            None,
        ))
        .await
        .expect("list route responds"),
    )
    .await;
    assert_eq!(
        listed["principals"]
            .as_array()
            .expect("principal list")
            .iter()
            .find(|row| row["principal_id"] == admin_id)
            .expect("a suspended administrator is still listed")["status"],
        "suspended",
        "the listing shows a revoked administrator rather than hiding it"
    );
}

/// The deployment cannot be locked out of its own control plane.
#[tokio::test]
async fn the_last_active_platform_principal_cannot_be_suspended() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;

    let listed = body_json(
        srv.oneshot(platform_request(
            Method::GET,
            "/platform/admins",
            &session,
            None,
        ))
        .await
        .expect("list route responds"),
    )
    .await;
    let principals = listed["principals"].as_array().expect("principal list");
    assert_eq!(
        principals.len(),
        1,
        "a freshly initialized deployment has exactly one platform principal"
    );
    let root_id = principals[0]["principal_id"]
        .as_str()
        .expect("principal id")
        .to_owned();

    let resp = srv
        .oneshot(platform_request(
            Method::PUT,
            &format!("/platform/admins/{root_id}/status"),
            &session,
            Some(json!({ "status": "suspended" })),
        ))
        .await
        .expect("status route responds");
    assert_eq!(
        resp.status(),
        StatusCode::CONFLICT,
        "suspending the only way in is refused"
    );

    // And the deployment still works.
    let resp = srv
        .oneshot(platform_post(
            "/platform/tenants",
            &session,
            json!({ "slug": "still-administrable", "display_name": "Still Administrable" }),
        ))
        .await
        .expect("tenant route responds");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "the refused suspension left the deployment administrable"
    );
}

/// Suspending a tenant stops its credentials working.
///
/// Tenant lifecycle states existed before this and were read nowhere on the
/// authentication path, which made suspension a label rather than a control.
/// The check lives at credential exchange, the one place every credential-
/// bearing entry to a tenant converges.
#[tokio::test]
async fn a_suspended_tenant_admits_no_credential() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;

    let created = body_json(
        srv.oneshot(platform_post(
            "/platform/tenants",
            &session,
            json!({ "slug": "suspendable", "display_name": "Suspendable" }),
        ))
        .await
        .expect("tenant route responds"),
    )
    .await;
    let credential = created["admin"]["credential"]
        .as_str()
        .expect("credential")
        .to_owned();
    let tenant_id = created["tenant"]["id"].as_str().expect("tenant id");

    assert!(
        tenant_token(&srv, &credential).await.is_ok(),
        "an active tenant's credential authenticates"
    );

    sqlx::query("UPDATE platform.tenants SET status = 'suspended' WHERE data_tenant_id = $1")
        .bind(tenant_id.parse::<uuid::Uuid>().expect("tenant uuid"))
        .execute(srv.operator_pool().pool())
        .await
        .expect("tenant suspends");

    assert_eq!(
        tenant_token(&srv, &credential).await.unwrap_err(),
        StatusCode::BAD_REQUEST,
        "a suspended tenant's credential stops authenticating"
    );

    // Restoring the tenant restores its credentials, so suspension is
    // reversible rather than destructive.
    sqlx::query("UPDATE platform.tenants SET status = 'active' WHERE data_tenant_id = $1")
        .bind(tenant_id.parse::<uuid::Uuid>().expect("tenant uuid"))
        .execute(srv.operator_pool().pool())
        .await
        .expect("tenant restores");
    assert!(
        tenant_token(&srv, &credential).await.is_ok(),
        "restoring the tenant restores its credentials"
    );
}

/// A failed provisioning can be retried with the same slug.
///
/// A bare insert would burn the slug permanently: the row exists, every retry
/// of the identical request conflicts, and the operator can never create that
/// tenant. Resuming keeps the original tenant id, because the failed attempt
/// may already have written rows under it.
#[tokio::test]
async fn a_failed_provisioning_can_be_retried_with_the_same_slug() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;

    let created = body_json(
        srv.oneshot(platform_post(
            "/platform/tenants",
            &session,
            json!({ "slug": "retryable", "display_name": "Retryable" }),
        ))
        .await
        .expect("tenant route responds"),
    )
    .await;
    let original_id = created["tenant"]["id"]
        .as_str()
        .expect("tenant id")
        .to_owned();

    // Put the tenant in the state a mid-provisioning failure leaves behind.
    sqlx::query(
        "UPDATE platform.tenants
            SET status = 'failed', provisioning_failed_reason = 'simulated failure'
          WHERE data_tenant_id = $1",
    )
    .bind(original_id.parse::<uuid::Uuid>().expect("tenant uuid"))
    .execute(srv.operator_pool().pool())
    .await
    .expect("tenant marks failed");

    let resp = srv
        .oneshot(platform_post(
            "/platform/tenants",
            &session,
            json!({ "slug": "retryable", "display_name": "Retryable" }),
        ))
        .await
        .expect("tenant route responds");
    let status = resp.status();
    let retried = body_json(resp).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "retrying a failed provisioning succeeds rather than reporting the slug taken: {retried}"
    );
    assert_eq!(
        retried["tenant"]["id"].as_str().expect("tenant id"),
        original_id,
        "the retry resumes the original tenant rather than creating a second"
    );
    assert_eq!(retried["tenant"]["status"], "active");

    // The credential the retry hands back is usable, which is what makes the
    // resumed tenant genuinely provisioned rather than merely marked active.
    assert!(
        tenant_token(
            &srv,
            retried["admin"]["credential"].as_str().expect("credential")
        )
        .await
        .is_ok(),
        "the resumed tenant's administrator authenticates"
    );

    // The first attempt committed a credential whose plaintext went nowhere,
    // so the retry has to retire it. Two usable ways in, one of them held by
    // nobody, is the outcome this stage exists to prevent.
    assert!(
        tenant_token(
            &srv,
            created["admin"]["credential"].as_str().expect("credential")
        )
        .await
        .is_err(),
        "the abandoned attempt's credential no longer authenticates"
    );

    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool for the tenant's identities");
    let tenant_uuid = original_id.parse::<uuid::Uuid>().expect("tenant uuid");
    let administrators: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wyrd.auth_service_accounts
          WHERE data_tenant_id = $1 AND principal_kind = 'tenant_admin'",
    )
    .bind(tenant_uuid)
    .fetch_one(&superuser)
    .await
    .expect("administrator count reads");
    assert_eq!(
        administrators, 1,
        "the retry reuses the administrator rather than creating a second"
    );

    let usable: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wyrd.auth_api_keys
          WHERE data_tenant_id = $1
            AND revoked_at IS NULL
            AND (expires_at IS NULL OR expires_at > now())",
    )
    .bind(tenant_uuid)
    .fetch_one(&superuser)
    .await
    .expect("usable credential count reads");
    assert_eq!(
        usable, 1,
        "exactly one credential is usable, and it is the one the retry disclosed"
    );

    // Both creation decisions name the slug. The retry proposed a fresh tenant
    // id that the directory discarded in favour of the original, so a decision
    // retained against a proposed id would name a tenant that does not exist.
    let creation_resources = system_audit_decisions(&srv)
        .await
        .into_iter()
        .filter(|row| {
            row["operation"].as_deref() == Some("platform.authz")
                && row["permission"].as_deref() == Some("tenants:write")
        })
        .map(|row| row["resource"].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        creation_resources,
        vec![Some("tenant_slug:retryable".to_owned()); 2],
        "every provisioning decision names the requested slug, never a proposed tenant id"
    );
}

/// An occupied slug is still a conflict.
#[tokio::test]
async fn an_active_tenant_slug_is_still_refused() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;

    let create = || {
        platform_post(
            "/platform/tenants",
            &session,
            json!({ "slug": "taken", "display_name": "Taken" }),
        )
    };
    assert_eq!(
        srv.oneshot(create())
            .await
            .expect("tenant route responds")
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        srv.oneshot(create())
            .await
            .expect("tenant route responds")
            .status(),
        StatusCode::CONFLICT,
        "a live tenant's slug is a genuine collision, not a resumable failure"
    );
}

/// Internal strings a served failure must never carry.
///
/// Each names a layer behind the boundary — the database and its schema, the
/// SQLx and `serde_json` message shapes, the PL/pgSQL fault this test injects.
/// A caller that can see any of them can map the server's internals from
/// ordinary error responses.
const INTERNAL_FRAGMENTS: [&str; 5] =
    ["platform.", "sqlx", "plpgsql", "expected value", "injected"];

/// Assert a problem body is the stable public shape and nothing more.
///
/// `details` is checked as a whole rather than key by key: a scrubbed boundary
/// adds no key at all, so anything present is a leak this test should fail on.
fn assert_safe_problem(body: &Value, code: &str) {
    assert_eq!(body["code"], code, "stable catalog code: {body}");
    assert_eq!(body["details"], json!({}), "details carry no cause: {body}");
    let rendered = body.to_string().to_lowercase();
    for fragment in INTERNAL_FRAGMENTS {
        assert!(
            !rendered.contains(fragment),
            "served failure leaks {fragment}: {body}"
        );
    }
}

/// An unreadable grant fails safely.
///
/// A grant row the server itself cannot deserialize is a failure a caller has
/// no business seeing; it used to serialize its source text into `details`.
#[tokio::test]
async fn served_platform_failures_disclose_nothing_internal() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;

    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");

    // A grant row the server cannot read back is a serialization failure on the
    // authenticated path itself, before any handler runs.
    sqlx::query("UPDATE platform.principal_grants SET permissions = '\"not-a-permission-list\"'")
        .execute(&superuser)
        .await
        .expect("grant corrupts");

    let resp = srv
        .oneshot(platform_post(
            "/platform/tenants",
            &session,
            json!({ "slug": "corrupt-grant", "display_name": "Corrupt" }),
        ))
        .await
        .expect("tenant route responds");
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_safe_problem(&body_json(resp).await, "WYRD_SPEC_500_INTERNAL");
}

/// The deployment root is rotatable through Wyrd, with no outage and no SQL.
///
/// This is the journey the platform credential routes exist for. Until they
/// shipped, a leaked root credential had no remedy inside the product: issuing
/// was internal and revoking was reachable only from a test. The sequence an
/// operator actually performs is issue → verify → revoke, in that order, and it
/// has to hold end to end: the replacement must administer the platform before
/// the original is retired, the listing must show both so the operator can see
/// what they are retiring, and retiring the original must end its sessions
/// while leaving the replacement's untouched.
#[tokio::test]
async fn an_operator_rotates_the_deployment_root_credential() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;

    let resp = srv
        .oneshot(platform_request(
            Method::GET,
            "/platform/admins",
            &session,
            None,
        ))
        .await
        .expect("listing responds");
    assert_eq!(resp.status(), StatusCode::OK);
    let principal_id = body_json(resp).await["principals"][0]["principal_id"]
        .as_str()
        .expect("the deployment root is listed")
        .to_owned();

    let resp = srv
        .oneshot(platform_post(
            &format!("/platform/admins/{principal_id}/credentials"),
            &session,
            json!({ "expires_in_days": 30 }),
        ))
        .await
        .expect("issue responds");
    assert_eq!(resp.status(), StatusCode::OK);
    let issued = body_json(resp).await;
    let replacement = issued["credential"]
        .as_str()
        .expect("the plaintext is returned once")
        .to_owned();

    // The replacement works before anything is retired, which is what makes the
    // rotation outage-free rather than a window with no usable credential.
    let rotated = platform_session(&srv, &replacement).await;
    let resp = srv
        .oneshot(platform_post(
            "/platform/tenants",
            &rotated,
            json!({ "slug": "rotated-root", "display_name": "Rotated" }),
        ))
        .await
        .expect("tenant route responds");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "the replacement administers the platform before the original is retired"
    );

    let resp = srv
        .oneshot(platform_request(
            Method::GET,
            &format!("/platform/admins/{principal_id}/credentials"),
            &rotated,
            None,
        ))
        .await
        .expect("listing responds");
    assert_eq!(resp.status(), StatusCode::OK);
    let listing = body_json(resp).await;
    let credentials = listing["credentials"]
        .as_array()
        .expect("credentials are listed");
    assert_eq!(
        credentials.len(),
        2,
        "the operator sees the original and its replacement: {listing}"
    );
    assert!(
        !listing.to_string().contains(&replacement),
        "a listing never carries credential material"
    );
    let original = credentials
        .iter()
        .find(|credential| credential["id"] != issued["id"])
        .expect("the original is still listed")["id"]
        .as_str()
        .expect("credential ids are strings")
        .to_owned();

    let resp = srv
        .oneshot(platform_request(
            Method::DELETE,
            &format!("/platform/admins/{principal_id}/credentials/{original}"),
            &rotated,
            None,
        ))
        .await
        .expect("revoke responds");
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let resp = srv
        .oneshot(platform_post(
            "/platform/tenants",
            &session,
            json!({ "slug": "after-rotation", "display_name": "After" }),
        ))
        .await
        .expect("tenant route responds");
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "the retired credential's live session stops administering the platform"
    );

    let resp = srv
        .oneshot(platform_post(
            "/platform/tenants",
            &rotated,
            json!({ "slug": "still-administrable", "display_name": "Still" }),
        ))
        .await
        .expect("tenant route responds");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "retiring the original leaves the replacement's session working"
    );

    // Replaying the revoke is refused rather than silently accepted, so an
    // operator cannot mistake a no-op for a second retirement.
    let resp = srv
        .oneshot(platform_request(
            Method::DELETE,
            &format!("/platform/admins/{principal_id}/credentials/{original}"),
            &rotated,
            None,
        ))
        .await
        .expect("revoke responds");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // The credential's non-secret id is what travels into audit; the secret
    // itself never reaches a retained row.
    let decisions = system_audit_decisions(&srv).await;
    let replacement_id = issued["id"].as_str().expect("the replacement's id");
    assert!(
        decisions.iter().any(|row| {
            row["sub_principal_id"].as_deref() == Some(principal_id.as_str())
                && row["credential_id"].as_deref() == Some(replacement_id)
        }),
        "the replacement session's decisions name its credential id"
    );
    assert!(
        decisions
            .iter()
            .flat_map(|row| row.values().flatten())
            .all(|value| !value.contains(&replacement)),
        "no retained audit row carries credential material"
    );
}

/// A tenant administrator ends a compromised identity outright and the reason
/// survives in the audit record.
///
/// Principal revocation is the blunt instrument next to credential rotation,
/// and the only part of it that cannot be reconstructed afterwards is why an
/// operator reached for it. The journey therefore proves three things a
/// generated client depends on: the declared kind selects the table, a reason
/// that looks like a credential is refused rather than persisted, and an
/// accepted reason is what the audit row carries. A token the principal held
/// before the revocation is refused as inactive at its next request.
#[tokio::test]
async fn a_tenant_revokes_a_compromised_principal_with_its_reason() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let admin = provisioned_tenant_admin(&srv, "compromised").await;

    let resp = srv
        .oneshot_authenticated(
            &admin,
            tenant_request(
                Method::POST,
                "/v1/principals",
                Some(json!({ "name": "leaked-runner", "assignments": [{ "role": "viewer", "scope": "all" }] })),
            ),
        )
        .await
        .expect("principal route responds");
    let created = body_json(resp).await;
    let principal_id = created["principal_id"]
        .as_str()
        .expect("principal id")
        .to_owned();
    let credential = created["credential"]
        .as_str()
        .expect("credential")
        .to_owned();

    let live_token = tenant_token(&srv, &credential)
        .await
        .expect("the credential mints a token");

    let revoke = |body: serde_json::Value| {
        srv.oneshot_authenticated(
            &admin,
            tenant_request(
                Method::POST,
                &format!("/v1/principals/{principal_id}/revoke"),
                Some(body),
            ),
        )
    };

    let resp = revoke(json!({
        "principal_kind": "service",
        "reason": "Bearer eyJhbGciOiJIUzI1NiJ9.leaked"
    }))
    .await
    .expect("revoke route responds");
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "a secret-like reason is refused before anything is recorded"
    );

    let resp = revoke(json!({ "principal_kind": "user", "reason": "wrong table" }))
        .await
        .expect("revoke route responds");
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "the declared kind selects the table rather than hinting at it"
    );

    // An unknown id is the same authorized miss as a wrong kind.
    let unknown = uuid::Uuid::now_v7();
    let resp = srv
        .oneshot_authenticated(
            &admin,
            tenant_request(
                Method::POST,
                &format!("/v1/principals/{unknown}/revoke"),
                Some(json!({ "principal_kind": "service", "reason": "no such runner" })),
            ),
        )
        .await
        .expect("revoke route responds");
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "an unknown principal is not found"
    );

    assert!(
        tenant_token(&srv, &credential).await.is_ok(),
        "no refusal revoked anything"
    );

    let reason = "leaked-runner key found in a public build log";
    let resp = revoke(json!({ "principal_kind": "service", "reason": reason }))
        .await
        .expect("revoke route responds");
    assert_eq!(resp.status(), StatusCode::OK, "the revocation is accepted");

    let cards_status = || async {
        srv.oneshot_authenticated(&live_token, tenant_request(Method::GET, "/v1/cards", None))
            .await
            .expect("cards route responds")
            .status()
    };
    within_propagation(&StatusCode::UNAUTHORIZED, cards_status).await;
    let resp = srv
        .oneshot_authenticated(&live_token, tenant_request(Method::GET, "/v1/cards", None))
        .await
        .expect("cards route responds");
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "a token minted before the revocation spends current authority, and the account is inactive"
    );
    let problem: Value = serde_json::from_slice(
        &to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("problem body reads"),
    )
    .expect("problem is JSON");
    assert_eq!(problem["details"]["reason"], "inactive", "{problem}");
    assert_eq!(
        tenant_token(&srv, &credential).await.unwrap_err(),
        StatusCode::BAD_REQUEST,
        "the suspended principal's credential mints nothing new"
    );

    // Two decisions, not three. The secret-like reason never reached the
    // authorization boundary. The wrong-kind attempt was an authorized decision
    // that found nothing: its allowance commits with no effect before the
    // `404`, so an operator can see the attempt. The accepted revocation
    // commits its allowance together with the suspension. No tenant-plane
    // route projects the retained audit row, so it is read directly.
    let tenant = tenant_named(&srv, "compromised").await;
    srv.await_audit_retained()
        .await
        .expect("audit retention settles");
    let unknown_decisions = srv
        .retained_audit_records(
            tenant,
            "operation",
            &format!("operation = 'auth.principal.revoke' AND resource = 'principal:{unknown}'"),
        )
        .await
        .expect("the unknown-id decision is audited")
        .len();
    assert_eq!(
        unknown_decisions, 1,
        "the unknown-id miss commits exactly one allowed decision"
    );
    let rows: Vec<Option<String>> = srv
        .retained_audit_records(
            tenant,
            "detail",
            &format!(
                "operation = 'auth.principal.revoke' AND resource = 'principal:{principal_id}'"
            ),
        )
        .await
        .expect("the revocation decisions are audited")
        .into_iter()
        .map(|mut row| row.remove(0))
        .collect();
    // `detail` is retained as canonical JSON text, so it is parsed here.
    let decisions: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|row| {
            row.map_or(serde_json::Value::Null, |text| {
                serde_json::from_str(&text).expect("an audit detail is canonical JSON")
            })
        })
        .collect();
    assert_eq!(
        decisions.len(),
        2,
        "the wrong-kind miss and the accepted revocation are recorded: {decisions:?}"
    );
    assert_eq!(decisions[0]["principal_kind"], "user");
    assert_eq!(decisions[0]["reason"], "wrong table");

    let detail = decisions.last().expect("the accepted decision is last");
    assert_eq!(detail["kind"], "principal_revocation");
    assert_eq!(detail["reason"], reason);
    assert_eq!(detail["principal_kind"], "service");
    assert_eq!(detail["principal_id"], principal_id);
}

/// Recovery is refused for every tenant state but `active`, and the refusal
/// costs the tenant nothing.
///
/// Recovery is the one path that mints a durable secret into a tenant from
/// outside it, so the directory state is the gate: a provisioning, failed,
/// suspended, or soft-deleted tenant must not acquire a working key into state
/// the deployment has frozen or abandoned. Every refusal shares one
/// non-enumerating shape, and each is checked against the credential count so
/// a refusal that still wrote is caught. The active case closes the loop:
/// the same principal comes back and its grants still administer the tenant.
#[tokio::test]
async fn recovery_is_refused_for_every_state_but_active() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;

    let created = body_json(
        srv.oneshot(platform_post(
            "/platform/tenants",
            &session,
            json!({ "slug": "frozen", "display_name": "Frozen" }),
        ))
        .await
        .expect("tenant route responds"),
    )
    .await;
    let tenant_id = created["tenant"]["id"]
        .as_str()
        .expect("tenant id")
        .to_owned();
    let original_principal = created["admin"]["principal_id"]
        .as_str()
        .expect("principal id")
        .to_owned();

    // Read past RLS on purpose: the assertion is that no durable secret was
    // written, which no tenant-plane route projects while the tenant is frozen.
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    let credentials = async |tenant: &str| -> i64 {
        sqlx::query_scalar(
            "SELECT count(*) FROM wyrd.auth_api_keys WHERE data_tenant_id = $1::uuid",
        )
        .bind(tenant.to_owned())
        .fetch_one(&superuser)
        .await
        .expect("credentials are counted")
    };
    let before = credentials(&tenant_id).await;

    for state in ["provisioning", "suspended", "failed", "deleted"] {
        sqlx::query(
            "UPDATE platform.tenants \
               SET status = $1, \
                   provisioning_failed_reason = CASE WHEN $1 = 'failed' THEN 'injected' END \
             WHERE data_tenant_id = $2::uuid",
        )
        .bind(state)
        .bind(tenant_id.clone())
        .execute(srv.operator_pool().pool())
        .await
        .expect("the directory state is set");

        let resp = srv
            .oneshot(platform_post(
                "/platform/tenants/admin/credentials",
                &session,
                json!({ "tenant_id": tenant_id }),
            ))
            .await
            .expect("recovery route responds");
        let status = resp.status();
        let body = body_json(resp).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "a {state} tenant is refused recovery: {body}"
        );
        assert!(
            !body.to_string().contains(state),
            "the refusal does not disclose which state it is: {body}"
        );
        assert_eq!(
            credentials(&tenant_id).await,
            before,
            "a refused recovery writes no credential for a {state} tenant"
        );
    }

    // Restored to active, the same principal is recovered and still administers.
    sqlx::query(
        "UPDATE platform.tenants \
           SET status = 'active', provisioning_failed_reason = NULL \
         WHERE data_tenant_id = $1::uuid",
    )
    .bind(tenant_id.clone())
    .execute(srv.operator_pool().pool())
    .await
    .expect("the directory state is restored");

    let resp = srv
        .oneshot(platform_post(
            "/platform/tenants/admin/credentials",
            &session,
            json!({ "tenant_id": tenant_id }),
        ))
        .await
        .expect("recovery route responds");
    let status = resp.status();
    let recovered = body_json(resp).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "an active tenant recovers: {recovered}"
    );
    assert_eq!(
        recovered["principal_id"].as_str().expect("principal id"),
        original_principal,
        "recovery restores the existing principal rather than creating a second"
    );

    let token = tenant_token(&srv, recovered["credential"].as_str().expect("credential"))
        .await
        .expect("the replacement credential exchanges");
    let resp = srv
        .oneshot_authenticated(
            &token,
            tenant_request(
                Method::POST,
                "/v1/principals",
                Some(json!({ "name": "post-recovery", "assignments": [{ "role": "viewer", "scope": "all" }] })),
            ),
        )
        .await
        .expect("principal route responds");
    let status = resp.status();
    let body = body_json(resp).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the recovered principal keeps the grants it had: {body}"
    );
}

/// One tenant's administrator cannot reach another tenant's identities, and
/// learns nothing about them from the refusal.
///
/// Row-level security and composite keys make the isolation true in the store;
/// this drives it through the served product path, which is where a real caller
/// would discover a leak. Every attempt names a real principal and credential
/// belonging to the other tenant, so a refusal proves isolation rather than a
/// bad identifier. The refusals are `404`, not `403`: a `403` would confirm the
/// target exists.
#[tokio::test]
async fn one_tenant_cannot_reach_another_tenants_identities() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;

    let provision = async |slug: &str| -> Value {
        body_json(
            srv.oneshot(platform_post(
                "/platform/tenants",
                &session,
                json!({ "slug": slug, "display_name": slug }),
            ))
            .await
            .expect("tenant route responds"),
        )
        .await
    };
    let alpha = provision("isolation-alpha").await;
    let beta = provision("isolation-beta").await;

    let alpha_admin = tenant_token(
        &srv,
        alpha["admin"]["credential"].as_str().expect("credential"),
    )
    .await
    .expect("tenant alpha administers");
    let beta_admin = tenant_token(
        &srv,
        beta["admin"]["credential"].as_str().expect("credential"),
    )
    .await
    .expect("tenant beta administers");

    // Tenant beta owns a principal with a live credential, which alpha will
    // name directly in every attempt below.
    let created = body_json(
        srv.oneshot_authenticated(
            &beta_admin,
            tenant_request(
                Method::POST,
                "/v1/principals",
                Some(json!({ "name": "beta-runner", "assignments": [{ "role": "viewer", "scope": "all" }] })),
            ),
        )
        .await
        .expect("principal route responds"),
    )
    .await;
    let target = created["principal_id"]
        .as_str()
        .expect("principal id")
        .to_owned();
    let target_credential = created["credential"]
        .as_str()
        .expect("credential")
        .to_owned();
    let listed = body_json(
        srv.oneshot_authenticated(
            &beta_admin,
            tenant_request(
                Method::GET,
                &format!("/v1/principals/{target}/credentials"),
                None,
            ),
        )
        .await
        .expect("credential listing responds"),
    )
    .await;
    let target_credential_id = listed["credentials"][0]["id"]
        .as_str()
        .expect("credential id")
        .to_owned();

    let attempts: Vec<(&str, Request<Body>)> = vec![
        (
            "list credentials",
            tenant_request(
                Method::GET,
                &format!("/v1/principals/{target}/credentials"),
                None,
            ),
        ),
        (
            "issue a credential",
            tenant_request(
                Method::POST,
                &format!("/v1/principals/{target}/credentials"),
                Some(json!({})),
            ),
        ),
        (
            "revoke a credential",
            tenant_request(
                Method::DELETE,
                &format!("/v1/principals/{target}/credentials/{target_credential_id}"),
                None,
            ),
        ),
        (
            "revoke the principal",
            tenant_request(
                Method::POST,
                &format!("/v1/principals/{target}/revoke"),
                Some(json!({ "principal_kind": "service", "reason": "cross-tenant attempt" })),
            ),
        ),
    ];
    for (what, request) in attempts {
        let resp = srv
            .oneshot_authenticated(&alpha_admin, request)
            .await
            .expect("route responds");
        let status = resp.status();
        let body = body_json(resp).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "tenant alpha cannot {what} in tenant beta: {body}"
        );
        assert!(
            !body.to_string().contains("beta-runner"),
            "the refusal discloses nothing about the target: {body}"
        );
    }

    // Beta's credential still authenticates as beta, and does so as beta only:
    // none of the attempts above disturbed it.
    let beta_runner = tenant_token(&srv, &target_credential)
        .await
        .expect("the target credential still exchanges");
    let resp = srv
        .oneshot_authenticated(
            &beta_runner,
            tenant_request(
                Method::POST,
                "/v1/principals",
                Some(json!({ "name": "escalated", "assignments": [{ "role": "admin", "scope": "all" }] })),
            ),
        )
        .await
        .expect("principal route responds");
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "a reader in beta administers neither tenant"
    );

    // Recovery is a platform capability, so a tenant administrator cannot use
    // it against any tenant, its own included.
    for (what, tenant) in [("its own", &alpha), ("another", &beta)] {
        let resp = srv
            .oneshot_authenticated(
                &alpha_admin,
                tenant_request(
                    Method::POST,
                    "/platform/tenants/admin/credentials",
                    Some(json!({ "tenant_id": tenant["tenant"]["id"] })),
                ),
            )
            .await
            .expect("recovery route responds");
        assert!(
            resp.status() == StatusCode::UNAUTHORIZED || resp.status() == StatusCode::FORBIDDEN,
            "a tenant administrator cannot recover {what} tenant: {}",
            resp.status()
        );
    }
}

/// An operator administers the tenant lifecycle without touching the database.
///
/// Suspension has to mean the tenant stops admitting callers, not merely that a
/// column changed, so both halves are checked: a credential that has not been
/// exchanged yet is refused, and a token minted *before* the suspension stops
/// working too. Resuming restores exactly what was there — the same
/// administrative principal, still holding the grants it had — because
/// suspension freezes a tenant rather than dismantling it.
#[tokio::test]
async fn an_operator_suspends_and_resumes_a_tenant_through_the_platform_plane() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;

    let created = body_json(
        srv.oneshot(platform_post(
            "/platform/tenants",
            &session,
            json!({ "slug": "lifecycle", "display_name": "Lifecycle" }),
        ))
        .await
        .expect("tenant route responds"),
    )
    .await;
    let tenant_id = created["tenant"]["id"]
        .as_str()
        .expect("tenant id")
        .to_owned();
    let admin_credential = created["admin"]["credential"]
        .as_str()
        .expect("credential")
        .to_owned();
    let admin_principal = created["admin"]["principal_id"]
        .as_str()
        .expect("principal id")
        .to_owned();

    // The directory is readable without SQL, in every lifecycle state.
    let listing = body_json(
        srv.oneshot(platform_request(
            Method::GET,
            "/platform/tenants",
            &session,
            None,
        ))
        .await
        .expect("tenant listing responds"),
    )
    .await;
    assert!(
        listing["tenants"]
            .as_array()
            .expect("tenants are listed")
            .iter()
            .any(|tenant| tenant["id"] == tenant_id.as_str() && tenant["status"] == "active"),
        "the new tenant is in the directory as active: {listing}"
    );

    let inspected = body_json(
        srv.oneshot(platform_request(
            Method::GET,
            &format!("/platform/tenants/{tenant_id}"),
            &session,
            None,
        ))
        .await
        .expect("tenant inspect responds"),
    )
    .await;
    assert_eq!(
        inspected["slug"], "lifecycle",
        "inspection reads one tenant: {inspected}"
    );

    // A token minted before the suspension is the interesting one: suspension
    // governs issuance, and an issued token spends its account's current
    // authority, which suspending the tenant does not withdraw.
    let live_token = tenant_token(&srv, &admin_credential)
        .await
        .expect("the tenant administers before suspension");
    let primed = srv
        .oneshot_authenticated(
            &live_token,
            tenant_request(
                Method::POST,
                "/v1/principals",
                Some(json!({ "name": "before-suspension", "assignments": [{ "role": "viewer", "scope": "all" }] })),
            ),
        )
        .await
        .expect("principal route responds");
    assert_eq!(
        primed.status(),
        StatusCode::OK,
        "the live token works before suspension"
    );

    let suspend = async |status: &str| -> StatusCode {
        srv.oneshot(platform_request(
            Method::PUT,
            &format!("/platform/tenants/{tenant_id}/status"),
            &session,
            Some(json!({ "status": status })),
        ))
        .await
        .expect("tenant status route responds")
        .status()
    };
    assert_eq!(
        suspend("suspended").await,
        StatusCode::OK,
        "the tenant suspends"
    );

    assert!(
        tenant_token(&srv, &admin_credential).await.is_err(),
        "a suspended tenant mints no fresh token"
    );
    let resp = srv
        .oneshot_authenticated(
            &live_token,
            tenant_request(
                Method::POST,
                "/v1/principals",
                Some(json!({ "name": "during-suspension", "assignments": [{ "role": "viewer", "scope": "all" }] })),
            ),
        )
        .await
        .expect("principal route responds");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "a token minted before the suspension keeps its account's current authority \
         until its five-minute expiry"
    );

    // Replaying the same transition changes nothing and says so.
    assert_eq!(
        suspend("suspended").await,
        StatusCode::NOT_FOUND,
        "a tenant already suspended is not suspended again"
    );
    assert_eq!(
        suspend("deleted").await,
        StatusCode::BAD_REQUEST,
        "only active and suspended are settable"
    );

    assert_eq!(
        suspend("active").await,
        StatusCode::OK,
        "the tenant resumes"
    );

    // Restored, not rebuilt: the same principal, still holding its grants.
    let restored = tenant_token(&srv, &admin_credential)
        .await
        .expect("the resumed tenant admits its credential again");
    let resp = srv
        .oneshot_authenticated(
            &restored,
            tenant_request(
                Method::POST,
                "/v1/principals",
                Some(json!({ "name": "after-resume", "assignments": [{ "role": "viewer", "scope": "all" }] })),
            ),
        )
        .await
        .expect("principal route responds");
    let status = resp.status();
    let body = body_json(resp).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the administrator keeps the grants it had: {body}"
    );
    let recovered = body_json(
        srv.oneshot(platform_post(
            "/platform/tenants/admin/credentials",
            &session,
            json!({ "tenant_id": tenant_id }),
        ))
        .await
        .expect("recovery route responds"),
    )
    .await;
    assert_eq!(
        recovered["principal_id"].as_str().expect("principal id"),
        admin_principal,
        "the resumed tenant still has the administrative principal it started with"
    );
}

/// Every durable provisioning stage, failed in turn, leaves nothing usable and
/// converges on one tenant when retried.
///
/// The stages are the actual commit boundaries the workflow crosses: the role
/// seed, the administrative principal,
/// its grant, its credential, the tenant transaction's own commit, and the
/// promotion to active. Each one is forced to fail by a trigger scoped to that
/// stage's slug, so the server's real error path — not a test's idea of it —
/// is what marks the tenant failed. The property under test is the same at
/// every stage: no admitted tenant, no usable credential, and a retry that
/// lands on the original tenant id holding exactly one administrative
/// principal, one grant, and one live credential.
///
/// The commit-time case uses a deferred constraint trigger, which is the only
/// way to fail the tenant transaction at `COMMIT` rather than at a statement.
#[tokio::test]
async fn every_durable_provisioning_stage_fails_closed_and_retries_clean() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");

    // One guard for every stage, told which slug to fail through a trigger
    // argument: Postgres forbids a subquery in a trigger `WHEN` clause, so the
    // slug lookup has to happen in the function body. Scoping to one slug is
    // what keeps an installed trigger from disturbing another stage's tenant
    // or the deployment's own rows.
    sqlx::query(
        "CREATE OR REPLACE FUNCTION wyrd.injected_stage_failure() RETURNS trigger
         LANGUAGE plpgsql AS $$
         DECLARE
             target text := TG_ARGV[0];
             mine boolean;
         BEGIN
             IF TG_TABLE_NAME = 'tenants' THEN
                 mine := NEW.slug = target AND NEW.status = 'active';
             ELSE
                 mine := EXISTS (
                     SELECT 1 FROM platform.tenants
                      WHERE data_tenant_id = NEW.data_tenant_id AND slug = target);
             END IF;
             IF mine THEN
                 RAISE EXCEPTION 'injected stage failure';
             END IF;
             RETURN NEW;
         END $$",
    )
    .execute(&superuser)
    .await
    .expect("the injection function is created");

    let stages: [(&str, &str, &str); 6] = [
        (
            "role-seed",
            "CREATE TRIGGER fail_stage BEFORE INSERT ON wyrd.auth_roles
             FOR EACH ROW EXECUTE FUNCTION wyrd.injected_stage_failure('stage-role-seed')",
            "DROP TRIGGER fail_stage ON wyrd.auth_roles",
        ),
        (
            "principal",
            "CREATE TRIGGER fail_stage BEFORE INSERT ON wyrd.auth_service_accounts
             FOR EACH ROW EXECUTE FUNCTION wyrd.injected_stage_failure('stage-principal')",
            "DROP TRIGGER fail_stage ON wyrd.auth_service_accounts",
        ),
        (
            "grant",
            "CREATE TRIGGER fail_stage BEFORE INSERT ON wyrd.auth_role_bindings
             FOR EACH ROW EXECUTE FUNCTION wyrd.injected_stage_failure('stage-grant')",
            "DROP TRIGGER fail_stage ON wyrd.auth_role_bindings",
        ),
        (
            "credential",
            "CREATE TRIGGER fail_stage BEFORE INSERT ON wyrd.auth_api_keys
             FOR EACH ROW EXECUTE FUNCTION wyrd.injected_stage_failure('stage-credential')",
            "DROP TRIGGER fail_stage ON wyrd.auth_api_keys",
        ),
        (
            "tenant-commit",
            "CREATE CONSTRAINT TRIGGER fail_stage AFTER INSERT ON wyrd.auth_api_keys
             DEFERRABLE INITIALLY DEFERRED
             FOR EACH ROW EXECUTE FUNCTION wyrd.injected_stage_failure('stage-tenant-commit')",
            "DROP TRIGGER fail_stage ON wyrd.auth_api_keys",
        ),
        (
            "active-promotion",
            "CREATE TRIGGER fail_stage BEFORE UPDATE ON platform.tenants
             FOR EACH ROW EXECUTE FUNCTION wyrd.injected_stage_failure('stage-active-promotion')",
            "DROP TRIGGER fail_stage ON platform.tenants",
        ),
    ];

    let create = async |slug: &str| -> (StatusCode, Value) {
        let resp = srv
            .oneshot(platform_post(
                "/platform/tenants",
                &session,
                json!({ "slug": slug, "display_name": slug }),
            ))
            .await
            .expect("tenant route responds");
        let status = resp.status();
        (status, body_json(resp).await)
    };

    for (stage, install, remove) in stages {
        let slug = format!("stage-{stage}");
        sqlx::query(install)
            .execute(&superuser)
            .await
            .unwrap_or_else(|error| panic!("the {stage} injection installs: {error}"));

        let (status, body) = create(&slug).await;
        assert_ne!(
            status,
            StatusCode::OK,
            "a {stage} failure is not reported as a provisioned tenant: {body}"
        );
        assert!(
            !body.to_string().contains("injected stage failure"),
            "the served {stage} failure discloses nothing internal: {body}"
        );

        // Nothing admitted: a directory row that survived the failure is
        // visibly incomplete, and a stage that failed before the claim
        // committed left no row at all.
        let claimed: Option<(uuid::Uuid, String)> =
            sqlx::query_as("SELECT data_tenant_id, status FROM platform.tenants WHERE slug = $1")
                .bind(&slug)
                .fetch_optional(srv.operator_pool().pool())
                .await
                .expect("the directory reads back");
        if let Some((_, status)) = &claimed {
            assert_eq!(
                status, "failed",
                "an interrupted {stage} leaves the tenant visibly incomplete"
            );
        }

        // No usable credential: a plaintext was never returned, and anything
        // a partial attempt committed is revoked or rolled back.
        assert!(
            body.get("admin").is_none(),
            "a failed {stage} returns no credential: {body}"
        );

        sqlx::query(remove)
            .execute(&superuser)
            .await
            .unwrap_or_else(|error| panic!("the {stage} injection is removed: {error}"));

        let (status, retried) = create(&slug).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "the retry after a {stage} failure provisions: {retried}"
        );
        let tenant_id = retried["tenant"]["id"].as_str().expect("tenant id");
        if let Some((claimed_id, _)) = claimed {
            assert_eq!(
                tenant_id,
                claimed_id.to_string(),
                "the retry after a {stage} failure resumes the claimed tenant"
            );
        }

        // The administrative principal is counted apart from the internal
        // SYSTEM verification writer: provisioning creates exactly one of
        // each, and a resumed attempt must duplicate neither.
        let counts: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM wyrd.auth_service_accounts
                      WHERE data_tenant_id = $1 AND principal_kind <> 'system'),
                    (SELECT count(*) FROM wyrd.auth_role_bindings WHERE data_tenant_id = $1 AND service_account_id IS NOT NULL),
                    (SELECT count(*) FROM wyrd.auth_api_keys
                      WHERE data_tenant_id = $1 AND revoked_at IS NULL),
                    (SELECT count(*) FROM wyrd.auth_service_accounts
                      WHERE data_tenant_id = $1 AND principal_kind = 'system')",
        )
        .bind(uuid::Uuid::parse_str(tenant_id).expect("the tenant id is a uuid"))
        .fetch_one(&superuser)
        .await
        .expect("the tenant's durable rows are counted");
        assert_eq!(
            counts,
            (1, 1, 1, 1),
            "the retry after a {stage} failure leaves one administrative principal, grant, \
             live credential, and SYSTEM writer"
        );
        assert!(
            tenant_token(
                &srv,
                retried["admin"]["credential"].as_str().expect("credential")
            )
            .await
            .is_ok(),
            "the credential the retry after a {stage} failure returned is usable"
        );
    }

    sqlx::query("DROP FUNCTION wyrd.injected_stage_failure()")
        .execute(&superuser)
        .await
        .expect("the injection function is removed");
}

/// Provisioning survives a real stage failure, an abandoned attempt, and two
/// callers racing for the same slug.
///
/// The earlier retry journey marks a *successful* tenant failed, which proves
/// the adoption query but not the state machine. Here the failure is injected
/// where it actually happens — the tenant-scoped write that creates the
/// administrative principal — so the server's own error path is what marks the
/// tenant failed. The abandoned case is the one an operator hits most: a
/// cancelled request leaves a `provisioning` row nobody will finish, and
/// without adoption that slug is burned forever. Every case ends the same way:
/// one active tenant under the original id, one administrative principal, one
/// usable credential.
#[tokio::test]
async fn interrupted_and_racing_provisioning_converge_on_one_tenant() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");

    // Fail exactly this slug's post-claim stage, and nothing else running
    // alongside it: the guard names the tenant through the directory row the
    // claim just wrote.
    sqlx::query(
        "CREATE OR REPLACE FUNCTION wyrd.injected_stage_failure() RETURNS trigger LANGUAGE plpgsql AS $$
         BEGIN
             IF EXISTS (
                 SELECT 1 FROM platform.tenants
                  WHERE data_tenant_id = NEW.data_tenant_id AND slug = 'interrupted'
             ) THEN
                 RAISE EXCEPTION 'injected stage failure';
             END IF;
             RETURN NEW;
         END $$",
    )
    .execute(&superuser)
    .await
    .expect("the injection function is created");
    sqlx::query(
        "CREATE TRIGGER fail_stage BEFORE INSERT ON wyrd.auth_service_accounts
         FOR EACH ROW EXECUTE FUNCTION wyrd.injected_stage_failure()",
    )
    .execute(&superuser)
    .await
    .expect("the injection trigger is installed");

    let create = async |slug: &str| -> (StatusCode, Value) {
        let resp = srv
            .oneshot(platform_post(
                "/platform/tenants",
                &session,
                json!({ "slug": slug, "display_name": slug }),
            ))
            .await
            .expect("tenant route responds");
        let status = resp.status();
        (status, body_json(resp).await)
    };

    let (status, body) = create("interrupted").await;
    assert_ne!(
        status,
        StatusCode::OK,
        "a stage failure is not reported as a provisioned tenant: {body}"
    );
    assert!(
        !body.to_string().contains("injected stage failure"),
        "the served failure discloses nothing internal: {body}"
    );

    // The server marked it failed itself, which is what makes the slug
    // retryable rather than burned.
    let (claimed_id, claimed_status): (uuid::Uuid, String) = sqlx::query_as(
        "SELECT data_tenant_id, status FROM platform.tenants WHERE slug = 'interrupted'",
    )
    .fetch_one(srv.operator_pool().pool())
    .await
    .expect("the claim survives the failure");
    assert_eq!(
        claimed_status, "failed",
        "an interrupted attempt is left visibly incomplete"
    );

    sqlx::query("DROP TRIGGER fail_stage ON wyrd.auth_service_accounts")
        .execute(&superuser)
        .await
        .expect("the injection is removed");
    sqlx::query("DROP FUNCTION wyrd.injected_stage_failure()")
        .execute(&superuser)
        .await
        .expect("the injection function is removed");

    let (status, retried) = create("interrupted").await;
    assert_eq!(status, StatusCode::OK, "the retry provisions: {retried}");
    assert_eq!(
        retried["tenant"]["id"].as_str().expect("tenant id"),
        claimed_id.to_string(),
        "the retry resumes the original tenant rather than orphaning its rows"
    );
    assert!(
        tenant_token(
            &srv,
            retried["admin"]["credential"].as_str().expect("credential")
        )
        .await
        .is_ok(),
        "the resumed tenant's administrator authenticates"
    );

    // An abandoned attempt: the row a cancelled request leaves behind, aged
    // past the window in which it could still be in flight.
    let (status, abandoned) = create("cancelled").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the first attempt provisions: {abandoned}"
    );
    let abandoned_id = abandoned["tenant"]["id"]
        .as_str()
        .expect("tenant id")
        .to_owned();
    sqlx::query(
        "UPDATE platform.tenants
            SET status = 'provisioning', updated_at = now() - interval '1 hour'
          WHERE slug = 'cancelled'",
    )
    .execute(srv.operator_pool().pool())
    .await
    .expect("the attempt is aged");

    let (status, resumed) = create("cancelled").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "an abandoned attempt does not burn its slug: {resumed}"
    );
    assert_eq!(
        resumed["tenant"]["id"].as_str().expect("tenant id"),
        abandoned_id,
        "the resumed attempt keeps the original tenant id"
    );

    // A `provisioning` row that is still moving is a live attempt, not an
    // abandoned one, and is refused rather than stolen.
    sqlx::query("UPDATE platform.tenants SET status = 'provisioning', updated_at = now() WHERE slug = 'cancelled'")
        .execute(srv.operator_pool().pool())
        .await
        .expect("the attempt is made current");
    let (status, refused) = create("cancelled").await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "an attempt still in flight is not adopted: {refused}"
    );
    sqlx::query("UPDATE platform.tenants SET status = 'active' WHERE slug = 'cancelled'")
        .execute(srv.operator_pool().pool())
        .await
        .expect("the attempt is restored");

    // Two callers racing for one slug: one tenant, one winner.
    let (first, second) = tokio::join!(create("raced"), create("raced"));
    let statuses = [first.0, second.0];
    assert_eq!(
        statuses.iter().filter(|s| **s == StatusCode::OK).count(),
        1,
        "exactly one racing create provisions: {statuses:?}"
    );
    assert!(
        statuses.contains(&StatusCode::CONFLICT),
        "the loser is told the slug is taken: {statuses:?}"
    );
    let rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM platform.tenants WHERE slug = 'raced'")
            .fetch_one(srv.operator_pool().pool())
            .await
            .expect("the directory is counted");
    assert_eq!(rows, 1, "racing creates leave one directory row");

    let winner = if first.0 == StatusCode::OK {
        first.1
    } else {
        second.1
    };
    // Only the administrative principal is counted; every provisioned tenant
    // also carries the internal credentialless SYSTEM verification writer.
    let principals: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wyrd.auth_service_accounts
          WHERE data_tenant_id = $1::uuid AND principal_kind <> 'system'",
    )
    .bind(
        winner["tenant"]["id"]
            .as_str()
            .expect("tenant id")
            .to_owned(),
    )
    .fetch_one(&superuser)
    .await
    .expect("the tenant's principals are counted");
    assert_eq!(principals, 1, "the winner has one administrative principal");
    assert!(
        tenant_token(
            &srv,
            winner["admin"]["credential"].as_str().expect("credential")
        )
        .await
        .is_ok(),
        "the winner's credential is usable"
    );
}

/// Two operators initializing at once produce one root and one refusal.
///
/// Singleness is enforced by the unique principal name rather than a
/// read-then-write check, so this is the case that distinguishes the two: a
/// check-then-insert would let both attempts pass the check and leave a
/// deployment with two administrative roots, each believing it is the only one.
#[tokio::test(flavor = "multi_thread")]
async fn initialization_has_exactly_one_winner_under_concurrency() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");

    let attempts = futures_util::future::join_all(
        (0..8).map(|_| async { initialize_platform_root(&srv).await }),
    )
    .await;

    let established = attempts.iter().filter(|result| result.is_ok()).count();
    assert_eq!(established, 1, "exactly one attempt establishes the root");
    assert!(
        attempts
            .iter()
            .filter(|result| result.is_err())
            .all(|result| matches!(result, Err(InitError::AlreadyInitialized))),
        "every loser is told the deployment is already initialized"
    );

    let roots: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM platform.principals WHERE principal_kind = 'global_admin'",
    )
    .fetch_one(srv.operator_pool().pool())
    .await
    .expect("platform principals are counted");
    assert_eq!(roots, 1, "the deployment has one administrative root");
}

/// A terminal that cannot take the credential leaves the deployment
/// uninitialized, and the identical retry then succeeds once.
///
/// This is the cross-system ordering initialization exists to get right. The
/// root's name is unique, so a commit that outran a failed disclosure would
/// leave a durable root whose only credential nobody ever read and which no
/// later attempt could replace — the deployment would be permanently
/// unadministrable.
#[tokio::test]
async fn a_terminal_that_cannot_take_the_credential_leaves_nothing_behind() {
    /// A closed terminal: every write it is offered fails.
    struct ClosedTerminal;

    impl Write for ClosedTerminal {
        /// Refuses the bytes the way a closed pipe does.
        ///
        /// # Errors
        /// Always returns [`std::io::ErrorKind::BrokenPipe`].
        fn write(&mut self, _buf: &[u8]) -> IoResult<usize> {
            Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe))
        }

        /// Nothing was ever buffered, so there is nothing to flush.
        ///
        /// # Errors
        /// Never returns an error.
        fn flush(&mut self) -> IoResult<()> {
            Ok(())
        }
    }
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");

    let refused = wyrd_server::boot::init::initialize_platform_root(
        &srv.operator_pool(),
        &mut ClosedTerminal,
    )
    .await;
    assert!(
        matches!(refused, Err(InitError::Disclose(_))),
        "an undisclosable credential is refused as a disclosure failure, got: {refused:?}"
    );

    for (count_sql, what) in [
        ("SELECT count(*) FROM platform.principals", "root"),
        ("SELECT count(*) FROM platform.principal_grants", "grant"),
        ("SELECT count(*) FROM platform.credentials", "credential"),
    ] {
        let rows: i64 = sqlx::query_scalar(count_sql)
            .fetch_one(srv.operator_pool().pool())
            .await
            .expect("platform rows are counted");
        assert_eq!(
            rows, 0,
            "an undisclosed initialization leaves no {what} behind"
        );
    }

    // The identical invocation now succeeds, which is what "retryable" means.
    let root = initialize_platform_root(&srv)
        .await
        .expect("the retry initializes the deployment");
    let _session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;

    let roots: i64 = sqlx::query_scalar("SELECT count(*) FROM platform.principals")
        .fetch_one(srv.operator_pool().pool())
        .await
        .expect("platform principals are counted");
    assert_eq!(roots, 1, "the retry establishes exactly one root");
}

/// A failure at any of initialization's three writes leaves nothing behind, and
/// the identical retry then succeeds.
///
/// The three writes commit together for a reason: the principal's name is
/// unique, so a partial initialization would leave a root with no credential
/// that no later attempt could replace, and the deployment would be
/// permanently unadministrable. Each write is failed in turn to prove that.
/// The captured diagnostics are checked in the same pass, because the credential
/// is generated before any of these writes and a failure is exactly when an
/// error path is tempted to print what it was holding.
#[tokio::test]
async fn initialization_retries_cleanly_after_a_failure_at_each_write() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");

    sqlx::query(
        "CREATE OR REPLACE FUNCTION platform.injected_init_failure() RETURNS trigger
         LANGUAGE plpgsql AS $$
         BEGIN RAISE EXCEPTION 'injected write failure'; END $$",
    )
    .execute(&superuser)
    .await
    .expect("the injection function is created");

    let logs = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));

    // Written out per table rather than interpolated: a dynamic SQL string
    // would have to be audited for injection, and there are exactly three
    // writes to fail.
    let stages = [
        (
            "principals",
            "CREATE TRIGGER injected_failure BEFORE INSERT ON platform.principals
             FOR EACH ROW EXECUTE FUNCTION platform.injected_init_failure()",
            "DROP TRIGGER injected_failure ON platform.principals",
        ),
        (
            "principal_grants",
            "CREATE TRIGGER injected_failure BEFORE INSERT ON platform.principal_grants
             FOR EACH ROW EXECUTE FUNCTION platform.injected_init_failure()",
            "DROP TRIGGER injected_failure ON platform.principal_grants",
        ),
        (
            "credentials",
            "CREATE TRIGGER injected_failure BEFORE INSERT ON platform.credentials
             FOR EACH ROW EXECUTE FUNCTION platform.injected_init_failure()",
            "DROP TRIGGER injected_failure ON platform.credentials",
        ),
    ];
    for (table, install, remove) in stages {
        sqlx::query(install)
            .execute(&superuser)
            .await
            .expect("the injection trigger is installed");

        let sink = std::sync::Arc::clone(&logs);
        let failed = {
            let subscriber = tracing_subscriber::fmt()
                .with_writer(move || CaptureWriter(std::sync::Arc::clone(&sink)))
                .with_max_level(tracing::Level::TRACE)
                .finish();
            let _guard = tracing::subscriber::set_default(subscriber);
            initialize_platform_root(&srv).await
        };
        let error = failed.expect_err("the injected failure is surfaced");
        assert!(
            matches!(error, InitError::Store(_)),
            "a write failure is a store failure, not a false already-initialized: {error}"
        );

        // Nothing committed: a partial root is the one state that cannot be
        // recovered from, so its absence is the point of the transaction.
        let rows: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM platform.principals WHERE principal_kind = 'global_admin'",
        )
        .fetch_one(srv.operator_pool().pool())
        .await
        .expect("platform principals are counted");
        assert_eq!(
            rows, 0,
            "a failure at {table} leaves the deployment uninitialized"
        );

        sqlx::query(remove)
            .execute(&superuser)
            .await
            .expect("the injection is removed");
    }

    // The identical invocation now succeeds, which is what "retryable" means.
    let credential = initialize_platform_root(&srv)
        .await
        .expect("the retry initializes the deployment");
    let plaintext = secrecy::ExposeSecret::expose_secret(&credential);
    // Exchanging it is the proof it is a working root, not merely a row.
    let _session = platform_session(&srv, plaintext).await;

    let captured = String::from_utf8(logs.lock().expect("the capture is readable").clone())
        .expect("captured output is UTF-8");
    assert!(
        !captured.contains(plaintext),
        "no credential material reaches the diagnostics"
    );
    assert!(
        !captured.contains("wyrd_global_"),
        "not even a credential prefix reaches the diagnostics"
    );

    // What is stored is a verifier, not the secret.
    let stored: Vec<String> = sqlx::query_scalar("SELECT secret_hash FROM platform.credentials")
        .fetch_all(srv.operator_pool().pool())
        .await
        .expect("platform credentials are read");
    assert!(
        stored.iter().all(|hash| !hash.contains(plaintext)),
        "the stored verifier does not carry the credential"
    );
}

/// Collects a subscriber's output into a shared buffer for inspection.
struct CaptureWriter(Arc<Mutex<Vec<u8>>>);

impl Write for CaptureWriter {
    /// Appends to the shared buffer, ignoring a poisoned lock as unreachable
    /// here: nothing else writes to it while a test holds the subscriber.
    ///
    /// # Errors
    /// Never fails: a poisoned lock is dropped silently and the write is
    /// reported as fully accepted, because losing capture output must not fail
    /// the subscriber a test is observing through.
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        if let Ok(mut sink) = self.0.lock() {
            sink.extend_from_slice(buf);
        }
        Ok(buf.len())
    }

    /// Nothing is buffered beyond the shared vector, so flushing is a no-op.
    ///
    /// # Errors
    /// Never fails.
    fn flush(&mut self) -> IoResult<()> {
        Ok(())
    }
}

/// An uninitialized deployment serves its tenants and refuses its platform plane.
///
/// Initialization is an operator subcommand, not a server-start side effect, so
/// a deployment can run indefinitely without one. That must be a stable state
/// rather than a broken one: ordinary tenant traffic works, the platform plane
/// refuses every caller because there is no identity that could administer it,
/// and the refusal is the same on the tenth attempt as on the first.
#[tokio::test]
async fn an_uninitialized_deployment_serves_tenants_and_refuses_the_platform_plane() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");

    // Tenant traffic is unaffected: tenants do not depend on a platform root.
    let tenant = srv
        .bootstrap_service("uninitialized-tenant", &["admin"])
        .await
        .expect("tenant principal seeds");
    let token = srv
        .exchange_api_key(tenant.api_key().expect("machine key"))
        .await
        .expect("tenant token exchanges");
    let resp = srv
        .oneshot_authenticated(
            &token,
            tenant_request(
                Method::POST,
                "/v1/principals",
                Some(json!({ "name": "unaffected", "assignments": [{ "role": "viewer", "scope": "all" }] })),
            ),
        )
        .await
        .expect("principal route responds");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "an uninitialized deployment still serves its tenants"
    );

    // The platform plane admits nobody, the same way every time.
    for attempt in 0..3 {
        let resp = srv
            .oneshot(token_exchange(
                "/auth/platform/token",
                "wyrd_global_not_a_real_credential",
            ))
            .await
            .expect("platform token route responds");
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "attempt {attempt} is refused"
        );
        let body = body_json(resp).await;
        assert!(
            !body.to_string().to_lowercase().contains("uninitialized"),
            "the refusal does not advertise that the deployment is uninitialized: {body}"
        );
    }

    // And nothing about that state created an identity by accident.
    let roots: i64 = sqlx::query_scalar("SELECT count(*) FROM platform.principals")
        .fetch_one(srv.operator_pool().pool())
        .await
        .expect("platform principals are counted");
    assert_eq!(roots, 0, "serving traffic establishes no platform identity");
}

/// Install a trigger that makes every write to `table` fail.
///
/// This is the only honest way to prove the coupling: the allowance is written
/// by the server, and the effect is written by the server, and what has to be
/// shown is that a failure of the *second* also discards the *first*. A
/// statement-level trigger is the smallest thing that makes the second fail
/// without touching the code under test.
///
/// # Panics
///
/// Panics when the failure function or its trigger cannot be created, which
/// means the fixture cannot prove the coupling it exists for.
async fn fail_writes_to(pool: &PgPool, label: &str, table: &str, event: &str) {
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "CREATE OR REPLACE FUNCTION platform.test_fail_{label}()
           RETURNS trigger LANGUAGE plpgsql AS $$
           BEGIN
             RAISE EXCEPTION 'injected {label} failure';
           END;
           $$;"
    )))
    .execute(pool)
    .await
    .expect("failure function installs");
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "CREATE TRIGGER test_fail_{label}
           BEFORE {event} ON {table}
           FOR EACH ROW EXECUTE FUNCTION platform.test_fail_{label}()"
    )))
    .execute(pool)
    .await
    .expect("failure trigger installs");
}

/// Remove an injected write failure.
///
/// # Panics
///
/// Panics when the trigger cannot be dropped, which would leave later
/// assertions in the same test failing for the wrong reason.
async fn stop_failing_writes(pool: &PgPool, label: &str, table: &str) {
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DROP TRIGGER test_fail_{label} ON {table}"
    )))
    .execute(pool)
    .await
    .expect("failure trigger drops");
}

/// Count `tenant`'s retained authorization decisions for one operation and
/// outcome.
///
/// Settles `srv`'s Scribe outbox first, so every decision staged so far is
/// counted.
///
/// # Panics
///
/// Panics when audit retention does not settle or the retained read fails.
async fn tenant_decisions(
    srv: &WyrdTestServer,
    tenant: DataTenantId,
    operation: &str,
    outcome: &str,
) -> usize {
    srv.await_audit_retained()
        .await
        .expect("audit retention settles");
    srv.retained_audit_records(
        tenant,
        "operation",
        &format!("operation = '{operation}' AND outcome = '{outcome}'"),
    )
    .await
    .expect("decision count reads")
    .len()
}

/// Count the retained audit batches Scribe has committed for the reserved
/// system owner.
///
/// Platform-plane decisions are staged under [`DataTenantId::SYSTEM_OWNER`],
/// which Oracle refuses to read, so their retention is observed through the
/// batch fence Scribe commits per retained batch. Settles `srv`'s Scribe outbox
/// first, so a single decision settled on its own adds exactly one batch.
///
/// # Panics
///
/// Panics when audit retention does not settle or the fence cannot be read.
async fn system_audit_batches(srv: &WyrdTestServer) -> i64 {
    srv.await_audit_retained()
        .await
        .expect("audit retention settles");
    let mut conn = srv
        .tenant_conn_for(DataTenantId::SYSTEM_OWNER)
        .await
        .expect("system owner connection opens");
    let batches = sqlx::query_scalar(
        "SELECT count(*) FROM vala.scribe_batch_commits \
          WHERE logical_table_fqn = 'bifrost.system.audit_log'",
    )
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("system audit batches read");
    conn.commit().await.expect("batch read commits");
    batches
}

/// Every system-owner audit decision Scribe has acknowledged, one map of
/// column name to text value per row.
///
/// Platform-plane decisions are retained under [`DataTenantId::SYSTEM_OWNER`],
/// which Oracle refuses to read on every production surface, so the journey
/// reads them from the durable Scribe WAL instead: Scribe acknowledges only
/// after the WAL fsync, so every settled decision is already replayable.
/// Settles `srv`'s Scribe outbox first, and must run before a Scribe drain
/// seals the generation, because replay suppresses sealed records.
///
/// # Panics
///
/// Panics when audit retention does not settle, or the WAL cannot be replayed
/// or decoded.
async fn system_audit_decisions(
    srv: &WyrdTestServer,
) -> Vec<std::collections::HashMap<String, Option<String>>> {
    use arrow::array::Array as _;

    srv.await_audit_retained()
        .await
        .expect("audit retention settles");
    let replayed = vala_bifrost_redux::scribe::replay::replay_wal_directory(
        srv.scribe_wal_root_for_test()
            .expect("the in-process server owns a WAL root"),
    )
    .expect("acknowledged WAL replays");
    let mut decisions = Vec::new();
    for stream in replayed.values().filter(|stream| {
        stream.seal_key.tenant == DataTenantId::SYSTEM_OWNER
            && vala_bifrost_redux::tables::AuditLogTable::admits_system_owner(
                &stream.seal_key.table,
            )
    }) {
        for record in &stream.data_records {
            let reader =
                arrow::ipc::reader::StreamReader::try_new(std::io::Cursor::new(record), None)
                    .expect("a WAL record is an Arrow IPC stream");
            for batch in reader {
                let batch = batch.expect("a WAL record decodes");
                for row in 0..batch.num_rows() {
                    decisions.push(
                        batch
                            .schema()
                            .fields()
                            .iter()
                            .zip(batch.columns())
                            .map(|(field, column)| {
                                let value = (!column.is_null(row)).then(|| {
                                    arrow::util::display::array_value_to_string(column, row)
                                        .expect("a retained value displays")
                                });
                                (field.name().clone(), value)
                            })
                            .collect(),
                    );
                }
            }
        }
    }
    decisions
}

/// Resolve the data tenant provisioned under `slug`.
///
/// # Panics
///
/// Panics when no live tenant carries the slug.
async fn tenant_named(srv: &WyrdTestServer, slug: &str) -> DataTenantId {
    let id: uuid::Uuid = sqlx::query_scalar(
        "SELECT data_tenant_id FROM platform.tenants WHERE slug = $1 AND deleted_at IS NULL",
    )
    .bind(slug)
    .fetch_one(srv.operator_pool().pool())
    .await
    .expect("tenant resolves by slug");
    DataTenantId::new(id).expect("a provisioned tenant id is valid")
}

/// An authorized request that changes nothing still records the decision.
///
/// Every route here evaluates the caller's permission, allows it, stages the
/// decision on the Scribe outbox, and then discovers there is nothing to do:
/// the credential is unknown or belongs to someone else, the revoke is a
/// replay, no connection is configured, the principal does not exist,
/// suspending it would strand the deployment, or the issuer or binding is
/// already gone. Each is a stable administrative answer, not a failure, so the
/// decision has to survive it. Rolling back with the response would let an
/// authorized caller probe the deployment's administrative state and leave no
/// durable evidence that permission was ever evaluated — which is exactly what
/// an attacker mapping a plane would want.
///
/// # Panics
///
/// Panics when a no-effect response records no decision, records more than
/// one, or mutates anything.
#[tokio::test]
async fn an_authorized_request_that_changes_nothing_still_records_the_decision() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let secret = secrecy::ExposeSecret::expose_secret(&root).to_owned();
    let session = platform_session(&srv, &secret).await;
    // Platform rows are read as the superuser across the platform plane and the
    // provisioned tenant.
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");

    // Each case: the request, the status it must answer with, and what it is.
    // Platform decisions are retained under the reserved system owner, which
    // Oracle refuses to read, so each case is counted as the one retained batch
    // its settled decision adds to the running total.
    let mut expected = system_audit_batches(&srv).await;
    let unknown = uuid::Uuid::now_v7();
    let cases: Vec<(Request<Body>, StatusCode, &str)> = vec![
        (
            platform_request(
                Method::DELETE,
                &format!("/platform/admins/{unknown}/credentials/{unknown}"),
                &session,
                None,
            ),
            StatusCode::NOT_FOUND,
            "revoking a credential that does not exist",
        ),
        (
            platform_request(Method::DELETE, "/platform/oidc/connection", &session, None),
            StatusCode::NOT_FOUND,
            "removing a connection that was never configured",
        ),
        (
            platform_request(
                Method::PUT,
                &format!("/platform/admins/{unknown}/status"),
                &session,
                Some(json!({ "status": "suspended" })),
            ),
            StatusCode::NOT_FOUND,
            "changing the status of a principal that does not exist",
        ),
        (
            platform_request(
                Method::PUT,
                &format!("/platform/tenants/{unknown}/status"),
                &session,
                Some(json!({ "status": "suspended" })),
            ),
            StatusCode::NOT_FOUND,
            "suspending a tenant that does not exist",
        ),
        (
            platform_request(
                Method::PUT,
                &format!("/platform/tenants/{}/status", DataTenantId::SYSTEM_OWNER),
                &session,
                Some(json!({ "status": "suspended" })),
            ),
            StatusCode::NOT_FOUND,
            "suspending the platform's own wyrd-system tenant",
        ),
        (
            platform_post(
                "/platform/admins",
                &session,
                json!({ "name": "unreachable", "match_claim": "nobody@example.com" }),
            ),
            StatusCode::BAD_REQUEST,
            "registering an administrator with no platform connection",
        ),
    ];
    for (request, status, what) in cases {
        let resp = srv.oneshot(request).await.expect("route responds");
        assert_eq!(resp.status(), status, "{what} answers {status}");
        expected += 1;
        assert_eq!(
            system_audit_batches(&srv).await,
            expected,
            "{what} records exactly one allowed decision"
        );
    }

    // Suspending the deployment's only active principal is refused as a
    // conflict, and that refusal is still an evaluated permission.
    let root_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM platform.principals WHERE status = 'active' LIMIT 1")
            .fetch_one(&superuser)
            .await
            .expect("the deployment root reads");
    let resp = srv
        .oneshot(platform_request(
            Method::PUT,
            &format!("/platform/admins/{root_id}/status"),
            &session,
            Some(json!({ "status": "suspended" })),
        ))
        .await
        .expect("status route responds");
    assert_eq!(
        resp.status(),
        StatusCode::CONFLICT,
        "the last active principal cannot be suspended"
    );
    expected += 1;
    assert_eq!(
        system_audit_batches(&srv).await,
        expected,
        "a last-admin conflict records exactly one allowed decision"
    );
    let still_active: String =
        sqlx::query_scalar("SELECT status FROM platform.principals WHERE id = $1")
            .bind(root_id)
            .fetch_one(&superuser)
            .await
            .expect("root status reads");
    assert_eq!(still_active, "active", "the refusal changed nothing");
    let registered: i64 =
        sqlx::query_scalar("SELECT count(*) FROM platform.principals WHERE name = 'unreachable'")
            .fetch_one(&superuser)
            .await
            .expect("platform principals read");
    assert_eq!(registered, 0, "a refused registration writes no principal");

    // A malformed status is refused before any permission is evaluated, so it
    // must not record a decision at all.
    let resp = srv
        .oneshot(platform_request(
            Method::PUT,
            &format!("/platform/admins/{root_id}/status"),
            &session,
            Some(json!({ "status": "retired" })),
        ))
        .await
        .expect("status route responds");
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        system_audit_batches(&srv).await,
        expected,
        "a request refused on syntax opens no decision to record"
    );

    // The tenant plane has the same shape: the delete finds nothing, and the
    // decision staged before it still commits.
    let created = body_json(
        srv.oneshot(platform_post(
            "/platform/tenants",
            &session,
            json!({ "slug": "no-effect", "display_name": "no-effect" }),
        ))
        .await
        .expect("tenant route responds"),
    )
    .await;
    let admin = tenant_token(
        &srv,
        created["admin"]["credential"]
            .as_str()
            .expect("admin credential"),
    )
    .await
    .expect("the tenant administrator authenticates");

    // Resuming a tenant that is already active is the replayed transition: it
    // changes nothing and is refused, but the permission was evaluated.
    let tenant_id = created["tenant"]["id"].as_str().expect("tenant id");
    let tenant: DataTenantId = tenant_id.parse().expect("tenant id is a data tenant");
    let before = system_audit_batches(&srv).await;
    let resp = srv
        .oneshot(platform_request(
            Method::PUT,
            &format!("/platform/tenants/{tenant_id}/status"),
            &session,
            Some(json!({ "status": "active" })),
        ))
        .await
        .expect("tenant status route responds");
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "a replayed resume answers 404"
    );
    assert_eq!(
        system_audit_batches(&srv).await,
        before + 1,
        "a replayed resume records exactly one allowed decision"
    );
    let status: String =
        sqlx::query_scalar("SELECT status FROM platform.tenants WHERE data_tenant_id = $1")
            .bind(tenant_id.parse::<uuid::Uuid>().expect("tenant uuid"))
            .fetch_one(&superuser)
            .await
            .expect("tenant status reads");
    assert_eq!(status, "active", "the replayed resume changed nothing");

    // Tenant principal administration: an unknown principal and an unknown
    // role are stable refusals of an evaluated permission.
    for (operation, request, status, what) in [
        (
            "auth.credential.issue",
            tenant_request(
                Method::POST,
                &format!("/v1/principals/{unknown}/credentials"),
                None,
            ),
            StatusCode::NOT_FOUND,
            "issuing for a principal that does not exist",
        ),
        (
            "auth.credential.list",
            tenant_request(
                Method::GET,
                &format!("/v1/principals/{unknown}/credentials"),
                None,
            ),
            StatusCode::NOT_FOUND,
            "listing for a principal that does not exist",
        ),
        (
            "auth.principal.create",
            tenant_request(
                Method::POST,
                "/v1/principals",
                Some(
                    json!({ "name": "roleless", "assignments": [{ "role": "absent_role", "scope": "all" }] }),
                ),
            ),
            StatusCode::BAD_REQUEST,
            "creating a principal with a role that does not exist",
        ),
    ] {
        let before = tenant_decisions(&srv, tenant, operation, "allowed").await;
        let resp = srv
            .oneshot_authenticated(&admin, request)
            .await
            .expect("principal route responds");
        assert_eq!(resp.status(), status, "{what} answers {status}");
        assert_eq!(
            tenant_decisions(&srv, tenant, operation, "allowed").await,
            before + 1,
            "{what} records exactly one allowed decision"
        );
    }
    let effects: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM wyrd.auth_service_accounts WHERE name = 'roleless'),
                (SELECT count(*) FROM wyrd.auth_api_keys WHERE principal_id = $1)",
    )
    .bind(unknown)
    .fetch_one(&superuser)
    .await
    .expect("tenant principal tables read");
    assert_eq!(
        effects,
        (0, 0),
        "no refused request wrote a principal or credential"
    );

    for (operation, uri, what) in [
        (
            "admin.trusted_issuer.delete",
            "/v1/admin/trusted-issuers?issuer=https://absent.example",
            "deleting an issuer that is not configured",
        ),
        (
            "admin.workload_binding.delete",
            "/v1/admin/workload-bindings?issuer=https://absent.example&subject=nobody",
            "deleting a binding that is not configured",
        ),
    ] {
        let before = tenant_decisions(&srv, tenant, operation, "allowed").await;
        let resp = srv
            .oneshot_authenticated(&admin, tenant_request(Method::DELETE, uri, None))
            .await
            .expect("admin route responds");
        assert_eq!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "{what} answers 404 Not Found"
        );
        assert_eq!(
            tenant_decisions(&srv, tenant, operation, "allowed").await,
            before + 1,
            "{what} records exactly one allowed decision"
        );
    }
}

/// A failed platform mutation records the decision it evaluated and no effect.
///
/// The audit row records an authorization decision, not the operation's
/// outcome: the decision is staged on the Scribe outbox as soon as permission
/// is evaluated, before the mutation runs and outside its transaction. Four
/// mutation classes cover the plane's write surface — the identity connection,
/// a credential, a principal's status, and a tenant's admission — and each is
/// driven through the real route with its own table's writes failing. Each
/// refuses, records exactly one allowance, and leaves no effect.
///
/// A refused caller is the control: it earns no allowance.
///
/// # Panics
/// Panics when a failed mutation reports success, records other than one
/// allowance, or leaves an effect behind.
#[tokio::test]
async fn a_failed_platform_mutation_records_its_decision_and_no_effect() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let secret = secrecy::ExposeSecret::expose_secret(&root).to_owned();
    let session = platform_session(&srv, &secret).await;
    let provider = wyrd_testing::DiscoveryFixture::start().await;

    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");

    // A tenant and a registered administrator to aim the status and suspension
    // classes at, written before any failure is injected.
    let tenant = body_json(
        srv.oneshot(platform_post(
            "/platform/tenants",
            &session,
            json!({ "slug": "atomic-tenant", "display_name": "Atomic" }),
        ))
        .await
        .expect("tenant route responds"),
    )
    .await;
    let tenant_id = tenant["tenant"]["id"]
        .as_str()
        .expect("tenant id")
        .to_owned();
    let tenant_credential = tenant["admin"]["credential"]
        .as_str()
        .expect("admin credential")
        .to_owned();

    srv.oneshot(platform_request(
        Method::PUT,
        "/platform/oidc/connection",
        &session,
        Some(json!({
            "issuer_url": provider.issuer(),
            "client_id": "wyrd-platform",
            "client_auth": { "method": "public" },
        })),
    ))
    .await
    .expect("configure route responds");

    let registered = body_json(
        srv.oneshot(platform_request(
            Method::POST,
            "/platform/admins",
            &session,
            Some(json!({ "name": "atomic-admin", "match_claim": "atomic@example.com" })),
        ))
        .await
        .expect("register route responds"),
    )
    .await;
    let admin_id = registered["principal_id"].as_str().expect("id").to_owned();

    let root_principal: String =
        sqlx::query_scalar("SELECT id::text FROM platform.principals WHERE name = $1")
            .bind(wyrd_server::boot::init::PLATFORM_ROOT_NAME)
            .fetch_one(&superuser)
            .await
            .expect("root principal reads");

    // The assertions below are about what each *failed* attempt adds.
    let mut expected = system_audit_batches(&srv).await;

    let classes: Vec<(&str, &str, &str, Request<Body>)> = vec![
        (
            "identity",
            "platform.oidc_connection",
            "INSERT OR UPDATE",
            platform_request(
                Method::PUT,
                "/platform/oidc/connection",
                &session,
                Some(json!({
                    "issuer_url": provider.issuer(),
                    "client_id": "wyrd-platform-two",
                    "client_auth": { "method": "public" },
                })),
            ),
        ),
        (
            "credential",
            "platform.credentials",
            "INSERT",
            platform_request(
                Method::POST,
                &format!("/platform/admins/{root_principal}/credentials"),
                &session,
                Some(json!({})),
            ),
        ),
        (
            "status",
            "platform.principals",
            "UPDATE",
            platform_request(
                Method::PUT,
                &format!("/platform/admins/{admin_id}/status"),
                &session,
                Some(json!({ "status": "suspended" })),
            ),
        ),
        (
            "suspension",
            "platform.tenants",
            "UPDATE",
            platform_request(
                Method::PUT,
                &format!("/platform/tenants/{tenant_id}/status"),
                &session,
                Some(json!({ "status": "suspended" })),
            ),
        ),
    ];

    for (label, table, event, request) in classes {
        fail_writes_to(&superuser, label, table, event).await;
        let resp = srv.oneshot(request).await.expect("route responds");
        let status = resp.status();
        stop_failing_writes(&superuser, label, table).await;

        assert!(
            status.is_server_error(),
            "the {label} mutation must refuse rather than report success: {status}"
        );
        expected += 1;
        assert_eq!(
            system_audit_batches(&srv).await,
            expected,
            "the {label} failure records exactly its one allowance"
        );
    }

    // The effects never landed either.
    let client_id: String =
        sqlx::query_scalar("SELECT client_id FROM platform.oidc_connection WHERE singleton")
            .fetch_one(&superuser)
            .await
            .expect("connection reads");
    assert_eq!(client_id, "wyrd-platform", "the identity write rolled back");

    let credentials: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM platform.credentials WHERE principal_id = $1::uuid",
    )
    .bind(&root_principal)
    .fetch_one(&superuser)
    .await
    .expect("credential count reads");
    assert_eq!(credentials, 1, "no credential outlived its failed decision");

    let admin_status: String =
        sqlx::query_scalar("SELECT status FROM platform.principals WHERE id = $1::uuid")
            .bind(&admin_id)
            .fetch_one(&superuser)
            .await
            .expect("status reads");
    assert_eq!(admin_status, "active", "the status write rolled back");

    let tenant_status: String =
        sqlx::query_scalar("SELECT status FROM platform.tenants WHERE data_tenant_id = $1::uuid")
            .bind(&tenant_id)
            .fetch_one(&superuser)
            .await
            .expect("tenant status reads");
    assert_eq!(tenant_status, "active", "the suspension rolled back");

    // The control: a refusal writes no allowance either. Durability of the
    // denial row itself is proved where a denial is actually reachable —
    // `wyrd_auth::platform_authz::pg_tests::a_denial_is_recorded_and_refuses`
    // — because every credential this plane issues carries the fixed platform
    // grant, so the only refusal an HTTP caller can provoke is the session
    // extractor's, which evaluates no permission and stages nothing.
    let allowances = system_audit_batches(&srv).await;
    let tenant_admin = tenant_token(&srv, &tenant_credential)
        .await
        .expect("the tenant administrator authenticates");
    let refused = srv
        .oneshot(platform_request(
            Method::POST,
            "/platform/admins",
            &tenant_admin,
            Some(json!({ "name": "nope", "match_claim": "nope@example.com" })),
        ))
        .await
        .expect("register route responds");
    assert_eq!(
        refused.status(),
        StatusCode::UNAUTHORIZED,
        "tenant authority never reaches the platform plane"
    );
    assert_eq!(
        system_audit_batches(&srv).await,
        allowances,
        "a refused caller earns no allowance"
    );
}

/// A provisioned tenant administrator renews by re-exchanging its credential.
///
/// A tenant administrator authenticates with a durable API key, which makes its
/// grant a machine grant: the exchange advertises no refresh token and stores no
/// refresh row, and renewal is spending the same durable credential again. Only
/// a human OIDC session receives and rotates a refresh token, which
/// `identity_e2e::human_oidc_login_journey` drives end to end.
#[tokio::test]
async fn a_tenant_administrator_renews_by_re_exchanging_its_credential() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;
    let created = body_json(
        srv.oneshot(platform_post(
            "/platform/tenants",
            &session,
            json!({ "slug": "refreshing", "display_name": "Refreshing" }),
        ))
        .await
        .expect("tenant route responds"),
    )
    .await;
    let credential = created["admin"]["credential"]
        .as_str()
        .expect("admin credential");

    let exchanged = body_json(
        srv.oneshot(token_exchange("/auth/token", credential))
            .await
            .expect("token route responds"),
    )
    .await;
    assert!(
        exchanged.get("refresh_token").is_none_or(Value::is_null),
        "a machine grant advertises no refresh token: {exchanged}"
    );
    let access = exchanged["access_token"]
        .as_str()
        .expect("the exchange returns an access token")
        .to_owned();

    // Renewal is the second exchange of the same durable credential, and the
    // token it returns must administer the tenant exactly as the first did.
    let renewed = body_json(
        srv.oneshot(token_exchange("/auth/token", credential))
            .await
            .expect("token route responds"),
    )
    .await;
    let successor = renewed["access_token"]
        .as_str()
        .expect("re-exchange returns an access token")
        .to_owned();
    assert_ne!(successor, access, "re-exchange mints a new access token");

    let protected = srv
        .oneshot_authenticated(
            &successor,
            tenant_request(Method::GET, "/v1/admin/trusted-issuers", None),
        )
        .await
        .expect("admin route responds");
    let protected_status = protected.status();
    let protected_body = body_json(protected).await;
    assert_eq!(
        protected_status,
        StatusCode::OK,
        "the re-exchanged token administers the tenant: {protected_body}"
    );
}

/// A trailing-slash issuer configures, preregisters, and still pins on first
/// login.
///
/// `https://idp.example/` and `https://idp.example` are the same issuer, and
/// `IssuerUrl` says so by trimming the slash. Configuration used to store the
/// request text verbatim while login reparsed the row and searched with the
/// normalized form, so a provider whose URL an operator typed with a trailing
/// slash preregistered an administrator that first login could never find. The
/// pin is a one-way transition, so that failure is permanent for that claim.
///
/// # Panics
///
/// Panics when configuration, registration, or the first-login pin does not
/// behave as an operator following the documented path would require.
#[tokio::test]
async fn a_trailing_slash_platform_issuer_completes_first_login() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;
    let provider = wyrd_testing::DiscoveryFixture::start().await;
    let canonical = provider.issuer();
    let typed = format!("{canonical}/");

    let resp = srv
        .oneshot(platform_request(
            Method::PUT,
            "/platform/oidc/connection",
            &session,
            Some(json!({
                "issuer_url": typed,
                "client_id": "wyrd-platform",
                "client_auth": { "method": "public" },
            })),
        ))
        .await
        .expect("configure route responds");
    let status = resp.status();
    let view = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "connection configures: {view}");
    assert_eq!(
        view["issuer_url"], canonical,
        "the connection reports the canonical issuer, not the request text"
    );

    let resp = srv
        .oneshot(platform_request(
            Method::POST,
            "/platform/admins",
            &session,
            Some(json!({ "name": "ops-lead", "match_claim": "ops@example.com" })),
        ))
        .await
        .expect("register route responds");
    let status = resp.status();
    let registered = body_json(resp).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "administrator registers: {registered}"
    );

    // What login actually searches with: the stored row, reparsed by the same
    // resolver the served login path uses.
    let pool = srv.operator_pool();
    let row = wyrd_sql::queries::platform::identity::platform_oidc_connection(&pool)
        .await
        .expect("connection reads")
        .expect("a connection was configured");
    let connection = wyrd_auth::pg_resolvers::platform_connection_from_row(row, None)
        .await
        .expect("the stored connection decodes");
    let searched = connection.verification.issuer.as_str();
    assert_eq!(
        searched, canonical,
        "login searches with the canonical issuer"
    );

    // First login: the real one-time pin, against the registration that was
    // just written. `None` here is the defect this test exists for.
    let mut conn = pool
        .begin_platform_audited()
        .await
        .expect("the grant transaction opens");
    let pinned = wyrd_sql::queries::platform::identity::pin_platform_identity(
        &mut conn,
        searched,
        "ops@example.com",
        "provider-subject-1",
    )
    .await
    .expect("the pin runs");
    conn.commit().await.expect("the pin commits");
    assert_eq!(
        pinned.map(|id| id.to_string()).as_deref(),
        registered["principal_id"].as_str(),
        "first login pins the preregistered administrator"
    );
}

/// Read the canonical built-ins registered for `tenant` without creating any.
///
/// Lists the tenant's catalog registrations — a read that provisions nothing —
/// and keeps the `namespace.name` of each that the canonical inventory owns, so
/// the result is comparable with [`every_builtin`].
///
/// # Panics
/// Panics when the catalog listing fails.
async fn registered_builtins(
    srv: &WyrdTestServer,
    tenant: wyrd_spec::DataTenantId,
) -> std::collections::BTreeSet<String> {
    srv.bifrost_catalog()
        .list_tables(tenant)
        .await
        .expect("the tenant's catalog lists")
        .into_iter()
        .map(|entry| {
            format!(
                "{}.{}",
                entry.namespace.trim_start_matches("bifrost."),
                entry.name
            )
        })
        .filter(|fqn| every_builtin().contains(fqn))
        .collect()
}

/// The `namespace.name` of every table in the canonical built-in inventory.
fn every_builtin() -> std::collections::BTreeSet<String> {
    vala_bifrost_redux::tables::builtin_tables()
        .iter()
        .map(|definition| format!("{}.{}", definition.namespace, definition.name))
        .collect()
}

/// The distinct creator pairs `query` reads in `tenant`, as `(sub, act)`.
///
/// `query` selects `sub_principal_id, act_principal_id` from a tenant-scoped
/// table, so forced RLS confines it to `tenant`.
///
/// # Panics
/// Panics when the tenant transaction or the read fails.
async fn creator_pairs(
    srv: &WyrdTestServer,
    tenant: DataTenantId,
    query: &'static str,
) -> std::collections::BTreeSet<(uuid::Uuid, uuid::Uuid)> {
    let mut conn = srv.tenant_conn_for(tenant).await.expect("tenant opens");
    let rows: Vec<(uuid::Uuid, uuid::Uuid)> = sqlx::query_as(query)
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("creator pairs read");
    rows.into_iter().collect()
}

/// Reads the creator pair of every catalog table.
const TABLE_CREATORS: &str =
    "SELECT DISTINCT sub_principal_id, act_principal_id FROM vala.bifrost_tables";

/// Reads the creator pair of the tenant's SYSTEM principal.
const SYSTEM_CREATOR: &str = "SELECT sub_principal_id, act_principal_id \
     FROM wyrd.auth_service_accounts WHERE principal_kind = 'system'";

/// A tenant has every canonical built-in from the moment it is usable, and a
/// tenant that predates the inventory receives them at the next startup.
///
/// A provisioned tenant is read immediately, before anything writes or
/// describes a table, so only eager provisioning can have created them. The
/// pre-existing tenant is a directory row with no tables at all — what an
/// upgrade finds — and gains the whole inventory only through the restart's
/// reconciliation, which must also leave the provisioned tenant unchanged.
///
/// Creation keeps its verified creator (AC-019): the provisioning operator's
/// platform pair stamps the provisioned tenant's SYSTEM principal and tables,
/// while callerless startup reconciliation stamps the no-verified-caller
/// sentinel on the existing tenant and the reserved audit owner, and leaves the
/// provisioned tenant's first pair in place.
///
/// # Panics
/// Panics when the server fails to start or restart, the platform root or a
/// route fails to respond, a response lacks the tenant id or listed root
/// principal or it does not parse, the pre-existing tenant row does not
/// insert, a tenant read fails, or any built-in inventory or creator-pair
/// assertion does not hold.
#[tokio::test]
async fn new_and_existing_tenants_receive_every_builtin() {
    let srv = WyrdTestServer::builder()
        .start_bound()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;
    let created = body_json(
        srv.oneshot(platform_post(
            "/platform/tenants",
            &session,
            json!({ "slug": "eager", "display_name": "Eager" }),
        ))
        .await
        .expect("tenant route responds"),
    )
    .await;
    let provisioned: wyrd_spec::DataTenantId = created["tenant"]["id"]
        .as_str()
        .expect("tenant id")
        .parse()
        .expect("tenant id parses");
    assert_eq!(
        registered_builtins(&srv, provisioned).await,
        every_builtin(),
        "a provisioned tenant has every built-in before any write or describe"
    );
    let admins = body_json(
        srv.oneshot(platform_request(
            Method::GET,
            "/platform/admins",
            &session,
            None,
        ))
        .await
        .expect("listing responds"),
    )
    .await;
    let root_id: uuid::Uuid = admins["principals"][0]["principal_id"]
        .as_str()
        .expect("the deployment root is listed")
        .parse()
        .expect("principal id parses");
    let operator = std::collections::BTreeSet::from([(root_id, root_id)]);
    let sentinel = std::collections::BTreeSet::from([(
        wyrd_spec::auth::PLATFORM_AUDIT_PRINCIPAL.as_uuid(),
        wyrd_spec::auth::PLATFORM_AUDIT_PRINCIPAL.as_uuid(),
    )]);
    assert_eq!(
        creator_pairs(&srv, provisioned, SYSTEM_CREATOR).await,
        operator,
        "provisioning records its operator as SYSTEM's creator"
    );
    assert_eq!(
        creator_pairs(&srv, provisioned, TABLE_CREATORS).await,
        operator,
        "provisioning records its operator as every built-in's creator"
    );

    let existing = wyrd_spec::DataTenantId::new_v7();
    sqlx::query(
        "INSERT INTO platform.tenants (data_tenant_id, slug, display_name, status)
         VALUES ($1, 'existing', 'Existing', 'active')",
    )
    .bind(existing.as_uuid())
    .execute(srv.operator_pool().pool())
    .await
    .expect("a pre-existing tenant row inserts");
    assert!(
        registered_builtins(&srv, existing).await.is_empty(),
        "a tenant that predates the inventory starts with no built-ins"
    );

    let srv = Box::pin(srv.restart_bound(WyrdTestServer::builder()))
        .await
        .expect("server restarts");
    assert_eq!(
        registered_builtins(&srv, existing).await,
        every_builtin(),
        "startup backfills every built-in for an existing tenant"
    );
    assert_eq!(
        registered_builtins(&srv, provisioned).await,
        every_builtin(),
        "startup reconciliation is idempotent for an already provisioned tenant"
    );
    assert_eq!(
        creator_pairs(&srv, existing, TABLE_CREATORS).await,
        sentinel,
        "callerless reconciliation records the no-verified-caller sentinel"
    );
    assert_eq!(
        creator_pairs(&srv, DataTenantId::SYSTEM_OWNER, TABLE_CREATORS).await,
        sentinel,
        "the reserved audit owner's tables have no verified caller"
    );
    assert_eq!(
        creator_pairs(&srv, provisioned, TABLE_CREATORS).await,
        operator,
        "reconciliation keeps each built-in's first creator"
    );
}

/// The benchmark Scribe control exists only in a `bench` build.
///
/// A shipped server routes neither path: even the platform administrator gets
/// exactly what an unrouted path answers, so nothing outside a benchmark can
/// stage or publish on request. A `bench` build answers the administrator with
/// what it moved.
#[tokio::test]
async fn the_benchmark_control_exists_only_in_a_bench_build() {
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;
    let unrouted = srv
        .oneshot(platform_post(
            "/platform/benchmark/unrouted",
            &session,
            json!({}),
        ))
        .await
        .expect("the router responds")
        .status();
    for (path, field) in [
        ("/platform/benchmark/scribe/stage", "rows_staged"),
        ("/platform/benchmark/scribe/publish", "claims_published"),
    ] {
        let resp = srv
            .oneshot(platform_post(path, &session, json!({})))
            .await
            .expect("the router responds");
        if cfg!(feature = "bench") {
            assert_eq!(resp.status(), StatusCode::OK, "{path}");
            assert!(
                body_json(resp).await[field].is_u64(),
                "{path} reports {field}"
            );
        } else {
            assert_eq!(resp.status(), unrouted, "{path} is unrouted");
        }
    }
}

/// Host every shared social sign-in request names, so callbacks derive from
/// one public origin.
const SOCIAL_HOST: &str = "wyrd.test";

/// Build a request at [`SOCIAL_HOST`] carrying `token`, when given.
fn social_request(
    method: Method,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, SOCIAL_HOST);
    if let Some(token) = token {
        builder = builder.header(WYRD_ACCESS_TOKEN_HEADER, format!("Bearer {token}"));
    }
    match body {
        Some(body) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("serializes")))
            .expect("request builds"),
        None => builder.body(Body::empty()).expect("request builds"),
    }
}

/// Send `request` and read its status and JSON body (`Null` when not JSON).
async fn social_call(srv: &WyrdTestServer, request: Request<Body>) -> (StatusCode, Value) {
    let resp = srv.oneshot(request).await.expect("route responds");
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), 1 << 20)
        .await
        .expect("read body");
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// An operator configures Google beside a tenant's company SSO, tests the
/// exact stored revision through a real provider sign-in and only then
/// enables it:
///   1. with no social configuration the tenant page offers company SSO and
///      password, no provider and no signup; every provider is unconfigured;
///   2. storing the client returns the exact registration callback and an
///      untested revision, never the secret, and the secret is sealed at rest;
///   3. an untested revision cannot be enabled, and a test of a stale
///      revision is refused;
///   4. the test sign-in at the provider marks exactly that revision tested,
///      and a replayed test return is refused;
///   5. once enabled, another replica reads the same tested revision and the
///      tenant page offers Google beside company SSO;
///   6. a tenant administrator cannot write the shared configuration but
///      still manages their own connection, a platform session cannot change
///      tenant settings, and provider setup created no tenant grant.
#[tokio::test]
async fn an_operator_configures_and_tests_google_beside_company_sso() {
    let google = wiremock::MockServer::start().await;
    let srv = WyrdTestServer::builder()
        .with_social_endpoints_for_test(wyrd_server::config::SocialConfig {
            google_issuer: Some(google.uri()),
            ..Default::default()
        })
        .start_in_process()
        .await
        .expect("server starts");
    mount_platform_provider(
        &google,
        platform_key_set("google-key"),
        wiremock::ResponseTemplate::new(500),
    )
    .await;
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;
    let tenant = srv.data_tenant_id();
    let slug = srv.pg_fixture().tenant_slug().to_owned();
    let mut conn = srv
        .tenant_conn_for(tenant)
        .await
        .expect("tenant conn opens");
    wyrd_dev_fixtures::pg::seed_active_human_connection(&mut conn)
        .await
        .expect("company connection seeds");
    conn.commit().await.expect("seed commits");
    let admin = srv
        .bootstrap_service_in_tenant(tenant, "social-tenant-admin", &["admin"])
        .await
        .expect("tenant admin bootstraps");
    let admin = srv
        .exchange_api_key(admin.api_key().expect("admin has an api key"))
        .await
        .expect("admin key exchanges");
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    let grants_before: i64 = sqlx::query_scalar("SELECT count(*) FROM wyrd.auth_role_bindings")
        .fetch_one(&superuser)
        .await
        .expect("grants count");
    let methods = format!("/auth/browser/methods?tenant={slug}");

    // 1. Nothing social is offered before configuration.
    let (status, offered) =
        social_call(&srv, social_request(Method::GET, &methods, None, None)).await;
    assert_eq!(status, StatusCode::OK, "methods list: {offered}");
    assert_eq!(
        offered,
        json!({ "password": true, "company_sso": true, "providers": [], "signup": false, "local_signup": false, "email_discovery": false, "password_reset": false, "email_verification_required": true, "signup_providers": [] }),
        "password starts enabled, signup disabled, and no social button appears"
    );
    let (status, listed) = social_call(
        &srv,
        social_request(
            Method::GET,
            "/platform/social/providers",
            Some(&session),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "providers list: {listed}");
    assert_eq!(listed["providers"][0]["provider"], "google");
    assert_eq!(
        listed["providers"][0]["state"],
        Value::Null,
        "Google is unconfigured"
    );
    assert_eq!(listed["providers"][1]["provider"], "github");
    let (status, settings) = social_call(
        &srv,
        social_request(
            Method::GET,
            "/platform/login-settings",
            Some(&session),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        settings,
        json!({ "password_enabled": true, "signup_enabled": false, "signup_workspace_cap": 3,
            "signup_challenge": null })
    );

    // 2. Storing the client discloses the callback and never the secret.
    let secret = "google-client-secret-value";
    let (status, stored) = social_call(
        &srv,
        social_request(
            Method::PUT,
            "/platform/social/providers/google",
            Some(&session),
            Some(json!({ "client_id": "wyrd-google", "client_secret": secret })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Google configures: {stored}");
    assert_eq!(
        stored["callback_url"],
        "http://wyrd.test/auth/social/google/callback"
    );
    assert_eq!(stored["revision"], 1);
    assert_eq!(stored["state"], "untested");
    assert!(
        !stored.to_string().contains(secret),
        "no secret in a response: {stored}"
    );
    let sealed: Vec<u8> = sqlx::query_scalar(
        "SELECT client_secret_enc FROM platform.social_providers WHERE provider = 'google'",
    )
    .fetch_one(&superuser)
    .await
    .expect("sealed secret reads");
    assert!(
        !sealed
            .windows(secret.len())
            .any(|window| window == secret.as_bytes()),
        "the client secret is sealed at rest"
    );

    // 3. Untested and stale revisions are refused.
    let enable = |revision: u64| {
        social_request(
            Method::PUT,
            "/platform/social/providers/google/enabled",
            Some(&session),
            Some(json!({ "enabled": true, "expected_revision": revision })),
        )
    };
    let (status, refused) = social_call(&srv, enable(1)).await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "untested Google cannot be enabled: {refused}"
    );
    let (status, _) = social_call(
        &srv,
        social_request(
            Method::POST,
            "/platform/social/providers/google/test",
            Some(&session),
            Some(json!({ "expected_revision": 2 })),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "a revision that is not current is not tested"
    );

    // 4. A real provider sign-in tests exactly revision 1.
    let (status, begun) = social_call(
        &srv,
        social_request(
            Method::POST,
            "/platform/social/providers/google/test",
            Some(&session),
            Some(json!({ "expected_revision": 1 })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "test begins: {begun}");
    let authorization = url::Url::parse(begun["authorization_url"].as_str().expect("url"))
        .expect("authorization url parses");
    let query: std::collections::HashMap<_, _> = authorization.query_pairs().into_owned().collect();
    assert_eq!(query["client_id"], "wyrd-google");
    assert_eq!(
        query["redirect_uri"],
        "http://wyrd.test/auth/social/google/callback"
    );
    assert_eq!(
        query["scope"], "openid profile email",
        "only the agreed Google scopes"
    );
    mount_platform_provider(
        &google,
        platform_key_set("google-key"),
        platform_token_reply(&google.uri(), &query["nonce"], "wyrd-google", "google-key"),
    )
    .await;
    let callback = format!(
        "/auth/social/google/callback?code=provider-code&state={}&iss={}",
        query["state"],
        url::form_urlencoded::byte_serialize(google.uri().as_bytes()).collect::<String>()
    );
    let (status, _) = social_call(&srv, social_request(Method::GET, &callback, None, None)).await;
    assert_eq!(status, StatusCode::OK, "the provider test passes");
    let (status, _) = social_call(&srv, social_request(Method::GET, &callback, None, None)).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a replayed test return is refused"
    );

    // 5. The tested revision enables and every replica serves it.
    let (status, enabled) = social_call(&srv, enable(1)).await;
    assert_eq!(status, StatusCode::OK, "tested Google enables: {enabled}");
    assert_eq!(enabled["state"], "enabled");
    let replica = srv
        .start_replica(WyrdTestServer::builder())
        .await
        .expect("replica starts");
    let (status, listed) = social_call(
        &replica,
        social_request(
            Method::GET,
            "/platform/social/providers",
            Some(&session),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "replica lists providers: {listed}");
    assert_eq!(listed["providers"][0]["revision"], 1);
    assert_eq!(listed["providers"][0]["state"], "enabled");
    assert!(
        !listed.to_string().contains(secret),
        "no secret in a read: {listed}"
    );
    let (_, offered) =
        social_call(&replica, social_request(Method::GET, &methods, None, None)).await;
    assert_eq!(
        offered,
        json!({ "password": true, "company_sso": true, "providers": ["google"], "signup": false, "local_signup": false, "email_discovery": false, "password_reset": false, "email_verification_required": true, "signup_providers": [] }),
        "Google is offered beside company SSO"
    );

    // 6. Each control plane stays on its own side.
    let (status, _) = social_call(
        &srv,
        social_request(
            Method::PUT,
            "/platform/social/providers/google",
            Some(&admin),
            Some(json!({ "client_id": "attacker", "client_secret": "attacker" })),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "a tenant admin cannot write shared config"
    );
    let (status, connections) = social_call(
        &srv,
        social_request(
            Method::GET,
            "/v1/identity/oidc/connections",
            Some(&admin),
            None,
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the tenant admin manages their connection: {connections}"
    );
    assert!(
        connections.to_string().contains("Active"),
        "the company connection is untouched: {connections}"
    );
    let (status, _) = social_call(
        &srv,
        social_request(
            Method::PUT,
            "/v1/identity/oidc/candidate",
            Some(&session),
            Some(json!({ "issuer_url": "https://idp.example", "client_id": "x" })),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "a platform session cannot change tenant settings"
    );
    let grants_after: i64 = sqlx::query_scalar("SELECT count(*) FROM wyrd.auth_role_bindings")
        .fetch_one(&superuser)
        .await
        .expect("grants count");
    assert_eq!(
        grants_after, grants_before,
        "provider setup creates no tenant grant"
    );
}

/// Configure, test and enable the deployment's Google client `wyrd-google`
/// against the mock `google` through the platform API, leaving `google`
/// with a working key set.
///
/// # Panics
/// Panics when any platform step is refused.
async fn enable_google(srv: &WyrdTestServer, session: &str, google: &wiremock::MockServer) {
    let (status, stored) = social_call(
        srv,
        social_request(
            Method::PUT,
            "/platform/social/providers/google",
            Some(session),
            Some(json!({ "client_id": "wyrd-google", "client_secret": "google-secret" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Google configures: {stored}");
    let revision = stored["revision"].as_u64().expect("revision");
    let (status, begun) = social_call(
        srv,
        social_request(
            Method::POST,
            "/platform/social/providers/google/test",
            Some(session),
            Some(json!({ "expected_revision": revision })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "test begins: {begun}");
    let query = authorization_query(begun["authorization_url"].as_str().expect("url"));
    mount_platform_provider(
        google,
        platform_key_set("google-key"),
        platform_token_reply(&google.uri(), &query["nonce"], "wyrd-google", "google-key"),
    )
    .await;
    let (status, _) = social_call(
        srv,
        social_request(
            Method::GET,
            &social_callback(&query["state"], &google.uri()),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the provider test passes");
    set_google_enabled(srv, session, revision, true).await;
}

/// Enable or disable Google revision `revision` through the platform API.
///
/// # Panics
/// Panics when the change is refused.
async fn set_google_enabled(srv: &WyrdTestServer, session: &str, revision: u64, enabled: bool) {
    let (status, changed) = social_call(
        srv,
        social_request(
            Method::PUT,
            "/platform/social/providers/google/enabled",
            Some(session),
            Some(json!({ "enabled": enabled, "expected_revision": revision })),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "Google enablement changes: {changed}"
    );
}

/// The query parameters of a provider authorization URL.
///
/// # Panics
/// Panics when `url` does not parse.
fn authorization_query(url: &str) -> std::collections::HashMap<String, String> {
    url::Url::parse(url)
        .expect("authorization url parses")
        .query_pairs()
        .into_owned()
        .collect()
}

/// The Google social callback for `state`, naming `issuer` as RFC 9207 `iss`.
fn social_callback(state: &str, issuer: &str) -> String {
    format!(
        "/auth/social/google/callback?code=provider-code&state={state}&iss={}",
        url::form_urlencoded::byte_serialize(issuer.as_bytes()).collect::<String>()
    )
}

/// One Google sign-in begun from a tenant sign-in page.
struct BegunSocialLogin {
    /// The provider state and nonce from the authorization URL.
    query: std::collections::HashMap<String, String>,
    /// The `wyrd_login` cookie pair the beginning browser holds.
    cookie: String,
    /// That cookie's `Set-Cookie` attributes.
    set_cookie: String,
}

/// Begin a Google sign-in for tenant `slug` the way its sign-in page does.
///
/// # Panics
/// Panics when sign-in does not begin or sets no `wyrd_login` cookie.
async fn begin_google_login(srv: &WyrdTestServer, slug: &str) -> BegunSocialLogin {
    begin_google(srv, json!({ "tenant": slug, "provider": "google" })).await
}

/// Begin a Google sign-in with the login request `body`.
///
/// # Panics
/// Panics when sign-in does not begin or sets no `wyrd_login` cookie.
async fn begin_google(srv: &WyrdTestServer, body: Value) -> BegunSocialLogin {
    begin_social(srv, body).await
}

/// Begin a social sign-in with the login request `body`.
///
/// # Panics
/// Panics when sign-in does not begin or sets no `wyrd_login` cookie.
async fn begin_social(srv: &WyrdTestServer, body: Value) -> BegunSocialLogin {
    begin_social_at(srv, "/auth/browser/login", body).await
}

/// Begin a social round-trip at `uri` with the request `body`.
///
/// # Panics
/// Panics when it does not begin or sets no `wyrd_login` cookie.
async fn begin_social_at(srv: &WyrdTestServer, uri: &str, body: Value) -> BegunSocialLogin {
    let request = Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(header::HOST, SOCIAL_HOST)
        .header(header::ORIGIN, format!("http://{SOCIAL_HOST}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&body).expect("serializes")))
        .expect("request builds");
    let resp = srv.oneshot(request).await.expect("login route responds");
    let status = resp.status();
    let set_cookie = resp
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with("wyrd_login="))
        .map(str::to_owned);
    let body = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "Google sign-in begins: {body}");
    let set_cookie = set_cookie.expect("the login-transaction cookie is set");
    BegunSocialLogin {
        query: authorization_query(body["authorization_url"].as_str().expect("url")),
        cookie: set_cookie
            .split(';')
            .next()
            .expect("cookie pair")
            .to_owned(),
        set_cookie,
    }
}

/// Return to the Google callback for `begun` from a browser holding
/// `cookie`, naming `issuer`; returns the status and `Location`.
///
/// # Panics
/// Panics when the route does not respond.
async fn finish_google_login(
    srv: &WyrdTestServer,
    begun: &BegunSocialLogin,
    cookie: Option<&str>,
    issuer: &str,
) -> (StatusCode, Option<String>) {
    let mut builder = Request::builder()
        .method(Method::GET)
        .uri(social_callback(&begun.query["state"], issuer))
        .header(header::HOST, SOCIAL_HOST);
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    let resp = srv
        .oneshot(builder.body(Body::empty()).expect("request builds"))
        .await
        .expect("callback responds");
    let location = resp
        .headers()
        .get(header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    (resp.status(), location)
}

/// Counts of every local record a sign-in could create: Users, linked
/// identities, browser sessions and role bindings.
///
/// # Panics
/// Panics when a count query fails.
async fn local_records(pool: &PgPool) -> (i64, i64, i64, i64) {
    sqlx::query_as(
        "SELECT (SELECT count(*) FROM wyrd.auth_users), \
                (SELECT count(*) FROM wyrd.auth_user_identities), \
                (SELECT count(*) FROM wyrd.auth_browser_sessions), \
                (SELECT count(*) FROM wyrd.auth_role_bindings)",
    )
    .fetch_one(pool)
    .await
    .expect("counts run")
}

/// Google proves only the sign-in purpose bound before redirecting and
/// never admits a stranger:
///   1. a tenant page begins Google with only the agreed scopes and a
///      login-transaction cookie scoped to the social callbacks;
///   2. a valid Google identity no tenant User has linked is refused with
///      account guidance and creates no User, identity, session or grant,
///      and a replay of the return is refused;
///   3. an email matching an existing User still selects nothing;
///   4. a return reaching another browser, naming another issuer, or
///      carrying a wrong nonce, audience or signature is refused;
///   5. a Google transaction cannot complete a company sign-in;
///   6. disabling Google between start and return refuses completion on
///      every replica, and so does replacing its client;
///   7. a provider outage refuses; no provider token is retained.
#[tokio::test]
async fn google_proves_a_bound_login_without_admitting_a_stranger() {
    let google = wiremock::MockServer::start().await;
    let srv = WyrdTestServer::builder()
        .with_social_endpoints_for_test(wyrd_server::config::SocialConfig {
            google_issuer: Some(google.uri()),
            ..Default::default()
        })
        .start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;
    let tenant = srv.data_tenant_id();
    let slug = srv.pg_fixture().tenant_slug().to_owned();
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    mount_platform_provider(
        &google,
        platform_key_set("google-key"),
        wiremock::ResponseTemplate::new(500),
    )
    .await;
    enable_google(&srv, &session, &google).await;
    let issuer = google.uri();
    let answer = |nonce: &str, audience: &str, kid: &str| {
        platform_token_reply(&issuer, nonce, audience, kid)
    };
    let before = local_records(&superuser).await;
    let no_account = format!("/t/{slug}/login?login=no_account");
    let failed = format!("/t/{slug}/login?login=failed");

    // 1. Only the agreed scopes; the cookie reaches only the social callbacks.
    let begun = begin_google_login(&srv, &slug).await;
    assert_eq!(begun.query["client_id"], "wyrd-google");
    assert_eq!(
        begun.query["redirect_uri"],
        "http://wyrd.test/auth/social/google/callback"
    );
    assert_eq!(begun.query["scope"], "openid profile email");
    assert!(
        !begun.query.contains_key("access_type"),
        "no offline Google access is requested"
    );
    assert!(
        begun.set_cookie.contains("Path=/auth/social"),
        "the transaction cookie is scoped to the social callbacks: {}",
        begun.set_cookie
    );

    // 2. A stranger is refused with guidance and creates nothing.
    mount_platform_provider(
        &google,
        platform_key_set("google-key"),
        answer(&begun.query["nonce"], "wyrd-google", "google-key"),
    )
    .await;
    let (status, location) = finish_google_login(&srv, &begun, Some(&begun.cookie), &issuer).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        location.as_deref(),
        Some(no_account.as_str()),
        "account guidance"
    );
    assert_eq!(
        local_records(&superuser).await,
        before,
        "a stranger creates nothing"
    );
    let (status, _) = finish_google_login(&srv, &begun, Some(&begun.cookie), &issuer).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a replayed return is refused"
    );

    // 3. A matching email selects no account.
    sqlx::query(
        "INSERT INTO wyrd.auth_users (id, data_tenant_id, email, auth_type, status) \
         VALUES ($1, $2, 'ops@example.com', 'oidc', 'active')",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(tenant.as_uuid())
    .execute(&superuser)
    .await
    .expect("same-email User seeds");
    let before = local_records(&superuser).await;
    let begun = begin_google_login(&srv, &slug).await;
    mount_platform_provider(
        &google,
        platform_key_set("google-key"),
        answer(&begun.query["nonce"], "wyrd-google", "google-key"),
    )
    .await;
    let (_, location) = finish_google_login(&srv, &begun, Some(&begun.cookie), &issuer).await;
    assert_eq!(
        location.as_deref(),
        Some(no_account.as_str()),
        "email never selects a User"
    );
    assert_eq!(local_records(&superuser).await, before);

    // 4. Wrong browser, issuer, nonce, audience and signature are refused.
    let begun = begin_google_login(&srv, &slug).await;
    let (_, location) = finish_google_login(&srv, &begun, None, &issuer).await;
    assert_eq!(
        location.as_deref(),
        Some(failed.as_str()),
        "no transaction cookie"
    );
    let begun = begin_google_login(&srv, &slug).await;
    let other = begin_google_login(&srv, &slug).await;
    let (_, location) = finish_google_login(&srv, &begun, Some(&other.cookie), &issuer).await;
    assert_eq!(
        location.as_deref(),
        Some(failed.as_str()),
        "another browser's cookie"
    );
    let (_, location) =
        finish_google_login(&srv, &other, Some(&other.cookie), "https://evil.example").await;
    assert_eq!(location.as_deref(), Some(failed.as_str()), "another issuer");
    for (nonce, audience, kid, case) in [
        (None, "wyrd-google", "google-key", "a wrong nonce"),
        (Some(()), "someone-else", "google-key", "a wrong audience"),
        (
            Some(()),
            "wyrd-google",
            "unknown-key",
            "an unknown signing key",
        ),
    ] {
        let begun = begin_google_login(&srv, &slug).await;
        let nonce = nonce.map_or("other-nonce", |()| begun.query["nonce"].as_str());
        mount_platform_provider(
            &google,
            platform_key_set("google-key"),
            answer(nonce, audience, kid),
        )
        .await;
        let (_, location) = finish_google_login(&srv, &begun, Some(&begun.cookie), &issuer).await;
        assert_eq!(
            location.as_deref(),
            Some(failed.as_str()),
            "{case} is refused"
        );
    }

    // 5. A Google transaction cannot complete a company sign-in.
    let begun = begin_google_login(&srv, &slug).await;
    let company = Request::builder()
        .method(Method::GET)
        .uri(format!(
            "/auth/callback?code=provider-code&state={}",
            begun.query["state"]
        ))
        .header(header::HOST, SOCIAL_HOST)
        .header(header::COOKIE, &begun.cookie)
        .body(Body::empty())
        .expect("request builds");
    let (status, _) = social_call(&srv, company).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "purpose substitution is refused"
    );

    // 6. Disabled or replaced between start and return, Google refuses everywhere.
    let replica = srv
        .start_replica(WyrdTestServer::builder().with_social_endpoints_for_test(
            wyrd_server::config::SocialConfig {
                google_issuer: Some(google.uri()),
                ..Default::default()
            },
        ))
        .await
        .expect("replica starts");
    let begun = begin_google_login(&srv, &slug).await;
    mount_platform_provider(
        &google,
        platform_key_set("google-key"),
        answer(&begun.query["nonce"], "wyrd-google", "google-key"),
    )
    .await;
    set_google_enabled(&srv, &session, 1, false).await;
    let (_, location) = finish_google_login(&replica, &begun, Some(&begun.cookie), &issuer).await;
    assert_eq!(
        location.as_deref(),
        Some(failed.as_str()),
        "disabled Google refuses on a replica"
    );
    set_google_enabled(&srv, &session, 1, true).await;
    let begun = begin_google_login(&srv, &slug).await;
    mount_platform_provider(
        &google,
        platform_key_set("google-key"),
        answer(&begun.query["nonce"], "wyrd-google", "google-key"),
    )
    .await;
    let (status, _) = social_call(
        &srv,
        social_request(
            Method::PUT,
            "/platform/social/providers/google",
            Some(&session),
            Some(json!({ "client_id": "wyrd-google", "client_secret": "rotated" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the client is replaced");
    let (_, location) = finish_google_login(&srv, &begun, Some(&begun.cookie), &issuer).await;
    assert_eq!(
        location.as_deref(),
        Some(failed.as_str()),
        "a replaced client refuses"
    );

    // 7. Outage refuses, and no provider token is kept anywhere.
    enable_google(&srv, &session, &google).await;
    let begun = begin_google_login(&srv, &slug).await;
    mount_platform_outage(&google, wiremock::ResponseTemplate::new(503)).await;
    let (status, location) = finish_google_login(&srv, &begun, Some(&begun.cookie), &issuer).await;
    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "an outage refuses as retryable: {location:?}"
    );
    assert_eq!(
        local_records(&superuser).await,
        before,
        "no refusal created anything"
    );
    let retained: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM wyrd.auth_login_state s WHERE s::text LIKE '%provider-access%') \
              + (SELECT count(*) FROM platform.social_login_state s WHERE s::text LIKE '%provider-access%')",
    )
    .fetch_one(&superuser)
    .await
    .expect("retention check runs");
    assert_eq!(retained, 0, "no provider token is retained");
}

/// The repository-managed Mailpit relay: its SMTP `host:port` and HTTP API.
///
/// # Panics
/// Panics when `scripts/postgres/with-test-postgres.sh` did not start it.
fn mailpit() -> (String, u16, String) {
    let smtp = env::var("WYRD_TEST_MAILPIT_SMTP")
        .expect("WYRD_TEST_MAILPIT_SMTP is set by scripts/postgres/with-test-postgres.sh");
    let api = env::var("WYRD_TEST_MAILPIT_API").expect("WYRD_TEST_MAILPIT_API is set");
    let (host, port) = smtp.rsplit_once(':').expect("host:port");
    (host.to_owned(), port.parse().expect("port"), api)
}

/// Every message Mailpit holds for `recipient`, newest first, as
/// `(to, from, subject, text)`.
async fn mail_to(api: &str, recipient: &str) -> Vec<(Vec<String>, String, String, String)> {
    let http = reqwest::Client::new();
    let found: Value = http
        .get(format!("{api}/api/v1/search"))
        .query(&[("query", format!("to:\"{recipient}\""))])
        .send()
        .await
        .expect("mailpit search")
        .json()
        .await
        .expect("search json");
    let mut messages = Vec::new();
    for summary in found["messages"].as_array().expect("messages") {
        let id = summary["ID"].as_str().expect("message id");
        let message: Value = http
            .get(format!("{api}/api/v1/message/{id}"))
            .send()
            .await
            .expect("mailpit message")
            .json()
            .await
            .expect("message json");
        let to = message["To"]
            .as_array()
            .expect("to")
            .iter()
            .map(|to| to["Address"].as_str().expect("address").to_owned())
            .collect();
        messages.push((
            to,
            message["From"]["Address"]
                .as_str()
                .expect("from")
                .to_owned(),
            message["Subject"].as_str().expect("subject").to_owned(),
            message["Text"].as_str().expect("text").to_owned(),
        ));
    }
    messages
}

/// An operator points the deployment at an SMTP relay, and invitations are
/// emailed after they commit:
///   1. a tenant administrator can neither read nor change the relay, and
///      reads never return its password, which is sealed at rest;
///   2. a test send reaches the relay;
///   3. each new invitation is emailed separately to its own recipient, from
///      the configured sender, carrying exactly the returned link, and its
///      outcome reads `accepted`: relay acceptance, not inbox delivery;
///   4. a resend emails the replacement link;
///   5. with the relay down, the link is still returned beside a `failed`
///      outcome, the invitation stays pending, no User appears, and a resend
///      after recovery is emailed;
///   6. a stalled relay holds no tenant transaction: the invitation is
///      committed and listed while the send is still waiting.
///
/// # Panics
/// Panics when any step's response or the relay's mailbox differs.
#[tokio::test(flavor = "multi_thread")]
async fn an_operator_relays_invitations_through_smtp() {
    let (host, port, api) = mailpit();
    let srv = WyrdTestServer::builder()
        .start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;
    let tenant = srv.data_tenant_id();
    let slug = srv.pg_fixture().tenant_slug().to_owned();
    let admin = srv
        .bootstrap_service_in_tenant(tenant, "mail-tenant-admin", &["admin"])
        .await
        .expect("tenant admin bootstraps");
    let admin = srv
        .exchange_api_key(admin.api_key().expect("admin has an api key"))
        .await
        .expect("admin key exchanges");
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    let relay = |host: &str, port: u16| {
        json!({
            "host": host,
            "port": port,
            "tls": "none",
            "username": "relay-user",
            "password": "relay-password-plaintext",
            "sender": "invites@wyrd.test",
        })
    };
    let addressed = |name: &str| format!("{name}-{slug}@example.test");
    let invite = |recipients: Vec<String>| {
        social_request(
            Method::POST,
            "/v1/principals/users/invitations",
            Some(&admin),
            Some(json!({
                "recipients": recipients,
                "assignments": [{ "role": "viewer", "scope": "all" }],
            })),
        )
    };

    // 1. The relay is operator-only, and reads never return its password.
    let (status, unconfigured) = social_call(
        &srv,
        social_request(Method::GET, "/platform/mail", Some(&session), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "mail read: {unconfigured}");
    assert_eq!(unconfigured, json!({ "relay": null }), "no relay yet");
    for (method, body) in [(Method::GET, None), (Method::PUT, Some(relay(&host, port)))] {
        let (status, refused) = social_call(
            &srv,
            social_request(method, "/platform/mail", Some(&admin), body),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "a tenant administrator cannot reach the relay: {refused}"
        );
    }
    let (status, configured) = social_call(
        &srv,
        social_request(
            Method::PUT,
            "/platform/mail",
            Some(&session),
            Some(relay(&host, port)),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "mail configure: {configured}");
    assert_eq!(
        configured,
        json!({ "relay": {
            "host": host, "port": port, "tls": "none", "username": "relay-user",
            "password_set": true, "sender": "invites@wyrd.test",
        } }),
        "the stored relay reads back without its password"
    );
    let sealed: Vec<u8> =
        sqlx::query_scalar("SELECT smtp_password_enc FROM platform.login_settings")
            .fetch_one(&superuser)
            .await
            .expect("sealed password reads");
    assert!(
        !sealed
            .windows(b"relay-password-plaintext".len())
            .any(|w| w == b"relay-password-plaintext"),
        "the relay password is sealed at rest"
    );

    // 2. A test send reaches the relay.
    let probe = addressed("probe");
    let (status, tested) = social_call(
        &srv,
        social_request(
            Method::POST,
            "/platform/mail/test",
            Some(&session),
            Some(json!({ "recipient": probe })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "test send: {tested}");
    assert_eq!(tested, json!({ "accepted": true, "error": null }));
    assert_eq!(
        mail_to(&api, &probe).await.len(),
        1,
        "the test message arrived"
    );

    // 3. Each invitation is emailed separately with exactly its returned link.
    let team: Vec<String> = ["ana", "ben", "cy"].iter().map(|n| addressed(n)).collect();
    let (status, issued) = social_call(&srv, invite(team.clone())).await;
    assert_eq!(status, StatusCode::CREATED, "invite: {issued}");
    for (invitation, recipient) in issued["invitations"]
        .as_array()
        .expect("invitations")
        .iter()
        .zip(&team)
    {
        assert_eq!(invitation["invitation"]["recipient"], recipient.as_str());
        assert_eq!(
            invitation["invitation"]["delivery"], "accepted",
            "the relay accepted it: {invitation}"
        );
        let link = invitation["invitation_link"].as_str().expect("link");
        let mail = mail_to(&api, recipient).await;
        assert_eq!(mail.len(), 1, "one message per recipient");
        let (to, from, subject, text) = &mail[0];
        assert_eq!(
            to,
            &vec![recipient.clone()],
            "addressed to its recipient only"
        );
        assert_eq!(from, "invites@wyrd.test");
        assert!(subject.contains(&slug), "names the workspace: {subject}");
        assert!(text.contains(link), "carries the returned link: {text}");
        for other in team.iter().filter(|other| *other != recipient) {
            assert!(!text.contains(other.as_str()), "names no other recipient");
        }
    }

    // 4. A resend emails the replacement link.
    let first = &issued["invitations"][0];
    let resend_path = format!(
        "/v1/principals/users/invitations/{}/resend",
        first["invitation"]["id"].as_str().expect("id")
    );
    let (status, resent) = social_call(
        &srv,
        social_request(Method::POST, &resend_path, Some(&admin), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "resend: {resent}");
    assert_eq!(resent["invitation"]["delivery"], "accepted");
    let replacement = resent["invitation_link"].as_str().expect("link");
    assert_ne!(
        replacement,
        first["invitation_link"].as_str().expect("link")
    );
    let mail = mail_to(&api, &team[0]).await;
    assert_eq!(mail.len(), 2, "the resend is a second message");
    assert!(
        mail.iter()
            .any(|(_, _, _, text)| text.contains(replacement))
    );

    // 5. With the relay down the link still returns, marked failed.
    let closed = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("port reserves");
    let closed_port = closed.local_addr().expect("addr").port();
    drop(closed);
    let (status, _) = social_call(
        &srv,
        social_request(
            Method::PUT,
            "/platform/mail",
            Some(&session),
            Some(relay("127.0.0.1", closed_port)),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "relay repoints");
    let users_before: i64 = sqlx::query_scalar("SELECT count(*) FROM wyrd.auth_users")
        .fetch_one(&superuser)
        .await
        .expect("users count");
    let dana = addressed("dana");
    let (status, failed) = social_call(&srv, invite(vec![dana.clone()])).await;
    assert_eq!(status, StatusCode::CREATED, "invite: {failed}");
    let failed = &failed["invitations"][0];
    assert_eq!(failed["invitation"]["delivery"], "failed", "{failed}");
    assert!(
        failed["invitation_link"]
            .as_str()
            .is_some_and(|link| link.contains("#invitation=")),
        "the link is still returned to copy"
    );
    let (_, pending) = social_call(
        &srv,
        social_request(
            Method::GET,
            "/v1/principals/users/invitations",
            Some(&admin),
            None,
        ),
    )
    .await;
    let listed = pending["invitations"]
        .as_array()
        .expect("invitations")
        .iter()
        .find(|row| row["recipient"] == dana.as_str())
        .expect("the failed invitation stays pending");
    assert_eq!(listed["delivery"], "failed", "its last outcome is listed");
    let users_after: i64 = sqlx::query_scalar("SELECT count(*) FROM wyrd.auth_users")
        .fetch_one(&superuser)
        .await
        .expect("users count");
    assert_eq!(users_after, users_before, "a failed send creates no User");
    let (status, _) = social_call(
        &srv,
        social_request(
            Method::PUT,
            "/platform/mail",
            Some(&session),
            Some(relay(&host, port)),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "relay recovers");
    let (status, recovered) = social_call(
        &srv,
        social_request(
            Method::POST,
            &format!(
                "/v1/principals/users/invitations/{}/resend",
                failed["invitation"]["id"].as_str().expect("id")
            ),
            Some(&admin),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "resend after recovery: {recovered}");
    assert_eq!(recovered["invitation"]["delivery"], "accepted");
    let mail = mail_to(&api, &dana).await;
    assert_eq!(mail.len(), 1, "only the recovered resend reached the relay");
    assert!(
        mail[0]
            .3
            .contains(recovered["invitation_link"].as_str().expect("link"))
    );

    // 6. A stalled relay holds no tenant transaction.
    let stalled = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("stalled relay binds");
    let stalled_port = stalled.local_addr().expect("addr").port();
    let (status, _) = social_call(
        &srv,
        social_request(
            Method::PUT,
            "/platform/mail",
            Some(&session),
            Some(relay("127.0.0.1", stalled_port)),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "relay repoints to the stall");
    let eve = addressed("eve");
    let ((status, stalled_issue), ()) =
        tokio::join!(social_call(&srv, invite(vec![eve.clone()])), async {
            let (socket, _) = stalled.accept().await.expect("the server dials the relay");
            let (_, pending) = social_call(
                &srv,
                social_request(
                    Method::GET,
                    "/v1/principals/users/invitations",
                    Some(&admin),
                    None,
                ),
            )
            .await;
            assert!(
                pending["invitations"]
                    .as_array()
                    .expect("invitations")
                    .iter()
                    .any(|row| row["recipient"] == eve.as_str()),
                "the invitation committed before the relay answered"
            );
            drop(socket);
        });
    assert_eq!(status, StatusCode::CREATED, "invite: {stalled_issue}");
    assert_eq!(
        stalled_issue["invitations"][0]["invitation"]["delivery"],
        "failed"
    );
}

/// The session cookie a successful sign-in set for tenant `slug`.
fn session_cookie(location_and_cookies: &Response<Body>, slug: &str) -> Option<String> {
    let name = format!("wyrd_session_{slug}=");
    location_and_cookies
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with(&name))
        .and_then(|value| value.split(';').next())
        .map(str::to_owned)
}

/// Finish `begun` at the Google callback from its own browser; returns the
/// status, `Location` and the tenant session cookie pair, when one was set.
async fn finish_google_session(
    srv: &WyrdTestServer,
    begun: &BegunSocialLogin,
    issuer: &str,
    slug: &str,
) -> (StatusCode, Option<String>, Option<String>) {
    finish_social_session(
        srv,
        begun,
        &social_callback(&begun.query["state"], issuer),
        slug,
    )
    .await
}

/// Return to the social callback `uri` from `begun`'s own browser; returns
/// the status, `Location` and the tenant session cookie pair, when one was
/// set.
async fn finish_social_session(
    srv: &WyrdTestServer,
    begun: &BegunSocialLogin,
    uri: &str,
    slug: &str,
) -> (StatusCode, Option<String>, Option<String>) {
    let resp = srv
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(uri)
                .header(header::HOST, SOCIAL_HOST)
                .header(header::COOKIE, &begun.cookie)
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("callback responds");
    let location = resp
        .headers()
        .get(header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    (resp.status(), location, session_cookie(&resp, slug))
}

/// Call tenant route `uri` as a browser holding the session `cookie`.
async fn browser_get(srv: &WyrdTestServer, uri: &str, cookie: &str) -> (StatusCode, Value) {
    social_call(
        srv,
        Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header(header::HOST, SOCIAL_HOST)
            .header(header::ORIGIN, format!("http://{SOCIAL_HOST}"))
            .header(header::COOKIE, cookie)
            .body(Body::empty())
            .expect("request builds"),
    )
    .await
}

/// An invited person opens their link, proves Google and enters with
/// exactly the invited access, then returns with Google as the same User:
///   1. beginning Google from the link spends nothing;
///   2. a Google identity whose email differs from the invited address
///      enrolls one User, one identity and exactly the invited binding, and
///      consumes the invitation;
///   3. the session reaches the invited scope and nothing outside it;
///   4. after logout the same Google identity signs back in to the same User
///      and access, adding no record;
///   5. a spent, revoked, superseded or expired link admits no one and
///      creates no User, and an identity that already has a User cannot
///      enroll a second one and spends no link;
///   6. the identity is a stranger in another tenant until invited there,
///      then a distinct User, and no session crosses tenants;
///   7. a suspended User cannot return.
///
/// # Panics
/// Panics when any step's response or stored records differ.
#[tokio::test(flavor = "multi_thread")]
async fn an_invited_person_enrolls_with_google_and_returns() {
    let google = wiremock::MockServer::start().await;
    let srv = WyrdTestServer::builder()
        .with_social_endpoints_for_test(wyrd_server::config::SocialConfig {
            google_issuer: Some(google.uri()),
            ..Default::default()
        })
        .start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;
    let tenant = srv.data_tenant_id();
    let slug = srv.pg_fixture().tenant_slug().to_owned();
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    mount_platform_provider(
        &google,
        platform_key_set("google-key"),
        wiremock::ResponseTemplate::new(500),
    )
    .await;
    enable_google(&srv, &session, &google).await;
    let issuer = google.uri();
    let admin = srv
        .bootstrap_service_in_tenant(tenant, "google-invite-admin", &["admin"])
        .await
        .expect("tenant admin bootstraps");
    let admin = srv
        .exchange_api_key(admin.api_key().expect("admin has an api key"))
        .await
        .expect("admin key exchanges");
    let payments = json!({ "card": { "family": { "kind": "Prompt", "name": "payments" } } });
    let invite = |recipient: &str| {
        social_request(
            Method::POST,
            "/v1/principals/users/invitations",
            Some(&admin),
            Some(json!({
                "recipients": [recipient],
                "assignments": [{ "role": "viewer", "scope": payments }],
            })),
        )
    };
    let code_of = |issued: &Value| {
        issued["invitations"][0]["invitation_link"]
            .as_str()
            .and_then(|link| link.split_once("#invitation="))
            .map(|(_, code)| code.to_owned())
            .expect("the link carries its code")
    };
    let pending = || async {
        let (_, listed) = social_call(
            &srv,
            social_request(
                Method::GET,
                "/v1/principals/users/invitations",
                Some(&admin),
                None,
            ),
        )
        .await;
        listed["invitations"].as_array().expect("invitations").len()
    };
    let answer = |begun: &BegunSocialLogin, subject: &str, email: &str| {
        identity_token_reply(
            &issuer,
            &begun.query["nonce"],
            "wyrd-google",
            "google-key",
            subject,
            email,
        )
    };

    // 1. Beginning Google from the link spends nothing.
    let (status, issued) = social_call(&srv, invite("ana@example.test")).await;
    assert_eq!(status, StatusCode::CREATED, "invite: {issued}");
    let code = code_of(&issued);
    let before = local_records(&superuser).await;
    let begun = begin_google(
        &srv,
        json!({ "tenant": slug, "provider": "google", "invitation_code": code }),
    )
    .await;
    assert_eq!(pending().await, 1, "opening the link spends nothing");

    // 2. A differing Google email enrolls exactly the invited access.
    mount_platform_provider(
        &google,
        platform_key_set("google-key"),
        answer(&begun, "google-ana", "ana.personal@gmail.com"),
    )
    .await;
    let (status, location, cookie) = finish_google_session(&srv, &begun, &issuer, &slug).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "enrollment: {location:?}");
    assert_eq!(location.as_deref(), Some(format!("/t/{slug}").as_str()));
    let cookie = cookie.expect("a tenant session is set");
    let after = local_records(&superuser).await;
    assert_eq!(
        (after.0 - before.0, after.1 - before.1),
        (1, 1),
        "one User and one identity"
    );
    let (user,): (uuid::Uuid,) = sqlx::query_as(
        "SELECT user_id FROM wyrd.auth_user_identities WHERE issuer = $1 AND subject = 'google-ana'",
    )
    .bind(&issuer)
    .fetch_one(&superuser)
    .await
    .expect("the identity links its User");
    let bindings: Vec<(String, Value)> = sqlx::query_as(
        "SELECT r.name, b.scope FROM wyrd.auth_role_bindings b \
           JOIN wyrd.auth_roles r ON r.id = b.role_id WHERE b.user_id = $1",
    )
    .bind(user)
    .fetch_all(&superuser)
    .await
    .expect("bindings read");
    assert_eq!(bindings, vec![("viewer".to_owned(), payments.clone())]);
    assert_eq!(pending().await, 0, "the invitation is consumed");

    // 3. The session reaches the invited scope only.
    let (status, viewed) = browser_get(&srv, "/auth/browser/session", &cookie).await;
    assert_eq!(status, StatusCode::OK, "session view: {viewed}");
    assert_eq!(viewed["sessions"][0]["principal_id"], user.to_string());
    assert_eq!(viewed["sessions"][0]["credential"], "social");
    for name in ["payments", "ledger"] {
        let mut declare = social_request(
            Method::POST,
            "/v1/cards",
            Some(&admin),
            Some(json!({ "submissions": [{
                "apiVersion": "wyrd/v1",
                "kind": "Prompt",
                "metadata": { "name": name, "version": "1.0.0" },
                "spec": {
                    "provider": "openai",
                    "model": "gpt-4o",
                    "messages": [{ "role": "user", "content": "hello" }]
                },
                "artifacts": []
            }] })),
        );
        declare.headers_mut().insert(
            "idempotency-key",
            header::HeaderValue::from_static(if name == "payments" {
                "declare-p"
            } else {
                "declare-l"
            }),
        );
        let (status, body) = social_call(&srv, declare).await;
        assert_eq!(status, StatusCode::CREATED, "declare {name}: {body}");
    }
    let (status, body) = browser_get(&srv, "/v1/cards/Prompt/payments/latest", &cookie).await;
    assert_eq!(status, StatusCode::OK, "the invited scope: {body}");
    let (status, body) = browser_get(&srv, "/v1/cards/Prompt/ledger/latest", &cookie).await;
    assert_eq!(
        (status, body["code"].as_str()),
        (
            StatusCode::NOT_FOUND,
            Some("WYRD_REGISTRY_404_CARD_NOT_FOUND")
        ),
        "outside the invited scope a Card is not visible: {body}"
    );

    // 4. Returning with Google signs in the same User, adding nothing.
    let begun = begin_google_login(&srv, &slug).await;
    mount_platform_provider(
        &google,
        platform_key_set("google-key"),
        answer(&begun, "google-ana", "ana.renamed@gmail.com"),
    )
    .await;
    let records = local_records(&superuser).await;
    let (status, location, returned) = finish_google_session(&srv, &begun, &issuer, &slug).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "return: {location:?}");
    let returned = returned.expect("the returning sign-in sets a session");
    let (_, viewed) = browser_get(&srv, "/auth/browser/session", &returned).await;
    assert_eq!(viewed["sessions"][0]["principal_id"], user.to_string());
    let after = local_records(&superuser).await;
    assert_eq!(
        (after.0, after.1, after.3),
        (records.0, records.1, records.3),
        "a return adds no User, identity or grant"
    );

    // 5. Spent, revoked and superseded links, and an owned identity, admit no one.
    let refused = format!("/t/{slug}/login?login=failed");
    let enroll_with = |code: String, subject: &'static str| {
        let srv = &srv;
        let slug = &slug;
        let google = &google;
        let issuer = &issuer;
        async move {
            let begun = begin_google(
                srv,
                json!({ "tenant": slug, "provider": "google", "invitation_code": code }),
            )
            .await;
            mount_platform_provider(
                google,
                platform_key_set("google-key"),
                identity_token_reply(
                    issuer,
                    &begun.query["nonce"],
                    "wyrd-google",
                    "google-key",
                    subject,
                    "someone@example.test",
                ),
            )
            .await;
            finish_google_session(srv, &begun, issuer, slug).await
        }
    };
    let before = local_records(&superuser).await;
    let (_, location, cookie) = enroll_with(code, "google-replay").await;
    assert_eq!(location.as_deref(), Some(refused.as_str()), "a spent link");
    assert!(cookie.is_none());
    let (_, revoked) = social_call(&srv, invite("rev@example.test")).await;
    let (status, _) = social_call(
        &srv,
        social_request(
            Method::DELETE,
            &format!(
                "/v1/principals/users/invitations/{}",
                revoked["invitations"][0]["invitation"]["id"]
                    .as_str()
                    .expect("id")
            ),
            Some(&admin),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, location, _) = enroll_with(code_of(&revoked), "google-revoked").await;
    assert_eq!(
        location.as_deref(),
        Some(refused.as_str()),
        "a revoked link"
    );
    let (_, superseded) = social_call(&srv, invite("sup@example.test")).await;
    let (status, _) = social_call(
        &srv,
        social_request(
            Method::POST,
            &format!(
                "/v1/principals/users/invitations/{}/resend",
                superseded["invitations"][0]["invitation"]["id"]
                    .as_str()
                    .expect("id")
            ),
            Some(&admin),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, location, _) = enroll_with(code_of(&superseded), "google-superseded").await;
    assert_eq!(
        location.as_deref(),
        Some(refused.as_str()),
        "a superseded link"
    );
    assert_eq!(
        local_records(&superuser).await.0,
        before.0,
        "no User was created"
    );
    let (_, owned) = social_call(&srv, invite("owned@example.test")).await;
    let live = pending().await;
    let (_, location, cookie) = enroll_with(code_of(&owned), "google-ana").await;
    assert_eq!(
        location.as_deref(),
        Some(format!("/t/{slug}/login?login=account_exists").as_str()),
        "an identity with a User is told to sign in"
    );
    assert!(cookie.is_none());
    assert_eq!(
        pending().await,
        live,
        "the owned-identity refusal spends no link"
    );
    assert_eq!(local_records(&superuser).await.0, before.0);
    let (_, expired) = social_call(&srv, invite("late@example.test")).await;
    sqlx::query(
        "UPDATE wyrd.auth_user_invitations SET created_at = created_at - interval '25 hours', \
            expires_at = expires_at - interval '25 hours' WHERE id = $1",
    )
    .bind(
        uuid::Uuid::parse_str(
            expired["invitations"][0]["invitation"]["id"]
                .as_str()
                .expect("id"),
        )
        .expect("uuid"),
    )
    .execute(&superuser)
    .await
    .expect("invitation ages");
    let (_, location, _) = enroll_with(code_of(&expired), "google-late").await;
    assert_eq!(
        location.as_deref(),
        Some(refused.as_str()),
        "an expired link"
    );
    assert_eq!(
        local_records(&superuser).await.0,
        before.0,
        "no User was created"
    );

    // 6. The same Google identity is a stranger in another tenant until that
    //    tenant invites it, and then it is a distinct User there.
    let beta = srv.seed_tenant("beta").await.expect("tenant seeds");
    let begun = begin_google_login(&srv, "beta").await;
    mount_platform_provider(
        &google,
        platform_key_set("google-key"),
        answer(&begun, "google-ana", "ana.personal@gmail.com"),
    )
    .await;
    let (_, location, cookie) = finish_google_session(&srv, &begun, &issuer, "beta").await;
    assert_eq!(location.as_deref(), Some("/t/beta/login?login=no_account"));
    assert!(cookie.is_none(), "an identity is tenant-local");
    let beta_admin = srv
        .bootstrap_service_in_tenant(beta, "beta-invite-admin", &["admin"])
        .await
        .expect("beta admin bootstraps");
    let beta_admin = srv
        .exchange_api_key(beta_admin.api_key().expect("admin has an api key"))
        .await
        .expect("beta admin key exchanges");
    let (status, beta_issued) = social_call(
        &srv,
        social_request(
            Method::POST,
            "/v1/principals/users/invitations",
            Some(&beta_admin),
            Some(json!({
                "recipients": ["ana@example.test"],
                "assignments": [{ "role": "viewer", "scope": "all" }],
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "beta invite: {beta_issued}");
    let begun = begin_google(
        &srv,
        json!({ "tenant": "beta", "provider": "google", "invitation_code": code_of(&beta_issued) }),
    )
    .await;
    mount_platform_provider(
        &google,
        platform_key_set("google-key"),
        answer(&begun, "google-ana", "ana.personal@gmail.com"),
    )
    .await;
    let (_, location, beta_cookie) = finish_google_session(&srv, &begun, &issuer, "beta").await;
    assert_eq!(location.as_deref(), Some("/t/beta"));
    let beta_cookie = beta_cookie.expect("a beta session is set");
    let (_, viewed) = browser_get(&srv, "/auth/browser/session", &beta_cookie).await;
    let beta_user = viewed["sessions"][0]["principal_id"]
        .as_str()
        .expect("principal")
        .to_owned();
    assert_ne!(beta_user, user.to_string(), "a distinct User per tenant");
    let relabeled = returned.replacen(&format!("wyrd_session_{slug}="), "wyrd_session_beta=", 1);
    let (status, _) = browser_get(&srv, "/auth/browser/session", &relabeled).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "no cross-tenant session use"
    );

    // 7. A suspended User cannot return.
    sqlx::query("UPDATE wyrd.auth_users SET status = 'suspended' WHERE id = $1")
        .bind(user)
        .execute(&superuser)
        .await
        .expect("User suspends");
    let begun = begin_google_login(&srv, &slug).await;
    mount_platform_provider(
        &google,
        platform_key_set("google-key"),
        answer(&begun, "google-ana", "ana.personal@gmail.com"),
    )
    .await;
    let (_, location, cookie) = finish_google_session(&srv, &begun, &issuer, &slug).await;
    assert_eq!(
        location.as_deref(),
        Some(refused.as_str()),
        "a suspended User"
    );
    assert!(cookie.is_none());
}

/// GitHub.com's identity namespace, fixed whatever endpoints a test uses.
const GITHUB_NAMESPACE: &str = "https://github.com";

/// Reset `github` to a GitHub.com stand-in: its token endpoint redeems any
/// code for `token`, and its user API answers `user` only to that token.
async fn mount_github(github: &wiremock::MockServer, token: &str, user: Value) {
    github.reset().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/login/oauth/access_token"))
        .and(wiremock::matchers::header("accept", "application/json"))
        .and(wiremock::matchers::body_string_contains("code_verifier="))
        .and(wiremock::matchers::body_string_contains(
            "client_secret=github-secret",
        ))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "access_token": token, "token_type": "bearer", "scope": ""
        })))
        .mount(github)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/user"))
        .and(wiremock::matchers::header(
            "authorization",
            format!("Bearer {token}").as_str(),
        ))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(user))
        .mount(github)
        .await;
}

/// The GitHub callback for provider `state`, as GitHub redirects to it.
fn github_callback(state: &str) -> String {
    format!("/auth/social/github/callback?code=github-code&state={state}")
}

/// Configure, test and enable the deployment's GitHub client `wyrd-github`
/// against the mock `github` through the platform API; returns the enabled
/// revision.
///
/// # Panics
/// Panics when any platform step is refused.
async fn enable_github(srv: &WyrdTestServer, session: &str, github: &wiremock::MockServer) -> u64 {
    let (status, stored) = social_call(
        srv,
        social_request(
            Method::PUT,
            "/platform/social/providers/github",
            Some(session),
            Some(json!({ "client_id": "wyrd-github", "client_secret": "github-secret" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "GitHub configures: {stored}");
    let revision = stored["revision"].as_u64().expect("revision");
    let (status, begun) = social_call(
        srv,
        social_request(
            Method::POST,
            "/platform/social/providers/github/test",
            Some(session),
            Some(json!({ "expected_revision": revision })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "test begins: {begun}");
    let query = authorization_query(begun["authorization_url"].as_str().expect("url"));
    mount_github(
        github,
        "gho_operator",
        json!({ "id": 1, "login": "operator" }),
    )
    .await;
    let (status, tested) = social_call(
        srv,
        social_request(Method::GET, &github_callback(&query["state"]), None, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the provider test passes: {tested}");
    let (status, enabled) = social_call(
        srv,
        social_request(
            Method::PUT,
            "/platform/social/providers/github/enabled",
            Some(session),
            Some(json!({ "enabled": true, "expected_revision": revision })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "GitHub enables: {enabled}");
    revision
}

/// GitHub.com admits an invited person through the same flow as Google,
/// keyed by GitHub's numeric account ID:
///   1. the operator configures, tests and enables GitHub;
///   2. GitHub begins with S256 PKCE and state and requests no scope at all;
///   3. the invited person enrolls one User under the fixed GitHub.com
///      namespace and their numeric ID, with exactly the invited binding,
///      and the session reads the invited Card and not another;
///   4. after renaming their GitHub login they return to the same User;
///   5. another account ID with the same login and email is a stranger;
///   6. a spent link, an owned identity and a provider revision replaced
///      mid-sign-in admit no one;
///   7. a failing GitHub token endpoint creates nothing and sets no session.
///
/// # Panics
/// Panics when any step's response or stored records differ.
#[tokio::test(flavor = "multi_thread")]
async fn an_invited_person_enrolls_with_github_and_returns() {
    let github = wiremock::MockServer::start().await;
    let srv = WyrdTestServer::builder()
        .with_social_endpoints_for_test(wyrd_server::config::SocialConfig {
            google_issuer: None,
            github_url: Some(github.uri()),
            github_api_url: Some(github.uri()),
            turnstile_siteverify_url: None,
        })
        .start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;
    let tenant = srv.data_tenant_id();
    let slug = srv.pg_fixture().tenant_slug().to_owned();
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");

    // 1. The operator enables GitHub through the management journey.
    let revision = enable_github(&srv, &session, &github).await;
    let admin = srv
        .bootstrap_service_in_tenant(tenant, "github-invite-admin", &["admin"])
        .await
        .expect("tenant admin bootstraps");
    let admin = srv
        .exchange_api_key(admin.api_key().expect("admin has an api key"))
        .await
        .expect("admin key exchanges");
    let payments = json!({ "card": { "family": { "kind": "Prompt", "name": "payments" } } });
    let invite = |recipient: &str| {
        social_request(
            Method::POST,
            "/v1/principals/users/invitations",
            Some(&admin),
            Some(json!({
                "recipients": [recipient],
                "assignments": [{ "role": "viewer", "scope": payments }],
            })),
        )
    };
    let code_of = |issued: &Value| {
        issued["invitations"][0]["invitation_link"]
            .as_str()
            .and_then(|link| link.split_once("#invitation="))
            .map(|(_, code)| code.to_owned())
            .expect("the link carries its code")
    };
    let sign_in = |code: Option<String>, user: Value| {
        let srv = &srv;
        let slug = &slug;
        let github = &github;
        async move {
            let begun = begin_social(
                srv,
                json!({ "tenant": slug, "provider": "github", "invitation_code": code }),
            )
            .await;
            mount_github(github, "gho_person", user).await;
            let finished =
                finish_social_session(srv, &begun, &github_callback(&begun.query["state"]), slug)
                    .await;
            (begun, finished)
        }
    };

    // 2 and 3. The invited person enrolls with GitHub.
    let (status, issued) = social_call(&srv, invite("ana@example.test")).await;
    assert_eq!(status, StatusCode::CREATED, "invite: {issued}");
    let before = local_records(&superuser).await;
    let (begun, (status, location, cookie)) = sign_in(
        Some(code_of(&issued)),
        json!({ "id": 4242, "login": "ana-gh", "name": "  ", "email": "ana@users.example" }),
    )
    .await;
    assert_eq!(begun.query["client_id"], "wyrd-github");
    assert_eq!(begun.query["code_challenge_method"], "S256");
    assert!(begun.query.contains_key("code_challenge") && begun.query.contains_key("state"));
    assert!(
        !begun.query.contains_key("scope"),
        "GitHub is asked for no scope: {:?}",
        begun.query
    );
    assert!(begun.query["redirect_uri"].ends_with("/auth/social/github/callback"));
    assert_eq!(status, StatusCode::SEE_OTHER, "enrollment: {location:?}");
    assert_eq!(location.as_deref(), Some(format!("/t/{slug}").as_str()));
    let cookie = cookie.expect("a tenant session is set");
    let after = local_records(&superuser).await;
    assert_eq!(
        (after.0 - before.0, after.1 - before.1),
        (1, 1),
        "one User and one identity"
    );
    let (user,): (uuid::Uuid,) = sqlx::query_as(
        "SELECT user_id FROM wyrd.auth_user_identities WHERE issuer = $1 AND subject = '4242'",
    )
    .bind(GITHUB_NAMESPACE)
    .fetch_one(&superuser)
    .await
    .expect("the numeric ID under the GitHub.com namespace links the User");
    let bindings: Vec<(String, Value)> = sqlx::query_as(
        "SELECT r.name, b.scope FROM wyrd.auth_role_bindings b \
           JOIN wyrd.auth_roles r ON r.id = b.role_id WHERE b.user_id = $1",
    )
    .bind(user)
    .fetch_all(&superuser)
    .await
    .expect("bindings read");
    assert_eq!(bindings, vec![("viewer".to_owned(), payments.clone())]);
    let mut declare = social_request(
        Method::POST,
        "/v1/cards",
        Some(&admin),
        Some(json!({ "submissions": [{
            "apiVersion": "wyrd/v1",
            "kind": "Prompt",
            "metadata": { "name": "payments", "version": "1.0.0" },
            "spec": {
                "provider": "openai",
                "model": "gpt-4o",
                "messages": [{ "role": "user", "content": "hello" }]
            },
            "artifacts": []
        }] })),
    );
    declare.headers_mut().insert(
        "idempotency-key",
        header::HeaderValue::from_static("declare-github-payments"),
    );
    let (status, body) = social_call(&srv, declare).await;
    assert_eq!(status, StatusCode::CREATED, "declare: {body}");
    let (status, body) = browser_get(&srv, "/v1/cards/Prompt/payments/latest", &cookie).await;
    assert_eq!(status, StatusCode::OK, "the invited scope: {body}");
    let (_, viewed) = browser_get(&srv, "/auth/browser/session", &cookie).await;
    assert_eq!(viewed["sessions"][0]["credential"], "social");
    // A blank profile name falls back to the login, read back through `/me`
    // and the directory as a GitHub sign-in.
    let named = |cookie: String| {
        let srv = &srv;
        let admin = &admin;
        async move {
            let (status, me) = browser_get(srv, "/v1/principals/me", &cookie).await;
            assert_eq!(status, StatusCode::OK, "profile reads: {me}");
            let (_, page) = social_call(
                srv,
                social_request(
                    Method::GET,
                    "/v1/principals?email=ana@users.example",
                    Some(admin),
                    None,
                ),
            )
            .await;
            let item = &page["principals"][0];
            assert_eq!(item["authentication"], "github");
            assert_eq!(item["display_name"], me["display_name"]);
            assert_eq!(me["authentication"], "github");
            assert_eq!(me["email"], "ana@users.example");
            me["display_name"].clone()
        }
    };
    assert_eq!(named(cookie.clone()).await, json!("ana-gh"));

    // 4. A renamed login with the same numeric ID returns to the same User,
    // whose profile name now replaces the stored one.
    let records = local_records(&superuser).await;
    let (_, (status, _, returned)) = sign_in(
        None,
        json!({ "id": 4242, "login": "ana-renamed", "name": "Ana Lima", "email": null }),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let returned = returned.expect("the returning sign-in sets a session");
    let (_, viewed) = browser_get(&srv, "/auth/browser/session", &returned).await;
    assert_eq!(viewed["sessions"][0]["principal_id"], user.to_string());
    assert_eq!(named(returned.clone()).await, json!("Ana Lima"));
    let after = local_records(&superuser).await;
    assert_eq!(
        (after.0, after.1, after.3),
        (records.0, records.1, records.3),
        "a return adds no User, identity or grant"
    );

    // 5. Another numeric ID with the same login and email inherits nothing.
    let (_, (_, location, cookie)) = sign_in(
        None,
        json!({ "id": 9999, "login": "ana-renamed", "email": "ana@users.example" }),
    )
    .await;
    assert_eq!(
        location.as_deref(),
        Some(format!("/t/{slug}/login?login=no_account").as_str())
    );
    assert!(cookie.is_none());

    // 6. A spent link, an owned identity and a replaced revision admit no one.
    let refused = format!("/t/{slug}/login?login=failed");
    let (_, (_, location, _)) = sign_in(
        Some(code_of(&issued)),
        json!({ "id": 5150, "login": "replayer" }),
    )
    .await;
    assert_eq!(location.as_deref(), Some(refused.as_str()), "a spent link");
    let (_, owned) = social_call(&srv, invite("owned@example.test")).await;
    let (_, (_, location, _)) = sign_in(
        Some(code_of(&owned)),
        json!({ "id": 4242, "login": "ana-renamed" }),
    )
    .await;
    assert_eq!(
        location.as_deref(),
        Some(format!("/t/{slug}/login?login=account_exists").as_str()),
        "an owned identity is told to sign in"
    );
    let begun = begin_social(
        &srv,
        json!({ "tenant": slug, "provider": "github", "invitation_code": code_of(&owned) }),
    )
    .await;
    let (status, replaced) = social_call(
        &srv,
        social_request(
            Method::PUT,
            "/platform/social/providers/github",
            Some(&session),
            Some(json!({ "client_id": "wyrd-github", "client_secret": "github-secret" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "GitHub reconfigures: {replaced}");
    assert!(replaced["revision"].as_u64() > Some(revision));
    mount_github(
        &github,
        "gho_person",
        json!({ "id": 6060, "login": "late" }),
    )
    .await;
    let (_, location, cookie) =
        finish_social_session(&srv, &begun, &github_callback(&begun.query["state"]), &slug).await;
    assert_eq!(
        location.as_deref(),
        Some(refused.as_str()),
        "a replaced revision"
    );
    assert!(cookie.is_none());
    assert_eq!(
        local_records(&superuser).await.0,
        records.0,
        "no User was created"
    );

    // 7. A failing GitHub creates nothing and falls back to nothing.
    let current = enable_github(&srv, &session, &github).await;
    assert!(current > revision);
    let begun = begin_social(
        &srv,
        json!({ "tenant": slug, "provider": "github", "invitation_code": code_of(&owned) }),
    )
    .await;
    github.reset().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .respond_with(wiremock::ResponseTemplate::new(502))
        .mount(&github)
        .await;
    let (status, _, cookie) =
        finish_social_session(&srv, &begun, &github_callback(&begun.query["state"]), &slug).await;
    assert!(status.is_server_error(), "a provider outage: {status}");
    assert!(cookie.is_none());
    assert_eq!(local_records(&superuser).await.0, records.0);
}

/// Prove `subject` at Google for a workspace signup begun from a fresh
/// browser; returns that browser's `wyrd_signup` cookie pair.
///
/// # Panics
/// Panics when signup does not begin, or the return is not sent on to the
/// workspace form with a completion cookie.
async fn prove_signup(
    srv: &WyrdTestServer,
    google: &wiremock::MockServer,
    subject: &str,
) -> String {
    prove_signup_with(srv, google, subject, true).await
}

/// [`prove_signup`] where Google reports the email `email_verified`.
///
/// # Panics
/// As [`prove_signup`].
async fn prove_signup_with(
    srv: &WyrdTestServer,
    google: &wiremock::MockServer,
    subject: &str,
    email_verified: bool,
) -> String {
    let begun = begin_social_at(
        srv,
        "/auth/signup",
        json!({ "method": "social", "provider": "google" }),
    )
    .await;
    assert!(
        begun.set_cookie.contains("Path=/auth/social"),
        "{}",
        begun.set_cookie
    );
    mount_platform_provider(
        google,
        platform_key_set("google-key"),
        identity_token_reply_with(
            &google.uri(),
            &begun.query["nonce"],
            "wyrd-google",
            "google-key",
            subject,
            &format!("{subject}@example.test"),
            email_verified,
        ),
    )
    .await;
    let resp = srv
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(social_callback(&begun.query["state"], &google.uri()))
                .header(header::HOST, SOCIAL_HOST)
                .header(header::COOKIE, &begun.cookie)
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("callback responds");
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        resp.headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok()),
        Some("/signup/workspace")
    );
    let set_cookie = resp
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with("wyrd_signup="))
        .expect("the completion cookie is set")
        .to_owned();
    assert!(set_cookie.contains("Path=/auth/signup"), "{set_cookie}");
    assert!(set_cookie.contains("HttpOnly"), "{set_cookie}");
    set_cookie
        .split(';')
        .next()
        .expect("cookie pair")
        .to_owned()
}

/// Complete the signup held by `cookie` as workspace `slug` from source
/// `ip`; returns the status, body and new workspace's session cookie.
async fn complete_signup(
    srv: &WyrdTestServer,
    cookie: &str,
    slug: &str,
    ip: [u8; 4],
) -> (StatusCode, Value, Option<String>) {
    let resp = srv
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup/complete")
                .header(header::HOST, SOCIAL_HOST)
                .header(header::ORIGIN, format!("http://{SOCIAL_HOST}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie)
                .extension(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                    ip, 40_000,
                ))))
                .body(Body::from(
                    serde_json::to_vec(&json!({ "slug": slug, "display_name": "Acme" }))
                        .expect("serializes"),
                ))
                .expect("request builds"),
        )
        .await
        .expect("completion responds");
    let status = resp.status();
    let session = session_cookie(&resp, slug);
    (status, body_json(resp).await, session)
}

/// The directory rows named `slug`: status and the attempt that claimed it.
async fn tenants_named(pool: &PgPool, slug: &str) -> Vec<(uuid::Uuid, String, Option<uuid::Uuid>)> {
    sqlx::query_as(
        "SELECT data_tenant_id, status, signup_attempt_id FROM platform.tenants WHERE slug = $1",
    )
    .bind(slug)
    .fetch_all(pool)
    .await
    .expect("directory reads")
}

/// A visitor proves Google, creates a workspace and administers it as an
/// ordinary tenant User:
///   1. while signup is off, signup cannot begin;
///   2. an invalid or existing slug creates nothing and leaves the proof
///      retryable;
///   3. a valid slug provisions an active workspace whose builtin roles and
///      SYSTEM exist, whose one User holds the admin binding through the
///      proven identity, and whose response carries a tenant session and no
///      key, platform principal or platform authority; that session invites
///      a teammate;
///   4. repeating the completion returns the same tenant without a session
///      and creates no second one, naming another slug conflicts, and a lost
///      session response is recovered by returning Google sign-in;
///   5. racing one proof across two replicas creates exactly one tenant: the
///      other completion is in progress or returns that tenant without a
///      session;
///   6. an interrupted claim resumes only for its own attempt: another
///      identity cannot adopt it, and it never becomes active without its
///      administrator;
///   7. the same account creates a second independent workspace and gains
///      nothing in the existing one;
///   8. disabling signup or the provider after proof refuses completion;
///   9. the shared identity and source windows refuse creation;
///  10. unavailable admission state refuses creation.
///
/// # Panics
/// Panics when any step's response or stored records differ.
#[tokio::test(flavor = "multi_thread")]
async fn a_visitor_creates_a_workspace_with_google() {
    let google = wiremock::MockServer::start().await;
    let srv = WyrdTestServer::builder()
        .with_social_endpoints_for_test(wyrd_server::config::SocialConfig {
            google_issuer: Some(google.uri()),
            ..Default::default()
        })
        .start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    let existing = srv.pg_fixture().tenant_slug().to_owned();
    mount_platform_provider(
        &google,
        platform_key_set("google-key"),
        wiremock::ResponseTemplate::new(500),
    )
    .await;
    enable_google(&srv, &session, &google).await;
    let settings = |signup: bool| {
        social_request(
            Method::PUT,
            "/platform/login-settings",
            Some(&session),
            Some(json!({ "password_enabled": true, "signup_enabled": signup })),
        )
    };
    let home = [192, 0, 2, 10];

    // 1. While signup is off, signup cannot begin.
    let request = Request::builder()
        .method(Method::POST)
        .uri("/auth/signup")
        .header(header::HOST, SOCIAL_HOST)
        .header(header::ORIGIN, format!("http://{SOCIAL_HOST}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"method":"social","provider":"google"}"#))
        .expect("request builds");
    let (status, refused) = social_call(&srv, request).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");
    assert_eq!(
        refused["code"], "WYRD_AUTH_403_METHOD_UNAVAILABLE",
        "{refused}"
    );
    let (status, changed) = social_call(&srv, settings(true)).await;
    assert_eq!(status, StatusCode::OK, "signup enables: {changed}");

    // 2. An invalid or existing slug creates nothing; the proof stays usable.
    let ana = prove_signup(&srv, &google, "google-ana").await;
    let (status, _, _) = complete_signup(&srv, &ana, "Not A Slug", home).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, taken, _) = complete_signup(&srv, &ana, &existing, home).await;
    assert_eq!(status, StatusCode::CONFLICT, "{taken}");
    assert_eq!(tenants_named(&superuser, &existing).await.len(), 1);
    assert_eq!(
        tenants_named(&superuser, &existing).await[0].2,
        None,
        "an existing tenant is never adopted"
    );

    // 3. A valid slug provisions a usable workspace administered by its creator.
    let operators = platform_principals(&superuser).await;
    let (status, created, cookie) = complete_signup(&srv, &ana, "acme", home).await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(
        created,
        json!({ "tenant": "acme" }),
        "no key or platform authority"
    );
    let cookie = cookie.expect("the creator's session is set");
    let rows = tenants_named(&superuser, "acme").await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1, "active");
    let acme = rows[0].0;
    let (users, identities, admin_bindings, keys, services, roles, system): (
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
    ) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM wyrd.auth_users WHERE data_tenant_id = $1), \
                (SELECT count(*) FROM wyrd.auth_user_identities \
                  WHERE data_tenant_id = $1 AND subject = 'google-ana'), \
                (SELECT count(*) FROM wyrd.auth_role_bindings b \
                   JOIN wyrd.auth_roles r ON r.id = b.role_id \
                  WHERE b.data_tenant_id = $1 AND r.name = 'admin' AND b.user_id IS NOT NULL), \
                (SELECT count(*) FROM wyrd.auth_api_keys WHERE data_tenant_id = $1), \
                (SELECT count(*) FROM wyrd.auth_service_accounts \
                  WHERE data_tenant_id = $1 AND principal_kind <> 'system'), \
                (SELECT count(*) FROM wyrd.auth_roles WHERE data_tenant_id = $1 AND builtin), \
                (SELECT count(*) FROM wyrd.auth_service_accounts \
                  WHERE data_tenant_id = $1 AND principal_kind = 'system')",
    )
    .bind(acme)
    .fetch_one(&superuser)
    .await
    .expect("tenant records count");
    assert_eq!((users, identities, admin_bindings), (1, 1, 1));
    assert_eq!((keys, services), (0, 0), "no bootstrap principal or key");
    assert!(roles > 0, "builtin roles are seeded");
    assert_eq!(system, 1, "SYSTEM exists");
    assert_eq!(
        platform_principals(&superuser).await,
        operators,
        "no platform principal"
    );
    let (status, invited) = social_call(
        &srv,
        Request::builder()
            .method(Method::POST)
            .uri("/v1/principals/users/invitations")
            .header(header::HOST, SOCIAL_HOST)
            .header(header::ORIGIN, format!("http://{SOCIAL_HOST}"))
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, &cookie)
            .body(Body::from(
                serde_json::to_vec(&json!({
                    "recipients": ["teammate@example.test"],
                    "assignments": [{ "role": "viewer", "scope": "all" }],
                }))
                .expect("serializes"),
            ))
            .expect("request builds"),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "the creator invites: {invited}"
    );

    // 4. A replay returns the same workspace and creates nothing; a lost
    // response is recovered by sign-in.
    let (status, replay, again) = complete_signup(&srv, &ana, "acme", home).await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    assert_eq!(replay, json!({ "tenant": "acme" }));
    assert!(again.is_none(), "a replay opens no session");
    let (status, _, _) = complete_signup(&srv, &ana, "acme-other", home).await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "a completed proof creates no other tenant"
    );
    assert!(tenants_named(&superuser, "acme-other").await.is_empty());
    let begun = begin_google_login(&srv, "acme").await;
    mount_platform_provider(
        &google,
        platform_key_set("google-key"),
        identity_token_reply(
            &google.uri(),
            &begun.query["nonce"],
            "wyrd-google",
            "google-key",
            "google-ana",
            "ana@example.test",
        ),
    )
    .await;
    let (status, location, returned) =
        finish_google_session(&srv, &begun, &google.uri(), "acme").await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{location:?}");
    assert_eq!(location.as_deref(), Some("/t/acme"));
    assert!(returned.is_some(), "the creator signs back in");

    // 5. One proof raced across replicas creates exactly one tenant.
    let replica = srv
        .start_replica(WyrdTestServer::builder())
        .await
        .expect("replica starts");
    let bea = prove_signup(&srv, &google, "google-bea").await;
    let (first, second) = tokio::join!(
        complete_signup(&srv, &bea, "bea", home),
        complete_signup(&replica, &bea, "bea", home),
    );
    for (status, body, _) in [&first, &second] {
        assert!(
            (*status == StatusCode::OK && *body == json!({ "tenant": "bea" }))
                || (*status == StatusCode::CONFLICT
                    && body["details"]["reason"] == "signup_in_progress"),
            "each converges on the one workspace: {first:?} {second:?}"
        );
    }
    assert_eq!(
        [&first.2, &second.2]
            .iter()
            .filter(|cookie| cookie.is_some())
            .count(),
        1,
        "only the creating completion opens a session: {first:?} {second:?}"
    );
    let rows = tenants_named(&superuser, "bea").await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1, "active");

    // 6. An interrupted claim resumes only for its own attempt.
    let cara = prove_signup(&srv, &google, "google-cara").await;
    let dan = prove_signup(&srv, &google, "google-dan").await;
    let interrupted = uuid::Uuid::now_v7();
    sqlx::query(
        "WITH a AS (UPDATE platform.signup_attempts SET status = 'failed' \
                     WHERE subject = 'google-cara' RETURNING id) \
         INSERT INTO platform.tenants \
             (data_tenant_id, slug, display_name, status, provisioning_failed_reason, \
              signup_attempt_id, signup_creator_key) \
         SELECT $1, 'cara', 'Cara', 'failed', 'interrupted', id, \
                sha256(convert_to('cara', 'UTF8')) FROM a",
    )
    .bind(interrupted)
    .execute(&superuser)
    .await
    .expect("an interrupted creation is staged");
    let (status, adopted, _) = complete_signup(&srv, &dan, "cara", home).await;
    assert_eq!(status, StatusCode::CONFLICT, "{adopted}");
    let rows = tenants_named(&superuser, "cara").await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1, "failed", "no active ownerless tenant");
    let (status, resumed, cookie) = complete_signup(&srv, &cara, "cara", home).await;
    assert_eq!(status, StatusCode::OK, "{resumed}");
    assert!(cookie.is_some());
    let rows = tenants_named(&superuser, "cara").await;
    assert_eq!((rows[0].0, rows[0].1.as_str()), (interrupted, "active"));

    // 7. The same account creates a second, independent workspace.
    let ana_again = prove_signup(&srv, &google, "google-ana").await;
    let (status, second, _) = complete_signup(&srv, &ana_again, "acme-two", home).await;
    assert_eq!(status, StatusCode::OK, "{second}");
    let users: Vec<(uuid::Uuid,)> = sqlx::query_as(
        "SELECT user_id FROM wyrd.auth_user_identities WHERE subject = 'google-ana'",
    )
    .fetch_all(&superuser)
    .await
    .expect("identities read");
    assert_eq!(users.len(), 2, "one User per workspace");
    assert_ne!(users[0].0, users[1].0);
    let tenant = srv.data_tenant_id();
    let elsewhere: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wyrd.auth_user_identities WHERE data_tenant_id = $1",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&superuser)
    .await
    .expect("identities count");
    assert_eq!(elsewhere, 0, "no access to an existing workspace");

    // 8. Disabling signup or the provider after proof refuses completion.
    let eve = prove_signup(&srv, &google, "google-eve").await;
    let (status, _) = social_call(&srv, settings(false)).await;
    assert_eq!(status, StatusCode::OK);
    let (status, off, _) = complete_signup(&srv, &eve, "eve", home).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{off}");
    assert_eq!(off["code"], "WYRD_AUTH_403_METHOD_UNAVAILABLE", "{off}");
    let (status, _) = social_call(&srv, settings(true)).await;
    assert_eq!(status, StatusCode::OK);
    set_google_enabled(&srv, &session, 1, false).await;
    let (status, off, _) = complete_signup(&srv, &eve, "eve", home).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{off}");
    assert_eq!(off["code"], "WYRD_AUTH_403_METHOD_UNAVAILABLE", "{off}");
    assert!(tenants_named(&superuser, "eve").await.is_empty());
    set_google_enabled(&srv, &session, 1, true).await;

    // 9. The shared identity and source windows refuse creation.
    let flood = [198, 51, 100, 7];
    let mut proofs = Vec::new();
    for subject in [
        "google-f1",
        "google-f2",
        "google-f3",
        "google-f4",
        "google-f5",
    ] {
        proofs.push(prove_signup(&srv, &google, subject).await);
    }
    for proof in &proofs[..4] {
        for _ in 0..5 {
            let (status, body, _) = complete_signup(&srv, proof, &existing, flood).await;
            assert_eq!(status, StatusCode::CONFLICT, "{body}");
        }
    }
    let (status, limited, _) = complete_signup(&replica, &proofs[0], &existing, home).await;
    assert_eq!(
        status,
        StatusCode::TOO_MANY_REQUESTS,
        "identity window: {limited}"
    );
    assert_eq!(limited["code"], "WYRD_AUTH_429_SIGNUP_ATTEMPTS");
    assert!(limited["details"]["retry_after_seconds"].as_u64() > Some(0));
    let (status, limited, _) = complete_signup(&replica, &proofs[4], "f5", flood).await;
    assert_eq!(
        status,
        StatusCode::TOO_MANY_REQUESTS,
        "source window: {limited}"
    );
    assert!(tenants_named(&superuser, "f5").await.is_empty());
    let (status, admitted, _) = complete_signup(&srv, &proofs[4], "f5", home).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "another source is admitted: {admitted}"
    );

    // 10. Unavailable admission state refuses creation.
    let gil = prove_signup(&srv, &google, "google-gil").await;
    sqlx::query("ALTER TABLE platform.signup_admissions RENAME TO signup_admissions_down")
        .execute(&superuser)
        .await
        .expect("admission state goes away");
    let (status, down, _) = complete_signup(&srv, &gil, "gil", home).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{down}");
    assert!(tenants_named(&superuser, "gil").await.is_empty());
    sqlx::query("ALTER TABLE platform.signup_admissions_down RENAME TO signup_admissions")
        .execute(&superuser)
        .await
        .expect("admission state returns");
    let (status, created, _) = complete_signup(&srv, &gil, "gil", home).await;
    assert_eq!(status, StatusCode::OK, "{created}");
}

/// How many platform principals exist.
async fn platform_principals(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM platform.principals")
        .fetch_one(pool)
        .await
        .expect("platform principals count")
}
