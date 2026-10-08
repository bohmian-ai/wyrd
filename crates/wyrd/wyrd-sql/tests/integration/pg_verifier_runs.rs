//! PgFixture coverage for the durable Verifier run queue: shared enqueue,
//! scheduler claims, leased execution, token-fenced settlement, Operator
//! dispatch creation, status reads, and tenant isolation.
//!
//! Every test runs against the repository managed Postgres through tenant
//! connections, so forced RLS applies. Each fixture owns its own database, so
//! cross-tenant counts are exact.

use chrono::{DateTime, Duration, TimeZone, Utc};
use serde_json::Value;
use uuid::Uuid;
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_runtime::PermissionSet;
use wyrd_runtime::principal::{Principal, PrincipalId, PrincipalKind};
use wyrd_spec::DataTenantId;
use wyrd_spec::card::operator::{MAX_SUMMARY_CHARS, OperatorFailureContext, VerifierCounts};
use wyrd_spec::card::verifier::OWNER_OCCURRENCE_KEY;
use wyrd_spec::envelope::{Card, CardKind};
use wyrd_spec::ids::{BindingId, CardUid, VerificationResultId, VerificationRunId};
use wyrd_spec::registry::RegistrationOperationId;
use wyrd_spec::verification::{
    DriftWindow, OperatorDispatchStatus, VerificationError, VerificationExecutionStatus,
    VerificationRunTarget, VerificationVerdict, VerifierReadiness,
};
use wyrd_sql::TenantConn;
use wyrd_sql::queries::cards::{
    NewCardRow, NewRegistrationOperation, insert_card_row, insert_registration_operation,
    upsert_service_account_from_card,
};
use wyrd_sql::queries::operator_dispatches::OperatorDispatchQueue;
use wyrd_sql::queries::verification::{
    BindingActivation, FrozenTarget, NewBinding, project_bindings, record_machine_authentication,
};
use wyrd_sql::queries::verifier_runs::{
    ClaimedRun, EnqueueOutcome, EnqueueRefusal, ManualEnqueueOutcome, ObservationRecord,
    QueueCounts, RequestKey, RetryOutcome, RunInput, RunOrigin, RunRequest, ScheduleOutcome,
    ScheduleSkip, Settlement, StagedBatch, StagedResult, TerminalStatus, TraceWaitOutcome,
    VerifierRunQueue,
};
use wyrd_sql::row_types::cards::CardStatus;

