//! Terminal-aware query stream owner.
//!
//! The stream retains admission, renewal, cancellation, telemetry, and lazy
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

/// Complete owned inputs for one terminal-aware query stream.
pub(super) struct QueryStreamInput {
    /// Public output schema.
    pub(super) schema: SchemaRef,
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
    /// Whether live visibility degraded.
    pub(super) degraded: bool,
    /// Whether the one stale-cut replan was consumed.
    pub(super) stale_replanned: bool,
    /// Production query telemetry retained through terminal emission.
    pub(super) query_telemetry: QueryTelemetryGuard,
    /// Optional Gate lifecycle retained through frame consumption.
    pub(super) gate_lifecycle: Option<Arc<QueryStreamLifecycle>>,
}

/// One cancellation-, timeout-, or batch-aware stream step.
enum QueryStreamEvent {
    /// Next physical batch result, or `None` when execution completed.
    Batch(Option<Result<RecordBatch, datafusion::error::DataFusionError>>),
    /// Stable terminal failure selected before another batch is exposed.
    Failed(QueryTerminalErrorCode),
}

/// Waits for the next physical batch while enforcing lease cancellation and deadline.
///
/// Lease cancellation wins over starting another read when already observable.
/// While waiting, cancellation and the absolute deadline race the physical stream;
/// any earlier batches remain accounted by the owner, but no additional batch is
/// exposed after a cancellation or timeout event is selected.
async fn next_query_stream_event(
    batches: &mut SendableRecordBatchStream,
    cancellation: &CancellationToken,
    renewal_terminal: &Mutex<Option<QueryTerminalErrorCode>>,
    deadline: Instant,
) -> QueryStreamEvent {
    if cancellation.is_cancelled() {
        return QueryStreamEvent::Failed(renewal_terminal_code(renewal_terminal));
    }
    let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
        return QueryStreamEvent::Failed(QueryTerminalErrorCode::QueryTimeout);
    };
    tokio::select! {
        () = cancellation.cancelled() => {
            QueryStreamEvent::Failed(renewal_terminal_code(renewal_terminal))
        }
        () = tokio::time::sleep(remaining) => {
            QueryStreamEvent::Failed(QueryTerminalErrorCode::QueryTimeout)
        }
        value = batches.next() => QueryStreamEvent::Batch(value),
    }
}

/// Reads the renewal-selected terminal code without exposing lock poisoning.
fn renewal_terminal_code(
    renewal_terminal: &Mutex<Option<QueryTerminalErrorCode>>,
) -> QueryTerminalErrorCode {
    renewal_terminal
        .lock()
        .ok()
        .and_then(|reason| *reason)
        .unwrap_or(QueryTerminalErrorCode::QueryExecutionFailed)
}

