//! Pending `group_end` groups of a registered binding, scheduled by
//! Scribe-acknowledged source activity and handed off through the real
//! [`VerificationScheduler`] group pass, [`GroupPoller`] and queue on a bound
//! server.
//!
//! The verification runtime stays off, so no background pass consumes an
//! armed query fault or a pending group and no runner claims an execution;
//! each test runs passes itself. Time elapses only on PostgreSQL's clock: a
//! test brings pending groups due by moving their due time to PostgreSQL's
//! statement time instead of waiting out the binding's `after`.

use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use secrecy::SecretString;
use serde_json::{Value, json};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
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
use wyrd_server::verification::groups::GroupPoller;
use wyrd_server::verification::results::{ResultPayload, ResultPayloadBuilder, ResultRun};
use wyrd_server::verification::scheduler::{GroupHandoff, VerificationScheduler};
use wyrd_spec::ids::{CardUid, ExecutionId, GroupId, SlotName};
use wyrd_spec::registry::RegistrationReceipt;
use wyrd_spec::verification::execution::{InputDeclaration, VerificationResult};
use wyrd_sql::TenantConn;
use wyrd_sql::queries::verifier_executions::{
    ClaimedExecution, EnqueueOutcome, ExecutionInput, GroupActivity, VerifierExecutionQueue,
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
    /// The support Service, owner of every turn.
    owner: CardUid,
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
        let (receipt, service) =
            register(&server, &admin, "verify_in_real_time/support.yaml").await;
        let uid = |name: &str| {
            receipt
                .outcomes
                .iter()
                .find(|outcome| outcome.card_ref.name.as_str() == name)
                .and_then(|outcome| outcome.card_ref.uid.clone())
                .expect("the receipt names the Card")
        };
        Self {
            owner: uid("support"),
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

    /// A scheduler over the server's Postgres and the real [`GroupPoller`],
    /// sharing `capacity` as the runtime's execution capacity.
    fn scheduler(&self, capacity: &Arc<Semaphore>) -> VerificationScheduler {
        let state = self.server.state();
        VerificationScheduler::new(
            state.postgres.wyrd().clone(),
            self.server.operator_pool(),
            VerifierExecutionQueue::default(),
            BackgroundAuthority::new(state.auth.tenant_issuer(&state.scribe_outbox)),
            Duration::from_secs(1),
        )
        .with_groups(
            GroupPoller::new(state.clone(), Duration::from_secs(30)),
            Arc::clone(capacity),
        )
    }

    /// A scheduler like [`Self::scheduler`] resolving owner authority through
    /// `authority` instead of the server's tenant issuer.
    fn scheduler_with(
        &self,
        capacity: &Arc<Semaphore>,
        authority: BackgroundAuthority,
    ) -> VerificationScheduler {
        let state = self.server.state();
        VerificationScheduler::new(
            state.postgres.wyrd().clone(),
            self.server.operator_pool(),
            VerifierExecutionQueue::default(),
            authority,
            Duration::from_secs(1),
        )
        .with_groups(
            GroupPoller::new(state.clone(), Duration::from_secs(30)),
            Arc::clone(capacity),
        )
    }

    /// Run one tenant statement as the admin's tenant transaction and commit.
    ///
    /// # Panics
    /// Panics when the statement or commit fails.
    async fn exec(&self, sql: &'static str) {
        let mut conn = self.conn().await;
        sqlx::query(sql)
            .execute(&mut **conn.transaction())
            .await
            .expect("the statement applies");
        conn.commit().await.expect("the statement commits");
    }

    /// The errored group executions of `group` with their error codes, and
    /// whether each asserts no authority, inputs, snapshot, or result.
    ///
    /// # Panics
    /// Panics when the read fails.
    async fn errored(&self, group: &str) -> Vec<(String, bool)> {
        let mut conn = self.conn().await;
        sqlx::query_as(
            "SELECT error ->> 'code',
                    authority IS NULL AND inputs IS NULL AND group_sources IS NULL
                    AND group_snapshot_sha256 IS NULL AND result_id IS NULL
               FROM wyrd.verifier_executions
              WHERE origin = 'group' AND group_id = $1 AND status = 'errored'
              ORDER BY created_at",
        )
        .bind(group)
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("errored executions read")
    }

    /// The due time of `group`'s pending row, if one exists.
    ///
    /// # Panics
    /// Panics when the read fails.
    async fn pending(&self, group: &str) -> Option<chrono::DateTime<chrono::Utc>> {
        let mut conn = self.conn().await;
        sqlx::query_scalar("SELECT due_at FROM wyrd.verification_group_pending WHERE group_id = $1")
            .bind(group)
            .fetch_optional(&mut **conn.transaction())
            .await
            .expect("the pending row reads")
    }

    /// Bring every pending group due on PostgreSQL's clock.
    ///
    /// # Panics
    /// Panics when the update fails.
    async fn make_due(&self) {
        let mut conn = self.conn().await;
        sqlx::query("UPDATE wyrd.verification_group_pending SET due_at = statement_timestamp()")
            .execute(&mut **conn.transaction())
            .await
            .expect("pending groups come due");
        conn.commit().await.expect("the due times commit");
    }

    /// The acknowledged activity of one `turn-check` source item in `group`,
    /// as the Scribe writer records it.
    ///
    /// C13 uses it to drive the pending-row upsert directly beside a held
    /// handoff, with the support fixture's owner, agent and `turn-check`
    /// identities.
    ///
    /// # Panics
    /// Panics when `group` is not a valid [`GroupId`].
    fn activity(&self, group: &str) -> GroupActivity {
        GroupActivity {
            owner_card_uid: self.owner.clone(),
            card_uid: self.agent.clone(),
            verifier_card_uid: self.turn_check.clone(),
            group_id: GroupId::new(group).expect("a valid group"),
        }
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

    /// Claim the earliest claimable execution, committed so it stays leased.
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

    /// Claim executions until the group execution `outcome` created is
    /// leased, leaving the source executions claimed before it leased too.
    ///
    /// # Panics
    /// Panics when `outcome` created nothing or the queue runs out first.
    async fn claim_group(&self, outcome: &GroupHandoff) -> ClaimedExecution {
        let GroupHandoff::Enqueued(execution) = outcome else {
            panic!("the pass did not create a group execution: {outcome:?}");
        };
        loop {
            let claimed = self.claim().await;
            if claimed.lease.execution_id == *execution {
                return claimed;
            }
        }
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
    async fn snapshot(&self, outcome: &GroupHandoff) -> Value {
        let GroupHandoff::Enqueued(execution) = outcome else {
            panic!("the poll did not create a group execution: {outcome:?}");
        };
        let execution = self.execution(*execution).await;
        execution["group"].clone()
    }
}

/// Register the Service fixture at `fixture` under `fixtures/cards/` as
/// `admin`, credential it as an editor, and exchange its API key, which
/// records the Service's activity.
///
/// Returns the registration receipt and the Service's access token.
///
/// # Panics
/// Panics when loading, registration, or the credential exchange fails.
async fn register(
    server: &WyrdTestServer,
    admin: &str,
    fixture: &str,
) -> (RegistrationReceipt, String) {
    let receipt = register_card(server, admin, fixture).await;
    let machine = server
        .credential_registered_service(&receipt.root, &["editor"])
        .await
        .expect("Service credentials");
    let token = server
        .exchange_api_key(machine.api_key().expect("machine has an API key"))
        .await
        .expect("API key exchanges");
    (receipt, token)
}

/// Register the Card fixture at `fixture` under `fixtures/cards/` as `admin`.
///
/// # Panics
/// Panics when loading or registration fails.
async fn register_card(server: &WyrdTestServer, admin: &str, fixture: &str) -> RegistrationReceipt {
    let path = format!(
        "{}/../../../fixtures/cards/{fixture}",
        env!("CARGO_MANIFEST_DIR")
    );
    let input = build_registration_input(load(std::path::Path::new(&path)).expect("loads"))
        .expect("registration input builds");
    Cards::connect_with(&client(
        server.base_url().expect("bound server has a URL"),
        admin,
    ))
    .await
    .expect("handle connects")
    .register(&input)
    .await
    .expect("the Card registers")
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

/// The Scribe write of `payload`, the result rows of the claimed source
/// execution `source`; staging it leaves `source` unsettled, and staging an
/// equal `payload` again replays the identical rows.
fn result(source: &ClaimedExecution, payload: ResultPayload) -> ScribeWrite {
    ScribeWrite::Result {
        payload,
        attribution: source.attribution,
    }
}

/// The `turn-check` result rows of the claimed source execution `source`
/// in `group`, judging `outcome`.
///
/// # Panics
/// Panics when the rows do not build.
fn payload(
    source: &ClaimedExecution,
    group: &GroupId,
    outcome: &VerificationResult,
) -> ResultPayload {
    ResultPayloadBuilder::new(
        ResultRun {
            execution_id: source.lease.execution_id,
            run_id: None,
            verifier_version: &source.verifier_version,
            card_uid: &source.card_uid,
            owner_card_uid: source.authority.principal.owner_service_uid(),
            group_id: Some(group),
            binding_id: None,
            trigger: None,
            window: None,
        },
        &source.verifier_card_uid,
        chrono::DateTime::UNIX_EPOCH,
        chrono::DateTime::UNIX_EPOCH,
    )
    .build(&outcome.outcome)
    .expect("the result builds")
}

/// Acknowledged evidence, not historical metadata, makes a group due: a
/// visible but unsettled result and an input-only turn form one snapshot
/// after expiration, a physically replayed result pins the same snapshot, a
/// refused read leaves the group pending with no partial snapshot, and the
/// next acknowledged turn reaches a later snapshot while the earlier one
/// stays immutable.
///
/// # Panics
/// Panics when setup fails or a pass hands off other snapshots.
#[tokio::test]
async fn available_rows_form_expiring_group_snapshots() {
    let support = Support::start().await;
    let group = GroupId::new("visible").expect("a valid group");
    let first = support.turn("visible", "yes", None).await;
    let source = support.claim().await;
    assert_eq!(source.lease.execution_id, first);
    let judged = support.execute("yes").await;
    let staged = payload(&source, &group, &judged);
    let state = support.server.state();
    state.scribe_outbox.stage(
        support.server.data_tenant_id(),
        result(&source, staged.clone()),
    );
    let second = support.turn("visible", "later", None).await;
    support
        .server
        .flush_bifrost()
        .await
        .expect("Bifrost flushes");
    let capacity = Arc::new(Semaphore::new(4));
    let scheduler = support.scheduler(&capacity);
    let stop = CancellationToken::new();

    assert!(
        support.pending("visible").await.is_some(),
        "ACKs schedule the group"
    );
    let early = scheduler.group_pass(&stop).await.expect("the pass runs");
    assert!(
        early.is_empty(),
        "nothing hands off before expiration: {early:?}"
    );

    support.make_due().await;
    let handed = scheduler.group_pass(&stop).await.expect("the pass runs");
    assert_eq!(handed.len(), 1, "{handed:?}");
    assert!(
        support.pending("visible").await.is_none(),
        "the handoff consumes it"
    );
    let snapshot = support.snapshot(&handed[0]).await;
    assert_eq!(
        listed(&snapshot, "turns"),
        json!([first, second]),
        "{snapshot}"
    );
    assert_eq!(
        listed(&snapshot, "answered"),
        json!([first, second]),
        "{snapshot}"
    );
    let judging = support.claim_group(&handed[0]).await;
    assert_eq!(values(&judging, "answered"), json!(["passed", null]));
    assert_eq!(
        values(&judging, "turns"),
        json!([{"answer": "yes"}, {"answer": "later"}])
    );

    state
        .scribe_outbox
        .stage(support.server.data_tenant_id(), result(&source, staged));
    support
        .server
        .flush_bifrost()
        .await
        .expect("Bifrost flushes");
    support.make_due().await;
    let replayed = scheduler.group_pass(&stop).await.expect("the pass runs");
    assert!(
        matches!(replayed.as_slice(), [GroupHandoff::AlreadyEnqueued(execution)]
            if GroupHandoff::Enqueued(*execution) == handed[0]),
        "a replayed row pins the same snapshot: {replayed:?}"
    );

    let third = support.turn("visible", "last", None).await;
    support
        .server
        .flush_bifrost()
        .await
        .expect("Bifrost flushes");
    support.make_due().await;
    support.server.fail_next_query_after_batch();
    let refused = scheduler.group_pass(&stop).await.expect("the pass runs");
    assert!(
        refused.is_empty(),
        "a refused read hands off nothing: {refused:?}"
    );
    assert!(
        support.pending("visible").await.is_some(),
        "the group stays pending"
    );

    let later = scheduler.group_pass(&stop).await.expect("the pass runs");
    assert_eq!(later.len(), 1, "{later:?}");
    let next = support.snapshot(&later[0]).await;
    assert_eq!(
        listed(&next, "turns"),
        json!([first, second, third]),
        "{next}"
    );
    assert_eq!(
        support.snapshot(&handed[0]).await,
        snapshot,
        "snapshots are immutable"
    );
}

/// Pending groups debounce on acknowledged activity and stop after handoff:
/// a later ACK moves the one due time; a full capacity pool and a pass with
/// nothing due issue no Bifrost read, which a query fault armed before them
/// and consumed only by the next real read observes; a handoff consumes the
/// row; two claimers never take one group; a source item Scribe
/// acknowledges while a handoff holds its group waits on that row lock and
/// leaves fresh pending work; a pass hands off at most 32 groups of a
/// tenant; the scheduler's own run schedules a real cron occurrence that
/// became due while its group read is stalled, and cancellation leaves the
/// group pending. Unconfigured authority keeps a group pending, while an
/// inactive, retired, or query-denied owner records exactly one terminal
/// errored execution, once, consuming the group — unless writing it fails,
/// which rolls back and keeps it pending.
///
/// # Panics
/// Panics when setup fails or the pending state differs.
#[tokio::test]
async fn pending_groups_debounce_and_stop_after_handoff() {
    let support = Support::start().await;
    support.turn("debounce", "one", None).await;
    support
        .server
        .flush_bifrost()
        .await
        .expect("Bifrost flushes");
    let first_due = support
        .pending("debounce")
        .await
        .expect("the ACK schedules it");
    support.execute("ungrouped").await;
    support.turn("debounce", "two", None).await;
    support
        .server
        .flush_bifrost()
        .await
        .expect("Bifrost flushes");
    let moved = support
        .pending("debounce")
        .await
        .expect("still one pending row");
    assert!(moved > first_due, "{moved} moves past {first_due}");
    let mut conn = support.conn().await;
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM wyrd.verification_group_pending")
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("pending rows count");
    assert_eq!(rows, 1, "ungrouped work schedules nothing");
    drop(conn);

    let capacity = Arc::new(Semaphore::new(1));
    let scheduler = support.scheduler(&capacity);
    let stop = CancellationToken::new();
    support.make_due().await;
    let held = capacity.try_acquire().expect("the one permit");
    support.server.fail_next_query_after_batch();
    let starved = scheduler.group_pass(&stop).await.expect("the pass runs");
    assert!(starved.is_empty(), "no permit, no claim: {starved:?}");
    assert!(support.pending("debounce").await.is_some());
    drop(held);
    let faulted = scheduler.group_pass(&stop).await.expect("the pass runs");
    assert!(
        faulted.is_empty() && support.pending("debounce").await.is_some(),
        "the fault armed before the starved pass is still armed, so it read nothing: {faulted:?}"
    );
    let handed = scheduler.group_pass(&stop).await.expect("the pass runs");
    assert_eq!(handed.len(), 1, "{handed:?}");
    assert!(
        support.pending("debounce").await.is_none(),
        "the handoff consumes it"
    );
    support.server.fail_next_query_after_batch();
    let idle = scheduler.group_pass(&stop).await.expect("the pass runs");
    assert!(idle.is_empty(), "no activity, no handoff: {idle:?}");

    support.turn("debounce", "three", None).await;
    support
        .server
        .flush_bifrost()
        .await
        .expect("Bifrost flushes");
    assert!(
        support.pending("debounce").await.is_some(),
        "new activity reschedules"
    );
    support.make_due().await;
    let faulted = scheduler.group_pass(&stop).await.expect("the pass runs");
    assert!(
        faulted.is_empty() && support.pending("debounce").await.is_some(),
        "the fault armed before the idle pass is still armed, so it read nothing: {faulted:?}"
    );
    let queue = VerifierExecutionQueue::default();
    let mut claimer = support.conn().await;
    let due = queue
        .claim_due_group(&mut claimer)
        .await
        .expect("claim runs")
        .expect("the group is due");
    let mut rival = support.conn().await;
    assert!(
        queue
            .claim_due_group(&mut rival)
            .await
            .expect("claim runs")
            .is_none(),
        "a claimed group is skipped"
    );
    drop(rival);
    let state = support.server.state();
    let authority = BackgroundAuthority::new(state.auth.tenant_issuer(&state.scribe_outbox))
        .resolve(
            &mut claimer,
            due.owner.expect("the Service has a principal"),
        )
        .await
        .expect("authority resolves")
        .expect("the owner is accepted");
    let poller = GroupPoller::new(state.clone(), Duration::from_secs(30));
    let assertion = support
        .server
        .pg_fixture()
        .superuser_pool()
        .expect("fixture exposes a superuser pool");
    let ((), ()) = tokio::join!(
        async {
            support.turn("debounce", "four", None).await;
            support
                .server
                .flush_bifrost()
                .await
                .expect("the arrival is acknowledged");
        },
        async {
            let deadline = tokio::time::Instant::now() + VISIBLE;
            loop {
                let waiting: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM pg_stat_activity
                      WHERE pid <> pg_backend_pid() AND wait_event_type = 'Lock'
                        AND query LIKE '%INSERT INTO wyrd.verification_group_pending%'",
                )
                .fetch_one(&assertion)
                .await
                .expect("activity reads");
                if waiting > 0 {
                    break;
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "the acknowledged arrival never waited on the held group"
                );
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            let outcome = poller
                .poll(&mut claimer, &queue, &due, &authority)
                .await
                .expect("the snapshot pins");
            assert!(
                matches!(outcome, EnqueueOutcome::Enqueued(_)),
                "{outcome:?}"
            );
            queue
                .consume_group(&mut claimer, &due)
                .await
                .expect("consumes");
            claimer.commit().await.expect("the handoff commits");
        },
    );
    assert!(
        support.pending("debounce").await.is_some(),
        "an arrival acknowledged during a handoff leaves fresh pending work"
    );

    let mut conn = support.conn().await;
    let many: Vec<GroupActivity> = (0..33)
        .map(|index| support.activity(&format!("many-{index:02}")))
        .collect();
    queue
        .record_group_activity(&mut conn, &many)
        .await
        .expect("activity records");
    conn.commit().await.expect("activity commits");
    support.make_due().await;
    let wide = Arc::new(Semaphore::new(64));
    let batched = support.scheduler(&wide);
    let first_pass = batched.group_pass(&stop).await.expect("the pass runs");
    assert_eq!(first_pass.len(), 32, "one tenant pass hands off at most 32");
    let second_pass = batched.group_pass(&stop).await.expect("the pass runs");
    assert_eq!(second_pass.len(), 2, "the remainder: {second_pass:?}");

    register_card(
        &support.server,
        &support.admin,
        "observe_a_run/observed-model.yaml",
    )
    .await;
    let (observed, _) = register(
        &support.server,
        &support.admin,
        "observe_a_run/observed-service.yaml",
    )
    .await;
    support.turn("blocked", "one", None).await;
    support
        .server
        .flush_bifrost()
        .await
        .expect("Bifrost flushes");
    support.make_due().await;
    support.server.stall_next_query_after_schema();
    let running = Arc::new(support.scheduler(&wide));
    let halt = CancellationToken::new();
    let run = tokio::spawn(Arc::clone(&running).run(halt.clone()));
    support
        .server
        .wait_query_schema_stall()
        .await
        .expect("the group read stalls");
    // ponytail: a daily occurrence due at today's UTC midnight is claimable
    // only 30s after it, so this run fails in the first 30s of a UTC day.
    support
        .exec(
            "UPDATE wyrd.verification_bindings
                SET next_run_at = date_trunc('day', statement_timestamp() AT TIME ZONE 'UTC')
                                  AT TIME ZONE 'UTC'
              WHERE activation = 'schedule'",
        )
        .await;
    let deadline = tokio::time::Instant::now() + VISIBLE;
    loop {
        let mut conn = support.conn().await;
        let scheduled: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM wyrd.verifier_executions
              WHERE origin = 'schedule' AND owner_card_uid = $1",
        )
        .bind(
            observed
                .root
                .uid
                .as_ref()
                .expect("the receipt has a uid")
                .as_uuid(),
        )
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("executions read");
        if scheduled == 1 {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the cron loop never scheduled while the group read was blocked"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        support.pending("blocked").await.is_some(),
        "the group read is still held"
    );
    halt.cancel();
    run.await.expect("the scheduler stops");
    assert!(
        support.pending("blocked").await.is_some(),
        "a cancelled handoff rolls back and the group stays pending"
    );
    let unavailable = support.scheduler_with(&wide, BackgroundAuthority::new(None));
    let blocked = unavailable.group_pass(&stop).await.expect("the pass runs");
    assert!(
        blocked.is_empty() && support.pending("blocked").await.is_some(),
        "unconfigured authority is retryable, not a refusal: {blocked:?}"
    );
    assert!(support.errored("blocked").await.is_empty());
    let recovered = batched.group_pass(&stop).await.expect("the pass runs");
    assert!(
        matches!(recovered.as_slice(), [GroupHandoff::Enqueued(_)]),
        "{recovered:?}"
    );

    support.turn("lapsed", "one", None).await;
    support
        .server
        .flush_bifrost()
        .await
        .expect("Bifrost flushes");
    support
        .exec(
            "UPDATE wyrd.auth_service_accounts
                SET last_authenticated_at = statement_timestamp() - INTERVAL '400 days'
              WHERE card_uid IS NOT NULL AND principal_kind <> 'system'",
        )
        .await;
    support.make_due().await;
    for statement in [
        "CREATE OR REPLACE FUNCTION public.fail_group_refusal() RETURNS trigger \
         LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected refusal failure'; END $$",
        "CREATE TRIGGER fail_group_refusal BEFORE INSERT ON wyrd.verifier_executions \
         FOR EACH ROW WHEN (NEW.status = 'errored' AND NEW.authority IS NULL) \
         EXECUTE FUNCTION public.fail_group_refusal()",
    ] {
        sqlx::query(statement)
            .execute(&assertion)
            .await
            .expect("the refusal fault installs");
    }
    let failed = batched.group_pass(&stop).await.expect("the pass runs");
    assert!(
        failed.is_empty() && support.pending("lapsed").await.is_some(),
        "a failed refusal write rolls back and keeps the group pending: {failed:?}"
    );
    assert!(
        support.errored("lapsed").await.is_empty(),
        "no partial record"
    );
    sqlx::query("DROP TRIGGER fail_group_refusal ON wyrd.verifier_executions")
        .execute(&assertion)
        .await
        .expect("the refusal fault uninstalls");
    let refused = batched.group_pass(&stop).await.expect("the pass runs");
    let [GroupHandoff::Errored(lapsed)] = refused.as_slice() else {
        panic!("an inactive owner records one refusal: {refused:?}");
    };
    assert!(support.pending("lapsed").await.is_none(), "it is consumed");
    assert_eq!(
        support.errored("lapsed").await,
        [("WYRD_AUTH_401_CREDENTIAL_REVOKED".to_owned(), true)]
    );
    let readback = support.execution(*lapsed).await;
    assert_eq!(readback["status"], "errored", "{readback}");
    assert_eq!(readback["group_id"], "lapsed", "{readback}");
    assert!(readback["group"].is_null() && readback["result"].is_null());
    assert!(readback["dispatches"].as_array().is_some_and(Vec::is_empty));
    let again = batched.group_pass(&stop).await.expect("the pass runs");
    assert!(again.is_empty(), "a refusal is not polled again: {again:?}");
    assert_eq!(support.errored("lapsed").await.len(), 1);
    support
        .exec(
            "UPDATE wyrd.auth_service_accounts SET last_authenticated_at = statement_timestamp()
              WHERE card_uid IS NOT NULL AND principal_kind <> 'system'",
        )
        .await;

    support.turn("retired", "one", None).await;
    support
        .server
        .flush_bifrost()
        .await
        .expect("Bifrost flushes");
    let mut conn = support.conn().await;
    sqlx::query("UPDATE wyrd.cards SET status = 'deprecated' WHERE card_uid = $1")
        .bind(support.owner.as_uuid())
        .execute(&mut **conn.transaction())
        .await
        .expect("the owner retires");
    conn.commit().await.expect("the retirement commits");
    support.make_due().await;
    let refused = batched.group_pass(&stop).await.expect("the pass runs");
    assert!(
        matches!(refused.as_slice(), [GroupHandoff::Errored(_)]),
        "a retired owner records one refusal: {refused:?}"
    );
    assert_eq!(support.errored("retired").await.len(), 1);
    let mut conn = support.conn().await;
    sqlx::query("UPDATE wyrd.cards SET status = 'active' WHERE card_uid = $1")
        .bind(support.owner.as_uuid())
        .execute(&mut **conn.transaction())
        .await
        .expect("the owner returns");
    conn.commit().await.expect("the return commits");

    support.turn("denied", "one", None).await;
    support
        .server
        .flush_bifrost()
        .await
        .expect("Bifrost flushes");
    let mut conn = support.conn().await;
    sqlx::query(
        "DELETE FROM wyrd.auth_service_account_roles
          WHERE service_account_id = (SELECT id FROM wyrd.auth_service_accounts WHERE card_uid = $1)",
    )
    .bind(support.owner.as_uuid())
    .execute(&mut **conn.transaction())
    .await
    .expect("the owner loses its roles");
    conn.commit().await.expect("the revocation commits");
    support.make_due().await;
    let refused = batched.group_pass(&stop).await.expect("the pass runs");
    assert!(
        matches!(refused.as_slice(), [GroupHandoff::Errored(_)]),
        "a denied snapshot read records one refusal: {refused:?}"
    );
    let denied = support.errored("denied").await;
    assert!(
        matches!(denied.as_slice(), [(code, true)] if code.starts_with("WYRD_PERMISSION_403")),
        "{denied:?}"
    );
    let again = batched.group_pass(&stop).await.expect("the pass runs");
    assert!(again.is_empty(), "a refusal is not polled again: {again:?}");
}
