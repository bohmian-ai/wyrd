//! Bounded complete-result Bifrost queries for agent tools.
//!
//! [`BoundedQuery`] serves `bifrost.query` for both the MCP tool and the
//! Workflow Agent tool. It parses nothing protocol-specific: it takes closed
//! [`QueryArguments`], runs them through [`super::service::stream_query`]
//! as the verified caller, and collects the stream into one
//! `{columns, rows, terminal}` value under the caller's row and byte
//! ceilings. Authorization, tenancy, audit, SQL admission, execution,
//! cancellation, and terminal settlement stay with the query service and the
//! Oracle controls it calls; each surface keeps only its own argument parsing
//! and error projection.

use std::sync::Arc;

use arrow::json::writer::{EncoderOptions, make_encoder};
use arrow::record_batch::RecordBatch;
use futures_util::StreamExt as _;
use serde::Deserialize;
use serde_json::Value as JsonValue;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::oracle::{OracleQueryStream, QueryIpcDecoder};
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::BifrostError as ValaError;
use wyrd_spec::vala::api::BifrostQueryRequest;
use wyrd_spec::vala::api::{QueryStreamFrame, QueryTerminalFrame, QueryTerminalOutcome};

use crate::components::auth::Caller;
use crate::state::AppState;

/// Wire name of the bounded read-only query tool on every agent surface.
pub(crate) const QUERY: &str = "bifrost.query";

/// Largest SQL text `bifrost.query` accepts, in UTF-8 bytes.
///
/// This is an agent-tool input bound, not a Wyrd query contract: it exists so
/// a runaway agent prompt is refused before it reaches the SQL floor at all.
pub(crate) const MAX_SQL_BYTES: usize = 65_536;

/// Rows returned when a caller names no `max_rows`.
pub(crate) const DEFAULT_MAX_ROWS: u32 = 1_000;

/// Largest `max_rows` an agent caller may ask for.
///
/// Deliberately far below the server's own million-row sync ceiling: an MCP
/// result is one JSON value an agent has to hold in a context window, so the
/// binding constraint here is the reader, not the engine.
pub(crate) const MAX_ROWS_CEILING: u32 = 10_000;

/// Serialized result bytes returned when a caller names no `max_bytes`.
pub(crate) const DEFAULT_MAX_BYTES: usize = 4 * 1024 * 1024;

/// Largest `max_bytes` an agent caller may ask for, well under the server's 256 MiB.
pub(crate) const MAX_BYTES_CEILING: usize = 16 * 1024 * 1024;

/// Compact-JSON bytes the result object costs before any content is charged.
///
/// `{"columns":`, `,"rows":`, the row array's `[` and `]`, `,"terminal":`, and
/// the closing `}` are structural and always present, so they are charged once
/// up front and the columns, each row plus its separating comma, and the
/// terminal are charged as they are produced.
pub(crate) const STRUCTURED_OVERHEAD_BYTES: usize = 11 + 8 + 2 + 12 + 1;

/// JSON Schema of [`QueryArguments`] as every agent surface advertises it.
///
/// The schema is advice a client may ignore; [`QueryArguments::validate`]
/// enforces the same bounds.
pub(crate) fn input_schema() -> JsonValue {
    serde_json::json!({
        "type": "object",
        "properties": {
            "sql": {
                "type": "string",
                "description": "A single read-only SELECT statement.",
                "minLength": 1,
                "maxLength": MAX_SQL_BYTES
            },
            "deadline_ms": {
                "type": "integer",
                "minimum": 1,
                "maximum": u32::MAX,
                "description": "Optional caller deadline in milliseconds."
            },
            "max_rows": {
                "type": "integer",
                "minimum": 1,
                "maximum": MAX_ROWS_CEILING,
                "default": DEFAULT_MAX_ROWS,
                "description": "Row ceiling for the complete result. Exceeding it fails; results are never truncated."
            },
            "max_bytes": {
                "type": "integer",
                "minimum": 1,
                "maximum": MAX_BYTES_CEILING,
                "default": DEFAULT_MAX_BYTES,
                "description": "Serialized JSON byte ceiling for the complete result. Exceeding it fails; results are never truncated."
            }
        },
        "required": ["sql"],
        "additionalProperties": false
    })
}

