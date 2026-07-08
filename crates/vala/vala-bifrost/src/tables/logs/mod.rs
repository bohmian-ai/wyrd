use arrow::datatypes::SchemaRef;

use crate::tables::{
    CorrelationPolicy, DeclaredIndex, DomainTable, EntityBoundsMapping, IndexKind, PayloadClass,
    SortKey, generated,
};
use wyrd_spec::vala::system_columns::WYRD_EVENT_TIME;

pub struct RecordsTable;

impl DomainTable for RecordsTable {
    const NAMESPACE: &'static str = "logs";
    const NAME: &'static str = "records";
    const SCHEMA_FINGERPRINT: [u8; 32] = generated::LOGS_RECORDS_FINGERPRINT;
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] = &["body", "attributes"];

    fn schema() -> SchemaRef {
        generated::logs_records_schema()
    }

    fn sort_keys() -> Vec<SortKey> {
        vec![
            SortKey {
                column: WYRD_EVENT_TIME.into(),
                ascending: false,
                nulls_first: false,
            },
            SortKey {
                column: "service_name".into(),
                ascending: true,
                nulls_first: true,
            },
        ]
    }

    fn declared_indexes() -> Vec<DeclaredIndex> {
        vec![
            DeclaredIndex {
                name: "logs_service_bloom".into(),
                columns: vec!["service_name".into()],
                kind: IndexKind::BloomFilter,
            },
            DeclaredIndex {
                name: "logs_severity_bloom".into(),
                columns: vec!["severity_number".into()],
                kind: IndexKind::BloomFilter,
            },
        ]
    }

    fn entity_bounds_mapping() -> Option<EntityBoundsMapping> {
        None
    }
}