/// Counts every test completion freezes, matching a one-of-three drift report.
const DRIFT_COUNTS: VerifierCounts = VerifierCounts::Drift {
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

/// The fixed manual Drift window every manual test run analyzes.
fn window() -> DriftWindow {
    DriftWindow {
        start: at(1, 0, 0),
        end: at(2, 0, 0),
    }
}

/// A registering user principal in `tenant`, which also serves as a manual
/// run requester.
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

/// A ready Custom Drift Verifier implementation.
fn custom_drift() -> Value {
    serde_json::json!({ "implementation": { "kind": "drift", "spec": { "method": "Custom" } } })
}

/// A PSI Drift Verifier implementation, never ready until a baseline exists.
fn psi_drift() -> Value {
    serde_json::json!({ "implementation": { "kind": "drift", "spec": { "method": "Psi" } } })
}

/// A ready Eval Verifier implementation.
fn eval() -> Value {
    serde_json::json!({ "implementation": { "kind": "eval", "spec": { "tasks": {} } } })
}

/// Insert one Card row named `name` and return its UID.
///
/// Every row starts as a Service Card because only the stored kind, spec,
/// and status matter to the queue; callers rewrite them as needed.
///
/// # Panics
/// Panics when the registration operation or Card row fails to insert.
async fn insert_card(conn: &mut TenantConn<'_>, actor: &Principal, card: &Card) -> CardUid {
    let operation_id = RegistrationOperationId::new(Uuid::now_v7());
    let key = Uuid::now_v7().to_string();
    insert_registration_operation(
        conn,
        NewRegistrationOperation {
            operation_id,
            principal_id: actor.id,
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
            principal_id: actor.id,
            operation_id,
            status: CardStatus::Pending,
            spec_hash: "spec-hash",
            artifact_hash: None,
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
    serde_json::from_value(serde_json::json!({
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
async fn register_service(
    conn: &mut TenantConn<'_>,
    actor: &Principal,
    name: &str,
) -> (CardUid, PrincipalId) {
    let card = service_card(name);
    let card_uid = insert_card(conn, actor, &card).await;
    let principal = upsert_service_account_from_card(conn, &card_uid, &card, actor)
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

/// Project one owner-level binding of `verifier` on `owner` with `operators`.
///
/// # Panics
/// Panics when the projection fails.
async fn bind(
    conn: &mut TenantConn<'_>,
    owner: &CardUid,
    verifier: &CardUid,
    activation: BindingActivation,
    operators: Vec<FrozenTarget>,
) -> BindingId {
    project_bindings(
        conn,
        owner,
        &CardKind::Service,
        &[NewBinding {
            subject_occurrence_key: OWNER_OCCURRENCE_KEY.to_owned(),
            subject_card_uid: owner.clone(),
            verifier_uid: verifier.clone(),
            trigger: FrozenTarget::Digest("sha256:trigger".to_owned()),
            operators,
            activation,
        }],
    )
    .await
    .expect("binding projects")[0]
}

/// Daily-at-02:00-UTC activation.
fn daily() -> BindingActivation {
    BindingActivation::Schedule {
        cron: "0 2 * * *".to_owned(),
        tz: None,
    }
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

/// Bring `run`'s lease and retry deadlines due by moving whichever of them is
/// set to the database's current instant.
///
/// This is how a test reaches an expired lease or an elapsed backoff without a
/// process clock to advance: the row under test is placed in the past and
/// PostgreSQL still decides whether it is due.
///
/// # Panics
/// Panics when the update fails.
async fn expire_deadlines(conn: &mut TenantConn<'_>, run: VerificationRunId) {
    sqlx::query(
        "UPDATE wyrd.verifier_runs
            SET lease_expires_at = CASE WHEN lease_expires_at IS NULL
                                        THEN NULL ELSE statement_timestamp() END,
                next_attempt_at = CASE WHEN next_attempt_at IS NULL
                                       THEN NULL ELSE statement_timestamp() END
          WHERE run_id = $1",
    )
    .bind(run.as_uuid())
    .execute(&mut **conn.transaction())
    .await
    .expect("deadlines expire");
}

/// Read the backoff the queue stored for `run`: the gap between the retry's
/// own write instant and the attempt deadline it scheduled.
///
/// Both timestamps come from the same `statement_timestamp()`, so the gap is
/// exactly the configured backoff regardless of when the test reads it.
///
/// # Panics
/// Panics when the run row cannot be read or is not awaiting a retry.
async fn stored_backoff(conn: &mut TenantConn<'_>, run: VerificationRunId) -> Duration {
    let (attempt_at, updated_at): (DateTime<Utc>, DateTime<Utc>) = sqlx::query_as(
        "SELECT next_attempt_at, updated_at FROM wyrd.verifier_runs WHERE run_id = $1",
    )
    .bind(run.as_uuid())
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

/// Count the tenant's runs.
///
/// # Panics
/// Panics when the count cannot be read.
async fn run_count(conn: &mut TenantConn<'_>) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM wyrd.verifier_runs")
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("runs count")
}

/// Unwrap a newly created run.
///
/// # Panics
/// Panics when `outcome` is not [`EnqueueOutcome::Enqueued`].
fn enqueued(outcome: EnqueueOutcome) -> VerificationRunId {
    match outcome {
        EnqueueOutcome::Enqueued(run_id) => run_id,
        other => panic!("expected a new run, got {other:?}"),
    }
}

/// Manual request for a binding by `requester`.
fn manual_binding(requester: &Principal, binding_id: BindingId) -> RunRequest {
    RunRequest::Manual {
        requested_by: requester.id,
        target: VerificationRunTarget::Binding { binding_id },
        window: window(),
    }
}

/// A retryable engine error.
fn engine_error() -> VerificationError {
    VerificationError {
        code: "engine_unavailable".to_owned(),
        message: "the analysis engine did not respond".to_owned(),
    }
}

/// Claim the next runnable run, which must exist.
///
/// # Panics
/// Panics when the claim fails or nothing is runnable.
async fn claim(queue: &VerifierRunQueue, conn: &mut TenantConn<'_>) -> ClaimedRun {
    queue
        .claim(conn, Duration::minutes(5))
        .await
        .expect("claim runs")
        .expect("a run is runnable")
}

/// A binding run freezes the owner, binding, Trigger, Operators, window, and
/// requester; a direct run freezes only the Verifier, subject, window, and
/// requester. Both claims expose exactly those identities.
///
/// # Panics
/// Panics when a run is refused or any frozen identity differs.
#[tokio::test]
async fn manual_enqueue_freezes_binding_and_direct_identities() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let queue = VerifierRunQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, _) = register_service(&mut conn, &actor, "svc").await;
    let verifier = register_verifier(&mut conn, &actor, "drift", custom_drift()).await;
    let operator = FrozenTarget::Uid(uid());
    let binding = bind(
        &mut conn,
        &owner,
        &verifier,
        daily(),
        vec![operator.clone()],
    )
    .await;

    let bound = enqueued(
        queue
            .enqueue(&mut conn, &manual_binding(&actor, binding))
            .await
            .expect("binding run enqueues"),
    );
    let direct_request = RunRequest::Manual {
        requested_by: actor.id,
        target: VerificationRunTarget::Verifier {
            verifier_uid: verifier.clone(),
            subject_card_uid: owner.clone(),
        },
        window: window(),
    };
    let direct = enqueued(
        queue
            .enqueue(&mut conn, &direct_request)
            .await
            .expect("direct run enqueues"),
    );
    assert_ne!(bound, direct);

    let first = claim(&queue, &mut conn).await;
    let second = claim(&queue, &mut conn).await;
    let (bound_claim, direct_claim) = if first.lease.run_id == bound {
        (first, second)
    } else {
        (second, first)
    };
    assert_eq!(bound_claim.origin, RunOrigin::Manual);
    assert_eq!(bound_claim.binding_id, Some(binding));
    assert_eq!(bound_claim.owner_card_uid, Some(owner.clone()));
    assert_eq!(
        bound_claim.trigger,
        Some(FrozenTarget::Digest("sha256:trigger".to_owned()))
    );
    assert_eq!(bound_claim.verifier_uid, verifier);
    assert_eq!(bound_claim.verifier_version, "1.0.0");
    assert_eq!(bound_claim.subject_card_uid, owner);
    assert_eq!(bound_claim.input, RunInput::DriftWindow(window()));
    assert_eq!(bound_claim.requested_by, Some(actor.id));
    assert_eq!((bound_claim.attempt, bound_claim.max_attempts), (1, 3));

    assert_eq!(direct_claim.lease.run_id, direct);
    assert_eq!(direct_claim.binding_id, None);
    assert_eq!(direct_claim.owner_card_uid, None);
    assert_eq!(direct_claim.trigger, None);
    assert_eq!(direct_claim.requested_by, Some(actor.id));
    let operators: Value =
        sqlx::query_scalar("SELECT operators FROM wyrd.verifier_runs WHERE run_id = $1")
            .bind(direct.as_uuid())
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("operators read");
    assert_eq!(
        operators,
        serde_json::json!([]),
        "a direct run has no Operators"
    );
}

/// Unknown bindings, missing or inactive Verifiers, unready PSI baselines,
/// missing subjects, and implementation/input mismatches are refused with a
/// typed reason and create nothing.
///
/// # Panics
/// Panics when any request is accepted or refused for the wrong reason.
#[tokio::test]
async fn enqueue_refuses_unrunnable_targets_without_writing() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let queue = VerifierRunQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, _) = register_service(&mut conn, &actor, "svc").await;
    let custom = register_verifier(&mut conn, &actor, "custom", custom_drift()).await;
    let psi = register_verifier(&mut conn, &actor, "psi", psi_drift()).await;
    let eval_verifier = register_verifier(&mut conn, &actor, "eval", eval()).await;
    let psi_binding = bind(&mut conn, &owner, &psi, daily(), Vec::new()).await;
    let direct = |verifier: &CardUid, subject: &CardUid| RunRequest::Manual {
        requested_by: actor.id,
        target: VerificationRunTarget::Verifier {
            verifier_uid: verifier.clone(),
            subject_card_uid: subject.clone(),
        },
        window: window(),
    };

    let cases = [
        (
            manual_binding(&actor, BindingId::new_v7()),
            EnqueueRefusal::BindingNotFound,
        ),
        (
            manual_binding(&actor, psi_binding),
            EnqueueRefusal::NotReady(VerifierReadiness::BaselineNotReady),
        ),
        (
            direct(&uid(), &owner),
            EnqueueRefusal::NotReady(VerifierReadiness::VerifierUnavailable),
        ),
        (
            direct(&owner, &owner),
            EnqueueRefusal::NotReady(VerifierReadiness::VerifierUnavailable),
        ),
        (direct(&custom, &uid()), EnqueueRefusal::SubjectUnavailable),
        (
            direct(&eval_verifier, &owner),
            EnqueueRefusal::InputMismatch,
        ),
    ];
    for (request, refusal) in cases {
        assert_eq!(
            queue
                .enqueue(&mut conn, &request)
                .await
                .expect("enqueue answers"),
            EnqueueOutcome::Refused(refusal)
        );
    }

    sqlx::query("UPDATE wyrd.cards SET status = 'deleted' WHERE card_uid = $1")
        .bind(custom.as_uuid())
        .execute(&mut **conn.transaction())
        .await
        .expect("verifier deletes");
    assert_eq!(
        queue
            .enqueue(&mut conn, &direct(&custom, &owner))
            .await
            .expect("enqueue answers"),
        EnqueueOutcome::Refused(EnqueueRefusal::NotReady(
            VerifierReadiness::VerifierUnavailable
        ))
    );
    assert_eq!(run_count(&mut conn).await, 0);
}

/// A keyed manual request replays its run for the same digest, refuses a
/// different digest under the same key, and scopes keys to the requester; a
/// refused keyed request leaves the key unused. `target_subject` resolves a
/// binding's subject, echoes a direct subject, and answers `None` for an
/// unknown binding.
///
/// # Panics
/// Panics when any keyed outcome, stored key, run count, or subject differs.
#[tokio::test]
async fn manual_idempotency_keys_replay_conflict_and_scope_to_requester() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let other = self::actor(fixture.data_tenant_id());
    let queue = VerifierRunQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, _) = register_service(&mut conn, &actor, "svc").await;
    let custom = register_verifier(&mut conn, &actor, "custom", custom_drift()).await;
    let psi = register_verifier(&mut conn, &actor, "psi", psi_drift()).await;
    let binding = bind(&mut conn, &owner, &custom, daily(), Vec::new()).await;
    let psi_binding = bind(&mut conn, &owner, &psi, daily(), Vec::new()).await;
    let target = VerificationRunTarget::Binding {
        binding_id: binding,
    };
    let key = |digest: &'static [u8]| RequestKey {
        key: "retry-1",
        request_sha256: digest,
    };

    let first = queue
        .enqueue_manual(&mut conn, actor.id, &target, window(), Some(key(b"a")))
        .await
        .expect("keyed run enqueues");
    let ManualEnqueueOutcome::Enqueued(run_id) = first else {
        panic!("expected a new run, got {first:?}");
    };
    assert_eq!(
        queue
            .enqueue_manual(&mut conn, actor.id, &target, window(), Some(key(b"a")))
            .await
            .expect("replay answers"),
        ManualEnqueueOutcome::Replayed(run_id)
    );
    assert_eq!(
        queue
            .enqueue_manual(&mut conn, actor.id, &target, window(), Some(key(b"b")))
            .await
            .expect("conflict answers"),
        ManualEnqueueOutcome::KeyReused(run_id)
    );
    let stored: (Option<String>, Option<Vec<u8>>) = sqlx::query_as(
        "SELECT idempotency_key, request_sha256 FROM wyrd.verifier_runs WHERE run_id = $1",
    )
    .bind(run_id.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("stored key reads");
    assert_eq!(stored, (Some("retry-1".to_owned()), Some(b"a".to_vec())));

    let other_run = queue
        .enqueue_manual(&mut conn, other.id, &target, window(), Some(key(b"a")))
        .await
        .expect("other requester enqueues");
    assert!(matches!(other_run, ManualEnqueueOutcome::Enqueued(id) if id != run_id));
    let unready = VerificationRunTarget::Binding {
        binding_id: psi_binding,
    };
    let refused_key = RequestKey {
        key: "refused",
        request_sha256: b"c",
    };
    for _ in 0..2 {
        assert_eq!(
            queue
                .enqueue_manual(&mut conn, actor.id, &unready, window(), Some(refused_key))
                .await
                .expect("refusal answers"),
            ManualEnqueueOutcome::Refused(EnqueueRefusal::NotReady(
                VerifierReadiness::BaselineNotReady
            ))
        );
    }
    assert_eq!(run_count(&mut conn).await, 2);

    assert_eq!(
        queue
            .target_subject(&mut conn, &target)
            .await
            .expect("binding subject reads"),
        Some(owner.clone())
    );
    let direct = VerificationRunTarget::Verifier {
        verifier_uid: custom.clone(),
        subject_card_uid: owner.clone(),
    };
    assert_eq!(
        queue
            .target_subject(&mut conn, &direct)
            .await
            .expect("direct subject answers"),
        Some(owner)
    );
    let unknown = VerificationRunTarget::Binding {
        binding_id: BindingId::new_v7(),
    };
    assert_eq!(
        queue
            .target_subject(&mut conn, &unknown)
            .await
            .expect("unknown binding answers"),
        None
    );
}

/// An observation creates one run per (binding, input record); a repeated
/// delivery of the same record returns the existing run, and the claim carries
/// the record and its exact event time.
///
/// # Panics
/// Panics when a duplicate creates a second run or the claimed input differs.
#[tokio::test]
async fn observation_runs_are_unique_per_input_record() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let queue = VerifierRunQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, _) = register_service(&mut conn, &actor, "svc").await;
    let verifier = register_verifier(&mut conn, &actor, "eval", eval()).await;
    let binding = bind(
        &mut conn,
        &owner,
        &verifier,
        BindingActivation::ObservationsReady,
        Vec::new(),
    )
    .await;
    let event_time = at(22, 11, 59);
    let observation = |record: &str| RunRequest::Observation {
        binding_id: binding,
        record_id: record.to_owned(),
        event_time,
    };

    let run = enqueued(
        queue
            .enqueue(&mut conn, &observation("record-1"))
            .await
            .expect("observation enqueues"),
    );
    assert_eq!(
        queue
            .enqueue(&mut conn, &observation("record-1"))
            .await
            .expect("duplicate answers"),
        EnqueueOutcome::AlreadyEnqueued(run)
    );
    enqueued(
        queue
            .enqueue(&mut conn, &observation("record-2"))
            .await
            .expect("second record enqueues"),
    );
    assert_eq!(run_count(&mut conn).await, 2);

    let claimed = claim(&queue, &mut conn).await;
    assert_eq!(claimed.lease.run_id, run);
    assert_eq!(claimed.origin, RunOrigin::Observation);
    assert_eq!(claimed.requested_by, None);
    assert_eq!(
        claimed.input,
        RunInput::EvalRecord {
            record_id: "record-1".to_owned(),
            event_time,
        }
    );
}

/// Wait until some backend of this fixture's own database is blocked behind
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

/// Observation runs of one binding get their `every_nth` ordinal once, at
/// enqueue: a later enqueue waits for an uncommitted earlier one, so the
/// earlier record is always ordinal 1 and the later ordinal 2. That stored
/// ordinal is what every attempt reads, before and after a restart, so the
/// sampling decision cannot flip. Duplicate enqueues consume no ordinal.
///
/// # Panics
/// Panics when the later enqueue does not wait, an ordinal is not the
/// serialized position, a reclaim reads a different ordinal, or a duplicate
/// consumes one.
#[tokio::test]
async fn observation_ordinal_is_fixed_at_serialized_enqueue() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let queue = VerifierRunQueue::default();
    let mut setup = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, _) = register_service(&mut setup, &actor, "svc").await;
    let verifier = register_verifier(&mut setup, &actor, "eval", eval()).await;
    let binding = bind(
        &mut setup,
        &owner,
        &verifier,
        BindingActivation::ObservationsReady,
        Vec::new(),
    )
    .await;
    setup.commit().await.expect("setup commits");
    let event_time = at(22, 11, 59);
    let observation = |record: &str| RunRequest::Observation {
        binding_id: binding,
        record_id: record.to_owned(),
        event_time,
    };

    let mut earlier = fixture.tenant_conn().await.expect("earlier opens");
    let first = enqueued(
        queue
            .enqueue(&mut earlier, &observation("record-1"))
            .await
            .expect("earlier enqueues"),
    );
    let mut later = fixture.tenant_conn().await.expect("later opens");
    let mut probe = fixture.tenant_conn().await.expect("probe opens");
    let later_enqueue = async {
        let run = enqueued(
            queue
                .enqueue(&mut later, &observation("record-2"))
                .await
                .expect("later enqueues"),
        );
        later.commit().await.expect("later commits");
        run
    };
    tokio::pin!(later_enqueue);
    let raced = tokio::select! {
        run = &mut later_enqueue => Some(run),
        () = await_blocked_backend(&mut probe) => None,
    };
    assert_eq!(
        raced, None,
        "a later enqueue for the binding waits for the uncommitted earlier one"
    );
    earlier.commit().await.expect("earlier commits");
    let second = later_enqueue.await;
    drop(probe);

    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    for (record, run) in [("record-1", first), ("record-2", second)] {
        assert_eq!(
            queue
                .enqueue(&mut conn, &observation(record))
                .await
                .expect("duplicate answers"),
            EnqueueOutcome::AlreadyEnqueued(run)
        );
    }
    let third = enqueued(
        queue
            .enqueue(&mut conn, &observation("record-3"))
            .await
            .expect("third record enqueues"),
    );
    let mut attempts = Vec::new();
    for _ in 0..3 {
        attempts.push(claim(&queue, &mut conn).await);
    }
    let ordinals: Vec<_> = attempts
        .iter()
        .map(|run| (run.lease.run_id, run.observation_ordinal))
        .collect();
    assert_eq!(
        ordinals,
        vec![(first, Some(1)), (second, Some(2)), (third, Some(3))],
        "ordinals follow serialized enqueue and duplicates consume none"
    );
    assert_eq!(
        queue
            .release(&mut conn, attempts[1].lease, Duration::zero())
            .await
            .expect("release answers"),
        Settlement::Applied
    );
    conn.commit().await.expect("first attempts commit");

    let (restarted, _) = fixture
        .fresh_runtime_handles()
        .await
        .expect("fresh handles open");
    let mut conn = restarted
        .tenant_conn(fixture.data_tenant_id())
        .await
        .expect("restarted runner opens");
    let retried = claim(&queue, &mut conn).await;
    assert_eq!(retried.lease.run_id, second);
    assert_eq!(
        retried.observation_ordinal,
        Some(2),
        "a restarted attempt reads the ordinal stored at enqueue, so `every_nth` cannot flip"
    );
}

