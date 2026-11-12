//! Pull-request deliveries admitted into the Verifier queue through the real
//! webhook route, against the GitHub App mock and repository Postgres.
//!
//! The tenant is connected to installation 101 with `acme/policy` (4242)
//! selected and declares the `policy` Repository binding two queued review
//! Verifiers to its pull requests. The default test server holds the
//! Verifier runner, so admitted work stays queued until a test claims it.

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use chrono::Duration;
use serde_json::{Value, json};
use uuid::Uuid;
use wyrd_loader::{build_declaration_input, load};
use wyrd_server::components::github::service::GithubConnections;
use wyrd_sql::queries::verifier_executions::{
    Settlement, TerminalDelivery, TerminalStatus, VerifierExecutionQueue,
};
use wyrd_testing::github::{GithubAppMock, PullRequest};
use wyrd_testing::{Bootstrap, WyrdTestServer};

/// The connected repository.
const REPOSITORY: u64 = 4242;
/// The connected installation.
const INSTALLATION: u64 = 101;
/// The Repository binding two queued review Verifiers to pull requests.
const POLICY: &str = "apiVersion: wyrd/v1\nkind: Repository\nmetadata:\n  name: policy\n  version: 1.0.0\nspec:\n  repository_id: \"4242\"\n  verified_by:\n    - verifier: ./a-review.yaml\n      runs_on: {kind: pull_request}\n    - verifier: ./z-review.yaml\n      runs_on: {kind: pull_request}\n";

/// An exact queued Verifier named `name`.
fn review_verifier(name: &str) -> String {
    format!(
        "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: {name}\n  version: 1.0.0\nspec:\n  description: The answer is approve.\n  inputs:\n    answer: {{type: json}}\n  requirements:\n    - id: approves\n      statement: The answer is approve\n      check: {{kind: assert, path: inputs.answer, operator: equals, expected: approve}}\n"
    )
}

/// One pull-request execution as the queue holds it.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct Row {
    /// The execution.
    execution_id: Uuid,
    /// Its status.
    status: String,
    /// Its settled error code.
    code: Option<String>,
    /// The frozen head commit.
    head: String,
    /// Whether it is reserved, not yet due.
    reserved: bool,
}

/// A connected, declared deployment: the mock, the server, and the
/// declaring editor's id and JWT.
struct Deployment {
    /// GitHub.
    mock: GithubAppMock,
    /// Wyrd.
    server: WyrdTestServer,
    /// The declaring editor's JWT.
    jwt: String,
}

impl Deployment {
    /// Start the deployment, connect the tenant, and declare [`POLICY`].
    ///
    /// # Panics
    /// Panics when a step fails.
    async fn start() -> Self {
        let mock = GithubAppMock::start().expect("the GitHub mock starts");
        mock.add_installation(INSTALLATION, &[(REPOSITORY, "acme/policy")]);
        let server = WyrdTestServer::builder()
            .with_github_app_for_test(&mock)
            .start_bound()
            .await
            .expect("bound test server starts");
        Self::connect(&server, &mock).await;
        let Bootstrap::User { jwt, .. } = server
            .bootstrap_user("pull-request-editor", &["editor"])
            .await
            .expect("editor bootstraps")
        else {
            panic!("user bootstrap returned a non-user principal");
        };
        let deployment = Self { mock, server, jwt };
        deployment.declare().await;
        deployment
    }

    /// Associate the tenant's slot with installation 101 selecting
    /// `acme/policy`.
    ///
    /// # Panics
    /// Panics when the slot cannot be written.
    async fn connect(server: &WyrdTestServer, mock: &GithubAppMock) {
        let mut conn = server
            .tenant_conn_for(server.data_tenant_id())
            .await
            .expect("tenant connection opens");
        sqlx::query(
            "INSERT INTO wyrd.github_installations
                 (data_tenant_id, connection_id, revision, host, installation_id, repositories,
                  connected_at)
             VALUES ($1, $2, 1, $3, 101,
                     '[{\"repository_id\":\"4242\",\"full_name\":\"acme/policy\"}]'::jsonb,
                     statement_timestamp())",
        )
        .bind(server.data_tenant_id().as_uuid())
        .bind(Uuid::now_v7())
        .bind(mock.host())
        .execute(&mut **conn.transaction())
        .await
        .expect("slot associates");
        conn.commit().await.expect("slot commits");
    }

