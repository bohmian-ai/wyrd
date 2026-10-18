//! Group polls of a registered `group_end` binding driven through the real
//! [`GroupPoller`], queue and cursor owner on a bound server.
//!
//! The verification runtime stays off, so no background poll consumes an
//! armed query fault or claims the binding; each test claims the binding,
//! polls and commits exactly as one scheduler tick does.

use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use secrecy::SecretString;
use serde_json::{Value, json};
use wyrd_client::WyrdClient;
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::cards::Cards;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::HttpTransport;
use wyrd_client::transport::config::HttpConfig;
use wyrd_client::transport::credential::ResolvedCredential;
use wyrd_loader::{build_registration_input, load};
use wyrd_server::verification::authority::BackgroundAuthority;
use wyrd_server::verification::groups::{GroupPollError, GroupPoller};
use wyrd_spec::ids::{CardUid, ExecutionId};
use wyrd_sql::queries::verifier_executions::{EnqueueOutcome, VerifierExecutionQueue};
use wyrd_testing::{Bootstrap, WyrdTestServer};

/// How long an accepted enqueue may take to become readable.
const VISIBLE: Duration = Duration::from_secs(30);

/// One bound server with the support Service registered and credentialed.
struct Support {
    /// The bound server, verification runtime off.
    server: WyrdTestServer,
    /// Tenant admin token that registered the Service and reads executions.
    admin: String,
    /// The Service's access token, which also records its activity.
    service: String,
    /// The Agent each turn judges.
    agent: CardUid,
    /// The per-turn source Verifier.
    turn_check: CardUid,
}

