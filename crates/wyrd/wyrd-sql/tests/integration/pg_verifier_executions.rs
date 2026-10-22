//! PgFixture coverage for the durable Verifier execution queue: batched
//! submission acceptance with keyed replay and conflict, scheduled enqueue,
//! scheduler claims, leased execution, token-fenced settlement, Operator
//! dispatch creation, status reads, and tenant isolation.
//!
//! Every test runs against the repository managed Postgres through tenant
//! connections, so forced RLS applies. Each fixture owns its own database, so
//! cross-tenant counts are exact.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::{DateTime, Duration, TimeZone, Utc};
use serde_json::{Value, json};
use uuid::Uuid;
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_runtime::principal::{Principal, PrincipalId, PrincipalKind, PrincipalRef};
use wyrd_runtime::{AcceptedAuthority, DelegationStep, PermissionSet};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::Attribution;
use wyrd_spec::card::operator::{DriftCounts, MAX_SUMMARY_CHARS, OperatorFailureContext};
use wyrd_spec::card::trigger::TriggerDuration;
use wyrd_spec::card::verifier::{OWNER_OCCURRENCE_KEY, SourceText};
use wyrd_spec::envelope::{Card, CardKind};
use wyrd_spec::ids::{BindingId, CardUid, ExecutionId, GroupId, SlotName, VerificationResultId};
use wyrd_spec::reference::CardRefScope;
use wyrd_spec::registry::DeclarationOperationId;
use wyrd_spec::vala::ids::{RunId, SpanId, TraceId};
use wyrd_spec::verification::execution::{InputDeclaration, VerificationRequest};
use wyrd_spec::verification::{
    DriftWindow, GroupEndSettings, GroupSource, OperatorDispatchStatus, VerificationError,
    VerificationExecutionStatus, VerificationVerdict, VerifierReadiness,
};
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::provision_system_principal;
use wyrd_sql::queries::cards::{
    NewCardRow, NewDeclarationOperation, insert_card_row, insert_declaration_operation,
    upsert_service_account_from_card,
};
use wyrd_sql::queries::operator_dispatches::OperatorDispatchQueue;
use wyrd_sql::queries::verification::{
    BindingActivation, FrozenTarget, NewBinding, project_bindings, record_machine_authentication,
};
use wyrd_sql::queries::verification_target::VerificationTargets;
use wyrd_sql::queries::verifier_executions::{
    Cause, ClaimedExecution, Completion, EnqueueOutcome, EnqueueRefusal, ExecutionAuthority,
    ExecutionInput, ExecutionLease, ExecutionOrigin, ExecutionStatus, GroupActivity,
    GroupSelection, PinnedGroup, QueueCounts, RetryOutcome, ScheduleOutcome, ScheduleSkip,
    ScheduleTick, Settlement, Submission, SubmissionOutcome, TerminalStatus,
    VerifierExecutionQueue,
};
use wyrd_sql::row_types::cards::CardStatus;

/// Counts every test completion freezes, matching a one-of-three drift report.
const DRIFT_COUNTS: DriftCounts = DriftCounts {
    drifted_features: 1,
    total_features: 3,
};

/// Mint a fresh Card UID.
///
/// # Panics
/// Never in practice: a freshly minted `UUIDv7` is always a valid Card UID.
fn uid() -> CardUid {
    CardUid::from_uuid(Uuid::now_v7()).expect("UUIDv7 is a valid card UID")
}

/// Build one fixed UTC instant on 2026-09-`day` at `hour`:`minute`.
///
/// # Panics
/// Panics when the components do not name a valid instant.
fn at(day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, day, hour, minute, 0)
        .single()
        .expect("valid instant")
}

/// The fixed window of a binding's first scheduled execution in these tests.
fn window() -> DriftWindow {
    DriftWindow {
        start: at(1, 0, 0),
        end: at(2, 0, 0),
    }
}

/// A registering user principal in `tenant`, which also serves as a
/// submitting caller.
///
/// The principal carries no permissions; persistence-level queries do not
/// authorize.
fn actor(tenant: DataTenantId) -> Principal {
    Principal::new(
        PrincipalId::new(Uuid::now_v7()),
        PrincipalKind::User,
        tenant,
        Vec::new(),
        PermissionSet::new(),
    )
}

/// A stored building-block Verifier spec with one drift check by `method`.
fn drift(method: &str) -> Value {
    json!({
        "requirements": [{
            "id": "latency_drift",
            "statement": "latency keeps its baseline distribution",
            "check": { "kind": "drift", "method": method, "on": { "input": "current" } }
        }]
    })
}

/// A ready Verifier whose one drift check needs no fitted baseline.
fn custom_drift() -> Value {
    drift("Custom")
}

/// A PSI Verifier, never ready until its baseline is fitted.
fn psi_drift() -> Value {
    drift("Psi")
}

/// A Verifier stored under the retired implementation contract.
fn retired() -> Value {
    json!({ "implementation": { "kind": "eval", "spec": { "tasks": {} } } })
}

/// Insert one Card row named `name` and return its UID.
///
/// Every row starts as a Service Card because only the stored kind, spec,
/// and status matter to the queue; callers rewrite them as needed.
///
/// # Panics
/// Panics when the registration operation or Card row fails to insert.
async fn insert_card(conn: &mut TenantConn<'_>, actor: &Principal, card: &Card) -> CardUid {
    let operation_id = DeclarationOperationId::new(Uuid::now_v7());
    let key = Uuid::now_v7().to_string();
    insert_declaration_operation(
        conn,
        NewDeclarationOperation {
            operation_id,
            attribution: wyrd_spec::auth::Attribution::direct(actor.id),
            idempotency_key: &key,
            request_hash: "request-hash",
        },
    )
    .await
    .expect("operation inserts");
    insert_card_row(
        conn,
        NewCardRow {
            card,
            card_uid: uid(),
            attribution: wyrd_spec::auth::Attribution::direct(actor.id),
            operation_id,
            status: CardStatus::Pending,
            spec_hash: "spec-hash",
            artifact_hash: None,
            reconcile_delay: std::time::Duration::ZERO,
        },
    )
    .await
    .expect("card inserts")
    .card_uid
}

/// Build one Service Card fixture named `name`.
///
/// # Panics
/// Panics when the fixture JSON no longer deserializes as a [`Card`].
fn service_card(name: &str) -> Card {
    serde_json::from_value(json!({
        "apiVersion": "wyrd/v1",
        "kind": "Service",
        "metadata": { "name": name, "version": "1.0.0", "space": "default" },
        "spec": {},
        "relationships": { "outbound": [], "inbound": [] }
    }))
    .expect("service fixture must deserialize")
}

/// Register one active Service Card and its principal; return (card UID, principal id).
///
/// Activation is forced with a direct update because the full lifecycle
/// requires a verified blob that this persistence-level test does not need.
///
/// # Panics
/// Panics when the Card, principal, or activation write fails.
async fn declare_service(
    conn: &mut TenantConn<'_>,
    actor: &Principal,
    name: &str,
) -> (CardUid, PrincipalId) {
    let card = service_card(name);
    let card_uid = insert_card(conn, actor, &card).await;
    let principal = upsert_service_account_from_card(
        conn,
        &card_uid,
        &card,
        wyrd_spec::auth::Attribution::direct(actor.id),
    )
    .await
    .expect("principal projects");
    sqlx::query("UPDATE wyrd.cards SET status = 'active' WHERE card_uid = $1")
        .bind(card_uid.as_uuid())
        .execute(&mut **conn.transaction())
        .await
        .expect("card activates");
    (card_uid, principal)
}

/// Register one active Verifier Card with the stored spec body `spec`.
///
/// # Panics
/// Panics when the Card row fails to insert or rewrite.
async fn register_verifier(
    conn: &mut TenantConn<'_>,
    actor: &Principal,
    name: &str,
    spec: Value,
) -> CardUid {
    let card_uid = insert_card(conn, actor, &service_card(name)).await;
    sqlx::query(
        "UPDATE wyrd.cards SET kind = 'Verifier', spec = $2, status = 'active' WHERE card_uid = $1",
    )
    .bind(card_uid.as_uuid())
    .bind(spec)
    .execute(&mut **conn.transaction())
    .await
    .expect("verifier card rewrites");
    card_uid
}

/// Project one owner-level daily-at-02:00-UTC binding of `verifier` on
/// `owner` with `operators`.
///
/// # Panics
/// Panics when the projection fails.
async fn bind(
    conn: &mut TenantConn<'_>,
    owner: &CardUid,
    verifier: &CardUid,
    operators: Vec<FrozenTarget>,
) -> BindingId {
    project_bindings(
        conn,
        owner,
        &CardKind::Service,
        &[NewBinding {
            subject_occurrence_key: OWNER_OCCURRENCE_KEY.to_owned(),
            card_uid: owner.clone(),
            verifier_card_uid: verifier.clone(),
            trigger: FrozenTarget::Digest("sha256:trigger".to_owned()),
            operators,
            activation: BindingActivation::Schedule {
                cron: "0 2 * * *".to_owned(),
                tz: None,
            },
        }],
    )
    .await
    .expect("binding projects")[0]
}

/// Overwrite one binding's schedule cursor.
///
/// # Panics
/// Panics when the update fails.
async fn set_cursor(conn: &mut TenantConn<'_>, binding: BindingId, next: Option<DateTime<Utc>>) {
    sqlx::query("UPDATE wyrd.verification_bindings SET next_run_at = $2 WHERE binding_id = $1")
        .bind(binding.as_uuid())
        .bind(next)
        .execute(&mut **conn.transaction())
        .await
        .expect("cursor updates");
}

/// Read one binding's schedule cursor.
///
/// # Panics
/// Panics when the binding row cannot be read.
async fn cursor(conn: &mut TenantConn<'_>, binding: BindingId) -> Option<DateTime<Utc>> {
    sqlx::query_scalar("SELECT next_run_at FROM wyrd.verification_bindings WHERE binding_id = $1")
        .bind(binding.as_uuid())
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("cursor reads")
}

/// REQ-186's fixed wait after a scheduled window ends before its occurrence
/// may be claimed; a cursor is due only once it is this far behind the
/// database clock.
const SCHEDULE_CLAIM_DELAY: Duration = Duration::seconds(30);

/// Read the database's current statement instant, the queue's only clock.
///
/// # Panics
/// Panics when the instant cannot be read.
async fn database_now(conn: &mut TenantConn<'_>) -> DateTime<Utc> {
    sqlx::query_scalar("SELECT statement_timestamp()")
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("database clock reads")
}

/// Bring `execution`'s lease and retry deadlines due by moving whichever of
/// them is set to the database's current instant.
///
/// This is how a test reaches an expired lease or an elapsed backoff without a
/// process clock to advance: the row under test is placed in the past and
/// PostgreSQL still decides whether it is due.
///
/// # Panics
/// Panics when the update fails.
async fn expire_deadlines(conn: &mut TenantConn<'_>, execution: ExecutionId) {
    sqlx::query(
        "UPDATE wyrd.verifier_executions
            SET lease_expires_at = CASE WHEN lease_expires_at IS NULL
                                        THEN NULL ELSE statement_timestamp() END,
                next_attempt_at = CASE WHEN next_attempt_at IS NULL
                                       THEN NULL ELSE statement_timestamp() END
          WHERE execution_id = $1",
    )
    .bind(execution.as_uuid())
    .execute(&mut **conn.transaction())
    .await
    .expect("deadlines expire");
}

