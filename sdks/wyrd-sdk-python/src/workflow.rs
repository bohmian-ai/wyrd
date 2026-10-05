//! `PyO3` boundary for `wyrd.agent.Workflow` and `wyrd.agent.WorkflowRun`.
//!
//! [`PyWorkflow`] wraps the shared [`ClientWorkflow`], which keeps the
//! client a Workflow was loaded through: each method converts its Python
//! arguments at the boundary and delegates to the native builder, validator,
//! codec, or executor, which own every rule.
//! [`PyWorkflowRun`] projects the portable [`WorkflowRun`] wire snapshot.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyList, PyModule, PyString};
use serde_json::Value;
use skald_agent::Agent;
use skald_workflow::{Workflow as SkaldWorkflow, WorkflowInput, step_id_for_name};
use wyrd_client::Workflow as ClientWorkflow;
use wyrd_spec::card::common::ParameterValue;
use wyrd_spec::card::workflow::{WorkflowBinding, WorkflowRun};
use wyrd_spec::error::WyrdError;
use wyrd_spec::metadata::{
    AnnotationKey, AnnotationValue, Annotations, LabelKey, LabelValue, Labels,
};
use wyrd_utils::py::{WyrdPyError, WyrdPyResult};

/// Reject a caller-supplied argument the workflow surface cannot accept.
///
/// Raises `WYRD_WORKFLOW_422_VALIDATION` naming `name` in `details.argument`.
fn invalid_argument(name: &str, detail: impl std::fmt::Display) -> WyrdPyError {
    WyrdPyError::from(WyrdError::WorkflowValidation {
        message: format!("{name} is invalid: {detail}"),
        details: serde_json::json!({ "argument": name, "reason": detail.to_string() }),
    })
}

/// Convert a Python run input: a string is the declared `input` shorthand,
/// `None` is an empty object, and a mapping is the JSON input object.
///
/// # Errors
///
/// Returns `WYRD_WORKFLOW_422_VALIDATION` for any other JSON shape, or the
/// shared conversion error when the value is not JSON-compatible.
fn workflow_input_from_py(value: Option<&Bound<'_, PyAny>>) -> WyrdPyResult<WorkflowInput> {
    let Some(value) = value else {
        return Ok(WorkflowInput::from(serde_json::Map::new()));
    };
    if let Ok(text) = value.extract::<String>() {
        return Ok(WorkflowInput::Text(text));
    }
    match wyrd_utils::py::pyobject_to_json(value)? {
        Value::Object(map) => Ok(WorkflowInput::from(map)),
        _ => Err(invalid_argument("input", "must be a str or a mapping")),
    }
}

/// Convert a Python default value to its native Workflow parameter variant:
/// `bool`, `int`, `float`, and `str` map to their scalar variants and any
/// other JSON value is a `json` parameter.
///
/// # Errors
///
/// Returns the shared conversion error when the value is not JSON-compatible.
fn parameter_from_py(value: &Bound<'_, PyAny>) -> WyrdPyResult<ParameterValue> {
    Ok(match wyrd_utils::py::pyobject_to_json(value)? {
        Value::Bool(value) => ParameterValue::Bool(value),
        Value::Number(number) => match number.as_i64() {
            Some(value) => ParameterValue::Int(value),
            None => number.as_f64().map_or(
                ParameterValue::Json(Value::Number(number)),
                ParameterValue::Float,
            ),
        },
        Value::String(value) => ParameterValue::Str(value),
        other => ParameterValue::Json(other),
    })
}

/// Parse a `name -> source` mapping into Workflow bindings.
///
/// # Errors
///
/// Returns `WYRD_WORKFLOW_422_VALIDATION` naming `argument` when a source is
/// not a valid binding path.
fn bindings_from_py(
    argument: &str,
    bindings: HashMap<String, String>,
) -> WyrdPyResult<BTreeMap<String, WorkflowBinding>> {
    bindings
        .into_iter()
        .map(|(name, source)| {
            WorkflowBinding::new(&source)
                .map(|binding| (name, binding))
                .map_err(|error| invalid_argument(argument, error))
        })
        .collect()
}

/// Convert optional Python labels into validated Card labels.
///
/// # Errors
///
/// Returns `WYRD_WORKFLOW_422_VALIDATION` naming `labels` for an invalid key
/// or value.
fn coerce_labels(labels: Option<HashMap<String, String>>) -> WyrdPyResult<Labels> {
    let mut out = Labels::default();
    if let Some(labels) = labels {
        for (key, value) in labels {
            let key = LabelKey::new(&key).map_err(|error| invalid_argument("labels", error))?;
            let value =
                LabelValue::new(&value).map_err(|error| invalid_argument("labels", error))?;
            out.insert(key, value);
        }
    }
    Ok(out)
}

