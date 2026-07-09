use arrow::datatypes::Field;

use crate::tables::fields::{
    bool_field, fixed_binary, float64, int64, ts_us_utc, uint32, uint64, utf8, utf8_view,
};
use crate::tables::{
    CorrelationPolicy, DeclaredIndex, DomainTable, IndexKind, PayloadClass, SortKey,
};
use wyrd_spec::vala::system_columns::WYRD_EVENT_TIME;

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
            uint64("duration_ms", false),
            utf8("status", false),
            utf8("service_name", false),
            utf8("provider_name", false),
            utf8("operation_name", false),
            utf8("request_model", false),
            utf8("response_model", true),
            utf8("conversation_id", true),
            utf8("response_id", true),
            utf8_view("response_finish_reasons", true),
            float64("response_time_to_first_chunk_seconds", true),
            float64("request_temperature", true),
            float64("request_top_p", true),
            uint32("request_top_k", true),
            uint32("request_max_tokens", true),
            float64("request_frequency_penalty", true),
            float64("request_presence_penalty", true),
            int64("request_seed", true),
            uint32("request_choice_count", true),
            utf8_view("request_stop_sequences", true),
            bool_field("request_stream", true),
            utf8_view("request_encoding_formats", true),
            uint32("usage_input_tokens", true),
            uint32("usage_output_tokens", true),
            uint32("usage_cache_creation_input_tokens", true),
            uint32("usage_cache_read_input_tokens", true),
            uint32("usage_reasoning_output_tokens", true),
            utf8("output_type", true),
            utf8_view("input_messages", true),
            utf8_view("output_messages", true),
            utf8_view("system_instructions", true),
            utf8("openai_api_type", true),
            utf8("openai_service_tier", true),
            utf8("error_type", true),
            utf8_view("eval_results", true),
            utf8_view("extra", true),
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
