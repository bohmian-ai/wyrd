//! Distributed product journey for the real Scribe-to-Forge publication path.

use std::sync::Arc;
use std::time::{Duration, Instant};

use arrow::array::{ArrayRef, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use opendal::Buffer;
use secrecy::SecretString;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
use vala_bifrost_redux::forge::{
    ForgeCapacity, ForgeConfig, ForgeEnvelopeSizer, ForgeError, ForgeLease, ForgeScheduler,
    ForgeSchedulerTrigger, ForgeWorker, ForgeWorkerCompletionObserver, ForgeWorkerConfig,
};
use vala_bifrost_redux::maintenance::{StagingFileCommitted, StagingPublishOutcome};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::resources::{
    MIN_SCRATCH_FREE_BYTES, ResourceSource, SystemResourceSnapshot,
};
use vala_sdk::{BifrostGrpcTransport, IngestTransport};
use vala_sql::queries::forge_tasks::ForgeTasks;
use vala_sql::row_types::forge_tasks::{
    FORGE_TASK_PAYLOAD_VERSION, ForgeClaimStrategy, ForgeTaskEstimates, ForgeTaskLane,
    ForgeTaskPlan, ForgeTaskStrategy, ForgeTaskTableIdentity, NewForgeTask,
};
use wyrd_client::WyrdClient;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_server::config::ForgeProcessRole;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{
    AuditDetail, BifrostQueryRequest, ForgeCompactionPhase, FreshnessPolicy, StoragePath,
    VisibilityMode,
};
use wyrd_testing::bifrost::forge_harness::seed_forge_group;
use wyrd_testing::bifrost::{
    BifrostClusterSpec, BifrostTopology, ForgeCausalDiagnosis, ForgeCausalTelemetryReport,
    WyrdTestCluster, shared_process_telemetry_for_test,
};
use wyrd_testing::otlp::RandomTraceGenerator;
use wyrd_testing::{Bootstrap, WyrdTestServer};
use wyrd_tonic::frame_codec::FrameDecoder;
use wyrd_tonic::wyrd::v1 as proto;

/// One deployment and writer-concurrency shape exercised by the journey.
#[derive(Clone, Copy)]
struct Scenario {
    /// Diagnostic name included in assertion failures.
    name: &'static str,
    /// Number of isolated tenants writing concurrently.
    tenants: usize,
}

/// Number of independent flush cycles produced by every pod for every tenant.
///
/// Multiple cycles create independent Scribe files so active-day Forge
/// compaction is exercised instead of leaving a single open-tail file.
const WRITE_CYCLES: usize = 3;

/// Number of spans carried by each OTLP writer request.
const SPANS_PER_WRITE: usize = 6;

/// Stable scheduler identity for maintenance journey fixtures.
const MAINTENANCE_JOURNEY_SCHEDULER_OWNER: u128 = 0x0198_39f4_2b51_7000_8000_0000_0000_0005;

/// Rows sent by each bounded public-ingest request in the convergence journey.
const FORGE_CONVERGENCE_BATCH_ROWS: usize = 5_000;

/// Public-ingest batches durably sealed together while creating small-file debt.
const FORGE_CONVERGENCE_BATCHES_PER_SEAL: usize = 10;

/// Sizes one journey task from the same config-clamped live governor capacity as production.
fn journey_envelope(
    fixture: &wyrd_testing::bifrost::forge_harness::ForgeFixture,
    input_bytes: u64,
    file_count: usize,
) -> vala_sql::row_types::forge_tasks::ForgeTaskEnvelope {
    let mut capacity = ForgeCapacity::try_from(&fixture.config).expect("journey Forge capacity");
    let plan = fixture
        .forge
        .resources_for_test()
        .snapshot()
        .expect("journey resource snapshot")
        .plan;
    capacity.max_memory_bytes = capacity
        .max_memory_bytes
        .min(u64::try_from(plan.elastic_memory_bytes).expect("journey memory capacity"));
    capacity.max_spill_bytes = capacity.max_spill_bytes.min(plan.scratch_limit_bytes);
    capacity.max_parallelism = capacity
        .max_parallelism
        .min(u16::try_from(plan.effective_cpu).unwrap_or(u16::MAX));
    ForgeEnvelopeSizer::size(
        input_bytes,
        file_count,
        fixture.config.max_concurrent_reads,
        capacity,
    )
    .expect("journey executable envelope")
}

/// One bounded production scheduler and worker pair for a lifecycle proof.
struct JourneyMaintenance {
    /// Passive trigger driving deterministic production scheduler passes.
    scheduler_trigger: ForgeSchedulerTrigger,
    /// Observer proving which durable worker strategy completed.
    worker_observer: ForgeWorkerCompletionObserver,
    /// Cancellation boundary for the production scheduler loop.
    scheduler_stop: CancellationToken,
    /// Cancellation boundary for the production worker loop.
    worker_stop: CancellationToken,
    /// Running production scheduler task.
    scheduler_task: JoinHandle<Result<(), ForgeError>>,
    /// Running production worker task, consumed during shutdown.
    worker_task: Option<JoinHandle<Result<(), ForgeError>>>,
}

impl JourneyMaintenance {
    /// Starts production scheduler and worker supervision for one fixture.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot construct the configured production
    /// scheduler or worker.
    fn start(
        fixture: &wyrd_testing::bifrost::ForgeFixture,
        config: vala_bifrost_redux::forge::ForgeConfig,
    ) -> Self {
        let scheduler_trigger = ForgeSchedulerTrigger::with_owner_for_test(uuid::Uuid::from_u128(
            MAINTENANCE_JOURNEY_SCHEDULER_OWNER,
        ));
        let worker_observer = ForgeWorkerCompletionObserver::new();
        let (forge, _) = fixture.context_with_worker_supervision(
            config,
            Arc::clone(&fixture.catalog),
            Arc::clone(&fixture.object_store),
            worker_observer.clone(),
            scheduler_trigger.clone(),
        );
        let worker = ForgeWorker::new(
            Arc::clone(&forge),
            ForgeWorkerConfig::default(),
            uuid::Uuid::now_v7(),
        )
        .expect("validated journey maintenance worker");
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

    /// Runs exactly one completed production worker attempt and returns its strategy.
    ///
    /// # Panics
    ///
    /// Panics when the scheduler or worker misses its bounded completion or the
    /// selected attempt returns an error.
    async fn run_one_success(&mut self) -> ForgeTaskStrategy {
        let expected_passes = self.scheduler_trigger.completed_passes().saturating_add(1);
        let expected_attempts = self.worker_observer.attempts().saturating_add(1);
        let expected_completions = self.worker_observer.completed().saturating_add(1);
        let errors = self.worker_observer.returned_errors().len();
        self.worker_observer.hold_after_next_attempt_for_test();
        self.scheduler_trigger.request_pass();
        tokio::time::timeout(
            Duration::from_secs(30),
            self.scheduler_trigger
                .wait_for_passes_at_least(expected_passes),
        )
        .await
        .expect("journey maintenance scheduler pass bound");
        tokio::time::timeout(
            Duration::from_secs(30),
            self.worker_observer.wait_for_held_attempt_for_test(),
        )
        .await
        .expect("journey maintenance worker attempt bound");
        assert_eq!(self.worker_observer.attempts(), expected_attempts);
        assert_eq!(self.worker_observer.returned_errors().len(), errors);
        self.stop_worker().await;
        assert_eq!(self.worker_observer.completed(), expected_completions);
        match self
            .worker_observer
            .completed_strategies()
            .last()
            .expect("completed journey maintenance strategy")
        {
            ForgeClaimStrategy::Known(strategy) => *strategy,
            ForgeClaimStrategy::Unknown(strategy) => {
                panic!("journey maintenance completed an unknown strategy: {strategy}")
            }
        }
    }

    /// Cancels and joins the production worker loop once.
    ///
    /// # Panics
    ///
    /// Panics when the worker does not shut down within the bounded test window.
    async fn stop_worker(&mut self) {
        self.worker_stop.cancel();
        self.worker_observer.release_held_attempt_for_test();
        let task = self.worker_task.take().expect("journey worker stops once");
        tokio::time::timeout(Duration::from_secs(30), task)
            .await
            .expect("journey maintenance worker shutdown bound")
            .expect("journey maintenance worker task")
            .expect("journey maintenance worker shutdown");
    }

    /// Stops both production loops after the selected lifecycle observation.
    ///
    /// # Panics
    ///
    /// Panics when scheduler or worker shutdown exceeds the bounded test window.
    async fn shutdown(mut self) {
        self.scheduler_stop.cancel();
        if self.worker_task.is_some() {
            self.stop_worker().await;
        }
        tokio::time::timeout(Duration::from_secs(30), self.scheduler_task)
            .await
            .expect("journey maintenance scheduler shutdown bound")
            .expect("journey maintenance scheduler task")
            .expect("journey maintenance scheduler shutdown");
    }
}

/// Stable cross-topology evidence from one complete supervised Forge fixture.
#[derive(Debug, PartialEq, Eq)]
struct RoleTopologyEvidence {
    /// Exact public query row counts in provisioned tenant order.
    rows: Vec<u64>,
    /// Terminal Forge task counts in the same tenant order.
    terminal_tasks: Vec<i64>,
    /// Terminal Forge audit counts in the same tenant order.
    terminal_audits: Vec<i64>,
    /// Terminal task rows retaining exact commit evidence in the same tenant order.
    evidence_rows: Vec<i64>,
    /// Terminal task rows retaining active watermark state in the same tenant order.
    watermark_rows: Vec<i64>,
    /// Remaining table-scoped Forge leases after Scribe retirement.
    cleanup_leases: i64,
}

/// Return one rendered staging-fold duration count for a closed task result label.
fn rendered_staging_fold_task_duration_count(rendered: &str, result: &str) -> Option<f64> {
    rendered
        .lines()
        .filter(|line| line.starts_with("bifrost_forge_task_duration_seconds_count{"))
        .find(|line| {
            line.contains("strategy=\"staging_fold\"")
                && line.contains(&format!("result=\"{result}\""))
        })
        .and_then(|line| line.rsplit_once(' ').map(|(_, value)| value))
        .and_then(|value| value.parse().ok())
}

/// Prove a real superseded worker attempt records the durable `cancelled` metric result.
///
/// The stale task returns `Ok(())` after cancellation, so this regression fails if worker
/// telemetry regresses to deriving the metric result from the Rust return value.
///
/// # Panics
///
/// Panics when the real server, scheduler, worker, durable task state, or production Prometheus
/// exposition does not establish the cancellation contract.
#[tokio::test]
async fn superseded_worker_records_cancelled_duration_from_durable_state() {
    let (server, telemetry) = start_telemetry_maintenance_server().await;
    let fixture = seed_forge_group(&server, "durable_cancelled_metric").await;
    commit_journey_staging_snapshot(&fixture).await;
    let current_snapshot = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("committed staging table")
        .metadata()
        .current_snapshot_id()
        .expect("committed staging snapshot");
    assert_ne!(current_snapshot, 0, "fresh task base must be stale");

    let tasks = ForgeTasks::new(fixture.operator_pool.clone());
    let envelope = journey_envelope(&fixture, 1, 1);
    let task_id = tasks
        .enqueue(&NewForgeTask {
            data_tenant_id: fixture.tenant,
            table_ref: ForgeTaskTableIdentity::new(
                "wyrd-redux",
                &fixture.binding.logical_namespace,
                &fixture.binding.table_name,
            )
            .expect("metric task identity"),
            strategy: ForgeTaskStrategy::StagingFold,
            lane: ForgeTaskLane::Ordinary,
            base_snapshot_id: 0,
            plan: ForgeTaskPlan {
                version: FORGE_TASK_PAYLOAD_VERSION,
                inputs: vec!["telemetry-superseded.parquet".to_owned()],
                parameters: serde_json::json!({"kind":"staging_fold"}),
            },
            plan_hash: [0xA5; 32],
            estimates: ForgeTaskEstimates {
                envelope: Some(envelope),
                files: 1,
                bytes: 1,
                parallelism: envelope.reader_permits,
                memory_bytes: envelope.memory_bytes().expect("journey resident total"),
                spill_bytes: envelope.scratch_bytes().expect("journey scratch total"),
                large_ceiling_bytes: 1,
            },
            ready_at: chrono::Utc::now(),
        })
        .await
        .expect("enqueue stale metric task");
    let before =
        rendered_staging_fold_task_duration_count(&telemetry.render(), "cancelled").unwrap_or(0.0);
    let worker = ForgeWorker::new(
        Arc::clone(&fixture.forge),
        ForgeWorkerConfig::default(),
        uuid::Uuid::now_v7(),
    )
    .expect("validated production worker");
    assert!(
        worker
            .execute_one_for_test(&CancellationToken::new())
            .await
            .expect("execute stale task"),
        "the production worker must claim the stale task"
    );
    let after = rendered_staging_fold_task_duration_count(&telemetry.render(), "cancelled")
        .expect("rendered result=cancelled task duration count");
    assert!(
        after > before,
        "the real superseded completion must render result=cancelled rather than result=succeeded"
    );
    let state: String = sqlx::query_scalar(
        "SELECT state FROM vala.forge_tasks WHERE task_id = $1 AND data_tenant_id = $2",
    )
    .bind(task_id)
    .bind(fixture.tenant.as_uuid())
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("durable cancelled task state");
    assert_eq!(state, "cancelled");
    server.shutdown().await.expect("telemetry server shutdown");
}

/// Starts an isolated production server for one maintenance lifecycle journey.
///
/// # Panics
///
/// Panics when the local Postgres, catalog, storage, or server fixture cannot
/// start with its background Forge interval disabled for deterministic control.
async fn start_maintenance_journey_server() -> WyrdTestServer {
    WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .with_forge_process_role_for_test(ForgeProcessRole::Server)
        .start_in_process()
        .await
        .expect("in-process maintenance journey server")
}

/// Start an isolated maintenance server attached to the shared production telemetry capture.
///
/// # Panics
///
/// Panics when the process-global recorder or isolated server cannot start.
async fn start_telemetry_maintenance_server() -> (
    WyrdTestServer,
    wyrd_testing::bifrost::BifrostTelemetryCapture,
) {
    let (telemetry_guard, telemetry) =
        shared_process_telemetry_for_test().expect("shared production telemetry runtime");
    let server = WyrdTestServer::builder()
        .with_telemetry_for_test(telemetry_guard)
        .with_forge_interval(Duration::from_secs(3600))
        .with_forge_process_role_for_test(ForgeProcessRole::Server)
        .start_in_process()
        .await
        .expect("in-process telemetry maintenance server");
    (server, telemetry)
}

/// Commits one staging-fold snapshot through a real production scheduler and worker.
///
/// # Panics
///
/// Panics when the fixture cannot create a successful staging-fold task within
/// two bounded production passes.
async fn commit_journey_staging_snapshot(fixture: &wyrd_testing::bifrost::ForgeFixture) {
    let mut config = fixture.config.clone();
    config.max_files_per_bin = 2;
    config.max_files_per_tick = 2;
    for _ in 0..2 {
        let mut lifecycle = JourneyMaintenance::start(fixture, config.clone());
        let strategy = lifecycle.run_one_success().await;
        lifecycle.shutdown().await;
        if strategy == ForgeTaskStrategy::StagingFold {
            return;
        }
    }
    panic!("bounded maintenance journey did not execute staging_fold");
}

/// Commits one production small-file rewrite required before maintenance proof.
///
/// # Panics
///
/// Panics when the real scheduler and worker do not complete the expected
/// SmallFiles task needed to leave only reset-generation objects for GC.
async fn commit_journey_live_rewrite(fixture: &wyrd_testing::bifrost::ForgeFixture) {
    let mut lifecycle = JourneyMaintenance::start(fixture, fixture.config.clone());
    let strategy = lifecycle.run_one_success().await;
    lifecycle.shutdown().await;
    assert_eq!(
        strategy,
        ForgeTaskStrategy::SmallFiles,
        "orphan journey setup must drain its production live rewrite"
    );
}

/// Seeds one durable Reset generation whose output paths are GC-eligible.
///
/// The fixture invokes the same Prepared and Reset transition writers used by
/// the maintenance interleaving journey. The later product journey still drives
/// production scheduling, worker execution, eligibility checks, deletion, and
/// terminal orphan-GC audit; this helper only establishes the durable recovery
/// lineage that defines an eligible never-published output.
///
/// # Panics
///
/// Panics when the reset transition fixture cannot retain its exact input,
/// output, lease, or operation-state evidence.
async fn seed_journey_reset_generation(
    fixture: &wyrd_testing::bifrost::ForgeFixture,
) -> Vec<String> {
    fixture.append_forge_file(2).await;
    fixture.append_forge_file(3).await;
    let rows: Vec<(uuid::Uuid, String, chrono::NaiveDate)> = sqlx::query_as(
        "SELECT id, file_path, partition_day FROM vala.file_list \
         WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3 \
         AND committed_snapshot_id IS NULL ORDER BY id",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_all(fixture.operator_pool.pool())
    .await
    .expect("journey reset staging rows");
    assert_eq!(rows.len(), 2);
    let partition_day = rows[0].2;
    assert!(rows.iter().all(|row| row.2 == partition_day));
    let outputs = (0..2)
        .map(|ordinal| {
            format!(
                "{}/data/forge/bifrost-writer-v1/{}-{ordinal:05}.parquet",
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
            .expect("journey reset generation object");
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
            .map(|row| StoragePath::new(row.1.clone()).expect("journey reset input storage path"))
            .collect(),
        output_paths: outputs
            .iter()
            .map(|path| StoragePath::new(path.clone()).expect("journey reset output storage path"))
            .collect(),
        snapshot_id: None,
        writer_recipe_version: "bifrost-writer-v1".to_owned(),
    };
    let lease_key = vala_bifrost_redux::forge::forge_lease_key(
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
    .expect("journey reset generation lease query")
    .expect("journey reset generation lease");
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
        .expect("prepared journey reset generation");
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
        .expect("terminal journey reset generation");
    lease
        .release(&fixture.operator_pool)
        .await
        .expect("journey reset generation lease release");
    assert_eq!(
        fixture
            .forge
            .reset_generation_paths_for_test(&fixture.binding)
            .await
            .expect("journey Reset generation projection"),
        outputs
    );
    outputs
}

/// Runs the real retained-snapshot expiry path and proves one expiry commit occurs.
///
/// # Panics
///
/// Panics when two production staging commits cannot create retained history,
/// when the dedicated SnapshotExpiry task does not complete, or when the
/// authoritative expiry audit is absent.
#[tokio::test]
#[ignore = "gated journey: real Postgres, Forge maintenance, and Iceberg snapshots"]
async fn forge_snapshot_expiry_journey() {
    let server = start_maintenance_journey_server().await;
    let fixture = seed_forge_group(&server, "journey_snapshot_expiry").await;
    commit_journey_staging_snapshot(&fixture).await;
    fixture.append_forge_file(2).await;
    fixture.append_forge_file(3).await;
    commit_journey_staging_snapshot(&fixture).await;
    let table = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("journey snapshot-expiry table");
    let newest_snapshot_ms = table
        .metadata()
        .snapshots()
        .map(|snapshot| snapshot.timestamp_ms())
        .max()
        .expect("journey snapshot-expiry retained history");
    server
        .forge_clock()
        .set(
            chrono::DateTime::from_timestamp_millis(newest_snapshot_ms + 2)
                .expect("journey snapshot timestamp is UTC-representable"),
        )
        .expect("advance journey expiry clock");
    let mut config = fixture.config.clone();
    config.snapshot_retention = Duration::from_millis(1);
    config.min_files = 3;
    config.max_files_per_bin = 3;
    config.max_files_per_tick = 3;
    let mut completed_expiry = false;
    for _ in 0..3 {
        let mut lifecycle = JourneyMaintenance::start(&fixture, config.clone());
        let strategy = lifecycle.run_one_success().await;
        lifecycle.shutdown().await;
        if strategy == ForgeTaskStrategy::SnapshotExpiry {
            completed_expiry = true;
            break;
        }
    }
    assert!(
        completed_expiry,
        "production journey must complete snapshot_expiry"
    );
    assert!(
        fixture
            .operation_count("forge.snapshot_expire.committed")
            .await
            >= 1,
        "production journey must persist an expiry commit audit"
    );
    server
        .shutdown()
        .await
        .expect("journey expiry server shutdown");
}

/// Proves a sustained compaction backlog never starves Forge snapshot expiry.
///
/// The fixture builds real retained Iceberg history through two production
/// staging-fold commits, then seeds an additional pool of uncompacted staging
/// files so a staging-fold compaction candidate stays continuously available on
redacted
/// (`maintenance_trigger_snapshot_count` / `maintenance_trigger_interval`) making
/// expiry due independently of that backlog, and the reserved maintenance worker
/// slot claiming maintenance ahead of ready compaction, the journey asserts that
/// the durable production scheduler and worker still complete `SnapshotExpiry`
/// and persist its commit audit while the compaction backlog remains pending.
///
/// # Panics
///
/// Panics when the fixture cannot build retained history, when no compaction
/// candidate is present to contend with maintenance, when the bounded production
/// maintenance loop never completes `SnapshotExpiry`, or when the authoritative
/// expiry commit audit is absent.
#[tokio::test]
#[ignore = "gated journey: real Postgres, sustained compaction backlog, and Forge maintenance"]
async fn sustained_ingest_does_not_starve_snapshot_expiry_journey() {
    let server = start_maintenance_journey_server().await;
    let fixture = seed_forge_group(&server, "journey_sustained_ingest_maintenance").await;
    // Two production staging-fold commits create retained Iceberg history so the
    // maintenance trigger has snapshots beyond `retain_last` to expire.
    commit_journey_staging_snapshot(&fixture).await;
    fixture.append_forge_file(2).await;
    fixture.append_forge_file(3).await;
    commit_journey_staging_snapshot(&fixture).await;
    // Seed a sustained compaction backlog: uncompacted staging files keep a
    // staging-fold candidate available on every subsequent planning tick, so
    // maintenance must lead through real compaction contention rather than an
    // idle table.
    fixture.append_forge_file(4).await;
    fixture.append_forge_file(5).await;
    fixture.append_forge_file(6).await;
    let pending_backlog: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.file_list \
         WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3 \
         AND committed_snapshot_id IS NULL",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("sustained compaction backlog count");
    assert!(
        pending_backlog > 0,
        "journey must hold a live staging-fold compaction candidate while maintenance runs"
    );
    let table = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("journey sustained-ingest table");
    let newest_snapshot_ms = table
        .metadata()
        .snapshots()
        .map(|snapshot| snapshot.timestamp_ms())
        .max()
        .expect("journey sustained-ingest retained history");
    server
        .forge_clock()
        .set(
            chrono::DateTime::from_timestamp_millis(newest_snapshot_ms + 2)
                .expect("journey snapshot timestamp is UTC-representable"),
        )
        .expect("advance journey expiry clock");
    let mut config = fixture.config.clone();
    config.snapshot_retention = Duration::from_millis(1);
    config.maintenance_trigger_snapshot_count = 1;
    config.maintenance_trigger_interval = Duration::from_millis(1);
    let mut completed_expiry = false;
    for _ in 0..3 {
        let mut lifecycle = JourneyMaintenance::start(&fixture, config.clone());
        let strategy = lifecycle.run_one_success().await;
        lifecycle.shutdown().await;
        if strategy == ForgeTaskStrategy::SnapshotExpiry {
            completed_expiry = true;
            break;
        }
    }
    assert!(
        completed_expiry,
        "maintenance must complete snapshot_expiry despite a pending compaction backlog"
    );
    assert!(
        fixture
            .operation_count("forge.snapshot_expire.committed")
            .await
            >= 1,
        "sustained-backlog journey must persist an expiry commit audit"
    );
    let remaining_backlog: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.file_list \
         WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3 \
         AND committed_snapshot_id IS NULL",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("residual compaction backlog count");
    assert!(
        remaining_backlog > 0,
        "maintenance must lead ahead of the still-pending compaction backlog"
    );
    server
        .shutdown()
        .await
        .expect("journey sustained-ingest server shutdown");
}

/// Real Forge workers converge public small-file debt without constructing Oracle.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Bifrost journey lane"]
async fn pg_bifrost_forge_small_files_converges_without_query_dependency() {
    let cluster = WyrdTestCluster::start_spec_with_forge_completion_observer(
        BifrostClusterSpec::one_mixed().with_system_resources(forge_convergence_system_resources()),
    )
    .await
    .expect("Forge-local convergence cluster");
    let checkpoint = cluster
        .telemetry()
        .checkpoint()
        .expect("Forge-local causal telemetry checkpoint");
    let server = cluster.server(0).expect("Forge-local convergence server");
    let table = prepare_forge_convergence_table(&cluster, server, 1_000_000)
        .await
        .expect("Forge-local convergence fixture");
    server
        .forge_clock()
        .advance(chrono::Duration::days(1))
        .expect("close the convergence fixture event-day partition");
    let observer = cluster
        .forge_completion_observer()
        .expect("Forge-local completion observer");

    for transition in 1..=64 {
        let workflow = server
            .inspect_forge_workflow_for_test(cluster.data_tenant_id(), &table)
            .await
            .expect("inspect Forge-local staging-fold convergence");
        if workflow.uncompacted_staging_files == 0 {
            break;
        }
        advance_transient_forge_retries(server, cluster.data_tenant_id(), &table).await;
        let expected_attempts = observer.attempts().saturating_add(1);
        let completed_passes = server.completed_forge_scheduler_passes_for_test();
        cluster.request_forge_scheduler_pass_for_test();
        tokio::time::timeout(
            Duration::from_secs(30),
            server.wait_for_forge_scheduler_passes_for_test(completed_passes + 1),
        )
        .await
        .expect("production staging scheduler pass completes");
        if tokio::time::timeout(
            Duration::from_secs(90),
            observer.wait_for_attempts_at_least(expected_attempts),
        )
        .await
        .is_err()
        {
            panic_with_forge_convergence_diagnosis(
                &cluster,
                server,
                &checkpoint,
                &table,
                &format!("staging transition {transition} did not settle"),
            )
            .await;
        }
    }

    let workflow = server
        .inspect_forge_workflow_for_test(cluster.data_tenant_id(), &table)
        .await
        .expect("inspect Forge-local staging terminal state");
    assert_eq!(
        workflow.uncompacted_staging_files, 0,
        "production continuation must drain staging debt"
    );
    let before = server
        .inspect_forge_table_for_test(cluster.data_tenant_id(), &table)
        .await
        .expect("inspect pre-compaction Forge table");
    assert!(before.data_file_count() > 1);
    let forge_baseline = server
        .state()
        .forge()
        .expect("Forge composition")
        .resources_for_test()
        .snapshot()
        .expect("Forge resource baseline");

    for _ in 0..64 {
        if observer
            .completed_strategies()
            .contains(&ForgeClaimStrategy::Known(ForgeTaskStrategy::SmallFiles))
        {
            break;
        }
        advance_transient_forge_retries(server, cluster.data_tenant_id(), &table).await;
        let expected_completions = observer.completed().saturating_add(1);
        let completed_passes = server.completed_forge_scheduler_passes_for_test();
        cluster.request_forge_scheduler_pass_for_test();
        tokio::time::timeout(
            Duration::from_secs(30),
            server.wait_for_forge_scheduler_passes_for_test(completed_passes + 1),
        )
        .await
        .expect("production rewrite scheduler pass completes");
        if tokio::time::timeout(
            Duration::from_secs(90),
            observer.wait_for_at_least(expected_completions),
        )
        .await
        .is_err()
        {
            panic_with_forge_convergence_diagnosis(
                &cluster,
                server,
                &checkpoint,
                &table,
                "SmallFiles rewrite did not settle",
            )
            .await;
        }
    }

    assert!(
        observer
            .completed_strategies()
            .contains(&ForgeClaimStrategy::Known(ForgeTaskStrategy::SmallFiles)),
        "bounded production continuation must reach SmallFiles"
    );
    let planned_inputs = observer
        .lifecycle_events()
        .iter()
        .rev()
        .find_map(|event| match event {
            vala_bifrost_redux::forge::ForgeLifecycleEvent::Planned {
                table: observed,
                inputs,
                ..
            } if observed == &table => Some(inputs.clone()),
            _ => None,
        })
        .expect("table-scoped planned inputs");
    let after = server
        .inspect_forge_table_for_test(cluster.data_tenant_id(), &table)
        .await
        .expect("inspect post-compaction Forge table");
    let rewrite = WyrdTestServer::compare_forge_rewrite_for_test(&before, &after, &planned_inputs)
        .expect("exact Forge replacement comparison");
    let delta = cluster
        .telemetry()
        .delta_since(&checkpoint)
        .expect("Forge-local production telemetry delta");
    let report = ForgeCausalTelemetryReport::from_production_delta(&delta)
        .expect("Forge-local causal telemetry report");
    let workflow = server
        .inspect_forge_workflow_for_test(cluster.data_tenant_id(), &table)
        .await
        .expect("Forge-local converged workflow");
    assert_eq!(
        report
            .diagnose(&workflow, Some(&rewrite))
            .expect("telemetry matches converged durable state"),
        ForgeCausalDiagnosis::Converged
    );
    assert!(after.data_file_count() < before.data_file_count());
    assert_eq!(rewrite.input_files.len(), planned_inputs.len());
    assert!(!rewrite.output_files.is_empty());
    assert!(rewrite.input_bytes > 0 && rewrite.output_bytes > 0);
    assert_eq!(after.active_claims, 0);
    assert_eq!(after.active_attempts, 0);
    assert_eq!(
        server
            .state()
            .forge()
            .expect("Forge composition after rewrite")
            .resources_for_test()
            .snapshot()
            .expect("Forge resources after rewrite"),
        forge_baseline
    );
    assert_eq!(report.capacity_refusals, 0);
    assert_eq!(report.internal_invariant_failures, 0);
    cluster
        .shutdown()
        .await
        .expect("Forge-local convergence shutdown");
}

/// Builds the production-shaped resource observation used by Forge convergence.
fn forge_convergence_system_resources() -> SystemResourceSnapshot {
    let scratch = (1_u64 << 30)
        .checked_add(MIN_SCRATCH_FREE_BYTES)
        .expect("fixed convergence scratch observation fits u64");
    SystemResourceSnapshot {
        memory_limit_bytes: 2 << 30,
        effective_cpu: 4,
        scratch_capacity_bytes: scratch,
        scratch_available_bytes: scratch,
        memory_source: ResourceSource::Injected,
        cpu_source: ResourceSource::Injected,
    }
}

/// Creates deterministic public small-file debt through the real client and server.
async fn prepare_forge_convergence_table(
    cluster: &WyrdTestCluster,
    server: &WyrdTestServer,
    row_count: usize,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let table = format!("forge_local_{}", uuid::Uuid::now_v7().simple());
    server
        .state()
        .bifrost_redux
        .as_ref()
        .ok_or("missing Redux catalog")?
        .create_table(CreateTableRequest {
            table: TableRef::new(BifrostNamespace::Bifrost, &table),
            user_fields: vec![
                Field::new("id", DataType::Int64, false),
                Field::new("value", DataType::Utf8, false),
            ],
            tenant: cluster.data_tenant_id(),
            audit: None,
        })
        .await?;
    let bootstrap = server
        .bootstrap_service_in_tenant(cluster.data_tenant_id(), "forge-local-writer", &["admin"])
        .await?;
    let api_key = match bootstrap {
        Bootstrap::Machine { api_key, .. } => api_key,
        Bootstrap::User { .. } => return Err("machine bootstrap returned user".into()),
    };
    let client = WyrdClient::with_config(ClientConfig {
        grpc: GrpcConfig {
            endpoint: server.grpc_url().ok_or("missing gRPC URL")?,
            connect_retries: 0,
            max_message_bytes: 32 * 1024 * 1024,
            ..GrpcConfig::default()
        },
        http: HttpConfig {
            base_url: server.base_url().ok_or("missing HTTP URL")?.to_owned(),
            ..HttpConfig::default()
        },
        api_key: Some(api_key),
        ..ClientConfig::default()
    })?;
    let transport = BifrostGrpcTransport::connect(&client).await?;
    for start in (0..row_count).step_by(FORGE_CONVERGENCE_BATCH_ROWS) {
        let end = (start + FORGE_CONVERGENCE_BATCH_ROWS).min(row_count);
        let ids = (start..end)
            .map(i64::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        transport
            .insert_batch(
                &format!("vala.bifrost.{table}"),
                uuid::Uuid::now_v7().into_bytes(),
                forge_convergence_ipc(&ids),
            )
            .await?;
        let ingested = end.div_ceil(FORGE_CONVERGENCE_BATCH_ROWS);
        if ingested.is_multiple_of(FORGE_CONVERGENCE_BATCHES_PER_SEAL) || end == row_count {
            server.flush_bifrost().await?;
        }
    }
    Ok(table)
}

/// Encodes one bounded public-ingest batch for the convergence fixture.
fn forge_convergence_ipc(ids: &[i64]) -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("value", DataType::Utf8, false),
    ]));
    let arrays: Vec<ArrayRef> = vec![
        Arc::new(Int64Array::from(ids.to_vec())),
        Arc::new(StringArray::from(vec!["x".repeat(256); ids.len()])),
    ];
    let batch = RecordBatch::try_new(Arc::clone(&schema), arrays)
        .expect("fixed convergence arrays share a length");
    let mut bytes = Vec::new();
    let mut writer =
        StreamWriter::try_new(&mut bytes, schema.as_ref()).expect("valid convergence IPC schema");
    writer.write(&batch).expect("bounded convergence IPC write");
    writer.finish().expect("bounded convergence IPC finish");
    bytes
}

/// Advances only durably classified transient Forge retries in the journey.
async fn advance_transient_forge_retries(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    table: &str,
) {
    let pool = server
        .state()
        .postgres
        .operator_pool()
        .expect("Forge-local operator pool");
    let mut transaction = pool
        .pool()
        .begin()
        .await
        .expect("begin Forge-local retry clock step");
    let retries = sqlx::query_as::<_, (uuid::Uuid, Option<String>)>(
        "SELECT task_id,failure_class FROM vala.forge_tasks WHERE data_tenant_id=$1 AND table_name=$2 AND state='retryable' FOR UPDATE",
    )
    .bind(tenant.as_uuid())
    .bind(table)
    .fetch_all(&mut *transaction)
    .await
    .expect("load Forge-local retry classifications");
    for (task_id, failure_class) in &retries {
        assert!(
            matches!(
                failure_class.as_deref(),
                Some("transient_object_store" | "transient_coordination" | "storage_health")
            ),
            "task {task_id} must have a permitted transient class; observed {failure_class:?}"
        );
    }
    let task_ids = retries
        .into_iter()
        .map(|(task_id, _)| task_id)
        .collect::<Vec<_>>();
    if !task_ids.is_empty() {
        let updated = sqlx::query(
            "UPDATE vala.forge_tasks SET ready_at=statement_timestamp(),next_eligible_at=statement_timestamp()-interval '15 minutes' WHERE task_id=ANY($1) AND state='retryable'",
        )
        .bind(&task_ids)
        .execute(&mut *transaction)
        .await
        .expect("advance Forge-local transient eligibility")
        .rows_affected();
        assert_eq!(
            updated,
            u64::try_from(task_ids.len()).expect("retry count fits u64")
        );
    }
    transaction
        .commit()
        .await
        .expect("commit Forge-local retry clock step");
}

/// Emits the complete causal diagnosis before failing a stalled convergence transition.
async fn panic_with_forge_convergence_diagnosis(
    cluster: &WyrdTestCluster,
    server: &WyrdTestServer,
    checkpoint: &wyrd_testing::bifrost::BifrostTelemetryCheckpoint,
    table: &str,
    reason: &str,
) -> ! {
    let delta = cluster
        .telemetry()
        .delta_since(checkpoint)
        .expect("stalled Forge-local telemetry delta");
    let report = ForgeCausalTelemetryReport::from_production_delta(&delta);
    let workflow = server
        .inspect_forge_workflow_for_test(cluster.data_tenant_id(), table)
        .await
        .expect("inspect stalled Forge-local workflow");
    let diagnosis = report
        .as_ref()
        .map_err(ToString::to_string)
        .and_then(|report| {
            report
                .diagnose(&workflow, None)
                .map_err(|error| error.to_string())
        });
    panic!("{reason}: diagnosis={diagnosis:?} telemetry={report:?} workflow={workflow:?}");
}

/// A bad shared scratch volume quarantines its first worker, defers its peer,
/// and lets a healthy-volume worker publish the exact debt once.
#[tokio::test]
#[ignore = "gated journey: real Postgres and filesystem scratch quarantine"]
#[cfg(unix)]
async fn unhealthy_scratch_volume_defers_peer_and_healthy_worker_takes_over() {
    let (server, telemetry) = start_telemetry_maintenance_server().await;
    let fixture = seed_forge_group(&server, "journey_unhealthy_scratch_takeover").await;
    let checkpoint = telemetry
        .checkpoint()
        .expect("takeover causal telemetry checkpoint");
    sqlx::query("DELETE FROM vala.forge_tasks WHERE data_tenant_id=$1")
        .bind(fixture.tenant.as_uuid())
        .execute(fixture.operator_pool.pool())
        .await
        .expect("isolate takeover task queue");
    sqlx::query("DELETE FROM vala.forge_planning_demands WHERE data_tenant_id=$1")
        .bind(fixture.tenant.as_uuid())
        .execute(fixture.operator_pool.pool())
        .await
        .expect("isolate takeover demand queue");
    let inputs: Vec<String> = sqlx::query_scalar(
        "SELECT file_path FROM vala.file_list WHERE data_tenant_id=$1 AND namespace=$2 AND table_name=$3 ORDER BY file_path",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_all(fixture.operator_pool.pool())
    .await
    .expect("takeover inputs");
    let envelope = journey_envelope(&fixture, 200, 2);
    let task_id = ForgeTasks::new(fixture.operator_pool.clone())
        .enqueue(&NewForgeTask {
            data_tenant_id: fixture.tenant,
            table_ref: ForgeTaskTableIdentity::new(
                "wyrd-redux",
                &fixture.binding.logical_namespace,
                &fixture.binding.table_name,
            )
            .expect("takeover identity"),
            strategy: ForgeTaskStrategy::StagingFold,
            lane: ForgeTaskLane::Ordinary,
            base_snapshot_id: 0,
            plan: ForgeTaskPlan {
                version: FORGE_TASK_PAYLOAD_VERSION,
                inputs,
                parameters: serde_json::json!({"kind":"staging_fold"}),
            },
            plan_hash: [231; 32],
            estimates: ForgeTaskEstimates {
                envelope: Some(envelope),
                files: 2,
                bytes: 200,
                parallelism: envelope.reader_permits,
                memory_bytes: envelope.memory_bytes().expect("journey resident total"),
                spill_bytes: envelope.scratch_bytes().expect("journey scratch total"),
                large_ceiling_bytes: 2 * 1024 * 1024 * 1024,
            },
            ready_at: chrono::Utc::now(),
        })
        .await
        .expect("takeover task");
    let bad_parent = tempfile::tempdir().expect("bad scratch parent");
    let bad_root = bad_parent.path().join("scratch");
    std::fs::create_dir(&bad_root).expect("initial valid scratch root");
    let telemetry_trigger = ForgeSchedulerTrigger::with_owner_for_test(uuid::Uuid::now_v7());
    let bad_forge = fixture.context_with_worker_supervision_and_spill_root(
        fixture.config.clone(),
        ForgeWorkerCompletionObserver::new(),
        telemetry_trigger.clone(),
        &bad_root,
    );
    let telemetry_scheduler = ForgeScheduler::with_owner_for_test(&bad_forge, uuid::Uuid::now_v7())
        .expect("takeover telemetry scheduler");
    telemetry_scheduler
        .record_hint(StagingFileCommitted::new(
            fixture.binding.clone(),
            chrono::NaiveDate::from_ymd_opt(2026, 7, 14).expect("takeover hint day"),
        ))
        .await
        .expect("takeover telemetry hint");
    let scheduler_stop = CancellationToken::new();
    let scheduler_task = tokio::spawn({
        let forge = Arc::clone(&bad_forge);
        let stop = scheduler_stop.clone();
        async move { forge.run(stop).await }
    });
    telemetry_trigger.request_pass();
    tokio::time::timeout(
        Duration::from_secs(30),
        telemetry_trigger.wait_for_passes_at_least(1),
    )
    .await
    .expect("takeover telemetry scheduler pass bound");
    scheduler_stop.cancel();
    scheduler_task
        .await
        .expect("takeover telemetry scheduler join")
        .expect("takeover telemetry scheduler shutdown");
    sqlx::query("DELETE FROM vala.forge_tasks WHERE data_tenant_id=$1 AND task_id<>$2")
        .bind(fixture.tenant.as_uuid())
        .bind(task_id)
        .execute(fixture.operator_pool.pool())
        .await
        .expect("remove telemetry-only planned task");
    std::fs::remove_dir(&bad_root).expect("replace empty scratch directory");
    std::fs::File::create(&bad_root).expect("replace scratch root with file");
    let first_owner = uuid::Uuid::now_v7();
    let first = ForgeWorker::new(
        Arc::clone(&bad_forge),
        ForgeWorkerConfig::default(),
        first_owner,
    )
    .expect("faulted worker");
    assert!(
        first
            .execute_one_for_test(&CancellationToken::new())
            .await
            .is_err()
    );
    let qualified: (String, Option<String>, i32, bool) = sqlx::query_as(
        "SELECT state,failure_class,attempt_count,EXISTS(SELECT 1 FROM vala.forge_worker_registry WHERE worker_id=$2 AND quarantined) FROM vala.forge_tasks WHERE task_id=$1",
    )
    .bind(task_id)
    .bind(first_owner)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("qualified storage failure");
    assert_eq!(
        qualified,
        (
            "retryable".to_owned(),
            Some("storage_health".to_owned()),
            1,
            true
        )
    );
    sqlx::query("UPDATE vala.forge_tasks SET next_eligible_at=statement_timestamp(),ready_at=statement_timestamp() WHERE task_id=$1")
        .bind(task_id)
        .execute(fixture.operator_pool.pool())
        .await
        .expect("advance first backoff");
    let peer = ForgeWorker::new(
        Arc::clone(&bad_forge),
        ForgeWorkerConfig::default(),
        uuid::Uuid::now_v7(),
    )
    .expect("same-volume peer");
    let peer_claim = peer
        .claim_next_for_test(false)
        .await
        .expect("peer claim query");
    assert!(
        peer_claim.is_none(),
        "same-volume peer must defer before the soft fallback bound: {peer_claim:?}"
    );
    let healthy_root = tempfile::tempdir().expect("healthy scratch root");
    let healthy_forge = fixture.context_with_worker_supervision_and_spill_root(
        fixture.config.clone(),
        ForgeWorkerCompletionObserver::new(),
        ForgeSchedulerTrigger::with_owner_for_test(uuid::Uuid::now_v7()),
        healthy_root.path(),
    );
    let healthy = ForgeWorker::new(
        healthy_forge,
        ForgeWorkerConfig::default(),
        uuid::Uuid::now_v7(),
    )
    .expect("healthy worker");
    assert!(
        healthy
            .execute_one_for_test(&CancellationToken::new())
            .await
            .expect("healthy takeover")
    );
    let outcome: (String, i64) = sqlx::query_as(
        "SELECT state,(SELECT count(*) FROM vala.file_list WHERE data_tenant_id=$2 AND namespace=$3 AND table_name=$4 AND NOT compacted) FROM vala.forge_tasks WHERE task_id=$1",
    )
    .bind(task_id)
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("healthy takeover outcome");
    assert_eq!(outcome, ("succeeded".to_owned(), 0));
    let inspection = server
        .inspect_forge_table_for_test(fixture.tenant, &fixture.binding.table_name)
        .await
        .expect("authoritative takeover table inspection");
    assert!(
        inspection
            .tasks
            .iter()
            .any(|(_, state)| state == "succeeded")
    );
    let workflow = server
        .inspect_forge_workflow_for_test(fixture.tenant, &fixture.binding.table_name)
        .await
        .expect("authoritative takeover workflow inspection");
    assert_eq!(workflow.uncompacted_staging_files, 0);
    let table = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("takeover publication ancestry");
    let publications = table
        .metadata()
        .snapshots()
        .filter(|snapshot| {
            snapshot
                .summary()
                .additional_properties
                .contains_key("forge.operation_id")
        })
        .count();
    assert_eq!(publications, 1, "one operation identity may publish once");
    let delta = telemetry
        .delta_since(&checkpoint)
        .expect("takeover causal telemetry delta");
    let report = ForgeCausalTelemetryReport::from_production_delta(&delta)
        .expect("takeover causal telemetry report");
    assert_eq!(report.storage_health_failures, 1);
    assert_eq!(report.transient_object_store_failures, 0);
    assert_eq!(report.data_refusals, 0);
    assert_eq!(report.capacity_refusals, 0);
    assert!(report.quarantined_workers >= 1);
    assert!(report.rewrite_input_files >= 1 && report.rewrite_output_files >= 1);
    server.shutdown().await.expect("takeover server shutdown");
}

/// Production supervision fails stop on Forge panics and restart recovers one publication.
#[tokio::test]
#[ignore = "gated journey: real supervised server panic and lease-expiry recovery"]
async fn supervised_forge_panic_fails_stop_and_recovers_exactly_once() {
    for after_commit in [false, true] {
        let config = ForgeConfig {
            lease_ttl: Duration::from_secs(4),
            iceberg_total_retry_timeout: Duration::from_secs(1),
            catalog_request_timeout: Duration::from_secs(1),
            uncertainty_margin: Duration::from_secs(1),
            uncertainty_bound: Duration::from_secs(1),
            ..ForgeConfig::default()
        };
        let mut cluster =
            WyrdTestCluster::start_with_embedded_forge_panic_recovery_for_test(config)
                .await
                .expect("panic recovery cluster");
        let node = cluster.configured_node_ids()[0];
        let server = cluster.server_by_node(node).expect("panic recovery server");
        let table_name = if after_commit {
            "forge_panic_after_commit"
        } else {
            "forge_panic_before_commit"
        };
        let fixture = seed_forge_group(server, table_name).await;
        let control = cluster
            .commit_uncertainty_catalog()
            .expect("panic catalog control");
        if after_commit {
            control.panic_after_next_commit();
        } else {
            control.panic_before_next_commit();
        }
        let completed_passes = server.completed_forge_scheduler_passes_for_test();
        cluster.request_forge_scheduler_pass_for_test();
        tokio::time::timeout(Duration::from_secs(30), control.wait_for_panic())
            .await
            .expect("production catalog panic reached");
        let terminal = cluster
            .await_node_terminal_failure_for_test(node, Duration::from_secs(30))
            .await
            .expect("production supervisor terminal failure");
        assert!(
            terminal.contains("InternalInvariant") || terminal.contains("Forge"),
            "unexpected terminal supervisor result: {terminal}"
        );
        assert!(completed_passes <= 1);
        control.disarm_panic();
        let retained: (uuid::Uuid, uuid::Uuid, String, i32) = sqlx::query_as(
            "SELECT task_id,attempt_id,state,attempt_count FROM vala.forge_tasks WHERE data_tenant_id=$1 AND table_name=$2 AND state IN ('claimed','running') ORDER BY created_at DESC LIMIT 1",
        )
        .bind(fixture.tenant.as_uuid())
        .bind(&fixture.binding.table_name)
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("panic-retained durable attempt");
        assert_eq!(retained.2, "running");
        assert_eq!(retained.3, 0);
        tokio::time::sleep(Duration::from_secs(5)).await;
        cluster
            .restart_node(node)
            .await
            .expect("fresh server restart");
        let restarted = cluster.server_by_node(node).expect("restarted server");
        let reclaimed = restarted
            .reclaim_expired_forge_attempts_for_test(1)
            .await
            .expect("production attempt reclaim");
        if !reclaimed.is_empty() {
            assert_eq!(reclaimed, vec![(retained.0, retained.1)]);
        }
        cluster
            .advance_reclaimed_panic_task_for_test(retained.0, retained.1)
            .await
            .expect("exact panic eligibility advance");
        let observer = cluster
            .forge_completion_observer()
            .expect("panic recovery completion observer");
        cluster.request_forge_scheduler_pass_for_test();
        tokio::time::timeout(Duration::from_secs(90), observer.wait_for_at_least(1))
            .await
            .expect("restarted production worker settles retained debt");
        let restarted = cluster
            .server_by_node(node)
            .expect("settled restart server");
        let workflow = restarted
            .inspect_forge_workflow_for_test(fixture.tenant, &fixture.binding.table_name)
            .await
            .expect("panic recovery workflow");
        assert_eq!(workflow.uncompacted_staging_files, 0);
        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("panic recovery table");
        let publications = table
            .metadata()
            .snapshots()
            .filter(|snapshot| {
                snapshot
                    .summary()
                    .additional_properties
                    .contains_key("forge.operation_id")
            })
            .count();
        assert_eq!(publications, 1, "one operation identity may publish once");
        let resources = restarted
            .state()
            .forge()
            .expect("restarted Forge")
            .resources_for_test()
            .snapshot()
            .expect("panic recovery resource snapshot");
        assert_eq!(resources.elastic_memory_used_bytes, 0);
        assert_eq!(resources.scratch_used_bytes, 0);
        assert_eq!(resources.forge_reader_permits_used, 0);
        cluster.shutdown().await.expect("panic recovery shutdown");
    }
}

/// Runs real orphan collection and proves an aged, unreferenced object is deleted.
///
/// # Panics
///
/// Panics when the production maintenance path does not complete SnapshotExpiry
/// and its coupled orphan collector, retain the object, or omit its terminal
/// orphan-GC audit.
#[tokio::test]
#[ignore = "gated journey: real Postgres, Forge maintenance, and object cleanup"]
async fn forge_orphan_cleanup_journey() {
    let server = start_maintenance_journey_server().await;
    let fixture = seed_forge_group(&server, "journey_orphan_cleanup").await;
    commit_journey_staging_snapshot(&fixture).await;
    let orphans = seed_journey_reset_generation(&fixture).await;
    commit_journey_staging_snapshot(&fixture).await;
    commit_journey_live_rewrite(&fixture).await;
    let modified = fixture
        .staging
        .stat(&orphans[0])
        .await
        .expect("journey orphan metadata")
        .last_modified()
        .expect("journey orphan modification time")
        .into_inner()
        .as_millisecond();
    server
        .forge_clock()
        .set(
            chrono::DateTime::from_timestamp_millis(modified + 100)
                .expect("journey orphan timestamp is UTC-representable"),
        )
        .expect("advance journey orphan clock");
    let mut config = fixture.config.clone();
    config.snapshot_retention = Duration::from_millis(1);
    config.orphan_gc_ttl = Duration::from_millis(1);
    config.min_files = 3;
    config.max_files_per_bin = 3;
    config.max_files_per_tick = 3;
    let mut lifecycle = JourneyMaintenance::start(&fixture, config);
    let strategy = lifecycle.run_one_success().await;
    lifecycle.shutdown().await;
    assert_eq!(
        strategy,
        ForgeTaskStrategy::SnapshotExpiry,
        "production journey must complete orphan maintenance"
    );
    for orphan in &orphans {
        assert!(
            fixture.staging.stat(orphan).await.is_err(),
            "production orphan collector must delete every aged reset-generation output"
        );
    }
    assert!(
        fixture.operation_count("forge.orphan_gc.committed").await
            + fixture.operation_count("forge.orphan_gc.recovered").await
            >= 1,
        "production journey must persist an orphan-GC terminal audit"
    );
    server
        .shutdown()
        .await
        .expect("journey orphan server shutdown");
}

/// Proves Forge fixture rebuilds retain the server-owned wall clock.
#[tokio::test]
#[ignore = "gated journey: real bound Wyrd server, Postgres, and Forge"]
async fn forge_fixture_rebuilds_retain_manual_forge_clock() {
    let server = WyrdTestServer::builder()
        .start_bound()
        .await
        .expect("server");
    let fixture = seed_forge_group(&server, "manual_forge_clock").await;
    let initial = fixture
        .forge
        .clock_for_test()
        .now()
        .expect("initial Forge clock");
    let advanced = server
        .forge_clock()
        .advance(chrono::Duration::seconds(1))
        .expect("advance Forge clock");
    let catalog = fixture.context_with_catalog(fixture.config.clone(), fixture.catalog.clone());
    assert!(advanced > initial);
    assert_eq!(
        catalog.clock_for_test().now().expect("catalog clock"),
        advanced
    );
    server.shutdown().await.expect("server shutdown");
}

/// Prove the production worker-only role shares dependencies without serving APIs.
///
/// The cluster starts the same production `WyrdServer` supervisor used by the
/// bound journey: one `server` process owns the public API and scheduler while
/// three `forge-worker` processes join the shared durable Forge work pool. The
/// journey writes and queries through the sole server endpoint, then uses a
/// deterministic claim barrier to make all three worker-only processes claim
/// independent tenant tasks through the production worker loop. Listener
/// assertions prevent a worker role from silently becoming a second serving
/// surface.
///
/// # Panics
///
/// Panics when the real shared Postgres/catalog/storage topology cannot start,
/// a process role differs from its requested composition, a worker binds an
/// API listener, or shutdown cannot complete.
#[tokio::test]
#[ignore = "gated journey: real server role topology, Postgres, and shared Forge dependencies"]
async fn dedicated_forge_workers_share_dependencies_without_public_listeners() {
    dedicated_forge_workers_journey().await;
}

/// Implements the dedicated-worker product journey.
///
/// # Panics
///
/// Panics when the worker topology, listener isolation, durable work, or
/// supervised shutdown differs from the production role contract.
async fn dedicated_forge_workers_journey() {
    let cluster = WyrdTestCluster::start_with_dedicated_forge_workers()
        .await
        .expect("dedicated Forge worker cluster");
    assert_eq!(cluster.topology(), BifrostTopology::DedicatedForgeWorkers);
    let roles = cluster
        .servers()
        .iter()
        .map(|server| server.forge_process_role())
        .collect::<Vec<_>>();
    assert_eq!(
        roles,
        vec![
            ForgeProcessRole::Server,
            ForgeProcessRole::ForgeWorker,
            ForgeProcessRole::ForgeWorker,
            ForgeProcessRole::ForgeWorker,
        ]
    );
    let scheduler_server = cluster.server(0).expect("scheduler/server pod");
    assert!(scheduler_server.base_url().is_some());
    assert!(scheduler_server.grpc_url().is_some());
    for worker in &cluster.servers()[1..] {
        assert_eq!(worker.base_url(), None);
        assert_eq!(worker.grpc_url(), None);
        assert_eq!(worker.bound_addr(), None);
    }
    let completion = cluster
        .forge_completion_observer()
        .expect("dedicated worker completion observer");
    let scenario = Scenario {
        name: "dedicated-forge-workers",
        tenants: 3,
    };
    let tenants = provision_tenants(scheduler_server, scenario).await;
    assert_active_traces_roster(scheduler_server, &tenants, scenario).await;
    for cycle in 0..WRITE_CYCLES {
        write_cycle(
            std::slice::from_ref(scheduler_server),
            &tenants,
            cycle,
            scenario,
        )
        .await;
        for tenant in &tenants {
            scheduler_server
                .flush_bifrost_for_tenant(tenant.id)
                .await
                .expect("dedicated scheduler/server flush");
        }
    }
    scheduler_server
        .forge_clock()
        .advance(chrono::Duration::days(1) + chrono::Duration::minutes(3))
        .expect("advance scheduler Forge clock");
    completion.hold_after_claims_for_test(3);
    trigger_supervised_scheduler(scheduler_server, scenario).await;
    tokio::time::timeout(
        Duration::from_secs(10),
        completion.wait_for_claims_for_test(),
    )
    .await
    .expect("each worker-only process claimed one independent durable task");
    assert_claim_barrier_shape(scheduler_server, 3, scenario).await;
    completion.release_claims_for_test();
    tokio::time::timeout(
        Duration::from_secs(10),
        completion.wait_for_distinct_workers_at_least(3),
    )
    .await
    .expect("all three worker-only processes completed durable work");
    tokio::time::timeout(
        Duration::from_secs(10),
        completion.wait_for_at_least(tenants.len()),
    )
    .await
    .expect("every dedicated worker tenant reached terminal completion");
    let scribe = scheduler_server
        .bifrost_scribe()
        .expect("server-role Scribe");
    scribe
        .retire_committed_for_test()
        .await
        .expect("retire committed Scribe generations");
    let oracle = scheduler_server
        .state()
        .bifrost_query()
        .expect("server-role Oracle runtime");
    let readiness = oracle.oracle().readiness_snapshot();
    assert!(
        readiness.startup_reconciled,
        "dedicated worker Oracle startup reconciliation was lost before query: {readiness:?}"
    );
    assert!(
        readiness.live_oracles > 0,
        "dedicated worker Oracle membership disappeared before query: {readiness:?}"
    );
    assert!(
        readiness.running_capacity > 0,
        "dedicated worker Oracle has no local query capacity before query: {readiness:?}"
    );
    let expected_rows =
        u64::try_from(WRITE_CYCLES * SPANS_PER_WRITE).expect("bounded dedicated journey row count");
    for tenant in &tenants {
        assert_eq!(
            query_rows(scheduler_server, &tenant.jwt).await,
            expected_rows,
            "dedicated worker tenant {} rows",
            tenant.id
        );
        assert_terminal_audits(scheduler_server, tenant.id, scenario).await;
        assert_snapshot(scheduler_server, tenant.id, scenario).await;
    }
    assert_no_forge_leases(scheduler_server, scenario).await;
    cluster
        .shutdown()
        .await
        .expect("dedicated cluster shutdown");
}

/// Compare embedded and dedicated supervised roles through identical tenant work.
///
/// # Panics
///
/// Panics when either real topology diverges in public rows or its durable
/// task, audit, evidence, watermark, or cleanup footprint.
#[tokio::test]
#[ignore = "gated journey: paired real Forge role topologies, Postgres, and shared Iceberg dependencies"]
async fn embedded_and_dedicated_forge_roles_preserve_exact_durable_parity() {
    let embedded = WyrdTestCluster::start_with_embedded_forge_observer()
        .await
        .expect("embedded Forge cluster");
    assert_eq!(
        embedded
            .server(0)
            .expect("embedded server")
            .forge_process_role(),
        ForgeProcessRole::All
    );
    let embedded_evidence = run_supervised_role_fixture(&embedded, 1, "embedded-forge-role").await;
    embedded
        .shutdown()
        .await
        .expect("embedded cluster shutdown");

    let dedicated = WyrdTestCluster::start_with_dedicated_forge_workers()
        .await
        .expect("dedicated Forge cluster");
    assert_eq!(dedicated.topology(), BifrostTopology::DedicatedForgeWorkers);
    assert!(dedicated.servers()[1..].iter().all(|worker| {
        worker.forge_process_role() == ForgeProcessRole::ForgeWorker
            && worker.base_url().is_none()
            && worker.grpc_url().is_none()
            && worker.bound_addr().is_none()
    }));
    let dedicated_evidence =
        run_supervised_role_fixture(&dedicated, 3, "dedicated-forge-role").await;
    dedicated
        .shutdown()
        .await
        .expect("dedicated cluster shutdown");

    assert_eq!(embedded_evidence, dedicated_evidence);
}

/// Prove a real dedicated worker loss is reclaimed without duplicate public rows.
///
/// One worker stops only after PostgreSQL has persisted its claim. The test
/// invokes the production planner once, then lets only supervised worker roles
/// claim, lose, reclaim, publish, and retire the work.
///
/// # Panics
///
/// Panics when the supervised role topology does not reclaim the lost claim,
/// creates duplicate rows, or leaves task/audit/evidence/lease state divergent.
#[tokio::test]
#[ignore = "gated journey: supervised dedicated Forge worker loss and lease reclaim"]
async fn supervised_dedicated_roles_reclaim_lost_worker_without_duplicate_rows() {
    let cluster = WyrdTestCluster::start_with_dedicated_forge_workers()
        .await
        .expect("dedicated Forge cluster");
    let server = cluster.server(0).expect("scheduler server");
    let completion = cluster
        .forge_completion_observer()
        .expect("shared supervised observer");
    let scenario = Scenario {
        name: "supervised-worker-reclaim",
        tenants: 3,
    };
    let tenants = provision_tenants(server, scenario).await;
    assert_active_traces_roster(server, &tenants, scenario).await;
    for cycle in 0..WRITE_CYCLES {
        write_cycle(std::slice::from_ref(server), &tenants, cycle, scenario).await;
        for tenant in &tenants {
            server
                .flush_bifrost_for_tenant(tenant.id)
                .await
                .expect("supervised loss fixture flush");
        }
    }
    completion.abandon_next_claim_for_test();
    server
        .forge_clock()
        .advance(chrono::Duration::days(1) + chrono::Duration::minutes(3))
        .expect("age staged files for production planner");
    trigger_supervised_scheduler(server, scenario).await;
    tokio::time::timeout(
        Duration::from_secs(10),
        completion.wait_for_abandoned_claim_for_test(),
    )
    .await
    .expect("one real worker stopped after its durable claim");
    let (lost_task, lost_attempt, lost_owner) = completion.abandoned_claim_identity_for_test();
    let expired: uuid::Uuid = sqlx::query_scalar(
        "UPDATE vala.forge_tasks SET claim_expires_at = statement_timestamp() - interval '1 millisecond' WHERE task_id = $1 AND attempt_id = $2 AND claimed_by = $3 AND state = 'claimed' RETURNING task_id",
    )
    .bind(lost_task)
    .bind(lost_attempt)
    .bind(lost_owner)
    .fetch_one(
        server
            .state()
            .postgres
            .operator_pool()
            .expect("operator pool")
            .pool(),
    )
    .await
    .expect("expire the exact abandoned durable claim");
    assert_eq!(expired, lost_task, "only the stopped claim may expire");
    let staged_completion = tokio::time::timeout(
        Duration::from_secs(15),
        completion.wait_for_strategy_at_least(ForgeTaskStrategy::StagingFold, 3),
    )
    .await;
    if staged_completion.is_err() {
        panic!(
            "remaining supervised workers did not reclaim staged tasks: {:?}",
            forge_task_diagnostics(server).await
        );
    }

    server
        .bifrost_scribe()
        .expect("server-role Scribe")
        .retire_committed_for_test()
        .await
        .expect("retire committed Scribe generations");
    let expected_rows =
        u64::try_from(WRITE_CYCLES * SPANS_PER_WRITE).expect("bounded reclaim rows");
    for tenant in &tenants {
        assert_terminal_audits(server, tenant.id, scenario).await;
        assert_snapshot(server, tenant.id, scenario).await;
        let mut conn = server
            .state()
            .postgres
            .vala()
            .tenant_conn(tenant.id)
            .await
            .expect("reclaim tenant connection");
        let (terminal, evidence): (i64, i64) = sqlx::query_as(
            "SELECT count(*), count(*) FILTER (WHERE evidence IS NOT NULL) FROM vala.forge_tasks WHERE data_tenant_id = $1 AND strategy = 'staging_fold' AND state = 'succeeded'",
        )
        .bind(tenant.id.as_uuid())
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("reclaimed terminal task evidence");
        assert_eq!(
            terminal, 1,
            "tenant {} has one terminal reclaimed task",
            tenant.id
        );
        assert_eq!(
            evidence, 1,
            "tenant {} retained exact task evidence",
            tenant.id
        );
        assert_eq!(
            supervised_reclaim_query_rows(server, &tenant.jwt).await,
            expected_rows,
            "reclaimed tenant {} public rows after terminal={terminal}, evidence={evidence}",
            tenant.id
        );
    }
    assert_no_forge_leases(server, scenario).await;
    cluster
        .shutdown()
        .await
        .expect("dedicated cluster shutdown");
}

/// Prove real dedicated workers reconcile a post-commit uncertain publication exactly once.
///
/// # Panics
///
/// Panics when the catalog boundary is not reached, prepared recovery does not
/// terminalize through a supervised worker, or public/durable results duplicate.
#[tokio::test]
#[ignore = "gated journey: supervised dedicated Forge uncertain-commit recovery"]
async fn supervised_dedicated_roles_recover_uncertain_commit_without_duplicate_rows() {
    supervised_uncertain_commit_recovery_journey().await;
}

/// Implements the uncertain-commit recovery product journey.
///
/// # Panics
///
/// Panics when a prepared uncertain publication is not reconciled exactly once
/// through the dedicated production worker topology.
async fn supervised_uncertain_commit_recovery_journey() {
    let cluster = WyrdTestCluster::start_with_dedicated_forge_workers_with_uncertainty_for_test()
        .await
        .expect("dedicated uncertain Forge cluster");
    let server = cluster.server(0).expect("scheduler server");
    let completion = cluster
        .forge_completion_observer()
        .expect("shared supervised observer");
    let control = cluster
        .commit_uncertainty_catalog()
        .expect("shared commit uncertainty catalog");
    let scenario = Scenario {
        name: "supervised-uncertain-commit",
        tenants: 1,
    };
    let tenants = provision_tenants(server, scenario).await;
    for cycle in 0..WRITE_CYCLES {
        write_cycle(std::slice::from_ref(server), &tenants, cycle, scenario).await;
        server
            .flush_bifrost_for_tenant(tenants[0].id)
            .await
            .expect("uncertain fixture flush");
    }
    server
        .forge_clock()
        .advance(chrono::Duration::days(1) + chrono::Duration::minutes(3))
        .expect("age uncertain fixture files");
    control.pause_after_commit();
    trigger_supervised_scheduler(server, scenario).await;
    tokio::time::timeout(Duration::from_secs(10), control.wait_for_commit())
        .await
        .expect("supervised worker reached real post-commit boundary");
    control.reject_paused_commit();
    tokio::time::timeout(
        Duration::from_secs(10),
        completion.wait_for_strategy_at_least(ForgeTaskStrategy::StagingFold, 1),
    )
    .await
    .expect("supervised worker reconciled prepared uncertain commit");
    assert_eq!(
        control.update_attempts(),
        1,
        "staging publication performs one catalog update attempt"
    );
    server
        .bifrost_scribe()
        .expect("server-role Scribe")
        .retire_committed_for_test()
        .await
        .expect("retire uncertain fixture Scribe generations");
    let tail_stats = server
        .bifrost_scribe()
        .expect("server-role Scribe")
        .memtable_stats()
        .expect("uncertain fixture Scribe stats");
    assert_eq!(
        tail_stats.writable_rows, 0,
        "uncertain writable tail drained"
    );
    assert_eq!(
        tail_stats.immutable_rows, 0,
        "uncertain immutable tail retired"
    );
    let expected_rows =
        u64::try_from(WRITE_CYCLES * SPANS_PER_WRITE).expect("bounded uncertain rows");
    assert_terminal_audits(server, tenants[0].id, scenario).await;
    assert_snapshot(server, tenants[0].id, scenario).await;
    let mut conn = server
        .state()
        .postgres
        .vala()
        .tenant_conn(tenants[0].id)
        .await
        .expect("uncertain tenant connection");
    let (terminal, evidence): (i64, i64) = sqlx::query_as(
        "SELECT count(*), count(*) FILTER (WHERE evidence IS NOT NULL) FROM vala.forge_tasks WHERE data_tenant_id = $1 AND strategy = 'staging_fold' AND state = 'succeeded'",
    )
    .bind(tenants[0].id.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("uncertain terminal task evidence");
    assert_eq!(terminal, 1, "uncertain publication terminalized once");
    assert_eq!(evidence, 1, "uncertain publication retained exact evidence");
    let (files, compacted, committed): (i64, i64, i64) = sqlx::query_as(
        "SELECT count(*), count(*) FILTER (WHERE compacted), count(*) FILTER (WHERE committed_snapshot_id IS NOT NULL) FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = 'vala.traces' AND table_name = 'spans'",
    )
    .bind(tenants[0].id.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("uncertain file-list convergence");
    conn.commit()
        .await
        .expect("complete uncertain evidence read");
    assert_eq!(
        committed, files,
        "uncertain recovery stamps every exact staging input"
    );
    assert_eq!(
        query_rows(server, &tenants[0].jwt).await,
        expected_rows,
        "uncertain commit public rows after terminal={terminal}, evidence={evidence}, files={files}, compacted={compacted}, committed={committed}"
    );
    assert_no_forge_leases(server, scenario).await;
    cluster
        .shutdown()
        .await
        .expect("dedicated cluster shutdown");
}

/// Prove real planner admission reaches a cluster-exclusive large lane and terminal unschedulable lane.
///
/// # Panics
///
/// Panics when production planning does not persist the configured lane state.
#[tokio::test]
#[ignore = "gated journey: real Forge unschedulable admission"]
async fn dedicated_roles_terminalize_unschedulable_work() {
    dedicated_unschedulable_admission_journey().await;
}

/// Implements the bounded planner-admission product journey.
///
/// # Panics
///
/// Panics when production planner admission fails to retain the exact large or
/// unschedulable durable task classification.
async fn dedicated_unschedulable_admission_journey() {
    let large_config = vala_bifrost_redux::forge::ForgeConfig {
        max_bytes_per_tick: 1,
        max_memory_bytes: i64::MAX as u64,
        max_large_task_bytes: i64::MAX as u64,
        ..vala_bifrost_redux::forge::ForgeConfig::default()
    };
    let cluster = WyrdTestCluster::start_with_dedicated_forge_workers_with_config_for_test(
        large_config.clone(),
    )
    .await
    .expect("large-lane cluster");
    let server = cluster.server(0).expect("scheduler server");
    let scenario = Scenario {
        name: "large-lane",
        tenants: 2,
    };
    let tenants = provision_tenants(server, scenario).await;
    let fixture = seed_forge_group(server, "large_singleton").await;
    let bootstrap = fixture.context_with_config(vala_bifrost_redux::forge::ForgeConfig::default());
    ForgeScheduler::new(&bootstrap)
        .expect("bootstrap scheduler")
        .record_hint(StagingFileCommitted::new(
            fixture.binding.clone(),
            chrono::NaiveDate::from_ymd_opt(2026, 7, 14).expect("fixture day"),
        ))
        .await
        .expect("record bootstrap ingest hint");
    bootstrap.run_once().await.expect("schedule bootstrap fold");
    let bootstrap_stop = CancellationToken::new();
    let bootstrap_worker = ForgeWorker::new(
        bootstrap,
        ForgeWorkerConfig {
            worker_concurrency: 1,
            per_tenant_active_cap: 1,
        },
        uuid::Uuid::now_v7(),
    )
    .expect("bootstrap worker");
    let bootstrap_task = tokio::spawn(bootstrap_worker.run(bootstrap_stop.clone()));
    let mut bootstrap_completed = false;
    for _ in 0..200 {
        let completed: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM vala.forge_tasks WHERE data_tenant_id=$1 AND table_name='large_singleton' AND strategy='staging_fold' AND state='succeeded')",
        )
        .bind(fixture.tenant.as_uuid())
        .fetch_one(
            server
                .state()
                .postgres
                .operator_pool()
                .expect("operator pool")
                .pool(),
        )
        .await
        .expect("observe bootstrap fold");
        if completed {
            bootstrap_completed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    bootstrap_stop.cancel();
    bootstrap_task
        .await
        .expect("join bootstrap worker")
        .expect("stop bootstrap worker");
    assert!(
        bootstrap_completed,
        "bootstrap fold must create one snapshot"
    );
    sqlx::query("UPDATE vala.forge_scheduler_state SET expires_at=statement_timestamp()-interval '1 second' WHERE singleton")
        .execute(
            server
                .state()
                .postgres
                .operator_pool()
                .expect("operator pool")
                .pool(),
        )
        .await
        .expect("release bootstrap scheduler lease");
    assert_eq!(
        server
            .forge_publisher()
            .try_publish(StagingFileCommitted::new(
                fixture.binding.clone(),
                chrono::NaiveDate::from_ymd_opt(2026, 7, 14).expect("fixture day"),
            )),
        StagingPublishOutcome::Published,
        "production scheduler hint channel must accept the maintenance discovery signal"
    );
    trigger_supervised_scheduler(server, scenario).await;
    let (large_lane, estimated_files): (String, i64) =
        sqlx::query_as("SELECT lane, estimated_files FROM vala.forge_tasks WHERE data_tenant_id=$1 AND table_name='large_singleton' AND strategy='snapshot_expiry' ORDER BY created_at DESC LIMIT 1")
            .bind(fixture.tenant.as_uuid())
            .fetch_one(
                server
                    .state()
                    .postgres
                    .operator_pool()
                    .expect("operator pool")
                    .pool(),
            )
            .await
            .expect("observe durable one-input large task");
    assert_eq!(large_lane, ForgeTaskLane::LargeSingleton.as_str());
    assert_eq!(estimated_files, 1);
    for cycle in 0..WRITE_CYCLES {
        write_cycle(std::slice::from_ref(server), &tenants, cycle, scenario).await;
        for tenant in &tenants {
            server
                .flush_bifrost_for_tenant(tenant.id)
                .await
                .expect("large flush");
        }
    }
    server
        .forge_clock()
        .advance(chrono::Duration::days(1) + chrono::Duration::minutes(3))
        .expect("age large files");
    trigger_supervised_scheduler(server, scenario).await;
    trigger_supervised_scheduler(server, scenario).await;
    let tasks: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT strategy, lane, state FROM vala.forge_tasks WHERE data_tenant_id = ANY($1) AND table_name='spans' AND strategy='staging_fold' ORDER BY data_tenant_id, strategy",
    )
    .bind(tenants.iter().map(|tenant| tenant.id.as_uuid()).collect::<Vec<_>>())
    .fetch_all(
        server
            .state()
            .postgres
            .operator_pool()
            .expect("operator pool")
            .pool(),
    )
    .await
    .expect("large singleton task states");
    assert!(
        !tasks.is_empty()
            && tasks
                .iter()
                .all(|(strategy, lane, state)| strategy == "staging_fold"
                    && lane == "ordinary"
                    && state == "unschedulable"),
        "multi-input overflow must be terminally unschedulable even when its bytes fit the large ceiling: {tasks:?}"
    );
    cluster.shutdown().await.expect("large cluster shutdown");

    let unschedulable_config = vala_bifrost_redux::forge::ForgeConfig {
        max_bytes_per_tick: 1,
        max_large_task_bytes: 1,
        ..vala_bifrost_redux::forge::ForgeConfig::default()
    };
    let cluster = WyrdTestCluster::start_with_dedicated_forge_workers_with_config_for_test(
        unschedulable_config,
    )
    .await
    .expect("unschedulable cluster");
    let server = cluster.server(0).expect("scheduler server");
    let scenario = Scenario {
        name: "unschedulable",
        tenants: 1,
    };
    let tenants = provision_tenants(server, scenario).await;
    for cycle in 0..WRITE_CYCLES {
        write_cycle(std::slice::from_ref(server), &tenants, cycle, scenario).await;
        server
            .flush_bifrost_for_tenant(tenants[0].id)
            .await
            .expect("unschedulable flush");
    }
    server
        .forge_clock()
        .advance(chrono::Duration::days(1) + chrono::Duration::minutes(3))
        .expect("age unschedulable files");
    trigger_supervised_scheduler(server, scenario).await;
    let mut conn = server
        .state()
        .postgres
        .tenant_conn(tenants[0].id)
        .await
        .expect("unschedulable tenant connection");
    let (state, audits): (String, i64) = sqlx::query_as("SELECT state, (SELECT count(*) FROM vala.audit_outbox WHERE operation='forge.task.unschedulable') FROM vala.forge_tasks WHERE strategy='staging_fold'").fetch_one(&mut **conn.transaction()).await.expect("unschedulable terminal");
    conn.commit()
        .await
        .expect("commit unschedulable observation");
    assert_eq!(state, "unschedulable");
    assert_eq!(audits, 1);
    cluster
        .shutdown()
        .await
        .expect("unschedulable cluster shutdown");
}

/// Proves `schedule_once` stops before demand effects when exact renewal is lost.
///
/// # Panics
///
/// Panics when the deterministic renewal boundary is not reached, exact lease
/// expiry fails, or any enqueue, acknowledgement, cursor, status, or complete
/// telemetry effect occurs after leadership loss.
#[tokio::test]
#[ignore = "gated journey: real scheduler renewal-loss boundary"]
async fn scheduler_renewal_loss_stops_every_later_effect() {
    let cluster = WyrdTestCluster::start_with_dedicated_forge_workers()
        .await
        .expect("renewal-loss cluster");
    let server = cluster.server(0).expect("scheduler server");
    let scenario = Scenario {
        name: "renewal-loss",
        tenants: 1,
    };
    let tenants = provision_tenants(server, scenario).await;
    let identity = ForgeTaskTableIdentity::new("wyrd-redux", "vala.traces", "spans")
        .expect("renewal-loss identity");
    ForgeTasks::new(
        server
            .state()
            .postgres
            .operator_pool()
            .expect("operator pool")
            .clone(),
    )
    .upsert_periodic(tenants[0].id, &identity)
    .await
    .expect("renewal-loss demand");
    let owner = uuid::Uuid::now_v7();
    let scheduler = ForgeScheduler::with_owner_for_test(
        server.state().forge().expect("server-owned Forge"),
        owner,
    )
    .expect("renewal-loss scheduler");
    scheduler.pause_before_demand_renewal_for_test();
    let stop = CancellationToken::new();
    let operator_pool = server
        .state()
        .postgres
        .operator_pool()
        .expect("operator pool");
    let (result, evidence) = tokio::join!(scheduler.schedule_once(&stop), async {
        scheduler.wait_for_demand_renewal_pause_for_test().await;
        let before: (i64, i64, Option<uuid::Uuid>) = sqlx::query_as(
                "SELECT (SELECT count(*) FROM vala.forge_tasks), (SELECT count(*) FROM vala.forge_planning_demands), last_tenant_id FROM vala.forge_scheduler_state WHERE singleton AND owner=$1",
            )
            .bind(owner)
            .fetch_one(operator_pool.pool())
            .await
            .expect("pre-loss scheduler evidence");
        let expired = sqlx::query(
                "UPDATE vala.forge_scheduler_state SET expires_at=statement_timestamp()-interval '1 millisecond' WHERE singleton AND owner=$1",
            )
            .bind(owner)
            .execute(operator_pool.pool())
            .await
            .expect("expire exact scheduler owner")
            .rows_affected();
        assert_eq!(expired, 1);
        scheduler.release_demand_renewal_pause_for_test();
        before
    });
    assert!(
        matches!(result, Err(ForgeError::FenceLost { .. })),
        "renewal loss must retain the acquired-fence failure class: {result:?}"
    );
    let after: (i64, i64, Option<uuid::Uuid>) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM vala.forge_tasks), (SELECT count(*) FROM vala.forge_planning_demands), last_tenant_id FROM vala.forge_scheduler_state WHERE singleton",
    )
    .fetch_one(operator_pool.pool())
    .await
    .expect("post-loss scheduler evidence");
    assert_eq!(
        after, evidence,
        "loss must suppress every later durable effect"
    );
    assert_eq!(
        scheduler.complete_publications_for_test(),
        0,
        "loss must suppress complete status and telemetry publication"
    );
    cluster.shutdown().await.expect("renewal-loss shutdown");
}

/// Drive independent tenant tasks through one supervised production role topology.
///
/// # Panics
///
/// Panics when production ingest, durable scheduling, supervised worker
/// completion, public query, or durable parity inspection fails.
async fn run_supervised_role_fixture(
    cluster: &WyrdTestCluster,
    worker_count: usize,
    name: &'static str,
) -> RoleTopologyEvidence {
    let server = cluster.server(0).expect("serving Forge server");
    let completion = cluster
        .forge_completion_observer()
        .expect("shared supervised completion observer");
    let scenario = Scenario {
        name,
        tenants: worker_count.max(3),
    };
    let tenants = provision_tenants(server, scenario).await;
    assert_active_traces_roster(server, &tenants, scenario).await;
    for cycle in 0..WRITE_CYCLES {
        write_cycle(std::slice::from_ref(server), &tenants, cycle, scenario).await;
        for tenant in &tenants {
            server
                .flush_bifrost_for_tenant(tenant.id)
                .await
                .expect("supervised role fixture flush");
        }
    }
    server
        .forge_clock()
        .advance(chrono::Duration::days(1) + chrono::Duration::minutes(3))
        .expect("advance supervised role fixture clock");
    completion.hold_after_claims_for_test(worker_count);
    trigger_supervised_scheduler(server, scenario).await;
    let queued: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.forge_tasks WHERE data_tenant_id = ANY($1) AND state = 'ready'",
    )
    .bind(
        tenants
            .iter()
            .map(|tenant| tenant.id.as_uuid())
            .collect::<Vec<_>>(),
    )
    .fetch_one(
        server
            .state()
            .postgres
            .operator_pool()
            .expect("operator pool")
            .pool(),
    )
    .await
    .expect("supervised queued tenant tasks");
    assert!(
        usize::try_from(queued).expect("nonnegative queued count") >= worker_count,
        "{name} constrained tenant must remain inside the measured eligible set"
    );
    assert_scheduler_takeover_preserves_fairness_bound(server, &tenants, scenario).await;
    tokio::time::timeout(
        Duration::from_secs(10),
        completion.wait_for_claims_for_test(),
    )
    .await
    .expect("configured supervised roles claimed independent tasks");
    assert_claim_barrier_shape(server, worker_count, scenario).await;
    completion.release_claims_for_test();
    tokio::time::timeout(
        Duration::from_secs(10),
        completion.wait_for_distinct_workers_at_least(worker_count),
    )
    .await
    .expect("configured supervised roles completed durable work");
    tokio::time::timeout(
        Duration::from_secs(10),
        completion.wait_for_at_least(tenants.len()),
    )
    .await
    .expect("every role fixture tenant reached a terminal supervised completion");
    server
        .bifrost_scribe()
        .expect("server-role Scribe")
        .retire_committed_for_test()
        .await
        .expect("retire Scribe generations");

    let expected_rows =
        u64::try_from(WRITE_CYCLES * SPANS_PER_WRITE).expect("bounded role fixture rows");
    let mut rows = Vec::with_capacity(tenants.len());
    let mut terminal_tasks = Vec::with_capacity(tenants.len());
    let mut terminal_audits = Vec::with_capacity(tenants.len());
    let mut evidence_rows = Vec::with_capacity(tenants.len());
    let mut watermark_rows = Vec::with_capacity(tenants.len());
    for tenant in &tenants {
        let row_count = query_rows(server, &tenant.jwt).await;
        assert_eq!(row_count, expected_rows, "{name} tenant {} rows", tenant.id);
        assert_terminal_audits(server, tenant.id, scenario).await;
        assert_snapshot(server, tenant.id, scenario).await;
        rows.push(row_count);
        let mut conn = server
            .state()
            .postgres
            .vala()
            .tenant_conn(tenant.id)
            .await
            .expect("role fixture tenant connection");
        let durable: (i64, i64, i64) = sqlx::query_as(
            "SELECT count(*), count(*) FILTER (WHERE evidence IS NOT NULL), count(*) FILTER (WHERE watermark_snapshot_id IS NOT NULL) FROM vala.forge_tasks WHERE data_tenant_id = $1 AND state IN ('succeeded', 'unschedulable', 'failed', 'cancelled')",
        )
        .bind(tenant.id.as_uuid())
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("role fixture durable task evidence");
        let audits: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id = $1 AND operation IN ('forge.file_compact.committed', 'forge.file_compact.recovered', 'forge.file_compact.reset')",
        )
        .bind(tenant.id.as_uuid())
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("role fixture terminal audit count");
        terminal_tasks.push(durable.0);
        evidence_rows.push(durable.1);
        watermark_rows.push(durable.2);
        terminal_audits.push(audits);
    }
    assert_no_forge_leases(server, scenario).await;
    let cleanup_leases: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.maintenance_leases WHERE lease_key LIKE 'forge:table:%'",
    )
    .fetch_one(
        server
            .state()
            .postgres
            .operator_pool()
            .expect("operator pool")
            .pool(),
    )
    .await
    .expect("role fixture cleanup lease count");
    RoleTopologyEvidence {
        rows,
        terminal_tasks,
        terminal_audits,
        evidence_rows,
        watermark_rows,
        cleanup_leases,
    }
}

