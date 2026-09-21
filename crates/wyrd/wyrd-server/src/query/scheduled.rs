//! Server-owned scheduled query caller.

use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::oracle::{AuthorizedQueryContext, QueryIpcDecoder};
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{
    BifrostQueryRequest, QueryStreamFrame, QueryTerminalFrame, QueryTerminalOutcome, VisibilityMode,
};
use wyrd_spec::vala::error::BifrostError;

use crate::state::AppState;
use futures_util::StreamExt as _;

/// One scheduled query's settled result.
#[derive(Debug)]
pub struct ScheduledQueryOutcome {
    /// Rows the caller decoded before the terminal frame.
    pub rows: u64,
    /// The server's own terminal, carrying the path it selected.
    pub terminal: QueryTerminalFrame,
}

/// Server-side caller that runs one already-authorized statement like a client.
///
/// A scheduled statement is not a second query surface: it holds a context the
/// server authorized once, dispatches through the same [`Bifrost::query_sql`](crate::state::Bifrost::query_sql)
/// entry every public caller uses, and settles the returned stream itself. It
/// deliberately owns no clock, queue, job, loop, path selector, or alternate
/// operation — whatever decides *when* a statement runs stays outside it.
pub struct ScheduledQueryCaller {
    /// Shared server state owning Gate, the Oracle, and admission.
    state: AppState,
    /// Context the server authorized for this caller, reused per statement.
    context: AuthorizedQueryContext,
    /// Token whose cancellation settles the stream before its own deadline.
    cancellation: CancellationToken,
}

impl ScheduledQueryCaller {
    /// Binds one authorized context and cancellation token to the server state.
    #[must_use]
    pub const fn new(
        state: AppState,
        context: AuthorizedQueryContext,
        cancellation: CancellationToken,
    ) -> Self {
        Self {
            state,
            context,
            cancellation,
        }
    }

    /// Runs and settles one statement through the ordinary query entry.
    ///
    /// The stream enforces its own pinned deadline, so this only adds the
    /// caller's cancellation: a cancelled run cancels the stream rather than
    /// dropping it. Local and remote cancellation are driven together while
    /// the original response is retained until its owner reports settlement.
    ///
    /// # Errors
    ///
    /// Returns the stable pre-stream query error, a frame or Arrow decode
    /// error, [`WyrdError`] for a failed terminal, unconfirmed lifecycle routing,
    /// and a protocol error when
    /// the stream ended without a terminal frame.
    ///
    /// # Cancellation
    ///
    /// Cancelling the bound token cancels the live stream and returns
    /// [`wyrd_spec::vala::error::BifrostError::QueryStreamIncomplete`], the
    /// same error a stream that ended without a terminal reports.
    pub async fn run(
        &self,
        request: BifrostQueryRequest,
    ) -> Result<ScheduledQueryOutcome, WyrdError> {
        let visibility = request.visibility;
        let mut stream = self
            .state
            .bifrost
            .query_sql(self.context.clone(), request)
            .await
            .map_err(WyrdError::from)?;
        // The stream's absolute deadline becomes one fixed instant before any
        // frame is taken, so no leg of consumption — least of all the wait for
        // clean EOF after a terminal — can start a fresh budget.
        let deadline = deadline_instant(stream.deadline_ms);
        let mut terminal = None;
        match Self::consume_to_terminal(
            &mut stream.frames,
            deadline,
            &self.cancellation,
            visibility,
            &mut terminal,
        )
        .await
        {
            Ok(outcome) => Ok(outcome),
            Err(error) => {
                self.state
                    .bifrost
                    .query_controls()
                    .ok_or(BifrostError::RunningQueryControlUnavailable)?
                    .cancel_and_settle(
                        self.context.data_tenant_id,
                        self.context.request_id.clone(),
                        stream,
                        visibility,
                        terminal,
                    )
                    .await?;
                Err(error)
            }
        }
    }

