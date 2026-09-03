//! Tier-2 coverage for the Postgres boundaries that bracket the one pinned
//! Iceberg snapshot-expiration call.
//!
//! Preparation closes and releases its lock before the catalog gate, a definite
//! rejection releases the claims without expiring anything, an unproven outcome
//! retains every claim under the preparing worker's immutable evidence, and a
//! takeover settles the same operation under its own live fence while storing
//! the exact cleanup candidates and deleting nothing.

use std::sync::Arc;

use chrono::Duration as ChronoDuration;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use vala_bifrost_redux::forge::{ForgeWorker, ForgeWorkerConfig};
use wyrd_spec::DataTenantId;

use super::support::{
    CountingObjectStore, PromotionCatalogSeam, PromotionIntegrationFixture, SupervisedPromotion,
    manual_clock,
};

/// Durable snapshot of everything one expiration boundary must have written.
#[derive(Debug, PartialEq, Eq)]
struct ExpiryState {
    /// Current `vala.forge_tasks.state` of the seeded task.
    task_state: String,
    /// Number of unresolved claim rows the task still owns.
    claims: i64,
    /// Current operation phase, if the projection row exists.
    operation_phase: Option<String>,
    /// Every snapshot-expiry and task audit operation, in sequence order.
    audits: Vec<String>,
    /// Current planning-demand generation for the table.
    demand_generation: Option<i64>,
}

/// Reads the complete durable expiration state for one task and table.
///
/// # Panics
///
/// Panics when any read-only diagnostic query fails.
async fn expiry_state(fixture: &PromotionIntegrationFixture, task_id: Uuid) -> ExpiryState {
    let pool = fixture.operator_pool.pool();
    let task_state: String =
        sqlx::query_scalar("SELECT state FROM vala.forge_tasks WHERE task_id = $1")
            .bind(task_id)
            .fetch_one(pool)
            .await
            .expect("seeded task remains readable");
    let claims: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.forge_snapshot_expiration_claims WHERE task_id = $1",
    )
    .bind(task_id)
    .fetch_one(pool)
    .await
    .expect("claim count");
    let operation_phase: Option<String> = sqlx::query_scalar(
        "SELECT phase FROM vala.forge_operation_state \
         WHERE data_tenant_id = $1 AND family = 'snapshot_expire' ORDER BY prepared_at DESC LIMIT 1",
    )
    .bind(fixture.tenant.as_uuid())
    .fetch_optional(pool)
    .await
    .expect("operation phase");
    let mut conn = fixture
        .vala
        .tenant_conn(fixture.tenant)
        .await
        .expect("fixture tenant connection");
    let audits: Vec<String> = sqlx::query_scalar(
        "SELECT operation FROM vala.audit_outbox \
         WHERE operation LIKE 'forge.snapshot_expire.%' OR resource = $1 \
         ORDER BY seq",
    )
    .bind(format!("forge-task:{task_id}"))
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("audit operations");
    conn.commit().await.expect("fixture audit read commit");
    let demand_generation: Option<i64> = sqlx::query_scalar(
        "SELECT generation FROM vala.forge_planning_demands \
         WHERE data_tenant_id = $1 AND namespace_name = $2 AND table_name = $3",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(fixture.binding.table_ref.namespace.as_str())
    .bind(&fixture.binding.table_ref.name)
    .fetch_optional(pool)
    .await
    .expect("planning demand");
    ExpiryState {
        task_state,
        claims,
        operation_phase,
        audits,
        demand_generation,
    }
}

