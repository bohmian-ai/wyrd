use arrow::datatypes::{DataType, Field};

use crate::tables::fields::utf8;
use crate::tables::{
    CorrelationPolicy, DeclaredIndex, DomainTable, IndexKind, PayloadClass, SortKey,
};
use wyrd_spec::vala::system_columns::{DATA_TENANT_ID, WYRD_EVENT_TIME};

/// `vala.system.audit_log` — 16 audit content columns + 4 Bifrost system
/// columns, `CorrelationPolicy::None` (C-01). No universal correlation columns.
pub struct AuditLogTable;

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
            utf8("principal_id", false),
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

    fn sort_keys() -> Vec<SortKey> {
        vec![
            SortKey {
                column: WYRD_EVENT_TIME.into(),
                ascending: false,
                nulls_first: false,
            },
            SortKey {
                column: DATA_TENANT_ID.into(),
                ascending: true,
                nulls_first: false,
            },
            SortKey {
                column: "seq".into(),
                ascending: true,
                nulls_first: false,
            },
        ]
    }

    fn declared_indexes() -> Vec<DeclaredIndex> {
        vec![
            DeclaredIndex {
                name: "audit_log_seq_bloom".into(),
                columns: vec!["seq".into()],
                kind: IndexKind::BloomFilter,
            },
            DeclaredIndex {
                name: "audit_log_operation_bloom".into(),
                columns: vec!["operation".into()],
                kind: IndexKind::BloomFilter,
            },
        ]
    }
}
