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
use uuid::Uuid;

use crate::DataTenantId;
use crate::auth::{Attribution, PrincipalId, PrincipalKindTag};
use crate::error::WyrdProblem;
use crate::reference::CardRef;
use crate::request_id::RequestId;
pub use crate::vala::audit_detail::{
    AuditDelegationStep, AuditDetail, AuditDetailValueError, BatchId, BifrostSecurityPhase,
    BifrostSecurityViolationKind, ForgeIcebergRewritePhase, ForgeManifestRewritePhase,
    ForgeOrphanGcPhase, ForgePromotedFile, ForgePromotedFileSetDigest, ForgeScribePromotionPhase,
    ForgeSnapshotExpirePhase, OracleTableProtectionPhase, QueryAuditDigest, QueryExecutionMode,
    ScopeHash, StoragePath, audit_detail_canonical_json,
};
use crate::vala::ids::TableVersion;

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
    /// The table version this schema and layout describe.
    ///
    /// A writer sends it with every batch built from this description; a
    /// batch naming a version other than the table's current one is refused.
    pub table_version: TableVersion,
    /// The table's own declared columns, in stored order.
    ///
    /// Server-owned correlation and managed columns are not here; they are
    /// reported separately so a writer can tell what it declares from what the
    /// server appends.
    pub user_fields: Vec<FieldSpec>,
    /// The correlation inputs a writer supplies alongside its user fields.
    ///
    /// Exactly nullable `subject_card_uid: Utf8` then nullable `run_id: Utf8`, each
    /// with the field id its physical column holds.
    ///
    /// Card correlation is optional, so a writer may omit `subject_card_uid` entirely;
    /// the server then stores the row with its authenticated subject/actor
    /// pair and a null `subject_card_uid`. A supplied UID is stored only once the server has
    /// confirmed the writer may correlate to that Card.
    pub correlation_fields: Vec<FieldSpec>,
    /// Lower-case 64-character hex of the complete canonical physical
    /// fingerprint, present only for a canonical `OTel` signal built-in.
    ///
    /// [`BifrostTableEntry::fingerprint`] keeps its user-schema meaning for
    /// every table; this is the separate whole-physical-schema identity, and it
    /// is omitted for a dynamically registered table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "crate::schema::skipped_option::<String>")]
    pub canonical_physical_fingerprint: Option<String>,
    /// The server-resolved canonical physical layout, fully populated.
    pub physical_layout: PhysicalLayoutWire,
    /// The Forge compaction file target the table's definition stores, in
    /// bytes.
    ///
    /// The registered target, or the deployment default resolved when the
    /// table was created; a later deployment change does not alter it.
    /// Omitted only for a definition that stores no
    /// `write.target-file-size-bytes`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "crate::schema::skipped_option::<u64>")]
    pub compaction_target_file_size_bytes: Option<u64>,
    /// The table's explicit Forge compaction type.
    ///
    /// Present only when the table stores its own `wyrd.forge.compaction.type`;
    /// omitted means Forge compacts it `small-files`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "crate::schema::skipped_option::<CompactionTypeWire>")]
    pub compaction_type: Option<CompactionTypeWire>,
}

/// Physical compaction strategy a table asks Forge to apply.
///
/// The public wire spelling is hyphenated (`auto`, `full`, `small-files`,
/// `files-with-delete`), matching the table's stored
/// `wyrd.forge.compaction.type` Iceberg property; underscore spellings are
/// rejected. A table that names no type compacts `small-files`, and a
/// copy-on-write table always compacts `full` whatever it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "kebab-case")]
pub enum CompactionTypeWire {
    /// Choose a delete-heavy or small-file plan from table-wide thresholds.
    Auto,
    /// Rewrite every live data file.
    Full,
    /// Rewrite only data files below the small-file threshold; the default
    /// when none is declared.
    SmallFiles,
    /// Rewrite only data files with associated delete files.
    FilesWithDelete,
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
///
/// A parameterized variant refuses unknown members, so a misspelled
/// parameter fails decoding instead of being dropped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
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
        ///
        /// Omitted when `None`; an explicit `null` is refused.
        #[serde(
            default,
            deserialize_with = "present",
            skip_serializing_if = "Option::is_none"
        )]
        #[schemars(with = "String")]
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
    /// Variable-length list whose boxed element declaration carries the child's
    /// name, nullability, metadata, and stable field id.
    ///
    /// The element is a full [`FieldSpec`] rather than a bare type so a nested
    /// child keeps the same identity the physical schema stores; a client that
    /// rebuilds an Arrow schema from a description reproduces the stored list
    /// child exactly instead of inventing a nullable `item`.
    // `no_recursion` stops OpenAPI schema collection from re-entering
    // `FieldSpec`; both variants still reference it by name.
    #[cfg_attr(feature = "server", schema(no_recursion))]
    List(Box<FieldSpec>),
    /// Nested struct of named fields.
    #[cfg_attr(feature = "server", schema(no_recursion))]
    Struct(Vec<FieldSpec>),
    /// Unordered key/value entries, the Iceberg `map` type.
    ///
    /// Key and value are full [`FieldSpec`]s for the same reason as
    /// [`Self::List`]'s element: each keeps its name, nullability, metadata,
    /// and stable field id. An Iceberg map key is always required, so register
    /// stores a key declared nullable as non-nullable.
    #[cfg_attr(feature = "server", schema(no_recursion))]
    Map {
        /// Declaration of every entry's key.
        key: Box<FieldSpec>,
        /// Declaration of every entry's value.
        value: Box<FieldSpec>,
    },
    /// Self-describing semi-structured value stored in the Parquet/Iceberg
    /// Variant encoding.
    ///
    /// On the Arrow wire it is the canonical `arrow.parquet.variant` extension
    /// type over a `metadata`/`value` binary struct. Each value keeps its own
    /// type: integers stay integers, objects keep missing keys distinct from
    /// null ones. Values are bounded by [`VARIANT_MAX_DEPTH`] and
    /// [`VARIANT_MAX_ENCODED_BYTES`].
    Variant,
}

/// Maximum Variant nesting depth, counting the root container as one.
///
/// A fixed contract constant, not configuration: a deeper value is refused
/// with `WYRD_VALA_400_VARIANT_TOO_DEEP`.
pub const VARIANT_MAX_DEPTH: u32 = 64;

/// Maximum canonical encoded Variant bytes, metadata plus value.
///
/// A fixed contract constant, not configuration: a larger value is refused
/// with `WYRD_VALA_413_VARIANT_TOO_LARGE` before queue reservation or durable
/// write.
pub const VARIANT_MAX_ENCODED_BYTES: u64 = 8_388_608;

/// Arrow-free field declaration. Follows the `card::field::FieldSpec` precedent
/// but carries a typed [`DataTypeSpec`] instead of a loose dtype string.
///
/// Unknown members are refused, so a misspelled key fails decoding instead of
/// being dropped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct FieldSpec {
    /// Column name.
    pub name: String,
    /// Logical column type.
    pub data_type: DataTypeSpec,
    /// Whether the column is nullable. Defaults to `true`.
    #[serde(default = "default_true")]
    pub nullable: bool,
    /// String metadata carried verbatim through every schema conversion.
    ///
    /// [`PARQUET_FIELD_ID_KEY`] is meaningful to Bifrost: it holds the stored
    /// stable field id as decimal text.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

/// Metadata key holding one field's stable id, as decimal text.
///
/// This is the same key Parquet and Iceberg read a field id from, so a
/// description round-trips through Arrow without a second id convention.
pub const PARQUET_FIELD_ID_KEY: &str = "PARQUET:field_id";

/// The [`DataTypeSpec::Timestamp`] zone of every stored zoned timestamp.
///
/// A zoned timestamp is one UTC instant; its zone only labels how to display
/// it. Iceberg stores it as `timestamptz` and reads it back with this label,
/// so a table description, a built-in declaration, and a read all spell it
/// this way. A writer may send any zone label: Bifrost converts it to this one
/// on ingest without changing the instant.
pub const UTC_TIME_ZONE: &str = "+00:00";

/// Time-partition granularity of a private partition identity.
///
/// The time component of every partition is exactly one of these two
/// Iceberg-native transforms of the table's time column.
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

