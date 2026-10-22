//! Native Anthropic Messages and Gemini `GenerateContent` ingress journeys.
//!
//! Each drives the public routes of a real bound server with a normal Wyrd
//! access token, or a Wyrd API key ingress exchanges for one, reaching the
//! shared local upstream through the built-in adapters.

use std::time::Duration;

use serde_json::{Value, json};
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, ResponseTemplate};
use wyrd_telemetry::{TelemetryConfig, init_test_capture};
use wyrd_testing::Bootstrap;
use wyrd_testing::gateway_capture::GatewayCapture;

use crate::harness::{
    ANTHROPIC_EVENTS, ANTHROPIC_TRUNCATED, GEMINI_EVENTS, GEMINI_TRUNCATED, INVOKER_ROLE, Journey,
    PROVIDER_KEY, anthropic_code, api_key, assert_provider_credentials, google_code, openai_code,
    refusal, tokens,
};

/// Proves `POST /v1/messages` for an Anthropic SDK caller.
///
/// A message and a stream relay native bytes and account the mock's usage; a
/// truncated stream never completes; and conflicting headers, a query token,
/// a missing or invalid token, a reader, a model whose deployment lacks the
/// capability, a cross-provider fallback, and a malformed body are refused in
/// the Anthropic envelope with `wyrd-request-id` and no upstream call. No
/// production-shaped span, including the root request span of a refused
/// `?key=` call, records the caller token.
///
/// # Panics
/// Panics when any status, body, stream, captured row, or upstream expectation
/// fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn anthropic_messages_ingress_is_native_governed_and_fail_closed() {
    let (_telemetry, traces) = init_test_capture(TelemetryConfig::default())
        .expect("production-shaped trace capture installs");
    let journey = Journey::start().await;
    journey.capture_metadata().await;
    let message = json!({"id": "msg_1", "type": "message", "role": "assistant", "model": "claude-sonnet-5", "content": [{"type": "text", "text": "hi"}], "stop_reason": "end_turn", "stop_sequence": null, "usage": {"input_tokens": 5, "output_tokens": 3}, "container": null});
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(body_string_contains("truncate-me"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(ANTHROPIC_TRUNCATED, "text/event-stream"),
        )
        .with_priority(1)
        .mount(&journey.upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(body_string_contains("\"stream\":true"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(ANTHROPIC_EVENTS, "text/event-stream"),
        )
        .with_priority(2)
        .mount(&journey.upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(message.clone()))
        .with_priority(3)
        .mount(&journey.upstream)
        .await;
    journey
        .deploy("anthropic", "x-api-key", &["chat_completions"])
        .await;
    allow_only(&journey, "anthropic", "x-api-key", "claude-sonnet-5").await;
    journey
        .deploy("openai", "authorization", &["chat_completions"])
        .await;
    journey
        .put(
            "fallback-policy",
            json!({"rules": [{"scope": {"model": {"model": {"deployment": "anthropic", "model": "claude-missing"}}},
                              "candidates": [{"deployment": "openai", "model": "gpt-4o"}]}]}),
        )
        .await;
    let key = |token: &str| {
        vec![
            ("x-api-key", token.to_owned()),
            ("anthropic-version", "2023-06-01".to_owned()),
        ]
    };
    let body = |model: &str, extra: Value| {
        let mut body = json!({"model": model, "max_tokens": 16, "messages": [{"role": "user", "content": "hi"}]});
        body.as_object_mut()
            .expect("object")
            .extend(extra.as_object().expect("object").clone());
        body
    };

    let answer = journey
        .post(
            "/v1/messages",
            &key(&journey.caller),
            &body("anthropic/claude-sonnet-5", json!({"service_tier": "auto"})),
        )
        .await;
    assert_eq!(answer.status().as_u16(), 200);
    assert_eq!(
        answer.json::<Value>().await.expect("message json"),
        message,
        "the message relays unchanged, including its unmodeled `container`"
    );
    assert_eq!(
        journey.upstream_calls().await[0]
            .body_json::<Value>()
            .expect("upstream body")["service_tier"],
        "auto",
        "an unmodeled request member reaches the provider"
    );
    let bearer = vec![("authorization", format!("Bearer {}", journey.caller))];
    let stream = journey
        .post(
            "/v1/messages",
            &bearer,
            &body("anthropic/claude-sonnet-5", json!({"stream": true})),
        )
        .await;
    assert_eq!(stream.status().as_u16(), 200);
    assert_eq!(stream.headers()["content-type"], "text/event-stream");
    assert_eq!(
        stream.text().await.expect("complete stream"),
        ANTHROPIC_EVENTS
    );
    assert_eq!(
        journey.settled_usage(2).await,
        vec![tokens(5, 3), tokens(5, 3)],
        "the captured rows account the mock's message and stream usage"
    );
    let truncated = journey
        .post(
            "/v1/messages",
            &key(&journey.caller),
            &body(
                "anthropic/claude-sonnet-5",
                json!({"stream": true, "metadata": {"user_id": "truncate-me"}}),
            ),
        )
        .await;
    assert_eq!(truncated.status().as_u16(), 200);
    let events = truncated
        .text()
        .await
        .expect("the relay ends with an in-band error");
    assert!(
        events.ends_with("\n\n")
            && events.contains("event: error\n")
            && !events.contains("message_stop"),
        "a truncated stream ends with an Anthropic error event, never message_stop: {events}"
    );
    let dispatched = journey.upstream_calls().await;
    assert_eq!(dispatched.len(), 3);
    assert_provider_credentials(&dispatched, "x-api-key", PROVIDER_KEY, &journey.caller);

    let valid = body("anthropic/claude-sonnet-5", json!({}));
    let both = vec![
        ("x-api-key", journey.caller.clone()),
        ("authorization", format!("Bearer {}", journey.caller)),
    ];
    refusal(
        journey.post("/v1/messages", &both, &valid).await,
        400,
        "WYRD_AUTH_400_BAD_TOKEN_FORMAT",
        anthropic_code,
    )
    .await;
    let google_header = vec![("x-goog-api-key", journey.caller.clone())];
    refusal(
        journey.post("/v1/messages", &google_header, &valid).await,
        400,
        "WYRD_AUTH_400_BAD_TOKEN_FORMAT",
        anthropic_code,
    )
    .await;
    let query = format!("/v1/messages?key={}", journey.caller);
    refusal(
        journey.post(&query, &key(&journey.caller), &valid).await,
        400,
        "WYRD_AUTH_400_BAD_TOKEN_FORMAT",
        anthropic_code,
    )
    .await;
    let limit = format!("/v1/chat/completions?limit={}", journey.caller);
    refusal(
        journey
            .post(
                &limit,
                &bearer,
                &json!({"model": "openai/gpt-4o", "max_completion_tokens": 1, "messages": [{"role": "user", "content": "hi"}]}),
            )
            .await,
        400,
        "WYRD_AUTH_400_BAD_TOKEN_FORMAT",
        openai_code,
    )
    .await;
    let page = journey
        .http
        .get(format!("{}/v1/batches?limit=1", journey.base))
        .bearer_auth(&journey.caller)
        .send()
        .await
        .expect("batch list sends");
    assert_eq!(
        page.status().as_u16(),
        200,
        "batch-list pagination keeps its query: {}",
        page.text().await.unwrap_or_default()
    );
    refusal(
        journey.post("/v1/messages", &[], &valid).await,
        401,
        "WYRD_AUTH_401_UNAUTHENTICATED",
        anthropic_code,
    )
    .await;
    refusal(
        journey
            .post("/v1/messages", &key("sk-ant-not-a-wyrd-token"), &valid)
            .await,
        400,
        "WYRD_AUTH_400_BAD_TOKEN_FORMAT",
        anthropic_code,
    )
    .await;
    refusal(
        journey
            .post("/v1/messages", &key(&journey.reader), &valid)
            .await,
        403,
        "WYRD_PERMISSION_403_DENIED_RBAC",
        anthropic_code,
    )
    .await;
    refusal(
        journey
            .post(
                "/v1/messages",
                &key(&journey.caller),
                &body("anthropic/claude-missing", json!({})),
            )
            .await,
        400,
        "WYRD_GATEWAY_400_INVALID_REQUEST",
        anthropic_code,
    )
    .await;
    refusal(
        journey
            .post(
                "/v1/messages",
                &key(&journey.caller),
                &json!({"max_tokens": 1}),
            )
            .await,
        400,
        "WYRD_GATEWAY_400_INVALID_REQUEST",
        anthropic_code,
    )
    .await;
    assert_eq!(
        journey.upstream_calls().await.len(),
        3,
        "no refusal reaches a provider, including the OpenAI fallback candidate"
    );
    let spans = traces.finished_since(0);
    assert!(
        spans.iter().any(|span| span.name == "request"),
        "the HTTP request span is captured"
    );
    for span in &spans {
        for (field, value) in &span.attributes {
            assert!(
                !value.contains(&journey.caller),
                "span {} records a caller token in {field}",
                span.name
            );
        }
    }
}

/// Proves `POST /v1beta/models/{model}:generateContent` and
/// `:streamGenerateContent?alt=sse` for a Google GenAI SDK caller.
///
/// Both relay native bytes and account the mock's usage; a truncated stream
/// never completes; and conflicting headers, a query key, an Anthropic header,
/// a repeated `alt` carrying a token, a reader, an unknown method, a non-SSE stream, a body without `contents`,
/// and a cross-provider fallback are refused in the Google envelope with `wyrd-request-id` and no
/// upstream call.
///
/// # Panics
/// Panics when any status, body, stream, captured row, or upstream expectation
/// fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn gemini_generate_content_ingress_is_native_governed_and_fail_closed() {
    let journey = Journey::start().await;
    journey.capture_metadata().await;
    let answer = json!({"candidates": [{"content": {"role": "model", "parts": [{"text": "hi"}]}, "finishReason": "STOP", "index": 0}], "usageMetadata": {"promptTokenCount": 7, "candidatesTokenCount": 2, "totalTokenCount": 9}, "modelVersion": "gemini-2.5-flash", "responseId": "resp_1"});
    Mock::given(method("POST"))
        .and(path(
            "/v1beta/models/gemini-2.5-flash:streamGenerateContent",
        ))
        .and(body_string_contains("truncate-me"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(GEMINI_TRUNCATED, "text/event-stream"),
        )
        .with_priority(1)
        .mount(&journey.upstream)
        .await;
    Mock::given(method("POST"))
        .and(path(
            "/v1beta/models/gemini-2.5-flash:streamGenerateContent",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_raw(GEMINI_EVENTS, "text/event-stream"))
        .with_priority(2)
        .mount(&journey.upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-2.5-flash:generateContent"))
        .respond_with(ResponseTemplate::new(200).set_body_json(answer.clone()))
        .mount(&journey.upstream)
        .await;
    journey
        .deploy("gemini", "x-goog-api-key", &["chat_completions"])
        .await;
    allow_only(&journey, "gemini", "x-goog-api-key", "gemini-2.5-flash").await;
    journey
        .deploy("openai", "authorization", &["chat_completions"])
        .await;
    journey
        .put(
            "fallback-policy",
            json!({"rules": [{"scope": {"model": {"model": {"deployment": "gemini", "model": "gemini-missing"}}},
                              "candidates": [{"deployment": "openai", "model": "gpt-4o"}]}]}),
        )
        .await;
    let key = |token: &str| vec![("x-goog-api-key", token.to_owned())];
    let body = |text: &str| json!({"contents": [{"role": "user", "parts": [{"text": text}]}], "generationConfig": {"maxOutputTokens": 16}});
    let generate = "/v1beta/models/gemini/gemini-2.5-flash:generateContent";
    let stream_route = "/v1beta/models/gemini/gemini-2.5-flash:streamGenerateContent?alt=sse";

    let mut labeled = body("hi");
    labeled["labels"] = json!({"team": "wyrd"});
    let response = journey
        .post(generate, &key(&journey.caller), &labeled)
        .await;
    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(
        response.json::<Value>().await.expect("answer json"),
        answer,
        "the answer relays unchanged, including its unmodeled `responseId`"
    );
    assert_eq!(
        journey.upstream_calls().await[0]
            .body_json::<Value>()
            .expect("upstream body")["labels"],
        json!({"team": "wyrd"}),
        "an unmodeled request member reaches the provider"
    );
    let bearer = vec![("authorization", format!("Bearer {}", journey.caller))];
    let stream = journey.post(stream_route, &bearer, &body("hi")).await;
    assert_eq!(stream.status().as_u16(), 200);
    assert_eq!(stream.headers()["content-type"], "text/event-stream");
    assert_eq!(stream.text().await.expect("complete stream"), GEMINI_EVENTS);
    assert_eq!(
        journey.settled_usage(2).await,
        vec![tokens(7, 2), tokens(7, 2)],
        "the captured rows account the mock's answer and stream usage"
    );
    let mut truncated = journey
        .http
        .post(format!("{}{stream_route}", journey.base))
        .json(&body("truncate-me"));
    truncated = truncated.header("x-goog-api-key", &journey.caller);
    let complete = match truncated.send().await {
        Ok(response) => response.text().await.is_ok(),
        Err(_) => false,
    };
    assert!(
        !complete,
        "Google SSE has no in-band error, so a truncated stream aborts the transport"
    );
    let dispatched = journey.upstream_calls().await;
    assert_eq!(dispatched.len(), 3);
    assert_provider_credentials(&dispatched, "x-goog-api-key", PROVIDER_KEY, &journey.caller);

    let valid = body("hi");
    let both = vec![
        ("x-goog-api-key", journey.caller.clone()),
        ("x-wyrd-access-token", format!("Bearer {}", journey.caller)),
    ];
    refusal(
        journey.post(generate, &both, &valid).await,
        400,
        "WYRD_AUTH_400_BAD_TOKEN_FORMAT",
        google_code,
    )
    .await;
    let anthropic_header = vec![("x-api-key", journey.caller.clone())];
    refusal(
        journey.post(generate, &anthropic_header, &valid).await,
        400,
        "WYRD_AUTH_400_BAD_TOKEN_FORMAT",
        google_code,
    )
    .await;
    let query = format!("{generate}?key={}", journey.caller);
    refusal(
        journey.post(&query, &[], &valid).await,
        400,
        "WYRD_AUTH_400_BAD_TOKEN_FORMAT",
        google_code,
    )
    .await;
    let repeated_alt = format!("{generate}?alt=json&alt={}", journey.caller);
    refusal(
        journey
            .post(&repeated_alt, &key(&journey.caller), &valid)
            .await,
        400,
        "WYRD_GATEWAY_400_INVALID_REQUEST",
        google_code,
    )
    .await;
    let denied = refusal(
        journey.post(generate, &key(&journey.reader), &valid).await,
        403,
        "WYRD_PERMISSION_403_DENIED_RBAC",
        google_code,
    )
    .await;
    assert_eq!(denied["error"]["status"], "PERMISSION_DENIED");
    refusal(
        journey
            .post(
                "/v1beta/models/gemini/gemini-2.5-flash:countTokens",
                &key(&journey.caller),
                &valid,
            )
            .await,
        400,
        "WYRD_GATEWAY_400_INVALID_REQUEST",
        google_code,
    )
    .await;
    refusal(
        journey
            .post(
                "/v1beta/models/gemini/gemini-2.5-flash:streamGenerateContent",
                &key(&journey.caller),
                &valid,
            )
            .await,
        400,
        "WYRD_GATEWAY_400_INVALID_REQUEST",
        google_code,
    )
    .await;
    refusal(
        journey
            .post(
                "/v1beta/models/gemini/gemini-missing:generateContent",
                &key(&journey.caller),
                &valid,
            )
            .await,
        400,
        "WYRD_GATEWAY_400_INVALID_REQUEST",
        google_code,
    )
    .await;
    let missing = refusal(
        journey
            .post(
                generate,
                &key(&journey.caller),
                &json!({"generationConfig": {"maxOutputTokens": 16}}),
            )
            .await,
        400,
        "WYRD_GATEWAY_400_INVALID_REQUEST",
        google_code,
    )
    .await;
    assert!(
        missing["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("contents")),
        "a body without `contents` names the missing member: {missing}"
    );
    assert_eq!(
        journey.upstream_calls().await.len(),
        3,
        "no refusal reaches a provider, including the OpenAI fallback candidate"
    );
    let openai = journey
        .post(
            "/v1/chat/completions",
            &[("x-api-key", journey.caller.clone())],
            &json!({"model": "openai/gpt-4o", "messages": []}),
        )
        .await;
    assert_eq!(
        openai.status().as_u16(),
        400,
        "OpenAI routes accept only bearer carriers"
    );
}

/// Restricts the deployment `provider` [`Journey::deploy`] stored, which
/// authenticates with `header`, to the single model `model`, so any other
/// model on it is unservable and only its fallback can answer.
///
/// # Panics
/// Panics when the write is refused.
async fn allow_only(journey: &Journey, provider: &str, header: &str, model: &str) {
    journey
        .put(
            &format!("provider-deployments/{provider}"),
            json!({
                "name": provider,
                "adapter": provider,
                "auth": {"api_key_header": {"header": header, "credential": format!("{provider}-key")}},
                "models": [model],
                "capabilities": ["chat_completions"],
            }),
        )
        .await;
}

/// The `wyrd-request-id` of a gateway answer.
///
/// # Panics
/// Panics when the answer carries no request id.
fn request_id(response: &reqwest::Response) -> String {
    response
        .headers()
        .get("wyrd-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("the answer carries its request id")
        .to_owned()
}

/// Retained audit operations of one request, as `(operation, principal)`
/// pairs in decision order, once the process Scribe outbox has settled.
///
/// # Panics
/// Panics when the outbox does not settle or retained audit cannot be read.
async fn decisions(journey: &Journey, request_id: &str) -> Vec<(String, String)> {
    journey
        .server
        .await_audit_retained()
        .await
        .expect("the Scribe outbox settles");
    journey
        .server
        .retained_audit_records(
            journey.server.data_tenant_id(),
            "operation, sub_principal_id",
            &format!("request_id = '{request_id}'"),
        )
        .await
        .expect("retained audit reads")
        .into_iter()
        .map(|row| {
            let mut cells = row.into_iter().map(Option::unwrap_or_default);
            (
                cells.next().unwrap_or_default(),
                cells.next().unwrap_or_default(),
            )
        })
        .collect()
}

/// Proves a Wyrd API key presented directly to every gateway ingress dialect.
///
/// A static key in each accepted carrier (`Authorization` and
/// `X-Wyrd-Access-Token` bearers on `OpenAI`, `x-api-key` on Anthropic,
/// `x-goog-api-key` on Gemini) is exchanged once, audited as one
/// `auth.token.exchange`, and then reused: later calls carry only their own
/// gateway decisions. A warmed key confers nothing on another key, so a
/// reader's key is still denied before dispatch, and a second tenant's key
/// sees only its own deployments and captures. Malformed keys earn the API-key
/// error in each native envelope, ambiguous and URL-borne keys stay refused,
/// and no refusal reaches a provider. A retired key, whose principal stays
/// active, keeps working only from its cached exchange, never beyond sixty
/// seconds after it, however often it is used. The raw key reaches no provider request, response, refusal,
/// captured call row, or production-shaped span.
///
/// # Panics
/// Panics when any status, envelope, audit, capture, upstream, or
/// span expectation fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn gateway_api_key_exchange_is_scoped_cached_and_revocable() {
    use secrecy::ExposeSecret as _;
    use std::time::Instant;

    let (_telemetry, traces) = init_test_capture(TelemetryConfig::default())
        .expect("production-shaped trace capture installs");
    let journey = Journey::start().await;
    let message = json!({"id": "msg_1", "type": "message", "role": "assistant", "model": "claude-sonnet-5", "content": [{"type": "text", "text": "hi"}], "stop_reason": "end_turn", "stop_sequence": null, "usage": {"input_tokens": 5, "output_tokens": 3}});
    let answer = json!({"candidates": [{"content": {"role": "model", "parts": [{"text": "hi"}]}, "finishReason": "STOP", "index": 0}], "usageMetadata": {"promptTokenCount": 7, "candidatesTokenCount": 2, "totalTokenCount": 9}, "modelVersion": "gemini-2.5-flash"});
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(GatewayCapture::completion()))
        .mount(&journey.upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(message.clone()))
        .mount(&journey.upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-2.5-flash:generateContent"))
        .respond_with(ResponseTemplate::new(200).set_body_json(answer.clone()))
        .mount(&journey.upstream)
        .await;
    journey
        .deploy("openai", "authorization", &["chat_completions"])
        .await;
    journey
        .deploy("anthropic", "x-api-key", &["chat_completions"])
        .await;
    journey
        .deploy("gemini", "x-goog-api-key", &["chat_completions"])
        .await;
    journey
        .put(
            "capture-policy",
            json!({"mode": "metadata", "payload_fields": []}),
        )
        .await;
    let Bootstrap::Machine {
        api_key: retired,
        id: retired_id,
        ..
    } = journey
        .server
        .bootstrap_service("key_retired", &[INVOKER_ROLE])
        .await
        .expect("retired caller bootstraps")
    else {
        panic!("service bootstrap returned a user principal");
    };
    let Bootstrap::Machine {
        api_key: caller,
        id: caller_id,
        ..
    } = journey
        .server
        .bootstrap_service("key_caller", &[INVOKER_ROLE])
        .await
        .expect("caller bootstraps")
    else {
        panic!("service bootstrap returned a user principal");
    };
    let reader = api_key(&journey.server, "key_reader", &[]).await;
    let inspector = api_key(&journey.server, "key_inspector", &["admin"]).await;
    let keys = [
        retired.expose_secret().to_owned(),
        caller.expose_secret().to_owned(),
        reader.expose_secret().to_owned(),
    ];
    let bearer = |key: &str| vec![("authorization", format!("Bearer {key}"))];
    let anthropic = |key: &str| {
        vec![
            ("x-api-key", key.to_owned()),
            ("anthropic-version", "2023-06-01".to_owned()),
        ]
    };
    let google = |key: &str| vec![("x-goog-api-key", key.to_owned())];
    let chat = json!({"model": "openai/gpt-4o", "max_completion_tokens": 16, "messages": [{"role": "user", "content": "hi"}]});
    let messages = json!({"model": "anthropic/claude-sonnet-5", "max_tokens": 16, "messages": [{"role": "user", "content": "hi"}]});
    let contents = json!({"contents": [{"role": "user", "parts": [{"text": "hi"}]}], "generationConfig": {"maxOutputTokens": 16}});
    let generate = "/v1beta/models/gemini/gemini-2.5-flash:generateContent";
    let mut bodies = Vec::new();

    // Warm the key that is retired below, then retire that one credential
    // while its principal stays active: the cached exchange keeps working,
    // but only until sixty seconds after that exchange.
    let warmed_at = Instant::now();
    let warm = journey
        .post("/v1/messages", &anthropic(&keys[0]), &messages)
        .await;
    let warmed_by = Instant::now();
    assert_eq!(warm.status().as_u16(), 200);
    bodies.push(warm.text().await.expect("answer"));
    let admin = |request: reqwest::RequestBuilder| {
        request.header("x-wyrd-access-token", format!("Bearer {}", journey.admin))
    };
    let listed: Value = admin(journey.http.get(format!(
        "{}/v1/principals/{retired_id}/credentials",
        journey.base
    )))
    .send()
    .await
    .expect("credential list sends")
    .json()
    .await
    .expect("credential list parses");
    let credentials = listed["credentials"].as_array().expect("credential list");
    assert_eq!(
        credentials.len(),
        1,
        "the principal holds the warmed key only"
    );
    let credential = &credentials[0];
    assert!(
        keys[0].contains(credential["prefix"].as_str().expect("prefix")),
        "the listed credential is the warmed key"
    );
    let retire = admin(journey.http.delete(format!(
        "{}/v1/principals/{retired_id}/credentials/{}",
        journey.base,
        credential["id"].as_str().expect("credential id")
    )))
    .send()
    .await
    .expect("retirement sends");
    assert_eq!(
        retire.status().as_u16(),
        204,
        "the warmed credential is retired"
    );
    let principals: Value = admin(journey.http.get(format!(
        "{}/v1/principals?kind=service&name=key_retired",
        journey.base
    )))
    .send()
    .await
    .expect("principal read sends")
    .json()
    .await
    .expect("principal read parses");
    assert_eq!(
        principals["principals"][0]["status"], "active",
        "only the credential is retired, never its principal: {principals}"
    );
    assert!(
        journey.server.exchange_api_key(&retired).await.is_err(),
        "the retired key no longer exchanges at /auth/token"
    );
    let cached = journey
        .post("/v1/messages", &anthropic(&keys[0]), &messages)
        .await;
    assert_eq!(
        cached.status().as_u16(),
        200,
        "the cached exchange is reused"
    );
    bodies.push(cached.text().await.expect("answer"));

    // A cold key is exchanged once and audited as that exchange; every later
    // carrier and dialect reuses the token and audits only its own call.
    let cold = journey
        .post("/v1/chat/completions", &bearer(&keys[1]), &chat)
        .await;
    assert_eq!(cold.status().as_u16(), 200);
    let cold_id = request_id(&cold);
    bodies.push(cold.text().await.expect("answer"));
    let mut hits = Vec::new();
    for (route, headers, body) in [
        (
            "/v1/chat/completions",
            vec![("x-wyrd-access-token", format!("Bearer {}", keys[1]))],
            &chat,
        ),
        ("/v1/messages", anthropic(&keys[1]), &messages),
        (generate, google(&keys[1]), &contents),
    ] {
        let response = journey.post(route, &headers, body).await;
        assert_eq!(response.status().as_u16(), 200, "{route}");
        hits.push(request_id(&response));
        bodies.push(response.text().await.expect("answer"));
    }
    let caller_principal = caller_id.to_string();
    let exchanges = |rows: &[(String, String)]| {
        rows.iter()
            .filter(|(operation, _)| operation == "auth.token.exchange")
            .map(|(_, principal)| principal.clone())
            .collect::<Vec<_>>()
    };
    let cold_decisions = decisions(&journey, &cold_id).await;
    assert_eq!(
        exchanges(&cold_decisions),
        std::slice::from_ref(&caller_principal),
        "the miss stages the key's one exchange decision: {cold_decisions:?}"
    );
    assert!(
        cold_decisions
            .iter()
            .any(|(operation, principal)| operation.starts_with("gateway.")
                && *principal == caller_principal),
        "the miss is still authorized as the key's principal: {cold_decisions:?}"
    );
    for hit in &hits {
        let hit_decisions = decisions(&journey, hit).await;
        assert!(
            exchanges(&hit_decisions).is_empty(),
            "a hit stages no exchange: {hit_decisions:?}"
        );
        assert!(
            hit_decisions
                .iter()
                .any(|(operation, principal)| operation.starts_with("gateway.")
                    && *principal == caller_principal),
            "a hit is still authorized and audited as the key's principal: {hit_decisions:?}"
        );
    }

    // A warmed key confers nothing on another key of the same tenant.
    let dispatched = journey.upstream_calls().await.len();
    for _ in 0..2 {
        bodies.push(
            refusal(
                journey
                    .post("/v1/messages", &anthropic(&keys[2]), &messages)
                    .await,
                403,
                "WYRD_PERMISSION_403_DENIED_RBAC",
                anthropic_code,
            )
            .await
            .to_string(),
        );
    }

    // Malformed keys earn the API-key error in each envelope; ambiguous and
    // URL-borne keys stay refused before any exchange.
    let malformed = "wyrd_sk_not-a-key";
    for (route, headers, body, code_of) in [
        (
            "/v1/chat/completions",
            bearer(malformed),
            &chat,
            openai_code as fn(&Value) -> Option<&str>,
        ),
        (
            "/v1/messages",
            anthropic(malformed),
            &messages,
            anthropic_code,
        ),
        (generate, google(malformed), &contents, google_code),
    ] {
        bodies.push(
            refusal(
                journey.post(route, &headers, body).await,
                401,
                "WYRD_AUTH_401_API_KEY_INVALID",
                code_of,
            )
            .await
            .to_string(),
        );
    }
    let mut both = anthropic(&keys[1]);
    both.push(("authorization", format!("Bearer {}", keys[1])));
    let query = format!("/v1/messages?key={}", keys[1]);
    for (route, headers, status, code) in [
        ("/v1/messages", both, 400, "WYRD_AUTH_400_BAD_TOKEN_FORMAT"),
        (
            query.as_str(),
            anthropic(&keys[1]),
            400,
            "WYRD_AUTH_400_BAD_TOKEN_FORMAT",
        ),
        (
            "/v1/messages",
            google(&keys[1]),
            400,
            "WYRD_AUTH_400_BAD_TOKEN_FORMAT",
        ),
        (
            "/v1/messages",
            Vec::new(),
            401,
            "WYRD_AUTH_401_UNAUTHENTICATED",
        ),
    ] {
        bodies.push(
            refusal(
                journey.post(route, &headers, &messages).await,
                status,
                code,
                anthropic_code,
            )
            .await
            .to_string(),
        );
    }
    assert_eq!(
        journey.upstream_calls().await.len(),
        dispatched,
        "no refused key reaches a provider"
    );

    // A second tenant's key sees only its own deployments and captures.
    let other = journey
        .server
        .seed_tenant("gateway-key-other")
        .await
        .expect("second tenant seeds");
    let Bootstrap::Machine {
        api_key: other_key, ..
    } = journey
        .server
        .bootstrap_service_in_tenant(other, "key_other", &["admin"])
        .await
        .expect("second-tenant service bootstraps")
    else {
        panic!("service bootstrap returned a user principal");
    };
    let other_key = other_key.expose_secret().to_owned();
    bodies.push(
        refusal(
            journey
                .post("/v1/messages", &anthropic(&other_key), &messages)
                .await,
            404,
            "WYRD_GATEWAY_404_MODEL_UNAVAILABLE",
            anthropic_code,
        )
        .await
        .to_string(),
    );
    let other_admin = journey
        .server
        .exchange_api_key(&secrecy::SecretString::from(other_key.clone()))
        .await
        .expect("second-tenant key exchanges");
    journey
        .deploy_as(
            &other_admin,
            "anthropic",
            "x-api-key",
            &["chat_completions"],
        )
        .await;
    journey
        .put_as(
            &other_admin,
            "capture-policy",
            json!({"mode": "metadata", "payload_fields": []}),
        )
        .await;
    let own = journey
        .post("/v1/messages", &anthropic(&other_key), &messages)
        .await;
    assert_eq!(
        own.status().as_u16(),
        200,
        "the second tenant's own deployment serves it"
    );
    bodies.push(own.text().await.expect("answer"));
    let settled = journey.settled_usage(6).await;
    assert_eq!(
        settled.len(),
        6,
        "the first tenant's captures hold only its own six calls: {settled:?}"
    );
    assert_eq!(
        journey
            .captured_calls(&secrecy::SecretString::from(other_key.clone()), 1)
            .await
            .len(),
        1,
        "the second tenant's captures hold only its call"
    );

    // Repeated hits never extend the retired key's window.
    let mid = warmed_at + Duration::from_secs(50);
    assert!(
        Instant::now() < mid,
        "the journey reached the mid-window check before it"
    );
    tokio::time::sleep_until(mid.into()).await;
    let late = journey
        .post("/v1/messages", &anthropic(&keys[0]), &messages)
        .await;
    assert_eq!(
        late.status().as_u16(),
        200,
        "a hit inside the window is reused"
    );
    bodies.push(late.text().await.expect("answer"));
    tokio::time::sleep_until((warmed_by + Duration::from_secs(61)).into()).await;
    let dispatched = journey.upstream_calls().await.len();
    bodies.push(
        refusal(
            journey
                .post("/v1/messages", &anthropic(&keys[0]), &messages)
                .await,
            401,
            "WYRD_AUTH_401_API_KEY_INVALID",
            anthropic_code,
        )
        .await
        .to_string(),
    );
    assert_eq!(
        journey.upstream_calls().await.len(),
        dispatched,
        "the retired key reaches no provider once its window ends"
    );

    // The raw key reaches no provider, response, capture row, or span.
    let calls = journey.upstream_calls().await;
    for (prefix, header) in [
        ("/v1/chat", "authorization"),
        ("/v1/messages", "x-api-key"),
        ("/v1beta", "x-goog-api-key"),
    ] {
        let routed: Vec<_> = calls
            .iter()
            .filter(|call| call.url.path().starts_with(prefix))
            .cloned()
            .collect();
        assert!(!routed.is_empty(), "{prefix} was dispatched");
        let expected = if header == "authorization" {
            format!("Bearer {PROVIDER_KEY}")
        } else {
            PROVIDER_KEY.to_owned()
        };
        for key in keys.iter().chain([&other_key]) {
            assert_provider_credentials(&routed, header, &expected, key);
        }
    }
    let rows = journey.captured_calls(&inspector, 7).await;
    let spans = traces.finished_since(0);
    for key in keys.iter().chain([&other_key]) {
        for body in &bodies {
            assert!(
                !body.contains(key.as_str()),
                "a response carries a key: {body}"
            );
        }
        for row in &rows {
            assert!(
                !row.contains(key.as_str()),
                "a captured call carries a key: {row}"
            );
        }
        for span in &spans {
            for (field, value) in &span.attributes {
                assert!(
                    !value.contains(key.as_str()),
                    "span {} records a key in {field}",
                    span.name
                );
            }
        }
    }
}

/// Proves an API key, even one gateway ingress has already exchanged and
/// cached, stays refused by authentication at `/mcp`, a `/v1` management
/// route, and Bifrost, while an access token still passes each.
///
/// # Panics
/// Panics when a surface accepts the key or refuses the access token.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn gateway_api_keys_remain_rejected_outside_ingress() {
    use secrecy::ExposeSecret as _;

    let journey = Journey::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(GatewayCapture::completion()))
        .mount(&journey.upstream)
        .await;
    journey
        .deploy("openai", "authorization", &["chat_completions"])
        .await;
    let key = api_key(&journey.server, "key_admin", &["admin"]).await;
    let key = key.expose_secret().to_owned();
    let token = journey
        .server
        .exchange_api_key(&secrecy::SecretString::from(key.clone()))
        .await
        .expect("the key exchanges");
    let warm = journey
        .post(
            "/v1/chat/completions",
            &[("authorization", format!("Bearer {key}"))],
            &json!({"model": "openai/gpt-4o", "max_completion_tokens": 16, "messages": [{"role": "user", "content": "hi"}]}),
        )
        .await;
    assert_eq!(
        warm.status().as_u16(),
        200,
        "gateway ingress caches the key"
    );

    let surfaces = [
        (
            reqwest::Method::POST,
            "/mcp",
            Some(json!({"jsonrpc": "2.0", "id": 1, "method": "ping"})),
        ),
        (
            reqwest::Method::GET,
            "/v1/admin/gateway/provider-deployments",
            None,
        ),
        (
            reqwest::Method::POST,
            "/v1/query",
            Some(json!({"sql": "SELECT 1"})),
        ),
    ];
    for (method, route, body) in surfaces {
        let send = |header: &'static str, credential: String| {
            let mut request = journey
                .http
                .request(method.clone(), format!("{}{route}", journey.base))
                .header("accept", "application/json, text/event-stream")
                .header(header, credential);
            if let Some(body) = &body {
                request = request.json(body);
            }
            request.send()
        };
        for (header, credential) in [
            ("x-wyrd-access-token", format!("Bearer {key}")),
            ("authorization", format!("Bearer {key}")),
        ] {
            let refused = send(header, credential).await.expect("request sends");
            let status = refused.status().as_u16();
            let problem: Value = refused.json().await.expect("refusal is JSON");
            assert!(
                matches!(
                    (status, problem["code"].as_str()),
                    (400, Some("WYRD_AUTH_400_BAD_TOKEN_FORMAT"))
                        | (401, Some("WYRD_AUTH_401_UNAUTHENTICATED"))
                ),
                "{route} via {header} refuses the key by authentication: {status} {problem}"
            );
        }
        let accepted = send("x-wyrd-access-token", format!("Bearer {token}"))
            .await
            .expect("request sends");
        let status = accepted.status().as_u16();
        let text = accepted.text().await.unwrap_or_default();
        assert!(
            status != 401 && status != 404 && !text.contains("WYRD_AUTH_"),
            "{route} serves an access token: {status} {text}"
        );
    }
}
