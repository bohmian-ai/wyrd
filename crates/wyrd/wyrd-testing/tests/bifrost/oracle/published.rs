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

use std::fmt::Display;
use std::sync::Arc;
use std::time::Duration;

use arrow::array::{Array, Int64Array};
use arrow::datatypes::{DataType, Field, Fields, Schema, SchemaRef};
use arrow::record_batch::RecordBatch;
use chrono::{DateTime, Utc};
use vala_bifrost_redux::catalog::{TableRef, TenantTableBinding};
use vala_bifrost_redux::forge::ForgeConfig;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::storage::{
    BifrostStorage, BifrostStorageError, StorageInspection, StorageLifecycle, StorageOperation,
    StorageOperationBarrier, StorageRequestOutcome,
};
use wyrd_client::WyrdClient;
use wyrd_runtime::builtin_roles::WORKLOAD_ROLE;
use wyrd_server::config::BifrostTarget;
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdProblem;
use wyrd_spec::vala::api::{BifrostQueryRequest, QueryClass};
use wyrd_spec::vala::error::BifrostError;
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::telemetry::{BifrostMetricKind, BifrostTelemetryDelta};
use wyrd_testing::bifrost::{
    BifrostClusterSpec, ScribeCacheMode, WyrdTestCluster, shared_process_telemetry_for_test,
};

use crate::analytical_activation::{
    COORDINATOR, PEER_FOLLOWERS, PEER_SCRIBE, RemoteWork, sdk_code,
};
use crate::distributed::{FilterCase, expect_measure, run_case};
use crate::peer_cluster::PeerCluster;
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
        .fetch_one(&cluster.pg_fixture().superuser_pool().await?)
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
    let outcome = tokio::time::timeout(Duration::from_secs(60), stalled)
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

/// One Variant value per fixture row, built from the row's own `filter_key`.
///
/// Deriving the document from a column keeps every Variant expression a
/// per-row computation the planner cannot fold to a constant, so it runs in
/// whichever session executes the scan: the leader for an Interactive query
/// and every worker for an Analytical one.
const ROW_VARIANT: &str = "parse_json('{\"k\":\"' || filter_key || \
     '\",\"n\":9007199254740993,\"o\":{\"a\":[1,\"x\",null]}}')";

/// Every production Oracle session exposes the one Variant SQL surface.
///
/// One four-pod topology runs the same operator and function matrix through
/// the Interactive leader session and through an Analytical graph whose stages
/// round-trip the distributed plan to both followers, so each session decodes
/// and executes the Variant UDFs itself. Invalid `parse_json` text is the
/// stable Variant error on both paths, before the first batch or after a
/// delivered one, while `try_parse_json` is null, and a
/// Variant read of a sensitive gateway payload is refused before any remote
/// work. Plan shape (`variant_get` for Variant, `get_field` for Struct) is the
/// shared installer's contract, pinned by
/// `oracle::variant_sql::tests::variant_operators_and_functions_follow_the_contract`;
/// this journey proves every production session runs that installer.
///
/// # Panics
///
/// Panics when any session diverges from the Variant contract.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn variant_sql_registry_covers_every_session() {
    prove_variant_sql_sessions()
        .await
        .expect("Variant SQL session journey");
}

