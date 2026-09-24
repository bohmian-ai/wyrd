//! Operator delivery journeys through a real bound server, its verification
//! runtime, and local mock providers.
//!
//! A tenant administrator creates Slack, PagerDuty, and HTTP connections and
//! registers Operator Cards over HTTP; a scheduled binding's failed Drift
//! verdict fans out one dispatch per Operator, and the Operator worker
//! delivers each to a `wiremock` provider. Retry eligibility is PostgreSQL's,
//! so a test that needs a retry to come due moves the dispatch row itself.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use url::Url;
use uuid::Uuid;
use vala_drift::{DriftReport, DriftVerdict, FeatureDriftReport};
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, Request as MockRequest, ResponseTemplate};
use wyrd_server::components::operators::keys::OperatorKeys;
use wyrd_server::config::{OperatorKeySource, OperatorKeysConfig, VaultKeysConfig};
use wyrd_server::state::AppState;
use wyrd_server::verification::engines::{EngineOutcome, VerifierReport};
use wyrd_server::verification::health::RuntimeCapability;
use wyrd_server::verification::operators::ProviderEndpoints;
use wyrd_server::verification::runner::EngineScript;
use wyrd_server::verification::{CapabilityCrash, RuntimeLimits, VerificationRuntime};
use wyrd_spec::DataTenantId;
use wyrd_spec::card::drift::DriftMethod;
use wyrd_spec::ids::{CardUid, FeatureName, VerificationRunId};
use wyrd_spec::verification::FrozenTarget;
use wyrd_testing::verification::VerificationFixture;
use wyrd_testing::{Bootstrap, WyrdTestServer};

/// Upper bound on every wait for the runtime to make progress.
const WAIT: Duration = Duration::from_secs(30);
/// Slack bot token of the fixture connection.
const SLACK_TOKEN: &str = "xoxb-delivery-slack-token";
/// Slack bot token after rotation.
const ROTATED_TOKEN: &str = "xoxb-rotated-slack-token";
/// PagerDuty Global Integration key of the fixture connection.
const PAGER_KEY: &str = "pd-delivery-integration-key";
/// HTTP header credential of the fixture connection.
const HOOK_KEY: &str = "hook-delivery-header-key";

/// One dispatch's durable delivery state.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
struct DispatchRow {
    /// Operator Card the dispatch delivers.
    operator_uid: Uuid,
    /// Dispatch identity, also the PagerDuty dedup key and Idempotency-Key.
    dispatch_id: Uuid,
    /// Stored status.
    status: String,
    /// Attempts charged.
    attempts: i32,
    /// Stored error code of a retried or failed attempt.
    error_code: Option<String>,
    /// Seconds from the settling update to the next eligible attempt.
    delay: Option<f64>,
}

/// A bound server, its seeded tenant, an administrator, and mock providers.
struct Delivery {
    /// The production bound server.
    server: WyrdTestServer,
    /// Seeded verification state of the fixture tenant.
    seed: VerificationFixture,
    /// Drift Verifier every binding runs.
    verifier: CardUid,
    /// Tenant administrator token.
    jwt: String,
    /// Superuser pool for dispatch inspection and deadline control.
    assertion: PgPool,
    /// Local Slack, PagerDuty, and HTTP provider.
    mock: MockServer,
}

impl Delivery {
    /// Boot a bound server, seed a Verifier, create the three connections
    /// against the mock provider, and return the harness.
    ///
    /// # Panics
    /// Panics when startup, seeding, or a connection create fails.
    async fn start() -> Self {
        let delivery = Self::boot().await;
        delivery
            .connect(&[
                json!({ "provider": "slack", "name": "ops-slack", "workspace_id": "T0001",
                        "bot_token": SLACK_TOKEN }),
                json!({ "provider": "pager_duty", "name": "ops-pagerduty",
                        "integration_key": PAGER_KEY }),
                json!({ "provider": "http", "name": "ops-hooks", "origin": delivery.mock.uri(),
                        "auth": { "scheme": "header", "name": "X-Api-Key", "value": HOOK_KEY } }),
            ])
            .await;
        delivery
    }

    /// Boot a bound server, seed a Verifier and an administrator, and start
    /// the mock provider; creates no connection.
    ///
    /// # Panics
    /// Panics when startup or seeding fails.
    async fn boot() -> Self {
        let server = WyrdTestServer::builder()
            .without_audit_publication_for_test()
            .start_bound()
            .await
            .expect("bound server starts");
        let tenant = server.pg_fixture().data_tenant_id();
        let seed = VerificationFixture::provision(server.state().postgres.wyrd(), tenant)
            .await
            .expect("tenant provisions");
        let verifier = seed
            .drift_verifier("drift")
            .await
            .expect("verifier registers");
        let jwt = match server
            .bootstrap_user("operator-admin", &["admin"])
            .await
            .expect("admin bootstraps")
        {
            Bootstrap::User { jwt, .. } => jwt,
            Bootstrap::Machine { .. } => panic!("admin bootstrap returned a machine"),
        };
        let assertion = server
            .pg_fixture()
            .superuser_pool()
            .await
            .expect("fixture exposes a superuser pool");
        Self {
            server,
            seed,
            verifier,
            jwt,
            assertion,
            mock: MockServer::start().await,
        }
    }

    /// Create every connection in `bodies` as the administrator.
    ///
    /// # Panics
    /// Panics when a create is refused.
    async fn connect(&self, bodies: &[Value]) {
        for body in bodies {
            let (status, view) = self
                .call(Method::POST, "/v1/operator-connections", Some(body))
                .await;
            assert_eq!(status, StatusCode::CREATED, "{view}");
        }
    }

