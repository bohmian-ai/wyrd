//! Distributed product journey for the real Scribe-to-Forge publication path.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use opendal::Buffer;
use secrecy::SecretString;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::forge::{
    ForgeError, ForgeLease, ForgeScheduler, ForgeSchedulerTrigger, ForgeWorker,
    ForgeWorkerCompletionObserver, ForgeWorkerConfig,
};
use vala_bifrost_redux::maintenance::{StagingFileCommitted, StagingPublishOutcome};
use vala_sql::queries::forge_tasks::ForgeTasks;
use vala_sql::row_types::forge_tasks::{
    FORGE_TASK_PAYLOAD_VERSION, ForgeClaimStrategy, ForgeTaskEstimates, ForgeTaskLane,
    ForgeTaskPlan, ForgeTaskStrategy, ForgeTaskTableIdentity, NewForgeTask,
};
use wyrd_server::config::ForgeProcessRole;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{AuditDetail, ForgeCompactionPhase, StoragePath, SyncQueryRequest};
use wyrd_testing::bifrost::forge_harness::seed_forge_group;
use wyrd_testing::bifrost::{
    BifrostQueryTelemetryReport, BifrostTopology, WyrdTestCluster,
    shared_process_telemetry_for_test,
};
use wyrd_testing::otlp::RandomTraceGenerator;
use wyrd_testing::{Bootstrap, WyrdTestServer};

/// One deployment and writer-concurrency shape exercised by the journey.
#[derive(Clone, Copy)]
struct Scenario {
    /// Diagnostic name included in assertion failures.
    name: &'static str,
    /// Number of bound Wyrd server processes sharing durable state.
    pods: usize,
    /// Number of isolated tenants writing concurrently.
    tenants: usize,
}

/// Product deployment shapes that Forge must support.
const SCENARIOS: [Scenario; 3] = [
    Scenario {
        name: "single-node-single-tenant",
        pods: 1,
        tenants: 1,
    },
    Scenario {
        name: "single-node-multi-tenant-multi-writer",
        pods: 1,
        tenants: 10,
    },
    Scenario {
        name: "multi-node-multi-tenant-multi-writer",
        pods: 3,
        tenants: 10,
    },
];

/// Number of independent flush cycles produced by every pod for every tenant.
///
/// Multiple cycles create independent Scribe files so active-day Forge
/// compaction is exercised instead of leaving a single open-tail file.
const WRITE_CYCLES: usize = 3;

/// Number of spans carried by each OTLP writer request.
const SPANS_PER_WRITE: usize = 6;

redacted
const FORGE_PARITY_TARGET_COMMIT: &str = "d4a8483f7f0df0d27ccf0ec39495661cb476c4c0";

/// Exact journey assertions selected by the versioned Forge parity ledger.
const FORGE_PARITY_ASSERTIONS: [&str; 6] = [
    "forge.continuous-triggering",
    "forge.dedicated-workers",
    "forge.bin-packing",
    "forge.snapshot-expiry",
    "forge.orphan-cleanup",
    "forge.failover",
];

/// Stable scheduler identity for receipt-owned lifecycle proof fixtures.
const PARITY_MAINTENANCE_SCHEDULER_OWNER: u128 = 0x0198_39f4_2b51_7000_8000_0000_0000_0005;

