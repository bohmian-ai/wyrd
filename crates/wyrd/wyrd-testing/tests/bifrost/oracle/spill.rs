//! Oracle journeys — Spill accounting: bounded success, typed disk ceiling, cancellation
//! scratch cleanup, and pod-loss isolation.
//!
//! Module of the `oracle` binary; see `oracle.rs` for the capability it
//! proves and `support.rs` for the fixtures it shares.

use vala_bifrost_redux::resources::ResourceSource;
use vala_sdk::{CollectedQueryLimits, QueryClient, ValaSdkError};
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{BifrostQueryRequest, FreshnessPolicy, VisibilityMode};
use wyrd_spec::vala::error::BifrostError;
use wyrd_testing::bifrost::{BifrostClusterSpec, WyrdTestCluster};

use crate::support::*;

/// Asserts the production-derived mixed-role plan before costly fixture ingest.
///
/// # Panics
///
/// Panics when the server does not expose the single runtime owner or any
/// production-derived input, floor, elastic, scratch, CPU, or source differs.
fn assert_spill_resource_plan(server: &wyrd_testing::WyrdTestServer, scratch_bytes: u64) {
    let resources = server
        .state()
        .bifrost_resources()
        .expect("server owns global Bifrost resources");
    let plan = resources.plan();
    assert_eq!(plan.memory_limit_bytes, 832 << 20);
    assert_eq!(plan.unmanaged_reserve_bytes, 256 << 20);
    assert_eq!(plan.managed_memory_bytes, 576 << 20);
    assert_eq!(plan.scribe_floor_bytes, 256 << 20);
    assert_eq!(plan.oracle_floor_bytes, 256 << 20);
    assert_eq!(plan.elastic_memory_bytes, 64 << 20);
    assert_eq!(plan.scratch_limit_bytes, scratch_bytes);
    assert_eq!(plan.effective_cpu, 4);
    assert_eq!(resources.sources().memory, ResourceSource::Injected);
    assert_eq!(resources.sources().cpu, ResourceSource::Injected);
    assert_eq!(resources.sources().scratch, ResourceSource::Filesystem);
}

/// Asserts the exact lease held at the public query's schema barrier.
///
/// # Panics
///
/// Panics when the admitted query does not own the production-derived memory,
/// scratch, or adaptive partition envelope.
fn assert_active_spill_query_resources(server: &wyrd_testing::WyrdTestServer, scratch_bytes: u64) {
    let resources = server
        .state()
        .bifrost_resources()
        .expect("server owns global Bifrost resources");
    let snapshot = resources.snapshot().expect("active resource snapshot");
    assert!(snapshot.oracle_query_active);
    assert_eq!(snapshot.elastic_memory_used_bytes, 0);
    assert_eq!(snapshot.scratch_used_bytes, scratch_bytes);
    let query_memory = snapshot
        .plan
        .oracle_floor_bytes
        .checked_add(snapshot.elastic_memory_used_bytes)
        .expect("fixed query memory fits usize");
    assert_eq!(query_memory, 256 << 20);
    // A fully local scan hides no IO latency, so parallelism tracks cores rather
    // than fanning out; it is bounded by the working memory each partition needs
    // and never collapses to a single serial partition.
    let partitions = vala_bifrost_redux::resources::oracle_target_partitions(
        snapshot.plan.effective_cpu,
        1.0,
        query_memory,
    )
    .expect("active partition plan");
    assert_eq!(
        partitions,
        snapshot
            .plan
            .effective_cpu
            .min(
                query_memory / vala_bifrost_redux::resources::ORACLE_PARTITION_WORKING_MEMORY_BYTES
            )
            .max(vala_bifrost_redux::resources::ORACLE_MIN_TARGET_PARTITIONS),
        "local partition plan must follow cores clamped by per-partition working memory"
    );
    assert!(
        partitions >= vala_bifrost_redux::resources::ORACLE_MIN_TARGET_PARTITIONS,
        "an admitted query must never execute on a single serial partition"
    );
}

