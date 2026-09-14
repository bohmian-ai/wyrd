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
use wyrd_client::WyrdClient;
use wyrd_client::bifrost::QueryClient;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{
    BifrostQueryRequest, FreshnessPolicy, QueryStreamFrame, QueryTerminalOutcome, VisibilityMode,
};
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::telemetry::BifrostMetricSample;
use wyrd_testing::bifrost::{
    BifrostClusterSpec, BifrostNodeSpec, TestOracleResources, WyrdTestCluster,
};

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
const ANALYTICAL_DEADLINE_MS: i64 = 600_000;

/// Absolute deadline for each bounded public Interactive query.
const INTERACTIVE_DEADLINE_MS: i64 = 60_000;

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

    let mut stream = wyrd_client::Bifrost::query_only(&client)
        .query_client()
        .clone()
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

/// Memory observation injected on the smaller of the two heterogeneous Oracles.
///
/// Above the combined Scribe and Oracle role floors a mixed pod must satisfy,
/// and far enough below its sibling that the two derived plans cannot coincide.
const SMALL_ORACLE_MEMORY_BYTES: usize = 2 * 1024 * 1024 * 1024;

/// Memory observation injected on the larger of the two heterogeneous Oracles.
const LARGE_ORACLE_MEMORY_BYTES: usize = 4 * 1024 * 1024 * 1024;

/// Effective CPU injected on the smaller heterogeneous Oracle.
const SMALL_ORACLE_CPU: usize = 2;

/// Effective CPU injected on the larger heterogeneous Oracle.
const LARGE_ORACLE_CPU: usize = 6;

/// Rows written to the fixture table both heterogeneous Oracles read.
///
/// Small enough that the query is a flat Interactive scan; the journey is about
/// which capacity admitted it, not about how much work it did.
const HETEROGENEOUS_ROWS: i64 = 3;

/// Metric family fragments that would prove a durable admission ledger survived.
///
/// Every one of them named a step in the deleted allocation protocol. A pod
/// that still emitted any of them would be renewing or overdrawing a
/// cluster-wide grant rather than scheduling against its own observation.
const DURABLE_ADMISSION_FRAGMENTS: [&str; 5] =
    ["allocation", "renewal", "overdraft", "delegated", "_lease"];

/// Two Oracles observing different local resources each keep their own derived
/// capacity across a restart, while the historical durable admission tables
/// take no part in admitting their queries.
///
/// # Panics
///
/// Panics when any capacity, restart, query, or historical-row claim fails.
// Two pods, two public clients, and a restart share this runtime; on the
// single-threaded default the executing query starves the pods' own heartbeats
// and their role leases expire for a reason no deployment would produce.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn heterogeneous_oracles_ignore_historical_admission_rows() {
    prove_heterogeneous_local_capacity()
        .await
        .expect("heterogeneous local Oracle capacity journey");
}

/// Drives the complete heterogeneous-capacity journey over one live cluster.
///
/// # Errors
///
/// Returns the first claim that broke.
async fn prove_heterogeneous_local_capacity() -> Result<(), JourneyError> {
    // The cluster-wide `with_system_resources` helper would give both pods the
    // same observation, which is the exact thing under test. Each node's entry
    // is rewritten individually instead, preserving the generated identities
    // and roles so the restart below rebuilds the same two pods.
    let mut spec = BifrostClusterSpec::two_mixed();
    let observations = [
        heterogeneous_observation(SMALL_ORACLE_MEMORY_BYTES, SMALL_ORACLE_CPU),
        heterogeneous_observation(LARGE_ORACLE_MEMORY_BYTES, LARGE_ORACLE_CPU),
    ];
    spec.nodes = spec
        .nodes
        .iter()
        .zip(observations)
        .map(|(node, snapshot)| BifrostNodeSpec {
            node_id: node.node_id,
            roles: node.roles.clone(),
            oracle: Some(TestOracleResources {
                system_resources: Some(snapshot),
                ..TestOracleResources::default()
            }),
            forge_compaction_memory_limit_bytes: node.forge_compaction_memory_limit_bytes,
            role_timing: node.role_timing,
        })
        .collect();
    let node_ids: Vec<_> = spec.nodes.iter().map(|node| node.node_id).collect();
    let mut cluster = WyrdTestCluster::start_spec(spec).await?;
    let tenant = cluster.data_tenant_id();

    // Rows the deleted protocol would have consulted: a canonical tenant-default
    // ceiling far below what either pod derives locally, and an open block
    // already holding the whole of it under a foreign holder fence.
    seed_historical_admission_rows(&cluster, tenant, node_ids[0]).await?;
    let seeded = historical_admission_state(&cluster).await?;

    let table = unique_table("oracle_heterogeneous");
    let ingest = cluster.server(0).ok_or("missing ingest node")?;
    register_table(ingest, tenant, &table).await?;
    let rows = writer(ingest, "heterogeneous-writer").await?;
    for id in 1..=HETEROGENEOUS_ROWS {
        rows.write(
            &format!("vala.bifrost.{table}"),
            &journey_schema(),
            [journey_row(id, "heterogeneous")],
        )
        .await?;
    }
    ingest.flush_bifrost().await?;
    cluster.refresh_oracle_snapshots().await?;

    let before = derived_capacities(&cluster, &node_ids)?;
    if before[0] == before[1] {
        return Err(format!(
            "two differently observed Oracles derived the same capacity: {before:?}"
        )
        .into());
    }
    query_each_node(&cluster, &node_ids, &table, "first").await?;

    // Reverse order so the pod that restarts first is not the one that booted
    // first: a plan recovered from boot ordering rather than from the node's
    // own retained observation would survive one order and not the other.
    for node_id in node_ids.iter().rev() {
        cluster.terminate_node_abruptly_for_test(*node_id).await?;
        cluster.restart_node(*node_id).await?;
    }
    cluster.refresh_oracle_snapshots().await?;

    let after = derived_capacities(&cluster, &node_ids)?;
    if after != before {
        return Err(
            format!("restart changed derived Oracle capacity: {before:?} -> {after:?}").into(),
        );
    }
    query_each_node(&cluster, &node_ids, &table, "restarted").await?;

    let settled = historical_admission_state(&cluster).await?;
    if settled != seeded {
        return Err(format!(
            "historical admission rows changed under live queries: {seeded:?} -> {settled:?}"
        )
        .into());
    }
    prove_no_durable_admission_telemetry(&cluster)?;

    cluster.shutdown().await?;
    Ok(())
}

