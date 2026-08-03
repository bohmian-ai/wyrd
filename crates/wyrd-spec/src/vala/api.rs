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
pub use crate::vala::audit_detail::{
    AuditDetail, AuditDetailValueError, BatchId, BifrostSecurityPhase,
    BifrostSecurityViolationKind, ForgeCompactionPhase, ForgeIcebergRewritePhase,
    ForgeOrphanGcPhase, ForgeSnapshotExpirePhase, QueryAuditDigest, QueryExecutionMode, ScopeHash,
    StoragePath, audit_detail_canonical_json,
};

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

/// Lifecycle status of a registered Bifrost table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
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
    /// Span events in the trace. Stage 4 stub — always empty until traces.events extraction is implemented.
    pub events: Vec<SpanEventRow>,
    /// Span links in the trace. Stage 4 stub — always empty until traces.links extraction is implemented.
    pub links: Vec<SpanLinkRow>,
}

/// `GetTrace` response — wraps the full waterfall for one trace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct GetTraceResponse {
    /// The full trace waterfall.
    pub trace: TraceWaterfall,
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
    /// Cost in USD. Always absent — planned for a future stage when the column is added to `genai.messages`.
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
    /// Run id, if correlated via `CorrelationPolicy::Observation`. Absent when no `run_id`
    /// correlation column was stamped on this row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
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
    /// Commit sha; nullable in the physical schema — absent when not recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
    /// Branch; nullable in the physical schema — absent when not recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
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

/// Maximum number of warnings carried by a terminal frame.
pub const MAX_QUERY_TERMINAL_WARNINGS: usize = 16;
/// Maximum number of closed source-completion entries.
pub const MAX_QUERY_SOURCE_COMPLETIONS: usize = 3;
/// Maximum byte length of scrubbed terminal error detail.
pub const MAX_QUERY_ERROR_DETAIL_BYTES: usize = 1_024;

/// Error returned when an Oracle query contract violates its closed protocol.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum QueryContractError {
    /// A required string is empty.
    #[error("{field} must not be empty")]
    Empty {
        /// Invalid field.
        field: &'static str,
    },
    /// A bounded collection exceeds its protocol maximum.
    #[error("{field} exceeds its maximum of {maximum}")]
    TooMany {
        /// Invalid collection field.
        field: &'static str,
        /// Protocol maximum.
        maximum: usize,
    },
    /// A bounded scrubbed value is invalid.
    #[error("{field} is not a valid scrubbed value")]
    InvalidDetail {
        /// Invalid scrubbed field.
        field: &'static str,
    },
    /// Terminal fields form an invalid state.
    #[error("invalid query terminal: {reason}")]
    InvalidTerminal {
        /// Closed validation reason.
        reason: &'static str,
    },
}

/// Visibility tiers included in one immutable Oracle query cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum VisibilityMode {
    /// Read pinned Iceberg and hot sealed files.
    PublishedOnly,
    /// Also read the exact fenced live-tail interval.
    Fused,
}

/// Behavior when a requested live source cannot complete.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum FreshnessPolicy {
    /// Fail when every requested source cannot complete.
    Strict,
    /// Retain a bounded degraded result when live data is unavailable.
    AllowDegraded,
}

impl Default for FreshnessPolicy {
    /// Uses strict freshness so omitted client policy never hides an
    /// unavailable live source.
    fn default() -> Self {
        Self::Strict
    }
}

/// Public synchronous Oracle query request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct BifrostQueryRequest {
    /// SELECT-only SQL text.
    pub sql: String,
    /// Visibility tiers requested by the caller.
    pub visibility: VisibilityMode,
    /// Required freshness behavior.
    pub freshness: FreshnessPolicy,
    /// Optional caller deadline in milliseconds.
    pub deadline_ms: Option<u64>,
}

impl BifrostQueryRequest {
    /// Validates request fields whose limits are part of the pure protocol.
    ///
    /// # Errors
    /// Returns [`QueryContractError`] when SQL is empty or the deadline is zero.
    pub fn validate(&self) -> Result<(), QueryContractError> {
        if self.sql.trim().is_empty() {
            return Err(QueryContractError::Empty { field: "sql" });
        }
        if self.deadline_ms == Some(0) {
            return Err(QueryContractError::InvalidTerminal {
                reason: "deadline_ms must be positive",
            });
        }
        Ok(())
    }
}

/// Server-derived admission class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum QueryClass {
    /// Latency-sensitive bounded work.
    Interactive,
    /// Larger analytical work.
    Analytical,
}

/// One logical frame in the public query stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum QueryStreamFrame {
    /// Stream schema, emitted exactly once.
    Schema(QuerySchemaFrame),
    /// Arrow IPC record batch bytes.
    Batch(QueryBatchFrame),
    /// Required terminal state.
    Terminal(QueryTerminalFrame),
}

/// Schema frame for a query stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct QuerySchemaFrame {
    /// Stable schema fingerprint.
    pub schema_fingerprint: String,
    /// Arrow IPC schema bytes.
    pub arrow_ipc_schema: Vec<u8>,
}

/// Data frame for a query stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct QueryBatchFrame {
    /// Exact Arrow IPC batch bytes.
    pub arrow_ipc_batch: Vec<u8>,
}

/// Final stream outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum QueryTerminalOutcome {
    /// Query completed with its full cut.
    Success,
    /// Query completed with an explicitly degraded cut.
    Degraded,
    /// Query failed after framing began.
    Failed,
}

/// Freshness achieved by the admitted visibility cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum QueryFreshness {
    /// Every selected source was available.
    Complete,
    /// The admitted cut omitted an unavailable live source.
    Degraded,
}

/// Closed source tiers represented in terminal metadata.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum QuerySource {
    /// Published Iceberg snapshot.
    Iceberg,
    /// Sealed files not yet published into Iceberg.
    HotSealed,
    /// Fenced Scribe live-tail interval.
    LiveTail,
}

/// Closed warnings emitted by the Oracle protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum QueryWarning {
    /// A Fused query omitted unavailable live-tail data.
    LiveTailUnavailable,
    /// A stale sealed cut was replaced once before output.
    StaleCutReplanned,
}

/// Completion state for one source tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum SourceCompletionOutcome {
    /// The source completed.
    Complete,
    /// The live source was unavailable under degraded freshness.
    Unavailable,
}