/// An unordered production query streams the exact durable rows without a
/// mandatory reconciliation spill and releases every local owner.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_spill_success_is_bounded_and_exact() {
    let cluster = WyrdTestCluster::start_spec(
        BifrostClusterSpec::one_mixed().with_system_resources(spill_system_resources(1 << 30)),
    )
    .await
    .expect("spill success cluster");
    let server = cluster.server(0).expect("spill success server");
    assert_spill_resource_plan(server, 1 << 30);
    let table = prepare_spill_table(&cluster, server, "oracle_spill_success", 1_000_000)
        .await
        .expect("spill success fixture");
    let reader = client(server, "oracle-spill-success-reader")
        .await
        .expect("spill success reader");
    let baseline = server
        .oracle_runtime_inspection()
        .expect("spill success baseline");
    let memory = server
        .state()
        .bifrost_resources()
        .expect("spill success governor")
        .snapshot()
        .expect("spill success resource snapshot");
    let memory_baseline = (managed_memory_used(memory), memory.oracle_memory_used_bytes);
    let checkpoint = cluster.telemetry().checkpoint().expect("spill checkpoint");

    assert_eq!(
        strict_unordered_summary(&reader, &format!("vala.bifrost.{table}"))
            .await
            .expect("spilling stream succeeds"),
        (1_000_000, 256_000_000)
    );

    let delta = cluster
        .telemetry()
        .delta_since(&checkpoint)
        .expect("spill metric delta");
    assert!(!delta.metrics.iter().any(|sample| {
        matches!(
            sample.family.as_str(),
            "oracle_query_spill_bytes_total" | "oracle_query_spill_files_total"
        ) && sample.value > 0.0
    }));
    assert_oracle_runtime_restored(server, baseline, memory_baseline)
        .expect("success cleanup is exact");
    cluster.shutdown().await.expect("spill success shutdown");
}

/// A production disk-ceiling refusal remains a typed public 429, cleans all
/// partial ownership, and leaves a smaller durable query usable.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_spill_disk_ceiling_is_typed_and_recovers() {
    let cluster = WyrdTestCluster::start_spec(
        BifrostClusterSpec::one_mixed().with_system_resources(spill_system_resources(1)),
    )
    .await
    .expect("spill ceiling cluster");
    let server = cluster.server(0).expect("spill ceiling server");
    assert_spill_resource_plan(server, 1);
    let large = prepare_spill_table(&cluster, server, "oracle_spill_ceiling", 1_000_000)
        .await
        .expect("spill ceiling fixture");
    let small = prepare_spill_table(&cluster, server, "oracle_spill_recovery", 2)
        .await
        .expect("spill recovery fixture");
    let reader = client(server, "oracle-spill-ceiling-reader")
        .await
        .expect("spill ceiling reader");
    let baseline = server
        .oracle_runtime_inspection()
        .expect("spill ceiling baseline");
    let memory = server
        .state()
        .bifrost_resources()
        .expect("spill ceiling governor")
        .snapshot()
        .expect("spill ceiling resource snapshot");
    let memory_baseline = (managed_memory_used(memory), memory.oracle_memory_used_bytes);

    let refusal = match QueryClient::new(&reader)
        .query(&BifrostQueryRequest {
            sql: format!("SELECT id, value FROM vala.bifrost.{large} ORDER BY id"),
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: Some(30_000),
        })
        .await
    {
        Ok(_) => panic!("one-byte disk quota must refuse before a public stream opens"),
        Err(error) => error,
    };
    assert!(matches!(
        refusal,
        ValaSdkError::Transport(WyrdError::Vala {
            error: BifrostError::QueryAdmissionRejected
        })
    ));
    assert_oracle_runtime_restored(server, baseline, memory_baseline)
        .expect("disk refusal cleanup is exact");
    assert_eq!(
        strict_count_value(&reader, &format!("vala.bifrost.{small}"))
            .await
            .expect("smaller recovery COUNT succeeds"),
        2
    );
    assert_oracle_runtime_restored(server, baseline, memory_baseline)
        .expect("recovery cleanup is exact");
    cluster.shutdown().await.expect("spill ceiling shutdown");
}

