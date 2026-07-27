//! Sustained public SDK Bifrost journey over three bound server pods.

use std::sync::Arc;

use arrow::array::{Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_sdk::{BifrostFrame, BifrostGrpcTransport};
use wyrd_client::WyrdClient;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_testing::Bootstrap;
use wyrd_testing::bifrost::{WyrdTestCluster, full_bifrost_topology};

const TABLE_PREFIX: &str = "sustained_ingest_events";

#[tokio::test]
#[ignore = "requires the real three-pod sustained Bifrost journey lane"]
async fn pg_bifrost_sustained_public_sdk_journey() {
    let cluster = WyrdTestCluster::start(3, full_bifrost_topology())
        .await
        .expect("three-pod Bifrost cluster");
    let result = run(&cluster).await;
    cluster.shutdown().await.expect("cluster shutdown");
    result.expect("sustained public Bifrost journey");
}

async fn run(cluster: &WyrdTestCluster) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut tenants = vec![cluster.data_tenant_id()];
    for index in 1..10 {
        tenants.push(
            cluster
                .add_tenant(&format!("sustained-tenant-{index}"))
                .await?,
        );
    }

    for tenant in &tenants {
        cluster
            .server(0)
            .ok_or("missing catalog pod")?
            .state()
            .bifrost_redux
            .as_ref()
            .ok_or("missing Redux catalog")?
            .create_table(CreateTableRequest {
                table: TableRef::new(BifrostNamespace::Bifrost, TABLE_PREFIX),
                user_fields: vec![
                    Field::new("id", DataType::Int64, false),
                    Field::new("value", DataType::Utf8, false),
                ],
                tenant: *tenant,
                audit: None,
            })
            .await?;
    }

    let payload = ipc();
    for (tenant_index, tenant) in tenants.iter().enumerate() {
        let table_fqn = format!("vala.bifrost.{TABLE_PREFIX}");
        let server = cluster
            .server(tenant_index % 3)
            .ok_or("missing sustained pod")?;
        let bootstrap = server
            .bootstrap_service_in_tenant(
                *tenant,
                &format!("sustained-writer-{tenant_index}"),
                &["admin"],
            )
            .await?;
        let api_key = match bootstrap {
            Bootstrap::Machine { api_key, .. } => api_key,
            Bootstrap::User { .. } => return Err("sustained bootstrap returned a user".into()),
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
        let transport = BifrostGrpcTransport::connect(&WyrdClient::with_config(config)?).await?;
        for _ in 0..4 {
            transport
                .send_frame(BifrostFrame {
                    table: table_fqn.clone(),
                    batch_id: uuid::Uuid::now_v7().into_bytes(),
                    arrow_ipc: payload.clone().into(),
                })
                .await?;
        }
    }

    for (tenant_index, tenant) in tenants.iter().enumerate() {
        cluster
            .server(tenant_index % 3)
            .ok_or("missing sustained flush pod")?
            .flush_bifrost_for_tenant(*tenant)
            .await?;
    }

    for tenant in tenants {
        let rows: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(row_count), 0)::bigint
               FROM vala.file_list
              WHERE data_tenant_id = $1 AND namespace = 'vala.bifrost' AND table_name = $2",
        )
        .bind(tenant.as_uuid())
        .bind(TABLE_PREFIX)
        .fetch_one(cluster.pg_fixture().platform_admin_pool())
        .await?;
        assert_eq!(
            rows, 8,
            "tenant data must remain isolated and duplicate-free"
        );
    }
    Ok(())
}

fn ipc() -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("value", DataType::Utf8, false),
    ]));
    let rows = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(vec![1_i64, 2_i64])),
            Arc::new(StringArray::from(vec!["sustained", "sustained"])),
        ],
    )
    .expect("valid sustained batch");
    let mut payload = Vec::new();
    let mut writer = StreamWriter::try_new(&mut payload, &schema).expect("IPC writer");
    writer.write(&rows).expect("IPC batch");
    writer.finish().expect("IPC finish");
    payload
}
