//! `OpenAI`-compatible ingress journeys against a real bound server.
//!
//! The official-client matrix over every built-in provider lives in the Python
//! journeys; these cover what an SDK cannot observe from the outside: how a
//! truncated provider stream terminates on the wire, that a Scribe outage
//! which stops capture publication never changes a caller's answer, and that a
//! managed provider secret keeps working across rotation and server restart.

use serde_json::{Value, json};
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, ResponseTemplate};

use secrecy::{ExposeSecret, SecretString};
use wyrd_spec::ids::DataTenantId;
use wyrd_testing::Bootstrap;

use crate::harness::{Journey, PROVIDER_KEY, assert_provider_credentials, openai_code, tokens};

/// Chat Completions stream whose last frames carry the finish reason, usage,
/// and the `[DONE]` sentinel.
const CHAT_EVENTS: &str = concat!(
    "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"h\"}}]}\n\n",
    "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"i\"},\"finish_reason\":\"stop\"}]}\n\n",
    "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-4o\",\"choices\":[],\"usage\":{\"prompt_tokens\":11,\"completion_tokens\":4,\"total_tokens\":15}}\n\n",
    "data: [DONE]\n\n",
);

/// Chat Completions stream that stops before any choice finishes.
const CHAT_TRUNCATED: &str = "data: {\"id\":\"chatcmpl-2\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"h\"}}]}\n\n";

/// Buffered Chat Completions answer the mock returns for a non-streaming call.
fn completion() -> Value {
    json!({
        "id": "chatcmpl-0",
        "object": "chat.completion",
        "created": 1,
        "model": "gpt-4o",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": "hi"}, "finish_reason": "stop", "logprobs": null}],
        "usage": {"prompt_tokens": 11, "completion_tokens": 4, "total_tokens": 15},
    })
}

/// One Chat Completions request body, streaming when `stream` is set.
fn chat(stream: bool, marker: &str) -> Value {
    let mut body = json!({
        "model": "openai/gpt-4o",
        "max_completion_tokens": 16,
        "messages": [{"role": "user", "content": marker}],
    });
    if stream {
        let object = body.as_object_mut().expect("object");
        object.insert("stream".to_owned(), json!(true));
        object.insert("stream_options".to_owned(), json!({"include_usage": true}));
    }
    body
}

