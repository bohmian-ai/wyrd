use axum::body::to_bytes;
use axum::http::{Request, StatusCode};
use chrono::Duration;
use object_store::local::LocalFileSystem;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::collections::HashMap;
use std::sync::Arc;
use tower::ServiceExt;
use wyrd_auth_issue::IssuingKey;
use wyrd_auth_verify::{
    Kid, PrincipalKindWire, TokenPrincipalRef, TokenVerifier, WyrdAuthVerifySettings,
    public_key_from_pem,
};
use wyrd_runtime::PrincipalId;
use wyrd_server::{AppState, build_router};
use wyrd_spec::DataTenantId;
use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
const PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n";

#[tokio::test]
async fn healthz_returns_ok_and_request_id() {
    let response = build_router(test_state())
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(axum::body::Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().contains_key("x-request-id"));
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body collects");
    assert_eq!(&body[..], b"ok");
}

#[tokio::test]
async fn unauthenticated_v1_request_returns_problem_json() {
    let response = build_router(test_state())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/cards/upload/init")
                .body(axum::body::Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/problem+json")
    );
    assert!(response.headers().contains_key("x-request-id"));

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body collects");
    let problem: serde_json::Value = serde_json::from_slice(&body).expect("problem JSON");
    assert_eq!(problem["code"], "WYRD_AUTH_401_UNAUTHENTICATED");
    assert_eq!(problem["status"], 401);
}

#[tokio::test]
async fn request_with_real_jwt_passes_extractor() {
    let tenant = DataTenantId::new_v7();
    let state = test_state();
    let token = mint_test_user_jwt(&state, tenant);
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/cards/upload/init")
                .header("x-wyrd-access-token", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::json!({
                        "card_uid": "018f0000-0000-7000-8000-000000000001",
                        "relative_path": "model.bin",
                        "expected_sha256": "abc",
                        "expected_size_bytes": 1
                    })
                    .to_string(),
                ))
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_ne!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn request_with_missing_header_returns_401_unauthenticated() {
    let response = build_router(test_state())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/cards/upload/init")
                .body(axum::body::Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body collects");
    let problem: serde_json::Value = serde_json::from_slice(&body).expect("problem JSON");
    assert_eq!(problem["code"], "WYRD_AUTH_401_UNAUTHENTICATED");
}

fn test_state() -> AppState {
    let app_pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
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
                app_pool.clone(),
            )),
        ),
        WyrdAuthVerifySettings::default(),
    ));

    AppState::new(
        app_pool,
        None,
        Arc::new(StorageHandle::new(
            BackendSigner::Local(signer),
            object_store,
        )),
    )
    .with_auth_handles(issuing_key, verifier)
}

fn mint_test_user_jwt(state: &AppState, tenant: DataTenantId) -> String {
    let principal = TokenPrincipalRef {
        id: PrincipalId::new(uuid::Uuid::now_v7()),
        kind: PrincipalKindWire::User,
        tenant_id: tenant,
        card_ref: None,
    };
    state
        .issuing_key
        .as_ref()
        .expect("test state has issuing key")
        .issue_user_access_token(principal, vec![], Duration::minutes(5))
        .expect("test jwt mints")
}