/// Two schedulers ticking the same due binding concurrently create exactly one
/// run: the second skips the locked row. The committed cursor moves to the
/// next future boundary, a restarted scheduler on fresh handles finds nothing
/// due, and re-enqueueing the same occurrence returns the existing run. The
/// cursor sits one claim delay behind the database clock, and the claimed
/// window still ends at the cursor.
///
/// # Panics
/// Panics when an occurrence produces zero or two runs, the window or cursor
/// is wrong, or a restarted scheduler re-runs the occurrence.
#[tokio::test]
async fn concurrent_schedulers_create_one_run_per_occurrence() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let queue = VerifierRunQueue::default();
    let mut setup = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, principal) = register_service(&mut setup, &actor, "svc").await;
    let verifier = register_verifier(&mut setup, &actor, "drift", custom_drift()).await;
    let binding = bind(&mut setup, &owner, &verifier, daily(), Vec::new()).await;
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
    let tick = queue
        .schedule_next_due(&mut first)
        .await
        .expect("first tick runs")
        .expect("the binding is due");
    assert!(
        queue
            .schedule_next_due(&mut second)
            .await
            .expect("second tick runs")
            .is_none(),
        "a locked due binding is skipped"
    );
    assert_eq!(tick.binding_id, binding);
    assert_eq!(tick.due_at, due);
    assert_next_daily_boundary(tick.next_run_at, due);
    let ScheduleOutcome::Enqueued(run) = tick.outcome else {
        panic!("expected a scheduled run, got {:?}", tick.outcome);
    };
    first.commit().await.expect("first scheduler commits");
    assert!(
        queue
            .schedule_next_due(&mut second)
            .await
            .expect("second tick reruns")
            .is_none(),
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
    assert!(
        queue
            .schedule_next_due(&mut conn)
            .await
            .expect("restarted tick runs")
            .is_none()
    );
    assert_next_daily_boundary(cursor(&mut conn, binding).await, due);
    let claimed = claim(&queue, &mut conn).await;
    assert_eq!(claimed.lease.run_id, run);
    assert_eq!(claimed.origin, RunOrigin::Schedule);
    assert_eq!(claimed.requested_by, None);
    let RunInput::DriftWindow(occurrence) = claimed.input else {
        panic!(
            "a scheduled Drift run analyzes a window, got {:?}",
            claimed.input
        );
    };
    assert_eq!(
        occurrence.end, due,
        "the occurrence closes at its due instant"
    );
    assert_eq!(
        queue
            .enqueue(
                &mut conn,
                &RunRequest::Scheduled {
                    binding_id: binding,
                    window: occurrence,
                },
            )
            .await
            .expect("duplicate occurrence answers"),
        EnqueueOutcome::AlreadyEnqueued(run)
    );
    assert_eq!(run_count(&mut conn).await, 1);
}

