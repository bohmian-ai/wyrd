//! Server-free proof of the observation surface: one Bifrost lifetime, immutable
//! Card-scoped views, and the two fixed projections.
//!
//! The fixed-table describe bodies are built from the canonical
//! `vala-bifrost-redux` table definitions rather than a hand-copied column list,
//! so a server-side schema change fails these tests instead of passing them.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use arrow_schema::{DataType, Field, Schema};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use vala_bifrost_redux::tables::{DomainTable, DriftObservationsTable, EvalObservationsTable};
use wyrd_queue::{ClientByteGuard, MockSink, QueueConfig};
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{
    BifrostTableDescription, BifrostTableEntry, FieldSpec, INPUT_CLASS_GATE_CORRELATION,
    INPUT_CLASS_KEY, PARQUET_FIELD_ID_KEY,
};
use wyrd_spec::vala::eval::media::{MediaKind, MediaRef};
use wyrd_spec::vala::ids::{SessionId, SpanId, TraceId};

use crate::bifrost::Bifrost;
use crate::observe::lifecycle::BifrostLifecycle;
use crate::observe::{
    DRIFT_OBSERVATIONS_TABLE, EVAL_OBSERVATIONS_TABLE, EvalObservationOptions, drift, eval,
};
use crate::state::WyrdState;
use crate::state::tests::TestBundle;

/// A describe endpoint that answers per table path and counts its calls.
///
/// The counter is the whole point of the harness: "describes once per table for
/// the writer's lifetime" is only provable by observing that a second emit makes
/// no second request.
struct DescribeServer {
    /// Base URL a `ClientConfig` points at.
    base_url: String,
    /// `namespace/name` → describe body, or absence for a 404.
    bodies: Arc<Mutex<HashMap<String, String>>>,
    /// `namespace/name` → number of describe requests served.
    calls: Arc<Mutex<HashMap<String, usize>>>,
}

impl DescribeServer {
    /// Bind a stub server on an ephemeral port and serve `bodies`.
    ///
    /// # Panics
    /// Panics when the listener cannot bind or adopt the test runtime.
    fn start(bodies: HashMap<String, String>) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("test listener binds");
        let address = listener.local_addr().expect("listener has an address");
        listener
            .set_nonblocking(true)
            .expect("listener converts to tokio");
        let listener = TcpListener::from_std(listener).expect("listener adopts the runtime");
        let bodies = Arc::new(Mutex::new(bodies));
        let calls = Arc::new(Mutex::new(HashMap::new()));
        let served = Arc::clone(&bodies);
        let counted = Arc::clone(&calls);
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let served = Arc::clone(&served);
                let counted = Arc::clone(&counted);
                tokio::spawn(async move {
                    let mut buffer = [0_u8; 4096];
                    let Ok(read) = socket.read(&mut buffer).await else {
                        return;
                    };
                    let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                    let response = respond(&request, &served, &counted);
                    let _ = socket.write_all(response.as_bytes()).await;
                });
            }
        });
        Self {
            base_url: format!("http://{address}"),
            bodies,
            calls,
        }
    }

    /// How many describe requests this server served for `fqn`.
    ///
    /// # Panics
    /// Panics if the call counter lock is poisoned.
    fn calls(&self, fqn: &str) -> usize {
        *self
            .calls
            .lock()
            .expect("call counter lock")
            .get(&route_key(fqn))
            .unwrap_or(&0)
    }

    /// Publish a describe body for `fqn`, replacing any previous one.
    ///
    /// # Panics
    /// Panics if the body map lock is poisoned.
    fn publish(&self, fqn: &str, description: &BifrostTableDescription) {
        self.bodies.lock().expect("body lock").insert(
            route_key(fqn),
            serde_json::to_string(description).expect("description serializes"),
        );
    }
}

