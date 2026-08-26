//! Oracle journeys — Distributed follower dispatch: fragment assignment, heterogeneous
//! topologies, signed selective closures, settlement recovery, and replanning
//! through the boot directory.
//!
//! Module of the `oracle` binary; see `oracle.rs` for the capability it
//! proves and `support.rs` for the fixtures it shares.

use arrow::array::{Array, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use std::sync::Arc;
use vala_sdk::{BifrostGrpcTransport, QueryClient, ValaSdkError};
use wyrd_client::WyrdClient;
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{
    BifrostQueryRequest, FreshnessPolicy, QueryTerminalErrorCode, QueryTerminalOutcome,
    QueryWarning, VisibilityMode,
};
use wyrd_spec::vala::error::BifrostError;
use wyrd_testing::bifrost::{BifrostClusterSpec, WyrdTestCluster};

use crate::support::*;

/// J4 enters the final mixed node so its frozen membership includes remote workers.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_distributed_journey() {
    public_roundtrip(
        BifrostClusterSpec::three_mixed(),
        VisibilityMode::PublishedOnly,
        true,
        2,
        Some("oracle_query_rows_total"),
        true,
    )
    .await
    .expect("J4 distributed journey");
}

/// Proves the native physical-plan cut executes persisted and live subtrees on
/// distinct remote role owners before the leader applies the final operators.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_heterogeneous_distributed_query_journey() {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::three_mixed())
        .await
        .expect("distributed physical cluster");
    let leader = cluster.server(0).expect("query leader");
    let writer = cluster.server(2).expect("remote Scribe writer");
    let leader_id = cluster.configured_node_ids()[0];
    let table = unique_table("oracle_heterogeneous_physical");
    register_table(writer, cluster.data_tenant_id(), &table)
        .await
        .expect("distributed table");
    let writer_client = client(writer, "heterogeneous-physical-writer")
        .await
        .expect("writer client");
    ingest(&writer_client, &format!("vala.bifrost.{table}"), &[1, 2, 3])
        .await
        .expect("persisted rows");
    writer.flush_bifrost().await.expect("persisted flush");
    ingest(&writer_client, &format!("vala.bifrost.{table}"), &[4, 5])
        .await
        .expect("live rows");
    let day = current_hour_partition();
    cluster
        .observe_live_tail(&format!("vala.bifrost.{table}"), day)
        .await
        .expect("live-tail discovery");
    cluster
        .refresh_oracle_snapshots()
        .await
        .expect("immutable membership input");

    let before = cluster
        .servers()
        .map(|server| {
            let inspection = server
                .state()
                .oracle_peer()
                .expect("mixed node peer")
                .worker()
                .physical_inspection();
            let scribe = server
                .state()
                .bifrost
                .scribe()
                .map(|scribe| scribe.fragment_inspection())
                .unwrap_or_default();
            (inspection.node_id, (inspection, scribe))
        })
        .collect::<std::collections::HashMap<_, _>>();
    let probe = Arc::new(vala_bifrost_redux::oracle::OracleTopologyProbe::default());
    leader
        .state()
        .bifrost_query()
        .expect("leader query runtime")
        .oracle()
        .bind_topology_probe_for_test(Arc::clone(&probe));
    let reader = client(leader, "heterogeneous-physical-reader")
        .await
        .expect("reader client");
    let sql = format!(
        "SELECT value, COUNT(*) AS total FROM vala.bifrost.{table} GROUP BY value ORDER BY total DESC LIMIT 1"
    );
    // Classification and execution each used to pin the catalog, so every query
    // paid two round trips for one file list. A locally led query must now pin
    // its single table exactly once.
    vala_bifrost_redux::catalog::reset_sealed_pin_count_for_test();
    let mut query = tokio::spawn(async move {
        let mut stream = QueryClient::new(&reader)
            .query(&BifrostQueryRequest {
                sql,
                visibility: VisibilityMode::Fused,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(20_000),
            })
            .await?;
        let mut total = None;
        while let Some(batch) = stream.next_batch().await? {
            if batch.num_rows() > 0 {
                total = Some(
                    batch
                        .column(1)
                        .as_any()
                        .downcast_ref::<Int64Array>()
                        .ok_or("aggregate column is not Int64")?
                        .value(0),
                );
            }
        }
        Ok::<_, JourneyError>((total, stream.terminal().cloned()))
    });
    tokio::select! {
        () = probe.wait_selected() => {}
        result = &mut query => panic!("query ended before registry-before-dispatch seam: {result:?}"),
    }
    let registry = leader
        .state()
        .bifrost_query()
        .expect("leader query runtime")
        .running_queries();
    let summaries = registry.list(cluster.data_tenant_id());
    assert_eq!(
        summaries.len(),
        1,
        "registry insertion must precede dispatch"
    );
    let entry = registry
        .get(cluster.data_tenant_id(), &summaries[0].request_id)
        .expect("immutable running entry");
    assert!(entry.participant_cut().oracles().len() >= 2);
    assert!(entry.participant_cut().scribes().len() >= 2);
    let pinned_scribe_nodes = entry
        .participant_cut()
        .scribes()
        .iter()
        .map(|participant| participant.node_id)
        .collect::<std::collections::BTreeSet<_>>();
    let cut_fingerprint = entry.participant_cut().fingerprint();
    assert!(!cut_fingerprint.is_empty());
    probe.resume();
    let (total, terminal) = query
        .await
        .expect("distributed query joins")
        .expect("distributed query succeeds");
    assert_eq!(total, Some(5), "leader final aggregate/order/limit");
    assert_eq!(
        vala_bifrost_redux::catalog::sealed_pin_count_for_test(),
        1,
        "a locally led query must pin its single table exactly once, not once to          classify and again to execute"
    );
    assert_eq!(
        terminal.expect("distributed terminal").outcome,
        QueryTerminalOutcome::Success
    );

    let after = cluster
        .servers()
        .map(|server| {
            let inspection = server
                .state()
                .oracle_peer()
                .expect("mixed node peer")
                .worker()
                .physical_inspection();
            let scribe = server
                .state()
                .bifrost
                .scribe()
                .map(|scribe| scribe.fragment_inspection())
                .unwrap_or_default();
            (inspection.node_id, (inspection, scribe))
        })
        .collect::<std::collections::HashMap<_, _>>();
    let oracle_nodes = after
        .iter()
        .filter_map(|(node, (current, _))| {
            (current.oracle_executions > before[node].0.oracle_executions).then_some(*node)
        })
        .collect::<Vec<_>>();
    assert_eq!(oracle_nodes.len(), 1, "one Oracle subtree owner");
    assert_ne!(
        oracle_nodes[0], leader_id,
        "persisted subtree must be remote"
    );
    let scribe_nodes = after
        .iter()
        .filter_map(|(node, (_, current))| (current.0 > before[node].1.0).then_some(*node))
        .collect::<Vec<_>>();
    assert_eq!(
        scribe_nodes
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>(),
        pinned_scribe_nodes,
        "every pinned Scribe must execute its explicit-empty persisted hot cut"
    );
    assert!(
        scribe_nodes.iter().any(|node| *node != oracle_nodes[0]),
        "persisted and live subtrees include distinct role owners"
    );
    let fragment_delta = after
        .iter()
        .map(|(node, (current, scribe))| {
            current.oracle_executions - before[node].0.oracle_executions + scribe.0
                - before[node].1.0
        })
        .sum::<u64>();
    let footer_delta = after
        .iter()
        .map(|(node, (current, scribe))| {
            current.footers_emitted - before[node].0.footers_emitted + scribe.1 - before[node].1.1
        })
        .sum::<u64>();
    assert!(fragment_delta >= 2, "both remote role subtrees execute");
    assert_eq!(
        footer_delta, fragment_delta,
        "every executed remote fragment emits one authenticated footer"
    );
    assert!(registry.list(cluster.data_tenant_id()).is_empty());
    let inspection = cluster.oracle_inspection().await.expect("clean settlement");
    assert_eq!(inspection.active_queries, 0);
    assert_eq!(inspection.queued_queries, 0);
    assert_eq!(inspection.reserved_memory_bytes, 0);
    assert_eq!(inspection.reserved_spill_bytes, 0);
    assert_eq!(inspection.peer_pending, 0);
    assert_eq!(inspection.peer_running, 0);
    assert_eq!(
        leader
            .cancel_and_observe_shared_bifrost_shutdown_for_test()
            .expect("mixed pod shares one Bifrost process token"),
        (true, true),
        "one composed process cancellation must reach both Scribe and Oracle"
    );
    cluster
        .shutdown()
        .await
        .expect("distributed cluster shutdown");
}

