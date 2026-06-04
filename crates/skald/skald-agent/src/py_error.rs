//! Python-boundary error projection for `skald-agent`.

#![cfg(feature = "python")]

use pyo3::exceptions::PyRuntimeError;
use pyo3::types::PyAnyMethods;
use pyo3::{PyErr, Python};
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
            AgentPyError::Runtime(error) => agent_error_to_py(error),
            AgentPyError::Json(error) => PyRuntimeError::new_err(error.to_string()),
            AgentPyError::Py(error) => error,
        }
    }
}

fn agent_error_to_py(error: AgentError) -> PyErr {
    Python::attach(|py| {
        let message = error.to_string();
        let exception = match py
            .get_type::<wyrd_utils::py::WyrdError>()
            .call1((message.clone(),))
        {
            Ok(exception) => exception,
            Err(source) => return source,
        };
        if let Err(source) = exception.setattr("code", error.code()) {
            return source;
        }
        if let Err(source) = exception.setattr("message", message) {
            return source;
        }
        let details = match wyrd_utils::py::json_to_pyobject(py, &serde_json::json!({})) {
            Ok(details) => details,
            Err(source) => return source,
        };
        if let Err(source) = exception.setattr("details", details.bind(py)) {
            return source;
        }
        if let Err(source) = exception.setattr("status", error.status()) {
            return source;
        }
        if let Err(source) = exception.setattr("title", error.title()) {
            return source;
        }
        PyErr::from_value(exception)
    })
}
