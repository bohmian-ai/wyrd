//! `PyOpenAi*` response-side wrappers — choices, usage, logprobs, token
//! details. Constructors take `Arc<ProviderResponse>`; nested accessors hold
//! the same `Arc` and project into the variant fields. `.expect("guarded")`
//! and `unreachable!()` assert the variant-match invariant established at
//! construction.

use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyModule;
use skald_spec::ProviderResponse;
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatLogprobs, OpenAiChatResponse, OpenAiCompletionTokensDetails,
    OpenAiPromptTokensDetails, OpenAiUsage,
};
use wyrd_utils::py::WyrdPyResult;

use super::openai_chat_messages::PyOpenAiChatMessage;
use super::shared::ChatMessageSource;

// ──────────────────────────────────────────────────────────────────────────────
// OpenAI Chat — response side
// ──────────────────────────────────────────────────────────────────────────────

#[pyclass(module = "wyrd.prompt", name = "OpenAiChatResponse")]
pub struct PyOpenAiChatResponse {
    inner: Arc<ProviderResponse>,
}
impl PyOpenAiChatResponse {
    pub fn new(inner: Arc<ProviderResponse>) -> Self {
        Self { inner }
    }
    fn resp(&self) -> &OpenAiChatResponse {
        match self.inner.as_ref() {
            ProviderResponse::OpenAiChatCompletion(r) => r,
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiChatResponse {
    #[getter]
    fn id(&self) -> &str {
        &self.resp().id
    }
    #[getter]
    fn object(&self) -> &str {
        &self.resp().object
    }
    #[getter]
    fn created(&self) -> u64 {
        self.resp().created
    }
    #[getter]
    fn model(&self) -> &str {
        &self.resp().model
    }
    #[getter]
    fn system_fingerprint(&self) -> Option<&str> {
        self.resp().system_fingerprint.as_deref()
    }
    #[getter]
    fn service_tier(&self) -> Option<&str> {
        self.resp().service_tier.as_deref()
    }
    #[getter]
    fn usage(&self) -> Option<PyOpenAiUsage> {
        self.resp().usage.as_ref().map(|_| PyOpenAiUsage {
            inner: Arc::clone(&self.inner),
        })
    }
    #[getter]
    fn choices(&self) -> Vec<PyOpenAiChatChoice> {
        (0..self.resp().choices.len())
            .map(|i| PyOpenAiChatChoice {
                inner: Arc::clone(&self.inner),
                index: i,
            })
            .collect()
    }
    fn __repr__(&self) -> String {
        format!(
            "OpenAiChatResponse(id={:?}, model={:?})",
            self.resp().id,
            self.resp().model
        )
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiChatChoice")]
pub struct PyOpenAiChatChoice {
    inner: Arc<ProviderResponse>,
    index: usize,
}
impl PyOpenAiChatChoice {
    fn c(&self) -> &OpenAiChatChoice {
        match self.inner.as_ref() {
            ProviderResponse::OpenAiChatCompletion(r) => &r.choices[self.index],
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiChatChoice {
    #[getter]
    fn index(&self) -> u32 {
        self.c().index
    }
    #[getter]
    fn finish_reason(&self) -> Option<&str> {
        self.c().finish_reason.as_deref()
    }
    #[getter]
    fn message(&self) -> PyOpenAiChatMessage {
        PyOpenAiChatMessage {
            src: ChatMessageSource::Choice {
                inner: Arc::clone(&self.inner),
                choice_index: self.index,
            },
        }
    }
    #[getter]
    fn logprobs(&self) -> Option<PyOpenAiChatLogprobs> {
        self.c().logprobs.as_ref().map(|_| PyOpenAiChatLogprobs {
            inner: Arc::clone(&self.inner),
            choice_index: self.index,
        })
    }
    fn __repr__(&self) -> String {
        format!(
            "OpenAiChatChoice(index={}, finish_reason={:?})",
            self.c().index,
            self.c().finish_reason
        )
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiChatLogprobs")]
pub struct PyOpenAiChatLogprobs {
    inner: Arc<ProviderResponse>,
    choice_index: usize,
}
impl PyOpenAiChatLogprobs {
    fn l(&self) -> &OpenAiChatLogprobs {
        match self.inner.as_ref() {
            ProviderResponse::OpenAiChatCompletion(r) => r.choices[self.choice_index]
                .logprobs
                .as_ref()
                .expect("guarded"),
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiChatLogprobs {
    #[getter]
    fn content(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        wyrd_utils::py::json_to_pyobject(py, &serde_json::Value::Array(self.l().content.clone()))
            .map_err(Into::into)
    }
    #[getter]
    fn refusal(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        wyrd_utils::py::json_to_pyobject(py, &serde_json::Value::Array(self.l().refusal.clone()))
            .map_err(Into::into)
    }
    fn __repr__(&self) -> String {
        "OpenAiChatLogprobs".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiUsage")]
pub struct PyOpenAiUsage {
    inner: Arc<ProviderResponse>,
}
impl PyOpenAiUsage {
    fn u(&self) -> &OpenAiUsage {
        match self.inner.as_ref() {
            ProviderResponse::OpenAiChatCompletion(r) => r.usage.as_ref().expect("guarded"),
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiUsage {
    #[getter]
    fn prompt_tokens(&self) -> u64 {
        self.u().prompt_tokens
    }
    #[getter]
    fn completion_tokens(&self) -> u64 {
        self.u().completion_tokens
    }
    #[getter]
    fn total_tokens(&self) -> u64 {
        self.u().total_tokens
    }
    #[getter]
    fn prompt_tokens_details(&self) -> Option<PyOpenAiPromptTokensDetails> {
        self.u()
            .prompt_tokens_details
            .as_ref()
            .map(|_| PyOpenAiPromptTokensDetails {
                inner: Arc::clone(&self.inner),
            })
    }
    #[getter]
    fn completion_tokens_details(&self) -> Option<PyOpenAiCompletionTokensDetails> {
        self.u()
            .completion_tokens_details
            .as_ref()
            .map(|_| PyOpenAiCompletionTokensDetails {
                inner: Arc::clone(&self.inner),
            })
    }
    fn __repr__(&self) -> String {
        format!(
            "OpenAiUsage(prompt={}, completion={}, total={})",
            self.u().prompt_tokens,
            self.u().completion_tokens,
            self.u().total_tokens
        )
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiPromptTokensDetails")]
pub struct PyOpenAiPromptTokensDetails {
    inner: Arc<ProviderResponse>,
}
impl PyOpenAiPromptTokensDetails {
    fn d(&self) -> &OpenAiPromptTokensDetails {
        match self.inner.as_ref() {
            ProviderResponse::OpenAiChatCompletion(r) => r
                .usage
                .as_ref()
                .expect("guarded")
                .prompt_tokens_details
                .as_ref()
                .expect("guarded"),
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiPromptTokensDetails {
    #[getter]
    fn audio_tokens(&self) -> u64 {
        self.d().audio_tokens
    }
    #[getter]
    fn cached_tokens(&self) -> u64 {
        self.d().cached_tokens
    }
    fn __repr__(&self) -> String {
        "OpenAiPromptTokensDetails".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiCompletionTokensDetails")]
pub struct PyOpenAiCompletionTokensDetails {
    inner: Arc<ProviderResponse>,
}
impl PyOpenAiCompletionTokensDetails {
    fn d(&self) -> &OpenAiCompletionTokensDetails {
        match self.inner.as_ref() {
            ProviderResponse::OpenAiChatCompletion(r) => r
                .usage
                .as_ref()
                .expect("guarded")
                .completion_tokens_details
                .as_ref()
                .expect("guarded"),
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiCompletionTokensDetails {
    #[getter]
    fn accepted_prediction_tokens(&self) -> u64 {
        self.d().accepted_prediction_tokens
    }
    #[getter]
    fn audio_tokens(&self) -> u64 {
        self.d().audio_tokens
    }
    #[getter]
    fn reasoning_tokens(&self) -> u64 {
        self.d().reasoning_tokens
    }
    #[getter]
    fn rejected_prediction_tokens(&self) -> u64 {
        self.d().rejected_prediction_tokens
    }
    fn __repr__(&self) -> String {
        "OpenAiCompletionTokensDetails".to_owned()
    }
}

pub(super) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyOpenAiChatResponse>()?;
    module.add_class::<PyOpenAiChatChoice>()?;
    module.add_class::<PyOpenAiChatLogprobs>()?;
    module.add_class::<PyOpenAiUsage>()?;
    module.add_class::<PyOpenAiPromptTokensDetails>()?;
    module.add_class::<PyOpenAiCompletionTokensDetails>()?;
    Ok(())
}
