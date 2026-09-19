//! Route-level contract for `POST /v1/authz/check`.
//!
//! Every case drives [`WyrdTestServer`], so the router, edge stack, and authz
//! components under assertion are the composed production ones. The suite locks
//! the delegation gate (which token kinds the route accepts and the machine-
//! readable `reason` it returns when it refuses), the policy-hook denial path,
//! the header-independent permission check, and unknown-action validation.

use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use chrono::Duration;
use wyrd_auth_check::DenyAllPolicyHook;
use wyrd_auth_issue::DelegationCaller;
use wyrd_auth_verify::{ActClaim, TokenPrincipalRef};
use wyrd_runtime::{PrincipalId, RoleRef};
use wyrd_semver::VersionBlock;
use wyrd_server::AppState;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PrincipalKindTag;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_testing::WyrdTestServer;

/// A user access token is not a delegated token, so the route must refuse it on
/// principal kind alone — before any permission lookup.
#[tokio::test]
async fn authz_user_jwt_returns_403_kind_not_eligible() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let token = mint_user_jwt(server.state(), server.data_tenant_id());

    let (status, problem) = post_authz(&server, &token, true).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(problem["code"], "WYRD_AUTHZ_403_REQUIRES_DELEGATED_TOKEN");
    assert_eq!(problem["details"]["reason"], "kind_not_eligible");

    server.shutdown().await.expect("server shuts down");
}

/// A service token minted without a delegation chain carries no caller, so the
/// route must refuse it with the distinct `chain_empty` reason rather than
/// collapsing both refusals into one opaque 403.
#[tokio::test]
async fn authz_direct_service_jwt_returns_403_chain_empty() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let token = mint_service_jwt(server.state(), server.data_tenant_id(), "callee");

    let (status, problem) = post_authz(&server, &token, true).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(problem["code"], "WYRD_AUTHZ_403_REQUIRES_DELEGATED_TOKEN");
    assert_eq!(problem["details"]["reason"], "chain_empty");

    server.shutdown().await.expect("server shuts down");
}

/// The full production path: two bootstrapped services, an API-key exchange, a
/// real delegation, and an allow decision carrying a request id.
#[tokio::test]
async fn authz_delegated_token_allows() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let caller = server
        .bootstrap_service("route-caller", &["runtime_admin"])
        .await
        .expect("caller bootstraps");
    let callee = server
        .bootstrap_service("route-callee", &["writer"])
        .await
        .expect("callee bootstraps");
    let caller_jwt = server
        .exchange_api_key(caller.api_key().expect("machine has key"))
        .await
        .expect("caller key exchanges");
    let delegated = server
        .delegate(
            &caller_jwt,
            callee.card_ref().expect("machine has card ref"),
        )
        .await
        .expect("delegates");

    let response = server
        .oneshot_authenticated(
            &delegated,
            authz_request_without_token(callee.card_ref().expect("machine has card ref"), true),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().contains_key("wyrd-request-id"));
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body collects");
    let value: serde_json::Value = serde_json::from_slice(&body).expect("json parses");
    assert_eq!(value["decision"], "allow");

    server.shutdown().await.expect("server shuts down");
}

/// A deploy-time policy hook can veto an otherwise well-formed delegated call,
/// and its reason must reach the caller intact for operator diagnosis.
#[tokio::test]
async fn authz_deny_hook_returns_403_with_reason() {
    let server = WyrdTestServer::builder()
        .with_policy_hook(Arc::new(DenyAllPolicyHook {
            reason: "test-deny".to_owned(),
        }))
        .start_in_process()
        .await
        .expect("test server starts");
    let token = mint_delegated_service_jwt(server.state(), server.data_tenant_id());

    let (status, problem) = post_authz(&server, &token, true).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(problem["code"], "WYRD_AUTHZ_403_POLICY_DENIED");
    assert_eq!(problem["details"]["reason"], "test-deny");

    server.shutdown().await.expect("server shuts down");
}

/// `x-original-method` is a hint, not the authority. With the header absent the
/// route must still evaluate the action named in the body and return a normal
/// `deny` decision rather than a validation error.
#[tokio::test]
async fn authz_missing_x_original_method_still_uses_body_check() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let token = mint_delegated_service_jwt(server.state(), server.data_tenant_id());

    let (status, body) = post_authz(&server, &token, false).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["decision"], "deny");
    assert_eq!(body["reason"], "missing_permission");

    server.shutdown().await.expect("server shuts down");
}

/// An unrecognised action is a caller mistake, not a denial. The route must
/// answer 400 and name both the rejected action and valid examples so an agent
/// can self-correct without reading documentation.
#[tokio::test]
async fn authz_unknown_action_returns_validation_error() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let token = mint_delegated_service_jwt(server.state(), server.data_tenant_id());

    let response = server
        .oneshot(authz_request_with_action(&token, "not_a_real_action"))
        .await
        .expect("router responds");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body collects");
    let value: serde_json::Value = serde_json::from_slice(&body).expect("json parses");

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(value["code"], "WYRD_SPEC_400_VALIDATION");
    assert_eq!(value["details"]["action"], "not_a_real_action");
    assert!(value["details"]["example_actions"].is_array());

    server.shutdown().await.expect("server shuts down");
}

/// Post a `card_write` check with the given token and collect the status plus
/// the parsed response or problem body.
///
/// # Panics
/// Panics when the router errors or the response body is not valid JSON.
async fn post_authz(
    server: &WyrdTestServer,
    token: &str,
    include_method: bool,
) -> (StatusCode, serde_json::Value) {
    let response = server
        .oneshot(authz_request(token, include_method))
        .await
        .expect("router responds");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body collects");
    let value = serde_json::from_slice(&body).expect("problem json parses");
    (status, value)
}