/// Return durable task strategy, state, owner, and expiry data for a failing role journey.
///
/// # Panics
///
/// Panics when platform-admin task inspection fails during failure reporting.
async fn forge_task_diagnostics(
    server: &WyrdTestServer,
) -> Vec<(
    String,
    String,
    Option<uuid::Uuid>,
    Option<chrono::DateTime<chrono::Utc>>,
)> {
    sqlx::query_as(
        "SELECT strategy, state, claimed_by, claim_expires_at FROM vala.forge_tasks ORDER BY data_tenant_id, created_at",
    )
    .fetch_all(
        server
            .state()
            .postgres
            .operator_pool()
            .expect("operator pool")
            .pool(),
    )
    .await
    .expect("Forge task diagnostic query")
}

/// Assert a real-role claim barrier has one active task per independent tenant table.
///
/// The durable unique active-publication constraint is inspected before the
/// barrier releases a worker. This proves concurrent independent-table claims
/// without allowing overlapping publication for a tenant/table identity.
///
/// # Panics
///
/// Panics when platform-admin claim inspection fails or the observed claim
/// shape differs from the configured production worker topology.
async fn assert_claim_barrier_shape(server: &WyrdTestServer, expected: usize, scenario: Scenario) {
    let rows: Vec<(uuid::Uuid, String, String, i64)> = sqlx::query_as(
        "SELECT data_tenant_id, namespace_name, table_name, count(*) FROM vala.forge_tasks WHERE state IN ('claimed', 'running', 'prepared') GROUP BY data_tenant_id, namespace_name, table_name ORDER BY data_tenant_id, namespace_name, table_name",
    )
    .fetch_all(
        server
            .state()
            .postgres
            .operator_pool()
            .expect("operator pool")
            .pool(),
    )
    .await
    .expect("claim-barrier durable task inspection");
    assert_eq!(
        rows.len(),
        expected,
        "{} must expose one independent active claim per worker: {rows:?}",
        scenario.name
    );
    assert!(
        rows.iter().all(|(_, _, _, count)| *count == 1),
        "{} same-table publication overlap: {rows:?}",
        scenario.name
    );
}

