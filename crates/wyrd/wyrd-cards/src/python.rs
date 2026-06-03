//! Python registration for Agent Card holder types.

#![cfg(feature = "python")]

use std::sync::Arc;

use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyModule};
use skald_agent::{AgentDelegateTool, RunConfig, SessionId};
use skald_prompt::Prompt;
use skald_runtime::ProviderRegistry;
use wyrd_interfaces::error::{CardPyResult, WyrdPyError};

use crate::agent::{AgentBuilder, AgentWithMeta};

/// Python wrapper for a Wyrd Agent holder.
#[pyclass(
    module = "wyrd._wyrd.cards",
    name = "_AgentWithMetaInner",
    skip_from_py_object
)]
#[derive(Clone)]
pub struct PyAgentWithMetaInner {
    inner: AgentWithMeta,
    providers: Arc<ProviderRegistry>,
}

#[pymethods]
impl PyAgentWithMetaInner {
    /// Load an Agent holder from a YAML path.
    #[staticmethod]
    pub fn from_yaml(path: &str) -> CardPyResult<Self> {
        AgentWithMeta::from_yaml_path(path)
            .map(|inner| Self {
                inner,
                providers: skald_runtime::default_registry(),
            })
            .map_err(Into::into)
    }

    /// Load an Agent holder from a YAML path.
    #[staticmethod]
    pub fn from_yaml_path(path: &str) -> CardPyResult<Self> {
        Self::from_yaml(path)
    }

    /// Load an Agent holder from a YAML string.
    #[staticmethod]
    pub fn from_yaml_str(input: &str) -> CardPyResult<Self> {
        AgentWithMeta::from_yaml_str(input)
            .map(|inner| Self {
                inner,
                providers: skald_runtime::default_registry(),
            })
            .map_err(Into::into)
    }

    /// Rebuild an Agent holder from serialized card JSON.
    #[staticmethod]
    pub fn model_validate_json(data: &str) -> CardPyResult<Self> {
        let card = serde_json::from_str(data)
            .map_err(|error| WyrdPyError::validation(error.to_string()))?;
        AgentWithMeta::from_card(card, skald_tool::default_registry())
            .map(|inner| Self {
                inner,
                providers: skald_runtime::default_registry(),
            })
            .map_err(Into::into)
    }

    /// Save this Agent holder to a YAML path.
    pub fn save(&self, path: &str) -> CardPyResult<()> {
        self.inner.save(path).map_err(Into::into)
    }

    /// Convert this Agent holder to a YAML string.
    pub fn to_yaml_string(&self) -> CardPyResult<String> {
        self.inner.to_yaml_string().map_err(Into::into)
    }

    /// Convert this Agent holder to card-envelope JSON.
    pub fn model_dump_json(&self) -> CardPyResult<String> {
        Ok(serde_json::to_string(&self.inner.to_card()?)?)
    }

    /// Validate whether this Agent holder is registrable.
    pub fn validate_registrable(&self) -> CardPyResult<()> {
        self.inner.validate_registrable().map_err(Into::into)
    }

