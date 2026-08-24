//! Public-client Oracle operational journeys.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use arrow::array::{
    Array, ArrayRef, FixedSizeBinaryArray, FixedSizeBinaryBuilder, Int32Array, Int64Array,
    StringArray, TimestampMicrosecondArray,
};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use axum::body::Body;
use axum::http::{HeaderValue, Response, header};
use axum::routing::post;
use axum::{Json, Router};
use chrono::{NaiveDate, Utc};
use opendal::Buffer;
use parquet::arrow::ArrowWriter;
use secrecy::SecretString;
use serde::Serialize;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef, TenantTableBinding};
use vala_bifrost_redux::cluster::RoleTiming;
use vala_bifrost_redux::forge::ForgeLifecycleEvent;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::resources::{
    MIN_SCRATCH_FREE_BYTES, ResourceSnapshot, ResourceSource, SystemResourceSnapshot,
};
use vala_bifrost_redux::schema::with_managed_columns;
use vala_bifrost_redux::scribe::file_list_writer::{FileListInsert, insert_and_audit};
use vala_sdk::{BifrostGrpcTransport, CollectedQueryLimits, QueryClient, ValaSdkError};
use vala_sql::row_types::forge_tasks::{ForgeClaimStrategy, ForgeTaskStrategy};
use wyrd_client::WyrdClient;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_server::config::BifrostRuntimeRole;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuditDecision, AuditEvent, AuditResult, AuthMethod, BifrostQueryRequest, EventDay,
    FreshnessPolicy, QueryClass, QueryTerminalErrorCode, QueryTerminalOutcome, QueryWarning,
    VisibilityMode,
};
use wyrd_spec::vala::error::BifrostError;
use wyrd_testing::bifrost::{
    BifrostClusterSpec, ForgeCausalDiagnosis, ForgeCausalTelemetryReport, WyrdTestCluster,
};
use wyrd_testing::{Bootstrap, WyrdTestServer};
use wyrd_tonic::frame_codec::FrameDecoder;
use wyrd_tonic::otlp::common::v1::{AnyValue, KeyValue, any_value};
use wyrd_tonic::otlp::resource::v1::Resource as OtlpResource;
use wyrd_tonic::otlp::trace::v1::{
    ResourceSpans, ScopeSpans, Span as OtlpSpan, Status as OtlpStatus, span, status::StatusCode,
};
use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;
use wyrd_tonic::otlp::trace_service::trace_service_client::TraceServiceClient;
use wyrd_tonic::tonic::Request;
use wyrd_tonic::tonic::metadata::MetadataValue;
use wyrd_tonic::wyrd::v1 as proto;
use wyrd_tonic::wyrd::v1::bifrost_query_service_client::BifrostQueryServiceClient;
use wyrd_tonic::wyrd::v1::vala_query_service_client::ValaQueryServiceClient;
use wyrd_tonic::wyrd::v1::{QueryTracesRequest, QueryWindow};

type JourneyError = Box<dyn std::error::Error + Send + Sync>;

/// Rows carried by each public ingest request in the spill journeys.
const SPILL_INGEST_BATCH_ROWS: usize = 5_000;
/// Public ingest batches durably sealed together during fixture preparation.
const SPILL_BATCHES_PER_SEAL: usize = 10;

/// Returns the one-root managed-memory total represented by a snapshot.
fn managed_memory_used(snapshot: ResourceSnapshot) -> usize {
    snapshot.scribe_memory_used_bytes
        + snapshot.oracle_memory_used_bytes
        + snapshot.forge_memory_used_bytes
}

/// Builds a complete pod observation for production resource-policy journeys.
///
/// The fixture states only process-visible inputs. The Bifrost runtime derives
/// reserves, role floors, elastic capacity, leases, and target partitions.
///
/// # Panics
///
/// Panics only if the fixed test scratch observation exceeds `u64` capacity.
fn spill_system_resources(scratch_bytes: u64) -> SystemResourceSnapshot {
    let observed_scratch = scratch_bytes
        .checked_add(MIN_SCRATCH_FREE_BYTES)
        .expect("fixed spill observation fits u64");
    SystemResourceSnapshot {
        memory_limit_bytes: 832 << 20,
        effective_cpu: 4,
        scratch_capacity_bytes: observed_scratch,
        scratch_available_bytes: observed_scratch,
        memory_source: ResourceSource::Injected,
        cpu_source: ResourceSource::Injected,
    }
}

/// Builds a bounded Forge-convergence observation that can decode one Scribe row group.
fn forge_convergence_system_resources() -> SystemResourceSnapshot {
    let mut snapshot = spill_system_resources(1 << 30);
    snapshot.memory_limit_bytes = 2 << 30;
    snapshot
}

/// Asserts the production-derived mixed-role plan before costly fixture ingest.
///
/// # Panics
///
/// Panics when the server does not expose the single runtime owner or any
/// production-derived input, floor, elastic, scratch, CPU, or source differs.
fn assert_spill_resource_plan(server: &wyrd_testing::WyrdTestServer, scratch_bytes: u64) {
    let resources = server
        .state()
        .bifrost_resources()
        .expect("server owns global Bifrost resources");
    let plan = resources.plan();
    assert_eq!(plan.memory_limit_bytes, 832 << 20);
    assert_eq!(plan.unmanaged_reserve_bytes, 256 << 20);
    assert_eq!(plan.managed_memory_bytes, 576 << 20);
    assert_eq!(plan.scribe_floor_bytes, 256 << 20);
    assert_eq!(plan.oracle_floor_bytes, 256 << 20);
    assert_eq!(plan.elastic_memory_bytes, 64 << 20);
    assert_eq!(plan.scratch_limit_bytes, scratch_bytes);
    assert_eq!(plan.effective_cpu, 4);
    assert_eq!(resources.sources().memory, ResourceSource::Injected);
    assert_eq!(resources.sources().cpu, ResourceSource::Injected);
    assert_eq!(resources.sources().scratch, ResourceSource::Filesystem);
}

/// Asserts the exact lease held at the public query's schema barrier.
///
/// # Panics
///
/// Panics when the admitted query does not own the production-derived memory,
/// scratch, or adaptive partition envelope.
fn assert_active_spill_query_resources(server: &wyrd_testing::WyrdTestServer, scratch_bytes: u64) {
    let resources = server
        .state()
        .bifrost_resources()
        .expect("server owns global Bifrost resources");
    let snapshot = resources.snapshot().expect("active resource snapshot");
    assert!(snapshot.oracle_query_active);
    assert_eq!(snapshot.elastic_memory_used_bytes, 0);
    assert_eq!(snapshot.scratch_used_bytes, scratch_bytes);
    let query_memory = snapshot
        .plan
        .oracle_floor_bytes
        .checked_add(snapshot.elastic_memory_used_bytes)
        .expect("fixed query memory fits usize");
    assert_eq!(query_memory, 256 << 20);
    // A fully local scan hides no IO latency, so parallelism tracks cores rather
    // than fanning out; it is bounded by the working memory each partition needs
    // and never collapses to a single serial partition.
    let partitions = vala_bifrost_redux::resources::oracle_target_partitions(
        snapshot.plan.effective_cpu,
        1.0,
        query_memory,
    )
    .expect("active partition plan");
    assert_eq!(
        partitions,
        snapshot
            .plan
            .effective_cpu
            .min(
                query_memory / vala_bifrost_redux::resources::ORACLE_PARTITION_WORKING_MEMORY_BYTES
            )
            .max(vala_bifrost_redux::resources::ORACLE_MIN_TARGET_PARTITIONS),
        "local partition plan must follow cores clamped by per-partition working memory"
    );
    assert!(
        partitions >= vala_bifrost_redux::resources::ORACLE_MIN_TARGET_PARTITIONS,
        "an admitted query must never execute on a single serial partition"
    );
}

/// Complete normative production metric inventory and exact label-key sets.
const ORACLE_METRIC_LABELS: &[(&str, &[&str])] = &[
    ("oracle_queries_active", &["class"]),
    ("oracle_queries_queued", &["class"]),
    ("oracle_admission_total", &["class", "outcome", "reason"]),
    ("oracle_admission_queue_duration_seconds", &["class"]),
    ("oracle_tenant_budget_pressure", &["class"]),
    ("oracle_query_duration_seconds", &["class", "outcome"]),
    ("oracle_query_time_to_first_batch_seconds", &["class"]),
    ("oracle_query_rows_total", &["class"]),
    ("oracle_query_logical_bytes_selected_total", &["class"]),
    ("oracle_query_bytes_scanned_total", &["class"]),
    ("oracle_query_bytes_returned_total", &["class"]),
    ("oracle_query_files_scanned_total", &["class"]),
    ("oracle_query_partitions_scanned_total", &["class"]),
    ("oracle_query_spill_bytes_total", &["class"]),
    ("oracle_query_spill_files_total", &["class"]),
    ("oracle_query_spill_queries_total", &["class", "outcome"]),
    ("oracle_fragments_active", &[]),
    ("oracle_fragment_duration_seconds", &["outcome"]),
    ("oracle_query_cancellations_total", &["reason"]),
    ("oracle_audit_wal_records", &[]),
    ("oracle_audit_wal_bytes", &[]),
    ("oracle_audit_oldest_record_age_seconds", &[]),
    ("oracle_audit_append_duration_seconds", &[]),
    ("oracle_audit_relay_total", &["outcome"]),
    ("oracle_audit_relay_lag_seconds", &[]),
    ("oracle_audit_relay_batch_size", &[]),
    ("oracle_audit_relay_failures_total", &["reason"]),
    ("bifrost_role_ready", &["role"]),
];

/// Open-loop absolute-deadline schedule for one measured request stream.
#[derive(Debug, Clone, Copy)]
struct FixedRatePacer {
    /// Exact number of offered requests in each second.
    rate_per_sec: u64,
    /// Exact phase duration.
    duration: std::time::Duration,
    /// Maximum independently in-flight requests for this stream.
    max_in_flight: usize,
}

impl FixedRatePacer {
    /// Constructs a non-empty fixed-rate schedule with its own permit pool.
    ///
    /// # Errors
    ///
    /// Returns an error when the rate, duration, or permit count is zero.
    fn new(
        rate_per_sec: u64,
        duration: std::time::Duration,
        max_in_flight: usize,
    ) -> Result<Self, JourneyError> {
        if rate_per_sec == 0 || duration.is_zero() || max_in_flight == 0 {
            return Err("fixed-rate pacer inputs must be nonzero".into());
        }
        Ok(Self {
            rate_per_sec,
            duration,
            max_in_flight,
        })
    }

    /// Returns the exact number of absolute-deadline slots in this phase.
    #[must_use]
    fn slots(self) -> u64 {
        self.rate_per_sec.saturating_mul(self.duration.as_secs())
    }

    /// Returns the interval separating adjacent absolute-deadline slots.
    #[must_use]
    fn interval(self) -> std::time::Duration {
        std::time::Duration::from_nanos(1_000_000_000 / self.rate_per_sec)
    }
}

/// Exact terminal accounting for one phase and one request stream.
#[derive(Debug, Clone, Default, Serialize)]
struct PhaseCounters {
    /// Absolute-deadline slots offered by the harness.
    scheduled: u64,
    /// Slots dispatched into the public client.
    dispatched: u64,
    /// Requests that reached a terminal result.
    completed: u64,
    /// Slots rejected by harness lateness or permit exhaustion.
    harness_saturated: u64,
    /// Requests that exceeded the bounded drain.
    timed_out: u64,
    /// Requests cancelled after the bounded drain.
    cancelled: u64,
    /// Rows accepted by successful write terminals.
    accepted_rows: u64,
    /// Typed retryable capacity refusals observed by this stream.
    capacity_refusals: u64,
}

/// Serializable evidence from one fresh-cluster paired trial.
#[derive(Debug, Clone, Serialize)]
struct PairedTrialResult {
    /// One-based trial ordinal.
    trial: u64,
    /// Baseline write-stream counters.
    baseline_writes: PhaseCounters,
    /// Overlap write-stream counters.
    overlap_writes: PhaseCounters,
    /// Overlap COUNT-stream counters.
    overlap_queries: PhaseCounters,
    /// Accepted-row rate ratio between overlap and baseline.
    ratio: f64,
    /// Maximum strict visibility delay in milliseconds.
    visibility_ms: u64,
    /// Successful write and COUNT completions in each five-second window.
    progress_windows: Vec<PairedProgressWindow>,
    /// Maxima sampled from production owners every ten milliseconds.
    peaks: PairedPeaks,
}

/// Successful terminal progress observed in one five-second overlap window.
#[derive(Debug, Clone, Serialize)]
struct PairedProgressWindow {
    /// Zero-based five-second window ordinal.
    window: u64,
    /// Successful write terminals in this window.
    writes_completed: u64,
    /// Successful COUNT terminals in this window.
    queries_completed: u64,
}

/// Maxima and invariant limits sampled from production memory owners.
#[derive(Debug, Clone, Default, Serialize)]
struct PairedPeaks {
    /// Maximum parent Bifrost bytes.
    parent_bytes: usize,
    /// Parent Bifrost ceiling.
    parent_limit: usize,
    /// Maximum Scribe child bytes.
    scribe_bytes: usize,
    /// Scribe child ceiling.
    scribe_limit: usize,
    /// Maximum Oracle child bytes.
    oracle_bytes: usize,
    /// Oracle child ceiling.
    oracle_limit: usize,
    /// Maximum Oracle admission-reserved bytes.
    oracle_admission_bytes: u64,
}

/// Fixed report retained for Gate 4 integration and pre-review validation.
#[derive(Debug, Serialize)]
struct PairedProgressReport {
    /// Three independent fresh-cluster trials.
    trials: Vec<PairedTrialResult>,
    /// Median of the three accepted-row ratios.
    median_ratio: f64,
    /// Range across the three accepted-row ratios.
    ratio_range: f64,
    /// Median absolute deviation across the three ratios.
    ratio_mad: f64,
}

/// Task-local owner for one production-client baseline/overlap comparison.
struct PairedProgressEngine {
    /// Authenticated public writer transport shared by cloned request tasks.
    writer: BifrostGrpcTransport,
    /// Authenticated public query client configuration.
    reader: WyrdClient,
    /// Fully-qualified table targeted by both streams.
    table: String,
    /// Baseline and overlap write schedule.
    write_pacer: FixedRatePacer,
    /// Independent overlap query schedule.
    query_pacer: FixedRatePacer,
}

/// Terminal result returned by one phase-tagged public request.
enum PairedRequestResult {
    /// One write completed and accepted the fixed 64-row payload.
    WriteAccepted,
    /// One COUNT query completed successfully.
    QueryCompleted,
    /// One request received the typed retryable capacity refusal.
    CapacityRefused,
    /// A non-capacity failure invalidated the trial.
    Fatal(String),
}

impl PairedProgressEngine {
    /// Builds one engine from already-authenticated public clients.
    ///
    /// # Errors
    ///
    /// Returns an error when either locked pacer configuration is invalid.
    fn new(
        writer: BifrostGrpcTransport,
        reader: WyrdClient,
        table: String,
    ) -> Result<Self, JourneyError> {
        Ok(Self {
            writer,
            reader,
            table,
            write_pacer: FixedRatePacer::new(50, std::time::Duration::from_secs(30), 32)?,
            query_pacer: FixedRatePacer::new(25, std::time::Duration::from_secs(30), 16)?,
        })
    }

    /// Runs one write-only measured phase at the locked offered rate.
    ///
    /// # Errors
    ///
    /// Returns an error for harness saturation, timeout, cancellation, or any
    /// non-capacity public write failure.
    async fn run_write_phase(&self, phase: u64) -> Result<PhaseCounters, JourneyError> {
        self.run_write_phase_with(phase, self.write_pacer).await
    }

    /// Runs a caller-selected write pacer used by measured and warmup phases.
    ///
    /// # Errors
    ///
    /// Returns the same exact-schedule and public-write errors as the measured
    /// phase owner.
    async fn run_write_phase_with(
        &self,
        phase: u64,
        pacer: FixedRatePacer,
    ) -> Result<PhaseCounters, JourneyError> {
        self.run_write_phase_tracked(phase, pacer, None).await
    }

    /// Runs a write phase while optionally recording five-second completions.
    async fn run_write_phase_tracked(
        &self,
        phase: u64,
        pacer: FixedRatePacer,
        progress: Option<Arc<Vec<AtomicU64>>>,
    ) -> Result<PhaseCounters, JourneyError> {
        let permits = Arc::new(Semaphore::new(pacer.max_in_flight));
        let payload = Arc::new(paired_ipc());
        if payload.len() > 64 * 1024 {
            return Err("paired write payload exceeds 64 KiB".into());
        }
        let start = tokio::time::Instant::now();
        let interval = pacer.interval();
        let mut counters = PhaseCounters::default();
        let mut tasks = JoinSet::new();
        for ordinal in 0..pacer.slots() {
            counters.scheduled = counters.scheduled.saturating_add(1);
            let slot = start + interval.mul_f64(ordinal as f64);
            tokio::time::sleep_until(slot).await;
            if tokio::time::Instant::now().saturating_duration_since(slot) > interval {
                counters.harness_saturated = counters.harness_saturated.saturating_add(1);
                continue;
            }
            let Ok(permit) = Arc::clone(&permits).try_acquire_owned() else {
                counters.harness_saturated = counters.harness_saturated.saturating_add(1);
                continue;
            };
            counters.dispatched = counters.dispatched.saturating_add(1);
            let transport = self.writer.clone();
            let table = self.table.clone();
            let payload = Arc::clone(&payload);
            let progress = progress.clone();
            tasks.spawn(async move {
                let _permit = permit;
                let batch_id = paired_batch_id(phase as usize, ordinal);
                match transport
                    .insert_batch(&table, batch_id, (*payload).clone())
                    .await
                {
                    Ok(_) => {
                        record_progress_window(
                            progress.as_ref().map(|value| value.as_slice()),
                            start,
                        );
                        PairedRequestResult::WriteAccepted
                    }
                    Err(WyrdError::Vala {
                        error: BifrostError::IngestBusy { .. },
                    }) => PairedRequestResult::CapacityRefused,
                    Err(error) => PairedRequestResult::Fatal(error.to_string()),
                }
            });
        }
        collect_paired_tasks(&mut tasks, &mut counters).await?;
        validate_phase_counters(&counters, pacer.slots(), "write")?;
        Ok(counters)
    }

