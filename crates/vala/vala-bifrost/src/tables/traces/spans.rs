use arrow::datatypes::Field;

use crate::tables::fields::{fixed_binary, ts_us_utc, uint32, uint64, utf8, utf8_view};
use crate::tables::{
    CorrelationPolicy, DeclaredIndex, DomainTable, EntityBoundsMapping, IndexKind, PayloadClass,
    SortKey,
};
use wyrd_spec::vala::system_columns::WYRD_EVENT_TIME;

pub struct SpansTable;

impl DomainTable for SpansTable {
    const NAMESPACE: &'static str = "traces";
    const NAME: &'static str = "spans";
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] = &["attributes"];

    fn arrow_fields() -> Vec<Field> {
        vec![
            fixed_binary("trace_id", 16, false),
            fixed_binary("span_id", 8, false),
            fixed_binary("parent_span_id", 8, true),
            uint32("flags", false),
            utf8("trace_state", true),
            utf8("name", false),
            utf8("kind", false),
            ts_us_utc("start_time", false),
            ts_us_utc("end_time", false),
            uint64("duration_ms", false),
            utf8("status", false),
            utf8_view("attributes", true),
            uint32("dropped_attributes_count", false),
            uint32("dropped_events_count", false),
            uint32("dropped_links_count", false),
            utf8("scope_name", true),
            utf8("scope_version", true),
            utf8("service_name", false),
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
                column: "data_tenant_id".into(),
                ascending: true,
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
                name: "spans_trace_id_lookup".into(),
                columns: vec!["trace_id".into()],
                kind: IndexKind::BloomFilter,
            },
            DeclaredIndex {
                name: "spans_service_bloom".into(),
                columns: vec!["service_name".into()],
                kind: IndexKind::BloomFilter,
            },
        ]
    }

    fn entity_bounds_mapping() -> Option<EntityBoundsMapping> {
        Some(EntityBoundsMapping {
            entity_kind: "trace".into(),
            entity_id_column: "trace_id".into(),
        })
    }
}
