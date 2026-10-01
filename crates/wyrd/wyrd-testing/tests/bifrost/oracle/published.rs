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

use arrow::array::{Array, Int64Array};
use chrono::{DateTime, Utc};
use vala_bifrost_redux::storage::{
    BifrostStorage, BifrostStorageError, CacheEffect, MetadataCacheSnapshot, StorageLifecycle,
    StorageOperation, StorageOperationBarrier, StorageRequestOutcome,
};
use wyrd_client::WyrdClient;
use wyrd_spec::vala::api::BifrostQueryRequest;
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::telemetry::BifrostMetricKind;
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
    // The production metric window opens before any governed phase and is read
    // after teardown, so what it reports is the emitted stream this run
    // produced rather than a retained total the owner also happens to keep. The
    // capture handle is cloned because `shutdown_and_inspect` consumes the
    // cluster, and the delta has to be taken after that.
    let telemetry = cluster.telemetry().clone();
    let production = telemetry.checkpoint()?;
    let owner_tenant = cluster.data_tenant_id();
    let neighbour_tenant = cluster.add_tenant("oracle-published-neighbour").await?;
    let table = unique_table("oracle_published");
    let fqn = format!("vala.bifrost.{table}");
    register_table(server, owner_tenant, &table).await?;
    register_table(server, neighbour_tenant, &table).await?;
    let owner = writer_for_tenant(server, owner_tenant, "published-owner").await?;
    let neighbour = writer_for_tenant(server, neighbour_tenant, "published-neighbour").await?;

    // 1. Tenant isolation. Two tenants publish under one logical table name;
    //    each public query must return its own rows and only its own rows.
    owner
        .write(&fqn, &journey_schema(), [journey_row(1, "row-1")])
        .await?;
    neighbour
        .write(&fqn, &journey_schema(), [journey_row(2, "row-2")])
        .await?;
    server.flush_bifrost().await?;
    cluster.refresh_oracle_snapshots().await?;
    let owner_rows = query_ids(owner.client(), &fqn, None).await?;
    let neighbour_rows = query_ids(neighbour.client(), &fqn, None).await?;
    assert_eq!(owner_rows, vec![1], "the owning tenant reads only its row");
    assert_eq!(
        neighbour_rows,
        vec![2],
        "the neighbouring tenant reads only its row"
    );

    // 1b. Query stream telemetry. The same owner query is held open after its
    //     first batch, so request opening and stream completion are observed
    //     as the two separate production facts they are.
    prove_query_stream_telemetry(&cluster, server, owner.client(), &fqn, owner_tenant, &table)
        .await?;

    // 2. Hot cache single-flight and reuse. A second object is published, and
    //    the first read of it is held at the owner's deterministic barrier so a
    //    concurrent identical query provably arrives while that load is still
    //    in flight rather than after it. The event-time floor excludes the
    //    phase-1 object before its footer, so the stalled range belongs to the
    //    identity under test and to nothing else.
    let since_phase_one = Utc::now();
    owner
        .write(&fqn, &journey_schema(), [journey_row(3, "row-3")])
        .await?;
    server.flush_bifrost().await?;
    cluster.refresh_oracle_snapshots().await?;

    let before = storage.telemetry_snapshot();
    let barrier = StorageOperationBarrier::new(StorageOperation::ReadRange);
    storage.install_operation_barrier_for_test(Arc::clone(&barrier));
    let first = tokio::spawn({
        let client = owner.client().clone();
        let fqn = fqn.clone();
        async move { query_ids(&client, &fqn, Some(since_phase_one)).await }
    });
    tokio::time::timeout(Duration::from_secs(30), barrier.wait_until_reached())
        .await
        .map_err(|_| "the first read never reached the storage barrier")?;
    let second = tokio::spawn({
        let client = owner.client().clone();
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
    // Repeated once, unstalled: the identity is now resident, so this caller
    // must be served from the cache rather than decode the object again.
    let repeated = query_ids(owner.client(), &fqn, Some(since_phase_one)).await?;
    assert_eq!(
        repeated, first_rows,
        "a cached read returns the same exact rows as the decode that filled it"
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
    owner
        .write(&fqn, &journey_schema(), [journey_row(4, "row-4")])
        .await?;
    server.flush_bifrost().await?;
    cluster.refresh_oracle_snapshots().await?;
    let combined = query_ids(owner.client(), &fqn, Some(since_phase_one)).await?;
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
    let before_promotion = query_ids(owner.client(), &fqn, None).await?;
    compact_sealed_batch(&cluster, owner_tenant, &table, 3).await?;
    cluster.refresh_oracle_snapshots().await?;
    let after_promotion = query_ids(owner.client(), &fqn, None).await?;
    assert_eq!(
        after_promotion, before_promotion,
        "promotion changes which leaf serves a row, never which rows exist"
    );

    // 5. Pre-footer pruning. One hot object and one current-snapshot object
    //    both declare bounds disjoint from the queried interval, so both are
    //    excluded before any footer is opened.
    owner
        .write(&fqn, &journey_schema(), [journey_row(5, "row-5")])
        .await?;
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
    let empty = query_ids_between(
        owner.client(),
        &fqn,
        "1970-01-01T00:00:00Z",
        "1970-01-02T00:00:00Z",
    )
    .await?;
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
    // Only the hot path decides exclusion from file bounds; the Iceberg
    // snapshot prunes inside its own scan planning, and no second walk of the
    // pinned file list runs to manufacture a per-source series.
    let excluded = metric_value(
        &delta,
        "bifrost_oracle_file_pruning_total",
        BifrostMetricKind::Counter,
        &[("outcome", "excluded")],
    );
    assert!(
        excluded >= 1.0 && excluded <= f64::from(u32::try_from(hot)?),
        "every hot object is excluded before its footer, and only hot objects \
         are counted, saw {excluded} for {hot} hot objects"
    );
    assert!(
        delta
            .metrics
            .iter()
            .filter(|sample| sample.family == "bifrost_oracle_file_pruning_total")
            .all(|sample| !sample.labels.contains_key("source")),
        "pruning carries no telemetry-only source label"
    );

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

    // 7b. The production metric stream, not the retained totals. Everything
    //     asserted above is read back out of what the node actually emitted,
    //     because a retained snapshot proves only that the owner counted an
    //     event — an owner that counted correctly and published nothing is
    //     invisible to every operator, dashboard, and alert that consumes it.
    let stream = telemetry.delta_since(&production)?;
    for effect in [CacheEffect::Miss, CacheEffect::Join, CacheEffect::Hit] {
        let emitted = counted(&stream, CACHE_EFFECTS, "effect", Some(effect.as_str()));
        assert!(
            emitted > 0.0,
            "the emitted stream must carry the {} the retained snapshot recorded, saw {emitted}",
            effect.as_str()
        );
    }
    let loads = counted(&stream, CACHE_LOADS, "outcome", Some("success"));
    assert!(
        loads > 0.0,
        "every successful decode must reach the emitted load-terminal counter, saw {loads}"
    );
    let starts = counted(&stream, REQUEST_STARTS, "operation", None);
    let terminals = counted(&stream, REQUEST_TERMINALS, "outcome", None);
    assert!(
        starts > 0.0 && terminals > 0.0,
        "the emitted stream must carry governed request starts and terminals, saw          {starts} starts and {terminals} terminals"
    );
    assert!(
        terminals >= starts,
        "no admitted request may leave the window without an emitted terminal, saw          {starts} starts and {terminals} terminals"
    );
    assert!(
        counted(&stream, REQUEST_TERMINALS, "outcome", Some("success")) > 0.0,
        "the emitted terminal breakdown must carry the successful reads this run made"
    );
    // The retry counter is asserted against the retained total rather than
    // against zero: this fixture's objects are reachable on the first attempt,
    // so the honest statement is that the emitted stream agrees with the owner,
    // whatever number that is. Forcing a retry here to make the counter nonzero
    // would prove only that the fixture can break a backend.
    let retries = counted(&stream, REQUEST_RETRIES, "operation", None);
    #[allow(clippy::cast_precision_loss)]
    let retained_retries = terminal.request_retries() as f64;
    assert!(
        (retries - retained_retries).abs() < f64::EPSILON,
        "the emitted retry counter must agree with the owner's retained total, saw          {retries} emitted and {retained_retries} retained"
    );
    // The lifecycle itself is retained state rather than a series, so `Closed`
    // is asserted from the terminal snapshot above; what the stream can state
    // is that the node ended with no governed request still admitted.
    let active = gauge_at_end(&stream, ACTIVE_REQUESTS);
    assert!(
        active.abs() < f64::EPSILON,
        "the emitted active-request gauge must end at zero, saw {active}"
    );
    let anomalies = counted(&stream, TRANSITION_ANOMALIES, "transition", None);
    assert!(
        anomalies.abs() < f64::EPSILON,
        "the emitted stream must carry no unmatched settlement, saw {anomalies}"
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
        .with_shutdown_drain_for_test(CANCELLED_READ_DRAIN)
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
    let reader = writer(&server, "cancelled-reader").await?;
    reader
        .write(&fqn, &journey_schema(), [journey_row(1, "row-1")])
        .await?;
    server.flush_bifrost().await?;

    let barrier = StorageOperationBarrier::new(StorageOperation::ReadRange);
    storage.install_operation_barrier_for_test(Arc::clone(&barrier));
    let before_cancellation = storage.telemetry_snapshot();
    let stalled = tokio::spawn({
        let client = reader.client().clone();
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
    // The stalled read must end in a terminal the owner's cancellation contract
    // authorizes. Reconciled totals alone are not that proof: a read that hit
    // some unrelated backend failure would reconcile just as neatly while
    // saying nothing about whether cancellation is bounded, which is the
    // property this phase exists to establish.
    let cancellation: Vec<(&str, u64)> = AUTHORIZED_CANCELLATION_TERMINALS
        .iter()
        .map(|outcome| {
            (
                outcome.as_str(),
                snapshot
                    .request_terminal(*outcome)
                    .saturating_sub(before_cancellation.request_terminal(*outcome)),
            )
        })
        .filter(|(_, delta)| *delta > 0)
        .collect();
    let authorized: u64 = cancellation.iter().map(|(_, delta)| *delta).sum();
    assert_eq!(
        authorized, 1,
        "the stalled governed read must end in exactly one authorized cancellation \
         terminal, observed {cancellation:?} against {snapshot:?}"
    );
    assert_reconciled(&snapshot, "the cancelled server");
    server
        .shutdown()
        .await
        .map_err(|error| format!("the harness releases its fixtures: {error}"))?;
    Ok(())
}

/// Shutdown budget the cancellation phase binds its server with.
///
/// The stalled read is released only once `cancel_and_join_for_test` returns,
/// and that join necessarily consumes the whole drain budget: the barrier holds
/// the one governed request the drain is waiting on. The production default is
/// sized for the verification runtime's own in-flight drain and exceeds an
/// Oracle query's first-batch timeout, so a server bound with it would let this
/// query reach its own deadline mid-drain — ending the read on a timeout rather
/// than on the cancellation this phase exists to observe, and leaving the
/// barrier-held storage child outliving its released owner. A budget inside the
/// query timeout keeps cancellation the cause of the terminal.
const CANCELLED_READ_DRAIN: Duration = Duration::from_secs(15);

/// Terminals the owner's cancellation contract permits for a stalled read.
///
/// Deliberately narrow: `Backend` is excluded because it is the outcome an
/// unrelated failure also reaches, so accepting it would let this phase pass on
/// a read that was never actually governed to a bounded stop.
const AUTHORIZED_CANCELLATION_TERMINALS: [StorageRequestOutcome; 3] = [
    StorageRequestOutcome::Closed,
    StorageRequestOutcome::Cancelled,
    StorageRequestOutcome::Deadline,
];

/// Emitted counter of decisions the metadata cache took.
const CACHE_EFFECTS: &str = "bifrost_storage_metadata_cache_effects_total";
/// Emitted counter of terminal metadata loads, by outcome.
const CACHE_LOADS: &str = "bifrost_storage_metadata_cache_loads_total";
/// Emitted counter of governed logical requests admitted.
const REQUEST_STARTS: &str = "bifrost_storage_requests_total";
/// Emitted counter of governed logical request terminals, by outcome.
const REQUEST_TERMINALS: &str = "bifrost_storage_request_terminals_total";
/// Emitted counter of attempts beyond a read's first.
const REQUEST_RETRIES: &str = "bifrost_storage_request_retries_total";
/// Emitted gauge of governed requests admitted and not yet settled.
const ACTIVE_REQUESTS: &str = "bifrost_storage_active_requests";
/// Emitted counter of settlements that had no matching admission.
const TRANSITION_ANOMALIES: &str = "bifrost_storage_metadata_cache_transition_anomalies_total";

/// Sums one production counter family in a delta, optionally by one label.
///
/// `label` of `None` sums every series in the family, which is how a total is
/// read without enumerating a closed label domain the test would then have to
/// keep in step with the owner.
fn counted(
    delta: &wyrd_testing::bifrost::telemetry::BifrostTelemetryDelta,
    family: &str,
    key: &str,
    label: Option<&str>,
) -> f64 {
    delta
        .metrics
        .iter()
        .filter(|sample| {
            sample.family == family
                && label.is_none_or(|expected| {
                    sample.labels.get(key).map(String::as_str) == Some(expected)
                })
        })
        .map(|sample| sample.value)
        .sum()
}

/// Sums one production gauge family's value at the end of a delta window.
fn gauge_at_end(
    delta: &wyrd_testing::bifrost::telemetry::BifrostTelemetryDelta,
    family: &str,
) -> f64 {
    delta
        .gauge_final
        .iter()
        .filter(|sample| sample.family == family)
        .map(|sample| sample.value)
        .sum()
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

/// Waits until a second caller is queued behind one in-flight decode.
///
/// The single-flight observation is only meaningful if the second caller
/// provably arrives while the first load is still running, so the fixture waits
/// for the owner to say so rather than for a duration. The elected loader
/// counts itself as a waiter, so two is the first count that means a caller
/// actually joined rather than started.
///
/// # Errors
/// Returns an error when no second caller queues within the bound.
async fn wait_for_waiter(storage: &Arc<BifrostStorage>) -> Result<(), JourneyError> {
    for _ in 0..600 {
        if storage.telemetry_snapshot().waiters() > 1 {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err("no concurrent caller joined the in-flight decode".into())
}

/// Families retired as zero-only, duplicate, or telemetry-only work.
///
/// A sample of any of these in a real query window means a deleted emitter
/// came back: the production HPA and query report read
/// `oracle_query_duration_seconds`, and the rest never described real work.
const RETIRED_QUERY_FAMILIES: [&str; 6] = [
    "bifrost_query_duration_seconds",
    "oracle_tenant_budget_pressure",
    "oracle_query_spill_bytes_total",
    "oracle_query_spill_files_total",
    "oracle_query_logical_bytes_selected_total",
    "bifrost_oracle_analytical_exchanges_active",
];

/// Proves the Gate request, Gate stream, and Oracle query facts of one query.
///
/// The query is parked after its first batch frame, so the window taken while
/// it is parked shows the request opened successfully and no stream terminal
/// exists yet. The window taken after the client reads the terminal shows
/// exactly one successful stream and one Oracle execution, nested inside the
/// server-edge interval, which is itself inside the client's own clock. The
/// trace must be one causal story: the dispatch span is a child of the stream
/// span, and the Oracle stream span shares its trace and ends inside it.
///
/// # Errors
/// Returns a client, park, Postgres, or telemetry error.
///
/// # Panics
/// Panics when an emitted fact disagrees with the stream the client observed.
async fn prove_query_stream_telemetry(
    cluster: &WyrdTestCluster,
    server: &WyrdTestServer,
    client: &WyrdClient,
    fqn: &str,
    tenant: wyrd_spec::DataTenantId,
    table: &str,
) -> Result<(), JourneyError> {
    let telemetry = cluster.telemetry();
    let opened_window = telemetry.checkpoint()?;
    let final_window = telemetry.checkpoint()?;
    let park = server.park_next_query_after_rows()?;
    let client_clock = std::time::Instant::now();
    let mut stream = wyrd_client::Bifrost::query_only(client)
        .query(&BifrostQueryRequest {
            sql: format!("SELECT id FROM {fqn} ORDER BY id"),
            deadline_ms: None,
        })
        .await?;
    tokio::time::timeout(Duration::from_secs(30), park.wait_entered())
        .await
        .map_err(|_| "the query never parked after its first batch")?;

    let opened = telemetry.delta_since(&opened_window)?;
    assert_eq!(
        metric_value(
            &opened,
            "bifrost_gate_requests_total",
            BifrostMetricKind::Counter,
            &[("operation", "query"), ("outcome", "success")],
        ),
        1.0,
        "an opened stream is one successful Gate query request"
    );
    assert_eq!(
        metric_value(
            &opened,
            "bifrost_gate_query_streams_total",
            BifrostMetricKind::Counter,
            &[],
        ),
        0.0,
        "an open, unconsumed stream has no terminal outcome yet"
    );
    assert_eq!(
        metric_value(
            &opened,
            "bifrost_gate_query_stream_duration_seconds",
            BifrostMetricKind::HistogramCount,
            &[],
        ),
        0.0,
        "an open stream has no server-edge duration yet"
    );
    let active: f64 = opened
        .gauge_final
        .iter()
        .filter(|sample| {
            sample.family == "oracle_queries_active"
                && sample.labels.get("class").map(String::as_str) == Some("interactive")
        })
        .map(|sample| sample.value)
        .sum();
    assert_eq!(
        active, 1.0,
        "the parked admitted query is the one active query"
    );
    assert!(
        !opened.spans.iter().any(|span| {
            span.name == "bifrost.gate.query.stream" || span.name == "bifrost.oracle.stream"
        }),
        "no query operation span closes while its stream is still open"
    );

    park.resume();
    let mut ids = Vec::new();
    while let Some(batch) = stream.next_batch().await? {
        let column = batch
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .ok_or("the leading projected column is not Int64")?;
        ids.extend(column.iter().flatten());
    }
    stream.terminal().ok_or("query terminal missing")?;
    let client_elapsed = client_clock.elapsed().as_secs_f64();
    drop(stream);
    assert_eq!(
        ids,
        vec![1],
        "the parked query returns the owner's exact row"
    );
    wait_for_spans(
        telemetry,
        &final_window,
        &["bifrost.gate.query.stream", "bifrost.oracle.stream"],
    )
    .await?;

    let finished = telemetry.delta_since(&final_window)?;
    for outcome in ["success", "degraded", "rejected", "failed", "cancelled"] {
        let expected = if outcome == "success" { 1.0 } else { 0.0 };
        assert_eq!(
            metric_value(
                &finished,
                "bifrost_gate_query_streams_total",
                BifrostMetricKind::Counter,
                &[("outcome", outcome)],
            ),
            expected,
            "one consumed stream is exactly one {outcome} terminal when expected"
        );
    }
    let gate_count = metric_value(
        &finished,
        "bifrost_gate_query_stream_duration_seconds",
        BifrostMetricKind::HistogramCount,
        &[("outcome", "success")],
    );
    let gate_seconds = metric_value(
        &finished,
        "bifrost_gate_query_stream_duration_seconds",
        BifrostMetricKind::HistogramSum,
        &[("outcome", "success")],
    );
    let oracle_count = metric_value(
        &finished,
        "oracle_query_duration_seconds",
        BifrostMetricKind::HistogramCount,
        &[("class", "interactive"), ("outcome", "success")],
    );
    let oracle_seconds = metric_value(
        &finished,
        "oracle_query_duration_seconds",
        BifrostMetricKind::HistogramSum,
        &[("class", "interactive"), ("outcome", "success")],
    );
    assert_eq!(
        gate_count, 1.0,
        "one server-edge stream duration observation"
    );
    assert_eq!(
        oracle_count, 1.0,
        "one Oracle execution duration observation"
    );
    assert!(
        oracle_seconds <= gate_seconds && gate_seconds <= client_elapsed,
        "Oracle work ({oracle_seconds}s) starts after the Gate stream ({gate_seconds}s), \
         which the client clock ({client_elapsed}s) contains"
    );

    let (compacted, hot) = file_tier_counts(cluster, tenant, table).await?;
    let files = metric_value(
        &finished,
        "oracle_query_files_scanned_total",
        BifrostMetricKind::Counter,
        &[("class", "interactive")],
    );
    let bytes = metric_value(
        &finished,
        "oracle_query_bytes_scanned_total",
        BifrostMetricKind::Counter,
        &[("class", "interactive")],
    );
    assert_eq!(
        files,
        f64::from(u32::try_from(compacted + hot)?),
        "scanned files are the published objects the plan actually read"
    );
    assert!(bytes > 0.0, "a read of a published object scans its bytes");

    for family in RETIRED_QUERY_FAMILIES {
        assert!(
            !finished
                .metrics
                .iter()
                .chain(&finished.gauge_final)
                .any(|sample| sample.family == family && sample.value != 0.0),
            "retired family {family} must not be emitted"
        );
    }
    assert!(
        !finished.metrics.iter().any(|sample| {
            sample.family == "oracle_query_phase_seconds"
                && sample.labels.get("phase").is_some_and(|phase| {
                    [
                        "first_row",
                        "terminal",
                        "table_lookup",
                        "manifest_scan",
                        "hot_cut",
                    ]
                    .contains(&phase.as_str())
                })
        }),
        "duplicate and per-substep phases are not emitted"
    );

    let span = |name: &str| {
        finished
            .spans
            .iter()
            .filter(|span| span.name == name)
            .collect::<Vec<_>>()
    };
    let gate_streams = span("bifrost.gate.query.stream");
    let [gate_stream] = gate_streams.as_slice() else {
        panic!("one query has one Gate stream span, saw {gate_streams:?}");
    };
    assert_eq!(
        gate_stream.attributes.get("outcome").map(String::as_str),
        Some("success")
    );
    let dispatches = span("bifrost.gate.query");
    assert!(
        dispatches
            .iter()
            .any(|dispatch| dispatch.parent_span_id == gate_stream.span_id),
        "dispatch is causal child work of the client-facing stream"
    );
    let oracle_streams = span("bifrost.oracle.stream");
    let [oracle_stream] = oracle_streams.as_slice() else {
        panic!("one query has one Oracle stream span, saw {oracle_streams:?}");
    };
    assert_eq!(oracle_stream.trace_id, gate_stream.trace_id);
    assert_eq!(
        oracle_stream.attributes.get("outcome").map(String::as_str),
        Some("success")
    );
    assert!(
        oracle_stream.duration_nanos <= gate_stream.duration_nanos,
        "Oracle stream work ends inside the Gate stream lifetime"
    );
    Ok(())
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
    let mut stream = wyrd_client::Bifrost::query_only(client)
        .query(&BifrostQueryRequest {
            sql,
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
