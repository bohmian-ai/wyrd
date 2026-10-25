use arrow::datatypes::Field;

use wyrd_types::variant::variant_field;

use crate::catalog::TableRef;
use crate::namespaces::BifrostNamespace;
use crate::tables::fields::{ts_us_utc, utf8};
use crate::tables::{DomainTable, PayloadClass, daily_layout, sort_desc, with_shared_dimensions};
use wyrd_spec::vala::api::PhysicalLayoutWire;
use wyrd_spec::vala::managed_columns::{ACT_PRINCIPAL_ID, SUB_PRINCIPAL_ID, WYRD_REQUEST_ID};

/// `bifrost.system.audit_log` — 10 authorization-decision content columns, the
/// six shared dimensions, and the managed envelope.
///
/// The envelope's `sub_principal_id`/`act_principal_id` pair is the decision's
/// verified identity — or the unresolved sentinel — that the server carries on
/// each row, never the outbox's transport identity, and `wyrd_request_id` is
/// the request the decision was made for. The audited Card keeps its own
/// `audit_card_ref` column: an audit target is not an observation subject, so
/// `subject_card_uid` stays null unless the decision truthfully has one.
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
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Standard;

    /// The 10 decision-content columns in contract order, then the shared
    /// dimensions.
    ///
    /// `sub_principal_kind` classifies the envelope's decision subject. The
    /// remaining columns name the trace, operation, resource, optional Card,
    /// permission, outcome, Variant detail, credential, and decision instant.
    fn arrow_fields() -> Vec<Field> {
        with_shared_dimensions(vec![
            utf8("trace_id", true),
            utf8("operation", false),
            utf8("resource", false),
            utf8("audit_card_ref", true),
            utf8("sub_principal_kind", false),
            utf8("permission", false),
            utf8("outcome", false),
            variant_field(DETAIL, true),
            utf8(CREDENTIAL_ID, true),
            ts_us_utc(CREATED_AT, false),
        ])
    }

    /// Daily partitions on the decision instant `created_at`, sorted newest
    /// first, Bloom-pruned on the decision pair, `wyrd_request_id`, and
    /// `resource`.
    ///
    /// Partitioning by decision time rather than Scribe receipt keeps a
    /// retained decision in the day it was made; the Bloom columns are the
    /// complete accountability default and serve the "who did what to which
    /// resource in which request" audit lookups.
    fn physical_layout() -> PhysicalLayoutWire {
        daily_layout(
            CREATED_AT,
            vec![sort_desc(CREATED_AT)],
            &[
                SUB_PRINCIPAL_ID,
                ACT_PRINCIPAL_ID,
                WYRD_REQUEST_ID,
                "resource",
            ],
        )
    }
}
