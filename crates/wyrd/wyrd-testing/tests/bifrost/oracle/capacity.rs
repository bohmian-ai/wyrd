//! Oracle journeys — admission contention at the lowest supported Oracle rung.
//!
//! One production-shaped run proves that the retained admission owner stays
//! safe *and* useful when a real memory-heavy Analytical query already holds a
//! query envelope, a distributed graph, and live spill ownership on the exact
//! 512 MiB Oracle-only memory floor: bounded Interactive queries from two
//! separate tenants still complete through the real public client, every query
//! pool stays inside the grant it was issued, root admission accounting stays
//! inside the managed budget, and every owner returns to baseline.
//!
//! Module of the `oracle` binary; see `main.rs` for the capability it proves
//! and `support.rs` for the fixtures it shares.

use std::sync::Arc;
use std::time::Duration;

use arrow::array::{Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use futures_util::StreamExt as _;
use sha2::{Digest as _, Sha256};
use vala_bifrost_redux::oracle::{QueryIpcDecoder, QueryResourceProbe, QueryResourceSnapshot};
use vala_bifrost_redux::resources::{ResourceSource, SystemResourceSnapshot};
use vala_sdk::QueryClient;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{
    BifrostQueryRequest, FreshnessPolicy, QueryStreamFrame, QueryTerminalOutcome, VisibilityMode,
};
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::telemetry::BifrostMetricSample;
use wyrd_testing::bifrost::{BifrostClusterSpec, WyrdTestCluster};

use crate::support::*;

/// Exact Oracle-only process memory floor this qualification runs at.
///
/// The lowest observation `BifrostRuntimeResources` accepts for an Oracle-only
/// pod. It is an injected observation, not an OS limit, so this journey bounds
/// what admission *accounts for* and makes no aggregate physical-memory claim.
const ORACLE_MEMORY_FLOOR_BYTES: usize = 512 * 1024 * 1024;

/// Effective CPU injected on every Oracle-only node.
const ORACLE_EFFECTIVE_CPU: usize = 2;

/// Scratch capacity and availability injected on every Oracle-only node.
///
/// Large enough that the Analytical query's spill is bounded by its admitted
/// scratch envelope rather than by a fixture-sized filesystem.
const ORACLE_SCRATCH_BYTES: u64 = 1024 * 1024 * 1024;

/// Rows in each tenant's bounded Interactive table.
///
/// Small enough to stay a flat sub-second scan admission classifies
/// Interactive, and large enough that its encoded result cannot fit in the
/// transport's buffers. The second half is what makes the observation window
/// real rather than racy: a result that fits in a socket buffer lets the server
/// finish and release the query before the client has sampled anything, so the
/// window would observe an already-completed query. This size makes the
/// undrained client the thing holding the query open.
const INTERACTIVE_ROWS: i64 = 200_000;

/// Distinct `filter_key` groups in a bounded Interactive table.
const INTERACTIVE_GROUPS: i64 = 1_000;

/// Rows per fixture ingest request.
///
/// One request per table would hold a whole fixture table as a single Arrow
/// batch in the Scribe's address space, which no real writer does.
const INGEST_CHUNK: i64 = 100_000;

/// Distinct `filter_key` groups the Analytical fixture rows fall into.
///
/// Irrelevant to the statement under test, which derives its own key from `id`;
/// it only keeps the published files from being one trivial group.
const ANALYTICAL_INGEST_GROUPS: i64 = 1_000;

/// Absolute deadline for the long-lived Analytical stream, in milliseconds.
///
/// It stays open across two complete Interactive windows, so its deadline has
/// to cover them; it is still finite, because a stream that outlives the
/// journey is a leak rather than a pass.
const ANALYTICAL_DEADLINE_MS: u64 = 600_000;

/// Absolute deadline for each bounded public Interactive query.
const INTERACTIVE_DEADLINE_MS: u64 = 60_000;

/// Bound on frame pulls used to bring the Analytical query to live ownership.
///
/// The stream is driven rather than slept on: each pull is the only thing that
/// advances execution, so a bound on pulls is a bound on the wait.
const ANALYTICAL_WARMUP_FRAMES: usize = 4096;

/// Bound on how long one awaited observation may take before it is a failure.
const OBSERVATION_DEADLINE: Duration = Duration::from_secs(60);

/// Bound on polls waiting for the settled graph to publish its spill evidence.
const PHYSICAL_EVIDENCE_POLLS: usize = 300;

/// Interval between polls for settled physical evidence.
const PHYSICAL_EVIDENCE_INTERVAL: Duration = Duration::from_millis(100);

/// A live memory-heavy Analytical query does not stop two tenants' bounded
/// Interactive queries from being admitted and completing on the lowest
/// supported Oracle rung.
///
/// # Panics
///
/// Panics when any admission, memory, spill, telemetry, or cleanup claim fails.
// Four pods, a distributed join, and two client streams share this runtime. On
// the single-threaded default the executing query starves the pods' own
// heartbeats, their Oracle role leases expire, and the graph fails for a reason
// no production deployment would ever produce.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn lowest_rung_analytical_contention_preserves_two_interactive_tenants() {
    prove_lowest_rung_contention()
        .await
        .expect("lowest-rung Oracle contention journey");
}

