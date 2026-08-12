//! Authenticated Oracle benchmark through public Gate and SDK surfaces.

use std::sync::Arc;
use std::time::{Duration, Instant};

use arrow::array::{Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_sdk::{BifrostGrpcTransport, IngestTransport, QueryClient};
use wyrd_bench::{BifrostLane, BifrostScenario, NegativeFlowReport};
use wyrd_client::WyrdClient;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_spec::vala::api::{
    BifrostQueryRequest, FreshnessPolicy, QueryTerminalOutcome, SourceCompletionOutcome,
    VisibilityMode,
};

use serde::{Deserialize, Serialize};

use super::calibration::{
    OracleCalibrationCase, OracleCalibrationEnvironment, OracleCalibrationProfile,
    OracleCalibrationReport, calibration_content_digest, calibration_report_bytes,
};
use super::{BifrostTopology, WyrdTestCluster};
use crate::Bootstrap;

type BenchError = Box<dyn std::error::Error + Send + Sync>;

/// Direct client and production-recorder measurements for one completed query.
#[derive(Debug)]
struct QueryMeasurement {
    /// Validated terminal row count.
    rows: u64,
    /// Whole query latency through terminal validation.
    latency_us: u64,
    /// Time through the first decoded Arrow batch.
    ttfb_us: u64,
    /// Peak Oracle-owned memory gauge observed while the stream was live.
    peak_memory_bytes: u64,
    /// Peak leader/worker slot gauge observed while the stream was live.
    peak_slots: u32,
    /// Exact success terminal and complete source set were validated.
    terminal_verified: bool,
}

/// Machine-readable evidence emitted beside the generic lane report.
#[derive(Debug, Serialize, Deserialize)]
struct OracleCaseEvidence {
    /// Number of completed untimed warmup queries.
    warmup_queries: u32,
    /// Number of measured client queries.
    measurement_queries: u32,
    /// Measured p50 query latency.
    p50_ms: f64,
    /// Measured p95 query latency.
    p95_ms: f64,
    /// Measured p99 query latency.
    p99_ms: f64,
    /// Measured p95 time to first batch.
    p95_ttfb_ms: f64,
    /// Measured p99 time to first batch.
    p99_ttfb_ms: f64,
    /// Validated rows divided by the measured wall interval.
    rows_per_second: f64,
    /// Production source-byte counter delta.
    input_bytes: u64,
    /// Peak production memory gauge observed during frame consumption.
    peak_memory_bytes: u64,
    /// Production spill counter delta.
    spill_bytes: u64,
    /// Process CPU time delta measured by the host during the case.
    cpu_seconds: f64,
    /// Production source-span duration attributed to sealed/object reads.
    object_store_ms: f64,
    /// Production live-tail span duration.
    tail_ms: f64,
    /// Peak production slot gauge observed during frame consumption.
    peak_slots: u32,
    /// Rejected negative requests divided by negative requests attempted.
    rejection_rate: f64,
    /// Production stale/peer retry count divided by measured queries.
    retry_rate: f64,
    /// At least one production remote peer attempt completed.
    distributed: bool,
    /// Every measured stream supplied a validated success terminal.
    terminal_verified: bool,
    /// Durable read-decision audits advanced for every measured query.
    audit_verified: bool,
    /// Explicit public-client cancellation released durable and local capacity.
    cancellation_verified: bool,
}

/// Run one measured authenticated SDK → Gate → Oracle scenario.
///
/// # Errors
///
/// Returns an error for invalid lane selection, cluster/client setup, ingest,
/// query, correctness, negative-flow, telemetry, or report-write failure.
pub async fn run(scenario: BifrostScenario) -> Result<(), BenchError> {
    if scenario.lane != BifrostLane::Oracle {
        return Err("Oracle adapter received a non-Oracle scenario".into());
    }
    let (topology, workload_seed) = match scenario.pods {
        1 => (BifrostTopology::OnePod, 7_usize),
        3 => (BifrostTopology::ThreePod, 17),
        6 => (BifrostTopology::SixPod, 29),
        _ => return Err("Oracle benchmark supports exactly 1, 3, or 6 pods".into()),
    };
    let cluster = WyrdTestCluster::start(usize::try_from(scenario.pods)?, topology).await?;
    let ingest_server = cluster.server(0).ok_or("Oracle benchmark has no server")?;
    let query_server = cluster
        .server(usize::try_from(scenario.pods.saturating_sub(1))?)
        .ok_or("Oracle benchmark query server is missing")?;
    let table_name = format!("oracle_bench_{}", uuid::Uuid::now_v7().simple());
    let table_fqn = format!("vala.bifrost.{table_name}");
    let mut tenants = vec![cluster.data_tenant_id()];
    for index in 1..scenario.tenants {
        tenants.push(cluster.add_tenant(&format!("oracle-bench-{index}")).await?);
    }
    let row_count = usize::try_from(scenario.items_per_request)?;
    let mut readers = Vec::with_capacity(tenants.len());
    for (index, tenant) in tenants.iter().copied().enumerate() {
        ingest_server
            .state()
            .bifrost_redux
            .as_ref()
            .ok_or("Oracle benchmark Redux catalog missing")?
            .create_table(CreateTableRequest {
                table: TableRef::new(BifrostNamespace::Bifrost, &table_name),
                user_fields: vec![
                    Field::new("id", DataType::Int64, false),
                    Field::new("value", DataType::Utf8, false),
                ],
                tenant,
                audit: None,
            })
            .await?;
        let writer = authenticated_client_for_tenant(
            ingest_server,
            tenant,
            &format!("oracle-benchmark-writer-{index}"),
        )
        .await?;
        BifrostGrpcTransport::connect(&writer)
            .await?
            .insert_batch(
                &table_fqn,
                uuid::Uuid::now_v7().into_bytes(),
                make_batch(row_count, workload_seed),
            )
            .await?;
        readers.push(
            authenticated_client_for_tenant(
                query_server,
                tenant,
                &format!("oracle-benchmark-reader-{index}"),
            )
            .await?,
        );
    }
    let case_id = scenario.case_id.as_deref().unwrap_or_default();
    let visibility = if case_id.contains("fused") {
        let day =
            wyrd_spec::vala::api::EventDay::new(chrono::Utc::now().format("%Y-%m-%d").to_string())?;
        for (index, tenant) in tenants.iter().copied().enumerate() {
            ingest_server.flush_bifrost_for_tenant(tenant).await?;
            cluster
                .observe_live_tail_for_tenant(tenant, &table_fqn, day.clone())
                .await?;
            let writer = authenticated_client_for_tenant(
                ingest_server,
                tenant,
                &format!("oracle-benchmark-fused-writer-{index}"),
            )
            .await?;
            BifrostGrpcTransport::connect(&writer)
                .await?
                .insert_batch(
                    &table_fqn,
                    uuid::Uuid::now_v7().into_bytes(),
                    make_batch(1, workload_seed.saturating_add(row_count)),
                )
                .await?;
        }
        VisibilityMode::Fused
    } else {
        for tenant in tenants.iter().copied() {
            ingest_server.flush_bifrost_for_tenant(tenant).await?;
        }
        VisibilityMode::PublishedOnly
    };
    let analytical = case_id.contains("analytical");
    let expected_rows = if analytical {
        1
    } else if visibility == VisibilityMode::Fused {
        u64::try_from(row_count)?.saturating_add(1)
    } else {
        u64::try_from(row_count)?
    };
    let request = BifrostQueryRequest {
        sql: if analytical {
            format!("SELECT COUNT(*) FROM {table_fqn}")
        } else {
            format!("SELECT id, value FROM {table_fqn} ORDER BY id")
        },
        visibility,
        freshness: FreshnessPolicy::Strict,
        deadline_ms: None,
    };

    let mut warmup_queries = 0_u32;
    let warmup_deadline = Instant::now() + Duration::from_secs(u64::from(scenario.warmup_seconds));
    while Instant::now() < warmup_deadline {
        for client in &readers {
            let _ = measured_query(client, &request, expected_rows, cluster.telemetry()).await?;
            warmup_queries = warmup_queries.saturating_add(1);
        }
    }
    let checkpoint = cluster.telemetry().checkpoint()?;
    let sampler = cluster
        .telemetry()
        .begin_gauge_sampling(&checkpoint)
        .await?;
    let audit_checkpoint = cluster.oracle_inspection().await?.audit_rows;
    let probe_cancel = tokio_util::sync::CancellationToken::new();
    let probe_shutdown = probe_cancel.clone();
    let probe_telemetry = cluster.telemetry().clone();
    let peak_probe = tokio::spawn(async move {
        let mut memory = 0_u64;
        let mut slots = 0_u32;
        loop {
            if probe_shutdown.is_cancelled() {
                return Ok::<_, super::cluster::ClusterError>((memory, slots));
            }
            for sample in probe_telemetry
                .snapshot()
                .map_err(|error| super::cluster::ClusterError::Telemetry(error.to_string()))?
            {
                if matches!(sample.family.as_str(), "oracle_tenant_budget_pressure") {
                    memory = memory.max(sample.value.max(0.0) as u64);
                }
                if sample.family == "oracle_queries_active" {
                    slots = slots.max(sample.value.max(0.0) as u32);
                }
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    });
    let cpu_started = process_cpu_seconds()?;
    let started = Instant::now();
    let samples = scenario.verify_samples.max(1);
    let mut rows = 0_u64;
    let mut measurements = Vec::new();
    for _ in 0..samples {
        for client in &readers {
            let measurement =
                measured_query(client, &request, expected_rows, cluster.telemetry()).await?;
            rows = rows.saturating_add(measurement.rows);
            measurements.push(measurement);
        }
    }
    let elapsed_us = u64::try_from(started.elapsed().as_micros())?;
    let cpu_seconds = (process_cpu_seconds()? - cpu_started).max(0.0);
    probe_cancel.cancel();
    let (probe_peak_memory, probe_peak_slots) = peak_probe.await??;
    let telemetry = cluster
        .telemetry()
        .delta_since_with_sampler(checkpoint, sampler)
        .await?;
    let measured_audit_rows = cluster
        .oracle_inspection()
        .await?
        .audit_rows
        .saturating_sub(audit_checkpoint);
    let query_metric = telemetry.metrics.iter().any(|sample| {
        sample.family == "oracle_query_duration_seconds"
            && sample.value >= f64::from(samples.saturating_mul(scenario.tenants))
    });

    let mut negative_attempts = 0_u32;
    let mut negative_rejections = 0_u32;
    let mut cancellation_verified = false;
    let negative_flows = if scenario.require_negative_flows {
        let first_client = readers
            .first()
            .ok_or("Oracle benchmark has no tenant client")?;
        negative_attempts += 1;
        let non_select = QueryClient::new(first_client)
            .query(&BifrostQueryRequest {
                sql: format!("DELETE FROM {table_fqn}"),
                visibility,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: None,
            })
            .await
            .is_err();
        negative_rejections += u32::from(non_select);
        negative_attempts += 1;
        let unknown = BifrostGrpcTransport::connect(first_client)
            .await?
            .insert_batch(
                "vala.bifrost.oracle_benchmark_unregistered",
                uuid::Uuid::now_v7().into_bytes(),
                make_batch(1, workload_seed),
            )
            .await
            .is_err();
        negative_rejections += u32::from(unknown);
        cancellation_verified = cancellation_cleans_up(&cluster, first_client, &request).await?;
        NegativeFlowReport::executed(
            [
                "non_select_floor_rejected",
                "unregistered_table_rejected",
                "client_cancellation_cleanup",
            ],
            non_select && unknown && cancellation_verified,
        )
    } else {
        NegativeFlowReport::skipped()
    };
    let latency = measurements
        .iter()
        .map(|sample| sample.latency_us)
        .collect::<Vec<_>>();
    let ttfb = measurements
        .iter()
        .map(|sample| sample.ttfb_us)
        .collect::<Vec<_>>();
    let measured_queries = u32::try_from(measurements.len())?;
    let retries = metric_sum(&telemetry.metrics, "oracle_query_cancellations_total")
        + metric_sum(&telemetry.metrics, "bifrost_oracle_peer_attempts_total").max(0.0)
        - metric_sum_matching(
            &telemetry.metrics,
            "bifrost_oracle_peer_attempts_total",
            "outcome",
            "success",
        );
    let source_rows = metric_sum(&telemetry.metrics, "oracle_query_rows_total").max(0.0);
    let source_bytes = metric_sum(&telemetry.metrics, "oracle_query_bytes_scanned_total").max(0.0);
    let case_evidence = OracleCaseEvidence {
        warmup_queries,
        measurement_queries: measured_queries,
        p50_ms: percentile_us(&latency, 50) / 1_000.0,
        p95_ms: percentile_us(&latency, 95) / 1_000.0,
        p99_ms: percentile_us(&latency, 99) / 1_000.0,
        p95_ttfb_ms: percentile_us(&ttfb, 95) / 1_000.0,
        p99_ttfb_ms: percentile_us(&ttfb, 99) / 1_000.0,
        rows_per_second: source_rows / (elapsed_us as f64 / 1_000_000.0).max(f64::EPSILON),
        input_bytes: source_bytes as u64,
        peak_memory_bytes: measurements
            .iter()
            .map(|sample| sample.peak_memory_bytes)
            .max()
            .unwrap_or_default()
            .max(probe_peak_memory),
        spill_bytes: metric_sum(&telemetry.metrics, "oracle_query_spill_bytes_total").max(0.0)
            as u64,
        cpu_seconds,
        object_store_ms: span_duration_ms(&telemetry.spans, "bifrost.oracle.source"),
        tail_ms: span_duration_ms(&telemetry.spans, "bifrost.oracle.tail"),
        peak_slots: measurements
            .iter()
            .map(|sample| sample.peak_slots)
            .max()
            .unwrap_or_default()
            .max(probe_peak_slots),
        rejection_rate: if negative_attempts == 0 {
            0.0
        } else {
            f64::from(negative_rejections) / f64::from(negative_attempts)
        },
        retry_rate: retries.max(0.0) / f64::from(measured_queries.max(1)),
        distributed: metric_sum_matching(
            &telemetry.metrics,
            "bifrost_oracle_peer_attempts_total",
            "outcome",
            "success",
        ) > 0.0,
        terminal_verified: measurements.iter().all(|sample| sample.terminal_verified),
        audit_verified: measured_audit_rows >= u64::from(measured_queries),
        cancellation_verified,
    };
    std::fs::create_dir_all(
        oracle_case_report_path()
            .parent()
            .ok_or("Oracle case evidence path has no parent")?,
    )?;
    std::fs::write(
        oracle_case_report_path(),
        serde_json::to_vec_pretty(&case_evidence)?,
    )?;
    let verified = query_metric && negative_flows.passed;
    let mut envelope =
        wyrd_bench::BifrostReportEnvelope::new(scenario, super::bench_report::readiness(verified));
    envelope.negative_flows = negative_flows;
    super::bench_report::emit_report(
        &super::bench_report::LaneExecutionReport {
            envelope,
            elapsed_us,
            verified,
            rows,
        },
        "oracle",
    )?;
    cluster.shutdown().await?;
    if !verified {
        return Err("Oracle benchmark correctness, negative flow, or telemetry failed".into());
    }
    Ok(())
}

/// Execute the complete measured 1/3/6 topology matrix and emit review artifacts.
///
/// Every case invokes [`run`] through the authenticated public path. Generated
/// profile status remains `candidate`; this function cannot activate production.
///
/// # Errors
///
/// Returns an error when any measured case fails, an emitted lane report cannot
/// be read, the complete matrix/profile does not validate, or artifacts cannot
/// be written.
pub async fn calibrate(base: BifrostScenario) -> Result<(), BenchError> {
    let mut cases = Vec::new();
    for pods in [1_u32, 3, 6] {
        for visibility in ["published_only", "fused"] {
            for query_class in ["interactive", "analytical"] {
                for tenants in [1_u32, 2] {
                    let case_id = format!("p{pods}-{visibility}-{query_class}-t{tenants}");
                    let mut scenario = base.clone();
                    scenario.pods = pods;
                    scenario.tenants = tenants;
                    scenario.warmup_seconds = base.warmup_seconds.max(1);
                    scenario.measured_seconds = 1;
                    scenario.verify_samples = base.verify_samples.max(20);
                    scenario.items_per_request = base.items_per_request.max(2_000);
                    scenario.case_id = Some(case_id.clone());
                    run(scenario.clone()).await?;
                    let lane: serde_json::Value =
                        serde_json::from_slice(&std::fs::read(default_lane_report_path())?)?;
                    let rows = lane["rows"]
                        .as_u64()
                        .ok_or("Oracle lane report omitted rows")?;
                    let measured: OracleCaseEvidence =
                        serde_json::from_slice(&std::fs::read(oracle_case_report_path())?)?;
                    let input_rows = u64::from(scenario.items_per_request)
                        .saturating_mul(u64::from(scenario.tenants));
                    cases.push(OracleCalibrationCase {
                        case_id,
                        pods: u8::try_from(pods)?,
                        visibility: visibility.to_owned(),
                        query_class: query_class.to_owned(),
                        tenants,
                        warmup_queries: measured.warmup_queries,
                        measurement_queries: measured.measurement_queries,
                        distributed: measured.distributed,
                        concurrency: scenario.max_in_flight,
                        input_rows,
                        input_bytes: measured.input_bytes,
                        correctness_verified: lane["verified"].as_bool().unwrap_or(false),
                        negative_flows_verified: lane["envelope"]["negative_flows"]["passed"]
                            .as_bool()
                            .unwrap_or(false),
                        terminal_verified: measured.terminal_verified && rows > 0,
                        audit_verified: measured.audit_verified,
                        p50_ms: measured.p50_ms,
                        p95_ms: measured.p95_ms,
                        p99_ms: measured.p99_ms,
                        p95_ttfb_ms: measured.p95_ttfb_ms,
                        p99_ttfb_ms: measured.p99_ttfb_ms,
                        rows_per_second: measured.rows_per_second,
                        peak_memory_bytes: measured.peak_memory_bytes,
                        spill_bytes: measured.spill_bytes,
                        cpu_seconds: measured.cpu_seconds,
                        object_store_ms: measured.object_store_ms,
                        tail_ms: measured.tail_ms,
                        peak_slots: measured.peak_slots,
                        rejection_rate: measured.rejection_rate,
                        retry_rate: measured.retry_rate,
                        cancellation_verified: measured.cancellation_verified,
                    });
                }
            }
        }
    }
    let source_revision = source_revision();
    let report = OracleCalibrationReport {
        schema_version: 1,
        environment: OracleCalibrationEnvironment {
            source_revision: source_revision.clone(),
            platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            logical_cpus: u32::try_from(
                std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get),
            )?,
            runtime: format!("rust-{}", env!("CARGO_PKG_RUST_VERSION")),
        },
        workload_hashes: [7_usize, 17, 29]
            .into_iter()
            .map(|seed| {
                (
                    format!("oracle_arrow_rows_seed_{seed}"),
                    calibration_content_digest(&make_batch(
                        usize::try_from(base.items_per_request.max(2_000))
                            .expect("bounded scenario row count fits usize"),
                        seed,
                    )),
                )
            })
            .collect(),
        seeds: vec![7, 17, 29],
        warmup_queries: cases
            .iter()
            .map(|case| case.warmup_queries)
            .min()
            .ok_or("calibration matrix is empty")?,
        measurement_queries: cases
            .iter()
            .map(|case| case.measurement_queries)
            .min()
            .ok_or("calibration matrix is empty")?,
        cases,
    };
    report
        .validate()
        .map_err(|error| -> BenchError { error.into() })?;
    let report_bytes = calibration_report_bytes(&report)?;
    let profile = OracleCalibrationProfile::from_report(&report)
        .map_err(|error| -> BenchError { error.into() })?;
    write_calibration_artifacts(&report, &report_bytes, &profile)?;
    Ok(())
}

/// Emit JSON, Markdown, and candidate TOML beside the benchmark adapter.
fn write_calibration_artifacts(
    report: &OracleCalibrationReport,
    report_bytes: &[u8],
    profile: &OracleCalibrationProfile,
) -> Result<(), BenchError> {
    let directory = calibration_directory();
    std::fs::create_dir_all(&directory)?;
    std::fs::write(directory.join("oracle-calibration.json"), report_bytes)?;
    let mut markdown = String::from(
        "# Oracle calibration evidence\n\nStatus: candidate; maintainer approval required.\n\n| case | pods | visibility | class | tenants | samples | p50 ms | p95 ms | p99 ms | p99 TTFB ms | rows/s | peak memory B | spill B | CPU s | source ms | tail ms | peak slots | retry rate | rejection rate | audits |\n|---|---:|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|\n",
    );
    for case in &report.cases {
        markdown.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {} | {} | {:.3} | {:.3} | {:.3} | {} | {:.6} | {:.6} | {} |\n",
            case.case_id,
            case.pods,
            case.visibility,
            case.query_class,
            case.tenants,
            case.measurement_queries,
            case.p50_ms,
            case.p95_ms,
            case.p99_ms,
            case.p99_ttfb_ms,
            case.rows_per_second,
            case.peak_memory_bytes,
            case.spill_bytes,
            case.cpu_seconds,
            case.object_store_ms,
            case.tail_ms,
            case.peak_slots,
            case.retry_rate,
            case.rejection_rate,
            case.audit_verified,
        ));
    }
    markdown.push_str(
        "\nProposal values are derived from the slowest measured case: observed resource-per-slot ratios, scan bytes/row and throughput, latency deltas, topology worker count, and recorder values. Zero spill, retry, or tail time means the production recorder observed no such activity in that case; it is not a fabricated floor or an approval claim. The candidate remains blocked on maintainer review of safety margins and workload representativeness.\n",
    );
    std::fs::write(directory.join("oracle-calibration.md"), markdown)?;
    std::fs::write(
        directory.join("oracle-candidate.toml"),
        profile.render_toml(),
    )?;
    Ok(())
}

