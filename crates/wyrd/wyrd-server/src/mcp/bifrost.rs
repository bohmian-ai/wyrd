//! The three read-only Bifrost capabilities Wyrd advertises over MCP.
//!
//! The adapter owns exactly three things: parsing a closed request, projecting
//! the answer an agent can act on, and translating failure onto the protocol.
//! Authorization, tenancy, audit, SQL admission, execution, cancellation, and
//! terminal settlement all stay with the server operations this module calls —
//! `bifrost::service` for the catalog and `query::service` for the read path —
//! so there is one implementation of each, not an MCP-shaped copy.

use std::sync::Arc;

use rmcp::model::{CallToolResult, Tool, ToolAnnotations};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::BifrostTableEntry;

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
fn read_tool(name: &'static str, title: &str, description: &'static str, schema: JsonValue) -> Tool {
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
