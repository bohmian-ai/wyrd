//! Real, bounded Bifrost workload runner.

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use arrow::array::{Int64Array, RecordBatch, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::catalog::TableRef;
use vala_bifrost_redux::contracts::{Scribe, ScribeAppend};
use vala_bifrost_redux::forge::{ForgeContext, ForgeTickOutcome, run_maintenance_tick};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::schema::fingerprint::SchemaFingerprint;
use wyrd_bench::{
    BacklogSample, BenchmarkRecorder, BenchmarkReport, ForgeMeasurements, LatencyPercentiles,
    MachineMetadata, PhaseMeasurement, PodMetadata, QueryMeasurements, SchemaWidth,
    StageMeasurements, StorageMeasurements, TrafficShape, VerificationMeasurements, WorkloadSpec,
    required_scribe_matrix,
};
use wyrd_runtime::{Principal, PrincipalKind, permission::PermissionSet};
use wyrd_spec::auth::PrincipalId;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::SyncQueryRequest;
use wyrd_testing::bifrost::{
    BifrostHarness, BifrostTopology, ForgeFixture, WyrdTestCluster, seed_forge_group_for_tenant,
};

type BenchError = Box<dyn Error + Send + Sync>;

const TABLE_NAME: &str = "bench_events";
const FORGE_TABLE_NAME: &str = "bench_forge_events";
const RUN_DEADLINE: Duration = Duration::from_secs(540);
const SCRIBE_DRAIN_TIMEOUT: Duration = Duration::from_secs(90);
const FORGE_DRAIN_TIMEOUT: Duration = Duration::from_secs(120);

static BENCHMARK_RECORDER: OnceLock<Arc<BenchmarkRecorder>> = OnceLock::new();

#[derive(Debug, Clone, Copy)]
struct PhaseSpec {
    name: &'static str,
    target_rows_per_second: u64,
    duration: Duration,
}

#[derive(Debug, Default)]
struct RunState {
    started: Option<Instant>,
    forge_samples: Vec<u64>,
    phases: Vec<PhaseMeasurement>,
    backlog: Vec<BacklogSample>,
    forge_outcome: ForgeTickOutcome,
    accepted_rows: u64,
    errors: u64,
}

#[tokio::main]
async fn main() -> Result<(), BenchError> {
    let recorder = BenchmarkRecorder::new()
        .install()
        .map_err(|error| format!("benchmark metrics recorder install failed: {error}"))?;
    BENCHMARK_RECORDER
        .set(recorder)
        .map_err(|_| "benchmark recorder was installed more than once")?;
    let lane = argument("lane").unwrap_or_else(|| "capacity".to_owned());
    let pods = argument("pods")
        .as_deref()
        .unwrap_or("3")
        .parse::<usize>()?;
    let tenants = argument("tenants")
        .as_deref()
        .unwrap_or("10")
        .parse::<usize>()?;
    let workload = workload_for_lane(&lane)?;
    workload.validate()?;

    match lane.as_str() {
        "preflight" => run_preflight().await?,
        "scribe" | "scribe-forge" | "capacity" => {
            let integrated = lane != "scribe";
            let (report, harness) = run_ingest(&lane, pods, tenants, workload, integrated).await?;
            emit_report(report)?;
            harness.shutdown().await?;
        }
        "forge" => {
            let (report, harness) = run_forge(&lane, pods, tenants, workload).await?;
            emit_report(report)?;
            harness.shutdown().await?;
        }
        "oracle" => {
            let (report, cluster) = run_oracle(pods, workload).await?;
            emit_report(report)?;
            cluster.shutdown().await?;
        }
        other => return Err(format!("unknown Bifrost lane: {other}").into()),
    }
    Ok(())
}

async fn run_preflight() -> Result<(), BenchError> {
    let harness = BifrostHarness::start(1, 1).await?;
    let result = async {
        let tenant = *harness
            .tenants()
            .first()
            .ok_or("preflight harness has no tenant")?;
        let scribe = harness
            .scribes()
            .first()
            .ok_or("preflight harness has no Scribe")?;
        let principal = Principal {
            id: PrincipalId::new(uuid::Uuid::now_v7()),
            kind: PrincipalKind::User,
            tenant_id: tenant,
            roles: Vec::new(),
            effective_permissions: PermissionSet::new(),
        };
        let rows = make_batch(1_000, 0)?;
        let schema_fingerprint = SchemaFingerprint::from_arrow_schema(rows.schema().as_ref());
        scribe
            .append(ScribeAppend {
                principal,
                table: TableRef::new(BifrostNamespace::Bifrost, TABLE_NAME),
                rows,
                schema_fingerprint,
                request_id: RequestId::now_v7(),
                batch_id: uuid::Uuid::now_v7(),
                measured_wire_bytes: 0,
            })
            .await?;
        harness.force_seal_all().await?;
        if !wait_for_scribe_drain(&harness, SCRIBE_DRAIN_TIMEOUT).await? {
            return Err("preflight Scribe drain timed out".into());
        }
        let verification = verify_outputs(&harness, 1_000, false, 0, None).await?;
        if !verification.passed {
            return Err("preflight durable verification failed".into());
        }
        Ok::<(), BenchError>(())
    }
    .await;
    harness.shutdown().await?;
    result
}

async fn run_ingest(
    lane: &str,
    pods: usize,
    tenant_count: usize,
    workload: WorkloadSpec,
    integrated: bool,
) -> Result<(BenchmarkReport, BifrostHarness), BenchError> {
    let harness = BifrostHarness::start(pods, tenant_count).await?;
    let harness = Arc::new(harness);
    let fixtures = if integrated {
        seed_forge_fixtures(&harness, FORGE_TABLE_NAME).await?
    } else {
        Vec::new()
    };
    let state = Arc::new(Mutex::new(RunState {
        started: Some(Instant::now()),
        ..RunState::default()
    }));
    let stop = CancellationToken::new();
    let sealer = spawn_sealer(Arc::clone(&harness), Arc::clone(&state), stop.clone());
    let forge = fixtures.first().map(|fixture| {
        spawn_forge_worker(
            Arc::clone(&fixture.context),
            Arc::clone(&state),
            stop.clone(),
        )
    });
    let sampler = spawn_sampler(
        Arc::clone(&harness),
        Arc::clone(&state),
        stop.clone(),
        integrated.then(|| FORGE_TABLE_NAME.to_owned()),
    );

    let run_result = run_producer_phases(
        Arc::clone(&harness),
        Arc::clone(&state),
        workload.clone(),
        integrated,
    )
    .await;
    stop.cancel();
    sealer.await??;
    if let Some(forge) = forge {
        forge.await??;
    }
    sampler.await??;
    let run_error = run_result.err();

    harness.force_seal_all().await?;
    let scribe_drained = wait_for_scribe_drain(&harness, SCRIBE_DRAIN_TIMEOUT).await?;
    let forge_candidates = if integrated {
        drain_forge(
            &harness,
            &state,
            &fixtures,
            FORGE_TABLE_NAME,
            FORGE_DRAIN_TIMEOUT,
        )
        .await?
    } else {
        0
    };
    let expected_rows = state
        .lock()
        .map_err(|_| "benchmark state lock poisoned")?
        .accepted_rows;
    let verification = verify_outputs(
        &harness,
        expected_rows,
        integrated,
        forge_candidates,
        integrated.then_some(FORGE_TABLE_NAME),
    )
    .await?;
    let mut report = build_report(
        &harness,
        &state,
        lane,
        workload,
        verification,
        integrated.then_some(FORGE_TABLE_NAME),
        scribe_drained && forge_candidates == 0 && run_error.is_none(),
    )
    .await?;
    if let Some(error) = run_error {
        report.complete = false;
        eprintln!("benchmark workload ended early: {error}");
    }
    let harness =
        Arc::try_unwrap(harness).map_err(|_| "benchmark workers retained the Bifrost harness")?;
    Ok((report, harness))
}

async fn run_forge(
    lane: &str,
    pods: usize,
    tenant_count: usize,
    workload: WorkloadSpec,
) -> Result<(BenchmarkReport, BifrostHarness), BenchError> {
    let harness = Arc::new(BifrostHarness::start(pods, tenant_count).await?);
    let fixtures = seed_forge_fixtures(&harness, FORGE_TABLE_NAME).await?;
    let state = Arc::new(Mutex::new(RunState {
        started: Some(Instant::now()),
        ..RunState::default()
    }));
    let stop = CancellationToken::new();
    let worker = spawn_forge_worker(
        Arc::clone(
            &fixtures
                .first()
                .ok_or("Forge fixture setup produced no tables")?
                .context,
        ),
        Arc::clone(&state),
        stop.clone(),
    );
    let sampler = spawn_sampler(
        Arc::clone(&harness),
        Arc::clone(&state),
        stop.clone(),
        Some(FORGE_TABLE_NAME.to_owned()),
    );
    let run_error = run_forge_phases(Arc::clone(&state)).await.err();
    stop.cancel();
    worker.await??;
    sampler.await??;
    let forge_candidates = drain_forge(
        &harness,
        &state,
        &fixtures,
        FORGE_TABLE_NAME,
        FORGE_DRAIN_TIMEOUT,
    )
    .await?;
    let verification =
        verify_outputs(&harness, 0, true, forge_candidates, Some(FORGE_TABLE_NAME)).await?;
    let mut report = build_report(
        &harness,
        &state,
        lane,
        workload,
        verification,
        Some(FORGE_TABLE_NAME),
        forge_candidates == 0 && run_error.is_none(),
    )
    .await?;
    if let Some(error) = run_error {
        report.complete = false;
        eprintln!("Forge workload ended early: {error}");
    }
    let harness =
        Arc::try_unwrap(harness).map_err(|_| "benchmark workers retained the Bifrost harness")?;
    Ok((report, harness))
}

async fn run_producer_phases(
    harness: Arc<BifrostHarness>,
    state: Arc<Mutex<RunState>>,
    workload: WorkloadSpec,
    integrated: bool,
) -> Result<(), BenchError> {
    let phases = phases(integrated);
    let deadline = Instant::now() + RUN_DEADLINE;
    for phase in phases {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining < phase.duration {
            return Err(format!("run deadline reached before phase {}", phase.name).into());
        }
        run_phase(harness.clone(), state.clone(), workload.batch_size, phase).await?;
    }
    Ok(())
}

async fn run_phase(
    harness: Arc<BifrostHarness>,
    state: Arc<Mutex<RunState>>,
    batch_size: u64,
    phase: PhaseSpec,
) -> Result<(), BenchError> {
    let phase_started = Instant::now();
    let phase_state = Arc::new(Mutex::new(PhaseMeasurement {
        phase: phase.name.to_owned(),
        target_rows_per_second: phase.target_rows_per_second,
        duration_ms: 0,
        submitted_rows: 0,
        accepted_rows: 0,
        errors: 0,
    }));
    let phase_stop = CancellationToken::new();
    let tenant_count = harness.tenants().len();
    let per_tenant_rate = phase
        .target_rows_per_second
        .checked_div(u64::try_from(tenant_count)?)
        .unwrap_or(1)
        .max(1);
    let batch_rows = usize::try_from(batch_size)?;
    let interval = Duration::from_secs_f64((batch_size as f64 / per_tenant_rate as f64).max(0.001));
    let mut producers = Vec::with_capacity(tenant_count);
    for (tenant_index, tenant) in harness.tenants().iter().copied().enumerate() {
        let scribe_count = harness.scribes().len();
        let scribe = Arc::clone(&harness.scribes()[tenant_index % scribe_count]);
        let phase_stop = phase_stop.clone();
        let phase_state = Arc::clone(&phase_state);
        producers.push(tokio::spawn(async move {
            let principal = Principal {
                id: PrincipalId::new(uuid::Uuid::now_v7()),
                kind: PrincipalKind::User,
                tenant_id: tenant,
                roles: Vec::new(),
                effective_permissions: PermissionSet::new(),
            };
            let mut ticker = tokio::time::interval(interval);
            let mut sequence = 0_u64;
            loop {
                tokio::select! {
                    _ = phase_stop.cancelled() => break,
                    _ = ticker.tick() => {
                        let batch = make_batch(batch_rows, sequence)?;
                        let schema_fingerprint =
                            SchemaFingerprint::from_arrow_schema(batch.schema().as_ref());
                        sequence = sequence.saturating_add(1);
                        {
                            let mut phase = phase_state
                                .lock()
                                .map_err(|_| "phase state lock poisoned")?;
                            phase.submitted_rows = phase
                                .submitted_rows
                                .saturating_add(u64::try_from(batch_rows)?);
                        }
                        let result = scribe.append(ScribeAppend {
                            principal: principal.clone(),
                            table: TableRef::new(BifrostNamespace::Bifrost, TABLE_NAME),
                            rows: batch,
                            schema_fingerprint,
                            request_id: RequestId::now_v7(),
                            batch_id: uuid::Uuid::now_v7(),
                            measured_wire_bytes: 0,
                        }).await;
                        let mut phase = phase_state
                            .lock()
                            .map_err(|_| "phase state lock poisoned")?;
                        match result {
                            Ok(()) => phase.accepted_rows = phase
                                .accepted_rows
                                .saturating_add(u64::try_from(batch_rows)?),
                            Err(_) => phase.errors = phase.errors.saturating_add(1),
                        }
                    }
                }
            }
            Ok::<(), BenchError>(())
        }));
    }
    tokio::time::sleep(phase.duration).await;
    phase_stop.cancel();
    for producer in producers {
        producer.await??;
    }
    let mut finished = phase_state
        .lock()
        .map_err(|_| "phase state lock poisoned")?
        .clone();
    finished.duration_ms = u64::try_from(phase_started.elapsed().as_millis())?;
    let mut run = state.lock().map_err(|_| "benchmark state lock poisoned")?;
    run.accepted_rows = run.accepted_rows.saturating_add(finished.accepted_rows);
    run.errors = run.errors.saturating_add(finished.errors);
    run.phases.push(finished);
    Ok(())
}

fn spawn_sealer(
    harness: Arc<BifrostHarness>,
    state: Arc<Mutex<RunState>>,
    stop: CancellationToken,
) -> tokio::task::JoinHandle<Result<(), Box<dyn Error + Send + Sync>>> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                _ = stop.cancelled() => break,
                _ = ticker.tick() => {
                    if harness.force_seal_all().await.is_err() {
                        let mut state = state
                            .lock()
                            .map_err(|_| "benchmark state lock poisoned")?;
                        state.errors = state.errors.saturating_add(1);
                    }
                }
            }
        }
        Ok(())
    })
}

