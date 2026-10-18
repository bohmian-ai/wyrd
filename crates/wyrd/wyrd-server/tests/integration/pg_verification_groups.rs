//! Group polls of a registered `group_end` binding driven through the real
//! [`GroupPoller`], queue and cursor owner on a bound server.
//!
//! The verification runtime stays off, so no background poll consumes an
//! armed query fault or claims the binding and no runner claims a source
//! execution; each test claims the binding, polls and commits exactly as one
//! scheduler tick does. Time elapses only on PostgreSQL's clock: rows carry
//! Bifrost's PostgreSQL-anchored ingest time, and each tick lets one second
//! pass there before judging under a one-second idle.

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
use wyrd_server::scribe_outbox::ScribeWrite;
use wyrd_server::verification::authority::BackgroundAuthority;
use wyrd_server::verification::groups::{GroupPollError, GroupPoller};
use wyrd_server::verification::results::{ResultPayloadBuilder, ResultRun};
use wyrd_spec::card::trigger::TriggerDuration;
use wyrd_spec::ids::{CardUid, ExecutionId, GroupId, SlotName, VerificationResultId};
use wyrd_spec::verification::VerificationVerdict;
use wyrd_spec::verification::execution::{InputDeclaration, VerificationResult};
use wyrd_sql::TenantConn;
use wyrd_sql::queries::verifier_executions::{
    ClaimedExecution, EnqueueOutcome, ExecutionInput, VerifierExecutionQueue,
};
use wyrd_testing::{Bootstrap, WyrdTestServer};

/// How long an accepted enqueue may take to become readable.
const VISIBLE: Duration = Duration::from_secs(30);

