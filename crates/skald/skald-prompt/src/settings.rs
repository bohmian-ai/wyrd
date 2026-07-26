//! Python settings wrappers for provider-native generation settings.

use skald_spec::{
    AnthropicMessagesSettings, GoogleGenerateSettings, OpenAiChatSettings, OpenAiResponsesSettings,
};

#[cfg(feature = "python")]
use {
    pyo3::IntoPyObjectExt,
    pyo3::{prelude::*, types::PyDict},
    skald_spec::{ProviderName, ProviderRequest, SkaldError},
    wyrd_interfaces::error::{CardPyResult, WyrdPyError},
};

#[cfg(feature = "python")]
use crate::prompt::Prompt;

/// Python wrapper for native `OpenAI` Chat generation settings.
///
/// Keyword arguments map directly to `OpenAI` Chat Completions request fields and
/// serialize at the native request top level. Unknown keyword arguments are
/// preserved in the native settings `extra` map.
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(module = "wyrd.prompt", name = "OpenAISettings", skip_from_py_object)
)]
#[derive(Debug, Clone, PartialEq)]
pub struct PyOpenAiChatSettings {
    pub(crate) inner: OpenAiChatSettings,
}

/// Python wrapper for native `OpenAI` Responses generation settings.
///
/// Keyword arguments map directly to `OpenAI` Responses request fields and
/// serialize at the native request top level. Unknown keyword arguments are
/// preserved in the native settings `extra` map.
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(
        module = "wyrd.prompt",
        name = "OpenAIResponsesSettings",
        skip_from_py_object
    )
)]
#[derive(Debug, Clone, PartialEq)]
pub struct PyOpenAiResponsesSettings {
    pub(crate) inner: OpenAiResponsesSettings,
}

/// Python wrapper for native Anthropic Messages generation settings.
///
/// Keyword arguments map directly to Anthropic Messages request fields and
/// serialize at the native request top level. `max_tokens` defaults to `4096`.
/// Unknown keyword arguments are preserved in the native settings `extra` map.
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(
        module = "wyrd.prompt",
        name = "AnthropicSettings",
        skip_from_py_object
    )
)]
#[derive(Debug, Clone, PartialEq)]
pub struct PyAnthropicSettings {
    pub(crate) inner: AnthropicMessagesSettings,
}

/// Python wrapper for native Gemini and Vertex `GenerateContent` settings.
///
/// Request-level fields serialize at the native request top level. Generation
/// knobs such as temperature, token limits, and thinking config live inside the
/// native `generation_config` object. Unknown keyword arguments are preserved in
/// the native settings `extra` map.
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(module = "wyrd.prompt", name = "GeminiSettings", skip_from_py_object)
)]
#[derive(Debug, Clone, PartialEq)]
pub struct PyGoogleGenerateSettings {
    pub(crate) inner: GoogleGenerateSettings,
}

macro_rules! impl_settings_wrapper {
    ($wrapper:ident, $native:ty, $provider:expr, $label:literal) => {
        impl $wrapper {
            /// Borrow the native provider settings.
            pub const fn native(&self) -> &$native {
                &self.inner
            }

            /// Wrap native provider settings.
            pub const fn from_native(inner: $native) -> Self {
                Self { inner }
            }
        }

        #[cfg(feature = "python")]
        #[pyo3::pymethods]
        impl $wrapper {
            /// Build provider settings from keyword arguments.
            #[new]
            #[pyo3(signature = (**kwargs))]
            pub fn __new__(kwargs: Option<&Bound<'_, PyDict>>) -> CardPyResult<Self> {
                let value = kwargs
                    .map(wyrd_utils::py::pydict_to_json_value)
                    .transpose()?
                    .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));
                decode_settings::<$native>(value, $provider).map(Self::from_native)
            }

            /// Build provider settings from a Python dictionary.
            #[staticmethod]
            pub fn from_dict(value: &Bound<'_, PyAny>) -> CardPyResult<Self> {
                let value = wyrd_utils::py::pyobject_to_json(value)?;
                decode_settings::<$native>(value, $provider).map(Self::from_native)
            }

            /// Return settings as a Python dictionary.
            pub fn to_dict(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
                Ok(wyrd_utils::py::json_to_pyobject(
                    py,
                    &serde_json::to_value(&self.inner)?,
                )?)
            }

            /// Return settings as JSON.
            pub fn model_dump_json(&self) -> CardPyResult<String> {
                Ok(serde_json::to_string(&self.inner)?)
            }

            /// Return a concise Python representation.
            pub fn __repr__(&self) -> String {
                let body = serde_json::to_string(&self.inner).unwrap_or_else(|_| "{}".to_owned());
                format!("{}({})", $label, body)
            }
        }
    };
}

