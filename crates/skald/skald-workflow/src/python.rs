//! Python boundary for the `wyrd.agent.Workflow` pyclass.

#![cfg(feature = "python")]

use std::collections::HashMap;
use std::path::PathBuf;

use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyModule, PyString};
use skald_agent::Agent;
use wyrd_spec::error::WyrdError;
use wyrd_spec::metadata::{AnnotationKey, AnnotationValue, LabelKey, LabelValue, Labels};

use crate::error::WorkflowError;
use crate::run::{TaskEvent, TaskOutcome, WorkflowRun};
use crate::task::TaskStatus;
use crate::workflow_surface::Workflow;

fn workflow_error_to_py(error: WorkflowError) -> PyErr {
    let wyrd: WyrdError = match error {
        WorkflowError::Spec(spec) => spec.into(),
        WorkflowError::Other(message) => WyrdError::Validation {
            message,
            details: serde_json::json!({}),
        },
        other => WyrdError::Internal {
            message: format!("{other}"),
            details: serde_json::json!({ "code": other.code() }),
        },
    };
    wyrd_utils::py::wyrd_error_to_py_err(wyrd)
}

fn wyrd_error_to_py(error: WyrdError) -> PyErr {
    wyrd_utils::py::wyrd_error_to_py_err(error)
}

fn coerce_labels(labels: Option<HashMap<String, String>>) -> Result<Labels, PyErr> {
    let mut out = Labels::default();
    if let Some(labels) = labels {
        for (key, value) in labels {
            let key = LabelKey::new(&key)
                .map_err(|error| PyTypeError::new_err(format!("invalid label key: {error}")))?;
            let value = LabelValue::new(&value)
                .map_err(|error| PyTypeError::new_err(format!("invalid label value: {error}")))?;
            out.insert(key, value);
        }
    }
    Ok(out)
}

fn coerce_annotations(
    annotations: Option<HashMap<String, String>>,
) -> Result<wyrd_spec::metadata::Annotations, PyErr> {
    let mut out = wyrd_spec::metadata::Annotations::default();
    if let Some(annotations) = annotations {
        for (key, value) in annotations {
            let key = AnnotationKey::new(&key).map_err(|error| {
                PyTypeError::new_err(format!("invalid annotation key: {error}"))
            })?;
            let value = AnnotationValue::new(&value).map_err(|error| {
                PyTypeError::new_err(format!("invalid annotation value: {error}"))
            })?;
            out.insert(key, value);
        }
    }
    Ok(out)
}

fn extract_agents(py: Python<'_>, agents: Vec<Py<PyAny>>) -> PyResult<Vec<Agent>> {
    let mut out = Vec::with_capacity(agents.len());
    for value in agents {
        let bound = value.bind(py);
        let py_agent: Py<Agent> = bound.extract().map_err(|_| {
            PyTypeError::new_err("expected an Agent value; pass Agent objects positionally")
        })?;
        out.push(py_agent.borrow(py).clone());
    }
    Ok(out)
}

fn coerce_after<'py>(py: Python<'py>, after: &Bound<'py, PyAny>) -> PyResult<Vec<String>> {
    if let Ok(text) = after.cast::<PyString>() {
        return Ok(vec![text.to_str()?.to_owned()]);
    }
    if let Ok(py_agent) = after.extract::<Py<Agent>>() {
        return Ok(vec![step_id_from_agent(&py_agent.borrow(py))]);
    }
    if let Ok(list) = after.cast::<PyList>() {
        let mut out = Vec::with_capacity(list.len());
        for item in list.iter() {
            if let Ok(text) = item.cast::<PyString>() {
                out.push(text.to_str()?.to_owned());
            } else if let Ok(py_agent) = item.extract::<Py<Agent>>() {
                out.push(step_id_from_agent(&py_agent.borrow(py)));
            } else {
                return Err(PyTypeError::new_err(
                    "after entries must be Agent or str values",
                ));
            }
        }
        return Ok(out);
    }
    Err(PyTypeError::new_err(
        "after must be Agent, str, or a sequence of Agent | str",
    ))
}

fn step_id_from_agent(agent: &Agent) -> String {
    agent
        .name_str()
        .map(str::to_owned)
        .unwrap_or_else(|| agent.id.clone())
}

