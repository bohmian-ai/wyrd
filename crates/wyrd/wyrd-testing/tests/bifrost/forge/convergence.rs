//! Small-file convergence without a query dependency, and deferral when a
//! scratch volume is unhealthy.
//!
//! Module of the `forge` group; shared fixtures live in `support.rs`.

use arrow::array::{ArrayRef, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
use vala_bifrost_redux::forge::{
    ForgeConfig, ForgeScheduler, ForgeSchedulerTrigger, ForgeWorker, ForgeWorkerCompletionObserver,
    ForgeWorkerConfig,
};
use vala_bifrost_redux::maintenance::StagingFileCommitted;
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
use wyrd_spec::DataTenantId;
use wyrd_testing::bifrost::forge_harness::seed_forge_group;
use wyrd_testing::bifrost::{
    BifrostClusterSpec, ForgeCausalDiagnosis, ForgeCausalTelemetryReport,
    ForgeTelemetryFailureClass, ForgeTelemetryResource, WyrdTestCluster,
};
use wyrd_testing::{Bootstrap, WyrdTestServer};

use super::support::*;

/// Public-ingest batches durably sealed together while creating small-file debt.
const FORGE_CONVERGENCE_BATCHES_PER_SEAL: usize = 10;

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