/// One bounded production scheduler and worker pair for a lifecycle proof.
struct ParityMaintenance {
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

impl ParityMaintenance {
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
            PARITY_MAINTENANCE_SCHEDULER_OWNER,
        ));
        let worker_observer = ForgeWorkerCompletionObserver::new();
        let (forge, _) = fixture.context_with_worker_supervision(
            config,
            Arc::clone(&fixture.catalog),
            Arc::clone(&fixture.object_store),
            worker_observer.clone(),
            scheduler_trigger.clone(),
        );
        let worker = ForgeWorker::new(Arc::clone(&forge), ForgeWorkerConfig::default())
            .expect("validated parity maintenance worker");
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
        .expect("parity maintenance scheduler pass bound");
        tokio::time::timeout(
            Duration::from_secs(30),
            self.worker_observer.wait_for_held_attempt_for_test(),
        )
        .await
        .expect("parity maintenance worker attempt bound");
        assert_eq!(self.worker_observer.attempts(), expected_attempts);
        assert_eq!(self.worker_observer.returned_errors().len(), errors);
        self.stop_worker().await;
        assert_eq!(self.worker_observer.completed(), expected_completions);
        match self
            .worker_observer
            .completed_strategies()
            .last()
            .expect("completed parity maintenance strategy")
        {
            ForgeClaimStrategy::Known(strategy) => *strategy,
            ForgeClaimStrategy::Unknown(strategy) => {
                panic!("parity maintenance completed an unknown strategy: {strategy}")
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
        let task = self.worker_task.take().expect("parity worker stops once");
        tokio::time::timeout(Duration::from_secs(30), task)
            .await
            .expect("parity maintenance worker shutdown bound")
            .expect("parity maintenance worker task")
            .expect("parity maintenance worker shutdown");
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
            .expect("parity maintenance scheduler shutdown bound")
            .expect("parity maintenance scheduler task")
            .expect("parity maintenance scheduler shutdown");
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

/// Typed server-integrated query artifact joined with Forge reports by scenario.
#[derive(serde::Serialize)]
struct QueryTelemetryArtifact {
    /// Immutable process and tenant identity shared with the Forge benchmark.
    scenario: QueryScenarioIdentity,
    /// Production query histogram mapped by the server-only report owner.
    query: BifrostQueryTelemetryReport,
}

/// Exact scenario join key shared by independent maintenance and query artifacts.
#[derive(serde::Serialize)]
struct QueryScenarioIdentity {
    /// Number of serving/scheduling pods in the integrated journey.
    pods: usize,
    /// Number of isolated tenants queried in the integrated journey.
    tenants: usize,
}

/// One completed assertion retained in the atomic distributed-journey receipt.
#[derive(serde::Serialize)]
struct ForgeParityAssertion {
    /// Stable ledger assertion identity.
    id: &'static str,
    /// Whether the production-path assertion completed successfully.
    passed: bool,
}

/// Versioned proof emitted only after all selected distributed assertions pass.
#[derive(serde::Serialize)]
struct ForgeParityProofReceipt {
    /// Wire version accepted by the fail-closed parity matrix.
    proof_version: &'static str,
redacted
    target_commit: &'static str,
    /// Exact ignored integration-test identity that produced this receipt.
    test_identity: &'static str,
    /// Stable identity joining this receipt to every journey ledger row.
    result_id: String,
    /// Whether every selected assertion passed before receipt publication.
    passed: bool,
    /// Exact selected assertions, each recorded as a passing production proof.
    assertions: Vec<ForgeParityAssertion>,
}

#[tokio::test]
#[ignore = "gated journey: real bound Wyrd servers, Postgres, Scribe, Forge, and Iceberg"]
/// Validate supported deployment shapes through the complete production path.
///
/// Every case drives concurrent authenticated OTLP writers into server-owned
/// Scribe instances, flushes through the normal durable seal path, competes
/// server-owned Forge handles over shared leases, and reads the resulting
/// Iceberg table through the public query API. The assertions cover exact
/// tenant row isolation, terminal audit reconciliation, ordered output paths,
/// snapshots, lease cleanup, and supervised shutdown.
///
/// # Panics
///
/// Panics when infrastructure cannot start or any production-path invariant
/// diverges or the supervised harness cannot shut down cleanly.
async fn forge_distributed_writer_matrix_preserves_rows_and_converges_once() {
    forge_distributed_writer_matrix_journey().await;
}

/// Implements the distributed writer matrix so the parity receipt can reuse its proof.
///
/// # Panics
///
/// Panics when a supported deployment shape violates the real production-path
/// row, audit, snapshot, lease, or shutdown assertions.
async fn forge_distributed_writer_matrix_journey() {
    for scenario in SCENARIOS {
        run_scenario(scenario).await;
    }
}

/// Run the selected distributed proofs and atomically publish their parity receipt.
///
/// The receipt is removed before the journey starts so a failing or interrupted
/// pass cannot leave stale success evidence. Each invoked helper drives its
/// existing real Postgres, Scribe, Forge, Iceberg, and public-query assertions.
/// The receipt is written only after all six ledger-selected assertions return.
///
/// # Panics
///
/// Panics when a selected production journey fails, the mandatory target commit
/// is absent or stale, or the receipt cannot be atomically written.
#[tokio::test]
#[ignore = "gated parity proof: real distributed Forge journey and atomic receipt"]
async fn forge_parity_proof_receipt_follows_all_selected_distributed_assertions() {
    let receipt_path = forge_parity_receipt_path();
    if receipt_path.exists() {
        std::fs::remove_file(&receipt_path).expect("remove stale Forge parity receipt");
    }

    let mut assertions = Vec::with_capacity(FORGE_PARITY_ASSERTIONS.len());
    forge_distributed_writer_matrix_journey().await;
    assertions.extend([
        ForgeParityAssertion {
            id: "forge.continuous-triggering",
            passed: true,
        },
        ForgeParityAssertion {
            id: "forge.bin-packing",
            passed: true,
        },
    ]);
    dedicated_forge_workers_journey().await;
    assertions.push(ForgeParityAssertion {
        id: "forge.dedicated-workers",
        passed: true,
    });
    forge_snapshot_expiry_journey().await;
    assertions.push(ForgeParityAssertion {
        id: "forge.snapshot-expiry",
        passed: true,
    });
    forge_orphan_cleanup_journey().await;
    assertions.push(ForgeParityAssertion {
        id: "forge.orphan-cleanup",
        passed: true,
    });
    dedicated_unschedulable_admission_journey().await;
    supervised_uncertain_commit_recovery_journey().await;
    assertions.push(ForgeParityAssertion {
        id: "forge.failover",
        passed: true,
    });
    write_forge_parity_receipt(&receipt_path, assertions)
        .expect("atomically write Forge parity receipt");
}

/// Resolve the required receipt path selected by the focused parity-proof task.
///
/// # Panics
///
/// Panics when the focused task does not provide the required proof path or
/// target revision, or when the revision differs from the pinned ledger target.
fn forge_parity_receipt_path() -> std::path::PathBuf {
    let target = std::env::var("WYRD_BIFROST_PARITY_TARGET_COMMIT")
        .expect("focused parity task sets WYRD_BIFROST_PARITY_TARGET_COMMIT");
    assert_eq!(
        target, FORGE_PARITY_TARGET_COMMIT,
        "focused parity target must match the pinned ledger revision"
    );
    std::env::var_os("WYRD_BIFROST_PARITY_PROOF")
        .map(std::path::PathBuf::from)
        .expect("focused parity task sets WYRD_BIFROST_PARITY_PROOF")
}

/// Validate and atomically write one complete distributed-parity proof receipt.
///
/// The temporary file is written beside the final destination and renamed only
/// after serialization succeeds. Invalid or incomplete assertion sets return
/// before a destination is created, so callers cannot publish partial proof.
///
/// # Errors
///
/// Returns an error when the selected assertion set is missing, duplicated, or
/// failed; when the destination has no parent directory; or when filesystem or
/// serialization operations prevent the atomic rename.
fn write_forge_parity_receipt(
    path: &std::path::Path,
    assertions: Vec<ForgeParityAssertion>,
) -> Result<(), String> {
    let mut assertion_ids = assertions
        .iter()
        .map(|assertion| assertion.id)
        .collect::<Vec<_>>();
    assertion_ids.sort_unstable();
    let mut expected = FORGE_PARITY_ASSERTIONS.to_vec();
    expected.sort_unstable();
    if assertion_ids != expected || assertions.iter().any(|assertion| !assertion.passed) {
        return Err(
            "Forge parity receipt requires every selected passing assertion exactly once"
                .to_owned(),
        );
    }
    let parent = path
        .parent()
        .ok_or_else(|| "Forge parity receipt path must have a parent directory".to_owned())?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("create Forge parity receipt directory: {error}"))?;
    let receipt = ForgeParityProofReceipt {
        proof_version: "wyrd.bifrost.forge-parity-proof/v1",
        target_commit: FORGE_PARITY_TARGET_COMMIT,
        test_identity: "pg_bifrost_forge_distributed_journey",
        result_id: format!("{FORGE_PARITY_TARGET_COMMIT}:pg_bifrost_forge_distributed_journey"),
        passed: true,
        assertions,
    };
    let bytes = serde_json::to_vec_pretty(&receipt)
        .map_err(|error| format!("serialize Forge parity receipt: {error}"))?;
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::now_v7()));
    std::fs::write(&temporary, bytes)
        .map_err(|error| format!("write temporary Forge parity receipt: {error}"))?;
    std::fs::rename(&temporary, path)
        .map_err(|error| format!("rename Forge parity receipt atomically: {error}"))
}

