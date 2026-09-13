use wyrd_client::WyrdClient;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_spec::vala::api::{BifrostQueryRequest, FreshnessPolicy, VisibilityMode};
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

    cluster.shutdown()?;
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
    // A pod this journey killed on purpose has no stdin left to accept a
    // shutdown request, so explicit shutdown reports it. Asserting on that
    // rather than discarding it is the point: the report must name exactly the
    // pod this journey removed, which means every other pod was still asked,
    // reaped, and joined.
    let killed = cluster.nodes()[paused].label().to_owned();
    match (cause, cluster.shutdown()) {
        (TerminalCause::Cancellation, Ok(())) => {}
        (TerminalCause::PeerLoss, Err(reported)) => {
            let detail = reported.to_string();
            if !detail.contains(&killed) || detail.matches("pod-").count() != 1 {
                return Err(format!(
                    "shutdown after killing {killed} reported {detail}, not that pod alone"
                )
                .into());
            }
        }
        (cause, result) => {
            return Err(format!("shutdown after {cause:?} reported {result:?}").into());
        }
    }
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
const LEFT_ROWS: i64 = crate::support::ANALYTICAL_LEFT_ROWS;

/// Right-table rows; the join key range that actually matches.
const RIGHT_ROWS: i64 = crate::support::ANALYTICAL_RIGHT_ROWS;

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

    let sql = crate::support::analytical_baseline_sql(&left, &right);
    let expected_digest = crate::support::expected_analytical_digest();

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

    cluster.shutdown()?;
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

    let mut ownership_before = Vec::new();
    let mut leases_before = Vec::new();
    for index in oracles {
        ownership_before.push(cluster.nodes_mut()[index].ownership_snapshot()?);
        leases_before.push(cluster.nodes_mut()[index].graph_leases()?.0);
    }
    // Sampled inside this iteration, not once for the whole journey: these are
    // cumulative counters, so a second coordinator that exchanged nothing would
    // still read above zero on its predecessor's totals.
    let exchanged_before = cluster.nodes_mut()[coordinator].metric_totals(&EXCHANGE_COUNTERS)?;

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
        let before = exchanged_before.get(family).copied().unwrap_or_default();
        let after = exchanged.get(family).copied().unwrap_or_default();
        if after <= before {
            return Err(format!(
                "coordinator {coordinator} recorded no {family} for this execution: \
                 {before} then {after}"
            )
            .into());
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
        // Complete ownership, not a chosen subset: a graph, attempt, cleanup
        // failure, admitted or queued query, peer reservation, slot unit,
        // query memory or scratch owner, spill entry, or live gauge that this
        // execution failed to return diverges here.
        let ownership = cluster.nodes_mut()[index].ownership_snapshot()?;
        if ownership != ownership_before[index] {
            return Err(format!(
                "Oracle {index} left ownership at {ownership:?}, not its {:?} baseline",
                ownership_before[index]
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

/// One representative query style per operator family, all on real peers.
///
/// # Panics
///
/// Panics when a style cannot be driven across the process topology.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn stage_graph_executes_representative_query_styles() {
    prove_representative_query_styles()
        .await
        .expect("representative query style journey");
}

/// Drives the four query styles a stage graph must support end to end.
///
/// The baseline journey proves one qualified statement in depth. This one
/// proves breadth: a partitioned scan with a filter and projection, a grouped
/// aggregation, a left equi-join whose unmatched rows survive, and a sorted
/// statement bounded by a limit. Each runs on the same live three-Oracle
/// topology, returns a row count the fixture's own definition fixes, and must
/// leave every pod exactly as it found it.
///
/// # Errors
///
/// Returns the first claim that broke, naming the style.
async fn prove_representative_query_styles() -> Result<(), PeerJourneyError> {
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

    let suffix = uuid::Uuid::now_v7().simple();
    let left = format!("styles_left_{suffix}");
    let right = format!("styles_right_{suffix}");
    seed(&mut cluster, &left, LEFT_ROWS)?;
    seed(&mut cluster, &right, RIGHT_ROWS)?;
    for index in [LEADER, FOLLOWERS[0], FOLLOWERS[1], SCRIBE] {
        cluster.nodes_mut()[index].refresh_snapshot()?;
    }

    for style in query_styles(&left, &right) {
        execute_style(&mut cluster, &style).await?;
    }

    prove_canonical_genai_span(&mut cluster).await?;

    cluster.shutdown()?;
    Ok(())
}

/// One representative statement and everything its result must satisfy.
struct QueryStyle {
    /// Operator family this statement stands for.
    name: &'static str,
    /// The statement itself, qualified against this journey's fixture tables.
    sql: String,
    /// Rows the fixture's definition fixes for this statement.
    rows: usize,
    /// Whether the result must be strictly ascending in its first column.
    ordered: bool,
    /// Whether every grouped count in the result must be exactly one.
    unique_counts: bool,
    /// Whether the statement's sort input must exceed its grant and spill.
    spills: bool,
}

/// Builds the four representative statements over one seeded fixture pair.
///
/// Every row count here is derived from the fixture generator rather than from
/// an observation: ids are contiguous and `filter_key` is `id % INGEST_GROUPS`,
/// so each claim below fails if the engine drops, duplicates, or silently
/// converts a join.
fn query_styles(left: &str, right: &str) -> Vec<QueryStyle> {
    let wide_key = format!(
        "LPAD(CAST(l.id AS VARCHAR), {digits}, '0') || REPEAT('x', {filler})",
        digits = crate::support::ANALYTICAL_KEY_DIGITS,
        filler = crate::support::ANALYTICAL_KEY_FILLER
    );
    vec![
        QueryStyle {
            name: "partitioned scan, filter, and projection",
            sql: format!(
                "SELECT id FROM vala.bifrost.{left} WHERE filter_key = 'group_7' \
                 AND id < {LEFT_ROWS}"
            ),
            // One id per full pass of the group cycle across the whole table.
            rows: usize::try_from(LEFT_ROWS / INGEST_GROUPS).unwrap_or(usize::MAX),
            ordered: false,
            unique_counts: false,
            spills: false,
        },
        QueryStyle {
            name: "grouped aggregation",
            sql: format!(
                "SELECT filter_key, COUNT(*) AS matched FROM vala.bifrost.{left} \
                 GROUP BY filter_key"
            ),
            rows: usize::try_from(INGEST_GROUPS).unwrap_or(usize::MAX),
            ordered: false,
            unique_counts: false,
            spills: false,
        },
        QueryStyle {
            name: "left equi-join",
            // The left table is wider than the right, so an inner join would
            // return RIGHT_ROWS here. Only a left join keeps the unmatched ids,
            // and only a correct one keeps each of them exactly once.
            sql: format!(
                "SELECT {wide_key} AS filter_key, COUNT(*) AS matched \
                 FROM vala.bifrost.{left} AS l \
                 LEFT JOIN vala.bifrost.{right} AS r ON l.id = r.id \
                 GROUP BY l.id ORDER BY filter_key"
            ),
            rows: usize::try_from(LEFT_ROWS).unwrap_or(usize::MAX),
            ordered: true,
            unique_counts: true,
            spills: true,
        },
        QueryStyle {
            name: "sort with limit",
            sql: format!(
                "SELECT {wide_key} AS filter_key, COUNT(*) AS matched \
                 FROM vala.bifrost.{left} AS l \
                 GROUP BY l.id ORDER BY filter_key LIMIT {SORT_LIMIT_ROWS}"
            ),
            rows: SORT_LIMIT_ROWS,
            ordered: true,
            unique_counts: true,
            spills: false,
        },
    ]
}

/// Rows the bounded sort style keeps out of the whole left table.
const SORT_LIMIT_ROWS: usize = 5;

/// Runs one style on the leader and asserts its result, exchange, and cleanup.
///
/// # Errors
///
/// Returns the first claim that broke, naming the style.
async fn execute_style(
    cluster: &mut BifrostProcessCluster,
    style: &QueryStyle,
) -> Result<(), PeerJourneyError> {
    let oracles = [0, 1, 2];
    let mut ownership_before = Vec::new();
    let mut leases_before = Vec::new();
    for index in oracles {
        ownership_before.push(cluster.nodes_mut()[index].ownership_snapshot()?);
        leases_before.push(cluster.nodes_mut()[index].graph_leases()?.0);
    }
    let exchanged_before = cluster.nodes_mut()[LEADER].metric_totals(&EXCHANGE_COUNTERS)?;

    let name = style.name;
    let evidence = cluster.nodes_mut()[LEADER]
        .execute_analytical_baseline(&style.sql)
        .map_err(|error| PeerJourneyError::from(format!("{name} failed: {error}")))?;

    if evidence.rows != style.rows {
        return Err(format!(
            "{name} returned {} rows, not the {} its fixture fixes",
            evidence.rows, style.rows
        )
        .into());
    }
    if style.unique_counts && !evidence.counts_all_one {
        return Err(format!("{name} matched a grouped key more than once").into());
    }
    if style.ordered && !evidence.keys_strictly_increasing {
        return Err(format!("{name} returned unordered keys").into());
    }

    // Only a statement with an output sort carries physical evidence, and only
    // one whose sort input exceeds its grant may spill. A style that claims
    // either and produced neither is the failure.
    match (&evidence.physical, style.ordered) {
        (Some(physical), true) => {
            if style.spills && (physical.spill_count == 0 || physical.spilled_rows == 0) {
                return Err(format!(
                    "{name} reported no spill: {} spills, {} rows",
                    physical.spill_count, physical.spilled_rows
                )
                .into());
            }
        }
        (None, true) => {
            return Err(format!("{name} recorded no output-sort evidence").into());
        }
        (evidence, false) => {
            if let Some(evidence) = evidence {
                return Err(format!("{name} sorts nothing yet recorded {evidence:?}").into());
            }
        }
    }

    // Every style is a distributed graph, not a leader-local rewrite: a stage
    // that never crossed a peer socket would return the same rows and move no
    // exchange counter at all.
    let exchanged = cluster.nodes_mut()[LEADER].metric_totals(&EXCHANGE_COUNTERS)?;
    for family in EXCHANGE_COUNTERS {
        let before = exchanged_before.get(family).copied().unwrap_or_default();
        let after = exchanged.get(family).copied().unwrap_or_default();
        if after <= before {
            return Err(format!("{name} recorded no {family}: {before} then {after}").into());
        }
    }
    // How many followers a style reaches is the planner's decision — a single
    // scan stage may fit one task — so the claim is that real remote work
    // happened, not that every peer received some.
    let mut activated_followers = 0;
    for index in FOLLOWERS {
        if cluster.nodes_mut()[index].graph_leases()?.0 > leases_before[index] {
            activated_followers += 1;
        }
    }
    if activated_followers == 0 {
        return Err(format!("{name} activated no graph on any follower").into());
    }

    for index in oracles {
        let (_, live) = await_released_lease(cluster, index).await?;
        if live != 0 {
            return Err(format!("{name} left {live} graph leases on Oracle {index}").into());
        }
        let ownership = cluster.nodes_mut()[index].ownership_snapshot()?;
        if ownership != ownership_before[index] {
            return Err(format!(
                "{name} left Oracle {index} at {ownership:?}, not its {:?} baseline",
                ownership_before[index]
            )
            .into());
        }
    }
    Ok(())
}

/// Carries one complete GenAI span through this live topology, end to end.
///
/// The distributed styles above prove operator families over fixture rows. This
/// proves that the same topology carries the signal the product exists to
/// store: one parent/child GenAI trace with an event, a link, an error status,
/// resource and scope metadata, promoted GenAI fields, and the structured
/// input/output messages the payload gate protects. Nothing here reaches into a
/// pod: the span arrives through the Scribe's public OTLP route with a
/// provisioned credential, and it is read back through the leader's public
/// query listener over the same stage graph the styles used.
///
/// A second data tenant then runs the identical statement against the identical
/// listener. It must see none of it, which is the tenant tripwire stated as a
/// read rather than as a configuration.
///
/// # Errors
///
/// Returns the first claim that broke: the OTLP export, the publication, the
/// readback, or the foreign tenant reaching another tenant's span.
async fn prove_canonical_genai_span(
    cluster: &mut BifrostProcessCluster,
) -> Result<(), PeerJourneyError> {
    use wyrd_testing::bifrost::canonical_signals as fixture;

    let api_key = cluster
        .provision_public_api_key("canonical-signal-caller")
        .await?;
    let scope = format!("wyrd.peer.canonical.{}", uuid::Uuid::now_v7().simple());
    export_canonical_trace(&cluster.nodes()[SCRIBE], &api_key, &scope).await?;
    cluster.nodes_mut()[SCRIBE].flush()?;
    for index in [LEADER, FOLLOWERS[0], FOLLOWERS[1], SCRIBE] {
        cluster.nodes_mut()[index].refresh_snapshot()?;
    }

    let sql = format!(
        "SELECT name, gen_ai_operation_name, gen_ai_provider_name, gen_ai_request_model, \
                status_code, \
                CAST(gen_ai_usage_input_tokens AS BIGINT) AS input_tokens, \
                CAST(gen_ai_usage_output_tokens AS BIGINT) AS output_tokens, \
                CAST(array_length(events) AS BIGINT) AS events, \
                CAST(array_length(links) AS BIGINT) AS links, \
                events[1]['name'] AS event_name, links[1]['trace_state'] AS link_state, \
                service_name \
         FROM vala.traces.spans \
         WHERE scope_name = '{scope}' AND parent_span_id IS NULL"
    );
    let leader = public_client(&cluster.nodes()[LEADER], &api_key)?;
    let parent = public_query(&leader, &sql).await?;
    if parent.len() != 1 {
        return Err(format!(
            "the canonical parent span read back as {} rows, not one",
            parent.len()
        )
        .into());
    }
    let parent = &parent[0];
    for (column, expected) in [
        ("gen_ai_operation_name", "chat".to_owned()),
        ("gen_ai_provider_name", "anthropic".to_owned()),
        ("gen_ai_request_model", fixture::MODEL.to_owned()),
        ("event_name", fixture::EVENT_NAME.to_owned()),
        ("link_state", fixture::LINK_TRACE_STATE.to_owned()),
        ("service_name", CANONICAL_SERVICE.to_owned()),
    ] {
        let actual = parent.get(column).cloned().unwrap_or_default();
        if actual != expected {
            return Err(
                format!("the parent span reports {column} as {actual}, not {expected}").into(),
            );
        }
    }
    for (column, expected) in [
        ("input_tokens", fixture::INPUT_TOKENS.to_string()),
        ("output_tokens", fixture::OUTPUT_TOKENS.to_string()),
        ("status_code", "1".to_owned()),
        ("events", "1".to_owned()),
        ("links", "1".to_owned()),
    ] {
        let actual = parent.get(column).cloned().unwrap_or_default();
        if actual != expected {
            return Err(
                format!("the parent span reports {column} as {actual}, not {expected}").into(),
            );
        }
    }

    // The child is the same trace's tool call: it carries the parent's id and
    // the error status, so the hierarchy survived the round trip intact.
    let child = public_query(
        &leader,
        &format!(
            "SELECT name, gen_ai_operation_name, status_code \
             FROM vala.traces.spans \
             WHERE scope_name = '{scope}' AND parent_span_id IS NOT NULL"
        ),
    )
    .await?;
    if child.len() != 1 {
        return Err(format!("the trace carries {} child spans, not one", child.len()).into());
    }
    if child[0].get("gen_ai_operation_name").map(String::as_str) != Some("execute_tool")
        || child[0].get("status_code").map(String::as_str) != Some("2")
    {
        return Err(format!("the child span read back as {:?}", child[0]).into());
    }

    // The structured GenAI messages stay in the sensitive payload, which this
    // credential is entitled to read.
    let payload = public_query(
        &leader,
        &format!(
            "SELECT attributes FROM vala.traces.spans \
             WHERE scope_name = '{scope}' AND parent_span_id IS NULL"
        ),
    )
    .await?;
    let rendered = payload
        .first()
        .and_then(|row| row.get("attributes"))
        .cloned()
        .unwrap_or_default();
    for messages in [fixture::INPUT_MESSAGES, fixture::OUTPUT_MESSAGES] {
        if !rendered.contains(&hex::encode(messages)) {
            return Err("the structured GenAI messages did not survive the round trip".into());
        }
    }

    // The tripwire: another tenant on the same listener reaches none of it.
    let foreign_key = cluster
        .provision_foreign_public_api_key("canonical-foreign")
        .await?;
    let foreign = public_client(&cluster.nodes()[LEADER], &foreign_key)?;
    if let Ok(rows) = public_query(&foreign, &sql).await
        && !rows.is_empty()
    {
        return Err(format!(
            "a foreign tenant read {} rows of another tenant's canonical span",
            rows.len()
        )
        .into());
    }
    Ok(())
}

/// The `service.name` the canonical export declares on its resource.
const CANONICAL_SERVICE: &str = "wyrd.peer.canonical";

/// Sends the canonical parent/child GenAI trace to one pod's public OTLP route.
///
/// The export is the door a real tracer arrives through, so it also provisions
/// the canonical built-in table: no journey-side registration precedes it.
///
/// # Errors
///
/// Returns a failure when the credential cannot be exchanged, the request
/// cannot be sent, or the pod refuses the export.
async fn export_canonical_trace(
    node: &wyrd_testing::bifrost::process_cluster::ProcessNode,
    api_key: &secrecy::SecretString,
    scope: &str,
) -> Result<(), PeerJourneyError> {
    use wyrd_testing::bifrost::canonical_signals as fixture;

    let anchor = 1_760_000_000_000_000_000_i64;
    let string_attribute =
        |key: &str, value: &str| serde_json::json!({"key": key, "value": {"stringValue": value}});
    let int_attribute = |key: &str, value: i64| serde_json::json!({"key": key, "value": {"intValue": value.to_string()}});
    let parent = serde_json::json!({
        "traceId": hex::encode(fixture::TRACE_ID),
        "spanId": hex::encode(fixture::PARENT_SPAN_ID),
        "name": "chat claude-opus-5",
        "kind": 3,
        "startTimeUnixNano": anchor.to_string(),
        "endTimeUnixNano": (anchor + 2_000_000).to_string(),
        "status": {"code": 1, "message": "ok"},
        "attributes": [
            string_attribute("gen_ai.operation.name", "chat"),
            string_attribute("gen_ai.provider.name", "anthropic"),
            string_attribute("gen_ai.request.model", fixture::MODEL),
            string_attribute("gen_ai.conversation.id", "conversation-fixture"),
            int_attribute("gen_ai.usage.input_tokens", fixture::INPUT_TOKENS),
            int_attribute("gen_ai.usage.output_tokens", fixture::OUTPUT_TOKENS),
            string_attribute("gen_ai.input.messages", fixture::INPUT_MESSAGES),
            string_attribute("gen_ai.output.messages", fixture::OUTPUT_MESSAGES),
        ],
        "events": [{
            "timeUnixNano": (anchor + 1_000_000).to_string(),
            "name": fixture::EVENT_NAME,
            "attributes": [string_attribute("gen_ai.finish_reason", "stop")],
        }],
        "links": [{
            "traceId": hex::encode(fixture::TRACE_ID),
            "spanId": hex::encode(fixture::LINKED_SPAN_ID),
            "traceState": fixture::LINK_TRACE_STATE,
            "attributes": [string_attribute("link.kind", "follows_from")],
        }],
    });
    let child = serde_json::json!({
        "traceId": hex::encode(fixture::TRACE_ID),
        "spanId": hex::encode(fixture::CHILD_SPAN_ID),
        "parentSpanId": hex::encode(fixture::PARENT_SPAN_ID),
        "name": "execute_tool search",
        "kind": 1,
        "startTimeUnixNano": (anchor + 100_000).to_string(),
        "endTimeUnixNano": (anchor + 900_000).to_string(),
        "status": {"code": 2, "message": "tool call exhausted its retry budget"},
        "attributes": [
            string_attribute("gen_ai.operation.name", "execute_tool"),
            string_attribute("gen_ai.provider.name", "anthropic"),
            string_attribute("gen_ai.request.model", fixture::MODEL),
            string_attribute("gen_ai.tool.name", "search"),
        ],
    });

    let client = public_client(node, api_key)?;
    let bearer = client
        .auth()
        .bearer()
        .await
        .map_err(|error| PeerJourneyError::from(error.to_string()))?;
    let response = reqwest::Client::new()
        .post(format!("http://{}/v1/traces", node.http_addr()))
        .header("x-wyrd-access-token", format!("Bearer {}", bearer.expose()))
        .json(&serde_json::json!({"resourceSpans": [{
            "resource": {
                "attributes": [string_attribute("service.name", CANONICAL_SERVICE)],
            },
            "scopeSpans": [{
                "scope": {"name": scope, "version": "1.0.0"},
                "spans": [parent, child],
            }],
        }]}))
        .send()
        .await
        .map_err(|error| PeerJourneyError::from(error.to_string()))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("the canonical OTLP export was refused ({status}): {body}").into());
    }
    Ok(())
}

/// Builds one ordinary external client against a pod's public listeners.
///
/// # Errors
///
/// Returns a failure when the endpoints do not form a client.
fn public_client(
    node: &wyrd_testing::bifrost::process_cluster::ProcessNode,
    api_key: &secrecy::SecretString,
) -> Result<WyrdClient, PeerJourneyError> {
    WyrdClient::with_config(ClientConfig {
        grpc: GrpcConfig {
            endpoint: format!("http://{}", node.grpc_addr()),
            connect_retries: 0,
            ..GrpcConfig::default()
        },
        http: HttpConfig {
            base_url: format!("http://{}", node.http_addr()),
            ..HttpConfig::default()
        },
        credential: Some(api_key.clone()),
        ..ClientConfig::default()
    })
    .map_err(|error| PeerJourneyError::from(error.to_string()))
}

/// Runs one public query and renders every returned row as displayed text.
///
/// The journey asserts on fixed literal values rather than on Arrow layout, so
/// rendering each column with its own `Display` keeps the assertions readable
/// and type-independent; a binary payload renders as hex, which is what the
/// structured-message claim matches against.
///
/// # Errors
///
/// Returns a failure when the query is refused, a batch does not arrive, or the
/// stream carries no terminal frame.
async fn public_query(
    client: &WyrdClient,
    sql: &str,
) -> Result<Vec<std::collections::BTreeMap<String, String>>, PeerJourneyError> {
    let mut stream = wyrd_client::Bifrost::query_only(client)
        .query_client()
        .clone()
        .query(&BifrostQueryRequest {
            sql: sql.to_owned(),
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: None,
        })
        .await
        .map_err(|error| PeerJourneyError::from(error.to_string()))?;
    let mut rows = Vec::new();
    while let Some(batch) = stream
        .next_batch()
        .await
        .map_err(|error| PeerJourneyError::from(error.to_string()))?
    {
        for index in 0..batch.num_rows() {
            let mut row = std::collections::BTreeMap::new();
            for (position, field) in batch.schema().fields().iter().enumerate() {
                let column = batch.column(position);
                let rendered = if column.is_null(index) {
                    String::new()
                } else {
                    arrow::util::display::array_value_to_string(column, index)
                        .map_err(|error| PeerJourneyError::from(error.to_string()))?
                };
                row.insert(field.name().clone(), rendered);
            }
            rows.push(row);
        }
    }
    stream
        .terminal()
        .ok_or("the canonical readback produced no terminal frame")?;
    Ok(rows)
}
