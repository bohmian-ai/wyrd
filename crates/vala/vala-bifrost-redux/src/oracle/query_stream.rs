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

use super::telemetry::AnalyticalAttemptOutcome;

use super::*;

/// Owned query stream handle.  Frame production remains lazy and cancellation-aware.
pub struct OracleQueryStream {
    /// Stable fingerprint known before the first transport byte is emitted.
    pub schema_fingerprint: String,
    /// This query's absolute deadline as a nonnegative Unix epoch millisecond.
    ///
    /// Every transport projects this unchanged so an external settlement owner
    /// can bound its own drain and status polling by the same instant the
    /// server enforces, instead of inventing a client-side timeout.
    pub deadline_ms: i64,
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
    /// Execution path Oracle irreversibly selected before this stream opened.
    pub(super) execution_path: QueryExecutionPath,
    /// Public output schema drained from the query's own IPC encoder.
    pub(super) schema_frame: QuerySchemaFrame,
    /// The one IPC encoder every batch frame and the terminal EOS come from.
    pub(super) ipc: QueryIpcEncoder,
    /// Lazy physical batch stream.
    pub(super) batches: SendableRecordBatchStream,
    /// Pre-byte lookahead result.
    pub(super) first: Option<Result<RecordBatch, datafusion::error::DataFusionError>>,
    /// Stream-owned durable and local admission capacity.
    pub(super) admitted: AdmittedQueryGuard,
    /// Absolute execution deadline.
    pub(super) deadline: Instant,
    /// The same deadline as a nonnegative Unix epoch millisecond, for transports.
    pub(super) deadline_ms: i64,
    /// Requested visibility contract.
    pub(super) visibility: VisibilityMode,
    /// Public freshness policy applied only after aggregate disposition selection.
    pub(super) freshness_policy: wyrd_spec::vala::api::FreshnessPolicy,
    /// Exact source tiers completed as eligible degraded losses.
    pub(super) degraded_sources: super::DegradedSourceAccumulator,
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
    pub(super) fn finish(&mut self, outcome: QueryTerminalOutcome) {
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
    /// Execution path every terminal this stream emits must name.
    execution_path: QueryExecutionPath,
    /// Encoded public schema frame.
    schema_frame: QuerySchemaFrame,
    /// The one IPC encoder retained for this stream's batches and terminal.
    ipc: QueryIpcEncoder,
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
/// Resolves the next event one frame loop iteration acts on.
///
/// Cancellation is checked before anything is taken from the plan, so a
/// consumer that walked away is never charged for one more batch. The
/// pre-drained `first` batch is consumed next, because it was already pulled
/// from the plan to prove the stream opened, and only then is the plan polled.
///
/// # Errors
///
/// Errors are carried in the returned event rather than a `Result`, because
/// the loop settles a failure into a terminal instead of propagating it.
async fn next_frame_event(
    next: &mut Option<Result<RecordBatch, datafusion::error::DataFusionError>>,
    batches: &mut SendableRecordBatchStream,
    stream_cancellation: &CancellationToken,
    request_cancellation: &CancellationToken,
    deadline: std::time::Instant,
) -> QueryStreamEvent {
    if cancellation_requested(stream_cancellation, request_cancellation) {
        return QueryStreamEvent::Failed(QueryTerminalErrorCode::QueryExecutionFailed);
    }
    if let Some(value) = next.take() {
        return QueryStreamEvent::Batch(Some(value));
    }
    next_query_stream_event(batches, stream_cancellation, request_cancellation, deadline).await
}

/// Encodes one batch into a wire frame and charges it to the stream's counters.
///
/// The encoder buffers, so a batch may legitimately produce no frame yet; that
/// is reported as `Ok(None)` and the caller simply pulls the next batch. Rows
/// and payload bytes are counted only for a batch that actually became a frame,
/// which keeps the terminal's row count equal to what the consumer received.
/// The first frame also marks time-to-first-batch and, on the Analytical path,
/// charges egress.
///
/// # Errors
///
/// Returns `Err(())` when the IPC encoder rejects the batch. The encoder's own
/// error carries nothing the terminal can express, so the caller settles a
/// `QueryExecutionFailed` terminal instead.
fn encode_frame(
    batch: &RecordBatch,
    ipc: &mut QueryIpcEncoder,
    admitted: Option<&super::admission::AdmittedQueryGuard>,
    query_telemetry: &mut super::QueryTelemetryGuard,
    row_count: &mut u64,
) -> Result<Option<QueryBatchFrame>, ()> {
    let batch_rows = batch.num_rows() as u64;
    let Ok(frame) = ipc.write(batch) else {
        return Err(());
    };
    let Some(frame) = frame else {
        return Ok(None);
    };
    query_telemetry.first_batch();
    if let Some(admitted) = admitted {
        admitted.record_analytical_egress();
    }
    *row_count = row_count.saturating_add(batch_rows);
    query_telemetry.record_payload(batch_rows, frame.arrow_ipc_batch.len());
    Ok(Some(frame))
}

fn build_frames(input: FrameBuildInput) -> std::pin::Pin<Box<super::OracleFrameStream>> {
    let FrameBuildInput {
        execution_path,
        schema_frame,
        mut ipc,
        batches,
        first,
        admitted,
        deadline,
        visibility,
        freshness_policy,
        degraded_sources,
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
        // Re-bound after `admitted` on purpose. When a consumer walks away the
        // generator state is dropped in reverse declaration order, so the plan's
        // `RecordBatch` stream must be declared last to release its memory-pool
        // reservations before the analytical envelope that granted them. The
        // reverse order releases an Oracle query owner while a nested resource
        // child is still reserved, which poisons the process resource governor.
        let mut batches = batches;
        let mut next = first;
        let mut row_count = 0_u64;
        query_telemetry.record_payload(0, schema_frame.arrow_ipc_schema.len());
        yield Ok(QueryStreamFrame::Schema(schema_frame));
        let candidate = loop {
            let event = next_frame_event(
                &mut next,
                &mut batches,
                &stream_cancellation,
                &request_cancellation,
                deadline,
            ).await;
            match event {
                QueryStreamEvent::Batch(Some(Ok(batch))) => {
                    match encode_frame(
                        &batch,
                        &mut ipc,
                        admitted.as_ref(),
                        &mut query_telemetry,
                        &mut row_count,
                    ) {
                        Err(()) => break failed_terminal_for_visibility(
                            QueryTerminalErrorCode::QueryExecutionFailed,
                            row_count,
                            visibility,
                            execution_path,
                        ),
                        Ok(None) => {}
                        Ok(Some(frame)) => yield Ok(QueryStreamFrame::Batch(frame)),
                    }
                }
                QueryStreamEvent::Batch(Some(Err(error))) => {
                    let code = terminal_error_code(&error);
                    tracing::error!(error = %error, error_code = terminal_error_label(code), "Oracle query stream execution failed");
                    break failed_terminal_for_visibility(code, row_count, visibility, execution_path);
                }
                QueryStreamEvent::Batch(None) => break exhausted_terminal(
                    &degraded_sources,
                    visibility,
                    freshness_policy,
                                row_count,
                    execution_path,
                ),
                QueryStreamEvent::Failed(code) => {
                    break failed_terminal_for_visibility(code, row_count, visibility, execution_path);
                }
            }
        };
        drop(next);
        drop(batches);
        let terminal = settle_and_finish_stream(StreamSettlementInputs {
            candidate,
            stream_telemetry_cancelled: &stream_telemetry_cancelled,
            request_cancellation: &request_cancellation,
            stream_cancellation: &stream_cancellation,
            distributed_settlement: &distributed_settlement,
            admitted: &mut admitted,
            ipc: &mut ipc,
            query_telemetry: &mut query_telemetry,
            gate_lifecycle: gate_lifecycle.as_ref(),
            running_query: &mut running_query,
            visibility,
            execution_path,
            row_count,
        })
        .await;
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

/// Everything one drained query stream must settle before it reports a terminal.
///
/// Grouped because settlement and terminal assembly are one ordered act: the
/// cancellation signals, the distributed fan-out, the Analytical ownership, the
/// IPC writer, admission, telemetry, and the running-query registry are all
/// closed in a fixed order, and none of them is meaningful apart from it.
struct StreamSettlementInputs<'a> {
    /// Terminal frame candidate the batch loop broke with.
    candidate: QueryTerminalFrame,
    /// Whether stream telemetry already recorded an explicit cancellation.
    stream_telemetry_cancelled: &'a Arc<AtomicBool>,
    /// Request-scoped cancellation, signalled on a failed terminal.
    request_cancellation: &'a CancellationToken,
    /// Stream-scoped cancellation, signalled on a failed terminal.
    stream_cancellation: &'a CancellationToken,
    /// Distributed fan-out this query must join before it settles.
    distributed_settlement: &'a Arc<super::admission::DistributedQuerySettlement>,
    /// Admission guard whose Analytical ownership settles with the stream.
    admitted: &'a mut Option<AdmittedQueryGuard>,
    /// Arrow IPC writer closed after the last batch frame.
    ipc: &'a mut QueryIpcEncoder,
    /// Query telemetry guard that emits this stream's terminal outcome.
    query_telemetry: &'a mut super::QueryTelemetryGuard,
    /// Gate lifecycle notified of the terminal, when this query has one.
    gate_lifecycle: Option<&'a Arc<QueryStreamLifecycle>>,
    /// Running-query registry entry retired with the terminal outcome.
    running_query: &'a mut Option<RunningQueryTerminalOwner>,
    /// Visibility mode governing what a failed terminal may disclose.
    visibility: VisibilityMode,
    /// Execution path this stream's terminal must name, however it ends.
    execution_path: QueryExecutionPath,
    /// Rows emitted before the terminal, reported on every outcome.
    row_count: u64,
}

/// Settles every owner the drained stream holds and assembles its terminal.
///
/// Cancellation is projected first so a failed terminal signals both tokens
/// before anything waits on them; the distributed fan-out is joined next so no
/// peer work outlives the stream; the Analytical attempt settles after that,
/// because its ownership is what releases the query envelope the fan-out
/// charged against. Only then is the IPC stream closed, admission released,
/// telemetry finished, and the registry entry retired — so a terminal frame is
/// never emitted while any owner it accounts for is still live.
async fn settle_and_finish_stream(inputs: StreamSettlementInputs<'_>) -> QueryTerminalFrame {
    let StreamSettlementInputs {
        candidate,
        stream_telemetry_cancelled,
        request_cancellation,
        stream_cancellation,
        distributed_settlement,
        admitted,
        ipc,
        query_telemetry,
        gate_lifecycle,
        running_query,
        visibility,
        execution_path,
        row_count,
    } = inputs;
    let outcome = candidate.outcome;
    let failed_outcome = settle_failed_cancellation(
        outcome,
        stream_telemetry_cancelled,
        request_cancellation,
        stream_cancellation,
    );
    settle_distributed(distributed_settlement, outcome, stream_cancellation).await;
    // A cleanup that could not be confirmed cannot become a success terminal:
    // the graph is retained as draining, so rows this query produced are not
    // provably complete and its owners are not provably returned.
    let settlement = settle_analytical(admitted, outcome).await;
    let candidate = if settlement.clean {
        candidate
    } else {
        failed_terminal_for_visibility(
            QueryTerminalErrorCode::QueryExecutionFailed,
            row_count,
            visibility,
            execution_path,
        )
    };
    let candidate = close_ipc_stream(ipc, candidate, visibility, row_count, execution_path);
    let terminal = release_and_finish_terminal(
        admitted,
        query_telemetry,
        gate_lifecycle,
        candidate,
        failed_outcome,
        settlement.transferred,
        row_count,
    );
    if let Some(owner) = running_query {
        owner.finish(terminal.outcome);
    }
    terminal
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

/// Settles an inactive Analytical attempt before its admission is released.
///
/// Dropping the admission guard would release the attempt too, but only an
/// explicit settlement cancels the attempt's cancellation child and joins the
/// driver futures started beneath it. Doing that here — after the distributed
/// join and before admission release — is what makes a leader stream's end,
/// however it ended, the point at which follower work stops rather than the
/// point at which it is merely no longer awaited.
///
/// Signalling and awaiting is the whole of this stream's part in settlement:
/// the graph's lifecycle task owns the cleanup order, and reproducing any of it
/// here would be a second, racing sequence.
///
/// Reports what one stream's Analytical settlement did with its two owners.
///
/// Both answers come from the same act and are needed by different callers, so
/// returning them together is what keeps the terminal truthful *and* stops the
/// stream releasing admission the graph now owns.
pub(super) struct AnalyticalStreamSettlement {
    /// Whether cleanup completed, so a success terminal is truthful.
    pub(super) clean: bool,
    /// Whether the graph took this query's admission permit.
    pub(super) transferred: bool,
}

/// Settles this stream's Analytical attempt after moving admission to its graph.
///
/// The transfer happens *before* settlement is signalled and awaited, not
/// after: the lifecycle task may already have settled by the time this observes
/// the outcome, and a permit handed over afterwards would arrive at a graph that
/// no longer exists. Releasing it here instead would decrement class, tenant,
/// and active-query accounting — waking a queued waiter — while a failed
/// cleanup still holds this query's whole envelope.
///
/// Taking the Analytical ownership out of the guard first is what makes the
/// transfer sound: the ownership names the supervisor that would then hold the
/// guard, so a graph storing it whole would own a handle to itself.
///
/// A retained cleanup is reported to the caller rather than logged and ignored,
/// because it is what makes a success terminal untruthful.
pub(super) async fn settle_analytical(
    admitted: &mut Option<AdmittedQueryGuard>,
    outcome: QueryTerminalOutcome,
) -> AnalyticalStreamSettlement {
    let Some(ownership) = admitted
        .as_mut()
        .and_then(AdmittedQueryGuard::take_analytical)
    else {
        return AnalyticalStreamSettlement {
            clean: true,
            transferred: false,
        };
    };
    let transferred = match admitted.take() {
        Some(guard) => match ownership.retain_admission(guard) {
            Ok(()) => true,
            Err(returned) => {
                tracing::error!(
                    public_query_id = %ownership.key().public_query_id,
                    "Oracle analytical graph refused this stream's admission owner"
                );
                *admitted = Some(*returned);
                false
            }
        },
        None => false,
    };
    let attempt_outcome = match outcome {
        QueryTerminalOutcome::Failed => AnalyticalAttemptOutcome::Failed,
        _ => AnalyticalAttemptOutcome::Success,
    };
    let key = ownership.key();
    let clean = match ownership.settle(attempt_outcome).await {
        Ok(_) => true,
        Err(error) => {
            tracing::error!(
                %error,
                public_query_id = %key.public_query_id,
                datafusion_query_id = %key.datafusion_query_id,
                "Oracle analytical attempt could not settle with its leader stream"
            );
            false
        }
    };
    AnalyticalStreamSettlement { clean, transferred }
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
    row_count: u64,
    execution_path: QueryExecutionPath,
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
        row_count,
        execution_path,
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
    row_count: u64,
    execution_path: QueryExecutionPath,
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
            execution_path,
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
        execution_path,
        row_count,
        warnings,
        source_completion,
        error: None,
        // The candidate is built before the stream is closed; `close_ipc_stream`
        // attaches the writer's finish delta once, on this exact terminal.
        arrow_ipc_eos: Vec::new(),
    }
}

/// Fixed framing scratch a query-stream IPC owner may retain beyond the one
/// fragment it is currently holding.
///
/// Arrow's writer and push decoder each keep a small internal buffer for a
/// message that straddles a write boundary. Neither grows with the result, so
/// the bounded-memory proof for a query stream is "one schema fragment plus one
/// batch fragment plus this constant" rather than a fraction of the result.
pub const ORACLE_IPC_FRAMING_SCRATCH_BYTES: usize = 64 * 1024;

/// One query's Arrow IPC encoder, owning the single stream the frames carry.
///
/// The public query stream is *one* Arrow IPC stream split across Wyrd frames,
/// not one standalone stream per batch. This owner is what makes that true: it
/// holds a single [`arrow::ipc::writer::StreamWriter`] over a drainable byte
/// sink for the whole query, so the schema and its dictionaries are written
/// once and every batch frame carries only the bytes that write appended.
/// Draining after each operation is what keeps retained state bounded — the
/// sink never holds more than the fragment being handed out.
///
/// Because the writer is shared across batches, [`Self::finish`] must run
/// exactly once on a successful or degraded terminal, and must not run at all
/// on a failed or cancelled one; the terminal contract in
/// [`QueryTerminalFrame::validate`] enforces the same rule from the other side.
pub struct QueryIpcEncoder {
    /// Single per-query writer over a drainable in-memory sink.
    writer: arrow::ipc::writer::StreamWriter<Vec<u8>>,
    /// Whether the writer already emitted its end-of-stream delta.
    finished: bool,
    /// Largest single fragment this encoder has retained before handing it out.
    peak_retained_ipc_bytes: usize,
}

impl std::fmt::Debug for QueryIpcEncoder {
    /// Formats only the bounded-memory evidence, never buffered query bytes.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("QueryIpcEncoder")
            .field("finished", &self.finished)
            .field("peak_retained_ipc_bytes", &self.peak_retained_ipc_bytes)
            .finish_non_exhaustive()
    }
}

impl QueryIpcEncoder {
    /// Opens the query's IPC stream and drains its schema prefix into a frame.
    ///
    /// Creating the writer is what writes the stream prefix through exactly one
    /// schema message, so the schema frame is produced here rather than by a
    /// separate encoder: there is no point at which a caller could observe the
    /// stream without its schema.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryExecutionFailed`] when Arrow IPC rejects
    /// the output schema.
    pub fn new(schema: &SchemaRef) -> Result<(Self, QuerySchemaFrame), BifrostError> {
        let mut writer = arrow::ipc::writer::StreamWriter::try_new(Vec::new(), schema)
            .map_err(|_| BifrostError::QueryExecutionFailed)?;
        let arrow_ipc_schema = std::mem::take(writer.get_mut());
        let mut encoder = Self {
            writer,
            finished: false,
            peak_retained_ipc_bytes: 0,
        };
        encoder.observe_retained(arrow_ipc_schema.len());
        Ok((
            encoder,
            QuerySchemaFrame {
                schema_fingerprint: hex::encode(SchemaFingerprint::from_arrow_schema(schema).0),
                arrow_ipc_schema,
            },
        ))
    }

    /// Appends one record batch and drains exactly that write's delta.
    ///
    /// The delta is whatever the writer appended for this call: zero or more
    /// dictionary messages followed by exactly one record-batch message. Keeping
    /// dictionaries adjacent to the batch that first needs them is what lets a
    /// stateful decoder consume fragments in order without buffering the result.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryExecutionFailed`] when the stream is already
    /// finished or Arrow IPC rejects the batch. Returns `Ok(None)` for a
    /// zero-row execution artifact because a wire batch must decode to exactly
    /// one nonempty record batch.
    pub fn write(&mut self, batch: &RecordBatch) -> Result<Option<QueryBatchFrame>, BifrostError> {
        if self.finished {
            return Err(BifrostError::QueryExecutionFailed);
        }
        if batch.num_rows() == 0 {
            return Ok(None);
        }
        self.writer
            .write(batch)
            .map_err(|_| BifrostError::QueryExecutionFailed)?;
        let arrow_ipc_batch = std::mem::take(self.writer.get_mut());
        self.observe_retained(arrow_ipc_batch.len());
        Ok(Some(QueryBatchFrame { arrow_ipc_batch }))
    }

    /// Closes the query's IPC stream once and returns its end-of-stream delta.
    ///
    /// Only a successful or degraded terminal calls this. A failed or cancelled
    /// stream drops the encoder instead, which is why the terminal contract
    /// requires empty end-of-stream bytes for a failure: there is no valid EOS
    /// for a stream that never completed.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryExecutionFailed`] when called twice or when
    /// Arrow IPC cannot write the end-of-stream marker.
    pub fn finish(&mut self) -> Result<Vec<u8>, BifrostError> {
        if self.finished {
            return Err(BifrostError::QueryExecutionFailed);
        }
        self.writer
            .finish()
            .map_err(|_| BifrostError::QueryExecutionFailed)?;
        self.finished = true;
        let eos = std::mem::take(self.writer.get_mut());
        self.observe_retained(eos.len());
        if eos.is_empty() {
            return Err(BifrostError::QueryExecutionFailed);
        }
        Ok(eos)
    }

    /// Returns the largest single fragment this encoder ever retained.
    ///
    /// This is the encoder's half of the bounded-memory proof: it must never
    /// exceed the largest individual fragment, because the sink is drained after
    /// every operation rather than accumulating the result.
    #[must_use]
    pub const fn peak_retained_ipc_bytes(&self) -> usize {
        self.peak_retained_ipc_bytes
    }

    /// Records one drained fragment against the retained-bytes high-water mark.
    fn observe_retained(&mut self, bytes: usize) {
        self.peak_retained_ipc_bytes = self.peak_retained_ipc_bytes.max(bytes);
    }
}

/// Stateful decoder for one query's Arrow IPC stream, fragment by fragment.
///
/// Wyrd's server-tier consumers — bounded collection, cross-node forwarding,
/// and the `oracle_core` journeys — all read the same split stream, so they
/// share this owner instead of constructing a throwaway `StreamReader` per
/// frame. A per-frame reader cannot work once the stream is one IPC stream:
/// the schema and its dictionaries arrive in earlier fragments and a fresh
/// reader has no record of them.
///
/// The decoder tracks end-of-stream receipt itself.
/// [`arrow::ipc::reader::StreamDecoder::finish`] reports success both for a
/// stream that ended cleanly and for one that never began, so it cannot prove
/// an explicit EOS arrived; only the terminal's carried delta can.
///
/// The client tier has its own copy of this state machine in `vala-sdk`,
/// because client-tier crates may not depend on the Bifrost server engine. The
/// wire contract, not shared code, is what keeps the two honest.
#[derive(Debug, Default)]
pub struct QueryIpcDecoder {
    /// Arrow's push decoder, retaining schema and dictionary state.
    decoder: arrow::ipc::reader::StreamDecoder,
    /// Schema recovered from the required initial schema fragment.
    schema: Option<SchemaRef>,
    /// Whether the terminal's explicit end-of-stream delta was accepted.
    eos_accepted: bool,
    /// Largest single fragment this decoder has held while decoding.
    peak_pending_frame_bytes: usize,
}

/// Closed reason one query IPC fragment could not be consumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum QueryIpcDecodeError {
    /// The fragment was not valid Arrow IPC, or violated its expected shape.
    #[error("query IPC fragment is not exactly one well-formed Arrow message")]
    Malformed,
    /// A fragment arrived out of order relative to the stream's state.
    #[error("query IPC fragment arrived out of order for this stream")]
    OutOfOrder,
    /// A batch's schema did not match the stream's initial schema.
    #[error("query IPC batch schema does not match the stream schema")]
    SchemaMismatch,
}

impl QueryIpcDecoder {
    /// Creates a decoder positioned before the required schema fragment.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Consumes the initial schema fragment and returns the stream schema.
    ///
    /// The schema is read with a throwaway [`StreamReader`] rather than from
    /// the push decoder. Arrow's push decoder only completes a zero-body
    /// message once the *next* fragment arrives, so its own `schema()` is still
    /// empty at this point; reading the prefix here both hands the caller a
    /// schema immediately and rejects a fragment that smuggles a record batch
    /// in alongside it. The same bytes are still pushed through the decoder,
    /// which is what carries the schema and dictionary state into later
    /// fragments.
    ///
    /// [`StreamReader`]: arrow::ipc::reader::StreamReader
    ///
    /// # Errors
    ///
    /// Returns [`QueryIpcDecodeError::OutOfOrder`] for a duplicate schema or a
    /// schema after end-of-stream, and [`QueryIpcDecodeError::Malformed`] when
    /// the fragment is not exactly one schema message.
    pub fn accept_schema(&mut self, bytes: &[u8]) -> Result<SchemaRef, QueryIpcDecodeError> {
        if self.schema.is_some() || self.eos_accepted {
            return Err(QueryIpcDecodeError::OutOfOrder);
        }
        let mut prefix =
            arrow::ipc::reader::StreamReader::try_new(std::io::Cursor::new(bytes), None)
                .map_err(|_| QueryIpcDecodeError::Malformed)?;
        let schema = prefix.schema();
        if prefix
            .next()
            .transpose()
            .map_err(|_| QueryIpcDecodeError::Malformed)?
            .is_some()
        {
            return Err(QueryIpcDecodeError::Malformed);
        }
        if self.feed(bytes)?.is_some() {
            return Err(QueryIpcDecodeError::Malformed);
        }
        self.schema = Some(SchemaRef::clone(&schema));
        Ok(schema)
    }

