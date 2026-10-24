//! `OpenAI`-compatible ingress journeys against a real bound server.
//!
//! The official-client matrix over every built-in provider lives in the Python
//! journeys; these cover what an SDK cannot observe from the outside: how a
//! truncated provider stream terminates on the wire, that a Scribe outage
//! which stops capture publication never changes a caller's answer, and that a
//! managed provider secret keeps working across rotation and server restart,
//! and that every call's request context is admitted before dispatch and lands
//! on its captured row with the registry's readable identity.

use serde_json::{Value, json};
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, ResponseTemplate};

use secrecy::{ExposeSecret, SecretString};
use wyrd_spec::ids::DataTenantId;
use wyrd_testing::Bootstrap;
use wyrd_testing::gateway_capture::GatewayCapture;

use crate::harness::{
    ANTHROPIC_EVENTS, INVOKER_ROLE, Journey, PROVIDER_KEY, anthropic_code, api_key,
    assert_provider_credentials, exchange, openai_code, refusal, tokens,
};

/// Chat Completions stream that stops before any choice finishes.
const CHAT_TRUNCATED: &str = "data: {\"id\":\"chatcmpl-2\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"h\"}}]}\n\n";

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
/// refuses every capture append leaves the answer and the operator
/// credential boundary unchanged, and once the outage clears the outbox's
/// retry publishes the outage call's row, with its usage, before the later
/// call's.
///
/// # Panics
///
/// Panics when a status, relayed body, stream terminator, captured row, or
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
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            GatewayCapture::chat_events("chatcmpl-1", 2),
            "text/event-stream",
        ))
        .with_priority(2)
        .mount(&journey.upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(GatewayCapture::completion()))
        .with_priority(3)
        .mount(&journey.upstream)
        .await;
    journey
        .deploy("openai", "authorization", &["chat_completions"])
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
        GatewayCapture::chat_events("chatcmpl-1", 2),
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
        GatewayCapture::completion(),
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
        journey.settled_usage(4).await,
        vec![tokens(11, 4), Value::Null, tokens(11, 4), tokens(11, 4)],
        "every call is captured in order, the truncated stream with no usage \
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

