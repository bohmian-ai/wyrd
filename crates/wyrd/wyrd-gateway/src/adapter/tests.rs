//! Adapter translation and HTTP dispatch tests against local mock providers.

use std::fmt::Write as _;
use std::num::NonZeroU32;
use std::time::Duration;

use base64::Engine as _;
use serde_json::{Value, json};
use skald_providers::{MediaAnswer, OpenAiMediaRoute, UploadContent, UploadFile};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_spec::gateway::{
    GATEWAY_JSON_MAX_BYTES, GatewayCallId, GatewayCallOutcome, GatewayDecimal, GatewayOperation,
    GatewayUsageAmount, ModelRef, ProviderDeployment,
};

use super::{
    BuiltinEndpoints, HttpProviderDispatch, IngressDialect, MediaRequest, Prepared, prepare,
};
use crate::credential::ProviderSecret;
use crate::endpoint::EndpointPolicy;
use crate::engine::{
    AttemptResult, AttemptUsage, FailureClass, ProviderAttempt, ProviderDispatch, ResponseBody,
    ResponseCapture, StreamEnd,
};

/// Parsed JSON of a buffered answer body.
///
/// # Panics
///
/// Panics when the body is an event stream or not JSON.
fn parsed(body: &ResponseBody) -> Value {
    let ResponseBody::Json(raw) = body else {
        panic!("buffered body expected");
    };
    serde_json::from_str(raw.get()).expect("json body")
}

/// Deployment of `model` (`provider/native`) with `adapter` and `auth` JSON.
fn deployment(model: &str, adapter: Value, auth: Value) -> ProviderDeployment {
    let mut value = json!({
        "name": "dep",
        "model": ModelRef::from_projection(model).expect("model"),
        "capabilities": ["chat_completions", "embeddings"],
        "routing_weight": 1,
    });
    value["adapter"] = adapter;
    value["auth"] = auth;
    serde_json::from_value(value).expect("deployment")
}

/// Bearer authentication through credential `cred`.
fn bearer() -> Value {
    json!({"bearer": {"credential": "cred"}})
}

/// API-key header authentication through credential `cred`.
fn api_key(name: &str) -> Value {
    json!({"api_key_header": {"header": name, "credential": "cred"}})
}

/// Dispatch whose built-in endpoints all point at `server`, with loopback
/// permitted by the non-production endpoint policy.
fn dispatch(server: &MockServer) -> HttpProviderDispatch {
    let base = url::Url::parse(&server.uri()).expect("mock url");
    let mut openai = base.clone();
    openai.set_path("/v1");
    HttpProviderDispatch::new(
        EndpointPolicy::new(false),
        BuiltinEndpoints {
            openai: Some(openai),
            anthropic: Some(base.clone()),
            gemini: Some(base.clone()),
            vertex: Some(base),
        },
    )
    .expect("dispatch builds")
}

/// Runs one buffered chat attempt of `body` in `ingress` against
/// `deployment`.
async fn attempt(
    dispatch: &HttpProviderDispatch,
    ingress: IngressDialect,
    deployment: &ProviderDeployment,
    credential: Option<&str>,
    body: &Value,
) -> AttemptResult {
    attempt_mode(dispatch, ingress, deployment, credential, body, false).await
}

/// Runs one chat attempt of `body` in `ingress` against `deployment`,
/// streamed when `stream` is set.
async fn attempt_mode(
    dispatch: &HttpProviderDispatch,
    ingress: IngressDialect,
    deployment: &ProviderDeployment,
    credential: Option<&str>,
    body: &Value,
    stream: bool,
) -> AttemptResult {
    let secret = credential.map(|value| ProviderSecret::new(value.to_owned().into()));
    let cancel = CancellationToken::new();
    dispatch
        .dispatch(ProviderAttempt {
            call_id: GatewayCallId::from_uuid(uuid::Uuid::from_u128(0xabc)),
            ordinal: NonZeroU32::MIN,
            operation: GatewayOperation::ChatCompletions,
            ingress,
            deployment,
            credential: secret.as_ref(),
            body,
            media: None,
            batch: None,
            stream,
            capture: false,
            deadline: Instant::now() + Duration::from_secs(10),
            cancel: &cancel,
        })
        .await
}

/// Streams one attempt and collects every relayed byte, then the terminal
/// result the relay reported once the upstream ended.
///
/// # Panics
///
/// Panics when the attempt does not open an event stream or the stream ends
/// unterminated.
async fn streamed(
    dispatch: &HttpProviderDispatch,
    ingress: IngressDialect,
    deployment: &ProviderDeployment,
    credential: &str,
    body: &Value,
) -> (Vec<u8>, StreamEnd) {
    let result = attempt_mode(dispatch, ingress, deployment, Some(credential), body, true).await;
    let (bytes, aborted, end) = collect(result).await;
    assert!(!aborted, "stream ends terminated");
    (bytes, end)
}

/// Normalized usage of a stream that ended successfully, `None` otherwise.
fn normalized(end: &StreamEnd) -> Option<Vec<GatewayUsageAmount>> {
    (end.outcome == GatewayCallOutcome::Succeeded)
        .then(|| end.usage.normalized.clone())
        .flatten()
}

/// Collects an opened event stream: every relayed byte, whether it ended
/// unterminated so the caller must abort, and the terminal result the relay
/// reported.
///
/// # Panics
///
/// Panics when `result` is not an event stream or the relay reports no end.
async fn collect(result: AttemptResult) -> (Vec<u8>, bool, StreamEnd) {
    let AttemptResult::Completed {
        body: ResponseBody::Events(mut events) | ResponseBody::MediaStream { mut events, .. },
        ..
    } = result
    else {
        panic!("event stream expected, got {result:?}");
    };
    let end = events.end.take().expect("end receiver");
    let mut bytes = Vec::new();
    let mut aborted = false;
    while let Some(frame) = events.recv().await {
        match frame {
            Ok(frame) => bytes.extend(frame),
            Err(_) => aborted = true,
        }
    }
    (
        bytes,
        aborted,
        end.await.expect("the relay reports its end"),
    )
}

/// Serves one streaming answer that sends `frame` and then holds the
/// connection open; the task reports whether the gateway closed it.
///
/// # Panics
///
/// Panics when the listener cannot bind, accept, read, or write.
async fn held_stream(frame: &'static str) -> (u16, JoinHandle<bool>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let upstream = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let mut request = Vec::new();
        let mut buffer = [0_u8; 4096];
        // Read headers, then the declared body.
        loop {
            let read = socket.read(&mut buffer).await.expect("read");
            request.extend_from_slice(&buffer[..read]);
            let text = String::from_utf8_lossy(&request);
            if let Some(end) = text.find("\r\n\r\n") {
                let length = text[..end]
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|value| value.trim().parse::<usize>().expect("length"))
                    })
                    .unwrap_or(0);
                if request.len() >= end + 4 + length {
                    break;
                }
            }
        }
        let head = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\n\r\n{:x}\r\n{frame}\r\n",
            frame.len()
        );
        socket.write_all(head.as_bytes()).await.expect("write");
        // The connection stays open until the gateway drops it.
        matches!(socket.read(&mut buffer).await, Ok(0) | Err(_))
    });
    (port, upstream)
}

/// Mounts a server-sent event answer for `POST route`.
async fn events(server: &MockServer, route: &str, body: &str) {
    Mock::given(method("POST"))
        .and(path(route))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "text/event-stream"))
        .mount(server)
        .await;
}

/// Normalized input and output token usage.
fn tokens(input: u64, output: u64) -> Vec<GatewayUsageAmount> {
    let amount = |dimension: &str, quantity: u64| GatewayUsageAmount {
        dimension: dimension.to_owned(),
        unit: "tokens".to_owned(),
        quantity: GatewayDecimal::new(&quantity.to_string()).expect("decimal"),
    };
    vec![
        amount("input_tokens", input),
        amount("output_tokens", output),
    ]
}

/// Single JSON body the mock provider received.
async fn received(server: &MockServer) -> Value {
    let requests = server.received_requests().await.expect("recording");
    assert_eq!(requests.len(), 1, "exactly one upstream request");
    serde_json::from_slice(&requests[0].body).expect("json body")
}

/// Mounts a JSON answer for `POST route` requiring header `name: value`.
async fn answer(server: &MockServer, route: &str, name: &str, value: &str, body: Value) {
    Mock::given(method("POST"))
        .and(path(route))
        .and(header(name, value))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

/// An OpenAI-compatible deployment receives the caller body unchanged except
/// for the native model, authenticated only by the provider credential, and
/// its usage is normalized and kept canonically.
#[tokio::test]
async fn compatible_passthrough_rewrites_model_and_uses_provider_credential() {
    let server = MockServer::start().await;
    let reply = json!({"id": "c1", "choices": [], "usage": {"total_tokens": 8, "prompt_tokens": 3, "completion_tokens": 5}});
    answer(
        &server,
        "/compat/chat/completions",
        "authorization",
        "Bearer sk-deepseek",
        reply.clone(),
    )
    .await;
    let target = deployment(
        "deepseek/deepseek-chat",
        json!({"openai_compatible": {"base_url": format!("{}/compat/", server.uri())}}),
        bearer(),
    );
    let body = json!({"model": "deepseek/deepseek-chat", "messages": [{"role": "user", "content": "hi"}], "thinking": {"type": "enabled"}});
    let result = attempt(
        &dispatch(&server),
        IngressDialect::OpenAi,
        &target,
        Some("sk-deepseek"),
        &body,
    )
    .await;
    let AttemptResult::Completed {
        body: answer,
        usage,
        ..
    } = result
    else {
        panic!("completion expected, got {result:?}");
    };
    assert_eq!(parsed(&answer), reply);
    assert_eq!(
        usage,
        AttemptUsage {
            provider_usage_json: Some(
                r#"{"completion_tokens":5,"completion_tokens_details":null,"prompt_tokens":3,"prompt_tokens_details":null,"total_tokens":8}"#
                    .to_owned()
            ),
            normalized: Some(tokens(3, 5)),
        }
    );
    let mut expected = body;
    expected["model"] = json!("deepseek-chat");
    assert_eq!(received(&server).await, expected);
    let requests = server.received_requests().await.expect("recording");
    assert!(
        requests[0]
            .headers
            .keys()
            .all(|name| !name.as_str().starts_with("wyrd-")),
        "no gateway or caller headers reach the provider"
    );
}

/// Noncanonical native answer bytes reach the caller unchanged.
#[tokio::test]
async fn native_answer_bytes_are_preserved() {
    let server = MockServer::start().await;
    let reply = "{ \"id\" : \"c1\",\"choices\":[ ],\"x\":\"\\u00e9\", \"usage\":{\"prompt_tokens\":3,\"completion_tokens\":5} }";
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(reply))
        .mount(&server)
        .await;
    let openai = deployment("openai/gpt-5", json!("openai"), bearer());
    let result = attempt(
        &dispatch(&server),
        IngressDialect::OpenAi,
        &openai,
        Some("sk"),
        &json!({"model": "m", "messages": []}),
    )
    .await;
    let AttemptResult::Completed {
        body: ResponseBody::Json(body),
        ..
    } = result
    else {
        panic!("buffered completion expected, got {result:?}");
    };
    assert_eq!(body.get(), reply);
}

/// Native Anthropic, Gemini, and Vertex bodies pass through to their own
/// adapter with that adapter's route and authentication.
#[tokio::test]
async fn native_dialects_pass_through_with_adapter_routes_and_auth() {
    let server = MockServer::start().await;
    let messages = json!({"id": "m1", "type": "message", "role": "assistant", "content": [], "usage": {"input_tokens": 2, "output_tokens": 1, "cache_read_input_tokens": 4}});
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("x-api-key", "sk-ant"))
        .and(header("anthropic-version", "2023-06-01"))
        .respond_with(ResponseTemplate::new(200).set_body_json(messages.clone()))
        .mount(&server)
        .await;
    let google = json!({"candidates": [], "usageMetadata": {"promptTokenCount": 7, "candidatesTokenCount": 2, "thoughtsTokenCount": 1}});
    answer(
        &server,
        "/v1beta/models/gemini-2.5-flash:generateContent",
        "x-goog-api-key",
        "g-key",
        google.clone(),
    )
    .await;
    answer(
        &server,
        "/v1/projects/acme/locations/us-central1/publishers/google/models/gemini-2.5-pro:generateContent",
        "authorization",
        "Bearer ya29",
        google.clone(),
    )
    .await;
    let dispatch = dispatch(&server);
    let anthropic = deployment(
        "anthropic/claude-sonnet-5",
        json!("anthropic"),
        api_key("x-api-key"),
    );
    let body = json!({"model": "claude", "max_tokens": 10, "messages": [], "vendor": 1});
    let result = attempt(
        &dispatch,
        IngressDialect::AnthropicMessages,
        &anthropic,
        Some("sk-ant"),
        &body,
    )
    .await;
    assert!(
        matches!(&result, AttemptResult::Completed { body, usage, .. } if parsed(body) == messages && usage.normalized == Some(tokens(6, 1))),
        "{result:?}"
    );

    let gemini = deployment(
        "gemini/gemini-2.5-flash",
        json!("gemini"),
        api_key("x-goog-api-key"),
    );
    let native = json!({"contents": [{"role": "user", "parts": [{"text": "hi"}]}]});
    let result = attempt(
        &dispatch,
        IngressDialect::GeminiGenerateContent,
        &gemini,
        Some("g-key"),
        &native,
    )
    .await;
    assert!(
        matches!(&result, AttemptResult::Completed { body, usage, .. } if parsed(body) == google && usage.normalized == Some(tokens(7, 3))),
        "{result:?}"
    );

    let vertex = deployment(
        "vertex/gemini-2.5-pro",
        json!({"vertex": {"project": "acme", "location": "us-central1"}}),
        bearer(),
    );
    let result = attempt(
        &dispatch,
        IngressDialect::VertexGenerateContent,
        &vertex,
        Some("ya29"),
        &native,
    )
    .await;
    assert!(
        matches!(&result, AttemptResult::Completed { .. }),
        "{result:?}"
    );
    let bodies: Vec<Value> = server
        .received_requests()
        .await
        .expect("recording")
        .iter()
        .map(|request| serde_json::from_slice(&request.body).expect("json"))
        .collect();
    assert_eq!(bodies[0]["model"], "claude-sonnet-5");
    assert_eq!(bodies[0]["vendor"], 1);
    assert_eq!(bodies[1..], [native.clone(), native]);
}

