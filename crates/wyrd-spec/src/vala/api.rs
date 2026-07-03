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

use crate::auth::{PrincipalId, PrincipalKind};
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
    pub principal_kind: PrincipalKind,
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
