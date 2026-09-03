//! Live per-family Bifrost capacity runners composed from retained primitives.
//!
//! Each runner is a thin cluster-bound offer loop over the fixture-driven plan
//! produced by [`super::bench_runner`]: it boots the exact topology the plan
//! selects, offers one family-native load through the retained public SDK
//! primitives ([`super::bench_cluster`]), binds every measured window to a T17
//! capture ([`run_sampled_window`]), and emits one causally-bound
//! [`BifrostCapacityReport`] through the canonical disk writer. The journey
//! engine (`WindowRun`) is deliberately not reused: per the step-1 fork ruling,
//! runners compose the lower-level retained primitives rather than parameterize
//! the journey, so the journey stays byte-stable and the runners own only their
//! offer/capture step. No ladder value, duration, or budget is compiled in
//! here; every such input is read from the resolved fixtures.

use std::collections::BTreeMap;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use arrow::array::{Array, Float64Array, Int64Array, StringArray};
use arrow::record_batch::RecordBatch;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use vala_sdk::{BifrostFrame, CollectedQueryLimits, QueryClient};
use wyrd_bench::jain_fairness;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{
    BifrostQueryRequest, FreshnessPolicy, QueryTerminalOutcome, VisibilityMode,
};

use super::BifrostClusterSpec;
use super::bench_cluster::{
    REFERENCE_TABLE, ROWS_PER_WRITE, ReferenceClient, await_forge_convergence,
    deterministic_batch_id, flush_tenant_writers, fresh_ingest_table, is_backpressure,
    is_retryable, provision_named_tables, provision_reference_tenants, reference_clients,
    reference_payload, tenant_row_base,
};
use super::bench_dataset::{
    BifrostQualificationDataset, DatasetShape, QualificationAggregate,
    QualificationExpectedResults, QualificationQueries, TierBudgets, TopologyProfiles,
    WorkloadProfiles, dataset_manifest_digest, smoke_dataset_shape, workload_profiles_digest,
};
use super::bench_materializer::{
    BackpressurePolicy, BifrostDatasetMaterializer, QUALIFICATION_TABLE,
};
use super::bench_qualification::{QualificationRunRecord, RunFingerprint};
use super::bench_report::{
    AuditRelaySample, BifrostCapacityReport, BifrostCapacityStage, CapacityReportContext,
    CorrectnessVerdict, DistributedBody, EnvironmentIdentity, IngestBody, LatencyPercentiles,
    MixedBody, QueryBody, RecommendedStage, ResourceSample, SaturationCause, StageControl,
    StageExecution, TelemetryWindowRef, TopologyIdentity, WorkCounts, sha256_hex,
};
use super::bench_runner::{
    ResolvedInvocation, RunnerFamily, RunnerInvocation, RunnerTier, StagePlan, enforce_budget,
    plan_stages, stage_control, topology_for_pods,
};
use super::cluster::{SharedRunResources, WyrdTestCluster};
use super::telemetry::{BifrostTelemetryDelta, run_sampled_window};

/// Logical bytes attributed to one reference row's single `Int64` `row_id`.
const LOGICAL_BYTES_PER_ROW: u64 = 8;
/// Fallback driver in-flight cap when a control omits an explicit one.
///
/// This bounds only the runner's dispatch concurrency; it is not a ladder value
/// and never appears in a report. When a control declares `max_in_flight`, that
/// fixture value is used instead.
const MAX_IN_FLIGHT_CAP: u64 = 4_096;

/// Logical bytes attributed to one full qualification row across all columns.
///
/// Sums the fixed-width columns (`row_id`, `wyrd_event_time`, `device_id`, and
/// `value` at eight bytes each) with the deterministic fixed-length string
/// columns (the ten-character `metric` and the 256-character `payload`). It is
/// the reference scan-cost weight that turns a per-query scanned-row count into
/// a logical scanned-bytes figure for the byte-rate control families; it is
/// never itself a ladder value.
const QUALIFICATION_LOGICAL_ROW_BYTES: u64 = 8 + 8 + 8 + 8 + 10 + 256;

/// Relative tolerance applied when comparing floating-point `SUM` aggregates.
///
/// DataFusion and the Rust reference generator accumulate `value` in different
/// orders, so bitwise equality is unattainable; a relative tolerance confirms
/// the aggregate is correct without demanding an identical summation order.
const SUM_RELATIVE_TOLERANCE: f64 = 1e-6;

/// Row cap applied to every collected query result.
///
/// Bounds the driver's per-query memory; the smoke ranges stay far below it.
const QUERY_MAX_ROWS: usize = 8_000_000;

/// Encoded-byte cap applied to every collected query result.
const QUERY_MAX_ENCODED_BYTES: usize = 256 * 1024 * 1024;

/// Terminal failure raised by a live family runner.
///
/// Every variant fails the lane closed: a cluster, capture, correctness, or
/// budget failure aborts before or instead of leaving a promotable artifact.
#[derive(Debug, thiserror::Error)]
pub enum FamilyRunError {
    /// A cluster lifecycle, provisioning, capture, or offer operation failed.
    #[error("Bifrost family runner failed: {0}")]
    Cluster(String),
    /// A measured stage observed unexpected (non-admission) write failures.
    #[error("Bifrost {family} stage {ordinal} observed {failed} unexpected write failures")]
    UnexpectedFailures {
        /// Family label whose stage failed.
        family: String,
        /// Ordinal of the failing stage.
        ordinal: u16,
        /// Number of unexpected failures observed.
        failed: u64,
    },
    /// A correctness probe query result disagreed with the expected dataset.
    #[error("Bifrost {query_id} correctness probe failed: {reason}")]
    Correctness {
        /// Query identifier whose probe failed.
        query_id: String,
        /// Human-readable mismatch description.
        reason: String,
    },
    /// Stage planning or budget enforcement failed.
    #[error(transparent)]
    Plan(#[from] super::bench_runner::RunnerPlanError),
    /// Report construction rejected the assembled evidence.
    #[error("Bifrost capacity report rejected: {0}")]
    Report(String),
    /// The canonical report disk writer failed.
    #[error("Bifrost capacity report write failed: {0}")]
    Write(String),
    /// Fixture resolution rejected an unknown workload or topology id.
    #[error(transparent)]
    Selection(#[from] super::bench_runner::RunnerCliError),
    /// A workload was routed to a binary that does not serve its family.
    #[error("Bifrost workload {workload_id} classifies as {family:?}, not served by this binary")]
    UnservedFamily {
        /// Workload id that was routed to the wrong binary.
        workload_id: String,
        /// Family the workload classifies as.
        family: RunnerFamily,
    },
}

/// Measured aggregate of one family's offered writes within a capture window.
///
/// Counts are kept in operation units so they map directly onto the
/// [`WorkCounts`] the healthy-stage predicate compares, and latencies are
/// retained per admitted write so the runner can derive a total percentile.
#[derive(Debug, Default)]
struct WriteWindow {
    /// Total writes offered (every dispatched attempt).
    offered: u64,
    /// Writes admitted and durably completed.
    admitted: u64,
    /// Writes refused by controlled admission backpressure.
    rejected: u64,
    /// Writes that failed unexpectedly.
    failed: u64,
    /// Per-admitted-write latencies in microseconds.
    latencies_us: Vec<u64>,
}

/// One offered write's classified outcome.
///
/// Mirrors the journey's stable classification: an explicit admission or
/// backpressure signal is controlled load-shedding, an exhausted retry or any
/// other error is an unexpected failure that makes the stage unhealthy.
enum WriteOutcome {
    /// Admitted and completed with the given microsecond latency.
    Admitted(u64),
    /// Refused by controlled admission backpressure.
    Rejected,
    /// Failed unexpectedly.
    Failed,
}

/// Load the fixtures, resolve the invocation, and run its classified family.
///
/// The single dispatch entry both benchmark binaries call. It loads the embedded
/// workload, topology, and tier-budget fixtures, resolves the invocation against
/// them (fail-closed on an unknown id before any cluster starts), classifies the
/// family from the resolved workload, and rejects a workload routed to a binary
/// that does not serve its family. The lane wall-clock budget is measured from
/// the moment this function is entered, so every runner shares one budget origin.
///
/// # Errors
/// Returns [`FamilyRunError::Selection`] for an unresolved id or a family the
/// caller does not serve, or any [`FamilyRunError`] raised by the selected runner.
pub async fn run_family(
    invocation: &RunnerInvocation,
    allowed: &[RunnerFamily],
) -> Result<std::path::PathBuf, FamilyRunError> {
    run_family_dispatch(invocation, allowed, None).await
}

/// Run a classified family over caller-supplied run-shared resources.
///
/// The qualification-tier sibling of [`run_family`]. It threads one run's
/// [`SharedRunResources`] handle into the selected runner so every family
/// cluster it boots observes the once-materialized qualification dataset over a
/// single shared fixture and storage root, rather than materializing a private
/// dataset per family. Smoke behavior is reached through [`run_family`] with a
/// `None` handle and stays byte-identical; this entry is the seam the T32
/// qualification orchestrator drives after it provisions the shared resources.
///
/// # Errors
/// Returns [`FamilyRunError::Selection`] for an unresolved id or a family the
/// caller does not serve, or any [`FamilyRunError`] raised by the selected runner.
pub async fn run_family_with_resources(
    invocation: &RunnerInvocation,
    allowed: &[RunnerFamily],
    resources: &SharedRunResources,
) -> Result<std::path::PathBuf, FamilyRunError> {
    run_family_dispatch(invocation, allowed, Some(resources)).await
}

/// Resolve the invocation and dispatch its family, optionally over shared resources.
///
/// The shared body behind [`run_family`] and [`run_family_with_resources`]. It
/// loads the embedded workload, topology, and tier-budget fixtures, resolves the
/// invocation against them (fail-closed on an unknown id before any cluster
/// starts), classifies the family, rejects a workload routed to a binary that
/// does not serve its family, and forwards the optional shared-resource handle to
/// the selected runner. The lane wall-clock budget is measured from entry so
/// every runner shares one budget origin.
///
/// # Errors
/// Returns [`FamilyRunError::Selection`] for an unresolved id or unserved family,
/// or any [`FamilyRunError`] raised by the selected runner.
async fn run_family_dispatch(
    invocation: &RunnerInvocation,
    allowed: &[RunnerFamily],
    resources: Option<&SharedRunResources>,
) -> Result<std::path::PathBuf, FamilyRunError> {
    let lane_start = Instant::now();
    let workloads = WorkloadProfiles::load();
    let topologies = TopologyProfiles::load();
    let budgets = TierBudgets::load();
    let resolved = invocation.resolve(&workloads, &topologies)?;
    let family = RunnerFamily::classify(resolved.workload);
    if !allowed.contains(&family) {
        return Err(FamilyRunError::UnservedFamily {
            workload_id: resolved.workload.workload_id.clone(),
            family,
        });
    }
    match family {
        RunnerFamily::Ingest => {
            run_ingest(lane_start, invocation, resolved, &budgets, resources).await
        }
        RunnerFamily::Query => {
            run_query(lane_start, invocation, resolved, &budgets, resources).await
        }
        RunnerFamily::Distributed => {
            run_distributed(lane_start, invocation, resolved, &budgets, resources).await
        }
        RunnerFamily::Fairness => {
            run_fairness(lane_start, invocation, resolved, &budgets, resources).await
        }
        RunnerFamily::Mixed => {
            run_mixed(lane_start, invocation, resolved, &budgets, resources).await
        }
    }
}

/// Boot a family cluster over run-shared resources when present, else standalone.
///
/// The single cluster-boot seam every runner uses. With `Some` shared resources
/// (qualification tier) it starts the topology over the run's shared fixture,
/// storage root, and once-provisioned oracle peer credentials so the cluster
/// observes the run's already-materialized dataset. With `None` (smoke tier) it
/// boots a byte-identical standalone cluster with a Forge worker-completion
/// observer, exactly as every runner did before this seam existed.
///
/// # Errors
/// Returns [`FamilyRunError::Cluster`] when the underlying cluster start fails.
async fn start_family_cluster(
    spec: BifrostClusterSpec,
    resources: Option<&SharedRunResources>,
) -> Result<WyrdTestCluster, FamilyRunError> {
    let started = match resources {
        Some(shared) => WyrdTestCluster::start_spec_with_shared_resources(spec, shared).await,
        None => WyrdTestCluster::start_spec_with_forge_completion_observer(spec).await,
    };
    started.map_err(|error| FamilyRunError::Cluster(error.to_string()))
}

/// Run the ingest family: pure open-loop public writes at each ladder rung.
///
/// Boots the invocation's topology, provisions one reference tenant and its
/// `row_id` table, and offers `requests_per_sec` writes for each planned stage's
/// measured window, binding each window to a T17 capture. Emits one
/// `ingest`-family [`BifrostCapacityReport`] under the invocation's output root
/// and returns the written artifact path. A smoke run executes exactly the first
/// ladder rung (per [`plan_stages`]) and enforces the per-lane smoke budget in
/// process.
///
/// # Errors
/// Returns [`FamilyRunError`] for a cluster/provisioning/capture failure, an
/// unexpected write failure, a rejected report, a budget breach, or a disk-write
/// failure.
///
/// # Panics
/// Does not panic; the bounded dispatch semaphore is never closed while offers
/// are in flight.
pub async fn run_ingest(
    lane_start: Instant,
    invocation: &RunnerInvocation,
    resolved: ResolvedInvocation<'_>,
    budgets: &TierBudgets,
    resources: Option<&SharedRunResources>,
) -> Result<std::path::PathBuf, FamilyRunError> {
    let family = "ingest";
    let workload_id = resolved.workload.workload_id.clone();
    let topology = topology_for_pods(resolved.topology.pods)?;
    let plan = plan_stages(resolved.workload, resolved.tier)?;
    // The qualification sweep shares one catalog across every family cluster in
    // the run, so it writes to a fresh per-run table to avoid colliding with the
    // mixed family's use of the smoke-default reference table. Smoke keeps the
    // reference table unchanged.
    let table = match resolved.tier {
        RunnerTier::Smoke => REFERENCE_TABLE.to_owned(),
        RunnerTier::Qualification => fresh_ingest_table(&invocation.run_id),
    };

    let cluster = start_family_cluster(topology.spec(), resources).await?;

    let result = run_ingest_stages(
        &cluster,
        invocation,
        resolved,
        &plan,
        family,
        &workload_id,
        &table,
    )
    .await;

    // Always attempt shutdown; a shutdown failure only overrides a prior success.
    let shutdown = cluster
        .shutdown_and_inspect()
        .await
        .map_err(|error| FamilyRunError::Cluster(error.to_string()));
    let (stages, saturation_reached, recommended) = result?;
    shutdown?;

    if let RunnerTier::Smoke = resolved.tier {
        enforce_budget(lane_start.elapsed(), budgets.smoke_lane_seconds)?;
    }

    let context = report_context(invocation, resolved);
    let body = IngestBody {
        profile_id: workload_id.clone(),
        saturation_reached,
        recommended,
    };
    let report = BifrostCapacityReport::new(context, stages, body)
        .map_err(|error| FamilyRunError::Report(error.to_string()))?;
    report
        .write_report(&invocation.output_root, &invocation.run_id, &workload_id)
        .map_err(|error| FamilyRunError::Write(error.to_string()))
}

/// Provision `table` and run every planned ingest stage against it.
///
/// Factored from [`run_ingest`] so cluster shutdown is guaranteed on both the
/// success and failure paths. `table` is the logical write target: the
/// smoke-default reference table at smoke tier, or a fresh per-run table at
/// qualification tier so a run-shared catalog sees no collision. Returns the
/// ordered measured stages plus the derived saturation outcome, stopping the
/// ladder after the first controlled saturation boundary (D73
/// saturation-optional).
///
/// # Errors
/// Returns [`FamilyRunError`] for a provisioning, capture, or unexpected-failure
/// condition encountered while offering load.
async fn run_ingest_stages(
    cluster: &WyrdTestCluster,
    invocation: &RunnerInvocation,
    resolved: ResolvedInvocation<'_>,
    plan: &[StagePlan],
    family: &str,
    workload_id: &str,
    table: &str,
) -> Result<(Vec<BifrostCapacityStage>, bool, Option<RecommendedStage>), FamilyRunError> {
    let tenants = provision_reference_tenants(cluster, 1)
        .await
        .map_err(|error| FamilyRunError::Cluster(error.to_string()))?;
    provision_named_tables(cluster, &tenants, &[table.to_owned()])
        .await
        .map_err(|error| FamilyRunError::Cluster(error.to_string()))?;
    let clients = reference_clients(cluster, &tenants)
        .await
        .map_err(|error| FamilyRunError::Cluster(error.to_string()))?;
    let client = clients
        .first()
        .ok_or_else(|| FamilyRunError::Cluster("ingest runner has no reference client".to_owned()))?
        .clone();

    let max_in_flight = resolved.workload.control.max_in_flight.map_or_else(
        || default_in_flight(resolved.workload.control.ladder.first().copied()),
        u64::from,
    );

    let mut stages = Vec::with_capacity(plan.len());
    for step in plan {
        let control = stage_control(&resolved.workload.control, step.control_value)?;
        let stage = measure_ingest_stage(
            cluster,
            &client,
            invocation,
            workload_id,
            control,
            step,
            max_in_flight,
            family,
            table,
        )
        .await?;
        let saturated = stage.saturation == Some(SaturationCause::ControlledAdmission);
        stages.push(stage);
        if saturated {
            break;
        }
    }

    let (saturation_reached, recommended) = derive_saturation_outcome(&mut stages);
    Ok((stages, saturation_reached, recommended))
}

/// Offer one ingest stage's measured window and assemble its causal stage.
///
/// The warmup and drain windows bracket the measured window; only the measured
/// window is T17-captured and bound into the stage's [`StageExecution`]. The
/// post-window flush is a drain-phase durability confirmation (never an
/// in-measurement force-seal, per D71).
///
/// # Errors
/// Returns [`FamilyRunError`] for a capture failure or unexpected write
/// failures observed during the measured window.
// justification: the qualification harness fixes this stage's inputs by contract; a single-use request wrapper would hide which of them the measured ingest window actually reads without removing one of them.
#[allow(clippy::too_many_arguments)]
async fn measure_ingest_stage(
    cluster: &WyrdTestCluster,
    client: &ReferenceClient,
    invocation: &RunnerInvocation,
    workload_id: &str,
    control: StageControl,
    step: &StagePlan,
    max_in_flight: u64,
    family: &str,
    table: &str,
) -> Result<BifrostCapacityStage, FamilyRunError> {
    if step.warmup_seconds > 0 {
        offer_writes(
            client,
            step.control_value,
            step.warmup_seconds,
            max_in_flight,
            table,
        )
        .await;
    }

    let wall_start = SystemTime::now();
    let (window, delta) = run_sampled_window(cluster.telemetry(), || async {
        Ok(offer_writes(
            client,
            step.control_value,
            step.measure_seconds,
            max_in_flight,
            table,
        )
        .await)
    })
    .await
    .map_err(|error| FamilyRunError::Cluster(error.to_string()))?;
    let wall_end = SystemTime::now();

    if step.drain_seconds > 0 {
        offer_writes(
            client,
            step.control_value,
            step.drain_seconds,
            max_in_flight,
            table,
        )
        .await;
    }
    super::bench_cluster::flush_tenant_writers(
        cluster,
        std::slice::from_ref(&cluster.data_tenant_id()),
    )
    .await
    .map_err(|error| FamilyRunError::Cluster(error.to_string()))?;

    if window.failed > 0 {
        return Err(FamilyRunError::UnexpectedFailures {
            family: family.to_owned(),
            ordinal: step.ordinal,
            failed: window.failed,
        });
    }

    let execution = StageExecution {
        run_id: invocation.run_id.clone(),
        ordinal: step.ordinal,
        capture_window: capture_window_ref(
            invocation,
            workload_id,
            step.ordinal,
            wall_start,
            wall_end,
        ),
        observation_digests: vec![observation_digest(&delta)],
    };

    Ok(build_ingest_stage(
        workload_id,
        control,
        step.ordinal,
        &window,
        &delta,
        execution,
    ))
}

/// Offer open-loop writes at `rate` per second for `seconds` and classify them.
///
/// Dispatch is bounded by `max_in_flight` acquired-permit concurrency; a late
/// wakeup still offers its planned operation, so `offered` always equals the
/// planned operation count. Latencies are measured only across the admitted
/// send, excluding any permit-acquisition wait. Every write targets `table`,
/// which is the smoke-default reference table at smoke tier and a fresh per-run
/// ingest table at qualification tier so a run-shared catalog sees no collision.
async fn offer_writes(
    client: &ReferenceClient,
    rate: u64,
    seconds: u32,
    max_in_flight: u64,
    table: &str,
) -> WriteWindow {
    let mut window = WriteWindow::default();
    let total_ops = rate.saturating_mul(u64::from(seconds));
    if total_ops == 0 || rate == 0 {
        return window;
    }
    window.offered = total_ops;
    let interval = Duration::from_secs_f64(1.0 / rate as f64);
    let permits = usize::try_from(max_in_flight.clamp(1, MAX_IN_FLIGHT_CAP)).unwrap_or(1);
    let semaphore = Arc::new(Semaphore::new(permits));
    let start = tokio::time::Instant::now();
    let mut tasks = JoinSet::new();
    for ordinal in 0..total_ops {
        tokio::time::sleep_until(start + interval.mul_f64(ordinal as f64)).await;
        let permit = Arc::clone(&semaphore)
            .acquire_owned()
            .await
            .expect("dispatch semaphore is never closed while offers are in flight");
        let client = client.clone();
        let table = table.to_owned();
        tasks.spawn(async move {
            let _permit = permit;
            send_one_write(client, ordinal, table).await
        });
    }
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok(WriteOutcome::Admitted(latency_us)) => {
                window.admitted = window.admitted.saturating_add(1);
                window.latencies_us.push(latency_us);
            }
            Ok(WriteOutcome::Rejected) => window.rejected = window.rejected.saturating_add(1),
            Ok(WriteOutcome::Failed) | Err(_) => window.failed = window.failed.saturating_add(1),
        }
    }
    window
}