/// Out-of-range [`QueryArguments`], with the safe details of the bound it
/// violates.
#[derive(Debug)]
pub(crate) struct InvalidQueryArguments {
    /// Fixed reason naming the offending field.
    pub(crate) reason: &'static str,
    /// The offending value and its bound.
    pub(crate) details: JsonValue,
}

/// Closed arguments of one bounded `bifrost.query` request.
///
/// Every field is either the SQL itself or the optional deadline the public
/// request already carries, plus the two response ceilings. There is no
/// path, class, tenant, or principal field to omit.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct QueryArguments {
    /// A single read-only SELECT statement.
    pub(crate) sql: String,
    /// Optional caller deadline in milliseconds.
    #[serde(default)]
    pub(crate) deadline_ms: Option<u32>,
    /// Row ceiling for the complete result.
    #[serde(default = "default_max_rows")]
    pub(crate) max_rows: u32,
    /// Serialized JSON byte ceiling for the complete result.
    #[serde(default = "default_max_bytes")]
    pub(crate) max_bytes: usize,
}

/// Default row ceiling for an omitted `max_rows`.
fn default_max_rows() -> u32 {
    DEFAULT_MAX_ROWS
}

/// Default byte ceiling for an omitted `max_bytes`.
fn default_max_bytes() -> usize {
    DEFAULT_MAX_BYTES
}

impl QueryArguments {
    /// Check the input bounds the JSON schema advertises.
    ///
    /// The advertised schema is advice a client may ignore, so the same bounds
    /// are enforced here. Everything about the SQL itself beyond its size —
    /// single statement, SELECT-only, allowed functions, known tables, tenant
    /// authorization — belongs to the server's SQL floor and query service and
    /// is deliberately not repeated.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidQueryArguments`] when the SQL is blank or oversized,
    /// the deadline is zero, or either ceiling is zero or above its maximum.
    pub(crate) fn validate(&self) -> Result<(), InvalidQueryArguments> {
        let invalid = |reason: &'static str, details: JsonValue| InvalidQueryArguments {
            reason,
            details,
        };
        if self.sql.trim().is_empty() {
            return Err(invalid("sql must not be blank", serde_json::json!({})));
        }
        if self.sql.len() > MAX_SQL_BYTES {
            return Err(invalid(
                "sql exceeds the input ceiling",
                serde_json::json!({ "bytes": self.sql.len(), "max_bytes": MAX_SQL_BYTES }),
            ));
        }
        if self.deadline_ms == Some(0) {
            return Err(invalid(
                "deadline_ms must be positive",
                serde_json::json!({ "deadline_ms": 0 }),
            ));
        }
        if self.max_rows == 0 || self.max_rows > MAX_ROWS_CEILING {
            return Err(invalid(
                "max_rows is outside the allowed range",
                serde_json::json!({ "max_rows": self.max_rows, "ceiling": MAX_ROWS_CEILING }),
            ));
        }
        if self.max_bytes == 0 || self.max_bytes > MAX_BYTES_CEILING {
            return Err(invalid(
                "max_bytes is outside the allowed range",
                serde_json::json!({ "max_bytes": self.max_bytes, "ceiling": MAX_BYTES_CEILING }),
            ));
        }
        Ok(())
    }

    /// Project the SQL and optional deadline onto the public query request.
    pub(crate) fn to_request(&self) -> BifrostQueryRequest {
        BifrostQueryRequest {
            sql: self.sql.clone(),
            deadline_ms: self.deadline_ms.map(i64::from),
        }
    }
}

/// One bounded complete-result query run as a verified caller.
///
/// Cloning is cheap; the owned state lets a query run on tracked server work
/// that outlives the request or step waiting for it.
#[derive(Clone)]
pub(crate) struct BoundedQuery {
    /// Server state whose query service and Oracle controls serve the query.
    state: AppState,
}

impl BoundedQuery {
    /// Bind a query runner to `state`.
    pub(crate) fn new(state: AppState) -> Self {
        Self { state }
    }

