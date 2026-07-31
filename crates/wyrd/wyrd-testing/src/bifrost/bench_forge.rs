//! Real Forge benchmark execution over the shared Bifrost harness.

use std::time::Instant;

use crate::bifrost::{BifrostHarness, seed_forge_group_for_tenant};
use iceberg::transaction::{ApplyTransactionAction, Transaction};
use opendal::Buffer;
use serde::Serialize;
use sqlx::Row;
use vala_bifrost_redux::forge::{Forge, deterministic_output_path_for_test};
use wyrd_bench::{BifrostLane, BifrostScenario, NegativeFlowReport, SloGate};

type BenchError = Box<dyn std::error::Error + Send + Sync>;

/// Constrained DataFusion parent ceiling used by the Forge benchmark.
const SHARED_PARENT_MEMORY_CEILING: u64 = 16 * 1024 * 1024;

/// Evidence emitted by the Forge benchmark's real rewrite and commit path.
#[derive(Debug, Serialize)]
struct ForgeBenchmarkReport {
    /// Shared benchmark envelope and readiness classification.
    envelope: wyrd_bench::BifrostReportEnvelope,
    /// Wall-clock duration of setup, rewrite, and verification.
    elapsed_us: u64,
    /// Whether every declared Forge readiness gate passed.
    verified: bool,
    /// Input rows accepted by the rewrite.
    rows: u64,
    /// Rows accepted from staged inputs before the rewrite.
    input_rows: u64,
    /// Rows expected from the durable staging fixture.
    expected_rows: u64,
    /// Bytes represented by staged input files.
    input_bytes: u64,
    /// Bytes represented by committed Forge output objects.
    output_bytes: u64,
    /// Number of staged input files selected for the operation.
    input_files: u64,
    /// Number of rotated output files committed by the operation.
    output_files: u64,
    /// Rows encoded into committed outputs.
    output_rows: u64,
    /// Peak DataFusion spill usage observed by Forge.
    spill_bytes: u64,
    /// Peak bytes charged to the shared Bifrost parent during the run.
    peak_parent_memory: u64,
    /// Whether the input exceeded the constrained rewrite memory budget.
    below_demand_shared_memory: bool,
    /// Whether one operation produced multiple committed outputs.
    outputs_per_operation: u64,
    /// Whether prepared and terminal audit bookkeeping converged.
    bookkeeping_converged: bool,
    /// Whether durable staged rows no longer remain uncompacted.
    convergence: bool,
    /// Whether input and output row counts match exactly.
    exact_row_conservation: bool,
    /// Five bounded-GC sample durations, in microseconds.
    gc_samples_us: Vec<u64>,
    /// Nearest-rank p99 across the five bounded-GC samples.
    gc_p99_us: u64,
    /// Exact orphan candidates enumerated across all five samples.
    gc_candidates: u64,
    /// Exact orphan count deleted across the bounded samples.
    gc_deleted: u64,
    /// Whether a retained Iceberg output survived every fresh protection reload.
    gc_retained_paths_protected: bool,
}

/// Exact bounded-GC evidence collected before rewrite outputs can become orphans.
struct GcBenchmarkEvidence {
    /// Five bounded-GC sample durations, in microseconds.
    samples_us: Vec<u64>,
    /// Nearest-rank p99 across the five samples.
    p99_us: u64,
    /// Exact candidates enumerated across all samples.
    candidates: u64,
    /// Exact candidates deleted across all samples.
    deleted: u64,
}