/// Expire one exact scheduler lease and prove the next owner preserves the fair cursor bound.
///
/// # Panics
///
/// Panics when the exact lease cannot be expired, takeover planning fails, or
/// the durable scheduler fence/cursor no longer describes a bounded tenant ring.
async fn assert_scheduler_takeover_preserves_fairness_bound(
    server: &WyrdTestServer,
    tenants: &[TenantWriter],
    scenario: Scenario,
) {
    let tasks = ForgeTasks::new(
        server
            .state()
            .postgres
            .operator_pool()
            .expect("operator pool")
            .clone(),
    );
    let table = ForgeTaskTableIdentity::new("wyrd-redux", "vala.traces", "spans")
        .expect("fairness table identity");
    for tenant in tenants {
        tasks
            .upsert_periodic(tenant.id, &table)
            .await
            .expect("seed eligible fairness demand");
    }
    let eligible_tenants: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT data_tenant_id) FROM vala.forge_planning_demands",
    )
    .fetch_one(
        server
            .state()
            .postgres
            .operator_pool()
            .expect("operator pool")
            .pool(),
    )
    .await
    .expect("measure exact eligible tenant count");
    let fairness_bound = usize::try_from(eligible_tenants).expect("nonnegative eligible tenants");
    assert_eq!(
        fairness_bound,
        tenants.len(),
        "{} exact fairness bound must include every role-fixture tenant",
        scenario.name
    );
    let hot_tenant = tenants[0].id;
    let constrained_tenant = tenants[tenants.len() - 1].id;
    let (first_owner, first_fence, cursor_before_takeover): (
        uuid::Uuid,
        i64,
        Option<uuid::Uuid>,
    ) = sqlx::query_as(
        "SELECT owner, fencing_token, last_tenant_id FROM vala.forge_scheduler_state WHERE singleton AND owner IS NOT NULL",
    )
    .fetch_one(
        server
            .state()
            .postgres
            .operator_pool()
            .expect("operator pool")
            .pool(),
    )
    .await
    .expect("read exact scheduler owner lease");
    let expired: uuid::Uuid = sqlx::query_scalar(
        "UPDATE vala.forge_scheduler_state SET expires_at = statement_timestamp() - interval '1 millisecond' WHERE singleton AND owner = $1 RETURNING owner",
    )
    .bind(first_owner)
    .fetch_one(
        server
            .state()
            .postgres
            .operator_pool()
            .expect("operator pool")
            .pool(),
    )
    .await
    .expect("expire exact scheduler owner lease");
    assert_eq!(
        expired, first_owner,
        "only the first scheduler lease may expire"
    );
    let mut selections = Vec::with_capacity(fairness_bound);
    let mut constrained_considered = false;
    for _ in 0..fairness_bound {
        trigger_supervised_scheduler(server, scenario).await;
        let selected: Option<uuid::Uuid> = sqlx::query_scalar(
            "SELECT last_tenant_id FROM vala.forge_scheduler_state WHERE singleton",
        )
        .fetch_one(
            server
                .state()
                .postgres
                .operator_pool()
                .expect("operator pool")
                .pool(),
        )
        .await
        .expect("observe production scheduler cursor");
        let selected = selected.expect("complete scheduler pass advances cursor");
        selections.push(selected);
        constrained_considered = !sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM vala.forge_planning_demands WHERE data_tenant_id=$1 AND catalog_name=$2 AND namespace_name=$3 AND table_name=$4)",
        )
        .bind(constrained_tenant.as_uuid())
        .bind(&table.catalog)
        .bind(&table.namespace)
        .bind(&table.table)
        .fetch_one(
            server
                .state()
                .postgres
                .operator_pool()
                .expect("operator pool")
                .pool(),
        )
        .await
        .expect("observe constrained demand acknowledgement");
        tasks
            .upsert_periodic(hot_tenant, &table)
            .await
            .expect("keep hot tenant eligible");
        if constrained_considered {
            break;
        }
    }
    let (fencing_token, cursor): (i64, Option<uuid::Uuid>) = sqlx::query_as(
        "SELECT fencing_token, last_tenant_id FROM vala.forge_scheduler_state WHERE singleton",
    )
    .fetch_one(
        server
            .state()
            .postgres
            .operator_pool()
            .expect("operator pool")
            .pool(),
    )
    .await
    .expect("takeover scheduler state");
    assert_eq!(
        fencing_token,
        first_fence + 1,
        "{} takeover must mint exactly one successor generation",
        scenario.name
    );
    assert!(
        cursor.is_some(),
        "{} scheduler takeover lost fair cursor",
        scenario.name
    );
    assert!(
        constrained_considered,
        "{} constrained tenant was not considered within exact N={fairness_bound} passes: {selections:?}",
        scenario.name
    );
    assert!(
        cursor_before_takeover.is_some(),
        "{} takeover fixture must begin from a continued cursor",
        scenario.name
    );
    let prior_cursor = cursor_before_takeover.expect("continued cursor checked above");
    let mut eligible = tenants
        .iter()
        .map(|tenant| tenant.id.as_uuid())
        .collect::<Vec<_>>();
    eligible.sort_unstable();
    let expected_final = eligible
        .iter()
        .copied()
        .rfind(|tenant| *tenant <= prior_cursor)
        .or_else(|| eligible.last().copied())
        .expect("non-empty eligible ring");
    assert_eq!(
        cursor,
        Some(expected_final),
        "{} takeover must retain the prior cursor and continue strict-after through the exact ring",
        scenario.name
    );
    let hot_pending: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM vala.forge_planning_demands WHERE data_tenant_id=$1)",
    )
    .bind(hot_tenant.as_uuid())
    .fetch_one(
        server
            .state()
            .postgres
            .operator_pool()
            .expect("operator pool")
            .pool(),
    )
    .await
    .expect("observe hot tenant eligibility");
    assert!(
        hot_pending,
        "{} hot tenant stopped being eligible",
        scenario.name
    );
}

