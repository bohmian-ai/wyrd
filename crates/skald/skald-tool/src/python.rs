//! Python boundary for runtime-local tools.

#![cfg(feature = "python")]

use std::cell::RefCell;
use std::sync::Arc;

use async_trait::async_trait;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use serde_json::Value;
use wyrd_spec::error::WyrdError;
use wyrd_utils::py::{WyrdPyError, WyrdPyResult};

use crate::{AgentTool, ToolError, ToolRegistry, default_registry};

/// Reject a caller-supplied tool declaration the registry cannot accept.
fn invalid_declaration(detail: impl std::fmt::Display) -> WyrdPyError {
    WyrdPyError::from(WyrdError::ToolInvalidSchema {
        message: detail.to_string(),
        details: serde_json::json!({ "boundary": "skald_tool_python" }),
    })
}

enum ActiveRegistry {
    Scoped(Arc<ToolRegistry>),
    Default,
}

impl ActiveRegistry {
    fn register(&self, tool: Arc<dyn AgentTool>) -> Result<(), ToolError> {
        match self {
            Self::Scoped(registry) => registry.register(tool),
            Self::Default => default_registry().register(tool),
        }
    }
}

thread_local! {
    static SCOPED_STACK: RefCell<Vec<Arc<ToolRegistry>>> = const { RefCell::new(Vec::new()) };
}

/// Python-backed executable tool.
pub struct PythonTool {
    name: String,
    description: String,
    input_schema: Value,
    output_schema: Value,
    callable: Py<PyAny>,
}

impl PythonTool {
    /// Build a Python-backed executable tool.
    pub fn new(
        name: String,
        description: String,
        input_schema: Value,
        output_schema: Value,
        callable: Py<PyAny>,
    ) -> Self {
        Self {
            name,
            description,
            input_schema,
            output_schema,
            callable,
        }
    }
}

#[async_trait]
impl AgentTool for PythonTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn input_schema(&self) -> Value {
        self.input_schema.clone()
    }

    fn output_schema(&self) -> Value {
        self.output_schema.clone()
    }

    async fn invoke(&self, args: Value) -> Result<Value, ToolError> {
        Python::attach(|py| {
            let callable = self.callable.bind(py);
            let kwargs = PyDict::new(py);
            if let Value::Object(values) = args {
                for (key, value) in values {
                    kwargs
                        .set_item(
                            key,
                            wyrd_utils::py::json_to_pyobject(py, &value).map_err(|error| {
                                ToolError::Invocation {
                                    detail: error.to_string(),
                                    cause: None,
                                }
                            })?,
                        )
                        .map_err(|error| ToolError::Invocation {
                            detail: error.to_string(),
                            cause: None,
                        })?;
                }
                let result =
                    callable
                        .call((), Some(&kwargs))
                        .map_err(|error| ToolError::Invocation {
                            detail: error.to_string(),
                            cause: None,
                        })?;
                wyrd_utils::py::pyobject_to_json(&result)
                    .map_err(|error| ToolError::OutputSerialization(error.to_string()))
            } else {
                let arg = wyrd_utils::py::json_to_pyobject(py, &args)
                    .map_err(|error| ToolError::InvalidInput(error.to_string()))?;
                let result = callable
                    .call1((arg,))
                    .map_err(|error| ToolError::Invocation {
                        detail: error.to_string(),
                        cause: None,
                    })?;
                wyrd_utils::py::pyobject_to_json(&result)
                    .map_err(|error| ToolError::OutputSerialization(error.to_string()))
            }
        })
    }
}

/// Register a Python callable in the active runtime-local tool registry.
#[pyfunction]
#[pyo3(signature = (name, description, input_schema, output_schema, callable))]
pub fn _register_tool(
    py: Python<'_>,
    name: String,
    description: String,
    input_schema: Py<PyAny>,
    output_schema: Py<PyAny>,
    callable: Py<PyAny>,
) -> WyrdPyResult<()> {
    let input_schema = wyrd_utils::py::pyobject_to_json(input_schema.bind(py))?;
    let output_schema = wyrd_utils::py::pyobject_to_json(output_schema.bind(py))?;
    let tool = Arc::new(PythonTool::new(
        name,
        description,
        input_schema,
        output_schema,
        callable,
    ));
    Ok(active_registry().register(tool)?)
}

/// Push a fresh Python tool registry scope.
#[pyfunction]
pub fn _push_tool_registry_scope() {
    SCOPED_STACK.with(|stack| {
        stack.borrow_mut().push(Arc::new(ToolRegistry::new()));
    });
}

