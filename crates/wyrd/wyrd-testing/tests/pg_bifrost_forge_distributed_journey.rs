//! Distributed product journey for the real Scribe-to-Forge publication path.

use std::time::{Duration, Instant};

use secrecy::SecretString;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::forge::{ForgeScheduler, ForgeWorker, ForgeWorkerConfig};
use vala_sql::queries::forge_tasks::ForgeTasks;
use vala_sql::row_types::forge_tasks::{ForgeTaskStrategy, ForgeTaskTableIdentity};
use wyrd_server::config::ForgeProcessRole;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{AuditDetail, SyncQueryRequest};
use wyrd_testing::bifrost::forge_harness::seed_forge_group;
use wyrd_testing::bifrost::{
    BifrostHarness, BifrostQueryTelemetryReport, BifrostTopology, WyrdTestCluster,
};
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
        tenants: 10,
    },
    Scenario {
        name: "multi-node-multi-tenant-multi-writer",
        pods: 3,
        tenants: 10,
    },
];

/// Number of independent flush cycles produced by every pod for every tenant.
///
/// Multiple cycles create independent Scribe files so active-day Forge
/// compaction is exercised instead of leaving a single open-tail file.
const WRITE_CYCLES: usize = 3;

/// Number of spans carried by each OTLP writer request.
const SPANS_PER_WRITE: usize = 6;

/// Stable cross-topology evidence from one complete supervised Forge fixture.
#[derive(Debug, PartialEq, Eq)]
struct RoleTopologyEvidence {
    /// Exact public query row counts in provisioned tenant order.
    rows: Vec<u64>,
    /// Terminal Forge task counts in the same tenant order.
    terminal_tasks: Vec<i64>,
    /// Terminal Forge audit counts in the same tenant order.
    terminal_audits: Vec<i64>,
    /// Terminal task rows retaining exact commit evidence in the same tenant order.
    evidence_rows: Vec<i64>,
    /// Terminal task rows retaining active watermark state in the same tenant order.
    watermark_rows: Vec<i64>,
    /// Remaining table-scoped Forge leases after Scribe retirement.
    cleanup_leases: i64,
}

/// Typed server-integrated query artifact joined with Forge reports by scenario.
#[derive(serde::Serialize)]
struct QueryTelemetryArtifact {
    /// Immutable process and tenant identity shared with the Forge benchmark.
    scenario: QueryScenarioIdentity,
    /// Production query histogram mapped by the server-only report owner.
    query: BifrostQueryTelemetryReport,
}

/// Exact scenario join key shared by independent maintenance and query artifacts.
#[derive(serde::Serialize)]
struct QueryScenarioIdentity {
    /// Number of serving/scheduling pods in the integrated journey.
    pods: usize,
    /// Number of isolated tenants queried in the integrated journey.
    tenants: usize,
}

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