/// Inactive owners, unready PSI Verifiers, and missed occurrences create no
/// run, and every cursor still advances to the first boundary after the
/// schedule clock (database time minus the claim delay) without backfill.
///
/// # Panics
/// Panics when a skipped occurrence creates a run, reports the wrong reason,
/// or leaves its cursor anywhere but the next future boundary.
#[tokio::test]
async fn scheduler_skips_inactive_unready_and_missed_occurrences() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let queue = VerifierRunQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let custom = register_verifier(&mut conn, &actor, "custom", custom_drift()).await;
    let psi = register_verifier(&mut conn, &actor, "psi", psi_drift()).await;
    let now = database_now(&mut conn).await;
    let due = now - SCHEDULE_CLAIM_DELAY;

    let (stale_owner, stale) = register_service(&mut conn, &actor, "stale").await;
    let inactive = bind(&mut conn, &stale_owner, &custom, daily(), Vec::new()).await;
    record_machine_authentication(&mut conn, stale)
        .await
        .expect("stale activation records");
    age_activity(&mut conn, stale, Duration::days(2)).await;
    set_cursor(&mut conn, inactive, Some(due)).await;

    let (live_owner, live) = register_service(&mut conn, &actor, "live").await;
    let unready = bind(&mut conn, &live_owner, &psi, daily(), Vec::new()).await;
    let missed = bind(&mut conn, &live_owner, &custom, daily(), Vec::new()).await;
    record_machine_authentication(&mut conn, live)
        .await
        .expect("live activation records");
    set_cursor(&mut conn, unready, Some(due)).await;
    set_cursor(&mut conn, missed, Some(due - Duration::days(2))).await;

    let mut outcomes = Vec::new();
    while let Some(tick) = queue.schedule_next_due(&mut conn).await.expect("tick runs") {
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
    assert_eq!(run_count(&mut conn).await, 0);
}

/// A two-batch staged result of `verifier`: a detail batch, then the summary.
///
/// # Panics
/// Never in practice: the static Card name and version are valid.
fn staged_result(verifier: &CardUid) -> StagedResult {
    StagedResult {
        result_id: VerificationResultId::new_v7(),
        event_time: Utc.with_ymd_and_hms(2026, 10, 3, 12, 0, 0).unwrap(),
        verdict: VerificationVerdict::Failed,
        summary: "1 of 3 features drifted".to_owned(),
        counts: DRIFT_COUNTS,
        verifier: wyrd_spec::reference::CardRef {
            kind: CardKind::Verifier,
            name: wyrd_spec::ids::CardName::new("drift").expect("static Card name"),
            version: "1.0.0".parse().expect("static version"),
            space: Some(wyrd_spec::ids::SpaceName::new("default").expect("static space")),
            uid: Some(verifier.clone()),
        },
        batches: vec![
            StagedBatch {
                table: "vala.drift.result_features".to_owned(),
                batch_id: Uuid::now_v7(),
                ipc: vec![1, 2, 3],
            },
            StagedBatch {
                table: "vala.verification.results".to_owned(),
                batch_id: Uuid::now_v7(),
                ipc: vec![4, 5],
            },
        ],
    }
}

/// Stored results visible to the tenant.
///
/// # Panics
/// Panics when the count cannot be read.
async fn staged_count(conn: &mut TenantConn<'_>) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM wyrd.verifier_run_results")
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("staged results count")
}

/// Only the current lease holder stores a run's result; a stale token stores
/// nothing. Every later claim of the run returns the stored result byte for
/// byte, an expired final attempt with a stored result is reclaimed rather
/// than exhausted, and the settlement that completes the run deletes it.
///
/// # Panics
/// Panics when a stale token stores, a reclaim misses or alters the stored
/// result, the run is exhausted, or completion leaves the result stored.
#[tokio::test]
async fn stored_results_are_lease_fenced_and_deleted_at_settle() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let queue = VerifierRunQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, _) = register_service(&mut conn, &actor, "svc").await;
    let verifier = register_verifier(&mut conn, &actor, "drift", custom_drift()).await;
    let binding = bind(&mut conn, &owner, &verifier, daily(), Vec::new()).await;
    let run = enqueued(
        queue
            .enqueue(&mut conn, &manual_binding(&actor, binding))
            .await
            .expect("run enqueues"),
    );
    let staged = staged_result(&verifier);

    let first = claim(&queue, &mut conn).await;
    assert_eq!(first.staged, None, "a fresh run has no stored result");
    assert!(
        first.verifier_present,
        "the claim reports the live Verifier"
    );
    expire_deadlines(&mut conn, run).await;
    let holder = claim(&queue, &mut conn).await;
    assert_eq!(
        queue
            .store_result(&mut conn, first.lease, &staged_result(&verifier))
            .await
            .expect("stale store answers"),
        Settlement::StaleLease,
        "a reclaimed token stores nothing"
    );
    assert_eq!(staged_count(&mut conn).await, 0);
    for _ in 0..2 {
        assert_eq!(
            queue
                .store_result(&mut conn, holder.lease, &staged)
                .await
                .expect("store answers"),
            Settlement::Applied,
            "the holder stores, and a repeated store keeps the same result"
        );
    }
    assert_eq!(staged_count(&mut conn).await, 1);

    sqlx::query("UPDATE wyrd.verifier_runs SET attempts = max_attempts WHERE run_id = $1")
        .bind(run.as_uuid())
        .execute(&mut **conn.transaction())
        .await
        .expect("attempts spent");
    expire_deadlines(&mut conn, run).await;
    let replay = claim(&queue, &mut conn).await;
    assert_eq!(replay.lease.run_id, run, "a decided run is not exhausted");
    assert_eq!(replay.staged.as_ref(), Some(&staged));

    assert_eq!(
        queue
            .complete(
                &mut conn,
                replay.lease,
                staged.result_id,
                staged.verdict,
                &staged.summary,
                staged.counts,
            )
            .await
            .expect("completion answers"),
        Settlement::Applied
    );
    assert_eq!(staged_count(&mut conn).await, 0, "completion deletes it");
}

/// The stored lease expiry of `run`.
///
/// # Panics
/// Panics when the run cannot be read or holds no lease.
async fn lease_expiry(conn: &mut TenantConn<'_>, run: VerificationRunId) -> DateTime<Utc> {
    sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
        "SELECT lease_expires_at FROM wyrd.verifier_runs WHERE run_id = $1",
    )
    .bind(run.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("lease reads")
    .expect("the run holds a lease")
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
    let queue = VerifierRunQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, _) = register_service(&mut conn, &actor, "svc").await;
    let verifier = register_verifier(&mut conn, &actor, "drift", custom_drift()).await;
    let binding = bind(&mut conn, &owner, &verifier, daily(), Vec::new()).await;
    let run = enqueued(
        queue
            .enqueue(&mut conn, &manual_binding(&actor, binding))
            .await
            .expect("run enqueues"),
    );
    let lease = Duration::minutes(5);
    let stale = claim(&queue, &mut conn).await;
    expire_deadlines(&mut conn, run).await;
    let held = claim(&queue, &mut conn).await;
    let young = lease_expiry(&mut conn, run).await;
    let renewed = queue
        .renew(&mut conn, &[stale.lease.token, held.lease.token], lease)
        .await
        .expect("renewal runs");
    assert_eq!(renewed, vec![held.lease.token], "a reclaimed token is gone");
    assert_eq!(
        lease_expiry(&mut conn, run).await,
        young,
        "a young lease stays"
    );

    sqlx::query(
        "UPDATE wyrd.verifier_runs \
         SET lease_expires_at = statement_timestamp() + INTERVAL '1 minute' WHERE run_id = $1",
    )
    .bind(run.as_uuid())
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
        lease_expiry(&mut conn, run).await >= now + lease,
        "a lease past a third of its length is extended from the database clock"
    );

    expire_deadlines(&mut conn, run).await;
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
    let queue = VerifierRunQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, _) = register_service(&mut conn, &actor, "svc").await;
    let verifier = register_verifier(&mut conn, &actor, "drift", custom_drift()).await;
    let binding = bind(&mut conn, &owner, &verifier, daily(), Vec::new()).await;
    let run = enqueued(
        queue
            .enqueue(&mut conn, &manual_binding(&actor, binding))
            .await
            .expect("run enqueues"),
    );

    let claimed_at = database_now(&mut conn).await;
    let stale = claim(&queue, &mut conn).await;
    assert!(
        stale.lease_expires_at >= claimed_at + Duration::minutes(5),
        "the database dates the lease from its own clock"
    );
    assert!(
        queue
            .claim(&mut conn, Duration::minutes(5))
            .await
            .expect("claim runs")
            .is_none(),
        "a live lease is not reclaimable"
    );
    expire_deadlines(&mut conn, run).await;
    let current = claim(&queue, &mut conn).await;
    assert_eq!(current.lease.run_id, run);
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
                DRIFT_COUNTS
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
                    DRIFT_COUNTS
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
                DRIFT_COUNTS,
            )
            .await
            .expect("conflicting completion answers"),
        Settlement::StaleLease,
        "a settled run cannot point at a different result"
    );
    let status = queue
        .run_status(&mut conn, run)
        .await
        .expect("status reads")
        .expect("run exists");
    assert_eq!(status.status, VerificationExecutionStatus::Completed);
    assert_eq!(status.result_id, Some(result));
    assert_eq!(status.error, None);
}