/// Builds one injected process observation for a heterogeneous Oracle pod.
fn heterogeneous_observation(
    memory_limit_bytes: usize,
    effective_cpu: usize,
) -> SystemResourceSnapshot {
    SystemResourceSnapshot {
        memory_limit_bytes,
        effective_cpu,
        scratch_capacity_bytes: ORACLE_SCRATCH_BYTES,
        scratch_available_bytes: ORACLE_SCRATCH_BYTES,
        memory_source: ResourceSource::Injected,
        cpu_source: ResourceSource::Injected,
    }
}

/// The capacity each named node derived from its own observation.
///
/// Only the boot calculation is projected. Live occupancy moves with whatever
/// query is running, so comparing it across a restart would prove nothing about
/// the derivation under test.
///
/// # Errors
///
/// Returns an error when a node is absent, is not running an Oracle, or cannot
/// report its resource snapshot.
fn derived_capacities(
    cluster: &WyrdTestCluster,
    node_ids: &[wyrd_spec::vala::api::NodeId],
) -> Result<Vec<(usize, usize, usize, usize)>, JourneyError> {
    node_ids
        .iter()
        .map(|node_id| {
            let plan = cluster
                .server_by_node(*node_id)
                .ok_or_else(|| format!("node {} is not running", node_id.as_uuid()))?
                .state()
                .bifrost_resources()
                .ok_or_else(|| format!("node {} composed no Bifrost resources", node_id.as_uuid()))?
                .plan();
            Ok((
                plan.oracle_floor_bytes,
                plan.elastic_memory_bytes,
                plan.effective_cpu,
                vala_bifrost_redux::resources::oracle_worker_slots(plan)
                    .map_err(|error| error.to_string())?,
            ))
        })
        .collect()
}

/// Runs one bounded Interactive query through each node's own public client.
///
/// # Errors
///
/// Returns an error when a client cannot be built, a query fails, or a node
/// returns a row count the fixture did not write.
async fn query_each_node(
    cluster: &WyrdTestCluster,
    node_ids: &[wyrd_spec::vala::api::NodeId],
    table: &str,
    label: &str,
) -> Result<(), JourneyError> {
    for (index, node_id) in node_ids.iter().enumerate() {
        let server = cluster
            .server_by_node(*node_id)
            .ok_or_else(|| format!("node {} is not running", node_id.as_uuid()))?;
        // One machine principal per pod and per pass: a bootstrap name is a
        // Card identity, so reusing it would collide rather than authenticate.
        let reader = client(server, &format!("heterogeneous-reader-{label}-{index}")).await?;
        let rows = query_rows(&reader, table, VisibilityMode::PublishedOnly).await?;
        if rows != u64::try_from(HETEROGENEOUS_ROWS)? {
            return Err(format!(
                "during {label} node {} returned {rows} rows",
                node_id.as_uuid()
            )
            .into());
        }
    }
    Ok(())
}

/// Seeds the historical policy ceiling and open block the deleted protocol used.
///
/// # Errors
///
/// Returns the database error unchanged.
async fn seed_historical_admission_rows(
    cluster: &WyrdTestCluster,
    tenant: DataTenantId,
    holder: wyrd_spec::vala::api::NodeId,
) -> Result<(), JourneyError> {
    let pool = cluster.pg_fixture().operator_pool().pool();
    sqlx::query(
        "INSERT INTO vala.oracle_admission_policies (scope_kind,data_tenant_id,query_class,capacity) VALUES ('tenant_default',NULL,'interactive',1)",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO vala.oracle_admission_blocks (block_id,allocation_id,scope_kind,data_tenant_id,principal_id,query_class,units,holder_node_id,holder_fencing_token,valid_from,expires_at,closed_at) \
         VALUES ($1,$2,'tenant',$3,NULL,'interactive',1,$4,1,now() - interval '1 hour',now() + interval '1 hour',NULL)",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(uuid::Uuid::now_v7())
    .bind(uuid::Uuid::from(tenant))
    .bind(uuid::Uuid::from(holder))
    .execute(pool)
    .await?;
    Ok(())
}

/// Projects every mutable fact the deleted allocation protocol would have changed.
///
/// # Errors
///
/// Returns the database error unchanged.
async fn historical_admission_state(
    cluster: &WyrdTestCluster,
) -> Result<(i64, i64, i64, Option<i64>), JourneyError> {
    let pool = cluster.pg_fixture().operator_pool().pool();
    let state: (i64, i64, i64, Option<i64>) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM vala.oracle_admission_policies), \
         (SELECT count(*) FROM vala.oracle_admission_blocks), \
         (SELECT count(*) FROM vala.oracle_admission_blocks WHERE closed_at IS NOT NULL), \
         (SELECT sum(units)::bigint FROM vala.oracle_admission_blocks)",
    )
    .fetch_one(pool)
    .await?;
    Ok(state)
}

