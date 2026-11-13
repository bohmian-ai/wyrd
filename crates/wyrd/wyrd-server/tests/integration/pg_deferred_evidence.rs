//! Deferred evidence published for a connected repository's commit and
//! consumed by its pull-request executions, through the real publish and
//! webhook routes, the queued runtime, Bifrost and repository Postgres.
//!
//! The tenant connects `acme/policy` (4242) as in
//! [`pg_github_pull_requests`](super::pg_github_pull_requests) and binds
//! `ci-gate`, whose `ci` slot is deferred, to its pull requests.

use std::time::Duration;

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use serde_json::{Value, json};
use uuid::Uuid;
use wyrd_server::verification::RuntimeLimits;
use wyrd_testing::github::PullRequest;

use super::pg_github_pull_requests::{Deployment, REPOSITORY, UNSELECTED};

/// A commit no pull request has opened yet.
const C0: &str = "c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0";
/// A commit first published to before anything subscribes.
const C1: &str = "c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1";
/// A head superseded while it waits.
const C2: &str = "c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2";
/// The head that supersedes [`C2`] and is never published to.
const C3: &str = "c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3";

/// The `ci-gate` Verifier at `version`: it passes when the deferred `ci`
/// evidence reports `status: green`.
fn ci_gate(version: &str) -> String {
    format!(
        "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: ci-gate\n  version: {version}\nspec:\n  description: CI is green.\n  inputs:\n    ci: {{type: json, deferred: true}}\n  requirements:\n    - id: green\n      statement: CI is green\n      check: {{kind: assert, path: inputs.ci.status, operator: equals, expected: green}}\n"
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

    /// Publish `body` and return the status and JSON answer.
    ///
    /// # Panics
    /// Panics when the request cannot be served.
    async fn publish(&self, body: &Value) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/verification/evidence/publish")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .expect("request builds");
        let response = self
            .server
            .oneshot_authenticated(&self.jwt, request)
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
    async fn publish_ci(&self, commit: &str, value: Value) -> (StatusCode, Value) {
        self.publish(&publication(
            &self.mock.host(),
            REPOSITORY,
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
    async fn open(&self, number: u64, head: &str) -> Uuid {
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
    /// Panics when it is not within a minute.
    async fn until(&self, execution: Uuid, status: &str) -> Value {
        let deadline = tokio::time::Instant::now() + Duration::from_mins(1);
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

    /// The attempts `execution` has spent and the Verifier version it froze.
    ///
    /// # Panics
    /// Panics when the read fails.
    async fn spent(&self, execution: Uuid) -> (i32, String) {
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
fn publication(host: &str, repository: u64, commit: &str, input: &str, value: Value) -> Value {
    json!({
        "repository": {"host": host, "repository_id": repository.to_string()},
        "tags": {"commit": commit},
        "input": input,
        "value": value,
    })
}

/// Runtime limits that settle work quickly and end it after `deadline`.
fn limits(deadline: Duration) -> RuntimeLimits {
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
/// retains nothing. Once `ci-gate` is bound, malformed or foreign
/// publications are refused; evidence published before any pull request is
/// accepted, replays identically and refuses different content, and a pull
/// request opened later consumes it without republishing. A pull request on
/// the commit refused earlier waits naming `ci`; publishing while a newer
/// policy pins another `ci-gate` version reaches both, and resumes the same
/// execution under its own pin; the park spends no attempt. A later
/// subscriber receives no historical evidence and waits.
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
    let host = deployment.mock.host();
    let json_value = json!({"kind": "json", "value": {"status": "green"}});
    for (body, expected) in [
        (
            publication(&host, REPOSITORY, "c0c0c0c", "ci", json_value.clone()),
            StatusCode::BAD_REQUEST,
        ),
        (
            publication(&host, REPOSITORY, C0, "other", json_value.clone()),
            StatusCode::CONFLICT,
        ),
        (
            publication(&host, UNSELECTED, C0, "ci", json_value.clone()),
            StatusCode::FORBIDDEN,
        ),
        (
            publication(
                &host,
                REPOSITORY,
                C0,
                "ci",
                json!({"kind": "file", "sha256": "ab".repeat(32), "size_bytes": 3, "format": "csv"}),
            ),
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let (status, answer) = deployment.publish(&body).await;
        assert_eq!(status, expected, "{body}: {answer}");
    }

    let (status, receipt) = deployment.publish_ci(C0, json!({"status": "green"})).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    assert_eq!(receipt["status"], "accepted");
    assert_eq!(receipt["matched_subscriptions"], 1);
    assert_eq!(receipt["tags"]["commit"], C0);
    let (status, replay) = deployment.publish_ci(C0, json!({"status": "green"})).await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    assert_eq!(replay["status"], "replayed");
    assert_eq!(replay["evidence_id"], receipt["evidence_id"]);
    assert_eq!(replay["recorded_at"], receipt["recorded_at"]);
    let (status, conflict) = deployment.publish_ci(C0, json!({"status": "red"})).await;
    assert_eq!(status, StatusCode::CONFLICT, "{conflict}");
    assert_eq!(conflict["code"], "WYRD_VERIFICATION_409_EVIDENCE_CONFLICT");

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

    deployment.gate("1.3.0", "1.0.2").await;
    let later = deployment.open(3, C1).await;
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