    /// Runs independent write and COUNT streams over the same measured window.
    ///
    /// # Errors
    ///
    /// Returns an error when either stream violates its exact schedule or
    /// receives a non-capacity failure.
    async fn run_overlap_phase(
        &self,
        phase: u64,
    ) -> Result<(PhaseCounters, PhaseCounters, Vec<PairedProgressWindow>), JourneyError> {
        let write_progress = Arc::new((0..6).map(|_| AtomicU64::new(0)).collect::<Vec<_>>());
        let query_progress = Arc::new((0..6).map(|_| AtomicU64::new(0)).collect::<Vec<_>>());
        let writes = self.run_write_phase_tracked(
            phase,
            self.write_pacer,
            Some(Arc::clone(&write_progress)),
        );
        let queries =
            self.run_query_phase_tracked(self.query_pacer, Some(Arc::clone(&query_progress)));
        let (writes, queries) = tokio::try_join!(writes, queries)?;
        let windows = (0..6)
            .map(|window| PairedProgressWindow {
                window: window as u64,
                writes_completed: write_progress[window].load(Ordering::Acquire),
                queries_completed: query_progress[window].load(Ordering::Acquire),
            })
            .collect::<Vec<_>>();
        if windows
            .iter()
            .any(|window| window.writes_completed == 0 || window.queries_completed == 0)
        {
            return Err(
                format!("overlap lacked continuous five-second progress: {windows:?}").into(),
            );
        }
        Ok((writes, queries, windows))
    }

    /// Runs a caller-selected independent query pacer for warmup or measurement.
    ///
    /// # Errors
    ///
    /// Returns the same exact-schedule and query-terminal errors as the measured
    /// query phase.
    async fn run_query_phase_with(
        &self,
        pacer: FixedRatePacer,
    ) -> Result<PhaseCounters, JourneyError> {
        self.run_query_phase_tracked(pacer, None).await
    }

    /// Runs a query phase while optionally recording five-second completions.
    async fn run_query_phase_tracked(
        &self,
        pacer: FixedRatePacer,
        progress: Option<Arc<Vec<AtomicU64>>>,
    ) -> Result<PhaseCounters, JourneyError> {
        let permits = Arc::new(Semaphore::new(pacer.max_in_flight));
        let start = tokio::time::Instant::now();
        let interval = pacer.interval();
        let mut counters = PhaseCounters::default();
        let mut tasks = JoinSet::new();
        for ordinal in 0..pacer.slots() {
            counters.scheduled = counters.scheduled.saturating_add(1);
            let slot = start + interval.mul_f64(ordinal as f64);
            tokio::time::sleep_until(slot).await;
            if tokio::time::Instant::now().saturating_duration_since(slot) > interval {
                counters.harness_saturated = counters.harness_saturated.saturating_add(1);
                continue;
            }
            let Ok(permit) = Arc::clone(&permits).try_acquire_owned() else {
                counters.harness_saturated = counters.harness_saturated.saturating_add(1);
                continue;
            };
            counters.dispatched = counters.dispatched.saturating_add(1);
            let reader = self.reader.clone();
            let table = self.table.clone();
            let progress = progress.clone();
            tasks.spawn(async move {
                let _permit = permit;
                let result = paired_count(&reader, &table).await;
                if matches!(result, PairedRequestResult::QueryCompleted) {
                    record_progress_window(progress.as_ref().map(|value| value.as_slice()), start);
                }
                result
            });
        }
        collect_paired_tasks(&mut tasks, &mut counters).await?;
        validate_phase_counters(&counters, pacer.slots(), "query")?;
        Ok(counters)
    }
}

/// Executes one strict public COUNT and preserves pre-stream typed capacity.
async fn paired_count(reader: &WyrdClient, table: &str) -> PairedRequestResult {
    let mut stream = match QueryClient::new(reader)
        .query(&BifrostQueryRequest {
            sql: format!("SELECT COUNT(*) FROM {table}"),
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: Some(10_000),
        })
        .await
    {
        Ok(stream) => stream,
        Err(ValaSdkError::Transport(WyrdError::Vala {
            error: BifrostError::QueryAdmissionRejected,
        })) => return PairedRequestResult::CapacityRefused,
        Err(error) => return PairedRequestResult::Fatal(error.to_string()),
    };
    loop {
        match stream.next_batch().await {
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(error) => return PairedRequestResult::Fatal(error.to_string()),
        }
    }
    if stream.terminal().is_some_and(|terminal| {
        terminal.outcome == QueryTerminalOutcome::Success && terminal.error.is_none()
    }) {
        PairedRequestResult::QueryCompleted
    } else {
        PairedRequestResult::Fatal("COUNT stream lacked a successful terminal".to_owned())
    }
}

/// Executes a strict COUNT and decodes its exact scalar value.
///
/// # Errors
///
/// Returns a typed client, Arrow, conversion, or terminal-contract error.
async fn strict_count_value(reader: &WyrdClient, table: &str) -> Result<u64, JourneyError> {
    let mut stream = QueryClient::new(reader)
        .query(&BifrostQueryRequest {
            sql: format!("SELECT COUNT(*) FROM {table}"),
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: Some(30_000),
        })
        .await?;
    let mut value = None;
    while let Some(batch) = stream.next_batch().await? {
        let counts = batch
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .ok_or("COUNT result is not Int64")?;
        if counts.len() != 1 {
            return Err("COUNT result must contain exactly one row".into());
        }
        value = Some(u64::try_from(counts.value(0))?);
    }
    let terminal = stream.terminal().ok_or("COUNT terminal missing")?;
    if terminal.outcome != QueryTerminalOutcome::Success || terminal.error.is_some() {
        return Err("COUNT terminal was not successful".into());
    }
    value.ok_or_else(|| "COUNT scalar missing".into())
}

/// Streams the spill fixture's wide rows through an analytical public query
/// and decodes exact row and value-byte totals from its successful terminal.
///
/// # Errors
///
/// Returns a typed client, Arrow, conversion, or terminal-contract error.
async fn strict_spill_summary(
    reader: &WyrdClient,
    table: &str,
) -> Result<(u64, u64), JourneyError> {
    strict_wide_summary(reader, table, true).await
}

/// Streams the spill fixture without a semantic ordering requirement.
///
/// # Errors
///
/// Returns the same typed query, Arrow, conversion, or terminal error as the
/// ordered spill summary.
async fn strict_unordered_summary(
    reader: &WyrdClient,
    table: &str,
) -> Result<(u64, u64), JourneyError> {
    strict_wide_summary(reader, table, false).await
}

/// Streams the wide fixture with optional caller-requested ordering.
///
/// # Errors
///
/// Returns a typed client, Arrow, conversion, or terminal-contract error.
async fn strict_wide_summary(
    reader: &WyrdClient,
    table: &str,
    ordered: bool,
) -> Result<(u64, u64), JourneyError> {
    let order = if ordered { " ORDER BY id" } else { "" };
    let mut stream = QueryClient::new(reader)
        .query(&BifrostQueryRequest {
            sql: format!("SELECT id, value FROM {table}{order}"),
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: Some(30_000),
        })
        .await?;
    let first_batch = stream
        .next_batch()
        .await?
        .ok_or("wide query returned no first batch")?;
    let first_ids = first_batch
        .column(0)
        .as_any()
        .downcast_ref::<Int64Array>()
        .ok_or("first spill id result is not Int64")?;
    let first_values = first_batch
        .column(1)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or("first spill value result is not Utf8")?;
    let mut row_count = u64::try_from(first_ids.len())?;
    let mut value_bytes = first_values
        .iter()
        .flatten()
        .try_fold(0_u64, |total, value| {
            total
                .checked_add(u64::try_from(value.len())?)
                .ok_or_else(|| JourneyError::from("spill value-byte total overflow"))
        })?;
    while let Some(batch) = stream.next_batch().await? {
        let ids = batch
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .ok_or("spill id result is not Int64")?;
        let values = batch
            .column(1)
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or("spill value result is not Utf8")?;
        if ids.len() != values.len() {
            return Err("spill result columns have unequal lengths".into());
        }
        row_count = row_count
            .checked_add(u64::try_from(ids.len())?)
            .ok_or("spill row count overflow")?;
        for value in values.iter() {
            value_bytes = value_bytes
                .checked_add(u64::try_from(value.ok_or("spill value is null")?.len())?)
                .ok_or("spill value-byte count overflow")?;
        }
    }
    let terminal = stream.terminal().ok_or("spill stream terminal missing")?;
    if terminal.outcome != QueryTerminalOutcome::Success || terminal.error.is_some() {
        return Err("spill stream terminal was not successful".into());
    }
    Ok((row_count, value_bytes))
}

/// Samples production Scribe/Oracle owners every ten milliseconds until cancelled.
///
/// # Errors
///
/// Returns an inspection error or a peak that exceeds its production ceiling.
async fn sample_paired_peaks(
    server: &wyrd_testing::WyrdTestServer,
    stop: CancellationToken,
) -> Result<PairedPeaks, JourneyError> {
    let governor = server
        .state()
        .bifrost_resources()
        .ok_or("paired server lacks the shared governor")?;
    let mut peaks = PairedPeaks::default();
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(10));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            () = stop.cancelled() => break,
            _ = interval.tick() => {
                let (memory, scribe, oracle) = tokio::task::block_in_place(|| {
                    Ok::<_, wyrd_testing::WyrdTestServerError>((
                        governor.snapshot().expect("paired root resource snapshot"),
                        server.scribe_inspection_snapshot()?,
                        server.oracle_runtime_inspection()?,
                    ))
                })?;
                peaks.parent_bytes = peaks.parent_bytes.max(managed_memory_used(memory));
                peaks.parent_limit = memory.plan.managed_memory_bytes;
                peaks.scribe_bytes = peaks.scribe_bytes.max(scribe.scribe_used_memory);
                peaks.scribe_limit = scribe.scribe_memory_limit;
                peaks.oracle_bytes = peaks.oracle_bytes.max(memory.oracle_memory_used_bytes);
                peaks.oracle_limit = memory.plan.oracle_floor_bytes + memory.plan.elastic_memory_bytes;
                peaks.oracle_admission_bytes = peaks
                    .oracle_admission_bytes
                    .max(oracle.reserved_memory_bytes);
            }
        }
    }
    if peaks.parent_bytes > peaks.parent_limit
        || peaks.scribe_bytes > peaks.scribe_limit
        || peaks.oracle_bytes > peaks.oracle_limit
        || peaks.oracle_admission_bytes > u64::try_from(peaks.oracle_limit)?
    {
        return Err(format!("paired memory peak exceeded a production limit: {peaks:?}").into());
    }
    Ok(peaks)
}

/// Drains phase-tagged request tasks for at most fifteen seconds.
///
/// # Errors
///
/// Returns the first fatal request error or a drain timeout after aborting all
/// remaining phase tasks.
async fn collect_paired_tasks(
    tasks: &mut JoinSet<PairedRequestResult>,
    counters: &mut PhaseCounters,
) -> Result<(), JourneyError> {
    let drain = async {
        while let Some(result) = tasks.join_next().await {
            counters.completed = counters.completed.saturating_add(1);
            match result.map_err(|error| error.to_string())? {
                PairedRequestResult::WriteAccepted => {
                    counters.accepted_rows = counters.accepted_rows.saturating_add(64);
                }
                PairedRequestResult::QueryCompleted => {}
                PairedRequestResult::CapacityRefused => {
                    counters.capacity_refusals = counters.capacity_refusals.saturating_add(1);
                }
                PairedRequestResult::Fatal(error) => return Err(error.into()),
            }
        }
        Ok::<(), JourneyError>(())
    };
    if tokio::time::timeout(std::time::Duration::from_secs(15), drain)
        .await
        .is_err()
    {
        counters.timed_out = tasks.len() as u64;
        counters.cancelled = tasks.len() as u64;
        tasks.abort_all();
        return Err("paired phase exceeded its fifteen-second drain".into());
    }
    Ok(())
}

/// Enforces exact offered/dispatched/completed counts and zero harness loss.
///
/// # Errors
///
/// Returns an error when the harness weakened the locked offered stream.
fn validate_phase_counters(
    counters: &PhaseCounters,
    expected: u64,
    stream: &str,
) -> Result<(), JourneyError> {
    if counters.scheduled != expected
        || counters.dispatched != expected
        || counters.completed != expected
        || counters.harness_saturated != 0
        || counters.timed_out != 0
        || counters.cancelled != 0
    {
        return Err(format!(
            "{stream} harness failed exact schedule (32 write permits assert sub-640ms service occupancy): {counters:?}"
        )
        .into());
    }
    Ok(())
}

/// Copies the locked UUIDv7-shaped deterministic batch identity algorithm.
#[must_use]
fn paired_batch_id(tenant_index: usize, ordinal: u64) -> [u8; 16] {
    let mut bytes = [0_u8; 16];
    bytes[..8].copy_from_slice(&(0xB1_F057_u64 ^ ordinal).to_be_bytes());
    bytes[8..].copy_from_slice(&(tenant_index as u64 ^ ordinal.rotate_left(17)).to_be_bytes());
    bytes[6] = (bytes[6] & 0x0f) | 0x70;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    bytes
}

/// Increments the actual five-second measurement window for one terminal.
///
/// Terminals completing during the later drain remain part of aggregate phase
/// counters but cannot establish progress inside the thirty-second overlap.
fn record_progress_window(progress: Option<&[AtomicU64]>, started: tokio::time::Instant) {
    let Some(progress) = progress else {
        return;
    };
    let Ok(window) = usize::try_from(started.elapsed().as_secs() / 5) else {
        return;
    };
    if let Some(counter) = progress.get(window) {
        counter.fetch_add(1, Ordering::AcqRel);
    }
}

/// Encodes the locked one-column, non-null 64-row paired write payload.
#[must_use]
fn paired_ipc() -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![Field::new(
        "row_id",
        DataType::Int64,
        false,
    )]));
    let rows = RecordBatch::try_new(
        Arc::clone(&schema),
        vec![Arc::new(Int64Array::from_iter_values(0_i64..64))],
    )
    .expect("paired rows match their one-column schema");
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, schema.as_ref())
        .expect("paired IPC writer accepts its schema");
    writer
        .write(&rows)
        .expect("paired IPC writer accepts its batch");
    writer.finish().expect("paired IPC stream finishes");
    assert!(bytes.len() <= 64 * 1024, "paired IPC exceeds 64 KiB");
    bytes
}

/// Consequential production spans required by the Oracle operational contract.
const ORACLE_SPANS: &[&str] = &[
    "bifrost.gate.role_dispatch",
    "bifrost.oracle.query",
    "bifrost.oracle.plan",
    "bifrost.oracle.audit",
    "bifrost.oracle.source",
    "bifrost.oracle.admission",
    "bifrost.oracle.tail",
    "bifrost.oracle.tail_fence",
    "bifrost.oracle.fragment",
    "bifrost.oracle.slot_reservation",
    "bifrost.oracle.reconcile",
    "bifrost.oracle.stream",
];

/// J1 proves exact PublishedOnly rows and a validated terminal through the public SDK.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_published_journey() {
    public_roundtrip(
        BifrostClusterSpec::one_mixed(),
        VisibilityMode::PublishedOnly,
        true,
        0,
        None,
        false,
    )
    .await
    .expect("J1 PublishedOnly journey");
}

/// A retained production Oracle reservation produces the public typed 429 and
/// allows the same durable query to complete after capacity is released.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_capacity_contract_journey() {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed())
        .await
        .expect("capacity contract cluster");
    let server = cluster.server(0).expect("mixed server");
    let table = unique_table("oracle_capacity_contract");
    register_table(server, cluster.data_tenant_id(), &table)
        .await
        .expect("capacity contract table");
    let writer = client(server, "oracle-capacity-writer")
        .await
        .expect("writer client");
    ingest(&writer, &format!("vala.bifrost.{table}"), &[1, 2])
        .await
        .expect("capacity fixture ingest");
    server
        .flush_bifrost()
        .await
        .expect("capacity fixture flush");
    let reader = client(server, "oracle-capacity-reader")
        .await
        .expect("reader client");
    let governor = server
        .state()
        .bifrost_resources()
        .expect("shared production memory governor");
    let oracle = governor.oracle().expect("Oracle resource capability");
    let retained = oracle
        .try_acquire_query(vala_bifrost_redux::resources::OracleResourceRequest {
            query_class: QueryClass::Interactive,
            memory_bytes: vala_bifrost_redux::resources::ORACLE_PARTITION_MEMORY_BYTES,
            scratch_bytes: vala_bifrost_redux::resources::ORACLE_PARTITION_MEMORY_BYTES as u64,
            slot_units: 1,
            local_ratio: 1.0,
        })
        .expect("retain the complete Oracle child budget");

    let refusal = match QueryClient::new(&reader)
        .query(&BifrostQueryRequest {
            sql: format!("SELECT id, value FROM vala.bifrost.{table} ORDER BY id"),
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: Some(10_000),
        })
        .await
    {
        Ok(_) => panic!("occupied Oracle budget must reject before opening a response stream"),
        Err(error) => error,
    };
    assert!(matches!(
        refusal,
        ValaSdkError::Transport(WyrdError::Vala {
            error: BifrostError::QueryAdmissionRejected
        })
    ));

    drop(retained);
    assert_eq!(
        query_rows(&reader, &table, VisibilityMode::PublishedOnly)
            .await
            .expect("query succeeds after retained capacity drains"),
        2
    );

    cluster.shutdown().await.expect("capacity cluster shutdown");
}

