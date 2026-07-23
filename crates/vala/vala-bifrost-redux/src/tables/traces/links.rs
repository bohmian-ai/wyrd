use arrow::datatypes::Field;

use crate::tables::fields::{fixed_binary, uint32, utf8, utf8_view};
use crate::tables::{
    CorrelationPolicy, DeclaredIndex, DomainTable, IndexKind, PayloadClass, SortKey,
};
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

pub struct LinksTable;

impl DomainTable for LinksTable {
    const NAMESPACE: &'static str = "traces";
    const NAME: &'static str = "links";
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] = &["attributes"];

    fn arrow_fields() -> Vec<Field> {
        vec![
            fixed_binary("trace_id", 16, false),
            fixed_binary("span_id", 8, false),
            fixed_binary("linked_trace_id", 16, false),
            fixed_binary("linked_span_id", 8, false),
            utf8("trace_state", true),
            uint32("flags", false),
            utf8_view("attributes", true),
            uint32("dropped_attributes_count", false),
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
        ]
    }

    fn declared_indexes() -> Vec<DeclaredIndex> {
        vec![
            DeclaredIndex {
                name: "links_trace_id_lookup".into(),
                columns: vec!["trace_id".into()],
                kind: IndexKind::BloomFilter,
            },
            DeclaredIndex {
                name: "links_linked_trace_id_lookup".into(),
                columns: vec!["linked_trace_id".into()],
                kind: IndexKind::BloomFilter,
            },
        ]
    }
}
