//! The three read-only Bifrost capabilities Wyrd advertises over MCP.
//!
//! The adapter owns exactly three things: parsing a closed request, projecting
//! the answer an agent can act on, and translating failure onto the protocol.
//! Authorization, tenancy, audit, SQL admission, execution, cancellation, and
//! terminal settlement all stay with the server operations this module calls —
//! `bifrost::service` for the catalog and `query::service` for the read path —
//! so there is one implementation of each, not an MCP-shaped copy.

use std::sync::Arc;

use arrow::json::writer::{EncoderOptions, make_encoder};
use arrow::record_batch::RecordBatch;
use futures_util::StreamExt as _;
use rmcp::model::{CallToolResult, Tool, ToolAnnotations};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use vala_bifrost_redux::oracle::{OracleQueryStream, QueryIpcDecoder};
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::BifrostError as ValaError;
use wyrd_spec::vala::api::{
    BifrostQueryRequest, BifrostTableEntry, FreshnessPolicy, VisibilityMode,
};
use wyrd_spec::vala::api::{QueryStreamFrame, QueryTerminalFrame, QueryTerminalOutcome};

use crate::bifrost::service;
use crate::components::auth::Caller;
use crate::state::AppState;

/// Wire name of the compact authorized table listing.
pub const LIST_TABLES: &str = "bifrost.list_tables";

/// Wire name of the full schema and physical-layout description.
pub const DESCRIBE_TABLE: &str = "bifrost.describe_table";

/// Wire name of the bounded read-only query.
pub const QUERY: &str = "bifrost.query";

/// Largest SQL text `bifrost.query` accepts, in UTF-8 bytes.
///
/// This is an MCP-local input bound, not a Wyrd query contract: it exists so a
/// runaway agent prompt is refused before it reaches the SQL floor at all.
const MAX_SQL_BYTES: usize = 65_536;

/// Rows returned when a caller names no `max_rows`.
const DEFAULT_MAX_ROWS: u32 = 1_000;

/// Largest `max_rows` an MCP caller may ask for.
///
/// Deliberately far below the server's own million-row sync ceiling: an MCP
/// result is one JSON value an agent has to hold in a context window, so the
/// binding constraint here is the reader, not the engine.
const MAX_ROWS_CEILING: u32 = 10_000;

/// Serialized result bytes returned when a caller names no `max_bytes`.
const DEFAULT_MAX_BYTES: usize = 4 * 1024 * 1024;

/// Largest `max_bytes` an MCP caller may ask for, well under the server's 256 MiB.
const MAX_BYTES_CEILING: usize = 16 * 1024 * 1024;

/// The exact catalog every ordinary Wyrd server advertises.
///
/// Order is part of the contract an agent reads: discovery, then description,
/// then the query those two exist to make writable.
#[must_use]
pub fn descriptors() -> Vec<Tool> {
    vec![list_tables_tool(), describe_table_tool(), query_tool()]
}

/// Build one read-only tool descriptor from its static input schema.
///
/// # Panics
///
/// Panics if `schema` is not a JSON object, which every caller below passes.
fn read_tool(
    name: &'static str,
    title: &str,
    description: &'static str,
    schema: JsonValue,
) -> Tool {
    let JsonValue::Object(schema) = schema else {
        unreachable!("every Bifrost tool input schema is a JSON object")
    };
    Tool::new(name, description, Arc::new(schema))
        .with_title(title)
        .annotate(ToolAnnotations::default().read_only(true))
}

/// Descriptor for the compact authorized table listing.
fn list_tables_tool() -> Tool {
    read_tool(
        LIST_TABLES,
        "List Bifrost tables",
        "List the Bifrost tables this caller's tenant is authorized to read, as compact \
         namespace, name, and status entries.",
        serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }),
    )
}

/// Descriptor for the full schema and physical-layout description.
fn describe_table_tool() -> Tool {
    read_tool(
        DESCRIBE_TABLE,
        "Describe a Bifrost table",
        "Describe one authorized Bifrost table: every field with its metadata, the event-time \
         partition granularity, the ordered sort keys, and the Bloom-filtered columns.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "namespace": {
                    "type": "string",
                    "description": "Fully qualified Bifrost namespace, e.g. `vala.bifrost`."
                },
                "name": {
                    "type": "string",
                    "description": "Table name within the namespace."
                }
            },
            "required": ["namespace", "name"],
            "additionalProperties": false
        }),
    )
}

