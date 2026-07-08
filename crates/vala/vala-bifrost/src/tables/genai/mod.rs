use arrow::datatypes::SchemaRef;

use crate::tables::{
    CorrelationPolicy, DeclaredIndex, DomainTable, EntityBoundsMapping, IndexKind, PayloadClass,
    SortKey, generated,
};
use wyrd_spec::vala::system_columns::WYRD_EVENT_TIME;

pub struct MessagesTable;

impl DomainTable for MessagesTable {
    const NAMESPACE: &'static str = "genai";
    const NAME: &'static str = "messages";
    const SCHEMA_FINGERPRINT: [u8; 32] = generated::GENAI_MESSAGES_FINGERPRINT;
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] =
        &["input_messages", "output_messages", "system_instructions"];

    fn schema() -> SchemaRef {
        generated::genai_messages_schema()
    }

    fn sort_keys() -> Vec<SortKey> {
        vec![
            SortKey {
                column: WYRD_EVENT_TIME.into(),
                ascending: false,
                nulls_first: false,
            },
            SortKey {
                column: "conversation_id".into(),
                ascending: true,
                nulls_first: true,
            },
        ]
    }

    fn declared_indexes() -> Vec<DeclaredIndex> {
        vec![
            DeclaredIndex {
                name: "messages_conversation_id_lookup".into(),
                columns: vec!["conversation_id".into()],
                kind: IndexKind::BloomFilter,
            },
            DeclaredIndex {
                name: "messages_model_bloom".into(),
                columns: vec!["request_model".into()],
                kind: IndexKind::BloomFilter,
            },
        ]
    }

    fn entity_bounds_mapping() -> Option<EntityBoundsMapping> {
        None
    }
}

pub struct EmbeddingsTable;

impl DomainTable for EmbeddingsTable {
    const NAMESPACE: &'static str = "genai";
    const NAME: &'static str = "embeddings";
    const SCHEMA_FINGERPRINT: [u8; 32] = generated::GENAI_EMBEDDINGS_FINGERPRINT;
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] = &["retrieval_query_text"];

    fn schema() -> SchemaRef {
        generated::genai_embeddings_schema()
    }

    fn sort_keys() -> Vec<SortKey> {
        vec![
            SortKey {
                column: WYRD_EVENT_TIME.into(),
                ascending: false,
                nulls_first: false,
            },
            SortKey {
                column: "data_source_id".into(),
                ascending: true,
                nulls_first: true,
            },
        ]
    }

    fn declared_indexes() -> Vec<DeclaredIndex> {
        vec![DeclaredIndex {
            name: "embeddings_data_source_bloom".into(),
            columns: vec!["data_source_id".into()],
            kind: IndexKind::BloomFilter,
        }]
    }
}

pub struct ToolCallsTable;

impl DomainTable for ToolCallsTable {
    const NAMESPACE: &'static str = "genai";
    const NAME: &'static str = "tool_calls";
    const SCHEMA_FINGERPRINT: [u8; 32] = generated::GENAI_TOOL_CALLS_FINGERPRINT;
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] =
        &["tool_call_arguments", "tool_call_result"];

    fn schema() -> SchemaRef {
        generated::genai_tool_calls_schema()
    }

    fn sort_keys() -> Vec<SortKey> {
        vec![
            SortKey {
                column: WYRD_EVENT_TIME.into(),
                ascending: false,
                nulls_first: false,
            },
            SortKey {
                column: "conversation_id".into(),
                ascending: true,
                nulls_first: true,
            },
        ]
    }

    fn declared_indexes() -> Vec<DeclaredIndex> {
        vec![
            DeclaredIndex {
                name: "tool_calls_tool_name_bloom".into(),
                columns: vec!["tool_name".into()],
                kind: IndexKind::BloomFilter,
            },
            DeclaredIndex {
                name: "tool_calls_conversation_id_lookup".into(),
                columns: vec!["conversation_id".into()],
                kind: IndexKind::BloomFilter,
            },
        ]
    }
}
