//! Oracle journeys — Distributed follower dispatch: signed selective closures
//! and the physical pruning they buy on the follower's rebuilt leaf.
//!
//! Module of the `oracle` binary; see `main.rs` for the capability it proves
//! and `support.rs` for the fixtures it shares.

use std::path::{Path, PathBuf};
use std::time::Duration;

use arrow::array::{Array, Int64Array};
use parquet::arrow::ARROW_SCHEMA_META_KEY;
use parquet::arrow::ArrowWriter;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::file::metadata::KeyValue;
use vala_bifrost_redux::forge::ForgeConfig;
use vala_bifrost_redux::oracle::iceberg_projection_probe;
use vala_bifrost_redux::parquet::footer::tenant_key_value;
use vala_bifrost_redux::parquet::writer_properties::bifrost_writer_properties_with_metadata;
use vala_bifrost_redux::scribe::hot_source::HotAuthority;
use wyrd_client::WyrdClient;
use wyrd_client::bifrost::BifrostClientError;
use wyrd_server::config::BifrostTarget;
use wyrd_server::oracle::{
    ScribeFragmentFault, arm_scribe_fragment_fault_for_test, arm_tail_listing_stale_for_test,
    arm_tail_listing_stall_for_test,
};
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{
    BifrostQueryRequest, QueryClass, QueryTerminalErrorCode, QueryTerminalOutcome, QueryWarning,
};
use wyrd_spec::vala::error::BifrostError;
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::{BifrostClusterSpec, WyrdTestCluster};