/// Drives the complete contention journey over one live four-node cluster.
///
/// # Errors
///
/// Returns the first claim that broke.
async fn prove_lowest_rung_contention() -> Result<(), JourneyError> {
    let cluster = WyrdTestCluster::start_spec(
        BifrostClusterSpec::three_oracles_one_scribe()
            .with_system_resources(oracle_floor_observation()),
    )
    .await?;
    let tenant_a = cluster.data_tenant_id();
    let tenant_b = cluster.add_tenant("contention-peer").await?;
    let ingest = cluster
        .servers()
        .find(|server| server.bifrost_scribe().is_some())
        .ok_or("missing ingest node")?;
    let coordinator = cluster.server(0).ok_or("missing coordinator node")?;

    let suffix = uuid::Uuid::now_v7().simple();
    let left = format!("contention_left_{suffix}");
    let right = format!("contention_right_{suffix}");
    let ui_a = format!("contention_ui_a_{suffix}");
    let ui_b = format!("contention_ui_b_{suffix}");
    seed_fixture_table(
        ingest,
        tenant_a,
        &left,
        ANALYTICAL_LEFT_ROWS,
        ANALYTICAL_INGEST_GROUPS,
    )
    .await?;
    seed_fixture_table(
        ingest,
        tenant_a,
        &right,
        ANALYTICAL_RIGHT_ROWS,
        ANALYTICAL_INGEST_GROUPS,
    )
    .await?;
    seed_fixture_table(
        ingest,
        tenant_a,
        &ui_a,
        INTERACTIVE_ROWS,
        INTERACTIVE_GROUPS,
    )
    .await?;
    seed_fixture_table(
        ingest,
        tenant_b,
        &ui_b,
        INTERACTIVE_ROWS,
        INTERACTIVE_GROUPS,
    )
    .await?;
    cluster.refresh_oracle_snapshots().await?;

    let journey_checkpoint = cluster
        .telemetry()
        .checkpoint()
        .map_err(|e| e.to_string())?;
    let baseline = ownership_baseline(&cluster).await?;
    // Sampled at baseline: the same acquisition against a saturated root would
    // be refused, so the grant is read while the envelope is genuinely free.
    let expected_grant = expected_analytical_grant(coordinator, ORACLE_MEMORY_FLOOR_BYTES)?;

    let engine = Arc::clone(
        coordinator
            .state()
            .bifrost_query()
            .ok_or("coordinator composed no Oracle")?
            .engine(),
    );
    let mut analytical = engine
        .query_sql_inactive_analytical(
            query_context(tenant_a)?,
            BifrostQueryRequest {
                sql: analytical_baseline_sql(&left, &right),
                visibility: VisibilityMode::PublishedOnly,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(ANALYTICAL_DEADLINE_MS),
            },
            attempt_context(),
        )
        .await?;
    let analytical_probe = analytical.resource_probe_for_test();

    // Driven, not slept on: pulling a frame is what advances execution, so the
    // stream reaches live ownership through its own progress and is then simply
    // left alone with that ownership held.
    let mut fold = BaselineFold::default();
    let mut decoder = QueryIpcDecoder::new();
    let mut live_analytical = None;
    for _ in 0..ANALYTICAL_WARMUP_FRAMES {
        if let Some(observed) = live_analytical_ownership(coordinator, &analytical_probe)? {
            live_analytical = Some(observed);
            break;
        }
        let frame = analytical
            .frames
            .next()
            .await
            .ok_or("the Analytical stream ended before it owned live resources")??;
        accept_frame(&mut decoder, &mut fold, frame)?;
    }
    let live_analytical =
        live_analytical.ok_or("the Analytical query never reported live envelope ownership")?;
    if live_analytical.granted_memory_bytes != expected_grant {
        return Err(format!(
            "the Analytical grant {} was not derived from the injected {ORACLE_MEMORY_FLOOR_BYTES}-byte observation",
            live_analytical.granted_memory_bytes
        )
        .into());
    }

    // Serial by construction: the fault controller owns exactly one next-query
    // slot, so each tenant's capture is armed, awaited, and drained on its own
    // while the Analytical stream above keeps its envelope the whole time.
    let mut interactive = Vec::new();
    for (tenant, name, table) in [
        (tenant_a, "contention-a", &ui_a),
        (tenant_b, "contention-b", &ui_b),
    ] {
        interactive.push(
            prove_interactive_window(
                &cluster,
                coordinator,
                &analytical_probe,
                tenant,
                name,
                table,
            )
            .await?,
        );
    }

    while let Some(frame) = analytical.frames.next().await {
        accept_frame(&mut decoder, &mut fold, frame?)?;
    }
    if fold.rows != u64::try_from(ANALYTICAL_RIGHT_ROWS)? {
        return Err(format!(
            "the Analytical query returned {} rows, not {ANALYTICAL_RIGHT_ROWS}",
            fold.rows
        )
        .into());
    }
    if fold.digest() != expected_analytical_digest() {
        return Err("the Analytical query returned a result the fixture never produced".into());
    }
    let terminal = fold
        .terminal
        .ok_or("the Analytical stream emitted no terminal frame")?;
    if terminal != QueryTerminalOutcome::Success {
        return Err(format!("the Analytical query settled as {terminal:?}, not a success").into());
    }
    prove_spill_evidence(&engine).await?;

    let analytical_final = analytical_probe.snapshot();
    for (label, live, final_snapshot) in [
        ("analytical", live_analytical, analytical_final),
        ("interactive tenant A", interactive[0].0, interactive[0].1),
        ("interactive tenant B", interactive[1].0, interactive[1].1),
    ] {
        prove_pool_within_grant(label, live, final_snapshot)?;
    }

    await_clean_nodes(&cluster).await?;
    let settled = ownership_baseline(&cluster).await?;
    if settled != baseline {
        return Err(format!(
            "Oracle ownership did not return to baseline: {baseline:?} then {settled:?}"
        )
        .into());
    }
    let delta = cluster
        .telemetry()
        .delta_since(&journey_checkpoint)
        .map_err(|e| e.to_string())?;
    prove_labels_are_bounded(&delta, &[tenant_a, tenant_b])?;

    cluster.shutdown().await?;
    Ok(())
}

