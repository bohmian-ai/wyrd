//! Terminal-safe Oracle query client and bounded result collection.

use std::collections::VecDeque;
use std::io::Cursor;
use std::pin::Pin;

use arrow::datatypes::SchemaRef;
use arrow::ipc::reader::StreamReader;
use arrow::record_batch::RecordBatch;
use futures_util::{Stream, StreamExt};
use wyrd_client::WyrdClient;
use wyrd_spec::vala::api::{
    BifrostQueryRequest, QueryStreamFrame, QueryTerminalFrame, QueryTerminalOutcome,
};
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
            Self::FailedTerminal { .. } => "WYRD_VALA_500_QUERY_EXECUTION_FAILED",
            Self::ResultTooLarge => "WYRD_VALA_413_QUERY_RESULT_TOO_LARGE",
        }
    }

    /// Returns the HTTP-equivalent status projected into language SDK errors.
    #[must_use]
    pub fn status(&self) -> u16 {
        match self {
            Self::Transport(error) => error.status(),
            Self::Protocol(_) | Self::Arrow(_) | Self::IncompleteQueryStream => 502,
            Self::FailedTerminal { .. } => 500,
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
            Self::FailedTerminal { .. } => "Query execution failed",
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
            Self::FailedTerminal { .. } => {
                "Inspect the retained terminal error and correct the query or source failure before retrying."
            }
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
        let response = self
            .client
            .request_json_stream(reqwest::Method::POST, "/v1/query", request)
            .await?;
        Ok(QueryResultStream::new(RawQueryStream::new(
            response.bytes_stream(),
            request.visibility,
        )))
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
    /// Complete frames decoded from the most recent body chunk.
    pending: VecDeque<QueryStreamFrame>,
    /// Shared wire-to-domain ordering and terminal validator.
    converter: QueryStreamConverter,
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
        loop {
            if let Some(frame) = self.pending.pop_front() {
                if let QueryStreamFrame::Terminal(terminal) = &frame {
                    self.terminal = Some(terminal.clone());
                }
                return Ok(Some(frame));
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
                        let batch_rows = match frame.frame.as_ref() {
                            Some(wyrd_tonic::wyrd::v1::query_stream_frame::Frame::Batch(batch)) => {
                                Some(decode_batch_rows(&batch.arrow_ipc_batch)?)
                            }
                            Some(
                                wyrd_tonic::wyrd::v1::query_stream_frame::Frame::Schema(_)
                                | wyrd_tonic::wyrd::v1::query_stream_frame::Frame::Terminal(_),
                            )
                            | None => None,
                        };
                        let frame = self
                            .converter
                            .convert(frame, batch_rows)
                            .map_err(|error| ValaSdkError::Protocol(error.to_string()))?;
                        self.pending.push_back(frame);
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
    });
    tracing::error!(
        error = ?error,
        transport = "http",
        phase = "response_body",
        timeout = error.is_timeout(),
        connect = error.is_connect(),
        body = error.is_body(),
        "Oracle query response body failed"
    );
    ValaSdkError::Transport(WyrdError::UpstreamFailure {
        message: "Oracle query response body failed".to_owned(),
        details,
    })
}

/// Arrow-projecting query stream that preserves terminal metadata.
pub struct QueryResultStream {
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
    fn new(raw: RawQueryStream) -> Self {
        Self {
            raw,
            schema: None,
            terminal: None,
            emitted_rows: 0,
            encoded_bytes: 0,
        }
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
            let frame = self.raw.next_frame().await;
            self.encoded_bytes = self.raw.received_bytes();
            let Some(frame) = frame? else {
                return Ok(None);
            };
            match frame {
                QueryStreamFrame::Schema(schema) => {
                    self.schema = Some(decode_schema(&schema.arrow_ipc_schema)?);
                }
                QueryStreamFrame::Batch(batch) => {
                    let decoded = decode_one_batch(&batch.arrow_ipc_batch)?;
                    let expected = self.schema.as_ref().ok_or_else(|| {
                        ValaSdkError::Protocol("batch arrived before schema".to_owned())
                    })?;
                    if decoded.schema().as_ref() != expected.as_ref() {
                        return Err(ValaSdkError::Arrow(
                            "batch schema does not match the initial schema".to_owned(),
                        ));
                    }
                    self.emitted_rows = self
                        .emitted_rows
                        .checked_add(u64::try_from(decoded.num_rows()).map_err(|_| {
                            ValaSdkError::Protocol("row count does not fit u64".to_owned())
                        })?)
                        .ok_or_else(|| ValaSdkError::Protocol("row count overflow".to_owned()))?;
                    return Ok(Some(decoded));
                }
                QueryStreamFrame::Terminal(terminal) => {
                    terminal
                        .validate_emitted_rows(self.emitted_rows)
                        .map_err(|error| ValaSdkError::Protocol(error.to_string()))?;
                    self.terminal = Some(terminal.clone());
                    if terminal.outcome == QueryTerminalOutcome::Failed {
                        return Err(ValaSdkError::FailedTerminal { terminal });
                    }
                    return Ok(None);
                }
            }
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
            let batch = self.next_batch().await?;
            if self.encoded_bytes > limits.max_encoded_bytes {
                return Err(ValaSdkError::ResultTooLarge);
            }
            let Some(batch) = batch else {
                break;
            };
            let next_rows = rows
                .checked_add(batch.num_rows())
                .ok_or(ValaSdkError::ResultTooLarge)?;
            if next_rows > limits.max_rows {
                return Err(ValaSdkError::ResultTooLarge);
            }
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

/// Decodes and counts every record batch in one Arrow IPC batch frame.
///
/// # Errors
///
/// Returns [`ValaSdkError::Arrow`] when the IPC payload is malformed or its row
/// count cannot fit the public `u64` terminal counter.
fn decode_batch_rows(bytes: &[u8]) -> Result<u64, ValaSdkError> {
    let mut reader = StreamReader::try_new(Cursor::new(bytes), None)
        .map_err(|error| ValaSdkError::Arrow(error.to_string()))?;
    reader.try_fold(0_u64, |rows, batch| {
        let batch = batch.map_err(|error| ValaSdkError::Arrow(error.to_string()))?;
        rows.checked_add(
            u64::try_from(batch.num_rows())
                .map_err(|_| ValaSdkError::Protocol("row count does not fit u64".to_owned()))?,
        )
        .ok_or_else(|| ValaSdkError::Protocol("row count overflow".to_owned()))
    })
}

/// Decodes the schema-only Arrow IPC stream from the initial frame.
///
/// # Errors
///
/// Returns [`ValaSdkError::Arrow`] when the schema stream is malformed or
/// unexpectedly contains a record batch.
fn decode_schema(bytes: &[u8]) -> Result<SchemaRef, ValaSdkError> {
    let mut reader = StreamReader::try_new(Cursor::new(bytes), None)
        .map_err(|error| ValaSdkError::Arrow(error.to_string()))?;
    let schema = reader.schema();
    if reader
        .next()
        .transpose()
        .map_err(|error| ValaSdkError::Arrow(error.to_string()))?
        .is_some()
    {
        return Err(ValaSdkError::Arrow(
            "schema frame unexpectedly contains a record batch".to_owned(),
        ));
    }
    Ok(schema)
}

/// Decodes exactly one record batch from one Arrow IPC batch frame.
///
/// # Errors
///
/// Returns [`ValaSdkError::Arrow`] when the payload is malformed, empty, or
/// contains more than one record batch.
fn decode_one_batch(bytes: &[u8]) -> Result<RecordBatch, ValaSdkError> {
    let mut reader = StreamReader::try_new(Cursor::new(bytes), None)
        .map_err(|error| ValaSdkError::Arrow(error.to_string()))?;
    let batch = reader
        .next()
        .transpose()
        .map_err(|error| ValaSdkError::Arrow(error.to_string()))?
        .ok_or_else(|| ValaSdkError::Arrow("batch frame contains no record batch".to_owned()))?;
    if reader
        .next()
        .transpose()
        .map_err(|error| ValaSdkError::Arrow(error.to_string()))?
        .is_some()
    {
        return Err(ValaSdkError::Arrow(
            "batch frame contains more than one record batch".to_owned(),
        ));
    }
    Ok(batch)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
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
        let mut stream = RawQueryStream::new(
            response.bytes_stream(),
            VisibilityMode::PublishedOnly,
        );

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
        assert_eq!(details["body"], true);
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

    /// Builds the schema used by client state-machine tests.
    fn test_schema() -> SchemaRef {
        Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]))
    }

    /// Encodes a schema-only Arrow IPC stream.
    fn schema_ipc(schema: &SchemaRef) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut writer =
            StreamWriter::try_new(&mut bytes, schema.as_ref()).expect("schema writer starts");
        writer.finish().expect("schema writer finishes");
        bytes
    }

    /// Encodes one Arrow record batch as a complete IPC stream.
    fn batch_ipc(schema: &SchemaRef, values: &[i64]) -> Vec<u8> {
        let batch = RecordBatch::try_new(
            Arc::clone(schema),
            vec![Arc::new(Int64Array::from(values.to_vec()))],
        )
        .expect("test batch is valid");
        let mut bytes = Vec::new();
        let mut writer =
            StreamWriter::try_new(&mut bytes, schema.as_ref()).expect("batch writer starts");
        writer.write(&batch).expect("batch writes");
        writer.finish().expect("batch writer finishes");
        bytes
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

    /// Builds a successful terminal for the supplied visibility and row count.
    fn success_terminal(visibility: VisibilityMode, rows: u64) -> QueryTerminalFrame {
        QueryTerminalFrame {
            outcome: QueryTerminalOutcome::Success,
            freshness: QueryFreshness::Complete,
            row_count: rows,
            warnings: Vec::new(),
            source_completion: sources(visibility),
            error: None,
        }
    }

    /// Builds a degraded Fused terminal with the required warning and source state.
    fn degraded_terminal(rows: u64) -> QueryTerminalFrame {
        QueryTerminalFrame {
            outcome: QueryTerminalOutcome::Degraded,
            freshness: QueryFreshness::Degraded,
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
        }
    }

    /// Builds a failed terminal that still retains the immutable cut metadata.
    fn failed_terminal(rows: u64) -> QueryTerminalFrame {
        QueryTerminalFrame {
            outcome: QueryTerminalOutcome::Failed,
            freshness: QueryFreshness::Complete,
            row_count: rows,
            warnings: Vec::new(),
            source_completion: sources(VisibilityMode::PublishedOnly),
            error: Some(QueryTerminalError {
                code: QueryTerminalErrorCode::QueryExecutionFailed,
                detail: None,
            }),
        }
    }

    /// Converts one domain frame into its length-delimited public protobuf bytes.
    fn encoded(frame: QueryStreamFrame) -> Vec<u8> {
        FrameEncoder::encode(&proto::QueryStreamFrame::from(frame)).expect("frame encodes")
    }

    /// Builds a result stream over arbitrary already-encoded response chunks.
    fn result_stream(chunks: Vec<Vec<u8>>, visibility: VisibilityMode) -> QueryResultStream {
        let body = stream::iter(chunks.into_iter().map(|chunk| Ok(Bytes::from(chunk))));
        QueryResultStream::new(RawQueryStream::new(body, visibility))
    }

    /// The converter rejects a second schema before any terminal can be accepted.
    #[test]
    fn bifrost_query_rejects_duplicate_schema() {
        let schema = QueryStreamFrame::Schema(QuerySchemaFrame {
            schema_fingerprint: "test".to_owned(),
            arrow_ipc_schema: schema_ipc(&test_schema()),
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
        let frame = QueryStreamFrame::Schema(QuerySchemaFrame {
            schema_fingerprint: "test".to_owned(),
            arrow_ipc_schema: schema_ipc(&test_schema()),
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
        let chunks = vec![
            encoded(QueryStreamFrame::Schema(QuerySchemaFrame {
                schema_fingerprint: "fingerprint".to_owned(),
                arrow_ipc_schema: schema_ipc(&schema),
            })),
            encoded(QueryStreamFrame::Batch(QueryBatchFrame {
                arrow_ipc_batch: batch_ipc(&schema, &[1, 2]),
            })),
            encoded(QueryStreamFrame::Terminal(success_terminal(
                VisibilityMode::PublishedOnly,
                2,
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
        let schema = test_schema();
        let chunks = vec![
            encoded(QueryStreamFrame::Schema(QuerySchemaFrame {
                schema_fingerprint: "fingerprint".to_owned(),
                arrow_ipc_schema: schema_ipc(&schema),
            })),
            encoded(QueryStreamFrame::Terminal(degraded_terminal(0))),
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
        let chunks = vec![
            encoded(QueryStreamFrame::Schema(QuerySchemaFrame {
                schema_fingerprint: "zero-row-fingerprint".to_owned(),
                arrow_ipc_schema: schema_ipc(&schema),
            })),
            encoded(QueryStreamFrame::Terminal(success_terminal(
                VisibilityMode::PublishedOnly,
                0,
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
        let chunks = vec![encoded(QueryStreamFrame::Terminal(success_terminal(
            VisibilityMode::PublishedOnly,
            0,
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
        let schema = test_schema();
        let chunks = vec![
            encoded(QueryStreamFrame::Schema(QuerySchemaFrame {
                schema_fingerprint: "fingerprint".to_owned(),
                arrow_ipc_schema: schema_ipc(&schema),
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

    /// A batch encoded with a different schema cannot cross the initial schema frame.
    #[tokio::test]
    async fn bifrost_query_rejects_batch_schema_mismatch() {
        let initial = test_schema();
        let other = Arc::new(Schema::new(vec![Field::new(
            "other",
            DataType::Int64,
            false,
        )]));
        let chunks = vec![
            encoded(QueryStreamFrame::Schema(QuerySchemaFrame {
                schema_fingerprint: "fingerprint".to_owned(),
                arrow_ipc_schema: schema_ipc(&initial),
            })),
            encoded(QueryStreamFrame::Batch(QueryBatchFrame {
                arrow_ipc_batch: batch_ipc(&other, &[1]),
            })),
        ];
        let mut result = result_stream(chunks, VisibilityMode::PublishedOnly);
        assert!(matches!(
            result.next_batch().await,
            Err(ValaSdkError::Arrow(_))
        ));
    }

    /// Row and encoded-byte bounds reject the whole collection without truncation.
    #[tokio::test]
    async fn bifrost_query_bounded_collection_rejects_rows_and_bytes() {
        let schema = test_schema();
        let batch = batch_ipc(&schema, &[1, 2]);
        let chunks = vec![
            encoded(QueryStreamFrame::Schema(QuerySchemaFrame {
                schema_fingerprint: "fingerprint".to_owned(),
                arrow_ipc_schema: schema_ipc(&schema),
            })),
            encoded(QueryStreamFrame::Batch(QueryBatchFrame {
                arrow_ipc_batch: batch.clone(),
            })),
            encoded(QueryStreamFrame::Terminal(success_terminal(
                VisibilityMode::PublishedOnly,
                2,
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
        let schema_frame = encoded(QueryStreamFrame::Schema(QuerySchemaFrame {
            schema_fingerprint: "wide-schema".to_owned(),
            arrow_ipc_schema: schema_ipc(&schema),
        }));
        let terminal_frame = encoded(QueryStreamFrame::Terminal(success_terminal(
            VisibilityMode::PublishedOnly,
            0,
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
}