/// A retryable failure backs off 30 then 60 seconds on the same run and input;
/// the third failure exhausts the budget and settles `errored` with the
/// structured error, no result, and no dispatch.
///
/// # Panics
/// Panics when a retry is claimable early, the backoff differs, or exhaustion
/// does not settle the run as a verdict-free error.
#[tokio::test]
async fn retries_back_off_then_exhaust_to_errored() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let queue = VerifierRunQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, _) = register_service(&mut conn, &actor, "svc").await;
    let verifier = register_verifier(&mut conn, &actor, "drift", custom_drift()).await;
    let binding = bind(
        &mut conn,
        &owner,
        &verifier,
        daily(),
        vec![FrozenTarget::Uid(uid())],
    )
    .await;
    let run = enqueued(
        queue
            .enqueue(&mut conn, &manual_binding(&actor, binding))
            .await
            .expect("run enqueues"),
    );

    for (attempt, delay) in [(1, 30), (2, 60)] {
        let claimed = claim(&queue, &mut conn).await;
        assert_eq!((claimed.lease.run_id, claimed.attempt), (run, attempt));
        assert_eq!(claimed.input, RunInput::DriftWindow(window()));
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
            stored_backoff(&mut conn, run).await,
            Duration::seconds(delay),
            "attempt {attempt} backs off {delay} seconds"
        );
        let status = queue
            .run_status(&mut conn, run)
            .await
            .expect("status reads")
            .expect("run exists");
        assert_eq!(status.status, VerificationExecutionStatus::Retrying);
        assert_eq!(status.error, Some(engine_error()));
        assert!(
            queue
                .claim(&mut conn, Duration::minutes(5))
                .await
                .expect("early claim runs")
                .is_none(),
            "a retry is not claimable before its backoff"
        );
        expire_deadlines(&mut conn, run).await;
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
        .run_status(&mut conn, run)
        .await
        .expect("status reads")
        .expect("run exists");
    assert_eq!(status.status, VerificationExecutionStatus::Errored);
    assert_eq!(status.error, Some(engine_error()));
    assert_eq!(status.result_id, None);
    assert!(status.dispatches.is_empty());
}

/// Cancelled and timed-out runs settle with their error and no result, and a
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
    let queue = VerifierRunQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, _) = register_service(&mut conn, &actor, "svc").await;
    let verifier = register_verifier(&mut conn, &actor, "drift", custom_drift()).await;
    let binding = bind(
        &mut conn,
        &owner,
        &verifier,
        daily(),
        vec![FrozenTarget::Uid(uid())],
    )
    .await;
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
        let run = enqueued(
            queue
                .enqueue(&mut conn, &manual_binding(&actor, binding))
                .await
                .expect("run enqueues"),
        );
        let claimed = claim(&queue, &mut conn).await;
        assert_eq!(
            queue
                .terminate(&mut conn, claimed.lease, terminal, &error)
                .await
                .expect("terminate answers"),
            Settlement::Applied
        );
        let status = queue
            .run_status(&mut conn, run)
            .await
            .expect("status reads")
            .expect("run exists");
        assert_eq!(status.status, expected);
        assert_eq!(status.result_id, None);
        assert_eq!(status.error, Some(error.clone()));
        assert!(status.dispatches.is_empty());
    }

    let run = enqueued(
        queue
            .enqueue(&mut conn, &manual_binding(&actor, binding))
            .await
            .expect("run enqueues"),
    );
    for _ in 0..3 {
        claim(&queue, &mut conn).await;
        expire_deadlines(&mut conn, run).await;
    }
    assert!(
        queue
            .claim(&mut conn, Duration::minutes(5))
            .await
            .expect("claim runs")
            .is_none(),
        "an exhausted expired lease is not reclaimed"
    );
    let status = queue
        .run_status(&mut conn, run)
        .await
        .expect("status reads")
        .expect("run exists");
    assert_eq!(status.status, VerificationExecutionStatus::Errored);
    assert_eq!(
        status.error.map(|error| error.code),
        Some("lease_expired".to_owned())
    );
}

/// A failed binding run creates one pending dispatch per distinct frozen
/// Operator, exactly once across settlement retries; passed binding runs and
/// failed direct runs create none. The Run GET projection lists dispatch
/// delivery state and never exposes a verdict.
///
/// # Panics
/// Panics when dispatch fan-out differs or the status projection carries a
/// verdict.
#[tokio::test]
async fn failed_binding_runs_dispatch_each_distinct_operator_once() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let queue = VerifierRunQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, _) = register_service(&mut conn, &actor, "svc").await;
    let verifier = register_verifier(&mut conn, &actor, "drift", custom_drift()).await;
    let pager = FrozenTarget::Uid(uid());
    let inline = FrozenTarget::Digest("sha256:operator".to_owned());
    let binding = bind(
        &mut conn,
        &owner,
        &verifier,
        daily(),
        vec![pager.clone(), inline.clone(), pager.clone()],
    )
    .await;
    let direct_request = RunRequest::Manual {
        requested_by: actor.id,
        target: VerificationRunTarget::Verifier {
            verifier_uid: verifier.clone(),
            subject_card_uid: owner.clone(),
        },
        window: window(),
    };

    let mut settled = Vec::new();
    for (request, verdict) in [
        (manual_binding(&actor, binding), VerificationVerdict::Failed),
        (manual_binding(&actor, binding), VerificationVerdict::Passed),
        (
            manual_binding(&actor, binding),
            VerificationVerdict::Inconclusive,
        ),
        (direct_request, VerificationVerdict::Failed),
    ] {
        let run = enqueued(
            queue
                .enqueue(&mut conn, &request)
                .await
                .expect("run enqueues"),
        );
        let claimed = claim(&queue, &mut conn).await;
        assert_eq!(claimed.lease.run_id, run);
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
                        DRIFT_COUNTS
                    )
                    .await
                    .expect("completion answers"),
                Settlement::Applied
            );
        }
        settled.push(run);
    }

    let failed = queue
        .run_status(&mut conn, settled[0])
        .await
        .expect("status reads")
        .expect("run exists");
    let mut operators: Vec<_> = failed
        .dispatches
        .iter()
        .map(|dispatch| dispatch.operator.clone())
        .collect();
    operators.sort_by_key(|operator| format!("{operator:?}"));
    let mut expected = vec![pager, inline];
    expected.sort_by_key(|operator| format!("{operator:?}"));
    assert_eq!(operators, expected);
    assert!(failed.dispatches.iter().all(|dispatch| {
        dispatch.status == OperatorDispatchStatus::Pending && dispatch.error.is_none()
    }));
    let wire = serde_json::to_value(&failed).expect("status serializes");
    assert!(
        wire.get("verdict").is_none(),
        "Run GET never carries a verdict"
    );
    for run in &settled[1..] {
        let status = queue
            .run_status(&mut conn, *run)
            .await
            .expect("status reads")
            .expect("run exists");
        assert_eq!(status.status, VerificationExecutionStatus::Completed);
        assert!(status.dispatches.is_empty());
    }
    let dispatches: i64 = sqlx::query_scalar("SELECT count(*) FROM wyrd.operator_dispatches")
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("dispatches count");
    assert_eq!(dispatches, 2);
}

/// Releasing a leased run for shutdown makes it immediately claimable again
/// with the same input and without consuming an attempt.
///
/// # Panics
/// Panics when the released run is not claimable at once or its attempt
/// counter advanced.
#[tokio::test]
async fn release_requeues_without_consuming_an_attempt() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let queue = VerifierRunQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, _) = register_service(&mut conn, &actor, "svc").await;
    let verifier = register_verifier(&mut conn, &actor, "drift", custom_drift()).await;
    let binding = bind(&mut conn, &owner, &verifier, daily(), Vec::new()).await;
    let run = enqueued(
        queue
            .enqueue(&mut conn, &manual_binding(&actor, binding))
            .await
            .expect("run enqueues"),
    );

    let first = claim(&queue, &mut conn).await;
    assert_eq!(
        queue
            .release(&mut conn, first.lease, Duration::zero())
            .await
            .expect("release answers"),
        Settlement::Applied
    );
    let status = queue
        .run_status(&mut conn, run)
        .await
        .expect("status reads")
        .expect("run exists");
    assert_eq!(status.status, VerificationExecutionStatus::Pending);
    let again = claim(&queue, &mut conn).await;
    assert_eq!(again.lease.run_id, run);
    assert_eq!(again.attempt, 1);
    assert_eq!(again.input, first.input);
}

