use arrow::datatypes::Field;

use crate::tables::fields::{boolean, fixed_binary, float64, int64, ts_us_utc, utf8};
use crate::tables::{
    CorrelationPolicy, DeclaredIndex, DomainTable, IndexKind, PayloadClass, SortKey,
};
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

pub struct MessagesTable;

impl DomainTable for MessagesTable {
    const NAMESPACE: &'static str = "genai";
    const NAME: &'static str = "messages";
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] =
        &["input_messages", "output_messages", "system_instructions"];

    fn arrow_fields() -> Vec<Field> {
        vec![
            fixed_binary("trace_id", 16, false),
            fixed_binary("span_id", 8, false),
            fixed_binary("parent_span_id", 8, true),
            ts_us_utc("start_time", false),
            ts_us_utc("end_time", false),
            int64("duration_ms", false),
            utf8("status", false),
            utf8("service_name", false),
            utf8("provider_name", false),
            utf8("operation_name", false),
            utf8("request_model", false),
            utf8("response_model", true),
            utf8("conversation_id", true),
            utf8("response_id", true),
            utf8("response_finish_reasons", true),
            float64("response_time_to_first_chunk_seconds", true),
            float64("request_temperature", true),
            float64("request_top_p", true),
            int64("request_top_k", true),
            int64("request_max_tokens", true),
            float64("request_frequency_penalty", true),
            float64("request_presence_penalty", true),
            int64("request_seed", true),
            int64("request_choice_count", true),
            utf8("request_stop_sequences", true),
            boolean("request_stream", true),
            utf8("request_encoding_formats", true),
            int64("usage_input_tokens", true),
            int64("usage_output_tokens", true),
            int64("usage_cache_creation_input_tokens", true),
            int64("usage_cache_read_input_tokens", true),
            int64("usage_reasoning_output_tokens", true),
            utf8("output_type", true),
            utf8("request_reasoning_level", true),
            boolean("conversation_compacted", true),
            utf8("input_messages", true),
            utf8("output_messages", true),
            utf8("system_instructions", true),
            utf8("openai_api_type", true),
            utf8("openai_request_service_tier", true),
            utf8("openai_response_service_tier", true),
            utf8("openai_response_system_fingerprint", true),
            utf8("error_type", true),
            utf8("eval_results", true),
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
}
