//! Oracle journeys — Distributed follower dispatch: signed selective closures
//! and the physical pruning they buy on the follower's rebuilt leaf.
//!
//! Module of the `oracle` binary; see `main.rs` for the capability it proves
//! and `support.rs` for the fixtures it shares.

use std::time::Duration;

use arrow::array::{Array, Int64Array};
use vala_bifrost_redux::oracle::{
    iceberg_projection_probe, set_live_fragment_batch_bound_for_test,
};
use wyrd_client::WyrdClient;
use wyrd_client::bifrost::BifrostClientError;
use wyrd_server::oracle::{
    ScribeFragmentFault, arm_scribe_fragment_fault_for_test,
    arm_tail_listing_ticket_rejection_for_test,
};
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{
    BifrostQueryRequest, QueryExecutionPath, QueryTerminalErrorCode, QueryTerminalOutcome,
    QueryWarning,
};
use wyrd_spec::vala::error::BifrostError;
use wyrd_testing::bifrost::process_cluster::{BifrostProcessCluster, ProcessNodeTarget};
use wyrd_testing::bifrost::{BifrostClusterSpec, WyrdTestCluster};

use crate::support::*;

/// Bound on polls waiting for the Oracle graph to release every reservation.
///
/// Terminal delivery precedes asynchronous graph settlement, so the journey
/// observes the zero-ownership invariant across this bound instead of sampling
/// it once at the terminal frame.
const SETTLEMENT_POLLS: usize = 300;

/// Interval between polls for a settled Oracle graph.
const SETTLEMENT_INTERVAL: Duration = Duration::from_millis(100);

/// Which physical pruning signal a topology's follower cut can actually move.
enum PruningExpectation {
    /// One node plans and scans locally: file-level or row-group-level
    /// exclusion both count, because the leaf's file set is the leader's.
    FilesOrRowGroups,
    /// A follower rebuilds its own leaf and `with_assigned_files` pins its file
    /// set, so row-group exclusion under the signed closure is the only signal
    /// that can move. This is the assertion that regresses if
    /// `OracleCatalogResolver::resolve` stops passing `assignment.predicates`.
    RowGroupsOnly,
}

/// A selective predicate and a narrow projection each prune physical local and
/// distributed Oracle reads while preserving exact residual rows, and the
/// tenant tripwire still fails closed once both are in effect.
///
/// This is the hot-Parquet owner of the projection half. The cut is asserted
/// to hold hot files and no compacted file immediately before the queries run,
/// so the broad/narrow byte difference is attributable to `HotParquetExec`'s
/// column mask and to nothing else. Predicate, retained rows, retained row
/// groups, and physical authority are identical across the two queries; only
/// the requested column set differs.
///
/// The two legs prove different halves. The `one_mixed` leg plans and scans on
/// one node, so either physical granularity may move and the assertion accepts
/// whichever the fixture's layout produced. The `three_mixed` leg dispatches to
/// a follower whose leaf file set is pinned by `with_assigned_files`, so file
/// counts cannot move and row-group exclusion under the signed
/// `assignment.predicates` closure is the only available signal — that leg
/// requires it positively.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_selective_predicate_and_projection_prune_distributed_reads() {
    prove_selective_predicate_pruning(
        BifrostClusterSpec::one_mixed(),
        0,
        PruningExpectation::FilesOrRowGroups,
    )
    .await
    .expect("local pruning journey");
    prove_selective_predicate_pruning(
        BifrostClusterSpec::three_mixed(),
        2,
        PruningExpectation::RowGroupsOnly,
    )
    .await
    .expect("distributed pruning journey");
}