/// Completion metadata for one source tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct SourceCompletion {
    /// Closed source tier.
    pub source: QuerySource,
    /// Source outcome.
    pub outcome: SourceCompletionOutcome,
}

/// Closed stable codes allowed in late failed terminals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum QueryTerminalErrorCode {
    /// The query deadline elapsed.
    QueryTimeout,
    /// A required visibility source was unavailable.
    QueryVisibilityUnavailable,
    /// A tenant isolation invariant failed.
    QueryTenantInvariant,
    /// Equal row identities contained unequal values.
    QueryReconciliationInvariant,
    /// Peer authentication, fencing, or replay validation failed.
    QueryPeerSecurity,
    /// The read-decision audit dependency failed.
    QueryAuditUnavailable,
    /// The table catalog was unavailable.
    CatalogUnreachable,
    /// Object storage was unavailable.
    StorageUnreachable,
    /// Query execution failed after framing began.
    QueryExecutionFailed,
}

/// Scrubbed detail attached to a failed terminal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct QueryErrorDetail(
    /// Normalized bounded detail text.
    String,
);

impl QueryErrorDetail {
    /// Constructs bounded, control-free terminal detail.
    ///
    /// # Errors
    /// Returns [`QueryContractError`] for empty, overlong, control-bearing, or
    /// secret-like input.
    pub fn new(value: impl Into<String>) -> Result<Self, QueryContractError> {
        let value = value.into();
        let value = value.trim();
        let lower = value.to_ascii_lowercase();
        if value.is_empty()
            || value.len() > MAX_QUERY_ERROR_DETAIL_BYTES
            || value.chars().any(char::is_control)
            || lower.contains("bearer ")
            || lower.contains("token=")
            || lower.contains("password=")
            || lower.contains("secret=")
            || lower.contains("-----begin ")
        {
            return Err(QueryContractError::InvalidDetail { field: "detail" });
        }
        Ok(Self(value.to_owned()))
    }

    /// Borrows the scrubbed detail.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for QueryErrorDetail {
    /// Deserializes diagnostic text while reapplying redaction invariants.
    ///
    /// # Errors
    /// Returns a deserializer error when the input is not text or contains
    /// forbidden secret-bearing material.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// Stable late-stream error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct QueryTerminalError {
    /// Closed stable error code.
    pub code: QueryTerminalErrorCode,
    /// Optional scrubbed diagnostic.
    pub detail: Option<QueryErrorDetail>,
}

/// Terminal frame retaining immutable cut metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct QueryTerminalFrame {
    /// Stream outcome.
    pub outcome: QueryTerminalOutcome,
    /// Admitted-cut freshness.
    pub freshness: QueryFreshness,
    /// Rows already emitted in batch frames.
    pub row_count: u64,
    /// Closed bounded warnings.
    pub warnings: Vec<QueryWarning>,
    /// Exactly one entry for each source present in the cut.
    pub source_completion: Vec<SourceCompletion>,
    /// Required only for failed terminals.
    pub error: Option<QueryTerminalError>,
}

impl QueryTerminalFrame {
    /// Validates closed terminal combinations for the selected visibility.
    ///
    /// # Errors
    /// Returns [`QueryContractError`] for invalid cardinality, duplicate or
    /// missing sources, or inconsistent outcome/freshness/error fields.
    pub fn validate(&self, visibility: VisibilityMode) -> Result<(), QueryContractError> {
        if self.warnings.len() > MAX_QUERY_TERMINAL_WARNINGS {
            return Err(QueryContractError::TooMany {
                field: "warnings",
                maximum: MAX_QUERY_TERMINAL_WARNINGS,
            });
        }
        let expected = if visibility == VisibilityMode::Fused {
            3
        } else {
            2
        };
        if self.source_completion.len() != expected {
            return Err(QueryContractError::InvalidTerminal {
                reason: "source completion does not match visibility",
            });
        }
        let mut seen = std::collections::BTreeSet::new();
        if self
            .source_completion
            .iter()
            .any(|entry| !seen.insert(entry.source))
        {
            return Err(QueryContractError::InvalidTerminal {
                reason: "source completion contains duplicates",
            });
        }
        for source in [QuerySource::Iceberg, QuerySource::HotSealed] {
            if !self.source_completion.iter().any(|entry| {
                entry.source == source && entry.outcome == SourceCompletionOutcome::Complete
            }) {
                return Err(QueryContractError::InvalidTerminal {
                    reason: "sealed sources must be complete",
                });
            }
        }
        let degraded_live = self.source_completion.iter().any(|entry| {
            entry.source == QuerySource::LiveTail
                && entry.outcome == SourceCompletionOutcome::Unavailable
        });
        let failed = self.outcome == QueryTerminalOutcome::Failed;
        if failed != self.error.is_some() {
            return Err(QueryContractError::InvalidTerminal {
                reason: "error presence must match failed outcome",
            });
        }
        if self.outcome == QueryTerminalOutcome::Success
            && self.freshness != QueryFreshness::Complete
        {
            return Err(QueryContractError::InvalidTerminal {
                reason: "success must be complete",
            });
        }
        if self.outcome == QueryTerminalOutcome::Degraded
            && (!degraded_live || self.freshness != QueryFreshness::Degraded)
        {
            return Err(QueryContractError::InvalidTerminal {
                reason: "degraded requires unavailable live source",
            });
        }
        if degraded_live != (self.freshness == QueryFreshness::Degraded) {
            return Err(QueryContractError::InvalidTerminal {
                reason: "freshness must match live-tail source completion",
            });
        }
        if self.warnings.contains(&QueryWarning::LiveTailUnavailable) != degraded_live {
            return Err(QueryContractError::InvalidTerminal {
                reason: "live-tail warning must match unavailable source",
            });
        }
        Ok(())
    }