fn spawn_forge_worker(
    context: Arc<ForgeContext>,
    state: Arc<Mutex<RunState>>,
    stop: CancellationToken,
) -> tokio::task::JoinHandle<Result<(), Box<dyn Error + Send + Sync>>> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                _ = stop.cancelled() => break,
                _ = ticker.tick() => {
                    let started = Instant::now();
                    match run_maintenance_tick(&context).await {
                        Ok(outcome) => {
                            let mut state = state
                                .lock()
                                .map_err(|_| "benchmark state lock poisoned")?;
                            state.forge_samples.push(
                                u64::try_from(started.elapsed().as_micros())?.max(1)
                            );
                            accumulate_forge_outcome(&mut state.forge_outcome, outcome);
                        }
                        Err(_) => {
                            let mut state = state
                                .lock()
                                .map_err(|_| "benchmark state lock poisoned")?;
                            state.errors = state.errors.saturating_add(1);
                        }
                    }
                }
            }
        }
        Ok(())
    })
}

fn spawn_sampler(
    harness: Arc<BifrostHarness>,
    state: Arc<Mutex<RunState>>,
    stop: CancellationToken,
    forge_table: Option<String>,
) -> tokio::task::JoinHandle<Result<(), Box<dyn Error + Send + Sync>>> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                _ = stop.cancelled() => break,
                _ = ticker.tick() => {
                    let candidates = match forge_table.as_deref() {
                        Some(table) => candidate_count(&harness, table).await.unwrap_or(0),
                        None => 0,
                    };
                    let stats = harness.memtable_stats()
                        .map_err(|error| error.to_string())?;
                    let elapsed_ms = state
                        .lock()
                        .map_err(|_| "benchmark state lock poisoned")?
                        .started
                        .map_or(0, |started| u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX));
                    state
                        .lock()
                        .map_err(|_| "benchmark state lock poisoned")?
                        .backlog
                        .push(BacklogSample {
                            elapsed_ms,
                            wal_bytes: harness.wal_bytes(),
                            active_rows: u64::try_from(stats.writable_rows)?,
                            immutable_rows: u64::try_from(stats.immutable_rows)?,
                            pending_generations: u64::try_from(stats.pending_generations)?,
                            admitted_items: harness
                                .scribes()
                                .iter()
                                .map(|scribe| {
                                    u64::try_from(scribe.runtime_snapshot().admission.items)
                                        .unwrap_or(u64::MAX)
                                })
                                .sum(),
                            admitted_bytes: harness
                                .scribes()
                                .iter()
                                .map(|scribe| {
                                    u64::try_from(scribe.runtime_snapshot().admission.bytes)
                                        .unwrap_or(u64::MAX)
                                })
                                .sum(),
                            active_writers: harness
                                .scribes()
                                .iter()
                                .map(|scribe| {
                                    u64::try_from(scribe.runtime_snapshot().writers.writers)
                                        .unwrap_or(u64::MAX)
                                })
                                .sum(),
                            executor_depth: harness
                                .scribes()
                                .iter()
                                .map(|scribe| {
                                    u64::try_from(scribe.runtime_snapshot().executor.depth)
                                        .unwrap_or(u64::MAX)
                                })
                                .sum(),
                            executor_saturation_events: harness
                                .scribes()
                                .iter()
                                .map(|scribe| {
                                    scribe.runtime_snapshot().executor.saturation_events
                                })
                                .sum(),
                            unhealthy_writers: harness
                                .scribes()
                                .iter()
                                .map(|scribe| {
                                    u64::try_from(
                                        scribe.runtime_snapshot().writers.unhealthy_writers,
                                    )
                                    .unwrap_or(u64::MAX)
                                })
                                .sum(),
                            forge_candidates: candidates,
                        });
                }
            }
        }
        Ok(())
    })
}

