use arrow::datatypes::Field;

use crate::tables::fields::{fixed_binary, int64, ts_us_utc, utf8};
use crate::tables::{
    CorrelationPolicy, DeclaredIndex, DomainTable, IndexKind, PayloadClass, SortKey,
};
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

pub struct EventsTable;

impl DomainTable for EventsTable {
    const NAMESPACE: &'static str = "traces";
    const NAME: &'static str = "events";
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] = &["attributes"];

    fn arrow_fields() -> Vec<Field> {
        vec![
            fixed_binary("trace_id", 16, false),
            fixed_binary("span_id", 8, false),
            ts_us_utc("timestamp", false),
            utf8("name", false),
            utf8("attributes", true),
            int64("dropped_attributes_count", false),
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
                column: "trace_id".into(),
                ascending: true,
                nulls_first: false,
            },
            SortKey {
                column: "span_id".into(),
                ascending: true,
                nulls_first: false,
            },
        ]
    }

    fn declared_indexes() -> Vec<DeclaredIndex> {
        vec![DeclaredIndex {
            name: "events_trace_id_lookup".into(),
            columns: vec!["trace_id".into()],
            kind: IndexKind::BloomFilter,
        }]
    }
}
