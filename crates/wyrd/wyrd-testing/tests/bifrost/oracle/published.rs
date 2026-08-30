//! Oracle journeys — published-cut governance: the production storage owner's
//! cache reuse, immutable identity, pruning, bounded cancellation, and the
//! process lifecycle that must close it.
//!
//! Module of the `oracle` binary; see `main.rs` for the capability it proves
//! and `support.rs` for the fixtures it shares.

//! Oracle journeys — Published governance: the node's one storage owner
//! decides every hot decode, every pre-footer exclusion, and every teardown.
//!
//! Module of the `oracle` binary; see `main.rs` for the capability it proves
//! and `support.rs` for the fixtures it shares.

use std::sync::Arc;
use std::time::Duration;

use arrow::array::StringArray;
use arrow::array::{Array, Int64Array};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use chrono::{DateTime, Utc};
use vala_bifrost_redux::storage::{
    BifrostStorage, BifrostStorageError, CacheEffect, MetadataCacheSnapshot, StorageLifecycle,
    StorageOperation, StorageOperationBarrier, StorageRequestOutcome,
};
use vala_sdk::{BifrostGrpcTransport, QueryClient};
use wyrd_client::WyrdClient;
use wyrd_spec::vala::api::{BifrostQueryRequest, FreshnessPolicy, VisibilityMode};
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::{BifrostClusterSpec, ScribeCacheMode, WyrdTestCluster};

use crate::support::*;

/// The published read path is governed end to end by the node's storage owner.
///
/// One fixture, seven observations, each measured as a delta against a
/// checkpoint taken immediately before its phase. Together they state the
/// property that makes the owner worth having: every decode of an immutable
/// object happens at most once per node, every object a query cannot need is
/// excluded before its footer is touched, a stalled backend ends in a stable
/// terminal instead of a hang, and teardown leaves nothing retained. An
/// aggregate counter from an earlier phase is never allowed to stand in for the
/// identity under test, which is why each phase re-reads the owner's snapshot
/// rather than the run's totals.
///
/// The cancellation observation runs against a separately bound server because
/// it needs `cancel_and_join_for_test` — a bound harness handle, not a cluster
/// pod — to cancel the process while a read is deliberately stalled. Both
/// owners are reconciled at the end.
///
/// # Panics
///
/// Panics when any phase's observation does not hold.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn published_cache_pruning_and_shutdown_are_production_governed() {
    prove_published_governance()
        .await
        .expect("the published governance journey holds");
}