/// Dropping a public response at the deterministic schema barrier cancels the
/// active spilling query and releases its scratch before the journey proceeds.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_spill_cancellation_cleans_query_scratch() {
    let cluster = WyrdTestCluster::start_spec(
        BifrostClusterSpec::one_mixed().with_system_resources(spill_system_resources(1 << 30)),
    )
    .await
    .expect("spill cancellation cluster");
    let server = cluster.server(0).expect("spill cancellation server");
    assert_spill_resource_plan(server, 1 << 30);
    let table = prepare_spill_table(&cluster, server, "oracle_spill_cancel", 1_000_000)
        .await
        .expect("spill cancellation fixture");
    let reader = client(server, "oracle-spill-cancel-reader")
        .await
        .expect("spill cancellation reader");
    let baseline = server
        .oracle_runtime_inspection()
        .expect("spill cancellation baseline");
    let memory = server
        .state()
        .bifrost_resources()
        .expect("spill cancellation governor")
        .snapshot()
        .expect("spill cancellation resource snapshot");
    let memory_baseline = (managed_memory_used(memory), memory.oracle_memory_used_bytes);
    let request = BifrostQueryRequest {
        sql: format!("SELECT id, value FROM vala.bifrost.{table} ORDER BY id"),
        visibility: VisibilityMode::PublishedOnly,
        freshness: FreshnessPolicy::Strict,
        deadline_ms: Some(30_000),
    };
    let query = QueryClient::new(&reader);
    let query_task = tokio::spawn(async move {
        query
            .collect_bounded(
                &request,
                CollectedQueryLimits {
                    max_rows: 1,
                    max_encoded_bytes: 1024,
                },
            )
            .await
    });
    let active = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let active = server
                .oracle_runtime_inspection()
                .expect("active spill inspection");
            if active.active_queries > baseline.active_queries
                && active.spill_files > baseline.spill_files
            {
                break active;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("spilling query did not expose owned scratch before cancellation"));
    assert_active_spill_query_resources(server, 1 << 30);
    assert!(active.active_queries > baseline.active_queries);
    assert!(active.spill_files > baseline.spill_files);
    query_task.abort();
    let _ = query_task.await;
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            if assert_oracle_runtime_restored(server, baseline, memory_baseline).is_ok() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("cancellation restores Oracle resources");
    assert_oracle_runtime_restored(server, baseline, memory_baseline)
        .expect("cancellation cleanup is exact");
    assert_eq!(
        strict_spill_summary(&reader, &format!("vala.bifrost.{table}"))
            .await
            .expect("durable rows remain readable after cancellation"),
        (1_000_000, 256_000_000)
    );
    cluster
        .shutdown()
        .await
        .expect("spill cancellation shutdown");
}

