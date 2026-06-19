use wyrd_testing::{WyrdTestServer, WyrdTestServerError};

fn e2e_enabled() -> bool {
    std::env::var("WYRD_AUTH_E2E").is_ok()
}

#[tokio::test]
async fn server_in_process_healthz_returns_ok() {
    if !e2e_enabled() {
        return;
    }
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("start_in_process");
    let req = axum::http::Request::builder()
        .uri("/healthz")
        .body(axum::body::Body::empty())
        .expect("request builds");
    let resp = srv.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    srv.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn server_bound_real_socket_serves_healthz() {
    if !e2e_enabled() {
        return;
    }
    let srv = WyrdTestServer::start_bound().await.expect("start_bound");
    let base_url = srv.base_url().expect("has base url").to_owned();
    let resp = reqwest::Client::new()
        .get(format!("{base_url}/healthz"))
        .send()
        .await
        .expect("request sent");
    assert_eq!(resp.status(), 200);
    srv.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn server_in_process_auth_token_round_trip() {
    if !e2e_enabled() {
        return;
    }
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("start_in_process");
    let bootstrap = srv
        .bootstrap_service("test-svc", &["writer"])
        .await
        .expect("bootstrap_service");
    let api_key = bootstrap.api_key().expect("machine has api key").clone();
    let jwt = srv.exchange_api_key(&api_key).await.expect("exchange_api_key");
    assert!(!jwt.is_empty());
    srv.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn server_drop_on_runtime_is_cancel_only() {
    if !e2e_enabled() {
        return;
    }
    let srv = WyrdTestServer::start_bound().await.expect("start_bound");
    drop(srv);
    // Test must not deadlock. If it returns, the cancel-only path fired.
}
