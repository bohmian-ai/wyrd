//! Coercion helpers for provider names, JSON objects, and Python inputs.

use serde_json::{Map, Value};
use skald_spec::ProviderName;

use crate::error::{PromptBuilderError, PromptBuilderResult};

/// Resolves a raw passthrough provider name from the Python/Rust builder spelling.
pub fn provider_name_from_str(value: &str) -> ProviderName {
    match value.trim().to_ascii_lowercase().as_str() {
        "openai" | "open_ai" => ProviderName::OpenAi,
        "anthropic" => ProviderName::Anthropic,
        "google" | "gemini" => ProviderName::Google,
        "vertex" | "vertex_ai" => ProviderName::Vertex,
        _ => ProviderName::Custom(value.to_owned()),
    }
}

/// Converts a JSON Schema value into the object map expected by `OpenAI` fields.
pub fn schema_object(value: &Value) -> PromptBuilderResult<Map<String, Value>> {
    value
        .as_object()
        .cloned()
        .ok_or(PromptBuilderError::InvalidResponseSchema)
}

/// Returns a non-empty model string.
pub fn checked_model(model: impl Into<String>) -> PromptBuilderResult<String> {
    let model = model.into();
    if model.trim().is_empty() {
        return Err(PromptBuilderError::EmptyModel);
    }
    Ok(model)
}

#[cfg(feature = "python")]
use {
    pyo3::prelude::*,
    pyo3::types::{PyString, PyTuple},
    wyrd_utils::py::WyrdPyResult,
};

/// Converts a Python provider spelling into a native provider name.
#[cfg(feature = "python")]
pub fn provider_name_from_py(value: &Bound<'_, PyAny>) -> WyrdPyResult<ProviderName> {
    if value.is_instance_of::<PyString>() {
        return Ok(provider_name_from_str(&value.extract::<String>()?));
    }

    if let Ok(tuple) = value.cast::<PyTuple>()
        && tuple.len() == 2
    {
        let tag = tuple.get_item(0)?.extract::<String>()?;
        let custom = tuple.get_item(1)?.extract::<String>()?;
        if tag == "custom" && !custom.trim().is_empty() {
            return Ok(ProviderName::Custom(custom));
        }
    }

    Err(PromptBuilderError::InvalidProvider(value.str()?.extract::<String>()?).into())
}

/// Reads Pydantic `model_json_schema()` explicitly or falls back to JSON coercion.
#[cfg(feature = "python")]
pub fn schema_from_py(value: &Bound<'_, PyAny>) -> WyrdPyResult<Value> {
    if let Ok(method) = value.getattr("model_json_schema")
        && method.is_callable()
    {
        return Ok(wyrd_utils::py::pyobject_to_json(&method.call0()?)?);
    }
    Ok(wyrd_utils::py::pyobject_to_json(value)?)
}

/// Extracts a string list from a Python object.
#[cfg(feature = "python")]
pub fn strings_from_py(value: Option<&Bound<'_, PyAny>>) -> WyrdPyResult<Vec<String>> {
    let Some(value) = value.filter(|value| !value.is_none()) else {
        return Ok(Vec::new());
    };

    if value.is_instance_of::<PyString>() {
        return Ok(vec![value.extract::<String>()?]);
    }

    let mut out = Vec::new();
    for item in value.try_iter()? {
        out.push(item?.extract::<String>()?);
    }
    Ok(out)
}