/// Send one deterministic 64-row write to `table` and classify its outcome.
///
/// Writes into the single reference tenant's disjoint row range above the
/// preload window so successive stages never collide. A bounded retry mirrors
/// the journey's transient-retry policy before classifying an exhausted retry as
/// an unexpected failure. `table` is the logical table name (without the
/// `vala.bifrost.` namespace prefix this function applies) so an ingest sweep
/// over run-shared resources can target a fresh per-run table.
async fn send_one_write(client: ReferenceClient, ordinal: u64, table: String) -> WriteOutcome {
    let first_row = tenant_row_base(0)
        .saturating_add(super::bench_cluster::PRELOAD_ROWS)
        .saturating_add(ordinal.saturating_mul(u64::from(ROWS_PER_WRITE)));
    let payload = match reference_payload(first_row, ROWS_PER_WRITE) {
        Ok(payload) => payload,
        Err(_) => return WriteOutcome::Failed,
    };
    let frame = BifrostFrame {
        table: format!("vala.bifrost.{table}"),
        batch_id: deterministic_batch_id(0, ordinal),
        arrow_ipc: payload.into(),
    };
    let started = Instant::now();
    let mut retries = 0_u64;
    loop {
        match client.writer.send_frame(frame.clone()).await {
            Ok(()) => {
                return WriteOutcome::Admitted(
                    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
                );
            }
            Err(error) if is_backpressure(&error.to_string()) => return WriteOutcome::Rejected,
            Err(error) if is_retryable(&error.to_string()) && retries < 2 => {
                retries = retries.saturating_add(1);
            }
            Err(_) => return WriteOutcome::Failed,
        }
    }
}

/// Assemble one causally-bound ingest stage from its measured window.
///
/// Populates the [`WorkCounts`] and health/saturation fields so the stage's
/// declared `healthy` flag and `saturation` marker recompute exactly under
/// [`BifrostCapacityReport`] validation: a fully admitted window is healthy, a
/// controlled-admission-only window is the sole saturation shape, and any other
/// nonhealthy shape carries no saturation marker.
fn build_ingest_stage(
    workload_id: &str,
    control: StageControl,
    ordinal: u16,
    window: &WriteWindow,
    delta: &BifrostTelemetryDelta,
    execution: StageExecution,
) -> BifrostCapacityStage {
    let rows = window.admitted.saturating_mul(u64::from(ROWS_PER_WRITE));
    let logical_bytes = rows.saturating_mul(LOGICAL_BYTES_PER_ROW);
    let work = |operations: u64| WorkCounts {
        operations,
        rows: operations.saturating_mul(u64::from(ROWS_PER_WRITE)),
        bytes: operations
            .saturating_mul(u64::from(ROWS_PER_WRITE))
            .saturating_mul(LOGICAL_BYTES_PER_ROW),
    };
    let latency = LatencyPercentiles {
        total: percentile_us(&window.latencies_us, 99.0),
        ..LatencyPercentiles::default()
    };
    let audit = AuditRelaySample::default();
    let correctness = if window.failed == 0 {
        CorrectnessVerdict::Passed
    } else {
        CorrectnessVerdict::Failed {
            reason: format!("{} unexpected write failures", window.failed),
        }
    };
    let healthy = window.rejected == 0
        && window.failed == 0
        && window.offered == window.admitted
        && matches!(correctness, CorrectnessVerdict::Passed)
        && latency.total.is_some();
    let controlled_admission_only = window.rejected > 0
        && window.failed == 0
        && window.offered == window.admitted.saturating_add(window.rejected)
        && matches!(correctness, CorrectnessVerdict::Passed)
        && latency.total.is_some();
    let saturation =
        (!healthy && controlled_admission_only).then_some(SaturationCause::ControlledAdmission);
    BifrostCapacityStage {
        profile_id: workload_id.to_owned(),
        ordinal,
        control,
        offered: work(window.offered),
        admitted: work(window.admitted),
        completed: work(window.admitted),
        rejected: window.rejected,
        failed: window.failed,
        latency,
        logical_bytes,
        physical_bytes: None,
        unavailable_reason: Some("physical bytes not exposed by the reference backend".to_owned()),
        rows_returned: 0,
        bytes_returned: 0,
        resources: ResourceSample {
            cpu_seconds: Some(delta.process.cpu_seconds),
            resident_bytes: Some(delta.process.current_rss_bytes),
            peak_query_memory_bytes: None,
            spill_bytes: None,
            network_bytes: None,
        },
        audit,
        fairness: None,
        correctness,
        healthy,
        saturation,
        recovery_replay: None,
        stage_execution: execution,
    }
}

/// Measured aggregate of one query family's offered reads within a window.
///
/// Counts stay in operation units so they map onto the [`WorkCounts`] the
/// healthy-stage predicate compares, and each completed read contributes its
/// returned rows/bytes and a latency sample so the runner derives a total
/// percentile and the byte-rate throughput numerator.
#[derive(Debug, Default)]
struct QueryWindow {
    /// Total reads offered (every dispatched attempt).
    offered: u64,
    /// Reads that returned a successful terminal.
    completed: u64,
    /// Reads refused by controlled admission backpressure.
    rejected: u64,
    /// Reads that failed unexpectedly.
    failed: u64,
    /// Rows returned across all completed reads.
    rows_returned: u64,
    /// Terminal bytes returned across all completed reads.
    bytes_returned: u64,
    /// Per-completed-read latencies in microseconds.
    latencies_us: Vec<u64>,
}

/// One offered read's classified outcome.
enum QueryOutcome {
    /// Completed with a successful terminal, returning `rows`/`encoded_bytes`.
    Completed {
        /// Rows returned by the completed read.
        rows: u64,
        /// Terminal-encoded bytes returned by the completed read.
        encoded_bytes: u64,
        /// Wall-clock latency of the completed read in microseconds.
        latency_us: u64,
    },
    /// Refused by controlled admission backpressure.
    Rejected,
    /// Failed unexpectedly.
    Failed,
}

/// Run one query family: fixed-query load at each ladder rung.
///
/// Boots the invocation's topology, acquires the qualification dataset for the
/// tier (smoke materializes the fixture smoke shape into the cluster; the
/// qualification tier opens the verified run scope and reuses its once-
/// materialized dataset after a digest check), probes the query's correctness
/// once against the deterministic expected results, then
/// offers the fixed query at each planned stage's measured window bound to a T17
/// capture. Emits one `query`-family [`BifrostCapacityReport`] under the
/// invocation's output root and returns the written artifact path. A smoke run
/// executes exactly the first ladder rung and enforces the smoke lane budget.
///
/// The control unit selects the offer law and the reported throughput: `qps`
/// and `requests_per_sec` offer open-loop at `value` reads/sec; `concurrency`,
/// `streams`, `logical_bytes_per_sec`, and `returned_bytes_per_sec` hold `value`
/// reads in flight for the measured window (the unit selecting only which
/// throughput the completions are reported as).
///
/// # Errors
/// Returns [`FamilyRunError`] for a cluster/materialization failure, a missing
/// `query_id`, a correctness mismatch, an unexpected read failure, a rejected
/// report, a budget breach, or a disk-write failure.
///
/// # Panics
/// Does not panic; the bounded dispatch semaphore is never closed while offers
/// are in flight.
pub async fn run_query(
    lane_start: Instant,
    invocation: &RunnerInvocation,
    resolved: ResolvedInvocation<'_>,
    budgets: &TierBudgets,
    resources: Option<&SharedRunResources>,
) -> Result<std::path::PathBuf, FamilyRunError> {
    let workload_id = resolved.workload.workload_id.clone();
    let query_id = resolved.workload.query_id.clone().ok_or_else(|| {
        FamilyRunError::Cluster(format!("query workload {workload_id} declares no query_id"))
    })?;
    let topology = topology_for_pods(resolved.topology.pods)?;
    let plan = plan_stages(resolved.workload, resolved.tier)?;

    let cluster = start_family_cluster(topology.spec(), resources).await?;

    let result = run_query_stages(
        &cluster,
        invocation,
        resolved,
        &plan,
        &workload_id,
        &query_id,
    )
    .await;

    // Always attempt shutdown; a shutdown failure only overrides a prior success.
    let shutdown = cluster
        .shutdown_and_inspect()
        .await
        .map_err(|error| FamilyRunError::Cluster(error.to_string()));
    let (stages, saturation_reached, recommended) = result?;
    shutdown?;

    if let RunnerTier::Smoke = resolved.tier {
        enforce_budget(lane_start.elapsed(), budgets.smoke_lane_seconds)?;
    }

    let context = report_context(invocation, resolved);
    let body = QueryBody {
        query_id,
        saturation_reached,
        recommended,
    };
    let report = BifrostCapacityReport::new(context, stages, body)
        .map_err(|error| FamilyRunError::Report(error.to_string()))?;
    report
        .write_report(&invocation.output_root, &invocation.run_id, &workload_id)
        .map_err(|error| FamilyRunError::Write(error.to_string()))
}

