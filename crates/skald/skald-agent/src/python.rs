//! Python docstrings and PyO3 glue for the Rust-backed `wyrd.agent` surface.

#![cfg(feature = "python")]

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use pyo3::exceptions::{PyRuntimeError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyList, PyModule, PyString};
use skald_prompt::{Prompt, PyProviderRequest};
use skald_spec::{ProviderRequest, ProviderResponse};
use wyrd_spec::metadata::{AnnotationKey, AnnotationValue, LabelKey, LabelValue};
use wyrd_spec::reference::PromptRef;

use crate::py_error::{AgentPyError, AgentPyResult};
use crate::{
    AfterAgentFn, AfterModelFn, AfterToolFn, Agent, AgentContext, AgentRun, BeforeAgentFn,
    BeforeModelFn, BeforeToolFn, CallbackOutcome, FinishReason, Role, RunConfig, SessionError,
    SessionId, SessionMemory, SessionTurn, default_prompt_resolver,
};

#[pymethods]
impl Agent {
    /// Build a runnable, savable Agent.
    ///
    /// Args:
    ///     prompt (Prompt | dict): Resolved prompt or PromptRef-like mapping.
    ///     name (str | None): Optional envelope name.
    ///     version (str | None): Optional envelope version.
    ///     space (str | None): Optional envelope space.
    ///     id (str | None): Optional stable runtime id.
    ///     tools (list | None): Optional runtime-local decorated tools.
    ///     run_config (RunConfig | None): Optional run configuration.
    ///     before_agent_callback (Callable | None): Optional callback fired before the run starts. Return None to continue, return replacement input text, or raise to abort.
    ///     after_agent_callback (Callable | None): Optional callback fired after the run completes. Return None to continue, return a replacement AgentRun, or raise to abort.
    ///     before_model_callback (Callable | None): Optional callback fired before each model invocation. Return None to continue, return a ProviderRequest replacement, or raise to abort.
    ///     after_model_callback (Callable | None): Optional callback fired after each model invocation. Return None to continue, return a ProviderResponse replacement, or raise to abort.
    ///     before_tool_callback (Callable | None): Optional callback fired before each tool invocation. Return None to continue, return replacement tool arguments, or raise to abort.
    ///     after_tool_callback (Callable | None): Optional callback fired after each tool invocation. Return None to continue, return replacement tool output, or raise to abort.
    ///     session (SessionMemory | None): Optional session memory object with recent and append methods.
    ///     labels (dict[str, str] | None): Optional envelope labels.
    ///     annotations (dict[str, str] | None): Optional envelope annotations.
    ///
    /// Returns:
    ///     Agent: New Agent ready to run.
    ///
    /// Raises:
    ///     WyrdError: When validation fails.
    #[new]
    #[pyo3(signature = (
        *,
        prompt,
        name = None,
        version = None,
        space = None,
        id = None,
        tools = None,
        run_config = None,
        before_agent_callback = None,
        after_agent_callback = None,
        before_model_callback = None,
        after_model_callback = None,
        before_tool_callback = None,
        after_tool_callback = None,
        session = None,
        labels = None,
        annotations = None
    ))]
    #[allow(clippy::too_many_arguments)]
    pub fn __new__(
        py: Python<'_>,
        prompt: &Bound<'_, PyAny>,
        name: Option<String>,
        version: Option<String>,
        space: Option<String>,
        id: Option<String>,
        tools: Option<Vec<Py<PyAny>>>,
        run_config: Option<Py<RunConfig>>,
        before_agent_callback: Option<Py<PyAny>>,
        after_agent_callback: Option<Py<PyAny>>,
        before_model_callback: Option<Py<PyAny>>,
        after_model_callback: Option<Py<PyAny>>,
        before_tool_callback: Option<Py<PyAny>>,
        after_tool_callback: Option<Py<PyAny>>,
        session: Option<Py<PyAny>>,
        labels: Option<HashMap<String, String>>,
        annotations: Option<HashMap<String, String>>,
    ) -> AgentPyResult<Self> {
        let mut agent = agent_from_prompt_py(prompt)?;

        if let Some(id) = id {
            agent = agent.with_id(id);
        }
        if let Some(run_config) = run_config {
            agent = agent.with_run_config(run_config.borrow(py).clone());
        }
        if let Some(session) = session {
            agent = agent.with_session(wrap_session(py, session)?);
        }
        for tool in tools.unwrap_or_default() {
            agent = agent.with_tool(skald_tool::python::wrap_callable(py, tool)?);
        }

        if let Some(name) = name {
            agent = agent.name(name);
        }
        if let Some(version) = version {
            agent = agent.version(version);
        }
        if let Some(space) = space {
            agent = agent.space(space);
        }
        if let Some(labels) = labels {
            agent = agent.labels(labels_from_py(labels)?);
        }
        if let Some(annotations) = annotations {
            agent = agent.annotations(annotations_from_py(annotations)?);
        }

        if let Some(callback) = before_agent_callback {
            agent = agent.before_agent(wrap_before_agent(py, callback)?);
        }
        if let Some(callback) = after_agent_callback {
            agent = agent.after_agent(wrap_after_agent(py, callback)?);
        }
        if let Some(callback) = before_model_callback {
            agent = agent.before_model(wrap_before_model(py, callback)?);
        }
        if let Some(callback) = after_model_callback {
            agent = agent.after_model(wrap_after_model(py, callback)?);
        }
        if let Some(callback) = before_tool_callback {
            agent = agent.before_tool(wrap_before_tool(py, callback)?);
        }
        if let Some(callback) = after_tool_callback {
            agent = agent.after_tool(wrap_after_tool(py, callback)?);
        }

        Ok(agent)
    }

    /// Optional envelope name.
    #[getter(name)]
    pub fn py_name(&self) -> Option<&str> {
        self.name_str()
    }

    /// Optional envelope version.
    #[getter(version)]
    pub fn py_version(&self) -> Option<&str> {
        self.version_str()
    }

    /// Optional envelope space.
    #[getter(space)]
    pub fn py_space(&self) -> Option<&str> {
        self.space_str()
    }

    /// Stable runtime id used for diagnostics.
    #[getter(id)]
    pub fn py_id(&self) -> &str {
        self.id()
    }

    /// Resolved Prompt that owns provider and model identity.
    #[getter(prompt)]
    pub fn py_prompt(&self) -> Prompt {
        self.prompt.as_ref().clone()
    }

    /// Provider name read from the resolved Prompt.
    #[getter]
    pub fn provider(&self) -> String {
        self.prompt.provider()
    }

    /// Model name read from the resolved Prompt.
    #[getter]
    pub fn model(&self) -> &str {
        &self.prompt.native().model
    }

    /// Runtime-local tool names attached to this Agent.
    #[getter(tool_names)]
    pub fn py_tool_names(&self) -> Vec<String> {
        self.tool_names().to_vec()
    }

    /// Save this Agent as a YAML Agent Card on local disk.
    #[pyo3(name = "save")]
    pub fn py_save(&self, path: PathBuf) -> AgentPyResult<()> {
        Ok(Agent::save(self, path)?)
    }

    /// Load an Agent from a YAML Agent Card on local disk.
    #[staticmethod]
    pub fn from_yaml(path: PathBuf) -> AgentPyResult<Self> {
        Ok(Self::from_yaml_path(
            path,
            skald_tool::default_registry(),
            default_prompt_resolver(),
        )?)
    }

    /// Return this Agent Card envelope as a YAML string.
    #[pyo3(name = "to_yaml_string")]
    pub fn py_to_yaml_string(&self) -> AgentPyResult<String> {
        Ok(Agent::to_yaml_string(self)?)
    }

    /// Return this Agent Card as a JSON-serializable Python mapping.
    #[pyo3(name = "to_card")]
    pub fn py_to_card(&self, py: Python<'_>) -> AgentPyResult<Py<PyAny>> {
        let card = Agent::to_card(self)?;
        let value = serde_json::to_value(card)?;
        Ok(wyrd_utils::py::json_to_pyobject(py, &value)?)
    }

    /// Return this Agent Card envelope as JSON.
    #[pyo3(name = "model_dump_json")]
    pub fn py_model_dump_json(&self) -> AgentPyResult<String> {
        Ok(serde_json::to_string(&Agent::to_card(self)?)?)
    }

    /// Validate an Agent Card envelope JSON payload into an Agent.
    #[staticmethod]
    pub fn model_validate_json(data: &str) -> AgentPyResult<Self> {
        let card = serde_json::from_str(data)?;
        Ok(Self::from_card(
            card,
            skald_tool::default_registry(),
            default_prompt_resolver(),
        )?)
    }

    /// Validate whether this local Agent can be durably registered.
    #[pyo3(name = "validate_registrable")]
    pub fn py_validate_registrable(&self) -> AgentPyResult<()> {
        Ok(Agent::validate_registrable(self)?)
    }

    /// Run the bounded tool loop and return an AgentRun value.
    #[pyo3(name = "run")]
    #[pyo3(signature = (input, *, session_id=None))]
    pub fn py_run(
        &self,
        py: Python<'_>,
        input: &Bound<'_, PyAny>,
        session_id: Option<String>,
    ) -> AgentPyResult<Py<PyAny>> {
        let input = input_to_string(input)?;
        let providers = skald_runtime::default_registry();
        let session_id = session_id.map(SessionId::new);
        let run = py.detach(|| {
            wyrd_runtime::runtime().block_on(self.run_with(providers.as_ref(), session_id, &input))
        })?;
        Ok(Py::new(py, run)?.into_any())
    }

    /// Add one runtime-local tool in place.
    #[pyo3(name = "add_tool")]
    pub fn py_add_tool(&mut self, py: Python<'_>, tool: Py<PyAny>) -> AgentPyResult<()> {
        let next = self
            .clone()
            .with_tool(skald_tool::python::wrap_callable(py, tool)?);
        *self = next;
        Ok(())
    }

    /// Replace runtime-local tools in place.
    #[pyo3(name = "set_tools")]
    pub fn py_set_tools(&mut self, py: Python<'_>, tools: Vec<Py<PyAny>>) -> AgentPyResult<()> {
        let mut resolved = Vec::with_capacity(tools.len());
        for tool in tools {
            resolved.push(skald_tool::python::wrap_callable(py, tool)?);
        }
        *self = self.clone().with_tools(resolved);
        Ok(())
    }

    /// Replace the resolved Prompt in place.
    #[pyo3(name = "with_prompt")]
    pub fn py_with_prompt(&mut self, prompt: &Bound<'_, PyAny>) -> AgentPyResult<()> {
        let next = self.clone().with_prompt(prompt_from_py(prompt)?);
        *self = next;
        Ok(())
    }

    /// Replace the session memory backend in place.
    #[pyo3(name = "with_session")]
    pub fn py_with_session(&mut self, py: Python<'_>, session: Py<PyAny>) -> AgentPyResult<()> {
        let next = self.clone().with_session(wrap_session(py, session)?);
        *self = next;
        Ok(())
    }

    /// Replace run configuration in place.
    #[pyo3(name = "with_run_config")]
    pub fn py_with_run_config(
        &mut self,
        py: Python<'_>,
        run_config: Py<RunConfig>,
    ) -> AgentPyResult<()> {
        let next = self.clone().with_run_config(run_config.borrow(py).clone());
        *self = next;
        Ok(())
    }

    /// Return this Agent as a runtime-local delegate tool.
    #[pyo3(name = "as_tool")]
    #[pyo3(signature = (*, description=None))]
    pub fn py_as_tool(
        &self,
        py: Python<'_>,
        description: Option<String>,
    ) -> AgentPyResult<Py<PyAny>> {
        let providers = skald_runtime::default_registry();
        let delegate = crate::AgentDelegateTool::new(Arc::new(self.clone()), providers);
        let tool = match description {
            Some(description) => delegate.with_description(description).into_tool(),
            None => delegate.into_tool(),
        };
        Ok(skald_tool::python::tool_callable_py(py, tool)?)
    }

    /// Register a before-agent callback in place.
    pub fn add_before_agent(&mut self, py: Python<'_>, callback: Py<PyAny>) -> AgentPyResult<()> {
        *self = self.clone().before_agent(wrap_before_agent(py, callback)?);
        Ok(())
    }

    /// Register an after-agent callback in place.
    pub fn add_after_agent(&mut self, py: Python<'_>, callback: Py<PyAny>) -> AgentPyResult<()> {
        *self = self.clone().after_agent(wrap_after_agent(py, callback)?);
        Ok(())
    }

    /// Register a before-model callback in place.
    pub fn add_before_model(&mut self, py: Python<'_>, callback: Py<PyAny>) -> AgentPyResult<()> {
        *self = self.clone().before_model(wrap_before_model(py, callback)?);
        Ok(())
    }

    /// Register an after-model callback in place.
    pub fn add_after_model(&mut self, py: Python<'_>, callback: Py<PyAny>) -> AgentPyResult<()> {
        *self = self.clone().after_model(wrap_after_model(py, callback)?);
        Ok(())
    }

    /// Register a before-tool callback in place.
    pub fn add_before_tool(&mut self, py: Python<'_>, callback: Py<PyAny>) -> AgentPyResult<()> {
        *self = self.clone().before_tool(wrap_before_tool(py, callback)?);
        Ok(())
    }

    /// Register an after-tool callback in place.
    pub fn add_after_tool(&mut self, py: Python<'_>, callback: Py<PyAny>) -> AgentPyResult<()> {
        *self = self.clone().after_tool(wrap_after_tool(py, callback)?);
        Ok(())
    }

    /// Return a concise Python representation.
    pub fn __repr__(&self) -> String {
        format!(
            "Agent(name={:?}, version={:?}, model={:?})",
            self.name_str(),
            self.version_str(),
            self.prompt.native().model
        )
    }
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
                let args = (ctx_to_py(py, ctx)?, Py::new(py, run.clone())?.into_any());
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
                    wyrd_utils::py::json_to_pyobject(
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
                    wyrd_utils::py::json_to_pyobject(
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
                    wyrd_utils::py::json_to_pyobject(py, args)?,
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
                    wyrd_utils::py::json_to_pyobject(py, &value)?,
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
    wyrd_utils::py::register_wyrd_error_exception(module)?;
    module.add_class::<Agent>()?;
    module.add_class::<AgentRun>()?;
    module.add_class::<FinishReason>()?;
    module.add_class::<RunConfig>()?;
    module.add_class::<Role>()?;
    module.add_class::<SessionTurn>()?;
    Ok(())
}

fn agent_from_prompt_py(value: &Bound<'_, PyAny>) -> AgentPyResult<Agent> {
    if let Ok(prompt) = value.extract::<PyRef<'_, Prompt>>() {
        return Ok(Agent::new(prompt.clone()));
    }
    let prompt_ref = prompt_ref_from_py(value)?;
    Ok(Agent::try_from_ref(prompt_ref, default_prompt_resolver())?)
}

