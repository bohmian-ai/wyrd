//! Shared production Forge supervisor lifecycle for the `forge` journey group.
//!
//! Contains no tests. Every item is the smallest real lifecycle a journey needs
//! to make one production scheduler pass and one production worker attempt
//! observable without polling or sleeping.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::catalog::TenantTableBinding;
use vala_bifrost_redux::forge::{
    ForgeError, ForgeRoleReadiness, ForgeSchedulerTrigger, ForgeWorker,
    ForgeWorkerCompletionObserver, ForgeWorkerConfig,
};
use wyrd_server::BifrostTarget;
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::ForgeFixture;

/// Start the real dependency server without a background Forge role supervisor.
///
/// The test-owned [`SupervisedForge`] is then the only scheduler and worker
/// lifecycle able to plan or claim this fixture's durable tasks, which is what
/// makes a single pass observable rather than racing an ambient loop.
///
/// # Panics
///
/// Panics when the in-process server resources cannot start, or when it
/// unexpectedly exposes a bound endpoint that would imply a background server
/// role supervisor.
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

/// Production scheduler and worker supervisors retained for one journey.
pub(crate) struct SupervisedForge {
    /// Privileged SQL pool used only for bounded lifecycle diagnostics.
    operator_pool: vala_sql::OperatorPool,
    /// Passive wake-up for deterministic production scheduler passes.
    scheduler_trigger: ForgeSchedulerTrigger,
    /// Passive observation of returned and successful worker attempts.
    worker_observer: ForgeWorkerCompletionObserver,
    /// Cancellation boundary for the production scheduler loop.
    scheduler_stop: CancellationToken,
    /// Cancellation boundary observed by active worker execution.
    worker_stop: CancellationToken,
    /// Running production scheduler supervisor.
    scheduler_task: JoinHandle<Result<(), ForgeError>>,
    /// Running production worker supervisor.
    worker_task: Option<JoinHandle<Result<(), ForgeError>>>,
}

impl SupervisedForge {
    /// Start one production scheduler and one production worker over the
    /// fixture's own catalog and object store.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot construct its validated worker graph.
    pub(crate) fn start_default(
        fixture: &ForgeFixture,
        config: vala_bifrost_redux::forge::ForgeConfig,
    ) -> Self {
        Self::start_with_seams(
            fixture,
            config,
            Arc::clone(&fixture.catalog),
            Arc::clone(&fixture.object_store),
        )
    }