/// `OpenAI` Chat Completions ingress translates to Anthropic Messages and the
/// answer translates back, including tool calls and cache usage.
#[tokio::test]
async fn openai_chat_translates_to_anthropic_and_back() {
    let server = MockServer::start().await;
    answer(&server, "/v1/messages", "x-api-key", "sk-ant", json!({
        "id": "msg_1", "type": "message", "role": "assistant", "model": "claude-sonnet-5",
        "content": [{"type": "text", "text": "Checking."}, {"type": "tool_use", "id": "toolu_1", "name": "weather", "input": {"city": "Oslo"}}],
        "stop_reason": "tool_use", "stop_sequence": null,
        "usage": {"input_tokens": 10, "output_tokens": 4, "cache_read_input_tokens": 2}
    })).await;
    let target = deployment(
        "anthropic/claude-sonnet-5",
        json!("anthropic"),
        api_key("x-api-key"),
    );
    let body = json!({
        "model": "anthropic/claude-sonnet-5", "max_completion_tokens": 256, "temperature": 0.5, "stop": "END", "user": "u1",
        "messages": [
            {"role": "system", "content": "Be brief."},
            {"role": "user", "content": [{"type": "text", "text": "Weather?"}, {"type": "image_url", "image_url": {"url": "data:image/png;base64,iVBO"}}]},
            {"role": "assistant", "content": null, "tool_calls": [{"id": "call_1", "type": "function", "function": {"name": "weather", "arguments": "{\"city\":\"Oslo\"}"}}]},
            {"role": "tool", "tool_call_id": "call_1", "content": "sunny"},
            {"role": "user", "content": "And tomorrow?"}
        ],
        "tools": [{"type": "function", "function": {"name": "weather", "parameters": {"type": "object"}}}],
        "tool_choice": "auto", "parallel_tool_calls": false
    });
    let result = attempt(
        &dispatch(&server),
        IngressDialect::OpenAi,
        &target,
        Some("sk-ant"),
        &body,
    )
    .await;
    assert_eq!(
        received(&server).await,
        json!({
            "model": "claude-sonnet-5", "max_tokens": 256, "temperature": 0.5, "stop_sequences": ["END"], "metadata": {"user_id": "u1"},
            "system": "Be brief.",
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "Weather?"}, {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "iVBO"}}]},
                {"role": "assistant", "content": [{"type": "tool_use", "id": "call_1", "name": "weather", "input": {"city": "Oslo"}}]},
                {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "call_1", "content": "sunny"}, {"type": "text", "text": "And tomorrow?"}]}
            ],
            "tools": [{"name": "weather", "input_schema": {"type": "object"}}],
            "tool_choice": {"type": "auto", "disable_parallel_tool_use": true}
        })
    );
    let AttemptResult::Completed { body, usage, .. } = result else {
        panic!("translated completion expected, got {result:?}");
    };
    let body = parsed(&body);
    assert_eq!(body["id"], "msg_1");
    assert_eq!(body["object"], "chat.completion");
    assert_eq!(body["model"], "claude-sonnet-5");
    assert_eq!(
        body["choices"],
        json!([{"index": 0, "finish_reason": "tool_calls", "logprobs": null, "message": {
            "role": "assistant", "content": "Checking.",
            "tool_calls": [{"id": "toolu_1", "type": "function", "function": {"name": "weather", "arguments": "{\"city\":\"Oslo\"}"}}]
        }}])
    );
    assert_eq!(
        body["usage"],
        json!({"prompt_tokens": 12, "completion_tokens": 4, "total_tokens": 16, "prompt_tokens_details": {"audio_tokens": 0, "cached_tokens": 2}, "completion_tokens_details": null})
    );
    assert_eq!(usage.normalized, Some(tokens(12, 4)));
}

/// `OpenAI` Chat Completions ingress translates to Gemini `GenerateContent`,
/// and a camelCase answer with thought parts and extra envelope members
/// translates back to one choice per candidate.
#[tokio::test]
async fn openai_chat_translates_to_gemini_and_back() {
    let server = MockServer::start().await;
    answer(&server, "/v1beta/models/gemini-2.5-flash:generateContent", "x-goog-api-key", "g-key", json!({
        "candidates": [
            {"content": {"role": "model", "parts": [{"text": "hmm", "thought": true}, {"text": "Sunny"}]}, "finishReason": "STOP", "index": 0},
            {"content": {"role": "model", "parts": [{"functionCall": {"name": "weather", "args": {"city": "Oslo"}}}]}, "finishReason": "STOP", "index": 1}
        ],
        "usageMetadata": {"promptTokenCount": 10, "candidatesTokenCount": 4, "totalTokenCount": 16, "thoughtsTokenCount": 2, "promptTokensDetails": [{"modality": "TEXT", "tokenCount": 10}]},
        "modelVersion": "gemini-2.5-flash", "responseId": "r1"
    })).await;
    let target = deployment(
        "gemini/gemini-2.5-flash",
        json!("gemini"),
        api_key("x-goog-api-key"),
    );
    let body = json!({
        "model": "gemini/gemini-2.5-flash", "n": 2, "max_tokens": 64, "response_format": {"type": "json_object"},
        "messages": [
            {"role": "developer", "content": "Be brief."},
            {"role": "user", "content": "Weather?"},
            {"role": "assistant", "tool_calls": [{"id": "call_1", "type": "function", "function": {"name": "weather", "arguments": "{\"city\":\"Oslo\"}"}}]},
            {"role": "tool", "tool_call_id": "call_1", "content": "{\"temp\":20}"}
        ],
        "tools": [{"type": "function", "function": {"name": "weather", "parameters": {"type": "object"}}}],
        "tool_choice": {"type": "function", "function": {"name": "weather"}}
    });
    let result = attempt(
        &dispatch(&server),
        IngressDialect::OpenAi,
        &target,
        Some("g-key"),
        &body,
    )
    .await;
    assert_eq!(
        received(&server).await,
        json!({
            "contents": [
                {"role": "user", "parts": [{"text": "Weather?"}]},
                {"role": "model", "parts": [{"function_call": {"name": "weather", "args": {"city": "Oslo"}}}]},
                {"role": "user", "parts": [{"function_response": {"name": "weather", "response": {"temp": 20}}}]}
            ],
            "system_instruction": {"role": "user", "parts": [{"text": "Be brief."}]},
            "tools": [{"function_declarations": [{"name": "weather", "parameters": {"type": "object"}}]}],
            "tool_config": {"function_calling_config": {"mode": "ANY", "allowed_function_names": ["weather"]}},
            "generation_config": {"candidate_count": 2, "max_output_tokens": 64, "response_mime_type": "application/json"}
        })
    );
    let AttemptResult::Completed { body, usage, .. } = result else {
        panic!("translated completion expected, got {result:?}");
    };
    let body = parsed(&body);
    assert_eq!(body["id"], "chatcmpl-00000000000000000000000000000abc");
    assert_eq!(
        body["choices"],
        json!([
            {"index": 0, "finish_reason": "stop", "logprobs": null, "message": {"role": "assistant", "content": "Sunny"}},
            {"index": 1, "finish_reason": "tool_calls", "logprobs": null, "message": {"role": "assistant", "tool_calls": [
                {"id": "call_1_0", "type": "function", "function": {"name": "weather", "arguments": "{\"city\":\"Oslo\"}"}}
            ]}}
        ])
    );
    assert_eq!(body["usage"]["completion_tokens"], 6);
    assert_eq!(
        body["usage"]["completion_tokens_details"]["reasoning_tokens"],
        2
    );
    assert_eq!(usage.normalized, Some(tokens(10, 6)));
}

/// Compatible, Anthropic, and Gemini deployments of one model each.
fn targets() -> [ProviderDeployment; 3] {
    [
        deployment(
            "acme/m",
            json!({"openai_compatible": {"base_url": "https://acme.example/v1"}}),
            bearer(),
        ),
        deployment("anthropic/claude", json!("anthropic"), bearer()),
        deployment("gemini/flash", json!("gemini"), bearer()),
    ]
}

/// Chat features a translated deployment cannot represent faithfully are
/// rejected by preparation, naming the member, instead of being dropped or
/// approximated.
#[test]
fn unrepresentable_chat_features_are_rejected_with_the_offending_member() {
    let [compatible, anthropic, gemini] = targets();
    let image = json!([{"type": "image_url", "image_url": {"url": "https://x.example/a.png"}}]);
    let audio = json!([{"type": "input_audio", "input_audio": {"data": "AA", "format": "wav"}}]);
    let cases = [
        (&compatible, json!({"stream": true}), "stream"),
        (&anthropic, json!({"n": 2}), "n"),
        (
            &anthropic,
            json!({"max_tokens": null}),
            "max_completion_tokens",
        ),
        (&anthropic, json!({"temperature": 1.5}), "temperature"),
        (&anthropic, json!({"logprobs": true}), "logprobs"),
        (&anthropic, json!({"vendor_flag": true}), "vendor_flag"),
        (
            &anthropic,
            json!({"messages": [{"role": "user", "content": "a"}, {"role": "system", "content": "late"}]}),
            "messages[1].role",
        ),
        (
            &anthropic,
            json!({"messages": [{"role": "user", "name": "bob", "content": "a"}]}),
            "messages[0].name",
        ),
        (&gemini, json!({"user": "u"}), "user"),
        (
            &gemini,
            json!({"messages": [{"role": "user", "content": image}]}),
            "messages[0].content[0].image_url.url",
        ),
        (
            &gemini,
            json!({"messages": [{"role": "user", "content": audio}]}),
            "messages[0].content[0]",
        ),
        (
            &gemini,
            json!({"messages": [{"role": "tool", "tool_call_id": "nope", "content": "x"}]}),
            "messages[0].tool_call_id",
        ),
    ];
    for (target, extra, field) in cases {
        let mut body =
            json!({"model": "m", "max_tokens": 5, "messages": [{"role": "user", "content": "hi"}]});
        body.as_object_mut()
            .expect("object")
            .extend(extra.as_object().expect("object").clone());
        let error = prepare(
            IngressDialect::OpenAi,
            GatewayOperation::ChatCompletions,
            false,
            &body,
            target,
            None,
        )
        .expect_err(field);
        assert_eq!(error.field, field, "{error}");
    }
}

/// Undeclared operations and native bodies for a different dialect are
/// rejected, while supported `OpenAI` routes pass through with the native
/// model.
#[test]
fn operations_and_dialects_are_matched_to_the_adapter() {
    let [compatible, anthropic, gemini] = targets();
    let cases = [
        (
            IngressDialect::OpenAi,
            GatewayOperation::Embeddings,
            &anthropic,
            "operation",
        ),
        (
            IngressDialect::OpenAi,
            GatewayOperation::Images,
            &compatible,
            "operation",
        ),
        (
            IngressDialect::AnthropicMessages,
            GatewayOperation::ChatCompletions,
            &gemini,
            "body",
        ),
        (
            IngressDialect::GeminiGenerateContent,
            GatewayOperation::ChatCompletions,
            &compatible,
            "body",
        ),
    ];
    for (ingress, operation, target, field) in cases {
        let error = prepare(
            ingress,
            operation,
            false,
            &json!({"model": "m"}),
            target,
            None,
        )
        .expect_err(field);
        assert_eq!(error.field, field, "{error}");
    }
    let embeddings = prepare(
        IngressDialect::OpenAi,
        GatewayOperation::Embeddings,
        false,
        &json!({"model": "acme/m", "input": "hi"}),
        &compatible,
        None,
    )
    .expect("compatible embeddings pass through");
    assert_eq!(
        embeddings,
        Prepared::Native(json!({"model": "m", "input": "hi"}))
    );
}

/// Provider refusals keep native bodies and statuses, translated refusals use
/// the `OpenAI` error envelope, an untranslatable answer is a rejection, and a
/// missing credential, an authentication style the adapter does not take, or
/// a blocked compatible base URL never dispatches.
#[tokio::test]
async fn refusals_and_untranslatable_answers_fail_explicitly() {
    let server = MockServer::start().await;
    let limited = json!({"error": {"message": "slow down", "type": "rate_limit"}});
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(429).set_body_json(limited.clone()))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("x-api-key", "sk-bad"))
        .respond_with(ResponseTemplate::new(400).set_body_json(
            json!({"type": "error", "error": {"type": "invalid_request_error", "message": "bad image"}}),
        ))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("x-api-key", "sk-good"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "m", "type": "message", "role": "assistant", "model": "c", "content": [],
            "stop_reason": "pause_turn", "usage": {"input_tokens": 3, "output_tokens": 0}
        })))
        .mount(&server)
        .await;
    let dispatch = dispatch(&server);
    let chat =
        json!({"model": "m", "max_tokens": 5, "messages": [{"role": "user", "content": "hi"}]});

    let openai = deployment("openai/gpt-5", json!("openai"), bearer());
    let result = attempt(
        &dispatch,
        IngressDialect::OpenAi,
        &openai,
        Some("sk"),
        &chat,
    )
    .await;
    assert!(
        matches!(&result, AttemptResult::Refused { class: FailureClass::Upstream, status: 429, body, usage } if *body == limited && *usage == AttemptUsage::default()),
        "{result:?}"
    );

    let anthropic = deployment("anthropic/claude", json!("anthropic"), api_key("x-api-key"));
    let result = attempt(
        &dispatch,
        IngressDialect::OpenAi,
        &anthropic,
        Some("sk-bad"),
        &chat,
    )
    .await;
    assert!(
        matches!(&result, AttemptResult::Refused { class: FailureClass::Rejected, status: 400, body, usage } if *body == json!({"error": {"message": "bad image", "type": "invalid_request_error", "param": null, "code": null}}) && *usage == AttemptUsage::default()),
        "{result:?}"
    );

    let result = attempt(
        &dispatch,
        IngressDialect::OpenAi,
        &anthropic,
        Some("sk-good"),
        &chat,
    )
    .await;
    assert!(
        matches!(&result, AttemptResult::Failed { class: FailureClass::Rejected, usage } if usage.normalized == Some(tokens(3, 0))),
        "{result:?}"
    );

    let before = server.received_requests().await.expect("recording").len();
    let bearer_anthropic = deployment("anthropic/claude", json!("anthropic"), bearer());
    let metadata = deployment(
        "acme/m",
        json!({"openai_compatible": {"base_url": "http://169.254.169.254/v1"}}),
        bearer(),
    );
    for (target, credential) in [
        (&anthropic, None),
        (&bearer_anthropic, Some("sk-good")),
        (&metadata, Some("sk")),
    ] {
        let result = attempt(&dispatch, IngressDialect::OpenAi, target, credential, &chat).await;
        assert!(
            matches!(&result, AttemptResult::Failed { class: FailureClass::BeforeDispatch, usage } if *usage == AttemptUsage::default()),
            "{result:?}"
        );
    }
    assert_eq!(
        server.received_requests().await.expect("recording").len(),
        before
    );
}

