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

use futures_util::StreamExt as _;
use serde_json::{Value, json};
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, ResponseTemplate};
use wyrd_testing::Bootstrap;
use wyrd_testing::bifrost::shared_process_telemetry_for_test;
use wyrd_testing::capacity::Histogram;
use wyrd_testing::gateway_capture::{GatewayCapture, PacedUpstream, Pacing};
use wyrd_testing::release_server::Metrics;

use crate::harness::{Journey, api_key, openai_code, refusal, tokens};

/// Address the failover call requests; its fallback is `acme-second/m`.
const PROJECTION: &str = "acme-first/m";

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
            .respond_with(ResponseTemplate::new(200).set_body_json(GatewayCapture::completion()))
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
        GatewayCapture::completion(),
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
                .set_body_json(GatewayCapture::completion())
                .set_delay(Duration::from_mins(1)),
        )
        .mount(&journey.upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/prompt/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(GatewayCapture::completion()))
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
        GatewayCapture::completion()
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
                .set_body_raw(
                    GatewayCapture::chat_events("chatcmpl-1", 2),
                    "text/event-stream",
                )
                .set_delay(UPSTREAM_DELAY),
        )
        .with_priority(1)
        .mount(&journey.upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/timed/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(GatewayCapture::completion())
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

/// Delta of `family` restricted to `labels` from `before` to `after`.
fn delta(before: &Metrics, after: &Metrics, family: &str, labels: &[&str]) -> f64 {
    after.sum(family, labels) - before.sum(family, labels)
}

/// Waits out the call tracker and then the Scribe outbox, the post-response
/// work a capture rides on, and scrapes the process telemetry.
///
/// # Panics
/// Panics when the tracker or outbox does not settle.
async fn drained(journey: &Journey) -> Metrics {
    journey.settled_usage().await;
    journey
        .server
        .await_audit_retained()
        .await
        .expect("the Scribe outbox settles");
    let (_, telemetry) = shared_process_telemetry_for_test().expect("process telemetry");
    Metrics::parse(&telemetry.render())
}

/// Proves the capture accounting the capacity benchmark reads settles only
/// after each call's post-response work.
///
/// A warmup call is settled and scraped first, so its capture never leaks
/// into the measured interval. A delayed buffered call and a stream are then
/// answered; once the call tracker and the Scribe outbox drain, exactly their
/// two captures are staged, none is lost, nothing is pending, and the tenant
/// reads all three rows. Loss behind an HTTP 200 is judged by the benchmark's
/// verdict unit cases: server-internal Scribe writes bypass the fingerprint
/// fence, so no existing seam forces a terminal capture refusal end to end.
///
/// # Panics
/// Panics when a call, metric, or capture-row expectation fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn capacity_capture_accounting_waits_for_post_response_work() {
    let journey = Journey::start().await;
    Mock::given(method("POST"))
        .and(path("/timed/v1/chat/completions"))
        .and(body_string_contains("\"stream\":true"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(
                    GatewayCapture::chat_events("chatcmpl-1", 2),
                    "text/event-stream",
                )
                .set_delay(UPSTREAM_DELAY),
        )
        .with_priority(1)
        .mount(&journey.upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/timed/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(GatewayCapture::completion())
                .set_delay(UPSTREAM_DELAY),
        )
        .with_priority(2)
        .mount(&journey.upstream)
        .await;
    deploy_compatible(&journey, "acme-first", "/timed").await;
    journey
        .put(
            "capture-policy",
            json!({"mode": "metadata", "payload_fields": []}),
        )
        .await;
    let inspector = api_key(&journey.server, "capacity_inspector", &["admin"]).await;
    let bearer = vec![("authorization", format!("Bearer {}", journey.caller))];
    let staged = ["outcome=\"staged\""];
    let scribe = ["outbox=\"scribe\""];

    let warmup = journey
        .post("/v1/chat/completions", &bearer, &chat("acme-first/m"))
        .await;
    assert_eq!(warmup.status().as_u16(), 200);
    warmup.json::<Value>().await.expect("warmup answer");
    let baseline = drained(&journey).await;

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
    let measured = drained(&journey).await;
    assert_eq!(
        delta(&baseline, &measured, "wyrd_gateway_capture_total", &staged),
        2.0,
        "the measured interval stages its own two captures, not the warmup's"
    );
    assert_eq!(
        delta(&baseline, &measured, "outbox_events_lost_total", &[]),
        0.0,
        "nothing is lost"
    );
    assert_eq!(
        measured.sum("outbox_pending", &scribe),
        0.0,
        "the drain leaves nothing pending"
    );
    assert_eq!(
        journey.captured_calls(&inspector, 3).await.len(),
        3,
        "the tenant reads every captured call"
    );
}

/// Stream events [`gateway_capacity_paces_streams_isolates_tenants_and_recovers`]'s
/// paced upstream writes, one per write.
const PACED_CHUNKS: usize = 4;

/// The paced upstream's answer shape: short, but long enough that each
/// stream event is a separate, measurable write.
const PACED: Pacing = Pacing {
    buffered_delay: Duration::from_millis(50),
    buffered_bytes: 64,
    stream_delay: Duration::from_millis(100),
    chunks: PACED_CHUNKS,
    chunk_interval: Duration::from_millis(30),
};

/// Stores `admin`'s tenant deployment `name`, serving any model from
/// `base_url`, authenticated with `none`.
async fn deploy_at(journey: &Journey, admin: &str, name: &str, base_url: &str) {
    journey
        .put_as(
            admin,
            &format!("provider-deployments/{name}"),
            json!({
                "name": name,
                "adapter": {"openai_compatible": {"base_url": base_url}},
                "auth": "none",
                "capabilities": ["chat_completions"],
            }),
        )
        .await;
}

/// One Chat Completions request for `model` whose first message is the
/// paced upstream's call `marker`.
fn marked(model: &str, marker: &str, stream: bool) -> Value {
    let mut body = chat(model);
    body["messages"][0]["content"] = json!(marker);
    if stream {
        body["stream"] = json!(true);
    }
    body
}

/// Proves the capacity workload's real-server path: paced streams, tenant
/// capture isolation, and configured admission refusals that recover.
///
/// A paced upstream delays a stream's headers and writes each event
/// separately. The caller sees headers no earlier than the delay, reassembles
/// every event across however many transport reads carried them, and
/// receives the provider's stream unchanged, while the upstream's recorded
/// sends are at least the chunk interval apart. A second tenant limited to one
/// concurrent call holds one slow call, so every concurrent extra call is
/// refused with the configured limit code before dispatch; once it answers,
/// lower-rate calls succeed. Each tenant's administrator reads exactly its
/// own answered calls' captures, and no refused call is captured.
///
/// # Panics
/// Panics when a status, timing, stream, refusal, or capture expectation
/// fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn gateway_capacity_paces_streams_isolates_tenants_and_recovers() {
    let journey = Journey::start().await;
    let paced = PacedUpstream::start(PACED, 16)
        .await
        .expect("paced upstream binds");
    Mock::given(method("POST"))
        .and(path("/held/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(GatewayCapture::completion())
                .set_delay(Duration::from_secs(2)),
        )
        .mount(&journey.upstream)
        .await;
    let paced_url = format!("{}/paced/v1", paced.uri());
    let metadata = json!({"mode": "metadata", "payload_fields": []});
    deploy_at(&journey, &journey.admin, "acme-first", &paced_url).await;
    journey.put("capture-policy", metadata.clone()).await;
    let inspector = api_key(&journey.server, "capacity_inspector", &["admin"]).await;

    let other = journey
        .server
        .seed_tenant("gateway-capacity-other")
        .await
        .expect("second tenant seeds");
    let Bootstrap::Machine {
        api_key: other_key, ..
    } = journey
        .server
        .bootstrap_service_in_tenant(other, "capacity_other", &["admin"])
        .await
        .expect("second-tenant service bootstraps")
    else {
        panic!("service bootstrap returned a user principal");
    };
    let other_admin = journey
        .server
        .exchange_api_key(&other_key)
        .await
        .expect("second-tenant key exchanges");
    deploy_at(&journey, &other_admin, "acme-first", &paced_url).await;
    deploy_at(
        &journey,
        &other_admin,
        "acme-held",
        &format!("{}/held/v1", journey.upstream.uri()),
    )
    .await;
    journey
        .put_as(&other_admin, "capture-policy", metadata)
        .await;
    journey
        .put_as(
            &other_admin,
            "governance-policy",
            json!({
                "limits": [{"subject": "tenant", "target": "all", "requests_per_minute": null,
                            "tokens_per_minute": null, "concurrent_calls": 1}],
                "budgets": [],
                "pricing": [],
                "unknown_cost": "allow_unpriced",
            }),
        )
        .await;
    let bearer = vec![("authorization", format!("Bearer {}", journey.caller))];
    let other_bearer = vec![("authorization", format!("Bearer {other_admin}"))];

    let sent = std::time::Instant::now();
    let stream = journey
        .post(
            "/v1/chat/completions",
            &bearer,
            &marked("acme-first/m", "paced-stream", true),
        )
        .await;
    assert_eq!(stream.status().as_u16(), 200);
    assert!(
        sent.elapsed() >= PACED.stream_delay,
        "the stream's headers wait on the provider's delayed headers"
    );
    let mut body = stream.bytes_stream();
    let (mut text, mut events, mut reads) = (String::new(), 0, 0);
    while let Some(read) = body.next().await {
        reads += 1;
        text.push_str(std::str::from_utf8(&read.expect("stream read")).expect("utf-8 stream"));
        while let Some(end) = text[events..].find("\n\n").map(|at| events + at + 2) {
            assert!(text[events..end].starts_with("data: "), "{text}");
            events = end;
        }
    }
    assert_eq!(
        text,
        GatewayCapture::chat_events("paced-stream", PACED_CHUNKS),
        "the paced stream relays unchanged"
    );
    assert!(
        reads > 1,
        "separately written events reach the caller apart"
    );
    let sends = paced.take("paced-stream");
    assert_eq!(sends.len(), PACED_CHUNKS, "one recorded send per event");
    assert!(
        sends
            .windows(2)
            .all(|pair| pair[1] - pair[0] >= PACED.chunk_interval),
        "the upstream paces its writes"
    );
    let buffered = journey
        .post(
            "/v1/chat/completions",
            &bearer,
            &marked("acme-first/m", "paced-buffered", false),
        )
        .await;
    assert_eq!(buffered.status().as_u16(), 200);
    buffered.json::<Value>().await.expect("buffered answer");

    let (http, base, token) = (
        journey.http.clone(),
        journey.base.clone(),
        other_admin.clone(),
    );
    let held = tokio::spawn(async move {
        http.post(format!("{base}/v1/chat/completions"))
            .bearer_auth(token)
            .json(&chat("acme-held/m"))
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
    let bodies: Vec<Value> = (0..3)
        .map(|index| marked("acme-first/m", &format!("over-{index}"), false))
        .collect();
    let over = futures_util::future::join_all(
        bodies
            .iter()
            .map(|body| journey.post("/v1/chat/completions", &other_bearer, body)),
    )
    .await;
    for refused in over {
        refusal(refused, 429, "WYRD_GATEWAY_429_LIMIT_EXCEEDED", openai_code).await;
    }
    let held = held
        .await
        .expect("held call joins")
        .expect("held call answers");
    assert_eq!(held.status().as_u16(), 200, "the admitted call completes");
    for index in 0..2 {
        let recovered = journey
            .post(
                "/v1/chat/completions",
                &other_bearer,
                &marked("acme-first/m", &format!("recovered-{index}"), false),
            )
            .await;
        assert_eq!(
            recovered.status().as_u16(),
            200,
            "lower-rate calls succeed once the limit frees"
        );
        recovered.json::<Value>().await.expect("recovered answer");
    }

    journey.settled_usage().await;
    assert_eq!(
        journey.captured_calls(&inspector, 2).await.len(),
        2,
        "the first tenant reads only its own two calls"
    );
    assert_eq!(
        journey.captured_calls(&other_key, 3).await.len(),
        3,
        "the second tenant reads its three answered calls and no refusal"
    );
}