    /// Run `arguments` as `caller` and collect the complete result.
    ///
    /// Cancelling `cancel` signals the query immediately and then awaits the
    /// Oracle controls' settlement under the query's original deadline before
    /// returning, so a returned error never leaves an unsettled query behind.
    ///
    /// # Errors
    /// Returns `RunningQueryControlUnavailable` when this process has no query
    /// controls, every error of [`super::service::stream_query`], and the
    /// stream, protocol, execution, cancellation, and result-ceiling errors
    /// of the collection, with no partial rows.
    pub(crate) async fn run(
        &self,
        caller: Caller,
        arguments: &QueryArguments,
        cancel: &CancellationToken,
    ) -> Result<JsonValue, WyrdError> {
        let controls = self
            .state
            .bifrost
            .query_controls()
            .ok_or(ValaError::RunningQueryControlUnavailable)?
            .clone();
        let settlement = Some((controls, caller.data_tenant_id, caller.request_id.clone()));
        let stream =
            super::service::stream_query(self.state.clone(), caller, arguments.to_request(), None)
                .await?;
        let collector = ResultCollector {
            settlement,
            max_rows: usize::try_from(arguments.max_rows).unwrap_or(usize::MAX),
            max_bytes: arguments.max_bytes,
            bytes: STRUCTURED_OVERHEAD_BYTES,
            #[cfg(feature = "test-support")]
            stall: self.claim_schema_stall(&stream),
        };
        collector.collect(stream, cancel).await
    }

    /// Bind the armed test-only schema hold to this stream's resource probe.
    ///
    /// The cancellation journey waits for this hold before cancelling, so fast
    /// fixture execution cannot make cancellation evidence vacuous.
    #[cfg(feature = "test-support")]
    fn claim_schema_stall(
        &self,
        stream: &OracleQueryStream,
    ) -> Option<Arc<crate::state::QueryStreamStall>> {
        let controller = self.state.query_stream_fault.as_ref()?;
        if !matches!(
            controller.claim(),
            Some(crate::state::QueryStreamFault::StallAfterSchema)
        ) {
            return None;
        }
        let stall = controller.claim_stall()?;
        stall.bind_resource_probe(stream.resource_probe_for_test());
        Some(stall)
    }
}

/// Collects one Oracle stream into the single value `bifrost.query` returns.
///
/// The collector is the agent-tool counterpart of the typed route's own
/// bounded collector: same stateful IPC decoder, same exactly-one-terminal rule, same
/// active cancellation on every pre-terminal failure. What differs is only the
/// budget it charges and the shape it produces.
struct ResultCollector {
    /// Trusted cancellation routing and identity; absent only in pure decoder fixtures.
    settlement: Option<(
        crate::oracle::RunningQueryControls,
        wyrd_spec::DataTenantId,
        wyrd_spec::request_id::RequestId,
    )>,
    /// Row ceiling this caller asked for. Exceeding it fails; nothing truncates.
    max_rows: usize,
    /// Exact compact-JSON byte ceiling for `{columns, rows, terminal}`.
    max_bytes: usize,
    /// Exact bytes charged so far, starting at the fixed structural overhead.
    bytes: usize,
    /// Deterministic post-schema hold used only by cancellation journeys.
    #[cfg(feature = "test-support")]
    stall: Option<Arc<crate::state::QueryStreamStall>>,
}

impl ResultCollector {
    /// Collects the compact result, confirming owner settlement before any error return.
    ///
    /// # Errors
    /// Returns the consumer error after valid terminal proof; otherwise returns
    /// the shared control owner's incomplete, protocol or unavailable error.
    /// Cancellation signals immediately and retains the response under its original deadline.
    async fn collect(
        mut self,
        mut stream: OracleQueryStream,
        cancel: &CancellationToken,
    ) -> Result<JsonValue, WyrdError> {
        let mut terminal = None;
        match self.consume(&mut stream, cancel, &mut terminal).await {
            Ok(value) => Ok(value),
            Err(error) => {
                let (controls, tenant, request_id) = self
                    .settlement
                    .take()
                    .ok_or(ValaError::RunningQueryControlUnavailable)?;
                controls
                    .cancel_and_settle(tenant, request_id, stream, terminal)
                    .await?;
                Err(error)
            }
        }
    }