/// Drives the Variant matrix through every session of one peer topology.
///
/// # Errors
///
/// Returns the first claim that broke.
async fn prove_variant_sql_sessions() -> Result<(), JourneyError> {
    let cluster = PeerCluster::start_with_slots(&[
        (BifrostTarget::Oracle, Some(2)),
        (BifrostTarget::Oracle, Some(2)),
        (BifrostTarget::Oracle, Some(2)),
        (BifrostTarget::Scribe, None),
    ])
    .await?;
    let suffix = uuid::Uuid::now_v7().simple();
    // Two published objects give the grouped statement remote work to
    // distribute; one object keeps the filtered read a leader-only leaf.
    let table = format!("variant_sql_{suffix}");
    cluster.register_table(PEER_SCRIBE, &table).await?;
    cluster.ingest_rows(PEER_SCRIBE, &table, 0, 12, 3).await?;
    cluster.ingest_rows(PEER_SCRIBE, &table, 0, 12, 3).await?;
    let ui_table = format!("variant_sql_ui_{suffix}");
    cluster.register_table(PEER_SCRIBE, &ui_table).await?;
    cluster
        .ingest_rows(PEER_SCRIBE, &ui_table, 0, 12, 3)
        .await?;
    let coordinator = cluster.server(COORDINATOR)?;
    coordinator
        .ensure_builtin_table_for_test(cluster.tenant(), "gateway", "calls")
        .await?;
    cluster.refresh_snapshots().await?;

    let api_key = cluster
        .provision_public_api_key("variant-sql-caller")
        .await?;
    let client = public_client(coordinator, &api_key)?;

    // Interactive: the leader session plans and executes every operator,
    // including a residual Variant predicate and exact Struct access.
    let (path, rows) = run_rows(
        &client,
        &format!(
            "SELECT {ROW_VARIANT} ->> 'k' AS k, \
             CAST({ROW_VARIANT} ->> 'n' AS BIGINT) AS n, \
             to_json({ROW_VARIANT} -> 'o') AS o, to_json({ROW_VARIANT}) AS root, \
             named_struct('m', filter_key)['m'] AS s, \
             try_parse_json('{{bad') IS NULL AS lenient \
             FROM vala.bifrost.{ui_table} WHERE ({ROW_VARIANT} ->> 'k') = 'group_0' ORDER BY id"
        ),
    )
    .await?;
    expect_path("Interactive matrix", path, QueryClass::Interactive)?;
    let expected = [
        "group_0",
        "9007199254740993",
        r#"{"a":[1,"x",null]}"#,
        r#"{"k":"group_0","n":9007199254740993,"o":{"a":[1,"x",null]}}"#,
        "group_0",
        "true",
    ]
    .map(str::to_owned)
    .to_vec();
    if rows != vec![expected; 4] {
        return Err(format!("the Interactive Variant matrix returned {rows:?}").into());
    }

    // Analytical: both followers decode the distributed plan, so the Variant
    // UDFs resolve in their worker sessions, not only on the leader.
    let remote_before = RemoteWork::observe(&cluster)?;
    let (path, rows) = run_rows(
        &client,
        &format!(
            "SELECT {ROW_VARIANT} ->> 'k' AS k, COUNT(*) AS matched, \
             MAX(to_json({ROW_VARIANT} -> 'o')) AS o \
             FROM vala.bifrost.{table} WHERE ({ROW_VARIANT} ->> 'n') IS NOT NULL \
             GROUP BY 1 ORDER BY 1"
        ),
    )
    .await?;
    expect_path("Analytical matrix", path, QueryClass::Analytical)?;
    remote_before.expect_advanced(&cluster, "Analytical Variant matrix")?;
    let expected: Vec<Vec<String>> = (0..3)
        .map(|group| {
            vec![
                format!("group_{group}"),
                "8".to_owned(),
                r#"{"a":[1,"x",null]}"#.to_owned(),
            ]
        })
        .collect();
    if rows != expected {
        return Err(format!("the Analytical Variant matrix returned {rows:?}").into());
    }

    // Invalid text is the stable Variant error whichever session meets it,
    // before any batch or after a valid one: the late terminal carries the
    // same catalog problem, and the whole result is refused.
    let invalid_json = wyrd_spec::error::WyrdError::from(BifrostError::VariantInvalidJson {
        field: "parse_json".to_owned(),
        row: 0,
        path: String::new(),
    })
    .problem();
    for (case, sql) in [
        (
            "Interactive invalid JSON",
            format!("SELECT parse_json('{{bad' || filter_key) AS v FROM vala.bifrost.{ui_table}"),
        ),
        (
            "Analytical invalid JSON",
            format!(
                "SELECT parse_json('{{bad' || filter_key) ->> 'k' AS k, COUNT(*) AS matched \
                 FROM vala.bifrost.{table} GROUP BY 1"
            ),
        ),
    ] {
        match run_rows(&client, &sql).await {
            Err(error) => {
                let problem = error
                    .downcast_ref::<wyrd_client::bifrost::BifrostClientError>()
                    .map(|error| wyrd_spec::error::WyrdError::from(error).problem());
                if problem.as_ref() != Some(&invalid_json) {
                    return Err(format!("{case} failed as {problem:?}: {error}").into());
                }
            }
            Ok(rows) => return Err(format!("{case} settled {rows:?}").into()),
        }
    }
    prove_late_failures(&cluster, &client, &suffix, &invalid_json).await?;
    prove_worker_tenant_refusal(&cluster, &client, &suffix).await?;

    // A caller without gateway payload authority is refused at the logical
    // plan, before any peer or provider work.
    let unprivileged = client_from_bootstrap(
        coordinator,
        coordinator
            .bootstrap_service_in_tenant(
                cluster.tenant(),
                "variant-sql-unprivileged",
                &[WORKLOAD_ROLE],
            )
            .await?,
    )
    .await?;
    let remote_before = RemoteWork::observe(&cluster)?;
    match run_rows(
        &unprivileged,
        "SELECT request_payload ->> 'model' AS model FROM vala.gateway.calls",
    )
    .await
    {
        Err(error)
            if error
                .downcast_ref::<wyrd_client::bifrost::BifrostClientError>()
                .map(sdk_code)
                == Some("WYRD_VALA_403_QUERY_FORBIDDEN") => {}
        Err(error) => {
            return Err(format!("the sensitive Variant read failed as {error}").into());
        }
        Ok(rows) => return Err(format!("the sensitive Variant read settled {rows:?}").into()),
    }
    for (offset, index) in PEER_FOLLOWERS.into_iter().enumerate() {
        if cluster.graph_leases(index)?.0 != remote_before.leases[offset] {
            return Err(format!("the refused read leased a graph on follower {index}").into());
        }
    }

    cluster.shutdown().await?;
    Ok(())
}

