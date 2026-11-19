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
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, Request as MockRequest, ResponseTemplate};
use wyrd_kms::{KmsConfig, WyrdKms};
use wyrd_server::components::operators::secrets::OperatorSecrets;
use wyrd_server::state::AppState;
use wyrd_server::verification::engines::EngineOutcome;
use wyrd_server::verification::health::RuntimeCapability;
use wyrd_server::verification::operators::ProviderEndpoints;
use wyrd_server::verification::runner::EngineScript;
use wyrd_server::verification::{CapabilityCrash, RuntimeLimits, VerificationRuntime};
use wyrd_spec::DataTenantId;
use wyrd_spec::card::drift::DriftMethod;
use wyrd_spec::card::verifier::Check;
use wyrd_spec::ids::{CardUid, ExecutionId, FeatureName};
use wyrd_spec::verification::TaskId;
use wyrd_spec::verification::execution::{
    ExecutedOutcome, ExecutedRequirement, RequirementDetail, RequirementOutcome, RequirementResult,
};
use wyrd_spec::verification::{
    DriftReport, DriftVerdict, FeatureDriftReport, FrozenTarget, VerificationVerdict,
};
use wyrd_testing::kms::{TransitFixture, transit_config};
use wyrd_testing::logs::LogCapture;
use wyrd_testing::verification::VerificationFixture;
use wyrd_testing::{Bootstrap, WyrdTestServer};

