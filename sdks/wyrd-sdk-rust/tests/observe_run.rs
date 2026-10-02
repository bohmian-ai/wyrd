//! Rust SDK scoped-observation journey through the public `wyrd_sdk` crate.
//!
//! One invocation switches from the root Service to its Model and Agent views,
//! emits a typed Drift feature map and an Eval context, writes one row into a
//! caller-registered generic table, drains at graceful shutdown, and reads every
//! row back through Oracle by the exact subject Card UID and the one invocation
//! id. Negative flows cover a startup refused by each fixed table's describe, a
//! reserved table, an unregistered table the server describe refuses, an unknown
//! alias, and an Eval span without its trace or with malformed media. The
//! server's staged describe decisions prove a repeated dynamic write and every
//! fixed-table emit reuse the cached schema, and the writer's owner activity
//! proves ordinary observations never renew runtime activity. A span exported
//! over OTLP/gRPC under the Eval emit's trace and span identity joins the
//! persisted Eval row by the typed binary identity columns.

use std::path::{Path, PathBuf};

use base64::Engine;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use wyrd_sdk::bifrost::{TableConfig, client_from_options};
use wyrd_sdk::cards::{CardGraphHydrator, CardSelector, Cards, HydrationMode, RegistrationReceipt};
use wyrd_sdk::observe::{EvalObservationOptions, Run};
use wyrd_sdk::state::WyrdState;
use wyrd_sdk::{Bifrost, QueueConfig, WyrdClient};
use wyrd_testing::Bootstrap;
use wyrd_testing::server::WyrdTestServer;
use wyrd_tonic::otlp::common::v1::{AnyValue, KeyValue, any_value};
use wyrd_tonic::otlp::resource::v1::Resource;
use wyrd_tonic::otlp::trace::v1::{ResourceSpans, ScopeSpans, Span};
use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;
use wyrd_tonic::otlp::trace_service::trace_service_client::TraceServiceClient;
use wyrd_tonic::tonic::Request;
use wyrd_tonic::tonic::transport::Channel;

/// Payload of the single Prompt artifact the bundle carries verbatim.
const PROMPT_ARTIFACT: &[u8] = b"observe-prompt-artifact";

/// The Drift feature map one emit projects into one row per feature.
#[derive(Serialize)]
struct Features {
    /// A numeric series, projected into `num_value` and its canonical text.
    latency_ms: f64,
    /// A categorical series, projected into `str_value` only.
    tier: String,
}

/// The rows the Drift read-back selects, one per emitted feature.
#[derive(Debug, Deserialize)]
struct DriftRow {
    /// The feature name this row carries.
    series: String,
    /// The numeric projection, null for a categorical series.
    num_value: Option<f64>,
    /// The canonical text projection.
    str_value: Option<String>,
    /// The managed subject Card UID stamped from the authorized scope.
    card_uid: Option<String>,
    /// The managed invocation id every row of one run shares.
    run_id: Option<String>,
}

/// The explicit trace identity the Eval emit supplies, as canonical hex.
const EXPLICIT_TRACE: &str = "0af7651916cd43dd8448eb211c80319c";
/// The explicit span identity within [`EXPLICIT_TRACE`], as canonical hex.
const EXPLICIT_SPAN: &str = "b7ad6b7169203331";
/// The observed interaction's session the Eval emit supplies.
const SESSION: &str = "0190f5a4-8c3e-7b21-9d4f-3a6b2c1d0e9f";
/// The one media descriptor the Eval emit supplies, as the public options JSON.
const MEDIA: &str =
    r#"[{"id":"screenshot","kind":"image","uri":"s3://bucket/shot.png","media_type":"image/png"}]"#;
/// The two fixed tables `start_bifrost` must describe before it succeeds.
const FIXED_TABLES: [&str; 2] = ["vala.drift.observations", "vala.eval.observations"];

/// The rows the Eval read-back selects.
#[derive(Debug, Deserialize)]
struct EvalRow {
    /// The JSON context text the caller emitted.
    context: String,
    /// The authored session, as its UUID text.
    session_id: Option<String>,
    /// The persisted `FixedSizeBinary(16)` trace id, rendered as hex by Arrow.
    trace_id: Option<String>,
    /// The persisted `FixedSizeBinary(8)` span id, rendered as hex by Arrow.
    span_id: Option<String>,
    /// The authored media descriptors as canonical JSON text.
    media: Option<String>,
    /// The managed subject Card UID stamped from the authorized scope.
    card_uid: Option<String>,
    /// The managed invocation id every row of one run shares.
    run_id: Option<String>,
}

