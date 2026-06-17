//! `PyOpenAi*` request-side wrappers — settings, tools, tool-choice variants.
//!
//! Each wrapper holds an `Arc<ProviderRequest>` plus the index (or other
//! locator) into the target field. Variant projection is guarded by
//! construction: callers only build these wrappers from a
//! `ProviderRequest::OpenAiChatCompletion` / `OpenAiChatCompatible`, so the
//! `.expect("guarded")` calls and `unreachable!()` arms below assert that
//! invariant, not a runtime fact.

use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyModule;
use skald_spec::ProviderRequest;
use skald_spec::wire::openai_chat::{
    OpenAiAllowedTools, OpenAiAllowedToolsChoice, OpenAiAllowedToolsKind, OpenAiAllowedToolsMode,
    OpenAiAudioFormat, OpenAiBuiltInVoice, OpenAiChatAudio, OpenAiChatRequest,
    OpenAiChatToolChoice, OpenAiCustomChoice, OpenAiCustomTool, OpenAiCustomToolFormat,
    OpenAiFunctionChoice, OpenAiGrammar, OpenAiGrammarSyntax, OpenAiNamedCustomToolChoice,
    OpenAiNamedCustomToolChoiceKind, OpenAiNamedFunctionToolChoice,
    OpenAiNamedFunctionToolChoiceKind, OpenAiPredictionContent, OpenAiPredictionContentPart,
    OpenAiPredictionKind, OpenAiPredictionPayload, OpenAiReasoningEffort, OpenAiResponseFormat,
    OpenAiResponseModality, OpenAiStop, OpenAiStreamOptions, OpenAiTool, OpenAiToolChoiceMode,
    OpenAiVoice,
};
use wyrd_interfaces::error::CardPyResult;

use super::openai_chat_messages::PyOpenAiChatMessage;
use super::shared::ChatMessageSource;
use crate::prompt::wrong_variant;

// ──────────────────────────────────────────────────────────────────────────────
// OpenAI Chat — request side
// ──────────────────────────────────────────────────────────────────────────────

#[pyclass(module = "wyrd.prompt", name = "OpenAiChatRequest")]
pub struct PyOpenAiChatRequest {
    pub(crate) inner: Arc<ProviderRequest>,
}

impl PyOpenAiChatRequest {
    pub fn new(inner: Arc<ProviderRequest>) -> Self {
        Self { inner }
    }
    fn req(&self) -> &OpenAiChatRequest {
        match self.inner.as_ref() {
            ProviderRequest::OpenAiChatCompletion(r) => r,
            ProviderRequest::OpenAiChatCompatible { request, .. } => request,
            _ => unreachable!(),
        }
    }
}

