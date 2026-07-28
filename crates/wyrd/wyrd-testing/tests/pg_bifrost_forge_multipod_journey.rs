use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use vala_bifrost_redux::maintenance::{StagingFileCommitted, StagingPublishOutcome};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuditDecision, AuditDetail, AuditEvent, AuditResult, AuthMethod, ForgeCompactionPhase,
    StoragePath, SyncQueryRequest,
};
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
    let harness = BifrostHarness::start(3, 1)
        .await
        .expect("three-pod Bifrost harness");
    let server = harness
        .cluster()
        .server(0)
        .expect("first bound Bifrost pod");
    let fixtures = vec![
        seed_forge_group(server, "multipod_rows_a").await,
        seed_forge_group(server, "multipod_rows_b").await,
        seed_forge_group(server, "multipod_rows_c").await,
        seed_forge_group(server, "multipod_rows_d").await,
        seed_forge_group(server, "multipod_rows_e").await,
    ];
    for pod in harness.cluster().servers() {
        pod.state()
            .forge()
            .expect("server-owned Forge")
            .run_once()
            .await
            .expect("three-pod scheduler tick");
    }
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
        let prepared = fixture.operation_count("forge.file_compact.prepared").await;
        let committed = fixture
            .operation_count("forge.file_compact.committed")
            .await;
        let recovered = fixture
            .operation_count("forge.file_compact.recovered")
            .await;
        let reset = fixture.operation_count("forge.file_compact.reset").await;
        assert_eq!(prepared, committed + recovered + reset);
        assert_eq!(committed + recovered, 1, "one unique snapshot terminal");
        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("Forge snapshot");
        assert_eq!(table.metadata().snapshots().len(), 1);
    }
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
    harness
        .shutdown()
        .await
        .expect("three-pod harness shutdown");
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
/// Proves active-day rows compact without the periodic age guard and Oracle
/// fails closed for a real prepared-without-terminal audit transition before
/// returning exact rows after the matching terminal transition.
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
    let oracle_baseline = oracle_rows(server, &jwt).await;
    let unresolved_operation = Uuid::now_v7();
    append_oracle_fence_transition(
        server,
        tenant,
        unresolved_operation,
        "forge.file_compact.prepared",
        ForgeCompactionPhase::Prepared,
    )
    .await;
    assert_eq!(
        oracle_status(server, &jwt).await,
        reqwest::StatusCode::INTERNAL_SERVER_ERROR,
        "Oracle fails closed while the latest Forge operation is prepared"
    );
    append_oracle_fence_transition(
        server,
        tenant,
        unresolved_operation,
        "forge.file_compact.reset",
        ForgeCompactionPhase::Reset,
    )
    .await;
    assert_eq!(
        oracle_rows(server, &jwt).await,
        oracle_baseline,
        "a matching terminal transition restores Oracle reads"
    );
    let expected_oracle_rows = oracle_baseline + 36;
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
    let oracle_before = wait_for_oracle_rows(server, &jwt, expected_oracle_rows).await;
    assert_eq!(
        oracle_before - oracle_baseline,
        36,
        "Oracle sees the exact 36-span ingest delta before Forge"
    );
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
    let oracle_during = tokio::time::timeout(Duration::from_secs(5), oracle_rows(server, &jwt))
        .await
        .expect("Oracle during Forge bound");
    assert_eq!(oracle_during, oracle_before, "Oracle parity during Forge");
    let closed = seed_forge_group_for_tenant_with_schema_and_days(
        server,
        fixture.tenant,
        "closed_remainder_incremental",
        false,
        &[chrono::NaiveDate::from_ymd_opt(2026, 7, 14).expect("closed day")],
    )
    .await;
    let closed_publisher = harness.server_forge_publishers()[1].clone();
    assert_eq!(
        closed_publisher.try_publish(StagingFileCommitted::new(
            closed.binding.clone(),
            chrono::NaiveDate::from_ymd_opt(2026, 7, 14).expect("closed day"),
        )),
        StagingPublishOutcome::Published
    );
    let closed_forge = harness
        .cluster()
        .server(1)
        .expect("closed-day pod")
        .state()
        .forge()
        .expect("server-owned Forge");
    closed_forge
        .run_once()
        .await
        .expect("closed remainder tick");
    assert_forge_terminal_once(&closed, "closed remainder").await;
    let tail = seed_forge_group_for_tenant_with_schema_and_days(
        server,
        fixture.tenant,
        "open_tail_incremental",
        false,
        &[today],
    )
    .await;
    let tail_forge = harness
        .cluster()
        .server(2)
        .expect("open-tail pod")
        .state()
        .forge()
        .expect("server-owned Forge");
    let tail_publisher = harness.server_forge_publishers()[2].clone();
    let _ = tail_publisher.try_publish(StagingFileCommitted::new(tail.binding.clone(), today));
    tail_forge.run_once().await.expect("open-tail hint tick");
    let initial_tail: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3 AND NOT compacted")
        .bind(tail.tenant.as_uuid()).bind(&tail.binding.logical_namespace).bind(&tail.binding.table_name)
        .fetch_one(tail.operator_pool.pool()).await.expect("tail state");
    assert_eq!(initial_tail, 2);
    assert_eq!(
        tail.operation_count("forge.file_compact.committed").await,
        0
    );
    tail.append_forge_file_for_day(99, today, false).await;
    assert_eq!(
        tail_publisher.try_publish(StagingFileCommitted::new(tail.binding.clone(), today)),
        StagingPublishOutcome::Published
    );
    tail_forge
        .run_once()
        .await
        .expect("open-tail completion tick");
    assert_forge_terminal_once(&tail, "open tail").await;
    let lost_hint = seed_forge_group_for_tenant_with_schema_and_days(
        server,
        fixture.tenant,
        "periodic_lost_hint_recovery",
        true,
        &[chrono::NaiveDate::from_ymd_opt(2026, 7, 14).expect("aged day")],
    )
    .await;
    let periodic_forge = server.state().forge().expect("server-owned Forge");
    periodic_forge
        .run_once()
        .await
        .expect("periodic lost-hint tick");
    assert_forge_terminal_once(&lost_hint, "periodic lost hint").await;
    let oracle_after = oracle_rows(server, &jwt).await;
    assert_eq!(oracle_after, oracle_before, "Oracle parity after Forge");
    harness.shutdown().await.expect("harness shutdown");
}

