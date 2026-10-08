//! PgFixture coverage for fitted Drift baseline work and status: pending
//! registration, leased claims, token-fenced settlement, bounded retry,
//! generic readiness, and tenant isolation.
//!
//! Every test runs against the repository managed Postgres through tenant
//! connections, so forced RLS applies, and PostgreSQL's own clock decides every
//! due time and lease.

use chrono::Duration;
use serde_json::{Value, json};
use uuid::Uuid;
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_runtime::PermissionSet;
use wyrd_runtime::principal::{Principal, PrincipalId, PrincipalKind};
use wyrd_spec::DataTenantId;
use wyrd_spec::card::verifier::DriftBaselineState;
use wyrd_spec::envelope::{Card, CardKind};
use wyrd_spec::ids::CardUid;
use wyrd_spec::registry::RegistrationOperationId;
use wyrd_spec::verification::VerificationError;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::cards::{
    NewCardRow, NewRegistrationOperation, insert_card_row, insert_registration_operation,
};
use wyrd_sql::queries::drift_baselines::{ClaimedBaseline, DriftBaselineQueue};
use wyrd_sql::queries::verifier_runs::{RetryOutcome, Settlement};
use wyrd_sql::row_types::cards::CardStatus;

/// A registering user principal in `tenant`; persistence queries do not authorize.
fn actor(tenant: DataTenantId) -> Principal {
    Principal::new(
        PrincipalId::new(Uuid::now_v7()),
        PrincipalKind::User,
        tenant,
        Vec::new(),
        PermissionSet::new(),
    )
}

/// Insert one active Card of `kind` named `name` with stored `spec`; return its UID.
///
/// The row is inserted as a Service fixture and rewritten, because only the
/// stored kind, spec, and status matter to baseline readiness.
///
/// # Panics
/// Panics when the operation, Card row, or rewrite fails.
async fn insert_card(
    conn: &mut TenantConn<'_>,
    actor: &Principal,
    kind: &str,
    name: &str,
    spec: Value,
) -> CardUid {
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
    let card: Card = serde_json::from_value(json!({
        "apiVersion": "wyrd/v1",
        "kind": "Service",
        "metadata": { "name": name, "version": "1.0.0", "space": "default" },
        "spec": {},
        "relationships": { "outbound": [], "inbound": [] }
    }))
    .expect("fixture card deserializes");
    let card_uid = insert_card_row(
        conn,
        NewCardRow {
            card: &card,
            card_uid: CardUid::from_uuid(Uuid::now_v7()).expect("UUIDv7 is a valid card UID"),
            principal_id: actor.id,
            operation_id,
            status: CardStatus::Pending,
            spec_hash: "spec-hash",
            artifact_hash: None,
        },
    )
    .await
    .expect("card inserts")
    .card_uid;
    sqlx::query(
        "UPDATE wyrd.cards SET kind = $2, spec = $3, status = 'active' WHERE card_uid = $1",
    )
    .bind(card_uid.as_uuid())
    .bind(kind)
    .bind(spec)
    .execute(&mut **conn.transaction())
    .await
    .expect("card rewrites");
    card_uid
}

/// Register an active PSI Verifier and its baseline Data Card, with the
/// pending baseline row; return (Verifier UID, Data UID).
///
/// # Panics
/// Panics when a Card or the baseline row fails to insert.
async fn register_psi(
    conn: &mut TenantConn<'_>,
    queue: &DriftBaselineQueue,
    actor: &Principal,
) -> (CardUid, CardUid) {
    let data = insert_card(conn, actor, "Data", "baseline", json!({})).await;
    let verifier = insert_card(
        conn,
        actor,
        "Verifier",
        "psi",
        json!({ "implementation": { "kind": "drift", "spec": { "method": "Psi" } } }),
    )
    .await;
    queue
        .insert_pending(conn, &verifier, &data)
        .await
        .expect("pending baseline inserts");
    (verifier, data)
}

/// Read generic Verifier readiness through the shared SQL owner.
///
/// # Panics
/// Panics when the function cannot be evaluated.
async fn readiness(conn: &mut TenantConn<'_>, verifier: &CardUid) -> String {
    sqlx::query_scalar("SELECT wyrd.verifier_readiness($1)")
        .bind(verifier.as_uuid())
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("readiness reads")
}

