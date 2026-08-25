use arrow::datatypes::Field;

use crate::tables::fields::{fixed_binary, float64, ts_us_utc, utf8};
use crate::tables::{
    CorrelationPolicy, DomainTable, PayloadClass, hourly_layout, sort_asc, sort_asc_nulls_first,
    sort_desc,
};
use wyrd_spec::vala::api::PhysicalLayoutWire;
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

pub struct AssertionsTable;

impl DomainTable for AssertionsTable {
    const NAMESPACE: &'static str = "eval";
    const NAME: &'static str = "assertions";
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Standard;

    fn arrow_fields() -> Vec<Field> {
        vec![
            utf8("record_id", false),
            utf8("session_id", true),
            utf8("eval_ref", true),
            fixed_binary("trace_id", 16, true),
            fixed_binary("span_id", 8, true),
            ts_us_utc("created_at", false),
            utf8("assertion_name", false),
            utf8("score_label", true),
            float64("score_value", true),
            utf8("explanation", true),
            utf8("response_id", true),
        ]
    }

    fn physical_layout() -> PhysicalLayoutWire {
        hourly_layout(
            vec![
                sort_desc(WYRD_EVENT_TIME),
                sort_asc_nulls_first("run_id"),
                sort_asc("assertion_name"),
            ],
            &["run_id"],
        )
    }
}
