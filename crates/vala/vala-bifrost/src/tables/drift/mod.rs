use arrow::datatypes::SchemaRef;

use crate::tables::{
    CorrelationPolicy, DeclaredIndex, DomainTable, EntityBoundsMapping, IndexKind, PayloadClass,
    SortKey, generated,
};
use wyrd_spec::vala::system_columns::WYRD_EVENT_TIME;

pub struct FeaturesTable;

impl DomainTable for FeaturesTable {
    const NAMESPACE: &'static str = "drift";
    const NAME: &'static str = "features";
    const SCHEMA_FINGERPRINT: [u8; 32] = generated::DRIFT_FEATURES_FINGERPRINT;
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Standard;

    fn schema() -> SchemaRef {
        generated::drift_features_schema()
    }

    fn sort_keys() -> Vec<SortKey> {
        vec![
            SortKey { column: WYRD_EVENT_TIME.into(), ascending: false, nulls_first: false },
            SortKey { column: "drift_ref".into(), ascending: true, nulls_first: true },
        ]
    }

    fn declared_indexes() -> Vec<DeclaredIndex> {
        vec![
            DeclaredIndex {
                name: "drift_features_drift_ref_bloom".into(),
                columns: vec!["drift_ref".into()],
                kind: IndexKind::BloomFilter,
            },
        ]
    }

    fn entity_bounds_mapping() -> Option<EntityBoundsMapping> {
        None
    }
}

pub struct PredictionsTable;

impl DomainTable for PredictionsTable {
    const NAMESPACE: &'static str = "drift";
    const NAME: &'static str = "predictions";
    const SCHEMA_FINGERPRINT: [u8; 32] = generated::DRIFT_PREDICTIONS_FINGERPRINT;
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Standard;

    fn schema() -> SchemaRef {
        generated::drift_predictions_schema()
    }

    fn sort_keys() -> Vec<SortKey> {
        vec![
            SortKey { column: WYRD_EVENT_TIME.into(), ascending: false, nulls_first: false },
            SortKey { column: "drift_ref".into(), ascending: true, nulls_first: true },
        ]
    }

    fn declared_indexes() -> Vec<DeclaredIndex> {
        vec![
            DeclaredIndex {
                name: "drift_predictions_drift_ref_bloom".into(),
                columns: vec!["drift_ref".into()],
                kind: IndexKind::BloomFilter,
            },
        ]
    }
}

pub struct MonitorsTable;

impl DomainTable for MonitorsTable {
    const NAMESPACE: &'static str = "drift";
    const NAME: &'static str = "monitors";
    const SCHEMA_FINGERPRINT: [u8; 32] = generated::DRIFT_MONITORS_FINGERPRINT;
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Standard;

    fn schema() -> SchemaRef {
        generated::drift_monitors_schema()
    }

    fn sort_keys() -> Vec<SortKey> {
        vec![
            SortKey { column: WYRD_EVENT_TIME.into(), ascending: false, nulls_first: false },
            SortKey { column: "drift_ref".into(), ascending: true, nulls_first: true },
        ]
    }

    fn declared_indexes() -> Vec<DeclaredIndex> {
        vec![
            DeclaredIndex {
                name: "drift_monitors_drift_ref_bloom".into(),
                columns: vec!["drift_ref".into()],
                kind: IndexKind::BloomFilter,
            },
        ]
    }
}
