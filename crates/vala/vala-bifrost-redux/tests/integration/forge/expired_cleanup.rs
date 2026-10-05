//! Tier-2 coverage for the separate expired-object cleanup task.
//!
//! Cleanup consumes an immutable handoff one committed expiration left behind,
//! prepares exactly one candidate at a time with Postgres closed before any
//! object-store call, refuses while any Oracle query holds an active read on the
//! table, and advances only for a confirmed deletion or a proven absence.

use std::sync::Arc;

use chrono::Duration as ChronoDuration;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use vala_bifrost_redux::catalog::TenantTableBinding;
use vala_bifrost_redux::forge::{
    Forge, ForgeError, ForgeWorker, ForgeWorkerConfig, cleanup_projection,
};
use vala_sql::queries::forge_tasks::ForgeTasks;
use vala_sql::row_types::forge_tasks::{
    ExpiredCleanupPayload, ForgeCleanupCandidate, ForgeCleanupCategory, ForgeCleanupPath,
    ForgeTaskClaim, ForgeTaskStrategy, ForgeTaskTableIdentity, NewForgeTask,
};
use vala_sql::row_types::oracle_reader_authority::TableAuthorityIdentity;

use super::snapshot_expiration::{
    ExpirableTable, expirable_table, object_exists, seed_ready_expiry_task,
};
use super::support::{ForgeTelemetryCheckpoint, PromotionIntegrationFixture};

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
    let table = expirable_table(name).await;
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

    let key = vala_bifrost_redux::forge::ForgeTableKey {
        tenant: table.fixture.tenant,
        table: identity.clone(),
    };
    let projected: NewForgeTask =
        cleanup_projection(&key, &payload).expect("the fixed bounded projection builds");
    assert_eq!(projected.strategy, ForgeTaskStrategy::ExpiredCleanup);
    assert_eq!(projected.base_snapshot_id, payload.committed_snapshot_id);
    assert_eq!(
        projected.estimates.files as usize,
        payload.cleanup_candidates.len()
    );
    assert!(projected.plan.inputs.is_empty());
    tasks
        .enqueue(&projected)
        .await
        .expect("the handoff admits one cleanup enqueue");
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

/// Builds a path beside one candidate, sharing its table-bound directory.
///
/// Staying in the candidate's own directory keeps the derived path inside the
/// table binding, so a refusal it produces is the safety rule under test rather
/// than a path-shape rejection.
///
/// # Panics
///
/// Panics when the candidate path carries no directory.
fn sibling_path(candidate: &ForgeCleanupCandidate, file_name: &str) -> String {
    let (directory, _) = candidate
        .path
        .as_str()
        .rsplit_once('/')
        .expect("the candidate path has a directory");
    format!("{directory}/{file_name}")
}

/// Asserts the preparation committed and released its table-authority lock.
///
/// The drain is suspended between its committed preparation and its first
/// external call, so an independent transaction taking the same row without
/// waiting is the direct proof that no Postgres resource spans object IO.
///
/// # Panics
///
/// Panics when the cursor differs, a deletion was submitted, or the probe lock
/// is blocked.
async fn assert_preparation_closed_its_transaction(
    table: &ExpirableTable,
    identity: &TableAuthorityIdentity,
    cleanup_id: Uuid,
) {
    assert_eq!(
        cursor(&table.fixture, cleanup_id).await,
        ("prepared".to_owned(), 0, Some(0)),
        "the preparation is durable before the first object call"
    );
    assert_eq!(
        table.store.deletes(),
        0,
        "no deletion is submitted before the fresh proof completes"
    );
    let mut lock = table
        .fixture
        .operator_pool
        .pool()
        .begin()
        .await
        .expect("independent transaction");
    tokio::time::timeout(
        std::time::Duration::from_secs(30),
        sqlx::query("SELECT 1 FROM vala.bifrost_table_maintenance_authority WHERE data_tenant_id=$1 AND catalog_name=$2 AND namespace_name=$3 AND table_name=$4 FOR UPDATE NOWAIT")
            .bind(identity.tenant.as_uuid())
            .bind(&identity.catalog_name)
            .bind(&identity.namespace_name)
            .bind(&identity.table_name)
            .fetch_one(&mut *lock),
    )
    .await
    .expect("the independent lock is not blocked")
    .expect("the preparation transaction released the table-authority row");
    lock.rollback().await.expect("release the probe lock");
}

/// Asserts an unresolved preparation excludes every competing Forge authority.
///
/// A second Forge claim and a competing destructive effect are each refused by
/// their own production mechanism while the prepared row stands.
///
/// # Panics
///
/// Panics when any competitor is admitted.
async fn assert_prepared_candidate_excludes_competitors(
    worker: &ForgeWorker,
    forge: &Arc<Forge>,
    binding: &TenantTableBinding,
) {
    assert!(
        worker
            .claim_for_test()
            .await
            .expect("claim transaction runs")
            .is_none(),
        "the prepared cleanup task excludes every competing Forge claim"
    );
    assert!(
        matches!(
            forge.run_orphan_gc_report_for_test(binding).await,
            Err(ForgeError::FenceLost { .. })
        ),
        "the live table lease denies a competing destructive effect"
    );
}

