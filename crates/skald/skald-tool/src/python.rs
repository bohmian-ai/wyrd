//! Python boundary for runtime-local tools.

#![cfg(feature = "python")]

use std::cell::RefCell;
use std::sync::Arc;

use async_trait::async_trait;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use serde_json::Value;

use crate::{AgentTool, ToolError, ToolRegistry, default_registry};

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
) -> PyResult<()> {
    let input_schema = wyrd_utils::py::pyobject_to_json(input_schema.bind(py))?;
    let output_schema = wyrd_utils::py::pyobject_to_json(output_schema.bind(py))?;
    let tool = Arc::new(PythonTool::new(
        name,
        description,
        input_schema,
        output_schema,
        callable,
    ));
    active_registry()
        .register(tool)
        .map_err(|error| tool_error_to_py_err(py, error))
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
/// Returns Python extraction or JSON conversion failures.
pub fn wrap_callable(py: Python<'_>, callable: Py<PyAny>) -> PyResult<Arc<dyn AgentTool>> {
    let bound = callable.bind(py);
    let name: String = bound.getattr("name")?.extract()?;
    let description: String = bound.getattr("description")?.extract()?;
    let input_schema = wyrd_utils::py::pyobject_to_json(&bound.getattr("input_schema")?)?;
    let output_schema = wyrd_utils::py::pyobject_to_json(&bound.getattr("output_schema")?)?;
    let fn_obj: Py<PyAny> = bound.getattr("fn")?.unbind();
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
/// Returns Python object construction errors.
pub fn tool_callable_py(py: Python<'_>, tool: Arc<dyn AgentTool>) -> PyResult<Py<PyAny>> {
    let module = py.import("wyrd.tool")?;
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
    fn __call__(&self, py: Python<'_>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Py<PyAny>> {
        let args = match kwargs {
            Some(kwargs) => wyrd_utils::py::pydict_to_json_value(kwargs)?,
            None => serde_json::json!({}),
        };
        let tool = self.tool.clone();
        let result = py.detach(|| wyrd_runtime::runtime().block_on(tool.invoke(args)));
        match result {
            Ok(value) => wyrd_utils::py::json_to_pyobject(py, &value),
            Err(error) => Err(tool_error_to_py_err(py, error)),
        }
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

fn tool_error_to_py_err(py: Python<'_>, error: ToolError) -> PyErr {
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
        Err(source) => PyRuntimeError::new_err(source.to_string()),
    }
}
