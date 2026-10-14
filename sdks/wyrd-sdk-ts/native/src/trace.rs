//! Node's active OpenTelemetry context, handed to the Wyrd requests a native
//! call sends.
//!
//! Node's context lives in its `AsyncLocalStorage`, which Rust cannot read and
//! which never reaches the Tokio task an async native method runs on. The
//! TypeScript package therefore registers one [`set_trace_source`] callback at
//! load; each native entry calls it on the JavaScript thread, before any work
//! is spawned, and scopes the returned context over the Rust call with
//! `wyrd_client`'s caller-trace scope.

use std::cell::RefCell;
use std::future::Future;

use napi::Env;
use napi::Result;
use napi::bindgen_prelude::{Function, FunctionRef, PromiseRaw, ToNapiValue};
use napi_derive::napi;
use wyrd_client::transport::trace::{with_caller_trace, with_caller_trace_sync};
use wyrd_spec::trace::TraceContext;

thread_local! {
    /// The registered callback reading Node's active span; JavaScript-thread only.
    static SOURCE: RefCell<Option<FunctionRef<(), Option<NativeCallerTrace>>>> =
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

/// Register the callback each native call reads Node's active span through;
/// a later registration replaces the earlier one.
///
/// # Errors
///
/// Returns a napi error when the callback reference cannot be created.
#[napi]
pub fn set_trace_source(source: Function<(), Option<NativeCallerTrace>>) -> Result<()> {
    let source = source.create_ref()?;
    SOURCE.with_borrow_mut(|slot| *slot = Some(source));
    Ok(())
}

/// The caller's active span read through the registered source; `None`
/// without a source, an active span, or a valid `traceparent`.
fn caller(env: Env) -> Option<TraceContext> {
    let active =
        SOURCE.with_borrow(|source| source.as_ref()?.borrow_back(&env).ok()?.call(()).ok())??;
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

/// Run the synchronous `call` scoped to the caller's active span, such as a
/// queued write whose rows carry it as their producer.
pub(crate) fn traced<R>(env: Env, call: impl FnOnce() -> R) -> R {
    with_caller_trace_sync(caller(env), call)
}