/// Proves gateway request context becomes one canonical captured row per
/// call, admitted before any provider dispatch, on both credential modes.
///
/// A configured Service with one Agent component calls `OpenAI` with its
/// signed workload token in the provider credential slot and Anthropic, as a
/// stream, with an unsigned token in `x-wyrd-access-token` and an explicit
/// `x-wyrd-workload-uid`; Batch HTTP upload and creation carry the same
/// headers. Every row names the Service as workload by UID, name, and version
/// from the registry, never a forged label header; its subject by UID, kind,
/// name, and version; its Run; and the environment its own request sent, so a
/// later call without one records none. An unknown or out-of-graph subject, a
/// repeated or malformed context header, and a workload other than the
/// signed one are refused with their stable codes before any provider call,
/// and no Wyrd credential or context reaches the provider.
///
/// # Panics
///
/// Panics when a status, refusal, captured row, or upstream expectation
/// fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn configured_gateway_headers_are_authorized_and_captured() {
    let journey = Journey::start().await;
    let message = json!({"id": "msg_1", "type": "message", "role": "assistant", "model": "claude-sonnet-5", "content": [{"type": "text", "text": "hi"}], "stop_reason": "end_turn", "stop_sequence": null, "usage": {"input_tokens": 5, "output_tokens": 3}});
    for (route, answer) in [
        ("/v1/chat/completions", GatewayCapture::completion()),
        ("/v1/messages", message),
        (
            "/v1/files",
            json!({"id": "file-up-1", "object": "file", "purpose": "batch", "bytes": 1}),
        ),
        (
            "/v1/batches",
            json!({"id": "b-up-1", "object": "batch", "status": "validating",
                               "endpoint": "/v1/chat/completions", "input_file_id": "file-up-1"}),
        ),
    ] {
        Mock::given(method("POST"))
            .and(path(route))
            .respond_with(ResponseTemplate::new(200).set_body_json(answer))
            .mount(&journey.upstream)
            .await;
    }
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(body_string_contains("\"stream\":true"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(ANTHROPIC_EVENTS, "text/event-stream"),
        )
        .with_priority(1)
        .mount(&journey.upstream)
        .await;
    journey
        .deploy("openai", "authorization", &["chat_completions", "batches"])
        .await;
    journey
        .deploy("anthropic", "x-api-key", &["chat_completions"])
        .await;
    journey.capture_metadata().await;

    let inspector = api_key(&journey.server, "context_inspector", &["admin"]).await;
    let admin = wyrd_client::bifrost::client_from_options(
        journey.server.base_url(),
        Some(inspector.expose_secret()),
        journey.server.grpc_url().as_deref(),
    )
    .expect("admin client builds");
    let cards = wyrd_client::cards::Cards::connect_with(&admin)
        .await
        .expect("Cards connects");
    let fixtures = std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../fixtures/cards/observe_a_run"
    ));
    let configured = Box::pin(cards.declare_from_path(fixtures.join("configured-service.yaml")))
        .await
        .expect("the configured Service registers");
    let outside = Box::pin(cards.declare_from_path(fixtures.join("observed-model.yaml")))
        .await
        .expect("a Card outside the Service's graph registers")
        .root
        .uid
        .expect("the Model has a UID")
        .to_string();
    let service = configured.root.uid.clone().expect("the Service has a UID");
    let agent = configured
        .outcomes
        .iter()
        .find(|outcome| outcome.card_ref.name.as_str() == "observed-agent")
        .and_then(|outcome| outcome.card_ref.uid.clone())
        .expect("the Agent component registers")
        .to_string();
    let runtime = journey
        .server
        .credential_registered_service(&configured.root, &[INVOKER_ROLE])
        .await
        .expect("the Service's runtime account is credentialed");
    let runtime_key = runtime.api_key().expect("the runtime has an API key");
    let signed = journey
        .server
        .exchange_api_key_for_workload(runtime_key, &service)
        .await
        .expect("a signed workload token exchanges");
    let unsigned = exchange(&journey.server, runtime_key).await;
    let service = service.to_string();

    let openai = |context: &[(&'static str, &str)]| {
        let mut headers = vec![("authorization", format!("Bearer {signed}"))];
        headers.extend(
            context
                .iter()
                .map(|(name, value)| (*name, (*value).to_owned())),
        );
        headers
    };
    let anthropic = |context: &[(&'static str, &str)]| {
        let mut headers = vec![
            ("x-wyrd-access-token", format!("Bearer {unsigned}")),
            ("x-wyrd-workload-uid", service.clone()),
            ("anthropic-version", "2023-06-01".to_owned()),
        ];
        headers.extend(
            context
                .iter()
                .map(|(name, value)| (*name, (*value).to_owned())),
        );
        headers
    };
    let messages = |stream: bool| {
        json!({"model": "anthropic/claude-sonnet-5", "max_tokens": 16, "stream": stream,
               "messages": [{"role": "user", "content": "hi"}]})
    };

    let answer = journey
        .post(
            "/v1/chat/completions",
            &openai(&[
                ("wyrd-card-uid", &service),
                ("x-wyrd-deployment-environment", "prod"),
                ("x-wyrd-service-name", "forged"),
            ]),
            &chat(false, "hi"),
        )
        .await;
    assert_eq!(answer.status().as_u16(), 200);
    let stream = journey
        .post(
            "/v1/messages",
            &anthropic(&[
                ("wyrd-card-uid", &agent),
                ("wyrd-run-id", "agent-run"),
                ("x-wyrd-deployment-environment", "staging"),
            ]),
            &messages(true),
        )
        .await;
    assert_eq!(stream.status().as_u16(), 200);
    assert_eq!(
        stream.text().await.expect("stream relays"),
        ANTHROPIC_EVENTS
    );
    let unlabelled = journey
        .post(
            "/v1/chat/completions",
            &openai(&[("wyrd-card-uid", &agent)]),
            &chat(false, "hi"),
        )
        .await;
    assert_eq!(unlabelled.status().as_u16(), 200);
    let batch_context = [
        ("wyrd-card-uid", agent.as_str()),
        ("wyrd-run-id", "batch-run"),
        ("x-wyrd-deployment-environment", "prod"),
    ];
    let line = json!({"custom_id": "a", "method": "POST", "url": "/v1/chat/completions",
                      "body": {"model": "openai/gpt-4o", "messages": [{"role": "user", "content": "hi"}]}});
    let form = format!(
        "--wyrd\r\nContent-Disposition: form-data; name=\"purpose\"\r\n\r\nbatch\r\n\
         --wyrd\r\nContent-Disposition: form-data; name=\"file\"; filename=\"in.jsonl\"\r\n\
         Content-Type: application/jsonl\r\n\r\n{line}\n\r\n--wyrd--\r\n"
    );
    let upload = |context: &[(&'static str, &str)]| {
        let mut request = journey
            .http
            .post(format!("{}/v1/files", journey.base))
            .header("content-type", "multipart/form-data; boundary=wyrd")
            .body(form.clone());
        for (name, value) in openai(context) {
            request = request.header(name, value);
        }
        request.send()
    };
    let file: Value = upload(&batch_context)
        .await
        .expect("upload sends")
        .json()
        .await
        .expect("file json");
    let created = journey
        .post(
            "/v1/batches",
            &openai(&batch_context),
            &json!({"input_file_id": file["id"], "endpoint": "/v1/chat/completions",
                    "completion_window": "24h"}),
        )
        .await;
    assert_eq!(created.status().as_u16(), 200, "{file}");

    let dispatched = journey.upstream_calls().await;
    assert_eq!(dispatched.len(), 5);
    for call in &dispatched {
        for (name, value) in &call.headers {
            let value = value.to_str().unwrap_or_default();
            assert!(
                !value.contains(&signed) && !value.contains(&unsigned),
                "caller token forwarded in {name}"
            );
            assert!(
                !name.as_str().starts_with("x-wyrd") && !name.as_str().starts_with("wyrd-"),
                "Wyrd context forwarded: {name}"
            );
        }
    }

    let unknown = uuid::Uuid::now_v7().to_string();
    let invalid = "WYRD_GATEWAY_400_INVALID_REQUEST";
    let scope = "WYRD_VALA_403_BIFROST_CARD_SCOPE";
    for (headers, status, code) in [
        (openai(&[("wyrd-card-uid", &unknown)]), 403, scope),
        (openai(&[("wyrd-card-uid", &outside)]), 403, scope),
        (
            openai(&[("wyrd-card-uid", &agent), ("wyrd-card-uid", &service)]),
            400,
            invalid,
        ),
        (openai(&[("wyrd-card-uid", "not-a-uid")]), 400, invalid),
        (openai(&[("wyrd-run-id", "")]), 400, invalid),
        (
            openai(&[
                ("x-wyrd-deployment-environment", "prod"),
                ("x-wyrd-deployment-environment", "dev"),
            ]),
            400,
            invalid,
        ),
        (
            openai(&[("x-wyrd-workload-uid", &outside)]),
            400,
            "WYRD_AUTH_400_SIGNED_WORKLOAD_MISMATCH",
        ),
    ] {
        refusal(
            journey
                .post("/v1/chat/completions", &headers, &chat(false, "hi"))
                .await,
            status,
            code,
            openai_code,
        )
        .await;
    }
    refusal(
        journey
            .post(
                "/v1/messages",
                &anthropic(&[("wyrd-card-uid", &outside)]),
                &messages(false),
            )
            .await,
        403,
        scope,
        anthropic_code,
    )
    .await;
    refusal(
        upload(&[("wyrd-card-uid", &agent), ("wyrd-card-uid", &service)])
            .await
            .expect("upload sends"),
        400,
        invalid,
        openai_code,
    )
    .await;
    assert_eq!(
        journey.upstream_calls().await.len(),
        5,
        "every refusal precedes dispatch"
    );

    journey.drain_calls().await;
    let rows = journey
        .captured_json(
            &inspector,
            "SELECT ingress_dialect, CAST(workload_card_uid AS VARCHAR) AS workload, \
             service_name, service_version, deployment_environment, subject_kind, subject_name, \
             subject_version, CAST(subject_card_uid AS VARCHAR) AS subject, run_id \
             FROM bifrost.gateway.calls ORDER BY started_at",
            5,
        )
        .await;
    let row = |dialect: &str, environment: Value, subject: (&str, &str, &str), run: Value| {
        json!({
            "ingress_dialect": dialect, "workload": service, "service_name": "configured-service",
            "service_version": "1.0.0", "deployment_environment": environment,
            "subject_kind": subject.0, "subject_name": subject.1, "subject_version": "1.0.0",
            "subject": subject.2, "run_id": run,
        })
    };
    let as_service = ("Service", "configured-service", service.as_str());
    let as_agent = ("Agent", "observed-agent", agent.as_str());
    let canonical = |mut row: Value| {
        row.as_object_mut()
            .expect("row object")
            .retain(|_, value| !value.is_null());
        row
    };
    assert_eq!(
        rows,
        [
            row(
                "openai_chat_completions",
                json!("prod"),
                as_service,
                Value::Null
            ),
            row(
                "anthropic_messages",
                json!("staging"),
                as_agent,
                json!("agent-run")
            ),
            row(
                "openai_chat_completions",
                Value::Null,
                as_agent,
                Value::Null
            ),
            row(
                "openai_batches",
                json!("prod"),
                as_agent,
                json!("batch-run")
            ),
            row(
                "openai_batches",
                json!("prod"),
                as_agent,
                json!("batch-run")
            ),
        ]
        .map(canonical),
        "each call keeps its own admitted context with registry-derived names"
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
        .respond_with(ResponseTemplate::new(200).set_body_json(GatewayCapture::completion()))
        .mount(&journey.upstream)
        .await;
    journey
        .deploy("openai", "authorization", &["chat_completions"])
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
    assert_eq!(
        answer.json::<Value>().await.expect("answer"),
        GatewayCapture::completion()
    );
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
