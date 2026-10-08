//! `PyO3` boundary for `wyrd.agent.Workflow` and `wyrd.agent.WorkflowRun`.
//!
//! [`PyWorkflow`] wraps the shared [`ClientWorkflow`], which keeps the
//! client a Workflow was loaded through: each method converts its Python
//! arguments at the boundary and delegates to the native loader, codec, or
//! executor, which own every rule.
//! [`PyWorkflowRun`] projects the portable [`WorkflowRun`] wire snapshot.

use std::path::PathBuf;

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyModule};
use serde_json::Value;
use skald_workflow::{Workflow as SkaldWorkflow, WorkflowInput};
use wyrd_client::Workflow as ClientWorkflow;
use wyrd_spec::card::workflow::WorkflowRun;
use wyrd_spec::error::WyrdError;
use wyrd_utils::py::{WyrdPyError, WyrdPyResult};

use crate::client::PyWyrdClient;

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

/// Python `wyrd.agent.Workflow`: a YAML-authored Workflow and its local run
/// surface over the shared [`ClientWorkflow`].
///
/// Workflows are authored as YAML and loaded with `from_path` or `from_yaml`;
/// there is no Python builder. A Workflow loaded through a client keeps it, so
/// its runs reach the same server.
#[pyclass(module = "wyrd.agent", name = "Workflow")]
pub struct PyWorkflow {
    /// Shared Workflow every method delegates to.
    inner: ClientWorkflow,
}

#[pymethods]
impl PyWorkflow {
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

    /// Load an authored Workflow file and the Cards it references.
    ///
    /// Relative paths and loaded sibling Agents and Prompts resolve locally
    /// through the shared loader; a wholly local file needs no server or
    /// credentials. External Card refs are read exactly as `client`, or,
    /// when it is omitted, through the ambient Wyrd client configuration; a
    /// given `client` also makes the Workflow's gateway calls. The GIL is
    /// released while the shared Wyrd runtime drives loading.
    ///
    /// Loading only reads files and Cards; it registers and runs nothing. It
    /// blocks until loading finishes, and a failure after some reads returns
    /// no partial Workflow and writes nothing durable.
    ///
    /// Args:
    ///     path (str | os.PathLike[str]): Workflow entry file.
    ///     client (WyrdClient | None): Principal that reads registry refs and
    ///         calls the gateway; the ambient configuration when omitted.
    ///
    /// Returns:
    ///     Workflow: Fully hydrated and validated workflow.
    ///
    /// Raises:
    ///     `WyrdError`: `WYRD_LOADER_400_INVALID_ENVELOPE` when the file
    ///         fails to load; `WYRD_CLIENT_401_NO_CREDENTIALS` when a
    ///         registry ref needs a credential and none is configured;
    ///         `WYRD_PERMISSION_403_DENIED_RBAC` when the credential cannot
    ///         read Cards; `WYRD_REGISTRY_404_CARD_NOT_FOUND` when a referenced
    ///         Card is missing or deleted; and the Workflow validation error
    ///         when the loaded graph is invalid.
    ///
    /// # Errors
    ///
    /// Returns the error of the shared [`ClientWorkflow::from_path`] or
    /// [`ClientWorkflow::from_path_with_client`], raised in Python as the
    /// `WyrdError` with the codes listed above.
    #[staticmethod]
    #[pyo3(signature = (path, client=None))]
    fn from_path(
        py: Python<'_>,
        path: PathBuf,
        client: Option<&Bound<'_, PyWyrdClient>>,
    ) -> WyrdPyResult<Self> {
        let client = client.map(|client| client.get().inner().clone());
        let workflow = py.detach(|| {
            wyrd_runtime::runtime().block_on(async move {
                match client {
                    Some(client) => ClientWorkflow::from_path_with_client(path, client).await,
                    None => ClientWorkflow::from_path(path).await,
                }
            })
        })?;
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