/// An unordered production query streams the exact durable rows without a
/// mandatory reconciliation spill and releases every local owner.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_spill_success_is_bounded_and_exact() {
    let cluster = WyrdTestCluster::start_spec(
        BifrostClusterSpec::one_mixed().with_system_resources(spill_system_resources(1 << 30)),
    )
    .await
    .expect("spill success cluster");
    let server = cluster.server(0).expect("spill success server");
    assert_spill_resource_plan(server, 1 << 30);
    let table = prepare_spill_table(&cluster, server, "oracle_spill_success", 1_000_000)
        .await
        .expect("spill success fixture");
    let reader = client(server, "oracle-spill-success-reader")
        .await
        .expect("spill success reader");
    let baseline = server
        .oracle_runtime_inspection()
        .expect("spill success baseline");
    let memory = server
        .state()
        .bifrost_resources()
        .expect("spill success governor")
        .snapshot()
        .expect("spill success resource snapshot");
    let memory_baseline = (managed_memory_used(memory), memory.oracle_memory_used_bytes);
    let checkpoint = cluster.telemetry().checkpoint().expect("spill checkpoint");

    assert_eq!(
        strict_unordered_summary(&reader, &format!("vala.bifrost.{table}"))
            .await
            .expect("spilling stream succeeds"),
        (1_000_000, 256_000_000)
    );

    let delta = cluster
        .telemetry()
        .delta_since(&checkpoint)
        .expect("spill metric delta");
    assert!(!delta.metrics.iter().any(|sample| {
        matches!(
            sample.family.as_str(),
            "oracle_query_spill_bytes_total" | "oracle_query_spill_files_total"
        ) && sample.value > 0.0
    }));
    assert_oracle_runtime_restored(server, baseline, memory_baseline)
        .expect("success cleanup is exact");
    cluster.shutdown().await.expect("spill success shutdown");
}

/// Real Forge workers replace the fixture's small files before an ordered read.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_forge_small_files_converge_before_ordered_read() {
    let cluster = WyrdTestCluster::start_spec_with_forge_completion_observer(
        BifrostClusterSpec::one_mixed().with_system_resources(forge_convergence_system_resources()),
    )
    .await
    .expect("Forge convergence cluster");
    let checkpoint = cluster
        .telemetry()
        .checkpoint()
        .expect("Forge causal telemetry checkpoint");
    let server = cluster.server(0).expect("Forge convergence server");
    let table = prepare_spill_table(&cluster, server, "forge_small_files", 1_000_000)
        .await
        .expect("Forge convergence fixture");
    let reader = client(server, "forge-small-files-reader")
        .await
        .expect("Forge convergence reader");
    assert_eq!(
        strict_unordered_summary(&reader, &format!("vala.bifrost.{table}"))
            .await
            .expect("pre-Forge exact dataset"),
        (1_000_000, 256_000_000)
    );
    server
        .forge_clock()
        .advance(chrono::Duration::days(1))
        .expect("close the fixture's event-day partition");
    let observer = cluster
        .forge_completion_observer()
        .expect("Forge completion observer");
    for transition in 1..=64 {
        let workflow = server
            .inspect_forge_workflow_for_test(cluster.data_tenant_id(), &table)
            .await
            .expect("inspect staging-fold convergence");
        if workflow.uncompacted_staging_files == 0 {
            break;
        }
        advance_forge_retry_for_journey(server, cluster.data_tenant_id(), &table).await;
        let expected_attempts = observer.attempts().saturating_add(1);
        let completed_passes = server.completed_forge_scheduler_passes_for_test();
        cluster.request_forge_scheduler_pass_for_test();
        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            server.wait_for_forge_scheduler_passes_for_test(completed_passes + 1),
        )
        .await
        .expect("production staging scheduler pass completes");
        let transition_result = tokio::time::timeout(
            std::time::Duration::from_secs(90),
            observer.wait_for_attempts_at_least(expected_attempts),
        )
        .await;
        if transition_result.is_err() {
            let delta = cluster
                .telemetry()
                .delta_since(&checkpoint)
                .expect("stalled Forge continuation telemetry delta");
            let report = ForgeCausalTelemetryReport::from_production_delta(&delta);
            let workflow = server
                .inspect_forge_workflow_for_test(cluster.data_tenant_id(), &table)
                .await
                .expect("inspect stalled Forge continuation");
            let diagnosis = report
                .as_ref()
                .map_err(ToString::to_string)
                .and_then(|report| {
                    report
                        .diagnose(&workflow, None)
                        .map_err(|error| error.to_string())
                });
            panic!(
                "Forge continuation did not complete: transition={transition} diagnosis={diagnosis:?} telemetry={report:?} workflow={workflow:?}"
            );
        }
    }
    assert_eq!(
        server
            .inspect_forge_workflow_for_test(cluster.data_tenant_id(), &table)
            .await
            .expect("inspect staging-fold terminal state")
            .uncompacted_staging_files,
        0,
        "bounded production continuation must drain staging debt"
    );
    let before = server
        .inspect_forge_table_for_test(cluster.data_tenant_id(), &table)
        .await
        .expect("pre-compaction table inspection");
    assert!(before.data_file_count() > 1);
    assert!(before.total_data_file_bytes().expect("pre-Forge bytes") > 0);
    let forge_baseline = server
        .state()
        .forge()
        .expect("Forge composition")
        .resources()
        .snapshot()
        .expect("Forge resource baseline");
    for _ in 0..64 {
        if observer
            .completed_strategies()
            .contains(&ForgeClaimStrategy::Known(ForgeTaskStrategy::SmallFiles))
        {
            break;
        }
        advance_forge_retry_for_journey(server, cluster.data_tenant_id(), &table).await;
        let completed_passes = server.completed_forge_scheduler_passes_for_test();
        cluster.request_forge_scheduler_pass_for_test();
        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            server.wait_for_forge_scheduler_passes_for_test(completed_passes + 1),
        )
        .await
        .expect("production rewrite scheduler pass completes");
        let expected_completions = observer.completed().saturating_add(1);
        if tokio::time::timeout(
            std::time::Duration::from_secs(90),
            observer.wait_for_at_least(expected_completions),
        )
        .await
        .is_err()
        {
            let delta = cluster
                .telemetry()
                .delta_since(&checkpoint)
                .expect("stalled rewrite telemetry delta");
            let report = ForgeCausalTelemetryReport::from_production_delta(&delta);
            let workflow = server
                .inspect_forge_workflow_for_test(cluster.data_tenant_id(), &table)
                .await
                .expect("inspect stalled production rewrite");
            let diagnosis = report
                .as_ref()
                .map_err(ToString::to_string)
                .and_then(|report| {
                    report
                        .diagnose(&workflow, None)
                        .map_err(|error| error.to_string())
                });
            panic!(
                "production maintenance task did not complete: diagnosis={diagnosis:?} telemetry={report:?} workflow={workflow:?} table={before:?}"
            );
        }
    }
    assert!(
        observer.completed_strategies().iter().any(|strategy| {
            *strategy == ForgeClaimStrategy::Known(ForgeTaskStrategy::SmallFiles)
        }),
        "bounded production continuation must reach the SmallFiles rewrite"
    );
    let events = observer.lifecycle_events();
    let planned_inputs = events
        .iter()
        .rev()
        .find_map(|event| match event {
            ForgeLifecycleEvent::Planned {
                table: observed,
                inputs,
                ..
            } if observed == &table => Some(inputs.clone()),
            _ => None,
        })
        .expect("table-scoped planned inputs");
    let after = server
        .inspect_forge_table_for_test(cluster.data_tenant_id(), &table)
        .await
        .expect("post-Forge table inspection");
    let rewrite = wyrd_testing::WyrdTestServer::compare_forge_rewrite_for_test(
        &before,
        &after,
        &planned_inputs,
    )
    .expect("exact Forge replacement comparison");
    let delta = cluster
        .telemetry()
        .delta_since(&checkpoint)
        .expect("converged Forge production telemetry delta");
    let report = ForgeCausalTelemetryReport::from_production_delta(&delta)
        .expect("converged Forge causal telemetry report");
    let workflow = server
        .inspect_forge_workflow_for_test(cluster.data_tenant_id(), &table)
        .await
        .expect("converged Forge durable workflow");
    assert_eq!(
        report
            .diagnose(&workflow, Some(&rewrite))
            .expect("Forge telemetry matches converged durable state"),
        ForgeCausalDiagnosis::Converged
    );
    assert!(after.data_file_count() < before.data_file_count());
    assert_eq!(rewrite.input_files.len(), planned_inputs.len());
    assert!(!rewrite.output_files.is_empty());
    assert!(rewrite.input_bytes > 0 && rewrite.output_bytes > 0);
    assert!(
        rewrite
            .output_files
            .iter()
            .all(|file| file.bytes <= after.maximum_healthy_file_bytes)
    );
    assert!(
        rewrite.output_files.len() == 1
            || rewrite
                .output_files
                .iter()
                .all(|file| file.bytes >= after.minimum_healthy_file_bytes)
    );
    assert_eq!(after.active_claims, 0);
    assert_eq!(after.active_attempts, 0);
    assert_eq!(
        server
            .state()
            .forge()
            .expect("Forge composition after rewrite")
            .resources()
            .snapshot()
            .expect("Forge resources after rewrite"),
        forge_baseline
    );
    assert_eq!(
        strict_spill_summary(&reader, &format!("vala.bifrost.{table}"))
            .await
            .expect("post-Forge ordered exact dataset"),
        (1_000_000, 256_000_000)
    );
    cluster
        .shutdown()
        .await
        .expect("Forge convergence shutdown");
}

/// Advances only a durably settled retry so the gated journey need not sleep through backoff.
///
/// The production worker has already consumed and classified the failed attempt;
/// this test-only clock step preserves that durable taxonomy while keeping the
/// convergence proof bounded.
///
/// # Panics
///
/// Panics when the test database cannot update the exact tenant-table retry row.
async fn advance_forge_retry_for_journey(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    table: &str,
) {
    let operator_pool = server
        .state()
        .postgres
        .operator_pool()
        .expect("Forge journey operator pool");
    let pool = operator_pool.pool();
    let mut transaction = pool.begin().await.expect("begin Forge retry clock step");
    let retries = sqlx::query_as::<_, (uuid::Uuid, Option<String>)>(
        "SELECT task_id,failure_class FROM vala.forge_tasks WHERE data_tenant_id=$1 AND table_name=$2 AND state='retryable' FOR UPDATE",
    )
    .bind(tenant.as_uuid())
    .bind(table)
    .fetch_all(&mut *transaction)
    .await
    .expect("load durable Forge retry classification");
    for (task_id, failure_class) in &retries {
        assert!(
            matches!(
                failure_class.as_deref(),
                Some("transient_object_store" | "transient_coordination" | "storage_health")
            ),
            "task {task_id} must have a permitted durable transient class before clock advancement; observed {failure_class:?}"
        );
    }
    let task_ids = retries
        .into_iter()
        .map(|(task_id, _)| task_id)
        .collect::<Vec<_>>();
    if !task_ids.is_empty() {
        let updated = sqlx::query(
            "UPDATE vala.forge_tasks SET ready_at=statement_timestamp(),next_eligible_at=statement_timestamp()-interval '15 minutes' WHERE task_id=ANY($1) AND state='retryable'",
        )
        .bind(&task_ids)
        .execute(&mut *transaction)
        .await
        .expect("advance durable Forge retry eligibility")
        .rows_affected();
        assert_eq!(
            updated,
            u64::try_from(task_ids.len()).expect("retry fixture count fits u64"),
            "locked retry set must remain exact through eligibility advancement"
        );
    }
    transaction
        .commit()
        .await
        .expect("commit Forge retry clock step");
}

/// A production disk-ceiling refusal remains a typed public 429, cleans all
/// partial ownership, and leaves a smaller durable query usable.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_spill_disk_ceiling_is_typed_and_recovers() {
    let cluster = WyrdTestCluster::start_spec(
        BifrostClusterSpec::one_mixed().with_system_resources(spill_system_resources(1)),
    )
    .await
    .expect("spill ceiling cluster");
    let server = cluster.server(0).expect("spill ceiling server");
    assert_spill_resource_plan(server, 1);
    let large = prepare_spill_table(&cluster, server, "oracle_spill_ceiling", 1_000_000)
        .await
        .expect("spill ceiling fixture");
    let small = prepare_spill_table(&cluster, server, "oracle_spill_recovery", 2)
        .await
        .expect("spill recovery fixture");
    let reader = client(server, "oracle-spill-ceiling-reader")
        .await
        .expect("spill ceiling reader");
    let baseline = server
        .oracle_runtime_inspection()
        .expect("spill ceiling baseline");
    let memory = server
        .state()
        .bifrost_resources()
        .expect("spill ceiling governor")
        .snapshot()
        .expect("spill ceiling resource snapshot");
    let memory_baseline = (managed_memory_used(memory), memory.oracle_memory_used_bytes);

    let refusal = match QueryClient::new(&reader)
        .query(&BifrostQueryRequest {
            sql: format!("SELECT id, value FROM vala.bifrost.{large} ORDER BY id"),
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: Some(30_000),
        })
        .await
    {
        Ok(_) => panic!("one-byte disk quota must refuse before a public stream opens"),
        Err(error) => error,
    };
    assert!(matches!(
        refusal,
        ValaSdkError::Transport(WyrdError::Vala {
            error: BifrostError::QueryAdmissionRejected
        })
    ));
    assert_oracle_runtime_restored(server, baseline, memory_baseline)
        .expect("disk refusal cleanup is exact");
    assert_eq!(
        strict_count_value(&reader, &format!("vala.bifrost.{small}"))
            .await
            .expect("smaller recovery COUNT succeeds"),
        2
    );
    assert_oracle_runtime_restored(server, baseline, memory_baseline)
        .expect("recovery cleanup is exact");
    cluster.shutdown().await.expect("spill ceiling shutdown");
}

/// Dropping a public response at the deterministic schema barrier cancels the
/// active spilling query and releases its scratch before the journey proceeds.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_spill_cancellation_cleans_query_scratch() {
    let cluster = WyrdTestCluster::start_spec(
        BifrostClusterSpec::one_mixed().with_system_resources(spill_system_resources(1 << 30)),
    )
    .await
    .expect("spill cancellation cluster");
    let server = cluster.server(0).expect("spill cancellation server");
    assert_spill_resource_plan(server, 1 << 30);
    let table = prepare_spill_table(&cluster, server, "oracle_spill_cancel", 1_000_000)
        .await
        .expect("spill cancellation fixture");
    let reader = client(server, "oracle-spill-cancel-reader")
        .await
        .expect("spill cancellation reader");
    let baseline = server
        .oracle_runtime_inspection()
        .expect("spill cancellation baseline");
    let memory = server
        .state()
        .bifrost_resources()
        .expect("spill cancellation governor")
        .snapshot()
        .expect("spill cancellation resource snapshot");
    let memory_baseline = (managed_memory_used(memory), memory.oracle_memory_used_bytes);
    let request = BifrostQueryRequest {
        sql: format!("SELECT id, value FROM vala.bifrost.{table} ORDER BY id"),
        visibility: VisibilityMode::PublishedOnly,
        freshness: FreshnessPolicy::Strict,
        deadline_ms: Some(30_000),
    };
    let query = QueryClient::new(&reader);
    let query_task = tokio::spawn(async move {
        query
            .collect_bounded(
                &request,
                CollectedQueryLimits {
                    max_rows: 1,
                    max_encoded_bytes: 1024,
                },
            )
            .await
    });
    let active = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let active = server
                .oracle_runtime_inspection()
                .expect("active spill inspection");
            if active.active_queries > baseline.active_queries
                && active.spill_files > baseline.spill_files
            {
                break active;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("spilling query did not expose owned scratch before cancellation"));
    assert_active_spill_query_resources(server, 1 << 30);
    assert!(active.active_queries > baseline.active_queries);
    assert!(active.spill_files > baseline.spill_files);
    query_task.abort();
    let _ = query_task.await;
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            if assert_oracle_runtime_restored(server, baseline, memory_baseline).is_ok() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("cancellation restores Oracle resources");
    assert_oracle_runtime_restored(server, baseline, memory_baseline)
        .expect("cancellation cleanup is exact");
    assert_eq!(
        strict_spill_summary(&reader, &format!("vala.bifrost.{table}"))
            .await
            .expect("durable rows remain readable after cancellation"),
        (1_000_000, 256_000_000)
    );
    cluster
        .shutdown()
        .await
        .expect("spill cancellation shutdown");
}

