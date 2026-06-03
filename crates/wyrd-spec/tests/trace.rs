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
