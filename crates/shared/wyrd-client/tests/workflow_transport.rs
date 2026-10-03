//! Wire contract of the shared Workflow clients against a deterministic server.

use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use secrecy::SecretString;
use serde_json::{Value, json};
use skald_providers::{ProviderError, RemoteProblem};
use skald_spec::{ProviderRequest, ProviderResponse};
use skald_workflow::{WorkflowGatewayCorrelation, WyrdGatewayCall, WyrdGatewayCaller};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::HttpTransport;
use wyrd_client::transport::config::HttpConfig;
use wyrd_client::transport::credential::ResolvedCredential;
use wyrd_client::{PublicWyrdGatewayCaller, Workflows, WyrdClient};
use wyrd_spec::card::workflow::{CreateWorkflowRunRequest, WorkflowRunStatus};
use wyrd_spec::error::WyrdError;
use wyrd_spec::gateway::{GatewayFallbackOverride, ModelRef};
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

/// An OpenAI Chat request whose body names a model the caller must replace.
fn chat_request() -> ProviderRequest {
    ProviderRequest::OpenAiChatCompletion(
        serde_json::from_value(json!({
            "model": "body-model",
            "messages": [{"role": "user", "content": "hi"}]
        }))
        .expect("chat request decodes"),
    )
}

/// A completed OpenAI Chat answer.
fn chat_answer() -> Value {
    json!({
        "id": "resp",
        "object": "chat.completion",
        "created": 0,
        "model": "gpt-a",
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": "ok"},
            "finish_reason": "stop"
        }]
    })
}

/// A gateway call for `request` on `model` with `fallback` and `timeout`.
fn gateway_call(
    request: ProviderRequest,
    model: &str,
    fallback: Option<GatewayFallbackOverride>,
    timeout: Duration,
) -> WyrdGatewayCall {
    WyrdGatewayCall {
        request,
        model: ModelRef::from_projection(model).expect("model ref"),
        fallback,
        timeout,
        correlation: WorkflowGatewayCorrelation {
            run_id: run_id(),
            step_id: "step".to_owned(),
            attempt: 1,
        },
    }
}

/// A fallback override naming `candidate`.
fn fallback(candidate: &str) -> GatewayFallbackOverride {
    GatewayFallbackOverride {
        candidates: vec![ModelRef::from_projection(candidate).expect("candidate")],
    }
}

/// The problem a recognized Wyrd `code` normalizes to: its catalog title
/// and remediation, never text from the answer body.
///
/// # Panics
/// Panics when `code` has no reconstructable catalog variant.
fn catalog_problem(code: &str, status: u16, field: Option<&str>) -> RemoteProblem {
    let catalog = WyrdError::from_code(code, String::new(), json!({})).expect("catalog code");
    RemoteProblem {
        code: code.to_owned(),
        status,
        message: catalog.title().to_owned(),
        field: field.map(str::to_owned),
        remediation: catalog.remediation().to_owned(),
    }
}

/// Build an API-key [`WyrdClient`] pointed at `base_url`, so an
/// authentication refusal renews through a real `/auth/token` exchange.
fn api_key_client(base_url: &str) -> WyrdClient {
    let mut config = ClientConfig::default();
    config.http.base_url = base_url.to_owned();
    let auth = AuthMiddleware::new(
        &config,
        ResolvedCredential::ApiKey(SecretString::from("api-key")),
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

/// A successful `/auth/token` answer issuing `access`.
fn token_answer(access: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "access_token": access,
        "refresh_token": "unused",
        "token_type": "Bearer",
        "expires_at": "2099-01-01T00:00:00Z"
    }))
}

/// Every model POST the server received, skipping credential exchanges.
async fn model_posts(server: &MockServer) -> Vec<wiremock::Request> {
    received(server)
        .await
        .into_iter()
        .filter(|request| request.url.path() == "/v1/chat/completions")
        .collect()
}

/// The remote problem a call returned.
///
/// # Panics
/// Panics when the call did not fail with a remote problem.
fn problem(outcome: Result<ProviderResponse, ProviderError>) -> RemoteProblem {
    match outcome {
        Err(ProviderError::RemoteProblem(problem)) => *problem,
        other => panic!("expected a remote problem, got {other:?}"),
    }
}