/// Losing the pod executing an active spill terminates the in-flight request,
/// permits an exact public retry after restart, and removes only owned residue.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_spill_pod_loss_isolated_and_restart_cleans() {
    let mut cluster = WyrdTestCluster::start_spec(
        BifrostClusterSpec::one_mixed().with_system_resources(spill_system_resources(1 << 30)),
    )
    .await
    .expect("spill pod-loss cluster");
    for server in cluster.servers() {
        assert_spill_resource_plan(server, 1 << 30);
    }
    let query_node = cluster.configured_node_ids()[0];
    let table = {
        let server = cluster
            .server_by_node(query_node)
            .expect("spill pod-loss fixture server");
        prepare_spill_table(&cluster, server, "oracle_spill_pod_loss", 1_000_000)
            .await
            .expect("spill pod-loss fixture")
    };
    let reader = client(
        cluster
            .server_by_node(query_node)
            .expect("spill pod-loss query server"),
        "oracle-spill-pod-loss-reader",
    )
    .await
    .expect("spill pod-loss reader");
    let query_table = format!("vala.bifrost.{table}");
    let request_lifetime = cluster
        .abrupt_request_lifetime(query_node)
        .expect("spill pod-loss request lifetime");
    let query = tokio::spawn(async move {
        tokio::select! {
            result = strict_spill_summary(&reader, &query_table) => result,
            () = request_lifetime.cancelled() => Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionAborted,
                "bound test process terminated during public query",
            ).into()),
        }
    });
    let executing = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            if query.is_finished() {
                panic!("spilling query terminated before exposing owned scratch");
            }
            if let Some(node_id) = cluster
                .configured_node_ids()
                .iter()
                .copied()
                .find(|node_id| {
                    cluster
                        .server_by_node(*node_id)
                        .and_then(|server| server.oracle_runtime_inspection().ok())
                        .is_some_and(|inspection| {
                            inspection.active_queries > 0 && inspection.spill_files > 0
                        })
                })
            {
                break node_id;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("runtime inspection identifies the executing Oracle");
    let roots = cluster
        .terminate_node_abruptly_for_test(executing)
        .await
        .expect("executing Oracle terminates abruptly");
    let terminal = tokio::time::timeout(std::time::Duration::from_secs(30), query)
        .await
        .expect("pod-loss spill query terminates")
        .expect("pod-loss spill task joins");
    assert!(
        terminal.is_err(),
        "abrupt pod loss must fail the unfinished query"
    );
    cluster
        .seed_oracle_spill_restart_fixture(executing)
        .expect("seed stopped-node crash residue");
    assert_eq!(
        cluster
            .oracle_spill_restart_fixture_state(executing)
            .expect("pre-restart residue state"),
        (true, true)
    );
    cluster
        .restart_terminated_node_at_new_address(executing, roots)
        .await
        .expect("terminated Oracle restarts from retained roots");
    cluster
        .refresh_oracle_snapshots()
        .await
        .expect("restarted Oracle membership refresh");
    assert_eq!(
        cluster
            .oracle_spill_restart_fixture_state(executing)
            .expect("post-restart residue state"),
        (false, true),
        "restart must remove only the owned stale prefix"
    );
    let restarted = cluster
        .server_by_node(executing)
        .expect("restarted Oracle server");
    let residual = restarted
        .oracle_runtime_inspection()
        .expect("restarted Oracle residuals");
    assert_eq!(residual.active_queries, 0);
    assert_eq!(residual.queued_queries, 0);
    assert_eq!(residual.reserved_memory_bytes, 0);
    assert_eq!(residual.reserved_spill_bytes, 0);
    assert_eq!(residual.peer_pending, 0);
    assert_eq!(residual.peer_running, 0);
    let memory = restarted
        .state()
        .bifrost_resources()
        .expect("restarted Oracle governor")
        .snapshot()
        .expect("restarted Oracle resource snapshot");
    assert_eq!(memory.oracle_memory_used_bytes, 0);
    assert_eq!(managed_memory_used(memory), 0);
    let restarted_reader = client(restarted, "oracle-spill-restarted-reader")
        .await
        .expect("restarted Oracle reader");
    assert_eq!(
        strict_spill_summary(&restarted_reader, &format!("vala.bifrost.{table}"))
            .await
            .expect("restarted Oracle reads durable rows"),
        (1_000_000, 256_000_000)
    );
    cluster.shutdown().await.expect("spill pod-loss shutdown");
}

/// Requires query admission, parent/child memory, spill ownership, and local
/// scratch files to match the exact pre-query baseline.
///
/// # Errors
///
/// Returns an inspection error or a diagnostic mismatch for any retained
/// production owner.
fn assert_oracle_runtime_restored(
    server: &wyrd_testing::WyrdTestServer,
    baseline: wyrd_testing::OracleRuntimeInspection,
    memory_baseline: (usize, usize),
) -> Result<(), JourneyError> {
    let current = server.oracle_runtime_inspection()?;
    if current.active_queries != baseline.active_queries
        || current.queued_queries != baseline.queued_queries
        || current.reserved_memory_bytes != baseline.reserved_memory_bytes
        || current.reserved_spill_bytes != baseline.reserved_spill_bytes
        || current.peer_pending != baseline.peer_pending
        || current.peer_running != baseline.peer_running
        || current.spill_directories != baseline.spill_directories
        || current.spill_files != baseline.spill_files
        || current.spill_file_bytes != baseline.spill_file_bytes
    {
        return Err(format!(
            "Oracle runtime did not return to baseline: baseline={baseline:?} current={current:?}"
        )
        .into());
    }
    let memory = server
        .state()
        .bifrost_resources()
        .ok_or("Oracle server lacks the shared memory governor")?
        .snapshot()?;
    if (managed_memory_used(memory), memory.oracle_memory_used_bytes) != memory_baseline {
        return Err(format!(
            "Oracle memory did not return to baseline: baseline={memory_baseline:?} current=({},{})",
            managed_memory_used(memory), memory.oracle_memory_used_bytes
        )
        .into());
    }
    Ok(())
}
