use arrow::datatypes::Field;

use crate::tables::fields::{boolean, float64, int32, int64, ts_us_utc, uint32, utf8};
use crate::tables::{
    CorrelationPolicy, DeclaredIndex, DomainTable, IndexKind, PayloadClass, SortKey,
};
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

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
            int64("count", true),
            float64("sum", true),
            float64("min", true),
            float64("max", true),
            utf8("bucket_counts", true),
            utf8("explicit_bounds", true),
            int32("scale", true),
            int64("zero_count", true),
            float64("zero_threshold", true),
            utf8("positive_buckets", true),
            utf8("negative_buckets", true),
            utf8("quantile_values", true),
            utf8("exemplars", true),
            utf8("attributes", true),
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