/// Read the backoff the queue stored for `execution`: the gap between the
/// retry's own write instant and the attempt deadline it scheduled.
///
/// Both timestamps come from the same `statement_timestamp()`, so the gap is
/// exactly the configured backoff regardless of when the test reads it.
///
/// # Panics
/// Panics when the row cannot be read or is not awaiting a retry.
async fn stored_backoff(conn: &mut TenantConn<'_>, execution: ExecutionId) -> Duration {
    let (attempt_at, updated_at): (DateTime<Utc>, DateTime<Utc>) = sqlx::query_as(
        "SELECT next_attempt_at, updated_at FROM wyrd.verifier_executions WHERE execution_id = $1",
    )
    .bind(execution.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("retry deadline reads");
    attempt_at - updated_at
}

/// Move `principal`'s recorded machine activity `ago` into the past, so an
/// activity window can lapse without a process clock.
///
/// # Panics
/// Panics when the update fails.
async fn age_activity(conn: &mut TenantConn<'_>, principal: PrincipalId, ago: Duration) {
    sqlx::query(
        "UPDATE wyrd.auth_service_accounts
            SET last_authenticated_at =
                    statement_timestamp() - ($2::bigint * INTERVAL '1 millisecond')
          WHERE id = $1",
    )
    .bind(principal.as_uuid())
    .bind(ago.num_milliseconds())
    .execute(&mut **conn.transaction())
    .await
    .expect("activity ages");
}

/// Assert `cursor` is the first daily 02:00 UTC boundary strictly after
/// `after`.
///
/// Schedule arming now anchors on the database instant, so a test cannot name
/// the next boundary literally; it asserts the boundary's shape instead.
///
/// # Panics
/// Panics when the cursor is absent, not at 02:00:00 UTC, or not within the
/// day following `after`.
fn assert_next_daily_boundary(cursor: Option<DateTime<Utc>>, after: DateTime<Utc>) {
    use chrono::Timelike;

    let cursor = cursor.expect("a scheduled binding carries a cursor");
    assert_eq!(
        (cursor.hour(), cursor.minute(), cursor.second()),
        (2, 0, 0),
        "the cursor must land on the daily 02:00 UTC boundary"
    );
    assert!(
        cursor > after && cursor <= after + Duration::days(1),
        "{cursor} is not the first 02:00 boundary after {after}"
    );
}

/// Count the tenant's executions.
///
/// # Panics
/// Panics when the count cannot be read.
async fn execution_count(conn: &mut TenantConn<'_>) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM wyrd.verifier_executions")
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("executions count")
}

/// The direct accepted authority of `principal`, with no delegation chain.
fn authority(principal: &Principal) -> AcceptedAuthority {
    AcceptedAuthority::new(principal, &[])
}

/// The accepted authority of the binding owner's Card-bound `principal`.
///
/// Persistence does not interpret the authority, so a Card-free Service
/// principal with no permissions stands in for the owner.
fn owner_authority(tenant: DataTenantId, principal: PrincipalId) -> AcceptedAuthority {
    authority(&Principal::new(
        principal,
        PrincipalKind::Service {
            card_ref: None,
            card_ref_scope: CardRefScope::default(),
        },
        tenant,
        Vec::new(),
        PermissionSet::new(),
    ))
}

/// The background authority of a binding owned by `owner`, executed by the
/// tenant SYSTEM principal, provisioning SYSTEM when absent.
///
/// # Panics
/// Panics when SYSTEM cannot be provisioned.
async fn background(conn: &mut TenantConn<'_>, owner: PrincipalId) -> ExecutionAuthority {
    let system = provision_system_principal(conn, Attribution::unresolved())
        .await
        .expect("system principal provisions");
    ExecutionAuthority::background(
        owner_authority(conn.data_tenant_id(), owner),
        PrincipalId::new(system),
    )
}

/// Enqueue the next scheduled occurrence of `binding` and return its execution.
///
/// The n-th execution of a binding analyzes the n-th consecutive day starting
/// at [`window`], so repeated calls create distinct occurrences.
///
/// # Panics
/// Panics when the enqueue fails or does not create a new execution.
async fn scheduled(
    queue: &VerifierExecutionQueue,
    conn: &mut TenantConn<'_>,
    binding: BindingId,
) -> ExecutionId {
    let earlier: i64 =
        sqlx::query_scalar("SELECT count(*) FROM wyrd.verifier_executions WHERE binding_id = $1")
            .bind(binding.as_uuid())
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("earlier executions count");
    let offset = Duration::days(earlier);
    let occurrence = DriftWindow {
        start: window().start + offset,
        end: window().end + offset,
    };
    let authority = background(conn, PrincipalId::new(Uuid::now_v7())).await;
    match queue
        .enqueue_scheduled(conn, binding, occurrence, &authority)
        .await
        .expect("scheduled execution enqueues")
    {
        EnqueueOutcome::Enqueued(execution_id) => execution_id,
        other => panic!("expected a new execution, got {other:?}"),
    }
}

/// Claim and settle the next due binding the way the server scheduler does:
/// resolve the owner's authority with the tenant SYSTEM actor, then settle.
///
/// # Panics
/// Panics when the claim, SYSTEM provisioning, or settlement fails.
async fn schedule_next_due(
    queue: &VerifierExecutionQueue,
    conn: &mut TenantConn<'_>,
) -> Option<ScheduleTick> {
    let due = queue.claim_due(conn).await.expect("due binding claims")?;
    let authority = match due.owner() {
        Some(owner) => Some(background(conn, owner).await),
        None => None,
    };
    Some(
        queue
            .schedule_due(conn, due, authority)
            .await
            .expect("due binding settles"),
    )
}

/// A request to run `verifier` on `subject` with one inline JSON input.
fn request(verifier: &CardUid, subject: &CardUid) -> VerificationRequest {
    VerificationRequest {
        verifier_card_uid: verifier.clone(),
        card_uid: subject.clone(),
        run_id: None,
        group_id: None,
        inputs: BTreeMap::from([(
            SlotName::new("answer").expect("valid slot name"),
            InputDeclaration::Json {
                value: json!({ "text": "42" }),
            },
        )]),
        execution_route_overrides: BTreeMap::new(),
    }
}

/// Stage `request` by `authority` as the submit path does, under `key`.
///
/// # Panics
/// Panics when the request cannot be canonicalized.
fn submission(
    tenant: DataTenantId,
    authority: AcceptedAuthority,
    request: VerificationRequest,
    key: Option<&str>,
) -> Submission {
    Submission::new(tenant, authority, request, None, key.map(str::to_owned))
        .expect("request canonicalizes")
}

/// Accept one submission as its own batch and return the writer's decision.
///
/// # Panics
/// Panics when the batch fails or returns other than one outcome.
async fn accept(
    queue: &VerifierExecutionQueue,
    conn: &mut TenantConn<'_>,
    submission: &Submission,
) -> SubmissionOutcome {
    let batch = std::slice::from_ref(submission);
    let refused = refusals(conn, batch).await;
    let outcomes = queue
        .accept_submissions(conn, batch, &refused)
        .await
        .expect("submission batch accepts");
    assert_eq!(outcomes.len(), 1, "one outcome per submission");
    outcomes[0]
}

/// Resolve every target of `submissions` through `wyrd.verification_target`
/// as the server's writer does, keeping the refused ones.
///
/// # Panics
/// Panics when a target read fails.
async fn refusals(
    conn: &mut TenantConn<'_>,
    submissions: &[Submission],
) -> HashMap<(Uuid, Uuid), EnqueueRefusal> {
    let tenant = conn.data_tenant_id();
    let mut refused = HashMap::new();
    for submission in submissions {
        let request = &submission.request;
        let target = VerificationTargets::new(conn.transaction(), tenant)
            .read(&request.verifier_card_uid, &request.card_uid)
            .await
            .expect("target reads");
        if let Some(refusal) = target.refusal() {
            refused.insert(
                (
                    request.verifier_card_uid.as_uuid(),
                    request.card_uid.as_uuid(),
                ),
                refusal,
            );
        }
    }
    refused
}

/// Accept one valid submission and return its execution.
///
/// # Panics
/// Panics when the submission is not accepted.
async fn submitted(
    queue: &VerifierExecutionQueue,
    conn: &mut TenantConn<'_>,
    submission: &Submission,
) -> ExecutionId {
    assert_eq!(
        accept(queue, conn, submission).await,
        SubmissionOutcome::Accepted
    );
    submission.execution_id
}

/// Read one execution's status, which must exist.
///
/// # Panics
/// Panics when the read fails or the execution is not visible.
async fn status(
    queue: &VerifierExecutionQueue,
    conn: &mut TenantConn<'_>,
    execution: ExecutionId,
) -> ExecutionStatus {
    queue
        .execution_status(conn, execution)
        .await
        .expect("status reads")
        .expect("execution exists")
}

/// A retryable engine error.
fn engine_error() -> VerificationError {
    VerificationError {
        code: "engine_unavailable".to_owned(),
        message: "the analysis engine did not respond".to_owned(),
    }
}

/// Claim the next runnable execution, which must exist.
///
/// # Panics
/// Panics when the claim fails or nothing is runnable.
async fn claim(queue: &VerifierExecutionQueue, conn: &mut TenantConn<'_>) -> ClaimedExecution {
    queue
        .claim(conn, Duration::minutes(5), 1)
        .await
        .expect("claim runs")
        .pop()
        .expect("an execution is runnable")
}

/// A scheduled execution freezes the owner, binding, Trigger, Operators,
/// window, and the binding's samples selectors; a submission freezes only the
/// Verifier, subject, optional Run, inputs, route overrides, and the caller's
/// authority. Both claims expose exactly those identities.
///
/// # Panics
/// Panics when an execution is refused or any frozen identity differs.
#[tokio::test]
async fn scheduled_and_submitted_executions_freeze_their_inputs() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let actor = actor(tenant);
    let queue = VerifierExecutionQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, _) = declare_service(&mut conn, &actor, "svc").await;
    let verifier = register_verifier(&mut conn, &actor, "drift", custom_drift()).await;
    let operator = FrozenTarget::Uid(uid());
    let binding = bind(&mut conn, &owner, &verifier, vec![operator.clone()]).await;

    let bound = scheduled(&queue, &mut conn, binding).await;
    let mut direct_request = request(&verifier, &owner);
    direct_request.run_id = Some(RunId::from_string("app-run-1".to_owned()));
    let direct_submission = submission(tenant, authority(&actor), direct_request.clone(), None);
    let direct = submitted(&queue, &mut conn, &direct_submission).await;
    assert_ne!(bound, direct);

    let first = claim(&queue, &mut conn).await;
    let second = claim(&queue, &mut conn).await;
    let (bound_claim, direct_claim) = if first.lease.execution_id == bound {
        (first, second)
    } else {
        (second, first)
    };
    assert_eq!(bound_claim.origin, ExecutionOrigin::Schedule);
    assert_eq!(bound_claim.binding_id, Some(binding));
    assert_eq!(bound_claim.owner_card_uid, Some(owner.clone()));
    assert_eq!(
        bound_claim.trigger,
        Some(FrozenTarget::Digest("sha256:trigger".to_owned()))
    );
    assert_eq!(bound_claim.verifier_card_uid, verifier);
    assert_eq!(bound_claim.verifier_version, "1.0.0");
    assert_eq!(bound_claim.card_uid, owner);
    assert_eq!(
        bound_claim.input,
        ExecutionInput::Scheduled { window: window() }
    );
    assert_eq!((bound_claim.attempt, bound_claim.max_attempts), (1, 3));

    assert_eq!(direct_claim.lease.execution_id, direct);
    assert_eq!(direct_claim.origin, ExecutionOrigin::Manual);
    assert_eq!(direct_claim.binding_id, None);
    assert_eq!(direct_claim.owner_card_uid, None);
    assert_eq!(direct_claim.trigger, None);
    assert_eq!(
        direct_claim.input,
        ExecutionInput::Submitted {
            run_id: direct_request.run_id,
            inputs: direct_request.inputs,
            route_overrides: BTreeMap::new(),
        }
    );
    assert_eq!(direct_claim.attribution, Attribution::direct(actor.id));
    assert_eq!(direct_claim.authority, authority(&actor));
    let operators: Value = sqlx::query_scalar(
        "SELECT operators FROM wyrd.verifier_executions WHERE execution_id = $1",
    )
    .bind(direct.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("operators read");
    assert_eq!(operators, json!([]), "a submission has no Operators");
}

/// Unknown bindings and unready PSI baselines refuse a scheduled occurrence
/// without writing. A submission whose Verifier is missing, not a Verifier,
/// deleted, or stored under the retired contract, whose PSI baseline is not
/// ready, or whose subject is unavailable settles `errored` with the public
/// catalog code and never runs; a Card the tenant does not hold at all
/// leaves no row.
///
/// # Panics
/// Panics when any request is accepted or refused for the wrong reason.
#[tokio::test]
async fn unrunnable_targets_are_refused() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let actor = actor(tenant);
    let queue = VerifierExecutionQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, owner_principal) = declare_service(&mut conn, &actor, "svc").await;
    let custom = register_verifier(&mut conn, &actor, "custom", custom_drift()).await;
    let psi = register_verifier(&mut conn, &actor, "psi", psi_drift()).await;
    let old = register_verifier(&mut conn, &actor, "old", retired()).await;
    let psi_binding = bind(&mut conn, &owner, &psi, Vec::new()).await;
    let owner_authority = background(&mut conn, owner_principal).await;
    for (binding, refusal) in [
        (BindingId::new_v7(), EnqueueRefusal::BindingNotFound),
        (
            psi_binding,
            EnqueueRefusal::NotReady(VerifierReadiness::BaselineNotReady),
        ),
    ] {
        assert_eq!(
            queue
                .enqueue_scheduled(&mut conn, binding, window(), &owner_authority)
                .await
                .expect("enqueue answers"),
            EnqueueOutcome::Refused(refusal)
        );
    }
    assert_eq!(execution_count(&mut conn).await, 0);

    let (dormant, _) = declare_service(&mut conn, &actor, "dormant").await;
    sqlx::query("UPDATE wyrd.cards SET status = 'deleted' WHERE card_uid = $1")
        .bind(dormant.as_uuid())
        .execute(&mut **conn.transaction())
        .await
        .expect("subject deletes");
    let cases = [
        (
            &psi,
            &owner,
            EnqueueRefusal::NotReady(VerifierReadiness::BaselineNotReady),
            "WYRD_VERIFICATION_409_BASELINE_NOT_READY",
        ),
        (
            &owner,
            &owner,
            EnqueueRefusal::NotReady(VerifierReadiness::VerifierUnavailable),
            "WYRD_VERIFICATION_400_INVALID_TARGET",
        ),
        (
            &old,
            &owner,
            EnqueueRefusal::NotReady(VerifierReadiness::VerifierUnavailable),
            "WYRD_VERIFICATION_400_INVALID_TARGET",
        ),
        (
            &custom,
            &dormant,
            EnqueueRefusal::SubjectUnavailable,
            "WYRD_VERIFICATION_400_INVALID_TARGET",
        ),
    ];
    for (verifier, subject, refusal, code) in cases {
        let refused = submission(tenant, authority(&actor), request(verifier, subject), None);
        assert_eq!(
            accept(&queue, &mut conn, &refused).await,
            SubmissionOutcome::Refused(refusal)
        );
        let state = status(&queue, &mut conn, refused.execution_id).await.state;
        assert_eq!(state.status, VerificationExecutionStatus::Errored);
        assert_eq!(state.error.map(|error| error.code), Some(code.to_owned()));
    }
    assert!(
        queue
            .claim(&mut conn, Duration::minutes(5), 1)
            .await
            .expect("claim runs")
            .is_empty(),
        "a refused submission never runs"
    );

    let unknown = submission(tenant, authority(&actor), request(&uid(), &owner), None);
    assert_eq!(
        accept(&queue, &mut conn, &unknown).await,
        SubmissionOutcome::Refused(EnqueueRefusal::NotReady(
            VerifierReadiness::VerifierUnavailable
        ))
    );
    assert!(
        queue
            .execution_status(&mut conn, unknown.execution_id)
            .await
            .expect("status reads")
            .is_none(),
        "a Verifier the tenant does not hold leaves no row"
    );
}

