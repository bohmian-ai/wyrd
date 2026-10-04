//! Tier-2 coverage for the Postgres boundaries that bracket the one pinned
//! Iceberg snapshot-expiration call.
//!
//! Preparation closes and releases its lock before the catalog gate, a definite
//! rejection releases the claims without expiring anything, an unproven outcome
//! retains every claim under the preparing worker's immutable evidence, and a
//! takeover settles the same operation under its own live fence while storing
//! the exact cleanup candidates and deleting nothing.

use std::collections::BTreeSet;
use std::sync::Arc;

use chrono::Duration as ChronoDuration;
use iceberg::spec::{ManifestContentType, Operation};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use vala_bifrost_redux::catalog::layout::FORGE_WRITER_RECIPE;
use vala_bifrost_redux::forge::{
    ForgeCompactionOutcome, ForgeError, ForgeHeldTerm, ForgeTableKey, ForgeTableSettings,
    ForgeWorker, ForgeWorkerConfig,
};
use vala_sql::row_types::forge_tasks::ForgeTaskTableIdentity;
use wyrd_spec::DataTenantId;

use super::expired_cleanup::expire_claim;
use super::support::{
    CountingObjectStore, ForgeTelemetryCheckpoint, PromotionCatalogSeam,
    PromotionIntegrationFixture, SupervisedPromotion, manual_clock, set_table_properties,
};
use vala_bifrost_redux::forge::ForgeClockControl;

/// Durable snapshot of everything one expiration boundary must have written.
#[derive(Debug, PartialEq, Eq)]
struct ExpiryState {
    /// Current `vala.forge_tasks.state` of the seeded task.
    task_state: String,
    /// Number of unresolved claim rows the task still owns.
    claims: i64,
    /// Current operation phase, if the projection row exists.
    operation_phase: Option<String>,
    /// Whether a committed expiration left a cleanup handoff for the
    /// leader's next maintenance pass to consume.
    cleanup_handoff: bool,
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
    let identity = vala_sql::row_types::forge_tasks::ForgeTaskTableIdentity::new(
        "wyrd-redux",
        fixture.binding.table_ref.namespace.as_str(),
        &fixture.binding.table_ref.name,
    )
    .expect("fixture table identity");
    let cleanup_handoff =
        vala_sql::queries::forge_tasks::ForgeTasks::new(fixture.operator_pool.clone())
            .unconsumed_expiration_handoff(fixture.tenant, &identity)
            .await
            .expect("cleanup handoff read")
            .is_some();
    ExpiryState {
        task_state,
        claims,
        operation_phase,
        cleanup_handoff,
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
        "INSERT INTO vala.forge_tasks (task_id,data_tenant_id,catalog_name,namespace_name,table_name,strategy,base_snapshot_id,plan,plan_hash,estimated_files,estimated_bytes,state,attempt_id,claimed_by,claim_expires_at,watermark_snapshot_id,watermark_timestamp_ms,ready_at) \
         VALUES ($1,$2,'wyrd-redux',$3,$4,'snapshot_expiry',$7,'{\"version\":1,\"inputs\":[],\"parameters\":{\"kind\":\"maintenance\",\"trigger_commit_count\":1}}'::jsonb,decode(repeat($9,32),'hex'),1,1,'running',$5,$6,now()+interval '10 minutes',$7,$8,now())",
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
/// Panics when the reset does not cancel the task, drop every claim, settle the
/// operation as reset, and leave the object store untouched.
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
        control: _control,
        watermark,
    } = expirable_table("expiry_bracket").await;
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
    assert!(
        !prepared.cleanup_handoff,
        "a prepared expiration hands nothing off"
    );
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
        settled.cleanup_handoff,
        "settlement hands its cleanup to the leader's next pass"
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
pub(super) struct ExpirableTable {
    /// Live Scribe/Forge fixture over one repository-managed database.
    pub(super) fixture: PromotionIntegrationFixture,
    /// Delete- and read-counting object store every effect goes through.
    pub(super) store: Arc<CountingObjectStore>,
    /// Catalog seam that can refuse, park, or lose one commit.
    pub(super) seam: Arc<PromotionCatalogSeam>,
    /// Retained production scheduler and worker supervisor.
    pub(super) supervised: SupervisedPromotion,
    /// Manual clock control shared by the whole Forge graph.
    pub(super) control: ForgeClockControl,
    /// Current head snapshot and its timestamp, used as the task watermark.
    pub(super) watermark: (i64, i64),
}

/// Reads the current head snapshot and its timestamp from the real catalog.
///
/// # Panics
///
/// Panics when the table cannot be loaded or carries no current snapshot.
pub(super) async fn head_watermark(fixture: &PromotionIntegrationFixture) -> (i64, i64) {
    fixture
        .catalog
        .iceberg_catalog()
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("fixture table loads")
        .metadata()
        .current_snapshot()
        .map(|snapshot| (snapshot.snapshot_id(), snapshot.timestamp_ms()))
        .expect("a promotion left a current snapshot")
}

/// Promotes twice so the older snapshot is replaced and expirable.
///
/// # Panics
///
/// Panics when the fixture cannot promote twice or the table has no head.
pub(super) async fn expirable_table(name: &str) -> ExpirableTable {
    let fixture = PromotionIntegrationFixture::start(name).await;
    let store = CountingObjectStore::new(Arc::clone(&fixture.staging));
    let seam = PromotionCatalogSeam::new(fixture.catalog.iceberg_catalog(), store.read_counter());
    let (clock, control) = manual_clock();
    let mut supervised = SupervisedPromotion::start(
        &fixture,
        Arc::clone(&seam) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&store) as Arc<dyn vala_bifrost_redux::forge::ForgeObjectStore>,
        clock,
    );
    // Two real promotion commits leave two snapshots, so the replaced first
    // one is expirable as soon as nothing reads the table.
    supervised.run_one_success().await;
    fixture.seal_more(2).await;
    supervised.restart_worker();
    supervised.run_one_success().await;
    // Snapshot expiry has no age rule; this advance only puts the Forge clock
    // past the orphan TTL, so a never-published object seeded afterwards is
    // old enough for orphan collection.
    control
        .advance(ChronoDuration::hours(48))
        .expect("manual clock advance");
    // Every seeded task protects the live head, which is the watermark a real
    // claimed attempt would carry.
    let watermark = head_watermark(&fixture).await;
    ExpirableTable {
        fixture,
        store,
        seam,
        supervised,
        control,
        watermark,
    }
}