#[pymethods]
impl PyOpenAiChatRequest {
    #[getter]
    fn model(&self) -> &str {
        &self.req().model
    }
    #[getter]
    fn messages(&self) -> Vec<PyOpenAiChatMessage> {
        (0..self.req().messages.len())
            .map(|i| PyOpenAiChatMessage {
                src: ChatMessageSource::Request {
                    inner: Arc::clone(&self.inner),
                    index: i,
                },
            })
            .collect()
    }
    #[getter]
    fn response_format(&self) -> Option<PyOpenAiResponseFormat> {
        self.req()
            .response_format
            .as_ref()
            .map(|_| PyOpenAiResponseFormat {
                inner: Arc::clone(&self.inner),
            })
    }
    #[getter]
    fn stream(&self) -> Option<bool> {
        self.req().stream
    }
    #[getter]
    fn stream_options(&self) -> Option<PyOpenAiStreamOptions> {
        self.req()
            .stream_options
            .as_ref()
            .map(|_| PyOpenAiStreamOptions {
                inner: Arc::clone(&self.inner),
            })
    }
    #[getter]
    fn tools(&self) -> Vec<PyOpenAiTool> {
        let n = self.req().tools.as_ref().map_or(0, |t| t.len());
        (0..n)
            .map(|i| PyOpenAiTool {
                inner: Arc::clone(&self.inner),
                index: i,
            })
            .collect()
    }
    #[getter]
    fn tool_choice(&self) -> Option<PyOpenAiChatToolChoice> {
        self.req()
            .tool_choice
            .as_ref()
            .map(|_| PyOpenAiChatToolChoice {
                inner: Arc::clone(&self.inner),
            })
    }
    #[getter]
    fn parallel_tool_calls(&self) -> Option<bool> {
        self.req().parallel_tool_calls
    }
    #[getter]
    fn settings(&self) -> PyOpenAiChatSettings {
        PyOpenAiChatSettings {
            inner: Arc::clone(&self.inner),
        }
    }
    fn __repr__(&self) -> String {
        format!("OpenAiChatRequest(model={:?})", self.req().model)
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiChatSettings")]
pub struct PyOpenAiChatSettings {
    inner: Arc<ProviderRequest>,
}
impl PyOpenAiChatSettings {
    fn s(&self) -> &skald_spec::wire::openai_chat::OpenAiChatSettings {
        match self.inner.as_ref() {
            ProviderRequest::OpenAiChatCompletion(r) => &r.settings,
            ProviderRequest::OpenAiChatCompatible { request, .. } => &request.settings,
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiChatSettings {
    #[getter]
    fn temperature(&self) -> Option<f32> {
        self.s().temperature
    }
    #[getter]
    fn top_p(&self) -> Option<f32> {
        self.s().top_p
    }
    #[getter]
    fn max_tokens(&self) -> Option<u32> {
        self.s().max_tokens
    }
    #[getter]
    fn max_completion_tokens(&self) -> Option<u32> {
        self.s().max_completion_tokens
    }
    #[getter]
    fn n(&self) -> Option<u32> {
        self.s().n
    }
    #[getter]
    fn stop(&self) -> Option<PyOpenAiStop> {
        self.s().stop.as_ref().map(|_| PyOpenAiStop {
            inner: Arc::clone(&self.inner),
        })
    }
    #[getter]
    fn presence_penalty(&self) -> Option<f32> {
        self.s().presence_penalty
    }
    #[getter]
    fn frequency_penalty(&self) -> Option<f32> {
        self.s().frequency_penalty
    }
    #[getter]
    fn seed(&self) -> Option<i64> {
        self.s().seed
    }
    #[getter]
    fn logit_bias(&self, py: Python<'_>) -> CardPyResult<Option<Py<PyAny>>> {
        self.s()
            .logit_bias
            .as_ref()
            .map(|m| {
                wyrd_utils::py::json_to_pyobject(py, &serde_json::Value::Object(m.clone()))
                    .map_err(Into::into)
            })
            .transpose()
    }
    #[getter]
    fn user(&self) -> Option<&str> {
        self.s().user.as_deref()
    }
    #[getter]
    fn reasoning_effort(&self) -> Option<&'static str> {
        self.s()
            .reasoning_effort
            .as_ref()
            .map(openai_reasoning_effort_str)
    }
    #[getter]
    fn modalities(&self) -> Option<Vec<&'static str>> {
        self.s()
            .modalities
            .as_ref()
            .map(|ms| ms.iter().map(openai_response_modality_str).collect())
    }
    #[getter]
    fn audio(&self) -> Option<PyOpenAiChatAudio> {
        self.s().audio.as_ref().map(|_| PyOpenAiChatAudio {
            inner: Arc::clone(&self.inner),
        })
    }
    #[getter]
    fn prediction(&self) -> Option<PyOpenAiPredictionContent> {
        self.s()
            .prediction
            .as_ref()
            .map(|_| PyOpenAiPredictionContent {
                inner: Arc::clone(&self.inner),
            })
    }
    #[getter]
    fn prompt_cache_key(&self) -> Option<&str> {
        self.s().prompt_cache_key.as_deref()
    }
    #[getter]
    fn service_tier(&self) -> Option<&str> {
        self.s().service_tier.as_deref()
    }
    #[getter]
    fn safety_identifier(&self) -> Option<&str> {
        self.s().safety_identifier.as_deref()
    }
    #[getter]
    fn store(&self) -> Option<bool> {
        self.s().store
    }
    #[getter]
    fn metadata(&self, py: Python<'_>) -> CardPyResult<Option<Py<PyAny>>> {
        self.s()
            .metadata
            .as_ref()
            .map(|m| {
                wyrd_utils::py::json_to_pyobject(py, &serde_json::Value::Object(m.clone()))
                    .map_err(Into::into)
            })
            .transpose()
    }
    #[getter]
    fn logprobs(&self) -> Option<bool> {
        self.s().logprobs
    }
    #[getter]
    fn top_logprobs(&self) -> Option<u32> {
        self.s().top_logprobs
    }
    #[getter]
    fn extra(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        wyrd_utils::py::json_to_pyobject(py, &serde_json::Value::Object(self.s().extra.clone()))
            .map_err(Into::into)
    }
    fn __repr__(&self) -> String {
        "OpenAiChatSettings".to_owned()
    }
}

fn openai_reasoning_effort_str(e: &OpenAiReasoningEffort) -> &'static str {
    match e {
        OpenAiReasoningEffort::None => "none",
        OpenAiReasoningEffort::Minimal => "minimal",
        OpenAiReasoningEffort::Low => "low",
        OpenAiReasoningEffort::Medium => "medium",
        OpenAiReasoningEffort::High => "high",
        OpenAiReasoningEffort::Xhigh => "xhigh",
    }
}
fn openai_response_modality_str(m: &OpenAiResponseModality) -> &'static str {
    match m {
        OpenAiResponseModality::Text => "text",
        OpenAiResponseModality::Audio => "audio",
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiStop")]
pub struct PyOpenAiStop {
    inner: Arc<ProviderRequest>,
}
impl PyOpenAiStop {
    fn s(&self) -> &OpenAiStop {
        match self.inner.as_ref() {
            ProviderRequest::OpenAiChatCompletion(r) => r.settings.stop.as_ref().expect("guarded"),
            ProviderRequest::OpenAiChatCompatible { request, .. } => {
                request.settings.stop.as_ref().expect("guarded")
            }
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiStop {
    #[getter]
    fn kind(&self) -> &'static str {
        match self.s() {
            OpenAiStop::One(_) => "one",
            OpenAiStop::Many(_) => "many",
        }
    }
    fn as_one(&self) -> CardPyResult<String> {
        match self.s() {
            OpenAiStop::One(s) => Ok(s.clone()),
            _ => Err(wrong_variant("one", self.kind()).into()),
        }
    }
    fn as_many(&self) -> CardPyResult<Vec<String>> {
        match self.s() {
            OpenAiStop::Many(v) => Ok(v.clone()),
            _ => Err(wrong_variant("many", self.kind()).into()),
        }
    }
    fn __repr__(&self) -> String {
        format!("OpenAiStop(kind={:?})", self.kind())
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiChatAudio")]
pub struct PyOpenAiChatAudio {
    inner: Arc<ProviderRequest>,
}
impl PyOpenAiChatAudio {
    fn a(&self) -> &OpenAiChatAudio {
        match self.inner.as_ref() {
            ProviderRequest::OpenAiChatCompletion(r) => r.settings.audio.as_ref().expect("guarded"),
            ProviderRequest::OpenAiChatCompatible { request, .. } => {
                request.settings.audio.as_ref().expect("guarded")
            }
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiChatAudio {
    #[getter]
    fn voice(&self) -> PyOpenAiVoice {
        PyOpenAiVoice {
            inner: Arc::clone(&self.inner),
        }
    }
    #[getter]
    fn format(&self) -> &'static str {
        match self.a().format {
            OpenAiAudioFormat::Wav => "wav",
            OpenAiAudioFormat::Aac => "aac",
            OpenAiAudioFormat::Mp3 => "mp3",
            OpenAiAudioFormat::Flac => "flac",
            OpenAiAudioFormat::Opus => "opus",
            OpenAiAudioFormat::Pcm16 => "pcm16",
        }
    }
    fn __repr__(&self) -> String {
        "OpenAiChatAudio".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiVoice")]
pub struct PyOpenAiVoice {
    inner: Arc<ProviderRequest>,
}
impl PyOpenAiVoice {
    fn v(&self) -> &OpenAiVoice {
        match self.inner.as_ref() {
            ProviderRequest::OpenAiChatCompletion(r) => {
                &r.settings.audio.as_ref().expect("guarded").voice
            }
            ProviderRequest::OpenAiChatCompatible { request, .. } => {
                &request.settings.audio.as_ref().expect("guarded").voice
            }
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiVoice {
    #[getter]
    fn kind(&self) -> &'static str {
        match self.v() {
            OpenAiVoice::BuiltIn(_) => "built_in",
            OpenAiVoice::Custom(_) => "custom",
        }
    }
    fn as_built_in(&self) -> CardPyResult<&'static str> {
        match self.v() {
            OpenAiVoice::BuiltIn(b) => Ok(openai_built_in_voice_str(b)),
            _ => Err(wrong_variant("built_in", self.kind()).into()),
        }
    }
    fn as_custom(&self) -> CardPyResult<String> {
        match self.v() {
            OpenAiVoice::Custom(c) => Ok(c.id.clone()),
            _ => Err(wrong_variant("custom", self.kind()).into()),
        }
    }
    fn __repr__(&self) -> String {
        format!("OpenAiVoice(kind={:?})", self.kind())
    }
}
fn openai_built_in_voice_str(v: &OpenAiBuiltInVoice) -> &'static str {
    match v {
        OpenAiBuiltInVoice::Alloy => "alloy",
        OpenAiBuiltInVoice::Ash => "ash",
        OpenAiBuiltInVoice::Ballad => "ballad",
        OpenAiBuiltInVoice::Coral => "coral",
        OpenAiBuiltInVoice::Echo => "echo",
        OpenAiBuiltInVoice::Fable => "fable",
        OpenAiBuiltInVoice::Nova => "nova",
        OpenAiBuiltInVoice::Onyx => "onyx",
        OpenAiBuiltInVoice::Sage => "sage",
        OpenAiBuiltInVoice::Shimmer => "shimmer",
        OpenAiBuiltInVoice::Marin => "marin",
        OpenAiBuiltInVoice::Cedar => "cedar",
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiPredictionContent")]
pub struct PyOpenAiPredictionContent {
    inner: Arc<ProviderRequest>,
}
impl PyOpenAiPredictionContent {
    fn p(&self) -> &OpenAiPredictionContent {
        match self.inner.as_ref() {
            ProviderRequest::OpenAiChatCompletion(r) => {
                r.settings.prediction.as_ref().expect("guarded")
            }
            ProviderRequest::OpenAiChatCompatible { request, .. } => {
                request.settings.prediction.as_ref().expect("guarded")
            }
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiPredictionContent {
    #[getter]
    fn kind(&self) -> &'static str {
        match self.p().kind {
            OpenAiPredictionKind::Content => "content",
        }
    }
    #[getter]
    fn content(&self) -> PyOpenAiPredictionPayload {
        PyOpenAiPredictionPayload {
            inner: Arc::clone(&self.inner),
        }
    }
    fn __repr__(&self) -> String {
        "OpenAiPredictionContent".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiPredictionPayload")]
pub struct PyOpenAiPredictionPayload {
    inner: Arc<ProviderRequest>,
}
impl PyOpenAiPredictionPayload {
    fn p(&self) -> &OpenAiPredictionPayload {
        &self
            .inner
            .as_ref()
            .prediction_ref()
            .expect("guarded")
            .content
    }
}

// Helper trait to reach prediction content from the request
trait PredictionRef {
    fn prediction_ref(&self) -> Option<&OpenAiPredictionContent>;
}
impl PredictionRef for ProviderRequest {
    fn prediction_ref(&self) -> Option<&OpenAiPredictionContent> {
        match self {
            ProviderRequest::OpenAiChatCompletion(r) => r.settings.prediction.as_ref(),
            ProviderRequest::OpenAiChatCompatible { request, .. } => {
                request.settings.prediction.as_ref()
            }
            _ => None,
        }
    }
}

#[pymethods]
impl PyOpenAiPredictionPayload {
    #[getter]
    fn kind(&self) -> &'static str {
        match self.p() {
            OpenAiPredictionPayload::Text(_) => "text",
            OpenAiPredictionPayload::Parts(_) => "parts",
        }
    }
    fn as_text(&self) -> CardPyResult<String> {
        match self.p() {
            OpenAiPredictionPayload::Text(s) => Ok(s.clone()),
            _ => Err(wrong_variant("text", self.kind()).into()),
        }
    }
    fn as_parts(&self) -> CardPyResult<Vec<PyOpenAiPredictionContentPart>> {
        match self.p() {
            OpenAiPredictionPayload::Parts(ps) => Ok((0..ps.len())
                .map(|i| PyOpenAiPredictionContentPart {
                    inner: Arc::clone(&self.inner),
                    index: i,
                })
                .collect()),
            _ => Err(wrong_variant("parts", self.kind()).into()),
        }
    }
    fn __repr__(&self) -> String {
        format!("OpenAiPredictionPayload(kind={:?})", self.kind())
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiPredictionContentPart")]
pub struct PyOpenAiPredictionContentPart {
    inner: Arc<ProviderRequest>,
    index: usize,
}
impl PyOpenAiPredictionContentPart {
    fn p(&self) -> &OpenAiPredictionContentPart {
        match self.inner.prediction_ref().expect("guarded").content {
            OpenAiPredictionPayload::Parts(ref ps) => &ps[self.index],
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiPredictionContentPart {
    #[getter]
    fn kind(&self) -> &'static str {
        match self.p() {
            OpenAiPredictionContentPart::Text { .. } => "text",
        }
    }
    fn as_text(&self) -> CardPyResult<String> {
        match self.p() {
            OpenAiPredictionContentPart::Text { text } => Ok(text.clone()),
        }
    }
    fn __repr__(&self) -> String {
        format!("OpenAiPredictionContentPart(kind={:?})", self.kind())
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiStreamOptions")]
pub struct PyOpenAiStreamOptions {
    inner: Arc<ProviderRequest>,
}
impl PyOpenAiStreamOptions {
    fn s(&self) -> &OpenAiStreamOptions {
        match self.inner.as_ref() {
            ProviderRequest::OpenAiChatCompletion(r) => r.stream_options.as_ref().expect("guarded"),
            ProviderRequest::OpenAiChatCompatible { request, .. } => {
                request.stream_options.as_ref().expect("guarded")
            }
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiStreamOptions {
    #[getter]
    fn include_usage(&self) -> Option<bool> {
        self.s().include_usage
    }
    #[getter]
    fn include_obfuscation(&self) -> Option<bool> {
        self.s().include_obfuscation
    }
    fn __repr__(&self) -> String {
        "OpenAiStreamOptions".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiResponseFormat")]
pub struct PyOpenAiResponseFormat {
    inner: Arc<ProviderRequest>,
}
impl PyOpenAiResponseFormat {
    fn f(&self) -> &OpenAiResponseFormat {
        match self.inner.as_ref() {
            ProviderRequest::OpenAiChatCompletion(r) => {
                r.response_format.as_ref().expect("guarded")
            }
            ProviderRequest::OpenAiChatCompatible { request, .. } => {
                request.response_format.as_ref().expect("guarded")
            }
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiResponseFormat {
    #[getter]
    fn kind(&self) -> &'static str {
        match self.f() {
            OpenAiResponseFormat::Text => "text",
            OpenAiResponseFormat::JsonObject => "json_object",
            OpenAiResponseFormat::JsonSchema { .. } => "json_schema",
        }
    }
    fn as_json_schema(&self) -> CardPyResult<PyOpenAiJsonSchema> {
        match self.f() {
            OpenAiResponseFormat::JsonSchema { .. } => Ok(PyOpenAiJsonSchema {
                inner: Arc::clone(&self.inner),
            }),
            _ => Err(wrong_variant("json_schema", self.kind()).into()),
        }
    }
    fn __repr__(&self) -> String {
        format!("OpenAiResponseFormat(kind={:?})", self.kind())
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiJsonSchema")]
pub struct PyOpenAiJsonSchema {
    inner: Arc<ProviderRequest>,
}
impl PyOpenAiJsonSchema {
    fn s(&self) -> &skald_spec::wire::openai_chat::OpenAiJsonSchema {
        match self.inner.as_ref() {
            ProviderRequest::OpenAiChatCompletion(r) => {
                match r.response_format.as_ref().expect("guarded") {
                    OpenAiResponseFormat::JsonSchema { json_schema } => json_schema,
                    _ => unreachable!(),
                }
            }
            ProviderRequest::OpenAiChatCompatible { request, .. } => {
                match request.response_format.as_ref().expect("guarded") {
                    OpenAiResponseFormat::JsonSchema { json_schema } => json_schema,
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiJsonSchema {
    #[getter]
    fn name(&self) -> &str {
        &self.s().name
    }
    #[getter]
    fn description(&self) -> Option<&str> {
        self.s().description.as_deref()
    }
    #[getter]
    fn schema(&self, py: Python<'_>) -> CardPyResult<Option<Py<PyAny>>> {
        self.s()
            .schema
            .as_ref()
            .map(|m| {
                wyrd_utils::py::json_to_pyobject(py, &serde_json::Value::Object(m.clone()))
                    .map_err(Into::into)
            })
            .transpose()
    }
    #[getter]
    fn strict(&self) -> Option<bool> {
        self.s().strict
    }
    fn __repr__(&self) -> String {
        format!("OpenAiJsonSchema(name={:?})", self.s().name)
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiTool")]
pub struct PyOpenAiTool {
    inner: Arc<ProviderRequest>,
    index: usize,
}
impl PyOpenAiTool {
    fn t(&self) -> &OpenAiTool {
        match self.inner.as_ref() {
            ProviderRequest::OpenAiChatCompletion(r) => {
                &r.tools.as_ref().expect("guarded")[self.index]
            }
            ProviderRequest::OpenAiChatCompatible { request, .. } => {
                &request.tools.as_ref().expect("guarded")[self.index]
            }
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiTool {
    #[getter]
    fn kind(&self) -> &'static str {
        match self.t() {
            OpenAiTool::Function { .. } => "function",
            OpenAiTool::Custom { .. } => "custom",
        }
    }
    fn as_function(&self) -> CardPyResult<PyOpenAiFunction> {
        match self.t() {
            OpenAiTool::Function { .. } => Ok(PyOpenAiFunction {
                inner: Arc::clone(&self.inner),
                tool_index: self.index,
            }),
            _ => Err(wrong_variant("function", self.kind()).into()),
        }
    }
    fn as_custom_tool(&self) -> CardPyResult<PyOpenAiCustomTool> {
        match self.t() {
            OpenAiTool::Custom { .. } => Ok(PyOpenAiCustomTool {
                inner: Arc::clone(&self.inner),
                tool_index: self.index,
            }),
            _ => Err(wrong_variant("custom", self.kind()).into()),
        }
    }
    fn __repr__(&self) -> String {
        format!("OpenAiTool(kind={:?})", self.kind())
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiFunction")]
pub struct PyOpenAiFunction {
    inner: Arc<ProviderRequest>,
    tool_index: usize,
}
impl PyOpenAiFunction {
    fn f(&self) -> &skald_spec::wire::openai_chat::OpenAiFunction {
        match self.inner.as_ref() {
            ProviderRequest::OpenAiChatCompletion(r) => {
                match &r.tools.as_ref().expect("guarded")[self.tool_index] {
                    OpenAiTool::Function { function } => function,
                    _ => unreachable!(),
                }
            }
            ProviderRequest::OpenAiChatCompatible { request, .. } => {
                match &request.tools.as_ref().expect("guarded")[self.tool_index] {
                    OpenAiTool::Function { function } => function,
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiFunction {
    #[getter]
    fn name(&self) -> &str {
        &self.f().name
    }
    #[getter]
    fn description(&self) -> Option<&str> {
        self.f().description.as_deref()
    }
    #[getter]
    fn parameters(&self, py: Python<'_>) -> CardPyResult<Option<Py<PyAny>>> {
        self.f()
            .parameters
            .as_ref()
            .map(|m| {
                wyrd_utils::py::json_to_pyobject(py, &serde_json::Value::Object(m.clone()))
                    .map_err(Into::into)
            })
            .transpose()
    }
    #[getter]
    fn strict(&self) -> Option<bool> {
        self.f().strict
    }
    fn __repr__(&self) -> String {
        format!("OpenAiFunction(name={:?})", self.f().name)
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiCustomTool")]
pub struct PyOpenAiCustomTool {
    inner: Arc<ProviderRequest>,
    tool_index: usize,
}
impl PyOpenAiCustomTool {
    fn c(&self) -> &OpenAiCustomTool {
        match self.inner.as_ref() {
            ProviderRequest::OpenAiChatCompletion(r) => {
                match &r.tools.as_ref().expect("guarded")[self.tool_index] {
                    OpenAiTool::Custom { custom } => custom,
                    _ => unreachable!(),
                }
            }
            ProviderRequest::OpenAiChatCompatible { request, .. } => {
                match &request.tools.as_ref().expect("guarded")[self.tool_index] {
                    OpenAiTool::Custom { custom } => custom,
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiCustomTool {
    #[getter]
    fn name(&self) -> &str {
        &self.c().name
    }
    #[getter]
    fn description(&self) -> Option<&str> {
        self.c().description.as_deref()
    }
    #[getter]
    fn format(&self) -> Option<PyOpenAiCustomToolFormat> {
        self.c().format.as_ref().map(|_| PyOpenAiCustomToolFormat {
            inner: Arc::clone(&self.inner),
            tool_index: self.tool_index,
        })
    }
    fn __repr__(&self) -> String {
        format!("OpenAiCustomTool(name={:?})", self.c().name)
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiCustomToolFormat")]
pub struct PyOpenAiCustomToolFormat {
    inner: Arc<ProviderRequest>,
    tool_index: usize,
}
impl PyOpenAiCustomToolFormat {
    fn f(&self) -> &OpenAiCustomToolFormat {
        match self.inner.as_ref() {
            ProviderRequest::OpenAiChatCompletion(r) => {
                match &r.tools.as_ref().expect("guarded")[self.tool_index] {
                    OpenAiTool::Custom { custom } => custom.format.as_ref().expect("guarded"),
                    _ => unreachable!(),
                }
            }
            ProviderRequest::OpenAiChatCompatible { request, .. } => {
                match &request.tools.as_ref().expect("guarded")[self.tool_index] {
                    OpenAiTool::Custom { custom } => custom.format.as_ref().expect("guarded"),
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiCustomToolFormat {
    #[getter]
    fn kind(&self) -> &'static str {
        match self.f() {
            OpenAiCustomToolFormat::Text => "text",
            OpenAiCustomToolFormat::Grammar { .. } => "grammar",
        }
    }
    fn as_grammar(&self) -> CardPyResult<PyOpenAiGrammar> {
        match self.f() {
            OpenAiCustomToolFormat::Grammar { .. } => Ok(PyOpenAiGrammar {
                inner: Arc::clone(&self.inner),
                tool_index: self.tool_index,
            }),
            _ => Err(wrong_variant("grammar", self.kind()).into()),
        }
    }
    fn __repr__(&self) -> String {
        format!("OpenAiCustomToolFormat(kind={:?})", self.kind())
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiGrammar")]
pub struct PyOpenAiGrammar {
    inner: Arc<ProviderRequest>,
    tool_index: usize,
}
impl PyOpenAiGrammar {
    fn g(&self) -> &OpenAiGrammar {
        match self.inner.as_ref() {
            ProviderRequest::OpenAiChatCompletion(r) => match &r.tools.as_ref().expect("guarded")
                [self.tool_index]
            {
                OpenAiTool::Custom { custom } => match custom.format.as_ref().expect("guarded") {
                    OpenAiCustomToolFormat::Grammar { grammar } => grammar,
                    OpenAiCustomToolFormat::Text => unreachable!(),
                },
                OpenAiTool::Function { .. } => unreachable!(),
            },
            ProviderRequest::OpenAiChatCompatible { request, .. } => {
                match &request.tools.as_ref().expect("guarded")[self.tool_index] {
                    OpenAiTool::Custom { custom } => match custom.format.as_ref().expect("guarded")
                    {
                        OpenAiCustomToolFormat::Grammar { grammar } => grammar,
                        OpenAiCustomToolFormat::Text => unreachable!(),
                    },
                    OpenAiTool::Function { .. } => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiGrammar {
    #[getter]
    fn definition(&self) -> &str {
        &self.g().definition
    }
    #[getter]
    fn syntax(&self) -> &'static str {
        match self.g().syntax {
            OpenAiGrammarSyntax::Lark => "lark",
            OpenAiGrammarSyntax::Regex => "regex",
        }
    }
    fn __repr__(&self) -> String {
        "OpenAiGrammar".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiChatToolChoice")]
pub struct PyOpenAiChatToolChoice {
    inner: Arc<ProviderRequest>,
}
impl PyOpenAiChatToolChoice {
    fn c(&self) -> &OpenAiChatToolChoice {
        match self.inner.as_ref() {
            ProviderRequest::OpenAiChatCompletion(r) => r.tool_choice.as_ref().expect("guarded"),
            ProviderRequest::OpenAiChatCompatible { request, .. } => {
                request.tool_choice.as_ref().expect("guarded")
            }
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiChatToolChoice {
    #[getter]
    fn kind(&self) -> &'static str {
        match self.c() {
            OpenAiChatToolChoice::Mode(_) => "mode",
            OpenAiChatToolChoice::Allowed(_) => "allowed",
            OpenAiChatToolChoice::Function(_) => "function",
            OpenAiChatToolChoice::Custom(_) => "custom",
        }
    }
    fn as_mode(&self) -> CardPyResult<&'static str> {
        match self.c() {
            OpenAiChatToolChoice::Mode(m) => Ok(match m {
                OpenAiToolChoiceMode::None => "none",
                OpenAiToolChoiceMode::Auto => "auto",
                OpenAiToolChoiceMode::Required => "required",
            }),
            _ => Err(wrong_variant("mode", self.kind()).into()),
        }
    }
    fn as_allowed(&self) -> CardPyResult<PyOpenAiAllowedToolsChoice> {
        match self.c() {
            OpenAiChatToolChoice::Allowed(_) => Ok(PyOpenAiAllowedToolsChoice {
                inner: Arc::clone(&self.inner),
            }),
            _ => Err(wrong_variant("allowed", self.kind()).into()),
        }
    }
    fn as_function_choice(&self) -> CardPyResult<PyOpenAiNamedFunctionToolChoice> {
        match self.c() {
            OpenAiChatToolChoice::Function(_) => Ok(PyOpenAiNamedFunctionToolChoice {
                inner: Arc::clone(&self.inner),
            }),
            _ => Err(wrong_variant("function", self.kind()).into()),
        }
    }
    fn as_custom_choice(&self) -> CardPyResult<PyOpenAiNamedCustomToolChoice> {
        match self.c() {
            OpenAiChatToolChoice::Custom(_) => Ok(PyOpenAiNamedCustomToolChoice {
                inner: Arc::clone(&self.inner),
            }),
            _ => Err(wrong_variant("custom", self.kind()).into()),
        }
    }
    fn __repr__(&self) -> String {
        format!("OpenAiChatToolChoice(kind={:?})", self.kind())
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiAllowedToolsChoice")]
pub struct PyOpenAiAllowedToolsChoice {
    inner: Arc<ProviderRequest>,
}
impl PyOpenAiAllowedToolsChoice {
    fn a(&self) -> &OpenAiAllowedToolsChoice {
        match self.inner.as_ref() {
            ProviderRequest::OpenAiChatCompletion(r) => {
                match r.tool_choice.as_ref().expect("guarded") {
                    OpenAiChatToolChoice::Allowed(a) => a,
                    _ => unreachable!(),
                }
            }
            ProviderRequest::OpenAiChatCompatible { request, .. } => {
                match request.tool_choice.as_ref().expect("guarded") {
                    OpenAiChatToolChoice::Allowed(a) => a,
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiAllowedToolsChoice {
    #[getter]
    fn kind(&self) -> &'static str {
        match self.a().kind {
            OpenAiAllowedToolsKind::AllowedTools => "allowed_tools",
        }
    }
    #[getter]
    fn allowed_tools(&self) -> PyOpenAiAllowedTools {
        PyOpenAiAllowedTools {
            inner: Arc::clone(&self.inner),
        }
    }
    fn __repr__(&self) -> String {
        "OpenAiAllowedToolsChoice".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiAllowedTools")]
pub struct PyOpenAiAllowedTools {
    inner: Arc<ProviderRequest>,
}
impl PyOpenAiAllowedTools {
    fn a(&self) -> &OpenAiAllowedTools {
        match self.inner.as_ref() {
            ProviderRequest::OpenAiChatCompletion(r) => {
                match r.tool_choice.as_ref().expect("guarded") {
                    OpenAiChatToolChoice::Allowed(a) => &a.allowed_tools,
                    _ => unreachable!(),
                }
            }
            ProviderRequest::OpenAiChatCompatible { request, .. } => {
                match request.tool_choice.as_ref().expect("guarded") {
                    OpenAiChatToolChoice::Allowed(a) => &a.allowed_tools,
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiAllowedTools {
    #[getter]
    fn mode(&self) -> &'static str {
        match self.a().mode {
            OpenAiAllowedToolsMode::Auto => "auto",
            OpenAiAllowedToolsMode::Required => "required",
        }
    }
    #[getter]
    fn tools(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        let arr = serde_json::Value::Array(
            self.a()
                .tools
                .iter()
                .map(|m| serde_json::Value::Object(m.clone()))
                .collect(),
        );
        wyrd_utils::py::json_to_pyobject(py, &arr).map_err(Into::into)
    }
    fn __repr__(&self) -> String {
        "OpenAiAllowedTools".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiNamedFunctionToolChoice")]
pub struct PyOpenAiNamedFunctionToolChoice {
    inner: Arc<ProviderRequest>,
}
impl PyOpenAiNamedFunctionToolChoice {
    fn n(&self) -> &OpenAiNamedFunctionToolChoice {
        match self.inner.as_ref() {
            ProviderRequest::OpenAiChatCompletion(r) => {
                match r.tool_choice.as_ref().expect("guarded") {
                    OpenAiChatToolChoice::Function(f) => f,
                    _ => unreachable!(),
                }
            }
            ProviderRequest::OpenAiChatCompatible { request, .. } => {
                match request.tool_choice.as_ref().expect("guarded") {
                    OpenAiChatToolChoice::Function(f) => f,
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiNamedFunctionToolChoice {
    #[getter]
    fn kind(&self) -> &'static str {
        match self.n().kind {
            OpenAiNamedFunctionToolChoiceKind::Function => "function",
        }
    }
    #[getter]
    fn function(&self) -> PyOpenAiFunctionChoice {
        PyOpenAiFunctionChoice {
            inner: Arc::clone(&self.inner),
        }
    }
    fn __repr__(&self) -> String {
        "OpenAiNamedFunctionToolChoice".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiFunctionChoice")]
pub struct PyOpenAiFunctionChoice {
    inner: Arc<ProviderRequest>,
}
impl PyOpenAiFunctionChoice {
    fn f(&self) -> &OpenAiFunctionChoice {
        match self.inner.as_ref() {
            ProviderRequest::OpenAiChatCompletion(r) => {
                match r.tool_choice.as_ref().expect("guarded") {
                    OpenAiChatToolChoice::Function(f) => &f.function,
                    _ => unreachable!(),
                }
            }
            ProviderRequest::OpenAiChatCompatible { request, .. } => {
                match request.tool_choice.as_ref().expect("guarded") {
                    OpenAiChatToolChoice::Function(f) => &f.function,
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiFunctionChoice {
    #[getter]
    fn name(&self) -> &str {
        &self.f().name
    }
    fn __repr__(&self) -> String {
        format!("OpenAiFunctionChoice(name={:?})", self.f().name)
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiNamedCustomToolChoice")]
pub struct PyOpenAiNamedCustomToolChoice {
    inner: Arc<ProviderRequest>,
}
impl PyOpenAiNamedCustomToolChoice {
    fn n(&self) -> &OpenAiNamedCustomToolChoice {
        match self.inner.as_ref() {
            ProviderRequest::OpenAiChatCompletion(r) => {
                match r.tool_choice.as_ref().expect("guarded") {
                    OpenAiChatToolChoice::Custom(c) => c,
                    _ => unreachable!(),
                }
            }
            ProviderRequest::OpenAiChatCompatible { request, .. } => {
                match request.tool_choice.as_ref().expect("guarded") {
                    OpenAiChatToolChoice::Custom(c) => c,
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiNamedCustomToolChoice {
    #[getter]
    fn kind(&self) -> &'static str {
        match self.n().kind {
            OpenAiNamedCustomToolChoiceKind::Custom => "custom",
        }
    }
    #[getter]
    fn custom(&self) -> PyOpenAiCustomChoice {
        PyOpenAiCustomChoice {
            inner: Arc::clone(&self.inner),
        }
    }
    fn __repr__(&self) -> String {
        "OpenAiNamedCustomToolChoice".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiCustomChoice")]
pub struct PyOpenAiCustomChoice {
    inner: Arc<ProviderRequest>,
}
impl PyOpenAiCustomChoice {
    fn c(&self) -> &OpenAiCustomChoice {
        match self.inner.as_ref() {
            ProviderRequest::OpenAiChatCompletion(r) => {
                match r.tool_choice.as_ref().expect("guarded") {
                    OpenAiChatToolChoice::Custom(c) => &c.custom,
                    _ => unreachable!(),
                }
            }
            ProviderRequest::OpenAiChatCompatible { request, .. } => {
                match request.tool_choice.as_ref().expect("guarded") {
                    OpenAiChatToolChoice::Custom(c) => &c.custom,
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiCustomChoice {
    #[getter]
    fn name(&self) -> &str {
        &self.c().name
    }
    fn __repr__(&self) -> String {
        format!("OpenAiCustomChoice(name={:?})", self.c().name)
    }
}

pub(super) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyOpenAiChatRequest>()?;
    module.add_class::<PyOpenAiChatSettings>()?;
    module.add_class::<PyOpenAiStop>()?;
    module.add_class::<PyOpenAiChatAudio>()?;
    module.add_class::<PyOpenAiVoice>()?;
    module.add_class::<PyOpenAiPredictionContent>()?;
    module.add_class::<PyOpenAiPredictionPayload>()?;
    module.add_class::<PyOpenAiPredictionContentPart>()?;
    module.add_class::<PyOpenAiStreamOptions>()?;
    module.add_class::<PyOpenAiResponseFormat>()?;
    module.add_class::<PyOpenAiJsonSchema>()?;
    module.add_class::<PyOpenAiTool>()?;
    module.add_class::<PyOpenAiFunction>()?;
    module.add_class::<PyOpenAiCustomTool>()?;
    module.add_class::<PyOpenAiCustomToolFormat>()?;
    module.add_class::<PyOpenAiGrammar>()?;
    module.add_class::<PyOpenAiChatToolChoice>()?;
    module.add_class::<PyOpenAiAllowedToolsChoice>()?;
    module.add_class::<PyOpenAiAllowedTools>()?;
    module.add_class::<PyOpenAiNamedFunctionToolChoice>()?;
    module.add_class::<PyOpenAiFunctionChoice>()?;
    module.add_class::<PyOpenAiNamedCustomToolChoice>()?;
    module.add_class::<PyOpenAiCustomChoice>()?;
    Ok(())
}