/// Asserts no pod published a metric family from the deleted allocation protocol.
///
/// # Errors
///
/// Returns an error naming the first surviving family.
fn prove_no_durable_admission_telemetry(cluster: &WyrdTestCluster) -> Result<(), JourneyError> {
    for sample in cluster.telemetry().snapshot().map_err(|e| e.to_string())? {
        if let Some(fragment) = DURABLE_ADMISSION_FRAGMENTS
            .iter()
            .find(|fragment| sample.family.contains(*fragment))
        {
            return Err(format!(
                "{} still reports durable admission activity ({fragment})",
                sample.family
            )
            .into());
        }
    }
    Ok(())
}

/// Rows written to the fixture table the refusal journey reads.
const REFUSAL_ROWS: i64 = 24;

/// Deadline the stalled memory holder is submitted with, in milliseconds.
///
/// Long enough that the refusal, the release, and the cancel below all happen
/// inside one query's own lifetime rather than racing its deadline.
const REFUSAL_HOLDER_DEADLINE_MS: i64 = 60_000;

/// Deadline the refused query is submitted with, in milliseconds.
const REFUSAL_QUERY_DEADLINE_MS: i64 = 30_000;

/// Governed bytes the hold deliberately leaves the stalled holder.
///
/// One partition working set: exactly what the plan calls the minimum a query
/// needs to produce a batch. Holding the whole root instead would refuse the
/// holder itself before it could reach its schema frame, and there would be no
/// occupied root for the refusal below to meet.
const REFUSAL_HOLDER_HEADROOM_BYTES: usize = 64 * 1024 * 1024;

/// Bytes of filler each row of the refused query's sort key carries.
///
/// Three rows of this width exceed the headroom above, and a sort must reserve
/// a whole batch before it can spill any of it, so the refusal is a property of
/// the arithmetic rather than of how `DataFusion` chose to schedule the work.
const REFUSAL_SORT_KEY_BYTES: usize = 15_000_000;

/// Stable error code a query refused for capacity must carry.
const QUERY_ADMISSION_REJECTED_CODE: &str = "WYRD_VALA_429_QUERY_ADMISSION_REJECTED";

/// A memory refusal under a fully occupied Oracle root leaves the pod healthy
/// and its next query serviceable.
///
/// # Panics
///
/// Panics when any hold, refusal, release, health, or follow-up claim fails.
// The stalled holder, the refused query, and the follow-up query are three
// concurrent server-side lifecycles; on the single-threaded default the stalled
// holder's own task starves the ones observing it.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn memory_refusal_preserves_oracle_health_and_next_query() {
    prove_memory_refusal_preserves_health()
        .await
        .expect("Oracle memory refusal journey");
}