    /// Run this agent through its scoped provider registry.
    pub fn run(
        &self,
        py: Python<'_>,
        input: Py<PyAny>,
        session_id: Option<String>,
    ) -> PyResult<Py<PyAny>> {
        let providers = self.providers.clone();
        let agent = self.inner.agent.clone();
        let session_id = session_id.map(SessionId::new);

        if let Ok(input) = input.bind(py).extract::<String>() {
            let run = py.detach(|| {
                wyrd_runtime::runtime().block_on(async move {
                    agent
                        .run(providers.as_ref(), session_id, input.as_str())
                        .await
                })
            });
            return match run {
                Ok(run) => skald_agent::python::agent_run_to_py(py, run),
                Err(error) => Err(agent_error_to_py_err(py, error)),
            };
        }

        let vars_json = wyrd_utils::py::pyobject_to_json(input.bind(py))?;
        let vars = vars_from_json(vars_json).map_err(|error| {
            PyTypeError::new_err(format!("agent input must be str or dict: {error}"))
        })?;
        let prompt = self.inner.agent.prompt.clone();
        let run = py.detach(|| {
            let borrowed = vars
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str()))
                .collect::<Vec<_>>();
            wyrd_runtime::runtime().block_on(async move {
                agent
                    .run_prompt(providers.as_ref(), prompt.as_ref(), &borrowed)
                    .await
            })
        });
        match run {
            Ok(run) => skald_agent::python::agent_run_to_py(py, run),
            Err(error) => Err(agent_error_to_py_err(py, error)),
        }
    }

    /// Append one runtime-local tool.
    pub fn add_tool(&mut self, py: Python<'_>, tool: Py<PyAny>) -> PyResult<()> {
        let tool = skald_tool::python::wrap_callable(py, tool)?;
        self.inner.add_tool_in_place(tool);
        Ok(())
    }

    /// Replace all runtime-local tools.
    pub fn set_tools(&mut self, py: Python<'_>, tools: Vec<Py<PyAny>>) -> PyResult<()> {
        let mut out = Vec::with_capacity(tools.len());
        for tool in tools {
            out.push(skald_tool::python::wrap_callable(py, tool)?);
        }
        self.inner.set_tools_in_place(out);
        Ok(())
    }

    /// Set an inline prompt.
    pub fn with_prompt(&mut self, py: Python<'_>, prompt: Py<PyAny>) -> PyResult<()> {
        self.inner
            .with_prompt_in_place(prompt_from_py(py, &prompt)?);
        Ok(())
    }

    /// Set a session backend.
    pub fn with_session(&mut self, py: Python<'_>, session: Py<PyAny>) -> PyResult<()> {
        self.inner
            .set_session(skald_agent::python::wrap_session(py, session)?);
        Ok(())
    }

    /// Set run configuration.
    pub fn with_run_config(&mut self, py: Python<'_>, run_config: Py<PyAny>) -> PyResult<()> {
        self.inner
            .set_run_config(run_config_from_py(py, run_config)?);
        Ok(())
    }

    /// Return this agent as a runtime-local tool.
    pub fn as_tool(&self, py: Python<'_>, description: Option<String>) -> PyResult<Py<PyAny>> {
        let adapter = match description {
            Some(description) => {
                AgentDelegateTool::new(self.inner.agent_arc(), self.providers.clone())
                    .with_description(description)
                    .into_tool()
            }
            None => AgentDelegateTool::from_agent(self.inner.agent_arc(), self.providers.clone()),
        };
        skald_tool::python::tool_callable_py(py, adapter)
    }

    /// Add a before-agent callback.
    pub fn add_before_agent(&mut self, py: Python<'_>, callback: Py<PyAny>) -> PyResult<()> {
        self.inner
            .add_before_agent_in_place(skald_agent::python::wrap_before_agent(py, callback)?);
        Ok(())
    }

    /// Add an after-agent callback.
    pub fn add_after_agent(&mut self, py: Python<'_>, callback: Py<PyAny>) -> PyResult<()> {
        self.inner
            .add_after_agent_in_place(skald_agent::python::wrap_after_agent(py, callback)?);
        Ok(())
    }

    /// Add a before-model callback.
    pub fn add_before_model(&mut self, py: Python<'_>, callback: Py<PyAny>) -> PyResult<()> {
        self.inner
            .add_before_model_in_place(skald_agent::python::wrap_before_model(py, callback)?);
        Ok(())
    }

    /// Add an after-model callback.
    pub fn add_after_model(&mut self, py: Python<'_>, callback: Py<PyAny>) -> PyResult<()> {
        self.inner
            .add_after_model_in_place(skald_agent::python::wrap_after_model(py, callback)?);
        Ok(())
    }

    /// Add a before-tool callback.
    pub fn add_before_tool(&mut self, py: Python<'_>, callback: Py<PyAny>) -> PyResult<()> {
        self.inner
            .add_before_tool_in_place(skald_agent::python::wrap_before_tool(py, callback)?);
        Ok(())
    }

    /// Add an after-tool callback.
    pub fn add_after_tool(&mut self, py: Python<'_>, callback: Py<PyAny>) -> PyResult<()> {
        self.inner
            .add_after_tool_in_place(skald_agent::python::wrap_after_tool(py, callback)?);
        Ok(())
    }

    /// Optional card name.
    #[getter]
    pub fn name(&self) -> Option<String> {
        self.inner.name().map(str::to_owned)
    }

    /// Optional card version.
    #[getter]
    pub fn version(&self) -> Option<String> {
        self.inner.version().map(str::to_owned)
    }

    /// Optional card space.
    #[getter]
    pub fn space(&self) -> Option<String> {
        self.inner.space().map(str::to_owned)
    }

    /// Runtime id.
    #[getter]
    pub fn id(&self) -> String {
        self.inner.agent.id.clone()
    }

    /// Inline prompt object.
    #[getter]
    pub fn prompt(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(Py::new(py, self.inner.agent.prompt.as_ref().clone())?.into_any())
    }

    /// Runtime-local tool names.
    #[getter]
    pub fn tool_names(&self) -> Vec<String> {
        self.inner.tool_names().to_vec()
    }

    fn __repr__(&self) -> String {
        format!(
            "_AgentWithMetaInner(id={:?}, name={:?}, version={:?}, tools={:?})",
            self.inner.agent.id,
            self.inner.name(),
            self.inner.version(),
            self.inner.tool_names()
        )
    }
}

