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

use crate::DataTenantId;
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
    /// The server-resolved canonical physical layout, fully populated.
    pub physical_layout: PhysicalLayoutWire,
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

/// Time-partition granularity on the wire.
///
/// Bifrost v1 partitions every table on `wyrd_event_time` and admits exactly
/// these two Iceberg-native transforms; nothing else is representable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TimeGranularityWire {
    /// One partition per UTC hour.
    Hour,
    /// One partition per UTC day.
    Day,
}

/// Sort direction of one declared physical sort key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum SortDirectionWire {
    /// Ascending.
    Asc,
    /// Descending.
    Desc,
}

/// Null placement of one declared physical sort key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum NullOrderWire {
    /// Nulls sort before non-null values.
    First,
    /// Nulls sort after non-null values.
    Last,
}

/// One declared physical sort key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct SortKeyWire {
    /// Sorted column; must exist in the complete physical schema.
    pub column: String,
    /// Sort direction.
    pub direction: SortDirectionWire,
    /// Null placement.
    pub null_order: NullOrderWire,
}

/// The authoritative physical layout of one Bifrost table.
///
/// The same shape is used three ways: as the optional declaration on
/// [`RegisterTableRequest`], as the canonical server-resolved layout returned by
/// [`BifrostTableDescription`], and as the exact JSON persisted in the control
/// row. In the latter two it is always fully populated: the sort order carries
/// at least one key and the Bloom list begins with the schema-present managed
/// floor.
///
/// The partition itself is system-owned. Every table is partitioned on
/// `wyrd_event_time`, so the declaration carries only the granularity and no
/// wire field names a partition column.
///
/// On the request path an omitted `sort_keys` and an explicit empty `sort_keys`
/// resolve identically, because the server injects nothing ahead of a declared
/// key and defaults an empty order to `wyrd_event_time` descending.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PhysicalLayoutWire {
    /// Required partition granularity on the managed `wyrd_event_time` column.
    pub partition_granularity: TimeGranularityWire,
    /// Ordered sort keys; at most four may be declared.
    #[serde(default)]
    pub sort_keys: Vec<SortKeyWire>,
    /// Ordered Bloom-filtered columns.
    #[serde(default)]
    pub bloom_columns: Vec<String>,
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
    /// Optional physical layout declaration.
    ///
    /// Omitting the field entirely resolves to `hour(wyrd_event_time)`,
    /// `wyrd_event_time` descending nulls-last, and the managed Bloom floor.
    /// Supplying the object requires `partition_granularity`; within it, an
    /// explicit empty `sort_keys` resolves exactly as an omitted one does, and
    /// an explicit empty `bloom_columns` means "managed floor only".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physical_layout: Option<PhysicalLayoutWire>,
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

/// The current non-terminal lifecycle state of an admitted Oracle query.
///
/// The state is projected by live controls only; completed queries are removed
/// from the in-memory registry and are not durable query jobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum RunningQueryLifecycleState {
    /// Admission completed and execution has not emitted progress yet.
    Admitted,
    /// The selected participant cut is executing.
    Running,
    /// A caller requested cancellation and owners are draining the cut.
    Cancelling,
}

/// Aggregate progress for one request-ID-keyed Oracle query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct RunningQueryProgress {
    /// Number of selected participants that have reported completion.
    pub completed_participants: u32,
    /// Exact number of participants frozen into the immutable cut.
    pub total_participants: u32,
}

/// Public summary for one active Oracle query.
///
/// This contract deliberately excludes SQL text, parameters, Arrow batches,
/// and result rows. A summary is tenant-scoped by the serving boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct RunningQuerySummary {
    /// Stable request identity used by live controls.
    pub request_id: RequestId,
    /// Server-derived admission class.
    pub query_class: QueryClass,
    /// Wall-clock admission time.
    pub started_at: DateTime<Utc>,
    /// Absolute query deadline.
    pub deadline: DateTime<Utc>,
    /// Current non-terminal lifecycle state.
    pub state: RunningQueryLifecycleState,
    /// Aggregate progress over the immutable participant cut.
    pub progress: RunningQueryProgress,
    /// Whether cancellation has been requested for this request identity.
    pub cancellation_requested: bool,
}

/// Response containing every active Oracle query visible to one tenant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ListRunningQueriesResponse {
    /// Active request summaries in deterministic request-ID order.
    pub queries: Vec<RunningQuerySummary>,
}

/// Request to read one active Oracle query by its public request identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct GetRunningQueryRequest {
    /// Request identity selected by the caller.
    pub request_id: RequestId,
}

/// Request to cancel one active Oracle query by its public request identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CancelRunningQueryRequest {
    /// Request identity selected by the caller.
    pub request_id: RequestId,
}