/// The exact process observation injected on every Oracle-only node.
fn oracle_floor_observation() -> SystemResourceSnapshot {
    SystemResourceSnapshot {
        memory_limit_bytes: ORACLE_MEMORY_FLOOR_BYTES,
        effective_cpu: ORACLE_EFFECTIVE_CPU,
        scratch_capacity_bytes: ORACLE_SCRATCH_BYTES,
        scratch_available_bytes: ORACLE_SCRATCH_BYTES,
        memory_source: ResourceSource::Injected,
        cpu_source: ResourceSource::Injected,
    }
}

/// Runs one tenant's complete bounded Interactive window while Analytical work
/// holds its envelope, returning the query's live and settled observations.
///
/// The capture is passive: it reads the query's own resource probe and hands
/// the stream straight back, so every byte of the result still crosses the real
/// public client. Telemetry is sampled while the client still owns the stream,
/// because an active-query gauge read after the drain proves nothing.
///
/// `analytical_probe` is the still-executing Analytical query's probe. Its
/// ownership is re-read in the same window as the Interactive gauge sample:
/// the point of the window is contention, and an Analytical query that had
/// already drained would leave the floor uncontended and the observation void.
///
/// # Errors
///
/// Returns the first admission, contention, telemetry, terminal, or readiness
/// claim that broke, naming the tenant's client.
async fn prove_interactive_window(
    cluster: &WyrdTestCluster,
    coordinator: &WyrdTestServer,
    analytical_probe: &Arc<QueryResourceProbe>,
    tenant: DataTenantId,
    name: &str,
    table: &str,
) -> Result<(QueryResourceSnapshot, QueryResourceSnapshot), JourneyError> {
    let client = client_for_tenant(coordinator, tenant, name).await?;
    let checkpoint = cluster
        .telemetry()
        .checkpoint()
        .map_err(|e| e.to_string())?;
    let active_before = class_gauge(cluster, "interactive")?;
    let capture = coordinator.capture_next_query_resource_probe();

    let mut stream = QueryClient::new(&client)
        .query(&BifrostQueryRequest {
            // A flat bounded projection on purpose. Any global operator — a
            // sort, an aggregate, a join — is a "complex" plan, which the
            // planner classifies Analytical however few rows it reads, and an
            // Analytical query would never reach the Interactive floor this
            // window exists to observe. Rows are counted, not ordered.
            sql: format!("SELECT id, filter_key FROM vala.bifrost.{table}"),
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: Some(INTERACTIVE_DEADLINE_MS),
        })
        .await?;
    let probe = tokio::time::timeout(OBSERVATION_DEADLINE, capture.wait_resource_probe())
        .await
        .map_err(|_| format!("{name} never bound its query resource probe"))?;

    let active_during = class_gauge(cluster, "interactive")?;
    if (active_during - active_before - 1.0).abs() > f64::EPSILON {
        return Err(format!(
            "{name} moved oracle_queries_active{{class=\"interactive\"}} from \
             {active_before} to {active_during}, not by exactly one"
        )
        .into());
    }
    // Contemporaneous with the gauge sample above: admission active, the
    // Analytical graph live, and its grant and pool still held.
    live_analytical_ownership(coordinator, analytical_probe)?.ok_or_else(|| {
        format!("{name} was admitted after the Analytical query had already released its envelope")
    })?;
    let live = probe.snapshot();
    if live.granted_memory_bytes == 0 {
        return Err(format!("{name} was admitted without an issued memory grant").into());
    }
    prove_roots_within_budget(cluster, name)?;
    prove_nodes_ready(cluster, name).await?;

    let mut rows = 0_u64;
    while let Some(batch) = stream.next_batch().await? {
        rows = rows.saturating_add(u64::try_from(batch.num_rows())?);
    }
    let terminal = stream.terminal().ok_or("query terminal missing")?;
    if terminal.outcome != QueryTerminalOutcome::Success || terminal.error.is_some() {
        return Err(format!(
            "{name} settled as {:?} with {:?}",
            terminal.outcome, terminal.error
        )
        .into());
    }
    if rows != u64::try_from(INTERACTIVE_ROWS)? || terminal.row_count != rows {
        return Err(format!(
            "{name} returned {rows} rows against a {} terminal, not {INTERACTIVE_ROWS}",
            terminal.row_count
        )
        .into());
    }

    let delta = cluster
        .telemetry()
        .delta_since(&checkpoint)
        .map_err(|e| e.to_string())?;
    let admitted = labelled_sum(
        &delta,
        "oracle_admission_total",
        &[("class", "interactive"), ("outcome", "admitted")],
    );
    if (admitted - 1.0).abs() > f64::EPSILON {
        return Err(format!("{name} recorded {admitted} interactive admissions, not one").into());
    }
    let analytical_admitted = labelled_sum(
        &delta,
        "oracle_admission_total",
        &[("class", "analytical"), ("outcome", "admitted")],
    );
    if analytical_admitted != 0.0 {
        return Err(format!(
            "{name}'s window recorded {analytical_admitted} analytical admissions"
        )
        .into());
    }
    Ok((live, probe.snapshot()))
}