/// Sums the rows every peer worker in the cluster handed to attempt encoding.
///
/// Followers count rows as they enter the attempt encoder, so this is the
/// row volume that actually crosses the distributed wire — the quantity a
/// source-applied signed predicate is supposed to reduce.
fn cluster_rows_encoded(cluster: &WyrdTestCluster) -> u64 {
    cluster
        .servers()
        .filter_map(|server| {
            Some(
                server
                    .state()
                    .oracle_peer()?
                    .worker()
                    .physical_inspection()
                    .rows_encoded,
            )
        })
        .sum()
}

/// Runs one fused read-only statement and returns its rows plus terminal.
///
/// # Errors
/// Returns a journey error when the query cannot be issued, a frame fails to
/// decode, or the stream ends without a terminal.
async fn fused_query_rows(
    client: &WyrdClient,
    sql: String,
) -> Result<(u64, QueryTerminalOutcome), JourneyError> {
    let mut stream = QueryClient::new(client)
        .query(&BifrostQueryRequest {
            sql,
            visibility: VisibilityMode::Fused,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: Some(20_000),
        })
        .await?;
    let mut rows = 0_u64;
    while let Some(batch) = stream.next_batch().await? {
        rows = rows.saturating_add(u64::try_from(batch.num_rows())?);
    }
    let terminal = stream.terminal().ok_or("fused query terminal missing")?;
    if terminal.row_count != rows {
        return Err("terminal row count differs from Arrow frames".into());
    }
    Ok((rows, terminal.outcome))
}

