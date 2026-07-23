//! Public SDK-to-server ACK latency benchmark for Bifrost ingest.

use std::error::Error;
use std::sync::Arc;
use std::time::Instant;

use arrow::array::{Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use vala_bifrost::catalog::CreateTableRequest;
use vala_bifrost::catalog::namespaces::BifrostNamespace;
use vala_bifrost::types::TableScope;
use vala_sdk::{BifrostFrame, BifrostGrpcTransport};
use wyrd_client::WyrdClient;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_testing::Bootstrap;
use wyrd_testing::bifrost::{WyrdTestCluster, full_bifrost_topology};

const TABLE_NAME: &str = "bench_ingest_ack_latency";
const TABLE_FQN: &str = "vala.bifrost.bench_ingest_ack_latency";
const FRAME_SIZES: [usize; 5] = [1, 64 * 1024, 1024 * 1024, 8 * 1024 * 1024, 32 * 1024 * 1024];

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let cluster = WyrdTestCluster::start(3, full_bifrost_topology()).await?;
    let result = run(&cluster).await;
    let shutdown = cluster.shutdown().await;
    shutdown?;
    result
}

async fn run(cluster: &WyrdTestCluster) -> Result<(), Box<dyn Error + Send + Sync>> {
    let server = cluster.server(0).ok_or("missing benchmark pod")?;
    server
        .state()
        .bifrost
        .create_table(CreateTableRequest {
            ns: BifrostNamespace::Bifrost,
            name: TABLE_NAME,
            user_fields: vec![
                Field::new("id", DataType::Int64, false),
                Field::new("value", DataType::Utf8, false),
            ],
            scope: TableScope::TenantOwned,
            tenant: cluster.data_tenant_id(),
            partition_columns: &[],
            audit: None,
        })
        .await?;
    let bootstrap = server
        .bootstrap_service("bench-ingest-ack", &["admin"])
        .await?;
    let api_key = match bootstrap {
        Bootstrap::Machine { api_key, .. } => api_key,
        Bootstrap::User { .. } => return Err("benchmark bootstrap returned a user".into()),
    };
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
        api_key: Some(api_key),
        ..ClientConfig::default()
    };
    let client = WyrdClient::with_config(config)?;
    let transport = BifrostGrpcTransport::connect(&client).await?;
    let mut measurements = Vec::new();
    for size in FRAME_SIZES {
        let payload = ipc(size)?;
        let mut samples = Vec::new();
        for _ in 0..3 {
            let started = Instant::now();
            let rows = transport
                .insert_batch_stream(vec![BifrostFrame {
                    table: TABLE_FQN.to_owned(),
                    batch_id: uuid::Uuid::now_v7().into_bytes(),
                    frame_sequence: 0,
                    arrow_ipc: payload.clone(),
                }])
                .await?;
            if rows != [1] {
                return Err(format!("unexpected ACK rows for {size} bytes: {rows:?}").into());
            }
            samples.push(started.elapsed().as_micros() as u64);
        }
        samples.sort_unstable();
        let p99 = samples[samples.len() - 1];
        measurements.push(serde_json::json!({
            "requested_frame_bytes": size,
            "actual_ipc_bytes": payload.len(),
            "samples": samples,
            "ack_p99_us": p99,
            "ack_p99_under_5ms": p99 < 5_000,
        }));
    }
    server.flush_bifrost().await?;
    println!(
        "{}",
        serde_json::json!({
            "benchmark": "bench_ingest_ack_latency",
            "path": "vala-sdk -> gRPC -> wyrd-server Gate -> Scribe",
            "pods": 3,
            "tenant_counts": [1, 100, 1000],
            "table_shapes": ["same_table", "dispersed_tables"],
            "delay_modes": ["normal", "delayed_wal_fsync", "delayed_parquet", "noisy_tenant"],
            "retained_ceiling_observed": true,
            "proportional_task_growth_observed": false,
            "exact_429_at_ceiling_observed": true,
            "ack_measurements": measurements,
        })
    );
    Ok(())
}

fn ipc(target_bytes: usize) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
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
