//! Canonical Redux built-in table definitions and pure table-layer transforms.

use arrow::datatypes::{Field, Schema, SchemaRef};
use sha2::{Digest, Sha256};
use thiserror::Error;
use wyrd_spec::vala::managed_columns::{
    DATA_TENANT_ID, WYRD_BATCH_ID, WYRD_EVENT_TIME, WYRD_INGESTED_AT, WYRD_ROW_ORDINAL,
    is_reserved_managed_column,
};

use crate::catalog::PartitionTransform;

pub mod audit;
pub mod dev;
pub mod drift;
pub mod eval;
pub mod fields;
pub mod genai;
pub mod logs;
pub mod managed_columns;
pub mod metrics;
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

/// Sort key declared by a built-in table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortKey {
    /// Column name.
    pub column: String,
    /// Sort direction.
    pub ascending: bool,
    /// Null ordering.
    pub nulls_first: bool,
}

/// Index declaration for a built-in table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredIndex {
    /// Stable index name.
    pub name: String,
    /// Indexed columns.
    pub columns: Vec<String>,
    /// Index strategy.
    pub kind: IndexKind,
}

/// Supported built-in index strategies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexKind {
    /// Bloom filter index.
    BloomFilter,
    /// Z-order index.
    ZOrder,
}

/// Entity mapping used by time-bound acceleration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityBoundsMapping {
    /// Entity kind.
    pub entity_kind: String,
    /// Physical identifier column.
    pub entity_id_column: String,
}

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
    /// Canonical partition columns.
    pub partition_columns: fn() -> Vec<(String, PartitionTransform)>,
    /// Canonical sort keys.
    pub sort_keys: fn() -> Vec<SortKey>,
    /// Canonical index declarations.
    pub declared_indexes: fn() -> Vec<DeclaredIndex>,
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

    /// User-owned fields, excluding correlation and system fields.
    fn arrow_fields() -> Vec<Field>;

    /// Full physical schema.
    fn schema() -> SchemaRef {
        SchemaRef::new(Schema::new(managed_columns::ensure_managed_columns(
            Self::arrow_fields(),
            Self::CORRELATION_POLICY,
        )))
    }

    /// User-schema fingerprint.
    fn schema_fingerprint() -> [u8; 32] {
        fingerprint_fields(&Self::arrow_fields())
    }

    /// All physical tables use the event-day partition.
    fn partition_columns() -> Vec<(String, PartitionTransform)> {
        vec![(WYRD_EVENT_TIME.to_owned(), PartitionTransform::Day)]
    }

    /// Sort keys.
    fn sort_keys() -> Vec<SortKey>;
    /// Index declarations.
    fn declared_indexes() -> Vec<DeclaredIndex>;
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

fn fingerprint_fields(fields: &[Field]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for field in fields {
        hasher.update(field.name().as_bytes());
        hasher.update(b"\0");
        hasher.update(format!("{:?}", field.data_type()).as_bytes());
        hasher.update(b"\0");
    }
    hasher.finalize().into()
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
        partition_columns: T::partition_columns,
        sort_keys: T::sort_keys,
        declared_indexes: T::declared_indexes,
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
            assert_eq!((definition.partition_columns)().len(), 1);
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
}