/// Drives the complete memory-refusal journey over one live cluster.
///
/// # Errors
///
/// Returns the first claim that broke.
async fn prove_memory_refusal_preserves_health() -> Result<(), JourneyError> {
    // The lowest supported Oracle rung is the only one where a single query can
    // occupy the whole governed root: at 512 MiB an Oracle-only pod has no
    // elastic memory, so its root equals the ceiling one query is granted.
    // Above that rung no single query can fill the root, by design.
    let cluster = WyrdTestCluster::start_spec(
        BifrostClusterSpec::three_oracles_one_scribe()
            .with_system_resources(oracle_floor_observation()),
    )
    .await?;
    let tenant = cluster.data_tenant_id();
    let ingest = cluster.server(3).ok_or("missing Scribe node")?;
    let table = unique_table("oracle_memory_refusal");
    register_table(ingest, tenant, &table).await?;
    let rows = writer(ingest, "memory-refusal-writer").await?;
    for id in 1..=REFUSAL_ROWS {
        rows.write(
            &format!("vala.bifrost.{table}"),
            &journey_schema(),
            [journey_row(id, "refusal")],
        )
        .await?;
    }
    ingest.flush_bifrost().await?;
    cluster.refresh_oracle_snapshots().await?;

    let server = cluster.server(0).ok_or("missing Oracle node")?;
    let resources = server
        .state()
        .bifrost_resources()
        .ok_or("node composed no Bifrost resources")?;
    let plan = resources.plan();
    let root_limit = plan
        .oracle_floor_bytes
        .saturating_add(plan.elastic_memory_bytes);
    let health = resources.oracle().ok_or("node hosts no Oracle")?.health();

    // Both controls arm the same next query: it takes the whole governed root
    // and then parks after its schema frame, so the occupancy the refusal below
    // meets is one real query's own reservation rather than a harness counter.
    server.stall_next_query_after_schema();
    server.hold_next_query_memory(root_limit.saturating_sub(REFUSAL_HOLDER_HEADROOM_BYTES))?;
    let holder = client(server, "memory-refusal-holder").await?;
    let holder_query = wyrd_client::Bifrost::query_only(&holder)
        .query_client()
        .clone();
    let stream = holder_query
        .query(&BifrostQueryRequest {
            sql: format!("SELECT id FROM vala.bifrost.{table}"),
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: Some(REFUSAL_HOLDER_DEADLINE_MS),
        })
        .await
        .map_err(|error| format!("holder query: {error}"))?;
    let held_request_id = stream.request_id().clone();
    let holder_task = tokio::spawn(async move {
        let mut stream = stream;
        let _ = stream.next_batch().await;
    });
    server.wait_query_schema_stall().await?;
    server.wait_query_memory_hold().await?;

    let occupied = resources.snapshot().map_err(|error| error.to_string())?;
    let held_floor = root_limit.saturating_sub(REFUSAL_HOLDER_HEADROOM_BYTES);
    if occupied.oracle_query_memory_used_bytes < held_floor {
        return Err(format!(
            "the held reservation occupied {} of a {root_limit} byte root",
            occupied.oracle_query_memory_used_bytes
        )
        .into());
    }

    let refused = client(server, "memory-refusal-reader").await?;
    let refusal = drain_query(
        &wyrd_client::Bifrost::query_only(&refused)
            .query_client()
            .clone(),
        &format!(
            "SELECT REPEAT('x', {REFUSAL_SORT_KEY_BYTES}) || CAST(id AS VARCHAR) AS wide_key \
             FROM vala.bifrost.{table} ORDER BY wide_key"
        ),
    )
    .await
    .err()
    .ok_or("a query against a fully occupied root was not refused")?;
    if !refusal.contains(QUERY_ADMISSION_REJECTED_CODE) {
        return Err(
            format!("refusal carried {refusal} rather than the typed capacity code").into(),
        );
    }

    server.release_query_memory_hold()?;
    holder_query.cancel(&held_request_id).await?;
    holder_task.abort();
    let _ = holder_task.await;
    let released = server
        .wait_bifrost_query_resources_released(
            held_request_id.as_str(),
            wyrd_testing::server::BifrostQueryResourceSnapshot::default(),
        )
        .await?;
    if released != wyrd_testing::server::BifrostQueryResourceSnapshot::default() {
        return Err(format!("the cancelled holder retained {released:?}").into());
    }

    if let Some(reason) = health.reason() {
        return Err(
            format!("a bounded memory refusal poisoned the process root: {reason:?}").into(),
        );
    }
    let next = client(server, "memory-refusal-next").await?;
    let served = query_rows(&next, &table, VisibilityMode::PublishedOnly).await?;
    if served != u64::try_from(REFUSAL_ROWS)? {
        return Err(format!("the next query returned {served} rows after the refusal").into());
    }

    cluster.shutdown().await?;
    Ok(())
}

/// Drains one statement to its terminal, rendering any refusal as its code.
///
/// A capacity refusal surfaces either as a rejected request or as a terminal
/// failure once the stream is already open, depending on how far planning
/// progressed before the root refused. Both are the same refusal to this
/// journey, so both are rendered to one string the caller matches a code in.
///
/// # Errors
///
/// Returns the rendered refusal when the statement does not complete.
async fn drain_query(query: &QueryClient, sql: &str) -> Result<u64, String> {
    let mut stream = query
        .query(&BifrostQueryRequest {
            sql: sql.to_owned(),
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: Some(REFUSAL_QUERY_DEADLINE_MS),
        })
        .await
        .map_err(|error| {
            format!(
                "{}: {error}",
                wyrd_spec::error::WyrdError::from(&error).code()
            )
        })?;
    let mut rows = 0_u64;
    loop {
        match stream.next_batch().await {
            Ok(Some(batch)) => {
                rows = rows.saturating_add(u64::try_from(batch.num_rows()).unwrap_or(u64::MAX))
            }
            Ok(None) => break,
            Err(error) => {
                return Err(format!(
                    "{}: {error}",
                    wyrd_spec::error::WyrdError::from(&error).code()
                ));
            }
        }
    }
    match stream.terminal().map(|terminal| terminal.outcome) {
        Some(QueryTerminalOutcome::Success | QueryTerminalOutcome::Degraded) => Ok(rows),
        other => Err(format!("terminal {other:?}")),
    }
}

/// Rows seeded into each tenant's table for the bounded-scheduling journey.
const SCHEDULING_ROWS: i64 = 24;

/// Distinct `filter_key` groups the scheduling fixture rows fall into.
///
/// Also the row count the grouped Analytical statement must return.
const SCHEDULING_GROUPS: i64 = 4;

