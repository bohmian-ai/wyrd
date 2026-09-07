//! Canonical Redux built-in table definitions and pure table-layer transforms.

use arrow::datatypes::{DataType, Field, Fields, Schema, SchemaRef, TimeUnit as ArrowTimeUnit};
use arrow::record_batch::RecordBatch;
use sha2::{Digest, Sha256};
use thiserror::Error;
use wyrd_spec::vala::api::{
    NullOrderWire, PhysicalLayoutWire, SortDirectionWire, SortKeyWire, TimeGranularityWire,
};
use wyrd_spec::vala::managed_columns::{
    DATA_TENANT_ID, WYRD_BATCH_ID, WYRD_EVENT_TIME, WYRD_INGESTED_AT, WYRD_ROW_ORDINAL,
    is_reserved_managed_column,
};

pub mod audit;
pub mod dev;
pub mod drift;
pub mod eval;
pub mod fields;
pub mod genai;
pub mod logs;
pub mod managed_columns;
pub mod metrics;
pub mod signal;
pub mod traces;

pub use audit::AuditLogTable;
pub use dev::AgentTracesTable;
pub use drift::ObservationsTable;
pub use eval::{AssertionsTable, RunsTable};
pub use genai::{EmbeddingsTable, MemoryTable, MessagesTable, ToolCallsTable};
pub use logs::RecordsTable;
pub use metrics::PointsTable;
pub use traces::{EventsTable, LinksTable, SpansTable};

/// Errors from pure table schema and projection operations.
#[derive(Debug, Error)]
pub enum TableError {
    /// The source batch is missing a required field or uses an unexpected type.
    #[error("table projection error: {0}")]
    Internal(String),
}

/// Correlation columns appended to a built-in table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorrelationPolicy {
    /// Traces, metrics, logs, `GenAI`, eval, and drift rows.
    Observation,
    /// Agent traces own their `run_id` content column.
    CodeAxis,
    /// Audit content has its own identity columns.
    None,
}

impl CorrelationPolicy {
    /// Return the universal correlation columns appended by this policy.
    #[must_use]
    pub const fn appended_correlation_columns(self) -> &'static [&'static str] {
        match self {
            Self::Observation => &[
                wyrd_spec::vala::RUN_ID,
                wyrd_spec::vala::CARD_UID,
                wyrd_spec::vala::PRINCIPAL_ID,
            ],
            Self::CodeAxis => &[wyrd_spec::vala::CARD_UID, wyrd_spec::vala::PRINCIPAL_ID],
            Self::None => &[],
        }
    }
}

/// Payload handling classification for a built-in table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadClass {
    /// No sensitive-default columns.
    Standard,
    /// Sensitive columns require the elevated read/write handling.
    Sensitive,
}

/// Builds one ascending built-in sort key with nulls last.
#[must_use]
pub fn sort_asc(column: &str) -> SortKeyWire {
    SortKeyWire {
        column: column.to_owned(),
        direction: SortDirectionWire::Asc,
        null_order: NullOrderWire::Last,
    }
}

/// Builds one ascending built-in sort key with nulls first.
///
/// Used where the declared column is nullable and the built-in wants the
/// unknown group grouped ahead of the known values.
#[must_use]
pub fn sort_asc_nulls_first(column: &str) -> SortKeyWire {
    SortKeyWire {
        column: column.to_owned(),
        direction: SortDirectionWire::Asc,
        null_order: NullOrderWire::First,
    }
}

/// Builds one descending built-in sort key with nulls last.
#[must_use]
pub fn sort_desc(column: &str) -> SortKeyWire {
    SortKeyWire {
        column: column.to_owned(),
        direction: SortDirectionWire::Desc,
        null_order: NullOrderWire::Last,
    }
}

/// Builds one built-in physical-layout declaration on hourly
/// `wyrd_event_time`.
///
/// Every built-in partitions hourly; only its sort keys and Bloom columns
/// differ. The catalog resolves the result through the one
/// [`crate::catalog::PhysicalLayout::resolve`] entry point a caller
/// registration uses, which unions the managed Bloom floor, so a built-in
/// declares only what is specific to it.
#[must_use]
pub fn hourly_layout(sort_keys: Vec<SortKeyWire>, bloom_columns: &[&str]) -> PhysicalLayoutWire {
    PhysicalLayoutWire {
        partition_granularity: TimeGranularityWire::Hour,
        sort_keys,
        bloom_columns: bloom_columns.iter().map(|c| (*c).to_owned()).collect(),
    }
}

/// Entity mapping used by time-bound acceleration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityBoundsMapping {
    /// Entity kind.
    pub entity_kind: String,
    /// Physical identifier column.
    pub entity_id_column: String,
}

/// A table-owned canonical value validator.
///
/// A canonical signal table supplies one of these so the registry can enforce
/// the value-level rules its Arrow schema alone cannot express — canonical
/// payload encoding, and for metrics the kind/column agreement — without any
/// caller matching on a table name.
pub type CanonicalBatchValidator = fn(&RecordBatch) -> Result<RecordBatch, String>;

/// Canonical immutable definition for one built-in table.
#[derive(Debug, Clone, Copy)]
pub struct BuiltinTableDefinition {
    /// Logical namespace segment.
    pub namespace: &'static str,
    /// Logical table name.
    pub name: &'static str,
    /// Correlation policy.
    pub correlation_policy: CorrelationPolicy,
    /// Payload classification.
    pub payload_class: PayloadClass,
    /// Sensitive payload columns.
    pub sensitive_payload_columns: &'static [&'static str],
    /// User schema constructor.
    pub arrow_fields: fn() -> Vec<Field>,
    /// User schema fingerprint constructor.
    pub schema_fingerprint: fn() -> [u8; 32],
    /// Full physical schema constructor.
    pub schema: fn() -> SchemaRef,
    /// Canonical signal ledger constructor, `None` for a pre-declared table.
    pub canonical_fields: fn() -> Option<&'static [fields::CanonicalField]>,
    /// Canonical physical fingerprint constructor, `None` when not canonical.
    pub canonical_physical_fingerprint: fn() -> Option<CanonicalPhysicalFingerprint>,
    /// Canonical value validator, `None` for a pre-declared table.
    pub canonical_validator: Option<CanonicalBatchValidator>,
    /// Engine-owned physical-layout declaration.
    ///
    /// This is the built-in's single statement of partition granularity, sort
    /// intent, and Bloom intent. The catalog resolves it once and every
    /// downstream representation — Iceberg spec and sort order, the control
    /// row, the Parquet Bloom recipe, and every partition identity — is derived
    /// from that one canonical layout.
    pub physical_layout: fn() -> PhysicalLayoutWire,
    /// Optional entity mapping.
    pub entity_bounds_mapping: fn() -> Option<EntityBoundsMapping>,
}