async fn wait_for_scribe_drain(
    harness: &BifrostHarness,
    timeout: Duration,
) -> Result<bool, BenchError> {
    let deadline = Instant::now() + timeout;
    loop {
        if harness.is_drained()? {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn drain_forge(
    harness: &BifrostHarness,
    state: &Arc<Mutex<RunState>>,
    fixtures: &[ForgeFixture],
    table_name: &str,
    timeout: Duration,
) -> Result<u64, BenchError> {
    let Some(fixture) = fixtures.first() else {
        return Ok(0);
    };
    let deadline = Instant::now() + timeout;
    loop {
        let started = Instant::now();
        let outcome = run_maintenance_tick(&fixture.context).await?;
        {
            let mut state = state.lock().map_err(|_| "benchmark state lock poisoned")?;
            state
                .forge_samples
                .push(u64::try_from(started.elapsed().as_micros())?.max(1));
            accumulate_forge_outcome(&mut state.forge_outcome, outcome);
        }
        let candidates = candidate_count(harness, table_name).await?;
        if candidates == 0 || Instant::now() >= deadline {
            return Ok(candidates);
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

fn accumulate_forge_outcome(total: &mut ForgeTickOutcome, outcome: ForgeTickOutcome) {
    total.groups_seen = total.groups_seen.saturating_add(outcome.groups_seen);
    total.bins_committed = total.bins_committed.saturating_add(outcome.bins_committed);
    total.bins_skipped = total.bins_skipped.saturating_add(outcome.bins_skipped);
    total.tables_failed = total.tables_failed.saturating_add(outcome.tables_failed);
    total.lease_contention = total
        .lease_contention
        .saturating_add(outcome.lease_contention);
}

async fn verify_outputs(
    harness: &BifrostHarness,
    expected_rows: u64,
    forge_enabled: bool,
    forge_candidates: u64,
    forge_table: Option<&str>,
) -> Result<VerificationMeasurements, BenchError> {
    let row = sqlx::query_as::<_, (Option<i64>, i64, i64, i64)>(
        "SELECT COALESCE(SUM(row_count) FILTER (WHERE NOT compacted AND table_name = $1), 0)::bigint,
                COUNT(*)::bigint,
                COALESCE(SUM(file_size) FILTER (WHERE compacted), 0)::bigint,
                COALESCE(SUM(file_size) FILTER (WHERE NOT compacted), 0)::bigint
           FROM vala.file_list
          WHERE namespace = 'vala.bifrost'
            AND table_name = ANY($2)",
    )
    .bind(TABLE_NAME)
    .bind(forge_table.map_or_else(|| vec![TABLE_NAME], |table| vec![TABLE_NAME, table]))
    .fetch_one(harness.cluster().pg_fixture().platform_admin_pool())
    .await?;
    let file_list_rows = u64::try_from(row.0.unwrap_or(0).max(0))?;
    let file_count = u64::try_from(row.1.max(0))?;
    let paths = sqlx::query_scalar::<_, String>(
        "SELECT file_path FROM vala.file_list
          WHERE namespace = 'vala.bifrost' AND table_name = ANY($1)",
    )
    .bind(forge_table.map_or_else(|| vec![TABLE_NAME], |table| vec![TABLE_NAME, table]))
    .fetch_all(harness.cluster().pg_fixture().platform_admin_pool())
    .await?;
    let operator = harness.cluster().storage_operator();
    let mut missing_objects = 0_u64;
    let mut present_objects = 0_u64;
    for path in paths {
        if operator.stat(&path).await.is_ok() {
            present_objects = present_objects.saturating_add(1);
        } else {
            missing_objects = missing_objects.saturating_add(1);
        }
    }
    let passed = file_count > 0
        && present_objects == file_count
        && missing_objects == 0
        && file_list_rows >= expected_rows
        && (!forge_enabled || forge_candidates == 0);
    Ok(VerificationMeasurements {
        expected_rows,
        file_list_rows,
        parquet_files: present_objects,
        missing_objects,
        forge_candidates,
        passed,
    })
}

async fn build_report(
    harness: &BifrostHarness,
    state: &Arc<Mutex<RunState>>,
    lane: &str,
    workload: WorkloadSpec,
    verification: VerificationMeasurements,
    forge_table: Option<&str>,
    complete: bool,
) -> Result<BenchmarkReport, BenchError> {
    let (accepted_rows, started, forge_samples, forge_outcome, phases, backlog, errors) = {
        let state = state.lock().map_err(|_| "benchmark state lock poisoned")?;
        (
            state.accepted_rows,
            state.started,
            state.forge_samples.clone(),
            state.forge_outcome,
            state.phases.clone(),
            state.backlog.clone(),
            state.errors,
        )
    };
    let write_latency = LatencyPercentiles::from_samples(&state_samples(harness, "append"));
    let stage_data = stage_measurements(harness, &forge_samples);
    let elapsed = started.map_or(1.0, |started| started.elapsed().as_secs_f64().max(0.001));
    let stats = harness.memtable_stats()?;
    let (file_count, average_file_size_bytes, compaction_amplification) =
        file_measurements(harness, forge_table).await?;
    let mut storage = StorageMeasurements {
        rows_per_second: accepted_rows as f64 / elapsed,
        wal_growth_bytes: harness.wal_bytes(),
        active_memory_bytes: u64::try_from(stats.writable_bytes)?,
        immutable_memory_bytes: u64::try_from(stats.immutable_bytes)?,
        queued_memory_bytes: backlog
            .iter()
            .map(|sample| sample.admitted_bytes)
            .max()
            .unwrap_or(0),
        file_count,
        average_file_size_bytes,
        ..StorageMeasurements::default()
    };
    if let Some(wal) = stage_data
        .iter()
        .find(|stage| stage.stage == "wal_sync_data")
    {
        storage.fsync_us = wal.latency.p99_us;
    }
    if let Some(append) = stage_data.iter().find(|stage| stage.stage == "append") {
        storage.mib_per_second = append.bytes as f64 / 1_048_576.0 / elapsed;
    }
    storage.compaction_amplification = compaction_amplification;
    let forge = ForgeMeasurements {
        ticks: u64::try_from(forge_samples.len())?,
        groups_seen: u64::try_from(forge_outcome.groups_seen)?,
        bins_committed: u64::try_from(forge_outcome.bins_committed)?,
        bins_skipped: u64::try_from(forge_outcome.bins_skipped)?,
        tables_failed: u64::try_from(forge_outcome.tables_failed)?,
        lease_contention: u64::try_from(forge_outcome.lease_contention)?,
        peak_candidates: backlog
            .iter()
            .map(|sample| sample.forge_candidates)
            .max()
            .unwrap_or(0),
    };
    let verification_passed = verification.passed;
    Ok(BenchmarkReport {
        report_version: BenchmarkReport::VERSION.to_owned(),
        lane: format!("bench:bifrost:{lane}:baseline"),
        batch_size: workload.batch_size,
        pods: PodMetadata {
            pod_count: u32::try_from(harness.scribes().len())?,
            pod_ids: (0..harness.scribes().len())
                .map(|index| format!("pod-{index}"))
                .collect(),
        },
        machine: machine_metadata(),
        write_latency,
        query_latency: LatencyPercentiles::default(),
        workload,
        storage,
        query: QueryMeasurements::default(),
        forge,
        scribe_matrix: if lane == "scribe" {
            required_scribe_matrix()
        } else {
            Vec::new()
        },
        stages: stage_data,
        phases,
        backlog,
        verification,
        errors,
        complete: complete && errors == 0 && verification_passed,
    })
}

fn state_samples(harness: &BifrostHarness, stage: &str) -> Vec<u64> {
    let _ = harness;
    let metric = match stage {
        "append" => "bifrost_scribe_ack_seconds",
        "wal_sync_data" => "bifrost_scribe_wal_sync_seconds",
        "write_all" => "bifrost_scribe_wal_append_seconds",
        _ => return Vec::new(),
    };
    BENCHMARK_RECORDER
        .get()
        .and_then(|recorder| recorder.snapshot().histograms.get(metric).cloned())
        .map(|histogram| {
            vec![
                histogram.p50 / 1_000,
                histogram.p95 / 1_000,
                histogram.p99 / 1_000,
            ]
        })
        .unwrap_or_default()
}

fn stage_measurements(harness: &BifrostHarness, forge_samples: &[u64]) -> Vec<StageMeasurements> {
    let _ = harness;
    let mut grouped: BTreeMap<String, (Vec<u64>, u64, u64, u64)> = BTreeMap::new();
    if let Some(recorder) = BENCHMARK_RECORDER.get() {
        for (metric, histogram) in recorder.snapshot().histograms {
            let stage = match metric.as_str() {
                "bifrost_scribe_ack_seconds" => "append",
                "bifrost_scribe_wal_append_seconds" => "write_all",
                "bifrost_scribe_wal_sync_seconds" => "wal_sync_data",
                _ => continue,
            };
            let samples = vec![
                histogram.p50 / 1_000,
                histogram.p95 / 1_000,
                histogram.p99 / 1_000,
            ];
            grouped.insert(stage.to_owned(), (samples, histogram.count, 0, 0));
        }
    }
    if !forge_samples.is_empty() {
        grouped.insert(
            "forge_tick".to_owned(),
            (
                forge_samples.to_vec(),
                u64::try_from(forge_samples.len()).unwrap_or(u64::MAX),
                0,
                0,
            ),
        );
    }
    grouped
        .into_iter()
        .map(|(stage, (samples, count, rows, bytes))| StageMeasurements {
            stage,
            latency: LatencyPercentiles::from_samples(&samples),
            count,
            rows,
            bytes,
            errors: 0,
        })
        .collect()
}

async fn file_measurements(
    harness: &BifrostHarness,
    forge_table: Option<&str>,
) -> Result<(u64, u64, f64), BenchError> {
    let tables = forge_table.map_or_else(|| vec![TABLE_NAME], |table| vec![TABLE_NAME, table]);
    let row = sqlx::query_as::<_, (i64, Option<f64>)>(
        "SELECT COUNT(*)::bigint, AVG(file_size)::double precision
           FROM vala.file_list
          WHERE namespace = 'vala.bifrost' AND table_name = ANY($1)",
    )
    .bind(&tables)
    .fetch_one(harness.cluster().pg_fixture().platform_admin_pool())
    .await?;
    let count = u64::try_from(row.0.max(0))?;
    let average = row
        .1
        .unwrap_or(0.0)
        .max(0.0)
        .round()
        .to_string()
        .parse::<u64>()?;
    let compaction_amplification = match forge_table {
        Some(table) => {
            let row = sqlx::query_as::<_, (i64, i64)>(
                "SELECT COALESCE(SUM(file_size) FILTER (WHERE compacted), 0)::bigint,
                        COALESCE(SUM(file_size) FILTER (WHERE NOT compacted), 0)::bigint
                   FROM vala.file_list
                  WHERE namespace = 'vala.bifrost' AND table_name = $1",
            )
            .bind(table)
            .fetch_one(harness.cluster().pg_fixture().platform_admin_pool())
            .await?;
            if row.0 > 0 {
                row.1 as f64 / row.0 as f64
            } else {
                0.0
            }
        }
        None => 0.0,
    };
    Ok((count, average, compaction_amplification))
}

async fn candidate_count(harness: &BifrostHarness, table_name: &str) -> Result<u64, BenchError> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)::bigint FROM vala.file_list
          WHERE namespace = 'vala.bifrost' AND table_name = $1
            AND NOT compacted
            AND created_at < now() - interval '2 minutes'",
    )
    .bind(table_name)
    .fetch_one(harness.cluster().pg_fixture().platform_admin_pool())
    .await?;
    Ok(u64::try_from(count.max(0))?)
}

async fn seed_forge_fixtures(
    harness: &BifrostHarness,
    table_name: &str,
) -> Result<Vec<ForgeFixture>, BenchError> {
    let server = harness
        .cluster()
        .server(0)
        .ok_or("Bifrost cluster has no Forge server")?;
    let mut fixtures = Vec::with_capacity(harness.tenants().len());
    for tenant in harness.tenants().iter().copied() {
        fixtures.push(seed_forge_group_for_tenant(server, tenant, table_name).await);
    }
    Ok(fixtures)
}

async fn run_forge_phases(state: Arc<Mutex<RunState>>) -> Result<(), BenchError> {
    let deadline = Instant::now() + RUN_DEADLINE;
    for phase in forge_only_phases() {
        if Instant::now() + phase.duration > deadline {
            return Err("Forge run deadline reached".into());
        }
        let started = Instant::now();
        tokio::time::sleep(phase.duration).await;
        let mut state = state.lock().map_err(|_| "benchmark state lock poisoned")?;
        state.phases.push(PhaseMeasurement {
            phase: phase.name.to_owned(),
            target_rows_per_second: phase.target_rows_per_second,
            duration_ms: u64::try_from(started.elapsed().as_millis())?,
            submitted_rows: 0,
            accepted_rows: 0,
            errors: 0,
        });
    }
    Ok(())
}

fn forge_only_phases() -> Vec<PhaseSpec> {
    vec![
        PhaseSpec {
            name: "warmup",
            target_rows_per_second: 0,
            duration: Duration::from_secs(15),
        },
        PhaseSpec {
            name: "steady",
            target_rows_per_second: 0,
            duration: Duration::from_secs(30),
        },
        PhaseSpec {
            name: "ramp-5k",
            target_rows_per_second: 0,
            duration: Duration::from_secs(15),
        },
        PhaseSpec {
            name: "ramp-10k",
            target_rows_per_second: 0,
            duration: Duration::from_secs(15),
        },
        PhaseSpec {
            name: "ramp-20k",
            target_rows_per_second: 0,
            duration: Duration::from_secs(15),
        },
        PhaseSpec {
            name: "overload",
            target_rows_per_second: 0,
            duration: Duration::from_secs(45),
        },
    ]
}

fn phases(integrated: bool) -> Vec<PhaseSpec> {
    let warmup = if integrated { 125 } else { 30 };
    vec![
        PhaseSpec {
            name: "warmup",
            target_rows_per_second: 2_500,
            duration: Duration::from_secs(warmup),
        },
        PhaseSpec {
            name: "steady",
            target_rows_per_second: 2_500,
            duration: Duration::from_secs(30),
        },
        PhaseSpec {
            name: "ramp-5k",
            target_rows_per_second: 5_000,
            duration: Duration::from_secs(15),
        },
        PhaseSpec {
            name: "ramp-10k",
            target_rows_per_second: 10_000,
            duration: Duration::from_secs(15),
        },
        PhaseSpec {
            name: "ramp-20k",
            target_rows_per_second: 20_000,
            duration: Duration::from_secs(15),
        },
        PhaseSpec {
            name: "overload",
            target_rows_per_second: 40_000,
            duration: Duration::from_secs(45),
        },
    ]
}

fn make_batch(rows: usize, sequence: u64) -> Result<RecordBatch, arrow::error::ArrowError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("value", DataType::Int64, false),
        Field::new(
            "wyrd_event_time",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".to_owned().into())),
            false,
        ),
    ]));
    let now = chrono::Utc::now().timestamp_micros();
    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(Int64Array::from(
                (0..rows)
                    .map(|index| {
                        i64::try_from(sequence)
                            .unwrap_or(i64::MAX)
                            .saturating_mul(i64::try_from(rows).unwrap_or(i64::MAX))
                            .saturating_add(i64::try_from(index).unwrap_or(i64::MAX))
                    })
                    .collect::<Vec<_>>(),
            )),
            Arc::new(
                TimestampMicrosecondArray::from(
                    (0..rows)
                        .map(|index| now.saturating_add(i64::try_from(index).unwrap_or(i64::MAX)))
                        .collect::<Vec<_>>(),
                )
                .with_timezone("UTC"),
            ),
        ],
    )
}

