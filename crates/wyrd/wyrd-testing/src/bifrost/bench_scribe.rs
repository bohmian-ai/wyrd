//! Real Scribe benchmark execution owned by the reusable Bifrost harness.

use std::sync::Arc;
use std::time::Instant;
use tokio::task::JoinSet;

use crate::Bootstrap;
use crate::bifrost::BifrostHarness;
use arrow::array::{Int64Array, StringArray, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use bytes::Bytes;
use vala_bifrost_redux::bench_support::WalBenchSupport;
use vala_bifrost_redux::catalog::TableRef;
use vala_bifrost_redux::contracts::{Scribe, ScribeAppend};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::schema::fingerprint::SchemaFingerprint;
use vala_sdk::{BifrostFrame, BifrostGrpcTransport};
use wyrd_bench::{
    BenchmarkReadiness, BifrostReportEnvelope, BifrostScenario, DurableAckReport, DurableAckSample,
    ScribeComponentReport, ScribeDistribution, summarize_durable_acks,
};
use wyrd_client::WyrdClient;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_runtime::{Principal, PrincipalKind, permission::PermissionSet};
use wyrd_spec::auth::PrincipalId;
use wyrd_spec::request_id::RequestId;

type BenchError = Box<dyn std::error::Error + Send + Sync>;

#[derive(serde::Serialize)]
struct ScribeReport {
    ack: DurableAckReport,
    preflight_sync_p99_us: u64,
    warmup_durable_rows: u64,
    published_rows: u64,
    max_in_flight: u32,
    topology_verified: bool,
    exact_rows_verified: bool,
}

/// Run one real Scribe scenario and emit only durable ACK samples.
pub async fn run(scenario: BifrostScenario) -> Result<(), BenchError> {
    let mode = std::env::var("WYRD_BIFROST_BENCH_MODE").unwrap_or_default();
    if mode == "components" {
        return run_components(scenario).await;
    }
    let preflight_sync_p99_us = measure_sync_preflight()?;
    if mode == "preflight" || preflight_sync_p99_us > 3_000 {
        let mut envelope = BifrostReportEnvelope::new(
            scenario,
            if preflight_sync_p99_us <= 3_000 {
                BenchmarkReadiness::Ready
            } else {
                BenchmarkReadiness::Unsupported
            },
        );
        if preflight_sync_p99_us > 3_000 {
            envelope.failures.push(format!(
                "WAL sync_data p99 {preflight_sync_p99_us} us exceeds the qualified-volume limit of 3000 us"
            ));
        }
        let report = ScribeReport {
            ack: DurableAckReport {
                envelope,
                durable_samples: 0,
                durable_rows: 0,
                ack_p99_us: 0,
                verified: preflight_sync_p99_us <= 3_000,
            },
            preflight_sync_p99_us,
            warmup_durable_rows: 0,
            published_rows: 0,
            max_in_flight: 0,
            topology_verified: false,
            exact_rows_verified: false,
        };
        emit_report(&report)?;
        return Ok(());
    }
    let harness = BifrostHarness::start(
        usize::try_from(scenario.pods)?,
        usize::try_from(scenario.tenants)?,
    )
    .await?;
    let warmup = run_phase(
        &harness,
        &scenario,
        std::time::Duration::from_secs(u64::from(scenario.warmup_seconds)),
        true,
    )
    .await?;
    let (samples, max_in_flight) = run_phase(
        &harness,
        &scenario,
        std::time::Duration::from_secs(u64::from(scenario.measured_seconds)),
        true,
    )
    .await?;

    harness.force_seal_all().await?;
    let drained = harness.is_drained()?;
    let topology_verified = harness.inspection_snapshots()?.into_iter().all(|snapshot| {
        snapshot.shard_task_count == 16
            && snapshot.shard_channel_count == 16
            && snapshot.open_wal_stream_count <= 16
            && snapshot.memory_by_shard.iter().copied().sum::<usize>()
                == snapshot.total_accounted_memory
            && snapshot
                .memory_by_bucket
                .iter()
                .map(|bucket| bucket.writable_bytes + bucket.immutable_bytes)
                .sum::<usize>()
                == snapshot.memory_by_category[4] + snapshot.memory_by_category[5]
    });
    let warmup_durable_rows = warmup
        .0
        .iter()
        .filter(|sample| sample.durable && sample.response_id_matches)
        .map(|sample| sample.rows)
        .fold(0, u64::saturating_add);
    let measured_durable_rows = samples
        .iter()
        .filter(|sample| sample.durable && sample.response_id_matches)
        .map(|sample| sample.rows)
        .fold(0, u64::saturating_add);
    let expected_published_rows = warmup_durable_rows.saturating_add(measured_durable_rows);
    let mut published_rows = 0_u64;
    for tenant in harness.tenants() {
        let rows: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(row_count), 0)::bigint
               FROM vala.file_list
              WHERE data_tenant_id = $1
                AND namespace = 'vala.bifrost'
                AND table_name LIKE 'bifrost_bench_events_%'",
        )
        .bind(tenant.as_uuid())
        .fetch_one(harness.cluster().pg_fixture().platform_admin_pool())
        .await?;
        published_rows = published_rows.saturating_add(u64::try_from(rows).unwrap_or(0));
    }
    let exact_rows_verified = published_rows == expected_published_rows;
    let mut ack = summarize_durable_acks(&scenario, &samples);
    if warmup.0.is_empty() && ack.envelope.scenario.warmup_seconds > 0 {
        ack.envelope.readiness = BenchmarkReadiness::NotReady;
        ack.envelope
            .failures
            .push("warmup produced no completed durable ACKs".to_owned());
    }
    if samples.len() < usize::try_from(ack.envelope.scenario.verify_samples)? {
        ack.envelope.readiness = BenchmarkReadiness::NotReady;
        ack.envelope
            .failures
            .push("measured samples did not reach the verification minimum".to_owned());
    }
    if max_in_flight > ack.envelope.scenario.max_in_flight {
        ack.envelope.readiness = BenchmarkReadiness::NotReady;
        ack.envelope
            .failures
            .push("in-flight bound was exceeded".to_owned());
    }
    ack.verified &= drained && topology_verified && exact_rows_verified;
    if !drained {
        ack.envelope.readiness = BenchmarkReadiness::NotReady;
        ack.envelope
            .failures
            .push("harness did not drain writable or pending generations".to_owned());
    }
    if !topology_verified {
        ack.envelope.readiness = BenchmarkReadiness::NotReady;
        ack.envelope
            .failures
            .push("fixed-shard inspection did not reconcile".to_owned());
    }
    if !exact_rows_verified {
        ack.envelope.readiness = BenchmarkReadiness::NotReady;
        ack.envelope
            .failures
            .push("published rows did not match durable ACK rows".to_owned());
    }
    let report = ScribeReport {
        ack,
        preflight_sync_p99_us,
        warmup_durable_rows,
        published_rows,
        max_in_flight,
        topology_verified,
        exact_rows_verified,
    };
    emit_report(&report)?;
    harness.shutdown().await?;
    if !report.ack.verified || !report.ack.envelope.failures.is_empty() {
        return Err("Scribe benchmark did not prove durable ACK verification".into());
    }
    Ok(())
}

