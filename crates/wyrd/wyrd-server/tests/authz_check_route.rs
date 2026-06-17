use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use chrono::Duration;
use object_store::local::LocalFileSystem;
use sqlx::postgres::PgPoolOptions;
use std::collections::HashMap;
use std::sync::Arc;
use tower::ServiceExt;
use wyrd_auth_check::DenyAllPolicyHook;
use wyrd_auth_issue::{DelegationCaller, IssuingKey};
use wyrd_auth_verify::{
    ActClaim, Kid, PrincipalKindWire, TokenPrincipalRef, TokenVerifier, WyrdAuthVerifySettings,
    public_key_from_pem,
};
use wyrd_runtime::{PrincipalId, RoleRef};
use wyrd_semver::VersionBlock;
use wyrd_server::{AppState, build_router};
use wyrd_spec::DataTenantId;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};
use wyrd_testing::env::WyrdTestEnv;

const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
const PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n";

#[tokio::test]
async fn authz_user_jwt_returns_403_kind_not_eligible() {
    let fixture = wyrd_dev_fixtures::pg::PgFixture::start()
        .await
        .expect("fixture starts");
    let state = test_state(fixture.app_pool().clone());
    let token = mint_user_jwt(&state, fixture.data_tenant_id());

    let problem = post_authz(state, &token, true).await;

    assert_eq!(problem.0, StatusCode::FORBIDDEN);
    assert_eq!(problem.1["code"], "WYRD_AUTHZ_403_REQUIRES_DELEGATED_TOKEN");
    assert_eq!(problem.1["details"]["reason"], "kind_not_eligible");
}

#[tokio::test]
async fn authz_direct_service_jwt_returns_403_chain_empty() {
    let fixture = wyrd_dev_fixtures::pg::PgFixture::start()
        .await
        .expect("fixture starts");
    let state = test_state(fixture.app_pool().clone());
    let token = mint_service_jwt(&state, fixture.data_tenant_id(), "callee");

    let problem = post_authz(state, &token, true).await;

    assert_eq!(problem.0, StatusCode::FORBIDDEN);
    assert_eq!(problem.1["code"], "WYRD_AUTHZ_403_REQUIRES_DELEGATED_TOKEN");
    assert_eq!(problem.1["details"]["reason"], "chain_empty");
}