    /// Validates the terminal against the rows actually emitted before it.
    ///
    /// # Errors
    /// Returns [`QueryContractError`] when terminal `row_count` does not equal
    /// the accumulated rows from preceding batch frames.
    pub fn validate_emitted_rows(&self, emitted_rows: u64) -> Result<(), QueryContractError> {
        if self.row_count != emitted_rows {
            return Err(QueryContractError::InvalidTerminal {
                reason: "terminal row count does not match emitted rows",
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod query_terminal_tests {
    use super::*;

    /// Builds the required closed source set for one visibility mode.
    fn complete_sources(visibility: VisibilityMode) -> Vec<SourceCompletion> {
        let mut sources = vec![
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
            sources.push(SourceCompletion {
                source: QuerySource::LiveTail,
                outcome: SourceCompletionOutcome::Complete,
            });
        }
        sources
    }

    /// Success, degraded, and partial-row failed terminals preserve their cut.
    #[test]
    fn closed_terminal_matrix_validates() {
        let success = QueryTerminalFrame {
            outcome: QueryTerminalOutcome::Success,
            freshness: QueryFreshness::Complete,
            row_count: 2,
            warnings: vec![],
            source_completion: complete_sources(VisibilityMode::PublishedOnly),
            error: None,
        };
        success
            .validate(VisibilityMode::PublishedOnly)
            .expect("success terminal validates");
        success
            .validate_emitted_rows(2)
            .expect("success row count validates");

        let mut degraded_sources = complete_sources(VisibilityMode::Fused);
        degraded_sources[2].outcome = SourceCompletionOutcome::Unavailable;
        let degraded = QueryTerminalFrame {
            outcome: QueryTerminalOutcome::Degraded,
            freshness: QueryFreshness::Degraded,
            row_count: 1,
            warnings: vec![QueryWarning::LiveTailUnavailable],
            source_completion: degraded_sources.clone(),
            error: None,
        };
        degraded
            .validate(VisibilityMode::Fused)
            .expect("degraded terminal validates");

        let failed = QueryTerminalFrame {
            outcome: QueryTerminalOutcome::Failed,
            freshness: QueryFreshness::Degraded,
            row_count: 1,
            warnings: vec![QueryWarning::LiveTailUnavailable],
            source_completion: degraded_sources,
            error: Some(QueryTerminalError {
                code: QueryTerminalErrorCode::QueryExecutionFailed,
                detail: None,
            }),
        };
        failed
            .validate(VisibilityMode::Fused)
            .expect("partial-row failure validates");
        failed
            .validate_emitted_rows(1)
            .expect("partial-row count validates");
    }

    /// Failed terminals require an error and exact emitted-row count.
    #[test]
    fn invalid_failed_terminal_is_rejected() {
        let failed_without_error = QueryTerminalFrame {
            outcome: QueryTerminalOutcome::Failed,
            freshness: QueryFreshness::Complete,
            row_count: 3,
            warnings: vec![],
            source_completion: complete_sources(VisibilityMode::PublishedOnly),
            error: None,
        };
        assert!(
            failed_without_error
                .validate(VisibilityMode::PublishedOnly)
                .is_err()
        );

        let failed = QueryTerminalFrame {
            error: Some(QueryTerminalError {
                code: QueryTerminalErrorCode::QueryExecutionFailed,
                detail: None,
            }),
            ..failed_without_error
        };
        assert!(failed.validate_emitted_rows(2).is_err());
    }

    /// Query requests require both policies and reject empty or zero-valued input.
    #[test]
    fn query_request_requires_policies_and_validation_is_closed() {
        for omitted in ["visibility", "freshness"] {
            let mut value = serde_json::json!({
                "sql": "SELECT 1",
                "visibility": "published_only",
                "freshness": "strict",
                "deadline_ms": null
            });
            value
                .as_object_mut()
                .expect("request fixture is an object")
                .remove(omitted);
            assert!(
                serde_json::from_value::<BifrostQueryRequest>(value).is_err(),
                "omitting {omitted} must fail closed"
            );
        }

        let request: BifrostQueryRequest = serde_json::from_value(serde_json::json!({
            "sql": "SELECT 1",
            "visibility": "published_only",
            "freshness": "strict",
            "deadline_ms": null
        }))
        .expect("request deserializes");
        assert_eq!(request.freshness, FreshnessPolicy::Strict);
        request.validate().expect("explicit request validates");

        let invalid = BifrostQueryRequest {
            sql: " ".into(),
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: Some(0),
        };
        assert!(invalid.validate().is_err());
    }

    /// Every explicit safe or opt-in policy pair round-trips without inference.
    #[test]
    fn query_request_explicit_policy_pairs_round_trip() {
        let cases = [
            (
                VisibilityMode::PublishedOnly,
                FreshnessPolicy::Strict,
                "published_only",
                "strict",
            ),
            (
                VisibilityMode::Fused,
                FreshnessPolicy::AllowDegraded,
                "fused",
                "allow_degraded",
            ),
        ];

        for (visibility, freshness, wire_visibility, wire_freshness) in cases {
            let request = BifrostQueryRequest {
                sql: "SELECT 1".into(),
                visibility,
                freshness,
                deadline_ms: None,
            };
            let encoded = serde_json::to_value(&request).expect("request serializes");
            assert_eq!(encoded["visibility"], wire_visibility);
            assert_eq!(encoded["freshness"], wire_freshness);
            let decoded: BifrostQueryRequest =
                serde_json::from_value(encoded).expect("serialized request deserializes");
            assert_eq!(decoded, request);
        }
    }

    /// Failed terminals still obey settled freshness and source consistency.
    #[test]
    fn failed_terminal_rejects_inconsistent_freshness() {
        let failed = |freshness, source_completion| QueryTerminalFrame {
            outcome: QueryTerminalOutcome::Failed,
            freshness,
            row_count: 0,
            warnings: vec![],
            source_completion,
            error: Some(QueryTerminalError {
                code: QueryTerminalErrorCode::QueryExecutionFailed,
                detail: None,
            }),
        };
        let degraded_without_live = failed(
            QueryFreshness::Degraded,
            complete_sources(VisibilityMode::Fused),
        );
        assert!(
            degraded_without_live
                .validate(VisibilityMode::Fused)
                .is_err()
        );

        let mut unavailable_live = complete_sources(VisibilityMode::Fused);
        unavailable_live[2].outcome = SourceCompletionOutcome::Unavailable;
        let complete_with_unavailable_live = failed(QueryFreshness::Complete, unavailable_live);
        assert!(
            complete_with_unavailable_live
                .validate(VisibilityMode::Fused)
                .is_err()
        );
    }

    /// Oracle admission rejects empty and oversized execution-node selections.
    #[test]
    fn oracle_admission_selected_nodes_are_bounded() {
        let leader = NodeId::new(uuid::Uuid::now_v7());
        let lease = |selected_node_ids| OracleAdmissionLease {
            query_id: QueryId::new(uuid::Uuid::now_v7()),
            data_tenant_id: crate::DataTenantId::new_v7(),
            query_class: QueryClass::Interactive,
            slot_units: 1,
            selected_node_ids,
            leader_node_id: leader,
            leader_fencing_token: 1,
            acquired_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::minutes(1),
        };
        assert!(lease(vec![]).validate().is_err());
        assert!(
            lease((0..65).map(|_| NodeId::new(uuid::Uuid::now_v7())).collect())
                .validate()
                .is_err()
        );
    }
}

/// Stable cluster node identifier.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct NodeId(
    /// Physical cluster node UUID.
    uuid::Uuid,
);

impl NodeId {
    /// Wraps one UUID node identity.
    #[must_use]
    pub const fn new(value: uuid::Uuid) -> Self {
        Self(value)
    }
    /// Returns the underlying UUID.
    #[must_use]
    pub const fn as_uuid(self) -> uuid::Uuid {
        self.0
    }
}
impl From<NodeId> for uuid::Uuid {
    /// Unwraps the physical node identity for database and wire boundaries.
    fn from(value: NodeId) -> Self {
        value.0
    }
}
impl From<uuid::Uuid> for NodeId {
    /// Wraps a UUID as a typed physical cluster-node identity.
    fn from(value: uuid::Uuid) -> Self {
        Self(value)
    }
}
/// Monotonic role fencing token.
pub type FencingToken = u64;
/// Stable admitted query identifier.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct QueryId(
    /// Stable admitted-query UUID.
    uuid::Uuid,
);
impl QueryId {
    /// Wraps one UUID query identity.
    #[must_use]
    pub const fn new(value: uuid::Uuid) -> Self {
        Self(value)
    }
    /// Returns the underlying UUID.
    #[must_use]
    pub const fn as_uuid(self) -> uuid::Uuid {
        self.0
    }
}
impl From<QueryId> for uuid::Uuid {
    /// Unwraps the admitted query identity for database and wire boundaries.
    fn from(value: QueryId) -> Self {
        value.0
    }
}
impl From<uuid::Uuid> for QueryId {
    /// Wraps a UUID as a typed admitted-query identity.
    fn from(value: uuid::Uuid) -> Self {
        Self(value)
    }
}

/// Closed Bifrost runtime roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ClusterRole {
    /// WAL and live-tail owner.
    Scribe,
    /// Query leader and sealed-scan worker.
    Oracle,
}

/// Composite membership identity for one node role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ClusterNodeKey {
    /// Physical node identity.
    pub node_id: NodeId,
    /// Independently fenced role.
    pub role: ClusterRole,
}

/// Scribe v1 private-tail capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ScribeCapabilitiesV1 {
    /// Tail protocol version, exactly one in v1.
    pub tail_protocol_version: u16,
}