async fn run_phase(
    harness: &BifrostHarness,
    scenario: &BifrostScenario,
    duration: std::time::Duration,
    collect_samples: bool,
) -> Result<(Vec<DurableAckSample>, u32), BenchError> {
    if duration.is_zero() {
        return Ok((Vec::new(), 0));
    }
    let request_period = std::time::Duration::from_nanos(
        (1_000_000_000_u128
            .saturating_mul(u128::from(scenario.items_per_request))
            .div_ceil(u128::from(scenario.target_items_per_second)))
        .max(1)
        .try_into()?,
    );
    let mut interval = tokio::time::interval(request_period);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let deadline = Instant::now()
        .checked_add(duration)
        .ok_or("Scribe benchmark deadline overflow")?;
    let mut sequence = 0_u64;
    let mut tasks = JoinSet::new();
    let mut samples = Vec::new();
    let mut max_in_flight = 0_u32;

    while Instant::now() < deadline {
        interval.tick().await;
        if Instant::now() >= deadline {
            break;
        }
        while let Some(result) = tasks.try_join_next() {
            let sample = result??;
            if collect_samples {
                samples.push(sample);
            }
        }
        if tasks.len() >= usize::try_from(scenario.max_in_flight)?
            && let Some(result) = tasks.join_next().await
        {
            let sample = result??;
            if collect_samples {
                samples.push(sample);
            }
        }
        let tenant_index = usize::try_from(sequence)? % harness.tenants().len();
        let tenant = harness.tenants()[tenant_index];
        let table = TableRef::new(
            BifrostNamespace::Bifrost,
            format!(
                "bifrost_bench_events_{}",
                sequence % u64::from(scenario.tables)
            ),
        );
        let scribe = Arc::clone(
            harness
                .scribes()
                .get(tenant_index % harness.scribes().len())
                .ok_or("Scribe benchmark has no Scribe owner")?,
        );
        let rows = make_batch(usize::try_from(scenario.items_per_request)?, sequence)?;
        tasks.spawn(append_one(scribe, tenant, table, rows));
        max_in_flight = max_in_flight.max(u32::try_from(tasks.len())?);
        sequence = sequence.wrapping_add(1);
    }
    while let Some(result) = tasks.join_next().await {
        let sample = result??;
        if collect_samples {
            samples.push(sample);
        }
    }
    Ok((samples, max_in_flight))
}