/// Drives the seven published-governance observations in fixture order.
///
/// # Errors
///
/// Returns a client, Postgres, ingest, Forge-scheduling, telemetry, or
/// cluster-lifecycle error surfaced by any phase.
async fn prove_published_governance() -> Result<(), JourneyError> {
    let cluster = WyrdTestCluster::start_spec_with_forge_completion_observer(
        BifrostClusterSpec::one_mixed().with_metadata_cache_mode(ScribeCacheMode::Enabled),
    )
    .await?;
    let server = cluster
        .server(0)
        .ok_or("the one-pod cluster runs one server")?;
    let storage = Arc::clone(
        server
            .state()
            .bifrost_storage()
            .ok_or("a composed pod owns one Bifrost storage owner")?,
    );
    if !storage.metadata_cache_enabled() {
        return Err("this journey requires a retaining composition".into());
    }
    let owner_tenant = cluster.data_tenant_id();
    let neighbour_tenant = cluster.add_tenant("oracle-published-neighbour").await?;
    let table = unique_table("oracle_published");
    let fqn = format!("vala.bifrost.{table}");
    register_table(server, owner_tenant, &table).await?;
    register_table(server, neighbour_tenant, &table).await?;
    let owner = client_for_tenant(server, owner_tenant, "published-owner").await?;
    let neighbour = client_for_tenant(server, neighbour_tenant, "published-neighbour").await?;

    // 1. Tenant isolation. Two tenants publish under one logical table name;
    //    each public query must return its own rows and only its own rows.
    ingest_row(&owner, &fqn, 1).await?;
    ingest_row(&neighbour, &fqn, 2).await?;
    server.flush_bifrost().await?;
    cluster.refresh_oracle_snapshots().await?;
    let owner_rows = query_ids(&owner, &fqn, None).await?;
    let neighbour_rows = query_ids(&neighbour, &fqn, None).await?;
    assert_eq!(owner_rows, vec![1], "the owning tenant reads only its row");
    assert_eq!(
        neighbour_rows,
        vec![2],
        "the neighbouring tenant reads only its row"
    );

    // 2. Hot cache single-flight and reuse. A second object is published, and
    //    the first read of it is held at the owner's deterministic barrier so a
    //    concurrent identical query provably arrives while that load is still
    //    in flight rather than after it. The event-time floor excludes the
    //    phase-1 object before its footer, so the stalled range belongs to the
    //    identity under test and to nothing else.
    let since_phase_one = Utc::now();
    ingest_row(&owner, &fqn, 3).await?;
    server.flush_bifrost().await?;
    cluster.refresh_oracle_snapshots().await?;

    let before = storage.telemetry_snapshot();
    let barrier = StorageOperationBarrier::new(StorageOperation::ReadRange);
    storage.install_operation_barrier_for_test(Arc::clone(&barrier));
    let first = tokio::spawn({
        let client = owner.clone();
        let fqn = fqn.clone();
        async move { query_ids(&client, &fqn, Some(since_phase_one)).await }
    });
    tokio::time::timeout(Duration::from_secs(30), barrier.wait_until_reached())
        .await
        .map_err(|_| "the first read never reached the storage barrier")?;
    let second = tokio::spawn({
        let client = owner.clone();
        let fqn = fqn.clone();
        async move { query_ids(&client, &fqn, Some(since_phase_one)).await }
    });
    wait_for_waiter(&storage).await?;
    barrier.release();
    let first_rows = first.await??;
    let second_rows = second.await??;
    assert_eq!(
        first_rows,
        vec![3],
        "the floor admits exactly the new object"
    );
    assert_eq!(
        second_rows, first_rows,
        "two concurrent callers of one identity read the same exact rows"
    );
    let after = storage.telemetry_snapshot();
    assert_eq!(
        after.load_starts() - before.load_starts(),
        1,
        "one immutable identity is decoded exactly once, however many callers ask"
    );
    assert!(
        after.effect(CacheEffect::Miss) > before.effect(CacheEffect::Miss),
        "the elected caller records a miss"
    );
    assert!(
        after.effect(CacheEffect::Join) > before.effect(CacheEffect::Join),
        "the concurrent caller joins the in-flight load rather than starting one"
    );
    assert!(
        after.effect(CacheEffect::Hit) > before.effect(CacheEffect::Hit),
        "a later read of a retained identity is served from the cache"
    );

    // 3. Immutable identity miss. A third object is a different identity, so it
    //    must produce its own load rather than a hit on the retained one.
    let before = storage.telemetry_snapshot();
    ingest_row(&owner, &fqn, 4).await?;
    server.flush_bifrost().await?;
    cluster.refresh_oracle_snapshots().await?;
    let combined = query_ids(&owner, &fqn, Some(since_phase_one)).await?;
    assert_eq!(
        combined,
        vec![3, 4],
        "both admitted objects contribute rows"
    );
    let after = storage.telemetry_snapshot();
    assert_eq!(
        after.load_starts() - before.load_starts(),
        1,
        "a new identity loads once; the retained one is not reloaded"
    );
    assert!(
        after.effect(CacheEffect::Miss) > before.effect(CacheEffect::Miss),
        "the new identity misses"
    );
    assert!(
        after.effect(CacheEffect::Hit) > before.effect(CacheEffect::Hit),
        "the retained identity still hits"
    );

    // 4. Hot to Iceberg authority transition. The same rows must survive the
    //    move to snapshot authority exactly: no duplicate, no omission.
    let before_promotion = query_ids(&owner, &fqn, None).await?;
    compact_sealed_batch(&cluster, owner_tenant, &table, 3).await?;
    cluster.refresh_oracle_snapshots().await?;
    let after_promotion = query_ids(&owner, &fqn, None).await?;
    assert_eq!(
        after_promotion, before_promotion,
        "promotion changes which leaf serves a row, never which rows exist"
    );

    // 5. Pre-footer pruning. One hot object and one current-snapshot object
    //    both declare bounds disjoint from the queried interval, so both are
    //    excluded before any footer is opened.
    ingest_row(&owner, &fqn, 5).await?;
    server.flush_bifrost().await?;
    cluster.refresh_oracle_snapshots().await?;
    let (compacted, hot) = file_tier_counts(&cluster, owner_tenant, &table).await?;
    assert!(
        compacted > 0 && hot > 0,
        "the pruning phase needs both source variants present, saw {compacted} compacted \
         and {hot} hot"
    );
    let before = storage.telemetry_snapshot();
    let checkpoint = cluster.telemetry().checkpoint()?;
    let empty =
        query_ids_between(&owner, &fqn, "1970-01-01T00:00:00Z", "1970-01-02T00:00:00Z").await?;
    assert!(
        empty.is_empty(),
        "an interval no object overlaps returns exactly no rows, got {empty:?}"
    );
    let after = storage.telemetry_snapshot();
    assert_eq!(
        after.load_starts(),
        before.load_starts(),
        "an excluded object's footer is never opened"
    );
    let delta = cluster.telemetry().delta_since(&checkpoint)?;
    for source in ["hot", "iceberg"] {
        let excluded = pruning_exclusions(&delta, source);
        assert!(
            excluded > 0.0,
            "the {source} source must record a pre-footer exclusion, saw {excluded}"
        );
    }

    // 7 (cluster owner). Final reconciliation for the pod that served every
    //    phase above: the owner closes, every start has a terminal, and nothing
    //    stays resident, in flight, waiting, or unmatched.
    let inspection = cluster.shutdown_and_inspect().await?;
    let terminal = inspection
        .storage
        .first()
        .ok_or("the drained pod publishes its storage-owner snapshot")?;
    assert_reconciled(terminal, "the served pod");
    assert!(
        terminal.effect(CacheEffect::Hit) > 0
            && terminal.effect(CacheEffect::Miss) > 0
            && terminal.effect(CacheEffect::Join) > 0,
        "the run's cache effects survive into the terminal snapshot: {terminal:?}"
    );
    assert!(
        terminal.load_terminal(vala_bifrost_redux::storage::MetadataLoadOutcome::Success) > 0,
        "every decode this run performed ended in a recorded load terminal"
    );
    assert!(
        terminal.request_terminal(StorageRequestOutcome::Success) > 0,
        "the governed object requests this run issued ended in recorded terminals"
    );

    // 6. Unavailable backend cancellation. A governed read is stalled at the
    //    barrier so no cache hit can bypass it, the process is cancelled
    //    underneath it, and the read must end in a stable terminal — never in
    //    rows, and never in a hang.
    prove_cancelled_read_terminates().await
}