/// Native `OpenAI`, Anthropic, Gemini, and Vertex streams relay upstream bytes
/// unchanged through a bounded channel and report the usage their events
/// carry once the upstream ends.
#[tokio::test]
async fn native_streams_relay_bytes_and_usage() {
    let server = MockServer::start().await;
    let openai_events = "data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"}}]}\n\ndata: {\"id\":\"c1\",\"choices\":[],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":5,\"total_tokens\":8}}\n\ndata: [DONE]\n\n";
    events(&server, "/v1/chat/completions", openai_events).await;
    let anthropic_events = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"m1\",\"usage\":{\"input_tokens\":4,\"output_tokens\":1}}}\n\nevent: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":6}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
    events(&server, "/v1/messages", anthropic_events).await;
    let google_events = "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"hi\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":7,\"candidatesTokenCount\":2}}\r\n\r\n";
    events(
        &server,
        "/v1beta/models/gemini-2.5-flash:streamGenerateContent",
        google_events,
    )
    .await;
    events(
        &server,
        "/v1/projects/acme/locations/us-central1/publishers/google/models/gemini-2.5-pro:streamGenerateContent",
        google_events,
    )
    .await;
    let dispatch = dispatch(&server);
    let native = json!({"contents": [{"role": "user", "parts": [{"text": "hi"}]}]});
    let cases = [
        (
            IngressDialect::OpenAi,
            deployment("openai/gpt-5", json!("openai"), bearer()),
            json!({"model": "m", "messages": [], "stream": true}),
            openai_events,
            tokens(3, 5),
        ),
        (
            IngressDialect::AnthropicMessages,
            deployment(
                "anthropic/claude-sonnet-5",
                json!("anthropic"),
                api_key("x-api-key"),
            ),
            json!({"model": "c", "max_tokens": 5, "messages": [], "stream": true}),
            anthropic_events,
            tokens(4, 6),
        ),
        (
            IngressDialect::GeminiGenerateContent,
            deployment(
                "gemini/gemini-2.5-flash",
                json!("gemini"),
                api_key("x-goog-api-key"),
            ),
            native.clone(),
            google_events,
            tokens(7, 2),
        ),
        (
            IngressDialect::VertexGenerateContent,
            deployment(
                "vertex/gemini-2.5-pro",
                json!({"vertex": {"project": "acme", "location": "us-central1"}}),
                bearer(),
            ),
            native,
            google_events,
            tokens(7, 2),
        ),
    ];
    for (ingress, target, body, expected, usage) in cases {
        let (bytes, end) = streamed(&dispatch, ingress, &target, "key", &body).await;
        assert_eq!(String::from_utf8(bytes).expect("utf-8"), expected);
        assert_eq!(normalized(&end), Some(usage));
        assert_eq!(end.capture, None, "unselected content is not kept");
    }
    let requests = server.received_requests().await.expect("recording");
    assert!(
        requests[2..]
            .iter()
            .all(|request| request.url.query() == Some("alt=sse")),
        "Google streams request server-sent events"
    );

    let responses_events = "event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"r1\",\"status\":\"in_progress\"}}\n\nevent: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"r1\",\"status\":\"completed\",\"usage\":{\"input_tokens\":2,\"output_tokens\":4,\"total_tokens\":6}}}\n\n";
    events(&server, "/v1/responses", responses_events).await;
    let (bytes, aborted, end) = open_stream(
        &dispatch,
        GatewayOperation::Responses,
        IngressDialect::OpenAi,
        deployment("openai/gpt-5", json!("openai"), bearer()),
        json!({"model": "m", "input": "hi", "tools": [{"type": "function", "name": "f", "parameters": {}}], "stream": true}),
        later(),
        CancellationToken::new(),
    )
    .await;
    assert!(
        !aborted,
        "response.completed terminates the Responses stream"
    );
    assert_eq!(String::from_utf8(bytes).expect("utf-8"), responses_events);
    assert_eq!(normalized(&end), Some(tokens(2, 4)));
    assert_eq!(
        received_nth(&server, 4).await["tools"][0]["name"],
        "f",
        "Responses tools pass through unchanged"
    );
}

/// Embeddings pass a batch through unchanged, so per-item order, dimensions,
/// and encoding reach the provider as submitted and its ordered answer returns
/// byte for byte with input-token usage; malformed, empty, oversized,
/// zero-dimension, unknown-encoding, and streamed requests are rejected before
/// dispatch.
///
/// # Panics
///
/// Panics when a batch is reordered or altered, usage is lost, or an invalid
/// request reaches the provider or is rejected under the wrong member.
#[tokio::test]
async fn embeddings_preserve_batches_and_reject_invalid_requests() {
    let server = MockServer::start().await;
    let answer = r#"{"object":"list","data":[{"object":"embedding","index":0,"embedding":[0.1,0.2]},{"object":"embedding","index":1,"embedding":[0.3,0.4]}],"model":"m","usage":{"prompt_tokens":5,"total_tokens":5}}"#;
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(answer, "application/json"))
        .mount(&server)
        .await;
    let dispatch = dispatch(&server);
    let target = deployment("openai/text-embedding-3-small", json!("openai"), bearer());
    let secret = ProviderSecret::new("key".to_owned().into());
    let cancel = CancellationToken::new();
    let body = json!({"model": "openai/text-embedding-3-small", "input": ["second", "first"], "dimensions": 2, "encoding_format": "float"});
    let result = dispatch
        .dispatch(ProviderAttempt {
            call_id: GatewayCallId::from_uuid(uuid::Uuid::from_u128(0xabc)),
            ordinal: NonZeroU32::MIN,
            operation: GatewayOperation::Embeddings,
            ingress: IngressDialect::OpenAi,
            deployment: &target,
            credential: Some(&secret),
            body: &body,
            media: None,
            batch: None,
            stream: false,
            capture: false,
            deadline: later(),
            cancel: &cancel,
        })
        .await;
    let AttemptResult::Completed {
        body: ResponseBody::Json(raw),
        usage,
        ..
    } = result
    else {
        panic!("embeddings complete: {result:?}");
    };
    assert_eq!(raw.get(), answer, "the ordered answer returns unchanged");
    let mut input_only = tokens(5, 0);
    input_only.truncate(1);
    assert_eq!(usage.normalized, Some(input_only));
    assert_eq!(
        received(&server).await,
        json!({"model": "text-embedding-3-small", "input": ["second", "first"], "dimensions": 2, "encoding_format": "float"})
    );

    let oversized = vec!["x"; 2049];
    for (request, stream, field) in [
        (json!({"model": "m", "input": 7}), false, "body"),
        (json!({"model": "m", "input": []}), false, "input"),
        (json!({"model": "m", "input": ["a", ""]}), false, "input"),
        (json!({"model": "m", "input": oversized}), false, "input"),
        (
            json!({"model": "m", "input": "a", "dimensions": 0}),
            false,
            "dimensions",
        ),
        (
            json!({"model": "m", "input": "a", "encoding_format": "int8"}),
            false,
            "encoding_format",
        ),
        (
            json!({"model": "m", "input": "a", "stream": true}),
            true,
            "stream",
        ),
    ] {
        let error = prepare(
            IngressDialect::OpenAi,
            GatewayOperation::Embeddings,
            stream,
            &request,
            &target,
            None,
        )
        .expect_err(field);
        assert_eq!(error.field, field, "{error}");
    }
}

/// Decodes the `data:` payloads of relayed frames.
fn payloads(bytes: &[u8]) -> Vec<String> {
    String::from_utf8(bytes.to_vec())
        .expect("utf-8")
        .split("\n\n")
        .filter_map(|frame| frame.strip_prefix("data: "))
        .map(str::to_owned)
        .collect()
}

/// Translated Anthropic and Gemini streams become `OpenAI` chat completion
/// chunks ending in an optional usage chunk and `[DONE]`, and report usage.
#[tokio::test]
async fn translated_streams_become_chat_completion_chunks() {
    let server = MockServer::start().await;
    events(&server, "/v1/messages", concat!(
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude\",\"usage\":{\"input_tokens\":10,\"output_tokens\":1}}}\n\n",
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
        "event: ping\ndata: {\"type\":\"ping\"}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hel\"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"lo\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":5}}\n\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
    )).await;
    events(&server, "/v1beta/models/gemini-2.5-flash:streamGenerateContent", concat!(
        "data: {\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"Sun\"}]},\"index\":0}]}\r\n\r\n",
        "data: {\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"ny\"}]},\"finishReason\":\"STOP\",\"index\":0}],\"usageMetadata\":{\"promptTokenCount\":8,\"candidatesTokenCount\":2}}\r\n\r\n",
    )).await;
    let dispatch = dispatch(&server);
    let anthropic = deployment(
        "anthropic/claude-sonnet-5",
        json!("anthropic"),
        api_key("x-api-key"),
    );
    let gemini = deployment(
        "gemini/gemini-2.5-flash",
        json!("gemini"),
        api_key("x-goog-api-key"),
    );
    let chat = |usage: bool| json!({"model": "m", "max_tokens": 5, "stream": true, "stream_options": {"include_usage": usage}, "messages": [{"role": "user", "content": "hi"}]});
    let summary = |bytes: &[u8]| {
        let mut frames = payloads(bytes);
        assert_eq!(frames.pop().as_deref(), Some("[DONE]"));
        frames
            .iter()
            .map(|frame| serde_json::from_str::<Value>(frame).expect("chunk"))
            .collect::<Vec<_>>()
    };

    let (bytes, end) = streamed(
        &dispatch,
        IngressDialect::OpenAi,
        &anthropic,
        "key",
        &chat(true),
    )
    .await;
    let chunks = summary(&bytes);
    assert!(chunks.iter().all(|chunk| chunk["id"] == "msg_1"
        && chunk["object"] == "chat.completion.chunk"
        && chunk["model"] == "claude-sonnet-5"));
    let deltas: Vec<Value> = chunks[..chunks.len() - 1]
        .iter()
        .map(|chunk| chunk["choices"][0].clone())
        .collect();
    assert_eq!(
        deltas,
        [
            json!({"index": 0, "delta": {"role": "assistant"}, "finish_reason": null, "logprobs": null}),
            json!({"index": 0, "delta": {"content": "Hel"}, "finish_reason": null, "logprobs": null}),
            json!({"index": 0, "delta": {"content": "lo"}, "finish_reason": null, "logprobs": null}),
            json!({"index": 0, "delta": {}, "finish_reason": "stop", "logprobs": null}),
        ]
    );
    assert_eq!(chunks[4]["choices"], json!([]));
    assert_eq!(chunks[4]["usage"]["prompt_tokens"], 10);
    assert_eq!(chunks[4]["usage"]["completion_tokens"], 5);
    assert_eq!(normalized(&end), Some(tokens(10, 5)));
    assert_eq!(received_nth(&server, 0).await["stream"], true);

    let (bytes, end) = streamed(
        &dispatch,
        IngressDialect::OpenAi,
        &gemini,
        "g-key",
        &chat(false),
    )
    .await;
    let chunks = summary(&bytes);
    assert_eq!(
        chunks
            .iter()
            .map(|chunk| chunk["choices"][0].clone())
            .collect::<Vec<_>>(),
        [
            json!({"index": 0, "delta": {"role": "assistant", "content": "Sun"}, "finish_reason": null, "logprobs": null}),
            json!({"index": 0, "delta": {"content": "ny"}, "finish_reason": "stop", "logprobs": null}),
        ],
        "no usage chunk unless requested"
    );
    assert_eq!(chunks[0]["id"], "chatcmpl-00000000000000000000000000000abc");
    assert_eq!(normalized(&end), Some(tokens(8, 2)));
}

/// JSON body of the `index`th request the mock provider received.
async fn received_nth(server: &MockServer, index: usize) -> Value {
    let requests = server.received_requests().await.expect("recording");
    serde_json::from_slice(&requests[index].body).expect("json body")
}

