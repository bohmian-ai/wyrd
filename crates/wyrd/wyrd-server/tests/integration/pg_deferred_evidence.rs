//! Deferred evidence published for a connected repository's commit and
//! consumed by its pull-request executions, through the real publish and
//! webhook routes, the queued runtime, Bifrost and repository Postgres.
//!
//! The tenant connects `acme/policy` (4242) as in
//! [`pg_github_pull_requests`](super::pg_github_pull_requests) and binds
//! `ci-gate`, whose `ci` slot is deferred, to its pull requests.

use std::collections::BTreeSet;
use std::time::Duration;

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use wyrd_client::cards::Cards;
use wyrd_loader::{build_declaration_input, load};
use wyrd_server::state::LimitsConfig;
use wyrd_server::verification::engines::EngineOutcome;
use wyrd_server::verification::hosted::HostedLimits;
use wyrd_server::verification::runner::EngineScript;
use wyrd_server::verification::{RuntimeLimits, VerificationRuntime};
use wyrd_spec::ids::SlotName;
use wyrd_testing::Bootstrap;
use wyrd_testing::github::PullRequest;

use super::pg_github_pull_requests::{Deployment, INSTALLATION, REPOSITORY, expire_receipts};
use super::pg_hosted_artifacts::client;

/// The connected repository's slug.
const SLUG: &str = "acme/policy";
/// A repository the installation reaches that the tenant did not select.
const UNSELECTED_SLUG: &str = "acme/other";

/// A commit no pull request has opened yet.
const C0: &str = "c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0";
/// A commit first published to before anything subscribes.
const C1: &str = "c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1";
/// A head superseded while it waits.
const C2: &str = "c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2";
/// The head that supersedes [`C2`] and is never published to.
const C3: &str = "c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3";

/// The `ci-gate` Verifier at `version`: it passes when the deferred `ci`
/// evidence, an object with a `status`, reports `status: green`. Its
/// deferred CSV `report` slot is read by nothing, so no execution waits for
/// it; it only accepts media publications.
fn ci_gate(version: &str) -> String {
    format!(
        "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: ci-gate\n  version: {version}\nspec:\n  description: CI is green.\n  inputs:\n    ci: {{type: json, deferred: true, schema: {{type: object, required: [status]}}}}\n    report: {{type: media, formats: [csv], deferred: true}}\n  requirements:\n    - id: green\n      statement: CI is green\n      check: {{kind: assert, path: inputs.ci.status, operator: equals, expected: green}}\n"
    )
}

/// The Repository `policy` at `version` binding `ci-gate` to pull requests.
fn gated(version: &str) -> String {
    format!(
        "apiVersion: wyrd/v1\nkind: Repository\nmetadata:\n  name: policy\n  version: {version}\nspec:\n  repository_id: \"4242\"\n  verified_by:\n    - verifier: ./ci-gate.yaml\n      runs_on: {{kind: pull_request}}\n"
    )
}

impl Deployment {
    /// Declare `policy` at `version` binding `ci-gate` at `verifier`.
    ///
    /// # Panics
    /// Panics when the declaration is refused.
    async fn gate(&self, version: &str, verifier: &str) {
        self.declare_files(
            &[("ci-gate.yaml", ci_gate(verifier))],
            &gated(version),
            &format!("gate-{version}"),
        )
        .await;
    }

    /// Publish `body` as the editor and return the status and JSON answer.
    ///
    /// # Panics
    /// Panics when the request cannot be served.
    async fn publish(&self, body: &Value) -> (StatusCode, Value) {
        self.publish_as(&self.jwt, body).await
    }

    /// Publish `body` as the user holding `jwt` and return the status and
    /// JSON answer.
    ///
    /// # Panics
    /// Panics when the request cannot be served.
    pub(crate) async fn publish_as(&self, jwt: &str, body: &Value) -> (StatusCode, Value) {
        self.post(jwt, "/v1/verification/evidence/publish", body)
            .await
    }