/// Refuses an injected failed assertion without creating a parity receipt.
#[test]
fn forge_parity_receipt_rejects_failed_assertion_without_output() {
    let directory =
        std::env::temp_dir().join(format!("wyrd-forge-parity-{}", uuid::Uuid::now_v7()));
    let path = directory.join("forge-parity-proof.json");
    let assertions = FORGE_PARITY_ASSERTIONS
        .into_iter()
        .map(|id| ForgeParityAssertion {
            id,
            passed: id != "forge.failover",
        })
        .collect::<Vec<_>>();
    assert!(write_forge_parity_receipt(&path, assertions).is_err());
    assert!(!path.exists());
    assert!(!directory.exists());
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
    commit_parity_staging_snapshot(&fixture).await;
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
                files: 1,
                bytes: 1,
                parallelism: 1,
                memory_bytes: 1,
                spill_bytes: 1,
                large_ceiling_bytes: 1,
            },
            ready_at: chrono::Utc::now(),
        })
        .await
        .expect("enqueue stale metric task");
    let before =
        rendered_staging_fold_task_duration_count(&telemetry.render(), "cancelled").unwrap_or(0.0);
    let worker = ForgeWorker::new(Arc::clone(&fixture.forge), ForgeWorkerConfig::default())
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

/// Starts an isolated production server for one receipt-owned lifecycle proof.
///
/// # Panics
///
/// Panics when the local Postgres, catalog, storage, or server fixture cannot
/// start with its background Forge interval disabled for deterministic control.
async fn start_parity_maintenance_server() -> WyrdTestServer {
    WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .with_forge_process_role_for_test(ForgeProcessRole::Server)
        .start_in_process()
        .await
        .expect("in-process parity maintenance server")
}