    /// Decodes and charges one result, retaining validated terminal evidence separately.
    ///
    /// # Errors
    /// Returns protocol, Arrow, execution or result-ceiling errors. Early errors
    /// signal cancellation synchronously; `collect` owns authoritative settlement.
    /// Success remains provisional until Arrow EOS and clean response EOF.
    async fn consume(
        &mut self,
        stream: &mut OracleQueryStream,
        cancel: &CancellationToken,
        observed: &mut Option<QueryTerminalFrame>,
    ) -> Result<JsonValue, WyrdError> {
        let mut ipc = QueryIpcDecoder::new();
        let mut columns: Option<Vec<JsonValue>> = None;
        let mut schema: Option<arrow::datatypes::SchemaRef> = None;
        let mut rows: Vec<JsonValue> = Vec::new();
        let mut decoded_rows = 0_u64;
        let mut terminal: Option<JsonValue> = None;
        loop {
            let next = tokio::select! {
                biased;
                () = cancel.cancelled() => {
                    stream.request_cancel();
                    return Err(ValaError::QueryStreamIncomplete.into());
                }
                frame = stream.frames.next() => frame,
            };
            let Some(frame) = next else { break };
            let frame = match frame {
                Ok(frame) => frame,
                Err(error) => {
                    *observed = None;
                    stream.request_cancel();
                    return Err(WyrdError::from(error));
                }
            };
            match frame {
                QueryStreamFrame::Schema(accepted) if schema.is_none() && terminal.is_none() => {
                    match ipc.accept_schema(&accepted.arrow_ipc_schema) {
                        Ok(accepted) => {
                            let projected = project_columns(&accepted);
                            if let Err(error) = self.charge(&projected) {
                                stream.request_cancel();
                                return Err(error);
                            }
                            columns = Some(projected);
                            schema = Some(accepted);
                            #[cfg(feature = "test-support")]
                            if let Some(stall) = self.stall.take() {
                                stall.mark_entered();
                                cancel.cancelled().await;
                            }
                        }
                        Err(error) => {
                            stream.request_cancel();
                            return Err(super::service::arrow_decode_error(&error));
                        }
                    }
                }
                QueryStreamFrame::Batch(batch) if schema.is_some() && terminal.is_none() => {
                    let decoded = match ipc.accept_batch(&batch.arrow_ipc_batch) {
                        Ok(decoded) => decoded,
                        Err(error) => {
                            stream.request_cancel();
                            return Err(super::service::arrow_decode_error(&error));
                        }
                    };
                    let count = u64::try_from(decoded.num_rows())
                        .ok()
                        .and_then(|count| decoded_rows.checked_add(count));
                    let Some(count) = count else {
                        stream.request_cancel();
                        return Err(ValaError::QueryStreamProtocol.into());
                    };
                    decoded_rows = count;
                    if let Err(error) = self.retain(&mut rows, &decoded) {
                        stream.request_cancel();
                        return Err(error);
                    }
                }
                QueryStreamFrame::Terminal(frame) if schema.is_some() && terminal.is_none() => {
                    if frame
                        .validate()
                        .and_then(|()| frame.validate_emitted_rows(decoded_rows))
                        .is_err()
                    {
                        stream.request_cancel();
                        return Err(ValaError::QueryStreamProtocol.into());
                    }
                    if frame.outcome == QueryTerminalOutcome::Failed {
                        *observed = Some(frame.clone());
                        return Err(frame.error.as_ref().map_or(
                            WyrdError::from(ValaError::QueryExecutionFailed),
                            |error| {
                                WyrdError::from(super::service::terminal_error_to_bifrost(
                                    error.code,
                                ))
                            },
                        ));
                    }
                    if let Err(error) = ipc.accept_eos(&frame.arrow_ipc_eos) {
                        stream.request_cancel();
                        return Err(super::service::arrow_decode_error(&error));
                    }
                    terminal = Some(project_terminal(&frame));
                    *observed = Some(frame);
                }
                _ => {
                    *observed = None;
                    stream.request_cancel();
                    return Err(ValaError::QueryStreamProtocol.into());
                }
            }
        }
        let (Some(columns), Some(terminal)) = (columns, terminal) else {
            stream.request_cancel();
            return Err(ValaError::QueryStreamIncomplete.into());
        };
        if let Err(error) = self.charge(&terminal) {
            stream.request_cancel();
            return Err(error);
        }
        Ok(serde_json::json!({
            "columns": columns,
            "rows": rows,
            "terminal": terminal,
        }))
    }

