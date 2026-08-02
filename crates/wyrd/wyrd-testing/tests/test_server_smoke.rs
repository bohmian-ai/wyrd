use wyrd_testing::WyrdTestServer;

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

/// A readiness failure tears down the real bound task before another process starts.
///
/// # Panics
///
/// Panics when injected readiness rollback leaves a process-local pool or
/// listener task alive enough to prevent a fresh bound server from serving.
#[tokio::test]
async fn readiness_failure_rolls_back_before_fresh_bound_server_starts() {
    if !e2e_enabled() {
        return;
    }
    let result = WyrdTestServer::builder()
        .with_readiness_failure_for_test()
        .start_bound()
        .await;
    let error = match result {
        Ok(server) => {
            server
                .shutdown()
                .await
                .expect("unexpectedly ready server shutdown");
            panic!("injected readiness failure must fail startup");
        }
        Err(error) => error,
    };
    assert!(
        matches!(error, wyrd_testing::WyrdTestServerError::Bind(message) if message.contains("injected readiness failure")),
        "readiness error must remain primary after rollback"
    );

    let fresh = WyrdTestServer::start_bound()
        .await
        .expect("fresh process after readiness rollback");
    let response = reqwest::Client::new()
        .get(format!(
            "{}/healthz",
            fresh.base_url().expect("fresh base url")
        ))
        .send()
        .await
        .expect("fresh process local pool and listener");
    assert_eq!(response.status(), 200);
    fresh.shutdown().await.expect("fresh server shutdown");
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
    let jwt = srv
        .exchange_api_key(&api_key)
        .await
        .expect("exchange_api_key");
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