/// Drives one topology through a three-file selective-predicate fixture,
/// proving strictly fewer scanned bytes than an unfiltered scan, the pruning
/// signal `expectation` names, identical residual-filtered rows, and a
/// fail-closed tenant tripwire.
///
/// # Errors
///
/// Returns a client, telemetry, or cluster-lifecycle error surfaced by any
/// journey step.
async fn prove_selective_predicate_pruning(
    spec: BifrostClusterSpec,
    query_index: usize,
    expectation: PruningExpectation,
) -> Result<(), JourneyError> {
    let cluster = WyrdTestCluster::start_spec(spec).await?;
    let ingest_server = cluster
        .servers()
        .find(|server| server.bifrost_scribe().is_some())
        .ok_or("missing ingest node")?;
    let tenant = cluster.data_tenant_id();
    let table = unique_table("oracle_predicate");
    register_table(ingest_server, tenant, &table).await?;
    let rows = writer(ingest_server, "predicate-writer").await?;
    for (id, value) in [(1_i64, "alpha"), (2_i64, "target"), (3_i64, "zulu")] {
        rows.write(
            &format!("vala.bifrost.{table}"),
            &journey_schema(),
            [journey_row(id, value)],
        )
        .await?;
        ingest_server.flush_bifrost().await?;
    }
    cluster.refresh_oracle_snapshots().await?;

    let query_server = cluster.server(query_index).ok_or("missing query node")?;
    let reader = client(query_server, "predicate-reader").await?;
    let table_fqn = format!("vala.bifrost.{table}");

    // Physical-authority precondition, asserted rather than assumed: the
    // projection proof below is only a hot-Parquet proof if every file in the
    // cut is still hot. A maintenance sweep that compacted the fixture would
    // otherwise silently move the measurement onto the Iceberg leaf.
    let (compacted_files, hot_files) = file_tier_counts(&cluster, tenant, &table).await?;
    if hot_files == 0 || compacted_files != 0 {
        return Err(format!(
            "hot-only projection proof requires a hot-only cut: \
             hot={hot_files} compacted={compacted_files}"
        )
        .into());
    }

    let unfiltered_checkpoint = cluster
        .telemetry()
        .checkpoint()
        .map_err(|error| error.to_string())?;
    let unfiltered_rows = query_rows(&reader, &table).await?;
    if unfiltered_rows != 3 {
        return Err(format!("unfiltered baseline expected 3 rows, saw {unfiltered_rows}").into());
    }
    let unfiltered_delta = cluster
        .telemetry()
        .delta_since(&unfiltered_checkpoint)
        .map_err(|error| error.to_string())?;
    let unfiltered_files = sum_metric(&unfiltered_delta, "oracle_query_files_scanned_total");
    let unfiltered_bytes = sum_metric(&unfiltered_delta, "oracle_query_bytes_scanned_total");
    let unfiltered_row_groups =
        sum_metric(&unfiltered_delta, "oracle_query_row_groups_scanned_total");
    let unfiltered_pruned = sum_metric(&unfiltered_delta, "oracle_query_row_groups_pruned_total");
    if unfiltered_files < 3.0 {
        return Err(format!(
            "unfiltered scan expected at least 3 scanned files, saw {unfiltered_files}"
        )
        .into());
    }

    let selective_checkpoint = cluster
        .telemetry()
        .checkpoint()
        .map_err(|error| error.to_string())?;
    // The broad selective read: same predicate, same cut, every column.
    // `query_ids` refuses a non-success terminal, so a successful return is
    // also the outcome assertion this leg used to make separately.
    let broad_ids = query_ids(
        &reader,
        format!(
            "SELECT id, filter_key, unused_payload FROM {table_fqn} \
             WHERE filter_key = 'target' ORDER BY id"
        ),
    )
    .await?;
    if broad_ids != vec![2_i64] {
        return Err(format!(
            "selective predicate expected exactly the one target row, saw {broad_ids:?}"
        )
        .into());
    }
    let selective_delta = cluster
        .telemetry()
        .delta_since(&selective_checkpoint)
        .map_err(|error| error.to_string())?;
    let selective_files = sum_metric(&selective_delta, "oracle_query_files_scanned_total");
    let selective_bytes = sum_metric(&selective_delta, "oracle_query_bytes_scanned_total");
    let selective_row_groups =
        sum_metric(&selective_delta, "oracle_query_row_groups_scanned_total");
    let selective_pruned = sum_metric(&selective_delta, "oracle_query_row_groups_pruned_total");
    match expectation {
        // One node plans and scans, so pruning is accepted at either physical
        // granularity: whole files dropped by manifest/statistics exclusion, or
        // row groups dropped inside a retained file. Which one moves depends on
        // how the fixture's three published files were laid out, so requiring
        // both would assert a fixture detail rather than the pruning contract.
        PruningExpectation::FilesOrRowGroups => {
            if !(selective_files < unfiltered_files || selective_row_groups < unfiltered_row_groups)
            {
                return Err(format!(
                    "selective query must select strictly fewer files or row groups: \
                     files selective={selective_files} unfiltered={unfiltered_files}; \
                     row groups selective={selective_row_groups} unfiltered={unfiltered_row_groups}; \
                     row groups pruned selective={selective_pruned} unfiltered={unfiltered_pruned}; \
                     bytes selective={selective_bytes} unfiltered={unfiltered_bytes}"
                )
                .into());
            }
        }
        // The follower rebuilds its own leaf and `with_assigned_files`
        // overwrites its file set with the assignment's, so any file-level
        // pruning `provider.scan` performed is discarded and `files_scanned`
        // cannot move. What survives the rebuild is the signed
        // `assignment.predicates` closure reaching `with_filter`, which drives
        // `select_row_groups_for_predicates` and both row-group counters. On a
        // distributed leg the leader's plan holds only remote leaves, so the
        // folded `FollowerScanEvidence` is the sole source of these numbers.
        // Both halves are required, and `pruned` is required positively rather
        // than inferred: `sum_metric` reports an absent family as 0.0, so a
        // missing series must fail here, not pass.
        PruningExpectation::RowGroupsOnly => {
            if selective_row_groups >= unfiltered_row_groups
                || selective_pruned - unfiltered_pruned <= 0.0
            {
                return Err(format!(
                    "distributed selective query must scan strictly fewer row groups and \
                     prune strictly more: \
                     row groups selective={selective_row_groups} unfiltered={unfiltered_row_groups}; \
                     row groups pruned selective={selective_pruned} unfiltered={unfiltered_pruned}; \
                     files selective={selective_files} unfiltered={unfiltered_files}; \
                     bytes selective={selective_bytes} unfiltered={unfiltered_bytes}"
                )
                .into());
            }
        }
    }
    // Strict `Less` rather than a negated `<`: an incomparable (NaN) metric
    // must fail this proof, not silently satisfy it.
    if !matches!(
        selective_bytes.partial_cmp(&unfiltered_bytes),
        Some(std::cmp::Ordering::Less)
    ) {
        return Err(format!(
            "selective query must scan strictly fewer bytes: selective={selective_bytes} unfiltered={unfiltered_bytes}"
        )
        .into());
    }

    // The projection half. The narrow query differs from the broad one above
    // in exactly one respect: it does not request `unused_payload`. Predicate,
    // cut, retained rows, and retained row groups are identical, so a byte
    // difference can only come from the physical reader declining to decode
    // that column. On a hot-only cut that reader is `HotParquetExec`, and its
    // name-derived `ProjectionMask` is the only mechanism that can produce it.
    let narrow_checkpoint = cluster
        .telemetry()
        .checkpoint()
        .map_err(|error| error.to_string())?;
    let narrow_ids = query_ids(
        &reader,
        format!("SELECT id FROM {table_fqn} WHERE filter_key = 'target' ORDER BY id"),
    )
    .await?;
    if narrow_ids != broad_ids {
        return Err(format!(
            "narrow projection must return the same target rows: narrow={narrow_ids:?} broad={broad_ids:?}"
        )
        .into());
    }
    let narrow_delta = cluster
        .telemetry()
        .delta_since(&narrow_checkpoint)
        .map_err(|error| error.to_string())?;
    let narrow_bytes = sum_metric(&narrow_delta, "oracle_query_bytes_scanned_total");
    let narrow_row_groups = sum_metric(&narrow_delta, "oracle_query_row_groups_scanned_total");
    // Strict `Less`, so an incomparable (NaN) metric fails rather than passes.
    if !matches!(
        narrow_bytes.partial_cmp(&selective_bytes),
        Some(std::cmp::Ordering::Less)
    ) {
        return Err(format!(
            "narrow projection must scan strictly fewer bytes than the broad query \
             over the same predicate and cut: narrow={narrow_bytes} broad={selective_bytes}; \
             row groups narrow={narrow_row_groups} broad={selective_row_groups}"
        )
        .into());
    }
    // Follower scan evidence on the distributed leg: a remote leaf that
    // reported nothing would leave both byte counters at zero, which the
    // strict comparison above would accept as "not less".
    if narrow_bytes <= 0.0 {
        return Err(format!(
            "narrow projection reported no scanned bytes at all: narrow={narrow_bytes}"
        )
        .into());
    }

    // Tripwire: a physically scanned foreign-tenant row must refuse the
    // query with the tenant-isolation reason intact and deliver no rows.
    //
    // Asserted through `query_terminal_either_surface` because the refusal may
    // land on the pre-byte lookahead (early typed error) or after the first
    // batch (terminal frame) depending on fixture layout.
    // `QueryTenantInvariant` specifically — not merely "some failure" — is the
    // assertion that regresses if the closed predicate/projection path ever
    // loses the reason across the follower dispatch boundary.
    let foreign_tenant = cluster.add_tenant("oracle-predicate-foreign").await?;
    seed_foreign_hot_row(
        &cluster,
        tenant,
        &table,
        foreign_tenant,
        "predicate-foreign",
        ingest_server.node_id().as_uuid(),
    )
    .await?;
    let (tripwire_rows, tripwire_outcome, tripwire_error) = query_terminal_either_surface(
        &reader,
        format!("SELECT count(*) AS total FROM {table_fqn}"),
    )
    .await?;
    if tripwire_rows != 0 {
        return Err(format!("foreign row reached a SQL operator under predicate pushdown: rows={tripwire_rows} outcome={tripwire_outcome:?} error={tripwire_error:?}").into());
    }
    if tripwire_outcome != QueryTerminalOutcome::Failed
        || tripwire_error != Some(QueryTerminalErrorCode::QueryTenantInvariant)
    {
        return Err(format!(
            "tenant tripwire did not fail closed: {tripwire_outcome:?} {tripwire_error:?}"
        )
        .into());
    }

    // A terminal response reaches the caller before the graph settles, so the
    // exact zero-ownership invariant is observed under a bound rather than
    // sampled once. The final nonzero snapshot is reported on timeout.
    let mut inspection = cluster.oracle_inspection().await?;
    for _ in 0..SETTLEMENT_POLLS {
        if inspection.active_queries == 0
            && inspection.queued_queries == 0
            && inspection.reserved_memory_bytes == 0
            && inspection.reserved_spill_bytes == 0
            && inspection.peer_pending == 0
            && inspection.peer_running == 0
        {
            cluster.shutdown().await?;
            return Ok(());
        }
        tokio::time::sleep(SETTLEMENT_INTERVAL).await;
        inspection = cluster.oracle_inspection().await?;
    }
    Err(format!("Oracle runtime did not settle: {inspection:?}").into())
}

/// Drives one query to a terminal outcome across both of Oracle's refusal
/// surfaces.
///
/// Oracle refuses a failure observed on the pre-byte lookahead as an early
/// typed error and never opens a stream, but reports a failure observed after
/// the first batch as an in-band terminal frame. Which surface a given failure
/// lands on depends on how many clean batches precede the offending row, which
/// is a property of the fixture's physical layout rather than of the invariant
/// under test. A journey that asserted only one surface would therefore pin a
/// fixture detail; this normalizes both into the same
/// `(rows, outcome, error code)` triple so the assertion stays on the
/// invariant.
///
/// # Errors
///
/// Returns client, protocol, or Arrow errors that are not a typed Bifrost
/// refusal, and an error when a completed stream carried no terminal frame.
async fn query_terminal_either_surface(
    client: &WyrdClient,
    sql: String,
) -> Result<(u64, QueryTerminalOutcome, Option<QueryTerminalErrorCode>), JourneyError> {
    let opened = wyrd_client::Bifrost::query_only(client)
        .query(&BifrostQueryRequest {
            sql,
            deadline_ms: None,
        })
        .await;
    let mut stream = match opened {
        Ok(stream) => stream,
        Err(BifrostClientError::Transport(WyrdError::Vala { error })) => {
            let code = bifrost_terminal_code(&error)
                .ok_or_else(|| format!("refusal is not a terminal query outcome: {error:?}"))?;
            return Ok((0, QueryTerminalOutcome::Failed, Some(code)));
        }
        Err(other) => return Err(other.into()),
    };
    let mut rows = 0_u64;
    loop {
        match stream.next_batch().await {
            Ok(Some(batch)) => {
                rows = rows.saturating_add(u64::try_from(batch.num_rows())?);
            }
            Ok(None) => break,
            Err(error) if stream.terminal().is_some() => {
                let _ = error;
                break;
            }
            Err(error) => return Err(error.into()),
        }
    }
    let terminal = stream.terminal().ok_or("query terminal missing")?;
    Ok((
        rows,
        terminal.outcome,
        terminal.error.as_ref().map(|error| error.code),
    ))
}