/// One declared physical sort key; unknown members are refused.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct SortKeyWire {
    /// Sorted column; must exist in the complete physical schema.
    pub column: String,
    /// Sort direction.
    pub direction: SortDirectionWire,
    /// Null placement.
    pub null_order: NullOrderWire,
}

/// Transform one declared partition field applies to its source column.
///
/// `hour` and `day` partition the table's required UTC time column; the
/// `identity`, `bucket`, and `truncate` transforms form at most two additional
/// dimensions over `Utf8`, `Int32`, or `Int64` columns. Each follows Iceberg's
/// transform semantics, including null preservation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum PartitionTransformWire {
    /// One partition per UTC hour of the time column.
    Hour,
    /// One partition per UTC day of the time column.
    Day,
    /// One partition per distinct source value, null included.
    Identity,
    /// Iceberg's Murmur3 hash bucket of the source value; requires `buckets`.
    Bucket,
    /// Iceberg truncation of the source value; requires `width`.
    Truncate,
}

/// One declared partition field.
///
/// `buckets` is required for and only accepted on a `bucket` transform (2
/// through 1024); `width` is required for and only accepted on a `truncate`
/// transform (a positive integer counting Unicode code points for strings).
/// Each parameter is either omitted or carries a value; an explicit `null` is
/// refused.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct PartitionFieldWire {
    /// Top-level source column of the complete physical schema.
    pub column: String,
    /// Transform applied to `column`.
    pub transform: PartitionTransformWire,
    /// Bucket count of a `bucket` transform.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "u32")]
    pub buckets: Option<u32>,
    /// Truncation width of a `truncate` transform.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "u32")]
    pub width: Option<u32>,
}

impl PartitionFieldWire {
    /// Builds one parameterless partition field.
    #[must_use]
    pub fn new(column: impl Into<String>, transform: PartitionTransformWire) -> Self {
        Self {
            column: column.into(),
            transform,
            buckets: None,
            width: None,
        }
    }
}

/// The authoritative physical layout of one Bifrost table.
///
/// The same shape is used three ways: as the optional declaration on
/// [`RegisterTableRequest`], as the canonical server-resolved layout returned by
/// [`BifrostTableDescription`], and as the exact JSON persisted in the control
/// row. In the latter two it is always fully populated: `time_column` and
/// `partition_by` are named, the sort order carries at least one key, and the
/// Bloom list is the complete resolved set, which may be empty.
///
/// `partition_by` holds at most three fields: exactly one `hour` or `day`
/// transform of `time_column`, a required UTC microsecond or nanosecond
/// timestamp, plus at most two `identity`, `bucket`, or `truncate` dimensions.
/// An omitted or empty list resolves to `hour(time_column)`, and an omitted
/// `time_column` to `wyrd_ingested_at`.
///
/// On the request path an omitted `sort_keys` and an explicit empty `sort_keys`
/// resolve identically, because the server injects nothing ahead of a declared
/// key and defaults an empty order to the time column descending.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct PhysicalLayoutWire {
    /// The table's time column: a required `TIMESTAMP_LTZ` column at
    /// microsecond or nanosecond precision.
    ///
    /// Omitted on a request, the time column is `wyrd_ingested_at`. The
    /// canonical form in a description and the control row always names it.
    /// An explicit `null` is refused.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "String")]
    pub time_column: Option<String>,
    /// Ordered partition fields; at most three may be declared.
    #[serde(default)]
    pub partition_by: Vec<PartitionFieldWire>,
    /// Ordered sort keys; at most four may be declared.
    #[serde(default)]
    pub sort_keys: Vec<SortKeyWire>,
    /// Bloom-filtered columns; at most sixteen may be declared.
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
    /// User fields only; `wyrd_*`, `workload_card_uid`, `subject_card_uid`, `run_id`, `sub_principal_id`, and
    /// `act_principal_id` reserved names are rejected.
    pub fields: Vec<FieldSpec>,
    /// Optional physical layout declaration.
    ///
    /// Omitting the field entirely resolves to `hour(wyrd_ingested_at)`,
    /// `wyrd_ingested_at` descending nulls-last, and no Bloom columns. Within
    /// a supplied object an empty `partition_by` resolves to
    /// `hour(time_column)`, and an explicit empty `sort_keys` resolves exactly
    /// as an omitted one does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physical_layout: Option<PhysicalLayoutWire>,
    /// Optional soft file target Forge compacts this table toward, in bytes.
    ///
    /// Omitted, the new table stores the Forge deployment default resolved at
    /// creation (1 GiB unless the operator moved it). Supplied, it
    /// must be at least 134217728 (the 128 MiB row-group target) and is stored
    /// as the table's `write.target-file-size-bytes` Iceberg property. A
    /// re-register may omit it or repeat the stored value; a different value
    /// is `WYRD_VALA_409_BIFROST_COMPACTION_TARGET_MISMATCH`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compaction_target_file_size_bytes: Option<u64>,
    /// Optional physical compaction strategy Forge applies to this table.
    ///
    /// Omitted, the table stores no explicit type and Forge compacts it
    /// `small-files`. Supplied, it is stored as the table's
    /// `wyrd.forge.compaction.type` Iceberg property. A re-register may omit
    /// it or repeat the stored value; a different value is
    /// `WYRD_VALA_409_BIFROST_COMPACTION_TYPE_MISMATCH`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compaction_type: Option<CompactionTypeWire>,
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
    /// The table's current version, `1` for a newly created table.
    pub table_version: TableVersion,
}

/// Deserializes a field that may be omitted but, when present, is never
/// `null`: an explicit `null` fails as a type error instead of reading as
/// omitted.
///
/// Pair it with `#[serde(default, skip_serializing_if = "Option::is_none")]`
/// so `None` serializes as an omission this function accepts.
///
/// # Errors
///
/// Returns the deserializer's error when the value is `null` or is not a `T`.
fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// Explicitly alter one table's definition (`PATCH /v1/bifrost/tables/{namespace}/{name}`).
///
/// The whole request applies atomically against `expected_table_version` and
/// publishes exactly one new table version; a stale version changes nothing.
/// At least one field besides the version must make an effective change.
/// Unknown fields and explicit `null`s are refused, so a field is either
/// omitted or carries a value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct AlterTableRequest {
    /// The table version this edit was built from, as last described.
    pub expected_table_version: TableVersion,
    /// Layout lists to replace; an omitted list keeps the current one.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "PhysicalLayoutEdit")]
    pub physical_layout: Option<PhysicalLayoutEdit>,
    /// Nullable top-level custom columns to append, in the field-declaration
    /// shape registration uses.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "Vec<FieldSpec>")]
    pub add_columns: Option<Vec<FieldSpec>>,
    /// Top-level custom columns to rename; each keeps its field id and values.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "Vec<ColumnRename>")]
    pub rename_columns: Option<Vec<ColumnRename>>,
}

/// The layout lists one [`AlterTableRequest`] replaces.
///
/// Each supplied list replaces the table's complete current list and an
/// omitted one is kept; `bloom_columns: []` removes every Bloom column. The
/// time column is fixed at creation, so `time_column` is an unknown field
/// here.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct PhysicalLayoutEdit {
    /// The complete new partition list, holding exactly one `hour` or `day`
    /// field over the time column.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "Vec<PartitionFieldWire>")]
    pub partition_by: Option<Vec<PartitionFieldWire>>,
    /// The complete new sort order; an empty list sorts by the time column
    /// descending, nulls last.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "Vec<SortKeyWire>")]
    pub sort_keys: Option<Vec<SortKeyWire>>,
    /// The complete new Bloom column set, which may be empty.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "Vec<String>")]
    pub bloom_columns: Option<Vec<String>>,
}

/// One top-level custom column rename in an [`AlterTableRequest`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct ColumnRename {
    /// The column's current name.
    pub from: String,
    /// The column's new name, unused by any other column.
    pub to: String,
}

