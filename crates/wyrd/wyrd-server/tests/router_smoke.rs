use axum::body::to_bytes;
use axum::http::{Request, StatusCode};
use object_store::local::LocalFileSystem;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::sync::Arc;
use tower::ServiceExt;
use wyrd_server::{AppState, build_router};
use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

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

fn test_state() -> AppState {
    let app_pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
    let root = tempfile::tempdir().expect("temp dir");
    let signer = LocalSigner::new(root.path().to_path_buf()).expect("local signer");
    let object_store =
        Arc::new(LocalFileSystem::new_with_prefix(root.path()).expect("local object store"));
    AppState::new(
        app_pool,
        None,
        Arc::new(StorageHandle::new(
            BackendSigner::Local(signer),
            object_store,
        )),
    )
}
