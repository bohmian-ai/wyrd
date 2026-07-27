use arrow::datatypes::Field;

use crate::tables::fields::{float64, ts_us_utc, utf8};
use crate::tables::{
    CorrelationPolicy, DeclaredIndex, DomainTable, IndexKind, PayloadClass, SortKey,
};
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

    fn sort_keys() -> Vec<SortKey> {
        vec![
            SortKey {
                column: WYRD_EVENT_TIME.into(),
                ascending: false,
                nulls_first: false,
            },
            SortKey {
                column: "series".into(),
                ascending: true,
                nulls_first: true,
            },
        ]
    }

    fn declared_indexes() -> Vec<DeclaredIndex> {
        vec![
            DeclaredIndex {
                name: "drift_observations_series_bloom".into(),
                columns: vec!["series".into()],
                kind: IndexKind::BloomFilter,
            },
            DeclaredIndex {
                name: "drift_observations_drift_ref_bloom".into(),
                columns: vec!["drift_ref".into()],
                kind: IndexKind::BloomFilter,
            },
        ]
    }
}