/// An untranslatable event ends a translated stream with the `OpenAI` error
/// frame and `[DONE]` instead of a clean success, and the attempt's usage
/// stays unknown.
///
/// # Panics
///
/// Panics when the stream succeeds, lacks the terminal error, or reports
/// usage.
#[tokio::test]
async fn untranslatable_stream_event_closes_without_usage() {
    let server = MockServer::start().await;
    events(&server, "/v1/messages", concat!(
        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}}\n\n",
        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\",\"signature\":\"\"}}\n\n",
        "data: {\"type\":\"message_stop\"}\n\n",
    )).await;
    let anthropic = deployment("anthropic/claude", json!("anthropic"), api_key("x-api-key"));
    let body = json!({"model": "m", "max_tokens": 5, "stream": true, "messages": [{"role": "user", "content": "hi"}]});
    let (bytes, end) = streamed(
        &dispatch(&server),
        IngressDialect::OpenAi,
        &anthropic,
        "key",
        &body,
    )
    .await;
    let frames = payloads(&bytes);
    assert_eq!(frames.last().map(String::as_str), Some("[DONE]"));
    let error: Value = serde_json::from_str(&frames[frames.len() - 2]).expect("error frame");
    assert_eq!(
        error["error"]["code"],
        "WYRD_GATEWAY_502_UPSTREAM_UNAVAILABLE"
    );
    assert_eq!(
        (end.outcome, end.usage),
        (GatewayCallOutcome::Failed, AttemptUsage::default())
    );
}

/// Streamed chat frame every abnormal-stop fixture sends before stopping.
const CHAT_FRAME: &str = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"}}]}\n\n";

/// Dispatches one streaming attempt through `dispatch` and collects its bytes,
/// whether it aborted, and its usage.
///
/// # Panics
///
/// Panics when the attempt does not open a stream.
async fn open_stream(
    dispatch: &HttpProviderDispatch,
    operation: GatewayOperation,
    ingress: IngressDialect,
    deployment: ProviderDeployment,
    body: Value,
    deadline: Instant,
    cancel: CancellationToken,
) -> (Vec<u8>, bool, StreamEnd) {
    let secret = ProviderSecret::new("key".to_owned().into());
    let result = dispatch
        .dispatch(ProviderAttempt {
            call_id: GatewayCallId::from_uuid(uuid::Uuid::from_u128(0xabc)),
            ordinal: NonZeroU32::MIN,
            operation,
            ingress,
            deployment: &deployment,
            credential: Some(&secret),
            body: &body,
            media: None,
            batch: None,
            stream: true,
            capture: false,
            deadline,
            cancel: &cancel,
        })
        .await;
    collect(result).await
}

/// Streaming chat request body.
fn chat_stream() -> Value {
    json!({"model": "m", "messages": [], "stream": true})
}

/// A deadline no abnormal-stop fixture reaches.
fn later() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

/// Truncation at an event boundary appends the ingress protocol's terminal
/// error: the `OpenAI` error frame and `[DONE]` for Chat Completions, and an
/// `error` event for Responses and Anthropic. Usage stays unknown.
///
/// # Panics
///
/// Panics when a truncated stream aborts, reports usage, or ends without its
/// exact terminal error.
#[tokio::test]
async fn truncation_at_an_event_boundary_ends_in_the_terminal_error() {
    let server = MockServer::start().await;
    events(&server, "/v1/chat/completions", CHAT_FRAME).await;
    events(
        &server,
        "/v1/responses",
        "event: response.created\ndata: {\"type\":\"response.created\"}\n\n",
    )
    .await;
    events(
        &server,
        "/v1/messages",
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"m\"}}\n\n",
    )
    .await;
    let dispatch = dispatch(&server);
    let openai = || deployment("openai/gpt-5", json!("openai"), bearer());

    let (bytes, aborted, end) = open_stream(
        &dispatch,
        GatewayOperation::ChatCompletions,
        IngressDialect::OpenAi,
        openai(),
        chat_stream(),
        later(),
        CancellationToken::new(),
    )
    .await;
    let text = String::from_utf8(bytes).expect("utf-8");
    assert!(!aborted && end.outcome == GatewayCallOutcome::Failed);
    assert!(text.starts_with(CHAT_FRAME), "{text}");
    let terminal = text
        .strip_suffix("data: [DONE]\n\n")
        .and_then(|text| text.rsplit_once("data: "))
        .map(|(_, data)| data.trim_end())
        .expect("terminal error before DONE");
    assert_eq!(
        serde_json::from_str::<Value>(terminal).expect("terminal error JSON"),
        json!({"error": {"code": "WYRD_GATEWAY_502_UPSTREAM_UNAVAILABLE", "message": "the provider stream ended before completing", "param": null, "type": "api_error"}})
    );
    let (bytes, aborted, end) = open_stream(
        &dispatch,
        GatewayOperation::Responses,
        IngressDialect::OpenAi,
        openai(),
        json!({"model": "m", "input": "hi", "stream": true}),
        later(),
        CancellationToken::new(),
    )
    .await;
    let text = String::from_utf8(bytes).expect("utf-8");
    assert!(!aborted && end.outcome == GatewayCallOutcome::Failed);
    let terminal = text
        .strip_suffix("\n\n")
        .and_then(|text| text.rsplit_once("event: error\ndata: "))
        .map(|(_, data)| data)
        .expect("terminal error event");
    assert_eq!(
        serde_json::from_str::<Value>(terminal).expect("terminal error JSON"),
        json!({"code": "WYRD_GATEWAY_502_UPSTREAM_UNAVAILABLE", "message": "the provider stream ended before completing", "param": null, "type": "error"})
    );

    let (bytes, aborted, _) = open_stream(
        &dispatch,
        GatewayOperation::ChatCompletions,
        IngressDialect::AnthropicMessages,
        deployment(
            "anthropic/claude-sonnet-5",
            json!("anthropic"),
            api_key("x-api-key"),
        ),
        json!({"model": "c", "max_tokens": 5, "messages": [], "stream": true}),
        later(),
        CancellationToken::new(),
    )
    .await;
    let text = String::from_utf8(bytes).expect("utf-8");
    assert!(!aborted);
    let terminal = text
        .strip_suffix("\n\n")
        .and_then(|text| text.rsplit_once("event: error\ndata: "))
        .map(|(_, data)| data)
        .expect("terminal error event");
    assert_eq!(
        serde_json::from_str::<Value>(terminal).expect("terminal error JSON"),
        json!({"error": {"message": "the provider stream ended before completing", "type": "api_error"}, "type": "error"})
    );
}

/// Truncation inside an event, or on Google whose protocol has no in-band
/// error, leaves the stream unterminated so the caller aborts after the bytes
/// already relayed.
///
/// # Panics
///
/// Panics when a truncated stream ends cleanly, reports usage, or loses the
/// relayed bytes.
#[tokio::test]
async fn truncation_without_an_in_band_error_aborts() {
    let server = MockServer::start().await;
    events(
        &server,
        "/v1beta/models/gemini-2.5-flash:streamGenerateContent",
        "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"hi\"}]}}]}\n\n",
    )
    .await;
    events(&server, "/v1/chat/completions", "data: {\"cho").await;
    let dispatch = dispatch(&server);
    let (_, aborted, end) = open_stream(
        &dispatch,
        GatewayOperation::ChatCompletions,
        IngressDialect::GeminiGenerateContent,
        deployment(
            "gemini/gemini-2.5-flash",
            json!("gemini"),
            api_key("x-goog-api-key"),
        ),
        json!({"contents": []}),
        later(),
        CancellationToken::new(),
    )
    .await;
    assert!(
        aborted && normalized(&end).is_none() && end.outcome == GatewayCallOutcome::Failed,
        "Google truncation aborts"
    );

    let (bytes, aborted, _) = open_stream(
        &dispatch,
        GatewayOperation::ChatCompletions,
        IngressDialect::OpenAi,
        deployment(
            "acme/m",
            json!({"openai_compatible": {"base_url": format!("{}/v1", server.uri())}}),
            bearer(),
        ),
        chat_stream(),
        later(),
        CancellationToken::new(),
    )
    .await;
    assert!(aborted, "truncation inside an event aborts");
    assert_eq!(bytes, b"data: {\"cho");
}

/// The deadline and cancellation append their own terminal errors after the
/// relayed frame, end in `[DONE]`, and close the upstream connection.
///
/// # Panics
///
/// Panics when a stopped stream aborts, reports usage, carries the wrong code,
/// or keeps its upstream open.
#[tokio::test]
async fn deadline_and_cancellation_end_in_terminal_errors() {
    let dispatch = dispatch(&MockServer::start().await);
    for (deadline, cancelled, code, outcome) in [
        (
            Instant::now() + Duration::from_millis(300),
            false,
            "WYRD_GATEWAY_504_DEADLINE_EXCEEDED",
            GatewayCallOutcome::TimedOut,
        ),
        (
            later(),
            true,
            "WYRD_SERVER_503_SERVICE_UNAVAILABLE",
            GatewayCallOutcome::Cancelled,
        ),
    ] {
        let (port, upstream) = held_stream(CHAT_FRAME).await;
        let cancel = CancellationToken::new();
        if cancelled {
            let cancel = cancel.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(200)).await;
                cancel.cancel();
            });
        }
        let (bytes, aborted, end) = open_stream(
            &dispatch,
            GatewayOperation::ChatCompletions,
            IngressDialect::OpenAi,
            deployment(
                "acme/m",
                json!({"openai_compatible": {"base_url": format!("http://127.0.0.1:{port}/v1")}}),
                bearer(),
            ),
            chat_stream(),
            deadline,
            cancel,
        )
        .await;
        let text = String::from_utf8(bytes).expect("utf-8");
        assert!(!aborted && normalized(&end).is_none());
        assert_eq!(end.outcome, outcome, "{text}");
        assert!(text.starts_with(CHAT_FRAME), "{text}");
        assert!(text.contains(code), "{text}");
        assert!(text.ends_with("data: [DONE]\n\n"), "{text}");
        assert!(upstream.await.expect("upstream task"), "upstream closed");
    }
}

/// Dropping the caller's stream aborts the upstream request: the provider
/// sees its connection close and the relay reports a cancelled end with
/// unknown usage. The frame channel is bounded.
///
/// # Panics
///
/// Panics when the stream does not open, relay its first frame, close the
/// upstream, or report anything but a cancelled end with unknown usage.
#[tokio::test]
async fn dropping_a_stream_aborts_the_upstream_request() {
    let (port, upstream) = held_stream("data: {\"choices\":[]}\n\n").await;
    let compatible = deployment(
        "acme/m",
        json!({"openai_compatible": {"base_url": format!("http://127.0.0.1:{port}/v1")}}),
        bearer(),
    );
    let dispatch =
        HttpProviderDispatch::new(EndpointPolicy::new(false), BuiltinEndpoints::default())
            .expect("dispatch builds");
    let result = attempt_mode(
        &dispatch,
        IngressDialect::OpenAi,
        &compatible,
        Some("sk"),
        &json!({"model": "m", "messages": [], "stream": true}),
        true,
    )
    .await;
    let AttemptResult::Completed {
        body: ResponseBody::Events(mut events),
        ..
    } = result
    else {
        panic!("event stream expected, got {result:?}");
    };
    assert_eq!(events.frames.max_capacity(), super::stream::FRAME_BUFFER);
    assert_eq!(
        events.frames.recv().await.as_deref(),
        Some(&b"data: {\"choices\":[]}\n\n"[..])
    );
    let end = events.end.take().expect("end receiver");
    drop(events);
    assert!(upstream.await.expect("upstream task"), "upstream closed");
    let end = end.await.expect("a detached relay reports its end");
    assert_eq!(
        (end.outcome, end.usage),
        (GatewayCallOutcome::Cancelled, AttemptUsage::default())
    );
}

/// Streaming preparation rejects operations that do not stream, a body
/// `stream` member that disagrees with the call, and translated features a
/// chat completion chunk stream cannot carry.
#[test]
fn unrepresentable_streams_are_rejected_with_the_offending_member() {
    let [compatible, anthropic, gemini] = targets();
    let chat = json!({"model": "m", "max_tokens": 5, "stream": true, "messages": [{"role": "user", "content": "hi"}]});
    let with = |extra: Value| {
        let mut body = chat.clone();
        body.as_object_mut()
            .expect("object")
            .extend(extra.as_object().expect("object").clone());
        body
    };
    let tools = with(
        json!({"tools": [{"type": "function", "function": {"name": "w", "parameters": {"type": "object"}}}]}),
    );
    let obfuscated = with(json!({"stream_options": {"include_obfuscation": true}}));
    let cases = [
        (
            GatewayOperation::Embeddings,
            &compatible,
            chat.clone(),
            "stream",
        ),
        (
            GatewayOperation::ChatCompletions,
            &compatible,
            with(json!({"stream": false})),
            "stream",
        ),
        (
            GatewayOperation::ChatCompletions,
            &anthropic,
            tools.clone(),
            "tools",
        ),
        (GatewayOperation::ChatCompletions, &gemini, tools, "tools"),
        (
            GatewayOperation::ChatCompletions,
            &anthropic,
            obfuscated,
            "stream_options.include_obfuscation",
        ),
    ];
    for (operation, target, body, field) in cases {
        let error =
            prepare(IngressDialect::OpenAi, operation, true, &body, target, None).expect_err(field);
        assert_eq!(error.field, field, "{error}");
    }
    for target in [&compatible, &anthropic, &gemini] {
        prepare(
            IngressDialect::OpenAi,
            GatewayOperation::ChatCompletions,
            true,
            &chat,
            target,
            None,
        )
        .expect("plain chat streams");
    }
}

/// `OpenAI` Chat Completions ingress translates to Vertex `GenerateContent`
/// on the Vertex route with bearer authentication, and the answer translates
/// back.
#[tokio::test]
async fn openai_chat_translates_to_vertex_and_back() {
    let server = MockServer::start().await;
    answer(
        &server,
        "/v1/projects/acme/locations/us-central1/publishers/google/models/gemini-2.5-pro:generateContent",
        "authorization",
        "Bearer ya29",
        json!({
            "candidates": [{"content": {"role": "model", "parts": [{"text": "Sunny"}]}, "finishReason": "MAX_TOKENS", "index": 0}],
            "usageMetadata": {"promptTokenCount": 5, "candidatesTokenCount": 3}
        }),
    )
    .await;
    let target = deployment(
        "vertex/gemini-2.5-pro",
        json!({"vertex": {"project": "acme", "location": "us-central1"}}),
        bearer(),
    );
    let body = json!({"model": "vertex/gemini-2.5-pro", "max_tokens": 3, "messages": [{"role": "system", "content": "Be brief."}, {"role": "user", "content": "Weather?"}]});
    let result = attempt(
        &dispatch(&server),
        IngressDialect::OpenAi,
        &target,
        Some("ya29"),
        &body,
    )
    .await;
    assert_eq!(
        received(&server).await,
        json!({
            "contents": [{"role": "user", "parts": [{"text": "Weather?"}]}],
            "system_instruction": {"role": "user", "parts": [{"text": "Be brief."}]},
            "generation_config": {"max_output_tokens": 3}
        })
    );
    let AttemptResult::Completed { body, usage, .. } = result else {
        panic!("translated completion expected, got {result:?}");
    };
    assert_eq!(
        parsed(&body)["choices"],
        json!([{"index": 0, "finish_reason": "length", "logprobs": null, "message": {"role": "assistant", "content": "Sunny"}}])
    );
    assert_eq!(usage.normalized, Some(tokens(5, 3)));
}