/// Proves a failure after a delivered batch keeps its catalog problem.
///
/// One object read in id order streams 8192-row batches through the
/// Interactive leader, so rows from id 8192 fail in the second batch. Two
/// objects read through a bounded sort run as an Analytical graph whose
/// coordinator merges ten sorted batches before projecting them, so rows from
/// id 36864 fail only in the last batch. Each late Variant failure must match
/// the pre-stream problem exactly, an unrelated late cast failure must stay
/// the generic execution failure, and every collected result is refused
/// rather than returned partially.
///
/// # Errors
///
/// Returns the first case that settled, ran on the wrong path, failed before
/// a batch was delivered, or carried a different problem.
async fn prove_late_failures(
    cluster: &PeerCluster,
    client: &WyrdClient,
    suffix: &impl Display,
    invalid_json: &WyrdProblem,
) -> Result<(), JourneyError> {
    let interactive = format!("variant_late_one_{suffix}");
    cluster.register_table(PEER_SCRIBE, &interactive).await?;
    cluster
        .ingest_rows(PEER_SCRIBE, &interactive, 0, 10_000, 3)
        .await?;
    let analytical = format!("variant_late_two_{suffix}");
    cluster.register_table(PEER_SCRIBE, &analytical).await?;
    for _ in 0..2 {
        cluster
            .ingest_rows(PEER_SCRIBE, &analytical, 0, 40_960, 3)
            .await?;
    }
    cluster.refresh_snapshots().await?;
    let generic = wyrd_spec::error::WyrdError::from(BifrostError::QueryExecutionFailed).problem();
    let sorted =
        format!("(SELECT id FROM vala.bifrost.{analytical} ORDER BY id LIMIT 81920) AS sorted");
    for (case, path, sql, expected) in [
        (
            "Interactive late invalid JSON",
            QueryClass::Interactive,
            format!(
                "SELECT id, parse_json(CASE WHEN id < 8192 THEN '1' ELSE '{{bad' END) AS v \
                 FROM vala.bifrost.{interactive}"
            ),
            invalid_json,
        ),
        (
            "Analytical late invalid JSON",
            QueryClass::Analytical,
            format!(
                "SELECT id, parse_json(CASE WHEN id < 36864 THEN '1' ELSE '{{bad' END) AS v \
                 FROM {sorted}"
            ),
            invalid_json,
        ),
        (
            "Interactive late cast failure",
            QueryClass::Interactive,
            format!(
                "SELECT id, CAST(CASE WHEN id < 8192 THEN '1' ELSE 'x' END AS BIGINT) AS v \
                 FROM vala.bifrost.{interactive}"
            ),
            &generic,
        ),
    ] {
        let error = match wyrd_client::Bifrost::query_only(client).sql(&sql).await {
            Err(error) => error,
            Ok(result) => {
                return Err(format!("{case} returned {} rows", result.num_rows()).into());
            }
        };
        let terminal = error
            .terminal()
            .ok_or_else(|| format!("{case} failed before the stream opened: {error}"))?;
        expect_path(case, terminal.query_class, path)?;
        if terminal.row_count == 0 {
            return Err(format!("{case} failed before a batch was delivered").into());
        }
        let problem = wyrd_spec::error::WyrdError::from(&error).problem();
        if &problem != expected {
            return Err(format!("{case} failed as {problem:?}").into());
        }
    }
    Ok(())
}

