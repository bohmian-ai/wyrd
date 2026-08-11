//! Bifrost read/discovery MCP tools.
//!
//! Registers five read-only tools into the Skald tool registry:
//!
//! - `bifrost.list_tables` — lists the caller's tenant Bifrost tables;
//!   requires `bifrost_table:read` (enforced server-side).
//! - `bifrost.describe_table` — describes a single table's schema;
//!   requires `bifrost_table:read` (enforced server-side).
//! - `bifrost.list_permissions` — static catalog of the five Bifrost RBAC
//!   permissions; no bearer required.
//! - `bifrost.list_errors` — static catalog of every `WYRD_VALA_*_BIFROST_*`
//!   error code; no bearer required.
//! - `bifrost.query` — bounded terminal-safe Oracle query results; requires
//!   `bifrost_query:read` (enforced server-side).
//!
//! Write tools are not available in this module. This module never imports
//! `vala-bifrost`, `sqlx`, or `datafusion` — no engine in the MCP process.

use std::sync::Arc;

use arrow::json::ArrayWriter;
use async_trait::async_trait;
use reqwest::Method;
use serde_json::{Value, json};
use skald_tool::{AgentTool, StructuredInvocationError, ToolError, ToolRegistry};
use vala_sdk::{CollectedQueryLimits, QueryClient};
use wyrd_client::WyrdClient;
use wyrd_runtime::Permission;
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::api::{
    BifrostErrorDescriptor, BifrostPermissionDescriptor, BifrostQueryRequest,
    BifrostTableDescription, BifrostTableEntry, FreshnessPolicy, VisibilityMode,
};

// ── list_tables ─────────────────────────────────────────────────────────────

struct ListTablesTool {
    client: Arc<WyrdClient>,
}

#[async_trait]
impl AgentTool for ListTablesTool {
    fn name(&self) -> &str {
        "bifrost.list_tables"
    }

    fn description(&self) -> &str {
        "List all Bifrost tables visible to the caller's tenant. \
         Returns BifrostTableEntry objects including the universal card_uid correlation column. \
         Requires bifrost_table:read permission (server-enforced)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }

    fn output_schema(&self) -> Value {
        json!({"type": "array", "items": {"type": "object"}})
    }

    async fn invoke(&self, _args: Value) -> Result<Value, ToolError> {
        let entries: Vec<BifrostTableEntry> = self
            .client
            .request_json(Method::GET, "/v1/bifrost/tables", None::<&()>)
            .await
            .map_err(|e| ToolError::Invocation {
                detail: e.to_string(),
                cause: None,
            })?;
        serde_json::to_value(entries).map_err(|e| ToolError::OutputSerialization(e.to_string()))
    }
}

// ── describe_table ───────────────────────────────────────────────────────────

struct DescribeTableTool {
    client: Arc<WyrdClient>,
}

#[async_trait]
impl AgentTool for DescribeTableTool {
    fn name(&self) -> &str {
        "bifrost.describe_table"
    }

    fn description(&self) -> &str {
        "Describe a single Bifrost table's schema. Returns BifrostTableDescription \
         including user columns and the universal card_uid/run_id/principal_id correlation columns. \
         System columns (wyrd_*, data_tenant_id) are excluded. \
         Requires bifrost_table:read permission (server-enforced)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["namespace", "name"],
            "properties": {
                "namespace": {
                    "type": "string",
                    "description": "Bifrost namespace (e.g. 'bifrost')"
                },
                "name": {
                    "type": "string",
                    "description": "Table name within the namespace"
                }
            },
            "additionalProperties": false
        })
    }

    fn output_schema(&self) -> Value {
        json!({"type": "object"})
    }

    async fn invoke(&self, args: Value) -> Result<Value, ToolError> {
        let namespace = args
            .get("namespace")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing field: namespace".into()))?;
        let name = args
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing field: name".into()))?;
        let path = format!("/v1/bifrost/tables/{namespace}/{name}");
        let desc: BifrostTableDescription = self
            .client
            .request_json(Method::GET, &path, None::<&()>)
            .await
            .map_err(|e| ToolError::Invocation {
                detail: e.to_string(),
                cause: None,
            })?;
        serde_json::to_value(desc).map_err(|e| ToolError::OutputSerialization(e.to_string()))
    }
}

// ── list_permissions ─────────────────────────────────────────────────────────

struct ListPermissionsTool;

#[async_trait]
impl AgentTool for ListPermissionsTool {
    fn name(&self) -> &str {
        "bifrost.list_permissions"
    }

    fn description(&self) -> &str {
        "List all Bifrost RBAC permissions. Returns the five Bifrost-scoped permissions \
         (bifrost_table:read/write/install, bifrost_record:write, bifrost_query:read). \
         No authentication required."
    }

    fn input_schema(&self) -> Value {
        json!({"type": "object", "properties": {}, "additionalProperties": false})
    }