/// Stalls one governed read, cancels the process under it, and reconciles.
///
/// # Errors
///
/// Returns a client, ingest, or lifecycle error, and a descriptive error when
/// the stalled read is not reached, does not terminate, or returns rows.
async fn prove_cancelled_read_terminates() -> Result<(), JourneyError> {
    let mut server = WyrdTestServer::builder()
        .start_bound()
        .await
        .map_err(|error| format!("the bound production server starts: {error}"))?;
    let storage = Arc::clone(
        server
            .state()
            .bifrost_storage()
            .ok_or("a composed server owns one Bifrost storage owner")?,
    );
    let tenant = server.data_tenant_id();
    let table = unique_table("oracle_cancelled");
    let fqn = format!("vala.bifrost.{table}");
    register_table(&server, tenant, &table).await?;
    let reader = client(&server, "cancelled-reader").await?;
    ingest_row(&reader, &fqn, 1).await?;
    server.flush_bifrost().await?;

    let barrier = StorageOperationBarrier::new(StorageOperation::ReadRange);
    storage.install_operation_barrier_for_test(Arc::clone(&barrier));
    let stalled = tokio::spawn({
        let client = reader.clone();
        let fqn = fqn.clone();
        async move { query_ids(&client, &fqn, None).await }
    });
    tokio::time::timeout(Duration::from_secs(30), barrier.wait_until_reached())
        .await
        .map_err(|_| "the read never reached the storage barrier")?;

    let _ = server.cancel_and_join_for_test().await;
    barrier.release();
    let outcome = tokio::time::timeout(Duration::from_secs(60), stalled)
        .await
        .map_err(|_| "a cancelled read must terminate, not hang")??;
    assert!(
        outcome.is_err(),
        "a read cancelled under an unavailable backend must not return rows, got {outcome:?}"
    );

    let snapshot = storage.telemetry_snapshot();
    assert_reconciled(&snapshot, "the cancelled server");
    server
        .shutdown()
        .await
        .map_err(|error| format!("the harness releases its fixtures: {error}"))?;
    Ok(())
}

