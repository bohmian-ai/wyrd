//! Public Bifrost wire contracts — table management, query, and ingest types.
//!
//! This module is the C2 landing zone for the Bifrost HTTP register/insert and
//! query wire types. Every type here is Arrow-free and PyO3-free: the
//! `DataTypeSpec ↔ arrow::DataType` conversion and the schema-fingerprint
//! computation live in the server crate, never in `wyrd-spec`. All types derive
//! `schemars::JsonSchema` and, under the `server` feature, `utoipa::ToSchema`,
//! matching the sibling vala wire idiom.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::auth::{PrincipalId, PrincipalKindTag};
use crate::reference::CardRef;
use crate::request_id::RequestId;

/// Bifrost table-identifier newtype.
///
/// The canonical table name as it appears in the Iceberg catalog and on the
/// wire. Reused by the observation contract and the C2 register/insert wire
/// types.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct BifrostTableName(String);

impl BifrostTableName {
    /// Wraps a string as a [`BifrostTableName`].
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Borrows the table name as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for BifrostTableName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn default_true() -> bool {
    true
}

// ── Table registration / describe wire types ────────────────────────────────

/// Wire form of a Bifrost table scope. Distinct from the engine `TableScope` so
/// `wyrd-spec` carries no engine dependency; the engine maps between the two.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum TableScopeWire {
    /// Tenant-isolated by catalog namespace; carries no `data_tenant_id` column.
    #[default]
    TenantOwned,
    /// Cross-tenant physical table; rows carry a server-stamped `data_tenant_id`.
    SystemShared,
}

/// Lifecycle status of a registered Bifrost table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum TableStatus {
    /// Table is active and writable.
    Active,
    /// Table is deprecated but still readable.
    Deprecated,
    /// Table is quarantined and not writable.
    Quarantined,
}

/// Lightweight table listing entry — one row of the list-tables response.
///
/// Schema-free by design (review M-04): the stored schema is carried only by the
/// per-table describe contract, [`BifrostTableDescription`], so the list stays
/// small.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct BifrostTableEntry {
    /// Namespace component of the fully-qualified name (`<ns>.<name>` → ns).
    pub namespace: String,
    /// Name component of the fully-qualified name (`<ns>.<name>` → name).
    pub name: String,
    /// Lower-case hex of the 16-byte table uid.
    pub table_uid: String,
    /// Table scope (wire enum, not the engine type).
    pub scope: TableScopeWire,
    /// Lifecycle status.
    pub status: TableStatus,
    /// Lower-case hex of the 32-byte user-schema fingerprint.
    pub fingerprint: String,
    /// Declared partition columns.
    pub partition_columns: Vec<String>,
    /// Wall-clock registration time.
    pub registered_at: DateTime<Utc>,
    /// Wall-clock last-update time.
    pub updated_at: DateTime<Utc>,
}

/// Per-table describe response — the lightweight entry plus the stored schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct BifrostTableDescription {
    /// The same lightweight entry returned by the list route.
    pub entry: BifrostTableEntry,
    /// The stored schema as Arrow-free [`FieldSpec`]s. Includes the universal
    /// `card_ref`/`run_id` correlation columns (flagged in
    /// [`FieldSpec::metadata`]); the server-stamped `wyrd_*`/`data_tenant_id`
    /// system columns are excluded.
    pub fields: Vec<FieldSpec>,
}

// ── Arrow-free schema / field wire types ────────────────────────────────────

/// Timestamp / time precision. Arrow-free mirror of `arrow::datatypes::TimeUnit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum TimeUnit {
    /// Seconds.
    Second,
    /// Milliseconds.
    Millisecond,
    /// Microseconds — the canonical Bifrost physical timestamp precision.
    Microsecond,
    /// Nanoseconds — a caller may name it, but Bifrost coerces to microseconds.
    Nanosecond,
}