/// Provider usage decodes through the typed Skald usage of its wire: only
/// that projection is kept, and usage that does not decode or overflows is
/// unknown.
#[test]
fn provider_usage_is_typed_and_bounded() {
    let padding = "x".repeat(1 << 20);
    let chat = super::usage(
        super::Wire::OpenAi,
        &json!({"usage": {"prompt_tokens": 3, "completion_tokens": 5, "total_tokens": 8, "padding": padding}}),
    );
    assert_eq!(chat.normalized, Some(tokens(3, 5)));
    let kept = chat.provider_usage_json.expect("typed usage kept");
    assert!(!kept.contains("padding") && kept.len() < 256, "{kept}");

    let responses = super::usage(
        super::Wire::OpenAi,
        &json!({"usage": {"input_tokens": 2, "output_tokens": 4, "total_tokens": 6}}),
    );
    assert_eq!(responses.normalized, Some(tokens(2, 4)));

    let anthropic = super::usage(
        super::Wire::Anthropic,
        &json!({"usage": {"input_tokens": 1, "output_tokens": 2, "cache_read_input_tokens": 3, "server_tool_use": {"web_search_requests": 9}}}),
    );
    assert_eq!(anthropic.normalized, Some(tokens(4, 2)));
    assert!(
        !anthropic
            .provider_usage_json
            .expect("typed usage kept")
            .contains("server_tool_use")
    );

    for (wire, body) in [
        (
            super::Wire::OpenAi,
            json!({"usage": {"prompt_tokens": "3", "completion_tokens": 5, "total_tokens": 8}}),
        ),
        (
            super::Wire::Anthropic,
            json!({"usage": {"input_tokens": u64::MAX, "output_tokens": 1, "cache_read_input_tokens": 1}}),
        ),
        (
            super::Wire::Google,
            json!({"usageMetadata": {"promptTokenCount": -1}}),
        ),
    ] {
        assert_eq!(super::usage(wire, &body), AttemptUsage::default(), "{body}");
    }
}

/// A provider refusal echoing the resolved credential, plainly or behind a
/// JSON escape that decoding would restore, reaches the caller without it,
/// native or translated, keeping the status and the rest of the message.
///
/// # Panics
///
/// Panics when an attempt is not a 401 refusal, when the rendered body contains
/// the credential, or when it loses the provider message.
#[tokio::test]
async fn refusals_never_echo_the_resolved_credential() {
    let server = MockServer::start().await;
    let echo = r#"{"type": "error", "error": {"type": "authentication_error", "message": "Incorrect API key provided: sk-canary-\u0037f3 or sk-canary-7f3."}}"#;
    for route in ["/v1/chat/completions", "/v1/messages"] {
        Mock::given(method("POST"))
            .and(path(route))
            .respond_with(ResponseTemplate::new(401).set_body_raw(echo, "application/json"))
            .mount(&server)
            .await;
    }
    let dispatch = dispatch(&server);
    let chat =
        json!({"model": "m", "max_tokens": 5, "messages": [{"role": "user", "content": "hi"}]});
    for target in [
        deployment("openai/gpt-5", json!("openai"), bearer()),
        deployment("anthropic/claude", json!("anthropic"), api_key("x-api-key")),
    ] {
        let result = attempt(
            &dispatch,
            IngressDialect::OpenAi,
            &target,
            Some("sk-canary-7f3"),
            &chat,
        )
        .await;
        let AttemptResult::Refused {
            status: 401, body, ..
        } = &result
        else {
            panic!("refusal expected, got {result:?}");
        };
        let rendered = body.to_string();
        assert!(!rendered.contains("sk-canary-7f3"), "{rendered}");
        assert!(
            rendered.contains("Incorrect API key provided"),
            "{rendered}"
        );
    }
}

/// Uploaded file of `field` carrying a few binary bytes.
fn upload(field: &str) -> UploadFile {
    UploadFile {
        field: field.to_owned(),
        filename: format!("{field}.bin"),
        content_type: "application/octet-stream".to_owned(),
        content: UploadContent::Bytes(vec![0, 255, 7]),
    }
}

/// Runs one buffered media attempt of `body` on `media` against `deployment`.
async fn media_attempt(
    dispatch: &HttpProviderDispatch,
    deployment: &ProviderDeployment,
    operation: GatewayOperation,
    body: &Value,
    media: &MediaRequest,
) -> AttemptResult {
    let secret = ProviderSecret::new("key".to_owned().into());
    let cancel = CancellationToken::new();
    dispatch
        .dispatch(ProviderAttempt {
            call_id: GatewayCallId::from_uuid(uuid::Uuid::from_u128(0xabc)),
            ordinal: NonZeroU32::MIN,
            operation,
            ingress: IngressDialect::OpenAi,
            deployment,
            credential: Some(&secret),
            body,
            media: Some(media),
            batch: None,
            stream: false,
            capture: false,
            deadline: later(),
            cancel: &cancel,
        })
        .await
}