/// Request and await one pass from the server's already-supervised scheduler.
///
/// This preserves the real server role's durable scheduler owner and avoids a
/// test-created scheduler bypassing lifecycle composition.
///
/// # Panics
///
/// Panics when the supervised scheduler does not return one pass inside the
/// bounded journey deadline.
async fn trigger_supervised_scheduler(server: &WyrdTestServer, scenario: Scenario) {
    let trigger = server.forge_scheduler_trigger_for_test();
    let expected = trigger.completed_passes().saturating_add(1);
    server.trigger_forge_scheduler_for_test();
    tokio::time::timeout(
        Duration::from_secs(10),
        trigger.wait_for_passes_at_least(expected),
    )
    .await
    .unwrap_or_else(|_| {
        panic!(
            "{} supervised Forge scheduler did not return",
            scenario.name
        )
    });
}

/// Authenticated tenant identity used by concurrent writers and query checks.
struct TenantWriter {
    /// Durable tenant isolation key.
    id: DataTenantId,
    /// Tenant-scoped access token accepted by OTLP and query endpoints.
    jwt: String,
}

/// Provision the requested number of isolated tenants and writer identities.
///
/// # Panics
///
/// Panics when tenant, service-account, API-key, or access-token provisioning
/// fails.
async fn provision_tenants(server: &WyrdTestServer, scenario: Scenario) -> Vec<TenantWriter> {
    let mut tenant_ids = vec![server.data_tenant_id()];
    for index in 1..scenario.tenants {
        tenant_ids.push(
            server
                .seed_tenant(&format!("forge-distributed-{index}"))
                .await
                .unwrap_or_else(|error| panic!("{} seed tenant: {error}", scenario.name)),
        );
    }
    let mut tenants = Vec::with_capacity(tenant_ids.len());
    for (index, id) in tenant_ids.into_iter().enumerate() {
        server
            .ensure_traces_spans_table_for_test(id)
            .await
            .unwrap_or_else(|error| {
                panic!(
                    "{} provision traces spans table for {id}: {error}",
                    scenario.name
                )
            });
        let bootstrap = if index == 0 {
            server
                .bootstrap_service(&format!("forge-writer-{index}"), &["admin"])
                .await
        } else {
            server
                .bootstrap_service_in_tenant(id, &format!("forge-writer-{index}"), &["admin"])
                .await
        }
        .unwrap_or_else(|error| panic!("{} bootstrap tenant {id}: {error}", scenario.name));
        let api_key: SecretString = match bootstrap {
            Bootstrap::Machine { api_key, .. } => api_key,
            other => panic!(
                "{} expected machine bootstrap, got {other:?}",
                scenario.name
            ),
        };
        let jwt = server
            .exchange_api_key(&api_key)
            .await
            .unwrap_or_else(|error| panic!("{} exchange tenant {id}: {error}", scenario.name));
        tenants.push(TenantWriter { id, jwt });
    }
    tenants
}