/// Convert optional Python annotations into validated Card annotations.
///
/// # Errors
///
/// Returns `WYRD_WORKFLOW_422_VALIDATION` naming `annotations` for an
/// invalid key or value.
fn coerce_annotations(annotations: Option<HashMap<String, String>>) -> WyrdPyResult<Annotations> {
    let mut out = Annotations::default();
    if let Some(annotations) = annotations {
        for (key, value) in annotations {
            let key =
                AnnotationKey::new(&key).map_err(|error| invalid_argument("annotations", error))?;
            let value = AnnotationValue::new(&value)
                .map_err(|error| invalid_argument("annotations", error))?;
            out.insert(key, value);
        }
    }
    Ok(out)
}

/// Clone the native Agents out of positional Python `Agent` values.
///
/// # Errors
///
/// Returns `WYRD_WORKFLOW_422_VALIDATION` naming `agents` for a non-Agent.
fn extract_agents(py: Python<'_>, agents: Vec<Py<PyAny>>) -> WyrdPyResult<Vec<Agent>> {
    let mut out = Vec::with_capacity(agents.len());
    for value in agents {
        let bound = value.bind(py);
        let py_agent: Py<Agent> = bound.extract().map_err(|_| {
            invalid_argument(
                "agents",
                "expected an Agent value; pass Agent objects positionally",
            )
        })?;
        out.push(py_agent.borrow(py).clone());
    }
    Ok(out)
}

/// Resolve `after` (an id, an Agent, or a list of either) into step ids.
///
/// # Errors
///
/// Returns `WYRD_WORKFLOW_422_VALIDATION` naming `after` for any other value
/// or an unnamed Agent.
fn coerce_after<'py>(py: Python<'py>, after: &Bound<'py, PyAny>) -> WyrdPyResult<Vec<String>> {
    if let Ok(text) = after.cast::<PyString>() {
        return Ok(vec![text.to_str()?.to_owned()]);
    }
    if let Ok(py_agent) = after.extract::<Py<Agent>>() {
        return Ok(vec![step_id_from_agent(&py_agent.borrow(py))?]);
    }
    if let Ok(list) = after.cast::<PyList>() {
        let mut out = Vec::with_capacity(list.len());
        for item in list.iter() {
            if let Ok(text) = item.cast::<PyString>() {
                out.push(text.to_str()?.to_owned());
            } else if let Ok(py_agent) = item.extract::<Py<Agent>>() {
                out.push(step_id_from_agent(&py_agent.borrow(py))?);
            } else {
                return Err(invalid_argument(
                    "after",
                    "entries must be Agent or str values",
                ));
            }
        }
        return Ok(out);
    }
    Err(invalid_argument(
        "after",
        "must be Agent, str, or a sequence of Agent | str",
    ))
}

/// Predecessor step ID for an Agent passed as `after`.
///
/// # Errors
///
/// Returns `WYRD_WORKFLOW_422_VALIDATION` for an unnamed Agent.
fn step_id_from_agent(agent: &Agent) -> WyrdPyResult<String> {
    agent
        .name_str()
        .map(step_id_for_name)
        .ok_or_else(|| invalid_argument("after", "an unnamed Agent has no step id; pass the id"))
}

/// Python `wyrd.agent.Workflow`: authoring and local run surface over the
/// shared [`ClientWorkflow`].
///
/// Chaining methods replace the native [`SkaldWorkflow`] with the builder's
/// result, so a failed call leaves the Python object unchanged, and keep the
/// client a loaded Workflow was read through, so its runs reach the same
/// server.
#[pyclass(module = "wyrd.agent", name = "Workflow")]
pub struct PyWorkflow {
    /// Shared Workflow every method delegates to.
    inner: ClientWorkflow,
}