/// Maximum number of warnings carried by a terminal frame.
pub const MAX_QUERY_TERMINAL_WARNINGS: usize = 16;
/// Maximum number of closed source-completion entries.
pub const MAX_QUERY_SOURCE_COMPLETIONS: usize = 3;

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
    /// Terminal fields form an invalid state.
    #[error("invalid query terminal: {reason}")]
    InvalidTerminal {
        /// Closed validation reason.
        reason: &'static str,
    },
    /// One bind value is outside the supported scalar domain.
    #[error("params[{index}] must be a finite number")]
    InvalidParam {
        /// Zero-based position of the rejected bind value.
        index: usize,
    },
}

/// Projects a query request refusal onto the Bifrost error catalog.
///
/// Every contract refusal, the deadline range included, is
/// `WYRD_VALA_400_QUERY_INVALID_SQL`. Client pre-IO validation and Oracle's
/// server-side validation both route through this one projection.
impl From<QueryContractError> for crate::vala::error::BifrostError {
    fn from(error: QueryContractError) -> Self {
        Self::QueryInvalidSql {
            detail: error.to_string(),
        }
    }
}

/// Lifts a query request refusal into the public error through its Bifrost
/// projection, so client surfaces can `?` a validation result directly.
impl From<QueryContractError> for crate::error::WyrdError {
    fn from(error: QueryContractError) -> Self {
        Self::Vala {
            error: error.into(),
        }
    }
}

/// One typed scalar bind value for a positional SQL placeholder.
///
/// `params[i]` binds placeholder `$(i + 1)` in [`BifrostQueryRequest::sql`].
/// Values travel as data next to the SQL text and are never spliced into it,
/// so a string containing SQL stays a string. On the JSON wire each value is
/// the bare JSON scalar (`null`, `true`, `42`, `1.5`, `"text"`); an integral
/// JSON number is an [`QueryParam::Int`] and any other number a
/// [`QueryParam::Float`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(untagged)]
pub enum QueryParam {
    /// SQL `NULL`.
    Null,
    /// SQL `BOOLEAN`.
    Bool(bool),
    /// SQL `BIGINT`.
    Int(i64),
    /// SQL `DOUBLE`; must be finite.
    Float(f64),
    /// SQL `VARCHAR`.
    String(String),
}

/// Public synchronous Oracle query request.
///
/// Every query reads the same sources: its pinned published cut plus
/// best-effort live rows from selected online Scribes. There is no caller
/// source, freshness, or class selector, and unknown fields are rejected so a
/// client that still sends one learns the contract changed instead of silently
/// receiving different semantics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct BifrostQueryRequest {
    /// SELECT-only SQL text.
    pub sql: String,
    /// Ordered bind values for the `$1..$n` placeholders in `sql`.
    ///
    /// Omitted on the wire when empty, so a request without placeholders keeps
    /// its original shape.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<QueryParam>,
    /// Optional caller deadline in milliseconds, valid in `1..=u32::MAX`.
    ///
    /// The field is a wide signed integer so ordinary below- and above-range
    /// values deserialize and reach request validation, which owns the one
    /// closed range every client surface shares.
    #[schemars(range(min = 1, max = 4_294_967_295_u32))]
    #[cfg_attr(feature = "server", schema(minimum = 1, maximum = 4_294_967_295_u32))]
    pub deadline_ms: Option<i64>,
}

impl BifrostQueryRequest {
    /// Validates request fields whose limits are part of the pure protocol.
    ///
    /// # Errors
    /// Returns [`QueryContractError`] when SQL is empty, a bind value is a
    /// non-finite float, or the deadline is outside `1..=u32::MAX`
    /// milliseconds.
    pub fn validate(&self) -> Result<(), QueryContractError> {
        if self.sql.trim().is_empty() {
            return Err(QueryContractError::Empty { field: "sql" });
        }
        if let Some(index) = self
            .params
            .iter()
            .position(|param| matches!(param, QueryParam::Float(value) if !value.is_finite()))
        {
            return Err(QueryContractError::InvalidParam { index });
        }
        if self
            .deadline_ms
            .is_some_and(|deadline| !(1..=i64::from(u32::MAX)).contains(&deadline))
        {
            return Err(QueryContractError::InvalidTerminal {
                reason: "deadline_ms must be between 1 and 4294967295",
            });
        }
        Ok(())
    }
}

/// Server-derived class of one query: how Oracle admits and executes it.
///
/// Never a request field. Oracle reads it from the one physical root it built:
/// a distributed root is Analytical, anything else Interactive. The class
/// selects the admission rules and the execution path together, so the
/// terminal's class is also the path that ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum QueryClass {
    /// Executed locally on the leader.
    Interactive,
    /// Executed as a distributed graph across Oracle peers.
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
    /// The published cut completed and every selected live source finished
    /// the work the completed plan required. Live coverage is best effort:
    /// success never proves every acknowledged write was included.
    Success,
    /// A known live source was unavailable before yielding rows; the result
    /// carries [`QueryWarning::LiveTailUnavailable`].
    Degraded,
    /// Query failed after framing began.
    Failed,
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
    /// Live rows streamed from the selected online Scribes.
    LiveTail,
}

/// Closed warnings emitted by the Oracle protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum QueryWarning {
    /// A known live Scribe source was unavailable before yielding rows.
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
    /// A known live source was unavailable before yielding rows.
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

