//! Python boundary for the engine-direct Skald agent surface.

#![cfg(feature = "python")]

use std::sync::Arc;

use async_trait::async_trait;
use pyo3::IntoPyObjectExt;
use pyo3::exceptions::{PyRuntimeError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyList, PyModule};
use skald_prompt::{Prompt, PyProviderRequest};
use skald_runtime::ProviderRegistry;
use skald_spec::{ProviderRequest, ProviderResponse};

use crate::{
    AfterAgentFn, AfterModelFn, AfterToolFn, Agent, AgentContext, AgentError, AgentRun,
    BeforeAgentFn, BeforeModelFn, BeforeToolFn, CallbackOutcome, Role, RunConfig, SessionError,
    SessionId, SessionMemory, SessionTurn,
};

/// Engine-direct Python wrapper for `skald_agent::Agent`.
#[pyclass(module = "wyrd._wyrd.agent", name = "_AgentInner", skip_from_py_object)]
#[derive(Clone)]
pub struct PyAgentInner {
    inner: Agent,
    providers: Arc<ProviderRegistry>,
}

#[pymethods]
impl Role {
    /// Return the string value for this role.
    pub fn __str__(&self) -> &'static str {
        role_as_str(*self)
    }

    /// Return a concise Python representation.
    pub fn __repr__(&self) -> String {
        format!("Role.{}", role_variant_name(*self))
    }
}

#[pymethods]
impl SessionTurn {
    /// Build a session turn.
    #[new]
    #[pyo3(signature = (*, role, content, call_id=None))]
    pub fn __new__(
        py: Python<'_>,
        role: Py<PyAny>,
        content: String,
        call_id: Option<String>,
    ) -> PyResult<Self> {
        Ok(Self {
            role: role_from_py(role.bind(py))?,
            content,
            call_id,
        })
    }

    /// Role string for this turn.
    #[getter]
    pub fn role(&self) -> &'static str {
        role_as_str(self.role)
    }

    /// Text content for this turn.
    #[getter]
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Provider tool call id for tool-result turns.
    #[getter]
    pub fn call_id(&self) -> Option<&str> {
        self.call_id.as_deref()
    }

    /// Return this turn as a Python dictionary.
    pub fn model_dump(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        session_turn_to_py_dict(py, self)
    }

    /// Return this turn as JSON.
    pub fn model_dump_json(&self) -> PyResult<String> {
        serde_json::to_string(self).map_err(|error| PyRuntimeError::new_err(error.to_string()))
    }

    /// Validate a Python mapping or `SessionTurn` instance.
    #[staticmethod]
    pub fn model_validate(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        session_turn_from_py(value)
    }

    /// Validate JSON into a `SessionTurn`.
    #[staticmethod]
    pub fn model_validate_json(data: &str) -> PyResult<Self> {
        serde_json::from_str(data).map_err(|error| PyTypeError::new_err(error.to_string()))
    }

    /// Return a concise Python representation.
    pub fn __repr__(&self) -> String {
        format!(
            "SessionTurn(role={:?}, content={:?}, call_id={:?})",
            role_as_str(self.role),
            self.content,
            self.call_id
        )
    }
}

#[pymethods]
impl PyAgentInner {
    /// Build an engine-direct agent.
    #[new]
    #[pyo3(signature = (id, prompt, run_config=None, providers=None))]
    pub fn __new__(
        py: Python<'_>,
        id: String,
        prompt: Py<PyAny>,
        run_config: Option<Py<PyAny>>,
        providers: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let prompt = prompt_from_py(py, &prompt)?;
        let run_config = match run_config {
            Some(value) => run_config_from_py(py, value)?,
            None => RunConfig::default(),
        };
        let providers = match providers {
            Some(value) => provider_registry_from_py(py, value)?,
            None => skald_runtime::default_registry(),
        };
        let inner = Agent::new(id, Arc::new(prompt)).with_run_config(run_config);
        Ok(Self { inner, providers })
    }

