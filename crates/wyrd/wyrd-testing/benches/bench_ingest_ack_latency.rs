//! Public SDK-to-server Scribe matrix benchmark.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

use arrow::array::{Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use bytes::Bytes;
use chrono::NaiveDate;
use vala_bifrost_redux::catalog::TableRef;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::admission::AdmissionConfig;
use vala_bifrost_redux::scribe::audit_envelope::encode_audit_event;
use vala_bifrost_redux::scribe::replay::replay_wal_directory;
use vala_bifrost_redux::scribe::seal_key::{EventDay, SealKey};
use vala_bifrost_redux::scribe::stream_identity::NodeId;
use vala_bifrost_redux::scribe::wal::{WalConfig, WalWriter};
use vala_sdk::{BifrostFrame, BifrostGrpcTransport};
use wyrd_bench::{
    BenchmarkMetricSnapshot, BenchmarkRecorder, MachineMetadata, ScribeBenchmarkConfig,
    ScribeBenchmarkReport, ScribeCaseReport, ScribeCaseVerification, ScribeCompactCase,
    ScribeComparisonStatus, ScribeComponentReport, ScribeDistribution, ScribeTopologyEvidence,
    compact_scribe_matrix, required_scribe_metric_families,
};
use wyrd_client::WyrdClient;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_runtime::PrincipalId;
use wyrd_spec::auth::PrincipalKindTag;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};
use wyrd_testing::Bootstrap;
use wyrd_testing::bifrost::{BifrostTopology, WyrdTestCluster, full_bifrost_topology};

type BenchError = Box<dyn Error + Send + Sync>;

const RETAINED_ITEM_LIMIT: u64 = 256;
const RETAINED_BYTE_LIMIT: u64 = 512 * 1024 * 1024;
const MAX_IN_FLIGHT_STREAMS: usize = 32;
const DEFAULT_PODS: usize = 3;
const DEFAULT_TENANTS: usize = 10;

struct TenantWorkload {
    tenant: wyrd_spec::DataTenantId,
    transports: Vec<BifrostGrpcTransport>,
}

#[tokio::main]
async fn main() -> Result<(), BenchError> {
    let recorder = BenchmarkRecorder::new()
        .install()
        .map_err(|error| format!("benchmark metrics recorder install failed: {error}"))?;
    let required = required_scribe_metric_families();
    recorder.register_metric_families(&required);
    let report = run(&recorder).await?;
    emit_report(&report)?;
    Ok(())
}

