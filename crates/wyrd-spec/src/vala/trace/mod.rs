//! Trace primitive surface — OTel-compliant span, summary, and GenAI span
//! records. The shapes here are durable contracts; runtime (OTel SDK
//! singletons, OTLP receiver, exporters, propagation glue) lives in
//! `wyrd-telemetry` (PR4.1) and `vala-ingest` (PR4.2).
//!
//! Span attributes are stored as opaque `serde_json::Map<String,
//! serde_json::Value>` on every record. Downstream (`vala-data` PR4.1)
//! projects them to `Utf8View` Parquet columns; DataFusion 52.1+ JSON
//! shredding promotes hot keys to typed sidecar columns at compaction
//! time. The contract does not pre-extract tag or baggage attributes into
//! separate records — see the trace-primitive README's "Storage doctrine"
//! section for the rationale.
//!
//! Module layout (filled in by subsequent commits):
//!
//! - `attributes` — `wyrd.*` + `gen_ai.*` attribute key constants.
//! - `attribute_value` — typed projection of OTel `AnyValue`.
//! - `span_event`, `span_link` — OTel-canonical nested types.
//! - `resource`, `instrumentation_scope` — OTel-canonical producer typing.
//! - `span` — `SpanRecord` + `SpanKind` + `SpanStatus`.
//! - `trace_summary` — `TraceSummaryRecord`.
//! - `gen_ai_eval_result` — one evaluation result attached to a GenAI span.
//! - `gen_ai_span` — `GenAiSpanRecord` (full OTel GenAI semconv surface).
//!
//! Trace and span ids live in [`crate::vala::ids`] (commit 02 of the
//! trace-primitive plan promotes them from `vala::eval::ids`).

pub mod attribute_value;
pub mod attributes;
pub mod instrumentation_scope;
pub mod resource;
pub mod span_event;
pub mod span_link;

pub use attribute_value::AttributeValue;
pub use instrumentation_scope::InstrumentationScope;
pub use resource::Resource;
pub use span_event::SpanEvent;
pub use span_link::SpanLink;
// `attributes` is a constants module; consumers use `attributes::FOO`,
// not a glob re-export.
