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

/// Left-table rows, wider than the right so the join is not an identity.
const LEFT_ROWS: i64 = 400_000;

/// Right-table rows; the join key range that actually matches.
const RIGHT_ROWS: i64 = 300_000;

/// Rows per fixture ingest request.
///
/// One request per table would have to hold the whole table as a single Arrow
/// batch in the Scribe's address space, which is a fixture artifact rather than
/// anything a real writer does.
const INGEST_CHUNK: i64 = 100_000;

/// Distinct `filter_key` groups the fixture rows fall into.
///
/// Irrelevant to the query under test, which derives its own key from `id`;
/// it only keeps the published files from being one trivial group.
const INGEST_GROUPS: i64 = 1_000;

/// Digits the query left-pads each id to.
const KEY_DIGITS: usize = 6;

/// Filler characters appended to each key, making every key exactly 1 KiB.
const KEY_FILLER: usize = 1018;

/// Smallest possible in-memory size of the output sort's input, in bytes.
///
/// 300,000 keys of 1,024 bytes, plus a 4-byte offset per key and one past the
/// end, plus one 8-byte count per row. Arrow cannot represent this input in
/// less, so exceeding the grant is arithmetic rather than an observation.
const SORT_INPUT_LOWER_BOUND: u64 = 307_200_000 + 1_200_004 + 2_400_000;

/// Memory ceiling the fixed pod envelope grants one Analytical query.
///
/// 512 MiB process memory less the 256 MiB unmanaged reserve leaves a 256 MiB
/// Oracle budget, and one Analytical query holding both its slot units is
/// granted all of it, clamped to the partition ceiling.
const QUERY_GRANT_BYTES: u64 = 256 * 1024 * 1024;

/// Live gauges that must read exactly zero on either side of a settled query.
const LIVE_GAUGES: [&str; 3] = [
    "bifrost_oracle_analytical_attempts_active",
    "bifrost_oracle_analytical_exchanges_active",
    "oracle_fragments_active",
];

/// Counters proving followers exchanged real data rather than empty stages.
const EXCHANGE_COUNTERS: [&str; 2] = [
    "bifrost_oracle_analytical_exchange_batches_total",
    "bifrost_oracle_analytical_exchange_bytes_total",
];

/// A join whose grouped, ordered result cannot fit its grant spills on whichever
/// Oracle coordinates it, and returns the same rows either way.
///
/// # Panics
///
/// Panics when the baseline cannot be driven across the process topology.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn inactive_baseline_executes_join_group_spill_and_interchangeable_topology() {
    prove_physical_analytical_baseline()
        .await
        .expect("physical analytical baseline journey");
}

/// Drives the baseline on two different coordinators of one live topology.
///
/// Three Oracles and one Scribe: the leader is not a participant in its own
/// worker set, so two remote followers are the smallest shape that puts real
/// stages on real peers, and running the identical statement on two of the
/// three is what makes "interchangeable" an observation rather than a claim.
///
/// # Errors
///
/// Returns the first claim that broke.
async fn prove_physical_analytical_baseline() -> Result<(), PeerJourneyError> {
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

    let mut pids = std::collections::BTreeSet::new();
    for node in cluster.nodes() {
        if !pids.insert(node.pid()) {
            return Err(format!("pod {} shares a PID with another pod", node.label()).into());
        }
    }

    let suffix = uuid::Uuid::now_v7().simple();
    let left = format!("physical_left_{suffix}");
    let right = format!("physical_right_{suffix}");
    seed(&mut cluster, &left, LEFT_ROWS)?;
    seed(&mut cluster, &right, RIGHT_ROWS)?;
    for index in [LEADER, FOLLOWERS[0], FOLLOWERS[1], SCRIBE] {
        cluster.nodes_mut()[index].refresh_snapshot()?;
    }

    let sql = format!(
        "SELECT LPAD(CAST(l.id AS VARCHAR), {KEY_DIGITS}, '0') || REPEAT('x', {KEY_FILLER}) \
         AS filter_key, COUNT(*) AS matched \
         FROM vala.bifrost.{left} AS l \
         JOIN vala.bifrost.{right} AS r ON l.id = r.id \
         GROUP BY l.id ORDER BY filter_key"
    );
    let expected_digest = expected_result_digest();

    // Two different coordinators of the same cluster, same statement. Any
    // configuration that made one Oracle special would diverge here.
    let mut digests = Vec::new();
    for coordinator in [0, 1] {
        digests.push(coordinate_baseline(&mut cluster, coordinator, &sql, &expected_digest).await?);
    }
    if digests[0] != digests[1] {
        return Err("two coordinators of one cluster produced different results".into());
    }

    peer_planes_are_reachable_from_both_coordinators(&mut cluster).await?;

    cluster.shutdown();
    Ok(())
}