/// Name of the span exported under the Eval emit's trace and span identity.
const EXPLICIT_SPAN_NAME: &str = "observe-judge-call";

/// One Eval row joined to its span by typed trace and span identity.
#[derive(Debug, Deserialize)]
struct EvalSpanRow {
    /// The JSON context text of the joined Eval row.
    context: String,
    /// The name of the span sharing the row's trace and span identity.
    span_name: String,
}

/// The rows the generic-table read-back selects.
#[derive(Debug, Deserialize)]
struct DatasetRow {
    /// The caller-owned column the row carried.
    value: i64,
    /// The managed invocation id, proving generic rows correlate identically.
    run_id: Option<String>,
    /// The managed subject Card UID, proving each table keeps its own scope.
    card_uid: Option<String>,
}

/// Write a Service graph of one Model, one Agent, and its artifact-bearing Prompt.
///
/// The Model and Agent give the journey two sibling views to switch between; the
/// artifact-bearing Prompt registers alone, so the Service references it by
/// exact identity.
///
/// # Panics
/// Panics when a fixture file cannot be written.
fn write_service_graph(root: &Path) -> PathBuf {
    let digest =
        base64::engine::general_purpose::STANDARD.encode(sha2::Sha256::digest(PROMPT_ARTIFACT));
    std::fs::write(root.join("observe-prompt.txt"), PROMPT_ARTIFACT)
        .expect("prompt artifact writes");
    std::fs::write(
        root.join("observe-prompt.yaml"),
        format!(
            "apiVersion: wyrd/v1\nkind: Prompt\nmetadata:\n  name: observe-prompt\n  version: 1.0.0\n  space: default\nspec:\n  provider: openai\n  model: gpt-4o\n  messages: [hello]\nartifacts:\n  - relative_path: observe-prompt.txt\n    sha256: {digest}\n    size_bytes: {}\n    content_type: text/plain\n",
            PROMPT_ARTIFACT.len()
        ),
    )
    .expect("prompt card writes");
    std::fs::write(
        root.join("observe-model.yaml"),
        "apiVersion: wyrd/v1\nkind: Model\nmetadata:\n  name: observe-model\n  version: 1.0.0\n  space: default\nspec:\n  interface:\n    kind: Custom\n    meta:\n      framework_version: 0.1.0\n      loader_module: fixture\n      loader_class: TinyModel\n      extra: {}\n  task_type: Other\n  signature:\n    inputs:\n      - name: input\n        dtype: float64\n    outputs:\n      - name: output\n        dtype: float64\n  card_refs: []\n",
    )
    .expect("model card writes");
    std::fs::write(
        root.join("observe-agent.yaml"),
        "apiVersion: wyrd/v1\nkind: Agent\nmetadata:\n  name: observe-agent\n  version: 1.0.0\n  space: default\nspec:\n  prompt:\n    kind: Prompt\n    name: observe-prompt\n    version: 1.0.0\n    space: default\n  run_config:\n    max_iterations: 2\n    timeout_ms: 1000\n",
    )
    .expect("agent card writes");
    let service = root.join("observe-service.yaml");
    std::fs::write(
        &service,
        "apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: observe-service\n  version: 1.0.0\n  space: default\nspec:\n  service_type: agent\n  components:\n    - alias: model\n      ref: ./observe-model.yaml\n    - alias: agent\n      ref: ./observe-agent.yaml\n    - alias: prompt\n      ref:\n        kind: Prompt\n        name: observe-prompt\n        version: 1.0.0\n        space: default\n",
    )
    .expect("service card writes");
    service
}

/// Mint a machine API key through the harness's real bootstrap route.
///
/// # Panics
/// Panics when bootstrapping fails or returns a user principal.
async fn machine_key(server: &WyrdTestServer, name: &str, roles: &[&str]) -> String {
    match server
        .bootstrap_service(name, roles)
        .await
        .expect("service bootstraps")
    {
        Bootstrap::Machine { api_key, .. } => api_key.expose_secret().to_owned(),
        Bootstrap::User { .. } => panic!("service bootstrap returned a user principal"),
    }
}