#[tokio::test]
async fn authz_delegated_token_allows() {
    let env = WyrdTestEnv::start().await.expect("env starts");
    let caller = env
        .bootstrap_service("route-caller", &["runtime_admin"])
        .await
        .expect("caller bootstraps");
    let callee = env
        .bootstrap_service("route-callee", &["writer"])
        .await
        .expect("callee bootstraps");
    let caller_jwt = env
        .exchange_api_key(caller.api_key().expect("machine has key"))
        .await
        .expect("caller key exchanges");
    let delegated = env
        .delegate(
            &caller_jwt,
            callee.card_ref().expect("machine has card ref"),
        )
        .await
        .expect("delegates");

    let response = env
        .call(
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
}

#[tokio::test]
async fn authz_deny_hook_returns_403_with_reason() {
    let fixture = wyrd_dev_fixtures::pg::PgFixture::start()
        .await
        .expect("fixture starts");
    let mut state = test_state(fixture.app_pool().clone());
    state.policy_hook = Arc::new(DenyAllPolicyHook {
        reason: "test-deny".to_owned(),
    });
    let token = mint_delegated_service_jwt(&state, fixture.data_tenant_id());

    let problem = post_authz(state, &token, true).await;

    assert_eq!(problem.0, StatusCode::FORBIDDEN);
    assert_eq!(problem.1["code"], "WYRD_AUTHZ_403_POLICY_DENIED");
    assert_eq!(problem.1["details"]["reason"], "test-deny");
}

#[tokio::test]
async fn authz_missing_x_original_method_still_uses_body_check() {
    let fixture = wyrd_dev_fixtures::pg::PgFixture::start()
        .await
        .expect("fixture starts");
    let state = test_state(fixture.app_pool().clone());
    let token = mint_delegated_service_jwt(&state, fixture.data_tenant_id());

    let problem = post_authz(state, &token, false).await;

    assert_eq!(problem.0, StatusCode::OK);
    assert_eq!(problem.1["decision"], "deny");
    assert_eq!(problem.1["reason"], "missing_permission");
}

async fn post_authz(
    state: AppState,
    token: &str,
    include_method: bool,
) -> (StatusCode, serde_json::Value) {
    let response = build_router(state)
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

fn authz_body(target: &CardRef) -> serde_json::Value {
    serde_json::json!({
        "target": target,
        "action": "card_write",
        "context": {},
    })
}

fn test_state(pool: sqlx::PgPool) -> AppState {
    let root = tempfile::tempdir().expect("temp dir");
    let signer = LocalSigner::new(root.path().to_path_buf()).expect("local signer");
    let object_store =
        Arc::new(LocalFileSystem::new_with_prefix(root.path()).expect("local object store"));
    let issuing_key = Arc::new(
        IssuingKey::from_ed_pem(
            secrecy::SecretString::from(PRIVATE_KEY_PEM),
            Kid::new("k1").expect("kid is valid"),
            "wyrd",
        )
        .expect("test issuing key loads"),
    );
    let mut keys = HashMap::new();
    keys.insert(
        Kid::new("k1").expect("kid is valid"),
        Arc::new(public_key_from_pem(PUBLIC_KEY_PEM).expect("public key loads")),
    );
    let verifier = Arc::new(TokenVerifier::new(
        keys,
        "wyrd",
        Arc::new(
            wyrd_server::auth::permission_resolver::SqlPermissionResolver::new(Arc::new(
                pool.clone(),
            )),
        ),
        WyrdAuthVerifySettings::default(),
    ));

    AppState::new(
        pool,
        None,
        Arc::new(StorageHandle::new(
            BackendSigner::Local(signer),
            object_store,
        )),
    )
    .with_auth_handles(issuing_key, verifier)
}

fn mint_user_jwt(state: &AppState, tenant: DataTenantId) -> String {
    state
        .issuing_key
        .as_ref()
        .expect("issuing key exists")
        .issue_user_access_token(
            TokenPrincipalRef {
                id: PrincipalId::new(uuid::Uuid::now_v7()),
                kind: PrincipalKindWire::User,
                tenant_id: tenant,
                card_ref: None,
            },
            Vec::new(),
            Duration::minutes(5),
        )
        .expect("user jwt mints")
}

fn mint_service_jwt(state: &AppState, tenant: DataTenantId, name: &str) -> String {
    state
        .issuing_key
        .as_ref()
        .expect("issuing key exists")
        .issue_service_access_token(
            PrincipalId::new(uuid::Uuid::now_v7()),
            tenant,
            card_ref(CardKind::Service, name),
            Vec::new(),
            Duration::minutes(5),
        )
        .expect("service jwt mints")
}

fn mint_delegated_service_jwt(state: &AppState, tenant: DataTenantId) -> String {
    let caller = TokenPrincipalRef {
        id: PrincipalId::new(uuid::Uuid::now_v7()),
        kind: PrincipalKindWire::Service,
        tenant_id: tenant,
        card_ref: Some(card_ref(CardKind::Service, "caller")),
    };
    let requested = TokenPrincipalRef {
        id: PrincipalId::new(uuid::Uuid::now_v7()),
        kind: PrincipalKindWire::Service,
        tenant_id: tenant,
        card_ref: Some(card_ref(CardKind::Service, "callee")),
    };

    state
        .issuing_key
        .as_ref()
        .expect("issuing key exists")
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

fn card_ref(kind: CardKind, name: &str) -> CardRef {
    CardRef {
        kind,
        name: CardName::new(name).expect("static card name is valid"),
        version: VersionBlock::parse("1.0.0").expect("static version is valid"),
        space: SpaceName::new("prod").expect("static space is valid"),
        uid: None,
    }
}

#[allow(dead_code)]
fn lazy_pool_state() -> AppState {
    test_state(PgPoolOptions::new().connect_lazy_with(Default::default()))
}