/// Python wrapper for a Wyrd Agent builder.
#[pyclass(module = "wyrd._wyrd.cards", name = "_AgentBuilderInner")]
#[derive(Default)]
pub struct PyAgentBuilderInner {
    inner: Option<AgentBuilder>,
    providers: Option<Arc<ProviderRegistry>>,
}

#[pymethods]
impl PyAgentBuilderInner {
    /// Construct an empty Agent builder.
    #[new]
    pub fn __new__() -> Self {
        Self {
            inner: Some(AgentBuilder::default()),
            providers: None,
        }
    }

    /// Set the runtime id.
    pub fn id(&mut self, id: &str) -> CardPyResult<()> {
        let builder = self.take()?;
        self.inner = Some(builder.id(id));
        Ok(())
    }

    /// Set the card name.
    pub fn name(&mut self, name: &str) -> CardPyResult<()> {
        let builder = self.take()?;
        self.inner = Some(builder.name(name));
        Ok(())
    }

    /// Set the card version.
    pub fn version(&mut self, version: &str) -> CardPyResult<()> {
        let builder = self.take()?;
        self.inner = Some(builder.version(version));
        Ok(())
    }

    /// Set the card space.
    pub fn space(&mut self, space: &str) -> CardPyResult<()> {
        let builder = self.take()?;
        self.inner = Some(builder.space(space));
        Ok(())
    }

    /// Set the prompt.
    pub fn prompt(&mut self, py: Python<'_>, prompt: Py<PyAny>) -> CardPyResult<()> {
        let builder = self.take()?;
        self.inner = Some(builder.prompt(wyrd_spec::reference::PromptRef::from(
            prompt_from_py(py, &prompt)?.into_native(),
        )));
        Ok(())
    }

    /// Append a runtime-local tool.
    pub fn tool(&mut self, py: Python<'_>, tool: Py<PyAny>) -> PyResult<()> {
        let builder = self
            .inner
            .take()
            .ok_or_else(|| PyTypeError::new_err("AgentBuilder has already been consumed"))?;
        self.inner = Some(builder.tool(skald_tool::python::wrap_callable(py, tool)?));
        Ok(())
    }

    /// Set run configuration.
    pub fn run_config(&mut self, py: Python<'_>, run_config: Py<PyAny>) -> PyResult<()> {
        let builder = self
            .inner
            .take()
            .ok_or_else(|| PyTypeError::new_err("AgentBuilder has already been consumed"))?;
        self.inner = Some(builder.run_config(run_config_from_py(py, run_config)?));
        Ok(())
    }

    /// Set providers.
    pub fn providers(&mut self, py: Python<'_>, providers: Py<PyAny>) -> PyResult<()> {
        self.providers = Some(provider_registry_from_py(py, providers)?);
        Ok(())
    }

    /// Set a session backend.
    pub fn session(&mut self, py: Python<'_>, session: Py<PyAny>) -> PyResult<()> {
        let builder = self
            .inner
            .take()
            .ok_or_else(|| PyTypeError::new_err("AgentBuilder has already been consumed"))?;
        self.inner = Some(builder.session(skald_agent::python::wrap_session(py, session)?));
        Ok(())
    }

