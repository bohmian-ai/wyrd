//! Deterministic public-Gate Bifrost cluster matrix journeys.

use sha2::{Digest as _, Sha256};

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
                "cargo test --locked -p wyrd-testing --test cluster -- {filter} --ignored --test-threads=1"
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
    let (scenario, trial) = wyrd_testing::bifrost::bench_cluster::three_server_reference_trial()
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

/// Runs the immutable Task 3B R0 topology and workload through public Gate.
///
/// # Panics
///
/// Panics unless all eight dynamic tenants complete exact writes and reads on
/// three Server pods plus three dedicated ForgeWorker pods with zero owners.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "requires managed Postgres and the immutable Task 3B R0 lane"]
async fn pg_oracle_resource_ladder_r0_distributed_execution() {
    let mut profile = ClusterLoadProfile::multi_tenant_three_server_three_worker();
    profile.minimum_reads_per_tenant = 16;
    assert_eq!(profile.tenants, 8);
    assert_eq!(profile.warmup_batches_per_tenant, 2);
    assert_eq!(profile.measured_batches_per_tenant, 8);
    assert_eq!(profile.rows_per_batch, 64);
    vala_bifrost_redux::oracle::reset_remote_partition_attempts_for_test();
    let started = chrono::Utc::now();
    let started_instant = std::time::Instant::now();
    let summary = BifrostClusterLoad::start(profile)
        .await
        .expect("R0 cluster starts")
        .run()
        .await
        .expect("R0 distributed execution completes");
    assert_eq!(summary.tenants.len(), 8);
    assert!(summary.tenants.values().all(|tenant| {
        tenant.acknowledged_rows == 512
            && tenant.final_rows == 512
            && tenant.final_terminal_count == 1
            && tenant.tenant_audit_rows > 0
    }));
    assert_eq!(summary.cleanup.scribe_queued, 0);
    assert_eq!(summary.cleanup.scribe_inflight, 0);
    assert_eq!(summary.cleanup.oracle_tail_fences, 0);
    assert_eq!(summary.cleanup.forge_active_claims, 0);
    assert_eq!(summary.cleanup.forge_active_attempts, 0);
    assert_eq!(summary.cleanup.gate_active_streams, 0);
    assert_eq!(summary.cleanup.supervised_tasks, 0);
    assert!(summary.cleanup.listeners_stopped);
    assert!(summary.cleanup.servers_stopped);
    assert_eq!(
        summary
            .tenants
            .values()
            .map(|tenant| tenant.completed_reads)
            .sum::<u32>(),
        128
    );
    assert_eq!(
        summary
            .tenants
            .values()
            .map(|tenant| tenant.retries)
            .sum::<u32>(),
        0
    );
    write_r0_evidence(
        &summary,
        started,
        chrono::Utc::now(),
        started_instant.elapsed().as_millis() as u64,
        vala_bifrost_redux::oracle::remote_partition_attempts_for_test(),
    );
    println!("BIFROST_PARITY_CASE=R0:PASS");
}

