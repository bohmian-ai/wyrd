//! Distributed product journey for the real Scribe-to-Forge publication path.

use std::time::{Duration, Instant};

use iceberg::transaction::{ApplyTransactionAction, Transaction};
use secrecy::SecretString;
use vala_bifrost_redux::catalog::{TableRef, TenantTableBinding};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_sdk::QueryClient;
use wyrd_client::WyrdClient;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{AuditDetail, BifrostQueryRequest, FreshnessPolicy, VisibilityMode};
use wyrd_testing::bifrost::{BifrostTopology, WyrdTestCluster};
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

/// Run one deployment shape from authenticated ingest through query readback.
///
/// # Panics
///
/// Panics when tenant provisioning, OTLP ingestion, Scribe flush, Forge
/// maintenance, durable inspection, or public query validation fails.
async fn run_scenario(scenario: Scenario) {
    let topology = if scenario.pods == 1 {
        BifrostTopology::OnePod
    } else {
        BifrostTopology::ThreePod
    };
    let cluster = WyrdTestCluster::start(scenario.pods, topology)
        .await
        .unwrap_or_else(|error| panic!("{} harness: {error}", scenario.name));
    let servers = cluster.servers().collect::<Vec<_>>();
    assert_eq!(servers.len(), scenario.pods, "{} pod count", scenario.name);
    let control = &servers[0];
    let tenants = provision_tenants(control, scenario).await;

    for cycle in 0..WRITE_CYCLES {
        write_cycle(&servers, &tenants, cycle, scenario).await;
        for server in &servers {
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

    let expected_rows = u64::try_from(scenario.pods * WRITE_CYCLES * SPANS_PER_WRITE)
        .expect("bounded journey row count");
    let pending_before = pending_files(control, &tenants).await;
    assert!(
        pending_before
            .iter()
            .all(|(_, count)| *count >= i64::try_from(WRITE_CYCLES).expect("cycles fit i64")),
        "{} Scribe did not create independent durable files: {pending_before:?}",
        scenario.name
    );
    wait_for_scribe_retirement(&servers, scenario).await;
    for tenant in &tenants {
        let rows = query_rows(&tenant.query).await;
        assert!(
            rows > 0,
            "{} PublishedOnly Oracle query must see sealed rows before compaction",
            scenario.name,
        );
    }
    make_pending_files_periodically_eligible(control, &tenants).await;
    configure_active_day_targets(control, &tenants).await;
    let ticks = servers
        .iter()
        .map(|server| {
            let forge = server.state().forge().expect("server-owned Forge").clone();
            tokio::spawn(async move { forge.run_once().await })
        })
        .collect::<Vec<_>>();
    for tick in ticks {
        tick.await
            .expect("Forge tick task")
            .unwrap_or_else(|error| panic!("{} Forge tick: {error}", scenario.name));
    }

    wait_for_compaction(&servers, &tenants, scenario).await;
    for tenant in &tenants {
        assert_eq!(
            wait_for_query_rows(&tenant.query, expected_rows, scenario).await,
            expected_rows,
            "{} tenant {} exact rows",
            scenario.name,
            tenant.id
        );
        assert_terminal_audits(control, tenant.id, scenario).await;
        assert_snapshot(control, tenant.id, scenario).await;
    }
    assert_no_forge_leases(control, scenario).await;

    drop(servers);
    cluster
        .shutdown()
        .await
        .unwrap_or_else(|error| panic!("{} shutdown: {error}", scenario.name));
}

/// Wait until every Scribe releases the immutable overlap generations.
///
/// # Panics
///
/// Panics when Scribe inspection fails or the fixture's bounded retention
/// grace does not retire every immutable generation before the deadline.
async fn wait_for_scribe_retirement(servers: &[&WyrdTestServer], scenario: Scenario) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        for server in servers {
            server
                .bifrost_scribe()
                .expect("server-owned Scribe")
                .check_age(std::time::Instant::now() + Duration::from_secs(120));
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
        let remaining = servers
            .iter()
            .map(|server| {
                server
                    .scribe_inspection_snapshot()
                    .expect("Scribe retirement inspection")
                    .immutable_bucket_count
            })
            .sum::<usize>();
        if remaining == 0 {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{} Scribe retained {remaining} immutable generations",
            scenario.name
        );
    }
}

/// Authenticated tenant identity used by concurrent writers and query checks.
struct TenantWriter {
    /// Durable tenant isolation key.
    id: DataTenantId,
    /// Tenant-scoped access token accepted by OTLP and query endpoints.
    jwt: String,
    /// Tenant-scoped public SDK client used for terminal-safe Oracle queries.
    query: WyrdClient,
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
        let query = WyrdClient::with_config(ClientConfig {
            grpc: GrpcConfig {
                endpoint: server.grpc_url().expect("bound gRPC endpoint"),
                connect_retries: 0,
                ..GrpcConfig::default()
            },
            http: HttpConfig {
                base_url: server.base_url().expect("bound HTTP endpoint").to_owned(),
                ..HttpConfig::default()
            },
            api_key: Some(api_key),
            ..ClientConfig::default()
        })
        .unwrap_or_else(|error| panic!("{} query client for {id}: {error}", scenario.name));
        tenants.push(TenantWriter { id, jwt, query });
    }
    tenants
}