/// Arrow-free logical column type. The exhaustive set Bifrost accepts on the
/// register path; the `DataTypeSpec ↔ arrow::DataType` conversion lives in the
/// server/engine, never in `wyrd-spec`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum DataTypeSpec {
    /// Boolean.
    Bool,
    /// Signed 8-bit integer.
    Int8,
    /// Signed 16-bit integer.
    Int16,
    /// Signed 32-bit integer.
    Int32,
    /// Signed 64-bit integer.
    Int64,
    /// Unsigned 8-bit integer.
    UInt8,
    /// Unsigned 16-bit integer.
    UInt16,
    /// Unsigned 32-bit integer.
    UInt32,
    /// Unsigned 64-bit integer.
    UInt64,
    /// 32-bit float.
    Float32,
    /// 64-bit float.
    Float64,
    /// UTF-8 string.
    Utf8,
    /// Large UTF-8 string.
    LargeUtf8,
    /// Variable-length binary.
    Binary,
    /// Large variable-length binary.
    LargeBinary,
    /// Fixed-width binary.
    FixedSizeBinary {
        /// Byte width.
        len: i32,
    },
    /// 32-bit date (days since epoch).
    Date32,
    /// 64-bit date (milliseconds since epoch).
    Date64,
    /// Timestamp with explicit precision and optional timezone.
    Timestamp {
        /// Precision.
        unit: TimeUnit,
        /// IANA timezone string, or `None` for timezone-naive.
        tz: Option<String>,
    },
    /// Time-of-day at second/millisecond precision.
    Time32 {
        /// Precision.
        unit: TimeUnit,
    },
    /// Time-of-day at microsecond/nanosecond precision.
    Time64 {
        /// Precision.
        unit: TimeUnit,
    },
    /// 128-bit fixed-point decimal.
    Decimal128 {
        /// Total number of digits.
        precision: u8,
        /// Number of fractional digits.
        scale: i8,
    },
    /// Variable-length list of a single element type.
    List(Box<DataTypeSpec>),
    /// Nested struct of named fields.
    Struct(Vec<FieldSpec>),
}

/// Arrow-free field declaration. Follows the `card::field::FieldSpec` precedent
/// but carries a typed [`DataTypeSpec`] instead of a loose dtype string.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct FieldSpec {
    /// Column name.
    pub name: String,
    /// Logical column type.
    pub data_type: DataTypeSpec,
    /// Whether the column is nullable. Defaults to `true`.
    #[serde(default = "default_true")]
    pub nullable: bool,
    /// String metadata. Bifrost describe uses the `wyrd:column_class` key to
    /// flag `correlation` columns distinctly from user columns.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

/// Descriptor for one entry in the Bifrost error catalog.
///
/// Emitted by `gen_schemas` (Stage 3 C7) and returned by the
/// `bifrost.list_errors` MCP tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct BifrostErrorDescriptor {
    /// Stable machine-readable error code (e.g. `WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND`).
    pub code: String,
    /// HTTP status code associated with this error.
    pub status: u16,
    /// Short human-readable title.
    pub title: String,
    /// Actionable remediation guidance.
    pub remediation: String,
}

/// Descriptor for one Bifrost RBAC permission.
///
/// Emitted by `gen_schemas` (Stage 3 C7) and returned by the
/// `bifrost.list_permissions` MCP tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct BifrostPermissionDescriptor {
    /// Permission string in `resource:action` format (e.g. `bifrost_table:read`).
    pub permission: String,
    /// Resource component (e.g. `bifrost_table`).
    pub resource: String,
    /// Action component (e.g. `read`).
    pub action: String,
}

/// Partition transform on the wire. Arrow/Iceberg-free mirror of the engine
/// `PartitionTransform`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum PartitionTransformWire {
    /// Identity (value as-is).
    Identity,
    /// Truncate a timestamp to the year.
    Year,
    /// Truncate a timestamp to the month.
    Month,
    /// Truncate a timestamp to the day.
    Day,
    /// Truncate a timestamp to the hour.
    Hour,
    /// Hash into `n` buckets.
    Bucket {
        /// Bucket count.
        n: i32,
    },
    /// Truncate to width `w`.
    Truncate {
        /// Truncation width.
        w: i32,
    },
}

/// One partition-column declaration on the register request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PartitionColumnSpec {
    /// Column to partition on.
    pub column: String,
    /// Transform applied to the column value.
    pub transform: PartitionTransformWire,
}

/// Register (create) a Bifrost table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct RegisterTableRequest {
    /// Target namespace.
    pub namespace: String,
    /// Table name.
    pub name: String,
    /// User fields only; `wyrd_*`/`card_ref`/`run_id` reserved names are rejected.
    pub fields: Vec<FieldSpec>,
    /// Declared partition columns.
    #[serde(default)]
    pub partition_columns: Vec<PartitionColumnSpec>,
    /// Table scope; defaults to `TenantOwned`.
    #[serde(default)]
    pub scope: TableScopeWire,
}

/// Whether a register call created a new table or matched an existing one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum RegisterOutcome {
    /// A new table was created.
    Created,
    /// A table with a matching fingerprint already existed (idempotent).
    AlreadyExists,
}

