//! Continuous Eval verification journey through the real SDK, server, and a
//! local OpenAI-compatible provider.
//!
//! One Service binds two Agents to five `observations_ready` Eval Verifiers.
//! Eval records emitted through `observe.eval(...)` are acknowledged by Scribe,
//! enqueue runs after the ACK, and are executed by the verification runtime
//! through the existing Vala engine and production judge. The journey proves
//! the terminal matrix: gate pass and fail (with one Operator dispatch),
//! ungated, all-skipped, and sampled-out `inconclusive`, missing-context and
//! cross-tenant-media errors retried to `errored`, a trace that lands after an
//! `AwaitingTrace` requeue, and a trace that never lands timing out. A forced
//! post-commit enqueue failure keeps the acknowledged observation and creates
//! no run, and runs enqueued while no runtime is running complete after it
//! starts. A separate trace Service proves that trace tasks read persisted
//! events and links in one stable span order bounded to the record's trace
//! window, and that a trace over the span ceiling fails before any judge call.
//! Provider, media-locator, and Postgres failures settle with stable codes and
//! fixed text that expose none of their dependency detail. Two Services that
//! bind the same Agent prove each Eval record runs only its writer's bindings.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use arrow::array::{Array, StringArray};
use arrow::record_batch::RecordBatch;
use base64::Engine;
use secrecy::ExposeSecret;
use serde_json::{Value, json};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_client::cards::{
    CardGraphHydrator, CardSelector, Cards, HydrationMode, RegistrationReceipt,
};
use wyrd_client::observe::{EvalObservationOptions, Run};
use wyrd_client::state::WyrdState;
use wyrd_client::{QueueConfig, WyrdClient};
use wyrd_server::query::scheduled::ScheduledQueryCaller;
use wyrd_server::verification::{RuntimeLimits, VerificationRuntime};
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::BifrostQueryRequest;
use wyrd_spec::vala::managed_columns::CARD_UID;
use wyrd_storage::tenant_path;
use wyrd_testing::Bootstrap;
use wyrd_testing::WyrdTestServer;
use wyrd_testing::verification::{ObservationRun, VerificationFixture};
use wyrd_tonic::otlp::common::v1::{AnyValue, KeyValue, any_value};
use wyrd_tonic::otlp::resource::v1::Resource;
use wyrd_tonic::otlp::trace::v1::{ResourceSpans, ScopeSpans, Span};
use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;
use wyrd_tonic::otlp::trace_service::trace_service_client::TraceServiceClient;
use wyrd_tonic::tonic::Request;
use wyrd_tonic::tonic::transport::Channel;

use super::query::{ServerJourneyError, scheduled_context};

/// The server's fixed Eval media ceiling; a body one byte larger is refused.
const MEDIA_LIMIT_BYTES: usize = 20 * 1024 * 1024;
/// Upper bound on every wait for runs to settle.
const WAIT: Duration = Duration::from_mins(2);
/// The image bytes the judge must receive natively.
const IMAGE: &[u8] = b"\x89PNG\r\n\x1a\neval-journey-image";
/// Trace whose spans land after the run first awaits them.
const LANDED_TRACE: &str = "5b8efff798038103d269b633813fc60c";
/// Trace whose spans never land.
const MISSING_TRACE: &str = "6c9f000809149214e37ac744924fd71d";
/// Name of the one span exported under [`LANDED_TRACE`].
const SPAN_NAME: &str = "judge-call";

/// Write the Service graph: two Agents, five Eval Verifiers, one explicit-only
/// task Verifier, the judge Prompt, and one Operator. Returns the Service path.
///
/// The judge Prompt is native OpenAI Chat with a JSON-schema response and one
/// `${media:shot}` placeholder, built through Skald rather than hand-written.
///
/// # Panics
/// Panics when the judge prompt cannot be built or a file cannot be written.
fn write_graph(root: &Path) -> PathBuf {
    let judge = skald_prompt::openai_chat(
        "gpt-test",
        skald_prompt::OpenAiChatOptions {
            messages: vec!["Grade the answer ${answer} against ${media:shot}.".to_owned()],
            variables: vec!["answer".to_owned()],
            output: Some(
                skald_prompt::ResponseFormat::json_schema(
                    "judge_result",
                    json!({
                        "type": "object",
                        "properties": { "passed": { "type": "boolean" } },
                        "required": ["passed"],
                        "additionalProperties": false
                    }),
                )
                .expect("the judge response format builds"),
            ),
            ..skald_prompt::OpenAiChatOptions::default()
        },
    )
    .expect("the judge prompt builds");
    let card = json!({
        "apiVersion": "wyrd/v1",
        "kind": "Prompt",
        "metadata": { "name": "eval-judge-prompt", "version": "1.0.0", "space": "default" },
        "spec": judge.into_native(),
    });
    let files = [
        ("judge-prompt.json", card.to_string()),
        (
            "agent-prompt.yaml",
            "apiVersion: wyrd/v1\nkind: Prompt\nmetadata:\n  name: eval-agent-prompt\n  version: 1.0.0\n  space: default\nspec:\n  provider: openai\n  model: gpt-test\n  messages: [answer the question]\n".to_owned(),
        ),
        ("agent.yaml", agent("eval-agent")),
        ("traced-agent.yaml", agent("eval-traced-agent")),
        (
            "operator.yaml",
            "apiVersion: wyrd/v1\nkind: Operator\nmetadata:\n  name: eval-operator\n  version: 1.0.0\n  space: default\nspec:\n  kind: http\n  method: post\n  url: https://hooks.example.com/eval-gate-failed\n".to_owned(),
        ),
        (
            "gated.yaml",
            verifier(
                "eval-gated",
                "      pass_gate: {kind: all_pass}\n      tasks:\n        answer: {kind: assertion, id: answer, context_path: $.answer, operator: equals, expected: \"yes\"}\n        judge:\n          kind: llm_judge\n          id: judge\n          judge_ref: {prompt: ./judge-prompt.json, tool_names: [], run_config: {max_iterations: 1}}\n          context_path: $.answer\n          operator: equals\n          expected: {passed: true}\n          max_retries: 0\n",
            ),
        ),
        (
            "ungated.yaml",
            verifier(
                "eval-ungated",
                "      context_capture: redact\n      tasks:\n        answer: {kind: assertion, id: answer, context_path: $.answer, operator: equals, expected: \"yes\"}\n",
            ),
        ),
        (
            "skipped.yaml",
            verifier(
                "eval-skipped",
                "      pass_gate: {kind: all_pass}\n      tasks:\n        answer:\n          kind: assertion\n          id: answer\n          context_path: $.answer\n          operator: equals\n          expected: \"yes\"\n          condition: {path: $.answer, operator: equals, expected: never}\n",
            ),
        ),
        (
            "sampled.yaml",
            verifier(
                "eval-sampled",
                "      sampling: {kind: ratio, ratio: 0.0}\n      pass_gate: {kind: all_pass}\n      tasks:\n        answer: {kind: assertion, id: answer, context_path: $.answer, operator: equals, expected: \"yes\"}\n",
            ),
        ),
        (
            "traced.yaml",
            verifier(
                "eval-traced",
                &format!("      pass_gate: {{kind: all_pass}}\n      tasks:\n        span_name: {{kind: trace_assertion, id: span_name, span_selector: \"$.spans[0].name\", operator: equals, expected: {SPAN_NAME}}}\n        span_tier: {{kind: trace_assertion, id: span_tier, span_selector: \"$.spans[0].attributes.tier\", operator: equals, expected: gold}}\n"),
            ),
        ),
        (
            "task.yaml",
            "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: eval-task\n  version: 1.0.0\n  space: default\nspec:\n  implementation:\n    kind: task\n    spec: {kind: assertion, id: x_is_one, context_path: $.x, operator: equals, expected: 1}\n".to_owned(),
        ),
        (
            "service.yaml",
            "apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: eval-service\n  version: 1.0.0\n  space: default\nspec:\n  service_type: agent\n  components:\n    - alias: agent\n      ref: ./agent.yaml\n      verified_by:\n        - verifier: ./gated.yaml\n          runs_on: {kind: observations_ready}\n          on_failure: [./operator.yaml]\n        - verifier: ./ungated.yaml\n          runs_on: {kind: observations_ready}\n        - verifier: ./skipped.yaml\n          runs_on: {kind: observations_ready}\n        - verifier: ./sampled.yaml\n          runs_on: {kind: observations_ready}\n        - verifier: ./task.yaml\n    - alias: traced\n      ref: ./traced-agent.yaml\n      verified_by:\n        - verifier: ./traced.yaml\n          runs_on: {kind: observations_ready}\n".to_owned(),
        ),
    ];
    for (name, body) in files {
        std::fs::write(root.join(name), body).expect("fixture file writes");
    }
    root.join("service.yaml")
}

/// One Agent Card named `name` over the shared agent Prompt.
fn agent(name: &str) -> String {
    format!(
        "apiVersion: wyrd/v1\nkind: Agent\nmetadata:\n  name: {name}\n  version: 1.0.0\n  space: default\nspec:\n  prompt: ./agent-prompt.yaml\n  run_config:\n    max_iterations: 1\n"
    )
}

/// One Eval Verifier Card named `name` whose Eval spec body is `spec`.
fn verifier(name: &str, spec: &str) -> String {
    format!(
        "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: {name}\n  version: 1.0.0\n  space: default\nspec:\n  implementation:\n    kind: eval\n    spec:\n{spec}"
    )
}

/// Build one public client over the bound server.
///
/// # Panics
/// Panics when the client cannot be assembled.
fn connect(server: &WyrdTestServer, credential: &str) -> WyrdClient {
    wyrd_client::bifrost::client_from_options(
        Some(server.base_url().expect("bound server has a URL")),
        Some(credential),
        server.grpc_url().as_deref(),
    )
    .expect("client builds")
}

/// Register the Service graph and hydrate it into a complete local bundle.
///
/// # Panics
/// Panics when registration or hydration fails.
async fn register(client: &WyrdClient, service: &Path, bundle: &Path) -> RegistrationReceipt {
    let cards = Cards::with_client(WyrdClient::clone(client));
    let receipt = Box::pin(cards.register_from_path(service))
        .await
        .expect("the Eval Service graph registers");
    Box::pin(CardGraphHydrator::new(cards.registry_context()).hydrate(
        &CardSelector::exact(receipt.root.clone()),
        bundle,
        HydrationMode::Complete,
    ))
    .await
    .expect("complete bundle hydrates");
    receipt
}

/// Unwrap a machine bootstrap into its raw API key.
///
/// # Panics
/// Panics when the bootstrap is a user principal.
fn api_key(bootstrap: Bootstrap) -> String {
    match bootstrap {
        Bootstrap::Machine { api_key, .. } => api_key.expose_secret().to_owned(),
        Bootstrap::User { .. } => panic!("expected a machine principal"),
    }
}

/// Emit one Eval record from `view` with optional media JSON and trace.
///
/// # Panics
/// Panics when the options or the emit are refused.
fn emit(view: &Run, context: &Value, media: Option<&str>, trace: Option<&str>) {
    view.observe()
        .eval(
            context,
            EvalObservationOptions::from_parts(None, media, trace, None)
                .expect("journey options parse"),
        )
        .expect("eval emits");
}

/// One media descriptor JSON array naming `uri` as the `shot` PNG image.
fn media(uri: &str) -> String {
    media_as(uri, "image/png")
}