#[pymethods]
impl PyWorkflow {
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
    fn __new__(
        name: String,
        version: Option<String>,
        space: Option<String>,
        labels: Option<HashMap<String, String>>,
        annotations: Option<HashMap<String, String>>,
    ) -> WyrdPyResult<Self> {
        let labels = coerce_labels(labels)?;
        let annotations = coerce_annotations(annotations)?;
        let mut wf = SkaldWorkflow::new(name);
        if let Some(version) = version {
            wf = wf.with_version(version);
        }
        if let Some(space) = space {
            wf = wf.with_space(space);
        }
        let inner = wf.with_labels(labels).with_annotations(annotations);
        Ok(Self::from(inner))
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
    ///     `WyrdError`: When the resulting DAG is invalid.
    #[staticmethod]
    #[pyo3(signature = (name, *agents))]
    fn sequential(py: Python<'_>, name: String, agents: Vec<Py<PyAny>>) -> WyrdPyResult<Self> {
        let agents = extract_agents(py, agents)?;
        Ok(Self::from(SkaldWorkflow::sequential(name, agents)?))
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
    ///     `WyrdError`: When the resulting DAG is invalid.
    #[staticmethod]
    #[pyo3(signature = (name, *agents))]
    fn parallel(py: Python<'_>, name: String, agents: Vec<Py<PyAny>>) -> WyrdPyResult<Self> {
        let agents = extract_agents(py, agents)?;
        Ok(Self::from(SkaldWorkflow::parallel(name, agents)?))
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
    ///     `WyrdError`: When the resulting DAG is invalid.
    fn add<'py>(
        mut slf: PyRefMut<'py, Self>,
        agent: &Bound<'py, Agent>,
    ) -> WyrdPyResult<PyRefMut<'py, Self>> {
        let agent = agent.borrow().clone();
        let workflow = slf.inner.as_skald_mut();
        *workflow = workflow.clone().add(agent)?;
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
    ///     `WyrdError`: When the resulting DAG is invalid.
    #[pyo3(signature = (agent, after))]
    fn add_after<'py>(
        mut slf: PyRefMut<'py, Self>,
        py: Python<'py>,
        agent: &Bound<'py, Agent>,
        after: &Bound<'py, PyAny>,
    ) -> WyrdPyResult<PyRefMut<'py, Self>> {
        let agent = agent.borrow().clone();
        let deps = coerce_after(py, after)?;
        let workflow = slf.inner.as_skald_mut();
        *workflow = workflow.clone().add_after(agent, deps)?;
        Ok(slf)
    }

