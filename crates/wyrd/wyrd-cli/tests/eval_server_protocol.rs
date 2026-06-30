mod eval_support;

use std::sync::Arc;

use assert_cmd::prelude::*;
use axum::Router;
use axum::extract::{Request, State};
use axum::http::header;
use axum::middleware::{Next, from_fn_with_state};
use serde_json::json;
use tokio::sync::Mutex;
use vala_eval::orchestrator::OrchestratorError;
use vala_http::eval::{AppState, router};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_spec::vala::ids::LeaseToken;

use eval_support::{eval_ref, scenario, spec, write_eval_card};

#[derive(Clone, Default)]
struct Capture {
    rows: SharedCapturedRequests,
}

type CapturedRequest = (String, Option<String>);
type SharedCapturedRequests = Arc<Mutex<Vec<CapturedRequest>>>;

async fn capture_headers(
    State(state): State<Capture>,
    req: Request,
    next: Next,
) -> axum::response::Response {
    let path = req.uri().path().to_owned();
    let auth = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    state.rows.lock().await.push((path, auth));
    next.run(req).await
}

fn fixed_token() -> LeaseToken {
    LeaseToken::new("cli-lease-token").expect("static lease token is valid")
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

    let loader_ref = eval_ref();
    let state = AppState::new(
        Arc::new(move |got| {
            if got == &loader_ref {
                Ok(vec![scenario()])
            } else {
                Err(OrchestratorError::EmbeddedCallback {
                    reason: "unexpected eval ref".to_owned(),
                })
            }
        }),
        Arc::new(|| Ok(fixed_token())),
    );
    let capture = Capture::default();
    let app = Router::new()
        .nest("/api/v1/eval", router(state))
        .layer(from_fn_with_state(capture.clone(), capture_headers));
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
    let rows = capture.rows.lock().await.clone();
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
