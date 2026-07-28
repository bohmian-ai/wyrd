use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::maintenance::{StagingFileCommitted, StagingPublishOutcome};
use wyrd_testing::bifrost::{
    BifrostHarness, seed_forge_group, seed_forge_group_for_tenant_with_schema_and_days,
};
use wyrd_testing::otlp::RandomTraceGenerator;
use wyrd_testing::{Bootstrap, WyrdTestServer};

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
    let harness = BifrostHarness::start(3, 1)
        .await
        .expect("three-pod Bifrost harness");
    let servers = harness.cluster().servers();
    assert_eq!(
        servers.len(),
        3,
        "BifrostHarness must expose three real pods"
    );
    let server = &servers[0];
    let tenant = server.data_tenant_id();
    let today = chrono::Utc::now().date_naive();
    let machine = server
        .bootstrap_service("active-incremental-writer", &["admin"])
        .await
        .expect("bootstrap tenant service");
    let api_key = match machine {
        Bootstrap::Machine { api_key, .. } => api_key,
        other => panic!("expected machine bootstrap, got {other:?}"),
    };
    let jwt = server
        .exchange_api_key(&api_key)
        .await
        .expect("exchange API key for JWT");
    let publishers = harness.server_forge_publishers();
    let mut generator = RandomTraceGenerator::from_seed_at(
        0xA11CE,
        u64::try_from(
            chrono::Utc::now()
                .timestamp_nanos_opt()
                .expect("time nanos"),
        )
        .expect("positive time anchor"),
    );
    for (pod_index, pod) in servers.iter().enumerate() {
        let endpoint = pod.grpc_url().expect("bound pod gRPC URL");
        let deadline = Instant::now() + Duration::from_secs(5);
        let channel = loop {
            match wyrd_tonic::tonic::transport::Channel::from_shared(endpoint.clone())
                .expect("gRPC endpoint")
                .connect()
                .await
            {
                Ok(channel) => break channel,
                Err(error) => {
                    assert!(Instant::now() < deadline, "connect pod gRPC: {error}");
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        };
        let mut client =
            wyrd_tonic::otlp::trace_service::trace_service_client::TraceServiceClient::new(channel);
        for sequence in 0..2_u64 {
            let request = generator.export_request(pod_index, sequence, 6);
            let mut request = wyrd_tonic::tonic::Request::new(request);
            request.metadata_mut().insert(
                "x-wyrd-access-token",
                format!("Bearer {jwt}").parse().expect("token metadata"),
            );
            let response = client
                .export(request)
                .await
                .expect("OTLP export")
                .into_inner();
            assert!(response.partial_success.is_none());
        }
    }
    for (index, pod) in servers.iter().enumerate() {
        let before = publishers[index].capacity_for_test();
        pod.flush_bifrost_for_tenant(tenant)
            .await
            .expect("server-owned Scribe flush");
        let after_flush = publishers[index].capacity_for_test();
        assert!(after_flush <= before);
    }
    let total_files: i64 =
        sqlx::query_scalar("SELECT count(*) FROM vala.file_list WHERE data_tenant_id = $1")
            .bind(tenant.as_uuid())
            .fetch_one(
                server
                    .state()
                    .postgres
                    .operator_pool()
                    .expect("operator pool")
                    .pool(),
            )
            .await
            .expect("OTLP file-list rows");
    assert!(total_files > 0, "OTLP flush produced no file-list rows");
    let operator_pool = server
        .state()
        .postgres
        .operator_pool()
        .expect("operator pool");
    let rows = sqlx::query_as::<_, (String, String, chrono::NaiveDate, bool, Option<i64>, i64, i64)>(
        "SELECT namespace, table_name, partition_day, compacted, committed_snapshot_id, file_size, row_count FROM vala.file_list WHERE data_tenant_id = $1 ORDER BY created_at",
    )
    .bind(tenant.as_uuid())
    .fetch_all(operator_pool.pool())
    .await
    .expect("OTLP file-list diagnostics");
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(
        |(namespace, table, day, compacted, snapshot, _, row_count)| {
            namespace == "vala.traces"
                && table == "spans"
                && *day == chrono::Utc::now().date_naive()
                && !compacted
                && snapshot.is_none()
                && *row_count == 12
        }
    ));
    for publisher in &publishers {
        for _ in 0..100 {
            if publisher.capacity_for_test() == 256 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(publisher.capacity_for_test(), 256);
    }
    tokio::time::sleep(Duration::from_millis(250)).await;
    let mut audit_conn = server
        .state()
        .postgres
        .vala()
        .tenant_conn(tenant)
        .await
        .expect("Forge audit tenant connection");
    let audit_rows = sqlx::query_as::<_, (String, String, Option<String>)>(
        "SELECT operation, result, detail FROM vala.audit_outbox WHERE data_tenant_id = $1 AND resource LIKE 'bifrost://%/vala.traces/spans' ORDER BY created_at",
    )
    .bind(tenant.as_uuid())
    .fetch_all(&mut **audit_conn.transaction())
    .await
    .expect("Forge audit diagnostics");
    drop(audit_conn);
    let leases_now: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.maintenance_leases WHERE lease_key LIKE 'forge:table:%'",
    )
    .fetch_one(operator_pool.pool())
    .await
    .expect("Forge lease diagnostics");
    assert!(
        audit_rows
            .iter()
            .any(|(operation, _, _)| operation == "forge.file_compact.prepared")
    );
    assert!(
        audit_rows
            .iter()
            .any(|(operation, _, _)| operation == "forge.file_compact.committed")
    );
    assert_eq!(leases_now, 0);
    let mut compacted = 0_i64;
    for _ in 0..100 {
        compacted = sqlx::query_scalar(
            "SELECT count(*) FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = 'vala.traces' AND table_name = 'spans' AND compacted",
        )
        .bind(tenant.as_uuid())
        .fetch_one(operator_pool.pool())
        .await
        .expect("active compaction state");
        if compacted > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        compacted > 0,
        "active OTLP files must compact; compacted={compacted}"
    );
    let fixture = seed_forge_group_for_tenant_with_schema_and_days(
        server,
        tenant,
        "active_incremental_assertions",
        false,
        &[today],
    )
    .await;
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
    harness.shutdown().await.expect("harness shutdown");
}
