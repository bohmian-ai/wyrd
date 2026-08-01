//! Distributed product journey for the real Scribe-to-Forge publication path.

use std::time::{Duration, Instant};

use secrecy::SecretString;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::forge::{ForgeScheduler, ForgeWorker, ForgeWorkerConfig};
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{AuditDetail, SyncQueryRequest};
use wyrd_testing::bifrost::BifrostHarness;
use wyrd_testing::bifrost::forge_harness::seed_forge_group;
use wyrd_testing::otlp::RandomTraceGenerator;
use wyrd_testing::{Bootstrap, WyrdTestServer};

/// One deployment and writer-concurrency shape exercised by the journey.
#[derive(Clone, Copy)]
struct Scenario {
    /// Diagnostic name included in assertion failures.
    name: &'static str,
    /// Number of bound Wyrd server processes sharing durable state.
    pods: usize,
    /// Number of isolated tenants writing concurrently.
    tenants: usize,
}

/// Product deployment shapes that Forge must support.
const SCENARIOS: [Scenario; 3] = [
    Scenario {
        name: "single-node-single-tenant",
        pods: 1,
        tenants: 1,
    },
    Scenario {
        name: "single-node-multi-tenant-multi-writer",
        pods: 1,
        tenants: 3,
    },
    Scenario {
        name: "multi-node-multi-tenant-multi-writer",
        pods: 3,
        tenants: 3,
    },
];

/// Number of independent flush cycles produced by every pod for every tenant.
///
/// Multiple cycles create independent Scribe files so active-day Forge
/// compaction is exercised instead of leaving a single open-tail file.
const WRITE_CYCLES: usize = 3;

/// Number of spans carried by each OTLP writer request.
const SPANS_PER_WRITE: usize = 6;

#[tokio::test]
#[ignore = "gated journey: real bound Wyrd servers, Postgres, Scribe, Forge, and Iceberg"]
/// Validate supported deployment shapes through the complete production path.
///
/// Every case drives concurrent authenticated OTLP writers into server-owned
/// Scribe instances, flushes through the normal durable seal path, competes
/// server-owned Forge handles over shared leases, and reads the resulting
/// Iceberg table through the public query API. The assertions cover exact
/// tenant row isolation, terminal audit reconciliation, ordered output paths,
/// snapshots, lease cleanup, and supervised shutdown.
///
/// # Panics
///
/// Panics when infrastructure cannot start or any production-path invariant
/// diverges or the supervised harness cannot shut down cleanly.
async fn forge_distributed_writer_matrix_preserves_rows_and_converges_once() {
    for scenario in SCENARIOS {
        run_scenario(scenario).await;
    }
}

/// Proves Forge fixture rebuilds retain the server-owned wall clock.
#[tokio::test]
#[ignore = "gated journey: real bound Wyrd server, Postgres, and Forge"]
async fn forge_fixture_rebuilds_retain_manual_forge_clock() {
    let server = WyrdTestServer::builder()
        .start_bound()
        .await
        .expect("server");
    let fixture = seed_forge_group(&server, "manual_forge_clock").await;
    let initial = fixture
        .forge
        .clock_for_test()
        .now()
        .expect("initial Forge clock");
    let advanced = server
        .forge_clock()
        .advance(chrono::Duration::seconds(1))
        .expect("advance Forge clock");
    let catalog = fixture.context_with_catalog(fixture.config.clone(), fixture.catalog.clone());
    assert!(advanced > initial);
    assert_eq!(
        catalog.clock_for_test().now().expect("catalog clock"),
        advanced
    );
    server.shutdown().await.expect("server shutdown");
}