/// Assert the scheduler's operator-visible roster contains every journey tenant.
///
/// This journey writes only the canonical traces spans table, so a row for each
/// tenant proves Forge will discover the intended table independently of
/// staging-file history.
///
/// # Panics
///
/// Panics when the operator roster query fails or a canonical tenant table is
/// missing.
async fn assert_active_traces_roster(
    server: &WyrdTestServer,
    tenants: &[TenantWriter],
    scenario: Scenario,
) {
    let operator_pool = server
        .state()
        .postgres
        .operator_pool()
        .expect("operator pool");
    let roster =
        vala_sql::queries::forge_catalog_operator::list_active_tables_for_operator(&operator_pool)
            .await
            .expect("active Bifrost table roster");
    for tenant in tenants {
        assert!(
            roster.iter().any(|row| {
                row.data_tenant_id == tenant.id.as_uuid() && row.fqn == "vala.traces.spans"
            }),
            "{} missing active traces roster entry for tenant {}: {roster:?}",
            scenario.name,
            tenant.id
        );
    }
}

/// Send one concurrent OTLP write from every pod for every tenant.
///
/// # Panics
///
/// Panics when a bound gRPC endpoint cannot connect or rejects an authenticated
/// export request.
async fn write_cycle(
    servers: &[WyrdTestServer],
    tenants: &[TenantWriter],
    cycle: usize,
    scenario: Scenario,
) {
    let mut writers = Vec::with_capacity(servers.len() * tenants.len());
    for (pod_index, server) in servers.iter().enumerate() {
        let endpoint = server.grpc_url().expect("bound pod gRPC URL");
        for (tenant_index, tenant) in tenants.iter().enumerate() {
            let endpoint = endpoint.clone();
            let jwt = tenant.jwt.clone();
            writers.push(tokio::spawn(async move {
                let deadline = Instant::now() + Duration::from_secs(10);
                let channel = loop {
                    match wyrd_tonic::tonic::transport::Channel::from_shared(endpoint.clone())
                        .expect("valid gRPC endpoint")
                        .connect()
                        .await
                    {
                        Ok(channel) => break channel,
                        Err(error) => {
                            assert!(Instant::now() < deadline, "connect OTLP writer: {error}");
                            tokio::time::sleep(Duration::from_millis(50)).await;
                        }
                    }
                };
                let seed =
                    u64::try_from((cycle + 1) * 10_000 + (pod_index + 1) * 100 + tenant_index)
                        .expect("bounded seed");
                let anchor = u64::try_from(
                    (chrono::Utc::now() - chrono::Duration::days(2))
                        .timestamp_nanos_opt()
                        .expect("timestamp nanos"),
                )
                .expect("positive timestamp");
                let mut generator = RandomTraceGenerator::from_seed_at(seed, anchor);
                let request = generator.export_request(pod_index, seed, SPANS_PER_WRITE);
                let mut request = wyrd_tonic::tonic::Request::new(request);
                request.metadata_mut().insert(
                    "x-wyrd-access-token",
                    format!("Bearer {jwt}").parse().expect("token metadata"),
                );
                let mut client =
                    wyrd_tonic::otlp::trace_service::trace_service_client::TraceServiceClient::new(
                        channel,
                    );
                let response = client
                    .export(request)
                    .await
                    .expect("OTLP export")
                    .into_inner();
                assert!(response.partial_success.is_none());
            }));
        }
    }
    for writer in writers {
        writer
            .await
            .unwrap_or_else(|error| panic!("{} writer task: {error}", scenario.name));
    }
}

