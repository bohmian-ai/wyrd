use arrow::datatypes::{DataType, Field};

use wyrd_types::variant::variant_field;

use crate::catalog::TableRef;
use crate::namespaces::BifrostNamespace;
use crate::tables::fields::{ts_us_utc, utf8};
use crate::tables::{
    CorrelationPolicy, DomainTable, PayloadClass, daily_layout, sort_asc, sort_desc,
};
use wyrd_spec::vala::api::PhysicalLayoutWire;

/// `vala.system.audit_log` — 15 authorization-decision content columns plus the
/// managed physical envelope.
///
/// Audit carries its own principal and Card identity, so the table appends no
/// universal correlation envelope. Those two content columns keep an `audit_`
/// prefix because `principal_id` is a reserved Redux correlation name that the
/// ingest path excludes from a batch's logical identity; the row's own subject
/// must be named out of that set to travel as content.
///
/// The Postgres decision timestamp is the `created_at` content column and the
/// table's time column, so a retained row partitions by when the boundary
/// decided rather than by when the publisher shipped it.
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
}

/// The content column holding the decision's structured detail as a Variant.
///
/// The value decodes to exactly the canonical JSON the entry hash covered.
pub const DETAIL: &str = "detail";

/// The content column naming the credential a decision was made against.
pub const CREDENTIAL_ID: &str = "credential_id";

/// The content column holding the decision instant `PostgreSQL` recorded, and
/// the table's time column.
pub const CREATED_AT: &str = "created_at";

impl DomainTable for AuditLogTable {
    const NAMESPACE: &'static str = "system";
    const NAME: &'static str = "audit_log";
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::None;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Standard;

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
            variant_field(DETAIL, true),
            utf8(CREDENTIAL_ID, true),
            ts_us_utc(CREATED_AT, false),
        ]
    }

    fn physical_layout() -> PhysicalLayoutWire {
        daily_layout(
            CREATED_AT,
            vec![sort_desc(CREATED_AT), sort_asc("seq")],
            &["audit_principal_id", "resource", "operation"],
        )
    }
}