/// Seeds one running `snapshot_expiry` task bound to the fixture table.
///
/// # Panics
///
/// Panics when the seeding statement fails.
async fn seed_running_task(
    fixture: &PromotionIntegrationFixture,
    tenant: DataTenantId,
    attempt_id: Uuid,
    worker_id: Uuid,
    watermark: (i64, i64),
    plan_hash_byte: &str,
) -> Uuid {
    let task_id = Uuid::now_v7();
    let (base_snapshot_id, watermark_timestamp_ms) = watermark;
    sqlx::query(
        "INSERT INTO vala.forge_tasks (task_id,data_tenant_id,catalog_name,namespace_name,table_name,strategy,lane,base_snapshot_id,plan,plan_hash,estimated_files,estimated_bytes,estimated_parallelism,estimated_memory_bytes,estimated_spill_bytes,large_task_ceiling_bytes,state,attempt_id,claimed_by,claim_expires_at,watermark_snapshot_id,watermark_timestamp_ms,ready_at) \
         VALUES ($1,$2,'wyrd-redux',$3,$4,'snapshot_expiry','ordinary',$7,'{}'::jsonb,decode(repeat($9,32),'hex'),1,1,1,1,1,1,'running',$5,$6,now()+interval '10 minutes',$7,$8,now())",
    )
    .bind(task_id)
    .bind(tenant.as_uuid())
    .bind(fixture.binding.table_ref.namespace.as_str())
    .bind(&fixture.binding.table_ref.name)
    .bind(attempt_id)
    .bind(worker_id)
    .bind(base_snapshot_id)
    .bind(watermark_timestamp_ms)
    .bind(plan_hash_byte)
    .execute(fixture.operator_pool.pool())
    .await
    .expect("running snapshot-expiry task seeds");
    task_id
}

/// Proves a definite catalog rejection releases the whole preparation.
///
/// # Panics
///
/// Panics when the reset does not cancel the task, drop every claim, emit both
/// audits, and leave the object store untouched.
async fn reject_releases_every_claim(
    fixture: &PromotionIntegrationFixture,
    seam: &PromotionCatalogSeam,
    store: &CountingObjectStore,
    forge: &Arc<vala_bifrost_redux::forge::Forge>,
    watermark: (i64, i64),
) {
    // --- a definite rejection releases the claims, expiring nothing ------
    let rejected_attempt = Uuid::now_v7();
    let rejected_worker = Uuid::now_v7();
    let rejected_task = seed_running_task(
        fixture,
        fixture.tenant,
        rejected_attempt,
        rejected_worker,
        watermark,
        "00",
    )
    .await;
    seam.reject_next_commits(64);
    let outcome = forge
        .run_snapshot_expiry_for_test(
            &fixture.binding,
            rejected_task,
            rejected_attempt,
            rejected_worker,
        )
        .await
        .expect("a definite rejection is a released expiration, not a failed pass");
    assert!(
        outcome.is_none(),
        "a rejected expiration settles nothing: {outcome:?}"
    );
    let released = expiry_state(fixture, rejected_task).await;
    assert_eq!(released.task_state, "cancelled");
    assert_eq!(released.claims, 0);
    assert_eq!(released.operation_phase.as_deref(), Some("reset"));
    assert!(
        released
            .audits
            .contains(&"forge.snapshot_expire.prepared".to_owned())
            && released
                .audits
                .contains(&"forge.snapshot_expire.reset".to_owned())
            && released.audits.contains(&"forge.task.cancelled".to_owned()),
        "reset emits the prepared, reset, and task-cancelled audits: {:?}",
        released.audits
    );
    assert_eq!(store.deletes(), 0, "expiration never deletes an object");
}