/// Prove the production worker-only role shares dependencies without serving APIs.
///
/// The cluster starts the same production `WyrdServer` supervisor used by the
/// bound journey: one `server` process owns the public API and scheduler while
/// three `forge-worker` processes join the shared durable Forge work pool. The
/// journey writes and queries through the sole server endpoint, then uses a
/// deterministic claim barrier to make all three worker-only processes claim
/// independent tenant tasks through the production worker loop. Listener
/// assertions prevent a worker role from silently becoming a second serving
/// surface.
///
/// # Panics
///
/// Panics when the real shared Postgres/catalog/storage topology cannot start,
/// a process role differs from its requested composition, a worker binds an
/// API listener, or shutdown cannot complete.
#[tokio::test]
#[ignore = "gated journey: real server role topology, Postgres, and shared Forge dependencies"]
async fn dedicated_forge_workers_share_dependencies_without_public_listeners() {
    let cluster = WyrdTestCluster::start_with_dedicated_forge_workers()
        .await
        .expect("dedicated Forge worker cluster");
    assert_eq!(cluster.topology(), BifrostTopology::DedicatedForgeWorkers);
    let roles = cluster
        .servers()
        .iter()
        .map(WyrdTestServer::forge_process_role)
        .collect::<Vec<_>>();
    assert_eq!(
        roles,
        vec![
            ForgeProcessRole::Server,
            ForgeProcessRole::ForgeWorker,
            ForgeProcessRole::ForgeWorker,
            ForgeProcessRole::ForgeWorker,
        ]
    );
    let scheduler_server = cluster.server(0).expect("scheduler/server pod");
    assert!(scheduler_server.base_url().is_some());
    assert!(scheduler_server.grpc_url().is_some());
    for worker in &cluster.servers()[1..] {
        assert_eq!(worker.base_url(), None);
        assert_eq!(worker.grpc_url(), None);
        assert_eq!(worker.bound_addr(), None);
    }
    let completion = cluster
        .forge_completion_observer()
        .expect("dedicated worker completion observer");
    let scenario = Scenario {
        name: "dedicated-forge-workers",
        pods: 1,
        tenants: 3,
    };
    let tenants = provision_tenants(scheduler_server, scenario).await;
    assert_active_traces_roster(scheduler_server, &tenants, scenario).await;
    for cycle in 0..WRITE_CYCLES {
        write_cycle(
            std::slice::from_ref(scheduler_server),
            &tenants,
            cycle,
            scenario,
        )
        .await;
        for tenant in &tenants {
            scheduler_server
                .flush_bifrost_for_tenant(tenant.id)
                .await
                .expect("dedicated scheduler/server flush");
        }
    }
    scheduler_server
        .forge_clock()
        .advance(chrono::Duration::days(1) + chrono::Duration::minutes(3))
        .expect("advance scheduler Forge clock");
    completion.hold_after_claims_for_test(3);
    trigger_supervised_scheduler(scheduler_server, scenario).await;
    tokio::time::timeout(
        Duration::from_secs(10),
        completion.wait_for_claims_for_test(),
    )
    .await
    .expect("each worker-only process claimed one independent durable task");
    assert_claim_barrier_shape(scheduler_server, 3, scenario).await;
    completion.release_claims_for_test();
    tokio::time::timeout(
        Duration::from_secs(10),
        completion.wait_for_distinct_workers_at_least(3),
    )
    .await
    .expect("all three worker-only processes completed durable work");
    let scribe = scheduler_server
        .bifrost_scribe()
        .expect("server-role Scribe");
    scribe
        .retire_committed_for_test(std::time::Instant::now() + Duration::from_secs(120))
        .await
        .expect("retire committed Scribe generations");
    let expected_rows =
        u64::try_from(WRITE_CYCLES * SPANS_PER_WRITE).expect("bounded dedicated journey row count");
    for tenant in &tenants {
        assert_eq!(
            query_rows(scheduler_server, &tenant.jwt).await,
            expected_rows,
            "dedicated worker tenant {} rows",
            tenant.id
        );
        assert_terminal_audits(scheduler_server, tenant.id, scenario).await;
        assert_snapshot(scheduler_server, tenant.id, scenario).await;
    }
    assert_no_forge_leases(scheduler_server, scenario).await;
    cluster
        .shutdown()
        .await
        .expect("dedicated cluster shutdown");
}

/// Compare embedded and dedicated supervised roles through identical tenant work.
///
/// # Panics
///
/// Panics when either real topology diverges in public rows or its durable
/// task, audit, evidence, watermark, or cleanup footprint.
#[tokio::test]
#[ignore = "gated journey: paired real Forge role topologies, Postgres, and shared Iceberg dependencies"]
async fn embedded_and_dedicated_forge_roles_preserve_exact_durable_parity() {
    let embedded = WyrdTestCluster::start_with_embedded_forge_observer()
        .await
        .expect("embedded Forge cluster");
    assert_eq!(
        embedded
            .server(0)
            .expect("embedded server")
            .forge_process_role(),
        ForgeProcessRole::All
    );
    let embedded_evidence = run_supervised_role_fixture(&embedded, 1, "embedded-forge-role").await;
    embedded
        .shutdown()
        .await
        .expect("embedded cluster shutdown");

    let dedicated = WyrdTestCluster::start_with_dedicated_forge_workers()
        .await
        .expect("dedicated Forge cluster");
    assert_eq!(dedicated.topology(), BifrostTopology::DedicatedForgeWorkers);
    assert!(dedicated.servers()[1..].iter().all(|worker| {
        worker.forge_process_role() == ForgeProcessRole::ForgeWorker
            && worker.base_url().is_none()
            && worker.grpc_url().is_none()
            && worker.bound_addr().is_none()
    }));
    let dedicated_evidence =
        run_supervised_role_fixture(&dedicated, 3, "dedicated-forge-role").await;
    dedicated
        .shutdown()
        .await
        .expect("dedicated cluster shutdown");

    assert_eq!(embedded_evidence, dedicated_evidence);
}