/// Losing the pod executing an active spill terminates the in-flight request,
/// permits an exact public retry after restart, and removes only owned residue.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_spill_pod_loss_isolated_and_restart_cleans() {
    let mut cluster = WyrdTestCluster::start_spec(
        BifrostClusterSpec::one_mixed().with_system_resources(spill_system_resources(1 << 30)),
    )
    .await
    .expect("spill pod-loss cluster");
    for server in cluster.servers() {
        assert_spill_resource_plan(server, 1 << 30);
    }
    let query_node = cluster.configured_node_ids()[0];
    let table = {
        let server = cluster
            .server_by_node(query_node)
            .expect("spill pod-loss fixture server");
        prepare_spill_table(&cluster, server, "oracle_spill_pod_loss", 1_000_000)
            .await
            .expect("spill pod-loss fixture")
    };
    let reader = client(
        cluster
            .server_by_node(query_node)
            .expect("spill pod-loss query server"),
        "oracle-spill-pod-loss-reader",
    )
    .await
    .expect("spill pod-loss reader");
    let query_table = format!("vala.bifrost.{table}");
    let request_lifetime = cluster
        .abrupt_request_lifetime(query_node)
        .expect("spill pod-loss request lifetime");
    let query = tokio::spawn(async move {
        tokio::select! {
            result = strict_spill_summary(&reader, &query_table) => result,
            () = request_lifetime.cancelled() => Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionAborted,
                "bound test process terminated during public query",
            ).into()),
        }
    });
    let executing = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            if query.is_finished() {
                panic!("spilling query terminated before exposing owned scratch");
            }
            if let Some(node_id) = cluster
                .configured_node_ids()
                .iter()
                .copied()
                .find(|node_id| {
                    cluster
                        .server_by_node(*node_id)
                        .and_then(|server| server.oracle_runtime_inspection().ok())
                        .is_some_and(|inspection| {
                            inspection.active_queries > 0 && inspection.spill_files > 0
                        })
                })
            {
                break node_id;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("runtime inspection identifies the executing Oracle");
    let roots = cluster
        .terminate_node_abruptly_for_test(executing)
        .await
        .expect("executing Oracle terminates abruptly");
    let terminal = tokio::time::timeout(std::time::Duration::from_secs(30), query)
        .await
        .expect("pod-loss spill query terminates")
        .expect("pod-loss spill task joins");
    assert!(
        terminal.is_err(),
        "abrupt pod loss must fail the unfinished query"
    );
    cluster
        .seed_oracle_spill_restart_fixture(executing)
        .expect("seed stopped-node crash residue");
    assert_eq!(
        cluster
            .oracle_spill_restart_fixture_state(executing)
            .expect("pre-restart residue state"),
        (true, true)
    );
    cluster
        .restart_terminated_node_at_new_address(executing, roots)
        .await
        .expect("terminated Oracle restarts from retained roots");
    cluster
        .refresh_oracle_snapshots()
        .await
        .expect("restarted Oracle membership refresh");
    assert_eq!(
        cluster
            .oracle_spill_restart_fixture_state(executing)
            .expect("post-restart residue state"),
        (false, true),
        "restart must remove only the owned stale prefix"
    );
    let restarted = cluster
        .server_by_node(executing)
        .expect("restarted Oracle server");
    let residual = restarted
        .oracle_runtime_inspection()
        .expect("restarted Oracle residuals");
    assert_eq!(residual.active_queries, 0);
    assert_eq!(residual.queued_queries, 0);
    assert_eq!(residual.reserved_memory_bytes, 0);
    assert_eq!(residual.reserved_spill_bytes, 0);
    assert_eq!(residual.peer_pending, 0);
    assert_eq!(residual.peer_running, 0);
    let memory = restarted
        .state()
        .bifrost_resources()
        .expect("restarted Oracle governor")
        .snapshot()
        .expect("restarted Oracle resource snapshot");
    assert_eq!(memory.oracle_memory_used_bytes, 0);
    assert_eq!(managed_memory_used(memory), 0);
    let restarted_reader = client(restarted, "oracle-spill-restarted-reader")
        .await
        .expect("restarted Oracle reader");
    assert_eq!(
        strict_spill_summary(&restarted_reader, &format!("vala.bifrost.{table}"))
            .await
            .expect("restarted Oracle reads durable rows"),
        (1_000_000, 256_000_000)
    );
    cluster.shutdown().await.expect("spill pod-loss shutdown");
}

/// Sustained Oracle admission refusals leave the independent durable heartbeat
/// advancing and ready, and the same public query succeeds after pressure ends.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn oracle_heartbeat_survives_capacity_refusals() {
    let mut spec = BifrostClusterSpec::one_mixed();
    spec.nodes[0].role_timing = Some(RoleTiming::deterministic_test());
    let cluster = WyrdTestCluster::start_spec(spec)
        .await
        .expect("heartbeat capacity cluster");
    let server = cluster.server(0).expect("heartbeat mixed server");
    let table = unique_table("oracle_heartbeat_capacity");
    register_table(server, cluster.data_tenant_id(), &table)
        .await
        .expect("heartbeat table");
    let writer = client(server, "heartbeat-capacity-writer")
        .await
        .expect("heartbeat writer");
    ingest(&writer, &format!("vala.bifrost.{table}"), &[1, 2])
        .await
        .expect("heartbeat fixture ingest");
    server
        .flush_bifrost()
        .await
        .expect("heartbeat fixture flush");
    let reader = client(server, "heartbeat-capacity-reader")
        .await
        .expect("heartbeat reader");
    let owner = cluster
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("heartbeat owner pool");
    let heartbeat_before: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT heartbeat_at FROM vala.cluster_nodes WHERE role = 'oracle'")
            .fetch_one(&owner)
            .await
            .expect("initial Oracle heartbeat");
    let governor = server
        .state()
        .bifrost_resources()
        .expect("heartbeat shared governor");
    let oracle = governor.oracle().expect("Oracle resource capability");
    let retained = oracle
        .try_acquire_query(vala_bifrost_redux::resources::OracleResourceRequest {
            query_class: QueryClass::Interactive,
            memory_bytes: vala_bifrost_redux::resources::ORACLE_PARTITION_MEMORY_BYTES,
            scratch_bytes: vala_bifrost_redux::resources::ORACLE_PARTITION_MEMORY_BYTES as u64,
            slot_units: 1,
            local_ratio: 1.0,
        })
        .expect("retain Oracle capacity during heartbeat proof");
    let pressure_deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(250);
    let mut refusals = 0_u64;
    while tokio::time::Instant::now() < pressure_deadline {
        let result = QueryClient::new(&reader)
            .query(&BifrostQueryRequest {
                sql: format!("SELECT COUNT(*) FROM vala.bifrost.{table}"),
                visibility: VisibilityMode::PublishedOnly,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(1_000),
            })
            .await;
        assert!(matches!(
            result,
            Err(ValaSdkError::Transport(WyrdError::Vala {
                error: BifrostError::QueryAdmissionRejected
            }))
        ));
        refusals = refusals.saturating_add(1);
        tokio::task::yield_now().await;
    }
    assert!(
        refusals >= 2,
        "pressure window must observe repeated typed 429s"
    );
    let (heartbeat_after, ready): (chrono::DateTime<Utc>, bool) =
        sqlx::query_as("SELECT heartbeat_at, ready FROM vala.cluster_nodes WHERE role = 'oracle'")
            .fetch_one(&owner)
            .await
            .expect("Oracle heartbeat after pressure");
    println!(
        "oracle heartbeat advanced under capacity pressure: before={heartbeat_before} after={heartbeat_after}"
    );
    assert!(heartbeat_after > heartbeat_before);
    assert!(
        ready,
        "Oracle readiness must remain advertised under pressure"
    );
    drop(retained);
    assert_eq!(
        query_rows(&reader, &table, VisibilityMode::PublishedOnly)
            .await
            .expect("strict query succeeds after heartbeat pressure"),
        2
    );
    cluster
        .shutdown()
        .await
        .expect("heartbeat cluster shutdown");
}

/// Three fresh production clusters sustain identical fixed-rate writes with
/// and without independent COUNT traffic while retaining exact offered load.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized long-running Postgres progress lane"]
async fn concurrent_ingest_and_count_preserve_progress() {
    let mut trials = Vec::with_capacity(3);
    for trial in 1_u64..=3 {
        let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed())
            .await
            .expect("paired progress cluster");
        let server = cluster.server(0).expect("paired mixed server");
        let table_name = unique_table("paired_progress");
        register_paired_table(server, cluster.data_tenant_id(), &table_name)
            .await
            .expect("paired progress table");
        let public = client(server, &format!("paired-progress-{trial}"))
            .await
            .expect("paired progress public client");
        let writer = BifrostGrpcTransport::connect(&public)
            .await
            .expect("paired progress writer transport");
        let engine =
            PairedProgressEngine::new(writer, public, format!("vala.bifrost.{table_name}"))
                .expect("locked paired pacers");
        let warmup_writes = FixedRatePacer::new(50, std::time::Duration::from_secs(10), 32)
            .expect("baseline warmup pacer");
        engine
            .run_write_phase_with(trial * 10, warmup_writes)
            .await
            .expect("baseline warmup");
        let baseline = engine
            .run_write_phase(trial * 10 + 1)
            .await
            .expect("baseline measured phase");
        let baseline_visibility_started = std::time::Instant::now();
        server
            .flush_bifrost()
            .await
            .expect("baseline visibility flush");
        assert_eq!(
            strict_count_value(&engine.reader, &engine.table)
                .await
                .expect("baseline strict visibility COUNT"),
            128_000
        );
        let baseline_visibility_ms =
            u64::try_from(baseline_visibility_started.elapsed().as_millis()).unwrap_or(u64::MAX);
        assert!(
            baseline_visibility_ms <= 30_000,
            "baseline strict visibility exceeded 30s"
        );
        let warmup_queries = FixedRatePacer::new(25, std::time::Duration::from_secs(10), 16)
            .expect("overlap warmup query pacer");
        tokio::try_join!(
            engine.run_write_phase_with(trial * 10 + 2, warmup_writes),
            engine.run_query_phase_with(warmup_queries),
        )
        .expect("overlap warmup");
        let sampler_stop = CancellationToken::new();
        let measured = async {
            let result = engine.run_overlap_phase(trial * 10 + 3).await;
            sampler_stop.cancel();
            result
        };
        let (measured, peaks) =
            tokio::join!(measured, sample_paired_peaks(server, sampler_stop.clone()));
        let (overlap, queries, progress_windows) = measured.expect("overlap measured phase");
        let peaks = peaks.expect("overlap peak sampler");
        let visibility_started = std::time::Instant::now();
        server
            .flush_bifrost()
            .await
            .expect("overlap visibility flush");
        assert_eq!(
            strict_count_value(&engine.reader, &engine.table)
                .await
                .expect("strict final COUNT"),
            256_000
        );
        let overlap_visibility_ms =
            u64::try_from(visibility_started.elapsed().as_millis()).unwrap_or(u64::MAX);
        assert!(
            overlap_visibility_ms <= 30_000,
            "overlap strict visibility exceeded 30s"
        );
        let visibility_ms = baseline_visibility_ms.max(overlap_visibility_ms);
        let ratio = overlap.accepted_rows as f64 / baseline.accepted_rows as f64;
        assert!(ratio.is_finite());
        trials.push(PairedTrialResult {
            trial,
            baseline_writes: baseline,
            overlap_writes: overlap,
            overlap_queries: queries,
            ratio,
            visibility_ms,
            progress_windows,
            peaks,
        });
        cluster.shutdown().await.expect("paired cluster shutdown");
    }
    let mut ratios = trials.iter().map(|trial| trial.ratio).collect::<Vec<_>>();
    ratios.sort_by(f64::total_cmp);
    let median_ratio = ratios[1];
    let ratio_range = ratios[2] - ratios[0];
    let mut deviations = ratios
        .iter()
        .map(|ratio| (ratio - median_ratio).abs())
        .collect::<Vec<_>>();
    deviations.sort_by(f64::total_cmp);
    let ratio_mad = deviations[1];
    assert!(median_ratio >= 0.90, "median ingest ratio below 0.90");
    let report = PairedProgressReport {
        trials,
        median_ratio,
        ratio_range,
        ratio_mad,
    };
    let report_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("wyrd-testing manifest is three levels below the workspace")
        .join("target/bifrost-benchmarks/capacity-refusal/concurrent-progress.json");
    tokio::fs::create_dir_all(
        report_path
            .parent()
            .expect("concurrent progress report has a parent"),
    )
    .await
    .expect("concurrent progress report directory");
    tokio::fs::write(
        &report_path,
        serde_json::to_vec_pretty(&report).expect("serialize concurrent progress report"),
    )
    .await
    .expect("write concurrent progress report");
}

/// J2 proves a Fused query drains a live tonic tail without a Scribe flush.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_fused_reconcile_journey() {
    public_roundtrip(
        BifrostClusterSpec::one_mixed(),
        VisibilityMode::Fused,
        false,
        0,
        Some("bifrost_oracle_tail_pages_total"),
        false,
    )
    .await
    .expect("J2 Fused journey");
}

/// J3 proves ingest-only gRPC write and query-only HTTP/Arrow read routing.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_role_separated_journey() {
    public_roundtrip(
        BifrostClusterSpec::role_separated(),
        VisibilityMode::Fused,
        false,
        2,
        Some("bifrost_oracle_tail_pages_total"),
        false,
    )
    .await
    .expect("J3 role-separated journey");
}

/// Proves a two-Server write/query journey across query-scoped Scribe discovery.
///
/// Server A owns public writes and sealing; Server B performs strict and fused
/// reads through its Oracle/Scribe-tail path. The journey also checks tenant
/// isolation, transport outage fail-closed behavior, replacement after a
/// writer restart, post-seal tail visibility, and observation-only telemetry.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_two_server_scribe_tail_boundary_journey() {
    let mut cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::two_mixed())
        .await
        .expect("two-server boundary cluster");
    let writer_node = cluster
        .configured_node_ids()
        .first()
        .copied()
        .expect("writer node identity");
    let reader_node = cluster
        .configured_node_ids()
        .get(1)
        .copied()
        .expect("reader node identity");
    let table = unique_table("oracle_two_server_tail");
    {
        let writer = cluster
            .server_by_node(writer_node)
            .expect("writer Server A");
        register_table(writer, cluster.data_tenant_id(), &table)
            .await
            .expect("cross-server table registration");
        let writer_client = client(writer, "two-server-writer")
            .await
            .expect("writer client");
        ingest(&writer_client, &format!("vala.bifrost.{table}"), &[1, 2])
            .await
            .expect("public write on Server A");
    }
    // Exercise the supported server lifecycle flush instead of reaching into
    // the test-only Scribe handle; Server B must see these rows immediately.
    cluster
        .stop_node(writer_node)
        .await
        .expect("Server A shutdown flush");
    cluster
        .restart_node(writer_node)
        .await
        .expect("Server A restart after shutdown flush");
    cluster
        .refresh_oracle_snapshots()
        .await
        .expect("post-flush writer snapshot");
    let writer_client = client(
        cluster
            .server_by_node(writer_node)
            .expect("restarted writer Server A"),
        "two-server-writer-restarted",
    )
    .await
    .expect("restarted writer client");
    let reader = cluster
        .server_by_node(reader_node)
        .expect("reader Server B");
    let reader_client = client(reader, "two-server-reader")
        .await
        .expect("reader client");
    assert_eq!(
        query_rows(&reader_client, &table, VisibilityMode::PublishedOnly)
            .await
            .expect("strict query through Server B"),
        2
    );

    ingest(&writer_client, &format!("vala.bifrost.{table}"), &[3])
        .await
        .expect("post-seal write on Server A");
    let event_day =
        EventDay::new(chrono::Utc::now().format("%Y-%m-%d").to_string()).expect("event day");
    cluster
        .observe_live_tail(&format!("vala.bifrost.{table}"), event_day.clone())
        .await
        .expect("observation-only live-tail discovery");
    assert_eq!(
        query_rows(&reader_client, &table, VisibilityMode::Fused)
            .await
            .expect("fused tail query through Server B"),
        3
    );

    let foreign_tenant = cluster
        .add_tenant("oracle-two-server-foreign")
        .await
        .expect("foreign tenant");
    let foreign_client = client_for_tenant(
        cluster
            .server_by_node(reader_node)
            .expect("reader remains available"),
        foreign_tenant,
        "two-server-foreign-reader",
    )
    .await
    .expect("foreign reader client");
    let owner = cluster
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("two-server audit owner pool");
    let foreign_audit_before: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM vala.audit_outbox WHERE data_tenant_id = $1 AND operation LIKE 'bifrost.query.%'",
    )
    .bind(foreign_tenant.as_uuid())
    .fetch_one(&owner)
    .await
    .expect("foreign audit baseline");
    let system_audit_before: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM vala.audit_outbox WHERE data_tenant_id = $1 AND operation LIKE 'bifrost.query.%'",
    )
    .bind(DataTenantId::SYSTEM_OWNER.as_uuid())
    .fetch_one(&owner)
    .await
    .expect("system audit baseline");
    let cross_tenant_checkpoint = cluster
        .telemetry()
        .checkpoint()
        .expect("cross-tenant telemetry checkpoint");
    assert!(
        query_rows(&foreign_client, &table, VisibilityMode::Fused)
            .await
            .is_err(),
        "cross-tenant query must fail closed before tail access"
    );
    let foreign_audit_after: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM vala.audit_outbox WHERE data_tenant_id = $1 AND operation LIKE 'bifrost.query.%'",
    )
    .bind(foreign_tenant.as_uuid())
    .fetch_one(&owner)
    .await
    .expect("foreign audit attribution");
    let system_audit_after: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM vala.audit_outbox WHERE data_tenant_id = $1 AND operation LIKE 'bifrost.query.%'",
    )
    .bind(DataTenantId::SYSTEM_OWNER.as_uuid())
    .fetch_one(&owner)
    .await
    .expect("system audit attribution");
    // The public tenant authorization boundary rejects this unregistered
    // table before Oracle planning, so it emits no Oracle read/security audit.
    // Verified-tenant security audit attribution is covered by the private
    // tonic authority journey; this path proves the public denial does not
    // fall through to the system chain.
    assert_eq!(foreign_audit_after, foreign_audit_before);
    assert_eq!(system_audit_after, system_audit_before);
    let cross_tenant_delta = cluster
        .telemetry()
        .delta_since(&cross_tenant_checkpoint)
        .expect("cross-tenant telemetry delta");
    assert!(
        cross_tenant_delta.metrics.iter().all(|sample| {
            (!(sample.family == "bifrost_oracle_tail_pages_total"
                || sample.family == "bifrost_oracle_tail_fences_total"))
                || sample.value <= 0.0
        }),
        "cross-tenant denial must preserve tail capacity before private access"
    );

    reader.set_tail_discovery_unavailable_for_test(true);
    assert!(
        query_rows(&reader_client, &table, VisibilityMode::Fused)
            .await
            .is_err(),
        "private Scribe discovery/auth outage must fail closed before first batch"
    );
    reader.set_tail_discovery_unavailable_for_test(false);
    assert_eq!(
        query_rows(&reader_client, &table, VisibilityMode::Fused)
            .await
            .expect("private Scribe discovery recovery"),
        3
    );

    let prior_stream = cluster
        .server_by_node(writer_node)
        .expect("prior writer")
        .state()
        .bifrost_tail_reader_for_test()
        .expect("prior Scribe tail reader")
        .stream_identity();
    cluster
        .stop_node(writer_node)
        .await
        .expect("writer restart stop");
    cluster
        .restart_node_at_new_address(writer_node)
        .await
        .expect("writer restart replacement");
    cluster
        .refresh_oracle_snapshots()
        .await
        .expect("writer replacement snapshot");
    let replacement_writer = cluster
        .server_by_node(writer_node)
        .expect("replacement writer");
    let replacement_stream = replacement_writer
        .state()
        .bifrost_tail_reader_for_test()
        .expect("replacement Scribe tail reader")
        .stream_identity();
    assert_eq!(replacement_stream.node_id, prior_stream.node_id);
    assert!(
        replacement_stream.writer_epoch > prior_stream.writer_epoch,
        "replacement Scribe must reject the stale prior writer epoch"
    );
    let replacement_writer_client = client(replacement_writer, "two-server-replacement-writer")
        .await
        .expect("replacement writer client");
    ingest(
        &replacement_writer_client,
        &format!("vala.bifrost.{table}"),
        &[4, 5],
    )
    .await
    .expect("write after stale epoch replacement");
    cluster
        .observe_live_tail(&format!("vala.bifrost.{table}"), event_day)
        .await
        .expect("replacement live-tail discovery");
    let replacement_reader = cluster
        .server_by_node(reader_node)
        .expect("replacement reader");
    let replacement_reader_client = client(replacement_reader, "two-server-replacement-reader")
        .await
        .expect("replacement reader client");
    assert_eq!(
        query_rows(&replacement_reader_client, &table, VisibilityMode::Fused,)
            .await
            .expect("strict query succeeds after stale epoch replacement refresh"),
        5
    );

    let samples = cluster
        .telemetry()
        .snapshot()
        .expect("two-server observation telemetry");
    assert!(
        samples.iter().any(|sample| {
            sample.family == "bifrost_oracle_tail_pages_total" && sample.value > 0.0
        }),
        "tail query emitted no production observation"
    );
    cluster.shutdown().await.expect("two-server shutdown");
}

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
    let day = EventDay::new(chrono::Utc::now().format("%Y-%m-%d").to_string()).expect("event day");
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
             row groups selective={selective_row_groups} unfiltered={unfiltered_row_groups}"
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