/// Maps the closed terminal Bifrost query errors back to their terminal code.
///
/// Returns `None` for a Bifrost error that is not a terminal query outcome
/// (an admission rejection or invalid SQL, for example), so a caller cannot
/// silently reinterpret an unrelated refusal as a terminal result.
fn bifrost_terminal_code(error: &BifrostError) -> Option<QueryTerminalErrorCode> {
    Some(match error {
        BifrostError::QueryTimeout => QueryTerminalErrorCode::QueryTimeout,
        BifrostError::QueryVisibilityUnavailable => {
            QueryTerminalErrorCode::QueryVisibilityUnavailable
        }
        BifrostError::QueryTenantInvariant => QueryTerminalErrorCode::QueryTenantInvariant,
        BifrostError::QueryReconciliationInvariant => {
            QueryTerminalErrorCode::QueryReconciliationInvariant
        }
        BifrostError::QueryPeerSecurity => QueryTerminalErrorCode::QueryPeerSecurity,
        BifrostError::QueryAuditUnavailable => QueryTerminalErrorCode::QueryAuditUnavailable,
        BifrostError::QueryExecutionFailed => QueryTerminalErrorCode::QueryExecutionFailed,
        _ => return None,
    })
}

/// Number of rows written before compaction. Every one of them is sealed as
/// its own Parquet file, so the Forge pass has enough real inputs to rewrite
/// into a single published data file.
const COMPACTED_BATCH_ROWS: i64 = 20;

/// Number of rows written after compaction. These stay in the hot manifest for
/// the duration of the journey, so the query's cut spans both physical tiers.
///
/// Every one of them matches the predicate, which is what isolates the two
/// leaves' pruning from each other — see [`marker_value`].
const HOT_BATCH_ROWS: i64 = 8;

/// Every fourth row of the compacted batch carries the selective marker.
const MARKER_STRIDE: i64 = 4;

/// Ids in the second batch start here so a returned id names, unambiguously,
/// which physical tier it could only have come from.
const HOT_BATCH_ID_BASE: i64 = 101;

/// One selective distributed query reads a cut that spans both follower read
/// leaves at once — the compacted `:iceberg` leaf and the sealed `:hot` leaf —
/// and prunes physically on both.
///
/// The two leaves are otherwise untestable together. A table that has never
/// compacted records an `:iceberg` assignment with an empty file set, which
/// `OracleCatalogResolver::resolve` short-circuits before it ever calls
/// `provider.scan`, so a fixture that only writes and flushes exercises the hot
/// leaf and nothing else. This journey therefore compacts first and writes
/// second:
///
/// 1. write `COMPACTED_BATCH_ROWS` rows, each sealed as its own file;
/// 2. drive one Forge pass to completion and require the inputs to be marked
///    compacted, which moves them out of the hot manifest and into the Iceberg
///    snapshot;
/// 3. write `HOT_BATCH_ROWS` more rows and leave them sealed-but-uncompacted;
/// 4. query once with the selective predicate.
///
/// Id ranges are disjoint across the two batches, so the returned ids are the
/// proof that both leaves executed: an id below `HOT_BATCH_ID_BASE` exists only
/// inside the compacted data file, and an id at or above it exists only in the
/// hot manifest.
///
/// Pruning is then required to move bytes scanned against an unfiltered
/// baseline of the same cut, and the fixture is shaped so that only the
/// compacted leaf can move it: every hot row matches the predicate, so the hot
/// leaf reads the same bytes either way. Removing `assignment.predicates` from
/// the follower's `provider.scan` call collapses the difference to zero, which
/// is the regression this leg exists to catch.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_selective_predicate_spans_hot_and_compacted_reads() {
    prove_hot_and_compacted_pruning()
        .await
        .expect("hot and compacted pruning journey");
}

/// Builds the two-tier fixture and proves the span-and-prune contract on it.
///
/// # Errors
///
/// Returns a client, Postgres, telemetry, Forge-scheduling, or
/// cluster-lifecycle error surfaced by any journey step, and a descriptive
/// error when the fixture fails to reach the two-tier state the assertion
/// requires.
async fn prove_hot_and_compacted_pruning() -> Result<(), JourneyError> {
    let cluster = WyrdTestCluster::start_spec_with_forge_completion_observer(
        BifrostClusterSpec::three_mixed(),
    )
    .await?;
    let ingest_server = cluster
        .servers()
        .find(|server| server.bifrost_scribe().is_some())
        .ok_or("missing ingest node")?;
    let tenant = cluster.data_tenant_id();
    let table = unique_table("oracle_two_tier");
    register_table(ingest_server, tenant, &table).await?;
    let rows = writer(ingest_server, "two-tier-writer").await?;
    let table_fqn = format!("vala.bifrost.{table}");

    for id in 1..=COMPACTED_BATCH_ROWS {
        rows.write(
            &table_fqn,
            &journey_schema(),
            [journey_row(id, marker_value(id))],
        )
        .await?;
        ingest_server.flush_bifrost().await?;
    }
    compact_sealed_batch(&cluster, tenant, &table, COMPACTED_BATCH_ROWS).await?;
    prove_compacted_only_projection(&cluster, tenant, &table, &table_fqn).await?;

    for offset in 0..HOT_BATCH_ROWS {
        let id = HOT_BATCH_ID_BASE + offset;
        rows.write(
            &table_fqn,
            &journey_schema(),
            [journey_row(id, marker_value(id))],
        )
        .await?;
        ingest_server.flush_bifrost().await?;
    }
    cluster.refresh_oracle_snapshots().await?;

    // Precondition, asserted immediately before the query rather than assumed:
    // the periodic maintenance ticker could in principle sweep the second batch
    // too, and a cut with only one physical tier in it would silently prove
    // half of what this journey claims.
    let (compacted_files, hot_files) = file_tier_counts(&cluster, tenant, &table).await?;
    if compacted_files == 0 || hot_files == 0 {
        return Err(format!(
            "query cut must span both physical tiers: compacted={compacted_files} hot={hot_files}"
        )
        .into());
    }

    let query_server = cluster.server(2).ok_or("missing query node")?;
    let reader = client(query_server, "two-tier-reader").await?;
    let total_rows = COMPACTED_BATCH_ROWS + HOT_BATCH_ROWS;

    let unfiltered_checkpoint = cluster
        .telemetry()
        .checkpoint()
        .map_err(|error| error.to_string())?;
    let unfiltered_ids = query_ids(
        &reader,
        format!("SELECT id, filter_key, unused_payload FROM {table_fqn} ORDER BY id"),
    )
    .await?;
    if i64::try_from(unfiltered_ids.len())? != total_rows {
        return Err(format!(
            "unfiltered baseline expected {total_rows} rows, saw {}",
            unfiltered_ids.len()
        )
        .into());
    }
    let unfiltered_delta = cluster
        .telemetry()
        .delta_since(&unfiltered_checkpoint)
        .map_err(|error| error.to_string())?;
    let unfiltered_bytes = sum_metric(&unfiltered_delta, "oracle_query_bytes_scanned_total");

    let selective_checkpoint = cluster
        .telemetry()
        .checkpoint()
        .map_err(|error| error.to_string())?;
    let selective_ids = query_ids(
        &reader,
        format!(
            "SELECT id, filter_key, unused_payload FROM {table_fqn} \
             WHERE filter_key = 'target' ORDER BY id"
        ),
    )
    .await?;
    let expected_ids = expected_marked_ids();
    if selective_ids != expected_ids {
        return Err(format!(
            "selective predicate returned the wrong residual rows: got {selective_ids:?} want {expected_ids:?}"
        )
        .into());
    }
    // The user-visible half of the contract: the filter removed rows, it did
    // not merely reorder them.
    if i64::try_from(selective_ids.len())? >= total_rows {
        return Err(format!(
            "selective predicate returned {} of {total_rows} written rows",
            selective_ids.len()
        )
        .into());
    }
    // The span proof. These two conditions cannot both hold unless the
    // compacted leaf and the hot leaf each contributed rows to one query.
    // Their id ranges are disjoint and each range lives in exactly one tier.
    if !selective_ids.iter().any(|id| *id < HOT_BATCH_ID_BASE) {
        return Err(format!("no compacted-tier row reached the client: {selective_ids:?}").into());
    }
    if !selective_ids.iter().any(|id| *id >= HOT_BATCH_ID_BASE) {
        return Err(format!("no hot-tier row reached the client: {selective_ids:?}").into());
    }

    let selective_delta = cluster
        .telemetry()
        .delta_since(&selective_checkpoint)
        .map_err(|error| error.to_string())?;
    let selective_bytes = sum_metric(&selective_delta, "oracle_query_bytes_scanned_total");
    // Bytes, not row groups, is the pruning signal for the compacted leaf, and
    // the choice is forced rather than preferred.
    // `oracle_query_row_groups_{scanned,pruned}_total` are fed only by
    // `OracleScanMetricsHandle::record_row_groups`, which the hot Parquet
    // reader calls and the Iceberg reader does not; the compacted leaf's
    // counters come from `iceberg::arrow::ScanMetrics`, which exposes byte
    // ranges and nothing else. Asserting on row groups here would therefore
    // measure the hot leaf and report it as coverage of the compacted one.
    //
    // Attribution still holds: every hot row matches the predicate, so the hot
    // leaf reads identical bytes filtered and unfiltered, and the whole of this
    // difference is the compacted leaf skipping row groups inside its published
    // data file. Removing `assignment.predicates` from the follower's
    // `provider.scan` call collapses it to zero, which is what makes this a
    // real assertion rather than a restatement of the fixture.
    //
    // Strict `Less` rather than a negated `<`: an incomparable (NaN) metric
    // must fail this proof, not silently satisfy it.
    if !matches!(
        selective_bytes.partial_cmp(&unfiltered_bytes),
        Some(std::cmp::Ordering::Less)
    ) {
        return Err(format!(
            "compacted leaf must scan strictly fewer bytes under the signed predicate: \
             selective={selective_bytes} unfiltered={unfiltered_bytes}"
        )
        .into());
    }

    cluster.shutdown().await?;
    Ok(())
}