/// Oracle v1 placement and capacity capability.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct OracleCapabilitiesV1 {
    /// Peer execution protocol version.
    pub peer_protocol_version: u16,
    /// Shared storage protocol version.
    pub storage_protocol_version: u16,
    /// CPU cores available to Oracle work.
    pub cpu_cores: f64,
    /// Memory available to Oracle work.
    pub memory_budget_bytes: u64,
    /// CPU represented by one slot.
    pub cpu_cores_per_slot: f64,
    /// Memory represented by one slot.
    pub memory_bytes_per_slot: u64,
    /// Computed total slot count.
    pub raw_slots: u32,
    /// Slots exposed after reservations.
    pub usable_slots: u32,
    /// Supported admission classes.
    pub supported_classes: Vec<QueryClass>,
    /// Maximum accepted worker fanout.
    pub max_workers_per_query: u32,
}

/// Closed tagged role capability document persisted in membership.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClusterCapabilities {
    /// Scribe v1 tail capability.
    ScribeV1(ScribeCapabilitiesV1),
    /// Oracle v1 capacity capability.
    OracleV1(OracleCapabilitiesV1),
}

impl ClusterCapabilities {
    /// Validates the role match and all v1 protocol/capacity invariants.
    ///
    /// # Errors
    /// Returns [`QueryContractError`] for mismatched roles, unsupported
    /// versions, non-finite/zero capacity, or invalid slot/fanout bounds.
    pub fn validate_for_role(&self, role: ClusterRole) -> Result<(), QueryContractError> {
        match (role, self) {
            (ClusterRole::Scribe, Self::ScribeV1(value)) if value.tail_protocol_version == 1 => {
                Ok(())
            }
            (ClusterRole::Oracle, Self::OracleV1(value))
                if value.peer_protocol_version == 1
                    && value.storage_protocol_version == 1
                    && value.cpu_cores.is_finite()
                    && value.cpu_cores > 0.0
                    && value.cpu_cores_per_slot.is_finite()
                    && value.cpu_cores_per_slot > 0.0
                    && value.memory_budget_bytes > 0
                    && value.memory_bytes_per_slot > 0
                    && value.raw_slots > 0
                    && value.usable_slots > 0
                    && value.usable_slots <= value.raw_slots
                    && value.max_workers_per_query <= 63
                    && !value.supported_classes.is_empty() =>
            {
                Ok(())
            }
            _ => Err(QueryContractError::InvalidTerminal {
                reason: "invalid role capability document",
            }),
        }
    }
}

/// One live role lease projected from cluster membership.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ClusterRoleLease {
    /// Composite membership identity.
    pub key: ClusterNodeKey,
    /// Private service address.
    pub address: String,
    /// Current role fence.
    pub fencing_token: FencingToken,
    /// Capability schema version.
    pub capability_version: u16,
    /// Typed capability document.
    pub capabilities: ClusterCapabilities,
    /// Whether the role may receive new work.
    pub ready: bool,
    /// Role boot time.
    pub started_at: DateTime<Utc>,
    /// Latest fenced heartbeat.
    pub heartbeat_at: DateTime<Utc>,
}

/// Durable Oracle admission lease.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct OracleAdmissionLease {
    /// Query identity.
    pub query_id: QueryId,
    /// Authenticated data tenant.
    pub data_tenant_id: crate::DataTenantId,
    /// Server-derived class.
    pub query_class: QueryClass,
    /// Total demanded slots.
    pub slot_units: u32,
    /// Selected execution nodes.
    pub selected_node_ids: Vec<NodeId>,
    /// Leader node identity.
    pub leader_node_id: NodeId,
    /// Leader role fence.
    pub leader_fencing_token: FencingToken,
    /// Admission timestamp.
    pub acquired_at: DateTime<Utc>,
    /// Lease expiry.
    pub expires_at: DateTime<Utc>,
}

