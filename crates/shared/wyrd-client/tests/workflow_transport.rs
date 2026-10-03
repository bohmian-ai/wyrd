//! Wire contract of the shared Workflow clients against a deterministic server.

use std::sync::Arc;
use std::time::Duration;

use secrecy::SecretString;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::HttpTransport;
use wyrd_client::transport::config::HttpConfig;
use wyrd_client::transport::credential::ResolvedCredential;
use wyrd_client::{Workflows, WyrdClient};
use wyrd_spec::card::workflow::{CreateWorkflowRunRequest, WorkflowRunStatus};
use wyrd_spec::ids::WorkflowRunId;

/// Fixed run identity used by every scripted response.
const RUN_ID: &str = "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b20";

/// Build a bearer-authenticated [`WyrdClient`] pointed at `base_url`.
fn client(base_url: &str) -> WyrdClient {
    let mut config = ClientConfig::default();
    config.http.base_url = base_url.to_owned();
    let auth = AuthMiddleware::new(
        &config,
        ResolvedCredential::BearerToken(SecretString::from("test-bearer")),
    )
    .expect("auth builds");
    let transport = HttpTransport::new(
        &HttpConfig {
            base_url: base_url.to_owned(),
            ..HttpConfig::default()
        },
        Arc::clone(&auth),
    )
    .expect("transport builds");
    WyrdClient::from_parts(auth, transport, config.grpc)
}

/// The fixed run identity as its typed ID.
fn run_id() -> WorkflowRunId {
    RUN_ID.parse().expect("fixed run id is a UUIDv7")
}

/// Route of the fixed run.
fn run_path() -> String {
    format!("/v1/workflow-runs/{RUN_ID}")
}

/// A direct run snapshot with `status`.
fn run_json(status: &str) -> Value {
    json!({
        "run_id": RUN_ID,
        "workflow": {"kind": "Workflow", "name": "triage", "version": "1.0.0", "space": "team"},
        "status": status,
        "created_at": "2026-01-01T00:00:00Z",
        "started_at": null,
        "ended_at": null,
        "error": null
    })
}

/// The create request every create case submits.
fn create_request() -> CreateWorkflowRunRequest {
    serde_json::from_value(json!({
        "workflow": {"kind": "Workflow", "name": "triage", "version": "1.0.0", "space": "team"},
        "input": {"ticket": "T-1"},
        "timeout_seconds": 30
    }))
    .expect("create request is valid")
}

/// Mount `response` for `verb` on `route`, answering at most `times` requests.
async fn mount(
    server: &MockServer,
    verb: &str,
    route: &str,
    response: ResponseTemplate,
    times: u64,
) {
    Mock::given(method(verb))
        .and(path(route))
        .respond_with(response)
        .up_to_n_times(times)
        .mount(server)
        .await;
}

/// Every request the server has received, in arrival order.
async fn received(server: &MockServer) -> Vec<wiremock::Request> {
    server
        .received_requests()
        .await
        .expect("request recording is enabled")
}