/// Names one real manifest of the fixture table's current snapshot.
///
/// Maintenance planning takes the head snapshot's manifest entries as a task's
/// exact inputs, so a seeded task that carries one of them has the same payload
/// shape the production scheduler would have written.
///
/// # Panics
///
/// Panics when the table cannot be loaded, has no current snapshot, or its
/// manifest list is empty.
async fn head_manifest_path(fixture: &PromotionIntegrationFixture) -> String {
    let table = fixture
        .catalog
        .iceberg_catalog()
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("fixture table loads");
    let snapshot = table
        .metadata()
        .current_snapshot()
        .expect("a promotion left a current snapshot");
    let manifests = table
        .manifest_list_reader(snapshot)
        .load()
        .await
        .expect("the head manifest list loads");
    manifests
        .entries()
        .iter()
        .map(|manifest| manifest.manifest_path.clone())
        .min()
        .expect("the head snapshot has at least one manifest")
}

/// Asserts an unproven expiration retained the preparing worker's authority.
///
/// An uncertain outcome must leave the task `prepared`, every claim row in
/// place, the operation `prepared`, and the original attempt's immutable
/// evidence unchanged — which together are what let a successor reconcile
/// instead of re-expiring.
///
/// Returns the retained state so the caller can compare a later settlement
/// against it.
///
/// # Panics
///
/// Panics when any of those owners moved, or when the claim evidence is
/// missing or names another attempt.
async fn assert_uncertain_preparation_retained(
    fixture: &PromotionIntegrationFixture,
    task: Uuid,
    attempt: Uuid,
    preparing_worker: Uuid,
) -> ExpiryState {
    let retained = expiry_state(fixture, task).await;
    assert_eq!(retained.task_state, "prepared");
    assert!(retained.claims > 0, "every claim survives uncertainty");
    assert_eq!(retained.operation_phase.as_deref(), Some("prepared"));
    let preparing_evidence: (Uuid, Uuid, Uuid) = sqlx::query_as(
        "SELECT task_id, attempt_id, worker_id FROM vala.forge_snapshot_expiration_claims \
         WHERE task_id = $1 LIMIT 1",
    )
    .bind(task)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("immutable preparation evidence");
    assert_eq!(preparing_evidence, (task, attempt, preparing_worker));
    retained
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
        control: _control,
        watermark,
    } = expirable_table("expiry_lost_response").await;
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
    let retained =
        assert_uncertain_preparation_retained(&fixture, task, attempt, preparing_worker).await;
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
        !retained.cleanup_handoff && settled.cleanup_handoff,
        "recovered settlement hands its cleanup to the leader's next pass"
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
/// drives the same admission path a supervised worker would. The plan carries
/// one real head manifest as its exact input, because the production worker
/// refuses a maintenance payload with no inputs before it reaches any other
/// gate — a seeded empty plan would prove settlement against a task shape
/// production never executes.
///
/// # Panics
///
/// Panics when the fixture has no head manifest or the seeding statement fails.
pub(super) async fn seed_ready_expiry_task(
    fixture: &PromotionIntegrationFixture,
    watermark: (i64, i64),
    plan_hash_byte: &str,
) -> Uuid {
    let task_id = Uuid::now_v7();
    let (base_snapshot_id, _) = watermark;
    let plan = serde_json::json!({
        "version": 1,
        "inputs": [head_manifest_path(fixture).await],
        "parameters": {
            "kind": "maintenance",
            "trigger_commit_count": 1,
            "snapshot_expiry_due": true,
            "reconciliation_due": false,
        },
    });
    sqlx::query(
        "INSERT INTO vala.forge_tasks (task_id,data_tenant_id,catalog_name,namespace_name,table_name,strategy,base_snapshot_id,plan,plan_hash,estimated_files,estimated_bytes,state,ready_at) \
         VALUES ($1,$2,'wyrd-redux',$3,$4,'snapshot_expiry',$5,$7,decode(repeat($6,32),'hex'),1,1,'ready',now())",
    )
    .bind(task_id)
    .bind(fixture.tenant.as_uuid())
    .bind(fixture.binding.table_ref.namespace.as_str())
    .bind(&fixture.binding.table_ref.name)
    .bind(base_snapshot_id)
    .bind(plan_hash_byte)
    .bind(plan)
    .execute(fixture.operator_pool.pool())
    .await
    .expect("ready snapshot-expiry task seeds");
    task_id
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
        control: _control,
        watermark,
    } = expirable_table("expiry_worker_settles").await;
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

    // The entrypoint bypasses phase activation and nothing else: the same
    // payload contract production applies before its lease must still refuse a
    // claim whose exact inputs were cleared, without touching durable state.
    let mut without_inputs = claim.clone();
    without_inputs.plan.inputs.clear();
    let refused = worker
        .execute_snapshot_expiry_claim_for_test(without_inputs, &CancellationToken::new())
        .await
        .expect_err("a maintenance claim with no exact inputs is refused");
    assert!(
        matches!(&refused, ForgeError::Invariant { detail } if detail.contains("no exact inputs")),
        "the refusal is the production payload-contract invariant: {refused}"
    );
    assert_eq!(
        expiry_state(&fixture, task).await,
        before,
        "a refused payload leaves the claimed task and its claims untouched"
    );

    worker
        .execute_snapshot_expiry_claim_for_test(claim, &CancellationToken::new())
        .await
        .expect("a settled expiration is a successful worker attempt");

    let settled = expiry_state(&fixture, task).await;
    assert_eq!(settled.task_state, "succeeded");
    assert_eq!(settled.claims, 0);
    assert_eq!(settled.operation_phase.as_deref(), Some("committed"));
    assert!(
        !before.cleanup_handoff && settled.cleanup_handoff,
        "settlement hands its cleanup to the leader's next pass"
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

    // Durable settlement outranks whatever finishes concurrently.
    for event in [
        ConcurrentSettlementEvent::Shutdown,
        ConcurrentSettlementEvent::ObsoleteHeartbeat,
    ] {
        settled_expiration_outranks(event).await;
    }
}

