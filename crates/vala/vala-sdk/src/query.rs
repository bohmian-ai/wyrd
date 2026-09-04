//! Terminal-safe Oracle query client and bounded result collection.

use std::collections::VecDeque;
use std::io::Cursor;
use std::pin::Pin;

use arrow::datatypes::SchemaRef;
use arrow::ipc::reader::StreamReader;
use arrow::record_batch::RecordBatch;
use futures_util::{Stream, StreamExt};
use wyrd_client::WyrdClient;
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    BifrostQueryRequest, CancelRunningQueryResponse, ListRunningQueriesResponse, QueryStreamFrame,
    QueryTerminalErrorCode, QueryTerminalFrame, QueryTerminalOutcome, RunningQuerySummary,
};
use wyrd_spec::vala::error::BifrostError;
use wyrd_tonic::frame_codec::FrameDecoder;
use wyrd_tonic::query_conversion::QueryStreamConverter;

/// Maximum encoded protobuf frame accepted by the public query client.
const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// Boxed response-body stream retained so dropping a query cancels body consumption.
type ResponseBytes = Pin<Box<dyn Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send>>;

/// Errors returned while querying Oracle through the public client contract.
#[derive(Debug, thiserror::Error)]
pub enum ValaSdkError {
    /// The authenticated HTTP transport rejected or could not send a request.
    #[error("query transport failed: {0}")]
    Transport(#[from] wyrd_spec::error::WyrdError),
    /// The length-delimited protobuf stream violated framing or conversion rules.
    #[error("query stream protocol failed: {0}")]
    Protocol(String),
    /// Arrow IPC schema or batch bytes were invalid or inconsistent.
    #[error("query Arrow IPC decode failed: {0}")]
    Arrow(String),
    /// EOF arrived before the required terminal frame.
    #[error("query stream ended before its required terminal frame")]
    IncompleteQueryStream,
    /// Oracle reported a validated late failure after zero or more batches.
    #[error("query terminal reported failure")]
    FailedTerminal {
        /// Validated terminal metadata retained for diagnostics.
        terminal: QueryTerminalFrame,
    },
    /// The caller's bounded collection limits were exceeded.
    #[error("query result exceeds configured bounds")]
    ResultTooLarge,
}

impl ValaSdkError {
    /// Returns the scrubbed human-readable detail for boundary projections.
    #[must_use]
    pub fn detail(&self) -> String {
        match self {
            Self::Transport(error) => error
                .as_problem_json()
                .get("detail")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("query transport failed")
                .to_owned(),
            Self::Protocol(detail) | Self::Arrow(detail) => detail.clone(),
            Self::IncompleteQueryStream => {
                "query stream ended before its required terminal frame".to_owned()
            }
            Self::FailedTerminal { terminal } => terminal
                .error
                .as_ref()
                .and_then(|error| error.detail.as_ref())
                .map_or_else(
                    || "query terminal reported failure".to_owned(),
                    |detail| detail.as_str().to_owned(),
                ),
            Self::ResultTooLarge => "query result exceeds configured bounds".to_owned(),
        }
    }

    /// Returns structured diagnostics already scrubbed for public projection.
    #[must_use]
    pub fn safe_details(&self) -> Option<serde_json::Value> {
        match self {
            Self::Transport(error) => error.as_problem_json().get("details").cloned(),
            Self::FailedTerminal { terminal } => serde_json::to_value(terminal).ok(),
            Self::Protocol(_)
            | Self::Arrow(_)
            | Self::IncompleteQueryStream
            | Self::ResultTooLarge => None,
        }
    }

    /// Returns the stable public code used by language projections.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Transport(error) => error.code(),
            Self::Protocol(_) | Self::Arrow(_) => "WYRD_VALA_502_QUERY_STREAM_PROTOCOL",
            Self::IncompleteQueryStream => "WYRD_VALA_502_QUERY_STREAM_INCOMPLETE",
            Self::FailedTerminal { terminal } => terminal_bifrost_error(terminal).code(),
            Self::ResultTooLarge => "WYRD_VALA_413_QUERY_RESULT_TOO_LARGE",
        }
    }

    /// Returns the HTTP-equivalent status projected into language SDK errors.
    #[must_use]
    pub fn status(&self) -> u16 {
        match self {
            Self::Transport(error) => error.status(),
            Self::Protocol(_) | Self::Arrow(_) | Self::IncompleteQueryStream => 502,
            Self::FailedTerminal { terminal } => terminal_bifrost_error(terminal).status(),
            Self::ResultTooLarge => 413,
        }
    }

    /// Returns a stable human-readable SDK error title.
    #[must_use]
    pub fn title(&self) -> &'static str {
        match self {
            Self::Transport(error) => error.title(),
            Self::Protocol(_) | Self::Arrow(_) => "Query stream protocol failed",
            Self::IncompleteQueryStream => "Query stream incomplete",
            Self::FailedTerminal { terminal } => terminal_bifrost_error(terminal).title(),
            Self::ResultTooLarge => "Query result too large",
        }
    }

    /// Returns operator-facing remediation projected into language SDK errors.
    #[must_use]
    pub fn remediation(&self) -> &'static str {
        match self {
            Self::Transport(error) => error.remediation(),
            Self::Protocol(_) | Self::Arrow(_) => {
                "Retry the query; if the error persists, verify client and server contract versions."
            }
            Self::IncompleteQueryStream => {
                "Retry the query because the response ended before its required terminal frame."
            }
            Self::FailedTerminal { terminal } => terminal_bifrost_error(terminal).remediation(),
            Self::ResultTooLarge => {
                "Reduce the query result or raise the caller's explicit collection limit within its hard ceiling."
            }
        }
    }

    /// Returns terminal metadata when a validated failed terminal caused this error.
    #[must_use]
    pub fn terminal(&self) -> Option<&QueryTerminalFrame> {
        match self {
            Self::FailedTerminal { terminal } => Some(terminal),
            Self::Transport(_)
            | Self::Protocol(_)
            | Self::Arrow(_)
            | Self::IncompleteQueryStream
            | Self::ResultTooLarge => None,
        }
    }
}

/// Projects one validated closed terminal code through the canonical catalog.
fn terminal_bifrost_error(terminal: &QueryTerminalFrame) -> BifrostError {
    let detail = terminal
        .error
        .as_ref()
        .and_then(|error| error.detail.as_ref())
        .map_or_else(
            || "query terminal reported failure".to_owned(),
            |detail| detail.as_str().to_owned(),
        );
    match terminal.error.as_ref().map(|error| error.code) {
        Some(QueryTerminalErrorCode::QueryTimeout) => BifrostError::QueryTimeout,
        Some(QueryTerminalErrorCode::QueryVisibilityUnavailable) => {
            BifrostError::QueryVisibilityUnavailable
        }
        Some(QueryTerminalErrorCode::QueryTenantInvariant) => BifrostError::QueryTenantInvariant,
        Some(QueryTerminalErrorCode::QueryReconciliationInvariant) => {
            BifrostError::QueryReconciliationInvariant
        }
        Some(QueryTerminalErrorCode::QueryPeerSecurity) => BifrostError::QueryPeerSecurity,
        Some(QueryTerminalErrorCode::QueryAuditUnavailable) => BifrostError::QueryAuditUnavailable,
        Some(QueryTerminalErrorCode::CatalogUnreachable) => {
            BifrostError::CatalogUnreachable { detail }
        }
        Some(QueryTerminalErrorCode::StorageUnreachable) => {
            BifrostError::StorageUnreachable { detail }
        }
        Some(QueryTerminalErrorCode::QueryExecutionFailed) | None => {
            BifrostError::QueryExecutionFailed
        }
    }
}

/// Rust-owned query handle that reuses an assembled client's auth and HTTP pools.
#[derive(Debug, Clone)]
pub struct QueryClient {
    /// Cheap client clone sharing the original authentication state and connection pools.
    client: WyrdClient,
}

impl QueryClient {
    /// Creates a query handle sharing the supplied Wyrd client's transport state.
    #[must_use]
    pub fn new(client: &WyrdClient) -> Self {
        Self {
            client: client.clone(),
        }
    }

    /// Starts one authenticated terminal-safe Oracle query.
    ///
    /// The returned stream owns the HTTP response body. Dropping it stops body
    /// consumption, which propagates cancellation through the transport.
    ///
    /// # Errors
    ///
    /// Returns a contract error before IO when the request is invalid, or a
    /// transport error when authentication or the HTTP request fails.
    ///
    /// # Cancellation
    ///
    /// Cancellation before the response arrives abandons the request future.
    /// After return, dropping the result stream cancels response-body consumption.
    pub async fn query(
        &self,
        request: &BifrostQueryRequest,
    ) -> Result<QueryResultStream, ValaSdkError> {
        request
            .validate()
            .map_err(|error| ValaSdkError::Protocol(error.to_string()))?;
        let request_id = RequestId::now_v7();
        let response = self
            .client
            .request_json_stream_with_id(reqwest::Method::POST, "/v1/query", request, &request_id)
            .await?;
        let deadline_ms = response
            .headers()
            .get("x-wyrd-query-deadline-ms")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<i64>().ok())
            .filter(|deadline| *deadline >= 0)
            .ok_or_else(|| {
                ValaSdkError::Protocol(
                    "query response omits a valid x-wyrd-query-deadline-ms".to_owned(),
                )
            })?;
        Ok(QueryResultStream::new(
            RawQueryStream::new(response.bytes_stream(), request.visibility),
            request_id,
            self.clone(),
            deadline_ms,
        ))
    }

    /// Lists active queries visible to the authenticated tenant.
    ///
    /// Cancelling this future abandons the pending HTTP request without
    /// creating client-owned lifecycle state.
    ///
    /// # Errors
    ///
    /// Returns stable authentication, authorization, audit, availability, or protocol errors.
    pub async fn running(&self) -> Result<Vec<RunningQuerySummary>, ValaSdkError> {
        let response: ListRunningQueriesResponse = self
            .client
            .request_json::<(), _>(reqwest::Method::GET, "/v1/query/running", None)
            .await?;
        Ok(response.queries)
    }

    /// Gets one active query visible to the authenticated tenant.
    ///
    /// Cancelling this future abandons the pending HTTP request without
    /// changing the active query.
    ///
    /// # Errors
    ///
    /// Returns stable authentication, authorization, not-found, availability, or protocol errors.
    pub async fn status(
        &self,
        request_id: &RequestId,
    ) -> Result<RunningQuerySummary, ValaSdkError> {
        self.client
            .request_json::<(), _>(
                reqwest::Method::GET,
                &format!("/v1/query/{request_id}"),
                None,
            )
            .await
            .map_err(Into::into)
    }

    /// Requests server-side cancellation without closing a local response stream.
    ///
    /// Once the server accepts cancellation, cancelling this future does not
    /// reverse the server-side lifecycle transition.
    ///
    /// # Errors
    ///
    /// Returns stable authentication, authorization, not-found, availability, or protocol errors.
    pub async fn cancel(
        &self,
        request_id: &RequestId,
    ) -> Result<CancelRunningQueryResponse, ValaSdkError> {
        self.client
            .request_json::<(), _>(
                reqwest::Method::DELETE,
                &format!("/v1/query/{request_id}"),
                None,
            )
            .await
            .map_err(Into::into)
    }

    /// Collects a query while enforcing explicit row and encoded-byte limits.
    ///
    /// # Errors
    ///
    /// Returns a protocol, Arrow, transport, failed-terminal, incomplete-stream,
    /// or bounds error. Exceeding a bound drops the live stream and never returns
    /// truncated success.
    ///
    /// # Cancellation
    ///
    /// Cancelling the future drops the response stream and its HTTP body.
    pub async fn collect_bounded(
        &self,
        request: &BifrostQueryRequest,
        limits: CollectedQueryLimits,
    ) -> Result<CollectedQueryResult, ValaSdkError> {
        self.query(request).await?.collect_bounded(limits).await
    }
}

