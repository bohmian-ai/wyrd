//! Distributed product journey for the real Scribe-to-Forge publication path.

use std::sync::Arc;
use std::time::Duration;

use arrow::array::{ArrayRef, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use iceberg::transaction::{ApplyTransactionAction, Transaction};
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
use vala_bifrost_redux::resources::{ResourceSource, SystemResourceSnapshot};
use vala_sdk::BifrostGrpcTransport;
use vala_sql::queries::forge_tasks::ForgeTasks;
use vala_sql::row_types::forge_tasks::{
    FORGE_TASK_PAYLOAD_VERSION, ForgeClaimStrategy, ForgeTaskEstimates, ForgeTaskLane,
    ForgeTaskPlan, ForgeTaskStrategy, ForgeTaskTableIdentity, NewForgeTask,
};
use wyrd_client::WyrdClient;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_server::config::BifrostTarget;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{
    AuditDetail, BifrostQueryRequest, ForgeIcebergRewritePhase, FreshnessPolicy, StoragePath,
    VisibilityMode,
};
use wyrd_testing::bifrost::forge_harness::seed_forge_group;
use wyrd_testing::bifrost::{
    BifrostClusterSpec, BifrostTopology, ForgeCausalDiagnosis, ForgeCausalTelemetryReport,
    ForgeObjectStoreControl, ForgeTelemetryFailureClass, ForgeTelemetryResource, WyrdTestCluster,
    shared_process_telemetry_for_test,
};
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

/// Stable scheduler identity for maintenance journey fixtures.
const MAINTENANCE_JOURNEY_SCHEDULER_OWNER: u128 = 0x0198_39f4_2b51_7000_8000_0000_0000_0005;

/// Rejects private Forge ownership seams from the authoritative journey graph.
///
/// The guard discovers helpers transitively across the journey and bound
/// server/cluster harness, so cross-file indirection cannot hide a private
/// owner seam.
#[test]
fn authoritative_forge_call_tree_uses_only_supervised_owners() {
    let sources = [
        include_str!("journeys.rs"),
        include_str!("../../../src/server.rs"),
        include_str!("../../../src/bifrost/cluster.rs"),
        include_str!("../../../src/bifrost/forge_harness.rs"),
    ];
    let roots = [
        "pg_bifrost_forge_attempt_terminal_paths_settle_exactly_once",
        "pg_bifrost_forge_publication_recovery_and_orphan_gc",
        "pg_bifrost_forge_manifest_maintenance_precedes_expiry",
        "pg_bifrost_forge_small_files_converges_without_query_dependency",
    ];
    let forbidden = [
        "Forge::run",
        ".run_once(",
        "ForgeWorker::new",
        "execute_one_for_test",
        "claim_next_for_test",
        "claim_fair_for_test",
        "append_prepared",
        "reconcile_live_replacements_for_test",
        "run_orphan_gc_for_test",
    ];
    let mutation_forbidden = [
        "INSERT INTO vala.forge_tasks",
        "UPDATE vala.forge_tasks",
        "DELETE FROM vala.forge_tasks",
        "INSERT INTO vala.forge_operations",
        "UPDATE vala.forge_operations",
        "DELETE FROM vala.forge_operations",
    ];
    let violations =
        authoritative_forge_violations(&sources, &roots, &forbidden, &mutation_forbidden);
    assert!(
        violations.is_empty(),
        "authoritative Forge graph violations: {violations:?}"
    );
}

/// Traverses the closed journey facade and reports private ownership violations.
///
/// Qualified calls resolve by exact owner, the closed facade's receiver names
/// resolve to their concrete owners, `self` calls remain on the current owner,
/// and bare calls resolve only to module functions. This deliberately refuses
/// name-only cross-owner traversal, so same-named methods cannot alias one
/// another in the authority graph.
fn authoritative_forge_violations(
    sources: &[&str],
    roots: &[&str],
    forbidden: &[&str],
    mutation_forbidden: &[&str],
) -> Vec<String> {
    let local_functions = sources
        .iter()
        .enumerate()
        .flat_map(|(source_index, source)| {
            rust_functions(source)
                .into_iter()
                .map(move |function| (source_index, function))
        })
        .collect::<Vec<_>>();
    let mut pending = roots
        .iter()
        .map(|name| {
            local_functions
                .iter()
                .find(|(source, function)| *source == 0 && function.name == *name)
                .cloned()
                .expect("authoritative root has an owner-qualified definition")
        })
        .collect::<Vec<_>>();
    let mut authoritative = std::collections::BTreeSet::new();
    let mut violations = Vec::new();
    while let Some((source_index, function)) = pending.pop() {
        if !authoritative.insert((source_index, function.start)) {
            continue;
        }
        let body = rust_function_body(sources[source_index], &function)
            .expect("guarded function must exist in the authoritative source graph");
        for (callee_source, callee) in &local_functions {
            let qualified = format!("{}::{}(", callee.owner, callee.name);
            let receiver = format!("self.{}(", callee.name);
            let module_call = format!("{}(", callee.name);
            let facade_receiver = authoritative_receiver(&callee.owner)
                .is_some_and(|receiver| body.contains(&format!("{receiver}.{}(", callee.name)));
            let is_called = body.contains(&qualified)
                || (callee.owner == function.owner && body.contains(&receiver))
                || facade_receiver
                || (callee.owner == "module" && body.contains(&module_call));
            if is_called && !authoritative.contains(&(*callee_source, callee.start)) {
                pending.push((*callee_source, callee.clone()));
            }
        }
        for token in forbidden {
            if body.contains(token) {
                violations.push(format!(
                    "{}::{} contains private owner seam {token}",
                    function.owner, function.name
                ));
            }
        }
        for token in mutation_forbidden {
            if body.contains(token) {
                violations.push(format!(
                    "{}::{} mutates durable owner table through {token}",
                    function.owner, function.name
                ));
            }
        }
    }
    violations
}

/// Maps every stateful owner in the closed authoritative facade to its receiver.
fn authoritative_receiver(owner: &str) -> Option<&'static str> {
    match owner {
        "WyrdTestServer" => Some("server"),
        "WyrdTestCluster" => Some("cluster"),
        "ForgeFixture" => Some("fixture"),
        "ForgeObjectStoreControl" => Some("controls"),
        "JourneyMaintenance" => Some("maintenance"),
        _ => None,
    }
}

/// One owner-qualified function definition in an inspected source.
#[derive(Clone, Debug)]
struct RustFunction {
    owner: String,
    name: String,
    start: usize,
}

/// Returns every owner-qualified function and its exact definition offset.
fn rust_functions(source: &str) -> Vec<RustFunction> {
    let impls = rust_impl_ranges(source);
    let mut functions = Vec::new();
    let mut offset = 0;
    while let Some(relative) = source[offset..].find("fn ") {
        let start = offset + relative;
        let tail = &source[start + 3..];
        let name = tail
            .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
            .next()
            .unwrap_or_default();
        if !name.is_empty() {
            let owner = impls
                .iter()
                .find(|implementation| implementation.start < start && start < implementation.end)
                .map_or("module", |implementation| implementation.owner.as_str())
                .to_owned();
            functions.push(RustFunction {
                owner,
                name: name.to_owned(),
                start,
            });
        }
        offset = start + 3 + name.len();
    }
    functions
}

/// One lexical `impl` block and its concrete owner.
struct RustImplRange {
    /// Concrete type named by the implementation.
    owner: String,
    /// Opening-brace offset of the implementation.
    start: usize,
    /// Closing-brace offset of the implementation.
    end: usize,
}

/// Returns the lexical ranges of concrete inherent implementation blocks.
fn rust_impl_ranges(source: &str) -> Vec<RustImplRange> {
    let mut ranges = Vec::new();
    let mut offset = 0;
    while let Some(relative) = source[offset..].find("impl ") {
        let declaration = offset + relative;
        let tail = &source[declaration + 5..];
        let owner = tail
            .split(|character: char| character == '{' || character.is_whitespace())
            .next()
            .unwrap_or_default();
        let Some(open_relative) = tail.find('{') else {
            break;
        };
        let open = declaration + 5 + open_relative;
        if !owner.is_empty()
            && let Some(end) = matching_brace(source, open)
        {
            ranges.push(RustImplRange {
                owner: owner.to_owned(),
                start: open,
                end,
            });
        }
        offset = open + 1;
    }
    ranges
}

/// Finds the closing brace paired with `open`.
fn matching_brace(source: &str, open: usize) -> Option<usize> {
    let mut depth = 0_usize;
    for (relative, byte) in source.as_bytes()[open..].iter().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(open + relative);
                }
            }
            _ => {}
        }
    }
    None
}

/// Extract one Rust function body for the static authoritative-owner guard.
///
/// # Panics
///
/// Panics when the named function is absent or its braces are unbalanced.
fn rust_function_body<'a>(source: &'a str, function: &RustFunction) -> Option<&'a str> {
    let open = source[function.start..]
        .find('{')
        .map(|offset| function.start + offset)
        .expect("guarded function must have a body");
    let mut depth = 0_usize;
    for (offset, byte) in source.as_bytes()[open..].iter().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth = depth
                    .checked_sub(1)
                    .expect("function braces remain balanced");
                if depth == 0 {
                    return Some(&source[open..=open + offset]);
                }
            }
            _ => {}
        }
    }
    panic!("guarded function body must close")
}

/// Proves a same-named method cannot conceal a forbidden call in another owner.
#[test]
fn owner_guard_distinguishes_same_named_methods() {
    let source = "impl Closed { fn start(&self) {} } fn root() { helper(); } fn helper() { Hidden::start(); } impl Allowed { fn start(&self) {} } impl Hidden { fn start(&self) { ForgeWorker::new(); } }";
    let violations =
        authoritative_forge_violations(&[source], &["root"], &["ForgeWorker::new"], &[]);
    assert_eq!(violations.len(), 1);
    assert!(violations[0].starts_with("Hidden::start "));
}

