//! Shared fixtures for the forge modules.
//!
//! Every item here is used by more than one sibling module. A helper
//! used by exactly one module lives in that module instead. Contains
//! no tests.

use async_trait::async_trait;
use iceberg::Catalog;
use iceberg::transaction::{ApplyTransactionAction, Transaction};
use opendal::{Buffer, Entry, Metadata};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::forge::{
    Forge, ForgeError, ForgeLease, ForgeObjectStore, ForgeSchedulerTrigger, ForgeWorker,
    ForgeWorkerCompletionObserver, ForgeWorkerConfig, forge_lease_key,
};
use vala_bifrost_redux::maintenance::StagingFilePublisher;
use vala_sql::queries::forge_tasks::ForgeTasks;
use wyrd_server::BifrostTarget;
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::{CommitUncertaintyCatalog, ForgeObjectStoreControl};

/// Stable scheduler lease identity reused across reconstructed supervisors.
pub(crate) const INTERLEAVING_SCHEDULER_OWNER: u128 = 0x0198_39f4_2b51_7000_8000_0000_0000_0001;

/// Start the real dependency fixture without any background Forge role supervisor.
///
/// The test-owned [`SupervisedForge`] is therefore the only scheduler and
/// worker lifecycle allowed to plan or claim this fixture's durable tasks.
///
/// # Panics
///
/// Panics when the in-process server resources cannot start or unexpectedly
/// expose a bound endpoint that would imply a background server supervisor.
pub(crate) async fn start_engine_fixture_server() -> WyrdTestServer {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .with_forge_process_role_for_test(BifrostTarget::Server)
        .start_in_process()
        .await
        .expect("in-process Forge dependency server");
    assert!(
        server.base_url().is_none(),
        "engine fixture must not start a background server role supervisor"
    );
    server
}

/// Production scheduler and worker supervisors retained for one interleaving case.
pub(crate) struct SupervisedForge {
    /// Forge graph shared by the real scheduler and worker loops.
    pub(crate) forge: Arc<Forge>,
    /// Privileged SQL pool used only for bounded lifecycle diagnostics.
    pub(crate) operator_pool: vala_sql::OperatorPool,
    /// Publisher paired with the scheduler's production hint inbox.
    pub(crate) publisher: StagingFilePublisher,
    /// Passive wake-up for deterministic production scheduler passes.
    pub(crate) scheduler_trigger: ForgeSchedulerTrigger,
    /// Passive observation of returned and successful worker attempts.
    pub(crate) worker_observer: ForgeWorkerCompletionObserver,
    /// Cancellation boundary for the production scheduler loop.
    pub(crate) scheduler_stop: CancellationToken,
    /// Cancellation boundary observed by active worker execution.
    pub(crate) worker_stop: CancellationToken,
    /// Running production scheduler supervisor.
    pub(crate) scheduler_task: JoinHandle<Result<(), ForgeError>>,
    /// Running production worker supervisor.
    pub(crate) worker_task: Option<JoinHandle<Result<(), ForgeError>>>,
}

