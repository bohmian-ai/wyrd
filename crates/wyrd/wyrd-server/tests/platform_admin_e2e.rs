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
//! Gated on `WYRD_AUTH_E2E` so the default lane stays server-free.

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
use wyrd_testing::WyrdTestServer;

/// The one header every Wyrd plane authenticates on.
///
/// Spelled here rather than imported because the server keeps it private: a test
/// that hard-codes it fails if the wire name ever changes, which is the point.
const WYRD_ACCESS_TOKEN_HEADER: &str = "x-wyrd-access-token";

/// Skip unless the gated end-to-end lane is selected.
fn e2e_enabled() -> bool {
    env::var("WYRD_AUTH_E2E").is_ok()
}

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

/// Exchange a platform credential for a session token.
async fn platform_session(srv: &WyrdTestServer, credential: &str) -> String {
    let resp = srv
        .oneshot(anonymous_post(
            "/auth/platform/token",
            json!({ "credential": credential }),
        ))
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
    if !e2e_enabled() {
        return;
    }
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
        .oneshot(anonymous_post(
            "/auth/token",
            json!({ "grant_type": "wyrd_api_key", "api_key": admin_credential }),
        ))
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
    if !e2e_enabled() {
        return;
    }
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
/// The binary resolves its DSNs from the environment, so the only thing the
/// child needs is the lane's `WYRD_DATABASE_URL` with the database swapped for
/// the fixture's. Every other credential and password is inherited.
///
/// # Panics
///
/// Panics when the lane did not set `WYRD_DATABASE_URL`, or when its value is
/// not a Postgres DSN naming its database after the last slash.
fn operator_command(subcommand: &str, database: &str) -> Command {
    let base = env::var("WYRD_DATABASE_URL").expect("the journey lane sets WYRD_DATABASE_URL");
    let (prefix, _) = base
        .rsplit_once('/')
        .expect("a Postgres DSN names its database after the last slash");
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_wyrd-server"));
    command
        .arg(subcommand)
        .env("WYRD_DATABASE_URL", format!("{prefix}/{database}"));
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
    if !e2e_enabled() {
        return;
    }
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
    if !e2e_enabled() {
        return;
    }
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
        .oneshot(anonymous_post(
            "/auth/platform/token",
            json!({ "credential": root }),
        ))
        .await
        .expect("token exchange responds");
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
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
                "expected_audience": "wyrd-platform",
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
    if !e2e_enabled() {
        return;
    }
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
    if !e2e_enabled() {
        return;
    }
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

/// Losing every tenant credential does not cost the tenant: the platform plane
/// issues a replacement for the same principal.
#[tokio::test]
async fn tenant_administration_survives_losing_every_credential() {
    if !e2e_enabled() {
        return;
    }
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
        .oneshot(anonymous_post(
            "/auth/token",
            json!({ "grant_type": "wyrd_api_key", "api_key": replacement }),
        ))
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
        .oneshot(anonymous_post(
            "/auth/token",
            json!({ "grant_type": "wyrd_api_key", "api_key": credential }),
        ))
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
/// old credential mints nothing once the operator retires it. A token it
/// already minted is a self-contained snapshot that lapses at its five-minute
/// expiry, while the surviving credential exchanges and spends immediately —
/// no ordering between the two tokens is involved.
#[tokio::test]
async fn a_tenant_rotates_an_automation_credential_without_an_outage() {
    if !e2e_enabled() {
        return;
    }
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
                Some(json!({ "name": "ci-runner", "roles": ["reader"] })),
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
        StatusCode::UNAUTHORIZED,
        "the retired credential mints no further token"
    );
    let resp = srv
        .oneshot_authenticated(&live_token, tenant_request(Method::GET, "/v1/cards", None))
        .await
        .expect("cards route responds");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "a token the revoked credential already minted keeps its snapshot until expiry"
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
    if !e2e_enabled() {
        return;
    }
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
                Some(json!({ "name": "ci-runner", "roles": ["reader"] })),
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
    if !e2e_enabled() {
        return;
    }
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
        StatusCode::UNAUTHORIZED,
        "revoked credential A cannot exchange again"
    );
    assert_eq!(
        cards(&token_a).await,
        StatusCode::OK,
        "A's existing token keeps its snapshot authority before expiry"
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
    if !e2e_enabled() {
        return;
    }
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
                    Some(json!({ "name": name, "roles": ["reader"] })),
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
    if !e2e_enabled() {
        return;
    }
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
                Some(json!({ "name": "ci-runner", "roles": ["reader"] })),
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
                Some(json!({ "name": "escalated", "roles": ["admin"] })),
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
    if !e2e_enabled() {
        return;
    }
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
                "expected_audience": "wyrd-platform",
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
    // recorded kind is the only thing separating a machine root's decision from
    // a registered human's. It used to be hard-coded `global_admin` for every
    // platform decision, which filed every human action under a machine.
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("superuser pool for staged audit");
    let root_id: String =
        sqlx::query_scalar("SELECT id::text FROM platform.principals WHERE name = $1")
            .bind(wyrd_server::boot::init::PLATFORM_ROOT_NAME)
            .fetch_one(&superuser)
            .await
            .expect("the root principal exists");
    for (principal, expected) in [
        (root_id.as_str(), "global_admin"),
        (
            registered["principal_id"].as_str().expect("principal id"),
            "user",
        ),
    ] {
        let kinds: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT principal_kind FROM vala.audit_staging
              WHERE operation = 'platform.authz' AND principal_id = $1::uuid",
        )
        .bind(principal)
        .fetch_all(&superuser)
        .await
        .expect("staged decisions read");

        assert_eq!(
            kinds,
            vec![expected.to_owned()],
            "decisions by {principal} were recorded as {kinds:?}, expected only {expected}"
        );
    }

    // A federated sign-in presents a provider token, not a credential, so
    // there is no credential of this principal's for a decision to name.
    let attributed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.audit_staging
          WHERE operation = 'platform.authz' AND principal_id = $1::uuid
            AND credential_id IS NOT NULL",
    )
    .bind(registered["principal_id"].as_str().expect("principal id"))
    .fetch_one(&superuser)
    .await
    .expect("attributed count reads");
    assert_eq!(
        attributed, 0,
        "a federated session names no credential, so its decisions must not claim one"
    );
}