/// A table definition implemented by the canonical registry.
pub trait DomainTable: Send + Sync + 'static {
    /// Logical namespace segment.
    const NAMESPACE: &'static str;
    /// Logical table name.
    const NAME: &'static str;
    /// Correlation policy.
    const CORRELATION_POLICY: CorrelationPolicy;
    /// Payload classification.
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Standard;
    /// Sensitive payload fields.
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] = &[];

    /// The canonical signal ledger, when this table owns an `OTel` signal.
    ///
    /// Returning `Some` makes the table canonical: its physical schema, stable
    /// field ids, sensitivity metadata, and canonical physical fingerprint are
    /// all derived from the returned declaration rather than from a separately
    /// maintained Arrow field list. Pre-declared tables return `None` and keep
    /// their existing auto-assigned Iceberg ids and user-schema fingerprint.
    fn canonical_fields() -> Option<&'static [fields::CanonicalField]> {
        None
    }

    /// The table-owned canonical value validator, when this table is canonical.
    ///
    /// A canonical signal table sets this to the function that enforces its
    /// value-level rules; a pre-declared table leaves it `None` and is value
    /// validated by its declared Arrow schema alone.
    const CANONICAL_VALIDATOR: Option<CanonicalBatchValidator> = None;

    /// User-owned fields, excluding correlation and system fields.
    fn arrow_fields() -> Vec<Field>;

    /// Full physical schema.
    ///
    /// A canonical table derives its whole physical schema, including the
    /// Observation envelope's stable ids, from its ledger. Every other table
    /// keeps the existing managed-column append.
    fn schema() -> SchemaRef {
        match Self::canonical_fields() {
            Some(declared) => SchemaRef::new(Schema::new(
                managed_columns::canonical_physical_fields(declared),
            )),
            None => SchemaRef::new(Schema::new(managed_columns::ensure_managed_columns(
                Self::arrow_fields(),
                Self::CORRELATION_POLICY,
            ))),
        }
    }

    /// The canonical physical fingerprint, when this table owns an `OTel` signal.
    ///
    /// This is a different identity from [`Self::schema_fingerprint`], which
    /// keeps its user-schema meaning for catalog rows. The canonical physical
    /// fingerprint covers the complete physical schema — envelope included —
    /// with every stable id, nested child, nullability, sensitivity, and
    /// metadata entry, and is what the Arrow validator, the Gate validation
    /// proof, the Parquet/Iceberg checks, and recovery compare.
    ///
    /// # Panics
    ///
    /// Panics when a canonical ledger cannot be fingerprinted, which would mean
    /// the declaration escaped the closed canonical type set.
    fn canonical_physical_fingerprint() -> Option<CanonicalPhysicalFingerprint> {
        Self::canonical_fields().map(|_| {
            CanonicalPhysicalFingerprint::from_physical_fields(Self::schema().fields())
                .expect("a canonical ledger is always fingerprintable")
        })
    }

    /// User-schema fingerprint.
    fn schema_fingerprint() -> [u8; 32] {
        fingerprint_fields(&Self::arrow_fields())
    }

    /// Engine-owned physical-layout declaration.
    ///
    /// Defaults to hourly `wyrd_event_time` with no additional Bloom intent,
    /// which resolves to `wyrd_event_time DESC NULLS LAST` and the managed
    /// Bloom floor.
    fn physical_layout() -> PhysicalLayoutWire {
        hourly_layout(vec![sort_desc(WYRD_EVENT_TIME)], &[])
    }
    /// Optional entity bounds mapping.
    fn entity_bounds_mapping() -> Option<EntityBoundsMapping> {
        None
    }
}

/// Validate user fields against all server-owned columns for a policy.
pub fn reject_reserved_domain_fields(
    user_fields: &[&str],
    policy: CorrelationPolicy,
) -> Result<(), TableError> {
    let appended = policy.appended_correlation_columns();
    for name in user_fields {
        if is_reserved_managed_column(name)
            || [
                WYRD_EVENT_TIME,
                WYRD_INGESTED_AT,
                WYRD_BATCH_ID,
                WYRD_ROW_ORDINAL,
                DATA_TENANT_ID,
            ]
            .contains(name)
            || appended.contains(name)
        {
            return Err(TableError::Internal(format!("reserved column: {name}")));
        }
    }
    Ok(())
}

/// Versioned recursive fingerprint over one complete canonical physical schema.
///
/// This is a distinct identity from [`crate::schema::SchemaFingerprint`], which
/// keeps its existing user-schema meaning for `BifrostTableEntry.fingerprint`
/// and dynamic-table catalog rows. A canonical physical fingerprint commits to
/// the *whole* physical schema — envelope fields included — plus every stable
/// field id, nested child, nullability, sensitivity, and semantic metadata
/// entry, so a canonical built-in cannot drift in any of those dimensions
/// without the fingerprint changing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CanonicalPhysicalFingerprint(pub [u8; 32]);

impl CanonicalPhysicalFingerprint {
    /// Compute the fingerprint of one complete canonical physical schema.
    ///
    /// # Errors
    ///
    /// Returns [`TableError::Internal`] when a field carries no parsable
    /// `PARQUET:field_id`, no `wyrd:sensitive` marker, or an Arrow type outside
    /// the closed canonical set.
    pub fn from_physical_fields(fields: &Fields) -> Result<Self, TableError> {
        let mut hasher = Sha256::new();
        hasher.update(canonical_physical_fingerprint_bytes(fields)?);
        Ok(Self(hasher.finalize().into()))
    }

    /// Render the fingerprint as lowercase hexadecimal.
    #[must_use]
    pub fn to_hex(self) -> String {
        hex::encode(self.0)
    }
}

/// The two schema identities a resolved table registration carries.
///
/// `catalog_fingerprint` is the existing user-schema fingerprint every table
/// has, and is what `BifrostTableEntry.fingerprint` and the dynamic-table
/// catalog rows mean. `canonical_physical_fingerprint` is present only for a
/// canonical signal built-in and covers its complete physical schema. Keeping
/// both on one value is what stops a caller from comparing one meaning against
/// the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedSchemaIdentity {
    /// User-schema fingerprint stored in the catalog control row.
    pub catalog_fingerprint: crate::schema::SchemaFingerprint,
    /// Complete physical fingerprint, present only for a canonical built-in.
    pub canonical_physical_fingerprint: Option<CanonicalPhysicalFingerprint>,
}

impl ResolvedSchemaIdentity {
    /// Resolve both identities for one built-in definition.
    #[must_use]
    pub fn for_builtin(definition: &BuiltinTableDefinition) -> Self {
        Self {
            catalog_fingerprint: crate::schema::SchemaFingerprint::from_arrow_schema(&Schema::new(
                (definition.arrow_fields)(),
            )),
            canonical_physical_fingerprint: (definition.canonical_physical_fingerprint)(),
        }
    }

    /// Resolve the identity of a caller-registered dynamic table.
    ///
    /// A dynamic table never has a canonical physical fingerprint: its Iceberg
    /// ids are auto-assigned and its catalog fingerprint is the only identity
    /// its registration contract commits to.
    #[must_use]
    pub fn for_dynamic(user_fields: &[Field]) -> Self {
        Self {
            catalog_fingerprint: crate::schema::SchemaFingerprint::from_arrow_schema(&Schema::new(
                user_fields.to_vec(),
            )),
            canonical_physical_fingerprint: None,
        }
    }
}

/// Convert one physical Arrow schema into its Iceberg schema.
///
/// A canonical signal table declares an immutable stable id on every field,
/// including every nested child, so its Iceberg schema must adopt those ids
/// rather than a positional assignment. A dynamic user table declares none and
/// keeps the existing automatic assignment. The distinction is read from the
/// schema itself, so no consumer needs to know which table it holds.
///
/// # Errors
///
/// Propagates the Iceberg conversion error when the schema has no Iceberg
/// projection.
pub fn iceberg_schema_for(schema: &Schema) -> Result<iceberg::spec::Schema, iceberg::Error> {
    if declares_stable_field_ids(schema) {
        iceberg::arrow::arrow_schema_to_schema(schema)
    } else {
        iceberg::arrow::arrow_schema_to_schema_auto_assign_ids(schema)
    }
}

/// Report whether every top-level field carries a stable field id.
///
/// An empty schema declares none, which keeps the automatic assignment for the
/// degenerate case rather than claiming canonical identity.
fn declares_stable_field_ids(schema: &Schema) -> bool {
    !schema.fields().is_empty()
        && schema
            .fields()
            .iter()
            .all(|field| field.metadata().contains_key(fields::PARQUET_FIELD_ID))
}