    /// Start the same supervised pair over explicit catalog and object-store seams.
    ///
    /// Fault-injection journeys wrap one production interface and otherwise
    /// keep the fixture's real graph, so an injected condition is observed by
    /// the production scheduler and worker rather than by a substitute owner.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot construct its validated worker graph.
    pub(crate) fn start_with_seams(
        fixture: &ForgeFixture,
        config: vala_bifrost_redux::forge::ForgeConfig,
        catalog: Arc<dyn iceberg::Catalog>,
        object_store: Arc<dyn vala_bifrost_redux::forge::ForgeObjectStore>,
    ) -> Self {
        let scheduler_trigger = ForgeSchedulerTrigger::with_owner_for_test(uuid::Uuid::now_v7());
        let worker_observer = ForgeWorkerCompletionObserver::new();
        let (forge, _publisher) = fixture.context_with_worker_supervision(
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
        .expect("validated journey worker");
        let scheduler_stop = CancellationToken::new();
        let worker_stop = CancellationToken::new();
        let scheduler_task = tokio::spawn({
            let forge = Arc::clone(&forge);
            let stop = scheduler_stop.clone();
            async move { forge.run(stop, ForgeRoleReadiness::detached()).await }
        });
        let worker_task = tokio::spawn({
            let stop = worker_stop.clone();
            async move { worker.run(stop, ForgeRoleReadiness::detached()).await }
        });
        Self {
            operator_pool: fixture.operator_pool.clone(),
            scheduler_trigger,
            worker_observer,
            scheduler_stop,
            worker_stop,
            scheduler_task,
            worker_task: Some(worker_task),
        }
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

    /// Schedule one pass and await exactly one successful production worker
    /// attempt, holding the attempt so no retry can race the assertions.
    ///
    /// # Panics
    ///
    /// Panics when the scheduler or worker misses its deterministic bound, or
    /// when the attempt returned an error.
    pub(crate) async fn run_one_success(&mut self) {
        let expected = self.worker_observer.completed().saturating_add(1);
        let expected_attempt = self.worker_observer.attempts().saturating_add(1);
        let expected_errors = self.worker_observer.returned_errors().len();
        self.worker_observer.hold_after_next_attempt_for_test();
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
            let tables: Vec<(String, String)> =
                sqlx::query_as("SELECT fqn, status FROM vala.bifrost_tables ORDER BY fqn")
                    .fetch_all(self.operator_pool.pool())
                    .await
                    .expect("Forge table diagnostics");
            let demands: Vec<(String, String, i64)> = sqlx::query_as(
                "SELECT table_name, last_source, generation FROM vala.forge_planning_demands \
                 ORDER BY table_name",
            )
            .fetch_all(self.operator_pool.pool())
            .await
            .expect("Forge demand diagnostics");
            let files: Vec<(String, i64, bool)> = sqlx::query_as(
                "SELECT file_path, file_size, compacted FROM vala.file_list ORDER BY file_path",
            )
            .fetch_all(self.operator_pool.pool())
            .await
            .expect("Forge file diagnostics");
            panic!(
                "production Forge worker completion bound: completed={}, attempts={}, errors={:?}, tasks={tasks:?}, tables={tables:?}, demands={demands:?}, files={files:?}",
                self.worker_observer.completed(),
                self.worker_observer.attempts(),
                self.worker_observer.returned_errors(),
            );
        }
        assert_eq!(self.worker_observer.attempts(), expected_attempt);
        assert_eq!(
            self.worker_observer.returned_errors().len(),
            expected_errors,
            "the attempt was expected to succeed: {:?}",
            self.worker_observer.returned_errors()
        );
        self.stop_worker().await;
        assert_eq!(self.worker_observer.completed(), expected);
    }

    /// Schedule one pass and await exactly one *returned error* from the worker.
    ///
    /// The mirror of [`Self::run_one_success`], for the branches whose whole
    /// point is that the attempt does not complete: the worker is stopped
    /// while holding the attempt so the supervisor cannot retry it and blur
    /// what the assertions observe.
    ///
    /// # Panics
    ///
    /// Panics when the scheduler or worker misses its deterministic bound, or
    /// when the attempt unexpectedly succeeded.
    pub(crate) async fn run_one_failure(mut self) -> Self {
        let expected_errors = self
            .worker_observer
            .returned_errors()
            .len()
            .saturating_add(1);
        self.worker_observer.hold_after_next_attempt_for_test();
        self.schedule_once().await;
        tokio::time::timeout(
            Duration::from_secs(30),
            self.worker_observer.wait_for_held_attempt_for_test(),
        )
        .await
        .expect("production Forge worker attempt bound");
        self.stop_worker().await;
        assert_eq!(
            self.worker_observer.returned_errors().len(),
            expected_errors,
            "the attempt was expected to return exactly one error: {:?}",
            self.worker_observer.returned_errors()
        );
        self
    }

    /// Run one failing attempt while `during` drives a seam it is blocked on.
    ///
    /// The variant exists because the deterministic scenarios pause the
    /// production commit at a real catalog seam: the control that decides how
    /// the attempt ends can only run *while* the worker is parked there, so it
    /// cannot be applied before scheduling or after the attempt is held.
    ///
    /// # Panics
    ///
    /// Panics when the scheduler or worker misses its deterministic bound, or
    /// when the attempt unexpectedly succeeded.
    pub(crate) async fn run_one_failure_while<F>(mut self, during: F) -> Self
    where
        F: std::future::Future<Output = ()>,
    {
        let expected_errors = self
            .worker_observer
            .returned_errors()
            .len()
            .saturating_add(1);
        self.worker_observer.hold_after_next_attempt_for_test();
        self.schedule_once().await;
        tokio::time::timeout(Duration::from_secs(30), during)
            .await
            .expect("paused production commit seam bound");
        tokio::time::timeout(
            Duration::from_secs(30),
            self.worker_observer.wait_for_held_attempt_for_test(),
        )
        .await
        .expect("production Forge worker attempt bound");
        self.stop_worker().await;
        assert_eq!(
            self.worker_observer.returned_errors().len(),
            expected_errors,
            "the attempt was expected to return exactly one error: {:?}",
            self.worker_observer.returned_errors()
        );
        self
    }

    /// Borrows the token production worker execution observes as shutdown.
    ///
    /// A drain proof has to cancel *while* an attempt is parked at a real seam
    /// and then keep observing it, which joining the supervisor would prevent.
    pub(crate) fn worker_stop(&self) -> CancellationToken {
        self.worker_stop.clone()
    }

    /// Cancel and join the worker before it can retry a returned attempt.
    ///
    /// # Panics
    ///
    /// Panics when the worker misses its bounded shutdown or exits unexpectedly.
    async fn stop_worker(&mut self) {
        self.worker_stop.cancel();
        self.worker_observer.release_held_attempt_for_test();
        let task = self.worker_task.take().expect("worker is stopped once");
        tokio::time::timeout(Duration::from_secs(30), task)
            .await
            .expect("production Forge worker shutdown bound")
            .expect("production Forge worker task")
            .expect("production Forge worker shutdown");
    }

    /// Borrows the errors production worker attempts returned so far.
    ///
    /// A scenario that deliberately fails an attempt asserts on *which* error
    /// came back, not merely that one did.
    pub(crate) fn returned_errors(&self) -> Vec<String> {
        self.worker_observer.returned_errors()
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

/// Collects the live data-file paths of one table's current snapshot.
///
/// The projection mirrors the catalog's own pinning rule — table-relative
/// suffix rewritten onto the tenant object prefix — so a path here is directly
/// comparable to a `vala.file_list` path.
///
/// # Panics
///
/// Panics when the table, its manifest list, or a manifest cannot be read.
pub(crate) async fn live_data_paths(
    fixture: &ForgeFixture,
    binding: &TenantTableBinding,
) -> BTreeSet<String> {
    let table = iceberg::Catalog::load_table(fixture.catalog.as_ref(), &binding.table_ident())
        .await
        .expect("promotion table load");
    let mut paths = BTreeSet::new();
    let Some(snapshot) = table.metadata().current_snapshot() else {
        return paths;
    };
    let manifests = table
        .manifest_list_reader(snapshot)
        .load()
        .await
        .expect("promotion manifest list");
    for manifest_file in manifests.entries() {
        let manifest = manifest_file
            .load_manifest(table.file_io())
            .await
            .expect("promotion manifest");
        for entry in manifest.entries().iter().filter(|entry| entry.is_alive()) {
            let path = entry.data_file().file_path().to_owned();
            let canonical = path
                .strip_prefix(table.metadata().location())
                .and_then(|suffix| suffix.strip_prefix('/'))
                .map(|suffix| format!("{}/{suffix}", binding.object_prefix))
                .unwrap_or(path);
            paths.insert(canonical);
        }
    }
    paths
}

/// Returns the claim a fully stopped worker abandoned to a fresh attempt.
///
/// The deadline is expired only after the prior worker has joined, and the
/// reclaim itself runs through the production bounded transaction, so the
/// successor executes a real fresh attempt rather than a fabricated one. Only
/// the persisted eligibility clock is then advanced, which keeps wall-clock
/// sleeps out of the proof.
///
/// # Panics
///
/// Panics when no stopped claim or worker-settled retryable task is present.
pub(crate) async fn reclaim_stopped_claim(fixture: &ForgeFixture) {
    let running: Option<(uuid::Uuid, uuid::Uuid, uuid::Uuid)> = sqlx::query_as(
        "SELECT task_id, attempt_id, claimed_by FROM vala.forge_tasks \
         WHERE data_tenant_id = $1 AND state = 'running'",
    )
    .bind(fixture.tenant.as_uuid())
    .fetch_optional(fixture.operator_pool.pool())
    .await
    .expect("stopped running Forge claim query");
    if let Some((task_id, attempt_id, owner)) = running {
        sqlx::query(
            "UPDATE vala.forge_tasks \
             SET claim_expires_at = statement_timestamp() - interval '1 millisecond' \
             WHERE task_id = $1 AND attempt_id = $2 AND claimed_by = $3 AND state = 'running'",
        )
        .bind(task_id)
        .bind(attempt_id)
        .bind(owner)
        .execute(fixture.operator_pool.pool())
        .await
        .expect("expire the exact stopped Forge claim");
        assert_eq!(
            vala_sql::queries::forge_tasks::ForgeTasks::new(fixture.operator_pool.clone())
                .reclaim_expired_attempts(1)
                .await
                .expect("production bounded reclaim"),
            vec![(task_id, attempt_id)],
            "reclaim returns the exact stopped attempt"
        );
    }
    sqlx::query(
        "UPDATE vala.forge_tasks \
         SET next_eligible_at = statement_timestamp() - interval '1 millisecond', \
             ready_at = statement_timestamp() - interval '1 millisecond' \
         WHERE data_tenant_id = $1 AND state = 'retryable'",
    )
    .bind(fixture.tenant.as_uuid())
    .execute(fixture.operator_pool.pool())
    .await
    .expect("advance reclaimed task eligibility");
}
