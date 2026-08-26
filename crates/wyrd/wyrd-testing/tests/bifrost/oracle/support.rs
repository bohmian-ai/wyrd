//! Shared fixtures and helpers for the Oracle journey modules.
//!
//! Every item here is used by more than one module of the `oracle` binary.
//! A helper used by exactly one module lives in that module instead, so that
//! reading a journey does not mean reading this file first. Contains no
//! tests.

use arrow::array::{
    Array, ArrayRef, FixedSizeBinaryBuilder, Int32Array, Int64Array, StringArray,
    TimestampMicrosecondArray,
};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use chrono::{Timelike, Utc};
use opendal::Buffer;
use parquet::arrow::ArrowWriter;
use serde::Serialize;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef, TenantTableBinding};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::resources::{
    MIN_SCRATCH_FREE_BYTES, ResourceSnapshot, ResourceSource, SystemResourceSnapshot,
};
use vala_bifrost_redux::schema::with_managed_columns;
use vala_bifrost_redux::scribe::file_list_writer::{FileListInsert, insert_and_audit};
use vala_sdk::{BifrostGrpcTransport, QueryClient, ValaSdkError};
use wyrd_client::WyrdClient;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuditDecision, AuditEvent, AuditResult, AuthMethod, BifrostQueryRequest, FreshnessPolicy,
    QueryTerminalErrorCode, QueryTerminalOutcome, VisibilityMode,
};
use wyrd_spec::vala::error::BifrostError;
use wyrd_testing::Bootstrap;
use wyrd_testing::bifrost::{BifrostClusterSpec, WyrdTestCluster};

/// Exact hour partition the live Scribe tail is writing into right now.
///
/// Registrations in these journeys take the default `hour(wyrd_event_time)`
/// layout, so tail observation must name the current hour rather than the day.
pub(crate) fn current_hour_partition() -> wyrd_spec::vala::api::TimePartitionWire {
    wyrd_spec::vala::api::TimePartitionWire::new(
        wyrd_spec::vala::api::TimeGranularityWire::Hour,
        chrono::Utc::now()
            .with_minute(0)
            .and_then(|value| value.with_second(0))
            .and_then(|value| value.with_nanosecond(0))
            .expect("truncating to the hour is always representable"),
    )
    .expect("an hour-truncated instant is an exact hourly partition boundary")
}

pub(crate) type JourneyError = Box<dyn std::error::Error + Send + Sync>;

/// Rows carried by each public ingest request in the spill journeys.
pub(crate) const SPILL_INGEST_BATCH_ROWS: usize = 5_000;
/// Public ingest batches durably sealed together during fixture preparation.
pub(crate) const SPILL_BATCHES_PER_SEAL: usize = 10;

/// Returns the one-root managed-memory total represented by a snapshot.
pub(crate) fn managed_memory_used(snapshot: ResourceSnapshot) -> usize {
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
pub(crate) fn spill_system_resources(scratch_bytes: u64) -> SystemResourceSnapshot {
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
pub(crate) fn forge_convergence_system_resources() -> SystemResourceSnapshot {
    let mut snapshot = spill_system_resources(1 << 30);
    snapshot.memory_limit_bytes = 2 << 30;
    snapshot
}

/// Open-loop absolute-deadline schedule for one measured request stream.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FixedRatePacer {
    /// Exact number of offered requests in each second.
    pub(crate) rate_per_sec: u64,
    /// Exact phase duration.
    pub(crate) duration: std::time::Duration,
    /// Maximum independently in-flight requests for this stream.
    pub(crate) max_in_flight: usize,
}

impl FixedRatePacer {
    /// Constructs a non-empty fixed-rate schedule with its own permit pool.
    ///
    /// # Errors
    ///
    /// Returns an error when the rate, duration, or permit count is zero.
    pub(crate) fn new(
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
    pub(crate) fn slots(self) -> u64 {
        self.rate_per_sec.saturating_mul(self.duration.as_secs())
    }

    /// Returns the interval separating adjacent absolute-deadline slots.
    #[must_use]
    pub(crate) fn interval(self) -> std::time::Duration {
        std::time::Duration::from_nanos(1_000_000_000 / self.rate_per_sec)
    }
}

/// Exact terminal accounting for one phase and one request stream.
#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct PhaseCounters {
    /// Absolute-deadline slots offered by the harness.
    pub(crate) scheduled: u64,
    /// Slots dispatched into the public client.
    pub(crate) dispatched: u64,
    /// Requests that reached a terminal result.
    pub(crate) completed: u64,
    /// Slots rejected by harness lateness or permit exhaustion.
    pub(crate) harness_saturated: u64,
    /// Requests that exceeded the bounded drain.
    pub(crate) timed_out: u64,
    /// Requests cancelled after the bounded drain.
    pub(crate) cancelled: u64,
    /// Rows accepted by successful write terminals.
    pub(crate) accepted_rows: u64,
    /// Typed retryable capacity refusals observed by this stream.
    pub(crate) capacity_refusals: u64,
}

/// Serializable evidence from one fresh-cluster paired trial.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct PairedTrialResult {
    /// One-based trial ordinal.
    pub(crate) trial: u64,
    /// Baseline write-stream counters.
    pub(crate) baseline_writes: PhaseCounters,
    /// Overlap write-stream counters.
    pub(crate) overlap_writes: PhaseCounters,
    /// Overlap COUNT-stream counters.
    pub(crate) overlap_queries: PhaseCounters,
    /// Accepted-row rate ratio between overlap and baseline.
    pub(crate) ratio: f64,
    /// Maximum strict visibility delay in milliseconds.
    pub(crate) visibility_ms: u64,
    /// Successful write and COUNT completions in each five-second window.
    pub(crate) progress_windows: Vec<PairedProgressWindow>,
    /// Maxima sampled from production owners every ten milliseconds.
    pub(crate) peaks: PairedPeaks,
}