/// Asserts one query's pool never exceeded the grant admission issued it and
/// that the settled query released all of it.
///
/// # Errors
///
/// Returns an error when the observed peak exceeds the grant, when current
/// exceeds peak, when a query that ran reported no peak at all, or when the
/// settled query still holds pool memory.
fn prove_pool_within_grant(
    label: &str,
    live: QueryResourceSnapshot,
    settled: QueryResourceSnapshot,
) -> Result<(), JourneyError> {
    for snapshot in [live, settled] {
        if snapshot.pool_current_bytes > snapshot.pool_peak_bytes
            || snapshot.pool_peak_bytes > snapshot.granted_memory_bytes
        {
            return Err(format!(
                "{label} violated current <= peak <= grant: {} <= {} <= {}",
                snapshot.pool_current_bytes,
                snapshot.pool_peak_bytes,
                snapshot.granted_memory_bytes
            )
            .into());
        }
    }
    if settled.pool_peak_bytes == 0 {
        return Err(format!("{label} reported no pool reservation at all").into());
    }
    // A settled query holds nothing. Peak and grant bound a leak only in
    // relative terms; the release itself is the absolute claim.
    if settled.pool_current_bytes != 0 {
        return Err(format!(
            "{label} still holds {} pool bytes after settling",
            settled.pool_current_bytes
        )
        .into());
    }
    Ok(())
}