/// A concurrent event that must not replace a durable expiration settlement.
#[derive(Debug, Clone, Copy)]
enum ConcurrentSettlementEvent {
    /// The exact shutdown token the claim executes under is cancelled.
    Shutdown,
    /// The already-running claim heartbeat takes its post-settlement beat.
    ObsoleteHeartbeat,
}

impl ConcurrentSettlementEvent {
    /// Unique fixture name for this case's isolated expirable table.
    fn fixture_name(self) -> &'static str {
        match self {
            Self::Shutdown => "expiry_settled_beats_shutdown",
            Self::ObsoleteHeartbeat => "expiry_settled_beats_heartbeat",
        }
    }
}

/// Proves one concurrent event cannot displace a durable expiration settlement.
///
/// The worker is held at the production post-settlement barrier — after its
/// atomic task and operation transition committed, and before it reads shutdown
/// or joins the heartbeat — so the concurrent event provably precedes the
/// worker's own interpretation without any timing dependence.
///
/// # Panics
///
/// Panics when the attempt does not return success, when a terminal owner is
/// written twice, or when any durable expiration fact is not the settled one.
async fn settled_expiration_outranks(event: ConcurrentSettlementEvent) {
    let table = expirable_table(event.fixture_name()).await;
    let worker = ForgeWorker::new(
        table.supervised.forge(),
        ForgeWorkerConfig::default(),
        Uuid::now_v7(),
    )
    .expect("fixture Forge worker");
    let task = seed_ready_expiry_task(&table.fixture, table.watermark, "46").await;
    let claim = worker
        .claim_for_test()
        .await
        .expect("the production claim transaction runs")
        .expect("the ready snapshot-expiry task is claimable");
    let before = expiry_state(&table.fixture, task).await;

    let observer = table.supervised.observer().clone();
    observer.hold_after_next_durable_settlement_for_test();
    let shutdown = CancellationToken::new();
    let claim_shutdown = shutdown.clone();
    let attempt = tokio::spawn(async move {
        worker
            .execute_snapshot_expiry_claim_for_test(claim, &claim_shutdown)
            .await
    });
    observer.wait_for_held_durable_settlement_for_test().await;
    match event {
        ConcurrentSettlementEvent::Shutdown => shutdown.cancel(),
        ConcurrentSettlementEvent::ObsoleteHeartbeat => {
            observer.beat_claim_heartbeat_now_for_test();
        }
    }
    observer.release_held_durable_settlement_for_test();
    attempt
        .await
        .expect("the held attempt joins")
        .unwrap_or_else(|error| {
            panic!("a durably settled expiration stays successful under {event:?}: {error}")
        });

    let settled = expiry_state(&table.fixture, task).await;
    assert_eq!(settled.task_state, "succeeded", "{event:?}");
    assert_eq!(settled.claims, 0, "{event:?}");
    assert_eq!(
        settled.operation_phase.as_deref(),
        Some("committed"),
        "{event:?}"
    );
    assert!(
        !before.cleanup_handoff && settled.cleanup_handoff,
        "{event:?} hands its cleanup to the leader's next pass"
    );

    table.supervised.shutdown().await;
}