    /// Declare [`POLICY`] with both review Verifiers.
    ///
    /// # Panics
    /// Panics when the declaration is refused.
    async fn declare(&self) {
        let dir = tempfile::tempdir().expect("checkout creates");
        for (name, contents) in [
            ("a-review.yaml", review_verifier("a-review")),
            ("z-review.yaml", review_verifier("z-review")),
            ("policy.yaml", POLICY.to_owned()),
        ] {
            std::fs::write(dir.path().join(name), contents).expect("card file writes");
        }
        let input = build_declaration_input(load(&dir.path().join("policy.yaml")).expect("loads"))
            .expect("declaration input builds");
        let body = serde_json::to_value(input.to_declare_card_request()).expect("serializes");
        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/cards")
            .header(header::CONTENT_TYPE, "application/json")
            .header("Idempotency-Key", "pull-request-policy")
            .body(Body::from(body.to_string()))
            .expect("request builds");
        let response = self
            .server
            .oneshot_authenticated(&self.jwt, request)
            .await
            .expect("declaration responds");
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    /// The server's base URL.
    fn base(&self) -> &str {
        self.server.base_url().expect("bound")
    }

    /// Deliver `action` on PR `number` from GitHub's current state; returns
    /// the status and delivery id.
    ///
    /// # Panics
    /// Panics when the request cannot be sent.
    async fn send(&self, number: u64, action: &str) -> (StatusCode, String) {
        let payload = self
            .mock
            .pull_request_event(INSTALLATION, REPOSITORY, number, action);
        self.mock
            .deliver_new(self.base(), "pull_request", &payload)
            .await
            .expect("delivery sends")
    }

    /// Every execution of PR `number`, by admission order.
    ///
    /// # Panics
    /// Panics when the read fails.
    async fn rows(&self, number: u64) -> Vec<Row> {
        let mut conn = self
            .server
            .tenant_conn_for(self.server.data_tenant_id())
            .await
            .expect("tenant connection opens");
        let rows = sqlx::query_as::<_, Row>(
            "SELECT execution_id, status, error->>'code' AS code,
                    pull_request->'head'->>'sha' AS head,
                    next_attempt_at IS NOT DISTINCT FROM 'infinity' AS reserved
               FROM wyrd.verifier_executions
              WHERE origin = 'pull_request' AND (pull_request->>'number')::bigint = $1
              ORDER BY pr_observed_at, binding_id",
        )
        .bind(i64::try_from(number).expect("small"))
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("executions read");
        conn.commit().await.expect("read commits");
        rows
    }

    /// The served status of `execution`.
    ///
    /// # Panics
    /// Panics when the read fails.
    async fn status(&self, execution: Uuid) -> Value {
        let request = Request::builder()
            .uri(format!("/v1/verification/executions/{execution}"))
            .body(Body::empty())
            .expect("request builds");
        let response = self
            .server
            .oneshot_authenticated(&self.jwt, request)
            .await
            .expect("status responds");
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 1 << 20)
            .await
            .expect("body reads");
        serde_json::from_slice(&bytes).expect("status is JSON")
    }
}

/// The status, code and head of `rows`.
fn summary(rows: &[Row]) -> Vec<(&str, Option<&str>, &str)> {
    rows.iter()
        .map(|row| (row.status.as_str(), row.code.as_deref(), row.head.as_str()))
        .collect()
}

