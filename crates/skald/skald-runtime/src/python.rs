//! Python boundary for provider registries used by agent tests.

#![cfg(feature = "python")]

use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyModule;
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatResponse, OpenAiMessageContent, OpenAiUsage,
};
use skald_spec::{ProviderName, ProviderResponse};

use crate::{MockProvider, ProviderRegistry};

/// Python-visible provider registry handle.
#[pyclass(
    module = "wyrd._wyrd.providers",
    name = "_ProviderRegistryInner",
    skip_from_py_object
)]
#[derive(Clone)]
pub struct PyProviderRegistryInner {
    /// Shared provider registry.
    pub inner: Arc<ProviderRegistry>,
}

#[pymethods]
impl PyProviderRegistryInner {
    /// Build an empty provider registry.
    #[new]
    pub fn __new__() -> Self {
        Self {
            inner: Arc::new(ProviderRegistry::new()),
        }
    }

    /// Return registered provider names.
    pub fn names(&self) -> Vec<String> {
        self.inner.names().map(provider_name_label).collect()
    }
}

/// Build a registry preloaded with a deterministic OpenAI-shaped mock provider.
///
/// # Errors
/// Returns PyO3 registration errors only.
#[pyfunction]
#[pyo3(signature = (text="mock response"))]
pub fn _build_mock_registry(text: &str) -> PyProviderRegistryInner {
    let mock = MockProvider::new(ProviderName::Custom("mock".to_owned()));
    mock.push_response(openai_text_response(text));
    let mut registry = ProviderRegistry::new();
    registry.register(Arc::new(mock));
    PyProviderRegistryInner {
        inner: Arc::new(registry),
    }
}

/// Refresh the process default provider registry from environment variables.
#[pyfunction]
pub fn _init_default_providers() {
    crate::refresh_default_registry_from_env();
}

/// Register provider Python classes.
///
/// # Errors
/// Returns PyO3 registration errors.
pub fn python_register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyProviderRegistryInner>()?;
    module.add_function(wrap_pyfunction!(_build_mock_registry, module)?)?;
    module.add_function(wrap_pyfunction!(_init_default_providers, module)?)?;
    Ok(())
}

fn provider_name_label(name: &ProviderName) -> String {
    match name {
        ProviderName::OpenAi => "openai".to_owned(),
        ProviderName::Anthropic => "anthropic".to_owned(),
        ProviderName::Google => "google".to_owned(),
        ProviderName::Vertex => "vertex".to_owned(),
        ProviderName::Custom(value) => value.clone(),
    }
}

fn openai_text_response(text: &str) -> ProviderResponse {
    ProviderResponse::OpenAiChatCompletion(OpenAiChatResponse {
        id: "mock_response".to_owned(),
        object: "chat.completion".to_owned(),
        created: 0,
        model: "mock-model".to_owned(),
        choices: vec![OpenAiChatChoice {
            index: 0,
            message: OpenAiChatMessage {
                role: "assistant".to_owned(),
                content: Some(OpenAiMessageContent::Text(text.to_owned())),
                name: None,
                tool_calls: None,
                tool_call_id: None,
                refusal: None,
            },
            finish_reason: Some("stop".to_owned()),
            logprobs: None,
        }],
        usage: Some(OpenAiUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            prompt_tokens_details: None,
            completion_tokens_details: None,
        }),
        system_fingerprint: None,
        service_tier: None,
    })
}
