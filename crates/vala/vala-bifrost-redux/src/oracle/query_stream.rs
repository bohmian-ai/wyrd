//! Terminal-aware query stream owner.
//!
//! The stream retains local admission, cancellation, telemetry, and lazy
//! physical batches until exactly one terminal outcome releases owned resources.

use std::sync::Arc;
#[cfg(feature = "test-support")]
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use futures_util::StreamExt;

use super::*;

/// Owned query stream handle.  Frame production remains lazy and cancellation-aware.
pub struct OracleQueryStream {
    /// Stable fingerprint known before the first transport byte is emitted.
    pub schema_fingerprint: String,
    /// Underlying frame stream.
    pub frames: std::pin::Pin<Box<OracleFrameStream>>,
    /// Cancellation signal used by the stream owner to force awaited cleanup.
    pub(super) cancellation: CancellationToken,
    /// Marker distinguishing explicit owner cancellation from client drop.
    telemetry_cancelled: Arc<std::sync::atomic::AtomicBool>,
    /// Query-keyed lifecycle observation used only by test-tier transports.
    #[cfg(feature = "test-support")]
    resource_probe: Option<Arc<super::admission::QueryResourceProbe>>,
}

/// Exactly-once lifecycle owner for a public Gate query stream.
///
/// Gate creates this owner before Oracle admission and attaches it to the
/// returned stream. The owner records the terminal outcome when the stream
/// emits its terminal frame; dropping it before then records cancellation.
pub struct QueryStreamLifecycle {
    started_at: Instant,
    finished: AtomicBool,
    finish_callback: Arc<dyn Fn(&'static str, Duration) + Send + Sync>,
    /// Production span retained for the complete public stream lifetime.
    span: tracing::Span,
    #[cfg(feature = "test-support")]
    observer: Arc<QueryLifecycleObserver>,
}

/// Process-local test observer notified after Gate lifecycle accounting commits.
#[cfg(feature = "test-support")]
#[derive(Debug, Default)]
pub struct QueryLifecycleObserver {
    /// Number of terminal lifecycle outcomes recorded.
    completed: std::sync::atomic::AtomicU64,
    /// Number of outcomes normalized to the canonical cancelled label.
    cancelled: std::sync::atomic::AtomicU64,
    /// Wakes test waiters after record-before-notify completion.
    notify: tokio::sync::Notify,
}

#[cfg(feature = "test-support")]
impl QueryLifecycleObserver {
    /// Return the number of completed query lifecycle outcomes.
    #[must_use]
    pub fn completed(&self) -> u64 {
        self.completed.load(Ordering::Acquire)
    }

    /// Return the number of lifecycle owners that recorded cancellation.
    #[must_use]
    pub fn cancelled(&self) -> u64 {
        self.cancelled.load(Ordering::Acquire)
    }

    /// Wait until at least `target` outcomes have committed.
    pub async fn wait_for_at_least(&self, target: u64) {
        while self.completed() < target {
            self.notify.notified().await;
        }
    }

    /// Wait until at least `target` query lifecycles record cancellation.
    pub async fn wait_for_cancelled_at_least(&self, target: u64) {
        while self.cancelled() < target {
            self.notify.notified().await;
        }
    }

    /// Publish one completed lifecycle outcome and wake bounded waiters.
    fn record(&self, outcome: &'static str) {
        if outcome == "cancelled" {
            self.cancelled.fetch_add(1, Ordering::AcqRel);
        }
        self.completed.fetch_add(1, Ordering::AcqRel);
        self.notify.notify_waiters();
    }
}

#[cfg(feature = "test-support")]
static QUERY_LIFECYCLE_OBSERVER: OnceLock<Arc<QueryLifecycleObserver>> = OnceLock::new();

/// Return the shared test observer for completed Gate query lifecycles.
#[cfg(feature = "test-support")]
#[must_use]
pub fn query_lifecycle_observer_for_test() -> Arc<QueryLifecycleObserver> {
    Arc::clone(QUERY_LIFECYCLE_OBSERVER.get_or_init(|| Arc::new(QueryLifecycleObserver::default())))
}

impl QueryStreamLifecycle {
    /// Create a lifecycle owner with a callback for the Gate metric family.
    #[must_use]
    pub fn new(finish_callback: impl Fn(&'static str, Duration) + Send + Sync + 'static) -> Self {
        Self {
            started_at: Instant::now(),
            finished: AtomicBool::new(false),
            finish_callback: Arc::new(finish_callback),
            span: tracing::info_span!("bifrost.gate.query.stream", outcome = tracing::field::Empty),
            #[cfg(feature = "test-support")]
            observer: query_lifecycle_observer_for_test(),
        }
    }

    /// Record one terminal outcome; repeated calls are ignored.
    pub fn finish(&self, outcome: &'static str) {
        if !self.finished.swap(true, Ordering::AcqRel) {
            self.span.record("outcome", outcome);
            (self.finish_callback)(outcome, self.started_at.elapsed());
            #[cfg(feature = "test-support")]
            self.observer.record(outcome);
        }
    }
}

impl Drop for QueryStreamLifecycle {
    /// Records cancellation when the client drops before a terminal frame.
    fn drop(&mut self) {
        if !self.finished.swap(true, Ordering::AcqRel) {
            self.span.record("outcome", "cancelled");
            (self.finish_callback)("cancelled", self.started_at.elapsed());
            #[cfg(feature = "test-support")]
            self.observer.record("cancelled");
        }
    }
}

impl std::fmt::Debug for OracleQueryStream {
    /// Formats only the stream owner label without exposing schema, frames,
    /// cancellation, or lifecycle state; formatting never consumes or polls
    /// the lazy frame stream.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OracleQueryStream")
            .finish_non_exhaustive()
    }
}

impl OracleQueryStream {
    /// Attaches a Gate lifecycle to a stream selected through an external forwarding owner.
    ///
    /// The wrapper observes the real terminal frame and otherwise retains the lifecycle until
    /// client cancellation or stream drop, preserving exactly-once terminal accounting.
    #[must_use]
    pub fn with_gate_lifecycle(mut self, lifecycle: Arc<QueryStreamLifecycle>) -> Self {
        let mut frames = self.frames;
        self.frames = Box::pin(async_stream::stream! {
            while let Some(frame) = frames.next().await {
                if let Ok(QueryStreamFrame::Terminal(terminal)) = &frame {
                    let outcome = match terminal.outcome {
                        QueryTerminalOutcome::Success => "success",
                        QueryTerminalOutcome::Degraded => "degraded",
                        QueryTerminalOutcome::Failed => "failed",
                    };
                    lifecycle.finish(outcome);
                }
                yield frame;
            }
        });
        self
    }
}

/// Complete owned inputs for one terminal-aware query stream.
pub(super) struct QueryStreamInput {
    /// Public output schema encoded before admission ownership transfers.
    pub(super) schema_frame: QuerySchemaFrame,
    /// Lazy physical batch stream.
    pub(super) batches: SendableRecordBatchStream,
    /// Pre-byte lookahead result.
    pub(super) first: Option<Result<RecordBatch, datafusion::error::DataFusionError>>,
    /// Stream-owned durable and local admission capacity.
    pub(super) admitted: AdmittedQueryGuard,
    /// Absolute execution deadline.
    pub(super) deadline: Instant,
    /// Requested visibility contract.
    pub(super) visibility: VisibilityMode,
    /// Public freshness policy applied only after aggregate disposition selection.
    pub(super) freshness_policy: wyrd_spec::vala::api::FreshnessPolicy,
    /// Exact source tiers completed as eligible degraded losses.
    pub(super) degraded_sources: super::DegradedSourceAccumulator,
    /// Whether the one stale-cut replan was consumed.
    pub(super) stale_replanned: bool,
    /// Production query telemetry retained through terminal emission.
    pub(super) query_telemetry: QueryTelemetryGuard,
    /// Final physical scan evidence retained through terminal emission.
    pub(super) scan_stats: OracleQueryScanStats,
    /// Optional Gate lifecycle retained through frame consumption.
    pub(super) gate_lifecycle: Option<Arc<QueryStreamLifecycle>>,
    /// Exactly-once owner-local registry settlement retained through terminal output.
    pub(super) running_query: Option<RunningQueryTerminalOwner>,
}

/// Exactly-once terminal owner for one inserted running-query entry.
pub(super) struct RunningQueryTerminalOwner {
    /// Exact process-local registry used by lifecycle controls.
    registry: Arc<RunningQueryRegistry>,
    /// Authenticated tenant key.
    tenant_id: wyrd_spec::DataTenantId,
    /// Public request identity.
    request_id: wyrd_spec::request_id::RequestId,
    /// Whether terminal settlement already removed the entry.
    settled: bool,
}

impl RunningQueryTerminalOwner {
    /// Retains the identity of one entry already inserted before dispatch.
    #[must_use]
    pub(super) fn new(
        registry: Arc<RunningQueryRegistry>,
        tenant_id: wyrd_spec::DataTenantId,
        request_id: wyrd_spec::request_id::RequestId,
    ) -> Self {
        Self {
            registry,
            tenant_id,
            request_id,
            settled: false,
        }
    }