/// A Scribe assignment now carries the same closed predicates as every other
/// assignment, and the live-tail fetch applies them before returning batches.
///
/// The proof is a parity comparison against the same tail-backed data: a
/// selective fused query returns exactly the rows the predicate admits, while
/// sending strictly fewer rows into follower attempt encoding than the
/// unfiltered query over the identical live tail.
///
/// # Panics
/// Panics when the cluster, table registration, ingest, live-tail discovery,
/// or either query violates its journey invariant.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_selective_tail_backed_distributed_parity() {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::three_mixed())
        .await
        .expect("tail parity cluster");
    let tenant = cluster.data_tenant_id();
    let writer = cluster.server(2).expect("remote Scribe writer");
    let table = unique_table("oracle_tail_parity");
    register_table(writer, tenant, &table)
        .await
        .expect("tail parity table");
    let table_fqn = format!("vala.bifrost.{table}");
    let writer_client = client(writer, "tail-parity-writer")
        .await
        .expect("writer client");
    // Every row stays in the live tail: no flush runs, so the whole result
    // must be served by Scribe follower assignments.
    for (id, value) in [
        (1_i64, "alpha"),
        (2_i64, "target"),
        (3_i64, "zulu"),
        (4_i64, "omega"),
    ] {
        ingest_marked(&writer_client, &table_fqn, id, value)
            .await
            .expect("live tail row");
    }
    let day = current_hour_partition();
    cluster
        .observe_live_tail(&table_fqn, day)
        .await
        .expect("live-tail discovery");
    cluster
        .refresh_oracle_snapshots()
        .await
        .expect("immutable membership input");

    let leader = cluster.server(0).expect("query leader");
    let reader = client(leader, "tail-parity-reader")
        .await
        .expect("reader client");

    let before_unfiltered = cluster_rows_encoded(&cluster);
    let (unfiltered_rows, unfiltered_outcome) = fused_query_rows(
        &reader,
        format!("SELECT id, value FROM {table_fqn} ORDER BY id"),
    )
    .await
    .expect("unfiltered tail query");
    let unfiltered_encoded = cluster_rows_encoded(&cluster) - before_unfiltered;
    assert_eq!(unfiltered_outcome, QueryTerminalOutcome::Success);
    assert_eq!(unfiltered_rows, 4, "the whole live tail is visible");
    assert!(
        unfiltered_encoded >= 4,
        "an unfiltered tail-backed query must encode at least the tail it returns, \
         saw {unfiltered_encoded}"
    );

    let before_selective = cluster_rows_encoded(&cluster);
    let (selective_rows, selective_outcome) = fused_query_rows(
        &reader,
        format!("SELECT id, value FROM {table_fqn} WHERE value = 'target' ORDER BY id"),
    )
    .await
    .expect("selective tail query");
    let selective_encoded = cluster_rows_encoded(&cluster) - before_selective;
    assert_eq!(selective_outcome, QueryTerminalOutcome::Success);
    assert_eq!(
        selective_rows, 1,
        "the signed predicate admits exactly the matching tail row"
    );
    assert!(
        selective_encoded < unfiltered_encoded,
        "a signed predicate must be applied inside the live-tail fetch: \
         selective={selective_encoded} unfiltered={unfiltered_encoded}"
    );

    let inspection = cluster
        .oracle_inspection()
        .await
        .expect("clean tail parity settlement");
    assert_eq!(inspection.active_queries, 0);
    assert_eq!(inspection.reserved_memory_bytes, 0);
    cluster.shutdown().await.expect("tail parity shutdown");
}