/// Acquire the dataset, probe correctness, and run every planned query stage.
///
/// Factored from [`run_query`] so cluster shutdown is guaranteed on both the
/// success and failure paths. Returns the ordered measured stages plus the
/// derived saturation outcome, stopping the ladder after the first controlled
/// saturation boundary (D73 saturation-optional).
///
/// # Errors
/// Returns [`FamilyRunError`] for a dataset-acquisition, client, correctness,
/// capture, or unexpected-failure condition encountered while offering load.
async fn run_query_stages(
    cluster: &WyrdTestCluster,
    invocation: &RunnerInvocation,
    resolved: ResolvedInvocation<'_>,
    plan: &[StagePlan],
    workload_id: &str,
    query_id: &str,
) -> Result<(Vec<BifrostCapacityStage>, bool, Option<RecommendedStage>), FamilyRunError> {
    let dataset = acquire_query_dataset(cluster, invocation, resolved, workload_id).await?;
    let queries = dataset.queries();
    let expected = dataset.expected_results();
    let shape = dataset.shape();
    let sql = query_sql(query_id, &queries, shape)?;
    let scan_logical_bytes =
        scan_range_rows(query_id, &queries, shape)?.saturating_mul(QUALIFICATION_LOGICAL_ROW_BYTES);

    let clients = reference_clients(cluster, std::slice::from_ref(&cluster.data_tenant_id()))
        .await
        .map_err(|error| FamilyRunError::Cluster(error.to_string()))?;
    let query = clients
        .first()
        .ok_or_else(|| FamilyRunError::Cluster("query runner has no reference client".to_owned()))?
        .query
        .clone();

    probe_query_correctness(&query, query_id, &sql, &expected).await?;

    let mut stages = Vec::with_capacity(plan.len());
    for step in plan {
        let control = stage_control(&resolved.workload.control, step.control_value)?;
        let stage = measure_query_stage(
            cluster,
            &query,
            invocation,
            workload_id,
            control,
            step,
            &sql,
            scan_logical_bytes,
        )
        .await?;
        let saturated = stage.saturation == Some(SaturationCause::ControlledAdmission);
        stages.push(stage);
        if saturated {
            break;
        }
    }

    let (saturation_reached, recommended) = derive_saturation_outcome(&mut stages);
    Ok((stages, saturation_reached, recommended))
}

/// Acquire the qualification dataset appropriate to the run tier.
///
/// The smoke tier materializes the fixture smoke shape into a fresh run scope on
/// the booted cluster (T29 seam). The qualification tier opens the verified run
/// scope named by `--run-id` and reuses its once-materialized dataset, gated by
/// a `verify_reuse` fingerprint check on the source commit, dataset digest,
/// shape, and storage configuration; it refuses only when that fingerprint does
/// not match, never fabricating qualification-tier data.
///
/// # Errors
/// Returns [`FamilyRunError::Cluster`] for a smoke materialization failure or,
/// on the qualification tier, when the run record cannot be opened, its recorded
/// shape is invalid, or the reuse fingerprint does not match the current
/// environment.
async fn acquire_query_dataset(
    cluster: &WyrdTestCluster,
    invocation: &RunnerInvocation,
    resolved: ResolvedInvocation<'_>,
    workload_id: &str,
) -> Result<BifrostQualificationDataset, FamilyRunError> {
    match resolved.tier {
        RunnerTier::Smoke => materialize_smoke_dataset(cluster, invocation, workload_id).await,
        RunnerTier::Qualification => acquire_qualification_dataset(invocation),
    }
}

/// Open the verified qualification run scope named by `--run-id` and reuse it.
///
/// Opens the persisted run record under `<output_root>/<run_id>`, reconstructs
/// the canonical-anchor dataset for the recorded shape, and refuses reuse unless
/// the current qualified source commit, dataset digest, shape, and storage
/// configuration all match what the run recorded. Reuse never crosses source
/// commits and never fabricates qualification data: a mismatch fails before any
/// cluster work with a field diagnostic. The returned dataset supplies the
/// deterministic queries and expected results the family runner probes against
/// the once-materialized run store.
///
/// # Errors
/// Returns [`FamilyRunError::Cluster`] when the run record cannot be opened, the
/// recorded shape is invalid, or the reuse fingerprint does not match the
/// current environment.
fn acquire_qualification_dataset(
    invocation: &RunnerInvocation,
) -> Result<BifrostQualificationDataset, FamilyRunError> {
    let run_root = invocation.output_root.join(&invocation.run_id);
    let record = QualificationRunRecord::open(&run_root)
        .map_err(|error| FamilyRunError::Cluster(error.to_string()))?;
    let shape = record.fingerprint.shape;
    let dataset = BifrostQualificationDataset::new(shape)
        .map_err(|error| FamilyRunError::Cluster(error.to_string()))?;
    let current = RunFingerprint {
        qualified_source_commit: git_head(),
        dataset_digest: dataset.manifest().digest,
        shape,
        storage_config: environment_identity().storage_mode,
    };
    record
        .fingerprint
        .verify_reuse(&current)
        .map_err(|error| FamilyRunError::Cluster(error.to_string()))?;
    Ok(dataset)
}

/// Materialize the fixture smoke shape into the cluster's data tenant.
///
/// Constructs the canonical-anchor dataset for its shape and content facts,
/// hands its shape to the materializer (which re-anchors event times onto a live
/// admission window while keeping every counted and grouped fact invariant), and
/// returns the canonical-anchor dataset so correctness comparisons use the
/// anchor-invariant expected results.
///
/// # Errors
/// Returns [`FamilyRunError::Cluster`] for an invalid shape, an absent Server,
/// or a materialization failure.
async fn materialize_smoke_dataset(
    cluster: &WyrdTestCluster,
    invocation: &RunnerInvocation,
    workload_id: &str,
) -> Result<BifrostQualificationDataset, FamilyRunError> {
    let tenant = cluster.data_tenant_id();
    materialize_smoke_dataset_into(
        cluster,
        invocation,
        workload_id,
        std::slice::from_ref(&tenant),
    )
    .await
}

/// Materialize the fixture smoke shape into every listed tenant.
///
/// The tenant-parameterized core of [`materialize_smoke_dataset`]: the fairness
/// and mixed families reuse it to seed the same anchor-invariant dataset into a
/// tenant matrix. The returned canonical-anchor dataset supplies the
/// anchor-invariant expected results correctness comparisons compare against; the
/// materializer re-anchors event times internally onto a live admission window.
///
/// # Errors
/// Returns [`FamilyRunError::Cluster`] for an invalid shape, an absent Server, or
/// a materialization failure against any tenant.
async fn materialize_smoke_dataset_into(
    cluster: &WyrdTestCluster,
    invocation: &RunnerInvocation,
    workload_id: &str,
    tenants: &[DataTenantId],
) -> Result<BifrostQualificationDataset, FamilyRunError> {
    let dataset = BifrostQualificationDataset::new(smoke_dataset_shape())
        .map_err(|error| FamilyRunError::Cluster(error.to_string()))?;
    let server = cluster
        .server(0)
        .ok_or_else(|| FamilyRunError::Cluster("query runner cluster has no Server".to_owned()))?;
    let run_root = invocation
        .output_root
        .join(&invocation.run_id)
        .join(format!("materialize-{workload_id}"));
    let policy = BackpressurePolicy::with_deadline(Instant::now() + Duration::from_secs(120));
    let materializer = BifrostDatasetMaterializer::from_server(
        server,
        dataset,
        tenants.to_vec(),
        policy,
        run_root,
        CancellationToken::new(),
    )
    .await
    .map_err(|error| FamilyRunError::Cluster(error.to_string()))?;
    materializer
        .materialize()
        .await
        .map_err(|error| FamilyRunError::Cluster(error.to_string()))?;
    Ok(dataset)
}

/// Offer one query stage's measured window and assemble its causal stage.
///
/// The warmup and drain windows bracket the measured window; only the measured
/// window is T17-captured and bound into the stage's [`StageExecution`]. The
/// query's correctness is proved once before the ladder runs, so every measured
/// stage carries a passing correctness verdict.
///
/// # Errors
/// Returns [`FamilyRunError`] for a capture failure or unexpected read failures
/// observed during the measured window.
// justification: the qualification harness fixes this stage's inputs by contract; a single-use request wrapper would hide which of them the measured query window actually reads without removing one of them.
#[allow(clippy::too_many_arguments)]
async fn measure_query_stage(
    cluster: &WyrdTestCluster,
    query: &QueryClient,
    invocation: &RunnerInvocation,
    workload_id: &str,
    control: StageControl,
    step: &StagePlan,
    sql: &str,
    scan_logical_bytes_per_query: u64,
) -> Result<BifrostCapacityStage, FamilyRunError> {
    if step.warmup_seconds > 0 {
        offer_queries(query, &control, sql, step.warmup_seconds).await;
    }

    let wall_start = SystemTime::now();
    let (window, delta) = run_sampled_window(cluster.telemetry(), || async {
        Ok(offer_queries(query, &control, sql, step.measure_seconds).await)
    })
    .await
    .map_err(|error| FamilyRunError::Cluster(error.to_string()))?;
    let wall_end = SystemTime::now();

    if step.drain_seconds > 0 {
        offer_queries(query, &control, sql, step.drain_seconds).await;
    }

    if window.failed > 0 {
        return Err(FamilyRunError::UnexpectedFailures {
            family: workload_id.to_owned(),
            ordinal: step.ordinal,
            failed: window.failed,
        });
    }

    let execution = StageExecution {
        run_id: invocation.run_id.clone(),
        ordinal: step.ordinal,
        capture_window: capture_window_ref(
            invocation,
            workload_id,
            step.ordinal,
            wall_start,
            wall_end,
        ),
        observation_digests: vec![observation_digest(&delta)],
    };

    Ok(build_query_stage(
        workload_id,
        control,
        step.ordinal,
        &window,
        scan_logical_bytes_per_query,
        &delta,
        execution,
    ))
}

/// Offer the fixed query for `seconds` under the control's offer law.
///
/// Open-loop laws (`qps`, `requests_per_sec`) offer `value` reads per second
/// bounded by the control's dispatch concurrency. Closed-loop laws
/// (`concurrency`, `streams`, `logical_bytes_per_sec`, `returned_bytes_per_sec`)
/// hold `value` reads in flight for the whole window; the unit selects only how
/// the completions are reported, not how the load is shaped.
async fn offer_queries(
    query: &QueryClient,
    control: &StageControl,
    sql: &str,
    seconds: u32,
) -> QueryWindow {
    match *control {
        StageControl::Qps {
            value,
            max_in_flight,
        } => offer_open_loop(query, sql, value, seconds, u64::from(max_in_flight)).await,
        StageControl::RequestsPerSec { value } => {
            offer_open_loop(query, sql, value, seconds, default_in_flight(Some(value))).await
        }
        StageControl::Concurrency { value }
        | StageControl::Streams { value }
        | StageControl::LogicalBytesPerSec { value }
        | StageControl::ReturnedBytesPerSec { value } => {
            offer_closed_loop(query, sql, value, seconds).await
        }
    }
}

/// Offer open-loop reads at `rate` per second for `seconds` and classify them.
///
/// Dispatch is bounded by `max_in_flight` acquired-permit concurrency; a late
/// wakeup still offers its planned operation, so `offered` always equals the
/// planned operation count. Latencies are measured only across the read itself,
/// excluding any permit-acquisition wait.
async fn offer_open_loop(
    query: &QueryClient,
    sql: &str,
    rate: u64,
    seconds: u32,
    max_in_flight: u64,
) -> QueryWindow {
    let mut window = QueryWindow::default();
    let total_ops = rate.saturating_mul(u64::from(seconds));
    if total_ops == 0 || rate == 0 {
        return window;
    }
    window.offered = total_ops;
    let interval = Duration::from_secs_f64(1.0 / rate as f64);
    let permits = usize::try_from(max_in_flight.clamp(1, MAX_IN_FLIGHT_CAP)).unwrap_or(1);
    let semaphore = Arc::new(Semaphore::new(permits));
    let start = tokio::time::Instant::now();
    let mut tasks = JoinSet::new();
    for ordinal in 0..total_ops {
        tokio::time::sleep_until(start + interval.mul_f64(ordinal as f64)).await;
        let permit = Arc::clone(&semaphore)
            .acquire_owned()
            .await
            .expect("dispatch semaphore is never closed while offers are in flight");
        let query = query.clone();
        let sql = sql.to_owned();
        tasks.spawn(async move {
            let _permit = permit;
            execute_one_query(query, sql).await
        });
    }
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok(outcome) => record_query_outcome(&mut window, outcome),
            Err(_) => window.failed = window.failed.saturating_add(1),
        }
    }
    window
}

/// Hold `workers` reads in flight for `seconds`, classifying every completion.
///
/// Each worker loops executing the query until the shared deadline, so the
/// stage saturates the fixed concurrency the rung declares. `offered` is the
/// total dispatched (completed plus rejected plus failed), which the healthy and
/// controlled-admission predicates compare against the completed count.
async fn offer_closed_loop(
    query: &QueryClient,
    sql: &str,
    workers: u64,
    seconds: u32,
) -> QueryWindow {
    let mut window = QueryWindow::default();
    let workers = workers.min(MAX_IN_FLIGHT_CAP);
    if workers == 0 || seconds == 0 {
        return window;
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(u64::from(seconds));
    let mut tasks = JoinSet::new();
    for _ in 0..workers {
        let query = query.clone();
        let sql = sql.to_owned();
        tasks.spawn(async move {
            let mut outcomes = Vec::new();
            while tokio::time::Instant::now() < deadline {
                outcomes.push(execute_one_query(query.clone(), sql.clone()).await);
            }
            outcomes
        });
    }
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok(outcomes) => {
                for outcome in outcomes {
                    record_query_outcome(&mut window, outcome);
                }
            }
            Err(_) => window.failed = window.failed.saturating_add(1),
        }
    }
    window.offered = window
        .completed
        .saturating_add(window.rejected)
        .saturating_add(window.failed);
    window
}

/// Fold one classified read outcome into the measured window.
fn record_query_outcome(window: &mut QueryWindow, outcome: QueryOutcome) {
    match outcome {
        QueryOutcome::Completed {
            rows,
            encoded_bytes,
            latency_us,
        } => {
            window.completed = window.completed.saturating_add(1);
            window.rows_returned = window.rows_returned.saturating_add(rows);
            window.bytes_returned = window.bytes_returned.saturating_add(encoded_bytes);
            window.latencies_us.push(latency_us);
        }
        QueryOutcome::Rejected => window.rejected = window.rejected.saturating_add(1),
        QueryOutcome::Failed => window.failed = window.failed.saturating_add(1),
    }
}

