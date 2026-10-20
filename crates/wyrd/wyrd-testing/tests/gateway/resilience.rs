//! Failover, limit, and cancellation journeys against a real bound server.
//!
//! These cover the call lifecycle a successful journey never reaches: a
//! deployment that refuses retryably hands the call to its sibling, an
//! administered limit refuses the next call before any dispatch, and a caller
//! that goes away mid-call still leaves settled accounting behind. The
//! gateway's latency telemetry is proven over the same lifecycle: what the
//! server spent, what it waited on its provider, and the overhead between.
//!
//! A draining server's `503` refusal is not observable through the
//! public surface — the listener is shut down gracefully, so the connection is
//! simply refused. That refusal, and the terminal error frame an open stream
//! receives on drain, are proven in-process by
//! `wyrd_server::components::gateway::pg_invocation_tests::gateway_onboards_compatible_provider_at_runtime`,
//! which drives the same handlers over real HTTP dispatch.

use std::time::Duration;

use serde_json::{Value, json};
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, ResponseTemplate};
use wyrd_testing::bifrost::shared_process_telemetry_for_test;
use wyrd_testing::capacity::Histogram;
use wyrd_testing::release_server::Metrics;

use crate::harness::{Journey, openai_code, refusal, tokens};

/// Address the failover call requests; its fallback is `acme-second/m`.
const PROJECTION: &str = "acme-first/m";

/// Buffered Chat Completions answer the healthy deployment returns.
fn completion() -> Value {
    json!({
        "id": "chatcmpl-1",
        "object": "chat.completion",
        "created": 1,
        "model": "m",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": "hi"}, "finish_reason": "stop"}],
        "usage": {"prompt_tokens": 11, "completion_tokens": 4, "total_tokens": 15},
    })
}

/// Chat Completions stream ending with its finish reason, usage, and the
/// `[DONE]` sentinel.
const CHAT_EVENTS: &str = concat!(
    "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n\n",
    "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"m\",\"choices\":[],\"usage\":{\"prompt_tokens\":11,\"completion_tokens\":4,\"total_tokens\":15}}\n\n",
    "data: [DONE]\n\n",
);

/// How long the timed upstream holds every answer, so provider wait is
/// distinguishable from the gateway's own work.
const UPSTREAM_DELAY: Duration = Duration::from_millis(50);

/// Request-path phases every governed call of the timing journey records.
const CALL_PHASES: [&str; 10] = [
    "authenticate",
    "parse",
    "snapshot",
    "authorize",
    "route",
    "admit",
    "credential",
    "translate",
    "account",
    "capture",
];

/// One Chat Completions request body addressing `model`.
fn chat(model: &str) -> Value {
    json!({
        "model": model,
        "max_completion_tokens": 16,
        "messages": [{"role": "user", "content": "hi"}],
    })
}

/// Stores one `openai_compatible` deployment named `name` serving any model,
/// whose routes live under `prefix` of the journey's mock upstream.
///
/// The adapter needs no upstream credential, so the deployment authenticates
/// with `none` and the mock records exactly what the gateway sent.
async fn deploy_compatible(journey: &Journey, name: &str, prefix: &str) {
    journey
        .put(
            &format!("provider-deployments/{name}"),
            json!({
                "name": name,
                "adapter": {"openai_compatible": {
                    "base_url": format!("{}{prefix}/v1", journey.upstream.uri()),
                }},
                "auth": "none",
                "capabilities": ["chat_completions"],
            }),
        )
        .await;
}