async fn append_one(
    scribe: Arc<vala_bifrost_redux::scribe::ScribeImpl>,
    tenant: wyrd_spec::ids::DataTenantId,
    table: TableRef,
    rows: RecordBatch,
) -> Result<DurableAckSample, BenchError> {
    let fingerprint = SchemaFingerprint::from_arrow_schema(rows.schema().as_ref());
    let batch_id = uuid::Uuid::now_v7();
    let started = Instant::now();
    let result = scribe
        .append_durable(ScribeAppend {
            principal: Principal {
                id: PrincipalId::new(uuid::Uuid::now_v7()),
                kind: PrincipalKind::User,
                tenant_id: tenant,
                roles: Vec::new(),
                effective_permissions: PermissionSet::new(),
            },
            table,
            rows,
            schema_fingerprint: fingerprint,
            request_id: RequestId::now_v7(),
            batch_id,
            measured_wire_bytes: 0,
        })
        .await;
    let latency_us = u64::try_from(started.elapsed().as_micros())?;
    match result {
        Ok(ack) => Ok(DurableAckSample {
            response_id_matches: ack.batch_id == batch_id,
            durable: true,
            latency_us,
            rows: ack.rows_accepted,
        }),
        Err(_) => Ok(DurableAckSample {
            response_id_matches: false,
            durable: false,
            latency_us,
            rows: 0,
        }),
    }
}

const COMPONENT_WARMUP: usize = 100;
const COMPONENT_SAMPLES: usize = 1_000;

fn measure_sync_preflight() -> Result<u64, BenchError> {
    let temp_dir = tempfile::tempdir()?;
    let tenant = wyrd_spec::DataTenantId::new_v7();
    let support = WalBenchSupport::new(temp_dir.path(), tenant)?;
    let audit = b"{}";
    let data = vec![b'x'; 64];
    for index in 0..COMPONENT_WARMUP {
        let fixture = support.prepare(batch_id(index), audit, &data)?;
        let _ = support.append_and_sync(fixture)?;
    }
    let mut samples = Vec::with_capacity(COMPONENT_SAMPLES);
    for index in COMPONENT_WARMUP..(COMPONENT_WARMUP + COMPONENT_SAMPLES) {
        let fixture = support.prepare(batch_id(index), audit, &data)?;
        let _ = support.append_no_sync(fixture)?;
        let started = Instant::now();
        support.sync_alone()?;
        samples.push(elapsed_us(started));
    }
    samples.sort_unstable();
    Ok(percentile(&samples, 99))
}

#[derive(serde::Serialize)]
struct ScribeComponentsReport {
    envelope: BifrostReportEnvelope,
    components: Vec<ScribeComponentReport>,
    topology_verified: bool,
    exact_rows_verified: bool,
    published_rows: i64,
}