/// Run one deployment shape from authenticated ingest through query readback.
///
/// # Panics
///
/// Panics when tenant provisioning, OTLP ingestion, Scribe flush, Forge
/// maintenance, durable inspection, or public query validation fails.
async fn run_scenario(scenario: Scenario) {
    let harness = BifrostHarness::start(scenario.pods, 1)
        .await
        .unwrap_or_else(|error| panic!("{} harness: {error}", scenario.name));
    let servers = harness.cluster().servers();
    assert_eq!(servers.len(), scenario.pods, "{} pod count", scenario.name);
    let control = &servers[0];
    let tenants = provision_tenants(control, scenario).await;
    assert_active_traces_roster(control, &tenants, scenario).await;

    for cycle in 0..WRITE_CYCLES {
        write_cycle(servers, &tenants, cycle, scenario).await;
        for server in servers {
            for tenant in &tenants {
                server
                    .flush_bifrost_for_tenant(tenant.id)
                    .await
                    .unwrap_or_else(|error| {
                        panic!("{} tenant {} flush: {error}", scenario.name, tenant.id)
                    });
            }
        }
    }
    for server in servers {
        server
            .forge_clock()
            .advance(chrono::Duration::days(1) + chrono::Duration::minutes(3))
            .unwrap_or_else(|error| panic!("{} advance Forge clock: {error}", scenario.name));
    }

    let expected_rows = u64::try_from((scenario.pods * WRITE_CYCLES + 2) * SPANS_PER_WRITE)
        .expect("bounded journey row count");
    let pending_before = pending_files(control, &tenants).await;
    assert_pending_files_are_old_enough(control, &tenants, scenario).await;
    assert!(
        pending_before
            .iter()
            .all(|(_, count)| *count >= i64::try_from(WRITE_CYCLES).expect("cycles fit i64")),
        "{} Scribe did not create independent durable files: {pending_before:?}",
        scenario.name
    );

    for tenant in &tenants {
        let response = query_response(control, &tenant.jwt).await;
        assert!(
            response.status().is_success(),
            "{} query must not use an Oracle publication fence: {}",
            scenario.name,
            response.status()
        );
    }
    let scheduler_owner = uuid::Uuid::now_v7();
    wait_for_compaction(servers, &tenants, scheduler_owner, scenario).await;
    let retention_writer = &servers[..1];
    for cycle in WRITE_CYCLES..(WRITE_CYCLES + 2) {
        write_cycle(retention_writer, &tenants, cycle, scenario).await;
        for server in retention_writer {
            for tenant in &tenants {
                server
                    .flush_bifrost_for_tenant(tenant.id)
                    .await
                    .unwrap_or_else(|error| {
                        panic!(
                            "{} tenant {} retained-history flush: {error}",
                            scenario.name, tenant.id
                        )
                    });
            }
        }
    }
    for server in servers {
        server
            .forge_clock()
            .advance(chrono::Duration::days(1) + chrono::Duration::minutes(3))
            .unwrap_or_else(|error| {
                panic!("{} advance retained-history clock: {error}", scenario.name)
            });
    }
    let tails = wait_for_compaction(servers, &tenants, scheduler_owner, scenario).await;
    for server in servers {
        let scribe = server.bifrost_scribe().expect("server-owned Scribe");
        scribe
            .retire_committed_for_test(std::time::Instant::now() + Duration::from_secs(120))
            .await
            .expect("Scribe retirement pass");
        let inspection = server
            .scribe_inspection_snapshot()
            .expect("Scribe inspection after retirement");
        assert_eq!(
            inspection.immutable_bucket_count, 0,
            "{} retained Scribe hot buckets after explicit retirement",
            scenario.name
        );
    }
    for tenant in &tenants {
        assert_eq!(
            wait_for_query_rows(control, &tenant.jwt, expected_rows, scenario).await,
            expected_rows,
            "{} tenant {} exact rows",
            scenario.name,
            tenant.id
        );
        if tails
            .iter()
            .any(|(tail_tenant, _, _)| *tail_tenant == tenant.id)
        {
            assert_no_compaction_audits(control, tenant.id, scenario).await;
        } else {
            assert_terminal_audits(control, tenant.id, scenario).await;
            assert_snapshot(control, tenant.id, scenario).await;
        }
    }
    assert_no_forge_leases(control, scenario).await;

    harness
        .shutdown()
        .await
        .unwrap_or_else(|error| panic!("{} shutdown: {error}", scenario.name));
}

/// Authenticated tenant identity used by concurrent writers and query checks.
struct TenantWriter {
    /// Durable tenant isolation key.
    id: DataTenantId,
    /// Tenant-scoped access token accepted by OTLP and query endpoints.
    jwt: String,
}

