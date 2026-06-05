//! Runtime configuration and per-run output for the bounded tool loop.

use serde::{Deserialize, Serialize};
use skald_spec::ProviderResponse;
use wyrd_spec::error::WyrdError;

use crate::conversation::Conversation;

/// Configuration for the bounded tool loop.
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(module = "wyrd.agent", name = "RunConfig", skip_from_py_object)
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunConfig {
    /// Maximum loop iterations before [`crate::AgentError::MaxIterations`].
    pub max_iterations: u32,
    /// Maximum concurrent tool calls per model iteration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_concurrency_cap: Option<usize>,
    /// Maximum recent session turns to recall at run start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_recent_limit: Option<usize>,
    /// Overall run timeout. `None` disables timeout enforcement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<std::time::Duration>,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            tool_concurrency_cap: Some(8),
            session_recent_limit: None,
            timeout: None,
        }
    }
}

#[cfg(feature = "python")]
#[pyo3::pymethods]
impl RunConfig {
    /// Build run configuration.
    ///
    /// Args:
    ///     max_iterations (int): Maximum model/tool loop iterations.
    ///     tool_concurrency_cap (int | None): Maximum concurrent tool calls.
    ///     session_recent_limit (int | None): Maximum recent session turns to recall.
    ///     timeout_ms (int | None): Overall run timeout in milliseconds.
    ///
    /// Returns:
    ///     RunConfig: New run configuration.
    #[new]
    #[pyo3(signature = (*, max_iterations=10, tool_concurrency_cap=Some(8), session_recent_limit=None, timeout_ms=None))]
    pub fn __new__(
        max_iterations: u32,
        tool_concurrency_cap: Option<usize>,
        session_recent_limit: Option<usize>,
        timeout_ms: Option<u64>,
    ) -> Self {
        Self {
            max_iterations,
            tool_concurrency_cap,
            session_recent_limit,
            timeout: timeout_ms.map(std::time::Duration::from_millis),
        }
    }

    /// Return maximum loop iterations.
    ///
    /// Returns:
    ///     int: Maximum loop iterations.
    #[getter]
    pub fn max_iterations(&self) -> u32 {
        self.max_iterations
    }

    /// Return maximum concurrent tool calls.
    ///
    /// Returns:
    ///     int | None: Maximum concurrent tool calls.
    #[getter]
    pub fn tool_concurrency_cap(&self) -> Option<usize> {
        self.tool_concurrency_cap
    }

    /// Return maximum recent session turns.
    ///
    /// Returns:
    ///     int | None: Maximum recent session turns.
    #[getter]
    pub fn session_recent_limit(&self) -> Option<usize> {
        self.session_recent_limit
    }

    /// Return timeout in milliseconds.
    ///
    /// Returns:
    ///     int | None: Timeout in milliseconds.
    #[getter]
    pub fn timeout_ms(&self) -> Option<u64> {
        self.timeout.and_then(|timeout| {
            let millis = timeout.as_millis();
            u64::try_from(millis).ok()
        })
    }

    /// Return a concise Python representation.
    ///
    /// Returns:
    ///     str: Python representation.
    pub fn __repr__(&self) -> String {
        format!(
            "RunConfig(max_iterations={}, tool_concurrency_cap={:?}, session_recent_limit={:?}, timeout_ms={:?})",
            self.max_iterations,
            self.tool_concurrency_cap,
            self.session_recent_limit,
            self.timeout_ms(),
        )
    }
}

/// Why an agent run terminated.
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(
        module = "wyrd.agent",
        name = "FinishReason",
        eq,
        eq_int,
        skip_from_py_object
    )
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    /// The model returned an assistant response with no tool calls.
    ModelStopped,
    /// The loop exhausted [`RunConfig::max_iterations`].
    MaxIterations,
    /// The run terminated because a callback returned `CallbackOutcome::Abort(...)` (Rust) or raised an exception (Python). The cause is available on `AgentRun.error`.
    CallbackAborted,
    /// Provider dispatch failed.
    ProviderError,
    /// Tool handling failed.
    ToolError,
    /// The configured timeout elapsed. Added in a later stage.
    Timeout,
}

/// One error recorded on a successful run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunError {
    /// One-based iteration where the error occurred.
    pub iteration: u32,
    /// Stable error code.
    pub code: String,
    /// Human-readable detail.
    pub detail: String,
}

