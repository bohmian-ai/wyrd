//! `wyrd.prompt` registration: the shared Skald Prompt classes, plus the
//! Verdict factory this SDK adds to their `ResponseFormat`.

use pyo3::prelude::*;
use pyo3::types::PyModule;
use skald_prompt::{PromptBuilderError, ResponseFormat};
use wyrd_utils::py::WyrdPyResult;

/// The canonical Verdict format, optionally requiring custom `data`
/// described by a Pydantic model class or a JSON Schema, normalized as a
/// Prompt's structured output is.
///
/// # Errors
/// Raises `WyrdError` when `data` is neither a model class nor a JSON Schema
/// object.
#[pyfunction]
#[pyo3(signature = (data = None))]
fn verdict(data: Option<&Bound<'_, PyAny>>) -> WyrdPyResult<ResponseFormat> {
    let Some(data) = data.filter(|data| !data.is_none()) else {
        return Ok(ResponseFormat::verdict(None));
    };
    let serde_json::Value::Object(schema) = skald_prompt::prompt::output_schema_from_py(data)?
    else {
        return Err(PromptBuilderError::InvalidResponseSchema.into());
    };
    Ok(ResponseFormat::verdict(Some(schema)))
}

/// Register the shared Prompt classes on `module` and add `verdict` to its
/// `ResponseFormat` as a static method, so `ResponseFormat.verdict` and the
/// class's other constructors share one class identity.
///
/// # Errors
/// Propagates the Python error from registration or attribute assignment.
pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    skald_prompt::register_prompt(module)?;
    let py = module.py();
    let factory = py
        .import("builtins")?
        .getattr("staticmethod")?
        .call1((wrap_pyfunction!(verdict, module)?,))?;
    module
        .getattr("ResponseFormat")?
        .setattr("verdict", factory)
}