    /// Consumes one dispatched stream to a valid terminal followed by clean EOF.
    ///
    /// A terminal is a claim about a stream that has not ended, so a validated
    /// success is held provisionally and the same stream keeps being polled.
    /// Only clean EOF turns it into a [`ScheduledQueryOutcome`]; a second
    /// terminal, a late schema, a trailing batch, or a stream error after the
    /// terminal is a protocol failure. A failed terminal never becomes an
    /// outcome at all.
    ///
    /// # Errors
    ///
    /// Returns the mapped terminal error for a failed terminal,
    /// [`wyrd_spec::vala::error::BifrostError::QueryStreamProtocol`] for a row
    /// mismatch, a row-count overflow, or any frame after the terminal, the
    /// service Arrow projection for a malformed end-of-stream, and
    /// [`wyrd_spec::vala::error::BifrostError::QueryStreamIncomplete`] when the
    /// stream ends, the deadline passes, or the token is cancelled before a
    /// terminal arrives.
    ///
    /// # Cancellation
    ///
    /// Cancelling the bound token or reaching the fixed deadline abandons the
    /// stream; the caller settles it.
    async fn consume_to_terminal<S>(
        frames: &mut S,
        deadline: tokio::time::Instant,
        cancellation: &CancellationToken,
        visibility: VisibilityMode,
        observed: &mut Option<QueryTerminalFrame>,
    ) -> Result<ScheduledQueryOutcome, WyrdError>
    where
        S: futures_util::Stream<Item = Result<QueryStreamFrame, BifrostError>> + Unpin,
    {
        let mut decoder = QueryIpcDecoder::new();
        let mut rows = 0_u64;
        let mut settled: Option<ScheduledQueryOutcome> = None;
        loop {
            let frame = tokio::select! {
                // Cancellation and the pinned deadline are settlement claims, not
                // a race against buffered frames: an already-cancelled token or an
                // elapsed deadline must settle even when the stream has a frame
                // ready, so this branch order is biased rather than random.
                biased;
                () = cancellation.cancelled() => {
                    return Err(BifrostError::QueryStreamIncomplete.into());
                }
                () = tokio::time::sleep_until(deadline) => {
                    return Err(BifrostError::QueryStreamIncomplete.into());
                }
                frame = frames.next() => frame,
            };
            let Some(frame) = frame else {
                // Clean EOF. Only a validated terminal makes this a result.
                return settled.ok_or_else(|| BifrostError::QueryStreamIncomplete.into());
            };
            let frame = frame.map_err(|error| {
                *observed = None;
                WyrdError::from(error)
            })?;
            if settled.is_some() {
                *observed = None;
                return Err(BifrostError::QueryStreamProtocol.into());
            }
            match frame {
                QueryStreamFrame::Schema(schema) => {
                    decoder
                        .accept_schema(&schema.arrow_ipc_schema)
                        .map_err(|_| WyrdError::from(BifrostError::QueryExecutionFailed))?;
                }
                QueryStreamFrame::Batch(batch) => {
                    let decoded = decoder
                        .accept_batch(&batch.arrow_ipc_batch)
                        .map_err(|_| WyrdError::from(BifrostError::QueryExecutionFailed))?;
                    let decoded = u64::try_from(decoded.num_rows())
                        .map_err(|_| WyrdError::from(BifrostError::QueryStreamProtocol))?;
                    rows = rows
                        .checked_add(decoded)
                        .ok_or_else(|| WyrdError::from(BifrostError::QueryStreamProtocol))?;
                }
                QueryStreamFrame::Terminal(terminal) => {
                    terminal
                        .validate(visibility)
                        .and_then(|()| terminal.validate_emitted_rows(rows))
                        .map_err(|_| WyrdError::from(BifrostError::QueryStreamProtocol))?;
                    if terminal.outcome == QueryTerminalOutcome::Failed {
                        *observed = Some(terminal.clone());
                        return Err(terminal
                            .error
                            .as_ref()
                            .map_or(BifrostError::QueryExecutionFailed, |error| {
                                super::service::terminal_error_to_bifrost(error.code)
                            })
                            .into());
                    }
                    decoder
                        .accept_eos(&terminal.arrow_ipc_eos)
                        .map_err(|error| super::service::arrow_decode_error(&error))?;
                    *observed = Some(terminal.clone());
                    settled = Some(ScheduledQueryOutcome { rows, terminal });
                }
            }
        }
    }
}