/// Create, get, cancel, and wait project the exact run routes and snapshots.
#[tokio::test]
async fn shared_workflow_client_contract() {
    // First acceptance answers 202 and an idempotent replay 200; both decode
    // the same direct snapshot.
    for status in [202, 200] {
        let server = MockServer::start().await;
        mount(
            &server,
            "POST",
            "/v1/workflow-runs",
            ResponseTemplate::new(status).set_body_json(run_json("queued")),
            1,
        )
        .await;
        let run = Workflows::new(client(&server.uri()))
            .create(&create_request())
            .await
            .expect("create succeeds");
        assert_eq!(run.run_id, run_id());
        assert_eq!(run.status, WorkflowRunStatus::Queued);
        let requests = received(&server).await;
        assert_eq!(requests.len(), 1);
        let body: Value = serde_json::from_slice(&requests[0].body).expect("JSON body");
        assert_eq!(
            body,
            serde_json::to_value(create_request()).expect("request serializes")
        );
        assert!(requests[0].headers.contains_key("idempotency-key"));
    }

    // A retried create replays one Idempotency-Key.
    let server = MockServer::start().await;
    mount(
        &server,
        "POST",
        "/v1/workflow-runs",
        ResponseTemplate::new(503),
        1,
    )
    .await;
    mount(
        &server,
        "POST",
        "/v1/workflow-runs",
        ResponseTemplate::new(202).set_body_json(run_json("queued")),
        1,
    )
    .await;
    Workflows::new(client(&server.uri()))
        .create(&create_request())
        .await
        .expect("retried create succeeds");
    let keys: Vec<_> = received(&server)
        .await
        .iter()
        .map(|request| request.headers["idempotency-key"].clone())
        .collect();
    assert_eq!(keys.len(), 2);
    assert_eq!(keys[0], keys[1]);

    // Get and cancel use the run route and its cancel action.
    let server = MockServer::start().await;
    mount(
        &server,
        "GET",
        &run_path(),
        ResponseTemplate::new(200).set_body_json(run_json("running")),
        1,
    )
    .await;
    mount(
        &server,
        "POST",
        &format!("{}/cancel", run_path()),
        ResponseTemplate::new(200).set_body_json(run_json("cancelled")),
        1,
    )
    .await;
    let workflows = Workflows::new(client(&server.uri()));
    let got = workflows.get(&run_id()).await.expect("get succeeds");
    assert_eq!(got.status, WorkflowRunStatus::Running);
    let cancelled = workflows.cancel(&run_id()).await.expect("cancel succeeds");
    assert_eq!(cancelled.status, WorkflowRunStatus::Cancelled);

    // Wait polls a running run until it succeeds.
    let server = MockServer::start().await;
    mount(
        &server,
        "GET",
        &run_path(),
        ResponseTemplate::new(200).set_body_json(run_json("running")),
        1,
    )
    .await;
    mount(
        &server,
        "GET",
        &run_path(),
        ResponseTemplate::new(200).set_body_json(run_json("succeeded")),
        1,
    )
    .await;
    let run = Workflows::new(client(&server.uri()))
        .wait(&run_id())
        .await
        .expect("wait succeeds");
    assert_eq!(run.status, WorkflowRunStatus::Succeeded);
    assert_eq!(received(&server).await.len(), 2);

    // Every unsuccessful terminal status is a returned value, not an error.
    for (status, expected) in [
        ("failed", WorkflowRunStatus::Failed),
        ("cancelled", WorkflowRunStatus::Cancelled),
        ("timed_out", WorkflowRunStatus::TimedOut),
    ] {
        let server = MockServer::start().await;
        mount(
            &server,
            "GET",
            &run_path(),
            ResponseTemplate::new(200).set_body_json(run_json(status)),
            1,
        )
        .await;
        let run = Workflows::new(client(&server.uri()))
            .wait(&run_id())
            .await
            .expect("terminal run is returned");
        assert_eq!(run.status, expected);
    }

    // A canonical server error propagates with its stable code.
    let server = MockServer::start().await;
    mount(
        &server,
        "GET",
        &run_path(),
        ResponseTemplate::new(404).set_body_json(json!({
            "type": "about:blank",
            "title": "Workflow run not found",
            "status": 404,
            "code": "WYRD_WORKFLOW_404_RUN_NOT_FOUND",
            "detail": "workflow run not found",
            "details": {}
        })),
        1,
    )
    .await;
    let error = Workflows::new(client(&server.uri()))
        .wait(&run_id())
        .await
        .expect_err("missing run is refused");
    assert_eq!(error.code(), "WYRD_WORKFLOW_404_RUN_NOT_FOUND");

    // Dropping wait stops polling and never cancels or resubmits the run.
    let server = MockServer::start().await;
    mount(
        &server,
        "GET",
        &run_path(),
        ResponseTemplate::new(200).set_body_json(run_json("running")),
        u64::MAX,
    )
    .await;
    let workflows = Workflows::new(client(&server.uri()));
    let outcome =
        tokio::time::timeout(Duration::from_millis(1500), workflows.wait(&run_id())).await;
    assert!(outcome.is_err(), "a running run keeps wait polling");
    let requests = received(&server).await;
    assert!(!requests.is_empty());
    assert!(
        requests
            .iter()
            .all(|request| request.method == wiremock::http::Method::GET
                && request.url.path() == run_path())
    );
}