/// Raw decoded protocol stream before Arrow IPC projection.
pub struct RawQueryStream {
    /// Incremental authenticated HTTP response body.
    body: ResponseBytes,
    /// Bounded protobuf length-delimited decoder.
    decoder: FrameDecoder,
    /// Complete frames decoded from the most recent body chunk, each paired
    /// with the record batch its Arrow fragment produced, if any.
    pending: VecDeque<(QueryStreamFrame, Option<RecordBatch>)>,
    /// Shared wire-to-domain ordering and terminal validator.
    converter: QueryStreamConverter,
    /// The one Arrow IPC decoder for this query's single split stream.
    ipc: QueryIpcDecoder,
    /// Validated terminal retained after it is yielded.
    terminal: Option<QueryTerminalFrame>,
    /// Exact length-delimited response bytes received from the HTTP body.
    received_bytes: usize,
}

impl RawQueryStream {
    /// Creates a raw stream over arbitrary response chunks.
    fn new<S>(body: S, visibility: wyrd_spec::vala::api::VisibilityMode) -> Self
    where
        S: Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
    {
        Self {
            body: Box::pin(body),
            decoder: FrameDecoder::new(MAX_FRAME_BYTES),
            pending: VecDeque::new(),
            converter: QueryStreamConverter::new(visibility),
            ipc: QueryIpcDecoder::new(),
            terminal: None,
            received_bytes: 0,
        }
    }

    /// Returns the next validated frame, or `None` only after a valid terminal.
    ///
    /// # Errors
    ///
    /// Returns a typed protocol or Arrow error for malformed, truncated,
    /// out-of-order, duplicate, post-terminal, or invalid-terminal streams.
    /// EOF before terminal returns [`ValaSdkError::IncompleteQueryStream`].
    ///
    /// # Cancellation
    ///
    /// Cancelling this operation preserves decoder state. Dropping the stream
    /// drops the HTTP response body and stops further reads.
    pub async fn next_frame(&mut self) -> Result<Option<QueryStreamFrame>, ValaSdkError> {
        Ok(self
            .next_decoded_frame()
            .await?
            .map(|(frame, _batch)| frame))
    }

    /// Returns the next validated frame with the batch its fragment decoded to.
    ///
    /// Each Arrow fragment is decoded exactly once, here, because the stream is
    /// stateful: a batch consumed twice would advance the shared decoder twice
    /// and desynchronise every later fragment. The Arrow projection therefore
    /// takes the already-decoded batch rather than re-reading the bytes.
    ///
    /// # Errors
    ///
    /// Returns a typed protocol or Arrow error for malformed, truncated,
    /// out-of-order, duplicate, post-terminal, or invalid-terminal streams.
    /// EOF before terminal returns [`ValaSdkError::IncompleteQueryStream`].
    async fn next_decoded_frame(
        &mut self,
    ) -> Result<Option<(QueryStreamFrame, Option<RecordBatch>)>, ValaSdkError> {
        loop {
            if let Some((frame, batch)) = self.pending.pop_front() {
                if let QueryStreamFrame::Terminal(terminal) = &frame {
                    self.terminal = Some(terminal.clone());
                }
                return Ok(Some((frame, batch)));
            }
            match self.body.next().await {
                Some(Ok(bytes)) => {
                    self.received_bytes =
                        self.received_bytes
                            .checked_add(bytes.len())
                            .ok_or_else(|| {
                                ValaSdkError::Protocol(
                                    "encoded response byte count overflow".to_owned(),
                                )
                            })?;
                    let frames = self
                        .decoder
                        .push::<wyrd_tonic::wyrd::v1::QueryStreamFrame>(&bytes)
                        .map_err(|error| ValaSdkError::Protocol(error.to_string()))?;
                    for frame in frames {
                        // The Arrow fragment is consumed before conversion so
                        // the converter's row accounting and the decoder's
                        // stream position advance from the same bytes exactly
                        // once.
                        let decoded = match frame.frame.as_ref() {
                            Some(wyrd_tonic::wyrd::v1::query_stream_frame::Frame::Batch(batch)) => {
                                Some(self.ipc.accept_batch(&batch.arrow_ipc_batch)?)
                            }
                            Some(wyrd_tonic::wyrd::v1::query_stream_frame::Frame::Schema(
                                schema,
                            )) => {
                                self.ipc.accept_schema(&schema.arrow_ipc_schema)?;
                                None
                            }
                            Some(wyrd_tonic::wyrd::v1::query_stream_frame::Frame::Terminal(_))
                            | None => None,
                        };
                        let batch_rows = decoded
                            .as_ref()
                            .map(|batch| {
                                u64::try_from(batch.num_rows()).map_err(|_| {
                                    ValaSdkError::Protocol("row count does not fit u64".to_owned())
                                })
                            })
                            .transpose()?;
                        let frame = self
                            .converter
                            .convert(frame, batch_rows)
                            .map_err(|error| ValaSdkError::Protocol(error.to_string()))?;
                        // The terminal's own validation already refused a
                        // success that omits its end-of-stream; closing the
                        // Arrow stream here proves the bytes it carries are the
                        // real end of this decoder's stream.
                        if let QueryStreamFrame::Terminal(terminal) = &frame
                            && terminal.outcome != QueryTerminalOutcome::Failed
                        {
                            self.ipc.accept_eos(&terminal.arrow_ipc_eos)?;
                        }
                        self.pending.push_back((frame, decoded));
                    }
                }
                Some(Err(error)) => {
                    return Err(query_body_transport_error(error));
                }
                None => {
                    self.decoder
                        .finish()
                        .map_err(|error| ValaSdkError::Protocol(error.to_string()))?;
                    if self.terminal.is_none() {
                        return Err(ValaSdkError::IncompleteQueryStream);
                    }
                    return Ok(None);
                }
            }
        }
    }

    /// Returns the validated terminal metadata, if it has been observed.
    #[must_use]
    pub fn terminal(&self) -> Option<&QueryTerminalFrame> {
        self.terminal.as_ref()
    }

    /// Returns exact length-delimited response bytes received so far.
    #[must_use]
    fn received_bytes(&self) -> usize {
        self.received_bytes
    }

    /// Returns the stream schema once its initial fragment was accepted.
    #[must_use]
    pub fn arrow_schema(&self) -> Option<&SchemaRef> {
        self.ipc.schema.as_ref()
    }

    /// Reports whether the stream's Arrow IPC end-of-stream was accepted.
    #[must_use]
    pub const fn arrow_ipc_closed(&self) -> bool {
        self.ipc.eos_accepted()
    }

    /// Returns the largest single Arrow fragment this stream held while decoding.
    ///
    /// This is the client half of the bounded-memory contract: it must never
    /// exceed the largest individual fragment, because fragments are decoded
    /// and released one at a time rather than accumulated.
    #[must_use]
    pub const fn peak_pending_frame_bytes(&self) -> usize {
        self.ipc.peak_pending_frame_bytes()
    }

    /// Returns total Arrow IPC bytes consumed across every fragment.
    #[must_use]
    pub const fn arrow_ipc_bytes(&self) -> usize {
        self.ipc.total_fragment_bytes()
    }
}

/// Maps a Reqwest body failure without confusing transport with framing.
///
/// The stable projection retains machine-readable phase and Reqwest
/// classifications. The structured diagnostic keeps the original error as the
/// tracing source for operators while the public detail remains scrubbed.
fn query_body_transport_error(error: reqwest::Error) -> ValaSdkError {
    let details = serde_json::json!({
        "transport": "http",
        "phase": "response_body",
        "timeout": error.is_timeout(),
        "connect": error.is_connect(),
        "body": error.is_body(),
        "decode": error.is_decode(),
    });
    tracing::error!(
        error = ?error,
        transport = "http",
        phase = "response_body",
        timeout = error.is_timeout(),
        connect = error.is_connect(),
        body = error.is_body(),
        decode = error.is_decode(),
        "Oracle query response body failed"
    );
    ValaSdkError::Transport(WyrdError::UpstreamFailure {
        message: "Oracle query response body failed".to_owned(),
        details,
    })
}

/// What one incomplete stream still owes the server before the caller walks away.
///
/// A stream that reached its terminal owes nothing. Anything else left a query
/// running on a server that has no other way to learn the caller is gone, so
/// exactly one settlement is owed — and which one depends entirely on whether
/// the body can still be trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamSettlement {
    /// The body is intact and can be drained to its real terminal.
    Healthy,
    /// The body failed to decode or transport and must never be read again.
    Broken,
    /// A terminal was observed, or settlement already ran exactly once.
    Settled,
}

/// Stable code proving one query is no longer running and needs no settlement.
const RUNNING_QUERY_RETIRED_CODE: &str = "WYRD_VALA_404_RUNNING_QUERY_NOT_FOUND";

/// Interval between status polls while proving a broken stream was cleaned up.
const SETTLEMENT_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

/// Arrow-projecting query stream that preserves terminal metadata.
pub struct QueryResultStream {
    /// Canonical server-visible request identity available before body polling.
    request_id: RequestId,
    /// Client reused for the one cancellation and any status proof this stream owes.
    client: QueryClient,
    /// Server-pinned absolute deadline bounding every settlement wait.
    deadline_ms: i64,
    /// Settlement this stream still owes, or [`StreamSettlement::Settled`].
    settlement: StreamSettlement,
    /// Validated logical-frame stream.
    raw: RawQueryStream,
    /// Schema established by the unique initial schema frame.
    schema: Option<SchemaRef>,
    /// Terminal retained after successful, degraded, or failed completion.
    terminal: Option<QueryTerminalFrame>,
    /// Rows yielded to the caller before the current state.
    emitted_rows: u64,
    /// Length-delimited response bytes received so far.
    encoded_bytes: usize,
}

impl QueryResultStream {
    /// Constructs the Arrow projection over a raw protocol stream.
    fn new(
        raw: RawQueryStream,
        request_id: RequestId,
        client: QueryClient,
        deadline_ms: i64,
    ) -> Self {
        Self {
            request_id,
            client,
            deadline_ms,
            settlement: StreamSettlement::Healthy,
            raw,
            schema: None,
            terminal: None,
            emitted_rows: 0,
            encoded_bytes: 0,
        }
    }

    /// Returns the canonical request identity used by server lifecycle controls.
    #[must_use]
    pub const fn request_id(&self) -> &RequestId {
        &self.request_id
    }