impl OracleAdmissionLease {
    /// Validates the pure admitted-node and lease bounds before persistence.
    ///
    /// # Errors
    /// Returns [`QueryContractError`] when selected nodes are outside `1..=64`,
    /// contain duplicates or omit the leader, slot demand is zero, or expiry
    /// does not follow acquisition.
    pub fn validate(&self) -> Result<(), QueryContractError> {
        if self.selected_node_ids.is_empty() || self.selected_node_ids.len() > 64 {
            return Err(QueryContractError::TooMany {
                field: "selected_node_ids",
                maximum: 64,
            });
        }
        let mut nodes = self.selected_node_ids.clone();
        nodes.sort();
        nodes.dedup();
        if nodes.len() != self.selected_node_ids.len()
            || !nodes.contains(&self.leader_node_id)
            || self.slot_units == 0
            || self.acquired_at >= self.expires_at
        {
            return Err(QueryContractError::InvalidTerminal {
                reason: "invalid Oracle admission lease",
            });
        }
        Ok(())
    }
}

/// Canonical admission accounting scope that rejected a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AdmissionScope {
    /// Deployment-wide slot ceiling.
    Cluster,
    /// Query-class slot ceiling.
    Class,
    /// Tenant and query-class slot ceiling.
    Tenant,
}

/// Monotonic Scribe writer boot epoch.
pub type WriterEpoch = u64;
/// Monotonic write-ahead-log sequence within one writer epoch.
pub type WalLsn = u64;

macro_rules! private_uuid_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(
            Debug,
            Clone,
            Copy,
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Hash,
            Serialize,
            Deserialize,
            schemars::JsonSchema,
        )]
        #[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
        #[serde(transparent)]
        pub struct $name(
            /// Opaque UUID carried by the private control-plane contract.
            uuid::Uuid,
        );

        impl $name {
            /// Wraps one validated UUID identity.
            #[must_use]
            pub const fn new(value: uuid::Uuid) -> Self {
                Self(value)
            }

            /// Returns the underlying UUID.
            #[must_use]
            pub const fn as_uuid(self) -> uuid::Uuid {
                self.0
            }
        }
    };
}

private_uuid_id!(TailFenceId, "Opaque identity for one Scribe tail fence.");
private_uuid_id!(
    ReservationId,
    "Opaque identity for one pending Oracle worker reservation."
);

/// Validated UTC event-day carried by private tail contracts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct EventDay(
    /// Canonical validated `YYYY-MM-DD` text.
    String,
);

impl EventDay {
    /// Parses one canonical `YYYY-MM-DD` UTC event day.
    ///
    /// # Errors
    /// Returns [`QueryContractError`] when the value is not a calendar date.
    pub fn new(value: impl Into<String>) -> Result<Self, QueryContractError> {
        let value = value.into();
        chrono::NaiveDate::parse_from_str(&value, "%Y-%m-%d").map_err(|_| {
            QueryContractError::InvalidTerminal {
                reason: "event_day must be YYYY-MM-DD",
            }
        })?;
        Ok(Self(value))
    }

    /// Borrows the canonical event day.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for EventDay {
    /// Deserializes an event-day string while enforcing its canonical date form.
    ///
    /// # Errors
    /// Returns a deserializer error when the input is not text or is not a
    /// calendar date formatted as `YYYY-MM-DD`.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// Validated non-empty schema fingerprint used by private tail contracts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct SchemaFingerprint(
    /// Validated bounded fingerprint text.
    String,
);

impl SchemaFingerprint {
    /// Constructs one bounded schema fingerprint.
    ///
    /// # Errors
    /// Returns [`QueryContractError`] when empty or over 1,024 bytes.
    pub fn new(value: impl Into<String>) -> Result<Self, QueryContractError> {
        let value = value.into();
        if value.is_empty() || value.len() > 1_024 {
            return Err(QueryContractError::InvalidTerminal {
                reason: "invalid schema fingerprint",
            });
        }
        Ok(Self(value))
    }

    /// Borrows the fingerprint.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for SchemaFingerprint {
    /// Deserializes a schema fingerprint while restoring its size invariants.
    ///
    /// # Errors
    /// Returns a deserializer error when the input is not text, empty, or
    /// exceeds the contract's maximum fingerprint length.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// Exact tenant and table binding for private tail access.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct TenantTableBinding {
    /// Authenticated data tenant.
    pub tenant_id: crate::DataTenantId,
    /// Non-empty table namespace.
    pub namespace: String,
    /// Non-empty table name.
    pub table: String,
}

/// Stable position within one Scribe writer stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct TailCursor {
    /// Writer boot epoch.
    pub writer_epoch: WriterEpoch,
    /// WAL sequence within the epoch.
    pub wal_lsn: WalLsn,
    /// Exact UUID batch identity.
    pub batch_id: uuid::Uuid,
    /// Stable row ordinal within the batch.
    pub row_ordinal: u32,
}

/// Identity of the fenced stream; cursors intentionally carry no node ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct TailStreamIdentity {
    /// Scribe node identity.
    pub node_id: NodeId,
    /// Writer boot epoch.
    pub writer_epoch: WriterEpoch,
}

/// Request to acquire one immutable Scribe tail fence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct AcquireTailFenceRequest {
    /// Query identity bound into the private signed tail ticket.
    pub query_id: uuid::Uuid,
    /// Tenant/table binding.
    pub binding: TenantTableBinding,
    /// UTC event-day string.
    pub event_day: EventDay,
    /// Exclusive sealed cursor.
    pub exclusive_sealed: TailCursor,
    /// Absolute execution deadline.
    pub deadline: DateTime<Utc>,
    /// Expected schema fingerprint.
    pub schema_fingerprint: SchemaFingerprint,
    /// Required tail protocol version.
    pub tail_protocol_version: u16,
}

/// Immutable interval and stream identity returned by Scribe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct TailReadFence {
    /// Fence identity.
    pub fence_id: TailFenceId,
    /// Tenant/table binding.
    pub binding: TenantTableBinding,
    /// UTC event-day string.
    pub event_day: EventDay,
    /// Fenced stream identity.
    pub stream: TailStreamIdentity,
    /// Exclusive sealed cursor.
    pub exclusive_sealed: TailCursor,
    /// Inclusive live cursor.
    pub inclusive_live: TailCursor,
    /// Exact schema fingerprint.
    pub schema_fingerprint: SchemaFingerprint,
    /// Tail protocol version.
    pub tail_protocol_version: u16,
    /// Fence expiry.
    pub expires_at: DateTime<Utc>,
}