/// A tenant administrator cannot configure who signs in to the platform.
#[tokio::test]
async fn a_tenant_administrator_cannot_configure_platform_sign_in() {
    if !e2e_enabled() {
        return;
    }
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
                "expected_audience": "wyrd-platform",
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
    if !e2e_enabled() {
        return;
    }
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
                "expected_audience": "wyrd-platform",
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
    if !e2e_enabled() {
        return;
    }
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
            "expected_audience": "wyrd-platform",
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
    if !e2e_enabled() {
        return;
    }
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
    if !e2e_enabled() {
        return;
    }
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
        StatusCode::UNAUTHORIZED,
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
    if !e2e_enabled() {
        return;
    }
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
        .await
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
    // recorded against a proposed id would name a tenant that does not exist.
    let creation_resources: Vec<String> = sqlx::query_scalar(
        "SELECT resource FROM vala.audit_staging
          WHERE operation = 'platform.authz' AND permission = 'tenants:write'
          ORDER BY resource",
    )
    .fetch_all(&superuser)
    .await
    .expect("creation decision resources read");
    assert_eq!(
        creation_resources,
        vec!["tenant_slug:retryable".to_owned(); 2],
        "every provisioning decision names the requested slug, never a proposed tenant id"
    );
}

/// An occupied slug is still a conflict.
#[tokio::test]
async fn an_active_tenant_slug_is_still_refused() {
    if !e2e_enabled() {
        return;
    }
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
const INTERNAL_FRAGMENTS: [&str; 6] = [
    "audit_staging",
    "platform.",
    "sqlx",
    "plpgsql",
    "expected value",
    "injected",
];

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

/// An injected store failure and an unreadable grant both fail safely.
///
/// These are the two shapes the platform plane can fail in that a caller has no
/// business seeing: a database error raised underneath the canonical audit
/// append, and a grant row the server itself cannot deserialize. Both used to
/// serialize their source text into `details`.
#[tokio::test]
async fn served_platform_failures_disclose_nothing_internal() {
    if !e2e_enabled() {
        return;
    }
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;

    // A fault underneath the canonical audit append, installed the same way the
    // card registration journey installs its own.
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("superuser pool opens");
    sqlx::query(
        r"CREATE OR REPLACE FUNCTION vala.test_fail_platform_authz_audit()
           RETURNS trigger LANGUAGE plpgsql AS $$
           BEGIN
             IF NEW.operation = 'platform.authz' THEN
               RAISE EXCEPTION 'injected platform authorization audit failure';
             END IF;
             RETURN NEW;
           END;
           $$;",
    )
    .execute(&superuser)
    .await
    .expect("failure function installs");
    sqlx::query(
        r"CREATE TRIGGER test_fail_platform_authz_audit
           BEFORE INSERT ON vala.audit_staging
           FOR EACH ROW EXECUTE FUNCTION vala.test_fail_platform_authz_audit()",
    )
    .execute(&superuser)
    .await
    .expect("failure trigger installs");

    let resp = srv
        .oneshot(platform_post(
            "/platform/tenants",
            &session,
            json!({ "slug": "unrecordable", "display_name": "Unrecordable" }),
        ))
        .await
        .expect("tenant route responds");
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_safe_problem(&body_json(resp).await, "WYRD_SPEC_500_INTERNAL");

    sqlx::query("DROP TRIGGER test_fail_platform_authz_audit ON vala.audit_staging")
        .execute(&superuser)
        .await
        .expect("failure trigger drops");

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

/// A tenant mutation whose decision cannot be recorded happens at all.
///
/// Audit is fail-closed, and the append shares the mutation's transaction, so
/// the guarantee is not merely that the caller sees an error: the principal
/// must not exist afterwards and no audit row may survive for it either. This
/// forces the append to fail the way the card registration journey does, at the
/// database, so nothing in the server is mocked out of the path.
#[tokio::test]
async fn an_unrecordable_tenant_mutation_leaves_nothing_behind() {
    if !e2e_enabled() {
        return;
    }
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let admin = provisioned_tenant_admin(&srv, "unrecordable-tenant").await;

    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("superuser pool opens");
    sqlx::query(
        r"CREATE OR REPLACE FUNCTION vala.test_fail_principal_create_audit()
           RETURNS trigger LANGUAGE plpgsql AS $$
           BEGIN
             IF NEW.operation = 'auth.principal.create' THEN
               RAISE EXCEPTION 'injected principal creation audit failure';
             END IF;
             RETURN NEW;
           END;
           $$;",
    )
    .execute(&superuser)
    .await
    .expect("failure function installs");
    sqlx::query(
        r"CREATE TRIGGER test_fail_principal_create_audit
           BEFORE INSERT ON vala.audit_staging
           FOR EACH ROW EXECUTE FUNCTION vala.test_fail_principal_create_audit()",
    )
    .execute(&superuser)
    .await
    .expect("failure trigger installs");

    let resp = srv
        .oneshot_authenticated(
            &admin,
            tenant_request(
                Method::POST,
                "/v1/principals",
                Some(json!({ "name": "unrecordable-runner", "roles": ["reader"] })),
            ),
        )
        .await
        .expect("principal route responds");
    let status = resp.status();
    let body = body_json(resp).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "refused: {body}");
    assert_eq!(body["code"], "WYRD_VALA_500_AUDIT_UNAVAILABLE");

    sqlx::query("DROP TRIGGER test_fail_principal_create_audit ON vala.audit_staging")
        .execute(&superuser)
        .await
        .expect("failure trigger drops");

    let principals: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wyrd.auth_service_accounts WHERE name = 'unrecordable-runner'",
    )
    .fetch_one(&superuser)
    .await
    .expect("principal count reads");
    assert_eq!(principals, 0, "the refused principal was never created");

    let audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.audit_staging WHERE operation = 'auth.principal.create'",
    )
    .fetch_one(&superuser)
    .await
    .expect("audit count reads");
    assert_eq!(audits, 0, "no allowance survives the refused mutation");
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
    if !e2e_enabled() {
        return;
    }
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
    // itself never reaches a staged row. Which decision named which credential
    // is settled deterministically in `platform_authz`, because publication
    // drains staging on its own schedule here.
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("superuser pool for staged audit");
    // What travels is the credential's id, never the credential.
    let leaked: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.audit_staging WHERE to_jsonb(audit_staging)::text LIKE $1",
    )
    .bind(format!("%{replacement}%"))
    .fetch_one(&superuser)
    .await
    .expect("staged rows scan");
    assert_eq!(leaked, 0, "no staged audit row carries credential material");
}