/// Start an isolated maintenance server attached to the shared production telemetry capture.
///
/// # Panics
///
/// Panics when the process-global recorder or isolated server cannot start.
async fn start_telemetry_maintenance_server()
-> (WyrdTestServer, wyrd_testing::bifrost::ForgeTelemetryCapture) {
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
async fn commit_parity_staging_snapshot(fixture: &wyrd_testing::bifrost::ForgeFixture) {
    let mut config = fixture.config.clone();
    config.max_files_per_bin = 2;
    config.max_files_per_tick = 2;
    for _ in 0..2 {
        let mut lifecycle = ParityMaintenance::start(fixture, config.clone());
        let strategy = lifecycle.run_one_success().await;
        lifecycle.shutdown().await;
        if strategy == ForgeTaskStrategy::StagingFold {
            return;
        }
    }
    panic!("bounded parity maintenance setup did not execute staging_fold");
}

/// Commits one production small-file rewrite required before maintenance proof.
///
/// # Panics
///
/// Panics when the real scheduler and worker do not complete the expected
/// SmallFiles task needed to leave only reset-generation objects for GC.
async fn commit_parity_live_rewrite(fixture: &wyrd_testing::bifrost::ForgeFixture) {
    let mut lifecycle = ParityMaintenance::start(fixture, fixture.config.clone());
    let strategy = lifecycle.run_one_success().await;
    lifecycle.shutdown().await;
    assert_eq!(
        strategy,
        ForgeTaskStrategy::SmallFiles,
        "parity orphan setup must drain its production live rewrite"
    );
}

/// Seeds one durable Reset generation whose output paths are GC-eligible.
///
/// The fixture invokes the same Prepared and Reset transition writers used by
/// the maintenance interleaving journey. The later receipt proof still drives
/// production scheduling, worker execution, eligibility checks, deletion, and
/// terminal orphan-GC audit; this helper only establishes the durable recovery
/// lineage that defines an eligible never-published output.
///
/// # Panics
///
/// Panics when the reset transition fixture cannot retain its exact input,
/// output, lease, or operation-state evidence.
async fn seed_parity_reset_generation(
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
    .expect("parity reset staging rows");
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
            .expect("parity reset generation object");
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
            .map(|row| StoragePath::new(row.1.clone()).expect("parity reset input storage path"))
            .collect(),
        output_paths: outputs
            .iter()
            .map(|path| StoragePath::new(path.clone()).expect("parity reset output storage path"))
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
    .expect("parity reset generation lease query")
    .expect("parity reset generation lease");
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
        .expect("prepared parity reset generation");
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
        .expect("terminal parity reset generation");
    lease
        .release(&fixture.operator_pool)
        .await
        .expect("parity reset generation lease release");
    assert_eq!(
        fixture
            .forge
            .reset_generation_paths_for_test(&fixture.binding)
            .await
            .expect("parity Reset generation projection"),
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
async fn forge_snapshot_expiry_journey() {
    let server = start_parity_maintenance_server().await;
    let fixture = seed_forge_group(&server, "parity_snapshot_expiry").await;
    commit_parity_staging_snapshot(&fixture).await;
    fixture.append_forge_file(2).await;
    fixture.append_forge_file(3).await;
    commit_parity_staging_snapshot(&fixture).await;
    let table = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("parity snapshot-expiry table");
    let newest_snapshot_ms = table
        .metadata()
        .snapshots()
        .map(|snapshot| snapshot.timestamp_ms())
        .max()
        .expect("parity snapshot-expiry retained history");
    server
        .forge_clock()
        .set(
            chrono::DateTime::from_timestamp_millis(newest_snapshot_ms + 2)
                .expect("parity snapshot timestamp is UTC-representable"),
        )
        .expect("advance parity expiry clock");
    let mut config = fixture.config.clone();
    config.snapshot_retention = Duration::from_millis(1);
    config.min_files = 3;
    config.max_files_per_bin = 3;
    config.max_files_per_tick = 3;
    let mut completed_expiry = false;
    for _ in 0..3 {
        let mut lifecycle = ParityMaintenance::start(&fixture, config.clone());
        let strategy = lifecycle.run_one_success().await;
        lifecycle.shutdown().await;
        if strategy == ForgeTaskStrategy::SnapshotExpiry {
            completed_expiry = true;
            break;
        }
    }
    assert!(
        completed_expiry,
        "production receipt journey must complete snapshot_expiry"
    );
    assert!(
        fixture
            .operation_count("forge.snapshot_expire.committed")
            .await
            >= 1,
        "production receipt journey must persist an expiry commit audit"
    );
    server
        .shutdown()
        .await
        .expect("parity expiry server shutdown");
}

/// Runs real orphan collection and proves an aged, unreferenced object is deleted.
///
/// # Panics
///
/// Panics when the production maintenance path does not complete SnapshotExpiry
/// and its coupled orphan collector, retain the object, or omit its terminal
/// orphan-GC audit.
async fn forge_orphan_cleanup_journey() {
    let server = start_parity_maintenance_server().await;
    let fixture = seed_forge_group(&server, "parity_orphan_cleanup").await;
    commit_parity_staging_snapshot(&fixture).await;
    let orphans = seed_parity_reset_generation(&fixture).await;
    commit_parity_staging_snapshot(&fixture).await;
    commit_parity_live_rewrite(&fixture).await;
    let modified = fixture
        .staging
        .stat(&orphans[0])
        .await
        .expect("parity orphan metadata")
        .last_modified()
        .expect("parity orphan modification time")
        .into_inner()
        .as_millisecond();
    server
        .forge_clock()
        .set(
            chrono::DateTime::from_timestamp_millis(modified + 100)
                .expect("parity orphan timestamp is UTC-representable"),
        )
        .expect("advance parity orphan clock");
    let mut config = fixture.config.clone();
    config.snapshot_retention = Duration::from_millis(1);
    config.orphan_gc_ttl = Duration::from_millis(1);
    config.min_files = 3;
    config.max_files_per_bin = 3;
    config.max_files_per_tick = 3;
    let mut lifecycle = ParityMaintenance::start(&fixture, config);
    let strategy = lifecycle.run_one_success().await;
    lifecycle.shutdown().await;
    assert_eq!(
        strategy,
        ForgeTaskStrategy::SnapshotExpiry,
        "production receipt journey must complete orphan maintenance"
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
        "production receipt journey must persist an orphan-GC terminal audit"
    );
    server
        .shutdown()
        .await
        .expect("parity orphan server shutdown");
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

/// Implements the dedicated-worker journey for direct and receipt-backed proof runs.
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
        .map(WyrdTestServer::forge_process_role)
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
        pods: 1,
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
    let scribe = scheduler_server
        .bifrost_scribe()
        .expect("server-role Scribe");
    scribe
        .retire_committed_for_test(std::time::Instant::now() + Duration::from_secs(120))
        .await
        .expect("retire committed Scribe generations");
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
        pods: 1,
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
        .retire_committed_for_test(std::time::Instant::now() + Duration::from_secs(120))
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

/// Implements the uncertain-commit recovery journey for receipt-backed failover proof.
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
        pods: 1,
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
        .retire_committed_for_test(std::time::Instant::now() + Duration::from_secs(120))
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

/// Implements the bounded planner-admission journey used by the parity proof.
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
        pods: 1,
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
            ..ForgeWorkerConfig::default()
        },
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
        pods: 1,
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
        pods: 1,
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
    assert!(result.is_err(), "renewal loss must fail schedule_once");
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
        pods: 1,
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
        .retire_committed_for_test(std::time::Instant::now() + Duration::from_secs(120))
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
        .filter(|tenant| *tenant <= prior_cursor)
        .next_back()
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

/// Run one deployment shape from authenticated ingest through query readback.
///
/// # Panics
///
/// Panics when tenant provisioning, OTLP ingestion, Scribe flush, Forge
/// maintenance, durable inspection, or public query validation fails.
async fn run_scenario(scenario: Scenario) {
    let cluster = WyrdTestCluster::start_with_dedicated_forge_workers()
        .await
        .unwrap_or_else(|error| panic!("{} dedicated Forge cluster: {error}", scenario.name));
    let telemetry = cluster.telemetry().clone();
    let checkpoint = telemetry
        .checkpoint()
        .unwrap_or_else(|error| panic!("{} query telemetry checkpoint: {error}", scenario.name));
    let sampler = telemetry
        .begin_gauge_sampling(&checkpoint)
        .await
        .unwrap_or_else(|error| panic!("{} query telemetry sampler: {error}", scenario.name));
    let servers = cluster.servers();
    assert_eq!(cluster.topology(), BifrostTopology::DedicatedForgeWorkers);
    assert_eq!(servers[0].forge_process_role(), ForgeProcessRole::Server);
    assert!(servers[0].base_url().is_some());
    assert!(servers[0].grpc_url().is_some());
    assert_eq!(servers.len(), 4, "{} dedicated role count", scenario.name);
    assert!(servers[1..].iter().all(|worker| {
        worker.forge_process_role() == ForgeProcessRole::ForgeWorker
            && worker.base_url().is_none()
            && worker.grpc_url().is_none()
            && worker.bound_addr().is_none()
    }));
    let control = &servers[0];
    let completion = cluster
        .forge_completion_observer()
        .expect("dedicated Forge worker completion observer");
    let tenants = provision_tenants(control, scenario).await;
    assert_active_traces_roster(control, &tenants, scenario).await;

    let first_workload_baseline = capture_compaction_workload(control, &tenants).await;
    let completed_before = completion.completed();
    for writer_shard in 0..scenario.pods {
        for cycle in 0..WRITE_CYCLES {
            let cycle = writer_shard * WRITE_CYCLES + cycle;
            write_cycle(std::slice::from_ref(control), &tenants, cycle, scenario).await;
            for tenant in &tenants {
                control
                    .flush_bifrost_for_tenant(tenant.id)
                    .await
                    .unwrap_or_else(|error| {
                        panic!("{} tenant {} flush: {error}", scenario.name, tenant.id)
                    });
            }
        }
    }
    let first_workload = wait_for_published_workload(
        control,
        &tenants,
        first_workload_baseline,
        scenario.pods * WRITE_CYCLES,
        scenario,
    )
    .await;
    control
        .forge_clock()
        .advance(chrono::Duration::days(1) + chrono::Duration::minutes(3))
        .unwrap_or_else(|error| panic!("{} advance Forge clock: {error}", scenario.name));

    let expected_rows = u64::try_from((scenario.pods * WRITE_CYCLES + 2) * SPANS_PER_WRITE)
        .expect("bounded journey row count");
    let pending_before = pending_files(control, &tenants).await;
    let first_workload_completed =
        workload_has_terminal_effect(&inspect_workload_progress(control, &first_workload).await);
    if !first_workload_completed {
        assert_pending_files_are_old_enough(control, &tenants, scenario).await;
    }
    assert!(
        pending_before
            .iter()
            .all(|(_, count)| *count >= i64::try_from(WRITE_CYCLES).expect("cycles fit i64"))
            || first_workload_completed,
        "{} Scribe did not create independent durable files: {pending_before:?}",
        scenario.name
    );

    for tenant in &tenants {
        let response = query_response(control, &tenant.jwt).await;
        assert!(
            response.status().is_success(),
            "{} query must not use an Oracle publication fence: {}",
            scenario.name,
            response.status()
        );
    }
    wait_for_compaction(control, &tenants, &completion, &first_workload, scenario).await;
    assert!(
        completion.completed() > completed_before,
        "{} dedicated workers did not complete the first durable Forge pass",
        scenario.name
    );
    let retained_workload_baseline = capture_compaction_workload(control, &tenants).await;
    let completed_before_retention = completion.completed();
    for cycle in WRITE_CYCLES..(WRITE_CYCLES + 2) {
        write_cycle(std::slice::from_ref(control), &tenants, cycle, scenario).await;
        for tenant in &tenants {
            control
                .flush_bifrost_for_tenant(tenant.id)
                .await
                .unwrap_or_else(|error| {
                    panic!(
                        "{} tenant {} retained-history flush: {error}",
                        scenario.name, tenant.id
                    )
                });
        }
    }
    let retained_workload =
        wait_for_published_workload(control, &tenants, retained_workload_baseline, 2, scenario)
            .await;
    control
        .forge_clock()
        .advance(chrono::Duration::days(1) + chrono::Duration::minutes(3))
        .unwrap_or_else(|error| {
            panic!("{} advance retained-history clock: {error}", scenario.name)
        });
    let tails =
        wait_for_compaction(control, &tenants, &completion, &retained_workload, scenario).await;
    assert!(
        completion.completed() > completed_before_retention,
        "{} dedicated workers did not complete retained-history Forge work",
        scenario.name
    );
    let scribe = control.bifrost_scribe().expect("server-owned Scribe");
    scribe
        .retire_committed_for_test(std::time::Instant::now() + Duration::from_secs(120))
        .await
        .expect("Scribe retirement pass");
    let inspection = control
        .scribe_inspection_snapshot()
        .expect("Scribe inspection after retirement");
    assert_eq!(
        inspection.immutable_bucket_count, 0,
        "{} retained Scribe hot buckets after explicit retirement",
        scenario.name
    );
    for tenant in &tenants {
        assert_eq!(
            wait_for_query_rows(control, &tenant.jwt, expected_rows, scenario).await,
            expected_rows,
            "{} tenant {} exact rows",
            scenario.name,
            tenant.id
        );
        if tails
            .iter()
            .any(|(tail_tenant, _, _)| *tail_tenant == tenant.id)
        {
            assert_no_compaction_audits(control, tenant.id, scenario).await;
        } else {
            assert_terminal_audits(control, tenant.id, scenario).await;
            assert_snapshot(control, tenant.id, scenario).await;
        }
    }
    let delta = telemetry
        .delta_since(checkpoint, sampler)
        .await
        .unwrap_or_else(|error| panic!("{} query telemetry delta: {error}", scenario.name));
    let query = BifrostQueryTelemetryReport::from_server_delta(&delta)
        .unwrap_or_else(|error| panic!("{} query telemetry report: {error}", scenario.name));
    write_query_telemetry_artifact(scenario, query);

    cluster
        .shutdown()
        .await
        .unwrap_or_else(|error| panic!("{} shutdown: {error}", scenario.name));
}

/// Write one server-only query artifact without copying any Forge maintenance field.
///
/// # Panics
///
/// Panics when the report directory cannot be created or typed JSON cannot be
/// serialized and written.
fn write_query_telemetry_artifact(scenario: Scenario, query: BifrostQueryTelemetryReport) {
    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("wyrd-testing manifest is nested below the repository root");
    let directory = repository.join("target/bifrost-benchmarks/task16");
    std::fs::create_dir_all(&directory).expect("create Task 16 query report directory");
    let artifact = QueryTelemetryArtifact {
        scenario: QueryScenarioIdentity {
            pods: scenario.pods,
            tenants: scenario.tenants,
        },
        query,
    };
    let path = directory.join(format!("query-{}-{}.json", scenario.pods, scenario.tenants));
    std::fs::write(
        path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&artifact).expect("serialize typed query artifact")
        ),
    )
    .expect("write typed query artifact");
}

/// Authenticated tenant identity used by concurrent writers and query checks.
struct TenantWriter {
    /// Durable tenant isolation key.
    id: DataTenantId,
    /// Tenant-scoped access token accepted by OTLP and query endpoints.
    jwt: String,
}

/// One durable Forge demand and its pre-workload evidence baseline.
#[derive(Debug)]
struct ForgeCompactionTenantTarget {
    /// Tenant owning the canonical traces table.
    tenant: DataTenantId,
    /// Exact durable planning generation emitted by the completed writer workload.
    demand_generation: Option<i64>,
    /// Tasks that existed before this workload was offered to Forge.
    task_ids_before: BTreeSet<uuid::Uuid>,
    /// Terminal compaction audits that existed before this workload was offered.
    terminal_audits_before: i64,
}

/// Exact durable effects expected from one completed Scribe writer workload.
#[derive(Debug)]
struct ForgeCompactionWorkload {
    /// Per-tenant demand generations and pre-workload durable evidence.
    tenants: Vec<ForgeCompactionTenantTarget>,
}

/// One observed target task, restricted to work created by the current workload.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ForgeCompactionTargetTask {
    /// Durable task identity assigned atomically with the target demand acknowledgement.
    task_id: uuid::Uuid,
    /// Current worker-visible durable state.
    state: String,
    /// Whether the worker recorded terminal compaction evidence.
    has_evidence: bool,
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

/// Return uncompacted durable Scribe file counts for every tenant.
///
/// # Panics
///
/// Panics when tenant-scoped Postgres inspection fails.
async fn pending_files(
    server: &WyrdTestServer,
    tenants: &[TenantWriter],
) -> Vec<(DataTenantId, i64)> {
    let mut counts = Vec::with_capacity(tenants.len());
    for tenant in tenants {
        let mut conn = server
            .state()
            .postgres
            .vala()
            .tenant_conn(tenant.id)
            .await
            .expect("pending Scribe tenant connection");
        let count = sqlx::query_scalar(
            "SELECT count(*) FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = 'vala.traces' AND table_name = 'spans' AND NOT compacted",
        )
        .bind(tenant.id.as_uuid())
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("pending Scribe files");
        counts.push((tenant.id, count));
    }
    counts
}

/// Wait until a completed writer workload has published stable Forge demand state.
///
/// Scribe persistence publishes files and advances the coalesced demand
/// generation asynchronously. The scheduler must not be triggered until every
/// tenant has the expected durable files and two consecutive demand snapshots
/// agree; otherwise an acknowledgement can correctly reject a newer generation.
///
/// # Panics
///
/// Panics when durable file or demand inspection fails, or when publication
/// does not quiesce within the bounded journey deadline.
async fn wait_for_published_workload(
    server: &WyrdTestServer,
    tenants: &[TenantWriter],
    mut workload: ForgeCompactionWorkload,
    minimum_files: usize,
    scenario: Scenario,
) -> ForgeCompactionWorkload {
    let minimum_files = i64::try_from(minimum_files).expect("bounded workload file count");
    let tenant_ids = tenants
        .iter()
        .map(|tenant| tenant.id.as_uuid())
        .collect::<Vec<_>>();
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut previous_demands = None;
    loop {
        let files = pending_files(server, tenants).await;
        let inspection = server
            .scribe_inspection_snapshot()
            .expect("published workload Scribe inspection");
        let demands: Vec<(uuid::Uuid, i64)> = sqlx::query_as(
            "SELECT data_tenant_id, generation FROM vala.forge_planning_demands WHERE data_tenant_id = ANY($1) ORDER BY data_tenant_id, catalog_name, namespace_name, table_name",
        )
        .bind(&tenant_ids)
        .fetch_all(
            server
                .state()
                .postgres
                .operator_pool()
                .expect("operator pool")
                .pool(),
        )
        .await
        .expect("published Forge demand generations");
        bind_workload_demands(server, &mut workload).await;
        let progress = inspect_workload_progress(server, &workload).await;
        if inspection.queued_items == 0
            && ((files.iter().all(|(_, count)| *count >= minimum_files)
                && previous_demands.as_ref() == Some(&demands))
                || workload_has_terminal_effect(&progress))
        {
            return workload;
        }
        assert!(
            Instant::now() < deadline,
            "{} published workload did not stabilize: files={files:?} queued_items={} immutable_buckets={} demands={demands:?}; {}",
            scenario.name,
            inspection.queued_items,
            inspection.immutable_bucket_count,
            format_workload_progress(&progress),
        );
        previous_demands = Some(demands);
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Captures the durable evidence baseline immediately before a writer workload.
///
/// A later scheduler pass must acknowledge a generation produced after this
/// snapshot, create a new staging-fold task, record terminal evidence, and leave
/// at most one open file per partition. Periodic expiry or orphan work is
/// deliberately outside this workload contract.
///
/// # Panics
///
/// Panics when durable baseline inspection fails.
async fn capture_compaction_workload(
    server: &WyrdTestServer,
    tenants: &[TenantWriter],
) -> ForgeCompactionWorkload {
    let mut targets = Vec::with_capacity(tenants.len());
    for tenant in tenants {
        let mut conn = server
            .state()
            .postgres
            .vala()
            .tenant_conn(tenant.id)
            .await
            .expect("workload target tenant connection");
        let task_ids_before: Vec<uuid::Uuid> = sqlx::query_scalar(
            "SELECT task_id FROM vala.forge_tasks WHERE data_tenant_id=$1 AND catalog_name='wyrd-redux' AND namespace_name='vala.traces' AND table_name='spans' AND strategy='staging_fold'",
        )
        .bind(tenant.id.as_uuid())
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("workload Forge task baseline");
        let terminal_audits_before: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id=$1 AND operation IN ('forge.file_compact.committed','forge.file_compact.recovered','forge.file_compact.reset')",
        )
        .bind(tenant.id.as_uuid())
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("workload Forge audit baseline");
        targets.push(ForgeCompactionTenantTarget {
            tenant: tenant.id,
            demand_generation: None,
            task_ids_before: task_ids_before.into_iter().collect(),
            terminal_audits_before,
        });
    }
    ForgeCompactionWorkload { tenants: targets }
}

/// Binds each pre-write baseline to the first durable generation emitted later.
///
/// # Panics
///
/// Panics when tenant-scoped demand inspection fails.
async fn bind_workload_demands(server: &WyrdTestServer, workload: &mut ForgeCompactionWorkload) {
    for target in &mut workload.tenants {
        if target.demand_generation.is_some() {
            continue;
        }
        let mut conn = server
            .state()
            .postgres
            .vala()
            .tenant_conn(target.tenant)
            .await
            .expect("workload demand tenant connection");
        target.demand_generation = sqlx::query_scalar(
            "SELECT generation FROM vala.forge_planning_demands WHERE data_tenant_id=$1 AND catalog_name='wyrd-redux' AND namespace_name='vala.traces' AND table_name='spans'",
        )
        .bind(target.tenant.as_uuid())
        .fetch_optional(&mut **conn.transaction())
        .await
        .expect("workload Forge demand generation");
    }
}

/// Return the remaining logical staging tails for the journey's canonical table.
///
/// A tail is the at-most-one uncompacted file for one tenant, table, and
/// partition day after bounded Forge convergence.
///
/// # Panics
///
/// Panics when tenant-scoped Postgres inspection fails.
async fn pending_tail_groups(
    server: &WyrdTestServer,
    tenants: &[TenantWriter],
) -> Vec<(DataTenantId, chrono::NaiveDate, i64)> {
    let mut tails = Vec::new();
    for tenant in tenants {
        let mut conn = server
            .state()
            .postgres
            .vala()
            .tenant_conn(tenant.id)
            .await
            .expect("pending staging-tail tenant connection");
        let rows = sqlx::query_as::<_, (chrono::NaiveDate, i64)>(
            "SELECT partition_day, count(*) FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = 'vala.traces' AND table_name = 'spans' AND NOT compacted GROUP BY partition_day ORDER BY partition_day",
        )
        .bind(tenant.id.as_uuid())
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("pending staging tails");
        tails.extend(
            rows.into_iter()
                .map(|(partition_day, files)| (tenant.id, partition_day, files)),
        );
    }
    tails
}

/// Assert the manual Forge clock has moved past the staging age guard.
///
/// # Panics
///
/// Panics when tenant-scoped Postgres inspection fails or a journey file remains too
/// new for periodic Forge discovery.
async fn assert_pending_files_are_old_enough(
    server: &WyrdTestServer,
    tenants: &[TenantWriter],
    scenario: Scenario,
) {
    let cutoff = server.forge_clock().now().expect("Forge clock") - chrono::Duration::minutes(2);
    for tenant in tenants {
        let mut conn = server
            .state()
            .postgres
            .vala()
            .tenant_conn(tenant.id)
            .await
            .expect("pending-file-age tenant connection");
        let newest: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
            "SELECT max(created_at) FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = 'vala.traces' AND table_name = 'spans' AND NOT compacted",
        )
        .bind(tenant.id.as_uuid())
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("pending file age");
        assert!(
            newest.is_some_and(|created_at| created_at < cutoff),
            "{} tenant {} pending files are younger than the Forge cutoff {cutoff}: {newest:?}",
            scenario.name,
            tenant.id
        );
    }
}

/// Wait until every tenant's durable files have a committed Forge snapshot.
///
/// # Panics
///
/// Panics when SQL inspection fails or the bounded convergence deadline
/// expires.
async fn wait_for_compaction(
    control: &WyrdTestServer,
    tenants: &[TenantWriter],
    completion: &ForgeWorkerCompletionObserver,
    workload: &ForgeCompactionWorkload,
    scenario: Scenario,
) -> Vec<(DataTenantId, chrono::NaiveDate, i64)> {
    let completed_at_start = completion.completed();
    let started = Instant::now();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if Instant::now() >= deadline {
            panic!(
                "{} Forge workload did not converge after {:?}; completion_delta={}; {}",
                scenario.name,
                started.elapsed(),
                completion.completed().saturating_sub(completed_at_start),
                workload_convergence_diagnostics(control, tenants, workload).await,
            );
        }
        let tails = pending_tail_groups(control, tenants).await;
        let progress = inspect_workload_progress(control, workload).await;
        if workload_has_converged(&tails, &progress) {
            return tails;
        }
        let completed_before = completion.completed();
        trigger_supervised_scheduler(control, scenario).await;
        let progress_deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let progress = inspect_workload_progress(control, workload).await;
            if completion.completed() > completed_before || workload_has_terminal_effect(&progress)
            {
                break;
            }
            assert!(
                Instant::now() < progress_deadline,
                "{} supervised Forge pass made no workload progress: {}",
                scenario.name,
                format_workload_progress(&progress),
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

/// Describe every durable convergence component when a bounded journey expires.
///
/// # Panics
///
/// Panics when cross-tenant diagnostic inspection fails.
async fn workload_convergence_diagnostics(
    server: &WyrdTestServer,
    tenants: &[TenantWriter],
    workload: &ForgeCompactionWorkload,
) -> String {
    let tails = pending_tail_groups(server, tenants).await;
    let progress = inspect_workload_progress(server, workload).await;
    format!("tails={tails:?}; {}", format_workload_progress(&progress))
}

/// Returns target task state, acknowledgement, and audit evidence for one workload.
///
/// # Panics
///
/// Panics when durable target inspection fails.
async fn inspect_workload_progress(
    server: &WyrdTestServer,
    workload: &ForgeCompactionWorkload,
) -> Vec<(DataTenantId, bool, Vec<ForgeCompactionTargetTask>, i64, i64)> {
    let mut progress = Vec::with_capacity(workload.tenants.len());
    for target in &workload.tenants {
        let mut conn = server
            .state()
            .postgres
            .vala()
            .tenant_conn(target.tenant)
            .await
            .expect("target progress tenant connection");
        let demand_is_pending = if let Some(generation) = target.demand_generation {
            sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM vala.forge_planning_demands WHERE data_tenant_id=$1 AND catalog_name='wyrd-redux' AND namespace_name='vala.traces' AND table_name='spans' AND generation=$2)",
            )
            .bind(target.tenant.as_uuid())
            .bind(generation)
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("target demand inspection")
        } else {
            false
        };
        let task_rows: Vec<(uuid::Uuid, String, bool)> = sqlx::query_as(
            "SELECT task_id, state, evidence IS NOT NULL FROM vala.forge_tasks WHERE data_tenant_id=$1 AND catalog_name='wyrd-redux' AND namespace_name='vala.traces' AND table_name='spans' AND strategy='staging_fold'",
        )
        .bind(target.tenant.as_uuid())
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("target task inspection");
        let tasks = task_rows
            .into_iter()
            .filter(|(task_id, _, _)| !target.task_ids_before.contains(task_id))
            .map(|(task_id, state, has_evidence)| ForgeCompactionTargetTask {
                task_id,
                state,
                has_evidence,
            })
            .collect();
        let terminal_audits: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id=$1 AND operation IN ('forge.file_compact.committed','forge.file_compact.recovered','forge.file_compact.reset')",
        )
        .bind(target.tenant.as_uuid())
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("target audit inspection");
        progress.push((
            target.tenant,
            demand_is_pending,
            tasks,
            terminal_audits,
            target.terminal_audits_before,
        ));
    }
    progress
}

