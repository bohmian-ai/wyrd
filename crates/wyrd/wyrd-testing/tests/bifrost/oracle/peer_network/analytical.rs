use wyrd_client::WyrdClient;
use wyrd_server::config::BifrostTarget;
use wyrd_spec::vala::api::BifrostQueryRequest;

use super::support::PeerJourneyError;
use crate::peer_cluster::PeerCluster;
use crate::support::{metric_value, public_client, wait_for_spans};
use wyrd_telemetry::CapturedSpan;
use wyrd_testing::bifrost::BifrostTelemetryCheckpoint;
use wyrd_testing::bifrost::telemetry::BifrostMetricKind;

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
    let cluster = PeerCluster::start(&[
        BifrostTarget::Oracle,
        BifrostTarget::Oracle,
        BifrostTarget::Oracle,
        BifrostTarget::Scribe,
    ])
    .await?;

    let table = format!("graph_lease_{}", uuid::Uuid::now_v7().simple());
    cluster.register_table(SCRIBE, &table).await?;
    cluster.ingest_rows(SCRIBE, &table, 0, 12, 3).await?;
    cluster.ingest_rows(SCRIBE, &table, 0, 12, 3).await?;
    for index in [LEADER, FOLLOWERS[0], FOLLOWERS[1], SCRIBE] {
        cluster.refresh_snapshot(index).await?;
    }

    // Baseline first: every later count is a difference from a node that is
    // provably holding nothing, so a leak from an earlier lane cannot be read
    // as this attempt's own release.
    for index in FOLLOWERS {
        let (activated, live) = cluster.graph_leases(index)?;
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
    let rows = cluster.execute_sql(LEADER, &sql).await?;
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
        let (activated, live) = cluster.graph_leases(index)?;
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
    let repeated = cluster.execute_sql(LEADER, &sql).await?;
    if repeated != 3 {
        return Err(PeerJourneyError::from(format!(
            "the repeated attempt must return one row per group, returned {repeated}"
        )));
    }
    for index in FOLLOWERS {
        let (activated, live) = cluster.graph_leases(index)?;
        if (activated, live) != (2, 0) {
            return Err(PeerJourneyError::from(format!(
                "follower {index} must activate and release one lease per attempt, \
                 reported ({activated}, {live})"
            )));
        }
    }

    cluster.shutdown().await?;
    Ok(())
}