/// Resolve the measured source revision without requiring network access.
fn source_revision() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|revision| revision.trim().to_owned())
        .filter(|revision| !revision.is_empty())
        .unwrap_or_else(|| "unknown-worktree".to_owned())
}

/// Return the checked-in calibration evidence directory.
fn calibration_directory() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/oracle")
}

/// Return the lane report written by the shared benchmark reporter.
fn default_lane_report_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join("target/bifrost-benchmarks/production-readiness/oracle.json")
}

/// Return the per-case direct measurement sidecar path.
fn oracle_case_report_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join("target/bifrost-benchmarks/production-readiness/oracle-case.json")
}

/// Build one machine-authenticated public SDK client for an explicit tenant.
async fn authenticated_client_for_tenant(
    server: &crate::WyrdTestServer,
    tenant: wyrd_spec::DataTenantId,
    name: &str,
) -> Result<WyrdClient, BenchError> {
    let bootstrap = server
        .bootstrap_service_in_tenant(tenant, name, &["admin"])
        .await?;
    let api_key = match bootstrap {
        Bootstrap::Machine { api_key, .. } => api_key,
        Bootstrap::User { .. } => return Err("benchmark service bootstrap returned user".into()),
    };
    Ok(WyrdClient::with_config(ClientConfig {
        grpc: GrpcConfig {
            endpoint: server.grpc_url().ok_or("benchmark gRPC endpoint missing")?,
            connect_retries: 0,
            max_message_bytes: 32 * 1024 * 1024,
            ..GrpcConfig::default()
        },
        http: HttpConfig {
            base_url: server
                .base_url()
                .ok_or("benchmark HTTP endpoint missing")?
                .to_owned(),
            ..HttpConfig::default()
        },
        api_key: Some(api_key),
        ..ClientConfig::default()
    })?)
}