    /// Append one decoded batch's rows as positional JSON arrays.
    ///
    /// # Errors
    ///
    /// Returns [`ValaError::QueryResultTooLarge`] when the batch would carry the
    /// result past the caller's row ceiling, and [`WyrdError::Internal`] when a
    /// column has no Arrow JSON encoder.
    fn retain(&mut self, rows: &mut Vec<JsonValue>, batch: &RecordBatch) -> Result<(), WyrdError> {
        if rows.len().saturating_add(batch.num_rows()) > self.max_rows {
            return Err(ValaError::QueryResultTooLarge.into());
        }
        let schema = batch.schema();
        let options = EncoderOptions::default();
        let mut encoders = Vec::with_capacity(batch.num_columns());
        for (field, array) in schema.fields().iter().zip(batch.columns()) {
            encoders.push(
                make_encoder(field, array.as_ref(), &options).map_err(|error| {
                    WyrdError::Internal {
                        message: format!("{QUERY} cannot encode column {}: {error}", field.name()),
                        details: serde_json::json!({ "tool": QUERY }),
                    }
                })?,
            );
        }
        let mut buffer = Vec::new();
        for index in 0..batch.num_rows() {
            let mut row = Vec::with_capacity(encoders.len());
            for encoder in &mut encoders {
                if encoder.is_null(index) {
                    row.push(JsonValue::Null);
                    continue;
                }
                buffer.clear();
                encoder.encode(index, &mut buffer);
                row.push(
                    serde_json::from_slice(&buffer).map_err(|error| WyrdError::Internal {
                        message: format!(
                            "{QUERY} produced an unreadable Arrow JSON value: {error}"
                        ),
                        details: serde_json::json!({ "tool": QUERY }),
                    })?,
                );
            }
            let row = JsonValue::Array(row);
            if !rows.is_empty() {
                self.charge_bytes(1)?;
            }
            self.charge(&row)?;
            rows.push(row);
        }
        Ok(())
    }

    /// Charge one value's exact compact-JSON length against the budget.
    ///
    /// The value is serialized into a temporary buffer purely to measure it, so
    /// the caller can refuse it before retaining it: an overflow must never
    /// leave a partially built result that could be mistaken for a short one.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdError::Internal`] when the value cannot be serialized, and
    /// [`ValaError::QueryResultTooLarge`] when charging it exceeds the ceiling.
    fn charge<T: serde::Serialize + ?Sized>(&mut self, value: &T) -> Result<(), WyrdError> {
        let measured = serde_json::to_vec(value).map_err(|error| WyrdError::Internal {
            message: format!("{QUERY} could not measure its own result: {error}"),
            details: serde_json::json!({ "tool": QUERY }),
        })?;
        self.charge_bytes(measured.len())
    }

    /// Charge an exact byte count, refusing overflow of the count or ceiling.
    ///
    /// # Errors
    ///
    /// Returns [`ValaError::QueryResultTooLarge`] when the running count would
    /// overflow or would exceed the caller's byte ceiling.
    fn charge_bytes(&mut self, amount: usize) -> Result<(), WyrdError> {
        let charged = self
            .bytes
            .checked_add(amount)
            .ok_or(ValaError::QueryResultTooLarge)?;
        if charged > self.max_bytes {
            return Err(ValaError::QueryResultTooLarge.into());
        }
        self.bytes = charged;
        Ok(())
    }
}