/// A claim reports how long its run waited past its due time and how long
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
    let queue = VerifierRunQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, _) = register_service(&mut conn, &actor, "svc").await;
    let verifier = register_verifier(&mut conn, &actor, "drift", custom_drift()).await;
    let binding = bind(&mut conn, &owner, &verifier, daily(), Vec::new()).await;
    let run = enqueued(
        queue
            .enqueue(&mut conn, &manual_binding(&actor, binding))
            .await
            .expect("run enqueues"),
    );
    sqlx::query(
        "UPDATE wyrd.verifier_runs
            SET created_at = statement_timestamp() - INTERVAL '30 seconds',
                next_attempt_at = statement_timestamp() - INTERVAL '10 seconds'
          WHERE run_id = $1",
    )
    .bind(run.as_uuid())
    .execute(&mut **conn.transaction())
    .await
    .expect("run is backdated");
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
        "UPDATE wyrd.verifier_runs
            SET lease_expires_at = statement_timestamp() - INTERVAL '20 seconds'
          WHERE run_id = $1",
    )
    .bind(run.as_uuid())
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

/// One committed record of `subject` written by a principal bound to
/// `writer`.
fn record(subject: &CardUid, writer: &CardUid, record_id: &str) -> ObservationRecord {
    ObservationRecord {
        subject: subject.clone(),
        writer: writer.clone(),
        record_id: record_id.to_owned(),
        event_time: at(22, 11, 59),
    }
}

/// A binding's observation runs as `(record_id, ordinal)` in ordinal order.
///
/// # Panics
/// Panics when the runs cannot be read.
async fn ordinals(conn: &mut TenantConn<'_>, binding: BindingId) -> Vec<(String, i64)> {
    sqlx::query_as(
        "SELECT input_record_id, observation_ordinal FROM wyrd.verifier_runs \
          WHERE binding_id = $1 AND origin = 'observation' ORDER BY observation_ordinal",
    )
    .bind(binding.as_uuid())
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("ordinals read")
}

/// A batch of records for several subjects inserts one run per
/// (`observations_ready` binding the writer owns, record): two bindings of one
/// subject each take every record of it, a scheduled binding takes none, a
/// record whose writer does not own its subject's binding takes none, and a
/// record repeated in the batch runs once.
/// Resubmitting the batch inserts nothing and consumes no ordinal, so a later
/// record numbers straight after the first batch.
///
/// # Panics
/// Panics when a run count or ordinal differs.
#[tokio::test]
async fn observation_batches_insert_once_per_binding_and_record() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let queue = VerifierRunQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let assertion = register_verifier(&mut conn, &actor, "assertion", eval()).await;
    let judge = register_verifier(&mut conn, &actor, "judge", eval()).await;
    let drift = register_verifier(&mut conn, &actor, "drift", custom_drift()).await;
    let (live, live_principal) = register_service(&mut conn, &actor, "live").await;
    let first = bind(
        &mut conn,
        &live,
        &assertion,
        BindingActivation::ObservationsReady,
        Vec::new(),
    )
    .await;
    let second = bind(
        &mut conn,
        &live,
        &judge,
        BindingActivation::ObservationsReady,
        Vec::new(),
    )
    .await;
    bind(&mut conn, &live, &drift, daily(), Vec::new()).await;
    let (other, other_principal) = register_service(&mut conn, &actor, "other").await;
    let other_binding = bind(
        &mut conn,
        &other,
        &assertion,
        BindingActivation::ObservationsReady,
        Vec::new(),
    )
    .await;
    let (foreign, foreign_principal) = register_service(&mut conn, &actor, "foreign").await;
    bind(
        &mut conn,
        &foreign,
        &assertion,
        BindingActivation::ObservationsReady,
        Vec::new(),
    )
    .await;
    for principal in [live_principal, other_principal, foreign_principal] {
        record_machine_authentication(&mut conn, principal)
            .await
            .expect("activation records");
    }
    let batch = [
        record(&live, &live, "r-1"),
        record(&other, &other, "o-1"),
        record(&foreign, &live, "s-1"),
        record(&live, &live, "r-2"),
        record(&live, &live, "r-1"),
    ];

    let inserted = queue
        .enqueue_observation_batch(&mut conn, &batch)
        .await
        .expect("batch enqueues");
    assert_eq!(inserted, 5, "two live bindings x two records, one other");
    assert_eq!(
        queue
            .enqueue_observation_batch(&mut conn, &batch)
            .await
            .expect("repeat answers"),
        0,
        "a repeated batch inserts nothing"
    );
    assert_eq!(run_count(&mut conn).await, 5);
    assert_eq!(
        queue
            .enqueue_observation_batch(&mut conn, &[record(&live, &live, "r-3")])
            .await
            .expect("later record enqueues"),
        2
    );
    let numbered = |records: &[(&str, i64)]| -> Vec<(String, i64)> {
        records
            .iter()
            .map(|(record, ordinal)| ((*record).to_owned(), *ordinal))
            .collect()
    };
    for binding in [first, second] {
        assert_eq!(
            ordinals(&mut conn, binding).await,
            numbered(&[("r-1", 1), ("r-2", 2), ("r-3", 3)]),
            "no repeat consumed an ordinal"
        );
    }
    assert_eq!(
        ordinals(&mut conn, other_binding).await,
        numbered(&[("o-1", 1)])
    );
}

/// Two frames that name the same two subjects in opposite orders serialize
/// instead of deadlocking.
///
/// One outbox batch mixes records for every subject its producers emitted,
/// in arrival order, and enqueues them in one transaction. Locking each
/// binding as its record arrived let two such batches hold one binding each
/// and wait on the other until Postgres aborted one. The batch enqueue locks
/// every binding it will touch up front, in binding order, so the second
/// batch waits at its first statement and then completes.
///
/// # Panics
/// Panics when the second frame does not wait for the first, either frame
/// fails, or the frames do not make one run per row.
#[tokio::test]
async fn frames_naming_subjects_in_opposite_orders_serialize() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let queue = VerifierRunQueue::default();
    let mut setup = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let verifier = register_verifier(&mut setup, &actor, "eval", eval()).await;
    let mut subjects = Vec::new();
    for name in ["assertion", "judge"] {
        let (owner, principal) = register_service(&mut setup, &actor, name).await;
        bind(
            &mut setup,
            &owner,
            &verifier,
            BindingActivation::ObservationsReady,
            Vec::new(),
        )
        .await;
        record_machine_authentication(&mut setup, principal)
            .await
            .expect("activation records");
        subjects.push(owner);
    }
    setup.commit().await.expect("setup commits");
    let [x, y] = [&subjects[0], &subjects[1]];

    let mut first = fixture.tenant_conn().await.expect("first opens");
    let made = queue
        .enqueue_observation_batch(&mut first, &[record(x, x, "a-1"), record(y, y, "a-2")])
        .await
        .expect("first frame enqueues");
    assert_eq!(made, 2);
    let mut second = fixture.tenant_conn().await.expect("second opens");
    let mut probe = fixture.tenant_conn().await.expect("probe opens");
    let second_frame = async {
        let made = queue
            .enqueue_observation_batch(&mut second, &[record(y, y, "b-1"), record(x, x, "b-2")])
            .await
            .expect("second frame enqueues without a deadlock");
        second.commit().await.expect("second commits");
        made
    };
    tokio::pin!(second_frame);
    let raced = tokio::select! {
        made = &mut second_frame => Some(made),
        () = await_blocked_backend(&mut probe) => None,
    };
    assert_eq!(
        raced, None,
        "the second frame waits for every binding the first frame holds"
    );
    first.commit().await.expect("first commits");
    assert_eq!(second_frame.await, 2);
    drop(probe);

    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    assert_eq!(run_count(&mut conn).await, 4);
}