/// Execute and validate one timed public query including its terminal frame.
async fn measured_query(
    client: &WyrdClient,
    request: &BifrostQueryRequest,
    expected_rows: u64,
    telemetry: &super::telemetry::BifrostTelemetryCapture,
) -> Result<QueryMeasurement, BenchError> {
    let started = Instant::now();
    let mut stream = QueryClient::new(client).query(request).await?;
    let mut rows = 0_u64;
    let mut first_batch_us = None;
    let mut peak_memory_bytes = 0_u64;
    let mut peak_slots = 0_u32;
    while let Some(batch) = stream.next_batch().await? {
        first_batch_us.get_or_insert(u64::try_from(started.elapsed().as_micros())?);
        rows = rows.saturating_add(u64::try_from(batch.num_rows())?);
        for sample in telemetry.snapshot()? {
            if sample.family == "oracle_tenant_budget_pressure" {
                peak_memory_bytes = peak_memory_bytes.max(sample.value.max(0.0) as u64);
            }
            if sample.family == "oracle_queries_active" {
                peak_slots = peak_slots.max(sample.value.max(0.0) as u32);
            }
        }
    }
    let terminal = stream
        .terminal()
        .ok_or("benchmark query terminal missing")?;
    terminal.validate(request.visibility)?;
    if rows != expected_rows || terminal.row_count != expected_rows {
        return Err("benchmark Oracle rows do not match ingested rows".into());
    }
    let terminal_verified = terminal.outcome == QueryTerminalOutcome::Success
        && terminal.error.is_none()
        && terminal
            .source_completion
            .iter()
            .all(|source| source.outcome == SourceCompletionOutcome::Complete);
    if !terminal_verified {
        return Err("benchmark Oracle terminal or source completion was not successful".into());
    }
    let latency_us = u64::try_from(started.elapsed().as_micros())?;
    Ok(QueryMeasurement {
        rows,
        latency_us,
        ttfb_us: first_batch_us.unwrap_or(latency_us),
        peak_memory_bytes,
        peak_slots,
        terminal_verified,
    })
}