impl Support {
    /// Boot a bound server, register the support Service fixture, and
    /// credential and activate the Service.
    ///
    /// # Panics
    /// Panics when the server, registration, or credential exchange fails.
    async fn start() -> Self {
        let server = WyrdTestServer::builder()
            .start_bound()
            .await
            .expect("bound server starts");
        let Bootstrap::User { jwt: admin, .. } = server
            .bootstrap_user("group-admin", &["admin"])
            .await
            .expect("admin bootstraps")
        else {
            panic!("user bootstrap returned a machine principal");
        };
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../fixtures/cards/verify_in_real_time/support.yaml"
        );
        let input = build_registration_input(load(std::path::Path::new(path)).expect("loads"))
            .expect("registration input builds");
        let receipt = Cards::connect_with(&client(
            server.base_url().expect("bound server has a URL"),
            &admin,
        ))
        .await
        .expect("handle connects")
        .register(&input)
        .await
        .expect("support registers");
        let uid = |name: &str| {
            receipt
                .outcomes
                .iter()
                .find(|outcome| outcome.card_ref.name.as_str() == name)
                .and_then(|outcome| outcome.card_ref.uid.clone())
                .expect("the receipt names the Card")
        };
        let machine = server
            .credential_registered_service(&receipt.root, &["editor"])
            .await
            .expect("Service credentials");
        let service = server
            .exchange_api_key(machine.api_key().expect("machine has an API key"))
            .await
            .expect("API key exchanges");
        Self {
            agent: uid("assistant-agent"),
            turn_check: uid("turn-check"),
            server,
            admin,
            service,
        }
    }

    /// Enqueue one turn of `group` answering `answer` as the Service,
    /// optionally under an Idempotency-Key, and wait until it is readable.
    ///
    /// # Panics
    /// Panics when the enqueue is not accepted or never becomes readable.
    async fn turn(&self, group: &str, answer: &str, key: Option<&str>) -> ExecutionId {
        let body = json!({
            "verifier_card_uid": self.turn_check,
            "card_uid": self.agent,
            "group_id": group,
            "inputs": {"turn": {"kind": "json", "value": {"answer": answer}}},
        });
        let mut request = Request::builder()
            .method(Method::POST)
            .uri("/v1/verification/enqueue")
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(key) = key {
            request = request.header("Idempotency-Key", key);
        }
        let (status, accepted) = self
            .send(
                &self.service,
                request.body(Body::from(body.to_string())).expect("builds"),
            )
            .await;
        assert_eq!(status, StatusCode::ACCEPTED, "{accepted}");
        let execution: ExecutionId =
            serde_json::from_value(accepted["execution_id"].clone()).expect("an execution ID");
        self.execution(execution).await;
        execution
    }

    /// Read `execution` as the admin once the queue writer has accepted it.
    ///
    /// # Panics
    /// Panics when it is not readable within [`VISIBLE`] or a read answers
    /// anything but OK or not found.
    async fn execution(&self, execution: ExecutionId) -> Value {
        let deadline = tokio::time::Instant::now() + VISIBLE;
        loop {
            let (status, body) = self
                .send(
                    &self.admin,
                    Request::builder()
                        .uri(format!("/v1/verification/executions/{execution}"))
                        .body(Body::empty())
                        .expect("builds"),
                )
                .await;
            if status == StatusCode::OK {
                return body;
            }
            assert_eq!(
                body["code"], "WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND",
                "{status}: {body}"
            );
            assert!(
                tokio::time::Instant::now() < deadline,
                "execution {execution} never became readable"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Send `request` as `jwt` and decode the JSON answer.
    ///
    /// # Panics
    /// Panics when the route does not respond or the body is not JSON.
    async fn send(&self, jwt: &str, request: Request<Body>) -> (StatusCode, Value) {
        let response = self
            .server
            .oneshot_authenticated(jwt, request)
            .await
            .expect("route responds");
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 1 << 20)
            .await
            .expect("body reads");
        (
            status,
            serde_json::from_slice(&bytes).expect("body is JSON"),
        )
    }

    /// Run one scheduler tick of the tenant's `group_end` binding through
    /// `poller`: bring its cursor due, claim it, judge its groups a minute
    /// past PostgreSQL's clock so every idle window has elapsed, and commit
    /// whatever the poll enqueued with the advanced cursor.
    ///
    /// # Panics
    /// Panics when a Postgres statement fails, nothing is due, or the
    /// Service is not active.
    async fn tick(&self, poller: &GroupPoller) -> Result<Vec<EnqueueOutcome>, GroupPollError> {
        let tenant = self.server.data_tenant_id();
        let mut conn = self
            .server
            .tenant_conn_for(tenant)
            .await
            .expect("tenant connection opens");
        sqlx::query(
            "UPDATE wyrd.verification_bindings SET next_run_at = statement_timestamp() \
              WHERE activation = 'group_end'",
        )
        .execute(&mut **conn.transaction())
        .await
        .expect("the binding is due");
        let queue = VerifierExecutionQueue::default();
        let mut due = queue
            .claim_due_group(&mut conn)
            .await
            .expect("claim runs")
            .expect("the group binding is due");
        due.clock += chrono::TimeDelta::minutes(1);
        let state = self.server.state();
        let authority = BackgroundAuthority::new(state.auth.tenant_issuer(&state.scribe_outbox))
            .resolve(&mut conn, due.owner.expect("the Service is active"))
            .await
            .expect("authority resolves")
            .expect("the owner is accepted");
        let polled = poller.poll(&mut conn, &queue, &due, &authority).await;
        conn.commit().await.expect("the tick commits");
        polled
    }

    /// The group snapshot of the group execution `outcome` created.
    ///
    /// # Panics
    /// Panics when `outcome` created nothing or the execution has no group.
    async fn snapshot(&self, outcome: &EnqueueOutcome) -> Value {
        let EnqueueOutcome::Enqueued(execution) = outcome else {
            panic!("the poll did not create a group execution: {outcome:?}");
        };
        let execution = self.execution(*execution).await;
        execution["group"].clone()
    }
}

/// Assemble the production HTTP client the registry saga runs over.
///
/// # Panics
/// Panics when the auth or transport layers cannot be constructed.
fn client(base_url: &str, jwt: &str) -> WyrdClient {
    let config = ClientConfig {
        http: HttpConfig {
            base_url: base_url.to_owned(),
            ..HttpConfig::default()
        },
        ..ClientConfig::default()
    };
    let auth = AuthMiddleware::new(
        &config,
        ResolvedCredential::BearerToken(SecretString::from(jwt.to_owned())),
    )
    .expect("client auth builds");
    let transport = HttpTransport::new(&config.http, Arc::clone(&auth)).expect("transport builds");
    WyrdClient::from_parts(auth, transport, config.grpc)
}

/// The execution IDs a snapshot's `slot` lists.
fn listed(snapshot: &Value, slot: &str) -> Value {
    snapshot["sources"][slot]["execution_ids"].clone()
}

/// A group read cut short after its first batch enqueues nothing; the next
/// tick creates one snapshot listing every turn, and a third tick creates no
/// second one for the same activity.
///
/// # Panics
/// Panics when setup fails, the truncated tick enqueues, or the recovered
/// snapshot is not the one complete selection.
#[tokio::test]
async fn partial_group_reads_enqueue_nothing_until_a_complete_poll() {
    let support = Support::start().await;
    let turns = [
        support.turn("partial", "one", None).await,
        support.turn("partial", "two", None).await,
    ];
    support
        .server
        .flush_bifrost()
        .await
        .expect("Bifrost flushes");
    let poller = GroupPoller::new(support.server.state().clone(), 20, Duration::from_secs(30));

    support.server.fail_next_query_after_batch();
    let refused = support.tick(&poller).await;
    assert!(
        matches!(refused, Err(GroupPollError::Read(_))),
        "{refused:?}"
    );

    let enqueued = support.tick(&poller).await.expect("the next tick reads");
    assert_eq!(enqueued.len(), 1, "{enqueued:?}");
    let snapshot = support.snapshot(&enqueued[0]).await;
    let ids = json!(turns);
    assert_eq!(listed(&snapshot, "turns"), ids, "{snapshot}");
    assert_eq!(listed(&snapshot, "answered"), ids, "{snapshot}");
    assert_eq!(
        snapshot["members"], 2,
        "two inputs, no results yet: {snapshot}"
    );

    let again = support.tick(&poller).await.expect("a covered tick reads");
    assert!(
        again.is_empty(),
        "covered activity is not re-enqueued: {again:?}"
    );
}

/// Under a ceiling of two, a group of two accepted turns and a refused
/// idempotency conflict is judged normally over the two, while a group of
/// three accepted turns is marked over the ceiling and lists nothing.
///
/// # Panics
/// Panics when setup fails or either snapshot differs.
#[tokio::test]
async fn refused_submissions_do_not_count_toward_the_ceiling() {
    let support = Support::start().await;
    let first = support
        .turn("at-ceiling", "one", Some("at-ceiling-0001"))
        .await;
    support
        .turn("at-ceiling", "conflict", Some("at-ceiling-0001"))
        .await;
    let second = support.turn("at-ceiling", "two", None).await;
    for answer in ["one", "two", "three"] {
        support.turn("over", answer, None).await;
    }
    support
        .server
        .flush_bifrost()
        .await
        .expect("Bifrost flushes");
    let poller = GroupPoller::new(support.server.state().clone(), 2, Duration::from_secs(30));

    let enqueued = support.tick(&poller).await.expect("the tick reads");
    assert_eq!(enqueued.len(), 2, "{enqueued:?}");
    let mut snapshots = Vec::new();
    for outcome in &enqueued {
        snapshots.push(support.snapshot(outcome).await);
    }
    let normal = snapshots
        .iter()
        .find(|snapshot| snapshot["sources"]["turns"]["exceeded_ceiling"].is_null())
        .expect("the at-ceiling group is judged normally");
    assert_eq!(listed(normal, "turns"), json!([first, second]), "{normal}");
    let over = snapshots
        .iter()
        .find(|snapshot| snapshot["sources"]["turns"]["exceeded_ceiling"] == 2)
        .expect("the over group is marked");
    assert_eq!(listed(over, "turns"), json!([]), "{over}");
    assert_eq!(over["sources"]["answered"]["exceeded_ceiling"], 2, "{over}");
}
