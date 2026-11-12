//! Pull-request deliveries admitted into the Verifier queue through the real
//! webhook route, against the GitHub App mock and repository Postgres.
//!
//! The tenant connects to installation 101 through GitHub's install page
//! with `acme/policy` (4242) selected and declares the `policy` Repository binding two queued review
//! Verifiers to its pull requests. The default test server holds the
//! Verifier runner, so admitted work stays queued until a test claims it.

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use chrono::Duration;
use secrecy::ExposeSecret;
use serde_json::{Value, json};
use uuid::Uuid;
use wyrd_loader::{build_declaration_input, load};
use wyrd_server::components::github::service::GithubConnections;
use wyrd_spec::ids::VerificationResultId;
use wyrd_spec::verification::VerificationVerdict;
use wyrd_sql::queries::verifier_executions::{
    Settlement, TerminalDelivery, TerminalStatus, VerifierExecutionQueue,
};
use wyrd_testing::github::{GithubAppMock, GithubBrowser, PullRequest};
use wyrd_testing::{Bootstrap, WyrdTestServer};

/// The connected repository.
const REPOSITORY: u64 = 4242;
/// A repository the installation reaches that the tenant did not select.
const UNSELECTED: u64 = 4343;
/// The connected installation.
const INSTALLATION: u64 = 101;
/// The Repository binding two queued review Verifiers to pull requests:
/// `a-review` through an inline Trigger, `z-review` through the referenced
/// [`ON_PULL_REQUEST`] Trigger Card.
const POLICY: &str = "apiVersion: wyrd/v1\nkind: Repository\nmetadata:\n  name: policy\n  version: 1.0.0\nspec:\n  repository_id: \"4242\"\n  verified_by:\n    - verifier: ./a-review.yaml\n      runs_on: {kind: pull_request}\n    - verifier: ./z-review.yaml\n      runs_on: ./on-pull-request.yaml\n";

/// A referenced `pull_request` Trigger Card.
const ON_PULL_REQUEST: &str = "apiVersion: wyrd/v1\nkind: Trigger\nmetadata:\n  name: on-pull-request\n  version: 1.0.0\nspec:\n  description: Every ready pull request.\n  kind: pull_request\n";

/// [`POLICY`] re-declared binding only `a-review`.
const POLICY_V2: &str = "apiVersion: wyrd/v1\nkind: Repository\nmetadata:\n  name: policy\n  version: 1.0.1\nspec:\n  repository_id: \"4242\"\n  verified_by:\n    - verifier: ./a-review.yaml\n      runs_on: {kind: pull_request}\n";

/// [`POLICY`] re-declared binding no Verifier.
const POLICY_EMPTY: &str = "apiVersion: wyrd/v1\nkind: Repository\nmetadata:\n  name: policy\n  version: 1.0.2\nspec:\n  repository_id: \"4242\"\n  verified_by: []\n";