/// One media descriptor JSON array naming `uri` as the `shot` image declared
/// as `media_type`, which the server may refuse for the image kind.
fn media_as(uri: &str, media_type: &str) -> String {
    json!([{ "id": "shot", "kind": "image", "uri": uri, "media_type": media_type }]).to_string()
}

/// Start one SDK Bifrost lifetime over the hydrated bundle.
///
/// # Panics
/// Panics when the bundle does not load or Bifrost does not start.
async fn start_state(bundle: &Path, client: &WyrdClient) -> WyrdState {
    let state = WyrdState::from_path_with_client(bundle, client.clone())
        .expect("complete bundle loads offline");
    state
        .start_bifrost_with_config(None, QueueConfig::default())
        .await
        .expect("Bifrost starts");
    state
}

/// Compose and spawn a verification runtime with fast bounds and the local
/// provider; returns its stop token and task.
///
/// # Panics
/// Panics when the runtime does not compose.
fn spawn_runtime(server: &WyrdTestServer, provider: &str) -> (CancellationToken, JoinHandle<()>) {
    let providers = skald_runtime::ProviderRegistry::for_provider(
        &skald_spec::ProviderName::OpenAi,
        provider,
        Some("journey-key"),
    )
    .expect("the local provider registers");
    let runtime = VerificationRuntime::builder(server.state())
        .limits(RuntimeLimits {
            lease: Duration::from_mins(1),
            execution_timeout: Duration::from_secs(20),
            drain_grace: Duration::from_secs(10),
            poll_interval: Duration::from_millis(50),
            restart_backoff: Duration::from_millis(300),
            trace_poll: Duration::from_millis(200),
            trace_deadline: Duration::from_secs(20),
            ..RuntimeLimits::default()
        })
        .providers(Arc::new(providers))
        .build()
        .expect("the runtime composes");
    let stop = CancellationToken::new();
    let task = tokio::spawn(runtime.run(stop.clone()));
    (stop, task)
}

/// Poll until `count` observation runs exist and every one is terminal,
/// bringing each retry's deadline due in the database so bounded retries
/// exhaust without waiting out production backoff.
///
/// # Errors
/// Returns a fixture error or a timeout naming the unsettled runs.
async fn settle(
    seed: &VerificationFixture,
    count: usize,
) -> Result<Vec<ObservationRun>, ServerJourneyError> {
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let runs = seed.observation_runs().await?;
        let open: Vec<&ObservationRun> = runs
            .iter()
            .filter(|run| {
                !matches!(
                    run.state.status.as_str(),
                    "completed" | "errored" | "timed_out"
                )
            })
            .collect();
        if runs.len() >= count && open.is_empty() {
            return Ok(runs);
        }
        for run in open.iter().filter(|run| run.state.status == "retrying") {
            seed.make_retry_due(run.run).await?;
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("runs never settled ({} of {count}): {runs:?}", runs.len()).into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Watch the run queue as the superuser for one second, twenty runtime poll
/// intervals, while a test policy makes tenant Card reads fail, and confirm
/// no open observation run was claimed or charged an attempt.
///
/// A claim reads the claimed run's Verifier Card status in its own
/// transaction, so a failing Card read rolls the claim back and consumes no
/// attempt. [`settle`] names each run's Verifier from its Card, so it cannot
/// poll while the policy stands; the superuser bypasses row security and
/// reads only the run queue.
///
/// # Errors
/// Returns a query error, or the number of open runs that were claimed.
async fn unclaimed_while_cards_fail(superuser: &sqlx::PgPool) -> Result<(), ServerJourneyError> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
    while tokio::time::Instant::now() < deadline {
        let claimed: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM wyrd.verifier_runs WHERE origin = 'observation' \
                AND status NOT IN ('completed', 'errored', 'timed_out') \
                AND (status <> 'pending' OR attempts <> 0)",
        )
        .fetch_one(superuser)
        .await?;
        if claimed != 0 {
            return Err(format!("{claimed} runs were claimed while Card reads fail").into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Ok(())
}

/// Poll until at least `count` observation runs exist while no runtime runs,
/// and return how many exist.
///
/// # Errors
/// Returns a fixture error, a timeout, or an error when any run was touched.
async fn enqueued(seed: &VerificationFixture, count: usize) -> Result<usize, ServerJourneyError> {
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let runs = seed.observation_runs().await?;
        if runs.len() >= count {
            if runs.iter().any(|run| run.state.status != "pending") {
                return Err(format!("a run moved without a runtime: {runs:?}").into());
            }
            return Ok(runs.len());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("only {} of {count} runs were enqueued", runs.len()).into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Run `sql` for `tenant` through the server's own query entry.
///
/// # Errors
/// Returns the context or query error.
async fn query(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    sql: String,
) -> Result<Vec<RecordBatch>, ServerJourneyError> {
    let mut batches = Vec::new();
    ScheduledQueryCaller::new(
        server.state().clone(),
        scheduled_context(tenant)?,
        CancellationToken::new(),
    )
    .run_with(
        BifrostQueryRequest {
            params: Vec::new(),
            sql,
            deadline_ms: Some(30_000),
        },
        |batch| {
            batches.push(batch);
            Ok(())
        },
    )
    .await?;
    Ok(batches)
}

/// Every value of the first column of `batches`, cast to text.
///
/// # Errors
/// Returns an error when the column cannot be cast to text.
fn texts(batches: &[RecordBatch]) -> Result<Vec<Option<String>>, ServerJourneyError> {
    let mut values = Vec::new();
    for batch in batches {
        let column = arrow::compute::cast(batch.column(0), &arrow::datatypes::DataType::Utf8)?;
        let column = column
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or("the cast column is not text")?;
        for row in 0..column.len() {
            values.push(column.is_valid(row).then(|| column.value(row).to_owned()));
        }
    }
    Ok(values)
}

/// The one run of `verifier` over `record`.
///
/// # Errors
/// Returns an error naming the missing run.
fn run_of<'a>(
    runs: &'a [ObservationRun],
    verifier: &str,
    record: &str,
) -> Result<&'a ObservationRun, ServerJourneyError> {
    let found: Vec<&ObservationRun> = runs
        .iter()
        .filter(|run| run.verifier == verifier && run.record_id == record)
        .collect();
    match found.as_slice() {
        [run] => Ok(run),
        _ => Err(format!("expected one {verifier} run for {record}, found {found:?}").into()),
    }
}

/// Assert `run` completed with `verdict`, `items` result items of which
/// `skipped` are skipped, and `dispatches` Operator dispatches.
///
/// # Errors
/// Returns a description of the first mismatch.
async fn assert_completed(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    run: &ObservationRun,
    verdict: &str,
    (items, skipped): (usize, usize),
    dispatches: i64,
) -> Result<(), ServerJourneyError> {
    let ("completed", Some(result)) = (run.state.status.as_str(), run.state.result_id) else {
        return Err(format!("{} did not complete: {run:?}", run.verifier).into());
    };
    let verdicts = texts(
        &query(
            server,
            tenant,
            format!("SELECT verdict FROM vala.verification.results WHERE result_id = '{result}'"),
        )
        .await?,
    )?;
    let kinds = texts(
        &query(
            server,
            tenant,
            format!(
                "SELECT outcome_kind FROM vala.eval.result_items WHERE result_id = '{result}' \
                 AND source_record_id = '{}'",
                run.record_id
            ),
        )
        .await?,
    )?;
    let skips = kinds
        .iter()
        .filter(|kind| kind.as_deref() == Some("skipped"))
        .count();
    if verdicts != [Some(verdict.to_owned())]
        || kinds.len() != items
        || skips != skipped
        || run.dispatches != dispatches
    {
        return Err(format!(
            "{}: expected {verdict} with {items} items ({skipped} skipped) and {dispatches} \
             dispatches, read {verdicts:?} {kinds:?} {}",
            run.verifier, run.dispatches
        )
        .into());
    }
    Ok(())
}

/// Assert `run` ended `status` with no result row and no dispatch.
///
/// # Errors
/// Returns a description of the mismatch.
async fn assert_unresulted(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    run: &ObservationRun,
    status: &str,
) -> Result<(), ServerJourneyError> {
    let results = query(
        server,
        tenant,
        format!(
            "SELECT result_id FROM vala.verification.results \
             WHERE {CARD_UID} = '{}' AND source_record_id = '{}'",
            run.verifier_uid, run.record_id
        ),
    )
    .await?;
    let rows: usize = results.iter().map(RecordBatch::num_rows).sum();
    if run.state.status != status
        || run.state.result_id.is_some()
        || rows != 0
        || run.dispatches != 0
    {
        return Err(
            format!("expected {status} without results: {run:?}, {rows} result rows").into(),
        );
    }
    Ok(())
}

/// One string-valued OTLP attribute.
fn text_attribute(key: &str, value: &str) -> KeyValue {
    KeyValue {
        key: key.to_owned(),
        value: Some(AnyValue {
            value: Some(any_value::Value::StringValue(value.to_owned())),
        }),
    }
}

/// Export one span under `trace` over OTLP/gRPC authenticated by `token`.
///
/// # Panics
/// Panics when the collector cannot be dialed or refuses the export.
async fn export_span(server: &WyrdTestServer, token: &str, trace: &str) {
    let start = u64::try_from(chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default())
        .expect("now is after the epoch");
    let span = Span {
        trace_id: hex::decode(trace).expect("the trace fixture is hex"),
        span_id: hex::decode("1a2b3c4d5e6f7081").expect("the span fixture is hex"),
        name: SPAN_NAME.to_owned(),
        kind: 3,
        start_time_unix_nano: start,
        end_time_unix_nano: start + 1_000_000,
        attributes: vec![text_attribute("tier", "gold")],
        ..Span::default()
    };
    export_spans(server, token, vec![span]).await;
}

/// Export `spans` in one OTLP/gRPC request authenticated by `token`, in the
/// given order.
///
/// # Panics
/// Panics when the collector cannot be dialed or refuses the export.
async fn export_spans(server: &WyrdTestServer, token: &str, spans: Vec<Span>) {
    let channel = Channel::from_shared(server.grpc_url().expect("bound server serves gRPC"))
        .expect("the gRPC URL is a valid endpoint")
        .connect()
        .await
        .expect("the OTLP exporter dials the collector");
    let mut request = Request::new(ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(Resource {
                attributes: vec![text_attribute("service.name", "eval-journey")],
                ..Resource::default()
            }),
            scope_spans: vec![ScopeSpans {
                spans,
                ..ScopeSpans::default()
            }],
            ..ResourceSpans::default()
        }],
    });
    request.metadata_mut().insert(
        "x-wyrd-access-token",
        format!("Bearer {token}")
            .parse()
            .expect("the bearer is valid metadata"),
    );
    TraceServiceClient::new(channel)
        .export(request)
        .await
        .expect("the collector accepts the spans");
}

/// The record ID of the one observation whose context carries `marker`.
///
/// # Errors
/// Returns the query error or an error when not exactly one row matches.
async fn record_id(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    marker: &str,
) -> Result<String, ServerJourneyError> {
    let ids = texts(
        &query(
            server,
            tenant,
            format!(
                "SELECT record_id FROM vala.eval.observations WHERE context LIKE '%\"{marker}\"%'"
            ),
        )
        .await?,
    )?;
    match ids.as_slice() {
        [Some(id)] => Ok(id.clone()),
        _ => Err(format!("expected one observation marked {marker}, read {ids:?}").into()),
    }
}

