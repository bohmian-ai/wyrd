//! Shared fixtures for the forge modules.
//!
//! Every item here is used by more than one sibling module. A helper
//! used by exactly one module lives in that module instead. Contains
//! no tests.

use arrow::array::{ArrayRef, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use secrecy::SecretString;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
use vala_bifrost_redux::forge::{
    ForgeCapacity, ForgeConfig, ForgeEnvelopeSizer, ForgeError, ForgeSchedulerTrigger, ForgeWorker,
    ForgeWorkerCompletionObserver, ForgeWorkerConfig,
};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_sdk::BifrostGrpcTransport;
use vala_sql::row_types::forge_tasks::{ForgeClaimStrategy, ForgeTaskStrategy};
use wyrd_client::WyrdClient;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_server::config::BifrostTarget;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{AuditDetail, BifrostQueryRequest, FreshnessPolicy, VisibilityMode};
use wyrd_testing::bifrost::forge_harness::seed_forge_group;
use wyrd_testing::bifrost::{WyrdTestCluster, shared_process_telemetry_for_test};
use wyrd_testing::{Bootstrap, WyrdTestServer};
use wyrd_tonic::frame_codec::FrameDecoder;
use wyrd_tonic::wyrd::v1 as proto;

/// One deployment and writer-concurrency shape exercised by the journey.
#[derive(Clone, Copy)]
pub(crate) struct Scenario {
    /// Diagnostic name included in assertion failures.
    pub(crate) name: &'static str,
    /// Number of isolated tenants writing concurrently.
    pub(crate) tenants: usize,
}

/// Number of independent flush cycles produced by every pod for every tenant.
///
/// Multiple cycles create independent Scribe files so active-day Forge
/// compaction is exercised instead of leaving a single open-tail file.
pub(crate) const WRITE_CYCLES: usize = 3;

/// Stable scheduler identity for maintenance journey fixtures.
pub(crate) const MAINTENANCE_JOURNEY_SCHEDULER_OWNER: u128 =
    0x0198_39f4_2b51_7000_8000_0000_0000_0005;

/// Provisions one Forge fixture and creates its compaction debt through native ingestion.
///
/// # Panics
///
/// Panics when provisioning or native ingestion cannot cross the durable flush barrier.
pub(crate) async fn native_forge_group(
    server: &WyrdTestServer,
    table_name: &str,
) -> wyrd_testing::bifrost::ForgeFixture {
    let fixture = seed_forge_group(server, table_name).await;
    append_native_forge_cycles(server, &fixture, 0..3).await;
    fixture
}

/// Appends Scribe-owned files through the public native Arrow transport.
///
/// # Panics
///
/// Panics when authentication, ingestion, or the server-owned tenant flush fails.
pub(crate) async fn append_native_forge_cycles(
    server: &WyrdTestServer,
    fixture: &wyrd_testing::bifrost::ForgeFixture,
    sequences: impl IntoIterator<Item = i64>,
) {
    append_native_forge_cycles_with_rows(server, fixture, sequences, 1).await;
}

/// Appends a selected row count in each independently sealed native batch.
pub(crate) async fn append_native_forge_cycles_with_rows(
    server: &WyrdTestServer,
    fixture: &wyrd_testing::bifrost::ForgeFixture,
    sequences: impl IntoIterator<Item = i64>,
    rows_per_batch: usize,
) {
    append_native_forge_cycles_with_flush_mode(server, fixture, sequences, rows_per_batch, true)
        .await;
}

/// Sends public batches with either per-batch or group-level durable flushes.
pub(crate) async fn append_native_forge_cycles_with_flush_mode(
    server: &WyrdTestServer,
    fixture: &wyrd_testing::bifrost::ForgeFixture,
    sequences: impl IntoIterator<Item = i64>,
    rows_per_batch: usize,
    flush_each: bool,
) {
    let table_name = &fixture.binding.table_name;
    let sequences = sequences.into_iter().collect::<Vec<_>>();
    let writer_sequence = sequences.first().copied().unwrap_or_default();
    let bootstrap = server
        .bootstrap_service_in_tenant(
            fixture.tenant,
            &format!("{table_name}-writer-{writer_sequence}"),
            &["admin"],
        )
        .await
        .expect("bootstrap native Forge writer");
    let api_key = match bootstrap {
        Bootstrap::Machine { api_key, .. } => api_key,
        Bootstrap::User { .. } => panic!("native Forge writer bootstrap returned a user"),
    };
    let client = WyrdClient::with_config(ClientConfig {
        grpc: GrpcConfig {
            endpoint: server.grpc_url().expect("bound Forge gRPC URL"),
            connect_retries: 3,
            ..GrpcConfig::default()
        },
        http: HttpConfig {
            base_url: server.base_url().expect("bound Forge HTTP URL").to_owned(),
            ..HttpConfig::default()
        },
        api_key: Some(api_key),
        ..ClientConfig::default()
    })
    .expect("native Forge client config");
    let transport = BifrostGrpcTransport::connect(&client)
        .await
        .expect("connect native Forge writer");
    for sequence in sequences {
        transport
            .insert_batch(
                &format!("vala.bifrost.{table_name}"),
                uuid::Uuid::now_v7().into_bytes(),
                native_forge_value_ipc(sequence, rows_per_batch),
            )
            .await
            .expect("native Forge insert");
        if flush_each {
            server
                .flush_bifrost_for_tenant(fixture.tenant)
                .await
                .expect("server-owned tenant Scribe flush barrier");
        }
    }
    if !flush_each {
        server
            .flush_bifrost_for_tenant(fixture.tenant)
            .await
            .expect("server-owned grouped Scribe flush barrier");
    }
}

/// Encodes the public one-column schema owned by the Forge fixture table.
pub(crate) fn native_forge_value_ipc(sequence: i64, rows: usize) -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![Field::new(
        "value",
        DataType::Int64,
        false,
    )]));
    let batch = RecordBatch::try_new(
        Arc::clone(&schema),
        vec![
            Arc::new(Int64Array::from_iter_values((0..rows).map(|offset| {
                sequence.saturating_add(i64::try_from(offset).expect("bounded native row offset"))
            }))) as ArrayRef,
        ],
    )
    .expect("native Forge value batch");
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, schema.as_ref()).expect("native IPC writer");
    writer.write(&batch).expect("native IPC batch");
    writer.finish().expect("native IPC finish");
    bytes
}