    fn output_schema(&self) -> Value {
        json!({"type": "array", "items": {"type": "object"}})
    }

    async fn invoke(&self, _args: Value) -> Result<Value, ToolError> {
        let perms = bifrost_permissions();
        serde_json::to_value(perms).map_err(|e| ToolError::OutputSerialization(e.to_string()))
    }
}

// ── list_errors ──────────────────────────────────────────────────────────────

struct ListErrorsTool;

#[async_trait]
impl AgentTool for ListErrorsTool {
    fn name(&self) -> &str {
        "bifrost.list_errors"
    }

    fn description(&self) -> &str {
        "List all Bifrost error codes with their HTTP status, title, and remediation guidance. \
         Includes WYRD_VALA_500_AUDIT_UNAVAILABLE. \
         No authentication required."
    }

    fn input_schema(&self) -> Value {
        json!({"type": "object", "properties": {}, "additionalProperties": false})
    }

    fn output_schema(&self) -> Value {
        json!({"type": "array", "items": {"type": "object"}})
    }

    async fn invoke(&self, _args: Value) -> Result<Value, ToolError> {
        let errors = bifrost_error_catalog();
        serde_json::to_value(errors).map_err(|e| ToolError::OutputSerialization(e.to_string()))
    }
}

// ── query ───────────────────────────────────────────────────────────────────

/// Configured defaults and non-negotiable MCP result ceilings.
#[derive(Debug, Clone, Copy)]
pub struct McpQueryLimits {
    /// Default maximum decoded rows when the caller omits a lower ceiling.
    pub default_max_rows: usize,
    /// Absolute maximum decoded rows accepted from any caller.
    pub hard_max_rows: usize,
    /// Default maximum encoded Arrow bytes.
    pub default_max_bytes: usize,
    /// Absolute maximum encoded Arrow bytes accepted from any caller.
    pub hard_max_bytes: usize,
}

impl Default for McpQueryLimits {
    /// Uses conservative MCP defaults and hard response ceilings.
    fn default() -> Self {
        Self {
            default_max_rows: 1_000,
            hard_max_rows: 10_000,
            default_max_bytes: 4 * 1024 * 1024,
            hard_max_bytes: 16 * 1024 * 1024,
        }
    }
}

/// Bounded read tool that delegates all query and terminal semantics to Vala.
struct BifrostQueryTool {
    /// Shared registration-owned authenticated Wyrd client.
    client: Arc<WyrdClient>,
    /// MCP-specific collection ceilings.
    limits: McpQueryLimits,
}

#[async_trait]
impl AgentTool for BifrostQueryTool {
    fn name(&self) -> &str {
        "bifrost.query"
    }