/// Continuous Eval runs the full terminal matrix through the real SDK,
/// server, Oracle, Scribe, object storage, and a local OpenAI-compatible
/// provider that receives the record's image as native base64 content.
///
/// Unbound prompt media, an unsupported image MIME, an oversized body, and a
/// cross-tenant URI each settle `errored` with `eval_execution_failed`, no
/// result, no dispatch, and no provider request.
///
/// The matrix records are received one day ahead of the SDK's clock, yet each
/// run freezes and reads its record by the client emit time the SDK stamped
/// as `wyrd_event_time`, never by the receipt day.
///
/// # Errors
/// Returns server, registration, query, or fixture errors, or a description of
/// the first terminal state that does not match the matrix.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn continuous_eval_runs_the_terminal_matrix() -> Result<(), ServerJourneyError> {
    let root = tempfile::tempdir()?;
    let service = write_graph(root.path());
    let bundle = root.path().join("bundle");
    let server = Box::pin(WyrdTestServer::start_bound()).await?;
    let tenant = server.data_tenant_id();
    let seed = VerificationFixture::provision(server.state().postgres.wyrd(), tenant).await?;
    let admin = api_key(
        server
            .bootstrap_service("eval_journey_admin", &["admin"])
            .await?,
    );
    let receipt = register(&connect(&server, &admin), &service, &bundle).await;
    let writer = api_key(
        server
            .credential_registered_service(&receipt.root, &[])
            .await?,
    );
    let client = connect(&server, &writer);

    let provider = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "chatcmpl_eval", "object": "chat.completion", "created": 1_700_000_000,
            "model": "gpt-test",
            "choices": [{ "index": 0, "finish_reason": "stop",
                "message": { "role": "assistant", "content": "{\"passed\":true}" } }],
            "usage": { "prompt_tokens": 5, "completion_tokens": 3, "total_tokens": 8 }
        })))
        .mount(&provider)
        .await;

    // The matrix records are received one day ahead of the SDK's own clock,
    // so a receipt-stamped `wyrd_event_time` would land on the next UTC day;
    // the SDK's emit-time stamp must win.
    let scribe = server.bifrost_scribe().ok_or("the server owns no Scribe")?;
    scribe.shift_receipt_clock_for_test(DAY);
    let state = start_state(&bundle, &client).await;
    let run = state.run();
    let agent = run.for_card("agent")?;
    let agent_uid = agent.subject().uid.clone().ok_or("agent has no UID")?;
    let object = format!("{tenant}/cards/{agent_uid}/shot.png");
    server
        .state()
        .storage
        .put_object(&tenant_path::validate(&object, tenant)?, IMAGE.to_vec())
        .await?;
    let shot = media(&format!("file:///{object}"));
    let foreign = media(&format!(
        "file:///{}/cards/{agent_uid}/shot.png",
        uuid::Uuid::now_v7()
    ));
    let unsupported = media_as(&format!("file:///{object}"), "image/tiff");
    let oversized_object = format!("{tenant}/cards/{agent_uid}/oversized.png");
    server
        .state()
        .storage
        .put_object(
            &tenant_path::validate(&oversized_object, tenant)?,
            vec![0; MEDIA_LIMIT_BYTES + 1],
        )
        .await?;
    let oversized = media(&format!("file:///{oversized_object}"));

    let emitted_from = chrono::Utc::now();
    emit(
        &agent,
        &json!({ "answer": "yes", "marker": "pass" }),
        Some(&shot),
        None,
    );
    emit(
        &agent,
        &json!({ "answer": "no", "marker": "fail" }),
        Some(&shot),
        None,
    );
    emit(&agent, &json!({ "marker": "missing" }), Some(&shot), None);
    emit(
        &agent,
        &json!({ "answer": "yes", "marker": "foreign" }),
        Some(&foreign),
        None,
    );
    // Negative media: an unbound `${media:shot}`, a supported kind with an
    // unsupported MIME, and a body past the ceiling must each error before
    // any provider call.
    emit(
        &agent,
        &json!({ "answer": "yes", "marker": "unbound" }),
        None,
        None,
    );
    emit(
        &agent,
        &json!({ "answer": "yes", "marker": "unsupported" }),
        Some(&unsupported),
        None,
    );
    emit(
        &agent,
        &json!({ "answer": "yes", "marker": "oversized" }),
        Some(&oversized),
        None,
    );
    let emitted_to = chrono::Utc::now();
    state.shutdown().await?;
    scribe.shift_receipt_clock_for_test(Duration::ZERO);
    server.flush_bifrost().await?;

    // Runs enqueued while no runtime runs are durable and complete once one
    // starts; this is the restart-recovery path.
    let queued = enqueued(&seed, 28).await?;
    let (stop, task) = spawn_runtime(&server, &provider.uri());
    let runs = settle(&seed, queued).await?;
    stop.cancel();
    tokio::time::timeout(WAIT, task).await??;
    server.flush_bifrost().await?;

    let [
        pass,
        fail,
        missing,
        foreign_id,
        unbound,
        unsupported,
        oversized,
    ] = [
        record_id(&server, tenant, "pass").await?,
        record_id(&server, tenant, "fail").await?,
        record_id(&server, tenant, "missing").await?,
        record_id(&server, tenant, "foreign").await?,
        record_id(&server, tenant, "unbound").await?,
        record_id(&server, tenant, "unsupported").await?,
        record_id(&server, tenant, "oversized").await?,
    ];
    let gated_pass = run_of(&runs, "eval-gated", &pass)?;
    assert_completed(&server, tenant, gated_pass, "passed", (2, 0), 0).await?;
    assert_completed(
        &server,
        tenant,
        run_of(&runs, "eval-gated", &fail)?,
        "failed",
        (2, 0),
        1,
    )
    .await?;
    for record in [&pass, &fail] {
        assert_completed(
            &server,
            tenant,
            run_of(&runs, "eval-ungated", record)?,
            "inconclusive",
            (1, 0),
            0,
        )
        .await?;
        assert_completed(
            &server,
            tenant,
            run_of(&runs, "eval-skipped", record)?,
            "inconclusive",
            (1, 1),
            0,
        )
        .await?;
        assert_completed(
            &server,
            tenant,
            run_of(&runs, "eval-sampled", record)?,
            "inconclusive",
            (0, 0),
            0,
        )
        .await?;
    }
    for record in [&missing, &foreign_id, &unbound, &unsupported, &oversized] {
        let errored = run_of(&runs, "eval-gated", record)?;
        assert_unresulted(&server, tenant, errored, "errored").await?;
        if errored.state.error_code.as_deref() != Some("eval_execution_failed") {
            return Err(format!("the error is not an execution failure: {errored:?}").into());
        }
    }
    // Every result, sampled out or scored, records the application Run that
    // wrote its observation, never the Verifier run.
    let correlated = texts(
        &query(
            &server,
            tenant,
            "SELECT DISTINCT run_id FROM vala.verification.results".to_owned(),
        )
        .await?,
    )?;
    if correlated != [Some(run.run_id().as_str().to_owned())] {
        return Err(
            format!("results are not correlated to the application Run: {correlated:?}").into(),
        );
    }

    // The frozen record identity and the committed event time of the row.
    let times = texts(
        &query(
            &server,
            tenant,
            format!(
                "SELECT CAST(wyrd_event_time AS BIGINT) FROM vala.eval.observations \
                 WHERE record_id = '{pass}'"
            ),
        )
        .await?,
    )?;
    if times != [Some(gated_pass.event_time.timestamp_micros().to_string())] {
        return Err(format!(
            "the run froze {} but the row holds {times:?}",
            gated_pass.event_time
        )
        .into());
    }
    // The run froze the SDK's emit-time stamp, not the receipt a day ahead.
    if !(emitted_from..=emitted_to).contains(&gated_pass.event_time) {
        return Err(format!(
            "the run froze {} outside the emit window [{emitted_from}, {emitted_to}]",
            gated_pass.event_time
        )
        .into());
    }
    // The exact row reads back by the frozen managed day.
    let day = gated_pass.event_time.date_naive();
    let read = texts(
        &query(
            &server,
            tenant,
            format!(
                "SELECT record_id FROM vala.eval.observations \
                 WHERE record_id = '{pass}' \
                   AND wyrd_event_time >= '{}' AND wyrd_event_time < '{}'",
                day.and_hms_opt(0, 0, 0)
                    .ok_or("midnight")?
                    .and_utc()
                    .to_rfc3339(),
                day.succ_opt()
                    .ok_or("next day")?
                    .and_hms_opt(0, 0, 0)
                    .ok_or("midnight")?
                    .and_utc()
                    .to_rfc3339()
            ),
        )
        .await?,
    )?;
    if read != [Some(pass.clone())] {
        return Err(format!("the frozen day read {read:?} for {pass}").into());
    }

    // Redacted capture stores no `actual`.
    let ungated = run_of(&runs, "eval-ungated", &pass)?;
    let actual = texts(
        &query(
            &server,
            tenant,
            format!(
                "SELECT actual FROM vala.eval.result_items WHERE result_id = '{}'",
                ungated.state.result_id.ok_or("no result")?
            ),
        )
        .await?,
    )?;
    if actual != [None] {
        return Err(format!("redacted capture stored {actual:?}").into());
    }

    // The judge received the image natively, never the private URI, and only
    // for the `pass` and `fail` records: no refused media reached it.
    let encoded = base64::engine::general_purpose::STANDARD.encode(IMAGE);
    let bodies: Vec<String> = provider
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .map(|request| String::from_utf8_lossy(&request.body).into_owned())
        .collect();
    if bodies.len() != 2
        || bodies.iter().any(|body| {
            !body.contains(&encoded) || body.contains("file://") || body.contains("image/tiff")
        })
    {
        return Err(format!("the judge did not receive native image content: {bodies:?}").into());
    }

    // Trace lifecycle: one trace lands after the run first awaits it, the
    // other never lands and times out without a result.
    let state = start_state(&bundle, &client).await;
    let run = state.run();
    let traced_view = run.for_card("traced")?;
    emit(
        &traced_view,
        &json!({ "marker": "landed" }),
        None,
        Some(LANDED_TRACE),
    );
    emit(
        &traced_view,
        &json!({ "marker": "never" }),
        None,
        Some(MISSING_TRACE),
    );
    state.shutdown().await?;
    server.flush_bifrost().await?;
    let (stop, task) = spawn_runtime(&server, &provider.uri());
    let landed = record_id(&server, tenant, "landed").await?;
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let runs = seed.observation_runs().await?;
        if run_of(&runs, "eval-traced", &landed)
            .is_ok_and(|run| run.state.error_code.as_deref() == Some("eval_awaiting_trace"))
        {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("the landed-trace run never awaited its trace: {runs:?}").into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let token = server
        .exchange_api_key(&secrecy::SecretString::from(admin.clone()))
        .await?;
    export_span(&server, &token, LANDED_TRACE).await;
    let runs = settle(&seed, queued + 2).await?;
    stop.cancel();
    tokio::time::timeout(WAIT, task).await??;
    server.flush_bifrost().await?;
    let never = record_id(&server, tenant, "never").await?;
    assert_completed(
        &server,
        tenant,
        run_of(&runs, "eval-traced", &landed)?,
        "passed",
        (2, 0),
        0,
    )
    .await?;
    assert_unresulted(
        &server,
        tenant,
        run_of(&runs, "eval-traced", &never)?,
        "timed_out",
    )
    .await?;
    server.shutdown().await?;
    Ok(())
}

