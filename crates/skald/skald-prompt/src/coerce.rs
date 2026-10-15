//! Coercion helpers for provider names, JSON objects, and Python inputs.

use serde_json::{Map, Value};

use crate::error::{PromptBuilderError, PromptBuilderResult};

/// Converts a JSON Schema value into the object map expected by `OpenAI` fields.
pub fn schema_object(value: &Value) -> PromptBuilderResult<Map<String, Value>> {
    value
        .as_object()
        .cloned()
        .ok_or(PromptBuilderError::InvalidResponseSchema)
}

/// Returns the model unchanged once it passes the shared literal-model rule.
///
/// Builders and the model setter route through here so runtime-built Prompts
/// accept exactly the models authored Prompts do ([`skald_spec::prompt::check_model`]).
///
/// # Errors
///
/// Returns [`PromptBuilderError::EmptyModel`] for a blank model and
/// [`PromptBuilderError::Skald`] wrapping the `model` field refusal for
/// control characters or placeholder syntax.
pub fn checked_model(model: impl Into<String>) -> PromptBuilderResult<String> {
    let model = model.into();
    if model.trim().is_empty() {
        return Err(PromptBuilderError::EmptyModel);
    }
    skald_spec::prompt::check_model(&model)?;
    Ok(model)
}

#[cfg(feature = "python")]
use {
    pyo3::prelude::*, pyo3::types::PyString, skald_spec::ProviderName, wyrd_utils::py::WyrdPyResult,
};

/// Converts a Python provider spelling into a native provider name.
#[cfg(feature = "python")]
pub fn provider_name_from_py(value: &Bound<'_, PyAny>) -> WyrdPyResult<ProviderName> {
    if value.is_instance_of::<PyString>() {
        return value
            .extract::<String>()?
            .parse()
            .map_err(|_| PromptBuilderError::InvalidProvider(value.to_string()).into());
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