/// Project the accepted Arrow schema as ordered `{name, data_type, nullable}`.
///
/// The schema is accepted exactly once, so columns occur once in the result no
/// matter how many batches follow. Duplicate column names are preserved rather
/// than deduplicated, which is why rows are positional arrays: an object row
/// model cannot represent a result the engine can legally produce.
fn project_columns(schema: &arrow::datatypes::Schema) -> Vec<JsonValue> {
    schema
        .fields()
        .iter()
        .map(|field| {
            serde_json::json!({
                "name": field.name(),
                "data_type": field.data_type().to_string(),
                "nullable": field.is_nullable(),
            })
        })
        .collect()
}

/// Project the settled terminal as the evidence an agent can act on.
///
/// The Arrow end-of-stream delta is deliberately absent: it is stream framing
/// the collector already consumed to prove the stream closed, not a value.
fn project_terminal(frame: &QueryTerminalFrame) -> JsonValue {
    serde_json::json!({
        "outcome": frame.outcome,
        "query_class": frame.query_class,
        "row_count": frame.row_count,
        "warnings": frame.warnings,
        "source_completion": frame.source_completion,
    })
}

#[cfg(test)]
mod tests {
    use super::{MAX_BYTES_CEILING, ResultCollector, STRUCTURED_OVERHEAD_BYTES};
    use std::sync::Arc;

    use arrow::array::{ArrayRef, BinaryArray, Int64Array, StringArray};
    use arrow::datatypes::{DataType, Field, Fields, Schema};
    use arrow::record_batch::RecordBatch;
    use futures_util::StreamExt as _;
    use tokio_util::sync::CancellationToken;
    use vala_bifrost_redux::oracle::OracleQueryStream;
    use wyrd_spec::error::WyrdError;
    use wyrd_spec::vala::BifrostError;
    use wyrd_spec::vala::api::{
        QueryBatchFrame, QueryClass, QuerySchemaFrame, QuerySource, QueryStreamFrame,
        QueryTerminalError, QueryTerminalErrorCode, QueryTerminalFrame, QueryTerminalOutcome,
        SourceCompletion, SourceCompletionOutcome,
    };