    /// Consumes one batch fragment and returns its decoded record batch.
    ///
    /// # Errors
    ///
    /// Returns [`QueryIpcDecodeError::OutOfOrder`] before the schema or after
    /// end-of-stream, [`QueryIpcDecodeError::Malformed`] when the fragment does
    /// not decode to exactly one record batch, and
    /// [`QueryIpcDecodeError::SchemaMismatch`] when the batch contradicts the
    /// stream schema.
    pub fn accept_batch(&mut self, bytes: &[u8]) -> Result<RecordBatch, QueryIpcDecodeError> {
        let expected = self
            .schema
            .as_ref()
            .ok_or(QueryIpcDecodeError::OutOfOrder)?;
        if self.eos_accepted {
            return Err(QueryIpcDecodeError::OutOfOrder);
        }
        let expected = SchemaRef::clone(expected);
        let batch = self.feed(bytes)?.ok_or(QueryIpcDecodeError::Malformed)?;
        if batch.schema().as_ref() != expected.as_ref() {
            return Err(QueryIpcDecodeError::SchemaMismatch);
        }
        Ok(batch)
    }

    /// Consumes the terminal's end-of-stream delta and closes the stream.
    ///
    /// # Errors
    ///
    /// Returns [`QueryIpcDecodeError::OutOfOrder`] before the schema or on a
    /// second end-of-stream, and [`QueryIpcDecodeError::Malformed`] when the
    /// delta carries a record batch or leaves a partial message behind.
    pub fn accept_eos(&mut self, bytes: &[u8]) -> Result<(), QueryIpcDecodeError> {
        if self.schema.is_none() || self.eos_accepted {
            return Err(QueryIpcDecodeError::OutOfOrder);
        }
        if bytes.is_empty() || self.feed(bytes)?.is_some() {
            return Err(QueryIpcDecodeError::Malformed);
        }
        self.decoder
            .finish()
            .map_err(|_| QueryIpcDecodeError::Malformed)?;
        self.eos_accepted = true;
        Ok(())
    }

