//! Server-owned scheduled query caller.

use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::oracle::{AuthorizedQueryContext, QueryIpcDecoder};
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{BifrostQueryRequest, QueryStreamFrame, QueryTerminalFrame};

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
/// server authorized once, dispatches through the same [`AppState::query_sql`]
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
    /// dropping it, which is what returns the admitted owner promptly.
    ///
    /// # Errors
    ///
    /// Returns the stable pre-stream query error, a frame or Arrow decode
    /// error, [`WyrdError`] for a failed terminal, and a protocol error when
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
        let mut stream = self
            .state
            .bifrost
            .query_sql(self.context.clone(), request)
            .await
            .map_err(WyrdError::from)?;
        let mut decoder = QueryIpcDecoder::new();
        let mut rows = 0_u64;
        loop {
            let frame = tokio::select! {
                () = self.cancellation.cancelled() => {
                    stream.cancel().await;
                    return Err(WyrdError::from(
                        wyrd_spec::vala::error::BifrostError::QueryStreamIncomplete,
                    ));
                }
                frame = stream.frames.next() => frame,
            };
            let Some(frame) = frame else {
                return Err(WyrdError::from(
                    wyrd_spec::vala::error::BifrostError::QueryStreamIncomplete,
                ));
            };
            match frame.map_err(WyrdError::from)? {
                QueryStreamFrame::Schema(schema) => {
                    decoder
                        .accept_schema(&schema.arrow_ipc_schema)
                        .map_err(|_| {
                            WyrdError::from(
                                wyrd_spec::vala::error::BifrostError::QueryExecutionFailed,
                            )
                        })?;
                }
                QueryStreamFrame::Batch(batch) => {
                    let decoded = decoder.accept_batch(&batch.arrow_ipc_batch).map_err(|_| {
                        WyrdError::from(wyrd_spec::vala::error::BifrostError::QueryExecutionFailed)
                    })?;
                    rows = rows.saturating_add(decoded.num_rows() as u64);
                }
                QueryStreamFrame::Terminal(terminal) => {
                    return Ok(ScheduledQueryOutcome { rows, terminal });
                }
            }
        }
    }
}