    /// Removes the active entry with the truthful terminal classification once.
    fn finish(&mut self, outcome: QueryTerminalOutcome) {
        if !self.settled {
            let _ = self
                .registry
                .settle_terminal(self.tenant_id, &self.request_id, outcome);
            self.settled = true;
        }
    }
}

impl Drop for RunningQueryTerminalOwner {
    /// Settles an abandoned execution as failed without blocking or spawning cleanup.
    fn drop(&mut self) {
        self.finish(QueryTerminalOutcome::Failed);
    }
}

/// One cancellation-, timeout-, or batch-aware stream step.
enum QueryStreamEvent {
    /// Next physical batch result, or `None` when execution completed.
    Batch(Option<Result<RecordBatch, datafusion::error::DataFusionError>>),
    /// Stable terminal failure selected before another batch is exposed.
    Failed(QueryTerminalErrorCode),
}

/// Inputs retained by the lazy frame stream until terminal cleanup.
struct FrameBuildInput {
    /// Encoded public schema frame.
    schema_frame: QuerySchemaFrame,
    /// Physical batch stream.
    batches: SendableRecordBatchStream,
    /// Optional pre-read batch.
    first: Option<Result<RecordBatch, datafusion::error::DataFusionError>>,
    /// Admission guard retained through terminal output.
    admitted: AdmittedQueryGuard,
    /// Absolute stream deadline.
    deadline: Instant,
    /// Visibility contract for terminal mapping.
    visibility: VisibilityMode,
    /// Public freshness policy applied after partition aggregation.
    freshness_policy: wyrd_spec::vala::api::FreshnessPolicy,
    /// Exact source tiers completed as eligible degraded losses.
    degraded_sources: super::DegradedSourceAccumulator,
    /// Whether stale replanning was consumed.
    stale_replanned: bool,
    /// Query telemetry guard.
    query_telemetry: super::QueryTelemetryGuard,
    /// Optional gate lifecycle guard.
    gate_lifecycle: Option<Arc<QueryStreamLifecycle>>,
    /// Stream cancellation token.
    stream_cancellation: CancellationToken,
    /// Request cancellation token.
    request_cancellation: CancellationToken,
    /// Cancellation marker shared with telemetry.
    stream_telemetry_cancelled: Arc<std::sync::atomic::AtomicBool>,
    /// Exactly-once active-registry terminal owner.
    running_query: Option<RunningQueryTerminalOwner>,
}

/// Builds the lazy frame stream that owns terminal cleanup state.
fn build_frames(input: FrameBuildInput) -> std::pin::Pin<Box<super::OracleFrameStream>> {
    let FrameBuildInput {
        schema_frame,
        mut batches,
        first,
        admitted,
        deadline,
        visibility,
        freshness_policy,
        degraded_sources,
        stale_replanned,
        mut query_telemetry,
        gate_lifecycle,
        stream_cancellation,
        request_cancellation,
        stream_telemetry_cancelled,
        mut running_query,
    } = input;
    let frames = async_stream::stream! {
        let distributed_settlement = Arc::clone(&admitted.distributed_settlement);
        let mut admitted = Some(admitted);
        let mut next = first;
        let mut row_count = 0_u64;
        query_telemetry.record_payload(0, schema_frame.arrow_ipc_schema.len());
        yield Ok(QueryStreamFrame::Schema(schema_frame));
        let candidate = loop {
            let event = if cancellation_requested(&stream_cancellation, &request_cancellation) {
                QueryStreamEvent::Failed(QueryTerminalErrorCode::QueryExecutionFailed)
            } else if let Some(value) = next.take() {
                QueryStreamEvent::Batch(Some(value))
            } else {
                next_query_stream_event(
                    &mut batches,
                    &stream_cancellation,
                    &request_cancellation,
                    deadline,
                ).await
            };
            match event {
                QueryStreamEvent::Batch(Some(Ok(batch))) => {
                    query_telemetry.first_batch();
                    let batch_rows = batch.num_rows() as u64;
                    row_count = row_count.saturating_add(batch_rows);
                    let Ok(frame) = encode_batch_frame(&batch) else {
                        break failed_terminal_for_visibility(
                            QueryTerminalErrorCode::QueryExecutionFailed,
                            row_count,
                            visibility,
                        );
                    };
                    query_telemetry.record_payload(batch_rows, frame.arrow_ipc_batch.len());
                    yield Ok(QueryStreamFrame::Batch(frame));
                }
                QueryStreamEvent::Batch(Some(Err(error))) => {
                    let code = terminal_error_code(&error);
                    tracing::error!(error = %error, error_code = terminal_error_label(code), "Oracle query stream execution failed");
                    break failed_terminal_for_visibility(code, row_count, visibility);
                }
                QueryStreamEvent::Batch(None) => {
                    break exhausted_terminal(
                        &degraded_sources,
                        visibility,
                        freshness_policy,
                        stale_replanned,
                        row_count,
                    );
                }
                QueryStreamEvent::Failed(code) => {
                    break failed_terminal_for_visibility(code, row_count, visibility);
                }
            }
        };
        let failed_outcome = settle_failed_cancellation(
            candidate.outcome,
            &stream_telemetry_cancelled,
            &request_cancellation,
            &stream_cancellation,
        );
        drop(next);
        drop(batches);
        settle_distributed(&distributed_settlement, candidate.outcome, &stream_cancellation).await;
        let terminal = release_and_finish_terminal(
            &mut admitted,
            &mut query_telemetry,
            gate_lifecycle.as_ref(),
            candidate,
            failed_outcome,
            visibility,
            row_count,
        );
        if let Some(owner) = &mut running_query {
            owner.finish(terminal.outcome);
        }
        yield Ok(QueryStreamFrame::Terminal(terminal));
    };
    Box::pin(frames)
}

/// Ordered degradation observed by one distributed query.
///
/// The two projections are taken from the same ordinal-sorted accumulator so
/// the reported sources and the reported reasons describe the same partitions
/// in the same order.
struct ObservedDegradation {
    /// Deduplicated, sorted source classes lost across every degraded partition.
    sources: Vec<QuerySource>,
    /// Closed reason labels in participant-ordinal order, one per degraded partition.
    reasons: Vec<&'static str>,
}

/// Collects the sources lost and the reasons recorded across degraded partitions.
///
/// Entries are ordered by participant ordinal so the observed degradation order
/// is stable across runs. Sources are flattened to a deduplicated, sorted list
/// because the terminal reports which source classes were lost, not which
/// partition lost them; the reason labels stay per-partition and in ordinal
/// order so a degraded terminal can name why each partition degraded rather
/// than only that something did. A poisoned accumulator yields empty
/// projections rather than failing a query that has already produced its rows.
fn collect_degraded_sources(
    degraded_sources: &super::DegradedSourceAccumulator,
) -> ObservedDegradation {
    degraded_sources.lock().map_or_else(
        |_| ObservedDegradation {
            sources: Vec::new(),
            reasons: Vec::new(),
        },
        |entries| {
            let mut entries = entries.clone();
            entries.sort_by_key(|entry| entry.ordinal);
            let reasons = entries.iter().map(|entry| entry.reason).collect::<Vec<_>>();
            let mut sources = entries
                .into_iter()
                .flat_map(|entry| entry.sources)
                .collect::<Vec<_>>();
            sources.sort();
            sources.dedup();
            ObservedDegradation { sources, reasons }
        },
    )
}

/// Cancels the request and stream on a failed terminal and names the outcome.
///
/// Cancellation is driven from the terminal rather than from the error site so
/// every failure path converges here: the request token stops any work the
/// caller still owns and the stream token stops the distributed children, in
/// that order, before settlement joins them. A successful terminal cancels
/// nothing and reports the fixed `"failed"` label the caller ignores.
fn settle_failed_cancellation(
    outcome: QueryTerminalOutcome,
    stream_telemetry_cancelled: &Arc<AtomicBool>,
    request_cancellation: &CancellationToken,
    stream_cancellation: &CancellationToken,
) -> &'static str {
    if outcome != QueryTerminalOutcome::Failed {
        return "failed";
    }
    let failed_outcome = failed_stream_outcome(stream_telemetry_cancelled);
    request_cancellation.cancel();
    stream_cancellation.cancel();
    failed_outcome
}

