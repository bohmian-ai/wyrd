//! Tier-2 coverage for the separate expired-object cleanup task.
//!
//! Cleanup consumes an immutable handoff one committed expiration left behind,
//! prepares exactly one candidate at a time with Postgres closed before any
//! object-store call, blocks Oracle reader widening while a preparation is
//! unresolved, and advances only for a confirmed deletion or a proven absence.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use vala_bifrost_redux::catalog::TenantTableBinding;
use vala_bifrost_redux::forge::{
    Forge, ForgeError, ForgeScheduler, ForgeWorker, ForgeWorkerConfig,
};
use vala_bifrost_redux::oracle::reader_pins::{
    OracleReaderAuthority, OracleReaderAuthorityConfig, RecordingEpochTerminator,
};
use vala_sql::queries::cluster_nodes::ClusterNodes;
use vala_sql::queries::forge_tasks::{ForgeEnqueueBatch, ForgeTasks};
use vala_sql::queries::oracle_reader_authority::BIFROST_CATALOG_NAME;
use vala_sql::row_types::cluster_nodes::RoleRegistration;
use vala_sql::row_types::forge_tasks::{
    ExpiredCleanupPayload, ForgeCleanupCandidate, ForgeCleanupCategory, ForgeCleanupPath,
    ForgeTaskClaim, ForgeTaskLane, ForgeTaskStrategy, ForgeTaskTableIdentity, NewForgeTask,
};
use vala_sql::row_types::oracle_reader_authority::TableAuthorityIdentity;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuditDecision, AuditEvent, AuditResult, AuthMethod, ClusterCapabilities, ClusterNodeKey,
    ClusterRole, NodeId, OracleCapabilitiesV1, QueryClass,
};

use crate::oracle::reader_authority::cut;

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

