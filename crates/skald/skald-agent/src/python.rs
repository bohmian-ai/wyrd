//! Python docstrings and PyO3 glue for the Rust-backed `wyrd.agent` surface.

#![cfg(feature = "python")]

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyList, PyModule, PyString};
use skald_prompt::{Prompt, PyProviderRequest};
use skald_spec::{ProviderRequest, ProviderResponse};
use wyrd_spec::error::WyrdError;
use wyrd_spec::metadata::{AnnotationKey, AnnotationValue, LabelKey, LabelValue};
use wyrd_spec::reference::PromptRef;
use wyrd_utils::py::{WyrdPyError, WyrdPyResult, py_err_to_wyrd_error};

use crate::error::AgentError;
use crate::{
    AfterAgentFn, AfterModelFn, AfterToolFn, Agent, AgentContext, AgentRun, BeforeAgentFn,
    BeforeModelFn, BeforeToolFn, CallbackOutcome, FinishReason, Role, RunConfig, SessionError,
    SessionId, SessionMemory, SessionTurn, default_prompt_resolver,
};

impl From<AgentError> for WyrdPyError {
    /// Widen an agent failure into the shared Python boundary error.
    ///
    /// The catalog projection in [`crate::error`] owns every public metadata
    /// field, so `?` on an [`AgentError`] inside a `#[pymethods]` body raises
    /// the shared `wyrd.WyrdError` with its canonical code and status.
    fn from(error: AgentError) -> Self {
        Self::from(WyrdError::from(error))
    }
}

/// Reject a caller-supplied argument that the agent surface cannot accept.
fn invalid_argument(name: &str, detail: impl std::fmt::Display) -> WyrdPyError {
    AgentError::InvalidArgument {
        name: name.to_owned(),
        detail: detail.to_string(),
    }
    .into()
}

/// Project a failure to decode caller-supplied JSON into the agent surface.
fn json_decode_error(error: &serde_json::Error) -> WyrdPyError {
    invalid_argument("json", error)
}

/// Project a failure to instantiate the caller's declared output class.
fn structured_decode_error(error: &impl std::fmt::Display) -> WyrdPyError {
    AgentError::StructuredOutputDecode {
        agent: "<python>".to_owned(),
        detail: error.to_string(),
    }
    .into()
}

/// Project an interpreter-side or self-serialization failure at this boundary.
///
/// These paths are unreachable for well-formed agent state; surfacing them as
/// catalog `WYRD_SPEC_500_INTERNAL` keeps the boundary from raising a bare
/// Python exception when the interpreter refuses an allocation or conversion.
fn boundary_internal(detail: &impl std::fmt::Display) -> WyrdPyError {
    WyrdPyError::from(WyrdError::Internal {
        message: detail.to_string(),
        details: serde_json::json!({ "boundary": "skald_agent_python" }),
    })
}