/// Prepares one expiration and abandons the worker at the catalog gate.
///
/// Returns the task, its attempt, and the durable state a takeover must find
/// unchanged.
///
/// # Panics
///
/// Panics when preparation is not fully durable before the catalog gate, or
/// when the abandoned pass changed any durable expiration state.
async fn prepare_and_abandon_at_the_catalog_gate(
    fixture: &PromotionIntegrationFixture,
    seam: &Arc<PromotionCatalogSeam>,
    forge: &Arc<vala_bifrost_redux::forge::Forge>,
    watermark: (i64, i64),
) -> (Uuid, Uuid, ExpiryState) {
    // --- an unproven outcome retains every claim -------------------------
    seam.reject_next_commits(0);
    let attempt = Uuid::now_v7();
    let preparing_worker = Uuid::now_v7();
    let task = seed_running_task(
        fixture,
        fixture.tenant,
        attempt,
        preparing_worker,
        watermark,
        "11",
    )
    .await;
    seam.park_next_commit();
    let parked = {
        let forge = Arc::clone(forge);
        let binding = fixture.binding.clone();
        tokio::spawn(async move {
            forge
                .run_snapshot_expiry_for_test(&binding, task, attempt, preparing_worker)
                .await
        })
    };
    if tokio::time::timeout(
        std::time::Duration::from_secs(30),
        seam.wait_for_parked_commit(),
    )
    .await
    .is_err()
    {
        let early = tokio::time::timeout(std::time::Duration::from_secs(5), parked).await;
        panic!("the expiration never reached the catalog gate: {early:?}");
    }
    // Postgres closed before the catalog gate: the preparation is fully durable
    // and holds no lock while the commit is parked.
    let prepared = expiry_state(fixture, task).await;
    assert_eq!(prepared.task_state, "prepared");
    assert!(prepared.claims > 0, "preparation claims every selection");
    assert_eq!(prepared.operation_phase.as_deref(), Some("prepared"));
    let preparing_evidence: (Uuid, Uuid, Uuid) = sqlx::query_as(
        "SELECT task_id, attempt_id, worker_id FROM vala.forge_snapshot_expiration_claims \
         WHERE task_id = $1 LIMIT 1",
    )
    .bind(task)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("immutable preparation evidence");
    assert_eq!(preparing_evidence, (task, attempt, preparing_worker));

    // The preparing worker dies at the catalog gate, so its outcome is
    // genuinely unproven: nothing is released and every claim survives.
    parked.abort();
    seam.wait_for_parked_commit_drop().await;
    let unproven = expiry_state(fixture, task).await;
    assert_eq!(
        unproven, prepared,
        "an unproven pass changes no durable expiration state"
    );
    (task, attempt, unproven)
}

/// Proves the three Postgres boundaries bracket the one pinned Iceberg call.
///
/// # Panics
///
/// Panics on any durable-state, candidate, delete-count, or ordering mismatch.
#[tokio::test]
async fn prepared_claim_releases_sql_before_iceberg_and_hands_exact_names_to_cleanup() {
    let ExpirableTable {
        fixture,
        store,
        seam,
        supervised,
        watermark,
    } = expirable_table("expiry_bracket", false).await;
    let forge = supervised.forge();

    reject_releases_every_claim(&fixture, &seam, &store, &forge, watermark).await;

    let (task, attempt, prepared) =
        prepare_and_abandon_at_the_catalog_gate(&fixture, &seam, &forge, watermark).await;

    // --- a takeover settles with exact candidates and zero deletes -------
    // The dead worker's lease ages out and the scheduler hands the same task
    // and attempt to a new owner, exactly as a real crash recovery does.
    let settle_task = task;
    let settling_worker = Uuid::now_v7();
    sqlx::query("UPDATE vala.maintenance_leases SET expires_at = now() - interval '1 hour'")
        .execute(fixture.operator_pool.pool())
        .await
        .expect("the dead preparing worker's lease ages out");
    sqlx::query("UPDATE vala.forge_tasks SET claimed_by = $2 WHERE task_id = $1")
        .bind(settle_task)
        .bind(settling_worker)
        .execute(fixture.operator_pool.pool())
        .await
        .expect("a new owner takes the prepared task over");
    let before_demand = prepared.demand_generation;
    let deletes_before = store.deletes();
    let evidence = forge
        .run_snapshot_expiry_for_test(&fixture.binding, settle_task, attempt, settling_worker)
        .await
        .expect("a corroborated selection commits")
        .expect("a committed expiration settles its own task");
    assert!(
        !evidence.cleanup_candidates.is_empty(),
        "settlement stores the exact cleanup candidates"
    );
    assert_eq!(evidence.deleted_candidate_count, 0);
    let settled = expiry_state(&fixture, settle_task).await;
    assert_eq!(settled.task_state, "succeeded");
    assert_eq!(settled.claims, 0);
    assert_eq!(settled.operation_phase.as_deref(), Some("committed"));
    assert!(
        settled
            .audits
            .contains(&"forge.snapshot_expire.committed".to_owned())
            && settled.audits.contains(&"forge.task.succeeded".to_owned()),
        "settlement emits both terminal audits: {:?}",
        settled.audits
    );
    assert!(
        settled.demand_generation > before_demand,
        "settlement creates cleanup demand: {before_demand:?} -> {:?}",
        settled.demand_generation
    );
    assert_eq!(
        store.deletes(),
        deletes_before,
        "snapshot expiration never invokes physical cleanup"
    );

    supervised.shutdown().await;
}