/// Asserts every Oracle root's live ownership stays inside its managed budget.
///
/// # Errors
///
/// Returns an error when any node's Scribe, Oracle, and Forge ownership
/// together exceed the memory the plan governs.
fn prove_roots_within_budget(cluster: &WyrdTestCluster, label: &str) -> Result<(), JourneyError> {
    for snapshot in cluster.oracle_resource_snapshots()? {
        // Forge holds no live root lease: its compaction budget is reserved
        // once during plan calculation, so the plan's figure is its whole
        // standing claim on the managed budget.
        let held = snapshot.scribe_memory_used_bytes
            + snapshot.oracle_memory_used_bytes
            + snapshot.plan.forge_compaction_memory_limit_bytes;
        if held > snapshot.plan.managed_memory_bytes {
            return Err(format!(
                "during {label} one Oracle root held {held} bytes against a managed budget of {}",
                snapshot.plan.managed_memory_bytes
            )
            .into());
        }
    }
    Ok(())
}

/// Asserts every configured node is still a ready member of the cluster.
///
/// # Errors
///
/// Returns an error when a membership row reports a node as not ready.
async fn prove_nodes_ready(cluster: &WyrdTestCluster, label: &str) -> Result<(), JourneyError> {
    let inspection = cluster.oracle_inspection().await?;
    if let Some(down) = inspection
        .memberships
        .iter()
        .find(|membership| !membership.ready)
    {
        return Err(format!(
            "during {label} node {} was not ready",
            down.node_id.as_uuid()
        )
        .into());
    }
    Ok(())
}

/// Asserts the executed graph published real operator spill evidence.
///
/// The terminal reaches the caller before the graph's own lifecycle settles,
/// and the physical fold happens inside that settlement, so this waits for the
/// evidence rather than asserting on whatever the race left behind.
///
/// # Errors
///
/// Returns an error when no Analytical handle exists, no evidence settles
/// within the bound, or the settled evidence reports no spill.
async fn prove_spill_evidence(
    engine: &Arc<vala_bifrost_redux::oracle::Oracle>,
) -> Result<(), JourneyError> {
    let supervisor = engine
        .analytical_execution()
        .ok_or("Oracle composed no Analytical handle")?
        .supervisor()
        .clone();
    for _ in 0..PHYSICAL_EVIDENCE_POLLS {
        if let Some(evidence) = supervisor.settled_physical_evidence() {
            if evidence.spill_count == 0
                || evidence.spilled_bytes == 0
                || evidence.spilled_rows == 0
            {
                return Err(format!(
                    "the Analytical query reported no spill: {} spills, {} bytes, {} rows",
                    evidence.spill_count, evidence.spilled_bytes, evidence.spilled_rows
                )
                .into());
            }
            return Ok(());
        }
        tokio::time::sleep(PHYSICAL_EVIDENCE_INTERVAL).await;
    }
    Err("the settled Analytical graph published no physical evidence".into())
}

/// Asserts no production metric label carries a tenant identity.
///
/// # Errors
///
/// Returns an error naming the first series whose labels leak a tenant.
fn prove_labels_are_bounded(
    delta: &wyrd_testing::bifrost::telemetry::BifrostTelemetryDelta,
    tenants: &[DataTenantId],
) -> Result<(), JourneyError> {
    let forbidden: Vec<String> = tenants.iter().map(ToString::to_string).collect();
    for sample in delta.metrics.iter().chain(delta.gauge_final.iter()) {
        for value in sample.labels.values() {
            if forbidden.iter().any(|tenant| value == tenant) {
                return Err(format!(
                    "{} carried a tenant identity as a metric label",
                    sample.family
                )
                .into());
            }
        }
    }
    Ok(())
}