/// Sums every metric sample matching one production family across all labels.
fn sum_metric(
    delta: &wyrd_testing::bifrost::telemetry::BifrostTelemetryDelta,
    family: &str,
) -> f64 {
    delta
        .metrics
        .iter()
        .filter(|sample| sample.family == family)
        .map(|sample| sample.value)
        .sum()
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

/// Proves public gRPC query frames match the HTTP stream for one seeded table.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn public_grpc_matches_http_frames() {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed())
        .await
        .expect("gRPC parity cluster");
    let server = cluster.server(0).expect("gRPC parity server");
    let table = unique_table("oracle_grpc_parity");
    register_table(server, cluster.data_tenant_id(), &table)
        .await
        .expect("register parity table");
    let client = client(server, "grpc-parity").await.expect("parity client");
    ingest(&client, &format!("vala.bifrost.{table}"), &[11, 22])
        .await
        .expect("parity ingest");
    server.flush_bifrost().await.expect("parity flush");
    let request = BifrostQueryRequest {
        sql: format!("SELECT id, value FROM vala.bifrost.{table} ORDER BY id"),
        visibility: VisibilityMode::PublishedOnly,
        freshness: FreshnessPolicy::Strict,
        deadline_ms: None,
    };
    let http = http_query_frames(server.base_url().expect("HTTP URL"), &client, &request)
        .await
        .expect("HTTP frames");
    let grpc = grpc_query_frames(&client, &request)
        .await
        .expect("gRPC frames");
    assert_eq!(http, grpc, "HTTP and gRPC logical frames must be identical");
    let terminal = http
        .iter()
        .find_map(|frame| match frame.frame.as_ref() {
            Some(proto::query_stream_frame::Frame::Terminal(terminal)) => Some(terminal),
            _ => None,
        })
        .expect("parity terminal");
    assert_eq!(terminal.row_count, 2);
    assert_eq!(
        terminal.outcome,
        proto::QueryTerminalOutcome::Success as i32
    );
    assert_eq!(terminal.source_completion.len(), 2);
    cluster.shutdown().await.expect("parity shutdown");
}

/// Rejects the retired component-partial process topology.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn public_grpc_without_oracle_is_unavailable() {
    let mut spec = BifrostClusterSpec::one_mixed();
    spec.nodes = vec![wyrd_testing::bifrost::BifrostNodeSpec {
        node_id: wyrd_spec::vala::api::NodeId::new(uuid::Uuid::now_v7()),
        roles: [
            BifrostRuntimeRole::Scribe,
            BifrostRuntimeRole::ForgeCoordinator,
        ]
        .into_iter()
        .collect(),
        oracle: None,
        role_timing: None,
    }];
    assert!(WyrdTestCluster::start_spec(spec).await.is_err());
}

/// Proves production shutdown drains a dropped stream's queued durable release.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn public_grpc_drop_releases_query_resources() {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed())
        .await
        .expect("drop cluster");
    let server = cluster.server(0).expect("drop server");
    let table = unique_table("oracle_grpc_drop");
    register_table(server, cluster.data_tenant_id(), &table)
        .await
        .expect("register drop table");
    let client = client(server, "grpc-drop").await.expect("drop client");
    ingest(&client, &format!("vala.bifrost.{table}"), &[1, 2])
        .await
        .expect("drop ingest");
    server.flush_bifrost().await.expect("drop flush");
    let request = BifrostQueryRequest {
        sql: format!("SELECT id, value FROM vala.bifrost.{table} ORDER BY id"),
        visibility: VisibilityMode::PublishedOnly,
        freshness: FreshnessPolicy::Strict,
        deadline_ms: None,
    };
    server.stall_next_query_after_schema();
    let query = QueryClient::new(&client);
    let query_task = tokio::spawn(async move {
        query
            .collect_bounded(
                &request,
                CollectedQueryLimits {
                    max_rows: 16,
                    max_encoded_bytes: 1024 * 1024,
                },
            )
            .await
    });
    let _query_id = server
        .wait_query_schema_stall()
        .await
        .expect("query reaches schema stall");
    query_task.abort();
    let _ = query_task.await;
    let _ = cluster.shutdown_and_inspect().await.expect("drop shutdown");
}

/// Proves tenant isolation, local fairness, durable audit, and audit refusal.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_multitenant_isolation_and_fairness_journey() {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed())
        .await
        .expect("isolation journey cluster");
    let server = cluster.server(0).expect("isolation journey server");
    let tenant_a = cluster.data_tenant_id();
    let tenant_b = cluster.add_tenant("oracle-j5-b").await.expect("tenant B");
    let table_name = unique_table("oracle_j5");
    for tenant in [tenant_a, tenant_b] {
        register_table(server, tenant, &table_name)
            .await
            .expect("tenant table registration");
        let client = client_for_tenant(server, tenant, &format!("j5-{tenant}"))
            .await
            .expect("tenant client");
        ingest(
            &client,
            &format!("vala.bifrost.{table_name}"),
            &[tenant_marker(tenant)],
        )
        .await
        .expect("tenant ingest");
        server
            .flush_bifrost_for_tenant(tenant)
            .await
            .expect("tenant flush");
        assert_eq!(
            query_rows(&client, &table_name, VisibilityMode::PublishedOnly)
                .await
                .expect("tenant query"),
            1
        );
    }
    seed_foreign_hot_row(&cluster, tenant_a, &table_name, tenant_b, "j5-foreign")
        .await
        .expect("foreign physical row");
    let tenant_a_client = client_for_tenant(server, tenant_a, "j5-tripwire")
        .await
        .expect("tripwire client");
    let table_fqn = format!("vala.bifrost.{table_name}");
    for sql in [
        format!("SELECT count(*) AS total FROM {table_fqn}"),
        format!("SELECT a.id FROM {table_fqn} a JOIN {table_fqn} b ON a.id = b.id"),
    ] {
        let (rows, outcome, error) = query_statement(&tenant_a_client, sql)
            .await
            .expect("tripwire terminal");
        assert_eq!(rows, 0, "foreign row reached a SQL operator");
        assert_eq!(outcome, QueryTerminalOutcome::Failed);
        assert_eq!(error, Some(QueryTerminalErrorCode::QueryTenantInvariant));
    }
    let owner = cluster
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("isolation journey table-owner pool");
    let audit_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM vala.audit_outbox \
             WHERE data_tenant_id = $1 \
               AND operation = 'bifrost.query.security_violation'",
        )
        .bind(tenant_a.as_uuid())
        .fetch_one(&owner)
        .await
        .expect("security audit count");
        if count >= 2 {
            break;
        }
        assert!(
            tokio::time::Instant::now() < audit_deadline,
            "security audit relay did not drain"
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let security_details: Vec<String> = sqlx::query_scalar(
        "SELECT detail::text FROM vala.audit_outbox \
         WHERE data_tenant_id = $1 \
           AND operation = 'bifrost.query.security_violation' \
         ORDER BY seq",
    )
    .bind(tenant_a.as_uuid())
    .fetch_all(&owner)
    .await
    .expect("security audit rows");
    assert!(
        security_details.len() >= 2,
        "COUNT and JOIN each require a durable security audit"
    );
    assert!(security_details.iter().all(|detail| {
        detail.contains("\"violation\":\"tenant_row\"") && detail.contains("\"phase\":\"source\"")
    }));

    let inspection = cluster
        .oracle_inspection()
        .await
        .expect("isolation journey inspection");
    assert!(inspection.audit_rows >= 4);
    assert_eq!(inspection.active_queries, 0);
    assert_eq!(inspection.queued_queries, 0);
    assert_eq!(inspection.reserved_memory_bytes, 0);
    assert_eq!(inspection.reserved_spill_bytes, 0);
    assert_eq!(inspection.peer_pending, 0);
    assert_eq!(inspection.peer_running, 0);
    drop(owner);
    cluster
        .shutdown()
        .await
        .expect("isolation journey shutdown");
}

/// J6 proves durable-before-read acceptance, bounded relay backlog, and replay windows.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_audit_relay_journey() {
    let mut cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed())
        .await
        .expect("J6 cluster");
    let server = cluster.server(0).expect("J6 server");
    let node_id = server.node_id();
    let table = unique_table("oracle_audit_relay");
    register_table(server, cluster.data_tenant_id(), &table)
        .await
        .expect("J6 table");
    let primary_bootstrap = server
        .bootstrap_service_in_tenant(cluster.data_tenant_id(), "oracle-audit-relay", &["admin"])
        .await
        .expect("J6 primary bootstrap");
    let primary_principal = primary_bootstrap.id().as_uuid();
    let query_client = client_from_bootstrap(server, primary_bootstrap)
        .await
        .expect("J6 client");
    let secondary_bootstrap = server
        .bootstrap_service_in_tenant(
            cluster.data_tenant_id(),
            "oracle-audit-relay-second",
            &["admin"],
        )
        .await
        .expect("J6 secondary bootstrap");
    let secondary_principal = secondary_bootstrap.id().as_uuid();
    let secondary_client = client_from_bootstrap(server, secondary_bootstrap)
        .await
        .expect("J6 second client");
    ingest(&query_client, &format!("vala.bifrost.{table}"), &[1])
        .await
        .expect("J6 ingest");
    server.flush_bifrost().await.expect("J6 flush");
    let pause = server.pause_audit_relay_for_test().expect("J6 pause relay");
    let audit_pool = cluster
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("J6 audit owner");
    let primary_boundary: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(seq), 0) FROM vala.audit_outbox WHERE data_tenant_id = $1",
    )
    .bind(cluster.data_tenant_id().as_uuid())
    .fetch_one(&audit_pool)
    .await
    .expect("J6 primary sequence boundary");
    assert_eq!(
        query_rows(&query_client, &table, VisibilityMode::PublishedOnly)
            .await
            .expect("J6 query"),
        1
    );
    let secondary_boundary: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(seq), 0) FROM vala.audit_outbox WHERE data_tenant_id = $1",
    )
    .bind(cluster.data_tenant_id().as_uuid())
    .fetch_one(&audit_pool)
    .await
    .expect("J6 secondary sequence boundary");
    assert_eq!(
        query_rows(&secondary_client, &table, VisibilityMode::PublishedOnly,)
            .await
            .expect("J6 second query"),
        1
    );
    let blocked = server.oracle_runtime_inspection().expect("J6 inspection");
    assert!(
        blocked.audit_wal_records >= 1,
        "accepted audit must be durable before relay"
    );
    drop(pause);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if server
            .oracle_runtime_inspection()
            .expect("J6 relay inspection")
            .audit_wal_records
            == 0
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        assert!(
            tokio::time::Instant::now() < deadline,
            "relay did not drain"
        );
    }
    let primary_rows: Vec<(uuid::Uuid, String, String, uuid::Uuid, String)> = sqlx::query_as(
        "SELECT data_tenant_id, request_id, resource, principal_id, operation FROM vala.audit_outbox \
         WHERE data_tenant_id = $1 AND resource = $2 AND principal_id = $3 \
           AND operation = $4 AND seq > $5 ORDER BY seq",
    )
    .bind(cluster.data_tenant_id().as_uuid())
    .bind("bifrost.query")
    .bind(primary_principal)
    .bind("bifrost.query.read_decision")
    .bind(primary_boundary)
    .fetch_all(&audit_pool)
    .await
    .expect("J6 primary correlated audit row");
    let secondary_rows: Vec<(uuid::Uuid, String, String, uuid::Uuid, String)> = sqlx::query_as(
        "SELECT data_tenant_id, request_id, resource, principal_id, operation FROM vala.audit_outbox \
         WHERE data_tenant_id = $1 AND resource = $2 AND principal_id = $3 \
           AND operation = $4 AND seq > $5 ORDER BY seq",
    )
    .bind(cluster.data_tenant_id().as_uuid())
    .bind("bifrost.query")
    .bind(secondary_principal)
    .bind("bifrost.query.read_decision")
    .bind(secondary_boundary)
    .fetch_all(&audit_pool)
    .await
    .expect("J6 secondary correlated audit row");
    assert_eq!(primary_rows.len(), 1);
    assert_eq!(secondary_rows.len(), 1);
    assert_ne!(primary_rows[0].1, secondary_rows[0].1);
    let primary = &primary_rows[0];
    let primary_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM vala.audit_outbox \
         WHERE data_tenant_id = $1 AND request_id = $2 AND resource = $3 \
           AND principal_id = $4 AND operation = $5",
    )
    .bind(primary.0)
    .bind(&primary.1)
    .bind(&primary.2)
    .bind(primary.3)
    .bind(&primary.4)
    .fetch_one(
        &cluster
            .pg_fixture()
            .superuser_pool()
            .await
            .expect("J6 exact tuple owner"),
    )
    .await
    .expect("J6 exact tuple count");
    assert_eq!(primary_count, 1);
    let roots = cluster
        .terminate_node_abruptly_for_test(node_id)
        .await
        .expect("J6 terminate");
    cluster
        .restart_terminated_node_at_new_address(node_id, roots)
        .await
        .expect("J6 restart");
    let restarted = cluster.server(0).expect("J6 restarted");
    let current: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM vala.audit_outbox \
         WHERE data_tenant_id = $1 AND request_id = $2 AND resource = $3 \
           AND principal_id = $4 AND operation = $5",
    )
    .bind(primary.0)
    .bind(&primary.1)
    .bind(&primary.2)
    .bind(primary.3)
    .bind(&primary.4)
    .fetch_one(
        &cluster
            .pg_fixture()
            .superuser_pool()
            .await
            .expect("J6 owner restart"),
    )
    .await
    .expect("J6 restart count");
    assert_eq!(current, 1, "checkpointed relay must not replay on restart");
    let replay_bootstrap = restarted
        .bootstrap_service_in_tenant(cluster.data_tenant_id(), "oracle-audit-replay", &["admin"])
        .await
        .expect("J6 replay bootstrap");
    let replay_principal = replay_bootstrap.id().as_uuid();
    let replay_client = client_from_bootstrap(restarted, replay_bootstrap)
        .await
        .expect("J6 replay client");
    let replay_boundary: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(seq), 0) FROM vala.audit_outbox WHERE data_tenant_id = $1",
    )
    .bind(cluster.data_tenant_id().as_uuid())
    .fetch_one(&audit_pool)
    .await
    .expect("J6 replay sequence boundary");
    restarted
        .fail_audit_after_commit_for_test()
        .expect("J6 crash seam");
    assert_eq!(
        query_rows(&replay_client, &table, VisibilityMode::PublishedOnly)
            .await
            .expect("J6 replay query"),
        1
    );
    let replay_owner = cluster
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("J6 replay owner");
    let replay_tuple: (uuid::Uuid, String, String, uuid::Uuid, String) = loop {
        let tuples: Vec<(uuid::Uuid, String, String, uuid::Uuid, String)> = sqlx::query_as(
            "SELECT data_tenant_id, request_id, resource, principal_id, operation \
             FROM vala.audit_outbox WHERE data_tenant_id = $1 AND resource = $2 \
               AND principal_id = $3 AND operation = $4 AND seq > $5 ORDER BY seq",
        )
        .bind(cluster.data_tenant_id().as_uuid())
        .bind("bifrost.query")
        .bind(replay_principal)
        .bind("bifrost.query.read_decision")
        .bind(replay_boundary)
        .fetch_all(&replay_owner)
        .await
        .expect("J6 replay tuple");
        if !tuples.is_empty() {
            assert_eq!(tuples.len(), 1, "J6 replay initial tuple must be unique");
            break tuples.into_iter().next().expect("J6 replay tuple exists");
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    };
    let commit_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let committed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM vala.audit_outbox \
             WHERE data_tenant_id = $1 AND request_id = $2 AND resource = $3 \
               AND principal_id = $4 AND operation = $5",
        )
        .bind(replay_tuple.0)
        .bind(&replay_tuple.1)
        .bind(&replay_tuple.2)
        .bind(replay_tuple.3)
        .bind(&replay_tuple.4)
        .fetch_one(&replay_owner)
        .await
        .expect("J6 committed replay count");
        let inspection = restarted
            .oracle_runtime_inspection()
            .expect("J6 replay inspection");
        if committed == 1 && inspection.audit_wal_records >= 1 {
            break;
        }
        assert!(
            tokio::time::Instant::now() < commit_deadline,
            "commit-before-checkpoint seam did not expose a durable uncheckpointed record"
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let replay_roots = cluster
        .terminate_node_abruptly_for_test(node_id)
        .await
        .expect("J6 replay terminate");
    cluster
        .restart_terminated_node_at_new_address(node_id, replay_roots)
        .await
        .expect("J6 replay restart");
    let replay_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let replayed: i64 = loop {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM vala.audit_outbox \
             WHERE data_tenant_id = $1 AND request_id = $2 AND resource = $3 \
               AND principal_id = $4 AND operation = $5",
        )
        .bind(replay_tuple.0)
        .bind(&replay_tuple.1)
        .bind(&replay_tuple.2)
        .bind(replay_tuple.3)
        .bind(&replay_tuple.4)
        .fetch_one(&replay_owner)
        .await
        .expect("J6 replay count");
        if count >= 2 || tokio::time::Instant::now() >= replay_deadline {
            break count;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    };
    assert_eq!(replayed, 2, "commit-before-checkpoint is at-least-once");
    let chain: Vec<(i64, Vec<u8>, Vec<u8>)> = sqlx::query_as(
        "SELECT seq, prev_hash, entry_hash FROM vala.audit_outbox \
         WHERE data_tenant_id = $1 AND request_id = $2 AND resource = $3 \
           AND principal_id = $4 AND operation = $5 ORDER BY seq DESC LIMIT 2",
    )
    .bind(replay_tuple.0)
    .bind(&replay_tuple.1)
    .bind(&replay_tuple.2)
    .bind(replay_tuple.3)
    .bind(&replay_tuple.4)
    .fetch_all(
        &cluster
            .pg_fixture()
            .superuser_pool()
            .await
            .expect("J6 chain owner"),
    )
    .await
    .expect("J6 chain rows");
    assert_eq!(chain.len(), 2);
    assert_eq!(chain[0].0, chain[1].0 + 1);
    assert_eq!(chain[0].1, chain[1].2);
    cluster.shutdown().await.expect("J6 shutdown");
}