/// Execute one bounded public query and classify its terminal outcome.
///
/// A backpressure error is controlled load-shedding; any other error or a
/// non-success terminal is an unexpected failure. The latency spans the whole
/// bounded collection through terminal validation.
async fn execute_one_query(query: QueryClient, sql: String) -> QueryOutcome {
    let request = BifrostQueryRequest {
        sql,
        visibility: VisibilityMode::PublishedOnly,
        freshness: FreshnessPolicy::Strict,
        deadline_ms: Some(30_000),
    };
    let started = Instant::now();
    match query
        .collect_bounded(
            &request,
            CollectedQueryLimits {
                max_rows: QUERY_MAX_ROWS,
                max_encoded_bytes: QUERY_MAX_ENCODED_BYTES,
            },
        )
        .await
    {
        Ok(result)
            if result.terminal.outcome == QueryTerminalOutcome::Success
                && result.terminal.error.is_none() =>
        {
            QueryOutcome::Completed {
                rows: u64::try_from(result.rows).unwrap_or(u64::MAX),
                encoded_bytes: u64::try_from(result.encoded_bytes).unwrap_or(u64::MAX),
                latency_us: u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
            }
        }
        Ok(_) => QueryOutcome::Failed,
        Err(error) if is_backpressure(&error.to_string()) => QueryOutcome::Rejected,
        Err(_) => QueryOutcome::Failed,
    }
}

/// Assemble one causally-bound query stage from its measured window.
///
/// Populates the [`WorkCounts`] and health/saturation fields so the stage's
/// declared `healthy` flag and `saturation` marker recompute exactly under
/// [`BifrostCapacityReport`] validation: a fully completed window is healthy, a
/// controlled-admission-only window is the sole saturation shape, and any other
/// nonhealthy shape carries no saturation marker. Logical bytes are the
/// completed reads times the per-query scanned-byte weight.
// justification: the qualification harness fixes this stage's inputs by contract; a single-use request wrapper would hide which of them the constructed query stage actually reads without removing one of them.
#[allow(clippy::too_many_arguments)]
fn build_query_stage(
    workload_id: &str,
    control: StageControl,
    ordinal: u16,
    window: &QueryWindow,
    scan_logical_bytes_per_query: u64,
    delta: &BifrostTelemetryDelta,
    execution: StageExecution,
) -> BifrostCapacityStage {
    let logical_bytes = window
        .completed
        .saturating_mul(scan_logical_bytes_per_query);
    let work = |operations: u64| WorkCounts {
        operations,
        rows: 0,
        bytes: operations.saturating_mul(scan_logical_bytes_per_query),
    };
    let latency = LatencyPercentiles {
        total: percentile_us(&window.latencies_us, 99.0),
        ..LatencyPercentiles::default()
    };
    let audit = AuditRelaySample::default();
    let correctness = CorrectnessVerdict::Passed;
    let healthy = window.rejected == 0
        && window.failed == 0
        && window.offered == window.completed
        && matches!(correctness, CorrectnessVerdict::Passed)
        && latency.total.is_some();
    let controlled_admission_only = window.rejected > 0
        && window.failed == 0
        && window.offered == window.completed.saturating_add(window.rejected)
        && matches!(correctness, CorrectnessVerdict::Passed)
        && latency.total.is_some();
    let saturation =
        (!healthy && controlled_admission_only).then_some(SaturationCause::ControlledAdmission);
    BifrostCapacityStage {
        profile_id: workload_id.to_owned(),
        ordinal,
        control,
        offered: work(window.offered),
        admitted: work(window.completed),
        completed: work(window.completed),
        rejected: window.rejected,
        failed: window.failed,
        latency,
        logical_bytes,
        physical_bytes: None,
        unavailable_reason: Some("physical bytes not exposed by the reference backend".to_owned()),
        rows_returned: window.rows_returned,
        bytes_returned: window.bytes_returned,
        resources: ResourceSample {
            cpu_seconds: Some(delta.process.cpu_seconds),
            resident_bytes: Some(delta.process.current_rss_bytes),
            peak_query_memory_bytes: None,
            spill_bytes: None,
            network_bytes: None,
        },
        audit,
        fairness: None,
        correctness,
        healthy,
        saturation,
        recovery_replay: None,
        stage_execution: execution,
    }
}

/// Build the deterministic SQL for one query id over the qualification table.
///
/// Every day-indexed predicate is expanded to a `row_id` range using the shape's
/// `rows_per_day`, matching the reference [`QualificationExpectedResults`]
/// computation exactly so the probe compares like for like.
///
/// # Errors
/// Returns [`FamilyRunError::Correctness`] for an unrecognized query id.
fn query_sql(
    query_id: &str,
    queries: &QualificationQueries,
    shape: DatasetShape,
) -> Result<String, FamilyRunError> {
    let table = format!("vala.bifrost.{QUALIFICATION_TABLE}");
    let rows_per_day = shape.rows_per_day;
    let sql = match query_id {
        "q1" => format!(
            "SELECT row_id, device_id, metric, value, payload FROM {table} WHERE row_id = {}",
            queries.q1_row_id
        ),
        "q2" => {
            let start = u64::from(queries.q2_day).saturating_mul(rows_per_day);
            let end = start.saturating_add(rows_per_day);
            format!(
                "SELECT COUNT(*) AS c FROM {table} WHERE row_id >= {start} AND row_id < {end} \
                 AND device_id < {}",
                queries.q2_device_less_than
            )
        }
        "q3" => {
            let start = u64::from(queries.q3_day).saturating_mul(rows_per_day);
            let end = start.saturating_add(rows_per_day / 2);
            format!(
                "SELECT metric, COUNT(*) AS c, SUM(value) AS s FROM {table} \
                 WHERE row_id >= {start} AND row_id < {end} GROUP BY metric ORDER BY metric"
            )
        }
        "q4" => {
            let start = u64::from(queries.q4_start_day).saturating_mul(rows_per_day);
            let end = u64::from(queries.q4_end_day).saturating_mul(rows_per_day);
            format!(
                "SELECT device_id % 256 AS bucket, COUNT(*) AS c, SUM(value) AS s FROM {table} \
                 WHERE row_id >= {start} AND row_id < {end} \
                 GROUP BY device_id % 256 ORDER BY device_id % 256"
            )
        }
        "q5" => format!(
            "SELECT row_id, device_id, metric, value, payload FROM {table} \
             WHERE row_id >= {} AND row_id < {} ORDER BY row_id",
            queries.q5_start_row_id, queries.q5_end_row_id
        ),
        other => {
            return Err(FamilyRunError::Correctness {
                query_id: other.to_owned(),
                reason: format!("unknown query id {other}"),
            });
        }
    };
    Ok(sql)
}

/// Return the number of rows one query scans, for the logical-byte weight.
///
/// # Errors
/// Returns [`FamilyRunError::Correctness`] for an unrecognized query id.
fn scan_range_rows(
    query_id: &str,
    queries: &QualificationQueries,
    shape: DatasetShape,
) -> Result<u64, FamilyRunError> {
    let rows_per_day = shape.rows_per_day;
    let rows = match query_id {
        "q1" => 1,
        "q2" => rows_per_day,
        "q3" => rows_per_day / 2,
        "q4" => u64::from(queries.q4_end_day.saturating_sub(queries.q4_start_day))
            .saturating_mul(rows_per_day),
        "q5" => u64::try_from(
            queries
                .q5_end_row_id
                .saturating_sub(queries.q5_start_row_id),
        )
        .unwrap_or(0),
        other => {
            return Err(FamilyRunError::Correctness {
                query_id: other.to_owned(),
                reason: format!("unknown query id {other}"),
            });
        }
    };
    Ok(rows)
}

/// Prove the query returns the deterministic expected result once, up front.
///
/// Compares only anchor-invariant facts: row identity and content for Q1, exact
/// counts for Q2/Q5, and count plus tolerance-bounded sum per group for Q3/Q4.
/// A single successful proof lets every measured stage assert a passing
/// correctness verdict without re-verifying under load.
///
/// # Errors
/// Returns [`FamilyRunError::Cluster`] for a probe execution failure and
/// [`FamilyRunError::Correctness`] for a result that disagrees with the dataset.
async fn probe_query_correctness(
    query: &QueryClient,
    query_id: &str,
    sql: &str,
    expected: &QualificationExpectedResults,
) -> Result<(), FamilyRunError> {
    let request = BifrostQueryRequest {
        sql: sql.to_owned(),
        visibility: VisibilityMode::PublishedOnly,
        freshness: FreshnessPolicy::Strict,
        deadline_ms: Some(60_000),
    };
    let result = query
        .collect_bounded(
            &request,
            CollectedQueryLimits {
                max_rows: QUERY_MAX_ROWS,
                max_encoded_bytes: QUERY_MAX_ENCODED_BYTES,
            },
        )
        .await
        .map_err(|error| FamilyRunError::Cluster(error.to_string()))?;
    if result.terminal.outcome != QueryTerminalOutcome::Success || result.terminal.error.is_some() {
        return Err(FamilyRunError::Correctness {
            query_id: query_id.to_owned(),
            reason: "correctness probe query did not terminate successfully".to_owned(),
        });
    }
    let batches = &result.batches;
    let outcome = match query_id {
        "q1" => compare_q1(batches, &expected.q1.row),
        "q2" => compare_count(batches, expected.q2.count),
        "q3" => compare_metric_groups(batches, &expected.q3.groups),
        "q4" => compare_bucket_groups(batches, &expected.q4.groups),
        "q5" => compare_row_count(batches, expected.q5.row_count),
        other => Err(format!("unknown query id {other}")),
    };
    outcome.map_err(|reason| FamilyRunError::Correctness {
        query_id: query_id.to_owned(),
        reason,
    })
}

/// Return whether two floats agree within the summation-order tolerance.
fn float_close(actual: f64, expected: f64) -> bool {
    (actual - expected).abs() <= SUM_RELATIVE_TOLERANCE * expected.abs().max(1.0)
}

/// Sum the row counts across every returned batch.
fn total_rows(batches: &[RecordBatch]) -> u64 {
    batches
        .iter()
        .map(|batch| u64::try_from(batch.num_rows()).unwrap_or(0))
        .sum()
}

/// Downcast one column to `Int64`, or describe the type mismatch.
fn int_col(batch: &RecordBatch, index: usize) -> Result<&Int64Array, String> {
    batch
        .column(index)
        .as_any()
        .downcast_ref::<Int64Array>()
        .ok_or_else(|| format!("column {index} is not Int64"))
}

/// Downcast one column to `Float64`, or describe the type mismatch.
fn float_col(batch: &RecordBatch, index: usize) -> Result<&Float64Array, String> {
    batch
        .column(index)
        .as_any()
        .downcast_ref::<Float64Array>()
        .ok_or_else(|| format!("column {index} is not Float64"))
}

/// Downcast one column to UTF-8, or describe the type mismatch.
fn str_col(batch: &RecordBatch, index: usize) -> Result<&StringArray, String> {
    batch
        .column(index)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| format!("column {index} is not Utf8"))
}

/// Compare the Q1 point result against the anchor-invariant expected row.
///
/// The re-anchored event time is deliberately excluded; only `row_id`,
/// `device_id`, `metric`, `value`, and `payload` are anchor-invariant.
fn compare_q1(
    batches: &[RecordBatch],
    expected: &super::bench_dataset::QualificationDatasetRow,
) -> Result<(), String> {
    let rows = total_rows(batches);
    if rows != 1 {
        return Err(format!("q1 expected exactly one row, got {rows}"));
    }
    let batch = batches
        .iter()
        .find(|batch| batch.num_rows() > 0)
        .ok_or("q1 returned no populated batch")?;
    let row_id = int_col(batch, 0)?.value(0);
    let device_id = int_col(batch, 1)?.value(0);
    let metric = str_col(batch, 2)?.value(0);
    let value = float_col(batch, 3)?.value(0);
    let payload = str_col(batch, 4)?.value(0);
    if row_id != expected.row_id {
        return Err(format!(
            "q1 row_id mismatch: got {row_id}, expected {}",
            expected.row_id
        ));
    }
    if device_id != expected.device_id {
        return Err(format!(
            "q1 device_id mismatch: got {device_id}, expected {}",
            expected.device_id
        ));
    }
    if metric != expected.metric {
        return Err(format!(
            "q1 metric mismatch: got {metric}, expected {}",
            expected.metric
        ));
    }
    if !float_close(value, expected.value) {
        return Err(format!(
            "q1 value mismatch: got {value}, expected {}",
            expected.value
        ));
    }
    if payload != expected.payload {
        return Err("q1 payload mismatch".to_owned());
    }
    Ok(())
}

/// Compare a single-row `COUNT(*)` result against the expected count.
fn compare_count(batches: &[RecordBatch], expected: u64) -> Result<(), String> {
    let batch = batches
        .iter()
        .find(|batch| batch.num_rows() > 0)
        .ok_or("count query returned no row")?;
    let count = u64::try_from(int_col(batch, 0)?.value(0)).map_err(|error| error.to_string())?;
    if count != expected {
        return Err(format!("count mismatch: got {count}, expected {expected}"));
    }
    Ok(())
}

/// Compare Q3's metric groups against the expected count/sum per metric.
fn compare_metric_groups(
    batches: &[RecordBatch],
    expected: &BTreeMap<String, QualificationAggregate>,
) -> Result<(), String> {
    let mut actual: BTreeMap<String, (u64, f64)> = BTreeMap::new();
    for batch in batches {
        let metric = str_col(batch, 0)?;
        let count = int_col(batch, 1)?;
        let sum = float_col(batch, 2)?;
        for row in 0..batch.num_rows() {
            let value = u64::try_from(count.value(row)).map_err(|error| error.to_string())?;
            actual.insert(metric.value(row).to_owned(), (value, sum.value(row)));
        }
    }
    if actual.len() != expected.len() {
        return Err(format!(
            "q3 group count mismatch: got {}, expected {}",
            actual.len(),
            expected.len()
        ));
    }
    for (key, aggregate) in expected {
        let (count, sum) = actual
            .get(key)
            .ok_or_else(|| format!("q3 missing group {key}"))?;
        if *count != aggregate.count {
            return Err(format!(
                "q3 count mismatch for {key}: got {count}, expected {}",
                aggregate.count
            ));
        }
        if !float_close(*sum, aggregate.sum) {
            return Err(format!("q3 sum mismatch for {key}"));
        }
    }
    Ok(())
}

/// Compare Q4's device-bucket groups against the expected count/sum per bucket.
fn compare_bucket_groups(
    batches: &[RecordBatch],
    expected: &BTreeMap<u64, QualificationAggregate>,
) -> Result<(), String> {
    let mut actual: BTreeMap<u64, (u64, f64)> = BTreeMap::new();
    for batch in batches {
        let bucket = int_col(batch, 0)?;
        let count = int_col(batch, 1)?;
        let sum = float_col(batch, 2)?;
        for row in 0..batch.num_rows() {
            let key = u64::try_from(bucket.value(row)).map_err(|error| error.to_string())?;
            let value = u64::try_from(count.value(row)).map_err(|error| error.to_string())?;
            actual.insert(key, (value, sum.value(row)));
        }
    }
    if actual.len() != expected.len() {
        return Err(format!(
            "q4 group count mismatch: got {}, expected {}",
            actual.len(),
            expected.len()
        ));
    }
    for (key, aggregate) in expected {
        let (count, sum) = actual
            .get(key)
            .ok_or_else(|| format!("q4 missing bucket {key}"))?;
        if *count != aggregate.count {
            return Err(format!(
                "q4 count mismatch for {key}: got {count}, expected {}",
                aggregate.count
            ));
        }
        if !float_close(*sum, aggregate.sum) {
            return Err(format!("q4 sum mismatch for {key}"));
        }
    }
    Ok(())
}

/// Compare Q5's returned row count against the expected range width.
fn compare_row_count(batches: &[RecordBatch], expected: u64) -> Result<(), String> {
    let rows = total_rows(batches);
    if rows != expected {
        return Err(format!(
            "q5 row count mismatch: got {rows}, expected {expected}"
        ));
    }
    Ok(())
}

