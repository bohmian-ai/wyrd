//! Verification control-plane routes through the authenticated HTTP surface.
//!
//! Drives `GET /v1/verification/bindings/{id}`, `POST /v1/verification/enqueue`,
//! `GET /v1/verification/executions/{id}`, and `POST /v1/verification/execute`
//! against the in-process server and the repository-managed Postgres:
//! acknowledged enqueue under the caller's authority, Idempotency-Key replay
//! and conflict, every refusal before staging, the audited allow and deny
//! decisions, and tenant isolation. No runtime claims the queue, so every
//! accepted execution stays `pending`; direct executions judge inline and never
//! enqueue. It also proves the tenant's internal SYSTEM result writer is
//! unreachable through the public principal and credential routes.

use std::time::Duration;

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, Response, StatusCode, header};
use serde_json::{Value, json};
use uuid::Uuid;
use wyrd_runtime::Permission;
use wyrd_spec::auth::TokenAudience;
use wyrd_spec::ids::{BindingId, CardUid, DataTenantId, ExecutionId};
use wyrd_spec::reference::CardRef;
use wyrd_testing::verification::VerificationFixture;
use wyrd_testing::{Bootstrap, WyrdTestServer};

/// How long an acknowledged submission may take to become readable.
const VISIBLE: Duration = Duration::from_secs(10);

/// An assertion Verifier spec whose one requirement holds when the inline
/// `answer` slot equals `1`.
fn answer_is_one() -> Value {
    json!({
        "inputs": { "answer": { "type": "json" } },
        "requirements": [{
            "id": "answer-is-one",
            "statement": "The answer is one.",
            "check": { "kind": "assert", "path": "inputs.answer", "operator": "equals", "expected": 1 }
        }]
    })
}

/// A PSI Drift Verifier spec over the `score` samples, never ready until its
/// baseline is fitted.
fn psi_drift() -> Value {
    json!({
        "inputs": { "samples": { "type": "samples", "features": ["score"] } },
        "requirements": [{
            "id": "drift",
            "statement": "The window does not drift.",
            "check": {
                "kind": "drift", "on": "inputs.samples", "method": "Psi",
                "signal": { "kind": "Distribution",
                            "baseline_ref": { "kind": "Data", "name": "vr-psi-baseline", "version": "1.0.0" },
                            "features": ["score"] },
                "condition": { "kind": "Statistical" },
                "profile": { "kind": "Psi",
                             "binning_strategy": { "kind": "EqualWidth", "n_bins": 10 },
                             "threshold": { "kind": "Fixed", "value": 0.25 } }
            }
        }]
    })
}

/// The tenant fixture: one Service, an assertion Verifier for direct and
/// queued executions, and a ready and an unready Drift binding.
struct Fixture {
    /// The tenant's verification fixture, which reads its queue.
    seed: VerificationFixture,
    /// The Service owning both bindings and the subject of every execution.
    service: CardUid,
    /// Assertion Verifier judging the `answer` slot.
    assertion: CardUid,
    /// Ready Custom Drift Verifier.
    drift: CardUid,
    /// Binding of the Custom Drift Verifier.
    ready_binding: BindingId,
    /// Binding of a PSI Verifier with no fitted baseline.
    unready_binding: BindingId,
}

impl Fixture {
    /// Seed the fixture Cards and bindings in the server's tenant.
    ///
    /// # Panics
    /// Panics when any fixture write fails.
    async fn seed(server: &WyrdTestServer) -> Self {
        let seed = server
            .verification_fixture()
            .await
            .expect("verification fixture provisions");
        let (service, _) = seed.service("vr-service").await.expect("Service registers");
        let assertion = seed
            .verifier("vr-assertion", &answer_is_one())
            .await
            .expect("assertion Verifier registers");
        let drift = seed
            .custom_drift_verifier("vr-custom", "latency", 100.0, 10.0)
            .await
            .expect("Custom Drift Verifier registers");
        let psi = seed
            .verifier("vr-psi", &psi_drift())
            .await
            .expect("PSI Verifier registers");
        let ready_binding = seed
            .bind_schedule(&service, &service, &drift, "0 2 * * *", Vec::new())
            .await
            .expect("ready binding projects");
        let unready_binding = seed
            .bind_schedule(&service, &service, &psi, "0 2 * * *", Vec::new())
            .await
            .expect("unready binding projects");
        Self {
            seed,
            service,
            assertion,
            drift,
            ready_binding,
            unready_binding,
        }
    }

    /// A request judging the Service with `verifier` over the inline `answer`.
    fn request(&self, verifier: &CardUid, answer: Value) -> Value {
        json!({
            "verifier_card_uid": verifier,
            "card_uid": self.service,
            "inputs": { "answer": { "kind": "json", "value": answer } }
        })
    }

    /// The exact reference of the Service every request judges.
    ///
    /// # Panics
    /// Panics when the reference does not decode, which a registered UID
    /// never causes.
    fn subject_ref(&self) -> CardRef {
        serde_json::from_value(json!({
            "kind": "Service", "name": "vr-service", "version": "1.0.0", "uid": self.service
        }))
        .expect("subject ref decodes")
    }

    /// Count the tenant's queued executions.
    ///
    /// # Panics
    /// Panics when the queue cannot be read.
    async fn queued(&self) -> usize {
        self.seed.executions().await.expect("executions read").len()
    }

    /// Count the tenant's queued executions once every earlier request has
    /// been processed, excluding a sentinel enqueued as `jwt` to order the
    /// read.
    ///
    /// Enqueue answers before Scribe acknowledges the inputs and the
    /// submission writer commits the execution, so a plain count can run
    /// ahead of an earlier request's processing. The tenant's writes are
    /// processed in arrival order on both outboxes, so once the later
    /// sentinel is visible every earlier staged submission has been
    /// processed too.
    ///
    /// # Panics
    /// Panics when the sentinel is not accepted or never becomes visible, or
    /// the queue cannot be read.
    async fn queued_before_sentinel(&self, server: &WyrdTestServer, jwt: &str) -> usize {
        let (status, sentinel) = enqueue(
            server,
            jwt,
            &self.request(&self.assertion, json!("sentinel")),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::ACCEPTED, "{sentinel}");
        visible_execution(server, jwt, execution_id(&sentinel)).await;
        self.queued().await - 1
    }
}

/// POST one raw body to `uri` as `jwt`, optionally keyed.
///
/// # Panics
/// Panics when the request cannot be built or the route fails to respond.
async fn post(
    server: &WyrdTestServer,
    jwt: &str,
    uri: &str,
    body: String,
    key: Option<&str>,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(key) = key {
        request = request.header("Idempotency-Key", key);
    }
    let response = server
        .oneshot_authenticated(jwt, request.body(Body::from(body)).expect("request builds"))
        .await
        .expect("request responds");
    decoded(response).await
}

/// POST one enqueue request as `jwt`, optionally keyed.
///
/// # Panics
/// Panics when the request cannot be built or the route fails to respond.
async fn enqueue(
    server: &WyrdTestServer,
    jwt: &str,
    body: &Value,
    key: Option<&str>,
) -> (StatusCode, Value) {
    post(
        server,
        jwt,
        "/v1/verification/enqueue",
        body.to_string(),
        key,
    )
    .await
}

/// POST one raw direct execution body as `jwt`.
///
/// # Panics
/// Panics when the request cannot be built or the route fails to respond.
async fn execute(server: &WyrdTestServer, jwt: &str, body: String) -> (StatusCode, Value) {
    post(server, jwt, "/v1/verification/execute", body, None).await
}

/// The execution ID of an accepted enqueue response.
///
/// # Panics
/// Panics when the response carries no execution ID.
fn execution_id(accepted: &Value) -> ExecutionId {
    serde_json::from_value(accepted["execution_id"].clone()).expect("execution_id is an ID")
}