/// Closed proof returned by one explicit local admission-release attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AdmissionReleaseOutcome {
    /// Local permits and reservations were released before terminal emission.
    Released,
}

/// Waits for the next physical batch while enforcing cancellation and deadline.
///
/// Query cancellation wins over starting another read when already observable.
/// While waiting, cancellation and the absolute deadline race the physical stream;
/// any earlier batches remain accounted by the owner, but no additional batch is
/// exposed after a cancellation or timeout event is selected.
async fn next_query_stream_event(
    batches: &mut SendableRecordBatchStream,
    cancellation: &CancellationToken,
    request_cancellation: &CancellationToken,
    deadline: Instant,
) -> QueryStreamEvent {
    if cancellation.is_cancelled() || request_cancellation.is_cancelled() {
        return QueryStreamEvent::Failed(QueryTerminalErrorCode::QueryExecutionFailed);
    }
    let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
        return QueryStreamEvent::Failed(QueryTerminalErrorCode::QueryTimeout);
    };
    tokio::select! {
        () = cancellation.cancelled() => {
            QueryStreamEvent::Failed(QueryTerminalErrorCode::QueryExecutionFailed)
        }
        () = request_cancellation.cancelled() => {
            QueryStreamEvent::Failed(QueryTerminalErrorCode::QueryExecutionFailed)
        }
        () = tokio::time::sleep(remaining) => {
            QueryStreamEvent::Failed(QueryTerminalErrorCode::QueryTimeout)
        }
        value = batches.next() => QueryStreamEvent::Batch(value),
    }
}

/// Reports whether either independent query cancellation edge has fired.
fn cancellation_requested(
    cancellation: &CancellationToken,
    request_cancellation: &CancellationToken,
) -> bool {
    cancellation.is_cancelled() || request_cancellation.is_cancelled()
}