/// Output of one successful `Agent::run`.
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(module = "wyrd.agent", name = "AgentRun", skip_from_py_object)
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRun {
    /// Convenience projection of the final assistant text.
    pub output: String,
    /// Final native provider response, when the run reached one.
    pub final_response: Option<ProviderResponse>,
    /// Number of loop iterations executed (1-based).
    pub iterations: u32,
    /// Why the loop terminated.
    pub finish_reason: FinishReason,
    /// Full in-run conversation accumulator.
    pub conversation: Conversation,
    /// Structured terminal error for abort-shaped completions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<WyrdError>,
    /// Errors recorded without aborting the run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<RunError>,
    /// Parsed JSON object for prompts that declared a structured output schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_output: Option<serde_json::Map<String, serde_json::Value>>,
    /// Typed model instance when the prompt declared an output class.
    ///
    /// Populated by `instantiate_parsed` in `py_run` after a successful
    /// structured-output run. None for text prompts, YAML prompts with no
    /// class bound at the Agent level, or when no class was passed.
    #[serde(skip)]
    #[cfg(feature = "python")]
    pub parsed: Option<pyo3::Py<pyo3::PyAny>>,
}

#[cfg(feature = "python")]
#[pyo3::pymethods]
impl AgentRun {
    /// Return why the run terminated.
    ///
    /// Returns:
    ///     FinishReason: Run termination reason.
    #[getter]
    pub fn finish_reason(&self) -> FinishReason {
        self.finish_reason
    }

    /// Return final assistant output text.
    ///
    /// Returns:
    ///     str: Final assistant output text.
    #[getter]
    pub fn output(&self) -> &str {
        &self.output
    }

    /// Return loop iteration count.
    ///
    /// Returns:
    ///     int: Loop iteration count.
    #[getter]
    pub fn iterations(&self) -> u32 {
        self.iterations
    }

    /// Return provider input token count.
    ///
    /// Returns:
    ///     int: Provider input token count.
    #[getter]
    pub fn tokens_in(&self) -> u64 {
        self.final_response
            .as_ref()
            .and_then(|response| response.adapter().usage())
            .map_or(0, |usage| usage.usage.input_tokens)
    }

    /// Return provider output token count.
    ///
    /// Returns:
    ///     int: Provider output token count.
    #[getter]
    pub fn tokens_out(&self) -> u64 {
        self.final_response
            .as_ref()
            .and_then(|response| response.adapter().usage())
            .map_or(0, |usage| usage.usage.output_tokens)
    }

    /// Return the run conversation.
    ///
    /// Returns:
    ///     dict: JSON-compatible conversation mapping.
    ///
    /// Raises:
    ///     WyrdError: When conversation conversion fails.
    #[getter]
    pub fn conversation(&self, py: pyo3::Python<'_>) -> pyo3::PyResult<pyo3::Py<pyo3::PyAny>> {
        let value = serde_json::to_value(&self.conversation)
            .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string()))?;
        wyrd_utils::py::json_to_pyobject(py, &value)
    }

    /// Return the structured terminal error.
    ///
    /// Returns:
    ///     WyrdError | None: Terminal error for abort-shaped runs.
    #[getter]
    pub fn error(&self, py: pyo3::Python<'_>) -> Option<pyo3::Py<pyo3::PyAny>> {
        self.error
            .clone()
            .map(|error| wyrd_utils::py::wyrd_error_to_py_object(py, error))
            .and_then(Result::ok)
    }

    /// Return parsed structured output, when the prompt declared an output schema.
    ///
    /// Returns:
    ///     dict[str, Any] | None: Parsed JSON object for structured-output prompts.
    #[getter]
    pub fn structured_output(
        &self,
        py: pyo3::Python<'_>,
    ) -> pyo3::PyResult<Option<pyo3::Py<pyo3::PyAny>>> {
        let Some(map) = self.structured_output.as_ref() else {
            return Ok(None);
        };
        let value = serde_json::Value::Object(map.clone());
        wyrd_utils::py::json_to_pyobject(py, &value).map(Some)
    }

    /// Return the typed model instance when the prompt declared an output class.
    ///
    /// Returns:
    ///     Any | None: Typed model instance, or None when no class was declared.
    #[getter]
    pub fn parsed(&self, py: pyo3::Python<'_>) -> Option<pyo3::Py<pyo3::PyAny>> {
        self.parsed.as_ref().map(|p| p.clone_ref(py))
    }

    /// Return a concise Python representation.
    ///
    /// Returns:
    ///     str: Python representation.
    pub fn __repr__(&self) -> String {
        format!(
            "AgentRun(finish_reason={:?}, output={:?}, iterations={})",
            self.finish_reason, self.output, self.iterations
        )
    }
}
