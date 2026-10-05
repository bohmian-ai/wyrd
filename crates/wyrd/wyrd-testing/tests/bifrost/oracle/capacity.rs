//! Oracle journeys — admission contention at the 4 GiB pod floor.
//!
//! One production-shaped run proves that the retained admission owner stays
//! safe *and* useful when a real memory-heavy Analytical query already holds a
//! query envelope, a distributed graph, and live spill ownership on the exact
//! 4 GiB pod floor (a 3 GiB shared cap): bounded Interactive queries from two
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
use tokio::task::JoinHandle;
use vala_bifrost_redux::oracle::{QueryIpcDecoder, QueryResourceProbe, QueryResourceSnapshot};
use vala_bifrost_redux::resources::{ResourceSource, SystemResourceSnapshot};
use wyrd_client::Bifrost;
use wyrd_client::WyrdClient;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::NodeId;
use wyrd_spec::vala::api::{BifrostQueryRequest, QueryStreamFrame, QueryTerminalOutcome};
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::telemetry::BifrostMetricKind;
use wyrd_testing::bifrost::telemetry::BifrostMetricSample;
use wyrd_testing::bifrost::telemetry::BifrostTelemetryDelta;
use wyrd_testing::bifrost::{
    BifrostClusterSpec, BifrostNodeSpec, TestOracleResources, WyrdTestCluster,
};

use crate::support::*;

/// Exact Oracle-only process memory this qualification runs at.
///
/// The hard 4 GiB pod floor: the 1 GiB server minimum plus a 3 GiB shared cap,
/// half of which is one query's memory limit. It is an injected observation,
/// not an OS limit, so this journey bounds what admission *accounts for* and
/// makes no aggregate physical-memory claim.
const ORACLE_MEMORY_FLOOR_BYTES: usize = vala_bifrost_redux::resources::MIN_POD_MEMORY_BYTES;

/// Effective CPU injected on every Oracle-only node.
const ORACLE_EFFECTIVE_CPU: usize = 2;

/// Scratch capacity and availability injected on every Oracle-only node.
///
/// Large enough that the Analytical query's spill is bounded by its admitted
/// scratch envelope rather than by a fixture-sized filesystem.
const ORACLE_SCRATCH_BYTES: u64 = 8 * 1024 * 1024 * 1024;

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

/// Left-table rows under the contending Analytical join.
///
/// Wider than the right table so the join is not an identity.
const CONTENTION_LEFT_ROWS: i64 = 400_000;

/// Right-table rows under the contending Analytical join, and its result rows.
///
/// Large enough that the undrained result cannot fit the Oracle stream's
/// buffers, so the unpulled stream keeps the query holding its grant across
/// both Interactive windows.
const CONTENTION_RIGHT_ROWS: i64 = 300_000;

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

/// Bound on polls waiting for a released query's gauge to settle.
const PHYSICAL_EVIDENCE_POLLS: usize = 300;

/// Interval between polls for a settled gauge.
const PHYSICAL_EVIDENCE_INTERVAL: Duration = Duration::from_millis(100);