/// S3 proves a selective predicate prunes physical local and distributed
/// Oracle reads while preserving exact residual rows, and that the tenant
/// tripwire still fails closed once closed predicate/projection pushdown is
/// in effect.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_selective_predicate_prunes_distributed_reads() {
    prove_selective_predicate_pruning(BifrostClusterSpec::one_mixed(), 0)
        .await
        .expect("S3 local pruning journey");
    prove_selective_predicate_pruning(BifrostClusterSpec::three_mixed(), 2)
        .await
        .expect("S3 distributed pruning journey");
}

/// Drives one topology through a three-file selective-predicate fixture,
/// proving strictly fewer scanned files and bytes than an unfiltered scan,
/// identical residual-filtered rows, and a fail-closed tenant tripwire.
///
/// # Errors
///
/// Returns a client, telemetry, or cluster-lifecycle error surfaced by any
/// journey step.
async fn prove_selective_predicate_pruning(
    spec: BifrostClusterSpec,
    query_index: usize,
) -> Result<(), JourneyError> {
    let cluster = WyrdTestCluster::start_spec(spec).await?;
    let ingest_server = cluster
        .servers()
        .find(|server| server.bifrost_scribe().is_some())
        .ok_or("missing S3 ingest node")?;
    let tenant = cluster.data_tenant_id();
    let table = unique_table("oracle_s3_predicate");
    register_table(ingest_server, tenant, &table).await?;
    let writer = client(ingest_server, "s3-predicate-writer").await?;
    for (id, value) in [(1_i64, "alpha"), (2_i64, "target"), (3_i64, "zulu")] {
        ingest_marked(&writer, &format!("vala.bifrost.{table}"), id, value).await?;
        ingest_server.flush_bifrost().await?;
    }
    cluster.refresh_oracle_snapshots().await?;

    let query_server = cluster.server(query_index).ok_or("missing S3 query node")?;
    let reader = client(query_server, "s3-predicate-reader").await?;
    let table_fqn = format!("vala.bifrost.{table}");

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
    let (selective_rows, selective_outcome, selective_error) = query_statement(
        &reader,
        format!("SELECT id, value FROM {table_fqn} WHERE value = 'target' ORDER BY id"),
    )
    .await?;
    if selective_rows != 1 {
        return Err(format!(
            "selective predicate expected exactly one residual row, saw {selective_rows}"
        )
        .into());
    }
    if selective_outcome != QueryTerminalOutcome::Success || selective_error.is_some() {
        return Err(format!(
            "selective predicate query did not succeed: {selective_outcome:?} {selective_error:?}"
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
    // Pruning is accepted at either physical granularity: whole files
    // dropped by manifest/statistics exclusion, or row groups dropped inside a
    // retained file. Which one moves depends on how the fixture's three
    // published files were laid out, so requiring both would assert a fixture
    // detail rather than the pruning contract.
    if !(selective_files < unfiltered_files || selective_row_groups < unfiltered_row_groups) {
        return Err(format!(
            "selective query must select strictly fewer files or row groups: \
             files selective={selective_files} unfiltered={unfiltered_files}; \
             row groups selective={selective_row_groups} unfiltered={unfiltered_row_groups}; \
             bytes selective={selective_bytes} unfiltered={unfiltered_bytes}"
        )
        .into());
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

    // Tripwire: a physically scanned foreign-tenant row must refuse the
    // query with the tenant-isolation reason intact and deliver no rows.
    //
    // Asserted through `query_terminal_either_surface` because the refusal may
    // land on the pre-byte lookahead (early typed error) or after the first
    // batch (terminal frame) depending on fixture layout.
    // `QueryTenantInvariant` specifically — not merely "some failure" — is the
    // assertion that regresses if the closed predicate/projection path ever
    // loses the reason across the follower dispatch boundary.
    let foreign_tenant = cluster.add_tenant("oracle-s3-foreign").await?;
    seed_foreign_hot_row(&cluster, tenant, &table, foreign_tenant, "s3-foreign").await?;
    let (tripwire_rows, tripwire_outcome, tripwire_error) = query_terminal_either_surface(
        &reader,
        format!("SELECT count(*) AS total FROM {table_fqn}"),
    )
    .await?;
    if tripwire_rows != 0 {
        return Err("foreign row reached a SQL operator under predicate pushdown".into());
    }
    if tripwire_outcome != QueryTerminalOutcome::Failed
        || tripwire_error != Some(QueryTerminalErrorCode::QueryTenantInvariant)
    {
        return Err(format!(
            "tenant tripwire did not fail closed: {tripwire_outcome:?} {tripwire_error:?}"
        )
        .into());
    }

    let inspection = cluster.oracle_inspection().await?;
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

/// Proves cancellation at the registry-before-dispatch seam settles every
/// admitted owner without replaying the immutable physical assignment.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_distributed_recovery_and_settlement_journey() {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::three_mixed())
        .await
        .expect("distributed cancellation cluster");
    let leader = cluster.server(0).expect("query leader");
    let writer = cluster.server(2).expect("remote writer");
    let table = unique_table("oracle_distributed_cancel");
    register_table(writer, cluster.data_tenant_id(), &table)
        .await
        .expect("cancellation table");
    let writer_client = client(writer, "distributed-cancel-writer")
        .await
        .expect("writer client");
    ingest(&writer_client, &format!("vala.bifrost.{table}"), &[1, 2, 3])
        .await
        .expect("cancellation rows");
    writer.flush_bifrost().await.expect("cancellation flush");
    cluster
        .refresh_oracle_snapshots()
        .await
        .expect("cancellation membership");
    let probe = Arc::new(vala_bifrost_redux::oracle::OracleTopologyProbe::default());
    let runtime = leader
        .state()
        .bifrost_query()
        .expect("leader query runtime");
    runtime
        .oracle()
        .bind_topology_probe_for_test(Arc::clone(&probe));
    let registry = Arc::clone(runtime.running_queries());
    let reader = client(leader, "distributed-cancel-reader")
        .await
        .expect("reader client");
    let query = tokio::spawn(async move {
        QueryClient::new(&reader)
            .query(&BifrostQueryRequest {
                sql: format!("SELECT COUNT(*) FROM vala.bifrost.{table}"),
                visibility: VisibilityMode::PublishedOnly,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(20_000),
            })
            .await
    });
    probe.wait_selected().await;
    let summaries = registry.list(cluster.data_tenant_id());
    assert_eq!(summaries.len(), 1, "cancel sees exact admitted owner");
    let cancellation = registry
        .cancel(cluster.data_tenant_id(), &summaries[0].request_id)
        .expect("registry cancellation");
    assert!(cancellation.cancellation_started);
    probe.resume();
    let _ = query.await.expect("cancelled query joins");
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    while !registry.list(cluster.data_tenant_id()).is_empty()
        && tokio::time::Instant::now() < deadline
    {
        tokio::task::yield_now().await;
    }
    assert!(registry.list(cluster.data_tenant_id()).is_empty());
    let inspection = cluster.oracle_inspection().await.expect("cancel cleanup");
    assert_eq!(inspection.active_queries, 0);
    assert_eq!(inspection.queued_queries, 0);
    assert_eq!(inspection.reserved_memory_bytes, 0);
    assert_eq!(inspection.reserved_spill_bytes, 0);
    assert_eq!(inspection.peer_pending, 0);
    assert_eq!(inspection.peer_running, 0);
    cluster
        .shutdown()
        .await
        .expect("cancellation cluster shutdown");
}

/// A real SDK query replans once when its selected worker restarts before dispatch.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_live_topology_replans_through_boot_directory() {
    let mut cluster = WyrdTestCluster::start_spec_with_oracle_peer_tls_delayed_last(
        BifrostClusterSpec::three_mixed(),
    )
    .await
    .expect("leader-first TLS topology");
    let delayed = *cluster
        .configured_node_ids()
        .last()
        .expect("delayed worker identity");
    cluster
        .restart_node_at_new_address(delayed)
        .await
        .expect("worker joins after leader boot");
    cluster
        .refresh_oracle_snapshots()
        .await
        .expect("late worker snapshot");
    let server = cluster.server(0).expect("query leader");
    let table = unique_table("oracle_topology_replan");
    register_table(server, cluster.data_tenant_id(), &table)
        .await
        .expect("topology table");
    let client = client(server, "oracle-topology-replan")
        .await
        .expect("topology client");
    // The production planner closes at most 16 files into one fragment. Four
    // deterministic fragments exercise the real portable assignment rather
    // than the single-fragment leader-only fast path.
    for ordinal in 0..64 {
        seed_foreign_hot_row(
            &cluster,
            cluster.data_tenant_id(),
            &table,
            cluster.data_tenant_id(),
            &format!("topology-{ordinal}"),
        )
        .await
        .expect("independent sealed file");
    }
    let probe = Arc::new(vala_bifrost_redux::oracle::OracleTopologyProbe::default());
    server
        .state()
        .bifrost_query()
        .expect("boot query runtime")
        .oracle()
        .bind_topology_probe_for_test(Arc::clone(&probe));
    let mut query = tokio::spawn(async move {
        let mut stream = QueryClient::new(&client)
            .query(&BifrostQueryRequest {
                sql: format!("SELECT id FROM vala.bifrost.{table} ORDER BY id"),
                visibility: VisibilityMode::PublishedOnly,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(20_000),
            })
            .await?;
        let mut rows = 0_u64;
        while let Some(batch) = stream.next_batch().await? {
            rows = rows.saturating_add(u64::try_from(batch.num_rows())?);
        }
        let terminal = stream.terminal().cloned().ok_or("query terminal missing")?;
        Ok::<_, JourneyError>((rows, terminal))
    });
    tokio::select! {
        () = probe.wait_selected() => {}
        result = &mut query => panic!("query ended before remote selection: {result:?}"),
    }
    let selected = probe.selected_worker().expect("remote selected worker");
    let old_peer = cluster
        .server_by_node(selected)
        .and_then(|server| server.state().oracle_peer())
        .expect("selected peer")
        .clone();
    cluster
        .stop_node(selected)
        .await
        .expect("selected worker stops");
    cluster
        .restart_node_at_new_address(selected)
        .await
        .expect("selected worker replacement");
    cluster
        .refresh_oracle_snapshots()
        .await
        .expect("replacement snapshot");
    probe.resume();
    let (rows, terminal) = query
        .await
        .expect("query task joins")
        .expect("replacement query succeeds");
    assert_eq!(rows, 64);
    assert_eq!(terminal.outcome, QueryTerminalOutcome::Success);
    assert!(terminal.warnings.contains(&QueryWarning::StaleCutReplanned));
    assert_eq!(old_peer.worker().pending_reservations(), 0);
    cluster.shutdown().await.expect("topology replan shutdown");
}

/// AC4: a distributed Analytical query admits at least one fragment to run on a
/// non-leader peer.
///
/// Proves the peer-side schedulability clamp end to end on the multi-pod
/// harness. Every harness Oracle child derives running capacity 1 (a 1 GiB pod
/// yields a 256 MiB child, one 256 MiB memory slot, `min(cpu, 1)`), while an
/// Analytical query carries the fixed per-node slot demand 2. Without the clamp
/// in `take_for_execute`, every peer rejects the demand-2 reservation, every
/// fragment falls back to the leader's local path, and each non-leader peer's
/// successful-admission count stays zero even though the aggregate still returns
/// correctly (the leader absorbs the work). With the clamp, demand is charged as
/// 1 and at least one non-leader peer admits a fragment to run. The final
/// assertion therefore fails without the fix and passes with it, which is why it
/// is a real gate rather than a fan-out-only proxy.
///
/// The successful-admission counter is a `test-support` observable because the
/// production fragment span carries locality but no outcome and the outcome
/// metric carries no locality, so "a fragment executed successfully on a peer"
/// is otherwise unobservable without a new production telemetry contract.
///
/// # Panics
///
/// Panics if the cluster, table registration, seeding, query, or aggregate
/// value does not match the expected distributed Analytical journey.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_analytical_query_admits_fragment_on_non_leader_peer() {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::three_mixed())
        .await
        .expect("start Analytical admission cluster");
    let server = cluster.server(0).expect("query leader");
    let table = unique_table("oracle_analytical_admission");
    register_table(server, cluster.data_tenant_id(), &table)
        .await
        .expect("analytical table");
    let client = client(server, "oracle-analytical-admission")
        .await
        .expect("analytical client");
    // 64 independent sealed files force the production planner past the
    // single-fragment leader-only fast path (at most 16 files per fragment) into
    // the real portable assignment that fans fragments onto non-leader peers.
    for ordinal in 0..64 {
        seed_foreign_hot_row(
            &cluster,
            cluster.data_tenant_id(),
            &table,
            cluster.data_tenant_id(),
            &format!("analytical-{ordinal}"),
        )
        .await
        .expect("independent sealed file");
    }
    // Freeze each node's membership cut so the leader's portable assignment sees
    // the worker nodes as eligible and fans fragments onto them; without this the
    // assigner's eligible set is the leader alone and every fragment runs local.
    cluster
        .refresh_oracle_snapshots()
        .await
        .expect("membership includes remote workers");
    // COUNT(*) with no GROUP BY is an Aggregate, which the classifier maps to
    // QueryClass::Analytical (fixed per-node slot demand 2) — the exact class the
    // materializer visibility poll issues and the one the clamp governs. A plain
    // scan would classify Interactive (demand 1) and schedule on capacity-1 peers
    // even without the fix, so it could not gate this behavior.
    let mut stream = QueryClient::new(&client)
        .query(&BifrostQueryRequest {
            sql: format!("SELECT COUNT(*) AS total FROM vala.bifrost.{table}"),
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: Some(20_000),
        })
        .await
        .expect("analytical query opens");
    let mut total: Option<i64> = None;
    while let Some(batch) = stream.next_batch().await.expect("analytical batch") {
        if batch.num_rows() == 0 {
            continue;
        }
        let counts = batch
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .expect("count column is Int64");
        total = Some(counts.value(0));
    }
    let terminal = stream.terminal().cloned().expect("analytical terminal");
    assert_eq!(terminal.outcome, QueryTerminalOutcome::Success);
    assert_eq!(total, Some(64), "aggregate must observe every sealed row");
    // Only take_for_execute's success arm increments this counter, and the leader
    // runs its own fragments through take_for_local_leader_execute (which never
    // touches it). A non-zero sum across every peer therefore proves at least one
    // fragment was admitted to run on a non-leader peer rather than falling back
    // to the leader.
    let peer_admissions: u64 = cluster
        .servers()
        .filter_map(|server| {
            server
                .state()
                .oracle_peer()
                .map(|peer| peer.worker().admitted_running_total())
        })
        .sum();
    assert!(
        peer_admissions >= 1,
        "expected at least one Analytical fragment admitted on a non-leader peer, got {peer_admissions}"
    );
    cluster
        .shutdown()
        .await
        .expect("analytical admission shutdown");
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
    let opened = QueryClient::new(client)
        .query(&BifrostQueryRequest {
            sql,
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: None,
        })
        .await;
    let mut stream = match opened {
        Ok(stream) => stream,
        Err(ValaSdkError::Transport(WyrdError::Vala { error })) => {
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

/// Send one Arrow IPC batch carrying a single row with an explicit `value`,
/// used to build multiple statistically distinguishable published files.
async fn ingest_marked(
    client: &WyrdClient,
    table: &str,
    id: i64,
    value: &str,
) -> Result<(), JourneyError> {
    BifrostGrpcTransport::connect(client)
        .await?
        .insert_batch(
            table,
            uuid::Uuid::now_v7().into_bytes(),
            ipc_marked(id, value),
        )
        .await?;
    Ok(())
}

/// Encode one deterministic `(id, value)` journey row as one Arrow stream.
fn ipc_marked(id: i64, value: &str) -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("value", DataType::Utf8, false),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(vec![id])),
            Arc::new(StringArray::from(vec![value])),
        ],
    )
    .expect("fixed marked journey arrays share a length");
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, schema.as_ref()).expect("valid schema");
    writer.write(&batch).expect("in-memory IPC write");
    writer.finish().expect("in-memory IPC finish");
    bytes
}