/// A tenant administrator ends a compromised identity outright and the reason
/// survives in the audit record.
///
/// Principal revocation is the blunt instrument next to credential rotation,
/// and the only part of it that cannot be reconstructed afterwards is why an
/// operator reached for it. The journey therefore proves three things a
/// generated client depends on: the declared kind selects the table, a reason
/// that looks like a credential is refused rather than persisted, and an
/// accepted reason is what the audit row carries.
#[tokio::test]
async fn a_tenant_revokes_a_compromised_principal_with_its_reason() {
    if !e2e_enabled() {
        return;
    }
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
                Some(json!({ "name": "leaked-runner", "roles": ["reader"] })),
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

    assert!(
        tenant_token(&srv, &credential).await.is_ok(),
        "neither refusal revoked anything"
    );

    let reason = "leaked-runner key found in a public build log";
    let resp = revoke(json!({ "principal_kind": "service", "reason": reason }))
        .await
        .expect("revoke route responds");
    assert_eq!(resp.status(), StatusCode::OK, "the revocation is accepted");

    let resp = srv
        .oneshot_authenticated(&live_token, tenant_request(Method::GET, "/v1/cards", None))
        .await
        .expect("cards route responds");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "a token minted before the revocation keeps its snapshot until expiry"
    );
    assert_eq!(
        tenant_token(&srv, &credential).await.unwrap_err(),
        StatusCode::UNAUTHORIZED,
        "the suspended principal's credential mints nothing new"
    );

    // Two decisions, not three. The secret-like reason never reached the
    // authorization boundary. The wrong-kind attempt was an authorized decision
    // that found nothing: its allowance commits with no effect before the
    // `404`, so an operator can see the attempt. The accepted revocation
    // commits its allowance together with the suspension.
    // Read past RLS on purpose: the assertion is about the durable audit row
    // the server wrote, which no tenant-plane route projects.
    let rows: Vec<Option<String>> = sqlx::query_scalar(
        "SELECT detail FROM vala.audit_staging \
          WHERE operation = 'auth.principal.revoke' AND resource = $1 \
          ORDER BY seq",
    )
    .bind(format!("principal:{principal_id}"))
    .fetch_all(
        &srv.pg_fixture()
            .superuser_pool()
            .await
            .expect("superuser pool opens"),
    )
    .await
    .expect("the revocation decisions are audited");
    // `detail` is stored as the canonical JSON string the audit hash is taken
    // over, so it is parsed here rather than decoded as JSONB.
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
    if !e2e_enabled() {
        return;
    }
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
        .await
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
                Some(json!({ "name": "post-recovery", "roles": ["reader"] })),
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
    if !e2e_enabled() {
        return;
    }
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
                Some(json!({ "name": "beta-runner", "roles": ["reader"] })),
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
                Some(json!({ "name": "escalated", "roles": ["admin"] })),
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
    if !e2e_enabled() {
        return;
    }
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
    // governs issuance, and an issued token is a self-contained snapshot.
    let live_token = tenant_token(&srv, &admin_credential)
        .await
        .expect("the tenant administers before suspension");
    let primed = srv
        .oneshot_authenticated(
            &live_token,
            tenant_request(
                Method::POST,
                "/v1/principals",
                Some(json!({ "name": "before-suspension", "roles": ["reader"] })),
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
                Some(json!({ "name": "during-suspension", "roles": ["reader"] })),
            ),
        )
        .await
        .expect("principal route responds");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "a token minted before the suspension keeps its snapshot authority \
         until its five-minute expiry; verification reads no tenant state"
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
                Some(json!({ "name": "after-resume", "roles": ["reader"] })),
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
/// The stages are the actual commit boundaries the workflow crosses: the
/// directory claim's audit row, the role seed, the administrative principal,
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
    if !e2e_enabled() {
        return;
    }
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
        .await
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
             IF TG_TABLE_NAME = 'audit_staging' THEN
                 mine := NEW.resource = 'tenant_slug:' || target;
             ELSIF TG_TABLE_NAME = 'tenants' THEN
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

    let stages: [(&str, &str, &str); 7] = [
        (
            "claim-audit",
            "CREATE TRIGGER fail_stage BEFORE INSERT ON vala.audit_staging
             FOR EACH ROW EXECUTE FUNCTION wyrd.injected_stage_failure('stage-claim-audit')",
            "DROP TRIGGER fail_stage ON vala.audit_staging",
        ),
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
            "CREATE TRIGGER fail_stage BEFORE INSERT ON wyrd.auth_service_account_roles
             FOR EACH ROW EXECUTE FUNCTION wyrd.injected_stage_failure('stage-grant')",
            "DROP TRIGGER fail_stage ON wyrd.auth_service_account_roles",
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

        let counts: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM wyrd.auth_service_accounts WHERE data_tenant_id = $1),
                    (SELECT count(*) FROM wyrd.auth_service_account_roles WHERE data_tenant_id = $1),
                    (SELECT count(*) FROM wyrd.auth_api_keys
                      WHERE data_tenant_id = $1 AND revoked_at IS NULL)",
        )
        .bind(uuid::Uuid::parse_str(tenant_id).expect("the tenant id is a uuid"))
        .fetch_one(&superuser)
        .await
        .expect("the tenant's durable rows are counted");
        assert_eq!(
            counts,
            (1, 1, 1),
            "the retry after a {stage} failure leaves one principal, grant, and live credential"
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
    if !e2e_enabled() {
        return;
    }
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
        .await
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
    let principals: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wyrd.auth_service_accounts WHERE data_tenant_id = $1::uuid",
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
    if !e2e_enabled() {
        return;
    }
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
    if !e2e_enabled() {
        return;
    }
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");

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
    if !e2e_enabled() {
        return;
    }
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .await
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
    if !e2e_enabled() {
        return;
    }
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
                Some(json!({ "name": "unaffected", "roles": ["reader"] })),
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
            .oneshot(anonymous_post(
                "/auth/platform/token",
                json!({ "credential": "wyrd_global_not_a_real_credential" }),
            ))
            .await
            .expect("platform token route responds");
        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
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

/// Count staged authorization decisions for one operation and outcome.
///
/// # Panics
///
/// Panics when the staging table cannot be read.
async fn staged_decisions(pool: &PgPool, operation: &str, outcome: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM vala.audit_staging
          WHERE operation = $1 AND outcome = $2",
    )
    .bind(operation)
    .bind(outcome)
    .fetch_one(pool)
    .await
    .expect("decision count reads")
}