/// Concurrent Interactive queries the lowest supported Oracle rung seats.
///
/// The rung derives four slot units from two effective CPUs and protects one
/// of them for Interactive work. Slot units are not what binds here: every
/// admitted query leases 256 MiB of scratch per unit against this rung's
/// 768 MiB scratch capacity, so the fourth concurrent Interactive query is
/// refused for scratch while a slot unit is still free. Three is still three
/// times the one unit the rung protects, so seating them is only possible by
/// borrowing idle Analytical capacity.
const SCHEDULING_HOLDS: usize = 3;

/// Contention rounds run while exactly one slot unit remains free.
///
/// Each round submits both tenants concurrently against a single free unit, so
/// one tenant is served and the other is refused. Four rounds is enough for a
/// rotation that ignored a tenant to show as that tenant never being served.
const SCHEDULING_ROTATION_ROUNDS: usize = 4;

/// Two tenants make bounded progress across both query classes on one pod.
///
/// Covers, through real authenticated requests against a live server: idle
/// Analytical capacity borrowed by Interactive work, retryable overload once
/// the pod is saturated, tenant rotation that serves both tenants from a
/// single free slot unit, the Interactive floor an Analytical query cannot
/// cross, and eventual progress for both classes once the pod drains.
///
/// # Panics
///
/// Panics when any admission, refusal, rotation, floor, or progress claim
/// fails.
// Four pods and several concurrent client streams share this runtime; on the
// single-threaded default the parked streams starve the pods' own heartbeats
// and the Oracle role leases expire for a reason no deployment would produce.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn two_tenants_make_bounded_progress_across_query_classes() {
    prove_two_tenant_bounded_progress()
        .await
        .expect("two-tenant bounded Oracle scheduling journey");
}

/// Drives the complete two-tenant scheduling journey over one live cluster.
///
/// # Errors
///
/// Returns the first scheduling claim that broke.
async fn prove_two_tenant_bounded_progress() -> Result<(), JourneyError> {
    let cluster = WyrdTestCluster::start_spec(
        BifrostClusterSpec::three_oracles_one_scribe()
            .with_system_resources(oracle_floor_observation()),
    )
    .await?;
    let tenants = [
        cluster.data_tenant_id(),
        cluster.add_tenant("scheduling-peer").await?,
    ];
    let ingest = cluster
        .servers()
        .find(|server| server.bifrost_scribe().is_some())
        .ok_or("missing ingest node")?;
    let suffix = uuid::Uuid::now_v7().simple();
    let mut tables = Vec::new();
    let mut clients = Vec::new();
    let server = cluster.server(0).ok_or("missing query node")?;
    for (index, tenant) in tenants.iter().enumerate() {
        let table = format!("scheduling_{index}_{suffix}");
        seed_fixture_table(ingest, *tenant, &table, SCHEDULING_ROWS, SCHEDULING_GROUPS).await?;
        clients.push(
            client_for_tenant(server, *tenant, &format!("scheduling-{index}-{suffix}")).await?,
        );
        tables.push(table);
    }
    cluster.refresh_oracle_snapshots().await?;

    // Idle-capacity borrowing: the rung protects one Interactive unit and
    // offers three to Analytical, so seating four concurrent Interactive
    // queries is only possible by borrowing every idle Analytical unit.
    let mut held = Vec::new();
    for index in 0..SCHEDULING_HOLDS {
        let slot = index % clients.len();
        held.push(
            hold_envelope(
                server,
                &clients[slot],
                &scheduling_interactive_sql(&tables[slot]),
            )
            .await
            .map_err(|error| format!("hold {index}: {error}"))?,
        );
    }
    let borrowed = class_gauge(&cluster, "interactive")?;
    #[expect(
        clippy::cast_precision_loss,
        reason = "three concurrent queries is exact in f64"
    )]
    let expected = SCHEDULING_HOLDS as f64;
    if (borrowed - expected).abs() > f64::EPSILON {
        return Err(format!(
            "the saturated pod reported {borrowed} active Interactive queries, not {expected}"
        )
        .into());
    }

    prove_saturated_pod_refuses_retryably(&clients, &tables).await?;

    // Rotation: exactly one unit is free from here, so each round can serve
    // exactly one of the two tenants that ask for it at the same moment.
    release_envelope(
        &cluster,
        held.pop().ok_or("no held envelope to release")?,
        "interactive",
        expected - 1.0,
    )
    .await?;
    prove_rotation_serves_both_tenants(&clients, &tables).await?;

    for (index, envelope) in held.into_iter().enumerate() {
        #[expect(
            clippy::cast_precision_loss,
            reason = "at most three concurrent queries is exact in f64"
        )]
        let remaining = (SCHEDULING_HOLDS - 2 - index) as f64;
        release_envelope(&cluster, envelope, "interactive", remaining).await?;
    }

    prove_analytical_cannot_cross_the_interactive_floor(&cluster, server, &clients, &tables)
        .await?;
    prove_both_classes_progress(&clients, &tables).await?;

    cluster.shutdown().await?;
    Ok(())
}

/// Builds the flat bounded projection the planner keeps Interactive.
///
/// Any global operator would make the plan distributed, and therefore
/// Analytical, however few rows it reads.
fn scheduling_interactive_sql(table: &str) -> String {
    format!("SELECT id, filter_key FROM vala.bifrost.{table}")
}

/// Builds the grouped aggregate the planner distributes as Analytical.
fn scheduling_analytical_sql(table: &str) -> String {
    format!("SELECT filter_key, COUNT(*) AS matched FROM vala.bifrost.{table} GROUP BY filter_key")
}