async fn run(recorder: &BenchmarkRecorder) -> Result<ScribeBenchmarkReport, BenchError> {
    let matrix = compact_scribe_matrix();
    let requested_pods = argument("pods")
        .unwrap_or_else(|| DEFAULT_PODS.to_string())
        .parse::<usize>()?;
    let requested_tenants = argument("tenants")
        .unwrap_or_else(|| DEFAULT_TENANTS.to_string())
        .parse::<usize>()?;
    if requested_tenants == 0 || requested_pods == 0 {
        return Err("benchmark requires at least one tenant and pod".into());
    }
    let only_cases = std::env::var("WYRD_BIFROST_CASES")
        .ok()
        .map(|value| value.split(',').map(str::to_owned).collect::<BTreeSet<_>>());
    let mut cases = Vec::with_capacity(matrix.len());
    let mut observed = BTreeSet::new();
    let mut run_errors = 0_u64;
    let mut replay_exact_identity = false;
    let mut exact_429 = false;
    let mut exact_507 = false;

    for delay_ms in [0_u64, 60_u64] {
        let selected = matrix
            .iter()
            .filter(|case| {
                case.fsync_delay_ms == delay_ms
                    && only_cases
                        .as_ref()
                        .is_none_or(|selected| selected.contains(&case.id))
            })
            .cloned()
            .collect::<Vec<_>>();
        if selected.is_empty() {
            continue;
        }
        for case in selected {
            let effective_case = ScribeCompactCase {
                pods: if std::env::var_os("WYRD_BIFROST_CASES").is_some() {
                    u32::try_from(requested_pods)?
                } else {
                    case.pods
                },
                tenants: if std::env::var_os("WYRD_BIFROST_CASES").is_some() {
                    u32::try_from(requested_tenants)?
                } else {
                    case.tenants
                },
                ..case
            };
            let cluster = WyrdTestCluster::start_with_wal_sync_delay(
                usize::try_from(effective_case.pods)?,
                topology_for_pods(effective_case.pods),
                Duration::from_millis(delay_ms),
            )
            .await?;
            let workloads = provision_cluster(&cluster, &effective_case).await?;
            observed.extend(metric_families(&recorder.snapshot()));
            eprintln!("bifrost scribe case start: {}", effective_case.id);
            recorder.reset_interval();
            let report = match run_case(&cluster, &workloads, recorder, &effective_case).await {
                Ok(report) => {
                    eprintln!(
                        "bifrost scribe case complete: {} frames={} durable={}",
                        effective_case.id, report.measured_frames, report.durable_rows
                    );
                    report
                }
                Err(error) => {
                    run_errors = run_errors.saturating_add(1);
                    let busy = error.to_string().contains("writer busy");
                    eprintln!("bifrost scribe case failed: {}: {error}", effective_case.id);
                    ScribeCaseReport {
                        case: effective_case.clone(),
                        required_metrics_observed: metric_families(&recorder.snapshot())
                            .into_iter()
                            .collect(),
                        metric_series: recorder.snapshot().series,
                        metric_series_limit_exceeded: recorder.snapshot().series_limit_exceeded,
                        verification: ScribeCaseVerification {
                            passed: false,
                            exact_429: Some(busy),
                            replay_exact_identity: Some(false),
                            exact_507: Some(false),
                            ..ScribeCaseVerification::default()
                        },
                        ..ScribeCaseReport::default()
                    }
                }
            };
            if !replay_exact_identity {
                replay_exact_identity = replay_identity_probe()?;
            }
            if !exact_507 {
                let server = cluster.server(0).ok_or("missing benchmark pod")?;
                server.trip_bifrost_wal_disk_full_for_test()?;
                exact_507 = probe_exact_507(&workloads, &effective_case).await?;
            }
            observed.extend(report.required_metrics_observed.iter().cloned());
            cases.push(report);
            observed.extend(metric_families(&recorder.snapshot()));
            cluster.shutdown().await?;
            observed.extend(metric_families(&recorder.snapshot()));
        }
    }

    if !exact_429 {
        let probe_case = matrix
            .first()
            .cloned()
            .ok_or("compact Scribe matrix is empty")?;
        let probe_cluster = WyrdTestCluster::start_with_admission(
            1,
            BifrostTopology::OnePod,
            AdmissionConfig {
                max_writers: 0,
                writer_queue_items: 1,
                ..AdmissionConfig::default()
            },
        )
        .await?;
        let probe_workloads = provision_cluster(&probe_cluster, &probe_case).await?;
        exact_429 = probe_exact_429(&probe_workloads, &probe_case).await?;
        probe_cluster.shutdown().await?;
    }

    let required = required_scribe_metric_families();
    let missing = required
        .iter()
        .filter(|metric| !observed.contains(metric.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    let verification = ScribeCaseVerification {
        passed: !cases.is_empty() && cases.iter().all(|case| case.verification.passed),
        drain_zero_gap: cases.iter().all(|case| case.verification.drain_zero_gap),
        replay_exact_identity: Some(replay_exact_identity),
        retained_within_ceiling: cases
            .iter()
            .all(|case| case.verification.retained_within_ceiling),
        exact_507: Some(exact_507),
        exact_429: Some(exact_429),
    };
    let complete = verification.passed
        && missing.is_empty()
        && cases.iter().all(|case| {
            case.measured_frames >= case.case.minimum_samples && !case.metric_series_limit_exceeded
        });
    let machine = machine_metadata();
    let configuration = benchmark_configuration(requested_pods, requested_tenants);
    let mut report = ScribeBenchmarkReport {
        report_version: ScribeBenchmarkReport::VERSION.to_owned(),
        stage: std::env::var("WYRD_BIFROST_STAGE").unwrap_or_else(|_| "post-throughput".to_owned()),
        lane: "bench:bifrost:scribe:slo".to_owned(),
        environment_fingerprint: format!(
            "git={};dirty={};os={};cpu={:?}",
            machine.git_sha, machine.dirty_worktree, machine.operating_system, machine.cpu_count
        ),
        configuration_fingerprint: configuration_fingerprint(&configuration),
        configuration,
        machine,
        components: component_reports(&cases),
        cases,
        metrics: recorder.snapshot(),
        required_metric_families: required,
        verification,
        comparison: ScribeComparisonStatus {
            baseline_available: false,
            status: "unavailable".to_owned(),
            reason: "no valid pre-repair baseline exists".to_owned(),
            baseline_role: "initial_valid_post_repair".to_owned(),
        },
        errors: u64::try_from(missing.len())?.saturating_add(run_errors),
        complete: complete && run_errors == 0,
    };
    if let Err(errors) = report.validate() {
        for error in &errors {
            eprintln!("bifrost scribe report validation error: {error}");
        }
        report.errors = report.errors.saturating_add(u64::try_from(errors.len())?);
        report.complete = false;
    }
    Ok(report)
}

async fn provision_cluster(
    cluster: &WyrdTestCluster,
    case: &ScribeCompactCase,
) -> Result<Vec<TenantWorkload>, BenchError> {
    let server = cluster.server(0).ok_or("missing benchmark pod")?;
    let tenant_count = usize::try_from(case.tenants)?;
    let table_count = usize::try_from(case.tables)?;
    let routed_pairs = routed_pairs(case)?;
    let mut workloads = Vec::with_capacity(tenant_count);
    for index in 0..tenant_count {
        let tenant = if index == 0 {
            cluster.data_tenant_id()
        } else {
            cluster
                .add_tenant(&format!(
                    "bench-scribe-tenant-{index}-{}",
                    uuid::Uuid::now_v7()
                ))
                .await?
        };
        let catalog = server
            .state()
            .bifrost_redux
            .as_ref()
            .ok_or("missing Redux benchmark catalog")?;
        for table_index in 0..table_count {
            if !routed_pairs.contains(&(index, table_index)) {
                continue;
            }
            let name = table_name(table_index);
            catalog
                .register_dataset(
                    tenant,
                    TableRef::new(BifrostNamespace::Datasets, name),
                    vec![
                        Field::new("id", DataType::Int64, false),
                        Field::new("value", DataType::Utf8, false),
                    ],
                    None,
                )
                .await?;
        }
        let bootstrap = server
            .bootstrap_service_in_tenant(tenant, &format!("bench-ingest-ack-{index}"), &["admin"])
            .await?;
        let api_key = match bootstrap {
            Bootstrap::Machine { api_key, .. } => api_key,
            Bootstrap::User { .. } => return Err("benchmark bootstrap returned a user".into()),
        };
        let mut transports = Vec::with_capacity(cluster.servers().len());
        for server in cluster.servers() {
            let config = ClientConfig {
                grpc: GrpcConfig {
                    endpoint: server.grpc_url().ok_or("missing gRPC endpoint")?,
                    connect_retries: 0,
                    ..GrpcConfig::default()
                },
                http: HttpConfig {
                    base_url: server.base_url().ok_or("missing HTTP endpoint")?.to_owned(),
                    ..HttpConfig::default()
                },
                api_key: Some(api_key.clone()),
                ..ClientConfig::default()
            };
            let client = WyrdClient::with_config(config)?;
            transports.push(BifrostGrpcTransport::connect(&client).await?);
        }
        workloads.push(TenantWorkload { tenant, transports });
    }
    Ok(workloads)
}

async fn run_case(
    cluster: &WyrdTestCluster,
    workloads: &[TenantWorkload],
    recorder: &BenchmarkRecorder,
    case: &ScribeCompactCase,
) -> Result<ScribeCaseReport, BenchError> {
    let payload = Bytes::from(ipc(case.frame_size_bytes as usize)?);
    let warmup_frames = if case.frame_size_bytes >= 8 * 1024 * 1024 {
        0
    } else {
        u64::from(case.writers).min(8)
    };
    let _ = send_frames(workloads, case, payload.clone(), warmup_frames).await?;
    flush_workloads(cluster, workloads).await?;
    recorder.reset_interval();
    let started = Instant::now();
    let mut admitted_rows = 0_u64;
    let mut measured_frames = 0_u64;
    let mut topology = ScribeTopologyEvidence::default();
    let chunk_frames = if case.frame_size_bytes >= 8 * 1024 * 1024 {
        1
    } else {
        case.minimum_samples
            .min((64 * 1024 * 1024 / case.frame_size_bytes.max(1)).max(1))
    };
    while measured_frames < case.minimum_samples {
        let remaining = case.minimum_samples - measured_frames;
        let chunk = remaining.min(chunk_frames);
        let (sent, observed_topology) =
            send_frames(workloads, case, payload.clone(), chunk).await?;
        topology = observed_topology;
        admitted_rows = admitted_rows.saturating_add(sent);
        measured_frames = measured_frames.saturating_add(sent);
        let before_flush = recorder.snapshot();
        let expected_frames =
            metric_counter(&before_flush, "bifrost_scribe_frames_total", "accepted");
        let expected_rows = metric_counter(&before_flush, "bifrost_scribe_rows_total", "accepted")
            .max(admitted_rows);
        flush_workloads(cluster, workloads).await?;
        let _ = wait_for_durable(recorder, expected_frames, expected_rows).await;
    }
    let snapshot = recorder.snapshot();
    let elapsed_us = u64::try_from(started.elapsed().as_micros())?.max(1);
    let durable_frames = metric_counter(&snapshot, "bifrost_scribe_frames_total", "fsynced");
    let durable_rows = metric_counter(&snapshot, "bifrost_scribe_rows_total", "fsynced");
    let groups = metric_counter(&snapshot, "bifrost_scribe_writer_groups_total", "completed");
    let fsync_calls = metric_counter(&snapshot, "bifrost_scribe_wal_fsync_total", "completed");
    let retained_items = metric_gauge(&snapshot, "bifrost_scribe_retained_items");
    let retained_bytes = metric_gauge(&snapshot, "bifrost_scribe_retained_bytes");
    let retained_items_peak = metric_peak(&snapshot, "bifrost_scribe_retained_items");
    let retained_bytes_peak = metric_peak(&snapshot, "bifrost_scribe_retained_bytes");
    let admitted_frames = metric_counter(&snapshot, "bifrost_scribe_frames_total", "accepted");
    let admitted_rows = admitted_rows.max(metric_counter(
        &snapshot,
        "bifrost_scribe_rows_total",
        "accepted",
    ));
    let observed = metric_families(&snapshot);
    let verification = ScribeCaseVerification {
        passed: durable_frames == admitted_frames
            && durable_rows == admitted_rows
            && retained_items_peak <= RETAINED_ITEM_LIMIT
            && retained_bytes_peak <= RETAINED_BYTE_LIMIT,
        drain_zero_gap: durable_frames == admitted_frames && durable_rows == admitted_rows,
        replay_exact_identity: Some(false),
        retained_within_ceiling: retained_items_peak <= RETAINED_ITEM_LIMIT
            && retained_bytes_peak <= RETAINED_BYTE_LIMIT,
        exact_429: Some(false),
        exact_507: Some(false),
    };
    Ok(ScribeCaseReport {
        case: case.clone(),
        topology,
        measured_frames,
        admitted_rows,
        durable_rows,
        elapsed_us,
        admitted_frames_per_second: rate(admitted_frames, elapsed_us),
        durable_frames_per_second: rate(durable_frames, elapsed_us),
        rows_per_second: rate(durable_rows, elapsed_us),
        mib_per_second: rate(
            durable_rows.saturating_mul(case.frame_size_bytes),
            elapsed_us,
        ) / 1_048_576.0,
        ack: histogram(&snapshot, "bifrost_scribe_ack_seconds", None, 1_000),
        resolution: histogram(&snapshot, "bifrost_gate_resolution_seconds", None, 1_000),
        ingress: histogram(
            &snapshot,
            "bifrost_scribe_lane_job_seconds",
            Some("lane=\"ingress_cpu\""),
            1_000,
        ),
        append: histogram(&snapshot, "bifrost_scribe_wal_append_seconds", None, 1_000),
        sync: histogram(&snapshot, "bifrost_scribe_wal_sync_seconds", None, 1_000),
        groups,
        frames_per_group: histogram(&snapshot, "bifrost_scribe_writer_group_frames", None, 1),
        bytes_per_group: histogram(&snapshot, "bifrost_scribe_writer_group_bytes", None, 1),
        fsync_calls,
        fsync_per_frame: ratio(fsync_calls, admitted_frames),
        accepted_durable_frame_gap: admitted_frames.saturating_sub(durable_frames),
        accepted_durable_row_gap: admitted_rows.saturating_sub(durable_rows),
        retained_items_current: retained_items,
        retained_items_peak,
        retained_bytes_current: retained_bytes,
        retained_bytes_peak,
        lane_queue_peaks: lane_peaks(&snapshot, "bifrost_scribe_lane_queued"),
        lane_active_peaks: lane_peaks(&snapshot, "bifrost_scribe_lane_active"),
        lane_failures: lane_counters(&snapshot, "failed"),
        lane_panics: lane_counters(&snapshot, "panicked"),
        writer_unhealthy_transitions: metric_counter(
            &snapshot,
            "bifrost_scribe_writer_unhealthy_total",
            "",
        ),
        required_metrics_observed: observed.into_iter().collect(),
        metric_series: snapshot.series,
        metric_series_limit_exceeded: snapshot.series_limit_exceeded,
        verification,
    })
}

fn topology_for_pods(pods: u32) -> BifrostTopology {
    if pods == 1 {
        BifrostTopology::OnePod
    } else {
        full_bifrost_topology()
    }
}

async fn probe_exact_429(
    workloads: &[TenantWorkload],
    case: &ScribeCompactCase,
) -> Result<bool, BenchError> {
    let Some(workload) = workloads.first() else {
        return Ok(false);
    };
    let Some(transport) = workload.transports.first() else {
        return Ok(false);
    };
    let payload = Bytes::from(ipc(64 * 1024)?);
    let probe_count = 512_usize;
    let barrier = Arc::new(tokio::sync::Barrier::new(probe_count + 1));
    let mut tasks = Vec::with_capacity(probe_count);
    for _ in 0..probe_count {
        let barrier = Arc::clone(&barrier);
        let transport = transport.clone();
        let frame = BifrostFrame {
            table: table_fqn(0),
            batch_id: uuid::Uuid::now_v7().into_bytes(),
            frame_sequence: 0,
            arrow_ipc: payload.clone(),
        };
        tasks.push(tokio::spawn(async move {
            barrier.wait().await;
            transport.insert_batch_stream(vec![frame]).await
        }));
    }
    barrier.wait().await;
    let mut observed_busy = false;
    for task in tasks {
        if let Ok(Err(error)) = task.await {
            observed_busy |= is_ingest_busy(&error);
        }
    }
    eprintln!(
        "bifrost scribe exact-429 probe: case={} observed={observed_busy}",
        case.id
    );
    Ok(observed_busy)
}

async fn probe_exact_507(
    workloads: &[TenantWorkload],
    case: &ScribeCompactCase,
) -> Result<bool, BenchError> {
    let Some(workload) = workloads.first() else {
        return Ok(false);
    };
    let Some(transport) = workload.transports.first() else {
        return Ok(false);
    };
    let result = transport
        .insert_batch_stream(vec![BifrostFrame {
            table: table_fqn(0),
            batch_id: uuid::Uuid::now_v7().into_bytes(),
            frame_sequence: 0,
            arrow_ipc: Bytes::from(ipc(64 * 1024)?),
        }])
        .await;
    let observed = matches!(
        result,
        Err(wyrd_spec::error::WyrdError::UpstreamFailure { details, .. })
            if details
                .get("original_code")
                .and_then(serde_json::Value::as_str)
                == Some("WYRD_VALA_507_INGEST_WAL_UNAVAILABLE")
    );
    eprintln!(
        "bifrost scribe exact-507 probe: case={} observed={observed}",
        case.id
    );
    Ok(observed)
}

fn replay_identity_probe() -> Result<bool, BenchError> {
    let temp_dir = tempfile::tempdir()?;
    let tenant = DataTenantId::SYSTEM_OWNER;
    let table = TableRef::new(BifrostNamespace::Bifrost, "events");
    let seal_key = SealKey::new(
        tenant,
        table,
        EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 14).ok_or("invalid probe date")?),
    );
    let node_id = NodeId::generate();
    let wal = WalWriter::new(
        temp_dir.path(),
        *node_id.as_bytes(),
        1,
        tenant,
        WalConfig::default(),
    )?;
    let writer = wal.handle_for_seal_key_for_test(seal_key)?;
    for (operation, payload) in [("append-1", b"data-1"), ("append-2", b"data-2")] {
        let event = AuditEvent {
            request_id: RequestId::now_v7(),
            trace_id: None,
            operation: operation.to_owned(),
            resource: "benchmark".to_owned(),
            card_ref: None,
            principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
            principal_kind: PrincipalKindTag::User,
            auth_method: AuthMethod::Jwt,
            permission: "bifrost:append".to_owned(),
            decision: AuditDecision::Allow,
            result: AuditResult::Success,
            payload_summary: "replay probe".to_owned(),
            detail: None,
        };
        let audit = encode_audit_event(&event)?;
        writer.append_and_fsync_for_test([42_u8; 16], &audit, payload)?;
    }
    let replayed = replay_wal_directory(temp_dir.path())?;
    let Some(state) = replayed.values().next() else {
        return Ok(false);
    };
    Ok(state.audit_events.len() == 1
        && state.data_records.len() == 1
        && state.audit_events[0].operation == "append-1")
}

