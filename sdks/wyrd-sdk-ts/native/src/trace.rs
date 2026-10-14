//! Node's active OpenTelemetry context, handed to the Wyrd requests a native
//! call sends.
//!
//! Node's context lives in its `AsyncLocalStorage`, which Rust cannot read and
//! which never reaches the Tokio task an async native method runs on. The
//! TypeScript package therefore registers one [`set_trace_source`] callback at
//! load; each native entry calls it on the JavaScript thread, before any work
//! is spawned, and scopes the returned context over the Rust call with
//! `wyrd_client`'s caller-trace scope. Given a table, the callback instead
//! opens and ends a queued write's `bifrost.enqueue` PRODUCER span in Node's
//! tracer and returns that span's context.

use std::cell::RefCell;
use std::future::Future;

use napi::Env;
use napi::Result;
use napi::bindgen_prelude::{Function, FunctionRef, PromiseRaw, ToNapiValue};
use napi_derive::napi;
use wyrd_client::transport::trace::{with_caller_trace, with_producer_span};
use wyrd_spec::trace::TraceContext;

thread_local! {
    /// The registered callback reading Node's active span, or opening a
    /// queued write's PRODUCER span for a table; JavaScript-thread only.
    static SOURCE: RefCell<Option<FunctionRef<Option<String>, Option<NativeCallerTrace>>>> =
        const { RefCell::new(None) };
}

/// Node's active span as W3C `traceparent` and optional `tracestate`.
#[napi(object)]
pub struct NativeCallerTrace {
    /// The W3C `traceparent` header value.
    pub traceparent: String,
    /// The W3C `tracestate` header value, when the span carries one.
    pub tracestate: Option<String>,
}

/// Register the callback each native call reads Node's active span through,
/// and a queued write opens its PRODUCER span through when given the table;
/// a later registration replaces the earlier one.
///
/// # Errors
///
/// Returns a napi error when the callback reference cannot be created.
#[napi]
pub fn set_trace_source(source: Function<Option<String>, Option<NativeCallerTrace>>) -> Result<()> {
    let source = source.create_ref()?;
    SOURCE.with_borrow_mut(|slot| *slot = Some(source));
    Ok(())
}

/// The caller's active span read through the registered source; `None`
/// without a source, an active span, or a valid `traceparent`.
fn caller(env: Env) -> Option<TraceContext> {
    source(env, None)
}

/// Call the registered source on the JavaScript thread: the active span
/// without `table`, or the `bifrost.enqueue` PRODUCER span it opened and
/// ended for a queued write to `table`.
fn source(env: Env, table: Option<String>) -> Option<TraceContext> {
    let active = SOURCE
        .with_borrow(|source| source.as_ref()?.borrow_back(&env).ok()?.call(table).ok())??;
    TraceContext::parse_traceparent(&active.traceparent, active.tracestate.unwrap_or_default()).ok()
}

/// Spawn `future` as a JavaScript promise, scoped to the caller's active
/// span read now, on the JavaScript thread.
///
/// # Errors
///
/// Returns a napi error when the promise cannot be created; the future's own
/// error rejects the promise.
pub(crate) fn spawn<T, F>(env: &Env, future: F) -> Result<PromiseRaw<'_, T>>
where
    T: ToNapiValue + Send + 'static,
    F: Future<Output = Result<T>> + Send + 'static,
{
    env.spawn_future(with_caller_trace(caller(*env), future))
}

/// Run the synchronous `call` so each queued write it makes records a
/// `bifrost.enqueue` PRODUCER span in Node's tracer.
pub(crate) fn traced<R>(env: Env, call: impl FnOnce() -> R) -> R {
    with_producer_span(move |table| source(env, Some(table.to_owned())), call)
}