/// Descriptor for the bounded read-only query.
///
/// The schema is closed on purpose: identity, tenancy, delegation, roles,
/// execution path, query class, topology, and plan are all server state, and a
/// caller that names any of them is refused rather than quietly ignored.
fn query_tool() -> Tool {
    read_tool(
        QUERY,
        "Query Bifrost",
        "Run one bounded read-only SELECT against this caller's authorized Bifrost tables and \
         return the complete result. Wyrd selects the execution path; the caller cannot.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "sql": {
                    "type": "string",
                    "description": "A single read-only SELECT statement.",
                    "minLength": 1,
                    "maxLength": MAX_SQL_BYTES
                },
                "visibility": {
                    "type": "string",
                    "enum": ["published_only", "fused"],
                    "default": "published_only",
                    "description": "Which visibility tiers the immutable read cut includes."
                },
                "freshness": {
                    "type": "string",
                    "enum": ["strict", "allow_degraded"],
                    "default": "strict",
                    "description": "Whether an unavailable live source fails the query or degrades it."
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
        }),
    )
}

/// Arguments accepted by [`DESCRIBE_TABLE`].
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DescribeTableArguments {
    /// Fully qualified Bifrost namespace as the catalog spells it.
    namespace: String,
    /// Table name within that namespace.
    name: String,
}

/// List the caller's authorized tables as compact entries.
///
/// The projection is deliberately narrower than the catalog's own entry: a
/// listing exists to let an agent choose a table, and uid, fingerprint, and
/// timestamps are all things `bifrost.describe_table` answers precisely.
///
/// # Errors
///
/// Propagates the catalog service's authorization, audit, role-availability,
/// and lookup errors unchanged.
pub(crate) async fn list_tables(
    state: &AppState,
    caller: Caller,
) -> Result<CallToolResult, WyrdError> {
    let tables = service::list_tables(state, caller).await?;
    Ok(CallToolResult::structured(serde_json::json!({
        "tables": tables.iter().map(compact_entry).collect::<Vec<_>>(),
    })))
}

/// Project one catalog entry onto the three fields a listing carries.
fn compact_entry(entry: &BifrostTableEntry) -> JsonValue {
    serde_json::json!({
        "namespace": entry.namespace,
        "name": entry.name,
        "status": entry.status,
    })
}

/// Describe one authorized table's schema and physical layout.
///
/// The catalog's description already carries every field, its metadata, the
/// partition granularity, the ordered sort keys, and the Bloom columns, so the
/// adapter serializes it whole rather than reassembling a second view of it.
///
/// # Errors
///
/// Returns [`WyrdError::Validation`] when the arguments do not match the input
/// schema, propagates the catalog service's authorization, audit, namespace,
/// and not-found errors, and returns [`WyrdError::Internal`] if the
/// description cannot be serialized.
pub(crate) async fn describe_table(
    state: &AppState,
    caller: Caller,
    arguments: Option<serde_json::Map<String, JsonValue>>,
) -> Result<CallToolResult, WyrdError> {
    let arguments: DescribeTableArguments = parse_arguments(DESCRIBE_TABLE, arguments)?;
    let description =
        service::describe_table(state, caller, arguments.namespace, arguments.name).await?;
    Ok(CallToolResult::structured(
        serde_json::to_value(&description).map_err(|error| WyrdError::Internal {
            message: format!("{DESCRIBE_TABLE} could not serialize its description: {error}"),
            details: serde_json::json!({ "tool": DESCRIBE_TABLE }),
        })?,
    ))
}

/// Deserialize one tool's closed arguments, defaulting an absent object.
///
/// # Errors
///
/// Returns [`WyrdError::Validation`] naming the tool when the arguments carry
/// an unknown key, a wrong type, or a missing required field.
fn parse_arguments<T: serde::de::DeserializeOwned>(
    tool: &'static str,
    arguments: Option<serde_json::Map<String, JsonValue>>,
) -> Result<T, WyrdError> {
    let value = JsonValue::Object(arguments.unwrap_or_default());
    serde_json::from_value(value).map_err(|error| WyrdError::Validation {
        message: format!("invalid {tool} arguments: {error}"),
        details: serde_json::json!({ "tool": tool }),
    })
}

/// Arguments accepted by [`QUERY`].
///
/// Every field is either the SQL itself or one of the four query-policy values
/// the public request already carries, plus the two MCP-local response
/// ceilings. There is no path, class, tenant, or principal field to omit.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct QueryArguments {
    /// A single read-only SELECT statement.
    sql: String,
    /// Visibility tiers the immutable read cut includes.
    #[serde(default = "default_visibility")]
    visibility: VisibilityMode,
    /// Behavior when a requested live source cannot complete.
    #[serde(default)]
    freshness: FreshnessPolicy,
    /// Optional caller deadline in milliseconds.
    #[serde(default)]
    deadline_ms: Option<u32>,
    /// Row ceiling for the complete result.
    #[serde(default = "default_max_rows")]
    max_rows: u32,
    /// Serialized JSON byte ceiling for the complete result.
    #[serde(default = "default_max_bytes")]
    max_bytes: usize,
}

