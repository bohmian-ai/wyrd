//! Server-free proof of the observation surface: one Bifrost lifetime, immutable
//! Card-scoped views, Verifier execution, and the Drift projection.
//!
//! The fixed-table describe bodies are built from the canonical
//! `vala-bifrost-redux` table definitions rather than a hand-copied column list,
//! so a server-side schema change fails these tests instead of passing them.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use arrow_schema::{DataType, Field, Schema};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use vala_bifrost_redux::tables::{DomainTable, DriftObservationsTable};
use wyrd_queue::{ClientByteGuard, MockSink, QueueConfig};
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{
    BifrostTableDescription, BifrostTableEntry, FieldSpec, PARQUET_FIELD_ID_KEY,
};
use wyrd_spec::vala::error::BifrostError;
use wyrd_spec::vala::ids::SessionId;

use crate::bifrost::Bifrost;
use crate::observe::lifecycle::BifrostLifecycle;
use crate::observe::{DRIFT_OBSERVATIONS_TABLE, drift};
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
        let token = r#"{"access_token":"test-token","token_type":"Bearer","expires_in":3600}"#;
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
    let body = WyrdError::from(BifrostError::TableNotFound {
        table: "no.such_table".to_owned(),
    })
    .as_problem_json()
    .to_string();
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
        user_fields: wyrd_types::arrow_schema_to_fieldspec(&schema)
            .expect("the fixture schema is representable"),
        correlation_fields: vec![
            FieldSpec {
                name: "card_uid".to_owned(),
                data_type: wyrd_spec::vala::api::DataTypeSpec::Utf8,
                nullable: true,
                metadata: [(PARQUET_FIELD_ID_KEY.to_owned(), "1001".to_owned())].into(),
            },
            FieldSpec {
                name: "run_id".to_owned(),
                data_type: wyrd_spec::vala::api::DataTypeSpec::Utf8,
                nullable: true,
                metadata: [(PARQUET_FIELD_ID_KEY.to_owned(), "1000".to_owned())].into(),
            },
        ],
        canonical_physical_fingerprint: None,
        physical_layout: wyrd_spec::vala::api::PhysicalLayoutWire {
            partition_granularity: wyrd_spec::vala::api::TimeGranularityWire::Day,
            time_column: None,
            sort_keys: Vec::new(),
            bloom_columns: Vec::new(),
        },
        compaction_target_file_size_bytes: None,
        compaction_type: None,
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
    [(
        route_key(DRIFT_OBSERVATIONS_TABLE),
        serde_json::to_string(&description(
            DRIFT_OBSERVATIONS_TABLE,
            DriftObservationsTable::arrow_fields(),
        ))
        .expect("drift description serializes"),
    )]
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
            linger_ms: 60_000,
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

/// Startup describes the fixed Drift table exactly once and then serves
/// every run.
#[tokio::test]
async fn startup_describes_the_fixed_table_once() {
    let server = DescribeServer::start(fixed_table_bodies());
    let (_bundle, state) = state_fixture();

    start_over(&state, &server).await.expect("startup succeeds");

    assert_eq!(server.calls(DRIFT_OBSERVATIONS_TABLE), 1);
    let run = state.run();
    for _ in 0..2 {
        run.observe()
            .drift_value(&json!({ "age": 42 }), None)
            .expect("drift enqueues");
    }
    assert_eq!(
        server.calls(DRIFT_OBSERVATIONS_TABLE),
        1,
        "an observation never re-describes its fixed table"
    );
}