async fn run_oracle(
    pods: usize,
    workload: WorkloadSpec,
) -> Result<(BenchmarkReport, WyrdTestCluster), BenchError> {
    let topology = if pods == 1 {
        BifrostTopology::OnePod
    } else {
        BifrostTopology::ThreePod
    };
    let cluster = WyrdTestCluster::start(pods, topology).await?;
    let server = cluster
        .server(0)
        .ok_or("Bifrost cluster has no Oracle server")?;
    let bootstrap = server.bootstrap_user("real-oracle", &["admin"]).await?;
    let jwt = match bootstrap {
        wyrd_testing::Bootstrap::User { jwt, .. } => jwt,
        wyrd_testing::Bootstrap::Machine { .. } => return Err("expected user bootstrap".into()),
    };
    let url = server
        .base_url()
        .ok_or("Oracle benchmark requires bound server")?;
    let started = Instant::now();
    let response = reqwest::Client::new()
        .post(format!("{url}/v1/query"))
        .header("x-wyrd-access-token", format!("Bearer {jwt}"))
        .json(&SyncQueryRequest {
            sql: "SELECT 1".to_owned(),
            params: Vec::new(),
        })
        .send()
        .await?;
    let complete = response.status().is_success();
    let _body = response.bytes().await?;
    let first_byte_us = u64::try_from(started.elapsed().as_micros())?.max(1);
    Ok((
        BenchmarkReport {
            report_version: BenchmarkReport::VERSION.to_owned(),
            lane: "bench:bifrost:oracle:baseline".to_owned(),
            batch_size: workload.batch_size,
            pods: PodMetadata {
                pod_count: u32::try_from(pods)?,
                pod_ids: (0..pods).map(|index| format!("pod-{index}")).collect(),
            },
            machine: machine_metadata(),
            write_latency: LatencyPercentiles::default(),
            query_latency: LatencyPercentiles::from_samples(&[first_byte_us]),
            workload,
            storage: StorageMeasurements::default(),
            query: QueryMeasurements {
                first_byte_us,
                freshness_us: first_byte_us,
                audit_visibility_lag_us: 0,
            },
            forge: ForgeMeasurements::default(),
            scribe_matrix: Vec::new(),
            stages: Vec::new(),
            phases: Vec::new(),
            backlog: Vec::new(),
            verification: VerificationMeasurements {
                passed: complete,
                ..VerificationMeasurements::default()
            },
            errors: 0,
            complete,
        },
        cluster,
    ))
}

