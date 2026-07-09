use arrow::datatypes::Field;

use crate::tables::fields::{fixed_binary, ts_us_utc, utf8, utf8_view};
use crate::tables::{
    CorrelationPolicy, DeclaredIndex, DomainTable, EntityBoundsMapping, IndexKind, PayloadClass,
    SortKey,
};

pub struct AgentTracesTable;

impl DomainTable for AgentTracesTable {
    const NAMESPACE: &'static str = "dev";
    const NAME: &'static str = "agent_traces";
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::CodeAxis;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] = &["messages", "tool_io"];

    fn arrow_fields() -> Vec<Field> {
        vec![
            utf8("dev_session_id", false),
            utf8("repo", false),
            utf8("commit_sha", true),
            utf8("branch", true),
            utf8("run_id", false),
            fixed_binary("trace_id", 16, true),
            fixed_binary("span_id", 8, true),
            utf8("role", false),
            utf8("model", false),
            utf8("provider", false),
            utf8_view("messages", false),
            utf8_view("tool_io", true),
            ts_us_utc("started_at", false),
            ts_us_utc("ended_at", false),
        ]
    }

    fn sort_keys() -> Vec<SortKey> {
        vec![
            SortKey {
                column: "started_at".into(),
                ascending: false,
                nulls_first: false,
            },
            SortKey {
                column: "dev_session_id".into(),
                ascending: true,
                nulls_first: false,
            },
        ]
    }

    fn declared_indexes() -> Vec<DeclaredIndex> {
        vec![
            DeclaredIndex {
                name: "agent_traces_dev_session_lookup".into(),
                columns: vec!["dev_session_id".into()],
                kind: IndexKind::BloomFilter,
            },
            DeclaredIndex {
                name: "agent_traces_repo_bloom".into(),
                columns: vec!["repo".into()],
                kind: IndexKind::BloomFilter,
            },
        ]
    }

    fn entity_bounds_mapping() -> Option<EntityBoundsMapping> {
        Some(EntityBoundsMapping {
            entity_kind: "agent_run".into(),
            entity_id_column: "dev_session_id".into(),
        })
    }
}