/// Returns whether a target workload made a durable terminal transition.
fn workload_has_terminal_effect(
    progress: &[(DataTenantId, bool, Vec<ForgeCompactionTargetTask>, i64, i64)],
) -> bool {
    progress.iter().any(|(_, demand_is_pending, tasks, _, _)| {
        !*demand_is_pending
            && tasks.iter().any(|task| {
                matches!(
                    task.state.as_str(),
                    "succeeded" | "failed" | "cancelled" | "unschedulable"
                )
            })
    })
}

/// Returns whether the exact workload, rather than unrelated maintenance, converged.
fn workload_has_converged(
    tails: &[(DataTenantId, chrono::NaiveDate, i64)],
    progress: &[(DataTenantId, bool, Vec<ForgeCompactionTargetTask>, i64, i64)],
) -> bool {
    tails.iter().all(|(_, _, files)| *files <= 1)
        && progress.iter().all(
            |(_, demand_is_pending, tasks, terminal_audits, terminal_audits_before)| {
                !*demand_is_pending
                    && !tasks.is_empty()
                    && tasks
                        .iter()
                        .all(|task| task.state == "succeeded" && task.has_evidence)
                    && *terminal_audits > *terminal_audits_before
            },
        )
}

/// Renders exact workload convergence state for bounded timeout failures.
fn format_workload_progress(
    progress: &[(DataTenantId, bool, Vec<ForgeCompactionTargetTask>, i64, i64)],
) -> String {
    format!("targets={progress:?}")
}

