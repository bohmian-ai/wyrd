//! Gated real-server Forge scheduler journey.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use arrow::array::{Int64Array, RecordBatch, StringArray, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema as ArrowSchema, TimeUnit};
use iceberg::TableCreation;
use opendal::Buffer;
use parquet::arrow::ArrowWriter;
use vala_bifrost_redux::catalog::{TableRef, TenantTableBinding, build_partition_spec};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use wyrd_testing::WyrdTestServer;

async fn seed_forge_group(
    server: &WyrdTestServer,
) -> (
    Arc<vala_bifrost_redux::forge::ForgeContext>,
    TenantTableBinding,
) {
    let context = server
        .state()
        .forge_context
        .as_ref()
        .cloned()
        .expect("production server has Redux Forge context");
    let tenant = server.data_tenant_id();
    let binding = TenantTableBinding::resolve((
        tenant,
        TableRef::new(BifrostNamespace::Bifrost, "journey_rows"),
    ))
    .expect("binding");
    let schema = ArrowSchema::new(vec![
        Field::new("value", DataType::Int64, false),
        Field::new(
            "wyrd_event_time",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
        Field::new("data_tenant_id", DataType::Utf8, false),
    ]);
    let iceberg_schema =
        iceberg::arrow::arrow_schema_to_schema_auto_assign_ids(&schema).expect("Iceberg schema");
    let spec = build_partition_spec(&iceberg_schema, &binding.partition_columns())
        .expect("partition spec");
    let warehouse =
        vala_bifrost::catalog::storage::warehouse_uri(server.state().storage.backend_config());
    context
        .catalog
        .create_namespace(binding.physical_namespace(), HashMap::new())
        .await
        .expect("tenant namespace");
    context
        .catalog
        .create_table(
            binding.physical_namespace(),
            TableCreation::builder()
                .name(binding.table_name.clone())
                .location(format!("{warehouse}/{}", binding.object_prefix))
                .schema(iceberg_schema)
                .partition_spec(spec)
                .build(),
        )
        .await
        .expect("tenant table");

    let base = chrono::DateTime::parse_from_rfc3339("2026-07-14T12:00:00Z")
        .expect("timestamp")
        .timestamp_micros();
    let mut conn = context
        .vala
        .tenant_conn(tenant)
        .await
        .expect("tenant connection");
    for file_number in 0..2_i64 {
        let batch = RecordBatch::try_new(
            Arc::new(schema.clone()),
            vec![
                Arc::new(Int64Array::from(vec![file_number, file_number + 10])),
                Arc::new(
                    TimestampMicrosecondArray::from(vec![
                        base + file_number * 1_000_000,
                        base + file_number * 1_000_000 + 1_000,
                    ])
                    .with_timezone("UTC"),
                ),
                Arc::new(StringArray::from(vec![tenant.to_string(); 2])),
            ],
        )
        .expect("staging batch");
        let mut bytes = Vec::new();
        let mut writer =
            ArrowWriter::try_new(&mut bytes, batch.schema(), None).expect("Parquet writer");
        writer.write(&batch).expect("Parquet batch");
        writer.close().expect("Parquet close");
        let path = format!("{}/journey-{file_number}.parquet", binding.object_prefix);
        let size = i64::try_from(bytes.len()).expect("file size");
        context
            .staging
            .write(&path, Buffer::from(bytes))
            .await
            .expect("staging object");
        let min_time = chrono::DateTime::parse_from_rfc3339("2026-07-14T12:00:00Z")
            .expect("timestamp")
            .with_timezone(&chrono::Utc)
            + chrono::Duration::seconds(file_number);
        sqlx::query(
            "INSERT INTO vala.file_list (id, data_tenant_id, namespace, table_name, file_path, file_size, row_count, min_event_time, max_event_time, partition_day, node_id, writer_epoch, wal_lsn_min, wal_lsn_max) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
        )
        .bind(uuid::Uuid::now_v7())
        .bind(tenant.as_uuid())
        .bind(&binding.logical_namespace)
        .bind(&binding.table_name)
        .bind(path)
        .bind(size)
        .bind(2_i64)
        .bind(min_time)
        .bind(min_time + chrono::Duration::milliseconds(1))
        .bind(chrono::NaiveDate::from_ymd_opt(2026, 7, 14).expect("partition day"))
        .bind(uuid::Uuid::now_v7())
        .bind(1_i64)
        .bind(file_number * 2 + 1)
        .bind(file_number * 2 + 2)
        .execute(&mut **conn.transaction())
        .await
        .expect("file list row");
    }
    conn.commit().await.expect("file list commit");
    sqlx::query(
        "UPDATE vala.file_list SET created_at = now() - interval '3 minutes' WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3",
    )
    .bind(tenant.as_uuid())
    .bind(&binding.logical_namespace)
    .bind(&binding.table_name)
    .execute(context.operator_pool.pool())
    .await
    .expect("age file list rows");
    (context, binding)
}

#[tokio::test]
#[ignore = "requires the real Postgres-backed WyrdTestServer journey lane"]
async fn journey_forge_scheduler_single_pod_end_to_end() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_millis(10))
        .start_bound()
        .await
        .expect("real server");
    let (context, binding) = seed_forge_group(&server).await;
    let tenant = server.data_tenant_id();

    let mut compacted = false;
    for _ in 0..100 {
        compacted = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3 AND compacted AND committed_snapshot_id IS NOT NULL",
        )
        .bind(tenant.as_uuid())
        .bind(&binding.logical_namespace)
        .bind(&binding.table_name)
        .fetch_one(context.operator_pool.pool())
        .await
        .expect("compaction state")
            == 2;
        if compacted {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        compacted,
        "server scheduler did not commit the seeded group"
    );
    let table = context
        .catalog
        .load_table(&binding.table_ident())
        .await
        .expect("journey table");
    assert!(table.metadata().current_snapshot_id().is_some());
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id = $1 AND operation IN ('forge.file_compact.prepared', 'forge.file_compact.committed')",
    )
    .bind(tenant.as_uuid())
    .fetch_one(context.operator_pool.pool())
    .await
    .expect("audit count");
    assert_eq!(audit_count, 2);
    server.shutdown().await.expect("server shutdown");
}
