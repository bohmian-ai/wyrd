//! Oracle journeys — Distributed follower dispatch: signed selective closures
//! and the physical pruning they buy on the follower's rebuilt leaf.
//!
//! Module of the `oracle` binary; see `main.rs` for the capability it proves
//! and `support.rs` for the fixtures it shares.

use std::time::Duration;

use arrow::array::{Array, Int64Array};
use vala_bifrost_redux::oracle::iceberg_projection_probe;
use wyrd_client::WyrdClient;
use wyrd_client::bifrost::BifrostClientError;
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{
    BifrostQueryRequest, FreshnessPolicy, QueryTerminalErrorCode, QueryTerminalOutcome,
    VisibilityMode,
};
use wyrd_spec::vala::error::BifrostError;
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
    let unfiltered_rows = query_rows(&reader, &table, VisibilityMode::PublishedOnly).await?;
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
            break;
        }
        tokio::time::sleep(SETTLEMENT_INTERVAL).await;
        inspection = cluster.oracle_inspection().await?;
    }
    if inspection.active_queries != 0
        || inspection.queued_queries != 0
        || inspection.reserved_memory_bytes != 0
        || inspection.reserved_spill_bytes != 0
        || inspection.peer_pending != 0
        || inspection.peer_running != 0
    {
        return Err(format!("Oracle runtime did not settle: {inspection:?}").into());
    }
    cluster.shutdown().await?;
    Ok(())
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
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
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
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
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