/// Proves a worker's tenant refusal reaches the caller with its full problem.
///
/// Two published objects plus one hot file whose footer names a foreign
/// tenant are read through a bounded sort, so the statement runs as an
/// Analytical graph whose remote leaf stage scans the foreign file on a
/// follower. The refusal is raised on that worker and must cross the peer
/// boundary as the complete `QueryTenantInvariant` problem, with no rows
/// returned and every Oracle back at its pre-query ownership.
///
/// The refusal is accepted on either surface: the footer check fails on the
/// first poll of the partition holding the foreign file, so whether a clean
/// batch is delivered first depends on scheduling, and ordering it
/// deterministically would need a new execution hook. When a terminal frame
/// is present it must report Analytical execution.
///
/// # Errors
///
/// Returns the first claim that broke: a settled query, a different problem,
/// a non-Analytical terminal, no remote work, or an Oracle that did not
/// settle.
async fn prove_worker_tenant_refusal(
    cluster: &PeerCluster,
    client: &WyrdClient,
    suffix: &impl Display,
) -> Result<(), JourneyError> {
    let table = format!("variant_late_tenant_{suffix}");
    cluster.register_table(PEER_SCRIBE, &table).await?;
    for _ in 0..2 {
        cluster
            .ingest_rows(PEER_SCRIBE, &table, 0, 4_096, 3)
            .await?;
    }
    cluster
        .seed_foreign_hot_row(
            PEER_SCRIBE,
            &table,
            &format!("variant-late-foreign-{suffix}"),
        )
        .await?;
    cluster.refresh_snapshots().await?;
    let baseline = cluster
        .indices_of(BifrostTarget::Oracle)
        .into_iter()
        .map(|index| Ok((index, cluster.ownership_snapshot(index)?)))
        .collect::<Result<Vec<_>, JourneyError>>()?;
    let remote_before = RemoteWork::observe(cluster)?;
    let case = "worker tenant refusal";
    let sql = format!(
        "SELECT id FROM (SELECT id FROM vala.bifrost.{table} ORDER BY id LIMIT 8193) AS sorted"
    );
    let error = match wyrd_client::Bifrost::query_only(client).sql(&sql).await {
        Err(error) => error,
        Ok(result) => return Err(format!("{case} returned {} rows", result.num_rows()).into()),
    };
    if let Some(terminal) = error.terminal() {
        expect_path(case, terminal.query_class, QueryClass::Analytical)?;
    }
    let problem = wyrd_spec::error::WyrdError::from(&error).problem();
    let expected = wyrd_spec::error::WyrdError::from(BifrostError::QueryTenantInvariant).problem();
    if problem != expected {
        return Err(format!("{case} failed as {problem:?}").into());
    }
    // Every task of the scanning stage routes to one follower, so remote work
    // is proven by any follower leasing the graph.
    let mut leased = false;
    for (offset, index) in PEER_FOLLOWERS.into_iter().enumerate() {
        leased |= cluster.graph_leases(index)?.0 > remote_before.leases[offset];
    }
    if !leased {
        return Err(format!("{case} leased no graph on any follower").into());
    }
    for (index, before) in baseline {
        await_baseline(cluster, index, before).await?;
    }
    Ok(())
}

