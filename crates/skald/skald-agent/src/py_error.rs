//! Python-boundary error projection for `skald-agent`.

#![cfg(feature = "python")]

use pyo3::PyErr;
use pyo3::exceptions::PyRuntimeError;
use thiserror::Error;
use wyrd_spec::error::WyrdError;

use crate::AgentError;

/// Result alias for Python-visible Agent methods.
pub type AgentPyResult<T> = Result<T, AgentPyError>;

/// Errors raised while converting between Python and the Rust Agent surface.
#[derive(Debug, Error)]
pub enum AgentPyError {
    /// Durable Wyrd card/spec error.
    #[error(transparent)]
    Spec(#[from] WyrdError),
    /// Runtime-only Skald agent error.
    #[error(transparent)]
    Runtime(#[from] AgentError),
    /// JSON serialization or conversion error.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// Python extraction or callback conversion error.
    #[error(transparent)]
    Py(#[from] PyErr),
}

impl From<AgentPyError> for PyErr {
    fn from(error: AgentPyError) -> Self {
        match error {
            AgentPyError::Spec(error) => wyrd_utils::py::wyrd_error_to_py_err(error),
            AgentPyError::Runtime(error) => {
                PyRuntimeError::new_err(format!("{}: {error}", error.code()))
            }
            AgentPyError::Json(error) => PyRuntimeError::new_err(error.to_string()),
            AgentPyError::Py(error) => error,
        }
    }
}