/// Derive the saturation flag and recommendation from the measured ladder.
///
/// Implements the exact recommendation formula
/// [`BifrostCapacityReport`] validates: the recommendation is the highest
/// healthy rung whose controlling value is at or below 70% of the first
/// controlled-admission boundary. When no healthy rung qualifies, the boundary
/// marker is cleared so the report stays internally consistent with a
/// no-saturation outcome.
fn derive_saturation_outcome(
    stages: &mut [BifrostCapacityStage],
) -> (bool, Option<RecommendedStage>) {
    let Some(boundary_index) = stages
        .iter()
        .position(|stage| stage.saturation == Some(SaturationCause::ControlledAdmission))
    else {
        return (false, None);
    };
    let boundary_ordinal = stages[boundary_index].ordinal;
    let saturation_value = controlling_value(&stages[boundary_index].control);
    let expected = stages
        .iter()
        .filter(|stage| {
            stage.healthy
                && u128::from(controlling_value(&stage.control)) * 10
                    <= u128::from(saturation_value) * 7
        })
        .max_by_key(|stage| stage.ordinal)
        .map(|stage| (stage.ordinal, controlling_value(&stage.control)));
    match (saturation_value, expected) {
        (value, Some((ordinal, controlling))) if value > 0 => {
            let headroom_fraction = 1.0 - controlling as f64 / value as f64;
            (
                true,
                Some(RecommendedStage {
                    ordinal,
                    controlling_value: controlling,
                    headroom_fraction,
                    saturation_ordinal: boundary_ordinal,
                    saturation: SaturationCause::ControlledAdmission,
                }),
            )
        }
        _ => {
            for stage in stages.iter_mut() {
                stage.saturation = None;
            }
            (false, None)
        }
    }
}

/// Return the controlling ladder value carried by a stage control.
#[must_use]
fn controlling_value(control: &StageControl) -> u64 {
    match *control {
        StageControl::Qps { value, .. }
        | StageControl::Concurrency { value }
        | StageControl::Streams { value }
        | StageControl::LogicalBytesPerSec { value }
        | StageControl::ReturnedBytesPerSec { value }
        | StageControl::RequestsPerSec { value } => value,
    }
}

/// Return the `p`-percentile of latency samples in microseconds, if any.
///
/// Uses nearest-rank on a sorted copy; an empty sample set yields `None`, which
/// makes a stage nonhealthy exactly as the report predicate requires.
#[must_use]
fn percentile_us(samples: &[u64], p: f64) -> Option<u64> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let rank = ((p / 100.0) * (sorted.len() as f64 - 1.0)).round();
    let index = (rank as usize).min(sorted.len() - 1);
    Some(sorted[index])
}

/// Build the stable capture-window reference for one measured stage.
#[must_use]
fn capture_window_ref(
    invocation: &RunnerInvocation,
    workload_id: &str,
    ordinal: u16,
    started: SystemTime,
    ended: SystemTime,
) -> TelemetryWindowRef {
    let started_at_micros = unix_micros(started);
    let ended_at_micros = unix_micros(ended).max(started_at_micros.saturating_add(1));
    TelemetryWindowRef {
        window_id: format!("{}-{}-stage-{}", invocation.run_id, workload_id, ordinal),
        started_at_micros,
        ended_at_micros,
    }
}

/// Convert a wall-clock instant to Unix microseconds, saturating on error.
#[must_use]
fn unix_micros(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_micros()).ok())
        .unwrap_or(0)
}

/// Compute the single observation digest bound to a captured window.
///
/// Renders the window's counter/histogram deltas (or gauge finals when no
/// counters moved) into a sorted canonical form and hashes them, so the digest
/// is recomputable from the same telemetry and always a nonempty 64-hex value.
#[must_use]
fn observation_digest(delta: &BifrostTelemetryDelta) -> String {
    let samples = if delta.metrics.is_empty() {
        &delta.gauge_final
    } else {
        &delta.metrics
    };
    let mut rendered: Vec<String> = samples
        .iter()
        .map(|sample| {
            format!(
                "{}|{:?}|{}|{:?}",
                sample.family, sample.labels, sample.value, sample.kind
            )
        })
        .collect();
    rendered.sort();
    sha256_hex(rendered.join("\n").as_bytes())
}

/// Build the immutable report context shared by every stage in this invocation.
#[must_use]
fn report_context(
    invocation: &RunnerInvocation,
    resolved: ResolvedInvocation<'_>,
) -> CapacityReportContext {
    let _ = invocation;
    CapacityReportContext {
        source_commit: git_head(),
        tree_clean: git_tree_clean(),
        dataset_digest: dataset_manifest_digest(),
        workload_digest: workload_profiles_digest(),
        tier: resolved.tier.benchmark_tier(),
        topology: TopologyIdentity {
            topology_id: resolved.topology.topology_id.clone(),
            oracle_pods: resolved.topology.pods,
        },
        environment: environment_identity(),
    }
}

/// Probe the local environment identity recorded with every report.
///
/// CPU and memory probes are not implemented; their absence is explained by
/// `unavailable_reason` so the report stays honest about what was measured.
#[must_use]
pub(crate) fn environment_identity() -> EnvironmentIdentity {
    EnvironmentIdentity {
        cpu: None,
        memory_bytes: None,
        os: std::env::consts::OS.to_owned(),
        storage_mode: "local".to_owned(),
        unavailable_reason: Some("cpu and memory probes are not implemented".to_owned()),
    }
}

/// Return the current 40-hex source commit, or a non-hex sentinel on failure.
///
/// A non-hex sentinel makes report construction fail closed rather than emit an
/// artifact with an unverifiable provenance.
#[must_use]
pub(crate) fn git_head() -> String {
    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|revision| revision.trim().to_owned())
        .filter(|revision| !revision.is_empty())
        .unwrap_or_else(|| "unknown-worktree".to_owned())
}

/// Return whether the source tree has no tracked or untracked changes.
#[must_use]
pub(crate) fn git_tree_clean() -> bool {
    Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .map(|output| output.status.success() && output.stdout.is_empty())
        .unwrap_or(false)
}

/// Choose a driver in-flight cap from the offered rate when none is declared.
///
/// One second of offered operations bounds concurrency without shaping the
/// measured load; the value never appears in a report.
#[must_use]
fn default_in_flight(first_rung: Option<u64>) -> u64 {
    first_rung.unwrap_or(1).clamp(1, MAX_IN_FLIGHT_CAP)
}

/// Build the distributed stage control directly from the pod count (ruling D-A).
///
/// The distributed family never routes through
/// [`super::bench_runner::stage_control`]: its fixture ladder `[1, 2, 3, 6]` is
/// the D70 pod ladder, not an offer ladder, so at `pods` pods the runner holds
/// one query in flight per pod (a `Concurrency { value: pods }` closed-loop,
/// weak-scaling shape) rather than mapping the fixture control unit to an offer
/// law. Keeping this a distinct constructor makes the D-A invariant unit-testable
/// without a cluster.
#[must_use]
fn distributed_stage_control(pods: u16) -> StageControl {
    StageControl::Concurrency {
        value: u64::from(pods),
    }
}

/// Compute the weak-scaling efficiency vector `X(n)/(n·X(1))` for each pod rung.
///
/// The single place the distributed sweep derives its efficiency values, kept as
/// a pure helper so the formula is unit-testable against
/// [`DistributedBody::validate`] without a cluster. `oracle_pods` and
/// `throughput` are the per-rung pod counts and measured controlling throughputs
/// `X(n)` in ladder order; the baseline `X(1)` is the first rung's throughput. A
/// zero baseline yields non-finite ratios, which the report validator then
/// rejects, so a degenerate run fails closed rather than reporting a fabricated
/// efficiency. The returned vector has the same length and order as the inputs.
#[must_use]
fn distributed_efficiency(oracle_pods: &[u16], throughput: &[u64]) -> Vec<f64> {
    let baseline = throughput.first().copied().unwrap_or_default() as f64;
    oracle_pods
        .iter()
        .zip(throughput)
        .map(|(pods, rung_throughput)| *rung_throughput as f64 / (f64::from(*pods) * baseline))
        .collect()
}

/// Run the distributed family: weak-scaling reads under one query per pod.
///
/// Ruling D-A/D-B: the smoke tier is a plumbing proof that boots the CLI
/// `--topology`, materializes the smoke dataset, proves the fixture query's
/// correctness once, and offers a closed-loop hold of one query in flight per pod
/// for the single planned stage, emitting a single-stage [`QueryBody`]. The
/// qualification tier runs the full `[1, 2, 3, 6]` pod ladder over the run's
/// shared resources (one cluster per rung, no early break), measures the
/// controlling throughput `X(n)` at each rung, and emits the four-pod
/// `DistributedBody` weak-scaling sweep (`efficiency(n) = X(n)/(n·X(1))`); see
/// [`run_distributed_qualification_sweep`].
///
/// # Errors
/// Returns [`FamilyRunError`] for an absent `query_id`, a cluster/capture/
/// correctness failure, a rejected report, a budget breach, or a disk-write
/// failure.
pub async fn run_distributed(
    lane_start: Instant,
    invocation: &RunnerInvocation,
    resolved: ResolvedInvocation<'_>,
    budgets: &TierBudgets,
    resources: Option<&SharedRunResources>,
) -> Result<std::path::PathBuf, FamilyRunError> {
    let workload_id = resolved.workload.workload_id.clone();
    let query_id = resolved.workload.query_id.clone().ok_or_else(|| {
        FamilyRunError::Cluster(format!(
            "distributed workload {workload_id} declares no query_id"
        ))
    })?;
    if let RunnerTier::Qualification = resolved.tier {
        return run_distributed_qualification_sweep(
            invocation,
            resolved,
            &workload_id,
            &query_id,
            resources,
        )
        .await;
    }
    let topology = topology_for_pods(resolved.topology.pods)?;
    let plan = plan_stages(resolved.workload, resolved.tier)?;

    let cluster = start_family_cluster(topology.spec(), resources).await?;

    // A multi-pod distributed query dispatches sealed fragments to peer Oracles,
    // whose eligibility and fencing tokens are read from each pod's frozen
    // membership cut (`Oracle::prepare_sealed_dispatch` over `cluster.snapshot()`).
    // Converge every node's cut before the first cross-pod query, mirroring the
    // real multi-pod journeys (`oracle_edge_journeys`, `oracle_peer`) which refresh
    // membership before querying; the distributed runner is the only bench family
    // that boots more than one pod. Membership is stable for the remainder of the
    // run, so a single refresh suffices, and it pins node eligibility only (never
    // the per-query file set), so it is safe before any data is materialized.
    // Note: convergence alone does not guarantee cross-pod execution on an
    // under-provisioned harness — a peer must also hold enough running slots for
    // the query's slot demand (Analytical demands two); that provisioning is a
    // qualification-tier/cluster concern outside this runner.
    cluster
        .refresh_oracle_snapshots()
        .await
        .map_err(|error| FamilyRunError::Cluster(error.to_string()))?;

    let result = run_distributed_smoke_stages(
        &cluster,
        invocation,
        resolved,
        &plan,
        &workload_id,
        &query_id,
    )
    .await;

    // Always attempt shutdown; a shutdown failure only overrides a prior success.
    let shutdown = cluster
        .shutdown_and_inspect()
        .await
        .map_err(|error| FamilyRunError::Cluster(error.to_string()));
    let (stages, saturation_reached, recommended) = result?;
    shutdown?;

    if let RunnerTier::Smoke = resolved.tier {
        enforce_budget(lane_start.elapsed(), budgets.smoke_lane_seconds)?;
    }

    let context = report_context(invocation, resolved);
    let body = QueryBody {
        query_id,
        saturation_reached,
        recommended,
    };
    let report = BifrostCapacityReport::new(context, stages, body)
        .map_err(|error| FamilyRunError::Report(error.to_string()))?;
    report
        .write_report(&invocation.output_root, &invocation.run_id, &workload_id)
        .map_err(|error| FamilyRunError::Write(error.to_string()))
}

/// Run the distributed weak-scaling sweep across the full `[1, 2, 3, 6]` ladder.
///
/// The qualification-tier body of [`run_distributed`]. For each pod rung on the
/// D70 ladder it boots one cluster over the run's shared resources (so every rung
/// observes the once-materialized qualification dataset), converges peer-Oracle
/// membership, proves the fixture query's correctness once, and measures one
/// closed-loop stage holding one query in flight per pod
/// ([`distributed_stage_control`] from the rung's pod count). It never breaks
/// early on saturation: the `DistributedBody` requires all four rungs so the
/// weak-scaling efficiency `X(n)/(n·X(1))` is defined at every pod count. The
/// controlling throughput `X(n)` at a rung is that stage's completed-operation
/// count over the identical measured window, so the per-rung efficiency ratio is
/// window-duration invariant. Each rung's cluster is shut down before the next
/// starts; the shared resources outlive the sweep and are torn down only by the
/// run.
///
/// # Errors
/// Returns [`FamilyRunError`] for a stage plan whose ladder is not the locked
/// `[1, 2, 3, 6]`, a cluster/capture/correctness failure, a rejected report
/// (including a zero baseline throughput), or a disk-write failure.
async fn run_distributed_qualification_sweep(
    invocation: &RunnerInvocation,
    resolved: ResolvedInvocation<'_>,
    workload_id: &str,
    query_id: &str,
    resources: Option<&SharedRunResources>,
) -> Result<std::path::PathBuf, FamilyRunError> {
    let plan = plan_stages(resolved.workload, resolved.tier)?;
    let mut stages = Vec::with_capacity(plan.len());
    let mut throughput = Vec::with_capacity(plan.len());
    let mut oracle_pods = Vec::with_capacity(plan.len());
    for step in &plan {
        let pods = u16::try_from(step.control_value).map_err(|_| {
            FamilyRunError::Cluster(format!(
                "distributed rung control value {} exceeds the pod-count range",
                step.control_value
            ))
        })?;
        let topology = topology_for_pods(pods)?;
        let cluster = start_family_cluster(topology.spec(), resources).await?;

        // Converge every pod's frozen membership cut before the first cross-pod
        // query, exactly as the smoke path does; membership is stable for the
        // rung and pins node eligibility only, never the per-query file set.
        let converged = cluster
            .refresh_oracle_snapshots()
            .await
            .map_err(|error| FamilyRunError::Cluster(error.to_string()));
        let measured = match converged {
            Ok(()) => {
                measure_distributed_rung(
                    &cluster,
                    invocation,
                    resolved,
                    step,
                    workload_id,
                    query_id,
                )
                .await
            }
            Err(error) => Err(error),
        };

        // Always attempt shutdown; a shutdown failure only overrides a success.
        let shutdown = cluster
            .shutdown_and_inspect()
            .await
            .map_err(|error| FamilyRunError::Cluster(error.to_string()));
        let stage = measured?;
        shutdown?;

        throughput.push(stage.completed.operations);
        oracle_pods.push(pods);
        stages.push(stage);
    }

    let efficiency = distributed_efficiency(&oracle_pods, &throughput);
    let (saturation_reached, recommended) = derive_saturation_outcome(&mut stages);

    let context = report_context(invocation, resolved);
    let body = DistributedBody {
        query_id: query_id.to_owned(),
        oracle_pods,
        throughput,
        efficiency,
        saturation_reached,
        recommended,
    };
    let report = BifrostCapacityReport::new(context, stages, body)
        .map_err(|error| FamilyRunError::Report(error.to_string()))?;
    report
        .write_report(&invocation.output_root, &invocation.run_id, workload_id)
        .map_err(|error| FamilyRunError::Write(error.to_string()))
}