/// A full PR lifecycle through the webhook route.
///
/// Opening admits one frozen, due execution per binding with its pins, and
/// answers only after that commit. A duplicate delivery, and a delivery
/// whose answer was lost and is recovered from GitHub's delivery log,
/// admit nothing twice. A new head supersedes the queued and the running
/// generation, fencing the running worker and keeping its history. A
/// metadata-only edit admits nothing. A delivery whose provider read
/// observed the PR before a newer head committed cannot make its older
/// source current. Draft conversion, close and access loss cancel unfinished
/// work with their codes; ready and reopen admit a new generation.
///
/// # Panics
/// Panics when an admission, replay, supersession or cancellation differs.
#[tokio::test(flavor = "multi_thread")]
async fn pull_request_lifecycle_admits_supersedes_and_ends_through_the_webhook() {
    let deployment = Deployment::start().await;
    let mock = &deployment.mock;
    mock.set_pull_request(REPOSITORY, 7, PullRequest::open(REPOSITORY, "b1", "a1"));

    let (status, opened) = deployment.send(7, "opened").await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let a = deployment.rows(7).await;
    assert_eq!(
        summary(&a),
        [("pending", None, "a1"), ("pending", None, "a1")],
        "one due execution per binding, committed before the answer"
    );
    assert!(a.iter().all(|row| !row.reserved));
    let served = deployment.status(a[0].execution_id).await;
    assert_eq!(served["pull_request"]["head"]["sha"], "a1");
    assert_eq!(served["pull_request"]["base"]["ref"], "main");
    assert_eq!(served["pull_request"]["repository_id"], "4242");
    assert_eq!(served["pull_request"]["number"], 7);
    assert_eq!(served["pull_request"]["delivery_id"], opened.as_str());

    mock.lose_answer(&opened);
    let asked = GithubConnections::new(deployment.server.state())
        .recover_deliveries()
        .await
        .expect("the delivery log reads");
    assert_eq!(
        asked, 1,
        "only the delivery without a 2xx answer is redelivered"
    );
    assert_eq!(
        mock.delivery_attempts(&opened).last(),
        Some(&(202, true)),
        "the recovered redelivery is answered"
    );
    assert_eq!(
        GithubConnections::new(deployment.server.state())
            .recover_deliveries()
            .await
            .expect("the delivery log reads"),
        0,
        "a recovered delivery is not redelivered again"
    );
    assert_eq!(
        mock.redeliver(&opened).await.expect("redelivers"),
        StatusCode::ACCEPTED,
        "a manual redelivery of an admitted delivery is answered"
    );
    assert_eq!(deployment.rows(7).await, a, "replays admit nothing twice");

    let queue = VerifierExecutionQueue::default();
    let mut conn = deployment
        .server
        .tenant_conn_for(deployment.server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let running = queue
        .claim(&mut conn, Duration::minutes(5), Duration::minutes(5), 1)
        .await
        .expect("claim runs")
        .pop()
        .expect("A is claimable");
    conn.commit().await.expect("claim commits");

    mock.edit_pull_request(REPOSITORY, 7, |pr| pr.head_sha = "b2".to_owned());
    assert_eq!(
        deployment.send(7, "synchronize").await.0,
        StatusCode::ACCEPTED
    );
    let rows = deployment.rows(7).await;
    assert_eq!(
        summary(&rows),
        [
            ("cancelled", Some("pull_request_superseded"), "a1"),
            ("cancelled", Some("pull_request_superseded"), "a1"),
            ("pending", None, "b2"),
            ("pending", None, "b2"),
        ]
    );
    let mut conn = deployment
        .server
        .tenant_conn_for(deployment.server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let fenced = queue
        .terminate(
            &mut conn,
            running.lease,
            TerminalStatus::Errored,
            &wyrd_spec::verification::VerificationError {
                code: "late".to_owned(),
                message: "the superseded worker finished late".to_owned(),
            },
            TerminalDelivery::Owed,
        )
        .await
        .expect("settlement runs");
    conn.commit().await.expect("settlement commits");
    assert_eq!(
        fenced,
        Settlement::StaleLease,
        "the running worker is fenced"
    );
    let history = deployment.status(a[0].execution_id).await;
    assert_eq!(history["status"], "cancelled");
    assert_eq!(history["error"]["code"], "pull_request_superseded");
    assert_eq!(
        history["pull_request"]["head"]["sha"], "a1",
        "history keeps its pins"
    );

    let edited = json!({
        "action": "edited",
        "number": 7,
        "changes": {"title": {"from": "old"}},
        "repository": {"id": REPOSITORY},
        "installation": {"id": INSTALLATION},
    });
    let (status, _) = mock
        .deliver_new(deployment.base(), "pull_request", &edited)
        .await
        .expect("delivery sends");
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(
        deployment.rows(7).await.len(),
        4,
        "a metadata edit admits nothing"
    );

    let held = mock.hold_next_pull_request_read();
    let delayed = {
        let payload = mock.pull_request_event(INSTALLATION, REPOSITORY, 7, "synchronize");
        let mock = mock.clone();
        let base = deployment.base().to_owned();
        tokio::spawn(async move { mock.deliver_new(&base, "pull_request", &payload).await })
    };
    held.arrived().await;
    mock.edit_pull_request(REPOSITORY, 7, |pr| pr.head_sha = "c3".to_owned());
    assert_eq!(
        deployment.send(7, "synchronize").await.0,
        StatusCode::ACCEPTED
    );
    held.release();
    let (status, _) = delayed.await.expect("joins").expect("delivery sends");
    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "an obsolete observation owes no work"
    );
    let rows = deployment.rows(7).await;
    assert_eq!(
        summary(&rows[2..]),
        [
            ("cancelled", Some("pull_request_superseded"), "b2"),
            ("cancelled", Some("pull_request_superseded"), "b2"),
            ("pending", None, "c3"),
            ("pending", None, "c3"),
        ],
        "queued B is replaced by C and the delayed read of B never returns"
    );

    mock.edit_pull_request(REPOSITORY, 7, |pr| pr.draft = true);
    assert_eq!(
        deployment.send(7, "converted_to_draft").await.0,
        StatusCode::ACCEPTED
    );
    let rows = deployment.rows(7).await;
    assert_eq!(
        summary(&rows[4..]),
        [
            ("cancelled", Some("pull_request_draft"), "c3"),
            ("cancelled", Some("pull_request_draft"), "c3"),
        ]
    );
    assert_eq!(
        deployment.send(7, "synchronize").await.0,
        StatusCode::ACCEPTED
    );
    assert_eq!(deployment.rows(7).await.len(), 6, "a draft admits nothing");

    mock.edit_pull_request(REPOSITORY, 7, |pr| pr.draft = false);
    assert_eq!(
        deployment.send(7, "ready_for_review").await.0,
        StatusCode::ACCEPTED
    );
    mock.edit_pull_request(REPOSITORY, 7, |pr| pr.open = false);
    assert_eq!(deployment.send(7, "closed").await.0, StatusCode::ACCEPTED);
    let rows = deployment.rows(7).await;
    assert_eq!(
        summary(&rows[6..]),
        [
            ("cancelled", Some("pull_request_closed"), "c3"),
            ("cancelled", Some("pull_request_closed"), "c3"),
        ],
        "ready admits the same source anew, and close cancels it"
    );

    mock.edit_pull_request(REPOSITORY, 7, |pr| pr.open = true);
    assert_eq!(deployment.send(7, "reopened").await.0, StatusCode::ACCEPTED);
    assert_eq!(
        deployment.rows(7).await.len(),
        10,
        "reopen admits a new generation"
    );
    let removed = json!({
        "action": "removed",
        "installation": {"id": INSTALLATION},
        "repositories_removed": [{"id": REPOSITORY}],
    });
    let status = mock
        .deliver(deployment.base(), "installation_repositories", &removed)
        .await
        .expect("delivery sends");
    assert_eq!(status, StatusCode::ACCEPTED);
    let rows = deployment.rows(7).await;
    assert_eq!(
        summary(&rows[8..]),
        [
            ("cancelled", Some("repository_access_lost"), "c3"),
            ("cancelled", Some("repository_access_lost"), "c3"),
        ]
    );
    assert_eq!(
        summary(&rows[..2]),
        [
            ("cancelled", Some("pull_request_superseded"), "a1"),
            ("cancelled", Some("pull_request_superseded"), "a1"),
        ],
        "access loss keeps settled history"
    );
}

/// Forty-one PRs opened at once each admit exactly one due execution per
/// binding, every delivery answered only after its admission committed.
///
/// # Panics
/// Panics when a delivery is refused or any PR's fanout differs.
#[tokio::test(flavor = "multi_thread")]
async fn burst_of_forty_one_pull_requests_admits_each_binding_once() {
    let deployment = Deployment::start().await;
    for number in 1..=41 {
        deployment.mock.set_pull_request(
            REPOSITORY,
            number,
            PullRequest::open(REPOSITORY, "b1", &format!("h{number}")),
        );
    }
    let mut burst = tokio::task::JoinSet::new();
    for number in 1..=41 {
        let payload =
            deployment
                .mock
                .pull_request_event(INSTALLATION, REPOSITORY, number, "opened");
        let mock = deployment.mock.clone();
        let base = deployment.base().to_owned();
        burst.spawn(async move { mock.deliver_new(&base, "pull_request", &payload).await });
    }
    while let Some(answer) = burst.join_next().await {
        let (status, _) = answer.expect("joins").expect("delivery sends");
        assert_eq!(status, StatusCode::ACCEPTED);
    }
    for number in 1..=41 {
        let rows = deployment.rows(number).await;
        let head = format!("h{number}");
        assert_eq!(
            summary(&rows),
            [
                ("pending", None, head.as_str()),
                ("pending", None, head.as_str())
            ],
            "PR {number}"
        );
    }
}

/// A delivery interrupted after its generation was reserved, with its input
/// handoff still unacknowledged, is answered with no 2xx; after a restart,
/// failed-delivery recovery redelivers it and completes the same frozen
/// fanout under the same execution IDs, never a second generation.
///
/// # Panics
/// Panics when the interrupted delivery is acknowledged, or recovery admits
/// other or duplicate executions.
#[tokio::test(flavor = "multi_thread")]
async fn interrupted_handoff_completes_the_same_fanout_after_restart() {
    let Deployment {
        mock,
        server,
        jwt: _,
    } = Deployment::start().await;
    mock.set_pull_request(REPOSITORY, 9, PullRequest::open(REPOSITORY, "b1", "a1"));
    server.flush_bifrost().await.expect("setup settles");
    let held = server
        .stall_next_bifrost_write_into("bifrost.verification.execution_inputs")
        .expect("Scribe stalls");
    let payload = mock.pull_request_event(INSTALLATION, REPOSITORY, 9, "opened");
    let delivery = {
        let mock = mock.clone();
        let base = server.base_url().expect("bound").to_owned();
        tokio::spawn(async move { mock.deliver_new(&base, "pull_request", &payload).await })
    };
    held.wait_entered().await;
    let deployment = Deployment {
        mock: mock.clone(),
        server,
        jwt: String::new(),
    };
    let reserved = deployment.rows(9).await;
    assert_eq!(
        reserved.len(),
        2,
        "the generation is frozen before the handoff"
    );
    let restarted = deployment
        .server
        .restart_bound(WyrdTestServer::builder().with_github_app_for_test(&mock))
        .await
        .expect("server restarts");
    held.release();
    let answered = delivery.await.expect("joins");
    assert!(
        !matches!(answered, Ok((StatusCode::ACCEPTED, _))),
        "an interrupted handoff is not acknowledged: {answered:?}"
    );
    let Bootstrap::User { jwt, .. } = restarted
        .bootstrap_user("pull-request-reader", &["editor"])
        .await
        .expect("reader bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };
    let deployment = Deployment {
        mock,
        server: restarted,
        jwt,
    };
    let asked = GithubConnections::new(deployment.server.state())
        .recover_deliveries()
        .await
        .expect("the delivery log reads");
    assert_eq!(asked, 1);
    let rows = deployment.rows(9).await;
    assert_eq!(
        rows.iter().map(|row| row.execution_id).collect::<Vec<_>>(),
        reserved
            .iter()
            .map(|row| row.execution_id)
            .collect::<Vec<_>>(),
        "recovery completes the same executions"
    );
    assert_eq!(
        summary(&rows),
        [("pending", None, "a1"), ("pending", None, "a1")],
        "every binding of the frozen fanout is due once"
    );
    for row in &rows {
        let served = deployment.status(row.execution_id).await;
        assert_eq!(served["execution_id"], row.execution_id.to_string());
    }
}

/// When the declaring principal can no longer act, an open PR settles one
/// errored execution per binding with the refusal, runs nothing, and is
/// still answered: no work is owed.
///
/// # Panics
/// Panics when the delivery is refused or work becomes runnable.
#[tokio::test(flavor = "multi_thread")]
async fn revoked_accountable_principal_settles_a_durable_refusal() {
    let deployment = Deployment::start().await;
    let pool = deployment
        .server
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    sqlx::query(
        "UPDATE wyrd.auth_users SET status = 'suspended'
          WHERE id = (SELECT o.sub_principal_id FROM wyrd.cards c
                        JOIN wyrd.card_declaration_operations o
                          ON o.operation_id = c.selection_occurrence
                       WHERE c.kind = 'Repository' AND c.name = 'policy')",
    )
    .execute(&pool)
    .await
    .expect("the editor is suspended");
    deployment
        .mock
        .set_pull_request(REPOSITORY, 3, PullRequest::open(REPOSITORY, "b1", "a1"));

    assert_eq!(deployment.send(3, "opened").await.0, StatusCode::ACCEPTED);
    let rows = deployment.rows(3).await;
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .all(|row| row.status == "errored" && row.code.is_some()),
        "{rows:?}"
    );
    let queue = VerifierExecutionQueue::default();
    let mut conn = deployment
        .server
        .tenant_conn_for(deployment.server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    assert!(
        queue
            .claim(&mut conn, Duration::minutes(5), Duration::minutes(5), 10)
            .await
            .expect("claim runs")
            .is_empty(),
        "a refusal is not runnable work"
    );
}