/// Proves `POST /v1/chat/completions` stream termination and capture-outage
/// isolation through the public server.
///
/// A complete provider stream relays byte-for-byte with its `[DONE]`
/// sentinel; a provider stream that stops mid-answer ends in a terminal
/// in-band error carrying the stable upstream code before the sentinel, so a
/// caller cannot mistake truncation for completion. A Scribe WAL outage that
/// refuses every capture append
/// leaves the answer, its usage accounting, and the operator credential
/// boundary unchanged, and rows published after the outage clears carry the
/// later call.
///
/// # Panics
///
/// Panics when a status, relayed body, stream terminator, ledger entry, or
/// upstream credential expectation fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn openai_compatible_streams_terminate_and_survive_a_capture_outage() {
    let journey = Journey::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_string_contains("truncate-me"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(CHAT_TRUNCATED, "text/event-stream"))
        .with_priority(1)
        .mount(&journey.upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_string_contains("\"stream\":true"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(CHAT_EVENTS, "text/event-stream"))
        .with_priority(2)
        .mount(&journey.upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(completion()))
        .with_priority(3)
        .mount(&journey.upstream)
        .await;
    journey
        .deploy("openai", "authorization", "gpt-4o", &["chat_completions"])
        .await;
    journey
        .put(
            "capture-policy",
            json!({"mode": "payload", "payload_fields": ["response"]}),
        )
        .await;
    let bearer = vec![("authorization", format!("Bearer {}", journey.caller))];

    let stream = journey
        .post("/v1/chat/completions", &bearer, &chat(true, "hi"))
        .await;
    assert_eq!(stream.status().as_u16(), 200);
    assert_eq!(stream.headers()["content-type"], "text/event-stream");
    assert_eq!(
        stream.text().await.expect("complete stream"),
        CHAT_EVENTS,
        "a complete provider stream relays unchanged, sentinel included"
    );

    let truncated = journey
        .post("/v1/chat/completions", &bearer, &chat(true, "truncate-me"))
        .await;
    assert_eq!(truncated.status().as_u16(), 200);
    let events = truncated
        .text()
        .await
        .expect("the relay ends with an in-band error");
    let frames: Vec<&str> = events.trim_end().split("\n\n").collect();
    assert_eq!(frames.last().copied(), Some("data: [DONE]"));
    let terminal: Value = serde_json::from_str(
        frames[frames.len() - 2]
            .strip_prefix("data: ")
            .expect("an SSE data frame"),
    )
    .expect("the terminal frame is JSON");
    assert_eq!(
        openai_code(&terminal),
        Some("WYRD_GATEWAY_502_UPSTREAM_UNAVAILABLE"),
        "a truncated stream ends in a terminal in-band error before the sentinel: {events}"
    );
    assert!(
        !events.contains("finish_reason\":\"stop"),
        "no relayed choice ever finished: {events}"
    );

    journey
        .server
        .trip_bifrost_wal_disk_full_for_test()
        .expect("the Scribe WAL refuses appends");
    let during_outage = journey
        .post("/v1/chat/completions", &bearer, &chat(false, "hi"))
        .await;
    assert_eq!(
        during_outage.status().as_u16(),
        200,
        "a capture outage never affects the call"
    );
    assert_eq!(
        during_outage.json::<Value>().await.expect("answer json"),
        completion(),
        "the answer relays unchanged while capture cannot publish"
    );
    journey
        .server
        .clear_bifrost_wal_disk_full_injection_for_test()
        .expect("the injected WAL refusal clears");
    let after_outage = journey
        .post("/v1/chat/completions", &bearer, &chat(false, "hi"))
        .await;
    assert_eq!(after_outage.status().as_u16(), 200);

    assert_eq!(
        journey.settled_usage().await,
        vec![tokens(11, 4), Value::Null, tokens(11, 4), tokens(11, 4)],
        "every call is accounted in order, the truncated stream with no usage \
         and the outage call with the mock's"
    );
    let dispatched = journey.upstream_calls().await;
    assert_eq!(dispatched.len(), 4);
    assert_provider_credentials(
        &dispatched,
        "authorization",
        &format!("Bearer {PROVIDER_KEY}"),
        &journey.caller,
    );
    let refused = journey
        .post(
            "/v1/chat/completions",
            &[("authorization", format!("Bearer {}", journey.reader))],
            &chat(false, "hi"),
        )
        .await;
    assert_eq!(refused.status().as_u16(), 403);
    let body: Value = refused.json().await.expect("refusal is JSON");
    assert_eq!(openai_code(&body), Some("WYRD_PERMISSION_403_DENIED_RBAC"));
    assert_eq!(
        journey.upstream_calls().await.len(),
        4,
        "the refusal reaches no provider"
    );
}

/// Provider key the administrator rotates the managed credential to.
const ROTATED_KEY: &str = "sk-rotated-upstream";