/// A keyed submission replays its execution for the same request, settles a
/// different request under the same key `errored` with the idempotency
/// conflict under its own ID while the original stays pending, and scopes
/// keys to the subject and actor together. A retried batch changes nothing,
/// and a refused keyed request replays its refusal.
///
/// # Panics
/// Panics when any keyed outcome, stored key, row count, or state differs.
#[tokio::test]
async fn keyed_submissions_replay_conflict_and_scope_to_subject_and_actor() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let (alice, bob, carol) = (actor(tenant), actor(tenant), actor(tenant));
    let queue = VerifierExecutionQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, _) = declare_service(&mut conn, &alice, "svc").await;
    let custom = register_verifier(&mut conn, &alice, "custom", custom_drift()).await;
    let psi = register_verifier(&mut conn, &alice, "psi", psi_drift()).await;
    let through = |subject: &Principal, actor: &Principal| {
        AcceptedAuthority::new(
            subject,
            &[DelegationStep {
                principal: PrincipalRef::from_principal(actor),
            }],
        )
    };
    let original_request = request(&custom, &owner);
    let mut changed_request = original_request.clone();
    changed_request.run_id = Some(RunId::from_string("app-run-2".to_owned()));

    let original = submission(
        tenant,
        authority(&alice),
        original_request.clone(),
        Some("retry-1"),
    );
    let execution = submitted(&queue, &mut conn, &original).await;
    let retry = submission(
        tenant,
        authority(&alice),
        original_request.clone(),
        Some("retry-1"),
    );
    assert_eq!(
        retry.execution_id, execution,
        "an identical keyed retry is assigned the same execution before acceptance"
    );
    assert_eq!(
        queue
            .accept_submissions(&mut conn, &[original.clone(), retry], &HashMap::new())
            .await
            .expect("replayed batch accepts"),
        vec![
            SubmissionOutcome::Replayed(execution),
            SubmissionOutcome::Replayed(execution)
        ]
    );

    let conflicting = submission(
        tenant,
        authority(&alice),
        changed_request.clone(),
        Some("retry-1"),
    );
    assert_ne!(conflicting.execution_id, execution);
    assert_eq!(
        accept(&queue, &mut conn, &conflicting).await,
        SubmissionOutcome::Conflict {
            original: execution
        }
    );
    let conflict = status(&queue, &mut conn, conflicting.execution_id)
        .await
        .state;
    assert_eq!(conflict.status, VerificationExecutionStatus::Errored);
    assert_eq!(
        conflict.error.map(|error| error.code),
        Some("WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT".to_owned())
    );
    assert_eq!(
        status(&queue, &mut conn, execution).await.state.status,
        VerificationExecutionStatus::Pending,
        "a conflict never changes the original execution"
    );
    assert_eq!(
        accept(&queue, &mut conn, &conflicting).await,
        SubmissionOutcome::Replayed(conflicting.execution_id),
        "a retried conflict replays its own errored row"
    );
    let stored: (Option<String>, Option<Vec<u8>>) = sqlx::query_as(
        "SELECT idempotency_key, request_sha256 FROM wyrd.verifier_executions \
          WHERE execution_id = $1",
    )
    .bind(execution.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("stored key reads");
    assert_eq!(
        stored,
        (
            Some("retry-1".to_owned()),
            original.key.as_ref().map(|key| key.request_sha256.to_vec())
        )
    );

    let mut pairs = Vec::new();
    for accepted in [through(&alice, &bob), through(&carol, &bob)] {
        let pair = accepted.attribution();
        let keyed = submission(tenant, accepted, original_request.clone(), Some("retry-1"));
        let execution_id = submitted(&queue, &mut conn, &keyed).await;
        let stored: (Uuid, Uuid) = sqlx::query_as(
            "SELECT sub_principal_id, act_principal_id FROM wyrd.verifier_executions \
              WHERE execution_id = $1",
        )
        .bind(execution_id.as_uuid())
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("stored pair reads");
        assert_eq!(
            stored,
            (
                pair.sub_principal_id.as_uuid(),
                pair.act_principal_id.as_uuid()
            )
        );
        pairs.push(execution_id);
    }
    assert_ne!(pairs[0], pairs[1]);
    assert!(!pairs.contains(&execution));

    let unready = submission(
        tenant,
        authority(&alice),
        request(&psi, &owner),
        Some("refused"),
    );
    assert_eq!(
        accept(&queue, &mut conn, &unready).await,
        SubmissionOutcome::Refused(EnqueueRefusal::NotReady(
            VerifierReadiness::BaselineNotReady
        ))
    );
    assert_eq!(
        accept(&queue, &mut conn, &unready).await,
        SubmissionOutcome::Replayed(unready.execution_id),
        "a retried refusal replays the refused execution"
    );
    assert_eq!(execution_count(&mut conn).await, 5);
}

/// Another tenant's keyed submission, or one that names another tenant's
/// Cards, never replays or conflicts with this tenant's executions.
///
/// # Panics
/// Panics when a foreign submission observes or reuses a local execution.
#[tokio::test]
async fn keyed_submissions_are_tenant_scoped() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let other = fixture
        .seed_additional_tenant("verifier-executions-keys")
        .await
        .expect("second tenant seeds");
    let tenant = fixture.data_tenant_id();
    let alice = actor(tenant);
    let queue = VerifierExecutionQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, _) = declare_service(&mut conn, &alice, "svc").await;
    let custom = register_verifier(&mut conn, &alice, "custom", custom_drift()).await;
    let local = submission(
        tenant,
        authority(&alice),
        request(&custom, &owner),
        Some("shared-key"),
    );
    submitted(&queue, &mut conn, &local).await;
    conn.commit().await.expect("tenant A commits");

    let mut foreign = fixture
        .tenant_conn_for(other)
        .await
        .expect("tenant B opens");
    let replayed_elsewhere = Submission {
        execution_id: local.execution_id,
        ..local.clone()
    };
    assert_eq!(
        accept(&queue, &mut foreign, &replayed_elsewhere).await,
        SubmissionOutcome::Refused(EnqueueRefusal::NotReady(
            VerifierReadiness::VerifierUnavailable
        )),
        "another tenant's execution ID is invisible, and so are its Cards"
    );
    assert_eq!(execution_count(&mut foreign).await, 0);
}

/// Concurrent writers accepting the same subject, actor, and key serialize:
/// the later writer waits for the uncommitted earlier one, then replays an
/// identical request or records a conflict for a different one.
///
/// # Panics
/// Panics when the later writer does not wait or decides without seeing the
/// committed original.
#[tokio::test]
async fn concurrent_keyed_writers_serialize_on_the_key() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let alice = actor(tenant);
    let queue = VerifierExecutionQueue::default();
    let mut setup = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, _) = declare_service(&mut setup, &alice, "svc").await;
    let custom = register_verifier(&mut setup, &alice, "custom", custom_drift()).await;
    setup.commit().await.expect("setup commits");
    let original = submission(
        tenant,
        authority(&alice),
        request(&custom, &owner),
        Some("race"),
    );
    let mut changed = request(&custom, &owner);
    changed.run_id = Some(RunId::from_string("app-run-3".to_owned()));
    let conflicting = submission(tenant, authority(&alice), changed, Some("race"));

    let mut earlier = fixture.tenant_conn().await.expect("earlier opens");
    submitted(&queue, &mut earlier, &original).await;
    let mut later = fixture.tenant_conn().await.expect("later opens");
    let mut probe = fixture.tenant_conn().await.expect("probe opens");
    let later_accept = async {
        let outcome = queue
            .accept_submissions(
                &mut later,
                &[original.clone(), conflicting.clone()],
                &HashMap::new(),
            )
            .await
            .expect("later batch accepts");
        later.commit().await.expect("later commits");
        outcome
    };
    tokio::pin!(later_accept);
    let raced = tokio::select! {
        outcome = &mut later_accept => Some(outcome),
        () = await_blocked_backend(&mut probe) => None,
    };
    assert_eq!(
        raced, None,
        "a later writer under the same key waits for the uncommitted earlier one"
    );
    earlier.commit().await.expect("earlier commits");
    assert_eq!(
        later_accept.await,
        vec![
            SubmissionOutcome::Replayed(original.execution_id),
            SubmissionOutcome::Conflict {
                original: original.execution_id
            }
        ]
    );
}

/// Wait until some backend of this fixture's database is blocked behind
/// another transaction's lock.
///
/// Clears the activity snapshot on every poll, because Postgres otherwise
/// keeps the first read's backend list for the rest of `conn`'s transaction.
///
/// # Panics
/// Panics when the probe query fails or nothing blocks within 30 seconds.
async fn await_blocked_backend(conn: &mut TenantConn<'_>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        sqlx::query("SELECT pg_stat_clear_snapshot()")
            .execute(&mut **conn.transaction())
            .await
            .expect("snapshot clears");
        let blocked: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_stat_activity \
              WHERE datname = current_database() AND cardinality(pg_blocking_pids(pid)) > 0",
        )
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("blocked backends count");
        if blocked > 0 {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "no backend blocked within 30 seconds"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

/// Two schedulers ticking the same due binding concurrently create exactly one
/// execution: the second skips the locked row. The committed cursor moves to the
/// next future boundary, a restarted scheduler on fresh handles finds nothing
/// due, and re-enqueueing the same occurrence returns the existing execution. The
/// cursor sits one claim delay behind the database clock, and the claimed
/// window still ends at the cursor.
///
/// # Panics
/// Panics when an occurrence produces zero or two executions, the window or cursor
/// is wrong, or a restarted scheduler executes the occurrence again.
#[tokio::test]
async fn concurrent_schedulers_create_one_run_per_occurrence() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let queue = VerifierExecutionQueue::default();
    let mut setup = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let system = provision_system_principal(&mut setup, Attribution::unresolved())
        .await
        .expect("the tenant SYSTEM principal provisions");
    let (owner, principal) = declare_service(&mut setup, &actor, "svc").await;
    let verifier = register_verifier(&mut setup, &actor, "drift", custom_drift()).await;
    let binding = bind(&mut setup, &owner, &verifier, Vec::new()).await;
    let armed_after = database_now(&mut setup).await;
    record_machine_authentication(&mut setup, principal)
        .await
        .expect("owner activates");
    assert_next_daily_boundary(cursor(&mut setup, binding).await, armed_after);
    let now = database_now(&mut setup).await;
    let due = now - SCHEDULE_CLAIM_DELAY;
    set_cursor(&mut setup, binding, Some(due)).await;
    setup.commit().await.expect("setup commits");

    let mut first = fixture.tenant_conn().await.expect("first scheduler opens");
    let mut second = fixture.tenant_conn().await.expect("second scheduler opens");
    let tick = schedule_next_due(&queue, &mut first)
        .await
        .expect("the binding is due");
    assert!(
        schedule_next_due(&queue, &mut second).await.is_none(),
        "a locked due binding is skipped"
    );
    assert_eq!(tick.binding_id, binding);
    assert_eq!(tick.due_at, due);
    assert_next_daily_boundary(tick.next_run_at, due);
    let ScheduleOutcome::Enqueued(execution) = tick.outcome else {
        panic!("expected a scheduled execution, got {:?}", tick.outcome);
    };
    first.commit().await.expect("first scheduler commits");
    assert!(
        schedule_next_due(&queue, &mut second).await.is_none(),
        "the committed cursor is no longer due"
    );
    drop(second);

    let (restarted, _) = fixture
        .fresh_runtime_handles()
        .await
        .expect("fresh handles open");
    let mut conn = restarted
        .tenant_conn(fixture.data_tenant_id())
        .await
        .expect("restarted scheduler opens");
    assert!(schedule_next_due(&queue, &mut conn).await.is_none());
    assert_next_daily_boundary(cursor(&mut conn, binding).await, due);
    let claimed = claim(&queue, &mut conn).await;
    assert_eq!(claimed.lease.execution_id, execution);
    assert_eq!(claimed.origin, ExecutionOrigin::Schedule);
    assert_eq!(
        claimed.attribution,
        Attribution::new(principal, PrincipalId::new(system)),
        "a scheduled execution serves the owner with the SYSTEM actor"
    );
    assert_eq!(
        claimed.authority,
        owner_authority(fixture.data_tenant_id(), principal)
    );
    let ExecutionInput::Scheduled { window: occurrence } = claimed.input else {
        panic!(
            "a scheduled execution analyzes a window, got {:?}",
            claimed.input
        );
    };
    assert_eq!(
        occurrence.end, due,
        "the occurrence closes at its due instant"
    );
    let replay = background(&mut conn, principal).await;
    assert_eq!(
        queue
            .enqueue_scheduled(&mut conn, binding, occurrence, &replay)
            .await
            .expect("duplicate occurrence answers"),
        EnqueueOutcome::AlreadyEnqueued(execution)
    );
    assert_eq!(execution_count(&mut conn).await, 1);
}