/// Idempotent cancellation result for one active Oracle query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CancelRunningQueryResponse {
    /// Request identity selected by the caller.
    pub request_id: RequestId,
    /// Whether this call changed the registry from active to cancelling.
    pub cancellation_started: bool,
}

/// Exact private role identity accepted for one immutable Oracle participant cut.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct OracleRoleFence {
    /// Physical node identity.
    pub node_id: NodeId,
    /// Exact role represented by this participant.
    pub role: ClusterRole,
    /// Exact role-incarnation fence.
    pub fencing_token: FencingToken,
}

/// Private owner-local lifecycle lookup scoped by authenticated tenant and request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct OracleLifecycleLookupRequest {
    /// Authenticated tenant that owns the request.
    pub tenant_id: DataTenantId,
    /// Public request identity used for all lifecycle controls.
    pub request_id: RequestId,
}

/// Private owner-local lifecycle list scoped only by authenticated tenant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ListOracleLifecyclesRequest {
    /// Authenticated tenant whose complete node-local registry is enumerated.
    pub tenant_id: DataTenantId,
}

/// Private owner-local listing for one tenant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ListOracleLifecyclesResponse {
    /// Matching owner-local query summaries.
    pub queries: Vec<RunningQuerySummary>,
}

/// Private owner-local get result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct GetOracleLifecycleResponse {
    /// Matching owner-local query summary.
    pub query: RunningQuerySummary,
}

/// Private cancellation signal tied to one tenant-qualified request identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CancelOracleLifecycleRequest {
    /// Authenticated tenant that owns the request.
    pub tenant_id: DataTenantId,
    /// Public request identity selected for cancellation.
    pub request_id: RequestId,
}

/// Private acknowledgement of a lifecycle cancellation signal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CancelOracleLifecycleResponse {
    /// Public request identity selected for cancellation.
    pub request_id: RequestId,
    /// Whether cancellation was newly requested by this message.
    pub cancellation_started: bool,
}

/// One durable accounting level used by delegated Oracle query admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum OracleAdmissionScopeKind {
    /// Cluster-wide class ceiling.
    Global,
    /// Tenant-local class ceiling.
    Tenant,
    /// Principal accounting that inherits the tenant ceiling.
    Principal,
}

/// A holder demand submitted to the background delegated-capacity allocator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct OracleAdmissionDemand {
    /// Tenant whose canonical policy applies.
    pub tenant_id: DataTenantId,
    /// Authenticated principal accounted within the tenant.
    pub principal_id: PrincipalId,
    /// Query class requested by local waiters.
    pub query_class: QueryClass,
    /// Exact configured number of units requested from each scope.
    pub requested_units: u32,
    /// Physical Oracle role holder.
    pub holder_node_id: NodeId,
    /// Exact Oracle role incarnation.
    pub holder_fencing_token: FencingToken,
}

/// Typed signal emitted once when delegated admission continuity is lost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct OracleAdmissionContinuityLost {
    /// Physical Oracle role whose cached blocks are closed.
    pub holder_node_id: NodeId,
    /// Exact role incarnation that lost continuity.
    pub holder_fencing_token: FencingToken,
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
    /// Arrow IPC end-of-stream delta closing the query's single IPC stream.
    ///
    /// The public query stream is one Arrow IPC stream split across Wyrd
    /// frames: the schema frame carries the stream prefix through exactly one
    /// schema message, each batch frame carries that write's exact delta, and
    /// this field carries the writer's `finish` delta. It is required for
    /// [`QueryTerminalOutcome::Success`] and [`QueryTerminalOutcome::Degraded`]
    /// and must be empty for [`QueryTerminalOutcome::Failed`], because a failed
    /// or cancelled stream never calls `finish` and therefore has no valid EOS
    /// to report. Arrow's own `StreamDecoder::finish` cannot prove an explicit
    /// EOS was received, so carrying the delta here is what makes an empty
    /// successful result (`Schema` then `Terminal`) unambiguous on the wire.
    pub arrow_ipc_eos: Vec<u8>,
}