/// Writes one aged, otherwise eligible never-published Forge object.
///
/// The name is an immutable Forge attempt generation, which is the only path
/// shape orphan collection may reach by listing alone.
///
/// # Panics
///
/// Panics when the staging operator rejects the write.
pub(super) async fn seed_never_published_object(fixture: &PromotionIntegrationFixture) -> String {
    let path = format!(
        "{}/data/forge/{FORGE_WRITER_RECIPE}/{}-00001-{}.parquet",
        fixture.binding.object_prefix,
        Uuid::now_v7(),
        Uuid::now_v7()
    );
    fixture
        .staging
        .write(&path, b"never published".to_vec())
        .await
        .expect("the orphan object seeds");
    path
}

/// Reports whether the seeded orphan object still exists in staging.
///
/// # Panics
///
/// Panics when the staging operator fails for a reason other than absence.
pub(super) async fn object_exists(fixture: &PromotionIntegrationFixture, path: &str) -> bool {
    match fixture.staging.stat(path).await {
        Ok(_) => true,
        Err(error) if error.kind() == opendal::ErrorKind::NotFound => false,
        Err(error) => panic!("staging stat failed: {error}"),
    }
}

/// Asserts an expiration left one never-published object to the orphan owner.
///
/// Expiration must neither delete the object nor invoke physical cleanup at
/// all, and the independent orphan strategy must still reclaim exactly that
/// candidate afterwards — which is what proves the object was a genuine orphan
/// the expiration merely declined to touch.
///
/// # Panics
///
/// Panics when the expiration deleted anything, or when the orphan owner does
/// not delete exactly its one candidate.
async fn assert_orphan_survives_expiration(
    table: &ExpirableTable,
    orphan: &str,
    deletes_before: usize,
) {
    assert!(
        object_exists(&table.fixture, orphan).await,
        "expiration deleted a never-published object"
    );
    assert_eq!(
        table.store.deletes(),
        deletes_before,
        "expiration invoked physical cleanup"
    );
    let deleted = table
        .supervised
        .forge()
        .run_orphan_gc_for_test(&table.fixture.binding)
        .await
        .expect("the independent orphan strategy runs");
    assert_eq!(deleted, 1, "the orphan owner deletes exactly its candidate");
    assert!(
        !object_exists(&table.fixture, orphan).await,
        "the orphan owner did not delete the object it reported"
    );
}

