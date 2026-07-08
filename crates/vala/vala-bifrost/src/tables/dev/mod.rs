use arrow::datatypes::SchemaRef;

use crate::tables::{
    CorrelationPolicy, DeclaredIndex, DomainTable, EntityBoundsMapping, IndexKind, PayloadClass,
    SortKey, generated,
};

pub struct AgentTracesTable;

impl DomainTable for AgentTracesTable {
    const NAMESPACE: &'static str = "dev";
    const NAME: &'static str = "agent_traces";
    const SCHEMA_FINGERPRINT: [u8; 32] = generated::DEV_AGENT_TRACES_FINGERPRINT;
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::CodeAxis;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] = &["messages", "tool_io"];

    fn schema() -> SchemaRef {
        generated::dev_agent_traces_schema()
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