/// Everything one snapshot-expiration scenario needs over a live fixture.
///
/// The supervisor is retained by value because dropping it would take the
/// scheduler, worker, and shared `Forge` graph down with it.
struct ExpirableTable {
    /// Live Scribe/Forge fixture over one repository-managed database.
    fixture: PromotionIntegrationFixture,
    /// Delete- and read-counting object store every effect goes through.
    store: Arc<CountingObjectStore>,
    /// Catalog seam that can refuse, park, or lose one commit.
    seam: Arc<PromotionCatalogSeam>,
    /// Retained production scheduler and worker supervisor.
    supervised: SupervisedPromotion,
    /// Current head snapshot and its timestamp, used as the task watermark.
    watermark: (i64, i64),
}

/// Promotes twice and ages the clock so the older snapshot is expirable.
///
/// `worker_routed` enables the fixture's `snapshot_expiry_enabled` config so a
/// maintenance dispatch reaches the expiry owner. It is a fixture capability
/// flag, not production phase activation, which TASK-055 still owns.
///
/// # Panics
///
/// Panics when the fixture cannot promote twice or the table has no head.
async fn expirable_table(name: &str, worker_routed: bool) -> ExpirableTable {
    let mut fixture = PromotionIntegrationFixture::start(name).await;
    fixture.config.snapshot_expiry_enabled = worker_routed;
    let store = CountingObjectStore::new(Arc::clone(&fixture.staging));
    let seam = PromotionCatalogSeam::new(fixture.catalog.iceberg_catalog(), store.read_counter());
    let (clock, control) = manual_clock();
    let mut supervised = SupervisedPromotion::start(
        &fixture,
        Arc::clone(&seam) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&store) as Arc<dyn vala_bifrost_redux::forge::ForgeObjectStore>,
        clock,
    );
    // Two real promotion commits leave two snapshots, so the retained head
    // still leaves an expirable ancestor under `retain_last = 1`.
    supervised.run_one_success().await;
    fixture.seal_more(2).await;
    supervised.restart_worker();
    supervised.run_one_success().await;
    // Every existing snapshot is now older than the retention cutoff. Each
    // later advance moves the cutoff, so every pass derives its own
    // deterministic operation identity instead of replaying the previous one.
    control
        .advance(ChronoDuration::hours(48))
        .expect("manual clock advance");
    // Every seeded task protects the live head, which is the watermark a real
    // claimed attempt would carry.
    let watermark = fixture
        .catalog
        .iceberg_catalog()
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("fixture table loads")
        .metadata()
        .current_snapshot()
        .map(|snapshot| (snapshot.snapshot_id(), snapshot.timestamp_ms()))
        .expect("two promotions left a current snapshot");
    ExpirableTable {
        fixture,
        store,
        seam,
        supervised,
        watermark,
    }
}

/// Counts the snapshots the real catalog still retains for the fixture table.
///
/// Expiration removes ancestry rather than moving the head, so the retained
/// snapshot count — not the current snapshot id — is what proves the catalog
/// accepted the mutation.
///
/// # Panics
///
/// Panics when the fixture table cannot be loaded.
async fn retained_snapshots(fixture: &PromotionIntegrationFixture) -> usize {
    fixture
        .catalog
        .iceberg_catalog()
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("fixture table loads")
        .metadata()
        .snapshots()
        .count()
}