use crate::peer_cluster::PeerCluster;
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
/// per-file footer tenant proof still fails closed once both are in effect.
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
/// fail-closed footer tenant proof.
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
    // Every proof below reads sealed hot Parquet: files and row groups
    // scanned, footer statistics, `HotParquetExec`'s projection mask and the
    // footer tenant proof. Scribe's unsealed live tail has none of those, so
    // the fixture flushes. Forge promotes each flushed object to Iceberg as
    // soon as it is published (REQ-002), which would move the measurement
    // onto the Iceberg leaf, so every Forge catalog is wrapped in the
    // production commit seam and the first promotion commit is parked before
    // any write. The parked attempt is the table's active attempt, so every
    // later promotion on any replica defers behind it and the cut stays hot
    // until shutdown drains the parked commit unsettled.
    let cluster = WyrdTestCluster::start_spec_with_forge_config_and_completion_observer(
        spec,
        ForgeConfig::default(),
        false,
        true,
    )
    .await?;
    let promotion = cluster
        .commit_uncertainty_catalog()
        .ok_or("the topology wraps its Forge catalog in the commit seam")?;
    promotion.pause_before_commit();
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
    tokio::time::timeout(Duration::from_secs(30), promotion.wait_for_before_commit())
        .await
        .map_err(|_| "the first promotion never reached the parked commit")?;
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

    // Footer tenant proof: a scanned file whose footer names a foreign tenant
    // must refuse the query with the tenant-isolation reason intact and
    // deliver no rows, even for a count(*) that reads no payload column.
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
    let (refused_rows, refused_outcome, refused_error) = query_terminal_either_surface(
        &reader,
        format!("SELECT count(*) AS total FROM {table_fqn}"),
    )
    .await?;
    if refused_rows != 0 {
        return Err(format!("foreign file reached a SQL operator under predicate pushdown: rows={refused_rows} outcome={refused_outcome:?} error={refused_error:?}").into());
    }
    if refused_outcome != QueryTerminalOutcome::Failed
        || refused_error != Some(QueryTerminalErrorCode::QueryTenantInvariant)
    {
        return Err(format!(
            "footer tenant proof did not fail closed: {refused_outcome:?} {refused_error:?}"
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
    // The first batch must reach Iceberg and the second must stay hot. Forge
    // promotes each flushed object as soon as it is published (REQ-002), so
    // every Forge catalog is wrapped in the production commit seam: inert
    // while the first batch promotes, then armed to park the first promotion
    // commit of the second batch. The parked attempt is the table's active
    // attempt, so every later promotion defers behind it and the second batch
    // stays hot until shutdown drains the parked commit unsettled.
    let cluster = WyrdTestCluster::start_spec_with_forge_config_and_completion_observer(
        BifrostClusterSpec::three_mixed(),
        ForgeConfig::default(),
        false,
        true,
    )
    .await?;
    let promotion = cluster
        .commit_uncertainty_catalog()
        .ok_or("the topology wraps its Forge catalog in the commit seam")?;
    let ingest_server = cluster
        .servers()
        .find(|server| server.bifrost_scribe().is_some())
        .ok_or("missing ingest node")?;
    let tenant = cluster.data_tenant_id();
    let table = unique_table("oracle_two_tier");
    register_table(ingest_server, tenant, &table).await?;
    // Compaction is on for every table by default. This journey measures the
    // promoted files exactly as Scribe sealed them, so a background rewrite
    // between its baseline and selective queries would change the cut under
    // measurement; the table opts out.
    let catalog = ingest_server
        .state()
        .bifrost_catalog()
        .ok_or("Scribe composition retains the shared catalog")?
        .iceberg_catalog();
    let binding = vala_bifrost_redux::catalog::TenantTableBinding::resolve((
        tenant,
        vala_bifrost_redux::catalog::TableRef::new(
            vala_bifrost_redux::namespaces::BifrostNamespace::Bifrost,
            &table,
        ),
    ))?;
    let loaded = catalog.load_table(&binding.table_ident()).await?;
    let tx = iceberg::transaction::Transaction::new(&loaded);
    let tx = iceberg::transaction::ApplyTransactionAction::apply(
        tx.update_table_properties().set(
            "wyrd.forge.enable-compaction".to_owned(),
            "false".to_owned(),
        ),
        tx,
    )?;
    tx.commit_once(catalog.as_ref()).await?;
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

    promotion.pause_before_commit();
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
    tokio::time::timeout(Duration::from_secs(30), promotion.wait_for_before_commit())
        .await
        .map_err(|_| "the hot batch's first promotion never reached the parked commit")?;
    cluster.refresh_oracle_snapshots().await?;

    // Precondition, asserted immediately before the query rather than assumed:
    // a promotion that escaped the parked commit would move the second batch
    // into Iceberg too, and a cut with only one physical tier in it would
    // silently prove half of what this journey claims.
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
    // Tenancy is proven per file from the footer, so neither closure reads a
    // per-row tenant column.
    for (label, closure) in [("broad", &broad_closure), ("narrow", &narrow_closure)] {
        if closure.iter().any(|name| name == "data_tenant_id") {
            return Err(format!(
                "the {label} Iceberg closure read a per-row tenant column: {closure:?}"
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

/// Drains one public grouped count into `(filter_key, matched)` rows and its path.
///
/// # Errors
///
/// Returns a client or Arrow error, and an error when the stream does not
/// succeed or a batch does not carry a `Utf8` key and an `Int64` count.
async fn grouped_counts(
    client: &WyrdClient,
    sql: String,
) -> Result<(Vec<(String, i64)>, QueryClass), JourneyError> {
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
    Ok((rows, terminal.query_class))
}

/// A filtered aggregate over published files and live rows on two Scribes is
/// one Analytical plan: the published scan runs on the one Oracle worker the
/// table's cut is frozen to while each Scribe executes exactly one live
/// fragment, and the result counts both.
///
/// The live leaf caps its stage at one task, so the aggregate above the union
/// stays on the coordinator and only the published leaf stage is remote. A
/// published table is bound to one destination, so exactly one worker leases a
/// graph for it.
///
/// Published rows alone, or live rows drained anywhere but the two Scribe
/// fragments, cannot produce these counts and deltas together.
///
/// # Errors
///
/// Returns an error when the peer cluster, ingest, publication, or query
/// fails, when counts or per-node fragment deltas differ, or when other than
/// exactly one published worker leased a graph for the query.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn published_workers_and_live_scribes_share_one_plan() -> Result<(), JourneyError> {
    /// Oracle pod the public query enters.
    const COORDINATOR: usize = 0;
    /// Oracle pods that may run published work.
    const WORKERS: [usize; 2] = [1, 2];
    /// Scribe pods that each hold live rows.
    const SCRIBES: [usize; 2] = [3, 4];
    let cluster = PeerCluster::start(&[
        BifrostTarget::Oracle,
        BifrostTarget::Oracle,
        BifrostTarget::Oracle,
        BifrostTarget::Scribe,
        BifrostTarget::Scribe,
    ])
    .await?;
    let api_key = cluster
        .provision_public_api_key("live-share-reader")
        .await?;
    let table = format!("live_share_{}", uuid::Uuid::now_v7().simple());
    cluster.register_table(SCRIBES[0], &table).await?;
    // Two published objects of ids 0..12 give every group 8 published rows.
    cluster.ingest_rows(SCRIBES[0], &table, 0, 12, 3).await?;
    cluster.ingest_rows(SCRIBES[0], &table, 0, 12, 3).await?;
    // Six live rows per Scribe add 2 + 2 to every group, plus one negative id
    // per Scribe that the filter must remove.
    cluster
        .ingest_live_rows(SCRIBES[0], &table, 100, 6, 3)
        .await?;
    cluster
        .ingest_live_rows(SCRIBES[1], &table, 200, 6, 3)
        .await?;
    cluster
        .ingest_live_rows(SCRIBES[0], &table, -1, 1, 3)
        .await?;
    cluster
        .ingest_live_rows(SCRIBES[1], &table, -2, 1, 3)
        .await?;
    cluster.refresh_snapshots().await?;
    // A published scan task runs on a worker only under a graph lease that
    // worker activated, so the per-worker activation delta attributes the
    // work; the process-wide body-poll delta proves it crossed a peer socket.
    let leases_before = WORKERS
        .iter()
        .map(|index| Ok(cluster.graph_leases(*index)?.0))
        .collect::<Result<Vec<_>, JourneyError>>()?;
    let polls_before = cluster.peer_body_polls();
    let fragments_before = SCRIBES
        .iter()
        .map(|index| cluster.scribe_fragments(*index))
        .collect::<Result<Vec<_>, _>>()?;

    let client = public_client(cluster.server(COORDINATOR)?, &api_key)?;
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
    if path != QueryClass::Analytical {
        return Err(
            format!("the distributed published scan must stay Analytical, saw {path:?}").into(),
        );
    }
    let mut leased = Vec::new();
    for (offset, index) in WORKERS.into_iter().enumerate() {
        if cluster.graph_leases(index)?.0 > leases_before[offset] {
            leased.push(index);
        }
    }
    if leased.len() != 1 {
        return Err(format!(
            "the published scan must run on its one frozen worker, saw leases on {leased:?}"
        )
        .into());
    }
    if cluster.peer_body_polls() <= polls_before {
        return Err("no published work crossed the private peer plane".into());
    }
    let fragments = SCRIBES
        .iter()
        .map(|index| cluster.scribe_fragments(*index))
        .collect::<Result<Vec<_>, _>>()?;
    let delta = execution_delta(&fragments_before, &fragments);
    if delta != vec![1, 1] {
        return Err(
            format!("each Scribe must execute exactly one live fragment, saw {delta:?}").into(),
        );
    }
    cluster.shutdown().await?;
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

/// Returns the query-memory bytes the writer Scribe's follower views hold.
///
/// A follower charges only the bytes its `DataFusion` consumers actually hold
/// on the pod's one governed pool, attributed to query memory. A paused
/// follower may legitimately hold almost nothing, so this proves release
/// (zero) rather than a held amount.
///
/// # Errors
///
/// Returns an error when the node carries no Scribe or its root snapshot fails.
fn writer_follower_bytes(cluster: &WyrdTestCluster) -> Result<usize, JourneyError> {
    Ok(cluster
        .server(0)
        .ok_or("missing writer node")?
        .state()
        .bifrost_ingest()
        .ok_or("writer node has no Scribe runtime")?
        .resources()
        .snapshot()?
        .oracle_query_memory_used_bytes)
}

/// Reports whether the leader node still admits the live query.
///
/// The leader's admitted query owns the follower work it dispatched, so an
/// admitted query while the read is paused proves the leader stream, not a
/// follower-side lease, is what keeps the read open.
///
/// # Errors
///
/// Returns an error when the leader node has no Bifrost resources or its
/// snapshot fails.
fn leader_admits_query(cluster: &WyrdTestCluster) -> Result<bool, JourneyError> {
    Ok(cluster
        .server(1)
        .ok_or("missing leader node")?
        .state()
        .bifrost_resources()
        .ok_or("leader node has no Bifrost resources")?
        .snapshot()?
        .oracle_active_queries
        != 0)
}

/// Waits until no live producer is open, the writer's follower views hold no
/// bytes, and no Oracle still admits a query or holds query memory.
///
/// # Errors
///
/// Returns an error naming `case` when any of them still holds after the bound.
async fn await_live_released(cluster: &WyrdTestCluster, case: &str) -> Result<(), JourneyError> {
    let deadline = tokio::time::Instant::now() + LIVE_SETTLE_TIMEOUT;
    loop {
        let producers = vala_bifrost_redux::scribe::tail_rpc::open_live_producers_for_test();
        let held = writer_follower_bytes(cluster)? != 0;
        let admitted = cluster.oracle_resource_snapshots()?.iter().any(|snapshot| {
            snapshot.oracle_active_queries != 0 || snapshot.oracle_query_memory_used_bytes != 0
        });
        if producers == 0 && !held && !admitted {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!(
                "{case}: {producers} live producers open, follower bytes held={held}, \
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
/// the producer's snapshot held under the leader's admitted query and no
/// further batch produced, then released, and the query succeeds with every
/// row. Cancelling an open
/// read and separately dropping a public client stream each release the
/// producer, its snapshot, and its follower's pool bytes.
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
    if producers != 1 || !leader_admits_query(&cluster)? {
        return Err(format!(
            "{case}: a paused read must stay open with its snapshot under the leader's \
             admitted query (producers={producers})"
        )
        .into());
    }
    pause.release();
    let mut ids = tokio::time::timeout(LIVE_SETTLE_TIMEOUT, drain).await???;
    ids.sort_unstable();
    if ids != vec![1, 2, 3] {
        return Err(format!("{case}: expected live ids [1, 2, 3], saw {ids:?}").into());
    }
    await_live_released(&cluster, case).await?;

    let case = "cancelled";
    let stream = open_paused_live_query(&query, &sql, case).await?;
    let request_id = stream.request_id().clone();
    query.cancel(&request_id).await?;
    await_live_released(&cluster, case).await?;
    pause.release();
    drop(stream);

    let case = "client stream dropped";
    let stream = open_paused_live_query(&query, &sql, case).await?;
    drop(stream);
    await_live_released(&cluster, case).await?;
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
/// snapshot, and follower pool bytes release without producing the remaining
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
    await_live_released(&cluster, case).await?;
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
    observe_query_within(client, sql, None).await
}

/// Runs `sql` under an optional client deadline and records what the client
/// observed, exactly as [`observe_query`] does.
///
/// # Errors
///
/// Returns the same errors as [`observe_query`].
async fn observe_query_within(
    client: &WyrdClient,
    sql: &str,
    deadline_ms: Option<i64>,
) -> Result<ObservedQuery, JourneyError> {
    let opened = wyrd_client::Bifrost::query_only(client)
        .query(&BifrostQueryRequest {
            sql: sql.to_owned(),
            deadline_ms,
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
/// - one stale listing restarts the attempt once on a refrozen roster and
///   returns every row, while a second stale listing on the refrozen roster
///   degrades rather than mixing cuts;
/// - a listing held past the query deadline is cancelled with the query, which
///   fails within the deadline, and the next query succeeds;
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

        arm_tail_listing_stale_for_test(1);
        let restarted = observe_query(&reader, &sql).await?;
        if restarted.rejected
            || restarted.outcome != QueryTerminalOutcome::Success
            || restarted.ids != vec![1, 2, 3, 101, 102, 103]
        {
            return Err(format!(
                "stale listing restart: expected every row, saw {:?} rejected={} {:?}",
                restarted.outcome, restarted.rejected, restarted.ids
            )
            .into());
        }
        arm_tail_listing_stale_for_test(2);
        expect_degraded(
            "stale listing on the refrozen roster",
            &observe_query(&reader, &sql).await?,
        )?;

        arm_tail_listing_stall_for_test();
        let started = std::time::Instant::now();
        let stalled = observe_query_within(&reader, &sql, Some(2_000)).await?;
        expect_failed("listing held past the query deadline", &stalled)?;
        if started.elapsed() > Duration::from_secs(10) {
            return Err(format!(
                "stalled listing outlived its deadline: {:?}",
                started.elapsed()
            )
            .into());
        }
        let after_stall = observe_query(&reader, &sql).await?;
        if after_stall.outcome != QueryTerminalOutcome::Success || after_stall.ids.len() != 6 {
            return Err(format!(
                "query after cancelled listing: expected Success, saw {:?} {:?}",
                after_stall.outcome, after_stall.ids
            )
            .into());
        }

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

/// A remote Scribe's staged run whose footer names another tenant fails the
/// query closed with the tenant reason and exactly one leader refusal audit.
///
/// The footer proof runs when the remote fragment stream is first polled,
/// after its schema frame, so the refusal ends an already-open peer stream.
/// The row is sealed to a staged run first, so the refused run is the only
/// source and no data row precedes the refusal: a stream status that lost its
/// tenant class there would let the leader omit the route and report a
/// successful or degraded query instead.
///
/// # Errors
///
/// Returns an error when the cluster, ingest, seal, run rewrite, or query
/// fails, or when the query succeeds, degrades, returns rows, carries another
/// error, or audits the refusal other than exactly once.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn remote_staged_footer_refusal_fails_closed() -> Result<(), JourneyError> {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::three_mixed()).await?;
    let tenant = cluster.data_tenant_id();
    let table = unique_table("oracle_staged_refusal");
    let table_fqn = format!("vala.bifrost.{table}");
    let leader = cluster.server(0).ok_or("missing node 0")?;
    let scribe = cluster.server(1).ok_or("missing node 1")?;
    register_table(leader, tenant, &table).await?;
    let writer = client(scribe, "staged-refusal-writer").await?;
    append_event_time_row(
        &writer,
        &table_fqn,
        1,
        chrono::Utc::now().timestamp_micros(),
    )
    .await?;
    scribe.seal_bifrost_writable_for_test().await?;
    let runs = staged_runs(scribe, tenant, &table).await?;
    let foreign = cluster.add_tenant("oracle-staged-foreign").await?;
    for run in &runs {
        rewrite_footer_tenant(run, foreign)?;
    }
    cluster.refresh_oracle_snapshots().await?;

    let reader = client(leader, "staged-refusal-reader").await?;
    let checkpoint = cluster
        .telemetry()
        .checkpoint()
        .map_err(|error| error.to_string())?;
    let (rows, outcome, error) =
        query_terminal_either_surface(&reader, format!("SELECT id FROM {table_fqn}")).await?;
    let delta = cluster
        .telemetry()
        .delta_since(&checkpoint)
        .map_err(|error| error.to_string())?;
    if rows != 0
        || outcome != QueryTerminalOutcome::Failed
        || error != Some(QueryTerminalErrorCode::QueryTenantInvariant)
    {
        return Err(format!(
            "staged foreign footer did not fail closed: rows={rows} {outcome:?} {error:?}"
        )
        .into());
    }
    let audits: f64 = delta
        .metrics
        .iter()
        .filter(|sample| {
            sample.family == "bifrost_oracle_security_events_total"
                && sample.labels.get("event_class").map(String::as_str) == Some("tenant_file")
        })
        .map(|sample| sample.value)
        .sum();
    if (audits - 1.0).abs() > f64::EPSILON {
        return Err(format!("expected one leader tenant_file audit, saw {audits}").into());
    }
    Ok(())
}

/// Returns the staged run paths serving `table` on `server`, once its rows have
/// left the memtable.
///
/// Staging completes after the seal returns, so the authority is polled under
/// a bound rather than sampled once.
///
/// # Errors
///
/// Returns an error when the authority registry is unreadable, or when the
/// table is not served only by staged runs within the bound.
async fn staged_runs(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    table: &str,
) -> Result<Vec<PathBuf>, JourneyError> {
    for _ in 0..200 {
        let mut runs = Vec::new();
        let mut memtable = false;
        for live in server.bifrost_live_authorities_for_test()? {
            if live.key.tenant != tenant || live.key.table.name != table {
                continue;
            }
            match live.authority {
                HotAuthority::StagedRun { runs: staged, .. } => runs.extend(staged),
                HotAuthority::Memtable => memtable = true,
                _ => {}
            }
        }
        if !runs.is_empty() && !memtable {
            return Ok(runs);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!("{table} never became served only by staged runs").into())
}

/// Rewrites one staged run in place so its footer names `foreign` as tenant.
///
/// Rows, schema and every other footer key are copied unchanged, so the
/// footer tenant proof is the only check the rewritten run can fail.
///
/// # Errors
///
/// Returns a filesystem, Arrow, or Parquet error.
fn rewrite_footer_tenant(run: &Path, foreign: DataTenantId) -> Result<(), JourneyError> {
    let builder = ParquetRecordBatchReaderBuilder::try_new(std::fs::File::open(run)?)?;
    let tenant = tenant_key_value(foreign);
    let mut metadata: Vec<KeyValue> = builder
        .metadata()
        .file_metadata()
        .key_value_metadata()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|entry| entry.key != tenant.key && entry.key != ARROW_SCHEMA_META_KEY)
        .collect();
    metadata.push(tenant);
    let rows = usize::try_from(builder.metadata().file_metadata().num_rows())?;
    let schema = builder.schema().clone();
    let batches = builder.build()?.collect::<Result<Vec<_>, _>>()?;
    let mut parquet = Vec::new();
    let properties = bifrost_writer_properties_with_metadata(rows, metadata, &[]);
    let mut writer = ArrowWriter::try_new(&mut parquet, schema, Some(properties))?;
    for batch in &batches {
        writer.write(batch)?;
    }
    writer.close()?;
    std::fs::write(run, parquet)?;
    Ok(())
}

/// Every Bifrost field id comes from the registered Iceberg table.
///
/// One fresh custom table and the three fresh canonical signal tables are
/// written through their public write doors. For each table the journey
/// proves the single authority end to end:
///
/// - the built-in declaration carries no field id at any depth, and the
///   registered table numbers its fields densely from 1, which is Iceberg's
///   own assignment rather than an adopted declaration;
/// - every object Scribe sealed carries, on every field at every depth, the
///   id the registered table assigns the field at the same path;
/// - Forge promotes those fresh objects, and every data file of the promoted
///   snapshot carries the same ids;
/// - Forge's footer decode refuses the same object against a table that
///   numbers one field differently.
///
/// # Errors
///
/// Returns an error when setup, writing, promotion, or reading fails, or when
/// any of the properties above does not hold.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn iceberg_assigned_field_ids_promote_fresh_signal_tables() -> Result<(), JourneyError> {
    use vala_bifrost_redux::catalog::{TableRef, TenantTableBinding};
    use vala_bifrost_redux::namespaces::BifrostNamespace;

    let cluster = WyrdTestCluster::start_spec_with_forge_config_and_completion_observer(
        BifrostClusterSpec::one_mixed(),
        ForgeConfig::default(),
        false,
        false,
    )
    .await?;
    let server = cluster.server(0).ok_or("missing node")?;
    let tenant = cluster.data_tenant_id();
    let custom = unique_table("oracle_field_ids");
    register_table(server, tenant, &custom).await?;
    let rows = writer(server, "field-id-writer").await?;
    rows.write(
        &format!("vala.bifrost.{custom}"),
        &journey_schema(),
        [journey_row(1, "target")],
    )
    .await?;
    server.flush_bifrost().await?;
    wyrd_testing::bifrost::canonical_signals::seed_canonical_signals(server, "field-id-signals")
        .await?;

    let catalog = server
        .state()
        .bifrost_catalog()
        .ok_or("Scribe composition retains the shared catalog")?
        .iceberg_catalog();
    let tables = [
        (BifrostNamespace::Bifrost, custom.as_str()),
        (BifrostNamespace::Traces, "spans"),
        (BifrostNamespace::Logs, "records"),
        (BifrostNamespace::Metrics, "points"),
    ];
    for (namespace, name) in tables {
        let table_ref = TableRef::new(namespace, name);
        if let Some(definition) = vala_bifrost_redux::tables::builtin_table(
            table_ref
                .namespace
                .as_str()
                .strip_prefix("vala.")
                .unwrap_or_default(),
            name,
        ) {
            let declared = (definition.schema)();
            if declared_field_ids(declared.fields()) {
                return Err(format!("{name}: the built-in declaration carries a field id").into());
            }
        }
        let binding = TenantTableBinding::resolve((tenant, table_ref))?;
        let loaded = catalog.load_table(&binding.table_ident()).await?;
        let registered = std::sync::Arc::clone(loaded.metadata().current_schema());
        let mut assigned = registered_ids(registered.as_struct(), "");
        assigned.sort_unstable_by_key(|(_, id)| *id);
        let dense = (1..).take(assigned.len()).collect::<Vec<i32>>();
        if assigned.iter().map(|(_, id)| *id).collect::<Vec<_>>() != dense {
            return Err(
                format!("{name}: the table's ids are not Iceberg's dense assignment").into(),
            );
        }

        let sealed: Vec<(String, i64)> = sqlx::query_as(
            "SELECT file_path, file_size FROM vala.file_list \
             WHERE data_tenant_id = $1 AND table_name = $2",
        )
        .bind(tenant.as_uuid())
        .bind(name)
        .fetch_all(cluster.pg_fixture().operator_pool().pool())
        .await?;
        if sealed.is_empty() {
            return Err(format!("{name}: Scribe sealed no object").into());
        }
        for (path, size) in &sealed {
            let bytes = loaded.file_io().new_input(path)?.read().await?;
            expect_registered_ids(name, &bytes, &registered)?;
            let size = u64::try_from(*size)?;
            vala_bifrost_redux::parquet::PromotedObjectFooter::decode(
                &bytes,
                path,
                std::sync::Arc::clone(&registered),
                size,
            )
            .map_err(|refusal| format!("{name}: Forge refuses its own table's ids: {refusal}"))?;
            let renumbered = renumbered_first_field(&registered)?;
            match vala_bifrost_redux::parquet::PromotedObjectFooter::decode(
                &bytes,
                path,
                std::sync::Arc::new(renumbered),
                size,
            ) {
                Err(refusal) if refusal.contains("field id") => {}
                Err(refusal) => {
                    return Err(format!("{name}: refused for another reason: {refusal}").into());
                }
                Ok(_) => {
                    return Err(format!("{name}: Forge accepted a disagreeing field id").into());
                }
            }
        }

        compact_sealed_batch(&cluster, tenant, name, i64::try_from(sealed.len())?).await?;
        let promoted = catalog.load_table(&binding.table_ident()).await?;
        let mut tasks = promoted.scan().select_all().build()?.plan_files().await?;
        let mut data_files = 0_usize;
        while let Some(task) = futures_util::TryStreamExt::try_next(&mut tasks).await? {
            let bytes = promoted
                .file_io()
                .new_input(&task.data_file_path)?
                .read()
                .await?;
            expect_registered_ids(name, &bytes, &registered)?;
            data_files += 1;
        }
        if data_files == 0 {
            return Err(format!("{name}: the promoted snapshot holds no data file").into());
        }
    }

    let reader = client(server, "field-id-reader").await?;
    if query_rows(&reader, &custom).await? != 1 {
        return Err("the promoted custom table does not read back its row".into());
    }
    cluster.shutdown().await?;
    Ok(())
}

/// Report whether any field in `fields`, at any depth, declares a field id.
fn declared_field_ids(fields: &arrow::datatypes::Fields) -> bool {
    use arrow::datatypes::DataType;

    fields.iter().any(|field| {
        field.metadata().contains_key("PARQUET:field_id")
            || match field.data_type() {
                DataType::List(element) | DataType::LargeList(element) => {
                    declared_field_ids(&arrow::datatypes::Fields::from(vec![
                        element.as_ref().clone(),
                    ]))
                }
                DataType::Struct(children) => declared_field_ids(children),
                _ => false,
            }
    })
}

/// Every `(path, id)` pair a registered Iceberg struct assigns, depth-first.
///
/// A list element is named `element` and a map's entries `key` and `value`,
/// so the same path names the same field in the table and in a Parquet file
/// read back through Iceberg's Arrow conversion.
fn registered_ids(fields: &iceberg::spec::StructType, prefix: &str) -> Vec<(String, i32)> {
    let mut ids = Vec::new();
    for field in fields.fields() {
        collect_ids(field, &format!("{prefix}{}", field.name), &mut ids);
    }
    ids
}

/// Push `field`'s id under `path`, then every descendant's.
fn collect_ids(field: &iceberg::spec::NestedField, path: &str, ids: &mut Vec<(String, i32)>) {
    use iceberg::spec::Type;

    ids.push((path.to_owned(), field.id));
    match field.field_type.as_ref() {
        Type::Struct(children) => ids.extend(registered_ids(children, &format!("{path}."))),
        Type::List(list) => collect_ids(&list.element_field, &format!("{path}.element"), ids),
        Type::Map(map) => {
            collect_ids(&map.key_field, &format!("{path}.key"), ids);
            collect_ids(&map.value_field, &format!("{path}.value"), ids);
        }
        _ => {}
    }
}

/// Require every field of one Parquet object to carry its table's id.
///
/// The footer schema is converted without the embedded Arrow schema, so the
/// ids read are exactly the Parquet field ids; a field without one fails the
/// conversion.
///
/// # Errors
///
/// Returns an error when the footer does not parse, a field carries no id, or
/// any path's id differs from the registered table's.
fn expect_registered_ids(
    table: &str,
    object: &bytes::Bytes,
    registered: &iceberg::spec::Schema,
) -> Result<(), JourneyError> {
    let footer = parquet::file::metadata::ParquetMetaDataReader::new().parse_and_finish(object)?;
    let arrow =
        parquet::arrow::parquet_to_arrow_schema(footer.file_metadata().schema_descr(), None)?;
    let written = iceberg::arrow::arrow_schema_to_schema(&arrow)
        .map_err(|error| format!("{table}: an object field carries no id: {error}"))?;
    let mut expected = registered_ids(registered.as_struct(), "");
    let mut actual = registered_ids(written.as_struct(), "");
    expected.sort();
    actual.sort();
    if actual != expected {
        return Err(format!(
            "{table}: object ids differ from the registered table: object={actual:?} table={expected:?}"
        )
        .into());
    }
    Ok(())
}

/// Return `registered` with its first top-level field numbered past every id.
///
/// # Errors
///
/// Returns the Iceberg error when the renumbered schema does not build.
fn renumbered_first_field(
    registered: &iceberg::spec::Schema,
) -> Result<iceberg::spec::Schema, JourneyError> {
    let shift = registered.highest_field_id() + 1;
    Ok(iceberg::spec::Schema::builder()
        .with_fields(
            registered
                .as_struct()
                .fields()
                .iter()
                .enumerate()
                .map(|(index, field)| {
                    let mut field = field.as_ref().clone();
                    if index == 0 {
                        field.id = shift;
                    }
                    std::sync::Arc::new(field)
                }),
        )
        .build()?)
}

/// Microseconds in one hour, the fixture's single event-time partition.
const FILTER_HOUR_MICROS: i64 = 3_600_000_000;

/// Width of each sealed file's disjoint event-time slice inside the hour.
const FILTER_SLICE_MICROS: i64 = 15 * 60 * 1_000_000;

/// Hot files every filtering table seals, one per flush.
const FILTER_FILES: i64 = 3;

/// Custom rows per sealed file.
///
/// Above the writer's 20,000-row page limit, so each custom file carries
/// several pages and a point lookup can skip pages inside its one row group.
const CUSTOM_ROWS_PER_FILE: i64 = 45_000;

/// Rows each canonical signal table seals per file.
const SIGNAL_ROWS_PER_FILE: i64 = 4;

/// The custom key the row-group and page probes look up; it lies in file 1.
const CUSTOM_PROBE_KEY: i64 = CUSTOM_ROWS_PER_FILE + 5_000;

/// The caller-declared custom Bloom column.
const CUSTOM_BLOOM_COLUMN: &str = "label";

/// On a proven hot-only cut, the caller-registered custom dataset and the
/// built-in spans, records, and points tables return exact matches and
/// nonmatches for Bloom-key equality, event-time ranges, mixed key/time
/// predicates, and null filters. Each physical mechanism is proven
/// separately, with a fixture that no other mechanism can satisfy:
///
/// - **sort order** — every sealed file is read back from storage and is in
///   its resolved layout's order even though rows were written out of order;
/// - **file-level min/max** — an event-time slice opens one file of three,
///   because the durable file-list bounds exclude the others before any
///   footer is read;
/// - **row-group min/max** — a custom key and a `wyrd_request_id` lookup open
///   every file but exclude the other files' only row groups by footer
///   statistics, with no Bloom exclusion;
/// - **Bloom** — an absent `trace_id`, verified Bloom-negative and inside its
///   file's min/max, and an absent custom label inside every file's bounds,
///   are excluded by the Bloom filter after statistics kept the group;
/// - **page index** — the custom key lookup skips rows of its retained
///   multi-page row group.
///
/// Rows are written through a Scribe-only node and read through an
/// Oracle-only node. Promotion is impossible during the proof: the only Forge
/// coordinator is the delayed third node, so no hot object can move into
/// Iceberg.
///
/// # Errors
///
/// Returns cluster, registration, write, storage, telemetry, or query errors,
/// or a description of the first expectation that does not hold.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn hot_filtering_mechanisms_cover_all_table_kinds() -> Result<(), JourneyError> {
    let cluster = WyrdTestCluster::start_spec_with_forge_config_and_completion_observer(
        BifrostClusterSpec::for_targets(&[
            BifrostTarget::Scribe,
            BifrostTarget::Oracle,
            BifrostTarget::Server,
        ]),
        ForgeConfig::default(),
        true,
        false,
    )
    .await?;
    let scribe = cluster.server(0).ok_or("missing Scribe node")?;
    let oracle = cluster.server(1).ok_or("missing Oracle node")?;
    let fixture = FilteringFixture::write(scribe).await?;
    cluster.refresh_oracle_snapshots().await?;
    let reader = client(oracle, "filtering-reader").await?;
    fixture.prove_hot_cut(&cluster, scribe, &reader).await?;
    cluster.shutdown().await?;
    Ok(())
}

/// The four-table filtering fixture: one hour of deterministic rows written
/// as three disjoint event-time slices, each sealed as its own hot file.
///
/// Every expected row identity is derived from the same `(file, row)` rule
/// that wrote it, so a query's exact answer never restates the data.
struct FilteringFixture {
    /// First microsecond of the fixture hour.
    hour: i64,
    /// Unqualified name of the caller-registered custom dataset.
    custom: String,
    /// The Scribe-node client that registered and wrote every table.
    writer: wyrd_testing::bifrost::write::BifrostWriter,
}

/// One query of the filtering matrix and its exact expected row identities.
struct FilterCase {
    /// Name reported with every failure and evidence line.
    name: String,
    /// The SQL, projecting the row identity as an `Int64` `id`, ordered by it.
    sql: String,
    /// Ascending row identities the query must return.
    expected: Vec<i64>,
}

/// Physical scan evidence one query's Oracle telemetry delta records.
#[derive(Debug, Clone, Copy)]
struct ScanEvidence {
    /// Hot files opened.
    files: f64,
    /// Row groups excluded by statistics or Bloom filters.
    row_groups_pruned: f64,
    /// Row groups of those excluded by a Bloom filter alone.
    bloom_pruned: f64,
    /// Rows the page index skipped inside retained row groups.
    page_rows_pruned: f64,
}

impl FilteringFixture {
    /// Registers the custom dataset, provisions the signal tables, and seals
    /// three hot files per table.
    ///
    /// The custom dataset is declared through the public `TableConfig` path
    /// with an explicit hour partition, sort key, and Bloom column; its
    /// server-resolved layout is asserted before any row is written. Each
    /// slice writes one batch per table and then flushes, so every table seals
    /// exactly one file per slice.
    ///
    /// # Errors
    ///
    /// Returns registration, describe, write, or flush errors, and an error
    /// when the resolved custom layout differs from the declaration.
    async fn write(server: &WyrdTestServer) -> Result<Self, JourneyError> {
        let now = chrono::Utc::now().timestamp_micros();
        let hour = (now / FILTER_HOUR_MICROS - 2) * FILTER_HOUR_MICROS;
        let fixture = Self {
            hour,
            custom: unique_table("oracle_filtering"),
            writer: writer(server, "filtering-writer").await?,
        };
        fixture.register_custom().await?;
        for (namespace, table) in [
            ("traces", "spans"),
            ("logs", "records"),
            ("metrics", "points"),
        ] {
            server
                .ensure_builtin_table_for_test(server.data_tenant_id(), namespace, table)
                .await?;
        }
        let mut schemas = Vec::new();
        for fqn in [
            "vala.traces.spans",
            "vala.logs.records",
            "vala.metrics.points",
        ] {
            let described =
                wyrd_client::bifrost::TableConfig::describe(fixture.writer.client(), fqn).await?;
            schemas.push(std::sync::Arc::clone(described.user_schema()));
        }
        let [spans, records, points] = schemas.as_slice() else {
            return Err("three signal schemas were described".into());
        };
        for file in 0..FILTER_FILES {
            fixture
                .writer
                .write_batch(&fixture.custom_fqn(), &fixture.custom_batch(file)?)
                .await?;
            fixture
                .writer
                .write_batch("vala.traces.spans", &fixture.spans_batch(spans, file)?)
                .await?;
            fixture
                .writer
                .write_batch("vala.logs.records", &fixture.records_batch(records, file)?)
                .await?;
            fixture
                .writer
                .write_batch("vala.metrics.points", &fixture.points_batch(points, file)?)
                .await?;
            server.flush_bifrost().await?;
        }
        Ok(fixture)
    }

    /// Registers the custom dataset through the public client and asserts the
    /// layout the server resolved from the declaration.
    ///
    /// # Errors
    ///
    /// Returns the registration or describe error, and an error when the
    /// resolved partition, sort order, or Bloom columns differ from the
    /// declaration.
    async fn register_custom(&self) -> Result<(), JourneyError> {
        use wyrd_spec::vala::api::{
            NullOrderWire, PhysicalLayoutWire, SortDirectionWire, SortKeyWire, TimeGranularityWire,
        };

        let declared_sort = vec![SortKeyWire {
            column: "key_id".to_owned(),
            direction: SortDirectionWire::Asc,
            null_order: NullOrderWire::Last,
        }];
        let config =
            wyrd_client::bifrost::TableConfig::from_arrow(&self.custom_fqn(), custom_schema())?
                .with_layout(PhysicalLayoutWire {
                    partition_granularity: TimeGranularityWire::Hour,
                    sort_keys: declared_sort.clone(),
                    bloom_columns: vec![CUSTOM_BLOOM_COLUMN.to_owned()],
                });
        wyrd_client::Bifrost::connect_with_table(self.writer.client(), config)
            .await?
            .register()
            .await?;
        let resolved = wyrd_client::Bifrost::query_only(self.writer.client())
            .describe(&self.custom_fqn())
            .await?
            .physical_layout;
        if resolved.partition_granularity != TimeGranularityWire::Hour
            || resolved.sort_keys != declared_sort
            || !resolved
                .bloom_columns
                .iter()
                .any(|column| column == CUSTOM_BLOOM_COLUMN)
            || resolved
                .bloom_columns
                .iter()
                .any(|column| column == "key_id" || column == "score")
        {
            return Err(format!(
                "the server did not resolve the declared custom layout: {resolved:?}"
            )
            .into());
        }
        Ok(())
    }

    /// The custom dataset's fully qualified name.
    fn custom_fqn(&self) -> String {
        format!("vala.datasets.{}", self.custom)
    }

    /// First microsecond of `file`'s event-time slice.
    fn slice_start(&self, file: i64) -> i64 {
        self.hour + file * FILTER_SLICE_MICROS
    }

    /// The half-open SQL event-time predicate selecting `file`'s slice.
    ///
    /// # Errors
    ///
    /// Returns an error when a slice bound is not a representable timestamp.
    fn slice_filter(&self, file: i64) -> Result<String, JourneyError> {
        let start = self.slice_start(file);
        Ok(format!(
            "wyrd_event_time >= TIMESTAMP '{}' AND wyrd_event_time < TIMESTAMP '{}'",
            sql_timestamp(start)?,
            sql_timestamp(start + FILTER_SLICE_MICROS)?
        ))
    }

    /// The event time of custom `key`, inside its file's slice.
    fn custom_event_time(&self, key: i64) -> i64 {
        self.slice_start(key / CUSTOM_ROWS_PER_FILE) + (key % CUSTOM_ROWS_PER_FILE) * 10_000
    }

    /// The event time of signal row `row` of `file`, inside the file's slice.
    fn signal_event_time(&self, file: i64, row: i64) -> i64 {
        self.slice_start(file) + row * 1_000_000
    }

    /// One custom slice, written in descending key order so the sealed file's
    /// ascending order can only come from the declared sort key.
    ///
    /// # Errors
    ///
    /// Returns the Arrow error when the batch does not assemble.
    fn custom_batch(&self, file: i64) -> Result<arrow::record_batch::RecordBatch, JourneyError> {
        let keys: Vec<i64> = custom_keys(file).rev().collect();
        let batch = arrow::record_batch::RecordBatch::try_new(
            custom_schema(),
            vec![
                std::sync::Arc::new(arrow::array::Int64Array::from(keys.clone())),
                std::sync::Arc::new(arrow::array::StringArray::from(
                    keys.iter()
                        .map(|key| custom_label(*key))
                        .collect::<Vec<_>>(),
                )),
                std::sync::Arc::new(arrow::array::Float64Array::from(
                    keys.iter()
                        .map(|key| f64::from(i32::try_from(*key).unwrap_or(i32::MAX)))
                        .collect::<Vec<_>>(),
                )),
            ],
        )?;
        with_event_time(
            &batch,
            keys.iter()
                .map(|key| self.custom_event_time(*key))
                .collect(),
        )
    }

    /// One span slice over the described schema, written in ascending event
    /// time so the sealed descending order comes from the layout.
    ///
    /// Row `row` belongs to trace `fixture_trace_id(file, 2 * row)`; odd
    /// ordinals inside a file's range are never written, which is where the
    /// absent Bloom probe is drawn from. Even rows are roots.
    ///
    /// # Errors
    ///
    /// Returns the Arrow error when the event-time column does not attach.
    fn spans_batch(
        &self,
        schema: &arrow::datatypes::SchemaRef,
        file: i64,
    ) -> Result<arrow::record_batch::RecordBatch, JourneyError> {
        use wyrd_testing::bifrost::canonical_signals::{Cell, Row, attributes, batch};

        let rows: Vec<Row> = (0..SIGNAL_ROWS_PER_FILE)
            .map(|row| {
                let id = signal_id(file, row);
                let mut cells = Row::from([
                    (
                        "trace_id",
                        Cell::Bytes(fixture_trace_id(file, 2 * row).to_vec()),
                    ),
                    ("span_id", Cell::Bytes(fixture_span_id(file, row).to_vec())),
                    ("name", Cell::Text(format!("operation-{row}"))),
                    ("kind", Cell::Int32(1)),
                    ("start_time_unix_nano", Cell::Int64(id)),
                    ("end_time_unix_nano", Cell::Int64(id + 10)),
                    ("duration_nano", Cell::Int64(10)),
                    ("status_present", Cell::Bool(true)),
                    ("status_code", Cell::Int32(1)),
                    ("resource_present", Cell::Bool(true)),
                    (
                        "resource_attributes",
                        Cell::Bytes(attributes(&[("service.name", "filtering")])),
                    ),
                    ("scope_present", Cell::Bool(true)),
                    ("scope_name", Cell::Text("filtering".to_owned())),
                    ("service_name", Cell::Text("filtering".to_owned())),
                ]);
                if row % 2 == 1 {
                    cells.insert(
                        "parent_span_id",
                        Cell::Bytes(fixture_span_id(file, row - 1).to_vec()),
                    );
                }
                cells
            })
            .collect();
        with_event_time(
            &batch(schema, &rows),
            (0..SIGNAL_ROWS_PER_FILE)
                .map(|row| self.signal_event_time(file, row))
                .collect(),
        )
    }

    /// One log slice: even rows are uncorrelated (null `trace_id`) and row 0
    /// carries no event name; severity rises with the row.
    ///
    /// # Errors
    ///
    /// Returns the Arrow error when the event-time column does not attach.
    fn records_batch(
        &self,
        schema: &arrow::datatypes::SchemaRef,
        file: i64,
    ) -> Result<arrow::record_batch::RecordBatch, JourneyError> {
        use wyrd_testing::bifrost::canonical_signals::{Cell, Row, batch};

        let rows: Vec<Row> = (0..SIGNAL_ROWS_PER_FILE)
            .map(|row| {
                let id = signal_id(file, row);
                let mut cells = Row::from([
                    ("time_unix_nano", Cell::Int64(id)),
                    ("observed_time_unix_nano", Cell::Int64(id)),
                    ("severity_number", Cell::Int32(record_severity(row))),
                    ("severity_text", Cell::Text("INFO".to_owned())),
                    ("resource_present", Cell::Bool(true)),
                    ("scope_present", Cell::Bool(true)),
                    ("scope_name", Cell::Text("filtering".to_owned())),
                ]);
                if row > 0 {
                    cells.insert("event_name", Cell::Text(format!("event-{row}")));
                }
                if row % 2 == 1 {
                    cells.insert(
                        "trace_id",
                        Cell::Bytes(fixture_trace_id(file, 2 * row).to_vec()),
                    );
                    cells.insert("span_id", Cell::Bytes(fixture_span_id(file, row).to_vec()));
                }
                cells
            })
            .collect();
        with_event_time(
            &batch(schema, &rows),
            (0..SIGNAL_ROWS_PER_FILE)
                .map(|row| self.signal_event_time(file, row))
                .collect(),
        )
    }

    /// One point slice: even rows are integer sums and odd rows are double
    /// gauges, so `int_value` is null on exactly the odd rows.
    ///
    /// # Errors
    ///
    /// Returns the Arrow error when the event-time column does not attach.
    fn points_batch(
        &self,
        schema: &arrow::datatypes::SchemaRef,
        file: i64,
    ) -> Result<arrow::record_batch::RecordBatch, JourneyError> {
        use wyrd_testing::bifrost::canonical_signals::{Cell, Row, batch};

        let rows: Vec<Row> = (0..SIGNAL_ROWS_PER_FILE)
            .map(|row| {
                let id = signal_id(file, row);
                let mut cells = Row::from([
                    ("metric_name", Cell::Text(metric_name(file, row))),
                    ("unit", Cell::Text("1".to_owned())),
                    ("time_unix_nano", Cell::Int64(id)),
                    ("start_time_unix_nano", Cell::Int64(id)),
                    ("resource_present", Cell::Bool(true)),
                    ("scope_present", Cell::Bool(true)),
                    ("scope_name", Cell::Text("filtering".to_owned())),
                ]);
                if row % 2 == 0 {
                    cells.insert("metric_type", Cell::Text("sum".to_owned()));
                    cells.insert("int_value", Cell::Int64(row));
                    cells.insert("aggregation_temporality", Cell::Int32(2));
                    cells.insert("is_monotonic", Cell::Bool(true));
                } else {
                    cells.insert("metric_type", Cell::Text("gauge".to_owned()));
                    cells.insert("double_value", Cell::Float64(0.5));
                }
                cells
            })
            .collect();
        with_event_time(
            &batch(schema, &rows),
            (0..SIGNAL_ROWS_PER_FILE)
                .map(|row| self.signal_event_time(file, row))
                .collect(),
        )
    }

    /// Proves the hot-only cut, the physical layout of every sealed file, and
    /// the per-mechanism query matrix.
    ///
    /// # Errors
    ///
    /// Returns storage, telemetry, or query errors, and a description of the
    /// first physical or result expectation that does not hold.
    async fn prove_hot_cut(
        &self,
        cluster: &WyrdTestCluster,
        server: &WyrdTestServer,
        reader: &WyrdClient,
    ) -> Result<(), JourneyError> {
        use vala_bifrost_redux::namespaces::BifrostNamespace;

        let tenant = cluster.data_tenant_id();
        for table in [self.custom.as_str(), "spans", "records", "points"] {
            let (compacted, hot) = file_tier_counts(cluster, tenant, table).await?;
            if compacted != 0 || hot != FILTER_FILES {
                return Err(format!(
                    "{table}: the proof needs a hot-only cut of {FILTER_FILES} files: \
                     hot={hot} compacted={compacted}"
                )
                .into());
            }
        }

        let custom = SealedObject::read_tier(
            cluster,
            server,
            tenant,
            BifrostNamespace::Datasets,
            &self.custom,
            PhysicalTier::Hot,
        )
        .await?;
        let spans = SealedObject::read_tier(
            cluster,
            server,
            tenant,
            BifrostNamespace::Traces,
            "spans",
            PhysicalTier::Hot,
        )
        .await?;
        let records = SealedObject::read_tier(
            cluster,
            server,
            tenant,
            BifrostNamespace::Logs,
            "records",
            PhysicalTier::Hot,
        )
        .await?;
        let points = SealedObject::read_tier(
            cluster,
            server,
            tenant,
            BifrostNamespace::Metrics,
            "points",
            PhysicalTier::Hot,
        )
        .await?;
        let probes = self
            .expect_physical_layout(&custom, &spans, &records, &points)
            .await?;
        self.run_matrix(cluster, reader, &probes).await
    }

    /// Asserts each table's sealed files follow its resolved layout and draws
    /// the Bloom-negative probes the matrix needs.
    ///
    /// # Errors
    ///
    /// Returns describe or decode errors, and an error naming the first file
    /// that is out of order, lacks a declared Bloom filter, carries an
    /// undeclared one, lacks pages, or yields no Bloom-negative probe.
    async fn expect_physical_layout(
        &self,
        custom: &[SealedObject],
        spans: &[SealedObject],
        records: &[SealedObject],
        points: &[SealedObject],
    ) -> Result<FilterProbes, JourneyError> {
        let client = self.writer.client();
        for (fqn, objects, undeclared) in [
            (self.custom_fqn(), custom, "score"),
            ("vala.traces.spans".to_owned(), spans, "name"),
            ("vala.logs.records".to_owned(), records, "severity_text"),
            ("vala.metrics.points".to_owned(), points, "unit"),
        ] {
            let layout = wyrd_client::Bifrost::query_only(client)
                .describe(&fqn)
                .await?
                .physical_layout;
            if objects.len() != usize::try_from(FILTER_FILES)? {
                return Err(format!("{fqn}: expected {FILTER_FILES} sealed files").into());
            }
            for object in objects {
                object.expect_sorted(&fqn, &layout.sort_keys)?;
                object.expect_bloom_columns(&fqn, &layout.bloom_columns, undeclared)?;
            }
        }
        for object in custom {
            let pages = object.page_count("key_id")?;
            if pages < 2 {
                return Err(format!(
                    "{}: a custom file must carry several key pages, saw {pages}",
                    object.path
                )
                .into());
            }
        }

        // Odd ordinals strictly between file 1's smallest and largest written
        // trace are unwritten yet inside its statistics bounds.
        let span_probe = (1..2 * (SIGNAL_ROWS_PER_FILE - 1))
            .step_by(2)
            .map(|ordinal| fixture_trace_id(1, ordinal))
            .find(|candidate| {
                spans[1]
                    .may_contain("trace_id", candidate)
                    .is_ok_and(|hit| !hit)
            })
            .ok_or("no unwritten trace id inside file 1's bounds is Bloom-negative")?;
        // Odd labels lie between every file's `label-00` and `label-14`.
        let label_probe = (0..7)
            .map(|index| format!("label-{:02}", 2 * index + 1))
            .find(|candidate| {
                custom.iter().all(|object| {
                    object
                        .may_contain(CUSTOM_BLOOM_COLUMN, candidate.as_bytes())
                        .is_ok_and(|hit| !hit)
                })
            })
            .ok_or("no unwritten label inside every file's bounds is Bloom-negative")?;
        let request_ids = custom
            .iter()
            .map(SealedObject::request_id)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(FilterProbes {
            span_probe,
            label_probe,
            request_ids,
        })
    }

    /// Runs every table's query matrix and asserts each mechanism's evidence.
    ///
    /// # Errors
    ///
    /// Returns query or telemetry errors, and an error naming the first case
    /// whose rows or physical evidence differ from the expectation.
    async fn run_matrix(
        &self,
        cluster: &WyrdTestCluster,
        reader: &WyrdClient,
        probes: &FilterProbes,
    ) -> Result<(), JourneyError> {
        let custom = self.custom_fqn();
        let all_custom: Vec<i64> = (0..FILTER_FILES).flat_map(custom_keys).collect();
        let custom_where = |keep: &dyn Fn(i64) -> bool| -> Vec<i64> {
            all_custom
                .iter()
                .copied()
                .filter(|key| keep(*key))
                .collect()
        };

        let baseline = run_case(
            cluster,
            reader,
            FilterCase {
                name: "custom unfiltered".to_owned(),
                sql: format!("SELECT key_id AS id FROM {custom} ORDER BY id"),
                expected: all_custom.clone(),
            },
        )
        .await?;
        expect_measure("custom unfiltered", "files", baseline.files, 3.0)?;
        expect_measure(
            "custom unfiltered",
            "pruned",
            baseline.row_groups_pruned,
            0.0,
        )?;

        run_case(
            cluster,
            reader,
            FilterCase {
                name: "custom Bloom key present".to_owned(),
                sql: format!(
                    "SELECT key_id AS id FROM {custom} WHERE label = 'label-04' ORDER BY id"
                ),
                expected: custom_where(&|key| custom_label(key).as_deref() == Some("label-04")),
            },
        )
        .await?;

        let bloom = run_case(
            cluster,
            reader,
            FilterCase {
                name: "custom Bloom key absent within bounds".to_owned(),
                sql: format!(
                    "SELECT key_id AS id FROM {custom} WHERE label = '{}' ORDER BY id",
                    probes.label_probe
                ),
                expected: Vec::new(),
            },
        )
        .await?;
        expect_measure("custom Bloom absent", "files", bloom.files, 3.0)?;
        expect_measure("custom Bloom absent", "bloom", bloom.bloom_pruned, 3.0)?;
        expect_measure(
            "custom Bloom absent",
            "pruned",
            bloom.row_groups_pruned,
            3.0,
        )?;

        let point = run_case(
            cluster,
            reader,
            FilterCase {
                name: "custom key min/max and page index".to_owned(),
                sql: format!(
                    "SELECT key_id AS id FROM {custom} WHERE key_id = {CUSTOM_PROBE_KEY} ORDER BY id"
                ),
                expected: vec![CUSTOM_PROBE_KEY],
            },
        )
        .await?;
        expect_measure("custom key", "files", point.files, 3.0)?;
        expect_measure("custom key", "pruned", point.row_groups_pruned, 2.0)?;
        expect_measure("custom key", "bloom", point.bloom_pruned, 0.0)?;
        if point.page_rows_pruned <= 0.0 {
            return Err(format!(
                "custom key: the page index skipped no rows of the retained group: {point:?}"
            )
            .into());
        }

        let slice = run_case(
            cluster,
            reader,
            FilterCase {
                name: "custom event-time slice".to_owned(),
                sql: format!(
                    "SELECT key_id AS id FROM {custom} WHERE {} ORDER BY id",
                    self.slice_filter(2)?
                ),
                expected: custom_keys(2).collect(),
            },
        )
        .await?;
        expect_measure("custom slice", "files", slice.files, 1.0)?;

        for (name, filter, expected) in [
            (
                "custom mixed key/time match",
                format!("label = 'label-04' AND {}", self.slice_filter(1)?),
                custom_where(&|key| {
                    key / CUSTOM_ROWS_PER_FILE == 1
                        && custom_label(key).as_deref() == Some("label-04")
                }),
            ),
            (
                "custom mixed key/time miss",
                format!("key_id = {CUSTOM_PROBE_KEY} AND {}", self.slice_filter(2)?),
                Vec::new(),
            ),
            (
                "custom null label",
                "label IS NULL AND key_id < 200".to_owned(),
                custom_where(&|key| key < 200 && custom_label(key).is_none()),
            ),
            ("custom key miss", "key_id = 999999".to_owned(), Vec::new()),
        ] {
            run_case(
                cluster,
                reader,
                FilterCase {
                    name: name.to_owned(),
                    sql: format!("SELECT key_id AS id FROM {custom} WHERE {filter} ORDER BY id"),
                    expected,
                },
            )
            .await?;
        }

        for (file, request_id) in (0..FILTER_FILES).zip(&probes.request_ids) {
            let present = run_case(
                cluster,
                reader,
                FilterCase {
                    name: format!("wyrd_request_id of file {file}"),
                    sql: format!(
                        "SELECT key_id AS id FROM {custom} \
                         WHERE wyrd_request_id = '{request_id}' ORDER BY id"
                    ),
                    expected: custom_keys(file).collect(),
                },
            )
            .await?;
            expect_measure(
                "request id present",
                "pruned",
                present.row_groups_pruned,
                2.0,
            )?;
            expect_measure("request id present", "bloom", present.bloom_pruned, 0.0)?;
        }
        let absent = run_case(
            cluster,
            reader,
            FilterCase {
                name: "wyrd_request_id absent".to_owned(),
                sql: format!(
                    "SELECT key_id AS id FROM {custom} WHERE wyrd_request_id = '{}' ORDER BY id",
                    uuid::Uuid::now_v7()
                ),
                expected: Vec::new(),
            },
        )
        .await?;
        expect_measure("request id absent", "pruned", absent.row_groups_pruned, 3.0)?;
        expect_measure("request id absent", "bloom", absent.bloom_pruned, 0.0)?;

        self.run_signal_matrix(cluster, reader, probes).await
    }

    /// Runs the spans, records, and points matrices.
    ///
    /// # Errors
    ///
    /// Returns query or telemetry errors, and an error naming the first case
    /// whose rows or physical evidence differ from the expectation.
    async fn run_signal_matrix(
        &self,
        cluster: &WyrdTestCluster,
        reader: &WyrdClient,
        probes: &FilterProbes,
    ) -> Result<(), JourneyError> {
        let signals = |keep: &dyn Fn(i64, i64) -> bool| -> Vec<i64> {
            (0..FILTER_FILES)
                .flat_map(|file| (0..SIGNAL_ROWS_PER_FILE).map(move |row| (file, row)))
                .filter(|(file, row)| keep(*file, *row))
                .map(|(file, row)| signal_id(file, row))
                .collect()
        };
        let trace = |file: i64, ordinal: i64| hex::encode(fixture_trace_id(file, ordinal));

        for (table, id) in [
            ("vala.traces.spans", "start_time_unix_nano"),
            ("vala.logs.records", "time_unix_nano"),
            ("vala.metrics.points", "time_unix_nano"),
        ] {
            let baseline = run_case(
                cluster,
                reader,
                FilterCase {
                    name: format!("{table} unfiltered"),
                    sql: format!("SELECT {id} AS id FROM {table} ORDER BY id"),
                    expected: signals(&|_, _| true),
                },
            )
            .await?;
            expect_measure(table, "files", baseline.files, 3.0)?;
            let slice = run_case(
                cluster,
                reader,
                FilterCase {
                    name: format!("{table} event-time slice"),
                    sql: format!(
                        "SELECT {id} AS id FROM {table} WHERE {} ORDER BY id",
                        self.slice_filter(0)?
                    ),
                    expected: signals(&|file, _| file == 0),
                },
            )
            .await?;
            expect_measure(table, "slice files", slice.files, 1.0)?;
        }

        let absent = run_case(
            cluster,
            reader,
            FilterCase {
                name: "spans absent trace id within bounds".to_owned(),
                sql: format!(
                    "SELECT start_time_unix_nano AS id FROM vala.traces.spans \
                     WHERE trace_id = X'{}' ORDER BY id",
                    hex::encode(probes.span_probe)
                ),
                expected: Vec::new(),
            },
        )
        .await?;
        expect_measure("spans absent trace", "files", absent.files, 3.0)?;
        expect_measure(
            "spans absent trace",
            "pruned",
            absent.row_groups_pruned,
            3.0,
        )?;
        expect_measure("spans absent trace", "bloom", absent.bloom_pruned, 1.0)?;

        for (name, table, id, filter, expected) in [
            (
                "spans trace present",
                "vala.traces.spans",
                "start_time_unix_nano",
                format!("trace_id = X'{}'", trace(1, 2)),
                signals(&|file, row| file == 1 && row == 1),
            ),
            (
                "spans mixed trace/time match",
                "vala.traces.spans",
                "start_time_unix_nano",
                format!(
                    "trace_id = X'{}' AND {}",
                    trace(2, 4),
                    self.slice_filter(2)?
                ),
                signals(&|file, row| file == 2 && row == 2),
            ),
            (
                "spans mixed trace/time miss",
                "vala.traces.spans",
                "start_time_unix_nano",
                format!(
                    "trace_id = X'{}' AND {}",
                    trace(2, 4),
                    self.slice_filter(0)?
                ),
                Vec::new(),
            ),
            (
                "spans root (null parent)",
                "vala.traces.spans",
                "start_time_unix_nano",
                "parent_span_id IS NULL".to_owned(),
                signals(&|_, row| row % 2 == 0),
            ),
            (
                "records trace present",
                "vala.logs.records",
                "time_unix_nano",
                format!("trace_id = X'{}'", trace(0, 2)),
                signals(&|file, row| file == 0 && row == 1),
            ),
            (
                "records severity key",
                "vala.logs.records",
                "time_unix_nano",
                format!("severity_number = {}", record_severity(2)),
                signals(&|_, row| row == 2),
            ),
            (
                "records severity miss",
                "vala.logs.records",
                "time_unix_nano",
                "severity_number = 99".to_owned(),
                Vec::new(),
            ),
            (
                "records uncorrelated in slice",
                "vala.logs.records",
                "time_unix_nano",
                format!("trace_id IS NULL AND {}", self.slice_filter(1)?),
                signals(&|file, row| file == 1 && row % 2 == 0),
            ),
            (
                "points metric present",
                "vala.metrics.points",
                "time_unix_nano",
                format!("metric_name = '{}'", metric_name(1, 3)),
                signals(&|file, row| file == 1 && row == 3),
            ),
            (
                "points metric miss",
                "vala.metrics.points",
                "time_unix_nano",
                "metric_name = 'metric-9-9'".to_owned(),
                Vec::new(),
            ),
            (
                "points null integer",
                "vala.metrics.points",
                "time_unix_nano",
                "int_value IS NULL".to_owned(),
                signals(&|_, row| row % 2 == 1),
            ),
            (
                "points mixed type/time",
                "vala.metrics.points",
                "time_unix_nano",
                format!("metric_type = 'sum' AND {}", self.slice_filter(2)?),
                signals(&|file, row| file == 2 && row % 2 == 0),
            ),
        ] {
            run_case(
                cluster,
                reader,
                FilterCase {
                    name: name.to_owned(),
                    sql: format!("SELECT {id} AS id FROM {table} WHERE {filter} ORDER BY id"),
                    expected,
                },
            )
            .await?;
        }
        Ok(())
    }
}

/// The Bloom-negative and identity probes drawn from the sealed files.
struct FilterProbes {
    /// An unwritten trace id inside file 1's `trace_id` bounds whose Bloom
    /// filter answers absent.
    span_probe: [u8; 16],
    /// An unwritten label inside every custom file's bounds whose Bloom
    /// filters all answer absent.
    label_probe: String,
    /// The one `wyrd_request_id` each custom file carries, in file order.
    request_ids: Vec<String>,
}

/// The physical tier a [`SealedObject`] read draws its objects from.
#[derive(Debug, Clone, Copy)]
enum PhysicalTier {
    /// Scribe hot objects not yet promoted into Iceberg.
    Hot,
    /// Data files of the table's current Iceberg snapshot.
    Iceberg,
}

/// One sealed object read back from storage, with its decoded rows and its
/// footer's column and offset indexes.
struct SealedObject {
    /// Durable `vala.file_list` path.
    path: String,
    /// Complete object bytes.
    bytes: bytes::Bytes,
    /// Footer decoded with page indexes.
    metadata: parquet::file::metadata::ParquetMetaData,
    /// Every row in physical order.
    rows: arrow::record_batch::RecordBatch,
}

impl SealedObject {
    /// Reads every object of one table in `tier`, ordered by its first event
    /// time.
    ///
    /// Hot objects are the table's unpromoted `vala.file_list` rows; Iceberg
    /// objects are the data files the table's current snapshot plans.
    ///
    /// # Errors
    ///
    /// Returns SQL, catalog, scan-planning, storage, or Parquet errors.
    async fn read_tier(
        cluster: &WyrdTestCluster,
        server: &WyrdTestServer,
        tenant: DataTenantId,
        namespace: vala_bifrost_redux::namespaces::BifrostNamespace,
        table: &str,
        tier: PhysicalTier,
    ) -> Result<Vec<Self>, JourneyError> {
        let catalog = server
            .state()
            .bifrost_catalog()
            .ok_or("Scribe composition retains the shared catalog")?
            .iceberg_catalog();
        let binding = vala_bifrost_redux::catalog::TenantTableBinding::resolve((
            tenant,
            vala_bifrost_redux::catalog::TableRef::new(namespace, table),
        ))?;
        let loaded = catalog.load_table(&binding.table_ident()).await?;
        let paths: Vec<String> = match tier {
            PhysicalTier::Hot => {
                sqlx::query_scalar(
                    "SELECT file_path FROM vala.file_list \
                     WHERE data_tenant_id = $1 AND table_name = $2 AND NOT compacted",
                )
                .bind(tenant.as_uuid())
                .bind(table)
                .fetch_all(cluster.pg_fixture().operator_pool().pool())
                .await?
            }
            PhysicalTier::Iceberg => {
                let mut tasks = loaded.scan().select_all().build()?.plan_files().await?;
                let mut paths = Vec::new();
                while let Some(task) = futures_util::TryStreamExt::try_next(&mut tasks).await? {
                    paths.push(task.data_file_path);
                }
                paths
            }
        };
        let mut objects = Vec::with_capacity(paths.len());
        for path in paths {
            let bytes = loaded.file_io().new_input(&path)?.read().await?;
            let metadata = parquet::file::metadata::ParquetMetaDataReader::new()
                .with_page_index_policy(parquet::file::metadata::PageIndexPolicy::Required)
                .parse_and_finish(&bytes)?;
            let decoded = ParquetRecordBatchReaderBuilder::try_new(bytes.clone())?
                .build()?
                .collect::<Result<Vec<_>, _>>()?;
            let schema = decoded
                .first()
                .map(arrow::record_batch::RecordBatch::schema)
                .ok_or_else(|| format!("{path}: the sealed object holds no rows"))?;
            let rows = arrow::compute::concat_batches(&schema, &decoded)?;
            objects.push(Self {
                path,
                bytes,
                metadata,
                rows,
            });
        }
        let mut keyed = objects
            .into_iter()
            .map(|object| object.first_event_time().map(|first| (first, object)))
            .collect::<Result<Vec<_>, _>>()?;
        keyed.sort_by_key(|(first, _)| *first);
        Ok(keyed.into_iter().map(|(_, object)| object).collect())
    }

    /// The earliest `wyrd_event_time` this object holds.
    ///
    /// # Errors
    ///
    /// Returns an error when the column is missing, mistyped, or empty.
    fn first_event_time(&self) -> Result<i64, JourneyError> {
        let column = self
            .rows
            .column_by_name(wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME)
            .ok_or_else(|| format!("{}: no event-time column", self.path))?
            .as_any()
            .downcast_ref::<arrow::array::TimestampMicrosecondArray>()
            .ok_or_else(|| format!("{}: event time is not microseconds", self.path))?;
        arrow::compute::min(column).ok_or_else(|| format!("{}: no event time", self.path).into())
    }

    /// Requires the rows to be in `keys` order.
    ///
    /// Rows are compared through Arrow's row format with each key's direction
    /// and null placement, so ties are allowed and any inversion fails.
    ///
    /// # Errors
    ///
    /// Returns an error naming the first inverted row pair, or the Arrow
    /// error when a key column is missing or cannot be encoded.
    fn expect_sorted(
        &self,
        table: &str,
        keys: &[wyrd_spec::vala::api::SortKeyWire],
    ) -> Result<(), JourneyError> {
        use wyrd_spec::vala::api::{NullOrderWire, SortDirectionWire};

        let mut fields = Vec::with_capacity(keys.len());
        let mut columns = Vec::with_capacity(keys.len());
        for key in keys {
            let column = self
                .rows
                .column_by_name(&key.column)
                .ok_or_else(|| format!("{table}: sort column {} is missing", key.column))?;
            fields.push(arrow::row::SortField::new_with_options(
                column.data_type().clone(),
                arrow::compute::SortOptions {
                    descending: key.direction == SortDirectionWire::Desc,
                    nulls_first: key.null_order == NullOrderWire::First,
                },
            ));
            columns.push(std::sync::Arc::clone(column));
        }
        let encoded = arrow::row::RowConverter::new(fields)?.convert_columns(&columns)?;
        for index in 1..encoded.num_rows() {
            if encoded.row(index - 1) > encoded.row(index) {
                return Err(format!(
                    "{table}: {} is not in its declared order at row {index}",
                    self.path
                )
                .into());
            }
        }
        Ok(())
    }

    /// Requires a Bloom filter on every declared column and none on
    /// `undeclared`.
    ///
    /// # Errors
    ///
    /// Returns an error naming the first column whose filter presence differs.
    fn expect_bloom_columns(
        &self,
        table: &str,
        declared: &[String],
        undeclared: &str,
    ) -> Result<(), JourneyError> {
        let group = self.metadata.row_group(0);
        for column in declared {
            let Ok(index) = self.column_index(column) else {
                continue;
            };
            if group.column(index).bloom_filter_offset().is_none() {
                return Err(format!(
                    "{table}: {} has no Bloom filter on declared {column}",
                    self.path
                )
                .into());
            }
        }
        if group
            .column(self.column_index(undeclared)?)
            .bloom_filter_offset()
            .is_some()
        {
            return Err(format!(
                "{table}: {} carries a Bloom filter on undeclared {undeclared}",
                self.path
            )
            .into());
        }
        Ok(())
    }

    /// The leaf index of top-level column `name`.
    ///
    /// # Errors
    ///
    /// Returns an error when the object has no such leaf.
    fn column_index(&self, name: &str) -> Result<usize, JourneyError> {
        self.metadata
            .file_metadata()
            .schema_descr()
            .columns()
            .iter()
            .position(|column| column.path().string() == name)
            .ok_or_else(|| format!("{}: no column {name}", self.path).into())
    }

    /// Whether row group 0's Bloom filter on `column` may contain `value`.
    ///
    /// # Errors
    ///
    /// Returns an error when the object does not decode or carries no filter
    /// on the column.
    fn may_contain(&self, column: &str, value: &[u8]) -> Result<bool, JourneyError> {
        use parquet::file::reader::FileReader;

        let reader = parquet::file::serialized_reader::SerializedFileReader::new_with_options(
            self.bytes.clone(),
            parquet::file::serialized_reader::ReadOptionsBuilder::new()
                .with_reader_properties(
                    parquet::file::properties::ReaderProperties::builder()
                        .set_read_bloom_filter(true)
                        .build(),
                )
                .build(),
        )?;
        let group = reader.get_row_group(0)?;
        let filter = group
            .get_column_bloom_filter(self.column_index(column)?)
            .ok_or_else(|| format!("{}: no Bloom filter on {column}", self.path))?;
        Ok(filter.check(value))
    }

    /// The number of pages row group 0 stores for `column`.
    ///
    /// # Errors
    ///
    /// Returns an error when the object carries no offset index.
    fn page_count(&self, column: &str) -> Result<usize, JourneyError> {
        let index = self.column_index(column)?;
        Ok(self
            .metadata
            .offset_index()
            .and_then(|groups| groups.first())
            .and_then(|columns| columns.get(index))
            .ok_or_else(|| format!("{}: no offset index", self.path))?
            .page_locations()
            .len())
    }

    /// The single `wyrd_request_id` every row of this object carries.
    ///
    /// # Errors
    ///
    /// Returns an error when the column is missing or holds several values.
    fn request_id(&self) -> Result<String, JourneyError> {
        let column = self
            .rows
            .column_by_name("wyrd_request_id")
            .ok_or_else(|| format!("{}: no request id column", self.path))?
            .as_any()
            .downcast_ref::<arrow::array::StringArray>()
            .ok_or_else(|| format!("{}: request id is not UTF-8", self.path))?;
        let distinct = column
            .iter()
            .flatten()
            .collect::<std::collections::BTreeSet<_>>();
        match distinct.into_iter().collect::<Vec<_>>().as_slice() {
            [only] => Ok((*only).to_owned()),
            many => {
                Err(format!("{}: expected one request id, saw {}", self.path, many.len()).into())
            }
        }
    }
}

/// Runs one filtering case and returns the hot reader's evidence for it.
///
/// # Errors
///
/// Returns telemetry or query errors, and an error when the returned ids are
/// not exactly the expected ids.
async fn run_case(
    cluster: &WyrdTestCluster,
    reader: &WyrdClient,
    case: FilterCase,
) -> Result<ScanEvidence, JourneyError> {
    let checkpoint = cluster
        .telemetry()
        .checkpoint()
        .map_err(|error| error.to_string())?;
    let ids = query_ids(reader, case.sql.clone()).await?;
    let delta = cluster
        .telemetry()
        .delta_since(&checkpoint)
        .map_err(|error| error.to_string())?;
    if ids != case.expected {
        return Err(format!(
            "{}: returned {} ids, expected {}; first returned {:?}, first expected {:?}; sql: {}",
            case.name,
            ids.len(),
            case.expected.len(),
            ids.iter().take(8).collect::<Vec<_>>(),
            case.expected.iter().take(8).collect::<Vec<_>>(),
            case.sql
        )
        .into());
    }
    let evidence = ScanEvidence {
        files: sum_metric(&delta, "oracle_query_files_scanned_total"),
        row_groups_pruned: sum_metric(&delta, "oracle_query_row_groups_pruned_total"),
        bloom_pruned: sum_metric(&delta, "oracle_query_row_groups_pruned_bloom_total"),
        page_rows_pruned: sum_metric(&delta, "oracle_query_rows_pruned_page_index_total"),
    };
    eprintln!(
        "filtering case `{}`: rows={} {evidence:?}",
        case.name,
        ids.len()
    );
    Ok(evidence)
}

/// Requires one evidence measure to equal its expected value exactly.
///
/// # Errors
///
/// Returns an error naming the case, the measure, and both values.
fn expect_measure(
    case: &str,
    measure: &str,
    actual: f64,
    expected: f64,
) -> Result<(), JourneyError> {
    if (actual - expected).abs() > f64::EPSILON {
        return Err(format!("{case}: {measure} expected {expected}, saw {actual}").into());
    }
    Ok(())
}

/// The custom dataset's user schema: a sort key, a nullable Bloom key, and a
/// value column with neither.
fn custom_schema() -> arrow::datatypes::SchemaRef {
    std::sync::Arc::new(arrow::datatypes::Schema::new(vec![
        arrow::datatypes::Field::new("key_id", arrow::datatypes::DataType::Int64, false),
        arrow::datatypes::Field::new(CUSTOM_BLOOM_COLUMN, arrow::datatypes::DataType::Utf8, true),
        arrow::datatypes::Field::new("score", arrow::datatypes::DataType::Float64, false),
    ]))
}

/// The ascending custom keys file `file` holds; files own disjoint ranges.
fn custom_keys(file: i64) -> std::ops::Range<i64> {
    file * CUSTOM_ROWS_PER_FILE..(file + 1) * CUSTOM_ROWS_PER_FILE
}

/// The label of custom `key`: null on every tenth key, otherwise one of
/// eight even-numbered labels, so odd-numbered labels lie inside every
/// file's bounds without ever being written.
fn custom_label(key: i64) -> Option<String> {
    (key % 10 != 0).then(|| format!("label-{:02}", 2 * (key % 8)))
}

/// The row identity of signal row `row` of `file`.
fn signal_id(file: i64, row: i64) -> i64 {
    file * 100 + row + 1
}

/// A fixture trace id whose last two bytes are `file` and `ordinal`.
///
/// Written rows use even ordinals, so an odd ordinal below a file's largest
/// written one is unwritten yet inside that file's `trace_id` bounds.
fn fixture_trace_id(file: i64, ordinal: i64) -> [u8; 16] {
    let mut id = [0xf1; 16];
    id[14] = u8::try_from(file).unwrap_or(u8::MAX);
    id[15] = u8::try_from(ordinal).unwrap_or(u8::MAX);
    id
}

/// A fixture span id unique to `(file, row)`.
fn fixture_span_id(file: i64, row: i64) -> [u8; 8] {
    let mut id = [0x5a; 8];
    id[6] = u8::try_from(file).unwrap_or(u8::MAX);
    id[7] = u8::try_from(row).unwrap_or(u8::MAX);
    id
}

/// The severity of log row `row`.
fn record_severity(row: i64) -> i32 {
    9 + i32::try_from(row).unwrap_or(0)
}

/// The metric name of point row `row` of `file`.
fn metric_name(file: i64, row: i64) -> String {
    format!("metric-{file}-{row}")
}

/// Formats `micros` as a SQL `TIMESTAMP` literal body in UTC.
///
/// # Errors
///
/// Returns an error when `micros` is not a representable instant.
fn sql_timestamp(micros: i64) -> Result<String, JourneyError> {
    Ok(
        chrono::DateTime::<chrono::Utc>::from_timestamp_micros(micros)
            .ok_or("fixture timestamp is out of range")?
            .format("%Y-%m-%d %H:%M:%S%.6f")
            .to_string(),
    )
}

/// Appends a caller-supplied `wyrd_event_time` to `batch`.
///
/// # Errors
///
/// Returns the Arrow error when the column count differs from the rows.
fn with_event_time(
    batch: &arrow::record_batch::RecordBatch,
    micros: Vec<i64>,
) -> Result<arrow::record_batch::RecordBatch, JourneyError> {
    let mut fields = batch.schema().fields().iter().cloned().collect::<Vec<_>>();
    fields.push(std::sync::Arc::new(arrow::datatypes::Field::new(
        wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME,
        arrow::datatypes::DataType::Timestamp(
            arrow::datatypes::TimeUnit::Microsecond,
            Some("UTC".into()),
        ),
        false,
    )));
    let mut columns = batch.columns().to_vec();
    columns.push(std::sync::Arc::new(
        arrow::array::TimestampMicrosecondArray::from(micros).with_timezone("UTC"),
    ));
    Ok(arrow::record_batch::RecordBatch::try_new(
        std::sync::Arc::new(arrow::datatypes::Schema::new(fields)),
        columns,
    )?)
}

/// Rows the binary-key dataset seals: above the 20,000-row page limit, so
/// its one row group spans several pages of the binary sort key.
const BINARY_KEY_ROWS: i64 = 45_000;

/// The binary-key row the page probes look up.
const BINARY_PROBE_ROW: i64 = 30_000;

/// A custom dataset sorted by a fixed-size binary key keeps exact results
/// and selects pages from both binary key columns once Forge promotes it.
///
/// Every key is non-UTF-8 (its leading bytes are `0xff`/`0xfd`), so a reader
/// that decoded binary page bounds as text would fail or guess. The promoted
/// file's one row group spans several pages of the sorted key, so the
/// Iceberg reader's page index is the only mechanism that can skip rows of a
/// point lookup: row-group statistics keep the group, and there is no other
/// file. Equality on the fixed-size key, on the variable-length key, and the
/// fixed-size key combined with a selective event-time bound each return the
/// one written row while skipping rows by the page index; an absent key
/// returns nothing.
///
/// # Errors
///
/// Returns cluster, registration, write, promotion, storage, telemetry, or
/// query errors, or a description of the first expectation that does not
/// hold.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn binary_sort_key_page_pruning() -> Result<(), JourneyError> {
    use vala_bifrost_redux::namespaces::BifrostNamespace;
    use wyrd_spec::vala::api::{
        NullOrderWire, PhysicalLayoutWire, SortDirectionWire, SortKeyWire, TimeGranularityWire,
    };

    let cluster = WyrdTestCluster::start_spec_with_forge_config_and_completion_observer(
        BifrostClusterSpec::one_mixed(),
        ForgeConfig::default(),
        false,
        false,
    )
    .await?;
    let server = cluster.server(0).ok_or("missing node")?;
    let tenant = cluster.data_tenant_id();
    let table = unique_table("oracle_binary_key");
    let fqn = format!("vala.datasets.{table}");
    let rows = writer(server, "binary-key-writer").await?;
    let sort_keys = vec![SortKeyWire {
        column: "fixed_key".to_owned(),
        direction: SortDirectionWire::Asc,
        null_order: NullOrderWire::Last,
    }];
    let config = wyrd_client::bifrost::TableConfig::from_arrow(&fqn, binary_key_schema())?
        .with_layout(PhysicalLayoutWire {
            partition_granularity: TimeGranularityWire::Hour,
            sort_keys: sort_keys.clone(),
            bloom_columns: Vec::new(),
        });
    wyrd_client::Bifrost::connect_with_table(rows.client(), config)
        .await?
        .register()
        .await?;

    let hour =
        (chrono::Utc::now().timestamp_micros() / FILTER_HOUR_MICROS - 2) * FILTER_HOUR_MICROS;
    let event_time = |row: i64| hour + row * 10_000;
    let written: Vec<i64> = (0..BINARY_KEY_ROWS).rev().collect();
    let batch = arrow::record_batch::RecordBatch::try_new(
        binary_key_schema(),
        vec![
            std::sync::Arc::new(arrow::array::Int64Array::from(written.clone())),
            std::sync::Arc::new(arrow::array::FixedSizeBinaryArray::try_from_iter(
                written.iter().map(|row| fixed_binary_key(*row)),
            )?),
            std::sync::Arc::new(arrow::array::BinaryArray::from_iter_values(
                written.iter().map(|row| variable_binary_key(*row)),
            )),
        ],
    )?;
    rows.write_batch(
        &fqn,
        &with_event_time(&batch, written.iter().map(|row| event_time(*row)).collect())?,
    )
    .await?;
    server.flush_bifrost().await?;
    compact_sealed_batch(&cluster, tenant, &table, 1).await?;
    cluster.refresh_oracle_snapshots().await?;

    let promoted = SealedObject::read_tier(
        &cluster,
        server,
        tenant,
        BifrostNamespace::Datasets,
        &table,
        PhysicalTier::Iceberg,
    )
    .await?;
    let [object] = promoted.as_slice() else {
        return Err(format!(
            "the promoted snapshot must hold exactly one data file, saw {}",
            promoted.len()
        )
        .into());
    };
    object.expect_sorted(&fqn, &sort_keys)?;
    for column in ["fixed_key", "variable_key"] {
        let pages = object.page_count(column)?;
        if pages < 2 {
            return Err(format!(
                "{column}: the promoted file must carry several pages, saw {pages}"
            )
            .into());
        }
    }
    if object.metadata.num_row_groups() != 1 {
        return Err("the promoted file must hold one row group".into());
    }

    let reader = client(server, "binary-key-reader").await?;
    let probe_fixed = hex::encode(fixed_binary_key(BINARY_PROBE_ROW));
    let probe_variable = hex::encode(variable_binary_key(BINARY_PROBE_ROW));
    for (name, filter) in [
        ("fixed-size key", format!("fixed_key = X'{probe_fixed}'")),
        (
            "variable-length key",
            format!("variable_key = X'{probe_variable}'"),
        ),
        (
            "fixed-size key and time",
            format!(
                "fixed_key = X'{probe_fixed}' AND wyrd_event_time >= TIMESTAMP '{}'",
                sql_timestamp(event_time(BINARY_PROBE_ROW))?
            ),
        ),
    ] {
        let evidence = run_case(
            &cluster,
            &reader,
            FilterCase {
                name: format!("promoted {name}"),
                sql: format!("SELECT row_id AS id FROM {fqn} WHERE {filter} ORDER BY id"),
                expected: vec![BINARY_PROBE_ROW],
            },
        )
        .await?;
        if evidence.page_rows_pruned <= 0.0 {
            return Err(format!(
                "promoted {name}: the Iceberg page index skipped no rows: {evidence:?}"
            )
            .into());
        }
    }
    run_case(
        &cluster,
        &reader,
        FilterCase {
            name: "promoted absent binary key".to_owned(),
            sql: format!(
                "SELECT row_id AS id FROM {fqn} WHERE fixed_key = X'{}' ORDER BY id",
                hex::encode(fixed_binary_key(BINARY_KEY_ROWS + 1))
            ),
            expected: Vec::new(),
        },
    )
    .await?;
    cluster.shutdown().await?;
    Ok(())
}

/// The binary-key dataset's user schema: a row identity, a fixed-size binary
/// sort key, and a variable-length binary key ordered the same way.
fn binary_key_schema() -> arrow::datatypes::SchemaRef {
    std::sync::Arc::new(arrow::datatypes::Schema::new(vec![
        arrow::datatypes::Field::new("row_id", arrow::datatypes::DataType::Int64, false),
        arrow::datatypes::Field::new(
            "fixed_key",
            arrow::datatypes::DataType::FixedSizeBinary(16),
            false,
        ),
        arrow::datatypes::Field::new("variable_key", arrow::datatypes::DataType::Binary, false),
    ]))
}

/// The non-UTF-8 fixed-size key of `row`, ascending with `row`.
fn fixed_binary_key(row: i64) -> [u8; 16] {
    let mut key = [0xff; 16];
    key[8..].copy_from_slice(&row.to_be_bytes());
    key
}

/// The non-UTF-8 variable-length key of `row`, ascending with `row`.
fn variable_binary_key(row: i64) -> Vec<u8> {
    let mut key = vec![0xfd];
    key.extend_from_slice(&row.to_be_bytes());
    key
}
