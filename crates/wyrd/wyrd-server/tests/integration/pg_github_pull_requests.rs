//! Pull-request deliveries admitted into the Verifier queue through the real
//! webhook route, against the GitHub App mock and repository Postgres.
//!
//! The tenant connects to installation 101 through GitHub's install page
//! with `acme/policy` (4242) selected and declares the `policy` Repository binding two queued review
//! Verifiers to its pull requests. The default test server holds the
//! Verifier runner, so admitted work stays queued until a test claims it.

use arrow::array::{Array as _, StringArray};
use arrow::datatypes::DataType;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use chrono::Duration;
use secrecy::ExposeSecret;
use serde_json::{Value, json};
use uuid::Uuid;
use wyrd_client::Bifrost;
use wyrd_loader::{build_declaration_input, load};
use wyrd_server::components::github::service::GithubConnections;
use wyrd_server::verification::RuntimeLimits;
use wyrd_spec::error::WyrdError;
use wyrd_spec::github_connection::{
    GithubPullRequestReviewRequest, GithubPullRequestReviewResponse,
};
use wyrd_spec::ids::{RepositoryId, VerificationResultId};
use wyrd_spec::vala::api::BifrostQueryRequest;
use wyrd_spec::verification::VerificationVerdict;
use wyrd_sql::queries::verifier_executions::{
    Completion, Settlement, TerminalStatus, VerifierExecutionQueue,
};
use wyrd_testing::github::{GithubAppMock, GithubBrowser, PullRequest};
use wyrd_testing::{Bootstrap, WyrdTestServer, WyrdTestServerBuilder};

use super::pg_hosted_artifacts::client;

/// The connected repository.
pub(crate) const REPOSITORY: u64 = 4242;
/// A repository the installation reaches that the tenant did not select.
pub(crate) const UNSELECTED: u64 = 4343;
/// The connected installation.
pub(crate) const INSTALLATION: u64 = 101;
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
pub(crate) struct Row {
    /// The execution.
    pub(crate) execution_id: Uuid,
    /// Its status.
    pub(crate) status: String,
    /// Its settled error code.
    pub(crate) code: Option<String>,
    /// The frozen head commit.
    pub(crate) head: String,
    /// Whether it is reserved, not yet due.
    pub(crate) reserved: bool,
}

/// A connected, declared deployment: the mock, the server, and the
/// declaring editor's id and JWT.
pub(crate) struct Deployment {
    /// GitHub.
    pub(crate) mock: GithubAppMock,
    /// Wyrd.
    pub(crate) server: WyrdTestServer,
    /// The declaring editor's JWT.
    pub(crate) jwt: String,
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
    pub(crate) async fn start_with(runtime: bool) -> Self {
        Self::launch(|builder| {
            if runtime {
                builder.with_verification_runtime_for_test()
            } else {
                builder
            }
        })
        .await
    }

    /// Start the deployment as [`Self::start`] does, on a `worker` target
    /// running queued work under `limits`, so the webhook, deferred-input
    /// and approval journeys on it complete without the `combined` target.
    ///
    /// # Panics
    /// Panics when a step fails.
    pub(crate) async fn start_limited(limits: RuntimeLimits) -> Self {
        Self::launch(|builder| {
            builder
                .with_bifrost_target_for_test(wyrd_server::config::ServerTarget::Worker)
                .with_verification_runtime_for_test()
                .with_verification_runtime_limits_for_test(limits)
        })
        .await
    }

    /// Start the deployment as [`Self::start`] does, its server further
    /// configured by `configure`.
    ///
    /// # Panics
    /// Panics when a step fails.
    pub(crate) async fn launch(
        configure: impl FnOnce(WyrdTestServerBuilder) -> WyrdTestServerBuilder,
    ) -> Self {
        let mock = GithubAppMock::start().expect("the GitHub mock starts");
        mock.add_installation(INSTALLATION, &[(REPOSITORY, "acme/policy")]);
        mock.add_user("octo", &[INSTALLATION]);
        mock.sign_in("octo");
        mock.install_target(INSTALLATION);
        let builder = configure(WyrdTestServer::builder().with_github_app_for_test(&mock));
        let server = builder
            .start_bound()
            .await
            .expect("bound test server starts");
        Self::connect(&server, &mock).await;
        let Bootstrap::User { jwt, .. } = server
            .bootstrap_user("pull-request-editor", &["editor", "approver"])
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
        self.declare_files(
            &[
                ("a-review.yaml", review_verifier("a-review")),
                ("z-review.yaml", review_verifier("z-review")),
                ("on-pull-request.yaml", ON_PULL_REQUEST.to_owned()),
            ],
            policy,
            key,
        )
        .await;
    }

    /// Declare the Repository `policy` from the checkout holding `files`
    /// beside it under idempotency `key`.
    ///
    /// # Panics
    /// Panics when the declaration is refused.
    pub(crate) async fn declare_files(&self, files: &[(&str, String)], policy: &str, key: &str) {
        let dir = tempfile::tempdir().expect("checkout creates");
        for (name, contents) in files {
            std::fs::write(dir.path().join(name), contents).expect("card file writes");
        }
        std::fs::write(dir.path().join("policy.yaml"), policy).expect("policy file writes");
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
        let status = response.status();
        let body = to_bytes(response.into_body(), 1 << 20)
            .await
            .expect("body reads");
        assert_eq!(
            status,
            StatusCode::CREATED,
            "{}",
            String::from_utf8_lossy(&body)
        );
    }

    /// The server's base URL.
    ///
    /// # Panics
    /// Panics when the server was started without a bound listener.
    pub(crate) fn base(&self) -> &str {
        self.server.base_url().expect("bound")
    }