/// Response to a register call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct RegisterTableResponse {
    /// Whether the table was created or already existed.
    pub outcome: RegisterOutcome,
    /// Lower-case hex of the 16-byte table uid.
    pub table_uid: String,
    /// Server-authoritative lower-case hex of the 32-byte schema fingerprint.
    pub fingerprint: String,
}

// ── Query wire types (split sync / async — review M7) ────────────────────────

/// Maximum rows a single ValaQueryService page may return. Requests naming a
/// larger `limit` are capped to this value server-side.
pub const MAX_QUERY_PAGE_SIZE: u32 = 1000;

/// Shared time-window + pagination envelope carried by every ValaQueryService
/// request. `limit` is capped at [`MAX_QUERY_PAGE_SIZE`] server-side.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct QueryWindow {
    /// Inclusive lower bound on `wyrd_event_time`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<DateTime<Utc>>,
    /// Exclusive upper bound on `wyrd_event_time`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<DateTime<Utc>>,
    /// Requested page size; capped at [`MAX_QUERY_PAGE_SIZE`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// Opaque continuation token from a prior page's `next_page_token`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_token: Option<String>,
}

/// `GetTrace` request — fetch one full trace waterfall by id.
///
/// # Example
/// ```json
/// { "trace_id": "b7f3c1e2a4d5", "since": "2026-07-01T00:00:00Z" }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct GetTraceRequest {
    /// Shared time-window + pagination envelope.
    #[serde(flatten)]
    pub window: QueryWindow,
    /// Trace id to fetch. Required.
    pub trace_id: String,
}

/// `QueryTraces` request — list trace summaries matching the filters.
///
/// # Example
/// ```json
/// { "since": "2026-07-01T00:00:00Z", "service": "checkout", "min_duration_ms": 250, "limit": 100 }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct QueryTracesRequest {
    /// Shared time-window + pagination envelope.
    #[serde(flatten)]
    pub window: QueryWindow,
    /// Optional service-name filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    /// Optional minimum root-span duration in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_duration_ms: Option<u32>,
    /// Optional status filter (e.g. `"ERROR"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Optional root-span name filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// `QueryRecentTraces` request — most-recent traces matching the filters.
///
/// # Example
/// ```json
/// { "service": "checkout", "limit": 50 }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct QueryRecentTracesRequest {
    /// Shared time-window + pagination envelope.
    #[serde(flatten)]
    pub window: QueryWindow,
    /// Optional service-name filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    /// Optional status filter (e.g. `"ERROR"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Optional minimum root-span duration in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_duration_ms: Option<u32>,
}

/// `QueryGenAi` request — GenAI generation records matching the filters.
///
/// # Example
/// ```json
/// { "since": "2026-07-01T00:00:00Z", "model": "gpt-4o", "limit": 100 }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct QueryGenAiRequest {
    /// Shared time-window + pagination envelope.
    #[serde(flatten)]
    pub window: QueryWindow,
    /// Optional conversation-id filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    /// Optional model-name filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Optional provider filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

/// `QueryEval` request — evaluation results matching the filters.
///
/// # Example
/// ```json
/// { "eval_id": "eval-abc", "limit": 100 }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct QueryEvalRequest {
    /// Shared time-window + pagination envelope.
    #[serde(flatten)]
    pub window: QueryWindow,
    /// Optional eval-id filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eval_id: Option<String>,
    /// Optional run-id filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}

/// `QueryDrift` request — drift observations matching the filters.
///
/// # Example
/// ```json
/// { "feature": "amount", "since": "2026-07-01T00:00:00Z" }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct QueryDriftRequest {
    /// Shared time-window + pagination envelope.
    #[serde(flatten)]
    pub window: QueryWindow,
    /// Optional feature-name filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
    /// Optional run-id filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}

/// `QueryMetrics` request — metric points matching the filters.
///
/// # Example
/// ```json
/// { "metric_name": "request_latency", "metric_type": "histogram", "limit": 500 }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct QueryMetricsRequest {
    /// Shared time-window + pagination envelope.
    #[serde(flatten)]
    pub window: QueryWindow,
    /// Optional metric-name filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric_name: Option<String>,
    /// Optional metric-type filter (e.g. `"gauge"`, `"counter"`, `"histogram"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric_type: Option<String>,
}

