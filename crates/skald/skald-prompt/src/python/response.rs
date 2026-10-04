//! `PyProviderResponse` — top-level response wrapper that projects to each
//! provider's typed Python response wrapper.

use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyModule;
use skald_spec::ProviderResponse;
use wyrd_utils::py::WyrdPyResult;

use super::anthropic::PyAnthropicMessagesResponse;
use super::google::{PyGeminiResponse, PyVertexResponse};
use super::openai_chat_response::PyOpenAiChatResponse;
use super::openai_responses::PyOpenAiResponsesResponse;
use crate::prompt::{provider_name_to_string, wrong_provider};

/// Python `ProviderResponse`: a provider-native response with typed per-provider views.
///
/// The response sits behind an `Arc` so each typed view shares it without copying.
#[pyclass(module = "wyrd.prompt", name = "ProviderResponse")]
pub struct PyProviderResponse {
    pub(crate) inner: Arc<ProviderResponse>,
}

impl PyProviderResponse {
    pub fn from_native(inner: ProviderResponse) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }
    pub fn from_arc(inner: Arc<ProviderResponse>) -> Self {
        Self { inner }
    }
    pub fn native(&self) -> &ProviderResponse {
        &self.inner
    }
}

#[pymethods]
impl PyProviderResponse {
    /// Provider name: `"openai"`, `"anthropic"`, `"google"`, `"vertex"`, or a custom name.
    #[getter]
    pub fn provider(&self) -> String {
        provider_name_to_string(&self.inner.provider())
    }

    /// Return the typed OpenAI Chat Completions view, sharing this response.
    ///
    /// # Errors
    /// Returns `WYRD_PROMPT_400_PROVIDER_MISMATCH` when the response is from another API.
    pub fn openai(&self) -> WyrdPyResult<PyOpenAiChatResponse> {
        match self.inner.as_ref() {
            ProviderResponse::OpenAiChatCompletion(_) => {
                Ok(PyOpenAiChatResponse::new(Arc::clone(&self.inner)))
            }
            other => Err(wrong_provider("openai", other.provider()).into()),
        }
    }

    /// Return the typed OpenAI Responses view, sharing this response.
    ///
    /// # Errors
    /// Returns `WYRD_PROMPT_400_PROVIDER_MISMATCH` when the response is from another API.
    pub fn openai_responses(&self) -> WyrdPyResult<PyOpenAiResponsesResponse> {
        match self.inner.as_ref() {
            ProviderResponse::OpenAiResponses(_) => {
                Ok(PyOpenAiResponsesResponse::new(Arc::clone(&self.inner)))
            }
            other => Err(wrong_provider("openai_responses", other.provider()).into()),
        }
    }

    /// Return the typed Anthropic Messages view, sharing this response.
    ///
    /// # Errors
    /// Returns `WYRD_PROMPT_400_PROVIDER_MISMATCH` when the response is from another API.
    pub fn anthropic(&self) -> WyrdPyResult<PyAnthropicMessagesResponse> {
        match self.inner.as_ref() {
            ProviderResponse::AnthropicMessage(_) => {
                Ok(PyAnthropicMessagesResponse::new(Arc::clone(&self.inner)))
            }
            other => Err(wrong_provider("anthropic", other.provider()).into()),
        }
    }

    /// Return the typed Gemini GenerateContent view, sharing this response.
    ///
    /// # Errors
    /// Returns `WYRD_PROMPT_400_PROVIDER_MISMATCH` when the response is from another API.
    pub fn gemini(&self) -> WyrdPyResult<PyGeminiResponse> {
        match self.inner.as_ref() {
            ProviderResponse::GeminiGenerateContent(_) => {
                Ok(PyGeminiResponse::new(Arc::clone(&self.inner)))
            }
            other => Err(wrong_provider("gemini", other.provider()).into()),
        }
    }

    /// Return the typed Vertex GenerateContent view, sharing this response.
    ///
    /// # Errors
    /// Returns `WYRD_PROMPT_400_PROVIDER_MISMATCH` when the response is from another API.
    pub fn vertex(&self) -> WyrdPyResult<PyVertexResponse> {
        match self.inner.as_ref() {
            ProviderResponse::VertexGenerateContent(_) => {
                Ok(PyVertexResponse::new(Arc::clone(&self.inner)))
            }
            other => Err(wrong_provider("vertex", other.provider()).into()),
        }
    }

    /// Return the native provider response as a Python dictionary.
    ///
    /// # Errors
    /// Returns a Wyrd error when serialization or Python conversion fails.
    pub fn model_dump(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        wyrd_utils::py::json_to_pyobject(py, &serde_json::to_value(self.inner.as_ref())?)
            .map_err(Into::into)
    }

    /// Return the native provider response as a JSON string.
    ///
    /// # Errors
    /// Returns a Wyrd error when JSON serialization fails.
    pub fn model_dump_json(&self) -> WyrdPyResult<String> {
        serde_json::to_string(self.inner.as_ref()).map_err(Into::into)
    }

    /// Return `ProviderResponse(provider=...)` for interactive display.
    pub fn __repr__(&self) -> String {
        format!("ProviderResponse(provider={:?})", self.provider())
    }
}

pub(super) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyProviderResponse>()?;
    Ok(())
}