/// J7 proves stale replan, audit refusal, stale fence, SDK terminal rejection, and recovery.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_recovery_terminal_journey() {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed())
        .await
        .expect("J7 cluster");
    let server = cluster.server(0).expect("J7 server");
    let table = unique_table("oracle_j7");
    register_table(server, cluster.data_tenant_id(), &table)
        .await
        .expect("J7 table");
    let client = client(server, "oracle-j7").await.expect("J7 client");
    let missing_path = seed_missing_hot_row(&cluster, cluster.data_tenant_id(), &table)
        .await
        .expect("J7 missing hot row");
    let stale = QueryClient::new(&client)
        .query(&BifrostQueryRequest {
            sql: format!("SELECT * FROM vala.bifrost.{table}"),
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: None,
        })
        .await;
    assert!(
        stale.is_err(),
        "stale source must fail before a public batch"
    );
    let owner = cluster
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("J7 owner pool");
    let audit_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let retry_details: Vec<String> = loop {
        let details: Vec<String> = sqlx::query_scalar(
            "SELECT detail::text FROM vala.audit_outbox \
             WHERE data_tenant_id = $1 \
               AND operation = 'bifrost.query.read_decision' \
             ORDER BY seq",
        )
        .bind(cluster.data_tenant_id().as_uuid())
        .fetch_all(&owner)
        .await
        .expect("J7 retry audits");
        if details.len() >= 2 || tokio::time::Instant::now() >= audit_deadline {
            break details;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    };
    assert_eq!(retry_details.len(), 2);
    assert!(retry_details[0].contains("\"retry_ordinal\":0"));
    assert!(retry_details[1].contains("\"retry_ordinal\":1"));
    sqlx::query("DELETE FROM vala.file_list WHERE file_path = $1")
        .bind(&missing_path)
        .execute(&owner)
        .await
        .expect("remove missing manifest row");

    prove_sdk_missing_terminal_rejected().await;
    ingest(&client, &format!("vala.bifrost.{table}"), &[7])
        .await
        .expect("J7 ingest");
    server.flush_bifrost().await.expect("J7 flush");
    assert_eq!(
        query_rows(&client, &table, VisibilityMode::PublishedOnly)
            .await
            .expect("J7 recovered query"),
        1
    );
    let residual = cluster
        .oracle_inspection()
        .await
        .expect("J7 residual inspection");
    assert_eq!(residual.active_queries, 0);
    assert_eq!(residual.queued_queries, 0);
    assert_eq!(residual.reserved_memory_bytes, 0);
    assert_eq!(residual.reserved_spill_bytes, 0);
    assert_eq!(residual.peer_pending, 0);
    assert_eq!(residual.peer_running, 0);
    drop(owner);
    cluster.shutdown().await.expect("J7 shutdown");
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

/// J-typed proves Vala's typed route enters the same Oracle cut as SQL.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn typed_vala_route_uses_oracle_cut() {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed())
        .await
        .expect("start Oracle journey cluster");
    let server = cluster.server(0).expect("query server");
    let reader = client(server, "typed-route-reader")
        .await
        .expect("typed-route client");
    let connection = reader.connect_grpc().await.expect("gRPC endpoint connects");
    let token = connection
        .auth()
        .bearer()
        .await
        .expect("reader bearer")
        .expose()
        .to_owned();
    let channel = connection.channel();
    let trace = typed_route_trace();
    let mut otlp = TraceServiceClient::new(channel.clone());
    otlp.export(with_access_token(Request::new(trace.clone()), &token))
        .await
        .expect("sealed typed-route trace export");
    server.flush_bifrost().await.expect("flush sealed trace");
    otlp.export(with_access_token(Request::new(trace), &token))
        .await
        .expect("hot typed-route trace export");
    let mut typed = ValaQueryServiceClient::new(channel);
    let response = typed
        .query_traces(with_access_token(
            Request::new(QueryTracesRequest {
                window: Some(QueryWindow {
                    since: String::new(),
                    until: String::new(),
                    limit: 100,
                    page_token: String::new(),
                }),
                service: "typed-route".to_owned(),
                min_duration_ms: 0,
                status: String::new(),
                name: "typed-cut-span".to_owned(),
            }),
            &token,
        ))
        .await
        .expect("typed Vala route succeeds");
    let typed_row = response
        .into_inner()
        .rows
        .into_iter()
        .next()
        .expect("typed route returns one trace summary");
    assert_eq!(typed_row.trace_id, "11".repeat(16));
    assert_eq!(typed_row.root_name, "typed-cut-span");
    assert_eq!(typed_row.service, "typed-route");
    assert_eq!(typed_row.span_count, 1);
    assert!(!typed_row.error);
    let owner = cluster
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("typed-route owner pool");
    let audit_baseline: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM vala.audit_outbox WHERE data_tenant_id = $1 AND operation = 'bifrost.query.security_violation'",
    )
    .bind(cluster.data_tenant_id().as_uuid())
    .fetch_one(&owner)
    .await
    .expect("typed audit baseline");
    let sql_identity = query_trace_identity_value(&reader)
        .await
        .expect("SQL Oracle identity/value query succeeds");
    assert_eq!(
        sql_identity,
        vec![(
            "11".repeat(16),
            "22".repeat(8),
            "typed-cut-span".to_owned(),
            "typed-route".to_owned(),
        )]
    );
    let (rows, outcome, error) =
        query_statement(
            &reader,
            "SELECT * FROM vala.traces.spans WHERE service_name = 'typed-route' AND name = 'typed-cut-span'"
                .to_owned(),
        )
            .await
            .expect("SQL Oracle cut succeeds");
    assert_eq!(rows, 1);
    assert_eq!(outcome, QueryTerminalOutcome::Success);
    assert!(error.is_none());

    let foreign_tenant = cluster
        .add_tenant("typed-route-foreign")
        .await
        .expect("foreign tenant");
    seed_foreign_trace_row(&cluster, cluster.data_tenant_id(), foreign_tenant)
        .await
        .expect("foreign trace fixture");
    let typed_error = typed
        .query_traces(with_access_token(
            Request::new(QueryTracesRequest {
                window: Some(QueryWindow {
                    since: String::new(),
                    until: String::new(),
                    limit: 100,
                    page_token: String::new(),
                }),
                service: "typed-route".to_owned(),
                min_duration_ms: 0,
                status: String::new(),
                name: "typed-cut-span".to_owned(),
            }),
            &token,
        ))
        .await;
    let typed_error = typed_error.expect_err("foreign tenant must fail typed query closed");
    assert_eq!(typed_error.code(), wyrd_tonic::tonic::Code::Internal);
    assert_eq!(typed_error.message(), "query tenant invariant violated");
    let (sql_rows, sql_outcome, sql_error) = query_statement(
        &reader,
        "SELECT * FROM vala.traces.spans WHERE service_name = 'typed-route' AND name = 'typed-cut-span'"
            .to_owned(),
    )
    .await
    .expect("foreign SQL terminal");
    assert_eq!(sql_rows, 0);
    assert_eq!(sql_outcome, QueryTerminalOutcome::Failed);
    assert_eq!(
        sql_error,
        Some(QueryTerminalErrorCode::QueryTenantInvariant)
    );
    let audit_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let security_audits: i64 = loop {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM vala.audit_outbox WHERE data_tenant_id = $1 AND operation = 'bifrost.query.security_violation'",
        )
        .bind(cluster.data_tenant_id().as_uuid())
        .fetch_one(&owner)
        .await
        .expect("typed security audit");
        if count >= audit_baseline + 2 || tokio::time::Instant::now() >= audit_deadline {
            break count;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    };
    assert_eq!(
        security_audits - audit_baseline,
        2,
        "typed and SQL tripwires each append exactly one durable audit"
    );
    cluster.shutdown().await.expect("shutdown cluster");
}

