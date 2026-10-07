//! Observation ingest reliability through a started `WyrdState`.
//!
//! Each test boots a fresh server, registers the shared `observe_a_run`
//! fixture Service, and drives its Model view; every read-back sees only the
//! rows that test wrote, so no query filters by run.

use std::path::PathBuf;
use std::time::Duration;

use secrecy::ExposeSecret;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use wyrd_client::bifrost::TableConfig;
use wyrd_client::cards::{CardGraphHydrator, CardSelector, Cards, HydrationMode};
use wyrd_client::observe::{EvalObservationOptions, Run};
use wyrd_client::state::WyrdState;
use wyrd_client::{Bifrost, QueueConfig, WyrdClient};
use wyrd_testing::Bootstrap;
use wyrd_testing::WyrdTestServer;

/// The dataset the emit-time test records into.
const DATASET: &str = "vala.datasets.event_times";

/// One stored `wyrd_event_time`, as microseconds since the Unix epoch.
#[derive(Debug, Deserialize)]
struct EventTime {
    /// `CAST(wyrd_event_time AS BIGINT)` of one row.
    event_time: i64,
}

/// One `record_id` group of a drift read-back.
#[derive(Debug, Deserialize)]
struct RecordCount {
    /// Rows that share this observation's `record_id`.
    n: i64,
}

/// A server with the observed Service registered and hydrated, the
/// administrator's client, and a started state keyed to the Service.
struct Observed {
    /// The server every call reaches.
    server: WyrdTestServer,
    /// The administrator's client, for setup and read-back.
    admin: WyrdClient,
    /// The started state over the hydrated bundle.
    state: WyrdState,
    /// Keeps the hydrated bundle on disk for the state's lifetime.
    _bundle: tempfile::TempDir,
}

impl Observed {
    /// Boot a server, register and hydrate the fixture Service, and start
    /// Bifrost on the bundle with the Service's own key and `queue`.
    ///
    /// # Panics
    /// Panics when a setup step fails.
    async fn start(queue: QueueConfig) -> Self {
        let server = Box::pin(WyrdTestServer::start_bound())
            .await
            .expect("test server starts");
        let admin = connect(
            &server,
            &api_key(
                server
                    .bootstrap_service("observe_ingest_admin", &["admin"])
                    .await
                    .expect("administrator bootstraps"),
            ),
        );
        let cards = Cards::with_client(WyrdClient::clone(&admin));
        Box::pin(cards.register_from_path(&fixture("observed-model.yaml")))
            .await
            .expect("the Model registers");
        let receipt = Box::pin(cards.register_from_path(&fixture("observed-service.yaml")))
            .await
            .expect("the Service registers");
        let bundle = tempfile::tempdir().expect("bundle directory creates");
        Box::pin(CardGraphHydrator::new(cards.registry_context()).hydrate(
            &CardSelector::exact(receipt.root.clone()),
            &bundle.path().join("bundle"),
            HydrationMode::Complete,
        ))
        .await
        .expect("the Service hydrates");
        let service = connect(
            &server,
            &api_key(
                server
                    .credential_registered_service(&receipt.root, &[])
                    .await
                    .expect("the Service is credentialed"),
            ),
        );
        let state =
            WyrdState::from_path(&bundle.path().join("bundle")).expect("bundle loads offline");
        state
            .start_bifrost_with_config(&service, None, queue)
            .await
            .expect("Bifrost starts");
        Self {
            server,
            admin,
            state,
            _bundle: bundle,
        }
    }

    /// The run's Model view.
    ///
    /// # Panics
    /// Panics when the bundle has no `model` alias.
    fn model(&self) -> Run {
        self.state
            .run()
            .for_card("model")
            .expect("model view resolves")
    }

    /// Stop the state, publish everything it wrote, and return the
    /// administrator's rows for `sql`.
    ///
    /// # Panics
    /// Panics when the drain, publication, or query fails.
    async fn drain_and_read<T: serde::de::DeserializeOwned>(&self, sql: &str) -> Vec<T> {
        self.state.shutdown().await.expect("the state drains");
        self.server.flush_bifrost().await.expect("rows publish");
        self.read(sql).await
    }

    /// The administrator's rows for `sql`.
    ///
    /// # Panics
    /// Panics when the query fails.
    async fn read<T: serde::de::DeserializeOwned>(&self, sql: &str) -> Vec<T> {
        Bifrost::query_only(&self.admin)
            .sql_as(sql, &[])
            .await
            .unwrap_or_else(|error| panic!("{sql} reads back: {error}"))
    }
}