    /// Run this engine-direct agent.
    pub fn run(
        &self,
        py: Python<'_>,
        input: &str,
        session_id: Option<String>,
    ) -> PyResult<Py<PyAny>> {
        let providers = self.providers.clone();
        let agent = self.inner.clone();
        let session_id = session_id.map(SessionId::new);
        let run = py.detach(|| {
            wyrd_runtime::runtime()
                .block_on(async move { agent.run(providers.as_ref(), session_id, input).await })
        });
        match run {
            Ok(run) => agent_run_to_py(py, run),
            Err(error) => Err(agent_error_to_py_err(py, error)),
        }
    }

    /// Runtime-local tool names attached to this engine-direct agent.
    #[getter]
    pub fn tool_names(&self) -> Vec<String> {
        self.inner.tool_names()
    }

    fn __repr__(&self) -> String {
        format!(
            "_AgentInner(id={:?}, tools={:?})",
            self.inner.id,
            self.inner.tool_names()
        )
    }
}

/// Convert a Skald agent run to the public Python dataclass.
///
/// # Errors
/// Returns Python import or object construction errors.
pub fn agent_run_to_py(py: Python<'_>, run: AgentRun) -> PyResult<Py<PyAny>> {
    let module = py.import("wyrd.run")?;
    let cls = module.getattr("AgentRun")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("finish_reason", finish_reason_str(run.finish_reason))?;
    kwargs.set_item("output", run.output)?;
    kwargs.set_item("iterations", run.iterations)?;
    let usage = run
        .final_response
        .as_ref()
        .and_then(|response| response.adapter().usage())
        .map(|usage| usage.usage);
    kwargs.set_item(
        "tokens_in",
        usage.as_ref().map_or(0, |usage| usage.input_tokens),
    )?;
    kwargs.set_item(
        "tokens_out",
        usage.as_ref().map_or(0, |usage| usage.output_tokens),
    )?;
    kwargs.set_item(
        "conversation",
        wyrd_utils_like_json_to_py(
            py,
            &serde_json::to_value(&run.conversation)
                .map_err(|error| PyRuntimeError::new_err(error.to_string()))?,
        )?,
    )?;
    Ok(cls.call((), Some(&kwargs))?.unbind())
}

/// Wrap a callable as a before-agent callback.
///
/// # Errors
/// Returns Python conversion failures.
pub fn wrap_before_agent(_py: Python<'_>, cb: Py<PyAny>) -> PyResult<BeforeAgentFn> {
    Ok(Arc::new(move |ctx, input| {
        invoke_callback(
            &cb,
            |py| {
                let args = (ctx_to_py(py, ctx)?, input);
                cb.bind(py).call1(args)
            },
            extract_string_replacement,
        )
    }))
}

/// Wrap a callable as an after-agent callback.
///
/// # Errors
/// Returns Python conversion failures.
pub fn wrap_after_agent(_py: Python<'_>, cb: Py<PyAny>) -> PyResult<AfterAgentFn> {
    Ok(Arc::new(move |ctx, run| {
        invoke_callback(
            &cb,
            |py| {
                let args = (
                    ctx_to_py(py, ctx)?,
                    wyrd_utils_like_json_to_py(py, &serde_json::to_value(run).unwrap_or_default())?,
                );
                cb.bind(py).call1(args)
            },
            extract_agent_run_replacement,
        )
    }))
}

/// Wrap a callable as a before-model callback.
///
/// # Errors
/// Returns Python conversion failures.
pub fn wrap_before_model(_py: Python<'_>, cb: Py<PyAny>) -> PyResult<BeforeModelFn> {
    Ok(Arc::new(move |ctx, request| {
        invoke_callback(
            &cb,
            |py| {
                let args = (
                    ctx_to_py(py, ctx)?,
                    wyrd_utils_like_json_to_py(
                        py,
                        &serde_json::to_value(request).unwrap_or_default(),
                    )?,
                );
                cb.bind(py).call1(args)
            },
            extract_provider_request_replacement,
        )
    }))
}