impl QueryTerminalFrame {
    /// Validates closed terminal combinations for the selected visibility.
    ///
    /// The end-of-stream rule is part of this matrix rather than a separate
    /// check: a terminal that claims success without closing its Arrow IPC
    /// stream, or a failure that claims to have closed one it never finished,
    /// is as invalid as a mismatched source set.
    ///
    /// # Errors
    /// Returns [`QueryContractError`] for invalid cardinality, duplicate or
    /// missing sources, inconsistent outcome/freshness/error fields, or an
    /// Arrow IPC end-of-stream whose presence contradicts the outcome.
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
        if failed != self.arrow_ipc_eos.is_empty() {
            return Err(QueryContractError::InvalidTerminal {
                reason: if failed {
                    "failed terminal must not carry an Arrow IPC end-of-stream"
                } else {
                    "successful terminal must carry its Arrow IPC end-of-stream"
                },
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

    /// Exact Arrow IPC end-of-stream marker: one continuation token followed by
    /// a zero-length message, which is what `StreamWriter::finish` appends.
    const EOS: [u8; 8] = [0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0];

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
            arrow_ipc_eos: EOS.to_vec(),
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
            arrow_ipc_eos: EOS.to_vec(),
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
            arrow_ipc_eos: Vec::new(),
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
            arrow_ipc_eos: Vec::new(),
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
            arrow_ipc_eos: Vec::new(),
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
                if value.storage_protocol_version == 1
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

/// Validated time-partition value carried by private tail and follower
/// contracts.
///
/// The pair `(granularity, start_utc)` is the one durable partition identity in
/// Bifrost: WAL slices, seal keys, object paths, file-list rows, Forge audit
/// detail, and tail fences all carry exactly this value. `start_utc` is always
/// the exact UTC boundary of the partition, so two values are equal if and only
/// if they name the same physical partition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct TimePartitionWire {
    /// Partition granularity.
    granularity: TimeGranularityWire,
    /// Exact UTC start boundary of the partition.
    start_utc: DateTime<Utc>,
}

impl TimePartitionWire {
    /// Constructs one canonical partition value.
    ///
    /// # Errors
    /// Returns [`QueryContractError`] when `start_utc` is not the exact boundary
    /// of its granularity: minute, second, and sub-second components must be
    /// zero for [`TimeGranularityWire::Hour`], and the hour must also be zero for
    /// [`TimeGranularityWire::Day`].
    pub fn new(
        granularity: TimeGranularityWire,
        start_utc: DateTime<Utc>,
    ) -> Result<Self, QueryContractError> {
        if !partition_start_is_canonical(granularity, start_utc) {
            return Err(QueryContractError::InvalidTerminal {
                reason: "time partition start must be the exact UTC boundary of its granularity",
            });
        }
        Ok(Self {
            granularity,
            start_utc,
        })
    }

    /// Returns the partition granularity.
    #[must_use]
    pub const fn granularity(&self) -> TimeGranularityWire {
        self.granularity
    }

    /// Returns the exact UTC start boundary.
    #[must_use]
    pub const fn start_utc(&self) -> DateTime<Utc> {
        self.start_utc
    }

    /// Returns the signed epoch-microsecond start used by digests and protobuf.
    #[must_use]
    pub fn start_unix_micros(&self) -> i64 {
        self.start_utc.timestamp_micros()
    }

    /// Returns the durable one-byte granularity tag used by digests, WAL slices,
    /// and protobuf conversion. Tag `0` is reserved for "unspecified" and is
    /// never produced here.
    #[must_use]
    pub const fn granularity_tag(&self) -> u8 {
        match self.granularity {
            TimeGranularityWire::Hour => 1,
            TimeGranularityWire::Day => 2,
        }
    }

    /// Renders the canonical object-path segment pair, for example
    /// `partition_granularity=hour/partition_start=2026-08-23T14Z`.
    #[must_use]
    pub fn as_path_components(&self) -> String {
        format!(
            "partition_granularity={}/partition_start={}",
            self.granularity_str(),
            self.start_utc.format("%Y-%m-%dT%HZ")
        )
    }

    /// Returns the lower-case durable granularity token (`hour` or `day`).
    #[must_use]
    pub const fn granularity_str(&self) -> &'static str {
        match self.granularity {
            TimeGranularityWire::Hour => "hour",
            TimeGranularityWire::Day => "day",
        }
    }
}

impl PartialOrd for TimePartitionWire {
    /// Delegates to the total order defined by [`Ord`].
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TimePartitionWire {
    /// Orders by granularity tag first, then by start instant, so a sorted list
    /// never interleaves two granularities. Callers that need a comparable range
    /// must first prove both endpoints share one granularity.
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.granularity_tag()
            .cmp(&other.granularity_tag())
            .then(self.start_utc.cmp(&other.start_utc))
    }
}

impl std::fmt::Display for TimePartitionWire {
    /// Renders `<granularity>:<RFC 3339 start>` for logs and error text.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}",
            self.granularity_str(),
            self.start_utc.to_rfc3339()
        )
    }
}

impl<'de> Deserialize<'de> for TimePartitionWire {
    /// Deserializes a partition value while restoring its boundary invariant.
    ///
    /// # Errors
    /// Returns a deserializer error when the object is malformed or when the
    /// start instant is not the exact boundary of the declared granularity.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        /// Unvalidated mirror used only to reach the checked constructor.
        #[derive(Deserialize)]
        struct Raw {
            /// Declared granularity.
            granularity: TimeGranularityWire,
            /// Declared start instant.
            start_utc: DateTime<Utc>,
        }