/// Builds one deterministic OTLP span reused across sealed and hot writes.
fn typed_route_trace() -> ExportTraceServiceRequest {
    let span = OtlpSpan {
        trace_id: vec![0x11; 16],
        span_id: vec![0x22; 8],
        parent_span_id: Vec::new(),
        trace_state: String::new(),
        flags: 0,
        name: "typed-cut-span".to_owned(),
        kind: span::SpanKind::Server as i32,
        start_time_unix_nano: 1_700_000_000_000_000_000,
        end_time_unix_nano: 1_700_000_000_050_000_000,
        attributes: Vec::new(),
        dropped_attributes_count: 0,
        events: Vec::new(),
        dropped_events_count: 0,
        links: Vec::new(),
        dropped_links_count: 0,
        status: Some(OtlpStatus {
            message: String::new(),
            code: StatusCode::Ok as i32,
        }),
    };
    ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(OtlpResource {
                attributes: vec![KeyValue {
                    key: "service.name".to_owned(),
                    value: Some(AnyValue {
                        value: Some(any_value::Value::StringValue("typed-route".to_owned())),
                    }),
                }],
                dropped_attributes_count: 0,
                entity_refs: Vec::new(),
            }),
            scope_spans: vec![ScopeSpans {
                scope: None,
                spans: vec![span],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
}

/// Adds the OTLP access-token metadata expected by the Gate collector.
fn with_access_token<T>(mut request: Request<T>, token: &str) -> Request<T> {
    request.metadata_mut().insert(
        "x-wyrd-access-token",
        format!("Bearer {token}").parse().expect("token metadata"),
    );
    request
}

/// Persists one foreign-tenant trace row beneath the built-in spans table.
async fn seed_foreign_trace_row(
    cluster: &WyrdTestCluster,
    owner: DataTenantId,
    foreign: DataTenantId,
) -> Result<(), JourneyError> {
    let binding =
        TenantTableBinding::resolve((owner, TableRef::new(BifrostNamespace::Traces, "spans")))?;
    let user_fields = vec![
        Field::new("trace_id", DataType::FixedSizeBinary(16), false),
        Field::new("span_id", DataType::FixedSizeBinary(8), false),
        Field::new("parent_span_id", DataType::FixedSizeBinary(8), true),
        Field::new("flags", DataType::Int64, false),
        Field::new("trace_state", DataType::Utf8, true),
        Field::new("name", DataType::Utf8, false),
        Field::new("kind", DataType::Utf8, false),
        Field::new(
            "start_time",
            DataType::Timestamp(arrow::datatypes::TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
        Field::new(
            "end_time",
            DataType::Timestamp(arrow::datatypes::TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
        Field::new("duration_ms", DataType::Int64, false),
        Field::new("status", DataType::Utf8, false),
        Field::new("attributes", DataType::Utf8, true),
        Field::new("dropped_attributes_count", DataType::Int64, false),
        Field::new("dropped_events_count", DataType::Int64, false),
        Field::new("dropped_links_count", DataType::Int64, false),
        Field::new("scope_name", DataType::Utf8, true),
        Field::new("scope_version", DataType::Utf8, true),
        Field::new("service_name", DataType::Utf8, false),
    ];
    let schema = Arc::new(Schema::new(with_managed_columns(user_fields)));
    let mut trace_id = FixedSizeBinaryBuilder::with_capacity(1, 16);
    trace_id.append_value([0x31; 16])?;
    let mut span_id = FixedSizeBinaryBuilder::with_capacity(1, 8);
    span_id.append_value([0x41; 8])?;
    let mut parent_span_id = FixedSizeBinaryBuilder::with_capacity(1, 8);
    parent_span_id.append_null();
    let mut batch_id = FixedSizeBinaryBuilder::with_capacity(1, 16);
    batch_id.append_value(uuid::Uuid::now_v7().as_bytes())?;
    let batch = RecordBatch::try_new(
        Arc::clone(&schema),
        vec![
            Arc::new(trace_id.finish()),
            Arc::new(span_id.finish()),
            Arc::new(parent_span_id.finish()),
            Arc::new(Int64Array::from(vec![0])),
            Arc::new(StringArray::from(vec![None::<String>])),
            Arc::new(StringArray::from(vec!["typed-cut-span"])),
            Arc::new(StringArray::from(vec!["SERVER"])),
            Arc::new(
                TimestampMicrosecondArray::from(vec![1_700_000_000_000_000_i64])
                    .with_timezone("UTC"),
            ),
            Arc::new(
                TimestampMicrosecondArray::from(vec![1_700_000_000_050_000_i64])
                    .with_timezone("UTC"),
            ),
            Arc::new(Int64Array::from(vec![50])),
            Arc::new(StringArray::from(vec!["OK"])),
            Arc::new(StringArray::from(vec![None::<String>])),
            Arc::new(Int64Array::from(vec![0])),
            Arc::new(Int64Array::from(vec![0])),
            Arc::new(Int64Array::from(vec![0])),
            Arc::new(StringArray::from(vec![None::<String>])),
            Arc::new(StringArray::from(vec![None::<String>])),
            Arc::new(StringArray::from(vec!["typed-route"])),
            Arc::new(StringArray::from(vec![None::<String>])),
            Arc::new(StringArray::from(vec![None::<String>])),
            Arc::new(StringArray::from(vec![uuid::Uuid::now_v7().to_string()])),
            Arc::new(StringArray::from(vec![RequestId::now_v7().to_string()])),
            Arc::new(
                TimestampMicrosecondArray::from(vec![1_700_000_000_050_000_i64])
                    .with_timezone("UTC"),
            ),
            Arc::new(
                TimestampMicrosecondArray::from(vec![1_700_000_000_050_001_i64])
                    .with_timezone("UTC"),
            ),
            Arc::new(batch_id.finish()),
            Arc::new(Int32Array::from(vec![0])),
            Arc::new(StringArray::from(vec![foreign.to_string()])),
        ],
    )?;
    let mut parquet = Vec::new();
    let mut writer = ArrowWriter::try_new(&mut parquet, schema, None)?;
    writer.write(&batch)?;
    writer.close()?;
    let path = format!("{}/typed-route-foreign.parquet", binding.object_prefix);
    cluster
        .storage_operator()
        .write(&path, Buffer::from(parquet.clone()))
        .await?;
    let event = AuditEvent::new(
        RequestId::now_v7(),
        None,
        "oracle.journey.typed_foreign_row".to_owned(),
        "bifrost.oracle.journey".to_owned(),
        None,
        PrincipalId::new(uuid::Uuid::now_v7()),
        PrincipalKindTag::User,
        AuthMethod::Internal,
        "bifrost_query:read".to_owned(),
        AuditDecision::Allow,
        AuditResult::Success,
        "typed foreign tripwire fixture".to_owned(),
    );
    let mut conn = cluster.pg_fixture().tenant_conn_for(owner).await?;
    insert_and_audit(
        &mut conn,
        &FileListInsert {
            id: uuid::Uuid::now_v7(),
            data_tenant_id: owner,
            namespace: &binding.logical_namespace,
            table_name: &binding.table_name,
            file_path: &path,
            file_size: i64::try_from(parquet.len())?,
            row_count: 1,
            min_event_time: Utc::now(),
            max_event_time: Utc::now(),
            partition_day: NaiveDate::from_ymd_opt(2023, 11, 14).ok_or("invalid fixture day")?,
            node_id: uuid::Uuid::now_v7(),
            writer_epoch: 1,
            wal_lsn_min: 9_101,
            wal_lsn_max: 9_101,
        },
        &[event],
    )
    .await?;
    conn.commit().await?;
    Ok(())
}

/// Assert production recorder labels and production-pipeline spans for a real query.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn oracle_production_telemetry_contract() {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed())
        .await
        .expect("telemetry cluster");
    let server = cluster.server(0).expect("telemetry server");
    let query_server = cluster.server(0).expect("telemetry query server");
    let table = unique_table("oracle_telemetry");
    register_table(server, cluster.data_tenant_id(), &table)
        .await
        .expect("telemetry table");
    let writer = client(server, "oracle-telemetry-writer")
        .await
        .expect("telemetry writer");
    let reader = client(query_server, "oracle-telemetry-reader")
        .await
        .expect("telemetry reader");
    let published_rows = (0_i64..10_000).collect::<Vec<_>>();
    ingest(&writer, &format!("vala.bifrost.{table}"), &published_rows)
        .await
        .expect("telemetry ingest");
    server.flush_bifrost().await.expect("telemetry flush");
    let checkpoint = cluster
        .telemetry()
        .checkpoint()
        .expect("telemetry checkpoint");
    assert_eq!(
        query_rows(&reader, &table, VisibilityMode::PublishedOnly)
            .await
            .expect("telemetry query"),
        10_000
    );
    ingest(&writer, &format!("vala.bifrost.{table}"), &[10_000])
        .await
        .expect("telemetry live ingest");
    cluster
        .observe_live_tail(
            &format!("vala.bifrost.{table}"),
            EventDay::new(chrono::Utc::now().format("%Y-%m-%d").to_string()).expect("event day"),
        )
        .await
        .expect("telemetry tail observation");
    assert_eq!(
        query_rows(&reader, &table, VisibilityMode::Fused)
            .await
            .expect("telemetry fused query"),
        10_001
    );
    let delta = cluster
        .telemetry()
        .delta_since(&checkpoint)
        .expect("telemetry delta");
    let mut observed_families = delta
        .metrics
        .iter()
        .map(|sample| sample.family.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let absolute_families = cluster
        .telemetry()
        .snapshot()
        .expect("absolute telemetry snapshot")
        .into_iter()
        .map(|sample| sample.family)
        .collect::<std::collections::BTreeSet<_>>();
    observed_families.extend(absolute_families.iter().cloned());
    for (required, _) in ORACLE_METRIC_LABELS {
        assert!(
            observed_families.contains(*required),
            "missing production metric {required}: {observed_families:?}"
        );
    }
    for family in [
        "oracle_query_logical_bytes_selected_total",
        "oracle_query_bytes_scanned_total",
    ] {
        assert!(
            delta
                .metrics
                .iter()
                .any(|sample| sample.family == family && sample.value > 0.0),
            "canonical Parquet query did not emit positive {family}"
        );
    }
    let prohibited = [
        "tenant",
        "tenant_id",
        "principal",
        "principal_id",
        "request_id",
        "query_id",
        "fragment_id",
        "path",
        "sql",
        "error",
        "error_text",
    ];
    for sample in delta.metrics.iter().filter(|sample| {
        sample.family.starts_with("oracle_") || sample.family == "bifrost_role_ready"
    }) {
        if let Some((_, expected)) = ORACLE_METRIC_LABELS
            .iter()
            .find(|(family, _)| *family == sample.family)
        {
            let actual = sample
                .labels
                .keys()
                .filter(|label| !matches!(label.as_str(), "quantile" | "le"))
                .map(String::as_str)
                .collect::<std::collections::BTreeSet<_>>();
            let expected = expected
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(
                actual, expected,
                "metric {} changed its closed label contract",
                sample.family
            );
            for (label, value) in &sample.labels {
                if matches!(label.as_str(), "quantile" | "le") {
                    continue;
                }
                assert!(
                    metric_label_value_is_closed(label, value),
                    "metric {} emitted open label {label}={value}",
                    sample.family
                );
            }
        }
        assert!(
            sample
                .labels
                .keys()
                .all(|key| !prohibited.contains(&key.as_str())),
            "high-cardinality label leaked on {}: {:?}",
            sample.family,
            sample.labels
        );
    }
    let spans = delta
        .spans
        .iter()
        .map(|span| span.name.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    for required in ORACLE_SPANS {
        assert!(
            spans.contains(required),
            "missing production span {required}: {spans:?}"
        );
    }
    for span in delta
        .spans
        .iter()
        .filter(|span| ORACLE_SPANS.contains(&span.name.as_str()))
    {
        assert!(
            span.attributes
                .keys()
                .all(|key| !prohibited.contains(&key.as_str())),
            "high-cardinality span field leaked on {}: {:?}",
            span.name,
            span.attributes
        );
    }
    let query_spans = delta
        .spans
        .iter()
        .filter(|span| span.name == "bifrost.oracle.query")
        .collect::<Vec<_>>();
    assert!(
        !query_spans.is_empty(),
        "query span attributes were not captured"
    );
    for span in query_spans {
        for field in ["request_node_id", "leader_node_id"] {
            let value = span
                .attributes
                .get(field)
                .expect("required scrubbed node field");
            uuid::Uuid::parse_str(value).expect("node field must be a scrubbed UUID");
        }
        assert_eq!(
            span.attributes.get("search_role").map(String::as_str),
            Some("oracle")
        );
    }
    assert!(absolute_families.contains("oracle_fragments_active"));
    let residual = cluster
        .oracle_inspection()
        .await
        .expect("telemetry residual inspection");
    assert_eq!(residual.active_queries, 0);
    assert_eq!(residual.queued_queries, 0);
    assert_eq!(residual.reserved_memory_bytes, 0);
    assert_eq!(residual.reserved_spill_bytes, 0);
    assert_eq!(residual.peer_pending, 0);
    assert_eq!(residual.peer_running, 0);
    cluster.shutdown().await.expect("telemetry shutdown");

    let role_cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::role_separated())
        .await
        .expect("role telemetry cluster");
    for server in role_cluster.servers() {
        assert_eq!(
            server.forge_process_role(),
            wyrd_server::config::BifrostTarget::Server,
            "role-separated roster must use full Server processes"
        );
        assert!(server.bifrost_scribe().is_some());
        assert!(server.state().bifrost_query().is_some());
    }
    let role_samples = role_cluster
        .telemetry()
        .snapshot()
        .expect("role gauge snapshot");
    observed_families.extend(role_samples.iter().map(|sample| sample.family.clone()));
    for role in ["scribe", "oracle"] {
        assert!(role_samples.iter().any(|sample| {
            sample.family == "bifrost_role_ready"
                && sample.labels.get("role").map(String::as_str) == Some(role)
        }));
    }
    for (family, _) in ORACLE_METRIC_LABELS {
        assert!(
            observed_families.contains(*family),
            "required production metric family {family} was absent across captured scenarios: {observed_families:?}"
        );
    }
    role_cluster
        .shutdown()
        .await
        .expect("role telemetry shutdown");
}

/// Validate every normative label value against its closed vocabulary.
fn metric_label_value_is_closed(label: &str, value: &str) -> bool {
    match label {
        "visibility" => matches!(value, "published_only" | "fused"),
        "query_class" | "class" => matches!(value, "interactive" | "analytical"),
        "role" | "required_role" => matches!(value, "leader" | "worker" | "oracle" | "scribe"),
        "locality" => matches!(value, "local" | "remote"),
        "source" | "losing_source" => matches!(value, "iceberg" | "hot_sealed" | "live_tail"),
        "freshness" => matches!(value, "complete" | "degraded"),
        "memory_kind" => matches!(value, "source" | "tail" | "reconciliation" | "attempt"),
        "operator" => matches!(value, "reconcile" | "attempt"),
        "audit_kind" => matches!(value, "read_decision" | "security_violation"),
        "event_class" => matches!(value, "tenant_row" | "ticket" | "claims" | "fragment"),
        "scope" => matches!(value, "cluster" | "class" | "tenant"),
        "reason" => matches!(
            value,
            "class_capacity"
                | "tenant_budget"
                | "queue_full"
                | "queue_deadline"
                | "memory"
                | "spill"
                | "audit_unavailable"
                | "shutdown"
                | "client_drop"
                | "deadline"
                | "peer_failure"
                | "postgres"
                | "timeout"
                | "serialization"
        ),
        "error_class" => matches!(
            value,
            "none" | "availability" | "security" | "exhausted" | "footer" | "attempt"
        ),
        "outcome" => matches!(
            value,
            "success"
                | "failed"
                | "cancelled"
                | "degraded"
                | "acquired"
                | "rejected"
                | "pending"
                | "running"
                | "spilled"
                | "retried"
                | "admitted"
                | "retried_transient"
                | "committed"
        ),
        _ => false,
    }
}

/// Builds one deterministic sealed table large enough to exercise native
/// ordered-query spill through the production DataFusion path.
///
/// # Errors
///
/// Returns a catalog, authentication, public ingest, or durable flush error.
async fn prepare_spill_table(
    cluster: &WyrdTestCluster,
    server: &wyrd_testing::WyrdTestServer,
    prefix: &str,
    row_count: usize,
) -> Result<String, JourneyError> {
    let table = unique_table(prefix);
    register_table(server, cluster.data_tenant_id(), &table).await?;
    let writer = client(server, &format!("{prefix}-writer")).await?;
    let transport = BifrostGrpcTransport::connect(&writer).await?;
    let scribe_baseline = server
        .state()
        .bifrost_resources()
        .ok_or("spill fixture lacks memory governor")?
        .snapshot()?
        .scribe_memory_used_bytes;
    for start in (0..row_count).step_by(SPILL_INGEST_BATCH_ROWS) {
        let end = (start + SPILL_INGEST_BATCH_ROWS).min(row_count);
        let chunk = u64::try_from(start / SPILL_INGEST_BATCH_ROWS)?;
        let batch_id = uuid::Uuid::new_v7(uuid::Timestamp::from_unix_time(
            2_000_000_000_u64.saturating_sub(chunk),
            0,
            0,
            0,
        ));
        let ids = (start..end)
            .map(i64::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        transport
            .insert_batch(
                &format!("vala.bifrost.{table}"),
                batch_id.into_bytes(),
                spill_ipc(&ids),
            )
            .await?;
        let batches_ingested = end.div_ceil(SPILL_INGEST_BATCH_ROWS);
        if batches_ingested.is_multiple_of(SPILL_BATCHES_PER_SEAL) || end == row_count {
            server.flush_bifrost().await?;
            wait_scribe_memory_restored(server, scribe_baseline).await?;
            assert_scribe_fixture_peaks_bounded(server)?;
        }
    }
    Ok(table)
}

/// Verifies fixture ingestion remains within the production memory ceilings.
///
/// # Errors
///
/// Returns an inspection error or a diagnostic mismatch when current
/// Scribe-child or Bifrost-parent ownership exceeds its production limit.
fn assert_scribe_fixture_peaks_bounded(
    server: &wyrd_testing::WyrdTestServer,
) -> Result<(), JourneyError> {
    let governor = server
        .state()
        .bifrost_resources()
        .ok_or("spill fixture lacks memory governor")?;
    let snapshot = governor.snapshot()?;
    let scribe_used = snapshot.scribe_memory_used_bytes;
    let bifrost_used = managed_memory_used(snapshot);
    let scribe_limit = snapshot.plan.scribe_floor_bytes + snapshot.plan.elastic_memory_bytes;
    let bifrost_limit = snapshot.plan.managed_memory_bytes;
    if scribe_used > scribe_limit || bifrost_used > bifrost_limit {
        return Err(format!(
            "spill fixture exceeded production memory: scribe_used={scribe_used} scribe_limit={scribe_limit} bifrost_used={bifrost_used} bifrost_limit={bifrost_limit}",
        )
        .into());
    }
    Ok(())
}

/// Waits cooperatively for a completed flush to release its Scribe generation
/// before the next bounded ingest chunk is admitted.
///
/// # Errors
///
/// Returns an inspection error or a timeout when the production governor does
/// not return to the exact pre-fixture Scribe baseline.
async fn wait_scribe_memory_restored(
    server: &wyrd_testing::WyrdTestServer,
    baseline: usize,
) -> Result<(), JourneyError> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let current = server
            .state()
            .bifrost_resources()
            .ok_or("spill fixture lacks memory governor")?
            .snapshot()?
            .scribe_memory_used_bytes;
        if current == baseline {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!(
                "Scribe memory did not return to fixture baseline: baseline={baseline} current={current}"
            )
            .into());
        }
        tokio::task::yield_now().await;
    }
}

/// Requires query admission, parent/child memory, spill ownership, and local
/// scratch files to match the exact pre-query baseline.
///
/// # Errors
///
/// Returns an inspection error or a diagnostic mismatch for any retained
/// production owner.
fn assert_oracle_runtime_restored(
    server: &wyrd_testing::WyrdTestServer,
    baseline: wyrd_testing::OracleRuntimeInspection,
    memory_baseline: (usize, usize),
) -> Result<(), JourneyError> {
    let current = server.oracle_runtime_inspection()?;
    if current.active_queries != baseline.active_queries
        || current.queued_queries != baseline.queued_queries
        || current.reserved_memory_bytes != baseline.reserved_memory_bytes
        || current.reserved_spill_bytes != baseline.reserved_spill_bytes
        || current.peer_pending != baseline.peer_pending
        || current.peer_running != baseline.peer_running
        || current.spill_directories != baseline.spill_directories
        || current.spill_files != baseline.spill_files
        || current.spill_file_bytes != baseline.spill_file_bytes
    {
        return Err(format!(
            "Oracle runtime did not return to baseline: baseline={baseline:?} current={current:?}"
        )
        .into());
    }
    let memory = server
        .state()
        .bifrost_resources()
        .ok_or("Oracle server lacks the shared memory governor")?
        .snapshot()?;
    if (managed_memory_used(memory), memory.oracle_memory_used_bytes) != memory_baseline {
        return Err(format!(
            "Oracle memory did not return to baseline: baseline={memory_baseline:?} current=({},{})",
            managed_memory_used(memory), memory.oracle_memory_used_bytes
        )
        .into());
    }
    Ok(())
}

/// Run one public gRPC-ingest to HTTP-query roundtrip and validate the terminal.
async fn public_roundtrip(
    spec: BifrostClusterSpec,
    visibility: VisibilityMode,
    flush: bool,
    query_index: usize,
    expected_metric: Option<&str>,
    assert_distributed_physical: bool,
) -> Result<(), JourneyError> {
    let cluster = WyrdTestCluster::start_spec(spec).await?;
    let ingest_server = cluster
        .servers()
        .find(|server| server.bifrost_scribe().is_some())
        .ok_or("missing ingest node")?;
    let table = unique_table("oracle_journey");
    register_table(ingest_server, cluster.data_tenant_id(), &table).await?;
    let writer = client(ingest_server, "oracle-journey-writer").await?;
    ingest(&writer, &format!("vala.bifrost.{table}"), &[1, 2]).await?;
    if flush {
        ingest_server.flush_bifrost().await?;
    } else {
        let day = EventDay::new(chrono::Utc::now().format("%Y-%m-%d").to_string())?;
        cluster
            .observe_live_tail(&format!("vala.bifrost.{table}"), day)
            .await?;
    }
    let query_server = cluster.server(query_index).ok_or("missing query node")?;
    let reader = client(query_server, "oracle-journey-reader").await?;
    let checkpoint = cluster
        .telemetry()
        .checkpoint()
        .expect("helper telemetry checkpoint");
    assert_eq!(query_rows(&reader, &table, visibility).await?, 2);
    if let Some(family) = expected_metric {
        let delta = cluster
            .telemetry()
            .delta_since(&checkpoint)
            .map_err(|error| error.to_string())?;
        assert!(
            delta
                .metrics
                .iter()
                .any(|sample| sample.family == family && sample.value > 0.0),
            "journey did not emit required production metric {family}: {:?}",
            delta.metrics
        );
        if assert_distributed_physical {
            for family in [
                "oracle_query_bytes_scanned_total",
                "oracle_query_files_scanned_total",
                "oracle_query_partitions_scanned_total",
            ] {
                let physical = delta
                    .metrics
                    .iter()
                    .filter(|sample| sample.family == family && sample.value > 0.0)
                    .count();
                assert_eq!(physical, 1, "distributed worker must emit {family} once");
            }
        }
    }
    let inspection = cluster.oracle_inspection().await?;
    assert_eq!(inspection.active_queries, 0);
    assert_eq!(inspection.queued_queries, 0);
    assert_eq!(inspection.reserved_memory_bytes, 0);
    assert_eq!(inspection.reserved_spill_bytes, 0);
    assert_eq!(inspection.peer_pending, 0);
    assert_eq!(inspection.peer_running, 0);
    cluster.shutdown().await?;
    Ok(())
}

/// Register one tenant-owned Redux table through the server-owned catalog.
async fn register_table(
    server: &wyrd_testing::WyrdTestServer,
    tenant: DataTenantId,
    table: &str,
) -> Result<(), JourneyError> {
    server
        .state()
        .bifrost_catalog()
        .expect("Scribe composition retains the shared catalog")
        .create_table(CreateTableRequest {
            table: TableRef::new(BifrostNamespace::Bifrost, table),
            user_fields: vec![
                Field::new("id", DataType::Int64, false),
                Field::new("value", DataType::Utf8, false),
            ],
            tenant,
            audit: None,
        })
        .await?;
    Ok(())
}

/// Registers the locked one-column table used only by the paired progress proof.
async fn register_paired_table(
    server: &wyrd_testing::WyrdTestServer,
    tenant: DataTenantId,
    table: &str,
) -> Result<(), JourneyError> {
    server
        .state()
        .bifrost_catalog()
        .expect("Scribe composition retains the shared catalog")
        .create_table(CreateTableRequest {
            table: TableRef::new(BifrostNamespace::Bifrost, table),
            user_fields: vec![Field::new("row_id", DataType::Int64, false)],
            tenant,
            audit: None,
        })
        .await?;
    Ok(())
}

/// Build one authenticated public client for the fixture tenant.
async fn client(
    server: &wyrd_testing::WyrdTestServer,
    name: &str,
) -> Result<WyrdClient, JourneyError> {
    client_for_tenant(server, server.data_tenant_id(), name).await
}

/// Build one authenticated public client for an explicit tenant.
async fn client_for_tenant(
    server: &wyrd_testing::WyrdTestServer,
    tenant: DataTenantId,
    name: &str,
) -> Result<WyrdClient, JourneyError> {
    let bootstrap = server
        .bootstrap_service_in_tenant(tenant, name, &["admin"])
        .await?;
    client_from_bootstrap(server, bootstrap).await
}

/// Build a client while retaining the bootstrap principal for exact audit correlation.
async fn client_from_bootstrap(
    server: &wyrd_testing::WyrdTestServer,
    bootstrap: Bootstrap,
) -> Result<WyrdClient, JourneyError> {
    let api_key = match bootstrap {
        Bootstrap::Machine { api_key, .. } => api_key,
        Bootstrap::User { .. } => return Err("machine bootstrap returned user".into()),
    };
    Ok(WyrdClient::with_config(ClientConfig {
        grpc: GrpcConfig {
            endpoint: server.grpc_url().ok_or("missing gRPC URL")?,
            connect_retries: 0,
            max_message_bytes: 32 * 1024 * 1024,
            ..GrpcConfig::default()
        },
        http: HttpConfig {
            base_url: server.base_url().ok_or("missing HTTP URL")?.to_owned(),
            ..HttpConfig::default()
        },
        api_key: Some(api_key),
        ..ClientConfig::default()
    })?)
}

/// Collect one HTTP query response into canonical protobuf frames.
///
/// # Errors
///
/// Returns a transport, status, framing, or protobuf error when the HTTP
/// stream cannot be decoded to the public frame contract.
async fn http_query_frames(
    base_url: &str,
    client: &WyrdClient,
    request: &BifrostQueryRequest,
) -> Result<Vec<proto::QueryStreamFrame>, JourneyError> {
    let bearer = client.auth().bearer().await?;
    let base_url = base_url.trim_end_matches('/');
    let response = reqwest::Client::new()
        .post(format!("{base_url}/v1/query"))
        .header("x-wyrd-access-token", format!("Bearer {}", bearer.expose()))
        .json(request)
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(format!("HTTP query failed with {}", response.status()).into());
    }
    let body = response.bytes().await?;
    let mut decoder = FrameDecoder::new(32 * 1024 * 1024);
    let frames = decoder.push::<proto::QueryStreamFrame>(&body)?;
    decoder.finish()?;
    Ok(frames)
}

/// Open one authenticated public gRPC query stream.
///
/// # Errors
///
/// Returns a connection, metadata, request validation, or typed tonic status
/// error before the first stream frame.
async fn grpc_query_stream(
    client: &WyrdClient,
    request: &BifrostQueryRequest,
) -> Result<wyrd_tonic::tonic::codec::Streaming<proto::QueryStreamFrame>, JourneyError> {
    let connection = client.connect_grpc().await?;
    let bearer = connection.auth().bearer().await?;
    let mut rpc = BifrostQueryServiceClient::new(connection.channel());
    let mut rpc_request =
        wyrd_tonic::tonic::Request::new(proto::BifrostQueryRequest::from(request.clone()));
    rpc_request.metadata_mut().insert(
        "x-wyrd-access-token",
        MetadataValue::try_from(format!("Bearer {}", bearer.expose()))?,
    );
    Ok(rpc.query(rpc_request).await?.into_inner())
}

/// Collect one public gRPC query response into canonical protobuf frames.
///
/// # Errors
///
/// Returns a connection, typed tonic status, or stream decode error.
async fn grpc_query_frames(
    client: &WyrdClient,
    request: &BifrostQueryRequest,
) -> Result<Vec<proto::QueryStreamFrame>, JourneyError> {
    let mut stream = grpc_query_stream(client, request).await?;
    let mut frames = Vec::new();
    while let Some(frame) = stream.message().await? {
        frames.push(frame);
    }
    Ok(frames)
}

/// Send one Arrow IPC batch through the public authenticated Gate transport.
async fn ingest(client: &WyrdClient, table: &str, ids: &[i64]) -> Result<(), JourneyError> {
    BifrostGrpcTransport::connect(client)
        .await?
        .insert_batch(table, uuid::Uuid::now_v7().into_bytes(), ipc(ids))
        .await?;
    Ok(())
}

/// Execute arbitrary read-only SQL and retain the exact terminal after draining frames.
///
/// # Errors
///
/// Returns a client error before a terminal or when a successful stream is malformed.
async fn query_statement(
    client: &WyrdClient,
    sql: String,
) -> Result<(u64, QueryTerminalOutcome, Option<QueryTerminalErrorCode>), JourneyError> {
    let mut stream = QueryClient::new(client)
        .query(&BifrostQueryRequest {
            sql,
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: None,
        })
        .await?;
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

/// Read the returned trace identity and user-visible values through SQL.
///
/// # Errors
///
/// Returns client, protocol, Arrow, or terminal errors when the query cannot
/// be drained or its typed columns do not match the trace contract.
async fn query_trace_identity_value(
    client: &WyrdClient,
) -> Result<Vec<(String, String, String, String)>, JourneyError> {
    let mut stream = QueryClient::new(client)
        .query(&BifrostQueryRequest {
            sql: "SELECT trace_id, span_id, name, service_name FROM vala.traces.spans WHERE service_name = 'typed-route' AND name = 'typed-cut-span'".to_owned(),
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: None,
        })
        .await?;
    let mut rows = Vec::new();
    while let Some(batch) = stream.next_batch().await? {
        let trace_ids = batch
            .column_by_name("trace_id")
            .and_then(|column| column.as_any().downcast_ref::<FixedSizeBinaryArray>())
            .ok_or("trace_id column has unexpected Arrow type")?;
        let span_ids = batch
            .column_by_name("span_id")
            .and_then(|column| column.as_any().downcast_ref::<FixedSizeBinaryArray>())
            .ok_or("span_id column has unexpected Arrow type")?;
        let names = batch
            .column_by_name("name")
            .and_then(|column| column.as_any().downcast_ref::<StringArray>())
            .ok_or("name column has unexpected Arrow type")?;
        let services = batch
            .column_by_name("service_name")
            .and_then(|column| column.as_any().downcast_ref::<StringArray>())
            .ok_or("service_name column has unexpected Arrow type")?;
        for row in 0..batch.num_rows() {
            rows.push((
                hex::encode(trace_ids.value(row)),
                hex::encode(span_ids.value(row)),
                names.value(row).to_owned(),
                services.value(row).to_owned(),
            ));
        }
    }
    let terminal = stream.terminal().ok_or("query terminal missing")?;
    if terminal.outcome != QueryTerminalOutcome::Success {
        return Err(format!("identity query failed: {:?}", terminal.error).into());
    }
    Ok(rows)
}

/// Persist one foreign-tenant physical row beneath the production provider union.
///
/// # Errors
///
/// Returns an Arrow, Parquet, storage, tenant-SQL, or manifest persistence error.
async fn seed_foreign_hot_row(
    cluster: &WyrdTestCluster,
    owner: DataTenantId,
    table: &str,
    foreign: DataTenantId,
    path_tag: &str,
) -> Result<(), JourneyError> {
    let table_ref = TableRef::new(BifrostNamespace::Bifrost, table);
    let binding = TenantTableBinding::resolve((owner, table_ref))?;
    let schema = Arc::new(Schema::new(with_managed_columns(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("value", DataType::Utf8, false),
    ])));
    let mut batch_ids = FixedSizeBinaryBuilder::with_capacity(1, 16);
    batch_ids.append_value(uuid::Uuid::now_v7().as_bytes())?;
    let batch = RecordBatch::try_new(
        Arc::clone(&schema),
        vec![
            Arc::new(Int64Array::from(vec![999_i64])) as ArrayRef,
            Arc::new(StringArray::from(vec!["foreign"])),
            Arc::new(StringArray::from(vec![None::<String>])),
            Arc::new(StringArray::from(vec![None::<String>])),
            Arc::new(StringArray::from(vec![uuid::Uuid::now_v7().to_string()])),
            Arc::new(StringArray::from(vec![RequestId::now_v7().to_string()])),
            Arc::new(TimestampMicrosecondArray::from(vec![1_000_000_i64]).with_timezone("UTC")),
            Arc::new(TimestampMicrosecondArray::from(vec![1_000_001_i64]).with_timezone("UTC")),
            Arc::new(batch_ids.finish()),
            Arc::new(Int32Array::from(vec![0])),
            Arc::new(StringArray::from(vec![foreign.to_string()])),
        ],
    )?;
    let mut parquet = Vec::new();
    let mut writer = ArrowWriter::try_new(&mut parquet, schema, None)?;
    writer.write(&batch)?;
    writer.close()?;
    let path = format!("{}/{path_tag}.parquet", binding.object_prefix);
    cluster
        .storage_operator()
        .write(&path, Buffer::from(parquet.clone()))
        .await?;
    let event = AuditEvent::new(
        RequestId::now_v7(),
        None,
        "oracle.journey.foreign_row".to_owned(),
        "bifrost.oracle.journey".to_owned(),
        None,
        PrincipalId::new(uuid::Uuid::now_v7()),
        PrincipalKindTag::User,
        AuthMethod::Internal,
        "bifrost_query:read".to_owned(),
        AuditDecision::Allow,
        AuditResult::Success,
        "foreign tripwire fixture".to_owned(),
    );
    let mut conn = cluster.pg_fixture().tenant_conn_for(owner).await?;
    insert_and_audit(
        &mut conn,
        &FileListInsert {
            id: uuid::Uuid::now_v7(),
            data_tenant_id: owner,
            namespace: &binding.logical_namespace,
            table_name: &binding.table_name,
            file_path: &path,
            file_size: i64::try_from(parquet.len())?,
            row_count: 1,
            min_event_time: Utc::now(),
            max_event_time: Utc::now(),
            partition_day: NaiveDate::from_ymd_opt(1970, 1, 1).ok_or("invalid fixture day")?,
            node_id: uuid::Uuid::now_v7(),
            writer_epoch: 1,
            wal_lsn_min: 9_001,
            wal_lsn_max: 9_001,
        },
        &[event],
    )
    .await?;
    conn.commit().await?;
    Ok(())
}

/// Persist one manifest identity whose pinned object is intentionally absent.
///
/// # Errors
///
/// Returns a tenant-SQL or manifest persistence error.
async fn seed_missing_hot_row(
    cluster: &WyrdTestCluster,
    tenant: DataTenantId,
    table: &str,
) -> Result<String, JourneyError> {
    let binding =
        TenantTableBinding::resolve((tenant, TableRef::new(BifrostNamespace::Bifrost, table)))?;
    let path = format!("{}/j7-missing.parquet", binding.object_prefix);
    let event = AuditEvent::new(
        RequestId::now_v7(),
        None,
        "oracle.journey.missing_row".to_owned(),
        "bifrost.oracle.journey".to_owned(),
        None,
        PrincipalId::new(uuid::Uuid::now_v7()),
        PrincipalKindTag::User,
        AuthMethod::Internal,
        "bifrost_query:read".to_owned(),
        AuditDecision::Allow,
        AuditResult::Success,
        "stale replan fixture".to_owned(),
    );
    let mut conn = cluster.pg_fixture().tenant_conn_for(tenant).await?;
    insert_and_audit(
        &mut conn,
        &FileListInsert {
            id: uuid::Uuid::now_v7(),
            data_tenant_id: tenant,
            namespace: &binding.logical_namespace,
            table_name: &binding.table_name,
            file_path: &path,
            file_size: 128,
            row_count: 1,
            min_event_time: Utc::now(),
            max_event_time: Utc::now(),
            partition_day: NaiveDate::from_ymd_opt(1970, 1, 1).ok_or("invalid fixture day")?,
            node_id: uuid::Uuid::now_v7(),
            writer_epoch: 1,
            wal_lsn_min: 9_002,
            wal_lsn_max: 9_002,
        },
        &[event],
    )
    .await?;
    conn.commit().await?;
    Ok(path)
}

/// Drive the real Rust SDK against an EOF-before-terminal HTTP response.
async fn prove_sdk_missing_terminal_rejected() {
    let app = Router::new()
        .route(
            "/auth/token",
            post(|| async {
                Json(serde_json::json!({
                    "access_token": "missing-terminal-access-token",
                    "token_type": "Bearer",
                    "expires_at": "2099-01-01T00:00:00Z"
                }))
            }),
        )
        .route(
            "/v1/query",
            post(|| async {
                let mut response = Response::new(Body::empty());
                response.headers_mut().insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/vnd.wyrd.bifrost-query-stream"),
                );
                response
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("missing-terminal listener");
    let address = listener.local_addr().expect("missing-terminal address");
    let server = tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("missing-terminal server");
    });
    let client = WyrdClient::with_config(ClientConfig {
        grpc: GrpcConfig {
            endpoint: format!("http://{address}"),
            connect_retries: 0,
            ..GrpcConfig::default()
        },
        http: HttpConfig {
            base_url: format!("http://{address}"),
            ..HttpConfig::default()
        },
        api_key: Some(SecretString::from("missing-terminal-test")),
        ..ClientConfig::default()
    })
    .expect("missing-terminal SDK client");
    let mut stream = QueryClient::new(&client)
        .query(&BifrostQueryRequest {
            sql: "SELECT 1".to_owned(),
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: None,
        })
        .await
        .expect("missing-terminal response stream");
    assert!(matches!(
        stream.next_batch().await,
        Err(ValaSdkError::IncompleteQueryStream)
    ));
    server.abort();
}

/// Drain a public query stream and require its terminal row count to match frames.
async fn query_rows(
    client: &WyrdClient,
    table: &str,
    visibility: VisibilityMode,
) -> Result<u64, JourneyError> {
    let mut stream = QueryClient::new(client)
        .query(&BifrostQueryRequest {
            sql: format!("SELECT id, value FROM vala.bifrost.{table} ORDER BY id"),
            visibility,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: None,
        })
        .await?;
    let mut rows = 0_u64;
    while let Some(batch) = stream.next_batch().await? {
        rows = rows.saturating_add(u64::try_from(batch.num_rows())?);
    }
    let terminal = stream.terminal().ok_or("query terminal missing")?;
    if terminal.row_count != rows {
        return Err("terminal row count differs from Arrow frames".into());
    }
    Ok(rows)
}

/// Encode deterministic journey rows as one Arrow stream.
fn ipc(ids: &[i64]) -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("value", DataType::Utf8, false),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(ids.to_vec())),
            Arc::new(StringArray::from(vec!["oracle"; ids.len()])),
        ],
    )
    .expect("fixed journey arrays share a length");
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, schema.as_ref()).expect("valid schema");
    writer.write(&batch).expect("in-memory IPC write");
    writer.finish().expect("in-memory IPC finish");
    bytes
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

/// Encodes production-shaped rows in bounded transport chunks so aggregate
/// reconciliation state, rather than a single ingest request, owns the peak.
fn spill_ipc(ids: &[i64]) -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("value", DataType::Utf8, false),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(ids.to_vec())),
            Arc::new(StringArray::from(vec!["x".repeat(256); ids.len()])),
        ],
    )
    .expect("fixed spill arrays share a length");
    let mut bytes = Vec::new();
    let mut writer =
        StreamWriter::try_new(&mut bytes, schema.as_ref()).expect("valid spill schema");
    writer.write(&batch).expect("in-memory spill IPC write");
    writer.finish().expect("in-memory spill IPC finish");
    bytes
}

/// Return a collision-free SQL identifier for one serialized journey.
fn unique_table(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::now_v7().simple())
}

/// Derive a deterministic row marker without exposing tenant identity in telemetry.
fn tenant_marker(tenant: DataTenantId) -> i64 {
    i64::from(tenant.as_uuid().as_bytes()[0])
}