/// Serve one stub HTTP request: token exchange, describe hit, or describe miss.
///
/// # Panics
/// Panics if a shared stub lock is poisoned.
fn respond(
    request: &str,
    bodies: &Arc<Mutex<HashMap<String, String>>>,
    calls: &Arc<Mutex<HashMap<String, usize>>>,
) -> String {
    if request.starts_with("POST /auth/token") {
        let token = r#"{"access_token":"test-token","token_type":"Bearer","expires_at":"2099-01-01T00:00:00Z"}"#;
        return ok_json(token);
    }
    let line = request.lines().next().unwrap_or_default();
    let path = line.split_whitespace().nth(1).unwrap_or_default();
    let Some(table) = path.strip_prefix("/v1/bifrost/tables/") else {
        return not_found();
    };
    *calls
        .lock()
        .expect("call counter lock")
        .entry(table.to_owned())
        .or_insert(0) += 1;
    match bodies.lock().expect("body lock").get(table) {
        Some(body) => ok_json(body),
        None => not_found(),
    }
}

/// One 200 JSON response.
fn ok_json(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// One stable problem-JSON 404 for an unknown table.
fn not_found() -> String {
    let body = r#"{"type":"about:blank","title":"Not Found","status":404,"code":"WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND","detail":"no such table"}"#;
    format!(
        "HTTP/1.1 404 Not Found\r\ncontent-type: application/problem+json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// Build a describe body for `fqn` over exactly `fields`.
///
/// # Panics
/// Panics when a field is not representable as a wire `FieldSpec`.
fn description(fqn: &str, fields: Vec<Field>) -> BifrostTableDescription {
    let (namespace, name) = fqn.rsplit_once('.').expect("test fqn is namespaced");
    let schema = Schema::new(fields);
    BifrostTableDescription {
        entry: BifrostTableEntry {
            namespace: namespace.to_owned(),
            name: name.to_owned(),
            table_uid: "0102030405060708090a0b0c0d0e0f10".to_owned(),
            status: wyrd_spec::vala::api::TableStatus::Active,
            fingerprint: "aa".to_owned(),
            registered_at: "2026-07-01T00:00:00Z".parse().expect("fixture timestamp"),
            updated_at: "2026-07-01T00:00:00Z".parse().expect("fixture timestamp"),
        },
        user_fields: wyrd_queue::arrow_schema_to_fieldspec(&schema),
        correlation_fields: vec![
            FieldSpec {
                name: "card_ref".to_owned(),
                data_type: wyrd_spec::vala::api::DataTypeSpec::Utf8,
                nullable: true,
                metadata: [(
                    INPUT_CLASS_KEY.to_owned(),
                    INPUT_CLASS_GATE_CORRELATION.to_owned(),
                )]
                .into(),
            },
            FieldSpec {
                name: "run_id".to_owned(),
                data_type: wyrd_spec::vala::api::DataTypeSpec::Utf8,
                nullable: true,
                metadata: [(PARQUET_FIELD_ID_KEY.to_owned(), "1000".to_owned())].into(),
            },
        ],
        managed_candidates: Vec::new(),
        canonical_physical_fingerprint: None,
        physical_layout: wyrd_spec::vala::api::PhysicalLayoutWire {
            partition_granularity: wyrd_spec::vala::api::TimeGranularityWire::Day,
            sort_keys: Vec::new(),
            bloom_columns: Vec::new(),
        },
    }
}

/// The describe route key for one fully-qualified table name.
///
/// The route is `/v1/bifrost/tables/{namespace}/{name}`, and a Bifrost namespace
/// keeps its `vala.` root, so the key is the FQN with its final dot replaced.
///
/// # Panics
/// Panics when `fqn` is not `<namespace>.<name>`.
fn route_key(fqn: &str) -> String {
    let (namespace, name) = fqn.rsplit_once('.').expect("test fqn is namespaced");
    format!("{namespace}/{name}")
}

/// The describe bodies a healthy startup sees, from the canonical definitions.
fn fixed_table_bodies() -> HashMap<String, String> {
    [
        (
            route_key(DRIFT_OBSERVATIONS_TABLE),
            serde_json::to_string(&description(
                DRIFT_OBSERVATIONS_TABLE,
                DriftObservationsTable::arrow_fields(),
            ))
            .expect("drift description serializes"),
        ),
        (
            route_key(EVAL_OBSERVATIONS_TABLE),
            serde_json::to_string(&description(
                EVAL_OBSERVATIONS_TABLE,
                EvalObservationsTable::arrow_fields(),
            ))
            .expect("eval description serializes"),
        ),
    ]
    .into()
}

/// A client pointed at `base_url` with a static credential.
///
/// # Panics
/// Panics if the static configuration cannot build a client.
fn client_for(base_url: &str) -> crate::WyrdClient {
    crate::WyrdClient::with_config(crate::config::ClientConfig {
        credential: Some(secrecy::SecretString::from("test-key")),
        http: crate::transport::HttpConfig {
            base_url: base_url.to_owned(),
            timeout_ms: 5_000,
            ..crate::transport::HttpConfig::default()
        },
        ..crate::config::ClientConfig::default()
    })
    .expect("static config builds a client")
}

/// A Bifrost over a mock sink whose describes reach `server`.
///
/// `with_sink` performs no IO, so the write plane is in-memory while the read
/// plane — and therefore every describe — still goes through real HTTP.
fn bifrost_for(server: &DescribeServer) -> Bifrost {
    bifrost_over(
        server,
        Arc::new(MockSink::new()),
        QueueConfig {
            flush_interval_ms: 0,
            ..QueueConfig::default()
        },
    )
}

/// A Bifrost over a caller-held mock sink and queue configuration.
///
/// The caller keeps `sink` to script ambiguous sends and to read back the batch
/// identities the writer attempted and settled.
fn bifrost_over(server: &DescribeServer, sink: Arc<MockSink>, config: QueueConfig) -> Bifrost {
    Bifrost::with_sink(
        &client_for(&server.base_url),
        None,
        sink as Arc<dyn wyrd_queue::BatchSink<ClientByteGuard>>,
        config,
    )
}

/// A loaded state over the shared complete-Service bundle fixture.
///
/// # Panics
/// Panics if the fixture bundle does not load.
fn state_fixture() -> (TestBundle, WyrdState) {
    let bundle = TestBundle::complete_service();
    let state = WyrdState::from_path(bundle.path()).expect("fixture bundle loads");
    (bundle, state)
}

/// Start `state` against `server` without dialling a gRPC ingest channel.
///
/// # Errors
/// Returns whatever `start_bifrost_with` would return: both doors take
/// the same `StartClaim` and run the same fixed-table describes.
async fn start_over(state: &WyrdState, server: &DescribeServer) -> Result<(), WyrdError> {
    state
        .adopt_started_bifrost_for_test(bifrost_for(server))
        .await
}

// ── Scenario 1: one Bifrost lifetime per state ──────────────────────────────

/// Startup describes both fixed tables exactly once and then serves every run.
#[tokio::test]
async fn startup_describes_both_fixed_tables_once() {
    let server = DescribeServer::start(fixed_table_bodies());
    let (_bundle, state) = state_fixture();

    start_over(&state, &server).await.expect("startup succeeds");

    assert_eq!(server.calls(DRIFT_OBSERVATIONS_TABLE), 1);
    assert_eq!(server.calls(EVAL_OBSERVATIONS_TABLE), 1);
    let run = state.run();
    run.observe()
        .drift(&json!({ "age": 42 }), None)
        .expect("drift enqueues");
    run.observe()
        .eval(
            &json!({ "answer": "yes" }),
            EvalObservationOptions::default(),
        )
        .expect("eval enqueues");
    assert_eq!(
        (
            server.calls(DRIFT_OBSERVATIONS_TABLE),
            server.calls(EVAL_OBSERVATIONS_TABLE)
        ),
        (1, 1),
        "an observation never re-describes its fixed table"
    );
}

/// A missing fixed table fails startup, so no run can emit.
#[tokio::test]
async fn missing_fixed_table_fails_startup() {
    let mut bodies = fixed_table_bodies();
    bodies.remove(&route_key(EVAL_OBSERVATIONS_TABLE));
    let server = DescribeServer::start(bodies);
    let (_bundle, state) = state_fixture();

    let error = start_over(&state, &server)
        .await
        .expect_err("an absent fixed table must fail startup");
    assert_eq!(error.status(), 404);
    let refusal = state
        .run()
        .observe()
        .drift(&json!({ "age": 42 }), None)
        .expect_err("a failed startup leaves no writer");
    assert_eq!(refusal.code(), "WYRD_SDK_400_BIFROST_NOT_STARTED");
}

/// A fixed table whose authored fields differ from the projection in any
/// position — reordered, extra, retyped, or renullabled — fails startup.
#[tokio::test]
async fn incompatible_fixed_table_fails_startup() {
    let canonical = DriftObservationsTable::arrow_fields();
    let mut reordered = canonical.clone();
    reordered.swap(0, 1);
    let mut extra = canonical.clone();
    extra.push(Field::new("extra", DataType::Utf8, true));
    let retyped: Vec<Field> = canonical
        .iter()
        .map(|field| match field.name().as_str() {
            "num_value" => field.clone().with_data_type(DataType::Int64),
            _ => field.clone(),
        })
        .collect();
    let renullabled: Vec<Field> = canonical
        .iter()
        .map(|field| match field.name().as_str() {
            "series" => field.clone().with_nullable(true),
            _ => field.clone(),
        })
        .collect();

    for (case, fields) in [
        ("reordered", reordered),
        ("extra", extra),
        ("wrong type", retyped),
        ("wrong nullability", renullabled),
    ] {
        let server = DescribeServer::start(fixed_table_bodies());
        server.publish(
            DRIFT_OBSERVATIONS_TABLE,
            &description(DRIFT_OBSERVATIONS_TABLE, fields),
        );
        let (_bundle, state) = state_fixture();

        let error = start_over(&state, &server)
            .await
            .expect_err(&format!("a {case} authored field must fail startup"));
        assert_eq!(error.code(), "WYRD_SDK_400_INVALID_OBSERVATION", "{case}");
        assert!(error.to_string().contains("incompatible"), "{case}");
    }
}

/// A failed startup releases its claim, so startup may be retried.
#[tokio::test]
async fn failed_startup_may_be_retried() {
    let mut bodies = fixed_table_bodies();
    let eval_body = bodies
        .remove(&route_key(EVAL_OBSERVATIONS_TABLE))
        .expect("fixture has an eval body");
    let server = DescribeServer::start(bodies);
    let (_bundle, state) = state_fixture();

    start_over(&state, &server)
        .await
        .expect_err("the first startup fails");
    server
        .bodies
        .lock()
        .expect("body lock")
        .insert(route_key(EVAL_OBSERVATIONS_TABLE), eval_body);
    start_over(&state, &server)
        .await
        .expect("a released claim lets startup retry");
}

/// A second startup on the same state — or any clone of it — is refused.
#[tokio::test]
async fn second_startup_is_refused() {
    let server = DescribeServer::start(fixed_table_bodies());
    let (_bundle, state) = state_fixture();
    start_over(&state, &server).await.expect("first startup");

    let error = start_over(&state.clone(), &server)
        .await
        .expect_err("a clone shares the one Bifrost lifetime");
    assert_eq!(error.code(), "WYRD_SDK_409_BIFROST_ALREADY_STARTED");
    assert_eq!(error.status(), 409);
}

/// Shutdown closes the state to writes and stays closed.
#[tokio::test]
async fn shutdown_closes_the_state_permanently() {
    let server = DescribeServer::start(fixed_table_bodies());
    let (_bundle, state) = state_fixture();
    start_over(&state, &server).await.expect("startup");
    let run = state.run();

    state.shutdown().await.expect("drain succeeds");
    state
        .shutdown()
        .await
        .expect("a closed state shuts down idempotently");

    let refusal = run
        .observe()
        .drift(&json!({ "age": 42 }), None)
        .expect_err("a closed state refuses writes");
    assert_eq!(refusal.code(), "WYRD_SDK_409_BIFROST_CLOSED");
    let restart = start_over(&state, &server)
        .await
        .expect_err("a closed state cannot start again");
    assert_eq!(restart.code(), "WYRD_SDK_409_BIFROST_CLOSED");
}

/// A never-started state has nothing to drain, and its shutdown is terminal.
#[tokio::test]
async fn shutdown_without_startup_closes_permanently() {
    let server = DescribeServer::start(fixed_table_bodies());
    let (_bundle, state) = state_fixture();
    state.shutdown().await.expect("nothing to drain");

    let restart = start_over(&state, &server)
        .await
        .expect_err("a closed state cannot start");
    assert_eq!(restart.code(), "WYRD_SDK_409_BIFROST_CLOSED");
    let refusal = state
        .run()
        .observe()
        .drift(&json!({ "age": 42 }), None)
        .expect_err("a closed state refuses writes");
    assert_eq!(refusal.code(), "WYRD_SDK_409_BIFROST_CLOSED");
}

/// A shutdown that lands while a start holds its claim wins: the start cannot
/// install its writer, and neither publishing nor abandoning the claim reopens
/// the state.
#[tokio::test]
async fn shutdown_during_start_stays_closed() {
    let server = DescribeServer::start(fixed_table_bodies());

    let lifecycle = BifrostLifecycle::default();
    let claim = lifecycle.claim().expect("first claim");
    lifecycle
        .shutdown()
        .await
        .expect("shutdown closes a starting state");
    let lost = claim
        .complete(bifrost_for(&server))
        .await
        .expect_err("the losing start cannot publish its writer");
    assert_eq!(lost.code(), "WYRD_SDK_409_BIFROST_CLOSED");
    assert_eq!(
        lifecycle.started().err().map(|error| error.code()),
        Some("WYRD_SDK_409_BIFROST_CLOSED")
    );
    assert_eq!(
        lifecycle.claim().err().map(|error| error.code()),
        Some("WYRD_SDK_409_BIFROST_CLOSED")
    );

    let abandoned = BifrostLifecycle::default();
    let claim = abandoned.claim().expect("first claim");
    abandoned
        .shutdown()
        .await
        .expect("shutdown closes a starting state");
    drop(claim);
    assert_eq!(
        abandoned.claim().err().map(|error| error.code()),
        Some("WYRD_SDK_409_BIFROST_CLOSED"),
        "an abandoned claim does not roll a closed state back"
    );
}

/// An ambiguous drain keeps the one state-owned writer: retrying shutdown on the
/// same state settles the very batch it retained, then the state is closed.
#[tokio::test(flavor = "multi_thread")]
async fn ambiguous_shutdown_retries_the_same_batch_on_the_same_state() {
    let server = DescribeServer::start(fixed_table_bodies());
    let (_bundle, state) = state_fixture();
    let sink = Arc::new(MockSink::new());
    state
        .adopt_started_bifrost_for_test(bifrost_over(
            &server,
            Arc::clone(&sink),
            QueueConfig {
                flush_interval_ms: 0,
                ..QueueConfig::default()
            },
        ))
        .await
        .expect("startup");
    state
        .run()
        .observe()
        .drift(&json!({ "age": 42 }), None)
        .expect("one row enqueues");
    sink.fail_next(1);

    state
        .shutdown()
        .await
        .expect_err("an ambiguous send makes the drain fail");
    let retained = sink.attempted();
    assert_eq!(retained.len(), 1, "one batch was attempted and retained");
    assert!(
        sink.received().is_empty(),
        "the ambiguous batch is not settled"
    );

    // A shutdown inside the retained batch's backoff window answers at once
    // with a timeout, so a caller retries after a pause, as this loop does. The
    // bound only stops a regression from hanging the lane; correctness is the
    // batch identity asserted below, not the timing.
    let mut settled = false;
    for _ in 0..500 {
        if state.shutdown().await.is_ok() {
            settled = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        settled,
        "a retried shutdown on the same state eventually drains"
    );
    assert!(
        sink.attempted()
            .iter()
            .all(|batch_id| *batch_id == retained[0]),
        "every attempt reuses the retained batch identity"
    );
    let received = sink.received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].batch_id, retained[0]);

    let refusal = state
        .run()
        .observe()
        .drift(&json!({ "age": 42 }), None)
        .expect_err("a closed state refuses writes");
    assert_eq!(refusal.code(), "WYRD_SDK_409_BIFROST_CLOSED");
    let restart = start_over(&state, &server)
        .await
        .expect_err("a closed state cannot start again");
    assert_eq!(restart.code(), "WYRD_SDK_409_BIFROST_CLOSED");
}

// ── Scenario 2: one invocation, immutable Card-scoped views ─────────────────

/// A run targets the root Service; views share its id and keep their own subject.
#[test]
fn scoped_views_share_one_invocation_and_keep_their_subjects() {
    let (_bundle, state) = state_fixture();
    let run = state.run();
    assert_eq!(run.card_ref(), state.root_ref());

    let model = run.for_card("model").expect("registered alias resolves");
    let backup = run.for_card("backup").expect("registered alias resolves");

    assert_eq!(model.run_id(), run.run_id());
    assert_eq!(backup.run_id(), run.run_id());
    assert_eq!(
        model.card_ref(),
        state.card_ref("model").expect("model ref")
    );
    assert_eq!(
        backup.card_ref(),
        state.card_ref("backup").expect("backup ref")
    );
    assert_ne!(model.card_ref(), backup.card_ref());
    assert_eq!(
        run.card_ref(),
        state.root_ref(),
        "scoping a view never mutates its parent"
    );
}

/// Two runs over one state are two invocations.
#[test]
fn each_run_is_its_own_invocation() {
    let (_bundle, state) = state_fixture();
    assert_ne!(state.run().run_id(), state.run().run_id());
}

/// An unknown alias fails locally, before any transport exists.
#[test]
fn unknown_alias_fails_without_network_io() {
    let (_bundle, state) = state_fixture();
    let error = state
        .run()
        .for_card("not_in_this_graph")
        .expect_err("an unregistered alias must refuse");
    assert_eq!(error.code(), "WYRD_SDK_404_UNKNOWN_ALIAS");
}

// ── Scenario 3: the Drift projection ───────────────────────────────────────

/// Parse one projected row's JSON.
///
/// # Panics
/// Panics when the projected bytes are not a JSON object.
fn row_value(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).expect("a projected row is JSON")
}

/// Every feature becomes one tall row sharing the observation's identity.
#[test]
fn drift_projects_one_tall_row_per_feature() {
    let record = drift::observation(
        &json!({ "age": 42, "plan": "premium", "score": 0.82, "churned": true }),
        Some(SessionId(uuid::Uuid::nil())),
    )
    .expect("a flat scalar object is a feature map");
    let rows: Vec<Value> = drift::rows(&record)
        .expect("projection succeeds")
        .iter()
        .map(|row| row_value(row))
        .collect();

    assert_eq!(rows.len(), 4);
    let series: Vec<&str> = rows
        .iter()
        .map(|row| row["series"].as_str().expect("series is a string"))
        .collect();
    assert_eq!(series, vec!["age", "churned", "plan", "score"]);
    for row in &rows {
        assert_eq!(row["record_id"], json!(record.record_id.0.to_string()));
        assert_eq!(row["session_id"], json!(uuid::Uuid::nil().to_string()));
        assert!(row["created_at"].is_string());
        assert_eq!(
            row.as_object().expect("row is an object").len(),
            6,
            "a row carries exactly the fixed user columns"
        );
        assert!(
            row.get("run_id").is_none() && row.get("card_ref").is_none(),
            "correlation is not a row field"
        );
    }
}

/// Numeric features carry the Arrow-canonical string a baseline fitter derives.
#[test]
fn drift_numeric_str_value_matches_arrow_cast() {
    let record = drift::observation(
        &json!({ "income": 82000.0, "visits": 5, "flagged": true, "plan": "premium" }),
        None,
    )
    .expect("feature map builds");
    let rows: Vec<Value> = drift::rows(&record)
        .expect("projection succeeds")
        .iter()
        .map(|row| row_value(row))
        .collect();
    let by_series: HashMap<&str, &Value> = rows
        .iter()
        .map(|row| (row["series"].as_str().expect("series"), row))
        .collect();

    // Arrow renders Float64 82000.0 as "82000.0"; Rust's Display renders
    // "82000". A baseline fitted by casting a Float64 column to Utf8 holds the
    // former, so the client must too or the value lands in no bucket.
    assert_eq!(by_series["income"]["num_value"], json!(82000.0));
    assert_eq!(by_series["income"]["str_value"], json!("82000.0"));
    assert_eq!(by_series["visits"]["num_value"], json!(5.0));
    assert_eq!(by_series["visits"]["str_value"], json!("5"));
    assert_eq!(by_series["flagged"]["num_value"], Value::Null);
    assert_eq!(by_series["flagged"]["str_value"], json!("true"));
    assert_eq!(by_series["plan"]["num_value"], Value::Null);
    assert_eq!(by_series["plan"]["str_value"], json!("premium"));
}

/// Inputs a feature map cannot represent are refused before any enqueue.
#[test]
fn drift_refuses_unsupported_inputs() {
    for input in [
        json!({ "age": null }),
        json!({ "age": [1, 2] }),
        json!({ "nested": { "age": 1 } }),
        json!({ "huge": 9_007_199_254_740_993_i64 }),
        json!({ "Age": 1 }),
        json!({}),
        json!([{ "age": 1 }]),
        json!("age=1"),
    ] {
        let error = drift::observation(&input, None)
            .expect_err(&format!("{input} must be refused before projection"));
        assert_eq!(
            error.code(),
            "WYRD_SDK_400_INVALID_OBSERVATION",
            "refusal for {input}"
        );
        assert_eq!(error.status(), 400);
    }
}

/// A non-finite float cannot even reach JSON, so it is refused at the door.
#[test]
fn drift_refuses_non_finite_floats() {
    let (_bundle, state) = state_fixture();
    /// A typed Drift input whose only feature is a float the test sets to NaN.
    #[derive(serde::Serialize)]
    struct Features {
        /// The feature under test; holds a non-finite value.
        score: f64,
    }
    let error = state
        .run()
        .observe()
        .drift(&Features { score: f64::NAN }, None)
        .expect_err("NaN is not a Float64 projection");
    assert_eq!(error.code(), "WYRD_SDK_400_INVALID_OBSERVATION");
}

// ── Scenario 4: the Eval projection ────────────────────────────────────────

/// The fixed row preserves context, session, media, and trace identity.
#[test]
fn eval_projects_context_media_and_trace_identity() {
    let trace_id = TraceId::from_hex("0102030405060708090a0b0c0d0e0f10").expect("trace hex");
    let span_id = SpanId::from_hex("1112131415161718").expect("span hex");
    let record = eval::observation(
        json!({ "question": "why?", "answer": "because" }),
        EvalObservationOptions {
            session_id: Some(SessionId(uuid::Uuid::nil())),
            media: vec![MediaRef {
                id: "screenshot".parse().expect("binding id"),
                kind: MediaKind::Image,
                uri: "s3://bucket/shot.png".to_owned(),
                media_type: Some("image/png".to_owned()),
            }],
            trace_id: Some(trace_id),
            span_id: Some(span_id),
        },
    )
    .expect("the canonical record builds");
    let row = row_value(&eval::row(&record).expect("projection succeeds"));

    assert_eq!(row["trace_id"], json!("0102030405060708090a0b0c0d0e0f10"));
    assert_eq!(row["span_id"], json!("1112131415161718"));
    assert_eq!(row["session_id"], json!(uuid::Uuid::nil().to_string()));
    let context: Value = serde_json::from_str(row["context"].as_str().expect("context text"))
        .expect("context is canonical JSON text");
    assert_eq!(context, json!({ "question": "why?", "answer": "because" }));
    let media: Value = serde_json::from_str(row["media"].as_str().expect("media text"))
        .expect("media is canonical JSON text");
    assert_eq!(media[0]["id"], json!("screenshot"));
    assert_eq!(media[0]["uri"], json!("s3://bucket/shot.png"));
    assert_eq!(
        row.as_object().expect("row is an object").len(),
        7,
        "a row carries exactly the fixed user columns"
    );
    assert!(
        row.get("eval_ref").is_none() && row.get("run_id").is_none(),
        "the row names neither a Verifier nor a duplicate run id"
    );
}

/// No active span and no explicit ids leaves both trace fields absent.
#[test]
fn eval_without_a_span_omits_trace_identity() {
    let record = eval::observation(
        json!({ "answer": "yes" }),
        EvalObservationOptions::default(),
    )
    .expect("record builds");
    assert!(record.trace_id.is_none() && record.span_id.is_none());
    let row = row_value(&eval::row(&record).expect("projection succeeds"));
    assert_eq!(row["trace_id"], Value::Null);
    assert_eq!(row["span_id"], Value::Null);
    assert_eq!(row["media"], Value::Null);
}

/// A span without its trace is refused before enqueue.
#[test]
fn eval_refuses_a_span_without_its_trace() {
    let error = eval::observation(
        json!({ "answer": "yes" }),
        EvalObservationOptions {
            span_id: Some(SpanId::from_hex("1112131415161718").expect("span hex")),
            ..EvalObservationOptions::default()
        },
    )
    .expect_err("a span id is only meaningful inside its trace");
    assert_eq!(error.code(), "WYRD_SPEC_400_VALIDATION");
}

// ── Scenario 6: the generic record path ────────────────────────────────────

/// The first call describes the table once; later calls reuse its schema.
#[tokio::test]
async fn record_describes_a_dataset_table_once() {
    let server = DescribeServer::start(fixed_table_bodies());
    server.publish(
        "vala.datasets.app_events",
        &description(
            "vala.datasets.app_events",
            vec![Field::new("event", DataType::Utf8, false)],
        ),
    );
    let (_bundle, state) = state_fixture();
    start_over(&state, &server).await.expect("startup");
    let run = state.run();

    run.observe()
        .record("vala.datasets.app_events", &json!({ "event": "done" }))
        .await
        .expect("first record describes then enqueues");
    run.for_card("model")
        .expect("model view")
        .observe()
        .record("vala.datasets.app_events", &json!({ "event": "scored" }))
        .await
        .expect("a sibling view reuses the cached schema");

    assert_eq!(
        server.calls("vala.datasets.app_events"),
        1,
        "one schema is authoritative per table for the writer's lifetime"
    );
}

/// Racing first uses of one dynamic table converge on one describe and one
/// producer, while two distinct tables keep their own destinations.
#[tokio::test]
async fn concurrent_first_records_describe_each_table_once() {
    let server = DescribeServer::start(fixed_table_bodies());
    for table in ["vala.datasets.first", "vala.datasets.second"] {
        server.publish(
            table,
            &description(table, vec![Field::new("event", DataType::Utf8, false)]),
        );
    }
    let (_bundle, state) = state_fixture();
    start_over(&state, &server).await.expect("startup");
    let run = state.run();
    let observe = run.observe();
    let row = json!({ "event": "raced" });

    let results = futures_util::future::join_all((0..8).map(|index| {
        let table = if index % 2 == 0 {
            "vala.datasets.first"
        } else {
            "vala.datasets.second"
        };
        observe.record(table, &row)
    }))
    .await;

    assert!(results.iter().all(Result::is_ok), "{results:?}");
    assert_eq!(server.calls("vala.datasets.first"), 1);
    assert_eq!(server.calls("vala.datasets.second"), 1);
    let started = state.started_bifrost().expect("started");
    assert_eq!(
        started.bifrost.producer_count(),
        2,
        "one pooled producer per distinct table"
    );
    assert_ne!(
        started
            .bifrost
            .cached_writer_table("vala.datasets.first")
            .expect("first cached")
            .fqn(),
        started
            .bifrost
            .cached_writer_table("vala.datasets.second")
            .expect("second cached")
            .fqn(),
    );
}

/// A reserved or system-managed table is refused before any describe.
#[tokio::test]
async fn record_refuses_reserved_and_system_tables() {
    let server = DescribeServer::start(fixed_table_bodies());
    let (_bundle, state) = state_fixture();
    start_over(&state, &server).await.expect("startup");
    let run = state.run();

    for table in [
        DRIFT_OBSERVATIONS_TABLE,
        EVAL_OBSERVATIONS_TABLE,
        "vala.system.audit_log",
        "vala.traces.spans",
        "vala.verification.results",
        "app.events",
        "vala.datasets",
        "vala.datasets.nested.name",
    ] {
        let before = server.calls(table);
        let error = run
            .observe()
            .record(table, &json!({ "event": "done" }))
            .await
            .expect_err("only registered vala.datasets tables are caller-owned");
        assert_eq!(error.code(), "WYRD_SDK_400_INVALID_OBSERVATION", "{table}");
        assert_eq!(
            server.calls(table),
            before,
            "{table} is refused before any describe"
        );
    }
}

/// An unknown dataset table fails at describe, before queue admission.
#[tokio::test]
async fn record_refuses_an_unknown_dataset_table() {
    let server = DescribeServer::start(fixed_table_bodies());
    let (_bundle, state) = state_fixture();
    start_over(&state, &server).await.expect("startup");

    let error = state
        .run()
        .observe()
        .record("vala.datasets.absent", &json!({ "event": "done" }))
        .await
        .expect_err("an unregistered table must refuse");
    assert_eq!(error.status(), 404);
}