        let raw = Raw::deserialize(deserializer)?;
        Self::new(raw.granularity, raw.start_utc).map_err(serde::de::Error::custom)
    }
}

/// Reports whether `start_utc` is the exact UTC boundary for `granularity`.
///
/// Shared by the checked constructor and by durable decoders that must reject a
/// noncanonical start before restoring any state.
#[must_use]
pub fn partition_start_is_canonical(
    granularity: TimeGranularityWire,
    start_utc: DateTime<Utc>,
) -> bool {
    use chrono::Timelike as _;

    let sub_hour_is_zero =
        start_utc.minute() == 0 && start_utc.second() == 0 && start_utc.nanosecond() == 0;
    match granularity {
        TimeGranularityWire::Hour => sub_hour_is_zero,
        TimeGranularityWire::Day => sub_hour_is_zero && start_utc.hour() == 0,
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
    /// Exact time partition the fence is bound to.
    pub time_partition: TimePartitionWire,
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
    /// Exact time partition the fence is bound to.
    pub time_partition: TimePartitionWire,
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

/// One signed persisted object a leader assigned to a follower.
///
/// A path string alone forces a follower to re-derive every other fact about
/// the object — which source it came from, how large it is, whether it can be
/// pruned — by querying the catalog a second time. That query is both a cost
/// and a correctness hazard: it observes a catalog that may have moved since
/// the leader pinned its cut.
///
/// The typed descriptor removes the re-query. The leader already resolved every
/// fact a follower needs at pin time, and the descriptor carries exactly those
/// facts inside the assignment-authority digest, so a follower that validates
/// the signature has validated the object's identity with it. The variant, not
/// a suffix on `scan_id`, is the authority for which source an object came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum PersistedFileDescriptor {
    /// A sealed object still unresolved in `vala.file_list`.
    Hot(HotFileDescriptor),
    /// A data file reachable from the pinned Iceberg snapshot's manifests.
    Iceberg(IcebergFileDescriptor),
}

impl PersistedFileDescriptor {
    /// Returns the catalog-pinned object path this descriptor names.
    #[must_use]
    pub fn path(&self) -> &str {
        match self {
            Self::Hot(hot) => &hot.path,
            Self::Iceberg(iceberg) => &iceberg.path,
        }
    }

    /// Returns the object's exact size in bytes.
    #[must_use]
    pub const fn size_bytes(&self) -> u64 {
        match self {
            Self::Hot(hot) => hot.size_bytes,
            Self::Iceberg(iceberg) => iceberg.size_bytes,
        }
    }

    /// Returns the object's exact record count.
    #[must_use]
    pub const fn row_count(&self) -> u64 {
        match self {
            Self::Hot(hot) => hot.row_count,
            Self::Iceberg(iceberg) => iceberg.row_count,
        }
    }

    /// Returns the declared inclusive event-time bounds, when both are present.
    ///
    /// A half-present or reversed pair yields `None`, which is the descriptor's
    /// only representation of "retain this file": a follower never invents an
    /// interval from a partial one.
    #[must_use]
    pub const fn event_time_micros(&self) -> Option<(i64, i64)> {
        let (min, max) = match self {
            Self::Hot(hot) => (hot.min_event_time_micros, hot.max_event_time_micros),
            Self::Iceberg(iceberg) => {
                (iceberg.min_event_time_micros, iceberg.max_event_time_micros)
            }
        };
        match (min, max) {
            (Some(min), Some(max)) if min <= max => Some((min, max)),
            _ => None,
        }
    }

    /// Returns whether this descriptor carries a well-formed immutable identity.
    ///
    /// The minimal identity differs by source because the authorities differ: a
    /// hot object is identified by its `vala.file_list` row and the writer's
    /// decoded SHA-256, while a pinned Iceberg object is identified by the
    /// snapshot its manifest belongs to. Both require a canonical nonempty path
    /// and a positive size, and both require an event-time pair that is either
    /// wholly absent or ordered.
    ///
    /// A zero-row file is valid: an empty object still participates in residual
    /// execution. Whether the object exists, belongs to the tenant, or matches
    /// the schema is decided by the binding and ticket checks that run before
    /// this value is used, not here.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        let (min, max) = match self {
            Self::Hot(hot) => {
                if hot.file_list_id.is_nil() {
                    return false;
                }
                (hot.min_event_time_micros, hot.max_event_time_micros)
            }
            Self::Iceberg(iceberg) => {
                if iceberg.snapshot_id <= 0 {
                    return false;
                }
                (iceberg.min_event_time_micros, iceberg.max_event_time_micros)
            }
        };
        let bounds_valid = match (min, max) {
            (None, None) => true,
            (Some(min), Some(max)) => min <= max,
            _ => false,
        };
        !self.path().is_empty() && self.size_bytes() > 0 && bounds_valid
    }
}