/// Build an authz-check request carrying an explicit bearer token, optionally
/// omitting `x-original-method` to exercise the header-independent path.
fn authz_request(token: &str, include_method: bool) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/v1/authz/check")
        .header("x-wyrd-access-token", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .header("x-original-path", "/invoke")
        .header("x-original-host", "service.wyrd");
    if include_method {
        builder = builder.header("x-original-method", "POST");
    }
    builder
        .body(Body::from(
            serde_json::to_vec(&authz_body(&card_ref(CardKind::Service, "callee")))
                .expect("body serializes"),
        ))
        .expect("request builds")
}

/// Build an authz-check request for the supplied target with no bearer token,
/// so the caller can attach one through `oneshot_authenticated`.
fn authz_request_without_token(target: &CardRef, include_method: bool) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/v1/authz/check")
        .header("content-type", "application/json")
        .header("x-original-path", "/invoke")
        .header("x-original-host", "service.wyrd");
    if include_method {
        builder = builder.header("x-original-method", "POST");
    }
    builder
        .body(Body::from(
            serde_json::to_vec(&authz_body(target)).expect("body serializes"),
        ))
        .expect("request builds")
}

/// Build an authz-check request naming an arbitrary action string, used to
/// drive the unknown-action validation branch.
fn authz_request_with_action(token: &str, action: &str) -> Request<Body> {
    let body = serde_json::json!({
        "target": card_ref(CardKind::Service, "callee"),
        "action": action,
        "context": {},
    });
    Request::builder()
        .method("POST")
        .uri("/v1/authz/check")
        .header("x-wyrd-access-token", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .header("x-original-method", "POST")
        .header("x-original-path", "/invoke")
        .header("x-original-host", "service.wyrd")
        .body(Body::from(
            serde_json::to_vec(&body).expect("body serializes"),
        ))
        .expect("request builds")
}

/// The canonical `card_write` check body against one target card.
fn authz_body(target: &CardRef) -> serde_json::Value {
    serde_json::json!({
        "target": target,
        "action": "card_write",
        "context": {},
    })
}

/// Mint a user access token from the composed server's issuing key.
///
/// # Panics
/// Panics when the composed state has no issuing key or minting fails.
fn mint_user_jwt(state: &AppState, tenant: DataTenantId) -> String {
    state
        .auth
        .issuing_key
        .as_ref()
        .expect("composed server carries an issuing key")
        .issue_user_access_token(
            TokenPrincipalRef {
                id: PrincipalId::new(uuid::Uuid::now_v7()),
                kind: PrincipalKindTag::User,
                tenant_id: tenant,
                card_ref: None,
                card_ref_scope: wyrd_spec::reference::CardRefScope::default(),
            },
            Vec::new(),
            Duration::minutes(5),
        )
        .expect("user jwt mints")
}

/// Mint a direct (undelegated) service access token from the composed server's
/// issuing key, used to prove the route refuses an empty delegation chain.
///
/// # Panics
/// Panics when the composed state has no issuing key or minting fails.
fn mint_service_jwt(state: &AppState, tenant: DataTenantId, name: &str) -> String {
    state
        .auth
        .issuing_key
        .as_ref()
        .expect("composed server carries an issuing key")
        .issue_card_access_token(
            wyrd_auth_verify::TokenPrincipalRef {
                id: PrincipalId::new(uuid::Uuid::now_v7()),
                kind: wyrd_spec::auth::PrincipalKindTag::Service,
                tenant_id: tenant,
                card_ref: Some(card_ref(CardKind::Service, name)),
                card_ref_scope: wyrd_spec::reference::CardRefScope::default(),
            },
            Vec::new(),
            None,
            Duration::minutes(5),
        )
        .expect("service jwt mints")
}

/// Mint a delegated service token whose chain names a distinct caller and
/// callee, so the route's delegation gate admits it and evaluation proceeds to
/// the policy hook and permission check.
///
/// # Panics
/// Panics when the composed state has no issuing key or minting fails.
fn mint_delegated_service_jwt(state: &AppState, tenant: DataTenantId) -> String {
    let caller_ref = card_ref(CardKind::Service, "caller");
    let caller = TokenPrincipalRef {
        id: PrincipalId::new(uuid::Uuid::now_v7()),
        kind: PrincipalKindTag::Service,
        tenant_id: tenant,
        card_ref: Some(caller_ref.clone()),
        card_ref_scope: wyrd_spec::reference::CardRefScope::own(&caller_ref),
    };
    let requested_ref = card_ref(CardKind::Service, "callee");
    let requested = TokenPrincipalRef {
        id: PrincipalId::new(uuid::Uuid::now_v7()),
        kind: PrincipalKindTag::Service,
        tenant_id: tenant,
        card_ref: Some(requested_ref.clone()),
        card_ref_scope: wyrd_spec::reference::CardRefScope::own(&requested_ref),
    };

    state
        .auth
        .issuing_key
        .as_ref()
        .expect("composed server carries an issuing key")
        .issue_delegated_access_token(
            &DelegationCaller {
                sub: caller.id.to_string(),
                principal: caller,
                act: None::<Box<ActClaim>>,
            },
            requested,
            Vec::<RoleRef>::new(),
            Duration::minutes(5),
        )
        .expect("delegated jwt mints")
}

/// Build a fixed `prod` card reference for the supplied kind and name.
///
/// # Panics
/// Panics if the static name, version, or space literals stop being valid.
fn card_ref(kind: CardKind, name: &str) -> CardRef {
    CardRef {
        kind,
        name: CardName::new(name).expect("static card name is valid"),
        version: VersionBlock::parse("1.0.0").expect("static version is valid"),
        space: Some(SpaceName::new("prod").expect("static space is valid")),
        uid: None,
    }
}
