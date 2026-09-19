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

use std::env;

use chrono::{DateTime, Utc};

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, Response, StatusCode, header};
use serde_json::{Value, json};
use wyrd_server::boot::init::{InitError, initialize_platform_root};
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
    let root = initialize_platform_root(&srv.operator_pool())
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

    initialize_platform_root(&srv.operator_pool())
        .await
        .expect("first initialization succeeds");

    let second = initialize_platform_root(&srv.operator_pool()).await;

    assert!(
        matches!(second, Err(InitError::AlreadyInitialized)),
        "a second initialization refuses instead of creating another root"
    );
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
    let root = initialize_platform_root(&srv.operator_pool())
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
/// The platform plane has no revocation epoch; its answer is that every request
/// re-reads the minting credential. That claim is load-bearing, and only a real
/// request against a real route can show it holds.
#[tokio::test]
async fn revoking_a_platform_credential_ends_its_live_sessions() {
    if !e2e_enabled() {
        return;
    }
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("server starts");
    let root = initialize_platform_root(&srv.operator_pool())
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
    let root = initialize_platform_root(&srv.operator_pool())
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
    let root = initialize_platform_root(&srv.operator_pool())
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
/// old credential stops working only when the operator says so.
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

    // A token minted by the credential about to be revoked. The property that
    // matters when a credential leaks is not that it can mint no *new* token —
    // it is that what it already minted stops working.
    //
    // This token is deliberately not exercised before the revocation. The
    // verifier caches a principal's revocation epoch for five seconds, so a
    // request made now would seed that cache and the assertion after the
    // revocation would pass on stale state — proving nothing. Leaving it unused
    // means the first request it makes is a cache miss that reads the epoch
    // fresh, which is the state a real caller is in.
    let live_token = tenant_token(&srv, &first)
        .await
        .expect("the original credential still mints a token");

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
    assert_ne!(
        resp.status(),
        StatusCode::OK,
        "a token the revoked credential already minted stops authorizing"
    );
    assert!(
        tenant_token(&srv, &second).await.is_ok(),
        "the principal's other credential still authenticates, so rotation has no gap"
    );
}

/// Replaying a revoke changes nothing.
///
/// Advancing the revocation epoch is not idempotent in effect: it kills every
/// live token the principal holds. A retried DELETE — an SDK retry after a
/// timed-out 204, a re-run pipeline, a second operator — must therefore not
/// perform it again, or an already-completed rotation would lose the surviving
/// credential's live tokens for no reason.
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

    // The epoch itself is the assertion. A live token would be a weaker and
    // flakier proxy: JWT `iat` is whole seconds, so a token minted in the same
    // second as an epoch advance is already below it and would fail for a
    // reason that has nothing to do with the replay.
    let epoch_after_first = revocation_epoch(&srv, principal_id).await;

    let resp = srv
        .oneshot_authenticated(&admin, revoke(first_id))
        .await
        .expect("revoke route responds");
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "a replayed revoke reports that there was nothing left to retire"
    );

    assert_eq!(
        revocation_epoch(&srv, principal_id).await,
        epoch_after_first,
        "the replay advanced no epoch, so the surviving credential's live tokens are untouched"
    );

    // And the surviving credential is still a working way in.
    assert!(
        tenant_token(&srv, &second).await.is_ok(),
        "the surviving credential still authenticates after the replay"
    );
}

/// Read a principal's revocation epoch through the operator boundary.
///
/// Goes around row-level security deliberately: the test is asserting on
/// server-owned state that no tenant-plane route exposes.
async fn revocation_epoch(srv: &WyrdTestServer, principal_id: &str) -> Option<DateTime<Utc>> {
    sqlx::query_scalar("SELECT tokens_not_before FROM wyrd.auth_service_accounts WHERE id = $1")
        .bind(principal_id.parse::<uuid::Uuid>().expect("principal uuid"))
        .fetch_one(srv.operator_pool().pool())
        .await
        .expect("revocation epoch reads")
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
    let root = initialize_platform_root(&srv.operator_pool())
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
    let root = initialize_platform_root(&srv.operator_pool())
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
    let root = initialize_platform_root(&srv.operator_pool())
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
    let root = initialize_platform_root(&srv.operator_pool())
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
    let root = initialize_platform_root(&srv.operator_pool())
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
    let root = initialize_platform_root(&srv.operator_pool())
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
    let root = initialize_platform_root(&srv.operator_pool())
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
    let root = initialize_platform_root(&srv.operator_pool())
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
    let root = initialize_platform_root(&srv.operator_pool())
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

    // Minted now and deliberately unexercised: the verifier caches a
    // principal's revocation epoch, so a request before the revocation would
    // seed that cache and make the assertion after it pass on stale state.
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
    assert_ne!(
        resp.status(),
        StatusCode::OK,
        "a token minted before the revocation stops authorizing"
    );
    // Revocation advances the principal's authorization epoch; it does not
    // retire the credential, which stays the administrator's call.

    // Two decisions, not three: the secret-like reason never reached the
    // authorization boundary, while the wrong-kind attempt did and its allowed
    // decision is durable even though the revocation behind it found nothing.
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
        "only the attempts that were authorized are recorded: {decisions:?}"
    );

    let detail = decisions.last().expect("the accepted decision is last");
    assert_eq!(detail["kind"], "principal_revocation");
    assert_eq!(detail["reason"], reason);
    assert_eq!(detail["principal_kind"], "service");
    assert_eq!(detail["principal_id"], principal_id);
    assert_eq!(
        decisions[0]["principal_kind"], "user",
        "the refused attempt recorded the kind its caller declared: {decisions:?}"
    );
}