/// One sealed hot object assigned from `vala.file_list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct HotFileDescriptor {
    /// Catalog-pinned object location.
    pub path: String,
    /// Exact object size in bytes, always positive.
    pub size_bytes: u64,
    /// Exact record count the writer sealed into this object.
    pub row_count: u64,
    /// Identity of the `vala.file_list` row that declared this object.
    pub file_list_id: uuid::Uuid,
    /// Writer-recorded object checksum, already decoded from its durable hex
    /// form. It is carried decoded because it is cache identity, not a display
    /// value: two distinct objects that shared a cache key would return one
    /// object's footer for the other's rows.
    #[serde(with = "sha256_hex")]
    #[schemars(with = "String")]
    pub sha256: [u8; 32],
    /// Inclusive lower `wyrd_event_time` bound in epoch microseconds.
    pub min_event_time_micros: Option<i64>,
    /// Inclusive upper `wyrd_event_time` bound in epoch microseconds.
    pub max_event_time_micros: Option<i64>,
}

/// One pinned Iceberg data file assigned from the snapshot's manifests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct IcebergFileDescriptor {
    /// Catalog-pinned object location.
    pub path: String,
    /// Exact object size in bytes from the pinned manifest, always positive.
    pub size_bytes: u64,
    /// Exact record count from the pinned manifest entry.
    pub row_count: u64,
    /// Pinned snapshot the manifest carrying this file belongs to, always
    /// positive. It is the publication authority for the object: the same path
    /// under a different snapshot is a different immutable file.
    pub snapshot_id: i64,
    /// Inclusive lower `wyrd_event_time` bound in epoch microseconds.
    pub min_event_time_micros: Option<i64>,
    /// Inclusive upper `wyrd_event_time` bound in epoch microseconds.
    pub max_event_time_micros: Option<i64>,
}

/// Serializes a decoded object checksum as lowercase hex on the wire.
///
/// The durable `vala.file_list` column and every operator-facing surface use
/// the 64-character hex form, so the wire keeps it; only in-memory identity
/// comparisons use the decoded bytes.
mod sha256_hex {
    use serde::{Deserialize as _, Deserializer, Serializer};

    /// Emits the 64-character lowercase hex form of a decoded checksum.
    ///
    /// # Errors
    /// Returns the serializer's own error when the string cannot be emitted.
    pub(super) fn serialize<S: Serializer>(
        value: &[u8; 32],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&hex::encode(value))
    }

    /// Decodes exactly 32 bytes from the 64-character lowercase hex form.
    ///
    /// # Errors
    /// Returns a deserializer error when the value is not valid hex or does not
    /// decode to exactly 32 bytes; a shorter or longer checksum is a different
    /// identity domain, never a truncation to tolerate.
    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<[u8; 32], D::Error> {
        let encoded = String::deserialize(deserializer)?;
        let decoded = hex::decode(&encoded).map_err(serde::de::Error::custom)?;
        <[u8; 32]>::try_from(decoded.as_slice())
            .map_err(|_| serde::de::Error::custom("object checksum must decode to 32 bytes"))
    }
}

/// Explicit persisted-file assignment for one physical-plan scan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PersistedFileAssignment {
    /// Signed typed descriptors assigned to this follower, in order.
    pub files: Vec<PersistedFileDescriptor>,
}

/// Immutable Scribe memory-provider cut carried by the private follower wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ScribeProviderCut {
    /// Exact writer epoch selected from the signed participant incarnation.
    pub writer_epoch: u64,
    /// Inclusive first partition in the provider projection.
    pub start_partition: TimePartitionWire,
    /// Inclusive final partition in the provider projection.
    pub end_partition: TimePartitionWire,
    /// Maximum Arrow batches retained by the provider.
    pub maximum_batch_count: u32,
    /// Maximum bytes retained by the provider.
    pub maximum_retained_bytes: u64,
}

impl ScribeProviderCut {
    /// Validates the canonical Scribe memory-provider cut.
    ///
    /// The cut bounds which partitions a follower may read from memory, on which
    /// writer incarnation, and how much it may retain. It carries no statement
    /// about which rows are already published: Scribe decides that from the
    /// generation authority it owns, and a WAL interval on the wire would be a
    /// second, weaker answer that a reader could mistake for ownership.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.writer_epoch > 0
            && self.start_partition.granularity() == self.end_partition.granularity()
            && self.start_partition <= self.end_partition
            && self.maximum_batch_count > 0
            && self.maximum_retained_bytes > 0
    }
}