/// Proves deployment failover and an administered rate limit through the
/// public server.
///
/// The requested deployment falls back to a sibling; the first provider exchange of the
/// call refuses with a retryable `503`, so the call completes on the sibling
/// deployment and the caller never sees the refusal. Both attempts are
/// accounted, and because the refused attempt reached its provider with no
/// reported usage the call total stays unknown rather than understating what
/// the refused exchange may have consumed. A tenant limit of one request per
/// minute then refuses the next call with the stable limit code and dispatches
/// nothing.
///
/// # Panics
/// Panics when a status, body, ledger entry, or dispatch-count expectation
/// fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn a_retryable_refusal_fails_over_and_a_limit_refuses_before_dispatch() {
    let journey = Journey::start().await;
    // Exactly one provider exchange refuses retryably: the requested
    // deployment's, since it is always attempted first.
    Mock::given(method("POST"))
        .and(path("/first/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(503).set_body_json(
                json!({"error": {"message": "upstream busy", "type": "server_error"}}),
            ),
        )
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&journey.upstream)
        .await;
    for prefix in ["/first", "/second"] {
        Mock::given(method("POST"))
            .and(path(format!("{prefix}/v1/chat/completions")))
            .respond_with(ResponseTemplate::new(200).set_body_json(completion()))
            .with_priority(2)
            .mount(&journey.upstream)
            .await;
    }
    deploy_compatible(&journey, "acme-first", "/first").await;
    deploy_compatible(&journey, "acme-second", "/second").await;
    journey
        .put(
            "fallback-policy",
            json!({"rules": [{"scope": {"model": {"model": {"deployment": "acme-first", "model": "m"}}},
                              "candidates": [{"deployment": "acme-second", "model": "m"}]}]}),
        )
        .await;
    // Administered before any call, so the limit counts this journey's own
    // calls: the failover call is the tenant's one admitted request.
    journey
        .put(
            "governance-policy",
            json!({
                "limits": [{"subject": "tenant", "target": "all", "requests_per_minute": 1,
                            "tokens_per_minute": null, "concurrent_calls": null}],
                "budgets": [],
                "pricing": [],
                "unknown_cost": "allow_unpriced",
            }),
        )
        .await;
    let bearer = vec![("authorization", format!("Bearer {}", journey.caller))];

    let answered = journey
        .post("/v1/chat/completions", &bearer, &chat(PROJECTION))
        .await;
    assert_eq!(
        answered.status().as_u16(),
        200,
        "a retryable refusal never reaches the caller"
    );
    assert_eq!(
        answered.json::<Value>().await.expect("answer json"),
        completion(),
        "the sibling deployment's answer is the caller's"
    );
    assert_eq!(
        journey.upstream_calls().await.len(),
        2,
        "the refused attempt and the completing one both dispatched"
    );
    assert_eq!(
        journey.settled_usage().await,
        vec![Value::Null],
        "the call is accounted once; a dispatched attempt whose usage the \
         provider never reported leaves the call total unknown rather than \
         understated"
    );
    assert_eq!(
        journey.accounted_attempts().await,
        vec![
            (json!("failed"), Value::Null),
            (json!("succeeded"), tokens(11, 4)),
        ],
        "both attempts are accounted, the completing one with its usage"
    );

    let dispatched = journey.upstream_calls().await.len();
    refusal(
        journey
            .post("/v1/chat/completions", &bearer, &chat(PROJECTION))
            .await,
        429,
        "WYRD_GATEWAY_429_LIMIT_EXCEEDED",
        openai_code,
    )
    .await;
    assert_eq!(
        journey.upstream_calls().await.len(),
        dispatched,
        "a limited call reaches no provider"
    );
}