#[pymethods]
impl Workflow {
    /// Build an empty Workflow with the given name and optional metadata.
    ///
    /// Args:
    ///     name (str): Workflow name.
    ///     version (str | None): Optional semantic version.
    ///     space (str | None): Optional logical space.
    ///     labels (dict[str, str] | None): Optional queryable labels.
    ///     annotations (dict[str, str] | None): Optional free-form annotations.
    ///
    /// Returns:
    ///     Workflow: New, empty workflow.
    #[new]
    #[pyo3(signature = (
        *,
        name,
        version = None,
        space = None,
        labels = None,
        annotations = None,
    ))]
    pub fn __new__(
        name: String,
        version: Option<String>,
        space: Option<String>,
        labels: Option<HashMap<String, String>>,
        annotations: Option<HashMap<String, String>>,
    ) -> PyResult<Self> {
        let labels = coerce_labels(labels)?;
        let annotations = coerce_annotations(annotations)?;
        let mut wf = Workflow::new(name);
        if let Some(version) = version {
            wf = wf.with_version(version);
        }
        if let Some(space) = space {
            wf = wf.with_space(space);
        }
        wf.meta.labels = labels;
        wf.meta.annotations = annotations;
        Ok(wf)
    }

    /// Build a workflow whose steps run sequentially.
    ///
    /// Args:
    ///     name (str): Workflow name.
    ///     *agents (Agent): One or more Agent values to chain.
    ///
    /// Returns:
    ///     Workflow: Workflow with each agent depending on the previous one.
    ///
    /// Raises:
    ///     WyrdError: When the resulting DAG is invalid.
    #[staticmethod]
    #[pyo3(name = "sequential", signature = (name, *agents))]
    pub fn py_sequential(py: Python<'_>, name: String, agents: Vec<Py<PyAny>>) -> PyResult<Self> {
        let agents = extract_agents(py, agents)?;
        Workflow::sequential(name, agents).map_err(workflow_error_to_py)
    }

    /// Build a workflow whose steps run in parallel with no dependencies.
    ///
    /// Args:
    ///     name (str): Workflow name.
    ///     *agents (Agent): One or more Agent values to run in parallel.
    ///
    /// Returns:
    ///     Workflow: Workflow with each agent as an independent root step.
    ///
    /// Raises:
    ///     WyrdError: When the resulting DAG is invalid.
    #[staticmethod]
    #[pyo3(name = "parallel", signature = (name, *agents))]
    pub fn py_parallel(py: Python<'_>, name: String, agents: Vec<Py<PyAny>>) -> PyResult<Self> {
        let agents = extract_agents(py, agents)?;
        Workflow::parallel(name, agents).map_err(workflow_error_to_py)
    }

    /// Append `agent` as a new step with no dependencies.
    ///
    /// Args:
    ///     agent (Agent): Agent to append.
    ///
    /// Returns:
    ///     Workflow: This workflow (for chaining).
    ///
    /// Raises:
    ///     WyrdError: When the resulting DAG is invalid.
    #[pyo3(name = "add")]
    pub fn py_add<'py>(
        mut slf: PyRefMut<'py, Self>,
        py: Python<'py>,
        agent: Py<Agent>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let agent = agent.borrow(py).clone();
        let next = slf.clone().add(agent).map_err(workflow_error_to_py)?;
        *slf = next;
        Ok(slf)
    }

    /// Append `agent` as a new step depending on the supplied predecessors.
    ///
    /// Args:
    ///     agent (Agent): Agent to append.
    ///     after (Agent | str | list[Agent | str]): Predecessor step ids or
    ///         Agent values (their names are used as ids).
    ///
    /// Returns:
    ///     Workflow: This workflow (for chaining).
    ///
    /// Raises:
    ///     WyrdError: When the resulting DAG is invalid.
    #[pyo3(name = "add_after", signature = (agent, after))]
    pub fn py_add_after<'py>(
        mut slf: PyRefMut<'py, Self>,
        py: Python<'py>,
        agent: Py<Agent>,
        after: &Bound<'py, PyAny>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let agent = agent.borrow(py).clone();
        let deps = coerce_after(py, after)?;
        let next = slf
            .clone()
            .add_after(agent, deps)
            .map_err(workflow_error_to_py)?;
        *slf = next;
        Ok(slf)
    }

    /// Set the workflow's semantic version in place.
    #[pyo3(name = "set_version")]
    pub fn py_set_version(&mut self, version: String) {
        self.meta.version = Some(version);
    }

    /// Set the workflow's space in place.
    #[pyo3(name = "set_space")]
    pub fn py_set_space(&mut self, space: String) {
        self.meta.space = Some(space);
    }

    /// Return the workflow name.
    #[getter]
    pub fn name(&self) -> Option<&str> {
        self.name_str()
    }

    /// Return the workflow version.
    #[getter]
    pub fn version(&self) -> Option<&str> {
        self.version_str()
    }

    /// Return the workflow space.
    #[getter]
    pub fn space(&self) -> Option<&str> {
        self.space_str()
    }

    /// Return the ordered step ids.
    #[getter]
    pub fn steps(&self) -> Vec<String> {
        self.step_ids()
    }

    /// Serialize this workflow to a canonical envelope YAML string.
    ///
    /// Raises:
    ///     WyrdError: When identity or codec fails.
    #[pyo3(name = "to_yaml")]
    pub fn py_to_yaml(&self) -> PyResult<String> {
        self.to_yaml_string().map_err(wyrd_error_to_py)
    }

    /// Save this workflow to disk as canonical envelope YAML.
    ///
    /// Args:
    ///     path (str): Filesystem path.
    ///
    /// Raises:
    ///     WyrdError: When identity, IO, or codec fails.
    #[pyo3(name = "save")]
    pub fn py_save(&self, path: PathBuf) -> PyResult<()> {
        self.save(path).map_err(wyrd_error_to_py)
    }

    /// Load a workflow from disk.
    ///
    /// Args:
    ///     path (str): Filesystem path.
    ///
    /// Returns:
    ///     Workflow: Reconstructed workflow with eager inline agent resolution.
    ///
    /// Raises:
    ///     WyrdError: When IO, codec, or resolution fails.
    #[staticmethod]
    #[pyo3(name = "load")]
    pub fn py_load(path: PathBuf) -> PyResult<Self> {
        let tool_resolver = skald_tool::default_registry();
        let prompt_resolver = skald_agent::default_prompt_resolver();
        Workflow::load(path, tool_resolver, prompt_resolver).map_err(wyrd_error_to_py)
    }

    /// Parse a workflow from a canonical envelope YAML string.
    ///
    /// Args:
    ///     yaml (str): Envelope YAML body.
    ///
    /// Returns:
    ///     Workflow: Reconstructed workflow with eager inline agent resolution.
    ///
    /// Raises:
    ///     WyrdError: When parse or resolution fails.
    #[staticmethod]
    #[pyo3(name = "from_yaml")]
    pub fn py_from_yaml(yaml: String) -> PyResult<Self> {
        let tool_resolver = skald_tool::default_registry();
        let prompt_resolver = skald_agent::default_prompt_resolver();
        Workflow::from_yaml_str(&yaml, tool_resolver, prompt_resolver).map_err(wyrd_error_to_py)
    }

    /// Run this workflow against the process-local provider registry.
    ///
    /// Args:
    ///     input (str): User-facing input string forwarded to every step.
    ///
    /// Returns:
    ///     WorkflowRun: Final run envelope with per-step outcomes and events.
    ///
    /// Raises:
    ///     WyrdError: When a provider call fails, retries exhaust, or any
    ///         step's output validation fails.
    #[pyo3(name = "run", signature = (input))]
    pub fn py_run(&self, py: Python<'_>, input: String) -> PyResult<Py<PyAny>> {
        let providers = skald_runtime::default_registry();
        let run = py.detach(|| {
            wyrd_runtime::runtime().block_on(Workflow::run(self, providers.as_ref(), &input))
        });
        let run = run.map_err(workflow_error_to_py)?;
        Ok(Py::new(py, run)?.into_any())
    }
}