    /// Deliver `action` on PR `number` from GitHub's current state; returns
    /// the status and delivery id.
    ///
    /// # Panics
    /// Panics when the request cannot be sent.
    pub(crate) async fn send(&self, number: u64, action: &str) -> (StatusCode, String) {
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
    pub(crate) async fn rows(&self, number: u64) -> Vec<Row> {
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

    /// Wait until the newest recorded status of every one of `executions`,
    /// read through the editor's SDK Bifrost query, is `expected`.
    ///
    /// # Panics
    /// Panics when the query fails or any newest status still differs after
    /// ten seconds.
    pub(crate) async fn await_newest(&self, executions: &[Uuid], expected: &str) {
        self.await_newest_as(&self.jwt, executions, expected).await;
    }

    /// [`Self::await_newest`], read as `jwt`.
    ///
    /// # Panics
    /// Panics as [`Self::await_newest`] does.
    pub(crate) async fn await_newest_as(&self, jwt: &str, executions: &[Uuid], expected: &str) {
        let ids = executions
            .iter()
            .map(|execution| format!("'{execution}'"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT e.execution_id, e.status FROM bifrost.verification.execution_events e \
             JOIN (SELECT execution_id, max(occurred_at) AS occurred_at \
                     FROM bifrost.verification.execution_events \
                    WHERE event = 'status' GROUP BY execution_id) l \
               ON e.execution_id = l.execution_id AND e.occurred_at = l.occurred_at \
            WHERE e.event = 'status' AND e.execution_id IN ({ids})"
        );
        let sdk = client(self.base(), jwt);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let mut stream = Bifrost::query_only(&sdk)
                .query(&BifrostQueryRequest {
                    params: Vec::new(),
                    sql: sql.clone(),
                    deadline_ms: Some(30_000),
                })
                .await
                .expect("status events query");
            let mut newest = std::collections::BTreeMap::new();
            while let Some(batch) = stream.next_batch().await.expect("status batch reads") {
                let text = |column: usize| {
                    arrow::compute::cast(batch.column(column), &DataType::Utf8)
                        .expect("column casts to text")
                };
                let (ids, statuses) = (text(0), text(1));
                let ids = ids.as_any().downcast_ref::<StringArray>().expect("text");
                let statuses = statuses
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .expect("text");
                for row in 0..batch.num_rows() {
                    newest.insert(ids.value(row).to_owned(), statuses.value(row).to_owned());
                }
            }
            if executions.iter().all(|execution| {
                newest.get(&execution.to_string()).map(String::as_str) == Some(expected)
            }) {
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "newest statuses never all became {expected}: {newest:?}"
            );
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
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
    pub(crate) async fn status(&self, execution: Uuid) -> Value {
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
    pub(crate) async fn conn(&self) -> wyrd_sql::TenantConn<'_> {
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
    pub(crate) async fn settled(&self, numbers: std::ops::RangeInclusive<u64>) -> Vec<Row> {
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
pub(super) async fn install(
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
            "title": "Change",
            "full_name": "acme/policy",
            "base": {"ref": "main", "sha": "b1"},
            "head": {"ref": "feature", "sha": "a1"},
            "head_repository_id": "4242",
            "delivery_id": opened,
            "generation": served["pull_request"]["generation"],
            "members": [a[0].execution_id.to_string(), a[1].execution_id.to_string()],
        }),
        "the execution is pinned to GitHub's current source and its generation's members"
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
    // Supersession, draft, close and access loss each record the
    // cancellation as the execution's current status.
    let cancelled: Vec<Uuid> = rows.iter().map(|row| row.execution_id).collect();
    deployment.await_newest(&cancelled, "cancelled").await;
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
                &Completion {
                    lease: claimed.lease,
                    result_id: VerificationResultId::new_v7(),
                    verdict: VerificationVerdict::Passed,
                    summary: "P passes".into(),
                    drift: None,
                    pr_source_extracted: true,
                },
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
                &Completion {
                    lease: claimed.lease,
                    result_id: VerificationResultId::new_v7(),
                    verdict: VerificationVerdict::Passed,
                    summary: "A finished late".into(),
                    drift: None,
                    pr_source_extracted: false,
                },
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
                &Completion {
                    lease: claimed.lease,
                    result_id: VerificationResultId::new_v7(),
                    verdict: VerificationVerdict::Passed,
                    summary: "P passes".into(),
                    drift: None,
                    pr_source_extracted: true,
                },
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
            &Completion {
                lease: running.lease,
                result_id: VerificationResultId::new_v7(),
                verdict: VerificationVerdict::Passed,
                summary: "A finished late".into(),
                drift: None,
                pr_source_extracted: false,
            },
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
            &Completion {
                lease: running.lease,
                result_id: VerificationResultId::new_v7(),
                verdict: VerificationVerdict::Passed,
                summary: "finished after the close".into(),
                drift: None,
                pr_source_extracted: false,
            },
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
/// `executions:read` on its trusted parent, the Repository: the same grant
/// on the shared Verifier reads it as unknown, also once the queue row is gone and
/// only the accepted state remains.
///
/// # Panics
/// Panics when a caller reading only the Verifier reads a PR execution, or
/// one reading the Repository is refused.
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
    let verifier = {
        let mut conn = deployment.conn().await;
        let (uid,): (Uuid,) = sqlx::query_as(
            "SELECT verifier_card_uid FROM wyrd.verifier_executions WHERE execution_id = $1",
        )
        .bind(execution)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("execution reads");
        let uid = wyrd_spec::ids::CardUid::from_uuid(uid).expect("card uid");
        let mut scopes =
            wyrd_sql::queries::cards::card_scopes(&mut conn, std::slice::from_ref(&uid))
                .await
                .expect("scope reads");
        conn.commit().await.expect("read commits");
        scopes.remove(&uid).expect("the Verifier is registered")
    };
    server
        .seed_role("pr_verifier_only", &[Permission::executions_read(verifier)])
        .await
        .expect("role seeds");
    server
        .seed_role(
            "pr_verifier_and_repository",
            &[Permission::executions_read_any(), Permission::card_read()],
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
        // Without the Repository parent the execution reads as unknown.
        let (code, body) = read_as(&deployment, &verifier_only, uri).await;
        assert_eq!(code, StatusCode::NOT_FOUND, "{uri}: {body}");
        assert_eq!(body["code"], "WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND");
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
        StatusCode::NOT_FOUND,
        "retained PR state still needs repository read: {body}"
    );
    let (code, body) = read_as(&deployment, &both, &status).await;
    assert_eq!(code, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "completed");
}

/// One PR execution's retained input record, as an authorized SDK Bifrost
/// query returns it.
#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct Retained {
    /// The execution.
    pub(crate) execution_id: Uuid,
    /// The Repository Card version whose binding admitted it.
    pub(crate) workload_card_uid: Option<String>,
    /// Its frozen PR pins.
    pub(crate) inputs: Value,
    /// When its generation observed the pull request, as text that orders
    /// chronologically.
    pub(crate) recorded_at: String,
}

impl Deployment {
    /// Every retained `pull_request` input record of PR `number`, read as
    /// `jwt` through the SDK's Bifrost query once at least `count` are
    /// visible.
    ///
    /// # Panics
    /// Panics when the query fails or fewer than `count` are visible after
    /// ten seconds.
    pub(crate) async fn retained_as(&self, jwt: &str, number: u64, count: usize) -> Vec<Retained> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let rows = self
                .history_as(jwt, number, "TRUE")
                .await
                .expect("retained inputs query");
            if rows.len() >= count {
                return rows;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "only {} of {count} members retained: {rows:?}",
                rows.len()
            );
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    /// The retained `pull_request` input records of PR `number` matching
    /// the SQL `predicate`, read once as `jwt` through the SDK's Bifrost
    /// query.
    ///
    /// # Errors
    /// Returns the query's refusal, such as a caller lacking the table.
    pub(crate) async fn history_as(
        &self,
        jwt: &str,
        number: u64,
        predicate: &str,
    ) -> Result<Vec<Retained>, wyrd_client::bifrost::BifrostClientError> {
        let rows: Vec<Retained> = Bifrost::query_only(&client(self.base(), jwt))
            .sql_as(
                &format!(
                    "SELECT execution_id, workload_card_uid, inputs, \
                            CAST(recorded_at AS VARCHAR) AS recorded_at \
                       FROM bifrost.verification.execution_inputs \
                      WHERE origin = 'pull_request' AND ({predicate}) ORDER BY execution_id"
                ),
                &[],
            )
            .await?;
        Ok(rows
            .into_iter()
            .filter(|row| row.inputs["number"] == json!(number))
            .collect())
    }

    /// The card UID of Repository `policy` at `version`.
    ///
    /// # Panics
    /// Panics when the read fails.
    pub(crate) async fn policy_uid(&self, version: &str) -> String {
        let mut conn = self.conn().await;
        let uid: Uuid = sqlx::query_scalar(
            "SELECT card_uid FROM wyrd.cards
              WHERE kind = 'Repository' AND name = 'policy' AND version = $1",
        )
        .bind(version)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("policy reads");
        conn.commit().await.expect("read commits");
        uid.to_string()
    }
}

/// Assert `retained` holds exactly `rows`' executions, each naming the same
/// generation, the selected `policy`, and the complete member list in
/// binding order.
///
/// # Panics
/// Panics when a member is missing, duplicated or names other membership.
fn assert_complete(retained: &[Retained], rows: &[Row], policy: &str) {
    let members: Vec<Value> = rows
        .iter()
        .map(|row| json!(row.execution_id.to_string()))
        .collect();
    let mut ids: Vec<Uuid> = retained.iter().map(|row| row.execution_id).collect();
    ids.sort_unstable();
    let mut expected: Vec<Uuid> = rows.iter().map(|row| row.execution_id).collect();
    expected.sort_unstable();
    assert_eq!(ids, expected, "every member is retained once");
    let generation = &retained[0].inputs["generation"];
    for row in retained {
        assert_eq!(&row.inputs["generation"], generation, "{row:?}");
        assert_eq!(row.inputs["members"], json!(members), "{row:?}");
        assert_eq!(row.workload_card_uid.as_deref(), Some(policy), "{row:?}");
    }
}

/// Every member of an admitted generation is retained with its generation,
/// selected policy and complete membership, refused members included: a
/// mixed generation keeps its runnable and its refused member, and an
/// all-refused generation keeps both refusals. Refused members record their
/// `errored` status and never become runnable.
///
/// # Panics
/// Panics when a member is not retained, membership is incomplete, a refusal
/// has no recorded status, or a refused member is claimable.
#[tokio::test(flavor = "multi_thread")]
async fn every_admitted_member_is_retained_with_its_status() {
    let deployment = Deployment::start().await;
    let Bootstrap::User { jwt: reader, .. } = deployment
        .server
        .bootstrap_user("pull-request-reader", &["editor"])
        .await
        .expect("reader bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };
    let policy = deployment.policy_uid("1.0.0").await;
    let pool = deployment
        .server
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    sqlx::query(
        "UPDATE wyrd.cards SET status = 'deleted' WHERE kind = 'Verifier' AND name = 'z-review'",
    )
    .execute(&pool)
    .await
    .expect("z-review is deleted");
    deployment
        .mock
        .set_pull_request(REPOSITORY, 21, PullRequest::open(REPOSITORY, "b1", "m1"));

    assert_eq!(deployment.send(21, "opened").await.0, StatusCode::ACCEPTED);
    let mixed = deployment.rows(21).await;
    let mut statuses: Vec<&str> = mixed.iter().map(|row| row.status.as_str()).collect();
    statuses.sort_unstable();
    assert_eq!(statuses, ["errored", "pending"], "{mixed:?}");
    assert_complete(
        &deployment.retained_as(&reader, 21, 2).await,
        &mixed,
        &policy,
    );
    let refused: Vec<Uuid> = mixed
        .iter()
        .filter(|row| row.status == "errored")
        .map(|row| row.execution_id)
        .collect();
    deployment
        .await_newest_as(&reader, &refused, "errored")
        .await;
    let claimed = deployment.claim(10).await;
    assert_eq!(
        claimed
            .iter()
            .map(|claim| claim.lease.execution_id.as_uuid())
            .collect::<Vec<_>>(),
        mixed
            .iter()
            .filter(|row| row.status == "pending")
            .map(|row| row.execution_id)
            .collect::<Vec<_>>(),
        "only the runnable member is claimable"
    );

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
        .set_pull_request(REPOSITORY, 22, PullRequest::open(REPOSITORY, "b1", "r1"));
    assert_eq!(deployment.send(22, "opened").await.0, StatusCode::ACCEPTED);
    let refusals = deployment.rows(22).await;
    assert!(
        refusals.iter().all(|row| row.status == "errored"),
        "{refusals:?}"
    );
    assert_complete(
        &deployment.retained_as(&reader, 22, 2).await,
        &refusals,
        &policy,
    );
    deployment
        .await_newest_as(
            &reader,
            &refusals
                .iter()
                .map(|row| row.execution_id)
                .collect::<Vec<_>>(),
            "errored",
        )
        .await;
    assert!(
        deployment.claim(10).await.is_empty(),
        "a refusal is never runnable"
    );
}

impl Deployment {
    /// The SDK's GitHub connection handle acting as `jwt`.
    ///
    /// # Panics
    /// Panics when the handle cannot connect.
    pub(crate) async fn github(&self, jwt: &str) -> wyrd_client::GithubConnection {
        wyrd_client::GithubConnection::connect_with(&client(self.base(), jwt))
            .await
            .expect("GitHub handle connects")
    }

    /// Send one review request for PR `number` under `key` as the editor,
    /// without the SDK's transport retry, and return its status and body.
    ///
    /// # Panics
    /// Panics when the request cannot be sent.
    pub(crate) async fn review_once(&self, number: u64, key: &str) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/integrations/github/pull-requests/reviews")
            .header(header::CONTENT_TYPE, "application/json")
            .header("Idempotency-Key", key)
            .body(Body::from(
                serde_json::to_vec(&self.review(number)).expect("serializes"),
            ))
            .expect("request builds");
        let response = self
            .server
            .oneshot_authenticated(&self.jwt, request)
            .await
            .expect("review responds");
        let status = response.status();
        let body = to_bytes(response.into_body(), 1 << 20)
            .await
            .expect("body reads");
        (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
    }

    /// The review request for PR `number` of the connected repository.
    pub(crate) fn review(&self, number: u64) -> GithubPullRequestReviewRequest {
        GithubPullRequestReviewRequest {
            host: self.mock.host(),
            repository_id: RepositoryId::new(REPOSITORY.to_string()).expect("valid id"),
            number,
        }
    }
}

/// The execution ids of `rows`.
fn ids(rows: &[Row]) -> Vec<Uuid> {
    rows.iter().map(|row| row.execution_id).collect()
}

/// The execution ids a review response accepted.
fn accepted(response: &GithubPullRequestReviewResponse) -> Vec<Uuid> {
    response
        .executions
        .iter()
        .map(wyrd_spec::ids::ExecutionId::as_uuid)
        .collect()
}

/// An explicit review request admits a new generation at an unchanged head
/// under each distinct key and supersedes the older one; a sequential,
/// concurrent or post-restart retry of a key returns its generation and
/// members unchanged; a policy change applies only to later requests;
/// ordinary redelivery still replays the current generation; and a retry of
/// an older key after the head advanced returns its superseded generation
/// without admitting or reactivating work.
///
/// # Panics
/// Panics when a request admits other members than expected, a retry
/// changes membership, or superseded work becomes runnable again.
#[tokio::test(flavor = "multi_thread")]
async fn explicit_review_requests_admit_new_generations_and_replay_by_key() {
    let deployment = Deployment::start().await;
    deployment
        .mock
        .set_pull_request(REPOSITORY, 31, PullRequest::open(REPOSITORY, "b1", "s1"));
    assert_eq!(deployment.send(31, "opened").await.0, StatusCode::ACCEPTED);
    let delivered = ids(&deployment.rows(31).await);
    let github = deployment.github(&deployment.jwt).await;
    let request = deployment.review(31);

    let first = github
        .review_pull_request(&request, "review-31-a")
        .await
        .expect("a fresh review is accepted");
    assert_eq!(first.executions.len(), 2, "{first:?}");
    let rows = deployment.rows(31).await;
    assert_eq!(rows.len(), 4, "a new generation at the same head: {rows:?}");
    assert_eq!(
        ids(&rows[2..]),
        accepted(&first),
        "members in binding order"
    );
    assert!(
        rows[..2].iter().all(|row| row.status == "cancelled"
            && row.code.as_deref() == Some("pull_request_superseded")),
        "the delivered generation is superseded: {rows:?}"
    );
    assert!(
        rows[2..]
            .iter()
            .all(|row| row.status == "pending" && !row.reserved && row.head == "s1"),
        "{rows:?}"
    );
    assert_eq!(ids(&rows[..2]), delivered);

    assert_eq!(
        github
            .review_pull_request(&request, "review-31-a")
            .await
            .expect("a retry is accepted"),
        first,
        "a retry returns the same generation and members"
    );
    let (left, right) = tokio::join!(
        github.review_pull_request(&request, "review-31-b"),
        github.review_pull_request(&request, "review-31-b"),
    );
    let second = left.expect("a concurrent request is accepted");
    assert_eq!(
        right.expect("its concurrent retry"),
        second,
        "concurrent retries share one generation"
    );
    assert_ne!(second.generation, first.generation);
    assert_eq!(deployment.rows(31).await.len(), 6, "one generation per key");

    assert_eq!(deployment.send(31, "edited").await.0, StatusCode::ACCEPTED);
    let (status, _) = deployment.send(31, "reopened").await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(
        deployment.rows(31).await.len(),
        6,
        "an ordinary redelivery replays the current generation"
    );

    deployment
        .declare(POLICY_V2, "pull-request-policy-v2")
        .await;
    let narrowed = github
        .review_pull_request(&request, "review-31-c")
        .await
        .expect("a request under the new policy is accepted");
    assert_eq!(
        narrowed.executions.len(),
        1,
        "the newly selected policy binds one Verifier"
    );

    deployment
        .mock
        .edit_pull_request(REPOSITORY, 31, |pr| pr.head_sha = "s2".to_owned());
    assert_eq!(
        deployment.send(31, "synchronize").await.0,
        StatusCode::ACCEPTED
    );
    let advanced = deployment.rows(31).await;
    assert_eq!(advanced.len(), 8, "{advanced:?}");
    assert_eq!(advanced[7].head, "s2");
    assert_eq!(
        advanced[6].status, "cancelled",
        "the head advance supersedes it"
    );

    assert_eq!(
        github
            .review_pull_request(&request, "review-31-a")
            .await
            .expect("an old retry is answered"),
        first,
        "an old retry returns its own generation"
    );
    let after = deployment.rows(31).await;
    assert_eq!(
        after, advanced,
        "an old retry admits and reactivates nothing"
    );

    let Deployment { mock, server, jwt } = deployment;
    let restarted =
        Box::pin(server.restart_bound(WyrdTestServer::builder().with_github_app_for_test(&mock)))
            .await
            .expect("server restarts");
    let deployment = Deployment {
        mock,
        server: restarted,
        jwt,
    };
    let github = deployment.github(&deployment.jwt).await;
    assert_eq!(
        github
            .review_pull_request(&request, "review-31-c")
            .await
            .expect("a retry after restart is answered"),
        narrowed,
        "a retry after a restart returns the same generation"
    );
    assert_eq!(deployment.rows(31).await, after);
}

/// A review request accepts no work when the caller may not run a selected
/// Verifier, names another host, or names a pull request that is missing,
/// closed, draft or bound by no Verifier; a provider or accountable
/// authority failure is a typed retryable error whose retry under the same
/// key is then accepted.
///
/// # Panics
/// Panics when a refused request admits work or answers another code.
#[tokio::test(flavor = "multi_thread")]
async fn refused_review_requests_accept_nothing() {
    let deployment = Deployment::start().await;
    let code = |error: WyrdError| error.code().to_owned();
    let Bootstrap::User { jwt: viewer, .. } = deployment
        .server
        .bootstrap_user("pull-request-viewer", &["viewer"])
        .await
        .expect("viewer bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };
    deployment
        .mock
        .set_pull_request(REPOSITORY, 41, PullRequest::open(REPOSITORY, "b1", "d1"));
    let denied = deployment
        .github(&viewer)
        .await
        .review_pull_request(&deployment.review(41), "denied-41")
        .await
        .expect_err("a viewer may not request a review");
    assert_eq!(code(denied), "WYRD_PERMISSION_403_DENIED_RBAC");
    assert!(deployment.rows(41).await.is_empty());

    let github = deployment.github(&deployment.jwt).await;
    let mut other = deployment.review(41);
    other.host = "github.example".to_owned();
    assert_eq!(
        code(
            github
                .review_pull_request(&other, "host-key-41")
                .await
                .expect_err("another host")
        ),
        "WYRD_GITHUB_400_INVALID_REQUEST"
    );

    deployment.mock.set_pull_request(
        REPOSITORY,
        42,
        PullRequest {
            open: false,
            ..PullRequest::open(REPOSITORY, "b1", "c1")
        },
    );
    deployment.mock.set_pull_request(
        REPOSITORY,
        43,
        PullRequest {
            draft: true,
            ..PullRequest::open(REPOSITORY, "b1", "c1")
        },
    );
    for (number, reason) in [(42, "closed"), (43, "draft"), (44, "not_found")] {
        let refused = github
            .review_pull_request(&deployment.review(number), "ineligible")
            .await
            .expect_err("an ineligible pull request is not reviewable");
        assert_eq!(
            refused.code(),
            "WYRD_GITHUB_409_PULL_REQUEST_NOT_REVIEWABLE",
            "{refused}"
        );
        assert_eq!(
            refused.as_problem_json()["details"]["reason"],
            json!(reason),
            "{refused}"
        );
        assert!(deployment.rows(number).await.is_empty());
    }

    deployment
        .mock
        .fail_next_pull_request_read(StatusCode::BAD_GATEWAY);
    let (status, unavailable) = deployment.review_once(41, "retry-41").await;
    assert_eq!(
        (status, &unavailable["code"]),
        (
            StatusCode::SERVICE_UNAVAILABLE,
            &json!("WYRD_GITHUB_503_PROVIDER_UNAVAILABLE")
        ),
        "{unavailable}"
    );
    assert!(deployment.rows(41).await.is_empty());
    let retried = github
        .review_pull_request(&deployment.review(41), "retry-41")
        .await
        .expect("the retry under the same key is accepted");
    assert_eq!(accepted(&retried), ids(&deployment.rows(41).await));

    let pool = deployment
        .server
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    // The schema keeps every SYSTEM principal active; this fixture database
    // lifts that check so the accountable authority has no SYSTEM actor.
    let system = |sql: &'static str| sqlx::raw_sql(sql).execute(&pool);
    deployment
        .mock
        .set_pull_request(REPOSITORY, 45, PullRequest::open(REPOSITORY, "b1", "u1"));
    system(
        "ALTER TABLE wyrd.auth_service_accounts
             DROP CONSTRAINT auth_service_accounts_system_active_check;
         UPDATE wyrd.auth_service_accounts SET status = 'suspended'
          WHERE principal_kind = 'system';",
    )
    .await
    .expect("SYSTEM is suspended");
    let (status, undecided) = deployment.review_once(45, "authority-45").await;
    assert_eq!(
        (status, &undecided["code"]),
        (
            StatusCode::SERVICE_UNAVAILABLE,
            &json!("WYRD_SERVER_503_SERVICE_UNAVAILABLE")
        ),
        "{undecided}"
    );
    assert!(deployment.rows(45).await.is_empty());
    system(
        "UPDATE wyrd.auth_service_accounts SET status = 'active'
          WHERE principal_kind = 'system';
         ALTER TABLE wyrd.auth_service_accounts
           ADD CONSTRAINT auth_service_accounts_system_active_check
           CHECK (principal_kind <> 'system' OR status = 'active');",
    )
    .await
    .expect("SYSTEM is restored");
    let decided = github
        .review_pull_request(&deployment.review(45), "authority-45")
        .await
        .expect("the retry under the same key is accepted");
    assert_eq!(accepted(&decided), ids(&deployment.rows(45).await));

    deployment
        .declare(POLICY_EMPTY, "pull-request-policy-empty")
        .await;
    let unbound = github
        .review_pull_request(&deployment.review(41), "unbound-41")
        .await
        .expect_err("a policy binding nothing reviews nothing");
    assert_eq!(
        unbound.as_problem_json()["details"]["reason"],
        json!("no_bindings")
    );
    assert_eq!(
        accepted(&retried),
        ids(&deployment.rows(41).await),
        "no state changed"
    );
}

/// The newest retained generation in `retained` by its observation time,
/// and whether every member it names is visible.
fn current(retained: &[Retained]) -> Option<(Value, bool)> {
    let newest = retained
        .iter()
        .max_by(|a, b| a.recorded_at.cmp(&b.recorded_at))?;
    let generation = &newest.inputs["generation"];
    let visible: Vec<Value> = retained
        .iter()
        .filter(|row| &row.inputs["generation"] == generation)
        .map(|row| json!(row.execution_id.to_string()))
        .collect();
    let members = newest.inputs["members"].as_array()?;
    Some((
        generation.clone(),
        members.iter().all(|member| visible.contains(member)),
    ))
}

/// The retained records of `generation` in `retained`.
fn of<'r>(retained: &'r [Retained], generation: &Value) -> Vec<&'r Retained> {
    retained
        .iter()
        .filter(|row| &row.inputs["generation"] == generation)
        .collect()
}

/// Authorized retained reads resolve a pull request's current review set
/// and history from its records alone: while a generation's handoff is
/// interrupted its visible records are fewer than the members the domain
/// read names, so it is never presented as complete, and recovery completes
/// it; the current set is the one with the latest PostgreSQL observation
/// time its records carry, whatever order they are read in, and every
/// earlier set keeps its complete membership; a time bound selects history
/// before it. A caller with no read is refused before any row or count, a
/// reader of another Repository family sees none of them and counts none,
/// and another tenant's reader sees none of them.
///
/// # Panics
/// Panics when visibility is reported complete early, latest regresses,
/// history differs, or an unauthorized reader sees membership or counts.
#[tokio::test(flavor = "multi_thread")]
async fn retained_reads_resolve_current_and_history_membership() {
    let deployment = Deployment::start().await;
    let Bootstrap::User { jwt: reader, .. } = deployment
        .server
        .bootstrap_user("pull-request-historian", &["viewer"])
        .await
        .expect("reader bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };
    let github = deployment.github(&deployment.jwt).await;
    deployment
        .mock
        .set_pull_request(REPOSITORY, 51, PullRequest::open(REPOSITORY, "b1", "h1"));
    let first = github
        .review_pull_request(&deployment.review(51), "history-51-a")
        .await
        .expect("the first review is accepted");
    let first_generation = json!(first.generation.to_string());
    let retained = deployment.retained_as(&reader, 51, 2).await;
    assert_eq!(current(&retained), Some((first_generation.clone(), true)));

    deployment
        .server
        .flush_bifrost()
        .await
        .expect("setup settles");
    let held = deployment
        .server
        .stall_next_bifrost_write_into("bifrost.verification.execution_inputs")
        .expect("Scribe stalls");
    let delayed = {
        let github = deployment.github(&deployment.jwt).await;
        let request = deployment.review(51);
        tokio::spawn(async move {
            github
                .review_pull_request(&request, "history-51-b")
                .await
                .expect("the delayed review is accepted")
        })
    };
    held.wait_entered().await;
    let rows = deployment.rows(51).await;
    assert_eq!(
        rows.len(),
        4,
        "the delayed generation is reserved: {rows:?}"
    );
    let pending = deployment.status(rows[2].execution_id).await;
    let delayed_generation = pending["pull_request"]["generation"].clone();
    assert_eq!(
        pending["pull_request"]["members"],
        json!([rows[2].execution_id, rows[3].execution_id]),
        "the domain read names the whole interrupted set"
    );
    let during = deployment
        .history_as(&reader, 51, "TRUE")
        .await
        .expect("history reads");
    assert!(
        of(&during, &delayed_generation).len() < 2,
        "the interrupted set is not fully visible: {during:?}"
    );
    assert_ne!(
        current(&during),
        Some((delayed_generation.clone(), true)),
        "an interrupted set is never complete"
    );

    held.release();
    let recovered = delayed.await.expect("the delayed review joins");
    assert_eq!(json!(recovered.generation.to_string()), delayed_generation);
    let after = deployment.retained_as(&reader, 51, 4).await;
    assert_eq!(
        current(&after),
        Some((delayed_generation.clone(), true)),
        "recovery completes the interrupted set"
    );

    let newest = github
        .review_pull_request(&deployment.review(51), "history-51-c")
        .await
        .expect("the newer review is accepted");
    let newest_generation = json!(newest.generation.to_string());
    let mut history = deployment.retained_as(&reader, 51, 6).await;
    assert_eq!(current(&history), Some((newest_generation.clone(), true)));
    let observed: Vec<&String> = [&first_generation, &delayed_generation, &newest_generation]
        .iter()
        .map(|generation| {
            let members = of(&history, generation);
            assert_eq!(members.len(), 2, "{generation}: {history:?}");
            assert!(members.iter().all(|row| {
                row.inputs["members"].as_array().map(Vec::len) == Some(2)
                    && row.recorded_at == members[0].recorded_at
            }));
            &members[0].recorded_at
        })
        .collect();
    assert!(
        observed.windows(2).all(|pair| pair[0] < pair[1]),
        "each generation records its own later observation: {observed:?}"
    );
    history.sort_by(|a, b| b.recorded_at.cmp(&a.recorded_at));
    assert_eq!(
        current(&history),
        Some((newest_generation.clone(), true)),
        "older records read last do not replace the current set"
    );
    let cutoff = &of(&history, &newest_generation)[0].recorded_at;
    let earlier = deployment
        .history_as(
            &reader,
            51,
            &format!("recorded_at < CAST('{cutoff}' AS TIMESTAMP)"),
        )
        .await
        .expect("bounded history reads");
    let mut generations: Vec<Value> = earlier
        .iter()
        .map(|row| row.inputs["generation"].clone())
        .collect();
    generations.dedup();
    assert_eq!(earlier.len(), 4, "{earlier:?}");
    assert!(
        !generations.contains(&newest_generation),
        "a bound before the current set selects only history"
    );

    let refusal = |error: wyrd_client::bifrost::BifrostClientError| match error {
        wyrd_client::bifrost::BifrostClientError::Transport(error) => error.code().to_owned(),
        other => format!("{other:?}"),
    };
    let reader_of = |name: &'static str, grants: Vec<wyrd_runtime::Permission>| {
        let server = &deployment.server;
        async move {
            server.seed_role(name, &grants).await.expect("role seeds");
            let Bootstrap::User { jwt, .. } = server
                .bootstrap_user(&name.replace('_', "-"), &[name])
                .await
                .expect("reader bootstraps")
            else {
                panic!("user bootstrap returned a non-user principal");
            };
            jwt
        }
    };
    let counted = |jwt: String| {
        let base = deployment.base().to_owned();
        async move {
            Bifrost::query_only(&client(&base, &jwt))
                .sql_as::<Value>(
                    "SELECT count(*) AS members FROM bifrost.verification.execution_inputs",
                    &[],
                )
                .await
        }
    };

    // A caller with no read is refused before any row or count.
    let blind = reader_of("pull_request_blind", Vec::new()).await;
    let members = deployment
        .history_as(&blind, 51, "TRUE")
        .await
        .expect_err("membership is refused");
    assert_eq!(refusal(members), "WYRD_PERMISSION_403_DENIED_RBAC");
    let refused = counted(blind).await.expect_err("counts are refused");
    assert_eq!(refusal(refused), "WYRD_PERMISSION_403_DENIED_RBAC");

    // A reader of another Repository family sees no membership and counts none.
    let other_family =
        wyrd_runtime::PermissionScope::Card(wyrd_runtime::CardPermissionScope::family(
            wyrd_spec::envelope::CardKind::Repository,
            wyrd_spec::ids::CardName::new("other").expect("name"),
        ));
    let elsewhere = reader_of(
        "pull_request_elsewhere",
        vec![wyrd_runtime::Permission::card_read().at(other_family)],
    )
    .await;
    assert!(
        deployment
            .history_as(&elsewhere, 51, "TRUE")
            .await
            .expect("another family's history reads")
            .is_empty()
    );
    assert_eq!(
        counted(elsewhere.clone())
            .await
            .expect("another family counts"),
        [json!({"members": 0})]
    );

    // A reader of executions at this Repository's family sees exactly the
    // viewer's membership and count, and opens each member through the
    // dedicated execution read, which another family's reader cannot tell
    // from an absent execution.
    let policy = reader_of(
        "pull_request_policy",
        vec![wyrd_runtime::Permission::executions_read(
            wyrd_runtime::CardPermissionScope::family(
                wyrd_spec::envelope::CardKind::Repository,
                wyrd_spec::ids::CardName::new("policy").expect("name"),
            ),
        )],
    )
    .await;
    let scoped = deployment
        .history_as(&policy, 51, "TRUE")
        .await
        .expect("the family reader's history reads");
    let ids = |rows: &[Retained]| rows.iter().map(|row| row.execution_id).collect::<Vec<_>>();
    let viewed = deployment
        .history_as(&reader, 51, "TRUE")
        .await
        .expect("the viewer's history reads");
    assert_eq!(ids(&scoped), ids(&viewed));
    assert_eq!(scoped.len(), 6, "{scoped:?}");
    assert_eq!(
        counted(policy.clone())
            .await
            .expect("the family reader counts"),
        counted(reader.clone()).await.expect("the viewer counts"),
    );
    let read_as = |jwt: String, execution: Uuid| {
        let server = &deployment.server;
        async move {
            server
                .oneshot_authenticated(
                    &jwt,
                    Request::builder()
                        .uri(format!("/v1/verification/executions/{execution}"))
                        .body(Body::empty())
                        .expect("request builds"),
                )
                .await
                .expect("the execution read responds")
                .status()
        }
    };
    let (execution, unknown) = (scoped[0].execution_id, Uuid::now_v7());
    assert_eq!(read_as(policy, execution).await, StatusCode::OK);
    assert_eq!(
        read_as(elsewhere.clone(), execution).await,
        read_as(elsewhere, unknown).await,
        "another family's execution reads as an unknown one"
    );

    let other = deployment
        .server
        .seed_tenant("pull-request-foreign")
        .await
        .expect("second tenant seeds");
    let foreigner = deployment
        .server
        .bootstrap_service_in_tenant(other, "pull-request-foreigner", &["viewer"])
        .await
        .expect("foreign reader bootstraps");
    let foreign = deployment
        .server
        .exchange_api_key(foreigner.api_key().expect("a machine principal"))
        .await
        .expect("foreign key exchanges");
    assert!(
        deployment
            .history_as(&foreign, 51, "TRUE")
            .await
            .expect("foreign history reads")
            .is_empty()
    );
    let counted: Vec<Value> = Bifrost::query_only(&client(deployment.base(), &foreign))
        .sql_as(
            "SELECT count(*) AS members FROM bifrost.verification.execution_inputs",
            &[],
        )
        .await
        .expect("foreign count reads");
    assert_eq!(counted, [json!({ "members": 0 })]);
}

/// A review set whose handoff was lost to a restart, then superseded by a
/// newer request before it was retried, is still retained with its complete
/// membership when its key is retried, while the newer set stays current.
///
/// # Panics
/// Panics when the interrupted request is acknowledged, the retry does not
/// replay its generation, or its members are not retained.
#[tokio::test(flavor = "multi_thread")]
async fn a_lost_review_set_superseded_before_its_retry_is_still_retained() {
    let Deployment { mock, server, jwt } = Deployment::start().await;
    mock.set_pull_request(REPOSITORY, 61, PullRequest::open(REPOSITORY, "b1", "l1"));
    server.flush_bifrost().await.expect("setup settles");
    let held = server
        .stall_next_bifrost_write_into("bifrost.verification.execution_inputs")
        .expect("Scribe stalls");
    let lost = {
        let url = format!(
            "{}v1/integrations/github/pull-requests/reviews",
            server
                .base_url()
                .expect("bound")
                .trim_end_matches('/')
                .to_owned()
                + "/"
        );
        let body = GithubPullRequestReviewRequest {
            host: mock.host(),
            repository_id: RepositoryId::new(REPOSITORY.to_string()).expect("valid id"),
            number: 61,
        };
        tokio::spawn(async move {
            reqwest::Client::new()
                .post(url)
                .header("X-Wyrd-Access-Token", format!("Bearer {jwt}"))
                .header("Idempotency-Key", "lost-61-a")
                .json(&body)
                .send()
                .await
                .map(|response| response.status())
        })
    };
    held.wait_entered().await;
    let restarted =
        Box::pin(server.restart_bound(WyrdTestServer::builder().with_github_app_for_test(&mock)))
            .await
            .expect("server restarts");
    held.release();
    let answered = lost.await.expect("joins");
    assert!(
        !matches!(answered, Ok(status) if status.is_success()),
        "an interrupted request is not accepted: {answered:?}"
    );
    let Bootstrap::User { jwt, .. } = restarted
        .bootstrap_user("pull-request-retrier", &["editor"])
        .await
        .expect("retrier bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };
    let deployment = Deployment {
        mock,
        server: restarted,
        jwt,
    };
    let lost_rows = deployment.rows(61).await;
    assert_eq!(
        lost_rows.len(),
        2,
        "the lost set was reserved: {lost_rows:?}"
    );

    let github = deployment.github(&deployment.jwt).await;
    let newer = github
        .review_pull_request(&deployment.review(61), "newer-61-b")
        .await
        .expect("the newer review is accepted");
    let retried = github
        .review_pull_request(&deployment.review(61), "lost-61-a")
        .await
        .expect("the lost key replays");
    assert_eq!(accepted(&retried), ids(&lost_rows));
    let retained = deployment.retained_as(&deployment.jwt, 61, 4).await;
    let lost_generation = json!(retried.generation.to_string());
    assert_eq!(of(&retained, &lost_generation).len(), 2, "{retained:?}");
    assert_eq!(
        current(&retained),
        Some((json!(newer.generation.to_string()), true)),
        "the retried older set does not become current"
    );
}

/// Publish everything `server`'s Scribe holds, so accepted rows have custody of
/// their own, then remove every batch receipt of the tenant's `tables`,
/// as cleanup removes receipts outside recent retry history.
///
/// # Panics
/// Panics when the flush or the removal fails, or no receipt was held.
pub(crate) async fn expire_receipts(server: &WyrdTestServer, tables: &[&str]) {
    server.flush_bifrost().await.expect("accepted rows publish");
    let pool = server
        .pg_fixture()
        .superuser_pool()
        .expect("fixture exposes a superuser pool");
    let removed = sqlx::query(
        "DELETE FROM vala.scribe_batch_commits \
          WHERE data_tenant_id = $1 AND logical_table_fqn = ANY($2)",
    )
    .bind(server.data_tenant_id().as_uuid())
    .bind(tables)
    .execute(&pool)
    .await
    .expect("the receipts are removed");
    assert!(removed.rows_affected() > 0, "receipts were held");
}

/// Record a recent receipt of the tenant's `table` under `batch_id`
/// whose row no query can see, as an accepted batch Scribe has not
/// made visible yet.
///
/// # Panics
/// Panics when the receipt cannot be written.
pub(crate) async fn invisible_receipt(server: &WyrdTestServer, table: &str, batch_id: Uuid) {
    let pool = server
        .pg_fixture()
        .superuser_pool()
        .expect("fixture exposes a superuser pool");
    sqlx::query(
        "INSERT INTO vala.scribe_batch_commits \
         (data_tenant_id, logical_table_fqn, batch_id, slice_set_digest, slice_count, \
          wal_node_id, wal_writer_epoch, wal_shard_id, wal_segment_sequence, wal_lsn_min, \
          wal_lsn_max, request_id, ingested_at, table_version) \
         VALUES ($1, $2, $3, decode(repeat('00', 16), 'hex'), 1, gen_random_uuid(), 1, 0, \
                 0, 0, 0, gen_random_uuid(), statement_timestamp(), 1)",
    )
    .bind(server.data_tenant_id().as_uuid())
    .bind(table)
    .bind(batch_id)
    .execute(&pool)
    .await
    .expect("the receipt is recorded");
}
