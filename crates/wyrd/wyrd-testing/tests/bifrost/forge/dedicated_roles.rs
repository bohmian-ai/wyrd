//! Dedicated Forge worker roles: durable parity with embedded roles, lost-
//! worker reclaim, uncertain-commit recovery, and unschedulable terminals.
//!
//! Module of the `forge` group; shared fixtures live in `support.rs`.

use std::time::Duration;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::forge::{ForgeConfig, ForgeError, ForgeScheduler};
use vala_sql::queries::forge_tasks::ForgeTasks;
use vala_sql::row_types::forge_tasks::ForgeTaskTableIdentity;
use wyrd_server::config::BifrostTarget;
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::{BifrostTopology, WyrdTestCluster};

use super::support::*;

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
    dedicated_forge_workers_journey().await;
}

/// Implements the dedicated-worker product journey.
///
/// # Panics
///
/// Panics when the worker topology, listener isolation, durable work, or
/// supervised shutdown differs from the production role contract.
async fn dedicated_forge_workers_journey() {
    let config = ForgeConfig {
        lease_ttl: Duration::from_millis(1),
        ..ForgeConfig::default()
    };
    let cluster = WyrdTestCluster::start_with_dedicated_forge_workers_with_config_for_test(config)
        .await
        .expect("dedicated Forge worker cluster");
    assert_eq!(cluster.topology(), BifrostTopology::DedicatedForgeWorkers);
    let roles = cluster
        .servers()
        .iter()
        .map(|server| server.forge_process_role())
        .collect::<Vec<_>>();
    assert_eq!(
        roles,
        vec![
            BifrostTarget::Server,
            BifrostTarget::ForgeWorker,
            BifrostTarget::ForgeWorker,
            BifrostTarget::ForgeWorker,
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
    tokio::time::timeout(
        Duration::from_secs(10),
        completion.wait_for_at_least(tenants.len()),
    )
    .await
    .expect("every dedicated worker tenant reached terminal completion");
    let scribe = scheduler_server
        .bifrost_scribe()
        .expect("server-role Scribe");
    scribe
        .retire_committed_for_test()
        .await
        .expect("retire committed Scribe generations");
    let oracle = scheduler_server
        .state()
        .bifrost_query()
        .expect("server-role Oracle runtime");
    let readiness = oracle.oracle().readiness_snapshot();
    assert!(
        readiness.startup_reconciled,
        "dedicated worker Oracle startup reconciliation was lost before query: {readiness:?}"
    );
    assert!(
        readiness.live_oracles > 0,
        "dedicated worker Oracle membership disappeared before query: {readiness:?}"
    );
    assert!(
        readiness.running_capacity > 0,
        "dedicated worker Oracle has no local query capacity before query: {readiness:?}"
    );
    let expected_rows = u64::try_from(WRITE_CYCLES).expect("bounded dedicated journey row count");
    for tenant in &tenants {
        assert_eq!(
            query_rows(scheduler_server, tenant).await,
            expected_rows,
            "dedicated worker tenant {} rows",
            tenant.id
        );
        assert_terminal_audits(scheduler_server, tenant.id, scenario).await;
        assert_snapshot(scheduler_server, tenant, scenario).await;
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
        BifrostTarget::All
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
        worker.forge_process_role() == BifrostTarget::ForgeWorker
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
    supervised_worker_claim_expiry_journey().await;
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
    supervised_uncertain_commit_recovery_journey().await;
}

/// Prove real planner admission reaches a cluster-exclusive large lane and terminal unschedulable lane.
///
/// # Panics
///
/// Panics when production planning does not persist the configured lane state.
#[tokio::test]
#[ignore = "gated journey: real Forge unschedulable admission"]
async fn dedicated_roles_terminalize_unschedulable_work() {
    dedicated_unschedulable_admission_journey().await;
}

/// Proves `schedule_once` stops before demand effects when exact renewal is lost.
///
/// # Panics
///
/// Panics when the deterministic renewal boundary is not reached, exact lease
/// expiry fails, or any enqueue, acknowledgement, cursor, status, or complete
/// telemetry effect occurs after leadership loss.
#[tokio::test]
#[ignore = "gated journey: real scheduler renewal-loss boundary"]
async fn scheduler_renewal_loss_stops_every_later_effect() {
    let cluster = WyrdTestCluster::start_with_dedicated_forge_workers()
        .await
        .expect("renewal-loss cluster");
    let server = cluster.server(0).expect("scheduler server");
    let scenario = Scenario {
        name: "renewal-loss",
        tenants: 1,
    };
    let tenants = provision_tenants(server, scenario).await;
    let identity = ForgeTaskTableIdentity::new("wyrd-redux", "vala.traces", "spans")
        .expect("renewal-loss identity");
    ForgeTasks::new(
        server
            .state()
            .postgres
            .operator_pool()
            .expect("operator pool")
            .clone(),
    )
    .upsert_periodic(tenants[0].id, &identity)
    .await
    .expect("renewal-loss demand");
    let owner = uuid::Uuid::now_v7();
    let scheduler = ForgeScheduler::with_owner_for_test(
        server
            .state()
            .forge_coordinator()
            .expect("server-owned Forge")
            .as_ref(),
        owner,
    )
    .expect("renewal-loss scheduler");
    scheduler.pause_before_demand_renewal_for_test();
    let stop = CancellationToken::new();
    let operator_pool = server
        .state()
        .postgres
        .operator_pool()
        .expect("operator pool");
    let (result, evidence) = tokio::join!(scheduler.schedule_once(&stop), async {
        scheduler.wait_for_demand_renewal_pause_for_test().await;
        let before: (i64, i64, Option<uuid::Uuid>) = sqlx::query_as(
                "SELECT (SELECT count(*) FROM vala.forge_tasks), (SELECT count(*) FROM vala.forge_planning_demands), last_tenant_id FROM vala.forge_scheduler_state WHERE singleton AND owner=$1",
            )
            .bind(owner)
            .fetch_one(operator_pool.pool())
            .await
            .expect("pre-loss scheduler evidence");
        let expired = sqlx::query(
                "UPDATE vala.forge_scheduler_state SET expires_at=statement_timestamp()-interval '1 millisecond' WHERE singleton AND owner=$1",
            )
            .bind(owner)
            .execute(operator_pool.pool())
            .await
            .expect("expire exact scheduler owner")
            .rows_affected();
        assert_eq!(expired, 1);
        scheduler.release_demand_renewal_pause_for_test();
        before
    });
    assert!(
        matches!(result, Err(ForgeError::FenceLost { .. })),
        "renewal loss must retain the acquired-fence failure class: {result:?}"
    );
    let after: (i64, i64, Option<uuid::Uuid>) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM vala.forge_tasks), (SELECT count(*) FROM vala.forge_planning_demands), last_tenant_id FROM vala.forge_scheduler_state WHERE singleton",
    )
    .fetch_one(operator_pool.pool())
    .await
    .expect("post-loss scheduler evidence");
    assert_eq!(
        after, evidence,
        "loss must suppress every later durable effect"
    );
    assert_eq!(
        scheduler.complete_publications_for_test(),
        0,
        "loss must suppress complete status and telemetry publication"
    );
    cluster.shutdown().await.expect("renewal-loss shutdown");
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
    assert_scheduler_takeover_preserves_fairness_bound(server, &tenants, scenario).await;
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
        .retire_committed_for_test()
        .await
        .expect("retire Scribe generations");

    let expected_rows = u64::try_from(WRITE_CYCLES).expect("bounded role fixture rows");
    let mut rows = Vec::with_capacity(tenants.len());
    let mut terminal_tasks = Vec::with_capacity(tenants.len());
    let mut terminal_audits = Vec::with_capacity(tenants.len());
    let mut evidence_rows = Vec::with_capacity(tenants.len());
    let mut watermark_rows = Vec::with_capacity(tenants.len());
    for tenant in &tenants {
        let row_count = query_rows(server, tenant).await;
        assert_eq!(row_count, expected_rows, "{name} tenant {} rows", tenant.id);
        assert_terminal_audits(server, tenant.id, scenario).await;
        assert_snapshot(server, tenant, scenario).await;
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
    tenants: &[TenantWriter],
    scenario: Scenario,
) {
    let tasks = ForgeTasks::new(
        server
            .state()
            .postgres
            .operator_pool()
            .expect("operator pool")
            .clone(),
    );
    let table = ForgeTaskTableIdentity::new("wyrd-redux", "vala.traces", "spans")
        .expect("fairness table identity");
    for tenant in tenants {
        tasks
            .upsert_periodic(tenant.id, &table)
            .await
            .expect("seed eligible fairness demand");
    }
    let eligible_tenants: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT data_tenant_id) FROM vala.forge_planning_demands",
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
    .expect("measure exact eligible tenant count");
    let fairness_bound = usize::try_from(eligible_tenants).expect("nonnegative eligible tenants");
    assert_eq!(
        fairness_bound,
        tenants.len(),
        "{} exact fairness bound must include every role-fixture tenant",
        scenario.name
    );
    let hot_tenant = tenants[0].id;
    let constrained_tenant = tenants[tenants.len() - 1].id;
    let (first_owner, first_fence, cursor_before_takeover): (
        uuid::Uuid,
        i64,
        Option<uuid::Uuid>,
    ) = sqlx::query_as(
        "SELECT owner, fencing_token, last_tenant_id FROM vala.forge_scheduler_state WHERE singleton AND owner IS NOT NULL",
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
    let mut selections = Vec::with_capacity(fairness_bound);
    let mut constrained_considered = false;
    for _ in 0..fairness_bound {
        trigger_supervised_scheduler(server, scenario).await;
        let selected: Option<uuid::Uuid> = sqlx::query_scalar(
            "SELECT last_tenant_id FROM vala.forge_scheduler_state WHERE singleton",
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
        .expect("observe production scheduler cursor");
        let selected = selected.expect("complete scheduler pass advances cursor");
        selections.push(selected);
        constrained_considered = !sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM vala.forge_planning_demands WHERE data_tenant_id=$1 AND catalog_name=$2 AND namespace_name=$3 AND table_name=$4)",
        )
        .bind(constrained_tenant.as_uuid())
        .bind(&table.catalog)
        .bind(&table.namespace)
        .bind(&table.table)
        .fetch_one(
            server
                .state()
                .postgres
                .operator_pool()
                .expect("operator pool")
                .pool(),
        )
        .await
        .expect("observe constrained demand acknowledgement");
        tasks
            .upsert_periodic(hot_tenant, &table)
            .await
            .expect("keep hot tenant eligible");
        if constrained_considered {
            break;
        }
    }
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
    assert_eq!(
        fencing_token,
        first_fence + 1,
        "{} takeover must mint exactly one successor generation",
        scenario.name
    );
    assert!(
        cursor.is_some(),
        "{} scheduler takeover lost fair cursor",
        scenario.name
    );
    assert!(
        constrained_considered,
        "{} constrained tenant was not considered within exact N={fairness_bound} passes: {selections:?}",
        scenario.name
    );
    assert!(
        cursor_before_takeover.is_some(),
        "{} takeover fixture must begin from a continued cursor",
        scenario.name
    );
    let prior_cursor = cursor_before_takeover.expect("continued cursor checked above");
    let mut eligible = tenants
        .iter()
        .map(|tenant| tenant.id.as_uuid())
        .collect::<Vec<_>>();
    eligible.sort_unstable();
    let expected_final = eligible
        .iter()
        .copied()
        .rfind(|tenant| *tenant <= prior_cursor)
        .or_else(|| eligible.last().copied())
        .expect("non-empty eligible ring");
    assert_eq!(
        cursor,
        Some(expected_final),
        "{} takeover must retain the prior cursor and continue strict-after through the exact ring",
        scenario.name
    );
    let hot_pending: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM vala.forge_planning_demands WHERE data_tenant_id=$1)",
    )
    .bind(hot_tenant.as_uuid())
    .fetch_one(
        server
            .state()
            .postgres
            .operator_pool()
            .expect("operator pool")
            .pool(),
    )
    .await
    .expect("observe hot tenant eligibility");
    assert!(
        hot_pending,
        "{} hot tenant stopped being eligible",
        scenario.name
    );
}
