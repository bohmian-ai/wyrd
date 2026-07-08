use arrow::datatypes::SchemaRef;

use crate::tables::{
    CorrelationPolicy, DeclaredIndex, DomainTable, EntityBoundsMapping, IndexKind, PayloadClass,
    SortKey, generated,
};
use wyrd_spec::vala::system_columns::{DATA_TENANT_ID, WYRD_EVENT_TIME};

/// `vala.system.audit_log` — 16 audit content columns + 4 Bifrost system
/// columns, `CorrelationPolicy::None` (C-01). No universal correlation columns.
pub struct AuditLogTable;

impl DomainTable for AuditLogTable {
    const NAMESPACE: &'static str = "system";
    const NAME: &'static str = "audit_log";
    const SCHEMA_FINGERPRINT: [u8; 32] = generated::SYSTEM_AUDIT_LOG_FINGERPRINT;
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::None;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Standard;

    fn schema() -> SchemaRef {
        generated::system_audit_log_schema()
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

    fn entity_bounds_mapping() -> Option<EntityBoundsMapping> {
        None
    }
}