/// Terminal frame retaining immutable cut metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct QueryTerminalFrame {
    /// Stream outcome.
    pub outcome: QueryTerminalOutcome,
    /// Class Oracle derived for this query, which is also the path that ran.
    ///
    /// Present on every terminal, successful or failed, so a caller always
    /// learns which path ran. A failed terminal on [`QueryClass::Analytical`]
    /// is the proof that selection was irreversible: REQ-002 forbids rerunning
    /// that query through Interactive.
    pub query_class: QueryClass,
    /// Rows already emitted in batch frames.
    pub row_count: u64,
    /// Closed bounded warnings.
    pub warnings: Vec<QueryWarning>,
    /// Exactly one entry for each source present in the cut.
    pub source_completion: Vec<SourceCompletion>,
    /// Catalog error that failed the query, as its RFC 9457 problem.
    ///
    /// Required for [`QueryTerminalOutcome::Failed`] and absent otherwise. It
    /// is the same problem, code and details included, the query would have
    /// returned had it failed before the first frame, so a caller reads one
    /// error whatever the row on which the failure happened. A failure with
    /// no catalog identity is `WYRD_VALA_500_QUERY_EXECUTION_FAILED`.
    #[serde(default)]
    pub error: Option<Box<WyrdProblem>>,
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
    /// Validates closed terminal combinations.
    ///
    /// Every terminal names all three source tiers exactly once. Published
    /// tiers are always complete on a non-failed terminal path because a
    /// published-source failure fails the query; only the live tier may be
    /// unavailable, and that fact, the degraded outcome of a non-failed
    /// terminal, and the live-tail warning must agree.
    ///
    /// The end-of-stream rule is part of this matrix rather than a separate
    /// check: a terminal that claims success without closing its Arrow IPC
    /// stream, or a failure that claims to have closed one it never finished,
    /// is as invalid as a mismatched source set.
    ///
    /// # Errors
    /// Returns [`QueryContractError`] for invalid cardinality, duplicate or
    /// missing sources, inconsistent outcome/warning/error fields, or an
    /// Arrow IPC end-of-stream whose presence contradicts the outcome.
    pub fn validate(&self) -> Result<(), QueryContractError> {
        if self.warnings.len() > MAX_QUERY_TERMINAL_WARNINGS {
            return Err(QueryContractError::TooMany {
                field: "warnings",
                maximum: MAX_QUERY_TERMINAL_WARNINGS,
            });
        }
        if self.source_completion.len() != MAX_QUERY_SOURCE_COMPLETIONS {
            return Err(QueryContractError::InvalidTerminal {
                reason: "source completion must name every source tier",
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
        if !failed && degraded_live != (self.outcome == QueryTerminalOutcome::Degraded) {
            return Err(QueryContractError::InvalidTerminal {
                reason: "degraded outcome must match unavailable live source",
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
    use QueryTerminalOutcome::{Degraded, Failed, Success};
    use SourceCompletionOutcome::{Complete, Unavailable};

    /// Exact Arrow IPC end-of-stream marker: one continuation token followed by
    /// a zero-length message, which is what `StreamWriter::finish` appends.
    const EOS: [u8; 8] = [0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0];

    /// Builds the closed three-tier source set with the given live outcome.
    fn sources(live: SourceCompletionOutcome) -> Vec<SourceCompletion> {
        vec![
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
                outcome: live,
            },
        ]
    }

    /// The generic execution failure a failed fixture terminal carries.
    fn execution_failed() -> Box<WyrdProblem> {
        Box::new(
            crate::error::WyrdError::from(crate::vala::error::BifrostError::QueryExecutionFailed)
                .problem(),
        )
    }

    /// Builds one terminal from its independently varied fields.
    fn terminal(
        outcome: QueryTerminalOutcome,
        live: SourceCompletionOutcome,
        warnings: Vec<QueryWarning>,
    ) -> QueryTerminalFrame {
        let failed = outcome == QueryTerminalOutcome::Failed;
        QueryTerminalFrame {
            outcome,
            query_class: QueryClass::Interactive,
            row_count: 1,
            warnings,
            source_completion: sources(live),
            error: failed.then(execution_failed),
            arrow_ipc_eos: if failed { Vec::new() } else { EOS.to_vec() },
        }
    }

    /// Success, degraded, and failed terminals each carry every source tier
    /// and no freshness field; only consistent combinations validate.
    #[test]
    fn closed_terminal_matrix_validates() {
        let warn = || vec![QueryWarning::LiveTailUnavailable];

        let success = terminal(Success, Complete, vec![]);
        success.validate().expect("success terminal validates");
        success
            .validate_emitted_rows(1)
            .expect("success row count validates");
        let encoded = serde_json::to_value(&success).expect("terminal serializes");
        assert!(encoded.get("freshness").is_none(), "no freshness field");
        assert_eq!(
            encoded["source_completion"].as_array().map(Vec::len),
            Some(3)
        );

        terminal(Degraded, Unavailable, warn())
            .validate()
            .expect("degraded terminal validates");
        terminal(Failed, Unavailable, warn())
            .validate()
            .expect("failure after known live loss validates");
        terminal(Failed, Complete, vec![])
            .validate()
            .expect("failure with complete live validates");

        for invalid in [
            terminal(Success, Unavailable, warn()),
            terminal(Success, Complete, warn()),
            terminal(Degraded, Complete, vec![]),
            terminal(Degraded, Unavailable, vec![]),
            terminal(Failed, Unavailable, vec![]),
        ] {
            assert!(invalid.validate().is_err(), "{invalid:?} must be rejected");
        }

        let mut two_tiers = terminal(Success, Complete, vec![]);
        two_tiers.source_completion.pop();
        assert!(two_tiers.validate().is_err(), "every tier is required");

        let mut unavailable_published = terminal(Failed, Complete, vec![]);
        unavailable_published.source_completion[0].outcome = Unavailable;
        assert!(
            unavailable_published.validate().is_err(),
            "published tiers are never degraded"
        );
    }

    /// Failed terminals require an error and exact emitted-row count.
    #[test]
    fn invalid_failed_terminal_is_rejected() {
        let mut failed_without_error = terminal(
            QueryTerminalOutcome::Failed,
            SourceCompletionOutcome::Complete,
            vec![],
        );
        failed_without_error.error = None;
        assert!(failed_without_error.validate().is_err());

        let failed = terminal(
            QueryTerminalOutcome::Failed,
            SourceCompletionOutcome::Complete,
            vec![],
        );
        assert!(failed.validate_emitted_rows(2).is_err());
    }

    /// The request carries only SQL and an optional deadline: a source or
    /// freshness selector is rejected rather than silently ignored, and empty
    /// SQL or a zero deadline fails validation.
    #[test]
    fn query_request_has_no_source_selectors_and_validation_is_closed() {
        let request: BifrostQueryRequest = serde_json::from_value(serde_json::json!({
            "sql": "SELECT 1",
            "deadline_ms": null
        }))
        .expect("request deserializes");
        request.validate().expect("minimal request validates");
        assert_eq!(
            serde_json::to_value(&request).expect("request serializes"),
            serde_json::json!({"sql": "SELECT 1", "deadline_ms": null})
        );

        for (field, value) in [
            ("visibility", "published_only"),
            ("freshness", "allow_degraded"),
            ("query_class", "analytical"),
        ] {
            let mut body = serde_json::json!({"sql": "SELECT 1", "deadline_ms": null});
            body[field] = serde_json::Value::from(value);
            assert!(
                serde_json::from_value::<BifrostQueryRequest>(body).is_err(),
                "{field} must be rejected"
            );
        }

        let invalid = BifrostQueryRequest {
            params: Vec::new(),
            sql: " ".into(),
            deadline_ms: Some(0),
        };
        assert!(invalid.validate().is_err());
    }

    /// The deadline accepts exactly `1..=u32::MAX`, and the generated schema
    /// publishes the same bounds.
    ///
    /// # Panics
    ///
    /// Panics when a boundary value is misclassified or the schema bounds drift.
    #[test]
    fn query_request_deadline_range_is_closed_and_published() {
        let request = |deadline_ms| BifrostQueryRequest {
            params: Vec::new(),
            sql: "SELECT 1".into(),
            deadline_ms: Some(deadline_ms),
        };
        let max = i64::from(u32::MAX);
        for valid in [1, max] {
            request(valid)
                .validate()
                .expect("in-range deadline validates");
        }
        for invalid in [-1, 0, max + 1] {
            assert!(
                request(invalid).validate().is_err(),
                "{invalid} must be rejected"
            );
        }
        let decoded: BifrostQueryRequest = serde_json::from_value(serde_json::json!({
            "sql": "SELECT 1",
            "deadline_ms": max + 1
        }))
        .expect("an above-range deadline deserializes for validation");
        assert!(decoded.validate().is_err());

        let schema = serde_json::to_value(schemars::schema_for!(BifrostQueryRequest))
            .expect("request schema serializes");
        let deadline = &schema["properties"]["deadline_ms"];
        assert_eq!(deadline["minimum"].as_f64(), Some(1.0));
        assert_eq!(deadline["maximum"].as_f64(), Some(f64::from(u32::MAX)));
        assert_eq!(
            schema["additionalProperties"],
            serde_json::Value::Bool(false)
        );
    }

    /// An out-of-range deadline and an empty statement both project onto the
    /// SQL refusal.
    ///
    /// # Panics
    ///
    /// Panics when either request validates or projects onto another code.
    #[test]
    fn query_request_refusals_project_onto_their_catalog_codes() {
        let request = |sql: &str, deadline_ms| BifrostQueryRequest {
            params: Vec::new(),
            sql: sql.into(),
            deadline_ms,
        };
        let refusal = |request: BifrostQueryRequest| {
            crate::error::WyrdError::from(request.validate().expect_err("request is refused"))
                .code()
        };

        assert_eq!(
            refusal(request("SELECT 1", Some(0))),
            "WYRD_VALA_400_QUERY_INVALID_SQL"
        );
        assert_eq!(
            refusal(request(" ", None)),
            "WYRD_VALA_400_QUERY_INVALID_SQL"
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

/// One transformed value of an additional partition dimension.
///
/// The variant is the value's stored type, so equal literals of different
/// types never name the same partition.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum PartitionValueWire {
    /// A string value.
    Utf8(String),
    /// A 32-bit integer value, including every bucket ordinal.
    Int32(i32),
    /// A 64-bit integer value.
    Int64(i64),
}

/// Complete partition identity of one physical slice, file, or claim, carried
/// by private durable records.
///
/// `time` is the required time partition and `values` the additional
/// dimensions in partition-spec order, `None` for a null source value;
/// `time_index` is the time field's position among all spec fields.
/// Identities are equal only when their spec, time, and every typed value
/// agree.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct PhysicalPartitionWire {
    /// Iceberg partition-spec id the identity was computed under.
    pub spec_id: i32,
    /// Required time partition.
    pub time: TimePartitionWire,
    /// Position of the time field among the spec's fields.
    pub time_index: u8,
    /// Additional dimension values in spec order; `None` is a null value.
    pub values: Vec<Option<PartitionValueWire>>,
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

/// Identity of one Scribe writer stream serving a live partition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct TailStreamIdentity {
    /// Scribe node identity.
    pub node_id: NodeId,
    /// Writer boot epoch.
    pub writer_epoch: WriterEpoch,
}

/// Fenced request opening one participant's held grant for a distributed graph.
///
/// Every grant charges exactly one query envelope, holding one slot unit, on
/// the receiving node, so the request carries no demand of its own. The grant
/// is held only while the leader keeps the request's stream open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ReserveNodeSlotsRequest {
    /// Query identity.
    pub query_id: QueryId,
    /// Leader node identity.
    pub leader_node_id: NodeId,
    /// Leader Oracle-role fence.
    pub leader_fencing_token: FencingToken,
    /// Absolute query deadline; the follower ends the held grant stream here.
    pub expires_at: DateTime<Utc>,
    /// Distributed Analytical graph this reservation is taken for.
    ///
    /// A follower binds the reservation, and later the graph lease, to this
    /// exact graph.
    pub graph: AnalyticalGraphRef,
}

/// The two-identity name of one distributed Analytical graph.
///
/// Carried on the private peer wire so a follower can bind a reservation, and
/// later a graph lease, to the exact graph the leader named rather than to
/// whichever graph happens to arrive first under that reservation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct AnalyticalGraphRef {
    /// Public query identity the graph answers.
    pub public_query_id: uuid::Uuid,
    /// `DataFusion` query identity the graph executes under.
    pub datafusion_query_id: uuid::Uuid,
}

/// Accepted grant, held by the leader's open stream rather than by an expiry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PendingNodeReservation {
    /// Reservation identity.
    pub reservation_id: ReservationId,
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
    /// The grant is held for as long as the leader keeps the stream open.
    Pending(PendingNodeReservation),
    /// Node lacked capacity.
    Rejected(ReservationRejected),
}

/// Unsigned typed operation context carried on the private peer plane.
///
/// The mTLS cluster identity establishes that a trusted Wyrd process sent it;
/// the receiver validates every claim against its own trusted state (local
/// node and role fence, reservations, deadlines, and bounds) before it decodes
/// a plan or touches tenant storage. The bytes are bounded before decoding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PeerContext {
    /// Encoded typed claims, at most 16 KiB for a fragment.
    pub claims_bytes: Vec<u8>,
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
    /// Retained Scribe live capture the follower reads, minted by that
    /// Scribe's listing for this query.
    ///
    /// The follower opens only this capture and never takes a fresh one, so
    /// the rows it reads are the rows Oracle validated against the cut.
    pub capture_id: Uuid,
}

impl ScribeProviderCut {
    /// Validates the canonical Scribe memory-provider cut.
    ///
    /// The cut bounds which partitions a follower may read from memory and on
    /// which writer incarnation. It carries no size limit: the read references
    /// rows the Scribe already holds, and the query's own memory pool governs
    /// what execution retains. It carries no statement
    /// about which rows are already published: Scribe decides that from the
    /// generation authority it owns, and a WAL interval on the wire would be a
    /// second, weaker answer that a reader could mistake for ownership.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.writer_epoch > 0
            && self.start_partition.granularity() == self.end_partition.granularity()
            && self.start_partition <= self.end_partition
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
    /// Scribe memory-provider cut, present only for a Scribe target.
    pub scribe_provider_cut: Option<ScribeProviderCut>,
    /// Schema fingerprint bound to the encoded placeholder.
    pub schema_fingerprint: String,
    /// Required output/predicate/hidden-tenant projection closure, in the
    /// stable order the leaf union and remote placeholder must expose.
    pub required_columns: Vec<String>,
}

/// Context-bound worker request carrying one serialized physical subtree.
///
/// This is the sole domain projection of the private
/// `wyrd.v1.ExecuteFragmentRequest` peer message. The leader mints it per
/// follower after splitting the admitted plan; the follower checks the
/// context, both fences, and the assignment-authority digest before it
/// deserializes `physical_plan_bytes` and substitutes each remote placeholder
/// with its role-local source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ExecuteFragmentRequest {
    /// Typed fragment context validated before plan decode.
    pub context: PeerContext,
    /// Runtime-bounded physical-plan bytes.
    pub physical_plan_bytes: Vec<u8>,
    /// Pending reservation identity.
    pub reservation_id: ReservationId,
    /// Request-local leader incarnation.
    pub leader_fence: OracleRoleFence,
    /// Target follower incarnation.
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
    /// how much a pushed-down predicate actually saved. Counts every excluded
    /// row group, whether statistics or a Bloom filter excluded it.
    pub row_groups_pruned: u64,
    /// Subset of `row_groups_pruned` that a Bloom filter excluded after
    /// min/max statistics retained the row group.
    pub row_groups_pruned_bloom: u64,
    /// Rows inside retained row groups that page-index row selection skipped.
    pub rows_pruned_page_index: u64,
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

    use super::*;

    /// Bind values keep their order and scalar types across the JSON wire.
    ///
    /// A string holding SQL text stays one string value rather than becoming
    /// part of the statement, and a non-finite float fails validation at the
    /// contract edge.
    ///
    /// # Panics
    ///
    /// Panics when a value changes type or position, or validation drifts.
    #[test]
    fn query_bind_values_round_trip_in_order() {
        let body = serde_json::json!({
            "sql": "SELECT * FROM t WHERE a = $1 AND b = $2 AND c = $3 AND d = $4 AND e = $5",
            "params": [null, true, 42, 1.5, "x'; DROP TABLE t; --"],
            "deadline_ms": null
        });
        let request: BifrostQueryRequest =
            serde_json::from_value(body.clone()).expect("request deserializes");
        assert_eq!(
            request.params,
            [
                QueryParam::Null,
                QueryParam::Bool(true),
                QueryParam::Int(42),
                QueryParam::Float(1.5),
                QueryParam::String("x'; DROP TABLE t; --".into()),
            ]
        );
        request.validate().expect("finite scalars validate");
        assert_eq!(
            serde_json::to_value(&request).expect("request serializes"),
            body
        );

        let invalid = BifrostQueryRequest {
            params: vec![QueryParam::Int(1), QueryParam::Float(f64::NAN)],
            ..request
        };
        assert_eq!(
            invalid.validate(),
            Err(QueryContractError::InvalidParam { index: 1 })
        );
    }

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
            SourceCompletion {
                source: QuerySource::LiveTail,
                outcome: SourceCompletionOutcome::Complete,
            },
        ];
        let base = QueryTerminalFrame {
            outcome: QueryTerminalOutcome::Success,
            query_class: QueryClass::Interactive,
            row_count: 0,
            warnings: vec![],
            source_completion: sources,
            error: None,
            arrow_ipc_eos: EOS.to_vec(),
        };

        // Empty success is the case the previous byteless terminal could not
        // express: no batch frame ever carried an EOS, so the terminal must.
        base.validate()
            .expect("empty success carries its end-of-stream");
        base.validate_emitted_rows(0)
            .expect("empty success emitted no rows");
        assert!(
            QueryTerminalFrame {
                arrow_ipc_eos: Vec::new(),
                ..base.clone()
            }
            .validate()
            .is_err(),
            "success without an end-of-stream must be rejected"
        );

        let mut degraded_sources = base.source_completion.clone();
        degraded_sources[2].outcome = SourceCompletionOutcome::Unavailable;
        let degraded = QueryTerminalFrame {
            outcome: QueryTerminalOutcome::Degraded,
            warnings: vec![QueryWarning::LiveTailUnavailable],
            source_completion: degraded_sources,
            ..base.clone()
        };
        degraded
            .validate()
            .expect("degraded still finishes its Arrow stream");
        assert!(
            QueryTerminalFrame {
                arrow_ipc_eos: Vec::new(),
                ..degraded
            }
            .validate()
            .is_err(),
            "degraded without an end-of-stream must be rejected"
        );

        let failed = QueryTerminalFrame {
            outcome: QueryTerminalOutcome::Failed,
            error: Some(Box::new(
                crate::error::WyrdError::from(
                    crate::vala::error::BifrostError::QueryExecutionFailed,
                )
                .problem(),
            )),
            arrow_ipc_eos: Vec::new(),
            ..base.clone()
        };
        failed.validate().expect("a failed terminal closes nothing");
        assert!(
            QueryTerminalFrame {
                arrow_ipc_eos: EOS.to_vec(),
                ..failed
            }
            .validate()
            .is_err(),
            "a failed terminal must not claim an end-of-stream"
        );

        let encoded = serde_json::to_value(&base).expect("terminal serializes");
        assert_eq!(
            serde_json::from_value::<QueryTerminalFrame>(encoded).expect("terminal deserializes"),
            base
        );
    }

    /// The public query contract carries the selected path out and never in.
    ///
    /// REQ-001 keeps the request free of any execution-path, class, worker, or
    /// plan selector, and REQ-008 requires the terminal to name the path Oracle
    /// actually selected. Both halves are pinned from the generated schema so a
    /// later field addition on either side has to break this test first.
    #[test]
    fn query_terminal_projects_path_without_request_selector() {
        let request = serde_json::to_value(schemars::schema_for!(BifrostQueryRequest))
            .expect("request schema serializes");
        let mut request_properties = request["properties"]
            .as_object()
            .expect("request has properties")
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        request_properties.sort_unstable();
        assert_eq!(
            request_properties,
            ["deadline_ms", "params", "sql"],
            "the request must not accept a source, freshness, path, class, worker, or plan selector"
        );

        let path = serde_json::to_value(schemars::schema_for!(QueryClass))
            .expect("path schema serializes");
        let path_variants = path["oneOf"]
            .as_array()
            .expect("the selected path is a closed enum")
            .iter()
            .map(|variant| {
                variant["const"]
                    .as_str()
                    .expect("each variant is one string constant")
                    .to_owned()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            path_variants,
            ["interactive", "analytical"],
            "the selected path is closed over Interactive and Analytical"
        );

        let terminal = QueryTerminalFrame {
            outcome: QueryTerminalOutcome::Success,
            query_class: QueryClass::Analytical,
            row_count: 0,
            warnings: vec![],
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
                    outcome: SourceCompletionOutcome::Complete,
                },
            ],
            error: None,
            arrow_ipc_eos: vec![0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0],
        };
        terminal
            .validate()
            .expect("an Analytical terminal is valid");
        let encoded = serde_json::to_value(&terminal).expect("terminal serializes");
        assert_eq!(
            encoded["query_class"],
            serde_json::Value::from("analytical"),
            "the terminal names the server-selected path on the wire"
        );
        assert_eq!(
            serde_json::from_value::<QueryTerminalFrame>(encoded).expect("terminal deserializes"),
            terminal
        );
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