/// Wrap a callable as an after-model callback.
///
/// # Errors
/// Returns Python conversion failures.
pub fn wrap_after_model(_py: Python<'_>, cb: Py<PyAny>) -> PyResult<AfterModelFn> {
    Ok(Arc::new(move |ctx, response| {
        invoke_callback(
            &cb,
            |py| {
                let args = (
                    ctx_to_py(py, ctx)?,
                    wyrd_utils_like_json_to_py(
                        py,
                        &serde_json::to_value(response).unwrap_or_default(),
                    )?,
                );
                cb.bind(py).call1(args)
            },
            extract_provider_response_replacement,
        )
    }))
}

/// Wrap a callable as a before-tool callback.
///
/// # Errors
/// Returns Python conversion failures.
pub fn wrap_before_tool(_py: Python<'_>, cb: Py<PyAny>) -> PyResult<BeforeToolFn> {
    Ok(Arc::new(move |ctx, tool, args| {
        invoke_callback(
            &cb,
            |py| {
                let py_args = (
                    ctx_to_py(py, ctx)?,
                    tool.name(),
                    wyrd_utils_like_json_to_py(py, args)?,
                );
                cb.bind(py).call1(py_args)
            },
            extract_json_replacement,
        )
    }))
}

/// Wrap a callable as an after-tool callback.
///
/// # Errors
/// Returns Python conversion failures.
pub fn wrap_after_tool(_py: Python<'_>, cb: Py<PyAny>) -> PyResult<AfterToolFn> {
    Ok(Arc::new(move |ctx, tool, result| {
        invoke_callback(
            &cb,
            |py| {
                let value = match result {
                    Ok(value) => value.clone(),
                    Err(error) => {
                        serde_json::json!({"error": error.to_string(), "code": error.code()})
                    }
                };
                let py_args = (
                    ctx_to_py(py, ctx)?,
                    tool.name(),
                    wyrd_utils_like_json_to_py(py, &value)?,
                );
                cb.bind(py).call1(py_args)
            },
            extract_tool_result_replacement,
        )
    }))
}

/// Wrap a Python session object.
pub fn wrap_session(py: Python<'_>, session: Py<PyAny>) -> PyResult<Arc<dyn SessionMemory>> {
    validate_callable_method(py, &session, "recent")?;
    validate_callable_method(py, &session, "append")?;
    Ok(Arc::new(PySessionMemory { inner: session }))
}

struct PySessionMemory {
    inner: Py<PyAny>,
}

#[async_trait]
impl SessionMemory for PySessionMemory {
    async fn recent(
        &self,
        session_id: &SessionId,
        limit: usize,
    ) -> Result<Vec<SessionTurn>, SessionError> {
        Python::attach(|py| {
            let result = self
                .inner
                .bind(py)
                .call_method1("recent", (session_id.as_str(), limit))
                .map_err(|error| SessionError::RecentFailed(error.to_string()))?;
            session_turns_from_py(&result)
                .map_err(|error| SessionError::RecentFailed(error.to_string()))
        })
    }

    async fn append(&self, session_id: &SessionId, turn: SessionTurn) -> Result<(), SessionError> {
        Python::attach(|py| {
            let py_turn =
                Py::new(py, turn).map_err(|error| SessionError::AppendFailed(error.to_string()))?;
            self.inner
                .bind(py)
                .call_method1("append", (session_id.as_str(), py_turn))
                .map_err(|error| SessionError::AppendFailed(error.to_string()))?;
            Ok(())
        })
    }
}

/// Register engine-direct agent Python classes.
///
/// # Errors
/// Returns PyO3 registration errors.
pub fn python_register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyAgentInner>()?;
    module.add_class::<Role>()?;
    module.add_class::<SessionTurn>()?;
    Ok(())
}

fn prompt_from_py(py: Python<'_>, prompt: &Py<PyAny>) -> PyResult<Prompt> {
    let borrowed: PyRef<'_, Prompt> = prompt
        .bind(py)
        .extract()
        .map_err(|_| PyTypeError::new_err("prompt must be a wyrd.Prompt instance"))?;
    Ok(borrowed.clone())
}