/// `QueryLogs` request — log records matching the filters.
///
/// # Example
/// ```json
/// { "severity_number_min": 17, "trace_id": "b7f3c1e2a4d5", "limit": 200 }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct QueryLogsRequest {
    /// Shared time-window + pagination envelope.
    #[serde(flatten)]
    pub window: QueryWindow,
    /// Optional inclusive minimum OTEL severity number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity_number_min: Option<i32>,
    /// Optional trace-id correlation filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    /// Optional event-name filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_name: Option<String>,
}

/// `QueryAgentTraces` request — agent/dev-session traces matching the filters.
///
/// # Example
/// ```json
/// { "repo": "wyrd", "branch": "main", "since": "2026-07-01T00:00:00Z" }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct QueryAgentTracesRequest {
    /// Shared time-window + pagination envelope.
    #[serde(flatten)]
    pub window: QueryWindow,
    /// Optional dev-session-id filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dev_session_id: Option<String>,
    /// Optional repository filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// Optional commit-sha filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
    /// Optional branch filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Optional run-id filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}

/// One span in a trace waterfall. `attributes` is payload-gated and omitted when
/// the caller lacks `bifrost_trace_payload:read`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct SpanRow {
    /// Span id.
    pub span_id: String,
    /// Parent span id; absent for the root span.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<String>,
    /// Span name.
    pub name: String,
    /// Span kind (e.g. `"SERVER"`, `"CLIENT"`).
    pub kind: String,
    /// Span start time.
    pub started_at: DateTime<Utc>,
    /// Span duration in milliseconds.
    pub duration_ms: f64,
    /// Span status (e.g. `"OK"`, `"ERROR"`).
    pub status: String,
    /// Payload-gated span attributes; omitted without `bifrost_trace_payload:read`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attributes: Option<serde_json::Value>,
}

/// One span event. `attributes` is payload-gated (`bifrost_trace_payload:read`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct SpanEventRow {
    /// Event name.
    pub name: String,
    /// Event timestamp.
    pub timestamp: DateTime<Utc>,
    /// Payload-gated event attributes; omitted without `bifrost_trace_payload:read`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attributes: Option<serde_json::Value>,
}

/// One span link. `attributes` is payload-gated (`bifrost_trace_payload:read`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct SpanLinkRow {
    /// Linked trace id.
    pub linked_trace_id: String,
    /// Linked span id.
    pub linked_span_id: String,
    /// Payload-gated link attributes; omitted without `bifrost_trace_payload:read`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attributes: Option<serde_json::Value>,
}

/// `GetTrace` response — the full waterfall for one trace. Nested-row payload
/// fields are omitted when the caller lacks `bifrost_trace_payload:read`.
///
/// # Example
/// ```json
/// {
///   "trace_id": "b7f3c1e2a4d5",
///   "spans": [
///     { "span_id": "1", "name": "GET /checkout", "kind": "SERVER",
///       "started_at": "2026-07-01T00:00:00Z", "duration_ms": 42.5, "status": "OK" }
///   ],
///   "events": [],
///   "links": []
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct TraceWaterfall {
    /// Trace id.
    pub trace_id: String,
    /// Spans in the trace.
    pub spans: Vec<SpanRow>,
    /// Span events in the trace.
    pub events: Vec<SpanEventRow>,
    /// Span links in the trace.
    pub links: Vec<SpanLinkRow>,
}

/// Derived one-row trace summary. Carries no attributes/payload columns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct TraceSummaryRow {
    /// Trace id.
    pub trace_id: String,
    /// Root span name.
    pub root_name: String,
    /// Emitting service.
    pub service: String,
    /// Trace start time.
    pub started_at: DateTime<Utc>,
    /// Total trace duration in milliseconds.
    pub duration_ms: f64,
    /// Number of spans in the trace.
    pub span_count: u32,
    /// Whether the trace contains an error.
    pub error: bool,
}

/// One GenAI generation record. `prompt`/`completion` are payload-gated and
/// omitted when the caller lacks `bifrost_genai_payload:read`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct GenAiRow {
    /// Conversation id.
    pub conversation_id: String,
    /// Model name.
    pub model: String,
    /// Provider.
    pub provider: String,
    /// Generation start time.
    pub started_at: DateTime<Utc>,
    /// Input token count, if recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<i64>,
    /// Output token count, if recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<i64>,
    /// Cost in USD, if recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    /// Payload-gated prompt text; omitted without `bifrost_genai_payload:read`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// Payload-gated completion text; omitted without `bifrost_genai_payload:read`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion: Option<String>,
}

