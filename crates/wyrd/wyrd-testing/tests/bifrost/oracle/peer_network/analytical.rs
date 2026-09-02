use wyrd_testing::bifrost::process_cluster::{BifrostProcessCluster, ProcessNodeTarget};

use super::support::PeerJourneyError;

/// Path of the compiled child every simulated pod runs.
const NODE_BINARY: &str = env!("CARGO_BIN_EXE_bifrost_peer_test_node");

/// Index of the pod that leads the distributed attempt.
const LEADER: usize = 0;

/// Indices of the pods that follow the leader's graph.
const FOLLOWERS: [usize; 2] = [1, 2];

/// Index of the pod that publishes the data every Oracle reads.
const SCRIBE: usize = 3;

/// A distributed graph owns exactly one leased reservation on each follower.
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
/// The topology is three Oracles and one Scribe because a graph only becomes
/// distributed when a leaf stage has more than one task, and the leader is not
/// a participant in its own worker set: two remote followers and two published
/// files are the smallest shape that puts a real stage on a real peer.
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
    cluster.nodes_mut()[SCRIBE].register_table(&table)?;
    cluster.nodes_mut()[SCRIBE].ingest_rows(&table, 0, 12, 3)?;
    cluster.nodes_mut()[SCRIBE].ingest_rows(&table, 0, 12, 3)?;
    for index in [LEADER, FOLLOWERS[0], FOLLOWERS[1], SCRIBE] {
        cluster.nodes_mut()[index].refresh_snapshot()?;
    }

    // Baseline first: every later count is a difference from a node that is
    // provably holding nothing, so a leak from an earlier lane cannot be read
    // as this attempt's own release.
    for index in FOLLOWERS {
        let (activated, live) = cluster.nodes_mut()[index].graph_leases()?;
        if (activated, live) != (0, 0) {
            return Err(PeerJourneyError::from(format!(
                "follower {index} must start with no graph lease, held ({activated}, {live})"
            )));
        }
    }

    let sql = format!(
        "SELECT filter_key, COUNT(*) AS matched FROM vala.bifrost.{table} \
         GROUP BY filter_key ORDER BY filter_key"
    );
    let rows = cluster.nodes_mut()[LEADER].execute_inactive_sql(&sql)?;
    if rows != 3 {
        return Err(PeerJourneyError::from(format!(
            "the distributed attempt must return one row per group, returned {rows}"
        )));
    }

    // One activation per follower, however many stage messages arrived. A
    // coordinator channel, a `SetPlan`, several `ExecuteTask`s, and every cached
    // stream address the same graph; each must reuse the lease the first one
    // activated rather than charge a second envelope.
    for index in FOLLOWERS {
        let (activated, live) = cluster.nodes_mut()[index].graph_leases()?;
        if activated != 1 {
            return Err(PeerJourneyError::from(format!(
                "follower {index} must activate exactly one graph lease, activated {activated}"
            )));
        }
        if live != 0 {
            return Err(PeerJourneyError::from(format!(
                "follower {index} must release its graph lease, still holds {live}"
            )));
        }
    }

    // A second attempt takes a second reservation and returns it too, which is
    // what distinguishes a lease that is released from one that was merely
    // never charged again.
    let repeated = cluster.nodes_mut()[LEADER].execute_inactive_sql(&sql)?;
    if repeated != 3 {
        return Err(PeerJourneyError::from(format!(
            "the repeated attempt must return one row per group, returned {repeated}"
        )));
    }
    for index in FOLLOWERS {
        let (activated, live) = cluster.nodes_mut()[index].graph_leases()?;
        if (activated, live) != (2, 0) {
            return Err(PeerJourneyError::from(format!(
                "follower {index} must activate and release one lease per attempt, \
                 reported ({activated}, {live})"
            )));
        }
    }

    cluster.shutdown();
    Ok(())
}

/// Peer loss and cancellation each end one attempt on every reachable process.
///
/// # Panics
///
/// Panics when either ordering starts a successor or leaves ownership behind.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn one_attempt_peer_loss_and_cancellation_join_every_process() {
    prove_one_attempt_peer_loss_and_cancellation()
        .await
        .expect("one-attempt peer loss journey");
}

/// Drives both terminal orderings over one live multi-process topology.
///
/// Each case gets a clean cluster on purpose: the claim is about what one
/// query leaves behind, and a second query on the same processes cannot
/// distinguish "released" from "never taken".
///
/// # Errors
///
/// Returns the first scenario failure, which names the claim that broke.
async fn prove_one_attempt_peer_loss_and_cancellation() -> Result<(), PeerJourneyError> {
    prove_terminal_ordering(TerminalCause::PeerLoss).await?;
    prove_terminal_ordering(TerminalCause::Cancellation).await
}