/// Upper bound on every wait for the runtime to make progress.
///
/// A due binding's occurrence is claimed only 30 seconds after its window
/// ends, so a scheduled-run wait must outlast that claim delay.
const WAIT: Duration = Duration::from_mins(1);
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
        Self::start_on(None).await
    }

    /// [`Self::start`] on a server booted with `kms` as its `[kms]`, or the
    /// test server's generated file key when `None`.
    ///
    /// # Panics
    /// Panics when startup, seeding, or a connection create fails.
    async fn start_on(kms: Option<KmsConfig>) -> Self {
        let delivery = Self::boot_on(kms).await;
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
        Self::boot_on(None).await
    }

    /// [`Self::boot`] on a server booted with `kms` as its `[kms]`, or the
    /// test server's generated file key when `None`.
    ///
    /// # Panics
    /// Panics when startup or seeding fails.
    async fn boot_on(kms: Option<KmsConfig>) -> Self {
        let builder = WyrdTestServer::builder();
        let builder = match kms {
            Some(kms) => builder.with_kms_for_test(kms),
            None => builder,
        };
        let server = builder.start_bound().await.expect("bound server starts");
        let tenant = server.pg_fixture().data_tenant_id();
        let seed = VerificationFixture::provision(server.state(), tenant)
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
        self.call_as(&self.jwt, method, uri, body).await
    }

    /// Send `method uri` with an optional JSON body as `jwt`.
    ///
    /// # Panics
    /// Panics when the request fails or the response is not JSON.
    async fn call_as(
        &self,
        jwt: &str,
        method: Method,
        uri: &str,
        body: Option<&Value>,
    ) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .header("Idempotency-Key", Uuid::now_v7().to_string())
            .body(body.map_or_else(Body::empty, |body| Body::from(body.to_string())))
            .expect("request builds");
        let response = self
            .server
            .oneshot_authenticated(jwt, request)
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
            "metadata": { "name": name, "version": "1.0.0" },
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
            .activate(principal, &owner)
            .await
            .expect("owner activates");
        self.seed
            .make_binding_due(binding)
            .await
            .expect("binding is due");
        owner
    }

    /// Poll until the scheduler created an execution not in `known` and
    /// return it.
    ///
    /// # Panics
    /// Panics when no new execution appears within [`WAIT`].
    async fn new_run(&self, known: &[ExecutionId]) -> ExecutionId {
        let deadline = tokio::time::Instant::now() + WAIT;
        loop {
            let runs = self.seed.executions().await.expect("executions read");
            if let Some(run) = runs.into_iter().find(|run| !known.contains(run)) {
                return run;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "no execution scheduled"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Seed a second tenant named `name` whose Drift Verifier binding is due
    /// and dispatches one unauthenticated HTTP Operator to the mock `/quick`.
    ///
    /// # Panics
    /// Panics when the tenant or its binding cannot be seeded.
    async fn quiet_tenant(&self, name: &str) -> QuietTenant {
        let tenant = DataTenantId::new_v7();
        self.server
            .pg_fixture()
            .seed_additional_tenant_with_uuid(tenant, name)
            .await
            .expect("second tenant seeds");
        self.server
            .ensure_builtin_tables_for_test(tenant)
            .await
            .expect("second tenant receives every built-in table");
        let seed = VerificationFixture::provision(self.server.state(), tenant)
            .await
            .expect("second tenant provisions");
        let verifier = seed.drift_verifier("drift").await.expect("verifier seeds");
        let quick = seed
            .operator(
                "quick",
                &json!({ "kind": "http", "method": "post",
                         "url": format!("{}/quick", self.mock.uri()) }),
            )
            .await
            .expect("operator seeds");
        let (owner, principal) = seed.service("quiet").await.expect("owner seeds");
        let binding = seed
            .bind_schedule(
                &owner,
                &owner,
                &verifier,
                "0 2 * * *",
                vec![FrozenTarget::Uid(quick)],
            )
            .await
            .expect("binding projects");
        seed.activate(principal, &owner)
            .await
            .expect("owner activates");
        seed.make_binding_due(binding)
            .await
            .expect("binding is due");
        QuietTenant {
            tenant,
            seed,
            verifier,
            owner,
        }
    }

    /// Runtime bounds fast enough for tests.
    fn limits() -> RuntimeLimits {
        RuntimeLimits {
            lease: Duration::from_mins(1),
            execution_timeout: Duration::from_secs(20),
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
        limits: &RuntimeLimits,
        script: &EngineScript,
        crash: &CapabilityCrash,
    ) -> Running {
        let base = Url::parse(&self.mock.uri()).expect("mock URI");
        self.spawn_to(
            state,
            limits,
            script,
            crash,
            None,
            ProviderEndpoints {
                slack: base.join("/slack").expect("slack URL"),
                pager_duty: base.join("/pagerduty").expect("pagerduty URL"),
            },
        )
    }

    /// Spawn a runtime over this server's state as a replica holding
    /// `secrets` would run it, delivering to the mock provider.
    ///
    /// # Panics
    /// Panics when the runtime cannot compose.
    fn spawn_replica(
        &self,
        secrets: OperatorSecrets,
        limits: &RuntimeLimits,
        script: &EngineScript,
        crash: &CapabilityCrash,
    ) -> Running {
        let base = Url::parse(&self.mock.uri()).expect("mock URI");
        self.spawn_to(
            self.server.state(),
            limits,
            script,
            crash,
            Some(secrets),
            ProviderEndpoints {
                slack: base.join("/slack").expect("slack URL"),
                pager_duty: base.join("/pagerduty").expect("pagerduty URL"),
            },
        )
    }

    /// Spawn a runtime over `state` delivering fixed providers to `endpoints`,
    /// with `secrets` standing in for the process's Operator secrets when
    /// given.
    ///
    /// # Panics
    /// Panics when the runtime cannot compose.
    fn spawn_to(
        &self,
        state: &AppState,
        limits: &RuntimeLimits,
        script: &EngineScript,
        crash: &CapabilityCrash,
        secrets: Option<OperatorSecrets>,
        endpoints: ProviderEndpoints,
    ) -> Running {
        let builder = VerificationRuntime::builder(state)
            .limits(*limits)
            .engine_script(script.clone())
            .crash_switch(crash.clone())
            .provider_endpoints(endpoints);
        let builder = match secrets {
            Some(secrets) => builder.operator_secrets(secrets),
            None => builder,
        };
        let runtime = builder.build().expect("the runtime composes");
        let stop = CancellationToken::new();
        let task = tokio::spawn(runtime.run(stop.clone()));
        Running { stop, task }
    }

    /// Every dispatch of the execution `run`, keyed by its Operator Card UID.
    ///
    /// # Panics
    /// Panics when the dispatch table cannot be read.
    async fn dispatches(&self, run: ExecutionId) -> BTreeMap<Uuid, DispatchRow> {
        let rows: Vec<DispatchRow> = sqlx::query_as(
            "SELECT operator_uid, dispatch_id, status, attempts, \
                    last_error->>'code' AS error_code, \
                    EXTRACT(EPOCH FROM next_attempt_at - updated_at)::float8 AS delay \
               FROM wyrd.operator_dispatches WHERE execution_id = $1",
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
        run: ExecutionId,
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
    async fn make_retries_due(&self, run: ExecutionId) {
        sqlx::query(
            "UPDATE wyrd.operator_dispatches SET next_attempt_at = statement_timestamp() \
              WHERE execution_id = $1 AND status = 'retrying'",
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

/// A second tenant seeded by [`Delivery::quiet_tenant`].
struct QuietTenant {
    /// The tenant's identity.
    tenant: DataTenantId,
    /// Seeded verification state of the tenant.
    seed: VerificationFixture,
    /// The tenant's Drift Verifier.
    verifier: CardUid,
    /// Owner Service of the tenant's due binding, also a direct-run subject.
    owner: CardUid,
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

/// A script whose next `runs` executions each return a failed outcome whose
/// one drift requirement scored `latency` as drifted.
///
/// # Panics
/// Panics when a static feature name, task id, or check is invalid.
fn failing_script(runs: usize) -> EngineScript {
    let feature = FeatureName::new("latency").expect("feature name");
    let report = DriftReport {
        method: DriftMethod::Custom,
        features: [(
            feature.clone(),
            FeatureDriftReport {
                feature,
                score: Some(3.0),
                threshold: Some(1.0),
                verdict: DriftVerdict::Drift,
                evidence: None,
            },
        )]
        .into_iter()
        .collect(),
        verdict: DriftVerdict::Drift,
    };
    let outcome = ExecutedOutcome {
        verdict: VerificationVerdict::Failed,
        requirements: vec![ExecutedRequirement {
            result: RequirementResult {
                id: TaskId::new("drift").expect("task id"),
                step: None,
                item_index: None,
                iteration_index: None,
                statement: "The window does not drift.".to_owned(),
                outcome: RequirementOutcome::Failed,
                reason: "latency drifted".to_owned(),
                actual: None,
                citations: Vec::new(),
                data: None,
                detail: Some(RequirementDetail::Drift(report)),
            },
            check: serde_json::from_value::<Check>(json!({
                "kind": "drift", "on": "inputs.samples", "method": "Custom",
                "signal": { "kind": "Metric", "name": "latency_ms" },
                "condition": { "kind": "Above", "limit": 250.0 }
            }))
            .expect("drift check decodes"),
            provenance: None,
            tool_calls: Vec::new(),
        }],
        steps: Vec::new(),
        evidence: BTreeMap::new(),
    };
    let script = EngineScript::default();
    for _ in 0..runs {
        script.push(EngineOutcome::Completed(outcome.clone()));
    }
    script
}

/// A passed outcome with no requirements, which dispatches no Operator.
fn passed_outcome() -> ExecutedOutcome {
    ExecutedOutcome {
        verdict: VerificationVerdict::Passed,
        requirements: Vec::new(),
        steps: Vec::new(),
        evidence: BTreeMap::new(),
    }
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
            "body": { "verdict": "{{verdict}}", "execution": "{{execution_id}}" } })
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
/// the completed Verifier run. No credential appears in any log line emitted
/// while the connections are created and every attempt settles.
///
/// # Panics
/// Panics when a dispatch settles differently, a provider request differs,
/// or a credential is logged.
#[tokio::test]
async fn failed_verdict_fans_out_to_every_provider_independently() {
    let logs = LogCapture::install();
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
        &Delivery::limits(),
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
    assert_eq!(hook_body["execution"], run.as_uuid().to_string());

    let settled_run = delivery.seed.execution(run).await.expect("execution reads");
    assert_eq!(settled_run.status, "completed");
    assert_eq!(
        delivery.seed.executions().await.expect("executions read"),
        vec![run]
    );

    let logged = logs.text();
    assert!(
        logged.contains("operator dispatch attempt settled"),
        "the capture observed the delivery worker"
    );
    for secret in [SLACK_TOKEN, PAGER_KEY, HOOK_KEY] {
        assert!(!logged.contains(secret), "a credential was logged");
    }
}

/// A transient failure retries after the 30-second backoff; the credential is
/// rotated in Postgres and a second replica, on a newer key version, delivers
/// the next attempt with the rotated token and rewraps the stored credential,
/// with no Card revision.
///
/// # Panics
/// Panics when the retry schedule, rotation, rewrap, or frozen kind counts
/// differ between attempts.
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
    let posted = delivery
        .operator(
            "posted",
            json!({ "kind": "notify", "channel": { "kind": "slack", "connection": "ops-slack",
                    "channel_id": "C0123456789",
                    "text": "{{drift.drifted_features}}/{{drift.total_features}} drifted" } }),
        )
        .await;
    delivery.fail_binding("owner", &[&posted]).await;
    let first = delivery.spawn(
        delivery.server.state(),
        &Delivery::limits(),
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

    let replica = rotated_file_kms(&delivery.server);
    delivery.make_retries_due(run).await;
    let second = delivery.spawn_replica(
        OperatorSecrets::new(Some(Arc::clone(&replica))),
        &Delivery::limits(),
        &EngineScript::default(),
        &CapabilityCrash::default(),
    );
    let rows = delivery
        .wait_dispatches(run, 1, |row| row.status == "delivered")
        .await;
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let envelope: Vec<u8> = sqlx::query_scalar(
            "SELECT secret_ciphertext FROM wyrd.operator_connections \
              WHERE connection_id = $1::uuid",
        )
        .bind(&slack_id)
        .fetch_one(&delivery.assertion)
        .await
        .expect("envelope reads");
        if replica.is_current(&envelope).expect("a common envelope") {
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
    let posts = delivery.requests("/slack").await;
    let texts: Vec<Value> = posts
        .iter()
        .map(|post| post.body_json::<Value>().expect("slack body")["text"].clone())
        .collect();
    assert_eq!(texts, vec![json!("1/1 drifted"), json!("1/1 drifted")]);
    let tokens: Vec<String> = posts
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

/// Two rotations of one credential race through a slow key service: the
/// rotation that commits first wins, the other returns the connection
/// conflict without overwriting it, and the next delivery sends the winner's
/// token.
///
/// # Panics
/// Panics when both or neither rotation commits, the loser's response is not
/// the conflict, or the delivery sends another token.
#[tokio::test]
async fn racing_rotations_keep_the_first_commit_and_deliver_it() {
    let transit = TransitFixture::start_delayed(Duration::from_millis(500)).await;
    let delivery = Delivery::start_on(Some(transit.config("ops", &[]))).await;
    Mock::given(method("POST"))
        .and(path("/slack"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "ok": true })))
        .mount(&delivery.mock)
        .await;
    let posted = delivery.operator("posted", slack("C0123456789")).await;
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
    let uri = format!("/v1/operator-connections/{slack_id}");
    let sealed_before = transit.requests("encrypt").await;

    let first = json!({ "provider": "slack", "bot_token": ROTATED_TOKEN });
    let second = json!({ "provider": "slack", "bot_token": "xoxb-losing-slack-token" });
    let first_rotation = delivery.call(Method::PATCH, &uri, Some(&first));
    let second_rotation = async {
        while transit.requests("encrypt").await <= sealed_before {
            tokio::task::yield_now().await;
        }
        delivery.call(Method::PATCH, &uri, Some(&second)).await
    };
    let ((first_status, _), (second_status, problem)) =
        tokio::join!(first_rotation, second_rotation);
    assert_eq!(first_status, StatusCode::OK, "the first commit wins");
    assert_eq!(
        (second_status, problem["code"].as_str()),
        (
            StatusCode::CONFLICT,
            Some("WYRD_OPERATOR_409_CONNECTION_CONFLICT")
        ),
        "the losing rotation conflicts"
    );

    delivery.fail_binding("owner", &[&posted]).await;
    let running = delivery.spawn(
        delivery.server.state(),
        &Delivery::limits(),
        &failing_script(1),
        &CapabilityCrash::default(),
    );
    let run = delivery.new_run(&[]).await;
    delivery
        .wait_dispatches(run, 1, |row| row.status == "delivered")
        .await;
    running.stop().await;
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
    assert_eq!(tokens, vec![format!("Bearer {ROTATED_TOKEN}")]);
}

/// The backend-wrapped DEK and payload nonce of a stored envelope, split by
/// the documented common format.
///
/// # Panics
/// Panics when `envelope` is shorter than its own length fields.
fn wrapped_dek_and_nonce(envelope: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let ref_len = usize::from(u16::from_be_bytes([envelope[6], envelope[7]]));
    let at = 8 + ref_len;
    let wrapped_len = u32::from_be_bytes(envelope[at..at + 4].try_into().expect("length field"));
    let wrapped_end = at + 4 + usize::try_from(wrapped_len).expect("length fits");
    (
        envelope[at + 4..wrapped_end].to_vec(),
        envelope[wrapped_end..wrapped_end + 12].to_vec(),
    )
}

/// Renewing a connection's encryption reseals the same credential under a
/// fresh DEK and nonce on the same identity and secret version, and the next
/// delivery sends the unchanged token with no client or Card change.
///
/// # Panics
/// Panics when the renewal response, stored envelope, or delivered token
/// differs.
#[tokio::test]
async fn renewed_encryption_keeps_the_credential_and_delivers_it() {
    let transit = TransitFixture::start().await;
    let delivery = Delivery::start_on(Some(transit.config("ops", &[]))).await;
    Mock::given(method("POST"))
        .and(path("/slack"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "ok": true })))
        .mount(&delivery.mock)
        .await;
    let posted = delivery.operator("posted", slack("C0123456789")).await;
    let (_, list) = delivery
        .call(Method::GET, "/v1/operator-connections", None)
        .await;
    let before = list
        .as_array()
        .expect("connection list")
        .iter()
        .find(|view| view["provider"] == "slack")
        .expect("slack connection")
        .clone();
    let slack_id = before["connection_id"].as_str().expect("id").to_owned();
    let stored = |id: String| {
        let pool = delivery.assertion.clone();
        async move {
            sqlx::query_as::<_, (Vec<u8>, i32)>(
                "SELECT secret_ciphertext, secret_version FROM wyrd.operator_connections \
                 WHERE connection_id = $1::uuid",
            )
            .bind(id)
            .fetch_one(&pool)
            .await
            .expect("row reads")
        }
    };
    let (old_envelope, old_version) = stored(slack_id.clone()).await;

    let (status, renewed) = delivery
        .call(
            Method::POST,
            &format!("/v1/operator-connections/{slack_id}/renew-encryption"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{renewed}");
    assert_eq!(
        (
            &renewed["connection_id"],
            &renewed["status"],
            &renewed["name"]
        ),
        (&before["connection_id"], &before["status"], &before["name"]),
        "renewal keeps the connection identity and status"
    );
    assert!(
        !renewed.to_string().contains(SLACK_TOKEN),
        "the response is redacted"
    );
    let (new_envelope, new_version) = stored(slack_id.clone()).await;
    assert_eq!(new_version, old_version, "the secret version is unchanged");
    let (old_dek, old_nonce) = wrapped_dek_and_nonce(&old_envelope);
    let (new_dek, new_nonce) = wrapped_dek_and_nonce(&new_envelope);
    assert_ne!(new_dek, old_dek, "renewal wraps a fresh DEK");
    assert_ne!(new_nonce, old_nonce, "renewal encrypts under a fresh nonce");

    delivery.fail_binding("owner", &[&posted]).await;
    let running = delivery.spawn(
        delivery.server.state(),
        &Delivery::limits(),
        &failing_script(1),
        &CapabilityCrash::default(),
    );
    let run = delivery.new_run(&[]).await;
    delivery
        .wait_dispatches(run, 1, |row| row.status == "delivered")
        .await;
    running.stop().await;
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
    assert_eq!(tokens, vec![format!("Bearer {SLACK_TOKEN}")]);
}

/// A slow endpoint times out each attempt, retries after 30 seconds and then
/// two minutes, and fails once the three-attempt budget is spent, without
/// rerunning the Verifier; a crashed worker restarts, shutdown releases an
/// in-flight attempt with its attempt refunded, and a restarted worker
/// delivers that released dispatch under the same dispatch identity and
/// Idempotency-Key.
///
/// # Panics
/// Panics when the timeout, schedule, exhaustion, restart, release, or
/// recovered delivery differs.
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
    let health = Arc::clone(&delivery.server.state().verification.health);
    let running = delivery.spawn(delivery.server.state(), &limits, &failing_script(1), &crash);
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
    assert_eq!(
        delivery.seed.executions().await.expect("executions read"),
        vec![run]
    );
    assert_eq!(
        delivery
            .seed
            .execution(run)
            .await
            .expect("execution reads")
            .status,
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
        &draining,
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

    let parked_id = row.dispatch_id;
    let resumed = delivery.spawn(
        delivery.server.state(),
        &draining,
        &EngineScript::default(),
        &CapabilityCrash::default(),
    );
    let rows = delivery
        .wait_dispatches(run, 1, |row| row.status == "delivered")
        .await;
    resumed.stop().await;
    let row = &rows[&parked.as_uuid()];
    assert_eq!(
        (row.dispatch_id, row.attempts),
        (parked_id, 1),
        "the restarted worker delivers the released dispatch under its identity"
    );
    let parked_posts = delivery.requests("/hook/parked").await;
    assert!(!parked_posts.is_empty());
    for post in &parked_posts {
        assert_eq!(
            post.headers
                .get("idempotency-key")
                .and_then(|v| v.to_str().ok()),
            Some(parked_id.to_string().as_str()),
            "every attempt carries the same Idempotency-Key"
        );
    }
    assert_eq!(
        delivery.seed.executions().await.expect("executions read"),
        vec![first, run],
        "redelivery never reruns the Verifier"
    );
}

/// Delivery re-checks authority and decrypts per attempt: a connection
/// disabled after registration fails its dispatch closed without a provider
/// call, and an unreachable wrapping backend retries instead of failing.
///
/// # Panics
/// Panics when either dispatch settles differently or a provider is called.
#[tokio::test]
async fn revoked_connection_fails_closed_and_key_outage_retries() {
    let transit = TransitFixture::start().await;
    let delivery = Delivery::start_on(Some(transit_config(&transit.uri(), "ops", &[]))).await;
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

    let outage = OperatorSecrets::new(Some(Arc::new(
        WyrdKms::new(&transit_config(&unreachable_endpoint(), "ops", &[]), false)
            .expect("transit facade builds"),
    )));
    let running = delivery.spawn_replica(
        outage,
        &Delivery::limits(),
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

/// Under the production Operator defaults a transient failure retries after
/// 30 seconds, a later retry is clipped to the dispatch's five-minute
/// deadline, and once PostgreSQL's clock passes that deadline the dispatch
/// fails `deadline_exceeded` without another provider call. The test moves
/// `created_at` back instead of waiting, because the deadline is measured
/// from it in the database.
///
/// # Panics
/// Panics when the schedule, clipping, or deadline failure differs.
#[tokio::test]
async fn production_schedule_retries_within_the_delivery_deadline() {
    let delivery = Delivery::start().await;
    let origin = delivery.mock.uri();
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&delivery.mock)
        .await;
    let flaky = delivery
        .operator("flaky", hook(&origin, "/hook/flaky"))
        .await;
    delivery.fail_binding("owner", &[&flaky]).await;
    let limits = Delivery::limits();
    assert_eq!(
        (limits.operator_deadline, limits.operator_attempts),
        (Duration::from_mins(5), 3),
        "production Operator defaults"
    );
    let running = delivery.spawn(
        delivery.server.state(),
        &limits,
        &failing_script(1),
        &CapabilityCrash::default(),
    );
    let run = delivery.new_run(&[]).await;
    let backdate = |seconds: i32| {
        let pool = delivery.assertion.clone();
        async move {
            sqlx::query(
                "UPDATE wyrd.operator_dispatches \
                    SET created_at = created_at - make_interval(secs => $2), \
                        next_attempt_at = statement_timestamp() \
                  WHERE execution_id = $1",
            )
            .bind(run.as_uuid())
            .bind(f64::from(seconds))
            .execute(&pool)
            .await
            .expect("dispatch backdates");
        }
    };

    let rows = delivery
        .wait_dispatches(run, 1, |row| row.status == "retrying" && row.attempts == 1)
        .await;
    let row = &rows[&flaky.as_uuid()];
    assert_eq!(row.error_code.as_deref(), Some("provider_transient"));
    assert_eq!(row.delay.expect("retry is scheduled").round(), 30.0);

    backdate(290).await;
    let rows = delivery
        .wait_dispatches(run, 1, |row| row.status == "retrying" && row.attempts == 2)
        .await;
    let clipped = rows[&flaky.as_uuid()].delay.expect("retry is scheduled");
    assert!(
        (5.0..=10.5).contains(&clipped),
        "the two-minute backoff is clipped to the deadline: {clipped}"
    );

    backdate(15).await;
    let rows = delivery
        .wait_dispatches(run, 1, |row| row.status == "failed")
        .await;
    running.stop().await;
    let row = &rows[&flaky.as_uuid()];
    assert_eq!(
        (row.attempts, row.error_code.as_deref()),
        (2, Some("deadline_exceeded"))
    );
    assert_eq!(delivery.requests("/hook/flaky").await.len(), 2);
}

/// An attempt that times out after PagerDuty accepted it is ambiguous: the
/// retry resends the same `dedup_key`, so PagerDuty collapses the duplicate,
/// and the completed Verifier run and its result are untouched. No Alert row
/// or table exists anywhere in the database or the Bifrost catalog.
///
/// # Panics
/// Panics when the retry changes the dedup key, the run or result changes, or
/// an Alert table exists.
#[tokio::test]
async fn ambiguous_pagerduty_retry_reuses_the_dedup_key() {
    let delivery = Delivery::start().await;
    Mock::given(method("POST"))
        .and(path("/pagerduty"))
        .respond_with(
            ResponseTemplate::new(202)
                .set_body_json(json!({ "status": "success" }))
                .set_delay(Duration::from_secs(2)),
        )
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&delivery.mock)
        .await;
    Mock::given(method("POST"))
        .and(path("/pagerduty"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({ "status": "success" })))
        .mount(&delivery.mock)
        .await;
    let paged = delivery
        .operator(
            "paged",
            json!({ "kind": "notify", "channel": { "kind": "pager_duty",
                    "connection": "ops-pagerduty", "route": "retention",
                    "severity": "warning", "summary": "{{verifier_ref}} failed" } }),
        )
        .await;
    delivery.fail_binding("owner", &[&paged]).await;
    let limits = RuntimeLimits {
        operator_attempt_timeout: Duration::from_millis(500),
        ..Delivery::limits()
    };
    let running = delivery.spawn(
        delivery.server.state(),
        &limits,
        &failing_script(1),
        &CapabilityCrash::default(),
    );
    let run = delivery.new_run(&[]).await;
    let rows = delivery
        .wait_dispatches(run, 1, |row| row.status == "retrying")
        .await;
    assert_eq!(
        rows[&paged.as_uuid()].error_code.as_deref(),
        Some("attempt_timed_out")
    );
    let before = delivery.seed.execution(run).await.expect("execution reads");
    assert_eq!(before.status, "completed");
    delivery.make_retries_due(run).await;
    let rows = delivery
        .wait_dispatches(run, 1, |row| row.status == "delivered")
        .await;
    running.stop().await;
    let dispatch_id = rows[&paged.as_uuid()].dispatch_id.to_string();
    assert_eq!(rows[&paged.as_uuid()].attempts, 2);

    let keys: Vec<Value> = delivery
        .requests("/pagerduty")
        .await
        .iter()
        .map(|page| page.body_json::<Value>().expect("pagerduty JSON")["dedup_key"].clone())
        .collect();
    assert_eq!(
        keys,
        vec![json!(dispatch_id), json!(dispatch_id)],
        "both attempts carry the stable dispatch ID"
    );
    let after = delivery.seed.execution(run).await.expect("execution reads");
    assert_eq!(
        (after.status, after.result_id),
        (before.status, before.result_id),
        "delivery never changes the Verifier result"
    );
    assert_eq!(
        delivery.seed.executions().await.expect("executions read"),
        vec![run]
    );

    let alert_tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.tables WHERE table_name ILIKE '%alert%'",
    )
    .fetch_one(&delivery.assertion)
    .await
    .expect("catalog reads");
    let alert_datasets: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM vala.bifrost_tables WHERE fqn ILIKE '%alert%'")
            .fetch_one(&delivery.assertion)
            .await
            .expect("bifrost catalog reads");
    assert_eq!(
        (alert_tables, alert_datasets),
        (0, 0),
        "no Alert table exists"
    );
}

/// Connection authority is resolved per attempt inside the dispatch's tenant:
/// an Operator naming a connection that exists only in another tenant, and
/// one naming no connection at all, each fail `connection_unavailable` at
/// delivery time without a provider call.
///
/// # Panics
/// Panics when either dispatch settles differently or a provider is called.
#[tokio::test]
async fn foreign_or_missing_connection_fails_at_delivery() {
    let delivery = Delivery::boot().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "ok": true })))
        .mount(&delivery.mock)
        .await;
    let other = delivery
        .server
        .seed_tenant("operator-foreign")
        .await
        .expect("tenant seeds");
    let foreign_admin = delivery
        .server
        .bootstrap_service_in_tenant(other, "operator-foreign-admin", &["admin"])
        .await
        .expect("foreign admin bootstraps");
    let foreign_jwt = delivery
        .server
        .exchange_api_key(foreign_admin.api_key().expect("api key"))
        .await
        .expect("api key exchanges");
    let (status, view) = delivery
        .call_as(
            &foreign_jwt,
            Method::POST,
            "/v1/operator-connections",
            Some(&json!({ "provider": "slack", "name": "ops-elsewhere",
                          "workspace_id": "T0002", "bot_token": SLACK_TOKEN })),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{view}");
    let channel = |connection: &str| {
        json!({ "kind": "notify", "channel": { "kind": "slack", "connection": connection,
                "channel_id": "C0123456789", "text": "{{verifier_ref}} failed" } })
    };
    let foreign = delivery
        .seed
        .operator("foreign", &channel("ops-elsewhere"))
        .await
        .expect("operator seeds");
    let missing = delivery
        .seed
        .operator("missing", &channel("ops-missing"))
        .await
        .expect("operator seeds");
    delivery.fail_binding("owner", &[&foreign, &missing]).await;
    let running = delivery.spawn(
        delivery.server.state(),
        &Delivery::limits(),
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
    for uid in [&foreign, &missing] {
        let row = &rows[&uid.as_uuid()];
        assert_eq!(
            (row.status.as_str(), row.error_code.as_deref()),
            ("failed", Some("connection_unavailable")),
            "{row:?}"
        );
    }
    assert!(
        delivery
            .mock
            .received_requests()
            .await
            .expect("recorded")
            .is_empty(),
        "no provider is called without a connection in the dispatch's tenant"
    );
}

/// A stored envelope opens only for the connection and tenant it was sealed
/// for. With the same tenant's other connection's envelope, another tenant's
/// envelope for the same connection name, or an altered envelope in place,
/// each dispatch fails `credential_invalid` without a provider call, so no
/// copied secret reaches a provider, while the untouched connection beside
/// them still delivers with its own key.
///
/// # Panics
/// Panics when an envelope cannot be replaced, a dispatch settles otherwise,
/// or a provider receives anything but the intact connection's key.
#[tokio::test]
async fn copied_or_altered_envelopes_fail_closed_at_delivery() {
    let delivery = Delivery::boot().await;
    let origin = delivery.mock.uri();
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&delivery.mock)
        .await;
    let connection = |name: &str, key: &str| {
        json!({ "provider": "http", "name": name, "origin": origin,
                "auth": { "scheme": "header", "name": "X-Api-Key", "value": key } })
    };
    delivery
        .connect(&[
            connection("hooks-cross-tenant", "cross-tenant-key"),
            connection("hooks-cross-owner", "cross-owner-key"),
            connection("hooks-altered", "altered-key"),
            connection("hooks-intact", "intact-key"),
        ])
        .await;
    let other = delivery
        .server
        .seed_tenant("operator-copied")
        .await
        .expect("tenant seeds");
    let foreign_admin = delivery
        .server
        .bootstrap_service_in_tenant(other, "operator-copied-admin", &["admin"])
        .await
        .expect("foreign admin bootstraps");
    let foreign_jwt = delivery
        .server
        .exchange_api_key(foreign_admin.api_key().expect("api key"))
        .await
        .expect("api key exchanges");
    let (status, view) = delivery
        .call_as(
            &foreign_jwt,
            Method::POST,
            "/v1/operator-connections",
            Some(&connection("hooks-cross-tenant", "foreign-tenant-key")),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{view}");

    let copy = |target: &str, source_tenant: DataTenantId, source: &str| {
        sqlx::query(
            "UPDATE wyrd.operator_connections SET secret_ciphertext = \
               (SELECT secret_ciphertext FROM wyrd.operator_connections \
                 WHERE data_tenant_id = $1 AND name = $2) \
             WHERE data_tenant_id = $3 AND name = $4",
        )
        .bind(source_tenant.as_uuid())
        .bind(source.to_owned())
        .bind(delivery.seed.tenant().as_uuid())
        .bind(target.to_owned())
        .execute(&delivery.assertion)
    };
    copy("hooks-cross-tenant", other, "hooks-cross-tenant")
        .await
        .expect("cross-tenant envelope copies");
    copy("hooks-cross-owner", delivery.seed.tenant(), "hooks-intact")
        .await
        .expect("cross-owner envelope copies");
    sqlx::query(
        "UPDATE wyrd.operator_connections \
            SET secret_ciphertext = overlay(secret_ciphertext PLACING \
                  decode(lpad(to_hex(get_byte(secret_ciphertext, length(secret_ciphertext) - 1) # 1), 2, '0'), 'hex') \
                  FROM length(secret_ciphertext)) \
          WHERE data_tenant_id = $1 AND name = 'hooks-altered'",
    )
    .bind(delivery.seed.tenant().as_uuid())
    .execute(&delivery.assertion)
    .await
    .expect("envelope alters in place");

    let hook_on = |connection: &str| {
        json!({ "kind": "http", "method": "post", "url": format!("{origin}/hook/{connection}"),
                "auth": { "scheme": "header", "name": "x-api-key", "connection": connection },
                "body": { "verdict": "{{verdict}}" } })
    };
    let mut operators = Vec::new();
    for name in [
        "hooks-cross-tenant",
        "hooks-cross-owner",
        "hooks-altered",
        "hooks-intact",
    ] {
        operators.push(
            delivery
                .seed
                .operator(name, &hook_on(name))
                .await
                .expect("operator seeds"),
        );
    }
    delivery
        .fail_binding("owner", &operators.iter().collect::<Vec<_>>())
        .await;
    let running = delivery.spawn(
        delivery.server.state(),
        &Delivery::limits(),
        &failing_script(1),
        &CapabilityCrash::default(),
    );
    let run = delivery.new_run(&[]).await;
    let rows = delivery
        .wait_dispatches(run, 4, |row| {
            row.status != "pending" && row.status != "running"
        })
        .await;
    running.stop().await;
    let settled: Vec<(&str, Option<&str>)> = operators
        .iter()
        .map(|uid| {
            let row = &rows[&uid.as_uuid()];
            (row.status.as_str(), row.error_code.as_deref())
        })
        .collect();
    assert_eq!(
        settled,
        [
            ("failed", Some("credential_invalid")),
            ("failed", Some("credential_invalid")),
            ("failed", Some("credential_invalid")),
            ("delivered", None),
        ]
    );
    let keys: Vec<(String, String)> = delivery
        .mock
        .received_requests()
        .await
        .expect("recorded")
        .iter()
        .map(|request| {
            let key = request
                .headers
                .get("x-api-key")
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default();
            (request.url.path().to_owned(), key.to_owned())
        })
        .collect();
    assert_eq!(
        keys,
        [("/hook/hooks-intact".to_owned(), "intact-key".to_owned())],
        "only the intact connection reaches its provider, with its own key"
    );
}

/// A Slack template referencing `drift.*` fields renders the frozen counts of
/// the failed Drift run, agreeing with its server-written `summary`.
///
/// # Panics
/// Panics when the dispatch is not delivered or the posted text differs.
#[tokio::test]
async fn drift_counts_render_into_the_slack_message() {
    let delivery = Delivery::start().await;
    Mock::given(method("POST"))
        .and(path("/slack"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "ok": true })))
        .mount(&delivery.mock)
        .await;
    let posted = delivery
        .operator(
            "counted",
            json!({ "kind": "notify", "channel": { "kind": "slack", "connection": "ops-slack",
                    "channel_id": "C0123456789",
                    "text": "{{drift.drifted_features}}/{{drift.total_features}} drifted | {{summary}}" } }),
        )
        .await;
    delivery.fail_binding("owner", &[&posted]).await;
    let running = delivery.spawn(
        delivery.server.state(),
        &Delivery::limits(),
        &failing_script(1),
        &CapabilityCrash::default(),
    );
    let run = delivery.new_run(&[]).await;
    let rows = delivery
        .wait_dispatches(run, 1, |row| {
            row.status != "pending" && row.status != "running"
        })
        .await;
    running.stop().await;
    assert!(
        rows.values().all(|row| row.status == "delivered"),
        "{rows:?}"
    );
    let sent: Vec<Value> = delivery
        .requests("/slack")
        .await
        .iter()
        .map(|request| serde_json::from_slice(&request.body).expect("slack body is JSON"))
        .collect();
    assert_eq!(
        sent,
        [json!({ "channel": "C0123456789",
                 "text": "1/1 drifted | Verdict failed: 1 of 1 requirements failed." })]
    );
}

/// Gated live release smoke: a failed verdict posts to a dedicated Slack test
/// channel through the production Operator runner and the public Slack
/// endpoint.
///
/// Reads `WYRD_LIVE_SLACK_BOT_TOKEN`, `WYRD_LIVE_SLACK_WORKSPACE_ID`, and
/// `WYRD_LIVE_SLACK_CHANNEL_ID`; never runs in a credential-free lane.
///
/// # Panics
/// Panics when a credential is unset or the dispatch is not delivered.
#[tokio::test]
#[ignore = "live release smoke: needs Slack test credentials"]
async fn live_smoke_delivers_to_slack() {
    live_smoke(
        json!({ "provider": "slack", "name": "ops-slack",
                "workspace_id": live_env("WYRD_LIVE_SLACK_WORKSPACE_ID"),
                "bot_token": live_env("WYRD_LIVE_SLACK_BOT_TOKEN") }),
        slack(&live_env("WYRD_LIVE_SLACK_CHANNEL_ID")),
    )
    .await;
}

/// Delivers one failed-verdict incident to a real PagerDuty Events API v2
/// integration through the production runner and default endpoint.
///
/// Reads `WYRD_LIVE_PAGERDUTY_KEY`; never runs in a credential-free lane.
///
/// # Panics
/// Panics when the credential is unset or the dispatch is not delivered.
#[tokio::test]
#[ignore = "live release smoke: needs PagerDuty test credentials"]
async fn live_smoke_delivers_to_pagerduty() {
    live_smoke(
        json!({ "provider": "pager_duty", "name": "ops-pagerduty",
                "integration_key": live_env("WYRD_LIVE_PAGERDUTY_KEY") }),
        json!({ "kind": "notify", "channel": { "kind": "pager_duty",
                "connection": "ops-pagerduty", "route": "wyrd-live-smoke",
                "severity": "info", "summary": "Wyrd live smoke {{execution_id}}" } }),
    )
    .await;
}

/// Reads one required live-smoke credential from the environment.
///
/// # Panics
/// Panics naming the variable when it is unset.
fn live_env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is set"))
}

/// Registers `connection` and one Operator `spec` on a failing binding, runs
/// the production worker against the real provider endpoints, and waits for
/// the single dispatch to settle.
///
/// # Panics
/// Panics when the dispatch settles in any status other than `delivered`.
async fn live_smoke(connection: Value, spec: Value) {
    let delivery = Delivery::boot().await;
    delivery.connect(&[connection]).await;
    let operator = delivery.operator("live", spec).await;
    delivery.fail_binding("owner", &[&operator]).await;
    let running = delivery.spawn_to(
        delivery.server.state(),
        &Delivery::limits(),
        &failing_script(1),
        &CapabilityCrash::default(),
        None,
        ProviderEndpoints::default(),
    );
    let run = delivery.new_run(&[]).await;
    let rows = delivery
        .wait_dispatches(run, 1, |row| {
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

    let quiet = delivery.quiet_tenant("operator-other").await;
    let other_tenant = quiet.tenant;

    let script = failing_script(2);
    let running = delivery.spawn(
        delivery.server.state(),
        &Delivery::limits(),
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

/// Running Verifier executions and running Operator dispatches of `tenant`.
///
/// # Panics
/// Panics when either table cannot be read.
async fn running(pool: &PgPool, tenant: DataTenantId) -> (i64, i64) {
    sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM wyrd.verifier_executions \
                  WHERE data_tenant_id = $1 AND status = 'running'), \
                (SELECT COUNT(*) FROM wyrd.operator_dispatches \
                  WHERE data_tenant_id = $1 AND status = 'running')",
    )
    .bind(tenant.as_uuid())
    .fetch_one(pool)
    .await
    .expect("running counts read")
}

/// Held Verifier runs exceed the former tenant ceiling while slow Operator
/// deliveries retain their separate limits and another tenant progresses.
/// All held work completes after release.
///
/// # Panics
/// Panics when an Operator ceiling is exceeded, Verifier runs are capped,
/// the quiet tenant is starved, or held work does not complete.
#[tokio::test]
async fn verifiers_progress_while_operator_deliveries_are_capped() {
    let delivery = Delivery::boot().await;
    let origin = delivery.mock.uri();
    Mock::given(method("POST"))
        .and(path("/slow"))
        .respond_with(ResponseTemplate::new(204).set_delay(Duration::from_secs(8)))
        .mount(&delivery.mock)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&delivery.mock)
        .await;
    let mut slow = Vec::new();
    for index in 0..6 {
        slow.push(
            delivery
                .seed
                .operator(
                    &format!("slow-{index}"),
                    &json!({ "kind": "http", "method": "post", "url": format!("{origin}/slow") }),
                )
                .await
                .expect("operator seeds"),
        );
    }
    let busy_owner = delivery
        .fail_binding("busy", &slow.iter().collect::<Vec<_>>())
        .await;
    let quiet = delivery.quiet_tenant("pools-quiet").await;
    let busy = delivery.seed.tenant();
    let limits = RuntimeLimits {
        drain_grace: Duration::from_millis(200),
        ..Delivery::limits()
    };
    assert_eq!(
        (
            limits.operator_global_permits,
            limits.operator_tenant_permits
        ),
        (16, 4),
        "production permit defaults"
    );
    let script = failing_script(2);
    let runtime = delivery.spawn(
        delivery.server.state(),
        &limits,
        &script,
        &CapabilityCrash::default(),
    );
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let quiet_dispatches: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM wyrd.operator_dispatches WHERE data_tenant_id = $1",
        )
        .bind(quiet.tenant.as_uuid())
        .fetch_one(&delivery.assertion)
        .await
        .expect("dispatches read");
        if running(&delivery.assertion, busy).await.1 == 4 && quiet_dispatches == 1 {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "binding runs never fanned out"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    script.hold();
    for _ in 0..8 {
        script.push(EngineOutcome::Completed(passed_outcome()));
    }
    let mut held = Vec::new();
    for _ in 0..6 {
        held.push((
            &delivery.seed,
            delivery
                .seed
                .submit(&delivery.verifier, &busy_owner, BTreeMap::new())
                .await
                .expect("busy run enqueues"),
        ));
    }
    for _ in 0..2 {
        held.push((
            &quiet.seed,
            quiet
                .seed
                .submit(&quiet.verifier, &quiet.owner, BTreeMap::new())
                .await
                .expect("quiet run enqueues"),
        ));
    }
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let (busy_runs, busy_dispatches) = running(&delivery.assertion, busy).await;
        let (quiet_runs, quiet_dispatches) = running(&delivery.assertion, quiet.tenant).await;
        assert!(
            busy_dispatches <= 4,
            "the busy tenant exceeded its Operator ceiling: {busy_dispatches} dispatches"
        );
        assert!(
            busy_dispatches + quiet_dispatches <= 16,
            "the process-wide Operator ceiling was exceeded"
        );
        // The quiet delivery settles on its own task, so it is awaited in the
        // same poll that sees the busy tenant saturated: both holding at once
        // is the claim, and a starved quiet tenant reaches the deadline.
        let quiet_delivered: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM wyrd.operator_dispatches \
              WHERE data_tenant_id = $1 AND status = 'delivered'",
        )
        .bind(quiet.tenant.as_uuid())
        .fetch_one(&delivery.assertion)
        .await
        .expect("quiet dispatch reads");
        // A claimed run recovers its execution state before its engine
        // starts, so every run having started is polled, not assumed.
        let entered = script.entered();
        if (
            busy_runs,
            busy_dispatches,
            quiet_runs,
            quiet_delivered,
            entered,
        ) == (6, 4, 2, 1, 10)
        {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "pools never saturated with the quiet tenant delivered and every \
             Verifier run started: busy {busy_runs}/{busy_dispatches}, quiet \
             {quiet_runs}, delivered {quiet_delivered}, started {entered} of 10"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    script.release();
    for (seed, run) in held {
        let deadline = tokio::time::Instant::now() + WAIT;
        while seed.execution(run).await.expect("execution reads").status != "completed" {
            assert!(
                tokio::time::Instant::now() < deadline,
                "held run never completed"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
    runtime.stop().await;
}

/// Whether each Operator connection envelope of `tenant` names `kms`'s
/// active key, in connection order.
///
/// # Panics
/// Panics when the connection table cannot be read or a row holds no
/// common envelope.
async fn current(pool: &PgPool, tenant: DataTenantId, kms: &WyrdKms) -> Vec<bool> {
    let envelopes: Vec<Vec<u8>> = sqlx::query_scalar(
        "SELECT secret_ciphertext FROM wyrd.operator_connections \
          WHERE data_tenant_id = $1 ORDER BY connection_id",
    )
    .bind(tenant.as_uuid())
    .fetch_all(pool)
    .await
    .expect("envelopes read");
    envelopes
        .iter()
        .map(|envelope| kms.is_current(envelope).expect("a common envelope"))
        .collect()
}

/// Publish a new owner-only file key `next` beside `server`'s generated
/// `[kms]` key and return a facade rotated onto it with that key retained,
/// as a rolling replica configures it.
///
/// # Panics
/// Panics when the server used no generated key or the key cannot be written.
fn rotated_file_kms(server: &WyrdTestServer) -> Arc<WyrdKms> {
    use std::os::unix::fs::PermissionsExt as _;

    use base64::Engine as _;
    let dir = server.kms_dir_for_test().expect("generated key directory");
    let next = dir.join("next");
    if !next.exists() {
        std::fs::write(
            &next,
            base64::engine::general_purpose::STANDARD.encode([8_u8; 32]),
        )
        .expect("key next writes");
        std::fs::set_permissions(&next, std::fs::Permissions::from_mode(0o600)).expect("chmod");
    }
    let mut config = server.kms_config_for_test().clone();
    config.retained_keys = vec![config.active_key.take().expect("active key")];
    config.active_key = Some("next".to_owned());
    Arc::new(WyrdKms::new(&config, false).expect("rotated file facade builds"))
}

/// A loopback URL nothing listens on.
///
/// # Panics
/// Panics when no loopback port can be reserved.
fn unreachable_endpoint() -> String {
    let closed = std::net::TcpListener::bind("127.0.0.1:0").expect("port reserves");
    let endpoint = format!("http://{}", closed.local_addr().expect("addr"));
    drop(closed);
    endpoint
}

/// Rewrap runs beside delivery, never ahead of it: while a stalled wrapping
/// provider times out every rewrap of one tenant's stale rows, another
/// tenant's due dispatch is still claimed and delivered; each timed-out pass
/// rolls back and is retried, and shutdown cancels the stalled pass at once.
/// Once the provider answers, a pass moves every row onto the active key.
///
/// # Panics
/// Panics when delivery waits on rewrap, a stalled pass commits or blocks
/// shutdown, or the answered pass leaves a row on the retained key.
#[tokio::test]
async fn slow_rewrap_never_holds_back_another_tenants_delivery() {
    let transit = TransitFixture::start().await;
    let delivery = Delivery::start_on(Some(transit_config(&transit.uri(), "old", &[]))).await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&delivery.mock)
        .await;
    let stale_tenant = delivery.seed.tenant();
    let old = WyrdKms::new(&transit_config(&transit.uri(), "old", &[]), false).expect("old facade");
    assert_eq!(
        current(&delivery.assertion, stale_tenant, &old).await,
        vec![true; 3]
    );

    let other_tenant = delivery.quiet_tenant("operator-rewrap-other").await.tenant;

    let stalled = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_mins(1)))
        .mount(&stalled)
        .await;
    let replica = OperatorSecrets::new(Some(Arc::new(
        WyrdKms::new(&transit_config(&stalled.uri(), "new", &["old"]), false)
            .expect("stalled facade"),
    )));
    let limits = RuntimeLimits {
        rewrap_interval: Duration::from_millis(50),
        rewrap_tenant_budget: Duration::from_millis(300),
        ..Delivery::limits()
    };
    let running = delivery.spawn_replica(
        replica,
        &limits,
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
    while stalled.received_requests().await.expect("recorded").len() < 2 {
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
        current(&delivery.assertion, stale_tenant, &old).await,
        vec![true; 3],
        "timed-out and cancelled passes roll back"
    );

    let answered = Arc::new(
        WyrdKms::new(&transit_config(&transit.uri(), "new", &["old"]), false)
            .expect("answered facade"),
    );
    let running = delivery.spawn_replica(
        OperatorSecrets::new(Some(Arc::clone(&answered))),
        &limits,
        &EngineScript::default(),
        &CapabilityCrash::default(),
    );
    let deadline = tokio::time::Instant::now() + WAIT;
    while current(&delivery.assertion, stale_tenant, &answered).await != vec![true; 3] {
        assert!(
            tokio::time::Instant::now() < deadline,
            "rewrap never completed"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    running.stop().await;
}

/// Rewrap keeps the work it finishes: with every key-service answer held back
/// so the tenant's stale rows together outlast one pass's tenant budget, each
/// pass commits the rows it rewrapped before the budget ran out, and repeated
/// passes move every row onto the active key. A rotation and a disable racing
/// the first pass win their rows: the rotated token is the one that opens and
/// the disabled connection stays disabled.
///
/// # Panics
/// Panics when a pass commits nothing, the passes do not converge, or a
/// racing write is lost.
#[tokio::test]
async fn rewrap_commits_progress_across_tenant_budgets() {
    let transit = TransitFixture::start_delayed(Duration::from_millis(200)).await;
    let delivery = Delivery::start_on(Some(transit.config("old", &[]))).await;
    let tenant = delivery.seed.tenant();
    let state = delivery.server.state();
    let operator = state.postgres.operator_pool().expect("operator pool");
    let kms =
        Arc::new(WyrdKms::new(&transit.config("new", &["old"]), false).expect("rotated facade"));
    let keys = OperatorSecrets::new(Some(Arc::clone(&kms)));
    let budget = Duration::from_millis(600);
    let (_, list) = delivery
        .call(Method::GET, "/v1/operator-connections", None)
        .await;
    let id_of = |provider: &str| {
        list.as_array()
            .or_else(|| list["connections"].as_array())
            .expect("connection list")
            .iter()
            .find(|view| view["provider"] == provider)
            .and_then(|view| view["connection_id"].as_str())
            .expect("connection")
            .to_owned()
    };
    let (slack_id, pager_id) = (id_of("slack"), id_of("pager_duty"));

    let slack_uri = format!("/v1/operator-connections/{slack_id}");
    let pager_uri = format!("/v1/operator-connections/{pager_id}");
    let rotate = json!({ "provider": "slack", "bot_token": ROTATED_TOKEN });
    let (moved, (rotated, _), (disabled, _)) = tokio::join!(
        keys.rewrap_pass(state.postgres.wyrd(), &operator, WAIT, budget),
        delivery.call(Method::PATCH, &slack_uri, Some(&rotate)),
        delivery.call(Method::DELETE, &pager_uri, None),
    );
    assert_eq!((rotated, disabled), (StatusCode::OK, StatusCode::OK));
    let moved = moved.expect("the pass runs");
    assert!(
        (1..3).contains(&moved),
        "one pass commits the rows it finished within its budget: {moved}"
    );
    let mut passes = 1;
    while current(&delivery.assertion, tenant, &kms).await != vec![true; 3] {
        assert!(passes < 10, "rewrap never converged");
        keys.rewrap_pass(state.postgres.wyrd(), &operator, WAIT, budget)
            .await
            .expect("the pass runs");
        passes += 1;
    }

    let mut conn = state
        .postgres
        .wyrd()
        .tenant_conn(tenant)
        .await
        .expect("conn");
    let slack = wyrd_sql::queries::operator_connections::get_connection(
        &mut conn,
        slack_id.parse().expect("connection id"),
    )
    .await
    .expect("connection reads")
    .expect("connection exists");
    conn.commit().await.expect("read commits");
    let opened = keys
        .open(tenant, &slack)
        .await
        .expect("rotated token opens");
    let opened: Value = serde_json::from_slice(&opened).expect("stored secret is JSON");
    assert_eq!(opened["value"], ROTATED_TOKEN, "the rotation's token opens");
    let (_, pager) = delivery.call(Method::GET, &pager_uri, None).await;
    assert_eq!(pager["status"], "disabled", "the disable is kept");
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
    let delivery = Delivery::start().await;
    let tenant = delivery.seed.tenant();
    let kms = rotated_file_kms(&delivery.server);
    let keys = OperatorSecrets::new(Some(Arc::clone(&kms)));
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
        current(&delivery.assertion, tenant, &kms).await,
        vec![false; 3],
        "a timed-out discovery mutates nothing"
    );

    let moved = keys
        .rewrap_pass(state.postgres.wyrd(), &operator, WAIT, WAIT)
        .await
        .expect("the next pass runs");
    assert_eq!(moved, 3);
    assert_eq!(
        current(&delivery.assertion, tenant, &kms).await,
        vec![true; 3]
    );
}