/// One evaluation result row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct EvalRow {
    /// Eval id.
    pub eval_id: String,
    /// Run id.
    pub run_id: String,
    /// Metric name.
    pub metric: String,
    /// Metric score.
    pub score: f64,
    /// Evaluation start time.
    pub started_at: DateTime<Utc>,
}

/// One drift observation row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct DriftRow {
    /// Feature name.
    pub feature: String,
    /// Run id.
    pub run_id: String,
    /// Computed drift score.
    pub drift_score: f64,
    /// Configured alert threshold, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,
    /// When drift was computed.
    pub computed_at: DateTime<Utc>,
}

/// One metric point. `attributes` is payload-gated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct MetricRow {
    /// Metric name.
    pub metric_name: String,
    /// Metric type (e.g. `"gauge"`, `"counter"`, `"histogram"`).
    pub metric_type: String,
    /// Metric value.
    pub value: f64,
    /// Point timestamp.
    pub timestamp: DateTime<Utc>,
    /// Payload-gated metric attributes; omitted without the payload permission.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attributes: Option<serde_json::Value>,
}

/// One log record. `body` is payload-gated and omitted when the caller lacks
/// `bifrost_log_payload:read`. No floats/JSON value → derives `Eq`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct LogRow {
    /// Record timestamp.
    pub timestamp: DateTime<Utc>,
    /// OTEL severity number.
    pub severity_number: i32,
    /// OTEL severity text.
    pub severity_text: String,
    /// Correlated trace id, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    /// Correlated span id, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_id: Option<String>,
    /// Event name, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_name: Option<String>,
    /// Payload-gated log body; omitted without `bifrost_log_payload:read`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

/// One agent/dev-session trace row. `payload` is payload-gated and omitted when
/// the caller lacks `bifrost_agent_trace_payload:read`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct AgentTraceRow {
    /// Dev-session id.
    pub dev_session_id: String,
    /// Repository.
    pub repo: String,
    /// Commit sha.
    pub commit_sha: String,
    /// Branch.
    pub branch: String,
    /// Run id, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    /// Trace start time.
    pub started_at: DateTime<Utc>,
    /// Payload-gated captured payload; omitted without `bifrost_agent_trace_payload:read`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}

/// `QueryTraces` response — a page of trace summaries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct QueryTracesResponse {
    /// Trace summaries in this page.
    pub rows: Vec<TraceSummaryRow>,
    /// Opaque continuation token; absent on the last page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
}

/// `QueryRecentTraces` response — a page of trace summaries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct QueryRecentTracesResponse {
    /// Trace summaries in this page.
    pub rows: Vec<TraceSummaryRow>,
    /// Opaque continuation token; absent on the last page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
}

/// `QueryGenAi` response — a page of GenAI rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct QueryGenAiResponse {
    /// GenAI rows in this page.
    pub rows: Vec<GenAiRow>,
    /// Opaque continuation token; absent on the last page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
}

/// `QueryEval` response — a page of eval rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct QueryEvalResponse {
    /// Eval rows in this page.
    pub rows: Vec<EvalRow>,
    /// Opaque continuation token; absent on the last page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
}

/// `QueryDrift` response — a page of drift rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct QueryDriftResponse {
    /// Drift rows in this page.
    pub rows: Vec<DriftRow>,
    /// Opaque continuation token; absent on the last page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
}

/// `QueryMetrics` response — a page of metric rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct QueryMetricsResponse {
    /// Metric rows in this page.
    pub rows: Vec<MetricRow>,
    /// Opaque continuation token; absent on the last page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
}

/// `QueryLogs` response — a page of log rows. No floats/JSON value → derives `Eq`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct QueryLogsResponse {
    /// Log rows in this page.
    pub rows: Vec<LogRow>,
    /// Opaque continuation token; absent on the last page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
}

/// `QueryAgentTraces` response — a page of agent-trace rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct QueryAgentTracesResponse {
    /// Agent-trace rows in this page.
    pub rows: Vec<AgentTraceRow>,
    /// Opaque continuation token; absent on the last page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
}

/// A bound SQL parameter value for a parameterized query. Arrow-free scalar set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum QueryParam {
    /// SQL `NULL`.
    Null,
    /// Boolean.
    Bool(bool),
    /// 64-bit signed integer.
    Int(i64),
    /// 64-bit float.
    Float(f64),
    /// UTF-8 text.
    Text(String),
}

/// Opaque async-query job identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct JobUid(pub uuid::Uuid);

