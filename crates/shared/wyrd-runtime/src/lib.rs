//! Runtime singleton and cross-cutting behavior shells.

#![deny(missing_docs)]

use std::sync::OnceLock;

use tokio::runtime::Runtime;

pub mod audit;
pub mod otel;
pub mod redaction;
pub mod request_id;

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// Borrow the process-wide Tokio runtime singleton.
///
/// Initialized on first access with Tokio's multi-thread scheduler. Runtime
/// construction failure is treated as process-start failure rather than a
/// recoverable application error.
///
/// # Panics
/// Panics if Tokio cannot create the runtime.
#[must_use]
pub fn runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        build_runtime().unwrap_or_else(|error| panic!("failed to build tokio runtime: {error}"))
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn build_runtime() -> std::io::Result<Runtime> {
    Runtime::new()
}

#[cfg(target_arch = "wasm32")]
fn build_runtime() -> std::io::Result<Runtime> {
    tokio::runtime::Builder::new_current_thread().build()
}
