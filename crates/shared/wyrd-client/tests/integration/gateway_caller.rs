//! Wire contract of the public Agent gateway caller against a deterministic server.

use std::sync::Arc;
use std::time::Duration;

use secrecy::SecretString;
use serde_json::{Value, json};
use skald_agent::{WyrdGatewayCall, WyrdGatewayCaller};
use skald_providers::{ProviderError, RemoteProblem};
use skald_spec::{ProviderRequest, ProviderResponse};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{self, UnboundedReceiver};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::HttpTransport;
use wyrd_client::transport::config::HttpConfig;
use wyrd_client::transport::credential::ResolvedCredential;
use wyrd_client::{PublicWyrdGatewayCaller, WyrdClient};
use wyrd_spec::error::WyrdError;
use wyrd_spec::gateway::ModelRef;
use wyrd_spec::ids::CardUid;
use wyrd_spec::vala::ids::RunId;

/// Build a bearer-authenticated [`WyrdClient`] pointed at `base_url`.
///
/// # Panics
/// Panics when the fixed bearer credential or `base_url` cannot assemble the
/// client's authentication or HTTP transport.
fn client(base_url: &str) -> WyrdClient {
    let mut config = ClientConfig::default();
    base_url.clone_into(&mut config.http.base_url);
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
///
/// # Panics
/// Panics when the mock server was started without request recording.
async fn received(server: &MockServer) -> Vec<Request> {
    server
        .received_requests()
        .await
        .expect("request recording is enabled")
}

/// An OpenAI Chat request whose body names a model the caller must replace.
///
/// # Panics
/// Panics when the fixed request document stops decoding as a Chat request.
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

/// A gateway call for `request` on `model` with `timeout`.
///
/// # Panics
/// Panics when `model` is not a valid identity.
fn gateway_call(request: ProviderRequest, model: &str, timeout: Duration) -> WyrdGatewayCall {
    WyrdGatewayCall {
        provider: request.provider(),
        request,
        model: ModelRef::from_projection(model).expect("model ref"),
        timeout,
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
///
/// # Panics
/// Panics when the fixed API key or `base_url` cannot assemble the client's
/// authentication or HTTP transport.
fn api_key_client(base_url: &str) -> WyrdClient {
    let mut config = ClientConfig::default();
    base_url.clone_into(&mut config.http.base_url);
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

/// A successful `/auth/token` body issuing `access`.
fn token_json(access: &str) -> Value {
    json!({
        "access_token": access,
        "refresh_token": "unused",
        "token_type": "Bearer",
        "expires_in": 3600
    })
}

/// A successful `/auth/token` answer issuing `access`.
fn token_answer(access: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(token_json(access))
}

/// A complete HTTP/1.1 answer with `status` and JSON `body` that closes its
/// connection.
fn raw_reply(status: &str, body: &Value) -> String {
    let body = body.to_string();
    format!(
        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// Status line and headers of a `401` refusal that promises a 64-byte body,
/// so a reply that writes fewer bytes is truncated or left pending.
const UNAUTHORIZED_HEAD: &str = "HTTP/1.1 401 Unauthorized\r\ncontent-type: application/json\r\ncontent-length: 64\r\nconnection: close\r\n\r\n";

/// Read one HTTP/1.1 request and its `content-length` body from `stream`.
///
/// Returns the request path, or `None` when the client closes first.
///
/// # Panics
/// Panics when the socket read fails or the request head is not a UTF-8
/// HTTP/1.1 head with a numeric `content-length`.
async fn read_request(stream: &mut TcpStream) -> Option<String> {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        if let Some(end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            let head = std::str::from_utf8(&request[..end]).expect("UTF-8 request head");
            let length = head
                .lines()
                .filter_map(|line| line.split_once(':'))
                .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                .map_or(0, |(_, value)| {
                    value
                        .trim()
                        .parse::<usize>()
                        .expect("numeric content-length")
                });
            if request.len() >= end + 4 + length {
                let target = head.split_whitespace().nth(1).expect("request target");
                return Some(target.to_owned());
            }
        }
        let read = stream.read(&mut buffer).await.expect("request reads");
        if read == 0 {
            return None;
        }
        request.extend_from_slice(&buffer[..read]);
    }
}

/// One scripted answer from a [`raw_server`] connection.
enum Reply {
    /// Write these bytes, then close the connection.
    Send(String),
    /// Write these bytes (possibly none), then hold the connection open
    /// until the client closes it, so the answer never completes.
    Hold(String),
}

/// Serve `replies` in order on a loopback listener, one connection each, and
/// report every request path as soon as the request has fully arrived.
///
/// Requests after the last reply are still reported and their connections
/// closed unanswered, so a resend is always observed. Returns the server's
/// base URL and the receiver of request paths.
///
/// # Panics
/// Panics when the listener cannot bind; the serving task panics when a
/// connection cannot be accepted, read, or written, or when the client sends
/// more bytes on a held connection.
async fn raw_server(replies: Vec<Reply>) -> (String, UnboundedReceiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener binds");
    let base_url = format!(
        "http://{}",
        listener.local_addr().expect("listener address")
    );
    let (paths, received) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let mut replies = replies.into_iter();
        loop {
            let (mut stream, _) = listener.accept().await.expect("connection accepts");
            let Some(path) = read_request(&mut stream).await else {
                continue;
            };
            paths.send(path).expect("the test receives request paths");
            match replies.next() {
                Some(Reply::Send(reply)) => stream
                    .write_all(reply.as_bytes())
                    .await
                    .expect("reply writes"),
                Some(Reply::Hold(partial)) => {
                    stream
                        .write_all(partial.as_bytes())
                        .await
                        .expect("partial reply writes");
                    tokio::spawn(async move {
                        let closed = stream.read(&mut [0_u8; 1]).await.expect("held read");
                        assert_eq!(closed, 0, "the client sends nothing more");
                    });
                }
                None => {}
            }
        }
    });
    (base_url, received)
}

/// Every request path a [`raw_server`] has reported so far, in arrival order.
fn reported(paths: &mut UnboundedReceiver<String>) -> Vec<String> {
    std::iter::from_fn(|| paths.try_recv().ok()).collect()
}

/// Every model POST the server received, skipping credential exchanges.
///
/// # Panics
/// Panics when the mock server was started without request recording.
async fn model_posts(server: &MockServer) -> Vec<Request> {
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

/// A caller sends each of the application Run and Card UID it was given as
/// its own `wyrd-run-id` or `wyrd-card-uid` header: both, either alone, or
/// neither.
///
/// # Panics
/// Panics when a call fails or a correlation header differs.
#[tokio::test]
async fn public_gateway_call_carries_run_and_card_uid() {
    let server = MockServer::start().await;
    mount(
        &server,
        "POST",
        "/v1/chat/completions",
        ResponseTemplate::new(200).set_body_json(chat_answer()),
        4,
    )
    .await;
    let run = RunId::from_string("run-1".to_owned());
    let card_uid = CardUid::new("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00").expect("Card UID");
    let token = CancellationToken::new();
    let call = || gateway_call(chat_request(), "openai/gpt-a", Duration::from_secs(5));
    for (run, card_uid) in [
        (Some(&run), Some(&card_uid)),
        (None, None),
        (Some(&run), None),
        (None, Some(&card_uid)),
    ] {
        PublicWyrdGatewayCaller::new(client(&server.uri()))
            .with_subject(run, card_uid)
            .call(call(), &token)
            .await
            .expect("the call answers");
    }
    let requests = received(&server).await;
    let header = |index: usize, name: &str| {
        requests[index]
            .headers
            .get(name)
            .map(|value| value.to_str().expect("ASCII header").to_owned())
    };
    assert_eq!(header(0, "wyrd-run-id").as_deref(), Some("run-1"));
    assert_eq!(
        header(0, "wyrd-card-uid").as_deref(),
        Some(card_uid.as_str())
    );
    assert_eq!(header(1, "wyrd-run-id"), None);
    assert_eq!(header(1, "wyrd-card-uid"), None);
    assert_eq!(header(2, "wyrd-run-id").as_deref(), Some("run-1"));
    assert_eq!(header(2, "wyrd-card-uid"), None);
    assert_eq!(header(3, "wyrd-run-id"), None);
    assert_eq!(
        header(3, "wyrd-card-uid").as_deref(),
        Some(card_uid.as_str())
    );
}

/// A gateway call made inside a `tracing` span carrying a valid
/// OpenTelemetry context sends `traceparent` in the ambient trace, naming
/// the call's own client span inside it, so the server's gateway work joins
/// the caller's trace.
///
/// # Panics
/// Panics when the call fails, the ambient span is not valid, or the sent
/// `traceparent` is not a sampled child position in the ambient trace.
#[tokio::test]
async fn public_gateway_call_carries_the_ambient_trace_context() {
    use opentelemetry::trace::{TraceContextExt as _, TracerProvider as _};
    use tracing::Instrument as _;
    use tracing::instrument::WithSubscriber as _;
    use tracing_opentelemetry::OpenTelemetrySpanExt as _;
    use tracing_subscriber::layer::SubscriberExt as _;

    let server = MockServer::start().await;
    mount(
        &server,
        "POST",
        "/v1/chat/completions",
        ResponseTemplate::new(200).set_body_json(chat_answer()),
        1,
    )
    .await;
    let traced = tracing::Dispatch::new(
        tracing_subscriber::registry().with(
            tracing_opentelemetry::layer().with_tracer(
                opentelemetry_sdk::trace::SdkTracerProvider::builder()
                    .build()
                    .tracer("gateway-caller"),
            ),
        ),
    );
    let ambient = tracing::dispatcher::with_default(&traced, || tracing::info_span!("caller"));
    let (trace, ambient_span) = tracing::dispatcher::with_default(&traced, || {
        let context = ambient.context();
        let span = context.span();
        let span_context = span.span_context();
        assert!(
            span_context.is_valid(),
            "the ambient span carries a valid OpenTelemetry context"
        );
        (
            format!("00-{}-", span_context.trace_id()),
            span_context.span_id().to_string(),
        )
    });
    let token = CancellationToken::new();
    PublicWyrdGatewayCaller::new(client(&server.uri()))
        .call(
            gateway_call(chat_request(), "openai/gpt-a", Duration::from_secs(5)),
            &token,
        )
        .instrument(ambient)
        .with_subscriber(traced)
        .await
        .expect("the call answers");
    let requests = received(&server).await;
    assert_eq!(requests.len(), 1);
    let sent = requests[0]
        .headers
        .get("traceparent")
        .and_then(|value| value.to_str().ok())
        .expect("the call sends traceparent");
    assert!(sent.starts_with(&trace) && sent.ends_with("-01"), "{sent}");
    assert!(
        !sent.contains(&ambient_span),
        "the call's own span is the parent: {sent}"
    );
    assert_eq!(
        requests[0].headers.get("tracestate"),
        None,
        "empty state is not sent"
    );
}

/// Concurrent public gateway calls keep their own deadline and model; each
/// dialect reaches its ingress with the call's model; Vertex,
/// cancellation, and timeouts stop locally; native error envelopes become
/// redacted problems keeping only status, Wyrd code, OpenAI `param`, and the
/// code's catalog title and remediation; a `401` is sent once and renews the
/// credential instead of replaying the model call, even when its body cannot
/// be read.
///
/// # Panics
/// Panics when a fixture fails to build, decode, or bind, or a request,
/// header, model, outcome, error category, or request count differs from the
/// asserted gateway contract.
#[tokio::test]
async fn public_gateway_call_context_and_errors() {
    // Concurrent calls on one caller each send only their own model.
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
            gateway_call(chat_request(), "openai/gpt-a", Duration::from_secs(5)),
            &token,
        ),
        caller.call(
            gateway_call(chat_request(), "openai/gpt-c", Duration::from_secs(9)),
            &token,
        ),
        caller.call(
            gateway_call(chat_request(), "openai/gpt-e", Duration::from_secs(5)),
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
    let mut models: Vec<String> = requests
        .iter()
        .map(|request| {
            assert!(request.headers.contains_key("x-wyrd-access-token"));
            assert_eq!(
                request
                    .headers
                    .get("wyrd-prompt-provider")
                    .map(reqwest::header::HeaderValue::as_bytes),
                Some(&b"openai"[..]),
                "each call binds its initial target to its Prompt provider"
            );
            let body: Value = serde_json::from_slice(&request.body).expect("JSON body");
            body["model"].as_str().expect("model").to_owned()
        })
        .collect();
    models.sort();
    assert_eq!(models, ["openai/gpt-a", "openai/gpt-c", "openai/gpt-e"]);

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
    // Google deployments are named, not inferred: one is named `vertex`, and
    // a slash-bearing model suffix stays one encoded path segment.
    for route in [
        "/v1beta/models/gemini/gemini-a:generateContent",
        "/v1beta/models/vertex/tuned%2Fgemini-a:generateContent",
    ] {
        mount(
            &server,
            "POST",
            route,
            ResponseTemplate::new(200).set_body_json(json!({
                "candidates": [{"content": {"role": "model", "parts": [{"text": "ok"}]}}]
            })),
            1,
        )
        .await;
    }
    mount(
        &server,
        "POST",
        "/v1/systemone",
        ResponseTemplate::new(200).set_body_json(json!({
            "model": "jev-1", "answers": {"ok": {"type": "noul", "noul": 0.9}}
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
            .call(gateway_call(responses, "openai/gpt-r", deadline), &token)
            .await,
        Ok(ProviderResponse::OpenAiResponses(_))
    ));
    assert!(matches!(
        caller
            .call(
                gateway_call(anthropic, "anthropic/claude-a", deadline),
                &token
            )
            .await,
        Ok(ProviderResponse::AnthropicMessage(_))
    ));
    assert!(matches!(
        caller
            .call(
                gateway_call(gemini.clone(), "gemini/gemini-a", deadline),
                &token
            )
            .await,
        Ok(ProviderResponse::GeminiGenerateContent(_))
    ));
    assert!(matches!(
        caller
            .call(
                gateway_call(gemini.clone(), "vertex/tuned/gemini-a", deadline),
                &token
            )
            .await,
        Ok(ProviderResponse::GeminiGenerateContent(_))
    ));
    let jev = ProviderRequest::JevSystemOne(
        serde_json::from_value(json!({
            "model": "body-model", "state": "hi",
            "questions": {"ok": {"type": "noul", "instructions": "Is it ok?"}}
        }))
        .expect("jev"),
    );
    assert!(matches!(
        caller
            .call(gateway_call(jev, "jev/jev-latest", deadline), &token)
            .await,
        Ok(ProviderResponse::JevSystemOne(_))
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
        vec![
            json!("openai/gpt-r"),
            json!("anthropic/claude-a"),
            Value::Null,
            Value::Null,
            json!("jev/jev-latest"),
        ]
    );

    // An already cancelled run stops before any request; a slow answer times
    // out at the call's own deadline.
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    let stopped = caller
        .call(
            gateway_call(chat_request(), "openai/gpt-a", deadline),
            &cancelled,
        )
        .await
        .expect_err("a cancelled run stops the call");
    assert_eq!(stopped.code(), "SKALD_PROVIDERS_408_TIMEOUT");
    assert_eq!(received(&server).await.len(), 5, "nothing else was sent");
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
            gateway_call(chat_request(), "openai/gpt-a", Duration::from_millis(100)),
            &token,
        )
        .await
        .expect_err("the call's deadline bounds it");
    assert_eq!(timed_out.code(), "SKALD_PROVIDERS_408_TIMEOUT");

    // Cancelling the run once its model call has reached the gateway stops the
    // pending call long before the call's own deadline, without a resend.
    let (base_url, mut paths) = raw_server(vec![Reply::Hold(String::new())]).await;
    let pending = PublicWyrdGatewayCaller::new(client(&base_url));
    let cancellation = CancellationToken::new();
    let call = pending.call(
        gateway_call(chat_request(), "openai/gpt-a", Duration::from_mins(5)),
        &cancellation,
    );
    let cancel_once_dispatched = async {
        assert_eq!(paths.recv().await.as_deref(), Some("/v1/chat/completions"));
        cancellation.cancel();
    };
    let (outcome, ()) = tokio::time::timeout(Duration::from_secs(30), async {
        tokio::join!(call, cancel_once_dispatched)
    })
    .await
    .expect("cancellation, not the 300 s deadline, ends the call");
    let stopped = outcome.expect_err("a cancelled run stops the pending call");
    assert_eq!(stopped.code(), "SKALD_PROVIDERS_408_TIMEOUT");
    assert!(
        reported(&mut paths).is_empty(),
        "the model call is not resent"
    );

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
                gateway_call(chat_request(), "openai/gpt-a", deadline),
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
                gateway_call(anthropic_request, "anthropic/claude-a", deadline),
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
        "/v1beta/models/gemini/gemini-a:generateContent",
        json!({"error": {
            "code": 504, "message": "echoed prompt sk-canary", "status": "DEADLINE_EXCEEDED",
            "details": [{"@type": "type.googleapis.com/google.rpc.ErrorInfo",
                "reason": "WYRD_GATEWAY_504_DEADLINE_EXCEEDED", "domain": "wyrd"}]
        }}),
    )
    .await;
    let refused = problem(
        PublicWyrdGatewayCaller::new(client(&google.uri()))
            .call(gateway_call(gemini, "gemini/gemini-a", deadline), &token)
            .await,
    );
    assert_eq!(
        refused,
        catalog_problem("WYRD_GATEWAY_504_DEADLINE_EXCEEDED", 504, None)
    );
    let jev = refusal(
        404,
        "/v1/systemone",
        json!({"error": {
            "message": "echoed prompt sk-canary", "type": "invalid_request_error",
            "param": "model", "code": "WYRD_GATEWAY_404_MODEL_UNAVAILABLE"
        }}),
    )
    .await;
    let jev_request = ProviderRequest::JevSystemOne(
        serde_json::from_value(json!({
            "model": "body-model", "state": "hi",
            "questions": {"ok": {"type": "noul", "instructions": "Is it ok?"}}
        }))
        .expect("jev"),
    );
    let refused = problem(
        PublicWyrdGatewayCaller::new(client(&jev.uri()))
            .call(
                gateway_call(jev_request, "jev/jev-latest", deadline),
                &token,
            )
            .await,
    );
    assert_eq!(
        refused,
        catalog_problem("WYRD_GATEWAY_404_MODEL_UNAVAILABLE", 404, Some("model"))
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
                    gateway_call(chat_request(), "openai/gpt-a", deadline),
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
                gateway_call(chat_request(), "openai/gpt-a", deadline),
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
                gateway_call(chat_request(), "openai/gpt-a", deadline),
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
                gateway_call(chat_request(), "openai/gpt-a", deadline),
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
                gateway_call(chat_request(), "openai/gpt-a", deadline),
                &token,
            )
            .await,
    );
    assert_eq!(
        (refused.code.as_str(), refused.status),
        ("WYRD_AUTH_401_API_KEY_INVALID", 401)
    );
    assert_eq!(model_posts(&unrenewable).await.len(), 1);

    // A 401 whose body is cut off after its headers still renews the
    // credential, without resending the model call, and then reports the
    // unread body as a transport failure.
    let (base_url, mut paths) = raw_server(vec![
        Reply::Send(raw_reply("200 OK", &token_json("tok-a"))),
        Reply::Send(format!("{UNAUTHORIZED_HEAD}{{\"error\"")),
        Reply::Send(raw_reply("200 OK", &token_json("tok-b"))),
    ])
    .await;
    let outcome = PublicWyrdGatewayCaller::new(api_key_client(&base_url))
        .call(
            gateway_call(chat_request(), "openai/gpt-a", deadline),
            &token,
        )
        .await;
    assert!(
        matches!(outcome, Err(ProviderError::Connect { .. })),
        "{outcome:?}"
    );
    assert_eq!(
        reported(&mut paths),
        ["/auth/token", "/v1/chat/completions", "/auth/token"],
        "one model POST, then one renewal"
    );

    // A 401 whose body never finishes arriving renews the credential as soon
    // as the status line is read. The renewal request reaching the server
    // while the model call's body is still pending proves renewal does not
    // wait for the body; the caller's deadline then ends the call.
    let (base_url, mut paths) = raw_server(vec![
        Reply::Send(raw_reply("200 OK", &token_json("tok-a"))),
        Reply::Hold(format!("{UNAUTHORIZED_HEAD}{{\"error\"")),
        Reply::Send(raw_reply("200 OK", &token_json("tok-b"))),
    ])
    .await;
    let outcome = PublicWyrdGatewayCaller::new(api_key_client(&base_url))
        .call(
            gateway_call(chat_request(), "openai/gpt-a", Duration::from_secs(2)),
            &token,
        )
        .await;
    assert!(
        matches!(outcome, Err(ProviderError::Timeout { .. })),
        "{outcome:?}"
    );
    assert_eq!(
        reported(&mut paths),
        ["/auth/token", "/v1/chat/completions", "/auth/token"],
        "renewal runs while the 401 body is still pending"
    );
}
