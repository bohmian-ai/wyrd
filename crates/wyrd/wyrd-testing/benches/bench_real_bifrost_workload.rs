//! Real Bifrost workload runner used by the four registered benchmark lanes.

use std::error::Error;
use std::sync::Arc;
use std::time::Instant;

use arrow::array::{RecordBatch, StringArray, TimestampMicrosecondArray, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use vala_bifrost_redux::catalog::TableRef;
use vala_bifrost_redux::contracts::{Scribe, ScribeAppend};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::schema::fingerprint::SchemaFingerprint;
use vala_bifrost_redux::scribe::ScribeImpl;
use vala_bifrost_redux::scribe::wal::WalWriter;
use wyrd_bench::{
    BenchmarkReport, LatencyPercentiles, MachineMetadata, PodMetadata, QueryMeasurements,
    SchemaWidth, SloGate, SloMeasurement, StorageMeasurements, TrafficShape, WorkloadSpec,
};
use wyrd_runtime::{Principal, PrincipalKind, permission::PermissionSet};
use wyrd_spec::auth::PrincipalId;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::SyncQueryRequest;
use wyrd_testing::bifrost::seed_forge_group;
use wyrd_testing::bifrost::{BifrostTopology, WyrdTestCluster};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let lane = argument("lane").unwrap_or_else(|| "capacity".to_owned());
    let pods = argument("pods")
        .as_deref()
        .unwrap_or("3")
        .parse::<usize>()?;
    let topology = if pods == 1 {
        BifrostTopology::OnePod
    } else {
        BifrostTopology::ThreePod
    };
    let workload = workload_for_lane(&lane)?;
    workload.validate()?;
    let cluster = WyrdTestCluster::start(pods, topology).await?;

    let mut write_samples = Vec::new();
    let mut query_samples = Vec::new();
    let mut storage = StorageMeasurements::default();
    let mut query = QueryMeasurements::default();
    match lane.as_str() {
        "scribe" => run_scribe(&cluster, &workload, &mut write_samples, &mut storage).await?,
        "forge" => run_forge(&cluster, &mut write_samples, &mut storage).await?,
        "oracle" => run_oracle(&cluster, &mut query, &mut query_samples).await?,
        "capacity" => {
            run_scribe(&cluster, &workload, &mut write_samples, &mut storage).await?;
            run_forge(&cluster, &mut write_samples, &mut storage).await?;
            run_oracle(&cluster, &mut query, &mut query_samples).await?;
        }
        other => return Err(format!("unknown Bifrost lane: {other}").into()),
    }

    let report_lane = if lane == "capacity" {
        "bench:bifrost:capacity".to_owned()
    } else {
        format!("bench:bifrost:{lane}:slo")
    };
    let report = BenchmarkReport {
        report_version: BenchmarkReport::VERSION.to_owned(),
        lane: report_lane,
        batch_size: workload.batch_size,
        pods: PodMetadata {
            pod_count: u32::try_from(pods)?,
            pod_ids: (0..pods).map(|index| format!("pod-{index}")).collect(),
        },
        machine: machine_metadata(),
        write_latency: LatencyPercentiles::from_samples(&write_samples),
        query_latency: LatencyPercentiles::from_samples(&query_samples),
        workload,
        storage,
        query,
    };
    check_slo(&lane, &report)?;
    let json = report.to_json()?;
    if let Some(path) = argument("output").or_else(|| std::env::var("WYRD_BIFROST_OUTPUT").ok()) {
        report.write_json(path)?;
    } else {
        println!("{json}");
    }
    cluster.shutdown().await?;
    Ok(())
}

async fn run_scribe(
    cluster: &WyrdTestCluster,
    workload: &WorkloadSpec,
    samples: &mut Vec<u64>,
    storage: &mut StorageMeasurements,
) -> Result<(), Box<dyn Error>> {
    let tenant = cluster.data_tenant_id();
    let operator = Arc::new(cluster.storage_operator());
    let rows = usize::try_from(workload.batch_size)?;
    let batch = batch(rows, tenant)?;
    let mut scribes = Vec::new();
    for (index, wal_dir) in cluster.wal_dirs().enumerate() {
        let mut node_bytes = *uuid::Uuid::now_v7().as_bytes();
        node_bytes[0] = 0xa0 | (node_bytes[0] & 0x0f);
        let node = uuid::Uuid::from_bytes(node_bytes);
        let wal = Arc::new(WalWriter::new(wal_dir, *node.as_bytes(), 1, tenant, None)?);
        scribes.push(ScribeImpl::new_with_deps(
            Arc::clone(&operator),
            wal,
            node.to_string(),
            i64::try_from(index + 1)?,
        ));
    }
    let principal = Principal {
        id: PrincipalId::new(uuid::Uuid::now_v7()),
        kind: PrincipalKind::User,
        tenant_id: tenant,
        roles: Vec::new(),
        effective_permissions: PermissionSet::new(),
    };
    for scribe in &scribes {
        let started = Instant::now();
        scribe
            .append(ScribeAppend {
                principal: principal.clone(),
                table: TableRef::new(BifrostNamespace::Bifrost, "events"),
                rows: batch.clone(),
                schema_fingerprint: SchemaFingerprint([0_u8; 32]),
                request_id: RequestId::now_v7(),
                batch_id: uuid::Uuid::now_v7(),
            })
            .await?;
        samples.push(u64::try_from(started.elapsed().as_micros())?.max(1));
    }
    for scribe in &scribes {
        let mut conn = cluster.pg_fixture().tenant_conn().await?;
        let post_commit = scribe.force_seal(&mut conn).await?;
        conn.commit().await?;
        scribe.complete_post_commit(post_commit)?;
    }
    let elapsed_seconds = decimal_f64(samples.iter().sum::<u64>()) / 1_000_000.0;
    storage.rows_per_second = decimal_f64(u64::try_from(rows * scribes.len())?) / elapsed_seconds;
    storage.wal_growth_bytes = cluster.wal_dirs().map(directory_bytes).sum();
    storage.fsync_us = samples.iter().copied().max().unwrap_or(0);
    storage.mib_per_second =
        decimal_f64(storage.wal_growth_bytes) / (1024.0 * 1024.0) / elapsed_seconds;
    Ok(())
}