/// Successful terminal progress observed in one five-second overlap window.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct PairedProgressWindow {
    /// Zero-based five-second window ordinal.
    pub(crate) window: u64,
    /// Successful write terminals in this window.
    pub(crate) writes_completed: u64,
    /// Successful COUNT terminals in this window.
    pub(crate) queries_completed: u64,
}

/// Maxima and invariant limits sampled from production memory owners.
#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct PairedPeaks {
    /// Maximum parent Bifrost bytes.
    pub(crate) parent_bytes: usize,
    /// Parent Bifrost ceiling.
    pub(crate) parent_limit: usize,
    /// Maximum Scribe child bytes.
    pub(crate) scribe_bytes: usize,
    /// Scribe child ceiling.
    pub(crate) scribe_limit: usize,
    /// Maximum Oracle child bytes.
    pub(crate) oracle_bytes: usize,
    /// Oracle child ceiling.
    pub(crate) oracle_limit: usize,
    /// Maximum Oracle admission-reserved bytes.
    pub(crate) oracle_admission_bytes: u64,
}

/// Fixed report retained for Gate 4 integration and pre-review validation.
#[derive(Debug, Serialize)]
pub(crate) struct PairedProgressReport {
    /// Three independent fresh-cluster trials.
    pub(crate) trials: Vec<PairedTrialResult>,
    /// Median of the three accepted-row ratios.
    pub(crate) median_ratio: f64,
    /// Range across the three accepted-row ratios.
    pub(crate) ratio_range: f64,
    /// Median absolute deviation across the three ratios.
    pub(crate) ratio_mad: f64,
}

/// Task-local owner for one production-client baseline/overlap comparison.
pub(crate) struct PairedProgressEngine {
    /// Authenticated public writer transport shared by cloned request tasks.
    pub(crate) writer: BifrostGrpcTransport,
    /// Authenticated public query client configuration.
    pub(crate) reader: WyrdClient,
    /// Fully-qualified table targeted by both streams.
    pub(crate) table: String,
    /// Baseline and overlap write schedule.
    pub(crate) write_pacer: FixedRatePacer,
    /// Independent overlap query schedule.
    pub(crate) query_pacer: FixedRatePacer,
}