    /// Send `method uri` with an optional JSON body as the administrator.
    ///
    /// # Panics
    /// Panics when the request fails or the response is not JSON.
    async fn call(&self, method: Method, uri: &str, body: Option<&Value>) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .header("Idempotency-Key", Uuid::now_v7().to_string())
            .body(body.map_or_else(Body::empty, |body| Body::from(body.to_string())))
            .expect("request builds");
        let response = self
            .server
            .oneshot_authenticated(&self.jwt, request)
            .await
            .expect("request responds");
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 1 << 20)
            .await
            .expect("body reads");
        (
            status,
            serde_json::from_slice(&bytes).expect("body is JSON"),
        )
    }

    /// Register an Operator Card named `name` and return its UID.
    ///
    /// # Panics
    /// Panics when registration is refused.
    async fn operator(&self, name: &str, spec: Value) -> CardUid {
        let body = json!({ "submissions": [{
            "apiVersion": "wyrd/v1", "kind": "Operator",
            "metadata": { "name": name, "version": "1.0.0", "space": "default" },
            "spec": spec, "artifacts": []
        }] });
        let (status, body) = self.call(Method::POST, "/v1/cards", Some(&body)).await;
        assert_eq!(status, StatusCode::CREATED, "{name}: {body}");
        serde_json::from_value(body["outcomes"][0]["card_ref"]["uid"].clone()).expect("uid")
    }

    /// Bind the Verifier to a new owner Service named `name` dispatching
    /// `operators` and make it due; returns the owner.
    ///
    /// # Panics
    /// Panics when seeding fails.
    async fn fail_binding(&self, name: &str, operators: &[&CardUid]) -> CardUid {
        let (owner, principal) = self.seed.service(name).await.expect("owner registers");
        let binding = self
            .seed
            .bind_schedule(
                &owner,
                &owner,
                &self.verifier,
                "0 2 * * *",
                operators
                    .iter()
                    .map(|uid| FrozenTarget::Uid((*uid).clone()))
                    .collect(),
            )
            .await
            .expect("binding projects");
        self.seed
            .activate(principal)
            .await
            .expect("owner activates");
        self.seed
            .make_binding_due(binding)
            .await
            .expect("binding is due");
        owner
    }

    /// Poll until the scheduler created a run not in `known` and return it.
    ///
    /// # Panics
    /// Panics when no new run appears within [`WAIT`].
    async fn new_run(&self, known: &[VerificationRunId]) -> VerificationRunId {
        let deadline = tokio::time::Instant::now() + WAIT;
        loop {
            let runs = self.seed.runs().await.expect("runs read");
            if let Some(run) = runs.into_iter().find(|run| !known.contains(run)) {
                return run;
            }
            assert!(tokio::time::Instant::now() < deadline, "no run scheduled");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Runtime bounds fast enough for tests.
    fn limits() -> RuntimeLimits {
        RuntimeLimits {
            lease: Duration::from_secs(60),
            execution_timeout: Duration::from_secs(20),
            publication_timeout: Duration::from_secs(20),
            drain_grace: Duration::from_secs(10),
            poll_interval: Duration::from_millis(50),
            restart_backoff: Duration::from_millis(300),
            ..RuntimeLimits::default()
        }
    }

    /// Spawn a runtime over `state` delivering to the mock provider.
    ///
    /// # Panics
    /// Panics when the runtime cannot compose.
    fn spawn(
        &self,
        state: &AppState,
        limits: RuntimeLimits,
        script: &EngineScript,
        crash: &CapabilityCrash,
    ) -> Running {
        let base = Url::parse(&self.mock.uri()).expect("mock URI");
        self.spawn_to(
            state,
            limits,
            script,
            crash,
            ProviderEndpoints {
                slack: base.join("/slack").expect("slack URL"),
                pager_duty: base.join("/pagerduty").expect("pagerduty URL"),
            },
        )
    }

    /// Spawn a runtime over `state` delivering fixed providers to `endpoints`.
    ///
    /// # Panics
    /// Panics when the runtime cannot compose.
    fn spawn_to(
        &self,
        state: &AppState,
        limits: RuntimeLimits,
        script: &EngineScript,
        crash: &CapabilityCrash,
        endpoints: ProviderEndpoints,
    ) -> Running {
        let runtime = VerificationRuntime::builder(state)
            .limits(limits)
            .ingest_endpoint(self.server.grpc_url().expect("bound server serves gRPC"))
            .engine_script(script.clone())
            .crash_switch(crash.clone())
            .provider_endpoints(endpoints)
            .build()
            .expect("the runtime composes");
        let stop = CancellationToken::new();
        let task = tokio::spawn(runtime.run(stop.clone()));
        Running { stop, task }
    }

    /// Every dispatch of `run`, keyed by its Operator Card UID.
    ///
    /// # Panics
    /// Panics when the dispatch table cannot be read.
    async fn dispatches(&self, run: VerificationRunId) -> BTreeMap<Uuid, DispatchRow> {
        let rows: Vec<DispatchRow> = sqlx::query_as(
            "SELECT operator_uid, dispatch_id, status, attempts, \
                    last_error->>'code' AS error_code, \
                    EXTRACT(EPOCH FROM next_attempt_at - updated_at)::float8 AS delay \
               FROM wyrd.operator_dispatches WHERE run_id = $1",
        )
        .bind(run.as_uuid())
        .fetch_all(&self.assertion)
        .await
        .expect("dispatches read");
        rows.into_iter()
            .map(|row| (row.operator_uid, row))
            .collect()
    }

    /// Poll `run`'s dispatches until `done` holds for every one.
    ///
    /// # Panics
    /// Panics when `done` never holds within [`WAIT`].
    async fn wait_dispatches(
        &self,
        run: VerificationRunId,
        count: usize,
        done: impl Fn(&DispatchRow) -> bool,
    ) -> BTreeMap<Uuid, DispatchRow> {
        let deadline = tokio::time::Instant::now() + WAIT;
        loop {
            let rows = self.dispatches(run).await;
            if rows.len() == count && rows.values().all(&done) {
                return rows;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "dispatches never settled: {rows:?}"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Bring every retrying dispatch of `run` due in the database.
    ///
    /// # Panics
    /// Panics when the update fails.
    async fn make_retries_due(&self, run: VerificationRunId) {
        sqlx::query(
            "UPDATE wyrd.operator_dispatches SET next_attempt_at = statement_timestamp() \
              WHERE run_id = $1 AND status = 'retrying'",
        )
        .bind(run.as_uuid())
        .execute(&self.assertion)
        .await
        .expect("retries come due");
    }

    /// Bodies of every request the mock received at `route`.
    ///
    /// # Panics
    /// Panics when request recording is disabled.
    async fn requests(&self, route: &str) -> Vec<MockRequest> {
        self.mock
            .received_requests()
            .await
            .expect("requests are recorded")
            .into_iter()
            .filter(|request| request.url.path() == route)
            .collect()
    }
}

/// A spawned runtime and the token that stops it.
struct Running {
    /// Cancels the runtime.
    stop: CancellationToken,
    /// The supervised runtime task.
    task: JoinHandle<()>,
}

impl Running {
    /// Cancel the runtime and wait for its drain.
    ///
    /// # Panics
    /// Panics when the runtime panics or does not drain within [`WAIT`].
    async fn stop(self) {
        self.stop.cancel();
        tokio::time::timeout(WAIT, self.task)
            .await
            .expect("the runtime drains")
            .expect("the runtime does not panic");
    }
}

/// A script whose next `runs` executions each return a failing Drift verdict.
///
/// # Panics
/// Panics when a static feature name is invalid.
fn failing_script(runs: usize) -> EngineScript {
    let feature = FeatureName::new("latency").expect("feature name");
    let report = DriftReport {
        method: DriftMethod::Custom,
        features: [(
            feature.clone(),
            FeatureDriftReport {
                feature,
                score: 3.0,
                threshold: 1.0,
                verdict: DriftVerdict::Drift,
            },
        )]
        .into_iter()
        .collect(),
        verdict: DriftVerdict::Drift,
    };
    let script = EngineScript::default();
    for _ in 0..runs {
        script.push(EngineOutcome::Completed(VerifierReport::Drift(Some(
            report.clone(),
        ))));
    }
    script
}

/// A Slack Operator posting to `channel_id`.
fn slack(channel_id: &str) -> Value {
    json!({ "kind": "notify", "channel": { "kind": "slack", "connection": "ops-slack",
            "channel_id": channel_id, "text": "{{verifier_ref}} failed {{subject_ref}}" } })
}

/// An HTTP Operator posting to `route` on the mock origin.
fn hook(origin: &str, route: &str) -> Value {
    json!({ "kind": "http", "method": "post", "url": format!("{origin}{route}"),
            "auth": { "scheme": "header", "name": "x-api-key", "connection": "ops-hooks" },
            "body": { "verdict": "{{verdict}}", "run": "{{run_id}}" } })
}

/// Poll `ready` until it holds.
///
/// # Panics
/// Panics when `ready` never holds within [`WAIT`].
async fn wait_until(what: &str, ready: impl Fn() -> bool) {
    let deadline = tokio::time::Instant::now() + WAIT;
    while !ready() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// A failed verdict fans out one dispatch per Operator; each provider gets its
/// tenant credential and authored target, and every dispatch settles on its
/// own — delivered, terminal provider refusal, rate-limited retry honoring
/// `Retry-After`, or an origin-changing redirect refused — without touching
/// the completed Verifier run.
///
/// # Panics
/// Panics when a dispatch settles differently or a provider request differs.
#[tokio::test]
async fn failed_verdict_fans_out_to_every_provider_independently() {
    let delivery = Delivery::start().await;
    let origin = delivery.mock.uri();
    Mock::given(method("POST"))
        .and(path("/slack"))
        .and(body_partial_json(json!({ "channel": "CGONE" })))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "ok": false, "error": "channel_not_found" })),
        )
        .with_priority(1)
        .mount(&delivery.mock)
        .await;
    Mock::given(method("POST"))
        .and(path("/slack"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "ok": true })))
        .mount(&delivery.mock)
        .await;
    Mock::given(method("POST"))
        .and(path("/pagerduty"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({ "status": "success" })))
        .mount(&delivery.mock)
        .await;
    Mock::given(method("POST"))
        .and(path("/hook/limited"))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "90"))
        .mount(&delivery.mock)
        .await;
    Mock::given(method("POST"))
        .and(path("/hook/moved"))
        .respond_with(
            ResponseTemplate::new(307).insert_header("Location", "https://elsewhere.example.com/x"),
        )
        .mount(&delivery.mock)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&delivery.mock)
        .await;

    let posted = delivery.operator("posted", slack("C0123456789")).await;
    let gone = delivery.operator("gone", slack("CGONE")).await;
    let paged = delivery
        .operator(
            "paged",
            json!({ "kind": "notify", "channel": { "kind": "pager_duty",
                    "connection": "ops-pagerduty", "route": "retention",
                    "severity": "warning", "summary": "{{verifier_ref}} failed" } }),
        )
        .await;
    let hooked = delivery
        .operator("hooked", hook(&origin, "/hook/{{subject_uid}}"))
        .await;
    let limited = delivery
        .operator("limited", hook(&origin, "/hook/limited"))
        .await;
    let moved = delivery
        .operator("moved", hook(&origin, "/hook/moved"))
        .await;
    let owner = delivery
        .fail_binding(
            "owner",
            &[&posted, &gone, &paged, &hooked, &limited, &moved],
        )
        .await;
    let running = delivery.spawn(
        delivery.server.state(),
        Delivery::limits(),
        &failing_script(1),
        &CapabilityCrash::default(),
    );
    let run = delivery.new_run(&[]).await;
    let rows = delivery
        .wait_dispatches(run, 6, |row| {
            matches!(row.status.as_str(), "delivered" | "failed" | "retrying")
        })
        .await;
    running.stop().await;

    let settled = |uid: &CardUid| {
        let row = &rows[&uid.as_uuid()];
        (row.status.as_str(), row.attempts, row.error_code.as_deref())
    };
    assert_eq!(settled(&posted), ("delivered", 1, None));
    assert_eq!(settled(&gone), ("failed", 1, Some("provider_rejected")));
    assert_eq!(settled(&paged), ("delivered", 1, None));
    assert_eq!(settled(&hooked), ("delivered", 1, None));
    assert_eq!(
        settled(&limited),
        ("retrying", 1, Some("provider_transient"))
    );
    assert_eq!(settled(&moved), ("failed", 1, Some("destination_rejected")));
    let wait = rows[&limited.as_uuid()].delay.expect("retry is scheduled");
    assert!(
        (89.0..=91.0).contains(&wait),
        "Retry-After 90s over the 30s backoff: {wait}"
    );

    let slack_posts = delivery.requests("/slack").await;
    assert_eq!(slack_posts.len(), 2);
    for post in &slack_posts {
        assert_eq!(
            post.headers
                .get("authorization")
                .and_then(|v| v.to_str().ok()),
            Some(format!("Bearer {SLACK_TOKEN}").as_str())
        );
    }
    let channels: Vec<Value> = slack_posts
        .iter()
        .map(|post| post.body_json::<Value>().expect("slack JSON")["channel"].clone())
        .collect();
    assert!(channels.contains(&json!("C0123456789")) && channels.contains(&json!("CGONE")));

    let pages = delivery.requests("/pagerduty").await;
    assert_eq!(pages.len(), 1);
    let page: Value = pages[0].body_json().expect("pagerduty JSON");
    assert_eq!(page["routing_key"], PAGER_KEY);
    assert_eq!(page["event_action"], "trigger");
    assert_eq!(
        page["dedup_key"],
        rows[&paged.as_uuid()].dispatch_id.to_string(),
        "an unauthored dedup key is the stable dispatch ID"
    );
    assert_eq!(page["payload"]["custom_details"]["wyrd_route"], "retention");
    assert_eq!(page["payload"]["severity"], "warning");

    let hooks = delivery
        .requests(&format!("/hook/{}", owner.as_uuid()))
        .await;
    assert_eq!(hooks.len(), 1, "the effective URL renders the subject UID");
    let hook_request = &hooks[0];
    assert_eq!(
        hook_request
            .headers
            .get("x-api-key")
            .and_then(|v| v.to_str().ok()),
        Some(HOOK_KEY)
    );
    assert_eq!(
        hook_request
            .headers
            .get("idempotency-key")
            .and_then(|v| v.to_str().ok()),
        Some(rows[&hooked.as_uuid()].dispatch_id.to_string().as_str())
    );
    let hook_body: Value = hook_request.body_json().expect("hook JSON");
    assert_eq!(hook_body["verdict"], "failed");
    assert_eq!(hook_body["run"], run.as_uuid().to_string());

    let settled_run = delivery.seed.run(run).await.expect("run reads");
    assert_eq!(settled_run.status, "completed");
    assert_eq!(delivery.seed.runs().await.expect("runs read"), vec![run]);
}