/// Settles every distributed child before the terminal frame is emitted.
///
/// A failed terminal cancels first, because the remaining children have no
/// consumer and would otherwise keep RPCs and reservations alive past the
/// query that owns them; any other terminal joins without cancelling so
/// in-flight children finish their own release. Either way this awaits to
/// completion, which is what guarantees no nested resource child outlives the
/// query owner.
async fn settle_distributed(
    settlement: &Arc<super::admission::DistributedQuerySettlement>,
    outcome: QueryTerminalOutcome,
    stream_cancellation: &CancellationToken,
) {
    if outcome == QueryTerminalOutcome::Failed {
        settlement.cancel_and_join(stream_cancellation).await;
    } else {
        settlement.join().await;
    }
}

/// Builds the terminal for a stream that exhausted its batches normally.
///
/// Draining the accumulator here — rather than at each degradation site — is
/// what makes the reported degradation whole: every partition that degraded has
/// already recorded itself by the time the batch stream ends. The accumulated
/// per-partition reasons are reported once, in participant-ordinal order,
/// because the terminal frame itself carries only the closed [`QueryWarning`]
/// set and cannot name which partitions degraded or why.
///
/// [`QueryWarning`]: wyrd_spec::vala::api::QueryWarning
fn exhausted_terminal(
    degraded_sources: &super::DegradedSourceAccumulator,
    visibility: VisibilityMode,
    freshness_policy: wyrd_spec::vala::api::FreshnessPolicy,
    stale_replanned: bool,
    row_count: u64,
) -> QueryTerminalFrame {
    let degraded = collect_degraded_sources(degraded_sources);
    if !degraded.reasons.is_empty() {
        tracing::warn!(
            degraded_reasons = ?degraded.reasons,
            degraded_sources = ?degraded.sources,
            "Oracle query completed with degraded partitions"
        );
    }
    successful_terminal(
        visibility,
        freshness_policy,
        &degraded.sources,
        stale_replanned,
        row_count,
    )
}

/// Constructs the validated success/degraded terminal for one completed stream.
///
/// Wyrd is a warehouse surface, so a completed stream may only report
/// `Success` when every source the requested [`VisibilityMode`] reads
/// contributed its rows. Loss is scoped to the request: a live-tail
/// degradation is irrelevant to a `PublishedOnly` query, which never opens
/// the fenced tail interval.
///
/// Two losses are distinguished because the wire contract admits only one
/// representation for each:
///
/// * A sealed loss (`Iceberg` or `HotSealed`) always fails. `validate`
///   requires both sealed entries to be `Complete`, so there is no way to
///   report a short answer over persisted data honestly, and
///   [`FreshnessPolicy::AllowDegraded`] does not license one.
/// * A live-tail loss under `Fused` fails under
///   [`FreshnessPolicy::Strict`] and degrades under
///   [`FreshnessPolicy::AllowDegraded`], which is the explicit
///   observability opt-in to a bounded incomplete answer.
fn successful_terminal(
    visibility: VisibilityMode,
    freshness_policy: wyrd_spec::vala::api::FreshnessPolicy,
    degraded_sources: &[QuerySource],
    stale_replanned: bool,
    row_count: u64,
) -> QueryTerminalFrame {
    let live_tail_lost =
        visibility == VisibilityMode::Fused && degraded_sources.contains(&QuerySource::LiveTail);
    let sealed_lost = degraded_sources
        .iter()
        .any(|source| *source != QuerySource::LiveTail);
    if sealed_lost
        || (live_tail_lost && freshness_policy == wyrd_spec::vala::api::FreshnessPolicy::Strict)
    {
        return failed_terminal_for_visibility(
            QueryTerminalErrorCode::QueryVisibilityUnavailable,
            row_count,
            visibility,
        );
    }
    let freshness = if live_tail_lost {
        QueryFreshness::Degraded
    } else {
        QueryFreshness::Complete
    };
    let outcome = if live_tail_lost {
        QueryTerminalOutcome::Degraded
    } else {
        QueryTerminalOutcome::Success
    };
    let mut warnings = Vec::new();
    if live_tail_lost {
        warnings.push(wyrd_spec::vala::api::QueryWarning::LiveTailUnavailable);
    }
    if stale_replanned {
        warnings.push(wyrd_spec::vala::api::QueryWarning::StaleCutReplanned);
    }
    let mut source_completion = vec![
        SourceCompletion {
            source: QuerySource::Iceberg,
            outcome: SourceCompletionOutcome::Complete,
        },
        SourceCompletion {
            source: QuerySource::HotSealed,
            outcome: SourceCompletionOutcome::Complete,
        },
    ];
    if visibility == VisibilityMode::Fused {
        source_completion.push(SourceCompletion {
            source: QuerySource::LiveTail,
            outcome: if live_tail_lost {
                SourceCompletionOutcome::Unavailable
            } else {
                SourceCompletionOutcome::Complete
            },
        });
    }
    QueryTerminalFrame {
        outcome,
        freshness,
        row_count,
        warnings,
        source_completion,
        error: None,
    }
}

/// Encodes one Arrow schema frame and its stable fingerprint.
///
/// # Errors
///
/// Returns query execution failure when Arrow IPC rejects the schema.
pub(super) fn encode_schema_frame(schema: &SchemaRef) -> Result<QuerySchemaFrame, BifrostError> {
    let mut bytes = Vec::new();
    let mut writer = arrow::ipc::writer::StreamWriter::try_new(&mut bytes, schema)
        .map_err(|_| BifrostError::QueryExecutionFailed)?;
    writer
        .finish()
        .map_err(|_| BifrostError::QueryExecutionFailed)?;
    Ok(QuerySchemaFrame {
        schema_fingerprint: hex::encode(SchemaFingerprint::from_arrow_schema(schema).0),
        arrow_ipc_schema: bytes,
    })
}

/// Encodes one bounded Arrow record batch frame.
///
/// # Errors
///
/// Returns query execution failure when Arrow IPC rejects the batch.
fn encode_batch_frame(batch: &RecordBatch) -> Result<QueryBatchFrame, BifrostError> {
    let mut bytes = Vec::new();
    let mut writer = arrow::ipc::writer::StreamWriter::try_new(&mut bytes, &batch.schema())
        .map_err(|_| BifrostError::QueryExecutionFailed)?;
    writer
        .write(batch)
        .and_then(|()| writer.finish())
        .map_err(|_| BifrostError::QueryExecutionFailed)?;
    Ok(QueryBatchFrame {
        arrow_ipc_batch: bytes,
    })
}