/// An authorized request that changes nothing still records the decision.
///
/// Every route here evaluates the caller's permission, allows it, appends the
/// decision on that transaction, and then discovers there is nothing to do:
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
    if !e2e_enabled() {
        return;
    }
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv)
        .await
        .expect("deployment initializes");
    let secret = secrecy::ExposeSecret::expose_secret(&root).to_owned();
    let session = platform_session(&srv, &secret).await;
    // Staged decisions are read as the superuser: `vala.audit_staging` is under
    // row-level security, and this reads across the platform sentinel and the
    // provisioned tenant.
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("superuser pool for staged decisions");

    // Each case: the request, the status it must answer with, and what it is.
    // `platform.authz` is one operation, so the cases are counted one at a time
    // against the running total rather than filtered apart.
    let mut expected = staged_decisions(&superuser, "platform.authz", "allowed").await;
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
            staged_decisions(&superuser, "platform.authz", "allowed").await,
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
        staged_decisions(&superuser, "platform.authz", "allowed").await,
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
        staged_decisions(&superuser, "platform.authz", "allowed").await,
        expected,
        "a request refused on syntax opens no decision to record"
    );

    // The tenant plane has the same shape: the delete finds nothing, and the
    // decision appended on that transaction has to commit with the answer.
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
    let before = staged_decisions(&superuser, "platform.authz", "allowed").await;
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
        staged_decisions(&superuser, "platform.authz", "allowed").await,
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
                Some(json!({ "name": "roleless", "roles": ["absent_role"] })),
            ),
            StatusCode::BAD_REQUEST,
            "creating a principal with a role that does not exist",
        ),
    ] {
        let before = staged_decisions(&superuser, operation, "allowed").await;
        let resp = srv
            .oneshot_authenticated(&admin, request)
            .await
            .expect("principal route responds");
        assert_eq!(resp.status(), status, "{what} answers {status}");
        assert_eq!(
            staged_decisions(&superuser, operation, "allowed").await,
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
        let before = staged_decisions(&superuser, operation, "allowed").await;
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
            staged_decisions(&superuser, operation, "allowed").await,
            before + 1,
            "{what} records exactly one allowed decision"
        );
    }
}