/// Report whether an actual Arrow type is the expected one after a storage
/// round trip.
///
/// The installed Iceberg conversion normalizes some Arrow types on the way
/// back — a binary or string column widens to its large variant, a list may
/// return as a large list, and a zoned timestamp may return as `+00:00` — so a
/// physical table's schema is compared by shape rather than by exact equality.
/// Nested children are compared by name, never by position.
#[must_use]
pub fn arrow_type_shape_matches(
    expected: &arrow::datatypes::DataType,
    actual: &arrow::datatypes::DataType,
) -> bool {
    use arrow::datatypes::DataType as Arrow;
    if expected.equals_datatype(actual) {
        return true;
    }
    match (expected, actual) {
        (Arrow::Binary, Arrow::LargeBinary)
        | (Arrow::LargeBinary, Arrow::Binary)
        | (Arrow::Utf8, Arrow::LargeUtf8)
        | (Arrow::LargeUtf8, Arrow::Utf8) => true,
        (
            Arrow::List(expected) | Arrow::LargeList(expected),
            Arrow::List(actual) | Arrow::LargeList(actual),
        ) => {
            expected.is_nullable() == actual.is_nullable()
                && arrow_type_shape_matches(expected.data_type(), actual.data_type())
        }
        (Arrow::Struct(expected), Arrow::Struct(actual)) => {
            expected.len() == actual.len()
                && expected.iter().all(|field| {
                    actual
                        .iter()
                        .find(|candidate| candidate.name() == field.name())
                        .is_some_and(|candidate| {
                            field.is_nullable() == candidate.is_nullable()
                                && arrow_type_shape_matches(
                                    field.data_type(),
                                    candidate.data_type(),
                                )
                        })
                })
        }
        (
            arrow::datatypes::DataType::Timestamp(expected_unit, Some(expected_timezone)),
            arrow::datatypes::DataType::Timestamp(actual_unit, Some(actual_timezone)),
        ) => {
            expected_unit == actual_unit
                && ((expected_timezone.as_ref() == "UTC" && actual_timezone.as_ref() == "+00:00")
                    || (expected_timezone.as_ref() == "+00:00"
                        && actual_timezone.as_ref() == "UTC"))
        }
        _ => false,
    }
}

/// Version byte prefixing every canonical physical fingerprint encoding.
///
/// Any change to the encoding below must take the next unused version byte so
/// an old and a new encoding can never collide.
const CANONICAL_FINGERPRINT_VERSION: u8 = 1;

/// Encode one canonical physical schema into its pre-hash fingerprint bytes.
///
/// The encoding is a version byte, the top-level field count, and then each
/// field in declared order as: stable id, length-prefixed name, type bytes,
/// nullability, sensitivity, its metadata entries in ascending raw key-byte
/// order, and finally its nested child count followed depth-first by the same
/// record for each child. Every count and length is an unsigned big-endian
/// `u32`; field ids and fixed-binary widths are signed big-endian `i32`.
///
/// The bytes are exposed separately from the hash so a golden test can pin the
/// exact encoding rather than only its digest.
///
/// # Errors
///
/// Returns [`TableError::Internal`] for a missing or unparsable stable id, a
/// missing sensitivity marker, or an Arrow type with no pinned type tag.
pub fn canonical_physical_fingerprint_bytes(fields: &Fields) -> Result<Vec<u8>, TableError> {
    let mut bytes = vec![CANONICAL_FINGERPRINT_VERSION];
    encode_field_sequence(fields, &mut bytes)?;
    Ok(bytes)
}

/// Encode one ordered field sequence as a count followed by each field record.
///
/// # Errors
///
/// Propagates every [`encode_field`] failure.
fn encode_field_sequence(fields: &Fields, out: &mut Vec<u8>) -> Result<(), TableError> {
    out.extend_from_slice(&count_u32(fields.len())?.to_be_bytes());
    for field in fields {
        encode_field(field, out)?;
    }
    Ok(())
}

/// Encode one field record, then recurse depth-first into its children.
///
/// # Errors
///
/// Returns [`TableError::Internal`] when the field lacks a parsable stable id
/// or sensitivity marker, or when its type has no pinned tag.
fn encode_field(field: &Field, out: &mut Vec<u8>) -> Result<(), TableError> {
    out.extend_from_slice(&stable_field_id(field)?.to_be_bytes());
    encode_len_prefixed(field.name().as_bytes(), out)?;
    encode_data_type(field.data_type(), out)?;
    out.push(u8::from(field.is_nullable()));
    out.push(u8::from(sensitivity(field)?));

    let metadata = field.metadata();
    out.extend_from_slice(&count_u32(metadata.len())?.to_be_bytes());
    let mut entries: Vec<(&String, &String)> = metadata.iter().collect();
    entries.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    for (key, value) in entries {
        encode_len_prefixed(key.as_bytes(), out)?;
        encode_len_prefixed(value.as_bytes(), out)?;
    }

    let children = child_fields(field.data_type());
    match children {
        Some(children) => encode_field_sequence(&children, out)?,
        None => out.extend_from_slice(&0_u32.to_be_bytes()),
    }
    Ok(())
}

/// Encode one pinned type tag and its inline parameters.
///
/// List and struct parameters are deliberately absent here: they exist only in
/// the following child records, so a nested type's shape is committed exactly
/// once.
///
/// # Errors
///
/// Returns [`TableError::Internal`] for any Arrow type outside the closed
/// canonical set.
fn encode_data_type(data_type: &DataType, out: &mut Vec<u8>) -> Result<(), TableError> {
    match data_type {
        DataType::Boolean => out.push(0x01),
        DataType::Int32 => out.push(0x02),
        DataType::Int64 => out.push(0x03),
        DataType::UInt32 => out.push(0x04),
        DataType::UInt64 => out.push(0x05),
        DataType::Float64 => out.push(0x06),
        DataType::Utf8 => out.push(0x07),
        DataType::Binary => out.push(0x08),
        DataType::FixedSizeBinary(width) => {
            out.push(0x09);
            out.extend_from_slice(&width.to_be_bytes());
        }
        DataType::Timestamp(unit, zone) => {
            out.push(0x0a);
            out.push(match unit {
                ArrowTimeUnit::Second => 0x00,
                ArrowTimeUnit::Millisecond => 0x01,
                ArrowTimeUnit::Microsecond => 0x02,
                ArrowTimeUnit::Nanosecond => 0x03,
            });
            match zone {
                None => out.push(0x00),
                Some(zone) => {
                    out.push(0x01);
                    encode_len_prefixed(zone.as_bytes(), out)?;
                }
            }
        }
        DataType::List(_) => out.push(0x0b),
        DataType::Struct(_) => out.push(0x0c),
        other => {
            return Err(TableError::Internal(format!(
                "canonical schema has no fingerprint tag for {other}"
            )));
        }
    }
    Ok(())
}

/// Return the ordered child fields of a nested Arrow type.
///
/// `None` marks a scalar, which encodes a zero child count.
fn child_fields(data_type: &DataType) -> Option<Fields> {
    match data_type {
        DataType::List(child) => Some(Fields::from(vec![child.as_ref().clone()])),
        DataType::Struct(children) => Some(children.clone()),
        _ => None,
    }
}

/// Read one field's table-local stable identity from its Arrow metadata.
///
/// # Errors
///
/// Returns [`TableError::Internal`] when the metadata entry is absent or is not
/// a decimal `i32`.
pub fn stable_field_id(field: &Field) -> Result<i32, TableError> {
    field
        .metadata()
        .get(fields::PARQUET_FIELD_ID)
        .ok_or_else(|| {
            TableError::Internal(format!("field {} carries no stable id", field.name()))
        })?
        .parse::<i32>()
        .map_err(|error| {
            TableError::Internal(format!(
                "field {} has an unparsable stable id: {error}",
                field.name()
            ))
        })
}