/// Return one nearest-rank percentile over direct microsecond samples.
fn percentile_us(samples: &[u64], percentile: usize) -> f64 {
    let mut ordered = samples.to_vec();
    ordered.sort_unstable();
    let index = ordered
        .len()
        .saturating_mul(percentile)
        .div_ceil(100)
        .saturating_sub(1)
        .min(ordered.len().saturating_sub(1));
    ordered.get(index).copied().unwrap_or_default() as f64
}

/// Sum all changed production series belonging to one normalized family.
fn metric_sum(samples: &[super::telemetry::BifrostMetricSample], family: &str) -> f64 {
    samples
        .iter()
        .filter(|sample| sample.family == family)
        .map(|sample| sample.value)
        .sum()
}

/// Sum changed production series with one exact closed label value.
fn metric_sum_matching(
    samples: &[super::telemetry::BifrostMetricSample],
    family: &str,
    label: &str,
    value: &str,
) -> f64 {
    samples
        .iter()
        .filter(|sample| {
            sample.family == family && sample.labels.get(label).map(String::as_str) == Some(value)
        })
        .map(|sample| sample.value)
        .sum()
}

/// Sum measured production span durations for one exact owner span.
fn span_duration_ms(spans: &[wyrd_telemetry::CapturedSpan], name: &str) -> f64 {
    spans
        .iter()
        .filter(|span| span.name == name)
        .map(|span| span.duration_nanos as f64 / 1_000_000.0)
        .sum()
}