/// Asserts the drain's self-exemption is exact in all four of its fields.
///
/// Every axis classifies the same real object and only the exemption tuple
/// varies, so a refusal is attributable to the one mismatched field. Live-set
/// containment and path binding are then mutated one at a time through the same
/// production proof.
///
/// # Panics
///
/// Panics when any exemption axis or safety input classifies unexpectedly.
async fn assert_self_exemption_is_exact(
    fixture: &PromotionIntegrationFixture,
    forge: &Arc<Forge>,
    binding: &TenantTableBinding,
    cleanup_id: Uuid,
    attempt: Uuid,
    candidate: &ForgeCleanupCandidate,
) {
    let eligibility = async |task_id, attempt_id, index, exempted: &ForgeCleanupCandidate, path| {
        forge
            .expired_cleanup_eligibility_for_test(
                binding, task_id, attempt_id, index, exempted, path,
            )
            .await
            .expect("the production protection proof loads")
    };
    let path = candidate.path.as_str();
    assert_eq!(
        eligibility(cleanup_id, attempt, 0, candidate, path).await,
        "Eligible",
        "the exact prepared tuple proceeds"
    );
    // The fresh proof before each delete re-reads the active table reads, so
    // a query admitted after the preparation still blocks the delete.
    let reader = fixture.hold_active_read().await;
    assert_eq!(
        eligibility(cleanup_id, attempt, 0, candidate, path).await,
        "Protected",
        "an active table read blocks even the exact prepared tuple"
    );
    fixture.release_active_read(reader).await;
    assert_eq!(
        eligibility(cleanup_id, attempt, 0, candidate, path).await,
        "Eligible",
        "the released read no longer blocks the prepared tuple"
    );
    for (task_id, attempt_id, index, mismatch) in [
        (Uuid::now_v7(), attempt, 0, "task id"),
        (cleanup_id, Uuid::now_v7(), 0, "attempt id"),
        (cleanup_id, attempt, 1, "cursor index"),
    ] {
        assert_eq!(
            eligibility(task_id, attempt_id, index, candidate, path).await,
            "Protected",
            "a mismatched {mismatch} leaves the preparation protecting its object"
        );
    }
    let sibling_candidate = ForgeCleanupCandidate {
        path: ForgeCleanupPath::new(sibling_path(candidate, "wyrd-other-candidate.parquet"))
            .expect("a sibling candidate path is valid"),
        ..candidate.clone()
    };
    assert_eq!(
        eligibility(cleanup_id, attempt, 0, &sibling_candidate, path).await,
        "Protected",
        "a mismatched candidate leaves the preparation protecting its object"
    );

    let live = forge
        .load_maintenance_protection_for_test(binding)
        .await
        .expect("the production live set loads");
    let reachable = live
        .paths_for_test()
        .iter()
        .next()
        .expect("the live table still reaches at least one object")
        .clone();
    assert_eq!(
        eligibility(cleanup_id, attempt, 0, candidate, &reachable).await,
        "Protected",
        "an object the live catalog still reaches is never eligible"
    );
    assert_eq!(
        eligibility(
            cleanup_id,
            attempt,
            0,
            candidate,
            "../outside/escape.parquet"
        )
        .await,
        "InvalidPath",
        "a path outside the table binding is never eligible"
    );
}

/// Asserts every durable gate that must hold at the paused object-store stat.
///
/// The drain is suspended between its committed preparation and its first
/// external call, so each assertion here observes production state directly:
/// the SQL transaction is closed, the prepared row is the competing-Forge
/// boundary, an active table read blocks the delete, and the self-exemption
/// covers exactly one tuple.
///
/// # Panics
///
/// Panics when any gate, lock, refusal, or exemption boundary differs.
#[expect(
    clippy::too_many_arguments,
    reason = "the gate proof observes the whole live drain context and owns no state of its own"
)]
async fn assert_paused_candidate_gates(
    table: &ExpirableTable,
    worker: &ForgeWorker,
    forge: &Arc<Forge>,
    binding: &TenantTableBinding,
    identity: &TableAuthorityIdentity,
    cleanup_id: Uuid,
    attempt: Uuid,
    payload: &ExpiredCleanupPayload,
) {
    assert_preparation_closed_its_transaction(table, identity, cleanup_id).await;
    assert_prepared_candidate_excludes_competitors(worker, forge, binding).await;
    assert_self_exemption_is_exact(
        &table.fixture,
        forge,
        binding,
        cleanup_id,
        attempt,
        &payload.cleanup_candidates[0],
    )
    .await;
}

