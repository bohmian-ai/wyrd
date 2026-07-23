use arrow::datatypes::Field;

use crate::tables::fields::{fixed_binary, int64, ts_us_utc, utf8, utf8_view};
use crate::tables::{
    CorrelationPolicy, DeclaredIndex, DomainTable, IndexKind, PayloadClass, SortKey,
};
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

pub struct MemoryTable;

impl DomainTable for MemoryTable {
    const NAMESPACE: &'static str = "genai";
    const NAME: &'static str = "memory";
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] =
        &["memory_query_text", "memory_records"];

    fn arrow_fields() -> Vec<Field> {
        vec![
            fixed_binary("trace_id", 16, false),
            fixed_binary("span_id", 8, false),
            fixed_binary("parent_span_id", 8, true),
            ts_us_utc("start_time", false),
            ts_us_utc("end_time", false),
            int64("duration_ms", false),
            utf8("status", false),
            utf8("service_name", false),
            utf8("provider_name", false),
            utf8("operation_name", false),
            utf8("memory_store_id", true),
            utf8("memory_record_id", true),
            int64("memory_record_count", true),
            utf8("memory_query_text", true),
            utf8_view("memory_records", true),
            utf8("error_type", true),
            utf8_view("extra", true),
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
                column: "memory_store_id".into(),
                ascending: true,
                nulls_first: true,
            },
        ]
    }

    fn declared_indexes() -> Vec<DeclaredIndex> {
        vec![DeclaredIndex {
            name: "memory_store_id_bloom".into(),
            columns: vec!["memory_store_id".into()],
            kind: IndexKind::BloomFilter,
        }]
    }
}
