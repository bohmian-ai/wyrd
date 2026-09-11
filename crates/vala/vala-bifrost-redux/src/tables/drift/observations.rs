use arrow::datatypes::Field;

use crate::tables::fields::{float64, ts_us_utc, utf8};
use crate::tables::{
    CorrelationPolicy, DomainTable, PayloadClass, hourly_layout, sort_asc_nulls_first, sort_desc,
};
use wyrd_spec::vala::api::PhysicalLayoutWire;
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

pub struct ObservationsTable;

impl DomainTable for ObservationsTable {
    const NAMESPACE: &'static str = "drift";
    const NAME: &'static str = "observations";
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Standard;

    fn arrow_fields() -> Vec<Field> {
        vec![
            utf8("record_id", false),
            utf8("drift_ref", true),
            utf8("series", false),
            float64("num_value", true),
            utf8("str_value", true),
            utf8("session_id", true),
            ts_us_utc("created_at", false),
        ]
    }

    fn physical_layout() -> PhysicalLayoutWire {
        hourly_layout(
            vec![sort_desc(WYRD_EVENT_TIME), sort_asc_nulls_first("series")],
            &["series", "drift_ref"],
        )
    }
}