impl_settings_wrapper!(
    PyOpenAiChatSettings,
    OpenAiChatSettings,
    ProviderName::OpenAi,
    "OpenAISettings"
);
impl_settings_wrapper!(
    PyOpenAiResponsesSettings,
    OpenAiResponsesSettings,
    ProviderName::OpenAi,
    "OpenAIResponsesSettings"
);
impl_settings_wrapper!(
    PyAnthropicSettings,
    AnthropicMessagesSettings,
    ProviderName::Anthropic,
    "AnthropicSettings"
);
impl_settings_wrapper!(
    PyGoogleGenerateSettings,
    GoogleGenerateSettings,
    ProviderName::Google,
    "GeminiSettings"
);

#[cfg(feature = "python")]
fn decode_settings<T>(value: serde_json::Value, provider: ProviderName) -> CardPyResult<T>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value(value).map_err(|error| {
        WyrdPyError::from(wyrd_spec::error::WyrdError::from(
            SkaldError::SettingsDecode {
                provider,
                message: error.to_string(),
            },
        ))
    })
}

#[cfg(feature = "python")]
fn settings_decode(provider: ProviderName, message: impl Into<String>) -> WyrdPyError {
    WyrdPyError::from(wyrd_spec::error::WyrdError::from(
        SkaldError::SettingsDecode {
            provider,
            message: message.into(),
        },
    ))
}

#[cfg(feature = "python")]
fn settings_mismatch(expected: ProviderName, got: ProviderName) -> WyrdPyError {
    WyrdPyError::from(wyrd_spec::error::WyrdError::from(
        SkaldError::SettingsProviderMismatch { expected, got },
    ))
}

#[cfg(feature = "python")]
fn json_object_from_py(
    value: &Bound<'_, PyAny>,
    provider: ProviderName,
) -> CardPyResult<serde_json::Value> {
    let value = wyrd_utils::py::pyobject_to_json(value)?;
    if value.is_object() {
        Ok(value)
    } else {
        Err(settings_decode(
            provider,
            "model_settings must be a typed settings object or a dict",
        ))
    }
}

/// Resolve `OpenAI` Chat `model_settings` into native settings.
#[cfg(feature = "python")]
pub(crate) fn resolve_openai_chat_settings(
    value: Option<&Bound<'_, PyAny>>,
) -> CardPyResult<OpenAiChatSettings> {
    let Some(value) = value.filter(|value| !value.is_none()) else {
        return Ok(OpenAiChatSettings::default());
    };
    if let Ok(settings) = value.extract::<PyRef<'_, PyOpenAiChatSettings>>() {
        return Ok(settings.inner.clone());
    }
    if value
        .extract::<PyRef<'_, PyOpenAiResponsesSettings>>()
        .is_ok()
    {
        return Err(settings_mismatch(
            ProviderName::OpenAi,
            ProviderName::OpenAi,
        ));
    }
    if value.extract::<PyRef<'_, PyAnthropicSettings>>().is_ok() {
        return Err(settings_mismatch(
            ProviderName::OpenAi,
            ProviderName::Anthropic,
        ));
    }
    if value
        .extract::<PyRef<'_, PyGoogleGenerateSettings>>()
        .is_ok()
    {
        return Err(settings_mismatch(
            ProviderName::OpenAi,
            ProviderName::Google,
        ));
    }
    decode_settings(
        json_object_from_py(value, ProviderName::OpenAi)?,
        ProviderName::OpenAi,
    )
}

/// Resolve `OpenAI` Responses `model_settings` into native settings.
#[cfg(feature = "python")]
pub(crate) fn resolve_openai_responses_settings(
    value: Option<&Bound<'_, PyAny>>,
) -> CardPyResult<OpenAiResponsesSettings> {
    let Some(value) = value.filter(|value| !value.is_none()) else {
        return Ok(OpenAiResponsesSettings::default());
    };
    if let Ok(settings) = value.extract::<PyRef<'_, PyOpenAiResponsesSettings>>() {
        return Ok(settings.inner.clone());
    }
    if value.extract::<PyRef<'_, PyOpenAiChatSettings>>().is_ok() {
        return Err(settings_mismatch(
            ProviderName::OpenAi,
            ProviderName::OpenAi,
        ));
    }
    if value.extract::<PyRef<'_, PyAnthropicSettings>>().is_ok() {
        return Err(settings_mismatch(
            ProviderName::OpenAi,
            ProviderName::Anthropic,
        ));
    }
    if value
        .extract::<PyRef<'_, PyGoogleGenerateSettings>>()
        .is_ok()
    {
        return Err(settings_mismatch(
            ProviderName::OpenAi,
            ProviderName::Google,
        ));
    }
    decode_settings(
        json_object_from_py(value, ProviderName::OpenAi)?,
        ProviderName::OpenAi,
    )
}

