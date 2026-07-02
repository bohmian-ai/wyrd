mod eval_support;

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use assert_cmd::prelude::*;
use axum::Router;
use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::{Next, from_fn_with_state};
use axum::response::IntoResponse;
use axum::routing::post;
use serde_json::json;
use tokio::sync::Mutex;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

use eval_support::{spec, write_eval_card};

type CapturedRequest = (String, Option<String>);

#[derive(Clone, Default)]
struct TestState {
    captured: Arc<Mutex<Vec<CapturedRequest>>>,
    next_call: Arc<AtomicU32>,
}

async fn capture_headers(
    State(state): State<TestState>,
    req: Request,
    next: Next,
) -> axum::response::Response {
    let path = req.uri().path().to_owned();
    let auth = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    state.captured.lock().await.push((path, auth));
    next.run(req).await
}

const FIXED_RUN_ID: &str = "test-run-01";
const FIXED_LEASE: &str = "cli-lease-token";

async fn handle_open() -> axum::Json<serde_json::Value> {
    axum::Json(json!({
        "run_id": FIXED_RUN_ID,
        "lease_token": FIXED_LEASE,
    }))
}

async fn handle_next(State(state): State<TestState>) -> axum::response::Response {
    let call = state.next_call.fetch_add(1, Ordering::SeqCst);
    if call == 0 {
        axum::Json(json!({
            "kind": "agent_turn",
            "scenario_id": "happy_path",
            "turn": 0,
            "message": "Start",
            "history": []
        }))
        .into_response()
    } else {
        axum::Json(json!({ "kind": "run_complete" })).into_response()
    }
}

async fn handle_submission() -> StatusCode {
    StatusCode::OK
}

#[tokio::test]
async fn server_protocol_carries_lease_after_open() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let eval_path = tmp.path().join("eval.json");
    write_eval_card(&eval_path, spec(Vec::new(), None));

    let agent = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "response": "DONE",
            "records": []
        })))
        .mount(&agent)
        .await;

    let state = TestState::default();
    let next_route = format!("/v1/eval/runs/{FIXED_RUN_ID}/next");
    let agent_turn_route = format!("/v1/eval/runs/{FIXED_RUN_ID}/agent-turn");
    let user_turn_route = format!("/v1/eval/runs/{FIXED_RUN_ID}/user-turn");
    let app = Router::new()
        .route("/v1/eval/runs", post(handle_open))
        .route(&next_route, post(handle_next))
        .route(&agent_turn_route, post(handle_submission))
        .route(&user_turn_route, post(handle_submission))
        .layer(from_fn_with_state(state.clone(), capture_headers))
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test server");
    let addr = listener.local_addr().expect("local addr");
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("server runs");
    });

    let server_url = format!("http://{addr}");
    let agent_url = agent.uri();
    let eval_arg = eval_path.to_str().expect("utf8 path").to_owned();
    tokio::task::spawn_blocking(move || {
        std::process::Command::cargo_bin("wyrd")
            .expect("wyrd binary")
            .args([
                "eval",
                "run",
                "--server",
                &server_url,
                "--agent-url",
                &agent_url,
                "--eval",
                &eval_arg,
                "--judge-mock",
            ])
            .assert()
            .success();
    })
    .await
    .expect("blocking command task joins");

    server.abort();
    let rows = state.captured.lock().await.clone();
    let protected: Vec<_> = rows
        .iter()
        .filter(|(path, _)| {
            path.ends_with("/next") || path.ends_with("/agent-turn") || path.ends_with("/user-turn")
        })
        .collect();
    assert!(!protected.is_empty(), "expected post-open protocol calls");
    for (_, auth) in protected {
        assert_eq!(
            auth.as_deref(),
            Some("Bearer cli-lease-token"),
            "post-open calls must carry the lease token"
        );
    }
}