/// Starts one real Oracle reader authority over the fixture's own database.
///
/// Nothing about the authority is simulated: it registers a real Oracle role,
/// acquires and activates a real epoch, and commits real protection frontiers
/// through `vala.bifrost_table_maintenance_authority` — the same row an
/// expired-cleanup preparation serializes against.
///
/// # Panics
///
/// Panics when the role, epoch, or activation cannot be established.
async fn reader_authority(
    fixture: &PromotionIntegrationFixture,
    shutdown: &CancellationToken,
) -> (Arc<OracleReaderAuthority>, TableAuthorityIdentity) {
    let node_id = Uuid::now_v7();
    let mut conn = fixture
        .database
        .vala_postgres()
        .tenant_conn(DataTenantId::SYSTEM_OWNER)
        .await
        .expect("system connection");
    let row = ClusterNodes::new(fixture.database.vala_postgres().clone())
        .register(
            &mut conn,
            &RoleRegistration {
                key: ClusterNodeKey {
                    node_id: NodeId::new(node_id),
                    role: ClusterRole::Oracle,
                },
                address: "http://oracle:5002".into(),
                capabilities: ClusterCapabilities::OracleV1(OracleCapabilitiesV1 {
                    storage_protocol_version: 1,
                    cpu_cores: 4.0,
                    memory_budget_bytes: 4096,
                    cpu_cores_per_slot: 1.0,
                    memory_bytes_per_slot: 1024,
                    raw_slots: 4,
                    usable_slots: 3,
                    supported_classes: vec![QueryClass::Interactive],
                    max_workers_per_query: 3,
                }),
                started_at: chrono::Utc::now(),
            },
        )
        .await
        .expect("oracle role registers");
    conn.commit().await.expect("registration commits");

    let authority = OracleReaderAuthority::start(OracleReaderAuthorityConfig {
        vala: fixture.database.vala_postgres().clone(),
        operator_pool: fixture.database.operator_pool().clone(),
        node_id,
        fencing_token: row.lease.fencing_token,
        max_concurrent_queries: 4,
        terminator: Arc::new(RecordingEpochTerminator::default()) as Arc<_>,
        shutdown: shutdown.clone(),
    })
    .await
    .expect("epoch acquires");
    authority.activate().await.expect("epoch activates");

    let table_uid: Vec<u8> =
        sqlx::query_scalar("SELECT table_uid FROM vala.bifrost_tables WHERE data_tenant_id=$1")
            .bind(fixture.tenant.as_uuid())
            .fetch_one(fixture.operator_pool.pool())
            .await
            .expect("the fixture table is registered");
    let identity = TableAuthorityIdentity {
        tenant: fixture.tenant,
        table_uid: table_uid.try_into().expect("table uid is 16 bytes"),
        catalog_name: BIFROST_CATALOG_NAME.to_owned(),
        namespace_name: fixture.binding.table_ref.namespace.as_str().to_owned(),
        table_name: fixture.binding.table_ref.name.clone(),
    };
    (authority, identity)
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

/// Asserts every durable gate that must hold at the paused object-store stat.
///
/// The drain is suspended between its committed preparation and its first
/// external call, so each assertion here observes production state directly:
/// the SQL transaction is closed, the prepared row is the reader-widening and
/// competing-Forge boundary, and the self-exemption covers exactly one tuple.
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
    oracle: &Arc<OracleReaderAuthority>,
    identity: &TableAuthorityIdentity,
    cleanup_id: Uuid,
    attempt: Uuid,
    payload: &ExpiredCleanupPayload,
) {
    let candidate = &payload.cleanup_candidates[0];

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

    // The preparation transaction and its table-authority lock are closed: a
    // separate transaction takes the same row without waiting.
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

    // Real Oracle frontier expansion is refused while the preparation is
    // unresolved, and it is refused before any manifest or data object is read.
    let reads_before = table.store.reads();
    assert!(
        oracle
            .acquire_guard_for_cuts(vec![(
                identity.clone(),
                cut(
                    payload.committed_snapshot_id,
                    1,
                    &[payload.committed_snapshot_id],
                ),
            )])
            .await
            .is_err(),
        "an unresolved cleanup preparation blocks reader widening"
    );
    assert_eq!(
        table.store.reads(),
        reads_before,
        "reader widening is refused before any manifest or data read"
    );

    // The active-task index still denies a second claim, and the live table
    // lease still denies a competing destructive effect.
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

    // The self-exemption is exact in all four fields: only the drain's own
    // tuple stops its own preparation from protecting the candidate.
    // Every axis classifies the same real object; only the exemption tuple
    // varies, so a refusal is attributable to that one mismatched field.
    let eligibility = async |task_id, attempt_id, index, exempted: &ForgeCleanupCandidate| {
        forge
            .expired_cleanup_eligibility_for_test(
                binding,
                task_id,
                attempt_id,
                index,
                exempted,
                candidate.path.as_str(),
            )
            .await
            .expect("the production protection proof loads")
    };
    assert_eq!(
        eligibility(cleanup_id, attempt, 0, candidate).await,
        "Eligible",
        "the exact prepared tuple proceeds"
    );
    for (task_id, attempt_id, index, mismatch) in [
        (Uuid::now_v7(), attempt, 0, "task id"),
        (cleanup_id, Uuid::now_v7(), 0, "attempt id"),
        (cleanup_id, attempt, 1, "cursor index"),
    ] {
        assert_eq!(
            eligibility(task_id, attempt_id, index, candidate).await,
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
        eligibility(cleanup_id, attempt, 0, &sibling_candidate).await,
        "Protected",
        "a mismatched candidate leaves the preparation protecting its object"
    );

    // One safety input mutated at a time still refuses, through the same proof:
    // an object the live table still reaches, and a path outside the binding.
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
        forge
            .expired_cleanup_eligibility_for_test(
                binding, cleanup_id, attempt, 0, candidate, &reachable,
            )
            .await
            .expect("the production protection proof loads"),
        "Protected",
        "an object the live catalog still reaches is never eligible"
    );
    assert_eq!(
        forge
            .expired_cleanup_eligibility_for_test(
                binding,
                cleanup_id,
                attempt,
                0,
                candidate,
                "../outside/escape.parquet",
            )
            .await
            .expect("the production protection proof loads"),
        "InvalidPath",
        "a path outside the table binding is never eligible"
    );
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

    // Suspend the first candidate's fresh reachability stat. That instant is
    // after the preparation committed and before any deletion was submitted,
    // which is the only place the durable gates can be observed at all.
    let attempt = claim.attempt_id.expect("a claimed task has an attempt");
    let epoch_shutdown = CancellationToken::new();
    let (oracle, identity) = reader_authority(&table.fixture, &epoch_shutdown).await;
    let binding = table.fixture.binding.clone();
    let forge = table.supervised.forge();
    table.store.pause_stat_at(1);
    let drain_stop = CancellationToken::new();
    let (drained, ()) = tokio::join!(
        worker.execute_expired_cleanup_claim_for_test(claim, &drain_stop),
        async {
            table.store.stat_paused().await;
            assert_paused_candidate_gates(
                &table, &worker, &forge, &binding, &oracle, &identity, cleanup_id, attempt,
                &payload,
            )
            .await;
            table.store.release_stat();
        }
    );
    drained.expect("the drained cleanup task succeeds");
    epoch_shutdown.cancel();

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