/// Send the public same-table query used before and after Forge publication.
///
/// # Panics
///
/// Panics when the HTTP request cannot be sent.
async fn query_response(server: &WyrdTestServer, jwt: &str) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!(
            "{}/v1/query",
            server.base_url().expect("bound server URL")
        ))
        .header("x-wyrd-access-token", format!("Bearer {jwt}"))
        .json(&BifrostQueryRequest {
            sql: "SELECT * FROM \"vala.traces.spans\" WHERE service_name = 'checkout-api'"
                .to_owned(),
            visibility: VisibilityMode::Fused,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: None,
        })
        .send()
        .await
        .expect("public query request")
}

/// Return the terminal row count from the public same-table query stream.
///
/// # Panics
///
/// Panics when the query fails, framing is invalid, or the terminal is missing.
async fn query_rows(server: &WyrdTestServer, jwt: &str) -> u64 {
    let response = query_response(server, jwt).await;
    if !response.status().is_success() {
        let status = response.status();
        let readiness = server
            .state()
            .bifrost_query()
            .map(|runtime| runtime.oracle().readiness_snapshot());
        let body = response.text().await.expect("query error body");
        panic!("public query response: {status}; readiness={readiness:?}; body={body}");
    }
    let body = response.bytes().await.expect("query response body");
    let mut decoder = FrameDecoder::new(32 * 1024 * 1024);
    let frames = decoder
        .push::<proto::QueryStreamFrame>(&body)
        .expect("query frames");
    decoder.finish().expect("query frame boundary");
    frames
        .into_iter()
        .find_map(|frame| match frame.frame {
            Some(proto::query_stream_frame::Frame::Terminal(terminal)) => Some(terminal.row_count),
            _ => None,
        })
        .expect("query terminal frame")
}

