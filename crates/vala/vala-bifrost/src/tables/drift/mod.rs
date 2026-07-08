use arrow::datatypes::SchemaRef;

use crate::tables::{
    CorrelationPolicy, DeclaredIndex, DomainTable, EntityBoundsMapping, IndexKind, PayloadClass,
    SortKey, generated,
};
use wyrd_spec::vala::system_columns::WYRD_EVENT_TIME;

pub struct ObservationsTable;

impl DomainTable for ObservationsTable {
    const NAMESPACE: &'static str = "drift";
    const NAME: &'static str = "observations";
    const SCHEMA_FINGERPRINT: [u8; 32] = generated::DRIFT_OBSERVATIONS_FINGERPRINT;
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Standard;

    fn schema() -> SchemaRef {
        generated::drift_observations_schema()
    }

    fn sort_keys() -> Vec<SortKey> {
        vec![
            SortKey { column: WYRD_EVENT_TIME.into(), ascending: false, nulls_first: false },
            SortKey { column: "series".into(), ascending: true, nulls_first: true },
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

    fn entity_bounds_mapping() -> Option<EntityBoundsMapping> {
        None
    }
}