/// Peer loss and cancellation each end one attempt on every reachable process.
///
/// # Panics
///
/// Panics when either ordering starts a successor or leaves ownership behind.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn one_attempt_peer_loss_and_cancellation_join_every_pod() {
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
    let mut cluster = PeerCluster::start(&[
        BifrostTarget::Oracle,
        BifrostTarget::Oracle,
        BifrostTarget::Oracle,
        BifrostTarget::Scribe,
    ])
    .await?;

    let table = format!("one_attempt_{}", uuid::Uuid::now_v7().simple());
    cluster.register_table(SCRIBE, &table).await?;
    cluster.ingest_rows(SCRIBE, &table, 0, 12, 3).await?;
    cluster.ingest_rows(SCRIBE, &table, 0, 12, 3).await?;
    for index in [LEADER, FOLLOWERS[0], FOLLOWERS[1], SCRIBE] {
        cluster.refresh_snapshot(index).await?;
    }

    // Armed before the query, so the follower is held at a real boundary: its
    // graph lease is active and it has consumed no source.
    let paused = FOLLOWERS[0];
    cluster.arm_execute_pause(paused)?;

    let sql = format!(
        "SELECT filter_key, COUNT(*) AS matched FROM vala.bifrost.{table} \
         GROUP BY filter_key ORDER BY filter_key"
    );
    cluster.start_sql(LEADER, &sql)?;
    cluster.await_execute_paused(paused).await?;

    let (activated, live) = cluster.graph_leases(paused)?;
    if (activated, live) != (1, 1) {
        return Err(PeerJourneyError::from(format!(
            "the paused follower must hold exactly one activated lease, held ({activated}, {live})"
        )));
    }

    match cause {
        TerminalCause::PeerLoss => cluster.kill(paused).await?,
        TerminalCause::Cancellation => cluster.cancel_sql(LEADER)?,
    }

    let outcome = cluster.await_sql(LEADER).await?;
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
        cluster.release_execute_pause(paused)?;
    }
    // Every surviving pod drains cleanly; the killed pod is already gone.
    cluster.shutdown().await?;
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
    cluster: &mut PeerCluster,
    index: usize,
) -> Result<(u64, usize), PeerJourneyError> {
    let mut last = (0, 0);
    for _ in 0..CLEAN_LEASE_POLLS {
        last = cluster.graph_leases(index)?;
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

/// Counters proving followers exchanged real data rather than empty stages.
const EXCHANGE_COUNTERS: [&str; 2] = [
    "bifrost_oracle_analytical_exchange_batches_total",
    "bifrost_oracle_analytical_exchange_bytes_total",
];

/// A distributed join, grouped aggregate, and output sort returns the same rows
/// on whichever Oracle coordinates it.
///
/// # Panics
///
/// Panics when the baseline cannot be driven across the process topology.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn baseline_executes_join_group_sort_and_interchangeable_topology() {
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
    let mut cluster = PeerCluster::start(&[
        BifrostTarget::Oracle,
        BifrostTarget::Oracle,
        BifrostTarget::Oracle,
        BifrostTarget::Scribe,
    ])
    .await?;

    let mut sockets = std::collections::BTreeSet::new();
    for index in 0..cluster.len() {
        if !sockets.insert(cluster.peer_addr(index)?) {
            return Err(format!("pod {index} shares a peer socket with another pod").into());
        }
    }

    let suffix = uuid::Uuid::now_v7().simple();
    let left = format!("physical_left_{suffix}");
    let right = format!("physical_right_{suffix}");
    seed(&cluster, &left, LEFT_ROWS).await?;
    seed(&cluster, &right, RIGHT_ROWS).await?;
    for index in [LEADER, FOLLOWERS[0], FOLLOWERS[1], SCRIBE] {
        cluster.refresh_snapshot(index).await?;
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

    cluster.shutdown().await?;
    Ok(())
}

/// Publishes one contiguous fixture table through the cluster's Scribe.
///
/// # Errors
///
/// Returns the registration or ingest failure unchanged.
async fn seed(cluster: &PeerCluster, table: &str, rows: i64) -> Result<(), PeerJourneyError> {
    cluster.register_table(SCRIBE, table).await?;
    let mut start_id = 0;
    while start_id < rows {
        let chunk = INGEST_CHUNK.min(rows - start_id);
        cluster
            .ingest_rows(SCRIBE, table, start_id, chunk, INGEST_GROUPS)
            .await?;
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
    cluster: &mut PeerCluster,
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
        ownership_before.push(cluster.ownership_snapshot(index)?);
        leases_before.push(cluster.graph_leases(index)?.0);
    }
    // Sampled inside this iteration, not once for the whole journey: these are
    // cumulative counters, so a second coordinator that exchanged nothing would
    // still read above zero on its predecessor's totals.
    let exchanged_before = cluster.metric_totals(&EXCHANGE_COUNTERS)?;

    let evidence = cluster
        .execute_analytical_baseline(coordinator, sql)
        .await?;

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

    let exchanged = cluster.metric_totals(&EXCHANGE_COUNTERS)?;
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
        let activated = cluster.graph_leases(*index)?.0;
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
        let ownership = cluster.ownership_snapshot(index)?;
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
    cluster: &mut PeerCluster,
) -> Result<(), PeerJourneyError> {
    for coordinator in [0_usize, 1] {
        let address = cluster
            .server(coordinator)?
            .grpc_url()
            .ok_or("a coordinator bound no public gRPC socket")?;
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

        // The body-poll counter is process-wide; the settled topology sends
        // no peer traffic of its own, so the advance is this probe's.
        let peer = cluster.advertise_addr(coordinator).await?;
        let before = cluster.peer_body_polls();
        cluster
            .probe(&crate::peer_cluster::PeerProbePlan::own(&peer))
            .await?;
        if cluster.peer_body_polls() == before {
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
    let mut cluster = PeerCluster::start(&[
        BifrostTarget::Oracle,
        BifrostTarget::Oracle,
        BifrostTarget::Oracle,
        BifrostTarget::Scribe,
    ])
    .await?;

    let suffix = uuid::Uuid::now_v7().simple();
    let left = format!("styles_left_{suffix}");
    let right = format!("styles_right_{suffix}");
    seed(&cluster, &left, LEFT_ROWS).await?;
    seed(&cluster, &right, RIGHT_ROWS).await?;
    for index in [LEADER, FOLLOWERS[0], FOLLOWERS[1], SCRIBE] {
        cluster.refresh_snapshot(index).await?;
    }

    for style in query_styles(&left, &right) {
        execute_style(&mut cluster, &style).await?;
    }

    prove_canonical_genai_span(&mut cluster).await?;

    cluster.shutdown().await?;
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
}

/// Builds the four representative statements over one seeded fixture pair.
///
/// Every row count here is derived from the fixture generator rather than from
/// an observation: ids are contiguous and `filter_key` is `id % INGEST_GROUPS`,
/// so each claim below fails if the engine drops, duplicates, or silently
/// converts a join.
fn query_styles(left: &str, right: &str) -> Vec<QueryStyle> {
    let key = format!(
        "LPAD(CAST(l.id AS VARCHAR), {digits}, '0')",
        digits = crate::support::ANALYTICAL_KEY_DIGITS
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
        },
        QueryStyle {
            name: "left equi-join",
            // The left table is wider than the right, so an inner join would
            // return RIGHT_ROWS here. Only a left join keeps the unmatched ids,
            // and only a correct one keeps each of them exactly once.
            sql: format!(
                "SELECT {key} AS filter_key, COUNT(*) AS matched \
                 FROM vala.bifrost.{left} AS l \
                 LEFT JOIN vala.bifrost.{right} AS r ON l.id = r.id \
                 GROUP BY l.id ORDER BY filter_key"
            ),
            rows: usize::try_from(LEFT_ROWS).unwrap_or(usize::MAX),
            ordered: true,
            unique_counts: true,
        },
        QueryStyle {
            name: "sort with limit",
            sql: format!(
                "SELECT {key} AS filter_key, COUNT(*) AS matched \
                 FROM vala.bifrost.{left} AS l \
                 GROUP BY l.id ORDER BY filter_key LIMIT {SORT_LIMIT_ROWS}"
            ),
            rows: SORT_LIMIT_ROWS,
            ordered: true,
            unique_counts: true,
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
    cluster: &mut PeerCluster,
    style: &QueryStyle,
) -> Result<(), PeerJourneyError> {
    let oracles = [0, 1, 2];
    let mut ownership_before = Vec::new();
    let mut leases_before = Vec::new();
    for index in oracles {
        ownership_before.push(cluster.ownership_snapshot(index)?);
        leases_before.push(cluster.graph_leases(index)?.0);
    }
    let exchanged_before = cluster.metric_totals(&EXCHANGE_COUNTERS)?;

    let name = style.name;
    let evidence = cluster
        .execute_analytical_baseline(LEADER, &style.sql)
        .await
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

    // Only a statement with an output sort carries physical evidence. A style
    // that claims a sort and produced no evidence is the failure.
    match (&evidence.physical, style.ordered) {
        (Some(_), true) => {}
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
    let exchanged = cluster.metric_totals(&EXCHANGE_COUNTERS)?;
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
        if cluster.graph_leases(index)?.0 > leases_before[index] {
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
        let ownership = cluster.ownership_snapshot(index)?;
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
/// listener. It must see none of it, which is tenant isolation stated as a
/// read rather than as a configuration.
///
/// # Errors
///
/// Returns the first claim that broke: the OTLP export, the publication, the
/// readback, or the foreign tenant reaching another tenant's span.
async fn prove_canonical_genai_span(cluster: &mut PeerCluster) -> Result<(), PeerJourneyError> {
    use wyrd_testing::bifrost::canonical_signals as fixture;

    let api_key = cluster
        .provision_public_api_key("canonical-signal-caller")
        .await?;
    let scope = format!("wyrd.peer.canonical.{}", uuid::Uuid::now_v7().simple());
    export_canonical_trace(cluster.server(SCRIBE)?, &api_key, &scope).await?;
    cluster.flush(SCRIBE).await?;
    for index in [LEADER, FOLLOWERS[0], FOLLOWERS[1], SCRIBE] {
        cluster.refresh_snapshot(index).await?;
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
    let leader = public_client(cluster.server(LEADER)?, &api_key)?;
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

    // Isolation: another tenant on the same listener reaches none of it.
    let foreign_key = cluster
        .provision_foreign_public_api_key("canonical-foreign")
        .await?;
    let foreign = public_client(cluster.server(LEADER)?, &foreign_key)?;
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
    node: &wyrd_testing::WyrdTestServer,
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
        .post(format!(
            "{}/v1/traces",
            node.base_url()
                .ok_or("the pod serves no public HTTP listener")?
        ))
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
        .query(&BifrostQueryRequest {
            params: Vec::new(),
            sql: sql.to_owned(),
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

/// Oracle pods of the remote live-read journey.
const LIVE_ORACLES: [usize; 3] = [0, 1, 2];

/// Deadline of the live reads that must outlive their pause.
const LIVE_OPEN_DEADLINE_MS: i64 = 120_000;

/// Deadline of the live read the leader must end while Scribe is paused.
const LIVE_SHORT_DEADLINE_MS: i64 = 5_000;

/// Stable code of a query the leader ended at its deadline.
const QUERY_TIMEOUT_CODE: &str = "WYRD_VALA_504_QUERY_TIMEOUT";

/// Bound on waiting for a remote live read to release every hold.
const LIVE_RELEASE_POLLS: usize = 300;

/// A live read served by a Scribe on another process is owned by its query.
///
/// Rows are left live on the Scribe pod, so the only source is a remote live
/// fragment over the peer plane. With that Scribe's producer paused after its
/// first batch, three endings are driven in turn: the client drops its
/// stream, the leader's deadline expires, and the Scribe process dies after
/// rows reached the client. Each ending must release the remote producer, its
/// follower pool bytes, and every Oracle admission; the deadline ending carries the
/// typed timeout, and the lost Scribe yields one failed terminal, never a
/// successful partial result.
///
/// # Panics
///
/// Panics when any ending leaves a hold behind or reports the wrong terminal.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn remote_live_scribe_drop_releases_query() {
    prove_remote_live_scribe_release()
        .await
        .expect("remote live Scribe journey");
}

/// Drives the three remote live-read endings over one process topology.
///
/// # Errors
///
/// Returns the first ending whose release or terminal broke the contract.
async fn prove_remote_live_scribe_release() -> Result<(), PeerJourneyError> {
    let mut cluster = PeerCluster::start(&[
        BifrostTarget::Oracle,
        BifrostTarget::Oracle,
        BifrostTarget::Oracle,
        BifrostTarget::Scribe,
    ])
    .await?;
    let api_key = cluster
        .provision_public_api_key("remote-live-reader")
        .await?;
    let table = format!("remote_live_{}", uuid::Uuid::now_v7().simple());
    cluster.register_table(SCRIBE, &table).await?;
    // One row per ingest, so the paused producer has a later batch to withhold.
    for id in 1..=3 {
        cluster.ingest_live_rows(SCRIBE, &table, id, 1, 1).await?;
    }
    for index in [LEADER, FOLLOWERS[0], FOLLOWERS[1], SCRIBE] {
        cluster.refresh_snapshot(index).await?;
    }
    let client = public_client(cluster.server(LEADER)?, &api_key)?;
    let query = wyrd_client::Bifrost::query_only(&client);
    let sql = format!("SELECT id FROM vala.bifrost.{table}");

    let case = "client dropped";
    let window = cluster.telemetry().checkpoint()?;
    let stream = open_paused_remote_live(&mut cluster, &query, &sql, LIVE_OPEN_DEADLINE_MS).await?;
    await_remote_live_held(&mut cluster, case).await?;
    drop(stream);
    await_remote_live_released(&mut cluster, case).await?;
    cluster.release_live_production_pause();
    assert_remote_query_telemetry(&cluster, &window, "client_drop", "cancelled").await?;

    let case = "leader deadline";
    let mut stream =
        open_paused_remote_live(&mut cluster, &query, &sql, LIVE_SHORT_DEADLINE_MS).await?;
    await_remote_live_held(&mut cluster, case).await?;
    let failure = loop {
        match stream.next_batch().await {
            Ok(Some(_)) => {}
            Ok(None) => break None,
            Err(error) => break Some(error),
        }
    };
    let failure =
        failure.ok_or_else(|| format!("{case}: the read ended {:?}", stream.terminal()))?;
    let code = wyrd_spec::error::WyrdError::from(&failure).code();
    if code != QUERY_TIMEOUT_CODE {
        return Err(format!(
            "{case}: the read failed with {code}: {failure}; terminal {:?}",
            stream.terminal()
        )
        .into());
    }
    await_remote_live_released(&mut cluster, case).await?;
    cluster.release_live_production_pause();

    let case = "remote Scribe lost";
    let window = cluster.telemetry().checkpoint()?;
    let mut stream =
        open_paused_remote_live(&mut cluster, &query, &sql, LIVE_OPEN_DEADLINE_MS).await?;
    await_remote_live_held(&mut cluster, case).await?;
    let first = tokio::time::timeout(CLEAN_LEASE_INTERVAL * 100, stream.next_batch())
        .await
        .map_err(|_| format!("{case}: the first live batch never reached the client"))?
        .map_err(|error| format!("{case}: the first live batch failed: {error}"))?
        .ok_or_else(|| format!("{case}: the read ended before its first batch"))?;
    let mut rows = first.num_rows();
    cluster.kill(SCRIBE).await?;
    let failure = loop {
        match stream.next_batch().await {
            Ok(Some(batch)) => rows += batch.num_rows(),
            Ok(None) => break None,
            Err(error) => break Some(error),
        }
    };
    if failure.is_none() || rows >= 3 {
        return Err(format!(
            "{case}: a lost Scribe must end the read failed after {rows} rows, ended {:?}",
            stream.terminal()
        )
        .into());
    }
    await_oracles_released(&mut cluster, case).await?;
    assert_remote_query_telemetry(&cluster, &window, "failed", "failed").await?;
    cluster.shutdown().await?;
    Ok(())
}

/// Proves one remote live read's production metrics and trace match its ending.
///
/// The window holds exactly one public query. The Gate request opened
/// successfully whatever happened later, the Gate stream and the Oracle
/// execution each record exactly one terminal with the expected outcome and
/// never Success, and the leader's remote-fragment span is causal child work
/// in the same trace as the query and the client-facing stream, ending inside
/// the stream's lifetime with the remote ending it observed.
///
/// # Errors
///
/// Returns a telemetry error, or an error naming the span that never closed.
///
/// # Panics
///
/// Panics when an emitted fact disagrees with the ending the client observed.
#[expect(
    clippy::float_cmp,
    reason = "Prometheus renders these metrics as whole numbers, so f64 equality is exact"
)]
async fn assert_remote_query_telemetry(
    cluster: &PeerCluster,
    window: &BifrostTelemetryCheckpoint,
    oracle_outcome: &str,
    gate_outcome: &str,
) -> Result<(), PeerJourneyError> {
    let telemetry = cluster.telemetry();
    wait_for_spans(
        telemetry,
        window,
        &[
            "bifrost.gate.query.stream",
            "bifrost.oracle.stream",
            "bifrost.oracle.peer.fragment",
        ],
    )
    .await?;
    let delta = telemetry.delta_since(window)?;
    let counter = |family: &str, labels: &[(&str, &str)]| {
        metric_value(&delta, family, BifrostMetricKind::Counter, labels)
    };
    assert_eq!(
        counter(
            "bifrost_gate_requests_total",
            &[("operation", "query"), ("outcome", "success")]
        ),
        1.0,
        "{oracle_outcome}: the stream opened as one successful Gate request"
    );
    assert_eq!(
        counter("bifrost_gate_query_streams_total", &[]),
        1.0,
        "{oracle_outcome}: one stream has one Gate terminal"
    );
    assert_eq!(
        counter(
            "bifrost_gate_query_streams_total",
            &[("outcome", gate_outcome)]
        ),
        1.0,
        "{oracle_outcome}: the Gate terminal is {gate_outcome}"
    );
    assert_eq!(
        metric_value(
            &delta,
            "oracle_query_duration_seconds",
            BifrostMetricKind::HistogramCount,
            &[]
        ),
        1.0,
        "{oracle_outcome}: one query has one Oracle duration observation"
    );
    assert_eq!(
        metric_value(
            &delta,
            "oracle_query_duration_seconds",
            BifrostMetricKind::HistogramCount,
            &[("outcome", oracle_outcome)]
        ),
        1.0,
        "{oracle_outcome}: the Oracle execution ended {oracle_outcome}"
    );
    assert!(
        metric_value(
            &delta,
            "oracle_query_phase_seconds",
            BifrostMetricKind::HistogramCount,
            &[("phase", "peer_open")]
        ) >= 1.0,
        "{oracle_outcome}: the remote live read opened a peer fragment"
    );
    if oracle_outcome == "client_drop" {
        assert_eq!(
            counter(
                "oracle_query_cancellations_total",
                &[("reason", "client_drop")]
            ),
            1.0,
            "a dropped client stream is one client-drop cancellation"
        );
    }

    let named = |name: &str| -> Vec<&CapturedSpan> {
        delta
            .spans
            .iter()
            .filter(|span| span.name == name)
            .collect()
    };
    let gate_streams = named("bifrost.gate.query.stream");
    let [gate_stream] = gate_streams.as_slice() else {
        panic!("{oracle_outcome}: one Gate stream span, saw {gate_streams:?}");
    };
    assert_eq!(
        gate_stream.attributes.get("outcome").map(String::as_str),
        Some(gate_outcome)
    );
    let oracle_streams = named("bifrost.oracle.stream");
    let [oracle_stream] = oracle_streams.as_slice() else {
        panic!("{oracle_outcome}: one Oracle stream span, saw {oracle_streams:?}");
    };
    assert_eq!(oracle_stream.trace_id, gate_stream.trace_id);
    assert_eq!(
        oracle_stream.attributes.get("outcome").map(String::as_str),
        Some(oracle_outcome)
    );
    let fragments = named("bifrost.oracle.peer.fragment");
    assert!(
        !fragments.is_empty()
            && fragments.iter().all(|fragment| {
                fragment.trace_id == oracle_stream.trace_id
                    && fragment.duration_nanos <= gate_stream.duration_nanos
                    && fragment.attributes.get("role").map(String::as_str) == Some("scribe")
            }),
        "{oracle_outcome}: every remote Scribe fragment is child work of the query, saw \
         {fragments:?}"
    );
    if oracle_outcome == "failed" {
        assert!(
            fragments.iter().any(|fragment| {
                fragment.attributes.get("outcome").map(String::as_str) == Some("failed")
            }),
            "the lost Scribe's fragment records its failure, saw {fragments:?}"
        );
    }
    Ok(())
}

/// Rows written to the Scribe pod so a remote live fragment outgrows its window.
///
/// Two million narrow rows encode to tens of MiB, far beyond the HTTP/2 window
/// and the leader's bounded buffering between an undrained client and the peer.
const WINDOW_ROWS: i64 = 2_000_000;

/// Graceful shutdown ends a remote live fragment its reader stopped draining.
///
/// The public client opens a live read on the leader and never reads it, so the
/// leader stops pulling the remote fragment and the Scribe's response blocks on
/// HTTP/2 window credit rather than in the response body. Stopping that Scribe
/// pod gracefully must still end the fragment's connection: the pod's drain
/// completes cleanly within its own budget, the remote producer is released,
/// the leader's read fails, and every Oracle admission is released — without
/// the client resuming reads or the read's deadline expiring.
///
/// # Panics
///
/// Panics when the stop misses its drain, a hold survives, or the read succeeds.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn remote_live_window_blocked_scribe_stops_cleanly() {
    prove_window_blocked_scribe_stop()
        .await
        .expect("window-blocked remote live Scribe stop");
}

/// Blocks one remote live fragment on window credit, then stops its Scribe.
///
/// The fragment is blocked once the Scribe holds its producer and that
/// producer yields no batch across one lease interval while the client reads
/// nothing.
///
/// # Errors
///
/// Returns the first step whose stop, release, or terminal broke the contract.
async fn prove_window_blocked_scribe_stop() -> Result<(), PeerJourneyError> {
    let case = "window-blocked Scribe stopped";
    let mut cluster = PeerCluster::start(&[
        BifrostTarget::Oracle,
        BifrostTarget::Oracle,
        BifrostTarget::Oracle,
        BifrostTarget::Scribe,
    ])
    .await?;
    let api_key = cluster.provision_public_api_key("window-reader").await?;
    let table = format!("window_live_{}", uuid::Uuid::now_v7().simple());
    cluster.register_table(SCRIBE, &table).await?;
    for start in
        (0..WINDOW_ROWS).step_by(usize::try_from(INGEST_CHUNK).expect("ingest chunk fits usize"))
    {
        cluster
            .ingest_live_rows(SCRIBE, &table, start, INGEST_CHUNK, INGEST_GROUPS)
            .await?;
    }
    for index in [LEADER, FOLLOWERS[0], FOLLOWERS[1], SCRIBE] {
        cluster.refresh_snapshot(index).await?;
    }
    let client = public_client(cluster.server(LEADER)?, &api_key)?;
    let query = wyrd_client::Bifrost::query_only(&client);
    let mut stream = query
        .query(&BifrostQueryRequest {
            params: Vec::new(),
            sql: format!("SELECT id, filter_key FROM vala.bifrost.{table}"),
            deadline_ms: Some(LIVE_OPEN_DEADLINE_MS),
        })
        .await
        .map_err(|error| PeerJourneyError::from(error.to_string()))?;

    let mut last = (
        0,
        vala_bifrost_redux::scribe::tail_rpc::live_batches_produced_for_test(),
    );
    let mut blocked = false;
    for _ in 0..LIVE_RELEASE_POLLS {
        tokio::time::sleep(CLEAN_LEASE_INTERVAL).await;
        let (producers, _) = cluster.live_scribe_holds(SCRIBE)?;
        let now = (
            producers,
            vala_bifrost_redux::scribe::tail_rpc::live_batches_produced_for_test(),
        );
        if producers == 1 && now.1 > 0 && now == last {
            blocked = true;
            break;
        }
        last = now;
    }
    if !blocked {
        return Err(format!(
            "{case}: the remote fragment never blocked on window credit, last \
             (producers, batches) {last:?}"
        )
        .into());
    }

    cluster.stop(SCRIBE).await?;
    let producers = vala_bifrost_redux::scribe::tail_rpc::open_live_producers_for_test();
    if producers != 0 {
        return Err(format!("{case}: the stopped Scribe still holds {producers} producers").into());
    }
    let failure = loop {
        match stream.next_batch().await {
            Ok(Some(_)) => {}
            Ok(None) => break None,
            Err(error) => break Some(error),
        }
    };
    if failure.is_none() {
        return Err(format!(
            "{case}: a stopped Scribe must fail the read, ended {:?}",
            stream.terminal()
        )
        .into());
    }
    await_oracles_released(&mut cluster, case).await?;
    cluster.shutdown().await?;
    Ok(())
}

/// Arms the Scribe pause, then opens one public live read on the leader.
///
/// # Errors
///
/// Returns the Scribe control failure or the query's open refusal.
async fn open_paused_remote_live(
    cluster: &mut PeerCluster,
    query: &wyrd_client::Bifrost,
    sql: &str,
    deadline_ms: i64,
) -> Result<wyrd_client::bifrost::QueryResultStream, PeerJourneyError> {
    cluster.arm_live_production_pause();
    query
        .query(&BifrostQueryRequest {
            params: Vec::new(),
            sql: sql.to_owned(),
            deadline_ms: Some(deadline_ms),
        })
        .await
        .map_err(|error| PeerJourneyError::from(error.to_string()))
}

/// Proves the paused remote read is held open by the leader's admitted query.
///
/// The remote Scribe holds exactly one paused producer with its snapshot, and
/// the leader still admits the query that dispatched it. The follower takes no
/// lease of its own: the leader stream owns its lifetime.
///
/// # Errors
///
/// Returns an error naming `case` when the producer never paused, the Scribe
/// holds other than one producer, or the leader no longer admits the query.
async fn await_remote_live_held(
    cluster: &mut PeerCluster,
    case: &str,
) -> Result<(), PeerJourneyError> {
    cluster.await_live_production_paused().await?;
    let (producers, _) = cluster.live_scribe_holds(SCRIBE)?;
    let admitted = cluster.ownership_snapshot(LEADER)?.active_queries;
    if producers != 1 || admitted == 0 {
        return Err(format!(
            "{case}: the paused remote read must hold one producer under the leader's \
             admitted query, held {producers} producers and {admitted} leader queries"
        )
        .into());
    }
    Ok(())
}

/// Waits until the remote Scribe and every Oracle hold nothing for the read.
///
/// # Errors
///
/// Returns an error naming `case` with the last holds when release never lands.
async fn await_remote_live_released(
    cluster: &mut PeerCluster,
    case: &str,
) -> Result<(), PeerJourneyError> {
    let mut last = (0, 0);
    for _ in 0..LIVE_RELEASE_POLLS {
        last = cluster.live_scribe_holds(SCRIBE)?;
        if last == (0, 0) {
            return await_oracles_released(cluster, case).await;
        }
        tokio::time::sleep(CLEAN_LEASE_INTERVAL).await;
    }
    Err(format!("{case}: the remote Scribe still holds (producers, bytes) {last:?}").into())
}

/// Waits until no Oracle pod admits a query or reserves query memory.
///
/// # Errors
///
/// Returns an error naming `case` and the pod that still holds admission.
async fn await_oracles_released(
    cluster: &mut PeerCluster,
    case: &str,
) -> Result<(), PeerJourneyError> {
    for index in LIVE_ORACLES {
        let mut held = (0, 0);
        for _ in 0..LIVE_RELEASE_POLLS {
            let snapshot = cluster.ownership_snapshot(index)?;
            held = (snapshot.active_queries, snapshot.reserved_memory_bytes);
            if held == (0, 0) {
                break;
            }
            tokio::time::sleep(CLEAN_LEASE_INTERVAL).await;
        }
        if held != (0, 0) {
            return Err(
                format!("{case}: Oracle {index} still holds (queries, bytes) {held:?}").into(),
            );
        }
    }
    Ok(())
}

/// Indices of the two pods that each lead one Analytical graph.
const RETRY_LEADERS: [usize; 2] = [0, 1];

/// Index of the pod that publishes the data both leaders read.
const RETRY_SCRIBE: usize = 2;

/// Index of the one Oracle both leaders' graphs must be placed on.
const RETRY_RECEIVER: usize = 3;

/// Oracle slot units each leader admits with.
///
/// Room for its own graph envelope, the other leader's follower envelope, and
/// the protected Interactive quantum, so neither leader is the node that runs
/// out of capacity.
const RETRY_LEADER_SLOTS: usize = 8;

/// Oracle slot units the receiving node admits with.
///
/// One Analytical envelope charges one unit and the root keeps one
/// Interactive quantum, so exactly one graph fits and the second leader's
/// reservation must be refused before it is accepted.
const RETRY_RECEIVER_SLOTS: usize = 2;

/// Observations of a saturated receiver while the second leader retries.
///
/// At [`CLEAN_LEASE_INTERVAL`] this spans three seconds, longer than two of
/// the receiver's one-second retry hints, so the second leader has been
/// refused and has retried at least once inside the window.
const RETRY_OBSERVATION_POLLS: usize = 30;

/// Two leaders target one Oracle whose running slots never exceed its limit,
/// and the refused leader retries within its original deadline.
///
/// # Panics
///
/// Panics when the retry journey cannot be driven to its claims.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn two_leaders_retry_preaccept_capacity() {
    prove_two_leaders_retry_preaccept_capacity()
        .await
        .expect("two-leader pre-accept retry journey");
}

/// Drives two leaders' graphs onto one receiving Oracle that fits only one.
///
/// The first leader's graph is held at the receiver's follower `ExecuteTask`
/// boundary, so the receiver's only Analytical envelope is running. The second
/// leader's graph is then admitted locally and must place on the receiver. The
/// receiver refuses it before accepting any work and never waits itself; the
/// second leader releases its round, waits the refusal's hint, and retries.
/// While the hold lasts the receiver must never run more than its limit, and
/// once it returns the slot the second graph must complete inside the
/// statement's original deadline. Every rejected round is released before
/// activation, so each follower activates exactly one lease per query.
///
/// # Errors
///
/// Returns the first claim that broke.
async fn prove_two_leaders_retry_preaccept_capacity() -> Result<(), PeerJourneyError> {
    let mut cluster = PeerCluster::start_with_slots(&[
        (BifrostTarget::Oracle, Some(RETRY_LEADER_SLOTS)),
        (BifrostTarget::Oracle, Some(RETRY_LEADER_SLOTS)),
        (BifrostTarget::Scribe, None),
        (BifrostTarget::Oracle, Some(RETRY_RECEIVER_SLOTS)),
    ])
    .await?;
    let receiver = cluster.node_id(RETRY_RECEIVER).as_uuid();
    await_receiver_membership(&cluster, receiver).await?;

    let table = format!("preaccept_retry_{}", uuid::Uuid::now_v7().simple());
    cluster.register_table(RETRY_SCRIBE, &table).await?;
    cluster.ingest_rows(RETRY_SCRIBE, &table, 0, 12, 3).await?;
    cluster.ingest_rows(RETRY_SCRIBE, &table, 0, 12, 3).await?;
    for index in [
        RETRY_LEADERS[0],
        RETRY_LEADERS[1],
        RETRY_SCRIBE,
        RETRY_RECEIVER,
    ] {
        cluster.refresh_snapshot(index).await?;
    }
    let baseline = cluster.ownership_snapshot(RETRY_RECEIVER)?;
    if baseline.root_active_queries != 0 {
        return Err(format!("the receiver must start idle, held {baseline:?}").into());
    }

    let sql = format!(
        "SELECT filter_key, COUNT(*) AS matched FROM vala.bifrost.{table} \
         GROUP BY filter_key ORDER BY filter_key"
    );
    // The first graph occupies the receiver's only Analytical envelope and is
    // held there with its lease active.
    cluster.arm_execute_pause(RETRY_RECEIVER)?;
    cluster.start_sql(RETRY_LEADERS[0], &sql)?;
    cluster.await_execute_paused(RETRY_RECEIVER).await?;
    let held = cluster.ownership_snapshot(RETRY_RECEIVER)?;
    if held.root_analytical_queries != 1 || held.follower_graphs != 1 {
        return Err(format!("the receiver must run exactly the first graph, held {held:?}").into());
    }

    // The second graph is admitted by its own leader, so the only thing it can
    // be waiting on is placement on the saturated receiver.
    cluster.start_sql(RETRY_LEADERS[1], &sql)?;
    await_leader_placing(&mut cluster, RETRY_LEADERS[1]).await?;
    let receiver_slots = u32::try_from(RETRY_RECEIVER_SLOTS)?;
    for _ in 0..RETRY_OBSERVATION_POLLS {
        let receiving = cluster.ownership_snapshot(RETRY_RECEIVER)?;
        if receiving.root_active_queries > receiver_slots
            || receiving.root_analytical_queries > 1
            || receiving.follower_graphs > 1
        {
            return Err(format!(
                "the receiver ran beyond its {RETRY_RECEIVER_SLOTS} slots: {receiving:?}"
            )
            .into());
        }
        let placing = cluster.ownership_snapshot(RETRY_LEADERS[1])?;
        if placing.leader_graphs != placing.follower_graphs + 1 || placing.active_queries != 1 {
            return Err(format!(
                "the refused leader must keep retrying its admitted graph, held {placing:?}"
            )
            .into());
        }
        tokio::time::sleep(CLEAN_LEASE_INTERVAL).await;
    }

    // Returning the slot is what lets the refused leader's next retry land,
    // well inside the statement's original deadline.
    cluster.release_execute_pause(RETRY_RECEIVER)?;
    for leader in RETRY_LEADERS {
        match cluster.await_sql(leader).await? {
            Ok(3) => {}
            Ok(rows) => {
                return Err(format!("leader {leader} returned {rows} groups, not 3").into());
            }
            Err(detail) => {
                return Err(format!("leader {leader} did not complete: {detail}").into());
            }
        }
    }

    // A refused round is released before activation, so each follower
    // activated exactly one lease per query that it actually ran.
    for (index, expected) in [
        (RETRY_LEADERS[0], 1),
        (RETRY_LEADERS[1], 1),
        (RETRY_RECEIVER, 2),
    ] {
        let (activated, live) = await_released_lease(&mut cluster, index).await?;
        if (activated, live) != (expected, 0) {
            return Err(format!(
                "node {index} must activate and release {expected} leases, reported \
                 ({activated}, {live})"
            )
            .into());
        }
    }
    for index in [RETRY_LEADERS[0], RETRY_LEADERS[1], RETRY_RECEIVER] {
        await_root_released(&mut cluster, index).await?;
    }

    cluster.shutdown().await?;
    Ok(())
}

/// Waits until both leaders observe the receiver as a ready Oracle.
///
/// Membership is heartbeat-driven, so the joined receiver appears in each
/// leader's cut within a heartbeat rather than at once.
///
/// # Errors
///
/// Returns the leader that never observed the receiver.
async fn await_receiver_membership(
    cluster: &PeerCluster,
    receiver: uuid::Uuid,
) -> Result<(), PeerJourneyError> {
    for leader in RETRY_LEADERS {
        let mut observed = false;
        for _ in 0..CLEAN_LEASE_POLLS {
            observed =
                cluster.membership(leader).await?.iter().any(|entry| {
                    entry.node_id == receiver && entry.role == "oracle" && entry.ready
                });
            if observed {
                break;
            }
            tokio::time::sleep(CLEAN_LEASE_INTERVAL).await;
        }
        if !observed {
            return Err(format!("leader {leader} never observed the receiving Oracle").into());
        }
    }
    Ok(())
}

/// Waits until `leader` has admitted its query and registered its graph.
///
/// Registration precedes participant placement, so from here the graph is
/// either placing or retrying placement. The node's supervisor also registers
/// every graph it follows, so the one graph it leads is the count beyond its
/// follower graphs.
///
/// # Errors
///
/// Returns the leader's last ownership when it never registered the graph.
async fn await_leader_placing(
    cluster: &mut PeerCluster,
    leader: usize,
) -> Result<(), PeerJourneyError> {
    let mut last = cluster.ownership_snapshot(leader)?;
    for _ in 0..CLEAN_LEASE_POLLS {
        if last.leader_graphs == last.follower_graphs + 1 && last.active_queries == 1 {
            return Ok(());
        }
        tokio::time::sleep(CLEAN_LEASE_INTERVAL).await;
        last = cluster.ownership_snapshot(leader)?;
    }
    Err(format!("leader {leader} never registered its graph, held {last:?}").into())
}

/// Waits until `index` retains no Oracle query slot units at its root.
///
/// # Errors
///
/// Returns the node's last ownership when its slots never return.
async fn await_root_released(
    cluster: &mut PeerCluster,
    index: usize,
) -> Result<(), PeerJourneyError> {
    let mut last = cluster.ownership_snapshot(index)?;
    for _ in 0..CLEAN_LEASE_POLLS {
        if last.root_active_queries == 0 && last.peer_running == 0 {
            return Ok(());
        }
        tokio::time::sleep(CLEAN_LEASE_INTERVAL).await;
        last = cluster.ownership_snapshot(index)?;
    }
    Err(format!("node {index} never returned its slots, held {last:?}").into())
}
