use arrow::datatypes::Field;

use crate::tables::fields::{fixed_binary, ts_us_utc, utf8};
use crate::tables::{
    CorrelationPolicy, DomainTable, PayloadClass, hourly_layout, sort_asc, sort_desc,
};
use wyrd_spec::vala::api::PhysicalLayoutWire;

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
            utf8("messages", false),
            utf8("tool_io", true),
            ts_us_utc("started_at", false),
            ts_us_utc("ended_at", false),
        ]
    }

    fn physical_layout() -> PhysicalLayoutWire {
        hourly_layout(
            vec![sort_desc("started_at"), sort_asc("dev_session_id")],
            &["dev_session_id", "repo"],
        )
    }
}