/// Proves an accepted commit whose response was lost keeps prepared authority.
///
/// # Panics
///
/// Panics when the lost response resets the preparation, when Iceberg did not
/// actually advance, or when takeover does not settle from the advanced
/// metadata without a second catalog mutation.
#[tokio::test]
async fn retryable_catalog_failure_retains_prepared_authority() {
    let ExpirableTable {
        fixture,
        store,
        seam,
        supervised,
        watermark,
    } = expirable_table("expiry_lost_response", false).await;
    let forge = supervised.forge();

    let attempt = Uuid::now_v7();
    let preparing_worker = Uuid::now_v7();
    let task = seed_running_task(
        &fixture,
        fixture.tenant,
        attempt,
        preparing_worker,
        watermark,
        "22",
    )
    .await;
    let snapshots_before = retained_snapshots(&fixture).await;
    let deletes_before = store.deletes();
    seam.lose_commit_responses(true);
    let uncertain = forge
        .run_snapshot_expiry_for_test(&fixture.binding, task, attempt, preparing_worker)
        .await;
    seam.lose_commit_responses(false);
    let error = uncertain.expect_err("a lost response is uncertainty, not a released expiration");
    assert!(
        matches!(error, vala_bifrost_redux::forge::ForgeError::Catalog(_)),
        "the retryable catalog error surfaces unwrapped: {error:?}"
    );
    assert!(
        retained_snapshots(&fixture).await < snapshots_before,
        "the catalog accepted the expiration before the response was lost"
    );
    let retained = expiry_state(&fixture, task).await;
    assert_eq!(retained.task_state, "prepared");
    assert!(retained.claims > 0, "every claim survives uncertainty");
    assert_eq!(retained.operation_phase.as_deref(), Some("prepared"));
    assert!(
        !retained
            .audits
            .contains(&"forge.snapshot_expire.reset".to_owned())
            && !retained.audits.contains(&"forge.task.cancelled".to_owned()),
        "an uncertain outcome releases nothing: {:?}",
        retained.audits
    );
    let preparing_evidence: (Uuid, Uuid, Uuid) = sqlx::query_as(
        "SELECT task_id, attempt_id, worker_id FROM vala.forge_snapshot_expiration_claims \
         WHERE task_id = $1 LIMIT 1",
    )
    .bind(task)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("immutable preparation evidence");
    assert_eq!(preparing_evidence, (task, attempt, preparing_worker));
    assert_eq!(
        store.deletes(),
        deletes_before,
        "expiration deletes nothing"
    );

    let attempts_before = seam.attempts();
    let settling_worker = Uuid::now_v7();
    sqlx::query("UPDATE vala.maintenance_leases SET expires_at = now() - interval '1 hour'")
        .execute(fixture.operator_pool.pool())
        .await
        .expect("the dead preparing worker's lease ages out");
    sqlx::query("UPDATE vala.forge_tasks SET claimed_by = $2 WHERE task_id = $1")
        .bind(task)
        .bind(settling_worker)
        .execute(fixture.operator_pool.pool())
        .await
        .expect("a new owner takes the prepared task over");
    let evidence = forge
        .run_snapshot_expiry_for_test(&fixture.binding, task, attempt, settling_worker)
        .await
        .expect("takeover recognizes the advanced metadata")
        .expect("recovered settlement settles the prepared task");
    assert!(
        !evidence.cleanup_candidates.is_empty(),
        "recovered settlement stores the exact cleanup candidates"
    );
    assert_eq!(
        seam.attempts(),
        attempts_before,
        "takeover settles from the committed metadata without a second mutation"
    );
    let settled = expiry_state(&fixture, task).await;
    assert_eq!(settled.task_state, "succeeded");
    assert_eq!(settled.claims, 0);
    assert_eq!(settled.operation_phase.as_deref(), Some("recovered"));
    assert!(
        settled
            .audits
            .contains(&"forge.snapshot_expire.recovered".to_owned())
            && settled.audits.contains(&"forge.task.succeeded".to_owned()),
        "recovery emits both terminal audits: {:?}",
        settled.audits
    );
    assert!(
        settled.demand_generation > retained.demand_generation,
        "recovered settlement creates cleanup demand"
    );
    assert_eq!(
        store.deletes(),
        deletes_before,
        "expiration deletes nothing"
    );

    supervised.shutdown().await;
}