/// Provisions one Forge fixture and creates its compaction debt through native ingestion.
///
/// # Panics
///
/// Panics when provisioning or native ingestion cannot cross the durable flush barrier.
async fn native_forge_group(
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
async fn append_native_forge_cycles(
    server: &WyrdTestServer,
    fixture: &wyrd_testing::bifrost::ForgeFixture,
    sequences: impl IntoIterator<Item = i64>,
) {
    append_native_forge_cycles_with_rows(server, fixture, sequences, 1).await;
}

/// Appends a selected row count in each independently sealed native batch.
async fn append_native_forge_cycles_with_rows(
    server: &WyrdTestServer,
    fixture: &wyrd_testing::bifrost::ForgeFixture,
    sequences: impl IntoIterator<Item = i64>,
    rows_per_batch: usize,
) {
    append_native_forge_cycles_with_flush_mode(server, fixture, sequences, rows_per_batch, true)
        .await;
}

/// Appends public batches behind one tenant flush so Forge observes the complete group.
async fn append_native_forge_group_with_rows(
    server: &WyrdTestServer,
    fixture: &wyrd_testing::bifrost::ForgeFixture,
    sequences: impl IntoIterator<Item = i64>,
    rows_per_batch: usize,
) {
    append_native_forge_cycles_with_flush_mode(server, fixture, sequences, rows_per_batch, false)
        .await;
}

/// Sends public batches with either per-batch or group-level durable flushes.
async fn append_native_forge_cycles_with_flush_mode(
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
fn native_forge_value_ipc(sequence: i64, rows: usize) -> Vec<u8> {
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
    operator_pool: vala_sql::OperatorPool,
    tenant: DataTenantId,
    /// Deterministic catalog boundaries shared with the production lifecycle.
    maintenance_controls: vala_bifrost_redux::forge::MaintenanceTestControls,
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
    async fn run_one_success(&mut self) -> ForgeTaskStrategy {
        self.run_one_success_at("maintenance").await
    }

    /// Runs one completed attempt with a diagnostic stage name.
    async fn run_one_success_at(&mut self, stage: &str) -> ForgeTaskStrategy {
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
    async fn run_scheduler_pass(&self) {
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
    superseded_worker_cancelled_settlement_journey().await;
}

/// Drives one real superseded worker through its durable cancelled settlement.
///
/// # Panics
///
/// Panics when cancellation fails to settle the task, audit, or bounded
/// telemetry evidence through the production worker path.
async fn superseded_worker_cancelled_settlement_journey() {
    let (server, telemetry) = start_telemetry_maintenance_server().await;
    let fixture = native_forge_group(&server, "durable_cancelled_metric").await;
    commit_journey_staging_snapshot(&server, &fixture).await;
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

/// Proves cancellation, qualified failure, claim expiry, and successor reclaim settle once.
///
/// Each phase drives a real PostgreSQL-backed production worker boundary and
/// asserts durable task, audit, telemetry, publication, and resource-release
/// evidence before its fixture shuts down.
///
/// # Panics
///
/// Panics when any terminal path retains ownership, duplicates publication, or
/// diverges from its exact durable and resource settlement.
#[tokio::test]
#[ignore = "gated journey: real Forge terminal paths and exact settlement"]
async fn pg_bifrost_forge_attempt_terminal_paths_settle_exactly_once() {
    supervised_same_tenant_fifo_journey().await;
    supervised_worker_claim_expiry_journey().await;
    supervised_forge_panic_recovery_scenario().await;
    dedicated_unschedulable_admission_journey().await;
    supervised_temporary_pressure_defers_without_ownership().await;
}

/// Proves two durable ready tasks for one tenant are claimed in FIFO order.
///
/// # Panics
///
/// Panics when owner-backed enqueue, supervised claim observation, or orderly
/// server shutdown fails to preserve the two distinct readiness instants.
async fn supervised_same_tenant_fifo_journey() {
    let observer = ForgeWorkerCompletionObserver::new();
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .with_forge_completion_observer_for_test(observer.clone())
        .start_bound()
        .await
        .expect("bound same-tenant FIFO server");
    let first = native_forge_group(&server, "journey_fifo_first").await;
    let second = native_forge_group(&server, "journey_fifo_second").await;
    let tasks = ForgeTasks::new(first.operator_pool.clone());
    let readiness = chrono::Utc::now();
    let first_id = enqueue_stale_fifo_task(
        &tasks,
        &first,
        readiness - chrono::Duration::seconds(2),
        0x31,
    )
    .await;
    let second_id = enqueue_stale_fifo_task(
        &tasks,
        &second,
        readiness - chrono::Duration::seconds(1),
        0x32,
    )
    .await;
    let events = tokio::time::timeout(
        Duration::from_secs(30),
        observer.wait_for_lifecycle(|events| {
            events
                .iter()
                .filter(|event| {
                    matches!(
                        event,
                        vala_bifrost_redux::forge::ForgeLifecycleEvent::Claimed { task_id, .. }
                            if *task_id == first_id || *task_id == second_id
                    )
                })
                .count()
                == 2
        }),
    )
    .await
    .expect("two supervised same-tenant claims");
    let claimed = events
        .iter()
        .filter_map(|event| match event {
            vala_bifrost_redux::forge::ForgeLifecycleEvent::Claimed { task_id, .. }
                if *task_id == first_id || *task_id == second_id =>
            {
                Some(*task_id)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        claimed,
        [first_id, second_id],
        "one tenant's durable ready queue must claim by ready_at then task_id"
    );
    server.shutdown().await.expect("same-tenant FIFO shutdown");
}

/// Enqueues one intentionally stale task whose supervised claim settles without publication.
async fn enqueue_stale_fifo_task(
    tasks: &ForgeTasks,
    fixture: &wyrd_testing::bifrost::ForgeFixture,
    ready_at: chrono::DateTime<chrono::Utc>,
    plan_byte: u8,
) -> uuid::Uuid {
    let envelope = journey_envelope(fixture, 1, 1);
    tasks
        .enqueue(&NewForgeTask {
            data_tenant_id: fixture.tenant,
            table_ref: ForgeTaskTableIdentity::new(
                "wyrd-redux",
                &fixture.binding.logical_namespace,
                &fixture.binding.table_name,
            )
            .expect("FIFO task table identity"),
            strategy: ForgeTaskStrategy::StagingFold,
            lane: ForgeTaskLane::Ordinary,
            base_snapshot_id: 1,
            plan: ForgeTaskPlan {
                version: FORGE_TASK_PAYLOAD_VERSION,
                inputs: vec![format!("fifo-{plan_byte}.parquet")],
                parameters: serde_json::json!({"kind":"staging_fold"}),
            },
            plan_hash: [plan_byte; 32],
            estimates: ForgeTaskEstimates {
                envelope: Some(envelope),
                files: 1,
                bytes: 1,
                parallelism: envelope.reader_permits,
                memory_bytes: envelope.memory_bytes().expect("FIFO resident total"),
                spill_bytes: envelope.scratch_bytes().expect("FIFO scratch total"),
                large_ceiling_bytes: 1,
            },
            ready_at,
        })
        .await
        .expect("enqueue same-tenant FIFO task")
}

/// Proves temporary root pressure defers durably and releases atomically.
async fn supervised_temporary_pressure_defers_without_ownership() {
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
/// start with its background Forge interval disabled for deterministic control.
async fn start_maintenance_journey_server() -> WyrdTestServer {
    WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .with_forge_process_role_for_test(BifrostTarget::All)
        .start_bound()
        .await
        .expect("bound maintenance journey server")
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
        .with_forge_process_role_for_test(BifrostTarget::All)
        .start_bound()
        .await
        .expect("bound telemetry maintenance server");
    (server, telemetry)
}

/// Commits one staging-fold snapshot through a real production scheduler and worker.
///
/// # Panics
///
/// Panics when the fixture cannot create a successful staging-fold task within
/// two bounded production passes.
async fn commit_journey_staging_snapshot(
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
    let fixture = native_forge_group(&server, "journey_snapshot_expiry").await;
    commit_journey_staging_snapshot(&server, &fixture).await;
    append_native_forge_cycles(&server, &fixture, 3..5).await;
    commit_journey_staging_snapshot(&server, &fixture).await;
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
    config.manifest_rewrite_enabled = true;
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
/// every planning tick. With the maintenance trigger
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
    let fixture = native_forge_group(&server, "journey_sustained_ingest_maintenance").await;
    // Two production staging-fold commits create retained Iceberg history so the
    // maintenance trigger has snapshots beyond `retain_last` to expire.
    commit_journey_staging_snapshot(&server, &fixture).await;
    append_native_forge_cycles(&server, &fixture, 3..5).await;
    commit_journey_staging_snapshot(&server, &fixture).await;
    // Seed a sustained compaction backlog: uncompacted staging files keep a
    // staging-fold candidate available on every subsequent planning tick, so
    // maintenance must lead through real compaction contention rather than an
    // idle table.
    append_native_forge_cycles(&server, &fixture, 5..8).await;
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

/// Proves manifest submission precedes accepted snapshot expiry under one lease.
///
/// The journey holds the real manifest catalog boundary, observes that no
/// expiry terminal audit exists, then releases it and requires the accepted
/// expiry boundary before the worker may complete.
///
/// # Panics
///
/// Panics when retained history cannot be built, expiry reaches acceptance
/// before manifest submission, or the ordered maintenance task fails to settle.
#[tokio::test]
#[ignore = "gated journey: real Postgres and ordered Forge maintenance boundaries"]
async fn supporting_in_process_manifest_maintenance_precedes_expiry() {
    let server = start_maintenance_journey_server().await;
    let fixture = native_forge_group(&server, "journey_manifest_before_expiry").await;
    commit_journey_staging_snapshot(&server, &fixture).await;
    append_native_forge_cycles(&server, &fixture, 3..5).await;
    commit_journey_staging_snapshot(&server, &fixture).await;
    let table_ref = ForgeTaskTableIdentity::new(
        "wyrd-redux",
        &fixture.binding.logical_namespace,
        &fixture.binding.table_name,
    )
    .expect("ordered maintenance task identity");
    ForgeTasks::new(fixture.operator_pool.clone())
        .upsert_periodic(fixture.tenant, &table_ref)
        .await
        .expect("seed no-fit manifest demand");
    let tasks_before: i64 =
        sqlx::query_scalar("SELECT count(*) FROM vala.forge_tasks WHERE data_tenant_id=$1")
            .bind(fixture.tenant.as_uuid())
            .fetch_one(fixture.operator_pool.pool())
            .await
            .expect("ordered maintenance task baseline");
    let mut no_fit = fixture.config.clone();
    no_fit.manifest_rewrite_enabled = true;
    no_fit.manifest_rewrite_target_size_bytes = 8 * 1024 * 1024;
    no_fit.manifest_rewrite_min_count = 2;
    no_fit.max_bytes_per_tick = 1;
    no_fit.maintenance_trigger_snapshot_count = usize::MAX;
    no_fit.maintenance_trigger_interval = Duration::from_hours(24 * 365);
    no_fit.snapshot_retention = Duration::from_hours(24 * 365);
    let mut fit = no_fit.clone();
    fit.max_files_per_tick = 1_000;
    fit.max_bytes_per_tick = fixture.config.max_bytes_per_tick;
    let no_fit_lifecycle = JourneyMaintenance::start(&fixture, no_fit);
    no_fit_lifecycle.run_scheduler_pass().await;
    let demand_retained: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM vala.forge_planning_demands WHERE data_tenant_id=$1 AND catalog_name=$2 AND namespace_name=$3 AND table_name=$4)",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&table_ref.catalog)
    .bind(&table_ref.namespace)
    .bind(&table_ref.table)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("observe no-fit manifest demand");
    let tasks_after: i64 =
        sqlx::query_scalar("SELECT count(*) FROM vala.forge_tasks WHERE data_tenant_id=$1")
            .bind(fixture.tenant.as_uuid())
            .fetch_one(fixture.operator_pool.pool())
            .await
            .expect("ordered maintenance task count after no-fit pass");
    assert!(
        demand_retained,
        "no-fit manifest demand must remain pending"
    );
    assert_eq!(
        tasks_after, tasks_before,
        "no-fit manifest demand must enqueue no task"
    );
    no_fit_lifecycle.shutdown().await;
    let mut manifest_only = JourneyMaintenance::start(&fixture, fit.clone());
    assert_eq!(
        manifest_only.run_one_success().await,
        ForgeTaskStrategy::ManifestRewrite,
        "manifest-only intent must retain its exact durable strategy"
    );
    manifest_only.shutdown().await;
    assert_eq!(
        fixture
            .operation_count("forge.snapshot_expire.committed")
            .await,
        0,
        "manifest-only demand must not bypass the independent expiry trigger"
    );
    server
        .forge_clock()
        .advance(chrono::Duration::milliseconds(2))
        .expect("advance ordered maintenance clock");
    let mut config = fixture.config.clone();
    config.snapshot_retention = Duration::from_millis(1);
    config.manifest_rewrite_enabled = true;
    config.manifest_rewrite_target_size_bytes = 8 * 1024 * 1024;
    config.manifest_rewrite_min_count = 2;
    config.maintenance_trigger_snapshot_count = 1;
    config.maintenance_trigger_interval = Duration::from_millis(1);
    config.min_files = 3;
    config.max_files_per_bin = 3;
    config.max_files_per_tick = 3;
    config.max_bytes_per_tick = fit.max_bytes_per_tick;
    let mut lifecycle = JourneyMaintenance::start(&fixture, config);
    let controls = lifecycle.maintenance_controls.clone();
    controls.arm_manifest_submission();
    controls.arm_expiry_accepted();
    let mut attempt = Box::pin(lifecycle.run_one_success_at("ordered-expiry"));
    tokio::select! {
        () = controls.wait_manifest_submission() => {}
        strategy = &mut attempt => panic!("maintenance completed before manifest boundary: {strategy:?}"),
    }
    assert_eq!(
        fixture
            .operation_count("forge.snapshot_expire.committed")
            .await,
        0,
        "expiry cannot commit while manifest submission is held"
    );
    controls.release_manifest_submission();
    tokio::select! {
        () = controls.wait_expiry_accepted() => {}
        strategy = &mut attempt => panic!("maintenance completed before expiry acceptance: {strategy:?}"),
    }
    controls.release_expiry_accepted();
    assert_eq!(attempt.await, ForgeTaskStrategy::SnapshotExpiry);
    lifecycle.shutdown().await;
    assert_eq!(
        fixture
            .operation_count("forge.snapshot_expire.committed")
            .await,
        1,
        "ordered expiry must terminalize exactly once"
    );
    server
        .shutdown()
        .await
        .expect("ordered maintenance server shutdown");
}

/// Proves the bound supervised Forge owner executes maintenance without a test-owned engine.
///
/// # Panics
///
/// Panics when native ingestion, durable scheduling, or the supervised worker
/// completion boundary fails to settle within the bounded journey window.
#[tokio::test]
#[ignore = "gated journey: real Postgres and supervised Forge maintenance"]
async fn pg_bifrost_forge_manifest_maintenance_precedes_expiry() {
    supervised_temporary_pressure_defers_without_ownership().await;
    supervised_uncertain_commit_recovery_journey().await;
    let observer = ForgeWorkerCompletionObserver::new();
    let config = ForgeConfig {
        lease_ttl: Duration::from_secs(4),
        iceberg_total_retry_timeout: Duration::from_secs(1),
        catalog_request_timeout: Duration::from_secs(1),
        uncertainty_margin: Duration::from_secs(1),
        uncertainty_bound: Duration::from_secs(1),
        manifest_rewrite_enabled: true,
        manifest_rewrite_min_count: 2,
        maintenance_trigger_snapshot_count: 1,
        maintenance_trigger_interval: Duration::from_millis(1),
        ..ForgeConfig::default()
    };
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .with_forge_config_for_test(config)
        .with_forge_completion_observer_for_test(observer.clone())
        .start_bound()
        .await
        .expect("bound supervised maintenance server");
    let fixture = native_forge_group(&server, "journey_supervised_maintenance").await;
    server
        .forge_clock()
        .advance(chrono::Duration::days(1) + chrono::Duration::minutes(3))
        .expect("close supervised maintenance staging partition");
    let latest_uncompacted_partition: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT max(partition_start) FROM vala.file_list \
         WHERE data_tenant_id=$1 AND namespace=$2 AND table_name=$3 \
         AND NOT compacted",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("supervised maintenance uncompacted partition day");
    assert!(
        latest_uncompacted_partition.is_some_and(|partition_start| {
            partition_start
                < server
                    .forge_clock()
                    .now()
                    .expect("supervised maintenance Forge clock")
        }),
        "all uncompacted maintenance inputs must belong to a closed partition"
    );
    for generation in 0..2 {
        if generation == 1 {
            append_native_forge_cycles(&server, &fixture, 3..4).await;
        }
        let expected = observer.completed().saturating_add(1);
        trigger_supervised_scheduler(
            &server,
            Scenario {
                name: "supervised-maintenance-staging",
                tenants: 1,
            },
        )
        .await;
        tokio::time::timeout(
            Duration::from_secs(30),
            observer.wait_for_at_least(expected),
        )
        .await
        .expect("supervised maintenance staging publication");
        assert!(matches!(
            observer.completed_strategies().last(),
            Some(ForgeClaimStrategy::Known(ForgeTaskStrategy::StagingFold))
        ));
    }
    server
        .forge_clock()
        .advance(chrono::Duration::days(1))
        .expect("age supervised maintenance partition");
    let controls = server
        .forge_maintenance_controls_for_test()
        .expect("supervised maintenance controls");
    controls.arm_manifest_submission();
    controls.arm_expiry_accepted();
    let failed_attempt = observer.attempts().saturating_add(1);
    let expected = observer.completed().saturating_add(1);
    observer.hold_after_next_attempt_for_test();
    server
        .fail_after_forge_maintenance_prepared_for_test()
        .expect("arm supervised maintenance Prepared crash");
    server.trigger_forge_scheduler_for_test();
    tokio::time::timeout(Duration::from_secs(30), controls.wait_manifest_submission())
        .await
        .expect("manifest pre-submission boundary reached before snapshot expiry");
    let (maintenance_task, carrier_plan): (uuid::Uuid, serde_json::Value) = sqlx::query_as(
        "SELECT task_id,plan FROM vala.forge_tasks WHERE data_tenant_id=$1 AND table_name=$2 AND strategy='snapshot_expiry' AND state='running'",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("exact running maintenance carrier at manifest pre-submission boundary");
    let selected_manifest_paths = carrier_plan["inputs"]
        .as_array()
        .expect("maintenance carrier inputs")
        .iter()
        .map(|path| path.as_str().expect("manifest path input").to_owned())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(selected_manifest_paths.len() >= 2);
    let before_table = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("load maintenance table before manifest rewrite");
    let before_manifest_facts =
        selected_manifest_facts(&before_table, &selected_manifest_paths, None).await;
    let before_live_paths = current_live_paths(&before_table).await;
    assert_eq!(
        fixture
            .operation_count("forge.snapshot_expire.committed")
            .await,
        0,
        "snapshot expiry cannot commit while the manifest future is still lazy"
    );
    controls.release_manifest_submission();
    tokio::time::timeout(Duration::from_secs(30), controls.wait_expiry_accepted())
        .await
        .expect("snapshot expiry accepted boundary");
    controls.release_expiry_accepted();
    tokio::time::timeout(
        Duration::from_secs(30),
        observer.wait_for_held_attempt_for_test(),
    )
    .await
    .expect("maintenance Prepared crash held after returning to supervisor");
    assert_eq!(observer.attempts(), failed_attempt);
    let (prepared_task, claim_expires_at, prepared_watermark, prepared_watermark_ms, prepared_plan, prepared_evidence): (uuid::Uuid, chrono::DateTime<chrono::Utc>, i64, i64, serde_json::Value, serde_json::Value) = sqlx::query_as(
        "SELECT task_id,claim_expires_at,watermark_snapshot_id,watermark_timestamp_ms,plan,evidence FROM vala.forge_tasks WHERE data_tenant_id=$1 AND table_name=$2 AND strategy='snapshot_expiry' AND state='prepared'",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("maintenance Prepared takeover evidence");
    assert_eq!(prepared_task, maintenance_task);
    assert_eq!(
        prepared_plan, carrier_plan,
        "Prepared preserves the immutable carrier plan"
    );
    assert!(
        prepared_evidence.is_object(),
        "Prepared preserves committed evidence"
    );
    observer.release_held_attempt_for_test();
    tokio::time::timeout(
        Duration::from_secs(12),
        async {
            loop {
                let expired: bool = sqlx::query_scalar(
                    "SELECT claim_expires_at < statement_timestamp() FROM vala.forge_tasks WHERE task_id=$1 AND state='prepared'",
                )
                .bind(prepared_task)
                .fetch_one(fixture.operator_pool.pool())
                .await
                .expect("observe exact Prepared lease expiry");
                if expired {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            observer.wait_for_at_least(expected).await;
        },
    )
    .await
    .expect("exact Prepared maintenance recovery after PostgreSQL lease expiry");
    let workflow = server
        .inspect_forge_workflow_for_test(fixture.tenant, &fixture.binding.table_name)
        .await
        .expect("supervised maintenance workflow");
    assert_eq!(workflow.active_claims, 0);
    assert_eq!(workflow.active_attempts, 0);
    let strategies = observer.completed_strategies();
    assert!(
        matches!(
            strategies.last(),
            Some(ForgeClaimStrategy::Known(ForgeTaskStrategy::SnapshotExpiry))
        ),
        "one SnapshotExpiry carrier completes both due maintenance effects"
    );
    let guarded: GuardedMaintenanceRow = sqlx::query_as(
        "SELECT (plan->'parameters'->>'manifest_rewrite_due')::boolean,(plan->'parameters'->>'snapshot_expiry_due')::boolean,attempt_id,claimed_by,claim_expires_at,watermark_snapshot_id,watermark_timestamp_ms,plan,evidence FROM vala.forge_tasks WHERE data_tenant_id=$1 AND table_name=$2 AND task_id=$3 AND strategy='snapshot_expiry' AND state='succeeded'",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.table_name)
    .bind(prepared_task)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("maintenance durable guards");
    assert!(guarded.0, "the carrier preserves manifest rewrite due");
    assert!(guarded.1, "the carrier preserves snapshot expiry due");
    assert_eq!(
        (guarded.2, guarded.3, guarded.4, guarded.5, guarded.6),
        (None, None, None, None, None)
    );
    assert_eq!(
        guarded.7, prepared_plan,
        "terminal carrier plan remains immutable"
    );
    for field in [
        "version",
        "committed_snapshot_id",
        "committed_metadata_location",
        "committed_metadata_digest",
        "cleanup_candidates",
    ] {
        assert_eq!(
            guarded.8[field], prepared_evidence[field],
            "terminal committed evidence field {field} remains immutable"
        );
    }
    let prepared_cleanup_cursor = prepared_evidence["deleted_candidate_count"]
        .as_u64()
        .expect("Prepared cleanup cursor");
    let terminal_cleanup_cursor = guarded.8["deleted_candidate_count"]
        .as_u64()
        .expect("terminal cleanup cursor");
    let cleanup_candidate_count = guarded.8["cleanup_candidates"]
        .as_array()
        .expect("terminal cleanup candidates")
        .len();
    assert_eq!(
        usize::try_from(terminal_cleanup_cursor).expect("bounded terminal cleanup cursor"),
        cleanup_candidate_count,
        "terminal cleanup cursor finalizes the immutable candidate list"
    );
    assert!(
        terminal_cleanup_cursor > prepared_cleanup_cursor,
        "Prepared recovery must durably advance the cleanup cursor"
    );
    assert!(claim_expires_at < chrono::Utc::now());
    assert!(prepared_watermark > 0 && prepared_watermark_ms >= 0);
    let active: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.forge_tasks WHERE data_tenant_id=$1 AND table_name=$2 AND strategy IN ('manifest_rewrite','snapshot_expiry') AND state IN ('ready','claimed','running','retryable','prepared')",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("maintenance active-work guard");
    assert_eq!(active, 0, "maintenance leaves no durable active work");
    let active_table_leases: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.maintenance_leases WHERE lease_key LIKE $1 AND expires_at >= statement_timestamp()",
    )
    .bind(format!("forge:table:{}:%", fixture.tenant))
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("maintenance active table leases");
    assert_eq!(
        active_table_leases, 0,
        "maintenance leaves no active table lease"
    );
    assert_eq!(
        fixture
            .operation_count("forge.snapshot_expire.recovered")
            .await,
        1,
        "lost-response expiry recovery has one recovered audit"
    );
    assert_eq!(
        fixture
            .operation_count("forge.snapshot_expire.committed")
            .await,
        0
    );
    let mut audit_conn = fixture
        .vala
        .tenant_conn(fixture.tenant)
        .await
        .expect("maintenance audit tenant connection");
    for operation in ["forge.task.prepared", "forge.task.succeeded"] {
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id=wyrd.current_tenant() AND operation=$1 AND resource=$2",
        )
        .bind(operation)
        .bind(format!("forge-task:{prepared_task}"))
        .fetch_one(&mut **audit_conn.transaction())
        .await
        .expect("exact maintenance task audit count");
        assert_eq!(count, 1, "{operation} must be emitted exactly once");
    }
    audit_conn
        .commit()
        .await
        .expect("close maintenance audit read");
    let after_table = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("load maintenance table after rewrite and expiry");
    let after_manifest_facts = selected_manifest_facts(
        &after_table,
        &selected_manifest_paths,
        Some(before_manifest_facts.2),
    )
    .await;
    assert!(
        after_manifest_facts.0 < before_manifest_facts.0,
        "selected same-spec manifest cardinality must decrease"
    );
    assert_eq!(
        after_manifest_facts.1, before_manifest_facts.1,
        "manifest rewrite preserves selected-bin live files"
    );
    assert_eq!(
        current_live_paths(&after_table).await,
        before_live_paths,
        "maintenance preserves all live data-file membership"
    );
    let retained = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("load retained maintenance history")
        .metadata()
        .snapshots()
        .count();
    assert!(
        retained >= 2,
        "retention keeps the current snapshot and retained history"
    );
    let settled_attempts = observer.attempts();
    trigger_supervised_scheduler(
        &server,
        Scenario {
            name: "supervised-maintenance-no-work",
            tenants: 1,
        },
    )
    .await;
    assert_eq!(
        observer.attempts(),
        settled_attempts,
        "a settled maintenance table does not loop on no work"
    );
    server
        .shutdown()
        .await
        .expect("maintenance server shutdown");
}

/// Returns selected same-spec manifest cardinality and live file membership.
async fn selected_manifest_facts(
    table: &iceberg::table::Table,
    selected_paths: &std::collections::BTreeSet<String>,
    known_partition_spec: Option<i32>,
) -> (usize, std::collections::BTreeSet<String>, i32) {
    let snapshot = table
        .metadata()
        .current_snapshot()
        .expect("maintenance current snapshot");
    let manifest_list = table
        .manifest_list_reader(snapshot)
        .load()
        .await
        .expect("maintenance manifest list");
    let selected = manifest_list
        .entries()
        .iter()
        .filter(|manifest| selected_paths.contains(&manifest.manifest_path))
        .collect::<Vec<_>>();
    let partition_spec = known_partition_spec.unwrap_or_else(|| {
        selected
            .first()
            .expect("selected manifests remain visible before rewrite")
            .partition_spec_id
    });
    assert!(
        selected
            .iter()
            .all(|manifest| manifest.partition_spec_id == partition_spec)
    );
    let mut live_paths = std::collections::BTreeSet::new();
    for manifest_file in manifest_list
        .entries()
        .iter()
        .filter(|manifest| manifest.partition_spec_id == partition_spec)
    {
        let manifest = manifest_file
            .load_manifest(table.file_io())
            .await
            .expect("selected-spec manifest");
        live_paths.extend(
            manifest
                .entries()
                .iter()
                .filter(|entry| entry.is_alive())
                .map(|entry| entry.file_path().to_owned()),
        );
    }
    let cardinality = manifest_list
        .entries()
        .iter()
        .filter(|manifest| manifest.partition_spec_id == partition_spec)
        .count();
    (cardinality, live_paths, partition_spec)
}

/// One row of the manifest-maintenance guard projection.
///
/// `sqlx::query_as` needs the column tuple spelled out, and this projection
/// reads nine columns spanning both maintenance-due flags, claim ownership,
/// watermark identity, and the persisted plan and evidence documents. Naming it
/// keeps the assertion readable and gives the shape one place to change.
type GuardedMaintenanceRow = (
    bool,
    bool,
    Option<uuid::Uuid>,
    Option<uuid::Uuid>,
    Option<chrono::DateTime<chrono::Utc>>,
    Option<i64>,
    Option<i64>,
    serde_json::Value,
    serde_json::Value,
);

/// Real Forge workers converge public small-file debt without constructing Oracle.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Bifrost journey lane"]
async fn pg_bifrost_forge_small_files_converges_without_query_dependency() {
    let forge_config = ForgeConfig {
        snapshot_expiry_enabled: false,
        max_files_per_bin: 2,
        ..ForgeConfig::default()
    };
    let cluster = WyrdTestCluster::start_spec_with_forge_config_and_completion_observer(
        BifrostClusterSpec::one_mixed().with_system_resources(forge_convergence_system_resources()),
        forge_config,
    )
    .await
    .expect("Forge-local convergence cluster");
    let checkpoint = cluster
        .telemetry()
        .checkpoint()
        .expect("Forge-local causal telemetry checkpoint");
    let server = cluster.server(0).expect("Forge-local convergence server");
    let table = prepare_forge_convergence_table(&cluster, server, 100_000)
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
        server
            .forge_clock()
            .advance(chrono::Duration::minutes(15))
            .expect("advance supervised transient retry clock");
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
        .resources()
        .snapshot()
        .expect("Forge resource baseline");
    for transition in 1..=64 {
        server
            .forge_clock()
            .advance(chrono::Duration::minutes(15))
            .expect("advance supervised rewrite retry clock");
        let expected_attempts = observer.attempts().saturating_add(1);
        let completed_passes = server.completed_forge_scheduler_passes_for_test();
        cluster.request_forge_scheduler_pass_for_test();
        tokio::time::timeout(
            Duration::from_secs(30),
            server.wait_for_forge_scheduler_passes_for_test(completed_passes + 1),
        )
        .await
        .expect("production rewrite scheduler pass completes");
        let workflow = server
            .inspect_forge_workflow_for_test(cluster.data_tenant_id(), &table)
            .await
            .expect("inspect bounded SmallFiles continuation");
        let has_live_task = workflow.tasks.iter().any(|(_, state)| {
            matches!(
                state.as_str(),
                "ready" | "retryable" | "claimed" | "running" | "prepared"
            )
        });
        if !workflow.has_demand && !has_live_task {
            break;
        }
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
                &format!("SmallFiles transition {transition} did not settle"),
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
    let lifecycle = observer.lifecycle_events();
    let committed_tasks = lifecycle
        .iter()
        .filter_map(|event| match event {
            vala_bifrost_redux::forge::ForgeLifecycleEvent::CatalogCommitted {
                task_id, ..
            } => Some(*task_id),
            _ => None,
        })
        .collect::<std::collections::BTreeSet<_>>();
    let after = server
        .inspect_forge_table_for_test(cluster.data_tenant_id(), &table)
        .await
        .expect("inspect post-compaction Forge table");
    let after_paths = after
        .live_data_files
        .iter()
        .map(|file| file.path.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let replaced_inputs = before
        .live_data_files
        .iter()
        .filter(|file| !after_paths.contains(file.path.as_str()))
        .map(|file| file.path.clone())
        .collect::<Vec<_>>();
    let planned_inputs = lifecycle
        .iter()
        .rev()
        .find_map(|event| match event {
            vala_bifrost_redux::forge::ForgeLifecycleEvent::Planned {
                task_id,
                table: observed,
                inputs,
                ..
            } if committed_tasks.contains(task_id)
                && observed == &table
                && inputs.iter().all(|input| replaced_inputs.contains(input)) =>
            {
                Some(inputs.clone())
            }
            _ => None,
        })
        .expect("one committed exact plan must remove its baseline inputs");
    assert!(!planned_inputs.is_empty());
    let rewrite = WyrdTestServer::compare_forge_rewrite_for_test(&before, &after, &replaced_inputs)
        .expect("whole convergence replacement comparison");
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
    assert_eq!(rewrite.input_files.len(), replaced_inputs.len());
    assert!(!rewrite.output_files.is_empty());
    assert!(rewrite.input_bytes > 0 && rewrite.output_bytes > 0);
    assert_eq!(after.active_claims, 0);
    assert_eq!(after.active_attempts, 0);
    assert_eq!(
        server
            .state()
            .forge()
            .expect("Forge composition after rewrite")
            .resources()
            .snapshot()
            .expect("Forge resources after rewrite"),
        forge_baseline
    );
    assert_eq!(report.capacity_refusals, 0);
    assert_eq!(report.internal_invariant_failures, 0);
    assert_eq!(report.admission_capacity_refusals, 0);
    assert_eq!(report.execution_capacity_refusals, 0);
    assert!(
        report
            .execution_envelope_failures
            .iter()
            .all(|failure| failure.count == 0)
    );
    assert!(report.changed_progress_effects > 0);
    assert_eq!(report.compaction_debt_files, 0);
    assert_eq!(report.compaction_debt_bytes, 0);
    assert!(report.attempt_resources.iter().all(|resource| {
        resource.planned_bytes > 0.0
            && resource.acquired_bytes == resource.planned_bytes
            && resource.peak_bytes <= resource.acquired_bytes
    }));
    assert!(
        report
            .resource_releases
            .iter()
            .all(|release| { release.released > 0 && release.poisoned == 0 })
    );
    cluster
        .shutdown()
        .await
        .expect("Forge-local convergence shutdown");
}

/// Builds the production-shaped resource observation used by Forge convergence.
fn forge_convergence_system_resources() -> SystemResourceSnapshot {
    SystemResourceSnapshot {
        memory_limit_bytes: 3 * 1024 * 1024 * 1024,
        effective_cpu: 4,
        scratch_capacity_bytes: 4 * 1024 * 1024 * 1024,
        scratch_available_bytes: 4 * 1024 * 1024 * 1024,
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
        .create_bifrost_table_for_test(CreateTableRequest {
            table: TableRef::new(BifrostNamespace::Bifrost, &table),
            user_fields: vec![
                Field::new("id", DataType::Int64, false),
                Field::new("value", DataType::Utf8, false),
            ],
            tenant: cluster.data_tenant_id(),
            physical_layout: None,
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

/// Advances one exact, durably classified transient Forge retry in the journey.
///
/// The guarded update repeats every classification and ownership predicate at
/// the mutation boundary. `mature_same_volume` is reserved for convergence
/// fixtures that intentionally cross the production same-volume fallback age;
/// takeover proofs leave that bound intact.
async fn advance_transient_forge_retry(
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
    unhealthy_scratch_takeover_journey().await;
}

/// Drives one qualified storage failure through healthy-volume takeover.
///
/// # Panics
///
/// Panics when the production failure classification, quarantine, retry,
/// publication, telemetry, or resource release contract diverges.
#[cfg(unix)]
async fn unhealthy_scratch_takeover_journey() {
    let (server, telemetry) = start_telemetry_maintenance_server().await;
    let checkpoint = telemetry
        .checkpoint()
        .expect("takeover causal telemetry checkpoint");
    let fixture = seed_forge_group(&server, "journey_unhealthy_scratch_takeover").await;
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
    let mut inputs: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT file_path FROM vala.file_list WHERE data_tenant_id=$1 AND namespace=$2 AND table_name=$3 ORDER BY file_path",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_all(fixture.operator_pool.pool())
    .await
    .expect("takeover inputs");
    inputs.sort_unstable();
    inputs.dedup();
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
            vala_bifrost_redux::catalog::layout::TimePartition::new(
                vala_bifrost_redux::catalog::layout::TimeGranularity::Day,
                chrono::NaiveDate::from_ymd_opt(2026, 7, 14)
                    .expect("takeover hint day")
                    .and_hms_opt(0, 0, 0)
                    .expect("midnight")
                    .and_utc(),
            )
            .expect("midnight is a daily partition boundary"),
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
    advance_transient_forge_retry(&server, task_id, "storage_health", false).await;
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
    assert_eq!(
        report
            .retries
            .iter()
            .find(|row| row.failure_class == ForgeTelemetryFailureClass::StorageHealth)
            .expect("storage-health retry telemetry row")
            .count,
        1
    );
    assert!(report.attempt_resources.iter().all(|resource| {
        matches!(
            resource.resource,
            ForgeTelemetryResource::Memory | ForgeTelemetryResource::Scratch
        ) && resource.planned_bytes > 0.0
            && resource.acquired_bytes == resource.planned_bytes
            && resource.peak_bytes <= resource.acquired_bytes
    }));
    assert!(
        report
            .resource_releases
            .iter()
            .all(|release| { release.released > 0 && release.poisoned == 0 })
    );
    server.shutdown().await.expect("takeover server shutdown");
}

/// Exercises production fail-stop supervision and exact recovery after Forge panics.
///
/// # Panics
///
/// Panics when supervision does not fail stop, the durable attempt cannot be
/// reclaimed, publication is duplicated, or attempt resources remain owned.
async fn supervised_forge_panic_recovery_scenario() {
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

/// Proves production supervision fails stop on Forge panics and restart recovers once.
///
/// # Panics
///
/// Panics when the shared panic-recovery scenario violates its supervision,
/// durable recovery, publication, or resource-settlement invariants.
#[tokio::test]
#[ignore = "gated journey: real supervised server panic and lease-expiry recovery"]
async fn supervised_forge_panic_fails_stop_and_recovers_exactly_once() {
    supervised_forge_panic_recovery_scenario().await;
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
    forge_orphan_cleanup_lifecycle_journey().await;
}

/// Drives one reset generation through protected production orphan collection.
///
/// # Panics
///
/// Panics when an aged reset generation is not reclaimed exclusively through
/// maintenance or when its terminal orphan audit is absent.
async fn forge_orphan_cleanup_lifecycle_journey() {
    let server = start_maintenance_journey_server().await;
    let fixture = native_forge_group(&server, "journey_orphan_cleanup").await;
    commit_journey_staging_snapshot(&server, &fixture).await;
    let (input_path, partition_granularity, partition_start): (
        String,
        String,
        chrono::DateTime<chrono::Utc>,
    ) = sqlx::query_as(
        "SELECT file_path, partition_granularity, partition_start FROM vala.file_list \
         WHERE data_tenant_id=$1 AND namespace=$2 AND table_name=$3 \
         AND committed_snapshot_id IS NOT NULL ORDER BY id LIMIT 1",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("journey live input path");
    let partition = vala_bifrost_redux::catalog::layout::TimePartition::from_durable_columns(
        &partition_granularity,
        partition_start,
    )
    .expect("durable file-list rows carry an exact time partition");
    let table = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("journey reset table");
    let base_snapshot_id = table
        .metadata()
        .current_snapshot_id()
        .expect("journey reset base snapshot");
    let operation_id = uuid::Uuid::now_v7();
    let orphan = format!(
        "{}/data/forge/bifrost-writer-v1/{operation_id}-00000.parquet",
        fixture.binding.object_prefix,
    );
    fixture
        .staging
        .write(&orphan, Buffer::from(vec![9_u8]))
        .await
        .expect("journey reset generation object");
    let resource = format!(
        "bifrost://{}/{}/{}",
        fixture.tenant, fixture.binding.table_ref.namespace, fixture.binding.table_ref.name,
    );
    let detail = AuditDetail::ForgeIcebergRewrite {
        operation_id,
        phase: ForgeIcebergRewritePhase::Prepared,
        group: resource,
        base_snapshot_id,
        committed_snapshot_id: None,
        partition_spec_id: table.metadata().default_partition_spec_id(),
        time_partition: partition.to_wire(),
        target_file_size_bytes: 1,
        input_paths: vec![StoragePath::new(input_path).expect("journey live input storage path")],
        output_paths: vec![
            StoragePath::new(orphan.clone()).expect("journey reset output storage path"),
        ],
        writer_recipe_version: "bifrost-writer-v1".to_owned(),
    };
    let object_store = ForgeObjectStoreControl::new(Arc::clone(&fixture.staging));
    let mut config = fixture.config.clone();
    config.orphan_gc_ttl = Duration::from_millis(1);
    let forge = fixture.context_with_object_store(config, Arc::clone(&object_store));
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
    .expect("journey reset lease query")
    .expect("journey reset lease");
    forge
        .append_prepared_live_rewrite_for_test(&mut lease, &fixture.binding, partition, detail)
        .await
        .expect("journey Prepared live rewrite");
    let reconcile_now = chrono::Utc::now()
        + chrono::Duration::from_std(fixture.config.uncertainty_bound)
            .expect("journey uncertainty bound")
        + chrono::Duration::seconds(1);
    let reconciled = forge
        .reconcile_live_replacements_for_test(
            &mut lease,
            &fixture.binding,
            &CancellationToken::new(),
            reconcile_now,
        )
        .await
        .expect("journey Reset reconciliation");
    assert_eq!(
        reconciled.reset, 1,
        "stable Prepared output must Reset once"
    );
    assert!(
        object_store.delete_paths().is_empty(),
        "Reset reconciliation must enqueue without remote deletion"
    );
    assert!(
        fixture.staging.stat(&orphan).await.is_ok(),
        "Reset output remains durable for delayed orphan GC"
    );
    lease
        .release(&fixture.operator_pool)
        .await
        .expect("journey reset lease release");
    let modified = fixture
        .staging
        .stat(&orphan)
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
    assert_eq!(
        forge
            .run_orphan_gc_for_test(&fixture.binding)
            .await
            .expect("aged reset orphan-GC pass"),
        1,
        "orphan GC must delete the aged Reset generation once"
    );
    assert!(fixture.staging.stat(&orphan).await.is_err());
    assert_eq!(
        object_store
            .delete_paths()
            .iter()
            .filter(|path| path.as_str() == orphan.as_str())
            .count(),
        1,
        "orphan GC is the sole exact-once physical-delete owner"
    );
    assert_eq!(
        forge
            .run_orphan_gc_for_test(&fixture.binding)
            .await
            .expect("idempotent reset orphan-GC pass"),
        0,
        "successor orphan GC must not repeat deletion"
    );
    assert_eq!(
        fixture.operation_count("forge.orphan_gc.committed").await
            + fixture.operation_count("forge.orphan_gc.recovered").await,
        1,
        "one terminal orphan-GC audit must settle the deletion"
    );
    server
        .shutdown()
        .await
        .expect("journey orphan server shutdown");
}

/// Proves a verified output whose `Prepared` audit fails is reclaimed only by orphan GC.
///
/// The production rewrite is held immediately after the shared uploader has
/// verified a real object. The injected audit failure then leaves that object
/// without a `Prepared` or Reset projection. Production orphan GC first
/// protects it below the TTL floor, then deletes it under the exact table lease
/// after the server-owned clock makes it eligible.
///
/// # Panics
///
/// Panics when the verified upload is deleted by the rewrite path, becomes
/// eligible before its TTL, survives an eligible production GC pass, is
/// deleted more than once, or lacks exactly one terminal orphan-GC audit.
#[tokio::test]
#[ignore = "supporting integration seam: direct owner controls"]
async fn supporting_preprepared_verified_output_orphan_gc_journey() {
    let server_config = vala_bifrost_redux::forge::ForgeConfig {
        min_files: 2,
        max_files_per_bin: 2,
        max_files_per_tick: 2,
        ..vala_bifrost_redux::forge::ForgeConfig::default()
    };
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .with_forge_process_role_for_test(BifrostTarget::All)
        .with_forge_config_for_test(server_config)
        .start_bound()
        .await
        .expect("bound pre-Prepared orphan journey server");
    let fixture = native_forge_group(&server, "journey_prepared_audit_failure_gc").await;
    commit_journey_staging_snapshot(&server, &fixture).await;
    append_native_forge_cycles(&server, &fixture, 3..5).await;
    commit_journey_staging_snapshot(&server, &fixture).await;

    server
        .forge_clock()
        .advance(chrono::Duration::days(1) + chrono::Duration::minutes(3))
        .expect("age committed files beyond the live-rewrite cutoff");
    let (partition_granularity, partition_start): (String, chrono::DateTime<chrono::Utc>) =
        sqlx::query_as(
            "SELECT partition_granularity, partition_start FROM vala.file_list \
             WHERE data_tenant_id=$1 AND namespace=$2 AND table_name=$3 \
             AND committed_snapshot_id IS NOT NULL ORDER BY partition_start LIMIT 1",
        )
        .bind(fixture.tenant.as_uuid())
        .bind(&fixture.binding.logical_namespace)
        .bind(&fixture.binding.table_name)
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("live-rewrite partition cutoff");
    let partition_day = vala_bifrost_redux::catalog::layout::TimePartition::from_durable_columns(
        &partition_granularity,
        partition_start,
    )
    .expect("durable file-list rows carry an exact time partition");
    assert_eq!(
        server
            .forge_publisher()
            .try_publish(StagingFileCommitted::new(
                fixture.binding.clone(),
                partition_day,
            )),
        StagingPublishOutcome::Published,
        "production demand channel must accept the live-rewrite hint"
    );

    let object_store = ForgeObjectStoreControl::new(Arc::clone(&fixture.staging));
    let mut rewrite_config = fixture.config.clone();
    rewrite_config.min_files = 2;
    rewrite_config.max_files_per_bin = 2;
    rewrite_config.max_files_per_tick = 2;
    let forge = fixture.context_with_object_store(rewrite_config, Arc::clone(&object_store));
    let completed_passes = server.completed_forge_scheduler_passes_for_test();
    server.request_forge_scheduler_pass_for_test();
    server
        .wait_for_forge_scheduler_passes_for_test(completed_passes + 1)
        .await;
    let queued: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM vala.forge_tasks \
         WHERE data_tenant_id=$1 AND namespace_name=$2 AND table_name=$3 \
         AND strategy='live_rewrite' AND state IN ('ready','retryable'))",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("observe server-owned live-rewrite enqueue");
    let planner_diagnostics: Vec<(String, String)> = sqlx::query_as(
        "SELECT strategy,state FROM vala.forge_tasks WHERE data_tenant_id=$1 ORDER BY created_at",
    )
    .bind(fixture.tenant.as_uuid())
    .fetch_all(fixture.operator_pool.pool())
    .await
    .expect("read production planner diagnostics");
    assert!(
        queued,
        "production planner must enqueue the live rewrite: {planner_diagnostics:?}"
    );
    forge.fail_next_prepared_live_audit_for_test();
    object_store.pause_after_next_output_put();
    let worker = ForgeWorker::new(
        Arc::clone(&forge),
        ForgeWorkerConfig::default(),
        uuid::Uuid::now_v7(),
    )
    .expect("pre-Prepared failure worker");
    let attempt =
        tokio::spawn(async move { worker.execute_one_for_test(&CancellationToken::new()).await });
    tokio::time::timeout(Duration::from_secs(30), object_store.wait_for_output_put())
        .await
        .expect("verified output PUT boundary");
    let orphan = object_store
        .last_output_path()
        .expect("verified output path");
    assert!(
        fixture.staging.stat(&orphan).await.is_ok(),
        "shared uploader must leave verified bytes durable"
    );
    object_store.release_output_put();
    assert!(
        tokio::time::timeout(Duration::from_secs(30), attempt)
            .await
            .expect("pre-Prepared failure worker bound")
            .expect("pre-Prepared failure worker join")
            .is_err(),
        "the injected Prepared audit failure must remain authoritative"
    );
    assert_eq!(
        fixture
            .operation_count("forge.iceberg_rewrite.prepared")
            .await,
        0,
        "the failure must precede durable Prepared evidence"
    );
    assert!(
        object_store.delete_paths().is_empty(),
        "the rewrite path must leave remote deletion exclusively to orphan GC"
    );
    assert_eq!(
        forge
            .gc_eligibility_for_test(&fixture.binding, &orphan)
            .await
            .expect("young pre-Prepared eligibility"),
        "TooYoung",
        "the TTL floor protects a freshly verified generation"
    );
    assert_eq!(
        forge
            .run_orphan_gc_for_test(&fixture.binding)
            .await
            .expect("young pre-Prepared GC pass"),
        0,
        "orphan GC must not delete below the TTL floor"
    );
    assert!(
        fixture.staging.stat(&orphan).await.is_ok(),
        "the young verified generation must remain durable"
    );

    let modified = fixture
        .staging
        .stat(&orphan)
        .await
        .expect("pre-Prepared orphan metadata")
        .last_modified()
        .expect("pre-Prepared orphan modification time")
        .into_inner()
        .as_millisecond();
    server
        .forge_clock()
        .set(
            chrono::DateTime::from_timestamp_millis(modified + 100)
                .expect("pre-Prepared orphan timestamp is UTC-representable"),
        )
        .expect("advance pre-Prepared orphan clock");
    let mut gc_config = fixture.config.clone();
    gc_config.orphan_gc_ttl = Duration::from_millis(1);
    let gc_forge = fixture.context_with_object_store(gc_config, Arc::clone(&object_store));
    assert_eq!(
        gc_forge
            .gc_eligibility_for_test(&fixture.binding, &orphan)
            .await
            .expect("aged pre-Prepared eligibility"),
        "Eligible"
    );
    let terminal_before = fixture.operation_count("forge.orphan_gc.committed").await
        + fixture.operation_count("forge.orphan_gc.recovered").await;
    assert!(
        gc_forge
            .run_orphan_gc_for_test(&fixture.binding)
            .await
            .expect("aged pre-Prepared GC pass")
            >= 1,
        "production orphan GC must reclaim the aged generation"
    );
    assert!(
        fixture.staging.stat(&orphan).await.is_err(),
        "the aged pre-Prepared generation must be absent"
    );
    assert_eq!(
        object_store
            .delete_paths()
            .iter()
            .filter(|path| path.as_str() == orphan.as_str())
            .count(),
        1,
        "the exact generation must be deleted once"
    );
    let terminal_after = fixture.operation_count("forge.orphan_gc.committed").await
        + fixture.operation_count("forge.orphan_gc.recovered").await;
    assert_eq!(
        terminal_after,
        terminal_before + 1,
        "one orphan-GC terminal audit must settle the deletion"
    );
    assert_eq!(
        gc_forge
            .run_orphan_gc_for_test(&fixture.binding)
            .await
            .expect("idempotent pre-Prepared GC pass"),
        0,
        "a successor pass must not repeat the deletion"
    );
    assert_eq!(
        object_store
            .delete_paths()
            .iter()
            .filter(|path| path.as_str() == orphan.as_str())
            .count(),
        1,
        "successor GC must preserve exactly-once deletion"
    );
    server
        .shutdown()
        .await
        .expect("pre-Prepared orphan server shutdown");
}

/// Proves ambiguous publication recovery and protected orphan GC converge once.
///
/// # Panics
///
/// Panics when the production recovery path duplicates a catalog update or
/// rows, or when protected maintenance fails to reclaim its reset generation.
#[tokio::test]
#[ignore = "gated journey: real Forge publication recovery and orphan collection"]
async fn pg_bifrost_forge_publication_recovery_and_orphan_gc() {
    supervised_prepared_audit_failure_and_gc_journey().await;
    supervised_uncertain_commit_recovery_journey().await;
}

/// Drives verified publication failure and later GC through the bound Forge roles.
///
/// # Panics
///
/// Panics when the durable planner does not expose the exact small-files
/// live-rewrite plan, the shared uploader boundary is not reached, the
/// supervised failure performs eager deletion, or its lawful snapshot-expiry
/// carrier does not settle orphan collection exactly once.
async fn supervised_prepared_audit_failure_and_gc_journey() {
    let observer = ForgeWorkerCompletionObserver::new();
    let config = ForgeConfig {
        min_files: 2,
        max_files_per_bin: 64,
        max_files_per_tick: 64,
        orphan_gc_ttl: Duration::from_millis(1),
        snapshot_retention: Duration::from_hours(48),
        ..ForgeConfig::default()
    };
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .with_forge_config_for_test(config)
        .with_forge_completion_observer_for_test(observer.clone())
        .start_bound()
        .await
        .expect("bound publication supervision server");
    let fixture = seed_forge_group(&server, "journey_supervised_prepared_gc").await;
    server
        .forge_clock()
        .advance(chrono::Duration::days(1) + chrono::Duration::minutes(3))
        .expect("close supervised publication staging partition");
    let latest_uncompacted_partition: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT max(partition_start) FROM vala.file_list \
         WHERE data_tenant_id=$1 AND namespace=$2 AND table_name=$3 \
         AND NOT compacted",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("supervised publication uncompacted partition day");
    assert!(
        latest_uncompacted_partition.is_some_and(|partition_start| {
            partition_start
                < server
                    .forge_clock()
                    .now()
                    .expect("supervised publication Forge clock")
        }),
        "all uncompacted publication inputs must belong to a closed partition"
    );
    let controls = server
        .forge_object_store_control_for_test()
        .expect("supervised Forge object-store controls");
    let mut staging_worker_held = false;
    for generation in 0..2 {
        if generation == 1 {
            observer.hold_after_next_attempt_for_test();
            let table = fixture
                .catalog
                .load_table(&fixture.binding.table_ident())
                .await
                .expect("publication first-fold table");
            let target_update = Transaction::new(&table).update_table_properties().set(
                "write.target-file-size-bytes".to_owned(),
                (10 * 1024 * 1024).to_string(),
            );
            ApplyTransactionAction::apply(target_update, Transaction::new(&table))
                .expect("multi-output target property update")
                .commit(fixture.catalog.as_ref())
                .await
                .expect("multi-output target property commit");
            append_native_forge_group_with_rows(
                &server,
                &fixture,
                0..8,
                14 * FORGE_CONVERGENCE_BATCH_ROWS,
            )
            .await;
        }
        let expected = observer.completed().saturating_add(1);
        let expected_attempts = observer.attempts().saturating_add(1);
        trigger_supervised_scheduler(
            &server,
            Scenario {
                name: "supervised-staging-publication",
                tenants: 1,
            },
        )
        .await;
        if generation == 1 {
            tokio::time::timeout(
                Duration::from_secs(30),
                observer.wait_for_attempts_at_least(expected_attempts),
            )
            .await
            .expect("supervised staging publication attempt");
            tokio::time::timeout(
                Duration::from_secs(30),
                observer.wait_for_held_attempt_for_test(),
            )
            .await
            .expect("hold publication worker after staging");
            staging_worker_held = true;
        } else {
            tokio::time::timeout(
                Duration::from_secs(30),
                observer.wait_for_at_least(expected),
            )
            .await
            .expect("supervised staging publication");
            assert!(matches!(
                observer.completed_strategies().last(),
                Some(ForgeClaimStrategy::Known(ForgeTaskStrategy::StagingFold))
            ));
        }
    }
    for _ in 0..4 {
        let remaining: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.file_list WHERE data_tenant_id=$1 AND namespace=$2 AND table_name=$3 AND NOT compacted",
        )
        .bind(fixture.tenant.as_uuid())
        .bind(&fixture.binding.logical_namespace)
        .bind(&fixture.binding.table_name)
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("publication staging remainder");
        if remaining == 0 {
            break;
        }
        assert!(staging_worker_held);
        let expected_attempts = observer.attempts().saturating_add(1);
        observer.hold_after_next_attempt_for_test();
        observer.release_held_attempt_for_test();
        trigger_supervised_scheduler(
            &server,
            Scenario {
                name: "supervised-staging-publication-remainder",
                tenants: 1,
            },
        )
        .await;
        tokio::time::timeout(
            Duration::from_secs(30),
            observer.wait_for_attempts_at_least(expected_attempts),
        )
        .await
        .expect("supervised staging publication remainder attempt");
        tokio::time::timeout(
            Duration::from_secs(30),
            observer.wait_for_held_attempt_for_test(),
        )
        .await
        .expect("hold publication worker after staging remainder");
        staging_worker_held = true;
    }
    let uncompacted_files: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.file_list WHERE data_tenant_id=$1 AND namespace=$2 AND table_name=$3 AND NOT compacted",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("closed-partition uncompacted file count");
    assert_eq!(
        uncompacted_files, 0,
        "both staging folds must be catalog-published"
    );
    let table = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("publication catalog table");
    assert!(
        table.metadata().snapshots().count() >= 2,
        "the staging folds must create multiple snapshots"
    );
    let catalog_paths = current_live_paths(&table)
        .await
        .into_iter()
        .collect::<Vec<_>>();
    assert!(
        catalog_paths.len() >= 2,
        "the two staging folds must expose multiple physical outputs"
    );
    let nonterminal_expiry_tasks: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.forge_tasks WHERE data_tenant_id=$1 AND table_name=$2 AND strategy='snapshot_expiry' AND state IN ('ready','claimed','running','prepared','retryable')",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("pre-rewrite snapshot-expiry task count");
    assert_eq!(
        nonterminal_expiry_tasks, 0,
        "snapshot retention must not compete with the live rewrite"
    );
    trigger_supervised_scheduler(
        &server,
        Scenario {
            name: "supervised-incomplete-multi-output-plan",
            tenants: 1,
        },
    )
    .await;
    let (planned_state, planned_inputs): (String, Vec<String>) = sqlx::query_as(
        "SELECT state,ARRAY(SELECT jsonb_array_elements_text(plan->'inputs') ORDER BY 1) FROM vala.forge_tasks WHERE data_tenant_id=$1 AND table_name=$2 AND strategy='small_files' AND state IN ('ready','retryable') ORDER BY created_at DESC LIMIT 1",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("observe planned multi-output rewrite");
    assert_eq!(planned_state, "ready");
    assert!(
        planned_inputs.len() >= 2
            && planned_inputs
                .iter()
                .all(|input| catalog_paths.contains(input)),
        "the multi-output failure must target one whole multi-file bin"
    );
    let publication_base = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("multi-output publication base");
    let publication_snapshot = publication_base
        .metadata()
        .current_snapshot_id()
        .expect("multi-output publication snapshot");
    let prepared_before = fixture
        .operation_count("forge.iceberg_rewrite.prepared")
        .await;
    let verified_outputs_before = controls.output_put_calls();
    let returned_errors_before = observer.returned_errors().len();
    controls.fail_output_put_at_ordinal_for_test(1);
    controls.pause_after_next_output_put();
    let expected_failure_attempts = observer.attempts().saturating_add(1);
    assert!(staging_worker_held);
    observer.release_held_attempt_for_test();
    trigger_supervised_scheduler(
        &server,
        Scenario {
            name: "supervised-incomplete-multi-output",
            tenants: 1,
        },
    )
    .await;
    tokio::time::timeout(Duration::from_secs(30), controls.wait_for_output_put())
        .await
        .expect("verified ordinal-zero output PUT");
    let incomplete_output = controls
        .last_output_path()
        .expect("verified ordinal-zero output path");
    controls.release_output_put();
    tokio::time::timeout(
        Duration::from_secs(30),
        observer.wait_for_attempts_at_least(expected_failure_attempts),
    )
    .await
    .expect("incomplete multi-output attempt");
    let incomplete_table = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("table after incomplete multi-output attempt");
    assert_eq!(
        controls.output_put_calls(),
        verified_outputs_before + 1,
        "output ordinal zero must verify before ordinal one reaches its injected failure"
    );
    assert_eq!(
        observer.returned_errors().len(),
        returned_errors_before + 1,
        "the selected second-output failure must return from the supervised attempt"
    );
    assert_eq!(
        fixture
            .operation_count("forge.iceberg_rewrite.prepared")
            .await,
        prepared_before,
        "an incomplete verified output set must not append Prepared evidence"
    );
    assert_eq!(
        incomplete_table.metadata().current_snapshot_id(),
        Some(publication_snapshot),
        "an incomplete verified output set must not replace the Iceberg snapshot"
    );
    assert!(
        fixture.staging.stat(&incomplete_output).await.is_ok(),
        "the verified prefix remains available to delayed fenced orphan GC"
    );
    let incomplete_task: uuid::Uuid = sqlx::query_scalar(
        "SELECT task_id FROM vala.forge_tasks WHERE data_tenant_id=$1 AND table_name=$2 \
         AND strategy='small_files' AND state='retryable' AND failure_class='transient_object_store' \
         AND attempt_id IS NULL AND claimed_by IS NULL ORDER BY created_at DESC LIMIT 1",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("incomplete live rewrite retry row");
    server
        .fail_next_forge_prepared_audit_for_test()
        .expect("arm Prepared audit failure");
    controls.pause_after_next_output_put();
    advance_transient_forge_retry(&server, incomplete_task, "transient_object_store", false).await;
    let expected_attempts = observer.attempts().saturating_add(1);
    trigger_supervised_scheduler(
        &server,
        Scenario {
            name: "supervised-prepared-gc",
            tenants: 1,
        },
    )
    .await;
    tokio::time::timeout(Duration::from_secs(30), controls.wait_for_output_put())
        .await
        .expect("supervised verified output PUT");
    let output = controls
        .last_output_path()
        .expect("supervised verified output path");
    let (strategy, state, base_snapshot_id, inputs): (String, String, i64, Vec<String>) =
        sqlx::query_as(
        "SELECT strategy,state,base_snapshot_id,ARRAY(SELECT jsonb_array_elements_text(plan->'inputs') ORDER BY 1) FROM vala.forge_tasks WHERE data_tenant_id=$1 AND table_name=$2 AND strategy='small_files' AND plan->'parameters'->>'kind'='live_rewrite' ORDER BY created_at DESC LIMIT 1",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("observe exact durable live-rewrite plan");
    assert_eq!(strategy, "small_files");
    assert_eq!(state, "running");
    assert_eq!(
        base_snapshot_id,
        table
            .metadata()
            .current_snapshot_id()
            .expect("current base snapshot")
    );
    assert!(
        inputs.len() >= 2 && inputs.iter().all(|input| catalog_paths.contains(input)),
        "the durable plan must retain one whole pinned catalog bin"
    );
    assert!(fixture.staging.stat(&output).await.is_ok());
    controls.release_output_put();
    tokio::time::timeout(
        Duration::from_secs(30),
        observer.wait_for_attempts_at_least(expected_attempts),
    )
    .await
    .expect("Prepared audit failure observation");
    assert_eq!(
        fixture
            .operation_count("forge.iceberg_rewrite.prepared")
            .await,
        0
    );
    assert!(controls.delete_paths().is_empty());
    assert!(fixture.staging.stat(&output).await.is_ok());
    server
        .forge_clock()
        .advance(chrono::Duration::days(1) + chrono::Duration::minutes(3))
        .expect("make snapshot retention and orphan TTL due");
    let terminal_before = fixture.operation_count("forge.orphan_gc.committed").await
        + fixture.operation_count("forge.orphan_gc.recovered").await;
    controls.pause_next_delete();
    server.trigger_forge_scheduler_for_test();
    tokio::time::timeout(Duration::from_secs(30), controls.wait_for_delete())
        .await
        .expect("orphan GC final recheck admitted one delete");
    assert!(fixture.staging.stat(&output).await.is_ok());
    let (expiry_task, expiry_state, snapshot_expiry_due): (uuid::Uuid, String, bool) =
        sqlx::query_as(
            "SELECT task_id,state,(plan->'parameters'->>'snapshot_expiry_due')::boolean FROM vala.forge_tasks WHERE data_tenant_id=$1 AND table_name=$2 AND strategy='snapshot_expiry' AND state='prepared' ORDER BY created_at DESC LIMIT 1",
        )
        .bind(fixture.tenant.as_uuid())
        .bind(&fixture.binding.table_name)
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("prepared snapshot-expiry orphan-GC carrier");
    assert_eq!(expiry_state, "prepared");
    assert!(
        snapshot_expiry_due,
        "the lawful carrier must be due for snapshot expiry"
    );
    observer.hold_after_next_attempt_for_test();
    controls.release_paused_delete();
    tokio::time::timeout(
        Duration::from_secs(30),
        observer.wait_for_held_attempt_for_test(),
    )
    .await
    .expect("held snapshot-expiry attempt after durable execution");
    let expiry_terminal_state: String = sqlx::query_scalar(
        "SELECT state FROM vala.forge_tasks WHERE data_tenant_id=$1 AND task_id=$2",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(expiry_task)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("terminal snapshot-expiry carrier");
    let mut conn = fixture
        .vala
        .tenant_conn(fixture.tenant)
        .await
        .expect("snapshot-expiry audit tenant connection");
    let expiry_terminal_audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id=wyrd.current_tenant() AND operation='forge.task.succeeded' AND resource=$1",
    )
    .bind(format!("forge-task:{expiry_task}"))
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("terminal snapshot-expiry carrier audit count");
    assert_eq!(expiry_terminal_state, "succeeded");
    assert_eq!(expiry_terminal_audits, 1);
    assert!(fixture.staging.stat(&output).await.is_err());
    assert!(
        fixture.staging.stat(&incomplete_output).await.is_err(),
        "orphan GC must reclaim the aged pre-Prepared output prefix"
    );
    assert_eq!(
        controls
            .delete_paths()
            .iter()
            .filter(|path| path.as_str() == output.as_str())
            .count(),
        1,
        "orphan GC deletes the verified generation once"
    );
    assert_eq!(
        controls
            .delete_paths()
            .iter()
            .filter(|path| path.as_str() == incomplete_output.as_str())
            .count(),
        1,
        "orphan GC deletes the incomplete generation prefix once"
    );
    let terminal_after = fixture.operation_count("forge.orphan_gc.committed").await
        + fixture.operation_count("forge.orphan_gc.recovered").await;
    assert_eq!(terminal_after, terminal_before + 1);
    let shutdown = tokio::spawn(async move { server.shutdown().await });
    tokio::task::yield_now().await;
    observer.release_held_attempt_for_test();
    shutdown
        .await
        .expect("publication shutdown task")
        .expect("publication server shutdown");
}

/// Read the sorted live data-file membership of the current Iceberg snapshot.
async fn current_live_paths(table: &iceberg::table::Table) -> std::collections::BTreeSet<String> {
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

/// Proves Forge fixture rebuilds retain the server-owned wall clock.
#[tokio::test]
#[ignore = "gated journey: real bound Wyrd server, Postgres, and Forge"]
async fn forge_fixture_rebuilds_retain_manual_forge_clock() {
    let server = WyrdTestServer::builder()
        .start_bound()
        .await
        .expect("server");
    let fixture = native_forge_group(&server, "manual_forge_clock").await;
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
    let config = ForgeConfig {
        lease_ttl: Duration::from_millis(1),
        ..ForgeConfig::default()
    };
    let cluster = WyrdTestCluster::start_with_dedicated_forge_workers_with_config_for_test(config)
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
            BifrostTarget::Server,
            BifrostTarget::ForgeWorker,
            BifrostTarget::ForgeWorker,
            BifrostTarget::ForgeWorker,
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
    let expected_rows = u64::try_from(WRITE_CYCLES).expect("bounded dedicated journey row count");
    for tenant in &tenants {
        assert_eq!(
            query_rows(scheduler_server, tenant).await,
            expected_rows,
            "dedicated worker tenant {} rows",
            tenant.id
        );
        assert_terminal_audits(scheduler_server, tenant.id, scenario).await;
        assert_snapshot(scheduler_server, tenant, scenario).await;
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
        BifrostTarget::All
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
        worker.forge_process_role() == BifrostTarget::ForgeWorker
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
    supervised_worker_claim_expiry_journey().await;
}

/// Drives one abandoned durable claim through expiry and exact worker reclaim.
///
/// # Panics
///
/// Panics when claim expiry, successor ownership, public rows, durable evidence,
/// audit settlement, or lease release diverges from the production contract.
async fn supervised_worker_claim_expiry_journey() {
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
        server
            .state()
            .forge_coordinator()
            .expect("server-owned Forge")
            .as_ref(),
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

    let expected_rows = u64::try_from(WRITE_CYCLES).expect("bounded role fixture rows");
    let mut rows = Vec::with_capacity(tenants.len());
    let mut terminal_tasks = Vec::with_capacity(tenants.len());
    let mut terminal_audits = Vec::with_capacity(tenants.len());
    let mut evidence_rows = Vec::with_capacity(tenants.len());
    let mut watermark_rows = Vec::with_capacity(tenants.len());
    for tenant in &tenants {
        let row_count = query_rows(server, tenant).await;
        assert_eq!(row_count, expected_rows, "{name} tenant {} rows", tenant.id);
        assert_terminal_audits(server, tenant.id, scenario).await;
        assert_snapshot(server, tenant, scenario).await;
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
    /// Tenant-scoped access token accepted by the public query endpoint.
    jwt: String,
    /// API key used by the public native Arrow SDK transport.
    api_key: SecretString,
    /// Tenant-local native Bifrost table receiving the journey rows.
    table: String,
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
fn native_journey_ipc(sequence: u64) -> Vec<u8> {
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
async fn query_response(server: &WyrdTestServer, tenant: &TenantWriter) -> reqwest::Response {
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
async fn query_rows(server: &WyrdTestServer, tenant: &TenantWriter) -> u64 {
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
async fn supervised_reclaim_query_rows(server: &WyrdTestServer, tenant: &TenantWriter) -> u64 {
    query_rows(server, tenant).await
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

/// Assert the tenant's native journey table has at least one committed snapshot.
///
/// # Panics
///
/// Panics when the Redux catalog or table cannot be loaded.
async fn assert_snapshot(server: &WyrdTestServer, tenant: &TenantWriter, scenario: Scenario) {
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