/// Proves the Iceberg half of the projection contract on an isolated
/// compacted-only cut, before the hot cohort is written.
///
/// This is the compacted owner: the tier counts are asserted first, so every
/// file the query can reach is a published Iceberg data file and nothing
/// observed here can be attributed to the hot leaf.
///
/// The evidence is the closure the Iceberg physical scan was built with, not a
/// byte count. Scanned bytes cannot decide this leg: the dependency's reader
/// prefetches file tail for Parquet metadata and coalesces nearby byte ranges,
/// so a published file below those thresholds is fetched whole regardless of
/// which columns were requested, and both queries report the same total. That
/// is a property of the current reader policy — changing it is a separate
/// production performance decision with its own evidence requirements — not a
/// property of Wyrd's projection. What this proves, exactly, is that the narrow
/// closure reaches the Iceberg physical reader: the broad scan carries
/// `unused_payload`, the narrow one does not, both retain the hidden tenant
/// column, and the narrow closure is a strict subset of the broad one. It does
/// not claim that the narrow query issued fewer object-store bytes.
///
/// The hot-only journey owns the byte proof for `HotParquetExec`. Neither leg
/// is inferred from an aggregate reduction over the mixed-tier cut this journey
/// later builds.
///
/// # Errors
///
/// Returns a client, Postgres, or telemetry error surfaced by any step, and a
/// descriptive error when the cut is not compacted-only, when the two queries
/// disagree on residual identity, or when the observed Iceberg closures do not
/// show the narrow projection reaching the reader.
async fn prove_compacted_only_projection(
    cluster: &WyrdTestCluster,
    tenant: wyrd_spec::DataTenantId,
    table: &str,
    table_fqn: &str,
) -> Result<(), JourneyError> {
    cluster.refresh_oracle_snapshots().await?;
    let (compacted_files, hot_files) = file_tier_counts(cluster, tenant, table).await?;
    if compacted_files == 0 || hot_files != 0 {
        return Err(format!(
            "compacted-only projection proof requires a compacted-only cut: \
             compacted={compacted_files} hot={hot_files}"
        )
        .into());
    }

    let query_server = cluster.server(2).ok_or("missing query node")?;
    let reader = client(query_server, "compacted-projection-reader").await?;
    let expected_ids: Vec<i64> = (1..=COMPACTED_BATCH_ROWS)
        .filter(|id| marker_value(*id) == "target")
        .collect();

    iceberg_projection_probe::reset();
    let broad_ids = query_ids(
        &reader,
        format!(
            "SELECT id, filter_key, unused_payload FROM {table_fqn} \
             WHERE filter_key = 'target' ORDER BY id"
        ),
    )
    .await?;
    let broad_closure = observed_iceberg_closure("broad")?;
    if broad_ids != expected_ids {
        return Err(format!(
            "compacted-only broad query returned the wrong residual rows: \
             got {broad_ids:?} want {expected_ids:?}"
        )
        .into());
    }

    iceberg_projection_probe::reset();
    let narrow_ids = query_ids(
        &reader,
        format!("SELECT id FROM {table_fqn} WHERE filter_key = 'target' ORDER BY id"),
    )
    .await?;
    let narrow_closure = observed_iceberg_closure("narrow")?;
    // Row identity first: a projection that reached the reader but changed the
    // answer is a defect, not a proof.
    if narrow_ids != broad_ids {
        return Err(format!(
            "compacted-only narrow projection must return the same rows: \
             narrow={narrow_ids:?} broad={broad_ids:?}"
        )
        .into());
    }

    if !broad_closure.iter().any(|name| name == "unused_payload") {
        return Err(format!(
            "the broad query must carry the wide column into the Iceberg scan: {broad_closure:?}"
        )
        .into());
    }
    if narrow_closure.iter().any(|name| name == "unused_payload") {
        return Err(format!(
            "the narrow query must not carry the wide column into the Iceberg scan: \
             {narrow_closure:?}"
        )
        .into());
    }
    // The hidden tenant column survives to the reader on both, because the
    // tripwire downstream cannot enforce isolation on a column that was never
    // read.
    for (label, closure) in [("broad", &broad_closure), ("narrow", &narrow_closure)] {
        if !closure
            .iter()
            .any(|name| name == wyrd_spec::vala::managed_columns::DATA_TENANT_ID)
        {
            return Err(format!(
                "the {label} Iceberg closure dropped the hidden tenant column: {closure:?}"
            )
            .into());
        }
    }
    // Strict subset, so a narrow closure that merely reordered the broad one
    // fails rather than passes.
    if !narrow_closure
        .iter()
        .all(|name| broad_closure.contains(name))
        || narrow_closure.len() >= broad_closure.len()
    {
        return Err(format!(
            "the narrow Iceberg closure must be strictly narrower than the broad one: \
             narrow={narrow_closure:?} broad={broad_closure:?}"
        )
        .into());
    }
    Ok(())
}

/// Returns the one column closure every Iceberg physical scan was built with
/// since the last probe reset.
///
/// A distributed query builds one Iceberg leaf per follower assignment, so
/// several scans are expected; what the proof requires is that they all carry
/// the same closure, because the leader signs one closure for the whole
/// fragment and a leaf that disagreed with it would be reading columns nobody
/// authorized.
///
/// # Errors
///
/// Returns an error when no scan was observed, when the observed scans disagree
/// on their closure, and when any observed scan carried no projection at all —
/// the last being exactly the regression this proof exists to catch.
fn observed_iceberg_closure(label: &str) -> Result<Vec<String>, JourneyError> {
    let observed = iceberg_projection_probe::observed();
    let Some(first) = observed.first() else {
        return Err(format!("the {label} query built no Iceberg scan at all").into());
    };
    let closure = first.clone().ok_or_else(|| -> JourneyError {
        format!("the {label} query built its Iceberg scan with no projection at all").into()
    })?;
    for other in &observed {
        if other.as_ref() != Some(&closure) {
            return Err(format!(
                "the {label} query's Iceberg leaves disagree on their closure: \
                 {closure:?} and {other:?}"
            )
            .into());
        }
    }
    Ok(closure)
}

/// The marker a given id carries: every `MARKER_STRIDE`-th row of the
/// compacted batch, and every row of the hot batch.
///
/// The asymmetry is the point. `oracle_query_row_groups_pruned_total` carries
/// no per-source label, so a pruning delta on a two-tier query says only that
/// *some* leaf pruned. Making every hot row match leaves the hot leaf with
/// nothing it is allowed to skip, so a nonzero delta can only have come from
/// the compacted leaf. Without this, the hot leaf alone satisfies the
/// assertion and a compacted leaf that stopped pushing predicates entirely
/// still passes.
fn marker_value(id: i64) -> &'static str {
    if id >= HOT_BATCH_ID_BASE || id % MARKER_STRIDE == 0 {
        "target"
    } else {
        "other"
    }
}

/// The exact ascending ids the selective predicate must return, derived from
/// the same rule that wrote them.
fn expected_marked_ids() -> Vec<i64> {
    (1..=COMPACTED_BATCH_ROWS)
        .chain(HOT_BATCH_ID_BASE..HOT_BATCH_ID_BASE + HOT_BATCH_ROWS)
        .filter(|id| marker_value(*id) == "target")
        .collect()
}