/// Send one concurrent OTLP write from every pod for every tenant.
///
/// # Panics
///
/// Panics when a bound gRPC endpoint cannot connect or rejects an authenticated
/// export request.
async fn write_cycle(
    servers: &[&WyrdTestServer],
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
                    chrono::Utc::now()
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

/// Move this fixture's staged files beyond Forge's periodic discovery age guard.
///
/// The journey invokes `Forge::run_once`, which intentionally exercises periodic
/// discovery rather than the asynchronous hint fast path. Backdating only the
/// freshly created tenant rows establishes that production-path precondition
/// without sleeping for the two-minute safety window.
///
/// # Panics
///
/// Panics when the shared operator connection cannot update all fixture rows.
async fn make_pending_files_periodically_eligible(
    server: &WyrdTestServer,
    tenants: &[TenantWriter],
) {
    let operator_pool = server
        .state()
        .postgres
        .operator_pool()
        .expect("operator pool");
    for tenant in tenants {
        sqlx::query(
            "UPDATE vala.file_list SET created_at = now() - interval '3 min' WHERE data_tenant_id = $1 AND namespace = 'vala.traces' AND table_name = 'spans' AND NOT compacted",
        )
        .bind(tenant.id.as_uuid())
        .execute(operator_pool.pool())
        .await
        .expect("backdate periodic Forge candidates");
    }
}

/// Set each fixture table's target to its exact staged-byte total.
///
/// Forge retains a below-target trailing group on the active day. This
/// production property update makes the complete fixture group eligible while
/// preserving its real partition and event-time identities.
///
/// # Panics
///
/// Panics when staged totals, table loading, property validation, or the
/// Iceberg metadata commit fails.
async fn configure_active_day_targets(server: &WyrdTestServer, tenants: &[TenantWriter]) {
    let operator_pool = server
        .state()
        .postgres
        .operator_pool()
        .expect("operator pool");
    let catalog = server
        .state()
        .bifrost_redux
        .as_ref()
        .expect("Bifrost Redux catalog")
        .iceberg_catalog();
    for tenant in tenants {
        let staged_bytes: i64 = sqlx::query_scalar(
            "SELECT sum(file_size)::bigint FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = 'vala.traces' AND table_name = 'spans' AND NOT compacted",
        )
        .bind(tenant.id.as_uuid())
        .fetch_one(operator_pool.pool())
        .await
        .expect("staged byte total");
        assert!(staged_bytes > 0, "fixture staged bytes must be positive");
        let binding = TenantTableBinding::resolve((
            tenant.id,
            TableRef::new(BifrostNamespace::Traces, "spans"),
        ))
        .expect("spans binding");
        let table = catalog
            .load_table(&binding.table_ident())
            .await
            .expect("load spans table");
        let action = Transaction::new(&table).update_table_properties().set(
            "write.target-file-size-bytes".to_owned(),
            staged_bytes.to_string(),
        );
        ApplyTransactionAction::apply(action, Transaction::new(&table))
            .expect("target property action")
            .commit(catalog.as_ref())
            .await
            .expect("target property commit");
    }
}

/// Wait until every tenant's durable files have a committed Forge snapshot.
///
/// # Panics
///
/// Panics when SQL inspection fails or the bounded convergence deadline
/// expires.
async fn wait_for_compaction(
    servers: &[&WyrdTestServer],
    tenants: &[TenantWriter],
    scenario: Scenario,
) {
    let control = servers.first().expect("at least one Forge server");
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let pending = pending_files(control, tenants).await;
        if pending.iter().all(|(_, count)| *count == 0) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{} Forge did not compact every tenant: {pending:?}",
            scenario.name
        );
        let ticks = servers
            .iter()
            .map(|server| {
                let forge = server.state().forge().expect("server-owned Forge").clone();
                tokio::spawn(async move { forge.run_once().await })
            })
            .collect::<Vec<_>>();
        for tick in ticks {
            tick.await
                .expect("Forge convergence task")
                .unwrap_or_else(|error| {
                    panic!("{} Forge convergence tick: {error}", scenario.name)
                });
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Return the exact row count from the public terminal-safe Oracle query.
///
/// # Panics
///
/// Panics when the query, Arrow stream, or validated terminal fails.
async fn query_rows(client: &WyrdClient) -> u64 {
    let request = BifrostQueryRequest {
        sql: "SELECT * FROM vala.traces.spans".to_owned(),
        visibility: VisibilityMode::PublishedOnly,
        freshness: FreshnessPolicy::Strict,
        deadline_ms: None,
    };
    let mut stream = QueryClient::new(client)
        .query(&request)
        .await
        .expect("public Oracle query");
    let mut rows = 0_u64;
    while let Some(batch) = stream.next_batch().await.expect("Oracle query batch") {
        rows = rows.saturating_add(u64::try_from(batch.num_rows()).expect("batch rows fit u64"));
    }
    let terminal = stream.terminal().expect("validated Oracle terminal");
    assert_eq!(terminal.row_count, rows, "terminal row count");
    rows
}

/// Wait for Scribe's committed-generation grace window to retire overlap.
///
/// # Panics
///
/// Panics when the public query does not converge to the exact expected row
/// count before the bounded deadline.
async fn wait_for_query_rows(client: &WyrdClient, expected: u64, scenario: Scenario) -> u64 {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let rows = query_rows(client).await;
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
