use std::time::Duration;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::maintenance::{StagingFileCommitted, StagingPublishOutcome};
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::{seed_forge_group, seed_forge_group_for_tenant_with_schema_and_days};

#[tokio::test]
#[ignore = "gated journey: bound Wyrd server plus shared Postgres and Iceberg"]
/// Runs three directly awaitable scheduler futures against shared durable state.
/// Lease cleanup and convergence prove the server can supervise competing pods
/// without leaving a stale ownership row.
async fn journey_forge_scheduler_three_pod_lease_competition_converges_once() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_millis(10))
        .start_bound()
        .await
        .expect("test server");
    let forge = server.state().forge().expect("Forge").clone();
    let fixtures = vec![
        seed_forge_group(&server, "multipod_rows_a").await,
        seed_forge_group(&server, "multipod_rows_b").await,
        seed_forge_group(&server, "multipod_rows_c").await,
        seed_forge_group(&server, "multipod_rows_d").await,
        seed_forge_group(&server, "multipod_rows_e").await,
    ];
    let shutdown = CancellationToken::new();
    let scheduler = tokio::spawn({
        let forge = forge.clone();
        let stop = shutdown.clone();
        async move { forge.run(stop).await }
    });
    for fixture in &fixtures {
        for _ in 0..100 {
            let compacted: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3 AND compacted AND committed_snapshot_id IS NOT NULL",
            )
            .bind(fixture.tenant.as_uuid())
            .bind(&fixture.binding.logical_namespace)
            .bind(&fixture.binding.table_name)
            .fetch_one(fixture.operator_pool.pool())
            .await
            .expect("compaction state");
            if compacted == 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
    shutdown.cancel();
    scheduler
        .await
        .expect("scheduler task")
        .expect("third scheduler shutdown");
    let operator_pool = server
        .state()
        .postgres
        .operator_pool()
        .expect("operator pool");
    let mut lease_count = i64::MAX;
    for _ in 0..100 {
        lease_count = sqlx::query_scalar(
            "SELECT count(*) FROM vala.maintenance_leases WHERE lease_key LIKE 'forge:table:%'",
        )
        .fetch_one(operator_pool.pool())
        .await
        .expect("Forge lease query");
        if lease_count == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(lease_count, 0, "server scheduler leaves no stale lease");
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "gated journey: bound Wyrd server plus restart/live-file assertions"]
/// Cancels one scheduler, runs a durable one-shot tick, and starts a successor
/// future. This protects restart-safe catalog reads, live-file retention, and
/// the public cancellation contract.
async fn journey_forge_scheduler_restart_preserves_reads_and_live_files() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("test server");
    let forge = server.state().forge().expect("Forge").clone();
    let fixture = seed_forge_group(&server, "restart_rows").await;
    let outcome = forge
        .run_once()
        .await
        .expect("one-shot production Forge tick");
    assert_eq!(outcome.bins_committed, 1);
    let shutdown = CancellationToken::new();
    let restarted = tokio::spawn({
        let forge = forge.clone();
        let stop = shutdown.clone();
        async move { forge.run(stop).await }
    });
    tokio::time::sleep(Duration::from_millis(30)).await;
    shutdown.cancel();
    restarted
        .await
        .expect("restarted scheduler task")
        .expect("restarted scheduler shutdown");
    let compacted: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3 AND compacted AND committed_snapshot_id IS NOT NULL",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("restart file-list state");
    assert_eq!(compacted, 2);
    assert!(
        fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("restart catalog read")
            .metadata()
            .current_snapshot_id()
            .is_some()
    );
    assert_eq!(
        fixture
            .operation_count("forge.file_compact.committed")
            .await,
        1
    );
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "gated journey: active-day incremental compaction against shared Postgres and Iceberg"]
/// Proves active-day rows are compacted from durable file-list state without
/// waiting for the periodic age guard, using the public Forge handle and the
/// authenticated physical tenant/table/day identity.
///
/// # Errors
///
/// The journey fails when the bound server, durable SQL state, or Iceberg
/// catalog cannot complete the incremental compaction.
async fn active_partition_incremental_compaction() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("test server");
    let today = chrono::Utc::now().date_naive();
    let fixture = seed_forge_group_for_tenant_with_schema_and_days(
        &server,
        server.data_tenant_id(),
        "active_incremental_rows",
        false,
        &[today],
    )
    .await;
    let total_bytes: i64 = sqlx::query_scalar(
        "SELECT coalesce(sum(file_size), 0)::bigint FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("active file sizes");
    let mut config = fixture.config.clone();
    config.target_bin_bytes = u64::try_from(total_bytes).expect("positive active bytes");
    config.max_hints_per_wake = 1;
    let (forge, publisher) = fixture.context_with_config_and_publisher(config.clone());
    let (competing_forge_a, competing_publisher_a) =
        fixture.context_with_config_and_publisher(config.clone());
    let (competing_forge_b, competing_publisher_b) =
        fixture.context_with_config_and_publisher(config);
    assert_eq!(
        publisher.try_publish(StagingFileCommitted::new(fixture.binding.clone(), today)),
        StagingPublishOutcome::Published
    );
    assert_eq!(
        publisher.try_publish(StagingFileCommitted::new(fixture.binding.clone(), today)),
        StagingPublishOutcome::DroppedFull
    );
    assert_eq!(
        competing_publisher_a
            .try_publish(StagingFileCommitted::new(fixture.binding.clone(), today)),
        StagingPublishOutcome::Published
    );
    assert_eq!(
        competing_publisher_b
            .try_publish(StagingFileCommitted::new(fixture.binding.clone(), today)),
        StagingPublishOutcome::Published
    );
    let shutdown = CancellationToken::new();
    let scheduler = tokio::spawn({
        let forge = forge.clone();
        let stop = shutdown.clone();
        async move { forge.run(stop).await }
    });
    let scheduler_a = tokio::spawn({
        let stop = shutdown.clone();
        async move { competing_forge_a.run(stop).await }
    });
    let scheduler_b = tokio::spawn({
        let stop = shutdown.clone();
        async move { competing_forge_b.run(stop).await }
    });
    let mut compacted = 0_i64;
    for _ in 0..100 {
        compacted = sqlx::query_scalar(
            "SELECT count(*) FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3 AND compacted",
        )
        .bind(fixture.tenant.as_uuid())
        .bind(&fixture.binding.logical_namespace)
        .bind(&fixture.binding.table_name)
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("active compaction state");
        if compacted == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    shutdown.cancel();
    scheduler
        .await
        .expect("active scheduler")
        .expect("scheduler shutdown");
    scheduler_a
        .await
        .expect("competing scheduler")
        .expect("scheduler shutdown");
    scheduler_b
        .await
        .expect("competing scheduler")
        .expect("scheduler shutdown");
    assert_eq!(compacted, 2);
    assert_eq!(
        fixture.operation_count("forge.file_compact.prepared").await,
        1
    );
    assert_eq!(
        fixture
            .operation_count("forge.file_compact.committed")
            .await,
        1
    );
    assert!(
        fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("active table")
            .metadata()
            .current_snapshot_id()
            .is_some()
    );
    let closed = seed_forge_group_for_tenant_with_schema_and_days(
        &server,
        fixture.tenant,
        "closed_remainder_incremental",
        false,
        &[chrono::NaiveDate::from_ymd_opt(2026, 7, 14).expect("closed day")],
    )
    .await;
    let mut closed_config = closed.config.clone();
    closed_config.max_hints_per_wake = 1;
    let (closed_forge, closed_publisher) = closed.context_with_config_and_publisher(closed_config);
    assert_eq!(
        closed_publisher.try_publish(StagingFileCommitted::new(
            closed.binding.clone(),
            chrono::NaiveDate::from_ymd_opt(2026, 7, 14).expect("closed day"),
        )),
        StagingPublishOutcome::Published
    );
    assert_eq!(
        closed_publisher.try_publish(StagingFileCommitted::new(
            closed.binding.clone(),
            chrono::NaiveDate::from_ymd_opt(2026, 7, 14).expect("closed day"),
        )),
        StagingPublishOutcome::DroppedFull
    );
    let closed_outcome = closed_forge
        .run_once()
        .await
        .expect("closed remainder tick");
    assert_eq!(closed_outcome.bins_committed, 1);
    assert_eq!(
        closed.operation_count("forge.file_compact.committed").await,
        1
    );
    let tail = seed_forge_group_for_tenant_with_schema_and_days(
        &server,
        fixture.tenant,
        "open_tail_incremental",
        false,
        &[today],
    )
    .await;
    let mut tail_config = tail.config.clone();
    tail_config.max_files_per_bin = 3;
    let (tail_forge, tail_publisher) = tail.context_with_config_and_publisher(tail_config.clone());
    let _ = tail_publisher.try_publish(StagingFileCommitted::new(tail.binding.clone(), today));
    let first_stop = CancellationToken::new();
    let first_task = tokio::spawn({
        let stop = first_stop.clone();
        async move { tail_forge.run(stop).await }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    first_stop.cancel();
    first_task
        .await
        .expect("tail scheduler")
        .expect("tail shutdown");
    let initial_tail: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3 AND NOT compacted")
        .bind(tail.tenant.as_uuid()).bind(&tail.binding.logical_namespace).bind(&tail.binding.table_name)
        .fetch_one(tail.operator_pool.pool()).await.expect("tail state");
    assert_eq!(initial_tail, 2);
    assert_eq!(
        tail.operation_count("forge.file_compact.committed").await,
        0
    );
    tail.append_forge_file_for_day(99, today, false).await;
    let (tail_forge, tail_publisher) = tail.context_with_config_and_publisher(tail_config);
    assert_eq!(
        tail_publisher.try_publish(StagingFileCommitted::new(tail.binding.clone(), today)),
        StagingPublishOutcome::Published
    );
    let tail_stop = CancellationToken::new();
    let tail_task = tokio::spawn({
        let stop = tail_stop.clone();
        async move { tail_forge.run(stop).await }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    tail_stop.cancel();
    tail_task
        .await
        .expect("tail scheduler")
        .expect("tail shutdown");
    assert_eq!(
        tail.operation_count("forge.file_compact.committed").await,
        1
    );
    server.shutdown().await.expect("server shutdown");
}