async fn run_components(scenario: BifrostScenario) -> Result<(), BenchError> {
    let harness = BifrostHarness::start(1, 1).await?;
    let server = harness
        .cluster()
        .server(0)
        .ok_or("missing benchmark server")?;
    let components = measure_wal_components()?
        .into_iter()
        .chain(measure_public_components(&harness, harness.tenants()[0]).await?)
        .collect::<Vec<_>>();

    server.flush_bifrost().await?;
    let server_snapshot = server.scribe_inspection_snapshot()?;
    let topology_verified = server_snapshot.shard_task_count == 16
        && server_snapshot.shard_channel_count == 16
        && server_snapshot.open_wal_stream_count <= 16
        && server_snapshot.memory_by_shard.iter().sum::<usize>()
            == server_snapshot.total_accounted_memory
        && server_snapshot
            .memory_by_bucket
            .iter()
            .map(|bucket| bucket.writable_bytes + bucket.immutable_bytes)
            .sum::<usize>()
            == server_snapshot.memory_by_category[4] + server_snapshot.memory_by_category[5];
    let tenant = harness.tenants()[0];
    let mut conn = harness.tenant_conn(tenant).await?;
    let published_rows: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(row_count), 0)::bigint
           FROM vala.file_list
          WHERE data_tenant_id = $1
            AND namespace = 'vala.bifrost'
            AND table_name = 'bifrost_component_events'",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await?;
    conn.commit().await?;
    let exact_rows_verified = published_rows == 2_100;
    let mut envelope = BifrostReportEnvelope::new(scenario, BenchmarkReadiness::Ready);
    if !topology_verified || !exact_rows_verified {
        envelope.readiness = BenchmarkReadiness::NotReady;
        envelope
            .failures
            .push("component harness did not drain or reconcile topology".to_owned());
    }
    let report = ScribeComponentsReport {
        envelope,
        components,
        topology_verified,
        exact_rows_verified,
        published_rows,
    };
    let path = std::env::var_os("WYRD_BIFROST_REPORT").map_or_else(
        || repository_report_path("components.json"),
        std::path::PathBuf::from,
    );
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(&report)?),
    )?;
    harness.shutdown().await?;
    if report.envelope.readiness != BenchmarkReadiness::Ready {
        return Err("Scribe component benchmark verification failed".into());
    }
    Ok(())
}

