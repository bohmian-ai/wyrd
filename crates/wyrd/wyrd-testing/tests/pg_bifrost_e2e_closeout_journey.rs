//! Primary public Bifrost write journey.
//!
//! This is deliberately an ignored e2e target. The canonical journey lane
//! selects it explicitly with Postgres and a real three-pod server topology;
//! the fast family lanes do not claim coverage from this test.

use std::sync::Arc;
use std::time::Duration;

use arrow::array::{Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_sdk::{BifrostFrame, BifrostGrpcTransport, IngestTransport};
use wyrd_client::WyrdClient;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_spec::vala::api::SyncQueryRequest;
use wyrd_testing::Bootstrap;
use wyrd_testing::bifrost::{BifrostTopology, WyrdTestCluster, full_bifrost_topology};

const TABLE_NAME: &str = "closeout_events";
const TABLE_FQN: &str = "vala.bifrost.closeout_events";

#[tokio::test]
#[ignore = "requires the real three-pod Bifrost journey lane"]
async fn pg_bifrost_e2e_closeout_journey() {
    let cluster = WyrdTestCluster::start(3, full_bifrost_topology())
        .await
        .expect("three-pod WyrdTestCluster");
    let result = run_closeout_journey(&cluster).await;
    let shutdown = cluster.shutdown().await;
    shutdown.expect("three-pod cluster shutdown");
    result.expect("public Bifrost closeout journey");
}

#[tokio::test]
#[ignore = "requires the real Postgres-backed Bifrost journey lane"]
async fn pg_bifrost_e2e_closeout_journey_delayed_fsync_drains_without_loss() {
    let cluster = WyrdTestCluster::start_with_wal_sync_delay(
        1,
        BifrostTopology::OnePod,
        Duration::from_millis(10),
    )
    .await
    .expect("one-pod delayed-fsync WyrdTestCluster");
    let result = run_delayed_fsync_journey(&cluster).await;
    let shutdown = cluster.shutdown().await;
    shutdown.expect("delayed-fsync cluster shutdown");
    result.expect("delayed-fsync Bifrost closeout journey");
}

#[tokio::test]
#[ignore = "requires the real Postgres-backed Bifrost journey lane"]
async fn fresh_boot_provisions_redux_before_first_write() {
    let cluster = WyrdTestCluster::start(1, BifrostTopology::OnePod)
        .await
        .expect("fresh WyrdTestCluster boot");
    let server = cluster.server(0).expect("booted Bifrost server");
    let tenant = cluster.data_tenant_id();
    let redux = server
        .state()
        .bifrost_redux
        .as_ref()
        .expect("server boot provisions Redux catalog");
    redux
        .create_table(CreateTableRequest {
            table: TableRef::new(BifrostNamespace::Bifrost, TABLE_NAME),
            user_fields: vec![
                Field::new("id", DataType::Int64, false),
                Field::new("value", DataType::Utf8, false),
            ],
            tenant,
            audit: None,
        })
        .await
        .expect("first Redux table resolution after boot");
    let transport = bootstrap_transport(server, "fresh-boot-writer", &["admin"])
        .await
        .expect("first writer after boot");
    transport
        .insert_batch(TABLE_FQN, uuid::Uuid::now_v7().into_bytes(), ipc(&[1]))
        .await
        .expect("first Redux write after boot");
    server.flush_bifrost().await.expect("first write flush");
    let rows: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(row_count), 0)::bigint FROM vala.file_list
         WHERE data_tenant_id = $1 AND namespace = 'vala.bifrost' AND table_name = $2",
    )
    .bind(tenant.as_uuid())
    .bind(TABLE_NAME)
    .fetch_one(cluster.pg_fixture().operator_pool().pool())
    .await
    .expect("first Redux file-list read");
    assert_eq!(rows, 1);
    cluster.shutdown().await.expect("cluster shutdown");
}

/// Each cluster role keeps serving after the fixture's seed pool is exhausted.
///
/// # Panics
///
/// Panics when a running role shares the fixture app pool instead of the
/// process-local pool created for that simulated server.
#[tokio::test]
#[ignore = "requires the real Postgres-backed Bifrost journey lane"]
async fn cluster_role_pool_is_independent_from_fixture_pool() {
    let cluster = WyrdTestCluster::start(1, BifrostTopology::OnePod)
        .await
        .expect("one-pod cluster");
    let tenant = cluster.data_tenant_id();
    let fixture_pool = cluster.pg_fixture().app_pool().clone();
    fixture_pool.close().await;

    let server = cluster.server(0).expect("bound cluster role");
    assert!(fixture_pool.is_closed(), "fixture pool must be exhausted");
    assert!(
        !server.app_pool().is_closed(),
        "role must own a process-local app pool"
    );
    let conn = server
        .tenant_conn_for(tenant)
        .await
        .expect("role-local tenant connection after fixture pool closure");
    conn.commit().await.expect("role-local transaction commit");

    cluster.shutdown().await.expect("cluster shutdown");
}

