//! Tier-2 coverage for the separate expired-object cleanup task.
//!
//! Cleanup consumes an immutable handoff one committed expiration left behind,
//! prepares exactly one candidate at a time with Postgres closed before any
//! object-store call, blocks Oracle reader widening while a preparation is
//! unresolved, and advances only for a confirmed deletion or a proven absence.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use vala_bifrost_redux::forge::{ForgeError, ForgeScheduler, ForgeWorker, ForgeWorkerConfig};
use vala_sql::queries::forge_tasks::{ForgeEnqueueBatch, ForgeTasks};
use vala_sql::row_types::forge_tasks::{
    ExpiredCleanupPayload, ForgeCleanupCandidate, ForgeCleanupCategory, ForgeCleanupPath,
    ForgeTaskClaim, ForgeTaskLane, ForgeTaskStrategy, ForgeTaskTableIdentity, NewForgeTask,
};
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

use super::snapshot_expiration::{
    ExpirableTable, expirable_table, object_exists, seed_ready_expiry_task,
};
use super::support::PromotionIntegrationFixture;

/// One drained expiration and the cleanup task its handoff projects to.
struct DrainedExpiration {
    /// Live fixture, object store, catalog seam, and supervisor.
    table: ExpirableTable,
    /// Worker used for every phase-bypassing execution in this module.
    worker: ForgeWorker,
    /// Durable identity of the enqueued cleanup task.
    cleanup_id: Uuid,
    /// Immutable handoff the cleanup task carries.
    payload: ExpiredCleanupPayload,
}

/// Builds one lifecycle audit event for a cleanup task.
fn task_event(operation: &str, task_id: Uuid) -> AuditEvent {
    AuditEvent::new(
        RequestId::now_v7(),
        None,
        operation.to_owned(),
        format!("forge-task:{task_id}"),
        None,
        PrincipalId::new(Uuid::nil()),
        PrincipalKindTag::Service,
        AuthMethod::Internal,
        "bifrost:forge".to_owned(),
        AuditDecision::Allow,
        AuditResult::Success,
        "unschedulable".to_owned(),
    )
}

/// Lists one task's audit operations in durable sequence order.
///
/// # Panics
///
/// Panics when the diagnostic read fails.
async fn audits(fixture: &PromotionIntegrationFixture, task_id: Uuid) -> Vec<String> {
    let mut conn = fixture
        .vala
        .tenant_conn(fixture.tenant)
        .await
        .expect("fixture tenant connection");
    let operations: Vec<String> = sqlx::query_scalar(
        "SELECT operation FROM vala.audit_outbox WHERE resource = $1 ORDER BY seq",
    )
    .bind(format!("forge-task:{task_id}"))
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("task audits are readable");
    conn.commit().await.expect("audit read commit");
    operations
}

/// Reads one cleanup task's durable state and cursor position.
///
/// # Panics
///
/// Panics when the read or evidence decode fails.
async fn cursor(
    fixture: &PromotionIntegrationFixture,
    task_id: Uuid,
) -> (String, u32, Option<u32>) {
    let (state, evidence): (String, Option<serde_json::Value>) =
        sqlx::query_as("SELECT state, evidence FROM vala.forge_tasks WHERE task_id = $1")
            .bind(task_id)
            .fetch_one(fixture.operator_pool.pool())
            .await
            .expect("cleanup task is readable");
    let decoded = evidence.map(|value| {
        vala_sql::row_types::forge_tasks::evidence_from_json(value).expect("evidence decodes")
    });
    (
        state,
        decoded
            .as_ref()
            .map_or(0, |value| value.deleted_candidate_count),
        decoded.and_then(|value| value.prepared_candidate_index),
    )
}