/// Terminal result returned by one phase-tagged public request.
pub(crate) enum PairedRequestResult {
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
    pub(crate) fn new(
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
    pub(crate) async fn run_write_phase(&self, phase: u64) -> Result<PhaseCounters, JourneyError> {
        self.run_write_phase_with(phase, self.write_pacer).await
    }

    /// Runs a caller-selected write pacer used by measured and warmup phases.
    ///
    /// # Errors
    ///
    /// Returns the same exact-schedule and public-write errors as the measured
    /// phase owner.
    pub(crate) async fn run_write_phase_with(
        &self,
        phase: u64,
        pacer: FixedRatePacer,
    ) -> Result<PhaseCounters, JourneyError> {
        self.run_write_phase_tracked(phase, pacer, None).await
    }

    /// Runs a write phase while optionally recording five-second completions.
    pub(crate) async fn run_write_phase_tracked(
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
    pub(crate) async fn run_overlap_phase(
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
    pub(crate) async fn run_query_phase_with(
        &self,
        pacer: FixedRatePacer,
    ) -> Result<PhaseCounters, JourneyError> {
        self.run_query_phase_tracked(pacer, None).await
    }

    /// Runs a query phase while optionally recording five-second completions.
    pub(crate) async fn run_query_phase_tracked(
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
pub(crate) async fn paired_count(reader: &WyrdClient, table: &str) -> PairedRequestResult {
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
pub(crate) async fn strict_count_value(
    reader: &WyrdClient,
    table: &str,
) -> Result<u64, JourneyError> {
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
pub(crate) async fn strict_spill_summary(
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
pub(crate) async fn strict_unordered_summary(
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
pub(crate) async fn strict_wide_summary(
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
pub(crate) async fn sample_paired_peaks(
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
pub(crate) async fn collect_paired_tasks(
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
pub(crate) fn validate_phase_counters(
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
pub(crate) fn paired_batch_id(tenant_index: usize, ordinal: u64) -> [u8; 16] {
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
pub(crate) fn record_progress_window(
    progress: Option<&[AtomicU64]>,
    started: tokio::time::Instant,
) {
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
pub(crate) fn paired_ipc() -> Vec<u8> {
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

/// Three fresh production clusters sustain identical fixed-rate writes with
/// and without independent COUNT traffic while retaining exact offered load.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized long-running Postgres progress lane"]
pub(crate) async fn concurrent_ingest_and_count_preserve_progress() {
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

/// Sums every metric sample matching one production family across all labels.
pub(crate) fn sum_metric(
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

/// Builds one deterministic sealed table large enough to exercise native
/// ordered-query spill through the production DataFusion path.
///
/// # Errors
///
/// Returns a catalog, authentication, public ingest, or durable flush error.
pub(crate) async fn prepare_spill_table(
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
pub(crate) fn assert_scribe_fixture_peaks_bounded(
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
pub(crate) async fn wait_scribe_memory_restored(
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

/// Run one public gRPC-ingest to HTTP-query roundtrip and validate the terminal.
pub(crate) async fn public_roundtrip(
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
        let day = current_hour_partition();
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
pub(crate) async fn register_table(
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
            physical_layout: None,
            audit: None,
        })
        .await?;
    Ok(())
}

/// Registers the locked one-column table used only by the paired progress proof.
pub(crate) async fn register_paired_table(
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
            physical_layout: None,
            audit: None,
        })
        .await?;
    Ok(())
}

/// Build one authenticated public client for the fixture tenant.
pub(crate) async fn client(
    server: &wyrd_testing::WyrdTestServer,
    name: &str,
) -> Result<WyrdClient, JourneyError> {
    client_for_tenant(server, server.data_tenant_id(), name).await
}

/// Build one authenticated public client for an explicit tenant.
pub(crate) async fn client_for_tenant(
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
pub(crate) async fn client_from_bootstrap(
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

/// Send one Arrow IPC batch through the public authenticated Gate transport.
pub(crate) async fn ingest(
    client: &WyrdClient,
    table: &str,
    ids: &[i64],
) -> Result<(), JourneyError> {
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
pub(crate) async fn query_statement(
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

/// Persist one foreign-tenant physical row beneath the production provider union.
///
/// # Errors
///
/// Returns an Arrow, Parquet, storage, tenant-SQL, or manifest persistence error.
pub(crate) async fn seed_foreign_hot_row(
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
            partition: vala_bifrost_redux::catalog::layout::TimePartition::new(
                vala_bifrost_redux::catalog::layout::TimeGranularity::Day,
                chrono::DateTime::UNIX_EPOCH,
            )?,
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

/// Drain a public query stream and require its terminal row count to match frames.
pub(crate) async fn query_rows(
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
pub(crate) fn ipc(ids: &[i64]) -> Vec<u8> {
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

/// Encodes production-shaped rows in bounded transport chunks so aggregate
/// reconciliation state, rather than a single ingest request, owns the peak.
pub(crate) fn spill_ipc(ids: &[i64]) -> Vec<u8> {
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
pub(crate) fn unique_table(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::now_v7().simple())
}
