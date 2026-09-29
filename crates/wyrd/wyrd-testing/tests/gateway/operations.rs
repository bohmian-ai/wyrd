//! Operation-family journeys for the `OpenAI`-compatible ingress.
//!
//! Chat Completions, Embeddings, and Images are covered by the Rust and
//! Python client journeys. These drive the three families no client journey
//! reaches — Responses, Audio, and Batches — against a real bound server and
//! one local mock upstream, and prove that an adapter which carries only Chat
//! Completions refuses the others before any provider is dispatched.

use serde_json::{Value, json};
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::harness::{Journey, PROVIDER_KEY, assert_provider_credentials, openai_code, refusal};

/// Model every `OpenAI` deployment in these journeys serves.
const MODEL: &str = "gpt-4o";

/// Its `<provider>/<model>` projection, as a caller names it.
const PROJECTION: &str = "openai/gpt-4o";

/// Audio bytes the mock speech upstream returns.
const SPEECH: &[u8] = b"ID3\x04\x00wyrd-gateway-journey-speech";

/// Provider batch id the mock upstream issues.
const UPSTREAM_BATCH: &str = "b-up-1";

/// Provider file id the mock upstream issues for an uploaded input file.
const UPSTREAM_FILE: &str = "file-up-1";

/// Responses stream whose last event completes the response with its usage.
const RESPONSE_EVENTS: &str = concat!(
    "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"item_id\":\"msg_1\",\"output_index\":0,\"content_index\":0,\"delta\":\"hi\"}\n\n",
    "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"object\":\"response\",\"status\":\"completed\",\"model\":\"gpt-4o\",\"usage\":{\"input_tokens\":9,\"output_tokens\":5,\"total_tokens\":14}}}\n\n",
);

/// Buffered Responses answer the mock returns for a non-streaming call.
fn response() -> Value {
    json!({
        "id": "resp_0",
        "object": "response",
        "created_at": 1,
        "status": "completed",
        "model": "gpt-4o",
        "output": [{"type": "message", "id": "msg_0", "status": "completed", "role": "assistant",
                    "content": [{"type": "output_text", "text": "hi", "annotations": []}]}],
        "usage": {"input_tokens": 9, "output_tokens": 5, "total_tokens": 14},
    })
}

/// Provider batch object in `status`.
fn upstream_batch(status: &str) -> Value {
    json!({
        "id": UPSTREAM_BATCH,
        "object": "batch",
        "status": status,
        "endpoint": "/v1/chat/completions",
        "input_file_id": UPSTREAM_FILE,
        "output_file_id": null,
        "error_file_id": null,
    })
}

/// One batch input line with `id` targeting `url` for `model`.
fn batch_line(id: &str, url: &str, model: &str) -> String {
    json!({"custom_id": id, "method": "POST", "url": url,
           "body": {"model": model, "messages": [{"role": "user", "content": "hi"}]}})
    .to_string()
}

/// A `multipart/form-data` body of `fields` and one file `part`.
///
/// Each part is a `(name, value)` text field; `part` is a
/// `(name, filename, content type, bytes)` file.
fn multipart(fields: &[(&str, &str)], part: (&str, &str, &str, &[u8])) -> Vec<u8> {
    let (name, filename, content_type, bytes) = part;
    let mut body = Vec::new();
    for (field, value) in fields {
        body.extend_from_slice(
            format!(
                "--wyrd\r\nContent-Disposition: form-data; name=\"{field}\"\r\n\r\n{value}\r\n"
            )
            .as_bytes(),
        );
    }
    body.extend_from_slice(
        format!(
            "--wyrd\r\nContent-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\nContent-Type: {content_type}\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(bytes);
    body.extend_from_slice(b"\r\n--wyrd--\r\n");
    body
}

/// Mounts every mock upstream route the three families reach.
async fn mount_upstream(journey: &Journey) {
    let upstream = &journey.upstream;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .and(body_string_contains("\"stream\":true"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(RESPONSE_EVENTS, "text/event-stream"))
        .with_priority(1)
        .mount(upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response()))
        .with_priority(2)
        .mount(upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/audio/speech"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(SPEECH, "audio/mpeg"))
        .mount(upstream)
        .await;
    for route in ["/v1/audio/transcriptions", "/v1/audio/translations"] {
        Mock::given(method("POST"))
            .and(path(route))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"text": "hi"})))
            .mount(upstream)
            .await;
    }
    Mock::given(method("POST"))
        .and(path("/v1/files"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"id": UPSTREAM_FILE, "object": "file", "purpose": "batch", "bytes": 1}),
        ))
        .mount(upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/batches"))
        .respond_with(ResponseTemplate::new(200).set_body_json(upstream_batch("validating")))
        .mount(upstream)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/batches/{UPSTREAM_BATCH}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(upstream_batch("in_progress")))
        .mount(upstream)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/v1/batches/{UPSTREAM_BATCH}/cancel")))
        .respond_with(ResponseTemplate::new(200).set_body_json(upstream_batch("cancelling")))
        .mount(upstream)
        .await;
}