/// Move `verifier`'s set lease or retry deadline to the database's current instant.
///
/// PostgreSQL still decides whether the row is due; the test only places it in
/// the past instead of advancing a process clock.
///
/// # Panics
/// Panics when the update fails.
async fn expire_deadlines(conn: &mut TenantConn<'_>, verifier: &CardUid) {
    sqlx::query(
        "UPDATE wyrd.drift_baselines
            SET lease_expires_at = CASE WHEN lease_expires_at IS NULL
                                        THEN NULL ELSE statement_timestamp() END,
                next_attempt_at = CASE WHEN next_attempt_at IS NULL
                                       THEN NULL ELSE statement_timestamp() END
          WHERE verifier_uid = $1",
    )
    .bind(verifier.as_uuid())
    .execute(&mut **conn.transaction())
    .await
    .expect("deadlines expire");
}

/// Claim the one due fit, failing when none is due.
///
/// # Panics
/// Panics when the claim fails or finds nothing due.
async fn claim(queue: &DriftBaselineQueue, conn: &mut TenantConn<'_>) -> ClaimedBaseline {
    queue
        .claim(conn, Duration::minutes(5))
        .await
        .expect("claim runs")
        .expect("a fit is due")
}

/// A structured fit failure.
fn fit_error() -> VerificationError {
    VerificationError {
        code: "baseline_unreadable".to_owned(),
        message: "baseline Parquet could not be read".to_owned(),
    }
}

/// Registration stores a pending, unready baseline pinned to exact Cards; a
/// claim builds it and completion makes the Verifier ready with its profile.
///
/// # Panics
/// Panics when any state, identity, or readiness differs from the lifecycle.
#[tokio::test]
async fn pending_baseline_builds_then_becomes_ready() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let queue = DriftBaselineQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (verifier, data) = register_psi(&mut conn, &queue, &actor).await;

    let pending = queue
        .status(&mut conn, &verifier)
        .await
        .expect("status reads")
        .expect("PSI has a baseline");
    assert_eq!(pending.state, DriftBaselineState::Pending);
    assert_eq!(pending.data.kind, CardKind::Data);
    assert_eq!(pending.data.name.as_str(), "baseline");
    assert_eq!(pending.data.uid.as_ref(), Some(&data));
    assert!(pending.error.is_none());
    assert_eq!(readiness(&mut conn, &verifier).await, "baseline_not_ready");

    let claimed = claim(&queue, &mut conn).await;
    assert_eq!(claimed.lease.verifier_uid, verifier);
    assert_eq!(claimed.data_card_uid, data);
    assert_eq!(claimed.attempt, 1);
    let building = queue
        .status(&mut conn, &verifier)
        .await
        .expect("status reads");
    assert_eq!(
        building.map(|status| status.state),
        Some(DriftBaselineState::Building)
    );
    assert!(
        queue
            .fitted(&mut conn, &verifier)
            .await
            .expect("fitted reads")
            .is_none()
    );
    assert_eq!(readiness(&mut conn, &verifier).await, "baseline_not_ready");

    let profile = json!({ "Psi": { "bins": [] } });
    assert_eq!(
        queue
            .complete(&mut conn, &claimed.lease, &profile)
            .await
            .expect("fit completes"),
        Settlement::Applied
    );
    let ready = queue
        .status(&mut conn, &verifier)
        .await
        .expect("status reads");
    assert_eq!(
        ready.map(|status| status.state),
        Some(DriftBaselineState::Ready)
    );
    assert_eq!(
        queue
            .fitted(&mut conn, &verifier)
            .await
            .expect("fitted reads"),
        Some(profile)
    );
    assert_eq!(readiness(&mut conn, &verifier).await, "ready");
    assert!(
        queue
            .claim(&mut conn, Duration::minutes(5))
            .await
            .expect("claim runs")
            .is_none(),
        "a ready baseline is never refitted"
    );
}

/// A failed fit stays visible and retries through the same row with a
/// database-clock backoff until the budget is exhausted, then is final.
///
/// # Panics
/// Panics when a failure is hidden, retried early, or retried past the budget.
#[tokio::test]
async fn failed_fits_retry_through_the_same_row_then_stop() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let queue = DriftBaselineQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (verifier, _) = register_psi(&mut conn, &queue, &actor).await;

    for attempt in 1..=3 {
        let claimed = claim(&queue, &mut conn).await;
        assert_eq!(claimed.attempt, attempt);
        let outcome = queue
            .fail(&mut conn, &claimed.lease, &fit_error())
            .await
            .expect("failure records");
        let status = queue
            .status(&mut conn, &verifier)
            .await
            .expect("status reads")
            .expect("baseline exists");
        assert_eq!(status.state, DriftBaselineState::Failed);
        assert_eq!(status.error, Some(fit_error()));
        assert_eq!(readiness(&mut conn, &verifier).await, "baseline_not_ready");
        if attempt < 3 {
            assert!(matches!(outcome, RetryOutcome::Scheduled(_)), "{outcome:?}");
            assert!(
                queue
                    .claim(&mut conn, Duration::minutes(5))
                    .await
                    .expect("claim runs")
                    .is_none(),
                "a retry is not due before its backoff"
            );
            expire_deadlines(&mut conn, &verifier).await;
        } else {
            assert_eq!(outcome, RetryOutcome::Exhausted);
        }
    }
    expire_deadlines(&mut conn, &verifier).await;
    assert!(
        queue
            .claim(&mut conn, Duration::minutes(5))
            .await
            .expect("claim runs")
            .is_none(),
        "a final failure is never claimed again"
    );
}