fn provider_registry_from_py(
    py: Python<'_>,
    providers: Py<PyAny>,
) -> PyResult<Arc<ProviderRegistry>> {
    let borrowed: PyRef<'_, skald_runtime::python::PyProviderRegistryInner> = providers
        .bind(py)
        .extract()
        .map_err(|_| PyTypeError::new_err("providers must be a wyrd.ProviderRegistry instance"))?;
    Ok(borrowed.inner.clone())
}

fn run_config_from_py(py: Python<'_>, value: Py<PyAny>) -> PyResult<RunConfig> {
    if value.bind(py).is_none() {
        return Ok(RunConfig::default());
    }
    let json = wyrd_utils_like_py_to_json(value.bind(py))?;
    let mut config = RunConfig::default();
    if let Some(max_iterations) = json
        .get("max_iterations")
        .and_then(serde_json::Value::as_u64)
    {
        config.max_iterations = u32::try_from(max_iterations).unwrap_or(u32::MAX);
    }
    if let Some(limit) = json
        .get("session_recent_limit")
        .and_then(serde_json::Value::as_u64)
    {
        config.session_recent_limit = Some(usize::try_from(limit).unwrap_or(usize::MAX));
    }
    if let Some(cap) = json
        .get("tool_concurrency_cap")
        .and_then(serde_json::Value::as_u64)
    {
        config.tool_concurrency_cap = Some(usize::try_from(cap).unwrap_or(usize::MAX));
    }
    Ok(config)
}

fn invoke_callback<T>(
    cb: &Py<PyAny>,
    call: impl for<'py> FnOnce(Python<'py>) -> PyResult<Bound<'py, PyAny>>,
    replacement: fn(&Bound<'_, PyAny>) -> PyResult<T>,
) -> CallbackOutcome<T> {
    Python::attach(|py| {
        if let Err(error) = cb
            .bind(py)
            .is_callable()
            .then_some(())
            .ok_or_else(|| PyTypeError::new_err("agent callback must be callable"))
        {
            panic!("{error}");
        }
        let result = call(py).unwrap_or_else(|error| panic!("{error}"));
        callback_outcome(&result, replacement).unwrap_or_else(|error| panic!("{error}"))
    })
}

fn callback_outcome<T>(
    result: &Bound<'_, PyAny>,
    replacement: fn(&Bound<'_, PyAny>) -> PyResult<T>,
) -> PyResult<CallbackOutcome<T>> {
    if result.is_none() {
        return Ok(CallbackOutcome::Continue);
    }
    let type_name = result.get_type().name()?;
    if let Ok(value) = result.getattr("value") {
        if type_name == "_ReplaceWith" {
            return replacement(&value).map(CallbackOutcome::ReplaceWith);
        }
        let outcome = value.extract::<String>()?;
        return callback_outcome_from_str(&outcome);
    }
    let outcome = result.str()?.extract::<String>()?;
    callback_outcome_from_str(&outcome)
}

fn callback_outcome_from_str<T>(outcome: &str) -> PyResult<CallbackOutcome<T>> {
    match outcome {
        "skip" | "CallbackOutcome.Skip" => Ok(CallbackOutcome::Skip),
        "continue" | "CallbackOutcome.Continue" => Ok(CallbackOutcome::Continue),
        other => Err(PyTypeError::new_err(format!(
            "callback returned unsupported outcome {other:?}"
        ))),
    }
}

fn ctx_to_py(py: Python<'_>, ctx: &AgentContext) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    dict.set_item("agent_id", &ctx.agent_id)?;
    dict.set_item("session_id", &ctx.session_id)?;
    dict.set_item("iteration", ctx.iteration)?;
    dict.set_item(
        "conversation",
        wyrd_utils_like_json_to_py(
            py,
            &serde_json::to_value(ctx.conversation.as_ref())
                .map_err(|error| PyRuntimeError::new_err(error.to_string()))?,
        )?,
    )?;
    Ok(dict.into_any().unbind())
}

fn validate_callable_method(py: Python<'_>, obj: &Py<PyAny>, method: &str) -> PyResult<()> {
    let method_obj = obj.bind(py).getattr(method).map_err(|_| {
        PyTypeError::new_err(format!("session object must define callable {method}()"))
    })?;
    if method_obj.is_callable() {
        Ok(())
    } else {
        Err(PyTypeError::new_err(format!(
            "session object attribute {method:?} must be callable"
        )))
    }
}

fn role_as_str(role: Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    }
}