    fn description(&self) -> &str {
        "Run a bounded SELECT-only Oracle query and return typed JSON rows plus \
         validated terminal metadata. Requires bifrost_query:read permission."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["sql"],
            "properties": {
                "sql": {"type": "string"},
                "visibility": {
                    "enum": ["published_only", "fused"],
                    "default": "published_only"
                },
                "freshness": {
                    "enum": ["strict", "allow_degraded"],
                    "default": "strict"
                },
                "max_rows": {"type": "integer", "minimum": 1},
                "max_bytes": {"type": "integer", "minimum": 1}
            },
            "additionalProperties": false
        })
    }

    /// Describes the closed successful query result and structured invocation error.
    ///
    /// Successful output always contains exactly `schema`, `rows`, and `terminal`.
    /// The terminal object carries the complete source outcome even when no rows
    /// were returned, while `x-wyrd-error` describes failures raised before a
    /// contract-valid terminal can be collected.
    fn output_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["schema", "rows", "terminal"],
            "additionalProperties": false,
            "x-wyrd-error": {
                "type": "object",
                "required": ["code", "status", "title", "detail", "remediation", "safe_details"],
                "additionalProperties": false,
                "properties": {
                    "code": {"type": "string"},
                    "status": {"type": "integer", "minimum": 100, "maximum": 599},
                    "title": {"type": "string"},
                    "detail": {"type": "string"},
                    "remediation": {"type": "string"},
                    "safe_details": {"type": ["object", "array", "string", "number", "boolean", "null"]}
                }
            },
            "properties": {
                "schema": {
                    "type": "object",
                    "required": ["fields"],
                    "additionalProperties": false,
                    "properties": {
                        "fields": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "required": ["name", "data_type", "nullable"],
                                "additionalProperties": false,
                                "properties": {
                                    "name": {"type": "string"},
                                    "data_type": {"type": "string"},
                                    "nullable": {"type": "boolean"}
                                }
                            }
                        }
                    }
                },
                "rows": {"type": "array", "items": {"type": "object", "additionalProperties": true}},
                "terminal": {
                    "type": "object",
                    "required": ["outcome", "freshness", "row_count", "warnings", "source_completion", "error"],
                    "additionalProperties": false,
                    "properties": {
                        "outcome": {"enum": ["success", "degraded", "failed"]},
                        "freshness": {"enum": ["complete", "degraded"]},
                        "row_count": {"type": "integer", "minimum": 0},
                        "warnings": {"type": "array", "items": {"enum": ["live_tail_unavailable", "stale_cut_replanned"]}},
                        "source_completion": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "required": ["source", "outcome"],
                                "additionalProperties": false,
                                "properties": {
                                    "source": {"enum": ["iceberg", "hot_sealed", "live_tail"]},
                                    "outcome": {"enum": ["complete", "unavailable"]}
                                }
                            }
                        },
                        "error": {
                            "type": ["object", "null"],
                            "required": ["code", "detail"],
                            "additionalProperties": false,
                            "properties": {
                                "code": {"enum": [
                                    "query_timeout", "query_visibility_unavailable", "query_tenant_invariant",
                                    "query_reconciliation_invariant", "query_peer_security", "query_audit_unavailable",
                                    "catalog_unreachable", "storage_unreachable", "query_execution_failed"
                                ]},
                                "detail": {"type": ["string", "null"]}
                            }
                        }
                    }
                }
            }
        })
    }

    /// Executes one bounded Oracle query and projects its terminal-aware result.
    ///
    /// The workflow validates the MCP arguments, applies hard row and encoded-byte
    /// ceilings, delegates planning and execution to the authenticated Vala client,
    /// validates the required schema and terminal frames, and only then serializes
    /// the authoritative schema plus any Arrow rows. Zero-row success therefore
    /// retains the real schema without fabricating a batch.
    /// Cancellation drops the in-flight client future; rows already collected are
    /// not returned without a validated terminal, so callers never observe partial
    /// success.
    ///
    /// # Errors
    ///
    /// Returns invalid-argument errors for malformed query policies or bounds,
    /// structured invocation errors for authorization, query-floor, admission,
    /// catalog, storage, timeout, terminal, or incomplete-stream failures, and
    /// output-serialization errors when an Arrow batch cannot be encoded as JSON.
    async fn invoke(&self, args: Value) -> Result<Value, ToolError> {
        let sql = required_string(&args, "sql")?;
        let request = BifrostQueryRequest {
            sql: sql.to_owned(),
            visibility: parse_visibility(args.get("visibility"))?,
            freshness: parse_freshness(args.get("freshness"))?,
            deadline_ms: None,
        };
        let limits = requested_limits(&args, self.limits)?;
        let result = QueryClient::new(self.client.as_ref())
            .collect_bounded(&request, limits)
            .await
            .map_err(query_tool_error)?;
        let schema = json!({
            "fields": result.schema.fields().iter().map(|field| json!({
                "name": field.name(),
                "data_type": field.data_type().to_string(),
                "nullable": field.is_nullable()
            })).collect::<Vec<_>>()
        });
        let mut writer = ArrayWriter::new(Vec::new());
        for batch in &result.batches {
            writer
                .write(batch)
                .map_err(|error| ToolError::OutputSerialization(error.to_string()))?;
        }
        writer
            .finish()
            .map_err(|error| ToolError::OutputSerialization(error.to_string()))?;
        let rows: Value = serde_json::from_slice(&writer.into_inner())
            .map_err(|error| ToolError::OutputSerialization(error.to_string()))?;
        Ok(json!({
            "schema": schema,
            "rows": rows,
            "terminal": result.terminal
        }))
    }
}

/// Maps one Vala SDK failure into the structured MCP invocation envelope.
///
/// The stable fields come directly from the SDK accessors. Transport details
/// use the Wyrd problem payload and terminal details use the already scrubbed
/// terminal object; no machine-readable field is recovered from formatted text.
fn query_tool_error(error: vala_sdk::ValaSdkError) -> ToolError {
    let (detail, safe_details) = match &error {
        vala_sdk::ValaSdkError::Transport(source) => {
            let problem = source.as_problem_json();
            (
                problem
                    .get("detail")
                    .and_then(Value::as_str)
                    .unwrap_or("query transport failed")
                    .to_owned(),
                problem.get("details").cloned(),
            )
        }
        vala_sdk::ValaSdkError::Protocol(detail) | vala_sdk::ValaSdkError::Arrow(detail) => {
            (detail.clone(), None)
        }
        vala_sdk::ValaSdkError::IncompleteQueryStream => (
            "query stream ended before its required terminal frame".to_owned(),
            None,
        ),
        vala_sdk::ValaSdkError::FailedTerminal { terminal } => (
            terminal
                .error
                .as_ref()
                .and_then(|error| error.detail.as_ref())
                .map_or_else(
                    || "query terminal reported failure".to_owned(),
                    |detail| detail.as_str().to_owned(),
                ),
            serde_json::to_value(terminal).ok(),
        ),
        vala_sdk::ValaSdkError::ResultTooLarge => {
            ("query result exceeds configured bounds".to_owned(), None)
        }
    };
    ToolError::StructuredInvocation(Box::new(StructuredInvocationError {
        code: error.code().to_owned(),
        status: error.status(),
        title: error.title().to_owned(),
        detail,
        remediation: error.remediation().to_owned(),
        safe_details,
    }))
}

