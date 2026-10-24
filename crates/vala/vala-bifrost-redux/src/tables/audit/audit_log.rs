use arrow::datatypes::Field;

use wyrd_types::variant::variant_field;

use crate::catalog::TableRef;
use crate::namespaces::BifrostNamespace;
use crate::tables::fields::{ts_us_utc, utf8};
use crate::tables::{CorrelationPolicy, DomainTable, PayloadClass, daily_layout, sort_desc};
use wyrd_spec::vala::api::PhysicalLayoutWire;

/// `bifrost.system.audit_log` — 13 authorization-decision content columns plus the
/// managed physical envelope.
///
/// Audit carries its own subject/actor pair and Card identity, so the table
/// appends no universal correlation envelope. The pair is the decision's
/// verified identity built by the server, never the outbox's transport
/// identity; the Card column keeps its `audit_` prefix because `card_uid` is
/// caller correlation on the ingest path.
///
/// The decision instant is the `created_at` content column and the table's
/// time column, so a retained row partitions by when the boundary decided
/// rather than by when Scribe received it.
pub struct AuditLogTable;

impl AuditLogTable {
    /// Whether `table` is the one table the system owner may physically own.
    ///
    /// System-attributed security decisions (unverified peer and tail
    /// rejections) stage under `DataTenantId::SYSTEM_OWNER` and must reach
    /// retained history like any tenant's. Every other table keeps rejecting
    /// the system owner, and Gate refuses caller writes into the audit namespace,
    /// so only the server's internal audit write can use this exception.
    #[must_use]
    pub fn admits_system_owner(table: &TableRef) -> bool {
        table.namespace == BifrostNamespace::Audit && table.name == Self::NAME
    }
}

/// The content column holding the decision's structured detail as a Variant.
///
/// The value decodes to exactly the decision's canonical detail JSON.
pub const DETAIL: &str = "detail";

/// The content column naming the credential a decision was made against.
pub const CREDENTIAL_ID: &str = "credential_id";

/// The content column holding the decision instant, and the table's time
/// column.
pub const CREATED_AT: &str = "created_at";

impl DomainTable for AuditLogTable {
    const NAMESPACE: &'static str = "system";
    const NAME: &'static str = "audit_log";
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::None;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Standard;

    /// The 13 decision-content columns in contract order.
    ///
    /// `sub_principal_id`, `sub_principal_kind`, and `act_principal_id` are
    /// decision content: the server writes the decision's verified
    /// subject/actor pair into the frame, and under
    /// [`CorrelationPolicy::None`] the catalog appends no managed
    /// correlation columns, so Scribe never overwrites them with the outbox's
    /// transport identity. The remaining columns name the request, operation,
    /// resource, optional Card, permission, outcome, Variant detail,
    /// credential, and decision instant; the catalog appends only the managed
    /// system columns.
    fn arrow_fields() -> Vec<Field> {
        vec![
            utf8("request_id", false),
            utf8("trace_id", true),
            utf8("operation", false),
            utf8("resource", false),
            utf8("audit_card_ref", true),
            utf8("sub_principal_id", false),
            utf8("sub_principal_kind", false),
            utf8("act_principal_id", false),
            utf8("permission", false),
            utf8("outcome", false),
            variant_field(DETAIL, true),
            utf8(CREDENTIAL_ID, true),
            ts_us_utc(CREATED_AT, false),
        ]
    }

    /// Daily partitions on the decision instant `created_at`, sorted newest
    /// first, Bloom-pruned on `sub_principal_id`, `act_principal_id`,
    /// `resource`, and `operation`.
    ///
    /// Partitioning by decision time rather than Scribe receipt keeps a
    /// retained decision in the day it was made; the Bloom columns are the
    /// complete set and serve the "who did what to which resource" audit
    /// lookups.
    fn physical_layout() -> PhysicalLayoutWire {
        daily_layout(
            CREATED_AT,
            vec![sort_desc(CREATED_AT)],
            &[
                "sub_principal_id",
                "act_principal_id",
                "resource",
                "operation",
            ],
        )
    }
}