/// Proves neither fresh nor recovered expiration deletes a never-published object.
///
/// Each half runs on its own table: one pass expires every eligible ancestor,
/// so proving fresh and recovered expiration on one table would mean promoting
/// on top of an already-expired ancestry, which is a different scenario.
///
/// # Panics
///
/// Panics when an expiration deletes anything, or when the retained orphan
/// owner cannot delete the same object afterwards.
#[tokio::test]
async fn worker_expiration_never_runs_orphan_cleanup() {
    let fresh = expirable_table("expiry_no_orphan_fresh").await;
    let orphan = seed_never_published_object(&fresh.fixture).await;
    let deletes_before = fresh.store.deletes();
    let worker = ForgeWorker::new(
        fresh.supervised.forge(),
        ForgeWorkerConfig::default(),
        Uuid::now_v7(),
    )
    .expect("fixture Forge worker");
    let fresh_task = seed_ready_expiry_task(&fresh.fixture, fresh.watermark, "55").await;
    let claim = worker
        .claim_for_test()
        .await
        .expect("the production claim transaction runs")
        .expect("the ready snapshot-expiry task is claimable");
    assert_eq!(claim.task_id, fresh_task);
    worker
        .execute_snapshot_expiry_claim_for_test(claim, &CancellationToken::new())
        .await
        .expect("fresh expiration succeeds");
    assert_eq!(
        expiry_state(&fresh.fixture, fresh_task).await.task_state,
        "succeeded"
    );
    assert_orphan_survives_expiration(&fresh, &orphan, deletes_before).await;
    fresh.supervised.shutdown().await;

    let recovered = expirable_table("expiry_no_orphan_takeover").await;
    let orphan = seed_never_published_object(&recovered.fixture).await;
    let deletes_before = recovered.store.deletes();
    let forge = recovered.supervised.forge();
    let (prepared_task, _attempt, _prepared) = prepare_and_abandon_at_the_catalog_gate(
        &recovered.fixture,
        &recovered.seam,
        &forge,
        recovered.watermark,
    )
    .await;
    sqlx::query("UPDATE vala.maintenance_leases SET expires_at = now() - interval '1 hour'")
        .execute(recovered.fixture.operator_pool.pool())
        .await
        .expect("the dead preparing worker's lease ages out");
    sqlx::query(
        "UPDATE vala.forge_tasks SET claim_expires_at = now() - interval '1 hour' WHERE task_id = $1",
    )
    .bind(prepared_task)
    .execute(recovered.fixture.operator_pool.pool())
    .await
    .expect("the prepared claim ages out");
    let successor = ForgeWorker::new(
        Arc::clone(&forge),
        ForgeWorkerConfig::default(),
        Uuid::now_v7(),
    )
    .expect("fixture successor worker");
    assert!(
        successor
            .execute_one_for_test(&CancellationToken::new())
            .await
            .expect("prepared reconciliation settles the taken-over expiration"),
        "the successor found the prepared task"
    );
    assert_eq!(
        expiry_state(&recovered.fixture, prepared_task)
            .await
            .task_state,
        "succeeded"
    );
    assert_orphan_survives_expiration(&recovered, &orphan, deletes_before).await;

    recovered.supervised.shutdown().await;
}

/// Head-snapshot shape the maintenance assertions compare across one pass.
#[derive(Debug)]
pub(super) struct HeadShape {
    /// Current snapshot id.
    pub(super) snapshot_id: i64,
    /// Current snapshot operation.
    pub(super) operation: Operation,
    /// Data manifests the current snapshot's manifest list names.
    pub(super) data_manifests: usize,
    /// Every snapshot the table metadata retains.
    pub(super) snapshots: BTreeSet<i64>,
}

/// Reads the fixture table's head shape from the catalog.
///
/// # Panics
///
/// Panics when the table, its head, or its manifest list cannot be read.
pub(super) async fn head_shape(fixture: &PromotionIntegrationFixture) -> HeadShape {
    let table = fixture
        .catalog
        .iceberg_catalog()
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("fixture table loads");
    let snapshot = table
        .metadata()
        .current_snapshot()
        .expect("a promotion left a head");
    let manifests = table
        .manifest_list_reader(snapshot)
        .load()
        .await
        .expect("the head manifest list loads");
    HeadShape {
        snapshot_id: snapshot.snapshot_id(),
        operation: snapshot.summary().operation.clone(),
        data_manifests: manifests
            .entries()
            .iter()
            .filter(|manifest| manifest.content == ManifestContentType::Data)
            .count(),
        snapshots: table
            .metadata()
            .snapshots()
            .map(|snapshot| snapshot.snapshot_id())
            .collect(),
    }
}