/// An interrupted fit is reclaimed after its lease expires, the stale holder
/// is fenced out, an exhausted expired lease fails final, and a drained fit
/// is refunded.
///
/// # Panics
/// Panics when a stale token settles, an expired lease is lost, or a release
/// consumes an attempt.
#[tokio::test]
async fn expired_leases_are_reclaimed_and_stale_tokens_fenced() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let actor = actor(fixture.data_tenant_id());
    let queue = DriftBaselineQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let (verifier, _) = register_psi(&mut conn, &queue, &actor).await;

    let first = claim(&queue, &mut conn).await;
    assert!(
        queue
            .claim(&mut conn, Duration::minutes(5))
            .await
            .expect("claim runs")
            .is_none(),
        "a live lease is not reclaimed"
    );
    queue
        .release(&mut conn, &first.lease)
        .await
        .expect("release applies");
    let refunded = claim(&queue, &mut conn).await;
    assert_eq!(
        refunded.attempt, 1,
        "a drained fit keeps its attempt budget"
    );

    expire_deadlines(&mut conn, &verifier).await;
    let second = claim(&queue, &mut conn).await;
    assert_eq!(second.attempt, 2);
    assert_eq!(
        queue
            .complete(&mut conn, &refunded.lease, &json!({}))
            .await
            .expect("stale completion answers"),
        Settlement::StaleLease
    );
    assert_eq!(
        queue
            .fail(&mut conn, &refunded.lease, &fit_error())
            .await
            .expect("stale failure answers"),
        RetryOutcome::StaleLease
    );

    expire_deadlines(&mut conn, &verifier).await;
    let third = claim(&queue, &mut conn).await;
    assert_eq!(third.attempt, 3);
    expire_deadlines(&mut conn, &verifier).await;
    assert!(
        queue
            .claim(&mut conn, Duration::minutes(5))
            .await
            .expect("claim runs")
            .is_none()
    );
    let status = queue
        .status(&mut conn, &verifier)
        .await
        .expect("status reads")
        .expect("baseline exists");
    assert_eq!(status.state, DriftBaselineState::Failed);
    assert_eq!(
        status.error.map(|error| error.code),
        Some("lease_expired".to_owned())
    );
}

/// Custom Drift is ready without a baseline row, and baseline work and status
/// are invisible across tenants while work discovery finds the owning tenant.
///
/// # Panics
/// Panics when Custom needs a fit or another tenant can observe the baseline.
#[tokio::test]
async fn custom_needs_no_fit_and_baselines_are_tenant_isolated() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let other = fixture
        .seed_additional_tenant("drift-baselines-other")
        .await
        .expect("second tenant seeds");
    let tenant = fixture.data_tenant_id();
    let actor = actor(tenant);
    let queue = DriftBaselineQueue::default();
    let mut conn = fixture
        .tenant_conn()
        .await
        .expect("tenant connection opens");
    let custom = insert_card(
        &mut conn,
        &actor,
        "Verifier",
        "custom",
        json!({ "implementation": { "kind": "drift", "spec": { "method": "Custom" } } }),
    )
    .await;
    assert_eq!(readiness(&mut conn, &custom).await, "ready");
    assert!(
        queue
            .status(&mut conn, &custom)
            .await
            .expect("status reads")
            .is_none()
    );
    let (verifier, data) = register_psi(&mut conn, &queue, &actor).await;
    conn.commit().await.expect("tenant A commits");

    let operator = fixture.operator_pool();
    assert_eq!(
        queue
            .tenants_with_due_fits(operator, 10)
            .await
            .expect("due tenants read"),
        vec![tenant]
    );

    let mut foreign = fixture
        .tenant_conn_for(other)
        .await
        .expect("tenant B opens");
    assert!(
        queue
            .status(&mut foreign, &verifier)
            .await
            .expect("foreign status")
            .is_none()
    );
    assert!(
        queue
            .fitted(&mut foreign, &verifier)
            .await
            .expect("foreign fitted")
            .is_none()
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
            .insert_pending(&mut foreign, &verifier, &data)
            .await
            .is_err(),
        "a foreign tenant cannot attach a baseline to another tenant's Cards"
    );
}