/// Drains one successful query stream and returns its `id` column in stream
/// order.
///
/// Returning the ids rather than a count is what lets a caller name which
/// physical tier a row could only have come from.
///
/// # Errors
///
/// Returns a client or Arrow error, and an error when the stream carries no
/// terminal frame, does not succeed, or emits a batch whose leading column is
/// not a non-null `Int64`.
async fn query_ids(client: &WyrdClient, sql: String) -> Result<Vec<i64>, JourneyError> {
    let mut stream = wyrd_client::Bifrost::query_only(client)
        .query(&BifrostQueryRequest {
            sql,
            deadline_ms: None,
        })
        .await?;
    let mut ids = Vec::new();
    while let Some(batch) = stream.next_batch().await? {
        let column = batch
            .column_by_name("id")
            .ok_or("query result is missing its id column")?
            .as_any()
            .downcast_ref::<Int64Array>()
            .ok_or("query result id column is not Int64")?;
        for index in 0..column.len() {
            if column.is_null(index) {
                return Err("query result carried a null id".into());
            }
            ids.push(column.value(index));
        }
    }
    let terminal = stream.terminal().ok_or("query terminal missing")?;
    if terminal.outcome != QueryTerminalOutcome::Success || terminal.error.is_some() {
        return Err(format!(
            "query did not succeed: {:?} {:?}",
            terminal.outcome, terminal.error
        )
        .into());
    }
    Ok(ids)
}

/// Appends one journey row carrying a caller-chosen managed event time.
///
/// The write door refuses to declare `wyrd_event_time`, but public IPC ingest
/// accepts a supplied value and Scribe lifts it verbatim into the managed
/// slot, which is how a live row is placed in a chosen time partition.
///
/// # Errors
///
/// Returns an error when the client cannot connect or ingest refuses the row.
async fn append_event_time_row(
    client: &WyrdClient,
    table: &str,
    id: i64,
    event_time_micros: i64,
) -> Result<(), JourneyError> {
    let mut fields = journey_schema()
        .fields()
        .iter()
        .cloned()
        .collect::<Vec<_>>();
    fields.push(std::sync::Arc::new(arrow::datatypes::Field::new(
        wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME,
        arrow::datatypes::DataType::Timestamp(
            arrow::datatypes::TimeUnit::Microsecond,
            Some("UTC".into()),
        ),
        false,
    )));
    let batch = arrow::record_batch::RecordBatch::try_new(
        std::sync::Arc::new(arrow::datatypes::Schema::new(fields)),
        vec![
            std::sync::Arc::new(arrow::array::Int64Array::from(vec![id])),
            std::sync::Arc::new(arrow::array::StringArray::from(vec!["live"])),
            std::sync::Arc::new(arrow::array::StringArray::from(vec![unused_payload(id)])),
            std::sync::Arc::new(
                arrow::array::TimestampMicrosecondArray::from(vec![event_time_micros])
                    .with_timezone("UTC"),
            ),
        ],
    )?;
    let mut ipc = Vec::new();
    let mut ipc_writer =
        arrow::ipc::writer::StreamWriter::try_new(&mut ipc, batch.schema().as_ref())?;
    ipc_writer.write(&batch)?;
    ipc_writer.finish()?;
    drop(ipc_writer);
    wyrd_testing::bifrost::write::RawIngest::connect(client)
        .await?
        .insert(table, uuid::Uuid::now_v7(), ipc)
        .await?;
    Ok(())
}

/// Returns each node's cumulative Scribe fragment executions, in node order.
///
/// # Errors
///
/// Returns an error when a node in the topology carries no Scribe runtime.
fn scribe_fragment_executions(cluster: &WyrdTestCluster) -> Result<Vec<u64>, JourneyError> {
    cluster
        .servers()
        .map(|server| {
            server
                .state()
                .bifrost_ingest()
                .map(|scribe| scribe.fragment_inspection().0)
                .ok_or_else(|| JourneyError::from("topology node has no Scribe runtime"))
        })
        .collect()
}

/// Subtracts one per-node execution snapshot from a later one.
fn execution_delta(before: &[u64], after: &[u64]) -> Vec<u64> {
    after
        .iter()
        .zip(before)
        .map(|(after, before)| after.saturating_sub(*before))
        .collect()
}

/// Live rows on two relevant Scribes are read by live fragments sent to
/// exactly those two owners, while a third Scribe whose only live partition an
/// event-time predicate excludes executes nothing. A predicate that cannot
/// prune by event time keeps every reported route.
///
/// Nothing is flushed, so every returned row can only have come from a live
/// Scribe fragment: the query is live-only and still plans, admits, and
/// succeeds.
///
/// # Errors
///
/// Returns an error when the cluster, ingest, or query fails, or when rows or
/// fragment routes differ from the expected owners.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn live_query_routes_only_relevant_scribes() -> Result<(), JourneyError> {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::three_mixed()).await?;
    let tenant = cluster.data_tenant_id();
    let table = unique_table("oracle_live_routes");
    let table_fqn = format!("vala.bifrost.{table}");
    let first = cluster.server(0).ok_or("missing node 0")?;
    register_table(first, tenant, &table).await?;
    let now = chrono::Utc::now();
    // A week back is inside ingest's accepted window yet in a live partition
    // the one-hour event-time floor below provably excludes.
    let stale = (now - chrono::Duration::days(7)).timestamp_micros();
    let now = now.timestamp_micros();
    for (index, id, event_time) in [(0_usize, 1_i64, now), (1, 2, now), (2, 3, stale)] {
        let server = cluster.server(index).ok_or("missing writer node")?;
        let writer = client(server, &format!("live-writer-{index}")).await?;
        append_event_time_row(&writer, &table_fqn, id, event_time).await?;
    }
    cluster.refresh_oracle_snapshots().await?;
    let reader = client(first, "live-route-reader").await?;
    let floor = (chrono::Utc::now() - chrono::Duration::hours(1)).format("%Y-%m-%d %H:%M:%S%.6f");

    let before = scribe_fragment_executions(&cluster)?;
    let pruned = query_ids(
        &reader,
        format!(
            "SELECT id FROM {table_fqn} WHERE wyrd_event_time >= TIMESTAMP '{floor}' ORDER BY id"
        ),
    )
    .await?;
    let pruned_delta = execution_delta(&before, &scribe_fragment_executions(&cluster)?);
    if pruned != vec![1, 2] {
        return Err(format!("event-time floor expected live ids [1, 2], saw {pruned:?}").into());
    }
    if pruned_delta != vec![1, 1, 0] {
        return Err(format!(
            "only the two relevant Scribes may execute one live fragment each, saw {pruned_delta:?}"
        )
        .into());
    }

    let before = scribe_fragment_executions(&cluster)?;
    let unpruned = query_ids(
        &reader,
        format!("SELECT id FROM {table_fqn} WHERE id > 0 ORDER BY id"),
    )
    .await?;
    let unpruned_delta = execution_delta(&before, &scribe_fragment_executions(&cluster)?);
    if unpruned != vec![1, 2, 3] {
        return Err(
            format!("unprunable predicate expected ids [1, 2, 3], saw {unpruned:?}").into(),
        );
    }
    if unpruned_delta != vec![1, 1, 1] {
        return Err(format!(
            "an unprunable predicate keeps every reported live route, saw {unpruned_delta:?}"
        )
        .into());
    }
    Ok(())
}

/// Test-node binary every process-cluster pod runs.
const NODE_BINARY: &str = env!("CARGO_BIN_EXE_bifrost_peer_test_node");

/// Drains one public grouped count into `(filter_key, matched)` rows and its path.
///
/// # Errors
///
/// Returns a client or Arrow error, and an error when the stream does not
/// succeed or a batch does not carry a `Utf8` key and an `Int64` count.
async fn grouped_counts(
    client: &WyrdClient,
    sql: String,
) -> Result<(Vec<(String, i64)>, QueryExecutionPath), JourneyError> {
    let mut stream = wyrd_client::Bifrost::query_only(client)
        .query(&BifrostQueryRequest {
            sql,
            deadline_ms: Some(30_000),
        })
        .await?;
    let mut rows = Vec::new();
    while let Some(batch) = stream.next_batch().await? {
        let keys = batch
            .column_by_name("filter_key")
            .and_then(|column| column.as_any().downcast_ref::<arrow::array::StringArray>())
            .ok_or("grouped result has no Utf8 filter_key")?;
        let counts = batch
            .column_by_name("matched")
            .and_then(|column| column.as_any().downcast_ref::<Int64Array>())
            .ok_or("grouped result has no Int64 matched")?;
        for index in 0..batch.num_rows() {
            rows.push((keys.value(index).to_owned(), counts.value(index)));
        }
    }
    let terminal = stream.terminal().ok_or("query terminal missing")?;
    if terminal.outcome != QueryTerminalOutcome::Success || terminal.error.is_some() {
        return Err(format!(
            "query did not succeed: {:?} {:?}",
            terminal.outcome, terminal.error
        )
        .into());
    }
    Ok((rows, terminal.execution_path))
}