/// GET one path as `jwt`.
///
/// # Panics
/// Panics when the request cannot be built or the route fails to respond.
async fn get(server: &WyrdTestServer, jwt: &str, uri: &str) -> (StatusCode, Value) {
    let response = server
        .oneshot_authenticated(
            jwt,
            Request::builder()
                .method(Method::GET)
                .uri(uri)
                .body(Body::empty())
                .expect("read request builds"),
        )
        .await
        .expect("read request responds");
    decoded(response).await
}

/// Read `execution` as `jwt` once the queue writer has accepted it.
///
/// Enqueue answers before Scribe acknowledges the inputs and the batched
/// writer accepts the submission, so the status read answers not found until
/// then; this polls through that window.
///
/// # Panics
/// Panics when the execution is not readable within [`VISIBLE`] or a read
/// answers anything but OK or the execution-not-found refusal.
async fn visible_execution(server: &WyrdTestServer, jwt: &str, execution: ExecutionId) -> Value {
    let uri = format!("/v1/verification/executions/{execution}");
    let deadline = tokio::time::Instant::now() + VISIBLE;
    loop {
        let (status, body) = get(server, jwt, &uri).await;
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

/// Split a response into its status and JSON body.
///
/// # Panics
/// Panics when the body cannot be read or is not JSON.
async fn decoded(response: Response<Body>) -> (StatusCode, Value) {
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("response body reads");
    (
        status,
        serde_json::from_slice(&bytes).expect("response body is JSON"),
    )
}

/// Bootstrap a user holding `roles` and return its principal ID and access token.
///
/// # Panics
/// Panics when bootstrapping fails or returns a machine principal.
async fn user(server: &WyrdTestServer, name: &str, roles: &[&str]) -> (Uuid, String) {
    match server
        .bootstrap_user(name, roles)
        .await
        .expect("user bootstraps")
    {
        Bootstrap::User { id, jwt } => (id.as_uuid(), jwt),
        Bootstrap::Machine { .. } => panic!("user bootstrap returned a machine principal"),
    }
}

/// Exchange a bootstrapped machine principal's API key for an access token
/// rooted at the Card it acts as.
///
/// # Panics
/// Panics when the principal has no API key or Card UID, or the exchange
/// fails.
async fn machine_jwt(server: &WyrdTestServer, machine: &Bootstrap) -> String {
    server
        .exchange_api_key_for_workload(
            machine.api_key().expect("machine has an API key"),
            machine
                .card_ref()
                .and_then(|card| card.uid.as_ref())
                .expect("machine acts as a Card UID"),
        )
        .await
        .expect("rooted exchange succeeds")
}

/// Read one execution's frozen (origin, owner, binding, requester) identity
/// columns.
///
/// # Panics
/// Panics when the execution cannot be read.
async fn execution_identity(
    server: &WyrdTestServer,
    execution: ExecutionId,
) -> (String, Option<Uuid>, Option<Uuid>, Option<Uuid>) {
    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let row = sqlx::query_as(
        "SELECT origin, owner_card_uid, binding_id, sub_principal_id \
           FROM wyrd.verifier_executions WHERE execution_id = $1",
    )
    .bind(execution.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("execution identity reads");
    conn.commit().await.expect("identity read commits");
    row
}

/// List `principal`'s retained `operation` decisions as (permission, outcome),
/// sorted by outcome.
///
/// # Panics
/// Panics when the Scribe outbox does not settle or retained audit history
/// cannot be read.
async fn decisions(
    server: &WyrdTestServer,
    operation: &str,
    principal: Uuid,
) -> Vec<(String, String)> {
    server
        .await_audit_retained()
        .await
        .expect("audit is retained");
    let mut rows: Vec<(String, String)> = server
        .retained_audit_records(
            server.data_tenant_id(),
            "permission, outcome",
            &format!("operation = '{operation}' AND sub_principal_id = '{principal}'"),
        )
        .await
        .expect("retained audit reads")
        .into_iter()
        .map(|record| {
            let mut fields = record.into_iter().map(Option::unwrap_or_default);
            (
                fields.next().unwrap_or_default(),
                fields.next().unwrap_or_default(),
            )
        })
        .collect();
    rows.sort_by(|left, right| left.1.cmp(&right.1));
    rows
}

/// One allowed or denied `verifier:run` decision.
fn run_decision(outcome: &str) -> (String, String) {
    ("verifier:run".to_owned(), outcome.to_owned())
}

/// Enqueued executions are accepted under the caller's authority, never the
/// binding owner's: a keyed retry shares its execution, a key reused for a
/// different body gets its own execution that settles with the idempotency
/// conflict, unkeyed requests never deduplicate, and the binding and
/// execution status reads return the frozen identities.
///
/// # Panics
/// Panics when the server fails to start, a fixture write fails, a route fails
/// to respond, or any status, identity, replay, or audit expectation fails.
#[tokio::test(flavor = "current_thread")]
async fn enqueued_executions_replay_conflict_and_read_back() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let fixture = Fixture::seed(&server).await;
    let (requester, jwt) = user(&server, "vr-writer", &["editor"]).await;

    let body = fixture.request(&fixture.assertion, json!(1));
    let (status, accepted) = enqueue(&server, &jwt, &body, Some("vr-retry-0001")).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{accepted}");
    let keyed = execution_id(&accepted);
    let (status, replay) = enqueue(&server, &jwt, &body, Some("vr-retry-0001")).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{replay}");
    assert_eq!(execution_id(&replay), keyed, "same key and body replays");
    let other = fixture.request(&fixture.assertion, json!(2));
    let (status, conflicting) = enqueue(&server, &jwt, &other, Some("vr-retry-0001")).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{conflicting}");
    let conflict = execution_id(&conflicting);
    assert_ne!(
        conflict, keyed,
        "conflicting content gets its own execution"
    );
    let (_, first) = enqueue(&server, &jwt, &body, None).await;
    let (_, second) = enqueue(&server, &jwt, &body, None).await;
    assert_ne!(
        execution_id(&first),
        execution_id(&second),
        "no key, no deduplication"
    );

    let execution = visible_execution(&server, &jwt, keyed).await;
    assert_eq!(execution["execution_id"], keyed.to_string());
    assert_eq!(execution["status"], "pending", "{execution}");
    assert_eq!(execution.get("result"), None, "{execution}");
    assert_eq!(execution["dispatches"], json!([]));
    assert_eq!(
        execution_identity(&server, keyed).await,
        ("manual".to_owned(), None, None, Some(requester))
    );
    let refused = visible_execution(&server, &jwt, conflict).await;
    assert_eq!(refused["status"], "errored", "{refused}");
    assert_eq!(
        refused["error"]["code"], "WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT",
        "{refused}"
    );
    for accepted in [&first, &second] {
        visible_execution(&server, &jwt, execution_id(accepted)).await;
    }
    assert_eq!(fixture.queued().await, 4, "the replay added no execution");

    let (status, binding) = get(
        &server,
        &jwt,
        &format!("/v1/verification/bindings/{}", fixture.ready_binding),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{binding}");
    assert_eq!(binding["owner_card_uid"], fixture.service.to_string());
    assert_eq!(binding["card_uid"], fixture.service.to_string());
    assert_eq!(binding["verifier_card_uid"], fixture.drift.to_string());
    assert_eq!(binding["readiness"], "ready");
    assert_eq!(binding["active"], false);
    let (status, unready) = get(
        &server,
        &jwt,
        &format!("/v1/verification/bindings/{}", fixture.unready_binding),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{unready}");
    assert_eq!(unready["readiness"], "baseline_not_ready");

    let decisions = decisions(&server, "verification.enqueue", requester).await;
    assert_eq!(
        decisions,
        vec![run_decision("allowed"); 5],
        "one per request"
    );
    server.shutdown().await.expect("test server shuts down");
}

/// Count the tenant's retained `vala.verification.execution_inputs` rows,
/// in total and for `execution`, after the Scribe outbox settles.
///
/// # Panics
/// Panics when staged writes do not publish or the rows cannot be read.
async fn input_rows(server: &WyrdTestServer, execution: ExecutionId) -> (usize, usize) {
    server.flush_bifrost().await.expect("inputs publish");
    let rows = server
        .retained_records(
            server.data_tenant_id(),
            "SELECT execution_id FROM vala.verification.execution_inputs".to_owned(),
        )
        .await
        .expect("inputs rows read");
    let mine = rows
        .iter()
        .filter(|row| row.first().cloned().flatten() == Some(execution.to_string()))
        .count();
    (rows.len(), mine)
}

/// A keyed enqueue refused for a full Scribe staging share is refused with
/// typed retryable pressure before anything is staged; once the share drains,
/// the identical keyed request is accepted, its replay returns the same
/// execution, and the tenant holds one execution and one inputs row per
/// accepted request, none for the refused attempt.
///
/// Scribe ingest is stalled so the two unkeyed enqueues ahead of it stay
/// unwritten and hold the whole reserved share (two of three slots)
/// deterministically.
///
/// Both counts are ordered after the requests before them through
/// [`Fixture::queued_before_sentinel`], whose first sentinel remains a
/// queued execution with one inputs row.
///
/// # Panics
/// Panics when the server fails to start, a fixture write fails, a route fails
/// to respond, or any status, code, replay, queue, or inputs expectation fails.
#[tokio::test(flavor = "current_thread")]
async fn staging_refusal_recovers_the_same_enqueue_identity() {
    let server = WyrdTestServer::builder()
        .with_scribe_outbox_capacity_for_test(3)
        .start_in_process()
        .await
        .expect("test server starts");
    let fixture = Fixture::seed(&server).await;
    let (_, jwt) = user(&server, "vr-pressure", &["editor"]).await;
    server.flush_bifrost().await.expect("setup settles");

    let stall = server.stall_next_bifrost_write().expect("Scribe stalls");
    let body = fixture.request(&fixture.assertion, json!(1));
    let (status, ahead) = enqueue(&server, &jwt, &body, None).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{ahead}");
    stall.wait_entered().await;
    let (status, behind) = enqueue(&server, &jwt, &body, None).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{behind}");
    let (status, refused) = enqueue(&server, &jwt, &body, Some("vr-pressure-0001")).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{refused}");
    assert_eq!(refused["code"], "WYRD_VERIFICATION_429_STAGING_FULL");
    assert_eq!(refused["details"]["retry_after_seconds"], json!(1));

    stall.release();
    stall.wait_completed().await;
    server.flush_bifrost().await.expect("the share drains");
    assert_eq!(
        fixture.queued_before_sentinel(&server, &jwt).await,
        2,
        "the refusal queued nothing"
    );
    let (status, accepted) = enqueue(&server, &jwt, &body, Some("vr-pressure-0001")).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{accepted}");
    let recovered = execution_id(&accepted);
    assert_eq!(
        input_rows(&server, recovered).await,
        (4, 1),
        "one inputs row per accepted request and the sentinel, none for the refused attempt"
    );
    let (status, replay) = enqueue(&server, &jwt, &body, Some("vr-pressure-0001")).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{replay}");
    assert_eq!(execution_id(&replay), recovered, "the retry replays");
    assert_eq!(
        fixture.queued_before_sentinel(&server, &jwt).await,
        4,
        "the two fills, the first sentinel, and the recovered call; the replay added none"
    );
    server.shutdown().await.expect("test server shuts down");
}

/// Malformed and oversized bodies, an invalid Idempotency-Key, a caller
/// without `verifier:run`, and a caller holding only an unrelated `operators:invoke` are refused
/// with stable codes before anything is staged, each authorization refusal
/// auditing exactly one `verifier:run` denial; unknown and malformed
/// execution reads are refused with their stable codes.
///
/// # Panics
/// Panics when the server fails to start, a fixture write fails, a route fails
/// to respond, or any status, code, queue, or audit expectation fails.
#[tokio::test(flavor = "current_thread")]
async fn enqueue_refusals_fail_before_staging() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let fixture = Fixture::seed(&server).await;
    let (_, jwt) = user(&server, "vr-refused-writer", &["editor"]).await;
    let body = fixture.request(&fixture.assertion, json!(1));

    let mut unknown = body.clone();
    unknown["x"] = json!(1);
    let cases = [
        (
            "{".to_owned(),
            StatusCode::BAD_REQUEST,
            "WYRD_VERIFICATION_400_INPUT_INVALID",
        ),
        (
            unknown.to_string(),
            StatusCode::BAD_REQUEST,
            "WYRD_VERIFICATION_400_INPUT_INVALID",
        ),
        (
            " ".repeat(1024 * 1024 + 1),
            StatusCode::PAYLOAD_TOO_LARGE,
            "WYRD_VERIFICATION_413_INPUT_TOO_LARGE",
        ),
    ];
    for (raw, status, code) in cases {
        let (actual, problem) = post(&server, &jwt, "/v1/verification/enqueue", raw, None).await;
        assert_eq!(
            (actual, problem["code"].as_str()),
            (status, Some(code)),
            "{problem}"
        );
    }
    let (status, problem) = enqueue(&server, &jwt, &body, Some("bad key")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{problem}");
    assert_eq!(problem["code"], "WYRD_SPEC_400_VALIDATION");

    let (reader, reader_jwt) = user(&server, "vr-reader", &["viewer"]).await;
    let (status, problem) = enqueue(&server, &reader_jwt, &body, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
    assert_eq!(problem["code"], "WYRD_PERMISSION_403_DENIED_RBAC");
    assert_eq!(
        decisions(&server, "verification.enqueue", reader).await,
        vec![run_decision("denied")]
    );
    server
        .seed_role("vr_operators_only", &[Permission::operator_invoke()])
        .await
        .expect("role seeds");
    let (operators_only, operators_only_jwt) =
        user(&server, "vr-operators-only-user", &["vr_operators_only"]).await;
    let (status, problem) = enqueue(&server, &operators_only_jwt, &body, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
    assert_eq!(problem["code"], "WYRD_PERMISSION_403_DENIED_RBAC");
    assert_eq!(
        decisions(&server, "verification.enqueue", operators_only).await,
        vec![run_decision("denied")]
    );

    let (status, problem) = get(
        &server,
        &jwt,
        &format!("/v1/verification/executions/{}", Uuid::now_v7()),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{problem}");
    assert_eq!(problem["code"], "WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND");
    let (status, problem) = get(&server, &jwt, "/v1/verification/executions/not-a-uuid").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{problem}");
    assert_eq!(
        fixture.queued_before_sentinel(&server, &jwt).await,
        0,
        "no refusal staged a submission"
    );
    server.shutdown().await.expect("test server shuts down");
}

/// A rooted caller may enqueue only a subject its signed Card scope carries:
/// holding `verifier:run` on exactly the selected Verifier, the subject
/// family's runtime account rooted at the subject is accepted, while an
/// unrelated Service rooted at its own Card, and an editor Service acting on
/// its behalf, are refused with one audited denial each before anything is
/// staged. A rootless caller is decided by RBAC alone.
///
/// # Panics
/// Panics when the server fails to start, a fixture write fails, a route fails
/// to respond, or any status, code, queue, or audit expectation fails.
#[tokio::test(flavor = "current_thread")]
async fn enqueue_admits_only_signed_scope_subjects() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let fixture = Fixture::seed(&server).await;
    let body = fixture.request(&fixture.assertion, json!(1));
    server
        .seed_role(
            "vq_exact_verifier",
            &[
                Permission::card_read(),
                Permission::verifier_run(fixture.assertion.clone()),
            ],
        )
        .await
        .expect("role seeds");

    let owner = server
        .credential_registered_service(&fixture.subject_ref(), &["vq_exact_verifier"])
        .await
        .expect("subject Service is credentialed");
    let owner_jwt = machine_jwt(&server, &owner).await;
    let (status, accepted) = enqueue(&server, &owner_jwt, &body, None).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{accepted}");
    visible_execution(&server, &owner_jwt, execution_id(&accepted)).await;
    assert_eq!(
        decisions(&server, "verification.enqueue", owner.id().as_uuid()).await,
        vec![run_decision("allowed")]
    );

    let foreign = server
        .bootstrap_service("vq-foreign-service", &["vq_exact_verifier"])
        .await
        .expect("unrelated Card-bound Service bootstraps");
    let foreign_jwt = machine_jwt(&server, &foreign).await;
    let (status, problem) = enqueue(&server, &foreign_jwt, &body, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
    assert_eq!(problem["code"], "WYRD_PERMISSION_403_DENIED_RBAC");

    let actor = server
        .bootstrap_service("vq-editor-actor", &["editor"])
        .await
        .expect("editor actor bootstraps");
    let delegated = server
        .delegate(
            &foreign_jwt,
            &machine_jwt(&server, &actor).await,
            TokenAudience::Wyrd,
        )
        .await
        .expect("editor acts on the unrelated Service's behalf");
    let (status, problem) = enqueue(&server, &delegated, &body, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
    assert_eq!(problem["code"], "WYRD_PERMISSION_403_DENIED_RBAC");
    assert_eq!(
        decisions(&server, "verification.enqueue", foreign.id().as_uuid()).await,
        vec![run_decision("denied"); 2],
        "the direct and the delegated refusal"
    );

    let (_, editor_jwt) = user(&server, "vq-editor", &["editor"]).await;
    let (status, accepted) = enqueue(&server, &editor_jwt, &body, None).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{accepted}");
    visible_execution(&server, &editor_jwt, execution_id(&accepted)).await;
    assert_eq!(fixture.queued().await, 2, "no refusal staged a submission");
    server.shutdown().await.expect("test server shuts down");
}

/// A Verifier judging one uploaded JSON document: its extracted `answer`
/// must equal `1`.
fn document_answer_is_one() -> Value {
    json!({
        "inputs": { "doc": { "type": "media", "formats": ["json"] } },
        "steps": [{
            "id": "read",
            "action": { "kind": "extract", "from": "inputs.doc" },
            "requirements": [{
                "id": "answer-is-one",
                "statement": "The document's answer is one.",
                "check": { "kind": "assert", "path": "steps.read.output.value.answer",
                           "operator": "equals", "expected": 1 }
            }]
        }]
    })
}

/// PUT `body` to the local evidence route for `sha256` as `jwt`.
///
/// # Panics
/// Panics when the request cannot be built or the route fails to respond.
async fn put_evidence(
    server: &WyrdTestServer,
    jwt: &str,
    sha256: &str,
    body: &'static [u8],
) -> StatusCode {
    let request = Request::builder()
        .method(Method::PUT)
        .uri(format!("/v1/verification/evidence/local/{sha256}"))
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .body(Body::from(body))
        .expect("request builds");
    server
        .oneshot_authenticated(jwt, request)
        .await
        .expect("request responds")
        .status()
}

/// Evidence staging is admitted by any `verifier:run` grant: a caller holding
/// it for exactly one Verifier plans and uploads a missing local file, sees it
/// held on a second plan, then verifies and submits it with that Verifier,
/// while another Verifier stays refused. A caller with no `verifier:run`
/// grant is refused with one audited denial on each staging route, and a body
/// of another digest is refused.
///
/// # Panics
/// Panics when the server fails to start, a fixture write fails, a route fails
/// to respond, or any status, code, plan, verdict, or audit expectation fails.
#[tokio::test(flavor = "current_thread")]
async fn exact_verifier_grant_stages_local_evidence() {
    const DOCUMENT: &[u8] = br#"{"answer":1}"#;
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let fixture = Fixture::seed(&server).await;
    let document = fixture
        .seed
        .verifier("vr-document", &document_answer_is_one())
        .await
        .expect("document Verifier registers");
    server
        .seed_role(
            "ve_exact_verifier",
            &[
                Permission::card_read(),
                Permission::verifier_run(document.clone()),
            ],
        )
        .await
        .expect("role seeds");
    let (exact, exact_jwt) = user(&server, "ve-exact", &["ve_exact_verifier"]).await;
    let sha256 = wyrd_spec::auth::Sha256Hex::digest(DOCUMENT).to_string();
    let plan = json!({ "files": [{ "sha256": sha256, "size_bytes": DOCUMENT.len() }] });

    let (status, planned) = send(
        &server,
        &exact_jwt,
        Method::POST,
        "/v1/verification/evidence",
        Some(&plan),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{planned}");
    assert_eq!(
        planned["uploads"].as_array().map(Vec::len),
        Some(1),
        "{planned}"
    );
    assert_eq!(
        put_evidence(&server, &exact_jwt, &sha256, br#"{"answer":2}"#).await,
        StatusCode::BAD_REQUEST,
        "a body of another digest is refused"
    );
    assert_eq!(
        put_evidence(&server, &exact_jwt, &sha256, DOCUMENT).await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        put_evidence(&server, &exact_jwt, &sha256, DOCUMENT).await,
        StatusCode::NO_CONTENT
    );
    let (status, held) = send(
        &server,
        &exact_jwt,
        Method::POST,
        "/v1/verification/evidence",
        Some(&plan),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{held}");
    assert_eq!(held["uploads"], json!([]), "the uploaded file is held");

    let request = |verifier: &CardUid| {
        json!({
            "verifier_card_uid": verifier,
            "card_uid": fixture.service,
            "inputs": { "doc": { "kind": "file", "sha256": sha256,
                                 "size_bytes": DOCUMENT.len(), "format": "json" } }
        })
    };
    let (status, judged) = execute(&server, &exact_jwt, request(&document).to_string()).await;
    assert_eq!(status, StatusCode::OK, "{judged}");
    assert_eq!(judged["outcome"]["verdict"], "passed", "{judged}");
    let (status, accepted) = enqueue(&server, &exact_jwt, &request(&document), None).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{accepted}");
    let (status, problem) =
        execute(&server, &exact_jwt, request(&fixture.assertion).to_string()).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
    let allowed = vec![run_decision("allowed"); 2];
    assert_eq!(
        decisions(&server, "verification.evidence", exact).await,
        allowed
    );
    assert_eq!(
        decisions(&server, "verification.evidence.upload", exact).await,
        vec![run_decision("allowed"); 3]
    );

    let (outsider, outsider_jwt) = user(&server, "ve-outsider", &["viewer"]).await;
    let (status, problem) = send(
        &server,
        &outsider_jwt,
        Method::POST,
        "/v1/verification/evidence",
        Some(&plan),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
    assert_eq!(problem["code"], "WYRD_PERMISSION_403_DENIED_RBAC");
    assert_eq!(
        put_evidence(&server, &outsider_jwt, &sha256, DOCUMENT).await,
        StatusCode::FORBIDDEN
    );
    let denied = vec![run_decision("denied")];
    assert_eq!(
        decisions(&server, "verification.evidence", outsider).await,
        denied
    );
    assert_eq!(
        decisions(&server, "verification.evidence.upload", outsider).await,
        denied
    );
    server.shutdown().await.expect("test server shuts down");
}

/// Binding and execution status reads each authorize and audit `cards:read`:
/// a reader is allowed and stages one allowed decision per read, while a
/// principal whose only role lacks `cards:read` is refused with the stable
/// RBAC code and stages one denied decision per read, before any lookup.
///
/// # Panics
/// Panics when the server fails to start, a fixture write fails, a route fails
/// to respond, or any status, code, or audit expectation fails.
#[tokio::test(flavor = "current_thread")]
async fn status_reads_audit_cards_read_decisions() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let fixture = Fixture::seed(&server).await;
    let (_, writer) = user(&server, "vr-status-writer", &["editor"]).await;
    let (status, accepted) = enqueue(
        &server,
        &writer,
        &fixture.request(&fixture.assertion, json!(1)),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{accepted}");
    let execution = execution_id(&accepted);
    visible_execution(&server, &writer, execution).await;
    let binding_uri = format!("/v1/verification/bindings/{}", fixture.ready_binding);
    let execution_uri = format!("/v1/verification/executions/{execution}");

    let (reader, reader_jwt) = user(&server, "vr-status-reader", &["viewer"]).await;
    for uri in [&binding_uri, &execution_uri] {
        let (status, body) = get(&server, &reader_jwt, uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
    }
    let allowed = vec![("cards:read".to_owned(), "allowed".to_owned())];
    assert_eq!(
        decisions(&server, "verification.binding.read", reader).await,
        allowed
    );
    assert_eq!(
        decisions(&server, "verification.execution.read", reader).await,
        allowed
    );

    let (outsider, outsider_jwt) = user(&server, "vr-status-outsider", &[]).await;
    for uri in [&binding_uri, &execution_uri] {
        let (status, problem) = get(&server, &outsider_jwt, uri).await;
        assert_eq!(
            (status, problem["code"].as_str()),
            (
                StatusCode::FORBIDDEN,
                Some("WYRD_PERMISSION_403_DENIED_RBAC")
            ),
            "{uri}"
        );
    }
    let denied = vec![("cards:read".to_owned(), "denied".to_owned())];
    assert_eq!(
        decisions(&server, "verification.binding.read", outsider).await,
        denied
    );
    assert_eq!(
        decisions(&server, "verification.execution.read", outsider).await,
        denied
    );
    server.shutdown().await.expect("test server shuts down");
}

/// Another tenant's rootless administrator, which Card admission lets
/// attribute to any Card it can find, cannot read this tenant's binding or
/// execution, nor execute its Verifier: each is indistinguishable from an
/// absent one.
///
/// # Panics
/// Panics when the server fails to start, a fixture write fails, a route fails
/// to respond, or any status, code, or queue expectation fails.
#[tokio::test(flavor = "current_thread")]
async fn verification_state_is_tenant_isolated() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let fixture = Fixture::seed(&server).await;
    let (_, jwt) = user(&server, "vr-home-writer", &["editor"]).await;
    let body = fixture.request(&fixture.assertion, json!(1));
    let (status, accepted) = enqueue(&server, &jwt, &body, None).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{accepted}");
    let execution = execution_id(&accepted);
    visible_execution(&server, &jwt, execution).await;

    let other: DataTenantId = server.seed_tenant("vr-other").await.expect("tenant seeds");
    let foreign = server
        .bootstrap_service_in_tenant(other, "vr-other-admin", &["admin"])
        .await
        .expect("foreign admin bootstraps");
    let foreign_jwt = server
        .exchange_api_key(foreign.api_key().expect("foreign admin has an API key"))
        .await
        .expect("rootless exchange succeeds");
    let reads = [
        (
            format!("/v1/verification/bindings/{}", fixture.ready_binding),
            "WYRD_VERIFICATION_404_BINDING_NOT_FOUND",
        ),
        (
            format!("/v1/verification/executions/{execution}"),
            "WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND",
        ),
    ];
    for (uri, code) in reads {
        let (status, problem) = get(&server, &foreign_jwt, &uri).await;
        assert_eq!(
            (status, problem["code"].as_str()),
            (StatusCode::NOT_FOUND, Some(code)),
            "{uri}"
        );
    }
    let (status, problem) = execute(&server, &foreign_jwt, body.to_string()).await;
    assert_eq!(
        (status, problem["code"].as_str()),
        (
            StatusCode::NOT_FOUND,
            Some("WYRD_VERIFICATION_404_TARGET_NOT_FOUND")
        ),
        "{problem}"
    );
    assert_eq!(fixture.queued().await, 1);
    server.shutdown().await.expect("test server shuts down");
}

/// Direct execution judges the assertion Verifier's inline evidence and
/// returns the exact identities and verdict: a matching answer passes and a
/// different one fails. Nothing is enqueued and each request audits exactly
/// one allowed `verifier:run` decision.
///
/// # Panics
/// Panics when the server fails to start, a fixture write fails, a route fails
/// to respond, or any verdict, identity, queue, or audit expectation fails.
#[tokio::test(flavor = "current_thread")]
async fn direct_execution_judges_inline_without_enqueueing() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let fixture = Fixture::seed(&server).await;
    let (caller, jwt) = user(&server, "vx-writer", &["editor"]).await;

    for (answer, verdict) in [(json!(1), "passed"), (json!(2), "failed")] {
        let body = fixture.request(&fixture.assertion, answer);
        let (status, response) = execute(&server, &jwt, body.to_string()).await;
        assert_eq!(status, StatusCode::OK, "{body} -> {response}");
        assert_eq!(response["outcome"]["verdict"], verdict, "{response}");
        assert_eq!(
            response["outcome"]["requirements"][0]["id"], "answer-is-one",
            "{response}"
        );
        assert_eq!(response["verifier"]["uid"], fixture.assertion.to_string());
        assert_eq!(response["subject"]["uid"], fixture.service.to_string());
        assert!(response["execution_id"].is_string(), "{response}");
    }
    assert_eq!(
        decisions(&server, "verification.execute", caller).await,
        vec![run_decision("allowed"); 2],
        "one decision per request"
    );
    assert_eq!(fixture.queued_before_sentinel(&server, &jwt).await, 0);
    server.shutdown().await.expect("test server shuts down");
}

/// Malformed, oversized, unknown, and non-Verifier executions and a Drift
/// Verifier, which judges only scheduled windows, are refused with stable
/// codes. A caller without `verifier:run`, holding only an unrelated `operators:invoke`, or
/// granted only an unrelated Verifier UID is refused with one audited denial,
/// while the subject's runtime account granted exactly that Verifier UID is
/// allowed; nothing is ever enqueued.
///
/// # Panics
/// Panics when the server fails to start, a fixture write fails, a route fails
/// to respond, or any status, code, queue, or audit expectation fails.
#[tokio::test(flavor = "current_thread")]
async fn direct_execution_refusals_are_stable() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let fixture = Fixture::seed(&server).await;
    let (_, jwt) = user(&server, "vx-refused-writer", &["editor"]).await;
    let allowed = fixture.request(&fixture.assertion, json!(1));
    let mut unknown = allowed.clone();
    unknown["x"] = json!(1);
    let unregistered = CardUid::from_uuid(Uuid::now_v7()).expect("UID is valid");

    let cases = [
        (
            "{".to_owned(),
            StatusCode::BAD_REQUEST,
            "WYRD_VERIFICATION_400_INPUT_INVALID",
        ),
        (
            unknown.to_string(),
            StatusCode::BAD_REQUEST,
            "WYRD_VERIFICATION_400_INPUT_INVALID",
        ),
        (
            " ".repeat(1024 * 1024 + 1),
            StatusCode::PAYLOAD_TOO_LARGE,
            "WYRD_VERIFICATION_413_INPUT_TOO_LARGE",
        ),
        (
            fixture.request(&unregistered, json!(1)).to_string(),
            StatusCode::NOT_FOUND,
            "WYRD_VERIFICATION_404_TARGET_NOT_FOUND",
        ),
        (
            fixture.request(&fixture.service, json!(1)).to_string(),
            StatusCode::NOT_FOUND,
            "WYRD_VERIFICATION_404_TARGET_NOT_FOUND",
        ),
        (
            fixture.request(&fixture.drift, json!(1)).to_string(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "WYRD_VERIFICATION_422_INPUT_UNSUPPORTED",
        ),
    ];
    for (body, status, code) in cases {
        let (actual, problem) = execute(&server, &jwt, body.clone()).await;
        assert_eq!(
            (actual, problem["code"].as_str()),
            (status, Some(code)),
            "{} -> {problem}",
            &body[..body.len().min(200)]
        );
    }

    let allowed = allowed.to_string();
    let (reader, reader_jwt) = user(&server, "vx-reader", &["viewer"]).await;
    let (status, problem) = execute(&server, &reader_jwt, allowed.clone()).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
    assert_eq!(problem["code"], "WYRD_PERMISSION_403_DENIED_RBAC");
    assert_eq!(
        decisions(&server, "verification.execute", reader).await,
        vec![run_decision("denied")]
    );
    for (role, permission) in [
        ("vx_operators_only", Permission::operator_invoke()),
        (
            "vx_other_verifier",
            Permission::verifier_run(fixture.drift.clone()),
        ),
    ] {
        server
            .seed_role(role, &[Permission::card_read(), permission])
            .await
            .expect("role seeds");
        let (denied, denied_jwt) = user(&server, &format!("{role}-user"), &[role]).await;
        let (status, problem) = execute(&server, &denied_jwt, allowed.clone()).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{role}: {problem}");
        assert_eq!(problem["code"], "WYRD_PERMISSION_403_DENIED_RBAC");
        assert_eq!(
            decisions(&server, "verification.execute", denied).await,
            vec![run_decision("denied")],
            "{role}"
        );
    }
    server
        .seed_role(
            "vx_exact_verifier",
            &[Permission::verifier_run(fixture.assertion.clone())],
        )
        .await
        .expect("role seeds");
    let machine = server
        .credential_registered_service(&fixture.subject_ref(), &["vx_exact_verifier"])
        .await
        .expect("subject Service is credentialed");
    let machine_token = machine_jwt(&server, &machine).await;
    let (status, judged) = execute(&server, &machine_token, allowed).await;
    assert_eq!(status, StatusCode::OK, "{judged}");
    assert_eq!(judged["outcome"]["verdict"], "passed", "{judged}");
    assert_eq!(
        decisions(&server, "verification.execute", machine.id().as_uuid()).await,
        vec![run_decision("allowed")]
    );
    assert_eq!(fixture.queued_before_sentinel(&server, &jwt).await, 0);
    server.shutdown().await.expect("test server shuts down");
}

/// Direct execution admits the verified subject through the caller's signed
/// Card limit before any input is staged or the engine runs, exactly as
/// enqueue does: the subject's runtime account rooted at the subject judges
/// it, while the same account without a root is refused as needing workload
/// context, and an unrelated Service rooted at its own Card is refused as out
/// of scope, each with one audited denial. Rootless callers that may attribute
/// to any registered Card, an editor user and an unrestricted Service, still
/// judge it.
///
/// # Panics
/// Panics when the server fails to start, a fixture write fails, a route fails
/// to respond, or any status, code, verdict, input, queue, or audit
/// expectation fails.
#[tokio::test(flavor = "current_thread")]
async fn direct_execution_enforces_card_access() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let fixture = Fixture::seed(&server).await;
    let body = fixture.request(&fixture.assertion, json!(1)).to_string();
    let owner = server
        .credential_registered_service(&fixture.subject_ref(), &["editor"])
        .await
        .expect("subject Service is credentialed");
    let rootless_owner = server
        .exchange_api_key(owner.api_key().expect("owner has an API key"))
        .await
        .expect("rootless exchange succeeds");
    let foreign = server
        .bootstrap_service("vx-scope-foreign", &["editor"])
        .await
        .expect("unrelated Card-bound Service bootstraps");
    let unrestricted = server
        .exchange_api_key(foreign.api_key().expect("foreign Service has an API key"))
        .await
        .expect("rootless exchange succeeds");
    let (_, editor) = user(&server, "vx-scope-editor", &["editor"]).await;

    let refusals = [
        (&rootless_owner, "WYRD_AUTH_403_WORKLOAD_CONTEXT_REQUIRED"),
        (
            &machine_jwt(&server, &foreign).await,
            "WYRD_PERMISSION_403_DENIED_RBAC",
        ),
    ];
    for (jwt, code) in refusals {
        let (status, problem) = execute(&server, jwt, body.clone()).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
        assert_eq!(problem["code"], code, "{problem}");
    }
    let (staged, _) = input_rows(&server, ExecutionId::new_v7()).await;
    assert_eq!(staged, 0, "a refused execution stages no inputs");

    for jwt in [machine_jwt(&server, &owner).await, unrestricted, editor] {
        let (status, judged) = execute(&server, &jwt, body.clone()).await;
        assert_eq!(status, StatusCode::OK, "{judged}");
        assert_eq!(judged["outcome"]["verdict"], "passed", "{judged}");
    }
    assert_eq!(
        decisions(&server, "verification.execute", owner.id().as_uuid()).await,
        vec![run_decision("allowed"), run_decision("denied")]
    );
    assert_eq!(
        decisions(&server, "verification.execute", foreign.id().as_uuid()).await,
        vec![run_decision("allowed"), run_decision("denied")],
        "the rooted refusal and the rootless judgment"
    );
    assert_eq!(fixture.queued().await, 0, "direct execution never enqueues");
    server.shutdown().await.expect("test server shuts down");
}

/// Evidence that does not conform to the selected Verifier's declared slots
/// is refused by the server before any block runs: an undeclared slot, a
/// declaration of another kind, a JSON value violating its slot schema, a
/// file in a format its media slot refuses, a file above the slot's
/// `max_bytes`, an unused file in an uncapped slot above the individual
/// input-file limit but within the combined one, and files whose declared
/// lengths together exceed the combined input-file limit. Conforming evidence still passes, an omitted
/// optional slot reads as null, and a missing required slot stays
/// inconclusive rather than refused. Nothing is enqueued.
///
/// # Panics
/// Panics when the server fails to start, a fixture write fails, a route
/// fails to respond, or a status, code, verdict, or queue expectation fails.
#[tokio::test(flavor = "current_thread")]
async fn direct_execution_refuses_nonconforming_evidence() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let fixture = Fixture::seed(&server).await;
    let (_, jwt) = user(&server, "vx-conform-writer", &["editor"]).await;
    let typed = fixture
        .seed
        .verifier(
            "vx-typed",
            &json!({
                "inputs": {
                    "x": { "type": "json", "schema": { "type": "integer" } },
                    "note": { "type": "json", "required": false },
                    "policy": { "type": "media", "formats": ["pdf"], "max_bytes": 10, "required": false },
                    "pages": { "type": "media", "formats": ["pdf"], "required": false }
                },
                "requirements": [
                    { "id": "x-present", "statement": "x is present.",
                      "check": { "kind": "assert", "path": "inputs.x", "operator": "is_not_null" } },
                    { "id": "no-note", "statement": "No note.",
                      "check": { "kind": "assert", "path": "inputs.note", "operator": "is_null" } }
                ]
            }),
        )
        .await
        .expect("typed Verifier registers");
    let body = |inputs: Value| {
        json!({ "verifier_card_uid": typed, "card_uid": fixture.service, "inputs": inputs })
            .to_string()
    };
    let file = |format: &str, size_bytes: u64| json!({ "kind": "file", "sha256": "ab".repeat(32), "size_bytes": size_bytes, "format": format });
    let x = json!({ "kind": "json", "value": 1 });
    let mib = 1024 * 1024;
    let cases = [
        (
            json!({ "x": x, "extra": x }),
            StatusCode::BAD_REQUEST,
            "WYRD_VERIFICATION_400_INPUT_INVALID",
        ),
        (
            json!({ "x": { "kind": "trace", "trace_id": "0af7651916cd43dd8448eb211c80319c" } }),
            StatusCode::BAD_REQUEST,
            "WYRD_VERIFICATION_400_INPUT_INVALID",
        ),
        (
            json!({ "x": { "kind": "json", "value": "one" } }),
            StatusCode::BAD_REQUEST,
            "WYRD_VERIFICATION_400_INPUT_INVALID",
        ),
        (
            json!({ "x": x, "policy": file("png", 5) }),
            StatusCode::BAD_REQUEST,
            "WYRD_VERIFICATION_400_INPUT_INVALID",
        ),
        (
            json!({ "x": x, "policy": file("pdf", 11) }),
            StatusCode::PAYLOAD_TOO_LARGE,
            "WYRD_VERIFICATION_413_INPUT_TOO_LARGE",
        ),
        (
            json!({ "x": x, "pages": file("pdf", 21 * mib) }),
            StatusCode::PAYLOAD_TOO_LARGE,
            "WYRD_VERIFICATION_413_INPUT_TOO_LARGE",
        ),
        (
            json!({ "x": x, "policy": file("pdf", 10), "pages": file("pdf", 200 * mib) }),
            StatusCode::PAYLOAD_TOO_LARGE,
            "WYRD_VERIFICATION_413_INPUT_TOO_LARGE",
        ),
    ];
    for (inputs, status, code) in cases {
        let (actual, problem) = execute(&server, &jwt, body(inputs.clone())).await;
        assert_eq!(
            (actual, problem["code"].as_str()),
            (status, Some(code)),
            "{inputs} -> {problem}"
        );
    }

    let (status, passed) = execute(&server, &jwt, body(json!({ "x": x }))).await;
    assert_eq!(status, StatusCode::OK, "{passed}");
    assert_eq!(passed["outcome"]["verdict"], "passed", "{passed}");
    let (status, missing) = execute(&server, &jwt, body(json!({}))).await;
    assert_eq!(status, StatusCode::OK, "{missing}");
    assert_eq!(missing["outcome"]["verdict"], "inconclusive", "{missing}");
    assert_eq!(fixture.queued_before_sentinel(&server, &jwt).await, 0);
    server.shutdown().await.expect("test server shuts down");
}

/// Send one request with an optional JSON body as `jwt`.
///
/// # Panics
/// Panics when the request cannot be built or the route fails to respond.
async fn send(
    server: &WyrdTestServer,
    jwt: &str,
    method: Method,
    uri: &str,
    body: Option<&Value>,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(body.map_or_else(Body::empty, |body| Body::from(body.to_string())))
        .expect("request builds");
    let response = server
        .oneshot_authenticated(jwt, request)
        .await
        .expect("request responds");
    decoded(response).await
}

/// Read the tenant's provisioned SYSTEM writer as (id, name, status, API-key count).
///
/// Reads through the superuser pool because every public tenant query omits
/// the SYSTEM row.
///
/// # Panics
/// Panics when the superuser pool cannot open or the tenant has no single
/// SYSTEM row.
async fn system_writer(
    server: &WyrdTestServer,
    tenant: DataTenantId,
) -> (Uuid, String, String, i64) {
    let pool = server
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    sqlx::query_as(
        "SELECT a.id, a.name, a.status, \
                (SELECT count(*) FROM wyrd.auth_api_keys k WHERE k.principal_id = a.id) \
           FROM wyrd.auth_service_accounts a \
          WHERE a.data_tenant_id = $1 AND a.principal_kind = 'system'",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&pool)
    .await
    .expect("the tenant has exactly one SYSTEM writer")
}

/// The provisioned SYSTEM writer is unreachable through every public principal
/// route: a tenant administrator's credential list, credential (API-key)
/// issuance, credential revocation, and principal revocation for its id all
/// answer the non-enumerating principal-not-found refusal, and the row stays
/// active and credentialless. The public surface has no principal list, get,
/// or update route; list omission is pinned by the SQL principal listing.
///
/// # Panics
/// Panics when the server fails to start, a route fails to respond, any route
/// reaches the SYSTEM row, or the row changes.
#[tokio::test(flavor = "current_thread")]
async fn system_writer_is_unreachable_through_public_principal_routes() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let tenant = server.data_tenant_id();
    // The fixture tenant is created after migration backfill, so provision it
    // through the one idempotent owner tenant provisioning calls.
    let mut conn = server
        .tenant_conn_for(tenant)
        .await
        .expect("tenant connection opens");
    let provisioned = wyrd_sql::queries::auth::provision_system_principal(
        &mut conn,
        wyrd_spec::auth::Attribution::unresolved(),
    )
    .await
    .expect("SYSTEM writer provisions");
    conn.commit().await.expect("provisioning commits");
    let (system, name, status, keys) = system_writer(&server, tenant).await;
    assert_eq!(system, provisioned);
    assert_eq!(
        (name.as_str(), status.as_str(), keys),
        ("verification-results-writer", "active", 0)
    );
    assert_eq!(system.get_version_num(), 7, "the SYSTEM id is UUIDv7");
    let (_, jwt) = user(&server, "vr-principal-admin", &["admin"]).await;
    let principal_missing = "WYRD_AUTH_404_PRINCIPAL_NOT_FOUND";
    let credentials = format!("/v1/principals/{system}/credentials");
    let revoke = format!("/v1/principals/{system}/revoke");
    let attempts = [
        (Method::GET, credentials.clone(), None, principal_missing),
        (Method::POST, credentials.clone(), None, principal_missing),
        (
            Method::DELETE,
            format!("{credentials}/{}", Uuid::now_v7()),
            None,
            "WYRD_SPEC_404_NOT_FOUND",
        ),
        (
            Method::POST,
            revoke.clone(),
            Some(json!({ "principal_kind": "system", "reason": "probe" })),
            principal_missing,
        ),
        (
            Method::POST,
            revoke,
            Some(json!({ "principal_kind": "service", "reason": "probe" })),
            principal_missing,
        ),
    ];
    for (method, uri, body, code) in attempts {
        let (status, problem) = send(&server, &jwt, method.clone(), &uri, body.as_ref()).await;
        assert_eq!(
            (status, problem["code"].as_str()),
            (StatusCode::NOT_FOUND, Some(code)),
            "{method} {uri} must not reach SYSTEM: {problem}"
        );
    }
    assert_eq!(
        system_writer(&server, tenant).await,
        (system, name, status, 0),
        "no public route changed or credentialed the SYSTEM writer"
    );
    server.shutdown().await.expect("test server shuts down");
}

/// A code-check Verifier spec over the inline `answer` slot whose check
/// declares `response_format`, returning a cited Verdict with `data`.
fn scored(response_format: &Value, data: &Value) -> Value {
    let source = format!(
        "def check(answer):\n    return {{\"outcome\": \"failed\", \"reason\": \"scored\", \"citations\": [{{\"slot\": \"answer\", \"locator\": {{\"json_pointer\": \"\"}}}}], \"data\": {data}}}\n"
    );
    json!({
        "inputs": { "answer": { "type": "json" } },
        "requirements": [{
            "id": "scored",
            "statement": "The answer is scored.",
            "check": {
                "kind": "code", "entrypoint": "check", "inputs": { "answer": "inputs.answer" },
                "source": { "inline": source }, "response_format": response_format
            }
        }]
    })
}

/// The `{kind: verdict}` format whose custom data is one numeric `score`.
fn score_format() -> Value {
    json!({ "kind": "verdict", "data": {
        "type": "object", "properties": { "score": { "type": "number" } },
        "required": ["score"], "additionalProperties": false
    } })
}

/// A check Agent judging `inputs.answer` with an inline Prompt answering in
/// `response_format`, or, when `native` is set, with a native Responses body
/// that also sets its own text format.
fn judged(response_format: &Value, native: bool) -> Value {
    let prompt = if native {
        json!({
            "provider": "openai", "model": "gpt-4o", "format": "responses",
            "body": {
                "input": [{ "role": "user", "content": [{ "type": "input_text", "text": "Judge {{answer}}" }] }],
                "text": { "format": { "type": "json_object" } }
            },
            "response_format": response_format
        })
    } else {
        json!({
            "provider": "openai", "model": "gpt-4o",
            "messages": [{ "role": "user", "content": "Judge {{answer}}" }],
            "response_format": response_format
        })
    };
    json!({
        "inputs": { "answer": { "type": "json" } },
        "requirements": [{
            "id": "judged",
            "statement": "The answer is judged.",
            "check": {
                "kind": "agent", "inputs": { "answer": "inputs.answer" },
                "agent": {
                    "execution_route": { "kind": "wyrd_gateway", "deployment": "openai" },
                    "run_config": { "max_iterations": 1 },
                    "prompt": prompt
                }
            }
        }]
    })
}

/// Register one Verifier named `name` with `spec` through `POST /v1/cards`
/// as `jwt`.
///
/// # Panics
/// Panics when the request cannot be built or the route fails to respond.
async fn declare_verifier(
    server: &WyrdTestServer,
    jwt: &str,
    name: &str,
    spec: &Value,
) -> (StatusCode, Value) {
    let body = json!({ "submissions": [{
        "apiVersion": "wyrd/v1", "kind": "Verifier",
        "metadata": { "name": name, "version": "1.0.0" },
        "spec": spec, "artifacts": []
    }] });
    post(
        server,
        jwt,
        "/v1/cards",
        body.to_string(),
        Some(&format!("vr-declare-{name}")),
    )
    .await
}

/// Unknown format names, invalid or non-object data schemas, data schemas
/// declaring another draft or a root `$id`, Prompts that
/// cannot produce a Verdict (including a composed handwritten schema whose
/// data is optional), and native bodies that set their own response format
/// are refused with a structured validation error naming the field,
/// and the refused Verifier does not exist afterwards; the valid code and
/// Agent declarations register.
///
/// # Panics
/// Panics when a refused declaration registers or a valid one is refused.
#[tokio::test(flavor = "current_thread")]
async fn verdict_declarations_validate_before_persistence() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let (_, jwt) = user(&server, "vr-declarer", &["editor"]).await;
    let plain = json!({ "outcome": "passed" });
    let mut optional_data = Value::Object(
        wyrd_spec::verification::execution::Verdict::response_format(
            &serde_json::from_value(json!("verdict")).expect("plain format"),
        )
        .expect("the plain format resolves"),
    );
    optional_data["properties"]["data"] = json!({ "type": "object" });
    let refused = [
        ("vr-unknown-name", scored(&json!("score"), &plain)),
        (
            "vr-scalar-data",
            scored(
                &json!({ "kind": "verdict", "data": { "type": "string" } }),
                &plain,
            ),
        ),
        (
            "vr-invalid-data",
            scored(
                &json!({ "kind": "verdict", "data": { "type": "object", "properties": { "x": { "type": "nope" } } } }),
                &plain,
            ),
        ),
        (
            "vr-not-a-verdict",
            judged(
                &json!({ "type": "object", "properties": { "score": { "type": "number" } },
                         "required": ["score"], "additionalProperties": false }),
                false,
            ),
        ),
        (
            "vr-older-draft-data",
            scored(
                &json!({ "kind": "verdict", "data": { "$schema": "http://json-schema.org/draft-04/schema#", "type": "object" } }),
                &plain,
            ),
        ),
        (
            "vr-authored-id-data",
            judged(
                &json!({ "kind": "verdict", "data": { "$id": "#", "type": "object" } }),
                false,
            ),
        ),
        ("vr-native-conflict", judged(&json!("verdict"), true)),
        (
            "vr-composed-optional-data",
            judged(
                &json!({ "type": "object", "allOf": [optional_data] }),
                false,
            ),
        ),
    ];
    for (name, spec) in refused {
        let (status, problem) = declare_verifier(&server, &jwt, name, &spec).await;
        assert!(status.is_client_error(), "{name}: {status} {problem}");
        assert!(
            problem["code"]
                .as_str()
                .is_some_and(|code| code.starts_with("WYRD_")),
            "{name}: {problem}"
        );
        assert!(
            problem.to_string().contains("response_format"),
            "{name} names the field: {problem}"
        );
        let (status, _) = get(&server, &jwt, &format!("/v1/cards/Verifier/{name}/latest")).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{name} was not persisted");
    }
    for (name, spec) in [
        (
            "vr-scored-code",
            scored(&score_format(), &json!({ "score": 1 })),
        ),
        ("vr-scored-agent", judged(&score_format(), false)),
        ("vr-plain-agent", judged(&json!("verdict"), false)),
    ] {
        let (status, body) = declare_verifier(&server, &jwt, name, &spec).await;
        assert!(status.is_success(), "{name}: {status} {body}");
        let (status, card) = get(&server, &jwt, &format!("/v1/cards/Verifier/{name}/latest")).await;
        assert_eq!(status, StatusCode::OK, "{name}: {card}");
    }
    server.shutdown().await.expect("test server shuts down");
}

/// A direct execution returns its code check's custom data to the home
/// tenant, through the execution read too, while another tenant's rootless
/// administrator can neither execute the Verifier nor read the execution.
///
/// # Panics
/// Panics when data is lost at home or any foreign read succeeds.
#[tokio::test(flavor = "current_thread")]
async fn custom_verdict_data_is_tenant_isolated() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let fixture = Fixture::seed(&server).await;
    let verifier = fixture
        .seed
        .verifier(
            "vr-scored",
            &scored(&score_format(), &json!({ "score": 0.5 })),
        )
        .await
        .expect("scored Verifier registers");
    let (_, jwt) = user(&server, "vr-scorer", &["editor"]).await;
    let body = fixture.request(&verifier, json!({ "text": "one" }));

    let (status, result) = execute(&server, &jwt, body.to_string()).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let requirement = &result["outcome"]["requirements"][0];
    assert_eq!(requirement["outcome"], "failed", "{result}");
    assert_eq!(requirement["data"], json!({ "score": 0.5 }), "{result}");

    let other: DataTenantId = server.seed_tenant("vr-other").await.expect("tenant seeds");
    let foreign = server
        .bootstrap_service_in_tenant(other, "vr-other-admin", &["admin"])
        .await
        .expect("foreign admin bootstraps");
    let foreign_jwt = server
        .exchange_api_key(foreign.api_key().expect("foreign admin has an API key"))
        .await
        .expect("rootless exchange succeeds");
    let (status, problem) = execute(&server, &foreign_jwt, body.to_string()).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{problem}");
    assert!(!problem.to_string().contains("score"), "{problem}");
    let uri = format!(
        "/v1/verification/executions/{}",
        result["execution_id"].as_str().expect("id")
    );
    let (status, problem) = get(&server, &foreign_jwt, &uri).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{problem}");
    assert!(!problem.to_string().contains("score"), "{problem}");
    server.shutdown().await.expect("test server shuts down");
}