fn measure_wal_components() -> Result<Vec<ScribeComponentReport>, BenchError> {
    let tenant = wyrd_spec::DataTenantId::new_v7();
    let audit = b"{}";
    let data = vec![b'x'; 64];
    let fixture_bytes = u64::try_from(audit.len() + data.len())?;

    let prepare_dir = tempfile::tempdir()?;
    let prepare = WalBenchSupport::new(prepare_dir.path(), tenant)?;
    let mut prepare_samples = Vec::with_capacity(COMPONENT_SAMPLES);
    for index in 0..(COMPONENT_WARMUP + COMPONENT_SAMPLES) {
        let started = Instant::now();
        let fixture = prepare.prepare(batch_id(index), audit, &data)?;
        let _ = fixture.crc32();
        if index >= COMPONENT_WARMUP {
            prepare_samples.push(elapsed_us(started));
        }
    }

    let append_dir = tempfile::tempdir()?;
    let append = WalBenchSupport::new(append_dir.path(), tenant)?;
    let mut append_samples = Vec::with_capacity(COMPONENT_SAMPLES);
    let mut append_bytes = 0_u64;
    for index in 0..(COMPONENT_WARMUP + COMPONENT_SAMPLES) {
        let fixture = append.prepare(batch_id(index), audit, &data)?;
        let started = Instant::now();
        let evidence = append.append_no_sync(fixture)?;
        if index >= COMPONENT_WARMUP {
            append_samples.push(elapsed_us(started));
            append_bytes = append_bytes.saturating_add(evidence.bytes);
        }
    }
    append.sync_alone()?;

    let sync_dir = tempfile::tempdir()?;
    let sync = WalBenchSupport::new(sync_dir.path(), tenant)?;
    let mut sync_samples = Vec::with_capacity(COMPONENT_SAMPLES);
    for index in 0..COMPONENT_SAMPLES {
        let fixture = sync.prepare(batch_id(index), audit, &data)?;
        let _ = sync.append_no_sync(fixture)?;
        let started = Instant::now();
        sync.sync_alone()?;
        sync_samples.push(elapsed_us(started));
    }

    let one_dir = tempfile::tempdir()?;
    let one = WalBenchSupport::new(one_dir.path(), tenant)?;
    let mut one_samples = Vec::with_capacity(COMPONENT_SAMPLES);
    let mut one_bytes = 0_u64;
    for index in 0..(COMPONENT_WARMUP + COMPONENT_SAMPLES) {
        let fixture = one.prepare(batch_id(index), audit, &data)?;
        let started = Instant::now();
        let evidence = one.append_and_sync(fixture)?;
        if index >= COMPONENT_WARMUP {
            one_samples.push(elapsed_us(started));
            one_bytes = one_bytes.saturating_add(evidence.bytes);
        }
    }

    let group_dir = tempfile::tempdir()?;
    let group = WalBenchSupport::new(group_dir.path(), tenant)?;
    let mut group_samples = Vec::with_capacity(COMPONENT_SAMPLES);
    let mut group_bytes = 0_u64;
    for group_index in 0..(COMPONENT_WARMUP + COMPONENT_SAMPLES) {
        let fixtures = (0..64_usize)
            .map(|frame| {
                group.prepare(
                    batch_id(group_index.saturating_mul(64).saturating_add(frame)),
                    audit,
                    &data,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let started = Instant::now();
        let evidence = group.append_group_one_sync(fixtures)?;
        if evidence.frames != 64 || evidence.fsyncs != 1 {
            return Err("64-frame WAL group did not issue one sync".into());
        }
        if group_index >= COMPONENT_WARMUP {
            group_samples.push(elapsed_us(started));
            group_bytes = group_bytes.saturating_add(evidence.bytes);
        }
    }

    Ok(vec![
        component_report(
            "wal_prepare_crc_no_io",
            "production WAL preparation",
            fixture_bytes,
            prepare_samples,
            fixture_bytes.saturating_mul(COMPONENT_SAMPLES as u64),
        ),
        component_report(
            "wal_append_no_sync",
            "production WAL append before sync_data",
            fixture_bytes,
            append_samples,
            append_bytes,
        ),
        component_report(
            "sync_alone",
            "production WAL sync_data",
            fixture_bytes,
            sync_samples,
            sync.bytes_on_disk(),
        ),
        component_report(
            "one_frame_append_sync",
            "production one-frame append plus sync",
            fixture_bytes,
            one_samples,
            one_bytes,
        ),
        component_report(
            "64_frame_append_one_sync",
            "production 64-frame grouped append plus sync",
            fixture_bytes.saturating_mul(64),
            group_samples,
            group_bytes,
        ),
    ])
}

async fn measure_public_components(
    harness: &BifrostHarness,
    tenant: wyrd_spec::DataTenantId,
) -> Result<Vec<ScribeComponentReport>, BenchError> {
    let server = harness
        .cluster()
        .server(0)
        .ok_or("missing benchmark server")?;
    let table_name = "bifrost_component_events";
    let table = TableRef::new(BifrostNamespace::Bifrost, table_name);
    let catalog = server
        .state()
        .bifrost_redux
        .as_ref()
        .ok_or("missing Redux benchmark catalog")?;
    catalog
        .create_table(vala_bifrost_redux::catalog::CreateTableRequest {
            table,
            user_fields: vec![
                Field::new("id", DataType::Int64, false),
                Field::new("value", DataType::Utf8, false),
            ],
            tenant,
            audit: None,
        })
        .await?;
    let bootstrap = server
        .bootstrap_service_in_tenant(tenant, "bifrost-component-writer", &["admin"])
        .await?;
    let api_key = match bootstrap {
        Bootstrap::Machine { api_key, .. } => api_key,
        Bootstrap::User { .. } => return Err("component bootstrap returned a user".into()),
    };
    let client = WyrdClient::with_config(ClientConfig {
        grpc: GrpcConfig {
            endpoint: server.grpc_url().ok_or("missing benchmark gRPC URL")?,
            connect_retries: 0,
            ..GrpcConfig::default()
        },
        http: HttpConfig {
            base_url: server
                .base_url()
                .ok_or("missing benchmark HTTP URL")?
                .to_owned(),
            ..HttpConfig::default()
        },
        api_key: Some(api_key),
        ..ClientConfig::default()
    })?;
    let transport = BifrostGrpcTransport::connect(&client).await?;
    let payload = component_ipc()?;
    let fqn = format!("vala.bifrost.{table_name}");
    for index in 0..COMPONENT_WARMUP {
        transport
            .send_frame(BifrostFrame {
                table: fqn.clone(),
                batch_id: uuid::Uuid::now_v7().into_bytes(),
                arrow_ipc: payload.clone(),
            })
            .await?;
        let _ = index;
    }
    let mut gate_samples = Vec::with_capacity(COMPONENT_SAMPLES);
    let started = Instant::now();
    for _ in 0..COMPONENT_SAMPLES {
        let sample_started = Instant::now();
        transport
            .send_frame(BifrostFrame {
                table: fqn.clone(),
                batch_id: uuid::Uuid::now_v7().into_bytes(),
                arrow_ipc: payload.clone(),
            })
            .await?;
        gate_samples.push(elapsed_us(sample_started));
    }
    let gate_elapsed = started.elapsed();

    let mut sdk_samples = Vec::with_capacity(COMPONENT_SAMPLES);
    let started = Instant::now();
    for _ in 0..COMPONENT_SAMPLES {
        let sample_started = Instant::now();
        let frame = BifrostFrame {
            table: fqn.clone(),
            batch_id: uuid::Uuid::now_v7().into_bytes(),
            arrow_ipc: payload.clone(),
        };
        transport.send_frame(frame).await?;
        sdk_samples.push(elapsed_us(sample_started));
    }
    let sdk_elapsed = started.elapsed();
    let bytes = u64::try_from(payload.len())?.saturating_mul(COMPONENT_SAMPLES as u64);
    Ok(vec![
        component_report_with_elapsed(
            "gate_ack",
            "unary Gate to Scribe durable ACK",
            u64::try_from(payload.len())?,
            gate_samples,
            bytes,
            gate_elapsed,
        ),
        component_report_with_elapsed(
            "sdk_gate_scribe",
            "public SDK transport through Gate to Scribe",
            u64::try_from(payload.len())?,
            sdk_samples,
            bytes,
            sdk_elapsed,
        ),
    ])
}

fn component_ipc() -> Result<Bytes, BenchError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("value", DataType::Utf8, false),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(vec![1_i64])),
            Arc::new(StringArray::from(vec!["component"])),
        ],
    )?;
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, schema.as_ref())?;
    writer.write(&batch)?;
    writer.finish()?;
    Ok(Bytes::from(bytes))
}

