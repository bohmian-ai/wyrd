use arrow::datatypes::Field;

use crate::tables::fields::{fixed_binary, int64, ts_us_utc, utf8};
use crate::tables::{
    CorrelationPolicy, DomainTable, PayloadClass, hourly_layout, sort_asc_nulls_first, sort_desc,
};
use wyrd_spec::vala::api::PhysicalLayoutWire;
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

pub struct ToolCallsTable;

impl DomainTable for ToolCallsTable {
    const NAMESPACE: &'static str = "genai";
    const NAME: &'static str = "tool_calls";
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] =
        &["tool_call_arguments", "tool_call_result"];

    fn arrow_fields() -> Vec<Field> {
        vec![
            fixed_binary("trace_id", 16, false),
            fixed_binary("span_id", 8, false),
            fixed_binary("parent_span_id", 8, true),
            utf8("conversation_id", true),
            ts_us_utc("start_time", false),
            ts_us_utc("end_time", false),
            int64("duration_ms", false),
            utf8("status", false),
            utf8("service_name", false),
            utf8("provider_name", false),
            utf8("operation_name", false),
            utf8("tool_name", true),
            utf8("tool_type", true),
            utf8("tool_call_id", true),
            utf8("tool_description", true),
            utf8("tool_call_arguments", true),
            utf8("tool_call_result", true),
            utf8("mcp_session_id", true),
            utf8("mcp_method_name", true),
            utf8("mcp_protocol_version", true),
            utf8("mcp_resource_uri", true),
            utf8("error_type", true),
            utf8("extra", true),
        ]
    }

    fn physical_layout() -> PhysicalLayoutWire {
        hourly_layout(
            vec![
                sort_desc(WYRD_EVENT_TIME),
                sort_asc_nulls_first("conversation_id"),
            ],
            &["tool_name", "conversation_id"],
        )
    }
}