/// Acquire the shared dataset, prove correctness, and measure one distributed rung.
///
/// The per-rung body of [`run_distributed_qualification_sweep`]. It acquires the
/// run's once-materialized qualification dataset (visible over the shared
/// resources this rung's cluster booted from), proves the fixture query's
/// correctness once against the deterministic expected results, then measures one
/// closed-loop stage holding one query in flight per pod
/// ([`distributed_stage_control`] from the rung's pod count). The stage ordinal is
/// the rung's plan ordinal so the emitted report's stage indices stay contiguous.
///
/// # Errors
/// Returns [`FamilyRunError`] for a dataset-acquisition, client, correctness, or
/// capture failure encountered while measuring the rung.
async fn measure_distributed_rung(
    cluster: &WyrdTestCluster,
    invocation: &RunnerInvocation,
    resolved: ResolvedInvocation<'_>,
    step: &StagePlan,
    workload_id: &str,
    query_id: &str,
) -> Result<BifrostCapacityStage, FamilyRunError> {
    let dataset = acquire_query_dataset(cluster, invocation, resolved, workload_id).await?;
    let queries = dataset.queries();
    let expected = dataset.expected_results();
    let shape = dataset.shape();
    let sql = query_sql(query_id, &queries, shape)?;
    let scan_logical_bytes =
        scan_range_rows(query_id, &queries, shape)?.saturating_mul(QUALIFICATION_LOGICAL_ROW_BYTES);

    let clients = reference_clients(cluster, std::slice::from_ref(&cluster.data_tenant_id()))
        .await
        .map_err(|error| FamilyRunError::Cluster(error.to_string()))?;
    let query = clients
        .first()
        .ok_or_else(|| {
            FamilyRunError::Cluster("distributed runner has no reference client".to_owned())
        })?
        .query
        .clone();

    probe_query_correctness(&query, query_id, &sql, &expected).await?;

    let pods = u16::try_from(step.control_value).unwrap_or(u16::MAX);
    let control = distributed_stage_control(pods);
    measure_query_stage(
        cluster,
        &query,
        invocation,
        workload_id,
        control,
        step,
        &sql,
        scan_logical_bytes,
    )
    .await
}

/// Materialize, prove correctness, and run the single distributed smoke stage.
///
/// Factored from [`run_distributed`] so cluster shutdown is guaranteed on both
/// paths. The stage control is built by [`distributed_stage_control`] from the
/// booted topology's pod count (ruling D-A), never from the fixture control unit.
///
/// # Errors
/// Returns [`FamilyRunError`] for a dataset-acquisition, client, correctness,
/// capture, or unexpected-failure condition, or an empty smoke plan.
async fn run_distributed_smoke_stages(
    cluster: &WyrdTestCluster,
    invocation: &RunnerInvocation,
    resolved: ResolvedInvocation<'_>,
    plan: &[StagePlan],
    workload_id: &str,
    query_id: &str,
) -> Result<(Vec<BifrostCapacityStage>, bool, Option<RecommendedStage>), FamilyRunError> {
    let dataset = materialize_smoke_dataset(cluster, invocation, workload_id).await?;
    let queries = dataset.queries();
    let expected = dataset.expected_results();
    let shape = dataset.shape();
    let sql = query_sql(query_id, &queries, shape)?;
    let scan_logical_bytes =
        scan_range_rows(query_id, &queries, shape)?.saturating_mul(QUALIFICATION_LOGICAL_ROW_BYTES);

    let clients = reference_clients(cluster, std::slice::from_ref(&cluster.data_tenant_id()))
        .await
        .map_err(|error| FamilyRunError::Cluster(error.to_string()))?;
    let query = clients
        .first()
        .ok_or_else(|| {
            FamilyRunError::Cluster("distributed runner has no reference client".to_owned())
        })?
        .query
        .clone();

    probe_query_correctness(&query, query_id, &sql, &expected).await?;

    let step = plan.first().ok_or_else(|| {
        FamilyRunError::Cluster("distributed smoke plan produced no stage".to_owned())
    })?;
    let control = distributed_stage_control(resolved.topology.pods);
    let stage = measure_query_stage(
        cluster,
        &query,
        invocation,
        workload_id,
        control,
        step,
        &sql,
        scan_logical_bytes,
    )
    .await?;

    let mut stages = vec![stage];
    let (saturation_reached, recommended) = derive_saturation_outcome(&mut stages);
    Ok((stages, saturation_reached, recommended))
}

/// Run the tenant-matrix fairness family under a total-volume-constant offer.
///
/// Ruling D-C: the fixture `query_id` is null, so the runner fixes the `q1`
/// per-tenant point lookup; the offer holds the concurrency rung in flight
/// distributed round-robin across the matrix's tenants (equal budget share), and
/// every stage records the budget-normalized Jain index over per-tenant
/// completions. Because budget shares are equal, budget-normalization is an
/// identity for the scale-invariant Jain index, so the raw per-tenant completion
/// counts are the budget-normalized index. The smoke tier runs the single planned
/// rung and emits a single-stage [`QueryBody`]. Fairness is a smoke-tier-only
/// family (D69): the qualification family set is ingest, oracle, distributed, and
/// mixed, so a qualification-tier fairness invocation fails closed here.
///
/// # Errors
/// Returns [`FamilyRunError`] for an absent tenant matrix, a cluster/provisioning/
/// capture/correctness failure, a rejected report, a budget breach, a disk-write
/// failure, or the qualification-tier refusal (fairness is smoke-tier only).
pub async fn run_fairness(
    lane_start: Instant,
    invocation: &RunnerInvocation,
    resolved: ResolvedInvocation<'_>,
    budgets: &TierBudgets,
    resources: Option<&SharedRunResources>,
) -> Result<std::path::PathBuf, FamilyRunError> {
    let workload_id = resolved.workload.workload_id.clone();
    let matrix = resolved.workload.tenant_matrix.clone().ok_or_else(|| {
        FamilyRunError::Cluster(format!(
            "fairness workload {workload_id} declares no tenant_matrix"
        ))
    })?;
    match resolved.tier {
        RunnerTier::Smoke => {}
        RunnerTier::Qualification => {
            return Err(FamilyRunError::Cluster(
                "fairness is a smoke-tier-only family (D69); the qualification family set is \
                 ingest, oracle, distributed, and mixed, so fairness has no qualification sweep"
                    .to_owned(),
            ));
        }
    }
    let query_id = "q1";
    let topology = topology_for_pods(resolved.topology.pods)?;
    let plan = plan_stages(resolved.workload, resolved.tier)?;

    let cluster = start_family_cluster(topology.spec(), resources).await?;

    let tenant_count = usize::try_from(matrix.tenants).unwrap_or(1).max(1);
    let result = run_fairness_stages(
        &cluster,
        invocation,
        resolved,
        &plan,
        &workload_id,
        query_id,
        tenant_count,
    )
    .await;

    // Always attempt shutdown; a shutdown failure only overrides a prior success.
    let shutdown = cluster
        .shutdown_and_inspect()
        .await
        .map_err(|error| FamilyRunError::Cluster(error.to_string()));
    let (stages, saturation_reached, recommended) = result?;
    shutdown?;

    if let RunnerTier::Smoke = resolved.tier {
        enforce_budget(lane_start.elapsed(), budgets.smoke_lane_seconds)?;
    }

    let context = report_context(invocation, resolved);
    let body = QueryBody {
        query_id: query_id.to_owned(),
        saturation_reached,
        recommended,
    };
    let report = BifrostCapacityReport::new(context, stages, body)
        .map_err(|error| FamilyRunError::Report(error.to_string()))?;
    report
        .write_report(&invocation.output_root, &invocation.run_id, &workload_id)
        .map_err(|error| FamilyRunError::Write(error.to_string()))
}

/// Provision the tenant matrix and run every planned fairness stage.
///
/// Factored from [`run_fairness`] so cluster shutdown is guaranteed on both
/// paths. Materializes the smoke dataset into all `tenant_count` tenants, proves
/// correctness once on the first tenant, and offers each rung round-robin across
/// their clients.
///
/// # Errors
/// Returns [`FamilyRunError`] for a provisioning, materialization, client,
/// correctness, capture, or unexpected-failure condition.
// justification: the qualification harness fixes this stage's inputs by contract; a single-use request wrapper would hide which of them the fairness stage sequence actually reads without removing one of them.
#[allow(clippy::too_many_arguments)]
async fn run_fairness_stages(
    cluster: &WyrdTestCluster,
    invocation: &RunnerInvocation,
    resolved: ResolvedInvocation<'_>,
    plan: &[StagePlan],
    workload_id: &str,
    query_id: &str,
    tenant_count: usize,
) -> Result<(Vec<BifrostCapacityStage>, bool, Option<RecommendedStage>), FamilyRunError> {
    let tenants = provision_reference_tenants(cluster, tenant_count)
        .await
        .map_err(|error| FamilyRunError::Cluster(error.to_string()))?;
    let dataset =
        materialize_smoke_dataset_into(cluster, invocation, workload_id, &tenants).await?;
    let queries = dataset.queries();
    let expected = dataset.expected_results();
    let shape = dataset.shape();
    let sql = query_sql(query_id, &queries, shape)?;
    let scan_logical_bytes =
        scan_range_rows(query_id, &queries, shape)?.saturating_mul(QUALIFICATION_LOGICAL_ROW_BYTES);

    let clients = reference_clients(cluster, &tenants)
        .await
        .map_err(|error| FamilyRunError::Cluster(error.to_string()))?;
    let query_clients: Vec<QueryClient> =
        clients.iter().map(|client| client.query.clone()).collect();
    let first = query_clients
        .first()
        .ok_or_else(|| {
            FamilyRunError::Cluster("fairness runner has no reference client".to_owned())
        })?
        .clone();
    probe_query_correctness(&first, query_id, &sql, &expected).await?;

    let mut stages = Vec::with_capacity(plan.len());
    for step in plan {
        let control = stage_control(&resolved.workload.control, step.control_value)?;
        let stage = measure_fairness_stage(
            cluster,
            &query_clients,
            invocation,
            workload_id,
            control,
            step,
            &sql,
            scan_logical_bytes,
        )
        .await?;
        let saturated = stage.saturation == Some(SaturationCause::ControlledAdmission);
        stages.push(stage);
        if saturated {
            break;
        }
    }

    let (saturation_reached, recommended) = derive_saturation_outcome(&mut stages);
    Ok((stages, saturation_reached, recommended))
}

/// Offer one fairness stage's measured window and assemble its causal stage.
///
/// Mirrors [`measure_query_stage`] but drives the multi-tenant round-robin offer
/// and stamps the budget-normalized Jain index onto the assembled stage's
/// `fairness` field (ruling D-C).
///
/// # Errors
/// Returns [`FamilyRunError`] for a capture failure or unexpected read failures
/// observed during the measured window.
// justification: the qualification harness fixes this stage's inputs by contract; a single-use request wrapper would hide which of them the measured fairness window actually reads without removing one of them.
#[allow(clippy::too_many_arguments)]
async fn measure_fairness_stage(
    cluster: &WyrdTestCluster,
    clients: &[QueryClient],
    invocation: &RunnerInvocation,
    workload_id: &str,
    control: StageControl,
    step: &StagePlan,
    sql: &str,
    scan_logical_bytes_per_query: u64,
) -> Result<BifrostCapacityStage, FamilyRunError> {
    let workers = controlling_value(&control);
    if step.warmup_seconds > 0 {
        offer_fairness_window(clients, sql, workers, step.warmup_seconds).await;
    }

    let wall_start = SystemTime::now();
    let ((window, per_tenant), delta) = run_sampled_window(cluster.telemetry(), || async {
        Ok(offer_fairness_window(clients, sql, workers, step.measure_seconds).await)
    })
    .await
    .map_err(|error| FamilyRunError::Cluster(error.to_string()))?;
    let wall_end = SystemTime::now();

    if step.drain_seconds > 0 {
        offer_fairness_window(clients, sql, workers, step.drain_seconds).await;
    }

    if window.failed > 0 {
        return Err(FamilyRunError::UnexpectedFailures {
            family: workload_id.to_owned(),
            ordinal: step.ordinal,
            failed: window.failed,
        });
    }

    let execution = StageExecution {
        run_id: invocation.run_id.clone(),
        ordinal: step.ordinal,
        capture_window: capture_window_ref(
            invocation,
            workload_id,
            step.ordinal,
            wall_start,
            wall_end,
        ),
        observation_digests: vec![observation_digest(&delta)],
    };

    let mut stage = build_query_stage(
        workload_id,
        control,
        step.ordinal,
        &window,
        scan_logical_bytes_per_query,
        &delta,
        execution,
    );
    // Budget-normalized Jain (ruling D-C). Equal per-tenant budget shares make the
    // normalization an identity for the scale-invariant Jain index, so the raw
    // per-tenant completion counts are the budget-normalized index.
    stage.fairness = Some(jain_fairness(&per_tenant));
    Ok(stage)
}

/// Hold `workers` reads in flight round-robin across `clients` for `seconds`.
///
/// Each worker cycles through the tenant clients so the total in-flight offer is
/// held constant across matrix sizes (total-volume-constant), and each tenant is
/// served in equal turn. Returns the aggregate window plus the per-tenant
/// completion vector the Jain index is computed over.
async fn offer_fairness_window(
    clients: &[QueryClient],
    sql: &str,
    workers: u64,
    seconds: u32,
) -> (QueryWindow, Vec<u64>) {
    let mut window = QueryWindow::default();
    let tenant_count = clients.len();
    let mut per_tenant = vec![0_u64; tenant_count];
    let workers = workers.min(MAX_IN_FLIGHT_CAP);
    if workers == 0 || seconds == 0 || tenant_count == 0 {
        return (window, per_tenant);
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(u64::from(seconds));
    let mut tasks = JoinSet::new();
    for worker in 0..workers {
        let clients: Vec<QueryClient> = clients.to_vec();
        let sql = sql.to_owned();
        let start = usize::try_from(worker).unwrap_or(0) % tenant_count;
        tasks.spawn(async move {
            let mut outcomes: Vec<(usize, QueryOutcome)> = Vec::new();
            let mut cursor = start;
            while tokio::time::Instant::now() < deadline {
                let index = cursor % clients.len();
                outcomes.push((
                    index,
                    execute_one_query(clients[index].clone(), sql.clone()).await,
                ));
                cursor = cursor.wrapping_add(1);
            }
            outcomes
        });
    }
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok(outcomes) => {
                for (index, outcome) in outcomes {
                    if matches!(outcome, QueryOutcome::Completed { .. }) {
                        per_tenant[index] = per_tenant[index].saturating_add(1);
                    }
                    record_query_outcome(&mut window, outcome);
                }
            }
            Err(_) => window.failed = window.failed.saturating_add(1),
        }
    }
    window.offered = window
        .completed
        .saturating_add(window.rejected)
        .saturating_add(window.failed);
    (window, per_tenant)
}