/// Inactive owners, unready PSI Verifiers, and missed occurrences create no
/// execution, and every cursor still advances to the first boundary after the
/// schedule clock (database time minus the claim delay) without backfill.
///
/// # Panics
/// Panics when a skipped occurrence creates a execution, reports the wrong reason,
/// or leaves its cursor anywhere but the next future boundary.
#[tokio::test]
async fn scheduler_skips_inactive_unready_and_missed_occurrences() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let queue = VerifierExecutionQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let custom = register_verifier(&mut conn, &actor, "custom", custom_drift()).await;
    let psi = register_verifier(&mut conn, &actor, "psi", psi_drift()).await;
    let now = database_now(&mut conn).await;
    let due = now - SCHEDULE_CLAIM_DELAY;

    let (stale_owner, stale) = declare_service(&mut conn, &actor, "stale").await;
    let inactive = bind(&mut conn, &stale_owner, &custom, Vec::new()).await;
    record_machine_authentication(&mut conn, stale)
        .await
        .expect("stale activation records");
    age_activity(&mut conn, stale, Duration::days(2)).await;
    set_cursor(&mut conn, inactive, Some(due)).await;

    let (live_owner, live) = declare_service(&mut conn, &actor, "live").await;
    let unready = bind(&mut conn, &live_owner, &psi, Vec::new()).await;
    let missed = bind(&mut conn, &live_owner, &custom, Vec::new()).await;
    record_machine_authentication(&mut conn, live)
        .await
        .expect("live activation records");
    set_cursor(&mut conn, unready, Some(due)).await;
    set_cursor(&mut conn, missed, Some(due - Duration::days(2))).await;

    let mut outcomes = Vec::new();
    while let Some(tick) = schedule_next_due(&queue, &mut conn).await {
        assert_next_daily_boundary(tick.next_run_at, due);
        outcomes.push((tick.binding_id, tick.outcome));
    }
    outcomes.sort_by_key(|(binding, _)| *binding);
    let mut expected = vec![
        (inactive, ScheduleOutcome::Skipped(ScheduleSkip::Inactive)),
        (
            unready,
            ScheduleOutcome::Skipped(ScheduleSkip::Refused(EnqueueRefusal::NotReady(
                VerifierReadiness::BaselineNotReady,
            ))),
        ),
        (missed, ScheduleOutcome::Skipped(ScheduleSkip::Missed)),
    ];
    expected.sort_by_key(|(binding, _)| *binding);
    assert_eq!(outcomes, expected);
    for binding in [inactive, unready, missed] {
        assert_next_daily_boundary(cursor(&mut conn, binding).await, due);
    }
    assert_eq!(execution_count(&mut conn).await, 0);
}

/// The stored lease expiry of `execution`.
///
/// # Panics
/// Panics when the execution cannot be read or holds no lease.
async fn lease_expiry(conn: &mut TenantConn<'_>, execution: ExecutionId) -> DateTime<Utc> {
    sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
        "SELECT lease_expires_at FROM wyrd.verifier_executions WHERE execution_id = $1",
    )
    .bind(execution.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("lease reads")
    .expect("the execution holds a lease")
}

/// One renewal statement leaves a lease younger than a third of its length
/// as it is, extends an older one from the database clock, and returns only
/// tokens that still hold an unexpired lease: a reclaimed token and an
/// expired lease are absent, so their work stops.
///
/// # Panics
/// Panics when a young lease moves, an old lease is not extended, or a
/// reclaimed or expired token is returned.
#[tokio::test]
async fn leases_renew_once_a_third_has_passed_and_never_revive() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let queue = VerifierExecutionQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, _) = declare_service(&mut conn, &actor, "svc").await;
    let verifier = register_verifier(&mut conn, &actor, "drift", custom_drift()).await;
    let binding = bind(&mut conn, &owner, &verifier, Vec::new()).await;
    let execution = scheduled(&queue, &mut conn, binding).await;
    let lease = Duration::minutes(5);
    let stale = claim(&queue, &mut conn).await;
    expire_deadlines(&mut conn, execution).await;
    let held = claim(&queue, &mut conn).await;
    let young = lease_expiry(&mut conn, execution).await;
    let renewed = queue
        .renew(&mut conn, &[stale.lease.token, held.lease.token], lease)
        .await
        .expect("renewal runs");
    assert_eq!(renewed, vec![held.lease.token], "a reclaimed token is gone");
    assert_eq!(
        lease_expiry(&mut conn, execution).await,
        young,
        "a young lease stays"
    );

    sqlx::query(
        "UPDATE wyrd.verifier_executions \
         SET lease_expires_at = statement_timestamp() + INTERVAL '1 minute' WHERE execution_id = $1",
    )
    .bind(execution.as_uuid())
    .execute(&mut **conn.transaction())
    .await
    .expect("lease ages");
    let now = database_now(&mut conn).await;
    let renewed = queue
        .renew(&mut conn, &[held.lease.token], lease)
        .await
        .expect("renewal runs");
    assert_eq!(renewed, vec![held.lease.token]);
    assert!(
        lease_expiry(&mut conn, execution).await >= now + lease,
        "a lease past a third of its length is extended from the database clock"
    );

    expire_deadlines(&mut conn, execution).await;
    assert!(
        queue
            .renew(&mut conn, &[held.lease.token], lease)
            .await
            .expect("renewal runs")
            .is_empty(),
        "an expired lease is never revived"
    );
}

/// An expired lease is reclaimed under a new token with the attempt counted;
/// the old holder's completion affects nothing, while the new holder's
/// completion applies and re-applies idempotently.
///
/// # Panics
/// Panics when a live lease is reclaimable, the stale token settles, or the
/// current token's settlement is not idempotent.
#[tokio::test]
async fn reclaimed_lease_fences_the_stale_token() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let queue = VerifierExecutionQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, _) = declare_service(&mut conn, &actor, "svc").await;
    let verifier = register_verifier(&mut conn, &actor, "drift", custom_drift()).await;
    let binding = bind(&mut conn, &owner, &verifier, Vec::new()).await;
    let execution = scheduled(&queue, &mut conn, binding).await;

    let claimed_at = database_now(&mut conn).await;
    let stale = claim(&queue, &mut conn).await;
    assert!(
        stale.lease_expires_at >= claimed_at + Duration::minutes(5),
        "the database dates the lease from its own clock"
    );
    assert!(
        queue
            .claim(&mut conn, Duration::minutes(5), 1)
            .await
            .expect("claim runs")
            .is_empty(),
        "a live lease is not reclaimable"
    );
    expire_deadlines(&mut conn, execution).await;
    let current = claim(&queue, &mut conn).await;
    assert_eq!(current.lease.execution_id, execution);
    assert_eq!(current.attempt, 2);
    assert_ne!(current.lease.token, stale.lease.token);

    let result = VerificationResultId::new_v7();
    assert_eq!(
        queue
            .complete(
                &mut conn,
                stale.lease,
                result,
                VerificationVerdict::Passed,
                "drift detected",
                Some(DRIFT_COUNTS)
            )
            .await
            .expect("stale completion answers"),
        Settlement::StaleLease
    );
    assert_eq!(
        queue
            .release(&mut conn, stale.lease, Duration::zero())
            .await
            .expect("stale release answers"),
        Settlement::StaleLease
    );
    for _ in 0..2 {
        assert_eq!(
            queue
                .complete(
                    &mut conn,
                    current.lease,
                    result,
                    VerificationVerdict::Passed,
                    "drift detected",
                    Some(DRIFT_COUNTS)
                )
                .await
                .expect("completion answers"),
            Settlement::Applied
        );
    }
    assert_eq!(
        queue
            .complete(
                &mut conn,
                current.lease,
                VerificationResultId::new_v7(),
                VerificationVerdict::Passed,
                "drift detected",
                Some(DRIFT_COUNTS),
            )
            .await
            .expect("conflicting completion answers"),
        Settlement::StaleLease,
        "a settled execution cannot point at a different result"
    );
    let status = queue
        .execution_status(&mut conn, execution)
        .await
        .expect("status reads")
        .expect("execution exists");
    assert_eq!(status.state.status, VerificationExecutionStatus::Completed);
    assert_eq!(status.result_id, Some(result));
    assert_eq!(status.state.error, None);
}

/// A submission's enqueuing cause, including the caller's sampling
/// decision, is returned unchanged by its first claim and by the reclaim of
/// an expired lease, so every attempt links back to the same caller.
///
/// # Panics
/// Panics when the submission is refused, nothing is claimable, or either
/// claim returns a different cause.
#[tokio::test]
async fn every_attempt_claims_the_submissions_cause() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let actor = actor(tenant);
    let queue = VerifierExecutionQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (subject, _) = declare_service(&mut conn, &actor, "svc").await;
    let verifier = register_verifier(&mut conn, &actor, "custom", custom_drift()).await;
    let cause = Cause {
        trace_id: TraceId::from_hex("4bf92f3577b34da6a3ce929d0e0e4736").expect("trace id parses"),
        span_id: SpanId::from_hex("00f067aa0ba902b7").expect("span id parses"),
        sampled: true,
    };
    let caused = Submission::new(
        tenant,
        authority(&actor),
        request(&verifier, &subject),
        Some(cause),
        None,
    )
    .expect("request canonicalizes");
    let execution = submitted(&queue, &mut conn, &caused).await;

    let first = claim(&queue, &mut conn).await;
    assert_eq!(first.cause, Some(cause));
    expire_deadlines(&mut conn, execution).await;
    let reclaimed = claim(&queue, &mut conn).await;
    assert_eq!(reclaimed.attempt, 2);
    assert_eq!(reclaimed.cause, Some(cause));
}

/// A retryable failure backs off 30 then 60 seconds on the same execution and input;
/// the third failure exhausts the budget and settles `errored` with the
/// structured error, no result, and no dispatch.
///
/// # Panics
/// Panics when a retry is claimable early, the backoff differs, or exhaustion
/// does not settle the execution as a verdict-free error.
#[tokio::test]
async fn retries_back_off_then_exhaust_to_errored() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let queue = VerifierExecutionQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, _) = declare_service(&mut conn, &actor, "svc").await;
    let verifier = register_verifier(&mut conn, &actor, "drift", custom_drift()).await;
    let binding = bind(&mut conn, &owner, &verifier, vec![FrozenTarget::Uid(uid())]).await;
    let execution = scheduled(&queue, &mut conn, binding).await;

    for (attempt, delay) in [(1, 30), (2, 60)] {
        let claimed = claim(&queue, &mut conn).await;
        assert_eq!(
            (claimed.lease.execution_id, claimed.attempt),
            (execution, attempt)
        );
        assert!(
            matches!(claimed.input, ExecutionInput::Scheduled { window: w, .. } if w == window()),
            "every attempt judges the same frozen window"
        );
        let retried_at = database_now(&mut conn).await;
        let RetryOutcome::Scheduled(due) = queue
            .retry(&mut conn, claimed.lease, &engine_error())
            .await
            .expect("retry answers")
        else {
            panic!("attempt {attempt} must schedule another attempt");
        };
        assert!(
            due >= retried_at,
            "the database dates the next attempt from its own clock"
        );
        assert_eq!(
            stored_backoff(&mut conn, execution).await,
            Duration::seconds(delay),
            "attempt {attempt} backs off {delay} seconds"
        );
        let status = queue
            .execution_status(&mut conn, execution)
            .await
            .expect("status reads")
            .expect("execution exists");
        assert_eq!(status.state.status, VerificationExecutionStatus::Retrying);
        assert_eq!(status.state.error, Some(engine_error()));
        assert!(
            queue
                .claim(&mut conn, Duration::minutes(5), 1)
                .await
                .expect("early claim runs")
                .is_empty(),
            "a retry is not claimable before its backoff"
        );
        expire_deadlines(&mut conn, execution).await;
    }
    let last = claim(&queue, &mut conn).await;
    assert_eq!(last.attempt, 3);
    assert_eq!(
        queue
            .retry(&mut conn, last.lease, &engine_error())
            .await
            .expect("final retry answers"),
        RetryOutcome::Exhausted
    );
    assert_eq!(
        queue
            .retry(&mut conn, last.lease, &engine_error())
            .await
            .expect("settled retry answers"),
        RetryOutcome::StaleLease
    );
    let status = queue
        .execution_status(&mut conn, execution)
        .await
        .expect("status reads")
        .expect("execution exists");
    assert_eq!(status.state.status, VerificationExecutionStatus::Errored);
    assert_eq!(status.state.error, Some(engine_error()));
    assert_eq!(status.result_id, None);
    assert!(status.state.dispatches.is_empty());
}

/// Cancelled and timed-out executions settle with their error and no result, and a
/// crash on the final attempt is settled `errored` by the next claim instead of
/// being reclaimed past the budget.
///
/// # Panics
/// Panics when a terminal status carries a result or dispatch, or an
/// exhausted expired lease is reclaimed.
#[tokio::test]
async fn terminal_failures_and_expired_final_attempts_carry_no_verdict() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let queue = VerifierExecutionQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, _) = declare_service(&mut conn, &actor, "svc").await;
    let verifier = register_verifier(&mut conn, &actor, "drift", custom_drift()).await;
    let binding = bind(&mut conn, &owner, &verifier, vec![FrozenTarget::Uid(uid())]).await;
    let error = engine_error();

    for (terminal, expected) in [
        (
            TerminalStatus::Cancelled,
            VerificationExecutionStatus::Cancelled,
        ),
        (
            TerminalStatus::TimedOut,
            VerificationExecutionStatus::TimedOut,
        ),
    ] {
        let execution = scheduled(&queue, &mut conn, binding).await;
        let claimed = claim(&queue, &mut conn).await;
        assert_eq!(
            queue
                .terminate(&mut conn, claimed.lease, terminal, &error)
                .await
                .expect("terminate answers"),
            Settlement::Applied
        );
        let status = queue
            .execution_status(&mut conn, execution)
            .await
            .expect("status reads")
            .expect("execution exists");
        assert_eq!(status.state.status, expected);
        assert_eq!(status.result_id, None);
        assert_eq!(status.state.error, Some(error.clone()));
        assert!(status.state.dispatches.is_empty());
    }

    let execution = scheduled(&queue, &mut conn, binding).await;
    for _ in 0..3 {
        claim(&queue, &mut conn).await;
        expire_deadlines(&mut conn, execution).await;
    }
    assert!(
        queue
            .claim(&mut conn, Duration::minutes(5), 1)
            .await
            .expect("claim runs")
            .is_empty(),
        "an exhausted expired lease is not reclaimed"
    );
    let status = queue
        .execution_status(&mut conn, execution)
        .await
        .expect("status reads")
        .expect("execution exists");
    assert_eq!(status.state.status, VerificationExecutionStatus::Errored);
    assert_eq!(
        status.state.error.map(|error| error.code),
        Some("lease_expired".to_owned())
    );
}