/// Read one field's declared sensitivity from its Arrow metadata.
///
/// # Errors
///
/// Returns [`TableError::Internal`] when the marker is absent or is not
/// `true`/`false`.
pub fn sensitivity(field: &Field) -> Result<bool, TableError> {
    match field
        .metadata()
        .get(fields::WYRD_SENSITIVE)
        .map(String::as_str)
    {
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        _ => Err(TableError::Internal(format!(
            "field {} carries no sensitivity marker",
            field.name()
        ))),
    }
}

/// Encode one byte string as an unsigned big-endian `u32` length plus bytes.
///
/// # Errors
///
/// Returns [`TableError::Internal`] when the length exceeds `u32`.
fn encode_len_prefixed(value: &[u8], out: &mut Vec<u8>) -> Result<(), TableError> {
    out.extend_from_slice(&count_u32(value.len())?.to_be_bytes());
    out.extend_from_slice(value);
    Ok(())
}

/// Narrow one count to the encoding's unsigned big-endian `u32` width.
///
/// # Errors
///
/// Returns [`TableError::Internal`] when the count exceeds `u32`.
fn count_u32(value: usize) -> Result<u32, TableError> {
    u32::try_from(value)
        .map_err(|_| TableError::Internal("canonical schema count exceeds u32".to_owned()))
}

/// Fingerprint one built-in's declared fields for catalog registration.
///
/// Delegates to [`crate::schema::fingerprint::SchemaFingerprint`] so a
/// registered table and an ingested batch of the same shape always agree.
fn fingerprint_fields(fields: &[Field]) -> [u8; 32] {
    let owned: Vec<std::sync::Arc<Field>> =
        fields.iter().cloned().map(std::sync::Arc::new).collect();
    crate::schema::fingerprint::SchemaFingerprint::from_fields(owned.iter()).0
}

const fn definition<T: DomainTable>() -> BuiltinTableDefinition {
    BuiltinTableDefinition {
        namespace: T::NAMESPACE,
        name: T::NAME,
        correlation_policy: T::CORRELATION_POLICY,
        payload_class: T::PAYLOAD_CLASS,
        sensitive_payload_columns: T::SENSITIVE_PAYLOAD_COLUMNS,
        arrow_fields: T::arrow_fields,
        schema_fingerprint: T::schema_fingerprint,
        schema: T::schema,
        canonical_fields: T::canonical_fields,
        canonical_physical_fingerprint: T::canonical_physical_fingerprint,
        canonical_validator: T::CANONICAL_VALIDATOR,
        physical_layout: T::physical_layout,
        entity_bounds_mapping: T::entity_bounds_mapping,
    }
}

/// The single canonical list of fourteen server-owned built-in tables.
pub static BUILTIN_TABLES: [BuiltinTableDefinition; 14] = [
    definition::<SpansTable>(),
    definition::<EventsTable>(),
    definition::<LinksTable>(),
    definition::<MessagesTable>(),
    definition::<EmbeddingsTable>(),
    definition::<ToolCallsTable>(),
    definition::<MemoryTable>(),
    definition::<PointsTable>(),
    definition::<RecordsTable>(),
    definition::<RunsTable>(),
    definition::<AssertionsTable>(),
    definition::<ObservationsTable>(),
    definition::<AgentTracesTable>(),
    definition::<AuditLogTable>(),
];

/// Return all immutable built-in definitions.
#[must_use]
pub const fn builtin_tables() -> &'static [BuiltinTableDefinition; 14] {
    &BUILTIN_TABLES
}

/// Resolve a canonical built-in by logical namespace and table name.
#[must_use]
pub fn builtin_table(namespace: &str, name: &str) -> Option<&'static BuiltinTableDefinition> {
    BUILTIN_TABLES
        .iter()
        .find(|definition| definition.namespace == namespace && definition.name == name)
}

/// Return the fourteen built-in logical FQNs in canonical order.
#[must_use]
pub fn builtin_fqns() -> Vec<String> {
    BUILTIN_TABLES
        .iter()
        .map(|definition| format!("vala.{}.{}", definition.namespace, definition.name))
        .collect()
}

#[cfg(test)]
mod tests {
    use arrow::datatypes::{DataType, TimeUnit as ArrowTimeUnit};
    use arrow::record_batch::RecordBatch;
    use std::collections::HashMap;
    use std::sync::Arc;
    use wyrd_tonic::otlp::common::v1::any_value::Value;
    use wyrd_tonic::otlp::common::v1::{AnyValue, KeyValue};

    use super::*;
    use wyrd_spec::vala::{CARD_UID, PRINCIPAL_ID, RUN_ID};

    #[test]
    fn registry_contains_exactly_the_fourteen_builtins() {
        assert_eq!(BUILTIN_TABLES.len(), 14);
        assert!(builtin_table("genai", "memory").is_some());
        assert_eq!(builtin_fqns().len(), 14);
    }

    #[test]
    fn registry_definition_and_trait_schema_match() {
        let spans = builtin_table("traces", "spans").expect("spans definition");
        assert_eq!(
            (spans.schema_fingerprint)(),
            SpansTable::schema_fingerprint()
        );
        assert_eq!((spans.schema)(), SpansTable::schema());
    }

    #[test]
    fn every_builtin_has_stable_schema_fingerprint_and_physical_columns() {
        for definition in builtin_tables() {
            let fields = (definition.arrow_fields)();
            let fingerprint = (definition.schema_fingerprint)();
            assert_ne!(
                fingerprint, [0; 32],
                "{}.{}",
                definition.namespace, definition.name
            );
            assert_eq!(fingerprint, fingerprint_fields(&fields));
            let schema = (definition.schema)();
            assert!(schema.field_with_name(WYRD_EVENT_TIME).is_ok());
            assert!(schema.field_with_name(DATA_TENANT_ID).is_ok());
            assert_eq!(
                (definition.physical_layout)().partition_granularity,
                TimeGranularityWire::Hour
            );
        }
    }

    /// The one authoritative physical-layout contract for every registration
    /// path: all fourteen built-ins plus each dynamic declaration class.
    ///
    /// Built-ins and dynamic tables run the same single resolution entry point,
    /// so this proves in one place that the system injects no sort key, that
    /// `data_tenant_id` appears in neither canonical list, that the managed
    /// Bloom floor is always present for schema-present columns, that the
    /// declared order is otherwise preserved, that omission and an explicit
    /// empty declaration resolve identically, and that every invalid class is
    /// refused without mutating anything.
    #[test]
    fn physical_layout_contract_all_builtins() {
        let dynamic_schema = Schema::new(vec![
            Field::new(
                WYRD_EVENT_TIME,
                DataType::Timestamp(ArrowTimeUnit::Microsecond, None),
                false,
            ),
            Field::new(DATA_TENANT_ID, DataType::Utf8, false),
            Field::new(RUN_ID, DataType::Utf8, true),
            Field::new(CARD_UID, DataType::Utf8, true),
            Field::new(PRINCIPAL_ID, DataType::Utf8, true),
            Field::new("customer", DataType::Utf8, true),
        ]);
        let table = "vala.datasets.dynamic";

        assert_builtin_layouts_are_canonical_fixed_points();
        assert_dynamic_declarations_resolve(&dynamic_schema, table);
        assert_invalid_declarations_are_refused(&dynamic_schema, table);
    }

