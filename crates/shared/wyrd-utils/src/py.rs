//! Python-boundary helpers shared by PyO3-enabled Wyrd crates.

use pyo3::IntoPyObjectExt;
use pyo3::create_exception;
use pyo3::exceptions::{PyAttributeError, PyException, PyModuleNotFoundError, PyRuntimeError};
use pyo3::prelude::*;
use pyo3::types::{
    PyAny, PyBool, PyBytes, PyDict, PyFloat, PyInt, PyList, PyModule, PyString, PyTuple, PyType,
};
use serde_json::Value;
use wyrd_spec::error::WyrdError as SpecWyrdError;

create_exception!(
    wyrd._wyrd,
    WyrdError,
    PyException,
    "Base Python exception for structured Wyrd errors."
);
create_exception!(wyrd._wyrd, AgentError, WyrdError, "Agent Wyrd error.");
create_exception!(wyrd._wyrd, ToolError, WyrdError, "Tool Wyrd error.");
create_exception!(wyrd._wyrd, SessionError, WyrdError, "Session Wyrd error.");

/// Convert a JSON value to a Python object.
///
/// # Errors
/// Returns a Python error when conversion fails.
pub fn json_to_pyobject(py: Python<'_>, value: &Value) -> PyResult<Py<PyAny>> {
    match value {
        Value::Null => Ok(py.None()),
        Value::Bool(value) => value.into_py_any(py),
        Value::Number(value) => {
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
        Value::String(value) => value.into_py_any(py),
        Value::Array(values) => {
            let list = PyList::empty(py);
            for value in values {
                list.append(json_to_pyobject(py, value)?)?;
            }
            Ok(list.into_any().unbind())
        }
        Value::Object(values) => {
            let dict = PyDict::new(py);
            for (key, value) in values {
                dict.set_item(key, json_to_pyobject(py, value)?)?;
            }
            Ok(dict.into_any().unbind())
        }
    }
}

/// Convert a Python object to a JSON value.
///
/// # Errors
/// Returns a Python error when extraction fails.
pub fn pyobject_to_json(obj: &Bound<'_, PyAny>) -> PyResult<Value> {
    if obj.is_none() {
        return Ok(Value::Null);
    }
    if obj.is_instance_of::<PyBool>() {
        return Ok(serde_json::json!(obj.extract::<bool>()?));
    }
    if obj.is_instance_of::<PyInt>() {
        return Ok(serde_json::json!(obj.extract::<i64>()?));
    }
    if obj.is_instance_of::<PyFloat>() {
        return Ok(serde_json::json!(obj.extract::<f64>()?));
    }
    if obj.is_instance_of::<PyString>() {
        return Ok(Value::String(obj.extract::<String>()?));
    }
    if obj.is_instance_of::<PyBytes>() {
        let bytes = obj.cast::<PyBytes>()?;
        return Ok(Value::Array(
            bytes
                .as_bytes()
                .iter()
                .map(|byte| serde_json::json!(byte))
                .collect(),
        ));
    }
    if obj.is_instance_of::<PyList>() {
        let list = obj.cast::<PyList>()?;
        return py_iterable_to_json(list.iter());
    }
    if obj.is_instance_of::<PyTuple>() {
        let tuple = obj.cast::<PyTuple>()?;
        return py_iterable_to_json(tuple.iter());
    }
    if obj.is_instance_of::<PyDict>() {
        let dict = obj.cast::<PyDict>()?;
        let mut map = serde_json::Map::new();
        for (key, value) in dict.iter() {
            let key = if key.is_instance_of::<PyString>() {
                key.extract::<String>()?
            } else {
                key.str()?.extract::<String>()?
            };
            map.insert(key, pyobject_to_json(&value)?);
        }
        return Ok(Value::Object(map));
    }
    // Pydantic models and other dataclass-like objects with a `model_dump` method.
    if let Ok(dumped) = obj.call_method0("model_dump") {
        return pyobject_to_json(&dumped);
    }
    Ok(Value::String(obj.str()?.extract::<String>()?))
}

/// Convert a Python dictionary to a JSON object.
///
/// # Errors
/// Returns a Python error when extraction fails.
pub fn pydict_to_json_value(dict: &Bound<'_, PyDict>) -> PyResult<Value> {
    let mut map = serde_json::Map::new();
    for (key, value) in dict.iter() {
        let key = if key.is_instance_of::<PyString>() {
            key.extract::<String>()?
        } else {
            key.str()?.extract::<String>()?
        };
        map.insert(key, pyobject_to_json(&value)?);
    }
    Ok(Value::Object(map))
}

/// Return the imported module's `__version__` when available.
///
/// Missing modules and modules without `__version__` return `Ok(None)`.
/// Import-time failures from installed modules are returned as Python errors.
///
/// # Errors
/// Returns a Python error when import or attribute extraction fails for reasons
/// other than a missing module or missing `__version__`.
pub fn module_version(py: Python<'_>, name: &str) -> PyResult<Option<String>> {
    let module = match py.import(name) {
        Ok(module) => module,
        Err(err) if err.is_instance_of::<PyModuleNotFoundError>(py) => return Ok(None),
        Err(err) => return Err(err),
    };

    match module.getattr("__version__") {
        Ok(version) => Ok(version.extract::<String>().ok()),
        Err(err) if err.is_instance_of::<PyAttributeError>(py) => Ok(None),
        Err(err) => Err(err),
    }
}

/// Register the base structured Wyrd Python exception on a module.
///
/// # Errors
/// Returns a Python error when module registration fails.
pub fn register_wyrd_error_exception(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = module.py();
    module.add("WyrdError", py.get_type::<WyrdError>())?;
    module.add("AgentError", py.get_type::<AgentError>())?;
    module.add("ToolError", py.get_type::<ToolError>())?;
    module.add("SessionError", py.get_type::<SessionError>())
}

/// Convert a public Wyrd error into a structured Python Wyrd error.
#[must_use]
pub fn wyrd_error_to_py_err(error: SpecWyrdError) -> PyErr {
    Python::attach(|py| match build_wyrd_py_err(py, error) {
        Ok(error) => error,
        Err(source) => PyRuntimeError::new_err(format!(
            "failed to construct structured Wyrd Python error: {source}"
        )),
    })
}

/// Convert a public Wyrd error into a structured Python Wyrd error object.
///
/// # Errors
/// Returns a Python error when exception construction fails.
pub fn wyrd_error_to_py_object(py: Python<'_>, error: SpecWyrdError) -> PyResult<Py<PyAny>> {
    let exception = build_wyrd_py_exception(py, error)?;
    Ok(exception.into_any().unbind())
}

/// Convert a Python exception into a structured Wyrd error.
pub fn py_err_to_wyrd_error(py: Python<'_>, error: PyErr) -> SpecWyrdError {
    let value = error.value(py);
    if let Ok(code) = value
        .getattr("code")
        .and_then(|code| code.extract::<String>())
    {
        let message = value
            .getattr("message")
            .and_then(|message| message.extract::<String>())
            .unwrap_or_else(|_| error.to_string());
        return wyrd_error_from_python_code(code, message);
    }
    if let Ok(args_obj) = value.getattr("args")
        && let Ok(args) = args_obj.cast::<PyTuple>()
        && args.len() >= 2
        && let (Ok(code), Ok(message)) = (
            args.get_item(0).and_then(|item| item.extract::<String>()),
            args.get_item(1).and_then(|item| item.extract::<String>()),
        )
    {
        return wyrd_error_from_python_code(code, message);
    }
    let type_name = error
        .get_type(py)
        .name()
        .map(|name| name.to_string())
        .unwrap_or_else(|_| "PyException".to_owned());
    let detail = error.to_string();
    SpecWyrdError::AgentValidation {
        message: detail.clone(),
        details: serde_json::json!({
            "python_exception": type_name,
            "detail": detail,
        }),
    }
}

fn wyrd_error_from_python_code(code: String, message: String) -> SpecWyrdError {
    match code.as_str() {
        "WYRD_AGENT_499_CALLBACK_ABORTED" => SpecWyrdError::AgentCallbackAborted {
            message,
            details: serde_json::json!({ "python_error_code": code }),
        },
        _ => SpecWyrdError::AgentValidation {
            message,
            details: serde_json::json!({ "python_error_code": code }),
        },
    }
}

fn py_iterable_to_json<'py>(iter: impl Iterator<Item = Bound<'py, PyAny>>) -> PyResult<Value> {
    let mut values = Vec::new();
    for item in iter {
        values.push(pyobject_to_json(&item)?);
    }
    Ok(Value::Array(values))
}