/// Provision the requested number of isolated tenants and writer identities.
///
/// # Panics
///
/// Panics when tenant, service-account, API-key, or access-token provisioning
/// fails.
async fn provision_tenants(server: &WyrdTestServer, scenario: Scenario) -> Vec<TenantWriter> {
    let mut tenant_ids = vec![server.data_tenant_id()];
    for index in 1..scenario.tenants {
        tenant_ids.push(
            server
                .seed_tenant(&format!("forge-distributed-{index}"))
                .await
                .unwrap_or_else(|error| panic!("{} seed tenant: {error}", scenario.name)),
        );
    }
    let mut tenants = Vec::with_capacity(tenant_ids.len());
    for (index, id) in tenant_ids.into_iter().enumerate() {
        server
            .ensure_traces_spans_table_for_test(id)
            .await
            .unwrap_or_else(|error| {
                panic!(
                    "{} provision traces spans table for {id}: {error}",
                    scenario.name
                )
            });
        let bootstrap = if index == 0 {
            server
                .bootstrap_service(&format!("forge-writer-{index}"), &["admin"])
                .await
        } else {
            server
                .bootstrap_service_in_tenant(id, &format!("forge-writer-{index}"), &["admin"])
                .await
        }
        .unwrap_or_else(|error| panic!("{} bootstrap tenant {id}: {error}", scenario.name));
        let api_key: SecretString = match bootstrap {
            Bootstrap::Machine { api_key, .. } => api_key,
            other => panic!(
                "{} expected machine bootstrap, got {other:?}",
                scenario.name
            ),
        };
        let jwt = server
            .exchange_api_key(&api_key)
            .await
            .unwrap_or_else(|error| panic!("{} exchange tenant {id}: {error}", scenario.name));
        tenants.push(TenantWriter { id, jwt });
    }
    tenants
}

/// Assert the scheduler's operator-visible roster contains every journey tenant.
///
/// This journey writes only the canonical traces spans table, so a row for each
/// tenant proves Forge will discover the intended table independently of
/// staging-file history.
///
/// # Panics
///
/// Panics when the operator roster query fails or a canonical tenant table is
/// missing.
async fn assert_active_traces_roster(
    server: &WyrdTestServer,
    tenants: &[TenantWriter],
    scenario: Scenario,
) {
    let operator_pool = server
        .state()
        .postgres
        .operator_pool()
        .expect("operator pool");
    let roster =
        vala_sql::queries::forge_catalog_operator::list_active_tables_for_operator(&operator_pool)
            .await
            .expect("active Bifrost table roster");
    for tenant in tenants {
        assert!(
            roster.iter().any(|row| {
                row.data_tenant_id == tenant.id.as_uuid() && row.fqn == "vala.traces.spans"
            }),
            "{} missing active traces roster entry for tenant {}: {roster:?}",
            scenario.name,
            tenant.id
        );
    }
}

/// Send one concurrent OTLP write from every pod for every tenant.
///
/// # Panics
///
/// Panics when a bound gRPC endpoint cannot connect or rejects an authenticated
/// export request.
async fn write_cycle(
    servers: &[WyrdTestServer],
    tenants: &[TenantWriter],
    cycle: usize,
    scenario: Scenario,
) {
    let mut writers = Vec::with_capacity(servers.len() * tenants.len());
    for (pod_index, server) in servers.iter().enumerate() {
        let endpoint = server.grpc_url().expect("bound pod gRPC URL");
        for (tenant_index, tenant) in tenants.iter().enumerate() {
            let endpoint = endpoint.clone();
            let jwt = tenant.jwt.clone();
            writers.push(tokio::spawn(async move {
                let deadline = Instant::now() + Duration::from_secs(10);
                let channel = loop {
                    match wyrd_tonic::tonic::transport::Channel::from_shared(endpoint.clone())
                        .expect("valid gRPC endpoint")
                        .connect()
                        .await
                    {
                        Ok(channel) => break channel,
                        Err(error) => {
                            assert!(Instant::now() < deadline, "connect OTLP writer: {error}");
                            tokio::time::sleep(Duration::from_millis(50)).await;
                        }
                    }
                };
                let seed =
                    u64::try_from((cycle + 1) * 10_000 + (pod_index + 1) * 100 + tenant_index)
                        .expect("bounded seed");
                let anchor = u64::try_from(
                    (chrono::Utc::now() - chrono::Duration::days(2))
                        .timestamp_nanos_opt()
                        .expect("timestamp nanos"),
                )
                .expect("positive timestamp");
                let mut generator = RandomTraceGenerator::from_seed_at(seed, anchor);
                let request = generator.export_request(pod_index, seed, SPANS_PER_WRITE);
                let mut request = wyrd_tonic::tonic::Request::new(request);
                request.metadata_mut().insert(
                    "x-wyrd-access-token",
                    format!("Bearer {jwt}").parse().expect("token metadata"),
                );
                let mut client =
                    wyrd_tonic::otlp::trace_service::trace_service_client::TraceServiceClient::new(
                        channel,
                    );
                let response = client
                    .export(request)
                    .await
                    .expect("OTLP export")
                    .into_inner();
                assert!(response.partial_success.is_none());
            }));
        }
    }
    for writer in writers {
        writer
            .await
            .unwrap_or_else(|error| panic!("{} writer task: {error}", scenario.name));
    }
}