/// Fully-qualified name of the fixed Eval input table.
const OBSERVATIONS: &str = "vala.eval.observations";
/// `observations_ready` Eval bindings whose subject is the `agent` Card.
const AGENT_BINDINGS: usize = 4;

/// One Eval observation frame for `subject`: the SDK's exact projection plus
/// correlation, its `wyrd_event_time` stamped once at `at` as the SDK stamps an
/// emit, so every replay of the frame carries the same event time.
///
/// # Panics
/// Panics when the row does not match the fixed table projection.
fn answered_observation(
    subject: &wyrd_spec::reference::CardRef,
    record: &str,
    at: chrono::DateTime<chrono::Utc>,
) -> Vec<u8> {
    use vala_bifrost_redux::tables::DomainTable as _;
    let schema = Arc::new(arrow::datatypes::Schema::new(
        vala_bifrost_redux::tables::EvalObservationsTable::arrow_fields(),
    ));
    let mut builder = wyrd_queue::BatchBuilder::new(schema);
    builder
        .append_json_row(
            &json!({
                "record_id": record,
                "context": json!({ "answer": "yes" }).to_string(),
                "created_at": at.to_rfc3339(),
            })
            .to_string(),
            Some(subject),
            None,
            Some(at.timestamp_micros()),
        )
        .expect("the observation row matches the fixed projection");
    builder.finish_ipc().expect("the observation frame encodes")
}

/// A sealed replay of an observation on a later receipt day is acknowledged
/// but never activates runs: only the attempt that committed the batch
/// enqueues, so every run freezes the stored row's client-stamped
/// `wyrd_event_time` and the row reads back from its original day.
///
/// Run inserts are held behind a table lock while the original, two concurrent
/// replays one day later, and a distinct sentinel frame are acknowledged.
/// Gate stages run requests before each acknowledgement returns, so once the
/// sentinel is acknowledged the run-request outbox must hold exactly two
/// requests (original and sentinel); a staged replay would add to them.
///
/// # Errors
/// Returns server, registration, query, or fixture errors, or a description of
/// the first mismatch.
///
/// # Panics
///
/// Panics only if `#[tokio::test]` cannot build its runtime; every
/// expectation failure is returned as an error instead.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn sealed_replay_on_a_later_day_activates_once() -> Result<(), ServerJourneyError> {
    let root = tempfile::tempdir()?;
    let service = write_graph(root.path());
    let bundle = root.path().join("bundle");
    let server = Box::pin(WyrdTestServer::start_bound()).await?;
    let tenant = server.data_tenant_id();
    let seed = VerificationFixture::provision(server.state().postgres.wyrd(), tenant).await?;
    let admin = api_key(
        server
            .bootstrap_service("eval_replay_admin", &["admin"])
            .await?,
    );
    let receipt = register(&connect(&server, &admin), &service, &bundle).await;
    let writer = api_key(
        server
            .credential_registered_service(&receipt.root, &[])
            .await?,
    );
    let client = connect(&server, &writer);
    let state = start_state(&bundle, &client).await;
    let subject = state.run().for_card("agent")?.subject().clone();
    state.shutdown().await?;
    let scribe = server.bifrost_scribe().ok_or("the server owns no Scribe")?;
    let ingest = wyrd_testing::bifrost::write::RawIngest::connect(&client).await?;

    let record = uuid::Uuid::now_v7().to_string();
    let batch = uuid::Uuid::now_v7();
    let superuser = server.pg_fixture().superuser_pool()?;
    let mut lock = superuser.begin().await?;
    sqlx::query("LOCK TABLE wyrd.verifier_runs IN SHARE MODE")
        .execute(&mut *lock)
        .await?;

    let emitted = chrono::Utc::now();
    let frame = answered_observation(&subject, &record, emitted);
    ingest.insert(OBSERVATIONS, batch, frame.clone()).await?;
    scribe.shift_receipt_clock_for_test(Duration::from_hours(24));
    let (first, second) = tokio::join!(
        ingest.insert(OBSERVATIONS, batch, frame.clone()),
        ingest.insert(OBSERVATIONS, batch, frame),
    );
    first?;
    second?;
    let sentinel = uuid::Uuid::now_v7().to_string();
    ingest
        .insert(
            OBSERVATIONS,
            uuid::Uuid::now_v7(),
            answered_observation(&subject, &sentinel, chrono::Utc::now()),
        )
        .await?;
    // Gate stages run requests before each acknowledgement returns, and the
    // held lock keeps the original's write in flight, so the outbox now holds
    // exactly the original and the sentinel; a staged replay would add to it.
    let activations = server
        .state()
        .bifrost
        .observation_runs()
        .ok_or("the server owns no run-request outbox")?
        .pending();
    scribe.shift_receipt_clock_for_test(Duration::ZERO);
    lock.commit().await?;
    if activations != 2 {
        return Err(format!("{activations} run requests for one original and one sentinel").into());
    }

    // Four `observations_ready` bindings observe `agent`; each frame
    // enqueues all of its runs in one transaction.
    enqueued(&seed, 2 * AGENT_BINDINGS).await?;
    let runs = seed.observation_runs().await?;
    server.flush_bifrost().await?;
    let stored = texts(
        &query(
            &server,
            tenant,
            format!(
                "SELECT CAST(wyrd_event_time AS BIGINT) FROM vala.eval.observations \
                 WHERE record_id = '{record}'"
            ),
        )
        .await?,
    )?;
    let [Some(stored)] = stored.as_slice() else {
        return Err(format!("the replayed batch is not stored once: {stored:?}").into());
    };
    let stored = chrono::DateTime::from_timestamp_micros(stored.parse()?)
        .ok_or("the stored event time is out of range")?;
    if stored.timestamp_micros() != emitted.timestamp_micros() {
        return Err(format!("the row holds {stored}, not its emit time {emitted}").into());
    }
    let frozen: Vec<&ObservationRun> = runs.iter().filter(|run| run.record_id == record).collect();
    let sentinel_runs = runs.iter().filter(|run| run.record_id == sentinel).count();
    if runs.len() != 2 * AGENT_BINDINGS
        || frozen.len() != AGENT_BINDINGS
        || sentinel_runs != AGENT_BINDINGS
        || frozen.iter().any(|run| run.event_time != stored)
    {
        return Err(format!("expected one run per binding frozen at {stored}: {runs:?}").into());
    }
    let day = stored.date_naive();
    let original_day = texts(
        &query(
            &server,
            tenant,
            format!(
                "SELECT record_id FROM vala.eval.observations WHERE record_id = '{record}' \
                 AND wyrd_event_time >= '{}' AND wyrd_event_time < '{}'",
                day.and_hms_opt(0, 0, 0)
                    .ok_or("midnight")?
                    .and_utc()
                    .to_rfc3339(),
                day.succ_opt()
                    .ok_or("next day")?
                    .and_hms_opt(0, 0, 0)
                    .ok_or("midnight")?
                    .and_utc()
                    .to_rfc3339()
            ),
        )
        .await?,
    )?;
    if original_day != [Some(record.clone())] {
        return Err(format!("the original day read {original_day:?}").into());
    }
    server.shutdown().await?;
    Ok(())
}