    /// Declare the Workflow inputs and their defaults, replacing any
    /// previous declaration.
    ///
    /// Args:
    ///     inputs (dict[str, Any]): Input name to default value; `bool`,
    ///         `int`, `float`, and `str` declare scalar inputs and any other
    ///         JSON value declares a JSON input.
    ///
    /// Returns:
    ///     Workflow: This workflow (for chaining).
    ///
    /// Raises:
    ///     `WyrdError`: When a name is not an identifier or a value is not JSON.
    #[pyo3(signature = (inputs))]
    fn with_inputs<'py>(
        mut slf: PyRefMut<'py, Self>,
        inputs: HashMap<String, Bound<'py, PyAny>>,
    ) -> WyrdPyResult<PyRefMut<'py, Self>> {
        let inputs = inputs
            .into_iter()
            .map(|(name, value)| Ok((name, parameter_from_py(&value)?)))
            .collect::<WyrdPyResult<BTreeMap<_, _>>>()?;
        let workflow = slf.inner.as_skald_mut();
        *workflow = workflow.clone().with_inputs(inputs)?;
        Ok(slf)
    }

    /// Bind the unresolved Prompt variables of one step, replacing any
    /// previous bindings for that step.
    ///
    /// Args:
    ///     `step_id` (str): Step to bind.
    ///     inputs (dict[str, str]): Variable name to source, either
    ///         `input.<name>` or a dependency's
    ///         `steps.<id>.output.text|structured[.<field>...]`.
    ///
    /// Returns:
    ///     Workflow: This workflow (for chaining).
    ///
    /// Raises:
    ///     `WyrdError`: For an unknown step, a non-identifier name, or an
    ///         invalid source.
    #[pyo3(signature = (step_id, inputs))]
    fn with_step_inputs<'py>(
        mut slf: PyRefMut<'py, Self>,
        step_id: &str,
        inputs: HashMap<String, String>,
    ) -> WyrdPyResult<PyRefMut<'py, Self>> {
        let bindings = bindings_from_py("inputs", inputs)?;
        let workflow = slf.inner.as_skald_mut();
        *workflow = workflow.clone().with_step_inputs(step_id, bindings)?;
        Ok(slf)
    }

    /// Declare the named Workflow outputs, replacing any previous declaration.
    ///
    /// Args:
    ///     outputs (dict[str, str]): Output name to source, in the same
    ///         grammar as step inputs.
    ///
    /// Returns:
    ///     Workflow: This workflow (for chaining).
    ///
    /// Raises:
    ///     `WyrdError`: For a non-identifier name or an invalid source.
    #[pyo3(signature = (outputs))]
    fn with_outputs(
        mut slf: PyRefMut<'_, Self>,
        outputs: HashMap<String, String>,
    ) -> WyrdPyResult<PyRefMut<'_, Self>> {
        let outputs = bindings_from_py("outputs", outputs)?;
        let workflow = slf.inner.as_skald_mut();
        *workflow = workflow.clone().with_outputs(outputs)?;
        Ok(slf)
    }

    /// Validate the complete Workflow against its resolved Agents.
    ///
    /// Raises:
    ///     `WyrdError`: For any contract, binding, Prompt-variable, output, or
    ///         route error that would fail a run before dispatch.
    fn validate(&self) -> WyrdPyResult<()> {
        Ok(self.inner.as_skald().validate()?)
    }

    /// Set the workflow's semantic version in place.
    fn set_version(&mut self, version: String) {
        let workflow = self.inner.as_skald_mut();
        *workflow = workflow.clone().with_version(version);
    }

    /// Set the workflow's space in place.
    fn set_space(&mut self, space: String) {
        let workflow = self.inner.as_skald_mut();
        *workflow = workflow.clone().with_space(space);
    }

    /// Return the workflow name.
    #[getter]
    fn name(&self) -> Option<&str> {
        self.inner.as_skald().name_str()
    }

    /// Return the workflow version.
    #[getter]
    fn version(&self) -> Option<&str> {
        self.inner.as_skald().version_str()
    }

    /// Return the workflow space.
    #[getter]
    fn space(&self) -> Option<&str> {
        self.inner.as_skald().space_str()
    }

    /// Return the ordered step ids.
    #[getter]
    fn steps(&self) -> Vec<String> {
        self.inner.as_skald().step_ids()
    }

    /// Serialize this workflow to a canonical envelope YAML string.
    ///
    /// Raises:
    ///     `WyrdError`: When identity or codec fails.
    fn to_yaml(&self) -> WyrdPyResult<String> {
        Ok(self.inner.as_skald().to_yaml_string()?)
    }

    /// Save this workflow to disk as canonical envelope YAML.
    ///
    /// Args:
    ///     path (str): Filesystem path.
    ///
    /// Raises:
    ///     `WyrdError`: When identity, IO, or codec fails.
    fn save(&self, path: PathBuf) -> WyrdPyResult<()> {
        Ok(self.inner.as_skald().save(path)?)
    }

    /// Load an authored Workflow file and the Cards it references.
    ///
    /// Relative paths and loaded sibling Agents and Prompts resolve locally
    /// through the shared loader; a wholly local file needs no server or
    /// credentials. External Card refs are read exactly through the ambient
    /// Wyrd client configuration. The GIL is released while the shared Wyrd
    /// runtime drives loading.
    ///
    /// Loading only reads files and Cards; it registers and runs nothing. It
    /// blocks until loading finishes, and a failure after some reads returns
    /// no partial Workflow and writes nothing durable.
    ///
    /// Args:
    ///     path (str | os.PathLike[str]): Workflow entry file.
    ///
    /// Returns:
    ///     Workflow: Fully hydrated and validated workflow.
    ///
    /// Raises:
    ///     `WyrdError`: `WYRD_REGISTRY_400_INVALID_CARD_SPEC` when the file
    ///         fails to load; `WYRD_CLIENT_401_NO_CREDENTIALS` when a
    ///         registry ref needs a credential and none is configured;
    ///         `WYRD_PERMISSION_403_DENIED_RBAC` when the credential cannot
    ///         read Cards; `WYRD_REGISTRY_404_CARD_NOT_FOUND` when a referenced
    ///         Card is missing or deleted; and the Workflow validation error
    ///         when the loaded graph is invalid.
    ///
    /// # Errors
    ///
    /// Returns the error of the shared [`ClientWorkflow::from_path`],
    /// raised in Python as the `WyrdError` with the codes listed above.
    #[staticmethod]
    fn from_path(py: Python<'_>, path: PathBuf) -> WyrdPyResult<Self> {
        let workflow =
            py.detach(|| wyrd_runtime::runtime().block_on(ClientWorkflow::from_path(path)))?;
        Ok(Self::from(workflow))
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
    ///     `WyrdError`: When parse or resolution fails.
    #[staticmethod]
    fn from_yaml(yaml: &str) -> WyrdPyResult<Self> {
        let tool_resolver = skald_tool::default_registry();
        let prompt_resolver = skald_agent::default_prompt_resolver();
        let inner = SkaldWorkflow::from_yaml_str(yaml, tool_resolver, prompt_resolver)?;
        Ok(Self::from(inner))
    }

    /// Run this workflow, preparing only what its step routes select.
    ///
    /// Native steps use the process-local provider registry. Steps routed to
    /// the Wyrd gateway call the server and credential this Workflow was
    /// loaded through, or the ambient client configuration when it was built
    /// locally. Steps routed to an external gateway use the bindings named in
    /// the shared client configuration; only the selected bindings' secrets
    /// are read, at run start. The GIL is released while the shared Wyrd runtime drives the native
    /// executor to its terminal snapshot.
    ///
    /// Args:
    ///     input (str | dict[str, Any] | None): Workflow input. A string is
    ///         shorthand for the declared string input named `input`; a
    ///         mapping supplies declared inputs by name; `None` uses defaults.
    ///
    /// Returns:
    ///     `WorkflowRun`: Terminal run snapshot with named outputs and
    ///     namespaced step results. Step failures are recorded in it.
    ///
    /// Raises:
    ///     `WyrdError`: When validation, input, or route checks fail before any
    ///         step is dispatched.
    #[pyo3(signature = (input = None))]
    fn run(&self, py: Python<'_>, input: Option<&Bound<'_, PyAny>>) -> WyrdPyResult<PyWorkflowRun> {
        let input = workflow_input_from_py(input)?;
        let run = py.detach(|| wyrd_runtime::runtime().block_on(self.inner.run(input)))?;
        Ok(PyWorkflowRun { run })
    }
}

