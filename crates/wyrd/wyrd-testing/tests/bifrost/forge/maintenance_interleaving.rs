//! Replay real Forge maintenance ticks against durable Iceberg state.

use std::sync::Arc;
use std::time::Duration;

use iceberg::Catalog;
use opendal::Buffer;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::forge::{
    ForgeError, ForgeLease, ForgeObjectStore, ForgeSchedulerTrigger, ForgeWorker,
    ForgeWorkerCompletionObserver, ForgeWorkerConfig, current_gc_gate_for_test, forge_lease_key,
};
use vala_sql::row_types::forge_operations::{
    ForgeOperationFamily, ForgeOperationPhase, ForgeOperationStateRow,
};
use wyrd_server::BifrostTarget;
use wyrd_spec::vala::api::{
    AuditDetail, ForgeCompactionPhase, ForgeOrphanGcPhase, ForgeSnapshotExpirePhase, StoragePath,
};
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::{CommitUncertaintyCatalog, ForgeObjectStoreControl, seed_forge_group};

/// Stable maintenance scheduler identity reused after supervised restarts.
const MAINTENANCE_SCHEDULER_OWNER: u128 = 0x0198_39f4_2b51_7000_8000_0000_0000_0002;

/// Start maintenance dependencies without a competing background Forge role.
///
/// # Panics
///
/// Panics when the isolated in-process server cannot start.
async fn start_maintenance_server() -> WyrdTestServer {
    WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .with_forge_process_role_for_test(BifrostTarget::Server)
        .start_in_process()
        .await
        .expect("in-process Forge maintenance server")
}

/// Bounded production scheduler and worker pair for one maintenance phase.
struct SupervisedMaintenance {
    /// Passive trigger for deterministic scheduler passes.
    scheduler_trigger: ForgeSchedulerTrigger,
    /// Passive worker attempt and completion observer.
    worker_observer: ForgeWorkerCompletionObserver,
    /// Scheduler cancellation boundary.
    scheduler_stop: CancellationToken,
    /// Worker cancellation boundary.
    worker_stop: CancellationToken,
    /// Running production scheduler.
    scheduler_task: JoinHandle<Result<(), ForgeError>>,
    /// Running production worker, consumed exactly once during shutdown.
    worker_task: Option<JoinHandle<Result<(), ForgeError>>>,
}

impl SupervisedMaintenance {
    /// Start production supervision over explicit catalog and object-store seams.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot build a validated worker.
    fn start(
        fixture: &wyrd_testing::bifrost::ForgeFixture,
        config: vala_bifrost_redux::forge::ForgeConfig,
        catalog: Arc<dyn Catalog>,
        object_store: Arc<dyn ForgeObjectStore>,
    ) -> Self {
        let scheduler_trigger = ForgeSchedulerTrigger::with_owner_for_test(uuid::Uuid::from_u128(
            MAINTENANCE_SCHEDULER_OWNER,
        ));
        let worker_observer = ForgeWorkerCompletionObserver::new();
        let (forge, _) = fixture.context_with_worker_supervision(
            config,
            catalog,
            object_store,
            worker_observer.clone(),
            scheduler_trigger.clone(),
        );
        let worker = ForgeWorker::new(
            Arc::clone(&forge),
            ForgeWorkerConfig::default(),
            uuid::Uuid::now_v7(),
        )
        .expect("validated maintenance worker");
        let scheduler_stop = CancellationToken::new();
        let worker_stop = CancellationToken::new();
        let scheduler_task = tokio::spawn({
            let stop = scheduler_stop.clone();
            async move { forge.run(stop).await }
        });
        let worker_task = tokio::spawn({
            let stop = worker_stop.clone();
            async move { worker.run(stop).await }
        });
        Self {
            scheduler_trigger,
            worker_observer,
            scheduler_stop,
            worker_stop,
            scheduler_task,
            worker_task: Some(worker_task),
        }
    }

    /// Start supervision over the fixture's ordinary dependencies.
    fn start_default(
        fixture: &wyrd_testing::bifrost::ForgeFixture,
        config: vala_bifrost_redux::forge::ForgeConfig,
    ) -> Self {
        Self::start(
            fixture,
            config,
            Arc::clone(&fixture.catalog),
            Arc::clone(&fixture.object_store),
        )
    }

    /// Request and await one production scheduler pass.
    ///
    /// # Panics
    ///
    /// Panics when the scheduler misses its deterministic bound.
    async fn schedule_once(&self) {
        let expected = self.scheduler_trigger.completed_passes().saturating_add(1);
        self.scheduler_trigger.request_pass();
        tokio::time::timeout(
            Duration::from_secs(30),
            self.scheduler_trigger.wait_for_passes_at_least(expected),
        )
        .await
        .expect("maintenance scheduler pass bound");
    }

    /// Arm the passive barrier after the next returned worker attempt.
    fn hold_next_attempt(&self) {
        self.worker_observer.hold_after_next_attempt_for_test();
    }

    /// Await the armed returned-attempt barrier.
    ///
    /// # Panics
    ///
    /// Panics when no worker attempt returns within the deterministic bound.
    async fn wait_for_held_attempt(&self) {
        tokio::time::timeout(
            Duration::from_secs(30),
            self.worker_observer.wait_for_held_attempt_for_test(),
        )
        .await
        .expect("maintenance worker attempt bound");
    }