/// Proves the tenant-setup path of a managed provider secret as one user
/// journey through a real server that restarts twice.
///
/// A tenant administrator submits a managed key and configures a deployment;
/// redacted reads never reveal it. An ordinary caller invokes and the upstream
/// receives only the submitted key. A principal without gateway permission
/// cannot read or write the credential, and an administrator of another
/// tenant neither sees it nor reaches the upstream through it. The committed
/// envelope survives a restart; the administrator then rotates the key, and
/// the rotated value alone reaches the upstream before and after a second
/// restart.
///
/// # Panics
///
/// Panics when a status, redacted view, upstream credential, or isolation
/// expectation fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn managed_secret_rotation_survives_server_restart() {
    let journey = Journey::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(completion()))
        .mount(&journey.upstream)
        .await;
    journey
        .deploy("openai", "authorization", "gpt-4o", &["chat_completions"])
        .await;
    let credential = "provider-credentials/openai-key";

    let view = get(&journey, credential, &journey.admin).await;
    assert_eq!(view.status().as_u16(), 200);
    let view = view.text().await.expect("redacted view");
    assert!(view.contains("managed_secret"), "{view}");
    assert!(!view.contains(PROVIDER_KEY), "the view is redacted: {view}");

    let denied_read = get(&journey, credential, &journey.reader).await;
    assert_eq!(denied_read.status().as_u16(), 403);
    let denied_write = journey
        .http
        .put(format!("{}/v1/admin/gateway/{credential}", journey.base))
        .header("x-wyrd-access-token", format!("Bearer {}", journey.reader))
        .json(&managed("openai-key", "sk-reader-key"))
        .send()
        .await
        .expect("request sends");
    assert_eq!(denied_write.status().as_u16(), 403);

    let other_tenant = journey
        .server
        .seed_tenant("gateway-other")
        .await
        .expect("second tenant seeds");
    let outsider = outsider_token(&journey, other_tenant).await;
    let hidden = get(&journey, credential, &outsider).await;
    assert_eq!(
        hidden.status().as_u16(),
        404,
        "another tenant never sees the credential"
    );
    let foreign = journey
        .post(
            "/v1/chat/completions",
            &[("authorization", format!("Bearer {outsider}"))],
            &chat(false, "hi"),
        )
        .await;
    assert!(
        foreign.status().is_client_error(),
        "another tenant cannot invoke through this tenant's deployment: {}",
        foreign.status()
    );

    invoke(&journey).await;
    let journey = journey.restart().await;
    invoke(&journey).await;
    assert_upstream_keys(&journey, &[PROVIDER_KEY; 2]).await;

    journey
        .put(credential, managed("openai-key", ROTATED_KEY))
        .await;
    let rotated = get(&journey, credential, &journey.admin)
        .await
        .text()
        .await
        .expect("redacted view");
    assert!(
        !rotated.contains(ROTATED_KEY) && !rotated.contains(PROVIDER_KEY),
        "the rotated view stays redacted: {rotated}"
    );
    invoke(&journey).await;
    let journey = journey.restart().await;
    invoke(&journey).await;
    assert_upstream_keys(
        &journey,
        &[PROVIDER_KEY, PROVIDER_KEY, ROTATED_KEY, ROTATED_KEY],
    )
    .await;
}

/// Managed-secret credential document for `name` holding `secret`.
fn managed(name: &str, secret: &str) -> Value {
    json!({
        "name": name,
        "provider": "openai",
        "source": {"managed_secret": {"secret": secret}},
    })
}

/// Reads one administration resource at `route` as `bearer`.
///
/// # Panics
/// Panics when the request cannot be sent.
async fn get(journey: &Journey, route: &str, bearer: &str) -> reqwest::Response {
    journey
        .http
        .get(format!("{}/v1/admin/gateway/{route}", journey.base))
        .header("x-wyrd-access-token", format!("Bearer {bearer}"))
        .send()
        .await
        .expect("request sends")
}

/// Access token of a full administrator of `tenant`, a tenant other than the
/// journey's.
///
/// # Panics
/// Panics when bootstrap or exchange fails.
async fn outsider_token(journey: &Journey, tenant: DataTenantId) -> String {
    let Bootstrap::Machine { api_key, .. } = journey
        .server
        .bootstrap_service_in_tenant(tenant, "gateway_outsider", &["admin"])
        .await
        .expect("outsider bootstraps")
    else {
        panic!("service bootstrap returned a user principal");
    };
    journey
        .server
        .exchange_api_key(&SecretString::from(api_key.expose_secret().to_owned()))
        .await
        .expect("outsider exchanges")
}

/// Invokes one non-streaming chat completion as the caller and asserts the
/// relayed answer.
///
/// # Panics
/// Panics when the call is refused or the answer differs.
async fn invoke(journey: &Journey) {
    let answer = journey
        .post(
            "/v1/chat/completions",
            &[("authorization", format!("Bearer {}", journey.caller))],
            &chat(false, "hi"),
        )
        .await;
    assert_eq!(answer.status().as_u16(), 200);
    assert_eq!(answer.json::<Value>().await.expect("answer"), completion());
}

/// Asserts the upstream received exactly `keys` as bearer credentials, in
/// order, and never the caller's token.
///
/// # Panics
/// Panics when the dispatched credentials differ.
async fn assert_upstream_keys(journey: &Journey, keys: &[&str]) {
    let calls = journey.upstream_calls().await;
    let seen: Vec<String> = calls
        .iter()
        .map(|call| {
            call.headers
                .get("authorization")
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .to_owned()
        })
        .collect();
    let expected: Vec<String> = keys.iter().map(|key| format!("Bearer {key}")).collect();
    assert_eq!(seen, expected);
    for (call, key) in calls.iter().zip(keys) {
        assert_provider_credentials(
            std::slice::from_ref(call),
            "authorization",
            &format!("Bearer {key}"),
            &journey.caller,
        );
    }
}