/// Poll until the refusing trigger has counted `count` enqueue attempts.
///
/// The trigger advances `wyrd.eval_journey_attempts` before it raises, and a
/// sequence advance survives the aborted transaction, so the sequence counts
/// every enqueue transaction that reached `wyrd.verifier_runs`.
///
/// # Errors
/// Returns a query error or a timeout naming the observed count.
async fn enqueue_attempts(superuser: &sqlx::PgPool, count: i64) -> Result<i64, ServerJourneyError> {
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let attempts: i64 = sqlx::query_scalar(
            "SELECT CASE WHEN is_called THEN last_value ELSE 0 END \
               FROM wyrd.eval_journey_attempts",
        )
        .fetch_one(superuser)
        .await?;
        if attempts >= count {
            return Ok(attempts);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("only {attempts} of {count} enqueue attempts ran").into());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// A post-ACK run-creation outage through the integrated Gate, Scribe, and
/// Eval path keeps the acknowledged observation readable and invents no run
/// or result while it lasts; the run-request outbox retries, and once
/// PostgreSQL accepts the writes every acknowledged record has exactly one run
/// per binding, a same-batch-ID replay adding none.
///
/// A trigger refuses every observation run insert after counting the attempt
/// in a sequence, so refused writes, and the retries that follow, are observed
/// before the trigger is dropped.
///
/// # Errors
/// Returns server, registration, query, or fixture errors, or a description of
/// the first mismatch.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn integrated_enqueue_outage_preserves_ack_and_recovers() -> Result<(), ServerJourneyError> {
    let root = tempfile::tempdir()?;
    let service = write_graph(root.path());
    let bundle = root.path().join("bundle");
    let server = Box::pin(WyrdTestServer::start_bound()).await?;
    let tenant = server.data_tenant_id();
    let seed = VerificationFixture::provision(server.state().postgres.wyrd(), tenant).await?;
    let admin = api_key(
        server
            .bootstrap_service("eval_enqueue_admin", &["admin"])
            .await?,
    );
    let receipt = register(&connect(&server, &admin), &service, &bundle).await;
    let writer = api_key(
        server
            .credential_registered_service(&receipt.root, &[])
            .await?,
    );
    let client = connect(&server, &writer);
    let state = start_state(&bundle, &client).await;
    let subject = state.run().for_card("agent")?.subject().clone();
    state.shutdown().await?;
    let ingest = wyrd_testing::bifrost::write::RawIngest::connect(&client).await?;

    let superuser = server.pg_fixture().superuser_pool()?;
    sqlx::query("CREATE SEQUENCE wyrd.eval_journey_attempts")
        .execute(&superuser)
        .await?;
    sqlx::query(
        "CREATE FUNCTION wyrd.eval_journey_refuse() RETURNS trigger LANGUAGE plpgsql AS \
         $$BEGIN PERFORM nextval('wyrd.eval_journey_attempts'); \
         RAISE EXCEPTION 'eval journey refuses this enqueue'; END$$",
    )
    .execute(&superuser)
    .await?;
    sqlx::query(
        "CREATE TRIGGER eval_journey_refuse BEFORE INSERT ON wyrd.verifier_runs \
         FOR EACH ROW WHEN (NEW.origin = 'observation') \
         EXECUTE FUNCTION wyrd.eval_journey_refuse()",
    )
    .execute(&superuser)
    .await?;

    let record = uuid::Uuid::now_v7().to_string();
    let batch = uuid::Uuid::now_v7();
    let frame = answered_observation(&subject, &record, chrono::Utc::now());
    ingest.insert(OBSERVATIONS, batch, frame.clone()).await?;
    enqueue_attempts(&superuser, 1).await?;
    ingest.insert(OBSERVATIONS, batch, frame).await?;
    let sentinel = uuid::Uuid::now_v7().to_string();
    ingest
        .insert(
            OBSERVATIONS,
            uuid::Uuid::now_v7(),
            answered_observation(&subject, &sentinel, chrono::Utc::now()),
        )
        .await?;
    // The outbox keeps retrying the refused requests.
    enqueue_attempts(&superuser, 3).await?;

    server.flush_bifrost().await?;
    let stored = texts(
        &query(
            &server,
            tenant,
            format!("SELECT record_id FROM vala.eval.observations WHERE record_id = '{record}'"),
        )
        .await?,
    )?;
    if stored != [Some(record.clone())] {
        return Err(format!("the acknowledged batch is not stored once: {stored:?}").into());
    }
    let runs = seed.observation_runs().await?;
    if !runs.is_empty() {
        return Err(format!("a refused enqueue invented runs: {runs:?}").into());
    }
    sqlx::query("DROP TRIGGER eval_journey_refuse ON wyrd.verifier_runs")
        .execute(&superuser)
        .await?;
    let deadline = tokio::time::Instant::now() + WAIT;
    let runs = loop {
        let runs = seed.observation_runs().await?;
        if runs.len() >= 2 * AGENT_BINDINGS {
            break runs;
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("the outbox never recovered: {runs:?}").into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    for id in [&record, &sentinel] {
        let made = runs.iter().filter(|run| &run.record_id == id).count();
        if made != AGENT_BINDINGS {
            return Err(format!("record {id} has {made} runs: {runs:?}").into());
        }
    }
    // No result ever published leaves the results table unregistered.
    let results = ScheduledQueryCaller::new(
        server.state().clone(),
        scheduled_context(tenant)?,
        CancellationToken::new(),
    )
    .run(BifrostQueryRequest {
        params: Vec::new(),
        sql: "SELECT result_id FROM vala.verification.results".to_owned(),
        deadline_ms: Some(30_000),
    })
    .await;
    let results = match results {
        Ok(outcome) => outcome.rows,
        Err(wyrd_spec::error::WyrdError::Vala {
            error: wyrd_spec::vala::error::BifrostError::TableNotFound { .. },
        }) => 0,
        Err(error) => return Err(error.into()),
    };
    if results != 0 {
        return Err(format!("run creation alone invented {results} results").into());
    }
    server.shutdown().await?;
    Ok(())
}

/// One UTC day, the receipt-clock step the matrix and trace-window journeys
/// move by.
const DAY: Duration = Duration::from_hours(24);
/// Trace whose committed spans the evidence Verifier asserts over.
const EVIDENCE_TRACE: &str = "7d0a111920253035a5b5c5d5e5f50515";
/// Trace the evidence trace's first span links to.
const LINKED_TRACE: &str = "8e1b222a31364146b6c6d6e6f6061626";
/// Span the evidence trace's first span links to.
const LINKED_SPAN: &str = "2b3c4d5e6f708192";
/// Trace holding exactly the server's span ceiling.
const CEILING_TRACE: &str = "9f2c333b42475257c7d7e7f707172737";
/// Trace holding one span more than the server's span ceiling.
const OVERFLOW_TRACE: &str = "a03d444c53586368d8e8f80818283848";
/// Fixed span start instant of the evidence trace, in UNIX nanoseconds.
const EVIDENCE_START: u64 = 1_700_000_000_000_000_000;

/// Write the trace Service graph beside [`write_graph`]'s Cards: the traced
/// Agent is verified by `eval-trace-evidence`, whose trace tasks select span
/// order, a persisted event, and a persisted link; the plain Agent is
/// verified by `eval-trace-ceiling`, which pairs one trace task with the
/// media judge. Returns the Service path.
///
/// # Panics
/// Panics when a file cannot be written.
fn write_trace_graph(root: &Path) -> PathBuf {
    write_graph(root);
    let task = |id: &str, selector: &str, expected: &str| {
        format!(
            "        {id}: {{kind: trace_assertion, id: {id}, span_selector: \"{selector}\", operator: equals, expected: {expected}}}\n"
        )
    };
    let evidence = [
        task("first", "$.spans[0].name", "first-span"),
        task("second", "$.spans[1].name", "second-span"),
        task("event", "$.spans[0].events[0].name", "retrieval"),
        task("event_doc", "$.spans[0].events[0].attributes.doc", "d1"),
        task(
            "link",
            "$.spans[0].links[0].span_id",
            &format!("\"{LINKED_SPAN}\""),
        ),
        task(
            "link_dropped",
            "$.spans[0].links[0].dropped_attributes_count",
            "2",
        ),
    ]
    .concat();
    let ceiling = format!(
        "{}        judge:\n          kind: llm_judge\n          id: judge\n          judge_ref: {{prompt: ./judge-prompt.json, tool_names: [], run_config: {{max_iterations: 1}}}}\n          context_path: $.answer\n          operator: equals\n          expected: {{passed: true}}\n          max_retries: 0\n",
        task("first", "$.spans[0].name", "span-0")
    );
    let files = [
        (
            "evidence.yaml",
            verifier(
                "eval-trace-evidence",
                &format!("      pass_gate: {{kind: all_pass}}\n      tasks:\n{evidence}"),
            ),
        ),
        (
            "ceiling.yaml",
            verifier(
                "eval-trace-ceiling",
                &format!("      pass_gate: {{kind: all_pass}}\n      tasks:\n{ceiling}"),
            ),
        ),
        (
            "trace-service.yaml",
            "apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: eval-trace-service\n  version: 1.0.0\n  space: default\nspec:\n  service_type: agent\n  components:\n    - alias: traced\n      ref: ./traced-agent.yaml\n      verified_by:\n        - verifier: ./evidence.yaml\n          runs_on: {kind: observations_ready}\n    - alias: agent\n      ref: ./agent.yaml\n      verified_by:\n        - verifier: ./ceiling.yaml\n          runs_on: {kind: observations_ready}\n".to_owned(),
        ),
    ];
    for (name, body) in files {
        std::fs::write(root.join(name), body).expect("fixture file writes");
    }
    root.join("trace-service.yaml")
}

/// One bound server with the trace Service graph registered and hydrated.
struct TraceJourney {
    /// Directory holding the graph files and the hydrated bundle; removed on drop.
    _root: tempfile::TempDir,
    /// The bound server.
    server: WyrdTestServer,
    /// The server's data tenant.
    tenant: DataTenantId,
    /// Verification fixture reading the tenant's runs.
    seed: VerificationFixture,
    /// The hydrated bundle an SDK lifetime starts from.
    bundle: PathBuf,
    /// Client authenticated as the registered Service.
    client: WyrdClient,
    /// Bearer token of the admin principal that exports spans.
    token: String,
}

impl TraceJourney {
    /// Boot a bound server and register [`write_trace_graph`] through the SDK.
    ///
    /// # Errors
    /// Returns server, registration, credential, or fixture errors.
    async fn start() -> Result<Self, ServerJourneyError> {
        let root = tempfile::tempdir()?;
        let service = write_trace_graph(root.path());
        let bundle = root.path().join("bundle");
        let server = Box::pin(WyrdTestServer::start_bound()).await?;
        let tenant = server.data_tenant_id();
        let seed = VerificationFixture::provision(server.state().postgres.wyrd(), tenant).await?;
        let admin = api_key(
            server
                .bootstrap_service("eval_trace_admin", &["admin"])
                .await?,
        );
        let receipt = register(&connect(&server, &admin), &service, &bundle).await;
        let writer = api_key(
            server
                .credential_registered_service(&receipt.root, &[])
                .await?,
        );
        let client = connect(&server, &writer);
        let token = server
            .exchange_api_key(&secrecy::SecretString::from(admin))
            .await?;
        Ok(Self {
            _root: root,
            server,
            tenant,
            seed,
            bundle,
            client,
            token,
        })
    }

    /// Emit one Eval record per `(context, media)` pair from the `alias`
    /// component under `trace`, then flush it to committed storage.
    ///
    /// # Errors
    /// Returns an SDK lifetime or flush error.
    async fn emit(
        &self,
        alias: &str,
        trace: &str,
        records: &[(Value, Option<&str>)],
    ) -> Result<(), ServerJourneyError> {
        let state = start_state(&self.bundle, &self.client).await;
        let view = state.run().for_card(alias)?;
        for (context, media) in records {
            emit(&view, context, *media, Some(trace));
        }
        state.shutdown().await?;
        self.server.flush_bifrost().await?;
        Ok(())
    }

    /// Run a fresh verification runtime until `count` observation runs are
    /// terminal, then stop it.
    ///
    /// # Errors
    /// Returns a settle timeout or the runtime task's failure.
    async fn run_to(
        &self,
        provider: &str,
        count: usize,
    ) -> Result<Vec<ObservationRun>, ServerJourneyError> {
        let (stop, task) = spawn_runtime(&self.server, provider);
        let runs = settle(&self.seed, count).await;
        stop.cancel();
        tokio::time::timeout(WAIT, task).await??;
        self.server.flush_bifrost().await?;
        runs
    }

    /// Every canonical result item of `result` as `task_id=actual`, sorted.
    ///
    /// # Errors
    /// Returns the query error.
    async fn items(&self, result: Option<uuid::Uuid>) -> Result<Vec<String>, ServerJourneyError> {
        let result = result.ok_or("the run has no result")?;
        let mut items: Vec<String> = texts(
            &query(
                &self.server,
                self.tenant,
                format!(
                    "SELECT task_id || '=' || actual FROM vala.eval.result_items \
                     WHERE result_id = '{result}'"
                ),
            )
            .await?,
        )?
        .into_iter()
        .map(Option::unwrap_or_default)
        .collect();
        items.sort();
        Ok(items)
    }
}

/// One span of `trace` named `name` starting `offset_ms` after
/// [`EVIDENCE_START`], with the given ordinal as its span ID.
fn span_at(trace: &str, name: &str, ordinal: u64, offset_ms: i64) -> Span {
    let start = EVIDENCE_START.saturating_add_signed(offset_ms * 1_000_000);
    Span {
        trace_id: hex::decode(trace).expect("the trace fixture is hex"),
        span_id: (ordinal + 1).to_be_bytes().to_vec(),
        name: name.to_owned(),
        kind: 1,
        start_time_unix_nano: start,
        end_time_unix_nano: start + 1_000_000,
        ..Span::default()
    }
}

/// One Eval observation frame for `subject` under `trace` whose caller-owned
/// `wyrd_event_time` is `at`, so Scribe commits it on that day instead of the
/// receipt day.
///
/// # Panics
/// Panics when the row does not match the fixed table projection.
fn stamped_observation(
    subject: &wyrd_spec::reference::CardRef,
    record: &str,
    trace: &str,
    at: chrono::DateTime<chrono::Utc>,
) -> Vec<u8> {
    use vala_bifrost_redux::tables::DomainTable as _;
    let schema = Arc::new(arrow::datatypes::Schema::new(
        vala_bifrost_redux::tables::EvalObservationsTable::arrow_fields(),
    ));
    let mut builder = wyrd_queue::BatchBuilder::new(schema);
    builder
        .append_json_row(
            &json!({
                "record_id": record,
                "context": json!({ "marker": record }).to_string(),
                "trace_id": trace,
                "created_at": at.to_rfc3339(),
            })
            .to_string(),
            Some(subject),
            None,
            Some(at.timestamp_micros()),
        )
        .expect("the observation row matches the fixed projection");
    builder.finish_ipc().expect("the observation frame encodes")
}

/// The principal ID of every retained Oracle read decision `tenant` made as
/// a System principal, in decision order, once its audit has published.
///
/// Only continuous Eval reads as the System principal; the journey's own
/// inspection reads run as users and are excluded.
///
/// # Errors
/// Returns the publication or retained-history failure.
async fn system_reads(
    server: &WyrdTestServer,
    tenant: DataTenantId,
) -> Result<Vec<String>, ServerJourneyError> {
    server.await_audit_retained().await?;
    Ok(server
        .retained_audit_records(
            tenant,
            "audit_principal_id",
            "operation = 'bifrost.query.read_decision' AND principal_kind = 'system'",
        )
        .await?
        .into_iter()
        .map(|record| record.into_iter().flatten().collect())
        .collect())
}

/// Continuous Eval reads a committed trace with its persisted events, links,
/// and dropped counts, in one total start-time order, bounded on both sides
/// to the record's trace window.
///
/// Both spans land on today's receipt day, exported in reverse start order.
/// A record received today reads them; a record committed five days ago
/// (spans after its window) and one stamped three days ahead (spans before
/// its window) read none and time out awaiting their trace. A second
/// in-window record over the same trace, scored by a restarted runtime,
/// produces identical canonical items. Every input read before and after the
/// restart is audited as the tenant's one stored System principal.
///
/// # Errors
/// Returns server, registration, query, or fixture errors, or a description of
/// the first mismatch.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn continuous_eval_reads_ordered_bounded_trace_evidence() -> Result<(), ServerJourneyError> {
    let journey = TraceJourney::start().await?;
    let mut first = span_at(EVIDENCE_TRACE, "first-span", 1, 0);
    first.events = vec![wyrd_tonic::otlp::trace::v1::span::Event {
        time_unix_nano: EVIDENCE_START + 500,
        name: "retrieval".to_owned(),
        attributes: vec![text_attribute("doc", "d1")],
        dropped_attributes_count: 1,
    }];
    first.dropped_events_count = 3;
    first.links = vec![wyrd_tonic::otlp::trace::v1::span::Link {
        trace_id: hex::decode(LINKED_TRACE)?,
        span_id: hex::decode(LINKED_SPAN)?,
        attributes: vec![text_attribute("kind", "follows")],
        dropped_attributes_count: 2,
        ..Default::default()
    }];
    first.dropped_links_count = 4;
    export_spans(
        &journey.server,
        &journey.token,
        vec![span_at(EVIDENCE_TRACE, "second-span", 2, 2), first],
    )
    .await;
    journey
        .emit(
            "traced",
            EVIDENCE_TRACE,
            &[(json!({ "marker": "evidence-1" }), None)],
        )
        .await?;
    let state = start_state(&journey.bundle, &journey.client).await;
    let subject = state.run().for_card("traced")?.subject().clone();
    state.shutdown().await?;
    let past = uuid::Uuid::now_v7().to_string();
    let ingest = wyrd_testing::bifrost::write::RawIngest::connect(&journey.client).await?;
    ingest
        .insert(
            OBSERVATIONS,
            uuid::Uuid::now_v7(),
            stamped_observation(
                &subject,
                &past,
                EVIDENCE_TRACE,
                chrono::Utc::now() - chrono::Duration::days(5),
            ),
        )
        .await?;
    let scribe = journey
        .server
        .bifrost_scribe()
        .ok_or("the server owns no Scribe")?;
    // The SDK stamps its own clock, so the future record is a raw frame stamped
    // three days ahead; the shifted receipt keeps it inside Scribe's window.
    let future = uuid::Uuid::now_v7().to_string();
    scribe.shift_receipt_clock_for_test(3 * DAY);
    ingest
        .insert(
            OBSERVATIONS,
            uuid::Uuid::now_v7(),
            stamped_observation(
                &subject,
                &future,
                EVIDENCE_TRACE,
                chrono::Utc::now() + chrono::Duration::days(3),
            ),
        )
        .await?;
    scribe.shift_receipt_clock_for_test(Duration::ZERO);

    let provider = MockServer::start().await;
    let runs = journey.run_to(&provider.uri(), 3).await?;
    let evidence = record_id(&journey.server, journey.tenant, "evidence-1").await?;
    for record in [&past, &future] {
        let outside = run_of(&runs, "eval-trace-evidence", record)?;
        assert_unresulted(&journey.server, journey.tenant, outside, "timed_out").await?;
    }
    let run = run_of(&runs, "eval-trace-evidence", &evidence)?;
    assert_completed(&journey.server, journey.tenant, run, "passed", (6, 0), 0).await?;
    let items = journey.items(run.state.result_id).await?;
    let expected = [
        "event=\"retrieval\"".to_owned(),
        "event_doc=\"d1\"".to_owned(),
        "first=\"first-span\"".to_owned(),
        format!("link=\"{LINKED_SPAN}\""),
        "link_dropped=2".to_owned(),
        "second=\"second-span\"".to_owned(),
    ];
    if items != expected {
        return Err(format!("canonical items {items:?}, expected {expected:?}").into());
    }
    let system = journey.seed.system_principal().to_string();
    let first_reads = system_reads(&journey.server, journey.tenant).await?;
    if first_reads.is_empty() || first_reads.iter().any(|id| *id != system) {
        return Err(format!(
            "Eval input reads must be audited as the tenant System principal {system}: \
             {first_reads:?}"
        )
        .into());
    }

    // A restarted runtime reads the same committed trace for a new record.
    journey
        .emit(
            "traced",
            EVIDENCE_TRACE,
            &[(json!({ "marker": "evidence-2" }), None)],
        )
        .await?;
    let runs = journey.run_to(&provider.uri(), 4).await?;
    let again = record_id(&journey.server, journey.tenant, "evidence-2").await?;
    let rerun = run_of(&runs, "eval-trace-evidence", &again)?;
    assert_completed(&journey.server, journey.tenant, rerun, "passed", (6, 0), 0).await?;
    let reread = journey.items(rerun.state.result_id).await?;
    if reread != items {
        return Err(format!("the restarted read produced {reread:?}, not {items:?}").into());
    }
    let reads = system_reads(&journey.server, journey.tenant).await?;
    if reads.len() <= first_reads.len() || reads.iter().any(|id| *id != system) {
        return Err(format!(
            "the restarted runtime must read as the same System principal {system}: \
             {reads:?} after {first_reads:?}"
        )
        .into());
    }
    journey.server.shutdown().await?;
    Ok(())
}

/// A trace of exactly the server's span ceiling decodes and is judged; one
/// span more is a trace-source failure that retries to `errored` before any
/// task or provider call.
///
/// # Errors
/// Returns server, registration, query, or fixture errors, or a description of
/// the first mismatch.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn continuous_eval_refuses_a_trace_over_the_span_ceiling() -> Result<(), ServerJourneyError> {
    let journey = TraceJourney::start().await?;
    let limit = wyrd_server::verification::eval::TRACE_SPAN_LIMIT;
    for (trace, count) in [(CEILING_TRACE, limit), (OVERFLOW_TRACE, limit + 1)] {
        let spans: Vec<Span> = (0..count)
            .map(|index| {
                let ordinal = u64::try_from(index).expect("span index fits u64");
                let offset = i64::try_from(index).expect("span index fits i64");
                span_at(trace, &format!("span-{index}"), ordinal, offset)
            })
            .collect();
        for chunk in spans.chunks(2_000) {
            export_spans(&journey.server, &journey.token, chunk.to_vec()).await;
        }
    }
    let state = start_state(&journey.bundle, &journey.client).await;
    let agent_uid = state
        .run()
        .for_card("agent")?
        .subject()
        .uid
        .clone()
        .ok_or("agent has no UID")?;
    state.shutdown().await?;
    let object = format!("{}/cards/{agent_uid}/shot.png", journey.tenant);
    journey
        .server
        .state()
        .storage
        .put_object(
            &tenant_path::validate(&object, journey.tenant)?,
            IMAGE.to_vec(),
        )
        .await?;
    let shot = media(&format!("file:///{object}"));
    journey
        .emit(
            "agent",
            CEILING_TRACE,
            &[(json!({ "answer": "yes", "marker": "ceiling" }), Some(&shot))],
        )
        .await?;
    journey
        .emit(
            "agent",
            OVERFLOW_TRACE,
            &[(
                json!({ "answer": "yes", "marker": "overflow" }),
                Some(&shot),
            )],
        )
        .await?;

    let provider = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "chatcmpl_eval", "object": "chat.completion", "created": 1_700_000_000,
            "model": "gpt-test",
            "choices": [{ "index": 0, "finish_reason": "stop",
                "message": { "role": "assistant", "content": "{\"passed\":true}" } }],
            "usage": { "prompt_tokens": 5, "completion_tokens": 3, "total_tokens": 8 }
        })))
        .mount(&provider)
        .await;
    let runs = journey.run_to(&provider.uri(), 2).await?;
    let ceiling = record_id(&journey.server, journey.tenant, "ceiling").await?;
    let overflow = record_id(&journey.server, journey.tenant, "overflow").await?;
    assert_completed(
        &journey.server,
        journey.tenant,
        run_of(&runs, "eval-trace-ceiling", &ceiling)?,
        "passed",
        (2, 0),
        0,
    )
    .await?;
    let refused = run_of(&runs, "eval-trace-ceiling", &overflow)?;
    assert_unresulted(&journey.server, journey.tenant, refused, "errored").await?;
    if refused.state.error_code.as_deref() != Some("eval_trace_unavailable") {
        return Err(format!("the overflow is not a trace-source failure: {refused:?}").into());
    }
    let calls = provider.received_requests().await.unwrap_or_default().len();
    if calls != 1 {
        return Err(format!("the provider saw {calls} calls; only the ceiling run judges").into());
    }
    journey.server.shutdown().await?;
    Ok(())
}