/// Maps a late `DataFusion` failure to the closed terminal-code catalog.
fn terminal_error_code(error: &datafusion::error::DataFusionError) -> QueryTerminalErrorCode {
    let message = error.to_string().to_ascii_lowercase();
    if message.contains("tenant invariant") {
        QueryTerminalErrorCode::QueryTenantInvariant
    } else if message.contains("reconciliation invariant") {
        QueryTerminalErrorCode::QueryReconciliationInvariant
    } else if message.contains("audit unavailable") {
        QueryTerminalErrorCode::QueryAuditUnavailable
    } else {
        QueryTerminalErrorCode::QueryExecutionFailed
    }
}

/// Selects the terminal outcome for a stream that observed a failed step.
fn failed_stream_outcome(explicit_cancelled: &std::sync::atomic::AtomicBool) -> &'static str {
    if explicit_cancelled.load(std::sync::atomic::Ordering::Acquire) {
        "cancelled"
    } else {
        "failed"
    }
}

/// Finish production query telemetry and the optional Gate lifecycle together.
fn finish_stream(
    telemetry: &mut QueryTelemetryGuard,
    lifecycle: Option<&Arc<QueryStreamLifecycle>>,
    outcome: &'static str,
    status: &'static str,
) {
    telemetry.finish(outcome, status);
    if let Some(lifecycle) = lifecycle {
        lifecycle.finish(outcome);
    }
}

/// Releases stream admission exactly once and returns local completion proof.
fn release_admitted(admitted: &mut Option<AdmittedQueryGuard>) -> AdmissionReleaseOutcome {
    let Some(admitted) = admitted.take() else {
        tracing::error!("Oracle query stream admission owner was already consumed");
        return AdmissionReleaseOutcome::Released;
    };
    admitted.release();
    AdmissionReleaseOutcome::Released
}

/// Releases admission, selects the truthful terminal, then finishes telemetry.
fn release_and_finish_terminal(
    admitted: &mut Option<AdmittedQueryGuard>,
    query_telemetry: &mut QueryTelemetryGuard,
    gate_lifecycle: Option<&Arc<QueryStreamLifecycle>>,
    candidate: QueryTerminalFrame,
    failed_outcome: &'static str,
    _visibility: VisibilityMode,
    _row_count: u64,
) -> QueryTerminalFrame {
    let _release = release_admitted(admitted);
    let terminal = candidate;
    let outcome = if terminal.outcome == QueryTerminalOutcome::Failed {
        failed_outcome
    } else {
        "success"
    };
    let status = if terminal.outcome == QueryTerminalOutcome::Degraded {
        "degraded"
    } else {
        "complete"
    };
    finish_stream(query_telemetry, gate_lifecycle, outcome, status);
    terminal
}

impl OracleQueryStream {
    /// Owns a validated private-forwarding stream at the public ingress replica.
    ///
    /// Dropping this owner drops the underlying tonic stream, propagating cancellation to the
    /// remote Oracle that retains admission, audit, registry, and execution ownership.
    #[must_use]
    pub fn from_forwarded(
        schema_fingerprint: String,
        frames: std::pin::Pin<Box<super::OracleFrameStream>>,
        cancellation: CancellationToken,
    ) -> Self {
        Self::assemble(
            schema_fingerprint,
            frames,
            cancellation,
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
    }

    /// Builds a synthetic stream for transport and collector tests.
    ///
    /// This constructor is available only to crate tests and the repository's
    /// `test-support` feature; production callers receive streams from Oracle
    /// admission and cannot inject lifecycle state.
    #[cfg(any(test, feature = "test-support"))]
    pub fn test_new(
        schema_fingerprint: String,
        frames: std::pin::Pin<Box<super::OracleFrameStream>>,
        cancellation: tokio_util::sync::CancellationToken,
    ) -> Self {
        Self {
            schema_fingerprint,
            frames,
            cancellation,
            telemetry_cancelled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            #[cfg(feature = "test-support")]
            resource_probe: None,
        }
    }

    /// Builds a test stream through the production telemetry and admission owners.
    #[cfg(test)]
    pub(super) fn test_from_physical(
        telemetry: &Arc<super::OracleTelemetry>,
        schema: &arrow::datatypes::SchemaRef,
        batches: datafusion::physical_plan::SendableRecordBatchStream,
        scan_stats: super::OracleQueryScanStats,
    ) -> Self {
        let (admitted, _shared, _request_cancellation) =
            super::admission::admitted_guard_for_test();
        Self::new(QueryStreamInput {
            schema_frame: encode_schema_frame(schema).expect("test schema frame"),
            batches,
            first: None,
            admitted,
            deadline: Instant::now() + Duration::from_secs(1),
            visibility: VisibilityMode::PublishedOnly,
            freshness_policy: wyrd_spec::vala::api::FreshnessPolicy::Strict,
            degraded_sources: Arc::new(std::sync::Mutex::new(Vec::new())),
            stale_replanned: false,
            query_telemetry: telemetry
                .start_query(VisibilityMode::PublishedOnly, QueryClass::Interactive),
            scan_stats,
            gate_lifecycle: None,
            running_query: None,
        })
    }

    /// Creates a lazy stream that owns local admission guards through terminal output.
    ///
    /// The returned future retains all cleanup state until the terminal frame
    /// is emitted or the stream is dropped, so cancellation cannot detach a
    /// permit or reservation from its query owner. The caller supplies
    /// the encoded schema, leaving no fallible work after guard transfer.
    pub(super) fn new(input: QueryStreamInput) -> Self {
        let _stream_span = tracing::info_span!("bifrost.oracle.stream").entered();
        let QueryStreamInput {
            schema_frame,
            batches,
            first,
            admitted,
            deadline,
            visibility,
            freshness_policy,
            degraded_sources,
            stale_replanned,
            mut query_telemetry,
            scan_stats,
            gate_lifecycle,
            running_query,
        } = input;
        #[cfg(feature = "test-support")]
        let mut admitted = admitted;
        #[cfg(feature = "test-support")]
        let resource_probe = Some(admitted.attach_resource_probe());
        let schema_fingerprint = schema_frame.schema_fingerprint.clone();
        let cancellation = admitted.cancellation.clone();
        let request_cancellation = admitted.request_cancellation.clone();
        let stream_cancellation = cancellation.clone();
        let telemetry_cancelled = query_telemetry.cancellation_marker();
        let stream_telemetry_cancelled = Arc::clone(&telemetry_cancelled);
        query_telemetry.record_scan_stats(scan_stats);
        query_telemetry.start_stream();
        let frames = build_frames(FrameBuildInput {
            schema_frame,
            batches,
            first,
            admitted,
            deadline,
            visibility,
            freshness_policy,
            degraded_sources,
            stale_replanned,
            query_telemetry,
            gate_lifecycle,
            stream_cancellation,
            request_cancellation,
            stream_telemetry_cancelled,
            running_query,
        });
        let stream = Self::assemble(
            schema_fingerprint,
            frames,
            cancellation,
            telemetry_cancelled,
        );
        #[cfg(feature = "test-support")]
        let stream = stream.with_resource_probe(resource_probe);
        stream
    }

    /// Assemble the stream owner from its schema, frame source, and cancellation edge.
    fn assemble(
        schema_fingerprint: String,
        frames: std::pin::Pin<Box<super::OracleFrameStream>>,
        cancellation: CancellationToken,
        telemetry_cancelled: Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        Self {
            schema_fingerprint,
            frames,
            cancellation,
            telemetry_cancelled,
            #[cfg(feature = "test-support")]
            resource_probe: None,
        }
    }

    #[cfg(feature = "test-support")]
    /// Attach query-keyed resource evidence without changing stream behavior.
    fn with_resource_probe(
        mut self,
        resource_probe: Option<Arc<super::admission::QueryResourceProbe>>,
    ) -> Self {
        self.resource_probe = resource_probe;
        self
    }

    /// Returns this stream's query-identity-keyed lifecycle probe for tests.
    ///
    /// # Panics
    ///
    /// Panics only when a synthetic stream bypasses the production constructor.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn resource_probe_for_test(&self) -> Arc<super::admission::QueryResourceProbe> {
        Arc::clone(
            self.resource_probe
                .as_ref()
                .expect("production Oracle streams attach a resource probe under test-support"),
        )
    }

