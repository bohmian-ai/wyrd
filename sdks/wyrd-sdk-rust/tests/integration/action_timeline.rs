//! A Service's action is one trace: the spans it records, the writes it
//! queues, the model call it makes, and the verification it asks for read
//! back through `Bifrost::trace_timeline` as one connected tree, and only to
//! the tenant that made them.
//!
//! The in-process test server owns the process-global `tracing` subscriber,
//! which carries its operator and tenant export. The Service's own pipeline
//! is therefore a dispatcher scoped to the action, exporting through the
//! SDK's `span_exporter` as the Service, as `start_telemetry` would for a
//! Service in its own process.

use std::collections::HashMap;
use wyrd_ai::bifrost::BifrostOptions;

use opentelemetry::propagation::TextMapPropagator as _;
use opentelemetry::trace::{TraceContextExt as _, TracerProvider as _};
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::SdkTracerProvider;
use serde_json::{Value, json};
use tracing::Instrument as _;
use tracing::instrument::WithSubscriber as _;
use tracing_opentelemetry::OpenTelemetrySpanExt as _;
use tracing_subscriber::layer::SubscriberExt as _;
use wyrd_ai::bifrost::{Correlation, TableConfig};
use wyrd_ai::otel::span_exporter;
use wyrd_ai::{Bifrost, SpanRecord, TraceId, VerificationVerdict, WyrdClient};
use wyrd_spec::vala::trace::SpanKind;
use wyrd_testing::bifrost::shared_process_telemetry_for_test;

use crate::support::{Deployment, ancestry};
use crate::verify_in_real_time::Assistant;

/// Instrumentation scope of the Service's own spans.
const SERVICE_SCOPE: &str = "journey.service";
/// Scope of the gateway's captured model-call attempt rows.
const GATEWAY_SCOPE: &str = "wyrd.gateway";
/// The table the Service writes one row to.
const ACTIONS: &str = "vala.datasets.actions";
/// The trace identity of a caller's W3C `traceparent`.
const CALLER_TRACE: &str = "5bf92f3577b34da6a3ce929d0e0e4736";
/// The caller's span identity within [`CALLER_TRACE`].
const CALLER_SPAN: &str = "10f067aa0ba902b7";

/// The Service's tracing pipeline: a dispatcher whose spans export to the
/// server as `client` through the SDK's span exporter, and its provider.
///
/// # Panics
/// Panics when the exporter cannot be built.
pub(crate) fn service_telemetry(client: &WyrdClient) -> (tracing::Dispatch, SdkTracerProvider) {
    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(span_exporter(client).expect("the span exporter builds"))
        .build();
    let dispatch = tracing::Dispatch::new(
        tracing_subscriber::registry()
            .with(tracing_opentelemetry::layer().with_tracer(provider.tracer(SERVICE_SCOPE))),
    );
    (dispatch, provider)
}

/// The W3C trace headers of the current `tracing` span, injected by the
/// standard trace-context propagator as an OpenTelemetry-instrumented HTTP
/// client sends them.
fn trace_headers() -> HashMap<String, String> {
    let mut headers = HashMap::new();
    TraceContextPropagator::new().inject_context(&tracing::Span::current().context(), &mut headers);
    headers
}

/// Send one Chat Completions turn through the Gateway as `client`, the way
/// an OpenTelemetry-instrumented OpenAI client does, with the current span's
/// [`trace_headers`].
///
/// # Panics
/// Panics when no token mints or the Gateway does not answer.
async fn ask_the_gateway(deployment: &Deployment, client: &WyrdClient) {
    let token = client.access_token().await.expect("access token mints");
    reqwest::Client::new()
        .post(format!(
            "{}/v1/chat/completions",
            deployment
                .server()
                .base_url()
                .expect("bound server has a URL")
        ))
        .bearer_auth(token.expose())
        .headers(
            (&trace_headers())
                .try_into()
                .expect("trace headers are valid HTTP headers"),
        )
        .json(&json!({
            "model": "openai/gpt-4o",
            "messages": [{ "role": "user", "content": "hi" }],
        }))
        .send()
        .await
        .expect("the Gateway answers")
        .error_for_status()
        .expect("the Gateway accepts the call");
}