fn prompt_from_py(value: &Bound<'_, PyAny>) -> AgentPyResult<Prompt> {
    Ok(value
        .extract::<PyRef<'_, Prompt>>()
        .map_err(|error| PyTypeError::new_err(error.to_string()))?
        .clone())
}

fn prompt_ref_from_py(value: &Bound<'_, PyAny>) -> AgentPyResult<PromptRef> {
    if let Ok(json) = value.call_method0("model_dump_json") {
        let data = json.extract::<String>()?;
        return Ok(serde_json::from_str(&data)?);
    }
    let json = wyrd_utils::py::pyobject_to_json(value)?;
    Ok(serde_json::from_value(json)?)
}

fn labels_from_py(
    values: HashMap<String, String>,
) -> AgentPyResult<BTreeMap<LabelKey, LabelValue>> {
    values
        .into_iter()
        .map(|(key, value)| {
            Ok((
                LabelKey::new_user(key).map_err(metadata_py_error)?,
                LabelValue::new_user(value).map_err(metadata_py_error)?,
            ))
        })
        .collect()
}

fn annotations_from_py(
    values: HashMap<String, String>,
) -> AgentPyResult<BTreeMap<AnnotationKey, AnnotationValue>> {
    values
        .into_iter()
        .map(|(key, value)| {
            Ok((
                AnnotationKey::new_user(key).map_err(metadata_py_error)?,
                AnnotationValue::new_user(value).map_err(metadata_py_error)?,
            ))
        })
        .collect()
}

