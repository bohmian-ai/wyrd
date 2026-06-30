//! OpenTelemetry wiring shell.

use wyrd_spec::trace::TraceContext;

/// Runtime telemetry install configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TelemetryConfig {
    /// Optional OTLP endpoint URL.
    pub endpoint: Option<String>,
}

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

/// Install the runtime telemetry shell.
///
/// No-op today. The signature reserves the server-boundary shape so concrete
/// SDK wiring can land without churning call sites.
pub fn install(_config: TelemetryConfig) {}

/// Attach an incoming trace context to the current runtime span.
///
/// No-op today; reserved for SDK integration.
pub fn attach(_context: &TraceContext) {}