/// Extracts one required string argument.
///
/// # Errors
///
/// Returns invalid input when the field is absent or not a string.
fn required_string<'a>(args: &'a Value, field: &str) -> Result<&'a str, ToolError> {
    args.get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::InvalidInput(format!("missing field: {field}")))
}

/// Parses the optional closed visibility spelling.
///
/// # Errors
///
/// Returns invalid input for an unknown or non-string value.
fn parse_visibility(value: Option<&Value>) -> Result<VisibilityMode, ToolError> {
    match value.and_then(Value::as_str).unwrap_or("published_only") {
        "published_only" => Ok(VisibilityMode::PublishedOnly),
        "fused" => Ok(VisibilityMode::Fused),
        _ => Err(ToolError::InvalidInput(
            "visibility must be published_only or fused".to_owned(),
        )),
    }
}

/// Parses the optional closed freshness spelling.
///
/// # Errors
///
/// Returns invalid input for an unknown or non-string value.
fn parse_freshness(value: Option<&Value>) -> Result<FreshnessPolicy, ToolError> {
    match value.and_then(Value::as_str).unwrap_or("strict") {
        "strict" => Ok(FreshnessPolicy::Strict),
        "allow_degraded" => Ok(FreshnessPolicy::AllowDegraded),
        _ => Err(ToolError::InvalidInput(
            "freshness must be strict or allow_degraded".to_owned(),
        )),
    }
}

/// Resolves caller ceilings without allowing either hard limit to be raised.
///
/// # Errors
///
/// Returns invalid input for zero, non-integer, platform-overflowing, or
/// above-hard-ceiling bounds.
fn requested_limits(
    args: &Value,
    configured: McpQueryLimits,
) -> Result<CollectedQueryLimits, ToolError> {
    fn bound(args: &Value, name: &str, default: usize, hard: usize) -> Result<usize, ToolError> {
        let Some(value) = args.get(name) else {
            return Ok(default);
        };
        let value = value
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value > 0 && *value <= hard)
            .ok_or_else(|| {
                ToolError::InvalidInput(format!("{name} must be between 1 and {hard}"))
            })?;
        Ok(value)
    }
    Ok(CollectedQueryLimits {
        max_rows: bound(
            args,
            "max_rows",
            configured.default_max_rows,
            configured.hard_max_rows,
        )?,
        max_encoded_bytes: bound(
            args,
            "max_bytes",
            configured.default_max_bytes,
            configured.hard_max_bytes,
        )?,
    })
}

// ── Registration ─────────────────────────────────────────────────────────────

/// Register the five Bifrost read/discovery tools into `registry`.
///
/// The two client-backed tools (`list_tables`, `describe_table`) use the
/// provided [`WyrdClient`] to call the Bifrost routes under the caller's bearer.
/// The two static tools (`list_permissions`, `list_errors`) need no bearer.
///
/// # Errors
/// Returns [`ToolError::NameTaken`] if any tool name is already registered.
pub fn register_bifrost_tools(
    registry: &ToolRegistry,
    client: Arc<WyrdClient>,
) -> Result<(), ToolError> {
    registry.register(Arc::new(ListTablesTool {
        client: Arc::clone(&client),
    }))?;
    registry.register(Arc::new(DescribeTableTool {
        client: Arc::clone(&client),
    }))?;
    registry.register(Arc::new(ListPermissionsTool))?;
    registry.register(Arc::new(ListErrorsTool))?;
    registry.register(Arc::new(BifrostQueryTool {
        client,
        limits: McpQueryLimits::default(),
    }))?;
    Ok(())
}

// ── Static catalogs (one source, two consumers) ──────────────────────────────

/// The five Bifrost RBAC permissions — projected from the `wyrd-runtime`
/// permission constructors (the single enforced source), read by
/// `bifrost.list_permissions`. Each entry corresponds to a `Permission`
/// constructor the server actually checks; there is no `bifrost_query:run`
/// (`BifrostQuery` only pairs with `Read`), so it is not surfaced —
/// projecting an unenforced permission would be a second source of truth.
pub fn bifrost_permissions() -> Vec<BifrostPermissionDescriptor> {
    let perms = [
        Permission::bifrost_table_read(),
        Permission::bifrost_table_write(),
        Permission::bifrost_table_install(),
        Permission::bifrost_record_write(),
        Permission::bifrost_query_read(),
    ];
    perms
        .into_iter()
        .map(|p| {
            let s = p.to_string();
            let (resource, action) = s
                .split_once(':')
                .expect("permission Display is always resource:action");
            BifrostPermissionDescriptor {
                permission: s.clone(),
                resource: resource.to_owned(),
                action: action.to_owned(),
            }
        })
        .collect()
}

