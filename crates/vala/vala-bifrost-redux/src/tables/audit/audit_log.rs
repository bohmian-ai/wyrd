use arrow::datatypes::{DataType, Field};

use crate::tables::fields::utf8;
use crate::tables::{CorrelationPolicy, DomainTable, PayloadClass, sort_asc, sort_desc};
use wyrd_spec::vala::api::{PhysicalLayoutWire, TimeGranularityWire};
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

/// `vala.system.audit_log` — 13 authorization-decision content columns plus the
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
        ]
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