/// Admits one query through the public path and parks it holding its envelope.
///
/// The stall is the repository's existing schema-frame control: the query is a
/// real admitted query that owns real slot units and a real grant, and it is
/// released by dropping its stream exactly as an abandoned client would.
///
/// # Errors
///
/// Returns an error when the request is refused rather than admitted, or when
/// the admitted query never reaches its schema stall.
async fn hold_envelope(
    server: &WyrdTestServer,
    client: &WyrdClient,
    sql: &str,
) -> Result<tokio::task::JoinHandle<()>, JourneyError> {
    server.stall_next_query_after_schema();
    let stream = wyrd_client::Bifrost::query_only(client)
        .query_client()
        .clone()
        .query(&BifrostQueryRequest {
            sql: sql.to_owned(),
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: Some(REFUSAL_HOLDER_DEADLINE_MS),
        })
        .await
        .map_err(|error| format!("parked query was not admitted: {error}"))?;
    let task = tokio::spawn(async move {
        let mut stream = stream;
        let _ = stream.next_batch().await;
    });
    server.wait_query_schema_stall().await?;
    Ok(task)
}

/// Drops one parked stream and waits for its class gauge to fall to `expected`.
///
/// # Errors
///
/// Returns an error when the abandoned query never returns its slot units.
async fn release_envelope(
    cluster: &WyrdTestCluster,
    task: tokio::task::JoinHandle<()>,
    class: &str,
    expected: f64,
) -> Result<(), JourneyError> {
    task.abort();
    let _ = task.await;
    for _ in 0..PHYSICAL_EVIDENCE_POLLS {
        if (class_gauge(cluster, class)? - expected).abs() < f64::EPSILON {
            return Ok(());
        }
        tokio::time::sleep(PHYSICAL_EVIDENCE_INTERVAL).await;
    }
    Err(format!(
        "a released {class} envelope left the gauge at {}, not {expected}",
        class_gauge(cluster, class)?
    )
    .into())
}

/// Proves a saturated pod refuses both classes with the retryable typed code.
///
/// The refusal is bounded by the pod-local queue wait, so a caller is told to
/// retry within it rather than being held for its whole request deadline.
///
/// # Errors
///
/// Returns an error when a saturated pod admits a query, refuses it with some
/// other error, or holds the caller past the bounded refusal deadline.
async fn prove_saturated_pod_refuses_retryably(
    clients: &[WyrdClient],
    tables: &[String],
) -> Result<(), JourneyError> {
    for (index, client) in clients.iter().enumerate() {
        for (label, sql) in [
            ("interactive", scheduling_interactive_sql(&tables[index])),
            ("analytical", scheduling_analytical_sql(&tables[index])),
        ] {
            let started = std::time::Instant::now();
            let refusal = drain_query(
                &wyrd_client::Bifrost::query_only(client)
                    .query_client()
                    .clone(),
                &sql,
            )
            .await
            .err()
            .ok_or_else(|| format!("the saturated pod admitted another {label} query"))?;
            if !refusal.contains(QUERY_ADMISSION_REJECTED_CODE) {
                return Err(format!(
                    "the saturated pod refused a {label} query with {refusal}, not the retryable \
                     capacity code"
                )
                .into());
            }
            if started.elapsed() > OBSERVATION_DEADLINE {
                return Err(format!(
                    "a {label} refusal took {:?}, so it was not bounded by the queue wait",
                    started.elapsed()
                )
                .into());
            }
        }
    }
    Ok(())
}

/// Proves one free slot unit is rotated between two concurrently asking tenants.
///
/// Every round both tenants submit at the same moment against a single free
/// unit, so exactly one is served and the other receives the retryable code.
/// A rotation that ignored a tenant would show as that tenant never being
/// served across every round.
///
/// # Errors
///
/// Returns an error when a served query returns the wrong rows, when a refusal
/// carries some other error, or when either tenant was never served.
async fn prove_rotation_serves_both_tenants(
    clients: &[WyrdClient],
    tables: &[String],
) -> Result<(), JourneyError> {
    let mut served = vec![0_usize; clients.len()];
    for _ in 0..SCHEDULING_ROTATION_ROUNDS {
        let first_query = wyrd_client::Bifrost::query_only(&clients[0])
            .query_client()
            .clone();
        let second_query = wyrd_client::Bifrost::query_only(&clients[1])
            .query_client()
            .clone();
        let first_sql = scheduling_interactive_sql(&tables[0]);
        let second_sql = scheduling_interactive_sql(&tables[1]);
        let (first, second) = tokio::join!(
            drain_query(&first_query, &first_sql),
            drain_query(&second_query, &second_sql),
        );
        for (index, outcome) in [first, second].into_iter().enumerate() {
            match outcome {
                Ok(rows) if rows == u64::try_from(SCHEDULING_ROWS)? => served[index] += 1,
                Ok(rows) => {
                    return Err(format!(
                        "tenant {index} was served {rows} rows, not {SCHEDULING_ROWS}"
                    )
                    .into());
                }
                Err(refusal) if refusal.contains(QUERY_ADMISSION_REJECTED_CODE) => {}
                Err(refusal) => {
                    return Err(format!(
                        "tenant {index} was refused with {refusal}, not retryably"
                    )
                    .into());
                }
            }
        }
    }
    if let Some(index) = served.iter().position(|count| *count == 0) {
        return Err(format!(
            "rotation never served tenant {index} across {SCHEDULING_ROTATION_ROUNDS} rounds: \
             {served:?}"
        )
        .into());
    }
    Ok(())
}