/// A missing fixed table fails startup, so no run can emit.
#[tokio::test]
async fn missing_fixed_table_fails_startup() {
    let mut bodies = fixed_table_bodies();
    bodies.remove(&route_key(DRIFT_OBSERVATIONS_TABLE));
    let server = DescribeServer::start(bodies);
    let (_bundle, state) = state_fixture();

    let error = start_over(&state, &server)
        .await
        .expect_err("an absent fixed table must fail startup");
    assert_eq!(error.status(), 404);
    let refusal = state
        .run()
        .observe()
        .drift_value(&json!({ "age": 42 }), None)
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
    let drift_body = bodies
        .remove(&route_key(DRIFT_OBSERVATIONS_TABLE))
        .expect("fixture has a drift body");
    let server = DescribeServer::start(bodies);
    let (_bundle, state) = state_fixture();

    start_over(&state, &server)
        .await
        .expect_err("the first startup fails");
    server
        .bodies
        .lock()
        .expect("body lock")
        .insert(route_key(DRIFT_OBSERVATIONS_TABLE), drift_body);
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
        .drift_value(&json!({ "age": 42 }), None)
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
        .drift_value(&json!({ "age": 42 }), None)
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
                linger_ms: 60_000,
                ..QueueConfig::default()
            },
        ))
        .await
        .expect("startup");
    state
        .run()
        .observe()
        .drift_value(&json!({ "age": 42 }), None)
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
        .drift_value(&json!({ "age": 42 }), None)
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
    assert_eq!(run.subject(), state.root_ref());

    let model = run.for_card("model").expect("registered alias resolves");
    let backup = run.for_card("backup").expect("registered alias resolves");

    assert_eq!(model.run_id(), run.run_id());
    assert_eq!(backup.run_id(), run.run_id());
    assert_eq!(
        (run.alias(), model.alias(), backup.alias()),
        ("root", "model", "backup"),
        "each view is named by the alias it was opened with"
    );
    assert_eq!(model.subject(), state.card_ref("model").expect("model ref"));
    assert_eq!(
        backup.subject(),
        state.card_ref("backup").expect("backup ref")
    );
    assert_ne!(model.subject(), backup.subject());
    assert_eq!(
        run.subject(),
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

/// An initial Card selection opens the run on that Card and later views share its id.
///
/// # Panics
/// Panics if the fixture bundle does not load, if `model` or `backup` fails to
/// resolve, or if the initial view's Card, the shared invocation id, or the
/// distinct id of a second selection does not hold.
#[test]
fn run_for_card_selects_the_initial_view_and_shares_its_invocation() {
    let (_bundle, state) = state_fixture();
    let model = state
        .run_for_card("model")
        .expect("registered alias resolves");
    assert_eq!(model.subject(), state.card_ref("model").expect("model ref"));

    let backup = model.for_card("backup").expect("registered alias resolves");
    assert_eq!(backup.run_id(), model.run_id());
    assert_eq!(
        model.subject(),
        state.card_ref("model").expect("model ref"),
        "a sibling view never mutates the initial view"
    );
    assert_ne!(
        state.run_for_card("model").expect("model run").run_id(),
        model.run_id(),
        "each selection opens its own invocation"
    );
}

/// An unknown initial alias fails locally and opens nothing.
///
/// # Panics
/// Panics if the fixture bundle does not load, if the unknown alias resolves,
/// or if the refusal code is not `WYRD_SDK_404_UNKNOWN_ALIAS`.
#[test]
fn run_for_card_refuses_an_unknown_alias_without_network_io() {
    let (_bundle, state) = state_fixture();
    let error = state
        .run_for_card("not_in_this_graph")
        .expect_err("an unregistered alias must refuse");
    assert_eq!(error.code(), "WYRD_SDK_404_UNKNOWN_ALIAS");
}

// ── Scenario 2b: Verifier execution through named slots ────────────────────

/// A client presenting a static bearer to `base_url` with `timeout_ms`, so no
/// token exchange appears among the recorded requests.
///
/// # Panics
/// Panics when the auth middleware or transport cannot be built.
fn bearer_client(base_url: String, timeout_ms: u64) -> crate::WyrdClient {
    let config = crate::config::ClientConfig::default();
    let auth = crate::auth::AuthMiddleware::new(
        &config,
        crate::transport::credential::ResolvedCredential::BearerToken(
            "test-bearer".to_owned().into(),
        ),
    )
    .expect("auth builds");
    let transport = crate::transport::HttpTransport::new(
        &crate::transport::HttpConfig {
            base_url,
            timeout_ms,
            ..crate::transport::HttpConfig::default()
        },
        Arc::clone(&auth),
    )
    .expect("transport builds");
    crate::WyrdClient::from_parts(auth, transport, config.grpc)
}

/// A Verifier with one slot of every Run type.
///
/// # Panics
/// Panics when the fixture does not decode.
fn run_verifier() -> wyrd_spec::card::verifier::VerifierSpec {
    serde_json::from_value(json!({
        "inputs": {
            "answer": { "type": "json" },
            "policy": { "type": "media", "formats": ["pdf"], "max_bytes": 64 },
            "deck": { "type": "media", "formats": ["pptx", "pdf"] },
            "copy": { "type": "media", "formats": ["pdf"], "required": false },
            "archive": { "type": "media", "formats": ["pdf"], "required": false },
            "trace": { "type": "trace", "required": false },
        },
        "requirements": [{
            "id": "answered", "statement": "An answer is given",
            "check": { "kind": "assert", "path": "inputs.answer", "operator": "is_not_null" },
        }],
    }))
    .expect("run Verifier decodes")
}

/// An execution target over `spec` with fixed Card UIDs.
///
/// # Panics
/// Panics when the fixed UIDs do not decode.
fn target(spec: &wyrd_spec::card::verifier::VerifierSpec) -> crate::verify::Target<'_> {
    crate::verify::Target {
        spec,
        verifier_card_uid: serde_json::from_value(json!("018f4d8e-0000-7000-8000-000000000001"))
            .expect("Verifier UID"),
        card_uid: serde_json::from_value(json!("018f4d8e-0000-7000-8000-000000000002"))
            .expect("subject UID"),
        run_id: wyrd_spec::vala::ids::RunId::new(),
        group_id: None,
    }
}