/// Return uncompacted durable Scribe file counts for every tenant.
///
/// # Panics
///
/// Panics when shared Postgres inspection fails.
async fn pending_files(
    server: &WyrdTestServer,
    tenants: &[TenantWriter],
) -> Vec<(DataTenantId, i64)> {
    let operator_pool = server
        .state()
        .postgres
        .operator_pool()
        .expect("operator pool");
    let pool = operator_pool.pool();
    let mut counts = Vec::with_capacity(tenants.len());
    for tenant in tenants {
        let count = sqlx::query_scalar(
            "SELECT count(*) FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = 'vala.traces' AND table_name = 'spans' AND NOT compacted",
        )
        .bind(tenant.id.as_uuid())
        .fetch_one(pool)
        .await
        .expect("pending Scribe files");
        counts.push((tenant.id, count));
    }
    counts
}

/// Return the remaining logical staging tails for the journey's canonical table.
///
/// A tail is the at-most-one uncompacted file for one tenant, table, and
/// partition day after bounded Forge convergence.
///
/// # Panics
///
/// Panics when shared Postgres inspection fails.
async fn pending_tail_groups(
    server: &WyrdTestServer,
    tenants: &[TenantWriter],
) -> Vec<(DataTenantId, chrono::NaiveDate, i64)> {
    let operator_pool = server
        .state()
        .postgres
        .operator_pool()
        .expect("operator pool");
    let mut tails = Vec::new();
    for tenant in tenants {
        let rows = sqlx::query_as::<_, (chrono::NaiveDate, i64)>(
            "SELECT partition_day, count(*) FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = 'vala.traces' AND table_name = 'spans' AND NOT compacted GROUP BY partition_day ORDER BY partition_day",
        )
        .bind(tenant.id.as_uuid())
        .fetch_all(operator_pool.pool())
        .await
        .expect("pending staging tails");
        tails.extend(
            rows.into_iter()
                .map(|(partition_day, files)| (tenant.id, partition_day, files)),
        );
    }
    tails
}

/// Assert the manual Forge clock has moved past the staging age guard.
///
/// # Panics
///
/// Panics when shared Postgres inspection fails or a journey file remains too
/// new for periodic Forge discovery.
async fn assert_pending_files_are_old_enough(
    server: &WyrdTestServer,
    tenants: &[TenantWriter],
    scenario: Scenario,
) {
    let cutoff = server.forge_clock().now().expect("Forge clock") - chrono::Duration::minutes(2);
    let operator_pool = server
        .state()
        .postgres
        .operator_pool()
        .expect("operator pool");
    for tenant in tenants {
        let newest: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
            "SELECT max(created_at) FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = 'vala.traces' AND table_name = 'spans' AND NOT compacted",
        )
        .bind(tenant.id.as_uuid())
        .fetch_one(operator_pool.pool())
        .await
        .expect("pending file age");
        assert!(
            newest.is_some_and(|created_at| created_at < cutoff),
            "{} tenant {} pending files are younger than the Forge cutoff {cutoff}: {newest:?}",
            scenario.name,
            tenant.id
        );
    }
}