/// A failed binding execution creates one pending dispatch per distinct frozen
/// Operator, exactly once across settlement retries; passed binding executions and
/// failed direct executions create none. The status projection lists dispatch
/// delivery state and never exposes a verdict.
///
/// # Panics
/// Panics when dispatch fan-out differs or the status projection carries a
/// verdict.
#[tokio::test]
async fn failed_binding_runs_dispatch_each_distinct_operator_once() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let queue = VerifierExecutionQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, _) = declare_service(&mut conn, &actor, "svc").await;
    let verifier = register_verifier(&mut conn, &actor, "drift", custom_drift()).await;
    let pager = FrozenTarget::Uid(uid());
    let inline = FrozenTarget::Digest("sha256:operator".to_owned());
    let binding = bind(
        &mut conn,
        &owner,
        &verifier,
        vec![pager.clone(), inline.clone(), pager.clone()],
    )
    .await;
    let direct = submission(
        fixture.data_tenant_id(),
        authority(&actor),
        request(&verifier, &owner),
        None,
    );

    let mut settled = Vec::new();
    for (bound, verdict) in [
        (true, VerificationVerdict::Failed),
        (true, VerificationVerdict::Passed),
        (true, VerificationVerdict::Inconclusive),
        (false, VerificationVerdict::Failed),
    ] {
        let execution = if bound {
            scheduled(&queue, &mut conn, binding).await
        } else {
            submitted(&queue, &mut conn, &direct).await
        };
        let claimed = claim(&queue, &mut conn).await;
        assert_eq!(claimed.lease.execution_id, execution);
        let result = VerificationResultId::new_v7();
        for _ in 0..2 {
            assert_eq!(
                queue
                    .complete(
                        &mut conn,
                        claimed.lease,
                        result,
                        verdict,
                        "drift detected",
                        Some(DRIFT_COUNTS)
                    )
                    .await
                    .expect("completion answers"),
                Settlement::Applied
            );
        }
        settled.push(execution);
    }

    let failed = queue
        .execution_status(&mut conn, settled[0])
        .await
        .expect("status reads")
        .expect("execution exists");
    let mut operators: Vec<_> = failed
        .state
        .dispatches
        .iter()
        .map(|dispatch| dispatch.operator.clone())
        .collect();
    operators.sort_by_key(|operator| format!("{operator:?}"));
    let mut expected = vec![pager, inline];
    expected.sort_by_key(|operator| format!("{operator:?}"));
    assert_eq!(operators, expected);
    assert!(failed.state.dispatches.iter().all(|dispatch| {
        dispatch.status == OperatorDispatchStatus::Pending && dispatch.error.is_none()
    }));
    let wire = serde_json::to_value(&failed.state).expect("status serializes");
    assert!(
        wire.get("verdict").is_none(),
        "execution status never carries a verdict"
    );
    for execution in &settled[1..] {
        let status = queue
            .execution_status(&mut conn, *execution)
            .await
            .expect("status reads")
            .expect("execution exists");
        assert_eq!(status.state.status, VerificationExecutionStatus::Completed);
        assert!(status.state.dispatches.is_empty());
    }
    let dispatches: i64 = sqlx::query_scalar("SELECT count(*) FROM wyrd.operator_dispatches")
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("dispatches count");
    assert_eq!(dispatches, 2);
}

/// Releasing a leased execution for shutdown makes it immediately claimable again
/// with the same input and without consuming an attempt.
///
/// # Panics
/// Panics when the released execution is not claimable at once or its attempt
/// counter advanced.
#[tokio::test]
async fn release_requeues_without_consuming_an_attempt() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let queue = VerifierExecutionQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, _) = declare_service(&mut conn, &actor, "svc").await;
    let verifier = register_verifier(&mut conn, &actor, "drift", custom_drift()).await;
    let binding = bind(&mut conn, &owner, &verifier, Vec::new()).await;
    let execution = scheduled(&queue, &mut conn, binding).await;

    let first = claim(&queue, &mut conn).await;
    assert_eq!(
        queue
            .release(&mut conn, first.lease, Duration::zero())
            .await
            .expect("release answers"),
        Settlement::Applied
    );
    let status = queue
        .execution_status(&mut conn, execution)
        .await
        .expect("status reads")
        .expect("execution exists");
    assert_eq!(status.state.status, VerificationExecutionStatus::Pending);
    let again = claim(&queue, &mut conn).await;
    assert_eq!(again.lease.execution_id, execution);
    assert_eq!(again.attempt, 1);
    assert_eq!(again.input, first.input);
}

/// A claim reports how long its execution waited past its due time and how long
/// ago its trigger created it, both measured by the claim statement's
/// PostgreSQL instant, and a reclaim measures its wait from the lease expiry.
///
/// # Panics
/// Panics when a reported interval does not cover the interval the test
/// placed in the database, or exceeds it by more than the test's own runtime.
#[tokio::test]
async fn claim_reports_postgres_measured_queue_wait_and_age() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let queue = VerifierExecutionQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, _) = declare_service(&mut conn, &actor, "svc").await;
    let verifier = register_verifier(&mut conn, &actor, "drift", custom_drift()).await;
    let binding = bind(&mut conn, &owner, &verifier, Vec::new()).await;
    let execution = scheduled(&queue, &mut conn, binding).await;
    sqlx::query(
        "UPDATE wyrd.verifier_executions
            SET created_at = statement_timestamp() - INTERVAL '30 seconds',
                next_attempt_at = statement_timestamp() - INTERVAL '10 seconds'
          WHERE execution_id = $1",
    )
    .bind(execution.as_uuid())
    .execute(&mut **conn.transaction())
    .await
    .expect("execution is backdated");
    let slack = std::time::Duration::from_secs(5);

    let first = claim(&queue, &mut conn).await;
    assert!(
        (std::time::Duration::from_secs(10)..std::time::Duration::from_secs(10) + slack)
            .contains(&first.queue_wait),
        "{:?}",
        first.queue_wait
    );
    assert!(
        (std::time::Duration::from_secs(30)..std::time::Duration::from_secs(30) + slack)
            .contains(&first.age),
        "{:?}",
        first.age
    );

    sqlx::query(
        "UPDATE wyrd.verifier_executions
            SET lease_expires_at = statement_timestamp() - INTERVAL '20 seconds'
          WHERE execution_id = $1",
    )
    .bind(execution.as_uuid())
    .execute(&mut **conn.transaction())
    .await
    .expect("lease expires");
    let reclaimed = claim(&queue, &mut conn).await;
    assert_eq!(reclaimed.attempt, 2);
    assert!(
        (std::time::Duration::from_secs(20)..std::time::Duration::from_secs(20) + slack)
            .contains(&reclaimed.queue_wait),
        "{:?}",
        reclaimed.queue_wait
    );
}

/// Runs, dispatches, and status reads are invisible to another tenant, which
/// can neither claim nor settle them; cross-tenant discovery names only the
/// owning tenant, and queue depth counts every tenant's non-terminal work.
///
/// # Panics
/// Panics when tenant B can read, claim, or settle tenant A's execution, or
/// discovery and depth differ from the committed queue.
#[tokio::test]
async fn queue_state_is_tenant_isolated() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let other = fixture
        .seed_additional_tenant("verifier-executions-other")
        .await
        .expect("second tenant seeds");
    let tenant = fixture.data_tenant_id();
    let actor = actor(tenant);
    let queue = VerifierExecutionQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, principal) = declare_service(&mut conn, &actor, "svc").await;
    let verifier = register_verifier(&mut conn, &actor, "drift", custom_drift()).await;
    let binding = bind(&mut conn, &owner, &verifier, vec![FrozenTarget::Uid(uid())]).await;
    record_machine_authentication(&mut conn, principal)
        .await
        .expect("owner activates");
    let failed = scheduled(&queue, &mut conn, binding).await;
    let claimed = claim(&queue, &mut conn).await;
    queue
        .complete(
            &mut conn,
            claimed.lease,
            VerificationResultId::new_v7(),
            VerificationVerdict::Failed,
            "drift detected",
            Some(DRIFT_COUNTS),
        )
        .await
        .expect("execution completes");
    let pending = scheduled(&queue, &mut conn, binding).await;
    conn.commit().await.expect("tenant A commits");

    let mut foreign = fixture
        .tenant_conn_for(other)
        .await
        .expect("tenant B opens");
    for execution in [failed, pending] {
        assert!(
            queue
                .execution_status(&mut foreign, execution)
                .await
                .expect("foreign status reads")
                .is_none()
        );
    }
    assert!(
        queue
            .binding_status(&mut foreign, binding)
            .await
            .expect("foreign binding reads")
            .is_none()
    );
    assert_eq!(
        queue
            .enqueue_scheduled(
                &mut foreign,
                binding,
                window(),
                &ExecutionAuthority::background(authority(&actor), actor.id),
            )
            .await
            .expect("foreign enqueue answers"),
        EnqueueOutcome::Refused(EnqueueRefusal::BindingNotFound)
    );
    assert!(
        queue
            .claim(&mut foreign, Duration::minutes(5), 1)
            .await
            .expect("foreign claim runs")
            .is_empty()
    );
    assert!(schedule_next_due(&queue, &mut foreign).await.is_none());
    let dispatches: i64 = sqlx::query_scalar("SELECT count(*) FROM wyrd.operator_dispatches")
        .fetch_one(&mut **foreign.transaction())
        .await
        .expect("foreign dispatches count");
    assert_eq!(dispatches, 0);
    drop(foreign);

    let operator = fixture.operator_pool();
    assert_eq!(
        queue
            .tenants_with_runnable_executions(operator)
            .await
            .expect("runnable tenants read"),
        vec![tenant]
    );
    assert!(
        queue
            .tenants_with_due_bindings(operator, 10)
            .await
            .expect("early due tenants read")
            .is_empty(),
        "a cursor armed at the next future boundary is not yet due"
    );
    let mut due_now = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let now = database_now(&mut due_now).await;
    set_cursor(&mut due_now, binding, Some(now - SCHEDULE_CLAIM_DELAY)).await;
    due_now.commit().await.expect("cursor commits");
    assert_eq!(
        queue
            .tenants_with_due_bindings(operator, 10)
            .await
            .expect("due tenants read"),
        vec![tenant]
    );
    let depth = queue.queue_depth(operator).await.expect("depth reads");
    assert_eq!(
        depth.executions,
        QueueCounts {
            pending: 1,
            retrying: 0,
            running: 0
        }
    );
    assert_eq!(
        depth.dispatches,
        QueueCounts {
            pending: 1,
            retrying: 0,
            running: 0
        }
    );
}

/// Binding GET reports the owner gate, generic readiness, cursor, last
/// qualifying exchange, and latest execution; activity lapses after the inactivity
/// window and PSI readiness stays fail-closed.
///
/// # Panics
/// Panics when any reported field differs from the durable state.
#[tokio::test]
async fn binding_status_reports_activity_readiness_and_latest_run() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let queue = VerifierExecutionQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, principal) = declare_service(&mut conn, &actor, "svc").await;
    let custom = register_verifier(&mut conn, &actor, "custom", custom_drift()).await;
    let psi = register_verifier(&mut conn, &actor, "psi", psi_drift()).await;
    let ready = bind(&mut conn, &owner, &custom, Vec::new()).await;
    let unready = bind(&mut conn, &owner, &psi, Vec::new()).await;

    let dormant = queue
        .binding_status(&mut conn, ready)
        .await
        .expect("status reads")
        .expect("binding exists");
    assert!(!dormant.active);
    assert_eq!(dormant.next_run_at, None);
    assert_eq!(dormant.last_activated_at, None);
    assert_eq!(dormant.last_execution_id, None);

    let activated_after = database_now(&mut conn).await;
    record_machine_authentication(&mut conn, principal)
        .await
        .expect("owner activates");
    let mut latest = None;
    for _ in 0..2 {
        latest = Some(scheduled(&queue, &mut conn, ready).await);
    }
    let status = queue
        .binding_status(&mut conn, ready)
        .await
        .expect("status reads")
        .expect("binding exists");
    assert_eq!(status.binding_id, ready);
    assert_eq!(status.owner_card_uid, owner);
    assert_eq!(status.card_uid, owner);
    assert_eq!(status.verifier_card_uid, custom);
    assert!(status.active);
    assert_eq!(status.readiness, VerifierReadiness::Ready);
    assert_next_daily_boundary(status.next_run_at, activated_after);
    assert!(
        status
            .last_activated_at
            .is_some_and(|at| at >= activated_after),
        "the exchange is stamped by the database clock"
    );
    assert_eq!(status.last_execution_id, latest);

    let psi_status = queue
        .binding_status(&mut conn, unready)
        .await
        .expect("status reads")
        .expect("binding exists");
    assert_eq!(psi_status.readiness, VerifierReadiness::BaselineNotReady);
    age_activity(&mut conn, principal, Duration::days(2)).await;
    let lapsed = queue
        .binding_status(&mut conn, ready)
        .await
        .expect("status reads")
        .expect("binding exists");
    assert!(
        !lapsed.active,
        "activity lapses after the inactivity window"
    );
}