/// Issue an API key for the principal the registered Service Card projects.
///
/// The writer must carry the real Service spec's card-ref scope: ingest stamps
/// `card_uid` from the signed claim alone, so a principal bound to a
/// fixture-seeded empty Card cannot scope a view onto the registered Model or
/// Agent and Gate refuses the write.
///
/// # Panics
/// Panics when credentialing fails or returns a user principal.
async fn card_bound_key(
    server: &WyrdTestServer,
    receipt: &RegistrationReceipt,
    roles: &[&str],
) -> String {
    match server
        .credential_registered_service(&receipt.root, roles)
        .await
        .expect("the registered Service principal is credentialed")
    {
        Bootstrap::Machine { api_key, .. } => api_key.expose_secret().to_owned(),
        Bootstrap::User { .. } => panic!("card credentialing returned a user principal"),
    }
}

/// Build one public client over a bound test server.
///
/// # Panics
/// Panics when the shared client cannot be assembled.
fn connect(server: &WyrdTestServer, credential: &str) -> WyrdClient {
    client_from_options(
        Some(server.base_url().expect("bound server has a URL")),
        Some(credential),
        server.grpc_url().as_deref(),
    )
    .expect("client builds")
}

/// Register one caller-owned generic table through the public SDK.
///
/// # Panics
/// Panics when the table config or its registration fails.
async fn register_dataset_table(client: &WyrdClient, fqn: &str) {
    let schema = serde_json::json!({
        "type": "object",
        "properties": { "value": { "type": "integer" } },
        "required": ["value"],
    });
    let table = TableConfig::from_json_schema(fqn, &schema).expect("dataset table config builds");
    let bifrost = Bifrost::connect_with_table(client, table)
        .await
        .expect("dataset writer connects");
    bifrost.register().await.expect("dataset table registers");
    bifrost.shutdown().await.expect("dataset writer closes");
}

/// Register the Service graph and hydrate it into one complete local bundle.
///
/// The Prompt carries an artifact, so it registers alone before the Service
/// composite references it by exact identity; hydration is what gives the run a
/// Card graph to scope views against offline.
///
/// Returns the Service registration receipt so the journey can mint its writer
/// principal bound to the exact registered root.
///
/// # Panics
/// Panics when registration or hydration fails.
async fn hydrate_bundle(
    client: &WyrdClient,
    root: &Path,
    service: &Path,
    bundle: &Path,
) -> RegistrationReceipt {
    let cards = Cards::with_client(WyrdClient::clone(client));
    Box::pin(cards.register_from_path(&root.join("observe-prompt.yaml")))
        .await
        .expect("prompt registers");
    let receipt = Box::pin(cards.register_from_path(service))
        .await
        .expect("service registers");
    let hydrator = CardGraphHydrator::new(cards.registry_context());
    Box::pin(hydrator.hydrate(
        &CardSelector::exact(receipt.root.clone()),
        bundle,
        HydrationMode::Complete,
    ))
    .await
    .expect("complete bundle hydrates");
    receipt
}