    /// Decodes and returns the next Arrow batch, or `None` after valid completion.
    ///
    /// # Errors
    ///
    /// Returns protocol and Arrow errors, including a schema mismatch, multiple
    /// record batches inside one batch frame, missing terminal, failed terminal,
    /// or mismatched terminal row count. A failed terminal is retained before
    /// the error is returned.
    ///
    /// # Cancellation
    ///
    /// Cancelling preserves the stream for a later call. Dropping the stream
    /// cancels response-body consumption.
    pub async fn next_batch(&mut self) -> Result<Option<RecordBatch>, ValaSdkError> {
        loop {
            let frame = self.raw.next_decoded_frame().await;
            self.encoded_bytes = self.raw.received_bytes();
            let frame = match frame {
                Ok(frame) => frame,
                Err(error) => return Err(self.mark_broken(error)),
            };
            let Some((frame, decoded)) = frame else {
                return Ok(None);
            };
            match frame {
                QueryStreamFrame::Schema(_) => {
                    self.schema = self.raw.arrow_schema().cloned();
                }
                QueryStreamFrame::Batch(_) => {
                    let Some(decoded) = decoded else {
                        return Err(self.mark_broken(ValaSdkError::Arrow(
                            "batch frame contains no record batch".to_owned(),
                        )));
                    };
                    let rows = match u64::try_from(decoded.num_rows()) {
                        Ok(rows) => rows,
                        Err(_) => {
                            return Err(self.mark_broken(ValaSdkError::Protocol(
                                "row count does not fit u64".to_owned(),
                            )));
                        }
                    };
                    let Some(emitted) = self.emitted_rows.checked_add(rows) else {
                        return Err(self
                            .mark_broken(ValaSdkError::Protocol("row count overflow".to_owned())));
                    };
                    self.emitted_rows = emitted;
                    return Ok(Some(decoded));
                }
                QueryStreamFrame::Terminal(terminal) => {
                    if let Err(error) = terminal.validate_emitted_rows(self.emitted_rows) {
                        return Err(self.mark_broken(ValaSdkError::Protocol(error.to_string())));
                    }
                    if terminal.outcome == QueryTerminalOutcome::Failed {
                        self.terminal = Some(terminal.clone());
                        // A failed terminal is the server's own settlement and
                        // can never become a successful result, so it settles
                        // here rather than waiting for the body to close.
                        self.settlement = StreamSettlement::Settled;
                        return Err(ValaSdkError::FailedTerminal { terminal });
                    }
                    return self.finish_at_clean_eof(terminal).await;
                }
            }
        }
    }

    /// Marks the body untrusted when `error` means it can no longer be read.
    ///
    /// A decode, protocol, transport, or incomplete-body failure leaves the
    /// response stream in an unknown position, so settlement must prove cleanup
    /// through the query's own status route instead of draining it. The error
    /// is returned unchanged: this marker never replaces what the caller sees.
    fn mark_broken(&mut self, error: ValaSdkError) -> ValaSdkError {
        if matches!(
            error,
            ValaSdkError::Protocol(_)
                | ValaSdkError::Arrow(_)
                | ValaSdkError::Transport(_)
                | ValaSdkError::IncompleteQueryStream
        ) {
            self.settlement = StreamSettlement::Broken;
        }
        error
    }

    /// Accepts a validated successful terminal only after the body reaches EOF.
    ///
    /// A terminal is a claim about a stream that has not ended yet. Retaining it
    /// before the body closes would let a duplicate terminal, a late schema, or
    /// a trailing batch arrive behind a result the caller already believes is
    /// complete. So the terminal is held locally, one further frame is polled
    /// within whatever remains of the server-pinned deadline, and only clean EOF
    /// promotes it to this stream's result.
    ///
    /// # Errors
    /// Returns [`ValaSdkError::Protocol`] when any frame follows the terminal,
    /// the raw stream's own error unchanged when the body fails, and
    /// [`ValaSdkError::IncompleteQueryStream`] when the deadline passes before
    /// the body closes. Every one of those marks the body broken first.
    async fn finish_at_clean_eof(
        &mut self,
        terminal: QueryTerminalFrame,
    ) -> Result<Option<RecordBatch>, ValaSdkError> {
        let remaining = self.remaining();
        let observed = tokio::time::timeout(remaining, self.raw.next_decoded_frame()).await;
        self.encoded_bytes = self.raw.received_bytes();
        match observed {
            Ok(Ok(None)) => {
                self.terminal = Some(terminal);
                // A terminal followed by clean EOF is the server's own
                // settlement. Nothing is owed.
                self.settlement = StreamSettlement::Settled;
                Ok(None)
            }
            Ok(Ok(Some(_))) => Err(self.mark_broken(ValaSdkError::Protocol(
                "a frame followed the query terminal".to_owned(),
            ))),
            Ok(Err(error)) => Err(self.mark_broken(error)),
            Err(_) => Err(self.mark_broken(ValaSdkError::IncompleteQueryStream)),
        }
    }

    /// Collects this stream within explicit result ceilings.
    ///
    /// The returned result retains the authoritative schema decoded from the
    /// initial schema frame, including when the query produces no batches. A
    /// successful terminal without that schema is rejected as incomplete.
    ///
    /// # Errors
    ///
    /// Returns a stream error or [`ValaSdkError::ResultTooLarge`] before
    /// retaining a batch that would exceed either ceiling.
    ///
    /// # Cancellation
    ///
    /// Cancelling drops this owned stream and stops response-body consumption.
    pub async fn collect_bounded(
        mut self,
        limits: CollectedQueryLimits,
    ) -> Result<CollectedQueryResult, ValaSdkError> {
        let mut batches = Vec::new();
        let mut rows = 0usize;
        loop {
            let batch = match self.next_batch().await {
                Ok(batch) => batch,
                Err(error) => return Err(self.settle_with(error).await),
            };
            if self.encoded_bytes > limits.max_encoded_bytes {
                return Err(self.settle_with(ValaSdkError::ResultTooLarge).await);
            }
            let Some(batch) = batch else {
                break;
            };
            let next_rows = match rows.checked_add(batch.num_rows()) {
                Some(next_rows) if next_rows <= limits.max_rows => next_rows,
                _ => return Err(self.settle_with(ValaSdkError::ResultTooLarge).await),
            };
            rows = next_rows;
            batches.push(batch);
        }
        let terminal = self.terminal.ok_or(ValaSdkError::IncompleteQueryStream)?;
        let schema = self.schema.ok_or(ValaSdkError::IncompleteQueryStream)?;
        Ok(CollectedQueryResult {
            schema,
            batches,
            terminal,
            rows,
            encoded_bytes: self.encoded_bytes,
        })
    }

    /// Settles this stream because `error` ended it, and returns that error.
    ///
    /// The originating error is always what the caller sees: settlement is the
    /// client's obligation to the server, not a second failure to report. A
    /// decode or transport error also means the body can no longer be trusted,
    /// so it downgrades settlement to the status-polling proof before running.
    async fn settle_with(&mut self, error: ValaSdkError) -> ValaSdkError {
        let error = self.mark_broken(error);
        self.settle().await;
        error
    }

    /// Settles one incomplete stream exactly once.
    ///
    /// A stream that already reached its terminal returns immediately, and a
    /// second call is a no-op, so cancellation is issued at most once per
    /// query. A healthy body is drained to its real terminal, which is both the
    /// cheapest proof of cleanup and the only one that keeps the connection
    /// reusable. A broken body is never read again; the query's own status
    /// route is polled instead until it reports the entry is gone.
    ///
    /// Every wait is bounded by the server-pinned deadline this stream was
    /// opened with, so settlement can never outlive the query it settles. If
    /// the deadline passes without proof, that is reported as scrubbed
    /// telemetry rather than raised: the caller's own error is the one that
    /// matters, and the server still owns its cleanup.
    pub async fn settle(&mut self) {
        let owed = std::mem::replace(&mut self.settlement, StreamSettlement::Settled);
        match owed {
            StreamSettlement::Settled => return,
            StreamSettlement::Healthy | StreamSettlement::Broken => {}
        }
        let remaining = self.remaining();
        if remaining.is_zero() {
            Self::warn_unconfirmed(&self.request_id, self.deadline_ms);
            return;
        }
        let client = self.client.clone();
        let request_id = self.request_id.clone();
        let _ = tokio::time::timeout(remaining, client.cancel(&request_id)).await;
        if owed == StreamSettlement::Healthy && self.drain_to_terminal().await {
            return;
        }
        Self::poll_until_retired(client, request_id, self.deadline_ms).await;
    }

    /// Drains a healthy body to its terminal within the query deadline.
    ///
    /// Returns whether the terminal actually arrived. A drain that hits the
    /// deadline or a late decode failure leaves the caller with no proof, so it
    /// reports `false` and the status poll takes over.
    async fn drain_to_terminal(&mut self) -> bool {
        let drained = tokio::time::timeout(self.remaining(), async {
            while let Ok(Some(_)) = self.next_batch().await {}
            self.terminal.is_some()
        })
        .await;
        drained.unwrap_or(false)
    }

    /// Polls one query's status until the server proves it is no longer running.
    ///
    /// It takes its owners by value rather than borrowing the stream, so the
    /// returned future stays `Send` without requiring the stream itself to be
    /// `Sync` — callers spawn settlement onto a multi-threaded runtime.
    ///
    /// Only the not-found projection is proof; every other outcome — still
    /// running, unavailable, a transport failure — means the answer is not in
    /// yet, so the poll simply waits out its fixed interval and asks again
    /// until the deadline retires it.
    async fn poll_until_retired(client: QueryClient, request_id: RequestId, deadline_ms: i64) {
        loop {
            let remaining = Self::remaining_until(deadline_ms);
            if remaining.is_zero() {
                Self::warn_unconfirmed(&request_id, deadline_ms);
                return;
            }
            if let Ok(outcome) = tokio::time::timeout(remaining, client.status(&request_id)).await
                && outcome.is_err_and(|error| error.code() == RUNNING_QUERY_RETIRED_CODE)
            {
                return;
            }
            let remaining = Self::remaining_until(deadline_ms);
            if remaining.is_zero() {
                Self::warn_unconfirmed(&request_id, deadline_ms);
                return;
            }
            tokio::time::sleep(SETTLEMENT_POLL_INTERVAL.min(remaining)).await;
        }
    }

    /// Reports unproven settlement as scrubbed telemetry rather than an error.
    ///
    /// The caller's own error is the one that matters and the server still owns
    /// its cleanup, so an unconfirmed settlement is only ever observability.
    fn warn_unconfirmed(request_id: &RequestId, deadline_ms: i64) {
        tracing::warn!(
            request_id = %request_id,
            deadline_ms,
            "Oracle query settlement remains unconfirmed at its deadline"
        );
    }

    /// Returns the time left before this query's server-pinned deadline.
    ///
    /// A deadline that has already passed yields zero rather than a negative
    /// duration, which makes every bounded settlement wait resolve immediately
    /// instead of panicking on the conversion.
    fn remaining(&self) -> std::time::Duration {
        Self::remaining_until(self.deadline_ms)
    }

    /// Returns the time left before `deadline_ms`, without borrowing a stream.
    ///
    /// Settlement's status poll owns its collaborators by value so its future
    /// stays `Send`, and it must recompute the remainder before every wait, so
    /// the deadline arithmetic lives here rather than on `&self`.
    fn remaining_until(deadline_ms: i64) -> std::time::Duration {
        let now = std::time::SystemTime::UNIX_EPOCH
            .elapsed()
            .map_or(i64::MAX, |since| {
                i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
            });
        u64::try_from(deadline_ms - now).map_or(std::time::Duration::ZERO, |remaining| {
            std::time::Duration::from_millis(remaining)
        })
    }

    /// Returns this stream's server-pinned absolute deadline in epoch milliseconds.
    #[must_use]
    pub const fn deadline_ms(&self) -> i64 {
        self.deadline_ms
    }

    /// Returns terminal metadata only after validated completion or failure.
    #[must_use]
    pub fn terminal(&self) -> Option<&QueryTerminalFrame> {
        self.terminal.as_ref()
    }