impl SupervisedForge {
    /// Start one production scheduler and one production worker over explicit seams.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot construct its validated worker graph.
    pub(crate) fn start(
        fixture: &wyrd_testing::bifrost::ForgeFixture,
        config: vala_bifrost_redux::forge::ForgeConfig,
        catalog: Arc<dyn Catalog>,
        object_store: Arc<dyn ForgeObjectStore>,
    ) -> Self {
        let scheduler_trigger = ForgeSchedulerTrigger::with_owner_for_test(uuid::Uuid::from_u128(
            INTERLEAVING_SCHEDULER_OWNER,
        ));
        let worker_observer = ForgeWorkerCompletionObserver::new();
        let (forge, publisher) = fixture.context_with_worker_supervision(
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
        .expect("validated interleaving worker");
        let scheduler_stop = CancellationToken::new();
        let worker_stop = CancellationToken::new();
        let scheduler_task = tokio::spawn({
            let forge = Arc::clone(&forge);
            let stop = scheduler_stop.clone();
            async move { forge.run(stop).await }
        });
        let worker_task = tokio::spawn({
            let stop = worker_stop.clone();
            async move { worker.run(stop).await }
        });
        Self {
            forge,
            operator_pool: fixture.operator_pool.clone(),
            publisher,
            scheduler_trigger,
            worker_observer,
            scheduler_stop,
            worker_stop,
            scheduler_task,
            worker_task: Some(worker_task),
        }
    }

    /// Start supervised loops over the fixture's ordinary catalog and object store.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot construct its validated worker graph.
    pub(crate) fn start_default(
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

    /// Request and await one pass from the running production scheduler.
    ///
    /// # Panics
    ///
    /// Panics when the scheduler does not return within the deterministic bound.
    pub(crate) async fn schedule_once(&self) {
        let expected = self.scheduler_trigger.completed_passes().saturating_add(1);
        self.scheduler_trigger.request_pass();
        tokio::time::timeout(
            Duration::from_secs(30),
            self.scheduler_trigger.wait_for_passes_at_least(expected),
        )
        .await
        .expect("production Forge scheduler pass bound");
    }

    /// Schedule one pass and await at least one successful production worker task.
    ///
    /// # Panics
    ///
    /// Panics when the scheduler or worker misses its deterministic bound.
    pub(crate) async fn run_one_success(&mut self) {
        let expected = self.worker_observer.completed().saturating_add(1);
        let expected_attempt = self.worker_observer.attempts().saturating_add(1);
        let expected_errors = self.worker_observer.returned_errors().len();
        self.hold_next_attempt();
        self.schedule_once().await;
        if tokio::time::timeout(
            Duration::from_secs(30),
            self.worker_observer.wait_for_held_attempt_for_test(),
        )
        .await
        .is_err()
        {
            let tasks: Vec<(String, String, i64, i64)> = sqlx::query_as(
                "SELECT state, strategy, estimated_files, estimated_bytes \
                 FROM vala.forge_tasks ORDER BY created_at, task_id",
            )
            .fetch_all(self.operator_pool.pool())
            .await
            .expect("Forge task timeout diagnostics");
            panic!(
                "production Forge worker completion bound: completed={}, attempts={}, errors={:?}, tasks={tasks:?}",
                self.worker_observer.completed(),
                self.worker_observer.attempts(),
                self.worker_observer.returned_errors(),
            );
        }
        assert_eq!(self.worker_observer.attempts(), expected_attempt);
        assert_eq!(
            self.worker_observer.returned_errors().len(),
            expected_errors
        );
        self.stop_worker().await;
        assert_eq!(self.worker_observer.completed(), expected);
    }

    /// Arm a passive barrier after the next production worker attempt returns.
    pub(crate) fn hold_next_attempt(&self) {
        self.worker_observer.hold_after_next_attempt_for_test();
    }

    /// Wait until the armed production worker attempt has returned and is held.
    ///
    /// # Panics
    ///
    /// Panics when no worker attempt returns within the deterministic bound.
    pub(crate) async fn wait_for_held_attempt(&self) {
        tokio::time::timeout(
            Duration::from_secs(30),
            self.worker_observer.wait_for_held_attempt_for_test(),
        )
        .await
        .expect("production Forge worker returned-attempt bound");
    }

    /// Cancel and join the worker before it can retry a returned failure.
    ///
    /// # Panics
    ///
    /// Panics when the worker misses its bounded shutdown or exits unexpectedly.
    pub(crate) async fn stop_worker(&mut self) {
        self.worker_stop.cancel();
        self.worker_observer.release_held_attempt_for_test();
        let task = self.worker_task.take().expect("worker is stopped once");
        tokio::time::timeout(Duration::from_secs(30), task)
            .await
            .expect("production Forge worker shutdown bound")
            .expect("production Forge worker task")
            .expect("production Forge worker shutdown");
    }

    /// Cancel and join both production supervisors.
    ///
    /// # Panics
    ///
    /// Panics when either production loop misses its bounded shutdown.
    pub(crate) async fn shutdown(mut self) {
        self.scheduler_stop.cancel();
        if self.worker_task.is_some() {
            self.stop_worker().await;
        }
        tokio::time::timeout(Duration::from_secs(30), self.scheduler_task)
            .await
            .expect("production Forge scheduler shutdown bound")
            .expect("production Forge scheduler task")
            .expect("production Forge scheduler shutdown");
    }
}

/// Pauses the rewrite fence boundary so a test can take over the lease before
/// the production staging PUT is attempted.
#[derive(Debug)]
pub(crate) struct OutputPutBarrier {
    /// Real object-store seam used for all source reads and cleanup deletes.
    pub(crate) inner: Arc<ForgeObjectStoreControl>,
    /// Signals that the next output reached the pre-fence boundary.
    pub(crate) reached: tokio::sync::Notify,
    /// Retains the reached signal for waiters that start after the callback.
    pub(crate) reached_flag: AtomicBool,
    /// Counts output boundaries so the first output boundary is paused.
    pub(crate) calls: std::sync::atomic::AtomicUsize,
    /// Releases the paused pre-fence boundary.
    pub(crate) release: tokio::sync::Notify,
}

impl OutputPutBarrier {
    /// Wrap a real Forge object-store control with one output barrier.
    pub(crate) fn new(inner: Arc<ForgeObjectStoreControl>) -> Arc<Self> {
        Arc::new(Self {
            inner,
            reached: tokio::sync::Notify::new(),
            reached_flag: AtomicBool::new(false),
            calls: std::sync::atomic::AtomicUsize::new(0),
            release: tokio::sync::Notify::new(),
        })
    }

    /// Wait until the stale rewrite reaches its first output boundary.
    pub(crate) async fn wait_until_reached(&self) {
        while !self.reached_flag.load(Ordering::Acquire) {
            self.reached.notified().await;
        }
    }

    /// Resume the stale rewrite after the successor acquires the lease.
    pub(crate) fn release(&self) {
        self.release.notify_waiters();
    }
}

#[async_trait]
impl vala_bifrost_redux::forge::ForgeObjectStore for OutputPutBarrier {
    async fn before_output_put(&self, _path: &str) -> opendal::Result<()> {
        if self.calls.fetch_add(1, Ordering::AcqRel) != 0 {
            return Ok(());
        }
        self.reached_flag.store(true, Ordering::Release);
        self.reached.notify_waiters();
        self.release.notified().await;
        Ok(())
    }

    async fn read(&self, path: &str) -> opendal::Result<Buffer> {
        self.inner.read(path).await
    }

    async fn read_range(&self, path: &str, range: std::ops::Range<u64>) -> opendal::Result<Buffer> {
        self.inner.read_range(path, range).await
    }

    async fn list(&self, prefix: &str) -> opendal::Result<Vec<Entry>> {
        self.inner.list(prefix).await
    }

    async fn stat(&self, path: &str) -> opendal::Result<Metadata> {
        self.inner.stat(path).await
    }

    async fn delete(&self, path: &str) -> opendal::Result<()> {
        self.inner.delete(path).await
    }
}

pub(crate) async fn steal_forge_lease(
    fixture: &wyrd_testing::bifrost::ForgeFixture,
) -> (uuid::Uuid, i64) {
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
    (owner, token.fencing_token)
}

/// Reclaims the exact running claim left by a fully stopped worker process.
///
/// This test seam expires the persisted deadline after the prior worker has
/// joined, invokes the production bounded reclaim transaction, verifies the
/// attempt was consumed, then advances only the persisted eligibility clock.
/// The successor still uses the production claim transaction and executes a
/// fresh attempt, without making wall-clock sleeps part of interleaving tests.
///
/// # Panics
///
/// Panics unless exactly one running task owned by one durable attempt exists
/// for the fixture tenant, or unless that exact identity changes before expiry.
pub(crate) async fn expire_stopped_running_claim(
    fixture: &wyrd_testing::bifrost::ForgeFixture,
) -> uuid::Uuid {
    let running: Option<(uuid::Uuid, uuid::Uuid, uuid::Uuid)> = sqlx::query_as(
        "SELECT task_id, attempt_id, claimed_by FROM vala.forge_tasks \
         WHERE data_tenant_id = $1 AND state = 'running'",
    )
    .bind(fixture.tenant.as_uuid())
    .fetch_optional(fixture.operator_pool.pool())
    .await
    .expect("stopped running Forge claim query");
    let Some((task_id, attempt_id, owner)) = running else {
        let (task_id, eligible): (uuid::Uuid, bool) = sqlx::query_as(
            "SELECT task_id,next_eligible_at<=statement_timestamp() FROM vala.forge_tasks WHERE data_tenant_id=$1 AND state='retryable' ORDER BY updated_at DESC LIMIT 1",
        )
        .bind(fixture.tenant.as_uuid())
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("worker-settled retryable Forge task");
        if !eligible {
            sqlx::query(
                "UPDATE vala.forge_tasks SET next_eligible_at=statement_timestamp()-interval '1 millisecond',ready_at=statement_timestamp()-interval '1 millisecond' WHERE task_id=$1",
            )
            .bind(task_id)
            .execute(fixture.operator_pool.pool())
            .await
            .expect("advance worker-settled task eligibility");
        }
        return task_id;
    };
    let expired: uuid::Uuid = sqlx::query_scalar(
        "UPDATE vala.forge_tasks \
         SET claim_expires_at = statement_timestamp() - interval '1 millisecond' \
         WHERE task_id = $1 AND attempt_id = $2 AND claimed_by = $3 \
           AND state = 'running' RETURNING task_id",
    )
    .bind(task_id)
    .bind(attempt_id)
    .bind(owner)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("expire exact stopped Forge claim");
    assert_eq!(expired, task_id, "only the stopped claim may expire");
    assert_eq!(
        ForgeTasks::new(fixture.operator_pool.clone())
            .reclaim_expired_attempts(1)
            .await
            .expect("production bounded reclaim"),
        vec![(task_id, attempt_id)],
        "reclaim returns the exact stopped attempt"
    );
    let reclaimed: (String, i32, bool) = sqlx::query_as(
        "SELECT state,attempt_count,next_eligible_at>statement_timestamp() FROM vala.forge_tasks WHERE task_id=$1",
    )
    .bind(task_id)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("persisted reclaimed task");
    assert_eq!(reclaimed.0, "retryable");
    assert!(reclaimed.1 > 0, "reclaim consumes one attempt");
    assert!(reclaimed.2, "reclaim persists future eligibility");
    sqlx::query(
        "UPDATE vala.forge_tasks SET next_eligible_at=statement_timestamp()-interval '1 millisecond',ready_at=statement_timestamp()-interval '1 millisecond' WHERE task_id=$1",
    )
    .bind(task_id)
    .execute(fixture.operator_pool.pool())
    .await
    .expect("advance reclaimed task eligibility");
    task_id
}

/// Fold exactly the fixture's two staged inputs without consuming a live rewrite.
///
/// The real scheduler still runs the complete production stage order. Limiting
/// the per-tick file budget to the two staged inputs deliberately exhausts the
/// shared budget before live replacement, leaving the resulting snapshot for a
/// test that needs to control that replacement separately.
///
/// # Panics
///
/// Panics when the production-shaped tick cannot compact the fixture's exact
/// staged pair or unexpectedly commits a live replacement.
pub(crate) async fn fold_staged_pair_without_live_replacement(
    fixture: &wyrd_testing::bifrost::ForgeFixture,
) {
    let mut config = fixture.config.clone();
    config.max_files_per_bin = 2;
    config.max_files_per_tick = 2;
    let committed = fixture
        .operation_count("forge.file_compact.committed")
        .await;
    let live = fixture
        .operation_count("forge.iceberg_rewrite.committed")
        .await;
    let mut lifecycle = SupervisedForge::start_default(fixture, config);
    lifecycle.run_one_success().await;
    lifecycle.shutdown().await;
    assert_eq!(
        fixture
            .operation_count("forge.file_compact.committed")
            .await,
        committed + 1
    );
    assert_eq!(
        fixture
            .operation_count("forge.iceberg_rewrite.committed")
            .await,
        live
    );
}

/// Advance the fixture's shared manual Forge clock beyond short uncertainty windows.
///
/// Live-rewrite recovery is intentionally based on Forge wall time rather than
/// elapsed Tokio time, so prepared-operation fixtures advance this control
/// explicitly to a full second beyond wall time before asking a fresh scheduler
/// to classify their state. The margin exceeds both Forge's millisecond clock
/// storage and PostgreSQL's microsecond audit timestamps.
///
/// # Panics
///
/// Panics when a checked future UTC instant cannot be represented or the
/// test-owned monotonic clock cannot reach it.
pub(crate) fn advance_forge_clock_past_uncertainty(server: &WyrdTestServer) {
    let future = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::seconds(1))
        .expect("future Forge clock instant is representable");
    server
        .forge_clock()
        .set(future)
        .expect("advance Forge clock beyond uncertainty window");
}

/// Build a recovery tick configuration that cannot start a two-file replacement.
///
/// Reset and recovered-state tests inspect one terminal transition. Their
/// scheduler tick must still execute production reconciliation, but a
/// three-file minimum keeps the next eligible two-file live rewrite for a later test step;
/// a one-year snapshot window prevents unrelated expiry commits from consuming
/// an uncertainty-injection catalog seam.
pub(crate) fn recovery_config_without_live_replacement(
    fixture: &wyrd_testing::bifrost::ForgeFixture,
) -> vala_bifrost_redux::forge::ForgeConfig {
    let mut config = fixture.config.clone();
    config.min_files = 3;
    config.max_files_per_bin = 3;
    config.max_files_per_tick = 3;
    config.uncertainty_bound = Duration::from_micros(1);
    config.snapshot_retention = Duration::from_secs(31_536_000);
    config
}

/// Build two eligible live files, then discover one exact replacement through
/// the supplied catalog graph.
pub(crate) async fn live_replacement_context(
    fixture: &wyrd_testing::bifrost::ForgeFixture,
    control: Arc<CommitUncertaintyCatalog>,
) -> (
    Arc<vala_bifrost_redux::forge::Forge>,
    iceberg::table::Table,
    vala_bifrost_redux::forge::IcebergTablePlan,
    vala_bifrost_redux::forge::ForgeLease,
) {
    fold_staged_pair_without_live_replacement(fixture).await;
    fixture.append_forge_file(2).await;
    fixture.append_forge_file(3).await;
    fold_staged_pair_without_live_replacement(fixture).await;

    let context = fixture.context_with_catalog(fixture.config.clone(), control.clone());
    let table = control
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("wrapped catalog table");
    let plan = context
        .discover_live_rewrites_for_test(
            &fixture.binding,
            &table,
            chrono::NaiveDate::from_ymd_opt(2026, 7, 15)
                .expect("current day")
                .and_hms_opt(0, 0, 0)
                .expect("midnight")
                .and_utc(),
        )
        .await
        .expect("wrapped catalog live rewrite discovery");
    assert!(
        plan.groups_for_test()
            .iter()
            .any(|group| group.files_for_test().len() >= 2),
        "fixture must produce an eligible live replacement group"
    );
    let lease = ForgeLease::acquire(
        &fixture.operator_pool,
        forge_lease_key(
            fixture.tenant,
            &fixture.binding.logical_namespace,
            &fixture.binding.table_name,
        ),
        uuid::Uuid::now_v7(),
        fixture.config.lease_ttl,
    )
    .await
    .expect("live replacement lease query")
    .expect("live replacement lease");
    (context, table, plan, lease)
}

/// Read the exact prepared output paths from durable audit state.
pub(crate) async fn prepared_live_output_paths(
    fixture: &wyrd_testing::bifrost::ForgeFixture,
) -> Vec<String> {
    let mut conn = fixture
        .vala
        .tenant_conn(fixture.tenant)
        .await
        .expect("live replacement audit tenant connection");
    let detail: String = sqlx::query_scalar(
        "SELECT detail FROM vala.audit_outbox \
         WHERE data_tenant_id = wyrd.current_tenant() \
           AND resource = $1 \
           AND operation = 'forge.iceberg_rewrite.prepared' \
         ORDER BY seq DESC LIMIT 1",
    )
    .bind(format!(
        "bifrost://{}/{}/{}",
        fixture.tenant, fixture.binding.logical_namespace, fixture.binding.table_name
    ))
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("prepared live replacement audit detail");
    let detail: serde_json::Value =
        serde_json::from_str(&detail).expect("prepared audit detail JSON");
    detail["output_paths"]
        .as_array()
        .expect("prepared audit output paths")
        .iter()
        .map(|path| {
            path.as_str()
                .expect("prepared output path string")
                .to_owned()
        })
        .collect()
}

/// Assert every exact prepared output remains in the real object store.
pub(crate) async fn assert_prepared_live_outputs_exist(
    fixture: &wyrd_testing::bifrost::ForgeFixture,
) -> Vec<String> {
    let paths = prepared_live_output_paths(fixture).await;
    assert!(
        !paths.is_empty(),
        "prepared replacement must persist at least one output path"
    );
    for path in &paths {
        let object_path = path
            .find(&fixture.binding.object_prefix)
            .map(|offset| &path[offset..])
            .expect("prepared output path belongs to the fixture table");
        fixture
            .staging
            .stat(object_path)
            .await
            .unwrap_or_else(|error| panic!("prepared output {object_path} is absent: {error}"));
    }
    paths
}

/// Read the exact live data-file membership of the current Iceberg snapshot.
pub(crate) async fn current_live_paths(table: &iceberg::table::Table) -> BTreeSet<String> {
    let snapshot = table
        .metadata()
        .current_snapshot()
        .expect("live replacement table snapshot");
    let manifest_list = table
        .manifest_list_reader(snapshot)
        .load()
        .await
        .expect("current manifest list");
    let mut paths = BTreeSet::new();
    for manifest_file in manifest_list.entries() {
        let manifest = manifest_file
            .load_manifest(table.file_io())
            .await
            .expect("current manifest");
        paths.extend(
            manifest
                .entries()
                .iter()
                .filter(|entry| entry.is_alive())
                .map(|entry| entry.file_path().to_owned()),
        );
    }
    paths
}

/// Restore a target size that classifies the current live files as healthy.
///
/// Prepared-reset fixtures temporarily raise the target to force their exact
/// two-file rewrite. Restoring it before scheduler recovery prevents an
/// unrelated replacement task from taking priority over maintenance while
/// retaining the same current snapshot and Prepared evidence.
///
/// # Panics
///
/// Panics when live membership, object metadata, or the property commit fails,
/// or when the fixture files cannot share one healthy target tolerance.
pub(crate) async fn restore_healthy_live_target(fixture: &wyrd_testing::bifrost::ForgeFixture) {
    let table = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("healthy-target live table");
    let mut sizes = Vec::new();
    for catalog_path in current_live_paths(&table).await {
        let object_path = catalog_path
            .find(&fixture.binding.object_prefix)
            .map(|offset| &catalog_path[offset..])
            .expect("healthy-target live path belongs to fixture");
        sizes.push(
            fixture
                .staging
                .stat(object_path)
                .await
                .expect("healthy-target live object")
                .content_length(),
        );
    }
    let minimum = *sizes.iter().min().expect("fixture has live files");
    let target = *sizes.iter().max().expect("fixture has live files");
    assert!(
        minimum.saturating_mul(4) >= target.saturating_mul(3),
        "fixture live files must share one healthy tolerance: {sizes:?}"
    );
    let action = Transaction::new(&table).update_table_properties().set(
        "write.target-file-size-bytes".to_owned(),
        target.to_string(),
    );
    ApplyTransactionAction::apply(action, Transaction::new(&table))
        .expect("healthy-target property update")
        .commit(fixture.catalog.as_ref())
        .await
        .expect("healthy-target property commit");
}

/// Assert that one replacement snapshot directly advances the selected plan base.
pub(crate) fn assert_single_snapshot_advance(table: &iceberg::table::Table, base_snapshot_id: i64) {
    let current = table
        .metadata()
        .current_snapshot()
        .expect("replacement current snapshot");
    assert_ne!(current.snapshot_id(), base_snapshot_id);
    assert_eq!(current.parent_snapshot_id(), Some(base_snapshot_id));
}

/// Read the typed live-replacement audit payloads in durable transition order.
pub(crate) async fn live_rewrite_details(
    fixture: &wyrd_testing::bifrost::ForgeFixture,
) -> Vec<serde_json::Value> {
    let mut conn = fixture
        .vala
        .tenant_conn(fixture.tenant)
        .await
        .expect("live replacement audit tenant connection");
    let details: Vec<String> = sqlx::query_scalar(
        "SELECT detail FROM vala.audit_outbox \
         WHERE data_tenant_id = wyrd.current_tenant() \
           AND resource = $1 \
           AND operation IN ('forge.iceberg_rewrite.prepared', 'forge.iceberg_rewrite.committed') \
         ORDER BY seq",
    )
    .bind(format!(
        "bifrost://{}/{}/{}",
        fixture.tenant, fixture.binding.logical_namespace, fixture.binding.table_name
    ))
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("live replacement audit details");
    details
        .into_iter()
        .map(|detail| serde_json::from_str(&detail).expect("typed live replacement audit JSON"))
        .collect()
}

/// Read every durable live-rewrite state row in deterministic operation order.
pub(crate) async fn live_rewrite_state_rows(
    fixture: &wyrd_testing::bifrost::ForgeFixture,
) -> Vec<(String, uuid::Uuid, serde_json::Value, serde_json::Value)> {
    let mut conn = fixture
        .vala
        .tenant_conn(fixture.tenant)
        .await
        .expect("live replacement state tenant connection");
    sqlx::query_as(
        "SELECT phase, operation_id, prepared_detail, current_detail \
         FROM vala.forge_operation_state \
         WHERE data_tenant_id = wyrd.current_tenant() \
           AND resource = $1 AND family = 'iceberg_rewrite' \
         ORDER BY operation_id",
    )
    .bind(format!(
        "bifrost://{}/{}/{}",
        fixture.tenant, fixture.binding.logical_namespace, fixture.binding.table_name
    ))
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("live replacement state rows")
}

/// Read the phase only when the fixture has exactly one live-rewrite state row.
pub(crate) async fn live_rewrite_state_phase(
    fixture: &wyrd_testing::bifrost::ForgeFixture,
) -> Option<String> {
    let rows = live_rewrite_state_rows(fixture).await;
    match rows.as_slice() {
        [(phase, _, _, _)] => Some(phase.clone()),
        [] => None,
        _ => panic!("fixture must not contain multiple live-rewrite operations: {rows:?}"),
    }
}