    /// Returns the stream schema once its initial fragment was accepted.
    #[must_use]
    pub fn schema(&self) -> Option<&SchemaRef> {
        self.schema.as_ref()
    }

    /// Reports whether the explicit end-of-stream delta was accepted.
    #[must_use]
    pub const fn eos_accepted(&self) -> bool {
        self.eos_accepted
    }

    /// Returns the largest single fragment this decoder held while decoding.
    #[must_use]
    pub const fn peak_pending_frame_bytes(&self) -> usize {
        self.peak_pending_frame_bytes
    }

    /// Pushes one fragment through Arrow's decoder, allowing at most one batch.
    ///
    /// # Errors
    ///
    /// Returns [`QueryIpcDecodeError::Malformed`] when Arrow rejects the bytes
    /// or the fragment yields more than one record batch.
    fn feed(&mut self, bytes: &[u8]) -> Result<Option<RecordBatch>, QueryIpcDecodeError> {
        self.peak_pending_frame_bytes = self.peak_pending_frame_bytes.max(bytes.len());
        let mut buffer = arrow::buffer::Buffer::from_vec(bytes.to_vec());
        let mut decoded = None;
        while !buffer.is_empty() {
            match self
                .decoder
                .decode(&mut buffer)
                .map_err(|_| QueryIpcDecodeError::Malformed)?
            {
                Some(batch) if decoded.is_none() => decoded = Some(batch),
                Some(_) => return Err(QueryIpcDecodeError::Malformed),
                None => {}
            }
        }
        Ok(decoded)
    }
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

/// Closes the query's IPC stream and attaches its end-of-stream to the terminal.
///
/// This is the only place `finish` runs, and it runs only for an outcome the
/// terminal contract requires an end-of-stream for. A failed or cancelled
/// candidate is returned untouched, so the encoder is dropped without a
/// `finish` and the terminal carries no end-of-stream — which is exactly the
/// pairing [`QueryTerminalFrame::validate`] enforces on the reading side.
///
/// Because the terminal is the last frame, a failure to close the stream cannot
/// be reported by emitting one more frame; it degrades the candidate into the
/// failed terminal for the requested visibility instead, which is also the only
/// representation that legitimately carries no end-of-stream.
fn close_ipc_stream(
    ipc: &mut QueryIpcEncoder,
    candidate: QueryTerminalFrame,
    visibility: VisibilityMode,
    row_count: u64,
    execution_path: QueryExecutionPath,
) -> QueryTerminalFrame {
    if candidate.outcome == QueryTerminalOutcome::Failed {
        return candidate;
    }
    match ipc.finish() {
        Ok(arrow_ipc_eos) => QueryTerminalFrame {
            arrow_ipc_eos,
            ..candidate
        },
        Err(error) => {
            tracing::error!(%error, "Oracle query stream could not close its Arrow IPC stream");
            failed_terminal_for_visibility(
                QueryTerminalErrorCode::QueryExecutionFailed,
                row_count,
                visibility,
                execution_path,
            )
        }
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
    admission_transferred: bool,
    _row_count: u64,
) -> QueryTerminalFrame {
    // A transferred permit is not this stream's to release: the graph holds it
    // until its cleanup is authoritative. Anything else is released here, and
    // an absent owner that was never transferred is still the diagnostic it was.
    if !admission_transferred {
        let _release = release_admitted(admitted);
    }
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
        deadline_ms: i64,
        frames: std::pin::Pin<Box<super::OracleFrameStream>>,
        cancellation: CancellationToken,
    ) -> Self {
        Self::assemble(
            schema_fingerprint,
            deadline_ms,
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
            deadline_ms: 0,
            frames,
            cancellation,
            telemetry_cancelled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            #[cfg(feature = "test-support")]
            resource_probe: None,
        }
    }

    /// Overrides a synthetic stream's projected deadline.
    ///
    /// Only [`Self::test_new`] streams need this: they have no participant cut
    /// to read a real deadline from, and a transport test that asserts the
    /// projected header still needs one exact value to assert against.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn with_deadline_ms(mut self, deadline_ms: i64) -> Self {
        self.deadline_ms = deadline_ms;
        self
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
        let (ipc, schema_frame) = QueryIpcEncoder::new(schema).expect("test schema frame");
        Self::new(QueryStreamInput {
            execution_path: QueryExecutionPath::Interactive,
            schema_frame,
            ipc,
            batches,
            first: None,
            admitted,
            deadline: Instant::now() + Duration::from_secs(1),
            deadline_ms: 0,
            visibility: VisibilityMode::PublishedOnly,
            freshness_policy: wyrd_spec::vala::api::FreshnessPolicy::Strict,
            degraded_sources: Arc::new(std::sync::Mutex::new(Vec::new())),
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
            execution_path,
            schema_frame,
            ipc,
            batches,
            first,
            admitted,
            deadline,
            deadline_ms,
            visibility,
            freshness_policy,
            degraded_sources,
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
            execution_path,
            schema_frame,
            ipc,
            batches,
            first,
            admitted,
            deadline,
            visibility,
            freshness_policy,
            degraded_sources,
                query_telemetry,
            gate_lifecycle,
            stream_cancellation,
            request_cancellation,
            stream_telemetry_cancelled,
            running_query,
        });
        let stream = Self::assemble(
            schema_fingerprint,
            deadline_ms,
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
        deadline_ms: i64,
        frames: std::pin::Pin<Box<super::OracleFrameStream>>,
        cancellation: CancellationToken,
        telemetry_cancelled: Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        Self {
            schema_fingerprint,
            deadline_ms,
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
    use wyrd_spec::vala::api::{FreshnessPolicy, QueryExecutionPath, QueryWarning};

    use super::{OracleQueryStream, QueryStreamInput, QueryStreamLifecycle, successful_terminal};
    use crate::oracle::admission::{active_queries_for_test, admitted_guard_for_test};
    use crate::oracle::exec::OracleQueryScanStats;
    use crate::oracle::failed_terminal_for_visibility;
    use crate::oracle::{
        BifrostError, OracleSlotManager, OracleTelemetry, QueryClass, QueryFreshness,
        QuerySchemaFrame, QuerySource, QueryStreamFrame, QueryTerminalErrorCode,
        QueryTerminalFrame, QueryTerminalOutcome, VisibilityMode,
    };
    use crate::test_support::{SpanCaptureSubscriber, has_span_outcome};

    /// An abandoned pre-transfer owner retires its entry failed from raw `Drop`.
    ///
    /// Selection moves this owner into the graph, so anything that still holds
    /// it never reached Analytical. Dropping it is therefore the last chance to
    /// remove the running entry, and it must do so without blocking or spawning
    /// — `Drop` runs on whatever thread abandoned the stream, including a
    /// runtime worker being torn down.
    #[test]
    fn running_query_terminal_owner_drop_retires_untransferred_failure() {
        let registry = Arc::new(crate::oracle::RunningQueryRegistry::new());
        let tenant_id = wyrd_spec::DataTenantId::new_v7();
        let request_id = wyrd_spec::request_id::RequestId::now_v7();
        assert!(registry.insert(crate::oracle::running::tests::entry(
            tenant_id,
            request_id.clone()
        )));

        let owner = super::RunningQueryTerminalOwner::new(
            Arc::clone(&registry),
            tenant_id,
            request_id.clone(),
        );
        assert!(
            registry.get(tenant_id, &request_id).is_some(),
            "the entry stays active while the owner lives"
        );
        drop(owner);

        assert!(
            registry.get(tenant_id, &request_id).is_none(),
            "dropping an untransferred owner retires its entry"
        );
        assert!(
            registry
                .settle_terminal(tenant_id, &request_id, QueryTerminalOutcome::Success)
                .is_none(),
            "the drop settlement is exactly once and cannot be reclassified"
        );
    }

    /// Closes a pre-terminal candidate the way the production stream does.
    ///
    /// [`successful_terminal`] builds the candidate before the stream's IPC
    /// writer is finished, so a success or degraded candidate legitimately
    /// carries no end-of-stream yet; `close_ipc_stream` attaches it. Validating
    /// the candidate directly would therefore assert on a state the wire never
    /// carries, so these tests close it first.
    fn closed(candidate: QueryTerminalFrame) -> QueryTerminalFrame {
        let schema: arrow::datatypes::SchemaRef = Arc::new(Schema::empty());
        let (mut ipc, _schema_frame) =
            super::QueryIpcEncoder::new(&schema).expect("empty schema opens an IPC stream");
        let row_count = candidate.row_count;
        super::close_ipc_stream(
            &mut ipc,
            candidate,
            VisibilityMode::Fused,
            row_count,
            QueryExecutionPath::Interactive,
        )
    }

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
            3,
            QueryExecutionPath::Interactive,
        );
        assert_eq!(complete.outcome, QueryTerminalOutcome::Success);
        closed(complete)
            .validate(VisibilityMode::Fused)
            .expect("complete terminal validates");

        // Row 1: Fused + LiveTail loss. Strict fails closed; AllowDegraded
        // is the explicit opt-in to a bounded incomplete answer.
        let strict_live = successful_terminal(
            VisibilityMode::Fused,
            FreshnessPolicy::Strict,
            &[QuerySource::LiveTail],
            2,
            QueryExecutionPath::Interactive,
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
            2,
            QueryExecutionPath::Interactive,
        );
        assert_eq!(allowed_live.outcome, QueryTerminalOutcome::Degraded);
        assert_eq!(allowed_live.freshness, QueryFreshness::Degraded);
        assert_eq!(
            allowed_live.warnings,
            vec![QueryWarning::LiveTailUnavailable]
        );
        closed(allowed_live)
            .validate(VisibilityMode::Fused)
            .expect("eligible live-tail degradation validates");

        // Row 2: Fused + sealed loss. The wire contract requires both sealed
        // entries to be Complete, so neither policy may report a short
        // answer over persisted data.
        for source in [QuerySource::Iceberg, QuerySource::HotSealed] {
            for policy in [FreshnessPolicy::Strict, FreshnessPolicy::AllowDegraded] {
                let failed = successful_terminal(
                    VisibilityMode::Fused,
                    policy,
                    &[source],
                    0,
                    QueryExecutionPath::Interactive,
                );
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
                4,
                QueryExecutionPath::Interactive,
            );
            assert_eq!(unaffected.outcome, QueryTerminalOutcome::Success);
            assert_eq!(unaffected.freshness, QueryFreshness::Complete);
            assert!(unaffected.warnings.is_empty());
            closed(unaffected)
                .validate(VisibilityMode::PublishedOnly)
                .expect("unrequested live-tail loss does not affect a published-only terminal");
        }
    }

    /// Opens a real per-query IPC encoder over the empty schema these tests use.
    ///
    /// The tests exercise terminal ownership rather than payload shape, but they
    /// must still travel through the production encoder: a hand-written schema
    /// frame would leave the stream with no writer to close, which is exactly
    /// the state the terminal end-of-stream contract now forbids.
    fn empty_schema_ipc(fingerprint: &str) -> (super::QueryIpcEncoder, QuerySchemaFrame) {
        let schema: arrow::datatypes::SchemaRef = Arc::new(Schema::empty());
        let (encoder, mut frame) =
            super::QueryIpcEncoder::new(&schema).expect("empty schema opens an IPC stream");
        frame.schema_fingerprint = fingerprint.to_owned();
        (encoder, frame)
    }

    /// Builds one dictionary-encoded batch so encoded fragments carry dictionaries.
    ///
    /// A dictionary-typed column is what separates a genuinely stateful stream
    /// from a sequence of standalone ones: Arrow writes the dictionary message
    /// only once, so a decoder that restarts per frame cannot read the later
    /// batches at all.
    fn dictionary_batch(values: &[&str]) -> arrow::record_batch::RecordBatch {
        let dictionary: arrow::array::DictionaryArray<arrow::datatypes::Int32Type> =
            values.iter().copied().collect();
        arrow::record_batch::RecordBatch::try_from_iter(vec![(
            "label",
            Arc::new(dictionary) as arrow::array::ArrayRef,
        )])
        .expect("dictionary batch builds")
    }

    /// Encodes `batch` as its own single-batch IPC stream and reports the total
    /// wire cost of that stream: schema message, batch message, end-of-stream.
    ///
    /// The stateful protocol contract compares a continuation fragment against
    /// this number to prove the schema is paid for exactly once per stream.
    ///
    /// # Panics
    ///
    /// Panics if the throwaway encoder rejects the schema, the batch, or the
    /// close, which would mean the encoder cannot round-trip its own input.
    fn standalone_stream_bytes(
        schema: &arrow::datatypes::SchemaRef,
        batch: &arrow::record_batch::RecordBatch,
    ) -> usize {
        let (mut throwaway, standalone_schema) =
            super::QueryIpcEncoder::new(schema).expect("standalone schema");
        let frame = throwaway
            .write(batch)
            .expect("standalone batch")
            .expect("a nonempty standalone batch produces a frame");
        let eos = throwaway.finish().expect("standalone close");
        standalone_schema.arrow_ipc_schema.len() + frame.arrow_ipc_batch.len() + eos.len()
    }

    /// Pins the terminal half of the protocol for the two row-count extremes a
    /// caller can observe: a stream that emitted no rows at all, and one whose
    /// terminal must agree with the rows already handed out.
    ///
    /// An empty logical result is schema-then-terminal, and its terminal still
    /// carries the one end-of-stream the stream ever produces.
    ///
    /// # Panics
    ///
    /// Panics if either stream fails to open, close, decode, or validate, or if
    /// a terminal disagrees with the row count it was closed with.
    fn terminal_contract_for_empty_and_mixed_streams(schema: &arrow::datatypes::SchemaRef) {
        let (mut empty_encoder, empty_schema) =
            super::QueryIpcEncoder::new(schema).expect("empty schema opens the stream");
        let empty_eos = empty_encoder.finish().expect("empty stream closes");
        let mut empty_decoder = super::QueryIpcDecoder::new();
        empty_decoder
            .accept_schema(&empty_schema.arrow_ipc_schema)
            .expect("empty schema decodes");
        empty_decoder
            .accept_eos(&empty_eos)
            .expect("empty stream is proven complete by its end-of-stream");
        assert!(empty_decoder.eos_accepted());
        let (mut empty_terminal_encoder, _) =
            super::QueryIpcEncoder::new(schema).expect("empty terminal stream opens");
        let empty_terminal = super::close_ipc_stream(
            &mut empty_terminal_encoder,
            successful_terminal(
                VisibilityMode::PublishedOnly,
                FreshnessPolicy::Strict,
                &[],
                0,
                QueryExecutionPath::Interactive,
            ),
            VisibilityMode::PublishedOnly,
            0,
            QueryExecutionPath::Interactive,
        );
        assert_eq!(empty_terminal.row_count, 0);
        empty_terminal
            .validate(VisibilityMode::PublishedOnly)
            .expect("empty logical result has one successful terminal");

        let (mut mixed_terminal_encoder, _) =
            super::QueryIpcEncoder::new(schema).expect("mixed terminal stream opens");
        let mixed_terminal = super::close_ipc_stream(
            &mut mixed_terminal_encoder,
            successful_terminal(
                VisibilityMode::PublishedOnly,
                FreshnessPolicy::Strict,
                &[],
                5,
                QueryExecutionPath::Interactive,
            ),
            VisibilityMode::PublishedOnly,
            5,
            QueryExecutionPath::Interactive,
        );
        assert_eq!(mixed_terminal.row_count, 5);
        mixed_terminal
            .validate(VisibilityMode::PublishedOnly)
            .expect("mixed stream terminal agrees with emitted rows");
    }

    /// Pins the decoder's ordering and framing refusals: a fragment or
    /// end-of-stream before the schema, a repeated schema, an absent
    /// end-of-stream, and bytes Arrow could never parse.
    ///
    /// Each is rejected by the decoder's own state machine before the payload
    /// reaches Arrow, so a malformed peer cannot drive Arrow decode work.
    ///
    /// # Panics
    ///
    /// Panics if the schema fragment fails to decode, or if any of the refused
    /// sequences is accepted.
    fn decoder_rejects_out_of_order_fragments(
        schema_fragment: &[u8],
        batch_fragment: &[u8],
        eos: &[u8],
    ) {
        let mut fresh = super::QueryIpcDecoder::new();
        assert!(fresh.accept_batch(batch_fragment).is_err());
        assert!(fresh.accept_eos(eos).is_err());
        fresh
            .accept_schema(schema_fragment)
            .expect("schema fragment decodes");
        assert!(fresh.accept_schema(schema_fragment).is_err());
        assert!(
            fresh.accept_eos(&[]).is_err(),
            "an absent EOS is not an EOS"
        );
        assert!(fresh.accept_batch(&[0xAA, 0xBB, 0xCC]).is_err());
    }

    /// Pins the failure half of the terminal contract: a stream dropped without
    /// `finish` — the cancelled or failed path — produces no end-of-stream, and
    /// its terminal validates precisely because it carries none.
    ///
    /// # Panics
    ///
    /// Panics if the stream fails to open, if the failed terminal carries an
    /// end-of-stream, or if it fails validation.
    fn failed_stream_terminal_carries_no_end_of_stream(schema: &arrow::datatypes::SchemaRef) {
        let (dropped, _dropped_schema) =
            super::QueryIpcEncoder::new(schema).expect("failed stream opens");
        drop(dropped);
        let failed = failed_terminal_for_visibility(
            QueryTerminalErrorCode::QueryExecutionFailed,
            0,
            VisibilityMode::PublishedOnly,
            QueryExecutionPath::Interactive,
        );
        assert!(failed.arrow_ipc_eos.is_empty());
        failed
            .validate(VisibilityMode::PublishedOnly)
            .expect("a failed terminal validates without an end-of-stream");
    }

    /// The split query stream is exactly one Arrow IPC stream, closed once.
    ///
    /// This pins the whole encoder/decoder state machine in the shape the wire
    /// carries it: one schema prefix, one continuation fragment per batch with
    /// its dictionaries adjacent, exactly one end-of-stream delta, and an empty
    /// result that is schema-then-terminal. It also pins the two negative
    /// halves — a stream that never closes has no end-of-stream to hand out,
    /// and bytes after the end-of-stream are rejected — plus the bounded-memory
    /// claim that neither side ever retains more than one fragment.
    #[test]
    fn stateful_ipc_protocol_contract() {
        let first = dictionary_batch(&["alpha", "beta", "alpha"]);
        let second = dictionary_batch(&["beta", "gamma"]);
        let schema = first.schema();

        let (mut encoder, schema_frame) =
            super::QueryIpcEncoder::new(&schema).expect("schema opens the stream");
        let empty = arrow::record_batch::RecordBatch::new_empty(schema.clone());
        assert!(
            encoder
                .write(&empty)
                .expect("empty artifact is accepted")
                .is_none(),
            "a zero-row execution artifact must not become a wire batch"
        );
        let first_frame = encoder
            .write(&first)
            .expect("first batch encodes")
            .expect("a nonempty batch produces a frame");
        let second_frame = encoder
            .write(&second)
            .expect("second batch encodes")
            .expect("a nonempty batch produces a frame");
        let eos = encoder.finish().expect("stream closes once");
        assert!(
            encoder.finish().is_err(),
            "a query stream may only be closed once"
        );

        // Schema-once: the second batch's fragment carries no schema message, so
        // it is strictly smaller than a standalone per-batch IPC stream of the
        // same rows would be.
        assert!(
            second_frame.arrow_ipc_batch.len() < standalone_stream_bytes(&schema, &second),
            "a continuation fragment must cost less than a standalone stream"
        );

        let mut decoder = super::QueryIpcDecoder::new();
        assert_eq!(
            decoder
                .accept_schema(&schema_frame.arrow_ipc_schema)
                .expect("schema fragment decodes"),
            schema
        );
        assert!(!decoder.eos_accepted());
        assert_eq!(
            decoder
                .accept_batch(&first_frame.arrow_ipc_batch)
                .expect("first fragment decodes"),
            first
        );
        assert_eq!(
            decoder
                .accept_batch(&second_frame.arrow_ipc_batch)
                .expect("second fragment reuses the retained dictionary"),
            second
        );
        decoder.accept_eos(&eos).expect("end-of-stream closes");
        assert!(decoder.eos_accepted());
        assert!(
            decoder.accept_batch(&second_frame.arrow_ipc_batch).is_err(),
            "no fragment may follow the end-of-stream"
        );
        assert!(
            decoder.accept_eos(&eos).is_err(),
            "the end-of-stream arrives exactly once"
        );

        // Bounded memory: neither owner ever retained more than one fragment
        // plus the documented fixed framing scratch.
        let largest = schema_frame
            .arrow_ipc_schema
            .len()
            .max(first_frame.arrow_ipc_batch.len())
            .max(second_frame.arrow_ipc_batch.len())
            .max(eos.len());
        let ceiling = largest + super::ORACLE_IPC_FRAMING_SCRATCH_BYTES;
        assert!(encoder.peak_retained_ipc_bytes() <= ceiling);
        assert!(decoder.peak_pending_frame_bytes() <= ceiling);

        terminal_contract_for_empty_and_mixed_streams(&schema);

        decoder_rejects_out_of_order_fragments(
            &schema_frame.arrow_ipc_schema,
            &first_frame.arrow_ipc_batch,
            &eos,
        );

        failed_stream_terminal_carries_no_end_of_stream(&schema);
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
        let (ipc, schema_frame) = empty_schema_ipc("production");
        let mut stream = OracleQueryStream::new(QueryStreamInput {
            execution_path: QueryExecutionPath::Interactive,
            schema_frame,
            ipc,
            batches: Box::pin(RecordBatchStreamAdapter::new(
                Arc::new(Schema::empty()),
                futures_util::stream::empty(),
            )),
            first: None,
            admitted,
            deadline: Instant::now() + Duration::from_secs(1),
            deadline_ms: 0,
            visibility: VisibilityMode::PublishedOnly,
            freshness_policy: FreshnessPolicy::Strict,
            degraded_sources: Arc::new(std::sync::Mutex::new(Vec::new())),
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
        let (ipc, schema_frame) = empty_schema_ipc("post-output-stale");
        let mut stream = OracleQueryStream::new(QueryStreamInput {
            execution_path: QueryExecutionPath::Interactive,
            schema_frame,
            ipc,
            batches: Box::pin(RecordBatchStreamAdapter::new(
                Arc::new(Schema::empty()),
                futures_util::stream::once(std::future::ready(Err(stale))),
            )),
            first: None,
            admitted,
            deadline: Instant::now() + Duration::from_secs(1),
            deadline_ms: 0,
            visibility: VisibilityMode::PublishedOnly,
            freshness_policy: FreshnessPolicy::Strict,
            degraded_sources: Arc::new(std::sync::Mutex::new(Vec::new())),
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
        let (ipc, schema_frame) = empty_schema_ipc("request-cancel");
        let mut stream = OracleQueryStream::new(QueryStreamInput {
            execution_path: QueryExecutionPath::Interactive,
            schema_frame,
            ipc,
            batches: Box::pin(RecordBatchStreamAdapter::new(
                Arc::new(Schema::empty()),
                futures_util::stream::pending(),
            )),
            first: None,
            admitted,
            deadline: Instant::now() + Duration::from_secs(1),
            deadline_ms: 0,
            visibility: VisibilityMode::PublishedOnly,
            freshness_policy: FreshnessPolicy::Strict,
            degraded_sources: Arc::new(std::sync::Mutex::new(Vec::new())),
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