/// Publishes one contiguous fixture table through the cluster's Scribe.
///
/// # Errors
///
/// Returns the control-protocol failure unchanged.
fn seed(
    cluster: &mut BifrostProcessCluster,
    table: &str,
    rows: i64,
) -> Result<(), PeerJourneyError> {
    cluster.nodes_mut()[SCRIBE].register_table(table)?;
    let mut start_id = 0;
    while start_id < rows {
        let chunk = INGEST_CHUNK.min(rows - start_id);
        cluster.nodes_mut()[SCRIBE].ingest_rows(table, start_id, chunk, INGEST_GROUPS)?;
        start_id += chunk;
    }
    Ok(())
}

/// Runs the baseline on one coordinator and asserts everything it left behind.
///
/// Returns the coordinator's own result digest so the caller can compare two.
///
/// # Errors
///
/// Returns the first claim that broke, naming the coordinator.
async fn coordinate_baseline(
    cluster: &mut BifrostProcessCluster,
    coordinator: usize,
    sql: &str,
    expected_digest: &str,
) -> Result<String, PeerJourneyError> {
    let oracles = [0, 1, 2];
    let followers: Vec<usize> = oracles
        .into_iter()
        .filter(|it| *it != coordinator)
        .collect();

    let mut scratch_before = Vec::new();
    let mut gauges_before = Vec::new();
    let mut leases_before = Vec::new();
    for index in oracles {
        scratch_before.push(cluster.nodes_mut()[index].scratch_usage()?);
        gauges_before.push(cluster.nodes_mut()[index].metric_totals(&LIVE_GAUGES)?);
        leases_before.push(cluster.nodes_mut()[index].graph_leases()?.0);
    }

    let evidence = cluster.nodes_mut()[coordinator].execute_analytical_baseline(sql)?;

    if evidence.granted_memory_bytes != QUERY_GRANT_BYTES {
        return Err(format!(
            "coordinator {coordinator} admits an Analytical query at {} bytes, not {QUERY_GRANT_BYTES}",
            evidence.granted_memory_bytes
        )
        .into());
    }
    let expected_rows = usize::try_from(RIGHT_ROWS)?;
    if evidence.rows != expected_rows {
        return Err(format!(
            "coordinator {coordinator} returned {} rows, not {expected_rows}",
            evidence.rows
        )
        .into());
    }
    if !evidence.counts_all_one {
        return Err(format!("coordinator {coordinator} matched a key more than once").into());
    }
    if !evidence.keys_strictly_increasing {
        return Err(format!("coordinator {coordinator} returned unordered keys").into());
    }
    if evidence.result_digest != expected_digest {
        return Err(format!(
            "coordinator {coordinator} returned a result the fixture generator did not produce"
        )
        .into());
    }
    if evidence.batch_memory_bytes < SORT_INPUT_LOWER_BOUND {
        return Err(format!(
            "coordinator {coordinator} sorted {} bytes, below the arithmetic minimum \
             {SORT_INPUT_LOWER_BOUND}",
            evidence.batch_memory_bytes
        )
        .into());
    }
    if evidence.batch_memory_bytes <= QUERY_GRANT_BYTES {
        return Err(format!(
            "coordinator {coordinator} sorted {} bytes, which its {QUERY_GRANT_BYTES}-byte grant \
             could have held without spilling",
            evidence.batch_memory_bytes
        )
        .into());
    }

    let physical = evidence.physical.ok_or_else(|| {
        PeerJourneyError::from(format!(
            "coordinator {coordinator} executed no uniquely identifiable output sort"
        ))
    })?;
    if physical.sort_schema != ["filter_key".to_owned(), "matched".to_owned()] {
        return Err(format!(
            "coordinator {coordinator} sorted {:?}, not the query's own output",
            physical.sort_schema
        )
        .into());
    }
    if !physical.sort_ordering.contains("filter_key") || !physical.sort_ordering.contains("ASC") {
        return Err(format!(
            "coordinator {coordinator} ordered by {}, not ascending filter_key",
            physical.sort_ordering
        )
        .into());
    }
    if physical.spill_count == 0 || physical.spilled_bytes == 0 || physical.spilled_rows == 0 {
        return Err(format!(
            "coordinator {coordinator} reported no spill: {} spills, {} bytes, {} rows",
            physical.spill_count, physical.spilled_bytes, physical.spilled_rows
        )
        .into());
    }
    if physical.aggregate_group_types != ["Int64".to_owned()] {
        return Err(format!(
            "coordinator {coordinator} grouped on {:?}, not the narrow join key alone",
            physical.aggregate_group_types
        )
        .into());
    }
    if physical.join_build_schemas.is_empty()
        || physical
            .join_build_schemas
            .iter()
            .any(|schema| schema != &["id".to_owned()])
    {
        return Err(format!(
            "coordinator {coordinator} built its join from {:?}, not the projected id alone",
            physical.join_build_schemas
        )
        .into());
    }

    let exchanged = cluster.nodes_mut()[coordinator].metric_totals(&EXCHANGE_COUNTERS)?;
    for family in EXCHANGE_COUNTERS {
        if exchanged.get(family).copied().unwrap_or_default() <= 0.0 {
            return Err(format!("coordinator {coordinator} recorded no {family}").into());
        }
    }

    // Every follower did work this attempt: a graph it leased, and the scan
    // that lease admitted. A topology where one Oracle silently did nothing
    // would return the same rows and none of this.
    for index in &followers {
        let activated = cluster.nodes_mut()[*index].graph_leases()?.0;
        if activated <= leases_before[*index] {
            return Err(format!(
                "follower {index} activated no graph for coordinator {coordinator}"
            )
            .into());
        }
    }

    for index in oracles {
        let (activated, live) = await_released_lease(cluster, index).await?;
        let _ = activated;
        if live != 0 {
            return Err(format!("Oracle {index} still holds {live} graph leases").into());
        }
        let scratch = cluster.nodes_mut()[index].scratch_usage()?;
        if scratch != scratch_before[index] {
            return Err(format!(
                "Oracle {index} left scratch at {scratch:?}, not its {:?} baseline",
                scratch_before[index]
            )
            .into());
        }
        let gauges = cluster.nodes_mut()[index].metric_totals(&LIVE_GAUGES)?;
        if gauges != gauges_before[index] {
            return Err(format!(
                "Oracle {index} left live gauges at {gauges:?}, not its {:?} baseline",
                gauges_before[index]
            )
            .into());
        }
    }

    Ok(evidence.result_digest)
}