    /// Returns the decoded query schema after the schema frame is consumed.
    #[must_use]
    pub fn schema(&self) -> Option<&SchemaRef> {
        self.schema.as_ref()
    }

    /// Returns exact length-delimited response bytes received so far.
    #[must_use]
    pub fn encoded_bytes(&self) -> usize {
        self.encoded_bytes
    }

    /// Reports whether the query's Arrow IPC stream was explicitly closed.
    ///
    /// A successful or degraded query closes its single IPC stream with the
    /// end-of-stream delta carried by the terminal frame. A caller that must
    /// distinguish a complete result from a truncated one reads this rather
    /// than inferring completeness from an absent batch.
    #[must_use]
    pub const fn arrow_ipc_closed(&self) -> bool {
        self.raw.arrow_ipc_closed()
    }

    /// Returns the largest single Arrow fragment this stream held while decoding.
    #[must_use]
    pub const fn peak_pending_frame_bytes(&self) -> usize {
        self.raw.peak_pending_frame_bytes()
    }

    /// Returns total Arrow IPC bytes this query carried across every fragment.
    ///
    /// The query is one split IPC stream, so this counts one schema prefix,
    /// each batch's bare delta, and the single end-of-stream delta — strictly
    /// less than re-encoding each batch as its own standalone stream once the
    /// query returns more than one batch.
    #[must_use]
    pub const fn arrow_ipc_bytes(&self) -> usize {
        self.raw.arrow_ipc_bytes()
    }
}

/// Explicit collection ceilings used by CLI, MCP, and tests.
#[derive(Debug, Clone, Copy)]
pub struct CollectedQueryLimits {
    /// Maximum decoded rows.
    pub max_rows: usize,
    /// Maximum length-delimited response bytes accounted for by the collector.
    pub max_encoded_bytes: usize,
}

/// Bounded Arrow query result with its authoritative schema and validated terminal metadata.
#[derive(Debug)]
pub struct CollectedQueryResult {
    /// Schema decoded from the stream's required initial schema frame.
    pub schema: SchemaRef,
    /// Collected record batches.
    pub batches: Vec<RecordBatch>,
    /// Terminal metadata.
    pub terminal: QueryTerminalFrame,
    /// Total decoded rows.
    pub rows: usize,
    /// Total length-delimited response bytes accounted for by the collector.
    pub encoded_bytes: usize,
}

/// Stateful Arrow IPC decoder for one public query stream.
///
/// The public stream is a single Arrow IPC stream split across Wyrd frames, so
/// the schema and every dictionary arrive once, in earlier fragments. A decoder
/// constructed per frame cannot read that stream at all; this owner is
/// constructed once per query and consumes fragments in order.
///
/// The server tier has its own copy of this state machine in
/// `vala-bifrost-redux`. Client-tier crates may not depend on the Bifrost
/// server engine, so the wire contract — not shared code — is what keeps the
/// two honest, and the journeys in `tests/pg_bifrost_e2e.rs` are what prove it.
///
/// End-of-stream receipt is tracked here rather than delegated to
/// [`arrow::ipc::reader::StreamDecoder::finish`], which reports success both
/// for a stream that closed cleanly and for one that never started.
struct QueryIpcDecoder {
    /// Arrow's push decoder, retaining schema and dictionary state.
    decoder: arrow::ipc::reader::StreamDecoder,
    /// Schema recovered from the required initial schema fragment.
    schema: Option<SchemaRef>,
    /// Whether the terminal's explicit end-of-stream delta was accepted.
    eos_accepted: bool,
    /// Largest single fragment this decoder has held while decoding.
    peak_pending_frame_bytes: usize,
    /// Total Arrow IPC bytes this decoder consumed across every fragment.
    total_fragment_bytes: usize,
}

impl QueryIpcDecoder {
    /// Creates a decoder positioned before the required schema fragment.
    fn new() -> Self {
        Self {
            decoder: arrow::ipc::reader::StreamDecoder::new(),
            schema: None,
            eos_accepted: false,
            peak_pending_frame_bytes: 0,
            total_fragment_bytes: 0,
        }
    }

    /// Consumes the initial schema fragment and returns the stream schema.
    ///
    /// The schema is read with a throwaway [`StreamReader`] because Arrow's
    /// push decoder only completes a zero-body message once the next fragment
    /// arrives; the same bytes are still pushed through the push decoder, which
    /// is what carries schema and dictionary state into later fragments.
    ///
    /// # Errors
    ///
    /// Returns [`ValaSdkError::Protocol`] for a duplicate or post-terminal
    /// schema, and [`ValaSdkError::Arrow`] when the fragment is not exactly one
    /// schema message.
    fn accept_schema(&mut self, bytes: &[u8]) -> Result<SchemaRef, ValaSdkError> {
        if self.schema.is_some() || self.eos_accepted {
            return Err(ValaSdkError::Protocol(
                "query stream carries more than one schema frame".to_owned(),
            ));
        }
        let mut prefix = StreamReader::try_new(Cursor::new(bytes), None)
            .map_err(|error| ValaSdkError::Arrow(error.to_string()))?;
        let schema = prefix.schema();
        if prefix
            .next()
            .transpose()
            .map_err(|error| ValaSdkError::Arrow(error.to_string()))?
            .is_some()
        {
            return Err(ValaSdkError::Arrow(
                "schema frame unexpectedly contains a record batch".to_owned(),
            ));
        }
        if self.feed(bytes)?.is_some() {
            return Err(ValaSdkError::Arrow(
                "schema frame unexpectedly contains a record batch".to_owned(),
            ));
        }
        self.schema = Some(SchemaRef::clone(&schema));
        Ok(schema)
    }

    /// Consumes one batch fragment and returns its decoded record batch.
    ///
    /// # Errors
    ///
    /// Returns [`ValaSdkError::Protocol`] before the schema or after
    /// end-of-stream, and [`ValaSdkError::Arrow`] when the fragment does not
    /// decode to exactly one record batch matching the stream schema.
    fn accept_batch(&mut self, bytes: &[u8]) -> Result<RecordBatch, ValaSdkError> {
        let expected = self
            .schema
            .as_ref()
            .ok_or_else(|| ValaSdkError::Protocol("batch arrived before schema".to_owned()))?;
        if self.eos_accepted {
            return Err(ValaSdkError::Protocol(
                "query stream frame arrived after its end-of-stream".to_owned(),
            ));
        }
        let expected = SchemaRef::clone(expected);
        let batch = self.feed(bytes)?.ok_or_else(|| {
            ValaSdkError::Arrow("batch frame contains no record batch".to_owned())
        })?;
        if batch.schema().as_ref() != expected.as_ref() {
            return Err(ValaSdkError::Arrow(
                "batch schema does not match the initial schema".to_owned(),
            ));
        }
        Ok(batch)
    }

    /// Consumes the terminal's end-of-stream delta and closes the stream.
    ///
    /// # Errors
    ///
    /// Returns [`ValaSdkError::Protocol`] before the schema or on a second
    /// end-of-stream, and [`ValaSdkError::Arrow`] when the delta is absent,
    /// carries a record batch, or leaves a partial message behind.
    fn accept_eos(&mut self, bytes: &[u8]) -> Result<(), ValaSdkError> {
        if self.schema.is_none() || self.eos_accepted {
            return Err(ValaSdkError::Protocol(
                "query stream end-of-stream is out of order".to_owned(),
            ));
        }
        if bytes.is_empty() {
            return Err(ValaSdkError::Arrow(
                "successful query terminal carries no Arrow IPC end-of-stream".to_owned(),
            ));
        }
        if self.feed(bytes)?.is_some() {
            return Err(ValaSdkError::Arrow(
                "query end-of-stream unexpectedly contains a record batch".to_owned(),
            ));
        }
        self.decoder
            .finish()
            .map_err(|error| ValaSdkError::Arrow(error.to_string()))?;
        self.eos_accepted = true;
        Ok(())
    }

    /// Reports whether the explicit end-of-stream delta was accepted.
    const fn eos_accepted(&self) -> bool {
        self.eos_accepted
    }

    /// Returns the largest single fragment this decoder held while decoding.
    const fn peak_pending_frame_bytes(&self) -> usize {
        self.peak_pending_frame_bytes
    }

    /// Returns the total Arrow IPC bytes consumed across every fragment.
    const fn total_fragment_bytes(&self) -> usize {
        self.total_fragment_bytes
    }

