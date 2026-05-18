//! OpenTelemetry wiring shell.

use wyrd_spec::trace::TraceContext;

/// Span context prepared for future server wiring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpanSeed {
    /// Optional incoming trace context.
    pub trace_context: Option<TraceContext>,
    /// Span name.
    pub name: String,
}

/// Prepare a span seed without attaching an SDK.
#[must_use]
pub fn prepare_span(name: impl Into<String>, trace_context: Option<TraceContext>) -> SpanSeed {
    SpanSeed {
        trace_context,
        name: name.into(),
    }
}