/// Move the tenant's System principal out of (`present = false`) or back
/// into (`present = true`) the System kind the Eval read authority resolves.
///
/// The schema keeps a System principal permanently active, so a journey
/// models its absence by relabelling the stored row rather than disabling it.
///
/// # Errors
/// Returns the Postgres failure.
async fn set_system_principal(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    present: bool,
) -> Result<(), ServerJourneyError> {
    let (from, to) = if present {
        ("service", "system")
    } else {
        ("system", "service")
    };
    let mut conn = server.state().postgres.tenant_conn(tenant).await?;
    sqlx::query(
        "UPDATE wyrd.auth_service_accounts SET principal_kind = $2 \
         WHERE principal_kind = $1 AND name = 'verification-results-writer'",
    )
    .bind(from)
    .bind(to)
    .execute(&mut **conn.transaction())
    .await?;
    conn.commit().await?;
    Ok(())
}

/// Run `sql` under `context` and return its row count, or its error.
async fn read_as(
    server: &WyrdTestServer,
    context: vala_bifrost_redux::oracle::AuthorizedQueryContext,
    sql: &str,
) -> Result<u64, wyrd_spec::error::WyrdError> {
    ScheduledQueryCaller::new(server.state().clone(), context, CancellationToken::new())
        .run(BifrostQueryRequest {
            params: Vec::new(),
            sql: sql.to_owned(),
            deadline_ms: Some(30_000),
        })
        .await
        .map(|outcome| outcome.rows)
}

/// Whether `outcome` is Oracle's object denial.
fn forbidden(outcome: &Result<u64, wyrd_spec::error::WyrdError>) -> bool {
    matches!(
        outcome,
        Err(wyrd_spec::error::WyrdError::Vala {
            error: wyrd_spec::vala::error::BifrostError::QueryForbidden,
        })
    )
}