/// The full Bifrost error catalog — read by `bifrost.list_errors`.
///
/// Every descriptor is **projected** from [`BifrostError`] via the `WyrdError`
/// derive accessors (`code`/`status`/`title`/`remediation`); the metadata is
/// never re-typed here, so the catalog cannot drift from the error
/// definitions. The `codegen:check` snapshot covers only the descriptor
/// *schema* (via `write::<BifrostErrorDescriptor>` in gen_schemas), not the
/// catalog contents — so completeness rests on [`bifrost_error_variants`]'s
/// explicit variant list and coverage test, not on the golden.
pub fn bifrost_error_catalog() -> Vec<BifrostErrorDescriptor> {
    bifrost_error_variants()
        .iter()
        .map(|e| BifrostErrorDescriptor {
            code: e.code().to_owned(),
            status: e.status(),
            title: e.title().to_owned(),
            remediation: e.remediation().to_owned(),
        })
        .collect()
}

/// One representative of every [`BifrostError`] variant, used only to project
/// the derive metadata into the catalog (field values are placeholders — the
/// accessors read the `#[wyrd_error]` attributes, not the instance data).
///
/// The catalog test below keeps this list aligned with the `BifrostError` enum.
fn bifrost_error_variants() -> Vec<BifrostError> {
    let variants = vec![
        BifrostError::OracleRoleUnavailable,
        BifrostError::ScribeRoleUnavailable,
        BifrostError::QueryAdmissionRejected,
        BifrostError::QueryMemoryRequestTooLarge,
        BifrostError::QueryVisibilityUnavailable,
        BifrostError::QueryTenantInvariant,
        BifrostError::QueryReconciliationInvariant,
        BifrostError::QueryPeerSecurity,
        BifrostError::QueryStreamProtocol,
        BifrostError::QueryStreamIncomplete,
        BifrostError::QueryAuditUnavailable,
        BifrostError::QueryExecutionFailed,
        BifrostError::EventTimeOutOfRange {
            value: String::new(),
            past_bound: String::new(),
            future_bound: String::new(),
        },
        BifrostError::IngestAuthentication {
            message: String::new(),
        },
        BifrostError::IngestProtocol {
            message: String::new(),
        },
        BifrostError::ReservedColumn {
            column: String::new(),
        },
        BifrostError::ReservedBuiltinWriteDenied {
            table: String::new(),
        },
        BifrostError::TenantIsolationColumnMissing {
            table: String::new(),
        },
        BifrostError::TenantBindingMissing,
        BifrostError::CardScopeDenied {
            card_ref: String::new(),
        },
        BifrostError::CardUnresolved {
            card_ref: String::new(),
        },
        BifrostError::PrincipalUnresolved,
        BifrostError::TableNotFound {
            table: String::new(),
        },
        BifrostError::FingerprintMismatch {
            table: String::new(),
        },
        BifrostError::CommitConflict {
            table: String::new(),
        },
        BifrostError::DuplicateFailedBatch {
            batch_id: String::new(),
        },
        BifrostError::MetadataMismatch {
            detail: String::new(),
        },
        BifrostError::CatalogUnreachable {
            detail: String::new(),
        },
        BifrostError::StorageUnreachable {
            detail: String::new(),
        },
        BifrostError::WriterUnavailable {
            table: String::new(),
        },
        BifrostError::QueryInvalidSql {
            detail: String::new(),
        },
        BifrostError::QueryTimeout,
        BifrostError::QueryResultTooLarge,
        BifrostError::PayloadTooLarge { bytes: 0 },
        BifrostError::IngestOversized { rows: 0, limit: 0 },
        BifrostError::Internal {
            detail: String::new(),
        },
        BifrostError::AuditUnavailable {
            detail: String::new(),
        },
        BifrostError::SchemaDrift {
            detail: String::new(),
        },
        BifrostError::PhysicalDrift {
            detail: String::new(),
        },
        BifrostError::IcebergMissing {
            detail: String::new(),
        },
        BifrostError::RedactionFailed(String::new()),
        BifrostError::TraceNotFound {
            trace_id: String::new(),
        },
        BifrostError::WindowRequired,
        BifrostError::FilterInvalid {
            detail: String::new(),
        },
        BifrostError::PageTokenInvalid,
        BifrostError::PageSnapshotExpired,
        BifrostError::QueryForbidden,
        BifrostError::PayloadForbidden,
        BifrostError::IngestBusy {
            table: String::new(),
        },
        BifrostError::WalDiskFull,
        BifrostError::OtlpRequestMalformed {
            table: String::new(),
            detail: String::new(),
        },
        BifrostError::StreamMismatch {
            requested: String::new(),
            actual: String::new(),
        },
    ];

    variants
}

/// Tests verifying Bifrost tool registration and static catalog tools.
///
/// Static tools (list_permissions, list_errors) are invoked without a server —
/// no bearer required, no DB required.
#[cfg(test)]
mod bifrost_tools {
    use std::sync::Arc;