/// Prove a real dedicated worker loss is reclaimed without duplicate public rows.
///
/// One worker stops only after PostgreSQL has persisted its claim. The test
/// invokes the production planner once, then lets only supervised worker roles
/// claim, lose, reclaim, publish, and retire the work.
///
/// # Panics
///
/// Panics when the supervised role topology does not reclaim the lost claim,
/// creates duplicate rows, or leaves task/audit/evidence/lease state divergent.
#[tokio::test]
#[ignore = "gated journey: supervised dedicated Forge worker loss and lease reclaim"]
async fn supervised_dedicated_roles_reclaim_lost_worker_without_duplicate_rows() {
    let cluster = WyrdTestCluster::start_with_dedicated_forge_workers()
        .await
        .expect("dedicated Forge cluster");
    let server = cluster.server(0).expect("scheduler server");
    let completion = cluster
        .forge_completion_observer()
        .expect("shared supervised observer");
    let scenario = Scenario {
        name: "supervised-worker-reclaim",
        pods: 1,
        tenants: 3,
    };
    let tenants = provision_tenants(server, scenario).await;
    assert_active_traces_roster(server, &tenants, scenario).await;
    for cycle in 0..WRITE_CYCLES {
        write_cycle(std::slice::from_ref(server), &tenants, cycle, scenario).await;
        for tenant in &tenants {
            server
                .flush_bifrost_for_tenant(tenant.id)
                .await
                .expect("supervised loss fixture flush");
        }
    }
    completion.abandon_next_claim_for_test();
    server
        .forge_clock()
        .advance(chrono::Duration::days(1) + chrono::Duration::minutes(3))
        .expect("age staged files for production planner");
    trigger_supervised_scheduler(server, scenario).await;
    tokio::time::timeout(
        Duration::from_secs(10),
        completion.wait_for_abandoned_claim_for_test(),
    )
    .await
    .expect("one real worker stopped after its durable claim");
    let (lost_task, lost_attempt, lost_owner) = completion.abandoned_claim_identity_for_test();
    let expired: uuid::Uuid = sqlx::query_scalar(
        "UPDATE vala.forge_tasks SET claim_expires_at = statement_timestamp() - interval '1 millisecond' WHERE task_id = $1 AND attempt_id = $2 AND claimed_by = $3 AND state = 'claimed' RETURNING task_id",
    )
    .bind(lost_task)
    .bind(lost_attempt)
    .bind(lost_owner)
    .fetch_one(
        server
            .state()
            .postgres
            .operator_pool()
            .expect("operator pool")
            .pool(),
    )
    .await
    .expect("expire the exact abandoned durable claim");
    assert_eq!(expired, lost_task, "only the stopped claim may expire");
    let staged_completion = tokio::time::timeout(
        Duration::from_secs(15),
        completion.wait_for_strategy_at_least(ForgeTaskStrategy::StagingFold, 3),
    )
    .await;
    if staged_completion.is_err() {
        panic!(
            "remaining supervised workers did not reclaim staged tasks: {:?}",
            forge_task_diagnostics(server).await
        );
    }

    server
        .bifrost_scribe()
        .expect("server-role Scribe")
        .retire_committed_for_test(std::time::Instant::now() + Duration::from_secs(120))
        .await
        .expect("retire committed Scribe generations");
    let expected_rows =
        u64::try_from(WRITE_CYCLES * SPANS_PER_WRITE).expect("bounded reclaim rows");
    for tenant in &tenants {
        assert_terminal_audits(server, tenant.id, scenario).await;
        assert_snapshot(server, tenant.id, scenario).await;
        let mut conn = server
            .state()
            .postgres
            .vala()
            .tenant_conn(tenant.id)
            .await
            .expect("reclaim tenant connection");
        let (terminal, evidence): (i64, i64) = sqlx::query_as(
            "SELECT count(*), count(*) FILTER (WHERE evidence IS NOT NULL) FROM vala.forge_tasks WHERE data_tenant_id = $1 AND strategy = 'staging_fold' AND state = 'succeeded'",
        )
        .bind(tenant.id.as_uuid())
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("reclaimed terminal task evidence");
        assert_eq!(
            terminal, 1,
            "tenant {} has one terminal reclaimed task",
            tenant.id
        );
        assert_eq!(
            evidence, 1,
            "tenant {} retained exact task evidence",
            tenant.id
        );
        assert_eq!(
            supervised_reclaim_query_rows(server, &tenant.jwt).await,
            expected_rows,
            "reclaimed tenant {} public rows after terminal={terminal}, evidence={evidence}",
            tenant.id
        );
    }
    assert_no_forge_leases(server, scenario).await;
    cluster
        .shutdown()
        .await
        .expect("dedicated cluster shutdown");
}