/// A post-bind construction failure explicitly stops the listener and local pool.
///
/// # Panics
///
/// Panics when partial cluster startup leaves a bound listener or local pool
/// alive after the injected failure returns.
#[tokio::test]
#[ignore = "requires the real Postgres-backed Bifrost journey lane"]
async fn cluster_partial_start_failure_rolls_back_listener_and_pool() {
    WyrdTestCluster::verify_partial_start_rollback_for_test()
        .await
        .expect("partial startup rollback proof");
}

async fn run_closeout_journey(
    cluster: &WyrdTestCluster,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let tenant = cluster.data_tenant_id();
    let first = cluster.server(0).ok_or("missing first Bifrost pod")?;
    first
        .state()
        .bifrost_redux
        .as_ref()
        .ok_or("missing Redux catalog")?
        .create_table(CreateTableRequest {
            table: TableRef::new(BifrostNamespace::Bifrost, TABLE_NAME),
            user_fields: vec![
                Field::new("id", DataType::Int64, false),
                Field::new("value", DataType::Utf8, false),
            ],
            tenant,
            audit: None,
        })
        .await?;

    let admin = bootstrap_transport(first, "closeout-admin", &["admin"]).await?;
    let batch_id = uuid::Uuid::now_v7().into_bytes();
    admin.send_frame(frame(batch_id, &[1, 2])).await?;
    admin
        .send_frame(frame(uuid::Uuid::now_v7().into_bytes(), &[3, 4]))
        .await?;

    // Exercise symmetric traffic: every independently bound server receives a
    // frame through its own public SDK connection and server-owned Gate.
    for (pod, server) in cluster.servers().iter().enumerate() {
        let transport =
            bootstrap_transport(server, &format!("closeout-pod-{pod}"), &["admin"]).await?;
        let id = uuid::Uuid::now_v7().into_bytes();
        transport
            .insert_batch(TABLE_FQN, id, ipc(&[10 + pod as i64]))
            .await?;
        server.flush_bifrost().await?;
    }

    // A retry of an already admitted frame is idempotent at the durable slice
    // boundary. The duplicate is intentionally flushed after the first frame.
    let retry_id = uuid::Uuid::now_v7().into_bytes();
    let retry_frame = frame(retry_id, &[99]);
    admin
        .insert_batch(
            TABLE_FQN,
            retry_frame.batch_id,
            retry_frame.arrow_ipc.to_vec(),
        )
        .await?;
    admin
        .insert_batch(
            TABLE_FQN,
            retry_frame.batch_id,
            retry_frame.arrow_ipc.to_vec(),
        )
        .await?;
    for server in cluster.servers() {
        server.flush_bifrost().await?;
    }

    // Negative auth and resolution paths are exercised through the same public
    // SDK transport. Neither request is allowed to create durable file-list
    // state because Gate rejects before Scribe admission.
    let underprivileged = bootstrap_transport(first, "closeout-underprivileged", &[]).await?;
    let denied = underprivileged
        .insert_batch(TABLE_FQN, uuid::Uuid::now_v7().into_bytes(), ipc(&[200]))
        .await
        .expect_err("under-privileged token must be rejected");
    assert_eq!(denied.status(), 403);

    let unknown = admin
        .insert_batch(
            "vala.bifrost.unknown_closeout_table",
            uuid::Uuid::now_v7().into_bytes(),
            ipc(&[201]),
        )
        .await
        .expect_err("unknown table must be rejected before Scribe");
    assert_eq!(unknown.status(), 404);

    let conflict = admin
        .insert_batch(
            TABLE_FQN,
            uuid::Uuid::now_v7().into_bytes(),
            conflicting_ipc(),
        )
        .await
        .expect_err("schema conflict must be rejected before ACK");
    assert_eq!(conflict.status(), 409);

    let mut oracle_rows = 0_u64;
    for (pod, server) in cluster.servers().iter().enumerate() {
        let query_client =
            bootstrap_client(server, &format!("closeout-query-{pod}"), &["admin"]).await?;
        let query = query_client
            .request_arrow(
                reqwest::Method::POST,
                "/v1/query",
                Some(&SyncQueryRequest {
                    sql: format!("SELECT id, value FROM \"{TABLE_FQN}\" ORDER BY id"),
                    params: Vec::new(),
                }),
            )
            .await?;
        oracle_rows = oracle_rows.saturating_add(query.row_count.unwrap_or_default());
        assert!(!query.frames.is_empty(), "Oracle must return Arrow data");
    }
    assert_eq!(
        oracle_rows, 8,
        "distributed Oracle reads must cover every unique row"
    );

    let rows: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(row_count), 0)::bigint
           FROM vala.file_list
          WHERE data_tenant_id = $1 AND namespace = 'vala.bifrost' AND table_name = $2",
    )
    .bind(tenant.as_uuid())
    .bind(TABLE_NAME)
    .fetch_one(cluster.pg_fixture().operator_pool().pool())
    .await?;
    assert_eq!(
        rows, 8,
        "durable readback must include every unique frame row"
    );

    let mut conn = wyrd_sql::TenantConn::acquire(cluster.pg_fixture().app_pool(), tenant).await?;
    let audit_principals: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT principal_id)::bigint
           FROM vala.audit_outbox
          WHERE data_tenant_id = wyrd.current_tenant() AND resource = $1",
    )
    .bind(TABLE_FQN)
    .fetch_one(&mut **conn.transaction())
    .await?;
    assert!(audit_principals >= 3, "audit rows retain verified callers");
    Ok(())
}