/// Target convergence ignores unrelated later Ready maintenance but rejects a
/// target task that has not succeeded with durable evidence.
#[test]
fn workload_convergence_tracks_only_captured_compaction_tasks() {
    let tenant = DataTenantId::new_v7();
    let tails = Vec::new();
    let succeeded = ForgeCompactionTargetTask {
        task_id: uuid::Uuid::now_v7(),
        state: "succeeded".to_owned(),
        has_evidence: true,
    };
    let target_complete = vec![(tenant, false, vec![succeeded.clone()], 4, 3)];

    // A later Ready snapshot-expiry task is intentionally not in the captured
    // staging-fold task set, so it cannot keep this writer workload open.
    assert!(workload_has_converged(&tails, &target_complete));

    let target_ready = vec![(
        tenant,
        false,
        vec![ForgeCompactionTargetTask {
            state: "ready".to_owned(),
            ..succeeded.clone()
        }],
        4,
        3,
    )];
    assert!(!workload_has_converged(&tails, &target_ready));

    let target_failed = vec![(
        tenant,
        false,
        vec![ForgeCompactionTargetTask {
            state: "failed".to_owned(),
            ..succeeded
        }],
        4,
        3,
    )];
    assert!(!workload_has_converged(&tails, &target_failed));
}

/// Assert an accepted open-partition tail has not created durable Forge work.
///
/// # Panics
///
/// Panics when audit inspection fails or Forge recorded a compaction operation
/// for a tail that the planner accepted without a rewrite.
async fn assert_no_compaction_audits(
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
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id = $1 AND operation LIKE 'forge.file_compact.%'",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("Forge audit count");
    assert_eq!(
        count, 0,
        "{} accepted tail for tenant {tenant} recorded Forge compaction work",
        scenario.name
    );
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
        .json(&SyncQueryRequest {
            sql: "SELECT * FROM \"vala.traces.spans\" WHERE service_name = 'checkout-api'"
                .to_owned(),
            params: Vec::new(),
        })
        .send()
        .await
        .expect("public query request")
}

/// Return the exact row count from the public same-table query.
///
/// # Panics
///
/// Panics when the query fails or omits its row-count header.
async fn query_rows(server: &WyrdTestServer, jwt: &str) -> u64 {
    let response = query_response(server, jwt)
        .await
        .error_for_status()
        .expect("public query response");
    response
        .headers()
        .get("x-wyrd-row-count")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
        .expect("row-count header")
}

/// Wait for Scribe's committed-generation grace window to retire overlap.
///
/// # Panics
///
/// Panics when the public query does not converge to the exact expected row
/// count before the bounded deadline.
async fn wait_for_query_rows(
    server: &WyrdTestServer,
    jwt: &str,
    expected: u64,
    scenario: Scenario,
) -> u64 {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let rows = query_rows(server, jwt).await;
        if rows == expected {
            return rows;
        }
        assert!(
            Instant::now() < deadline,
            "{} query did not retire Scribe/Iceberg overlap: expected {expected}, observed {rows}",
            scenario.name
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
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