/// Runs one real expiration and enqueues the cleanup task its handoff projects.
///
/// Only phase activation is bypassed: the projection, the locked-source
/// validation, and the durable enqueue transaction are the production owners.
///
/// # Panics
///
/// Panics when the expiration does not settle with candidates or the cleanup
/// task cannot be planned and enqueued.
async fn drained_expiration(name: &str) -> DrainedExpiration {
    let table = expirable_table(name, true).await;
    let worker = ForgeWorker::new(
        table.supervised.forge(),
        ForgeWorkerConfig::default(),
        Uuid::now_v7(),
    )
    .expect("fixture Forge worker");
    let expiry = seed_ready_expiry_task(&table.fixture, table.watermark, "51").await;
    let claim = worker
        .claim_for_test()
        .await
        .expect("claim transaction runs")
        .expect("the ready expiry task is claimable");
    assert_eq!(claim.task_id, expiry);
    worker
        .execute_snapshot_expiry_claim_for_test(claim, &CancellationToken::new())
        .await
        .expect("the expiration settles");

    let tasks = ForgeTasks::new(table.fixture.operator_pool.clone());
    let identity = vala_sql::row_types::forge_tasks::ForgeTaskTableIdentity::new(
        "wyrd-redux",
        table.fixture.binding.table_ref.namespace.as_str(),
        &table.fixture.binding.table_ref.name,
    )
    .expect("table identity");
    let payload = tasks
        .unconsumed_expiration_handoff(table.fixture.tenant, &identity)
        .await
        .expect("handoff read")
        .expect("the settled expiration left one handoff");
    assert!(!payload.cleanup_candidates.is_empty());

    // The supervised production scheduler holds the singleton planning fence;
    // ageing it out lets this test own one deterministic planning pass.
    sqlx::query("UPDATE vala.forge_scheduler_state SET expires_at = now() - interval '1 hour'")
        .execute(table.fixture.operator_pool.pool())
        .await
        .expect("the live scheduler fence ages out");
    let owner = Uuid::now_v7();
    let fence = tasks
        .acquire_scheduler(owner, 30)
        .await
        .expect("scheduler lease")
        .expect("uncontended fence");
    let (demands, _) = tasks
        .planning_demands(owner, fence, 8)
        .await
        .expect("demands");
    let demand = demands
        .into_iter()
        .find(|value| value.table_ref == identity)
        .expect("settlement created cleanup demand");
    let forge = table.supervised.forge();
    let projected: NewForgeTask = ForgeScheduler::with_owner_for_test(&forge, owner)
        .expect("fixture scheduler")
        .expired_cleanup_task_for_test(&demand)
        .await
        .expect("the fixed bounded projection builds")
        .expect("one unconsumed handoff projects one cleanup task");
    assert_eq!(projected.strategy, ForgeTaskStrategy::ExpiredCleanup);
    assert_eq!(projected.lane, ForgeTaskLane::Ordinary);
    assert_eq!(projected.base_snapshot_id, payload.committed_snapshot_id);
    assert_eq!(
        projected.estimates.files as usize,
        payload.cleanup_candidates.len()
    );
    assert_eq!(projected.estimates.parallelism, 1);
    assert!(projected.plan.inputs.is_empty());
    assert!(projected.estimates.envelope.is_some());

    assert_eq!(
        tasks
            .enqueue_and_acknowledge(
                owner,
                fence,
                &demand,
                ForgeEnqueueBatch {
                    executable: std::slice::from_ref(&projected),
                    unschedulable: &[],
                },
                |id| task_event("forge.task.unschedulable", id),
            )
            .await
            .expect("cleanup enqueue"),
        1
    );
    let cleanup_id: Uuid =
        sqlx::query_scalar("SELECT task_id FROM vala.forge_tasks WHERE strategy='expired_cleanup'")
            .fetch_one(table.fixture.operator_pool.pool())
            .await
            .expect("cleanup task id");
    DrainedExpiration {
        table,
        worker,
        cleanup_id,
        payload,
    }
}

/// Overwrites one cleanup task's persisted plan with an exact handoff.
///
/// Only a durably persisted plan can prove the consumer-side gate: the worker
/// re-reads what the row carries, so planting the payload here is the direct
/// way to present a plan the enqueue admission would have refused.
///
/// # Panics
///
/// Panics when the update fails.
async fn persist_cleanup_plan(
    fixture: &PromotionIntegrationFixture,
    task_id: Uuid,
    payload: &ExpiredCleanupPayload,
) {
    sqlx::query("UPDATE vala.forge_tasks SET plan=$2::jsonb WHERE task_id=$1")
        .bind(task_id)
        .bind(
            serde_json::json!({"version":1,"inputs":[],"parameters":payload.to_value()})
                .to_string(),
        )
        .execute(fixture.operator_pool.pool())
        .await
        .expect("the cleanup plan is replanted");
}

/// Returns one cleanup task to the claim pool without touching its evidence.
///
/// # Panics
///
/// Panics when the release fails.
async fn release_cleanup_claim(fixture: &PromotionIntegrationFixture, task_id: Uuid) {
    sqlx::query("UPDATE vala.forge_tasks SET state='ready',attempt_id=NULL,claimed_by=NULL,claim_expires_at=NULL,watermark_snapshot_id=NULL,watermark_timestamp_ms=NULL WHERE task_id=$1")
        .bind(task_id)
        .execute(fixture.operator_pool.pool())
        .await
        .expect("the cleanup claim releases");
}

/// Counts every live maintenance lease, which a refused claim must not change.
///
/// # Panics
///
/// Panics when the diagnostic read fails.
async fn live_leases(fixture: &PromotionIntegrationFixture) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM vala.maintenance_leases")
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("lease count")
}