/// [`POLICY`] re-declared again with both bindings, after [`POLICY_EMPTY`].
const POLICY_RESTORED: &str = "apiVersion: wyrd/v1\nkind: Repository\nmetadata:\n  name: policy\n  version: 1.0.3\nspec:\n  repository_id: \"4242\"\n  verified_by:\n    - verifier: ./a-review.yaml\n      runs_on: {kind: pull_request}\n    - verifier: ./z-review.yaml\n      runs_on: ./on-pull-request.yaml\n";

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
        Self::start_with(false).await
    }

    /// Start the deployment as [`Self::start`] does, running queued work
    /// when `runtime` is set.
    ///
    /// # Panics
    /// Panics when a step fails.
    async fn start_with(runtime: bool) -> Self {
        let mock = GithubAppMock::start().expect("the GitHub mock starts");
        mock.add_installation(INSTALLATION, &[(REPOSITORY, "acme/policy")]);
        mock.add_user("octo", &[INSTALLATION]);
        mock.sign_in("octo");
        mock.install_target(INSTALLATION);
        let mut builder = WyrdTestServer::builder().with_github_app_for_test(&mock);
        if runtime {
            builder = builder.with_verification_runtime_for_test();
        }
        let server = builder
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
        deployment.declare(POLICY, "pull-request-policy").await;
        deployment
    }

    /// Connect the tenant to installation 101 the way an administrator
    /// does, through GitHub's install page, selecting `acme/policy`; then
    /// let the installation reach `acme/other` without Wyrd being told, so
    /// it stays unselected for the tenant.
    ///
    /// # Panics
    /// Panics when the connection fails.
    async fn connect(server: &WyrdTestServer, mock: &GithubAppMock) {
        install(
            server,
            mock,
            server.data_tenant_id(),
            server.pg_fixture().tenant_slug(),
            "github_admin",
        )
        .await;
        mock.add_installation(
            INSTALLATION,
            &[(REPOSITORY, "acme/policy"), (UNSELECTED, "acme/other")],
        );
    }

    /// Declare the Repository `policy` with both review Verifiers under
    /// idempotency `key`.
    ///
    /// # Panics
    /// Panics when the declaration is refused.
    async fn declare(&self, policy: &str, key: &str) {
        let dir = tempfile::tempdir().expect("checkout creates");
        for (name, contents) in [
            ("a-review.yaml", review_verifier("a-review")),
            ("z-review.yaml", review_verifier("z-review")),
            ("on-pull-request.yaml", ON_PULL_REQUEST.to_owned()),
            ("policy.yaml", policy.to_owned()),
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
            .header("Idempotency-Key", key)
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
    ///
    /// # Panics
    /// Panics when the server was started without a bound listener.
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

    /// The frozen Verifier name and version, the admitting Repository
    /// version, and whether the Trigger is a referenced Card (else an
    /// inline digest) of every execution of PR `number`, in [`Self::rows`]
    /// order.
    ///
    /// # Panics
    /// Panics when the read fails.
    async fn frozen(&self, number: u64) -> Vec<(String, String, String, bool)> {
        let mut conn = self.conn().await;
        let rows = sqlx::query_as(
            "SELECT v.name, v.version, o.version, e.trigger_uid IS NOT NULL
               FROM wyrd.verifier_executions e
               JOIN wyrd.cards v ON v.card_uid = e.verifier_card_uid
               JOIN wyrd.cards o ON o.card_uid = e.owner_card_uid
              WHERE e.origin = 'pull_request' AND (e.pull_request->>'number')::bigint = $1
              ORDER BY e.pr_observed_at, e.binding_id",
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

impl Deployment {
    /// Deliver `action` on PR `number` from GitHub's current state on its
    /// own task; joins to the status and delivery id.
    ///
    /// # Panics
    /// Panics when the PR does not exist on GitHub; the task panics when
    /// the request cannot be sent.
    fn spawn_send(
        &self,
        number: u64,
        action: &str,
    ) -> tokio::task::JoinHandle<(StatusCode, String)> {
        let payload = self
            .mock
            .pull_request_event(INSTALLATION, REPOSITORY, number, action);
        let mock = self.mock.clone();
        let base = self.base().to_owned();
        tokio::spawn(async move {
            mock.deliver_new(&base, "pull_request", &payload)
                .await
                .expect("delivery sends")
        })
    }

    /// Wait until `delivery` either finished or waits on a pull request's
    /// lock, and return whether it waits.
    ///
    /// # Panics
    /// Panics when it does neither within a minute.
    async fn waits_on_the_pull_request(
        &self,
        delivery: &tokio::task::JoinHandle<(StatusCode, String)>,
    ) -> bool {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_mins(1);
        loop {
            let mut conn = self.conn().await;
            let waiting: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM pg_locks
                  WHERE locktype = 'advisory' AND NOT granted
                    AND classid = x'0CA2D032'::int::oid AND objsubid = 2
                    AND database = (SELECT oid FROM pg_database WHERE datname = current_database())",
            )
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("locks read");
            conn.commit().await.expect("read commits");
            if waiting > 0 {
                return true;
            }
            if delivery.is_finished() {
                return false;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "the delivery neither finished nor waits"
            );
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }
}

impl Deployment {
    /// A tenant connection.
    ///
    /// # Panics
    /// Panics when the connection cannot open.
    async fn conn(&self) -> wyrd_sql::TenantConn<'_> {
        self.server
            .tenant_conn_for(self.server.data_tenant_id())
            .await
            .expect("tenant connection opens")
    }

    /// Claim up to `limit` due executions as a worker would.
    ///
    /// # Panics
    /// Panics when the claim fails.
    async fn claim(
        &self,
        limit: u32,
    ) -> Vec<wyrd_sql::queries::verifier_executions::ClaimedExecution> {
        let mut conn = self.conn().await;
        let claimed = VerifierExecutionQueue::default()
            .claim(&mut conn, Duration::minutes(5), Duration::minutes(5), limit)
            .await
            .expect("claim runs");
        conn.commit().await.expect("claim commits");
        claimed
    }

    /// Wait until no execution of PRs `numbers` is unfinished and return
    /// their rows.
    ///
    /// # Panics
    /// Panics when work is still unfinished after a minute.
    async fn settled(&self, numbers: std::ops::RangeInclusive<u64>) -> Vec<Row> {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_mins(1);
        loop {
            let mut rows = Vec::new();
            for number in numbers.clone() {
                rows.extend(self.rows(number).await);
            }
            if rows
                .iter()
                .all(|row| !matches!(row.status.as_str(), "pending" | "running" | "retrying"))
            {
                return rows;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "unfinished: {rows:?}"
            );
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
}

/// Install the App for tenant `tenant` (slug `slug`) through GitHub's
/// install page as its new administrator `admin`, selecting the mock's
/// install target.
///
/// # Panics
/// Panics when the installation is not connected.
async fn install(
    server: &WyrdTestServer,
    mock: &GithubAppMock,
    tenant: wyrd_spec::ids::DataTenantId,
    slug: &str,
    admin: &str,
) {
    let admin = server
        .bootstrap_service_in_tenant(tenant, admin, &["admin"])
        .await
        .expect("administrator bootstraps");
    let api_key = admin.api_key().expect("a machine principal").clone();
    let mut browser = GithubBrowser::sign_in(
        server.base_url().expect("bound"),
        slug,
        api_key.expose_secret(),
    )
    .await
    .expect("browser signs in");
    let answer = browser
        .open(&format!(
            "{}apps/wyrd-test-app/installations/new",
            mock.origin()
        ))
        .await
        .expect("browser navigates");
    assert_eq!(answer.status, 200, "{:?}", answer.body);
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
/// metadata-only edit admits nothing. A newer head's delivery waits while
/// an earlier provider read is outstanding, so that read cannot make its
/// older source current. Draft conversion, close and access loss cancel unfinished
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
    assert_eq!(
        served["pull_request"],
        json!({
            "host": mock.host(),
            "repository_id": "4242",
            "number": 7,
            "base": {"ref": "main", "sha": "b1"},
            "head": {"ref": "feature", "sha": "a1"},
            "head_repository_id": "4242",
            "delivery_id": opened,
            "generation": served["pull_request"]["generation"],
        }),
        "the execution is pinned to GitHub's current source"
    );
    let text = served.to_string();
    for secret in mock.secrets() {
        assert!(!text.contains(&secret), "a credential is never served");
    }
    assert_eq!(
        deployment.frozen(7).await,
        [
            ("a-review".into(), "1.0.0".into(), "1.0.0".into(), false),
            ("z-review".into(), "1.0.0".into(), "1.0.0".into(), true),
        ],
        "inline and referenced pull_request Triggers each bind one Verifier"
    );
    let mut conn = deployment.conn().await;
    let unaccountable: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wyrd.verifier_executions e
           JOIN wyrd.cards c ON c.card_uid = e.owner_card_uid
           JOIN wyrd.card_declaration_operations o
             ON o.operation_id = c.selection_occurrence
          WHERE e.origin = 'pull_request'
            AND e.sub_principal_id IS DISTINCT FROM o.sub_principal_id",
    )
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("authority reads");
    conn.commit().await.expect("read commits");
    assert_eq!(
        unaccountable, 0,
        "each execution acts as the principal that declared the selected policy"
    );

    let title = json!({
        "action": "edited",
        "number": 7,
        "changes": {"title": {"from": "old"}},
        "repository": {"id": REPOSITORY},
        "installation": {"id": INSTALLATION},
    });
    let (status, other) = mock
        .deliver_new(deployment.base(), "pull_request", &title)
        .await
        .expect("delivery sends");
    assert_eq!(status, StatusCode::ACCEPTED);
    let recover = || async {
        GithubConnections::new(deployment.server.state())
            .recover_deliveries()
            .await
            .expect("the delivery log reads")
    };
    mock.lose_answer(&opened);
    assert_eq!(
        recover().await,
        1,
        "only the delivery without a 2xx answer is redelivered"
    );
    assert_eq!(
        mock.delivery_attempts(&opened).last(),
        Some(&(202, true)),
        "the recovered redelivery is answered"
    );
    assert_eq!(
        recover().await,
        0,
        "an answered delivery is not redelivered"
    );
    for _ in 0..2 {
        mock.lose_answer(&opened);
        assert_eq!(recover().await, 1);
    }
    mock.lose_answer(&opened);
    assert_eq!(
        recover().await,
        0,
        "recovery stops after three redeliveries"
    );
    assert_eq!(
        mock.delivery_attempts(&other),
        [(202, false)],
        "an answered delivery is never redelivered"
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
    let delayed = deployment.spawn_send(7, "synchronize");
    held.arrived().await;
    mock.edit_pull_request(REPOSITORY, 7, |pr| pr.head_sha = "c3".to_owned());
    let newer = deployment.spawn_send(7, "synchronize");
    assert!(
        deployment.waits_on_the_pull_request(&newer).await,
        "the newer delivery waits for the delayed read"
    );
    held.release();
    let (status, _) = delayed.await.expect("joins");
    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "the delayed read of the current source owes no new work"
    );
    assert_eq!(newer.await.expect("joins").0, StatusCode::ACCEPTED);
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

/// Forty-one PRs opened at once each admit exactly one execution per
/// binding, every delivery answered only after its admission committed, and
/// the fair queue runs every one to completion with typed status.
///
/// # Panics
/// Panics when a delivery is refused or any PR's fanout differs.
#[tokio::test(flavor = "multi_thread")]
async fn burst_of_forty_one_pull_requests_admits_each_binding_once() {
    let deployment = Deployment::start_with(true).await;
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
    let rows = deployment.settled(1..=41).await;
    assert_eq!(rows.len(), 82, "one execution per binding per PR: {rows:?}");
    for number in 1..=41 {
        let head = format!("h{number}");
        assert_eq!(
            rows.iter().filter(|row| row.head == head).count(),
            2,
            "PR {number}"
        );
    }
    assert!(
        rows.iter().all(|row| row.status == "completed"),
        "every admitted execution progresses to completion: {rows:?}"
    );
    for row in &rows {
        let served = deployment.status(row.execution_id).await;
        assert_eq!(served["status"], "completed");
        assert_eq!(served["pull_request"]["head"]["sha"], row.head);
    }
}

/// Open PR `number` and interrupt its delivery after the generation was
/// reserved and one binding activated, with the rest of the handoff
/// unacknowledged; restart the server and return it with the reserved rows.
///
/// # Panics
/// Panics when the generation is not reserved first or the interrupted
/// delivery is acknowledged.
async fn interrupted(number: u64) -> (Deployment, Vec<Row>) {
    let Deployment {
        mock,
        server,
        jwt: _,
    } = Deployment::start().await;
    mock.set_pull_request(
        REPOSITORY,
        number,
        PullRequest::open(REPOSITORY, "b1", "a1"),
    );
    server.flush_bifrost().await.expect("setup settles");
    let held = server
        .stall_next_bifrost_write_into("bifrost.verification.execution_inputs")
        .expect("Scribe stalls");
    let payload = mock.pull_request_event(INSTALLATION, REPOSITORY, number, "opened");
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
    let reserved = deployment.rows(number).await;
    assert_eq!(
        reserved.len(),
        2,
        "the generation is frozen before the handoff"
    );
    let mut conn = deployment.conn().await;
    let partial = VerifierExecutionQueue::default()
        .activate_pull_requests(
            &mut conn,
            &[wyrd_spec::ids::ExecutionId::new(reserved[0].execution_id).expect("a v7 id")],
        )
        .await
        .expect("activation runs");
    conn.commit().await.expect("activation commits");
    assert_eq!(partial.len(), 1, "one binding of the fanout is already due");
    let restarted = Box::pin(
        deployment
            .server
            .restart_bound(WyrdTestServer::builder().with_github_app_for_test(&mock)),
    )
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
    (deployment, reserved)
}

/// A delivery interrupted after its generation was reserved, with one
/// binding already due and the rest of its handoff unacknowledged, is
/// answered with no 2xx; after a restart, failed-delivery recovery
/// redelivers it and completes the same frozen fanout under the same
/// execution IDs, never a second generation or a second run of the due
/// binding.
///
/// # Panics
/// Panics when the interrupted delivery is acknowledged, or recovery admits
/// other or duplicate executions.
#[tokio::test(flavor = "multi_thread")]
async fn interrupted_handoff_completes_the_same_fanout_after_restart() {
    let (deployment, reserved) = interrupted(9).await;
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

/// An interrupted delivery whose PR moved to a new head before recovery
/// admits the current source under the selected policy and cancels the
/// interrupted generation, the binding already due included.
///
/// # Panics
/// Panics when recovery completes the stale generation.
#[tokio::test(flavor = "multi_thread")]
async fn recovery_after_a_new_head_admits_the_current_source() {
    let (deployment, reserved) = interrupted(10).await;
    deployment
        .mock
        .edit_pull_request(REPOSITORY, 10, |pr| pr.head_sha = "a2".to_owned());
    assert_eq!(
        GithubConnections::new(deployment.server.state())
            .recover_deliveries()
            .await
            .expect("the delivery log reads"),
        1
    );
    let rows = deployment.rows(10).await;
    assert_eq!(
        rows[..2]
            .iter()
            .map(|row| row.execution_id)
            .collect::<Vec<_>>(),
        reserved
            .iter()
            .map(|row| row.execution_id)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        summary(&rows),
        [
            ("cancelled", Some("pull_request_superseded"), "a1"),
            ("cancelled", Some("pull_request_superseded"), "a1"),
            ("pending", None, "a2"),
            ("pending", None, "a2"),
        ]
    );
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
        rows.iter().all(|row| row.status == "errored"
            && row.code.as_deref() == Some("WYRD_AUTH_401_CREDENTIAL_REVOKED")),
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

/// P completed, A running, B pushed: B cancels A and fences its worker, and
/// P's completed results stay as B's usable baseline.
///
/// # Panics
/// Panics when P changes, A survives or its worker can still settle.
#[tokio::test(flavor = "multi_thread")]
async fn completed_baseline_survives_when_a_new_head_supersedes_running_work() {
    let deployment = Deployment::start().await;
    let mock = &deployment.mock;
    mock.set_pull_request(REPOSITORY, 8, PullRequest::open(REPOSITORY, "b1", "p0"));
    assert_eq!(deployment.send(8, "opened").await.0, StatusCode::ACCEPTED);
    let queue = VerifierExecutionQueue::default();
    let mut conn = deployment.conn().await;
    for claimed in deployment.claim(2).await {
        let settled = queue
            .complete(
                &mut conn,
                claimed.lease,
                VerificationResultId::new_v7(),
                VerificationVerdict::Passed,
                "P passes",
                None,
            )
            .await
            .expect("P completes");
        assert_eq!(settled, Settlement::Applied);
    }
    conn.commit().await.expect("P commits");

    mock.edit_pull_request(REPOSITORY, 8, |pr| pr.head_sha = "a1".to_owned());
    assert_eq!(
        deployment.send(8, "synchronize").await.0,
        StatusCode::ACCEPTED
    );
    let running = deployment.claim(2).await;
    assert_eq!(running.len(), 2, "A runs");
    mock.edit_pull_request(REPOSITORY, 8, |pr| pr.head_sha = "b2".to_owned());
    assert_eq!(
        deployment.send(8, "synchronize").await.0,
        StatusCode::ACCEPTED
    );

    assert_eq!(
        summary(&deployment.rows(8).await),
        [
            ("completed", None, "p0"),
            ("completed", None, "p0"),
            ("cancelled", Some("pull_request_superseded"), "a1"),
            ("cancelled", Some("pull_request_superseded"), "a1"),
            ("pending", None, "b2"),
            ("pending", None, "b2"),
        ]
    );
    let mut conn = deployment.conn().await;
    for claimed in running {
        let late = queue
            .complete(
                &mut conn,
                claimed.lease,
                VerificationResultId::new_v7(),
                VerificationVerdict::Passed,
                "A finished late",
                None,
            )
            .await
            .expect("settlement runs");
        assert_eq!(late, Settlement::StaleLease, "A's worker is fenced");
    }
    conn.commit().await.expect("commits");
}

/// A new head supersedes unfinished work even after the Repository was
/// re-declared binding no Verifier: the re-declaration itself changes
/// nothing, the new head cancels and fences queued and running work,
/// keeps the completed baseline and admits nothing, and the cancelled
/// generation is never replayed as current once Verifiers are bound again.
///
/// # Panics
/// Panics when the empty policy sweeps, leaves older work runnable, admits
/// work, or replays the cancelled generation.
#[tokio::test(flavor = "multi_thread")]
async fn a_new_head_supersedes_work_when_the_policy_binds_nothing() {
    let deployment = Deployment::start().await;
    let mock = &deployment.mock;
    mock.set_pull_request(REPOSITORY, 29, PullRequest::open(REPOSITORY, "b1", "p0"));
    assert_eq!(deployment.send(29, "opened").await.0, StatusCode::ACCEPTED);
    let queue = VerifierExecutionQueue::default();
    let mut conn = deployment.conn().await;
    for claimed in deployment.claim(2).await {
        let settled = queue
            .complete(
                &mut conn,
                claimed.lease,
                VerificationResultId::new_v7(),
                VerificationVerdict::Passed,
                "P passes",
                None,
            )
            .await
            .expect("P completes");
        assert_eq!(settled, Settlement::Applied);
    }
    conn.commit().await.expect("P commits");
    mock.edit_pull_request(REPOSITORY, 29, |pr| pr.head_sha = "a1".to_owned());
    assert_eq!(
        deployment.send(29, "synchronize").await.0,
        StatusCode::ACCEPTED
    );
    let running = deployment.claim(1).await.pop().expect("one of A runs");

    deployment
        .declare(POLICY_EMPTY, "pull-request-policy-empty")
        .await;
    let before = deployment.rows(29).await;
    assert_eq!(
        summary(&before),
        [
            ("completed", None, "p0"),
            ("completed", None, "p0"),
            ("running", None, "a1"),
            ("pending", None, "a1"),
        ],
        "selecting the empty policy sweeps nothing"
    );

    mock.edit_pull_request(REPOSITORY, 29, |pr| pr.head_sha = "b2".to_owned());
    assert_eq!(
        deployment.send(29, "synchronize").await.0,
        StatusCode::ACCEPTED
    );
    assert_eq!(
        summary(&deployment.rows(29).await),
        [
            ("completed", None, "p0"),
            ("completed", None, "p0"),
            ("cancelled", Some("pull_request_superseded"), "a1"),
            ("cancelled", Some("pull_request_superseded"), "a1"),
        ],
        "the new head cancels A, keeps P and admits nothing"
    );
    let mut conn = deployment.conn().await;
    let late = queue
        .complete(
            &mut conn,
            running.lease,
            VerificationResultId::new_v7(),
            VerificationVerdict::Passed,
            "A finished late",
            None,
        )
        .await
        .expect("settlement runs");
    conn.commit().await.expect("commits");
    assert_eq!(late, Settlement::StaleLease, "A's worker is fenced");

    deployment
        .declare(POLICY_RESTORED, "pull-request-policy-restored")
        .await;
    mock.edit_pull_request(REPOSITORY, 29, |pr| pr.head_sha = "a1".to_owned());
    assert_eq!(
        deployment.send(29, "synchronize").await.0,
        StatusCode::ACCEPTED
    );
    let rows = deployment.rows(29).await;
    assert_eq!(
        summary(&rows[4..]),
        [("pending", None, "a1"), ("pending", None, "a1")],
        "returning to A admits a new generation instead of replaying the cancelled one"
    );
    assert!(
        rows[4..].iter().all(|row| before
            .iter()
            .all(|old| old.execution_id != row.execution_id)),
        "the new generation has new executions"
    );
}

/// Re-declaring the Repository changes only later admissions: an open PR's
/// accepted executions stay, a redelivery of its delivery keeps the first
/// policy and IDs, and only a new source generation uses the new policy.
///
/// # Panics
/// Panics when the re-declaration sweeps or resamples accepted work.
#[tokio::test(flavor = "multi_thread")]
async fn policy_reselection_affects_only_later_admissions() {
    let deployment = Deployment::start().await;
    let mock = &deployment.mock;
    mock.set_pull_request(REPOSITORY, 5, PullRequest::open(REPOSITORY, "b1", "a1"));
    let (_, opened) = deployment.send(5, "opened").await;
    let first = deployment.rows(5).await;
    assert_eq!(first.len(), 2);

    deployment
        .declare(POLICY_V2, "pull-request-policy-v2")
        .await;
    assert_eq!(
        deployment.rows(5).await,
        first,
        "selection sweeps no open PR"
    );
    assert_eq!(
        mock.redeliver(&opened).await.expect("redelivers"),
        StatusCode::ACCEPTED
    );
    assert_eq!(
        deployment.rows(5).await,
        first,
        "a redelivery keeps the first policy and IDs"
    );

    mock.edit_pull_request(REPOSITORY, 5, |pr| pr.head_sha = "b2".to_owned());
    assert_eq!(
        deployment.send(5, "synchronize").await.0,
        StatusCode::ACCEPTED
    );
    let rows = deployment.rows(5).await;
    assert_eq!(
        summary(&rows[2..]),
        [("pending", None, "b2")],
        "the new generation uses the new policy's one binding"
    );
    assert_eq!(
        deployment.frozen(5).await,
        [
            ("a-review".into(), "1.0.0".into(), "1.0.0".into(), false),
            ("z-review".into(), "1.0.0".into(), "1.0.0".into(), true),
            ("a-review".into(), "1.0.0".into(), "1.0.1".into(), false),
        ],
        "each generation freezes the policy selected when it was admitted"
    );

    mock.edit_pull_request(REPOSITORY, 5, |pr| pr.base_ref = "release".to_owned());
    let retargeted = json!({
        "action": "edited",
        "number": 5,
        "changes": {"base": {"ref": {"from": "main"}}},
        "repository": {"id": REPOSITORY},
        "installation": {"id": INSTALLATION},
    });
    let (status, _) = mock
        .deliver_new(deployment.base(), "pull_request", &retargeted)
        .await
        .expect("delivery sends");
    assert_eq!(status, StatusCode::ACCEPTED);
    let rows = deployment.rows(5).await;
    assert_eq!(
        summary(&rows[2..]),
        [
            ("cancelled", Some("pull_request_superseded"), "b2"),
            ("pending", None, "b2"),
        ],
        "a base branch change at the same head is a new generation"
    );
    let served = deployment.status(rows[3].execution_id).await;
    assert_eq!(served["pull_request"]["base"]["ref"], "release");
}

/// Installation suspension and disconnect cancel unfinished PR work with
/// `repository_access_lost`; unsigned and mis-signed deliveries, draft PRs
/// and unselected repositories admit nothing.
///
/// # Panics
/// Panics when ineligible work is admitted or access loss leaves it due.
#[tokio::test(flavor = "multi_thread")]
async fn suspension_disconnect_and_ineligible_pull_requests_owe_no_work() {
    let deployment = Deployment::start().await;
    let mock = &deployment.mock;
    let mut draft = PullRequest::open(REPOSITORY, "b1", "d1");
    draft.draft = true;
    mock.set_pull_request(REPOSITORY, 12, draft);
    assert_eq!(deployment.send(12, "opened").await.0, StatusCode::ACCEPTED);
    assert!(
        deployment.rows(12).await.is_empty(),
        "a draft admits nothing"
    );
    mock.set_pull_request(UNSELECTED, 13, PullRequest::open(UNSELECTED, "b1", "u1"));
    let payload = mock.pull_request_event(INSTALLATION, UNSELECTED, 13, "opened");
    let (status, _) = mock
        .deliver_new(deployment.base(), "pull_request", &payload)
        .await
        .expect("delivery sends");
    assert_eq!(status, StatusCode::ACCEPTED);
    assert!(
        deployment.rows(13).await.is_empty(),
        "an unselected repository admits nothing"
    );

    mock.add_installation(999, &[(5151, "elsewhere/repo")]);
    mock.set_pull_request(5151, 15, PullRequest::open(5151, "b1", "e1"));
    let payload = mock.pull_request_event(999, 5151, 15, "opened");
    let (status, _) = mock
        .deliver_new(deployment.base(), "pull_request", &payload)
        .await
        .expect("delivery sends");
    assert_eq!(status, StatusCode::ACCEPTED);
    assert!(
        deployment.rows(15).await.is_empty(),
        "an installation no tenant connected admits nothing"
    );

    mock.set_pull_request(REPOSITORY, 14, PullRequest::open(REPOSITORY, "b1", "s1"));
    let payload = mock.pull_request_event(INSTALLATION, REPOSITORY, 14, "opened");
    for signature in [None, Some("sha256=00")] {
        let status = mock
            .deliver_with_signature(deployment.base(), "pull_request", &payload, signature)
            .await
            .expect("delivery sends");
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{signature:?}");
    }
    assert!(
        deployment.rows(14).await.is_empty(),
        "an unsigned or mis-signed delivery admits nothing"
    );

    mock.set_pull_request(REPOSITORY, 11, PullRequest::open(REPOSITORY, "b1", "a1"));
    assert_eq!(deployment.send(11, "opened").await.0, StatusCode::ACCEPTED);
    let event = |action: &str| json!({"action": action, "installation": {"id": INSTALLATION}});
    mock.set_suspended(INSTALLATION, true);
    let status = mock
        .deliver(deployment.base(), "installation", &event("suspend"))
        .await
        .expect("delivery sends");
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(
        summary(&deployment.rows(11).await),
        [
            ("cancelled", Some("repository_access_lost"), "a1"),
            ("cancelled", Some("repository_access_lost"), "a1"),
        ]
    );
    mock.set_suspended(INSTALLATION, false);
    mock.deliver(deployment.base(), "installation", &event("unsuspend"))
        .await
        .expect("delivery sends");
    mock.edit_pull_request(REPOSITORY, 11, |pr| pr.head_sha = "b2".to_owned());
    assert_eq!(
        deployment.send(11, "synchronize").await.0,
        StatusCode::ACCEPTED
    );
    assert_eq!(deployment.rows(11).await.len(), 4, "unsuspend admits again");

    let Bootstrap::User { jwt, .. } = deployment
        .server
        .bootstrap_user("pull-request-admin", &["admin"])
        .await
        .expect("admin bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };
    let disconnected = deployment
        .server
        .oneshot_authenticated(
            &jwt,
            Request::delete("/v1/integrations/github")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("disconnect responds");
    assert_eq!(disconnected.status(), StatusCode::OK);
    assert_eq!(
        summary(&deployment.rows(11).await[2..]),
        [
            ("cancelled", Some("repository_access_lost"), "b2"),
            ("cancelled", Some("repository_access_lost"), "b2"),
        ]
    );
}

/// Uninstalling the App cancels the connected tenant's unfinished PR work
/// with `repository_access_lost`.
///
/// # Panics
/// Panics when the uninstall leaves PR work due.
#[tokio::test(flavor = "multi_thread")]
async fn uninstalling_the_app_cancels_unfinished_pull_request_work() {
    let deployment = Deployment::start().await;
    deployment
        .mock
        .set_pull_request(REPOSITORY, 16, PullRequest::open(REPOSITORY, "b1", "a1"));
    assert_eq!(deployment.send(16, "opened").await.0, StatusCode::ACCEPTED);
    let deleted = json!({"action": "deleted", "installation": {"id": INSTALLATION}});
    let status = deployment
        .mock
        .deliver(deployment.base(), "installation", &deleted)
        .await
        .expect("delivery sends");
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(
        summary(&deployment.rows(16).await),
        [
            ("cancelled", Some("repository_access_lost"), "a1"),
            ("cancelled", Some("repository_access_lost"), "a1"),
        ]
    );
}

/// Close fences a running execution as well as queued ones; the close waits
/// for an observation taken before it, which cannot admit afterwards; a failed GitHub read,
/// whether GitHub is unavailable or rate-limits the read with a 403, is not
/// acknowledged and its redelivery admits exactly once.
///
/// # Panics
/// Panics when deactivated work can still settle, an outdated observation
/// admits, or a failed read is acknowledged or lost.
#[tokio::test(flavor = "multi_thread")]
async fn close_fences_running_work_and_failed_reads_stay_retryable() {
    let deployment = Deployment::start().await;
    let mock = &deployment.mock;
    mock.set_pull_request(REPOSITORY, 17, PullRequest::open(REPOSITORY, "b1", "a1"));
    assert_eq!(deployment.send(17, "opened").await.0, StatusCode::ACCEPTED);
    let running = deployment.claim(1).await.pop().expect("one runs");

    let held = mock.hold_next_pull_request_read();
    let delayed = deployment.spawn_send(17, "synchronize");
    held.arrived().await;
    mock.edit_pull_request(REPOSITORY, 17, |pr| pr.open = false);
    let close = deployment.spawn_send(17, "closed");
    assert!(
        deployment.waits_on_the_pull_request(&close).await,
        "the close waits for the delayed read"
    );
    held.release();
    assert_eq!(delayed.await.expect("joins").0, StatusCode::ACCEPTED);
    assert_eq!(close.await.expect("joins").0, StatusCode::ACCEPTED);
    assert_eq!(
        summary(&deployment.rows(17).await),
        [
            ("cancelled", Some("pull_request_closed"), "a1"),
            ("cancelled", Some("pull_request_closed"), "a1"),
        ],
        "the running and the queued execution end, and the open state read \
         before the close admits nothing"
    );
    let mut conn = deployment.conn().await;
    let late = VerifierExecutionQueue::default()
        .complete(
            &mut conn,
            running.lease,
            VerificationResultId::new_v7(),
            VerificationVerdict::Passed,
            "finished after the close",
            None,
        )
        .await
        .expect("settlement runs");
    conn.commit().await.expect("commits");
    assert_eq!(late, Settlement::StaleLease, "the running worker is fenced");

    for (number, failure) in [(18, StatusCode::BAD_GATEWAY), (26, StatusCode::FORBIDDEN)] {
        mock.set_pull_request(
            REPOSITORY,
            number,
            PullRequest::open(REPOSITORY, "b1", "f1"),
        );
        mock.fail_next_pull_request_read(failure);
        let (status, failed) = deployment.send(number, "opened").await;
        assert!(
            !status.is_success(),
            "a source GitHub answered {failure} for is not acknowledged: {status}"
        );
        assert!(deployment.rows(number).await.is_empty());
        assert_eq!(
            mock.redeliver(&failed).await.expect("redelivers"),
            StatusCode::ACCEPTED
        );
        assert_eq!(
            summary(&deployment.rows(number).await),
            [("pending", None, "f1"), ("pending", None, "f1")],
            "the redelivery after {failure} admits once GitHub answers"
        );
    }
}

/// A delivery admits only into the tenant connected to its installation:
/// another tenant's installation reaching the same repository owes the
/// first tenant nothing, and the first tenant's delivery writes nothing in
/// the other.
///
/// # Panics
/// Panics when a delivery admits work into a tenant not connected to it.
#[tokio::test(flavor = "multi_thread")]
async fn deliveries_admit_only_into_the_connected_tenant() {
    let deployment = Deployment::start().await;
    let mock = &deployment.mock;
    let other = deployment
        .server
        .seed_tenant("other-tenant")
        .await
        .expect("second tenant seeds");
    mock.add_installation(202, &[(REPOSITORY, "acme/policy")]);
    mock.add_user("octo", &[INSTALLATION, 202]);
    mock.install_target(202);
    install(
        &deployment.server,
        mock,
        other,
        "other-tenant",
        "github_admin",
    )
    .await;
    mock.set_pull_request(REPOSITORY, 19, PullRequest::open(REPOSITORY, "b1", "t1"));

    let payload = mock.pull_request_event(202, REPOSITORY, 19, "opened");
    let (status, _) = mock
        .deliver_new(deployment.base(), "pull_request", &payload)
        .await
        .expect("delivery sends");
    assert_eq!(status, StatusCode::ACCEPTED);
    assert!(
        deployment.rows(19).await.is_empty(),
        "the other tenant's installation admits nothing here"
    );

    assert_eq!(deployment.send(19, "opened").await.0, StatusCode::ACCEPTED);
    assert_eq!(deployment.rows(19).await.len(), 2);
    let mut conn = deployment
        .server
        .tenant_conn_for(other)
        .await
        .expect("tenant connection opens");
    let foreign: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wyrd.verifier_executions WHERE origin = 'pull_request'",
    )
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("count reads");
    conn.commit().await.expect("read commits");
    assert_eq!(foreign, 0, "nothing is admitted into the other tenant");
}

/// Failed-delivery recovery stops at shutdown even while GitHub has not
/// answered its delivery log read, and asks for no redelivery afterwards.
///
/// The worker must join well inside GitHub's ten-second request deadline,
/// which would otherwise end the held read and let a stalled scan look
/// like a prompt stop.
///
/// # Panics
/// Panics when the worker outlives its shutdown or redelivers after it.
#[tokio::test(flavor = "multi_thread")]
async fn delivery_recovery_yields_to_shutdown_mid_scan() {
    let deployment = Deployment::start().await;
    let mock = &deployment.mock;
    mock.set_pull_request(REPOSITORY, 27, PullRequest::open(REPOSITORY, "b1", "s1"));
    let (status, failed) = deployment.send(27, "opened").await;
    assert_eq!(status, StatusCode::ACCEPTED);
    mock.lose_answer(&failed);

    let shutdown = tokio_util::sync::CancellationToken::new();
    let held = mock.hold_next_delivery_log_read();
    let worker =
        GithubConnections::new(deployment.server.state()).run_delivery_recovery(shutdown.clone());
    let stop = async {
        held.arrived().await;
        shutdown.cancel();
    };
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        tokio::join!(worker, stop)
    })
    .await
    .expect("the worker stops while GitHub still holds its read");
    held.release();
    assert_eq!(
        mock.delivery_attempts(&failed),
        [(0, false)],
        "no redelivery is asked for after shutdown"
    );
}

/// A close of a PR with no work yet is never undone by an open read begun
/// before it: the close waits while that read is outstanding, then cancels
/// what it admitted.
///
/// # Panics
/// Panics when the earlier read leaves work due or the close does not wait.
#[tokio::test(flavor = "multi_thread")]
async fn a_close_waits_for_an_earlier_open_read() {
    let deployment = Deployment::start().await;
    let mock = &deployment.mock;
    mock.set_pull_request(REPOSITORY, 20, PullRequest::open(REPOSITORY, "b1", "o1"));
    let held = mock.hold_next_pull_request_read();
    let open = deployment.spawn_send(20, "opened");
    held.arrived().await;
    mock.edit_pull_request(REPOSITORY, 20, |pr| pr.open = false);
    let close = deployment.spawn_send(20, "closed");
    let waited = deployment.waits_on_the_pull_request(&close).await;
    held.release();
    assert_eq!(open.await.expect("joins").0, StatusCode::ACCEPTED);
    assert_eq!(close.await.expect("joins").0, StatusCode::ACCEPTED);
    assert_eq!(
        summary(&deployment.rows(20).await),
        [
            ("cancelled", Some("pull_request_closed"), "o1"),
            ("cancelled", Some("pull_request_closed"), "o1"),
        ],
        "the open read before the close cannot admit after it"
    );
    assert!(waited, "the close waits for the outstanding read");
}

/// A replay of the current source is never undone by a read of an
/// intermediate source begun before it: the source GitHub shows last stays
/// under review.
///
/// # Panics
/// Panics when the intermediate source ends up under review or the replay
/// does not wait.
#[tokio::test(flavor = "multi_thread")]
async fn a_replay_waits_for_an_earlier_intermediate_read() {
    let deployment = Deployment::start().await;
    let mock = &deployment.mock;
    mock.set_pull_request(REPOSITORY, 21, PullRequest::open(REPOSITORY, "b1", "a1"));
    assert_eq!(deployment.send(21, "opened").await.0, StatusCode::ACCEPTED);
    mock.edit_pull_request(REPOSITORY, 21, |pr| pr.head_sha = "b1".to_owned());
    let held = mock.hold_next_pull_request_read();
    let intermediate = deployment.spawn_send(21, "synchronize");
    held.arrived().await;
    mock.edit_pull_request(REPOSITORY, 21, |pr| pr.head_sha = "a1".to_owned());
    let replay = deployment.spawn_send(21, "synchronize");
    let waited = deployment.waits_on_the_pull_request(&replay).await;
    held.release();
    assert_eq!(intermediate.await.expect("joins").0, StatusCode::ACCEPTED);
    assert_eq!(replay.await.expect("joins").0, StatusCode::ACCEPTED);
    let rows = deployment.rows(21).await;
    let due: Vec<_> = summary(&rows)
        .into_iter()
        .filter(|(status, _, _)| *status == "pending")
        .collect();
    assert_eq!(
        due,
        [("pending", None, "a1"), ("pending", None, "a1")],
        "the source GitHub shows last is the one under review"
    );
    assert!(waited, "the replay waits for the outstanding read");
}

/// A repeated draft conversion is never undone by a ready read begun before
/// it, although the draft finds nothing left to cancel when it is decided.
///
/// # Panics
/// Panics when the ready read leaves work due or the draft does not wait.
#[tokio::test(flavor = "multi_thread")]
async fn a_repeated_draft_waits_for_an_earlier_ready_read() {
    let deployment = Deployment::start().await;
    let mock = &deployment.mock;
    mock.set_pull_request(REPOSITORY, 22, PullRequest::open(REPOSITORY, "b1", "a1"));
    assert_eq!(deployment.send(22, "opened").await.0, StatusCode::ACCEPTED);
    mock.edit_pull_request(REPOSITORY, 22, |pr| pr.draft = true);
    assert_eq!(
        deployment.send(22, "converted_to_draft").await.0,
        StatusCode::ACCEPTED
    );
    mock.edit_pull_request(REPOSITORY, 22, |pr| {
        pr.draft = false;
        pr.head_sha = "c1".to_owned();
    });
    let held = mock.hold_next_pull_request_read();
    let ready = deployment.spawn_send(22, "ready_for_review");
    held.arrived().await;
    mock.edit_pull_request(REPOSITORY, 22, |pr| pr.draft = true);
    let draft = deployment.spawn_send(22, "converted_to_draft");
    let waited = deployment.waits_on_the_pull_request(&draft).await;
    held.release();
    assert_eq!(ready.await.expect("joins").0, StatusCode::ACCEPTED);
    assert_eq!(draft.await.expect("joins").0, StatusCode::ACCEPTED);
    assert_eq!(
        summary(&deployment.rows(22).await),
        [
            ("cancelled", Some("pull_request_draft"), "a1"),
            ("cancelled", Some("pull_request_draft"), "a1"),
            ("cancelled", Some("pull_request_draft"), "c1"),
            ("cancelled", Some("pull_request_draft"), "c1"),
        ],
        "the ready read before the second draft leaves nothing due"
    );
    assert!(waited, "the second draft waits for the outstanding read");
}

/// Hold GitHub's answer to the open read of PR `number` at `head` and
/// deliver it on its own task.
///
/// # Panics
/// Panics as [`Deployment::spawn_send`] does.
fn read_held(
    deployment: &Deployment,
    number: u64,
    head: &str,
) -> (
    wyrd_testing::github::HeldAnswer,
    tokio::task::JoinHandle<(StatusCode, String)>,
) {
    let mock = &deployment.mock;
    mock.set_pull_request(
        REPOSITORY,
        number,
        PullRequest::open(REPOSITORY, "b1", head),
    );
    let held = mock.hold_next_pull_request_read();
    (held, deployment.spawn_send(number, "opened"))
}

/// `action` on installation 101 as GitHub delivers it.
fn installation_event(action: &str) -> Value {
    json!({"action": action, "installation": {"id": INSTALLATION}})
}

/// A suspension committed while GitHub answers a PR read leaves the delayed
/// delivery nothing to admit.
///
/// # Panics
/// Panics when the delayed read admits work.
#[tokio::test(flavor = "multi_thread")]
async fn a_suspension_during_a_pull_request_read_admits_nothing() {
    let deployment = Deployment::start().await;
    let mock = &deployment.mock;
    let (held, delayed) = read_held(&deployment, 23, "s1");
    held.arrived().await;
    mock.set_suspended(INSTALLATION, true);
    let status = mock
        .deliver(
            deployment.base(),
            "installation",
            &installation_event("suspend"),
        )
        .await
        .expect("delivery sends");
    assert_eq!(status, StatusCode::ACCEPTED);
    held.release();
    delayed.await.expect("joins");
    assert!(
        deployment.rows(23).await.is_empty(),
        "a read begun before the suspension admits nothing"
    );
}

/// Disconnecting the GitHub integration in Wyrd while GitHub answers a PR
/// read leaves the delayed delivery nothing to admit.
///
/// # Panics
/// Panics when the delayed read admits work.
#[tokio::test(flavor = "multi_thread")]
async fn a_disconnect_during_a_pull_request_read_admits_nothing() {
    let deployment = Deployment::start().await;
    let (held, delayed) = read_held(&deployment, 28, "d1");
    held.arrived().await;
    let Bootstrap::User { jwt, .. } = deployment
        .server
        .bootstrap_user("pull-request-admin", &["admin"])
        .await
        .expect("admin bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };
    let disconnected = deployment
        .server
        .oneshot_authenticated(
            &jwt,
            Request::delete("/v1/integrations/github")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("disconnect responds");
    assert_eq!(disconnected.status(), StatusCode::OK);
    held.release();
    delayed.await.expect("joins");
    assert!(
        deployment.rows(28).await.is_empty(),
        "a read begun before the disconnect admits nothing"
    );
}

/// Uninstalling and reconnecting the App while GitHub answers a PR read
/// leaves the delayed delivery nothing to admit under the new connection,
/// while a delivery after the reconnect admits as usual.
///
/// # Panics
/// Panics when the delayed read admits work, or a later delivery does not.
#[tokio::test(flavor = "multi_thread")]
async fn a_reconnect_during_a_pull_request_read_admits_nothing_for_the_old_connection() {
    let deployment = Deployment::start().await;
    let mock = &deployment.mock;
    let (held, delayed) = read_held(&deployment, 24, "u1");
    held.arrived().await;
    let status = mock
        .deliver(
            deployment.base(),
            "installation",
            &installation_event("deleted"),
        )
        .await
        .expect("delivery sends");
    assert_eq!(status, StatusCode::ACCEPTED);
    install(
        &deployment.server,
        mock,
        deployment.server.data_tenant_id(),
        deployment.server.pg_fixture().tenant_slug(),
        "github_reconnecting_admin",
    )
    .await;
    held.release();
    delayed.await.expect("joins");
    assert!(
        deployment.rows(24).await.is_empty(),
        "a read begun under the uninstalled connection admits nothing under the new one"
    );
    assert_eq!(deployment.send(24, "opened").await.0, StatusCode::ACCEPTED);
    assert_eq!(
        summary(&deployment.rows(24).await),
        [("pending", None, "u1"), ("pending", None, "u1")],
        "a delivery after the reconnect admits"
    );
}

/// Removing the repository from the installation while GitHub answers a PR
/// read leaves the delayed delivery nothing to admit.
///
/// # Panics
/// Panics when the delayed read admits work.
#[tokio::test(flavor = "multi_thread")]
async fn a_repository_removal_during_a_pull_request_read_admits_nothing() {
    let deployment = Deployment::start().await;
    let mock = &deployment.mock;
    let (held, delayed) = read_held(&deployment, 25, "r1");
    held.arrived().await;
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
    held.release();
    delayed.await.expect("joins");
    assert!(
        deployment.rows(25).await.is_empty(),
        "a read begun before the repository was removed admits nothing"
    );
}

/// The status code of `GET uri` as the caller holding `jwt`.
///
/// # Panics
/// Panics when the request cannot be served.
async fn read_as(deployment: &Deployment, jwt: &str, uri: &str) -> (StatusCode, Value) {
    let request = Request::builder()
        .uri(uri)
        .body(Body::empty())
        .expect("request builds");
    let response = deployment
        .server
        .oneshot_authenticated(jwt, request)
        .await
        .expect("read responds");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("body reads");
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// Inspecting a PR execution, and its sibling progress read, needs
/// repository read as well as `verifier:read`, also once the queue row is
/// gone and only the accepted state remains.
///
/// # Panics
/// Panics when a caller without `cards:read` reads a PR execution, or one
/// holding both is refused.
#[tokio::test(flavor = "multi_thread")]
async fn pull_request_inspection_requires_repository_read() {
    use wyrd_runtime::Permission;

    let deployment = Deployment::start_with(true).await;
    let server = &deployment.server;
    deployment
        .mock
        .set_pull_request(REPOSITORY, 30, PullRequest::open(REPOSITORY, "b1", "i1"));
    assert_eq!(deployment.send(30, "opened").await.0, StatusCode::ACCEPTED);
    let execution = deployment.settled(30..=30).await[0].execution_id;
    server
        .seed_role("pr_verifier_only", &[Permission::verifier_read_any()])
        .await
        .expect("role seeds");
    server
        .seed_role(
            "pr_verifier_and_repository",
            &[Permission::verifier_read_any(), Permission::card_read()],
        )
        .await
        .expect("role seeds");
    let jwt = |name: &'static str, role: &'static str| async move {
        let Bootstrap::User { jwt, .. } = server
            .bootstrap_user(name, &[role])
            .await
            .expect("reader bootstraps")
        else {
            panic!("user bootstrap returned a non-user principal");
        };
        jwt
    };
    let verifier_only = jwt("pr-verifier-only", "pr_verifier_only").await;
    let both = jwt("pr-verifier-and-repository", "pr_verifier_and_repository").await;
    let status = format!("/v1/verification/executions/{execution}");
    let invocations = format!("{status}/invocations");

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_mins(1);
    loop {
        let (code, body) = read_as(&deployment, &both, &status).await;
        assert_eq!(code, StatusCode::OK, "{body}");
        if !body["result"].is_null() {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the result never became visible"
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    for uri in [&status, &invocations] {
        let (code, body) = read_as(&deployment, &verifier_only, uri).await;
        assert_eq!(code, StatusCode::FORBIDDEN, "{uri}: {body}");
        assert_eq!(body["code"], "WYRD_PERMISSION_403_DENIED_RBAC");
        assert_eq!(
            read_as(&deployment, &both, uri).await.0,
            StatusCode::OK,
            "{uri}"
        );
    }

    let mut conn = deployment.conn().await;
    let deleted = sqlx::query("DELETE FROM wyrd.verifier_executions WHERE execution_id = $1")
        .bind(execution)
        .execute(&mut **conn.transaction())
        .await
        .expect("the queue row deletes")
        .rows_affected();
    conn.commit().await.expect("delete commits");
    assert_eq!(deleted, 1);
    let (code, body) = read_as(&deployment, &verifier_only, &status).await;
    assert_eq!(
        code,
        StatusCode::FORBIDDEN,
        "retained PR state still needs repository read: {body}"
    );
    let (code, body) = read_as(&deployment, &both, &status).await;
    assert_eq!(code, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "completed");
}