fn batch_id(index: usize) -> [u8; 16] {
    let mut id = [0_u8; 16];
    id[..8].copy_from_slice(&(index as u64).to_le_bytes());
    id
}

fn component_report(
    name: &str,
    path: &str,
    fixture_bytes: u64,
    samples: Vec<u64>,
    bytes: u64,
) -> ScribeComponentReport {
    let elapsed_us = samples.iter().copied().sum::<u64>().max(1);
    component_report_with_elapsed(
        name,
        path,
        fixture_bytes,
        samples,
        bytes,
        std::time::Duration::from_micros(elapsed_us),
    )
}

fn component_report_with_elapsed(
    name: &str,
    path: &str,
    fixture_bytes: u64,
    mut samples: Vec<u64>,
    bytes: u64,
    elapsed: std::time::Duration,
) -> ScribeComponentReport {
    samples.sort_unstable();
    let operations = samples.len() as u64;
    let elapsed_us = u64::try_from(elapsed.as_micros())
        .unwrap_or(u64::MAX)
        .max(1);
    let seconds = elapsed_us as f64 / 1_000_000.0;
    ScribeComponentReport {
        name: name.to_owned(),
        path: path.to_owned(),
        provenance: "current Bifrost production seam".to_owned(),
        warmup_operations: COMPONENT_WARMUP as u64,
        fixture_bytes,
        operations,
        bytes,
        elapsed_us,
        operations_per_second: operations as f64 / seconds,
        mib_per_second: bytes as f64 / seconds / 1_048_576.0,
        distribution: ScribeDistribution {
            count: operations,
            p50: percentile(&samples, 50),
            p95: percentile(&samples, 95),
            p99: (operations >= 100).then(|| percentile(&samples, 99)),
            max: samples.last().copied().unwrap_or(0),
        },
    }
}

fn percentile(sorted: &[u64], percentile: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = (sorted.len().saturating_mul(percentile).saturating_add(99) / 100)
        .max(1)
        .min(sorted.len());
    sorted[rank - 1]
}

fn elapsed_us(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros())
        .unwrap_or(u64::MAX)
        .max(1)
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

fn emit_report(report: &ScribeReport) -> Result<(), BenchError> {
    let path = std::env::var_os("WYRD_BIFROST_REPORT").map_or_else(
        || repository_report_path("scribe.json"),
        std::path::PathBuf::from,
    );
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, format!("{}\n", serde_json::to_string_pretty(report)?))?;
    Ok(())
}

fn repository_report_path(file_name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join("target/bifrost-benchmarks/task16")
        .join(file_name)
}
