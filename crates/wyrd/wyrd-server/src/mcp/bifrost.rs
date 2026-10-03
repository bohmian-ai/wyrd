//! The three read-only Bifrost capabilities Wyrd advertises over MCP.
//!
//! The adapter owns exactly three things: parsing a closed request, projecting
//! the answer an agent can act on, and translating failure onto the protocol.
//! Authorization, tenancy, audit, SQL admission, execution, cancellation, and
//! terminal settlement all stay with the server operations this module calls —
//! `bifrost::service` for the catalog and `query::collect` for the bounded
//! read path it shares with Workflow Agent tools — so there is one
//! implementation of each, not an MCP-shaped copy.

use std::sync::Arc;

use rmcp::model::{CallToolResult, ErrorData, Tool, ToolAnnotations};
use rmcp::service::{RequestContext, RoleServer};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::BifrostTableEntry;

use super::WyrdMcpHandler;
use crate::bifrost::service;
use crate::components::auth::Caller;
use crate::query::collect::{BoundedQuery, QueryArguments, input_schema};

/// Wire name of the compact authorized table listing.
pub(super) const LIST_TABLES: &str = "bifrost.list_tables";

/// Wire name of the full schema and physical-layout description.
pub(super) const DESCRIBE_TABLE: &str = "bifrost.describe_table";

/// Wire name of the bounded read-only query.
pub(super) const QUERY: &str = crate::query::collect::QUERY;

/// The exact catalog every ordinary Wyrd server advertises.
///
/// Order is part of the contract an agent reads: discovery, then description,
/// then the query those two exist to make writable.
#[must_use]
pub(super) fn descriptors() -> Vec<Tool> {
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
        input_schema(),
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

/// Project one catalog entry onto the three fields a listing carries.
fn compact_entry(entry: &BifrostTableEntry) -> JsonValue {
    serde_json::json!({
        "namespace": entry.namespace,
        "name": entry.name,
        "status": entry.status,
    })
}

/// Deserialize one tool's closed arguments, defaulting an absent object.
///
/// # Errors
///
/// Returns MCP invalid params naming the tool when the arguments carry
/// an unknown key, a wrong type, or a missing required field.
fn parse_arguments<T: serde::de::DeserializeOwned>(
    tool: &'static str,
    arguments: Option<serde_json::Map<String, JsonValue>>,
) -> Result<T, ErrorData> {
    let value = JsonValue::Object(arguments.unwrap_or_default());
    serde_json::from_value(value).map_err(|error| {
        ErrorData::invalid_params(
            format!("invalid {tool} arguments: {error}"),
            Some(serde_json::json!({ "tool": tool })),
        )
    })
}

impl WyrdMcpHandler {
    /// List authorized tables using the catalog service and compact their entries.
    ///
    /// # Errors
    ///
    /// Returns MCP invalid params for any supplied key. Catalog authorization,
    /// audit, and availability failures become canonical structured tool errors.
    pub(super) async fn list_tables(
        &self,
        caller: Caller,
        arguments: Option<serde_json::Map<String, JsonValue>>,
    ) -> Result<CallToolResult, ErrorData> {
        if arguments
            .as_ref()
            .is_some_and(|arguments| !arguments.is_empty())
        {
            return Err(ErrorData::invalid_params(
                "bifrost.list_tables accepts no arguments; omit them or send an empty object.",
                None,
            ));
        }
        let result = service::list_tables(&self.state, caller)
            .await
            .map(|tables| {
                CallToolResult::structured(serde_json::json!({
                    "tables": tables.iter().map(compact_entry).collect::<Vec<_>>(),
                }))
            });
        Ok(
            result
                .unwrap_or_else(|error| CallToolResult::structured_error(error.as_problem_json())),
        )
    }

    /// Describe an authorized table using the catalog's complete schema and layout.
    ///
    /// # Errors
    ///
    /// Returns MCP invalid params for malformed arguments. Catalog and encoding
    /// failures become canonical structured tool errors without a partial result.
    pub(super) async fn describe_table(
        &self,
        caller: Caller,
        arguments: Option<serde_json::Map<String, JsonValue>>,
    ) -> Result<CallToolResult, ErrorData> {
        let arguments: DescribeTableArguments = parse_arguments(DESCRIBE_TABLE, arguments)?;
        let result = async {
            let description =
                service::describe_table(&self.state, caller, arguments.namespace, arguments.name)
                    .await?;
            let value =
                serde_json::to_value(&description).map_err(|error| WyrdError::Internal {
                    message: format!(
                        "{DESCRIBE_TABLE} could not serialize its description: {error}"
                    ),
                    details: serde_json::json!({"tool": DESCRIBE_TABLE}),
                })?;
            Ok(CallToolResult::structured(value))
        }
        .await;
        Ok(result.unwrap_or_else(|error: WyrdError| {
            CallToolResult::structured_error(error.as_problem_json())
        }))
    }

    /// Run a bounded query through the authenticated service and collect its result.
    ///
    /// Protocol cancellation awaits Oracle stream cancellation before returning;
    /// Oracle owns authorization, execution, audit, and resource settlement.
    ///
    /// # Errors
    ///
    /// Returns MCP invalid params for malformed or out-of-range arguments.
    /// Service, stream, and result-ceiling failures become canonical structured
    /// tool errors with no partial rows.
    pub(super) async fn query(
        &self,
        caller: Caller,
        arguments: Option<serde_json::Map<String, JsonValue>>,
        context: &RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let arguments: QueryArguments = parse_arguments(QUERY, arguments)?;
        arguments.validate().map_err(|invalid| {
            ErrorData::invalid_params(
                format!("invalid {QUERY} arguments: {}", invalid.reason),
                Some(invalid.details),
            )
        })?;
        let result = BoundedQuery::new(self.state.clone())
            .run(caller, &arguments, &context.ct)
            .await
            .map(CallToolResult::structured);
        Ok(
            result
                .unwrap_or_else(|error| CallToolResult::structured_error(error.as_problem_json())),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{QUERY, descriptors, parse_arguments};
    use crate::query::collect::{
        DEFAULT_MAX_BYTES, DEFAULT_MAX_ROWS, MAX_BYTES_CEILING, MAX_ROWS_CEILING, MAX_SQL_BYTES,
        QueryArguments,
    };

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
            vec!["deadline_ms", "max_bytes", "max_rows", "sql"],
            "the query input is exactly the four closed fields"
        );

        assert_eq!(properties["sql"]["minLength"], serde_json::json!(1));
        assert_eq!(
            properties["sql"]["maxLength"],
            serde_json::json!(MAX_SQL_BYTES)
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
            "query_class",
            "path",
            "query_class",
            "visibility",
            "freshness",
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
        assert_eq!(request.deadline_ms, None);
        assert_eq!(defaults.max_rows, DEFAULT_MAX_ROWS);
        assert_eq!(defaults.max_bytes, DEFAULT_MAX_BYTES);

        let named = parse(serde_json::json!({
            "sql": "SELECT 1",
            "deadline_ms": u32::MAX,
            "max_rows": MAX_ROWS_CEILING,
            "max_bytes": MAX_BYTES_CEILING,
        }))
        .expect("every field at its bound parses");
        named.validate().expect("every field at its bound is valid");
        let request = named.to_request();
        assert_eq!(request.deadline_ms, Some(i64::from(u32::MAX)));

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