/// How the one attempt is made to fail after its followers hold their graphs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminalCause {
    /// The paused follower's process disappears mid-graph.
    PeerLoss,
    /// The leader's caller cancels while the follower still holds its graph.
    Cancellation,
}

/// Drives one clean cluster to one failed terminal and proves nothing survives.
///
/// # Errors
///
/// Returns the first claim that broke.
async fn prove_terminal_ordering(cause: TerminalCause) -> Result<(), PeerJourneyError> {
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

    let table = format!("one_attempt_{}", uuid::Uuid::now_v7().simple());
    cluster.nodes_mut()[SCRIBE].register_table(&table)?;
    cluster.nodes_mut()[SCRIBE].ingest_rows(&table, 0, 12, 3)?;
    cluster.nodes_mut()[SCRIBE].ingest_rows(&table, 0, 12, 3)?;
    for index in [LEADER, FOLLOWERS[0], FOLLOWERS[1], SCRIBE] {
        cluster.nodes_mut()[index].refresh_snapshot()?;
    }

    // Armed before the query, so the follower is held at a real boundary: its
    // graph lease is active and it has consumed no source.
    let paused = FOLLOWERS[0];
    cluster.nodes_mut()[paused].arm_execute_pause()?;

    let sql = format!(
        "SELECT filter_key, COUNT(*) AS matched FROM vala.bifrost.{table} \
         GROUP BY filter_key ORDER BY filter_key"
    );
    cluster.nodes_mut()[LEADER].start_inactive_sql(&sql)?;
    cluster.nodes_mut()[paused].await_execute_paused()?;

    let (activated, live) = cluster.nodes_mut()[paused].graph_leases()?;
    if (activated, live) != (1, 1) {
        return Err(PeerJourneyError::from(format!(
            "the paused follower must hold exactly one activated lease, held ({activated}, {live})"
        )));
    }

    match cause {
        TerminalCause::PeerLoss => cluster.nodes_mut()[paused].kill()?,
        TerminalCause::Cancellation => cluster.nodes_mut()[LEADER].cancel_inactive_sql()?,
    }

    let outcome = cluster.nodes_mut()[LEADER].await_inactive_sql()?;
    if let Ok(rows) = outcome {
        return Err(PeerJourneyError::from(format!(
            "a lost peer must not produce a successful result, returned {rows} rows"
        )));
    }

    // The surviving follower was addressed exactly once and kept nothing. One
    // activation is the whole claim: a successor attempt would have reserved
    // and activated a second graph on this same process.
    let survivor = FOLLOWERS[1];
    let (activated, live) = await_released_lease(&mut cluster, survivor).await?;
    if activated > 1 {
        return Err(PeerJourneyError::from(format!(
            "follower {survivor} must be addressed by one attempt, activated {activated}"
        )));
    }
    if live != 0 {
        return Err(PeerJourneyError::from(format!(
            "follower {survivor} must release its graph lease, still holds {live}"
        )));
    }

    // The leader, too: a terminal that leaves the leader's own graph registered
    // would strand the query envelope on the node that owns it.
    let (_, leader_live) = await_released_lease(&mut cluster, LEADER).await?;
    if leader_live != 0 {
        return Err(PeerJourneyError::from(format!(
            "the leader must release its own graph, still holds {leader_live}"
        )));
    }

    if cause == TerminalCause::Cancellation {
        // Released only after the terminal, so the release cannot be what
        // produced it. The killed process has nothing left to release.
        cluster.nodes_mut()[paused].release_execute_pause()?;
    }
    cluster.shutdown();
    Ok(())
}

/// Polls one node until it holds no graph lease, then reports its counts.
///
/// Cleanup is asynchronous with the leader's terminal by design, so a single
/// observation would be a race rather than a claim.
///
/// # Errors
///
/// Returns the node's last observed counts when it never released.
async fn await_released_lease(
    cluster: &mut BifrostProcessCluster,
    index: usize,
) -> Result<(u64, usize), PeerJourneyError> {
    let mut last = (0, 0);
    for _ in 0..CLEAN_LEASE_POLLS {
        last = cluster.nodes_mut()[index].graph_leases()?;
        if last.1 == 0 {
            return Ok(last);
        }
        tokio::time::sleep(CLEAN_LEASE_INTERVAL).await;
    }
    Err(PeerJourneyError::from(format!(
        "node {index} never released its graph lease, last saw {last:?}"
    )))
}

/// Bound on how long terminal cleanup may take before it is called a leak.
const CLEAN_LEASE_POLLS: usize = 100;

/// Interval between graph-lease observations while waiting for cleanup.
const CLEAN_LEASE_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);