fn role_variant_name(role: Role) -> &'static str {
    match role {
        Role::System => "System",
        Role::User => "User",
        Role::Assistant => "Assistant",
        Role::Tool => "Tool",
    }
}

fn role_from_py(value: &Bound<'_, PyAny>) -> PyResult<Role> {
    if let Ok(role) = value.extract::<Role>() {
        return Ok(role);
    }
    match value.extract::<String>()?.as_str() {
        "system" | "System" => Ok(Role::System),
        "user" | "User" => Ok(Role::User),
        "assistant" | "Assistant" => Ok(Role::Assistant),
        "tool" | "Tool" => Ok(Role::Tool),
        other => Err(PyTypeError::new_err(format!(
            "unsupported session role {other:?}"
        ))),
    }
}

fn session_turn_to_py_dict(py: Python<'_>, turn: &SessionTurn) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    dict.set_item("role", role_as_str(turn.role))?;
    dict.set_item("content", &turn.content)?;
    dict.set_item("call_id", &turn.call_id)?;
    Ok(dict.into_any().unbind())
}

fn session_turn_from_py(value: &Bound<'_, PyAny>) -> PyResult<SessionTurn> {
    if let Ok(turn) = value.extract::<PyRef<'_, SessionTurn>>() {
        return Ok(turn.clone());
    }
    let json = wyrd_utils_like_py_to_json(value)?;
    serde_json::from_value(json).map_err(|error| PyTypeError::new_err(error.to_string()))
}

fn session_turns_from_py(value: &Bound<'_, PyAny>) -> PyResult<Vec<SessionTurn>> {
    if let Ok(list) = value.cast::<PyList>() {
        let mut turns = Vec::with_capacity(list.len());
        for item in list.iter() {
            turns.push(session_turn_from_py(&item)?);
        }
        return Ok(turns);
    }
    let json = wyrd_utils_like_py_to_json(value)?;
    serde_json::from_value(json).map_err(|error| PyTypeError::new_err(error.to_string()))
}

fn extract_string_replacement(value: &Bound<'_, PyAny>) -> PyResult<String> {
    value.extract()
}

fn extract_provider_request_replacement(value: &Bound<'_, PyAny>) -> PyResult<ProviderRequest> {
    if let Ok(request) = value.extract::<PyRef<'_, PyProviderRequest>>() {
        return Ok(request.native().clone());
    }
    serde_json::from_value(wyrd_utils_like_py_to_json(value)?)
        .map_err(|error| PyTypeError::new_err(error.to_string()))
}

fn extract_provider_response_replacement(value: &Bound<'_, PyAny>) -> PyResult<ProviderResponse> {
    serde_json::from_value(wyrd_utils_like_py_to_json(value)?)
        .map_err(|error| PyTypeError::new_err(error.to_string()))
}

fn extract_json_replacement(value: &Bound<'_, PyAny>) -> PyResult<serde_json::Value> {
    wyrd_utils_like_py_to_json(value)
}

fn extract_tool_result_replacement(
    value: &Bound<'_, PyAny>,
) -> PyResult<Result<serde_json::Value, skald_tool::ToolError>> {
    Ok(Ok(wyrd_utils_like_py_to_json(value)?))
}

fn extract_agent_run_replacement(value: &Bound<'_, PyAny>) -> PyResult<AgentRun> {
    serde_json::from_value(wyrd_utils_like_py_to_json(value)?)
        .map_err(|error| PyTypeError::new_err(error.to_string()))
}