/// Read every emitted row back through Oracle and assert its correlation.
///
/// Split from the journey body so the emit phase and the durable read boundary
/// each stay readable; it asserts the Drift projection, the Eval identity, and
/// both generic tables, all selected by the one invocation id.
///
/// # Panics
/// Panics when a query fails or any row, projection, or correlation differs.
async fn assert_read_back(
    client: &WyrdClient,
    run_id: &str,
    model_uid: &str,
    agent_uid: &str,
    datasets: (&str, &str),
) {
    let query = Bifrost::query_only(client);
    let drift: Vec<DriftRow> = query
        .sql_as(&format!(
            "SELECT series, num_value, str_value, card_uid, run_id \
             FROM vala.drift.observations WHERE run_id = '{run_id}' ORDER BY series"
        ))
        .await
        .expect("drift rows read back");
    assert_eq!(drift.len(), 2, "one tall row per feature: {drift:?}");
    assert_eq!(drift[0].series, "latency_ms");
    assert_eq!(drift[0].num_value, Some(12.5));
    assert_eq!(drift[0].str_value.as_deref(), Some("12.5"));
    assert_eq!(drift[1].series, "tier");
    assert_eq!(drift[1].num_value, None);
    assert_eq!(drift[1].str_value.as_deref(), Some("gold"));
    for row in &drift {
        assert_eq!(row.run_id.as_deref(), Some(run_id));
        assert_eq!(
            row.card_uid.as_deref(),
            Some(model_uid),
            "drift rows carry the Model view's subject"
        );
    }

    let evals: Vec<EvalRow> = query
        .sql_as(&format!(
            "SELECT context, session_id, trace_id, span_id, media, card_uid, run_id \
             FROM vala.eval.observations WHERE run_id = '{run_id}'"
        ))
        .await
        .expect("eval rows read back");
    assert_eq!(
        evals.len(),
        1,
        "the refused pair and media added no row: {evals:?}"
    );
    assert_eq!(evals[0].context, r#"{"answer":"yes"}"#);
    assert_eq!(evals[0].session_id.as_deref(), Some(SESSION));
    assert_eq!(evals[0].trace_id.as_deref(), Some(EXPLICIT_TRACE));
    assert_eq!(evals[0].span_id.as_deref(), Some(EXPLICIT_SPAN));
    assert_eq!(evals[0].media.as_deref(), Some(MEDIA));
    assert_eq!(evals[0].run_id.as_deref(), Some(run_id));
    assert_eq!(
        evals[0].card_uid.as_deref(),
        Some(agent_uid),
        "the eval row carries the Agent view's subject, not the Model's"
    );

    // Two tables, two scopes, one invocation: each row keeps the subject of the
    // view that wrote it, so a second table never inherits the first's scope.
    for (table, values, subject) in [
        (datasets.0, &[41, 43][..], agent_uid),
        (datasets.1, &[42][..], model_uid),
    ] {
        let rows: Vec<DatasetRow> = query
            .sql_as(&format!(
                "SELECT value, run_id, card_uid FROM {table} WHERE run_id = '{run_id}' \
                 ORDER BY value"
            ))
            .await
            .expect("generic rows read back");
        assert_eq!(
            rows.iter().map(|row| row.value).collect::<Vec<_>>(),
            values,
            "every write to {table} landed: {rows:?}"
        );
        for row in &rows {
            assert_eq!(row.run_id.as_deref(), Some(run_id));
            assert_eq!(row.card_uid.as_deref(), Some(subject));
        }
    }
}

/// Export one span under [`EXPLICIT_TRACE`] and [`EXPLICIT_SPAN`] over the
/// server's OTLP/gRPC collector, authenticated by the bearer `token`.
///
/// This is the path any stock OTLP exporter takes, so the span lands in
/// `vala.traces.spans` exactly as an instrumented agent's would.
///
/// # Panics
/// Panics when the fixtures are not hex, the collector cannot be dialed, or
/// it refuses the export.
async fn export_explicit_span(server: &WyrdTestServer, token: &str) {
    let start = u64::try_from(chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default())
        .expect("now is after the epoch");
    let channel = Channel::from_shared(server.grpc_url().expect("bound server serves gRPC"))
        .expect("the gRPC URL is a valid endpoint")
        .connect()
        .await
        .expect("the OTLP exporter dials the collector");
    let mut request = Request::new(ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(Resource {
                attributes: vec![KeyValue {
                    key: "service.name".to_owned(),
                    value: Some(AnyValue {
                        value: Some(any_value::Value::StringValue("observe-journey".to_owned())),
                    }),
                }],
                ..Resource::default()
            }),
            scope_spans: vec![ScopeSpans {
                spans: vec![Span {
                    trace_id: hex::decode(EXPLICIT_TRACE).expect("the trace fixture is hex"),
                    span_id: hex::decode(EXPLICIT_SPAN).expect("the span fixture is hex"),
                    name: EXPLICIT_SPAN_NAME.to_owned(),
                    kind: 3,
                    start_time_unix_nano: start,
                    end_time_unix_nano: start + 1_000_000,
                    ..Span::default()
                }],
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
        .expect("the collector accepts the span");
}

/// Prove the run's Eval row joins `vala.traces.spans` by its typed identity.
///
/// The Eval row's `FixedSizeBinary(16)` trace and `FixedSizeBinary(8)` span
/// columns are compared directly with the span ledger's, with no hex or text
/// conversion, so a width or encoding mismatch yields no joined row.
///
/// # Panics
/// Panics when the query fails or the run's Eval row does not join exactly
/// the exported span.
async fn assert_eval_joins_span(client: &WyrdClient, run_id: &str) {
    let joined: Vec<EvalSpanRow> = Bifrost::query_only(client)
        .sql_as(&format!(
            "SELECT e.context, s.name AS span_name \
             FROM vala.eval.observations e JOIN vala.traces.spans s \
               ON e.trace_id = s.trace_id AND e.span_id = s.span_id \
             WHERE e.run_id = '{run_id}'"
        ))
        .await
        .expect("eval rows join spans");
    assert_eq!(joined.len(), 1, "one Eval row joins its span: {joined:?}");
    assert_eq!(joined[0].context, r#"{"answer":"yes"}"#);
    assert_eq!(joined[0].span_name, EXPLICIT_SPAN_NAME);
}

/// Drive the refusals a real caller hits on one started run.
///
/// A reserved table is refused locally before any describe, an unregistered
/// `vala.datasets` table is refused by the server's real describe before queue
/// admission, an unknown alias is refused without network IO, and an Eval span
/// without its trace or with malformed media is refused before enqueue.
///
/// # Panics
/// Panics when any refusal succeeds or carries the wrong stable code.
async fn assert_negative_flows(run: &Run, agent: &Run) {
    let orphan_span = EvalObservationOptions::from_parts(None, None, None, Some(EXPLICIT_SPAN))
        .expect("a lone span id parses");
    let unpaired = agent
        .observe()
        .eval(&serde_json::json!({ "answer": "orphan" }), orphan_span)
        .expect_err("a span without its trace is refused");
    assert_eq!(unpaired.code(), "WYRD_SPEC_400_VALIDATION");
    let malformed = EvalObservationOptions::from_parts(
        None,
        Some(r#"[{"id":"screenshot","kind":"hologram","uri":"s3://bucket/shot.png"}]"#),
        None,
        None,
    )
    .expect_err("malformed media is refused before any emit");
    assert_eq!(malformed.code(), "WYRD_SPEC_400_VALIDATION");
    let reserved = agent
        .observe()
        .record("vala.drift.observations", &serde_json::json!({ "x": 1 }))
        .await
        .expect_err("a reserved table is refused before admission");
    assert_eq!(reserved.code(), "WYRD_SDK_400_INVALID_OBSERVATION");
    let unknown = agent
        .observe()
        .record(
            &format!("vala.datasets.absent_{}", uuid::Uuid::now_v7().simple()),
            &serde_json::json!({ "value": 1 }),
        )
        .await
        .expect_err("an unregistered table is refused by the server describe");
    assert_eq!(unknown.code(), "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND");
    assert_eq!(
        run.for_card("missing")
            .expect_err("an unknown alias is refused locally")
            .code(),
        "WYRD_SDK_404_UNKNOWN_ALIAS"
    );
}

/// The single-Card form opens its own invocation on the selected Card.
///
/// A plain `run()` targets the root Service; `run_for_card` selects the alias
/// before minting a fresh `run_id` that later views share, and an unknown
/// alias is refused locally.
///
/// # Panics
/// Panics when any selection resolves the wrong subject or invocation, or an
/// unknown alias is accepted.
fn assert_initial_card_selection(state: &WyrdState, run: &Run, agent: &Run) {
    assert_eq!(
        run.card_ref(),
        state.root_ref(),
        "no Card argument targets the root"
    );
    let agent_run = state
        .run_for_card("agent")
        .expect("the single-Card form resolves");
    assert_eq!(agent_run.card_ref(), agent.card_ref());
    assert_ne!(agent_run.run_id(), run.run_id(), "its own invocation");
    assert_eq!(
        agent_run
            .for_card("model")
            .expect("model view resolves")
            .run_id(),
        agent_run.run_id(),
        "later views share the initial view's invocation"
    );
    assert_eq!(
        state
            .run_for_card("missing")
            .expect_err("an unknown initial alias is refused locally")
            .code(),
        "WYRD_SDK_404_UNKNOWN_ALIAS"
    );
}

/// The server-observed describe count for each of `tables`, in order.
///
/// # Panics
/// Panics when the staged audit rows cannot be read.
async fn describe_counts(server: &WyrdTestServer, tables: &[&str]) -> Vec<i64> {
    let mut counts = Vec::with_capacity(tables.len());
    for table in tables {
        counts.push(
            server
                .table_describe_count(table)
                .await
                .expect("describe count reads"),
        );
    }
    counts
}

/// Refuse startup once per fixed table whose describe the server fails.
///
/// For each fixed table in turn the server fails only that table's describe,
/// so the Drift case fails first and the Eval case fails after Drift already
/// described. Both must refuse `start_bifrost` with the server's stable error
/// and leave the state startable, which the caller then proves by starting it.
///
/// # Panics
/// Panics when a start succeeds, carries the wrong code, or the fault cannot
/// be installed or removed.
async fn assert_fixed_table_preflight_refusals(
    server: &WyrdTestServer,
    state: &WyrdState,
    client: &WyrdClient,
) {
    for table in FIXED_TABLES {
        server
            .fail_table_describe(table)
            .await
            .expect("describe fault installs");
        let refused = state
            .start_bifrost_with_config(client, None, QueueConfig::default())
            .await
            .expect_err("startup cannot succeed without describing every fixed table");
        assert_eq!(
            refused.code(),
            "WYRD_VALA_500_AUDIT_UNAVAILABLE",
            "{table} describe refusal: {refused:?}"
        );
        server
            .restore_table_describe()
            .await
            .expect("describe fault is removed");
    }
}

/// Emit the journey's Drift, Eval, and generic rows from their subject views.
///
/// The Agent writes `dataset` twice and the staged describe count proves the
/// second write reused the cached table; the Model writes `dataset_b` once.
///
/// # Panics
/// Panics when any emit fails or the dataset was described more than once.
async fn emit_observations(
    server: &WyrdTestServer,
    model: &Run,
    agent: &Run,
    dataset: &str,
    dataset_b: &str,
) {
    model
        .observe()
        .drift(
            &Features {
                latency_ms: 12.5,
                tier: "gold".to_owned(),
            },
            None,
        )
        .expect("drift emits");
    agent
        .observe()
        .eval(
            &serde_json::json!({ "answer": "yes" }),
            EvalObservationOptions::from_parts(
                Some(SESSION),
                Some(MEDIA),
                Some(EXPLICIT_TRACE),
                Some(EXPLICIT_SPAN),
            )
            .expect("valid session, media, and trace pair parse"),
        )
        .expect("eval emits");
    agent
        .observe()
        .record(dataset, &serde_json::json!({ "value": 41 }))
        .await
        .expect("generic row emits");
    agent
        .observe()
        .record(dataset, &serde_json::json!({ "value": 43 }))
        .await
        .expect("a repeated write reuses the cached table");
    assert_eq!(
        server
            .table_describe_count(dataset)
            .await
            .expect("describe count reads"),
        1,
        "the second write to {dataset} reused the first describe"
    );
    model
        .observe()
        .record(dataset_b, &serde_json::json!({ "value": 42 }))
        .await
        .expect("a second table describes and routes on its own");
}

/// Prove one invocation emits Drift, Eval, and generic rows that read back
/// correlated to the exact subject Card and one invocation id.
///
/// # Panics
/// Panics when any journey step or structured-error expectation fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn scoped_run_emits_drift_eval_and_generic_rows() {
    let root = tempfile::tempdir().expect("fixture root creates");
    let service = write_service_graph(root.path());
    let bundle = root.path().join("bundle");
    // Publication would retire the staged describe decisions this journey
    // counts, so it stays off for the journey's lifetime.
    let server = Box::pin(
        WyrdTestServer::builder()
            .without_audit_publication_for_test()
            .start_bound(),
    )
    .await
    .expect("test server starts");
    let admin = machine_key(&server, "rust_observe_admin", &["admin"]).await;
    let dataset = format!("vala.datasets.observe_{}", uuid::Uuid::now_v7().simple());
    let dataset_b = format!("vala.datasets.observe_{}", uuid::Uuid::now_v7().simple());
    register_dataset_table(&connect(&server, &admin), &dataset).await;
    register_dataset_table(&connect(&server, &admin), &dataset_b).await;

    let receipt = hydrate_bundle(&connect(&server, &admin), root.path(), &service, &bundle).await;
    let credential = card_bound_key(&server, &receipt, &[]).await;

    let state = WyrdState::from_path(&bundle).expect("complete bundle loads offline");
    let client = connect(&server, &credential);
    assert_fixed_table_preflight_refusals(&server, &state, &client).await;
    state
        .start_bifrost_with_config(&client, None, QueueConfig::default())
        .await
        .expect("one Bifrost lifetime starts and describes both fixed tables");
    let fixed_describes = describe_counts(&server, &FIXED_TABLES).await;
    let owner = receipt.root.uid.clone().expect("Service root has a UID");
    let activated = server
        .last_authenticated_at(&owner)
        .await
        .expect("owner activity reads");
    assert!(
        activated.is_some(),
        "the writer's key exchange activates its owner"
    );

    let run = state.run();
    let run_id = run.run_id().as_str().to_owned();
    let model = run.for_card("model").expect("model view resolves");
    let agent = run.for_card("agent").expect("agent view resolves");
    assert_eq!(model.run_id().as_str(), run_id, "one invocation, two views");
    assert_initial_card_selection(&state, &run, &agent);
    let model_uid = model
        .card_ref()
        .uid
        .clone()
        .expect("hydrated Card carries its UID")
        .to_string();
    let agent_uid = agent
        .card_ref()
        .uid
        .clone()
        .expect("hydrated Card carries its UID")
        .to_string();

    emit_observations(&server, &model, &agent, &dataset, &dataset_b).await;

    assert_negative_flows(&run, &agent).await;
    assert_eq!(
        describe_counts(&server, &FIXED_TABLES).await,
        fixed_describes,
        "Drift and Eval emits perform no per-observation schema IO"
    );

    state
        .shutdown()
        .await
        .expect("graceful shutdown drains every producer");
    assert_eq!(
        state
            .shutdown()
            .await
            .map(|()| "closed state shuts down again")
            .expect("a closed state has nothing left to drain"),
        "closed state shuts down again"
    );
    server.flush_bifrost().await.expect("flush server Scribe");
    assert_eq!(
        server
            .last_authenticated_at(&owner)
            .await
            .expect("owner activity reads"),
        activated,
        "ordinary observations never write runtime activity"
    );

    assert_read_back(
        &connect(&server, &admin),
        &run_id,
        &model_uid,
        &agent_uid,
        (&dataset, &dataset_b),
    )
    .await;

    let token = server
        .exchange_api_key(&secrecy::SecretString::from(admin.clone()))
        .await
        .expect("admin key exchanges for a bearer");
    export_explicit_span(&server, &token).await;
    server.flush_bifrost().await.expect("flush server Scribe");
    assert_eval_joins_span(&connect(&server, &admin), &run_id).await;
    server.shutdown().await.expect("test server shuts down");
}

/// The one field of `POST /auth/issue-key`'s response this journey reads.
#[derive(Deserialize)]
struct IssuedKey {
    /// Plaintext API key, returned exactly once.
    key: String,
}

/// Issue an API key for a registered Service over the public HTTP route.
///
/// No harness SQL is involved: the principal's authority is whatever
/// registration projected for it.
///
/// # Panics
/// Panics when the route refuses the issuance.
async fn issue_key(admin: &WyrdClient, receipt: &RegistrationReceipt) -> String {
    let issued: IssuedKey = admin
        .request_json(
            reqwest::Method::POST,
            "/auth/issue-key",
            Some(&serde_json::json!({
                "card_ref": receipt.root,
                "label": "workload-journey",
            })),
        )
        .await
        .expect("the registered Service's key is issued");
    issued.key
}

/// Write a second, unrelated Service whose only component is its own Model.
///
/// # Panics
/// Panics when a fixture file cannot be written.
fn write_outside_graph(root: &Path) -> PathBuf {
    std::fs::write(
        root.join("outside-model.yaml"),
        "apiVersion: wyrd/v1\nkind: Model\nmetadata:\n  name: outside-model\n  version: 1.0.0\n  space: default\nspec:\n  interface:\n    kind: Custom\n    meta:\n      framework_version: 0.1.0\n      loader_module: fixture\n      loader_class: TinyModel\n      extra: {}\n  task_type: Other\n  signature:\n    inputs:\n      - name: input\n        dtype: float64\n    outputs:\n      - name: output\n        dtype: float64\n  card_refs: []\n",
    )
    .expect("outside model card writes");
    let service = root.join("outside-service.yaml");
    std::fs::write(
        &service,
        "apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: outside-service\n  version: 1.0.0\n  space: default\nspec:\n  service_type: agent\n  components:\n    - alias: model\n      ref: ./outside-model.yaml\n",
    )
    .expect("outside service card writes");
    service
}

/// Read every row of `table` with `client`, ordered by value.
///
/// # Panics
/// Panics when the query is refused.
async fn dataset_rows(client: &WyrdClient, table: &str) -> Vec<DatasetRow> {
    Bifrost::query_only(client)
        .sql_as(&format!(
            "SELECT value, run_id, card_uid FROM {table} ORDER BY value"
        ))
        .await
        .expect("the workload key queries its evidence")
}

/// A Service key issued over public HTTP writes and queries evidence with
/// only the `workload` role registration granted, and its Card scope still
/// refuses a write for another Service's Card.
///
/// # Panics
/// Panics when any journey step or the scope refusal differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn issued_card_key_writes_and_queries_within_its_scope_only() {
    let root = tempfile::tempdir().expect("fixture root creates");
    let service = write_service_graph(root.path());
    let outside = write_outside_graph(root.path());
    let server = Box::pin(WyrdTestServer::builder().start_bound())
        .await
        .expect("test server starts");
    let admin = connect(
        &server,
        &machine_key(&server, "rust_workload_admin", &["admin"]).await,
    );
    let dataset = format!("vala.datasets.workload_{}", uuid::Uuid::now_v7().simple());
    register_dataset_table(&admin, &dataset).await;
    let receipt = hydrate_bundle(&admin, root.path(), &service, &root.path().join("bundle")).await;
    let cards = Cards::with_client(WyrdClient::clone(&admin));
    let outside_receipt = Box::pin(cards.register_from_path(&outside))
        .await
        .expect("outside service registers");
    Box::pin(CardGraphHydrator::new(cards.registry_context()).hydrate(
        &CardSelector::exact(outside_receipt.root.clone()),
        &root.path().join("outside-bundle"),
        HydrationMode::Complete,
    ))
    .await
    .expect("outside bundle hydrates");

    let key = issue_key(&admin, &receipt).await;
    let workload = connect(&server, &key);
    let state =
        WyrdState::from_path(&root.path().join("bundle")).expect("complete bundle loads offline");
    state
        .start_bifrost_with_config(&workload, None, QueueConfig::default())
        .await
        .expect("the workload key starts a Bifrost lifetime");
    let run = state.run();
    let model = run.for_card("model").expect("model view resolves");
    model
        .observe()
        .record(&dataset, &serde_json::json!({ "value": 1 }))
        .await
        .expect("the workload key writes a record");
    state.shutdown().await.expect("the write drains");
    server.flush_bifrost().await.expect("flush server Scribe");
    let model_uid = model
        .card_ref()
        .uid
        .clone()
        .expect("hydrated Card carries its UID")
        .to_string();
    let rows = dataset_rows(&workload, &dataset).await;
    assert_eq!(
        rows.iter()
            .map(|row| (row.value, row.card_uid.as_deref()))
            .collect::<Vec<_>>(),
        [(1, Some(model_uid.as_str()))],
        "the workload key reads back its own row: {rows:?}"
    );

    let outside_state = WyrdState::from_path(&root.path().join("outside-bundle"))
        .expect("outside bundle loads offline");
    outside_state
        .start_bifrost_with_config(&workload, None, QueueConfig::default())
        .await
        .expect("the workload key starts a second lifetime");
    let written = outside_state
        .run()
        .for_card("model")
        .expect("outside model view resolves")
        .observe()
        .record(&dataset, &serde_json::json!({ "value": 2 }))
        .await;
    let refusal = match written {
        Err(error) => error,
        Ok(()) => outside_state
            .shutdown()
            .await
            .expect_err("a record for a Card outside the key's scope is refused"),
    };
    let code = match &refusal {
        wyrd_sdk::verification::WyrdError::UpstreamFailure { details, .. } => details
            .get("original_code")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        other => other.code().to_owned(),
    };
    assert_eq!(
        code, "WYRD_VALA_403_BIFROST_CARD_SCOPE",
        "the refusal is the typed scope denial: {refusal:?}"
    );
    server.flush_bifrost().await.expect("flush server Scribe");
    assert_eq!(
        dataset_rows(&workload, &dataset).await.len(),
        1,
        "the refused write left no row behind"
    );
    server.shutdown().await.expect("test server shuts down");
}