    use serde_json::{Value, json};
    use skald_tool::{ToolError, ToolRegistry};
    use strum::EnumCount;
    use vala_sdk::ValaSdkError;
    use wyrd_client::{WyrdClient, config::ClientConfig};
    use wyrd_spec::error::WyrdError;
    use wyrd_spec::vala::api::{
        QueryErrorDetail, QueryFreshness, QuerySource, QueryTerminalError, QueryTerminalErrorCode,
        QueryTerminalFrame, QueryTerminalOutcome, QueryWarning, SourceCompletion,
        SourceCompletionOutcome,
    };
    use wyrd_spec::vala::error::BifrostError;

    use crate::bifrost::{
        McpQueryLimits, bifrost_error_catalog, bifrost_permissions, parse_freshness,
        parse_visibility, query_tool_error, register_bifrost_tools, requested_limits,
    };

    fn dummy_client() -> WyrdClient {
        let config = ClientConfig {
            api_key: Some("test_key_placeholder".to_owned().into()),
            ..ClientConfig::default()
        };
        WyrdClient::with_config(config).expect("test client builds")
    }

    // ── Registration ─────────────────────────────────────────────────────────────

    #[test]
    fn bifrost_tools_register_five_tools() {
        let registry = ToolRegistry::new();
        let client = dummy_client();
        register_bifrost_tools(&registry, Arc::new(client)).expect("registration succeeds");

        let names = registry.names();
        assert_eq!(names.len(), 5, "expected exactly 5 tools, got {names:?}");
        assert!(names.contains(&"bifrost.list_tables".to_string()));
        assert!(names.contains(&"bifrost.describe_table".to_string()));
        assert!(names.contains(&"bifrost.list_permissions".to_string()));
        assert!(names.contains(&"bifrost.list_errors".to_string()));
        assert!(names.contains(&"bifrost.query".to_string()));
    }

    /// Proves caller-supplied MCP query bounds cannot exceed hard ceilings.
    #[test]
    fn bifrost_query_rejects_above_hard_bounds() {
        let limits = McpQueryLimits::default();
        assert!(requested_limits(&json!({"max_rows": limits.hard_max_rows + 1}), limits).is_err());
        assert!(
            requested_limits(&json!({"max_bytes": limits.hard_max_bytes + 1}), limits).is_err()
        );
    }

    /// Proves omitted MCP query bounds use conservative defaults.
    #[test]
    fn bifrost_query_uses_default_bounds() {
        let configured = McpQueryLimits::default();
        let actual = requested_limits(&json!({}), configured).expect("defaults are valid");
        assert_eq!(actual.max_rows, configured.default_max_rows);
        assert_eq!(actual.max_encoded_bytes, configured.default_max_bytes);
    }

    /// Omitted MCP policy matches every other first-class client boundary.
    #[test]
    fn bifrost_query_uses_published_strict_defaults_and_explicit_opt_in() {
        assert_eq!(
            parse_visibility(None).expect("visibility default is valid"),
            wyrd_spec::vala::api::VisibilityMode::PublishedOnly
        );
        assert_eq!(
            parse_freshness(None).expect("freshness default is valid"),
            wyrd_spec::vala::api::FreshnessPolicy::Strict
        );
        assert_eq!(
            parse_visibility(Some(&json!("fused"))).expect("fused is explicit"),
            wyrd_spec::vala::api::VisibilityMode::Fused
        );
        assert_eq!(
            parse_freshness(Some(&json!("allow_degraded"))).expect("degraded is explicit"),
            wyrd_spec::vala::api::FreshnessPolicy::AllowDegraded
        );

        let registry = ToolRegistry::new();
        register_bifrost_tools(&registry, Arc::new(dummy_client())).expect("tools register");
        let schema = registry
            .resolve("bifrost.query")
            .expect("query tool registered")
            .input_schema();
        assert_eq!(
            schema["properties"]["visibility"]["default"],
            "published_only"
        );
        assert_eq!(schema["properties"]["freshness"]["default"], "strict");
    }

    /// The query tool publishes a closed terminal and structured error shape.
    #[test]
    fn bifrost_query_schema_is_closed_and_terminal_typed() {
        let registry = ToolRegistry::new();
        register_bifrost_tools(&registry, Arc::new(dummy_client())).expect("registration succeeds");
        let schema = registry
            .resolve("bifrost.query")
            .expect("query tool registered")
            .output_schema();
        let terminal = &schema["properties"]["terminal"];
        assert_eq!(terminal["additionalProperties"], false);
        assert_eq!(
            schema["properties"]["schema"]["additionalProperties"],
            false
        );
        assert_eq!(schema["properties"]["rows"]["items"]["type"], "object");
        assert_eq!(
            schema["properties"]["rows"]["items"]["additionalProperties"],
            true
        );
        assert_eq!(
            schema["x-wyrd-error"]["required"],
            json!([
                "code",
                "status",
                "title",
                "detail",
                "remediation",
                "safe_details"
            ])
        );
        assert_eq!(schema["x-wyrd-error"]["additionalProperties"], false);
        assert_eq!(
            terminal["required"],
            json!([
                "outcome",
                "freshness",
                "row_count",
                "warnings",
                "source_completion",
                "error"
            ])
        );
        assert_eq!(
            terminal["properties"]["outcome"]["enum"],
            json!(["success", "degraded", "failed"])
        );
        assert_eq!(
            terminal["properties"]["error"]["properties"]["code"]["enum"]
                .as_array()
                .map(Vec::len),
            Some(9)
        );
    }