fn build_wyrd_py_err(py: Python<'_>, error: SpecWyrdError) -> PyResult<PyErr> {
    Ok(PyErr::from_value(build_wyrd_py_exception(py, error)?))
}

fn build_wyrd_py_exception(py: Python<'_>, error: SpecWyrdError) -> PyResult<Bound<'_, PyAny>> {
    let display = error.to_string();
    let problem = error.as_problem_json();
    let code = problem_string(&problem, "code", error.code()).to_owned();
    let title = problem_string(&problem, "title", error.title()).to_owned();
    let status = problem
        .get("status")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| u64::from(error.status()));
    let message = problem_string(&problem, "detail", &display).to_owned();
    let remediation = problem_string(&problem, "remediation", error.remediation()).to_owned();
    let problem_type = problem
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let details = problem.get("details").cloned().unwrap_or(Value::Null);

    let exception = exception_type_for_code(py, &code).call1((message.clone(),))?;
    exception.setattr("code", code)?;
    exception.setattr("message", message)?;
    exception.setattr("details", json_to_pyobject(py, &details)?.bind(py))?;
    exception.setattr("remediation", remediation)?;
    exception.setattr("status", status)?;
    exception.setattr("title", title)?;
    exception.setattr("type", problem_type)?;
    exception.setattr("problem", json_to_pyobject(py, &problem)?.bind(py))?;
    Ok(exception)
}

fn problem_string<'a>(problem: &'a Value, key: &str, fallback: &'a str) -> &'a str {
    problem.get(key).and_then(Value::as_str).unwrap_or(fallback)
}

fn exception_type_for_code<'py>(py: Python<'py>, code: &str) -> Bound<'py, PyType> {
    if code.starts_with("WYRD_AGENT_") || code.starts_with("SKALD_AGENT_") {
        py.get_type::<AgentError>()
    } else if code.starts_with("WYRD_TOOL_") || code.starts_with("SKALD_TOOL_") {
        py.get_type::<ToolError>()
    } else if code.starts_with("WYRD_SESSION_") || code.starts_with("SKALD_SESSION_") {
        py.get_type::<SessionError>()
    } else {
        py.get_type::<WyrdError>()
    }
}