#[tokio::test]
async fn candidate_preparation_releases_sql_and_blocks_oracle_and_competing_forge_claims() {
    let DrainedExpiration {
        table,
        worker,
        cleanup_id,
        payload,
    } = Box::pin(drained_expiration("cleanup_gates")).await;
    let deletes_before = table.store.deletes();

    // A persisted cleanup plan whose candidate is internally valid but belongs
    // to a sibling table is refused before the lease, the catalog, and any
    // object-store call.
    let sibling = ExpiredCleanupPayload {
        cleanup_candidates: vec![ForgeCleanupCandidate {
            category: ForgeCleanupCategory::Data,
            table: ForgeTaskTableIdentity::new(
                "wyrd-redux",
                table.fixture.binding.table_ref.namespace.as_str(),
                "sibling",
            )
            .expect("sibling identity"),
            path: ForgeCleanupPath::new("sibling/data/00000.parquet").expect("sibling path"),
        }],
        ..payload.clone()
    };
    persist_cleanup_plan(&table.fixture, cleanup_id, &sibling).await;
    let leases_before = live_leases(&table.fixture).await;
    let stats_before = table.store.stats();
    let stray = worker
        .claim_for_test()
        .await
        .expect("claim transaction runs")
        .expect("the replanted cleanup task is claimable");
    let refused = worker
        .execute_expired_cleanup_claim_for_test(stray, &CancellationToken::new())
        .await
        .expect_err("a cross-table cleanup plan is refused");
    assert!(
        matches!(refused, ForgeError::Sql(_)),
        "the cross-table plan is refused by the payload contract: {refused}"
    );
    assert_eq!(
        table.store.stats(),
        stats_before,
        "a refused cross-table plan stats no object"
    );
    assert_eq!(
        table.store.deletes(),
        deletes_before,
        "a refused cross-table plan deletes no object"
    );
    assert_eq!(
        live_leases(&table.fixture).await,
        leases_before,
        "a refused cross-table plan acquires no table lease"
    );
    assert_eq!(
        cursor(&table.fixture, cleanup_id).await,
        ("claimed".to_owned(), 0, None),
        "a refused cross-table plan writes no cleanup evidence"
    );
    persist_cleanup_plan(&table.fixture, cleanup_id, &payload).await;
    release_cleanup_claim(&table.fixture, cleanup_id).await;

    let claim = worker
        .claim_for_test()
        .await
        .expect("claim transaction runs")
        .expect("the cleanup task is claimable");
    assert_eq!(claim.task_id, cleanup_id);
    assert_eq!(
        claim.strategy.as_str(),
        "expired_cleanup",
        "the claim is the cleanup task"
    );

    // The one-active-task index refuses a second Forge claim for this table
    // while the cleanup task holds it.
    assert!(
        worker
            .claim_for_test()
            .await
            .expect("second claim transaction runs")
            .is_none(),
        "an active cleanup task excludes every competing Forge claim for the table"
    );

    worker
        .execute_expired_cleanup_claim_for_test(claim, &CancellationToken::new())
        .await
        .expect("the drained cleanup task succeeds");

    let (state, frontier, prepared) = cursor(&table.fixture, cleanup_id).await;
    assert_eq!(state, "succeeded");
    assert_eq!(frontier as usize, payload.cleanup_candidates.len());
    assert_eq!(
        prepared, None,
        "success requires an empty prepared frontier"
    );
    assert_eq!(
        table.store.deletes(),
        deletes_before + payload.cleanup_candidates.len(),
        "each candidate is deleted exactly once"
    );
    for candidate in &payload.cleanup_candidates {
        assert!(
            !object_exists(&table.fixture, candidate.path.as_str()).await,
            "every prepared candidate is gone"
        );
    }

    let sequence = audits(&table.fixture, cleanup_id).await;
    assert_eq!(
        sequence
            .iter()
            .filter(|entry| *entry == "forge.expired_cleanup.candidate_prepared")
            .count(),
        payload.cleanup_candidates.len(),
        "one preparation audit per candidate: {sequence:?}"
    );
    assert_eq!(
        sequence
            .iter()
            .filter(
                |entry| entry.starts_with("forge.expired_cleanup.candidate_deleted")
                    || entry.starts_with("forge.expired_cleanup.candidate_missing")
            )
            .count(),
        payload.cleanup_candidates.len(),
        "one advancing settlement audit per candidate: {sequence:?}"
    );
    assert_eq!(
        sequence
            .iter()
            .filter(|entry| *entry == "forge.task.succeeded")
            .count(),
        1
    );

    table.supervised.shutdown().await;
}