/// Proves a live Analytical query cannot take the units Interactive is owed.
///
/// One Analytical query costs two of this rung's four units and the Analytical
/// maximum cannot seat a second, so two units always remain for Interactive
/// work no matter how much Analytical work is offered.
///
/// # Errors
///
/// Returns an error when a second Analytical query is admitted, when either
/// tenant's Interactive query is refused, or when the parked Analytical query
/// does not release its envelope.
async fn prove_analytical_cannot_cross_the_interactive_floor(
    cluster: &WyrdTestCluster,
    server: &WyrdTestServer,
    clients: &[WyrdClient],
    tables: &[String],
) -> Result<(), JourneyError> {
    let parked = hold_envelope(server, &clients[0], &scheduling_analytical_sql(&tables[0])).await?;
    let refusal = drain_query(
        &wyrd_client::Bifrost::query_only(&clients[1])
            .query_client()
            .clone(),
        &scheduling_analytical_sql(&tables[1]),
    )
    .await
    .err()
    .ok_or("the rung seated a second Analytical query")?;
    if !refusal.contains(QUERY_ADMISSION_REJECTED_CODE) {
        return Err(format!(
            "the second Analytical query failed with {refusal}, not the capacity code"
        )
        .into());
    }
    for (index, client) in clients.iter().enumerate() {
        let rows = drain_query(
            &wyrd_client::Bifrost::query_only(client)
                .query_client()
                .clone(),
            &scheduling_interactive_sql(&tables[index]),
        )
        .await
        .map_err(|refusal| {
            format!("live Analytical work refused tenant {index}'s floor query: {refusal}")
        })?;
        if rows != u64::try_from(SCHEDULING_ROWS)? {
            return Err(format!("tenant {index}'s floor query returned {rows} rows").into());
        }
    }
    release_envelope(cluster, parked, "analytical", 0.0).await
}

/// Proves both tenants complete both classes once the pod has drained.
///
/// # Errors
///
/// Returns an error when either class is refused or returns the wrong rows.
async fn prove_both_classes_progress(
    clients: &[WyrdClient],
    tables: &[String],
) -> Result<(), JourneyError> {
    for (index, client) in clients.iter().enumerate() {
        let query = wyrd_client::Bifrost::query_only(client)
            .query_client()
            .clone();
        for (label, sql, expected) in [
            (
                "interactive",
                scheduling_interactive_sql(&tables[index]),
                u64::try_from(SCHEDULING_ROWS)?,
            ),
            (
                "analytical",
                scheduling_analytical_sql(&tables[index]),
                u64::try_from(SCHEDULING_GROUPS)?,
            ),
        ] {
            let rows = drain_query(&query, &sql).await.map_err(|refusal| {
                format!("the drained pod refused tenant {index}'s {label} query: {refusal}")
            })?;
            if rows != expected {
                return Err(format!(
                    "tenant {index}'s {label} query returned {rows} rows, not {expected}"
                )
                .into());
            }
        }
    }
    Ok(())
}

/// Scratch capacity that keeps slot units, not scratch, the binding admission
/// resource.
///
/// A queue only forms when a request is refused a slot while its other demands
/// are still fundable: a scratch refusal is immediate and never enqueues. Eight
/// gibibytes is well past the four concurrent 256 MiB query leases this rung's
/// slot units allow, so every refusal in this journey is a slot refusal.
const QUEUE_SCRATCH_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// Concurrent Interactive queries that consume every slot unit on the rung.
///
/// Two effective CPUs derive four slot units, and an Interactive query costs
/// one, so four parked queries leave the fifth request nothing but the queue.
const QUEUE_HOLDS: usize = 4;

/// Polls spent waiting for one enqueue to become visible to inspection.
///
/// Every waiter's bounded queue wait starts at its own enqueue, so this loop is
/// deliberately tight: the whole choreography has to fit inside the first
/// waiter's wait.
const QUEUE_ENQUEUE_POLLS: usize = 2000;

/// Interval between enqueue-visibility polls.
const QUEUE_ENQUEUE_INTERVAL: Duration = Duration::from_millis(1);

/// The raw observation the queue journey's pods boot from.
fn oracle_queue_observation() -> SystemResourceSnapshot {
    SystemResourceSnapshot {
        memory_limit_bytes: ORACLE_MEMORY_FLOOR_BYTES,
        effective_cpu: ORACLE_EFFECTIVE_CPU,
        scratch_capacity_bytes: QUEUE_SCRATCH_BYTES,
        scratch_available_bytes: QUEUE_SCRATCH_BYTES,
        memory_source: ResourceSource::Injected,
        cpu_source: ResourceSource::Injected,
    }
}