/// Runs one leader maintenance pass and returns the head before and after.
///
/// # Panics
///
/// Panics when the pass misses its bound or the head cannot be read.
pub(super) async fn maintain_once(table: &ExpirableTable) -> (HeadShape, HeadShape) {
    let before = head_shape(&table.fixture).await;
    table.supervised.maintain_only().await;
    (before, head_shape(&table.fixture).await)
}

/// Starts a compaction-enabled table whose only Forge actor is the leader.
///
/// The worker is joined before anything is owed, so every compaction the
/// scenario pulls stays pulled and never starts: the leader's in-memory
/// track is then the only record of its watermark. The first promotion runs
/// on the leader's own heartbeat pass.
///
/// # Panics
///
/// Panics when a fixture dependency fails or the first promotion commits no
/// snapshot.
pub(super) async fn leader_only_table(name: &str) -> ExpirableTable {
    let fixture = PromotionIntegrationFixture::start(name).await;
    let store = CountingObjectStore::new(Arc::clone(&fixture.staging));
    let seam = PromotionCatalogSeam::new(fixture.catalog.iceberg_catalog(), store.read_counter());
    let (clock, control) = manual_clock();
    let mut supervised = SupervisedPromotion::start(
        &fixture,
        Arc::clone(&seam) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&store) as Arc<dyn vala_bifrost_redux::forge::ForgeObjectStore>,
        clock,
    );
    supervised.join_worker().await;
    supervised.schedule_only().await;
    let watermark = head_watermark(&fixture).await;
    ExpirableTable {
        fixture,
        store,
        seam,
        supervised,
        control,
        watermark,
    }
}

/// Promotes one more sealed batch on the leader's pass and returns the new head.
///
/// # Panics
///
/// Panics when the seal or the pass fails, or the head does not move.
pub(super) async fn commit_snapshot(table: &ExpirableTable) -> i64 {
    let (before, _) = head_watermark(&table.fixture).await;
    table.fixture.seal_more(1).await;
    table.supervised.schedule_only().await;
    let (after, _) = head_watermark(&table.fixture).await;
    assert_ne!(after, before, "the leader pass promoted the sealed batch");
    after
}

/// Returns the leader schedule's key for the fixture table.
///
/// # Panics
///
/// Panics when the fixture identity is invalid.
fn table_key(fixture: &PromotionIntegrationFixture) -> ForgeTableKey {
    ForgeTableKey {
        tenant: fixture.tenant,
        table: ForgeTaskTableIdentity::new(
            "wyrd-redux",
            fixture.binding.table_ref.namespace.as_str(),
            &fixture.binding.table_ref.name,
        )
        .expect("fixture table identity"),
    }
}

/// Returns the schedule of the leader term the supervisor holds.
///
/// # Panics
///
/// Panics when the supervisor's Forge holds no leader term.
fn leader_term(table: &ExpirableTable) -> Arc<ForgeHeldTerm> {
    table
        .supervised
        .forge()
        .held_leader_term()
        .expect("the supervisor's pass holds the leader term")
}

/// Lists the URIs of one retained snapshot's manifest list and manifests.
///
/// # Panics
///
/// Panics when the table, the snapshot, or its manifest list cannot be read.
pub(super) async fn snapshot_files(fixture: &PromotionIntegrationFixture, snapshot_id: i64) -> Vec<String> {
    let table = fixture
        .catalog
        .iceberg_catalog()
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("fixture table loads");
    let snapshot = table
        .metadata()
        .snapshot_by_id(snapshot_id)
        .expect("the snapshot is retained");
    let manifests = table
        .manifest_list_reader(snapshot)
        .load()
        .await
        .expect("the manifest list loads");
    std::iter::once(snapshot.manifest_list().to_owned())
        .chain(
            manifests
                .entries()
                .iter()
                .map(|manifest| manifest.manifest_path.clone()),
        )
        .collect()
}

/// Asserts every named Iceberg file still exists in table storage.
///
/// # Panics
///
/// Panics when a file is missing or its existence cannot be read.
pub(super) async fn assert_files_exist(fixture: &PromotionIntegrationFixture, files: &[String], why: &str) {
    let table = fixture
        .catalog
        .iceberg_catalog()
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("fixture table loads");
    for file in files {
        assert!(
            table
                .file_io()
                .exists(file)
                .await
                .expect("file existence reads"),
            "{why}: {file} was deleted"
        );
    }
}

/// The one expired-cleanup task a committed expiration's handoff became.
pub(super) struct CleanupTask {
    /// Durable task identity.
    pub(super) task_id: Uuid,
    /// Current `vala.forge_tasks.state`.
    pub(super) state: String,
    /// Exact candidate paths the task's immutable plan copied from the handoff.
    pub(super) paths: Vec<String>,
}