/// Terminal/in-flight state of an async query job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum AsyncJobState {
    /// Accepted, not yet claimed by an executor.
    Queued,
    /// Claimed by an executor.
    Claimed,
    /// Executing.
    Running,
    /// Completed successfully.
    Succeeded,
    /// Failed.
    Failed,
    /// Canceled.
    Canceled,
}

/// Machine-readable executor-availability signal (review M-06). In Stage 3 a
/// queued job is accepted but its executor does not exist yet, so status carries
/// [`ExecutorAvailability::PendingStage5`] rather than a false "running" claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum ExecutorAvailability {
    /// An executor is available (Stage 5+).
    Available,
    /// No executor exists yet — the job is accepted but will not run until Stage 5.
    PendingStage5,
}

/// Synchronous SQL query request. The response is a raw Arrow IPC stream, not a
/// JSON type, so no response struct lives here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct SyncQueryRequest {
    /// SELECT-only SQL text.
    pub sql: String,
    /// Bound parameters.
    #[serde(default)]
    pub params: Vec<QueryParam>,
}

/// Asynchronous SQL query submission.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct AsyncQueryRequest {
    /// SELECT-only SQL text.
    pub sql: String,
    /// Bound parameters.
    #[serde(default)]
    pub params: Vec<QueryParam>,
}

/// Response to an async query submission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct AsyncQueryResponse {
    /// Assigned job id.
    pub job_uid: JobUid,
    /// Initial job state.
    pub state: AsyncJobState,
    /// Executor-availability signal (review M-06).
    pub executor_availability: ExecutorAvailability,
}

