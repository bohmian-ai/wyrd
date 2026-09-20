use arrow::datatypes::{DataType, Field, Schema, SchemaRef};

use crate::catalog::TableRef;
use crate::namespaces::BifrostNamespace;
use crate::tables::fields::{PARQUET_FIELD_ID, utf8};
use crate::tables::managed_columns::ensure_managed_columns;
use crate::tables::{CorrelationPolicy, DomainTable, PayloadClass, sort_asc, sort_desc};
use wyrd_spec::vala::api::{PhysicalLayoutWire, TimeGranularityWire};
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

/// `vala.system.audit_log` — 14 authorization-decision content columns plus the
/// managed physical envelope.
///
/// Audit carries its own principal and Card identity, so the table appends no
/// universal correlation envelope. Those two content columns keep an `audit_`
/// prefix because `principal_id` is a reserved Redux correlation name that the
/// ingest path excludes from a batch's logical identity; the row's own subject
/// must be named out of that set to travel as content.
///
/// The Postgres decision timestamp travels as `wyrd_event_time` rather than a
/// content column, so a retained row partitions by when the boundary decided.
pub struct AuditLogTable;

impl AuditLogTable {
    /// Whether `table` is the one table the system owner may physically own.
    ///
    /// System-attributed security decisions (unverified peer and tail
    /// rejections) stage under `DataTenantId::SYSTEM_OWNER` and must reach
    /// retained history like any tenant's. Every other table keeps rejecting
    /// the system owner, and Gate refuses caller writes into the audit namespace,
    /// so only the server's internal publication can use this exception.
    #[must_use]
    pub fn admits_system_owner(table: &TableRef) -> bool {
        table.namespace == BifrostNamespace::Audit && table.name == Self::NAME
    }

    /// The content columns this table declared before it carried a credential.
    ///
    /// A deployment that already retains audit history registered exactly these
    /// thirteen fields, and the catalog recognizes their fingerprint — and only
    /// theirs — as the one registration it may evolve forward. Keeping the list
    /// here rather than as an opaque digest means the recognized predecessor is
    /// readable, and it is the only place the pre-credential shape survives.
    #[must_use]
    pub fn legacy_arrow_fields() -> Vec<Field> {
        let mut fields = Self::arrow_fields();
        fields.retain(|field| field.name() != CREDENTIAL_ID);
        fields
    }

    /// The user-schema fingerprint of the pre-credential registration.
    ///
    /// Derived from [`Self::legacy_arrow_fields`] through the same function the
    /// catalog fingerprints a declaration with, so the recognized predecessor
    /// cannot drift away from the shape it describes.
    #[must_use]
    pub fn legacy_schema_fingerprint() -> [u8; 32] {
        crate::tables::fingerprint_fields(&Self::legacy_arrow_fields())
    }
}

/// The content column an additive evolution appends to an existing table.
pub const CREDENTIAL_ID: &str = "credential_id";

/// Stable Iceberg id for [`CREDENTIAL_ID`].
///
/// The pre-credential physical schema occupied ids 1 through 18 by position, so
/// an `ADD COLUMN` on a table that already exists is assigned 19. A table
/// created fresh declares the same id explicitly, which is why this table —
/// alone among the pre-declared built-ins — carries explicit field ids: an
/// evolved table appends the new column physically last while a fresh one
/// declares it in its canonical position, and only an explicit id makes the two
/// tables the same table.
const CREDENTIAL_ID_FIELD_ID: i32 = 19;

impl DomainTable for AuditLogTable {
    const NAMESPACE: &'static str = "system";
    const NAME: &'static str = "audit_log";
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::None;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Standard;
    const PAST_EVENT_TIME_EXEMPT: bool = true;

    fn arrow_fields() -> Vec<Field> {
        vec![
            Field::new("seq", DataType::Int64, false),
            utf8("entry_hash", false),
            utf8("prev_hash", false),
            utf8("request_id", false),
            utf8("trace_id", true),
            utf8("operation", false),
            utf8("resource", false),
            utf8("audit_card_ref", true),
            utf8("audit_principal_id", false),
            utf8("principal_kind", false),
            utf8("permission", false),
            utf8("outcome", false),
            utf8("detail", true),
            // Appended, not inserted: the column arrived after deployments were
            // already retaining history, and an Iceberg `ADD COLUMN` can only
            // append. Declaring it last keeps one column order for a fresh and
            // an evolved table alike.
            utf8(CREDENTIAL_ID, true),
        ]
    }

    /// The physical schema, with every column's Iceberg id stated outright.
    ///
    /// The managed append is unchanged; the only addition is the explicit
    /// `PARQUET:field_id` on each field. Ids 1 through 18 are exactly the ones
    /// positional assignment produced before `credential_id` existed, so a
    /// table registered by an older build keeps every id it already has, and
    /// [`CREDENTIAL_ID_FIELD_ID`] is the id an `ADD COLUMN` on that table
    /// assigns. Sealed objects are tagged from this schema, so a reader
    /// resolves a column by id no matter which of the two physical column
    /// orders its table has.
    fn schema() -> SchemaRef {
        let fields = ensure_managed_columns(Self::arrow_fields(), Self::CORRELATION_POLICY);
        let mut next_legacy_id = 0;
        let fields: Vec<Field> = fields
            .into_iter()
            .map(|field| {
                let id = if field.name() == CREDENTIAL_ID {
                    CREDENTIAL_ID_FIELD_ID
                } else {
                    next_legacy_id += 1;
                    next_legacy_id
                };
                let mut metadata = field.metadata().clone();
                metadata.insert(PARQUET_FIELD_ID.to_owned(), id.to_string());
                field.with_metadata(metadata)
            })
            .collect();
        SchemaRef::new(Schema::new(fields))
    }

    fn physical_layout() -> PhysicalLayoutWire {
        PhysicalLayoutWire {
            partition_granularity: TimeGranularityWire::Day,
            sort_keys: vec![sort_desc(WYRD_EVENT_TIME), sort_asc("seq")],
            bloom_columns: vec![
                "audit_principal_id".to_owned(),
                "resource".to_owned(),
                "operation".to_owned(),
            ],
        }
    }
}