/// The live-ownership half of one cluster inspection.
///
/// Compared rather than the whole inspection because audit, Forge history, and
/// telemetry inventories are monotone: including them would make a baseline
/// comparison fail for reasons that have nothing to do with released ownership.
#[derive(Debug, PartialEq, Eq)]
struct OwnershipBaseline {
    /// Admitted Oracle queries across every node.
    active_queries: u64,
    /// Queued Oracle admission waiters across every node.
    queued_queries: u64,
    /// Query-grant memory retained across every node.
    reserved_memory_bytes: u64,
    /// Query-grant scratch retained across every node.
    reserved_spill_bytes: u64,
    /// Retained Oracle spill directories.
    spill_directories: u64,
    /// Retained Oracle spill files.
    spill_files: u64,
    /// Bytes held by retained Oracle spill files.
    spill_file_bytes: u64,
    /// Pending peer reservations across every node.
    peer_pending: u64,
    /// Running peer reservations across every node.
    peer_running: u64,
    /// Live-tail fences retained by every Scribe.
    active_tail_fences: u64,
}

/// Projects the live-ownership half of the cluster's own inspection.
///
/// # Errors
///
/// Returns the inspection error unchanged.
async fn ownership_baseline(cluster: &WyrdTestCluster) -> Result<OwnershipBaseline, JourneyError> {
    let inspection = cluster.oracle_inspection().await?;
    Ok(OwnershipBaseline {
        active_queries: inspection.active_queries,
        queued_queries: inspection.queued_queries,
        reserved_memory_bytes: inspection.reserved_memory_bytes,
        reserved_spill_bytes: inspection.reserved_spill_bytes,
        spill_directories: inspection.spill_directories,
        spill_files: inspection.spill_files,
        spill_file_bytes: inspection.spill_file_bytes,
        peer_pending: inspection.peer_pending,
        peer_running: inspection.peer_running,
        active_tail_fences: inspection.active_tail_fences,
    })
}

/// Reports the Analytical query's observation once every owner is live.
///
/// Live means all four at once: the node's admission owner counts the query,
/// its Analytical handle owns a graph, admission issued a memory grant, and the
/// query's own pool holds bytes. Any one of them alone could be true of a query
/// that had not yet started executing.
///
/// # Errors
///
/// Returns an error when the coordinator hosts no Oracle or its ownership lock
/// is poisoned.
fn live_analytical_ownership(
    coordinator: &WyrdTestServer,
    probe: &Arc<QueryResourceProbe>,
) -> Result<Option<QueryResourceSnapshot>, JourneyError> {
    if coordinator.oracle_runtime_inspection()?.active_queries == 0 {
        return Ok(None);
    }
    let engine = coordinator
        .state()
        .bifrost_query()
        .ok_or("coordinator composed no Oracle")?
        .engine();
    let handle = engine
        .analytical_execution()
        .ok_or("Oracle composed no Analytical handle")?;
    if handle.live()?.is_clean() {
        return Ok(None);
    }
    let snapshot = probe.snapshot();
    if snapshot.granted_memory_bytes == 0 || snapshot.pool_current_bytes == 0 {
        return Ok(None);
    }
    Ok(Some(snapshot))
}

/// Derives the grant admission issues an Analytical query on this node.
///
/// Taken from the live resource plan by acquiring and immediately dropping one
/// envelope, so it is the arithmetic admission will apply rather than a
/// restatement of a constant, and it is checked against the injected
/// observation the node actually booted with.
///
/// # Errors
///
/// Returns an error when the node hosts no Oracle, its plan does not match the
/// injected observation, or the plan admits no Analytical query.
fn expected_analytical_grant(
    coordinator: &WyrdTestServer,
    memory_limit_bytes: usize,
) -> Result<u64, JourneyError> {
    let resources = coordinator
        .state()
        .bifrost_resources()
        .ok_or("coordinator composed no Bifrost resources")?;
    if resources.plan().memory_limit_bytes != memory_limit_bytes {
        return Err(format!(
            "the coordinator booted at {} bytes, not the injected {memory_limit_bytes}",
            resources.plan().memory_limit_bytes
        )
        .into());
    }
    let oracle = resources.oracle().ok_or("coordinator composed no Oracle")?;
    let envelope = oracle.try_acquire_query(
        vala_bifrost_redux::resources::OracleResourceRequest::for_class(
            wyrd_spec::vala::api::QueryClass::Analytical,
            0.0,
        ),
    )?;
    let granted = u64::try_from(envelope.granted_memory_bytes)?;
    drop(envelope);
    Ok(granted)
}