/// Return one public read while retaining the exact failure payload for role-loss diagnosis.
///
/// # Panics
///
/// Panics when the public read does not succeed or omits its row-count header.
async fn supervised_reclaim_query_rows(server: &WyrdTestServer, jwt: &str) -> u64 {
    let response = query_response(server, jwt).await;
    let status = response.status();
    let headers = response.headers().clone();
    let body = response
        .text()
        .await
        .expect("public reclaim query response body");
    assert!(
        status.is_success(),
        "supervised reclaimed public query {status}: {body}"
    );
    headers
        .get("x-wyrd-row-count")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
        .expect("successful supervised reclaim query row-count header")
}

/// Assert every prepared operation has one terminal event and ordered outputs.
///
/// # Panics
///
/// Panics when audit inspection fails, transitions do not reconcile, or a
/// terminal compaction lacks deterministically ordered output paths.
async fn assert_terminal_audits(server: &WyrdTestServer, tenant: DataTenantId, scenario: Scenario) {
    let mut conn = server
        .state()
        .postgres
        .vala()
        .tenant_conn(tenant)
        .await
        .expect("Forge audit tenant connection");
    let rows = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT operation, detail FROM vala.audit_outbox WHERE data_tenant_id = $1 AND operation LIKE 'forge.file_compact.%' ORDER BY created_at",
    )
    .bind(tenant.as_uuid())
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("Forge audit rows");
    let prepared = rows
        .iter()
        .filter(|(operation, _)| operation == "forge.file_compact.prepared")
        .count();
    let terminal = rows
        .iter()
        .filter(|(operation, _)| {
            matches!(
                operation.as_str(),
                "forge.file_compact.committed"
                    | "forge.file_compact.recovered"
                    | "forge.file_compact.reset"
            )
        })
        .count();
    assert!(prepared > 0, "{} missing prepared audit", scenario.name);
    assert_eq!(prepared, terminal, "{} terminal audit count", scenario.name);
    for (_, detail) in rows.iter().filter(|(operation, _)| {
        matches!(
            operation.as_str(),
            "forge.file_compact.committed" | "forge.file_compact.recovered"
        )
    }) {
        let detail: AuditDetail =
            serde_json::from_str(detail.as_deref().expect("terminal Forge audit detail"))
                .expect("typed Forge audit detail");
        let AuditDetail::ForgeCompaction { output_paths, .. } = detail else {
            panic!("{} unexpected Forge audit detail", scenario.name);
        };
        let paths = output_paths
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        assert!(
            paths.windows(2).all(|pair| pair[0] < pair[1]),
            "{} output_paths must be ordered: {paths:?}",
            scenario.name
        );
    }
}