/// Project `inputs` onto `spec` as Run evidence.
///
/// # Panics
/// Panics when the projection is refused or is Window evidence.
fn run_evidence(
    spec: &wyrd_spec::card::verifier::VerifierSpec,
    inputs: crate::verify::Inputs,
) -> crate::verify::RunEvidence {
    match target(spec)
        .project(inputs)
        .expect("values fit their slots")
    {
        crate::verify::Projected::Run(run) => run,
        crate::verify::Projected::Window(_) => panic!("expected Run evidence"),
    }
}

/// A passing result body for execution `id`.
fn result_body(id: &wyrd_spec::ids::ExecutionId) -> Value {
    let card = json!({ "kind": "Verifier", "name": "policy-check", "version": "1.0.0" });
    json!({
        "execution_id": id,
        "verifier": card,
        "subject": card,
        "outcome": { "verdict": "passed", "requirements": [], "steps": [], "evidence": {} },
    })
}

/// Samples fill a Window projection as one flat feature object, a
/// single-measurement slot takes a bare value, and a positional value fills a
/// single-slot Verifier; unknown slots, wrong kinds, bad trace IDs,
/// undeclared measurements, and ambiguous positional values are refused.
///
/// # Panics
/// Panics when a projection differs or a refusal passes.
#[test]
fn values_project_onto_named_slots_by_slot_type() {
    let window: wyrd_spec::card::verifier::VerifierSpec = serde_json::from_value(json!({
        "inputs": {
            "current": { "type": "samples", "features": ["income", "dti"] },
            "latency": { "type": "samples", "features": ["latency_p95"] },
        },
    }))
    .expect("window Verifier decodes");
    let projected = target(&window)
        .project(crate::verify::Inputs::named([
            ("current", json!({ "income": 52_000, "dti": 0.31 })),
            ("latency", json!(840)),
        ]))
        .expect("samples fit their slots");
    let crate::verify::Projected::Window(features) = projected else {
        panic!("samples are Window evidence");
    };
    assert_eq!(
        features,
        json!({ "income": 52_000, "dti": 0.31, "latency_p95": 840 })
    );

    let single: wyrd_spec::card::verifier::VerifierSpec =
        serde_json::from_value(json!({ "inputs": { "answer": { "type": "json" } } }))
            .expect("single-slot Verifier decodes");
    run_evidence(&single, json!("yes").into());

    let run = run_verifier();
    let refusals: [(
        &wyrd_spec::card::verifier::VerifierSpec,
        crate::verify::Inputs,
    ); 7] = [
        (&run, crate::verify::Inputs::named([("missing", json!(1))])),
        (
            &run,
            crate::verify::Inputs::named([("policy", json!("policy.pdf"))]),
        ),
        (
            &run,
            crate::verify::Inputs::named([("answer", crate::verify::File::bytes(vec![1]))]),
        ),
        (
            &run,
            crate::verify::Inputs::named([("trace", json!("not-a-trace"))]),
        ),
        (&run, json!("positional needs one slot").into()),
        (
            &window,
            crate::verify::Inputs::named([("current", json!({ "age": 1 }))]),
        ),
        (
            &window,
            crate::verify::Inputs::named([("current", json!(1))]),
        ),
    ];
    for (spec, inputs) in refusals {
        let error = target(spec)
            .project(inputs.clone())
            .expect_err("the value cannot fill its slot");
        assert_eq!(
            error.code(),
            "WYRD_VERIFICATION_400_INPUT_INVALID",
            "{inputs:?}"
        );
    }
}