/// Path of a shared `observe_a_run` fixture Card.
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../fixtures/cards/observe_a_run"
    ))
    .join(name)
}

/// A client of `server` that authenticates with `credential`.
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

/// Drift features `prefix0..prefixN`, valued by `value(feature)`.
fn features(count: u16, prefix: &str, value: impl Fn(u16) -> f64) -> Value {
    Value::Object(
        (0..count)
            .map(|feature| (format!("{prefix}{feature}"), json!(value(feature))))
            .collect::<Map<_, _>>(),
    )
}

/// Emit one drift observation, flushing and resubmitting it on `QUEUE_FULL`.
///
/// Admission is all-or-none per observation, so a refused observation
/// admitted no row and resubmitting it after the flush duplicates nothing.
///
/// # Panics
/// Panics when the emit fails with anything but `QUEUE_FULL`, or the flush
/// fails.
async fn emit_with_resubmit(state: &WyrdState, view: &Run, features: &Value) {
    loop {
        match view.observe().drift(features, None) {
            Ok(()) => return,
            Err(error) if error.code() == "WYRD_CLIENT_429_QUEUE_FULL" => {
                state.flush().await.expect("a flush frees the budget");
            }
            Err(error) => panic!("drift emit failed: {error:?}"),
        }
    }
}

/// Fail unless every drift observation landed as one `record_id` of exactly
/// `features` rows, `observations` times.
///
/// # Panics
/// Panics when an observation was lost, duplicated, or split.
fn assert_exactly_once(groups: &[RecordCount], observations: u16, features: u16) {
    assert_eq!(
        groups.len(),
        usize::from(observations),
        "one record_id per observation"
    );
    assert!(
        groups.iter().all(|group| group.n == i64::from(features)),
        "every observation landed all of its features exactly once"
    );
}

/// Every observation stores the client clock reading of its emit call as
/// `wyrd_event_time`, not Scribe's receipt instant, and a caller-supplied
/// `wyrd_event_time` in a record row is stored unchanged.
///
/// Scribe's receipt clock runs one day ahead for the whole emit-to-ingest
/// window, so a receipt stamp could never fall between the client readings
/// taken around the emits.
///
/// # Panics
/// Panics when a step fails or a stored event time differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn observations_store_the_client_emit_time() {
    let observed = Observed::start(QueueConfig::default()).await;
    let table = TableConfig::from_json_schema(
        DATASET,
        &json!({
            "type": "object",
            "properties": { "value": { "type": "integer" } },
            "required": ["value"],
        }),
    )
    .expect("dataset declares");
    let registrar = Bifrost::connect_with_table(&observed.admin, table)
        .await
        .expect("registrar connects");
    registrar.register().await.expect("dataset registers");
    registrar.shutdown().await.expect("registrar stops");
    let scribe = observed
        .server
        .bifrost_scribe()
        .expect("the server owns a Scribe");
    let model = observed.model();

    scribe.shift_receipt_clock_for_test(Duration::from_hours(24));
    let before = chrono::Utc::now().timestamp_micros();
    model
        .observe()
        .drift(&json!({ "latency": 12.5, "tier": "gold" }), None)
        .expect("drift emits");
    model
        .observe()
        .eval(
            &json!({ "answer": "yes" }),
            EvalObservationOptions::default(),
        )
        .expect("eval emits");
    model
        .observe()
        .record(DATASET, &json!({ "value": 1 }))
        .await
        .expect("record emits");
    let after = chrono::Utc::now().timestamp_micros();
    let supplied = chrono::DateTime::from_timestamp_micros(before - 3_600_000_000)
        .expect("an hour ago is representable");
    model
        .observe()
        .record(
            DATASET,
            &json!({
                "value": 2,
                "wyrd_event_time": supplied.to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
            }),
        )
        .await
        .expect("a caller event time emits");
    let mut emitted: Vec<EventTime> = observed
        .drain_and_read(
            "SELECT CAST(wyrd_event_time AS BIGINT) AS event_time FROM vala.drift.observations",
        )
        .await;
    scribe.shift_receipt_clock_for_test(Duration::ZERO);
    for sql in [
        "SELECT CAST(wyrd_event_time AS BIGINT) AS event_time FROM vala.eval.observations",
        "SELECT CAST(wyrd_event_time AS BIGINT) AS event_time \
         FROM vala.datasets.event_times WHERE value = 1",
    ] {
        emitted.extend(observed.read::<EventTime>(sql).await);
    }
    let kept: Vec<EventTime> = observed
        .read(
            "SELECT CAST(wyrd_event_time AS BIGINT) AS event_time \
             FROM vala.datasets.event_times WHERE value = 2",
        )
        .await;

    assert_eq!(emitted.len(), 4, "two drift rows, one eval, one record");
    for row in &emitted {
        assert!(
            (before..=after).contains(&row.event_time),
            "stored {} is the emit-time reading in [{before}, {after}], not receipt",
            row.event_time
        );
    }
    assert_eq!(
        kept.iter().map(|row| row.event_time).collect::<Vec<_>>(),
        [supplied.timestamp_micros()]
    );
    observed.server.shutdown().await.expect("server stops");
}