/// A filtered aggregate over published files and live rows on two Scribes is
/// one Analytical plan: published scans run on the Oracle workers while each
/// Scribe executes exactly one live fragment, and the result counts both.
///
/// Published rows alone, or live rows drained anywhere but the two Scribe
/// fragments, cannot produce these counts and deltas together.
///
/// # Errors
///
/// Returns an error when the process cluster, ingest, publication, or query
/// fails, or when counts or per-node fragment deltas differ.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn published_workers_and_live_scribes_share_one_plan() -> Result<(), JourneyError> {
    /// Oracle pod the public query enters.
    const COORDINATOR: usize = 0;
    /// Oracle pods that may run published work.
    const WORKERS: [usize; 2] = [1, 2];
    /// Scribe pods that each hold live rows.
    const SCRIBES: [usize; 2] = [3, 4];
    let mut cluster = BifrostProcessCluster::start(
        NODE_BINARY,
        &[
            ProcessNodeTarget::Oracle,
            ProcessNodeTarget::Oracle,
            ProcessNodeTarget::Oracle,
            ProcessNodeTarget::Scribe,
            ProcessNodeTarget::Scribe,
        ],
    )
    .await?;
    let api_key = cluster
        .provision_public_api_key("live-share-reader")
        .await?;
    let table = format!("live_share_{}", uuid::Uuid::now_v7().simple());
    let nodes = cluster.nodes_mut();
    nodes[SCRIBES[0]].register_table(&table)?;
    // Two published objects of ids 0..12 give every group 8 published rows
    // and the cut real work to split across both workers.
    nodes[SCRIBES[0]].ingest_rows(&table, 0, 12, 3)?;
    nodes[SCRIBES[0]].ingest_rows(&table, 0, 12, 3)?;
    // Six live rows per Scribe add 2 + 2 to every group, plus one negative id
    // per Scribe that the filter must remove.
    nodes[SCRIBES[0]].ingest_live_rows(&table, 100, 6, 3)?;
    nodes[SCRIBES[1]].ingest_live_rows(&table, 200, 6, 3)?;
    nodes[SCRIBES[0]].ingest_live_rows(&table, -1, 1, 3)?;
    nodes[SCRIBES[1]].ingest_live_rows(&table, -2, 1, 3)?;
    for node in nodes.iter_mut() {
        node.refresh_snapshot()?;
    }
    let polls_before = WORKERS
        .iter()
        .map(|index| cluster.nodes_mut()[*index].peer_body_polls())
        .collect::<Result<Vec<_>, _>>()?;
    let fragments_before = SCRIBES
        .iter()
        .map(|index| cluster.nodes_mut()[*index].scribe_fragments())
        .collect::<Result<Vec<_>, _>>()?;

    let client = public_client(&cluster.nodes()[COORDINATOR], &api_key)?;
    let (rows, path) = grouped_counts(
        &client,
        format!(
            "SELECT filter_key, COUNT(*) AS matched FROM vala.bifrost.{table} \
             WHERE id >= 0 GROUP BY filter_key ORDER BY filter_key"
        ),
    )
    .await?;

    let expected = (0..3)
        .map(|group| (format!("group_{group}"), 12_i64))
        .collect::<Vec<_>>();
    if rows != expected {
        return Err(
            format!("published plus live counts expected {expected:?}, saw {rows:?}").into(),
        );
    }
    if path != QueryExecutionPath::Analytical {
        return Err(
            format!("the distributed published scan must stay Analytical, saw {path:?}").into(),
        );
    }
    for (offset, index) in WORKERS.into_iter().enumerate() {
        let polls = cluster.nodes_mut()[index].peer_body_polls()?;
        if polls <= polls_before[offset] {
            return Err(format!("published worker {index} admitted no peer work").into());
        }
    }
    let fragments = SCRIBES
        .iter()
        .map(|index| cluster.nodes_mut()[*index].scribe_fragments())
        .collect::<Result<Vec<_>, _>>()?;
    let delta = execution_delta(&fragments_before, &fragments);
    if delta != vec![1, 1] {
        return Err(
            format!("each Scribe must execute exactly one live fragment, saw {delta:?}").into(),
        );
    }
    cluster.shutdown()?;
    Ok(())
}

/// How long the journey keeps one live Scribe producer paused mid-stream.
///
/// Past the 30-second window the removed tail-fence expiry enforced, so a
/// second timeout on the live read would fail the query here.
const LIVE_HOLD: Duration = Duration::from_secs(31);

/// Deadline of every live query here, well past [`LIVE_HOLD`].
///
/// The query deadline is the read's one timeout; the 30-second default would
/// end the held read on its own.
const LIVE_DEADLINE_MS: i64 = 120_000;

/// Bound on waiting for a paused, cancelled, or dropped live read to settle.
const LIVE_SETTLE_TIMEOUT: Duration = Duration::from_secs(60);

/// Reports whether the writer Scribe still holds a follower lease above `baseline`.
///
/// A follower lease charges a whole partition grant, orders of magnitude above
/// the memtable drift a few journey rows cause, so half a grant separates a
/// held lease from a released one without depending on exact memtable bytes.
///
/// # Errors
///
/// Returns an error when the node carries no Scribe or its root snapshot fails.
fn follower_lease_held(cluster: &WyrdTestCluster, baseline: usize) -> Result<bool, JourneyError> {
    let used = cluster
        .server(0)
        .ok_or("missing writer node")?
        .state()
        .bifrost_ingest()
        .ok_or("writer node has no Scribe runtime")?
        .resources()
        .snapshot()?
        .scribe_memory_used_bytes;
    Ok(used >= baseline + vala_bifrost_redux::resources::ORACLE_PARTITION_MEMORY_BYTES / 2)
}

/// Waits until no live producer is open, the writer's follower lease is
/// released, and no Oracle still admits a query or holds query memory.
///
/// # Errors
///
/// Returns an error naming `case` when any of them still holds after the bound.
async fn await_live_released(
    cluster: &WyrdTestCluster,
    baseline: usize,
    case: &str,
) -> Result<(), JourneyError> {
    let deadline = tokio::time::Instant::now() + LIVE_SETTLE_TIMEOUT;
    loop {
        let producers = vala_bifrost_redux::scribe::tail_rpc::open_live_producers_for_test();
        let held = follower_lease_held(cluster, baseline)?;
        let admitted = cluster.oracle_resource_snapshots()?.iter().any(|snapshot| {
            snapshot.oracle_active_queries != 0 || snapshot.oracle_query_memory_used_bytes != 0
        });
        if producers == 0 && !held && !admitted {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!(
                "{case}: {producers} live producers open, follower lease held={held}, \
                 Oracle admission held={admitted}"
            )
            .into());
        }
        tokio::time::sleep(SETTLEMENT_INTERVAL).await;
    }
}

/// Opens one public live query while the Scribe producer pause is armed and
/// waits until the producer has stopped after its first batch.
///
/// # Errors
///
/// Returns an error when the query cannot open, the producer never pauses, or
/// the first batch never reaches the Oracle live source.
async fn open_paused_live_query(
    query: &wyrd_client::Bifrost,
    sql: &str,
    case: &str,
) -> Result<wyrd_client::bifrost::QueryResultStream, JourneyError> {
    let pause = vala_bifrost_redux::scribe::tail_rpc::scribe_live_production_pause_for_test();
    let reached = vala_bifrost_redux::oracle::live_source_batches_for_test();
    pause.arm();
    let stream = query
        .query(&BifrostQueryRequest {
            sql: sql.to_owned(),
            deadline_ms: Some(LIVE_DEADLINE_MS),
        })
        .await?;
    tokio::time::timeout(LIVE_SETTLE_TIMEOUT, pause.wait_entered())
        .await
        .map_err(|_| format!("{case}: the Scribe producer never paused"))?;
    let deadline = tokio::time::Instant::now() + LIVE_SETTLE_TIMEOUT;
    while vala_bifrost_redux::oracle::live_source_batches_for_test() == reached {
        if tokio::time::Instant::now() >= deadline {
            return Err(format!(
                "{case}: no batch reached the Oracle live source before the pause"
            )
            .into());
        }
        tokio::time::sleep(SETTLEMENT_INTERVAL).await;
    }
    Ok(stream)
}

