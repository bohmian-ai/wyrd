use arrow::datatypes::Field;

use crate::tables::fields::{fixed_binary, int64, ts_us_utc, utf8};
use crate::tables::{
    CorrelationPolicy, DeclaredIndex, DomainTable, IndexKind, PayloadClass, SortKey,
};
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

pub struct RecordsTable;

impl DomainTable for RecordsTable {
    const NAMESPACE: &'static str = "logs";
    const NAME: &'static str = "records";
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] = &["body", "attributes"];

    fn arrow_fields() -> Vec<Field> {
        vec![
            ts_us_utc("time", true),
            ts_us_utc("observed_time", false),
            int64("severity_number", true),
            utf8("severity_text", true),
            utf8("event_name", true),
            utf8("body", true),
            fixed_binary("trace_id", 16, true),
            fixed_binary("span_id", 8, true),
            int64("trace_flags", true),
            utf8("attributes", true),
            int64("dropped_attributes_count", false),
            utf8("service_name", true),
            utf8("scope_name", true),
            utf8("scope_version", true),
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
}