/// Images and Audio reach the `OpenAI` media routes with the native model: a
/// JSON answer returns unchanged with typed usage, and binary or text answers
/// return unchanged with their content type and no invented usage; multipart
/// routes carry text fields and files.
///
/// # Panics
///
/// Panics when a request, answer, content type, or usage differs.
#[tokio::test]
async fn media_routes_relay_json_binary_and_text_answers() {
    let server = MockServer::start().await;
    let image = r#"{"created":1,"data":[{"b64_json":"aGk="}],"usage":{"input_tokens":3,"output_tokens":7,"total_tokens":10}}"#;
    Mock::given(method("POST"))
        .and(path("/v1/images/generations"))
        .and(header("authorization", "Bearer key"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(image, "application/json"))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/audio/speech"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(vec![255_u8, 0, 1], "audio/mpeg"))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/audio/transcriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_raw("hello", "text/plain"))
        .mount(&server)
        .await;
    let dispatch = dispatch(&server);
    let target = deployment("openai/gpt-4o", json!("openai"), bearer());
    let media = |route| MediaRequest {
        route,
        files: Vec::new(),
    };

    let generated = media_attempt(
        &dispatch,
        &target,
        GatewayOperation::Images,
        &json!({"model": "openai/gpt-4o", "prompt": "a cat"}),
        &media(OpenAiMediaRoute::ImageGenerations),
    )
    .await;
    let AttemptResult::Completed {
        body: ResponseBody::Json(raw),
        usage,
        ..
    } = generated
    else {
        panic!("image JSON expected: {generated:?}");
    };
    assert_eq!(raw.get(), image);
    assert_eq!(usage.normalized, Some(tokens(3, 7)));
    assert_eq!(
        received(&server).await,
        json!({"model": "gpt-4o", "prompt": "a cat"})
    );

    let spoken = media_attempt(
        &dispatch,
        &target,
        GatewayOperation::Audio,
        &json!({"model": "openai/gpt-4o", "input": "hi", "voice": "alloy"}),
        &media(OpenAiMediaRoute::AudioSpeech),
    )
    .await;
    assert!(
        matches!(&spoken, AttemptResult::Completed { body: ResponseBody::MediaStream { content_type, .. }, usage, .. } if content_type == "audio/mpeg" && *usage == AttemptUsage::default()),
        "{spoken:?}"
    );
    let (audio, aborted, end) = collect(spoken).await;
    assert_eq!((audio.as_slice(), aborted), (&[255_u8, 0, 1][..], false));
    assert_eq!(
        (end.outcome, end.usage),
        (GatewayCallOutcome::Succeeded, AttemptUsage::default())
    );

    let transcribed = media_attempt(
        &dispatch,
        &target,
        GatewayOperation::Audio,
        &json!({"model": "openai/gpt-4o", "response_format": "text"}),
        &MediaRequest {
            route: OpenAiMediaRoute::AudioTranscriptions,
            files: vec![upload("file")],
        },
    )
    .await;
    assert!(
        matches!(&transcribed, AttemptResult::Completed { body: ResponseBody::Media(answer), .. } if answer.bytes == b"hello"),
        "{transcribed:?}"
    );
    let requests = server.received_requests().await.expect("recording");
    let sent = requests.last().expect("transcription request");
    assert!(
        sent.headers["content-type"]
            .to_str()
            .expect("header")
            .starts_with("multipart/form-data; boundary="),
    );
    let body = String::from_utf8_lossy(&sent.body);
    assert!(
        body.contains("name=\"model\"\r\n\r\ngpt-4o\r\n")
            && body.contains("name=\"file\"; filename=\"file.bin\""),
        "{body}"
    );
}

/// Runs one media attempt of `body` on `route` with credential `secret`,
/// selecting response capture when `capture` is set.
async fn media_capturing(
    dispatch: &HttpProviderDispatch,
    deployment: &ProviderDeployment,
    body: &Value,
    route: OpenAiMediaRoute,
    secret: &str,
    capture: bool,
) -> AttemptResult {
    let secret = ProviderSecret::new(secret.to_owned().into());
    let cancel = CancellationToken::new();
    let media = MediaRequest {
        route,
        files: if route == OpenAiMediaRoute::AudioSpeech {
            Vec::new()
        } else {
            vec![upload("file")]
        },
    };
    dispatch
        .dispatch(ProviderAttempt {
            call_id: GatewayCallId::from_uuid(uuid::Uuid::from_u128(0xabc)),
            ordinal: NonZeroU32::MIN,
            operation: GatewayOperation::Audio,
            ingress: IngressDialect::OpenAi,
            deployment,
            credential: Some(&secret),
            body,
            media: Some(&media),
            batch: None,
            stream: false,
            capture,
            deadline: later(),
            cancel: &cancel,
        })
        .await
}

/// Selected media capture copies buffered and relayed speech bytes exactly,
/// keeps nothing when capture is unselected, and keeps nothing when the bytes
/// hold the attempt's credential, while the caller's bytes stay unchanged in
/// every case.
///
/// # Panics
///
/// Panics when the caller's bytes change, benign bytes are not captured
/// exactly, unselected bytes are kept, or credential-bearing bytes are kept.
#[tokio::test]
async fn selected_media_capture_copies_benign_bytes_and_drops_credentials() {
    const CANARY: &str = "sk-media-canary-42";
    let benign = vec![255_u8, 0, 1];
    let leaked = format!("RIFF{CANARY}RIFF").into_bytes();
    let server = MockServer::start().await;
    for (voice, bytes) in [("benign", &benign), ("leaked", &leaked)] {
        Mock::given(method("POST"))
            .and(path("/v1/audio/speech"))
            .and(wiremock::matchers::body_partial_json(
                json!({"voice": voice}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_raw(bytes.clone(), "audio/mpeg"))
            .mount(&server)
            .await;
    }
    Mock::given(method("POST"))
        .and(path("/v1/audio/transcriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(leaked.clone(), "text/plain"))
        .mount(&server)
        .await;
    let dispatch = dispatch(&server);
    let target = deployment("openai/gpt-4o", json!("openai"), bearer());
    let speech = |voice: &str| json!({"model": "openai/gpt-4o", "input": "hi", "voice": voice});

    for (voice, capture, bytes, expected) in [
        (
            "benign",
            true,
            &benign,
            Some(ResponseCapture::Media(MediaAnswer {
                content_type: "audio/mpeg".to_owned(),
                bytes: benign.clone(),
            })),
        ),
        ("benign", false, &benign, None),
        ("leaked", true, &leaked, None),
    ] {
        let spoken = media_capturing(
            &dispatch,
            &target,
            &speech(voice),
            OpenAiMediaRoute::AudioSpeech,
            CANARY,
            capture,
        )
        .await;
        let (audio, aborted, end) = collect(spoken).await;
        assert_eq!(
            (&audio, aborted),
            (bytes, false),
            "{voice} bytes relay unchanged"
        );
        assert_eq!(end.outcome, GatewayCallOutcome::Succeeded);
        assert_eq!(end.capture, expected, "{voice} capture={capture}");
    }

    let transcribed = media_capturing(
        &dispatch,
        &target,
        &json!({"model": "openai/gpt-4o", "response_format": "text"}),
        OpenAiMediaRoute::AudioTranscriptions,
        CANARY,
        true,
    )
    .await;
    let AttemptResult::Completed {
        body: ResponseBody::Media(answer),
        capture,
        ..
    } = transcribed
    else {
        panic!("buffered media expected: {transcribed:?}");
    };
    assert_eq!(answer.bytes, leaked, "the caller's answer is unchanged");
    assert_eq!(capture, None, "credential-bearing media is not kept");
}

/// Dropping an in-flight media exchange, as caller cancellation, drain, or the
/// deadline does in the engine, closes the upstream connection instead of
/// reading on.
///
/// # Panics
///
/// Panics when the listener fails, the exchange ends first, or the upstream
/// connection stays open.
#[tokio::test]
async fn dropping_a_media_exchange_closes_the_upstream_connection() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let (answering, answered) = tokio::sync::oneshot::channel();
    let upstream = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let mut buffer = [0_u8; 4096];
        let read = socket.read(&mut buffer).await.expect("read");
        assert!(read > 0, "request arrives");
        socket
            .write_all(b"HTTP/1.1 200 OK\r\ncontent-type: image/png\r\ntransfer-encoding: chunked\r\n\r\n3\r\nabc\r\n")
            .await
            .expect("write");
        answering.send(()).expect("test waits");
        // Drain until the gateway closes the connection.
        loop {
            match socket.read(&mut buffer).await {
                Ok(0) | Err(_) => break true,
                Ok(_) => {}
            }
        }
    });
    let target = deployment(
        "acme/m",
        json!({"openai_compatible": {"base_url": format!("http://127.0.0.1:{port}/v1")}}),
        bearer(),
    );
    let dispatch =
        HttpProviderDispatch::new(EndpointPolicy::new(false), BuiltinEndpoints::default())
            .expect("dispatch builds");
    let request = MediaRequest {
        route: OpenAiMediaRoute::ImageGenerations,
        files: Vec::new(),
    };
    let body = json!({"prompt": "x"});
    tokio::select! {
        result = media_attempt(&dispatch, &target, GatewayOperation::Images, &body, &request) => {
            panic!("an unfinished answer cannot complete: {result:?}");
        }
        answered = answered => answered.expect("upstream answered"),
    }
    assert!(upstream.await.expect("upstream task"), "upstream closed");
}

/// One media preparation case: operation, media, target, body, streaming
/// mode, and the member its rejection must name.
type MediaCase<'a> = (
    GatewayOperation,
    Option<MediaRequest>,
    &'a ProviderDeployment,
    Value,
    bool,
    &'static str,
);

/// Media request of `route` carrying one upload per listed field.
fn media_of(route: OpenAiMediaRoute, files: &[&str]) -> MediaRequest {
    MediaRequest {
        route,
        files: files.iter().map(|field| upload(field)).collect(),
    }
}

/// Asserts that preparation rejects every case under its expected member.
///
/// # Panics
///
/// Panics when a case is accepted or rejected under another member.
fn assert_media_rejections(cases: Vec<MediaCase<'_>>) {
    for (operation, media, target, body, stream, field) in cases {
        let error = prepare(
            IngressDialect::OpenAi,
            operation,
            stream,
            &body,
            target,
            media.as_ref(),
        )
        .expect_err(field);
        assert_eq!(error.field, field, "{error}");
    }
}

/// Media preparation requires a route of the requested family, never
/// streams, and passes Images and Audio only to `OpenAI`-protocol adapters.
///
/// # Panics
///
/// Panics when a case is accepted or rejected under the wrong member.
#[test]
fn media_requests_are_matched_to_their_family_and_adapter() {
    let [compatible, anthropic, _] = targets();
    let body = || json!({"model": "acme/m"});
    let generation = || Some(media_of(OpenAiMediaRoute::ImageGenerations, &[]));
    assert_media_rejections(vec![
        (
            GatewayOperation::Images,
            None,
            &compatible,
            body(),
            false,
            "operation",
        ),
        (
            GatewayOperation::Images,
            Some(media_of(OpenAiMediaRoute::AudioSpeech, &[])),
            &compatible,
            body(),
            false,
            "operation",
        ),
        (
            GatewayOperation::ChatCompletions,
            generation(),
            &compatible,
            body(),
            false,
            "operation",
        ),
        (
            GatewayOperation::Images,
            generation(),
            &anthropic,
            body(),
            false,
            "operation",
        ),
        (
            GatewayOperation::Images,
            generation(),
            &compatible,
            json!({"model": "acme/m", "stream": false}),
            false,
            "stream",
        ),
        (
            GatewayOperation::Audio,
            Some(media_of(OpenAiMediaRoute::AudioSpeech, &[])),
            &compatible,
            body(),
            true,
            "stream",
        ),
    ]);
}

/// Media preparation accepts only the route's file fields in their allowed
/// counts and passes an accepted edit through with the native model.
///
/// # Panics
///
/// Panics when a file set is accepted or rejected unexpectedly.
#[test]
fn media_files_are_matched_to_the_route() {
    let [compatible, ..] = targets();
    let body = || json!({"model": "acme/m"});
    assert_media_rejections(vec![
        (
            GatewayOperation::Images,
            Some(media_of(OpenAiMediaRoute::ImageEdits, &["mask"])),
            &compatible,
            body(),
            false,
            "image",
        ),
        (
            GatewayOperation::Images,
            Some(media_of(OpenAiMediaRoute::ImageEdits, &["image", "other"])),
            &compatible,
            body(),
            false,
            "other",
        ),
        (
            GatewayOperation::Images,
            Some(media_of(
                OpenAiMediaRoute::ImageVariations,
                &["image", "image"],
            )),
            &compatible,
            body(),
            false,
            "image",
        ),
        (
            GatewayOperation::Audio,
            Some(media_of(OpenAiMediaRoute::AudioTranslations, &[])),
            &compatible,
            body(),
            false,
            "file",
        ),
        (
            GatewayOperation::Audio,
            Some(media_of(OpenAiMediaRoute::AudioSpeech, &["file"])),
            &compatible,
            body(),
            false,
            "file",
        ),
    ]);
    let edit = prepare(
        IngressDialect::OpenAi,
        GatewayOperation::Images,
        false,
        &json!({"model": "acme/m", "prompt": "hat"}),
        &compatible,
        Some(&media_of(
            OpenAiMediaRoute::ImageEdits,
            &["image[]", "image[]", "mask"],
        )),
    )
    .expect("an edit with images and a mask passes through");
    assert_eq!(
        edit,
        Prepared::Native(json!({"model": "m", "prompt": "hat"}))
    );
}

/// Batches lifecycle actions reach the `OpenAI` Files and Batches routes: an
/// upload carries the input file with every line's native model, creation
/// sends its body without a model, JSON answers return unchanged, file content
/// keeps its content type.
///
/// # Panics
///
/// Panics when a request or answer differs.
#[tokio::test]
async fn batch_actions_reach_their_routes_with_native_files() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/files"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(r#"{"id":"file-up"}"#, "application/json"),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/batches"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(r#"{"id":"batch_1"}"#, "application/json"),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/files/file-out/content"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw("{\"custom_id\":\"a\"}\n", "application/octet-stream"),
        )
        .mount(&server)
        .await;
    let dispatch = dispatch(&server);
    let target = deployment("openai/gpt-4o", json!("openai"), bearer());
    let line = json!({"custom_id": "a", "method": "POST", "url": "/v1/chat/completions", "body": {"model": "openai/gpt-4o", "messages": []}});
    let input = super::BatchInput::parse(format!("{line}\n").as_bytes()).expect("input parses");
    let secret = ProviderSecret::new("key".to_owned().into());
    let cancel = CancellationToken::new();
    let run = |body: Value, action: super::BatchAction| {
        let (dispatch, target, secret, cancel) = (&dispatch, &target, &secret, &cancel);
        async move {
            dispatch
                .dispatch(ProviderAttempt {
                    call_id: GatewayCallId::from_uuid(uuid::Uuid::from_u128(0xabc)),
                    ordinal: NonZeroU32::MIN,
                    operation: GatewayOperation::Batches,
                    ingress: IngressDialect::OpenAi,
                    deployment: target,
                    credential: Some(secret),
                    body: &body,
                    media: None,
                    batch: Some(&action),
                    stream: false,
                    capture: false,
                    deadline: later(),
                    cancel,
                })
                .await
        }
    };

    let uploaded = run(
        json!({"purpose": "batch"}),
        super::BatchAction::UploadFile {
            filename: "in.jsonl".to_owned(),
            input,
        },
    )
    .await;
    assert!(
        matches!(&uploaded, AttemptResult::Completed { body, .. } if parsed(body) == json!({"id": "file-up"})),
        "{uploaded:?}"
    );
    let requests = server.received_requests().await.expect("recording");
    let sent = String::from_utf8_lossy(&requests[0].body);
    assert!(
        sent.contains("name=\"purpose\"\r\n\r\nbatch\r\n")
            && sent.contains(r#""model":"gpt-4o""#)
            && !sent.contains("openai/gpt-4o"),
        "the input file carries native models: {sent}"
    );

    let create = json!({"input_file_id": "file-up", "endpoint": "/v1/chat/completions", "completion_window": "24h"});
    let created = run(create.clone(), super::BatchAction::CreateBatch).await;
    assert!(
        matches!(&created, AttemptResult::Completed { body, .. } if parsed(body) == json!({"id": "batch_1"})),
        "{created:?}"
    );
    let requests = server.received_requests().await.expect("recording");
    assert_eq!(
        serde_json::from_slice::<Value>(&requests[1].body).expect("json body"),
        create,
        "creation sends its body without a model"
    );

    let content = run(
        json!({}),
        super::BatchAction::FileContent("file-out".to_owned()),
    )
    .await;
    let AttemptResult::Completed {
        body: ResponseBody::Media(answer),
        ..
    } = content
    else {
        panic!("file content expected: {content:?}");
    };
    assert_eq!(answer.bytes, b"{\"custom_id\":\"a\"}\n");
}

/// A non-`OpenAI` adapter rejects Batches before dispatch.
///
/// # Panics
///
/// Panics when preparation accepts Batches or names another field.
#[test]
fn batches_require_an_openai_protocol_adapter() {
    let [_, anthropic, _] = targets();
    let rejected = prepare(
        IngressDialect::OpenAi,
        GatewayOperation::Batches,
        false,
        &json!({"input_file_id": "file-up"}),
        &anthropic,
        None,
    )
    .expect_err("Batches need an OpenAI-protocol adapter");
    assert_eq!(rejected.field, "operation");
}

/// Operations in published column order with their column headings.
const OPERATIONS: [(GatewayOperation, &str); 6] = [
    (GatewayOperation::ChatCompletions, "Chat Completions"),
    (GatewayOperation::Responses, "Responses"),
    (GatewayOperation::Embeddings, "Embeddings"),
    (GatewayOperation::Images, "Images"),
    (GatewayOperation::Audio, "Audio"),
    (GatewayOperation::Batches, "Batches"),
];

/// One deployment per adapter with its published name.
fn adapters() -> [(&'static str, ProviderDeployment); 5] {
    [
        ("openai", deployment("openai/m", json!("openai"), bearer())),
        (
            "openai_compatible",
            deployment(
                "acme/m",
                json!({"openai_compatible": {"base_url": "https://acme.example/v1"}}),
                bearer(),
            ),
        ),
        (
            "anthropic",
            deployment("anthropic/m", json!("anthropic"), bearer()),
        ),
        ("gemini", deployment("gemini/m", json!("gemini"), bearer())),
        (
            "vertex",
            deployment(
                "vertex/m",
                json!({"vertex": {"project": "p", "location": "us-central1"}}),
                bearer(),
            ),
        ),
    ]
}

/// A representative `OpenAI` request of `operation`, with its media route.
fn representative(operation: GatewayOperation, stream: bool) -> (Value, Option<MediaRequest>) {
    let media = |route| {
        Some(MediaRequest {
            route,
            files: Vec::new(),
        })
    };
    let (mut body, media) = match operation {
        GatewayOperation::ChatCompletions => (
            json!({"model": "m", "max_completion_tokens": 16, "messages": [{"role": "user", "content": "hi"}]}),
            None,
        ),
        GatewayOperation::Responses | GatewayOperation::Embeddings => {
            (json!({"model": "m", "input": "hi"}), None)
        }
        GatewayOperation::Images => (
            json!({"model": "m", "prompt": "a cat"}),
            media(OpenAiMediaRoute::ImageGenerations),
        ),
        GatewayOperation::Audio => (
            json!({"model": "m", "input": "hi", "voice": "alloy"}),
            media(OpenAiMediaRoute::AudioSpeech),
        ),
        GatewayOperation::Batches => (json!({"input_file_id": "file-1"}), None),
    };
    if stream {
        body["stream"] = Value::Bool(true);
    }
    (body, media)
}

/// Request preparation agrees with the typed capability source for every
/// adapter and operation: a served operation prepares buffered, and streamed
/// exactly when it streams; an unserved one is rejected before dispatch.
///
/// # Panics
///
/// Panics when preparation and the capability source disagree.
#[test]
fn preparation_matches_the_typed_capability_source() {
    for (name, target) in adapters() {
        for (operation, _) in OPERATIONS {
            let served = target.adapter.serves(operation);
            for stream in [false, true] {
                let (body, media) = representative(operation, stream);
                let prepared = prepare(
                    IngressDialect::OpenAi,
                    operation,
                    stream,
                    &body,
                    &target,
                    media.as_ref(),
                );
                assert_eq!(
                    prepared.is_ok(),
                    served && (!stream || operation.streams()),
                    "{name} {operation:?} stream={stream}: {prepared:?}"
                );
            }
        }
    }
}

/// The operation-support table in the gateway administration guide is the
/// rendering of the typed capability source, so the published matrix cannot
/// drift from what deployments may declare and preparation carries.
///
/// # Panics
///
/// Panics when the guide cannot be read or lacks the rendered table.
#[test]
fn published_operation_support_matches_the_capability_source() {
    let mut table = String::from("| Adapter |");
    for (_, heading) in OPERATIONS {
        write!(table, " {heading} |").expect("writing to a String is infallible");
    }
    table.push_str("\n| --- |");
    table.push_str(&" --- |".repeat(OPERATIONS.len()));
    for (name, target) in adapters() {
        write!(table, "\n| `{name}` |").expect("writing to a String is infallible");
        for (operation, _) in OPERATIONS {
            let cell = match (target.adapter.serves(operation), operation.streams()) {
                (false, _) => "—",
                (true, true) => "streaming",
                (true, false) => "buffered",
            };
            write!(table, " {cell} |").expect("writing to a String is infallible");
        }
    }
    let guide = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../docs/src/content/docs/how-to/administer-the-gateway.svx"
    ))
    .expect("gateway administration guide reads");
    assert!(
        guide.contains(&table),
        "the guide's operation-support table must be:\n{table}"
    );
}

