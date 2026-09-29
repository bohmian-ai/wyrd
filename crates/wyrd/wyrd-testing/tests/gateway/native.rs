//! Native Anthropic Messages and Gemini `GenerateContent` ingress journeys.
//!
//! Each drives the public routes of a real bound server with a normal Wyrd
//! access token, reaching the shared local upstream through the built-in
//! adapters.

use serde_json::{Value, json};
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, ResponseTemplate};
use wyrd_telemetry::{TelemetryConfig, init_test_capture};

use crate::harness::{
    ANTHROPIC_EVENTS, ANTHROPIC_TRUNCATED, GEMINI_EVENTS, GEMINI_TRUNCATED, Journey, PROVIDER_KEY,
    anthropic_code, assert_provider_credentials, google_code, openai_code, refusal, tokens,
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
/// Panics when any status, body, stream, ledger, or upstream expectation fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn anthropic_messages_ingress_is_native_governed_and_fail_closed() {
    let (_telemetry, traces) = init_test_capture(TelemetryConfig {
        filter: "info,tower_http=debug,wyrd_server::http=debug".to_owned(),
        ..TelemetryConfig::default()
    })
    .expect("production-shaped trace capture installs");
    let journey = Journey::start().await;
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
        .deploy(
            "anthropic",
            "x-api-key",
            "claude-sonnet-5",
            &["chat_completions"],
        )
        .await;
    journey
        .deploy("openai", "authorization", "gpt-4o", &["chat_completions"])
        .await;
    journey
        .put(
            "fallback-policy",
            json!({"rules": [{"scope": {"model": {"model": {"provider": "anthropic", "model": "claude-missing"}}},
                              "candidates": [{"provider": "openai", "model": "gpt-4o"}]}]}),
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
            &body("claude-sonnet-5", json!({"service_tier": "auto"})),
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
            &body("claude-sonnet-5", json!({"stream": true})),
        )
        .await;
    assert_eq!(stream.status().as_u16(), 200);
    assert_eq!(stream.headers()["content-type"], "text/event-stream");
    assert_eq!(
        stream.text().await.expect("complete stream"),
        ANTHROPIC_EVENTS
    );
    assert_eq!(
        journey.settled_usage().await,
        vec![tokens(5, 3), tokens(5, 3)],
        "the ledger accounts the mock's message and stream usage"
    );
    let truncated = journey
        .post(
            "/v1/messages",
            &key(&journey.caller),
            &body(
                "claude-sonnet-5",
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

    let valid = body("claude-sonnet-5", json!({}));
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
                &body("claude-missing", json!({})),
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
/// Panics when any status, body, stream, ledger, or upstream expectation fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn gemini_generate_content_ingress_is_native_governed_and_fail_closed() {
    let journey = Journey::start().await;
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
        .deploy(
            "gemini",
            "x-goog-api-key",
            "gemini-2.5-flash",
            &["chat_completions"],
        )
        .await;
    journey
        .deploy("openai", "authorization", "gpt-4o", &["chat_completions"])
        .await;
    journey
        .put(
            "fallback-policy",
            json!({"rules": [{"scope": {"model": {"model": {"provider": "gemini", "model": "gemini-missing"}}},
                              "candidates": [{"provider": "openai", "model": "gpt-4o"}]}]}),
        )
        .await;
    let key = |token: &str| vec![("x-goog-api-key", token.to_owned())];
    let body = |text: &str| json!({"contents": [{"role": "user", "parts": [{"text": text}]}], "generationConfig": {"maxOutputTokens": 16}});
    let generate = "/v1beta/models/gemini-2.5-flash:generateContent";
    let stream_route = "/v1beta/models/gemini-2.5-flash:streamGenerateContent?alt=sse";

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
        journey.settled_usage().await,
        vec![tokens(7, 2), tokens(7, 2)],
        "the ledger accounts the mock's answer and stream usage"
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
                "/v1beta/models/gemini-2.5-flash:countTokens",
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
                "/v1beta/models/gemini-2.5-flash:streamGenerateContent",
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
                "/v1beta/models/gemini-missing:generateContent",
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
