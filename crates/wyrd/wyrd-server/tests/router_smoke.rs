use arc_swap::ArcSwap;
use axum::body::to_bytes;
use axum::http::{Request, StatusCode};
use chrono::Duration;
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
use wyrd_server::health::{ProbeOutcome, ProbeReason, ReadinessSnapshot};
use wyrd_server::{AppState, build_router};
use wyrd_spec::DataTenantId;
use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
const PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n";

#[tokio::test]
async fn healthz_returns_ok_without_request_id() {
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
    assert!(!response.headers().contains_key("wyrd-request-id"));
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
    assert!(response.headers().contains_key("wyrd-request-id"));

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

#[tokio::test]
async fn auth_routes_are_reachable_without_credential_and_carry_request_id() {
    // Guards two invariants: (1) /auth/token is NOT behind the default-deny layer
    // (a bootstrap deadlock would prevent any client from ever obtaining a token),
    // and (2) auth routes still carry wyrd-request-id from attach_request_id.
    let response = build_router(test_state())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/token")
                .header("content-type", "application/json")
                .body(axum::body::Body::from("{}"))
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_ne!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "/auth/token must be reachable without a credential (public route)"
    );
    assert!(
        response.headers().contains_key("wyrd-request-id"),
        "auth routes sit in the protected router and must carry wyrd-request-id"
    );
}

#[tokio::test]
async fn v1_request_without_verifier_configured_returns_503() {
    // When AppState has no token_verifier (auth backend not yet configured), every
    // /v1 request must return 503 WYRD_AUTH_503_VERIFY_UNAVAILABLE — not 401 or
    // 500. This locks the third arm of the 400/401/503 auth-error contract and the
    // retry_after_seconds hint that agents use for backoff.
    let tenant = DataTenantId::new_v7();
    let token = mint_test_user_jwt(&test_state(), tenant);

    let response = build_router(test_state_no_verifier())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/cards/upload/init")
                .header("x-wyrd-access-token", format!("Bearer {token}"))
                .body(axum::body::Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body collects");
    let problem: serde_json::Value = serde_json::from_slice(&body).expect("problem JSON");
    assert_eq!(problem["code"], "WYRD_AUTH_503_VERIFY_UNAVAILABLE");
    assert_eq!(problem["status"], 503);
}

#[tokio::test]
async fn healthz_returns_ok_without_problem_json() {
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
    let content_type = response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        !content_type.contains("problem"),
        "/healthz must not return problem+json"
    );
}

#[tokio::test]
async fn readyz_returns_json_even_on_cold_boot() {
    // On cold boot ReadinessSnapshot::initial() is all-warmup, so /readyz returns 503.
    // The body must be application/problem+json with the correct code.
    let response = build_router(test_state())
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(axum::body::Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("application/problem+json")
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body collects");
    let problem: serde_json::Value = serde_json::from_slice(&body).expect("problem JSON");
    assert_eq!(problem["code"], "WYRD_SERVER_503_NOT_READY");
    assert_eq!(problem["status"], 503);
}

#[tokio::test]
async fn readyz_returns_ok_when_all_probes_pass() {
    // Inject an all-ok snapshot and assert /readyz returns 200 with status=ok.
    // Guards the success path so a wrong status code or serialization regression
    // would block Kubernetes readiness without anyone noticing.
    let ok_probe = ProbeOutcome {
        ok: true,
        reason: ProbeReason::Ok,
        elapsed_ms: 1,
    };
    let snapshot = Arc::new(ArcSwap::from_pointee(ReadinessSnapshot {
        postgres: ok_probe.clone(),
        storage: ok_probe,
    }));
    let state = test_state().with_readiness(snapshot);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(axum::body::Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body collects");
    let json: serde_json::Value = serde_json::from_slice(&body).expect("JSON");
    assert_eq!(json["status"], "ok");
}

#[tokio::test]
async fn oversized_body_returns_413_problem_json() {
    use wyrd_server::state::LimitsConfig;

    // Build state with a 10-byte body limit.
    let state = test_state().with_limits(LimitsConfig {
        body_bytes: 10,
        timeout: std::time::Duration::from_secs(30),
        concurrency: 1024,
    });

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/cards/upload/init")
                .header("content-type", "application/json")
                .header("x-wyrd-access-token", "Bearer invalid.token.here")
                .body(axum::body::Body::from("a".repeat(100)))
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(
        response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("application/problem+json")
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body collects");
    let problem: serde_json::Value = serde_json::from_slice(&body).expect("problem JSON");
    assert_eq!(problem["code"], "WYRD_SPEC_413_PAYLOAD_TOO_LARGE");
}

#[tokio::test]
async fn unknown_v1_path_without_token_returns_401_via_layer() {
    // The default-deny layer wraps the /v1 fallback, so an unknown /v1 path with
    // no token is rejected with 401 before v1_not_found runs — not 404. This is
    // the "no route-existence oracle" guarantee: unauthenticated callers cannot
    // distinguish a real route from a missing one.
    let response = build_router(test_state())
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/this/route/does/not/exist")
                .body(axum::body::Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn representative_v1_routes_without_token_all_return_401() {
    // Sweep one real route from each mounted group (storage, authz, admin). With
    // no token every one must be rejected by the layer with 401, proving auth is
    // a property of the whole /v1 nest rather than any single handler. Assertions
    // key on status 401 so they are robust to the exact error code family.
    let routes = [
        ("POST", "/v1/cards/upload/init"),
        ("POST", "/v1/cards/download/init"),
        ("POST", "/v1/authz/check"),
        (
            "POST",
            "/v1/principals/018f0000-0000-7000-8000-000000000001/revoke",
        ),
        ("GET", "/v1/admin/trusted-issuers"),
        ("GET", "/v1/admin/workload-bindings"),
    ];

    for (method, path) in routes {
        let response = build_router(test_state())
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .body(axum::body::Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("router responds");

        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path} must be rejected with 401 by the default-deny layer"
        );
    }
}

#[tokio::test]
async fn valid_token_is_not_rejected_by_default_deny_layer() {
    // A minted valid token on a real /v1 route must pass the layer. The handler
    // may still fail downstream (no live DB), but the layer itself must not 401.
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

fn test_state() -> AppState {
    let app_pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
    let root = tempfile::tempdir().expect("temp dir");
    let signer = LocalSigner::new(root.path().to_path_buf()).expect("local signer");
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
        Arc::new(StorageHandle::new(BackendSigner::Local(signer))),
    )
    .with_auth_handles(issuing_key, verifier)
}

fn test_state_no_verifier() -> AppState {
    let app_pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
    let root = tempfile::tempdir().expect("temp dir");
    let signer = LocalSigner::new(root.path().to_path_buf()).expect("local signer");
    AppState::new(
        app_pool,
        None,
        Arc::new(StorageHandle::new(BackendSigner::Local(signer))),
    )
    // no .with_auth_handles() → token_verifier is None → 503 on any /v1 request
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
