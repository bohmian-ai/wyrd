//! Deterministic public-Gate Bifrost cluster matrix journeys.

#[cfg(feature = "bench")]
use wyrd_bench::EvidenceStatus;
use wyrd_testing::bifrost::{BifrostTopology, WyrdTestCluster};
use wyrd_testing::load::{BifrostClusterLoad, ClusterLoadProfile};

/// The public task owns Postgres and invokes only the six earned serial filters.
///
/// # Panics
///
/// Panics when the mise task fixture cannot be parsed or either task differs
/// from the exact managed-Postgres command and six-filter contract.
#[test]
fn bifrost_cluster_task_contract_is_exact() {
    let mise = include_str!("../../../../mise.toml");
    let outer = task_run_body(mise, "test:bifrost:cluster").expect("outer cluster task");
    assert_eq!(
        outer,
        "scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise run test:bifrost:cluster:inner'"
    );
    let inner = task_run_body(mise, "test:bifrost:cluster:inner").expect("inner cluster task");
    let expected = [
        "single_tenant_single_pod_mixed_gate_load",
        "multi_tenant_single_pod_mixed_gate_load",
        "multi_tenant_three_server_three_worker_mixed_gate_load",
        "pressured_tenant_does_not_starve_peers",
        "cluster_load_cancellation_and_shutdown_release_all_owners",
        "three_server_cluster_reconciles_production_telemetry",
    ];
    let commands = inner.split(" && ").collect::<Vec<_>>();
    assert_eq!(commands.len(), expected.len());
    for (command, filter) in commands.iter().zip(expected) {
        assert_eq!(
            *command,
            format!(
                "cargo test --locked -p wyrd-testing --test integration -- {filter} --ignored --test-threads=1"
            )
        );
    }
}

/// Extract one mise task body without executing repository configuration.
fn task_run_body(mise: &str, task: &str) -> Option<String> {
    let header = format!("[tasks.\"{task}\"]");
    let section = mise.split_once(&header)?.1;
    let section = section.split_once("\n[").map_or(section, |(body, _)| body);
    let run = section
        .lines()
        .find(|line| line.trim_start().starts_with("run ="))?;
    let value = run.split_once('=')?.1.trim();
    if value == "\"\"\"" {
        let body = section.split_once("\nrun = \"\"\"\n")?.1;
        Some(body.split_once("\n\"\"\"")?.0.trim().to_owned())
    } else {
        Some(value.trim_matches('"').to_owned())
    }
}

/// The final cluster disposal observes supervised tasks before and after stop.
///
/// # Panics
///
/// Panics when the managed cluster cannot start or stop, starts without a
/// supervised owner, or retains a supervised task after shutdown.
#[tokio::test]
#[ignore = "requires managed Postgres and the real public Gate cluster"]
async fn final_shutdown_releases_supervised_tasks() {
    let cluster = WyrdTestCluster::start(1, BifrostTopology::OnePod)
        .await
        .expect("cluster starts");
    assert!(cluster.supervised_task_count_for_test() > 0);
    let inspection = cluster
        .shutdown_and_inspect()
        .await
        .expect("cluster shutdown");
    assert_eq!(inspection.supervised_tasks, 0);
}

/// Run one common topology profile and assert its user-visible contract.
///
/// # Panics
/// Panics when cluster execution fails or any required result invariant differs.
async fn run_profile(profile: ClusterLoadProfile) {
    let summary = BifrostClusterLoad::start(profile)
        .await
        .expect("cluster load starts")
        .run()
        .await
        .expect("cluster load completes");
    assert!(summary.jain_fairness >= 0.95);
    assert!(
        summary
            .tenants
            .values()
            .all(|tenant| tenant.completed_reads >= profile.minimum_reads_per_tenant)
    );
    let expected_rows =
        u64::from(profile.measured_batches_per_tenant) * u64::from(profile.rows_per_batch);
    assert!(summary.tenants.values().all(|tenant| {
        tenant.final_rows == expected_rows
            && tenant.final_schema_columns == 3
            && tenant.final_terminal_count == 1
            && tenant.tenant_audit_rows > 0
    }));
}

/// One tenant performs simultaneous public reads and writes on one Server.
///
/// # Panics
///
/// Panics when the cluster fixture fails or the shared topology, fairness,
/// schema, row, terminal, read-progress, or audit invariants do not hold.
#[tokio::test]
#[ignore = "requires managed Postgres and the real public Gate cluster"]
async fn single_tenant_single_pod_mixed_gate_load() {
    run_profile(ClusterLoadProfile::single_tenant_single_pod()).await;
}

/// Eight isolated tenants share one Server without visibility leakage.
///
/// # Panics
///
/// Panics when the cluster fixture fails or the shared topology, fairness,
/// tenant-isolation, row, schema, terminal, read-progress, or audit invariants fail.
#[tokio::test]
#[ignore = "requires managed Postgres and the real public Gate cluster"]
async fn multi_tenant_single_pod_mixed_gate_load() {
    run_profile(ClusterLoadProfile::multi_tenant_single_pod()).await;
}