    /// Proves every built-in table declares a layout that resolves and is a
    /// fixed point of resolution.
    ///
    /// Each built-in must partition hourly, resolve through the same
    /// [`crate::catalog::layout::PhysicalLayout::resolve`] entry point a caller
    /// uses, carry the managed Bloom floor for every column its schema actually
    /// has, name only columns that exist, and name `data_tenant_id` in neither
    /// canonical list. Re-resolving the stored wire form must reproduce it byte
    /// for byte, which is what proves the floor is unioned once rather than
    /// accreted on every load.
    ///
    /// # Panics
    ///
    /// Panics when any built-in violates one of those invariants.
    fn assert_builtin_layouts_are_canonical_fixed_points() {
        for definition in builtin_tables() {
            let fqn = format!("vala.{}.{}", definition.namespace, definition.name);
            let schema = (definition.schema)();
            let declared = (definition.physical_layout)();
            let canonical =
                crate::catalog::layout::PhysicalLayout::resolve(&fqn, &schema, Some(&declared))
                    .unwrap_or_else(|error| panic!("{fqn} declares a canonical layout: {error}"));

            assert_eq!(
                canonical.granularity(),
                crate::catalog::layout::TimeGranularity::Hour,
                "{fqn} partitions hourly"
            );
            // Nothing is injected: the canonical order is exactly what the
            // built-in declared, in its declared order.
            assert_eq!(
                canonical
                    .sort_keys()
                    .iter()
                    .map(|key| key.column().to_owned())
                    .collect::<Vec<_>>(),
                declared
                    .sort_keys
                    .iter()
                    .map(|key| key.column.clone())
                    .collect::<Vec<_>>(),
                "{fqn} canonical sort order must equal its declaration"
            );
            for column in crate::catalog::layout::MANAGED_BLOOM_FLOOR {
                if schema.field_with_name(column).is_ok() {
                    assert!(
                        canonical.bloom_columns().contains(&column.to_owned()),
                        "{fqn} Bloom union omits schema-present managed column {column}"
                    );
                }
            }
            assert!(
                !canonical
                    .bloom_columns()
                    .contains(&DATA_TENANT_ID.to_owned()),
                "{fqn} Blooms a per-file constant"
            );
            for column in canonical.bloom_columns() {
                assert!(
                    schema.field_with_name(column).is_ok(),
                    "{fqn} Bloom column {column} is absent from its schema"
                );
            }
            for key in canonical.sort_keys() {
                assert_ne!(
                    key.column(),
                    DATA_TENANT_ID,
                    "{fqn} sorts on a per-file constant"
                );
                assert!(
                    schema.field_with_name(key.column()).is_ok(),
                    "{fqn} sort column {} is absent from its schema",
                    key.column()
                );
            }
            // Resolution is a fixed point: the stored form of a canonical
            // layout re-resolves to itself rather than accreting the Bloom
            // floor a second time.
            let stored = canonical.to_wire();
            let round_tripped =
                crate::catalog::layout::PhysicalLayout::from_stored_wire(&fqn, &schema, &stored)
                    .unwrap_or_else(|error| panic!("{fqn} stored layout re-resolves: {error}"));
            assert_eq!(round_tripped.to_wire(), stored);
        }
    }

    /// Proves dynamic declarations resolve through the same entry point
    /// built-ins use.
    ///
    /// Covers the three legal shapes: omission resolving to the hourly
    /// event-time default with the managed floor, an explicit empty declaration
    /// resolving to the same sort order as omission because the system injects
    /// nothing, and a custom declaration keeping its own order while unioning
    /// the managed floor.
    ///
    /// # Panics
    ///
    /// Panics when any of those three shapes resolves to the wrong layout.
    fn assert_dynamic_declarations_resolve(dynamic_schema: &Schema, table: &str) {
        // Dynamic declarations share the same resolution. The fixture schema
        // carries the full managed floor plus one user column.

        // Omission resolves to the hourly event-time default with the managed
        // floor and nothing else.
        let omitted = crate::catalog::layout::PhysicalLayout::resolve(table, dynamic_schema, None)
            .expect("omitted layout resolves to the default");
        assert_eq!(
            omitted.granularity(),
            crate::catalog::layout::TimeGranularity::Hour
        );
        assert_eq!(
            omitted
                .sort_keys()
                .iter()
                .map(|key| key.column().to_owned())
                .collect::<Vec<_>>(),
            vec![WYRD_EVENT_TIME.to_owned()]
        );
        assert_eq!(
            omitted.bloom_columns(),
            crate::catalog::layout::MANAGED_BLOOM_FLOOR
                .iter()
                .map(|column| (*column).to_owned())
                .collect::<Vec<_>>()
                .as_slice()
        );

        // An explicit empty declaration means exactly what omission means for
        // the sort order, because nothing is injected ahead of a declared key.
        let explicit_empty = PhysicalLayoutWire {
            partition_granularity: TimeGranularityWire::Day,
            sort_keys: Vec::new(),
            bloom_columns: Vec::new(),
        };
        let empty = crate::catalog::layout::PhysicalLayout::resolve(
            table,
            dynamic_schema,
            Some(&explicit_empty),
        )
        .expect("explicit empty layout resolves");
        assert_eq!(
            empty.granularity(),
            crate::catalog::layout::TimeGranularity::Day
        );
        assert_eq!(empty.sort_keys(), omitted.sort_keys());
        assert_eq!(empty.bloom_columns(), omitted.bloom_columns());

        // A custom declaration keeps its own order with nothing prepended and
        // unions rather than replaces the managed Bloom floor.
        let custom = PhysicalLayoutWire {
            partition_granularity: TimeGranularityWire::Hour,
            sort_keys: vec![sort_desc("customer"), sort_asc(WYRD_EVENT_TIME)],
            bloom_columns: vec!["customer".to_owned()],
        };
        let resolved =
            crate::catalog::layout::PhysicalLayout::resolve(table, dynamic_schema, Some(&custom))
                .expect("custom layout resolves");
        let sort_columns = resolved
            .sort_keys()
            .iter()
            .map(|key| key.column().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            sort_columns,
            vec!["customer".to_owned(), WYRD_EVENT_TIME.to_owned()]
        );
        assert!(resolved.bloom_columns().contains(&"customer".to_owned()));
        assert!(
            !resolved
                .bloom_columns()
                .contains(&DATA_TENANT_ID.to_owned())
        );
        for column in crate::catalog::layout::MANAGED_BLOOM_FLOOR {
            assert!(resolved.bloom_columns().contains(&column.to_owned()));
        }
    }

    /// Proves every invalid declaration class is refused as
    /// [`wyrd_spec::vala::BifrostError::InvalidPhysicalLayout`].
    ///
    /// Refusal is total: none of these declarations may yield a partially
    /// applied layout, so each is asserted to return an error rather than a
    /// repaired value.
    ///
    /// # Panics
    ///
    /// Panics when any invalid class is accepted or fails with a different
    /// error.
    fn assert_invalid_declarations_are_refused(dynamic_schema: &Schema, table: &str) {
        // Every invalid class is refused, and refusal is total: the same
        // declaration never yields a partially applied layout.
        let invalid_cases: Vec<(&str, PhysicalLayoutWire)> = vec![
            (
                "unknown sort column",
                PhysicalLayoutWire {
                    partition_granularity: TimeGranularityWire::Hour,
                    sort_keys: vec![sort_asc("absent")],
                    bloom_columns: Vec::new(),
                },
            ),
            (
                "duplicate sort column",
                PhysicalLayoutWire {
                    partition_granularity: TimeGranularityWire::Hour,
                    sort_keys: vec![sort_asc("customer"), sort_desc("customer")],
                    bloom_columns: Vec::new(),
                },
            ),
            (
                "a fifth declared sort key",
                PhysicalLayoutWire {
                    partition_granularity: TimeGranularityWire::Hour,
                    sort_keys: vec![
                        sort_asc(WYRD_EVENT_TIME),
                        sort_asc(DATA_TENANT_ID),
                        sort_asc(RUN_ID),
                        sort_asc(CARD_UID),
                        sort_asc(PRINCIPAL_ID),
                    ],
                    bloom_columns: Vec::new(),
                },
            ),
            (
                "unknown bloom column",
                PhysicalLayoutWire {
                    partition_granularity: TimeGranularityWire::Hour,
                    sort_keys: Vec::new(),
                    bloom_columns: vec!["absent".to_owned()],
                },
            ),
            (
                "duplicate bloom column",
                PhysicalLayoutWire {
                    partition_granularity: TimeGranularityWire::Hour,
                    sort_keys: Vec::new(),
                    bloom_columns: vec!["customer".to_owned(), "customer".to_owned()],
                },
            ),
        ];
        for (label, declared) in invalid_cases {
            let error = crate::catalog::layout::PhysicalLayout::resolve(
                table,
                dynamic_schema,
                Some(&declared),
            )
            .expect_err(label);
            assert!(
                matches!(
                    error,
                    wyrd_spec::vala::BifrostError::InvalidPhysicalLayout { .. }
                ),
                "{label} must fail as an invalid physical layout, got {error:?}"
            );
        }
    }

