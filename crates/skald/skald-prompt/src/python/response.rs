//! `PyProviderResponse` — top-level response wrapper that projects to each
//! provider's typed Python response wrapper.

use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyModule;
use skald_spec::ProviderResponse;
use wyrd_interfaces::error::CardPyResult;

use super::anthropic::PyAnthropicMessagesResponse;
use super::google::{PyGeminiResponse, PyVertexResponse};
use super::openai_chat_response::PyOpenAiChatResponse;
use super::openai_responses::PyOpenAiResponsesResponse;
use crate::prompt::{provider_name_to_string, wrong_provider};

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
    #[getter]
    pub fn provider(&self) -> String {
        provider_name_to_string(&self.inner.provider())
    }

    pub fn openai(&self) -> CardPyResult<PyOpenAiChatResponse> {
        match self.inner.as_ref() {
            ProviderResponse::OpenAiChatCompletion(_) => {
                Ok(PyOpenAiChatResponse::new(Arc::clone(&self.inner)))
            }
            other => Err(wrong_provider("openai", other.provider()).into()),
        }
    }

    pub fn openai_responses(&self) -> CardPyResult<PyOpenAiResponsesResponse> {
        match self.inner.as_ref() {
            ProviderResponse::OpenAiResponses(_) => {
                Ok(PyOpenAiResponsesResponse::new(Arc::clone(&self.inner)))
            }
            other => Err(wrong_provider("openai_responses", other.provider()).into()),
        }
    }

    pub fn anthropic(&self) -> CardPyResult<PyAnthropicMessagesResponse> {
        match self.inner.as_ref() {
            ProviderResponse::AnthropicMessage(_) => {
                Ok(PyAnthropicMessagesResponse::new(Arc::clone(&self.inner)))
            }
            other => Err(wrong_provider("anthropic", other.provider()).into()),
        }
    }

    pub fn gemini(&self) -> CardPyResult<PyGeminiResponse> {
        match self.inner.as_ref() {
            ProviderResponse::GeminiGenerateContent(_) => {
                Ok(PyGeminiResponse::new(Arc::clone(&self.inner)))
            }
            other => Err(wrong_provider("gemini", other.provider()).into()),
        }
    }

    pub fn vertex(&self) -> CardPyResult<PyVertexResponse> {
        match self.inner.as_ref() {
            ProviderResponse::VertexGenerateContent(_) => {
                Ok(PyVertexResponse::new(Arc::clone(&self.inner)))
            }
            other => Err(wrong_provider("vertex", other.provider()).into()),
        }
    }

    pub fn model_dump(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        wyrd_utils::py::json_to_pyobject(py, &serde_json::to_value(self.inner.as_ref())?)
            .map_err(Into::into)
    }

    pub fn model_dump_json(&self) -> CardPyResult<String> {
        serde_json::to_string(self.inner.as_ref()).map_err(Into::into)
    }

    pub fn __repr__(&self) -> String {
        format!("ProviderResponse(provider={:?})", self.provider())
    }
}

pub(super) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyProviderResponse>()?;
    Ok(())
}