    /// POST `body` to `uri` as the user holding `jwt` and return the status
    /// and JSON answer.
    ///
    /// # Panics
    /// Panics when the request cannot be served.
    pub(crate) async fn post(&self, jwt: &str, uri: &str, body: &Value) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(Method::POST)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .expect("request builds");
        let response = self
            .server
            .oneshot_authenticated(jwt, request)
            .await
            .expect("publication responds");
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 1 << 20)
            .await
            .expect("body reads");
        (
            status,
            serde_json::from_slice(&bytes).expect("answer is JSON"),
        )
    }

    /// Publish `ci` value `value` for `commit` of the connected repository.
    ///
    /// # Panics
    /// Panics when the request cannot be served.
    pub(crate) async fn publish_ci(&self, commit: &str, value: Value) -> (StatusCode, Value) {
        self.publish(&publication(
            SLUG,
            commit,
            "ci",
            json!({"kind": "json", "value": value}),
        ))
        .await
    }

    /// Open PR `number` at `head` and return its one execution.
    ///
    /// # Panics
    /// Panics when the delivery is refused or admits other than one
    /// execution.
    pub(crate) async fn open(&self, number: u64, head: &str) -> Uuid {
        self.mock.set_pull_request(
            REPOSITORY,
            number,
            PullRequest::open(REPOSITORY, "b1", head),
        );
        assert_eq!(self.send(number, "opened").await.0, StatusCode::ACCEPTED);
        let rows = self.rows(number).await;
        assert_eq!(rows.len(), 1, "one ci-gate execution: {rows:?}");
        rows[0].execution_id
    }

    /// Wait until `execution` is served with `status`, and a completed one
    /// with its result, and return it.
    ///
    /// # Panics
    /// Panics when it is not within two minutes, longer than a parked
    /// execution's one-minute recheck.
    pub(crate) async fn until(&self, execution: Uuid, status: &str) -> Value {
        let deadline = tokio::time::Instant::now() + Duration::from_mins(2);
        loop {
            let served = self.status(execution).await;
            // A completed execution's result lands in Bifrost after its status.
            if served["status"] == status && (status != "completed" || !served["result"].is_null())
            {
                return served;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "{execution} is not {status}: {served}"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// How many executions the tenant holds.
    ///
    /// # Panics
    /// Panics when the read fails.
    async fn executions(&self) -> i64 {
        let mut conn = self.conn().await;
        let (count,) = sqlx::query_as("SELECT count(*) FROM wyrd.verifier_executions")
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("executions count");
        conn.commit().await.expect("read commits");
        count
    }

    /// The attempts `execution` has spent and the Verifier version it froze.
    ///
    /// # Panics
    /// Panics when the read fails.
    pub(crate) async fn spent(&self, execution: Uuid) -> (i32, String) {
        let mut conn = self.conn().await;
        let spent = sqlx::query_as(
            "SELECT e.attempts, v.version
               FROM wyrd.verifier_executions e
               JOIN wyrd.cards v ON v.card_uid = e.verifier_card_uid
              WHERE e.execution_id = $1",
        )
        .bind(execution)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("execution reads");
        conn.commit().await.expect("read commits");
        spent
    }
}

/// A publish request body.
pub(crate) fn publication(repository: &str, commit: &str, input: &str, value: Value) -> Value {
    json!({
        "repository": repository,
        "tags": {"commit": commit},
        "input": input,
        "value": value,
    })
}

/// Runtime limits that settle work quickly and end it after `deadline`.
pub(crate) fn limits(deadline: Duration) -> RuntimeLimits {
    RuntimeLimits {
        lease: Duration::from_mins(1),
        execution_timeout: deadline,
        drain_grace: Duration::from_secs(10),
        poll_interval: Duration::from_millis(50),
        restart_backoff: Duration::from_millis(300),
        ..RuntimeLimits::default()
    }
}

/// Publication matches subscriptions, retains only what they accept, and
/// resumes the same waiting execution.
///
/// With nothing declaring `ci`, publication is refused as unmatched and
/// retains nothing. Once `ci-gate` is bound, malformed, foreign or
/// incompletely uploaded publications are refused; evidence published before
/// any pull request is accepted, replays identically, refuses different
/// content and another tenant, starts nothing, and a pull request opened
/// later consumes it without republishing. A pull request on the commit
/// refused earlier waits naming `ci`; publishing while a newer policy pins
/// another `ci-gate` version reaches both: it resumes the same execution
/// under its own pin, the park spending no attempt, and a pull request under
/// the newer pin consumes it too. A later subscriber receives no historical
/// evidence and waits.
///
/// # Panics
/// Panics when a receipt, refusal, wait or resumption differs.
#[tokio::test(flavor = "multi_thread")]
async fn publication_is_matched_retained_and_resumes_the_waiting_execution() {
    let deployment = Deployment::start_limited(limits(Duration::from_mins(5))).await;
    let (status, refused) = deployment.publish_ci(C1, json!({"status": "green"})).await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    assert_eq!(refused["code"], "WYRD_VERIFICATION_409_EVIDENCE_UNMATCHED");

    deployment.gate("1.1.0", "1.0.0").await;
    let json_value = json!({"kind": "json", "value": {"status": "green"}});
    for (body, expected) in [
        (
            publication(SLUG, "c0c0c0c", "ci", json_value.clone()),
            StatusCode::BAD_REQUEST,
        ),
        (
            publication(SLUG, C0, "other", json_value.clone()),
            StatusCode::CONFLICT,
        ),
        (
            publication(UNSELECTED_SLUG, C0, "ci", json_value.clone()),
            StatusCode::FORBIDDEN,
        ),
    ] {
        let (status, answer) = deployment.publish(&body).await;
        assert_eq!(status, expected, "{body}: {answer}");
    }
    let file = |format: &str| json!({"kind": "file", "sha256": "ab".repeat(32), "size_bytes": 3, "format": format});
    for (input, value, reason) in [
        (
            "ci",
            json!({"kind": "json", "value": {"result": "green"}}),
            "schema",
        ),
        ("ci", file("csv"), "json"),
        ("report", json_value.clone(), "media"),
        ("report", file("pdf"), "does not accept pdf"),
        ("report", file("csv"), "completed"),
    ] {
        let (status, refused) = deployment
            .publish(&publication(SLUG, C0, input, value))
            .await;
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "{input} {reason}: {refused}"
        );
        assert_eq!(refused["code"], "WYRD_VERIFICATION_400_INPUT_INVALID");
        assert!(
            refused.to_string().contains(reason),
            "{input} is refused naming {reason}: {refused}"
        );
    }

    let planned = json!({"sha256": "cd".repeat(32), "size_bytes": 3});
    let (status, plan) = deployment
        .post(
            &deployment.jwt,
            "/v1/verification/evidence",
            &json!({"files": [planned]}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{plan}");
    assert_eq!(plan["uploads"].as_array().map(Vec::len), Some(1), "{plan}");
    let (status, refused) = deployment
        .publish(&publication(
            SLUG,
            C0,
            "report",
            json!({"kind": "file", "sha256": "cd".repeat(32), "size_bytes": 3, "format": "csv"}),
        ))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
    assert!(
        refused.to_string().contains("completed"),
        "a planned upload that never completed is refused: {refused}"
    );

    let Bootstrap::User { jwt: viewer, .. } = deployment
        .server
        .bootstrap_user("evidence-viewer", &["viewer"])
        .await
        .expect("viewer bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };
    let (status, denied) = deployment
        .publish_as(&viewer, &publication(SLUG, C0, "ci", json_value.clone()))
        .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "publishing needs verifier:run: {denied}"
    );

    let (status, receipt) = deployment.publish_ci(C0, json!({"status": "green"})).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    assert_eq!(receipt["status"], "accepted");
    assert_eq!(receipt["matched_subscriptions"], 1);
    assert_eq!(receipt["tags"]["commit"], C0);
    assert_eq!(
        receipt["repository"],
        json!({"host": deployment.mock.host(), "repository_id": REPOSITORY.to_string()}),
        "the slug resolves to the connected repository's stable identity"
    );
    let (status, replay) = deployment.publish_ci(C0, json!({"status": "green"})).await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    assert_eq!(replay["status"], "replayed");
    assert_eq!(replay["evidence_id"], receipt["evidence_id"]);
    assert_eq!(replay["recorded_at"], receipt["recorded_at"]);
    let (status, conflict) = deployment.publish_ci(C0, json!({"status": "red"})).await;
    assert_eq!(status, StatusCode::CONFLICT, "{conflict}");
    assert_eq!(conflict["code"], "WYRD_VERIFICATION_409_EVIDENCE_CONFLICT");
    assert_eq!(
        deployment.executions().await,
        0,
        "publication alone starts no execution"
    );

    let other = deployment
        .server
        .seed_tenant("evidence-other")
        .await
        .expect("second tenant seeds");
    let Bootstrap::Machine { api_key, .. } = deployment
        .server
        .bootstrap_service_in_tenant(other, "evidence-other-admin", &["admin"])
        .await
        .expect("other tenant's admin bootstraps")
    else {
        panic!("service bootstrap returned a non-machine principal");
    };
    let foreign = deployment
        .server
        .exchange_api_key(&api_key)
        .await
        .expect("other tenant's key exchanges");
    let (status, refused) = deployment
        .publish_as(&foreign, &publication(SLUG, C0, "ci", json_value.clone()))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{refused}");
    assert_eq!(
        refused["code"], "WYRD_GITHUB_404_NOT_CONNECTED",
        "another tenant cannot publish to, or replay, this tenant's evidence"
    );

    let early = deployment.open(1, C0).await;
    let served = deployment.until(early, "completed").await;
    assert_eq!(served["result"]["outcome"]["verdict"], "passed", "{served}");

    let waiting = deployment.open(2, C1).await;
    let served = deployment.until(waiting, "waiting").await;
    assert_eq!(served["waiting"], json!(["ci"]));
    assert_eq!(
        deployment.spent(waiting).await,
        (0, "1.0.0".to_owned()),
        "parking refunds its attempt"
    );

    deployment.gate("1.2.0", "1.0.1").await;
    let (status, receipt) = deployment.publish_ci(C1, json!({"status": "green"})).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    assert_eq!(receipt["status"], "accepted");
    assert_eq!(
        receipt["matched_subscriptions"], 2,
        "the waiting pin and the selected pin each subscribe"
    );
    let served = deployment.until(waiting, "completed").await;
    assert_eq!(served["result"]["outcome"]["verdict"], "passed", "{served}");
    assert_eq!(
        deployment.spent(waiting).await,
        (1, "1.0.0".to_owned()),
        "the same execution resumed under its pin, spending one attempt in all"
    );

    let pinned = deployment.open(3, C1).await;
    let served = deployment.until(pinned, "completed").await;
    assert_eq!(served["result"]["outcome"]["verdict"], "passed", "{served}");
    assert_eq!(
        deployment.spent(pinned).await,
        (1, "1.0.1".to_owned()),
        "the selected pin consumes the evidence it accepted without republishing"
    );

    deployment.gate("1.3.0", "1.0.2").await;
    let later = deployment.open(4, C1).await;
    let served = deployment.until(later, "waiting").await;
    assert_eq!(
        served["waiting"],
        json!(["ci"]),
        "a later subscriber receives no historical evidence"
    );
}

/// A parked execution never reopens once superseded, and its deadline ends
/// it when evidence never arrives.
///
/// # Panics
/// Panics when superseded work resumes or missing evidence does not time
/// out.
#[tokio::test(flavor = "multi_thread")]
async fn parked_executions_end_by_supersession_and_deadline() {
    let deployment = Deployment::start_limited(limits(Duration::from_secs(8))).await;
    deployment.gate("1.1.0", "1.0.0").await;
    let superseded = deployment.open(4, C2).await;
    deployment.until(superseded, "waiting").await;
    deployment
        .mock
        .edit_pull_request(REPOSITORY, 4, |pr| C3.clone_into(&mut pr.head_sha));
    assert_eq!(
        deployment.send(4, "synchronize").await.0,
        StatusCode::ACCEPTED
    );
    let served = deployment.until(superseded, "cancelled").await;
    assert_eq!(served["error"]["code"], "pull_request_superseded");
    let (status, receipt) = deployment.publish_ci(C2, json!({"status": "green"})).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the binding still subscribes: {receipt}"
    );
    assert_eq!(receipt["matched_subscriptions"], 1);
    let served = deployment.status(superseded).await;
    assert_eq!(served["status"], "cancelled", "settled work never reopens");

    let current = deployment
        .rows(4)
        .await
        .into_iter()
        .find(|row| row.head == C3)
        .expect("the new head is admitted")
        .execution_id;
    deployment.until(current, "waiting").await;
    let served = deployment.until(current, "timed_out").await;
    assert!(
        served["result"].is_null(),
        "missing evidence never passes: {served}"
    );
}

/// Evidence published while its consumer is still running reaches it.
///
/// The consumer's first run is held running and then parks, as one that
/// read no `ci` does. Publishing `ci` meanwhile matches it, but the wake
/// cannot touch a running row; the parked row's bounded recheck runs the
/// real engine, which reads the accepted evidence, and the same execution
/// passes without a republish.
///
/// # Panics
/// Panics when the publication does not match the running consumer or the
/// consumer does not pass.
#[tokio::test(flavor = "multi_thread")]
async fn evidence_published_while_its_consumer_runs_reaches_it() {
    let deployment = Deployment::start_with(false).await;
    deployment.gate("1.1.0", "1.0.0").await;
    let script = EngineScript::default();
    script.hold();
    script.push(EngineOutcome::Parked(
        BTreeSet::from([SlotName::new("ci").expect("slot name")]),
        Vec::new(),
    ));
    let runtime = VerificationRuntime::builder(deployment.server.state())
        .limits(limits(Duration::from_mins(5)))
        .engine_script(script.clone())
        .build()
        .expect("the runtime composes");
    let stop = CancellationToken::new();
    let task = tokio::spawn(runtime.run(stop.clone()));

    let running = deployment.open(6, C1).await;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    while script.entered() == 0 {
        assert!(tokio::time::Instant::now() < deadline, "the consumer runs");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(deployment.status(running).await["status"], "running");
    let (status, receipt) = deployment.publish_ci(C1, json!({"status": "green"})).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    assert_eq!(
        receipt["matched_subscriptions"], 1,
        "the running consumer subscribes"
    );

    script.release();
    let served = deployment.until(running, "completed").await;
    assert_eq!(served["result"]["outcome"]["verdict"], "passed", "{served}");
    stop.cancel();
    task.await.expect("the runtime task does not panic");
}

/// Accepted evidence survives a restart and the waiting execution still
/// finishes.
///
/// The pull request's execution waits for `ci` under one verification
/// runtime, which then stops. `ci` published while no runtime runs is
/// accepted; a fresh runtime over the same deployment resumes the same
/// execution, which passes having spent one attempt.
///
/// # Panics
/// Panics when the execution does not wait, the publication is refused, or
/// the restarted runtime does not pass the same execution.
#[tokio::test(flavor = "multi_thread")]
async fn a_waiting_execution_finishes_after_a_restart() {
    let deployment = Deployment::start_with(false).await;
    deployment.gate("1.1.0", "1.0.0").await;
    let runtime = || {
        let stop = CancellationToken::new();
        let task = tokio::spawn(
            VerificationRuntime::builder(deployment.server.state())
                .limits(limits(Duration::from_mins(5)))
                .build()
                .expect("the runtime composes")
                .run(stop.clone()),
        );
        (stop, task)
    };

    let (stop, task) = runtime();
    let waiting = deployment.open(9, C1).await;
    deployment.until(waiting, "waiting").await;
    stop.cancel();
    task.await.expect("the runtime task does not panic");

    let (status, receipt) = deployment.publish_ci(C1, json!({"status": "green"})).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    assert_eq!(receipt["status"], "accepted", "{receipt}");
    assert_eq!(deployment.status(waiting).await["status"], "waiting");

    let (stop, task) = runtime();
    let served = deployment.until(waiting, "completed").await;
    assert_eq!(served["result"]["outcome"]["verdict"], "passed", "{served}");
    assert_eq!(deployment.spent(waiting).await, (1, "1.0.0".to_owned()));
    stop.cancel();
    task.await.expect("the runtime task does not panic");
}

/// A parked execution gives back its compute before its park is
/// acknowledged.
///
/// `ci-gate` here also runs an independent Python step. With one model
/// call, the step runs in Python and a workspace, then the execution
/// parks waiting for `ci`. While the park's write is held unacknowledged the
/// execution is still `running`, yet it holds no scratch, no
/// Python worker is checked out, and its model call is free; once the park
/// lands it waits, and publishing `ci` resumes it.
///
/// # Panics
/// Panics when compute is still held while the park is unacknowledged, or
/// the execution does not wait and then pass.
#[tokio::test(flavor = "multi_thread")]
async fn a_parked_execution_releases_compute_before_its_park_is_acknowledged() {
    let deployment = Deployment::launch(|builder| {
        builder
            .with_verification_runtime_for_test()
            .with_verification_runtime_limits_for_test(limits(Duration::from_mins(5)))
            .with_verification_limits_for_test(HostedLimits {
                monty_min_workers: 0,
                model_calls: 1,
                ..HostedLimits::default()
            })
    })
    .await;
    let measured = ci_gate("1.0.0").replace(
        "  requirements:\n",
        "  steps:\n    - id: measure\n      action:\n        kind: code\n        entrypoint: measure\n        source:\n          inline: |\n            def measure():\n                return {\"measured\": True}\n  requirements:\n    - id: measured\n      statement: Python ran\n      check: {kind: assert, path: steps.measure.output.measured, operator: equals, expected: true}\n",
    );
    deployment
        .declare_files(
            &[("ci-gate.yaml", measured)],
            &gated("1.1.0"),
            "gate-measured",
        )
        .await;
    let stall = deployment
        .server
        .stall_next_bifrost_write_into("bifrost.verification.execution_events")
        .expect("Scribe stalls");
    let parked = deployment.open(7, C1).await;
    stall.wait_entered().await;
    assert_eq!(deployment.status(parked).await["status"], "running");
    let blocks = deployment
        .server
        .state()
        .verification
        .blocks()
        .expect("blocks are hosted");
    let sandbox = blocks.sandbox();
    // The stalled write is the step's first revision, staged while Python
    // still runs; the park queues behind it, so it stays unacknowledged.
    // With no warm worker, an idle one proves the step ran and came back.
    let released = || {
        !sandbox.idle_worker_pids().is_empty()
            && blocks.resources().scratch().used() == 0
            && BTreeSet::from_iter(sandbox.charged_worker_pids())
                == BTreeSet::from_iter(sandbox.idle_worker_pids())
            && blocks.hold_model_calls_for_test().num_permits() == 1
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    while !released() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "compute is still held while the park is unacknowledged"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        deployment.status(parked).await["status"],
        "running",
        "the execution does not wait before its park is acknowledged"
    );

    stall.release();
    stall.wait_completed().await;
    let served = deployment.until(parked, "waiting").await;
    assert_eq!(served["waiting"], json!(["ci"]));
    let (status, receipt) = deployment.publish_ci(C1, json!({"status": "green"})).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    let served = deployment.until(parked, "completed").await;
    assert_eq!(served["result"]["outcome"]["verdict"], "passed", "{served}");
}

/// A finished execution gives back its compute before its terminal state is
/// acknowledged.
///
/// `ci-gate` here has no deferred input: one Python step and an assertion on
/// its output. While its state writes are held unacknowledged the execution
/// selects its terminal and stays `running`, yet holds no scratch, no Python
/// worker is checked out, and its model call is free; once the writes land
/// it completes and passes.
///
/// # Panics
/// Panics when compute is not released while the terminal is
/// unacknowledged, the execution completes before it is acknowledged, or it
/// does not then pass.
#[tokio::test(flavor = "multi_thread")]
async fn a_finished_execution_releases_compute_before_its_terminal_is_acknowledged() {
    let deployment = Deployment::launch(|builder| {
        builder
            .with_verification_runtime_for_test()
            .with_verification_runtime_limits_for_test(limits(Duration::from_mins(5)))
            .with_verification_limits_for_test(HostedLimits {
                monty_min_workers: 0,
                model_calls: 1,
                ..HostedLimits::default()
            })
    })
    .await;
    let measured = r#"apiVersion: wyrd/v1
kind: Verifier
metadata:
  name: ci-gate
  version: 1.0.0
spec:
  description: Python ran.
  steps:
    - id: measure
      action:
        kind: code
        entrypoint: measure
        source:
          inline: |
            def measure():
                return {"measured": True}
  requirements:
    - id: measured
      statement: Python ran
      check: {kind: assert, path: steps.measure.output.measured, operator: equals, expected: true}
"#;
    deployment
        .declare_files(
            &[("ci-gate.yaml", measured.to_owned())],
            &gated("1.1.0"),
            "gate-finished",
        )
        .await;
    let stall = deployment
        .server
        .stall_next_bifrost_write_into("bifrost.verification.execution_events")
        .expect("Scribe stalls");
    let finished = deployment.open(8, C1).await;
    stall.wait_entered().await;
    let blocks = deployment
        .server
        .state()
        .verification
        .blocks()
        .expect("blocks are hosted");
    let sandbox = blocks.sandbox();
    // With no warm worker, an idle one proves the step ran in Python and
    // its worker came back.
    let released = || {
        !sandbox.idle_worker_pids().is_empty()
            && blocks.resources().scratch().used() == 0
            && BTreeSet::from_iter(sandbox.charged_worker_pids())
                == BTreeSet::from_iter(sandbox.idle_worker_pids())
            && blocks.hold_model_calls_for_test().num_permits() == 1
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    while !released() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "compute is still held while the terminal is unacknowledged"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        deployment.status(finished).await["status"],
        "running",
        "the execution is not complete before its terminal is acknowledged"
    );

    stall.release();
    stall.wait_completed().await;
    let served = deployment.until(finished, "completed").await;
    assert_eq!(served["result"]["outcome"]["verdict"], "passed", "{served}");
}

/// A resumed execution keeps its admission deadline.
///
/// `ci-gate` here also defers `lint`. Halfway to the deadline `ci` is
/// published: the same execution resumes, parks again waiting for `lint`,
/// and times out at its original deadline, not one restarted by the resume.
///
/// # Panics
/// Panics when the execution does not resume onto `lint` or does not time
/// out by its admission deadline.
#[tokio::test(flavor = "multi_thread")]
async fn a_resumed_execution_keeps_its_admission_deadline() {
    const DEADLINE: Duration = Duration::from_secs(10);
    let deployment = Deployment::start_limited(limits(DEADLINE)).await;
    let both = ci_gate("1.0.0").replace(
        "  requirements:\n",
        "    lint: {type: json, deferred: true}\n  requirements:\n    - id: linted\n      statement: Lint is clean\n      check: {kind: assert, path: inputs.lint.clean, operator: equals, expected: true}\n",
    );
    deployment
        .declare_files(&[("ci-gate.yaml", both)], &gated("1.1.0"), "gate-both")
        .await;
    let admitted = tokio::time::Instant::now();
    let execution = deployment.open(5, C0).await;
    let served = deployment.until(execution, "waiting").await;
    assert_eq!(served["waiting"], json!(["ci", "lint"]));

    tokio::time::sleep_until(admitted + DEADLINE / 2).await;
    let (status, receipt) = deployment.publish_ci(C0, json!({"status": "green"})).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    while deployment.status(execution).await["waiting"] != json!(["lint"]) {
        assert!(
            tokio::time::Instant::now() < deadline,
            "{execution} resumes onto lint"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    deployment.until(execution, "timed_out").await;
    let mut conn = deployment.conn().await;
    let (lived,): (f64,) = sqlx::query_as(
        "SELECT EXTRACT(EPOCH FROM updated_at - created_at)::float8
           FROM wyrd.verifier_executions WHERE execution_id = $1",
    )
    .bind(execution)
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("execution reads");
    conn.commit().await.expect("read commits");
    assert!(
        lived < DEADLINE.as_secs_f64() + 3.0,
        "timed out {lived}s after admission; a deadline restarted at the resume ends after {}s",
        DEADLINE.as_secs_f64() * 1.5
    );
}

/// No receipt is given before Scribe acknowledges the acceptance: an
/// unacknowledged publication answers unavailable, not accepted or absent,
/// and the waiting execution keeps waiting. Once the held write lands, the
/// publisher's retry replays the exact accepted evidence and resumes the
/// execution.
///
/// # Panics
/// Panics when a receipt precedes the acknowledgement, or the retry does not
/// replay and resume.
#[tokio::test(flavor = "multi_thread")]
async fn unacknowledged_publication_is_unavailable_and_its_retry_resumes() {
    let deployment = Deployment::launch(|builder| {
        builder
            .with_verification_runtime_for_test()
            .with_verification_runtime_limits_for_test(limits(Duration::from_mins(5)))
            .with_limits_for_test(LimitsConfig {
                timeout: Duration::from_secs(4),
                ..LimitsConfig::default()
            })
    })
    .await;
    deployment.gate("1.1.0", "1.0.0").await;
    let waiting = deployment.open(5, C1).await;
    deployment.until(waiting, "waiting").await;

    let stall = deployment
        .server
        .stall_next_bifrost_write_into("bifrost.verification.evidence")
        .expect("Scribe stalls");
    let (status, unavailable) = deployment.publish_ci(C1, json!({"status": "green"})).await;
    stall.wait_entered().await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{unavailable}");
    assert_eq!(
        unavailable["code"],
        "WYRD_VALA_503_BIFROST_WRITER_UNAVAILABLE"
    );
    assert_eq!(deployment.status(waiting).await["status"], "waiting");

    stall.release();
    stall.wait_completed().await;
    let (status, replay) = deployment.publish_ci(C1, json!({"status": "green"})).await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    assert_eq!(
        replay["status"], "replayed",
        "the held acceptance landed and the retry reads it"
    );
    let served = deployment.until(waiting, "completed").await;
    assert_eq!(served["result"]["outcome"]["verdict"], "passed", "{served}");
}

/// Accepted evidence outlives its retry receipt: its Bifrost record stays
/// the authority once cleanup removes the receipt.
///
/// Evidence is accepted before any pull request and published, then its
/// receipt is removed. Publishing it again still replays the original
/// evidence and time, different content is still a conflict, and a pull
/// request opened later consumes it and passes. Reading acceptances never
/// touches receipts: while they cannot be read at all, a republication,
/// which only reads, still replays.
///
/// # Panics
/// Panics when the replay, conflict or consumption changes after the
/// receipt is removed, or publication depends on the receipts.
#[tokio::test(flavor = "multi_thread")]
async fn accepted_evidence_outlives_its_retry_receipt() {
    let deployment = Deployment::start_limited(limits(Duration::from_mins(5))).await;
    deployment.gate("1.1.0", "1.0.0").await;
    let (status, receipt) = deployment.publish_ci(C0, json!({"status": "green"})).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    assert_eq!(receipt["status"], "accepted");

    expire_receipts(&deployment.server, &["bifrost.verification.evidence"]).await;
    let (status, replay) = deployment.publish_ci(C0, json!({"status": "green"})).await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    assert_eq!(replay["status"], "replayed", "{replay}");
    assert_eq!(replay["evidence_id"], receipt["evidence_id"]);
    assert_eq!(replay["recorded_at"], receipt["recorded_at"]);
    let (status, conflict) = deployment.publish_ci(C0, json!({"status": "red"})).await;
    assert_eq!(status, StatusCode::CONFLICT, "{conflict}");
    assert_eq!(conflict["code"], "WYRD_VERIFICATION_409_EVIDENCE_CONFLICT");
    let consumer = deployment.open(1, C0).await;
    let served = deployment.until(consumer, "completed").await;
    assert_eq!(served["result"]["outcome"]["verdict"], "passed", "{served}");

    let pool = deployment
        .server
        .pg_fixture()
        .superuser_pool()
        .expect("fixture exposes a superuser pool");
    let grant = |statement: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query(statement)
                .execute(&pool)
                .await
                .expect("the receipt grant changes");
        }
    };
    grant("REVOKE SELECT ON vala.scribe_batch_commits FROM wyrd_tenant").await;
    let (status, replay) = deployment.publish_ci(C0, json!({"status": "green"})).await;
    grant("GRANT SELECT ON vala.scribe_batch_commits TO wyrd_tenant").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the replay reads no receipt: {replay}"
    );
    assert_eq!(replay["status"], "replayed", "{replay}");
}

/// A registered Artifact is admitted against the subscriber's slot before
/// it is accepted, and the waiting consumer then reads it.
///
/// `report-gate` reads its deferred CSV `report` and passes when it parses
/// as CSV; the pull request's execution waits for it. A publisher holding
/// `verifier:run` but not Artifact read, a file of the wrong format, a path
/// or version the registry does not hold, or a file past the slot's limit
/// is refused with nothing retained, so the right file then publishes as
/// `accepted` for the same commit and the same execution resumes and
/// passes.
///
/// # Panics
/// Panics when a refusal differs, the valid file is not accepted, or the
/// execution does not wait and then pass.
#[tokio::test(flavor = "multi_thread")]
async fn artifact_evidence_is_admitted_before_it_is_accepted() {
    use wyrd_runtime::Permission;

    let deployment = Deployment::start_limited(limits(Duration::from_mins(5))).await;
    deployment
        .declare_files(
            &[(
                "report-gate.yaml",
                "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: report-gate\n  version: 1.0.0\nspec:\n  description: The report reads as CSV.\n  inputs:\n    report: {type: media, formats: [csv], max_bytes: 64, deferred: true}\n  steps:\n    - id: read\n      action: {kind: extract, from: inputs.report}\n  requirements:\n    - id: readable\n      statement: The report reads as CSV\n      check: {kind: assert, path: steps.read.output.format, operator: equals, expected: csv}\n".to_owned(),
            )],
            "apiVersion: wyrd/v1\nkind: Repository\nmetadata:\n  name: policy\n  version: 1.1.0\nspec:\n  repository_id: \"4242\"\n  verified_by:\n    - verifier: ./report-gate.yaml\n      runs_on: {kind: pull_request}\n",
            "report-gate",
        )
        .await;
    let dir = tempfile::tempdir().expect("checkout creates");
    for (name, bytes) in [
        ("report.csv", "a,b\n1,2\n".to_owned()),
        ("big.csv", "a,b\n".repeat(32)),
        ("report.pdf", "%PDF-1.4\n".to_owned()),
    ] {
        std::fs::write(dir.path().join(name), bytes).expect("file writes");
    }
    std::fs::write(
        dir.path().join("reports.yaml"),
        "apiVersion: wyrd/v1\nkind: Artifact\nmetadata:\n  name: reports\n  version: 1.0.0\nartifacts:\n  - relative_path: report.csv\n    content_type: text/csv\n  - relative_path: big.csv\n    content_type: text/csv\n  - relative_path: report.pdf\n    content_type: application/pdf\nspec:\n  artifact_kind: dataset\n",
    )
    .expect("card writes");
    let input = build_declaration_input(load(&dir.path().join("reports.yaml")).expect("loads"))
        .expect("registration input builds");
    let reports = Cards::connect_with(&client(deployment.base(), &deployment.jwt))
        .await
        .expect("handle connects")
        .declare(&input)
        .await
        .expect("the Artifact registers")
        .root;
    let artifact = |version: &str, path: &str| {
        let mut card = serde_json::to_value(&reports).expect("reference serializes");
        card["version"] = json!(version);
        if version != "1.0.0" {
            card.as_object_mut()
                .expect("reference is an object")
                .remove("uid");
        }
        json!({"kind": "artifact", "artifact": card, "path": path})
    };

    let waiting = deployment.open(6, C0).await;
    assert_eq!(
        deployment.until(waiting, "waiting").await["waiting"],
        json!(["report"])
    );

    deployment
        .server
        .seed_role("evidence_runner", &[Permission::verifier_run_any()])
        .await
        .expect("role seeds");
    let Bootstrap::User { jwt: runner, .. } = deployment
        .server
        .bootstrap_user("evidence-runner", &["evidence_runner"])
        .await
        .expect("runner bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };
    let (status, denied) = deployment
        .publish_as(
            &runner,
            &publication(SLUG, C0, "report", artifact("1.0.0", "report.csv")),
        )
        .await;
    let invalid = "WYRD_VERIFICATION_400_INPUT_INVALID";
    assert_eq!(status, StatusCode::BAD_REQUEST, "{denied}");
    assert_eq!(denied["code"], invalid, "{denied}");

    for (value, status, code) in [
        (
            artifact("1.0.0", "report.pdf"),
            StatusCode::BAD_REQUEST,
            invalid,
        ),
        (
            artifact("1.0.0", "missing.csv"),
            StatusCode::BAD_REQUEST,
            invalid,
        ),
        (
            artifact("9.9.9", "report.csv"),
            StatusCode::BAD_REQUEST,
            invalid,
        ),
        (
            artifact("1.0.0", "big.csv"),
            StatusCode::PAYLOAD_TOO_LARGE,
            "WYRD_VERIFICATION_413_INPUT_TOO_LARGE",
        ),
    ] {
        let (answer, refused) = deployment
            .publish(&publication(SLUG, C0, "report", value.clone()))
            .await;
        assert_eq!(answer, status, "{value}: {refused}");
        assert_eq!(refused["code"], code, "{value}: {refused}");
    }
    let (status, receipt) = deployment
        .publish(&publication(
            SLUG,
            C0,
            "report",
            artifact("1.0.0", "report.csv"),
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    assert_eq!(
        receipt["status"], "accepted",
        "nothing was retained before: {receipt}"
    );
    let served = deployment.until(waiting, "completed").await;
    assert_eq!(served["result"]["outcome"]["verdict"], "passed", "{served}");
}

/// Losing repository access cancels a parked execution, and evidence
/// published afterwards is refused and reopens nothing.
///
/// # Panics
/// Panics when the parked execution survives the uninstall or the later
/// publication is accepted.
#[tokio::test(flavor = "multi_thread")]
async fn lost_repository_access_fences_a_parked_execution() {
    let deployment = Deployment::start_limited(limits(Duration::from_mins(5))).await;
    deployment.gate("1.1.0", "1.0.0").await;
    let parked = deployment.open(6, C1).await;
    deployment.until(parked, "waiting").await;
    let deleted = json!({"action": "deleted", "installation": {"id": INSTALLATION}});
    let status = deployment
        .mock
        .deliver(deployment.base(), "installation", &deleted)
        .await
        .expect("delivery sends");
    assert_eq!(status, StatusCode::ACCEPTED);
    let served = deployment.until(parked, "cancelled").await;
    assert_eq!(served["error"]["code"], "repository_access_lost");

    let (status, refused) = deployment.publish_ci(C1, json!({"status": "green"})).await;
    assert!(status.is_client_error(), "{status}: {refused}");
    assert_eq!(
        deployment.status(parked).await["status"],
        "cancelled",
        "settled work never reopens"
    );
}
