//! Mount for the trace test binary.

#[path = "trace/ids.rs"]
mod ids;

#[path = "trace/attributes.rs"]
mod attributes;

#[path = "trace/attribute_value.rs"]
mod attribute_value;

#[path = "trace/span_event.rs"]
mod span_event;

#[path = "trace/span_link.rs"]
mod span_link;

#[path = "trace/resource.rs"]
mod resource;

#[path = "trace/instrumentation_scope.rs"]
mod instrumentation_scope;

#[path = "trace/span_record.rs"]
mod span_record;

#[path = "trace/trace_summary_record.rs"]
mod trace_summary_record;

#[path = "trace/gen_ai_eval_result.rs"]
mod gen_ai_eval_result;

#[path = "trace/gen_ai_span_record.rs"]
mod gen_ai_span_record;

#[path = "trace/key_array_sync.rs"]
mod key_array_sync;

#[path = "trace/otel_proto_parity.rs"]
mod otel_proto_parity;

#[path = "trace/schema_drift.rs"]
mod schema_drift;
