use arrow::datatypes::Field;

use crate::tables::fields::{fixed_binary, int64, ts_us_utc, utf8};
use crate::tables::{
    CorrelationPolicy, DomainTable, EntityBoundsMapping, PayloadClass, hourly_layout, sort_asc,
    sort_desc,
};
use wyrd_spec::vala::api::PhysicalLayoutWire;
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

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
            int64("flags", false),
            utf8("trace_state", true),
            utf8("name", false),
            utf8("kind", false),
            ts_us_utc("start_time", false),
            ts_us_utc("end_time", false),
            int64("duration_ms", false),
            utf8("status", false),
            utf8("attributes", true),
            int64("dropped_attributes_count", false),
            int64("dropped_events_count", false),
            int64("dropped_links_count", false),
            utf8("scope_name", true),
            utf8("scope_version", true),
            utf8("service_name", false),
        ]
    }

    fn physical_layout() -> PhysicalLayoutWire {
        hourly_layout(
            vec![sort_desc(WYRD_EVENT_TIME), sort_asc("trace_id")],
            &["trace_id", "span_id", "service_name"],
        )
    }

    fn entity_bounds_mapping() -> Option<EntityBoundsMapping> {
        Some(EntityBoundsMapping {
            entity_kind: "trace".into(),
            entity_id_column: "trace_id".into(),
        })
    }
}