    /// One synthetic batch carrying every shape the projection must survive.
    ///
    /// Two columns share the name `value` so an object row model would lose
    /// one of them, the first column has a null slot, `payload` is binary, and
    /// `nested` is a struct: between them they cover the null, duplicate-name,
    /// opaque-bytes, and non-scalar cases the encoder handles differently.
    ///
    /// # Panics
    ///
    /// Panics when the fixture arrays do not match the fixture schema, which is
    /// a defect in this test rather than a runtime condition.
    fn projection_batch() -> RecordBatch {
        let nested_fields = Fields::from(vec![Field::new("depth", DataType::Int64, false)]);
        let schema = Arc::new(Schema::new(vec![
            Field::new("value", DataType::Int64, true),
            Field::new("value", DataType::Utf8, false),
            Field::new("payload", DataType::Binary, false),
            Field::new("nested", DataType::Struct(nested_fields.clone()), false),
        ]));
        let nested: ArrayRef = Arc::new(arrow::array::StructArray::new(
            nested_fields,
            vec![Arc::new(Int64Array::from(vec![7, 8]))],
            None,
        ));
        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(Int64Array::from(vec![None, Some(2)])),
                Arc::new(StringArray::from(vec!["first", "second"])),
                Arc::new(BinaryArray::from(vec![
                    b"\x00\xff".as_ref(),
                    b"ok".as_ref(),
                ])),
                nested,
            ],
        )
        .expect("the fixture arrays match the fixture schema")
    }

    /// Build one settled synthetic Oracle stream carrying `batch`.
    ///
    /// # Panics
    ///
    /// Panics when the Arrow IPC writer rejects the fixture batch.
    fn synthetic_stream(batch: &RecordBatch) -> (OracleQueryStream, CancellationToken) {
        let mut writer =
            arrow::ipc::writer::StreamWriter::try_new(Vec::new(), batch.schema().as_ref())
                .expect("the fixture schema writes");
        let prefix = std::mem::take(writer.get_mut());
        writer.write(batch).expect("the fixture batch writes");
        let fragment = std::mem::take(writer.get_mut());
        writer.finish().expect("the fixture writer finishes");
        let eos = std::mem::take(writer.get_mut());
        let frames = vec![
            Ok(QueryStreamFrame::Schema(QuerySchemaFrame {
                schema_fingerprint: "mcp-projection".to_owned(),
                arrow_ipc_schema: prefix,
            })),
            Ok(QueryStreamFrame::Batch(QueryBatchFrame {
                arrow_ipc_batch: fragment,
            })),
            Ok(QueryStreamFrame::Terminal(QueryTerminalFrame {
                outcome: QueryTerminalOutcome::Success,
                query_class: QueryClass::Interactive,
                row_count: 2,
                warnings: Vec::new(),
                source_completion: [
                    QuerySource::Iceberg,
                    QuerySource::HotSealed,
                    QuerySource::LiveTail,
                ]
                .into_iter()
                .map(|source| SourceCompletion {
                    source,
                    outcome: SourceCompletionOutcome::Complete,
                })
                .collect(),
                error: None,
                arrow_ipc_eos: eos,
            })),
        ];
        let cancellation = CancellationToken::new();
        let stream = OracleQueryStream::test_new(
            "mcp-projection".to_owned(),
            Box::pin(async_stream::stream! {
                for frame in frames {
                    yield frame;
                }
            }),
            cancellation.clone(),
        );
        (stream, cancellation)
    }

    /// Reject malformed terminals and trailing frames while signaling cancellation.
    ///
    /// A valid failed terminal is already settled, so its error survives empty
    /// EOS and does not trigger a second cancellation.
    /// This pure decoder seam checks the original errors and signal; the real
    /// MCP and server journeys check authoritative settlement before return.
    ///
    /// # Panics
    /// Panics if a malformed stream is accepted, its error changes, or signaling is lost.
    #[test]
    fn query_rejects_untrustworthy_terminal_and_settles_stream() {
        wyrd_runtime::runtime().block_on(async {
            for (case, code, cancelled) in [
                ("missing EOF", "WYRD_VALA_502_QUERY_STREAM_INCOMPLETE", true),
                ("matrix", "WYRD_VALA_502_QUERY_STREAM_PROTOCOL", true),
                ("row count", "WYRD_VALA_502_QUERY_STREAM_PROTOCOL", true),
                ("EOS", "WYRD_SPEC_500_INTERNAL", true),
                ("failed", "WYRD_VALA_504_QUERY_TIMEOUT", false),
                ("duplicate", "WYRD_VALA_502_QUERY_STREAM_PROTOCOL", true),
                (
                    "trailing batch",
                    "WYRD_VALA_502_QUERY_STREAM_PROTOCOL",
                    true,
                ),
                ("trailing error", "WYRD_VALA_504_QUERY_TIMEOUT", true),
                (
                    "missing terminal",
                    "WYRD_VALA_502_QUERY_STREAM_INCOMPLETE",
                    true,
                ),
            ] {
                let (mut stream, token) = synthetic_stream(&projection_batch());
                let mut frames: Vec<_> = stream.frames.by_ref().collect().await;
                let Ok(QueryStreamFrame::Terminal(terminal)) = &mut frames[2] else {
                    panic!("fixture ends with a terminal");
                };
                match case {
                    "missing EOF" => {}
                    "matrix" => terminal.source_completion.clear(),
                    "row count" => terminal.row_count += 1,
                    "EOS" => terminal.arrow_ipc_eos = vec![1],
                    "failed" => {
                        terminal.outcome = QueryTerminalOutcome::Failed;
                        terminal.error = Some(QueryTerminalError {
                            code: QueryTerminalErrorCode::QueryTimeout,
                            detail: None,
                        });
                        terminal.arrow_ipc_eos.clear();
                    }
                    "duplicate" => frames.push(frames[2].clone()),
                    "trailing batch" => frames.push(frames[1].clone()),
                    "trailing error" => frames.push(Err(BifrostError::QueryTimeout)),
                    "missing terminal" => {
                        frames.pop();
                    }
                    _ => unreachable!("closed test cases"),
                }
                let cancel = CancellationToken::new();
                let cancel_at_eof = cancel.clone();
                stream.frames = Box::pin(futures_util::stream::iter(frames).chain(
                    futures_util::stream::poll_fn(move |_| {
                        if case == "missing EOF" {
                            cancel_at_eof.cancel();
                            std::task::Poll::Pending
                        } else {
                            std::task::Poll::Ready(None)
                        }
                    }),
                ));
                let mut collector = ResultCollector {
                    settlement: None,
                    max_rows: 10,
                    max_bytes: MAX_BYTES_CEILING,
                    bytes: STRUCTURED_OVERHEAD_BYTES,
                    #[cfg(feature = "test-support")]
                    stall: None,
                };
                let error = collector
                    .consume(&mut stream, &cancel, &mut None)
                    .await
                    .expect_err(case);
                assert_eq!(error.code(), code, "{case}: {error}");
                assert_eq!(
                    token.is_cancelled(),
                    cancelled,
                    "{case}: cancellation signaled before return"
                );
            }
        });
    }

    /// The result is positional, Arrow-encoded, and charged to the exact byte.
    ///
    /// The byte ceiling is the only budget an agent cannot estimate for itself,
    /// so it has to mean exactly one thing: the compact JSON the tool actually
    /// returns. The test recomputes that length from the returned value, proves
    /// the exact length is accepted, and proves one byte less is refused as a
    /// canonical too-large error rather than a truncated success.
    ///
    /// # Panics
    /// Panics if projection changes ordering or values, or byte accounting misses the ceiling.
    #[test]
    fn query_result_is_positional_and_counts_exact_structured_json_bytes() {
        wyrd_runtime::runtime().block_on(async {
            let batch = projection_batch();
            let collector = |max_bytes: usize| ResultCollector {
                settlement: None,
                max_rows: 10,
                max_bytes,
                bytes: STRUCTURED_OVERHEAD_BYTES,
                #[cfg(feature = "test-support")]
                stall: None,
            };

            let (mut stream, _token) = synthetic_stream(&batch);
            let value = collector(MAX_BYTES_CEILING)
                .consume(&mut stream, &CancellationToken::new(), &mut None)
                .await
                .expect("an unconstrained projection succeeds");

            assert_eq!(
                value["columns"],
                serde_json::json!([
                    {"name": "value", "data_type": "Int64", "nullable": true},
                    {"name": "value", "data_type": "Utf8", "nullable": false},
                    {"name": "payload", "data_type": "Binary", "nullable": false},
                    {
                        "name": "nested",
                        "data_type": "Struct(\"depth\": non-null Int64)",
                        "nullable": false,
                    },
                ]),
                "columns are ordered triples that keep both `value` columns"
            );
            assert_eq!(
                value["rows"],
                serde_json::json!([
                    [null, "first", "00ff", {"depth": 7}],
                    [2, "second", "6f6b", {"depth": 8}],
                ]),
                "rows are positional arrays using Arrow's own JSON encoding"
            );

            let exact = serde_json::to_vec(&value)
                .expect("the projected value serializes")
                .len();

            let (mut stream, _token) = synthetic_stream(&batch);
            assert_eq!(
                collector(exact)
                    .consume(&mut stream, &CancellationToken::new(), &mut None)
                    .await
                    .expect("the exact serialized length is within budget"),
                value,
                "the ceiling counts exactly the bytes the tool returns"
            );

            let (mut stream, token) = synthetic_stream(&batch);
            let refused = collector(exact - 1)
                .consume(&mut stream, &CancellationToken::new(), &mut None)
                .await
                .expect_err("one byte less than the result is refused");
            assert_eq!(
                refused.code(),
                "WYRD_VALA_413_QUERY_RESULT_TOO_LARGE",
                "overflow is the canonical too-large error, never a truncation"
            );
            assert!(
                matches!(refused, WyrdError::Vala { .. }),
                "overflow keeps its Bifrost error identity: {refused:?}"
            );
            assert!(
                token.is_cancelled(),
                "an overflowing stream is settled before the tool returns"
            );
        });
    }
}