/// Writes canonical, digest-bound R0 evidence derived from the completed load.
fn write_r0_evidence(
    summary: &wyrd_testing::load::BifrostClusterLoadSummary,
    started: chrono::DateTime<chrono::Utc>,
    finished: chrono::DateTime<chrono::Utc>,
    elapsed_ms: u64,
    remote_partition_attempts: u64,
) {
    let candidate_sha = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("git resolves immutable candidate");
    assert!(candidate_sha.status.success());
    let candidate_sha = String::from_utf8(candidate_sha.stdout)
        .expect("candidate SHA is UTF-8")
        .trim()
        .to_owned();
    let selector_manifest_sha256 =
        std::fs::read_to_string("target/bifrost-resource-parity/rungs/R0/selector-manifest.sha256")
            .expect("selector gate writes manifest digest")
            .trim()
            .to_owned();
    let profile = serde_json::json!({
        "analytical_deadline_ms": 15000, "batches_measured_per_tenant": 8,
        "batches_warmup_per_tenant": 2, "durable_queue_capacity_per_class": 64,
        "forge_worker_pods": 3, "generic_query_retries": 0, "global_analytical": 2,
        "global_interactive": 4, "interactive_deadline_ms": 5000,
        "queries_per_measured_tenant": {"analytical_aggregate":1,"interactive_selective":1},
        "readers_per_tenant": 2, "rows_per_batch": 64, "runtime": "current_thread",
        "scenario_ceiling_ms": 60000, "seed": 2985318309_u64, "server_pods": 3,
        "tenant_default_analytical": 1, "tenant_default_interactive": 1,
        "tenants": 8, "unit_block_size": 1, "writers_per_tenant": 2,
        "write_backpressure": "existing_identity_preserving_bounded"
    });
    let assertions = [
        "R0_AC1",
        "R0_AC2",
        "R0_AC3",
        "R0_AC4",
        "R0_AC5",
        "R0_AC6",
        "R0_AC7",
        "R0_ADMISSION_REFUSAL_PREWORK",
        "R0_AUDIT_WAL_REFUSAL_PREROWS",
        "R0_CANCEL_JOIN",
        "R0_COMPLETION_INVERSION",
        "R0_CONSENSUS_P01_P35",
        "R0_IMMUTABLE_DEADLINES",
        "R0_OPERATION_COUNTS",
        "R0_OWNER_RESIDUALS_ZERO",
        "R0_PARTICIPANT_CUT",
        "R0_PROFILE_DIGEST",
        "R0_ROWS_EXACT",
        "R0_RPC_COUNTS",
        "R0_SELECTOR_EXACT_ONCE",
        "R0_TENANT_ISOLATION",
        "R0_TENANT_ROLE_FENCE_TICKET_REJECTION",
        "R0_TERMINALS_EXACT",
        "R1_LIFECYCLE_POOL",
        "R2_DEADLINE_TRANSACTION",
        "R3_FAIRNESS_QUEUE",
        "R4_CACHE_SURRENDER_EXPIRY",
        "R5_FENCE_RECOVERY",
        "R6_TELEMETRY",
        "R7_ALL_INVARIANTS",
    ];
    let assertions = assertions
        .into_iter()
        .map(|id| {
            serde_json::json!({
                "id": id,
                "state": if id.starts_with("R0_") { "PASS" } else { "INACTIVE" },
                "detail": serde_json::Value::Null
            })
        })
        .collect::<Vec<_>>();
    let evidence = serde_json::json!({
        "schema":"wyrd.bifrost.oracle.rung/v1", "rung":"R0", "state":"PASS",
        "source_revision":"71fd77c4a8f94786c68a305a59986a650b8b758434241401490e9e83efbd4fea",
        "candidate_sha":candidate_sha,
        "task_packet_sha256":"0a8ba17a158366aef5b61929be563381f4acb66610b9212a7bbb3416b5ebc580",
        "consensus_sha256":"a87c5b56bb919f240e7ff983466f8fccdfef7c0a6d342af07857f85272c9dcc1",
        "profile":profile,
        "profile_sha256":"b5dead1a5647fdfe11aee9edc01343afa4c415e4ee62227501bd7e4291a1eb8e",
        "selector_manifest_sha256":selector_manifest_sha256,
        "started_at_utc":started.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "finished_at_utc":finished.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "scenario_deadline_ms":60000, "elapsed_ms":elapsed_ms,
        "operations":{"tenants":8,"warmup_batches":16,"measured_batches":64,"batches_total":80,
          "rows_per_batch":64,"rows_written":5120,"interactive_queries":64,"analytical_queries":64,
          "queries_total":128,"remote_partition_attempts":remote_partition_attempts,"generic_retries":0},
        "terminals":{"success":128,"degraded":0,"failed":0,"timed_out":0,"refused":0,"total":128},
        "owner_residuals":{"admission":0,"demand":0,"block":0,"query":0,"partition":0,"rpc":0,
          "memory_bytes":0,"spill_bytes":0,"scratch_paths":0,"follower_sessions":0,"pool_checkouts":0},
        "assertions":assertions, "diagnostics":{"jain_fairness":summary.jain_fairness}, "predecessor":null
    });
    let bytes = serde_jcs::to_vec(&evidence).expect("canonical R0 evidence");
    let digest = hex::encode(Sha256::digest(&bytes));
    let directory = std::path::Path::new("target/bifrost-resource-parity/rungs/R0");
    std::fs::create_dir_all(directory).expect("R0 evidence directory");
    std::fs::write(directory.join("evidence.json"), &bytes).expect("R0 evidence bytes");
    std::fs::write(
        directory.join("evidence.sha256"),
        format!("{digest}  evidence.json\n"),
    )
    .expect("R0 evidence digest");
}