/// Requires one terminal owner snapshot to be closed and to retain nothing.
///
/// # Panics
/// Panics when the owner is not closed, a start has no terminal, any live count
/// is nonzero, or any settlement was unmatched.
fn assert_reconciled(snapshot: &MetadataCacheSnapshot, label: &str) {
    assert_eq!(
        snapshot.lifecycle(),
        StorageLifecycle::Closed,
        "{label}: a drained process closes its storage owner"
    );
    assert_eq!(
        snapshot.load_starts(),
        snapshot.load_terminals(),
        "{label}: every started decode publishes a terminal"
    );
    assert_eq!(
        snapshot.request_starts(),
        snapshot.request_terminals(),
        "{label}: every admitted governed request publishes a terminal"
    );
    assert_eq!(
        (
            snapshot.resident_entries(),
            snapshot.resident_bytes(),
            snapshot.inflight_loads(),
            snapshot.waiters(),
            snapshot.active_requests(),
            snapshot.anomalies(),
        ),
        (0, 0, 0, 0, 0, 0),
        "{label}: a drained owner retains nothing, observed {snapshot:?}"
    );
}

/// Waits until at least one caller is queued behind an in-flight decode.
///
/// The single-flight observation is only meaningful if the second caller
/// provably arrives while the first load is still running, so the fixture waits
/// for the owner to say so rather than for a duration.
///
/// # Errors
/// Returns an error when no caller queues within the bound.
async fn wait_for_waiter(storage: &Arc<BifrostStorage>) -> Result<(), JourneyError> {
    for _ in 0..600 {
        if storage.telemetry_snapshot().waiters() > 0 {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err("no concurrent caller joined the in-flight decode".into())
}

/// Sums one source's pre-footer exclusions in a production metric delta.
fn pruning_exclusions(
    delta: &wyrd_testing::bifrost::telemetry::BifrostTelemetryDelta,
    source: &str,
) -> f64 {
    delta
        .metrics
        .iter()
        .filter(|sample| {
            sample.family == "bifrost_oracle_file_pruning_total"
                && sample.labels.get("source").map(String::as_str) == Some(source)
                && sample.labels.get("outcome").map(String::as_str) == Some("excluded")
        })
        .map(|sample| sample.value)
        .sum()
}

/// Sends one deterministic journey row through the public ingest path.
///
/// # Errors
/// Returns the transport error when the batch is refused.
async fn ingest_row(client: &WyrdClient, table: &str, id: i64) -> Result<(), JourneyError> {
    BifrostGrpcTransport::connect(client)
        .await?
        .insert_batch(table, uuid::Uuid::now_v7().into_bytes(), ipc_row(id))
        .await?;
    Ok(())
}

/// Encodes one `(id, filter_key, unused_payload)` row as an Arrow IPC stream.
fn ipc_row(id: i64) -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("filter_key", DataType::Utf8, false),
        Field::new("unused_payload", DataType::Utf8, false),
    ]));
    let batch = RecordBatch::try_new(
        Arc::clone(&schema),
        vec![
            Arc::new(Int64Array::from(vec![id])) as arrow::array::ArrayRef,
            Arc::new(StringArray::from(vec![format!("row-{id}")])),
            Arc::new(StringArray::from(vec![unused_payload(id)])),
        ],
    )
    .expect("the journey row's arrays share one length");
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, schema.as_ref()).expect("valid schema");
    writer.write(&batch).expect("in-memory IPC write");
    writer.finish().expect("in-memory IPC finish");
    bytes
}

/// Reads one table's `id` column, optionally floored on event time.
///
/// The floor is what lets a phase name exactly one object: an object whose
/// declared bounds end before it is excluded before its footer is opened.
///
/// # Errors
/// Returns a client or Arrow error, and an error when the stream carries no
/// terminal or emits a batch whose leading column is not a non-null `Int64`.
async fn query_ids(
    client: &WyrdClient,
    table: &str,
    since: Option<DateTime<Utc>>,
) -> Result<Vec<i64>, JourneyError> {
    let filter = since.map_or_else(String::new, |floor| {
        format!(
            " WHERE wyrd_event_time >= TIMESTAMP '{}'",
            floor.format("%Y-%m-%d %H:%M:%S%.6f")
        )
    });
    collect_ids(
        client,
        format!("SELECT id FROM {table}{filter} ORDER BY id"),
    )
    .await
}