/// A live Scribe read streams batch by batch under backpressure and lives
/// exactly as long as its query.
///
/// Nothing is flushed, so every row is live. The Scribe producer is paused
/// before its second batch exists: the first batch has already reached the
/// Oracle live source while production waits, so the read is incremental
/// rather than a whole-cohort fetch. The pause is held past 30 seconds with
/// the snapshot and follower lease still held and no further batch produced,
/// then released, and the query succeeds with every row. Cancelling an open
/// read and separately dropping a public client stream each release the
/// producer, its snapshot, and its follower lease.
///
/// # Errors
///
/// Returns an error when the cluster, ingest, or query fails, or when batch
/// production, held resources, or their release differ from the contract.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn live_stream_backpressure_and_query_owned_lifetime() -> Result<(), JourneyError> {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::three_mixed()).await?;
    let tenant = cluster.data_tenant_id();
    let table = unique_table("oracle_live_lifetime");
    let table_fqn = format!("vala.bifrost.{table}");
    let writer_node = cluster.server(0).ok_or("missing node 0")?;
    register_table(writer_node, tenant, &table).await?;
    let writer = client(writer_node, "live-lifetime-writer").await?;
    let now = chrono::Utc::now().timestamp_micros();
    for id in 1..=3 {
        append_event_time_row(&writer, &table_fqn, id, now).await?;
    }
    cluster.refresh_oracle_snapshots().await?;
    let baseline = writer_node
        .state()
        .bifrost_ingest()
        .ok_or("writer node has no Scribe runtime")?
        .resources()
        .snapshot()?
        .scribe_memory_used_bytes;
    let leader = cluster.server(1).ok_or("missing node 1")?;
    let reader = client(leader, "live-lifetime-reader").await?;
    let query = wyrd_client::Bifrost::query_only(&reader);
    let pause = vala_bifrost_redux::scribe::tail_rpc::scribe_live_production_pause_for_test();
    let sql = format!("SELECT id FROM {table_fqn}");

    let case = "held past 30 seconds";
    let reached = vala_bifrost_redux::oracle::live_source_batches_for_test();
    let stream = open_paused_live_query(&query, &sql, case).await?;
    let drain = tokio::spawn(async move {
        let mut stream = stream;
        let mut ids = Vec::new();
        while let Some(batch) = stream.next_batch().await? {
            let column = batch
                .column_by_name("id")
                .and_then(|column| column.as_any().downcast_ref::<Int64Array>())
                .ok_or("query result id column is not Int64")?;
            ids.extend(column.iter().flatten());
        }
        let terminal = stream.terminal().ok_or("query terminal missing")?;
        if terminal.outcome != QueryTerminalOutcome::Success {
            return Err(format!("held live query ended {:?}", terminal.error).into());
        }
        Ok::<_, JourneyError>(ids)
    });
    tokio::time::sleep(LIVE_HOLD).await;
    let delivered = vala_bifrost_redux::oracle::live_source_batches_for_test() - reached;
    if delivered != 1 {
        return Err(format!("{case}: paused production delivered {delivered} batches").into());
    }
    if drain.is_finished() {
        return Err(format!("{case}: the paused query ended early: {:?}", drain.await?).into());
    }
    let producers = vala_bifrost_redux::scribe::tail_rpc::open_live_producers_for_test();
    if producers != 1 || !follower_lease_held(&cluster, baseline)? {
        return Err(format!(
            "{case}: a paused read must stay open with its snapshot and lease \
             (producers={producers})"
        )
        .into());
    }
    pause.release();
    let mut ids = tokio::time::timeout(LIVE_SETTLE_TIMEOUT, drain).await???;
    ids.sort_unstable();
    if ids != vec![1, 2, 3] {
        return Err(format!("{case}: expected live ids [1, 2, 3], saw {ids:?}").into());
    }
    await_live_released(&cluster, baseline, case).await?;

    let case = "cancelled";
    let stream = open_paused_live_query(&query, &sql, case).await?;
    let request_id = stream.request_id().clone();
    query.cancel(&request_id).await?;
    await_live_released(&cluster, baseline, case).await?;
    pause.release();
    drop(stream);

    let case = "client stream dropped";
    let stream = open_paused_live_query(&query, &sql, case).await?;
    drop(stream);
    await_live_released(&cluster, baseline, case).await?;
    pause.release();
    Ok(())
}

/// Returns the writer Scribe's cumulative live fragment footers.
///
/// # Errors
///
/// Returns an error when node 0 carries no Scribe runtime.
fn writer_fragment_footers(cluster: &WyrdTestCluster) -> Result<u64, JourneyError> {
    Ok(cluster
        .server(0)
        .ok_or("missing writer node")?
        .state()
        .bifrost_ingest()
        .ok_or("writer node has no Scribe runtime")?
        .fragment_inspection()
        .1)
}

/// Seeds one table with published ids 1..=3 and unflushed live ids 101..=103.
///
/// Node 2 writes and publishes the low ids; node 0 appends the high ids and
/// keeps them in its Scribe, so node 0 is the table's only live source. Oracle
/// snapshots are refreshed so every reader sees the published cut.
///
/// # Errors
///
/// Returns cluster, registration, write, flush, or refresh failures.
async fn seed_published_and_live(
    cluster: &WyrdTestCluster,
    prefix: &str,
) -> Result<String, JourneyError> {
    let table = unique_table(prefix);
    let table_fqn = format!("vala.bifrost.{table}");
    let live_node = cluster.server(0).ok_or("missing node 0")?;
    let published_node = cluster.server(2).ok_or("missing node 2")?;
    register_table(live_node, cluster.data_tenant_id(), &table).await?;
    let published = writer(published_node, "seed-published-writer").await?;
    for id in 1..=3 {
        published
            .write(
                &table_fqn,
                &journey_schema(),
                [journey_row(id, marker_value(id))],
            )
            .await?;
    }
    published_node.flush_bifrost().await?;
    let live = client(live_node, "seed-live-writer").await?;
    let now = chrono::Utc::now().timestamp_micros();
    for id in 101..=103 {
        append_event_time_row(&live, &table_fqn, id, now).await?;
    }
    cluster.refresh_oracle_snapshots().await?;
    Ok(table_fqn)
}

/// A completed `LIMIT` plan stops an opened live fragment it no longer needs.
///
/// Published ids stay below the predicate and live ids 101..=103 sit on one
/// unflushed Scribe. With the Scribe producer paused after its first batch,
/// `LIMIT 1` completes from that batch alone: the query succeeds with one
/// row, the paused fragment is cancelled without a footer, and its producer,
/// snapshot, and follower lease release without producing the remaining
/// rows. An ordered limit whose top row exists only live still returns it,
/// so DataFusion rather than an Oracle early-stop rule decides when a live
/// child is no longer needed.
///
/// # Errors
///
/// Returns an error when setup or a query fails, or when the limited result,
/// fragment cancellation, or resource release differs.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn limit_stops_unneeded_live_fragment_without_footer() -> Result<(), JourneyError> {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::three_mixed()).await?;
    let table_fqn = seed_published_and_live(&cluster, "oracle_live_limit").await?;
    let live_node = cluster.server(0).ok_or("missing node 0")?;
    let baseline = live_node
        .state()
        .bifrost_ingest()
        .ok_or("writer node has no Scribe runtime")?
        .resources()
        .snapshot()?
        .scribe_memory_used_bytes;
    let reader = client(cluster.server(1).ok_or("missing node 1")?, "limit-reader").await?;
    let query = wyrd_client::Bifrost::query_only(&reader);

    let case = "limit stops a paused live fragment";
    let footers = writer_fragment_footers(&cluster)?;
    let mut stream = open_paused_live_query(
        &query,
        &format!("SELECT id FROM {table_fqn} WHERE id > 100 LIMIT 1"),
        case,
    )
    .await?;
    let mut ids = Vec::new();
    while let Some(batch) = tokio::time::timeout(LIVE_SETTLE_TIMEOUT, stream.next_batch()).await?? {
        let column = batch
            .column_by_name("id")
            .and_then(|column| column.as_any().downcast_ref::<Int64Array>())
            .ok_or("query result id column is not Int64")?;
        ids.extend(column.iter().flatten());
    }
    let terminal = stream.terminal().ok_or("query terminal missing")?;
    if terminal.outcome != QueryTerminalOutcome::Success || terminal.error.is_some() {
        return Err(format!("{case}: ended {:?} {:?}", terminal.outcome, terminal.error).into());
    }
    // Shard lanes do not preserve append order, so any one live id qualifies.
    if ids.len() != 1 || !(101..=103).contains(&ids[0]) {
        return Err(format!("{case}: expected exactly one live id, saw {ids:?}").into());
    }
    drop(stream);
    await_live_released(&cluster, baseline, case).await?;
    let pause = vala_bifrost_redux::scribe::tail_rpc::scribe_live_production_pause_for_test();
    pause.release();
    if writer_fragment_footers(&cluster)? != footers {
        return Err(format!("{case}: the stopped fragment still wrote a footer").into());
    }

    let ordered = query_ids(
        &reader,
        format!("SELECT id FROM {table_fqn} ORDER BY id DESC LIMIT 2"),
    )
    .await?;
    if ordered != vec![103, 102] {
        return Err(format!("ordered limit expected live ids [103, 102], saw {ordered:?}").into());
    }
    Ok(())
}

