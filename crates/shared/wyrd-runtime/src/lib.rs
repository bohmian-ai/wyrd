//! Runtime shell contracts for server-tier Wyrd crates.

#![deny(missing_docs)]

use std::sync::OnceLock;

use tokio::runtime::Runtime;

pub mod otel;
pub mod redaction;
pub mod request_id;

/// Return the shared Tokio runtime singleton.
///
/// # Errors
/// Returns an error if Tokio cannot create a runtime.
pub fn runtime() -> Result<&'static Runtime, RuntimeError> {
    static RUNTIME: OnceLock<Result<Runtime, RuntimeError>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .map_err(RuntimeError::from)
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// Runtime setup errors.
#[derive(Debug, Clone, thiserror::Error)]
pub enum RuntimeError {
    /// Tokio runtime build failed.
    #[error("failed to build tokio runtime: {0}")]
    Build(String),
}

impl From<std::io::Error> for RuntimeError {
    fn from(value: std::io::Error) -> Self {
        Self::Build(value.to_string())
    }
}