/// Reads one table's `id` column inside a closed event-time interval.
///
/// # Errors
/// Returns the same failures as [`query_ids`].
async fn query_ids_between(
    client: &WyrdClient,
    table: &str,
    lower: &str,
    upper: &str,
) -> Result<Vec<i64>, JourneyError> {
    let lower = DateTime::parse_from_rfc3339(lower)?.naive_utc();
    let upper = DateTime::parse_from_rfc3339(upper)?.naive_utc();
    collect_ids(
        client,
        format!(
            "SELECT id FROM {table} WHERE wyrd_event_time >= TIMESTAMP '{lower}' \
             AND wyrd_event_time <= TIMESTAMP '{upper}' ORDER BY id"
        ),
    )
    .await
}

/// Drains one public query stream into its `id` column, in stream order.
///
/// # Errors
/// Returns a client or Arrow error, and an error when the stream carries no
/// terminal frame or a leading column that is not a non-null `Int64`.
async fn collect_ids(client: &WyrdClient, sql: String) -> Result<Vec<i64>, JourneyError> {
    let mut stream = QueryClient::new(client)
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
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .ok_or("the leading projected column is not Int64")?;
        for index in 0..column.len() {
            if column.is_null(index) {
                return Err("the journey writes no null ids".into());
            }
            ids.push(column.value(index));
        }
    }
    stream.terminal().ok_or("query terminal missing")?;
    Ok(ids)
}

/// An already-expired process drain aborts storage and reports the failure.
///
/// The branch under test is the one a real pod takes when its supervisor drain
/// consumed the whole budget: production must not treat "no time left" as a
/// clean teardown. Before this journey existed, that branch returned a
/// successful all-`false` report and never touched the storage owner, so a
/// process could exit reporting success while its metadata cache was still
/// open and its object I/O still admissible. The assertions are therefore
/// three: the terminal is the exact stable lifecycle failure, the retained
/// production owner is closed and settled, and a governed read issued
/// afterwards is refused by the owner rather than reaching the backend.
///
/// # Panics
///
/// Panics when the bound server does not start, when the join returns a
/// successful report or a different failure, when the retained storage snapshot
/// is not closed and quiescent, or when a post-shutdown read is admitted.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn expired_process_shutdown_aborts_storage_and_returns_failure() {
    let mut server = WyrdTestServer::builder()
        .with_shutdown_drain_for_test(Duration::ZERO)
        .start_bound()
        .await
        .expect("the bound production server starts");
    let storage = Arc::clone(
        server
            .state()
            .bifrost_storage()
            .expect("a composed server owns one Bifrost storage owner"),
    );

    let failure = server
        .cancel_and_join_for_test()
        .await
        .expect_err("an already-expired drain must not report a successful shutdown");
    let rendered = format!("{failure:?}");
    assert!(
        rendered.contains("Bifrost shutdown deadline elapsed before role drain"),
        "the expired branch must project its stable lifecycle failure, got {rendered}"
    );

    let snapshot = storage.telemetry_snapshot();
    assert_eq!(
        snapshot.lifecycle(),
        StorageLifecycle::Closed,
        "an aborted process must leave the storage owner closed"
    );
    assert_eq!(snapshot.load_starts(), snapshot.load_terminals());
    assert_eq!(snapshot.request_starts(), snapshot.request_terminals());
    assert_eq!(snapshot.active_requests(), 0);
    assert_eq!(snapshot.inflight_loads(), 0);
    assert_eq!(snapshot.waiters(), 0);
    assert_eq!(snapshot.resident_entries(), 0);
    assert_eq!(snapshot.resident_bytes(), 0);
    assert_eq!(snapshot.anomalies(), 0);

    let refused = storage
        .read("bifrost/journey/after-shutdown.parquet")
        .await
        .expect_err("a closed owner admits no governed read");
    assert_eq!(
        refused,
        BifrostStorageError::Closed,
        "the refusal must come from the owner, before any backend call"
    );
    let after = storage.telemetry_snapshot();
    assert_eq!(
        after.request_terminal(StorageRequestOutcome::Closed),
        snapshot.request_terminal(StorageRequestOutcome::Closed) + 1,
        "the refusal must publish exactly one closed request terminal"
    );
    assert_eq!(after.anomalies(), 0);

    server
        .shutdown()
        .await
        .expect("the harness releases its fixtures");
}