/// One scan-keyed role-local follower assignment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct FollowerScanAssignment {
    /// Stable identifier encoded in the physical extension node.
    pub scan_id: String,
    /// Authenticated tenant/table binding for this scan.
    pub binding: TenantTableBinding,
    /// Required wrapper preserving explicit-empty persisted semantics.
    pub persisted: PersistedFileAssignment,
    /// Scribe memory-provider cut, present only for a Scribe target.
    pub scribe_provider_cut: Option<ScribeProviderCut>,
    /// Schema fingerprint bound to the encoded placeholder.
    pub schema_fingerprint: String,
    /// Required output/predicate/hidden-tenant projection closure, in the
    /// stable order the leaf union and remote placeholder must expose.
    pub required_columns: Vec<String>,
    /// Closed leaf predicates pushed to this assignment's readers, in filter
    /// order. Recognized predicates are always `Inexact`; DataFusion retains
    /// its own residual filter above the table provider regardless.
    pub predicates: Vec<crate::vala::assignment_authority::ScanPredicate>,
}

/// Ticket-bound worker request carrying one serialized physical subtree.
///
/// This is the sole domain projection of the private
/// `wyrd.v1.ExecuteFragmentRequest` peer message. The leader mints it per
/// follower after splitting the admitted plan; the follower verifies the
/// ticket, both fences, and the assignment-authority digest before it
/// deserializes `physical_plan_bytes` and substitutes each remote placeholder
/// with its role-local source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ExecuteFragmentRequest {
    /// Opaque signed ticket.
    pub ticket: SignedPeerTicket,
    /// Runtime-bounded physical-plan bytes.
    pub physical_plan_bytes: Vec<u8>,
    /// Pending reservation identity.
    pub reservation_id: ReservationId,
    /// Signed request-local leader incarnation.
    pub leader_fence: OracleRoleFence,
    /// Signed target follower incarnation.
    pub target_fence: OracleRoleFence,
    /// Complete scan-keyed role-local assignment set.
    pub assignments: Vec<FollowerScanAssignment>,
    /// Fingerprint shared by every follower in this attempt.
    pub plan_fingerprint: String,
}