/// Rows sent by each bounded public-ingest request in the convergence journey.
pub(crate) const FORGE_CONVERGENCE_BATCH_ROWS: usize = 5_000;

/// Sizes one journey task from the same config-clamped live governor capacity as production.
pub(crate) fn journey_envelope(
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
pub(crate) struct JourneyMaintenance {
    pub(crate) operator_pool: vala_sql::OperatorPool,
    pub(crate) tenant: DataTenantId,
    /// Deterministic catalog boundaries shared with the production lifecycle.
    pub(crate) maintenance_controls: vala_bifrost_redux::forge::MaintenanceTestControls,
    /// Passive trigger driving deterministic production scheduler passes.
    pub(crate) scheduler_trigger: ForgeSchedulerTrigger,
    /// Observer proving which durable worker strategy completed.
    pub(crate) worker_observer: ForgeWorkerCompletionObserver,
    /// Cancellation boundary for the production scheduler loop.
    pub(crate) scheduler_stop: CancellationToken,
    /// Cancellation boundary for the production worker loop.
    pub(crate) worker_stop: CancellationToken,
    /// Running production scheduler task.
    pub(crate) scheduler_task: JoinHandle<Result<(), ForgeError>>,
    /// Running production worker task, consumed during shutdown.
    pub(crate) worker_task: Option<JoinHandle<Result<(), ForgeError>>>,
}

impl JourneyMaintenance {
    /// Starts production scheduler and worker supervision for one fixture.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot construct the configured production
    /// scheduler or worker.
    pub(crate) fn start(
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
        let maintenance_controls = forge.maintenance_controls_for_test();
        let scheduler_task = tokio::spawn({
            let stop = scheduler_stop.clone();
            async move { forge.run(stop).await }
        });
        let worker_task = tokio::spawn({
            let stop = worker_stop.clone();
            async move { worker.run(stop).await }
        });
        Self {
            operator_pool: fixture.operator_pool.clone(),
            tenant: fixture.tenant,
            maintenance_controls,
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
    pub(crate) async fn run_one_success(&mut self) -> ForgeTaskStrategy {
        self.run_one_success_at("maintenance").await
    }

    /// Runs one completed attempt with a diagnostic stage name.
    pub(crate) async fn run_one_success_at(&mut self, stage: &str) -> ForgeTaskStrategy {
        let expected_passes = self.scheduler_trigger.completed_passes().saturating_add(1);
        let expected_attempts = self.worker_observer.attempts().saturating_add(1);
        let expected_completions = self.worker_observer.completed().saturating_add(1);
        self.worker_observer.hold_after_next_attempt_for_test();
        self.scheduler_trigger.request_pass();
        tokio::time::timeout(
            Duration::from_secs(30),
            self.scheduler_trigger
                .wait_for_passes_at_least(expected_passes),
        )
        .await
        .expect("journey maintenance scheduler pass bound");
        let attempt_wait = tokio::time::timeout(
            Duration::from_secs(30),
            self.worker_observer.wait_for_held_attempt_for_test(),
        )
        .await;
        if let Err(error) = attempt_wait {
            let (demands, tasks): (i64, i64) = sqlx::query_as(
                "SELECT (SELECT count(*) FROM vala.forge_planning_demands WHERE data_tenant_id=$1), (SELECT count(*) FROM vala.forge_tasks WHERE data_tenant_id=$1)",
            )
            .bind(self.tenant.as_uuid())
            .fetch_one(self.operator_pool.pool())
            .await
            .expect("journey timeout durable diagnostics");
            panic!(
                "journey maintenance worker attempt bound at {stage}: {error}; scheduler_finished={} worker_finished={} demands={demands} tasks={tasks}",
                self.scheduler_task.is_finished(),
                self.worker_task
                    .as_ref()
                    .is_some_and(JoinHandle::is_finished)
            )
        }
        assert_eq!(self.worker_observer.attempts(), expected_attempts);
        self.stop_worker().await;
        assert_eq!(
            self.worker_observer.completed(),
            expected_completions,
            "held maintenance attempt errors: {:?}",
            self.worker_observer.returned_errors()
        );
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

    /// Runs one complete scheduler pass without requiring a worker attempt.
    pub(crate) async fn run_scheduler_pass(&self) {
        let expected_passes = self.scheduler_trigger.completed_passes().saturating_add(1);
        self.scheduler_trigger.request_pass();
        tokio::time::timeout(
            Duration::from_secs(30),
            self.scheduler_trigger
                .wait_for_passes_at_least(expected_passes),
        )
        .await
        .expect("journey maintenance scheduler-only pass bound");
    }

    /// Cancels and joins the production worker loop once.
    ///
    /// # Panics
    ///
    /// Panics when the worker does not shut down within the bounded test window.
    pub(crate) async fn stop_worker(&mut self) {
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
    pub(crate) async fn shutdown(mut self) {
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

/// Proves temporary root pressure defers durably and releases atomically.
pub(crate) async fn supervised_temporary_pressure_defers_without_ownership() {
    let observer = ForgeWorkerCompletionObserver::new();
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .with_forge_completion_observer_for_test(observer.clone())
        .start_bound()
        .await
        .expect("bound temporary-pressure server");
    let fixture = native_forge_group(&server, "journey_temporary_pressure").await;
    server
        .forge_clock()
        .advance(chrono::Duration::days(1) + chrono::Duration::minutes(3))
        .expect("age temporary-pressure inputs");
    let resources = server
        .state()
        .forge()
        .expect("temporary-pressure Forge")
        .resources();
    let baseline = resources.snapshot().expect("temporary-pressure baseline");
    let forge_baseline = (
        baseline.forge_memory_used_bytes,
        baseline.elastic_memory_used_bytes,
        baseline.scratch_used_bytes,
        baseline.forge_reader_permits_used,
    );
    let held = server
        .hold_forge_root_capacity_for_test()
        .expect("hold real Forge root capacity");
    let expected_attempts = observer.attempts().saturating_add(1);
    trigger_supervised_scheduler(
        &server,
        Scenario {
            name: "temporary-pressure-refusal",
            tenants: 1,
        },
    )
    .await;
    tokio::time::timeout(
        Duration::from_secs(30),
        observer.wait_for_attempts_at_least(expected_attempts),
    )
    .await
    .expect("supervised capacity refusal");
    let deferred: (String, bool, bool, i32, Option<String>) = sqlx::query_as(
        "SELECT state,attempt_id IS NULL,claimed_by IS NULL,attempt_count,failure_class FROM vala.forge_tasks WHERE data_tenant_id=$1 AND table_name=$2 AND strategy='staging_fold'",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("durable temporary-pressure refusal");
    assert!(deferred.1, "refusal retains no attempt identity");
    assert!(deferred.2, "refusal retains no claim owner");
    assert_eq!(deferred.3, 0, "refusal consumes no attempt budget");
    assert_eq!(deferred.4.as_deref(), Some("capacity_refused"));
    drop(held);
    let expected = observer.completed().saturating_add(1);
    trigger_supervised_scheduler(
        &server,
        Scenario {
            name: "temporary-pressure-release",
            tenants: 1,
        },
    )
    .await;
    tokio::time::timeout(
        Duration::from_secs(30),
        observer.wait_for_at_least(expected),
    )
    .await
    .expect("supervised progress after capacity release");
    let settled: (String, bool, bool) = sqlx::query_as(
        "SELECT state,attempt_id IS NULL,claimed_by IS NULL FROM vala.forge_tasks WHERE data_tenant_id=$1 AND table_name=$2 AND strategy='staging_fold'",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("supervised temporary-pressure settlement");
    assert_eq!(settled, ("succeeded".to_owned(), true, true));
    let released = resources
        .snapshot()
        .expect("released temporary-pressure root");
    assert_eq!(
        (
            released.forge_memory_used_bytes,
            released.elastic_memory_used_bytes,
            released.scratch_used_bytes,
            released.forge_reader_permits_used,
        ),
        forge_baseline,
        "RAII release restores every Forge-owned root dimension"
    );
    server
        .shutdown()
        .await
        .expect("temporary-pressure shutdown");
}

/// Starts an isolated production server for one maintenance lifecycle journey.
///
/// # Panics
///
/// Panics when the local Postgres, catalog, storage, or server fixture cannot
/// start with its background Forge interval disabled for deterministic control,
/// or when it unexpectedly exposes a bound endpoint that would imply a
/// background Forge worker role.
///
/// The journey's own [`JourneyMaintenance`] must be the only Forge scheduler and
/// worker lifecycle. A `BifrostTarget::All` server keeps polling the durable
/// queue regardless of its scheduler interval, so it claims the task before the
/// test-owned worker ever sees it and the journey times out with work still
/// pending. This mirrors `start_engine_fixture_server`'s rule.
pub(crate) async fn start_maintenance_journey_server() -> WyrdTestServer {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .with_forge_process_role_for_test(BifrostTarget::Server)
        .start_in_process()
        .await
        .expect("in-process maintenance journey server");
    assert!(
        server.base_url().is_none(),
        "maintenance journey fixture must not start a background Forge worker role"
    );
    server
}

/// Start an isolated maintenance server attached to the shared production telemetry capture.
///
/// # Panics
///
/// Panics when the process-global recorder or isolated server cannot start, or
/// when the server unexpectedly exposes a bound endpoint that would imply a
/// background Forge worker role.
///
/// Both consumers supervise their own worker, so the server must not run one:
/// a `BifrostTarget::All` server polls the durable queue regardless of its
/// scheduler interval and races the test-owned worker for the same task. This
/// mirrors `start_maintenance_journey_server` and `start_engine_fixture_server`.
pub(crate) async fn start_telemetry_maintenance_server() -> (
    WyrdTestServer,
    wyrd_testing::bifrost::BifrostTelemetryCapture,
) {
    let (telemetry_guard, telemetry) =
        shared_process_telemetry_for_test().expect("shared production telemetry runtime");
    let server = WyrdTestServer::builder()
        .with_telemetry_for_test(telemetry_guard)
        .with_forge_interval(Duration::from_secs(3600))
        .with_forge_process_role_for_test(BifrostTarget::Server)
        .start_in_process()
        .await
        .expect("in-process telemetry maintenance server");
    assert!(
        server.base_url().is_none(),
        "telemetry maintenance fixture must not start a background Forge worker role"
    );
    (server, telemetry)
}

/// Commits one staging-fold snapshot through a real production scheduler and worker.
///
/// # Panics
///
/// Panics when the fixture cannot create a successful staging-fold task within
/// two bounded production passes.
pub(crate) async fn commit_journey_staging_snapshot(
    server: &WyrdTestServer,
    fixture: &wyrd_testing::bifrost::ForgeFixture,
) {
    server
        .forge_clock()
        .advance(chrono::Duration::days(1))
        .expect("native Forge partition age advance must remain in range");
    let pending: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.file_list WHERE data_tenant_id=$1 AND namespace=$2 AND table_name=$3 AND committed_snapshot_id IS NULL",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("journey staging debt count");
    assert!(pending > 0, "journey staging commit requires native debt");
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

/// Advances one exact, durably classified transient Forge retry in the journey.
///
/// The guarded update repeats every classification and ownership predicate at
/// the mutation boundary. `mature_same_volume` is reserved for convergence
/// fixtures that intentionally cross the production same-volume fallback age;
/// takeover proofs leave that bound intact.
pub(crate) async fn advance_transient_forge_retry(
    server: &WyrdTestServer,
    task_id: uuid::Uuid,
    expected_class: &str,
    mature_same_volume: bool,
) {
    assert!(
        matches!(
            expected_class,
            "transient_object_store" | "transient_coordination" | "storage_health"
        ),
        "journey retry helper accepts only closed transient classes"
    );
    let pool = server
        .state()
        .postgres
        .operator_pool()
        .expect("Forge retry operator pool");
    let updated = if mature_same_volume {
        sqlx::query(
            "UPDATE vala.forge_tasks SET ready_at=statement_timestamp(),next_eligible_at=statement_timestamp()-interval '15 minutes' \
             WHERE task_id=$1 AND state='retryable' AND failure_class=$2 \
             AND attempt_id IS NULL AND claimed_by IS NULL",
        )
        .bind(task_id)
        .bind(expected_class)
        .execute(pool.pool())
        .await
    } else {
        sqlx::query(
            "UPDATE vala.forge_tasks SET ready_at=statement_timestamp(),next_eligible_at=statement_timestamp() \
             WHERE task_id=$1 AND state='retryable' AND failure_class=$2 \
             AND attempt_id IS NULL AND claimed_by IS NULL",
        )
        .bind(task_id)
        .bind(expected_class)
        .execute(pool.pool())
        .await
    }
    .expect("advance exact classified Forge retry")
    .rows_affected();
    assert_eq!(
        updated, 1,
        "exact Forge retry row must remain unclaimed and retain class {expected_class}"
    );
}

/// Exercises production fail-stop supervision and exact recovery after Forge panics.
///
/// # Panics
///
/// Panics when supervision does not fail stop, the durable attempt cannot be
/// reclaimed, publication is duplicated, or attempt resources remain owned.
pub(crate) async fn supervised_forge_panic_recovery_scenario() {
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
        let nodes = cluster.configured_node_ids();
        let scheduler_node = nodes[0];
        let server = cluster
            .server_by_node(scheduler_node)
            .expect("panic recovery scheduler server");
        let table_name = if after_commit {
            "forge_panic_after_commit"
        } else {
            "forge_panic_before_commit"
        };
        let fixture = native_forge_group(server, table_name).await;
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let persisted: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM vala.forge_planning_demands \
                     WHERE data_tenant_id=$1 AND catalog_name=$2 \
                     AND namespace_name=$3 AND table_name=$4)",
                )
                .bind(fixture.tenant.as_uuid())
                .bind("wyrd-redux")
                .bind(&fixture.binding.logical_namespace)
                .bind(&fixture.binding.table_name)
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("observe panic-recovery hint persistence");
                if persisted {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("production Forge hint owner persisted panic-recovery demand");
        server
            .forge_clock()
            .advance(chrono::Duration::days(1) + chrono::Duration::minutes(3))
            .expect("advance panic-recovery debt past the production retention cutoff");
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
        let node = cluster
            .forge_completion_observer()
            .expect("panic recovery completion observer")
            .lifecycle_events()
            .into_iter()
            .rev()
            .find_map(|event| match event {
                vala_bifrost_redux::forge::ForgeLifecycleEvent::Claimed { worker_id, .. } => {
                    Some(wyrd_spec::vala::api::NodeId::new(worker_id))
                }
                _ => None,
            })
            .expect("panic recovery production worker claim");
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
        let retained: (uuid::Uuid, uuid::Uuid, uuid::Uuid, String, i32) = sqlx::query_as(
            "SELECT task_id,attempt_id,claimed_by,state,attempt_count FROM vala.forge_tasks WHERE data_tenant_id=$1 AND table_name=$2 AND state IN ('claimed','running') ORDER BY created_at DESC LIMIT 1",
        )
        .bind(fixture.tenant.as_uuid())
        .bind(&fixture.binding.table_name)
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("panic-retained durable attempt");
        assert_eq!(retained.3, "running");
        assert_eq!(retained.4, 0);
        tokio::time::timeout(Duration::from_secs(12), async {
            loop {
                let expired: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM vala.forge_tasks WHERE task_id=$1 AND attempt_id=$2 AND claimed_by=$3 AND state='running' AND claim_expires_at < statement_timestamp())",
                )
                .bind(retained.0)
                .bind(retained.1)
                .bind(retained.2)
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("observe exact panic-retained claim expiry");
                if expired {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("panic-retained claim expires before node restart");
        cluster
            .restart_node(node)
            .await
            .expect("fresh server restart");
        let observer = cluster
            .forge_completion_observer()
            .expect("panic recovery completion observer");
        let expected_completions = observer.completed().saturating_add(1);
        cluster.request_forge_scheduler_pass_for_test();
        tokio::time::timeout(
            Duration::from_secs(90),
            observer.wait_for_at_least(expected_completions),
        )
        .await
        .expect("restarted production worker settles retained task");
        let unrecovered_inputs: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.file_list WHERE data_tenant_id=$1 AND namespace=$2 AND table_name=$3 AND file_path LIKE '%/journey-%' AND NOT compacted",
        )
        .bind(fixture.tenant.as_uuid())
        .bind(&fixture.binding.logical_namespace)
        .bind(&fixture.binding.table_name)
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("inspect exact panic-recovered task inputs");
        assert_eq!(unrecovered_inputs, 0);
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
        let restarted = cluster
            .server_by_node(node)
            .expect("settled restart server");
        let resources = restarted
            .state()
            .forge()
            .expect("restarted Forge")
            .resources()
            .snapshot()
            .expect("panic recovery resource snapshot");
        assert_eq!(resources.elastic_memory_used_bytes, 0);
        assert_eq!(resources.scratch_used_bytes, 0);
        assert_eq!(resources.forge_reader_permits_used, 0);
        cluster.shutdown().await.expect("panic recovery shutdown");
    }
}

/// Read the sorted live data-file membership of the current Iceberg snapshot.
pub(crate) async fn current_live_paths(
    table: &iceberg::table::Table,
) -> std::collections::BTreeSet<String> {
    let snapshot = table
        .metadata()
        .current_snapshot()
        .expect("publication table current snapshot");
    let manifest_list = table
        .manifest_list_reader(snapshot)
        .load()
        .await
        .expect("publication current manifest list");
    let mut paths = std::collections::BTreeSet::new();
    for manifest_file in manifest_list.entries() {
        let manifest = manifest_file
            .load_manifest(table.file_io())
            .await
            .expect("publication current manifest");
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

/// Drives one abandoned durable claim through expiry and exact worker reclaim.
///
/// # Panics
///
/// Panics when claim expiry, successor ownership, public rows, durable evidence,
/// audit settlement, or lease release diverges from the production contract.
pub(crate) async fn supervised_worker_claim_expiry_journey() {
    let config = ForgeConfig {
        lease_ttl: Duration::from_secs(4),
        iceberg_total_retry_timeout: Duration::from_secs(1),
        catalog_request_timeout: Duration::from_secs(1),
        uncertainty_margin: Duration::from_secs(1),
        uncertainty_bound: Duration::from_secs(1),
        ..ForgeConfig::default()
    };
    let cluster = WyrdTestCluster::start_with_dedicated_forge_workers_with_config_for_test(config)
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
    let abandoned_identity = completion.abandoned_claim_identity_for_test();
    let staged_completion = tokio::time::timeout(
        Duration::from_secs(90),
        completion.wait_for_strategy_at_least(ForgeTaskStrategy::StagingFold, 3),
    )
    .await;
    if staged_completion.is_err() {
        panic!(
            "remaining supervised workers did not reclaim staged tasks: {:?}",
            forge_task_diagnostics(server).await
        );
    }
    let reclaimed_by_other_worker = completion.lifecycle_events().iter().any(|event| {
        matches!(event, vala_bifrost_redux::forge::ForgeLifecycleEvent::Claimed { task_id, worker_id, .. }
            if *task_id == abandoned_identity.0 && *worker_id != abandoned_identity.2)
    });
    assert!(
        reclaimed_by_other_worker,
        "the abandoned task is reclaimed by a different production worker"
    );
    assert!(completion.lifecycle_events().iter().any(|event| {
        matches!(event, vala_bifrost_redux::forge::ForgeLifecycleEvent::Terminal { task_id, worker_id }
            if *task_id == abandoned_identity.0 && *worker_id != abandoned_identity.2)
    }), "the different reclaiming worker terminalizes the abandoned task");

    server
        .bifrost_scribe()
        .expect("server-role Scribe")
        .retire_committed_for_test()
        .await
        .expect("retire committed Scribe generations");
    let expected_rows = u64::try_from(WRITE_CYCLES).expect("bounded reclaim rows");
    for tenant in &tenants {
        assert_terminal_audits(server, tenant.id, scenario).await;
        assert_snapshot(server, tenant, scenario).await;
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
            supervised_reclaim_query_rows(server, tenant).await,
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

/// Implements the uncertain-commit recovery product journey.
///
/// # Panics
///
/// Panics when a prepared uncertain publication is not reconciled exactly once
/// through the dedicated production worker topology.
pub(crate) async fn supervised_uncertain_commit_recovery_journey() {
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
    let expected_rows = u64::try_from(WRITE_CYCLES).expect("bounded uncertain rows");
    assert_terminal_audits(server, tenants[0].id, scenario).await;
    assert_snapshot(server, &tenants[0], scenario).await;
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
        query_rows(server, &tenants[0]).await,
        expected_rows,
        "uncertain commit public rows after terminal={terminal}, evidence={evidence}, files={files}, compacted={compacted}, committed={committed}"
    );
    assert_no_forge_leases(server, scenario).await;
    cluster
        .shutdown()
        .await
        .expect("dedicated cluster shutdown");
}

/// Implements the bounded planner-admission product journey.
///
/// # Panics
///
/// Panics when production planner admission fails to retain the exact large or
/// unschedulable durable task classification.
pub(crate) async fn dedicated_unschedulable_admission_journey() {
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

/// Return durable task strategy, state, owner, and expiry data for a failing role journey.
///
/// # Panics
///
/// Panics when platform-admin task inspection fails during failure reporting.
pub(crate) async fn forge_task_diagnostics(
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

/// Request and await one pass from the server's already-supervised scheduler.
///
/// This preserves the real server role's durable scheduler owner and avoids a
/// test-created scheduler bypassing lifecycle composition.
///
/// # Panics
///
/// Panics when the supervised scheduler does not return one pass inside the
/// bounded journey deadline.
pub(crate) async fn trigger_supervised_scheduler(server: &WyrdTestServer, scenario: Scenario) {
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
pub(crate) struct TenantWriter {
    /// Durable tenant isolation key.
    pub(crate) id: DataTenantId,
    /// Tenant-scoped access token accepted by the public query endpoint.
    pub(crate) jwt: String,
    /// API key used by the public native Arrow SDK transport.
    pub(crate) api_key: SecretString,
    /// Tenant-local native Bifrost table receiving the journey rows.
    pub(crate) table: String,
}

/// Provision the requested number of isolated tenants and writer identities.
///
/// # Panics
///
/// Panics when tenant, service-account, API-key, or access-token provisioning
/// fails.
pub(crate) async fn provision_tenants(
    server: &WyrdTestServer,
    scenario: Scenario,
) -> Vec<TenantWriter> {
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
        let table = format!("forge_journey_{}", uuid::Uuid::now_v7().simple());
        server
            .create_bifrost_table_for_test(CreateTableRequest {
                table: TableRef::new(BifrostNamespace::Bifrost, &table),
                user_fields: vec![
                    Field::new("id", DataType::Int64, false),
                    Field::new("value", DataType::Utf8, false),
                ],
                tenant: id,
                physical_layout: None,
                audit: None,
            })
            .await
            .unwrap_or_else(|error| {
                panic!("{} provision native table for {id}: {error}", scenario.name)
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
        tenants.push(TenantWriter {
            id,
            jwt,
            api_key,
            table,
        });
    }
    tenants
}

/// Assert the scheduler's operator-visible roster contains every native journey table.
///
/// This journey writes only the canonical traces spans table, so a row for each
/// tenant proves Forge will discover the intended table independently of
/// staging-file history.
///
/// # Panics
///
/// Panics when the operator roster query fails or a canonical tenant table is
/// missing.
pub(crate) async fn assert_active_traces_roster(
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
                row.data_tenant_id == tenant.id.as_uuid()
                    && row.fqn == format!("vala.bifrost.{}", tenant.table)
            }),
            "{} missing active native-table roster entry for tenant {}: {roster:?}",
            scenario.name,
            tenant.id
        );
    }
}

/// Send one concurrent native Arrow SDK write from every pod for every tenant.
///
/// # Panics
///
/// Panics when a bound gRPC endpoint cannot connect or rejects an authenticated
/// export request.
pub(crate) async fn write_cycle(
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
            let http_endpoint = server.base_url().expect("bound pod HTTP URL").to_owned();
            let api_key = tenant.api_key.clone();
            let table = tenant.table.clone();
            writers.push(tokio::spawn(async move {
                let seed =
                    u64::try_from((cycle + 1) * 10_000 + (pod_index + 1) * 100 + tenant_index)
                        .expect("bounded seed");
                let client = WyrdClient::with_config(ClientConfig {
                    grpc: GrpcConfig {
                        endpoint,
                        connect_retries: 3,
                        ..GrpcConfig::default()
                    },
                    http: HttpConfig {
                        base_url: http_endpoint,
                        ..HttpConfig::default()
                    },
                    api_key: Some(api_key),
                    ..ClientConfig::default()
                })
                .expect("native Arrow client config");
                let transport = BifrostGrpcTransport::connect(&client)
                    .await
                    .expect("connect native Arrow writer");
                transport
                    .insert_batch(
                        &format!("vala.bifrost.{table}"),
                        uuid::Uuid::now_v7().into_bytes(),
                        native_journey_ipc(seed),
                    )
                    .await
                    .expect("native Arrow write");
            }));
        }
    }
    for writer in writers {
        writer
            .await
            .unwrap_or_else(|error| panic!("{} writer task: {error}", scenario.name));
    }
    for server in servers {
        for tenant in tenants {
            server
                .flush_bifrost_for_tenant(tenant.id)
                .await
                .expect("native Arrow write durable Scribe flush");
        }
    }
}

/// Encodes one deterministic native Arrow row for the public SDK fixture.
pub(crate) fn native_journey_ipc(sequence: u64) -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("value", DataType::Utf8, false),
    ]));
    let batch = RecordBatch::try_new(
        Arc::clone(&schema),
        vec![
            Arc::new(Int64Array::from(vec![
                i64::try_from(sequence).expect("bounded sequence"),
            ])) as ArrayRef,
            Arc::new(StringArray::from(vec!["forge-write"])) as ArrayRef,
        ],
    )
    .expect("native journey batch");
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, schema.as_ref()).expect("native IPC writer");
    writer.write(&batch).expect("native IPC batch");
    writer.finish().expect("native IPC finish");
    bytes
}

/// Send the public same-table query used before and after Forge publication.
///
/// # Panics
///
/// Panics when the HTTP request cannot be sent.
pub(crate) async fn query_response(
    server: &WyrdTestServer,
    tenant: &TenantWriter,
) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!(
            "{}/v1/query",
            server.base_url().expect("bound server URL")
        ))
        .header("x-wyrd-access-token", format!("Bearer {}", tenant.jwt))
        .json(&BifrostQueryRequest {
            sql: format!("SELECT * FROM \"vala.bifrost.{}\"", tenant.table),
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
pub(crate) async fn query_rows(server: &WyrdTestServer, tenant: &TenantWriter) -> u64 {
    let response = query_response(server, tenant).await;
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
pub(crate) async fn supervised_reclaim_query_rows(
    server: &WyrdTestServer,
    tenant: &TenantWriter,
) -> u64 {
    query_rows(server, tenant).await
}

/// Assert every prepared operation has one terminal event and ordered outputs.
///
/// # Panics
///
/// Panics when audit inspection fails, transitions do not reconcile, or a
/// terminal compaction lacks deterministically ordered output paths.
pub(crate) async fn assert_terminal_audits(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    scenario: Scenario,
) {
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

/// Assert the tenant's native journey table has at least one committed snapshot.
///
/// # Panics
///
/// Panics when the Redux catalog or table cannot be loaded.
pub(crate) async fn assert_snapshot(
    server: &WyrdTestServer,
    tenant: &TenantWriter,
    scenario: Scenario,
) {
    let binding = vala_bifrost_redux::catalog::TenantTableBinding::resolve((
        tenant.id,
        vala_bifrost_redux::catalog::TableRef::new(
            vala_bifrost_redux::namespaces::BifrostNamespace::Bifrost,
            &tenant.table,
        ),
    ))
    .expect("native journey binding");
    let table = server
        .state()
        .bifrost_catalog()
        .expect("Bifrost catalog")
        .iceberg_catalog()
        .load_table(&binding.table_ident())
        .await
        .unwrap_or_else(|error| {
            panic!("{} load tenant {} table: {error}", scenario.name, tenant.id)
        });
    assert!(
        table.metadata().current_snapshot_id().is_some(),
        "{} tenant {} missing snapshot",
        scenario.name,
        tenant.id
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
pub(crate) async fn forge_lease_count(server: &WyrdTestServer) -> i64 {
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
pub(crate) async fn assert_no_forge_leases(server: &WyrdTestServer, scenario: Scenario) {
    let leases = forge_lease_count(server).await;
    assert_eq!(leases, 0, "{} stale Forge leases", scenario.name);
}