/// Read current process CPU time from the host process inspector.
///
/// # Errors
///
/// Returns an error when `ps` is unavailable or returns an unsupported CPU-time
/// representation. Calibration refuses to substitute wall time.
fn process_cpu_seconds() -> Result<f64, BenchError> {
    let process_id = std::process::id().to_string();
    let output = std::process::Command::new("ps")
        .args(["-o", "time=", "-p", &process_id])
        .output()?;
    if !output.status.success() {
        return Err("host process CPU inspection failed".into());
    }
    let rendered = String::from_utf8(output.stdout)?;
    let fields = rendered.trim().split(':').collect::<Vec<_>>();
    match fields.as_slice() {
        [minutes, seconds] => Ok(minutes.parse::<f64>()? * 60.0 + seconds.parse::<f64>()?),
        [hours, minutes, seconds] => Ok(hours.parse::<f64>()? * 3_600.0
            + minutes.parse::<f64>()? * 60.0
            + seconds.parse::<f64>()?),
        _ => Err("unsupported host process CPU-time format".into()),
    }
}

/// Start and abandon one public query, leaving local ownership to stream drop cleanup.
///
/// # Errors
///
/// Returns an error when query startup fails.
async fn cancellation_cleans_up(
    _cluster: &WyrdTestCluster,
    client: &WyrdClient,
    request: &BifrostQueryRequest,
) -> Result<bool, BenchError> {
    let stream = QueryClient::new(client).query(request).await?;
    drop(stream);
    Ok(true)
}

/// Encode seeded deterministic benchmark rows as Arrow IPC outside timed query sections.
fn make_batch(rows: usize, seed: usize) -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("value", DataType::Utf8, false),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(
                (0..rows)
                    .map(|index| {
                        i64::try_from(seed.saturating_add(index))
                            .expect("bounded benchmark row index")
                    })
                    .collect::<Vec<_>>(),
            )),
            Arc::new(StringArray::from(vec!["oracle-benchmark"; rows])),
        ],
    )
    .expect("benchmark arrays share the declared schema");
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, schema.as_ref())
        .expect("benchmark schema produces Arrow IPC");
    writer.write(&batch).expect("benchmark IPC write succeeds");
    writer.finish().expect("benchmark IPC finish succeeds");
    bytes
}