/// Run the mixed Scribe/Oracle/Forge family along one diagonal fraction point.
///
/// Ruling D-D: the runner measures the standalone Scribe and Oracle healthy rates
/// within this invocation, then offers each stream concurrently at
/// `(percent/100) × standalone_healthy_rate`. The stage `controlling_value` stays
/// the fixture percent (`RequestsPerSec { value: percent }`); the standalone
/// anchor is recoverable losslessly from the recorded offered count, the control
/// percent, and the window duration, so no anchor field is added. Forge debt of
/// `forge_debt_generations_per_tenant` is seeded before measurement and drained to
/// convergence before the measured window opens (D71). Each mixed workload runs
/// its single planned diagonal rung and emits a single-stage [`MixedBody`]; the
/// diagonal sweep is expressed as three separate single-rung workloads
/// (`mixed-25-25`, `mixed-50-50`, `mixed-75-75`), not a multi-rung ladder within
/// one invocation. At smoke tier the runner materializes the fixture smoke dataset
/// into a fresh standalone cluster; at qualification tier it boots over the run's
/// shared resources and reads the once-materialized qualification dataset.
///
/// # Errors
/// Returns [`FamilyRunError`] for an absent `mixed_fractions`/debt parameter, a
/// cluster/provisioning/capture/correctness failure, an unexpected failure, a
/// rejected report, a budget breach, or a disk-write failure.
pub async fn run_mixed(
    lane_start: Instant,
    invocation: &RunnerInvocation,
    resolved: ResolvedInvocation<'_>,
    budgets: &TierBudgets,
    resources: Option<&SharedRunResources>,
) -> Result<std::path::PathBuf, FamilyRunError> {
    let workload_id = resolved.workload.workload_id.clone();
    let fractions = resolved.workload.mixed_fractions.clone().ok_or_else(|| {
        FamilyRunError::Cluster(format!(
            "mixed workload {workload_id} declares no mixed_fractions"
        ))
    })?;
    let debt_generations = resolved
        .workload
        .forge_debt_generations_per_tenant
        .ok_or_else(|| {
            FamilyRunError::Cluster(format!(
                "mixed workload {workload_id} declares no forge_debt_generations_per_tenant"
            ))
        })?;
    let topology = topology_for_pods(resolved.topology.pods)?;
    let plan = plan_stages(resolved.workload, resolved.tier)?;
    // Smoke boots a private cluster, so the smoke-default reference table never
    // collides. Qualification shares one catalog across every family cluster in
    // the run, so each mixed workload writes to a fresh table keyed by both the
    // run and the workload id (three mixed workloads share the run's resources).
    let table = match resolved.tier {
        RunnerTier::Smoke => REFERENCE_TABLE.to_owned(),
        RunnerTier::Qualification => {
            fresh_ingest_table(&format!("{}-{}", invocation.run_id, workload_id))
        }
    };

    let cluster = start_family_cluster(topology.spec(), resources).await?;

    let result = run_mixed_stage(
        &cluster,
        invocation,
        resolved,
        &plan,
        &workload_id,
        &fractions,
        debt_generations,
        &table,
    )
    .await;

    // Always attempt shutdown; a shutdown failure only overrides a prior success.
    let shutdown = cluster
        .shutdown_and_inspect()
        .await
        .map_err(|error| FamilyRunError::Cluster(error.to_string()));
    let (stages, saturation_reached, recommended) = result?;
    shutdown?;

    if let RunnerTier::Smoke = resolved.tier {
        enforce_budget(lane_start.elapsed(), budgets.smoke_lane_seconds)?;
    }

    let context = report_context(invocation, resolved);
    let body = MixedBody {
        fractions: vec![(fractions.scribe_percent, fractions.oracle_percent)],
        forge_debt_generations_per_tenant: debt_generations,
        saturation_reached,
        recommended,
    };
    let report = BifrostCapacityReport::new(context, stages, body)
        .map_err(|error| FamilyRunError::Report(error.to_string()))?;
    report
        .write_report(&invocation.output_root, &invocation.run_id, &workload_id)
        .map_err(|error| FamilyRunError::Write(error.to_string()))
}

/// Seed Forge debt, measure standalone anchors, and run the mixed diagonal stage.
///
/// Factored from [`run_mixed`] so cluster shutdown is guaranteed on both paths.
/// Every write phase draws a globally monotonic ordinal so no batch is deduplicated
/// against an earlier phase, keeping the anchor and diagonal measurements honest.
/// The Oracle read dataset is acquired tier-appropriately through
/// [`acquire_query_dataset`]: the smoke tier materializes the fixture smoke shape
/// into this cluster, the qualification tier reuses the run's once-materialized
/// dataset visible over the shared resources. All Scribe writes target `table`,
/// the run's mixed write target (the smoke reference table, or a fresh per-run,
/// per-workload table at qualification).
///
/// # Errors
/// Returns [`FamilyRunError`] for a provisioning, dataset-acquisition, client,
/// correctness, capture, or unexpected-failure condition, or an empty plan.
// justification: the qualification harness fixes this stage's inputs by contract; a single-use request wrapper would hide which of them the mixed stage actually reads without removing one of them.
#[allow(clippy::too_many_arguments)]
async fn run_mixed_stage(
    cluster: &WyrdTestCluster,
    invocation: &RunnerInvocation,
    resolved: ResolvedInvocation<'_>,
    plan: &[StagePlan],
    workload_id: &str,
    fractions: &super::bench_dataset::MixedFractions,
    debt_generations: u32,
    table: &str,
) -> Result<(Vec<BifrostCapacityStage>, bool, Option<RecommendedStage>), FamilyRunError> {
    let step = plan
        .first()
        .ok_or_else(|| FamilyRunError::Cluster("mixed plan produced no stage".to_owned()))?;
    let data_tenant = cluster.data_tenant_id();
    let tenant_slice = std::slice::from_ref(&data_tenant);

    // Scribe write path plus Oracle read dataset on the same tenant.
    provision_named_tables(cluster, tenant_slice, &[table.to_owned()])
        .await
        .map_err(|error| FamilyRunError::Cluster(error.to_string()))?;
    let dataset = acquire_query_dataset(cluster, invocation, resolved, workload_id).await?;
    let queries = dataset.queries();
    let expected = dataset.expected_results();
    let shape = dataset.shape();
    let query_id = "q1";
    let sql = query_sql(query_id, &queries, shape)?;
    let scan_logical_bytes =
        scan_range_rows(query_id, &queries, shape)?.saturating_mul(QUALIFICATION_LOGICAL_ROW_BYTES);

    let clients = reference_clients(cluster, tenant_slice)
        .await
        .map_err(|error| FamilyRunError::Cluster(error.to_string()))?;
    let client = clients
        .first()
        .ok_or_else(|| FamilyRunError::Cluster("mixed runner has no reference client".to_owned()))?
        .clone();
    let query = client.query.clone();
    probe_query_correctness(&query, query_id, &sql, &expected).await?;

    let driver_in_flight = u64::from(resolved.topology.pods).max(1);
    let ordinals = Arc::new(AtomicU64::new(0));

    // Pre-measurement Forge debt, released to convergence before the window opens.
    build_forge_debt(&client, debt_generations, &ordinals, table).await?;
    flush_tenant_writers(cluster, tenant_slice)
        .await
        .map_err(|error| FamilyRunError::Cluster(error.to_string()))?;
    await_forge_convergence(cluster, tenant_slice)
        .await
        .map_err(|error| FamilyRunError::Cluster(error.to_string()))?;

    // Standalone healthy-rate anchors measured in the same invocation (ruling D-D).
    let window_seconds = u64::from(step.measure_seconds.max(1));
    let scribe_window = offer_writes_saturating(
        &client,
        driver_in_flight,
        step.measure_seconds,
        &ordinals,
        table,
    )
    .await;
    flush_tenant_writers(cluster, tenant_slice)
        .await
        .map_err(|error| FamilyRunError::Cluster(error.to_string()))?;
    let scribe_anchor = scribe_window.admitted / window_seconds;
    let oracle_window =
        offer_closed_loop(&query, &sql, driver_in_flight, step.measure_seconds).await;
    let oracle_anchor = oracle_window.completed / window_seconds;

    // Diagonal offer at percent% of each standalone healthy rate.
    let scribe_rate = scribe_anchor.saturating_mul(u64::from(fractions.scribe_percent)) / 100;
    let oracle_rate = oracle_anchor.saturating_mul(u64::from(fractions.oracle_percent)) / 100;
    let control = stage_control(&resolved.workload.control, step.control_value)?;
    let query_control = StageControl::RequestsPerSec { value: oracle_rate };

    let wall_start = SystemTime::now();
    let ((write_window, query_window), delta) = run_sampled_window(cluster.telemetry(), || async {
        let (writes, reads) = tokio::join!(
            offer_writes_paced(
                &client,
                scribe_rate,
                step.measure_seconds,
                driver_in_flight,
                &ordinals,
                table,
            ),
            offer_queries(&query, &query_control, &sql, step.measure_seconds),
        );
        Ok((writes, reads))
    })
    .await
    .map_err(|error| FamilyRunError::Cluster(error.to_string()))?;
    let wall_end = SystemTime::now();
    flush_tenant_writers(cluster, tenant_slice)
        .await
        .map_err(|error| FamilyRunError::Cluster(error.to_string()))?;

    let failed = write_window.failed.saturating_add(query_window.failed);
    if failed > 0 {
        return Err(FamilyRunError::UnexpectedFailures {
            family: workload_id.to_owned(),
            ordinal: step.ordinal,
            failed,
        });
    }

    // Fold both streams into one combined window so the stage's public counts and
    // health/saturation recompute under the shared report validator; the write
    // stream contributes its admitted count and the read stream its completed
    // count, and the union of latencies drives the stage percentile.
    let mut latencies = write_window.latencies_us;
    latencies.extend(query_window.latencies_us);
    let combined = QueryWindow {
        offered: write_window.offered.saturating_add(query_window.offered),
        completed: write_window.admitted.saturating_add(query_window.completed),
        rejected: write_window.rejected.saturating_add(query_window.rejected),
        failed: 0,
        rows_returned: query_window.rows_returned,
        bytes_returned: query_window.bytes_returned,
        latencies_us: latencies,
    };
    let execution = StageExecution {
        run_id: invocation.run_id.clone(),
        ordinal: step.ordinal,
        capture_window: capture_window_ref(
            invocation,
            workload_id,
            step.ordinal,
            wall_start,
            wall_end,
        ),
        observation_digests: vec![observation_digest(&delta)],
    };
    let mut stages = vec![build_query_stage(
        workload_id,
        control,
        step.ordinal,
        &combined,
        scan_logical_bytes,
        &delta,
        execution,
    )];
    let (saturation_reached, recommended) = derive_saturation_outcome(&mut stages);
    Ok((stages, saturation_reached, recommended))
}

/// Seed `generations` pre-measurement Forge-debt batches into `table`.
///
/// Each batch draws a monotonic ordinal so it is a real, non-deduplicated write.
/// Controlled backpressure is tolerated (the debt is being applied under load);
/// only an unexpected write failure aborts the seeding. `table` is the mixed
/// family's Scribe write target for the run.
///
/// # Errors
/// Returns [`FamilyRunError::Cluster`] when a debt-seeding write fails
/// unexpectedly.
async fn build_forge_debt(
    client: &ReferenceClient,
    generations: u32,
    ordinals: &Arc<AtomicU64>,
    table: &str,
) -> Result<(), FamilyRunError> {
    for _ in 0..generations {
        let ordinal = ordinals.fetch_add(1, Ordering::Relaxed);
        match send_one_write(client.clone(), ordinal, table.to_owned()).await {
            WriteOutcome::Admitted(_) | WriteOutcome::Rejected => {}
            WriteOutcome::Failed => {
                return Err(FamilyRunError::Cluster(
                    "mixed Forge-debt seeding write failed unexpectedly".to_owned(),
                ));
            }
        }
    }
    Ok(())
}

/// Fold one classified write outcome into the measured window.
fn fold_write_outcome(window: &mut WriteWindow, outcome: WriteOutcome) {
    match outcome {
        WriteOutcome::Admitted(latency_us) => {
            window.admitted = window.admitted.saturating_add(1);
            window.latencies_us.push(latency_us);
        }
        WriteOutcome::Rejected => window.rejected = window.rejected.saturating_add(1),
        WriteOutcome::Failed => window.failed = window.failed.saturating_add(1),
    }
}

/// Hold `workers` writes in flight for `seconds`, measuring saturated throughput.
///
/// Used to measure a standalone Scribe healthy rate: each worker loops issuing
/// monotonic-ordinal writes until the deadline, so `admitted` over the window is
/// the achieved standalone write rate. `offered` is the total dispatched. Every
/// write targets `table`, the mixed family's Scribe write target for the run.
async fn offer_writes_saturating(
    client: &ReferenceClient,
    workers: u64,
    seconds: u32,
    ordinals: &Arc<AtomicU64>,
    table: &str,
) -> WriteWindow {
    let mut window = WriteWindow::default();
    let workers = workers.min(MAX_IN_FLIGHT_CAP);
    if workers == 0 || seconds == 0 {
        return window;
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(u64::from(seconds));
    let mut tasks = JoinSet::new();
    for _ in 0..workers {
        let client = client.clone();
        let ordinals = Arc::clone(ordinals);
        let table = table.to_owned();
        tasks.spawn(async move {
            let mut outcomes = Vec::new();
            while tokio::time::Instant::now() < deadline {
                let ordinal = ordinals.fetch_add(1, Ordering::Relaxed);
                outcomes.push(send_one_write(client.clone(), ordinal, table.clone()).await);
            }
            outcomes
        });
    }
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok(outcomes) => {
                for outcome in outcomes {
                    fold_write_outcome(&mut window, outcome);
                }
            }
            Err(_) => window.failed = window.failed.saturating_add(1),
        }
    }
    window.offered = window
        .admitted
        .saturating_add(window.rejected)
        .saturating_add(window.failed);
    window
}

/// Offer open-loop writes at `rate` per second for `seconds` using shared ordinals.
///
/// Mirrors [`offer_writes`] but draws every batch ordinal from the shared
/// monotonic allocator so a diagonal write never deduplicates against the anchor
/// or debt phases of the same mixed run. Every write targets `table`, the mixed
/// family's Scribe write target for the run.
async fn offer_writes_paced(
    client: &ReferenceClient,
    rate: u64,
    seconds: u32,
    max_in_flight: u64,
    ordinals: &Arc<AtomicU64>,
    table: &str,
) -> WriteWindow {
    let mut window = WriteWindow::default();
    let total_ops = rate.saturating_mul(u64::from(seconds));
    if total_ops == 0 || rate == 0 {
        return window;
    }
    window.offered = total_ops;
    let interval = Duration::from_secs_f64(1.0 / rate as f64);
    let permits = usize::try_from(max_in_flight.clamp(1, MAX_IN_FLIGHT_CAP)).unwrap_or(1);
    let semaphore = Arc::new(Semaphore::new(permits));
    let start = tokio::time::Instant::now();
    let mut tasks = JoinSet::new();
    for step in 0..total_ops {
        tokio::time::sleep_until(start + interval.mul_f64(step as f64)).await;
        let permit = Arc::clone(&semaphore)
            .acquire_owned()
            .await
            .expect("dispatch semaphore is never closed while offers are in flight");
        let client = client.clone();
        let ordinal = ordinals.fetch_add(1, Ordering::Relaxed);
        let table = table.to_owned();
        tasks.spawn(async move {
            let _permit = permit;
            send_one_write(client, ordinal, table).await
        });
    }
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok(outcome) => fold_write_outcome(&mut window, outcome),
            Err(_) => window.failed = window.failed.saturating_add(1),
        }
    }
    window
}