/// One line per span of `timeline`: its name, scope, kind, span id, and
/// parent span id, for a failure message a reader can follow.
fn outline(timeline: &[SpanRecord]) -> String {
    timeline
        .iter()
        .map(|span| {
            format!(
                "{} [{} {:?}] {} <- {}",
                span.name,
                span.scope.name,
                span.kind,
                span.span_id.to_hex(),
                span.parent_span_id
                    .map_or_else(|| "root".to_owned(), |parent| parent.to_hex())
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Names of `span` and its ancestors in `timeline`, nearest first.
fn lineage<'a>(timeline: &'a [SpanRecord], span: &'a SpanRecord) -> Vec<&'a str> {
    ancestry(timeline, span)
        .iter()
        .map(|span| span.name.as_str())
        .collect()
}

/// Whether `names` holds every one of `steps` in order, gaps allowed.
fn holds_in_order(names: &[&str], steps: &[fn(&str) -> bool]) -> bool {
    let mut steps = steps.iter().peekable();
    for name in names {
        steps.next_if(|step| step(name));
    }
    steps.peek().is_none()
}

/// A Service action that records spans, writes a Run observation and a
/// table row, calls a model through the Gateway, and verifies its answer
/// with an LLM judge reads back as one connected, time-ordered tree
/// rooted at the action's span. The queued writes are PRODUCER spans in that
/// tree, and the same read returns the CONSUMER ingest span linking each
/// producer as a linked trace. The operator's export holds every server span
/// of the action's trace with the same ids, and the consumers too. Every
/// request span is stamped with the Service as subject and actor, and the
/// judge's Gateway call keeps the Service as subject under the system actor.
///
/// # Panics
/// Panics when a step fails or the timeline or operator export differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn service_action_timeline_is_one_connected_tree() {
    let assistant = Assistant::start().await;
    let (_guard, operator) = shared_process_telemetry_for_test().expect("process telemetry");
    let checkpoint = operator.checkpoint().expect("operator checkpoint");
    let admin = Bifrost::connect_with(
        &assistant.deployment.admin(),
        BifrostOptions {
            table: Some(
                TableConfig::from_json_schema(
                    ACTIONS,
                    &json!({
                        "type": "object",
                        "properties": { "action": { "type": "string" } },
                        "required": ["action"],
                    }),
                )
                .expect("table declares"),
            ),
            ..BifrostOptions::default()
        },
    )
    .await
    .expect("Bifrost connects");
    admin.register().await.expect("table registers");
    let state = assistant.state().await;
    let service = assistant
        .deployment
        .client(&assistant.deployment.service_key(&assistant.service).await);
    let writer = Bifrost::connect_with(&service, BifrostOptions::default())
        .await
        .expect("Bifrost connects");
    let (dispatch, provider) = service_telemetry(&service);
    let action =
        tracing::dispatcher::with_default(&dispatch, || tracing::info_span!("service.action"));
    let trace = action
        .context()
        .span()
        .span_context()
        .trace_id()
        .to_string();

    async {
        tracing::info_span!("answer").in_scope(|| ());
        state
            .run_for_card("model")
            .expect("model run opens")
            .observe()
            .submit("latency-drift", json!({ "latency": 12.5 }))
            .await
            .expect("the latency is submitted");
        writer
            .use_table_by_name(ACTIONS)
            .await
            .expect("the table binds");
        writer
            .insert(
                serde_json::to_vec(&json!({ "action": "answered" })).expect("row serializes"),
                Correlation::default(),
            )
            .expect("row inserts");
        ask_the_gateway(&assistant.deployment, &service)
            .instrument(tracing::info_span!("model.ask"))
            .await;
        let result = state
            .run_for_card("agent")
            .expect("agent run opens")
            .observe()
            .verify("answer-judged", json!("yes"))
            .await
            .expect("the answer is judged");
        assert_eq!(result.outcome.verdict, VerificationVerdict::Passed);
    }
    .instrument(action)
    .with_subscriber(dispatch)
    .await;
    writer.shutdown().await.expect("the row drains");
    state.shutdown().await.expect("emits drain");
    tokio::task::block_in_place(|| provider.force_flush()).expect("Service spans export");

    let read = assistant.deployment.timeline(&trace).await;
    assert!(
        read.windows(2)
            .all(|pair| pair[0].start_time <= pair[1].start_time),
        "time-ordered: {}",
        outline(&read)
    );
    let (timeline, linked): (Vec<SpanRecord>, Vec<SpanRecord>) = read
        .into_iter()
        .partition(|span| span.trace_id.to_hex() == trace);
    let [root] = &timeline
        .iter()
        .filter(|span| span.parent_span_id.is_none())
        .collect::<Vec<_>>()[..]
    else {
        panic!("one root: {}", outline(&timeline));
    };
    assert_eq!(
        (root.name.as_str(), root.scope.name.as_str()),
        ("service.action", SERVICE_SCOPE)
    );
    for span in &timeline {
        assert_eq!(
            ancestry(&timeline, span).last().map(|top| top.span_id),
            Some(root.span_id),
            "{} is connected to the root: {}",
            span.name,
            outline(&timeline)
        );
    }
    assert!(timeline.iter().any(|span| span.name == "answer"));

    let producers: Vec<&SpanRecord> = timeline
        .iter()
        .filter(|span| span.kind == SpanKind::Producer && span.name == "bifrost.enqueue")
        .collect();
    assert!(
        producers.len() >= 2,
        "the observation and the row: {}",
        outline(&timeline)
    );

    let attempts: Vec<Vec<&str>> = timeline
        .iter()
        .filter(|span| span.scope.name == GATEWAY_SCOPE)
        .map(|attempt| lineage(&timeline, attempt))
        .collect();
    assert_eq!(attempts.len(), 2, "{attempts:?}");
    assert!(
        attempts.iter().any(|names| holds_in_order(
            names,
            &[
                |name| name == "gateway.call",
                |name| name == "request",
                |name| name == "model.ask"
            ]
        )),
        "the model call: {attempts:?}"
    );
    assert!(
        attempts.iter().any(|names| holds_in_order(
            names,
            &[
                |name| name == "gateway.call",
                |name| name.starts_with("verification."),
                |name| name == "request",
                |name| name == "service.action"
            ]
        )),
        "verification, judge, gateway: {attempts:?}"
    );

    let server = assistant.deployment.server();
    let stamped = server
        .retained_span_records(
            server.data_tenant_id(),
            "encode(span_id, 'hex'), sub_principal_id, act_principal_id",
            &format!("trace_id = X'{trace}'"),
        )
        .await
        .expect("the stamped pairs read");
    let pair = |span: &SpanRecord| {
        stamped
            .iter()
            .find(|row| row[0].as_deref() == Some(span.span_id.to_hex().as_str()))
            .map(|row| (row[1].clone(), row[2].clone()))
            .expect("every tenant span is stamped")
    };
    let requests: Vec<_> = timeline
        .iter()
        .filter(|span| span.name == "request")
        .map(pair)
        .collect();
    let judge: Vec<_> = timeline
        .iter()
        .filter(|span| {
            span.scope.name == GATEWAY_SCOPE
                && lineage(&timeline, span)
                    .iter()
                    .any(|name| name.starts_with("verification."))
        })
        .map(pair)
        .collect();
    let service = requests.first().and_then(|(sub, _)| sub.clone());
    assert!(
        service.is_some()
            && requests
                .iter()
                .all(|(sub, act)| *sub == service && *act == service),
        "each request is the Service acting for itself: {requests:?}"
    );
    assert!(
        !judge.is_empty()
            && judge
                .iter()
                .all(|(sub, act)| *sub == service && act.is_some() && *act != service),
        "the judge call keeps the Service as subject under the system actor: {judge:?}"
    );

    let exported = operator.spans_since(&checkpoint);
    for span in timeline
        .iter()
        .filter(|span| span.scope.name != SERVICE_SCOPE && span.scope.name != GATEWAY_SCOPE)
    {
        let parent = span.parent_span_id.map(|parent| parent.to_hex());
        assert!(
            exported.iter().any(|copy| copy.trace_id == trace
                && copy.span_id == span.span_id.to_hex()
                && Some(&copy.parent_span_id) == parent.as_ref()
                && copy.name == span.name),
            "the operator holds {} with the same ids",
            span.name
        );
    }
    for producer in producers {
        assert!(
            linked
                .iter()
                .any(|consumer| consumer.name == "bifrost.gate.write"
                    && consumer.kind == SpanKind::Consumer
                    && consumer.links.iter().any(|link| {
                        link.trace_id == producer.trace_id && link.span_id == producer.span_id
                    })),
            "the tenant reads a CONSUMER ingest span linking {}: {}",
            producer.span_id.to_hex(),
            outline(&linked)
        );
        let link = (trace.clone(), producer.span_id.to_hex());
        assert!(
            exported
                .iter()
                .any(|copy| copy.name == "bifrost.gate.write" && copy.links.contains(&link)),
            "the operator holds a CONSUMER ingest span linking {link:?}"
        );
    }
    assistant.deployment.shutdown().await;
}

/// A request one tenant made under a caller's trace reads back to that
/// tenant, and another tenant's timeline read of the same trace is empty.
///
/// # Panics
/// Panics when a request or read fails, the owner sees nothing, or the
/// other tenant sees anything.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn another_tenant_cannot_read_the_timeline() {
    let deployment = Deployment::start().await;
    let token = deployment
        .admin()
        .access_token()
        .await
        .expect("access token mints");
    let listed: Value = reqwest::Client::new()
        .get(format!(
            "{}/v1/bifrost/tables",
            deployment
                .server()
                .base_url()
                .expect("bound server has a URL")
        ))
        .header("x-wyrd-access-token", format!("Bearer {}", token.expose()))
        .header("traceparent", format!("00-{CALLER_TRACE}-{CALLER_SPAN}-01"))
        .send()
        .await
        .expect("the server answers")
        .error_for_status()
        .expect("the tables list")
        .json()
        .await
        .expect("the list is JSON");
    assert!(listed.is_array(), "{listed}");
    assert!(
        !deployment.timeline(CALLER_TRACE).await.is_empty(),
        "the owning tenant reads its request"
    );

    let foreign = deployment.client(&deployment.other_tenant_admin("other-tenant").await);
    let timeline = Bifrost::connect_with(&foreign, BifrostOptions::default())
        .await
        .expect("Bifrost connects")
        .trace_timeline(TraceId::from_hex(CALLER_TRACE).expect("trace id"))
        .await
        .expect("the timeline reads");
    assert!(timeline.is_empty(), "{}", outline(&timeline));
    deployment.shutdown().await;
}