/// An Eval record runs only the bindings its writer owns (REQ-108).
///
/// Services A and B each own an `observations_ready` binding on A's Card. A
/// record A writes runs A's binding and never B's: not while B is active, and
/// not when the record is retried after B authenticates again. A record B
/// writes on the same subject runs B's binding and not A's.
///
/// # Panics
/// Panics when a binding takes a run of a record its owner did not write, or
/// misses a run of one it did.
#[tokio::test]
async fn observation_runs_follow_the_writer() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let queue = VerifierRunQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let verifier = register_verifier(&mut conn, &actor, "eval", eval()).await;
    let (a, _) = register_service(&mut conn, &actor, "writer-a").await;
    let (b, b_principal) = register_service(&mut conn, &actor, "writer-b").await;
    let a_binding = bind(
        &mut conn,
        &a,
        &verifier,
        BindingActivation::ObservationsReady,
        Vec::new(),
    )
    .await;
    let b_binding = project_bindings(
        &mut conn,
        &b,
        &CardKind::Service,
        &[NewBinding {
            subject_occurrence_key: "a".to_owned(),
            subject_card_uid: a.clone(),
            verifier_uid: verifier.clone(),
            trigger: FrozenTarget::Digest("sha256:trigger".to_owned()),
            operators: Vec::new(),
            activation: BindingActivation::ObservationsReady,
        }],
    )
    .await
    .expect("B's binding on A projects")[0];
    record_machine_authentication(&mut conn, b_principal)
        .await
        .expect("B authenticates");

    let first = [record(&a, &a, "a-1")];
    assert_eq!(
        queue
            .enqueue_observation_batch(&mut conn, &first)
            .await
            .expect("A's record enqueues"),
        1
    );
    record_machine_authentication(&mut conn, b_principal)
        .await
        .expect("B authenticates again");
    let retried = [record(&a, &a, "a-1"), record(&a, &a, "a-2")];
    assert_eq!(
        queue
            .enqueue_observation_batch(&mut conn, &retried)
            .await
            .expect("A's retry enqueues"),
        1,
        "only A's new record runs, and only on A's binding"
    );
    assert_eq!(
        queue
            .enqueue_observation_batch(&mut conn, &[record(&a, &b, "b-1")])
            .await
            .expect("B's record enqueues"),
        1
    );

    let numbered = |records: &[(&str, i64)]| -> Vec<(String, i64)> {
        records
            .iter()
            .map(|(record, ordinal)| ((*record).to_owned(), *ordinal))
            .collect()
    };
    assert_eq!(
        ordinals(&mut conn, a_binding).await,
        numbered(&[("a-1", 1), ("a-2", 2)])
    );
    assert_eq!(
        ordinals(&mut conn, b_binding).await,
        numbered(&[("b-1", 1)]),
        "B never takes a run of A's record"
    );
}

/// Waiting for a trace refunds the attempt and requeues after the poll delay;
/// once the fixed deadline measured from the run's creation passes, the run
/// settles `timed_out` with no result, and a stale token changes nothing.
///
/// # Panics
/// Panics when a wait consumes an attempt, the deadline does not time the run
/// out, or a stale token settles.
#[tokio::test]
async fn trace_wait_requeues_until_deadline_then_times_out() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let queue = VerifierRunQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, _) = register_service(&mut conn, &actor, "svc").await;
    let verifier = register_verifier(&mut conn, &actor, "eval", eval()).await;
    let binding = bind(
        &mut conn,
        &owner,
        &verifier,
        BindingActivation::ObservationsReady,
        Vec::new(),
    )
    .await;
    let run = enqueued(
        queue
            .enqueue(
                &mut conn,
                &RunRequest::Observation {
                    binding_id: binding,
                    record_id: "record-1".to_owned(),
                    event_time: at(22, 11, 59),
                },
            )
            .await
            .expect("observation enqueues"),
    );
    let waiting = VerificationError {
        code: "trace_not_landed".to_owned(),
        message: "the required trace has not landed".to_owned(),
    };

    let first = claim(&queue, &mut conn).await;
    let TraceWaitOutcome::Requeued(due) = queue
        .await_trace(
            &mut conn,
            first.lease,
            Duration::seconds(5),
            Duration::hours(1),
            &waiting,
        )
        .await
        .expect("wait answers")
    else {
        panic!("a run inside its deadline must requeue");
    };
    assert!(due > database_now(&mut conn).await);
    expire_deadlines(&mut conn, run).await;
    let second = claim(&queue, &mut conn).await;
    assert_eq!(second.attempt, 1, "waiting must not consume an attempt");
    assert_eq!(second.input, first.input);
    assert_eq!(
        queue
            .await_trace(
                &mut conn,
                first.lease,
                Duration::seconds(5),
                Duration::hours(1),
                &waiting
            )
            .await
            .expect("stale wait answers"),
        TraceWaitOutcome::StaleLease
    );

    sqlx::query("UPDATE wyrd.verifier_runs SET created_at = created_at - INTERVAL '2 hours' WHERE run_id = $1")
        .bind(run.as_uuid())
        .execute(&mut **conn.transaction())
        .await
        .expect("run ages");
    assert_eq!(
        queue
            .await_trace(
                &mut conn,
                second.lease,
                Duration::seconds(5),
                Duration::hours(1),
                &waiting
            )
            .await
            .expect("deadline answers"),
        TraceWaitOutcome::TimedOut
    );
    let status = queue
        .run_status(&mut conn, run)
        .await
        .expect("status reads")
        .expect("run exists");
    assert_eq!(status.status, VerificationExecutionStatus::TimedOut);
    assert_eq!(status.result_id, None);
    assert_eq!(status.error, Some(waiting));
}

