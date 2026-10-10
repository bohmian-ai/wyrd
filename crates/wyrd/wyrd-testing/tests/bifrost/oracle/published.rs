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
use vala_bifrost_redux::forge::ForgeConfig;
use vala_bifrost_redux::storage::{
    BifrostStorage, BifrostStorageError, StorageInspection, StorageLifecycle, StorageOperation,
    StorageOperationBarrier, StorageRequestOutcome,
};
use wyrd_client::WyrdClient;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::BifrostQueryRequest;
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::telemetry::{BifrostMetricKind, BifrostTelemetryDelta};
use wyrd_testing::bifrost::{
    BifrostClusterSpec, ScribeCacheMode, WyrdTestCluster, shared_process_telemetry_for_test,
};

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
/// identity under test, which is why each phase reads its own production
/// metric window rather than the run's totals, and settled state is read from
/// the owners themselves.
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
///
/// # Panics
///
/// Panics when a phase's observation does not hold: tenant-isolated reads,
/// metadata-cache and pruning counters, or settled shutdown reports.
#[expect(
    clippy::float_cmp,
    reason = "Prometheus renders these metrics as whole numbers, so f64 equality is exact"
)]
async fn prove_published_governance() -> Result<(), JourneyError> {
    // Forge promotes each Scribe flush to Iceberg as soon as it is published
    // (REQ-002), but phases 1-3 and 5 observe hot objects. The pod's Forge
    // catalog is therefore wrapped in the production commit seam, and the
    // journey parks the first promotion commit before any write: promotion
    // runs inline on the coordinator's supervisor, so every later hint queues
    // behind the parked one and each published object stays hot until the
    // journey releases it. The parked supervisor skips its heartbeats, which
    // the 30 s leader term outlasts for these few-second phases.
    let cluster = WyrdTestCluster::start_spec_with_forge_config_and_completion_observer(
        // The tenant's audit table must not take the one parked promotion.
        BifrostClusterSpec::one_mixed().with_metadata_cache_mode(ScribeCacheMode::Enabled),
        ForgeConfig::default(),
        false,
        true,
    )
    .await?;
    let promotion = cluster
        .commit_uncertainty_catalog()
        .ok_or("the pod wraps its Forge catalog in the commit seam")?;
    promotion.pause_before_commit();
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
    //    identity under test and to nothing else. Scribe stamps event time
    //    from PostgreSQL, so the floor is read from that same clock.
    let since_phase_one: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&cluster.pg_fixture().superuser_pool()?)
        .await?;
    owner
        .write(&fqn, &journey_schema(), [journey_row(3, "row-3")])
        .await?;
    server.flush_bifrost().await?;
    cluster.refresh_oracle_snapshots().await?;

    let single_flight = telemetry.checkpoint()?;
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
    let delta = telemetry.delta_since(&single_flight)?;
    assert_eq!(
        counted(&delta, CACHE_LOADS, "outcome", None),
        1.0,
        "one immutable identity is decoded exactly once, however many callers ask"
    );
    assert!(
        cache_effect(&delta, "miss") > 0.0,
        "the elected caller records a miss"
    );
    assert!(
        cache_effect(&delta, "join") > 0.0,
        "the concurrent caller joins the in-flight load rather than starting one"
    );
    assert!(
        cache_effect(&delta, "hit") > 0.0,
        "a later read of a retained identity is served from the cache"
    );

    // 3. Immutable identity miss. A third object is a different identity, so it
    //    must produce its own load rather than a hit on the retained one.
    let identity = telemetry.checkpoint()?;
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
    let delta = telemetry.delta_since(&identity)?;
    assert_eq!(
        counted(&delta, CACHE_LOADS, "outcome", None),
        1.0,
        "a new identity loads once; the retained one is not reloaded"
    );
    assert!(
        cache_effect(&delta, "miss") > 0.0,
        "the new identity misses"
    );
    assert!(
        cache_effect(&delta, "hit") > 0.0,
        "the retained identity still hits"
    );

    // 4. Hot to Iceberg authority transition. The same rows must survive the
    //    move to snapshot authority exactly: no duplicate, no omission.
    //    The parked promotion is released to commit, the hints queued behind
    //    it follow, and the requested passes below promote any remaining debt.
    let before_promotion = query_ids(owner.client(), &fqn, None).await?;
    tokio::time::timeout(Duration::from_secs(30), promotion.wait_for_before_commit())
        .await
        .map_err(|_| "the first promotion never reached the parked commit")?;
    promotion.release_paused_before_commit();
    compact_sealed_batch(&cluster, owner_tenant, &table, 3).await?;
    cluster.refresh_oracle_snapshots().await?;
    let after_promotion = query_ids(owner.client(), &fqn, None).await?;
    assert_eq!(
        after_promotion, before_promotion,
        "promotion changes which leaf serves a row, never which rows exist"
    );

    // 5. Pre-footer pruning. One hot object and one current-snapshot object
    //    both declare bounds disjoint from the queried interval, so both are
    //    excluded before any footer is opened. Promotion is parked again so
    //    the new object stays hot; the parked commit is drained, unsettled,
    //    by the production shutdown in phase 7.
    promotion.pause_before_commit();
    owner
        .write(&fqn, &journey_schema(), [journey_row(5, "row-5")])
        .await?;
    server.flush_bifrost().await?;
    tokio::time::timeout(Duration::from_secs(30), promotion.wait_for_before_commit())
        .await
        .map_err(|_| "the new object's promotion never reached the parked commit")?;
    cluster.refresh_oracle_snapshots().await?;
    let (compacted, hot) = file_tier_counts(&cluster, owner_tenant, &table).await?;
    assert!(
        compacted > 0 && hot > 0,
        "the pruning phase needs both source variants present, saw {compacted} compacted \
         and {hot} hot"
    );
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
    let delta = cluster.telemetry().delta_since(&checkpoint)?;
    assert!(
        counted(&delta, CACHE_LOADS, "outcome", None).abs() < f64::EPSILON,
        "an excluded object's footer is never opened"
    );
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

    // 7 (cluster owner). Final settlement for the pod that served every phase
    //    above, read from the owners after production teardown: the owner
    //    closes and nothing stays resident, in flight, waiting, or admitted.
    let inspection = cluster.shutdown_and_inspect().await?;
    let terminal = inspection
        .storage
        .first()
        .ok_or("the drained pod publishes its storage-owner state")?;
    assert_settled(terminal, "the served pod");

    // 7b. The production metric stream over the whole run. An owner that
    //     settled correctly and published nothing is invisible to every
    //     operator, dashboard, and alert, so the run's decisions and terminals
    //     must be read back out of what the node actually emitted.
    let stream = telemetry.delta_since(&production)?;
    for effect in ["miss", "join", "hit"] {
        let emitted = cache_effect(&stream, effect);
        assert!(
            emitted > 0.0,
            "the emitted stream must carry the run's {effect} decisions, saw {emitted}"
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
    // The lifecycle is owner state rather than a series, so `Closed` is
    // asserted from the terminal inspection above; what the stream can state
    // is that the node ended with no governed request still admitted.
    let active = gauge_at_end(&stream, ACTIVE_REQUESTS);
    assert!(
        active.abs() < f64::EPSILON,
        "the emitted active-request gauge must end at zero, saw {active}"
    );
    for retired in RETIRED_STORAGE_FAMILIES {
        assert!(
            stream
                .metrics
                .iter()
                .chain(&stream.gauge_final)
                .all(|sample| sample.family != retired),
            "the retired shadow-ledger family {retired} must not be emitted"
        );
    }

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

    let (_telemetry_guard, telemetry) = shared_process_telemetry_for_test()?;
    let barrier = StorageOperationBarrier::new(StorageOperation::ReadRange);
    storage.install_operation_barrier_for_test(Arc::clone(&barrier));
    let cancellation_window = telemetry.checkpoint()?;
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
    let outcome = tokio::time::timeout(Duration::from_mins(1), stalled)
        .await
        .map_err(|_| "a cancelled read must terminate, not hang")??;
    assert!(
        outcome.is_err(),
        "a read cancelled under an unavailable backend must not return rows, got {outcome:?}"
    );

    // The stalled read must end in a terminal the owner's cancellation contract
    // authorizes. A settled owner alone is not that proof: a read that hit some
    // unrelated backend failure would settle just as neatly while saying
    // nothing about whether cancellation is bounded, which is the property
    // this phase exists to establish.
    let window = telemetry.delta_since(&cancellation_window)?;
    let cancellation: Vec<(&str, f64)> = AUTHORIZED_CANCELLATION_TERMINALS
        .iter()
        .map(|outcome| {
            let label = outcome.as_str();
            (
                label,
                counted(&window, REQUEST_TERMINALS, "outcome", Some(label)),
            )
        })
        .filter(|(_, delta)| *delta > 0.0)
        .collect();
    let authorized: f64 = cancellation.iter().map(|(_, delta)| *delta).sum();
    assert!(
        (authorized - 1.0).abs() < f64::EPSILON,
        "the stalled governed read must end in exactly one authorized cancellation \
         terminal, observed {cancellation:?}"
    );
    assert_settled(&storage.inspect(), "the cancelled server");
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

/// Emitted counter of decisions the metadata cache took, by effect and reason.
const CACHE_EFFECTS: &str = "bifrost_storage_metadata_cache_effects_total";
/// Emitted counter of terminal metadata loads, by outcome.
const CACHE_LOADS: &str = "bifrost_storage_metadata_cache_loads_total";
/// Emitted counter of governed logical requests admitted.
const REQUEST_STARTS: &str = "bifrost_storage_requests_total";
/// Emitted counter of governed logical request terminals, by outcome.
const REQUEST_TERMINALS: &str = "bifrost_storage_request_terminals_total";
/// Emitted gauge of governed requests admitted and not yet settled.
const ACTIVE_REQUESTS: &str = "bifrost_storage_active_requests";
/// Storage families retired with the shadow ledger that fed them.
///
/// The anomaly counter only ever reported the ledger disagreeing with itself,
/// and the waiter gauge was republished from that ledger on every transition;
/// settled state is now read from the owners, so either reappearing means a
/// deleted emitter came back.
const RETIRED_STORAGE_FAMILIES: [&str; 2] = [
    "bifrost_storage_metadata_cache_transition_anomalies_total",
    "bifrost_storage_metadata_cache_waiters",
];

/// Sums one cache decision's emitted count across its reasons.
fn cache_effect(delta: &BifrostTelemetryDelta, effect: &str) -> f64 {
    counted(delta, CACHE_EFFECTS, "effect", Some(effect))
}

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

/// Requires one terminal owner inspection to be closed and to hold nothing.
///
/// Every value is read from the owners — the request settlement teardown
/// waits on and the metadata cache's own state — so a nonzero count is work
/// still held, never a tally that drifted.
///
/// # Panics
/// Panics when the owner is not closed or any live count is nonzero.
fn assert_settled(inspection: &StorageInspection, label: &str) {
    assert_eq!(
        inspection.lifecycle,
        StorageLifecycle::Closed,
        "{label}: a drained process closes its storage owner"
    );
    assert!(
        inspection.is_settled(),
        "{label}: a drained owner holds nothing, observed {inspection:?}"
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
        let waiters = storage
            .inspect()
            .metadata_cache
            .map_or(0, |cache| cache.waiters);
        if waiters > 1 {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err("no concurrent caller joined the in-flight decode".into())
}

/// Gate and Oracle families the query-stream windows print as evidence.
const QUERY_STREAM_FAMILIES: [&str; 11] = [
    "bifrost_gate_requests_total",
    "bifrost_gate_query_streams_total",
    "bifrost_gate_query_stream_duration_seconds",
    "bifrost_gate_active_streams",
    "oracle_queries_active",
    "oracle_queries_queued",
    "oracle_admission_total",
    "oracle_query_duration_seconds",
    "oracle_query_files_scanned_total",
    "oracle_query_bytes_scanned_total",
    "oracle_query_rows_total",
];

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
#[expect(
    clippy::float_cmp,
    reason = "Prometheus renders these metrics as whole numbers, so f64 equality is exact"
)]
async fn prove_query_stream_telemetry(
    cluster: &WyrdTestCluster,
    server: &WyrdTestServer,
    client: &WyrdClient,
    fqn: &str,
    tenant: DataTenantId,
    table: &str,
) -> Result<(), JourneyError> {
    let telemetry = cluster.telemetry();
    let opened_window = telemetry.checkpoint()?;
    let final_window = telemetry.checkpoint()?;
    let park = server.park_next_query_after_rows()?;
    let client_clock = std::time::Instant::now();
    let mut stream = wyrd_client::Bifrost::query_only(client)
        .query(&BifrostQueryRequest {
            params: Vec::new(),
            sql: format!("SELECT id FROM {fqn} ORDER BY id"),
            deadline_ms: None,
        })
        .await?;
    tokio::time::timeout(Duration::from_secs(30), park.wait_entered())
        .await
        .map_err(|_| "the query never parked after its first batch")?;

    let opened = telemetry.delta_since(&opened_window)?;
    eprintln!(
        "evidence query_opened client_rows_read=0 parked=true samples: {}",
        opened.evidence(&QUERY_STREAM_FAMILIES)
    );
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
    eprintln!(
        "evidence query_finished client_rows={} client_elapsed_seconds={client_elapsed} \
         published_objects={} samples: {}",
        ids.len(),
        compacted + hot,
        finished.evidence(&QUERY_STREAM_FAMILIES)
    );

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
    for span in finished
        .spans
        .iter()
        .filter(|span| span.trace_id == gate_stream.trace_id)
    {
        eprintln!(
            "evidence query_trace trace={} span={} parent={} name={} duration_nanos={} outcome={:?}",
            span.trace_id,
            span.span_id,
            span.parent_span_id,
            span.name,
            span.duration_nanos,
            span.attributes.get("outcome")
        );
    }
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
            params: Vec::new(),
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
/// three: the terminal is the exact stable lifecycle failure, the production
/// owner is closed and settled, and a governed read issued
/// afterwards is refused by the owner rather than reaching the backend.
///
/// # Panics
///
/// Panics when the bound server does not start, when the join returns a
/// successful report or a different failure, when the storage owner is not
/// closed and settled, or when a post-shutdown read is admitted.
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

    let inspection = storage.inspect();
    assert_eq!(
        inspection.lifecycle,
        StorageLifecycle::Closed,
        "an aborted process must leave the storage owner closed"
    );
    assert!(
        inspection.is_settled(),
        "an aborted process leaves nothing held, observed {inspection:?}"
    );

    let (_telemetry_guard, telemetry) =
        shared_process_telemetry_for_test().expect("process production telemetry");
    let refusal_window = telemetry.checkpoint().expect("refusal telemetry window");
    let refused = storage
        .read("bifrost/journey/after-shutdown.parquet")
        .await
        .expect_err("a closed owner admits no governed read");
    assert_eq!(
        refused,
        BifrostStorageError::Closed,
        "the refusal must come from the owner, before any backend call"
    );
    let refusal = telemetry
        .delta_since(&refusal_window)
        .expect("refusal telemetry delta");
    assert!(
        (counted(
            &refusal,
            REQUEST_TERMINALS,
            "outcome",
            Some(StorageRequestOutcome::Closed.as_str())
        ) - 1.0)
            .abs()
            < f64::EPSILON,
        "the refusal must publish exactly one closed request terminal"
    );
    assert_eq!(storage.inspect().active_requests, 0);

    server
        .shutdown()
        .await
        .expect("the harness releases its fixtures");
}