/// Prove real dedicated workers reconcile a post-commit uncertain publication exactly once.
///
/// # Panics
///
/// Panics when the catalog boundary is not reached, prepared recovery does not
/// terminalize through a supervised worker, or public/durable results duplicate.
#[tokio::test]
#[ignore = "gated journey: supervised dedicated Forge uncertain-commit recovery"]
async fn supervised_dedicated_roles_recover_uncertain_commit_without_duplicate_rows() {
    let cluster = WyrdTestCluster::start_with_dedicated_forge_workers_with_uncertainty_for_test()
        .await
        .expect("dedicated uncertain Forge cluster");
    let server = cluster.server(0).expect("scheduler server");
    let completion = cluster
        .forge_completion_observer()
        .expect("shared supervised observer");
    let control = cluster
        .commit_uncertainty_catalog()
        .expect("shared commit uncertainty catalog");
    let scenario = Scenario {
        name: "supervised-uncertain-commit",
        pods: 1,
        tenants: 1,
    };
    let tenants = provision_tenants(server, scenario).await;
    for cycle in 0..WRITE_CYCLES {
        write_cycle(std::slice::from_ref(server), &tenants, cycle, scenario).await;
        server
            .flush_bifrost_for_tenant(tenants[0].id)
            .await
            .expect("uncertain fixture flush");
    }
    server
        .forge_clock()
        .advance(chrono::Duration::days(1) + chrono::Duration::minutes(3))
        .expect("age uncertain fixture files");
    control.pause_after_commit();
    trigger_supervised_scheduler(server, scenario).await;
    tokio::time::timeout(Duration::from_secs(10), control.wait_for_commit())
        .await
        .expect("supervised worker reached real post-commit boundary");
    control.reject_paused_commit();
    tokio::time::timeout(
        Duration::from_secs(10),
        completion.wait_for_strategy_at_least(ForgeTaskStrategy::StagingFold, 1),
    )
    .await
    .expect("supervised worker reconciled prepared uncertain commit");
    assert_eq!(
        control.update_attempts(),
        1,
        "staging publication performs one catalog update attempt"
    );
    server
        .bifrost_scribe()
        .expect("server-role Scribe")
        .retire_committed_for_test(std::time::Instant::now() + Duration::from_secs(120))
        .await
        .expect("retire uncertain fixture Scribe generations");
    let tail_stats = server
        .bifrost_scribe()
        .expect("server-role Scribe")
        .memtable_stats()
        .expect("uncertain fixture Scribe stats");
    assert_eq!(
        tail_stats.writable_rows, 0,
        "uncertain writable tail drained"
    );
    assert_eq!(
        tail_stats.immutable_rows, 0,
        "uncertain immutable tail retired"
    );
    let expected_rows =
        u64::try_from(WRITE_CYCLES * SPANS_PER_WRITE).expect("bounded uncertain rows");
    assert_terminal_audits(server, tenants[0].id, scenario).await;
    assert_snapshot(server, tenants[0].id, scenario).await;
    let mut conn = server
        .state()
        .postgres
        .vala()
        .tenant_conn(tenants[0].id)
        .await
        .expect("uncertain tenant connection");
    let (terminal, evidence): (i64, i64) = sqlx::query_as(
        "SELECT count(*), count(*) FILTER (WHERE evidence IS NOT NULL) FROM vala.forge_tasks WHERE data_tenant_id = $1 AND strategy = 'staging_fold' AND state = 'succeeded'",
    )
    .bind(tenants[0].id.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("uncertain terminal task evidence");
    assert_eq!(terminal, 1, "uncertain publication terminalized once");
    assert_eq!(evidence, 1, "uncertain publication retained exact evidence");
    let (files, compacted, committed): (i64, i64, i64) = sqlx::query_as(
        "SELECT count(*), count(*) FILTER (WHERE compacted), count(*) FILTER (WHERE committed_snapshot_id IS NOT NULL) FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = 'vala.traces' AND table_name = 'spans'",
    )
    .bind(tenants[0].id.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("uncertain file-list convergence");
    conn.commit()
        .await
        .expect("complete uncertain evidence read");
    assert_eq!(
        committed, files,
        "uncertain recovery stamps every exact staging input"
    );
    assert_eq!(
        query_rows(server, &tenants[0].jwt).await,
        expected_rows,
        "uncertain commit public rows after terminal={terminal}, evidence={evidence}, files={files}, compacted={compacted}, committed={committed}"
    );
    assert_no_forge_leases(server, scenario).await;
    cluster
        .shutdown()
        .await
        .expect("dedicated cluster shutdown");
}

/// Prove real planner admission reaches a cluster-exclusive large lane and terminal unschedulable lane.
///
/// # Panics
///
/// Panics when production planning does not persist the configured lane state.
#[tokio::test]
#[ignore = "gated journey: real Forge unschedulable admission"]
async fn dedicated_roles_terminalize_unschedulable_work() {
    let large_config = vala_bifrost_redux::forge::ForgeConfig {
        max_bytes_per_tick: 1,
        max_memory_bytes: i64::MAX as u64,
        max_large_task_bytes: i64::MAX as u64,
        ..vala_bifrost_redux::forge::ForgeConfig::default()
    };
    let cluster =
        WyrdTestCluster::start_with_dedicated_forge_workers_with_config_for_test(large_config)
            .await
            .expect("large-lane cluster");
    let server = cluster.server(0).expect("scheduler server");
    let completion = cluster
        .forge_completion_observer()
        .expect("large-lane completion observer");
    let scenario = Scenario {
        name: "large-lane",
        pods: 1,
        tenants: 2,
    };
    let tenants = provision_tenants(server, scenario).await;
    for cycle in 0..WRITE_CYCLES {
        write_cycle(std::slice::from_ref(server), &tenants, cycle, scenario).await;
        for tenant in &tenants {
            server
                .flush_bifrost_for_tenant(tenant.id)
                .await
                .expect("large flush");
        }
    }
    server
        .forge_clock()
        .advance(chrono::Duration::days(1) + chrono::Duration::minutes(3))
        .expect("age large files");
    completion.hold_after_claims_for_test(1);
    trigger_supervised_scheduler(server, scenario).await;
    let tasks: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT strategy, lane, state FROM vala.forge_tasks WHERE data_tenant_id = ANY($1) ORDER BY data_tenant_id, strategy",
    )
    .bind(tenants.iter().map(|tenant| tenant.id.as_uuid()).collect::<Vec<_>>())
    .fetch_all(
        server
            .state()
            .postgres
            .operator_pool()
            .expect("operator pool")
            .pool(),
    )
    .await
    .expect("large singleton task states");
    assert!(
        tasks.len() >= tenants.len()
            && tasks
                .iter()
                .all(|(strategy, lane, state)| strategy == "staging_fold"
                    && lane == "large_singleton"
                    && (state == "ready" || state == "claimed" || state == "running"))
            && tasks.iter().any(|(_, _, state)| state == "ready"),
        "planner must retain a ready multi-input large task while the supervised worker claims one: {tasks:?}"
    );
    tokio::time::timeout(
        Duration::from_secs(10),
        completion.wait_for_claims_for_test(),
    )
    .await
    .expect("one worker claimed a cluster-exclusive large task");
    let (active_large, ready_large): (i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE lane = 'large_singleton' AND state IN ('claimed', 'running', 'prepared')), count(*) FILTER (WHERE lane = 'large_singleton' AND state = 'ready') FROM vala.forge_tasks WHERE data_tenant_id = ANY($1)",
    )
    .bind(tenants.iter().map(|tenant| tenant.id.as_uuid()).collect::<Vec<_>>())
    .fetch_one(
        server
            .state()
            .postgres
            .operator_pool()
            .expect("operator pool")
            .pool(),
    )
    .await
    .expect("cluster-exclusive large task state");
    assert_eq!(active_large, 1, "one large task may be active cluster-wide");
    assert!(
        ready_large >= 1,
        "another large task must wait for the cluster lane"
    );
    let active_tenant_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT data_tenant_id FROM vala.forge_tasks WHERE lane = 'large_singleton' AND state IN ('claimed', 'running', 'prepared')",
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
    .expect("active large task tenant");
    let active_tenant = tenants
        .iter()
        .find(|tenant| tenant.id.as_uuid() == active_tenant_id)
        .expect("active task belongs to a fixture tenant");
    let identity = ForgeTaskTableIdentity::new("wyrd-redux", "vala.traces", "spans")
        .expect("canonical traces task identity");
    let operator_pool = server
        .state()
        .postgres
        .operator_pool()
        .expect("operator pool");
    ForgeTasks::new(operator_pool.clone())
        .upsert_periodic(active_tenant.id, &identity)
        .await
        .expect("enqueue same-table periodic demand");
    trigger_supervised_scheduler(server, scenario).await;
    let active_same_table: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.forge_tasks WHERE data_tenant_id = $1 AND namespace_name = 'vala.traces' AND table_name = 'spans' AND state IN ('claimed', 'running', 'prepared')",
    )
    .bind(active_tenant.id.as_uuid())
    .fetch_one(
        server
            .state()
            .postgres
            .operator_pool()
            .expect("operator pool")
            .pool(),
    )
    .await
    .expect("same-table active publication count");
    assert_eq!(
        active_same_table, 1,
        "same-table planning must not create an overlapping real-worker publication"
    );
    completion.release_claims_for_test();
    tokio::time::timeout(
        Duration::from_secs(15),
        completion.wait_for_strategy_at_least(ForgeTaskStrategy::StagingFold, tenants.len()),
    )
    .await
    .expect("supervised workers completed the serialized large tasks");
    cluster.shutdown().await.expect("large cluster shutdown");

    let unschedulable_config = vala_bifrost_redux::forge::ForgeConfig {
        max_bytes_per_tick: 1,
        max_large_task_bytes: 1,
        ..vala_bifrost_redux::forge::ForgeConfig::default()
    };
    let cluster = WyrdTestCluster::start_with_dedicated_forge_workers_with_config_for_test(
        unschedulable_config,
    )
    .await
    .expect("unschedulable cluster");
    let server = cluster.server(0).expect("scheduler server");
    let scenario = Scenario {
        name: "unschedulable",
        pods: 1,
        tenants: 1,
    };
    let tenants = provision_tenants(server, scenario).await;
    for cycle in 0..WRITE_CYCLES {
        write_cycle(std::slice::from_ref(server), &tenants, cycle, scenario).await;
        server
            .flush_bifrost_for_tenant(tenants[0].id)
            .await
            .expect("unschedulable flush");
    }
    server
        .forge_clock()
        .advance(chrono::Duration::days(1) + chrono::Duration::minutes(3))
        .expect("age unschedulable files");
    trigger_supervised_scheduler(server, scenario).await;
    let (state, audits): (String, i64) = sqlx::query_as("SELECT state, (SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id=$1 AND operation='forge.task.unschedulable') FROM vala.forge_tasks WHERE data_tenant_id=$1 AND strategy='staging_fold'").bind(tenants[0].id.as_uuid()).fetch_one(server.state().postgres.operator_pool().expect("operator pool").pool()).await.expect("unschedulable terminal");
    assert_eq!(state, "unschedulable");
    assert_eq!(audits, 1);
    cluster
        .shutdown()
        .await
        .expect("unschedulable cluster shutdown");
}

