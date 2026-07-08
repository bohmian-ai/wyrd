use arrow::datatypes::SchemaRef;

use crate::tables::{
    CorrelationPolicy, DeclaredIndex, DomainTable, EntityBoundsMapping, IndexKind, PayloadClass,
    SortKey, generated,
};
use wyrd_spec::vala::system_columns::WYRD_EVENT_TIME;

pub struct RunsTable;

impl DomainTable for RunsTable {
    const NAMESPACE: &'static str = "eval";
    const NAME: &'static str = "runs";
    const SCHEMA_FINGERPRINT: [u8; 32] = generated::EVAL_RUNS_FINGERPRINT;
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Standard;

    fn schema() -> SchemaRef {
        generated::eval_runs_schema()
    }

    fn sort_keys() -> Vec<SortKey> {
        vec![
            SortKey {
                column: WYRD_EVENT_TIME.into(),
                ascending: false,
                nulls_first: false,
            },
            SortKey {
                column: "eval_ref".into(),
                ascending: true,
                nulls_first: true,
            },
            SortKey {
                column: "run_id".into(),
                ascending: true,
                nulls_first: true,
            },
        ]
    }

    fn declared_indexes() -> Vec<DeclaredIndex> {
        vec![
            DeclaredIndex {
                name: "eval_runs_run_id_lookup".into(),
                columns: vec!["run_id".into()],
                kind: IndexKind::BloomFilter,
            },
            DeclaredIndex {
                name: "eval_runs_eval_ref_bloom".into(),
                columns: vec!["eval_ref".into()],
                kind: IndexKind::BloomFilter,
            },
        ]
    }

    fn entity_bounds_mapping() -> Option<EntityBoundsMapping> {
        None
    }
}

pub struct AssertionsTable;

impl DomainTable for AssertionsTable {
    const NAMESPACE: &'static str = "eval";
    const NAME: &'static str = "assertions";
    const SCHEMA_FINGERPRINT: [u8; 32] = generated::EVAL_ASSERTIONS_FINGERPRINT;
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Standard;

    fn schema() -> SchemaRef {
        generated::eval_assertions_schema()
    }

    fn sort_keys() -> Vec<SortKey> {
        vec![
            SortKey {
                column: WYRD_EVENT_TIME.into(),
                ascending: false,
                nulls_first: false,
            },
            SortKey {
                column: "run_id".into(),
                ascending: true,
                nulls_first: true,
            },
            SortKey {
                column: "assertion_name".into(),
                ascending: true,
                nulls_first: false,
            },
        ]
    }

    fn declared_indexes() -> Vec<DeclaredIndex> {
        vec![DeclaredIndex {
            name: "assertions_run_id_lookup".into(),
            columns: vec!["run_id".into()],
            kind: IndexKind::BloomFilter,
        }]
    }
}