/// Requires a settled query to have run on the expected execution path.
///
/// # Errors
///
/// Returns a message naming `case` and both paths when they differ.
fn expect_path(case: &str, path: QueryClass, expected: QueryClass) -> Result<(), JourneyError> {
    if path == expected {
        Ok(())
    } else {
        Err(format!("{case}: expected {expected:?} execution, settled {path:?}").into())
    }
}

/// Runs one public query to its terminal and renders every cell as text.
///
/// Rendering through Arrow's display formatter lets one comparison cover
/// strings, integers, and booleans without a per-column downcast.
///
/// # Errors
///
/// Returns the SDK error unchanged so a caller can read its catalog code, or a
/// message when the stream ends without a terminal or a cell cannot render.
async fn run_rows(
    client: &WyrdClient,
    sql: &str,
) -> Result<(QueryClass, Vec<Vec<String>>), JourneyError> {
    let mut stream = wyrd_client::Bifrost::query_only(client)
        .query(&BifrostQueryRequest {
            sql: sql.to_owned(),
            deadline_ms: Some(30_000),
        })
        .await?;
    let mut rows = Vec::new();
    while let Some(batch) = stream.next_batch().await? {
        rows.extend(render_rows(&batch)?);
    }
    let terminal = stream
        .terminal()
        .ok_or("public query produced no terminal frame")?;
    Ok((terminal.query_class, rows))
}

/// Renders every cell of one batch with Arrow's display formatter.
///
/// # Errors
///
/// Returns the formatter error for a column it cannot render.
fn render_rows(batch: &RecordBatch) -> Result<Vec<Vec<String>>, JourneyError> {
    (0..batch.num_rows())
        .map(|row| {
            batch
                .columns()
                .iter()
                .map(|column| Ok(arrow::util::display::array_value_to_string(column, row)?))
                .collect()
        })
        .collect()
}

/// Rows each sealed file of the pushdown journey holds.
const PUSHDOWN_ROWS_PER_FILE: i64 = 2_000;

/// The key every pushdown filter looks up; it lives in the second file.
const PUSHDOWN_PROBE: i64 = PUSHDOWN_ROWS_PER_FILE + 500;

/// Forge row-group target for the pushdown table, small enough that the
/// rewrite of its three files closes several row groups.
const PUSHDOWN_ROW_GROUP_BYTES: u64 = 16 * 1024;