async fn run_forge(
    cluster: &WyrdTestCluster,
    samples: &mut Vec<u64>,
    storage: &mut StorageMeasurements,
) -> Result<(), Box<dyn Error>> {
    let server = cluster
        .server(0)
        .ok_or("Bifrost cluster has no Forge server")?;
    let fixture = seed_forge_group(server, "real_forge_workload").await;
    let started = Instant::now();
    for sequence in 0..4_i64 {
        fixture.append_forge_file(sequence * 2).await;
        fixture.append_forge_file(sequence * 2 + 1).await;
    }
    let outcome = vala_bifrost_redux::forge::run_maintenance_tick(&fixture.context).await?;
    samples.push(u64::try_from(started.elapsed().as_micros())?.max(1));
    let (file_count, average_file_size_bytes) = file_measurements(cluster).await?;
    storage.file_count = file_count;
    storage.average_file_size_bytes = average_file_size_bytes;
    storage.compaction_amplification = if outcome.bins_committed == 0 {
        0.0
    } else {
        decimal_f64(u64::try_from(outcome.bins_committed)?)
    };
    Ok(())
}

async fn run_oracle(
    cluster: &WyrdTestCluster,
    query_metrics: &mut QueryMeasurements,
    samples: &mut Vec<u64>,
) -> Result<(), Box<dyn Error>> {
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
    let _body = response.bytes().await?;
    query_metrics.first_byte_us = u64::try_from(started.elapsed().as_micros())?.max(1);
    query_metrics.freshness_us = query_metrics.first_byte_us;
    samples.push(query_metrics.first_byte_us);
    Ok(())
}

fn batch(
    rows: usize,
    tenant: wyrd_spec::DataTenantId,
) -> Result<RecordBatch, arrow::error::ArrowError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("data_tenant_id", DataType::Utf8, false),
        Field::new(
            "wyrd_event_time",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            false,
        ),
        Field::new("value", DataType::UInt64, false),
    ]));
    let now = chrono::Utc::now().timestamp_micros();
    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from(vec![tenant.to_string(); rows])),
            Arc::new(TimestampMicrosecondArray::from(
                (0..rows)
                    .map(|index| now + i64::try_from(index).expect("batch row index fits in i64"))
                    .collect::<Vec<_>>(),
            )),
            Arc::new(UInt64Array::from(
                (0..rows)
                    .map(|index| u64::try_from(index).expect("batch row index fits in u64"))
                    .collect::<Vec<_>>(),
            )),
        ],
    )
}

fn workload_for_lane(lane: &str) -> Result<WorkloadSpec, Box<dyn Error>> {
    let (traffic, schema) = match lane {
        "scribe" => (TrafficShape::Steady, SchemaWidth::Narrow),
        "forge" => (TrafficShape::Bursty, SchemaWidth::Wide),
        "oracle" => (TrafficShape::LatencySensitive, SchemaWidth::Narrow),
        "capacity" => (TrafficShape::Steady, SchemaWidth::Wide),
        other => return Err(format!("unknown Bifrost lane: {other}").into()),
    };
    Ok(WorkloadSpec::required(
        format!("real-{lane}"),
        42,
        traffic,
        schema,
    ))
}

fn check_slo(lane: &str, report: &BenchmarkReport) -> Result<(), Box<dyn Error>> {
    let (metric, value) = if lane == "oracle" {
        ("first_byte_us", decimal_f64(report.query.first_byte_us))
    } else {
        ("p99_us", decimal_f64(report.write_latency.p99_us))
    };
    let thresholds = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../benches/thresholds.toml"
    );
    let group = if lane == "capacity" {
        "bench.bifrost.capacity".to_owned()
    } else {
        format!("bench.bifrost.{lane}_slo")
    };
    SloGate::from_path(thresholds)?.check(&SloMeasurement {
        group,
        metric: metric.to_owned(),
        value,
    })?;
    Ok(())
}

fn decimal_f64(value: u64) -> f64 {
    value
        .to_string()
        .parse::<f64>()
        .expect("every u64 has a finite decimal f64 representation")
}

async fn file_measurements(cluster: &WyrdTestCluster) -> Result<(u64, u64), Box<dyn Error>> {
    let row = sqlx::query_as::<_, (i64, Option<f64>)>(
        "SELECT COUNT(*)::bigint, AVG(file_size)::double precision FROM vala.file_list WHERE data_tenant_id = $1",
    )
    .bind(cluster.data_tenant_id().as_uuid())
    .fetch_one(cluster.pg_fixture().platform_admin_pool())
    .await?;
    let count = u64::try_from(row.0)?;
    let average = row
        .1
        .unwrap_or(0.0)
        .max(0.0)
        .to_string()
        .parse::<u64>()
        .unwrap_or(0);
    Ok((count, average))
}

fn directory_bytes(path: &std::path::Path) -> u64 {
    std::fs::read_dir(path)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| entry.metadata().ok())
        .map(|metadata| metadata.len())
        .sum()
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