/// Eight tenants traverse three Servers and three dedicated Forge workers.
///
/// # Panics
///
/// Panics when the cluster fixture fails or the shared multi-server topology,
/// fairness, isolation, row, schema, terminal, read-progress, or audit invariants fail.
#[tokio::test]
#[ignore = "requires managed Postgres and the real public Gate cluster"]
async fn multi_tenant_three_server_three_worker_mixed_gate_load() {
    run_profile(ClusterLoadProfile::multi_tenant_three_server_three_worker()).await;
}

/// Eight tenants reconcile public traffic, durable rows, audit, and production telemetry.
///
/// # Panics
///
/// Panics when the real three-server/three-worker lifecycle fails any public-client,
/// pillar, audit, durable-state, fairness, or exact production-telemetry assertion.
#[tokio::test]
#[ignore = "requires managed Postgres and the real public Gate cluster"]
#[cfg(not(feature = "bench"))]
async fn three_server_cluster_reconciles_production_telemetry() {
    let profile = ClusterLoadProfile::multi_tenant_three_server_three_worker();
    let summary = BifrostClusterLoad::start(profile)
        .await
        .expect("telemetry cluster starts")
        .run()
        .await
        .expect("production telemetry reconciles");
    assert_eq!(summary.tenants.len(), 8);
    assert!(summary.jain_fairness >= 0.95);
    assert!(summary.tenants.values().all(|tenant| {
        tenant.tenant_audit_rows > 0
            && tenant.final_rows == tenant.acknowledged_rows
            && tenant.completed_reads >= profile.minimum_reads_per_tenant
    }));
}

/// Eight tenants exercise the canonical sampled projection and reconciled report.
#[tokio::test]
#[ignore = "requires managed Postgres and the real public Gate cluster"]
#[cfg(feature = "bench")]
async fn three_server_cluster_reconciles_production_telemetry() {
    let definition = wyrd_testing::bifrost::bench_cluster::reference_scenario_matrix()[2];
    let (scenario, trial) =
        wyrd_testing::bifrost::bench_cluster::run_reference_trial(definition, 20, 1)
            .await
            .expect("canonical sampled production telemetry reconciles");
    assert_eq!(scenario.tenants, 8);
    assert_eq!(
        trial.production.required_telemetry,
        EvidenceStatus::Complete
    );
    assert_eq!(trial.production.counter_integrity, EvidenceStatus::Complete);
    assert_eq!(trial.production.spans_clean, EvidenceStatus::Complete);
    assert_eq!(trial.production.cleanup, EvidenceStatus::Complete);
    assert!(trial.production.reconciles());
}

/// Real admission pressure on one tenant does not stall peer progress.
///
/// # Panics
///
/// Panics when the pressure fixture fails, the configured pressured tenant is
/// absent, pressure is not observed, or any peer loses its exact progress guarantees.
#[tokio::test]
#[ignore = "requires managed Postgres and the real public Gate cluster"]
async fn pressured_tenant_does_not_starve_peers() {
    let profile = ClusterLoadProfile::pressured_multi_tenant_three_server_three_worker();
    let summary = BifrostClusterLoad::start(profile)
        .await
        .expect("pressure cluster starts")
        .run()
        .await
        .expect("pressure cluster completes");
    let pressured = summary
        .tenants
        .values()
        .find(|tenant| tenant.tenant_index == 0)
        .expect("configured tenant zero receives pressure");
    assert!(pressured.backpressure > 0);
    assert!(pressured.acknowledged_rows < 512);
    assert_eq!(pressured.final_rows, 128 + pressured.acknowledged_rows);
    assert!(
        summary
            .tenants
            .values()
            .filter(|tenant| tenant.tenant_index != 0)
            .all(|tenant| {
                assert_eq!(tenant.backpressure, 0, "peer tenant result: {tenant:?}");
                tenant.backpressure == 0
                    && tenant.acknowledged_rows == 512
                    && tenant.final_rows == 512
                    && tenant.completed_reads >= profile.minimum_reads_per_tenant
            })
    );
}

/// Cancellation and shutdown release every Bifrost owner.
///
/// # Panics
///
/// Panics when the fixture fails or shutdown retains any Scribe, Oracle,
/// Forge, Gate, supervisor, listener, or server owner.
#[tokio::test]
#[ignore = "requires managed Postgres and the real public Gate cluster"]
async fn cluster_load_cancellation_and_shutdown_release_all_owners() {
    let summary = BifrostClusterLoad::start(ClusterLoadProfile::single_tenant_single_pod())
        .await
        .expect("cluster load starts")
        .run()
        .await
        .expect("cluster load completes");
    assert_eq!(summary.cleanup.scribe_queued, 0);
    assert_eq!(summary.cleanup.scribe_inflight, 0);
    assert_eq!(summary.cleanup.oracle_tail_fences, 0);
    assert_eq!(summary.cleanup.forge_active_claims, 0);
    assert_eq!(summary.cleanup.forge_active_attempts, 0);
    assert_eq!(summary.cleanup.gate_active_streams, 0);
    assert_eq!(summary.cleanup.supervised_tasks, 0);
    assert!(summary.cleanup.listeners_stopped);
    assert!(summary.cleanup.servers_stopped);
}