/// Physical scan evidence one follower accumulated while executing a fragment.
///
/// The leader of a distributed query scans no storage of its own: every leaf of
/// its plan is a remote scan, so its local scan metrics are legitimately empty.
/// Followers report what their executed scans actually touched and the leader
/// sums these across the participant cut, which is the only way a distributed
/// query can report the same scan families a single-node query reports.
///
/// `bytes_scanned` is physical read volume reported by the executed scan. It is
/// deliberately distinct from [`WorkerFooter::encoded_bytes`], which is the
/// Arrow transport size of the rows sent back; projection, predicate pushdown,
/// and compression make the two unrelated, and substituting one for the other
/// would make the reported scan volume wrong rather than absent.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct WorkerScanStats {
    /// Physical bytes the follower's executed scans read, when its sources
    /// report physical IO at all. `None` means unavailable, not zero: a
    /// memory-backed source reads no storage and must not be reported as a
    /// zero-byte scan of one that does.
    pub bytes_scanned: Option<u64>,
    /// Number of files represented by the follower's executed scan nodes.
    pub files_scanned: u64,
    /// Number of file partitions represented by the follower's executed scans.
    pub partitions_scanned: u64,
    /// Row groups the follower retained after closed-predicate statistics
    /// pruning. Observable only on the follower: the leader's plan carries a
    /// remote placeholder in place of the executed scan leaf.
    pub row_groups_scanned: u64,
    /// Row groups the follower excluded by closed-predicate statistics
    /// pruning, reported alongside `row_groups_scanned` so an operator can see
    /// how much a pushed-down predicate actually saved.
    pub row_groups_pruned: u64,
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
    /// Physical scan evidence the leader aggregates across the participant cut.
    pub scan_stats: WorkerScanStats,
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

    /// Terminal Arrow IPC end-of-stream presence is closed over the outcome.
    ///
    /// The public query stream is one Arrow IPC stream split across frames, so
    /// the terminal is the only place its `finish` delta can travel. This pins
    /// the whole matrix: `Success` and `Degraded` must carry it — including the
    /// empty result, whose stream is `Schema` then `Terminal` and would
    /// otherwise be indistinguishable from a truncated one — and `Failed` must
    /// not, because a failed or cancelled stream never calls `finish`.
    #[test]
    fn query_terminal_eos_contract() {
        /// Exact `StreamWriter::finish` delta: continuation token, zero length.
        const EOS: [u8; 8] = [0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0];

        let sources = vec![
            SourceCompletion {
                source: QuerySource::Iceberg,
                outcome: SourceCompletionOutcome::Complete,
            },
            SourceCompletion {
                source: QuerySource::HotSealed,
                outcome: SourceCompletionOutcome::Complete,
            },
        ];
        let base = QueryTerminalFrame {
            outcome: QueryTerminalOutcome::Success,
            freshness: QueryFreshness::Complete,
            row_count: 0,
            warnings: vec![],
            source_completion: sources,
            error: None,
            arrow_ipc_eos: EOS.to_vec(),
        };

        // Empty success is the case the previous byteless terminal could not
        // express: no batch frame ever carried an EOS, so the terminal must.
        base.validate(VisibilityMode::PublishedOnly)
            .expect("empty success carries its end-of-stream");
        base.validate_emitted_rows(0)
            .expect("empty success emitted no rows");
        assert!(
            QueryTerminalFrame {
                arrow_ipc_eos: Vec::new(),
                ..base.clone()
            }
            .validate(VisibilityMode::PublishedOnly)
            .is_err(),
            "success without an end-of-stream must be rejected"
        );

        let mut degraded_sources = base.source_completion.clone();
        degraded_sources.push(SourceCompletion {
            source: QuerySource::LiveTail,
            outcome: SourceCompletionOutcome::Unavailable,
        });
        let degraded = QueryTerminalFrame {
            outcome: QueryTerminalOutcome::Degraded,
            freshness: QueryFreshness::Degraded,
            warnings: vec![QueryWarning::LiveTailUnavailable],
            source_completion: degraded_sources,
            ..base.clone()
        };
        degraded
            .validate(VisibilityMode::Fused)
            .expect("degraded still finishes its Arrow stream");
        assert!(
            QueryTerminalFrame {
                arrow_ipc_eos: Vec::new(),
                ..degraded
            }
            .validate(VisibilityMode::Fused)
            .is_err(),
            "degraded without an end-of-stream must be rejected"
        );

        let failed = QueryTerminalFrame {
            outcome: QueryTerminalOutcome::Failed,
            error: Some(QueryTerminalError {
                code: QueryTerminalErrorCode::QueryExecutionFailed,
                detail: None,
            }),
            arrow_ipc_eos: Vec::new(),
            ..base.clone()
        };
        failed
            .validate(VisibilityMode::PublishedOnly)
            .expect("a failed terminal closes nothing");
        assert!(
            QueryTerminalFrame {
                arrow_ipc_eos: EOS.to_vec(),
                ..failed
            }
            .validate(VisibilityMode::PublishedOnly)
            .is_err(),
            "a failed terminal must not claim an end-of-stream"
        );

        let encoded = serde_json::to_value(&base).expect("terminal serializes");
        assert_eq!(
            serde_json::from_value::<QueryTerminalFrame>(encoded).expect("terminal deserializes"),
            base
        );
    }

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

    /// Running-query controls retain one request identity and SQL-free state.
    ///
    /// # Panics
    ///
    /// Panics when a lifecycle contract cannot serialize or deserialize, or
    /// when its JSON round-trip changes the value.
    #[test]
    fn running_query_contract_round_trips() {
        let request_id = RequestId::now_v7();
        let started_at = DateTime::from_timestamp_millis(1_725_000_000_123)
            .expect("fixed running-query timestamp is valid");
        let summary = RunningQuerySummary {
            request_id: request_id.clone(),
            query_class: QueryClass::Interactive,
            started_at,
            deadline: started_at + chrono::Duration::seconds(30),
            state: RunningQueryLifecycleState::Running,
            progress: RunningQueryProgress {
                completed_participants: 1,
                total_participants: 2,
            },
            cancellation_requested: false,
        };
        assert_json_round_trip(&summary);
        assert_json_round_trip(&ListRunningQueriesResponse {
            queries: vec![summary.clone()],
        });
        assert_json_round_trip(&GetRunningQueryRequest {
            request_id: request_id.clone(),
        });
        assert_json_round_trip(&CancelRunningQueryRequest {
            request_id: request_id.clone(),
        });
        assert_json_round_trip(&CancelRunningQueryResponse {
            request_id: request_id.clone(),
            cancellation_started: true,
        });

        let tenant_id = DataTenantId::new_v7();
        assert_json_round_trip(&ListOracleLifecyclesRequest { tenant_id });
        assert_json_round_trip(&ListOracleLifecyclesResponse {
            queries: vec![summary.clone()],
        });
        assert_json_round_trip(&OracleLifecycleLookupRequest {
            tenant_id,
            request_id: request_id.clone(),
        });
        assert_json_round_trip(&GetOracleLifecycleResponse {
            query: summary.clone(),
        });
        assert_json_round_trip(&CancelOracleLifecycleRequest {
            tenant_id,
            request_id: request_id.clone(),
        });
        assert_json_round_trip(&CancelOracleLifecycleResponse {
            request_id,
            cancellation_started: true,
        });

        let _ = schemars::schema_for!(RunningQuerySummary);
        let _ = schemars::schema_for!(ListRunningQueriesResponse);
        let _ = schemars::schema_for!(GetRunningQueryRequest);
        let _ = schemars::schema_for!(CancelRunningQueryRequest);
        let _ = schemars::schema_for!(CancelRunningQueryResponse);
        let _ = schemars::schema_for!(ListOracleLifecyclesRequest);
        let _ = schemars::schema_for!(ListOracleLifecyclesResponse);
        let _ = schemars::schema_for!(OracleLifecycleLookupRequest);
        let _ = schemars::schema_for!(GetOracleLifecycleResponse);
        let _ = schemars::schema_for!(CancelOracleLifecycleRequest);
        let _ = schemars::schema_for!(CancelOracleLifecycleResponse);
    }

    /// Proves one pure contract survives a complete JSON encode/decode cycle.
    ///
    /// # Panics
    ///
    /// Panics when serialization or deserialization fails, or when the decoded
    /// contract differs from its source value.
    fn assert_json_round_trip<T>(expected: &T)
    where
        T: serde::Serialize + serde::de::DeserializeOwned + std::fmt::Debug + PartialEq,
    {
        let value = serde_json::to_value(expected).expect("lifecycle contract serializes");
        let actual: T = serde_json::from_value(value).expect("lifecycle contract deserializes");
        assert_eq!(&actual, expected);
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
        BifrostTableDescription, BifrostTableEntry, DataTypeSpec, FieldSpec, NullOrderWire,
        PhysicalLayoutWire, QueryParam, RegisterOutcome, RegisterTableRequest,
        RegisterTableResponse, SortDirectionWire, SortKeyWire, SyncQueryRequest, TableStatus,
        TimeGranularityWire, TimeUnit,
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

    /// An omitted `physical_layout` stays `None` on the wire so the server can
    /// distinguish "no declaration" (defaults apply) from an explicit object.
    #[test]
    fn bifrost_wire_register_request_omits_physical_layout() {
        let req: RegisterTableRequest =
            serde_json::from_str(r#"{"namespace":"vala.bifrost","name":"events","fields":[]}"#)
                .expect("deserialize");
        assert!(req.physical_layout.is_none());
        let json = serde_json::to_string(&req).expect("serialize");
        assert!(!json.contains("physical_layout"), "{json}");
    }

    /// An explicit object carrying only `partition_granularity` decodes with
    /// empty lists and keeps them through a round trip.
    ///
    /// The wire distinction between `None` and an explicit object survives; what
    /// the server no longer distinguishes is an empty `sort_keys` from an
    /// omitted one, which both resolve to the canonical event-time default.
    #[test]
    fn bifrost_wire_register_request_keeps_explicit_empty_layout_lists() {
        let req: RegisterTableRequest = serde_json::from_str(
            r#"{"namespace":"vala.bifrost","name":"events","fields":[],
                "physical_layout":{"partition_granularity":"hour",
                "sort_keys":[],"bloom_columns":[]}}"#,
        )
        .expect("deserialize");
        let layout = req.physical_layout.as_ref().expect("explicit layout");
        assert_eq!(layout.partition_granularity, TimeGranularityWire::Hour);
        assert!(layout.sort_keys.is_empty());
        assert!(layout.bloom_columns.is_empty());
        bifrost_wire_round_trip(&req);
    }

    /// No wire field names a partition column any more, so a declaration that
    /// carries one is rejected rather than silently ignored.
    #[test]
    fn bifrost_wire_physical_layout_has_no_partition_column_field() {
        let layout = PhysicalLayoutWire {
            partition_granularity: TimeGranularityWire::Hour,
            sort_keys: Vec::new(),
            bloom_columns: Vec::new(),
        };
        let json = serde_json::to_value(&layout).expect("serialize");
        let object = json.as_object().expect("layout is a JSON object");
        let mut keys = object.keys().map(String::as_str).collect::<Vec<_>>();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec!["bloom_columns", "partition_granularity", "sort_keys"]
        );
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
            physical_layout: PhysicalLayoutWire {
                partition_granularity: TimeGranularityWire::Hour,
                sort_keys: vec![SortKeyWire {
                    column: "wyrd_event_time".to_string(),
                    direction: SortDirectionWire::Desc,
                    null_order: NullOrderWire::Last,
                }],
                bloom_columns: vec!["run_id".to_string()],
            },
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
    fn bifrost_wire_register_response_and_layout_round_trip() {
        bifrost_wire_round_trip(&RegisterTableResponse {
            outcome: RegisterOutcome::Created,
            table_uid: "ab".repeat(16),
            fingerprint: "01".repeat(32),
        });
        bifrost_wire_round_trip(&PhysicalLayoutWire {
            partition_granularity: TimeGranularityWire::Day,
            sort_keys: vec![SortKeyWire {
                column: "wyrd_event_time".to_string(),
                direction: SortDirectionWire::Desc,
                null_order: NullOrderWire::Last,
            }],
            bloom_columns: vec!["run_id".to_string()],
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