/// How one matrix query ended, as a public client observed it.
struct ObservedQuery {
    /// Ids the client received before the stream ended.
    ids: Vec<i64>,
    /// Whether the client raised an error instead of ending cleanly.
    rejected: bool,
    /// Terminal outcome, `Failed` for an early typed refusal.
    outcome: QueryTerminalOutcome,
    /// Terminal warnings; empty for an early refusal.
    warnings: Vec<QueryWarning>,
}

/// Runs `sql` to its end and records what the client observed.
///
/// An early typed refusal counts as a rejected `Failed` query with no rows. A
/// stream error after the terminal frame is the client's rejection of a
/// failed stream and is recorded rather than returned.
///
/// # Errors
///
/// Returns client or Arrow errors that carry no terminal, and an error when a
/// cleanly ended stream has no terminal frame.
async fn observe_query(client: &WyrdClient, sql: &str) -> Result<ObservedQuery, JourneyError> {
    let opened = wyrd_client::Bifrost::query_only(client)
        .query(&BifrostQueryRequest {
            sql: sql.to_owned(),
            deadline_ms: None,
        })
        .await;
    let mut stream = match opened {
        Ok(stream) => stream,
        Err(BifrostClientError::Transport(WyrdError::Vala { .. })) => {
            return Ok(ObservedQuery {
                ids: Vec::new(),
                rejected: true,
                outcome: QueryTerminalOutcome::Failed,
                warnings: Vec::new(),
            });
        }
        Err(other) => return Err(other.into()),
    };
    let mut ids = Vec::new();
    let mut rejected = false;
    loop {
        match stream.next_batch().await {
            Ok(Some(batch)) => {
                let column = batch
                    .column_by_name("id")
                    .and_then(|column| column.as_any().downcast_ref::<Int64Array>())
                    .ok_or("query result id column is not Int64")?;
                ids.extend(column.iter().flatten());
            }
            Ok(None) => break,
            Err(_) if stream.terminal().is_some() => {
                rejected = true;
                break;
            }
            Err(error) => return Err(error.into()),
        }
    }
    let terminal = stream.terminal().ok_or("query terminal missing")?;
    Ok(ObservedQuery {
        ids,
        rejected,
        outcome: terminal.outcome,
        warnings: terminal.warnings.clone(),
    })
}

/// Requires a clean `Degraded` result with only the published ids.
///
/// # Errors
///
/// Returns an error naming `case` when the outcome, warning, or rows differ.
fn expect_degraded(case: &str, observed: &ObservedQuery) -> Result<(), JourneyError> {
    if observed.rejected
        || observed.outcome != QueryTerminalOutcome::Degraded
        || observed.warnings != vec![QueryWarning::LiveTailUnavailable]
        || observed.ids != vec![1, 2, 3]
    {
        return Err(format!(
            "{case}: expected Degraded published-only rows, saw {:?} {:?} rejected={} {:?}",
            observed.outcome, observed.warnings, observed.rejected, observed.ids
        )
        .into());
    }
    Ok(())
}

/// Requires a `Failed` result the client refused to accept as complete.
///
/// # Errors
///
/// Returns an error naming `case` when the query did not fail or the client
/// ended the stream cleanly.
fn expect_failed(case: &str, observed: &ObservedQuery) -> Result<(), JourneyError> {
    if observed.outcome != QueryTerminalOutcome::Failed || !observed.rejected {
        return Err(format!(
            "{case}: expected a rejected Failed query, saw {:?} rejected={} {:?}",
            observed.outcome, observed.rejected, observed.ids
        )
        .into());
    }
    Ok(())
}

/// Collects every Parquet file under `root` whose path names `table`.
///
/// # Errors
///
/// Returns filesystem errors from walking `root`.
fn table_parquet_files(
    root: &std::path::Path,
    table: &str,
) -> Result<Vec<std::path::PathBuf>, JourneyError> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "parquet")
                && path.to_string_lossy().contains(table)
            {
                found.push(path);
            }
        }
    }
    Ok(found)
}

/// Every live-read fault reaches exactly one terminal class.
///
/// One table holds published ids 1..=3 and live ids 101..=103 on node 0's
/// Scribe; node 1 reads. Each fault is driven at the boundary that owns it:
///
/// - an unavailable stream listing, and a selected Scribe refused before its
///   first row, each yield `Degraded` with `LiveTailUnavailable` and only the
///   published rows;
/// - a listing the ready Scribe refuses for its missing ticket fails rather
///   than degrades, because a trust-boundary refusal is not availability loss;
/// - a Scribe lost after its first row, a stream without its footer, a
///   rejected peer ticket, a refused follower lease, and a real live snapshot
///   over its signed batch ceiling each yield `Failed`, and the client
///   rejects the stream instead of accepting preceding rows;
/// - a Scribe stopped before discovery is absent rather than lost, so the
///   query succeeds with best-effort coverage and no warning;
/// - deleting the published data files fails the query rather than hiding
///   the published loss as a live omission.
///
/// # Errors
///
/// Returns an error when setup fails or any fault case reaches a terminal
/// class other than the one listed above.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn live_query_terminal_failure_matrix() -> Result<(), JourneyError> {
    let mut cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::three_mixed()).await?;
    let table_fqn = seed_published_and_live(&cluster, "oracle_live_faults").await?;
    let sql = format!("SELECT id FROM {table_fqn} ORDER BY id");
    {
        let reader_node = cluster.server(1).ok_or("missing node 1")?;
        let reader = client(reader_node, "fault-matrix-reader").await?;
        let complete = observe_query(&reader, &sql).await?;
        if complete.rejected
            || complete.outcome != QueryTerminalOutcome::Success
            || complete.ids != vec![1, 2, 3, 101, 102, 103]
        {
            return Err(format!(
                "baseline: expected every row, saw {:?} rejected={} {:?}",
                complete.outcome, complete.rejected, complete.ids
            )
            .into());
        }

        reader_node.set_tail_discovery_unavailable_for_test(true);
        let listing = observe_query(&reader, &sql).await;
        reader_node.set_tail_discovery_unavailable_for_test(false);
        expect_degraded("failed stream listing", &listing?)?;

        arm_tail_listing_ticket_rejection_for_test();
        expect_failed(
            "stream listing refused for its ticket",
            &observe_query(&reader, &sql).await?,
        )?;

        for (case, fault) in [
            (
                "Scribe unavailable before its first row",
                ScribeFragmentFault::UnavailableBeforeRows,
            ),
            (
                "Scribe lost after its first row",
                ScribeFragmentFault::UnavailableAfterFirstBatch,
            ),
            (
                "Scribe stream without its footer",
                ScribeFragmentFault::OmitFooter,
            ),
            (
                "rejected Scribe peer ticket",
                ScribeFragmentFault::RejectTicket,
            ),
            (
                "Scribe follower capacity refused",
                ScribeFragmentFault::CapacityRefused,
            ),
        ] {
            arm_scribe_fragment_fault_for_test(fault);
            let observed = observe_query(&reader, &sql).await?;
            if fault == ScribeFragmentFault::UnavailableBeforeRows {
                expect_degraded(case, &observed)?;
            } else {
                expect_failed(case, &observed)?;
            }
        }

        // Three separate live appends are three memtable batches, so a signed
        // ceiling of one makes the real bounded snapshot refuse for capacity.
        set_live_fragment_batch_bound_for_test(Some(1));
        let over_bound = observe_query(&reader, &sql).await;
        set_live_fragment_batch_bound_for_test(None);
        expect_failed("live snapshot over its batch bound", &over_bound?)?;
    }

    let stopped = cluster.server(0).ok_or("missing node 0")?.node_id();
    cluster.stop_node(stopped).await?;
    cluster.refresh_oracle_snapshots().await?;
    let reader = client(
        cluster.server(1).ok_or("missing node 1")?,
        "fault-matrix-after-stop",
    )
    .await?;
    let absent = observe_query(&reader, &sql).await?;
    if absent.rejected
        || absent.outcome != QueryTerminalOutcome::Success
        || !absent.warnings.is_empty()
        || !absent.ids.starts_with(&[1, 2, 3])
    {
        return Err(format!(
            "Scribe absent before discovery: expected Success, saw {:?} {:?} rejected={} {:?}",
            absent.outcome, absent.warnings, absent.rejected, absent.ids
        )
        .into());
    }

    let table = table_fqn.rsplit('.').next().ok_or("table name missing")?;
    let files = table_parquet_files(cluster.storage_root(), table)?;
    if files.is_empty() {
        return Err("published source fault: no published data file to remove".into());
    }
    for file in files {
        std::fs::remove_file(file)?;
    }
    expect_failed(
        "published data file lost",
        &observe_query(&reader, &sql).await?,
    )?;
    Ok(())
}
