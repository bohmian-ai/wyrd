//! Bifrost read/discovery MCP tools.
//!
//! Registers four read-only tools into the Skald tool registry:
//!
//! - `bifrost.list_tables` — lists the caller's tenant Bifrost tables;
//!   requires `bifrost_table:read` (enforced server-side).
//! - `bifrost.describe_table` — describes a single table's schema;
//!   requires `bifrost_table:read` (enforced server-side).
//! - `bifrost.list_permissions` — static catalog of the five Bifrost RBAC
//!   permissions; no bearer required.
//! - `bifrost.list_errors` — static catalog of every `WYRD_VALA_*_BIFROST_*`
//!   error code; no bearer required.
//!
//! Write tools are not available in this module. This module never imports
//! `vala-bifrost`, `sqlx`, or `datafusion` — no engine in the MCP process.

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Method;
use serde_json::{Value, json};
use skald_tool::{AgentTool, ToolError, ToolRegistry};
use wyrd_client::WyrdClient;
use wyrd_runtime::Permission;
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::api::{
    BifrostErrorDescriptor, BifrostPermissionDescriptor, BifrostTableDescription, BifrostTableEntry,
};

// ── list_tables ─────────────────────────────────────────────────────────────

struct ListTablesTool {
    client: WyrdClient,
}

#[async_trait]
impl AgentTool for ListTablesTool {
    fn name(&self) -> &str {
        "bifrost.list_tables"
    }

    fn description(&self) -> &str {
        "List all Bifrost tables visible to the caller's tenant. \
         Returns BifrostTableEntry objects including the universal card_ref correlation column. \
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
    client: WyrdClient,
}

#[async_trait]
impl AgentTool for DescribeTableTool {
    fn name(&self) -> &str {
        "bifrost.describe_table"
    }

    fn description(&self) -> &str {
        "Describe a single Bifrost table's schema. Returns BifrostTableDescription \
         including user columns and the universal card_ref/run_id correlation columns. \
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

// ── Registration ─────────────────────────────────────────────────────────────

/// Register the four Bifrost read/discovery tools into `registry`.
///
/// The two client-backed tools (`list_tables`, `describe_table`) use the
/// provided [`WyrdClient`] to call the Bifrost routes under the caller's bearer.
/// The two static tools (`list_permissions`, `list_errors`) need no bearer.
///
/// # Errors
/// Returns [`ToolError::NameTaken`] if any tool name is already registered.
pub fn register_bifrost_tools(
    registry: &ToolRegistry,
    client: WyrdClient,
) -> Result<(), ToolError> {
    registry.register(Arc::new(ListTablesTool {
        client: client.clone(),
    }))?;
    registry.register(Arc::new(DescribeTableTool { client }))?;
    registry.register(Arc::new(ListPermissionsTool))?;
    registry.register(Arc::new(ListErrorsTool))?;
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
/// compile-time exhaustiveness tripwire, not on the golden.
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
/// The `match` below is a compile-time exhaustiveness tripwire: adding a
/// `BifrostError` variant fails to compile until it is listed here, which is
/// the reminder to add it to the catalog above. Keep the `vec!` and the `match`
/// arms in sync.
fn bifrost_error_variants() -> Vec<BifrostError> {
    let variants = vec![
        BifrostError::ReservedColumn {
            column: String::new(),
        },
        BifrostError::MissingTenantColumn {
            table: String::new(),
        },
        BifrostError::UnexpectedTenantColumn {
            table: String::new(),
        },
        BifrostError::TenantBindingMissing,
        BifrostError::CardScopeDenied {
            card_ref: String::new(),
        },
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
    ];

    if let Some(sentinel) = variants.first() {
        match sentinel {
            BifrostError::ReservedColumn { .. }
            | BifrostError::MissingTenantColumn { .. }
            | BifrostError::UnexpectedTenantColumn { .. }
            | BifrostError::TenantBindingMissing
            | BifrostError::CardScopeDenied { .. }
            | BifrostError::TableNotFound { .. }
            | BifrostError::FingerprintMismatch { .. }
            | BifrostError::CommitConflict { .. }
            | BifrostError::DuplicateFailedBatch { .. }
            | BifrostError::MetadataMismatch { .. }
            | BifrostError::CatalogUnreachable { .. }
            | BifrostError::StorageUnreachable { .. }
            | BifrostError::WriterUnavailable { .. }
            | BifrostError::QueryInvalidSql { .. }
            | BifrostError::QueryTimeout
            | BifrostError::QueryResultTooLarge
            | BifrostError::Internal { .. }
            | BifrostError::AuditUnavailable { .. }
            | BifrostError::SchemaDrift { .. }
            | BifrostError::PhysicalDrift { .. }
            | BifrostError::IcebergMissing { .. }
            | BifrostError::RedactionFailed(..) => {}
        }
    }

    variants
}
