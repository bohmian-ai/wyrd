use arrow::datatypes::Field;

use crate::tables::fields::{fixed_binary, float64, int32, int64, ts_us_utc, utf8};
use crate::tables::{
    CorrelationPolicy, DeclaredIndex, DomainTable, IndexKind, PayloadClass, SortKey,
};
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

pub struct RunsTable;

impl DomainTable for RunsTable {
    const NAMESPACE: &'static str = "eval";
    const NAME: &'static str = "runs";
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Standard;

    fn arrow_fields() -> Vec<Field> {
        vec![
            utf8("record_id", false),
            utf8("session_id", true),
            utf8("eval_ref", true),
            utf8("context", false),
            fixed_binary("trace_id", 16, true),
            fixed_binary("span_id", 8, true),
            ts_us_utc("created_at", false),
            utf8("media", true),
            int32("total_tasks", true),
            int32("passed_tasks", true),
            int32("failed_tasks", true),
            float64("pass_rate", true),
            int64("duration_ms", true),
            utf8("execution_plan", true),
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
}
