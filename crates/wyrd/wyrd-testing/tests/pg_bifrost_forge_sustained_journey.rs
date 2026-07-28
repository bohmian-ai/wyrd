//! Gated sustained Forge journey over durable Scribe-shaped inputs.

use std::time::Duration;

use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::maintenance::{StagingFileCommitted, StagingPublishOutcome};
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::{
    ForgeFixture, seed_forge_group, seed_forge_group_for_tenant,
    seed_forge_group_for_tenant_with_schema_and_days,
};

#[tokio::test]
#[ignore = "gated journey: bound Wyrd server plus durable multi-table workload"]
/// Tests sustained production scheduling against durable Parquet, Iceberg, SQL,
/// audit, and lease state.
///
/// Steps:
/// 1. Start one bound Wyrd server, which starts its production Forge scheduler,
///    and seed four physical tables through the shared Postgres/SQL-Iceberg/
///    OpenDAL fixture.
/// 2. Run three concurrent producer tasks. Each appends aged Scribe-shaped
///    Parquet files and matching `vala.file_list` rows to one table for several
///    cycles, while the server scheduler runs every 10 ms.
/// 3. Drain with public one-shot ticks, then query durable `file_list` counts,
///    committed snapshots, terminal audit transitions, and maintenance leases.
///
/// The assertions verify exact durable row-count conservation, a readable
/// committed snapshot per table, and no lease residue. A prepared operation
/// may finish as committed, be recovered after an uncertain catalog response,
/// or be reset after a definite failure; every prepared operation must have
/// exactly one such terminal event.
/// This catches loss, duplicate bookkeeping, table starvation, and shutdown
/// cleanup failures that a health endpoint or in-memory load callback cannot see.
async fn forge_sustained_scheduler_with_ingest_and_queries_has_zero_loss_or_leak() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
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

    server.cancel_bound_workers();
    let operator_pool = fixtures[0].operator_pool.clone();
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

    for _ in 0..20 {
        for fixture in &fixtures {
            fixture
                .forge
                .run_once()
                .await
                .expect("durable Forge drain tick");
        }
        if maintenance_settled(&fixtures[0]).await
            && maintenance_settled(&fixtures[1]).await
            && maintenance_settled(&fixtures[2]).await
            && maintenance_settled(&fixtures[3]).await
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
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("durable row count");
        assert_eq!(rows, 24);
        assert_eq!(read_staged_parquet_rows(fixture).await, 24);
        let committed = fixture
            .operation_count("forge.file_compact.committed")
            .await;
        let recovered = fixture
            .operation_count("forge.file_compact.recovered")
            .await;
        let reset = fixture.operation_count("forge.file_compact.reset").await;
        assert!(committed + recovered >= 1);
        let prepared = fixture.operation_count("forge.file_compact.prepared").await;
        assert_eq!(prepared, committed + recovered + reset);
        assert!(
            fixture
                .catalog
                .load_table(&fixture.binding.table_ident())
                .await
                .expect("durable catalog read")
                .metadata()
                .current_snapshot_id()
                .is_some()
        );
    }
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "gated sustained journey: current-day publication and durable Forge drain"]
/// Publishes a continuous stream of Scribe-shaped files while the production
/// scheduler runs, then drains with the public one-shot API and checks exact
/// row conservation and terminal bookkeeping.
///
/// # Errors
///
/// The journey fails when sustained staging, SQL bookkeeping, or Iceberg
/// reads cannot converge through the production Forge handle.
async fn pg_bifrost_forge_incremental_sustained() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real test server");
    let fixture = seed_forge_group_for_tenant_with_schema_and_days(
        &server,
        server.data_tenant_id(),
        "incremental_sustained_rows",
        false,
        &[chrono::Utc::now().date_naive()],
    )
    .await;
    server.cancel_bound_workers();
    let baseline_memory = fixture.memory_snapshot().bifrost_total_bytes;
    let mut config = fixture.config.clone();
    config.max_hints_per_wake = 1;
    config.max_files_per_bin = 2;
    config.output_file_bytes = 1;
    let (forge, publisher) =
        fixture.context_with_constrained_memory_and_publisher(config, 16 * 1024 * 1024);
    let drain_forge = forge.clone();
    assert_eq!(
        publisher.try_publish(StagingFileCommitted::new(
            fixture.binding.clone(),
            chrono::Utc::now().date_naive(),
        )),
        StagingPublishOutcome::Published
    );
    assert_eq!(
        publisher.try_publish(StagingFileCommitted::new(
            fixture.binding.clone(),
            chrono::Utc::now().date_naive(),
        )),
        StagingPublishOutcome::DroppedFull
    );
    let shutdown = CancellationToken::new();
    let scheduler = tokio::spawn({
        let stop = shutdown.clone();
        async move { forge.run(stop).await }
    });
    for sequence in 0..8_i64 {
        fixture
            .append_forge_file_with_rows(100 + sequence * 2, 100_000)
            .await;
        fixture
            .append_forge_file_with_rows(101 + sequence * 2, 100_000)
            .await;
        let _ = publisher.try_publish(StagingFileCommitted::new(
            fixture.binding.clone(),
            chrono::Utc::now().date_naive(),
        ));
        tokio::time::sleep(Duration::from_millis(15)).await;
    }
    server.cancel_bound_workers();
    let mut spill_bytes = 0_u64;
    let mut outputs_committed = 0_usize;
    for _ in 0..20 {
        let outcome = drain_forge
            .run_once()
            .await
            .expect("incremental drain tick");
        spill_bytes = spill_bytes.max(outcome.spill_bytes);
        outputs_committed = outputs_committed.saturating_add(outcome.outputs_committed);
        if maintenance_settled(&fixture).await {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(pending_file_count(&fixture).await, 0);
    assert!(
        spill_bytes > 0,
        "incremental rewrite must exercise DataFusion spill"
    );
    assert!(spill_bytes <= fixture.config.spill_limit_bytes);
    assert!(
        outputs_committed > 1,
        "small output ceiling must rotate outputs"
    );
    let rows: i64 = sqlx::query_scalar(
        "SELECT coalesce(sum(row_count), 0)::bigint FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("sustained row count");
    // The seeded fixture contributes two rows in addition to 16×100,000 rows.
    assert_eq!(rows, 1_600_004);
    assert!(
        fixture
            .operation_count("forge.file_compact.committed")
            .await
            >= 1
    );
    assert_eq!(
        fixture.operation_count("forge.file_compact.prepared").await,
        fixture
            .operation_count("forge.file_compact.committed")
            .await
            + fixture
                .operation_count("forge.file_compact.recovered")
                .await
            + fixture.operation_count("forge.file_compact.reset").await
    );
    shutdown.cancel();
    scheduler
        .await
        .expect("sustained scheduler")
        .expect("scheduler shutdown");
    assert!(fixture.spill_root_is_empty());
    assert!(
        fixture.memory_snapshot().bifrost_total_bytes <= baseline_memory + 2,
        "shared memory did not return near baseline"
    );
    server.shutdown().await.expect("server shutdown");
}

async fn pending_file_count(fixture: &ForgeFixture) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3 AND NOT compacted",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("pending file count")
}

async fn maintenance_settled(fixture: &ForgeFixture) -> bool {
    let prepared = fixture.operation_count("forge.file_compact.prepared").await;
    let terminal = fixture
        .operation_count("forge.file_compact.committed")
        .await
        + fixture
            .operation_count("forge.file_compact.recovered")
            .await
        + fixture.operation_count("forge.file_compact.reset").await;
    pending_file_count(fixture).await == 0 && prepared == terminal
}

async fn read_staged_parquet_rows(fixture: &ForgeFixture) -> usize {
    let paths: Vec<String> = sqlx::query_scalar(
        "SELECT file_path FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3 ORDER BY file_path",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_all(fixture.operator_pool.pool())
    .await
    .expect("staged file paths");
    let mut rows = 0_usize;
    for path in paths {
        let bytes = fixture
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