/// Continuous Eval reads only under the tenant's stored System principal and
/// its tokenless Eval input scope, and fails closed without it.
///
/// With the System principal absent, the claim returns none and the run's
/// record read is refused before any Oracle read: the run errors
/// `eval_record_unavailable` with no result and no System read decision.
/// Restored, the authority names that stored principal with exactly one
/// table-scoped read grant per Eval input table and no role, credential, or
/// Verifier scope. It reads the committed observation, but Oracle refuses
/// (and audits as `denied` for the same principal) a table outside the scope
/// and an input table the scope was narrowed away from. A context presenting
/// it for another tenant is refused before Oracle.
///
/// # Errors
/// Returns server, registration, query, or fixture errors, or a description of
/// the first mismatch.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn continuous_eval_read_authority_fails_closed() -> Result<(), ServerJourneyError> {
    use vala_bifrost_redux::catalog::TableRef;
    use vala_bifrost_redux::namespaces::BifrostNamespace;
    use wyrd_server::verification::authority::{SystemReadAuthority, SystemReadAuthorityError};

    let journey = TraceJourney::start().await?;
    let (server, tenant) = (&journey.server, journey.tenant);
    let system = journey.seed.system_principal();
    let inputs = [
        TableRef::new(BifrostNamespace::Eval, "observations"),
        TableRef::new(BifrostNamespace::Traces, "spans"),
    ];
    export_span(server, &journey.token, LANDED_TRACE).await;
    journey
        .emit(
            "traced",
            LANDED_TRACE,
            &[(json!({ "marker": "authority" }), None)],
        )
        .await?;

    // Missing: no System principal, no read.
    set_system_principal(server, tenant, false).await?;
    let missing = SystemReadAuthority::resolve(server.state(), tenant, None, &inputs).await;
    if !matches!(
        missing,
        Err(SystemReadAuthorityError::SystemPrincipalMissing)
    ) {
        return Err(format!("a tenant without a System principal resolved {missing:?}").into());
    }
    let provider = MockServer::start().await;
    let runs = journey.run_to(&provider.uri(), 1).await?;
    let record = record_id(server, tenant, "authority").await?;
    let refused = run_of(&runs, "eval-trace-evidence", &record)?;
    // No run has published yet, so the results table does not even exist.
    if refused.state.status != "errored"
        || refused.state.result_id.is_some()
        || refused.dispatches != 0
        || refused.state.error_code.as_deref() != Some("eval_record_unavailable")
    {
        return Err(format!("the unauthorized read is not a record failure: {refused:?}").into());
    }
    let reads = system_reads(server, tenant).await?;
    if !reads.is_empty() {
        return Err(format!("no System read may happen without its principal: {reads:?}").into());
    }
    set_system_principal(server, tenant, true).await?;

    // The resolved authority is the stored principal, narrowly scoped.
    let authority = SystemReadAuthority::resolve(
        server.state(),
        tenant,
        Some(wyrd_spec::auth::PrincipalId::new(system)),
        &inputs,
    )
    .await?;
    let principal = &authority.context().principal;
    let table_reads = principal
        .effective_permissions
        .iter()
        .filter(|grant| {
            grant.resource == wyrd_runtime::Resource::BifrostQuery
                && grant.action == wyrd_runtime::Action::Read
                && matches!(
                    grant.scope,
                    wyrd_runtime::PermissionScope::Bifrost(
                        wyrd_runtime::BifrostPermissionScope::Table(_)
                    )
                )
        })
        .count();
    if principal.id.as_uuid() != system
        || principal.kind
            != (wyrd_runtime::PrincipalKind::System {
                card_ref_scope: wyrd_runtime::CardRefScope::default(),
            })
        || !principal.roles.is_empty()
        || principal.credential_id.is_some()
        || principal.effective_permissions.len() != 2
        || table_reads != 2
    {
        return Err(format!(
            "the Eval read authority is not the narrow System read: {principal:?}"
        )
        .into());
    }
    let context = authority.context().clone();
    let rows = read_as(
        server,
        context.clone(),
        &format!("SELECT record_id FROM vala.eval.observations WHERE record_id = '{record}'"),
    )
    .await?;
    if rows != 1 {
        return Err(format!("the System authority read {rows} rows of its own input").into());
    }

    // Under-scoped: outside the scope, and narrowed away from an input.
    server.await_audit_retained().await?;
    let outside = read_as(
        server,
        context.clone(),
        "SELECT seq FROM vala.system.audit_log",
    )
    .await;
    let mut narrowed = context.clone();
    narrowed.principal.effective_permissions = context
        .principal
        .effective_permissions
        .iter()
        .filter(|grant| {
            !matches!(
                &grant.scope,
                wyrd_runtime::PermissionScope::Bifrost(scope) if scope.schema() == "traces"
            )
        })
        .cloned()
        .collect();
    let spans = read_as(
        server,
        narrowed,
        &format!("SELECT span_id FROM vala.traces.spans WHERE trace_id = X'{LANDED_TRACE}'"),
    )
    .await;
    if !forbidden(&outside) || !forbidden(&spans) {
        return Err(format!("under-scoped reads were not refused: {outside:?}, {spans:?}").into());
    }

    // Wrong tenant: refused before Oracle.
    let foreign = DataTenantId::new_v7();
    let crossed = vala_bifrost_redux::oracle::AuthorizedQueryContext::try_new(
        context.principal.clone(),
        foreign,
        context.request_id.clone(),
        None,
        context.auth_method,
        context.permission.clone(),
    );
    if !matches!(
        crossed,
        Err(wyrd_spec::vala::error::BifrostError::QueryTenantInvariant)
    ) {
        return Err(format!("a cross-tenant System context was accepted: {crossed:?}").into());
    }

    // Audit: one allowed read and two denials, all the stored principal.
    let reads = system_reads(server, tenant).await?;
    let denials = server
        .retained_audit_records(
            tenant,
            "audit_principal_id",
            "operation = 'vala.query.sync' AND outcome = 'denied' AND principal_kind = 'system'",
        )
        .await?;
    let expected = Some(system.to_string());
    if reads != [system.to_string()]
        || denials.len() != 2
        || denials
            .iter()
            .any(|record| record.first().cloned().flatten() != expected)
    {
        return Err(format!(
            "expected one allowed read and two denials as {system}: {reads:?}, {denials:?}"
        )
        .into());
    }
    journey.server.shutdown().await?;
    Ok(())
}

/// Text a failing provider returns in its error body.
const PROVIDER_SENTINEL: &str = "PROVIDER-SENTINEL-4f1c";
/// Object name of an in-tenant media URI that names no stored object.
const LOCATOR_SENTINEL: &str = "LOCATOR-SENTINEL-8a2d";
/// Text a Postgres read of the tenant's Cards raises.
const SQL_SENTINEL: &str = "SQL-SENTINEL-c93e";

/// Continuous Eval failures persist and expose only stable codes and fixed
/// operation text, never the dependency detail that caused them.
///
/// A provider that fails with a sentinel body and a media URI whose private
/// object key is a sentinel each settle their gated run `errored` with the
/// stable code. A Postgres read of the Verifier Card that raises a sentinel
/// fails the claim itself, so the run is neither started nor charged an
/// attempt until the read succeeds. The public run status and every persisted
/// `VerificationError` of the tenant carry none of the sentinels.
///
/// # Errors
/// Returns server, registration, query, or fixture errors, or a description of
/// the first leak or mismatch.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn continuous_eval_failures_publish_only_stable_errors() -> Result<(), ServerJourneyError> {
    let root = tempfile::tempdir()?;
    let service = write_graph(root.path());
    let bundle = root.path().join("bundle");
    let server = Box::pin(WyrdTestServer::start_bound()).await?;
    let tenant = server.data_tenant_id();
    let seed = VerificationFixture::provision(server.state().postgres.wyrd(), tenant).await?;
    let admin = api_key(
        server
            .bootstrap_service("eval_errors_admin", &["admin"])
            .await?,
    );
    let receipt = register(&connect(&server, &admin), &service, &bundle).await;
    let writer = api_key(
        server
            .credential_registered_service(&receipt.root, &[])
            .await?,
    );
    let client = connect(&server, &writer);
    let provider = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "error": { "message": PROVIDER_SENTINEL, "type": "invalid_request_error" }
        })))
        .mount(&provider)
        .await;

    // Provider and storage-locator failures.
    let state = start_state(&bundle, &client).await;
    let run = state.run();
    let agent = run.for_card("agent")?;
    let agent_uid = agent.subject().uid.clone().ok_or("agent has no UID")?;
    let object = format!("{tenant}/cards/{agent_uid}/shot.png");
    server
        .state()
        .storage
        .put_object(&tenant_path::validate(&object, tenant)?, IMAGE.to_vec())
        .await?;
    emit(
        &agent,
        &json!({ "answer": "yes", "marker": "provider" }),
        Some(&media(&format!("file:///{object}"))),
        None,
    );
    emit(
        &agent,
        &json!({ "answer": "yes", "marker": "locator" }),
        Some(&media(&format!(
            "file:///{tenant}/cards/{agent_uid}/{LOCATOR_SENTINEL}.png"
        ))),
        None,
    );
    state.shutdown().await?;
    server.flush_bifrost().await?;
    let queued = enqueued(&seed, 2 * AGENT_BINDINGS).await?;
    let (stop, task) = spawn_runtime(&server, &provider.uri());
    settle(&seed, queued).await?;
    stop.cancel();
    tokio::time::timeout(WAIT, task).await??;

    // A Postgres failure: while this policy stands, every tenant read of the
    // Cards table raises the sentinel. The claim reads the run's Verifier
    // Card, so it rolls back and no run starts or spends an attempt; the
    // record's runs are enqueued before the policy is created, and they run
    // once the policy is dropped.
    let state = start_state(&bundle, &client).await;
    emit(
        &state.run().for_card("agent")?,
        &json!({ "answer": "yes", "marker": "sql" }),
        None,
        None,
    );
    state.shutdown().await?;
    server.flush_bifrost().await?;
    let [provider_id, locator_id, sql_id] = [
        record_id(&server, tenant, "provider").await?,
        record_id(&server, tenant, "locator").await?,
        record_id(&server, tenant, "sql").await?,
    ];
    let deadline = tokio::time::Instant::now() + WAIT;
    while seed.observation_runs().await?.len() < queued + AGENT_BINDINGS {
        if tokio::time::Instant::now() >= deadline {
            return Err("the sql record's runs were never enqueued".into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let superuser = server.pg_fixture().superuser_pool()?;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE FUNCTION wyrd.eval_errors_refuse() RETURNS boolean LANGUAGE plpgsql AS \
         $$BEGIN RAISE EXCEPTION '{SQL_SENTINEL}'; END$$"
    )))
    .execute(&superuser)
    .await?;
    sqlx::query(
        "CREATE POLICY eval_errors_refuse ON wyrd.cards AS RESTRICTIVE \
         FOR SELECT USING (wyrd.eval_errors_refuse())",
    )
    .execute(&superuser)
    .await?;
    let (stop, task) = spawn_runtime(&server, &provider.uri());
    let unclaimed = unclaimed_while_cards_fail(&superuser).await;
    sqlx::query("DROP POLICY eval_errors_refuse ON wyrd.cards")
        .execute(&superuser)
        .await?;
    unclaimed?;
    let runs = settle(&seed, queued + AGENT_BINDINGS).await;
    stop.cancel();
    tokio::time::timeout(WAIT, task).await??;
    let runs = runs?;

    let verification = connect(&server, &admin);
    for (record, code) in [
        (&provider_id, "eval_execution_failed"),
        (&locator_id, "eval_execution_failed"),
        (&sql_id, "eval_execution_failed"),
    ] {
        let run = run_of(&runs, "eval-gated", record)?;
        assert_unresulted(&server, tenant, run, "errored").await?;
        let status = serde_json::to_string(
            &verification
                .request_json::<(), wyrd_spec::verification::VerificationRunStatus>(
                    reqwest::Method::GET,
                    &format!("/v1/verification/runs/{}", run.run),
                    None,
                )
                .await?,
        )?;
        if !status.contains(&format!("\"code\":\"{code}\""))
            || [PROVIDER_SENTINEL, LOCATOR_SENTINEL, SQL_SENTINEL]
                .iter()
                .any(|sentinel| status.contains(sentinel))
        {
            return Err(
                format!("run status leaks dependency detail or lost {code}: {status}").into(),
            );
        }
    }
    let persisted: String = sqlx::query_scalar(
        "SELECT coalesce(string_agg(error::text, ' '), '') FROM wyrd.verifier_runs",
    )
    .fetch_one(&superuser)
    .await?;
    if [PROVIDER_SENTINEL, LOCATOR_SENTINEL, SQL_SENTINEL]
        .iter()
        .any(|sentinel| persisted.contains(sentinel))
    {
        return Err(format!("a persisted run error leaks dependency detail: {persisted}").into());
    }
    server.shutdown().await?;
    Ok(())
}