    /// Assert one SDK failure projects every structured invocation field exactly.
    fn assert_structured_error(
        error: ValaSdkError,
        code: &str,
        status: u16,
        title: &str,
        detail: &str,
        remediation: &str,
        safe_details: Option<Value>,
    ) {
        let ToolError::StructuredInvocation(error) = query_tool_error(error) else {
            panic!("query SDK failure must map to structured invocation metadata");
        };
        assert_eq!(error.code, code);
        assert_eq!(error.status, status);
        assert_eq!(error.title, title);
        assert_eq!(error.detail, detail);
        assert_eq!(error.remediation, remediation);
        assert_eq!(error.safe_details, safe_details);
    }

    /// Incomplete and unavailable SDK failures retain exact recovery metadata.
    #[test]
    fn bifrost_query_error_mapping_preserves_incomplete_and_unavailable() {
        assert_structured_error(
            ValaSdkError::IncompleteQueryStream,
            "WYRD_VALA_502_QUERY_STREAM_INCOMPLETE",
            502,
            "Query stream incomplete",
            "query stream ended before its required terminal frame",
            "Retry the query because the response ended before its required terminal frame.",
            None,
        );
        let details = json!({"dependency": "catalog"});
        assert_structured_error(
            ValaSdkError::Transport(WyrdError::UpstreamFailure {
                message: "catalog unavailable".to_owned(),
                details: details.clone(),
            }),
            "WYRD_SPEC_502_UPSTREAM_FAILURE",
            502,
            "Upstream dependency failed",
            "catalog unavailable",
            "Check the upstream dependency health and retry policy.",
            Some(details),
        );
    }

    /// A pre-stream permanent capacity refusal retains its 422 descriptor.
    #[test]
    fn bifrost_query_tool_preserves_pre_stream_422_descriptor() {
        assert_structured_error(
            ValaSdkError::Transport(BifrostError::QueryMemoryRequestTooLarge.into()),
            "WYRD_VALA_422_QUERY_MEMORY_REQUEST_TOO_LARGE",
            422,
            "Query memory request too large",
            "query memory request too large",
            "Reduce the requested range or query memory footprint; retrying unchanged will not succeed.",
            Some(json!({
                "variant": "query_memory_request_too_large"
            })),
        );
    }

    /// Failed terminals retain scrubbed terminal details without flattening them.
    #[test]
    fn bifrost_query_error_mapping_preserves_failed_terminal_details() {
        let terminal = QueryTerminalFrame {
            outcome: QueryTerminalOutcome::Failed,
            freshness: QueryFreshness::Complete,
            row_count: 2,
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
            error: Some(QueryTerminalError {
                code: QueryTerminalErrorCode::QueryExecutionFailed,
                detail: Some(QueryErrorDetail::new("safe terminal detail").expect("detail valid")),
            }),
        };
        let expected = serde_json::to_value(&terminal).expect("terminal serializes");
        assert_structured_error(
            ValaSdkError::FailedTerminal { terminal },
            "WYRD_VALA_500_QUERY_EXECUTION_FAILED",
            500,
            "Query execution failed",
            "safe terminal detail",
            "Inspect the retained terminal error and correct the query or source failure before retrying.",
            Some(expected),
        );
    }

    /// Degraded terminals remain closed and explicit for agent branching.
    #[test]
    fn bifrost_degraded_terminal_retains_structured_fields() {
        let terminal = QueryTerminalFrame {
            outcome: QueryTerminalOutcome::Degraded,
            freshness: QueryFreshness::Degraded,
            row_count: 2,
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
        };
        let value = serde_json::to_value(terminal).expect("degraded terminal serializes");
        assert_eq!(value["outcome"], "degraded");
        assert_eq!(value["freshness"], "degraded");
        assert_eq!(value["warnings"], json!(["live_tail_unavailable"]));
        assert_eq!(value["error"], Value::Null);
    }

    #[test]
    fn bifrost_tools_duplicate_registration_returns_name_taken() {
        let registry = ToolRegistry::new();
        let client = dummy_client();
        register_bifrost_tools(&registry, Arc::new(client.clone()))
            .expect("first registration succeeds");
        let err = register_bifrost_tools(&registry, Arc::new(client)).expect_err("duplicate fails");
        let code = err.code();
        assert_eq!(code, "SKALD_TOOL_409_NAME_TAKEN");
    }

