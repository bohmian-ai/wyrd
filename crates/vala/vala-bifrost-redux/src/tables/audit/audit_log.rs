use arrow::datatypes::{DataType, Field};

use crate::tables::fields::utf8;
use crate::tables::{
    CorrelationPolicy, DomainTable, PayloadClass, hourly_layout, sort_asc, sort_desc,
};
use wyrd_spec::vala::api::PhysicalLayoutWire;
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

/// `vala.system.audit_log` — 17 audit content columns plus the managed
/// physical envelope.
///
/// Two content columns carry an `audit_` prefix. `card_ref` and `principal_id`
/// are reserved Redux correlation names: the ingest path rejects a payload that
/// supplies one and excludes it from a batch's logical identity, so an audit
/// event's own card reference and principal must be named out of that set to
/// travel as content. The universal correlation columns therefore stay free to
/// carry what they carry everywhere else — the principal and request that
/// published the row, not the principal the row is about.
pub struct AuditLogTable;

impl DomainTable for AuditLogTable {
    const NAMESPACE: &'static str = "system";
    const NAME: &'static str = "audit_log";
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
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
            utf8("auth_method", false),
            utf8("permission", false),
            utf8("decision", false),
            utf8("result", false),
            utf8("payload_summary", false),
            utf8("detail", true),
            Field::new("created_at_us", DataType::Int64, false),
        ]
    }

    fn physical_layout() -> PhysicalLayoutWire {
        hourly_layout(
            vec![sort_desc(WYRD_EVENT_TIME), sort_asc("seq")],
            &["seq", "operation"],
        )
    }
}