/// Pure unit tests for the runner's fixture-independent report assembly logic.
#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    /// Build an empty capture delta whose process sample reads back as zero.
    fn empty_delta() -> BifrostTelemetryDelta {
        BifrostTelemetryDelta {
            families: BTreeSet::new(),
            metrics: Vec::new(),
            gauge_maxima: Vec::new(),
            gauge_final: Vec::new(),
            spans: Vec::new(),
            interval_seconds: 1.0,
            process: super::super::telemetry::ProcessWindow {
                identity: "test".to_owned(),
                epoch: 0,
                cpu_seconds: 0.0,
                current_rss_bytes: 0,
                peak_rss_bytes: 0,
                tokio_busy_seconds: 0.0,
                queue_peak: 0,
            },
        }
    }

    /// Build one measured window for stage-assembly tests.
    fn window(offered: u64, admitted: u64, rejected: u64, failed: u64) -> WriteWindow {
        WriteWindow {
            offered,
            admitted,
            rejected,
            failed,
            latencies_us: vec![10; usize::try_from(admitted).unwrap_or(0)],
        }
    }

    /// Proves nearest-rank percentile handles empty and populated samples.
    #[test]
    fn percentile_is_nearest_rank() {
        assert_eq!(percentile_us(&[], 99.0), None);
        assert_eq!(percentile_us(&[5], 99.0), Some(5));
        assert_eq!(percentile_us(&[1, 2, 3, 4, 5], 50.0), Some(3));
        assert_eq!(percentile_us(&[1, 2, 3, 4, 5], 100.0), Some(5));
    }

    /// Proves a fully admitted window yields a healthy stage with no saturation.
    #[test]
    fn fully_admitted_window_is_healthy() {
        let stage = assemble(window(100, 100, 0, 0));
        assert!(stage.healthy);
        assert_eq!(stage.saturation, None);
    }

    /// Proves a controlled-admission window is the sole saturation shape.
    #[test]
    fn controlled_admission_window_saturates() {
        let stage = assemble(window(100, 80, 20, 0));
        assert!(!stage.healthy);
        assert_eq!(stage.saturation, Some(SaturationCause::ControlledAdmission));
    }

    /// Proves the derived recommendation is the highest rung at or below 70%.
    #[test]
    fn recommendation_selects_the_seventy_percent_rung() {
        let mut stages = vec![
            assemble_at(0, 100, window(100, 100, 0, 0)),
            assemble_at(1, 200, window(100, 80, 20, 0)),
        ];
        let (reached, recommended) = derive_saturation_outcome(&mut stages);
        assert!(reached);
        let recommended = recommended.expect("saturated ladder recommends a rung");
        assert_eq!(recommended.ordinal, 0);
        assert_eq!(recommended.controlling_value, 100);
        assert_eq!(recommended.saturation_ordinal, 1);
        assert!((recommended.headroom_fraction - 0.5).abs() < 1e-9);
    }

    /// Proves a boundary with no qualifying healthy rung clears to no saturation.
    #[test]
    fn boundary_without_qualifying_rung_clears() {
        let mut stages = vec![assemble_at(0, 100, window(100, 80, 20, 0))];
        let (reached, recommended) = derive_saturation_outcome(&mut stages);
        assert!(!reached);
        assert!(recommended.is_none());
        assert_eq!(stages[0].saturation, None);
    }

    /// Build a smoke-shape dataset for the fixture-independent query tests.
    fn smoke_dataset() -> BifrostQualificationDataset {
        BifrostQualificationDataset::new(DatasetShape::new(2, 160_000).expect("valid smoke shape"))
            .expect("valid smoke dataset")
    }

    /// Proves each query id renders deterministic SQL and unknown ids fail.
    #[test]
    fn query_sql_is_deterministic_per_query() {
        let dataset = smoke_dataset();
        let queries = dataset.queries();
        let shape = dataset.shape();
        let q1 = query_sql("q1", &queries, shape).expect("q1 sql");
        assert!(q1.contains("vala.bifrost.bifrost_qualification_telemetry"));
        assert!(q1.contains("WHERE row_id = "));
        assert!(
            query_sql("q3", &queries, shape)
                .expect("q3 sql")
                .contains("GROUP BY metric")
        );
        assert!(
            query_sql("q4", &queries, shape)
                .expect("q4 sql")
                .contains("device_id % 256")
        );
        assert!(query_sql("q9", &queries, shape).is_err());
    }

    /// Proves scan widths match the reference predicate widths exactly.
    #[test]
    fn scan_range_rows_matches_predicate_widths() {
        let dataset = smoke_dataset();
        let queries = dataset.queries();
        let shape = dataset.shape();
        assert_eq!(scan_range_rows("q1", &queries, shape).expect("q1"), 1);
        assert_eq!(scan_range_rows("q2", &queries, shape).expect("q2"), 160_000);
        assert_eq!(scan_range_rows("q3", &queries, shape).expect("q3"), 80_000);
        let q5 = scan_range_rows("q5", &queries, shape).expect("q5");
        assert_eq!(
            q5,
            u64::try_from(queries.q5_end_row_id - queries.q5_start_row_id).expect("q5 width")
        );
        assert!(scan_range_rows("q9", &queries, shape).is_err());
    }

    /// Proves the summation tolerance accepts reordering but rejects drift.
    #[test]
    fn float_close_tolerates_summation_order() {
        assert!(float_close(1.0, 1.0 + 5e-7));
        assert!(float_close(0.0, 0.0));
        assert!(!float_close(1.0, 1.1));
    }

    /// Proves a fully completed query window is healthy with no saturation.
    #[test]
    fn fully_completed_query_window_is_healthy() {
        let stage = query_stage_for_test(query_window(50, 50, 0, 0));
        assert!(stage.healthy);
        assert_eq!(stage.saturation, None);
    }

    /// Proves a controlled-admission query window is the sole saturation shape.
    #[test]
    fn rejected_query_window_saturates() {
        let stage = query_stage_for_test(query_window(50, 40, 10, 0));
        assert!(!stage.healthy);
        assert_eq!(stage.saturation, Some(SaturationCause::ControlledAdmission));
    }

    /// Build one measured query window for stage-assembly tests.
    fn query_window(offered: u64, completed: u64, rejected: u64, failed: u64) -> QueryWindow {
        QueryWindow {
            offered,
            completed,
            rejected,
            failed,
            rows_returned: completed,
            bytes_returned: completed.saturating_mul(64),
            latencies_us: vec![10; usize::try_from(completed).unwrap_or(0)],
        }
    }

    /// Assemble a query stage at ordinal zero with a fixed `qps` control.
    fn query_stage_for_test(window: QueryWindow) -> BifrostCapacityStage {
        build_query_stage(
            "q1",
            StageControl::Qps {
                value: 100,
                max_in_flight: 256,
            },
            0,
            &window,
            QUALIFICATION_LOGICAL_ROW_BYTES,
            &empty_delta(),
            StageExecution {
                run_id: "run".to_owned(),
                ordinal: 0,
                capture_window: TelemetryWindowRef {
                    window_id: "w-0".to_owned(),
                    started_at_micros: 1,
                    ended_at_micros: 2,
                },
                observation_digests: vec![sha256_hex(b"")],
            },
        )
    }

    /// Assemble a stage at ordinal zero with a fixed `requests_per_sec` control.
    fn assemble(window: WriteWindow) -> BifrostCapacityStage {
        assemble_at(0, 100, window)
    }

    /// Assemble a stage at an ordinal and control value from a measured window.
    fn assemble_at(ordinal: u16, value: u64, window: WriteWindow) -> BifrostCapacityStage {
        let delta = empty_delta();
        build_ingest_stage(
            "ingest-small",
            StageControl::RequestsPerSec { value },
            ordinal,
            &window,
            &delta,
            StageExecution {
                run_id: "run".to_owned(),
                ordinal,
                capture_window: TelemetryWindowRef {
                    window_id: format!("w-{ordinal}"),
                    started_at_micros: 1,
                    ended_at_micros: 2,
                },
                observation_digests: vec![sha256_hex(b"")],
            },
        )
    }

    /// Ruling D-A: the distributed control is a closed-loop hold of one query per
    /// pod, built directly from the pod count. It never routes through the
    /// offer-ladder `stage_control` mapping, so the fixture `[1, 2, 3, 6]` ladder
    /// is honored as a pod ladder rather than an offer ladder.
    #[test]
    fn distributed_stage_control_holds_one_query_per_pod() {
        for pods in [1_u16, 2, 3, 6] {
            assert_eq!(
                distributed_stage_control(pods),
                StageControl::Concurrency {
                    value: u64::from(pods),
                }
            );
        }
    }

    /// AC5: `efficiency(n) = X(n) / (n · X(1))` is recomputed byte-exactly by the
    /// report validator, so a `DistributedBody` whose efficiency vector matches the
    /// weak-scaling formula validates and any tampered value is rejected.
    #[test]
    fn distributed_body_validates_weak_scaling_efficiency() {
        use super::super::bench_report::{CapacityBody, DistributedBody};

        let body = DistributedBody {
            query_id: "q1".to_owned(),
            oracle_pods: vec![1, 2, 3, 6],
            throughput: vec![1000, 1900, 2700, 4800],
            efficiency: vec![1.0, 0.95, 0.9, 0.8],
            saturation_reached: false,
            recommended: None,
        };
        assert!(body.validate().is_ok());

        let tampered = DistributedBody {
            efficiency: vec![1.0, 0.95, 0.9, 0.5],
            ..body
        };
        assert!(tampered.validate().is_err());
    }

    /// Ruling D-C: the fairness stage stamps the Jain index over per-tenant
    /// completions. Equal per-tenant service yields a perfect index; a window
    /// monopolized by one tenant collapses toward `1/N`.
    #[test]
    fn fairness_jain_index_rewards_equal_service() {
        assert!((jain_fairness(&[5, 5, 5, 5]) - 1.0).abs() < 1e-9);
        assert!((jain_fairness(&[10, 0, 0, 0]) - 0.25).abs() < 1e-9);
    }

    /// Proves the distributed sweep's efficiency vector is exactly the ratio the
    /// report validator recomputes, so a real sweep's body always validates.
    #[test]
    fn distributed_efficiency_matches_report_validator() {
        use super::super::bench_report::{CapacityBody, DistributedBody};

        let oracle_pods = [1_u16, 2, 3, 6];
        let throughput = [1000_u64, 1900, 2700, 4800];
        let efficiency = distributed_efficiency(&oracle_pods, &throughput);
        assert_eq!(efficiency.len(), oracle_pods.len());
        assert!((efficiency[0] - 1.0).abs() < 1e-9);

        let body = DistributedBody {
            query_id: "q1".to_owned(),
            oracle_pods: oracle_pods.to_vec(),
            throughput: throughput.to_vec(),
            efficiency,
            saturation_reached: false,
            recommended: None,
        };
        assert!(body.validate().is_ok());
    }

    /// Proves a degenerate zero baseline throughput yields non-finite ratios that
    /// the report validator rejects, so the sweep fails closed rather than
    /// fabricating an efficiency figure.
    #[test]
    fn distributed_efficiency_zero_baseline_is_non_finite() {
        let efficiency = distributed_efficiency(&[1, 2, 3, 6], &[0, 10, 20, 30]);
        assert!(efficiency.iter().any(|value| !value.is_finite()));
    }

    /// Proves a qualification-tier fairness invocation fails closed with the D69
    /// smoke-tier-only message and never attributes the sweep to T31, without
    /// booting a cluster (the refusal returns before any cluster start).
    #[tokio::test]
    async fn fairness_qualification_fails_closed_with_d69_message() {
        let workloads = WorkloadProfiles::load();
        let topologies = TopologyProfiles::load();
        let budgets = TierBudgets::load();
        let invocation = RunnerInvocation {
            workload_id: "tenants-2".to_owned(),
            topology_id: "one-pod".to_owned(),
            tier: RunnerTier::Qualification,
            output_root: std::path::PathBuf::from(
                "target/bifrost-benchmarks/test-fairness-refusal",
            ),
            run_id: "test-fairness-refusal".to_owned(),
        };
        let resolved = invocation
            .resolve(&workloads, &topologies)
            .expect("tenants-2 resolves against the locked fixtures");
        assert_eq!(
            RunnerFamily::classify(resolved.workload),
            RunnerFamily::Fairness
        );

        let error = run_fairness(Instant::now(), &invocation, resolved, &budgets, None)
            .await
            .expect_err("fairness has no qualification sweep");
        let message = error.to_string();
        assert!(message.contains("smoke-tier"), "message: {message}");
        assert!(message.contains("D69"), "message: {message}");
        assert!(
            !message.contains("T31"),
            "message must not attribute to T31: {message}"
        );
    }

    /// AC1: a table materialized by one family cluster is visible, with correct
    /// results, to a differently-shaped cluster booted over the same
    /// [`SharedRunResources`]. Cluster A (one pod) materializes the smoke dataset
    /// and shuts down; the shared fixture, storage root, and peer credentials
    /// survive that teardown; cluster B (two pods, a distinct topology) reboots
    /// over the same resources and reads the materialized
    /// `bifrost_qualification_telemetry` table with the expected results. This
    /// proves the run-shared catalog and storage make one materialization
    /// serviceable across every qualification-family cluster, and that cluster
    /// shutdown never tears down the run-owned shared resources.
    #[tokio::test]
    async fn materialized_table_is_visible_across_shared_resource_clusters() {
        let resources = SharedRunResources::provision()
            .await
            .expect("shared run resources provision");
        let invocation = RunnerInvocation {
            workload_id: "q2".to_owned(),
            topology_id: "one-pod".to_owned(),
            tier: RunnerTier::Smoke,
            output_root: std::path::PathBuf::from(
                "target/bifrost-benchmarks/test-cross-cluster-visibility",
            ),
            run_id: "test-cross-cluster-visibility".to_owned(),
        };

        // Cluster A (one pod) materializes the smoke dataset, then shuts down.
        let cluster_a = WyrdTestCluster::start_spec_with_shared_resources(
            topology_for_pods(1).expect("one-pod topology").spec(),
            &resources,
        )
        .await
        .expect("cluster A starts over shared resources");
        let dataset = materialize_smoke_dataset(&cluster_a, &invocation, "q2")
            .await
            .expect("cluster A materializes the smoke dataset");
        cluster_a
            .shutdown()
            .await
            .expect("cluster A shuts down without tearing down shared resources");

        // Cluster B (two pods, a distinct topology) reboots over the same
        // resources and must read cluster A's materialized table correctly.
        let cluster_b = WyrdTestCluster::start_spec_with_shared_resources(
            topology_for_pods(2).expect("two-pod topology").spec(),
            &resources,
        )
        .await
        .expect("cluster B starts over the same shared resources");
        let expected = dataset.expected_results();
        let queries = dataset.queries();
        let sql = query_sql("q2", &queries, dataset.shape()).expect("q2 sql renders");
        let clients = reference_clients(
            &cluster_b,
            std::slice::from_ref(&cluster_b.data_tenant_id()),
        )
        .await
        .expect("cluster B mints a reference client");
        let query = clients
            .first()
            .expect("cluster B reference client")
            .query
            .clone();
        probe_query_correctness(&query, "q2", &sql, &expected)
            .await
            .expect("cluster B reads cluster A's materialized table with correct results");

        cluster_b.shutdown().await.expect("cluster B shuts down");
        drop(resources);
    }
}