/// `execute` hashes path and in-memory files during the call, plans each
/// distinct digest once, uploads only the digest the server lacks with one
/// PUT, and sends inline JSON, trace, Artifact, and pinned file
/// declarations in one execution request.
///
/// # Panics
/// Panics when a request or the typed result differs from expected.
#[tokio::test]
async fn execute_pins_files_and_uploads_only_missing_digests() {
    use sha2::Digest as _;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let policy = dir.path().join("Policy.pdf");
    std::fs::write(&policy, b"%PDF-policy").expect("policy written");
    let policy_sha = hex::encode(sha2::Sha256::digest(b"%PDF-policy"));
    let deck_sha = hex::encode(sha2::Sha256::digest(b"%PDF-deck"));
    let plan = wyrd_spec::storage::UploadPlan::SinglePut {
        put_url: format!("{}/bucket/{deck_sha}", server.uri()),
        ttl_secs: 60,
        required_headers: Vec::new(),
    };
    Mock::given(method("POST"))
        .and(path("/v1/verification/evidence"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "uploads": [{ "sha256": deck_sha, "plan": plan }] })),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(format!("/bucket/{deck_sha}")))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    let id = wyrd_spec::ids::ExecutionId::new_v7();
    Mock::given(method("POST"))
        .and(path("/v1/verification/execute"))
        .respond_with(ResponseTemplate::new(200).set_body_json(result_body(&id)))
        .expect(1)
        .mount(&server)
        .await;
    let artifact: wyrd_spec::reference::CardRef =
        serde_json::from_value(json!({ "kind": "Artifact", "name": "decks", "version": "1.0.0" }))
            .expect("Artifact ref decodes");
    let spec = run_verifier();
    let run = run_evidence(
        &spec,
        crate::verify::Inputs::named([
            (
                "answer",
                crate::verify::Input::from(json!({ "text": "yes" })),
            ),
            ("trace", json!("0af7651916cd43dd8448eb211c80319c").into()),
            ("policy", crate::verify::File::path(&policy).into()),
            (
                "deck",
                crate::verify::File::bytes(&b"%PDF-deck"[..])
                    .with_format(skald_spec::MediaFormat::Pdf)
                    .into(),
            ),
            (
                "copy",
                crate::verify::File::bytes(&b"%PDF-policy"[..]).into(),
            ),
            (
                "archive",
                crate::verify::ArtifactRef::new(artifact, Some("q3.pdf".to_owned())).into(),
            ),
        ]),
    );

    let result = target(&spec)
        .execute(&bearer_client(server.uri(), 5_000), run)
        .await
        .expect("the execution succeeds");
    assert_eq!(result.execution_id, id);

    let requests = server.received_requests().await.expect("requests recorded");
    let body = |route: &str| -> Value {
        requests
            .iter()
            .find(|request| request.url.path() == route)
            .map(|request| serde_json::from_slice(&request.body).expect("JSON body"))
            .expect("route was called")
    };
    assert_eq!(
        body("/v1/verification/evidence"),
        json!({ "files": [
            { "sha256": policy_sha, "size_bytes": 11 },
            { "sha256": deck_sha, "size_bytes": 9 },
        ] }),
        "each distinct digest is planned once, in slot order"
    );
    assert_eq!(
        body("/v1/verification/execute")["inputs"],
        json!({
            "answer": { "kind": "json", "value": { "text": "yes" } },
            "archive": { "kind": "artifact", "artifact": { "kind": "Artifact", "name": "decks", "version": "1.0.0" }, "path": "q3.pdf" },
            "copy": { "kind": "file", "sha256": policy_sha, "size_bytes": 11, "format": "pdf" },
            "deck": { "kind": "file", "sha256": deck_sha, "size_bytes": 9, "format": "pdf" },
            "policy": { "kind": "file", "sha256": policy_sha, "size_bytes": 11, "format": "pdf", "filename": "Policy.pdf" },
            "trace": { "kind": "trace", "trace_id": "0af7651916cd43dd8448eb211c80319c" },
        })
    );
}