/// Assert the tenant's traces table has at least one committed snapshot.
///
/// # Panics
///
/// Panics when the Redux catalog or table cannot be loaded.
async fn assert_snapshot(server: &WyrdTestServer, tenant: DataTenantId, scenario: Scenario) {
    let binding = vala_bifrost_redux::catalog::TenantTableBinding::resolve((
        tenant,
        vala_bifrost_redux::catalog::TableRef::new(
            vala_bifrost_redux::namespaces::BifrostNamespace::Traces,
            "spans",
        ),
    ))
    .expect("traces binding");
    let table = server
        .state()
        .bifrost_redux
        .as_ref()
        .expect("Redux catalog")
        .iceberg_catalog()
        .load_table(&binding.table_ident())
        .await
        .unwrap_or_else(|error| panic!("{} load tenant {tenant} table: {error}", scenario.name));
    assert!(
        table.metadata().current_snapshot_id().is_some(),
        "{} tenant {tenant} missing snapshot",
        scenario.name
    );
}

/// Return the active table-scoped Forge lease count after one inspection pass.
///
/// The convergence loop uses this durable state to distinguish a published
/// file list from a worker that has not yet finished its matching lease.
///
/// # Panics
///
/// Panics when durable lease inspection fails.
async fn forge_lease_count(server: &WyrdTestServer) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM vala.maintenance_leases WHERE lease_key LIKE 'forge:table:%'",
    )
    .fetch_one(
        server
            .state()
            .postgres
            .operator_pool()
            .expect("operator pool")
            .pool(),
    )
    .await
    .expect("Forge lease count")
}

/// Assert no Forge table lease remains after the scenario converges.
///
/// # Panics
///
/// Panics when durable lease inspection fails.
async fn assert_no_forge_leases(server: &WyrdTestServer, scenario: Scenario) {
    let leases = forge_lease_count(server).await;
    assert_eq!(leases, 0, "{} stale Forge leases", scenario.name);
}

/// Captured task evidence used by the workload convergence predicate test.
#[derive(Clone)]
struct CapturedCompactionTask {
    /// Durable Forge terminal state.
    state: &'static str,
    /// Whether committed output evidence exists for the task.
    has_evidence: bool,
}

/// Return whether the captured workload has no open tails and all target tasks
/// have terminal committed evidence; unrelated later tasks are ignored.
fn workload_has_converged(
    tails: &[DataTenantId],
    targets: &[(DataTenantId, bool, Vec<CapturedCompactionTask>, i64, i64)],
) -> bool {
    tails.is_empty()
        && targets
            .iter()
            .all(|(_, active, tasks, audits, audits_before)| {
                !active
                    && audits > audits_before
                    && tasks
                        .iter()
                        .all(|task| task.state == "succeeded" && task.has_evidence)
            })
}

/// Target convergence ignores unrelated later Ready maintenance but rejects a
/// target task that has not succeeded with durable evidence.
#[test]
fn workload_convergence_tracks_only_captured_compaction_tasks() {
    let tenant = DataTenantId::new_v7();
    let succeeded = CapturedCompactionTask {
        state: "succeeded",
        has_evidence: true,
    };
    let target_complete = vec![(tenant, false, vec![succeeded.clone()], 4, 3)];
    assert!(workload_has_converged(&[], &target_complete));

    let target_ready = vec![(
        tenant,
        false,
        vec![CapturedCompactionTask {
            state: "ready",
            ..succeeded.clone()
        }],
        4,
        3,
    )];
    assert!(!workload_has_converged(&[], &target_ready));

    let target_failed = vec![(
        tenant,
        false,
        vec![CapturedCompactionTask {
            state: "failed",
            ..succeeded
        }],
        4,
        3,
    )];
    assert!(!workload_has_converged(&[], &target_failed));
}