/// Both coordinators still answer on the public and private planes afterwards.
///
/// The peer services must remain absent from the public listener and mounted on
/// the private one: a distributed execution that reached across processes and
/// left either plane changed is a boundary failure, not a completed query.
///
/// # Errors
///
/// Returns the first plane whose behavior diverged.
async fn peer_planes_are_reachable_from_both_coordinators(
    cluster: &mut BifrostProcessCluster,
) -> Result<(), PeerJourneyError> {
    for coordinator in [0_usize, 1] {
        let address = format!("http://{}", cluster.nodes()[coordinator].grpc_addr());
        let channel = wyrd_tonic::transport::plaintext_endpoint(address)?
            .connect()
            .await?;
        match super::support::probe_oracle_peer(channel).await {
            Err(status) if status.code() == wyrd_tonic::tonic::Code::Unimplemented => {}
            Err(status) => {
                return Err(format!(
                    "OraclePeerService answered {:?} on coordinator {coordinator}'s public listener",
                    status.code()
                )
                .into());
            }
            Ok(()) => {
                return Err(format!(
                    "OraclePeerService served a request on coordinator {coordinator}'s \
                     public listener"
                )
                .into());
            }
        }

        let peer = cluster.nodes()[coordinator]
            .ready_report()
            .advertise_addr
            .clone();
        let before = super::support::polls_at(cluster, coordinator)?;
        super::support::probe_from(
            cluster,
            SCRIBE,
            &wyrd_testing::bifrost::process_cluster::PeerProbePlan::own(&peer),
        )?;
        if super::support::polls_at(cluster, coordinator)? == before {
            return Err(format!(
                "coordinator {coordinator} admitted no body on its private peer listener"
            )
            .into());
        }
    }
    Ok(())
}

/// Recomputes the exact result the fixture tables must produce.
///
/// Generated from the fixture's own definition rather than from anything the
/// cluster returned, so a query that silently dropped, duplicated, or reordered
/// rows cannot agree with it.
fn expected_result_digest() -> String {
    use sha2::Digest as _;

    let mut digest = sha2::Sha256::new();
    for id in 0..RIGHT_ROWS {
        let key = format!("{id:0KEY_DIGITS$}{filler}", filler = "x".repeat(KEY_FILLER));
        digest.update((key.len() as u32).to_le_bytes().as_slice());
        digest.update(key.as_bytes());
        digest.update(1_i64.to_le_bytes().as_slice());
    }
    format!("{:x}", digest.finalize())
}