/// Write two Services, `eval-owner-a` and `eval-owner-b`, that each contain
/// the same Agent Card M and bind it to the same assertion-only Eval Verifier
/// with `observations_ready`. Returns the two Service paths.
///
/// # Panics
/// Panics when a fixture file cannot be written.
fn write_owner_graph(root: &Path) -> (PathBuf, PathBuf) {
    let service = |name: &str| {
        format!(
            "apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: {name}\n  version: 1.0.0\n  space: default\nspec:\n  service_type: agent\n  components:\n    - alias: agent\n      ref: ./agent.yaml\n      verified_by:\n        - verifier: ./verifier.yaml\n          runs_on: {{kind: observations_ready}}\n"
        )
    };
    let files = [
        (
            "agent-prompt.yaml",
            "apiVersion: wyrd/v1\nkind: Prompt\nmetadata:\n  name: eval-agent-prompt\n  version: 1.0.0\n  space: default\nspec:\n  provider: openai\n  model: gpt-test\n  messages: [answer the question]\n".to_owned(),
        ),
        ("agent.yaml", agent("eval-shared-agent")),
        (
            "verifier.yaml",
            verifier(
                "eval-shared",
                "      tasks:\n        answer: {kind: assertion, id: answer, context_path: $.answer, operator: equals, expected: \"yes\"}\n",
            ),
        ),
        ("service-a.yaml", service("eval-owner-a")),
        ("service-b.yaml", service("eval-owner-b")),
    ];
    for (name, body) in files {
        std::fs::write(root.join(name), body).expect("fixture file writes");
    }
    (root.join("service-a.yaml"), root.join("service-b.yaml"))
}

/// Emit one Eval record about `agent` through `client`'s SDK lifetime over
/// `bundle`, then shut it down so the record is acknowledged.
///
/// # Errors
/// Returns an error when the Agent is not in the bundle or shutdown fails.
async fn emit_as(
    client: &WyrdClient,
    bundle: &Path,
    marker: &str,
) -> Result<(), ServerJourneyError> {
    let state = start_state(bundle, client).await;
    emit(
        &state.run().for_card("agent")?,
        &json!({ "answer": "yes", "marker": marker }),
        None,
        None,
    );
    state.shutdown().await?;
    Ok(())
}

/// Every observation run as `(owner Card name, record ID)`, sorted.
const OWNED_RUNS_SQL: &str = "SELECT o.name, r.input_record_id FROM wyrd.verifier_runs r \
    JOIN wyrd.cards o ON o.card_uid = r.owner_card_uid \
    WHERE r.origin = 'observation' ORDER BY o.name, r.input_record_id";

/// Poll until `count` observation runs exist, wait for the run-request outbox
/// to hold nothing more, and return every run as `(owner Service name,
/// record ID)`, sorted.
///
/// Settling after the count is reached proves no further run is on its way,
/// so the returned set is exact.
///
/// # Errors
/// Returns a query error, a timeout, or an error when the outbox does not
/// settle.
async fn owned_runs(
    server: &WyrdTestServer,
    superuser: &sqlx::PgPool,
    count: usize,
) -> Result<Vec<(String, String)>, ServerJourneyError> {
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let runs: Vec<(String, String)> =
            sqlx::query_as(OWNED_RUNS_SQL).fetch_all(superuser).await?;
        if runs.len() >= count {
            let outbox = server
                .state()
                .bifrost
                .observation_runs()
                .ok_or("the server owns no run-request outbox")?;
            let left = outbox.settle(deadline.into_std()).await;
            if left != 0 {
                return Err(format!("{left} run requests never settled").into());
            }
            return Ok(sqlx::query_as(OWNED_RUNS_SQL).fetch_all(superuser).await?);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!(
                "only {} of {count} runs were enqueued: {runs:?}",
                runs.len()
            )
            .into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// An Eval record runs only the bindings its writer's Card owns (REQ-108).
///
/// Services A and B both contain Agent M and bind it to the same Eval
/// Verifier. A record A writes about M runs A's binding and none of B's, and
/// still none of B's after B authenticates; a record B writes about M runs
/// B's binding only.
///
/// # Errors
/// Returns server, registration, query, or SDK errors, or a description of the
/// first run set that does not follow its writer.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn eval_runs_follow_the_writing_owner() -> Result<(), ServerJourneyError> {
    let root = tempfile::tempdir()?;
    let (service_a, service_b) = write_owner_graph(root.path());
    let (bundle_a, bundle_b) = (root.path().join("bundle-a"), root.path().join("bundle-b"));
    let server = Box::pin(WyrdTestServer::start_bound()).await?;
    let tenant = server.data_tenant_id();
    let superuser = server.pg_fixture().superuser_pool()?;
    let admin = connect(
        &server,
        &api_key(
            server
                .bootstrap_service("eval_owner_admin", &["admin"])
                .await?,
        ),
    );
    let receipt_a = register(&admin, &service_a, &bundle_a).await;
    let receipt_b = register(&admin, &service_b, &bundle_b).await;
    let writer_a = connect(
        &server,
        &api_key(
            server
                .credential_registered_service(&receipt_a.root, &[])
                .await?,
        ),
    );

    emit_as(&writer_a, &bundle_a, "from-a").await?;
    server.flush_bifrost().await?;
    let from_a = record_id(&server, tenant, "from-a").await?;
    let owner_a = "eval-owner-a".to_owned();
    let expected = vec![(owner_a.clone(), from_a.clone())];
    let runs = owned_runs(&server, &superuser, 1).await?;
    if runs != expected {
        return Err(format!("A's record ran {runs:?}, not only A's binding").into());
    }

    // B authenticates and is now active; A's record still has no run of B's.
    let writer_b = connect(
        &server,
        &api_key(
            server
                .credential_registered_service(&receipt_b.root, &[])
                .await?,
        ),
    );
    start_state(&bundle_b, &writer_b).await.shutdown().await?;
    let runs = owned_runs(&server, &superuser, 1).await?;
    if runs != expected {
        return Err(format!("B's authentication changed A's runs: {runs:?}").into());
    }

    emit_as(&writer_b, &bundle_b, "from-b").await?;
    server.flush_bifrost().await?;
    let from_b = record_id(&server, tenant, "from-b").await?;
    let runs = owned_runs(&server, &superuser, 2).await?;
    let expected = vec![(owner_a, from_a), ("eval-owner-b".to_owned(), from_b)];
    if runs != expected {
        return Err(format!("expected one run per writer's own binding, read {runs:?}").into());
    }
    server.shutdown().await?;
    Ok(())
}

/// A direct task judgment through `observe.verify` records exactly one
/// `vala.verification.results` row correlated to the caller's application
/// Run, with no detail row, no verifier run, and no dispatch.
///
/// The row carries `result_id = execution_id`, the task verdict, the exact
/// Verifier version and subject, null binding facts and window, the Run as
/// both `run_id` and `source_record_id`, the execution interval, and the
/// canonical `AssertionResult` as `details`.
///
/// # Errors
/// Returns server, registration, query, or fixture errors, or a description of
/// the first recorded value that does not match.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn direct_task_judgment_records_one_correlated_result() -> Result<(), ServerJourneyError> {
    let root = tempfile::tempdir()?;
    let service = write_graph(root.path());
    let bundle = root.path().join("bundle");
    let server = Box::pin(WyrdTestServer::start_bound()).await?;
    let tenant = server.data_tenant_id();
    let seed = VerificationFixture::provision(server.state().postgres.wyrd(), tenant).await?;
    let admin = connect(
        &server,
        &api_key(
            server
                .bootstrap_service("direct_task_admin", &["admin"])
                .await?,
        ),
    );
    register(&admin, &service, &bundle).await;
    let state = WyrdState::from_path_with_client(&bundle, admin.clone())?;
    let run = state.run();
    let agent = run.for_card("agent")?;
    let judgment = agent
        .observe()
        .verify("eval-task", &json!({ "x": 2 }))
        .await?;
    server.flush_bifrost().await?;

    let result = judgment.execution_id.as_uuid();
    let cell =
        async |column: &str, table: &str| -> Result<Vec<Option<String>>, ServerJourneyError> {
            texts(
                &query(
                    &server,
                    tenant,
                    format!("SELECT {column} FROM {table} WHERE result_id = '{result}'"),
                )
                .await?,
            )
        };
    let application = Some(run.run_id().as_str().to_owned());
    let subject = agent.subject().uid.as_ref().map(ToString::to_string);
    for (column, expected) in [
        ("implementation", Some("task".to_owned())),
        ("execution_status", Some("completed".to_owned())),
        ("verdict", Some("failed".to_owned())),
        ("verifier_version", Some("1.0.0".to_owned())),
        ("subject_card_uid", subject),
        ("owner_card_uid", None),
        ("binding_id", None),
        ("trigger_identity", None),
        ("CAST(window_start AS VARCHAR)", None),
        ("source_record_id", application.clone()),
        ("run_id", application),
    ] {
        let read = cell(column, "vala.verification.results").await?;
        if read != [expected.clone()] {
            return Err(format!("{column}: expected [{expected:?}], read {read:?}").into());
        }
    }
    let details = cell("details", "vala.verification.results").await?;
    let [Some(details)] = details.as_slice() else {
        return Err(format!("expected one details payload, read {details:?}").into());
    };
    let details: Value = serde_json::from_str(details)?;
    if details["task_id"] != "x_is_one" || details["passed"] != false || details["actual"] != 2 {
        return Err(format!("details are not the AssertionResult: {details}").into());
    }
    let intervals = cell(
        "CAST(started_at <= ended_at AS VARCHAR)",
        "vala.verification.results",
    )
    .await?;
    if intervals != [Some("true".to_owned())] {
        return Err(format!("the execution interval is not ordered: {intervals:?}").into());
    }
    if !cell("result_id", "vala.eval.result_items")
        .await?
        .is_empty()
    {
        return Err("a direct task wrote a detail row".into());
    }
    if !seed.observation_runs().await?.is_empty() {
        return Err("a direct task created a verifier run".into());
    }
    server.shutdown().await?;
    Ok(())
}