/// Request for one bounded page inside a tail fence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct TailPageRequest {
    /// Query identity authorized to read the fence.
    pub query_id: uuid::Uuid,
    /// Fence identity.
    pub fence_id: TailFenceId,
    /// Cursor after which reading resumes.
    pub after: Option<TailCursor>,
    /// Maximum returned rows.
    pub max_rows: u32,
    /// Maximum encoded response bytes.
    pub max_encoded_bytes: u32,
}

/// One transport-owned Arrow IPC batch in a tail page.
pub type TailBatch = Vec<u8>;

/// One bounded page from an immutable tail fence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct TailPage {
    /// Owned Arrow IPC batches.
    pub batches: Vec<TailBatch>,
    /// Last included cursor when more data may follow.
    pub next: Option<TailCursor>,
    /// Whether the fence interval is exhausted.
    pub complete: bool,
}

/// Idempotent request to release one tail fence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ReleaseTailFenceRequest {
    /// Query identity authorized to release the fence.
    pub query_id: uuid::Uuid,
    /// Fence identity.
    pub fence_id: TailFenceId,
}

/// Fenced request to reserve worker slots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ReserveNodeSlotsRequest {
    /// Query identity.
    pub query_id: QueryId,
    /// Leader node identity.
    pub leader_node_id: NodeId,
    /// Leader Oracle-role fence.
    pub leader_fencing_token: FencingToken,
    /// Required admission class.
    pub query_class: QueryClass,
    /// Requested worker slots.
    pub slot_units: u32,
    /// Reservation expiry.
    pub expires_at: DateTime<Utc>,
}

/// Accepted pending worker reservation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PendingNodeReservation {
    /// Reservation identity.
    pub reservation_id: ReservationId,
    /// Reservation expiry.
    pub expires_at: DateTime<Utc>,
}

/// Capacity rejection with bounded caller backoff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ReservationRejected {
    /// Retry delay in milliseconds.
    pub retry_after_ms: u32,
}

/// Closed reservation outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum ReserveNodeSlotsResponse {
    /// Slots are pending ticket-bound execution.
    Pending(PendingNodeReservation),
    /// Node lacked capacity.
    Rejected(ReservationRejected),
}

/// Idempotent fenced reservation-release request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ReleaseNodeSlotsRequest {
    /// Reservation identity.
    pub reservation_id: ReservationId,
    /// Query identity.
    pub query_id: QueryId,
    /// Leader node identity.
    pub leader_node_id: NodeId,
    /// Leader Oracle-role fence.
    pub leader_fencing_token: FencingToken,
}

/// Signed opaque peer ticket verified before claims decoding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct SignedPeerTicket {
    /// ASCII signing-key identifier, at most 64 bytes.
    pub key_id: String,
    /// Opaque signed claims, at most 16 KiB.
    pub claims_bytes: Vec<u8>,
    /// Exact 64-byte signature.
    pub signature: Vec<u8>,
}

/// Ticket-bound worker fragment execution request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ExecuteFragmentRequest {
    /// Opaque signed ticket.
    pub ticket: SignedPeerTicket,
    /// Runtime-bounded fragment bytes.
    pub fragment_bytes: Vec<u8>,
    /// Pending reservation identity.
    pub reservation_id: ReservationId,
}

/// Verified worker footer for one completed attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct WorkerFooter {
    /// Fragment identity.
    pub fragment_id: String,
    /// Manifest digest.
    pub manifest_digest: QueryAuditDigest,
    /// Emitted row count.
    pub row_count: u64,
    /// Emitted encoded bytes.
    pub encoded_bytes: u64,
    /// Payload digest.
    pub payload_digest: QueryAuditDigest,
    /// Required completion marker.
    pub completed: bool,
}

