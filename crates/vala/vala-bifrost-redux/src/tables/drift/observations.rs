use arrow::datatypes::Field;

use crate::tables::fields::{float64, ts_us_utc, utf8};
use crate::tables::{
    CorrelationPolicy, DomainTable, PayloadClass, daily_layout, sort_asc_nulls_first, sort_desc,
};
use wyrd_spec::vala::api::PhysicalLayoutWire;
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

/// `vala.drift.observations` — one tall row per observed feature.
///
/// A client emits one logical Drift observation covering a whole feature map;
/// the SDK projects it into one row per feature so windowed analysis can scan
/// a single series without decoding every sibling feature. Rows from one
/// observation share `record_id`. The observed subject travels as the managed
/// `card_uid`, never as a payload column: raw input is attributed to the Card
/// that produced it, so no Verifier or binding identity is written here and no
/// per-binding copy of a row exists.
pub struct ObservationsTable;

impl DomainTable for ObservationsTable {
    const NAMESPACE: &'static str = "drift";
    const NAME: &'static str = "observations";
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Standard;

    /// Authored columns in the exact order the SDK Drift projection must match at `start_bifrost`.
    fn arrow_fields() -> Vec<Field> {
        vec![
            utf8("record_id", false),
            utf8("series", false),
            float64("num_value", true),
            utf8("str_value", true),
            utf8("session_id", true),
            ts_us_utc("created_at", false),
        ]
    }

    /// Daily partitions sorted newest first then by series, Bloom-pruned on `series` for one-feature scans.
    fn physical_layout() -> PhysicalLayoutWire {
        daily_layout(
            vec![sort_desc(WYRD_EVENT_TIME), sort_asc_nulls_first("series")],
            &["series"],
        )
    }
}
