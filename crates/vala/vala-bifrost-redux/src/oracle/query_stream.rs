//! Terminal-aware query stream owner.
//!
//! The stream retains admission, renewal, cancellation, telemetry, and lazy
//! physical batches until exactly one terminal outcome releases owned resources.

use std::sync::Arc;

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
    /// Query-keyed lifecycle observation used only by test-tier transports.
    #[cfg(feature = "test-support")]
    resource_probe: Option<Arc<super::admission::QueryResourceProbe>>,
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
    /// Immutable admission class.
    pub(super) query_class: QueryClass,
    /// Whether live visibility degraded.
    pub(super) degraded: bool,
    /// Whether the one stale-cut replan was consumed.
    pub(super) stale_replanned: bool,
    /// Production query telemetry retained through terminal emission.
    pub(super) query_telemetry: QueryTelemetryGuard,
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

/// Creates the low-cardinality span retained across one lazy query stream.
fn query_stream_span(visibility: VisibilityMode, query_class: QueryClass) -> tracing::Span {
    tracing::info_span!(
        "bifrost.oracle.stream",
        visibility = super::visibility_label(visibility),
        query_class = query_class_label(query_class)
    )
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
            mut admitted,
            deadline,
            visibility,
            query_class,
            degraded,
            stale_replanned,
            mut query_telemetry,
        } = input;
        let schema_frame = encode_schema_frame(&schema)?;
        #[cfg(feature = "test-support")]
        let resource_probe = Some(admitted.attach_resource_probe());
        let schema_fingerprint = schema_frame.schema_fingerprint.clone();
        let lease_cancellation = admitted.cancellation.clone();
        let cancellation = lease_cancellation.clone();
        let renewal_terminal = Arc::clone(&admitted.renewal_terminal);
        query_telemetry.start_stream();
        let _stream_span = query_stream_span(visibility, query_class);
        let frames = async_stream::stream! {
            let mut admitted = Some(admitted);
            let mut batches = batches;
            let mut next = first;
            let mut row_count = 0_u64;
            yield Ok(QueryStreamFrame::Schema(schema_frame));
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
                        match encode_batch_frame(&batch) {
                            Ok(frame) => yield Ok(QueryStreamFrame::Batch(frame)),
                            Err(_) => {
                                query_telemetry.finish("failed", "complete");
                                let terminal = failed_terminal_for_visibility(
                                    QueryTerminalErrorCode::QueryExecutionFailed,
                                    row_count,
                                    visibility,
                                );
                                if let Some(admitted) = admitted.take() {
                                    let _ = admitted.release().await;
                                }
                                yield Ok(QueryStreamFrame::Terminal(terminal));
                                return;
                            }
                        }
                    }
                    QueryStreamEvent::Batch(Some(Err(error))) => {
                        let code = terminal_error_code(&error);
                        tracing::error!(error = %error, error_code = terminal_error_label(code), "Oracle query stream execution failed");
                        query_telemetry.finish("failed", "complete");
                        let terminal = failed_terminal_for_visibility(code, row_count, visibility);
                        if let Some(admitted) = admitted.take() {
                            let _ = admitted.release().await;
                        }
                        yield Ok(QueryStreamFrame::Terminal(terminal));
                        return;
                    }
                    QueryStreamEvent::Batch(None) => break,
                    QueryStreamEvent::Failed(code) => {
                        query_telemetry.finish("failed", "complete");
                        let terminal = failed_terminal_for_visibility(code, row_count, visibility);
                        if let Some(admitted) = admitted.take() {
                            let _ = admitted.release().await;
                        }
                        yield Ok(QueryStreamFrame::Terminal(terminal));
                        return;
                    }
                }
            }
            let terminal = successful_terminal(visibility, degraded, stale_replanned, row_count);
            debug_assert!(terminal.validate(visibility).is_ok());
            query_telemetry.finish(if degraded { "degraded" } else { "success" }, if degraded { "degraded" } else { "complete" });
            if let Some(admitted) = admitted.take() {
                let _ = admitted.release().await;
            }
            yield Ok(QueryStreamFrame::Terminal(terminal));
        };
        Ok(Self {
            schema_fingerprint,
            frames: Box::pin(frames),
            cancellation,
            #[cfg(feature = "test-support")]
            resource_probe,
        })
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

    use super::OracleQueryStream;
    use crate::oracle::{BifrostError, QuerySchemaFrame, QueryStreamFrame};

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
            source.matches("admitted.release().await").count(),
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
}