fn metadata_py_error(error: wyrd_spec::metadata::MetadataError) -> AgentPyError {
    PyTypeError::new_err(error.to_string()).into()
}

fn input_to_string(value: &Bound<'_, PyAny>) -> PyResult<String> {
    if let Ok(value) = value.extract::<String>() {
        return Ok(value);
    }
    if value.is_instance_of::<PyString>() {
        return value.extract();
    }
    serde_json::to_string(&wyrd_utils::py::pyobject_to_json(value)?)
        .map_err(|error| PyRuntimeError::new_err(error.to_string()))
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
            return CallbackOutcome::Abort(wyrd_utils::py::py_err_to_wyrd_error(py, error));
        }
        let result = match call(py) {
            Ok(result) => result,
            Err(error) => {
                return CallbackOutcome::Abort(wyrd_utils::py::py_err_to_wyrd_error(py, error));
            }
        };
        callback_outcome(&result, replacement)
    })
}

fn callback_outcome<T>(
    result: &Bound<'_, PyAny>,
    replacement: fn(&Bound<'_, PyAny>) -> PyResult<T>,
) -> CallbackOutcome<T> {
    if result.is_none() {
        return CallbackOutcome::Continue;
    }
    match replacement(result) {
        Ok(value) => CallbackOutcome::ReplaceWith(value),
        Err(error) => {
            CallbackOutcome::Abort(wyrd_spec::error::WyrdError::AgentCallbackReturnType {
                message: format!("callback returned wrong type: {error}"),
                details: serde_json::json!({ "source": error.to_string() }),
            })
        }
    }
}

fn ctx_to_py(py: Python<'_>, ctx: &AgentContext) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    dict.set_item("agent_id", &ctx.agent_id)?;
    dict.set_item("session_id", &ctx.session_id)?;
    dict.set_item("iteration", ctx.iteration)?;
    dict.set_item(
        "conversation",
        wyrd_utils::py::json_to_pyobject(
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
    let json = wyrd_utils::py::pyobject_to_json(value)?;
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
    let json = wyrd_utils::py::pyobject_to_json(value)?;
    serde_json::from_value(json).map_err(|error| PyTypeError::new_err(error.to_string()))
}

fn extract_string_replacement(value: &Bound<'_, PyAny>) -> PyResult<String> {
    value.extract()
}

fn extract_provider_request_replacement(value: &Bound<'_, PyAny>) -> PyResult<ProviderRequest> {
    if let Ok(request) = value.extract::<PyRef<'_, PyProviderRequest>>() {
        return Ok(request.native().clone());
    }
    serde_json::from_value(wyrd_utils::py::pyobject_to_json(value)?)
        .map_err(|error| PyTypeError::new_err(error.to_string()))
}

fn extract_provider_response_replacement(value: &Bound<'_, PyAny>) -> PyResult<ProviderResponse> {
    serde_json::from_value(wyrd_utils::py::pyobject_to_json(value)?)
        .map_err(|error| PyTypeError::new_err(error.to_string()))
}

fn extract_json_replacement(value: &Bound<'_, PyAny>) -> PyResult<serde_json::Value> {
    wyrd_utils::py::pyobject_to_json(value)
}

fn extract_tool_result_replacement(
    value: &Bound<'_, PyAny>,
) -> PyResult<Result<serde_json::Value, skald_tool::ToolError>> {
    Ok(Ok(wyrd_utils::py::pyobject_to_json(value)?))
}

fn extract_agent_run_replacement(value: &Bound<'_, PyAny>) -> PyResult<AgentRun> {
    serde_json::from_value(wyrd_utils::py::pyobject_to_json(value)?)
        .map_err(|error| PyTypeError::new_err(error.to_string()))
}