    // ── list_permissions (static, no bearer) ─────────────────────────────────────

    #[test]
    fn bifrost_tools_list_permissions_returns_five_entries() {
        let perms = bifrost_permissions();
        assert_eq!(
            perms.len(),
            5,
            "expected 5 Bifrost permissions, got {}",
            perms.len()
        );
    }

    #[test]
    fn bifrost_tools_list_permissions_covers_all_expected() {
        let perms = bifrost_permissions();
        let names: Vec<&str> = perms.iter().map(|p| p.permission.as_str()).collect();
        assert!(
            names.contains(&"bifrost_table:read"),
            "missing bifrost_table:read"
        );
        assert!(
            names.contains(&"bifrost_table:write"),
            "missing bifrost_table:write"
        );
        assert!(
            names.contains(&"bifrost_table:install"),
            "missing bifrost_table:install"
        );
        assert!(
            names.contains(&"bifrost_record:write"),
            "missing bifrost_record:write"
        );
        assert!(
            names.contains(&"bifrost_query:read"),
            "missing bifrost_query:read"
        );
        // No bifrost_query:run — BifrostQuery only pairs with Read; nothing enforces
        // a `run` action, so projecting it would be a second source of truth.
        assert!(
            !names.contains(&"bifrost_query:run"),
            "bifrost_query:run is not an enforced permission"
        );
    }

    #[test]
    fn bifrost_tools_list_permissions_invoke_returns_array() {
        wyrd_runtime::runtime().block_on(async {
            let registry = ToolRegistry::new();
            let client = dummy_client();
            register_bifrost_tools(&registry, Arc::new(client)).expect("registration succeeds");
            let tool = registry
                .resolve("bifrost.list_permissions")
                .expect("tool is registered");
            let result = tool.invoke(json!({})).await.expect("invoke succeeds");
            let arr = result.as_array().expect("output is JSON array");
            assert_eq!(arr.len(), 5);
        });
    }

    // ── list_errors (static, no bearer) ──────────────────────────────────────────

    #[test]
    fn bifrost_tools_list_errors_covers_c1_c2_c5_codes() {
        let catalog = bifrost_error_catalog();
        let codes: Vec<&str> = catalog.iter().map(|e| e.code.as_str()).collect();

        // C5: WYRD_VALA_500_AUDIT_UNAVAILABLE must be present.
        assert!(
            codes.contains(&"WYRD_VALA_500_AUDIT_UNAVAILABLE"),
            "C5 code missing from error catalog"
        );
        // A representative C2 code.
        assert!(
            codes.contains(&"WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND"),
            "C2 code missing from error catalog"
        );
        // A representative C1 code (schema fingerprint).
        assert!(
            codes.contains(&"WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH"),
            "C1 code missing from error catalog"
        );
        assert!(
            codes.contains(&"WYRD_VALA_409_STREAM_MISMATCH"),
            "stream mismatch code missing from error catalog"
        );
        assert!(
            codes.contains(&"WYRD_VALA_507_WAL_DISK_FULL"),
            "WAL disk full code missing from error catalog"
        );
    }

    /// The public catalog includes the permanent query-memory descriptor.
    #[test]
    fn bifrost_tools_list_errors_includes_query_memory_request_too_large() {
        let descriptor = bifrost_error_catalog()
            .into_iter()
            .find(|entry| entry.code == "WYRD_VALA_422_QUERY_MEMORY_REQUEST_TOO_LARGE")
            .expect("query memory descriptor");
        assert_eq!(descriptor.status, 422);
        assert!(!descriptor.remediation.contains("Retry after"));
    }

    #[test]
    fn bifrost_error_variants_covers_all() {
        assert_eq!(
            super::bifrost_error_variants().len(),
            BifrostError::COUNT,
            "bifrost_error_variants() must list every BifrostError variant"
        );
    }

    #[test]
    fn bifrost_tools_list_errors_all_entries_have_nonempty_fields() {
        for entry in bifrost_error_catalog() {
            assert!(!entry.code.is_empty(), "code must not be empty");
            assert!(entry.status > 0, "status must be nonzero");
            assert!(!entry.title.is_empty(), "title must not be empty");
            assert!(
                !entry.remediation.is_empty(),
                "remediation must not be empty"
            );
        }
    }

    #[test]
    fn bifrost_tools_list_errors_invoke_returns_array() {
        wyrd_runtime::runtime().block_on(async {
            let registry = ToolRegistry::new();
            let client = dummy_client();
            register_bifrost_tools(&registry, Arc::new(client)).expect("registration succeeds");
            let tool = registry
                .resolve("bifrost.list_errors")
                .expect("tool is registered");
            let result = tool.invoke(json!({})).await.expect("invoke succeeds");
            let arr = result.as_array().expect("output is JSON array");
            assert!(!arr.is_empty(), "error catalog must not be empty");
        });
    }
}
