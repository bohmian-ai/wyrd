//! Canonical Redux built-in table definitions and pure table-layer transforms.

use arrow::datatypes::{Field, Schema, SchemaRef};
use sha2::{Digest, Sha256};
use thiserror::Error;
use wyrd_spec::vala::api::{
    NullOrderWire, PhysicalLayoutWire, SortDirectionWire, SortKeyWire, TimeGranularityWire,
    TimePartitionSpecWire,
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
/// differ. The catalog canonicalizes the result through
/// [`crate::catalog::PhysicalLayout::builtin`], which injects the tenant sort
/// prefix and the managed Bloom floor, so a built-in declares only what is
/// specific to it.
#[must_use]
pub fn hourly_layout(sort_keys: Vec<SortKeyWire>, bloom_columns: &[&str]) -> PhysicalLayoutWire {
    PhysicalLayoutWire {
        partition: TimePartitionSpecWire {
            column: WYRD_EVENT_TIME.to_owned(),
            granularity: TimeGranularityWire::Hour,
        },
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
    /// Engine-owned physical-layout declaration.
    ///
    /// This is the built-in's single statement of partition granularity, sort
    /// intent, and Bloom intent. The catalog canonicalizes it once and every
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

    /// Engine-owned physical-layout declaration.
    ///
    /// Defaults to hourly `wyrd_event_time` with no additional sort or Bloom
    /// intent, which canonicalizes to the tenant prefix plus
    /// `wyrd_event_time DESC NULLS LAST` and the managed Bloom floor.
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
                (definition.physical_layout)().partition.granularity,
                TimeGranularityWire::Hour
            );
        }
    }

    /// The one authoritative physical-layout contract for every registration
    /// path: all fourteen built-ins plus each dynamic declaration class.
    ///
    /// Built-ins and dynamic tables run the same canonicalization, so this
    /// proves in one place that the tenant sort prefix is always first, the
    /// managed Bloom floor is always present for schema-present columns, the
    /// declared order is otherwise preserved, omission resolves to the hourly
    /// default, an explicit empty list is not the same as omission, and every
    /// invalid class is refused without mutating anything.
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

    /// Proves every built-in table declares a layout that canonicalizes and is
    /// a fixed point of canonicalization.
    ///
    /// Each built-in must partition hourly, sort on the tenant prefix first,
    /// carry the managed Bloom floor for every column its schema actually has,
    /// and reference only columns that exist. Re-resolving the stored wire form
    /// must reproduce it byte for byte, which is what proves the prefix and
    /// floor are unioned once rather than accreted on every load.
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
                crate::catalog::layout::PhysicalLayout::builtin(&fqn, &schema, &declared)
                    .unwrap_or_else(|error| panic!("{fqn} declares a canonical layout: {error}"));

            assert_eq!(
                canonical.granularity(),
                crate::catalog::layout::TimeGranularity::Hour,
                "{fqn} partitions hourly"
            );
            assert_eq!(
                canonical.sort_keys()[0].column(),
                DATA_TENANT_ID,
                "{fqn} sorts on the tenant prefix first"
            );
            for column in crate::catalog::layout::MANAGED_BLOOM_FLOOR {
                if schema.field_with_name(column).is_ok() {
                    assert!(
                        canonical.bloom_columns().contains(&column.to_owned()),
                        "{fqn} Bloom union omits schema-present managed column {column}"
                    );
                }
            }
            for column in canonical.bloom_columns() {
                assert!(
                    schema.field_with_name(column).is_ok(),
                    "{fqn} Bloom column {column} is absent from its schema"
                );
            }
            for key in canonical.sort_keys() {
                assert!(
                    schema.field_with_name(key.column()).is_ok(),
                    "{fqn} sort column {} is absent from its schema",
                    key.column()
                );
            }
            // Canonicalization is a fixed point: the stored form of a canonical
            // layout re-resolves to itself rather than accreting the prefix or
            // floor a second time.
            let stored = canonical.to_wire();
            let round_tripped =
                crate::catalog::layout::PhysicalLayout::from_stored_wire(&fqn, &schema, &stored)
                    .unwrap_or_else(|error| panic!("{fqn} stored layout re-resolves: {error}"));
            assert_eq!(round_tripped.to_wire(), stored);
        }
    }

    /// Proves dynamic declarations resolve through the same canonicalization
    /// path as built-ins.
    ///
    /// Covers the three legal shapes: omission resolving to the hourly default
    /// with the managed floor, an explicit empty declaration meaning "tenant
    /// prefix only" rather than omission, and a custom declaration keeping its
    /// order behind the tenant prefix while unioning the managed floor.
    ///
    /// # Panics
    ///
    /// Panics when any of those three shapes resolves to the wrong layout.
    fn assert_dynamic_declarations_resolve(dynamic_schema: &Schema, table: &str) {
        // Dynamic declarations share the same resolution. The fixture schema
        // carries the full managed floor plus one user column.

        // Omission resolves to the hourly default with the managed floor.
        let omitted =
            crate::catalog::layout::PhysicalLayout::canonicalize(table, dynamic_schema, None)
                .expect("omitted layout resolves to the default");
        assert_eq!(
            omitted.granularity(),
            crate::catalog::layout::TimeGranularity::Hour
        );
        assert_eq!(
            omitted.bloom_columns(),
            crate::catalog::layout::MANAGED_BLOOM_FLOOR
                .iter()
                .map(|column| (*column).to_owned())
                .collect::<Vec<_>>()
                .as_slice()
        );

        // An explicit empty declaration is not omission: it means "tenant
        // prefix only" and "managed floor only", so the default event-time
        // sort key is absent.
        let explicit_empty = PhysicalLayoutWire {
            partition: TimePartitionSpecWire {
                column: WYRD_EVENT_TIME.to_owned(),
                granularity: TimeGranularityWire::Day,
            },
            sort_keys: Vec::new(),
            bloom_columns: Vec::new(),
        };
        let empty = crate::catalog::layout::PhysicalLayout::canonicalize(
            table,
            dynamic_schema,
            Some(&explicit_empty),
        )
        .expect("explicit empty layout resolves");
        assert_eq!(
            empty.granularity(),
            crate::catalog::layout::TimeGranularity::Day
        );
        assert_eq!(empty.sort_keys().len(), 1, "tenant prefix only");
        assert_ne!(empty.to_wire(), omitted.to_wire());

        // A custom declaration keeps its order behind the tenant prefix and
        // unions rather than replaces the managed Bloom floor.
        let custom = PhysicalLayoutWire {
            partition: TimePartitionSpecWire {
                column: WYRD_EVENT_TIME.to_owned(),
                granularity: TimeGranularityWire::Hour,
            },
            sort_keys: vec![sort_desc("customer")],
            bloom_columns: vec!["customer".to_owned()],
        };
        let resolved = crate::catalog::layout::PhysicalLayout::canonicalize(
            table,
            dynamic_schema,
            Some(&custom),
        )
        .expect("custom layout resolves");
        let sort_columns = resolved
            .sort_keys()
            .iter()
            .map(|key| key.column().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            sort_columns,
            vec![DATA_TENANT_ID.to_owned(), "customer".to_owned()]
        );
        assert!(resolved.bloom_columns().contains(&"customer".to_owned()));
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
                "unsupported partition column",
                PhysicalLayoutWire {
                    partition: TimePartitionSpecWire {
                        column: "customer".to_owned(),
                        granularity: TimeGranularityWire::Hour,
                    },
                    sort_keys: Vec::new(),
                    bloom_columns: Vec::new(),
                },
            ),
            (
                "unknown sort column",
                PhysicalLayoutWire {
                    partition: TimePartitionSpecWire {
                        column: WYRD_EVENT_TIME.to_owned(),
                        granularity: TimeGranularityWire::Hour,
                    },
                    sort_keys: vec![sort_asc("absent")],
                    bloom_columns: Vec::new(),
                },
            ),
            (
                "duplicate sort column",
                PhysicalLayoutWire {
                    partition: TimePartitionSpecWire {
                        column: WYRD_EVENT_TIME.to_owned(),
                        granularity: TimeGranularityWire::Hour,
                    },
                    sort_keys: vec![sort_asc("customer"), sort_desc("customer")],
                    bloom_columns: Vec::new(),
                },
            ),
            (
                "unknown bloom column",
                PhysicalLayoutWire {
                    partition: TimePartitionSpecWire {
                        column: WYRD_EVENT_TIME.to_owned(),
                        granularity: TimeGranularityWire::Hour,
                    },
                    sort_keys: Vec::new(),
                    bloom_columns: vec!["absent".to_owned()],
                },
            ),
            (
                "duplicate bloom column",
                PhysicalLayoutWire {
                    partition: TimePartitionSpecWire {
                        column: WYRD_EVENT_TIME.to_owned(),
                        granularity: TimeGranularityWire::Hour,
                    },
                    sort_keys: Vec::new(),
                    bloom_columns: vec!["customer".to_owned(), "customer".to_owned()],
                },
            ),
            (
                "caller sorts on a reserved managed column",
                PhysicalLayoutWire {
                    partition: TimePartitionSpecWire {
                        column: WYRD_EVENT_TIME.to_owned(),
                        granularity: TimeGranularityWire::Hour,
                    },
                    sort_keys: vec![sort_asc(WYRD_EVENT_TIME)],
                    bloom_columns: Vec::new(),
                },
            ),
        ];
        for (label, declared) in invalid_cases {
            let error = crate::catalog::layout::PhysicalLayout::canonicalize(
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
}