/// Closed worker-attempt stream frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum WorkerAttemptFrame {
    /// Arrow IPC schema bytes.
    Schema(Vec<u8>),
    /// Arrow IPC record-batch bytes.
    Batch(Vec<u8>),
    /// Required terminal worker footer.
    Footer(WorkerFooter),
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
        assert_eq!(
            serde_json::from_value::<GetTraceRequest>(v).unwrap(),
            get_trace
        );

        let query_traces = QueryTracesRequest {
            window: window.clone(),
            service: Some("checkout".to_owned()),
            min_duration_ms: Some(250),
            status: Some("ERROR".to_owned()),
            name: Some("GET /checkout".to_owned()),
        };
        let v = serde_json::to_value(&query_traces).unwrap();
        assert_eq!(
            serde_json::from_value::<QueryTracesRequest>(v).unwrap(),
            query_traces
        );

        let query_recent = QueryRecentTracesRequest {
            window: window.clone(),
            service: Some("svc".to_owned()),
            status: None,
            min_duration_ms: Some(10),
        };
        let v = serde_json::to_value(&query_recent).unwrap();
        assert_eq!(
            serde_json::from_value::<QueryRecentTracesRequest>(v).unwrap(),
            query_recent
        );

        let query_genai = QueryGenAiRequest {
            window: window.clone(),
            conversation_id: Some("conv-1".to_owned()),
            model: Some("gpt-4o".to_owned()),
            provider: Some("openai".to_owned()),
        };
        let v = serde_json::to_value(&query_genai).unwrap();
        assert_eq!(
            serde_json::from_value::<QueryGenAiRequest>(v).unwrap(),
            query_genai
        );

        let query_eval = QueryEvalRequest {
            window: window.clone(),
            eval_id: Some("eval-abc".to_owned()),
            run_id: Some("run-1".to_owned()),
        };
        let v = serde_json::to_value(&query_eval).unwrap();
        assert_eq!(
            serde_json::from_value::<QueryEvalRequest>(v).unwrap(),
            query_eval
        );

        let query_drift = QueryDriftRequest {
            window: window.clone(),
            feature: Some("amount".to_owned()),
            run_id: Some("run-2".to_owned()),
        };
        let v = serde_json::to_value(&query_drift).unwrap();
        assert_eq!(
            serde_json::from_value::<QueryDriftRequest>(v).unwrap(),
            query_drift
        );

        let query_metrics = QueryMetricsRequest {
            window: window.clone(),
            metric_name: Some("request_latency".to_owned()),
            metric_type: Some("histogram".to_owned()),
        };
        let v = serde_json::to_value(&query_metrics).unwrap();
        assert_eq!(
            serde_json::from_value::<QueryMetricsRequest>(v).unwrap(),
            query_metrics
        );

        let query_logs = QueryLogsRequest {
            window: window.clone(),
            severity_number_min: Some(17),
            trace_id: Some("b7f3c1e2a4d5".to_owned()),
            event_name: Some("exception".to_owned()),
        };
        let v = serde_json::to_value(&query_logs).unwrap();
        assert_eq!(
            serde_json::from_value::<QueryLogsRequest>(v).unwrap(),
            query_logs
        );

        let query_agent = QueryAgentTracesRequest {
            window: window.clone(),
            dev_session_id: Some("sess-1".to_owned()),
            repo: Some("wyrd".to_owned()),
            commit_sha: Some("abc123".to_owned()),
            branch: Some("main".to_owned()),
            run_id: Some("run-3".to_owned()),
        };
        let v = serde_json::to_value(&query_agent).unwrap();
        assert_eq!(
            serde_json::from_value::<QueryAgentTracesRequest>(v).unwrap(),
            query_agent
        );

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
        assert_eq!(
            serde_json::from_value::<TraceWaterfall>(v).unwrap(),
            waterfall
        );

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
        assert_eq!(
            serde_json::from_value::<QueryTracesResponse>(v).unwrap(),
            traces_resp
        );

        let recent_resp = QueryRecentTracesResponse {
            rows: vec![],
            next_page_token: None,
        };
        let v = serde_json::to_value(&recent_resp).unwrap();
        assert_eq!(
            serde_json::from_value::<QueryRecentTracesResponse>(v).unwrap(),
            recent_resp
        );

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
        let genai_resp = QueryGenAiResponse {
            rows: vec![genai_row],
            next_page_token: None,
        };
        let v = serde_json::to_value(&genai_resp).unwrap();
        assert_eq!(
            serde_json::from_value::<QueryGenAiResponse>(v).unwrap(),
            genai_resp
        );

        let eval_row = EvalRow {
            eval_id: "eval-abc".to_owned(),
            run_id: "run-1".to_owned(),
            metric: "accuracy".to_owned(),
            score: 0.95,
            started_at: now,
        };
        let eval_resp = QueryEvalResponse {
            rows: vec![eval_row],
            next_page_token: None,
        };
        let v = serde_json::to_value(&eval_resp).unwrap();
        assert_eq!(
            serde_json::from_value::<QueryEvalResponse>(v).unwrap(),
            eval_resp
        );

        let drift_row = DriftRow {
            feature: "amount".to_owned(),
            run_id: Some("run-2".to_owned()),
            drift_score: 0.12,
            threshold: Some(0.1),
            computed_at: now,
        };
        let drift_resp = QueryDriftResponse {
            rows: vec![drift_row],
            next_page_token: None,
        };
        let v = serde_json::to_value(&drift_resp).unwrap();
        assert_eq!(
            serde_json::from_value::<QueryDriftResponse>(v).unwrap(),
            drift_resp
        );

        let metric_row = MetricRow {
            metric_name: "request_latency".to_owned(),
            metric_type: "histogram".to_owned(),
            value: 42.0,
            timestamp: now,
            attributes: None,
        };
        let metrics_resp = QueryMetricsResponse {
            rows: vec![metric_row],
            next_page_token: None,
        };
        let v = serde_json::to_value(&metrics_resp).unwrap();
        assert_eq!(
            serde_json::from_value::<QueryMetricsResponse>(v).unwrap(),
            metrics_resp
        );

        let log_row = LogRow {
            timestamp: now,
            severity_number: 17,
            severity_text: "ERROR".to_owned(),
            trace_id: Some("b7f3c1e2a4d5".to_owned()),
            span_id: None,
            event_name: None,
            body: None,
        };
        let logs_resp = QueryLogsResponse {
            rows: vec![log_row],
            next_page_token: None,
        };
        let v = serde_json::to_value(&logs_resp).unwrap();
        assert_eq!(
            serde_json::from_value::<QueryLogsResponse>(v).unwrap(),
            logs_resp
        );

        let agent_row = AgentTraceRow {
            dev_session_id: "sess-1".to_owned(),
            repo: "wyrd".to_owned(),
            commit_sha: Some("abc123".to_owned()),
            branch: Some("main".to_owned()),
            run_id: None,
            started_at: now,
            payload: None,
        };
        let agent_resp = QueryAgentTracesResponse {
            rows: vec![agent_row],
            next_page_token: None,
        };
        let v = serde_json::to_value(&agent_resp).unwrap();
        assert_eq!(
            serde_json::from_value::<QueryAgentTracesResponse>(v).unwrap(),
            agent_resp
        );

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

/// One audited data-plane operation.
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
    /// Optional typed, redacted operation detail used as the canonical hash preimage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<AuditDetail>,
}

impl AuditEvent {
    /// Constructs an audit event from the required operation fields.
    #[must_use]
    #[expect(
        clippy::too_many_arguments,
        reason = "the constructor mirrors the complete audit wire shape"
    )]
    pub fn new(
        request_id: RequestId,
        trace_id: Option<String>,
        operation: String,
        resource: String,
        card_ref: Option<CardRef>,
        principal_id: PrincipalId,
        principal_kind: PrincipalKindTag,
        auth_method: AuthMethod,
        permission: String,
        decision: AuditDecision,
        result: AuditResult,
        payload_summary: String,
    ) -> Self {
        Self {
            request_id,
            trace_id,
            operation,
            resource,
            card_ref,
            principal_id,
            principal_kind,
            auth_method,
            permission,
            decision,
            result,
            payload_summary,
            detail: None,
        }
    }

    /// Attach typed, already-redacted detail to this event.
    #[must_use]
    pub fn with_detail(mut self, detail: AuditDetail) -> Self {
        self.detail = Some(detail);
        self
    }
}

#[cfg(test)]
mod bifrost_wire_tests {
    //! Contract tests for the Arrow-free Bifrost wire types in `crate::vala::api`.