    /// Run and join exactly one successful production attempt.
    ///
    /// # Panics
    ///
    /// Panics when the attempt fails, retries, or misses its bound.
    async fn run_one_success(&mut self) {
        let attempts = self.worker_observer.attempts().saturating_add(1);
        let completions = self.worker_observer.completed().saturating_add(1);
        let errors = self.worker_observer.returned_errors().len();
        self.hold_next_attempt();
        self.schedule_once().await;
        self.wait_for_held_attempt().await;
        assert_eq!(self.worker_observer.attempts(), attempts);
        assert_eq!(self.worker_observer.returned_errors().len(), errors);
        self.stop_worker().await;
        assert_eq!(self.worker_observer.completed(), completions);
    }

    /// Cancel and join the worker before it can retry.
    ///
    /// # Panics
    ///
    /// Panics when shutdown misses its bound or the worker exits unexpectedly.
    async fn stop_worker(&mut self) {
        self.worker_stop.cancel();
        self.worker_observer.release_held_attempt_for_test();
        let task = self
            .worker_task
            .take()
            .expect("maintenance worker stops once");
        tokio::time::timeout(Duration::from_secs(30), task)
            .await
            .expect("maintenance worker shutdown bound")
            .expect("maintenance worker task")
            .expect("maintenance worker shutdown");
    }

    /// Cancel and join both supervisors.
    ///
    /// # Panics
    ///
    /// Panics when either supervisor misses its shutdown bound.
    async fn shutdown(mut self) {
        self.scheduler_stop.cancel();
        if self.worker_task.is_some() {
            self.stop_worker().await;
        }
        tokio::time::timeout(Duration::from_secs(30), self.scheduler_task)
            .await
            .expect("maintenance scheduler shutdown bound")
            .expect("maintenance scheduler task")
            .expect("maintenance scheduler shutdown");
    }
}

/// Make the exact nonterminal task left by a fully joined worker reclaimable.
///
/// # Panics
///
/// Panics unless one exact Running, Prepared, or already-settled Retryable task exists.
async fn expire_stopped_claim(fixture: &wyrd_testing::bifrost::ForgeFixture) -> uuid::Uuid {
    let (task_id, state, attempt_id, owner): (
        uuid::Uuid,
        String,
        Option<uuid::Uuid>,
        Option<uuid::Uuid>,
    ) = sqlx::query_as(
        "SELECT task_id,state,attempt_id,claimed_by FROM vala.forge_tasks \
         WHERE data_tenant_id = $1 AND state IN ('running','prepared','retryable')",
    )
    .bind(fixture.tenant.as_uuid())
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("one stopped maintenance claim");
    if state == "retryable" {
        sqlx::query(
            "UPDATE vala.forge_tasks SET ready_at=statement_timestamp(),next_eligible_at=statement_timestamp() WHERE task_id=$1",
        )
        .bind(task_id)
        .execute(fixture.operator_pool.pool())
        .await
        .expect("advance settled maintenance retry eligibility");
        return task_id;
    }
    let attempt_id = attempt_id.expect("active maintenance attempt identity");
    let owner = owner.expect("active maintenance claim owner");
    let expired: uuid::Uuid = sqlx::query_scalar(
        "UPDATE vala.forge_tasks \
         SET claim_expires_at = statement_timestamp() - interval '1 millisecond' \
         WHERE task_id = $1 AND attempt_id = $2 AND claimed_by = $3 \
           AND state IN ('running', 'prepared') RETURNING task_id",
    )
    .bind(task_id)
    .bind(attempt_id)
    .bind(owner)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("expire exact maintenance claim");
    assert_eq!(expired, task_id);
    task_id
}

/// Commit one exact staged pair through the production scheduler and worker.
///
/// # Panics
///
/// Panics when the staged pair does not produce one successful worker task.
async fn commit_staging_snapshot(fixture: &wyrd_testing::bifrost::ForgeFixture) {
    let mut config = fixture.config.clone();
    config.max_files_per_bin = 2;
    config.max_files_per_tick = 2;
    for _ in 0..2 {
        let mut lifecycle = SupervisedMaintenance::start_default(fixture, config.clone());
        lifecycle.run_one_success().await;
        let strategies = lifecycle.worker_observer.completed_strategies();
        lifecycle.shutdown().await;
        if strategies
            .first()
            .is_some_and(|strategy| strategy.as_str() == "staging_fold")
        {
            return;
        }
    }
    panic!("bounded maintenance setup did not execute the staged pair");
}

/// Drain one production live-rewrite task created by staging setup.
///
/// # Panics
///
/// Panics when the bounded worker phase does not complete a small-files task.
async fn commit_live_rewrite(fixture: &wyrd_testing::bifrost::ForgeFixture) {
    let mut lifecycle = SupervisedMaintenance::start_default(fixture, fixture.config.clone());
    lifecycle.run_one_success().await;
    let strategies = lifecycle.worker_observer.completed_strategies();
    lifecycle.shutdown().await;
    assert!(
        strategies
            .first()
            .is_some_and(|strategy| strategy.as_str() == "small_files"),
        "setup must drain its live rewrite before maintenance: {strategies:?}",
    );
}