/// A 1,000-observation × 9-feature drift burst through a 16 MiB byte-budget
/// override lands every observation exactly once.
///
/// # Panics
/// Panics when the burst loses, duplicates, or splits an observation.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn drift_burst_survives_a_byte_budget_override() {
    const OBSERVATIONS: u16 = 1_000;
    const FEATURES: u16 = 9;
    let observed =
        Observed::start(QueueConfig::with_client_byte_limit(Some(16 * 1024 * 1024))).await;
    let model = observed.model();

    for observation in 0..OBSERVATIONS {
        let features = features(FEATURES, "feature_", |feature| {
            f64::from(observation) + f64::from(feature) / 10.0
        });
        emit_with_resubmit(&observed.state, &model, &features).await;
    }
    let groups: Vec<RecordCount> = observed
        .drain_and_read("SELECT COUNT(*) AS n FROM vala.drift.observations GROUP BY record_id")
        .await;

    assert_exactly_once(&groups, OBSERVATIONS, FEATURES);
    observed.server.shutdown().await.expect("server stops");
}

/// One state with the default queue emits paced 100-feature drift
/// observations for 15 seconds: client-owned bytes stay flat (the median of
/// the last third of samples stays within one message of the first third's),
/// drain to zero, and every observation lands exactly once.
///
/// The comparison uses medians because a seal reserves a full
/// `max_message_bytes` before encoding. A sample that lands mid-encode reads
/// that reservation on top of the staged rows, so a third's maximum depends on
/// sampling luck. A backlog raises every later sample and moves the median.
///
/// # Panics
/// Panics when the client bytes grow, the drained queue owns bytes, or an
/// observation is lost, duplicated, or split.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn sustained_hundred_feature_drift_lands_exactly_once_with_flat_client_bytes() {
    const FEATURES: u16 = 100;
    const RATE: u16 = 100;
    const SECONDS: u16 = 15;
    const SAMPLE: Duration = Duration::from_millis(250);
    let observed = Observed::start(QueueConfig::default()).await;
    let model = observed.model();
    let owned_bytes = || {
        observed
            .state
            .bifrost_metrics()
            .expect("a started state reports its queue")
            .owned_bytes
    };

    let mut samples = Vec::new();
    let began = tokio::time::Instant::now();
    let mut next_sample = began;
    for observation in 0..RATE * SECONDS {
        // Pacing is the workload under test, not a wait for state.
        tokio::time::sleep_until(
            began + Duration::from_secs_f64(f64::from(observation) / f64::from(RATE)),
        )
        .await;
        if tokio::time::Instant::now() >= next_sample {
            samples.push(owned_bytes());
            next_sample += SAMPLE;
        }
        let features = features(FEATURES, "f", |feature| {
            f64::from((observation + feature) % 100)
        });
        emit_with_resubmit(&observed.state, &model, &features).await;
    }
    let third = samples.len() / 3;
    let early = median(&samples[..third]);
    let late = median(&samples[samples.len() - third..]);
    observed.state.flush().await.expect("the emission drains");
    let drained = owned_bytes();
    let groups: Vec<RecordCount> = observed
        .drain_and_read("SELECT COUNT(*) AS n FROM vala.drift.observations GROUP BY record_id")
        .await;

    assert!(
        late <= early + QueueConfig::default().max_message_bytes,
        "client-owned bytes grew: early median {early}, late median {late}"
    );
    assert_eq!(drained, 0, "a drained queue owns no bytes");
    assert_exactly_once(&groups, RATE * SECONDS, FEATURES);
    observed.server.shutdown().await.expect("server stops");
}

/// Returns the median of `samples`, or zero for an empty slice.
///
/// An even count takes the upper middle value, so the result is always an
/// observed sample.
fn median(samples: &[usize]) -> usize {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    sorted.get(sorted.len() / 2).copied().unwrap_or(0)
}