/// Proves that a caller who goes away mid-call leaves settled accounting.
///
/// The provider holds the exchange open; once the upstream has recorded the
/// dispatch the caller's request is dropped, which cancels the call. The
/// gateway still settles it on its tracked task, so the tenant ledger carries
/// the call with no usage rather than losing it, and the server keeps serving.
///
/// # Panics
/// Panics when the dispatch is not recorded within the bound, or a ledger or
/// status expectation fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn a_cancelled_call_is_still_settled_and_the_server_keeps_serving() {
    let journey = Journey::start().await;
    Mock::given(method("POST"))
        .and(path("/held/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(completion())
                .set_delay(Duration::from_mins(1)),
        )
        .mount(&journey.upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/prompt/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(completion()))
        .mount(&journey.upstream)
        .await;
    deploy_compatible(&journey, "acme-held", "/held").await;
    deploy_compatible(&journey, "acme-prompt", "/prompt").await;
    let bearer = vec![("authorization", format!("Bearer {}", journey.caller))];

    let (http, base, token) = (
        journey.http.clone(),
        journey.base.clone(),
        journey.caller.clone(),
    );
    let abandoned = tokio::spawn(async move {
        http.post(format!("{base}/v1/chat/completions"))
            .bearer_auth(token)
            .json(&json!({
                "model": "acme-held/held",
                "max_completion_tokens": 16,
                "messages": [{"role": "user", "content": "hi"}],
            }))
            .send()
            .await
    });
    tokio::time::timeout(Duration::from_secs(30), async {
        while journey.upstream_calls().await.is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the held call dispatched");
    abandoned.abort();

    // The gateway settles a cancelled call on the task tracker, so the ledger
    // carries it once the tracker drains.
    assert_eq!(
        journey.settled_usage().await,
        vec![Value::Null],
        "the cancelled call is accounted with no usage"
    );

    // The server still serves the next caller.
    let answered = journey
        .post("/v1/chat/completions", &bearer, &chat("acme-prompt/m"))
        .await;
    assert_eq!(
        answered.status().as_u16(),
        200,
        "a cancelled call never wedges the server"
    );
    assert_eq!(
        answered.json::<Value>().await.expect("answer json"),
        completion()
    );
}

/// Proves the gateway's latency telemetry over one buffered and one streamed
/// call through the public server.
///
/// The upstream holds each answer for [`UPSTREAM_DELAY`]. The buffered call
/// exposes one `answer` point whose provider wait covers the delay and whose
/// overhead is exactly its server duration less that wait. The stream
/// exposes an `answer` point at its head, waiting only until the provider's
/// headers, and a later `end` point at its final frame, waiting until the
/// provider's last byte. Every request-path phase is recorded from the
/// request's arrival, the relay phase for the stream, and each family renders
/// the fine sub-millisecond buckets.
///
/// # Panics
/// Panics when a call fails or a metric expectation fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn gateway_capacity_metrics_cover_buffered_and_streamed_calls() {
    let journey = Journey::start().await;
    Mock::given(method("POST"))
        .and(path("/timed/v1/chat/completions"))
        .and(body_string_contains("\"stream\":true"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(CHAT_EVENTS, "text/event-stream")
                .set_delay(UPSTREAM_DELAY),
        )
        .with_priority(1)
        .mount(&journey.upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/timed/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(completion())
                .set_delay(UPSTREAM_DELAY),
        )
        .with_priority(2)
        .mount(&journey.upstream)
        .await;
    deploy_compatible(&journey, "acme-first", "/timed").await;
    let bearer = vec![("authorization", format!("Bearer {}", journey.caller))];
    let (_, telemetry) = shared_process_telemetry_for_test().expect("process telemetry");
    let before = Metrics::parse(&telemetry.render());

    let buffered = journey
        .post("/v1/chat/completions", &bearer, &chat("acme-first/m"))
        .await;
    assert_eq!(buffered.status().as_u16(), 200);
    buffered.json::<Value>().await.expect("buffered answer");
    let mut streamed = chat("acme-first/m");
    streamed["stream"] = json!(true);
    let streamed = journey
        .post("/v1/chat/completions", &bearer, &streamed)
        .await;
    assert_eq!(streamed.status().as_u16(), 200);
    assert!(
        streamed
            .text()
            .await
            .expect("complete stream")
            .ends_with("data: [DONE]\n\n")
    );
    journey.settled_usage().await;
    let after = Metrics::parse(&telemetry.render());
    let delta = |family: &str, labels: &[&str]| {
        Histogram::read(&before, family, labels).since(&Histogram::read(&after, family, labels))
    };
    let point = |stream: &'static str, point: &'static str| {
        let labels = [
            "operation=\"chat_completions\"",
            stream,
            point,
            "outcome=\"succeeded\"",
        ];
        (
            delta("wyrd_gateway_duration_seconds", &labels),
            delta("wyrd_gateway_overhead_seconds", &labels),
        )
    };
    let upstream = |stream: &str, point: &str| {
        delta(
            "wyrd_gateway_upstream_seconds",
            &["operation=\"chat_completions\"", stream, point],
        )
    };
    let delay = UPSTREAM_DELAY.as_secs_f64();

    let (duration, overhead) = point("stream=\"false\"", "point=\"answer\"");
    let waited = upstream("stream=\"false\"", "point=\"end\"");
    assert_eq!(
        (duration.count, overhead.count, waited.count),
        (1.0, 1.0, 1.0)
    );
    assert!(waited.sum >= delay, "the provider wait covers the delay");
    assert!(
        duration.sum >= waited.sum,
        "server duration covers the wait"
    );
    assert!(
        (overhead.sum - (duration.sum - waited.sum)).abs() < 1e-9,
        "overhead is server duration less provider wait"
    );

    let (head, head_overhead) = point("stream=\"true\"", "point=\"answer\"");
    let (end, end_overhead) = point("stream=\"true\"", "point=\"end\"");
    let headers = upstream("stream=\"true\"", "point=\"headers\"");
    let last_byte = upstream("stream=\"true\"", "point=\"end\"");
    assert_eq!(
        (
            head.count,
            head_overhead.count,
            end.count,
            end_overhead.count
        ),
        (1.0, 1.0, 1.0, 1.0),
        "a stream records its head and its final frame"
    );
    assert!(
        headers.sum >= delay,
        "the head waits on the provider's headers"
    );
    assert!(
        last_byte.sum >= headers.sum,
        "the end waits until the last byte"
    );
    assert!(end.sum >= head.sum, "the final frame follows the head");

    for phase in CALL_PHASES {
        let recorded = delta(
            "wyrd_gateway_phase_duration_seconds",
            &[
                "operation=\"chat_completions\"",
                &format!("phase=\"{phase}\""),
            ],
        );
        assert_eq!(recorded.count, 2.0, "both calls record the {phase} phase");
    }
    let relay = delta(
        "wyrd_gateway_phase_duration_seconds",
        &["operation=\"chat_completions\"", "phase=\"relay\""],
    );
    assert_eq!(relay.count, 1.0, "the stream records its relay phase");
    for family in [
        "wyrd_gateway_duration_seconds",
        "wyrd_gateway_overhead_seconds",
        "wyrd_gateway_upstream_seconds",
        "wyrd_gateway_phase_duration_seconds",
        "wyrd_gateway_request_duration_seconds",
        "wyrd_gateway_attempt_duration_seconds",
        "wyrd_gateway_time_to_first_byte_seconds",
        "wyrd_gateway_queue_delay_seconds",
    ] {
        assert!(
            after
                .buckets(family, &["operation=\"chat_completions\""])
                .iter()
                .any(|(bound, _)| (*bound - 0.0005).abs() < f64::EPSILON),
            "{family} renders the fine buckets"
        );
    }
}
