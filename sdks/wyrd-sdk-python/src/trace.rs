//! Hands Python's active OpenTelemetry span to the Rust client calls a
//! binding makes, so every Wyrd request joins the caller's trace.
//!
//! Python's OpenTelemetry context never becomes Rust's current `tracing` span
//! across `PyO3`, so each binding entry reads it here, on the calling thread,
//! and scopes it over the Rust call with the shared client's caller trace.
//! A queued write's `bifrost.enqueue` PRODUCER span opens in Python's own
//! tracer, so it reaches the application's exporter with the caller's spans.

use std::future::Future;

use pyo3::prelude::*;
use pyo3::types::{IntoPyDict, PyDict};
use wyrd_client::transport::trace::{with_caller_trace, with_producer_span};
use wyrd_spec::trace::TraceContext;

/// Read Python's active OpenTelemetry span context as W3C trace context.
///
/// A missing `opentelemetry` package (the optional `otel` extra), no active
/// span, an invalid context, or any lookup failure supplies nothing, because
/// correlation is best-effort and must never fail the call it rides on.
pub(crate) fn active_trace_context(py: Python<'_>) -> Option<TraceContext> {
    let lookup = || -> PyResult<Option<TraceContext>> {
        span_context(
            &py.import("opentelemetry.trace")?
                .call_method0("get_current_span")?,
        )
    };
    lookup().ok().flatten()
}

/// Open and end Python's `bifrost.enqueue` PRODUCER span for one queued write
/// to `table`, under the active span, and return its context.
///
/// The span carries the OpenTelemetry messaging attributes for the Bifrost
/// destination. As for [`active_trace_context`], a missing `opentelemetry`
/// package, a non-recording span without a valid context, or any lookup
/// failure supplies nothing.
fn enqueue_span(py: Python<'_>, table: &str) -> Option<TraceContext> {
    let open = || -> PyResult<Option<TraceContext>> {
        let trace = py.import("opentelemetry.trace")?;
        let options = PyDict::new(py);
        options.set_item("kind", trace.getattr("SpanKind")?.getattr("PRODUCER")?)?;
        options.set_item(
            "attributes",
            [
                ("messaging.system", "bifrost"),
                ("messaging.operation.type", "create"),
                ("messaging.destination.name", table),
            ]
            .into_py_dict(py)?,
        )?;
        let span = trace
            .call_method1("get_tracer", ("wyrd.bifrost",))?
            .call_method("start_span", ("bifrost.enqueue",), Some(&options))?;
        let context = span_context(&span);
        span.call_method0("end")?;
        context
    };
    open().ok().flatten()
}

/// The valid W3C trace context of the Python OpenTelemetry `span`.
///
/// # Errors
///
/// Returns the Python error of any attribute lookup or extraction failure.
fn span_context(span: &Bound<'_, PyAny>) -> PyResult<Option<TraceContext>> {
    let context = span.call_method0("get_span_context")?;
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

/// Run the synchronous client call `call` so each queued write it makes
/// records a `bifrost.enqueue` PRODUCER span in Python's tracer.
pub(crate) fn traced<R>(call: impl FnOnce() -> R) -> R {
    with_producer_span(|table| Python::attach(|py| enqueue_span(py, table)), call)
}