/// A `File` reads nothing when constructed; an unreadable file, a format
/// its slot does not accept or cannot be inferred, and a file above its
/// slot's `max_bytes` are refused during the call before any request.
///
/// # Panics
/// Panics when a refusal passes or any request reaches the server.
#[tokio::test]
async fn file_refusals_happen_locally_before_any_request() {
    let server = wiremock::MockServer::start().await;
    let client = bearer_client(server.uri(), 5_000);
    let spec = run_verifier();
    for (file, code) in [
        (
            crate::verify::File::path("/no/such/policy.pdf"),
            "WYRD_VERIFICATION_400_INPUT_INVALID",
        ),
        (
            crate::verify::File::bytes(&b"text"[..]).with_filename("policy.txt"),
            "WYRD_VERIFICATION_400_INPUT_INVALID",
        ),
        (
            crate::verify::File::bytes(vec![0_u8; 65]),
            "WYRD_VERIFICATION_413_INPUT_TOO_LARGE",
        ),
    ] {
        let run = run_evidence(&spec, crate::verify::Inputs::named([("policy", file)]));
        let error = target(&spec)
            .execute(&client, run)
            .await
            .expect_err("the file is refused");
        assert_eq!(error.code(), code);
    }
    let run = run_evidence(
        &spec,
        crate::verify::Inputs::named([("deck", crate::verify::File::bytes(vec![1]))]),
    );
    let error = target(&spec)
        .execute(&client, run)
        .await
        .expect_err("a format must be declared when several are accepted");
    assert_eq!(error.code(), "WYRD_VERIFICATION_400_INPUT_INVALID");
    assert!(
        server
            .received_requests()
            .await
            .expect("requests recorded")
            .is_empty(),
        "no refusal reached the server"
    );
}