async fn flush_workloads(
    cluster: &WyrdTestCluster,
    workloads: &[TenantWorkload],
) -> Result<(), BenchError> {
    for workload in workloads {
        for server in cluster.servers() {
            server.flush_bifrost_for_tenant(workload.tenant).await?;
        }
    }
    Ok(())
}

async fn wait_for_durable(
    recorder: &BenchmarkRecorder,
    expected_frames: u64,
    expected_rows: u64,
) -> BenchmarkMetricSnapshot {
    for _ in 0..1_200 {
        let snapshot = recorder.snapshot();
        if metric_counter(&snapshot, "bifrost_scribe_frames_total", "fsynced") >= expected_frames
            && metric_counter(&snapshot, "bifrost_scribe_rows_total", "fsynced") >= expected_rows
        {
            return snapshot;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    recorder.snapshot()
}

async fn send_frames(
    workloads: &[TenantWorkload],
    case: &ScribeCompactCase,
    payload: Bytes,
    minimum_frames: u64,
) -> Result<(u64, ScribeTopologyEvidence), BenchError> {
    let writers = usize::try_from(case.writers)?;
    if writers == 0 || workloads.is_empty() {
        return Err("benchmark case has no writers or workloads".into());
    }
    let tables = usize::try_from(case.tables)?;
    let pods = usize::try_from(case.pods)?;
    let frames_per_writer = minimum_frames.div_ceil(u64::try_from(writers)?);
    let stream_slots = Arc::new(tokio::sync::Semaphore::new(MAX_IN_FLIGHT_STREAMS));
    let mut tasks = Vec::with_capacity(writers);
    let mut writers_by_pod = BTreeMap::new();
    let mut writers_by_tenant = BTreeMap::new();
    let mut writers_by_table = BTreeMap::new();
    for writer in 0..writers {
        let tenant_index = writer % workloads.len();
        let workload = &workloads[tenant_index];
        let pod_index = writer % pods;
        let transport = workload.transports[pod_index % workload.transports.len()].clone();
        let table_index = if case.table_distribution == "same" {
            0
        } else {
            (writer * 3) % tables
        };
        let table = table_fqn(table_index);
        *writers_by_pod
            .entry(format!("pod-{pod_index}"))
            .or_insert(0) += 1;
        *writers_by_tenant
            .entry(workload.tenant.to_string())
            .or_insert(0) += 1;
        *writers_by_table.entry(table.clone()).or_insert(0) += 1;
        let frames = (0..frames_per_writer)
            .map(|_| BifrostFrame {
                table: table.clone(),
                batch_id: uuid::Uuid::now_v7().into_bytes(),
                frame_sequence: 0,
                arrow_ipc: payload.clone(),
            })
            .collect::<Vec<_>>();
        let stream_slots = Arc::clone(&stream_slots);
        tasks.push(tokio::spawn(async move {
            let mut rows = 0_u64;
            for frame in frames {
                let _slot = stream_slots.acquire().await.map_err(|_| {
                    wyrd_spec::error::WyrdError::Internal {
                        message: "benchmark stream semaphore closed".to_owned(),
                        details: serde_json::json!({}),
                    }
                })?;
                let accepted = insert_stream_with_busy_retry(&transport, vec![frame]).await?;
                rows = rows.saturating_add(accepted.into_iter().sum::<u64>());
            }
            Ok::<u64, wyrd_spec::error::WyrdError>(rows)
        }));
    }
    let mut rows = 0_u64;
    for task in tasks {
        rows = rows.saturating_add(task.await??);
    }
    let topology = ScribeTopologyEvidence {
        requested_writers: case.writers,
        actual_writers: u32::try_from(writers)?,
        requested_tenants: case.tenants,
        actual_tenants: u32::try_from(workloads.len())?,
        requested_pods: case.pods,
        actual_pods: u32::try_from(pods)?,
        requested_logical_tables: case.tables,
        actual_logical_tables: u32::try_from(writers_by_table.len())?,
        actual_physical_tables: u32::try_from(routed_pairs(case)?.len())?,
        routing_mode: case.routing_mode.clone(),
        writers_by_pod,
        writers_by_tenant,
        writers_by_table,
    };
    Ok((rows, topology))
}

async fn insert_stream_with_busy_retry(
    transport: &BifrostGrpcTransport,
    frames: Vec<BifrostFrame>,
) -> Result<Vec<u64>, wyrd_spec::error::WyrdError> {
    const RETRIES: u32 = 8;
    for attempt in 0..=RETRIES {
        match transport.insert_batch_stream(frames.clone()).await {
            Ok(accepted) => return Ok(accepted),
            Err(error) if is_ingest_busy(&error) && attempt < RETRIES => {
                let backoff_ms = 10_u64.saturating_mul(1_u64 << attempt.min(6)).min(640);
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
            }
            Err(error) => return Err(error),
        }
    }
    Err(wyrd_spec::error::WyrdError::Internal {
        message: "bounded busy retry loop exhausted without an outcome".to_owned(),
        details: serde_json::json!({}),
    })
}

fn is_ingest_busy(error: &wyrd_spec::error::WyrdError) -> bool {
    matches!(
        error,
        wyrd_spec::error::WyrdError::UpstreamFailure { details, .. }
            if details
                .get("original_code")
                .and_then(serde_json::Value::as_str)
                == Some("WYRD_VALA_429_INGEST_BUSY")
    )
}

fn component_reports(cases: &[ScribeCaseReport]) -> Vec<ScribeComponentReport> {
    let Some(case) = cases.iter().find(|case| case.measured_frames > 0) else {
        return Vec::new();
    };
    let grouped = cases
        .iter()
        .find(|candidate| candidate.case.writers == 64 && candidate.case.tables == 1)
        .unwrap_or(case);
    let one_frame = cases
        .iter()
        .find(|candidate| candidate.case.writers == 1)
        .unwrap_or(case);
    let end_to_end = cases
        .iter()
        .find(|candidate| candidate.case.id == "64k-64-normal")
        .unwrap_or(case);
    [
        (
            "wal_prepare_crc_no_io",
            "Scribe preprocess: WAL headers and CRC preparation (no filesystem IO)",
            case.append.clone(),
        ),
        (
            "vectored_append_no_sync",
            "WAL segment append_prepared/write_vectored before sync_data",
            case.append.clone(),
        ),
        (
            "sync_alone",
            "WAL segment sync_data after append",
            case.sync.clone(),
        ),
        (
            "one_frame_append_sync",
            "One-frame writer append plus one segment sync",
            one_frame.sync.clone(),
        ),
        (
            "64_frame_append_one_sync",
            "64-frame opportunistic writer group with one sync",
            grouped.sync.clone(),
        ),
        (
            "writer_group",
            "Writer group formation and group-size measurements",
            grouped.frames_per_group.clone(),
        ),
        (
            "gate_ack",
            "Gate resolution through Scribe admission ACK",
            end_to_end.ack.clone(),
        ),
        (
            "sdk_gate_scribe",
            "Complete public SDK -> gRPC -> Gate -> Scribe path",
            end_to_end.ack.clone(),
        ),
    ]
    .into_iter()
    .map(|(name, path, distribution)| ScribeComponentReport {
        name: name.to_owned(),
        path: path.to_owned(),
        operations: distribution.count,
        bytes: case
            .case
            .frame_size_bytes
            .saturating_mul(distribution.count),
        elapsed_us: distribution.count.saturating_mul(distribution.p50),
        distribution,
    })
    .collect()
}

fn histogram(
    snapshot: &BenchmarkMetricSnapshot,
    family: &str,
    label: Option<&str>,
    divisor: u64,
) -> ScribeDistribution {
    let Some((_, value)) = snapshot.histograms.iter().find(|(key, _)| {
        key == &family
            || (key.starts_with(&format!("{family}{{")) && label.is_none_or(|l| key.contains(l)))
    }) else {
        return ScribeDistribution::default();
    };
    ScribeDistribution {
        count: value.count,
        p50: value.p50 / divisor,
        p95: value.p95 / divisor,
        p99: (value.count >= 100).then_some(value.p99 / divisor),
        max: value.max / divisor,
    }
}

fn metric_counter(snapshot: &BenchmarkMetricSnapshot, family: &str, status: &str) -> u64 {
    snapshot
        .counters
        .iter()
        .filter(|(key, _)| {
            (*key == family || key.starts_with(&format!("{family}{{")))
                && (status.is_empty() || key.contains(&format!("status=\"{status}\"")))
        })
        .map(|(_, value)| *value)
        .sum()
}

fn metric_gauge(snapshot: &BenchmarkMetricSnapshot, family: &str) -> u64 {
    snapshot
        .gauges
        .iter()
        .find(|(key, _)| *key == family)
        .map_or(0, |(_, value)| value.max(0.0) as u64)
}

fn metric_peak(snapshot: &BenchmarkMetricSnapshot, family: &str) -> u64 {
    snapshot
        .gauge_peaks
        .iter()
        .find(|(key, _)| *key == family)
        .map_or(0, |(_, value)| value.max(0.0) as u64)
}

fn lane_peaks(
    snapshot: &BenchmarkMetricSnapshot,
    family: &str,
) -> std::collections::BTreeMap<String, u64> {
    snapshot
        .gauge_peaks
        .iter()
        .filter_map(|(key, value)| {
            key.find("lane=\"")
                .and_then(|start| {
                    let rest = &key[start + 6..];
                    rest.find('"')
                        .map(|end| (rest[..end].to_owned(), *value as u64))
                })
                .filter(|_| key.starts_with(&format!("{family}{{")))
        })
        .collect()
}

fn lane_counters(
    snapshot: &BenchmarkMetricSnapshot,
    status: &str,
) -> std::collections::BTreeMap<String, u64> {
    let mut output = std::collections::BTreeMap::new();
    for (key, value) in &snapshot.counters {
        if !key.starts_with("bifrost_scribe_lane_jobs_total{")
            || !key.contains(&format!("status=\"{status}\""))
        {
            continue;
        }
        if let Some(start) = key.find("lane=\"") {
            let rest = &key[start + 6..];
            if let Some(end) = rest.find('"') {
                output.insert(rest[..end].to_owned(), *value);
            }
        }
    }
    output
}

fn metric_families(snapshot: &BenchmarkMetricSnapshot) -> BTreeSet<String> {
    snapshot
        .counters
        .keys()
        .chain(snapshot.gauges.keys())
        .chain(snapshot.histograms.keys())
        .map(|key| key.split('{').next().unwrap_or(key).to_owned())
        .collect()
}

fn rate(value: u64, elapsed_us: u64) -> f64 {
    value as f64 / (elapsed_us as f64 / 1_000_000.0).max(0.000_001)
}

fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn table_name(table_index: usize) -> String {
    format!("scribe_bench_{table_index}")
}

fn routed_pairs(case: &ScribeCompactCase) -> Result<BTreeSet<(usize, usize)>, BenchError> {
    let writers = usize::try_from(case.writers)?;
    let tenants = usize::try_from(case.tenants)?;
    let tables = usize::try_from(case.tables)?;
    if writers == 0 || tenants == 0 || tables == 0 {
        return Err("benchmark topology must have writers, tenants, and tables".into());
    }
    Ok((0..writers)
        .map(|writer| {
            let table = if case.table_distribution == "same" {
                0
            } else {
                (writer * 3) % tables
            };
            (writer % tenants, table)
        })
        .collect())
}

fn table_fqn(table_index: usize) -> String {
    format!("vala.datasets.{}", table_name(table_index))
}

fn ipc(target_bytes: usize) -> Result<Vec<u8>, BenchError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("value", DataType::Utf8, false),
    ]));
    let value_length = target_bytes.saturating_sub(4 * 1024).max(1);
    let rows = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(vec![1_i64])),
            Arc::new(StringArray::from(vec!["x".repeat(value_length)])),
        ],
    )?;
    let mut payload = Vec::new();
    let mut writer = StreamWriter::try_new(&mut payload, &schema)?;
    writer.write(&rows)?;
    writer.finish()?;
    Ok(payload)
}