/// Re-enter the catalog from a PyO3-originated failure.
///
/// Used where an interpreter call inside a Wyrd-owned operation fails; the
/// shared converter preserves an already-structured Wyrd exception and
/// classifies anything else as agent validation.
fn from_py_err(error: PyErr) -> WyrdPyError {
    Python::attach(|py| WyrdPyError::from(py_err_to_wyrd_error(py, error)))
}

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
    ///     provider_base_url (str | None): Override the provider endpoint for this agent only.
    ///         Useful for routing through an AI gateway (e.g. LiteLLM). Falls back to the
    ///         standard environment variable when `provider_api_key` is not supplied.
    ///     provider_api_key (str | None): API key for the overridden provider endpoint.
    ///         When omitted, the standard environment variable for the prompt's provider is used.
    ///     output_type (type | None): Optional Python class for parsing AgentRun.parsed.
    ///         Must be callable and accept keyword arguments matching the structured output
    ///         fields (typically a pydantic.BaseModel subclass). Does NOT inject a schema into
    ///         the provider request — use Prompt(output=...) for schema enforcement.
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
        annotations = None,
        provider_base_url = None,
        provider_api_key = None,
        output_type = None
    ))]
    // justification: pyo3 #[new] signature must match the Python API surface; the params correspond 1:1 to the exposed Python constructor
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
        provider_base_url: Option<String>,
        provider_api_key: Option<String>,
        output_type: Option<&Bound<'_, PyAny>>,
    ) -> WyrdPyResult<Self> {
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
            agent =
                agent.with_tool(skald_tool::python::wrap_callable(py, tool).map_err(from_py_err)?);
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
            agent = agent.before_agent(wrap_before_agent(py, callback));
        }
        if let Some(callback) = after_agent_callback {
            agent = agent.after_agent(wrap_after_agent(py, callback));
        }
        if let Some(callback) = before_model_callback {
            agent = agent.before_model(wrap_before_model(py, callback));
        }
        if let Some(callback) = after_model_callback {
            agent = agent.after_model(wrap_after_model(py, callback));
        }
        if let Some(callback) = before_tool_callback {
            agent = agent.before_tool(wrap_before_tool(py, callback));
        }
        if let Some(callback) = after_tool_callback {
            agent = agent.after_tool(wrap_after_tool(py, callback));
        }

        if let Some(base_url) = provider_base_url {
            let provider_name = agent.prompt.native().request.provider();
            let registry = skald_runtime::ProviderRegistry::for_provider(
                &provider_name,
                base_url,
                provider_api_key,
            )
            .map_err(|error| {
                AgentError::Provider(skald_runtime::SkaldRuntimeError::from_provider(
                    provider_name.clone(),
                    error,
                ))
            })?;
            agent = agent.with_provider_registry(Arc::new(registry));
        }

        agent.py_output_cls = output_cls_from_py(output_type)?;

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
    pub fn py_save(&self, path: PathBuf) -> WyrdPyResult<()> {
        Ok(Agent::save(self, path)?)
    }

    /// Load an Agent from a YAML Agent Card on local disk.
    #[staticmethod]
    pub fn from_yaml(path: PathBuf) -> WyrdPyResult<Self> {
        Ok(Self::from_yaml_path(
            path,
            skald_tool::default_registry(),
            default_prompt_resolver(),
        )?)
    }

    /// Return this Agent Card envelope as a YAML string.
    #[pyo3(name = "to_yaml_string")]
    pub fn py_to_yaml_string(&self) -> WyrdPyResult<String> {
        Ok(Agent::to_yaml_string(self)?)
    }

    /// Return this Agent Card as a JSON-serializable Python mapping.
    #[pyo3(name = "to_card")]
    pub fn py_to_card(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        let card = Agent::to_card(self)?;
        let value = serde_json::to_value(card).map_err(|error| boundary_internal(&error))?;
        wyrd_utils::py::json_to_pyobject(py, &value).map_err(from_py_err)
    }

    /// Return this Agent Card envelope as JSON.
    #[pyo3(name = "model_dump_json")]
    pub fn py_model_dump_json(&self) -> WyrdPyResult<String> {
        serde_json::to_string(&Agent::to_card(self)?).map_err(|error| boundary_internal(&error))
    }

    /// Validate an Agent Card envelope JSON payload into an Agent.
    #[staticmethod]
    pub fn model_validate_json(data: &str) -> WyrdPyResult<Self> {
        let card = serde_json::from_str(data).map_err(|error| json_decode_error(&error))?;
        Ok(Self::from_card(
            card,
            skald_tool::default_registry(),
            default_prompt_resolver(),
        )?)
    }

    /// Validate whether this local Agent can be durably registered.
    #[pyo3(name = "validate_registrable")]
    pub fn py_validate_registrable(&self) -> WyrdPyResult<()> {
        Ok(Agent::validate_registrable(self)?)
    }

    /// Run the bounded tool loop and return an AgentRun value.
    #[pyo3(name = "run")]
    #[pyo3(signature = (input, *, session_id=None, output_type=None))]
    pub fn py_run(
        &self,
        py: Python<'_>,
        input: &Bound<'_, PyAny>,
        session_id: Option<String>,
        output_type: Option<&Bound<'_, PyAny>>,
    ) -> WyrdPyResult<Py<PyAny>> {
        let input_str = input_to_string(input)?;
        let providers = skald_runtime::default_registry();
        let session_id = session_id.map(SessionId::new);
        let mut run = py.detach(|| {
            wyrd_runtime::runtime().block_on(self.run_with(
                providers.as_ref(),
                session_id,
                &input_str,
            ))
        })?;

        // Resolution order: run(output_type=) > Agent.py_output_cls > Prompt.py_output_cls
        let call_cls = output_cls_from_py(output_type)?;
        let effective_cls = call_cls
            .as_ref()
            .map(|c| c.as_ref().clone_ref(py))
            .or_else(|| {
                self.py_output_cls
                    .as_ref()
                    .map(|c| c.as_ref().clone_ref(py))
            })
            .or_else(|| self.prompt.output_cls().map(|c| c.as_ref().clone_ref(py)));
        if let (Some(cls), Some(map)) = (effective_cls, run.structured_output.as_ref()) {
            run.parsed = Some(Arc::new(instantiate_parsed(py, &cls, &run.output, map)?));
        }

        Ok(Py::new(py, run).map_err(from_py_err)?.into_any())
    }

    /// Add one runtime-local tool in place.
    #[pyo3(name = "add_tool")]
    pub fn py_add_tool(&mut self, py: Python<'_>, tool: Py<PyAny>) -> WyrdPyResult<()> {
        let next = self
            .clone()
            .with_tool(skald_tool::python::wrap_callable(py, tool).map_err(from_py_err)?);
        *self = next;
        Ok(())
    }

    /// Replace runtime-local tools in place.
    #[pyo3(name = "set_tools")]
    pub fn py_set_tools(&mut self, py: Python<'_>, tools: Vec<Py<PyAny>>) -> WyrdPyResult<()> {
        let mut resolved = Vec::with_capacity(tools.len());
        for tool in tools {
            resolved.push(skald_tool::python::wrap_callable(py, tool).map_err(from_py_err)?);
        }
        *self = self.clone().with_tools(resolved);
        Ok(())
    }

    /// Replace the resolved Prompt in place.
    #[pyo3(name = "with_prompt")]
    pub fn py_with_prompt(&mut self, prompt: &Bound<'_, PyAny>) -> WyrdPyResult<()> {
        let next = self.clone().with_prompt(prompt_from_py(prompt)?);
        *self = next;
        Ok(())
    }

    /// Replace the session memory backend in place.
    #[pyo3(name = "with_session")]
    pub fn py_with_session(&mut self, py: Python<'_>, session: Py<PyAny>) -> WyrdPyResult<()> {
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
    ) -> WyrdPyResult<()> {
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
    ) -> WyrdPyResult<Py<PyAny>> {
        let providers = skald_runtime::default_registry();
        let delegate = crate::AgentDelegateTool::new(Arc::new(self.clone()), providers);
        let tool = match description {
            Some(description) => delegate.with_description(description).into_tool(),
            None => delegate.into_tool(),
        };
        skald_tool::python::tool_callable_py(py, tool).map_err(from_py_err)
    }

    /// Register a before-agent callback in place.
    pub fn add_before_agent(&mut self, py: Python<'_>, callback: Py<PyAny>) -> WyrdPyResult<()> {
        *self = self.clone().before_agent(wrap_before_agent(py, callback));
        Ok(())
    }

    /// Register an after-agent callback in place.
    pub fn add_after_agent(&mut self, py: Python<'_>, callback: Py<PyAny>) -> WyrdPyResult<()> {
        *self = self.clone().after_agent(wrap_after_agent(py, callback));
        Ok(())
    }

    /// Register a before-model callback in place.
    pub fn add_before_model(&mut self, py: Python<'_>, callback: Py<PyAny>) -> WyrdPyResult<()> {
        *self = self.clone().before_model(wrap_before_model(py, callback));
        Ok(())
    }

    /// Register an after-model callback in place.
    pub fn add_after_model(&mut self, py: Python<'_>, callback: Py<PyAny>) -> WyrdPyResult<()> {
        *self = self.clone().after_model(wrap_after_model(py, callback));
        Ok(())
    }

    /// Register a before-tool callback in place.
    pub fn add_before_tool(&mut self, py: Python<'_>, callback: Py<PyAny>) -> WyrdPyResult<()> {
        *self = self.clone().before_tool(wrap_before_tool(py, callback));
        Ok(())
    }

    /// Register an after-tool callback in place.
    pub fn add_after_tool(&mut self, py: Python<'_>, callback: Py<PyAny>) -> WyrdPyResult<()> {
        *self = self.clone().after_tool(wrap_after_tool(py, callback));
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
    ) -> WyrdPyResult<Self> {
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
    pub fn model_dump(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        session_turn_to_py_dict(py, self)
    }

    /// Return this turn as JSON.
    pub fn model_dump_json(&self) -> WyrdPyResult<String> {
        serde_json::to_string(self).map_err(|error| boundary_internal(&error))
    }

    /// Validate a Python mapping or `SessionTurn` instance.
    #[staticmethod]
    pub fn model_validate(value: &Bound<'_, PyAny>) -> WyrdPyResult<Self> {
        session_turn_from_py(value)
    }

    /// Validate JSON into a `SessionTurn`.
    #[staticmethod]
    pub fn model_validate_json(data: &str) -> WyrdPyResult<Self> {
        serde_json::from_str(data).map_err(|error| json_decode_error(&error))
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
pub fn wrap_before_agent(_py: Python<'_>, cb: Py<PyAny>) -> BeforeAgentFn {
    Arc::new(move |ctx, input| {
        invoke_callback(
            &cb,
            |py| {
                let args = (ctx_to_py(py, ctx)?, input);
                cb.bind(py).call1(args)
            },
            extract_string_replacement,
        )
    })
}

/// Wrap a callable as an after-agent callback.
pub fn wrap_after_agent(_py: Python<'_>, cb: Py<PyAny>) -> AfterAgentFn {
    Arc::new(move |ctx, run| {
        invoke_callback(
            &cb,
            |py| {
                let args = (ctx_to_py(py, ctx)?, Py::new(py, run.clone())?.into_any());
                cb.bind(py).call1(args)
            },
            extract_agent_run_replacement,
        )
    })
}

/// Wrap a callable as a before-model callback.
pub fn wrap_before_model(_py: Python<'_>, cb: Py<PyAny>) -> BeforeModelFn {
    Arc::new(move |ctx, request| {
        invoke_callback(
            &cb,
            |py| {
                let py_request = Py::new(
                    py,
                    skald_prompt::PyProviderRequest::from_native(request.clone()),
                )?
                .into_any();
                cb.bind(py).call1((ctx_to_py(py, ctx)?, py_request))
            },
            extract_provider_request_replacement,
        )
    })
}

/// Wrap a callable as an after-model callback.
pub fn wrap_after_model(_py: Python<'_>, cb: Py<PyAny>) -> AfterModelFn {
    Arc::new(move |ctx, response| {
        invoke_callback(
            &cb,
            |py| {
                let py_response = Py::new(
                    py,
                    skald_prompt::python::PyProviderResponse::from_native(response.clone()),
                )?
                .into_any();
                cb.bind(py).call1((ctx_to_py(py, ctx)?, py_response))
            },
            extract_provider_response_replacement,
        )
    })
}

/// Wrap a callable as a before-tool callback.
pub fn wrap_before_tool(_py: Python<'_>, cb: Py<PyAny>) -> BeforeToolFn {
    Arc::new(move |ctx, tool, args| {
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
    })
}

/// Wrap a callable as an after-tool callback.
pub fn wrap_after_tool(_py: Python<'_>, cb: Py<PyAny>) -> AfterToolFn {
    Arc::new(move |ctx, tool, result| {
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
    })
}

/// Wrap a Python session object.
///
/// # Errors
/// Returns `WYRD_AGENT_422_INVALID_ARGUMENT` when the object does not expose
/// callable `recent()` and `append()` methods.
pub fn wrap_session(py: Python<'_>, session: Py<PyAny>) -> WyrdPyResult<Arc<dyn SessionMemory>> {
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

fn agent_from_prompt_py(value: &Bound<'_, PyAny>) -> WyrdPyResult<Agent> {
    if let Ok(prompt) = value.extract::<PyRef<'_, Prompt>>() {
        return Ok(Agent::new(prompt.clone()));
    }
    let prompt_ref = prompt_ref_from_py(value)?;
    Ok(Agent::try_from_ref(prompt_ref, default_prompt_resolver())?)
}

fn prompt_from_py(value: &Bound<'_, PyAny>) -> WyrdPyResult<Prompt> {
    Ok(value
        .extract::<PyRef<'_, Prompt>>()
        .map_err(|error| invalid_argument("prompt", error))?
        .clone())
}

fn prompt_ref_from_py(value: &Bound<'_, PyAny>) -> WyrdPyResult<PromptRef> {
    if let Ok(json) = value.call_method0("model_dump_json") {
        let data = json
            .extract::<String>()
            .map_err(|error| invalid_argument("prompt", error))?;
        return serde_json::from_str(&data).map_err(|error| json_decode_error(&error));
    }
    let json = wyrd_utils::py::pyobject_to_json(value).map_err(from_py_err)?;
    serde_json::from_value(json).map_err(|error| json_decode_error(&error))
}

fn labels_from_py(values: HashMap<String, String>) -> WyrdPyResult<BTreeMap<LabelKey, LabelValue>> {
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
) -> WyrdPyResult<BTreeMap<AnnotationKey, AnnotationValue>> {
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

fn metadata_py_error(error: wyrd_spec::metadata::MetadataError) -> WyrdPyError {
    invalid_argument("metadata", error)
}

fn input_to_string(value: &Bound<'_, PyAny>) -> WyrdPyResult<String> {
    if let Ok(value) = value.extract::<String>() {
        return Ok(value);
    }
    if value.is_instance_of::<PyString>() {
        return value
            .extract()
            .map_err(|error| invalid_argument("input", error));
    }
    let json = wyrd_utils::py::pyobject_to_json(value).map_err(from_py_err)?;
    serde_json::to_string(&json).map_err(|error| boundary_internal(&error))
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
                .map_err(|error| PyErr::from(boundary_internal(&error)))?,
        )?,
    )?;
    Ok(dict.into_any().unbind())
}

fn validate_callable_method(py: Python<'_>, obj: &Py<PyAny>, method: &str) -> WyrdPyResult<()> {
    let method_obj = obj
        .bind(py)
        .getattr(method)
        .map_err(|_| invalid_argument("session", format!("must define callable {method}()")))?;
    if method_obj.is_callable() {
        Ok(())
    } else {
        Err(invalid_argument(
            "session",
            format!("attribute {method:?} must be callable"),
        ))
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

fn role_from_py(value: &Bound<'_, PyAny>) -> WyrdPyResult<Role> {
    if let Ok(role) = value.extract::<Role>() {
        return Ok(role);
    }
    let name = value
        .extract::<String>()
        .map_err(|error| invalid_argument("role", error))?;
    match name.as_str() {
        "system" | "System" => Ok(Role::System),
        "user" | "User" => Ok(Role::User),
        "assistant" | "Assistant" => Ok(Role::Assistant),
        "tool" | "Tool" => Ok(Role::Tool),
        other => Err(invalid_argument(
            "role",
            format!("unsupported session role {other:?}"),
        )),
    }
}

fn session_turn_to_py_dict(py: Python<'_>, turn: &SessionTurn) -> WyrdPyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    dict.set_item("role", role_as_str(turn.role))
        .map_err(from_py_err)?;
    dict.set_item("content", &turn.content)
        .map_err(from_py_err)?;
    dict.set_item("call_id", &turn.call_id)
        .map_err(from_py_err)?;
    Ok(dict.into_any().unbind())
}

fn session_turn_from_py(value: &Bound<'_, PyAny>) -> WyrdPyResult<SessionTurn> {
    if let Ok(turn) = value.extract::<PyRef<'_, SessionTurn>>() {
        return Ok(turn.clone());
    }
    let json = wyrd_utils::py::pyobject_to_json(value).map_err(from_py_err)?;
    serde_json::from_value(json).map_err(|error| json_decode_error(&error))
}

fn session_turns_from_py(value: &Bound<'_, PyAny>) -> WyrdPyResult<Vec<SessionTurn>> {
    if let Ok(list) = value.cast::<PyList>() {
        let mut turns = Vec::with_capacity(list.len());
        for item in list.iter() {
            turns.push(session_turn_from_py(&item)?);
        }
        return Ok(turns);
    }
    let json = wyrd_utils::py::pyobject_to_json(value).map_err(from_py_err)?;
    serde_json::from_value(json).map_err(|error| json_decode_error(&error))
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
    if let Ok(py_resp) = value.extract::<PyRef<'_, skald_prompt::python::PyProviderResponse>>() {
        return Ok(py_resp.native().clone());
    }
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

/// Extract and retain a Python class reference from an `output_type=` kwarg.
///
/// Accepts a Pydantic BaseModel subclass or any callable. Returns None when
/// the value is None or py-None. Returns an error when the value is not callable.
///
/// Does NOT extract a schema from the class. Schema must already be set on
/// the Prompt via `Prompt(output=...)` or in the YAML card spec.
fn output_cls_from_py(value: Option<&Bound<'_, PyAny>>) -> WyrdPyResult<Option<Arc<Py<PyAny>>>> {
    let Some(value) = value.filter(|v| !v.is_none()) else {
        return Ok(None);
    };
    if !value.is_callable() {
        return Err(invalid_argument(
            "output_type",
            "must be a callable class (e.g. a pydantic.BaseModel subclass)",
        ));
    }
    Ok(Some(Arc::new(value.clone().unbind())))
}

/// Instantiate a typed model from the agent's raw output text and structured map.
///
/// Pydantic path: calls `cls.model_validate_json(output_text)`.
/// Generic callable path: calls `cls(**structured_output_dict)`.
///
/// Returns `WYRD_AGENT_422_STRUCTURED_DECODE` on instantiation failure.
fn instantiate_parsed<'py>(
    py: Python<'py>,
    cls: &Py<PyAny>,
    output_text: &str,
    map: &serde_json::Map<String, serde_json::Value>,
) -> WyrdPyResult<Py<PyAny>> {
    let bound = cls.bind(py);

    if bound.hasattr("model_validate_json").unwrap_or(false) {
        return bound
            .call_method1("model_validate_json", (output_text,))
            .map(|r| r.unbind())
            .map_err(|e| structured_decode_error(&e));
    }

    let val = serde_json::Value::Object(map.clone());
    let py_val =
        wyrd_utils::py::json_to_pyobject(py, &val).map_err(|e| structured_decode_error(&e))?;
    let kwargs = py_val
        .cast_bound::<PyDict>(py)
        .map_err(|e| structured_decode_error(&e))?;
    bound
        .call((), Some(kwargs))
        .map(|r| r.unbind())
        .map_err(|e| structured_decode_error(&e))
}
