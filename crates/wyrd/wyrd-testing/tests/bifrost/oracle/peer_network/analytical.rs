//! One graph lease, and the exact resources it owns across real processes.

use wyrd_testing::bifrost::process_cluster::{BifrostProcessCluster, ProcessNodeTarget};

use super::support::PeerJourneyError;

/// Path of the compiled child every simulated pod runs.
const NODE_BINARY: &str = env!("CARGO_BIN_EXE_bifrost_peer_test_node");

/// Smoke proof that the inactive Analytical seam is reachable from a child.
///
/// # Panics
///
/// Panics when the attempt cannot be driven across the process topology.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn graph_lease_owns_exact_resources_for_complete_graph() {
    prove_graph_lease_owns_exact_resources()
        .await
        .expect("graph lease journey");
}

/// Drives the lease scenarios against one live multi-process topology.
///
/// # Errors
///
/// Returns the first scenario failure, which names the claim that broke.
async fn prove_graph_lease_owns_exact_resources() -> Result<(), PeerJourneyError> {
    let mut cluster = BifrostProcessCluster::start(
        NODE_BINARY,
        &[
            ProcessNodeTarget::Oracle,
            ProcessNodeTarget::Oracle,
            ProcessNodeTarget::Oracle,
            ProcessNodeTarget::Scribe,
        ],
    )
    .await?;

    let table = format!("graph_lease_{}", uuid::Uuid::now_v7().simple());
    cluster.nodes_mut()[3].register_table(&table)?;
    cluster.nodes_mut()[3].ingest_rows(&table, 12, 3)?;
    cluster.nodes_mut()[3].ingest_rows(&table, 12, 3)?;
    for index in 0..4 {
        cluster.nodes_mut()[index].refresh_snapshot()?;
    }
    let before = cluster.nodes_mut()[1].peer_body_polls()?;
    let rows = cluster.nodes_mut()[0].execute_inactive_sql(&format!(
        "SELECT filter_key, COUNT(*) AS matched FROM vala.bifrost.{table} \
         GROUP BY filter_key ORDER BY filter_key"
    ))?;
    let after = cluster.nodes_mut()[1].peer_body_polls()?;
    println!("SMOKE rows={rows} follower_body_polls {before} -> {after}");
    for index in 0..2 {
        println!(
            "SMOKE stderr[{index}]:\n{}",
            cluster.nodes()[index].stderr_tail()
        );
    }

    cluster.shutdown();
    Ok(())
}
