//! Terminal-aware query stream owner.

use std::sync::Arc;

use super::{
    OracleQueryStream, QueryStreamEvent, QueryStreamFrame, QueryStreamInput,
    QueryTerminalErrorCode, encode_batch_frame, encode_schema_frame,
    failed_terminal_for_visibility, query_class_label, successful_terminal, terminal_error_code,
    terminal_error_label,
};

impl OracleQueryStream {
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
            query_class,
            degraded,
            stale_replanned,
            mut query_telemetry,
        } = input;
        let schema_frame = encode_schema_frame(&schema)?;
        let schema_fingerprint = schema_frame.schema_fingerprint.clone();
        let lease_cancellation = admitted.cancellation.clone();
        let renewal_terminal = Arc::clone(&admitted.renewal_terminal);
        query_telemetry.start_stream();
        let _stream_span = tracing::info_span!(
            "bifrost.oracle.stream",
            visibility = super::visibility_label(visibility),
            query_class = query_class_label(query_class)
        );
        let frames = async_stream::stream! {
            let _admitted = admitted;
            let mut batches = batches;
            let mut next = first;
            let mut row_count = 0_u64;
            yield Ok(QueryStreamFrame::Schema(schema_frame));
            loop {
                let event = if let Some(value) = next.take() {
                    QueryStreamEvent::Batch(Some(value))
                } else {
                    super::next_query_stream_event(
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
                                yield Ok(QueryStreamFrame::Terminal(failed_terminal_for_visibility(
                                    QueryTerminalErrorCode::QueryExecutionFailed,
                                    row_count,
                                    visibility,
                                )));
                                return;
                            }
                        }
                    }
                    QueryStreamEvent::Batch(Some(Err(error))) => {
                        let code = terminal_error_code(&error);
                        tracing::error!(error = %error, error_code = terminal_error_label(code), "Oracle query stream execution failed");
                        query_telemetry.finish("failed", "complete");
                        yield Ok(QueryStreamFrame::Terminal(failed_terminal_for_visibility(code, row_count, visibility)));
                        return;
                    }
                    QueryStreamEvent::Batch(None) => break,
                    QueryStreamEvent::Failed(code) => {
                        query_telemetry.finish("failed", "complete");
                        yield Ok(QueryStreamFrame::Terminal(failed_terminal_for_visibility(code, row_count, visibility)));
                        return;
                    }
                }
            }
            let terminal = successful_terminal(visibility, degraded, stale_replanned, row_count);
            debug_assert!(terminal.validate(visibility).is_ok());
            query_telemetry.finish(if degraded { "degraded" } else { "success" }, if degraded { "degraded" } else { "complete" });
            yield Ok(QueryStreamFrame::Terminal(terminal));
        };
        Ok(Self {
            schema_fingerprint,
            frames: Box::pin(frames),
        })
    }
}