/// Pop the current Python tool registry scope.
#[pyfunction]
pub fn _pop_tool_registry_scope() {
    SCOPED_STACK.with(|stack| {
        let _ = stack.borrow_mut().pop();
    });
}

/// Convert a pure-Python `_ToolCallable` wrapper into an executable tool.
///
/// # Errors
/// Returns `WYRD_TOOL_400_INVALID_SCHEMA` when the wrapper does not expose the
/// `name`, `description`, `input_schema`, `output_schema`, and `fn` attributes
/// the executable tool needs.
pub fn wrap_callable(py: Python<'_>, callable: Py<PyAny>) -> WyrdPyResult<Arc<dyn AgentTool>> {
    let bound = callable.bind(py);
    let attribute = |name: &str| bound.getattr(name).map_err(invalid_declaration);
    let name: String = attribute("name")?.extract().map_err(invalid_declaration)?;
    let description: String = attribute("description")?
        .extract()
        .map_err(invalid_declaration)?;
    let input_schema = wyrd_utils::py::pyobject_to_json(&attribute("input_schema")?)?;
    let output_schema = wyrd_utils::py::pyobject_to_json(&attribute("output_schema")?)?;
    let fn_obj: Py<PyAny> = attribute("fn")?.unbind();
    Ok(Arc::new(PythonTool::new(
        name,
        description,
        input_schema,
        output_schema,
        fn_obj,
    )))
}

/// Build a Python `_ToolCallable` wrapper from a Rust tool.
///
/// # Errors
/// Returns a catalog error when the interpreter refuses to import
/// `wyrd.agent.tool` or to build the wrapper object.
pub fn tool_callable_py(py: Python<'_>, tool: Arc<dyn AgentTool>) -> WyrdPyResult<Py<PyAny>> {
    let build = || -> PyResult<Py<PyAny>> {
        let module = py.import("wyrd.agent.tool")?;
        let cls = module.getattr("_ToolCallable")?;
        let kwargs = PyDict::new(py);
        let callable = Py::new(py, PyToolInvoker { tool: tool.clone() })?;
        kwargs.set_item("fn", callable)?;
        kwargs.set_item("name", tool.name())?;
        kwargs.set_item("description", tool.description())?;
        kwargs.set_item(
            "input_schema",
            wyrd_utils::py::json_to_pyobject(py, &tool.input_schema())?,
        )?;
        kwargs.set_item(
            "output_schema",
            wyrd_utils::py::json_to_pyobject(py, &tool.output_schema())?,
        )?;
        Ok(cls.call((), Some(&kwargs))?.unbind())
    };
    build().map_err(WyrdPyError::from)
}

/// Register tool Python helpers.
///
/// # Errors
/// Returns PyO3 registration errors.
pub fn python_register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    wyrd_utils::py::register_wyrd_error_exception(module)?;
    module.add_function(wrap_pyfunction!(_register_tool, module)?)?;
    module.add_function(wrap_pyfunction!(_push_tool_registry_scope, module)?)?;
    module.add_function(wrap_pyfunction!(_pop_tool_registry_scope, module)?)?;
    Ok(())
}

#[pyclass(module = "wyrd._wyrd.tool", name = "_ToolInvoker")]
struct PyToolInvoker {
    tool: Arc<dyn AgentTool>,
}

#[pymethods]
impl PyToolInvoker {
    fn __call__(
        &self,
        py: Python<'_>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> WyrdPyResult<Py<PyAny>> {
        let args = match kwargs {
            Some(kwargs) => wyrd_utils::py::pydict_to_json_value(kwargs)?,
            None => serde_json::json!({}),
        };
        let tool = self.tool.clone();
        let value = py.detach(|| wyrd_runtime::runtime().block_on(tool.invoke(args)))?;
        wyrd_utils::py::json_to_pyobject(py, &value).map_err(WyrdPyError::from)
    }
}

impl From<ToolError> for WyrdPyError {
    /// Widen an executable-tool failure into the shared Python boundary error.
    fn from(error: ToolError) -> Self {
        Self::from(WyrdError::from(error))
    }
}

fn active_registry() -> ActiveRegistry {
    SCOPED_STACK.with(|stack| {
        stack
            .borrow()
            .last()
            .cloned()
            .map_or(ActiveRegistry::Default, ActiveRegistry::Scoped)
    })
}