/// Status of a previously-submitted async query job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct AsyncQueryStatus {
    /// Job id.
    pub job_uid: JobUid,
    /// Current job state.
    pub state: AsyncJobState,
    /// Executor-availability signal — `PendingStage5` in Stage 3 (review M-06).
    pub executor_availability: ExecutorAvailability,
    /// Stable Wyrd error code on failure (review M-15).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    /// Sanitized error detail on failure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_detail: Option<String>,
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    #[test]
    fn query_contracts_roundtrip() {
        assert_eq!(MAX_QUERY_PAGE_SIZE, 1000);

        let window = QueryWindow {
            since: Some(Utc::now()),
            until: None,
            limit: Some(100),
            page_token: None,
        };

        let get_trace = GetTraceRequest {
            window: window.clone(),
            trace_id: "b7f3c1e2a4d5".to_owned(),
        };
        let v = serde_json::to_value(&get_trace).unwrap();
        assert_eq!(serde_json::from_value::<GetTraceRequest>(v).unwrap(), get_trace);

        let query_traces = QueryTracesRequest {
            window: window.clone(),
            service: Some("checkout".to_owned()),
            min_duration_ms: Some(250),
            status: Some("ERROR".to_owned()),
            name: Some("GET /checkout".to_owned()),
        };
        let v = serde_json::to_value(&query_traces).unwrap();
        assert_eq!(serde_json::from_value::<QueryTracesRequest>(v).unwrap(), query_traces);

        let query_recent = QueryRecentTracesRequest {
            window: window.clone(),
            service: Some("svc".to_owned()),
            status: None,
            min_duration_ms: Some(10),
        };
        let v = serde_json::to_value(&query_recent).unwrap();
        assert_eq!(serde_json::from_value::<QueryRecentTracesRequest>(v).unwrap(), query_recent);

        let query_genai = QueryGenAiRequest {
            window: window.clone(),
            conversation_id: Some("conv-1".to_owned()),
            model: Some("gpt-4o".to_owned()),
            provider: Some("openai".to_owned()),
        };
        let v = serde_json::to_value(&query_genai).unwrap();
        assert_eq!(serde_json::from_value::<QueryGenAiRequest>(v).unwrap(), query_genai);

        let query_eval = QueryEvalRequest {
            window: window.clone(),
            eval_id: Some("eval-abc".to_owned()),
            run_id: Some("run-1".to_owned()),
        };
        let v = serde_json::to_value(&query_eval).unwrap();
        assert_eq!(serde_json::from_value::<QueryEvalRequest>(v).unwrap(), query_eval);

        let query_drift = QueryDriftRequest {
            window: window.clone(),
            feature: Some("amount".to_owned()),
            run_id: Some("run-2".to_owned()),
        };
        let v = serde_json::to_value(&query_drift).unwrap();
        assert_eq!(serde_json::from_value::<QueryDriftRequest>(v).unwrap(), query_drift);

        let query_metrics = QueryMetricsRequest {
            window: window.clone(),
            metric_name: Some("request_latency".to_owned()),
            metric_type: Some("histogram".to_owned()),
        };
        let v = serde_json::to_value(&query_metrics).unwrap();
        assert_eq!(serde_json::from_value::<QueryMetricsRequest>(v).unwrap(), query_metrics);

        let query_logs = QueryLogsRequest {
            window: window.clone(),
            severity_number_min: Some(17),
            trace_id: Some("b7f3c1e2a4d5".to_owned()),
            event_name: Some("exception".to_owned()),
        };
        let v = serde_json::to_value(&query_logs).unwrap();
        assert_eq!(serde_json::from_value::<QueryLogsRequest>(v).unwrap(), query_logs);

        let query_agent = QueryAgentTracesRequest {
            window: window.clone(),
            dev_session_id: Some("sess-1".to_owned()),
            repo: Some("wyrd".to_owned()),
            commit_sha: Some("abc123".to_owned()),
            branch: Some("main".to_owned()),
            run_id: Some("run-3".to_owned()),
        };
        let v = serde_json::to_value(&query_agent).unwrap();
        assert_eq!(serde_json::from_value::<QueryAgentTracesRequest>(v).unwrap(), query_agent);

        let now = Utc::now();

        let span_row = SpanRow {
            span_id: "s1".to_owned(),
            parent_span_id: None,
            name: "GET /checkout".to_owned(),
            kind: "SERVER".to_owned(),
            started_at: now,
            duration_ms: 42.5,
            status: "OK".to_owned(),
            attributes: None,
        };
        let waterfall = TraceWaterfall {
            trace_id: "b7f3c1e2a4d5".to_owned(),
            spans: vec![span_row],
            events: vec![],
            links: vec![],
        };
        let v = serde_json::to_value(&waterfall).unwrap();
        assert_eq!(serde_json::from_value::<TraceWaterfall>(v).unwrap(), waterfall);

        let summary = TraceSummaryRow {
            trace_id: "t1".to_owned(),
            root_name: "GET /".to_owned(),
            service: "checkout".to_owned(),
            started_at: now,
            duration_ms: 10.0,
            span_count: 3,
            error: false,
        };
        let traces_resp = QueryTracesResponse {
            rows: vec![summary],
            next_page_token: Some("tok".to_owned()),
        };
        let v = serde_json::to_value(&traces_resp).unwrap();
        assert_eq!(serde_json::from_value::<QueryTracesResponse>(v).unwrap(), traces_resp);

        let recent_resp = QueryRecentTracesResponse { rows: vec![], next_page_token: None };
        let v = serde_json::to_value(&recent_resp).unwrap();
        assert_eq!(serde_json::from_value::<QueryRecentTracesResponse>(v).unwrap(), recent_resp);

        let genai_row = GenAiRow {
            conversation_id: "conv-1".to_owned(),
            model: "gpt-4o".to_owned(),
            provider: "openai".to_owned(),
            started_at: now,
            input_tokens: Some(100),
            output_tokens: Some(200),
            cost_usd: Some(0.002),
            prompt: None,
            completion: None,
        };
        let genai_resp = QueryGenAiResponse { rows: vec![genai_row], next_page_token: None };
        let v = serde_json::to_value(&genai_resp).unwrap();
        assert_eq!(serde_json::from_value::<QueryGenAiResponse>(v).unwrap(), genai_resp);

        let eval_row = EvalRow {
            eval_id: "eval-abc".to_owned(),
            run_id: "run-1".to_owned(),
            metric: "accuracy".to_owned(),
            score: 0.95,
            started_at: now,
        };
        let eval_resp = QueryEvalResponse { rows: vec![eval_row], next_page_token: None };
        let v = serde_json::to_value(&eval_resp).unwrap();
        assert_eq!(serde_json::from_value::<QueryEvalResponse>(v).unwrap(), eval_resp);

        let drift_row = DriftRow {
            feature: "amount".to_owned(),
            run_id: "run-2".to_owned(),
            drift_score: 0.12,
            threshold: Some(0.1),
            computed_at: now,
        };
        let drift_resp = QueryDriftResponse { rows: vec![drift_row], next_page_token: None };
        let v = serde_json::to_value(&drift_resp).unwrap();
        assert_eq!(serde_json::from_value::<QueryDriftResponse>(v).unwrap(), drift_resp);

        let metric_row = MetricRow {
            metric_name: "request_latency".to_owned(),
            metric_type: "histogram".to_owned(),
            value: 42.0,
            timestamp: now,
            attributes: None,
        };
        let metrics_resp = QueryMetricsResponse { rows: vec![metric_row], next_page_token: None };
        let v = serde_json::to_value(&metrics_resp).unwrap();
        assert_eq!(serde_json::from_value::<QueryMetricsResponse>(v).unwrap(), metrics_resp);

        let log_row = LogRow {
            timestamp: now,
            severity_number: 17,
            severity_text: "ERROR".to_owned(),
            trace_id: Some("b7f3c1e2a4d5".to_owned()),
            span_id: None,
            event_name: None,
            body: None,
        };
        let logs_resp = QueryLogsResponse { rows: vec![log_row], next_page_token: None };
        let v = serde_json::to_value(&logs_resp).unwrap();
        assert_eq!(serde_json::from_value::<QueryLogsResponse>(v).unwrap(), logs_resp);

        let agent_row = AgentTraceRow {
            dev_session_id: "sess-1".to_owned(),
            repo: "wyrd".to_owned(),
            commit_sha: "abc123".to_owned(),
            branch: "main".to_owned(),
            run_id: None,
            started_at: now,
            payload: None,
        };
        let agent_resp = QueryAgentTracesResponse { rows: vec![agent_row], next_page_token: None };
        let v = serde_json::to_value(&agent_resp).unwrap();
        assert_eq!(serde_json::from_value::<QueryAgentTracesResponse>(v).unwrap(), agent_resp);

        let _ = schemars::schema_for!(QueryTracesRequest);
        let _ = schemars::schema_for!(QueryRecentTracesRequest);
        let _ = schemars::schema_for!(QueryGenAiRequest);
        let _ = schemars::schema_for!(QueryEvalRequest);
        let _ = schemars::schema_for!(QueryDriftRequest);
        let _ = schemars::schema_for!(QueryMetricsRequest);
        let _ = schemars::schema_for!(QueryLogsRequest);
        let _ = schemars::schema_for!(QueryAgentTracesRequest);
        let _ = schemars::schema_for!(GetTraceRequest);
        let _ = schemars::schema_for!(TraceWaterfall);
    }
}