/// Converts one absolute epoch-millisecond deadline into a fixed monotonic instant.
///
/// A deadline already in the past yields the current instant rather than a
/// negative offset, which makes the bounded wait resolve immediately instead of
/// overflowing.
fn deadline_instant(deadline_ms: i64) -> tokio::time::Instant {
    let now = std::time::SystemTime::UNIX_EPOCH
        .elapsed()
        .map_or(i64::MAX, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        });
    let remaining = u64::try_from(deadline_ms.saturating_sub(now)).unwrap_or(0);
    tokio::time::Instant::now() + std::time::Duration::from_millis(remaining)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use arrow::array::Int64Array;
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::record_batch::RecordBatch;
    use wyrd_spec::vala::api::{
        QueryBatchFrame, QueryFreshness, QuerySchemaFrame, QuerySource, QueryTerminalError,
        QueryTerminalErrorCode, SourceCompletion, SourceCompletionOutcome,
    };

    use super::*;

    /// Encodes one split query IPC stream exactly as the Oracle encoder does.
    ///
    /// Returns the schema fragment, one bare fragment per written batch, and the
    /// end-of-stream fragment a successful terminal must carry.
    ///
    /// # Panics
    ///
    /// Panics when the fixture batches cannot be encoded.
    fn split_ipc_stream(batches: &[&[i64]]) -> (Vec<u8>, Vec<Vec<u8>>, Vec<u8>) {
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));
        let mut writer = arrow::ipc::writer::StreamWriter::try_new(Vec::new(), schema.as_ref())
            .expect("schema writer starts");
        let prefix = std::mem::take(writer.get_mut());
        let fragments = batches
            .iter()
            .map(|values| {
                let batch = RecordBatch::try_new(
                    Arc::clone(&schema),
                    vec![Arc::new(Int64Array::from(values.to_vec()))],
                )
                .expect("fixture batch is valid");
                writer.write(&batch).expect("fixture batch writes");
                std::mem::take(writer.get_mut())
            })
            .collect();
        writer.finish().expect("fixture writer finishes");
        (prefix, fragments, std::mem::take(writer.get_mut()))
    }

    /// Builds a valid published-only terminal claiming `row_count` rows.
    /// Both persisted source classes are complete, matching the Oracle contract.
    fn success_terminal(row_count: u64, arrow_ipc_eos: Vec<u8>) -> QueryTerminalFrame {
        QueryTerminalFrame {
            outcome: QueryTerminalOutcome::Success,
            freshness: QueryFreshness::Complete,
            execution_path: wyrd_spec::vala::api::QueryExecutionPath::Interactive,
            row_count,
            warnings: Vec::new(),
            source_completion: vec![
                SourceCompletion {
                    source: QuerySource::Iceberg,
                    outcome: SourceCompletionOutcome::Complete,
                },
                SourceCompletion {
                    source: QuerySource::HotSealed,
                    outcome: SourceCompletionOutcome::Complete,
                },
            ],
            error: None,
            arrow_ipc_eos,
        }
    }

    /// A scheduled outcome requires one valid terminal followed by clean EOF.
    ///
    /// The cases run against the same production consumption helper the public
    /// entry uses, because the property under test is exactly that a scheduled
    /// caller has no weaker acceptance path than any other consumer: a failed
    /// terminal, a row-count mismatch, a malformed end-of-stream, a second
    /// terminal, and any frame after the terminal each return a stable query
    /// error and no outcome.
    ///
    /// # Panics
    /// Panics if malformed completion is accepted, valid completion is lost,
    /// or the canonical refusal differs from the forced failure.
    #[tokio::test]
    async fn scheduled_terminal_requires_clean_eof() {
        let (prefix, fragments, eos) = split_ipc_stream(&[&[1, 2, 3]]);
        let schema_frame = QueryStreamFrame::Schema(QuerySchemaFrame {
            schema_fingerprint: "test".to_owned(),
            arrow_ipc_schema: prefix.clone(),
        });
        let batch_frame = QueryStreamFrame::Batch(QueryBatchFrame {
            arrow_ipc_batch: fragments[0].clone(),
        });
        let head = vec![schema_frame.clone(), batch_frame.clone()];
        let with_head = |mut tail: Vec<QueryStreamFrame>| {
            let mut frames = head.clone();
            frames.append(&mut tail);
            frames
        };

        let failed = QueryTerminalFrame {
            outcome: QueryTerminalOutcome::Failed,
            error: Some(QueryTerminalError {
                code: QueryTerminalErrorCode::QueryTimeout,
                detail: None,
            }),
            ..success_terminal(3, Vec::new())
        };

        let cases: Vec<(&str, Vec<QueryStreamFrame>, BifrostError)> = vec![
            (
                "a failed terminal is never an outcome",
                with_head(vec![QueryStreamFrame::Terminal(failed)]),
                BifrostError::QueryTimeout,
            ),
            (
                "a terminal claiming rows the stream did not emit is refused",
                with_head(vec![QueryStreamFrame::Terminal(success_terminal(
                    4,
                    eos.clone(),
                ))]),
                BifrostError::QueryStreamProtocol,
            ),
            (
                "a second terminal after a valid one is refused",
                with_head(vec![
                    QueryStreamFrame::Terminal(success_terminal(3, eos.clone())),
                    QueryStreamFrame::Terminal(success_terminal(3, eos.clone())),
                ]),
                BifrostError::QueryStreamProtocol,
            ),
            (
                "a schema after a valid terminal is refused",
                with_head(vec![
                    QueryStreamFrame::Terminal(success_terminal(3, eos.clone())),
                    schema_frame,
                ]),
                BifrostError::QueryStreamProtocol,
            ),
            (
                "a batch after a valid terminal is refused",
                with_head(vec![
                    QueryStreamFrame::Terminal(success_terminal(3, eos.clone())),
                    batch_frame,
                ]),
                BifrostError::QueryStreamProtocol,
            ),
            (
                "a stream that ends before its terminal is incomplete",
                head.clone(),
                BifrostError::QueryStreamIncomplete,
            ),
        ];

        for (label, frames, expected) in cases {
            let mut stream = futures_util::stream::iter(
                frames
                    .into_iter()
                    .map(Ok::<_, BifrostError>)
                    .collect::<Vec<_>>(),
            );
            let error = ScheduledQueryCaller::consume_to_terminal(
                &mut stream,
                deadline_instant(deadline_in(30_000)),
                &CancellationToken::new(),
                VisibilityMode::PublishedOnly,
                &mut None,
            )
            .await
            .expect_err(label);
            assert_eq!(
                error.code(),
                WyrdError::from(expected).code(),
                "{label}: {error:?}"
            );
        }

        // A malformed end-of-stream is refused through the service Arrow
        // projection rather than becoming a result.
        let mut stream = futures_util::stream::iter(
            with_head(vec![QueryStreamFrame::Terminal(success_terminal(
                3,
                vec![0xff, 0xff, 0xff, 0xff],
            ))])
            .into_iter()
            .map(Ok::<_, BifrostError>)
            .collect::<Vec<_>>(),
        );
        let error = ScheduledQueryCaller::consume_to_terminal(
            &mut stream,
            deadline_instant(deadline_in(30_000)),
            &CancellationToken::new(),
            VisibilityMode::PublishedOnly,
            &mut None,
        )
        .await
        .expect_err("a malformed end-of-stream is refused");
        assert!(
            matches!(error, WyrdError::Internal { .. }),
            "a malformed end-of-stream keeps the service Arrow projection: {error:?}"
        );

        // A valid terminal followed by clean EOF is the only success.
        let mut stream = futures_util::stream::iter(
            with_head(vec![QueryStreamFrame::Terminal(success_terminal(3, eos))])
                .into_iter()
                .map(Ok::<_, BifrostError>)
                .collect::<Vec<_>>(),
        );
        let outcome = ScheduledQueryCaller::consume_to_terminal(
            &mut stream,
            deadline_instant(deadline_in(30_000)),
            &CancellationToken::new(),
            VisibilityMode::PublishedOnly,
            &mut None,
        )
        .await
        .expect("a valid terminal followed by clean EOF settles");
        assert_eq!(outcome.rows, 3);
        assert_eq!(outcome.terminal.outcome, QueryTerminalOutcome::Success);

        // An elapsed deadline never waits and never invents an outcome.
        let mut stream = futures_util::stream::iter(Vec::<Result<_, BifrostError>>::new());
        let error = ScheduledQueryCaller::consume_to_terminal(
            &mut stream,
            deadline_instant(deadline_in(-1)),
            &CancellationToken::new(),
            VisibilityMode::PublishedOnly,
            &mut None,
        )
        .await
        .expect_err("an elapsed deadline is incomplete");
        assert_eq!(
            error.code(),
            WyrdError::from(BifrostError::QueryStreamIncomplete).code()
        );
    }

    /// Returns an absolute epoch-millisecond deadline `millis` from now.
    ///
    /// # Panics
    ///
    /// Panics when the test clock predates the Unix epoch.
    fn deadline_in(millis: i64) -> i64 {
        let now = i64::try_from(
            std::time::SystemTime::UNIX_EPOCH
                .elapsed()
                .expect("the test clock is after the epoch")
                .as_millis(),
        )
        .expect("the epoch millisecond fits an i64");
        now + millis
    }
}