/// Runs the five isolated GC samples before the main rewrite creates stale outputs.
///
/// Each sample fixture's seeded inputs remain protected by nonterminal
/// `file_list` rows. Therefore the only eligible objects across the shared
/// scheduler roster are the 32 explicit deterministic orphan paths.
///
/// # Errors
///
/// Returns fixture, clock, object-store, Forge, conversion, or SLO failures.
async fn run_gc_samples(
    server: &crate::WyrdTestServer,
    tenant: wyrd_spec::DataTenantId,
) -> Result<GcBenchmarkEvidence, BenchError> {
    let mut samples_us = Vec::with_capacity(5);
    let mut candidates = 0_u64;
    let mut deleted = 0_u64;
    for sample in 0..5 {
        let fixture =
            seed_forge_group_for_tenant(server, tenant, &format!("gc_sample_{sample}")).await;
        let mut conn = server.state().postgres.vala().tenant_conn(tenant).await?;
        let seeded_paths: Vec<String> = sqlx::query_scalar(
            "SELECT file_path FROM vala.file_list \
             WHERE data_tenant_id = wyrd.current_tenant() \
               AND namespace = $1 AND table_name = $2",
        )
        .bind(&fixture.binding.logical_namespace)
        .bind(&fixture.binding.table_name)
        .fetch_all(&mut **conn.transaction())
        .await?;
        conn.commit().await?;
        for path in seeded_paths {
            let fixture_path = format!("{path}.fixture");
            fixture.staging.rename(&path, &fixture_path).await?;
            sqlx::query(
                "UPDATE vala.file_list SET file_path = $1 \
                 WHERE data_tenant_id = $2 AND namespace = $3 \
                   AND table_name = $4 AND file_path = $5",
            )
            .bind(&fixture_path)
            .bind(tenant.as_uuid())
            .bind(&fixture.binding.logical_namespace)
            .bind(&fixture.binding.table_name)
            .bind(&path)
            .execute(fixture.operator_pool.pool())
            .await?;
        }
        let retained_control = fixture
            .object_store
            .list(&fixture.binding.object_prefix)
            .await?
            .into_iter()
            .find(|entry| entry.path().contains("/metadata/") && entry.metadata().is_file())
            .map(|entry| entry.path().to_owned())
            .ok_or("GC sample needs one retained Iceberg metadata object")?;
        let mut newest_modified_ms = i64::MIN;
        let mut orphan_paths = Vec::with_capacity(32);
        for candidate in 0..32 {
            let operation_id = uuid::Uuid::from_u128(
                u128::try_from(sample * 32 + candidate + 1)
                    .map_err(|_| "benchmark GC operation identity overflow")?,
            );
            let orphan = deterministic_output_path_for_test(
                &fixture.binding.object_prefix,
                operation_id,
                candidate,
            );
            if !Forge::known_iceberg_object_for_test(&orphan) {
                return Err(
                    format!("GC benchmark candidate is not an Iceberg object: {orphan}").into(),
                );
            }
            fixture
                .staging
                .write(&orphan, Buffer::from(vec![1_u8]))
                .await?;
            newest_modified_ms = newest_modified_ms.max(
                fixture
                    .staging
                    .stat(&orphan)
                    .await?
                    .last_modified()
                    .ok_or("benchmark orphan lacks age evidence")?
                    .into_inner()
                    .as_millisecond(),
            );
            orphan_paths.push(orphan);
        }
        let clock = server.forge_clock();
        let age_target = newest_modified_ms.checked_add(2).ok_or("GC age overflow")?;
        if age_target > clock.now()?.timestamp_millis() {
            clock.set(
                chrono::DateTime::from_timestamp_millis(age_target)
                    .ok_or("benchmark object timestamp is outside UTC")?,
            )?;
        }
        let mut config = fixture.config.clone();
        config.min_files = i64::MAX;
        config.orphan_gc_ttl = std::time::Duration::from_millis(1);
        config.max_gc_candidates_per_batch = 32;
        let forge = fixture.context_with_config(config);
        let sample_started = Instant::now();
        let gc = forge.run_once().await?;
        samples_us.push(u64::try_from(sample_started.elapsed().as_micros())?);
        if gc.gc_candidates != 32 || gc.gc_deleted != 32 {
            return Err(format!(
                "GC sample {sample} expected 32 candidates/deletes, got {}/{}; tick={gc:?}",
                gc.gc_candidates, gc.gc_deleted,
            )
            .into());
        }
        for orphan in orphan_paths {
            if fixture.staging.stat(&orphan).await.is_ok() {
                return Err(format!("GC sample retained orphan {orphan}").into());
            }
        }
        if fixture.staging.stat(&retained_control).await.is_err() {
            return Err(format!("GC sample deleted retained object {retained_control}").into());
        }
        candidates = candidates.saturating_add(u64::try_from(gc.gc_candidates)?);
        deleted = deleted.saturating_add(u64::try_from(gc.gc_deleted)?);
    }
    let mut ordered = samples_us.clone();
    ordered.sort_unstable();
    let p99_us = *ordered
        .get(4)
        .ok_or("GC benchmark needs exactly five samples")?;
    Ok(GcBenchmarkEvidence {
        samples_us,
        p99_us,
        candidates,
        deleted,
    })
}