/// Wait until every tenant's durable files have a committed Forge snapshot.
///
/// # Panics
///
/// Panics when SQL inspection fails or the bounded convergence deadline
/// expires.
async fn wait_for_compaction(
    servers: &[WyrdTestServer],
    tenants: &[TenantWriter],
    scheduler_owner: uuid::Uuid,
    scenario: Scenario,
) -> Vec<(DataTenantId, chrono::NaiveDate, i64)> {
    let control = servers.first().expect("at least one Forge server");
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        assert!(
            Instant::now() < deadline,
            "{} Forge did not report a complete converged tick",
            scenario.name
        );
        drive_forge_pass(control, servers, scheduler_owner, scenario).await;
        let tails = pending_tail_groups(control, tenants).await;
        if tails.iter().all(|(_, _, files)| *files <= 1) {
            return tails;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Runs one durable planning pass and drains claims across every worker process.
///
/// # Panics
///
/// Panics when planning, worker construction, claiming, or exact task execution fails.
async fn drive_forge_pass(
    control: &WyrdTestServer,
    servers: &[WyrdTestServer],
    scheduler_owner: uuid::Uuid,
    scenario: Scenario,
) {
    let stop = CancellationToken::new();
    let forge = control
        .state()
        .forge()
        .expect("server-owned Forge scheduler");
    ForgeScheduler::with_owner_for_test(forge, scheduler_owner)
        .expect("journey Forge scheduler")
        .schedule_once(&stop)
        .await
        .unwrap_or_else(|error| panic!("{} Forge planning pass: {error}", scenario.name));
    let workers = servers
        .iter()
        .map(|server| {
            let forge = server
                .state()
                .forge()
                .expect("server-owned Forge worker")
                .clone();
            let stop = stop.clone();
            tokio::spawn(async move {
                let worker = ForgeWorker::new(
                    forge,
                    ForgeWorkerConfig {
                        worker_concurrency: 1,
                    },
                )?;
                while worker.execute_one_for_test(&stop).await? {}
                Ok::<(), vala_bifrost_redux::forge::ForgeError>(())
            })
        })
        .collect::<Vec<_>>();
    for worker in workers {
        worker
            .await
            .expect("Forge worker task")
            .unwrap_or_else(|error| panic!("{} Forge worker drain: {error}", scenario.name));
    }
}

/// Assert an accepted open-partition tail has not created durable Forge work.
///
/// # Panics
///
/// Panics when audit inspection fails or Forge recorded a compaction operation
/// for a tail that the planner accepted without a rewrite.
async fn assert_no_compaction_audits(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    scenario: Scenario,
) {
    let mut conn = server
        .state()
        .postgres
        .vala()
        .tenant_conn(tenant)
        .await
        .expect("Forge audit tenant connection");
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id = $1 AND operation LIKE 'forge.file_compact.%'",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("Forge audit count");
    assert_eq!(
        count, 0,
        "{} accepted tail for tenant {tenant} recorded Forge compaction work",
        scenario.name
    );
}

/// Send the public same-table query used before and after Forge publication.
///
/// # Panics
///
/// Panics when the HTTP request cannot be sent.
async fn query_response(server: &WyrdTestServer, jwt: &str) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!(
            "{}/v1/query",
            server.base_url().expect("bound server URL")
        ))
        .header("x-wyrd-access-token", format!("Bearer {jwt}"))
        .json(&SyncQueryRequest {
            sql: "SELECT * FROM \"vala.traces.spans\" WHERE service_name = 'checkout-api'"
                .to_owned(),
            params: Vec::new(),
        })
        .send()
        .await
        .expect("public query request")
}

/// Return the exact row count from the public same-table query.
///
/// # Panics
///
/// Panics when the query fails or omits its row-count header.
async fn query_rows(server: &WyrdTestServer, jwt: &str) -> u64 {
    let response = query_response(server, jwt)
        .await
        .error_for_status()
        .expect("public query response");
    response
        .headers()
        .get("x-wyrd-row-count")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
        .expect("row-count header")
}