/// Runs, dispatches, and status reads are invisible to another tenant, which
/// can neither claim nor settle them; cross-tenant discovery names only the
/// owning tenant, and queue depth counts every tenant's non-terminal work.
///
/// # Panics
/// Panics when tenant B can read, claim, or settle tenant A's run, or
/// discovery and depth differ from the committed queue.
#[tokio::test]
async fn queue_state_is_tenant_isolated() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let other = fixture
        .seed_additional_tenant("verifier-runs-other")
        .await
        .expect("second tenant seeds");
    let tenant = fixture.data_tenant_id();
    let actor = actor(tenant);
    let queue = VerifierRunQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, principal) = register_service(&mut conn, &actor, "svc").await;
    let verifier = register_verifier(&mut conn, &actor, "drift", custom_drift()).await;
    let binding = bind(
        &mut conn,
        &owner,
        &verifier,
        daily(),
        vec![FrozenTarget::Uid(uid())],
    )
    .await;
    record_machine_authentication(&mut conn, principal)
        .await
        .expect("owner activates");
    let failed = enqueued(
        queue
            .enqueue(&mut conn, &manual_binding(&actor, binding))
            .await
            .expect("run enqueues"),
    );
    let claimed = claim(&queue, &mut conn).await;
    queue
        .complete(
            &mut conn,
            claimed.lease,
            VerificationResultId::new_v7(),
            VerificationVerdict::Failed,
            "drift detected",
            DRIFT_COUNTS,
        )
        .await
        .expect("run completes");
    let pending = enqueued(
        queue
            .enqueue(&mut conn, &manual_binding(&actor, binding))
            .await
            .expect("second run enqueues"),
    );
    conn.commit().await.expect("tenant A commits");

    let mut foreign = fixture
        .tenant_conn_for(other)
        .await
        .expect("tenant B opens");
    for run in [failed, pending] {
        assert!(
            queue
                .run_status(&mut foreign, run)
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
            .enqueue(&mut foreign, &manual_binding(&actor, binding))
            .await
            .expect("foreign enqueue answers"),
        EnqueueOutcome::Refused(EnqueueRefusal::BindingNotFound)
    );
    assert!(
        queue
            .claim(&mut foreign, Duration::minutes(5))
            .await
            .expect("foreign claim runs")
            .is_none()
    );
    assert!(
        queue
            .schedule_next_due(&mut foreign)
            .await
            .expect("foreign tick runs")
            .is_none()
    );
    let dispatches: i64 = sqlx::query_scalar("SELECT count(*) FROM wyrd.operator_dispatches")
        .fetch_one(&mut **foreign.transaction())
        .await
        .expect("foreign dispatches count");
    assert_eq!(dispatches, 0);
    drop(foreign);

    let operator = fixture.operator_pool();
    assert_eq!(
        queue
            .tenants_with_runnable_runs(operator)
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
        depth.runs,
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
/// qualifying exchange, and latest run; activity lapses after the inactivity
/// window and PSI readiness stays fail-closed.
///
/// # Panics
/// Panics when any reported field differs from the durable state.
#[tokio::test]
async fn binding_status_reports_activity_readiness_and_latest_run() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let queue = VerifierRunQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, principal) = register_service(&mut conn, &actor, "svc").await;
    let custom = register_verifier(&mut conn, &actor, "custom", custom_drift()).await;
    let psi = register_verifier(&mut conn, &actor, "psi", psi_drift()).await;
    let ready = bind(&mut conn, &owner, &custom, daily(), Vec::new()).await;
    let unready = bind(&mut conn, &owner, &psi, daily(), Vec::new()).await;

    let dormant = queue
        .binding_status(&mut conn, ready)
        .await
        .expect("status reads")
        .expect("binding exists");
    assert!(!dormant.active);
    assert_eq!(dormant.next_run_at, None);
    assert_eq!(dormant.last_activated_at, None);
    assert_eq!(dormant.last_run_id, None);

    let activated_after = database_now(&mut conn).await;
    record_machine_authentication(&mut conn, principal)
        .await
        .expect("owner activates");
    let mut latest = None;
    for _ in 0..2 {
        latest = Some(enqueued(
            queue
                .enqueue(&mut conn, &manual_binding(&actor, ready))
                .await
                .expect("run enqueues"),
        ));
    }
    let status = queue
        .binding_status(&mut conn, ready)
        .await
        .expect("status reads")
        .expect("binding exists");
    assert_eq!(status.binding_id, ready);
    assert_eq!(status.owner_card_uid, owner);
    assert_eq!(status.subject_card_uid, owner);
    assert_eq!(status.verifier_uid, custom);
    assert!(status.active);
    assert_eq!(status.readiness, VerifierReadiness::Ready);
    assert_next_daily_boundary(status.next_run_at, activated_after);
    assert!(
        status
            .last_activated_at
            .is_some_and(|at| at >= activated_after),
        "the exchange is stamped by the database clock"
    );
    assert_eq!(status.last_run_id, latest);

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
    let queue = VerifierRunQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (owner, principal) = register_service(&mut conn, &actor, "svc").await;
    let verifier = register_verifier(&mut conn, &actor, "drift", custom_drift()).await;
    let binding = bind(
        &mut conn,
        &owner,
        &verifier,
        daily(),
        vec![FrozenTarget::Uid(uid())],
    )
    .await;

    let before_enqueue = database_now(&mut conn).await;
    let run = enqueued(
        queue
            .enqueue(&mut conn, &manual_binding(&actor, binding))
            .await
            .expect("run enqueues"),
    );
    let after_enqueue = database_now(&mut conn).await;
    let (created_at, next_attempt_at, updated_at): (DateTime<Utc>, DateTime<Utc>, DateTime<Utc>) =
        sqlx::query_as(
            "SELECT created_at, next_attempt_at, updated_at \
             FROM wyrd.verifier_runs WHERE run_id = $1",
        )
        .bind(run.as_uuid())
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("run timestamps read");
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
        stored_backoff(&mut conn, run).await,
        Duration::seconds(30),
        "the stored backoff is derived entirely inside the database"
    );
    assert!(
        queue
            .claim(&mut conn, Duration::minutes(5))
            .await
            .expect("early claim runs")
            .is_none(),
        "the database refuses a run whose own backoff has not elapsed"
    );

    expire_deadlines(&mut conn, run).await;
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
                DRIFT_COUNTS,
            )
            .await
            .expect("completion answers"),
        Settlement::Applied
    );
    let after_complete = database_now(&mut conn).await;
    let settled_at: DateTime<Utc> =
        sqlx::query_scalar("SELECT settled_at FROM wyrd.verifier_runs WHERE run_id = $1")
            .bind(run.as_uuid())
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("settled instant reads");
    assert!(
        (before_complete..=after_complete).contains(&settled_at),
        "settlement is stamped by the database clock"
    );
    let dispatched_at: DateTime<Utc> =
        sqlx::query_scalar("SELECT created_at FROM wyrd.operator_dispatches WHERE run_id = $1")
            .bind(run.as_uuid())
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
        queue
            .schedule_next_due(&mut conn)
            .await
            .expect("future tick runs")
            .is_none(),
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
        queue
            .schedule_next_due(&mut conn)
            .await
            .expect("window-end tick runs")
            .is_none(),
        "a window that has just ended waits out the claim delay"
    );
    let due = database_now(&mut conn).await - SCHEDULE_CLAIM_DELAY;
    set_cursor(&mut conn, binding, Some(due)).await;
    let tick = queue
        .schedule_next_due(&mut conn)
        .await
        .expect("due tick runs")
        .expect("a cursor the claim delay behind the database clock is due");
    assert_eq!(tick.due_at, due);
}

/// Settle one failed binding run of `verifier` on `owner` with two Operators
/// and return the run.
///
/// # Panics
/// Panics when the run cannot be enqueued, claimed, or completed.
async fn failed_run_with_two_dispatches(
    queue: &VerifierRunQueue,
    conn: &mut TenantConn<'_>,
    actor: &Principal,
) -> VerificationRunId {
    let (owner, _) = register_service(conn, actor, "svc").await;
    let verifier = register_verifier(conn, actor, "drift", custom_drift()).await;
    let binding = bind(
        conn,
        &owner,
        &verifier,
        daily(),
        vec![
            FrozenTarget::Uid(uid()),
            FrozenTarget::Digest("sha256:operator".to_owned()),
        ],
    )
    .await;
    let run = enqueued(
        queue
            .enqueue(conn, &manual_binding(actor, binding))
            .await
            .expect("run enqueues"),
    );
    let claimed = claim(queue, conn).await;
    let summary = "x".repeat(2 * MAX_SUMMARY_CHARS);
    queue
        .complete(
            conn,
            claimed.lease,
            VerificationResultId::new_v7(),
            VerificationVerdict::Failed,
            &summary,
            DRIFT_COUNTS,
        )
        .await
        .expect("failed completion settles");
    run
}

/// A failed settlement freezes the bounded failure context on each dispatch,
/// built from the settled run and its exact Cards.
///
/// # Panics
/// Panics when the stored context is missing a field, carries another
/// verdict, or exceeds the summary bound.
#[tokio::test]
async fn failed_settlement_freezes_the_bounded_failure_context() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let queue = VerifierRunQueue::default();
    let mut conn = fixture.tenant_conn().await.expect("tenant connection");
    let run = failed_run_with_two_dispatches(&queue, &mut conn, &actor).await;

    let contexts: Vec<(Uuid, Value)> = sqlx::query_as(
        "SELECT dispatch_id, failure_context FROM wyrd.operator_dispatches WHERE run_id = $1",
    )
    .bind(run.as_uuid())
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("contexts read");
    assert_eq!(contexts.len(), 2);
    for (dispatch_id, context) in contexts {
        let context: OperatorFailureContext =
            serde_json::from_value(context).expect("context is the closed shape");
        assert_eq!(context.dispatch_id.as_uuid(), dispatch_id);
        assert_eq!(context.run_id, run);
        assert_eq!(context.verdict, VerificationVerdict::Failed);
        assert_eq!(context.verifier_ref, "default/drift@1.0.0");
        assert_eq!(context.subject_ref, "default/svc@1.0.0");
        assert_eq!(context.summary.chars().count(), MAX_SUMMARY_CHARS);
        assert_eq!(context.verifier, DRIFT_COUNTS);
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
    let runs = VerifierRunQueue::default();
    let dispatches = OperatorDispatchQueue::default();
    let mut conn = fixture.tenant_conn().await.expect("tenant connection");
    let run = failed_run_with_two_dispatches(&runs, &mut conn, &actor).await;
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
    assert_eq!(first.run_id, run);
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
    let status = runs
        .run_status(&mut conn, run)
        .await
        .expect("status reads")
        .expect("run exists");
    let mut statuses: Vec<_> = status.dispatches.iter().map(|d| d.status).collect();
    statuses.sort_by_key(|status| <&'static str>::from(*status));
    assert_eq!(
        statuses,
        vec![
            OperatorDispatchStatus::Delivered,
            OperatorDispatchStatus::Failed
        ]
    );
    assert_eq!(status.status, VerificationExecutionStatus::Completed);
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
    let runs = VerifierRunQueue::default();
    let dispatches = OperatorDispatchQueue::default();
    let mut conn = fixture.tenant_conn().await.expect("tenant connection");
    failed_run_with_two_dispatches(&runs, &mut conn, &actor).await;
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
    let runs = VerifierRunQueue::default();
    let expired = OperatorDispatchQueue::new(
        3,
        std::time::Duration::ZERO,
        std::time::Duration::from_secs(45),
    );
    let mut conn = fixture.tenant_conn().await.expect("tenant connection");
    let run = failed_run_with_two_dispatches(&runs, &mut conn, &actor).await;
    assert!(
        expired
            .claim(&mut conn)
            .await
            .expect("claim runs")
            .is_none()
    );
    let status = runs
        .run_status(&mut conn, run)
        .await
        .expect("status reads")
        .expect("run exists");
    assert!(status.dispatches.iter().all(|dispatch| {
        dispatch.status == OperatorDispatchStatus::Failed
            && dispatch.error.as_ref().map(|e| e.code.as_str()) == Some("deadline_exceeded")
    }));
}
