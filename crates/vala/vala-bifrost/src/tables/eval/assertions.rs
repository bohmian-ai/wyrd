use arrow::datatypes::Field;

use crate::tables::fields::{fixed_binary, float64, ts_us_utc, utf8};
use crate::tables::{
    CorrelationPolicy, DeclaredIndex, DomainTable, IndexKind, PayloadClass, SortKey,
};
use wyrd_spec::vala::system_columns::WYRD_EVENT_TIME;

pub struct AssertionsTable;

impl DomainTable for AssertionsTable {
    const NAMESPACE: &'static str = "eval";
    const NAME: &'static str = "assertions";
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Standard;

    fn arrow_fields() -> Vec<Field> {
        vec![
            utf8("record_id", false),
            utf8("session_id", true),
            utf8("eval_ref", true),
            fixed_binary("trace_id", 16, true),
            fixed_binary("span_id", 8, true),
            ts_us_utc("created_at", false),
            utf8("assertion_name", false),
            utf8("score_label", true),
            float64("score_value", true),
            utf8("explanation", true),
            utf8("response_id", true),
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