/// Resolve Anthropic `model_settings` into native settings.
#[cfg(feature = "python")]
pub(crate) fn resolve_anthropic_settings(
    value: Option<&Bound<'_, PyAny>>,
) -> CardPyResult<AnthropicMessagesSettings> {
    let Some(value) = value.filter(|value| !value.is_none()) else {
        return Ok(AnthropicMessagesSettings::default());
    };
    if let Ok(settings) = value.extract::<PyRef<'_, PyAnthropicSettings>>() {
        return Ok(settings.inner.clone());
    }
    if value.extract::<PyRef<'_, PyOpenAiChatSettings>>().is_ok()
        || value
            .extract::<PyRef<'_, PyOpenAiResponsesSettings>>()
            .is_ok()
    {
        return Err(settings_mismatch(
            ProviderName::Anthropic,
            ProviderName::OpenAi,
        ));
    }
    if value
        .extract::<PyRef<'_, PyGoogleGenerateSettings>>()
        .is_ok()
    {
        return Err(settings_mismatch(
            ProviderName::Anthropic,
            ProviderName::Google,
        ));
    }
    decode_settings(
        json_object_from_py(value, ProviderName::Anthropic)?,
        ProviderName::Anthropic,
    )
}

/// Resolve Google or Vertex `model_settings` into native settings.
#[cfg(feature = "python")]
pub(crate) fn resolve_google_settings(
    value: Option<&Bound<'_, PyAny>>,
) -> CardPyResult<GoogleGenerateSettings> {
    let Some(value) = value.filter(|value| !value.is_none()) else {
        return Ok(GoogleGenerateSettings::default());
    };
    if let Ok(settings) = value.extract::<PyRef<'_, PyGoogleGenerateSettings>>() {
        return Ok(settings.inner.clone());
    }
    if value.extract::<PyRef<'_, PyOpenAiChatSettings>>().is_ok()
        || value
            .extract::<PyRef<'_, PyOpenAiResponsesSettings>>()
            .is_ok()
    {
        return Err(settings_mismatch(
            ProviderName::Google,
            ProviderName::OpenAi,
        ));
    }
    if value.extract::<PyRef<'_, PyAnthropicSettings>>().is_ok() {
        return Err(settings_mismatch(
            ProviderName::Google,
            ProviderName::Anthropic,
        ));
    }
    decode_settings(
        json_object_from_py(value, ProviderName::Google)?,
        ProviderName::Google,
    )
}

/// Return a prompt copy with `model_settings` resolved for its active provider.
#[cfg(feature = "python")]
pub fn apply_model_settings(
    prompt: &skald_spec::Prompt,
    value: Option<&Bound<'_, PyAny>>,
) -> CardPyResult<skald_spec::Prompt> {
    let Some(value) = value.filter(|value| !value.is_none()) else {
        return Ok(prompt.clone());
    };
    let mut prompt = prompt.clone();
    match &mut prompt.request {
        ProviderRequest::OpenAiChatCompletion(request)
        | ProviderRequest::OpenAiChatCompatible { request, .. } => {
            request.settings = resolve_openai_chat_settings(Some(value))?;
        }
        ProviderRequest::OpenAiResponses(request) => {
            request.settings = resolve_openai_responses_settings(Some(value))?;
        }
        ProviderRequest::AnthropicMessage(request) => {
            request.settings = resolve_anthropic_settings(Some(value))?;
        }
        ProviderRequest::GeminiGenerateContent(request) => {
            request.settings = resolve_google_settings(Some(value))?;
        }
        ProviderRequest::Vertex(request) => {
            request.0.settings = resolve_google_settings(Some(value))?;
        }
        _ => {
            return Err(settings_decode(
                prompt.request.provider(),
                "this prompt request does not support model_settings",
            ));
        }
    }
    Ok(prompt)
}

/// Return typed Python settings for a prompt, or `None` for raw prompts.
#[cfg(feature = "python")]
pub fn model_settings_py(
    prompt: &skald_spec::Prompt,
    py: Python<'_>,
) -> CardPyResult<Option<Py<PyAny>>> {
    Ok(match prompt.settings_ref() {
        Some(skald_spec::ProviderSettingsRef::OpenAiChat(settings)) => {
            Some(PyOpenAiChatSettings::from_native(settings.clone()).into_py_any(py)?)
        }
        Some(skald_spec::ProviderSettingsRef::OpenAiResponses(settings)) => {
            Some(PyOpenAiResponsesSettings::from_native(settings.clone()).into_py_any(py)?)
        }
        Some(skald_spec::ProviderSettingsRef::Anthropic(settings)) => {
            Some(PyAnthropicSettings::from_native(settings.clone()).into_py_any(py)?)
        }
        Some(skald_spec::ProviderSettingsRef::Google(settings)) => {
            Some(PyGoogleGenerateSettings::from_native(settings.clone()).into_py_any(py)?)
        }
        None => None,
    })
}

/// Return a typed Python `Prompt` for native prompt metadata.
#[cfg(feature = "python")]
pub fn prompt_py(prompt: skald_spec::Prompt, py: Python<'_>) -> CardPyResult<Py<Prompt>> {
    Ok(Py::new(py, Prompt::from_native(prompt))?)
}