async fn run_delayed_fsync_journey(
    cluster: &WyrdTestCluster,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let tenant = cluster.data_tenant_id();
    let server = cluster.server(0).ok_or("missing delayed-fsync pod")?;
    server
        .state()
        .bifrost_redux
        .as_ref()
        .ok_or("missing Redux catalog")?
        .create_table(CreateTableRequest {
            table: TableRef::new(BifrostNamespace::Bifrost, TABLE_NAME),
            user_fields: vec![
                Field::new("id", DataType::Int64, false),
                Field::new("value", DataType::Utf8, false),
            ],
            tenant,
            audit: None,
        })
        .await?;

    let transport = bootstrap_transport(server, "delayed-fsync-admin", &["admin"]).await?;
    for id in 0..8_i64 {
        transport
            .insert_batch(TABLE_FQN, uuid::Uuid::now_v7().into_bytes(), ipc(&[id]))
            .await?;
    }
    server.flush_bifrost().await?;

    let query_client = bootstrap_client(server, "delayed-fsync-query", &["admin"]).await?;
    let query = query_client
        .request_arrow(
            reqwest::Method::POST,
            "/v1/query",
            Some(&SyncQueryRequest {
                sql: format!("SELECT id, value FROM \"{TABLE_FQN}\" ORDER BY id"),
                params: Vec::new(),
            }),
        )
        .await?;
    assert_eq!(query.row_count, Some(8));
    Ok(())
}

async fn bootstrap_transport(
    server: &wyrd_testing::WyrdTestServer,
    name: &str,
    roles: &[&str],
) -> Result<BifrostGrpcTransport, Box<dyn std::error::Error + Send + Sync>> {
    let client = bootstrap_client(server, name, roles).await?;
    Ok(BifrostGrpcTransport::connect(&client).await?)
}

async fn bootstrap_client(
    server: &wyrd_testing::WyrdTestServer,
    name: &str,
    roles: &[&str],
) -> Result<WyrdClient, Box<dyn std::error::Error + Send + Sync>> {
    let bootstrap = server.bootstrap_service(name, roles).await?;
    let key = match bootstrap {
        Bootstrap::Machine { api_key, .. } => api_key,
        Bootstrap::User { .. } => return Err("service bootstrap returned a user".into()),
    };
    let mut config = ClientConfig {
        grpc: GrpcConfig {
            endpoint: server.grpc_url().ok_or("missing gRPC endpoint")?,
            connect_retries: 0,
            ..GrpcConfig::default()
        },
        http: HttpConfig {
            base_url: server.base_url().ok_or("missing HTTP endpoint")?.to_owned(),
            ..HttpConfig::default()
        },
        api_key: Some(key),
        ..ClientConfig::default()
    };
    config.grpc.max_message_bytes = 32 * 1024 * 1024;
    Ok(WyrdClient::with_config(config)?)
}

fn frame(batch_id: [u8; 16], ids: &[i64]) -> BifrostFrame {
    BifrostFrame {
        table: TABLE_FQN.to_owned(),
        batch_id,
        arrow_ipc: ipc(ids).into(),
    }
}

fn ipc(ids: &[i64]) -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("value", DataType::Utf8, false),
    ]));
    let rows = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(ids.to_vec())),
            Arc::new(StringArray::from(vec!["journey"; ids.len()])),
        ],
    )
    .expect("valid closeout batch");
    encode(rows, schema)
}

fn conflicting_ipc() -> Vec<u8> {
    let ids = &[1_i64];
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("value", DataType::Utf8, false),
    ]));
    let rows = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(StringArray::from(
                ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
            )),
            Arc::new(StringArray::from(vec!["conflict"; ids.len()])),
        ],
    )
    .expect("valid conflicting batch");
    encode(rows, schema)
}

fn encode(rows: RecordBatch, schema: Arc<Schema>) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, schema.as_ref()).expect("IPC writer");
    writer.write(&rows).expect("IPC batch");
    writer.finish().expect("IPC stream");
    bytes
}