/// Default visibility for an omitted `visibility`.
fn default_visibility() -> VisibilityMode {
    VisibilityMode::PublishedOnly
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
    /// Check the MCP-local input bounds the JSON schema advertises.
    ///
    /// The advertised schema is advice a client may ignore, so the same bounds
    /// are enforced here. Everything about the SQL itself beyond its size —
    /// single statement, SELECT-only, allowed functions, known tables, tenant
    /// authorization — belongs to the server's SQL floor and query service and
    /// is deliberately not repeated.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdError::Validation`] when the SQL is blank or oversized,
    /// the deadline is zero, or either ceiling is zero or above its maximum.
    fn validate(&self) -> Result<(), WyrdError> {
        let invalid = |reason: &str, details: JsonValue| WyrdError::Validation {
            message: format!("invalid {QUERY} arguments: {reason}"),
            details,
        };
        if self.sql.trim().is_empty() {
            return Err(invalid("sql must not be blank", serde_json::json!({})));
        }
        if self.sql.len() > MAX_SQL_BYTES {
            return Err(invalid(
                "sql exceeds the MCP input ceiling",
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
                "max_rows is outside the MCP range",
                serde_json::json!({ "max_rows": self.max_rows, "ceiling": MAX_ROWS_CEILING }),
            ));
        }
        if self.max_bytes == 0 || self.max_bytes > MAX_BYTES_CEILING {
            return Err(invalid(
                "max_bytes is outside the MCP range",
                serde_json::json!({ "max_bytes": self.max_bytes, "ceiling": MAX_BYTES_CEILING }),
            ));
        }
        Ok(())
    }

    /// Project the four query-policy values onto the public query request.
    fn to_request(&self) -> BifrostQueryRequest {
        BifrostQueryRequest {
            sql: self.sql.clone(),
            visibility: self.visibility,
            freshness: self.freshness,
            deadline_ms: self.deadline_ms.map(u64::from),
        }
    }
}

/// Run one bounded read-only query and return its complete result.
///
/// Everything that decides whether the query may run, what it may read, and how
/// it ends belongs to [`query::service::stream_query`] and the Oracle stream it
/// returns. This function owns exactly the MCP-shaped remainder: the closed
/// input, the lower response budget, the final serialization, and returning the
/// value only after the stream has settled.
///
/// # Errors
///
/// Returns [`WyrdError::Validation`] when the arguments do not match the closed
/// input schema or exceed an MCP bound, propagates the query service's
/// authorization, audit, SQL, planning, admission, and execution errors, and
/// returns [`ValaError::QueryResultTooLarge`] when the complete result would
/// exceed the caller's row or byte ceiling.
///
/// [`ValaError::QueryResultTooLarge`]: wyrd_spec::vala::BifrostError::QueryResultTooLarge
pub(crate) async fn query(
    state: &AppState,
    caller: Caller,
    arguments: Option<serde_json::Map<String, JsonValue>>,
) -> Result<CallToolResult, WyrdError> {
    let arguments: QueryArguments = parse_arguments(QUERY, arguments)?;
    arguments.validate()?;
    let stream =
        crate::query::service::stream_query(state.clone(), caller, arguments.to_request()).await?;
    let collector = ResultCollector {
        max_rows: usize::try_from(arguments.max_rows).unwrap_or(usize::MAX),
    };
    Ok(CallToolResult::structured(collector.collect(stream).await?))
}

/// Collects one Oracle stream into the single value `bifrost.query` returns.
///
/// The collector is the MCP counterpart of the typed route's own bounded
/// collector: same stateful IPC decoder, same exactly-one-terminal rule, same
/// active cancellation on every pre-terminal failure. What differs is only the
/// budget it charges and the shape it produces.
struct ResultCollector {
    /// Row ceiling this caller asked for. Exceeding it fails; nothing truncates.
    max_rows: usize,
}