/// `enqueue` replays one minted `Idempotency-Key` across a retried 503, and
/// `wait` on the returned handle tolerates a not-yet-visible execution, a
/// pending status, and a completion whose result is not visible yet before
/// returning the real result.
///
/// # Panics
/// Panics when the key changes across attempts or `wait` returns early.
#[tokio::test]
async fn enqueue_replays_its_key_and_wait_returns_only_a_real_result() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    let id = wyrd_spec::ids::ExecutionId::new_v7();
    Mock::given(method("POST"))
        .and(path("/v1/verification/enqueue"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/verification/enqueue"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({ "execution_id": id })))
        .mount(&server)
        .await;
    let status = format!("/v1/verification/executions/{id}");
    for answer in [
        ResponseTemplate::new(404)
            .set_body_json(json!({ "code": "WYRD_SPEC_404_NOT_FOUND", "detail": "not yet" })),
        ResponseTemplate::new(200)
            .set_body_json(json!({ "execution_id": id, "status": "pending" })),
        ResponseTemplate::new(200)
            .set_body_json(json!({ "execution_id": id, "status": "completed" })),
    ] {
        Mock::given(method("GET"))
            .and(path(status.as_str()))
            .respond_with(answer)
            .up_to_n_times(1)
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path(status.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "execution_id": id, "status": "completed", "result": result_body(&id),
        })))
        .mount(&server)
        .await;
    let spec = run_verifier();
    let run = run_evidence(
        &spec,
        crate::verify::Inputs::named([("answer", json!("yes"))]),
    );

    let execution = target(&spec)
        .enqueue(&bearer_client(server.uri(), 5_000), run)
        .await
        .expect("the retried submission is staged");
    assert_eq!(execution.id(), &id);
    let result = execution
        .wait(Duration::from_secs(10))
        .await
        .expect("the result arrives");
    assert_eq!(result.execution_id, id);

    let requests = server.received_requests().await.expect("requests recorded");
    let keys: Vec<_> = requests
        .iter()
        .filter(|request| request.url.path() == "/v1/verification/enqueue")
        .map(|request| request.headers.get("idempotency-key").cloned())
        .collect();
    assert_eq!(keys.len(), 2, "the 503 was retried once");
    assert!(keys[0].is_some() && keys[0] == keys[1], "one key: {keys:?}");
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.url.path() == status)
            .count(),
        4
    );
}

/// `wait` raises a settled execution's own error, including an asynchronous
/// idempotency conflict, returns a non-transient read error unchanged, and
/// times out with the last status instead of inventing a result; a handle
/// rebuilt with `connect_with` reads the same execution.
///
/// # Panics
/// Panics when an error code or detail differs from expected.
#[tokio::test]
async fn wait_raises_terminal_errors_and_times_out_without_a_result() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    let client = bearer_client(server.uri(), 5_000);
    let [conflicted, forbidden, pending] =
        std::array::from_fn(|_| wyrd_spec::ids::ExecutionId::new_v7());
    for (id, answer) in [
        (
            &conflicted,
            ResponseTemplate::new(200).set_body_json(json!({
                "execution_id": conflicted, "status": "errored",
                "error": { "code": "WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT", "message": "key reused" },
            })),
        ),
        (
            &forbidden,
            ResponseTemplate::new(403).set_body_json(
                json!({ "code": "WYRD_PERMISSION_403_DENIED_RBAC", "detail": "no read scope" }),
            ),
        ),
        (
            &pending,
            ResponseTemplate::new(200)
                .set_body_json(json!({ "execution_id": pending, "status": "running" })),
        ),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/v1/verification/executions/{id}")))
            .respond_with(answer)
            .mount(&server)
            .await;
    }
    let wait = |id: wyrd_spec::ids::ExecutionId, timeout: Duration| {
        let client = client.clone();
        async move {
            crate::verify::Execution::connect_with(id, &client)
                .await
                .expect("a bearer client connects without a request")
                .wait(timeout)
                .await
                .expect_err("no result")
        }
    };

    let error = wait(conflicted, Duration::from_secs(5)).await;
    assert_eq!(error.code(), "WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT");
    assert_eq!(error.as_problem_json()["details"]["status"], "errored");
    let error = wait(forbidden, Duration::from_secs(5)).await;
    assert_eq!(error.status(), 403);
    let error = wait(pending, Duration::from_millis(300)).await;
    assert_eq!(error.code(), "WYRD_SDK_504_VERIFICATION_WAIT_TIMEOUT");
    assert_eq!(error.as_problem_json()["details"]["last_status"], "running");
}