/// Sums one class-labelled Oracle gauge across the whole live cluster.
///
/// # Errors
///
/// Returns the telemetry parse error unchanged.
fn class_gauge(cluster: &WyrdTestCluster, class: &str) -> Result<f64, JourneyError> {
    let samples = cluster.telemetry().snapshot().map_err(|e| e.to_string())?;
    Ok(samples
        .iter()
        .filter(|sample| sample.family == "oracle_queries_active")
        .filter(|sample| sample.labels.get("class").is_some_and(|held| held == class))
        .map(|sample: &BifrostMetricSample| sample.value)
        .sum())
}

/// Sums one production metric family restricted to an exact label set.
fn labelled_sum(
    delta: &wyrd_testing::bifrost::telemetry::BifrostTelemetryDelta,
    family: &str,
    labels: &[(&str, &str)],
) -> f64 {
    delta
        .metrics
        .iter()
        .filter(|sample| sample.family == family)
        .filter(|sample| {
            labels
                .iter()
                .all(|(key, value)| sample.labels.get(*key).is_some_and(|held| held == value))
        })
        .map(|sample| sample.value)
        .sum()
}

/// Folds the Analytical result into a digest without retaining it.
///
/// The baseline result is a third of a gigabyte of keys; holding it to re-walk
/// later would change the very memory behavior this journey observes.
struct BaselineFold {
    /// Running digest over every ordered `(key, count)` pair.
    digest: Sha256,
    /// Rows accepted across every batch.
    rows: u64,
    /// Terminal outcome the stream published, if it published one.
    terminal: Option<QueryTerminalOutcome>,
}

impl Default for BaselineFold {
    /// Starts empty, before any frame has been accepted.
    fn default() -> Self {
        Self {
            digest: Sha256::new(),
            rows: 0,
            terminal: None,
        }
    }
}

impl BaselineFold {
    /// Accepts one decoded `(Utf8, Int64)` batch into the running digest.
    fn accept(&mut self, batch: &RecordBatch) {
        let (Some(keys), Some(counts)) = (
            batch.column(0).as_any().downcast_ref::<StringArray>(),
            batch.column(1).as_any().downcast_ref::<Int64Array>(),
        ) else {
            return;
        };
        for row in 0..batch.num_rows() {
            let key = keys.value(row);
            self.digest
                .update(u32::try_from(key.len()).unwrap_or(u32::MAX).to_le_bytes());
            self.digest.update(key.as_bytes());
            self.digest.update(counts.value(row).to_le_bytes());
            self.rows = self.rows.saturating_add(1);
        }
    }

    /// Returns the hex digest of everything accepted so far.
    fn digest(&self) -> String {
        format!("{:x}", self.digest.clone().finalize())
    }
}

/// Routes one stream frame into the decoder and the running fold.
///
/// # Errors
///
/// Returns the Arrow IPC decode error unchanged.
fn accept_frame(
    decoder: &mut QueryIpcDecoder,
    fold: &mut BaselineFold,
    frame: QueryStreamFrame,
) -> Result<(), JourneyError> {
    match frame {
        QueryStreamFrame::Schema(schema) => {
            decoder.accept_schema(&schema.arrow_ipc_schema)?;
        }
        QueryStreamFrame::Batch(batch) => {
            fold.accept(&decoder.accept_batch(&batch.arrow_ipc_batch)?);
        }
        QueryStreamFrame::Terminal(terminal) => fold.terminal = Some(terminal.outcome),
    }
    Ok(())
}