// ── Audit event (staged through the process audit outbox) ───────────────────

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

/// The authorization outcome recorded on an audit row.
///
/// An audit row states what the authorization boundary decided about a
/// principal's permission, never whether the operation it admitted later
/// succeeded. Engine outcomes are lineage in their own operational tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AuditOutcome {
    /// The evaluated permission authorized the operation.
    Allowed,
    /// The evaluated permission refused the operation.
    Denied,
}

/// One audited authorization decision.
///
/// Every boundary that evaluates a principal's permission stages exactly one
/// `AuditEvent` on the process audit outbox once the decision is known,
/// outside the operation's transaction; the operation proceeds or is refused
/// without waiting for the event to commit. The outbox later commits it as one
/// hash-chained staging row. The hash-chain canonical
/// encoding and per-tenant `seq` are owned by `vala-sql`; this type is the
/// Arrow-free, PyO3-free wire/codegen shape.
///
/// `card_ref` is the **writer-identity card** (who performed the op), derived
/// from the resolved `Principal`; it is `None` only for a `User` principal.
/// This is decoupled from the per-row `subject_card_uid` data column.
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
    /// The verified principal the decision was for: the token's top-level
    /// principal, or [`PLATFORM_AUDIT_PRINCIPAL`] when no identity was
    /// verified.
    ///
    /// [`PLATFORM_AUDIT_PRINCIPAL`]: crate::auth::PLATFORM_AUDIT_PRINCIPAL
    pub sub_principal_id: PrincipalId,
    /// Kind of the subject principal (tag encoding; card payload is not the
    /// audit subject — `card_ref` is its own field).
    pub sub_principal_kind: PrincipalKindTag,
    /// The verified principal that acted: the outermost delegated actor, the
    /// subject itself for direct work, or the same unavailable-identity
    /// sentinel as the subject. Older actors stay in the detail's delegation
    /// chain.
    pub act_principal_id: PrincipalId,
    /// Non-secret id of the credential that authenticated the request, when one
    /// did.
    ///
    /// A principal may hold several credentials at once so rotation can
    /// overlap, which makes the principal alone too coarse to answer "which key
    /// did this": revoking the compromised one requires knowing which one was
    /// used. `None` when no credential was presented — a federated human
    /// session, or an internal decision made by the server itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_id: Option<Uuid>,
    /// Effective dynamic permission the boundary evaluated.
    pub permission: String,
    /// The authorization outcome this boundary decided.
    pub outcome: AuditOutcome,
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
        attribution: Attribution,
        sub_principal_kind: PrincipalKindTag,
        permission: String,
        outcome: AuditOutcome,
    ) -> Self {
        Self {
            request_id,
            trace_id,
            operation,
            resource,
            card_ref,
            sub_principal_id: attribution.sub_principal_id,
            sub_principal_kind,
            act_principal_id: attribution.act_principal_id,
            credential_id: None,
            permission,
            outcome,
            detail: None,
        }
    }

    /// Name the credential that authenticated the audited request.
    ///
    /// Takes an `Option` because most callers are simply forwarding whatever
    /// the authenticated context holds, and a federated session holds nothing.
    #[must_use]
    pub const fn with_credential_id(mut self, credential_id: Option<Uuid>) -> Self {
        self.credential_id = credential_id;
        self
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

    use std::collections::BTreeMap;

    use crate::vala::api::{
        AlterTableRequest, BifrostTableDescription, BifrostTableEntry, CompactionTypeWire,
        DataTypeSpec, FieldSpec, NullOrderWire, PARQUET_FIELD_ID_KEY, PartitionFieldWire,
        PartitionTransformWire, PhysicalLayoutEdit, PhysicalLayoutWire, RegisterOutcome,
        RegisterTableRequest, RegisterTableResponse, SortDirectionWire, SortKeyWire, TableStatus,
        TimeUnit,
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
            metadata: BTreeMap::default(),
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

    /// A field's `wyrd:column_class` metadata survives the wire round trip, so a
    /// correlation column keeps its class between SDK and server.
    ///
    /// # Panics
    ///
    /// Panics when the field spec fails to serialize, deserialize, or compare
    /// equal after the round trip, or the decoded metadata no longer names the
    /// column `correlation`.
    #[test]
    fn bifrost_wire_field_spec_carries_correlation_metadata() {
        let mut spec = FieldSpec {
            name: "subject_card_uid".to_string(),
            data_type: DataTypeSpec::Utf8,
            nullable: true,
            metadata: BTreeMap::default(),
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
    ///
    /// # Panics
    ///
    /// Panics when the request fails to deserialize or serialize, decodes a
    /// `physical_layout`, or re-encodes with a `physical_layout` key.
    #[test]
    fn bifrost_wire_register_request_omits_physical_layout() {
        let req: RegisterTableRequest =
            serde_json::from_str(r#"{"namespace":"bifrost.bifrost","name":"events","fields":[]}"#)
                .expect("deserialize");
        assert!(req.physical_layout.is_none());
        let json = serde_json::to_string(&req).expect("serialize");
        assert!(!json.contains("physical_layout"), "{json}");
    }

    /// An explicit object with empty lists decodes and keeps them through a
    /// round trip.
    ///
    /// The wire distinction between `None` and an explicit object survives; what
    /// the server no longer distinguishes is an empty `sort_keys` from an
    /// omitted one, which both resolve to the canonical event-time default.
    ///
    /// # Panics
    ///
    /// Panics when the request fails to deserialize, decodes without a layout,
    /// decodes a non-empty `partition_by`, `sort_keys`, or `bloom_columns`, or
    /// differs from itself after a serialize/deserialize round trip.
    #[test]
    fn bifrost_wire_register_request_keeps_explicit_empty_layout_lists() {
        let req: RegisterTableRequest = serde_json::from_str(
            r#"{"namespace":"bifrost.bifrost","name":"events","fields":[],
                "physical_layout":{"partition_by":[],
                "sort_keys":[],"bloom_columns":[]}}"#,
        )
        .expect("deserialize");
        let layout = req.physical_layout.as_ref().expect("explicit layout");
        assert!(layout.partition_by.is_empty());
        assert!(layout.sort_keys.is_empty());
        assert!(layout.bloom_columns.is_empty());
        bifrost_wire_round_trip(&req);
    }

    /// The register request carries the compaction type in its hyphenated
    /// public spelling and rejects the underscore spelling, so one table has
    /// exactly one way to name its type on the wire.
    ///
    /// # Panics
    ///
    /// Panics when the hyphenated or omitted request fails to deserialize or
    /// round-trip, carries the wrong compaction type, or the underscore
    /// spelling is accepted.
    #[test]
    fn bifrost_wire_register_request_carries_hyphenated_compaction_type() {
        let req: RegisterTableRequest = serde_json::from_str(
            r#"{"namespace":"bifrost.datasets","name":"events","fields":[],
                "compaction_type":"small-files"}"#,
        )
        .expect("deserialize");
        assert_eq!(req.compaction_type, Some(CompactionTypeWire::SmallFiles));
        bifrost_wire_round_trip(&req);
        let omitted: RegisterTableRequest =
            serde_json::from_str(r#"{"namespace":"bifrost.datasets","name":"events","fields":[]}"#)
                .expect("deserialize");
        assert_eq!(omitted.compaction_type, None);
        assert!(
            serde_json::from_str::<RegisterTableRequest>(
                r#"{"namespace":"bifrost.datasets","name":"events","fields":[],
                    "compaction_type":"small_files"}"#,
            )
            .is_err()
        );
    }

    /// Every compaction type serializes to exactly its approved hyphenated
    /// value, the underscore spellings are refused, and the generated request
    /// and description schemas publish the same set and the `small-files`
    /// omitted default.
    ///
    /// # Panics
    ///
    /// Panics when a compaction type serializes or parses to a spelling other
    /// than its hyphenated value, an underscore spelling parses, or a generated
    /// schema omits the approved values or the `small-files` default.
    #[test]
    fn bifrost_wire_compaction_type_values_and_schemas_are_hyphenated() {
        for (kind, spelling) in [
            (CompactionTypeWire::Auto, "auto"),
            (CompactionTypeWire::Full, "full"),
            (CompactionTypeWire::SmallFiles, "small-files"),
            (CompactionTypeWire::FilesWithDelete, "files-with-delete"),
        ] {
            assert_eq!(
                serde_json::to_value(kind).expect("serialize"),
                serde_json::Value::String(spelling.to_owned())
            );
            assert_eq!(
                serde_json::from_value::<CompactionTypeWire>(spelling.into()).expect("parse"),
                kind
            );
        }
        for refused in ["small_files", "files_with_delete"] {
            assert!(serde_json::from_value::<CompactionTypeWire>(refused.into()).is_err());
        }
        for schema in [
            serde_json::to_string(&schema_for!(RegisterTableRequest)).expect("schema"),
            serde_json::to_string(&schema_for!(BifrostTableDescription)).expect("schema"),
        ] {
            assert!(schema.contains(r#""small-files""#), "{schema}");
            assert!(schema.contains(r#""files-with-delete""#), "{schema}");
            assert!(!schema.contains("small_files"), "{schema}");
            assert!(!schema.contains("files_with_delete"), "{schema}");
            assert!(
                schema
                    .replace("\\n", " ")
                    .contains("compacts it `small-files`"),
                "{schema}"
            );
        }
    }

    /// The layout carries exactly its four canonical keys, and the replaced
    /// `partition_granularity` spelling or an unknown partition-field key is
    /// refused rather than silently ignored.
    ///
    /// # Panics
    ///
    /// Panics when the serialized keys drift or a removed spelling decodes.
    #[test]
    fn bifrost_wire_physical_layout_has_one_partition_spelling() {
        let layout = PhysicalLayoutWire {
            time_column: Some("event_time".to_owned()),
            partition_by: vec![
                PartitionFieldWire::new("event_time", PartitionTransformWire::Day),
                PartitionFieldWire {
                    buckets: Some(32),
                    ..PartitionFieldWire::new("customer_id", PartitionTransformWire::Bucket)
                },
            ],
            sort_keys: Vec::new(),
            bloom_columns: Vec::new(),
        };
        let json = serde_json::to_value(&layout).expect("serialize");
        let object = json.as_object().expect("layout is a JSON object");
        let mut keys = object.keys().map(String::as_str).collect::<Vec<_>>();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec!["bloom_columns", "partition_by", "sort_keys", "time_column"]
        );
        assert_eq!(
            json["partition_by"],
            serde_json::json!([
                {"column": "event_time", "transform": "day"},
                {"column": "customer_id", "transform": "bucket", "buckets": 32}
            ])
        );
        bifrost_wire_round_trip(&layout);
        for refused in [
            r#"{"partition_granularity":"hour"}"#,
            r#"{"partition_by":[{"column":"a","transform":"hour","granularity":"hour"}]}"#,
            r#"{"partition_by":[{"column":"a","transform":"month"}]}"#,
        ] {
            assert!(
                serde_json::from_str::<PhysicalLayoutWire>(refused).is_err(),
                "{refused}"
            );
        }
    }

    #[test]
    fn bifrost_wire_nested_and_recursive_data_types_round_trip() {
        let spec = FieldSpec {
            name: "nested".to_string(),
            data_type: DataTypeSpec::Struct(vec![
                FieldSpec {
                    name: "tags".to_string(),
                    data_type: DataTypeSpec::List(Box::new(FieldSpec {
                        name: "item".to_string(),
                        data_type: DataTypeSpec::Utf8,
                        nullable: false,
                        metadata: BTreeMap::from([(
                            PARQUET_FIELD_ID_KEY.to_string(),
                            "42".to_string(),
                        )]),
                    })),
                    nullable: true,
                    metadata: BTreeMap::default(),
                },
                FieldSpec {
                    name: "ts".to_string(),
                    data_type: DataTypeSpec::Timestamp {
                        unit: TimeUnit::Microsecond,
                        tz: Some("UTC".to_string()),
                    },
                    nullable: false,
                    metadata: BTreeMap::default(),
                },
            ]),
            nullable: true,
            metadata: BTreeMap::default(),
        };
        bifrost_wire_round_trip(&spec);
    }

    /// A table entry and its full description survive a JSON round trip,
    /// including correlation fields, field-id metadata, managed candidates,
    /// physical layout, and the compaction target and type.
    ///
    /// # Panics
    ///
    /// Panics when either value does not serialize and deserialize back to an
    /// equal value.
    #[test]
    fn bifrost_wire_table_entry_and_description_round_trip() {
        let entry = BifrostTableEntry {
            namespace: "bifrost.bifrost".to_string(),
            name: "events".to_string(),
            table_uid: "ab".repeat(16),
            status: TableStatus::Active,
            fingerprint: "01".repeat(32),
            registered_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let desc = BifrostTableDescription {
            entry: entry.clone(),
            table_version: crate::vala::ids::TableVersion::new((1 << 53) + 1)
                .expect("valid version"),
            user_fields: vec![FieldSpec {
                name: "value".to_string(),
                data_type: DataTypeSpec::Int64,
                nullable: false,
                metadata: BTreeMap::default(),
            }],
            correlation_fields: vec![
                FieldSpec {
                    name: "subject_card_uid".to_string(),
                    data_type: DataTypeSpec::Utf8,
                    nullable: true,
                    metadata: BTreeMap::from([(
                        PARQUET_FIELD_ID_KEY.to_string(),
                        "1001".to_string(),
                    )]),
                },
                FieldSpec {
                    name: "run_id".to_string(),
                    data_type: DataTypeSpec::Utf8,
                    nullable: true,
                    metadata: BTreeMap::from([(
                        PARQUET_FIELD_ID_KEY.to_string(),
                        "1000".to_string(),
                    )]),
                },
            ],
            canonical_physical_fingerprint: None,
            physical_layout: PhysicalLayoutWire {
                time_column: Some("wyrd_ingested_at".to_string()),
                partition_by: vec![PartitionFieldWire::new(
                    "wyrd_ingested_at",
                    PartitionTransformWire::Hour,
                )],
                sort_keys: vec![SortKeyWire {
                    column: "wyrd_ingested_at".to_string(),
                    direction: SortDirectionWire::Desc,
                    null_order: NullOrderWire::Last,
                }],
                bloom_columns: vec!["run_id".to_string()],
            },
            compaction_target_file_size_bytes: Some(1_073_741_824),
            compaction_type: Some(CompactionTypeWire::SmallFiles),
        };
        bifrost_wire_round_trip(&entry);
        bifrost_wire_round_trip(&desc);
    }

    #[test]
    fn bifrost_wire_register_response_and_layout_round_trip() {
        bifrost_wire_round_trip(&RegisterTableResponse {
            outcome: RegisterOutcome::Created,
            table_uid: "ab".repeat(16),
            fingerprint: "01".repeat(32),
            table_version: crate::vala::ids::TableVersion::INITIAL,
        });
        bifrost_wire_round_trip(&PhysicalLayoutWire {
            time_column: Some("created_at".to_string()),
            partition_by: vec![
                PartitionFieldWire::new("created_at", PartitionTransformWire::Day),
                PartitionFieldWire {
                    width: Some(4),
                    ..PartitionFieldWire::new("region", PartitionTransformWire::Truncate)
                },
            ],
            sort_keys: vec![SortKeyWire {
                column: "created_at".to_string(),
                direction: SortDirectionWire::Desc,
                null_order: NullOrderWire::Last,
            }],
            bloom_columns: vec!["run_id".to_string()],
        });
    }

    /// A PATCH body decodes only with a canonical string version, no unknown
    /// field (the time column included), and no explicit `null` at any depth;
    /// omitted and empty lists stay distinct.
    ///
    /// # Panics
    ///
    /// Panics when a refused body decodes or an accepted one decodes
    /// differently.
    #[test]
    fn alter_table_request_decodes_strictly() {
        let decode = |body: serde_json::Value| serde_json::from_value::<AlterTableRequest>(body);
        let cleared = decode(serde_json::json!({
            "expected_table_version": "12",
            "physical_layout": {"bloom_columns": []}
        }))
        .expect("the supplement's Bloom-clearing body decodes");
        assert_eq!(cleared.expected_table_version.get(), 12);
        assert_eq!(
            cleared.physical_layout,
            Some(PhysicalLayoutEdit {
                bloom_columns: Some(Vec::new()),
                ..PhysicalLayoutEdit::default()
            })
        );
        assert_eq!(cleared.add_columns, None);
        bifrost_wire_round_trip(&cleared);
        let added = decode(serde_json::json!({
            "expected_table_version": "3",
            "add_columns": [
                {"name": "note", "data_type": "Utf8"},
                {"name": "seen", "data_type": {"Timestamp": {"unit": "Microsecond"}},
                 "metadata": {"PARQUET:field_id": "9", "origin": "caller"}}
            ],
            "physical_layout": {"partition_by": [{"column": "t", "transform": "day"}]}
        }))
        .expect("omitted nested members take their defaults");
        let columns = added.add_columns.as_deref().expect("columns decode");
        assert!(columns.iter().all(|column| column.nullable));
        assert_eq!(
            columns[1].data_type,
            DataTypeSpec::Timestamp {
                unit: TimeUnit::Microsecond,
                tz: None
            }
        );
        assert_eq!(columns[1].metadata.len(), 2);
        let encoded = serde_json::to_value(&added).expect("the request encodes");
        assert_eq!(
            encoded["add_columns"][1]["data_type"],
            serde_json::json!({"Timestamp": {"unit": "Microsecond"}}),
            "an unset zone is omitted, never sent as null"
        );
        assert_eq!(
            encoded["physical_layout"]["partition_by"][0],
            serde_json::json!({"column": "t", "transform": "day"})
        );
        assert_eq!(decode(encoded).expect("the encoding decodes"), added);
        let max = decode(serde_json::json!({"expected_table_version": "9223372036854775807"}))
            .expect("the maximum version decodes");
        assert_eq!(max.expected_table_version.get(), i64::MAX);

        for refused in [
            serde_json::json!({"expected_table_version": 12}),
            serde_json::json!({"expected_table_version": "0"}),
            serde_json::json!({"expected_table_version": "012"}),
            serde_json::json!({"expected_table_version": "9223372036854775808"}),
            serde_json::json!({"physical_layout": {"bloom_columns": []}}),
            serde_json::json!({"expected_table_version": "1", "table_version": "1"}),
            serde_json::json!({"expected_table_version": "1", "physical_layout": null}),
            serde_json::json!({"expected_table_version": "1", "add_columns": null}),
            serde_json::json!({"expected_table_version": "1", "rename_columns": null}),
            serde_json::json!({"expected_table_version": "1", "physical_layout": {"bloom_columns": null}}),
            serde_json::json!({"expected_table_version": "1", "physical_layout": {"time_column": "t"}}),
            serde_json::json!({"expected_table_version": "1", "rename_columns": [{"from": "a", "to": "b", "x": 1}]}),
            serde_json::json!({"expected_table_version": "1", "add_columns": [{"name": "n", "data_type": "Utf8", "x": 1}]}),
            serde_json::json!({"expected_table_version": "1", "add_columns": [{"name": "n", "data_type": "Utf8", "nullable": null}]}),
            serde_json::json!({"expected_table_version": "1", "add_columns": [{"name": "n", "data_type": "Utf8", "metadata": null}]}),
            serde_json::json!({"expected_table_version": "1", "add_columns": [{"name": "n", "data_type": {"Timestamp": {"unit": "Microsecond", "tz": null}}}]}),
            serde_json::json!({"expected_table_version": "1", "add_columns": [{"name": "n", "data_type": {"Timestamp": {"unit": "Microsecond", "zone": "UTC"}}}]}),
            serde_json::json!({"expected_table_version": "1", "add_columns": [{"name": "n", "data_type": {"Decimal128": {"precision": 9, "scale": 2, "x": 1}}}]}),
            serde_json::json!({"expected_table_version": "1", "add_columns": [{"name": "n", "data_type": {"List": {"name": "item", "data_type": "Utf8", "x": 1}}}]}),
            serde_json::json!({"expected_table_version": "1", "physical_layout": {"sort_keys": [{"column": "a", "direction": "asc", "null_order": "last", "x": 1}]}}),
            serde_json::json!({"expected_table_version": "1", "physical_layout": {"partition_by": [{"column": "t", "transform": "day", "buckets": null}]}}),
            serde_json::json!({"expected_table_version": "1", "physical_layout": {"partition_by": [{"column": "t", "transform": "day", "width": null}]}}),
        ] {
            assert!(
                decode(refused.clone()).is_err(),
                "{refused} must be refused"
            );
        }
    }

    /// Every Bifrost wire type, the strict nested declarations included,
    /// yields a JSON Schema.
    ///
    /// # Panics
    ///
    /// Panics when schema generation panics for any listed type.
    #[test]
    fn bifrost_wire_schema_for_wire_types_does_not_panic() {
        let _ = schema_for!(AlterTableRequest);
        let _ = schema_for!(BifrostTableEntry);
        let _ = schema_for!(BifrostTableDescription);
        let _ = schema_for!(DataTypeSpec);
        let _ = schema_for!(FieldSpec);
        let _ = schema_for!(RegisterTableRequest);
        let _ = schema_for!(RegisterTableResponse);
    }

    /// Private tail and peer DTOs remain schema-generatable pure contracts.
    #[test]
    fn private_query_schema_types_do_not_panic() {
        let _ = schema_for!(super::ReserveNodeSlotsRequest);
        let _ = schema_for!(super::ReserveNodeSlotsResponse);
        let _ = schema_for!(super::ExecuteFragmentRequest);
        let _ = schema_for!(super::WorkerAttemptFrame);
    }
}