/// Drive independent tenant tasks through one supervised production role topology.
///
/// # Panics
///
/// Panics when production ingest, durable scheduling, supervised worker
/// completion, public query, or durable parity inspection fails.
async fn run_supervised_role_fixture(
    cluster: &WyrdTestCluster,
    worker_count: usize,
    name: &'static str,
) -> RoleTopologyEvidence {
    let server = cluster.server(0).expect("serving Forge server");
    let completion = cluster
        .forge_completion_observer()
        .expect("shared supervised completion observer");
    let scenario = Scenario {
        name,
        pods: 1,
        tenants: worker_count.max(3),
    };
    let tenants = provision_tenants(server, scenario).await;
    assert_active_traces_roster(server, &tenants, scenario).await;
    for cycle in 0..WRITE_CYCLES {
        write_cycle(std::slice::from_ref(server), &tenants, cycle, scenario).await;
        for tenant in &tenants {
            server
                .flush_bifrost_for_tenant(tenant.id)
                .await
                .expect("supervised role fixture flush");
        }
    }
    server
        .forge_clock()
        .advance(chrono::Duration::days(1) + chrono::Duration::minutes(3))
        .expect("advance supervised role fixture clock");
    completion.hold_after_claims_for_test(worker_count);
    trigger_supervised_scheduler(server, scenario).await;
    let queued: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.forge_tasks WHERE data_tenant_id = ANY($1) AND state = 'ready'",
    )
    .bind(
        tenants
            .iter()
            .map(|tenant| tenant.id.as_uuid())
            .collect::<Vec<_>>(),
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
    .expect("supervised queued tenant tasks");
    assert!(
        usize::try_from(queued).expect("nonnegative queued count") >= worker_count,
        "{name} constrained tenant must remain inside the measured eligible set"
    );
    assert_scheduler_takeover_preserves_fairness_bound(server, scenario).await;
    tokio::time::timeout(
        Duration::from_secs(10),
        completion.wait_for_claims_for_test(),
    )
    .await
    .expect("configured supervised roles claimed independent tasks");
    assert_claim_barrier_shape(server, worker_count, scenario).await;
    completion.release_claims_for_test();
    tokio::time::timeout(
        Duration::from_secs(10),
        completion.wait_for_distinct_workers_at_least(worker_count),
    )
    .await
    .expect("configured supervised roles completed durable work");
    tokio::time::timeout(
        Duration::from_secs(10),
        completion.wait_for_at_least(tenants.len()),
    )
    .await
    .expect("every role fixture tenant reached a terminal supervised completion");
    server
        .bifrost_scribe()
        .expect("server-role Scribe")
        .retire_committed_for_test(std::time::Instant::now() + Duration::from_secs(120))
        .await
        .expect("retire Scribe generations");

    let expected_rows =
        u64::try_from(WRITE_CYCLES * SPANS_PER_WRITE).expect("bounded role fixture rows");
    let mut rows = Vec::with_capacity(tenants.len());
    let mut terminal_tasks = Vec::with_capacity(tenants.len());
    let mut terminal_audits = Vec::with_capacity(tenants.len());
    let mut evidence_rows = Vec::with_capacity(tenants.len());
    let mut watermark_rows = Vec::with_capacity(tenants.len());
    for tenant in &tenants {
        let row_count = query_rows(server, &tenant.jwt).await;
        assert_eq!(row_count, expected_rows, "{name} tenant {} rows", tenant.id);
        assert_terminal_audits(server, tenant.id, scenario).await;
        assert_snapshot(server, tenant.id, scenario).await;
        rows.push(row_count);
        let mut conn = server
            .state()
            .postgres
            .vala()
            .tenant_conn(tenant.id)
            .await
            .expect("role fixture tenant connection");
        let durable: (i64, i64, i64) = sqlx::query_as(
            "SELECT count(*), count(*) FILTER (WHERE evidence IS NOT NULL), count(*) FILTER (WHERE watermark_snapshot_id IS NOT NULL) FROM vala.forge_tasks WHERE data_tenant_id = $1 AND state IN ('succeeded', 'unschedulable', 'failed', 'cancelled')",
        )
        .bind(tenant.id.as_uuid())
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("role fixture durable task evidence");
        let audits: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id = $1 AND operation IN ('forge.file_compact.committed', 'forge.file_compact.recovered', 'forge.file_compact.reset')",
        )
        .bind(tenant.id.as_uuid())
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("role fixture terminal audit count");
        terminal_tasks.push(durable.0);
        evidence_rows.push(durable.1);
        watermark_rows.push(durable.2);
        terminal_audits.push(audits);
    }
    assert_no_forge_leases(server, scenario).await;
    let cleanup_leases: i64 = sqlx::query_scalar(
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
    .expect("role fixture cleanup lease count");
    RoleTopologyEvidence {
        rows,
        terminal_tasks,
        terminal_audits,
        evidence_rows,
        watermark_rows,
        cleanup_leases,
    }
}