    /// Pushes one fragment through Arrow's decoder, allowing at most one batch.
    ///
    /// # Errors
    ///
    /// Returns [`ValaSdkError::Arrow`] when Arrow rejects the bytes or the
    /// fragment yields more than one record batch.
    fn feed(&mut self, bytes: &[u8]) -> Result<Option<RecordBatch>, ValaSdkError> {
        self.peak_pending_frame_bytes = self.peak_pending_frame_bytes.max(bytes.len());
        self.total_fragment_bytes = self.total_fragment_bytes.saturating_add(bytes.len());
        let mut buffer = arrow::buffer::Buffer::from_vec(bytes.to_vec());
        let mut decoded = None;
        while !buffer.is_empty() {
            match self
                .decoder
                .decode(&mut buffer)
                .map_err(|error| ValaSdkError::Arrow(error.to_string()))?
            {
                Some(batch) if decoded.is_none() => decoded = Some(batch),
                Some(_) => {
                    return Err(ValaSdkError::Arrow(
                        "batch frame contains more than one record batch".to_owned(),
                    ));
                }
                None => {}
            }
        }
        Ok(decoded)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::task::{Context, Poll};

    use arrow::array::Int64Array;
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::ipc::writer::StreamWriter;
    use bytes::Bytes;
    use futures_util::stream;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use wyrd_spec::error::WyrdError;
    use wyrd_spec::vala::api::{
        QueryBatchFrame, QueryErrorDetail, QueryFreshness, QuerySchemaFrame, QuerySource,
        QueryTerminalError, QueryTerminalErrorCode, QueryWarning, SourceCompletion,
        SourceCompletionOutcome, VisibilityMode,
    };
    use wyrd_tonic::frame_codec::FrameEncoder;
    use wyrd_tonic::wyrd::v1 as proto;

    use super::*;

    /// Counts the lifecycle calls one settling stream makes against a server.
    #[derive(Debug, Default)]
    struct LifecycleCounts {
        /// Cancellation requests received on the query's own route.
        cancels: AtomicUsize,
        /// Status polls received on the query's own route.
        statuses: AtomicUsize,
    }

    /// Serves the two lifecycle routes settlement uses, counting every call.
    ///
    /// The status route always reports the query is gone, which is the proof a
    /// broken stream polls for. Each connection is answered once and closed, so
    /// the counters record exactly how many requests the client actually made.
    fn lifecycle_server(counts: Arc<LifecycleCounts>) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("test listener binds");
        let address = listener.local_addr().expect("listener has an address");
        listener
            .set_nonblocking(true)
            .expect("listener converts to tokio");
        let listener = TcpListener::from_std(listener).expect("listener adopts the runtime");
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let counts = Arc::clone(&counts);
                tokio::spawn(async move {
                    let mut request = [0_u8; 2048];
                    let Ok(read) = socket.read(&mut request).await else {
                        return;
                    };
                    let request = String::from_utf8_lossy(&request[..read]).to_string();
                    let response = if request.starts_with("POST /auth/token") {
                        // The client exchanges its API key before any lifecycle
                        // route; a far-future expiry keeps one exchange enough.
                        let body = format!(
                            "{{\"access_token\":\"test-token\",\"token_type\":\"Bearer\",\"expires_at\":\"{}\"}}",
                            "2099-01-01T00:00:00Z"
                        );
                        format!(
                            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                            body.len()
                        )
                    } else if request.starts_with("DELETE ") {
                        counts.cancels.fetch_add(1, Ordering::AcqRel);
                        let body = "{\"cancellation_requested\":true}";
                        format!(
                            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                            body.len()
                        )
                    } else {
                        counts.statuses.fetch_add(1, Ordering::AcqRel);
                        let body = "{\"code\":\"WYRD_VALA_404_RUNNING_QUERY_NOT_FOUND\",\"detail\":\"gone\"}";
                        format!(
                            "HTTP/1.1 404 Not Found\r\ncontent-type: application/problem+json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                            body.len()
                        )
                    };
                    let _ = socket.write_all(response.as_bytes()).await;
                });
            }
        });
        format!("http://{address}")
    }

    /// Builds a stream bound to a live lifecycle server with a live deadline.
    fn settling_stream(
        chunks: Vec<Vec<u8>>,
        base_url: &str,
        deadline_ms: i64,
    ) -> QueryResultStream {
        let body = stream::iter(chunks.into_iter().map(|chunk| Ok(Bytes::from(chunk))));
        QueryResultStream::new(
            RawQueryStream::new(body, VisibilityMode::PublishedOnly),
            RequestId::now_v7(),
            client_for(base_url),
            deadline_ms,
        )
    }

    /// Returns an absolute epoch-millisecond deadline `millis` from now.
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

    /// Every incomplete exit settles the server exactly once, under the deadline.
    ///
    /// The five exits a caller can actually take are covered together because
    /// the contract they share is a single one: whatever ends the stream, the
    /// server learns about it once, the caller keeps its own error, and nothing
    /// waits past the deadline the server pinned.
    #[tokio::test]
    async fn query_result_stream_settles_every_incomplete_exit_once() {
        let schema = test_schema();
        let (_, prefix_only) = TestQueryIpc::open(&schema);
        let (mut ipc, prefix) = TestQueryIpc::open(&schema);
        let batch = ipc.batch(&schema, &[1, 2, 3]);
        let eos = ipc.close();
        let complete = vec![
            encoded(QueryStreamFrame::Schema(QuerySchemaFrame {
                schema_fingerprint: String::new(),
                arrow_ipc_schema: prefix.clone(),
            })),
            encoded(QueryStreamFrame::Batch(QueryBatchFrame {
                arrow_ipc_batch: batch.clone(),
            })),
            encoded(QueryStreamFrame::Terminal(success_terminal(
                VisibilityMode::PublishedOnly,
                3,
                eos,
            ))),
        ];

        // A stream that reached its terminal owes the server nothing.
        let counts = Arc::new(LifecycleCounts::default());
        let base_url = lifecycle_server(Arc::clone(&counts));
        let mut settled = settling_stream(complete.clone(), &base_url, deadline_in(30_000));
        while settled
            .next_batch()
            .await
            .expect("complete stream decodes")
            .is_some()
        {}
        settled.settle().await;
        assert_eq!(
            counts.cancels.load(Ordering::Acquire),
            0,
            "a terminal is the server's own settlement"
        );

        // A healthy stream abandoned early cancels once and drains its body.
        let counts = Arc::new(LifecycleCounts::default());
        let base_url = lifecycle_server(Arc::clone(&counts));
        let mut healthy = settling_stream(complete, &base_url, deadline_in(30_000));
        healthy.next_batch().await.expect("first batch decodes");
        healthy.settle().await;
        healthy.settle().await;
        assert_eq!(
            counts.cancels.load(Ordering::Acquire),
            1,
            "settlement cancels at most once across repeated calls"
        );
        assert!(
            healthy.terminal().is_some(),
            "a healthy body is drained to its real terminal"
        );
        assert_eq!(
            counts.statuses.load(Ordering::Acquire),
            0,
            "a drained terminal needs no status proof"
        );

        // A bounded-result overflow settles before the caller sees the error.
        let (mut ipc, prefix) = TestQueryIpc::open(&schema);
        let wide = ipc.batch(&schema, &[1, 2, 3, 4, 5]);
        let eos = ipc.close();
        let counts = Arc::new(LifecycleCounts::default());
        let base_url = lifecycle_server(Arc::clone(&counts));
        let overflow = settling_stream(
            vec![
                encoded(QueryStreamFrame::Schema(QuerySchemaFrame {
                    schema_fingerprint: String::new(),
                    arrow_ipc_schema: prefix,
                })),
                encoded(QueryStreamFrame::Batch(QueryBatchFrame {
                    arrow_ipc_batch: wide,
                })),
                encoded(QueryStreamFrame::Terminal(success_terminal(
                    VisibilityMode::PublishedOnly,
                    5,
                    eos,
                ))),
            ],
            &base_url,
            deadline_in(30_000),
        );
        let error = overflow
            .collect_bounded(CollectedQueryLimits {
                max_rows: 1,
                max_encoded_bytes: usize::MAX,
            })
            .await
            .expect_err("the row ceiling refuses this result");
        assert!(matches!(error, ValaSdkError::ResultTooLarge));
        assert_eq!(
            counts.cancels.load(Ordering::Acquire),
            1,
            "an overflow exit settles the server once"
        );

        // A protocol failure keeps its own error and proves cleanup by status.
        let counts = Arc::new(LifecycleCounts::default());
        let base_url = lifecycle_server(Arc::clone(&counts));
        let broken = settling_stream(
            vec![encoded(QueryStreamFrame::Batch(QueryBatchFrame {
                arrow_ipc_batch: vec![0xff, 0xff, 0xff, 0xff],
            }))],
            &base_url,
            deadline_in(30_000),
        );
        let error = broken
            .collect_bounded(CollectedQueryLimits {
                max_rows: usize::MAX,
                max_encoded_bytes: usize::MAX,
            })
            .await
            .expect_err("an undecodable batch fails");
        assert!(
            matches!(error, ValaSdkError::Protocol(_) | ValaSdkError::Arrow(_)),
            "settlement never replaces the originating error: {error:?}"
        );
        assert_eq!(counts.cancels.load(Ordering::Acquire), 1);
        assert!(
            counts.statuses.load(Ordering::Acquire) >= 1,
            "a broken body is proven retired by status, never by reading it again"
        );

        // A deadline already in the past bounds settlement to no waiting at all,
        // and no leg of it receives a fresh budget: an expired stream neither
        // cancels, drains, nor polls status.
        let counts = Arc::new(LifecycleCounts::default());
        let base_url = lifecycle_server(Arc::clone(&counts));
        let mut expired = settling_stream(Vec::new(), &base_url, deadline_in(-1));
        expired.settlement = StreamSettlement::Broken;
        tokio::time::timeout(std::time::Duration::from_secs(5), expired.settle())
            .await
            .expect("an expired deadline settles without waiting");
        assert_eq!(
            counts.cancels.load(Ordering::Acquire),
            0,
            "an elapsed deadline gives cancellation no fresh budget"
        );
        assert_eq!(
            counts.statuses.load(Ordering::Acquire),
            0,
            "an elapsed deadline gives status polling no fresh budget"
        );

        // A cancellation endpoint that never answers cannot outlive the
        // stream's own deadline.
        let base_url = unanswering_server();
        let mut hung = settling_stream(Vec::new(), &base_url, deadline_in(400));
        hung.settlement = StreamSettlement::Broken;
        tokio::time::timeout(std::time::Duration::from_secs(10), hung.settle())
            .await
            .expect("settlement returns by the stream deadline, not the server's");

        // Every direct body failure marks the stream broken before it returns,
        // so settlement never reads that body again.
        for (label, chunks) in [
            (
                "an undecodable frame",
                vec![vec![0xff_u8, 0xff, 0xff, 0xff, 0xff, 0xff]],
            ),
            (
                "a body that ends before its terminal",
                vec![encoded(QueryStreamFrame::Schema(QuerySchemaFrame {
                    schema_fingerprint: String::new(),
                    arrow_ipc_schema: prefix_only.clone(),
                }))],
            ),
        ] {
            let counts = Arc::new(LifecycleCounts::default());
            let base_url = lifecycle_server(Arc::clone(&counts));
            let polls = Arc::new(AtomicUsize::new(0));
            let mut stream =
                counted_stream(chunks, Arc::clone(&polls), &base_url, deadline_in(30_000));
            let error = stream.next_batch().await.expect_err(label).to_string();
            let polled = polls.load(Ordering::Acquire);
            assert_eq!(
                stream.settlement,
                StreamSettlement::Broken,
                "{label} marks the body broken before returning: {error}"
            );
            stream.settle().await;
            assert_eq!(
                polls.load(Ordering::Acquire),
                polled,
                "{label} leaves a body settlement must never repoll"
            );
            assert_eq!(counts.cancels.load(Ordering::Acquire), 1);
            assert!(counts.statuses.load(Ordering::Acquire) >= 1);
        }

        // A real transport failure takes the same path: the truncated body is
        // marked broken and never read again.
        let counts = Arc::new(LifecycleCounts::default());
        let base_url = lifecycle_server(Arc::clone(&counts));
        let mut truncated = QueryResultStream::new(
            RawQueryStream::new(truncated_body().await, VisibilityMode::PublishedOnly),
            RequestId::now_v7(),
            client_for(&base_url),
            deadline_in(30_000),
        );
        let error = truncated
            .next_batch()
            .await
            .expect_err("a truncated body is a transport failure");
        assert!(
            matches!(error, ValaSdkError::Transport(_)),
            "the caller keeps its transport error: {error:?}"
        );
        assert_eq!(truncated.settlement, StreamSettlement::Broken);

        // A successful terminal is provisional until the body closes: anything
        // that follows it is a protocol failure and can never become a result.
        let (mut ipc, prefix) = TestQueryIpc::open(&schema);
        let batch = ipc.batch(&schema, &[1, 2, 3]);
        let eos = ipc.close();
        let head = vec![
            encoded(QueryStreamFrame::Schema(QuerySchemaFrame {
                schema_fingerprint: String::new(),
                arrow_ipc_schema: prefix.clone(),
            })),
            encoded(QueryStreamFrame::Batch(QueryBatchFrame {
                arrow_ipc_batch: batch.clone(),
            })),
            encoded(QueryStreamFrame::Terminal(success_terminal(
                VisibilityMode::PublishedOnly,
                3,
                eos.clone(),
            ))),
        ];
        for (label, trailing) in [
            (
                "a duplicate terminal",
                encoded(QueryStreamFrame::Terminal(success_terminal(
                    VisibilityMode::PublishedOnly,
                    3,
                    eos.clone(),
                ))),
            ),
            (
                "a late schema",
                encoded(QueryStreamFrame::Schema(QuerySchemaFrame {
                    schema_fingerprint: String::new(),
                    arrow_ipc_schema: prefix.clone(),
                })),
            ),
            (
                "a trailing batch",
                encoded(QueryStreamFrame::Batch(QueryBatchFrame {
                    arrow_ipc_batch: batch.clone(),
                })),
            ),
        ] {
            let counts = Arc::new(LifecycleCounts::default());
            let base_url = lifecycle_server(Arc::clone(&counts));
            let mut chunks = head.clone();
            chunks.push(trailing);
            let mut stream = settling_stream(chunks, &base_url, deadline_in(30_000));
            stream.next_batch().await.expect("the batch decodes");
            let error = stream
                .next_batch()
                .await
                .expect_err("{label} after a terminal is refused");
            assert!(
                matches!(error, ValaSdkError::Protocol(_) | ValaSdkError::Arrow(_)),
                "{label} after a terminal is a protocol failure: {error:?}"
            );
            assert_eq!(
                stream.settlement,
                StreamSettlement::Broken,
                "{label} after a terminal marks the body broken"
            );
            assert!(
                stream.terminal().is_none(),
                "{label} after a terminal never becomes a successful result"
            );
        }

        // The same terminal followed by clean EOF alone is what settles.
        let counts = Arc::new(LifecycleCounts::default());
        let base_url = lifecycle_server(Arc::clone(&counts));
        let mut clean = settling_stream(head, &base_url, deadline_in(30_000));
        clean.next_batch().await.expect("the batch decodes");
        assert!(
            clean
                .next_batch()
                .await
                .expect("clean EOF completes the stream")
                .is_none()
        );
        assert!(
            clean.terminal().is_some(),
            "clean EOF alone promotes the terminal to this stream's result"
        );
        clean.settle().await;
        assert_eq!(
            counts.cancels.load(Ordering::Acquire),
            0,
            "a terminal proven by clean EOF owes the server nothing"
        );
    }

    /// Serves the token exchange and then answers nothing at all.
    ///
    /// Settlement's cancellation and status legs each have to be bounded by the
    /// stream's own deadline rather than by the server answering, which is only
    /// observable against a peer that never does.
    fn unanswering_server() -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("test listener binds");
        let address = listener.local_addr().expect("listener has an address");
        listener
            .set_nonblocking(true)
            .expect("listener converts to tokio");
        let listener = TcpListener::from_std(listener).expect("listener adopts the runtime");
        tokio::spawn(async move {
            let mut held = Vec::new();
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let mut request = [0_u8; 2048];
                let Ok(read) = socket.read(&mut request).await else {
                    continue;
                };
                if String::from_utf8_lossy(&request[..read]).starts_with("POST /auth/token") {
                    let body = "{\"access_token\":\"test-token\",\"token_type\":\"Bearer\",\"expires_at\":\"2099-01-01T00:00:00Z\"}";
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                } else {
                    // Held open and unanswered for the rest of the test.
                    held.push(socket);
                }
            }
        });
        format!("http://{address}")
    }

    /// Builds a settling stream whose body counts every poll it receives.
    ///
    /// The proof that a broken body is never read again is exactly that this
    /// counter stops moving once the failure is returned.
    fn counted_stream(
        chunks: Vec<Vec<u8>>,
        polls: Arc<AtomicUsize>,
        base_url: &str,
        deadline_ms: i64,
    ) -> QueryResultStream {
        let body = stream::unfold(
            (chunks.into_iter(), polls),
            |(mut chunks, polls)| async move {
                polls.fetch_add(1, Ordering::AcqRel);
                chunks
                    .next()
                    .map(|chunk| (Ok(Bytes::from(chunk)), (chunks, polls)))
            },
        );
        QueryResultStream::new(
            RawQueryStream::new(body, VisibilityMode::PublishedOnly),
            RequestId::now_v7(),
            client_for(base_url),
            deadline_ms,
        )
    }

    /// Returns a real HTTP response body that ends before its content length.
    ///
    /// A `reqwest::Error` has no constructor, so the only way to exercise the
    /// transport branch is to make one.
    async fn truncated_body() -> impl Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static
    {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test listener binds");
        let address = listener.local_addr().expect("listener has an address");
        tokio::spawn(async move {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let mut request = [0_u8; 1024];
            let _ = socket.read(&mut request).await;
            let _ = socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-type: application/vnd.wyrd.bifrost-query-stream\r\ncontent-length: 100\r\nconnection: close\r\n\r\nabc",
                )
                .await;
        });
        reqwest::Client::new()
            .get(format!("http://{address}/v1/query"))
            .send()
            .await
            .expect("response headers arrive")
            .bytes_stream()
    }

    /// Proves an HTTP body failure remains a structured transport error.
    #[tokio::test]
    async fn query_body_failure_is_structured_transport_error() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test listener binds");
        let address = listener.local_addr().expect("listener has an address");
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("client connects");
            let mut request = [0_u8; 1024];
            let _ = socket.read(&mut request).await.expect("request reads");
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-type: application/vnd.wyrd.bifrost-query-stream\r\ncontent-length: 100\r\nconnection: close\r\n\r\nabc",
                )
                .await
                .expect("truncated response writes");
        });
        let response = reqwest::Client::new()
            .get(format!("http://{address}/v1/query"))
            .send()
            .await
            .expect("response headers arrive");
        let mut stream =
            RawQueryStream::new(response.bytes_stream(), VisibilityMode::PublishedOnly);

        let error = stream
            .next_frame()
            .await
            .expect_err("truncated body is a transport failure");
        let ValaSdkError::Transport(WyrdError::UpstreamFailure { details, .. }) = error else {
            panic!("body failure must retain the transport projection");
        };
        assert_eq!(details["transport"], "http");
        assert_eq!(details["phase"], "response_body");
        assert_eq!(details["timeout"], false);
        assert_eq!(details["connect"], false);
        assert_eq!(details["body"], false);
        assert_eq!(details["decode"], true);
        server.await.expect("test server exits");
    }

    /// Stable metadata accessors preserve typed transport and terminal diagnostics.
    #[test]
    fn vala_sdk_error_projects_typed_detail_and_safe_details() {
        let transport = ValaSdkError::Transport(WyrdError::PermissionDeniedRbac {
            message: "query denied".to_owned(),
            details: serde_json::json!({"required_scope": "bifrost_query:read"}),
        });
        assert_eq!(transport.detail(), "query denied");
        assert_eq!(
            transport.safe_details(),
            Some(serde_json::json!({"required_scope": "bifrost_query:read"}))
        );

        let mut terminal = failed_terminal(0);
        terminal
            .error
            .as_mut()
            .expect("failed terminal has error")
            .detail =
            Some(QueryErrorDetail::new("source failed").expect("fixed detail is scrubbed"));
        let failed = ValaSdkError::FailedTerminal {
            terminal: terminal.clone(),
        };
        assert_eq!(failed.detail(), "source failed");
        assert_eq!(
            failed.safe_details(),
            Some(serde_json::to_value(terminal).expect("terminal serializes"))
        );

        let protocol = ValaSdkError::Protocol("safe protocol detail".to_owned());
        assert_eq!(protocol.detail(), "safe protocol detail");
        assert_eq!(protocol.safe_details(), None);

        let arrow = ValaSdkError::Arrow("safe Arrow detail".to_owned());
        assert_eq!(arrow.detail(), "safe Arrow detail");
        assert_eq!(arrow.safe_details(), None);

        let incomplete = ValaSdkError::IncompleteQueryStream;
        assert_eq!(
            incomplete.detail(),
            "query stream ended before its required terminal frame"
        );
        assert_eq!(incomplete.safe_details(), None);

        let too_large = ValaSdkError::ResultTooLarge;
        assert_eq!(too_large.detail(), "query result exceeds configured bounds");
        assert_eq!(too_large.safe_details(), None);
    }

    /// Every closed failed-terminal code projects through its catalog entry.
    ///
    /// # Panics
    ///
    /// Panics when any code is flattened to generic query execution metadata.
    #[test]
    fn failed_terminal_metadata_projection_is_exhaustive() {
        let cases = [
            (
                QueryTerminalErrorCode::QueryTimeout,
                BifrostError::QueryTimeout,
            ),
            (
                QueryTerminalErrorCode::QueryVisibilityUnavailable,
                BifrostError::QueryVisibilityUnavailable,
            ),
            (
                QueryTerminalErrorCode::QueryTenantInvariant,
                BifrostError::QueryTenantInvariant,
            ),
            (
                QueryTerminalErrorCode::QueryReconciliationInvariant,
                BifrostError::QueryReconciliationInvariant,
            ),
            (
                QueryTerminalErrorCode::QueryPeerSecurity,
                BifrostError::QueryPeerSecurity,
            ),
            (
                QueryTerminalErrorCode::QueryAuditUnavailable,
                BifrostError::QueryAuditUnavailable,
            ),
            (
                QueryTerminalErrorCode::CatalogUnreachable,
                BifrostError::CatalogUnreachable {
                    detail: "source failed".to_owned(),
                },
            ),
            (
                QueryTerminalErrorCode::StorageUnreachable,
                BifrostError::StorageUnreachable {
                    detail: "source failed".to_owned(),
                },
            ),
            (
                QueryTerminalErrorCode::QueryExecutionFailed,
                BifrostError::QueryExecutionFailed,
            ),
        ];
        for (code, expected) in cases {
            let mut terminal = failed_terminal(0);
            let error = terminal.error.as_mut().expect("failed terminal has error");
            error.code = code;
            error.detail =
                Some(QueryErrorDetail::new("source failed").expect("detail is scrubbed"));
            let projected = ValaSdkError::FailedTerminal { terminal };
            assert_eq!(projected.code(), expected.code(), "code for {code:?}");
            assert_eq!(projected.status(), expected.status(), "status for {code:?}");
            assert_eq!(projected.title(), expected.title(), "title for {code:?}");
            assert_eq!(
                projected.remediation(),
                expected.remediation(),
                "remediation for {code:?}"
            );
        }
    }

    /// Builds the schema used by client state-machine tests.
    fn test_schema() -> SchemaRef {
        Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]))
    }

    /// Test-owned encoder that emits the same split IPC stream the server emits.
    ///
    /// One writer opens the stream, each batch contributes only its own write
    /// delta, and [`TestQueryIpc::close`] yields the end-of-stream delta the
    /// terminal frame carries. Fixtures built this way exercise the stateful
    /// decoder exactly as a real query stream does.
    struct TestQueryIpc {
        /// Writer whose sink is drained once per emitted fragment.
        writer: StreamWriter<Vec<u8>>,
    }

    impl TestQueryIpc {
        /// Opens a stream over `schema` and returns its schema fragment.
        ///
        /// # Panics
        ///
        /// Panics when the test schema cannot start an Arrow IPC stream.
        fn open(schema: &SchemaRef) -> (Self, Vec<u8>) {
            let mut writer =
                StreamWriter::try_new(Vec::new(), schema.as_ref()).expect("schema writer starts");
            let prefix = std::mem::take(writer.get_mut());
            (Self { writer }, prefix)
        }

        /// Writes one `Int64` batch and returns only that write's fragment.
        ///
        /// # Panics
        ///
        /// Panics when the batch is invalid for `schema` or cannot be written.
        fn batch(&mut self, schema: &SchemaRef, values: &[i64]) -> Vec<u8> {
            let batch = RecordBatch::try_new(
                Arc::clone(schema),
                vec![Arc::new(Int64Array::from(values.to_vec()))],
            )
            .expect("test batch is valid");
            self.writer.write(&batch).expect("batch writes");
            std::mem::take(self.writer.get_mut())
        }

        /// Finishes the stream and returns its end-of-stream fragment.
        ///
        /// # Panics
        ///
        /// Panics when the writer cannot finish the stream.
        fn close(&mut self) -> Vec<u8> {
            self.writer.finish().expect("writer finishes");
            std::mem::take(self.writer.get_mut())
        }
    }

    /// Opens and immediately closes a stream, yielding its schema and EOS fragments.
    fn empty_ipc(schema: &SchemaRef) -> (Vec<u8>, Vec<u8>) {
        let (mut ipc, prefix) = TestQueryIpc::open(schema);
        let eos = ipc.close();
        (prefix, eos)
    }

    /// Builds the exact complete source set required by the visibility mode.
    fn sources(visibility: VisibilityMode) -> Vec<SourceCompletion> {
        let mut values = vec![
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
            values.push(SourceCompletion {
                source: QuerySource::LiveTail,
                outcome: SourceCompletionOutcome::Complete,
            });
        }
        values
    }

    /// Builds a successful terminal for the supplied visibility, rows, and EOS.
    fn success_terminal(
        visibility: VisibilityMode,
        rows: u64,
        arrow_ipc_eos: Vec<u8>,
    ) -> QueryTerminalFrame {
        QueryTerminalFrame {
            outcome: QueryTerminalOutcome::Success,
            freshness: QueryFreshness::Complete,
            execution_path: wyrd_spec::vala::api::QueryExecutionPath::Interactive,
            row_count: rows,
            warnings: Vec::new(),
            source_completion: sources(visibility),
            error: None,
            arrow_ipc_eos,
        }
    }

    /// Builds a degraded Fused terminal with the required warning and source state.
    fn degraded_terminal(rows: u64, arrow_ipc_eos: Vec<u8>) -> QueryTerminalFrame {
        QueryTerminalFrame {
            outcome: QueryTerminalOutcome::Degraded,
            freshness: QueryFreshness::Degraded,
            execution_path: wyrd_spec::vala::api::QueryExecutionPath::Interactive,
            row_count: rows,
            warnings: vec![QueryWarning::LiveTailUnavailable],
            source_completion: vec![
                SourceCompletion {
                    source: QuerySource::Iceberg,
                    outcome: SourceCompletionOutcome::Complete,
                },
                SourceCompletion {
                    source: QuerySource::HotSealed,
                    outcome: SourceCompletionOutcome::Complete,
                },
                SourceCompletion {
                    source: QuerySource::LiveTail,
                    outcome: SourceCompletionOutcome::Unavailable,
                },
            ],
            error: None,
            arrow_ipc_eos,
        }
    }

    /// Builds a failed terminal that still retains the immutable cut metadata.
    fn failed_terminal(rows: u64) -> QueryTerminalFrame {
        QueryTerminalFrame {
            outcome: QueryTerminalOutcome::Failed,
            freshness: QueryFreshness::Complete,
            execution_path: wyrd_spec::vala::api::QueryExecutionPath::Interactive,
            row_count: rows,
            warnings: Vec::new(),
            source_completion: sources(VisibilityMode::PublishedOnly),
            error: Some(QueryTerminalError {
                code: QueryTerminalErrorCode::QueryExecutionFailed,
                detail: None,
            }),
            // A failed stream never calls `finish`, so it has no end-of-stream.
            arrow_ipc_eos: Vec::new(),
        }
    }

    /// Converts one domain frame into its length-delimited public protobuf bytes.
    fn encoded(frame: QueryStreamFrame) -> Vec<u8> {
        FrameEncoder::encode(&proto::QueryStreamFrame::from(frame)).expect("frame encodes")
    }

    /// Builds a result stream over arbitrary already-encoded response chunks.
    fn result_stream(chunks: Vec<Vec<u8>>, visibility: VisibilityMode) -> QueryResultStream {
        let body = stream::iter(chunks.into_iter().map(|chunk| Ok(Bytes::from(chunk))));
        QueryResultStream::new(
            RawQueryStream::new(body, visibility),
            RequestId::now_v7(),
            offline_client(),
            0,
        )
    }

    /// Builds a query client whose lifecycle routes are guaranteed unreachable.
    ///
    /// Tests that only exercise decoding never settle against a server, so the
    /// client they carry must be inert rather than absent: the stream's own
    /// contract is that it always holds one.
    fn offline_client() -> QueryClient {
        client_for("http://127.0.0.1:1")
    }

    /// Builds a query client bound to one base URL with a static credential.
    fn client_for(base_url: &str) -> QueryClient {
        let config = wyrd_client::config::ClientConfig {
            api_key: Some(secrecy::SecretString::from("test-key")),
            http: wyrd_client::transport::config::HttpConfig {
                base_url: base_url.to_owned(),
                timeout_ms: 2_000,
                ..wyrd_client::transport::config::HttpConfig::default()
            },
            ..wyrd_client::config::ClientConfig::default()
        };
        QueryClient::new(&WyrdClient::with_config(config).expect("static config builds a client"))
    }

    /// The converter rejects a second schema before any terminal can be accepted.
    #[test]
    fn bifrost_query_rejects_duplicate_schema() {
        let (prefix, _eos) = empty_ipc(&test_schema());
        let schema = QueryStreamFrame::Schema(QuerySchemaFrame {
            schema_fingerprint: "test".to_owned(),
            arrow_ipc_schema: prefix,
        });
        let proto = proto::QueryStreamFrame::from(schema);
        let mut converter = QueryStreamConverter::new(VisibilityMode::PublishedOnly);
        converter
            .convert(proto.clone(), None)
            .expect("schema accepted");
        assert!(converter.convert(proto, None).is_err());
    }

    /// Every byte boundary is accepted by the incremental protobuf decoder.
    #[test]
    fn bifrost_query_frame_decoder_accepts_arbitrary_chunk_splits() {
        let (prefix, _eos) = empty_ipc(&test_schema());
        let frame = QueryStreamFrame::Schema(QuerySchemaFrame {
            schema_fingerprint: "test".to_owned(),
            arrow_ipc_schema: prefix,
        });
        let encoded = encoded(frame);
        let mut decoder = FrameDecoder::new(MAX_FRAME_BYTES);
        let mut decoded = Vec::new();
        for byte in encoded {
            decoded.extend(
                decoder
                    .push::<proto::QueryStreamFrame>(&[byte])
                    .expect("split frame decodes"),
            );
        }
        decoder.finish().expect("complete frame");
        assert_eq!(decoded.len(), 1);
    }

    /// EOF without a terminal is always incomplete, never successful empty output.
    #[tokio::test]
    async fn bifrost_query_eof_before_terminal_is_rejected() {
        let body = stream::empty::<Result<Bytes, reqwest::Error>>();
        let mut stream = RawQueryStream::new(body, VisibilityMode::PublishedOnly);
        assert!(matches!(
            stream.next_frame().await,
            Err(ValaSdkError::IncompleteQueryStream)
        ));
    }

    /// Schema, one batch, and terminal decode incrementally and retain metadata.
    #[tokio::test]
    async fn bifrost_query_decodes_arrow_and_terminal() {
        let schema = test_schema();
        let (mut ipc, prefix) = TestQueryIpc::open(&schema);
        let batch = ipc.batch(&schema, &[1, 2]);
        let eos = ipc.close();
        let chunks = vec![
            encoded(QueryStreamFrame::Schema(QuerySchemaFrame {
                schema_fingerprint: "fingerprint".to_owned(),
                arrow_ipc_schema: prefix,
            })),
            encoded(QueryStreamFrame::Batch(QueryBatchFrame {
                arrow_ipc_batch: batch,
            })),
            encoded(QueryStreamFrame::Terminal(success_terminal(
                VisibilityMode::PublishedOnly,
                2,
                eos,
            ))),
        ];
        let mut result = result_stream(chunks, VisibilityMode::PublishedOnly);
        let batch = result
            .next_batch()
            .await
            .expect("batch decodes")
            .expect("batch present");
        assert_eq!(batch.num_rows(), 2);
        assert!(
            result
                .next_batch()
                .await
                .expect("terminal validates")
                .is_none()
        );
        assert_eq!(result.terminal().expect("terminal retained").row_count, 2);
    }

    /// Degraded is a successful completion with explicit retained terminal state.
    #[tokio::test]
    async fn bifrost_query_degraded_terminal_completes() {
        let (prefix, eos) = empty_ipc(&test_schema());
        let chunks = vec![
            encoded(QueryStreamFrame::Schema(QuerySchemaFrame {
                schema_fingerprint: "fingerprint".to_owned(),
                arrow_ipc_schema: prefix,
            })),
            encoded(QueryStreamFrame::Terminal(degraded_terminal(0, eos))),
        ];
        let mut result = result_stream(chunks, VisibilityMode::Fused);
        assert!(
            result
                .next_batch()
                .await
                .expect("degraded is success")
                .is_none()
        );
        assert_eq!(
            result.terminal().expect("terminal retained").outcome,
            QueryTerminalOutcome::Degraded
        );
    }

    /// Bounded zero-row collection retains the authoritative schema without a batch.
    #[tokio::test]
    async fn bifrost_query_zero_rows_retains_authoritative_schema() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("optional_value", DataType::Utf8, true),
        ]));
        let (prefix, eos) = empty_ipc(&schema);
        let chunks = vec![
            encoded(QueryStreamFrame::Schema(QuerySchemaFrame {
                schema_fingerprint: "zero-row-fingerprint".to_owned(),
                arrow_ipc_schema: prefix,
            })),
            encoded(QueryStreamFrame::Terminal(success_terminal(
                VisibilityMode::PublishedOnly,
                0,
                eos,
            ))),
        ];

        let result = result_stream(chunks, VisibilityMode::PublishedOnly)
            .collect_bounded(CollectedQueryLimits {
                max_rows: 1,
                max_encoded_bytes: usize::MAX,
            })
            .await
            .expect("schema and zero-row terminal collect");

        assert!(result.batches.is_empty());
        assert_eq!(result.rows, 0);
        assert_eq!(result.terminal.row_count, 0);
        assert_eq!(result.schema.fields(), schema.fields());
        assert_eq!(result.schema.field(0).name(), "id");
        assert_eq!(result.schema.field(0).data_type(), &DataType::Int64);
        assert!(!result.schema.field(0).is_nullable());
        assert_eq!(result.schema.field(1).name(), "optional_value");
        assert_eq!(result.schema.field(1).data_type(), &DataType::Utf8);
        assert!(result.schema.field(1).is_nullable());
    }

    /// A successful terminal without the required schema never becomes a result.
    #[tokio::test]
    async fn bifrost_query_collection_rejects_missing_schema() {
        let (_prefix, eos) = empty_ipc(&test_schema());
        let chunks = vec![encoded(QueryStreamFrame::Terminal(success_terminal(
            VisibilityMode::PublishedOnly,
            0,
            eos,
        )))];

        assert!(
            result_stream(chunks, VisibilityMode::PublishedOnly)
                .collect_bounded(CollectedQueryLimits {
                    max_rows: 1,
                    max_encoded_bytes: usize::MAX,
                })
                .await
                .is_err()
        );
    }

    /// A failed terminal errors after retaining exact diagnostic metadata.
    #[tokio::test]
    async fn bifrost_query_failed_terminal_is_retained_and_rejected() {
        let (prefix, _eos) = empty_ipc(&test_schema());
        let chunks = vec![
            encoded(QueryStreamFrame::Schema(QuerySchemaFrame {
                schema_fingerprint: "fingerprint".to_owned(),
                arrow_ipc_schema: prefix,
            })),
            encoded(QueryStreamFrame::Terminal(failed_terminal(0))),
        ];
        let mut result = result_stream(chunks, VisibilityMode::PublishedOnly);
        let error = result
            .next_batch()
            .await
            .expect_err("failed terminal rejects");
        assert!(matches!(error, ValaSdkError::FailedTerminal { .. }));
        assert_eq!(
            result.terminal().expect("failed terminal retained").outcome,
            QueryTerminalOutcome::Failed
        );
    }

    /// A standalone per-batch stream is refused; there is no compatibility mode.
    ///
    /// The public protocol is one split IPC stream, so a batch frame carries a
    /// bare write delta. A legacy frame that re-encodes its own schema message
    /// is a second schema on the same stream and must be rejected rather than
    /// silently reinterpreted under the initial schema.
    #[tokio::test]
    async fn bifrost_query_rejects_standalone_batch_stream() {
        let initial = test_schema();
        let other = Arc::new(Schema::new(vec![Field::new(
            "other",
            DataType::Int64,
            false,
        )]));
        let (_initial_ipc, prefix) = TestQueryIpc::open(&initial);
        let mut standalone = Vec::new();
        let mut writer = StreamWriter::try_new(&mut standalone, other.as_ref())
            .expect("standalone writer starts");
        writer
            .write(
                &RecordBatch::try_new(
                    Arc::clone(&other),
                    vec![Arc::new(Int64Array::from(vec![1]))],
                )
                .expect("standalone batch is valid"),
            )
            .expect("standalone batch writes");
        writer.finish().expect("standalone writer finishes");
        drop(writer);
        let chunks = vec![
            encoded(QueryStreamFrame::Schema(QuerySchemaFrame {
                schema_fingerprint: "fingerprint".to_owned(),
                arrow_ipc_schema: prefix,
            })),
            encoded(QueryStreamFrame::Batch(QueryBatchFrame {
                arrow_ipc_batch: standalone,
            })),
        ];
        let mut result = result_stream(chunks, VisibilityMode::PublishedOnly);
        assert!(matches!(
            result.next_batch().await,
            Err(ValaSdkError::Arrow(_))
        ));
    }

    /// The client decodes one split IPC stream per query with an explicit close.
    ///
    /// This is the client half of the stateful protocol: one schema fragment,
    /// one bare delta per batch, and one end-of-stream delta carried by the
    /// terminal. It proves ordering, closure, per-fragment bounded retention,
    /// and that the fragments are strictly smaller than standalone re-encodes.
    #[tokio::test]
    async fn stateful_ipc_decoder_contract() {
        let schema = test_schema();
        let (mut ipc, prefix) = TestQueryIpc::open(&schema);
        let batches = [
            ipc.batch(&schema, &[1, 2]),
            ipc.batch(&schema, &[3]),
            ipc.batch(&schema, &[4, 5, 6]),
        ];
        let eos = ipc.close();

        let mut standalone = Vec::new();
        let mut writer =
            StreamWriter::try_new(&mut standalone, schema.as_ref()).expect("standalone starts");
        writer
            .write(
                &RecordBatch::try_new(
                    Arc::clone(&schema),
                    vec![Arc::new(Int64Array::from(vec![1, 2]))],
                )
                .expect("standalone batch is valid"),
            )
            .expect("standalone batch writes");
        writer.finish().expect("standalone finishes");
        drop(writer);
        assert!(
            batches[0].len() < standalone.len(),
            "a continuation fragment must be smaller than a standalone stream"
        );

        let mut chunks = vec![encoded(QueryStreamFrame::Schema(QuerySchemaFrame {
            schema_fingerprint: "fingerprint".to_owned(),
            arrow_ipc_schema: prefix,
        }))];
        chunks.extend(batches.iter().map(|fragment| {
            encoded(QueryStreamFrame::Batch(QueryBatchFrame {
                arrow_ipc_batch: fragment.clone(),
            }))
        }));
        chunks.push(encoded(QueryStreamFrame::Terminal(success_terminal(
            VisibilityMode::PublishedOnly,
            6,
            eos.clone(),
        ))));

        let mut result = result_stream(chunks, VisibilityMode::PublishedOnly);
        let mut rows = Vec::new();
        while let Some(batch) = result.next_batch().await.expect("split stream decodes") {
            assert_eq!(batch.schema().as_ref(), schema.as_ref());
            let column = batch
                .column(0)
                .as_any()
                .downcast_ref::<Int64Array>()
                .expect("int64 column");
            rows.extend(column.values().iter().copied());
        }
        assert_eq!(rows, vec![1, 2, 3, 4, 5, 6]);
        assert_eq!(result.terminal().expect("terminal retained").row_count, 6);
        assert!(
            result.raw.arrow_ipc_closed(),
            "terminal EOS closes the stream"
        );
        let largest = batches
            .iter()
            .map(Vec::len)
            .max()
            .expect("fixture has batches");
        assert!(
            result.raw.peak_pending_frame_bytes() <= largest,
            "the decoder retains at most one fragment at a time"
        );

        let (prefix, _unused) = empty_ipc(&schema);
        let missing_eos = vec![
            encoded(QueryStreamFrame::Schema(QuerySchemaFrame {
                schema_fingerprint: "fingerprint".to_owned(),
                arrow_ipc_schema: prefix,
            })),
            encoded(QueryStreamFrame::Terminal(QueryTerminalFrame {
                arrow_ipc_eos: Vec::new(),
                ..success_terminal(VisibilityMode::PublishedOnly, 0, eos.clone())
            })),
        ];
        assert!(matches!(
            result_stream(missing_eos, VisibilityMode::PublishedOnly)
                .next_batch()
                .await,
            Err(ValaSdkError::Protocol(_)),
        ));

        let orphan_batch = vec![encoded(QueryStreamFrame::Batch(QueryBatchFrame {
            arrow_ipc_batch: batches[0].clone(),
        }))];
        assert!(matches!(
            result_stream(orphan_batch, VisibilityMode::PublishedOnly)
                .next_batch()
                .await,
            Err(ValaSdkError::Protocol(_))
        ));

        let (prefix, closing) = empty_ipc(&schema);
        let mut decoder = QueryIpcDecoder::new();
        decoder.accept_schema(&prefix).expect("schema accepted");
        assert!(
            decoder.accept_schema(&prefix).is_err(),
            "a stream carries exactly one schema"
        );
        decoder
            .accept_eos(&closing)
            .expect("end-of-stream accepted");
        assert!(decoder.eos_accepted());
        assert!(
            decoder.accept_batch(&batches[0]).is_err(),
            "no fragment may follow the end-of-stream"
        );
        assert!(
            decoder.accept_eos(&closing).is_err(),
            "a stream closes exactly once"
        );
    }

    /// Row and encoded-byte bounds reject the whole collection without truncation.
    #[tokio::test]
    async fn bifrost_query_bounded_collection_rejects_rows_and_bytes() {
        let schema = test_schema();
        let (mut ipc, prefix) = TestQueryIpc::open(&schema);
        let batch = ipc.batch(&schema, &[1, 2]);
        let eos = ipc.close();
        let chunks = vec![
            encoded(QueryStreamFrame::Schema(QuerySchemaFrame {
                schema_fingerprint: "fingerprint".to_owned(),
                arrow_ipc_schema: prefix,
            })),
            encoded(QueryStreamFrame::Batch(QueryBatchFrame {
                arrow_ipc_batch: batch.clone(),
            })),
            encoded(QueryStreamFrame::Terminal(success_terminal(
                VisibilityMode::PublishedOnly,
                2,
                eos,
            ))),
        ];
        let rows = result_stream(chunks.clone(), VisibilityMode::PublishedOnly)
            .collect_bounded(CollectedQueryLimits {
                max_rows: 1,
                max_encoded_bytes: usize::MAX,
            })
            .await;
        assert!(matches!(rows, Err(ValaSdkError::ResultTooLarge)));

        let bytes = result_stream(chunks, VisibilityMode::PublishedOnly)
            .collect_bounded(CollectedQueryLimits {
                max_rows: usize::MAX,
                max_encoded_bytes: batch.len().saturating_sub(1),
            })
            .await;
        assert!(matches!(bytes, Err(ValaSdkError::ResultTooLarge)));
    }

    /// Schema and framing bytes count toward the ceiling even with no result batches.
    #[tokio::test]
    async fn bifrost_query_bounded_collection_counts_schema_and_framing_bytes() {
        let fields = (0..128)
            .map(|index| {
                Field::new(
                    format!("wide_schema_field_{index:03}_with_descriptive_name"),
                    DataType::Int64,
                    true,
                )
            })
            .collect::<Vec<_>>();
        let schema = Arc::new(Schema::new(fields));
        let (prefix, eos) = empty_ipc(&schema);
        let schema_frame = encoded(QueryStreamFrame::Schema(QuerySchemaFrame {
            schema_fingerprint: "wide-schema".to_owned(),
            arrow_ipc_schema: prefix,
        }));
        let terminal_frame = encoded(QueryStreamFrame::Terminal(success_terminal(
            VisibilityMode::PublishedOnly,
            0,
            eos,
        )));
        let encoded_bytes = schema_frame
            .len()
            .checked_add(terminal_frame.len())
            .expect("test response byte count fits usize");
        let chunks = vec![schema_frame, terminal_frame];

        let rejected = result_stream(chunks.clone(), VisibilityMode::PublishedOnly)
            .collect_bounded(CollectedQueryLimits {
                max_rows: 0,
                max_encoded_bytes: encoded_bytes.saturating_sub(1),
            })
            .await;
        assert!(matches!(rejected, Err(ValaSdkError::ResultTooLarge)));

        let accepted = result_stream(chunks, VisibilityMode::PublishedOnly)
            .collect_bounded(CollectedQueryLimits {
                max_rows: 0,
                max_encoded_bytes: encoded_bytes,
            })
            .await
            .expect("exact response-byte ceiling is accepted");
        assert_eq!(accepted.encoded_bytes, encoded_bytes);
        assert!(accepted.batches.is_empty());
    }

    /// Pending response-body stream whose drop flag proves cancellation ownership.
    struct DropTrackedStream {
        /// Flag set when the response stream is dropped.
        dropped: Arc<AtomicBool>,
    }

    impl Stream for DropTrackedStream {
        type Item = Result<Bytes, reqwest::Error>;

        /// Remains pending so cancellation is driven only by dropping the owner.
        fn poll_next(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            Poll::Pending
        }
    }

    impl Drop for DropTrackedStream {
        /// Records that the raw query released its response body.
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::SeqCst);
        }
    }

    /// Dropping a raw stream releases the underlying body and propagates cancellation.
    #[test]
    fn bifrost_query_drop_releases_response_stream() {
        let dropped = Arc::new(AtomicBool::new(false));
        let raw = RawQueryStream::new(
            DropTrackedStream {
                dropped: Arc::clone(&dropped),
            },
            VisibilityMode::PublishedOnly,
        );
        drop(raw);
        assert!(dropped.load(Ordering::SeqCst));
    }

    /// The Rust client exposes one request ID and canonical typed lifecycle methods.
    #[test]
    fn running_query_client_projects_canonical_contract() {
        let request_id = RequestId::now_v7();
        let body = stream::pending::<Result<Bytes, reqwest::Error>>();
        let result = QueryResultStream::new(
            RawQueryStream::new(body, VisibilityMode::PublishedOnly),
            request_id.clone(),
            offline_client(),
            0,
        );
        assert_eq!(result.request_id(), &request_id);

        let source = include_str!("query.rs");
        assert!(source.contains("pub async fn running(&self)"));
        assert!(source.contains("pub async fn status("));
        assert!(source.contains("pub async fn cancel("));
        assert!(source.contains("/v1/query/running"));
        assert!(source.contains("/v1/query/{request_id}"));
    }
}
