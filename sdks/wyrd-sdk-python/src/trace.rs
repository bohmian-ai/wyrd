//! Hands Python's active OpenTelemetry span to the Rust client calls a
//! binding makes, so every Wyrd request joins the caller's trace.
//!
//! Python's OpenTelemetry context never becomes Rust's current `tracing` span
//! across `PyO3`, so each binding entry reads it here, on the calling thread,
//! and scopes it over the Rust call with the shared client's caller trace.

use std::future::Future;

use pyo3::prelude::*;
use wyrd_client::transport::trace::{with_caller_trace, with_caller_trace_sync};
use wyrd_spec::trace::TraceContext;

/// Read Python's active OpenTelemetry span context as W3C trace context.
///
/// A missing `opentelemetry` package (the optional `otel` extra), no active
/// span, an invalid context, or any lookup failure supplies nothing, because
/// correlation is best-effort and must never fail the call it rides on.
pub(crate) fn active_trace_context(py: Python<'_>) -> Option<TraceContext> {
    let lookup = || -> PyResult<Option<TraceContext>> {
        let context = py
            .import("opentelemetry.trace")?
            .call_method0("get_current_span")?
            .call_method0("get_span_context")?;
        if !context.getattr("is_valid")?.extract::<bool>()? {
            return Ok(None);
        }
        let trace_id: u128 = context.getattr("trace_id")?.extract()?;
        let span_id: u64 = context.getattr("span_id")?.extract()?;
        Ok(Some(TraceContext {
            trace_id: format!("{trace_id:032x}"),
            span_id: format!("{span_id:016x}"),
            trace_flags: context.getattr("trace_flags")?.extract()?,
            trace_state: context
                .getattr("trace_state")?
                .call_method0("to_header")?
                .extract()?,
        }))
    };
    lookup().ok().flatten()
}

/// Drive `future` to completion on the shared runtime with the GIL released,
/// under the caller's active OpenTelemetry span.
///
/// The span is read before the GIL is released, so every Wyrd request the
/// future sends carries it as its W3C trace context.
pub(crate) fn block_on<F>(py: Python<'_>, future: F) -> F::Output
where
    F: Future + Send,
    F::Output: Send,
{
    let caller = active_trace_context(py);
    py.detach(|| wyrd_runtime::runtime().block_on(with_caller_trace(caller, future)))
}

/// Run the synchronous client call `call` under the caller's active
/// OpenTelemetry span, as for a queued write's PRODUCER span.
pub(crate) fn traced<R>(py: Python<'_>, call: impl FnOnce() -> R) -> R {
    with_caller_trace_sync(active_trace_context(py), call)
}
