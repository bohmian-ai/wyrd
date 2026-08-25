use arrow::datatypes::Field;

use crate::tables::fields::{fixed_binary, float64, int32, int64, ts_us_utc, utf8};
use crate::tables::{
    CorrelationPolicy, DomainTable, PayloadClass, hourly_layout, sort_asc_nulls_first, sort_desc,
};
use wyrd_spec::vala::api::PhysicalLayoutWire;
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

pub struct RunsTable;

impl DomainTable for RunsTable {
    const NAMESPACE: &'static str = "eval";
    const NAME: &'static str = "runs";
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Standard;

    fn arrow_fields() -> Vec<Field> {
        vec![
            utf8("record_id", false),
            utf8("session_id", true),
            utf8("eval_ref", true),
            utf8("context", false),
            fixed_binary("trace_id", 16, true),
            fixed_binary("span_id", 8, true),
            ts_us_utc("created_at", false),
            utf8("media", true),
            int32("total_tasks", true),
            int32("passed_tasks", true),
            int32("failed_tasks", true),
            float64("pass_rate", true),
            int64("duration_ms", true),
            utf8("execution_plan", true),
        ]
    }

    fn physical_layout() -> PhysicalLayoutWire {
        hourly_layout(
            vec![
                sort_desc(WYRD_EVENT_TIME),
                sort_asc_nulls_first("eval_ref"),
                sort_asc_nulls_first("run_id"),
            ],
            &["run_id", "eval_ref"],
        )
    }
}