    use crate::vala::api::{
        BifrostTableDescription, BifrostTableEntry, DataTypeSpec, FieldSpec, PartitionColumnSpec,
        PartitionTransformWire, QueryParam, RegisterOutcome, RegisterTableRequest,
        RegisterTableResponse, SyncQueryRequest, TableStatus, TimeUnit,
    };
    use schemars::schema_for;

    /// Proves the removed asynchronous query-job contract family stays absent.
    #[test]
    fn async_query_contract_family_is_absent() {
        let production = include_str!("api.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("Vala API has a production section");
        for removed in [
            "struct JobUid",
            "enum AsyncJobState",
            "enum ExecutorAvailability",
            "struct AsyncQueryRequest",
            "struct AsyncQueryResponse",
            "struct AsyncQueryStatus",
        ] {
            assert!(
                !production.contains(removed),
                "removed asynchronous query contract returned: {removed}"
            );
        }
    }

    fn bifrost_wire_round_trip<T>(value: &T) -> T
    where
        T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        let json = serde_json::to_string(value).expect("serialize");
        let back: T = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*value, back, "round-trip mismatch");
        back
    }

    #[test]
    fn bifrost_wire_field_spec_round_trips_and_defaults_nullable_true() {
        let spec = FieldSpec {
            name: "value".to_string(),
            data_type: DataTypeSpec::Int64,
            nullable: false,
            metadata: Default::default(),
        };
        bifrost_wire_round_trip(&spec);

        // nullable defaults to true when absent; empty metadata is omitted on the wire.
        let json = serde_json::to_value(&spec).expect("serialize");
        assert!(
            json.get("metadata").is_none(),
            "empty metadata must be skipped"
        );

        let minimal: FieldSpec = serde_json::from_str(r#"{"name":"x","data_type":"Utf8"}"#)
            .expect("deserialize minimal");
        assert!(minimal.nullable, "nullable must default to true");
        assert!(minimal.metadata.is_empty());
    }

    #[test]
    fn bifrost_wire_field_spec_carries_correlation_metadata() {
        let mut spec = FieldSpec {
            name: "card_ref".to_string(),
            data_type: DataTypeSpec::Utf8,
            nullable: true,
            metadata: Default::default(),
        };
        spec.metadata
            .insert("wyrd:column_class".to_string(), "correlation".to_string());
        let back = bifrost_wire_round_trip(&spec);
        assert_eq!(
            back.metadata.get("wyrd:column_class").map(String::as_str),
            Some("correlation")
        );
    }

    #[test]
    fn bifrost_wire_register_request_defaults_partition() {
        let req: RegisterTableRequest =
            serde_json::from_str(r#"{"namespace":"vala.bifrost","name":"events","fields":[]}"#)
                .expect("deserialize");
        assert!(req.partition_columns.is_empty());
    }

    #[test]
    fn bifrost_wire_nested_and_recursive_data_types_round_trip() {
        let spec = FieldSpec {
            name: "nested".to_string(),
            data_type: DataTypeSpec::Struct(vec![
                FieldSpec {
                    name: "tags".to_string(),
                    data_type: DataTypeSpec::List(Box::new(DataTypeSpec::Utf8)),
                    nullable: true,
                    metadata: Default::default(),
                },
                FieldSpec {
                    name: "ts".to_string(),
                    data_type: DataTypeSpec::Timestamp {
                        unit: TimeUnit::Microsecond,
                        tz: Some("UTC".to_string()),
                    },
                    nullable: false,
                    metadata: Default::default(),
                },
            ]),
            nullable: true,
            metadata: Default::default(),
        };
        bifrost_wire_round_trip(&spec);
    }

    #[test]
    fn bifrost_wire_table_entry_and_description_round_trip() {
        let entry = BifrostTableEntry {
            namespace: "vala.bifrost".to_string(),
            name: "events".to_string(),
            table_uid: "ab".repeat(16),
            status: TableStatus::Active,
            fingerprint: "01".repeat(32),
            partition_columns: vec!["day".to_string()],
            registered_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let desc = BifrostTableDescription {
            entry: entry.clone(),
            fields: vec![FieldSpec {
                name: "value".to_string(),
                data_type: DataTypeSpec::Int64,
                nullable: false,
                metadata: Default::default(),
            }],
        };
        bifrost_wire_round_trip(&entry);
        bifrost_wire_round_trip(&desc);
    }

    #[test]
    fn bifrost_wire_query_types_round_trip() {
        bifrost_wire_round_trip(&SyncQueryRequest {
            sql: "SELECT 1".to_string(),
            params: vec![
                QueryParam::Null,
                QueryParam::Bool(true),
                QueryParam::Int(7),
                QueryParam::Float(1.5),
                QueryParam::Text("x".to_string()),
            ],
        });
    }

    #[test]
    fn bifrost_wire_register_response_and_partition_round_trip() {
        bifrost_wire_round_trip(&RegisterTableResponse {
            outcome: RegisterOutcome::Created,
            table_uid: "ab".repeat(16),
            fingerprint: "01".repeat(32),
        });
        bifrost_wire_round_trip(&PartitionColumnSpec {
            column: "day".to_string(),
            transform: PartitionTransformWire::Bucket { n: 16 },
        });
    }

    #[test]
    fn bifrost_wire_schema_for_wire_types_does_not_panic() {
        let _ = schema_for!(BifrostTableEntry);
        let _ = schema_for!(BifrostTableDescription);
        let _ = schema_for!(DataTypeSpec);
        let _ = schema_for!(FieldSpec);
        let _ = schema_for!(RegisterTableRequest);
        let _ = schema_for!(RegisterTableResponse);
        let _ = schema_for!(SyncQueryRequest);
        let _ = schema_for!(QueryParam);
    }

    /// Private tail and peer DTOs remain schema-generatable pure contracts.
    #[test]
    fn private_query_schema_types_do_not_panic() {
        let _ = schema_for!(super::AcquireTailFenceRequest);
        let _ = schema_for!(super::TailReadFence);
        let _ = schema_for!(super::TailPageRequest);
        let _ = schema_for!(super::TailPage);
        let _ = schema_for!(super::ReserveNodeSlotsRequest);
        let _ = schema_for!(super::ReserveNodeSlotsResponse);
        let _ = schema_for!(super::ReleaseNodeSlotsRequest);
        let _ = schema_for!(super::ExecuteFragmentRequest);
        let _ = schema_for!(super::WorkerAttemptFrame);
    }
}