/// Every coordination instant the queue writes — creation, availability,
/// lease expiry, retry deadline, settlement, and dispatch creation — is
/// PostgreSQL's own statement instant, and due discovery compares against that
/// same clock rather than a caller-supplied one.
///
/// The queue exposes no way to supply a coordination instant, so the proof is
/// that each stored deadline falls inside a bracket read from the database
/// around the call, offset only by the relative duration the caller asked for.
///
/// # Panics
/// Panics when a stored instant falls outside its database bracket, a backoff
/// differs from the configured delay, a future cursor or one whose window
/// ended less than the claim delay ago is reported due, or a due occurrence's
/// window does not end at its cursor.
#[tokio::test]
async fn database_clock_owns_verifier_queue_deadlines() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let queue = VerifierExecutionQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, principal) = declare_service(&mut conn, &actor, "svc").await;
    let verifier = register_verifier(&mut conn, &actor, "drift", custom_drift()).await;
    let binding = bind(&mut conn, &owner, &verifier, vec![FrozenTarget::Uid(uid())]).await;

    let before_enqueue = database_now(&mut conn).await;
    let execution = scheduled(&queue, &mut conn, binding).await;
    let after_enqueue = database_now(&mut conn).await;
    let (created_at, next_attempt_at, updated_at): (DateTime<Utc>, DateTime<Utc>, DateTime<Utc>) =
        sqlx::query_as(
            "SELECT created_at, next_attempt_at, updated_at \
             FROM wyrd.verifier_executions WHERE execution_id = $1",
        )
        .bind(execution.as_uuid())
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("execution timestamps read");
    for (field, stamp) in [
        ("created_at", created_at),
        ("next_attempt_at", next_attempt_at),
        ("updated_at", updated_at),
    ] {
        assert!(
            (before_enqueue..=after_enqueue).contains(&stamp),
            "{field} {stamp} is outside the database bracket \
             {before_enqueue}..={after_enqueue}"
        );
    }

    let before_claim = database_now(&mut conn).await;
    let claimed = claim(&queue, &mut conn).await;
    let after_claim = database_now(&mut conn).await;
    assert!(
        (before_claim + Duration::minutes(5)..=after_claim + Duration::minutes(5))
            .contains(&claimed.lease_expires_at),
        "the lease deadline is the database instant plus the requested lease"
    );

    let before_retry = database_now(&mut conn).await;
    let RetryOutcome::Scheduled(next_attempt_at) = queue
        .retry(&mut conn, claimed.lease, &engine_error())
        .await
        .expect("retry answers")
    else {
        panic!("the first failure must schedule another attempt");
    };
    let after_retry = database_now(&mut conn).await;
    assert!(
        (before_retry + Duration::seconds(30)..=after_retry + Duration::seconds(30))
            .contains(&next_attempt_at),
        "the retry deadline is the database instant plus the configured backoff"
    );
    assert_eq!(
        stored_backoff(&mut conn, execution).await,
        Duration::seconds(30),
        "the stored backoff is derived entirely inside the database"
    );
    assert!(
        queue
            .claim(&mut conn, Duration::minutes(5), 1)
            .await
            .expect("early claim runs")
            .is_empty(),
        "the database refuses a execution whose own backoff has not elapsed"
    );

    expire_deadlines(&mut conn, execution).await;
    let settled = claim(&queue, &mut conn).await;
    let before_complete = database_now(&mut conn).await;
    assert_eq!(
        queue
            .complete(
                &mut conn,
                settled.lease,
                VerificationResultId::new_v7(),
                VerificationVerdict::Failed,
                "drift detected",
                Some(DRIFT_COUNTS),
            )
            .await
            .expect("completion answers"),
        Settlement::Applied
    );
    let after_complete = database_now(&mut conn).await;
    let settled_at: DateTime<Utc> = sqlx::query_scalar(
        "SELECT settled_at FROM wyrd.verifier_executions WHERE execution_id = $1",
    )
    .bind(execution.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("settled instant reads");
    assert!(
        (before_complete..=after_complete).contains(&settled_at),
        "settlement is stamped by the database clock"
    );
    let dispatched_at: DateTime<Utc> = sqlx::query_scalar(
        "SELECT created_at FROM wyrd.operator_dispatches WHERE execution_id = $1",
    )
    .bind(execution.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("dispatch instant reads");
    assert!(
        (before_complete..=after_complete).contains(&dispatched_at),
        "a dispatch created by settlement is stamped by the database clock"
    );

    record_machine_authentication(&mut conn, principal)
        .await
        .expect("owner activates");
    set_cursor(
        &mut conn,
        binding,
        Some(after_complete + Duration::hours(1)),
    )
    .await;
    assert!(
        schedule_next_due(&queue, &mut conn).await.is_none(),
        "a cursor ahead of the database clock is not due"
    );
    sqlx::query(
        "UPDATE wyrd.verification_bindings SET next_run_at = statement_timestamp() \
          WHERE binding_id = $1",
    )
    .bind(binding.as_uuid())
    .execute(&mut **conn.transaction())
    .await
    .expect("cursor reaches the database clock");
    assert!(
        schedule_next_due(&queue, &mut conn).await.is_none(),
        "a window that has just ended waits out the claim delay"
    );
    let due = database_now(&mut conn).await - SCHEDULE_CLAIM_DELAY;
    set_cursor(&mut conn, binding, Some(due)).await;
    let tick = schedule_next_due(&queue, &mut conn)
        .await
        .expect("a cursor the claim delay behind the database clock is due");
    assert_eq!(tick.due_at, due);
}

/// Settle one failed binding execution of `verifier` on `owner` with two Operators
/// and return the execution.
///
/// # Panics
/// Panics when the execution cannot be enqueued, claimed, or completed.
async fn failed_execution_with_two_dispatches(
    queue: &VerifierExecutionQueue,
    conn: &mut TenantConn<'_>,
    actor: &Principal,
) -> ExecutionId {
    let (owner, _) = declare_service(conn, actor, "svc").await;
    let verifier = register_verifier(conn, actor, "drift", custom_drift()).await;
    let binding = bind(
        conn,
        &owner,
        &verifier,
        vec![
            FrozenTarget::Uid(uid()),
            FrozenTarget::Digest("sha256:operator".to_owned()),
        ],
    )
    .await;
    let execution = scheduled(queue, conn, binding).await;
    let claimed = claim(queue, conn).await;
    let summary = "x".repeat(2 * MAX_SUMMARY_CHARS);
    queue
        .complete(
            conn,
            claimed.lease,
            VerificationResultId::new_v7(),
            VerificationVerdict::Failed,
            &summary,
            Some(DRIFT_COUNTS),
        )
        .await
        .expect("failed completion settles");
    execution
}

/// A failed settlement freezes the bounded failure context on each dispatch,
/// built from the settled execution and its exact Cards.
///
/// # Panics
/// Panics when the stored context is missing a field, carries another
/// verdict, or exceeds the summary bound.
#[tokio::test]
async fn failed_settlement_freezes_the_bounded_failure_context() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let queue = VerifierExecutionQueue::default();
    let mut conn = fixture.tenant_conn().await.expect("tenant connection");
    let execution = failed_execution_with_two_dispatches(&queue, &mut conn, &actor).await;

    let contexts: Vec<(Uuid, Value)> = sqlx::query_as(
        "SELECT dispatch_id, failure_context FROM wyrd.operator_dispatches WHERE execution_id = $1",
    )
    .bind(execution.as_uuid())
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("contexts read");
    assert_eq!(contexts.len(), 2);
    for (dispatch_id, context) in contexts {
        let context: OperatorFailureContext =
            serde_json::from_value(context).expect("context is the closed shape");
        assert_eq!(context.dispatch_id.as_uuid(), dispatch_id);
        assert_eq!(context.execution_id, execution);
        assert_eq!(context.verdict, VerificationVerdict::Failed);
        assert_eq!(context.verifier_ref, "default/drift@1.0.0");
        assert_eq!(context.subject_ref, "default/svc@1.0.0");
        assert_eq!(context.summary.chars().count(), MAX_SUMMARY_CHARS);
        assert_eq!(context.drift, Some(DRIFT_COUNTS));
    }
}

/// Dispatches are claimed under PostgreSQL-owned leases, retried within the
/// attempt budget with a database-computed delay, released without spending
/// an attempt, fenced against stale tokens, delivered independently of their
/// siblings, and failed once the budget or the deadline is spent.
///
/// # Panics
/// Panics when any transition, fence, or ceiling differs.
#[tokio::test]
async fn dispatch_delivery_obeys_budget_deadline_and_fencing() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let executions = VerifierExecutionQueue::default();
    let dispatches = OperatorDispatchQueue::default();
    let mut conn = fixture.tenant_conn().await.expect("tenant connection");
    let execution = failed_execution_with_two_dispatches(&executions, &mut conn, &actor).await;
    conn.commit().await.expect("settlement commits");
    assert_eq!(
        dispatches
            .due_tenants(fixture.operator_pool())
            .await
            .expect("due tenants read"),
        vec![fixture.data_tenant_id()]
    );

    let mut conn = fixture.tenant_conn().await.expect("tenant connection");
    let first = dispatches
        .claim(&mut conn)
        .await
        .expect("claim runs")
        .expect("a dispatch is due");
    let sibling = dispatches
        .claim(&mut conn)
        .await
        .expect("claim runs")
        .expect("the sibling is due");
    assert!(
        dispatches
            .claim(&mut conn)
            .await
            .expect("claim runs")
            .is_none()
    );
    assert_eq!(first.execution_id, execution);
    assert_eq!(first.attempt, 1);
    assert!(first.remaining <= std::time::Duration::from_mins(5));
    assert!(first.remaining > std::time::Duration::from_secs(290));
    assert_eq!(
        dispatches
            .deliver(&mut conn, sibling.lease)
            .await
            .expect("sibling delivers"),
        Settlement::Applied
    );

    let error = engine_error();
    let RetryOutcome::Scheduled(at) = dispatches
        .retry(
            &mut conn,
            first.lease,
            &error,
            std::time::Duration::from_secs(30),
        )
        .await
        .expect("retry answers")
    else {
        panic!("the first failure schedules a retry");
    };
    let written: DateTime<Utc> = sqlx::query_scalar(
        "SELECT updated_at FROM wyrd.operator_dispatches WHERE dispatch_id = $1",
    )
    .bind(first.lease.dispatch_id.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("dispatch reads");
    assert_eq!(at - written, Duration::seconds(30));
    assert_eq!(
        dispatches
            .deliver(&mut conn, first.lease)
            .await
            .expect("stale deliver answers"),
        Settlement::StaleLease,
        "a settled lease cannot deliver"
    );

    for attempt in 2..=3 {
        sqlx::query(
            "UPDATE wyrd.operator_dispatches SET next_attempt_at = statement_timestamp()
              WHERE dispatch_id = $1",
        )
        .bind(first.lease.dispatch_id.as_uuid())
        .execute(&mut **conn.transaction())
        .await
        .expect("retry comes due");
        let again = dispatches
            .claim(&mut conn)
            .await
            .expect("claim runs")
            .expect("the retry is due");
        assert_eq!(again.lease.dispatch_id, first.lease.dispatch_id);
        assert_eq!(again.attempt, attempt);
        if attempt == 2 {
            assert_eq!(
                dispatches
                    .release(&mut conn, again.lease)
                    .await
                    .expect("release applies"),
                Settlement::Applied
            );
            let refunded = dispatches
                .claim(&mut conn)
                .await
                .expect("claim runs")
                .expect("a released dispatch is due at once");
            assert_eq!(refunded.attempt, 2, "release refunds the attempt");
            dispatches
                .retry(&mut conn, refunded.lease, &error, std::time::Duration::ZERO)
                .await
                .expect("retry answers");
        } else {
            assert_eq!(
                dispatches
                    .retry(&mut conn, again.lease, &error, std::time::Duration::ZERO)
                    .await
                    .expect("retry answers"),
                RetryOutcome::Exhausted
            );
        }
    }
    let status = executions
        .execution_status(&mut conn, execution)
        .await
        .expect("status reads")
        .expect("execution exists");
    let mut statuses: Vec<_> = status.state.dispatches.iter().map(|d| d.status).collect();
    statuses.sort_by_key(|status| <&'static str>::from(*status));
    assert_eq!(
        statuses,
        vec![
            OperatorDispatchStatus::Delivered,
            OperatorDispatchStatus::Failed
        ]
    );
    assert_eq!(status.state.status, VerificationExecutionStatus::Completed);
}

/// The largest decimal `Retry-After` a provider can send (`u64::MAX`
/// seconds) settles the retry durably: the delay is bounded by the deadline
/// before PostgreSQL builds the interval, so the dispatch is scheduled exactly
/// at its database-owned deadline with its lease cleared instead of erroring
/// and waiting for lease expiry.
///
/// # Panics
/// Panics when the retry errors, schedules past the deadline, or leaves the
/// dispatch leased.
#[tokio::test]
async fn maximum_retry_after_settles_at_the_deadline() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let executions = VerifierExecutionQueue::default();
    let dispatches = OperatorDispatchQueue::default();
    let mut conn = fixture.tenant_conn().await.expect("tenant connection");
    failed_execution_with_two_dispatches(&executions, &mut conn, &actor).await;
    let claimed = dispatches
        .claim(&mut conn)
        .await
        .expect("claim runs")
        .expect("a dispatch is due");

    let RetryOutcome::Scheduled(at) = dispatches
        .retry(
            &mut conn,
            claimed.lease,
            &engine_error(),
            std::time::Duration::from_secs(u64::MAX),
        )
        .await
        .expect("the maximum Retry-After settles without an interval error")
    else {
        panic!("a first failure within budget schedules a retry");
    };
    let (status, created, lease): (String, DateTime<Utc>, Option<DateTime<Utc>>) = sqlx::query_as(
        "SELECT status, created_at, lease_expires_at
               FROM wyrd.operator_dispatches WHERE dispatch_id = $1",
    )
    .bind(claimed.lease.dispatch_id.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("dispatch reads");
    assert_eq!(status, "retrying");
    assert_eq!(
        at - created,
        Duration::minutes(5),
        "clipped to the deadline"
    );
    assert!(lease.is_none(), "settlement clears the lease");
}

/// A dispatch whose deadline has passed is failed by the next claim and never
/// handed to a worker; the deadline is PostgreSQL's creation time plus the
/// bound duration.
///
/// # Panics
/// Panics when an expired dispatch is claimed or not failed.
#[tokio::test]
async fn expired_dispatch_deadline_fails_without_a_claim() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let executions = VerifierExecutionQueue::default();
    let expired = OperatorDispatchQueue::new(
        3,
        std::time::Duration::ZERO,
        std::time::Duration::from_secs(45),
    );
    let mut conn = fixture.tenant_conn().await.expect("tenant connection");
    let execution = failed_execution_with_two_dispatches(&executions, &mut conn, &actor).await;
    assert!(
        expired
            .claim(&mut conn)
            .await
            .expect("claim runs")
            .is_none()
    );
    let status = executions
        .execution_status(&mut conn, execution)
        .await
        .expect("status reads")
        .expect("execution exists");
    assert!(status.state.dispatches.iter().all(|dispatch| {
        dispatch.status == OperatorDispatchStatus::Failed
            && dispatch.error.as_ref().map(|e| e.code.as_str()) == Some("deadline_exceeded")
    }));
}