/// How long a test claim leases an execution; no runner competes for it.
const LEASE: chrono::TimeDelta = chrono::TimeDelta::minutes(10);

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
    /// `poller` under a one-second idle and `finality`: let one second pass
    /// on PostgreSQL's clock, bring the cursor due, claim it, poll, and
    /// commit whatever the poll enqueued with the advanced cursor.
    ///
    /// # Errors
    /// Returns the poll's [`GroupPollError`]: a failed or refused Bifrost
    /// read, or a failed Postgres statement. The tick still commits, so the
    /// claimed cursor and every whole snapshot enqueued before the failure
    /// persist.
    ///
    /// # Panics
    /// Panics when a Postgres statement outside the poll fails, nothing is
    /// due, or the Service is not active.
    async fn tick(
        &self,
        poller: &GroupPoller,
        finality: &str,
    ) -> Result<Vec<EnqueueOutcome>, GroupPollError> {
        let mut conn = self.conn().await;
        sqlx::query("SELECT pg_sleep(1)")
            .execute(&mut **conn.transaction())
            .await
            .expect("PostgreSQL time passes");
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
        let duration = |text| TriggerDuration::parse(text).expect("a valid duration");
        due.settings.idle = duration("1s");
        due.settings.finality = duration(finality);
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

    /// A transaction on the tenant's Postgres schema.
    ///
    /// # Panics
    /// Panics when the connection does not open.
    async fn conn(&self) -> TenantConn<'_> {
        self.server
            .tenant_conn_for(self.server.data_tenant_id())
            .await
            .expect("tenant connection opens")
    }

    /// Claim the one claimable execution, committed so it stays leased.
    ///
    /// # Panics
    /// Panics when the claim fails or nothing is claimable.
    async fn claim(&self) -> ClaimedExecution {
        let mut conn = self.conn().await;
        let claimed = VerifierExecutionQueue::default()
            .claim(&mut conn, LEASE)
            .await
            .expect("claim runs")
            .expect("an execution is claimable");
        conn.commit().await.expect("the claim commits");
        claimed
    }

    /// Settle `claimed` completed with `result_id`, which becomes its
    /// selected result.
    ///
    /// # Panics
    /// Panics when the settlement fails or does not apply.
    async fn complete(&self, claimed: &ClaimedExecution, result_id: VerificationResultId) {
        let mut conn = self.conn().await;
        VerifierExecutionQueue::default()
            .complete(
                &mut conn,
                claimed.lease,
                result_id,
                VerificationVerdict::Passed,
                "",
                None,
            )
            .await
            .expect("the settlement applies");
        conn.commit().await.expect("the settlement commits");
    }

    /// Judge one ungrouped turn answering `answer` directly and return its
    /// result.
    ///
    /// # Panics
    /// Panics when the execution is not answered OK.
    async fn execute(&self, answer: &str) -> VerificationResult {
        let body = json!({
            "verifier_card_uid": self.turn_check,
            "card_uid": self.agent,
            "inputs": {"turn": {"kind": "json", "value": {"answer": answer}}},
        });
        let (status, result) = self
            .send(
                &self.service,
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/verification/execute")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string()))
                    .expect("builds"),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{result}");
        serde_json::from_value(result).expect("a verification result")
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

/// The value list `claimed` group execution pins for `slot`.
///
/// # Panics
/// Panics when `claimed` is no group execution or lacks the slot.
fn values(claimed: &ClaimedExecution, slot: &str) -> Value {
    let ExecutionInput::Grouped { inputs, .. } = &claimed.input else {
        panic!("not a group execution: {:?}", claimed.input);
    };
    let Some(InputDeclaration::Json { value }) =
        inputs.get(&SlotName::new(slot).expect("a valid slot"))
    else {
        panic!("no {slot} list: {inputs:?}");
    };
    value.clone()
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
    let refused = support.tick(&poller, "1d").await;
    assert!(
        matches!(refused, Err(GroupPollError::Read(_))),
        "{refused:?}"
    );

    let enqueued = support
        .tick(&poller, "1d")
        .await
        .expect("the next tick reads");
    assert_eq!(enqueued.len(), 1, "{enqueued:?}");
    let snapshot = support.snapshot(&enqueued[0]).await;
    let ids = json!(turns);
    assert_eq!(listed(&snapshot, "turns"), ids, "{snapshot}");
    assert_eq!(listed(&snapshot, "answered"), ids, "{snapshot}");
    assert_eq!(
        snapshot["members"], 2,
        "two inputs, no results yet: {snapshot}"
    );

    let again = support
        .tick(&poller, "1d")
        .await
        .expect("a covered tick reads");
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

    let enqueued = support.tick(&poller, "1d").await.expect("the tick reads");
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

/// A queued turn's result visible before its settlement selects it is no
/// source row: the review pins its outcome null and counts only its input.
/// Selecting that exact visible result, without republishing it, yields one
/// next whole review with the actual outcome, and an unchanged poll none.
///
/// # Panics
/// Panics when setup fails or a review pins or counts otherwise.
#[tokio::test]
async fn a_visible_result_counts_once_its_settlement_selects_it() {
    let support = Support::start().await;
    let turn = support.turn("selected", "yes", None).await;
    let outcome = support.execute("yes").await.outcome;
    let source = support.claim().await;
    assert_eq!(source.lease.execution_id, turn);
    let group = GroupId::new("selected").expect("a valid group");
    let payload = ResultPayloadBuilder::new(
        ResultRun {
            execution_id: turn,
            run_id: None,
            verifier_version: &source.verifier_version,
            card_uid: &source.card_uid,
            owner_card_uid: source.authority.principal.owner_service_uid(),
            group_id: Some(&group),
            binding_id: None,
            trigger: None,
            window: None,
        },
        &source.verifier_card_uid,
        chrono::DateTime::UNIX_EPOCH,
        chrono::DateTime::UNIX_EPOCH,
    )
    .build(&outcome)
    .expect("the result builds");
    let result_id = payload.result_id();
    let state = support.server.state();
    state.scribe_outbox.stage(
        support.server.data_tenant_id(),
        ScribeWrite::Result {
            payload,
            attribution: source.attribution,
        },
    );
    support
        .server
        .flush_bifrost()
        .await
        .expect("Bifrost flushes");
    let poller = GroupPoller::new(state.clone(), 20, Duration::from_secs(30));

    let enqueued = support.tick(&poller, "1d").await.expect("the tick reads");
    assert_eq!(enqueued.len(), 1, "{enqueued:?}");
    let first = support.snapshot(&enqueued[0]).await;
    assert_eq!(first["members"], 1, "only the input counts: {first}");
    let review = support.claim().await;
    assert_eq!(values(&review, "answered"), json!([null]));
    support
        .complete(&review, VerificationResultId::new_v7())
        .await;

    support.complete(&source, result_id).await;
    let enqueued = support.tick(&poller, "1d").await.expect("the tick reads");
    assert_eq!(enqueued.len(), 1, "{enqueued:?}");
    let second = support.snapshot(&enqueued[0]).await;
    assert_eq!(second["members"], 2, "{second}");
    assert_eq!(listed(&second, "answered"), json!([turn]), "{second}");
    let review = support.claim().await;
    assert_eq!(values(&review, "answered"), json!(["passed"]));
    assert_eq!(values(&review, "turns"), json!([{"answer": "yes"}]));

    let again = support.tick(&poller, "1d").await.expect("the tick reads");
    assert!(again.is_empty(), "unchanged activity: {again:?}");
}

/// Finality anchors on a group's first eligible row: a group whose first
/// row is an old refused submission is judged from its recent accepted
/// turn, while a group whose old first row is accepted stays past finality.
///
/// # Panics
/// Panics when setup fails or the tick judges other groups.
#[tokio::test]
async fn finality_anchors_on_the_first_eligible_row() {
    let support = Support::start().await;
    support
        .turn("old-accepted", "one", Some("finality-0001"))
        .await;
    support
        .turn("refused-first", "conflict", Some("finality-0001"))
        .await;
    support
        .server
        .flush_bifrost()
        .await
        .expect("Bifrost flushes");
    let mut conn = support.conn().await;
    sqlx::query("SELECT pg_sleep(10)")
        .execute(&mut **conn.transaction())
        .await
        .expect("PostgreSQL time passes");
    conn.commit().await.expect("the wait commits");
    let accepted = support.turn("refused-first", "two", None).await;
    support
        .server
        .flush_bifrost()
        .await
        .expect("Bifrost flushes");
    let poller = GroupPoller::new(support.server.state().clone(), 20, Duration::from_secs(30));

    let enqueued = support.tick(&poller, "10s").await.expect("the tick reads");
    assert_eq!(enqueued.len(), 1, "{enqueued:?}");
    let snapshot = support.snapshot(&enqueued[0]).await;
    assert_eq!(listed(&snapshot, "turns"), json!([accepted]), "{snapshot}");
}