    #[test]
    fn correlation_policies_are_explicit() {
        assert!(
            CorrelationPolicy::Observation
                .appended_correlation_columns()
                .contains(&RUN_ID)
        );
        assert!(
            !CorrelationPolicy::CodeAxis
                .appended_correlation_columns()
                .contains(&RUN_ID)
        );
        assert!(
            !CorrelationPolicy::None
                .appended_correlation_columns()
                .contains(&CARD_UID)
        );
        assert!(
            CorrelationPolicy::Observation
                .appended_correlation_columns()
                .contains(&PRINCIPAL_ID)
        );
    }

    /// One attribute entry used by the round-trip fixtures.
    fn attribute(key: &str, value: Value) -> KeyValue {
        KeyValue {
            key: key.to_owned(),
            value: Some(AnyValue { value: Some(value) }),
        }
    }

    /// One span carrying a nested event, a link, and an entity reference.
    fn span_fixture() -> Vec<wyrd_tonic::otlp::trace::v1::ResourceSpans> {
        use wyrd_tonic::otlp::trace::v1::{ResourceSpans, ScopeSpans, Span, span};
        vec![ResourceSpans {
            resource: Some(wyrd_tonic::otlp::resource::v1::Resource {
                attributes: vec![attribute(
                    "service.name",
                    Value::StringValue("wyrd".to_owned()),
                )],
                dropped_attributes_count: 0,
                entity_refs: Vec::new(),
            }),
            scope_spans: vec![ScopeSpans {
                scope: None,
                spans: vec![Span {
                    trace_id: vec![1; 16],
                    span_id: vec![2; 8],
                    trace_state: String::new(),
                    parent_span_id: Vec::new(),
                    flags: 0,
                    name: "round-trip".to_owned(),
                    kind: span::SpanKind::Internal as i32,
                    start_time_unix_nano: 1,
                    end_time_unix_nano: 2,
                    attributes: vec![attribute("k", Value::IntValue(1))],
                    dropped_attributes_count: 0,
                    events: vec![span::Event {
                        time_unix_nano: 1,
                        name: "event".to_owned(),
                        attributes: Vec::new(),
                        dropped_attributes_count: 0,
                    }],
                    dropped_events_count: 0,
                    links: vec![span::Link {
                        trace_id: vec![3; 16],
                        span_id: vec![4; 8],
                        trace_state: String::new(),
                        attributes: Vec::new(),
                        dropped_attributes_count: 0,
                        flags: 0,
                    }],
                    dropped_links_count: 0,
                    status: None,
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }]
    }

    /// One log record carrying a body and an attribute payload.
    fn log_fixture() -> Vec<wyrd_tonic::otlp::logs::v1::ResourceLogs> {
        use wyrd_tonic::otlp::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
        vec![ResourceLogs {
            resource: None,
            scope_logs: vec![ScopeLogs {
                scope: None,
                log_records: vec![LogRecord {
                    body: Some(AnyValue {
                        value: Some(Value::StringValue("round-trip".to_owned())),
                    }),
                    attributes: vec![attribute("k", Value::BoolValue(true))],
                    ..LogRecord::default()
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }]
    }

    /// One histogram point carrying nested buckets and bounds.
    fn metric_fixture() -> Vec<wyrd_tonic::otlp::metrics::v1::ResourceMetrics> {
        use wyrd_tonic::otlp::metrics::v1::{
            Histogram, HistogramDataPoint, Metric, ResourceMetrics, ScopeMetrics, metric,
        };
        vec![ResourceMetrics {
            resource: None,
            scope_metrics: vec![ScopeMetrics {
                scope: None,
                metrics: vec![Metric {
                    name: "round.trip".to_owned(),
                    description: String::new(),
                    unit: String::new(),
                    metadata: Vec::new(),
                    data: Some(metric::Data::Histogram(Histogram {
                        data_points: vec![HistogramDataPoint {
                            attributes: Vec::new(),
                            start_time_unix_nano: 1,
                            time_unix_nano: 2,
                            count: 3,
                            sum: Some(1.0),
                            bucket_counts: vec![1, 2],
                            explicit_bounds: vec![0.5],
                            exemplars: Vec::new(),
                            flags: 0,
                            min: None,
                            max: None,
                        }],
                        aggregation_temporality: 1,
                    })),
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }]
    }

    /// Recursively assert two field sequences agree by declared identity.
    ///
    /// Binding is by stable id and name, never by position, so a reordering
    /// storage layer cannot silently pass.
    ///
    /// # Panics
    ///
    /// Panics when a declared field is missing from `actual`, or when its type,
    /// nullability, or stable id differs.
    fn assert_identity_matches(expected: &Fields, actual: &Fields, context: &str) {
        assert_eq!(
            expected.len(),
            actual.len(),
            "{context} keeps its declared field count"
        );
        for field in expected {
            let found = actual
                .iter()
                .find(|candidate| candidate.name() == field.name())
                .unwrap_or_else(|| panic!("{context} keeps field {}", field.name()));
            assert_field_pair(field, found, context);
        }
    }

    /// Assert one declared field and its round-tripped counterpart agree.
    ///
    /// A list's single element is matched positionally because the Iceberg
    /// projection renames it to `element`; everything else is matched by name.
    ///
    /// # Panics
    ///
    /// Panics when the stable id, nullability, type shape, or any nested child
    /// differs.
    fn assert_field_pair(expected: &Field, actual: &Field, context: &str) {
        assert_eq!(
            stable_field_id(expected).expect("declared stable id"),
            stable_field_id(actual).expect("round-tripped stable id"),
            "{context} keeps the stable id of {}",
            expected.name()
        );
        assert_eq!(
            expected.is_nullable(),
            actual.is_nullable(),
            "{context} keeps the nullability of {}",
            expected.name()
        );
        assert!(
            arrow_type_shape_matches(expected.data_type(), actual.data_type()),
            "{context} keeps the type of {}: {:?} became {:?}",
            expected.name(),
            expected.data_type(),
            actual.data_type()
        );
        match (expected.data_type(), actual.data_type()) {
            (
                DataType::List(expected_child) | DataType::LargeList(expected_child),
                DataType::List(actual_child) | DataType::LargeList(actual_child),
            ) => assert_field_pair(expected_child, actual_child, context),
            (DataType::Struct(expected_children), DataType::Struct(actual_children)) => {
                assert_identity_matches(expected_children, actual_children, context);
            }
            _ => {}
        }
    }

    /// Round-trip one canonical batch through Arrow IPC and read it back.
    ///
    /// # Panics
    ///
    /// Panics when the batch cannot be written or read back.
    fn ipc_round_trip(batch: &RecordBatch) -> RecordBatch {
        let mut buffer: Vec<u8> = Vec::new();
        let mut writer = arrow::ipc::writer::StreamWriter::try_new(&mut buffer, &batch.schema())
            .expect("canonical schema is IPC writable");
        writer
            .write(batch)
            .expect("canonical batch is IPC writable");
        writer.finish().expect("the IPC stream finishes");
        let mut reader =
            arrow::ipc::reader::StreamReader::try_new(std::io::Cursor::new(buffer), None)
                .expect("the IPC stream is readable");
        reader
            .next()
            .expect("the IPC stream carries one batch")
            .expect("the IPC batch decodes")
    }

    /// Round-trip one canonical batch through Parquet and read it back.
    ///
    /// # Panics
    ///
    /// Panics when the batch cannot be written or read back.
    fn parquet_round_trip(batch: &RecordBatch) -> RecordBatch {
        let mut buffer: Vec<u8> = Vec::new();
        let mut writer = parquet::arrow::ArrowWriter::try_new(&mut buffer, batch.schema(), None)
            .expect("canonical schema is Parquet writable");
        writer
            .write(batch)
            .expect("canonical batch is Parquet writable");
        writer.close().expect("the Parquet footer writes");
        let reader = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(
            bytes::Bytes::from(buffer),
        )
        .expect("the Parquet object is readable")
        .build()
        .expect("the Parquet reader builds");
        arrow::compute::concat_batches(
            &Arc::new(Schema::new(batch.schema().fields().clone())),
            &reader
                .map(|batch| batch.expect("the Parquet batch decodes"))
                .collect::<Vec<_>>(),
        )
        .expect("the Parquet batches concatenate")
    }

    /// The canonical schemas survive Arrow, IPC, Parquet, and Iceberg intact.
    ///
    /// # Panics
    ///
    /// Panics when a stable id, name, nested type, nullability, or value is
    /// lost by any leg of the round trip.
    #[test]
    fn canonical_signal_schemas_round_trip_arrow_parquet_and_iceberg() {
        let (spans, _) = crate::tables::traces::project_resource_spans(&span_fixture())
            .expect("the span fixture projects");
        let (logs, _) = crate::tables::logs::project_resource_logs(&log_fixture())
            .expect("the log fixture projects");
        let (points, _) = crate::tables::metrics::project_resource_metrics(&metric_fixture())
            .expect("the metric fixture projects");

        for (label, projected) in [("spans", spans), ("logs", logs), ("points", points)] {
            // The appended correlation columns are Scribe's stamping contract,
            // not declared ledger fields, so the canonical identity round trip
            // reads the ledger projection.
            let batch = crate::tables::signal::without_correlation_columns(&projected)
                .expect("the correlation columns split off cleanly");
            let declared = batch.schema().fields().clone();
            assert!(batch.num_rows() > 0, "{label} fixture produces rows");

            let ipc = ipc_round_trip(&batch);
            assert_identity_matches(&declared, ipc.schema().fields(), label);
            assert_eq!(ipc, batch, "{label} keeps every value through IPC");

            let parquet = parquet_round_trip(&batch);
            assert_identity_matches(&declared, parquet.schema().fields(), label);
            assert_eq!(
                parquet.columns(),
                batch.columns(),
                "{label} keeps every value through Parquet"
            );

            let iceberg = iceberg_schema_for(&Schema::new(declared.clone()))
                .expect("the canonical schema converts to Iceberg");
            let restored = iceberg::arrow::schema_to_arrow_schema(&iceberg)
                .expect("the Iceberg schema converts back to Arrow");
            assert_identity_matches(&declared, restored.fields(), label);
        }
    }

    /// The canonical fingerprint encoding is byte-stable.
    ///
    /// # Panics
    ///
    /// Panics when the pre-hash encoding or its digest differs from the pinned
    /// golden, which would silently change every canonical table's identity.
    #[test]
    fn canonical_physical_fingerprint_bytes_are_stable() {
        let child = Field::new("item", DataType::Utf8, true).with_metadata(HashMap::from([
            (fields::WYRD_SENSITIVE.to_owned(), "true".to_owned()),
            (fields::PARQUET_FIELD_ID.to_owned(), "3".to_owned()),
        ]));
        let naive = Field::new(
            "naive",
            DataType::Timestamp(ArrowTimeUnit::Microsecond, None),
            false,
        )
        .with_metadata(HashMap::from([
            (fields::WYRD_SENSITIVE.to_owned(), "false".to_owned()),
            (fields::PARQUET_FIELD_ID.to_owned(), "2".to_owned()),
        ]));
        let zoned = Field::new(
            "zoned",
            DataType::Timestamp(ArrowTimeUnit::Microsecond, Some("".into())),
            true,
        )
        .with_metadata(HashMap::from([
            (fields::PARQUET_FIELD_ID.to_owned(), "4".to_owned()),
            (fields::WYRD_SENSITIVE.to_owned(), "false".to_owned()),
        ]));
        let nested = Field::new(
            "nested",
            DataType::Struct(Fields::from(vec![
                Field::new("values", DataType::List(Arc::new(child)), false).with_metadata(
                    HashMap::from([
                        (fields::WYRD_SENSITIVE.to_owned(), "true".to_owned()),
                        (fields::PARQUET_FIELD_ID.to_owned(), "5".to_owned()),
                    ]),
                ),
            ])),
            true,
        )
        .with_metadata(HashMap::from([
            (fields::PARQUET_FIELD_ID.to_owned(), "1".to_owned()),
            (fields::WYRD_SENSITIVE.to_owned(), "false".to_owned()),
        ]));

        let schema = Fields::from(vec![nested, naive, zoned]);
        let bytes = canonical_physical_fingerprint_bytes(&schema).expect("the schema encodes");
        let hex = bytes.iter().fold(String::new(), |mut hex, byte| {
            use std::fmt::Write as _;
            let _ = write!(hex, "{byte:02x}");
            hex
        });
        assert_eq!(
            hex,
            "010000000300000001000000066e65737465640c01000000000200000010504152515545543a6669656c645f696400000001310000000e777972643a73656e7369746976650000000566616c736500000001000000050000000676616c7565730b00010000000200000010504152515545543a6669656c645f696400000001350000000e777972643a73656e73697469766500000004747275650000000100000003000000046974656d0701010000000200000010504152515545543a6669656c645f696400000001330000000e777972643a73656e73697469766500000004747275650000000000000002000000056e616976650a020000000000000200000010504152515545543a6669656c645f696400000001320000000e777972643a73656e7369746976650000000566616c73650000000000000004000000057a6f6e65640a02010000000001000000000200000010504152515545543a6669656c645f696400000001340000000e777972643a73656e7369746976650000000566616c736500000000"
        );
        assert_eq!(
            CanonicalPhysicalFingerprint::from_physical_fields(&schema)
                .expect("the schema fingerprints")
                .to_hex(),
            "e33647f94358ef330ddd8a07e3533b5e15d485530191c73c450c7d3d36cba269"
        );
    }

    /// The registry dispatches one canonical value validator per signal table.
    ///
    /// # Panics
    ///
    /// Panics when a canonical built-in carries no validator, when a validator
    /// admits a schema-valid but value-invalid batch, or when a pre-declared
    /// built-in claims a canonical validator it cannot own.
    #[test]
    fn builtin_registry_dispatches_canonical_value_validation() {
        use arrow::array::{Array, BinaryArray, Int64Array};

        let (spans, _) = crate::tables::traces::project_resource_spans(&span_fixture())
            .expect("the span fixture projects");
        let (logs, _) = crate::tables::logs::project_resource_logs(&log_fixture())
            .expect("the log fixture projects");
        let (points, _) = crate::tables::metrics::project_resource_metrics(&metric_fixture())
            .expect("the metric fixture projects");

        for (namespace, name, projected) in [
            ("traces", "spans", spans),
            ("logs", "records", logs),
            ("metrics", "points", points),
        ] {
            let definition = builtin_table(namespace, name).expect("canonical built-in");
            let validate = definition
                .canonical_validator
                .expect("a canonical signal table owns a value validator");
            let batch = crate::tables::signal::without_correlation_columns(&projected)
                .expect("the correlation columns split off cleanly");
            validate(&batch).expect("the table's own projection validates");

            let corrupted = RecordBatch::try_new(
                batch.schema(),
                batch
                    .schema()
                    .fields()
                    .iter()
                    .zip(batch.columns())
                    .map(|(field, column)| {
                        if field.name() == "attributes" {
                            Arc::new(BinaryArray::from_iter_values(std::iter::repeat_n(
                                [0xff_u8].as_slice(),
                                batch.num_rows(),
                            ))) as Arc<dyn Array>
                        } else {
                            Arc::clone(column)
                        }
                    })
                    .collect(),
            )
            .expect("the corrupted batch still assembles");
            assert!(
                validate(&corrupted).is_err(),
                "{namespace}.{name} rejects non-canonical payload bytes"
            );
        }

        // A summary point may not populate a numeric kind's column, which the
        // ledger schema alone cannot express.
        let (points, _) = crate::tables::metrics::project_resource_metrics(&metric_fixture())
            .expect("the metric fixture projects");
        let points = crate::tables::signal::without_correlation_columns(&points)
            .expect("the correlation columns split off cleanly");
        let kind_violation = RecordBatch::try_new(
            points.schema(),
            points
                .schema()
                .fields()
                .iter()
                .zip(points.columns())
                .map(|(field, column)| {
                    if field.name() == "int_value" {
                        Arc::new(Int64Array::from(vec![Some(1_i64); points.num_rows()]))
                            as Arc<dyn Array>
                    } else {
                        Arc::clone(column)
                    }
                })
                .collect(),
        )
        .expect("the kind-violating batch still assembles");
        let validate = builtin_table("metrics", "points")
            .expect("points definition")
            .canonical_validator
            .expect("the points table owns a value validator");
        assert!(
            validate(&kind_violation).is_err(),
            "a point may not populate a foreign kind's column"
        );

        assert!(
            builtin_table("eval", "runs")
                .expect("runs definition")
                .canonical_validator
                .is_none(),
            "a pre-declared built-in owns no canonical value validation"
        );
    }

    /// Assert every widened or re-spelled physical type drifts the identity.
    ///
    /// `Utf8`/`LargeUtf8`, `Binary`/`LargeBinary`, and a re-spelled UTC offset
    /// are the normalizations an Arrow-normalizing intermediary would silently
    /// apply, so each one is walked over the real physical schema.
    ///
    /// # Panics
    ///
    /// Panics when a widened field leaves the fingerprint unchanged, or when
    /// the schema exercises fewer than three drift dimensions.
    fn assert_widening_and_timezone_drift(
        physical: &Schema,
        baseline: CanonicalPhysicalFingerprint,
    ) {
        let widen = |data_type: &DataType| match data_type {
            DataType::Utf8 => Some(DataType::LargeUtf8),
            DataType::Binary => Some(DataType::LargeBinary),
            DataType::Timestamp(unit, Some(_)) => {
                Some(DataType::Timestamp(*unit, Some("+00:00".into())))
            }
            _ => None,
        };
        let mut widened = 0_usize;
        for (index, field) in physical.fields().iter().enumerate() {
            let Some(data_type) = widen(field.data_type()) else {
                continue;
            };
            widened += 1;
            let mut fields: Vec<Arc<Field>> = physical.fields().iter().map(Arc::clone).collect();
            fields[index] = Arc::new(
                Field::new(field.name(), data_type, field.is_nullable())
                    .with_metadata(field.metadata().clone()),
            );
            // A widened type may be refused outright rather than fingerprinted;
            // either way it never resolves back to the baseline identity.
            assert!(
                !CanonicalPhysicalFingerprint::from_physical_fields(&Fields::from(fields))
                    .is_ok_and(|drifted| drifted == baseline),
                "a widened or re-spelled {} drifts the canonical physical identity",
                field.name()
            );
        }
        assert!(
            widened >= 3,
            "the physical schema must exercise string, binary, and timestamp drift"
        );
    }

    /// The canonical physical identity rejects every normalized drift dimension.
    ///
    /// # Panics
    ///
    /// Panics when a widened type, a changed timezone spelling, a lost nested
    /// metadata entry, a changed field id, an unknown `wyrd_*` field, or a
    /// duplicated reserved field leaves the canonical physical fingerprint
    /// unchanged.
    #[test]
    fn resolved_identity_rejects_normalized_physical_drift() {
        let definition = builtin_table("traces", "spans").expect("spans definition");
        let identity = ResolvedSchemaIdentity::for_builtin(definition);
        let baseline = identity
            .canonical_physical_fingerprint
            .expect("a canonical built-in resolves a physical fingerprint");
        let physical = (definition.schema)();

        assert_widening_and_timezone_drift(&physical, baseline);

        let mutate_first = |predicate: fn(&Field) -> bool, mutate: &dyn Fn(&Field) -> Field| {
            let mut fields: Vec<Arc<Field>> = physical.fields().iter().map(Arc::clone).collect();
            let index = fields
                .iter()
                .position(|field| predicate(field))
                .expect("the physical schema carries the drift target");
            fields[index] = Arc::new(mutate(&fields[index]));
            Fields::from(fields)
        };

        let nested_metadata_dropped = mutate_first(
            |field| matches!(field.data_type(), DataType::List(_)),
            &|field| match field.data_type() {
                DataType::List(element) => Field::new(
                    field.name(),
                    DataType::List(Arc::new(Field::new(
                        element.name(),
                        element.data_type().clone(),
                        element.is_nullable(),
                    ))),
                    field.is_nullable(),
                )
                .with_metadata(field.metadata().clone()),
                other => Field::new(field.name(), other.clone(), field.is_nullable()),
            },
        );
        assert!(
            !CanonicalPhysicalFingerprint::from_physical_fields(&nested_metadata_dropped)
                .is_ok_and(|drifted| drifted == baseline),
            "a nested child that loses its metadata drifts the canonical physical identity"
        );

        let field_id_changed = mutate_first(|_| true, &|field| {
            let mut metadata = field.metadata().clone();
            metadata.insert(fields::PARQUET_FIELD_ID.to_owned(), "9999".to_owned());
            Field::new(field.name(), field.data_type().clone(), field.is_nullable())
                .with_metadata(metadata)
        });
        assert_ne!(
            CanonicalPhysicalFingerprint::from_physical_fields(&field_id_changed)
                .expect("a re-numbered field still fingerprints"),
            baseline,
            "a changed stable field id drifts the canonical physical identity"
        );

        let mut unknown: Vec<Arc<Field>> = physical.fields().iter().map(Arc::clone).collect();
        unknown.push(Arc::new(
            Field::new("wyrd_unknown", DataType::Utf8, true).with_metadata(
                std::collections::HashMap::from([
                    (fields::PARQUET_FIELD_ID.to_owned(), "9998".to_owned()),
                    (fields::WYRD_SENSITIVE.to_owned(), "false".to_owned()),
                ]),
            ),
        ));
        assert_ne!(
            CanonicalPhysicalFingerprint::from_physical_fields(&Fields::from(unknown))
                .expect("an extra field still fingerprints"),
            baseline,
            "an unknown wyrd_* field drifts the canonical physical identity"
        );

        let mut duplicated: Vec<Arc<Field>> = physical.fields().iter().map(Arc::clone).collect();
        let reserved = physical
            .fields()
            .iter()
            .find(|field| field.name() == WYRD_ROW_ORDINAL)
            .expect("the physical schema carries the reserved envelope")
            .clone();
        duplicated.push(reserved);
        assert_ne!(
            CanonicalPhysicalFingerprint::from_physical_fields(&Fields::from(duplicated))
                .expect("a duplicated field still fingerprints"),
            baseline,
            "a duplicated reserved field drifts the canonical physical identity"
        );
    }
}