/// A Struct field and a shredded Variant path share one physical pushdown on
/// hot, promoted, and rewritten files.
///
/// One table carries the same key `a` twice: as Struct field `s.a` and as
/// Variant path `doc.a`, which Scribe shreds into a typed integer leaf. Three
/// files hold disjoint key ranges. In each cut a Struct filter and a Variant
/// filter on that key return the same exact row and the same physical
/// evidence, an unshredded Variant key reads null and matches nothing, and the
/// full content, Variant values included, equals what the hot objects return,
/// so Forge's promoted and rewritten files read identically to Scribe's:
///
/// - **hot** — the hot reader's per-file plan prunes the other files' row
///   groups by footer statistics;
/// - **promoted** — the same files, now read by the Iceberg reader, are
///   pruned identically through the published per-file plan;
/// - **rewritten** — one Forge output holds several row groups, and both
///   filters prune the same ones.
///
/// # Errors
///
/// Returns cluster, registration, write, catalog, Forge, telemetry, or query
/// errors, or a description of the first expectation that does not hold.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn struct_and_variant_share_physical_pushdown() -> Result<(), JourneyError> {
    // Promotion is parked at the commit seam before any write so the first
    // cut stays hot; see `prove_published_governance`.
    let cluster = WyrdTestCluster::start_spec_with_forge_config_and_completion_observer(
        BifrostClusterSpec::one_mixed(),
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
    let tenant = cluster.data_tenant_id();
    let table = unique_table("oracle_pushdown");
    let fqn = format!("vala.bifrost.{table}");
    let schema: SchemaRef = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new(
            "s",
            DataType::Struct(Fields::from(vec![Field::new("a", DataType::Int64, true)])),
            true,
        ),
        wyrd_types::variant::variant_field("doc", true),
    ]));
    register_table_with(
        server,
        tenant,
        &table,
        schema
            .fields()
            .iter()
            .map(|field| field.as_ref().clone())
            .collect(),
    )
    .await?;
    // Compaction stays off until the rewritten cut, so the promoted cut is
    // measured exactly as Scribe sealed it.
    let catalog = server
        .state()
        .bifrost_catalog()
        .ok_or("Scribe composition retains the shared catalog")?
        .iceberg_catalog();
    let binding =
        TenantTableBinding::resolve((tenant, TableRef::new(BifrostNamespace::Bifrost, &table)))?;
    set_table_property(&catalog, &binding, "wyrd.forge.enable-compaction", "false").await?;
    set_table_property(
        &catalog,
        &binding,
        iceberg::spec::TableProperties::PROPERTY_PARQUET_ROW_GROUP_SIZE_BYTES,
        &PUSHDOWN_ROW_GROUP_BYTES.to_string(),
    )
    .await?;
    let rows = writer(server, "pushdown-writer").await?;
    for file in 0..3 {
        let ids = file * PUSHDOWN_ROWS_PER_FILE..(file + 1) * PUSHDOWN_ROWS_PER_FILE;
        rows.write(
            &fqn,
            &schema,
            ids.map(|id| {
                serde_json::json!({"id": id, "s": {"a": id}, "doc": {"a": id}})
                    .to_string()
                    .into_bytes()
            }),
        )
        .await?;
        server.flush_bifrost().await?;
    }
    let reader = client(server, "pushdown-reader").await?;

    tokio::time::timeout(Duration::from_secs(30), promotion.wait_for_before_commit())
        .await
        .map_err(|_| "the first promotion never reached the parked commit")?;
    cluster.refresh_oracle_snapshots().await?;
    if file_tier_counts(&cluster, tenant, &table).await? != (0, 3) {
        return Err("the hot cut must hold three unpromoted files".into());
    }
    prove_pushdown_cut(&cluster, &reader, &fqn, PushdownCut::Hot).await?;
    // Every later cut must return exactly the content Scribe's hot objects
    // return, Variant values included.
    let contents = format!(
        "SELECT id, s['a'], variant_as_text(doc) FROM {fqn} WHERE id < {} ORDER BY id",
        3 * PUSHDOWN_ROWS_PER_FILE
    );
    let (_, hot_rows) = run_rows(&reader, &contents).await?;
    if hot_rows.len() != usize::try_from(3 * PUSHDOWN_ROWS_PER_FILE)? {
        return Err(format!("the hot cut returned {} rows", hot_rows.len()).into());
    }

    promotion.release_paused_before_commit();
    compact_sealed_batch(&cluster, tenant, &table, 3).await?;
    cluster.refresh_oracle_snapshots().await?;
    let promoted = planned_paths(server, tenant, BifrostNamespace::Bifrost, &table).await?;
    if promoted.len() != 3
        || promoted
            .iter()
            .any(|path| path.contains(FORGE_DATA_SEGMENT))
    {
        return Err(
            format!("the promoted cut must plan the three Scribe files: {promoted:?}").into(),
        );
    }
    prove_pushdown_cut(&cluster, &reader, &fqn, PushdownCut::Promoted).await?;
    if run_rows(&reader, &contents).await?.1 != hot_rows {
        return Err("the promoted cut's content differs from the hot cut's".into());
    }

    // Forge learns a table's settings from its own commits, so compaction is
    // re-enabled and one more file, outside the probe's range, is promoted
    // to carry that setting into the schedule.
    set_table_property(&catalog, &binding, "wyrd.forge.enable-compaction", "true").await?;
    let extra = 3 * PUSHDOWN_ROWS_PER_FILE;
    rows.write(
        &fqn,
        &schema,
        [
            serde_json::json!({"id": extra, "s": {"a": extra}, "doc": {"a": extra}})
                .to_string()
                .into_bytes(),
        ],
    )
    .await?;
    server.flush_bifrost().await?;
    compact_sealed_batch(&cluster, tenant, &table, 4).await?;
    await_rewrite(
        &cluster,
        server,
        &[(BifrostNamespace::Bifrost, table.as_str())],
    )
    .await?;
    cluster.refresh_oracle_snapshots().await?;
    let rewritten = planned_paths(server, tenant, BifrostNamespace::Bifrost, &table).await?;
    if rewritten.len() != 1 {
        return Err(format!("the rewrite must publish one output: {rewritten:?}").into());
    }
    prove_pushdown_cut(&cluster, &reader, &fqn, PushdownCut::Rewritten).await?;
    if run_rows(&reader, &contents).await?.1 != hot_rows {
        return Err("the rewritten cut's content differs from the hot cut's".into());
    }
    cluster.shutdown().await?;
    Ok(())
}