/// One pinned snapshot of `group` for `binding` whose one `turns` list
/// holds `executions` with their selected results.
///
/// # Panics
/// Never in practice: the literals match the slot, group, and source grammars.
fn pinned(
    binding: BindingId,
    group: &str,
    executions: &[(ExecutionId, Option<VerificationResultId>)],
) -> PinnedGroup {
    let slot = SlotName::new("turns").expect("valid slot name");
    let values: Vec<Value> = executions
        .iter()
        .map(|(_, result)| result.map_or(Value::Null, |_| json!("passed")))
        .collect();
    PinnedGroup::new(
        binding,
        GroupId::new(group).expect("valid group id"),
        BTreeMap::from([(
            slot.clone(),
            InputDeclaration::Json {
                value: Value::Array(values),
            },
        )]),
        BTreeMap::from([(
            slot,
            GroupSelection {
                reference: source_text(),
                executions: executions.to_vec(),
            },
        )]),
    )
    .expect("snapshot pins")
}

/// The authored source declaration every test group slot reads.
///
/// # Panics
/// Never in practice: the literal matches the source grammar.
fn source_text() -> SourceText {
    serde_json::from_value(json!("verifiers.turn.requirements.promise.outcome"))
        .expect("source text decodes")
}

/// Acknowledged source activity schedules one pending row per matching
/// binding and group, due exactly `after` past database time — a one-year
/// `after` included, and `1d` equal to `24h` across a session time zone's
/// daylight-time change — and consumed by its claim; the same snapshot content
/// enqueues once whatever its status, while distinct snapshots overlap;
/// group verdicts are compared in settlement order — baseline, unchanged,
/// changed — and a replay never recompares. A group refused before
/// admission is one terminal errored execution with no authority, inputs,
/// snapshot, or result, which is never claimed; the database rejects any
/// runnable status, input, window, record, or snapshot field on it.
///
/// # Panics
/// Panics when scheduling, snapshot identity, the pinned input,
/// verdict-change dispatch, or the refused shape differs.
#[tokio::test]
async fn group_snapshots_are_idempotent_and_react_in_settlement_order() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let queue = VerifierExecutionQueue::default();
    let mut conn = fixture.tenant_conn().await.expect("tenant connection");
    let (owner, principal) = declare_service(&mut conn, &actor, "svc").await;
    let source = register_verifier(&mut conn, &actor, "turn", custom_drift()).await;
    let verifier = register_verifier(&mut conn, &actor, "conversation", custom_drift()).await;
    let yearly = register_verifier(&mut conn, &actor, "yearly", custom_drift()).await;
    let daily = register_verifier(&mut conn, &actor, "daily", custom_drift()).await;
    let hourly = register_verifier(&mut conn, &actor, "hourly", custom_drift()).await;
    let slot = SlotName::new("turns").expect("valid slot name");
    let settings = GroupEndSettings {
        after: TriggerDuration::parse("1h").expect("valid duration"),
        sources: BTreeMap::from([(
            slot,
            GroupSource {
                reference: source_text(),
                subject_occurrence_key: OWNER_OCCURRENCE_KEY.to_owned(),
                card_uid: owner.clone(),
                verifier_card_uid: source.clone(),
            },
        )]),
        order_by: None,
    };
    let operator = FrozenTarget::Digest("sha256:operator".to_owned());
    let lasting = |verifier: &CardUid, after: &str| NewBinding {
        subject_occurrence_key: OWNER_OCCURRENCE_KEY.to_owned(),
        card_uid: owner.clone(),
        verifier_card_uid: verifier.clone(),
        trigger: FrozenTarget::Digest(format!("sha256:{after}")),
        operators: Vec::new(),
        activation: BindingActivation::GroupEnd(GroupEndSettings {
            after: TriggerDuration::parse(after).expect("accepted duration"),
            ..settings.clone()
        }),
    };
    let bindings = project_bindings(
        &mut conn,
        &owner,
        &CardKind::Service,
        &[
            NewBinding {
                subject_occurrence_key: OWNER_OCCURRENCE_KEY.to_owned(),
                card_uid: owner.clone(),
                verifier_card_uid: verifier.clone(),
                trigger: FrozenTarget::Digest("sha256:trigger".to_owned()),
                operators: vec![operator],
                activation: BindingActivation::GroupEnd(settings.clone()),
            },
            lasting(&yearly, "365d"),
            lasting(&daily, "1d"),
            lasting(&hourly, "24h"),
        ],
    )
    .await
    .expect("bindings project");
    let (binding, year, day, hours) = (bindings[0], bindings[1], bindings[2], bindings[3]);
    record_machine_authentication(&mut conn, principal)
        .await
        .expect("owner activates");
    assert_eq!(
        cursor(&mut conn, binding).await,
        None,
        "group_end has no cursor"
    );

    let group = GroupId::new("order-1").expect("valid group id");
    let activity = |verifier: &CardUid| GroupActivity {
        owner_card_uid: owner.clone(),
        card_uid: owner.clone(),
        verifier_card_uid: verifier.clone(),
        group_id: group.clone(),
    };
    queue
        .record_group_activity(
            &mut conn,
            &[activity(&source), activity(&source), activity(&verifier)],
        )
        .await
        .expect("activity records");
    queue
        .record_group_activity(&mut conn, &[activity(&source)])
        .await
        .expect("repeated activity moves the row");
    let pending: Vec<(Uuid, String, bool)> = sqlx::query_as(
        "SELECT binding_id, group_id,
                due_at BETWEEN statement_timestamp() + INTERVAL '59 minutes'
                           AND statement_timestamp() + INTERVAL '61 minutes'
           FROM wyrd.verification_group_pending WHERE binding_id = $1",
    )
    .bind(binding.as_uuid())
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("pending rows read");
    assert_eq!(
        pending,
        vec![(binding.as_uuid(), "order-1".to_owned(), true)],
        "one row per binding and group, due after the quiet period"
    );
    let apart: bool = sqlx::query_scalar(
        "SELECT y.due_at - h.due_at = INTERVAL '365 days' - INTERVAL '1 hour'
           FROM wyrd.verification_group_pending h, wyrd.verification_group_pending y
          WHERE h.binding_id = $1 AND y.binding_id = $2",
    )
    .bind(binding.as_uuid())
    .bind(year.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("both rows read");
    assert!(
        apart,
        "one statement schedules both, each exactly its own after"
    );
    // A session zone whose daylight time starts within the next hour, so a
    // calendar day from now lasts 23 hours: accepted `1d` still waits the
    // same 86,400 elapsed seconds as `24h`.
    sqlx::query(
        "SELECT set_config('TimeZone', format('XST0XDT,%s/%s,%s/%s', d, h + 1, d, h + 49), true)
           FROM (SELECT extract(doy FROM statement_timestamp() AT TIME ZONE 'UTC')::int - 1 AS d,
                        extract(hour FROM statement_timestamp() AT TIME ZONE 'UTC')::int AS h) t",
    )
    .execute(&mut **conn.transaction())
    .await
    .expect("session zone sets");
    let crosses: bool = sqlx::query_scalar(
        "SELECT statement_timestamp() + INTERVAL '1 day'
              = statement_timestamp() + INTERVAL '23 hours'",
    )
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("calendar day reads");
    assert!(crosses, "a calendar day in this session is 23 hours");
    queue
        .record_group_activity(&mut conn, &[activity(&source)])
        .await
        .expect("activity records across daylight time");
    let elapsed: (bool, bool) = sqlx::query_as(
        "SELECT d.due_at = c.due_at, d.due_at - h.due_at = INTERVAL '23 hours'
           FROM wyrd.verification_group_pending h, wyrd.verification_group_pending d,
                wyrd.verification_group_pending c
          WHERE h.binding_id = $1 AND d.binding_id = $2 AND c.binding_id = $3",
    )
    .bind(binding.as_uuid())
    .bind(day.as_uuid())
    .bind(hours.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("rows read");
    assert_eq!(
        elapsed,
        (true, true),
        "1d and 24h are both exactly 86,400 seconds past the activity"
    );
    sqlx::query("RESET TimeZone")
        .execute(&mut **conn.transaction())
        .await
        .expect("session zone resets");
    assert!(
        queue
            .claim_due_group(&mut conn)
            .await
            .expect("claim")
            .is_none()
    );
    let expire = "UPDATE wyrd.verification_group_pending
                     SET due_at = statement_timestamp() - INTERVAL '1 second'
                   WHERE binding_id = $1";
    sqlx::query(expire)
        .bind(binding.as_uuid())
        .execute(&mut **conn.transaction())
        .await
        .expect("row expires");
    let due = queue
        .claim_due_group(&mut conn)
        .await
        .expect("claim")
        .expect("an expired group is due");
    assert_eq!(
        (
            due.binding_id,
            &due.group_id,
            &due.settings,
            due.owner,
            due.active
        ),
        (binding, &group, &settings, Some(principal), true)
    );
    queue
        .consume_group(&mut conn, &due)
        .await
        .expect("row consumed");
    assert!(
        queue
            .claim_due_group(&mut conn)
            .await
            .expect("claim")
            .is_none()
    );

    let authority = background(&mut conn, principal).await;
    let (one, two) = (ExecutionId::new_v7(), ExecutionId::new_v7());
    let first = pinned(binding, "order-1", &[(one, None)]);
    let enqueue = async |conn: &mut TenantConn<'_>, pinned: &PinnedGroup| {
        queue
            .enqueue_group(conn, binding, pinned, &authority)
            .await
            .expect("group enqueue answers")
    };
    let EnqueueOutcome::Enqueued(earlier) = enqueue(&mut conn, &first).await else {
        panic!("the first snapshot enqueues");
    };
    assert_eq!(
        enqueue(&mut conn, &pinned(binding, "order-1", &[(one, None)])).await,
        EnqueueOutcome::AlreadyEnqueued(earlier),
        "unchanged content is the same execution"
    );
    let result = VerificationResultId::new_v7();
    let later = pinned(binding, "order-1", &[(one, Some(result)), (two, None)]);
    assert_ne!(later.digest(), first.digest());
    assert_ne!(
        pinned(
            binding,
            "order-1",
            &[(one, Some(VerificationResultId::new_v7()))]
        )
        .digest(),
        pinned(binding, "order-1", &[(one, Some(result))]).digest(),
        "a different visible result is different content"
    );
    let EnqueueOutcome::Enqueued(overlapping) = enqueue(&mut conn, &later).await else {
        panic!("a distinct snapshot enqueues while the first is outstanding");
    };

    let claimed_earlier = claim(&queue, &mut conn).await;
    let claimed_later = claim(&queue, &mut conn).await;
    assert_eq!(claimed_earlier.lease.execution_id, earlier);
    assert_eq!(claimed_later.lease.execution_id, overlapping);
    assert_eq!(claimed_earlier.origin, ExecutionOrigin::Group);
    assert_eq!(claimed_earlier.group_id.as_ref(), Some(&group));
    assert_eq!(
        claimed_earlier.input,
        ExecutionInput::Grouped {
            inputs: first.inputs.clone(),
            snapshot: first.snapshot.clone(),
        }
    );
    let complete =
        async |conn: &mut TenantConn<'_>, claimed: &ClaimedExecution, result, verdict| {
            queue
                .complete(conn, claimed.lease, result, verdict, "verification", None)
                .await
                .expect("group settles")
        };
    // The later snapshot settles first, so it is the baseline.
    let (baseline, preceding) = (
        VerificationResultId::new_v7(),
        VerificationResultId::new_v7(),
    );
    complete(
        &mut conn,
        &claimed_later,
        baseline,
        VerificationVerdict::Failed,
    )
    .await;
    complete(
        &mut conn,
        &claimed_earlier,
        preceding,
        VerificationVerdict::Failed,
    )
    .await;
    let third = pinned(
        binding,
        "order-1",
        &[(one, Some(result)), (two, Some(result))],
    );
    enqueue(&mut conn, &third).await;
    let changed = claim(&queue, &mut conn).await;
    let current = VerificationResultId::new_v7();
    for _ in 0..2 {
        complete(&mut conn, &changed, current, VerificationVerdict::Passed).await;
    }
    complete(
        &mut conn,
        &claimed_later,
        baseline,
        VerificationVerdict::Failed,
    )
    .await;

    for execution in [earlier, overlapping] {
        let status = queue
            .execution_status(&mut conn, execution)
            .await
            .expect("status reads")
            .expect("execution exists");
        assert!(
            status.state.dispatches.is_empty(),
            "baseline, unchanged, and replayed verdicts do not react"
        );
    }
    let status = queue
        .execution_status(&mut conn, changed.lease.execution_id)
        .await
        .expect("status reads")
        .expect("execution exists");
    assert_eq!(status.state.group_id.as_ref(), Some(&group));
    assert_eq!(status.state.owner_card_uid.as_ref(), Some(&owner));
    assert_eq!(status.state.group.as_ref(), Some(&third.snapshot));
    assert_eq!(status.state.dispatches.len(), 1);
    let contexts: Vec<Value> = sqlx::query_scalar(
        "SELECT failure_context FROM wyrd.operator_dispatches WHERE execution_id = $1",
    )
    .bind(changed.lease.execution_id.as_uuid())
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("contexts read");
    let context: OperatorFailureContext =
        serde_json::from_value(contexts[0].clone()).expect("context is the closed shape");
    assert_eq!(context.result_id, current);
    assert_eq!(context.verdict, VerificationVerdict::Passed);
    let change = context.group.clone().expect("a group change context");
    assert_eq!(change.group_id, group);
    assert_eq!(change.previous_verdict, VerificationVerdict::Failed);
    assert_eq!(
        change.previous_result_id, preceding,
        "compared with the latest settled result, not the latest admitted"
    );
    assert_ne!(change.previous_result_id, baseline);
    assert_eq!(
        context.render("{{group.previous_verdict}}->{{verdict}}"),
        Ok("failed->passed".to_owned())
    );

    let refused_group = GroupId::new("refused-1").expect("valid group id");
    queue
        .record_group_activity(
            &mut conn,
            &[GroupActivity {
                group_id: refused_group.clone(),
                ..activity(&source)
            }],
        )
        .await
        .expect("activity records");
    sqlx::query(expire)
        .bind(binding.as_uuid())
        .execute(&mut **conn.transaction())
        .await
        .expect("row expires");
    let due = queue
        .claim_due_group(&mut conn)
        .await
        .expect("claim")
        .expect("the refused group is due");
    assert_eq!(due.group_id, refused_group);
    let system = authority.attribution().act_principal_id;
    let error = VerificationError {
        code: "WYRD_AUTH_401_CREDENTIAL_REVOKED".to_owned(),
        message: "the binding owner is not runtime-active".to_owned(),
    };
    let refused = queue
        .refuse_group(&mut conn, &due, principal, system, &error)
        .await
        .expect("the refusal records");
    queue
        .consume_group(&mut conn, &due)
        .await
        .expect("row consumed");
    let shape: (String, bool, bool, bool, bool, bool, bool, bool, i32) = sqlx::query_as(
        "SELECT status, authority IS NULL, inputs IS NULL, group_snapshot_sha256 IS NULL,
                group_sources IS NULL, result_id IS NULL, settled_at IS NOT NULL,
                lease_token IS NULL AND next_attempt_at IS NULL, attempts
           FROM wyrd.verifier_executions WHERE execution_id = $1",
    )
    .bind(refused.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("the refusal reads");
    assert_eq!(
        shape,
        (
            "errored".to_owned(),
            true,
            true,
            true,
            true,
            true,
            true,
            true,
            0
        ),
        "terminal, authority-free, unpinned, and unleased"
    );
    let status = queue
        .execution_status(&mut conn, refused)
        .await
        .expect("status reads")
        .expect("the refusal exists");
    assert_eq!(status.state.status, VerificationExecutionStatus::Errored);
    assert_eq!(status.state.error.as_ref(), Some(&error));
    assert_eq!(status.state.group_id.as_ref(), Some(&refused_group));
    assert_eq!(status.state.owner_card_uid.as_ref(), Some(&owner));
    assert_eq!(status.state.group, None);
    assert!(status.state.dispatches.is_empty(), "no Operator reacts");
    assert_eq!(
        status.attribution,
        Attribution::new(principal, system),
        "the owner's stored principal, recorded by SYSTEM"
    );
    assert!(
        queue
            .claim(&mut conn, Duration::minutes(5), 1)
            .await
            .expect("claim runs")
            .is_empty(),
        "a refusal is never claimed"
    );
    assert!(
        queue
            .claim_due_group(&mut conn)
            .await
            .expect("claim")
            .is_none(),
        "the refusal consumed its group"
    );
    // An otherwise valid refusal admits no runnable status or any input,
    // window, record, or snapshot field, malformed or not.
    for change in [
        "UPDATE wyrd.verifier_executions
            SET status = 'pending', error = NULL, settled_at = NULL,
                next_attempt_at = statement_timestamp()
          WHERE execution_id = $1",
        "UPDATE wyrd.verifier_executions
            SET inputs = '{}', route_overrides = '{}' WHERE execution_id = $1",
        "UPDATE wyrd.verifier_executions
            SET window_start = statement_timestamp() - INTERVAL '1 hour',
                window_end = statement_timestamp()
          WHERE execution_id = $1",
        "UPDATE wyrd.verifier_executions
            SET input_record_id = 'record', input_event_time = statement_timestamp()
          WHERE execution_id = $1",
        "UPDATE wyrd.verifier_executions
            SET group_snapshot_sha256 = '\\x00' WHERE execution_id = $1",
        "UPDATE wyrd.verifier_executions
            SET group_sources = '[]' WHERE execution_id = $1",
    ] {
        let mut attempt = sqlx::Connection::begin(&mut **conn.transaction())
            .await
            .expect("nested transaction begins");
        let error = sqlx::query(change)
            .bind(refused.as_uuid())
            .execute(&mut *attempt)
            .await
            .expect_err("the refused shape rejects the change");
        assert!(
            error
                .to_string()
                .contains("verifier_executions_refused_shape"),
            "{change}: {error}"
        );
        attempt
            .rollback()
            .await
            .expect("nested transaction rolls back");
    }
}