/// Return durable task strategy, state, owner, and expiry data for a failing role journey.
///
/// # Panics
///
/// Panics when platform-admin task inspection fails during failure reporting.
async fn forge_task_diagnostics(
    server: &WyrdTestServer,
) -> Vec<(
    String,
    String,
    Option<uuid::Uuid>,
    Option<chrono::DateTime<chrono::Utc>>,
)> {
    sqlx::query_as(
        "SELECT strategy, state, claimed_by, claim_expires_at FROM vala.forge_tasks ORDER BY data_tenant_id, created_at",
    )
    .fetch_all(
        server
            .state()
            .postgres
            .operator_pool()
            .expect("operator pool")
            .pool(),
    )
    .await
    .expect("Forge task diagnostic query")
}

/// Assert a real-role claim barrier has one active task per independent tenant table.
///
/// The durable unique active-publication constraint is inspected before the
/// barrier releases a worker. This proves concurrent independent-table claims
/// without allowing overlapping publication for a tenant/table identity.
///
/// # Panics
///
/// Panics when platform-admin claim inspection fails or the observed claim
/// shape differs from the configured production worker topology.
async fn assert_claim_barrier_shape(server: &WyrdTestServer, expected: usize, scenario: Scenario) {
    let rows: Vec<(uuid::Uuid, String, String, i64)> = sqlx::query_as(
        "SELECT data_tenant_id, namespace_name, table_name, count(*) FROM vala.forge_tasks WHERE state IN ('claimed', 'running', 'prepared') GROUP BY data_tenant_id, namespace_name, table_name ORDER BY data_tenant_id, namespace_name, table_name",
    )
    .fetch_all(
        server
            .state()
            .postgres
            .operator_pool()
            .expect("operator pool")
            .pool(),
    )
    .await
    .expect("claim-barrier durable task inspection");
    assert_eq!(
        rows.len(),
        expected,
        "{} must expose one independent active claim per worker: {rows:?}",
        scenario.name
    );
    assert!(
        rows.iter().all(|(_, _, _, count)| *count == 1),
        "{} same-table publication overlap: {rows:?}",
        scenario.name
    );
}

