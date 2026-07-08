use arrow::datatypes::SchemaRef;

use crate::tables::{
    CorrelationPolicy, DeclaredIndex, DomainTable, EntityBoundsMapping, IndexKind, PayloadClass,
    SortKey, generated,
};
use wyrd_spec::vala::system_columns::WYRD_EVENT_TIME;

pub struct PointsTable;

impl DomainTable for PointsTable {
    const NAMESPACE: &'static str = "metrics";
    const NAME: &'static str = "points";
    const SCHEMA_FINGERPRINT: [u8; 32] = generated::METRICS_POINTS_FINGERPRINT;
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Standard;

    fn schema() -> SchemaRef {
        generated::metrics_points_schema()
    }

    fn sort_keys() -> Vec<SortKey> {
        vec![
            SortKey { column: WYRD_EVENT_TIME.into(), ascending: false, nulls_first: false },
            SortKey { column: "metric_name".into(), ascending: true, nulls_first: false },
            SortKey { column: "service_name".into(), ascending: true, nulls_first: false },
        ]
    }

    fn declared_indexes() -> Vec<DeclaredIndex> {
        vec![
            DeclaredIndex {
                name: "metrics_name_bloom".into(),
                columns: vec!["metric_name".into()],
                kind: IndexKind::BloomFilter,
            },
        ]
    }

    fn entity_bounds_mapping() -> Option<EntityBoundsMapping> {
        None
    }
}
