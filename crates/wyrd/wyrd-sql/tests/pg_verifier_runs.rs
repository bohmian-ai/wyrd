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
use wyrd_sql::queries::verification::{
    BindingActivation, FrozenTarget, NewBinding, project_bindings, record_machine_authentication,
};
use wyrd_sql::queries::verifier_runs::{
    ClaimedRun, EnqueueOutcome, EnqueueRefusal, ManualEnqueueOutcome, QueueCounts, RequestKey,
    RetryOutcome, RunInput, RunOrigin, RunRequest, ScheduleOutcome, ScheduleSkip, Settlement,
    TerminalStatus, VerifierRunQueue,
};
use wyrd_sql::row_types::cards::CardStatus;

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

/// Two schedulers ticking the same due binding concurrently create exactly one
/// run: the second skips the locked row. The committed cursor moves to the
/// next future boundary, a restarted scheduler on fresh handles finds nothing
/// due, and re-enqueueing the same occurrence returns the existing run.
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
    let due = database_now(&mut setup).await;
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
/// database instant without backfill.
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
    let due = database_now(&mut conn).await;

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
            .complete(&mut conn, stale.lease, result, VerificationVerdict::Passed)
            .await
            .expect("stale completion answers"),
        Settlement::StaleLease
    );
    assert_eq!(
        queue
            .release(&mut conn, stale.lease)
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
                    VerificationVerdict::Passed
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
                    .complete(&mut conn, claimed.lease, result, verdict)
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
            .release(&mut conn, first.lease)
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
            .tenants_with_runnable_runs(operator, 10)
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
    set_cursor(&mut due_now, binding, Some(now)).await;
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
/// differs from the configured delay, or a future cursor is reported due.
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
    let due = database_now(&mut conn).await;
    set_cursor(&mut conn, binding, Some(due)).await;
    let tick = queue
        .schedule_next_due(&mut conn)
        .await
        .expect("due tick runs")
        .expect("the cursor is due on the database clock");
    assert_eq!(tick.due_at, due);
}
