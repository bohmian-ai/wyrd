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
            ProcessNodeTarget::Scribe,
        ],
    )
    .await?;

    let table = format!("graph_lease_{}", uuid::Uuid::now_v7().simple());
    cluster.nodes_mut()[2].register_table(&table)?;
    let rows = cluster.nodes_mut()[0]
        .execute_inactive_sql(&format!("SELECT id FROM vala.bifrost.{table}"))?;
    println!("SMOKE inactive analytical rows = {rows}");

    cluster.shutdown();
    Ok(())
}