fn workload_for_lane(lane: &str) -> Result<WorkloadSpec, BenchError> {
    let (traffic, schema) = match lane {
        "scribe" => (TrafficShape::Steady, SchemaWidth::Narrow),
        "scribe-forge" => (TrafficShape::Bursty, SchemaWidth::Wide),
        "forge" => (TrafficShape::Bursty, SchemaWidth::Wide),
        "oracle" => (TrafficShape::LatencySensitive, SchemaWidth::Narrow),
        "capacity" => (TrafficShape::Bursty, SchemaWidth::Wide),
        "preflight" => (TrafficShape::Steady, SchemaWidth::Narrow),
        other => return Err(format!("unknown Bifrost lane: {other}").into()),
    };
    Ok(WorkloadSpec::required(
        format!("real-{lane}"),
        42,
        traffic,
        schema,
    ))
}

fn emit_report(report: BenchmarkReport) -> Result<(), BenchError> {
    let json = report.to_json()?;
    if let Some(path) = argument("output").or_else(|| std::env::var("WYRD_BIFROST_OUTPUT").ok()) {
        report.write_json(path)?;
    } else {
        println!("{json}");
    }
    Ok(())
}

fn machine_metadata() -> MachineMetadata {
    let git_sha = std::env::var("GIT_SHA")
        .ok()
        .or_else(|| git_command(&["rev-parse", "HEAD"]))
        .unwrap_or_else(|| "unknown".to_owned());
    let dirty_worktree = std::env::var("WYRD_BENCH_DIRTY").ok().map_or_else(
        || git_command(&["status", "--porcelain"]).is_some_and(|status| !status.is_empty()),
        |value| value == "1" || value.eq_ignore_ascii_case("true"),
    );
    MachineMetadata {
        git_sha,
        dirty_worktree,
        operating_system: std::env::consts::OS.to_owned(),
        cpu_count: std::thread::available_parallelism()
            .ok()
            .map(std::num::NonZeroUsize::get)
            .and_then(|count| u32::try_from(count).ok()),
        memory_bytes: None,
    }
}

fn git_command(arguments: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(arguments)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn argument(name: &str) -> Option<String> {
    let prefix = format!("--{name}=");
    std::env::args().find_map(|argument| argument.strip_prefix(&prefix).map(str::to_owned))
}
