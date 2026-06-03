//! Python boundary for the engine-direct Skald agent surface.

#![cfg(feature = "python")]

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use pyo3::IntoPyObjectExt;
use pyo3::exceptions::{PyRuntimeError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyList, PyModule, PyString};
use skald_prompt::{Prompt, PyProviderRequest};
use skald_runtime::python::PyProviderRegistryInner;
use skald_spec::{ProviderRequest, ProviderResponse};
use wyrd_spec::metadata::{AnnotationKey, AnnotationValue, LabelKey, LabelValue};
use wyrd_spec::reference::PromptRef;

use crate::py_error::{AgentPyError, AgentPyResult};
use crate::{
    AfterAgentFn, AfterModelFn, AfterToolFn, Agent, AgentContext, AgentRun, BeforeAgentFn,
    BeforeModelFn, BeforeToolFn, CallbackOutcome, Role, RunConfig, SessionError, SessionId,
    SessionMemory, SessionTurn, default_prompt_resolver,
};

#[pymethods]
impl Agent {
    /// Build a runnable Agent from a resolved Prompt.
    #[new]
    #[pyo3(signature = (
        *,
        prompt,
        name = None,
        version = None,
        space = None,
        id = None,
        tools = None,
        providers = None,
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
        providers: Option<&Bound<'_, PyAny>>,
        run_config: Option<&Bound<'_, PyAny>>,
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
            agent = agent.with_run_config(run_config_from_py(run_config)?);
        }
        if let Some(session) = session {
            agent = agent.with_session(wrap_session(py, session)?);
        }
        if let Some(providers) = providers {
            agent = agent.with_providers(provider_registry_from_py(providers)?);
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

    /// Optional card name.
    #[getter(name)]
    pub fn py_name(&self) -> Option<&str> {
        self.name_str()
    }

    /// Optional card version.
    #[getter(version)]
    pub fn py_version(&self) -> Option<&str> {
        self.version_str()
    }

    /// Optional card space.
    #[getter(space)]
    pub fn py_space(&self) -> Option<&str> {
        self.space_str()
    }

    /// Runtime agent id.
    #[getter(id)]
    pub fn py_id(&self) -> &str {
        self.id()
    }

    /// Resolved prompt.
    #[getter(prompt)]
    pub fn py_prompt(&self) -> Prompt {
        self.prompt.as_ref().clone()
    }

    /// Prompt provider name.
    #[getter]
    pub fn provider(&self) -> String {
        self.prompt.provider()
    }

    /// Prompt model name.
    #[getter]
    pub fn model(&self) -> &str {
        &self.prompt.native().model
    }

    /// Runtime-local tool names attached to this agent.
    #[getter(tool_names)]
    pub fn py_tool_names(&self) -> Vec<String> {
        self.tool_names().to_vec()
    }

    /// Save this Agent Card YAML envelope to local disk.
    #[pyo3(name = "save")]
    pub fn py_save(&self, path: PathBuf) -> AgentPyResult<()> {
        Ok(Agent::save(self, path)?)
    }

    /// Load an Agent Card YAML envelope from local disk.
    #[staticmethod]
    pub fn from_yaml(path: PathBuf) -> AgentPyResult<Self> {
        Ok(Self::from_yaml_path(
            path,
            skald_tool::default_registry(),
            default_prompt_resolver(),
        )?)
    }

    /// Return this Agent Card as a YAML string.
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

    /// Validate whether this local Agent Card can be durably registered.
    #[pyo3(name = "validate_registrable")]
    pub fn py_validate_registrable(&self) -> AgentPyResult<()> {
        Ok(Agent::validate_registrable(self)?)
    }

    /// Run the bounded tool loop.
    #[pyo3(name = "run")]
    #[pyo3(signature = (input, *, session_id=None))]
    pub fn py_run(
        &self,
        py: Python<'_>,
        input: &Bound<'_, PyAny>,
        session_id: Option<String>,
    ) -> AgentPyResult<Py<PyAny>> {
        let input = input_to_string(input)?;
        let providers = self
            .providers
            .clone()
            .unwrap_or_else(skald_runtime::default_registry);
        let session_id = session_id.map(SessionId::new);
        let run = py.detach(|| {
            wyrd_runtime::runtime().block_on(self.run_with(providers.as_ref(), session_id, &input))
        })?;
        Ok(agent_run_to_py(py, run)?)
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

    /// Replace the resolved prompt in place.
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
    pub fn py_with_run_config(&mut self, run_config: &Bound<'_, PyAny>) -> AgentPyResult<()> {
        let next = self
            .clone()
            .with_run_config(run_config_from_py(run_config)?);
        *self = next;
        Ok(())
    }

    /// Return this agent as a runtime-local delegate tool.
    #[pyo3(name = "as_tool")]
    #[pyo3(signature = (*, description=None))]
    pub fn py_as_tool(
        &self,
        py: Python<'_>,
        description: Option<String>,
    ) -> AgentPyResult<Py<PyAny>> {
        let providers = self
            .providers
            .clone()
            .unwrap_or_else(skald_runtime::default_registry);
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
    wyrd_utils::py::register_wyrd_error_exception(module)?;
    module.add_class::<Agent>()?;
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

fn provider_registry_from_py(
    value: &Bound<'_, PyAny>,
) -> AgentPyResult<Arc<skald_runtime::ProviderRegistry>> {
    if let Ok(registry) = value.extract::<PyRef<'_, PyProviderRegistryInner>>() {
        return Ok(registry.inner.clone());
    }
    if let Ok(inner) = value.getattr("_inner") {
        let registry = inner
            .extract::<PyRef<'_, PyProviderRegistryInner>>()
            .map_err(|error| PyTypeError::new_err(error.to_string()))?;
        return Ok(registry.inner.clone());
    }
    Err(PyTypeError::new_err("providers must be a wyrd ProviderRegistry").into())
}

fn run_config_from_py(value: &Bound<'_, PyAny>) -> AgentPyResult<RunConfig> {
    if let Ok(json) = wyrd_utils::py::pyobject_to_json(value) {
        if let Ok(spec) = serde_json::from_value(json) {
            return Ok(crate::run_config_from_agent_run_config_spec(&spec));
        }
    }

    let mut config = RunConfig::default();
    if let Some(max_iterations) = optional_attr::<u32>(value, "max_iterations")? {
        config.max_iterations = max_iterations;
    }
    config.tool_concurrency_cap = optional_attr(value, "tool_concurrency_cap")?;
    config.session_recent_limit = optional_attr(value, "session_recent_limit")?;
    if let Some(timeout_ms) = optional_attr::<u64>(value, "timeout_ms")? {
        config.timeout = Some(Duration::from_millis(timeout_ms));
    }
    Ok(config)
}

fn optional_attr<T>(value: &Bound<'_, PyAny>, name: &str) -> PyResult<Option<T>>
where
    for<'py> T: FromPyObject<'py, 'py, Error = PyErr>,
{
    match value.getattr(name) {
        Ok(attr) if attr.is_none() => Ok(None),
        Ok(attr) => attr.extract().map(Some),
        Err(_) => Ok(None),
    }
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
    Ok(
        serde_json::to_string(&wyrd_utils::py::pyobject_to_json(value)?)
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))?,
    )
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