/// The physical cut one pushdown phase reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PushdownCut {
    /// Three Scribe hot objects.
    Hot,
    /// The same three objects promoted into Iceberg unchanged.
    Promoted,
    /// One Forge output replacing them.
    Rewritten,
}

/// Runs the Struct, Variant, and unshredded-key filters against one cut and
/// asserts their rows and pruning evidence.
///
/// # Errors
///
/// Returns query or telemetry errors, and an error naming the first case whose
/// rows or evidence differ from the cut's expectation.
async fn prove_pushdown_cut(
    cluster: &WyrdTestCluster,
    reader: &WyrdClient,
    fqn: &str,
    cut: PushdownCut,
) -> Result<(), JourneyError> {
    let case = |name: &str, filter: &str, expected: Vec<i64>| FilterCase {
        name: format!("{cut:?} {name}"),
        sql: format!("SELECT id FROM {fqn} WHERE {filter} ORDER BY id"),
        expected,
    };
    let structs = run_case(
        cluster,
        reader,
        case(
            "struct",
            &format!("s['a'] = {PUSHDOWN_PROBE}"),
            vec![PUSHDOWN_PROBE],
        ),
    )
    .await?;
    let variant = run_case(
        cluster,
        reader,
        case(
            "variant",
            &format!("CAST(doc->>'a' AS BIGINT) = {PUSHDOWN_PROBE}"),
            vec![PUSHDOWN_PROBE],
        ),
    )
    .await?;
    run_case(
        cluster,
        reader,
        case(
            "unshredded key",
            "CAST(doc->>'zz' AS BIGINT) = 1",
            Vec::new(),
        ),
    )
    .await?;
    // Hot and promoted: all three files open and the per-file plan drops the
    // other two files' row groups. Rewritten: one output whose row groups
    // outside the probe are dropped.
    let files = match cut {
        PushdownCut::Hot | PushdownCut::Promoted => 3.0,
        PushdownCut::Rewritten => 1.0,
    };
    if structs.row_groups_pruned < 2.0 {
        return Err(format!("{cut:?}: the Struct filter pruned too little: {structs:?}").into());
    }
    for (name, evidence) in [("struct", structs), ("variant", variant)] {
        expect_measure(name, "files", evidence.files, files)?;
        expect_measure(
            name,
            "pruned",
            evidence.row_groups_pruned,
            structs.row_groups_pruned,
        )?;
    }
    Ok(())
}