/// Dispatches one two-input, two-dimension embeddings request in `encoding`
/// to a mock provider answering `answer`, and checks the provider was called
/// exactly once.
///
/// # Panics
///
/// Panics when the mock server records other than one request.
async fn embeddings_attempt(encoding: &str, answer: &Value) -> AttemptResult {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(answer))
        .mount(&server)
        .await;
    let target = deployment("openai/text-embedding-3-small", json!("openai"), bearer());
    let secret = ProviderSecret::new("key".to_owned().into());
    let cancel = CancellationToken::new();
    let body = json!({"model": "openai/text-embedding-3-small", "input": ["a", "b"],
        "dimensions": 2, "encoding_format": encoding});
    let result = dispatch(&server)
        .dispatch(ProviderAttempt {
            call_id: GatewayCallId::from_uuid(uuid::Uuid::from_u128(0xabd)),
            ordinal: NonZeroU32::MIN,
            operation: GatewayOperation::Embeddings,
            ingress: IngressDialect::OpenAi,
            deployment: &target,
            credential: Some(&secret),
            body: &body,
            media: None,
            batch: None,
            stream: false,
            capture: false,
            deadline: later(),
            cancel: &cancel,
        })
        .await;
    received(&server).await;
    result
}

/// An embeddings answer that ignores the request fails after its one provider
/// request instead of succeeding: a vector length other than the requested
/// `dimensions`, fewer items than inputs, a duplicate, missing, or reordered
/// `index`, a vector in the representation the request did not ask for, a
/// base64 vector with an invalid alphabet or padding, and a base64 vector of
/// the wrong decoded length. A faithful float or base64 answer still returns
/// unchanged.
///
/// # Panics
///
/// Panics when a weakened answer succeeds, a faithful one fails, or the
/// provider is not called exactly once.
#[tokio::test]
async fn embeddings_answers_that_ignore_the_request_fail() {
    // Two little-endian f32 values are eight bytes: 12 base64 characters.
    let two = "AAAAAAAAgD8=";
    let three = "AAAAAAAAgD8AAIA/";
    let item = |index: u64, embedding: Value| json!({"object": "embedding", "index": index, "embedding": embedding});
    for (encoding, data, faithful) in [
        (
            "float",
            json!([item(0, json!([0.1, 0.2])), item(1, json!([0.3, 0.4]))]),
            true,
        ),
        (
            "base64",
            json!([item(0, json!(two)), item(1, json!(two))]),
            true,
        ),
        (
            "float",
            json!([item(1, json!([0.3, 0.4])), item(0, json!([0.1, 0.2]))]),
            false,
        ),
        (
            "base64",
            json!([item(1, json!(two)), item(0, json!(two))]),
            false,
        ),
        (
            "float",
            json!([item(0, json!(two)), item(1, json!(two))]),
            false,
        ),
        (
            "base64",
            json!([item(0, json!([0.1, 0.2])), item(1, json!([0.3, 0.4]))]),
            false,
        ),
        (
            "base64",
            json!([item(0, json!("AAAA!AAAgD8=")), item(1, json!(two))]),
            false,
        ),
        (
            "base64",
            json!([item(0, json!("AAAAAAAAgD8")), item(1, json!(two))]),
            false,
        ),
        (
            "float",
            json!([item(0, json!([0.1, 0.2, 0.3])), item(1, json!([0.3, 0.4]))]),
            false,
        ),
        ("float", json!([item(0, json!([0.1, 0.2]))]), false),
        (
            "float",
            json!([item(0, json!([0.1, 0.2])), item(0, json!([0.3, 0.4]))]),
            false,
        ),
        (
            "float",
            json!([item(0, json!([0.1, 0.2])), item(2, json!([0.3, 0.4]))]),
            false,
        ),
        (
            "base64",
            json!([item(0, json!(two)), item(1, json!(three))]),
            false,
        ),
    ] {
        let answer = json!({"object": "list", "data": data, "model": "m",
            "usage": {"prompt_tokens": 5, "total_tokens": 5}});
        let result = embeddings_attempt(encoding, &answer).await;
        match result {
            AttemptResult::Completed {
                body: ResponseBody::Json(raw),
                ..
            } if faithful => {
                assert_eq!(
                    serde_json::from_str::<Value>(raw.get()).expect("json"),
                    answer
                );
            }
            AttemptResult::Failed {
                class: FailureClass::Rejected,
                ..
            } if !faithful => {}
            other => panic!("{answer} (faithful: {faithful}) answered {other:?}"),
        }
    }
}

/// Generated speech past the transport's answer bound ends unterminated, so
/// the caller aborts instead of receiving truncated audio as success, and
/// the relay reports a failed end.
///
/// # Panics
///
/// Panics when the oversized answer ends terminated or does not end failed.
#[tokio::test]
async fn speech_answers_past_their_bound_abort() {
    let server = MockServer::start().await;
    let limit = skald_providers::TransportConfig::default().max_response_bytes;
    Mock::given(method("POST"))
        .and(path("/v1/audio/speech"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(vec![7_u8; limit + 1], "audio/mpeg"))
        .mount(&server)
        .await;
    let spoken = media_attempt(
        &dispatch(&server),
        &deployment("openai/gpt-4o", json!("openai"), bearer()),
        GatewayOperation::Audio,
        &json!({"model": "openai/gpt-4o", "input": "hi", "voice": "alloy"}),
        &MediaRequest {
            route: OpenAiMediaRoute::AudioSpeech,
            files: Vec::new(),
        },
    )
    .await;
    let (audio, aborted, end) = collect(spoken).await;
    assert!(aborted, "an oversized answer aborts");
    assert!(audio.len() <= limit, "{} bytes relayed", audio.len());
    assert_eq!(end.outcome, GatewayCallOutcome::Failed);
}

/// Dropping a relayed speech stream after its first chunk closes the upstream
/// connection instead of reading the rest of the audio.
///
/// # Panics
///
/// Panics when the stream does not open, the first chunk differs, or the
/// upstream connection stays open.
#[tokio::test]
async fn dropping_a_speech_stream_closes_the_upstream_connection() {
    let (port, upstream) = held_stream("abc").await;
    let spoken = media_attempt(
        &HttpProviderDispatch::new(EndpointPolicy::new(false), BuiltinEndpoints::default())
            .expect("dispatch builds"),
        &deployment(
            "acme/m",
            json!({"openai_compatible": {"base_url": format!("http://127.0.0.1:{port}/v1")}}),
            bearer(),
        ),
        GatewayOperation::Audio,
        &json!({"input": "hi", "voice": "alloy"}),
        &MediaRequest {
            route: OpenAiMediaRoute::AudioSpeech,
            files: Vec::new(),
        },
    )
    .await;
    let AttemptResult::Completed {
        body: ResponseBody::MediaStream { mut events, .. },
        ..
    } = spoken
    else {
        panic!("speech stream expected: {spoken:?}");
    };
    let first = events.recv().await.expect("first chunk").expect("chunk");
    assert_eq!(first, b"abc");
    drop(events);
    assert!(upstream.await.expect("upstream task"), "upstream closed");
}

/// Responses streams that end in `response.failed` or `response.incomplete`
/// relay unchanged and terminate cleanly, but end failed rather than
/// successful.
///
/// # Panics
///
/// Panics when either stream aborts, changes bytes, or ends successfully.
#[tokio::test]
async fn failed_and_incomplete_responses_streams_end_failed() {
    let server = MockServer::start().await;
    for kind in ["failed", "incomplete"] {
        let body = format!(
            "event: response.{kind}\ndata: {{\"type\":\"response.{kind}\",\"response\":{{\"id\":\"r1\",\"status\":\"{kind}\"}}}}\n\n"
        );
        Mock::given(method("POST"))
            .and(path("/v1/responses"))
            .and(wiremock::matchers::body_partial_json(
                json!({"input": kind}),
            ))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(body.clone(), "text/event-stream"),
            )
            .mount(&server)
            .await;
        let (bytes, aborted, end) = open_stream(
            &dispatch(&server),
            GatewayOperation::Responses,
            IngressDialect::OpenAi,
            deployment("openai/gpt-5", json!("openai"), bearer()),
            json!({"model": "m", "input": kind, "stream": true}),
            later(),
            CancellationToken::new(),
        )
        .await;
        assert!(!aborted, "response.{kind} terminates the stream");
        assert_eq!(String::from_utf8(bytes).expect("utf-8"), body);
        assert_eq!(end.outcome, GatewayCallOutcome::Failed, "response.{kind}");
    }
}

/// Runs one chat attempt of `body` against `deployment` with response
/// capture selected and credential `secret`.
async fn capturing(
    dispatch: &HttpProviderDispatch,
    deployment: &ProviderDeployment,
    secret: &str,
    body: &Value,
    stream: bool,
) -> AttemptResult {
    let secret = ProviderSecret::new(secret.to_owned().into());
    let cancel = CancellationToken::new();
    dispatch
        .dispatch(ProviderAttempt {
            call_id: GatewayCallId::from_uuid(uuid::Uuid::from_u128(0xabc)),
            ordinal: NonZeroU32::MIN,
            operation: GatewayOperation::ChatCompletions,
            ingress: IngressDialect::OpenAi,
            deployment,
            credential: Some(&secret),
            body,
            media: None,
            batch: None,
            stream,
            capture: true,
            deadline: later(),
            cancel: &cancel,
        })
        .await
}