// ── Audit event (S3.C5 — transactional audit outbox) ─────────────────────────

/// How the acting principal authenticated for an audited data-plane op.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    /// Presented a Wyrd-issued JWT access token.
    Jwt,
    /// An internal, non-JWT principal (e.g. a system/relay caller).
    Internal,
}

/// The RBAC authorization outcome recorded on an audit row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AuditDecision {
    /// The operation was authorized.
    Allow,
    /// The operation was refused by RBAC.
    Deny,
}

/// Whether the audited operation completed successfully.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AuditResult {
    /// The operation succeeded.
    Success,
    /// The operation failed.
    Failure,
}

/// One audited data-plane operation — the FULL locked M-06 field set.
///
/// Every audited op (register/install, sync query, async submit/status, ingest
/// commit, RBAC deny) appends exactly one hash-chained `AuditEvent` row in the
/// operation's own Postgres transaction. The hash-chain canonical encoding and
/// per-tenant `seq` are owned by `vala-sql`; this type is the Arrow-free,
/// PyO3-free wire/codegen shape.
///
/// `card_ref` is the **writer-identity card** (who performed the op), derived
/// from the resolved `Principal`; it is `None` only for a `User` principal.
/// This is decoupled from the per-row `card_ref` data column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct AuditEvent {
    /// Request correlation ID of the audited op.
    pub request_id: RequestId,
    /// Distributed-trace ID, when a trace context is present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    /// Logical operation name (e.g. `bifrost.register_table`).
    pub operation: String,
    /// Target resource the op acted on (e.g. the fully-qualified table name).
    pub resource: String,
    /// Writer-identity card of the acting principal; `None` for a `User`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_ref: Option<CardRef>,
    /// Stable ID of the acting principal.
    pub principal_id: PrincipalId,
    /// Kind of the acting principal (tag encoding; card payload is not the audit
    /// subject — `card_ref` is its own field).
    pub principal_kind: PrincipalKindTag,
    /// How the principal authenticated.
    pub auth_method: AuthMethod,
    /// Effective RBAC permission checked for the op.
    pub permission: String,
    /// The authorization decision.
    pub decision: AuditDecision,
    /// Whether the op completed successfully.
    pub result: AuditResult,
    /// Redacted summary of the operation payload.
    pub payload_summary: String,
}
