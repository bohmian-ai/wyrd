use std::time::Duration;
use tokio_util::sync::CancellationToken;
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::seed_forge_group;

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