/// Seed terminal Reset evidence for exact never-published Forge generations.
///
/// The production transition writers persist Prepared then Reset for one real
/// staged pair. The caller subsequently retries that pair through the worker,
/// leaving only the reset generation paths eligible for orphan GC.
///
/// # Panics
///
/// Panics when staging rows, lease ownership, transition persistence, or object
/// creation diverges from the production contracts.
async fn seed_reset_generations(
    fixture: &wyrd_testing::bifrost::ForgeFixture,
    output_count: usize,
) -> Vec<String> {
    fixture.append_forge_file(2).await;
    fixture.append_forge_file(3).await;
    let rows: Vec<(uuid::Uuid, String, String, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT id, file_path, partition_granularity, partition_start FROM vala.file_list \
         WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3 \
           AND committed_snapshot_id IS NULL ORDER BY id",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_all(fixture.operator_pool.pool())
    .await
    .expect("reset staging rows");
    assert_eq!(rows.len(), 2);
    let partition_day = vala_bifrost_redux::catalog::layout::TimePartition::from_durable_columns(
        &rows[0].2, rows[0].3,
    )
    .expect("durable file-list rows carry an exact time partition");
    assert!(
        rows.iter()
            .all(|row| row.2 == rows[0].2 && row.3 == rows[0].3)
    );
    let outputs = (0..output_count)
        .map(|ordinal| {
            format!(
                "{}/data/forge/bifrost-writer-v2/{}-{ordinal:05}.parquet",
                fixture.binding.object_prefix,
                uuid::Uuid::now_v7(),
            )
        })
        .collect::<Vec<_>>();
    for output in &outputs {
        fixture
            .staging
            .write(output, Buffer::from(vec![9_u8]))
            .await
            .expect("reset generation object");
    }
    let input_file_ids = rows.iter().map(|row| row.0).collect::<Vec<_>>();
    let operation_id = uuid::Uuid::now_v7();
    let detail = AuditDetail::ForgeCompaction {
        operation_id,
        phase: ForgeCompactionPhase::Prepared,
        group: format!(
            "bifrost://{}/{}/{}",
            fixture.tenant, fixture.binding.table_ref.namespace, fixture.binding.table_ref.name,
        ),
        input_file_ids: input_file_ids.clone(),
        input_paths: rows
            .iter()
            .map(|row| StoragePath::new(row.1.clone()).expect("reset input storage path"))
            .collect(),
        output_paths: outputs
            .iter()
            .map(|path| StoragePath::new(path.clone()).expect("reset output storage path"))
            .collect(),
        snapshot_id: None,
        writer_recipe_version: "bifrost-writer-v2".to_owned(),
    };
    let lease_key = forge_lease_key(
        fixture.tenant,
        &fixture.binding.logical_namespace,
        &fixture.binding.table_name,
    );
    let mut lease = ForgeLease::acquire(
        &fixture.operator_pool,
        lease_key,
        uuid::Uuid::now_v7(),
        fixture.config.lease_ttl,
    )
    .await
    .expect("reset generation lease query")
    .expect("reset generation lease");
    fixture
        .forge
        .append_compaction_transition_for_test(
            &mut lease,
            &fixture.binding,
            partition_day,
            detail.clone(),
            "forge.file_compact.prepared",
        )
        .await
        .expect("prepared reset generation");
    fixture
        .forge
        .reset_reconciled_for_test(
            &mut lease,
            &fixture.binding,
            partition_day,
            &input_file_ids,
            &detail,
        )
        .await
        .expect("terminal reset generation");
    lease
        .release(&fixture.operator_pool)
        .await
        .expect("reset generation lease release");
    let persisted: serde_json::Value = sqlx::query_scalar(
        "SELECT current_detail FROM vala.forge_operation_state \
         WHERE data_tenant_id = $1 AND family = 'staging_fold' \
           AND operation_id = $2 AND phase = 'reset'",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(operation_id)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("exact Reset projection");
    assert_eq!(
        persisted["output_paths"],
        serde_json::Value::Array(
            outputs
                .iter()
                .cloned()
                .map(serde_json::Value::String)
                .collect(),
        ),
    );
    assert!(
        outputs
            .iter()
            .all(|path| { vala_bifrost_redux::forge::Forge::known_iceberg_object_for_test(path) })
    );
    assert_eq!(
        fixture
            .forge
            .reset_generation_paths_for_test(&fixture.binding)
            .await
            .expect("production Reset generation projection"),
        outputs,
    );
    outputs
}

/// Acquire a successor owner after expiring the fixture's table lease.
///
/// The helper models a second worker taking ownership while the first worker
/// is paused at a destructive or terminal-audit boundary.
///
/// # Errors
///
/// Panics when the lease cannot be expired or acquired, because either result
/// means the deterministic takeover precondition was not established.
async fn steal_forge_lease(fixture: &wyrd_testing::bifrost::ForgeFixture) -> (uuid::Uuid, i64) {
    let lease_key = format!(
        "forge:table:{}:{}:{}",
        fixture.tenant, fixture.binding.logical_namespace, fixture.binding.table_name
    );
    sqlx::query("UPDATE vala.maintenance_leases SET expires_at = now() - interval '1 second' WHERE lease_key = $1")
        .bind(&lease_key)
        .execute(fixture.operator_pool.pool())
        .await
        .expect("expire Forge lease for deterministic takeover");
    let owner = uuid::Uuid::now_v7();
    let token = vala_sql::queries::maintenance_leases::try_acquire_lease(
        &fixture.operator_pool,
        &lease_key,
        owner,
        i64::try_from(fixture.config.lease_ttl.as_secs()).expect("lease seconds"),
    )
    .await
    .expect("successor lease acquisition")
    .expect("successor owns expired Forge lease");
    assert!(
        token.takeover,
        "expired different owner must report takeover"
    );
    (owner, token.fencing_token)
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Tests the list-to-delete GC race with a real durable file-list reference.
///
/// Steps:
/// 1. Seed a real table and age an orphan object in the local OpenDAL store.
/// 2. Pause the real GC list return, insert a durable `vala.file_list` row for
///    that path, and resume the production tick.
/// 3. Assert the object remains present and the durable audit table still has
///    exactly one compaction commit.
///
/// This proves that a reference appearing after the initial live-set/list
/// boundary is observed by the final per-object live-set check before delete.
///
/// # Errors
///
/// The test panics if the server, barriers, durable maintenance tick, or
/// object-store assertions fail. Cancellation is bounded by the test runtime;
/// no detached maintenance task is permitted to outlive the test.
async fn forge_gc_replay_preserves_live_reference() {
    let server = start_maintenance_server().await;
    let fixture = seed_forge_group(&server, "maintenance_reference_rows").await;
    commit_staging_snapshot(&fixture).await;
    fixture.append_forge_file(2).await;
    fixture.append_forge_file(3).await;
    commit_staging_snapshot(&fixture).await;
    let mut config = fixture.config.clone();
    config.min_files = 3;
    config.orphan_gc_ttl = Duration::from_millis(1);
    config.snapshot_retention = Duration::from_millis(1);
    config.maintenance_trigger_snapshot_count = 1;
    config.maintenance_trigger_interval = Duration::from_nanos(1);
    let control = ForgeObjectStoreControl::new(Arc::clone(&fixture.staging));
    control.pause_next_list();
    let orphan = format!("{}/reference-race.parquet", fixture.binding.object_prefix);
    fixture
        .staging
        .write(&orphan, Buffer::from(vec![1_u8]))
        .await
        .expect("race orphan object");
    let modified = fixture
        .staging
        .stat(&orphan)
        .await
        .expect("orphan metadata")
        .last_modified()
        .expect("object age evidence")
        .into_inner()
        .as_millisecond();
    server
        .forge_clock()
        .set(
            chrono::DateTime::from_timestamp_millis(modified + 100)
                .expect("object timestamp is UTC-representable"),
        )
        .expect("advance Forge object age");
    let mut lifecycle = SupervisedMaintenance::start(
        &fixture,
        config,
        Arc::clone(&fixture.catalog),
        control.clone(),
    );
    lifecycle.hold_next_attempt();
    lifecycle.schedule_once().await;
    tokio::time::timeout(Duration::from_secs(30), control.wait_for_list())
        .await
        .expect("maintenance GC list boundary");
    fixture.protect_path(&orphan).await;
    control.release_list();
    lifecycle.wait_for_held_attempt().await;
    assert!(lifecycle.worker_observer.returned_errors().is_empty());
    lifecycle.stop_worker().await;
    lifecycle.shutdown().await;
    assert!(control.stat(&orphan).await.is_ok());
    assert_eq!(
        fixture
            .operation_count("forge.file_compact.committed")
            .await,
        2
    );
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Tests partial GC failure and restart recovery with real object-store state.
///
/// Steps:
/// 1. Seed a real table, disable compaction eligibility, and write two aged
///    orphan objects inside its owned prefix.
/// 2. Wrap the real OpenDAL operator so the second delete fails after the first
///    delete has already succeeded; run one public maintenance tick.
/// 3. Restart through a fresh production context, reconcile the prepared GC
///    operation, and assert both objects are absent and one terminal audit
///    record describes the recovered operation.
///
/// The induced failure is a bounded partial batch, not a synthetic state
/// transition. Durable object absence plus the terminal audit proves restart
/// safety, idempotent handling of the already-absent first object, and no
/// dependence on a process-local deletion list.
///
/// # Errors
///
/// The test panics when the injected delete failure, restart tick, or terminal
/// audit assertions do not observe the expected durable state. Restart is
/// synchronous with the test and does not leave a detached task running.
async fn forge_gc_partial_delete_restarts_idempotently() {
    let server = start_maintenance_server().await;
    let fixture = seed_forge_group(&server, "maintenance_partial_delete_rows").await;
    commit_staging_snapshot(&fixture).await;
    let reset_outputs = seed_reset_generations(&fixture, 2).await;
    commit_staging_snapshot(&fixture).await;
    commit_live_rewrite(&fixture).await;
    let mut config = fixture.config.clone();
    config.min_files = 3;
    config.orphan_gc_ttl = Duration::from_millis(1);
    config.snapshot_retention = Duration::from_millis(1);
    config.maintenance_trigger_snapshot_count = 1;
    config.maintenance_trigger_interval = Duration::from_nanos(1);
    let control = ForgeObjectStoreControl::new(Arc::clone(&fixture.staging));
    control.fail_delete_at(3);
    let first_orphan = &reset_outputs[0];
    let second_orphan = &reset_outputs[1];
    let modified = fixture
        .staging
        .stat(second_orphan)
        .await
        .expect("orphan metadata")
        .last_modified()
        .expect("object age evidence")
        .into_inner()
        .as_millisecond();
    server
        .forge_clock()
        .set(
            chrono::DateTime::from_timestamp_millis(modified + 100)
                .expect("object timestamp is UTC-representable"),
        )
        .expect("advance Forge object age");
    let diagnostic = fixture.context_with_object_store(config.clone(), control.clone());
    assert_eq!(
        diagnostic
            .gc_eligibility_for_test(&fixture.binding, first_orphan)
            .await
            .expect("first Reset generation eligibility"),
        "Eligible",
    );
    assert_eq!(
        diagnostic
            .gc_eligibility_for_test(&fixture.binding, second_orphan)
            .await
            .expect("second Reset generation eligibility"),
        "Eligible",
    );
    let mut first = SupervisedMaintenance::start(
        &fixture,
        config.clone(),
        Arc::clone(&fixture.catalog),
        control.clone(),
    );
    first.hold_next_attempt();
    first.schedule_once().await;
    first.wait_for_held_attempt().await;
    assert_eq!(
        control.delete_calls(),
        3,
        "completed={}, errors={:?}",
        first.worker_observer.completed(),
        first.worker_observer.returned_errors(),
    );
    assert!(!first.worker_observer.returned_errors().is_empty());
    first.stop_worker().await;
    first.shutdown().await;
    let first_exists = control.stat(first_orphan).await.is_ok();
    let second_exists = control.stat(second_orphan).await.is_ok();
    assert_ne!(first_exists, second_exists);

    let recovered_task = expire_stopped_claim(&fixture).await;
    let mut recovery = SupervisedMaintenance::start_default(&fixture, config);
    recovery.run_one_success().await;
    assert_eq!(
        recovery.worker_observer.completed_tasks(),
        vec![recovered_task]
    );
    recovery.shutdown().await;
    assert!(fixture.staging.stat(first_orphan).await.is_err());
    assert!(fixture.staging.stat(second_orphan).await.is_err());
    let terminal = fixture.operation_count("forge.orphan_gc.committed").await
        + fixture.operation_count("forge.orphan_gc.recovered").await;
    assert_eq!(terminal, 1);
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Extends the maintenance journey with the orphan-GC evidence negative flow.
///
/// Steps:
/// 1. Seed a real table and one never-published Reset generation, giving an
///    evidenced orphan object aged past the GC TTL floor.
/// 2. Write a second object in the same prefix with no Reset evidence, aged the
///    same amount, so only the positive-evidence gate distinguishes the two.
/// 3. Run one clean production maintenance tick (compaction disabled) whose
///    snapshot-expiry tail performs orphan GC through the real object store.
/// 4. Assert the evidenced orphan is durably deleted, the unevidenced object
///    survives, and exactly one terminal `forge.orphan_gc.committed` audit is
///    recorded.
///
/// This proves the user-observable GC contract end to end through the real
/// maintenance path: positive Reset evidence plus the TTL floor is required to
/// delete, and an equally aged object lacking that evidence is preserved.
///
/// # Errors
///
/// The test panics when the server, seeding, durable maintenance tick, or
/// object-store assertions diverge from the expected durable state. No detached
/// maintenance task is permitted to outlive the test.
async fn forge_gc_deletes_evidenced_orphan_and_preserves_unevidenced() {
    let server = start_maintenance_server().await;
    let fixture = seed_forge_group(&server, "maintenance_gc_evidence_flow_rows").await;
    commit_staging_snapshot(&fixture).await;
    let reset_outputs = seed_reset_generations(&fixture, 1).await;
    commit_staging_snapshot(&fixture).await;
    commit_live_rewrite(&fixture).await;
    let evidenced = reset_outputs[0].clone();
    let unevidenced = format!(
        "{}/data/forge/bifrost-writer-v2/{}-unevidenced.parquet",
        fixture.binding.object_prefix,
        uuid::Uuid::now_v7(),
    );
    fixture
        .staging
        .write(&unevidenced, Buffer::from(vec![7_u8]))
        .await
        .expect("unevidenced survivor object");
    let mut config = fixture.config.clone();
    config.min_files = 3;
    config.orphan_gc_ttl = Duration::from_millis(1);
    config.snapshot_retention = Duration::from_millis(1);
    config.maintenance_trigger_snapshot_count = 1;
    config.maintenance_trigger_interval = Duration::from_nanos(1);
    let evidenced_modified = fixture
        .staging
        .stat(&evidenced)
        .await
        .expect("evidenced orphan metadata")
        .last_modified()
        .expect("object age evidence")
        .into_inner()
        .as_millisecond();
    let unevidenced_modified = fixture
        .staging
        .stat(&unevidenced)
        .await
        .expect("unevidenced object metadata")
        .last_modified()
        .expect("object age evidence")
        .into_inner()
        .as_millisecond();
    let latest_modified = evidenced_modified.max(unevidenced_modified);
    server
        .forge_clock()
        .set(
            chrono::DateTime::from_timestamp_millis(latest_modified + 100)
                .expect("object timestamp is UTC-representable"),
        )
        .expect("advance Forge object age");
    let control = ForgeObjectStoreControl::new(Arc::clone(&fixture.staging));
    let diagnostic = fixture.context_with_object_store(config.clone(), control.clone());
    assert_eq!(
        diagnostic
            .gc_eligibility_for_test(&fixture.binding, &evidenced)
            .await
            .expect("evidenced orphan eligibility"),
        "Eligible",
    );
    assert_ne!(
        diagnostic
            .gc_eligibility_for_test(&fixture.binding, &unevidenced)
            .await
            .expect("unevidenced object eligibility"),
        "Eligible",
        "an object without positive Reset evidence is never GC-eligible",
    );
    let mut rig = SupervisedMaintenance::start(
        &fixture,
        config,
        Arc::clone(&fixture.catalog),
        control.clone(),
    );
    rig.run_one_success().await;
    assert!(rig.worker_observer.returned_errors().is_empty());
    rig.shutdown().await;
    assert!(
        control.stat(&evidenced).await.is_err(),
        "the evidenced orphan past the TTL floor is durably deleted",
    );
    assert!(
        control.stat(&unevidenced).await.is_ok(),
        "the unevidenced object survives the evidence gate",
    );
    assert_eq!(
        fixture.operation_count("forge.orphan_gc.committed").await,
        1
    );
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Proves that only one exact prepared GC operation may pass its final gate.
///
/// # Panics
///
/// Panics when exact parity is refused, mismatched identity is accepted, or a
/// second prepared operation fails to block destructive maintenance.
async fn forge_gc_current_operation_exemption_requires_exact_parity() {
    let resource = "forge:test:gc";
    let operation_id = uuid::Uuid::now_v7();
    let candidate = StoragePath::new("table/data/orphan.parquet").expect("candidate");
    let detail = AuditDetail::ForgeOrphanGc {
        operation_id,
        phase: ForgeOrphanGcPhase::Prepared,
        group: resource.to_owned(),
        candidate_paths: vec![candidate],
        deleted_paths: Vec::new(),
        skipped_paths: Vec::new(),
    };
    let now = chrono::Utc::now();
    let row = ForgeOperationStateRow {
        resource: resource.to_owned(),
        family: ForgeOperationFamily::OrphanGc,
        operation_id,
        phase: ForgeOperationPhase::Prepared,
        prepared_detail: detail.clone(),
        current_detail: detail.clone(),
        prepared_audit_seq: 1,
        terminal_audit_seq: None,
        prepared_at: now,
        updated_at: now,
    };
    assert!(
        current_gc_gate_for_test(resource, &detail, std::slice::from_ref(&row))
            .expect("exact operation parity")
    );

    let mismatched = AuditDetail::ForgeOrphanGc {
        operation_id: uuid::Uuid::now_v7(),
        phase: ForgeOrphanGcPhase::Prepared,
        group: resource.to_owned(),
        candidate_paths: vec![StoragePath::new("table/data/orphan.parquet").expect("candidate")],
        deleted_paths: Vec::new(),
        skipped_paths: Vec::new(),
    };
    assert!(current_gc_gate_for_test(resource, &mismatched, std::slice::from_ref(&row)).is_err());

    let second_id = uuid::Uuid::now_v7();
    let second_detail = AuditDetail::ForgeOrphanGc {
        operation_id: second_id,
        phase: ForgeOrphanGcPhase::Prepared,
        group: resource.to_owned(),
        candidate_paths: vec![StoragePath::new("table/data/second.parquet").expect("candidate")],
        deleted_paths: Vec::new(),
        skipped_paths: Vec::new(),
    };
    let second = ForgeOperationStateRow {
        resource: resource.to_owned(),
        family: ForgeOperationFamily::OrphanGc,
        operation_id: second_id,
        phase: ForgeOperationPhase::Prepared,
        prepared_detail: second_detail.clone(),
        current_detail: second_detail,
        prepared_audit_seq: 2,
        terminal_audit_seq: None,
        prepared_at: now,
        updated_at: now,
    };
    assert!(
        !current_gc_gate_for_test(resource, &detail, &[row, second])
            .expect("second operation is valid but unsafe")
    );
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Tests fencing immediately before a destructive object-store effect.
///
/// # Errors
///
/// The test panics when lease takeover, stale-worker fencing, or recovery
/// assertions fail.
async fn forge_gc_lease_theft_before_delete_fails_closed() {
    let server = start_maintenance_server().await;
    let fixture = seed_forge_group(&server, "gc_fence_rows").await;
    commit_staging_snapshot(&fixture).await;
    let reset_outputs = seed_reset_generations(&fixture, 1).await;
    commit_staging_snapshot(&fixture).await;
    commit_live_rewrite(&fixture).await;
    let mut config = fixture.config.clone();
    config.min_files = 3;
    config.orphan_gc_ttl = Duration::from_millis(1);
    config.snapshot_retention = Duration::from_millis(1);
    config.maintenance_trigger_snapshot_count = 1;
    config.maintenance_trigger_interval = Duration::from_nanos(1);
    let control = ForgeObjectStoreControl::new(Arc::clone(&fixture.staging));
    control.pause_next_delete();
    let orphan = &reset_outputs[0];
    let modified = fixture
        .staging
        .stat(orphan)
        .await
        .expect("orphan metadata")
        .last_modified()
        .expect("orphan age evidence")
        .into_inner()
        .as_millisecond();
    server
        .forge_clock()
        .set(
            chrono::DateTime::from_timestamp_millis(modified + 100)
                .expect("orphan timestamp is UTC-representable"),
        )
        .expect("advance Forge clock beyond orphan TTL");
    let diagnostic = fixture.context_with_object_store(config.clone(), control.clone());
    assert_eq!(
        diagnostic
            .gc_eligibility_for_test(&fixture.binding, orphan)
            .await
            .expect("Reset generation eligibility"),
        "Eligible",
    );
    let mut lifecycle = SupervisedMaintenance::start(
        &fixture,
        config.clone(),
        Arc::clone(&fixture.catalog),
        control.clone(),
    );
    lifecycle.hold_next_attempt();
    lifecycle.schedule_once().await;
    tokio::time::timeout(Duration::from_secs(30), async {
        tokio::select! {
            () = control.wait_for_delete() => {}
            () = lifecycle.worker_observer.wait_for_held_attempt_for_test() => {
                let tasks: Vec<(String, String)> = sqlx::query_as(
                    "SELECT state, strategy FROM vala.forge_tasks \
                     WHERE data_tenant_id = $1 ORDER BY created_at, task_id",
                )
                .bind(fixture.tenant.as_uuid())
                .fetch_all(fixture.operator_pool.pool())
                .await
                .expect("maintenance task diagnostics");
                panic!(
                    "attempt returned before orphan delete: errors={:?}, deletes={}, tasks={tasks:?}",
                    lifecycle.worker_observer.returned_errors(),
                    control.delete_calls(),
                );
            }
        }
    })
    .await
    .expect("orphan delete boundary must be reached within the production bound");
    let (successor_owner, successor_token) = steal_forge_lease(&fixture).await;
    control.reject_paused_delete();
    lifecycle.wait_for_held_attempt().await;
    assert!(!lifecycle.worker_observer.returned_errors().is_empty());
    lifecycle.stop_worker().await;
    lifecycle.shutdown().await;
    let stopped_task = expire_stopped_claim(&fixture).await;
    assert!(control.stat(orphan).await.is_ok());
    assert_eq!(
        fixture.operation_count("forge.orphan_gc.committed").await,
        0
    );
    let lease_key = forge_lease_key(
        fixture.tenant,
        &fixture.binding.logical_namespace,
        &fixture.binding.table_name,
    );
    assert!(
        vala_sql::queries::maintenance_leases::release_lease_fenced(
            &fixture.operator_pool,
            &lease_key,
            successor_owner,
            successor_token,
        )
        .await
        .expect("successor release")
    );
    let mut recovery = SupervisedMaintenance::start_default(&fixture, config);
    recovery.run_one_success().await;
    assert_eq!(
        recovery.worker_observer.completed_tasks(),
        vec![stopped_task]
    );
    recovery.shutdown().await;
    assert!(fixture.staging.stat(orphan).await.is_err());
    let terminal = fixture.operation_count("forge.orphan_gc.committed").await
        + fixture.operation_count("forge.orphan_gc.recovered").await;
    assert_eq!(terminal, 1);
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Tests authoritative takeover evidence and stale-owner fence loss.
///
/// Steps:
/// 1. Acquire one real Forge table lease.
/// 2. Expire and replace it through the authoritative acquisition statement.
/// 3. Assert takeover evidence and stale-owner fence loss are both exact.
///
/// # Errors
///
/// The test panics when lease takeover or stale-worker fencing differs from the
/// durable maintenance-lease row.
async fn lease_theft_records_fence_loss_metric() {
    let server = start_maintenance_server().await;
    let fixture = seed_forge_group(&server, "lease_metric_rows").await;
    let lease_key = forge_lease_key(
        fixture.tenant,
        &fixture.binding.logical_namespace,
        &fixture.binding.table_name,
    );
    let mut stale = ForgeLease::acquire(
        &fixture.operator_pool,
        lease_key.clone(),
        uuid::Uuid::now_v7(),
        fixture.config.lease_ttl,
    )
    .await
    .expect("stale lease query")
    .expect("stale owner acquires lease");
    let (successor_owner, successor_token) = steal_forge_lease(&fixture).await;
    assert!(
        matches!(
            stale.require_fence(&fixture.operator_pool).await,
            Err(vala_bifrost_redux::forge::ForgeError::FenceLost { .. })
        ),
        "the replaced owner must observe exact fence loss"
    );
    assert!(
        vala_sql::queries::maintenance_leases::release_lease_fenced(
            &fixture.operator_pool,
            &lease_key,
            successor_owner,
            successor_token,
        )
        .await
        .expect("successor release")
    );
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Reconciles snapshot expiry after lease takeover while preserving the live
/// head and the retention-selected snapshot head.
///
/// Two real compaction commits create durable Iceberg snapshots. Expiry is
/// paused after the catalog applies its commit; a successor takes the table
/// lease and fences the stale worker before its terminal audit. The recovery
/// tick then reconciles the already-applied expiry exactly once and confirms
/// that the current snapshot remains readable while the retained head is not
/// removed by replay.
///
/// # Errors
///
/// The test panics if compaction, takeover, fencing, reconciliation, or the
/// snapshot-head assertions fail. The paused expiry task is joined before
/// lease release, so cancellation remains bounded to this test.
async fn forge_expiry_takeover_reconciles_current_and_retained_heads() {
    let server = start_maintenance_server().await;
    let fixture = seed_forge_group(&server, "maintenance_expiry_rows").await;

    commit_staging_snapshot(&fixture).await;
    fixture.append_forge_file(2).await;
    fixture.append_forge_file(3).await;
    commit_staging_snapshot(&fixture).await;
    let before = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("table before expiry");
    assert!(
        before.metadata().snapshots().len() >= 2,
        "expiry interleaving requires retained history"
    );
    let newest_snapshot_ms = before
        .metadata()
        .snapshots()
        .map(|snapshot| snapshot.timestamp_ms())
        .max()
        .expect("retained snapshot timestamp");
    server
        .forge_clock()
        .set(
            chrono::DateTime::from_timestamp_millis(newest_snapshot_ms + 2)
                .expect("snapshot timestamp is UTC-representable"),
        )
        .expect("advance Forge beyond retention");

    let mut config = fixture.config.clone();
    config.snapshot_retention = Duration::from_millis(1);
    config.min_files = 3;
    config.max_files_per_bin = 3;
    config.max_files_per_tick = 3;
    config.maintenance_trigger_snapshot_count = 1;
    config.maintenance_trigger_interval = Duration::from_nanos(1);
    let converged = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("converged expiry table");
    let snapshots_before_expiry = converged.metadata().snapshots().len();
    let active_watermark = converged
        .metadata()
        .current_snapshot_id()
        .expect("converged current snapshot");
    let control = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
    control.pause_after_snapshot_removal();
    let mut lifecycle = SupervisedMaintenance::start(
        &fixture,
        config.clone(),
        control.clone(),
        Arc::clone(&fixture.object_store),
    );
    lifecycle.hold_next_attempt();
    lifecycle.schedule_once().await;
    tokio::time::timeout(Duration::from_secs(30), control.wait_for_commit())
        .await
        .expect("snapshot-removal catalog boundary");
    let (successor_owner, successor_token) = steal_forge_lease(&fixture).await;
    control.reject_paused_commit();
    lifecycle.wait_for_held_attempt().await;
    assert!(!lifecycle.worker_observer.returned_errors().is_empty());
    lifecycle.stop_worker().await;
    lifecycle.shutdown().await;
    let stopped_task = expire_stopped_claim(&fixture).await;
    assert_eq!(
        fixture
            .operation_count("forge.snapshot_expire.committed")
            .await,
        0
    );

    let lease_key = format!(
        "forge:table:{}:{}:{}",
        fixture.tenant, fixture.binding.logical_namespace, fixture.binding.table_name
    );
    assert!(
        vala_sql::queries::maintenance_leases::release_lease_fenced(
            &fixture.operator_pool,
            &lease_key,
            successor_owner,
            successor_token,
        )
        .await
        .expect("successor release")
    );
    let mut recovery = SupervisedMaintenance::start_default(&fixture, config);
    recovery.run_one_success().await;
    assert_eq!(
        recovery.worker_observer.completed_tasks(),
        vec![stopped_task]
    );
    recovery.shutdown().await;
    assert_eq!(
        fixture
            .operation_count("forge.snapshot_expire.recovered")
            .await,
        1
    );
    let after = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("table after expiry recovery");
    assert_eq!(
        after.metadata().snapshots().len(),
        snapshots_before_expiry,
        "recovery preserves both the current head and the retained ref head",
    );
    assert!(after.metadata().snapshot_by_id(active_watermark).is_some());
    assert!(after.metadata().current_snapshot_id().is_some());
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Proves a young prepared expiry blocks both a second expiry and orphan GC.
///
/// # Panics
///
/// Panics when projection setup fails, the pass does not report pending work,
/// a second expiry terminal appears, or GC deletes the aged sentinel.
async fn forge_expiry_pending_blocks_new_expiry_and_gc() {
    let server = start_maintenance_server().await;
    let fixture = seed_forge_group(&server, "maintenance_expiry_pending").await;
    commit_staging_snapshot(&fixture).await;
    fixture.append_forge_file(2).await;
    fixture.append_forge_file(3).await;
    commit_staging_snapshot(&fixture).await;
    let table = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("expiry table");
    let current = table
        .metadata()
        .current_snapshot_id()
        .expect("current snapshot");
    let resource = format!(
        "bifrost://{}/{}/{}",
        fixture.tenant, fixture.binding.table_ref.namespace, fixture.binding.table_ref.name
    );
    let detail = AuditDetail::ForgeSnapshotExpire {
        operation_id: uuid::Uuid::now_v7(),
        phase: ForgeSnapshotExpirePhase::Prepared,
        group: resource,
        base_metadata_location: StoragePath::new(
            table
                .metadata_location_result()
                .expect("metadata location")
                .to_owned(),
        )
        .expect("storage path"),
        current_snapshot_id: Some(current),
        retained_ref_heads: Vec::new(),
        cutoff_ms: server
            .forge_clock()
            .now()
            .expect("clock")
            .timestamp_millis(),
        selected_snapshot_ids: vec![current],
    };
    let lease_key = forge_lease_key(
        fixture.tenant,
        &fixture.binding.logical_namespace,
        &fixture.binding.table_name,
    );
    let mut lease = ForgeLease::acquire(
        &fixture.operator_pool,
        lease_key,
        uuid::Uuid::now_v7(),
        fixture.config.lease_ttl,
    )
    .await
    .expect("lease query")
    .expect("fixture lease");
    fixture
        .forge
        .append_expiry_transition_for_test(
            &mut lease,
            fixture.tenant,
            &detail,
            "forge.snapshot_expire.prepared",
        )
        .await
        .expect("prepared expiry");
    lease
        .release(&fixture.operator_pool)
        .await
        .expect("release fixture lease");

    let orphan = format!(
        "{}/data/pending-expiry.parquet",
        fixture.binding.object_prefix
    );
    fixture
        .staging
        .write(&orphan, Buffer::from(vec![1_u8]))
        .await
        .expect("GC sentinel");
    let modified = fixture
        .staging
        .stat(&orphan)
        .await
        .expect("sentinel metadata")
        .last_modified()
        .expect("sentinel age")
        .into_inner()
        .as_millisecond();
    server
        .forge_clock()
        .set(chrono::DateTime::from_timestamp_millis(modified + 100).expect("sentinel timestamp"))
        .expect("age sentinel");
    let mut config = fixture.config.clone();
    config.min_files = 3;
    config.orphan_gc_ttl = Duration::from_millis(1);
    config.snapshot_retention = Duration::from_millis(1);
    config.maintenance_trigger_snapshot_count = 1;
    config.maintenance_trigger_interval = Duration::from_nanos(1);
    let mut lifecycle = SupervisedMaintenance::start_default(&fixture, config);
    lifecycle.run_one_success().await;
    lifecycle.shutdown().await;
    assert_eq!(
        fixture
            .operation_count("forge.snapshot_expire.prepared")
            .await,
        1,
    );
    assert!(fixture.staging.stat(&orphan).await.is_ok());
    assert_eq!(
        fixture
            .operation_count("forge.snapshot_expire.committed")
            .await,
        0
    );
    server.shutdown().await.expect("server shutdown");
}