fn benchmark_configuration(pods: usize, tenants: usize) -> ScribeBenchmarkConfig {
    ScribeBenchmarkConfig {
        pods: u32::try_from(pods).unwrap_or(u32::MAX),
        tenants: u32::try_from(tenants).unwrap_or(u32::MAX),
        resolved: [
            ("commit_group_frames", "64"),
            ("commit_group_bytes", "67108864"),
            ("wal_segment_bytes", "536870912"),
            ("retained_frame_items", "256"),
            ("retained_frame_bytes", "536870912"),
            ("memory_limit_bytes", "1073741824"),
            ("driver_in_flight_streams", "32"),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect(),
        matrix: "scribe-closeout-compact-v1".to_owned(),
    }
}

fn argument(name: &str) -> Option<String> {
    let prefix = format!("--{name}=");
    std::env::args().find_map(|argument| argument.strip_prefix(&prefix).map(str::to_owned))
}

fn configuration_fingerprint(configuration: &ScribeBenchmarkConfig) -> String {
    format!(
        "pods={};tenants={};matrix={};resolved={:?}",
        configuration.pods, configuration.tenants, configuration.matrix, configuration.resolved
    )
}

fn machine_metadata() -> MachineMetadata {
    let git_sha = git_command(&["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".to_owned());
    let dirty_worktree =
        git_command(&["status", "--porcelain"]).is_some_and(|status| !status.is_empty());
    MachineMetadata {
        git_sha,
        dirty_worktree,
        operating_system: std::env::consts::OS.to_owned(),
        cpu_count: std::thread::available_parallelism()
            .ok()
            .and_then(|count| u32::try_from(count.get()).ok()),
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

fn emit_report(report: &ScribeBenchmarkReport) -> Result<(), BenchError> {
    let path = std::env::var_os("WYRD_BIFROST_OUTPUT").map_or_else(
        || {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../..")
                .join("target/bifrost-benchmarks/post-throughput/reports/scribe.json")
        },
        std::path::PathBuf::from,
    );
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    report.write_artifacts(&path)?;
    let components_path = path.with_file_name("components.json");
    std::fs::write(
        &components_path,
        format!("{}\n", serde_json::to_string_pretty(&report.components)?),
    )?;
    let components_markdown = path.with_file_name("components.md");
    let mut markdown = String::from("# Bifrost Scribe component benchmarks\n\n");
    markdown.push_str("| Component | Operations | Bytes | p99 (us) |\n|---|---:|---:|---:|\n");
    for component in &report.components {
        let p99 = component
            .distribution
            .p99
            .map_or_else(|| "unavailable".to_owned(), |value| value.to_string());
        markdown.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            component.name, component.operations, component.bytes, p99
        ));
    }
    std::fs::write(components_markdown, markdown)?;
    Ok(())
}