/// Queued tenants are granted in per-tenant FIFO order with equal tenant weight.
///
/// # Panics
///
/// Panics when the queue grants out of order or a queued waiter is never
/// granted inside its bounded wait.
// The parked holders, the queued waiters, and the pods' own heartbeats all need
// to run at once; the single-threaded default would starve the leases.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn queued_tenants_are_granted_fifo_and_rotated() {
    prove_queue_is_fifo_and_tenant_rotated()
        .await
        .expect("tenant-fair Oracle queue journey");
}

/// Drives the complete queue-ordering journey over one live cluster.
///
/// The pod is saturated by parked queries, three identified requests are
/// enqueued in a known order behind them, each enqueue is confirmed through the
/// node's own inspection, and exactly one parked query is then released. From
/// there a single free slot unit passes down the queue, so completion order is
/// grant order: the first tenant's older request precedes the second tenant's,
/// which precedes the first tenant's newer one.
///
/// # Errors
///
/// Returns the first enqueue, grant, or ordering claim that broke.
async fn prove_queue_is_fifo_and_tenant_rotated() -> Result<(), JourneyError> {
    let cluster = WyrdTestCluster::start_spec(
        BifrostClusterSpec::three_oracles_one_scribe()
            .with_system_resources(oracle_queue_observation()),
    )
    .await?;
    let tenants = [
        cluster.data_tenant_id(),
        cluster.add_tenant("queue-peer").await?,
    ];
    let ingest = cluster
        .servers()
        .find(|server| server.bifrost_scribe().is_some())
        .ok_or("missing ingest node")?;
    let server = cluster.server(0).ok_or("missing query node")?;
    let suffix = uuid::Uuid::now_v7().simple();
    let mut tables = Vec::new();
    let mut clients = Vec::new();
    for (index, tenant) in tenants.iter().enumerate() {
        let table = format!("queue_{index}_{suffix}");
        seed_fixture_table(ingest, *tenant, &table, SCHEDULING_ROWS, SCHEDULING_GROUPS).await?;
        clients.push(client_for_tenant(server, *tenant, &format!("queue-{index}-{suffix}")).await?);
        tables.push(table);
    }
    cluster.refresh_oracle_snapshots().await?;

    // Warm both clients' query paths before anything is timed: the first
    // request through a client pays catalog and plan costs that would otherwise
    // land inside the first waiter's bounded queue wait.
    for (index, client) in clients.iter().enumerate() {
        drain_query(
            &wyrd_client::Bifrost::query_only(client)
                .query_client()
                .clone(),
            &scheduling_interactive_sql(&tables[index]),
        )
        .await
        .map_err(|refusal| format!("tenant {index}'s warm-up query was refused: {refusal}"))?;
    }

    let mut held = Vec::new();
    for index in 0..QUEUE_HOLDS {
        held.push(
            hold_envelope(
                server,
                &clients[index % clients.len()],
                &scheduling_interactive_sql(&tables[index % tables.len()]),
            )
            .await
            .map_err(|error| format!("queue hold {index}: {error}"))?,
        );
    }

    let order = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut waiters = Vec::new();
    for (label, tenant) in [("first-older", 0), ("second", 1), ("first-newer", 0)] {
        // The ring visits tenants in first-enqueue order, so the first tenant's
        // older request must be enqueued before the second tenant appears.
        let query = wyrd_client::Bifrost::query_only(&clients[tenant])
            .query_client()
            .clone();
        let sql = scheduling_interactive_sql(&tables[tenant]);
        let order = Arc::clone(&order);
        waiters.push(tokio::spawn(async move {
            let outcome = drain_query(&query, &sql).await;
            if outcome.is_ok()
                && let Ok(mut order) = order.lock()
            {
                order.push(label);
            }
            outcome.map(|_| label)
        }));
        wait_for_queued(server, waiters.len()).await?;
    }

    // One released unit is enough: each granted waiter returns it on completion,
    // so the queue drains one grant at a time and completion order is grant
    // order.
    let released = held.pop().ok_or("no held envelope to release")?;
    released.abort();
    let _ = released.await;

    for waiter in waiters {
        waiter
            .await
            .map_err(|error| format!("a queued waiter panicked: {error}"))?
            .map_err(|refusal| format!("a queued waiter was never granted: {refusal}"))?;
    }
    let granted = order
        .lock()
        .map_err(|_| "queue order lock poisoned")?
        .clone();
    if granted != ["first-older", "second", "first-newer"] {
        return Err(format!(
            "the queue granted {granted:?}, which is not per-tenant FIFO with equal tenant weight"
        )
        .into());
    }

    for envelope in held {
        envelope.abort();
        let _ = envelope.await;
    }
    cluster.shutdown().await?;
    Ok(())
}

/// Waits until the node's own inspection counts `expected` queued waiters.
///
/// # Errors
///
/// Returns an error when the enqueue never becomes visible.
async fn wait_for_queued(server: &WyrdTestServer, expected: usize) -> Result<(), JourneyError> {
    let expected = u64::try_from(expected)?;
    for _ in 0..QUEUE_ENQUEUE_POLLS {
        if server.oracle_runtime_inspection()?.queued_queries >= expected {
            return Ok(());
        }
        tokio::time::sleep(QUEUE_ENQUEUE_INTERVAL).await;
    }
    Err(format!(
        "only {} waiters ever enqueued, not {expected}",
        server.oracle_runtime_inspection()?.queued_queries
    )
    .into())
}