/// Constructs the validated success/degraded terminal for one completed stream.
fn successful_terminal(
    visibility: VisibilityMode,
    degraded: bool,
    stale_replanned: bool,
    row_count: u64,
) -> QueryTerminalFrame {
    let freshness = if degraded {
        QueryFreshness::Degraded
    } else {
        QueryFreshness::Complete
    };
    let outcome = if degraded {
        QueryTerminalOutcome::Degraded
    } else {
        QueryTerminalOutcome::Success
    };
    let mut warnings = Vec::new();
    if degraded {
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
            outcome: if degraded {
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
fn encode_schema_frame(schema: &SchemaRef) -> Result<QuerySchemaFrame, BifrostError> {
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

/// Releases stream admission exactly once after a terminal frame is selected.
async fn release_admitted(admitted: &mut Option<AdmittedQueryGuard>) {
    if let Some(admitted) = admitted.take() {
        let _ = admitted.release().await;
    }
}

impl OracleQueryStream {
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

    /// Creates a lazy stream that owns admission guards through terminal output.
    ///
    /// The returned future retains all cleanup state until the terminal frame
    /// is emitted or the stream is dropped, so cancellation cannot detach a
    /// lease, permit, or renewal task from its query owner.
    ///
    /// # Errors
    /// Returns query execution failure when the output schema cannot be encoded.
    pub(super) fn new(input: QueryStreamInput) -> Result<Self, super::BifrostError> {
        let QueryStreamInput {
            schema,
            batches,
            first,
            admitted,
            deadline,
            visibility,
            degraded,
            stale_replanned,
            mut query_telemetry,
            gate_lifecycle,
        } = input;
        #[cfg(feature = "test-support")]
        let mut admitted = admitted;
        let schema_frame = encode_schema_frame(&schema)?;
        #[cfg(feature = "test-support")]
        let resource_probe = Some(admitted.attach_resource_probe());
        let schema_fingerprint = schema_frame.schema_fingerprint.clone();
        let lease_cancellation = admitted.cancellation.clone();
        let cancellation = lease_cancellation.clone();
        let telemetry_cancelled = query_telemetry.cancellation_marker();
        let stream_telemetry_cancelled = Arc::clone(&telemetry_cancelled);
        let renewal_terminal = Arc::clone(&admitted.renewal_terminal);
        query_telemetry.start_stream();
        let frames = async_stream::stream! {
            let mut admitted = Some(admitted);
            let mut batches = batches;
            let mut next = first;
            let mut row_count = 0_u64;
            query_telemetry.record_payload(0, schema_frame.arrow_ipc_schema.len());
            yield Ok(QueryStreamFrame::Schema(schema_frame));
            macro_rules! finish_failed_stream {
                ($code:expr) => {{
                    let outcome = failed_stream_outcome(&stream_telemetry_cancelled);
                    finish_stream(&mut query_telemetry, gate_lifecycle.as_ref(), outcome, "complete");
                    let terminal = failed_terminal_for_visibility($code, row_count, visibility);
                    release_admitted(&mut admitted).await;
                    yield Ok(QueryStreamFrame::Terminal(terminal));
                    return;
                }};
            }
            loop {
                let event = if lease_cancellation.is_cancelled() {
                    QueryStreamEvent::Failed(renewal_terminal_code(&renewal_terminal))
                } else if let Some(value) = next.take() {
                    QueryStreamEvent::Batch(Some(value))
                } else {
                    next_query_stream_event(
                        &mut batches,
                        &lease_cancellation,
                        &renewal_terminal,
                        deadline,
                    ).await
                };
                match event {
                    QueryStreamEvent::Batch(Some(Ok(batch))) => {
                        query_telemetry.first_batch();
                        row_count = row_count.saturating_add(batch.num_rows() as u64);
                        if let Ok(frame) = encode_batch_frame(&batch) {
                            query_telemetry.record_payload(
                                batch.num_rows() as u64,
                                frame.arrow_ipc_batch.len(),
                            );
                            yield Ok(QueryStreamFrame::Batch(frame));
                        } else {
                            finish_failed_stream!(QueryTerminalErrorCode::QueryExecutionFailed);
                        }
                    }
                    QueryStreamEvent::Batch(Some(Err(error))) => {
                        let code = terminal_error_code(&error);
                        tracing::error!(error = %error, error_code = terminal_error_label(code), "Oracle query stream execution failed");
                        finish_failed_stream!(code);
                    }
                    QueryStreamEvent::Batch(None) => break,
                    QueryStreamEvent::Failed(code) => {
                        finish_failed_stream!(code);
                    }
                }
            }
            let terminal = successful_terminal(visibility, degraded, stale_replanned, row_count);
            debug_assert!(terminal.validate(visibility).is_ok());
            finish_stream(
                &mut query_telemetry,
                gate_lifecycle.as_ref(),
                "success",
                if degraded { "degraded" } else { "complete" },
            );
            release_admitted(&mut admitted).await;
            yield Ok(QueryStreamFrame::Terminal(terminal));
        };
        let stream = Self::assemble(
            schema_fingerprint,
            Box::pin(frames),
            cancellation,
            telemetry_cancelled,
        );
        #[cfg(feature = "test-support")]
        let stream = stream.with_resource_probe(resource_probe);
        Ok(stream)
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
    /// cleanup in place while durable lease expiry remains authoritative.
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

    use tokio_util::sync::CancellationToken;

    use super::{OracleQueryStream, QueryStreamLifecycle};
    use crate::oracle::{BifrostError, QuerySchemaFrame, QueryStreamFrame};
    use crate::test_support::{SpanCaptureSubscriber, has_span_outcome};

    /// Synthetic owner state mirroring admission's local-slot, durable-release,
    /// and fenced-tail completion invariants.
    #[derive(Default)]
    struct AdmissionReleaseProbe {
        /// Number of local slots still held by the synthetic owner.
        slots_in_use: AtomicUsize,
        /// Whether the owner completed its durable admission release.
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

    /// Keeps every production terminal path awaited on the admitted owner.
    #[test]
    fn production_paths_await_admitted_release() {
        let source = include_str!("query_stream.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("query stream production section");
        assert_eq!(
            source
                .matches("release_admitted(&mut admitted).await")
                .count(),
            4,
            "every stream terminal path must await admitted.release()"
        );
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
