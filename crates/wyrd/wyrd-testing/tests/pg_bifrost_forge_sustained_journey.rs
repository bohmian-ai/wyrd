//! Gated sustained Forge journey over durable Scribe-shaped inputs.

use std::time::Duration;

use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use vala_bifrost_redux::forge::run_maintenance_tick;
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::{ForgeFixture, seed_forge_group, seed_forge_group_for_tenant};

#[tokio::test]
#[ignore = "gated journey: bound Wyrd server plus durable multi-table workload"]
/// Tests sustained production scheduling against durable Parquet, Iceberg, SQL,
/// audit, and lease state.
///
/// Steps:
/// 1. Start one bound Wyrd server, which starts its production Forge scheduler,
///    and seed three physical tables through the shared Postgres/SQL-Iceberg/
///    OpenDAL fixture.
/// 2. Run three concurrent producer tasks. Each appends aged Scribe-shaped
///    Parquet files and matching `vala.file_list` rows to one table for several
///    cycles, while the server scheduler runs every 10 ms.
/// 3. Drain with public one-shot ticks, then query durable `file_list` counts,
///    committed snapshots, audit pairs, and maintenance leases.
///
/// The assertions verify exact durable row-count conservation, a readable
/// committed snapshot per table, matched prepared/committed audit pairs,
/// and no lease residue.
/// This catches loss, duplicate bookkeeping, table starvation, and shutdown
/// cleanup failures that a health endpoint or in-memory load callback cannot see.
async fn forge_sustained_scheduler_with_ingest_and_queries_has_zero_loss_or_leak() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_millis(10))
        .start_bound()
        .await
        .expect("real test server");
    let tenant_b = server
        .seed_tenant("forge-sustained-tenant-b")
        .await
        .expect("second sustained tenant");
    let fixtures = vec![
        seed_forge_group(&server, "sustained_rows_a").await,
        seed_forge_group(&server, "sustained_rows_b").await,
        seed_forge_group_for_tenant(&server, tenant_b, "sustained_rows_c").await,
        seed_forge_group_for_tenant(&server, tenant_b, "sustained_rows_d").await,
    ];
    let producer_fixtures = fixtures.clone();
    let producers = producer_fixtures
        .into_iter()
        .enumerate()
        .map(|(table_index, fixture)| {
            tokio::spawn(async move {
                for cycle in 0..10_i64 {
                    fixture
                        .append_forge_file(table_index as i64 * 100 + cycle * 2)
                        .await;
                    fixture
                        .append_forge_file(table_index as i64 * 100 + cycle * 2 + 1)
                        .await;
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            })
        })
        .collect::<Vec<_>>();
    let readers = fixtures
        .iter()
        .cloned()
        .map(|fixture| {
            tokio::spawn(async move {
                for _ in 0..40 {
                    let _ = pending_file_count(&fixture).await;
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
        })
        .collect::<Vec<_>>();
    for producer in producers {
        producer.await.expect("producer task");
    }
    for reader in readers {
        reader.await.expect("reader task");
    }

    for _ in 0..20 {
        for fixture in &fixtures {
            run_maintenance_tick(&fixture.context)
                .await
                .expect("durable Forge drain tick");
        }
        if pending_file_count(&fixtures[0]).await == 0
            && pending_file_count(&fixtures[1]).await == 0
            && pending_file_count(&fixtures[2]).await == 0
            && pending_file_count(&fixtures[3]).await == 0
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    for fixture in &fixtures {
        assert_eq!(pending_file_count(fixture).await, 0);
        let rows: i64 = sqlx::query_scalar(
            "SELECT coalesce(sum(row_count), 0)::bigint FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3",
        )
        .bind(fixture.tenant.as_uuid())
        .bind(&fixture.binding.logical_namespace)
        .bind(&fixture.binding.table_name)
        .fetch_one(fixture.context.operator_pool.pool())
        .await
        .expect("durable row count");
        assert_eq!(rows, 24);
        assert_eq!(read_staged_parquet_rows(fixture).await, 24);
        let committed: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id = $1 AND operation = 'forge.file_compact.committed' AND resource = $2",
        )
        .bind(fixture.tenant.as_uuid())
        .bind(format!(
            "bifrost://{}/{}/{}",
            fixture.tenant, fixture.binding.logical_namespace, fixture.binding.table_name
        ))
        .fetch_one(fixture.context.operator_pool.pool())
        .await
        .expect("durable committed audit count");
        assert!(committed >= 1);
        let prepared: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id = $1 AND operation = 'forge.file_compact.prepared' AND resource = $2",
        )
        .bind(fixture.tenant.as_uuid())
        .bind(format!(
            "bifrost://{}/{}/{}",
            fixture.tenant, fixture.binding.logical_namespace, fixture.binding.table_name
        ))
        .fetch_one(fixture.context.operator_pool.pool())
        .await
        .expect("durable prepared audit count");
        assert_eq!(prepared, committed);
        assert!(
            fixture
                .context
                .catalog
                .load_table(&fixture.binding.table_ident())
                .await
                .expect("durable catalog read")
                .metadata()
                .current_snapshot_id()
                .is_some()
        );
    }
    let operator_pool = fixtures[0].context.operator_pool.clone();
    let mut leases = i64::MAX;
    for _ in 0..100 {
        leases = sqlx::query_scalar(
            "SELECT count(*) FROM vala.maintenance_leases WHERE lease_key LIKE 'forge:table:%'",
        )
        .fetch_one(operator_pool.pool())
        .await
        .expect("durable lease count");
        if leases == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(leases, 0);
    server.shutdown().await.expect("server shutdown");
}

async fn pending_file_count(fixture: &ForgeFixture) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3 AND NOT compacted",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.context.operator_pool.pool())
    .await
    .expect("pending file count")
}

async fn read_staged_parquet_rows(fixture: &ForgeFixture) -> usize {
    let paths: Vec<String> = sqlx::query_scalar(
        "SELECT file_path FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3 ORDER BY file_path",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_all(fixture.context.operator_pool.pool())
    .await
    .expect("staged file paths");
    let mut rows = 0_usize;
    for path in paths {
        let bytes = fixture
            .context
            .staging
            .read(&path)
            .await
            .expect("staged Parquet read");
        let reader = ParquetRecordBatchReaderBuilder::try_new(bytes.to_bytes())
            .expect("staged Parquet reader")
            .build()
            .expect("staged Parquet batch reader");
        for batch in reader {
            rows += batch.expect("staged Parquet batch").num_rows();
        }
    }
    rows
}