fn finish_reason_str(reason: crate::FinishReason) -> &'static str {
    match reason {
        crate::FinishReason::ModelStopped => "model_stopped",
        crate::FinishReason::MaxIterations => "max_iterations",
        crate::FinishReason::CallbackSkipped => "callback_skipped",
        crate::FinishReason::ProviderError => "provider_error",
        crate::FinishReason::ToolError => "tool_error",
        crate::FinishReason::Timeout => "timeout",
    }
}

fn agent_error_to_py_err(py: Python<'_>, error: AgentError) -> PyErr {
    let detail = error.to_string();
    match py
        .get_type::<pyo3::exceptions::PyRuntimeError>()
        .call1((detail,))
    {
        Ok(exception) => {
            let _ = exception.setattr("code", error.code());
            let _ = exception.setattr("status", error.status());
            let _ = exception.setattr("http_status", error.status());
            PyErr::from_value(exception)
        }
        Err(source) => PyRuntimeError::new_err(source.to_string()),
    }
}

fn wyrd_utils_like_json_to_py(py: Python<'_>, value: &serde_json::Value) -> PyResult<Py<PyAny>> {
    match value {
        serde_json::Value::Null => Ok(py.None()),
        serde_json::Value::Bool(value) => value.into_py_any(py),
        serde_json::Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                value.into_py_any(py)
            } else if let Some(value) = value.as_u64() {
                value.into_py_any(py)
            } else if let Some(value) = value.as_f64() {
                value.into_py_any(py)
            } else {
                Err(PyRuntimeError::new_err("invalid JSON number"))
            }
        }
        serde_json::Value::String(value) => value.into_py_any(py),
        serde_json::Value::Array(values) => {
            let list = pyo3::types::PyList::empty(py);
            for value in values {
                list.append(wyrd_utils_like_json_to_py(py, value)?)?;
            }
            Ok(list.into_any().unbind())
        }
        serde_json::Value::Object(values) => {
            let dict = PyDict::new(py);
            for (key, value) in values {
                dict.set_item(key, wyrd_utils_like_json_to_py(py, value)?)?;
            }
            Ok(dict.into_any().unbind())
        }
    }
}

fn wyrd_utils_like_py_to_json(obj: &Bound<'_, PyAny>) -> PyResult<serde_json::Value> {
    if obj.is_none() {
        return Ok(serde_json::Value::Null);
    }
    if let Ok(value) = obj.extract::<bool>() {
        return Ok(serde_json::Value::Bool(value));
    }
    if let Ok(value) = obj.extract::<i64>() {
        return Ok(serde_json::Value::Number(value.into()));
    }
    if let Ok(value) = obj.extract::<u64>() {
        return Ok(serde_json::Value::Number(value.into()));
    }
    if let Ok(value) = obj.extract::<f64>() {
        return serde_json::Number::from_f64(value)
            .map(serde_json::Value::Number)
            .ok_or_else(|| PyTypeError::new_err("cannot convert non-finite float to JSON"));
    }
    if let Ok(value) = obj.extract::<String>() {
        return Ok(serde_json::Value::String(value));
    }
    if let Ok(json) = obj.call_method0("model_dump") {
        return wyrd_utils_like_py_to_json(&json);
    }
    if let Ok(list) = obj.cast::<PyList>() {
        let mut out = Vec::with_capacity(list.len());
        for value in list.iter() {
            out.push(wyrd_utils_like_py_to_json(&value)?);
        }
        return Ok(serde_json::Value::Array(out));
    }
    if let Ok(dict) = obj.cast::<PyDict>() {
        let mut map = serde_json::Map::new();
        for (key, value) in dict {
            map.insert(
                key.extract::<String>()?,
                wyrd_utils_like_py_to_json(&value)?,
            );
        }
        return Ok(serde_json::Value::Object(map));
    }
    Ok(serde_json::Value::Null)
}