/// Wait for Scribe's committed-generation grace window to retire overlap.
///
/// # Panics
///
/// Panics when the public query does not converge to the exact expected row
/// count before the bounded deadline.
async fn wait_for_query_rows(
    server: &WyrdTestServer,
    jwt: &str,
    expected: u64,
    scenario: Scenario,
) -> u64 {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let rows = query_rows(server, jwt).await;
        if rows == expected {
            return rows;
        }
        assert!(
            Instant::now() < deadline,
            "{} query did not retire Scribe/Iceberg overlap: expected {expected}, observed {rows}",
            scenario.name
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Assert every prepared operation has one terminal event and ordered outputs.
///
/// # Panics
///
/// Panics when audit inspection fails, transitions do not reconcile, or a
/// terminal compaction lacks deterministically ordered output paths.
async fn assert_terminal_audits(server: &WyrdTestServer, tenant: DataTenantId, scenario: Scenario) {
    let mut conn = server
        .state()
        .postgres
        .vala()
        .tenant_conn(tenant)
        .await
        .expect("Forge audit tenant connection");
    let rows = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT operation, detail FROM vala.audit_outbox WHERE data_tenant_id = $1 AND operation LIKE 'forge.file_compact.%' ORDER BY created_at",
    )
    .bind(tenant.as_uuid())
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("Forge audit rows");
    let prepared = rows
        .iter()
        .filter(|(operation, _)| operation == "forge.file_compact.prepared")
        .count();
    let terminal = rows
        .iter()
        .filter(|(operation, _)| {
            matches!(
                operation.as_str(),
                "forge.file_compact.committed"
                    | "forge.file_compact.recovered"
                    | "forge.file_compact.reset"
            )
        })
        .count();
    assert!(prepared > 0, "{} missing prepared audit", scenario.name);
    assert_eq!(prepared, terminal, "{} terminal audit count", scenario.name);
    for (_, detail) in rows.iter().filter(|(operation, _)| {
        matches!(
            operation.as_str(),
            "forge.file_compact.committed" | "forge.file_compact.recovered"
        )
    }) {
        let detail: AuditDetail =
            serde_json::from_str(detail.as_deref().expect("terminal Forge audit detail"))
                .expect("typed Forge audit detail");
        let AuditDetail::ForgeCompaction { output_paths, .. } = detail else {
            panic!("{} unexpected Forge audit detail", scenario.name);
        };
        let paths = output_paths
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        assert!(
            paths.windows(2).all(|pair| pair[0] < pair[1]),
            "{} output_paths must be ordered: {paths:?}",
            scenario.name
        );
    }
}

/// Assert the tenant's traces table has at least one committed snapshot.
///
/// # Panics
///
/// Panics when the Redux catalog or table cannot be loaded.
async fn assert_snapshot(server: &WyrdTestServer, tenant: DataTenantId, scenario: Scenario) {
    let binding = vala_bifrost_redux::catalog::TenantTableBinding::resolve((
        tenant,
        vala_bifrost_redux::catalog::TableRef::new(
            vala_bifrost_redux::namespaces::BifrostNamespace::Traces,
            "spans",
        ),
    ))
    .expect("traces binding");
    let table = server
        .state()
        .bifrost_redux
        .as_ref()
        .expect("Redux catalog")
        .iceberg_catalog()
        .load_table(&binding.table_ident())
        .await
        .unwrap_or_else(|error| panic!("{} load tenant {tenant} table: {error}", scenario.name));
    assert!(
        table.metadata().current_snapshot_id().is_some(),
        "{} tenant {tenant} missing snapshot",
        scenario.name
    );
}

/// Assert no Forge table lease remains after the scenario converges.
///
/// # Panics
///
/// Panics when durable lease inspection fails.
async fn assert_no_forge_leases(server: &WyrdTestServer, scenario: Scenario) {
    let leases: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.maintenance_leases WHERE lease_key LIKE 'forge:table:%'",
    )
    .fetch_one(
        server
            .state()
            .postgres
            .operator_pool()
            .expect("operator pool")
            .pool(),
    )
    .await
    .expect("Forge lease count");
    assert_eq!(leases, 0, "{} stale Forge leases", scenario.name);
}