/// Proves `POST /v1/responses`, the three Audio routes, and the Batches
/// submission lifecycle through the public server, and that a chat-only
/// adapter carries none of them.
///
/// Responses answers buffered and relays its native event stream to the
/// terminal `response.completed`; speech returns the provider's audio bytes
/// and content type while the two transcription forms stream a multipart
/// upload, and a form past the Audio upload limit is refused `413` before any
/// dispatch; a batch is uploaded, created, read, listed, and cancelled under
/// Wyrd ids that are never the provider's, with an unsupported batch endpoint
/// refused before the file is uploaded. Every dispatch carries the operator
/// key and never the caller token, and an Anthropic deployment can neither
/// declare nor serve a non-chat operation.
///
/// # Panics
/// Panics when a status, body, relayed stream, upstream route, credential, or
/// refusal expectation fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn responses_audio_and_batches_dispatch_through_the_public_server() {
    let journey = Journey::start().await;
    mount_upstream(&journey).await;
    journey
        .deploy(
            "openai",
            "authorization",
            MODEL,
            &["chat_completions", "responses", "audio", "batches"],
        )
        .await;
    let bearer = vec![("authorization", format!("Bearer {}", journey.caller))];

    // Responses: buffered, then the native stream to its terminal event.
    let buffered = journey
        .post(
            "/v1/responses",
            &bearer,
            &json!({"model": PROJECTION, "input": "hi", "max_output_tokens": 16}),
        )
        .await;
    assert_eq!(buffered.status().as_u16(), 200);
    assert_eq!(
        buffered.json::<Value>().await.expect("response json"),
        response(),
        "the Responses answer relays unchanged"
    );
    let streamed = journey
        .post(
            "/v1/responses",
            &bearer,
            &json!({"model": PROJECTION, "input": "hi", "max_output_tokens": 16, "stream": true}),
        )
        .await;
    assert_eq!(streamed.status().as_u16(), 200);
    assert_eq!(streamed.headers()["content-type"], "text/event-stream");
    assert_eq!(
        streamed.text().await.expect("complete stream"),
        RESPONSE_EVENTS,
        "a Responses stream relays its native events, terminator included"
    );

    // Audio: speech returns bytes, both upload forms return a transcript.
    let speech = journey
        .post(
            "/v1/audio/speech",
            &bearer,
            &json!({"model": PROJECTION, "input": "hi", "voice": "alloy"}),
        )
        .await;
    assert_eq!(speech.status().as_u16(), 200);
    assert_eq!(speech.headers()["content-type"], "audio/mpeg");
    assert_eq!(
        speech.bytes().await.expect("audio bytes").as_ref(),
        SPEECH,
        "speech relays the provider's bytes unchanged"
    );
    for route in ["/v1/audio/transcriptions", "/v1/audio/translations"] {
        let form = multipart(
            &[("model", PROJECTION)],
            ("file", "clip.mp3", "audio/mpeg", SPEECH),
        );
        let transcribed = journey
            .http
            .post(format!("{}{route}", journey.base))
            .bearer_auth(&journey.caller)
            .header("content-type", "multipart/form-data; boundary=wyrd")
            .body(form)
            .send()
            .await
            .expect("upload sends");
        assert_eq!(
            transcribed.status().as_u16(),
            200,
            "{route}: {}",
            transcribed.text().await.unwrap_or_default()
        );
        assert_eq!(
            transcribed.json::<Value>().await.expect("transcript json"),
            json!({"text": "hi"}),
            "{route} relays the provider's transcript"
        );
    }

    // An upload past the Audio form limit is refused before any dispatch.
    let dispatched = journey.upstream_calls().await.len();
    let oversized = journey
        .http
        .post(format!("{}/v1/audio/transcriptions", journey.base))
        .bearer_auth(&journey.caller)
        .header("content-type", "multipart/form-data; boundary=wyrd")
        .body(multipart(
            &[("model", PROJECTION)],
            (
                "file",
                "big.wav",
                "audio/wav",
                &vec![0_u8; 25 * 1024 * 1024 + 1],
            ),
        ))
        .send()
        .await
        .expect("upload sends");
    assert_eq!(
        oversized.status().as_u16(),
        413,
        "an oversized form is refused"
    );
    assert_eq!(
        journey.upstream_calls().await.len(),
        dispatched,
        "an oversized form reaches no provider"
    );

    // Batches: an unsupported endpoint never uploads; the supported one runs
    // the whole lifecycle under Wyrd ids.
    let upload = |jsonl: String| {
        let form = multipart(
            &[("purpose", "batch")],
            ("file", "in.jsonl", "application/jsonl", jsonl.as_bytes()),
        );
        journey
            .http
            .post(format!("{}/v1/files", journey.base))
            .bearer_auth(&journey.caller)
            .header("content-type", "multipart/form-data; boundary=wyrd")
            .body(form)
            .send()
    };
    let uploads = || async {
        journey
            .upstream_calls()
            .await
            .iter()
            .filter(|call| call.url.path() == "/v1/files")
            .count()
    };
    let unsupported = refusal(
        upload(batch_line("a", "/v1/images/generations", PROJECTION))
            .await
            .expect("upload sends"),
        400,
        "WYRD_GATEWAY_400_INVALID_REQUEST",
        openai_code,
    )
    .await;
    assert_eq!(
        unsupported["error"]["param"], "file[0].url",
        "an unsupported batch endpoint names its line: {unsupported}"
    );
    assert_eq!(uploads().await, 0, "an invalid input file never uploads");

    let jsonl = format!(
        "{}\n{}\n",
        batch_line("a", "/v1/chat/completions", PROJECTION),
        batch_line("b", "/v1/chat/completions", PROJECTION)
    );
    let uploaded = upload(jsonl.clone()).await.expect("upload sends");
    assert_eq!(uploaded.status().as_u16(), 200);
    let file: Value = uploaded.json().await.expect("file json");
    let file_id = file["id"].as_str().expect("file id").to_owned();
    assert!(
        file_id.starts_with("file-") && file_id != UPSTREAM_FILE,
        "the caller gets a Wyrd file id, never the provider's: {file}"
    );
    assert_eq!(file["bytes"], jsonl.len());
    let sent = journey
        .upstream_calls()
        .await
        .into_iter()
        .find(|call| call.url.path() == "/v1/files")
        .expect("the upload dispatched");
    let sent = String::from_utf8_lossy(&sent.body).into_owned();
    assert!(
        sent.contains("\"model\":\"gpt-4o\"") && !sent.contains(PROJECTION),
        "every line reaches the provider with its native model: {sent}"
    );

    let created = journey
        .post(
            "/v1/batches",
            &bearer,
            &json!({"input_file_id": file_id, "endpoint": "/v1/chat/completions",
                    "completion_window": "24h"}),
        )
        .await;
    assert_eq!(created.status().as_u16(), 200);
    let created: Value = created.json().await.expect("batch json");
    let batch_id = created["id"].as_str().expect("batch id").to_owned();
    assert!(
        batch_id.starts_with("batch_") && batch_id != UPSTREAM_BATCH,
        "the caller gets a Wyrd batch id, never the provider's: {created}"
    );
    assert_eq!(created["input_file_id"], file_id.as_str());
    assert_eq!(created["status"], "validating");

    let read = journey
        .http
        .get(format!("{}/v1/batches/{batch_id}", journey.base))
        .bearer_auth(&journey.caller)
        .send()
        .await
        .expect("batch read sends");
    assert_eq!(read.status().as_u16(), 200);
    let read: Value = read.json().await.expect("batch json");
    assert_eq!(
        (read["id"].as_str(), read["status"].as_str()),
        (Some(batch_id.as_str()), Some("in_progress")),
        "a read refreshes the batch from its provider: {read}"
    );

    let listed = journey
        .http
        .get(format!("{}/v1/batches?limit=10", journey.base))
        .bearer_auth(&journey.caller)
        .send()
        .await
        .expect("batch list sends");
    assert_eq!(listed.status().as_u16(), 200);
    let listed: Value = listed.json().await.expect("batch list json");
    assert_eq!(
        listed["data"]
            .as_array()
            .expect("a batch list")
            .iter()
            .filter_map(|batch| batch["id"].as_str())
            .collect::<Vec<_>>(),
        [batch_id.as_str()],
        "the tenant's batch is listed: {listed}"
    );

    let cancelled = journey
        .post(
            &format!("/v1/batches/{batch_id}/cancel"),
            &bearer,
            &json!({}),
        )
        .await;
    assert_eq!(cancelled.status().as_u16(), 200);
    let cancelled: Value = cancelled.json().await.expect("batch json");
    assert_eq!(
        (cancelled["id"].as_str(), cancelled["status"].as_str()),
        (Some(batch_id.as_str()), Some("cancelling"))
    );

    let dispatched = journey.upstream_calls().await;
    assert_eq!(
        dispatched.len(),
        9,
        "two Responses calls, three Audio calls, and the upload, creation, read, \
         and cancellation dispatched; the listing is a stored read"
    );
    assert_provider_credentials(
        &dispatched,
        "authorization",
        &format!("Bearer {PROVIDER_KEY}"),
        &journey.caller,
    );

    // A chat-only adapter carries no other operation, at administration and
    // at dispatch.
    journey
        .deploy(
            "anthropic",
            "x-api-key",
            "claude-sonnet-5",
            &["chat_completions"],
        )
        .await;
    let declared = journey
        .try_put(
            "provider-deployments/anthropic-embeddings",
            json!({
                "name": "anthropic-embeddings",
                "model": {"provider": "anthropic", "model": "claude-sonnet-5"},
                "adapter": "anthropic",
                "auth": {"api_key_header": {"header": "x-api-key", "credential": "anthropic-key"}},
                "capabilities": ["embeddings"],
                "routing_weight": 1,
            }),
        )
        .await;
    assert_eq!(declared.status().as_u16(), 400);
    let declared: Value = declared.json().await.expect("refusal is JSON");
    assert_eq!(
        declared.get("code").and_then(Value::as_str),
        Some("WYRD_GATEWAY_400_INVALID_CONFIGURATION"),
        "an Anthropic deployment cannot declare Embeddings: {declared}"
    );
    for (route, body) in [
        (
            "/v1/embeddings",
            json!({"model": "anthropic/claude-sonnet-5", "input": "hi"}),
        ),
        (
            "/v1/responses",
            json!({"model": "anthropic/claude-sonnet-5", "input": "hi", "max_output_tokens": 16}),
        ),
        (
            "/v1/audio/speech",
            json!({"model": "anthropic/claude-sonnet-5", "input": "hi", "voice": "alloy"}),
        ),
    ] {
        refusal(
            journey.post(route, &bearer, &body).await,
            404,
            "WYRD_GATEWAY_404_MODEL_UNAVAILABLE",
            openai_code,
        )
        .await;
    }
    assert_eq!(
        journey.upstream_calls().await.len(),
        dispatched.len(),
        "no refused operation reaches a provider"
    );
}