/// Registers and publishes one `(id, filter_key)` fixture table.
///
/// Rows are admitted through the same logical ingress seam the public write
/// surface uses and then frozen and published, so the files a later query reads
/// are ones this cluster's own Scribe encoded.
///
/// # Errors
///
/// Returns a catalog, ingest, or publication error.
async fn seed_fixture_table(
    ingest: &WyrdTestServer,
    tenant: DataTenantId,
    table: &str,
    rows: i64,
    groups: i64,
) -> Result<(), JourneyError> {
    let catalog = ingest
        .state()
        .bifrost_catalog()
        .ok_or("ingest node composed no Bifrost catalog")?;
    let table_ref = vala_bifrost_redux::catalog::TableRef::new(
        vala_bifrost_redux::namespaces::BifrostNamespace::Bifrost,
        table,
    );
    catalog
        .create_table(vala_bifrost_redux::catalog::CreateTableRequest {
            table: table_ref.clone(),
            user_fields: vec![
                Field::new("id", DataType::Int64, false),
                Field::new("filter_key", DataType::Utf8, false),
            ],
            tenant,
            physical_layout: None,
            audit: None,
        })
        .await?;
    let (fingerprint, _) = catalog.table_registration(&table_ref, tenant).await?;
    let scribe = ingest
        .bifrost_scribe()
        .ok_or("ingest node composed no Scribe")?;
    let mut start_id = 0;
    while start_id < rows {
        let chunk = INGEST_CHUNK.min(rows - start_id);
        scribe
            .ingest_native_for_test(vala_bifrost_redux::scribe::NativeIngressTestFrame {
                principal: fixture_principal(tenant),
                table: table_ref.clone(),
                expected_schema_fingerprint: fingerprint,
                request_id: wyrd_spec::request_id::RequestId::now_v7(),
                batch_id: uuid::Uuid::now_v7(),
                audit_event: fixture_audit_event(tenant, table),
                payload: fixture_rows_ipc(start_id, chunk, groups)?,
            })
            .await?;
        start_id += chunk;
    }
    ingest.flush_bifrost().await?;
    Ok(())
}

/// Builds the tenant-bound principal one fixture ingest is admitted under.
fn fixture_principal(tenant: DataTenantId) -> wyrd_runtime::principal::Principal {
    wyrd_runtime::principal::Principal {
        id: wyrd_spec::auth::PrincipalId::new(uuid::Uuid::now_v7()),
        kind: wyrd_runtime::principal::PrincipalKind::User,
        tenant_id: tenant,
        roles: Vec::new(),
        effective_permissions: wyrd_runtime::PermissionSet::new(),
    }
}

/// Builds the server-shaped audit event committed with one fixture batch.
fn fixture_audit_event(tenant: DataTenantId, table: &str) -> wyrd_spec::vala::api::AuditEvent {
    let _ = tenant;
    wyrd_spec::vala::api::AuditEvent::new(
        wyrd_spec::request_id::RequestId::now_v7(),
        None,
        "bifrost.write".to_owned(),
        format!("bifrost://vala.bifrost/{table}"),
        None,
        wyrd_spec::auth::PrincipalId::new(uuid::Uuid::now_v7()),
        wyrd_spec::auth::PrincipalKindTag::User,
        wyrd_spec::vala::api::AuthMethod::Internal,
        "bifrost_write:write".to_owned(),
        wyrd_spec::vala::api::AuditDecision::Allow,
        wyrd_spec::vala::api::AuditResult::Success,
        "oracle contention fixture ingest".to_owned(),
    )
}

/// Encodes `rows` deterministic `(id, filter_key)` rows as one Arrow IPC stream.
///
/// Ids run `start_id..start_id + rows`, so several bounded requests compose one
/// contiguous logical table.
///
/// # Errors
///
/// Returns the Arrow batch or IPC encoding error unchanged.
fn fixture_rows_ipc(start_id: i64, rows: i64, groups: i64) -> Result<bytes::Bytes, JourneyError> {
    let groups = groups.max(1);
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("filter_key", DataType::Utf8, false),
    ]));
    let ids: Vec<i64> = (start_id..start_id.saturating_add(rows)).collect();
    let keys: Vec<String> = ids
        .iter()
        .map(|id| format!("group_{}", id % groups))
        .collect();
    let batch = RecordBatch::try_new(
        Arc::clone(&schema),
        vec![
            Arc::new(Int64Array::from(ids)),
            Arc::new(StringArray::from(keys)),
        ],
    )?;
    let mut ipc = Vec::new();
    {
        let mut writer = StreamWriter::try_new(&mut ipc, schema.as_ref())?;
        writer.write(&batch)?;
        writer.finish()?;
    }
    Ok(bytes::Bytes::from(ipc))
}