#[pymethods]
impl WorkflowRun {
    /// Return the final step's outcome, when the workflow produced one.
    #[getter]
    pub fn final_step_id(&self) -> Option<&str> {
        self.last_task_id.as_deref()
    }

    /// Return per-step outcomes as a `dict[str, StepOutcome]`.
    #[getter]
    pub fn outcomes(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        for (id, outcome) in &self.tasks {
            dict.set_item(id, Py::new(py, outcome.clone())?)?;
        }
        Ok(dict.into())
    }

    /// Return per-step events as a `list[StepEvent]`.
    #[getter]
    pub fn events(&self, py: Python<'_>) -> PyResult<Py<PyList>> {
        let mut items: Vec<Py<TaskEvent>> = Vec::with_capacity(self.events.len());
        for event in &self.events {
            items.push(Py::new(py, event.clone())?);
        }
        Ok(PyList::new(py, items)?.into())
    }
}

#[pymethods]
impl TaskOutcome {
    /// Return the final step status.
    #[getter]
    pub fn status(&self) -> TaskStatus {
        self.status
    }

    /// Return the number of retries consumed before reaching the final status.
    #[getter]
    pub fn retries(&self) -> u32 {
        self.retries
    }
}

#[pymethods]
impl TaskEvent {
    /// Step id this event refers to.
    #[getter]
    pub fn step_id(&self) -> &str {
        &self.task_id
    }

    /// Status recorded for this transition.
    #[getter]
    pub fn status(&self) -> TaskStatus {
        self.status
    }

    /// Unix epoch milliseconds when the attempt started.
    #[getter]
    pub fn started_at(&self) -> i64 {
        self.started_at
    }

    /// Unix epoch milliseconds when the attempt ended.
    #[getter]
    pub fn ended_at(&self) -> i64 {
        self.ended_at
    }

    /// Attempt index, starting at 1.
    #[getter]
    pub fn attempt(&self) -> u32 {
        self.attempt
    }

    /// Stable error code for failed attempts, or `None` for completed ones.
    #[getter]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

/// Register Workflow + WorkflowRun + StepOutcome + StepEvent + StepStatus on
/// the supplied PyO3 module (intended to be the existing `wyrd.agent`
/// submodule registered by `skald_agent::python_register`).
///
/// # Errors
/// Returns `PyErr` when registration on the supplied module fails.
pub fn python_register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<Workflow>()?;
    module.add_class::<WorkflowRun>()?;
    module.add_class::<TaskOutcome>()?;
    module.add_class::<TaskEvent>()?;
    module.add_class::<TaskStatus>()?;
    Ok(())
}