/// Reads the fixture tenant's newest expired-cleanup task and its candidates.
///
/// # Panics
///
/// Panics when no cleanup task exists or its plan cannot be read.
pub(super) async fn latest_cleanup_task(fixture: &PromotionIntegrationFixture) -> CleanupTask {
    let pool = fixture.operator_pool.pool();
    let (task_id, state): (Uuid, String) = sqlx::query_as(
        "SELECT task_id, state FROM vala.forge_tasks \
         WHERE data_tenant_id = $1 AND strategy = 'expired_cleanup' \
         ORDER BY task_id DESC LIMIT 1",
    )
    .bind(fixture.tenant.as_uuid())
    .fetch_one(pool)
    .await
    .expect("a committed expiration's handoff became one cleanup task");
    let paths: Vec<String> = sqlx::query_scalar(
        "SELECT candidate->>'path' FROM vala.forge_tasks, \
         jsonb_array_elements(plan #> '{parameters,cleanup_candidates}') \
         WITH ORDINALITY AS c(candidate, ordinal) \
         WHERE task_id = $1 ORDER BY ordinal",
    )
    .bind(task_id)
    .fetch_all(pool)
    .await
    .expect("cleanup candidates read");
    CleanupTask {
        task_id,
        state,
        paths,
    }
}

/// Counts the fixture tenant's recorded snapshot-expiry attempts.
///
/// # Panics
///
/// Panics when the read-only count fails.
async fn expiry_attempts(fixture: &PromotionIntegrationFixture) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM vala.forge_tasks \
         WHERE data_tenant_id = $1 AND strategy = 'snapshot_expiry'",
    )
    .bind(fixture.tenant.as_uuid())
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("expiry attempt count")
}

/// An in-flight compaction's watermark clamps expiry; no watermark skips it.
///
/// Phase one pulls a compaction through the production pull route after two
/// commits, so the leader holds its watermark in memory only, then commits
/// twice more. The maintenance pass must expire the one snapshot older than
/// the watermark and keep the watermark and everything after it; a further
/// pass, with nothing outside the held chain left, must not open an expiry
/// attempt. Phase two runs a manual compaction on a
/// table the leader has observed no commit for, so the dispatch carries no
/// watermark at all, and the pass must expire nothing.
///
/// # Panics
///
/// Panics when a protected snapshot is expired, an unprotected one survives,
/// or the leader routes misbehave.
async fn assert_compaction_watermark_blocks_expiry(table: &ExpirableTable) {
    let forge = table.supervised.forge();
    let key = table_key(&table.fixture);
    let first = table.watermark.0;
    let held = commit_snapshot(table).await;
    let dispatch = forge.pull_compaction(4).await.expect("leader pull");
    assert_eq!(dispatch.len(), 1, "the promoted table is due: {dispatch:?}");
    assert_eq!(
        leader_term(table).schedule().processing_watermark(&key),
        Some(Some(held)),
        "the pull captured the latest observed snapshot"
    );
    let later = [commit_snapshot(table).await, commit_snapshot(table).await];
    let held_files = snapshot_files(&table.fixture, held).await;
    let (_, after) = maintain_once(table).await;
    assert_eq!(
        after.snapshots,
        BTreeSet::from([held, later[0], later[1]]),
        "expiry removes only what precedes the in-flight watermark {held} (first was {first})"
    );
    assert_files_exist(&table.fixture, &held_files, "the watermark snapshot").await;
    // With the watermark now the oldest snapshot, the held chain covers every
    // snapshot, so the leader records no expiry attempt at all.
    let attempts = expiry_attempts(&table.fixture).await;
    let (before, after) = maintain_once(table).await;
    assert_eq!(after.snapshots, before.snapshots);
    assert_eq!(
        expiry_attempts(&table.fixture).await,
        attempts,
        "a held chain covering every snapshot opens no expiry attempt"
    );
    forge
        .report_compaction(&dispatch[0], ForgeCompactionOutcome::Succeeded)
        .await
        .expect("leader report");

    // A table whose compaction is disabled has no track, so a manual run
    // creates one that has observed no snapshot. The request carries the
    // table's declared opt-out, so its track is temporary.
    set_table_properties(
        &table.fixture.catalog,
        &table.fixture.binding,
        &[("wyrd.forge.enable-compaction", "false")],
    )
    .await;
    commit_snapshot(table).await;
    assert!(
        leader_term(table).schedule().track_for_test(&key).is_none(),
        "a disabled table's idle track is dropped"
    );
    leader_term(table).schedule().request_compaction(
        &key,
        &ForgeTableSettings {
            compaction_enabled: false,
            ..ForgeTableSettings::default()
        },
        chrono::Utc::now(),
    );
    let manual = forge.pull_compaction(4).await.expect("leader pull");
    assert_eq!(manual.len(), 1, "the manual run is due: {manual:?}");
    assert_eq!(
        leader_term(table).schedule().processing_watermark(&key),
        Some(None),
        "a manual run on an unobserved table holds no watermark"
    );
    let (before, after) = maintain_once(table).await;
    assert_eq!(
        after.snapshots, before.snapshots,
        "a compaction without an observed snapshot skips expiry entirely"
    );
    forge
        .report_compaction(&manual[0], ForgeCompactionOutcome::Succeeded)
        .await
        .expect("leader report");
    assert!(
        leader_term(table).schedule().track_for_test(&key).is_none(),
        "the manual run's temporary track is removed after its report"
    );
}