    /// Signals cancellation and drains to a terminal frame under a short bound.
    ///
    /// This method is intentionally infallible: a timeout leaves local Drop
    /// cleanup in place while local ownership remains authoritative.
    pub async fn cancel(mut self) {
        self.telemetry_cancelled
            .store(true, std::sync::atomic::Ordering::Release);
        self.cancellation.cancel();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while self.frames.next().await.is_some() {}
        })
        .await;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    use arrow::datatypes::Schema;
    use datafusion::physical_plan::stream::RecordBatchStreamAdapter;
    use futures_util::StreamExt;
    use tokio_util::sync::CancellationToken;
    use wyrd_spec::vala::api::{FreshnessPolicy, QueryWarning};

    use super::{OracleQueryStream, QueryStreamInput, QueryStreamLifecycle, successful_terminal};
    use crate::oracle::admission::{active_queries_for_test, admitted_guard_for_test};
    use crate::oracle::exec::OracleQueryScanStats;
    use crate::oracle::{
        BifrostError, OracleSlotManager, OracleTelemetry, QueryClass, QueryFreshness,
        QuerySchemaFrame, QuerySource, QueryStreamFrame, QueryTerminalErrorCode,
        QueryTerminalOutcome, VisibilityMode,
    };
    use crate::test_support::{SpanCaptureSubscriber, has_span_outcome};

    /// Terminal construction scopes every source loss to the requested
    /// visibility and freshness policy.
    ///
    /// The matrix pins the three decisions that separate a warehouse answer
    /// from an observability answer: a requested loss never reports
    /// `Success`, `AllowDegraded` is the only policy that tolerates one, and
    /// a source the request never reads is not a loss at all.
    #[test]
    fn successful_terminal_scopes_loss_to_requested_visibility_and_policy() {
        let complete = successful_terminal(
            VisibilityMode::Fused,
            FreshnessPolicy::Strict,
            &[],
            false,
            3,
        );
        assert_eq!(complete.outcome, QueryTerminalOutcome::Success);
        complete
            .validate(VisibilityMode::Fused)
            .expect("complete terminal validates");

        // Row 1: Fused + LiveTail loss. Strict fails closed; AllowDegraded
        // is the explicit opt-in to a bounded incomplete answer.
        let strict_live = successful_terminal(
            VisibilityMode::Fused,
            FreshnessPolicy::Strict,
            &[QuerySource::LiveTail],
            false,
            2,
        );
        assert_eq!(strict_live.outcome, QueryTerminalOutcome::Failed);
        assert_eq!(
            strict_live.error.as_ref().map(|error| error.code),
            Some(QueryTerminalErrorCode::QueryVisibilityUnavailable)
        );
        strict_live
            .validate(VisibilityMode::Fused)
            .expect("strict live-tail loss fails with the parent terminal contract");

        let allowed_live = successful_terminal(
            VisibilityMode::Fused,
            FreshnessPolicy::AllowDegraded,
            &[QuerySource::LiveTail],
            false,
            2,
        );
        assert_eq!(allowed_live.outcome, QueryTerminalOutcome::Degraded);
        assert_eq!(allowed_live.freshness, QueryFreshness::Degraded);
        assert_eq!(
            allowed_live.warnings,
            vec![QueryWarning::LiveTailUnavailable]
        );
        allowed_live
            .validate(VisibilityMode::Fused)
            .expect("eligible live-tail degradation validates");

        // Row 2: Fused + sealed loss. The wire contract requires both sealed
        // entries to be Complete, so neither policy may report a short
        // answer over persisted data.
        for source in [QuerySource::Iceberg, QuerySource::HotSealed] {
            for policy in [FreshnessPolicy::Strict, FreshnessPolicy::AllowDegraded] {
                let failed =
                    successful_terminal(VisibilityMode::Fused, policy, &[source], false, 0);
                assert_eq!(
                    failed.outcome,
                    QueryTerminalOutcome::Failed,
                    "{source:?} loss under {policy:?} must fail"
                );
                assert_eq!(
                    failed.error.as_ref().map(|error| error.code),
                    Some(QueryTerminalErrorCode::QueryVisibilityUnavailable)
                );
                failed
                    .validate(VisibilityMode::Fused)
                    .expect("sealed loss fails with the parent terminal contract");
            }
        }

        // Row 3: PublishedOnly never opens the fenced tail interval, so a
        // live-tail degradation is not a loss for that request.
        for policy in [FreshnessPolicy::Strict, FreshnessPolicy::AllowDegraded] {
            let unaffected = successful_terminal(
                VisibilityMode::PublishedOnly,
                policy,
                &[QuerySource::LiveTail],
                false,
                4,
            );
            assert_eq!(unaffected.outcome, QueryTerminalOutcome::Success);
            assert_eq!(unaffected.freshness, QueryFreshness::Complete);
            assert!(unaffected.warnings.is_empty());
            unaffected
                .validate(VisibilityMode::PublishedOnly)
                .expect("unrequested live-tail loss does not affect a published-only terminal");
        }
    }

    /// Synthetic owner state used to exercise stream cancellation ordering.
    #[derive(Default)]
    struct AdmissionReleaseProbe {
        /// Number of local slots still held by the synthetic owner.
        slots_in_use: AtomicUsize,
        /// Whether the owner completed local admission release.
        release_complete: AtomicBool,
        /// Whether all fenced tail state was released.
        fence_released: AtomicBool,
    }

    impl AdmissionReleaseProbe {
        /// Marks every owner resource released in the same order as admission.
        async fn release(&self) {
            tokio::task::yield_now().await;
            self.slots_in_use.store(0, Ordering::Release);
            self.fence_released.store(true, Ordering::Release);
            self.release_complete.store(true, Ordering::Release);
        }
    }

    /// Wraps frames in an owner that must release after cancellation drains.
    fn owner_stream(
        frames: Vec<Result<QueryStreamFrame, BifrostError>>,
        owner: Arc<AdmissionReleaseProbe>,
        cancellation: CancellationToken,
    ) -> OracleQueryStream {
        let frames = async_stream::stream! {
            for frame in frames {
                yield frame;
            }
            owner.release().await;
        };
        OracleQueryStream::test_new("probe".to_owned(), Box::pin(frames), cancellation)
    }

    /// Asserts cancellation completed owner release before returning.
    fn assert_owner_released(owner: &AdmissionReleaseProbe) {
        assert_eq!(owner.slots_in_use.load(Ordering::Acquire), 0);
        assert!(owner.release_complete.load(Ordering::Acquire));
        assert!(owner.fence_released.load(Ordering::Acquire));
    }

    /// A success terminal is observable only after local ownership is released.
    #[tokio::test]
    async fn success_terminal_requires_completed_local_release() {
        let (admitted, shared, _request_cancellation) = admitted_guard_for_test();
        let telemetry_owner =
            Arc::new(OracleTelemetry::new(Arc::new(OracleSlotManager::new(1, 1))));
        let telemetry =
            telemetry_owner.start_query(VisibilityMode::PublishedOnly, QueryClass::Interactive);
        let mut stream = OracleQueryStream::new(QueryStreamInput {
            schema_frame: QuerySchemaFrame {
                schema_fingerprint: "production".to_owned(),
                arrow_ipc_schema: Vec::new(),
            },
            batches: Box::pin(RecordBatchStreamAdapter::new(
                Arc::new(Schema::empty()),
                futures_util::stream::empty(),
            )),
            first: None,
            admitted,
            deadline: Instant::now() + Duration::from_secs(1),
            visibility: VisibilityMode::PublishedOnly,
            freshness_policy: FreshnessPolicy::Strict,
            degraded_sources: Arc::new(std::sync::Mutex::new(Vec::new())),
            stale_replanned: false,
            query_telemetry: telemetry,
            scan_stats: OracleQueryScanStats::default(),
            gate_lifecycle: None,
            running_query: None,
        });
        let mut terminal_seen = false;
        while let Some(Ok(frame)) = stream.frames.next().await {
            if matches!(frame, QueryStreamFrame::Terminal(_)) {
                terminal_seen = true;
                assert_eq!(active_queries_for_test(&shared), 0);
            }
        }
        assert!(terminal_seen);
        assert_eq!(active_queries_for_test(&shared), 0);
    }

    /// A typed stale object observed after schema output fails without replacement.
    #[tokio::test]
    async fn post_output_typed_stale_object_is_terminal_without_replan() {
        let (admitted, shared, _request_cancellation) = admitted_guard_for_test();
        let telemetry_owner =
            Arc::new(OracleTelemetry::new(Arc::new(OracleSlotManager::new(1, 1))));
        let stale = crate::oracle::exec::iceberg_datafusion_error(std::io::Error::from(
            std::io::ErrorKind::NotFound,
        ));
        let mut stream = OracleQueryStream::new(QueryStreamInput {
            schema_frame: QuerySchemaFrame {
                schema_fingerprint: "post-output-stale".to_owned(),
                arrow_ipc_schema: Vec::new(),
            },
            batches: Box::pin(RecordBatchStreamAdapter::new(
                Arc::new(Schema::empty()),
                futures_util::stream::once(std::future::ready(Err(stale))),
            )),
            first: None,
            admitted,
            deadline: Instant::now() + Duration::from_secs(1),
            visibility: VisibilityMode::PublishedOnly,
            freshness_policy: FreshnessPolicy::Strict,
            degraded_sources: Arc::new(std::sync::Mutex::new(Vec::new())),
            stale_replanned: false,
            query_telemetry: telemetry_owner
                .start_query(VisibilityMode::PublishedOnly, QueryClass::Interactive),
            scan_stats: OracleQueryScanStats::default(),
            gate_lifecycle: None,
            running_query: None,
        });
        assert!(matches!(
            stream.frames.next().await,
            Some(Ok(QueryStreamFrame::Schema(_)))
        ));
        let terminal = match stream.frames.next().await {
            Some(Ok(QueryStreamFrame::Terminal(terminal))) => terminal,
            other => panic!("expected failed terminal after stale batch, got {other:?}"),
        };
        assert_eq!(terminal.outcome, QueryTerminalOutcome::Failed);
        assert!(!terminal.warnings.contains(&QueryWarning::StaleCutReplanned));
        assert_eq!(active_queries_for_test(&shared), 0);
    }

    /// Caller cancellation reaches the production stream without canceling siblings.
    #[tokio::test]
    async fn request_cancellation_interrupts_production_stream() {
        let (admitted, shared, request_cancellation) = admitted_guard_for_test();
        let telemetry_owner =
            Arc::new(OracleTelemetry::new(Arc::new(OracleSlotManager::new(1, 1))));
        let mut stream = OracleQueryStream::new(QueryStreamInput {
            schema_frame: QuerySchemaFrame {
                schema_fingerprint: "request-cancel".to_owned(),
                arrow_ipc_schema: Vec::new(),
            },
            batches: Box::pin(RecordBatchStreamAdapter::new(
                Arc::new(Schema::empty()),
                futures_util::stream::pending(),
            )),
            first: None,
            admitted,
            deadline: Instant::now() + Duration::from_secs(1),
            visibility: VisibilityMode::PublishedOnly,
            freshness_policy: FreshnessPolicy::Strict,
            degraded_sources: Arc::new(std::sync::Mutex::new(Vec::new())),
            stale_replanned: false,
            query_telemetry: telemetry_owner
                .start_query(VisibilityMode::PublishedOnly, QueryClass::Interactive),
            scan_stats: OracleQueryScanStats::default(),
            gate_lifecycle: None,
            running_query: None,
        });
        assert!(matches!(
            stream.frames.next().await,
            Some(Ok(QueryStreamFrame::Schema(_)))
        ));
        request_cancellation.cancel();
        assert!(matches!(
            tokio::time::timeout(Duration::from_secs(1), stream.frames.next())
                .await
                .expect("terminal timeout"),
            Some(Ok(QueryStreamFrame::Terminal(_)))
        ));
        assert_eq!(active_queries_for_test(&shared), 0);
    }

    /// Cancellation signals a stalled frame source before bounded drain returns.
    #[tokio::test]
    async fn cancel_signals_stalled_stream_under_bound() {
        let cancellation = CancellationToken::new();
        let observed = cancellation.clone();
        let owner = Arc::new(AdmissionReleaseProbe {
            slots_in_use: AtomicUsize::new(1),
            ..AdmissionReleaseProbe::default()
        });
        let owner_for_stream = Arc::clone(&owner);
        let frames = async_stream::stream! {
            observed.cancelled().await;
            owner_for_stream.release().await;
            yield Ok(crate::oracle::QueryStreamFrame::Schema(
                crate::oracle::QuerySchemaFrame {
                    schema_fingerprint: "cancel".to_owned(),
                    arrow_ipc_schema: Vec::new(),
                },
            ));
        };
        let stream = OracleQueryStream::test_new(
            "cancel".to_owned(),
            Box::pin(frames),
            cancellation.clone(),
        );
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), stream.cancel()).await;
        assert!(result.is_ok());
        assert!(cancellation.is_cancelled());
        assert_owner_released(&owner);
    }

    /// Every collector failure waits for owner release before returning.
    #[tokio::test]
    async fn collector_cancel_waits_for_owner_release() {
        let cases = [
            vec![
                Ok(QueryStreamFrame::Schema(QuerySchemaFrame {
                    schema_fingerprint: "probe".to_owned(),
                    arrow_ipc_schema: Vec::new(),
                })),
                Ok(QueryStreamFrame::Batch(crate::oracle::QueryBatchFrame {
                    arrow_ipc_batch: vec![0],
                })),
            ],
            vec![Ok(QueryStreamFrame::Batch(
                crate::oracle::QueryBatchFrame {
                    arrow_ipc_batch: vec![0],
                },
            ))],
            vec![Ok(QueryStreamFrame::Schema(QuerySchemaFrame {
                schema_fingerprint: "probe".to_owned(),
                arrow_ipc_schema: Vec::new(),
            }))],
        ];
        for frames in cases {
            let cancellation = CancellationToken::new();
            let owner = Arc::new(AdmissionReleaseProbe {
                slots_in_use: AtomicUsize::new(1),
                ..AdmissionReleaseProbe::default()
            });
            let stream = owner_stream(frames, Arc::clone(&owner), cancellation.clone());
            stream.cancel().await;
            assert!(cancellation.is_cancelled());
            assert_owner_released(&owner);
        }
    }

    /// Terminal completion records one Gate outcome even when the owner drops later.
    #[test]
    fn gate_lifecycle_records_terminal_once() {
        let outcomes = Arc::new(std::sync::Mutex::new(Vec::new()));
        let observed = Arc::clone(&outcomes);
        let lifecycle = QueryStreamLifecycle::new(move |outcome, _elapsed| {
            observed
                .lock()
                .expect("lifecycle outcomes lock")
                .push(outcome);
        });
        lifecycle.finish("success");
        lifecycle.finish("failed");
        drop(lifecycle);
        assert_eq!(
            *outcomes.lock().expect("lifecycle outcomes lock"),
            vec!["success"]
        );
    }

    /// Dropping a stream lifecycle before terminal emission records cancellation.
    #[test]
    fn gate_lifecycle_drop_records_cancelled_once() {
        let outcomes = Arc::new(std::sync::Mutex::new(Vec::new()));
        let observed = Arc::clone(&outcomes);
        let lifecycle = QueryStreamLifecycle::new(move |outcome, _elapsed| {
            observed
                .lock()
                .expect("lifecycle outcomes lock")
                .push(outcome);
        });
        drop(lifecycle);
        assert_eq!(
            *outcomes.lock().expect("lifecycle outcomes lock"),
            vec!["cancelled"]
        );
    }

    /// The production query span remains open until explicit success terminalization.
    #[test]
    fn gate_lifecycle_closes_exact_production_span_on_success() {
        let subscriber = SpanCaptureSubscriber::default();
        let records = Arc::clone(&subscriber.records);
        let closed = Arc::clone(&subscriber.closed);
        tracing::subscriber::with_default(subscriber, || {
            let lifecycle = QueryStreamLifecycle::new(|_, _| {});
            assert!(closed.lock().expect("closed spans").is_empty());
            lifecycle.finish("success");
            assert!(closed.lock().expect("closed spans").is_empty());
            drop(lifecycle);
        });
        assert!(has_span_outcome(
            &records,
            "bifrost.gate.query.stream",
            "success"
        ));
        assert_eq!(
            *closed.lock().expect("closed spans"),
            vec!["bifrost.gate.query.stream"]
        );
    }

    /// Dropping the lifecycle closes the exact production query span as cancelled.
    #[test]
    fn gate_lifecycle_closes_exact_production_span_on_drop() {
        let subscriber = SpanCaptureSubscriber::default();
        let records = Arc::clone(&subscriber.records);
        let closed = Arc::clone(&subscriber.closed);
        tracing::subscriber::with_default(subscriber, || {
            drop(QueryStreamLifecycle::new(|_, _| {}));
        });
        assert!(has_span_outcome(
            &records,
            "bifrost.gate.query.stream",
            "cancelled"
        ));
        assert_eq!(
            *closed.lock().expect("closed spans"),
            vec!["bifrost.gate.query.stream"]
        );
    }
}