/// A live Analytical query holding its grant does not stop two tenants'
/// bounded Interactive queries from being admitted and completing at the
/// 4 GiB pod floor.
///
/// # Panics
///
/// Panics when any admission, memory, telemetry, or cleanup claim fails.
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
        CONTENTION_LEFT_ROWS,
        ANALYTICAL_INGEST_GROUPS,
    )
    .await?;
    seed_fixture_table(
        ingest,
        tenant_a,
        &right,
        CONTENTION_RIGHT_ROWS,
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
        .query_sql(
            query_context(tenant_a)?,
            BifrostQueryRequest {
                // A distributed join and grouped aggregate that fits its grant:
                // this journey needs a live Analytical query holding its
                // envelope, not one that must spill. Exceeding the grant and
                // spilling is the peer-network baseline's claim.
                sql: format!(
                    "SELECT l.id, COUNT(*) AS matched FROM vala.bifrost.{left} AS l \
                     JOIN vala.bifrost.{right} AS r ON l.id = r.id GROUP BY l.id"
                ),
                deadline_ms: Some(ANALYTICAL_DEADLINE_MS),
            },
        )
        .await?;
    let analytical_probe = analytical.resource_probe_for_test();

    // Driven, not slept on: pulling a frame is what advances execution, so the
    // stream reaches live ownership through its own progress and is then simply
    // left alone with that ownership held.
    let mut fold = AnalyticalFold::default();
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
    if fold.rows != u64::try_from(CONTENTION_RIGHT_ROWS)? {
        return Err(format!(
            "the Analytical query returned {} rows, not {CONTENTION_RIGHT_ROWS}",
            fold.rows
        )
        .into());
    }
    let terminal = fold
        .terminal
        .ok_or("the Analytical stream emitted no terminal frame")?;
    if terminal != QueryTerminalOutcome::Success {
        return Err(format!("the Analytical query settled as {terminal:?}, not a success").into());
    }

    let analytical_final = analytical_probe.snapshot();
    for (label, live, final_snapshot) in [
        ("analytical", live_analytical, analytical_final),
        ("interactive tenant A", interactive[0].0, interactive[0].1),
        ("interactive tenant B", interactive[1].0, interactive[1].1),
    ] {
        prove_pool_within_grant(label, live, final_snapshot)?;
    }

    await_clean_nodes(&cluster).await?;
    // A graph releases only after every holder of its query runtime, and so
    // its spill directory, has dropped. The exact baseline is still observed
    // under the clean-node bound because an Interactive query's server-side
    // stream is dropped after the client has read its terminal.
    let mut settled = ownership_baseline(&cluster).await?;
    for _ in 0..CLEAN_NODE_POLLS {
        if settled == baseline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        settled = ownership_baseline(&cluster).await?;
    }
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
        .query(&BifrostQueryRequest {
            // A flat bounded projection on purpose. Any global operator — a
            // sort, an aggregate, a join — is a "complex" plan, which the
            // planner classifies Analytical however few rows it reads, and an
            // Analytical query would never reach the Interactive floor this
            // window exists to observe. Rows are counted, not ordered.
            sql: format!("SELECT id, filter_key FROM vala.bifrost.{table}"),
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
/// Returns an error when any node's governed ownership exceeds the one shared
/// cap the plan governs.
fn prove_roots_within_budget(cluster: &WyrdTestCluster, label: &str) -> Result<(), JourneyError> {
    for snapshot in cluster.oracle_resource_snapshots()? {
        let held = snapshot.governed_memory_used_bytes;
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

/// Asserts no production metric label carries a tenant identity.
///
/// # Errors
///
/// Returns an error naming the first series whose labels leak a tenant.
fn prove_labels_are_bounded(
    delta: &BifrostTelemetryDelta,
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
    /// Retained Oracle spill directories.
    spill_directories: u64,
    /// Retained Oracle spill files.
    spill_files: u64,
    /// Bytes held by retained Oracle spill files.
    spill_file_bytes: u64,
    /// Running peer reservations across every node.
    peer_running: u64,
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
        spill_directories: inspection.spill_directories,
        spill_files: inspection.spill_files,
        spill_file_bytes: inspection.spill_file_bytes,
        peer_running: inspection.peer_running,
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
    let granted = u64::try_from(envelope.execution().granted_memory_bytes())?;
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
fn labelled_sum(delta: &BifrostTelemetryDelta, family: &str, labels: &[(&str, &str)]) -> f64 {
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

/// Counts the Analytical result without retaining it.
#[derive(Default)]
struct AnalyticalFold {
    /// Rows accepted across every batch.
    rows: u64,
    /// Terminal outcome the stream published, if it published one.
    terminal: Option<QueryTerminalOutcome>,
}

/// Routes one stream frame into the decoder and the running fold.
///
/// # Errors
///
/// Returns the Arrow IPC decode error unchanged, or a row-count conversion
/// error.
fn accept_frame(
    decoder: &mut QueryIpcDecoder,
    fold: &mut AnalyticalFold,
    frame: QueryStreamFrame,
) -> Result<(), JourneyError> {
    match frame {
        QueryStreamFrame::Schema(schema) => {
            decoder.accept_schema(&schema.arrow_ipc_schema)?;
        }
        QueryStreamFrame::Batch(batch) => {
            let batch = decoder.accept_batch(&batch.arrow_ipc_batch)?;
            fold.rows = fold.rows.saturating_add(u64::try_from(batch.num_rows())?);
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
        credential_id: None,
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
/// The 4 GiB pod floor, far enough below its sibling that the two derived
/// plans cannot coincide.
const SMALL_ORACLE_MEMORY_BYTES: usize = vala_bifrost_redux::resources::MIN_POD_MEMORY_BYTES;

/// Memory observation injected on the larger of the two heterogeneous Oracles.
const LARGE_ORACLE_MEMORY_BYTES: usize = 8 * 1024 * 1024 * 1024;

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
    node_ids: &[NodeId],
) -> Result<Vec<(usize, usize, usize)>, JourneyError> {
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
                plan.managed_memory_bytes,
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
    node_ids: &[NodeId],
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
        let rows = query_rows(&reader, table).await?;
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
    holder: NodeId,
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

/// Acknowledged but unflushed rows, so every refusal-journey query also reads
/// a live Scribe source.
const REFUSAL_LIVE_ROWS: i64 = 8;

/// Stalled holder queries it takes to occupy the governed root.
///
/// One query's memory limit is half the root, so no single query can fill it;
/// two holders at their limits less half the headroom each leave exactly
/// [`REFUSAL_HOLDER_HEADROOM_BYTES`] free.
const REFUSAL_HOLDERS: usize = 2;

/// Deadline each stalled memory holder is submitted with, in milliseconds.
///
/// Long enough that the refusal, the release, and the cancel below all happen
/// inside one query's own lifetime rather than racing its deadline.
const REFUSAL_HOLDER_DEADLINE_MS: i64 = 60_000;

/// Deadline the refused query is submitted with, in milliseconds.
const REFUSAL_QUERY_DEADLINE_MS: i64 = 30_000;

/// Governed bytes the holds deliberately leave free in the root.
///
/// One partition working set: exactly what the plan calls the minimum a query
/// needs to produce a batch. Holding the whole root instead would refuse the
/// last holder itself before it could reach its schema frame, and there would
/// be no occupied root for the refusal below to meet.
const REFUSAL_HOLDER_HEADROOM_BYTES: usize = 64 * 1024 * 1024;

/// Bytes of filler each row of the refused query's sort key carries.
///
/// Three rows of this width exceed the headroom above, and a sort must reserve
/// a whole batch before it can spill any of it, so the refusal is a property of
/// the arithmetic rather than of how `DataFusion` chose to schedule the work.
const REFUSAL_SORT_KEY_BYTES: usize = 15_000_000;

/// Stable error code a query refused for capacity must carry.
const QUERY_ADMISSION_REJECTED_CODE: &str = "WYRD_VALA_429_QUERY_ADMISSION_REJECTED";

/// Stable error code a query refused because every waiting place is taken must carry.
const QUERY_QUEUE_FULL_CODE: &str = "WYRD_VALA_429_QUERY_QUEUE_FULL";

/// A memory refusal under a fully occupied Oracle root leaves the pod healthy
/// and its next query serviceable.
///
/// The table holds published and live Scribe rows, so the refused query has a
/// live source: its refusal must still end the whole query as a failure, never
/// a successful or degraded partial, and the next query must read both
/// sources with a `Success` terminal.
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
    // At the 4 GiB pod floor one query's limit is half the 3 GiB root, so the
    // root is occupied by two stalled holders rather than one.
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
    for id in REFUSAL_ROWS + 1..=REFUSAL_ROWS + REFUSAL_LIVE_ROWS {
        rows.write(
            &format!("vala.bifrost.{table}"),
            &journey_schema(),
            [journey_row(id, "refusal-live")],
        )
        .await?;
    }

    let server = cluster.server(0).ok_or("missing Oracle node")?;
    let resources = server
        .state()
        .bifrost_resources()
        .ok_or("node composed no Bifrost resources")?;
    let plan = resources.plan();
    let root_limit = plan.managed_memory_bytes;
    let health = resources.oracle().ok_or("node hosts no Oracle")?.health();

    // Both controls arm the same next query: it takes its share of the root
    // and then parks after its schema frame, so the occupancy the refusal below
    // meets is real queries' own reservations rather than a harness counter.
    let held_floor = root_limit.saturating_sub(REFUSAL_HOLDER_HEADROOM_BYTES);
    let mut holders = Vec::new();
    for index in 0..REFUSAL_HOLDERS {
        server.stall_next_query_after_schema();
        server.hold_next_query_memory(held_floor / REFUSAL_HOLDERS)?;
        let holder = client(server, &format!("memory-refusal-holder-{index}")).await?;
        let holder_query = wyrd_client::Bifrost::query_only(&holder);
        let stream = holder_query
            .query(&BifrostQueryRequest {
                sql: format!("SELECT id FROM vala.bifrost.{table}"),
                deadline_ms: Some(REFUSAL_HOLDER_DEADLINE_MS),
            })
            .await
            .map_err(|error| format!("holder query {index}: {error}"))?;
        let held_request_id = stream.request_id().clone();
        let holder_task = tokio::spawn(async move {
            let mut stream = stream;
            let _ = stream.next_batch().await;
        });
        server.wait_query_schema_stall().await?;
        server.wait_query_memory_hold().await?;
        holders.push((holder_query, held_request_id, holder_task));
    }

    let occupied = resources.snapshot().map_err(|error| error.to_string())?;
    if occupied.oracle_query_memory_used_bytes < held_floor {
        return Err(format!(
            "the held reservations occupied {} of a {root_limit} byte root",
            occupied.oracle_query_memory_used_bytes
        )
        .into());
    }

    let refused = client(server, "memory-refusal-reader").await?;
    let refusal = drain_query(
        &wyrd_client::Bifrost::query_only(&refused),
        &format!(
            "SELECT REPEAT('x', {REFUSAL_SORT_KEY_BYTES}) || CAST(id AS VARCHAR) AS wide_key \
             FROM vala.bifrost.{table} ORDER BY wide_key"
        ),
    )
    .await
    .err()
    .ok_or("a query against a fully occupied root was not refused")?;
    if !refusal.contains(QUERY_RESOURCES_EXHAUSTED_CODE) {
        return Err(
            format!("refusal carried {refusal} rather than the typed resource code").into(),
        );
    }

    server.release_query_memory_hold()?;
    for (holder_query, held_request_id, holder_task) in holders {
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
    }

    if let Some(reason) = health.reason() {
        return Err(
            format!("a bounded memory refusal poisoned the process root: {reason:?}").into(),
        );
    }
    let next = client(server, "memory-refusal-next").await?;
    let mut stream = wyrd_client::Bifrost::query_only(&next)
        .query(&BifrostQueryRequest {
            sql: format!("SELECT id FROM vala.bifrost.{table}"),
            deadline_ms: Some(REFUSAL_QUERY_DEADLINE_MS),
        })
        .await?;
    let mut served = 0_u64;
    while let Some(batch) = stream.next_batch().await? {
        served = served.saturating_add(u64::try_from(batch.num_rows())?);
    }
    let outcome = stream.terminal().map(|terminal| terminal.outcome);
    if served != u64::try_from(REFUSAL_ROWS + REFUSAL_LIVE_ROWS)?
        || outcome != Some(QueryTerminalOutcome::Success)
    {
        return Err(format!(
            "the next query returned {served} published and live rows ending {outcome:?}"
        )
        .into());
    }

    cluster.shutdown().await?;
    Ok(())
}

/// Drains one statement to its terminal under the refusal deadline.
///
/// # Errors
///
/// Returns the rendered refusal when the statement does not complete.
async fn drain_query(query: &Bifrost, sql: &str) -> Result<u64, String> {
    drain_query_within(query, sql, REFUSAL_QUERY_DEADLINE_MS).await
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
/// Returns the rendered refusal when the statement does not complete within
/// `deadline_ms`.
async fn drain_query_within(query: &Bifrost, sql: &str, deadline_ms: i64) -> Result<u64, String> {
    let mut stream = query
        .query(&BifrostQueryRequest {
            sql: sql.to_owned(),
            deadline_ms: Some(deadline_ms),
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

/// Rows seeded for the query-local memory-failure journey.
const FAILURE_ROWS: i64 = 16;

/// Oracle slot units the memory-failure journey's pod is configured with.
///
/// A routable pod: a forwarding candidate must offer both classes, which any
/// pod above one unit does. Interactive work borrows all three units, so three
/// parked Interactive queries saturate it and a fourth provably queues.
const FAILURE_SLOTS: usize = 3;

/// Stable error code of an admitted query whose governed memory ran out.
const QUERY_RESOURCES_EXHAUSTED_CODE: &str = "WYRD_VALA_503_QUERY_RESOURCES_EXHAUSTED";

/// A memory failure inside one admitted query is local to that query and typed.
///
/// Three queries are admitted on a three-slot pod and park after streaming
/// rows, and a fourth waits in the queue. One parked query then fails its next
/// allocation against its own admitted pool. The failed stream ends as the
/// typed `QueryResourcesExhausted` failure, the queued query is admitted only
/// once that failure's slot and memory return, both parked siblings resume and
/// complete every row, and the pod ends healthy with nothing held or queued.
///
/// # Panics
///
/// Panics when any admission, failure, sibling, queue, or health claim fails.
// Two parked streams, a queued stream, and the server share this runtime; on
// the single-threaded default a parked stream starves the others.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn memory_failure_is_query_local_and_typed() {
    prove_memory_failure_is_query_local()
        .await
        .expect("Oracle query-local memory failure journey");
}

/// Drives the query-local memory-failure journey over one combined server.
///
/// # Errors
///
/// Returns the first claim that broke.
async fn prove_memory_failure_is_query_local() -> Result<(), JourneyError> {
    let server = WyrdTestServer::builder()
        .with_oracle_query_slot_limit_for_test(FAILURE_SLOTS)
        .start_bound()
        .await?;
    wait_ready(&server).await?;
    let tenant = server.data_tenant_id();
    let table = unique_table("oracle_memory_failure");
    register_table(&server, tenant, &table).await?;
    let rows = writer(&server, "memory-failure-writer").await?;
    for id in 1..=FAILURE_ROWS {
        rows.write(
            &format!("vala.bifrost.{table}"),
            &journey_schema(),
            [journey_row(id, "failure")],
        )
        .await?;
    }
    server.flush_bifrost().await?;
    let expected_rows = usize::try_from(FAILURE_ROWS)?;
    let health = server
        .state()
        .bifrost_resources()
        .and_then(|resources| resources.oracle())
        .ok_or("node hosts no Oracle")?
        .health();
    let reader = client(&server, "memory-failure-reader").await?;
    let query = wyrd_client::Bifrost::query_only(&reader);
    let request = BifrostQueryRequest {
        sql: format!("SELECT id FROM vala.bifrost.{table}"),
        deadline_ms: Some(REFUSAL_QUERY_DEADLINE_MS),
    };

    let mut siblings = Vec::new();
    for _ in 0..FAILURE_SLOTS - 1 {
        let park = server.park_next_query_after_rows()?;
        let stream = query.query(&request).await?;
        park.wait_entered().await;
        siblings.push((park, stream));
    }
    let failing_park = server.park_next_query_after_rows()?;
    let mut failing = query.query(&request).await?;
    failing_park.wait_entered().await;
    let running = server.oracle_runtime_inspection()?;
    if running.active_queries != u64::try_from(FAILURE_SLOTS)? || running.queued_queries != 0 {
        return Err(format!("the parked queries left the pod at {running:?}").into());
    }

    let queued_request = request.clone();
    let queued_client = reader.clone();
    let queued = tokio::spawn(async move {
        drain_stream(
            &wyrd_client::Bifrost::query_only(&queued_client),
            &queued_request,
        )
        .await
    });
    wait_inspection(&server, |inspection| inspection.queued_queries == 1).await?;

    failing_park.exhaust();
    let mut failed_rows = 0_usize;
    let failure = loop {
        match failing.next_batch().await {
            Ok(Some(batch)) => failed_rows += batch.num_rows(),
            Ok(None) => break None,
            Err(error) => break Some(error),
        }
    };
    let failure = failure.ok_or_else(|| {
        format!(
            "the exhausted query ended {:?} after {failed_rows} rows",
            failing.terminal()
        )
    })?;
    let code = wyrd_spec::error::WyrdError::from(&failure).code();
    if code != QUERY_RESOURCES_EXHAUSTED_CODE {
        return Err(format!("the exhausted query failed with {code}: {failure}").into());
    }

    let (queued_rows, queued_outcome) = queued.await??;
    if queued_rows != expected_rows || queued_outcome != Some(QueryTerminalOutcome::Success) {
        return Err(
            format!("the queued query read {queued_rows} rows ending {queued_outcome:?}").into(),
        );
    }

    for (park, mut sibling) in siblings {
        park.resume();
        let mut sibling_rows = 0_usize;
        while let Some(batch) = sibling.next_batch().await? {
            sibling_rows += batch.num_rows();
        }
        let sibling_outcome = sibling.terminal().map(|terminal| terminal.outcome);
        if sibling_rows != expected_rows || sibling_outcome != Some(QueryTerminalOutcome::Success) {
            return Err(
                format!("a sibling read {sibling_rows} rows ending {sibling_outcome:?}").into(),
            );
        }
    }

    wait_inspection(&server, |inspection| {
        inspection.active_queries == 0
            && inspection.queued_queries == 0
            && inspection.reserved_memory_bytes == 0
    })
    .await?;
    if let Some(reason) = health.reason() {
        return Err(format!("a query-local memory failure poisoned the root: {reason:?}").into());
    }
    server.shutdown().await?;
    Ok(())
}

/// Drains one statement to its terminal, returning its rows and outcome.
///
/// # Errors
///
/// Returns the client error that ended the stream.
async fn drain_stream(
    query: &Bifrost,
    request: &BifrostQueryRequest,
) -> Result<(usize, Option<QueryTerminalOutcome>), wyrd_client::bifrost::BifrostClientError> {
    let mut stream = query.query(request).await?;
    let mut rows = 0_usize;
    while let Some(batch) = stream.next_batch().await? {
        rows += batch.num_rows();
    }
    Ok((rows, stream.terminal().map(|terminal| terminal.outcome)))
}

/// Polls `/readyz` until the server, including its Oracle membership, is ready.
///
/// # Errors
///
/// Returns the HTTP error, or a timeout naming the last readiness body.
async fn wait_ready(server: &WyrdTestServer) -> Result<(), JourneyError> {
    let base = server.base_url().ok_or("missing HTTP URL")?;
    let deadline = tokio::time::Instant::now() + OBSERVATION_DEADLINE;
    loop {
        let response = reqwest::get(format!("{base}/readyz")).await?;
        if response.status().is_success() {
            return Ok(());
        }
        let body = response.text().await?;
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("the server never became ready: {body}").into());
        }
        tokio::time::sleep(PHYSICAL_EVIDENCE_INTERVAL).await;
    }
}

/// Polls the Oracle's own runtime inspection until `ready` holds.
///
/// # Errors
///
/// Returns the inspection error, or a timeout naming the last inspection.
async fn wait_inspection(
    server: &WyrdTestServer,
    ready: impl Fn(&wyrd_testing::server::OracleRuntimeInspection) -> bool,
) -> Result<(), JourneyError> {
    let deadline = tokio::time::Instant::now() + OBSERVATION_DEADLINE;
    loop {
        let inspection = server.oracle_runtime_inspection()?;
        if ready(&inspection) {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(
                format!("the Oracle never reached the awaited state: {inspection:?}").into(),
            );
        }
        tokio::time::sleep(PHYSICAL_EVIDENCE_INTERVAL).await;
    }
}

/// Rows seeded into each tenant's table for the bounded-scheduling journey.
const SCHEDULING_ROWS: i64 = 24;

/// Distinct `filter_key` groups the scheduling fixture rows fall into.
///
/// Also the row count the grouped Analytical statement must return.
const SCHEDULING_GROUPS: i64 = 4;

/// Concurrent Interactive queries the 4 GiB pod floor seats.
///
/// The rung derives four slot units from two effective CPUs and protects one
/// of them for Interactive work. Disk is provisioned rather than leased, so
/// slot units are the only binding resource and four holds saturate the pod.
/// Four is four times the one unit the rung protects, so seating them is only
/// possible by borrowing every idle Analytical unit.
const SCHEDULING_HOLDS: usize = 4;

/// Contention rounds run while exactly one slot unit remains free.
///
/// Each round submits both tenants concurrently against a single free unit, so
/// one tenant is served and the other is refused. Four rounds is enough for a
/// rotation that ignored a tenant to show as that tenant never being served.
const SCHEDULING_ROTATION_ROUNDS: usize = 4;

/// Two tenants make bounded progress across both query classes on one pod.
///
/// Covers, through real authenticated requests against a live server: idle
/// Analytical capacity borrowed by Interactive work, a queued wait that ends
/// at the caller's deadline once the pod is saturated, tenant rotation that serves both tenants from a
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
        reason = "four concurrent queries is exact in f64"
    )]
    let expected = SCHEDULING_HOLDS as f64;
    if (borrowed - expected).abs() > f64::EPSILON {
        return Err(format!(
            "the saturated pod reported {borrowed} active Interactive queries, not {expected}"
        )
        .into());
    }

    prove_saturated_pod_queues_until_deadline(&clients, &tables).await?;

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
            reason = "at most four concurrent queries is exact in f64"
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
) -> Result<JoinHandle<()>, JourneyError> {
    server.stall_next_query_after_schema();
    let stream = wyrd_client::Bifrost::query_only(client)
        .query(&BifrostQueryRequest {
            sql: sql.to_owned(),
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
    task: JoinHandle<()>,
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

/// Stable error code a query whose queue wait or total deadline expired carries.
const QUERY_TIMEOUT_CODE: &str = "WYRD_VALA_504_QUERY_TIMEOUT";

/// Short caller deadline a query submitted to a saturated pod waits under.
const SATURATED_QUERY_DEADLINE_MS: i64 = 1_000;

/// Proves a saturated pod queues both classes until the caller's deadline.
///
/// A saturated pod no longer refuses a fundable query; the query waits in the
/// tenant-fair queue and, when nothing is released, ends with the typed timeout
/// at its own deadline rather than a retryable refusal.
///
/// # Errors
///
/// Returns an error when a saturated pod admits a query, ends it with some
/// other error, or holds the caller well past its deadline.
async fn prove_saturated_pod_queues_until_deadline(
    clients: &[WyrdClient],
    tables: &[String],
) -> Result<(), JourneyError> {
    for (index, client) in clients.iter().enumerate() {
        for (label, sql) in [
            ("interactive", scheduling_interactive_sql(&tables[index])),
            ("analytical", scheduling_analytical_sql(&tables[index])),
        ] {
            let started = std::time::Instant::now();
            let refusal = drain_query_within(
                &wyrd_client::Bifrost::query_only(client),
                &sql,
                SATURATED_QUERY_DEADLINE_MS,
            )
            .await
            .err()
            .ok_or_else(|| format!("the saturated pod admitted another {label} query"))?;
            if !refusal.contains(QUERY_TIMEOUT_CODE) {
                return Err(format!(
                    "the saturated pod ended a {label} query with {refusal}, not the typed \
                     timeout"
                )
                .into());
            }
            if started.elapsed() > OBSERVATION_DEADLINE {
                return Err(format!(
                    "a {label} query took {:?}, so it was not bounded by its deadline",
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
/// unit, so one is served first and the other waits for the unit.
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
        let first_query = wyrd_client::Bifrost::query_only(&clients[0]);
        let second_query = wyrd_client::Bifrost::query_only(&clients[1]);
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

/// Proves live Analytical work cannot take the unit Interactive is owed.
///
/// Every query charges one unit, so this rung seats at most
/// `SCHEDULING_HOLDS - 1` Analytical queries. With that many parked, one more
/// waits in the queue until its deadline, while the one protected unit still
/// serves each tenant's Interactive query in turn.
///
/// # Errors
///
/// Returns an error when an Analytical query beyond the maximum is admitted,
/// when either tenant's Interactive query is refused, or when the parked
/// Analytical queries do not release their envelopes.
async fn prove_analytical_cannot_cross_the_interactive_floor(
    cluster: &WyrdTestCluster,
    server: &WyrdTestServer,
    clients: &[WyrdClient],
    tables: &[String],
) -> Result<(), JourneyError> {
    let mut parked = Vec::new();
    for index in 0..SCHEDULING_HOLDS - 1 {
        let slot = index % clients.len();
        parked.push(
            hold_envelope(
                server,
                &clients[slot],
                &scheduling_analytical_sql(&tables[slot]),
            )
            .await
            .map_err(|error| format!("analytical hold {index}: {error}"))?,
        );
    }
    let refusal = drain_query_within(
        &wyrd_client::Bifrost::query_only(&clients[1]),
        &scheduling_analytical_sql(&tables[1]),
        SATURATED_QUERY_DEADLINE_MS,
    )
    .await
    .err()
    .ok_or("the rung seated an Analytical query past its maximum")?;
    if !refusal.contains(QUERY_TIMEOUT_CODE) {
        return Err(format!(
            "the excess Analytical query failed with {refusal}, not a queued timeout"
        )
        .into());
    }
    for (index, client) in clients.iter().enumerate() {
        let rows = drain_query(
            &wyrd_client::Bifrost::query_only(client),
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
    let last = parked.pop().ok_or("no parked Analytical envelope")?;
    for envelope in parked {
        envelope.abort();
        let _ = envelope.await;
    }
    release_envelope(cluster, last, "analytical", 0.0).await
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
        let query = wyrd_client::Bifrost::query_only(client);
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
/// gibibytes funds every query this pod's slot units allow, so every refusal
/// in this journey is a slot refusal.
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
            &wyrd_client::Bifrost::query_only(client),
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
        let query = wyrd_client::Bifrost::query_only(&clients[tenant]);
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

/// Bound on polls waiting for the burst's snapshot reads to block on the lock.
const SNAPSHOT_LOCK_POLLS: usize = 300;

/// Interval between polls for blocked snapshot reads.
const SNAPSHOT_LOCK_INTERVAL: Duration = Duration::from_millis(100);

/// Queries in the snapshot burst: one more than any two-slot planning bound.
const SNAPSHOT_BURST: usize = 3;

/// Three authorized queries whose snapshot preparation is held behind one
/// catalog lock all wait inside their leader deadline and then complete,
/// rather than the third receiving an immediate planning refusal.
///
/// # Panics
///
/// Panics when any burst query is refused or returns the wrong rows.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn three_concurrent_snapshots_are_not_refused() {
    prove_concurrent_snapshots_wait()
        .await
        .expect("concurrent snapshot preparation journey");
}

/// Holds the catalog table lock while a burst of public queries reaches
/// snapshot preparation, then releases it and requires every query to finish.
///
/// The lock is test-owned database state: `ACCESS EXCLUSIVE` on the Iceberg
/// catalog table makes each query's uncached metadata-pointer read wait in
/// Postgres exactly as a busy connection pool or slow catalog would. The
/// registration lookup is served from the node's cache after the setup write,
/// so it cannot be the blocked read. The burst is only released once
/// every query is provably blocked on that lock, or once any query has already
/// ended — an early end is the immediate refusal this journey forbids.
///
/// # Errors
///
/// Returns the first client, Postgres, or result claim that broke.
async fn prove_concurrent_snapshots_wait() -> Result<(), JourneyError> {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed()).await?;
    let server = cluster.server(0).ok_or("missing mixed node")?;
    let table = unique_table("oracle_snapshot_burst");
    let fqn = format!("vala.bifrost.{table}");
    register_table(server, cluster.data_tenant_id(), &table).await?;
    let writer = writer(server, "snapshot-burst").await?;
    writer
        .write(&fqn, &journey_schema(), [journey_row(1, "row-1")])
        .await?;
    server.flush_bifrost().await?;
    cluster.refresh_oracle_snapshots().await?;

    let superuser = cluster.pg_fixture().superuser_pool().await?;
    let mut lock = superuser.begin().await?;
    sqlx::query("LOCK TABLE iceberg_catalog.iceberg_tables IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *lock)
        .await?;
    let burst: Vec<JoinHandle<Result<u64, String>>> = (0..SNAPSHOT_BURST)
        .map(|_| {
            let client = writer.client().clone();
            let table = table.clone();
            tokio::spawn(async move {
                query_rows(&client, &table)
                    .await
                    .map_err(|error| error.to_string())
            })
        })
        .collect();
    let mut blocked = 0_i64;
    for _ in 0..SNAPSHOT_LOCK_POLLS {
        blocked = sqlx::query_scalar(
            "SELECT count(*) FROM pg_locks \
             WHERE relation = 'iceberg_catalog.iceberg_tables'::regclass AND NOT granted",
        )
        .fetch_one(&superuser)
        .await?;
        if usize::try_from(blocked)? >= SNAPSHOT_BURST || burst.iter().any(JoinHandle::is_finished)
        {
            break;
        }
        tokio::time::sleep(SNAPSHOT_LOCK_INTERVAL).await;
    }
    lock.rollback().await?;

    for (index, query) in burst.into_iter().enumerate() {
        let rows = query
            .await?
            .map_err(|error| format!("burst query {index} was refused: {error}"))?;
        if rows != 1 {
            return Err(format!("burst query {index} returned {rows} rows, not 1").into());
        }
    }
    if usize::try_from(blocked)? < SNAPSHOT_BURST {
        return Err(format!(
            "only {blocked} snapshot reads ever waited on the catalog lock, not {SNAPSHOT_BURST}"
        )
        .into());
    }
    cluster.shutdown().await?;
    Ok(())
}

/// Caller deadline the saturated queued queries are submitted with.
const QUEUED_QUERY_DEADLINE_MS: i64 = 30_000;

/// Time the queued queries must stay waiting before a unit is released.
const QUEUED_HOLD: Duration = Duration::from_millis(250);

/// Maximum queue wait the saturated-wait journey's pods are configured with.
///
/// Long enough that the released waiters are granted well inside it, short
/// enough that a queue-wait expiry is observed without the one-hour default.
const JOURNEY_MAX_QUEUE_WAIT_MS: u64 = 5_000;

/// A fundable query submitted to a saturated pod waits in the queue over both
/// public transports, completes once a unit is released, and otherwise ends
/// with the typed timeout at the earlier of its queue limit and its total
/// deadline, for both query classes.
///
/// # Panics
///
/// Panics when either transport refuses a queued query, a released query
/// returns the wrong rows, or an expired query ends with anything but the
/// typed timeout.
// The parked holders, both transports' queued queries, and the pods' own
// heartbeats all need to run at once.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn saturated_query_waits_on_http_and_grpc() {
    prove_saturated_query_waits()
        .await
        .expect("saturated Oracle queue wait journey");
}

/// Drives the saturated-wait journey over one live cluster.
///
/// Every slot unit is parked, one HTTP and one gRPC query are enqueued and
/// held past [`QUEUED_HOLD`], and one unit is then released so both complete
/// through the queue. The pod is then saturated again: a one-second total
/// deadline and, separately, the configured queue limit under a longer total
/// deadline must each end as the typed timeout on both transports. The
/// production metrics agree with the owner at each step: waiters are queue
/// depth and never active work, each grant records one queue wait, and each
/// expiry records one queue-deadline refusal.
///
/// # Errors
///
/// Returns the first enqueue, wait, completion, or expiry claim that broke.
///
/// # Panics
///
/// Panics when a production queue, active, wait, or admission series
/// disagrees with the admission owner or the queries' observed endings.
async fn prove_saturated_query_waits() -> Result<(), JourneyError> {
    let cluster = WyrdTestCluster::start_spec(
        BifrostClusterSpec::three_oracles_one_scribe()
            .with_system_resources(oracle_queue_observation())
            .with_oracle_runtime_for_test(wyrd_server::config::OracleRuntimeConfig {
                max_queue_wait_ms: JOURNEY_MAX_QUEUE_WAIT_MS,
                ..wyrd_server::config::OracleRuntimeConfig::default()
            }),
    )
    .await?;
    let tenant = cluster.data_tenant_id();
    let ingest = cluster
        .servers()
        .find(|server| server.bifrost_scribe().is_some())
        .ok_or("missing ingest node")?;
    let server = cluster.server(0).ok_or("missing query node")?;
    let table = format!("saturated_{}", uuid::Uuid::now_v7().simple());
    seed_fixture_table(ingest, tenant, &table, SCHEDULING_ROWS, SCHEDULING_GROUPS).await?;
    let client = client_for_tenant(server, tenant, &format!("saturated-{table}")).await?;
    cluster.refresh_oracle_snapshots().await?;
    let sql = scheduling_interactive_sql(&table);
    drain_query(&wyrd_client::Bifrost::query_only(&client), &sql)
        .await
        .map_err(|refusal| format!("the warm-up query was refused: {refusal}"))?;
    let channel = wyrd_tonic::tonic::transport::Endpoint::from_shared(
        server.grpc_url().ok_or("missing gRPC URL")?,
    )?
    .connect()
    .await?;
    let bearer = format!("Bearer {}", client.auth().bearer().await?.expose());

    let mut held = Vec::new();
    for index in 0..QUEUE_HOLDS {
        held.push(
            hold_envelope(server, &client, &sql)
                .await
                .map_err(|error| format!("saturation hold {index}: {error}"))?,
        );
    }

    let queue_window = cluster.telemetry().checkpoint()?;
    let http = {
        let query = wyrd_client::Bifrost::query_only(&client);
        let sql = sql.clone();
        tokio::spawn(
            async move { drain_query_within(&query, &sql, QUEUED_QUERY_DEADLINE_MS).await },
        )
    };
    wait_for_queued(server, 1).await?;
    let grpc = {
        let (channel, sql, bearer) = (channel.clone(), sql.clone(), bearer.clone());
        tokio::spawn(
            async move { grpc_query(channel, &sql, &bearer, QUEUED_QUERY_DEADLINE_MS).await },
        )
    };
    wait_for_queued(server, 2).await?;
    tokio::time::sleep(QUEUED_HOLD).await;
    let queued = server.oracle_runtime_inspection()?.queued_queries;
    if queued != 2 || http.is_finished() || grpc.is_finished() {
        return Err(format!(
            "after {QUEUED_HOLD:?} only {queued} queries were still waiting in the queue"
        )
        .into());
    }
    // The waiting queries are queue depth, not active work: the active gauge
    // equals the owner's admitted count, which is exactly the parked holds.
    let waiting = cluster.telemetry().snapshot()?;
    let gauge = |family: &str| -> f64 {
        waiting
            .iter()
            .filter(|sample| sample.family == family)
            .map(|sample| sample.value)
            .sum()
    };
    let admitted = server.oracle_runtime_inspection()?.active_queries;
    eprintln!(
        "evidence query_queue phase=held owner_queued={queued} owner_admitted={admitted} \
         scraped oracle_queries_queued={} oracle_queries_active={}",
        gauge("oracle_queries_queued"),
        gauge("oracle_queries_active")
    );
    assert_eq!(
        gauge("oracle_queries_queued"),
        2.0,
        "two waiters are queue depth"
    );
    assert_eq!(
        gauge("oracle_queries_active"),
        f64::from(u32::try_from(admitted)?),
        "the active gauge is the owner's admitted work and excludes waiters"
    );
    assert_eq!(
        usize::try_from(admitted)?,
        QUEUE_HOLDS,
        "only the parked holds are admitted"
    );

    let released = held.pop().ok_or("no held envelope to release")?;
    released.abort();
    let _ = released.await;
    let expected = u64::try_from(SCHEDULING_ROWS)?;
    let http_rows = http
        .await?
        .map_err(|refusal| format!("the queued HTTP query ended with {refusal}"))?;
    let grpc_rows = grpc
        .await?
        .map_err(|refusal| format!("the queued gRPC query ended with {refusal}"))?;
    if http_rows != expected || grpc_rows != expected {
        return Err(format!(
            "released queued queries returned {http_rows} HTTP and {grpc_rows} gRPC rows, not \
             {expected}"
        )
        .into());
    }

    // Both waiters were granted from the queue: each records one queue wait
    // at least as long as the hold that kept it waiting, and one admission.
    let granted = cluster.telemetry().delta_since(&queue_window)?;
    eprintln!(
        "evidence query_queue phase=granted http_rows={http_rows} grpc_rows={grpc_rows} \
         held_seconds={} samples: {}",
        QUEUED_HOLD.as_secs_f64(),
        granted.evidence(&QUEUE_FAMILIES)
    );
    assert_eq!(
        metric_value(
            &granted,
            "oracle_admission_queue_duration_seconds",
            BifrostMetricKind::HistogramCount,
            &[],
        ),
        2.0,
        "each granted waiter records one queue wait"
    );
    assert!(
        metric_value(
            &granted,
            "oracle_admission_queue_duration_seconds",
            BifrostMetricKind::HistogramSum,
            &[],
        ) >= 2.0 * QUEUED_HOLD.as_secs_f64(),
        "each queue wait covers the hold that kept it waiting"
    );
    assert_eq!(
        metric_value(
            &granted,
            "oracle_admission_total",
            BifrostMetricKind::Counter,
            &[("outcome", "admitted")],
        ),
        2.0,
        "the two waiters are the window's only admissions"
    );
    assert_eq!(
        metric_value(
            &granted,
            "oracle_admission_total",
            BifrostMetricKind::Counter,
            &[("outcome", "rejected")],
        ),
        0.0,
        "a granted waiter is never counted as refused"
    );

    held.push(hold_envelope(server, &client, &sql).await?);
    let expiry_window = cluster.telemetry().checkpoint()?;
    let analytical = scheduling_analytical_sql(&table);
    for (limit, deadline_ms, statement) in [
        ("total deadline", SATURATED_QUERY_DEADLINE_MS, &sql),
        ("queue limit", QUEUED_QUERY_DEADLINE_MS, &sql),
        ("queue limit", QUEUED_QUERY_DEADLINE_MS, &analytical),
    ] {
        let started = std::time::Instant::now();
        let http = drain_query_within(
            &wyrd_client::Bifrost::query_only(&client),
            statement,
            deadline_ms,
        )
        .await;
        let http_elapsed = started.elapsed();
        let started = std::time::Instant::now();
        let grpc = grpc_query(channel.clone(), statement, &bearer, deadline_ms).await;
        let grpc_elapsed = started.elapsed();
        for (transport, outcome, elapsed) in
            [("HTTP", http, http_elapsed), ("gRPC", grpc, grpc_elapsed)]
        {
            if !matches!(&outcome, Err(refusal) if refusal.contains(QUERY_TIMEOUT_CODE)) {
                return Err(format!(
                    "a {transport} wait past its {limit} ended with {outcome:?}, not \
                     {QUERY_TIMEOUT_CODE}"
                )
                .into());
            }
            let bound =
                Duration::from_millis(u64::try_from(deadline_ms)?.min(JOURNEY_MAX_QUEUE_WAIT_MS));
            if elapsed + QUEUED_HOLD < bound || elapsed > bound + OBSERVATION_DEADLINE {
                return Err(format!(
                    "a {transport} wait past its {limit} ended after {elapsed:?}, not near {bound:?}"
                )
                .into());
            }
        }
    }
    if server.oracle_runtime_inspection()?.queued_queries != 0 {
        return Err("an expired waiter retained its queue place".into());
    }
    // Six expiries, three statements over two transports, each one refusal
    // for the queue deadline and nothing left waiting.
    let expired = cluster.telemetry().delta_since(&expiry_window)?;
    eprintln!(
        "evidence query_queue phase=expired typed_timeouts=6 owner_queued={} samples: {}",
        server.oracle_runtime_inspection()?.queued_queries,
        expired.evidence(&QUEUE_FAMILIES)
    );
    assert_eq!(
        metric_value(
            &expired,
            "oracle_admission_total",
            BifrostMetricKind::Counter,
            &[("outcome", "rejected"), ("reason", "queue_deadline")],
        ),
        6.0,
        "every expired waiter is one queue-deadline refusal"
    );
    let still_queued: f64 = expired
        .gauge_final
        .iter()
        .filter(|sample| sample.family == "oracle_queries_queued")
        .map(|sample| sample.value)
        .sum();
    assert_eq!(still_queued, 0.0, "expired waiters leave no queue depth");

    for envelope in held {
        envelope.abort();
        let _ = envelope.await;
    }
    cluster.shutdown().await?;
    Ok(())
}

/// Admission and execution families the saturated-wait windows print as evidence.
const QUEUE_FAMILIES: [&str; 6] = [
    "oracle_queries_queued",
    "oracle_queries_active",
    "oracle_admission_total",
    "oracle_admission_queue_duration_seconds",
    "oracle_query_duration_seconds",
    "bifrost_gate_query_streams_total",
];

/// Queue places the overflow journey boots its Oracle pods with.
///
/// Small enough to fill with real queued requests in one journey, and more
/// than one so a refusal proves the bound rather than a single-waiter special
/// case.
const OVERFLOW_QUEUE_PLACES: usize = 2;

/// Longest an overflow refusal may take through the public path.
///
/// A full queue refuses before waiting, so the probe must return well inside
/// its own much longer deadline; a probe that is parked anywhere on the way to
/// admission instead runs into that deadline.
const OVERFLOW_REFUSAL_BOUND: Duration = Duration::from_secs(5);

/// A full queue refuses the next query immediately and keeps its waiters.
///
/// # Panics
///
/// Panics when the overflow request waits, succeeds, or is refused with
/// anything but the retryable admission code, or when the refusal disturbs a
/// queued waiter.
// The parked holders, the queued waiters, and the pods' own heartbeats all need
// to run at once.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn full_queue_refuses_the_next_query_immediately() {
    prove_full_queue_refuses_overflow()
        .await
        .expect("full Oracle queue overflow journey");
}

/// Drives the queue-overflow journey over one live cluster.
///
/// Every slot unit is parked, [`OVERFLOW_QUEUE_PLACES`] requests are enqueued
/// and confirmed through inspection, and one more request is then sent through
/// the public client with a long deadline. It must come back as
/// `WYRD_VALA_429_QUERY_ADMISSION_REJECTED` within
/// [`OVERFLOW_REFUSAL_BOUND`] while every queued waiter is still queued, and
/// the waiters must still complete once the holders release.
///
/// # Errors
///
/// Returns the first saturation, enqueue, refusal, or completion claim that
/// broke.
async fn prove_full_queue_refuses_overflow() -> Result<(), JourneyError> {
    let cluster = WyrdTestCluster::start_spec(
        BifrostClusterSpec::three_oracles_one_scribe()
            .with_system_resources(oracle_queue_observation())
            .with_oracle_runtime_for_test(wyrd_server::config::OracleRuntimeConfig {
                admission_waiters: OVERFLOW_QUEUE_PLACES,
                ..wyrd_server::config::OracleRuntimeConfig::default()
            }),
    )
    .await?;
    let tenant = cluster.data_tenant_id();
    let ingest = cluster
        .servers()
        .find(|server| server.bifrost_scribe().is_some())
        .ok_or("missing ingest node")?;
    let server = cluster.server(0).ok_or("missing query node")?;
    let table = format!("overflow_{}", uuid::Uuid::now_v7().simple());
    seed_fixture_table(ingest, tenant, &table, SCHEDULING_ROWS, SCHEDULING_GROUPS).await?;
    let client = client_for_tenant(server, tenant, &format!("overflow-{table}")).await?;
    cluster.refresh_oracle_snapshots().await?;
    let sql = scheduling_interactive_sql(&table);
    drain_query(&wyrd_client::Bifrost::query_only(&client), &sql)
        .await
        .map_err(|refusal| format!("the warm-up query was refused: {refusal}"))?;

    let mut held = Vec::new();
    for index in 0..QUEUE_HOLDS {
        held.push(
            hold_envelope(server, &client, &sql)
                .await
                .map_err(|error| format!("saturation hold {index}: {error}"))?,
        );
    }
    let mut waiters = Vec::new();
    for _ in 0..OVERFLOW_QUEUE_PLACES {
        let query = wyrd_client::Bifrost::query_only(&client);
        let sql = sql.clone();
        waiters.push(tokio::spawn(async move {
            drain_query_within(&query, &sql, QUEUED_QUERY_DEADLINE_MS).await
        }));
        wait_for_queued(server, waiters.len()).await?;
    }

    let started = std::time::Instant::now();
    let overflow = tokio::time::timeout(
        OVERFLOW_REFUSAL_BOUND,
        drain_query_within(
            &wyrd_client::Bifrost::query_only(&client),
            &sql,
            QUEUED_QUERY_DEADLINE_MS,
        ),
    )
    .await
    .map_err(|_| {
        format!(
            "the overflow request was not refused within {OVERFLOW_REFUSAL_BOUND:?}; queued \
             {:?}",
            server
                .oracle_runtime_inspection()
                .map(|inspection| inspection.queued_queries)
        )
    })?;
    if !matches!(&overflow, Err(refusal) if refusal.contains(QUERY_QUEUE_FULL_CODE)) {
        return Err(format!(
            "the overflow request ended with {overflow:?} after {:?}, not \
             {QUERY_QUEUE_FULL_CODE}",
            started.elapsed()
        )
        .into());
    }
    let queued = server.oracle_runtime_inspection()?.queued_queries;
    if queued != u64::try_from(OVERFLOW_QUEUE_PLACES)? {
        return Err(format!(
            "the refusal left {queued} waiters queued, not {OVERFLOW_QUEUE_PLACES}"
        )
        .into());
    }

    for envelope in held {
        envelope.abort();
        let _ = envelope.await;
    }
    let expected = u64::try_from(SCHEDULING_ROWS)?;
    for waiter in waiters {
        let rows = waiter
            .await?
            .map_err(|refusal| format!("a queued waiter ended with {refusal}"))?;
        if rows != expected {
            return Err(format!("a queued waiter returned {rows} rows, not {expected}").into());
        }
    }
    cluster.shutdown().await?;
    Ok(())
}

/// Runs one statement over the raw public gRPC query service and counts rows.
///
/// A rejected call or failed stream is rendered as its canonical problem code
/// so callers match it the same way as [`drain_query_within`].
///
/// # Errors
///
/// Returns the rendered failure when the call is rejected, the stream fails, or
/// the terminal frame is missing.
async fn grpc_query(
    channel: wyrd_tonic::tonic::transport::Channel,
    sql: &str,
    bearer: &str,
    deadline_ms: i64,
) -> Result<u64, String> {
    let status_code = |status: wyrd_tonic::tonic::Status| {
        status
            .metadata()
            .get_bin(wyrd_tonic::error::WYRD_ERROR_HEADER)
            .and_then(|value| value.to_bytes().ok())
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|problem| problem["code"].as_str().map(str::to_owned))
            .unwrap_or_else(|| format!("{status:?}"))
    };
    let mut request = wyrd_tonic::tonic::Request::new(
        wyrd_tonic::wyrd::v1::BifrostQueryRequest::from(BifrostQueryRequest {
            sql: sql.to_owned(),
            deadline_ms: Some(deadline_ms),
        }),
    );
    request.metadata_mut().insert(
        "x-wyrd-access-token",
        bearer.parse().map_err(|error| format!("{error}"))?,
    );
    let mut frames =
        wyrd_tonic::wyrd::v1::bifrost_query_service_client::BifrostQueryServiceClient::new(channel)
            .query(request)
            .await
            .map_err(status_code)?
            .into_inner();
    let mut decoder = QueryIpcDecoder::new();
    let mut rows = 0_u64;
    while let Some(frame) = frames.next().await {
        match frame.map_err(status_code)?.frame {
            Some(wyrd_tonic::wyrd::v1::query_stream_frame::Frame::Schema(schema)) => {
                decoder
                    .accept_schema(&schema.arrow_ipc_schema)
                    .map_err(|error| error.to_string())?;
            }
            Some(wyrd_tonic::wyrd::v1::query_stream_frame::Frame::Batch(batch)) => {
                let batch = decoder
                    .accept_batch(&batch.arrow_ipc_batch)
                    .map_err(|error| error.to_string())?;
                rows = rows.saturating_add(u64::try_from(batch.num_rows()).unwrap_or(u64::MAX));
            }
            Some(wyrd_tonic::wyrd::v1::query_stream_frame::Frame::Terminal(_)) => return Ok(rows),
            None => return Err("the gRPC stream carried an empty frame".to_owned()),
        }
    }
    Err("the gRPC stream ended without a terminal frame".to_owned())
}
