use arrow::datatypes::Field;

use crate::tables::fields::{fixed_binary, int64, ts_us_utc, utf8};
use crate::tables::{
    CorrelationPolicy, DeclaredIndex, DomainTable, IndexKind, PayloadClass, SortKey,
};
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

pub struct EmbeddingsTable;

impl DomainTable for EmbeddingsTable {
    const NAMESPACE: &'static str = "genai";
    const NAME: &'static str = "embeddings";
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] = &["retrieval_query_text"];

    fn arrow_fields() -> Vec<Field> {
        vec![
            fixed_binary("trace_id", 16, false),
            fixed_binary("span_id", 8, false),
            ts_us_utc("start_time", false),
            ts_us_utc("end_time", false),
            int64("duration_ms", false),
            utf8("status", false),
            utf8("service_name", false),
            utf8("provider_name", false),
            utf8("operation_name", false),
            utf8("request_model", false),
            utf8("response_model", true),
            int64("embeddings_dimension_count", true),
            utf8("data_source_id", true),
            int64("usage_input_tokens", true),
            int64("usage_output_tokens", true),
            int64("retrieval_top_k", true),
            utf8("error_type", true),
            utf8("retrieval_query_text", true),
            utf8("extra", true),
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