    /// Add a before-agent callback.
    pub fn before_agent(&mut self, py: Python<'_>, callback: Py<PyAny>) -> PyResult<()> {
        self.update_py(|builder| {
            Ok(builder.before_agent(skald_agent::python::wrap_before_agent(py, callback)?))
        })
    }

    /// Add an after-agent callback.
    pub fn after_agent(&mut self, py: Python<'_>, callback: Py<PyAny>) -> PyResult<()> {
        self.update_py(|builder| {
            Ok(builder.after_agent(skald_agent::python::wrap_after_agent(py, callback)?))
        })
    }

    /// Add a before-model callback.
    pub fn before_model(&mut self, py: Python<'_>, callback: Py<PyAny>) -> PyResult<()> {
        self.update_py(|builder| {
            Ok(builder.before_model(skald_agent::python::wrap_before_model(py, callback)?))
        })
    }

    /// Add an after-model callback.
    pub fn after_model(&mut self, py: Python<'_>, callback: Py<PyAny>) -> PyResult<()> {
        self.update_py(|builder| {
            Ok(builder.after_model(skald_agent::python::wrap_after_model(py, callback)?))
        })
    }

    /// Add a before-tool callback.
    pub fn before_tool(&mut self, py: Python<'_>, callback: Py<PyAny>) -> PyResult<()> {
        self.update_py(|builder| {
            Ok(builder.before_tool(skald_agent::python::wrap_before_tool(py, callback)?))
        })
    }

    /// Add an after-tool callback.
    pub fn after_tool(&mut self, py: Python<'_>, callback: Py<PyAny>) -> PyResult<()> {
        self.update_py(|builder| {
            Ok(builder.after_tool(skald_agent::python::wrap_after_tool(py, callback)?))
        })
    }

    /// Build the Agent holder.
    pub fn build(&mut self) -> CardPyResult<PyAgentWithMetaInner> {
        let builder = self.take()?;
        builder
            .build()
            .map(|inner| PyAgentWithMetaInner {
                inner,
                providers: self
                    .providers
                    .clone()
                    .unwrap_or_else(skald_runtime::default_registry),
            })
            .map_err(Into::into)
    }
}

impl PyAgentBuilderInner {
    fn take(&mut self) -> CardPyResult<AgentBuilder> {
        self.inner
            .take()
            .ok_or_else(|| WyrdPyError::validation("AgentBuilder has already been consumed"))
    }

    fn update_py(
        &mut self,
        f: impl FnOnce(AgentBuilder) -> PyResult<AgentBuilder>,
    ) -> PyResult<()> {
        let builder = self
            .inner
            .take()
            .ok_or_else(|| PyTypeError::new_err("AgentBuilder has already been consumed"))?;
        self.inner = Some(f(builder)?);
        Ok(())
    }
}

/// Register Agent Card Python classes.
///
/// # Errors
/// Returns `PyO3` module registration errors.
pub fn python_register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    wyrd_interfaces::error::register_exceptions(module)?;
    module.add_class::<PyAgentWithMetaInner>()?;
    module.add_class::<PyAgentBuilderInner>()?;
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
    let mut config = RunConfig::default();
    if value.bind(py).is_none() {
        return Ok(config);
    }
    let json = wyrd_utils::py::pyobject_to_json(value.bind(py))?;
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

fn vars_from_json(value: serde_json::Value) -> Result<Vec<(String, String)>, String> {
    let serde_json::Value::Object(map) = value else {
        return Err("expected a mapping of prompt variables".to_owned());
    };
    Ok(map
        .into_iter()
        .map(|(key, value)| {
            let value = value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value.to_string());
            (key, value)
        })
        .collect())
}

fn agent_error_to_py_err(py: Python<'_>, error: skald_agent::AgentError) -> PyErr {
    let detail = error.to_string();
    match py
        .get_type::<wyrd_utils::py::WyrdError>()
        .call1((detail.clone(),))
    {
        Ok(exception) => {
            let _ = exception.setattr("code", error.code());
            let _ = exception.setattr("message", detail);
            let _ = exception.setattr("status", error.status());
            let _ = exception.setattr("http_status", error.status());
            let _ = exception.setattr("title", error.title());
            let _ = exception.setattr("remediation", error.remediation());
            PyErr::from_value(exception)
        }
        Err(source) => PyTypeError::new_err(source.to_string()),
    }
}