/// Seeds one ready `snapshot_expiry` task with a valid executable envelope.
///
/// A ready row is what the production claim transaction consumes, so the test
/// drives the same admission path a supervised worker would.
///
/// # Panics
///
/// Panics when the seeding statement fails.
async fn seed_ready_expiry_task(
    fixture: &PromotionIntegrationFixture,
    watermark: (i64, i64),
    plan_hash_byte: &str,
) -> Uuid {
    let task_id = Uuid::now_v7();
    let (base_snapshot_id, _) = watermark;
    sqlx::query(
        "INSERT INTO vala.forge_tasks (task_id,data_tenant_id,catalog_name,namespace_name,table_name,strategy,lane,base_snapshot_id,plan,plan_hash,estimated_files,estimated_bytes,estimated_parallelism,estimated_memory_bytes,estimated_spill_bytes,large_task_ceiling_bytes,state,ready_at,envelope_version,decoded_batch_bytes,decoded_input_bytes,sort_working_bytes,sort_merge_reservation_bytes,encoder_buffer_bytes,upload_chunk_bytes,footer_encoded_bytes,footer_decode_workspace_bytes,sort_spill_bytes) \
         VALUES ($1,$2,'wyrd-redux',$3,$4,'snapshot_expiry','ordinary',$5,'{\"version\":1,\"inputs\":[],\"parameters\":{\"kind\":\"maintenance\",\"trigger_commit_count\":1}}'::jsonb,decode(repeat($6,32),'hex'),1,1,1,41943040,1024,1,'ready',now(),2,1024,1024,3072,1024,1024,1024,8388608,33554432,1024)",
    )
    .bind(task_id)
    .bind(fixture.tenant.as_uuid())
    .bind(fixture.binding.table_ref.namespace.as_str())
    .bind(&fixture.binding.table_ref.name)
    .bind(base_snapshot_id)
    .bind(plan_hash_byte)
    .execute(fixture.operator_pool.pool())
    .await
    .expect("ready snapshot-expiry task seeds");
    task_id
}

/// Counts how many times one audit operation appears in a settled sequence.
fn audit_count(audits: &[String], operation: &str) -> usize {
    audits.iter().filter(|entry| *entry == operation).count()
}

/// Proves a worker accepts atomic expiration settlement without transitioning.
///
/// # Panics
///
/// Panics when the claim is not produced, execution does not return success,
/// or any terminal owner is written twice.
#[tokio::test]
async fn worker_settled_expiration_returns_success_without_a_second_transition() {
    let ExpirableTable {
        fixture,
        store,
        seam: _seam,
        supervised,
        watermark,
    } = expirable_table("expiry_worker_settles", true).await;
    let worker = ForgeWorker::new(
        supervised.forge(),
        ForgeWorkerConfig::default(),
        Uuid::now_v7(),
    )
    .expect("fixture Forge worker");
    let task = seed_ready_expiry_task(&fixture, watermark, "44").await;
    let claim = worker
        .claim_for_test()
        .await
        .expect("the production claim transaction runs")
        .expect("the ready snapshot-expiry task is claimable");
    assert_eq!(claim.task_id, task, "the seeded task is the claimed task");
    let before = expiry_state(&fixture, task).await;
    let deletes_before = store.deletes();

    worker
        .execute_snapshot_expiry_claim_for_test(claim, &CancellationToken::new())
        .await
        .expect("a settled expiration is a successful worker attempt");

    let settled = expiry_state(&fixture, task).await;
    assert_eq!(settled.task_state, "succeeded");
    assert_eq!(settled.claims, 0);
    assert_eq!(settled.operation_phase.as_deref(), Some("committed"));
    assert_eq!(
        audit_count(&settled.audits, "forge.task.succeeded"),
        1,
        "the task has exactly one terminal audit: {:?}",
        settled.audits
    );
    assert_eq!(
        audit_count(&settled.audits, "forge.snapshot_expire.committed"),
        1,
        "the operation has exactly one terminal audit: {:?}",
        settled.audits
    );
    assert_eq!(
        settled.demand_generation,
        Some(before.demand_generation.unwrap_or(0) + 1),
        "settlement advances planning demand exactly once"
    );
    let owner: Option<Uuid> =
        sqlx::query_scalar("SELECT claimed_by FROM vala.forge_tasks WHERE task_id = $1")
            .bind(task)
            .fetch_one(fixture.operator_pool.pool())
            .await
            .expect("settled task remains readable");
    assert_eq!(owner, None, "settlement cleared task ownership");
    assert_eq!(
        store.deletes(),
        deletes_before,
        "snapshot expiration never invokes physical cleanup"
    );

    supervised.shutdown().await;
}