/// Selected capture removes the attempt's credential from a buffered answer
/// and from streamed events echoing it under innocuous nested keys, while the
/// caller's bytes stay unchanged; unselected attempts keep nothing, and
/// streamed content past the capture ceiling drops only the capture.
///
/// # Panics
///
/// Panics when the caller's bytes change, the capture keeps the credential,
/// unselected content is kept, or oversized content is captured.
#[tokio::test]
async fn selected_capture_is_scrubbed_bounded_and_leaves_answers_unchanged() {
    const CANARY: &str = "sk-capture-canary-77";
    let server = MockServer::start().await;
    let answer = format!(r#"{{"id":"c1","choices":[],"meta":{{"echo":{{"note":"{CANARY}"}}}}}}"#);
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(wiremock::matchers::body_partial_json(
            json!({"user": "buffered"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_raw(answer.clone(), "application/json"))
        .mount(&server)
        .await;
    let events = format!(
        "data: {{\"id\":\"c1\",\"choices\":[],\"meta\":{{\"echo\":\"{CANARY}\"}}}}\n\ndata: [DONE]\n\n"
    );
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(wiremock::matchers::body_partial_json(
            json!({"user": "streamed"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_raw(events.clone(), "text/event-stream"))
        .mount(&server)
        .await;
    let filler = "x".repeat(GATEWAY_JSON_MAX_BYTES / 2 + 1);
    let oversized = format!(
        "data: {{\"filler\":\"{filler}\"}}\n\ndata: {{\"filler\":\"{filler}\"}}\n\ndata: [DONE]\n\n"
    );
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(wiremock::matchers::body_partial_json(
            json!({"user": "oversized"}),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(oversized.clone(), "text/event-stream"),
        )
        .mount(&server)
        .await;
    let dispatch = dispatch(&server);
    let target = deployment("openai/gpt-5", json!("openai"), bearer());
    let chat = |user: &str, stream: bool| json!({"model": "m", "messages": [], "user": user, "stream": stream});

    let buffered = capturing(&dispatch, &target, CANARY, &chat("buffered", false), false).await;
    let AttemptResult::Completed {
        body: ResponseBody::Json(raw),
        capture: Some(ResponseCapture::Json(capture)),
        ..
    } = buffered
    else {
        panic!("captured JSON expected: {buffered:?}");
    };
    assert_eq!(raw.get(), answer, "the caller's answer is unchanged");
    assert!(!capture.to_string().contains(CANARY), "{capture}");
    assert_eq!(capture["meta"]["echo"]["note"], "");
    let unselected = attempt(
        &dispatch,
        IngressDialect::OpenAi,
        &target,
        Some(CANARY),
        &chat("buffered", false),
    )
    .await;
    assert!(
        matches!(unselected, AttemptResult::Completed { capture: None, .. }),
        "{unselected:?}"
    );

    let (bytes, aborted, end) =
        collect(capturing(&dispatch, &target, CANARY, &chat("streamed", true), true).await).await;
    assert!(!aborted);
    assert_eq!(String::from_utf8(bytes).expect("utf-8"), events);
    assert_eq!(end.outcome, GatewayCallOutcome::Succeeded);
    let Some(ResponseCapture::Json(capture)) = end.capture else {
        panic!("selected streamed content is captured as JSON");
    };
    assert!(!capture.to_string().contains(CANARY), "{capture}");
    assert_eq!(
        capture,
        json!([{"id": "c1", "choices": [], "meta": {"echo": ""}}])
    );

    let (bytes, aborted, end) =
        collect(capturing(&dispatch, &target, CANARY, &chat("oversized", true), true).await).await;
    assert!(!aborted);
    assert_eq!(
        bytes.len(),
        oversized.len(),
        "the caller still gets every byte"
    );
    assert_eq!(end.outcome, GatewayCallOutcome::Succeeded);
    assert_eq!(end.capture, None, "content past the ceiling drops capture");
}

/// Runs one image generation attempt of `prompt` against `deployment` with
/// response capture selected and credential `secret`.
async fn image_capturing(
    dispatch: &HttpProviderDispatch,
    deployment: &ProviderDeployment,
    prompt: &str,
    secret: &str,
) -> AttemptResult {
    let secret = ProviderSecret::new(secret.to_owned().into());
    let cancel = CancellationToken::new();
    let media = MediaRequest {
        route: OpenAiMediaRoute::ImageGenerations,
        files: Vec::new(),
    };
    dispatch
        .dispatch(ProviderAttempt {
            call_id: GatewayCallId::from_uuid(uuid::Uuid::from_u128(0xabc)),
            ordinal: NonZeroU32::MIN,
            operation: GatewayOperation::Images,
            ingress: IngressDialect::OpenAi,
            deployment,
            credential: Some(&secret),
            body: &json!({"model": "openai/gpt-4o", "prompt": prompt}),
            media: Some(&media),
            batch: None,
            stream: false,
            capture: true,
            deadline: later(),
            cancel: &cancel,
        })
        .await
}

/// Selected capture drops every projection holding the attempt's credential as
/// valid base64, which literal removal cannot see and capture would persist as
/// decoded object bytes or canonical member names: an image `b64_json`, a
/// media `data` member, a base64 `data:` URL, a member name, and a streamed
/// event. The caller's bytes stay unchanged and benign encoded media and
/// member names are still projected.
///
/// # Panics
///
/// Panics when a caller's bytes change, an encoded credential is projected, or
/// benign encoded media is not.
#[tokio::test]
async fn selected_capture_drops_valid_base64_credentials() {
    const CANARY: &str = "sk-encoded-canary-31";
    let encoded = base64::engine::general_purpose::STANDARD.encode(format!("PNG{CANARY}PNG"));
    let server = MockServer::start().await;
    let answers = [
        (
            "b64",
            json!({"created": 1, "data": [{"b64_json": encoded}]}),
        ),
        (
            "data",
            json!({"created": 1, "data": [{"mime_type": "image/png", "data": encoded}]}),
        ),
        (
            "url",
            json!({"created": 1, "data": [{"url": format!("data:image/png;base64,{encoded}")}]}),
        ),
        (
            "key",
            json!({"created": 1, "data": [{encoded.as_str(): "x"}]}),
        ),
        (
            "benign",
            json!({"created": 1, "data": [{"b64_json": "aGk="}]}),
        ),
        ("benign-key", json!({"created": 1, "data": [{"aGk=": "x"}]})),
    ];
    for (prompt, answer) in &answers {
        Mock::given(method("POST"))
            .and(path("/v1/images/generations"))
            .and(wiremock::matchers::body_partial_json(
                json!({"prompt": prompt}),
            ))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(answer.to_string(), "application/json"),
            )
            .mount(&server)
            .await;
    }
    let events = format!(
        "data: {{\"id\":\"c1\",\"choices\":[],\"b64_json\":\"{encoded}\"}}\n\ndata: [DONE]\n\n"
    );
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(wiremock::matchers::body_partial_json(
            json!({"user": "streamed"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_raw(events.clone(), "text/event-stream"))
        .mount(&server)
        .await;
    let dispatch = dispatch(&server);
    let target = deployment("openai/gpt-4o", json!("openai"), bearer());
    for (prompt, answer) in &answers {
        let generated = image_capturing(&dispatch, &target, prompt, CANARY).await;
        let AttemptResult::Completed {
            body: ResponseBody::Json(raw),
            capture,
            ..
        } = generated
        else {
            panic!("{prompt} image JSON expected: {generated:?}");
        };
        assert_eq!(
            serde_json::from_str::<Value>(raw.get()).expect("json"),
            *answer,
            "{prompt} caller answer is unchanged"
        );
        let expected = prompt
            .starts_with("benign")
            .then(|| ResponseCapture::Json(answer.clone()));
        assert_eq!(capture, expected, "{prompt} capture");
    }

    let (bytes, aborted, end) = collect(
        capturing(
            &dispatch,
            &target,
            CANARY,
            &json!({"model": "m", "messages": [], "user": "streamed", "stream": true}),
            true,
        )
        .await,
    )
    .await;
    assert!(!aborted);
    assert_eq!(String::from_utf8(bytes).expect("utf-8"), events);
    assert_eq!(end.outcome, GatewayCallOutcome::Succeeded);
    assert_eq!(
        end.capture, None,
        "an encoded streamed credential drops capture"
    );
}

/// A provider refusal holding the resolved credential as valid base64, which
/// capture would retain or decode into an object, relays only its status: a
/// JSON `data:` URL value, a JSON member name, non-JSON plain base64, and a
/// non-JSON base64 `data:` URL. Unrelated non-JSON text still relays.
///
/// # Panics
///
/// Panics when an attempt is not a 401 refusal, an encoded body holds the
/// encoding, or the unrelated text is not relayed.
#[tokio::test]
async fn refusals_holding_an_encoded_credential_relay_only_their_status() {
    const CANARY: &str = "sk-encoded-canary-32";
    let encoded = base64::engine::general_purpose::STANDARD.encode(CANARY);
    let server = MockServer::start().await;
    let refusals = [
        (
            "value",
            json!({"error": {"message": "bad key", "param": format!("data:text/plain;base64,{encoded}")}})
                .to_string(),
        ),
        (
            "key",
            json!({"error": {"message": "bad key", encoded.as_str(): "x"}}).to_string(),
        ),
        ("plain", encoded.clone()),
        ("url", format!("data:text/plain;base64,{encoded}")),
        ("benign", "rate limited, retry later".to_owned()),
    ];
    for (user, body) in &refusals {
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(wiremock::matchers::body_partial_json(json!({"user": user})))
            .respond_with(ResponseTemplate::new(401).set_body_raw(body.clone(), "text/plain"))
            .mount(&server)
            .await;
    }
    let dispatch = dispatch(&server);
    let target = deployment("openai/gpt-4o", json!("openai"), bearer());
    for (user, _) in &refusals {
        let refused = capturing(
            &dispatch,
            &target,
            CANARY,
            &json!({"model": "m", "messages": [], "user": user}),
            false,
        )
        .await;
        let AttemptResult::Refused {
            status: 401, body, ..
        } = &refused
        else {
            panic!("{user} refusal expected: {refused:?}");
        };
        if *user == "benign" {
            assert_eq!(*body, json!("rate limited, retry later"));
        } else {
            assert!(!body.to_string().contains(&encoded), "{user}: {body}");
            assert!(body.to_string().contains("HTTP 401"), "{user}: {body}");
        }
    }
}

/// Streams that stay open after a valid terminator settle as soon as the
/// terminator is delivered: native Chat Completions, native Responses, and a
/// translated Anthropic stream all succeed well before the deadline, deliver
/// nothing after the terminator, keep a capture matching the delivered
/// events, and close the upstream connection.
///
/// # Panics
///
/// Panics when a stream waits for the upstream to close, relays a
/// post-terminator byte, ends other than successful, keeps a capture that
/// does not match the delivered events, or leaves the upstream open.
#[tokio::test]
async fn held_open_streams_settle_at_their_terminator() {
    const CHAT: &str = "data: {\"id\":\"c1\",\"choices\":[],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":2,\"total_tokens\":3}}\n\ndata: [DONE]\n\n";
    const RESPONSES: &str = "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"r1\",\"status\":\"completed\",\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n";
    const ANTHROPIC: &str = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}}\n\nevent: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":2}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
    const LATE: &str = "data: {\"type\":\"late\"}\n\n";
    let cases: [(GatewayOperation, &'static str, &str, Value, usize); 3] = [
        (
            GatewayOperation::ChatCompletions,
            concat!(
                "data: {\"id\":\"c1\",\"choices\":[],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":2,\"total_tokens\":3}}\n\ndata: [DONE]\n\n",
                "data: {\"type\":\"late\"}\n\n"
            ),
            "openai",
            chat_stream(),
            1,
        ),
        (
            GatewayOperation::Responses,
            concat!(
                "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"r1\",\"status\":\"completed\",\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n",
                "data: {\"type\":\"late\"}\n\n"
            ),
            "openai",
            json!({"model": "m", "input": "hi", "stream": true}),
            1,
        ),
        (
            GatewayOperation::ChatCompletions,
            concat!(
                "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}}\n\nevent: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":2}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
                "data: {\"type\":\"late\"}\n\n"
            ),
            "anthropic",
            json!({"model": "m", "max_tokens": 5, "stream": true, "messages": [{"role": "user", "content": "hi"}]}),
            2,
        ),
    ];
    for (operation, frame, adapter, body, kept) in cases {
        let (port, upstream) = held_stream(frame).await;
        let (target, dispatch) = held_target(adapter, port);
        let secret = ProviderSecret::new("key".to_owned().into());
        let cancel = CancellationToken::new();
        let result = dispatch
            .dispatch(ProviderAttempt {
                call_id: GatewayCallId::from_uuid(uuid::Uuid::from_u128(0xabc)),
                ordinal: NonZeroU32::MIN,
                operation,
                ingress: IngressDialect::OpenAi,
                deployment: &target,
                credential: Some(&secret),
                body: &body,
                media: None,
                batch: None,
                stream: true,
                capture: true,
                deadline: later(),
                cancel: &cancel,
            })
            .await;
        let (bytes, aborted, end) = tokio::time::timeout(Duration::from_secs(10), collect(result))
            .await
            .expect("the stream settles at its terminator, not at connection close");
        let text = String::from_utf8(bytes).expect("utf-8");
        assert!(!aborted, "{text}");
        assert_eq!(end.outcome, GatewayCallOutcome::Succeeded, "{text}");
        assert!(!text.contains("late"), "no post-terminator byte: {text}");
        match adapter {
            "anthropic" => {
                assert!(text.ends_with("data: [DONE]\n\n"), "{text}");
                assert!(frame.starts_with(ANTHROPIC));
            }
            _ => assert!(
                text == CHAT || text == RESPONSES,
                "native bytes relay exactly through the terminator: {text}"
            ),
        }
        assert!(normalized(&end).is_some(), "terminator usage is reported");
        let Some(ResponseCapture::Json(capture)) = end.capture else {
            panic!("selected capture is kept as JSON");
        };
        let kept_events = capture.as_array().expect("captured events");
        assert_eq!(kept_events.len(), kept, "{capture}");
        assert!(!capture.to_string().contains("late"), "{capture}");
        assert!(upstream.await.expect("upstream task"), "upstream closed");
        assert!(frame.ends_with(LATE));
    }
}

/// Deployment and dispatch reaching a held upstream on `port`: the built-in
/// Anthropic endpoint for `anthropic`, otherwise an `OpenAI`-compatible base URL.
///
/// # Panics
///
/// Panics when the URL or dispatch cannot be built.
fn held_target(adapter: &str, port: u16) -> (ProviderDeployment, HttpProviderDispatch) {
    let base = url::Url::parse(&format!("http://127.0.0.1:{port}")).expect("url");
    if adapter == "anthropic" {
        let endpoints = BuiltinEndpoints {
            anthropic: Some(base),
            ..BuiltinEndpoints::default()
        };
        return (
            deployment("anthropic/claude", json!("anthropic"), api_key("x-api-key")),
            HttpProviderDispatch::new(EndpointPolicy::new(false), endpoints)
                .expect("dispatch builds"),
        );
    }
    (
        deployment(
            "acme/m",
            json!({"openai_compatible": {"base_url": format!("{base}v1")}}),
            bearer(),
        ),
        HttpProviderDispatch::new(EndpointPolicy::new(false), BuiltinEndpoints::default())
            .expect("dispatch builds"),
    )
}

/// A native non-JSON event other than the `[DONE]` sentinel relays unchanged
/// but drops the whole selected capture; a stream of JSON events and the
/// sentinel keeps its capture.
///
/// # Panics
///
/// Panics when the caller's bytes change, the stream fails, or the capture is
/// kept for a non-JSON event or dropped for a representable stream.
#[tokio::test]
async fn native_non_json_events_drop_selected_capture() {
    let server = MockServer::start().await;
    let valid = "data: {\"id\":\"c1\",\"choices\":[]}\n\ndata: [DONE]\n\n";
    let opaque = "data: {\"id\":\"c1\",\"choices\":[]}\n\ndata: keep-alive\n\ndata: [DONE]\n\n";
    for (user, answer) in [("valid", valid), ("opaque", opaque)] {
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(wiremock::matchers::body_partial_json(json!({"user": user})))
            .respond_with(ResponseTemplate::new(200).set_body_raw(answer, "text/event-stream"))
            .mount(&server)
            .await;
    }
    let dispatch = dispatch(&server);
    let target = deployment("openai/gpt-5", json!("openai"), bearer());
    for (user, answer, kept) in [("valid", valid, true), ("opaque", opaque, false)] {
        let body = json!({"model": "m", "messages": [], "user": user, "stream": true});
        let (bytes, aborted, end) =
            collect(capturing(&dispatch, &target, "key", &body, true).await).await;
        assert!(!aborted);
        assert_eq!(String::from_utf8(bytes).expect("utf-8"), answer);
        assert_eq!(end.outcome, GatewayCallOutcome::Succeeded);
        assert_eq!(end.capture.is_some(), kept, "{user}: {:?}", end.capture);
    }
}

/// A Gemini answer truncated before the model speaks still reaches the caller.
///
/// When thinking exhausts the output budget Gemini returns a `MAX_TOKENS`
/// candidate whose `content` is `{}`, with no `role` and no `parts`. That
/// answer must translate into an ordinary `length` choice with no content
/// rather than failing the call: a decode refusal here surfaces to the caller
/// as `502` on an exchange the provider completed and billed.
#[tokio::test]
async fn a_truncated_gemini_candidate_becomes_a_length_choice() {
    let server = MockServer::start().await;
    answer(
        &server,
        "/v1beta/models/gemini-2.5-flash:generateContent",
        "x-goog-api-key",
        "g-key",
        json!({
            "candidates": [{"content": {}, "finishReason": "MAX_TOKENS", "index": 0}],
            "usageMetadata": {"promptTokenCount": 8, "thoughtsTokenCount": 13, "totalTokenCount": 21},
        }),
    )
    .await;
    let gemini = deployment(
        "gemini/gemini-2.5-flash",
        json!("gemini"),
        api_key("x-goog-api-key"),
    );
    let result = attempt(
        &dispatch(&server),
        IngressDialect::OpenAi,
        &gemini,
        Some("g-key"),
        &json!({"model": "gemini/gemini-2.5-flash", "messages": [{"role": "user", "content": "hi"}]}),
    )
    .await;
    let AttemptResult::Completed { body, usage, .. } = &result else {
        panic!("the truncated answer completes the attempt: {result:?}");
    };
    let answer = parsed(body);
    assert_eq!(answer["choices"][0]["finish_reason"], json!("length"));
    assert_eq!(answer["choices"][0]["message"]["content"], Value::Null);
    assert_eq!(usage.normalized, Some(tokens(8, 13)));
}