impl From<SkaldWorkflow> for PyWorkflow {
    /// Wrap a natively built Workflow for Python; it was loaded through no
    /// client.
    fn from(inner: SkaldWorkflow) -> Self {
        Self {
            inner: inner.into(),
        }
    }
}

impl From<ClientWorkflow> for PyWorkflow {
    /// Wrap a loaded shared Workflow for Python, keeping its loading client.
    fn from(inner: ClientWorkflow) -> Self {
        Self { inner }
    }
}

/// Python view of the portable [`WorkflowRun`] snapshot.
///
/// Values are projected from the wire JSON on access, so Python sees exactly
/// the portable contract: named `outputs`, namespaced `steps`, and the
/// bounded run `error`.
#[pyclass(
    module = "wyrd.agent",
    name = "WorkflowRun",
    frozen,
    skip_from_py_object
)]
pub struct PyWorkflowRun {
    /// Terminal run snapshot.
    run: WorkflowRun,
}

impl PyWorkflowRun {
    /// Project the wire snapshot, or one top-level field of it, to Python.
    ///
    /// # Errors
    ///
    /// Returns `WYRD_WORKFLOW_500_INTERNAL` when the snapshot does not
    /// serialize, or a Python error when conversion fails.
    fn project(&self, py: Python<'_>, field: Option<&str>) -> WyrdPyResult<Py<PyAny>> {
        let value = serde_json::to_value(&self.run).map_err(|error| {
            WyrdPyError::from(WyrdError::WorkflowInternal {
                message: format!("workflow run does not serialize: {error}"),
                details: serde_json::json!({}),
            })
        })?;
        let value = match field {
            Some(name) => &value[name],
            None => &value,
        };
        Ok(wyrd_utils::py::json_to_pyobject(py, value)?)
    }
}

#[pymethods]
impl PyWorkflowRun {
    /// Return the run identifier.
    #[getter]
    fn run_id(&self) -> String {
        self.run.run_id.to_string()
    }

    /// Return the terminal run status wire name, such as `succeeded`.
    ///
    /// # Errors
    ///
    /// Returns the errors of the snapshot projection.
    #[getter]
    fn status(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        self.project(py, Some("status"))
    }

    /// Return the named Workflow outputs; empty unless the run succeeded.
    ///
    /// # Errors
    ///
    /// Returns the errors of the snapshot projection.
    #[getter]
    fn outputs(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        self.project(py, Some("outputs"))
    }

    /// Return step results keyed by step ID.
    ///
    /// # Errors
    ///
    /// Returns the errors of the snapshot projection.
    #[getter]
    fn steps(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        self.project(py, Some("steps"))
    }

    /// Return the primary run error, or `None`.
    ///
    /// # Errors
    ///
    /// Returns the errors of the snapshot projection.
    #[getter]
    fn error(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        self.project(py, Some("error"))
    }

    /// Return the complete snapshot as its wire-shaped dictionary.
    ///
    /// # Errors
    ///
    /// Returns the errors of the snapshot projection.
    fn to_dict(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        self.project(py, None)
    }
}

/// Register `Workflow` and `WorkflowRun` on the `wyrd._wyrd.agent` submodule
/// that `skald_agent::python_register` already populated.
///
/// # Errors
///
/// Returns `PyErr` when registration on the supplied module fails.
pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyWorkflow>()?;
    module.add_class::<PyWorkflowRun>()?;
    Ok(())
}