/// Assert one durable Forge operation has exactly one terminal transition and snapshot.
async fn assert_forge_terminal_once(fixture: &wyrd_testing::bifrost::ForgeFixture, scenario: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let (prepared, terminal) = loop {
        let prepared = fixture.operation_count("forge.file_compact.prepared").await;
        let terminal = fixture
            .operation_count("forge.file_compact.committed")
            .await
            + fixture
                .operation_count("forge.file_compact.recovered")
                .await
            + fixture.operation_count("forge.file_compact.reset").await;
        if prepared == 1 && terminal == 1 {
            break (prepared, terminal);
        }
        assert!(
            Instant::now() < deadline,
            "{scenario} did not reach one prepared and terminal audit"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    };
    assert_eq!(prepared, 1, "{scenario} prepared audit");
    assert_eq!(terminal, 1, "{scenario} terminal audit");
    let table = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("snapshot");
    assert_eq!(table.metadata().snapshots().len(), 1, "{scenario} snapshot");
}

/// Query the bound server's public Oracle endpoint and return its durable row count.
///
/// # Panics
///
/// Panics when the public request fails or omits a valid row-count header.
async fn oracle_rows(server: &WyrdTestServer, jwt: &str) -> u64 {
    let url = format!("{}/v1/query", server.base_url().expect("bound URL"));
    let response = reqwest::Client::new()
        .post(url)
        .header("x-wyrd-access-token", format!("Bearer {jwt}"))
        .json(&SyncQueryRequest {
            sql: "SELECT * FROM \"vala.traces.spans\" WHERE service_name = 'checkout-api'"
                .to_owned(),
            params: Vec::new(),
        })
        .send()
        .await
        .expect("Oracle request")
        .error_for_status()
        .expect("Oracle response");
    response
        .headers()
        .get("x-wyrd-row-count")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
        .expect("Oracle row-count header")
}

/// Wait for Oracle's hot-plus-durable view to retire flushed live-tail rows.
///
/// # Panics
///
/// Panics when Oracle does not converge to the exact expected row count within
/// five seconds.
async fn wait_for_oracle_rows(server: &WyrdTestServer, jwt: &str, expected: u64) -> u64 {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let rows = oracle_rows(server, jwt).await;
        if rows == expected {
            return rows;
        }
        assert!(
            Instant::now() < deadline,
            "Oracle did not converge to {expected} rows; observed {rows}"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Query the public Oracle endpoint without converting a fail-closed response.
///
/// # Panics
///
/// Panics when the HTTP request cannot be sent.
async fn oracle_status(server: &WyrdTestServer, jwt: &str) -> reqwest::StatusCode {
    let url = format!("{}/v1/query", server.base_url().expect("bound URL"));
    reqwest::Client::new()
        .post(url)
        .header("x-wyrd-access-token", format!("Bearer {jwt}"))
        .json(&SyncQueryRequest {
            sql: "SELECT * FROM \"vala.traces.spans\" WHERE service_name = 'checkout-api'"
                .to_owned(),
            params: Vec::new(),
        })
        .send()
        .await
        .expect("Oracle request")
        .status()
}

/// Append one real hash-chained Forge transition for Oracle fence classification.
///
/// The helper writes through `vala_sql::append_audit`, so the journey exercises
/// the production JSON operation-ID projection and latest-transition SQL rather
/// than only the pure classifier.
///
/// # Panics
///
/// Panics when a typed audit value is invalid or the tenant transaction cannot
/// append and commit the requested transition.
async fn append_oracle_fence_transition(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    operation_id: Uuid,
    operation: &str,
    phase: ForgeCompactionPhase,
) {
    let resource = format!("bifrost://{tenant}/vala.traces/spans");
    let input_file_ids = vec![Uuid::from_u128(1), Uuid::from_u128(2)];
    let detail = AuditDetail::ForgeCompaction {
        operation_id,
        phase,
        group: resource.clone(),
        input_file_ids,
        input_paths: vec![
            StoragePath::new("oracle-fence/input-1.parquet").expect("first input path"),
            StoragePath::new("oracle-fence/input-2.parquet").expect("second input path"),
        ],
        output_paths: vec![StoragePath::new("oracle-fence/output.parquet").expect("output path")],
        snapshot_id: None,
        writer_recipe_version: "bifrost-writer-v1".to_owned(),
    };
    let event = AuditEvent {
        request_id: RequestId::now_v7(),
        trace_id: None,
        operation: operation.to_owned(),
        resource,
        card_ref: None,
        principal_id: PrincipalId::new(Uuid::nil()),
        principal_kind: PrincipalKindTag::Service,
        auth_method: AuthMethod::Internal,
        permission: "bifrost:forge".to_owned(),
        decision: AuditDecision::Allow,
        result: AuditResult::Success,
        payload_summary: operation.to_owned(),
        detail: Some(detail),
    };
    let mut conn = server
        .state()
        .postgres
        .vala()
        .tenant_conn(tenant)
        .await
        .expect("Oracle fence tenant connection");
    vala_sql::queries::audit_outbox::append_audit(&mut conn, &event)
        .await
        .expect("append Oracle fence transition");
    conn.commit().await.expect("commit Oracle fence transition");
}
