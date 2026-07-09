use arrow::datatypes::Field;

use crate::tables::fields::{
    boolean, float64, int32, ts_us_utc, uint32, uint64, utf8, utf8_view,
};
use crate::tables::{
    CorrelationPolicy, DeclaredIndex, DomainTable, IndexKind, PayloadClass, SortKey,
};
use wyrd_spec::vala::system_columns::WYRD_EVENT_TIME;

pub struct PointsTable;

impl DomainTable for PointsTable {
    const NAMESPACE: &'static str = "metrics";
    const NAME: &'static str = "points";
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Standard;

    fn arrow_fields() -> Vec<Field> {
        vec![
            utf8("metric_name", false),
            ts_us_utc("time", false),
            ts_us_utc("start_time", true),
            utf8("description", true),
            utf8("unit", true),
            utf8("metric_type", false),
            utf8("temporality", true),
            boolean("is_monotonic", true),
            uint32("flags", true),
            float64("value", true),
            uint64("count", true),
            float64("sum", true),
            float64("min", true),
            float64("max", true),
            utf8_view("bucket_counts", true),
            utf8_view("explicit_bounds", true),
            int32("scale", true),
            uint64("zero_count", true),
            float64("zero_threshold", true),
            utf8_view("positive_buckets", true),
            utf8_view("negative_buckets", true),
            utf8_view("quantile_values", true),
            utf8_view("exemplars", true),
            utf8_view("attributes", true),
            utf8("service_name", false),
            utf8("scope_name", true),
            utf8("scope_version", true),
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
                column: "metric_name".into(),
                ascending: true,
                nulls_first: false,
            },
            SortKey {
                column: "service_name".into(),
                ascending: true,
                nulls_first: false,
            },
        ]
    }

    fn declared_indexes() -> Vec<DeclaredIndex> {
        vec![DeclaredIndex {
            name: "metrics_name_bloom".into(),
            columns: vec!["metric_name".into()],
            kind: IndexKind::BloomFilter,
        }]
    }
}