/// Asserts a persisted cross-table cleanup plan is refused before any effect.
///
/// The candidate is internally valid but names a sibling table, so the consumer
/// gate must reject it ahead of the table lease, the catalog, and every
/// object-store call. The original plan and claim are restored afterwards so the
/// caller continues from an untouched task.
///
/// # Panics
///
/// Panics when the plan is admitted or any counter, lease, or cursor moved.
async fn assert_cross_table_plan_is_refused(
    table: &ExpirableTable,
    worker: &ForgeWorker,
    cleanup_id: Uuid,
    payload: &ExpiredCleanupPayload,
) {
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
    let Some(stray) = worker
        .claim_for_test()
        .await
        .expect("claim transaction runs")
    else {
        panic!(
            "the replanted cleanup task is claimable; tasks at {}: {:?}",
            chrono::Utc::now(),
            table.fixture.forge_tasks().await
        );
    };
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
    persist_cleanup_plan(&table.fixture, cleanup_id, payload).await;
    release_cleanup_claim(&table.fixture, cleanup_id).await;
}

#[tokio::test]
async fn candidate_preparation_releases_sql_and_blocks_active_reads_and_competing_forge_claims() {
    let _telemetry = ForgeTelemetryCheckpoint::install();
    let DrainedExpiration {
        table,
        worker,
        cleanup_id,
        payload,
    } = Box::pin(drained_expiration("cleanup_gates")).await;
    let deletes_before = table.store.deletes();
    assert_cross_table_plan_is_refused(&table, &worker, cleanup_id, &payload).await;

    let Some(claim) = worker
        .claim_for_test()
        .await
        .expect("claim transaction runs")
    else {
        panic!(
            "the cleanup task is claimable; tasks at {}: {:?}",
            chrono::Utc::now(),
            table.fixture.forge_tasks().await
        );
    };
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

    // Suspend the first candidate's fresh reachability stat. That instant is
    // after the preparation committed and before any deletion was submitted,
    // which is the only place the durable gates can be observed at all.
    let attempt = claim.attempt_id.expect("a claimed task has an attempt");
    let identity = table.fixture.table_identity().await;
    let binding = table.fixture.binding.clone();
    let forge = table.supervised.forge();
    table.store.pause_stat_at(1);
    let drain_stop = CancellationToken::new();
    let (drained, ()) = tokio::join!(
        worker.execute_expired_cleanup_claim_for_test(claim, &drain_stop),
        async {
            table.store.stat_paused().await;
            assert_paused_candidate_gates(
                &table, &worker, &forge, &binding, &identity, cleanup_id, attempt, &payload,
            )
            .await;
            table.store.release_stat();
        }
    );
    drained.expect("the drained cleanup task succeeds");

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

    // Cleanup evaluates no principal permission, so the task's own evidence is
    // its lineage: every candidate advanced the cursor exactly once and none
    // stayed prepared.
    let (state, advanced, prepared) = cursor(&table.fixture, cleanup_id).await;
    assert_eq!(state, "succeeded");
    assert_eq!(
        usize::try_from(advanced).expect("advanced count is representable"),
        payload.cleanup_candidates.len(),
        "the cursor advanced exactly once per candidate"
    );
    assert_eq!(prepared, None, "no candidate stayed prepared");

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

/// Seeds more real expired objects and returns their plan candidates.
///
/// Partial progress and a nonzero resume frontier are only observable across
/// several candidates, and the handoff a single fixture expiration leaves
/// carries exactly one. Each object is written beside the existing candidate so
/// it stays inside the table binding, and the manual Forge clock — the only age
/// authority in this fixture — is moved past them so the shared minimum-age
/// proof admits them.
///
/// # Panics
///
/// Panics when an object cannot be written or the manual clock cannot advance.
async fn seed_extra_candidates(
    table: &ExpirableTable,
    payload: &ExpiredCleanupPayload,
    file_names: &[&str],
) -> Vec<ForgeCleanupCandidate> {
    let base = payload
        .cleanup_candidates
        .first()
        .expect("the handoff carries at least one candidate");
    let mut seeded = Vec::with_capacity(file_names.len());
    for file_name in file_names {
        let path = sibling_path(base, file_name);
        table
            .fixture
            .staging
            .write(&path, b"seeded expired candidate".to_vec())
            .await
            .expect("the extra candidate object seeds");
        seeded.push(ForgeCleanupCandidate {
            category: base.category,
            table: base.table.clone(),
            path: ForgeCleanupPath::new(&path).expect("extra candidate path"),
        });
    }
    let advanced = table.control.now().expect("manual clock reads") + ChronoDuration::days(7);
    table
        .control
        .set(advanced)
        .expect("the manual clock advances past the seeded objects");
    seeded
}

/// Lapses one task's claim lease so the prepared-recovery route can take it.
///
/// This is the durable trace a crashed owner leaves: the prepared row and its
/// cursor survive untouched while the lease simply stops being renewed.
///
/// # Panics
///
/// Panics when the update fails.
pub(super) async fn expire_claim(pool: &sqlx::PgPool, task_id: Uuid) {
    sqlx::query("UPDATE vala.forge_tasks SET claim_expires_at=statement_timestamp()-interval '1 hour' WHERE task_id=$1")
        .bind(task_id)
        .execute(pool)
        .await
        .expect("the claim lease lapses");
}

/// Reads one task's current attempt generation.
///
/// # Panics
///
/// Panics when the task is unreadable.
async fn attempt_of(pool: &sqlx::PgPool, task_id: Uuid) -> Option<Uuid> {
    sqlx::query_scalar("SELECT attempt_id FROM vala.forge_tasks WHERE task_id = $1")
        .bind(task_id)
        .fetch_one(pool)
        .await
        .expect("cleanup task readable")
}

/// Asserts cancellation before the first preparation releases nothing durable.
///
/// # Panics
///
/// Panics when the stop is not cooperative, a cursor was written, or an object
/// was touched.
async fn assert_cancellation_before_preparation_is_inert(
    table: &ExpirableTable,
    worker: &ForgeWorker,
    claim: &ForgeTaskClaim,
    cleanup_id: Uuid,
    payload: &ExpiredCleanupPayload,
) {
    // Cancelling before the first preparation is a cooperative stop: nothing
    // durable was written, so nothing advances and no object is touched.
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    let refused = worker
        .execute_expired_cleanup_claim_for_test(claim.clone(), &cancelled)
        .await
        .expect_err("a cancelled drain does not succeed");
    assert!(
        matches!(refused, ForgeError::Shutdown | ForgeError::ShutdownRetained),
        "cancellation before preparation is a cooperative stop: {refused}"
    );
    assert_eq!(
        cursor(&table.fixture, cleanup_id).await,
        ("running".to_owned(), 0, None),
        "cancellation before preparation writes no cursor"
    );
    for candidate in &payload.cleanup_candidates {
        assert!(
            object_exists(&table.fixture, candidate.path.as_str()).await,
            "no object is deleted before a candidate is prepared and submitted"
        );
    }
}

/// Asserts a stat failure after preparation settles as an authoritative refusal.
///
/// # Panics
///
/// Panics when the failure propagates instead of settling, a delete is
/// submitted, or the cursor advances.
async fn assert_stat_failure_settles_as_refusal(
    table: &ExpirableTable,
    worker: &ForgeWorker,
    claim: &ForgeTaskClaim,
    cleanup_id: Uuid,
    deletes_before: usize,
) {
    // A stat failure lands after the preparation committed, so it is a
    // definitive pre-submission failure: it settles as a refusal and retains
    // candidate zero for exact replay.
    let stats_before = table.store.stats();
    table.store.fail_next_stats(1);
    let retained = worker
        .execute_expired_cleanup_claim_for_test(claim.clone(), &CancellationToken::new())
        .await
        .expect_err("a refused candidate does not complete the drain");
    assert!(
        matches!(retained, ForgeError::CleanupRetained { index: 0, .. }),
        "a refusal retains the candidate for replay: {retained}"
    );
    assert!(
        table.store.stats() > stats_before,
        "the refusal happened at a real object stat"
    );
    assert_eq!(
        cursor(&table.fixture, cleanup_id).await,
        ("prepared".to_owned(), 0, Some(0)),
        "a refused candidate stays prepared at its own index"
    );
    assert_eq!(
        table.store.deletes(),
        deletes_before,
        "a pre-submission refusal submits no delete"
    );
}

/// Asserts cancellation released from a paused stat settles as a refusal.
///
/// The takeover worker resumes the prepared candidate, re-proves it with a
/// fresh stat, and observes the stop strictly before the delete is constructed.
///
/// # Panics
///
/// Panics when the cancellation propagates instead of settling, a delete is
/// submitted, the cursor advances, or the attempt identity changes.
async fn assert_cancelled_stat_settles_as_refusal(
    table: &ExpirableTable,
    taker: &ForgeWorker,
    cleanup_id: Uuid,
    attempt: Uuid,
    deletes_before: usize,
) {
    let pool = table.fixture.operator_pool.pool();
    // Cancellation released from a paused stat is also strictly pre-submission.
    let stats_before = table.store.stats();
    table.store.pause_stat_at(1);
    let stop = CancellationToken::new();
    let (result, ()) = tokio::join!(taker.execute_one_for_test(&stop), async {
        table.store.stat_paused().await;
        stop.cancel();
        table.store.release_stat();
    });
    let retained = result.expect_err("pre-submission cancellation retains the candidate");
    assert!(
        matches!(retained, ForgeError::CleanupRetained { index: 0, .. }),
        "cancellation observed before submission settles as a refusal: {retained}"
    );
    assert!(
        table.store.stats() > stats_before,
        "takeover re-stats the candidate before any retry"
    );
    assert_eq!(
        cursor(&table.fixture, cleanup_id).await,
        ("prepared".to_owned(), 0, Some(0)),
        "cancellation before submission advances nothing"
    );
    assert_eq!(
        table.store.deletes(),
        deletes_before,
        "cancellation before submission submits no delete"
    );
    assert_eq!(
        attempt_of(pool, cleanup_id).await,
        Some(attempt),
        "takeover resumes the same task and attempt"
    );
}

/// Asserts the displaced owner cannot mutate the prepared row.
///
/// # Panics
///
/// Panics when the stale drain succeeds or moves the cursor.
async fn assert_stale_owner_is_inert(
    table: &ExpirableTable,
    worker: &ForgeWorker,
    claim: ForgeTaskClaim,
    cleanup_id: Uuid,
) {
    assert!(
        worker
            .execute_expired_cleanup_claim_for_test(claim, &CancellationToken::new())
            .await
            .is_err(),
        "a displaced owner cannot drain the task it lost"
    );
    assert_eq!(
        cursor(&table.fixture, cleanup_id).await,
        ("prepared".to_owned(), 0, Some(0)),
        "a stale owner mutates no row"
    );
}

/// Asserts a delete that took effect but lost its acknowledgement is uncertain.
///
/// # Panics
///
/// Panics when the cursor advances past an object whose fate was unknown, the
/// or the delete count is wrong.
async fn assert_lost_acknowledgement_is_uncertain(
    table: &ExpirableTable,
    taker: &ForgeWorker,
    cleanup_id: Uuid,
    first: &str,
    deletes_before: usize,
) {
    let pool = table.fixture.operator_pool.pool();
    // A delete that took effect but reported failure is uncertain: the object is
    // gone, yet nothing advances, so the same candidate is replayed rather than
    // skipped past an object whose fate was unknown.
    expire_claim(pool, cleanup_id).await;
    table.store.fail_next_deletes(1);
    let retained = taker
        .execute_one_for_test(&CancellationToken::new())
        .await
        .expect_err("an uncertain acceptance does not complete the drain");
    assert!(
        matches!(retained, ForgeError::CleanupRetained { index: 0, .. }),
        "an unknown acceptance retains the candidate: {retained}"
    );
    assert_eq!(
        table.store.deletes(),
        deletes_before + 2,
        "exactly one further delete was submitted"
    );
    assert!(
        !object_exists(&table.fixture, first).await,
        "the submitted delete took effect even though its acknowledgement was lost"
    );
    assert_eq!(
        cursor(&table.fixture, cleanup_id).await,
        ("prepared".to_owned(), 0, Some(0)),
        "an uncertain acceptance advances nothing"
    );
}

/// Asserts cancellation after the delete future is polled records uncertainty.
///
/// The delete is suspended inside the polled deletion future, which is the only
/// point where the pinned submission boundary can be crossed: the object store
/// was reached, so its acceptance is unknown and the candidate must be retained
/// rather than refused or advanced past.
///
/// # Panics
///
/// Panics when the cancellation is settled as anything but uncertainty, the
/// cursor advances, or the object was actually removed.
async fn assert_polled_delete_cancellation_is_uncertain(
    table: &ExpirableTable,
    taker: &ForgeWorker,
    cleanup_id: Uuid,
    first: &str,
    deletes_before: usize,
) {
    let pool = table.fixture.operator_pool.pool();
    expire_claim(pool, cleanup_id).await;
    table.store.pause_delete_at(1);
    let stop = CancellationToken::new();
    let (result, ()) = tokio::join!(taker.execute_one_for_test(&stop), async {
        table.store.delete_paused().await;
        stop.cancel();
    });
    let retained = result.expect_err("cancellation racing a submitted delete does not complete");
    assert!(
        matches!(retained, ForgeError::CleanupRetained { index: 0, .. }),
        "cancellation after submission retains the candidate: {retained}"
    );
    assert_eq!(
        table.store.deletes(),
        deletes_before + 1,
        "the deletion was submitted before the stop was observed"
    );
    assert!(
        object_exists(&table.fixture, first).await,
        "the cancelled deletion never reached the real object store"
    );
    assert_eq!(
        cursor(&table.fixture, cleanup_id).await,
        ("prepared".to_owned(), 0, Some(0)),
        "an unknown acceptance advances nothing"
    );
}

/// Asserts the drain advances one candidate and then stops at the next.
///
/// The first candidate's absence is proven by a fresh stat, so the cursor
/// advances exactly once; the second candidate is then prepared and its delete
/// is cancelled inside the polled future, which leaves the task at a nonzero
/// partial frontier for the successor to resume from.
///
/// # Panics
///
/// Panics when the frontier is not exactly one, the prepared index is not the
/// second candidate, or the second object was removed.
async fn assert_partial_frontier_is_left_for_the_successor(
    table: &ExpirableTable,
    taker: &ForgeWorker,
    cleanup_id: Uuid,
    second: &str,
    deletes_before: usize,
) {
    let pool = table.fixture.operator_pool.pool();
    expire_claim(pool, cleanup_id).await;
    table.store.pause_delete_at(1);
    let stop = CancellationToken::new();
    let (result, ()) = tokio::join!(taker.execute_one_for_test(&stop), async {
        table.store.delete_paused().await;
        stop.cancel();
    });
    assert!(
        matches!(
            result.expect_err("the interrupted drain does not complete"),
            ForgeError::CleanupRetained { index: 1, .. }
        ),
        "the second candidate is retained at the partial frontier"
    );
    assert_eq!(
        cursor(&table.fixture, cleanup_id).await,
        ("prepared".to_owned(), 1, Some(1)),
        "the proven-absent first candidate advanced once and the second is prepared"
    );
    assert_eq!(
        table.store.deletes(),
        deletes_before + 3,
        "a proven absence submits no delete and the second candidate submits one"
    );
    assert!(
        object_exists(&table.fixture, second).await,
        "the cancelled deletion never reached the real object store"
    );
}

/// Asserts the finished cleanup task's terminal cursor and effects.
///
/// # Panics
///
/// Panics when the task did not succeed from its partial frontier, an object
/// survived, a candidate was deleted twice, or another destructive maintenance
/// strategy ran.
async fn assert_cleanup_finished_exactly(
    table: &ExpirableTable,
    cleanup_id: Uuid,
    payload: &ExpiredCleanupPayload,
    deletes_before: usize,
) {
    let pool = table.fixture.operator_pool.pool();
    let (state, frontier, prepared) = cursor(&table.fixture, cleanup_id).await;
    assert_eq!(state, "succeeded");
    assert_eq!(frontier as usize, payload.cleanup_candidates.len());
    assert_eq!(prepared, None);
    assert_eq!(
        table.store.deletes(),
        deletes_before + 5,
        "no settled candidate is ever blindly deleted again"
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
}

#[tokio::test]
async fn cursor_replays_exact_prepared_candidate_after_refusal_uncertainty_and_takeover() {
    let _telemetry = ForgeTelemetryCheckpoint::install();
    let DrainedExpiration {
        table,
        worker,
        cleanup_id,
        mut payload,
    } = Box::pin(drained_expiration("cleanup_replay")).await;
    let pool = table.fixture.operator_pool.pool();
    let deletes_before = table.store.deletes();

    payload.cleanup_candidates.extend(
        seed_extra_candidates(
            &table,
            &payload,
            &["wyrd-second-expired.parquet", "wyrd-third-expired.parquet"],
        )
        .await,
    );
    payload.cleanup_candidates.sort();
    assert_eq!(payload.cleanup_candidates.len(), 3);
    persist_cleanup_plan(&table.fixture, cleanup_id, &payload).await;
    let first = payload.cleanup_candidates[0].path.as_str().to_owned();
    let second = payload.cleanup_candidates[1].path.as_str().to_owned();

    let Some(claim) = worker
        .claim_for_test()
        .await
        .expect("claim transaction runs")
    else {
        panic!(
            "the cleanup task is claimable; tasks at {}: {:?}",
            chrono::Utc::now(),
            table.fixture.forge_tasks().await
        );
    };
    let attempt = claim.attempt_id.expect("a claimed task has an attempt");

    assert_cancellation_before_preparation_is_inert(&table, &worker, &claim, cleanup_id, &payload)
        .await;

    assert_foreign_takeover_is_refused(&table, &claim, cleanup_id, pool).await;

    // A cooperatively cancelled pre-effect attempt released nothing durable, so
    // the task returns to the claim pool with the identical immutable plan.
    release_cleanup_claim(&table.fixture, cleanup_id).await;
    let claim = worker
        .claim_for_test()
        .await
        .expect("claim transaction runs")
        .expect("the released cleanup task is claimable again");
    assert_eq!(claim.task_id, cleanup_id);
    let attempt = {
        let next = claim.attempt_id.expect("a claimed task has an attempt");
        assert_ne!(next, attempt, "a released pre-effect claim is re-attempted");
        next
    };

    assert_stat_failure_settles_as_refusal(&table, &worker, &claim, cleanup_id, deletes_before)
        .await;

    // The owner stops renewing its lease. The prepared task is recovered by the
    // production prepared-claim route under a new fence, retaining the same
    // task and attempt identity, and every retry re-proves the candidate.
    expire_claim(pool, cleanup_id).await;
    let taker = ForgeWorker::new(
        table.supervised.forge(),
        ForgeWorkerConfig::default(),
        Uuid::now_v7(),
    )
    .expect("takeover worker");

    assert_cancelled_stat_settles_as_refusal(&table, &taker, cleanup_id, attempt, deletes_before)
        .await;
    assert_stale_owner_is_inert(&table, &worker, claim, cleanup_id).await;
    assert_polled_delete_cancellation_is_uncertain(
        &table,
        &taker,
        cleanup_id,
        &first,
        deletes_before,
    )
    .await;
    assert_lost_acknowledgement_is_uncertain(&table, &taker, cleanup_id, &first, deletes_before)
        .await;
    assert_partial_frontier_is_left_for_the_successor(
        &table,
        &taker,
        cleanup_id,
        &second,
        deletes_before,
    )
    .await;

    // The successor resumes at the nonzero partial frontier: the settled first
    // candidate is never revisited, the retained second candidate is retried and
    // proven absent by the deletion itself, and the last candidate is deleted.
    expire_claim(pool, cleanup_id).await;
    assert_eq!(
        attempt_of(pool, cleanup_id).await,
        Some(attempt),
        "every takeover so far resumed the same task and attempt"
    );
    assert!(
        !object_exists(&table.fixture, &first).await,
        "the first candidate was already settled before this takeover"
    );
    table.store.not_found_next_deletes(1);
    assert!(
        taker
            .execute_one_for_test(&CancellationToken::new())
            .await
            .expect("the retained candidates replay to success"),
        "the prepared cleanup task is recovered"
    );
    assert_cleanup_finished_exactly(&table, cleanup_id, &payload, deletes_before).await;

    let _ = Arc::strong_count(&table.store);
    table.supervised.shutdown().await;
}

/// Proves a refused prepared candidate settles without an ownership conflict
/// and replays under its exact identity once the refusing root clears.
///
/// The fair-claimed attempt prepares candidate zero and is refused before its
/// delete; generic retry settlement accepts only `claimed`/`running` rows, so
/// the retained `prepared` row must bypass it. The same owner then replays the
/// prepared candidate through the production prepared-claim route while a fresh
/// active read — taken after the preparation committed — refuses the delete
/// proof. Releasing the read lets the identical task and attempt converge.
///
/// # Panics
///
/// Panics when a refusal reports an ownership or settlement error, the cursor
/// or attempt identity changes, a delete is submitted while refused, or the
/// replay does not converge after the read is released.
#[tokio::test]
async fn refused_prepared_cleanup_retains_identity_and_replays_after_root_clears() {
    let _telemetry = ForgeTelemetryCheckpoint::install();
    let DrainedExpiration {
        table,
        worker,
        cleanup_id,
        payload,
    } = Box::pin(drained_expiration("cleanup_root_refusal")).await;
    let pool = table.fixture.operator_pool.pool();
    let deletes_before = table.store.deletes();

    // Fair-claimed attempt: candidate zero is prepared, then refused before
    // any delete is submitted. The refusal is a retained outcome, not a
    // failed retry of a row that is no longer `claimed` or `running`.
    table.store.fail_next_stats(1);
    assert!(
        worker
            .execute_one_for_test(&CancellationToken::new())
            .await
            .expect("a refused prepared candidate settles without an ownership conflict"),
        "the fair-claimed cleanup attempt ran"
    );
    assert_eq!(
        cursor(&table.fixture, cleanup_id).await,
        ("prepared".to_owned(), 0, Some(0)),
        "the refused candidate stays prepared at its own index"
    );
    let attempt = attempt_of(pool, cleanup_id)
        .await
        .expect("the retained candidate keeps its attempt");

    // A query admitted after the preparation committed is a newly visible
    // root: the same owner's immediate replay refuses the delete proof again
    // and still reports the retained outcome rather than a slot-fatal error.
    let reader = table.fixture.hold_active_read().await;
    let refused = worker
        .execute_one_for_test(&CancellationToken::new())
        .await
        .expect_err("an active read refuses the prepared candidate's delete");
    assert!(
        matches!(refused, ForgeError::CleanupRetained { index: 0, .. }),
        "the newly visible root retains the candidate for replay: {refused}"
    );
    assert_eq!(
        cursor(&table.fixture, cleanup_id).await,
        ("prepared".to_owned(), 0, Some(0)),
        "the active read advances nothing"
    );
    assert_eq!(
        attempt_of(pool, cleanup_id).await,
        Some(attempt),
        "the refused replay keeps the exact task and attempt identity"
    );
    assert_eq!(
        table.store.deletes(),
        deletes_before,
        "no delete is submitted while the read is active"
    );

    // Once the root clears, the identical prepared identity converges.
    table.fixture.release_active_read(reader).await;
    assert!(
        worker
            .execute_one_for_test(&CancellationToken::new())
            .await
            .expect("the retained candidate replays after the read is released"),
        "the prepared cleanup task is reconciled"
    );
    let (state, frontier, prepared) = cursor(&table.fixture, cleanup_id).await;
    assert_eq!(state, "succeeded");
    assert_eq!(frontier as usize, payload.cleanup_candidates.len());
    assert_eq!(prepared, None);
    for candidate in &payload.cleanup_candidates {
        assert!(
            !object_exists(&table.fixture, candidate.path.as_str()).await,
            "every candidate is removed once the read is released"
        );
    }

    table.supervised.shutdown().await;
}

/// Copies one promoted `file_list` row under a new identity, path, and table.
///
/// The copy keeps the source row's settlement columns, so a copy of a
/// terminal row is terminal. A fresh writer node keeps the stream-range
/// uniqueness index satisfied.
///
/// # Panics
///
/// Panics when no promoted row exists or the insert fails.
async fn copy_terminal_row(fixture: &PromotionIntegrationFixture, path: &str, table_name: &str) {
    let copied = sqlx::query("INSERT INTO vala.file_list (id,data_tenant_id,namespace,table_name,file_path,file_size,row_count,min_event_time,max_event_time,partition_granularity,partition_start,compacted,committed_snapshot_id,node_id,writer_epoch,wal_lsn_min,wal_lsn_max,promotion_record,file_ordinal,file_checksum,forge_publication_operation_id) SELECT $1,data_tenant_id,namespace,$3,$2,file_size,row_count,min_event_time,max_event_time,partition_granularity,partition_start,compacted,committed_snapshot_id,$1,writer_epoch,wal_lsn_min,wal_lsn_max,promotion_record,file_ordinal,file_checksum,forge_publication_operation_id FROM vala.file_list WHERE data_tenant_id=$4 AND compacted AND committed_snapshot_id IS NOT NULL LIMIT 1")
        .bind(Uuid::now_v7())
        .bind(path)
        .bind(table_name)
        .bind(fixture.tenant.as_uuid())
        .execute(fixture.operator_pool.pool())
        .await
        .expect("the terminal row copies")
        .rows_affected();
    assert_eq!(copied, 1, "a promoted terminal row exists to copy");
}

/// Lists the tenant's `file_list` rows as `(table, path)` pairs.
///
/// # Panics
///
/// Panics when the read fails.
async fn file_list_paths(fixture: &PromotionIntegrationFixture) -> Vec<(String, String)> {
    sqlx::query_as("SELECT table_name, file_path FROM vala.file_list WHERE data_tenant_id=$1 ORDER BY table_name, file_path")
        .bind(fixture.tenant.as_uuid())
        .fetch_all(fixture.operator_pool.pool())
        .await
        .expect("file_list rows read")
}

/// Proves cleanup retires a terminal `file_list` row only with its object.
///
/// Two extra candidates carry terminal rows: one object is present, the other
/// is already absent, as after a delete that landed before its SQL settlement.
/// A terminal row at a non-candidate path and a same-path row of another table
/// are bystanders. A failed delete leaves every row in place; the drain that
/// follows removes exactly the candidates' rows, by deletion and by observed
/// absence, and keeps both bystanders.
///
/// # Panics
///
/// Panics when a row is removed before its object, a candidate row survives
/// the completed drain, or a bystander row is removed.
#[tokio::test]
async fn terminal_file_list_row_is_removed_only_after_object_cleanup() {
    let _telemetry = ForgeTelemetryCheckpoint::install();
    let DrainedExpiration {
        table,
        worker,
        cleanup_id,
        mut payload,
    } = Box::pin(drained_expiration("cleanup_file_list")).await;
    let fixture = &table.fixture;
    let seeded = seed_extra_candidates(
        &table,
        &payload,
        &[
            "wyrd-terminal-present.parquet",
            "wyrd-terminal-absent.parquet",
        ],
    )
    .await;
    let present = seeded[0].path.as_str().to_owned();
    let absent = seeded[1].path.as_str().to_owned();
    let bystander = sibling_path(&seeded[0], "wyrd-terminal-bystander.parquet");
    payload.cleanup_candidates.extend(seeded);
    payload.cleanup_candidates.sort();
    persist_cleanup_plan(fixture, cleanup_id, &payload).await;
    fixture
        .staging
        .delete(&absent)
        .await
        .expect("the absent candidate's object is removed up front");
    let own = fixture.binding.table_ref.name.clone();
    for path in [&present, &absent, &bystander] {
        copy_terminal_row(fixture, path, &own).await;
    }
    copy_terminal_row(fixture, &present, "other_table").await;
    let rows_before = file_list_paths(fixture).await;

    table.store.fail_next_deletes(1);
    assert!(
        worker
            .execute_one_for_test(&CancellationToken::new())
            .await
            .expect("an uncertain delete settles as a retained candidate"),
        "the fair-claimed cleanup attempt ran"
    );
    let (state, frontier, prepared) = cursor(fixture, cleanup_id).await;
    assert_eq!(
        (state.as_str(), prepared),
        ("prepared", Some(frontier)),
        "a failed delete retains its prepared candidate at the frontier"
    );
    assert_eq!(
        file_list_paths(fixture).await,
        rows_before,
        "a failed delete removes no file_list row"
    );

    // The lapsed claim is recovered by a takeover worker through the
    // production prepared-claim route, which replays the retained candidate.
    expire_claim(fixture.operator_pool.pool(), cleanup_id).await;
    let taker = ForgeWorker::new(
        table.supervised.forge(),
        ForgeWorkerConfig::default(),
        Uuid::now_v7(),
    )
    .expect("takeover worker");
    taker
        .execute_one_for_test(&CancellationToken::new())
        .await
        .expect("the takeover drains every candidate");
    assert_eq!(cursor(fixture, cleanup_id).await.0, "succeeded");
    let expected: Vec<(String, String)> = rows_before
        .into_iter()
        .filter(|(table_name, path)| !(table_name == &own && (path == &present || path == &absent)))
        .collect();
    assert_eq!(
        file_list_paths(fixture).await,
        expected,
        "exactly the cleaned candidates' terminal rows are removed"
    );
}