/// Expire one exact scheduler lease and prove the next owner preserves the fair cursor bound.
///
/// # Panics
///
/// Panics when the exact lease cannot be expired, takeover planning fails, or
/// the durable scheduler fence/cursor no longer describes a bounded tenant ring.
async fn assert_scheduler_takeover_preserves_fairness_bound(
    server: &WyrdTestServer,
    scenario: Scenario,
) {
    let first_owner: uuid::Uuid = sqlx::query_scalar(
        "SELECT owner FROM vala.forge_scheduler_state WHERE singleton AND owner IS NOT NULL",
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
    .expect("read exact scheduler owner lease");
    let expired: uuid::Uuid = sqlx::query_scalar(
        "UPDATE vala.forge_scheduler_state SET expires_at = statement_timestamp() - interval '1 millisecond' WHERE singleton AND owner = $1 RETURNING owner",
    )
    .bind(first_owner)
    .fetch_one(
        server
            .state()
            .postgres
            .operator_pool()
            .expect("operator pool")
            .pool(),
    )
    .await
    .expect("expire exact scheduler owner lease");
    assert_eq!(
        expired, first_owner,
        "only the first scheduler lease may expire"
    );
    trigger_supervised_scheduler(server, scenario).await;
    let (fencing_token, cursor): (i64, Option<uuid::Uuid>) = sqlx::query_as(
        "SELECT fencing_token, last_tenant_id FROM vala.forge_scheduler_state WHERE singleton",
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
    .expect("takeover scheduler state");
    assert!(
        fencing_token >= 2,
        "{} scheduler takeover did not fence",
        scenario.name
    );
    assert!(
        cursor.is_some(),
        "{} scheduler takeover lost fair cursor",
        scenario.name
    );
}

/// Request and await one pass from the server's already-supervised scheduler.
///
/// This preserves the real server role's durable scheduler owner and avoids a
/// test-created scheduler bypassing lifecycle composition.
///
/// # Panics
///
/// Panics when the supervised scheduler does not return one pass inside the
/// bounded journey deadline.
async fn trigger_supervised_scheduler(server: &WyrdTestServer, scenario: Scenario) {
    let trigger = server.forge_scheduler_trigger_for_test();
    let expected = trigger.completed_passes().saturating_add(1);
    server.trigger_forge_scheduler_for_test();
    tokio::time::timeout(
        Duration::from_secs(10),
        trigger.wait_for_passes_at_least(expected),
    )
    .await
    .unwrap_or_else(|_| {
        panic!(
            "{} supervised Forge scheduler did not return",
            scenario.name
        )
    });
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
    let telemetry = harness.cluster().telemetry().clone();
    let checkpoint = telemetry
        .checkpoint()
        .unwrap_or_else(|error| panic!("{} query telemetry checkpoint: {error}", scenario.name));
    let sampler = telemetry
        .begin_gauge_sampling(&checkpoint)
        .await
        .unwrap_or_else(|error| panic!("{} query telemetry sampler: {error}", scenario.name));
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

    let delta = telemetry
        .delta_since(checkpoint, sampler)
        .await
        .unwrap_or_else(|error| panic!("{} query telemetry delta: {error}", scenario.name));
    let query = BifrostQueryTelemetryReport::from_server_delta(&delta)
        .unwrap_or_else(|error| panic!("{} query telemetry report: {error}", scenario.name));
    write_query_telemetry_artifact(scenario, query);

    harness
        .shutdown()
        .await
        .unwrap_or_else(|error| panic!("{} shutdown: {error}", scenario.name));
}

/// Write one server-only query artifact without copying any Forge maintenance field.
///
/// # Panics
///
/// Panics when the report directory cannot be created or typed JSON cannot be
/// serialized and written.
fn write_query_telemetry_artifact(scenario: Scenario, query: BifrostQueryTelemetryReport) {
    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("wyrd-testing manifest is nested below the repository root");
    let directory = repository.join("target/bifrost-benchmarks/task16");
    std::fs::create_dir_all(&directory).expect("create Task 16 query report directory");
    let artifact = QueryTelemetryArtifact {
        scenario: QueryScenarioIdentity {
            pods: scenario.pods,
            tenants: scenario.tenants,
        },
        query,
    };
    let path = directory.join(format!("query-{}-{}.json", scenario.pods, scenario.tenants));
    std::fs::write(
        path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&artifact).expect("serialize typed query artifact")
        ),
    )
    .expect("write typed query artifact");
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

/// Return one public read while retaining the exact failure payload for role-loss diagnosis.
///
/// # Panics
///
/// Panics when the public read does not succeed or omits its row-count header.
async fn supervised_reclaim_query_rows(server: &WyrdTestServer, jwt: &str) -> u64 {
    let response = query_response(server, jwt).await;
    let status = response.status();
    let headers = response.headers().clone();
    let body = response
        .text()
        .await
        .expect("public reclaim query response body");
    assert!(
        status.is_success(),
        "supervised reclaimed public query {status}: {body}"
    );
    headers
        .get("x-wyrd-row-count")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
        .expect("successful supervised reclaim query row-count header")
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