/// Count staged platform authorization rows by outcome.
///
/// # Panics
///
/// Panics when the staging table cannot be read.
async fn staged_platform_decisions(pool: &PgPool, outcome: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM vala.audit_staging
          WHERE operation = 'platform.authz' AND outcome = $1",
    )
    .bind(outcome)
    .fetch_one(pool)
    .await
    .expect("decision count reads")
}

/// A failed platform mutation leaves no allowance saying it was permitted.
///
/// REQ-037 makes the decision and its effect one transaction. The failure mode
/// it exists for is durable and silent: a recorded allowance for an operation
/// that never happened is indistinguishable, to anyone auditing afterwards,
/// from one that did. Four mutation classes cover the plane's write surface —
/// the identity connection, a credential, a principal's status, and a tenant's
/// admission — and each is driven through the real route with its own table's
/// writes failing.
///
/// Denials are the control. They commit on their own by design, because a
/// refusal is durable evidence of an attempt and there is no effect to pair it
/// with.
#[tokio::test]
async fn a_failed_platform_mutation_leaves_no_allowance() {
    if !e2e_enabled() {
        return;
    }
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
        .await
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
            "expected_audience": "wyrd-platform",
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

    // Every allowance recorded so far is legitimate; the assertions below are
    // about what the *failed* attempts add.
    let baseline = staged_platform_decisions(&superuser, "allowed").await;

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
                    "expected_audience": "wyrd-platform-two",
                    "client_id": "wyrd-platform",
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
        assert_eq!(
            staged_platform_decisions(&superuser, "allowed").await,
            baseline,
            "the {label} failure rolled back its own allowance"
        );
    }

    // The effects never landed either.
    let audience: String = sqlx::query_scalar(
        "SELECT expected_audience FROM platform.oidc_connection WHERE singleton",
    )
    .fetch_one(&superuser)
    .await
    .expect("connection reads");
    assert_eq!(audience, "wyrd-platform", "the identity write rolled back");

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
    let allowances = staged_platform_decisions(&superuser, "allowed").await;
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
        staged_platform_decisions(&superuser, "allowed").await,
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
    if !e2e_enabled() {
        return;
    }
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
        srv.oneshot(anonymous_post(
            "/auth/token",
            json!({ "grant_type": "wyrd_api_key", "api_key": credential }),
        ))
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
        srv.oneshot(anonymous_post(
            "/auth/token",
            json!({ "grant_type": "wyrd_api_key", "api_key": credential }),
        ))
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
    if !e2e_enabled() {
        return;
    }
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
                "expected_audience": "wyrd-platform",
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