impl ResultCollector {
    /// Drain `stream` to its terminal and build `{columns, rows, terminal}`.
    ///
    /// # Errors
    ///
    /// Returns a stream-protocol, Arrow-decoding, execution, or result-size
    /// error. Every pre-terminal failure awaits [`OracleQueryStream::cancel`]
    /// first, so Oracle has released admission, memory, peer work, scratch, and
    /// graph leases before this returns.
    async fn collect(self, mut stream: OracleQueryStream) -> Result<JsonValue, WyrdError> {
        let mut ipc = QueryIpcDecoder::new();
        let mut columns: Option<Vec<JsonValue>> = None;
        let mut schema: Option<arrow::datatypes::SchemaRef> = None;
        let mut rows: Vec<JsonValue> = Vec::new();
        let mut terminal: Option<JsonValue> = None;
        while let Some(frame) = stream.frames.next().await {
            let frame = match frame {
                Ok(frame) => frame,
                Err(error) => {
                    stream.cancel().await;
                    return Err(WyrdError::from(error));
                }
            };
            match frame {
                QueryStreamFrame::Schema(accepted) if schema.is_none() => {
                    match ipc.accept_schema(&accepted.arrow_ipc_schema) {
                        Ok(accepted) => {
                            columns = Some(project_columns(&accepted));
                            schema = Some(accepted);
                        }
                        Err(error) => {
                            stream.cancel().await;
                            return Err(crate::query::service::arrow_decode_error(&error));
                        }
                    }
                }
                QueryStreamFrame::Batch(batch) if schema.is_some() && terminal.is_none() => {
                    let decoded = match ipc.accept_batch(&batch.arrow_ipc_batch) {
                        Ok(decoded) => decoded,
                        Err(error) => {
                            stream.cancel().await;
                            return Err(crate::query::service::arrow_decode_error(&error));
                        }
                    };
                    if let Err(error) = self.retain(&mut rows, &decoded) {
                        stream.cancel().await;
                        return Err(error);
                    }
                }
                QueryStreamFrame::Terminal(frame) if schema.is_some() && terminal.is_none() => {
                    if frame.outcome == QueryTerminalOutcome::Failed {
                        return Err(frame.error.as_ref().map_or(
                            WyrdError::from(ValaError::QueryExecutionFailed),
                            |error| {
                                WyrdError::from(crate::query::service::terminal_error_to_bifrost(
                                    error.code,
                                ))
                            },
                        ));
                    }
                    if let Err(error) = ipc.accept_eos(&frame.arrow_ipc_eos) {
                        return Err(crate::query::service::arrow_decode_error(&error));
                    }
                    terminal = Some(project_terminal(&frame));
                    break;
                }
                _ => {
                    stream.cancel().await;
                    return Err(ValaError::QueryStreamProtocol.into());
                }
            }
        }
        let (Some(columns), Some(terminal)) = (columns, terminal) else {
            stream.cancel().await;
            return Err(ValaError::QueryStreamIncomplete.into());
        };
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
    fn retain(&self, rows: &mut Vec<JsonValue>, batch: &RecordBatch) -> Result<(), WyrdError> {
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
            rows.push(JsonValue::Array(row));
        }
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
        "freshness": frame.freshness,
        "execution_path": frame.execution_path,
        "row_count": frame.row_count,
        "warnings": frame.warnings,
        "source_completion": frame.source_completion,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_MAX_BYTES, DEFAULT_MAX_ROWS, MAX_BYTES_CEILING, MAX_ROWS_CEILING, MAX_SQL_BYTES,
        QUERY, QueryArguments, descriptors, parse_arguments,
    };
    use wyrd_spec::vala::api::{FreshnessPolicy, VisibilityMode};