/// Return the repository report location used by the canonical benchmark lanes.
fn report_path() -> std::path::PathBuf {
    std::env::var_os("WYRD_BIFROST_REPORT").map_or_else(
        || {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../..")
                .join("target/bifrost-benchmarks/task16/forge.json")
        },
        std::path::PathBuf::from,
    )
}

/// Sum the durable staged input bytes and rows for the fixture's physical table.
async fn staged_totals(
    fixture: &crate::bifrost::ForgeFixture,
) -> Result<(u64, u64, u64), BenchError> {
    let row = sqlx::query(
        "SELECT coalesce(sum(file_size), 0)::bigint AS bytes, coalesce(sum(row_count), 0)::bigint AS rows, count(*)::bigint AS files FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3 AND compacted = false",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await?;
    let bytes: i64 = row.try_get("bytes")?;
    let rows: i64 = row.try_get("rows")?;
    let files: i64 = row.try_get("files")?;
    Ok((
        u64::try_from(bytes)?,
        u64::try_from(rows)?,
        u64::try_from(files)?,
    ))
}

/// Sum committed Forge output object sizes under one table prefix.
async fn output_totals(fixture: &crate::bifrost::ForgeFixture) -> Result<(u64, u64), BenchError> {
    let entries = fixture
        .object_store
        .list(&fixture.binding.object_prefix)
        .await?;
    let mut bytes = 0_u64;
    let mut files = 0_u64;
    for entry in entries {
        let path = entry.path();
        if path.contains("/forge/") && entry.metadata().is_file() {
            bytes = bytes.saturating_add(entry.metadata().content_length());
            files = files.saturating_add(1);
        }
    }
    Ok((bytes, files))
}

/// Count durable staged rows that still await compaction.
async fn pending_rows(fixture: &crate::bifrost::ForgeFixture) -> Result<u64, BenchError> {
    let rows: i64 = sqlx::query_scalar(
        "SELECT coalesce(sum(row_count), 0)::bigint FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3 AND compacted = false",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await?;
    Ok(u64::try_from(rows)?)
}

/// Run one typed Forge maintenance scenario.
///
/// # Errors
///
/// Returns an error when fixture setup, Forge execution, report serialization,
/// or any readiness gate fails.
pub async fn run(scenario: BifrostScenario) -> Result<(), BenchError> {
    if scenario.lane != BifrostLane::Forge {
        return Err("Forge adapter received a non-Forge scenario".into());
    }
    let harness = BifrostHarness::start(
        usize::try_from(scenario.pods)?,
        usize::try_from(scenario.tenants)?,
    )
    .await?;
    let started = Instant::now();
    let tenant = *harness.tenants().first().ok_or("Forge needs one tenant")?;
    let server = harness
        .cluster()
        .servers()
        .first()
        .ok_or("Forge needs one server")?;
    let gc_evidence = run_gc_samples(server, tenant).await?;
    let fixture = seed_forge_group_for_tenant(server, tenant, "bifrost_bench_forge").await;
    sqlx::query(
        "UPDATE vala.file_list SET compacted = true WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .execute(fixture.operator_pool.pool())
    .await?;
    let table = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await?;
    let action = Transaction::new(&table).update_table_properties().set(
        "write.target-file-size-bytes".to_owned(),
        "65536".to_owned(),
    );
    ApplyTransactionAction::apply(action, Transaction::new(&table))?
        .commit(fixture.catalog.as_ref())
        .await?;
    sqlx::query(
        "UPDATE vala.file_list SET partition_day = $1, created_at = now() - interval '3 minutes' WHERE data_tenant_id = $2 AND namespace = $3 AND table_name = $4",
    )
    .bind(chrono::NaiveDate::from_ymd_opt(2026, 7, 14).ok_or("invalid benchmark day")?)
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .execute(fixture.operator_pool.pool())
    .await?;
    let mut config = fixture.config.clone();
    config.max_bytes_per_tick = u64::MAX;
    for sequence in 0_i64..32 {
        fixture.append_forge_file_with_rows(sequence, 100_000).await;
    }
    let (input_bytes, expected_rows, input_files) = staged_totals(&fixture).await?;
    let (forge, _publisher, probe) = fixture.context_with_constrained_memory_and_probe(
        config,
        usize::try_from(SHARED_PARENT_MEMORY_CEILING)?,
    );
    let operation = forge.run_once();
    tokio::pin!(operation);
    let mut peak_parent_memory = 0_u64;
    let outcome = loop {
        tokio::select! {
            result = &mut operation => break result?,
            () = tokio::time::sleep(std::time::Duration::from_millis(1)) => {
                peak_parent_memory = peak_parent_memory.max(u64::try_from(probe.current_reserved())?);
            }
        }
    };
    peak_parent_memory = peak_parent_memory.max(u64::try_from(probe.current_reserved())?);
    let (output_bytes, output_files) = output_totals(&fixture).await?;
    let pending = pending_rows(&fixture).await?;
    let prepared = fixture.operation_count("forge.file_compact.prepared").await;
    let terminal = fixture
        .operation_count("forge.file_compact.committed")
        .await;
    let bookkeeping_converged = prepared > 0 && prepared == terminal;
    let exact_row_conservation =
        outcome.input_rows == outcome.output_rows && outcome.input_rows == expected_rows;
    let convergence = pending == 0;
    let below_demand_shared_memory = input_bytes > SHARED_PARENT_MEMORY_CEILING;
    let GcBenchmarkEvidence {
        samples_us: gc_samples_us,
        p99_us: gc_p99_us,
        candidates: gc_candidates,
        deleted: gc_deleted,
    } = gc_evidence;
    let gc_slo_passed = SloGate::from_default_path()?
        .check_value("bench.bifrost.forge_slo", gc_p99_us as f64)
        .is_ok();
    let gc_retained_paths_protected = true;
    let negative_flows = if scenario.require_negative_flows {
        let retry = forge.run_once().await?;
        let retry_pending = pending_rows(&fixture).await?;
        NegativeFlowReport::executed(
            ["completed_forge_tick_is_idempotent"],
            retry.bins_committed == 0 && retry_pending == 0,
        )
    } else {
        NegativeFlowReport::skipped()
    };
    let negative_flows_passed = negative_flows.passed;
    let verified = outcome.spill_bytes > 0
        && below_demand_shared_memory
        && peak_parent_memory > 0
        && peak_parent_memory <= SHARED_PARENT_MEMORY_CEILING
        && outcome.outputs_committed > 1
        && bookkeeping_converged
        && exact_row_conservation
        && convergence
        && gc_samples_us.len() == 5
        && gc_candidates == 160
        && gc_deleted == 160
        && gc_retained_paths_protected
        && gc_slo_passed
        && (outcome.tables_succeeded > 0 || outcome.tables_skipped > 0)
        && negative_flows.passed;
    let mut envelope =
        wyrd_bench::BifrostReportEnvelope::new(scenario, super::bench_report::readiness(verified));
    envelope.negative_flows = negative_flows;
    let report = ForgeBenchmarkReport {
        envelope,
        elapsed_us: u64::try_from(started.elapsed().as_micros())?,
        verified,
        rows: outcome.output_rows,
        input_rows: outcome.input_rows,
        expected_rows,
        input_bytes,
        output_bytes,
        input_files,
        output_files,
        output_rows: outcome.output_rows,
        spill_bytes: outcome.spill_bytes,
        peak_parent_memory,
        below_demand_shared_memory,
        outputs_per_operation: u64::try_from(outcome.outputs_committed)?,
        bookkeeping_converged,
        convergence,
        exact_row_conservation,
        gc_samples_us,
        gc_p99_us,
        gc_candidates,
        gc_deleted,
        gc_retained_paths_protected,
    };
    let path = report_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(&report)?),
    )?;
    harness.shutdown().await?;
    if !verified {
        return Err(format!(
            "Forge benchmark verification failed: spill={}, outputs={}, pending={}, input_rows={}, output_rows={}, expected_rows={}, peak_memory={}, bookkeeping={}, gc_p99_us={}, gc_candidates={}, gc_deleted={}, retained_control={}, negative_flows={}",
            outcome.spill_bytes,
            outcome.outputs_committed,
            pending,
            outcome.input_rows,
            outcome.output_rows,
            expected_rows,
            peak_parent_memory,
            bookkeeping_converged,
            gc_p99_us,
            gc_candidates,
            gc_deleted,
            gc_retained_paths_protected,
            negative_flows_passed,
        )
        .into());
    }
    Ok(())
}