/// Asserts a worker that does not own the claim cannot drain or take it over.
///
/// Keeps the durable owner assertion out of the scenario body so the replay
/// sequence stays one readable narrative.
async fn assert_foreign_takeover_is_refused(
    table: &ExpirableTable,
    claim: &ForgeTaskClaim,
    cleanup_id: Uuid,
    pool: &sqlx::PgPool,
) {
    let foreign = ForgeWorker::new(
        table.supervised.forge(),
        ForgeWorkerConfig::default(),
        Uuid::now_v7(),
    )
    .expect("second worker");
    assert!(
        foreign
            .execute_expired_cleanup_claim_for_test(claim.clone(), &CancellationToken::new())
            .await
            .is_err(),
        "a worker that does not own the claim cannot drain it"
    );
    let owner: Option<Uuid> =
        sqlx::query_scalar("SELECT claimed_by FROM vala.forge_tasks WHERE task_id = $1")
            .bind(cleanup_id)
            .fetch_one(pool)
            .await
            .expect("cleanup task readable");
    assert_eq!(owner, Some(claim.claimed_by.expect("claim owner")));
}

#[tokio::test]
async fn cursor_replays_exact_prepared_candidate_after_refusal_uncertainty_and_takeover() {
    let DrainedExpiration {
        table,
        worker,
        cleanup_id,
        payload,
    } = Box::pin(drained_expiration("cleanup_replay")).await;
    let pool = table.fixture.operator_pool.pool();

    let claim = worker
        .claim_for_test()
        .await
        .expect("claim transaction runs")
        .expect("the cleanup task is claimable");
    let attempt = claim.attempt_id.expect("a claimed task has an attempt");

    // Cancelling before the first delete is submitted retains the candidate:
    // nothing advances and the exact plan vector is unchanged.
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    let refused = worker
        .execute_expired_cleanup_claim_for_test(claim.clone(), &cancelled)
        .await
        .expect_err("a cancelled drain does not succeed");
    assert!(
        matches!(refused, ForgeError::Shutdown | ForgeError::ShutdownRetained),
        "pre-submission cancellation is a cooperative stop: {refused}"
    );
    let (_, frontier, _) = cursor(&table.fixture, cleanup_id).await;
    assert_eq!(
        frontier, 0,
        "cancellation before submission advances nothing"
    );
    for candidate in &payload.cleanup_candidates {
        assert!(
            object_exists(&table.fixture, candidate.path.as_str()).await,
            "no object is deleted before a candidate is prepared and submitted"
        );
    }

    assert_foreign_takeover_is_refused(&table, &claim, cleanup_id, pool).await;

    // A cooperatively cancelled pre-effect attempt released nothing durable, so
    // the task returns to the claim pool and a successor drains the identical
    // immutable candidate vector to success.
    sqlx::query("UPDATE vala.forge_tasks SET state='ready',attempt_id=NULL,claimed_by=NULL,claim_expires_at=NULL,watermark_snapshot_id=NULL,watermark_timestamp_ms=NULL WHERE task_id=$1")
        .bind(cleanup_id)
        .execute(pool)
        .await
        .expect("the cancelled claim releases");
    let successor = worker
        .claim_for_test()
        .await
        .expect("claim transaction runs")
        .expect("the released cleanup task is claimable again");
    assert_eq!(successor.task_id, cleanup_id);
    assert_ne!(
        successor.attempt_id,
        Some(attempt),
        "a released pre-effect claim is re-attempted"
    );
    assert_eq!(
        successor.plan.parameters, claim.plan.parameters,
        "the copied cleanup plan is immutable across attempts"
    );
    worker
        .execute_expired_cleanup_claim_for_test(successor, &CancellationToken::new())
        .await
        .expect("the retained candidates replay to success");
    let (state, frontier, prepared) = cursor(&table.fixture, cleanup_id).await;
    assert_eq!(state, "succeeded");
    assert_eq!(frontier as usize, payload.cleanup_candidates.len());
    assert_eq!(prepared, None);
    let sequence = audits(&table.fixture, cleanup_id).await;
    assert_eq!(
        sequence
            .iter()
            .filter(|entry| *entry == "forge.expired_cleanup.candidate_prepared")
            .count(),
        payload.cleanup_candidates.len(),
        "no candidate is prepared twice: {sequence:?}"
    );
    for candidate in &payload.cleanup_candidates {
        assert!(
            !object_exists(&table.fixture, candidate.path.as_str()).await,
            "every candidate is finally removed"
        );
    }

    // No destructive maintenance other than the handoff pair ran: scribe
    // promotion is ingest publication, not a maintenance effect.
    let strategies: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT strategy FROM vala.forge_tasks ORDER BY strategy")
            .fetch_all(pool)
            .await
            .expect("strategy list");
    assert!(
        strategies.iter().all(|value| matches!(
            value.as_str(),
            "expired_cleanup" | "snapshot_expiry" | "scribe_promotion"
        )),
        "cleanup runs no compaction or orphan effect: {strategies:?}"
    );

    let _ = Arc::strong_count(&table.store);
    table.supervised.shutdown().await;
}
