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

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use futures_util::StreamExt as _;
use secrecy::SecretString;
use serde_json::{Value, json};
use vala_bifrost_redux::catalog::{TableRef, TenantTableBinding};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, ResponseTemplate};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PLATFORM_AUDIT_PRINCIPAL;
use wyrd_testing::Bootstrap;
use wyrd_testing::bifrost::shared_process_telemetry_for_test;
use wyrd_testing::capacity::Histogram;
use wyrd_testing::gateway_capture::{GatewayCapture, PacedUpstream, Pacing};
use wyrd_testing::release_server::Metrics;

use crate::harness::{INVOKER_ROLE, Journey, api_key, openai_code, refusal};

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
/// captured on the call row, and because the refused attempt reached its
/// provider with no reported usage the call total stays unknown rather than
/// understating what the refused exchange may have consumed. A tenant limit of one request per
/// minute then refuses the next call with the stable limit code and dispatches
/// nothing.
///
/// # Panics
/// Panics when a status, body, captured row, or dispatch-count expectation
/// fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn a_retryable_refusal_fails_over_and_a_limit_refuses_before_dispatch() {
    let journey = Journey::start().await;
    journey.capture_metadata().await;
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
        journey.settled_calls(1).await,
        vec![(2, Value::Null)],
        "the call is captured once with both attempts; a dispatched attempt \
         whose usage the provider never reported leaves the call total \
         unknown rather than understated"
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
/// gateway still settles it on its tracked task, so the tenant's captured
/// row carries the call with no usage rather than losing it, and the server
/// keeps serving.
///
/// # Panics
/// Panics when the dispatch is not recorded within the bound, or a captured
/// row or status expectation fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn a_cancelled_call_is_still_settled_and_the_server_keeps_serving() {
    let journey = Journey::start().await;
    journey.capture_metadata().await;
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

    // The gateway settles a cancelled call on the task tracker, so its
    // capture is staged once the tracker drains.
    assert_eq!(
        journey.settled_usage(1).await,
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
    journey.drain_calls().await;
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
    journey.drain_calls().await;
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

/// Calls each per-client key of
/// [`gateway_capacity_paces_streams_isolates_tenants_and_recovers`] makes in
/// its concurrent two-tenant phase.
const CALLS_PER_CLIENT: usize = 3;

/// One tenant's view of the concurrent phase: the client that reads its
/// rows, and the verified principal each answered call's request ran under.
struct TenantCalls {
    /// Owning tenant.
    tenant: DataTenantId,
    /// Administrator key whose real client reads the tenant's captures.
    inspector: SecretString,
    /// Principal of every answered concurrent call, by its `wyrd-request-id`.
    callers: BTreeMap<String, String>,
}

/// Bootstraps `count` caller services named `{prefix}-{index}` in `tenant`
/// holding `roles`, and exchanges each one's own API key for a token.
///
/// # Panics
/// Panics when bootstrap or exchange fails.
async fn per_client_tokens(
    journey: &Journey,
    tenant: DataTenantId,
    prefix: &str,
    roles: &[&str],
    count: usize,
) -> Vec<(String, String)> {
    let mut tokens = Vec::new();
    for index in 0..count {
        let caller = journey
            .server
            .bootstrap_service_in_tenant(tenant, &format!("{prefix}_{index}"), roles)
            .await
            .expect("caller bootstraps");
        let token = journey
            .server
            .exchange_api_key(caller.api_key().expect("a service has a key"))
            .await
            .expect("caller key exchanges");
        tokens.push((caller.id().to_string(), token));
    }
    tokens
}

/// Sends [`CALLS_PER_CLIENT`] buffered calls as each `(principal, token)`
/// client, all concurrently, and returns the principal of each answered
/// call by the `wyrd-request-id` the server stamped on it.
///
/// # Panics
/// Panics when a call is not answered or carries no request ID.
async fn concurrent_calls(
    journey: &Journey,
    clients: &[(String, String)],
    label: &str,
) -> BTreeMap<String, String> {
    let calls = clients
        .iter()
        .enumerate()
        .flat_map(|(client, (id, token))| {
            (0..CALLS_PER_CLIENT).map(move |call| async move {
                let response = journey
                    .post(
                        "/v1/chat/completions",
                        &[("authorization", format!("Bearer {token}"))],
                        &marked("acme-first/m", &format!("{label}-{client}-{call}"), false),
                    )
                    .await;
                assert_eq!(response.status().as_u16(), 200, "concurrent call answers");
                let request = response.headers()["wyrd-request-id"]
                    .to_str()
                    .expect("request ID is text")
                    .to_owned();
                response.json::<Value>().await.expect("concurrent answer");
                (request, id.clone())
            })
        });
    futures_util::future::join_all(calls)
        .await
        .into_iter()
        .collect()
}

/// Asserts `own`'s real client reads exactly its answered concurrent calls,
/// each with the subject and actor of the key that made it and never the
/// no-caller marker, and their attempt spans under the same pair, while
/// none of `other`'s call or trace identities appear. Returns `own`'s call
/// IDs.
///
/// # Panics
/// Panics when a row is missing, duplicated, misattributed, or foreign.
async fn assert_tenant_calls(
    journey: &Journey,
    own: &TenantCalls,
    other: &TenantCalls,
    retained: usize,
) -> BTreeSet<String> {
    let marker = PLATFORM_AUDIT_PRINCIPAL.to_string();
    let rows = journey
        .captured_json(
            &own.inspector,
            "SELECT call_id, request_id, sub_principal_id, act_principal_id \
             FROM vala.gateway.calls",
            retained,
        )
        .await;
    assert_eq!(
        rows.len(),
        retained,
        "no omitted or duplicate captured call"
    );
    let mut calls = BTreeMap::new();
    for row in &rows {
        let (sub, act) = (
            row["sub_principal_id"].as_str(),
            row["act_principal_id"].as_str(),
        );
        assert!(
            sub == act && sub.is_some() && sub != Some(marker.as_str()),
            "{row}"
        );
        let request = row["request_id"].as_str().expect("request ID");
        assert!(!other.callers.contains_key(request), "foreign call {row}");
        if let Some(caller) = own.callers.get(request) {
            assert_eq!(
                sub,
                Some(caller.as_str()),
                "the row names its calling key: {row}"
            );
            let call = row["call_id"].as_str().expect("call ID").replace('-', "");
            assert!(calls.insert(call, caller.clone()).is_none(), "{row}");
        }
    }
    assert_eq!(
        calls.len(),
        own.callers.len(),
        "every concurrent call is captured once"
    );

    let spans = journey
        .captured_json(
            &own.inspector,
            "SELECT trace_id, sub_principal_id, act_principal_id FROM vala.traces.spans",
            retained,
        )
        .await;
    let mut traced = BTreeSet::new();
    for span in &spans {
        let trace = span["trace_id"].as_str().expect("trace ID").to_owned();
        if let Some(caller) = calls.get(&trace) {
            assert_eq!(span["sub_principal_id"], json!(caller), "{span}");
            assert_eq!(span["act_principal_id"], json!(caller), "{span}");
            traced.insert(trace);
        }
    }
    assert_eq!(
        traced.len(),
        calls.len(),
        "every concurrent call has its span"
    );
    calls.into_keys().collect()
}

/// The live data file paths of `tenant`'s physical `vala.gateway.calls`
/// table, relative to the warehouse, with that table's location and
/// canonical object prefix.
///
/// # Panics
/// Panics when the binding resolves to nothing or the table, its manifest
/// list, or a manifest cannot be read.
async fn calls_data_files(
    journey: &Journey,
    tenant: DataTenantId,
) -> (String, String, Vec<String>) {
    let binding =
        TenantTableBinding::resolve((tenant, TableRef::new(BifrostNamespace::Gateway, "calls")))
            .expect("the calls table binds");
    let catalog = journey.server.bifrost_catalog().iceberg_catalog();
    let table = catalog
        .load_table(&binding.table_ident())
        .await
        .expect("the tenant's calls table loads");
    let location = table.metadata().location().to_owned();
    let mut paths = Vec::new();
    if let Some(snapshot) = table.metadata().current_snapshot() {
        let manifests = table
            .manifest_list_reader(snapshot)
            .load()
            .await
            .expect("manifest list");
        for manifest in manifests.entries() {
            let manifest = manifest
                .load_manifest(table.file_io())
                .await
                .expect("manifest");
            paths.extend(
                manifest
                    .entries()
                    .iter()
                    .filter(|entry| entry.is_alive())
                    .map(|entry| entry.data_file().file_path().to_owned()),
            );
        }
    }
    (location, binding.object_prefix, paths)
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
/// lower-rate calls succeed. With the limit lifted, two caller keys in each
/// tenant then call concurrently. After drain and flush, each tenant's real
/// client reads exactly its own answered calls, no refused call, and every
/// concurrent call once with its own key's subject and actor and its attempt
/// span under the same pair; neither tenant's call or trace identities reach
/// the other, and each tenant's rows live only in its own physical table.
///
/// # Panics
/// Panics when a status, timing, stream, refusal, or capture expectation
/// fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn gateway_capacity_paces_streams_isolates_tenants_and_recovers() {
    let journey = Journey::start().await;
    let paced = PacedUpstream::start(PACED, 32)
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

    journey
        .put_as(
            &other_admin,
            "governance-policy",
            json!({"limits": [], "budgets": [], "pricing": [], "unknown_cost": "allow_unpriced"}),
        )
        .await;
    let first_tenant = journey.server.data_tenant_id();
    let first_clients =
        per_client_tokens(&journey, first_tenant, "capacity_first", &[INVOKER_ROLE], 2).await;
    let other_clients = per_client_tokens(&journey, other, "capacity_other", &["admin"], 2).await;
    let (first_callers, other_callers) = tokio::join!(
        concurrent_calls(&journey, &first_clients, "first"),
        concurrent_calls(&journey, &other_clients, "other"),
    );
    let concurrent = 2 * CALLS_PER_CLIENT;
    assert_eq!(
        (first_callers.len(), other_callers.len()),
        (concurrent, concurrent)
    );
    let first = TenantCalls {
        tenant: first_tenant,
        inspector,
        callers: first_callers,
    };
    let second = TenantCalls {
        tenant: other,
        inspector: other_key,
        callers: other_callers,
    };

    journey.drain_calls().await;
    assert_eq!(
        journey
            .captured_calls(&first.inspector, 2 + concurrent)
            .await
            .len(),
        2 + concurrent,
        "the first tenant reads only its own calls"
    );
    assert_eq!(
        journey
            .captured_calls(&second.inspector, 3 + concurrent)
            .await
            .len(),
        3 + concurrent,
        "the second tenant reads its answered calls and no refusal"
    );
    let first_ids = assert_tenant_calls(&journey, &first, &second, 2 + concurrent).await;
    let other_ids = assert_tenant_calls(&journey, &second, &first, 3 + concurrent).await;
    assert!(
        first_ids.is_disjoint(&other_ids),
        "call and trace identities are disjoint"
    );

    let (first_location, first_prefix, first_files) =
        calls_data_files(&journey, first.tenant).await;
    let (other_location, other_prefix, other_files) =
        calls_data_files(&journey, second.tenant).await;
    assert_ne!(
        first_prefix, other_prefix,
        "each tenant binds its own prefix"
    );
    assert_ne!(
        first_location, other_location,
        "each tenant has its own table"
    );
    for (tenant, location, prefix, files, foreign) in [
        (
            first.tenant,
            &first_location,
            &first_prefix,
            &first_files,
            &other_prefix,
        ),
        (
            second.tenant,
            &other_location,
            &other_prefix,
            &other_files,
            &first_prefix,
        ),
    ] {
        assert!(
            location.ends_with(prefix.as_str()) && prefix.contains(&tenant.to_string()),
            "{location} is {tenant}'s own prefix {prefix}"
        );
        assert!(
            !files.is_empty(),
            "the tenant's rows are published in {location}"
        );
        for file in files {
            assert!(
                file.starts_with(prefix.as_str()) && !file.starts_with(foreign.as_str()),
                "{file} lives only under {prefix}"
            );
        }
    }
}
