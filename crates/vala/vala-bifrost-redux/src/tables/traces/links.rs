use arrow::datatypes::Field;

use crate::tables::fields::{fixed_binary, int64, utf8};
use crate::tables::{
    CorrelationPolicy, DomainTable, PayloadClass, hourly_layout, sort_asc, sort_desc,
};
use wyrd_spec::vala::api::PhysicalLayoutWire;
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
            int64("flags", false),
            utf8("attributes", true),
            int64("dropped_attributes_count", false),
        ]
    }

    fn physical_layout() -> PhysicalLayoutWire {
        hourly_layout(
            vec![sort_desc(WYRD_EVENT_TIME), sort_asc("trace_id")],
            &["trace_id", "span_id", "linked_trace_id"],
        )
    }
}