/// The caller's deadline bounds the whole status read: a completed result
/// that arrives after it, or a read stuck retrying 503s, still times out on
/// time, and a zero budget starts no read at all.
///
/// # Panics
/// Panics when a late result beats the deadline, a wait outlives it, or a
/// zero-budget wait sends a request.
#[tokio::test]
async fn wait_deadline_bounds_slow_reads_and_starts_no_read_after_expiry() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    let client = bearer_client(server.uri(), 10_000);
    let [late, unavailable, unread] =
        std::array::from_fn(|_| wyrd_spec::ids::ExecutionId::new_v7());
    Mock::given(method("GET"))
        .and(path(format!("/v1/verification/executions/{late}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "execution_id": late, "status": "completed", "result": result_body(&late),
                }))
                .set_delay(Duration::from_secs(3)),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/verification/executions/{unavailable}")))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    for id in [late, unavailable] {
        let started = tokio::time::Instant::now();
        let error = crate::verify::Execution::connect_with(id, &client)
            .await
            .expect("a bearer client connects without a request")
            .wait(Duration::from_millis(200))
            .await
            .expect_err("the deadline wins");
        assert_eq!(error.code(), "WYRD_SDK_504_VERIFICATION_WAIT_TIMEOUT");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "the wait outlived its deadline: {:?}",
            started.elapsed()
        );
    }

    let error = crate::verify::Execution::connect_with(unread, &client)
        .await
        .expect("a bearer client connects without a request")
        .wait(Duration::ZERO)
        .await
        .expect_err("an expired budget times out");
    assert_eq!(error.code(), "WYRD_SDK_504_VERIFICATION_WAIT_TIMEOUT");
    let unread_path = format!("/v1/verification/executions/{unread}");
    assert!(
        server
            .received_requests()
            .await
            .expect("requests recorded")
            .iter()
            .all(|request| request.url.path() != unread_path),
        "no read starts after expiry"
    );
}

/// A direct execution waits past a shorter configured `timeout_ms` so the
/// server's own deadline answer reaches the caller, sent once.
///
/// # Panics
/// Panics when the delayed 504 is replaced by a client timeout or replayed.
#[tokio::test]
async fn execute_outlives_the_configured_timeout() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/verification/execute"))
        .respond_with(
            ResponseTemplate::new(504)
                .set_delay(Duration::from_millis(200))
                .set_body_json(json!({
                    "code": "WYRD_VERIFICATION_504_EXECUTION_TIMED_OUT",
                    "detail": "deadline elapsed",
                })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let spec = run_verifier();
    let run = run_evidence(
        &spec,
        crate::verify::Inputs::named([("answer", json!("yes"))]),
    );

    let error = target(&spec)
        .execute(&bearer_client(server.uri(), 20), run)
        .await
        .expect_err("the server's deadline answer is an error");
    assert_eq!(error.code(), "WYRD_VERIFICATION_504_EXECUTION_TIMED_OUT");
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
            row.get("run_id").is_none() && row.get("card_uid").is_none(),
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

// ── Scenario 5: the generic record path ────────────────────────────────────

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
        "vala.eval.observations",
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

/// A state created with a client keeps it for every server call: its clones
/// resolve the same client, which Bifrost startup and Verifier execution use
/// without reading the ambient configuration.
///
/// # Panics
/// Panics when the bundle does not load or the state resolves another client.
#[test]
fn a_state_created_with_a_client_keeps_it_for_every_server_call() {
    let bundle = TestBundle::with_typed_cards();
    let url = "http://127.0.0.1:9";
    let state =
        WyrdState::from_path_with(bundle.path(), client_for(url)).expect("fixture bundle loads");

    assert_eq!(
        state
            .clone()
            .client()
            .expect("the bound client")
            .server_url(),
        url
    );
}