/// Concurrent public gateway calls keep their own fallback, deadline, and
/// model; each dialect reaches its ingress with the call's model; Vertex,
/// cancellation, and timeouts stop locally; native error envelopes become
/// redacted problems keeping only status, Wyrd code, OpenAI `param`, and the
/// code's catalog title and remediation; a `401` is sent once and renews the
/// credential instead of replaying the model call.
#[tokio::test]
async fn public_gateway_call_context_and_errors() {
    // Concurrent calls on one caller carry only their own fallback header.
    let server = MockServer::start().await;
    mount(
        &server,
        "POST",
        "/v1/chat/completions",
        ResponseTemplate::new(200).set_body_json(chat_answer()),
        u64::MAX,
    )
    .await;
    let caller = PublicWyrdGatewayCaller::new(client(&server.uri()));
    let token = CancellationToken::new();
    let (first, second, third) = tokio::join!(
        caller.call(
            gateway_call(
                chat_request(),
                "openai/gpt-a",
                Some(fallback("openai/gpt-b")),
                Duration::from_secs(5)
            ),
            &token,
        ),
        caller.call(
            gateway_call(
                chat_request(),
                "openai/gpt-c",
                Some(fallback("openai/gpt-d")),
                Duration::from_secs(9)
            ),
            &token,
        ),
        caller.call(
            gateway_call(chat_request(), "openai/gpt-e", None, Duration::from_secs(5)),
            &token,
        ),
    );
    for outcome in [first, second, third] {
        assert!(matches!(
            outcome,
            Ok(ProviderResponse::OpenAiChatCompletion(_))
        ));
    }
    let requests = received(&server).await;
    assert_eq!(requests.len(), 3);
    for request in &requests {
        let body: Value = serde_json::from_slice(&request.body).expect("JSON body");
        assert!(request.headers.contains_key("x-wyrd-access-token"));
        let header = request.headers.get("wyrd-gateway-fallback").map(|value| {
            let json = URL_SAFE_NO_PAD.decode(value.as_bytes()).expect("base64url");
            serde_json::from_slice::<GatewayFallbackOverride>(&json).expect("fallback")
        });
        let expected = match body["model"].as_str() {
            Some("openai/gpt-a") => Some(fallback("openai/gpt-b")),
            Some("openai/gpt-c") => Some(fallback("openai/gpt-d")),
            Some("openai/gpt-e") => None,
            other => panic!("unexpected model {other:?}"),
        };
        assert_eq!(header, expected);
    }

    // Every dialect reaches its ingress with the call's model.
    let server = MockServer::start().await;
    mount(
        &server,
        "POST",
        "/v1/responses",
        ResponseTemplate::new(200).set_body_json(json!({
            "id": "resp", "object": "response", "model": "gpt-r", "status": "completed",
            "created_at": 0, "output": []
        })),
        1,
    )
    .await;
    mount(
        &server,
        "POST",
        "/v1/messages",
        ResponseTemplate::new(200).set_body_json(json!({
            "id": "msg", "type": "message", "role": "assistant", "model": "claude-a",
            "content": [{"type": "text", "text": "ok"}], "stop_reason": "end_turn",
            "stop_sequence": null, "usage": {"input_tokens": 1, "output_tokens": 1}
        })),
        1,
    )
    .await;
    mount(
        &server,
        "POST",
        "/v1beta/models/gemini-a:generateContent",
        ResponseTemplate::new(200).set_body_json(json!({
            "candidates": [{"content": {"role": "model", "parts": [{"text": "ok"}]}}]
        })),
        1,
    )
    .await;
    let caller = PublicWyrdGatewayCaller::new(client(&server.uri()));
    let responses = ProviderRequest::OpenAiResponses(
        serde_json::from_value(json!({"model": "body-model", "input": "hi"})).expect("responses"),
    );
    let anthropic = ProviderRequest::AnthropicMessage(
        serde_json::from_value(json!({
            "model": "body-model", "max_tokens": 16,
            "messages": [{"role": "user", "content": [{"type": "text", "text": "hi"}]}]
        }))
        .expect("anthropic"),
    );
    let gemini = ProviderRequest::GeminiGenerateContent(
        serde_json::from_value(json!({"contents": [{"role": "user", "parts": [{"text": "hi"}]}]}))
            .expect("gemini"),
    );
    let deadline = Duration::from_secs(5);
    assert!(matches!(
        caller
            .call(
                gateway_call(responses, "openai/gpt-r", None, deadline),
                &token
            )
            .await,
        Ok(ProviderResponse::OpenAiResponses(_))
    ));
    assert!(matches!(
        caller
            .call(
                gateway_call(anthropic, "anthropic/claude-a", None, deadline),
                &token
            )
            .await,
        Ok(ProviderResponse::AnthropicMessage(_))
    ));
    assert!(matches!(
        caller
            .call(
                gateway_call(gemini.clone(), "gemini/gemini-a", None, deadline),
                &token
            )
            .await,
        Ok(ProviderResponse::GeminiGenerateContent(_))
    ));
    let models: Vec<Value> = received(&server)
        .await
        .iter()
        .map(|request| {
            serde_json::from_slice::<Value>(&request.body).expect("JSON body")["model"].clone()
        })
        .collect();
    assert_eq!(
        models,
        vec![json!("openai/gpt-r"), json!("claude-a"), Value::Null]
    );

    // Vertex and a cancelled run stop before any request; a slow answer times
    // out at the call's own deadline.
    let vertex = ProviderRequest::Vertex(
        serde_json::from_value(json!({"contents": [{"role": "user", "parts": [{"text": "hi"}]}]}))
            .expect("vertex"),
    );
    let refused = caller
        .call(
            gateway_call(vertex, "vertex/gemini-a", None, deadline),
            &token,
        )
        .await
        .expect_err("Vertex is refused locally");
    assert_eq!(refused.code(), "SKALD_PROVIDERS_400_BAD_REQUEST");
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    let stopped = caller
        .call(
            gateway_call(chat_request(), "openai/gpt-a", None, deadline),
            &cancelled,
        )
        .await
        .expect_err("a cancelled run stops the call");
    assert_eq!(stopped.code(), "SKALD_PROVIDERS_408_TIMEOUT");
    assert_eq!(received(&server).await.len(), 3, "nothing else was sent");
    let slow = MockServer::start().await;
    mount(
        &slow,
        "POST",
        "/v1/chat/completions",
        ResponseTemplate::new(200)
            .set_body_json(chat_answer())
            .set_delay(Duration::from_secs(5)),
        1,
    )
    .await;
    let timed_out = PublicWyrdGatewayCaller::new(client(&slow.uri()))
        .call(
            gateway_call(
                chat_request(),
                "openai/gpt-a",
                None,
                Duration::from_millis(100),
            ),
            &token,
        )
        .await
        .expect_err("the call's deadline bounds it");
    assert_eq!(timed_out.code(), "SKALD_PROVIDERS_408_TIMEOUT");

    // Native error envelopes keep only the safe common fields.
    let refusal = |status: u16, route: &str, body: Value| {
        let route = route.to_owned();
        async move {
            let server = MockServer::start().await;
            mount(
                &server,
                "POST",
                &route,
                ResponseTemplate::new(status).set_body_json(body),
                1,
            )
            .await;
            server
        }
    };
    let openai = refusal(
        400,
        "/v1/chat/completions",
        json!({"error": {
            "message": "echoed prompt sk-canary", "type": "invalid_request_error",
            "param": "fallback", "code": "WYRD_GATEWAY_400_INVALID_REQUEST"
        }}),
    )
    .await;
    let refused = problem(
        PublicWyrdGatewayCaller::new(client(&openai.uri()))
            .call(
                gateway_call(chat_request(), "openai/gpt-a", None, deadline),
                &token,
            )
            .await,
    );
    assert_eq!(
        refused,
        catalog_problem("WYRD_GATEWAY_400_INVALID_REQUEST", 400, Some("fallback"))
    );
    let anthropic = refusal(
        429,
        "/v1/messages",
        json!({"type": "error", "error": {
            "type": "rate_limit_error", "message": "echoed prompt sk-canary",
            "code": "WYRD_GATEWAY_429_LIMIT_EXCEEDED"
        }}),
    )
    .await;
    let anthropic_request = ProviderRequest::AnthropicMessage(
        serde_json::from_value(json!({
            "model": "body-model", "max_tokens": 16,
            "messages": [{"role": "user", "content": [{"type": "text", "text": "hi"}]}]
        }))
        .expect("anthropic"),
    );
    let refused = problem(
        PublicWyrdGatewayCaller::new(client(&anthropic.uri()))
            .call(
                gateway_call(anthropic_request, "anthropic/claude-a", None, deadline),
                &token,
            )
            .await,
    );
    assert_eq!(
        refused,
        catalog_problem("WYRD_GATEWAY_429_LIMIT_EXCEEDED", 429, None)
    );
    let google = refusal(
        504,
        "/v1beta/models/gemini-a:generateContent",
        json!({"error": {
            "code": 504, "message": "echoed prompt sk-canary", "status": "DEADLINE_EXCEEDED",
            "details": [{"@type": "type.googleapis.com/google.rpc.ErrorInfo",
                "reason": "WYRD_GATEWAY_504_DEADLINE_EXCEEDED", "domain": "wyrd"}]
        }}),
    )
    .await;
    let refused = problem(
        PublicWyrdGatewayCaller::new(client(&google.uri()))
            .call(
                gateway_call(gemini, "gemini/gemini-a", None, deadline),
                &token,
            )
            .await,
    );
    assert_eq!(
        refused,
        catalog_problem("WYRD_GATEWAY_504_DEADLINE_EXCEEDED", 504, None)
    );

    // A relayed provider refusal without a Wyrd code keeps no upstream text.
    for (status, code) in [
        (429, "SKALD_PROVIDERS_429_RATE_LIMIT"),
        (503, "SKALD_PROVIDERS_5XX_UPSTREAM"),
        (408, "SKALD_PROVIDERS_408_TIMEOUT"),
        (400, "SKALD_PROVIDERS_400_BAD_REQUEST"),
    ] {
        let upstream = refusal(
            status,
            "/v1/chat/completions",
            json!({"error": {
                "message": "echoed prompt sk-canary", "type": "provider_error",
                "param": null, "code": "provider_specific"
            }}),
        )
        .await;
        let refused = problem(
            PublicWyrdGatewayCaller::new(client(&upstream.uri()))
                .call(
                    gateway_call(chat_request(), "openai/gpt-a", None, deadline),
                    &token,
                )
                .await,
        );
        assert_eq!((refused.code.as_str(), refused.status), (code, status));
        assert!(!format!("{refused:?}").contains("sk-canary"));
        assert_eq!(refused.field, None);
    }

    // An uncoded 401 is sent once and surfaces as a provider auth refusal.
    let unauthorized = refusal(
        401,
        "/v1/chat/completions",
        json!({"error": {"message": "echoed prompt sk-canary", "type": "provider_error"}}),
    )
    .await;
    let refused = problem(
        PublicWyrdGatewayCaller::new(client(&unauthorized.uri()))
            .call(
                gateway_call(chat_request(), "openai/gpt-a", None, deadline),
                &token,
            )
            .await,
    );
    assert_eq!(
        (refused.code.as_str(), refused.status),
        ("SKALD_PROVIDERS_401_AUTH", 401)
    );
    assert!(!format!("{refused:?}").contains("sk-canary"));
    assert_eq!(model_posts(&unauthorized).await.len(), 1);

    // A 401 carrying a Wyrd auth code renews the credential without
    // resending the model call and returns the original refusal; the next
    // call carries the renewed bearer.
    let spoofed = MockServer::start().await;
    mount(&spoofed, "POST", "/auth/token", token_answer("tok-a"), 1).await;
    mount(&spoofed, "POST", "/auth/token", token_answer("tok-b"), 1).await;
    mount(
        &spoofed,
        "POST",
        "/v1/chat/completions",
        ResponseTemplate::new(401).set_body_json(json!({"error": {
            "message": "echoed prompt sk-canary", "type": "invalid_request_error",
            "code": "WYRD_AUTH_401_INVALID_TOKEN"
        }})),
        1,
    )
    .await;
    mount(
        &spoofed,
        "POST",
        "/v1/chat/completions",
        ResponseTemplate::new(200).set_body_json(chat_answer()),
        1,
    )
    .await;
    let caller = PublicWyrdGatewayCaller::new(api_key_client(&spoofed.uri()));
    let refused = problem(
        caller
            .call(
                gateway_call(chat_request(), "openai/gpt-a", None, deadline),
                &token,
            )
            .await,
    );
    assert_eq!(
        refused,
        catalog_problem("WYRD_AUTH_401_INVALID_TOKEN", 401, None)
    );
    assert_eq!(
        model_posts(&spoofed).await.len(),
        1,
        "the model call is not resent"
    );
    assert!(
        caller
            .call(
                gateway_call(chat_request(), "openai/gpt-a", None, deadline),
                &token,
            )
            .await
            .is_ok()
    );
    let bearers: Vec<_> = model_posts(&spoofed)
        .await
        .iter()
        .map(|request| {
            request.headers["x-wyrd-access-token"]
                .to_str()
                .expect("ASCII")
                .to_owned()
        })
        .collect();
    assert_eq!(bearers, ["Bearer tok-a", "Bearer tok-b"]);

    // A failed renewal returns its authentication error, still without
    // resending the model call.
    let unrenewable = MockServer::start().await;
    mount(
        &unrenewable,
        "POST",
        "/auth/token",
        token_answer("tok-a"),
        1,
    )
    .await;
    mount(
        &unrenewable,
        "POST",
        "/auth/token",
        ResponseTemplate::new(401).set_body_json(json!({
            "type": "about:blank",
            "title": "API key invalid",
            "status": 401,
            "code": "WYRD_AUTH_401_API_KEY_INVALID",
            "detail": "api key revoked",
            "details": {}
        })),
        1,
    )
    .await;
    mount(
        &unrenewable,
        "POST",
        "/v1/chat/completions",
        ResponseTemplate::new(401).set_body_json(json!({"error": {
            "message": "echoed prompt sk-canary", "type": "provider_error"
        }})),
        1,
    )
    .await;
    let refused = problem(
        PublicWyrdGatewayCaller::new(api_key_client(&unrenewable.uri()))
            .call(
                gateway_call(chat_request(), "openai/gpt-a", None, deadline),
                &token,
            )
            .await,
    );
    assert_eq!(
        (refused.code.as_str(), refused.status),
        ("WYRD_AUTH_401_API_KEY_INVALID", 401)
    );
    assert_eq!(model_posts(&unrenewable).await.len(), 1);
}