/// A verification's target reads both Cards, the Verifier's readiness, and
/// the SYSTEM principal in one statement, only within the bound tenant, and
/// refuses a connection already bound to another tenant.
///
/// # Panics
/// Panics when the read misses a Card, the readiness, or the principal, sees
/// another tenant's rows, or rebinds a connection bound to another tenant.
#[tokio::test]
async fn a_verification_target_reads_its_tenant_in_one_statement() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let actor = actor(tenant);
    let mut setup = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let system = provision_system_principal(&mut setup, Attribution::unresolved())
        .await
        .expect("the tenant SYSTEM principal provisions");
    let (owner, _) = declare_service(&mut setup, &actor, "svc").await;
    let verifier = register_verifier(&mut setup, &actor, "assert", drift("psi")).await;
    setup.commit().await.expect("setup commits");

    let mut conn = fixture
        .wyrd_postgres()
        .conn()
        .await
        .expect("plain connection");
    let target = VerificationTargets::new(&mut conn, tenant)
        .read(&verifier, &owner)
        .await
        .expect("target reads");
    assert_eq!(target.readiness, VerifierReadiness::Ready);
    assert_eq!(target.refusal(), None);
    assert_eq!(
        target.verifier.map(|row| row.card_uid),
        Some(verifier.as_uuid())
    );
    assert_eq!(
        target.subject.map(|row| row.card_uid),
        Some(owner.as_uuid())
    );
    assert_eq!(target.system_principal, Some(system));

    let itself = VerificationTargets::new(&mut conn, tenant)
        .read(&verifier, &verifier)
        .await
        .expect("a self-target reads");
    assert!(itself.verifier.is_some() && itself.subject.is_some());

    let other = fixture
        .seed_additional_tenant(&format!("target-{}", Uuid::now_v7()))
        .await
        .expect("second tenant seeds");
    let foreign = VerificationTargets::new(&mut conn, other)
        .read(&verifier, &owner)
        .await
        .expect("another tenant reads");
    assert!(foreign.verifier.is_none() && foreign.subject.is_none());
    assert_eq!(
        foreign.refusal(),
        Some(EnqueueRefusal::NotReady(
            VerifierReadiness::VerifierUnavailable
        ))
    );

    let mut bound = fixture
        .tenant_conn()
        .await
        .expect("bound transaction opens");
    let refused = VerificationTargets::new(bound.transaction(), other)
        .read(&verifier, &owner)
        .await;
    assert!(
        refused.is_err(),
        "a connection bound to another tenant is refused"
    );
}

/// One claim statement takes up to its limit of runnable executions, each
/// under its own lease token, and never more than are runnable.
///
/// # Panics
/// Panics when a claim takes more than its limit, repeats an execution or a
/// lease token, or leaves a runnable execution unclaimed.
#[tokio::test]
async fn a_claim_takes_up_to_its_limit_in_one_statement() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let actor = actor(tenant);
    let queue = VerifierExecutionQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, _) = declare_service(&mut conn, &actor, "svc").await;
    let verifier = register_verifier(&mut conn, &actor, "drift", custom_drift()).await;
    let mut submitted_ids = BTreeSet::new();
    for _ in 0..3 {
        let sub = submission(tenant, authority(&actor), request(&verifier, &owner), None);
        submitted_ids.insert(submitted(&queue, &mut conn, &sub).await);
    }

    let first = queue
        .claim(&mut conn, Duration::minutes(5), 2)
        .await
        .expect("claim runs");
    let rest = queue
        .claim(&mut conn, Duration::minutes(5), 5)
        .await
        .expect("claim runs");
    let drained = queue
        .claim(&mut conn, Duration::minutes(5), 5)
        .await
        .expect("claim runs");

    assert_eq!((first.len(), rest.len()), (2, 1));
    assert!(drained.is_empty(), "nothing is left to claim");
    let leases: Vec<_> = first.iter().chain(&rest).map(|c| c.lease).collect();
    let claimed_ids: BTreeSet<_> = leases.iter().map(|l| l.execution_id).collect();
    assert_eq!(
        claimed_ids, submitted_ids,
        "every execution is claimed once"
    );
    for (i, lease) in leases.iter().enumerate() {
        assert!(
            leases[i + 1..]
                .iter()
                .all(|other| other.token != lease.token),
            "each claim carries its own lease token"
        );
    }
}

/// One batch decides its submissions in order against each other, not only
/// against committed rows: a later submission reusing an earlier one's key
/// for a different request conflicts with it, and a repeated execution ID
/// replays, though neither earlier row was written when they were decided.
///
/// # Panics
/// Panics when an outcome, or a stored status or error code, differs.
#[tokio::test]
async fn a_batch_decides_its_submissions_against_each_other() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let actor = actor(tenant);
    let queue = VerifierExecutionQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, _) = declare_service(&mut conn, &actor, "svc").await;
    let verifier = register_verifier(&mut conn, &actor, "drift", custom_drift()).await;
    let mut changed = request(&verifier, &owner);
    changed.run_id = Some(RunId::from_string("app-run-batch".to_owned()));
    let keyed = submission(
        tenant,
        authority(&actor),
        request(&verifier, &owner),
        Some("k"),
    );
    let conflicting = submission(tenant, authority(&actor), changed, Some("k"));
    let unkeyed = submission(tenant, authority(&actor), request(&verifier, &owner), None);

    let outcomes = queue
        .accept_submissions(
            &mut conn,
            &[
                keyed.clone(),
                conflicting.clone(),
                unkeyed.clone(),
                unkeyed.clone(),
            ],
            &HashMap::new(),
        )
        .await
        .expect("batch accepts");

    assert_eq!(
        outcomes,
        vec![
            SubmissionOutcome::Accepted,
            SubmissionOutcome::Conflict {
                original: keyed.execution_id
            },
            SubmissionOutcome::Accepted,
            SubmissionOutcome::Replayed(unkeyed.execution_id),
        ]
    );
    for pending in [keyed.execution_id, unkeyed.execution_id] {
        assert_eq!(
            status(&queue, &mut conn, pending).await.state.status,
            VerificationExecutionStatus::Pending
        );
    }
    let conflict = status(&queue, &mut conn, conflicting.execution_id)
        .await
        .state;
    assert_eq!(
        conflict.error.map(|error| error.code),
        Some("WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT".to_owned())
    );
}

/// One batch completes many executions with exactly the effects of
/// completing each alone: a failed binding run dispatches each of its
/// Operators once, a passed submission dispatches nothing, a completion whose
/// lease no longer holds is stale and changes nothing, and replaying the
/// batch re-applies idempotently without new dispatches.
///
/// # Panics
/// Panics when a settlement, status, or dispatch count differs.
#[tokio::test]
async fn a_batch_completes_many_executions_like_complete() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    let actor = actor(tenant);
    let queue = VerifierExecutionQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, _) = declare_service(&mut conn, &actor, "svc").await;
    let verifier = register_verifier(&mut conn, &actor, "drift", custom_drift()).await;
    let binding = bind(
        &mut conn,
        &owner,
        &verifier,
        vec![
            FrozenTarget::Uid(uid()),
            FrozenTarget::Digest("sha256:operator".to_owned()),
        ],
    )
    .await;
    let failed = scheduled(&queue, &mut conn, binding).await;
    let failed_claim = claim(&queue, &mut conn).await;
    assert_eq!(failed_claim.lease.execution_id, failed);
    let passed = submitted(
        &queue,
        &mut conn,
        &submission(tenant, authority(&actor), request(&verifier, &owner), None),
    )
    .await;
    let passed_claim = claim(&queue, &mut conn).await;
    let held = submitted(
        &queue,
        &mut conn,
        &submission(tenant, authority(&actor), request(&verifier, &owner), None),
    )
    .await;
    let held_claim = claim(&queue, &mut conn).await;
    assert_eq!(
        (
            passed_claim.lease.execution_id,
            held_claim.lease.execution_id
        ),
        (passed, held)
    );

    let completion = |lease: ExecutionLease, verdict| Completion {
        lease,
        result_id: VerificationResultId::new_v7(),
        verdict,
        summary: "drift detected".to_owned(),
        drift: Some(DRIFT_COUNTS),
    };
    let batch = vec![
        completion(failed_claim.lease, VerificationVerdict::Failed),
        completion(passed_claim.lease, VerificationVerdict::Passed),
        completion(
            ExecutionLease {
                execution_id: held,
                token: passed_claim.lease.token,
            },
            VerificationVerdict::Passed,
        ),
    ];
    for _ in 0..2 {
        assert_eq!(
            queue
                .complete_many(&mut conn, &batch)
                .await
                .expect("batch completes"),
            vec![
                Settlement::Applied,
                Settlement::Applied,
                Settlement::StaleLease
            ]
        );
    }

    for execution in [failed, passed] {
        assert_eq!(
            status(&queue, &mut conn, execution).await.state.status,
            VerificationExecutionStatus::Completed
        );
    }
    assert_eq!(
        status(&queue, &mut conn, held).await.state.status,
        VerificationExecutionStatus::Running,
        "a stale completion changes nothing"
    );
    assert_eq!(
        status(&queue, &mut conn, failed)
            .await
            .state
            .dispatches
            .len(),
        2
    );
    let dispatches: i64 = sqlx::query_scalar("SELECT count(*) FROM wyrd.operator_dispatches")
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("dispatches count");
    assert_eq!(
        dispatches, 2,
        "only the failed binding run dispatches, once"
    );
}
