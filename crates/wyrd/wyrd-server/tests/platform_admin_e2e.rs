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

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, Response, StatusCode, header};
use serde_json::{Value, json};
use wyrd_server::boot::init::{InitError, initialize_platform_root};
use wyrd_testing::WyrdTestServer;

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
        .header(header::AUTHORIZATION, format!("Bearer {session}"))
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
            builder = builder.header(header::AUTHORIZATION, value);
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
                .header(header::AUTHORIZATION, format!("Bearer {session}"))
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
        .header(header::AUTHORIZATION, format!("Bearer {session}"));
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
                "issuer_url": "https://idp.example.com/realms/platform",
                "jwks_uri": "https://idp.example.com/realms/platform/protocol/openid-connect/certs",
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
    assert_eq!(
        view["issuer_url"],
        "https://idp.example.com/realms/platform"
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
                "jwks_uri": "https://attacker.example.com/certs",
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