/// A transient failure retries after the 30-second backoff; the credential is
/// rotated in Postgres and a second replica, on a newer key version, delivers
/// the next attempt with the rotated token and rewraps the stored credential,
/// with no Card revision.
///
/// # Panics
/// Panics when the retry schedule, rotation, or rewrap differs.
#[tokio::test]
async fn next_attempt_on_another_replica_uses_the_rotated_credential() {
    let delivery = Delivery::start().await;
    Mock::given(method("POST"))
        .and(path("/slack"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&delivery.mock)
        .await;
    Mock::given(method("POST"))
        .and(path("/slack"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "ok": true })))
        .mount(&delivery.mock)
        .await;
    let posted = delivery.operator("posted", slack("C0123456789")).await;
    delivery.fail_binding("owner", &[&posted]).await;
    let first = delivery.spawn(
        delivery.server.state(),
        Delivery::limits(),
        &failing_script(1),
        &CapabilityCrash::default(),
    );
    let run = delivery.new_run(&[]).await;
    let rows = delivery
        .wait_dispatches(run, 1, |row| row.status == "retrying")
        .await;
    first.stop().await;
    let row = &rows[&posted.as_uuid()];
    assert_eq!(
        (row.attempts, row.error_code.as_deref()),
        (1, Some("provider_transient"))
    );
    let wait = row.delay.expect("retry is scheduled");
    assert!(
        (29.0..=31.0).contains(&wait),
        "first backoff is 30s: {wait}"
    );

    let (_, list) = delivery
        .call(Method::GET, "/v1/operator-connections", None)
        .await;
    let slack_id = list
        .as_array()
        .or_else(|| list["connections"].as_array())
        .expect("connection list")
        .iter()
        .find(|view| view["provider"] == "slack")
        .and_then(|view| view["connection_id"].as_str())
        .expect("slack connection")
        .to_owned();
    let (status, view) = delivery
        .call(
            Method::PATCH,
            &format!("/v1/operator-connections/{slack_id}"),
            Some(&json!({ "provider": "slack", "bot_token": ROTATED_TOKEN })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{view}");

    let dir = delivery
        .server
        .operator_keys_dir_for_test()
        .expect("generated key directory");
    std::fs::copy(dir.join("v1"), dir.join("v2")).expect("key v2 publishes");
    let mut replica = delivery.server.state().clone();
    replica.operator_keys = Arc::new(
        OperatorKeys::new(OperatorKeysConfig {
            source: OperatorKeySource::File,
            dir: Some(dir.to_path_buf()),
            active_version: std::num::NonZeroU32::new(2).expect("nonzero"),
            ..OperatorKeysConfig::default()
        })
        .expect("file keys build"),
    );
    delivery.make_retries_due(run).await;
    let second = delivery.spawn(
        &replica,
        Delivery::limits(),
        &EngineScript::default(),
        &CapabilityCrash::default(),
    );
    let rows = delivery
        .wait_dispatches(run, 1, |row| row.status == "delivered")
        .await;
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let version: i32 = sqlx::query_scalar(
            "SELECT key_version FROM wyrd.operator_connections WHERE connection_id = $1::uuid",
        )
        .bind(&slack_id)
        .fetch_one(&delivery.assertion)
        .await
        .expect("key version reads");
        if version == 2 {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "credential never rewrapped"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    second.stop().await;
    assert_eq!(rows[&posted.as_uuid()].attempts, 2);
    let tokens: Vec<String> = delivery
        .requests("/slack")
        .await
        .iter()
        .filter_map(|post| {
            post.headers
                .get("authorization")?
                .to_str()
                .ok()
                .map(str::to_owned)
        })
        .collect();
    assert_eq!(
        tokens,
        vec![
            format!("Bearer {SLACK_TOKEN}"),
            format!("Bearer {ROTATED_TOKEN}")
        ]
    );
}

/// A slow endpoint times out each attempt, retries after 30 seconds and then
/// two minutes, and fails once the three-attempt budget is spent, without
/// rerunning the Verifier; a crashed worker restarts, and shutdown releases an
/// in-flight attempt with its attempt refunded.
///
/// # Panics
/// Panics when the timeout, schedule, exhaustion, restart, or release differs.
#[tokio::test]
async fn slow_endpoint_exhausts_the_budget_and_shutdown_releases() {
    let delivery = Delivery::start().await;
    let origin = delivery.mock.uri();
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(204).set_delay(Duration::from_secs(5)))
        .mount(&delivery.mock)
        .await;
    let slow = delivery.operator("slow", hook(&origin, "/hook/slow")).await;
    delivery.fail_binding("owner", &[&slow]).await;
    let limits = RuntimeLimits {
        operator_attempt_timeout: Duration::from_millis(500),
        ..Delivery::limits()
    };
    let crash = CapabilityCrash::default();
    let health = Arc::clone(&delivery.server.state().verification);
    let running = delivery.spawn(delivery.server.state(), limits, &failing_script(1), &crash);
    wait_until("runtime health", || {
        health.is_composed() && !health.is_degraded()
    })
    .await;
    crash.crash_next(RuntimeCapability::OperatorWorker);
    wait_until("worker down", || {
        !health.is_up(RuntimeCapability::OperatorWorker)
    })
    .await;
    wait_until("worker restarted", || !health.is_degraded()).await;
    let run = delivery.new_run(&[]).await;

    let mut delays = Vec::new();
    for attempt in 1..=2 {
        let rows = delivery
            .wait_dispatches(run, 1, |row| {
                row.status == "retrying" && row.attempts == attempt
            })
            .await;
        let row = &rows[&slow.as_uuid()];
        assert_eq!(row.error_code.as_deref(), Some("attempt_timed_out"));
        delays.push(row.delay.expect("retry is scheduled").round());
        delivery.make_retries_due(run).await;
    }
    assert_eq!(delays, vec![30.0, 120.0]);
    let rows = delivery
        .wait_dispatches(run, 1, |row| row.status == "failed")
        .await;
    running.stop().await;
    assert_eq!(rows[&slow.as_uuid()].attempts, 3);
    assert_eq!(delivery.seed.runs().await.expect("runs read"), vec![run]);
    assert_eq!(
        delivery.seed.run(run).await.expect("run reads").status,
        "completed"
    );

    let parked = delivery
        .operator("parked", hook(&origin, "/hook/parked"))
        .await;
    delivery.fail_binding("parker", &[&parked]).await;
    let draining = RuntimeLimits {
        operator_attempt_timeout: Duration::from_secs(20),
        drain_grace: Duration::from_millis(200),
        ..Delivery::limits()
    };
    let running = delivery.spawn(
        delivery.server.state(),
        draining,
        &failing_script(1),
        &CapabilityCrash::default(),
    );
    let first = run;
    let run = delivery.new_run(&[first]).await;
    delivery
        .wait_dispatches(run, 1, |row| row.status == "running")
        .await;
    running.stop().await;
    let rows = delivery.dispatches(run).await;
    let row = &rows[&parked.as_uuid()];
    assert_eq!(
        (row.status.as_str(), row.attempts),
        ("pending", 0),
        "shutdown releases the in-flight attempt with its attempt refunded"
    );
}

/// Delivery re-checks authority and decrypts per attempt: a connection
/// disabled after registration fails its dispatch closed without a provider
/// call, and a key provider outage retries instead of failing.
///
/// # Panics
/// Panics when either dispatch settles differently or a provider is called.
#[tokio::test]
async fn revoked_connection_fails_closed_and_key_outage_retries() {
    let delivery = Delivery::start().await;
    let origin = delivery.mock.uri();
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&delivery.mock)
        .await;
    let revoked = delivery.operator("revoked", slack("C0123456789")).await;
    let hooked = delivery
        .operator("hooked", hook(&origin, "/hook/outage"))
        .await;
    let (_, list) = delivery
        .call(Method::GET, "/v1/operator-connections", None)
        .await;
    let slack_id = list
        .as_array()
        .or_else(|| list["connections"].as_array())
        .expect("connection list")
        .iter()
        .find(|view| view["provider"] == "slack")
        .and_then(|view| view["connection_id"].as_str())
        .expect("slack connection")
        .to_owned();
    let (status, view) = delivery
        .call(
            Method::DELETE,
            &format!("/v1/operator-connections/{slack_id}"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{view}");
    delivery.fail_binding("owner", &[&revoked, &hooked]).await;

    let empty = tempfile::tempdir().expect("empty key directory");
    let mut outage = delivery.server.state().clone();
    outage.operator_keys = Arc::new(
        OperatorKeys::new(OperatorKeysConfig {
            source: OperatorKeySource::File,
            dir: Some(empty.path().to_path_buf()),
            ..OperatorKeysConfig::default()
        })
        .expect("file keys build"),
    );
    let running = delivery.spawn(
        &outage,
        Delivery::limits(),
        &failing_script(1),
        &CapabilityCrash::default(),
    );
    let run = delivery.new_run(&[]).await;
    let rows = delivery
        .wait_dispatches(run, 2, |row| {
            row.status != "pending" && row.status != "running"
        })
        .await;
    running.stop().await;
    let settled = |uid: &CardUid| {
        let row = &rows[&uid.as_uuid()];
        (row.status.as_str(), row.error_code.as_deref())
    };
    assert_eq!(
        settled(&revoked),
        ("failed", Some("connection_unavailable"))
    );
    assert_eq!(
        settled(&hooked),
        ("retrying", Some("credential_store_unavailable"))
    );
    assert!(
        delivery
            .mock
            .received_requests()
            .await
            .expect("recorded")
            .is_empty(),
        "no provider is called without an authorized, decrypted credential"
    );
}

/// Gated live release smoke: a failed verdict posts to a dedicated Slack test
/// channel and triggers a PagerDuty test service through the same Operator
/// runner and the public provider endpoints.
///
/// Reads `WYRD_LIVE_SLACK_BOT_TOKEN`, `WYRD_LIVE_SLACK_WORKSPACE_ID`,
/// `WYRD_LIVE_SLACK_CHANNEL_ID`, and `WYRD_LIVE_PAGERDUTY_KEY`; never runs in
/// a credential-free lane.
///
/// # Panics
/// Panics when a credential is unset or either dispatch is not delivered.
#[tokio::test]
#[ignore = "live release smoke: needs Slack and PagerDuty test credentials"]
async fn live_smoke_delivers_to_slack_and_pagerduty() {
    let env = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("{name} is set"));
    let delivery = Delivery::boot().await;
    delivery
        .connect(&[
            json!({ "provider": "slack", "name": "ops-slack",
                    "workspace_id": env("WYRD_LIVE_SLACK_WORKSPACE_ID"),
                    "bot_token": env("WYRD_LIVE_SLACK_BOT_TOKEN") }),
            json!({ "provider": "pager_duty", "name": "ops-pagerduty",
                    "integration_key": env("WYRD_LIVE_PAGERDUTY_KEY") }),
        ])
        .await;
    let posted = delivery
        .operator("live-slack", slack(&env("WYRD_LIVE_SLACK_CHANNEL_ID")))
        .await;
    let paged = delivery
        .operator(
            "live-pager",
            json!({ "kind": "notify", "channel": { "kind": "pager_duty",
                    "connection": "ops-pagerduty", "route": "wyrd-live-smoke",
                    "severity": "info", "summary": "Wyrd live smoke {{run_id}}" } }),
        )
        .await;
    delivery.fail_binding("owner", &[&posted, &paged]).await;
    let running = delivery.spawn_to(
        delivery.server.state(),
        Delivery::limits(),
        &failing_script(1),
        &CapabilityCrash::default(),
        ProviderEndpoints::default(),
    );
    let run = delivery.new_run(&[]).await;
    let rows = delivery
        .wait_dispatches(run, 2, |row| {
            row.status != "pending" && row.status != "running"
        })
        .await;
    running.stop().await;
    for row in rows.values() {
        assert_eq!(row.status, "delivered", "{row:?}");
    }
}

/// One busy tenant is capped at four concurrent Operator deliveries while
/// another tenant's dispatch still delivers; every dispatch eventually
/// delivers and the process never exceeds its global ceiling.
///
/// # Panics
/// Panics when a ceiling is exceeded, the other tenant is starved, or a
/// dispatch does not deliver.
#[tokio::test]
async fn operator_permits_cap_each_tenant_without_starving_another() {
    let delivery = Delivery::boot().await;
    let origin = delivery.mock.uri();
    Mock::given(method("POST"))
        .and(path("/slow"))
        .respond_with(ResponseTemplate::new(204).set_delay(Duration::from_secs(2)))
        .mount(&delivery.mock)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&delivery.mock)
        .await;
    let unauthenticated = |route: &str| json!({ "kind": "http", "method": "post", "url": format!("{origin}{route}") });
    let mut busy = Vec::new();
    for index in 0..6 {
        busy.push(
            delivery
                .seed
                .operator(&format!("slow-{index}"), &unauthenticated("/slow"))
                .await
                .expect("operator seeds"),
        );
    }
    delivery
        .fail_binding("busy", &busy.iter().collect::<Vec<_>>())
        .await;

    let other_tenant = DataTenantId::new_v7();
    delivery
        .server
        .pg_fixture()
        .seed_additional_tenant_with_uuid(other_tenant, "operator-other")
        .await
        .expect("second tenant seeds");
    let other =
        VerificationFixture::provision(delivery.server.state().postgres.wyrd(), other_tenant)
            .await
            .expect("second tenant provisions");
    let other_verifier = other.drift_verifier("drift").await.expect("verifier seeds");
    let quick = other
        .operator("quick", &unauthenticated("/quick"))
        .await
        .expect("operator seeds");
    let (owner, principal) = other.service("quiet").await.expect("owner seeds");
    let binding = other
        .bind_schedule(
            &owner,
            &owner,
            &other_verifier,
            "0 2 * * *",
            vec![FrozenTarget::Uid(quick.clone())],
        )
        .await
        .expect("binding projects");
    other.activate(principal).await.expect("owner activates");
    other
        .make_binding_due(binding)
        .await
        .expect("binding is due");

    let script = failing_script(2);
    let running = delivery.spawn(
        delivery.server.state(),
        Delivery::limits(),
        &script,
        &CapabilityCrash::default(),
    );
    let tenant = delivery.seed.tenant();
    let count = |tenant: DataTenantId, status: &'static str| {
        let pool = delivery.assertion.clone();
        async move {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM wyrd.operator_dispatches \
                  WHERE data_tenant_id = $1 AND status = $2",
            )
            .bind(tenant.as_uuid())
            .bind(status)
            .fetch_one(&pool)
            .await
            .expect("dispatch counts read")
        }
    };
    let deadline = tokio::time::Instant::now() + WAIT;
    let (mut peak, mut other_delivered_while_busy) = (0, false);
    loop {
        let busy_running = count(tenant, "running").await;
        let all_running = busy_running + count(other_tenant, "running").await;
        assert!(
            busy_running <= 4,
            "the busy tenant exceeded its ceiling: {busy_running}"
        );
        assert!(
            all_running <= 16,
            "the process exceeded its ceiling: {all_running}"
        );
        peak = peak.max(busy_running);
        if count(other_tenant, "delivered").await == 1 && count(tenant, "delivered").await < 6 {
            other_delivered_while_busy = true;
        }
        if count(tenant, "delivered").await == 6 && count(other_tenant, "delivered").await == 1 {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "dispatches never delivered"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    running.stop().await;
    assert_eq!(peak, 4, "the busy tenant reached its ceiling");
    assert!(
        other_delivered_while_busy,
        "the other tenant delivered while the busy tenant was saturated"
    );
}

/// Key versions of every Operator connection of `tenant`.
///
/// # Panics
/// Panics when the connection table cannot be read.
async fn key_versions(pool: &PgPool, tenant: DataTenantId) -> Vec<i32> {
    sqlx::query_scalar(
        "SELECT key_version FROM wyrd.operator_connections \
          WHERE data_tenant_id = $1 ORDER BY connection_id",
    )
    .bind(tenant.as_uuid())
    .fetch_all(pool)
    .await
    .expect("key versions read")
}

/// Rewrap runs beside delivery, never ahead of it: while a stalled key
/// provider times out every rewrap of one tenant's stale rows, another
/// tenant's due dispatch is still claimed and delivered; each timed-out pass
/// rolls back and is retried, and shutdown cancels the stalled pass at once.
/// Once the provider answers, one pass rewraps every row reading each key
/// version exactly once.
///
/// # Panics
/// Panics when delivery waits on rewrap, a stalled pass commits or blocks
/// shutdown, or a key version is read more than once in a pass.
#[tokio::test]
async fn slow_rewrap_never_holds_back_another_tenants_delivery() {
    use base64::Engine as _;
    let delivery = Delivery::start().await;
    let origin = delivery.mock.uri();
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&delivery.mock)
        .await;
    let stale_tenant = delivery.seed.tenant();
    assert_eq!(
        key_versions(&delivery.assertion, stale_tenant).await,
        vec![1, 1, 1]
    );

    let other_tenant = DataTenantId::new_v7();
    delivery
        .server
        .pg_fixture()
        .seed_additional_tenant_with_uuid(other_tenant, "operator-rewrap-other")
        .await
        .expect("second tenant seeds");
    let other =
        VerificationFixture::provision(delivery.server.state().postgres.wyrd(), other_tenant)
            .await
            .expect("second tenant provisions");
    let other_verifier = other.drift_verifier("drift").await.expect("verifier seeds");
    let quick = other
        .operator(
            "quick",
            &json!({ "kind": "http", "method": "post", "url": format!("{origin}/quick") }),
        )
        .await
        .expect("operator seeds");
    let (owner, principal) = other.service("quiet").await.expect("owner seeds");
    let binding = other
        .bind_schedule(
            &owner,
            &owner,
            &other_verifier,
            "0 2 * * *",
            vec![FrozenTarget::Uid(quick.clone())],
        )
        .await
        .expect("binding projects");
    other.activate(principal).await.expect("owner activates");
    other
        .make_binding_due(binding)
        .await
        .expect("binding is due");

    let vault = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(60)))
        .mount(&vault)
        .await;
    let mut replica = delivery.server.state().clone();
    replica.operator_keys = Arc::new(
        OperatorKeys::new(OperatorKeysConfig {
            source: OperatorKeySource::Vault,
            active_version: std::num::NonZeroU32::new(2).expect("nonzero"),
            vault: Some(VaultKeysConfig {
                addr: vault.uri(),
                mount: "secret".to_owned(),
                prefix: "wyrd/operator-keys".to_owned(),
                token_file: None,
                token: Some(secrecy::SecretString::from("rewrap-vault-token")),
            }),
            ..OperatorKeysConfig::default()
        })
        .expect("vault keys build"),
    );
    let limits = RuntimeLimits {
        rewrap_interval: Duration::from_millis(50),
        rewrap_tenant_budget: Duration::from_millis(300),
        ..Delivery::limits()
    };
    let running = delivery.spawn(
        &replica,
        limits,
        &failing_script(1),
        &CapabilityCrash::default(),
    );
    let deadline = tokio::time::Instant::now() + WAIT;
    let delivered = loop {
        let status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM wyrd.operator_dispatches WHERE data_tenant_id = $1",
        )
        .bind(other_tenant.as_uuid())
        .fetch_optional(&delivery.assertion)
        .await
        .expect("dispatch reads");
        if status.as_deref() == Some("delivered") {
            break status;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the other tenant's dispatch was held back: {status:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert_eq!(delivered.as_deref(), Some("delivered"));
    let deadline = tokio::time::Instant::now() + WAIT;
    while vault.received_requests().await.expect("recorded").len() < 2 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "a timed-out rewrap pass is retried"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let stopping = tokio::time::Instant::now();
    running.stop().await;
    assert!(
        stopping.elapsed() < Duration::from_secs(5),
        "shutdown cancels a stalled rewrap pass: {:?}",
        stopping.elapsed()
    );
    assert_eq!(
        key_versions(&delivery.assertion, stale_tenant).await,
        vec![1, 1, 1],
        "timed-out and cancelled passes roll back"
    );

    let engine = base64::engine::general_purpose::STANDARD;
    let v1 = std::fs::read_to_string(
        delivery
            .server
            .operator_keys_dir_for_test()
            .expect("generated key directory")
            .join("v1"),
    )
    .expect("key v1 reads");
    vault.reset().await;
    let key_path =
        |version: u32| format!("/v1/secret/data/wyrd/operator-keys/{stale_tenant}/{version}");
    for (version, key) in [(1, v1.trim().to_owned()), (2, engine.encode([8_u8; 32]))] {
        Mock::given(method("GET"))
            .and(path(key_path(version)))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "data": { "data": { "key": key } } })),
            )
            .mount(&vault)
            .await;
    }
    let running = delivery.spawn(
        &replica,
        limits,
        &EngineScript::default(),
        &CapabilityCrash::default(),
    );
    let deadline = tokio::time::Instant::now() + WAIT;
    while key_versions(&delivery.assertion, stale_tenant).await != vec![2, 2, 2] {
        assert!(
            tokio::time::Instant::now() < deadline,
            "rewrap never completed"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    running.stop().await;
    let reads: Vec<String> = vault
        .received_requests()
        .await
        .expect("recorded")
        .iter()
        .map(|request| request.url.path().to_owned())
        .collect();
    assert_eq!(
        reads,
        vec![key_path(2), key_path(1)],
        "one pass reads the active and the repeated old version once each"
    );
}

/// A cross-tenant rewrap discovery stalled behind a lock on the connection
/// table returns within the pass budget without touching a row, and the next
/// pass after the stall clears rewraps every stale row.
///
/// # Panics
/// Panics when the stalled pass outlives its budget, mutates a row, or the
/// later pass does not rewrap.
#[tokio::test]
async fn stalled_rewrap_discovery_returns_within_the_pass_budget() {
    use std::os::unix::fs::PermissionsExt as _;

    use base64::Engine as _;
    let delivery = Delivery::start().await;
    let tenant = delivery.seed.tenant();
    let dir = delivery
        .server
        .operator_keys_dir_for_test()
        .expect("generated key directory");
    let v2 = dir.join("v2");
    std::fs::write(
        &v2,
        base64::engine::general_purpose::STANDARD.encode([8_u8; 32]),
    )
    .expect("key v2 writes");
    std::fs::set_permissions(&v2, std::fs::Permissions::from_mode(0o600)).expect("chmod");
    let keys = OperatorKeys::new(OperatorKeysConfig {
        source: OperatorKeySource::File,
        active_version: std::num::NonZeroU32::new(2).expect("nonzero"),
        dir: Some(dir.to_path_buf()),
        vault: None,
    })
    .expect("file keys build");
    let state = delivery.server.state();
    let operator = state.postgres.operator_pool().expect("operator pool");

    let mut lock = delivery.assertion.begin().await.expect("lock transaction");
    sqlx::query("LOCK TABLE wyrd.operator_connections IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *lock)
        .await
        .expect("connection table locks");
    let budget = Duration::from_millis(300);
    let started = tokio::time::Instant::now();
    let moved = tokio::time::timeout(
        WAIT,
        keys.rewrap_pass(state.postgres.wyrd(), &operator, budget, budget),
    )
    .await
    .expect("the stalled pass returns")
    .expect("a stalled discovery is not a pass failure");
    let elapsed = started.elapsed();
    assert_eq!(moved, 0);
    assert!(
        elapsed < budget + Duration::from_secs(2),
        "the stalled discovery outlived the pass budget: {elapsed:?}"
    );
    lock.rollback().await.expect("lock releases");
    assert_eq!(
        key_versions(&delivery.assertion, tenant).await,
        vec![1, 1, 1],
        "a timed-out discovery mutates nothing"
    );

    let moved = keys
        .rewrap_pass(state.postgres.wyrd(), &operator, WAIT, WAIT)
        .await
        .expect("the next pass runs");
    assert_eq!(moved, 3);
    assert_eq!(
        key_versions(&delivery.assertion, tenant).await,
        vec![2, 2, 2]
    );
}