/// A cleanup that fails after expiry commits is retried for exactly its files.
///
/// The first delete's acknowledgement is lost, so the catalog expiry commits
/// while its cleanup task retains that candidate, prepared, for replay under
/// its claim. Once the claim lapses, the production worker loop reclaims the
/// task and must drain exactly the handed-off candidates: the replayed one
/// advances on a stat that proves it absent, every other one is deleted once,
/// and the retained head's files are untouched.
///
/// # Panics
///
/// Panics when the expiry does not commit, the failed cleanup advances, the
/// retry deletes more or fewer objects than were handed off, or a retained
/// file is removed.
async fn assert_failed_cleanup_retries_exactly(table: &mut ExpirableTable) {
    let head = commit_snapshot(table).await;
    let head_files = snapshot_files(&table.fixture, head).await;
    table.store.fail_next_deletes(1);
    let deletes_before = table.store.deletes();
    let (_, after) = maintain_once(table).await;
    assert_eq!(
        after.snapshots,
        BTreeSet::from([head]),
        "the catalog expiry commits although its cleanup fails"
    );
    let failed = latest_cleanup_task(&table.fixture).await;
    assert_eq!(
        failed.state, "prepared",
        "an unknown delete acceptance retains its candidate for replay"
    );
    assert!(
        !failed.paths.is_empty(),
        "expiry handed off the files it freed"
    );
    assert_eq!(
        table.store.deletes(),
        deletes_before + 1,
        "the failed cleanup stopped at its first delete"
    );
    assert!(
        !object_exists(&table.fixture, &failed.paths[0]).await,
        "the delete whose acknowledgement was lost still took effect"
    );
    for path in &failed.paths[1..] {
        assert!(
            object_exists(&table.fixture, path).await,
            "nothing after the uncertain candidate was deleted: {path}"
        );
    }

    expire_claim(table.fixture.operator_pool.pool(), failed.task_id).await;
    let retry_before = table.store.deletes();
    table.supervised.restart_worker();
    table.supervised.settle_one_success().await;
    let retried = latest_cleanup_task(&table.fixture).await;
    assert_eq!(retried.task_id, failed.task_id, "the same task is retried");
    assert_eq!(retried.state, "succeeded");
    for path in &retried.paths {
        assert!(
            !object_exists(&table.fixture, path).await,
            "the retry deleted handed-off {path}"
        );
    }
    assert_eq!(
        table.store.deletes() - retry_before,
        retried.paths.len() - 1,
        "the replayed candidate is proven absent, and each other one is deleted once"
    );
    assert_eq!(
        table.store.deletes() - deletes_before,
        retried.paths.len(),
        "the failed pass and its retry delete each handed-off file exactly once"
    );
    assert_files_exist(&table.fixture, &head_files, "the retained head").await;
}

/// Leader maintenance never expires what an active watermark protects.
///
/// Drives the production leader maintenance pass, never a seeded task, over
/// one table: an in-flight compaction's watermark holds its chain and a
/// watermark-less one skips expiry; and a cleanup that fails after its expiry
/// committed is
/// retried, once its claim lapses, for exactly the handed-off files.
///
/// # Panics
///
/// Panics when any phase's retained snapshots, files, or deletes differ.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Postgres, Iceberg, and object storage"]
async fn active_watermark_blocks_expiry_and_failed_cleanup_retries_exactly() {
    let _telemetry = ForgeTelemetryCheckpoint::install();
    let mut table = leader_only_table("expiry_protection").await;
    assert_compaction_watermark_blocks_expiry(&table).await;
    assert_failed_cleanup_retries_exactly(&mut table).await;
    table.supervised.shutdown().await;
}