    /// The query tool accepts exactly six bounded fields and no path selector.
    ///
    /// The closed schema is the whole security claim of the input surface: a
    /// caller cannot name a tenant, principal, delegation, role, execution
    /// path, query class, topology, or plan, because every one of those is
    /// server state and an unknown key must be refused rather than dropped.
    /// The advertised schema and the parser are asserted together because a
    /// client is free to ignore the first — only the second is enforcement.
    #[test]
    fn query_schema_is_closed_bounded_and_has_no_path_selector() {
        let tool = descriptors()
            .into_iter()
            .find(|tool| tool.name == QUERY)
            .expect("the catalog advertises the query tool");
        let schema = serde_json::to_value(tool.input_schema.as_ref()).expect("schema serializes");

        assert_eq!(schema["additionalProperties"], serde_json::json!(false));
        assert_eq!(schema["required"], serde_json::json!(["sql"]));
        let properties = schema["properties"]
            .as_object()
            .expect("the schema declares properties");
        let mut names: Vec<&str> = properties.keys().map(String::as_str).collect();
        names.sort_unstable();
        assert_eq!(
            names,
            vec![
                "deadline_ms",
                "freshness",
                "max_bytes",
                "max_rows",
                "sql",
                "visibility"
            ],
            "the query input is exactly the six closed fields"
        );

        assert_eq!(properties["sql"]["minLength"], serde_json::json!(1));
        assert_eq!(
            properties["sql"]["maxLength"],
            serde_json::json!(MAX_SQL_BYTES)
        );
        assert_eq!(
            properties["visibility"]["enum"],
            serde_json::json!(["published_only", "fused"])
        );
        assert_eq!(
            properties["visibility"]["default"],
            serde_json::json!("published_only")
        );
        assert_eq!(
            properties["freshness"]["enum"],
            serde_json::json!(["strict", "allow_degraded"])
        );
        assert_eq!(
            properties["freshness"]["default"],
            serde_json::json!("strict")
        );
        assert_eq!(properties["deadline_ms"]["minimum"], serde_json::json!(1));
        assert_eq!(
            properties["deadline_ms"]["maximum"],
            serde_json::json!(u32::MAX)
        );
        assert_eq!(properties["max_rows"]["minimum"], serde_json::json!(1));
        assert_eq!(
            properties["max_rows"]["default"],
            serde_json::json!(DEFAULT_MAX_ROWS)
        );
        assert_eq!(
            properties["max_rows"]["maximum"],
            serde_json::json!(MAX_ROWS_CEILING)
        );
        assert_eq!(properties["max_bytes"]["minimum"], serde_json::json!(1));
        assert_eq!(
            properties["max_bytes"]["default"],
            serde_json::json!(DEFAULT_MAX_BYTES)
        );
        assert_eq!(
            properties["max_bytes"]["maximum"],
            serde_json::json!(MAX_BYTES_CEILING)
        );

        let parse = |value: serde_json::Value| {
            parse_arguments::<QueryArguments>(
                QUERY,
                Some(value.as_object().expect("test argument object").clone()),
            )
        };

        for rejected in [
            "tenant_id",
            "data_tenant_id",
            "principal_id",
            "delegation_chain",
            "roles",
            "execution_path",
            "path",
            "query_class",
            "topology",
            "plan",
        ] {
            assert!(
                parse(serde_json::json!({"sql": "SELECT 1", rejected: "anything"})).is_err(),
                "the query input must refuse a caller-supplied `{rejected}`"
            );
        }

        let defaults = parse(serde_json::json!({"sql": "SELECT 1"})).expect("bare sql parses");
        defaults.validate().expect("bare sql is in range");
        let request = defaults.to_request();
        assert_eq!(request.sql, "SELECT 1");
        assert_eq!(request.visibility, VisibilityMode::PublishedOnly);
        assert_eq!(request.freshness, FreshnessPolicy::Strict);
        assert_eq!(request.deadline_ms, None);
        assert_eq!(defaults.max_rows, DEFAULT_MAX_ROWS);
        assert_eq!(defaults.max_bytes, DEFAULT_MAX_BYTES);

        let named = parse(serde_json::json!({
            "sql": "SELECT 1",
            "visibility": "fused",
            "freshness": "allow_degraded",
            "deadline_ms": u32::MAX,
            "max_rows": MAX_ROWS_CEILING,
            "max_bytes": MAX_BYTES_CEILING,
        }))
        .expect("every field at its bound parses");
        named.validate().expect("every field at its bound is valid");
        let request = named.to_request();
        assert_eq!(request.visibility, VisibilityMode::Fused);
        assert_eq!(request.freshness, FreshnessPolicy::AllowDegraded);
        assert_eq!(request.deadline_ms, Some(u64::from(u32::MAX)));

        for (case, arguments) in [
            ("blank sql", serde_json::json!({"sql": "   "})),
            (
                "oversized sql",
                serde_json::json!({"sql": "a".repeat(MAX_SQL_BYTES + 1)}),
            ),
            (
                "zero deadline",
                serde_json::json!({"sql": "SELECT 1", "deadline_ms": 0}),
            ),
            (
                "deadline above the u32 range",
                serde_json::json!({"sql": "SELECT 1", "deadline_ms": u64::from(u32::MAX) + 1}),
            ),
            (
                "zero rows",
                serde_json::json!({"sql": "SELECT 1", "max_rows": 0}),
            ),
            (
                "rows above the ceiling",
                serde_json::json!({"sql": "SELECT 1", "max_rows": MAX_ROWS_CEILING + 1}),
            ),
            (
                "zero bytes",
                serde_json::json!({"sql": "SELECT 1", "max_bytes": 0}),
            ),
            (
                "bytes above the ceiling",
                serde_json::json!({"sql": "SELECT 1", "max_bytes": MAX_BYTES_CEILING + 1}),
            ),
        ] {
            let refused = match parse(arguments) {
                Ok(parsed) => parsed.validate().is_err(),
                Err(_) => true,
            };
            assert!(refused, "{case} must be refused");
        }
    }
}
