//! Typed Python pyclass mirrors for every in-scope provider wire struct.
//!
//! One clone per boundary crossing (callback/observer receives `PyProviderRequest` /
//! `PyProviderResponse`). Every nested field access after that is zero-copy via the `Arc`.

#![allow(missing_docs)]

use std::sync::Arc;

use pyo3::prelude::*;
use skald_spec::wire::anthropic_citation::AnthropicCitationV1;
use skald_spec::wire::anthropic_messages::{
    AnthropicCacheControl, AnthropicContentBlock, AnthropicDocumentSource, AnthropicImageSource,
    AnthropicMessage, AnthropicMessagesRequest, AnthropicMessagesResponse,
    AnthropicMessagesSettings, AnthropicOutputConfig, AnthropicOutputFormat, AnthropicStopReason,
    AnthropicSystem, AnthropicSystemBlock, AnthropicThinkingConfig, AnthropicTool,
    AnthropicToolResultContent, AnthropicUsage,
};
use skald_spec::wire::google_generate::{
    GoogleCandidate, GoogleCodeExecutionResult, GoogleContent, GoogleExecutableCode,
    GoogleFileData, GoogleFunctionCall, GoogleFunctionCallingConfig, GoogleFunctionDeclaration,
    GoogleFunctionResponse, GoogleGenerateContentRequest, GoogleGenerateContentResponse,
    GoogleGenerationConfig, GoogleInlineData, GooglePart, GoogleSafetyRating, GoogleSafetySetting,
    GoogleThinkingConfig, GoogleTool, GoogleToolConfig, GoogleUsageMetadata,
};
use skald_spec::wire::openai_chat::{
    OpenAiAllowedTools, OpenAiAllowedToolsChoice, OpenAiAllowedToolsKind, OpenAiAllowedToolsMode,
    OpenAiAudioFormat, OpenAiBuiltInVoice, OpenAiChatAudio, OpenAiChatChoice, OpenAiChatLogprobs,
    OpenAiChatMessage, OpenAiChatRequest, OpenAiChatResponse, OpenAiChatToolChoice,
    OpenAiCompletionTokensDetails, OpenAiContentPart, OpenAiCustomChoice, OpenAiCustomTool,
    OpenAiCustomToolFormat, OpenAiCustomVoice, OpenAiFilePart, OpenAiFunctionChoice, OpenAiGrammar,
    OpenAiGrammarSyntax, OpenAiImageUrl, OpenAiInputAudio, OpenAiJsonSchema,
    OpenAiMessageAnnotation, OpenAiMessageAudio, OpenAiMessageContent, OpenAiNamedCustomToolChoice,
    OpenAiNamedCustomToolChoiceKind, OpenAiNamedFunctionToolChoice,
    OpenAiNamedFunctionToolChoiceKind, OpenAiPredictionContent, OpenAiPredictionContentPart,
    OpenAiPredictionKind, OpenAiPredictionPayload, OpenAiPromptTokensDetails,
    OpenAiReasoningEffort, OpenAiResponseFormat, OpenAiResponseModality, OpenAiStop,
    OpenAiStreamOptions, OpenAiTool, OpenAiToolCall, OpenAiToolChoiceMode, OpenAiToolFunctionCall,
    OpenAiUrlCitation, OpenAiUsage, OpenAiVoice,
};
use skald_spec::wire::openai_responses::{
    OpenAiReasoning, OpenAiReasoningSummary, OpenAiResponseContentPart, OpenAiResponseItem,
    OpenAiResponsesAllowedToolsChoice, OpenAiResponsesAllowedToolsKind,
    OpenAiResponsesAllowedToolsMode, OpenAiResponsesApplyPatchToolChoice,
    OpenAiResponsesApplyPatchToolKind, OpenAiResponsesCustomToolChoice,
    OpenAiResponsesCustomToolFormat, OpenAiResponsesFunctionToolChoice,
    OpenAiResponsesFunctionToolKind, OpenAiResponsesGrammar, OpenAiResponsesGrammarSyntax,
    OpenAiResponsesHostedToolChoice, OpenAiResponsesHostedToolKind,
    OpenAiResponsesInputTokensDetails, OpenAiResponsesMcpToolChoice, OpenAiResponsesMcpToolKind,
    OpenAiResponsesOutputTokensDetails, OpenAiResponsesRequest, OpenAiResponsesResponse,
    OpenAiResponsesSettings, OpenAiResponsesShellToolChoice, OpenAiResponsesShellToolKind,
    OpenAiResponsesText, OpenAiResponsesTool, OpenAiResponsesToolChoice,
    OpenAiResponsesToolChoiceMode, OpenAiResponsesUsage, OpenAiTextResponseFormat,
};
use skald_spec::{ProviderRequest, ProviderResponse};
use wyrd_interfaces::error::CardPyResult;

use crate::prompt::{provider_name_to_string, wrong_provider, wrong_variant};

// ──────────────────────────────────────────────────────────────────────────────
// Top-level response wrapper
// ──────────────────────────────────────────────────────────────────────────────

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
    pub(crate) fn inner_arc(&self) -> Arc<ProviderResponse> {
        Arc::clone(&self.inner)
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

// ──────────────────────────────────────────────────────────────────────────────
// Shared message source enum (Rule 5)
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub enum ChatMessageSource {
    Request {
        inner: Arc<ProviderRequest>,
        index: usize,
    },
    Choice {
        inner: Arc<ProviderResponse>,
        choice_index: usize,
    },
}

impl ChatMessageSource {
    fn msg(&self) -> &OpenAiChatMessage {
        match self {
            Self::Request { inner, index } => match inner.as_ref() {
                ProviderRequest::OpenAiChatCompletion(r) => &r.messages[*index],
                ProviderRequest::OpenAiChatCompatible { request, .. } => &request.messages[*index],
                _ => unreachable!(),
            },
            Self::Choice {
                inner,
                choice_index,
            } => match inner.as_ref() {
                ProviderResponse::OpenAiChatCompletion(r) => &r.choices[*choice_index].message,
                _ => unreachable!(),
            },
        }
    }
}

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
                    _ => unreachable!(),
                },
                _ => unreachable!(),
            },
            ProviderRequest::OpenAiChatCompatible { request, .. } => {
                match &request.tools.as_ref().expect("guarded")[self.tool_index] {
                    OpenAiTool::Custom { custom } => match custom.format.as_ref().expect("guarded")
                    {
                        OpenAiCustomToolFormat::Grammar { grammar } => grammar,
                        _ => unreachable!(),
                    },
                    _ => unreachable!(),
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

// ──────────────────────────────────────────────────────────────────────────────
// OpenAI Chat — shared message types (Rule 5)
// ──────────────────────────────────────────────────────────────────────────────

#[pyclass(module = "wyrd.prompt", name = "OpenAiChatMessage")]
pub struct PyOpenAiChatMessage {
    pub(crate) src: ChatMessageSource,
}
impl PyOpenAiChatMessage {
    fn m(&self) -> &OpenAiChatMessage {
        self.src.msg()
    }
}
#[pymethods]
impl PyOpenAiChatMessage {
    #[getter]
    fn role(&self) -> &str {
        &self.m().role
    }
    #[getter]
    fn content(&self) -> Option<PyOpenAiMessageContent> {
        self.m().content.as_ref().map(|_| PyOpenAiMessageContent {
            src: self.src.clone(),
        })
    }
    #[getter]
    fn name(&self) -> Option<&str> {
        self.m().name.as_deref()
    }
    #[getter]
    fn tool_calls(&self) -> Option<Vec<PyOpenAiToolCall>> {
        self.m().tool_calls.as_ref().map(|tc| {
            (0..tc.len())
                .map(|i| PyOpenAiToolCall {
                    src: self.src.clone(),
                    call_index: i,
                })
                .collect()
        })
    }
    #[getter]
    fn tool_call_id(&self) -> Option<&str> {
        self.m().tool_call_id.as_deref()
    }
    #[getter]
    fn refusal(&self) -> Option<&str> {
        self.m().refusal.as_deref()
    }
    #[getter]
    fn annotations(&self) -> Vec<PyOpenAiMessageAnnotation> {
        (0..self.m().annotations.len())
            .map(|i| PyOpenAiMessageAnnotation {
                src: self.src.clone(),
                index: i,
            })
            .collect()
    }
    #[getter]
    fn audio(&self) -> Option<PyOpenAiMessageAudio> {
        self.m().audio.as_ref().map(|_| PyOpenAiMessageAudio {
            src: self.src.clone(),
        })
    }
    fn __repr__(&self) -> String {
        format!("OpenAiChatMessage(role={:?})", self.m().role)
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiMessageContent")]
pub struct PyOpenAiMessageContent {
    src: ChatMessageSource,
}
impl PyOpenAiMessageContent {
    fn c(&self) -> &OpenAiMessageContent {
        self.src.msg().content.as_ref().expect("guarded")
    }
}
#[pymethods]
impl PyOpenAiMessageContent {
    #[getter]
    fn kind(&self) -> &'static str {
        match self.c() {
            OpenAiMessageContent::Text(_) => "text",
            OpenAiMessageContent::Parts(_) => "parts",
        }
    }
    fn as_text(&self) -> CardPyResult<String> {
        match self.c() {
            OpenAiMessageContent::Text(s) => Ok(s.clone()),
            _ => Err(wrong_variant("text", self.kind()).into()),
        }
    }
    fn as_parts(&self) -> CardPyResult<Vec<PyOpenAiContentPart>> {
        match self.c() {
            OpenAiMessageContent::Parts(ps) => Ok((0..ps.len())
                .map(|i| PyOpenAiContentPart {
                    src: self.src.clone(),
                    part_index: i,
                })
                .collect()),
            _ => Err(wrong_variant("parts", self.kind()).into()),
        }
    }
    fn __repr__(&self) -> String {
        format!("OpenAiMessageContent(kind={:?})", self.kind())
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiContentPart")]
pub struct PyOpenAiContentPart {
    src: ChatMessageSource,
    part_index: usize,
}
impl PyOpenAiContentPart {
    fn p(&self) -> &OpenAiContentPart {
        match self.src.msg().content.as_ref().expect("guarded") {
            OpenAiMessageContent::Parts(ps) => &ps[self.part_index],
            _ => unreachable!("guarded by parent"),
        }
    }
}
#[pymethods]
impl PyOpenAiContentPart {
    #[getter]
    fn kind(&self) -> &'static str {
        match self.p() {
            OpenAiContentPart::Text { .. } => "text",
            OpenAiContentPart::ImageUrl { .. } => "image_url",
            OpenAiContentPart::InputAudio { .. } => "input_audio",
            OpenAiContentPart::File { .. } => "file",
        }
    }
    fn as_text(&self) -> CardPyResult<String> {
        match self.p() {
            OpenAiContentPart::Text { text } => Ok(text.clone()),
            _ => Err(wrong_variant("text", self.kind()).into()),
        }
    }
    fn as_image_url(&self) -> CardPyResult<PyOpenAiImageUrl> {
        match self.p() {
            OpenAiContentPart::ImageUrl { .. } => Ok(PyOpenAiImageUrl {
                src: self.src.clone(),
                part_index: self.part_index,
            }),
            _ => Err(wrong_variant("image_url", self.kind()).into()),
        }
    }
    fn as_input_audio(&self) -> CardPyResult<PyOpenAiInputAudio> {
        match self.p() {
            OpenAiContentPart::InputAudio { .. } => Ok(PyOpenAiInputAudio {
                src: self.src.clone(),
                part_index: self.part_index,
            }),
            _ => Err(wrong_variant("input_audio", self.kind()).into()),
        }
    }
    fn as_file(&self) -> CardPyResult<PyOpenAiFilePart> {
        match self.p() {
            OpenAiContentPart::File { .. } => Ok(PyOpenAiFilePart {
                src: self.src.clone(),
                part_index: self.part_index,
            }),
            _ => Err(wrong_variant("file", self.kind()).into()),
        }
    }
    fn __repr__(&self) -> String {
        format!("OpenAiContentPart(kind={:?})", self.kind())
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiImageUrl")]
pub struct PyOpenAiImageUrl {
    src: ChatMessageSource,
    part_index: usize,
}
impl PyOpenAiImageUrl {
    fn i(&self) -> &OpenAiImageUrl {
        match self.src.msg().content.as_ref().expect("guarded") {
            OpenAiMessageContent::Parts(ps) => match &ps[self.part_index] {
                OpenAiContentPart::ImageUrl { image_url } => image_url,
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiImageUrl {
    #[getter]
    fn url(&self) -> &str {
        &self.i().url
    }
    #[getter]
    fn detail(&self) -> Option<&str> {
        self.i().detail.as_deref()
    }
    fn __repr__(&self) -> String {
        "OpenAiImageUrl".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiInputAudio")]
pub struct PyOpenAiInputAudio {
    src: ChatMessageSource,
    part_index: usize,
}
impl PyOpenAiInputAudio {
    fn a(&self) -> &OpenAiInputAudio {
        match self.src.msg().content.as_ref().expect("guarded") {
            OpenAiMessageContent::Parts(ps) => match &ps[self.part_index] {
                OpenAiContentPart::InputAudio { input_audio } => input_audio,
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiInputAudio {
    #[getter]
    fn data(&self) -> &str {
        &self.a().data
    }
    #[getter]
    fn format(&self) -> &str {
        &self.a().format
    }
    fn __repr__(&self) -> String {
        "OpenAiInputAudio".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiFilePart")]
pub struct PyOpenAiFilePart {
    src: ChatMessageSource,
    part_index: usize,
}
impl PyOpenAiFilePart {
    fn f(&self) -> &OpenAiFilePart {
        match self.src.msg().content.as_ref().expect("guarded") {
            OpenAiMessageContent::Parts(ps) => match &ps[self.part_index] {
                OpenAiContentPart::File { file } => file,
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiFilePart {
    #[getter]
    fn file_id(&self) -> Option<&str> {
        self.f().file_id.as_deref()
    }
    #[getter]
    fn file_data(&self) -> Option<&str> {
        self.f().file_data.as_deref()
    }
    #[getter]
    fn filename(&self) -> Option<&str> {
        self.f().filename.as_deref()
    }
    fn __repr__(&self) -> String {
        "OpenAiFilePart".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiToolCall")]
pub struct PyOpenAiToolCall {
    src: ChatMessageSource,
    call_index: usize,
}
impl PyOpenAiToolCall {
    fn c(&self) -> &OpenAiToolCall {
        &self.src.msg().tool_calls.as_ref().expect("guarded")[self.call_index]
    }
}
#[pymethods]
impl PyOpenAiToolCall {
    #[getter]
    fn id(&self) -> &str {
        &self.c().id
    }
    #[getter]
    fn kind(&self) -> &str {
        &self.c().kind
    }
    #[getter]
    fn function(&self) -> PyOpenAiToolFunctionCall {
        PyOpenAiToolFunctionCall {
            src: self.src.clone(),
            call_index: self.call_index,
        }
    }
    fn __repr__(&self) -> String {
        format!("OpenAiToolCall(id={:?})", self.c().id)
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiToolFunctionCall")]
pub struct PyOpenAiToolFunctionCall {
    src: ChatMessageSource,
    call_index: usize,
}
impl PyOpenAiToolFunctionCall {
    fn f(&self) -> &OpenAiToolFunctionCall {
        &self.src.msg().tool_calls.as_ref().expect("guarded")[self.call_index].function
    }
}
#[pymethods]
impl PyOpenAiToolFunctionCall {
    #[getter]
    fn name(&self) -> &str {
        &self.f().name
    }
    #[getter]
    fn arguments(&self) -> &str {
        &self.f().arguments
    }
    fn __repr__(&self) -> String {
        format!("OpenAiToolFunctionCall(name={:?})", self.f().name)
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiMessageAnnotation")]
pub struct PyOpenAiMessageAnnotation {
    src: ChatMessageSource,
    index: usize,
}
impl PyOpenAiMessageAnnotation {
    fn a(&self) -> &OpenAiMessageAnnotation {
        &self.src.msg().annotations[self.index]
    }
}
#[pymethods]
impl PyOpenAiMessageAnnotation {
    #[getter]
    fn kind(&self) -> &str {
        &self.a().kind
    }
    #[getter]
    fn url_citation(&self) -> PyOpenAiUrlCitation {
        PyOpenAiUrlCitation {
            src: self.src.clone(),
            index: self.index,
        }
    }
    fn __repr__(&self) -> String {
        format!("OpenAiMessageAnnotation(kind={:?})", self.a().kind)
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiUrlCitation")]
pub struct PyOpenAiUrlCitation {
    src: ChatMessageSource,
    index: usize,
}
impl PyOpenAiUrlCitation {
    fn u(&self) -> &OpenAiUrlCitation {
        &self.src.msg().annotations[self.index].url_citation
    }
}
#[pymethods]
impl PyOpenAiUrlCitation {
    #[getter]
    fn url(&self) -> &str {
        &self.u().url
    }
    #[getter]
    fn title(&self) -> &str {
        &self.u().title
    }
    #[getter]
    fn start_index(&self) -> u32 {
        self.u().start_index
    }
    #[getter]
    fn end_index(&self) -> u32 {
        self.u().end_index
    }
    fn __repr__(&self) -> String {
        "OpenAiUrlCitation".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiMessageAudio")]
pub struct PyOpenAiMessageAudio {
    src: ChatMessageSource,
}
impl PyOpenAiMessageAudio {
    fn a(&self) -> &OpenAiMessageAudio {
        self.src.msg().audio.as_ref().expect("guarded")
    }
}
#[pymethods]
impl PyOpenAiMessageAudio {
    #[getter]
    fn id(&self) -> &str {
        &self.a().id
    }
    #[getter]
    fn expires_at(&self) -> u64 {
        self.a().expires_at
    }
    #[getter]
    fn data(&self) -> &str {
        &self.a().data
    }
    #[getter]
    fn transcript(&self) -> &str {
        &self.a().transcript
    }
    fn __repr__(&self) -> String {
        format!("OpenAiMessageAudio(id={:?})", self.a().id)
    }
}

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
    fn content(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        wyrd_utils::py::json_to_pyobject(py, &serde_json::Value::Array(self.l().content.clone()))
            .map_err(Into::into)
    }
    #[getter]
    fn refusal(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
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

// ──────────────────────────────────────────────────────────────────────────────
// Anthropic — request + response
// ──────────────────────────────────────────────────────────────────────────────

#[pyclass(module = "wyrd.prompt", name = "AnthropicMessagesRequest")]
pub struct PyAnthropicMessagesRequest {
    pub(crate) inner: Arc<ProviderRequest>,
}
impl PyAnthropicMessagesRequest {
    pub fn new(inner: Arc<ProviderRequest>) -> Self {
        Self { inner }
    }
    fn req(&self) -> &AnthropicMessagesRequest {
        match self.inner.as_ref() {
            ProviderRequest::AnthropicMessage(r) => r,
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyAnthropicMessagesRequest {
    #[getter]
    fn model(&self) -> &str {
        &self.req().model
    }
    #[getter]
    fn messages(&self) -> Vec<PyAnthropicMessage> {
        (0..self.req().messages.len())
            .map(|i| PyAnthropicMessage {
                inner: Arc::clone(&self.inner),
                index: i,
            })
            .collect()
    }
    #[getter]
    fn system(&self) -> Option<PyAnthropicSystem> {
        self.req().system.as_ref().map(|_| PyAnthropicSystem {
            inner: Arc::clone(&self.inner),
        })
    }
    #[getter]
    fn stream(&self) -> Option<bool> {
        self.req().stream
    }
    #[getter]
    fn tools(&self) -> Vec<PyAnthropicTool> {
        let n = self.req().tools.as_ref().map_or(0, |t| t.len());
        (0..n)
            .map(|i| PyAnthropicTool {
                inner: Arc::clone(&self.inner),
                index: i,
            })
            .collect()
    }
    #[getter]
    fn output_config(&self) -> Option<PyAnthropicOutputConfig> {
        self.req()
            .output_config
            .as_ref()
            .map(|_| PyAnthropicOutputConfig {
                inner: Arc::clone(&self.inner),
            })
    }
    #[getter]
    fn settings(&self) -> PyAnthropicMessagesSettings {
        PyAnthropicMessagesSettings {
            inner: Arc::clone(&self.inner),
        }
    }
    fn __repr__(&self) -> String {
        format!("AnthropicMessagesRequest(model={:?})", self.req().model)
    }
}

#[pyclass(module = "wyrd.prompt", name = "AnthropicMessagesSettings")]
pub struct PyAnthropicMessagesSettings {
    inner: Arc<ProviderRequest>,
}
impl PyAnthropicMessagesSettings {
    fn s(&self) -> &AnthropicMessagesSettings {
        match self.inner.as_ref() {
            ProviderRequest::AnthropicMessage(r) => &r.settings,
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyAnthropicMessagesSettings {
    #[getter]
    fn max_tokens(&self) -> u32 {
        self.s().max_tokens
    }
    #[getter]
    fn temperature(&self) -> Option<f32> {
        self.s().temperature
    }
    #[getter]
    fn top_p(&self) -> Option<f32> {
        self.s().top_p
    }
    #[getter]
    fn top_k(&self) -> Option<u32> {
        self.s().top_k
    }
    #[getter]
    fn stop_sequences(&self) -> Option<Vec<String>> {
        self.s().stop_sequences.clone()
    }
    #[getter]
    fn thinking(&self) -> Option<PyAnthropicThinkingConfig> {
        self.s()
            .thinking
            .as_ref()
            .map(|_| PyAnthropicThinkingConfig {
                inner: Arc::clone(&self.inner),
            })
    }
    #[getter]
    fn extra(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        wyrd_utils::py::json_to_pyobject(py, &serde_json::Value::Object(self.s().extra.clone()))
            .map_err(Into::into)
    }
    fn __repr__(&self) -> String {
        "AnthropicMessagesSettings".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "AnthropicSystem")]
pub struct PyAnthropicSystem {
    inner: Arc<ProviderRequest>,
}
impl PyAnthropicSystem {
    fn s(&self) -> &AnthropicSystem {
        match self.inner.as_ref() {
            ProviderRequest::AnthropicMessage(r) => r.system.as_ref().expect("guarded"),
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyAnthropicSystem {
    #[getter]
    fn kind(&self) -> &'static str {
        match self.s() {
            AnthropicSystem::Text(_) => "text",
            AnthropicSystem::Blocks(_) => "blocks",
        }
    }
    fn as_text(&self) -> CardPyResult<String> {
        match self.s() {
            AnthropicSystem::Text(s) => Ok(s.clone()),
            _ => Err(wrong_variant("text", self.kind()).into()),
        }
    }
    fn as_blocks(&self) -> CardPyResult<Vec<PyAnthropicSystemBlock>> {
        match self.s() {
            AnthropicSystem::Blocks(bs) => Ok((0..bs.len())
                .map(|i| PyAnthropicSystemBlock {
                    inner: Arc::clone(&self.inner),
                    index: i,
                })
                .collect()),
            _ => Err(wrong_variant("blocks", self.kind()).into()),
        }
    }
    fn __repr__(&self) -> String {
        format!("AnthropicSystem(kind={:?})", self.kind())
    }
}

#[pyclass(module = "wyrd.prompt", name = "AnthropicSystemBlock")]
pub struct PyAnthropicSystemBlock {
    inner: Arc<ProviderRequest>,
    index: usize,
}
impl PyAnthropicSystemBlock {
    fn b(&self) -> &AnthropicSystemBlock {
        match self.inner.as_ref() {
            ProviderRequest::AnthropicMessage(r) => match r.system.as_ref().expect("guarded") {
                AnthropicSystem::Blocks(bs) => &bs[self.index],
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyAnthropicSystemBlock {
    #[getter]
    fn kind(&self) -> &'static str {
        match self.b() {
            AnthropicSystemBlock::Text { .. } => "text",
        }
    }
    fn as_text(&self) -> CardPyResult<String> {
        match self.b() {
            AnthropicSystemBlock::Text { text, .. } => Ok(text.clone()),
        }
    }
    #[getter]
    fn cache_control(&self) -> Option<PyAnthropicCacheControl> {
        match self.b() {
            AnthropicSystemBlock::Text { cache_control, .. } => {
                cache_control.as_ref().map(|_| PyAnthropicCacheControl {
                    inner: Arc::clone(&self.inner),
                    path: AnthropicCacheControlPath::SystemBlock(self.index),
                })
            }
        }
    }
    fn __repr__(&self) -> String {
        "AnthropicSystemBlock".to_owned()
    }
}

// Path enum for cache control location
#[derive(Clone)]
enum AnthropicCacheControlPath {
    SystemBlock(usize),
    Tool(usize),
}

#[pyclass(module = "wyrd.prompt", name = "AnthropicCacheControl")]
pub struct PyAnthropicCacheControl {
    inner: Arc<ProviderRequest>,
    path: AnthropicCacheControlPath,
}
impl PyAnthropicCacheControl {
    fn c(&self) -> &AnthropicCacheControl {
        let req = match self.inner.as_ref() {
            ProviderRequest::AnthropicMessage(r) => r,
            _ => unreachable!(),
        };
        match &self.path {
            AnthropicCacheControlPath::SystemBlock(i) => {
                match req.system.as_ref().expect("guarded") {
                    AnthropicSystem::Blocks(bs) => match &bs[*i] {
                        AnthropicSystemBlock::Text { cache_control, .. } => {
                            cache_control.as_ref().expect("guarded")
                        }
                    },
                    _ => unreachable!(),
                }
            }
            AnthropicCacheControlPath::Tool(i) => req.tools.as_ref().expect("guarded")[*i]
                .cache_control
                .as_ref()
                .expect("guarded"),
        }
    }
}
#[pymethods]
impl PyAnthropicCacheControl {
    #[getter]
    fn kind(&self) -> &str {
        &self.c().kind
    }
    #[getter]
    fn ttl(&self) -> Option<&str> {
        self.c().ttl.as_deref()
    }
    fn __repr__(&self) -> String {
        format!("AnthropicCacheControl(kind={:?})", self.c().kind)
    }
}

#[pyclass(module = "wyrd.prompt", name = "AnthropicThinkingConfig")]
pub struct PyAnthropicThinkingConfig {
    inner: Arc<ProviderRequest>,
}
impl PyAnthropicThinkingConfig {
    fn t(&self) -> &AnthropicThinkingConfig {
        match self.inner.as_ref() {
            ProviderRequest::AnthropicMessage(r) => r.settings.thinking.as_ref().expect("guarded"),
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyAnthropicThinkingConfig {
    #[getter]
    fn kind(&self) -> &str {
        &self.t().kind
    }
    #[getter]
    fn budget_tokens(&self) -> Option<u32> {
        self.t().budget_tokens
    }
    fn __repr__(&self) -> String {
        "AnthropicThinkingConfig".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "AnthropicMessage")]
pub struct PyAnthropicMessage {
    inner: Arc<ProviderRequest>,
    index: usize,
}
impl PyAnthropicMessage {
    fn m(&self) -> &AnthropicMessage {
        match self.inner.as_ref() {
            ProviderRequest::AnthropicMessage(r) => &r.messages[self.index],
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyAnthropicMessage {
    #[getter]
    fn role(&self) -> &str {
        &self.m().role
    }
    #[getter]
    fn content(&self) -> Vec<PyAnthropicContentBlock> {
        (0..self.m().content.len())
            .map(|i| PyAnthropicContentBlock {
                inner: Arc::clone(&self.inner),
                msg_index: self.index,
                block_index: i,
            })
            .collect()
    }
    fn __repr__(&self) -> String {
        format!("AnthropicMessage(role={:?})", self.m().role)
    }
}

#[pyclass(module = "wyrd.prompt", name = "AnthropicContentBlock")]
pub struct PyAnthropicContentBlock {
    inner: Arc<ProviderRequest>,
    msg_index: usize,
    block_index: usize,
}
impl PyAnthropicContentBlock {
    fn b(&self) -> &AnthropicContentBlock {
        match self.inner.as_ref() {
            ProviderRequest::AnthropicMessage(r) => {
                &r.messages[self.msg_index].content[self.block_index]
            }
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyAnthropicContentBlock {
    #[getter]
    fn kind(&self) -> &'static str {
        match self.b() {
            AnthropicContentBlock::Text { .. } => "text",
            AnthropicContentBlock::Image { .. } => "image",
            AnthropicContentBlock::Document { .. } => "document",
            AnthropicContentBlock::Thinking { .. } => "thinking",
            AnthropicContentBlock::RedactedThinking { .. } => "redacted_thinking",
            AnthropicContentBlock::ToolUse { .. } => "tool_use",
            AnthropicContentBlock::ToolResult { .. } => "tool_result",
        }
    }
    fn as_text(&self) -> CardPyResult<String> {
        match self.b() {
            AnthropicContentBlock::Text { text, .. } => Ok(text.clone()),
            _ => Err(wrong_variant("text", self.kind()).into()),
        }
    }
    fn as_tool_use_id(&self) -> CardPyResult<String> {
        match self.b() {
            AnthropicContentBlock::ToolUse { id, .. } => Ok(id.clone()),
            _ => Err(wrong_variant("tool_use", self.kind()).into()),
        }
    }
    fn as_tool_use_name(&self) -> CardPyResult<String> {
        match self.b() {
            AnthropicContentBlock::ToolUse { name, .. } => Ok(name.clone()),
            _ => Err(wrong_variant("tool_use", self.kind()).into()),
        }
    }
    fn as_tool_use_input(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        match self.b() {
            AnthropicContentBlock::ToolUse { input, .. } => {
                wyrd_utils::py::json_to_pyobject(py, input).map_err(Into::into)
            }
            _ => Err(wrong_variant("tool_use", self.kind()).into()),
        }
    }
    fn as_thinking(&self) -> CardPyResult<String> {
        match self.b() {
            AnthropicContentBlock::Thinking { thinking, .. } => Ok(thinking.clone()),
            _ => Err(wrong_variant("thinking", self.kind()).into()),
        }
    }
    fn __repr__(&self) -> String {
        format!("AnthropicContentBlock(kind={:?})", self.kind())
    }
}

#[pyclass(module = "wyrd.prompt", name = "AnthropicImageSource")]
pub struct PyAnthropicImageSource {
    value: AnthropicImageSource,
}
#[pymethods]
impl PyAnthropicImageSource {
    #[getter]
    fn kind(&self) -> &'static str {
        match &self.value {
            AnthropicImageSource::Base64 { .. } => "base64",
            AnthropicImageSource::Url { .. } => "url",
            AnthropicImageSource::FileId { .. } => "file_id",
        }
    }
    fn __repr__(&self) -> String {
        format!("AnthropicImageSource(kind={:?})", self.kind())
    }
}

#[pyclass(module = "wyrd.prompt", name = "AnthropicDocumentSource")]
pub struct PyAnthropicDocumentSource {
    value: AnthropicDocumentSource,
}
#[pymethods]
impl PyAnthropicDocumentSource {
    #[getter]
    fn kind(&self) -> &'static str {
        match &self.value {
            AnthropicDocumentSource::Base64 { .. } => "base64",
            AnthropicDocumentSource::Url { .. } => "url",
            AnthropicDocumentSource::FileId { .. } => "file_id",
            AnthropicDocumentSource::Text { .. } => "text",
            AnthropicDocumentSource::Content { .. } => "content",
        }
    }
    fn __repr__(&self) -> String {
        format!("AnthropicDocumentSource(kind={:?})", self.kind())
    }
}

#[pyclass(module = "wyrd.prompt", name = "AnthropicToolResultContent")]
pub struct PyAnthropicToolResultContent {
    value: AnthropicToolResultContent,
}
#[pymethods]
impl PyAnthropicToolResultContent {
    #[getter]
    fn kind(&self) -> &'static str {
        match &self.value {
            AnthropicToolResultContent::Text(_) => "text",
            AnthropicToolResultContent::Blocks(_) => "blocks",
        }
    }
    fn as_text(&self) -> CardPyResult<String> {
        match &self.value {
            AnthropicToolResultContent::Text(s) => Ok(s.clone()),
            _ => Err(wrong_variant("text", self.kind()).into()),
        }
    }
    fn __repr__(&self) -> String {
        format!("AnthropicToolResultContent(kind={:?})", self.kind())
    }
}

#[pyclass(module = "wyrd.prompt", name = "AnthropicTool")]
pub struct PyAnthropicTool {
    inner: Arc<ProviderRequest>,
    index: usize,
}
impl PyAnthropicTool {
    fn t(&self) -> &AnthropicTool {
        match self.inner.as_ref() {
            ProviderRequest::AnthropicMessage(r) => &r.tools.as_ref().expect("guarded")[self.index],
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyAnthropicTool {
    #[getter]
    fn name(&self) -> &str {
        &self.t().name
    }
    #[getter]
    fn description(&self) -> Option<&str> {
        self.t().description.as_deref()
    }
    #[getter]
    fn input_schema(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        wyrd_utils::py::json_to_pyobject(py, &self.t().input_schema).map_err(Into::into)
    }
    #[getter]
    fn kind(&self) -> Option<&str> {
        self.t().kind.as_deref()
    }
    #[getter]
    fn display_width_px(&self) -> Option<u32> {
        self.t().display_width_px
    }
    #[getter]
    fn display_height_px(&self) -> Option<u32> {
        self.t().display_height_px
    }
    #[getter]
    fn display_number(&self) -> Option<u32> {
        self.t().display_number
    }
    fn __repr__(&self) -> String {
        format!("AnthropicTool(name={:?})", self.t().name)
    }
}

#[pyclass(module = "wyrd.prompt", name = "AnthropicOutputConfig")]
pub struct PyAnthropicOutputConfig {
    inner: Arc<ProviderRequest>,
}
impl PyAnthropicOutputConfig {
    fn c(&self) -> &AnthropicOutputConfig {
        match self.inner.as_ref() {
            ProviderRequest::AnthropicMessage(r) => r.output_config.as_ref().expect("guarded"),
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyAnthropicOutputConfig {
    #[getter]
    fn format_kind(&self) -> &'static str {
        match &self.c().format {
            AnthropicOutputFormat::JsonSchema { .. } => "json_schema",
        }
    }
    #[getter]
    fn schema(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        match &self.c().format {
            AnthropicOutputFormat::JsonSchema { schema } => {
                wyrd_utils::py::json_to_pyobject(py, schema).map_err(Into::into)
            }
        }
    }
    fn __repr__(&self) -> String {
        "AnthropicOutputConfig".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "AnthropicCitationV1")]
pub struct PyAnthropicCitationV1 {
    value: AnthropicCitationV1,
}
#[pymethods]
impl PyAnthropicCitationV1 {
    #[getter]
    fn kind(&self) -> &'static str {
        match &self.value {
            AnthropicCitationV1::CharLocation { .. } => "char_location",
            AnthropicCitationV1::PageLocation { .. } => "page_location",
            AnthropicCitationV1::ContentBlockLocation { .. } => "content_block_location",
            AnthropicCitationV1::WebSearchResultLocation { .. } => "web_search_result_location",
            AnthropicCitationV1::SearchResultLocation { .. } => "search_result_location",
        }
    }
    fn __repr__(&self) -> String {
        format!("AnthropicCitationV1(kind={:?})", self.kind())
    }
}

// Anthropic response

#[pyclass(module = "wyrd.prompt", name = "AnthropicMessagesResponse")]
pub struct PyAnthropicMessagesResponse {
    inner: Arc<ProviderResponse>,
}
impl PyAnthropicMessagesResponse {
    pub fn new(inner: Arc<ProviderResponse>) -> Self {
        Self { inner }
    }
    fn resp(&self) -> &AnthropicMessagesResponse {
        match self.inner.as_ref() {
            ProviderResponse::AnthropicMessage(r) => r,
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyAnthropicMessagesResponse {
    #[getter]
    fn id(&self) -> &str {
        &self.resp().id
    }
    #[getter]
    fn response_type(&self) -> &str {
        &self.resp().r#type
    }
    #[getter]
    fn role(&self) -> &str {
        &self.resp().role
    }
    #[getter]
    fn model(&self) -> &str {
        &self.resp().model
    }
    #[getter]
    fn stop_reason(&self) -> Option<&'static str> {
        self.resp().stop_reason.as_ref().map(|r| match r {
            AnthropicStopReason::EndTurn => "end_turn",
            AnthropicStopReason::MaxTokens => "max_tokens",
            AnthropicStopReason::StopSequence => "stop_sequence",
            AnthropicStopReason::ToolUse => "tool_use",
            AnthropicStopReason::PauseTurn => "pause_turn",
            AnthropicStopReason::Refusal => "refusal",
        })
    }
    #[getter]
    fn stop_sequence(&self) -> Option<&str> {
        self.resp().stop_sequence.as_deref()
    }
    #[getter]
    fn usage(&self) -> PyAnthropicUsage {
        PyAnthropicUsage {
            inner: Arc::clone(&self.inner),
        }
    }
    fn __repr__(&self) -> String {
        format!(
            "AnthropicMessagesResponse(id={:?}, model={:?})",
            self.resp().id,
            self.resp().model
        )
    }
}

#[pyclass(module = "wyrd.prompt", name = "AnthropicUsage")]
pub struct PyAnthropicUsage {
    inner: Arc<ProviderResponse>,
}
impl PyAnthropicUsage {
    fn u(&self) -> &AnthropicUsage {
        match self.inner.as_ref() {
            ProviderResponse::AnthropicMessage(r) => &r.usage,
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyAnthropicUsage {
    #[getter]
    fn input_tokens(&self) -> u64 {
        self.u().input_tokens
    }
    #[getter]
    fn output_tokens(&self) -> u64 {
        self.u().output_tokens
    }
    #[getter]
    fn cache_creation_input_tokens(&self) -> u64 {
        self.u().cache_creation_input_tokens
    }
    #[getter]
    fn cache_read_input_tokens(&self) -> u64 {
        self.u().cache_read_input_tokens
    }
    fn __repr__(&self) -> String {
        "AnthropicUsage".to_owned()
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Google Gemini — request + response
// ──────────────────────────────────────────────────────────────────────────────

fn google_request(arc: &Arc<ProviderRequest>) -> &GoogleGenerateContentRequest {
    match arc.as_ref() {
        ProviderRequest::GeminiGenerateContent(r) => r,
        ProviderRequest::Vertex(r) => &r.0,
        _ => unreachable!(),
    }
}

fn google_response(arc: &Arc<ProviderResponse>) -> &GoogleGenerateContentResponse {
    match arc.as_ref() {
        ProviderResponse::GeminiGenerateContent(r) | ProviderResponse::VertexGenerateContent(r) => {
            r
        }
        _ => unreachable!(),
    }
}

#[pyclass(module = "wyrd.prompt", name = "GeminiRequest")]
pub struct PyGeminiRequest {
    pub(crate) inner: Arc<ProviderRequest>,
}
impl PyGeminiRequest {
    pub fn new(inner: Arc<ProviderRequest>) -> Self {
        Self { inner }
    }
}
#[pymethods]
impl PyGeminiRequest {
    #[getter]
    fn contents(&self) -> Vec<PyGoogleContent> {
        let n = google_request(&self.inner).contents.len();
        (0..n)
            .map(|i| PyGoogleContent {
                inner: Arc::clone(&self.inner),
                index: i,
            })
            .collect()
    }
    #[getter]
    fn system_instruction(&self) -> Option<PyGoogleContent> {
        google_request(&self.inner)
            .system_instruction
            .as_ref()
            .map(|_| PyGoogleContent {
                inner: Arc::clone(&self.inner),
                index: usize::MAX,
            })
    }
    #[getter]
    fn settings(&self) -> PyGoogleGenerationConfig {
        PyGoogleGenerationConfig {
            inner: Arc::clone(&self.inner),
        }
    }
    fn __repr__(&self) -> String {
        "GeminiRequest".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "VertexRequest")]
pub struct PyVertexRequest {
    pub(crate) inner: Arc<ProviderRequest>,
}
impl PyVertexRequest {
    pub fn new(inner: Arc<ProviderRequest>) -> Self {
        Self { inner }
    }
}
#[pymethods]
impl PyVertexRequest {
    #[getter]
    fn contents(&self) -> Vec<PyGoogleContent> {
        let n = google_request(&self.inner).contents.len();
        (0..n)
            .map(|i| PyGoogleContent {
                inner: Arc::clone(&self.inner),
                index: i,
            })
            .collect()
    }
    #[getter]
    fn settings(&self) -> PyGoogleGenerationConfig {
        PyGoogleGenerationConfig {
            inner: Arc::clone(&self.inner),
        }
    }
    fn __repr__(&self) -> String {
        "VertexRequest".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "GoogleContent")]
pub struct PyGoogleContent {
    inner: Arc<ProviderRequest>,
    index: usize,
}
impl PyGoogleContent {
    fn c(&self) -> &GoogleContent {
        let req = google_request(&self.inner);
        if self.index == usize::MAX {
            req.system_instruction.as_ref().expect("guarded")
        } else {
            &req.contents[self.index]
        }
    }
}
#[pymethods]
impl PyGoogleContent {
    #[getter]
    fn role(&self) -> &str {
        &self.c().role
    }
    #[getter]
    fn parts(&self) -> Vec<PyGooglePart> {
        let n = self.c().parts.len();
        (0..n)
            .map(|i| PyGooglePart {
                inner: Arc::clone(&self.inner),
                content_index: self.index,
                part_index: i,
            })
            .collect()
    }
    fn __repr__(&self) -> String {
        format!("GoogleContent(role={:?})", self.c().role)
    }
}

#[pyclass(module = "wyrd.prompt", name = "GooglePart")]
pub struct PyGooglePart {
    inner: Arc<ProviderRequest>,
    content_index: usize,
    part_index: usize,
}
impl PyGooglePart {
    fn p(&self) -> &GooglePart {
        let req = google_request(&self.inner);
        let content = if self.content_index == usize::MAX {
            req.system_instruction.as_ref().expect("guarded")
        } else {
            &req.contents[self.content_index]
        };
        &content.parts[self.part_index]
    }
}
#[pymethods]
impl PyGooglePart {
    #[getter]
    fn kind(&self) -> &'static str {
        match self.p() {
            GooglePart::Text { .. } => "text",
            GooglePart::InlineData { .. } => "inline_data",
            GooglePart::FileData { .. } => "file_data",
            GooglePart::FunctionCall { .. } => "function_call",
            GooglePart::FunctionResponse { .. } => "function_response",
            GooglePart::Thought { .. } => "thought",
            GooglePart::ExecutableCode { .. } => "executable_code",
            GooglePart::CodeExecutionResult { .. } => "code_execution_result",
        }
    }
    fn as_text(&self) -> CardPyResult<String> {
        match self.p() {
            GooglePart::Text { text } => Ok(text.clone()),
            _ => Err(wrong_variant("text", self.kind()).into()),
        }
    }
    fn as_inline_data(&self) -> CardPyResult<PyGoogleInlineData> {
        match self.p() {
            GooglePart::InlineData { .. } => Ok(PyGoogleInlineData {
                inner: Arc::clone(&self.inner),
                content_index: self.content_index,
                part_index: self.part_index,
            }),
            _ => Err(wrong_variant("inline_data", self.kind()).into()),
        }
    }
    fn as_file_data(&self) -> CardPyResult<PyGoogleFileData> {
        match self.p() {
            GooglePart::FileData { .. } => Ok(PyGoogleFileData {
                inner: Arc::clone(&self.inner),
                content_index: self.content_index,
                part_index: self.part_index,
            }),
            _ => Err(wrong_variant("file_data", self.kind()).into()),
        }
    }
    fn as_function_call(&self) -> CardPyResult<PyGoogleFunctionCall> {
        match self.p() {
            GooglePart::FunctionCall { .. } => Ok(PyGoogleFunctionCall {
                inner: Arc::clone(&self.inner),
                content_index: self.content_index,
                part_index: self.part_index,
            }),
            _ => Err(wrong_variant("function_call", self.kind()).into()),
        }
    }
    fn as_function_response(&self) -> CardPyResult<PyGoogleFunctionResponse> {
        match self.p() {
            GooglePart::FunctionResponse { .. } => Ok(PyGoogleFunctionResponse {
                inner: Arc::clone(&self.inner),
                content_index: self.content_index,
                part_index: self.part_index,
            }),
            _ => Err(wrong_variant("function_response", self.kind()).into()),
        }
    }
    fn __repr__(&self) -> String {
        format!("GooglePart(kind={:?})", self.kind())
    }
}

#[pyclass(module = "wyrd.prompt", name = "GoogleInlineData")]
pub struct PyGoogleInlineData {
    inner: Arc<ProviderRequest>,
    content_index: usize,
    part_index: usize,
}
impl PyGoogleInlineData {
    fn d(&self) -> &GoogleInlineData {
        let req = google_request(&self.inner);
        let c = if self.content_index == usize::MAX {
            req.system_instruction.as_ref().expect("guarded")
        } else {
            &req.contents[self.content_index]
        };
        match &c.parts[self.part_index] {
            GooglePart::InlineData { inline_data } => inline_data,
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyGoogleInlineData {
    #[getter]
    fn mime_type(&self) -> &str {
        &self.d().mime_type
    }
    #[getter]
    fn data(&self) -> &str {
        &self.d().data
    }
    fn __repr__(&self) -> String {
        "GoogleInlineData".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "GoogleFileData")]
pub struct PyGoogleFileData {
    inner: Arc<ProviderRequest>,
    content_index: usize,
    part_index: usize,
}
impl PyGoogleFileData {
    fn d(&self) -> &GoogleFileData {
        let req = google_request(&self.inner);
        let c = if self.content_index == usize::MAX {
            req.system_instruction.as_ref().expect("guarded")
        } else {
            &req.contents[self.content_index]
        };
        match &c.parts[self.part_index] {
            GooglePart::FileData { file_data } => file_data,
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyGoogleFileData {
    #[getter]
    fn mime_type(&self) -> &str {
        &self.d().mime_type
    }
    #[getter]
    fn file_uri(&self) -> &str {
        &self.d().file_uri
    }
    fn __repr__(&self) -> String {
        "GoogleFileData".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "GoogleFunctionCall")]
pub struct PyGoogleFunctionCall {
    inner: Arc<ProviderRequest>,
    content_index: usize,
    part_index: usize,
}
impl PyGoogleFunctionCall {
    fn f(&self) -> &GoogleFunctionCall {
        let req = google_request(&self.inner);
        let c = if self.content_index == usize::MAX {
            req.system_instruction.as_ref().expect("guarded")
        } else {
            &req.contents[self.content_index]
        };
        match &c.parts[self.part_index] {
            GooglePart::FunctionCall { function_call } => function_call,
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyGoogleFunctionCall {
    #[getter]
    fn name(&self) -> &str {
        &self.f().name
    }
    #[getter]
    fn args(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        wyrd_utils::py::json_to_pyobject(py, &self.f().args).map_err(Into::into)
    }
    fn __repr__(&self) -> String {
        format!("GoogleFunctionCall(name={:?})", self.f().name)
    }
}

#[pyclass(module = "wyrd.prompt", name = "GoogleFunctionResponse")]
pub struct PyGoogleFunctionResponse {
    inner: Arc<ProviderRequest>,
    content_index: usize,
    part_index: usize,
}
impl PyGoogleFunctionResponse {
    fn f(&self) -> &GoogleFunctionResponse {
        let req = google_request(&self.inner);
        let c = if self.content_index == usize::MAX {
            req.system_instruction.as_ref().expect("guarded")
        } else {
            &req.contents[self.content_index]
        };
        match &c.parts[self.part_index] {
            GooglePart::FunctionResponse { function_response } => function_response,
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyGoogleFunctionResponse {
    #[getter]
    fn name(&self) -> &str {
        &self.f().name
    }
    #[getter]
    fn response(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        wyrd_utils::py::json_to_pyobject(py, &self.f().response).map_err(Into::into)
    }
    fn __repr__(&self) -> String {
        format!("GoogleFunctionResponse(name={:?})", self.f().name)
    }
}

#[pyclass(module = "wyrd.prompt", name = "GoogleExecutableCode")]
pub struct PyGoogleExecutableCode {
    value: GoogleExecutableCode,
}
#[pymethods]
impl PyGoogleExecutableCode {
    #[getter]
    fn language(&self) -> &str {
        &self.value.language
    }
    #[getter]
    fn code(&self) -> &str {
        &self.value.code
    }
    fn __repr__(&self) -> String {
        "GoogleExecutableCode".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "GoogleCodeExecutionResult")]
pub struct PyGoogleCodeExecutionResult {
    value: GoogleCodeExecutionResult,
}
#[pymethods]
impl PyGoogleCodeExecutionResult {
    #[getter]
    fn outcome(&self) -> &str {
        &self.value.outcome
    }
    #[getter]
    fn output(&self) -> &str {
        &self.value.output
    }
    fn __repr__(&self) -> String {
        "GoogleCodeExecutionResult".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "GoogleGenerationConfig")]
pub struct PyGoogleGenerationConfig {
    inner: Arc<ProviderRequest>,
}
impl PyGoogleGenerationConfig {
    fn cfg(&self) -> Option<&GoogleGenerationConfig> {
        google_request(&self.inner)
            .settings
            .generation_config
            .as_ref()
    }
}
#[pymethods]
impl PyGoogleGenerationConfig {
    #[getter]
    fn temperature(&self) -> Option<f32> {
        self.cfg().and_then(|c| c.temperature)
    }
    #[getter]
    fn top_p(&self) -> Option<f32> {
        self.cfg().and_then(|c| c.top_p)
    }
    #[getter]
    fn top_k(&self) -> Option<u32> {
        self.cfg().and_then(|c| c.top_k)
    }
    #[getter]
    fn candidate_count(&self) -> Option<u32> {
        self.cfg().and_then(|c| c.candidate_count)
    }
    #[getter]
    fn max_output_tokens(&self) -> Option<u32> {
        self.cfg().and_then(|c| c.max_output_tokens)
    }
    #[getter]
    fn stop_sequences(&self) -> Option<Vec<String>> {
        self.cfg().and_then(|c| c.stop_sequences.clone())
    }
    #[getter]
    fn response_mime_type(&self) -> Option<String> {
        self.cfg().and_then(|c| c.response_mime_type.clone())
    }
    #[getter]
    fn presence_penalty(&self) -> Option<f32> {
        self.cfg().and_then(|c| c.presence_penalty)
    }
    #[getter]
    fn frequency_penalty(&self) -> Option<f32> {
        self.cfg().and_then(|c| c.frequency_penalty)
    }
    #[getter]
    fn seed(&self) -> Option<i64> {
        self.cfg().and_then(|c| c.seed)
    }
    #[getter]
    fn thinking_config(&self) -> Option<PyGoogleThinkingConfig> {
        self.cfg()
            .and_then(|c| c.thinking_config.as_ref())
            .map(|_| PyGoogleThinkingConfig {
                inner: Arc::clone(&self.inner),
            })
    }
    fn __repr__(&self) -> String {
        "GoogleGenerationConfig".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "GoogleThinkingConfig")]
pub struct PyGoogleThinkingConfig {
    inner: Arc<ProviderRequest>,
}
impl PyGoogleThinkingConfig {
    fn t(&self) -> &GoogleThinkingConfig {
        google_request(&self.inner)
            .settings
            .generation_config
            .as_ref()
            .expect("guarded")
            .thinking_config
            .as_ref()
            .expect("guarded")
    }
}
#[pymethods]
impl PyGoogleThinkingConfig {
    #[getter]
    fn include_thoughts(&self) -> Option<bool> {
        self.t().include_thoughts
    }
    #[getter]
    fn thinking_budget(&self) -> Option<i32> {
        self.t().thinking_budget
    }
    fn __repr__(&self) -> String {
        "GoogleThinkingConfig".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "GoogleSafetySetting")]
pub struct PyGoogleSafetySetting {
    inner: Arc<ProviderRequest>,
    index: usize,
}
impl PyGoogleSafetySetting {
    fn s(&self) -> &GoogleSafetySetting {
        &google_request(&self.inner)
            .settings
            .safety_settings
            .as_ref()
            .expect("guarded")[self.index]
    }
}
#[pymethods]
impl PyGoogleSafetySetting {
    #[getter]
    fn category(&self) -> &str {
        &self.s().category
    }
    #[getter]
    fn threshold(&self) -> &str {
        &self.s().threshold
    }
    fn __repr__(&self) -> String {
        "GoogleSafetySetting".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "GoogleTool")]
pub struct PyGoogleTool {
    inner: Arc<ProviderRequest>,
    index: usize,
}
impl PyGoogleTool {
    fn t(&self) -> &GoogleTool {
        &google_request(&self.inner).tools.as_ref().expect("guarded")[self.index]
    }
}
#[pymethods]
impl PyGoogleTool {
    #[getter]
    fn function_declarations(&self) -> Vec<PyGoogleFunctionDeclaration> {
        let n = self
            .t()
            .function_declarations
            .as_ref()
            .map_or(0, |v| v.len());
        (0..n)
            .map(|i| PyGoogleFunctionDeclaration {
                inner: Arc::clone(&self.inner),
                tool_index: self.index,
                fn_index: i,
            })
            .collect()
    }
    fn __repr__(&self) -> String {
        "GoogleTool".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "GoogleFunctionDeclaration")]
pub struct PyGoogleFunctionDeclaration {
    inner: Arc<ProviderRequest>,
    tool_index: usize,
    fn_index: usize,
}
impl PyGoogleFunctionDeclaration {
    fn f(&self) -> &GoogleFunctionDeclaration {
        &google_request(&self.inner).tools.as_ref().expect("guarded")[self.tool_index]
            .function_declarations
            .as_ref()
            .expect("guarded")[self.fn_index]
    }
}
#[pymethods]
impl PyGoogleFunctionDeclaration {
    #[getter]
    fn name(&self) -> &str {
        &self.f().name
    }
    #[getter]
    fn description(&self) -> Option<&str> {
        self.f().description.as_deref()
    }
    #[getter]
    fn parameters(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        wyrd_utils::py::json_to_pyobject(py, &self.f().parameters).map_err(Into::into)
    }
    fn __repr__(&self) -> String {
        format!("GoogleFunctionDeclaration(name={:?})", self.f().name)
    }
}

#[pyclass(module = "wyrd.prompt", name = "GoogleToolConfig")]
pub struct PyGoogleToolConfig {
    inner: Arc<ProviderRequest>,
}
impl PyGoogleToolConfig {
    fn c(&self) -> &GoogleToolConfig {
        google_request(&self.inner)
            .tool_config
            .as_ref()
            .expect("guarded")
    }
}
#[pymethods]
impl PyGoogleToolConfig {
    #[getter]
    fn function_calling_config(&self) -> PyGoogleFunctionCallingConfig {
        PyGoogleFunctionCallingConfig {
            inner: Arc::clone(&self.inner),
        }
    }
    fn __repr__(&self) -> String {
        "GoogleToolConfig".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "GoogleFunctionCallingConfig")]
pub struct PyGoogleFunctionCallingConfig {
    inner: Arc<ProviderRequest>,
}
impl PyGoogleFunctionCallingConfig {
    fn c(&self) -> &GoogleFunctionCallingConfig {
        &google_request(&self.inner)
            .tool_config
            .as_ref()
            .expect("guarded")
            .function_calling_config
    }
}
#[pymethods]
impl PyGoogleFunctionCallingConfig {
    #[getter]
    fn mode(&self) -> &str {
        &self.c().mode
    }
    #[getter]
    fn allowed_function_names(&self) -> Option<Vec<String>> {
        self.c().allowed_function_names.clone()
    }
    fn __repr__(&self) -> String {
        "GoogleFunctionCallingConfig".to_owned()
    }
}

// Google response

#[pyclass(module = "wyrd.prompt", name = "GeminiResponse")]
pub struct PyGeminiResponse {
    inner: Arc<ProviderResponse>,
}
impl PyGeminiResponse {
    pub fn new(inner: Arc<ProviderResponse>) -> Self {
        Self { inner }
    }
}
#[pymethods]
impl PyGeminiResponse {
    #[getter]
    fn candidates(&self) -> Vec<PyGoogleCandidate> {
        let n = google_response(&self.inner).candidates.len();
        (0..n)
            .map(|i| PyGoogleCandidate {
                inner: Arc::clone(&self.inner),
                index: i,
            })
            .collect()
    }
    #[getter]
    fn usage_metadata(&self) -> Option<PyGoogleUsageMetadata> {
        google_response(&self.inner)
            .usage_metadata
            .as_ref()
            .map(|_| PyGoogleUsageMetadata {
                inner: Arc::clone(&self.inner),
            })
    }
    #[getter]
    fn model_version(&self) -> Option<String> {
        google_response(&self.inner).model_version.clone()
    }
    fn __repr__(&self) -> String {
        "GeminiResponse".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "VertexResponse")]
pub struct PyVertexResponse {
    inner: Arc<ProviderResponse>,
}
impl PyVertexResponse {
    pub fn new(inner: Arc<ProviderResponse>) -> Self {
        Self { inner }
    }
}
#[pymethods]
impl PyVertexResponse {
    #[getter]
    fn candidates(&self) -> Vec<PyGoogleCandidate> {
        let n = google_response(&self.inner).candidates.len();
        (0..n)
            .map(|i| PyGoogleCandidate {
                inner: Arc::clone(&self.inner),
                index: i,
            })
            .collect()
    }
    #[getter]
    fn usage_metadata(&self) -> Option<PyGoogleUsageMetadata> {
        google_response(&self.inner)
            .usage_metadata
            .as_ref()
            .map(|_| PyGoogleUsageMetadata {
                inner: Arc::clone(&self.inner),
            })
    }
    fn __repr__(&self) -> String {
        "VertexResponse".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "GoogleCandidate")]
pub struct PyGoogleCandidate {
    inner: Arc<ProviderResponse>,
    index: usize,
}
impl PyGoogleCandidate {
    fn c(&self) -> &GoogleCandidate {
        &google_response(&self.inner).candidates[self.index]
    }
}
#[pymethods]
impl PyGoogleCandidate {
    #[getter]
    fn finish_reason(&self) -> Option<&'static str> {
        use skald_spec::wire::google_generate::GoogleFinishReason;
        self.c().finish_reason.as_ref().map(|r| match r {
            GoogleFinishReason::Stop => "stop",
            GoogleFinishReason::MaxTokens => "max_tokens",
            GoogleFinishReason::Safety => "safety",
            GoogleFinishReason::Recitation => "recitation",
            GoogleFinishReason::Language => "language",
            GoogleFinishReason::Other => "other",
            GoogleFinishReason::Blocklist => "blocklist",
            GoogleFinishReason::ProhibitedContent => "prohibited_content",
            GoogleFinishReason::Spii => "spii",
            GoogleFinishReason::MalformedFunctionCall => "malformed_function_call",
            GoogleFinishReason::FinishReasonUnspecified => "unspecified",
        })
    }
    #[getter]
    fn index(&self) -> Option<u32> {
        self.c().index
    }
    #[getter]
    fn avg_logprobs(&self) -> Option<f64> {
        self.c().avg_logprobs
    }
    #[getter]
    fn safety_ratings(&self) -> Vec<PyGoogleSafetyRating> {
        let n = self.c().safety_ratings.len();
        (0..n)
            .map(|i| PyGoogleSafetyRating {
                inner: Arc::clone(&self.inner),
                candidate_index: self.index,
                rating_index: i,
            })
            .collect()
    }
    fn __repr__(&self) -> String {
        format!("GoogleCandidate(finish_reason={:?})", self.finish_reason())
    }
}

#[pyclass(module = "wyrd.prompt", name = "GoogleUsageMetadata")]
pub struct PyGoogleUsageMetadata {
    inner: Arc<ProviderResponse>,
}
impl PyGoogleUsageMetadata {
    fn u(&self) -> &GoogleUsageMetadata {
        google_response(&self.inner)
            .usage_metadata
            .as_ref()
            .expect("guarded")
    }
}
#[pymethods]
impl PyGoogleUsageMetadata {
    #[getter]
    fn prompt_token_count(&self) -> u64 {
        self.u().prompt_token_count
    }
    #[getter]
    fn candidates_token_count(&self) -> u64 {
        self.u().candidates_token_count
    }
    #[getter]
    fn total_token_count(&self) -> u64 {
        self.u().total_token_count
    }
    #[getter]
    fn cached_content_token_count(&self) -> u64 {
        self.u().cached_content_token_count
    }
    #[getter]
    fn thoughts_token_count(&self) -> u64 {
        self.u().thoughts_token_count
    }
    fn __repr__(&self) -> String {
        "GoogleUsageMetadata".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "GoogleSafetyRating")]
pub struct PyGoogleSafetyRating {
    inner: Arc<ProviderResponse>,
    candidate_index: usize,
    rating_index: usize,
}
impl PyGoogleSafetyRating {
    fn r(&self) -> &GoogleSafetyRating {
        &google_response(&self.inner).candidates[self.candidate_index].safety_ratings
            [self.rating_index]
    }
}
#[pymethods]
impl PyGoogleSafetyRating {
    #[getter]
    fn category(&self) -> &str {
        &self.r().category
    }
    #[getter]
    fn probability(&self) -> &str {
        &self.r().probability
    }
    #[getter]
    fn blocked(&self) -> Option<bool> {
        self.r().blocked
    }
    fn __repr__(&self) -> String {
        "GoogleSafetyRating".to_owned()
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// OpenAI Responses — request + response
// ──────────────────────────────────────────────────────────────────────────────

#[pyclass(module = "wyrd.prompt", name = "OpenAiResponsesRequest")]
pub struct PyOpenAiResponsesRequest {
    pub(crate) inner: Arc<ProviderRequest>,
}
impl PyOpenAiResponsesRequest {
    pub fn new(inner: Arc<ProviderRequest>) -> Self {
        Self { inner }
    }
    fn req(&self) -> &OpenAiResponsesRequest {
        match self.inner.as_ref() {
            ProviderRequest::OpenAiResponses(r) => r,
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiResponsesRequest {
    #[getter]
    fn model(&self) -> &str {
        &self.req().model
    }
    #[getter]
    fn input(&self) -> Vec<PyOpenAiResponseItem> {
        let n = self.req().input.len();
        (0..n)
            .map(|i| PyOpenAiResponseItem {
                inner: Arc::clone(&self.inner),
                index: i,
            })
            .collect()
    }
    #[getter]
    fn instructions(&self) -> Option<&str> {
        self.req().instructions.as_deref()
    }
    #[getter]
    fn text(&self) -> Option<PyOpenAiResponsesText> {
        self.req().text.as_ref().map(|_| PyOpenAiResponsesText {
            inner: Arc::clone(&self.inner),
        })
    }
    #[getter]
    fn tools(&self) -> Vec<PyOpenAiResponsesTool> {
        let n = self.req().tools.as_ref().map_or(0, |t| t.len());
        (0..n)
            .map(|i| PyOpenAiResponsesTool {
                inner: Arc::clone(&self.inner),
                index: i,
            })
            .collect()
    }
    #[getter]
    fn tool_choice(&self) -> Option<PyOpenAiResponsesToolChoice> {
        self.req()
            .tool_choice
            .as_ref()
            .map(|_| PyOpenAiResponsesToolChoice {
                inner: Arc::clone(&self.inner),
            })
    }
    #[getter]
    fn parallel_tool_calls(&self) -> Option<bool> {
        self.req().parallel_tool_calls
    }
    #[getter]
    fn previous_response_id(&self) -> Option<&str> {
        self.req().previous_response_id.as_deref()
    }
    #[getter]
    fn stream(&self) -> Option<bool> {
        self.req().stream
    }
    #[getter]
    fn settings(&self) -> PyOpenAiResponsesSettings {
        PyOpenAiResponsesSettings {
            inner: Arc::clone(&self.inner),
        }
    }
    fn __repr__(&self) -> String {
        format!("OpenAiResponsesRequest(model={:?})", self.req().model)
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiResponsesSettings")]
pub struct PyOpenAiResponsesSettings {
    inner: Arc<ProviderRequest>,
}
impl PyOpenAiResponsesSettings {
    fn s(&self) -> &OpenAiResponsesSettings {
        match self.inner.as_ref() {
            ProviderRequest::OpenAiResponses(r) => &r.settings,
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiResponsesSettings {
    #[getter]
    fn temperature(&self) -> Option<f32> {
        self.s().temperature
    }
    #[getter]
    fn top_p(&self) -> Option<f32> {
        self.s().top_p
    }
    #[getter]
    fn max_output_tokens(&self) -> Option<u32> {
        self.s().max_output_tokens
    }
    #[getter]
    fn reasoning(&self) -> Option<PyOpenAiReasoning> {
        self.s().reasoning.as_ref().map(|_| PyOpenAiReasoning {
            inner: Arc::clone(&self.inner),
        })
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
    fn extra(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        wyrd_utils::py::json_to_pyobject(py, &serde_json::Value::Object(self.s().extra.clone()))
            .map_err(Into::into)
    }
    fn __repr__(&self) -> String {
        "OpenAiResponsesSettings".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiResponsesText")]
pub struct PyOpenAiResponsesText {
    inner: Arc<ProviderRequest>,
}
impl PyOpenAiResponsesText {
    fn t(&self) -> &OpenAiResponsesText {
        match self.inner.as_ref() {
            ProviderRequest::OpenAiResponses(r) => r.text.as_ref().expect("guarded"),
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiResponsesText {
    #[getter]
    fn format_kind(&self) -> Option<&'static str> {
        self.t().format.as_ref().map(|f| match f {
            OpenAiTextResponseFormat::Text => "text",
            OpenAiTextResponseFormat::JsonObject => "json_object",
            OpenAiTextResponseFormat::JsonSchema { .. } => "json_schema",
        })
    }
    fn __repr__(&self) -> String {
        "OpenAiResponsesText".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiReasoning")]
pub struct PyOpenAiReasoning {
    inner: Arc<ProviderRequest>,
}
impl PyOpenAiReasoning {
    fn r(&self) -> &OpenAiReasoning {
        match self.inner.as_ref() {
            ProviderRequest::OpenAiResponses(r) => r.settings.reasoning.as_ref().expect("guarded"),
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiReasoning {
    #[getter]
    fn effort(&self) -> Option<&'static str> {
        use skald_spec::wire::openai_responses::OpenAiReasoningEffort as RE;
        self.r().effort.as_ref().map(|e| match e {
            RE::None => "none",
            RE::Minimal => "minimal",
            RE::Low => "low",
            RE::Medium => "medium",
            RE::High => "high",
            RE::Xhigh => "xhigh",
        })
    }
    #[getter]
    fn summary(&self) -> Option<&'static str> {
        self.r().summary.as_ref().map(|s| match s {
            OpenAiReasoningSummary::Auto => "auto",
            OpenAiReasoningSummary::Concise => "concise",
            OpenAiReasoningSummary::Detailed => "detailed",
        })
    }
    fn __repr__(&self) -> String {
        "OpenAiReasoning".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiResponsesToolChoice")]
pub struct PyOpenAiResponsesToolChoice {
    inner: Arc<ProviderRequest>,
}
impl PyOpenAiResponsesToolChoice {
    fn c(&self) -> &OpenAiResponsesToolChoice {
        match self.inner.as_ref() {
            ProviderRequest::OpenAiResponses(r) => r.tool_choice.as_ref().expect("guarded"),
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiResponsesToolChoice {
    #[getter]
    fn kind(&self) -> &'static str {
        match self.c() {
            OpenAiResponsesToolChoice::Mode(_) => "mode",
            OpenAiResponsesToolChoice::Allowed(_) => "allowed",
            OpenAiResponsesToolChoice::Hosted(_) => "hosted",
            OpenAiResponsesToolChoice::Function(_) => "function",
            OpenAiResponsesToolChoice::Mcp(_) => "mcp",
            OpenAiResponsesToolChoice::Custom(_) => "custom",
            OpenAiResponsesToolChoice::ApplyPatch(_) => "apply_patch",
            OpenAiResponsesToolChoice::Shell(_) => "shell",
        }
    }
    fn as_mode(&self) -> CardPyResult<&'static str> {
        match self.c() {
            OpenAiResponsesToolChoice::Mode(m) => Ok(match m {
                OpenAiResponsesToolChoiceMode::None => "none",
                OpenAiResponsesToolChoiceMode::Auto => "auto",
                OpenAiResponsesToolChoiceMode::Required => "required",
            }),
            _ => Err(wrong_variant("mode", self.kind()).into()),
        }
    }
    fn as_hosted(&self) -> CardPyResult<PyOpenAiResponsesHostedToolChoice> {
        match self.c() {
            OpenAiResponsesToolChoice::Hosted(_) => Ok(PyOpenAiResponsesHostedToolChoice {
                inner: Arc::clone(&self.inner),
            }),
            _ => Err(wrong_variant("hosted", self.kind()).into()),
        }
    }
    fn as_function_choice(&self) -> CardPyResult<PyOpenAiResponsesFunctionToolChoice> {
        match self.c() {
            OpenAiResponsesToolChoice::Function(_) => Ok(PyOpenAiResponsesFunctionToolChoice {
                inner: Arc::clone(&self.inner),
            }),
            _ => Err(wrong_variant("function", self.kind()).into()),
        }
    }
    fn as_allowed(&self) -> CardPyResult<PyOpenAiResponsesAllowedToolsChoice> {
        match self.c() {
            OpenAiResponsesToolChoice::Allowed(_) => Ok(PyOpenAiResponsesAllowedToolsChoice {
                inner: Arc::clone(&self.inner),
            }),
            _ => Err(wrong_variant("allowed", self.kind()).into()),
        }
    }
    fn as_mcp(&self) -> CardPyResult<PyOpenAiResponsesMcpToolChoice> {
        match self.c() {
            OpenAiResponsesToolChoice::Mcp(_) => Ok(PyOpenAiResponsesMcpToolChoice {
                inner: Arc::clone(&self.inner),
            }),
            _ => Err(wrong_variant("mcp", self.kind()).into()),
        }
    }
    fn as_custom_choice(&self) -> CardPyResult<PyOpenAiResponsesCustomToolChoice> {
        match self.c() {
            OpenAiResponsesToolChoice::Custom(_) => Ok(PyOpenAiResponsesCustomToolChoice {
                inner: Arc::clone(&self.inner),
            }),
            _ => Err(wrong_variant("custom", self.kind()).into()),
        }
    }
    fn as_apply_patch(&self) -> CardPyResult<PyOpenAiResponsesApplyPatchToolChoice> {
        match self.c() {
            OpenAiResponsesToolChoice::ApplyPatch(_) => Ok(PyOpenAiResponsesApplyPatchToolChoice {
                inner: Arc::clone(&self.inner),
            }),
            _ => Err(wrong_variant("apply_patch", self.kind()).into()),
        }
    }
    fn as_shell(&self) -> CardPyResult<PyOpenAiResponsesShellToolChoice> {
        match self.c() {
            OpenAiResponsesToolChoice::Shell(_) => Ok(PyOpenAiResponsesShellToolChoice {
                inner: Arc::clone(&self.inner),
            }),
            _ => Err(wrong_variant("shell", self.kind()).into()),
        }
    }
    fn __repr__(&self) -> String {
        format!("OpenAiResponsesToolChoice(kind={:?})", self.kind())
    }
}

macro_rules! simple_responses_choice {
    ($name:ident, $py_name:literal, $variant:ident, $inner_ty:ident, $kind_fn:expr) => {
        #[pyclass(module = "wyrd.prompt", name = $py_name)]
        pub struct $name {
            inner: Arc<ProviderRequest>,
        }
        impl $name {
            fn c(&self) -> &$inner_ty {
                match self.inner.as_ref() {
                    ProviderRequest::OpenAiResponses(r) => {
                        match r.tool_choice.as_ref().expect("guarded") {
                            OpenAiResponsesToolChoice::$variant(c) => c,
                            _ => unreachable!(),
                        }
                    }
                    _ => unreachable!(),
                }
            }
        }
    };
}

simple_responses_choice!(
    PyOpenAiResponsesAllowedToolsChoice,
    "OpenAiResponsesAllowedToolsChoice",
    Allowed,
    OpenAiResponsesAllowedToolsChoice,
    |_| "allowed_tools"
);
#[pymethods]
impl PyOpenAiResponsesAllowedToolsChoice {
    #[getter]
    fn kind(&self) -> &'static str {
        match self.c().kind {
            OpenAiResponsesAllowedToolsKind::AllowedTools => "allowed_tools",
        }
    }
    #[getter]
    fn mode(&self) -> &'static str {
        match self.c().mode {
            OpenAiResponsesAllowedToolsMode::Auto => "auto",
            OpenAiResponsesAllowedToolsMode::Required => "required",
        }
    }
    fn __repr__(&self) -> String {
        "OpenAiResponsesAllowedToolsChoice".to_owned()
    }
}

simple_responses_choice!(
    PyOpenAiResponsesHostedToolChoice,
    "OpenAiResponsesHostedToolChoice",
    Hosted,
    OpenAiResponsesHostedToolChoice,
    |_| "hosted"
);
#[pymethods]
impl PyOpenAiResponsesHostedToolChoice {
    #[getter]
    fn kind(&self) -> &'static str {
        match self.c().kind {
            OpenAiResponsesHostedToolKind::FileSearch => "file_search",
            OpenAiResponsesHostedToolKind::WebSearchPreview => "web_search_preview",
            OpenAiResponsesHostedToolKind::Computer => "computer",
            OpenAiResponsesHostedToolKind::ComputerUsePreview => "computer_use_preview",
            OpenAiResponsesHostedToolKind::ComputerUse => "computer_use",
            OpenAiResponsesHostedToolKind::WebSearchPreview20250311 => {
                "web_search_preview_2025_03_11"
            }
            OpenAiResponsesHostedToolKind::ImageGeneration => "image_generation",
            OpenAiResponsesHostedToolKind::CodeInterpreter => "code_interpreter",
        }
    }
    fn __repr__(&self) -> String {
        format!("OpenAiResponsesHostedToolChoice(kind={:?})", self.kind())
    }
}

simple_responses_choice!(
    PyOpenAiResponsesFunctionToolChoice,
    "OpenAiResponsesFunctionToolChoice",
    Function,
    OpenAiResponsesFunctionToolChoice,
    |_| "function"
);
#[pymethods]
impl PyOpenAiResponsesFunctionToolChoice {
    #[getter]
    fn kind(&self) -> &'static str {
        match self.c().kind {
            OpenAiResponsesFunctionToolKind::Function => "function",
        }
    }
    #[getter]
    fn name(&self) -> &str {
        &self.c().name
    }
    fn __repr__(&self) -> String {
        format!(
            "OpenAiResponsesFunctionToolChoice(name={:?})",
            self.c().name
        )
    }
}

simple_responses_choice!(
    PyOpenAiResponsesMcpToolChoice,
    "OpenAiResponsesMcpToolChoice",
    Mcp,
    OpenAiResponsesMcpToolChoice,
    |_| "mcp"
);
#[pymethods]
impl PyOpenAiResponsesMcpToolChoice {
    #[getter]
    fn kind(&self) -> &'static str {
        match self.c().kind {
            OpenAiResponsesMcpToolKind::Mcp => "mcp",
        }
    }
    #[getter]
    fn server_label(&self) -> &str {
        &self.c().server_label
    }
    #[getter]
    fn name(&self) -> Option<&str> {
        self.c().name.as_deref()
    }
    fn __repr__(&self) -> String {
        "OpenAiResponsesMcpToolChoice".to_owned()
    }
}

simple_responses_choice!(
    PyOpenAiResponsesCustomToolChoice,
    "OpenAiResponsesCustomToolChoice",
    Custom,
    OpenAiResponsesCustomToolChoice,
    |_| "custom"
);
#[pymethods]
impl PyOpenAiResponsesCustomToolChoice {
    #[getter]
    fn kind(&self) -> &'static str {
        match self.c().kind {
            skald_spec::wire::openai_responses::OpenAiResponsesCustomToolKind::Custom => "custom",
        }
    }
    #[getter]
    fn name(&self) -> &str {
        &self.c().name
    }
    fn __repr__(&self) -> String {
        format!("OpenAiResponsesCustomToolChoice(name={:?})", self.c().name)
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiResponsesApplyPatchToolChoice")]
pub struct PyOpenAiResponsesApplyPatchToolChoice {
    inner: Arc<ProviderRequest>,
}
#[pymethods]
impl PyOpenAiResponsesApplyPatchToolChoice {
    #[getter]
    fn kind(&self) -> &'static str {
        "apply_patch"
    }
    fn __repr__(&self) -> String {
        "OpenAiResponsesApplyPatchToolChoice".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiResponsesShellToolChoice")]
pub struct PyOpenAiResponsesShellToolChoice {
    inner: Arc<ProviderRequest>,
}
#[pymethods]
impl PyOpenAiResponsesShellToolChoice {
    #[getter]
    fn kind(&self) -> &'static str {
        "shell"
    }
    fn __repr__(&self) -> String {
        "OpenAiResponsesShellToolChoice".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiResponseItem")]
pub struct PyOpenAiResponseItem {
    inner: Arc<ProviderRequest>,
    index: usize,
}
impl PyOpenAiResponseItem {
    fn i(&self) -> &OpenAiResponseItem {
        match self.inner.as_ref() {
            ProviderRequest::OpenAiResponses(r) => &r.input[self.index],
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiResponseItem {
    #[getter]
    fn kind(&self) -> &'static str {
        match self.i() {
            OpenAiResponseItem::Message { .. } => "message",
            OpenAiResponseItem::FunctionCall { .. } => "function_call",
            OpenAiResponseItem::FunctionCallOutput { .. } => "function_call_output",
            OpenAiResponseItem::Reasoning { .. } => "reasoning",
            OpenAiResponseItem::InputFile { .. } => "input_file",
            OpenAiResponseItem::InputImage { .. } => "input_image",
        }
    }
    fn as_message_role(&self) -> CardPyResult<String> {
        match self.i() {
            OpenAiResponseItem::Message { role, .. } => Ok(role.clone()),
            _ => Err(wrong_variant("message", self.kind()).into()),
        }
    }
    fn as_function_call_name(&self) -> CardPyResult<String> {
        match self.i() {
            OpenAiResponseItem::FunctionCall { name, .. } => Ok(name.clone()),
            _ => Err(wrong_variant("function_call", self.kind()).into()),
        }
    }
    fn as_function_call_arguments(&self) -> CardPyResult<String> {
        match self.i() {
            OpenAiResponseItem::FunctionCall { arguments, .. } => Ok(arguments.clone()),
            _ => Err(wrong_variant("function_call", self.kind()).into()),
        }
    }
    fn as_function_call_output(&self) -> CardPyResult<String> {
        match self.i() {
            OpenAiResponseItem::FunctionCallOutput { output, .. } => Ok(output.clone()),
            _ => Err(wrong_variant("function_call_output", self.kind()).into()),
        }
    }
    fn __repr__(&self) -> String {
        format!("OpenAiResponseItem(kind={:?})", self.kind())
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiResponseContentPart")]
pub struct PyOpenAiResponseContentPart {
    value: OpenAiResponseContentPart,
}
#[pymethods]
impl PyOpenAiResponseContentPart {
    #[getter]
    fn kind(&self) -> &'static str {
        match &self.value {
            OpenAiResponseContentPart::InputText { .. } => "input_text",
            OpenAiResponseContentPart::OutputText { .. } => "output_text",
            OpenAiResponseContentPart::InputImage { .. } => "input_image",
            OpenAiResponseContentPart::InputFile { .. } => "input_file",
        }
    }
    fn as_text(&self) -> CardPyResult<String> {
        match &self.value {
            OpenAiResponseContentPart::InputText { text }
            | OpenAiResponseContentPart::OutputText { text } => Ok(text.clone()),
            _ => Err(wrong_variant("text", self.kind()).into()),
        }
    }
    fn __repr__(&self) -> String {
        format!("OpenAiResponseContentPart(kind={:?})", self.kind())
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiResponsesTool")]
pub struct PyOpenAiResponsesTool {
    inner: Arc<ProviderRequest>,
    index: usize,
}
impl PyOpenAiResponsesTool {
    fn t(&self) -> &OpenAiResponsesTool {
        match self.inner.as_ref() {
            ProviderRequest::OpenAiResponses(r) => &r.tools.as_ref().expect("guarded")[self.index],
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiResponsesTool {
    #[getter]
    fn kind(&self) -> &'static str {
        match self.t() {
            OpenAiResponsesTool::Function { .. } => "function",
            OpenAiResponsesTool::WebSearchPreview {} => "web_search_preview",
            OpenAiResponsesTool::FileSearch { .. } => "file_search",
            OpenAiResponsesTool::CodeInterpreter {} => "code_interpreter",
            OpenAiResponsesTool::ImageGeneration {} => "image_generation",
            OpenAiResponsesTool::Mcp { .. } => "mcp",
            OpenAiResponsesTool::Custom { .. } => "custom",
        }
    }
    fn as_function_name(&self) -> CardPyResult<String> {
        match self.t() {
            OpenAiResponsesTool::Function { name, .. } => Ok(name.clone()),
            _ => Err(wrong_variant("function", self.kind()).into()),
        }
    }
    fn as_mcp_server_label(&self) -> CardPyResult<String> {
        match self.t() {
            OpenAiResponsesTool::Mcp { server_label, .. } => Ok(server_label.clone()),
            _ => Err(wrong_variant("mcp", self.kind()).into()),
        }
    }
    fn as_custom_name(&self) -> CardPyResult<String> {
        match self.t() {
            OpenAiResponsesTool::Custom { name, .. } => Ok(name.clone()),
            _ => Err(wrong_variant("custom", self.kind()).into()),
        }
    }
    fn __repr__(&self) -> String {
        format!("OpenAiResponsesTool(kind={:?})", self.kind())
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiResponsesGrammar")]
pub struct PyOpenAiResponsesGrammar {
    inner: Arc<ProviderRequest>,
    tool_index: usize,
}
impl PyOpenAiResponsesGrammar {
    fn g(&self) -> &OpenAiResponsesGrammar {
        match self.inner.as_ref() {
            ProviderRequest::OpenAiResponses(r) => {
                match &r.tools.as_ref().expect("guarded")[self.tool_index] {
                    OpenAiResponsesTool::Custom {
                        format: Some(OpenAiResponsesCustomToolFormat::Grammar { grammar }),
                        ..
                    } => grammar,
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiResponsesGrammar {
    #[getter]
    fn definition(&self) -> &str {
        &self.g().definition
    }
    #[getter]
    fn syntax(&self) -> &'static str {
        match self.g().syntax {
            OpenAiResponsesGrammarSyntax::Lark => "lark",
            OpenAiResponsesGrammarSyntax::Regex => "regex",
        }
    }
    fn __repr__(&self) -> String {
        "OpenAiResponsesGrammar".to_owned()
    }
}

// OpenAI Responses response

#[pyclass(module = "wyrd.prompt", name = "OpenAiResponsesResponse")]
pub struct PyOpenAiResponsesResponse {
    inner: Arc<ProviderResponse>,
}
impl PyOpenAiResponsesResponse {
    pub fn new(inner: Arc<ProviderResponse>) -> Self {
        Self { inner }
    }
    fn resp(&self) -> &skald_spec::wire::openai_responses::OpenAiResponsesResponse {
        match self.inner.as_ref() {
            ProviderResponse::OpenAiResponses(r) => r,
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiResponsesResponse {
    #[getter]
    fn id(&self) -> &str {
        &self.resp().id
    }
    #[getter]
    fn object(&self) -> &str {
        &self.resp().object
    }
    #[getter]
    fn model(&self) -> &str {
        &self.resp().model
    }
    #[getter]
    fn status(&self) -> &str {
        &self.resp().status
    }
    #[getter]
    fn created_at(&self) -> u64 {
        self.resp().created_at
    }
    #[getter]
    fn previous_response_id(&self) -> Option<&str> {
        self.resp().previous_response_id.as_deref()
    }
    #[getter]
    fn usage(&self) -> Option<PyOpenAiResponsesUsage> {
        self.resp().usage.as_ref().map(|_| PyOpenAiResponsesUsage {
            inner: Arc::clone(&self.inner),
        })
    }
    fn __repr__(&self) -> String {
        format!(
            "OpenAiResponsesResponse(id={:?}, model={:?})",
            self.resp().id,
            self.resp().model
        )
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiResponsesUsage")]
pub struct PyOpenAiResponsesUsage {
    inner: Arc<ProviderResponse>,
}
impl PyOpenAiResponsesUsage {
    fn u(&self) -> &OpenAiResponsesUsage {
        match self.inner.as_ref() {
            ProviderResponse::OpenAiResponses(r) => r.usage.as_ref().expect("guarded"),
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiResponsesUsage {
    #[getter]
    fn input_tokens(&self) -> u64 {
        self.u().input_tokens
    }
    #[getter]
    fn output_tokens(&self) -> u64 {
        self.u().output_tokens
    }
    #[getter]
    fn total_tokens(&self) -> u64 {
        self.u().total_tokens
    }
    #[getter]
    fn input_tokens_details(&self) -> Option<PyOpenAiResponsesInputTokensDetails> {
        self.u()
            .input_tokens_details
            .as_ref()
            .map(|_| PyOpenAiResponsesInputTokensDetails {
                inner: Arc::clone(&self.inner),
            })
    }
    #[getter]
    fn output_tokens_details(&self) -> Option<PyOpenAiResponsesOutputTokensDetails> {
        self.u()
            .output_tokens_details
            .as_ref()
            .map(|_| PyOpenAiResponsesOutputTokensDetails {
                inner: Arc::clone(&self.inner),
            })
    }
    fn __repr__(&self) -> String {
        "OpenAiResponsesUsage".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiResponsesInputTokensDetails")]
pub struct PyOpenAiResponsesInputTokensDetails {
    inner: Arc<ProviderResponse>,
}
impl PyOpenAiResponsesInputTokensDetails {
    fn d(&self) -> &OpenAiResponsesInputTokensDetails {
        match self.inner.as_ref() {
            ProviderResponse::OpenAiResponses(r) => r
                .usage
                .as_ref()
                .expect("guarded")
                .input_tokens_details
                .as_ref()
                .expect("guarded"),
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiResponsesInputTokensDetails {
    #[getter]
    fn cached_tokens(&self) -> u64 {
        self.d().cached_tokens
    }
    fn __repr__(&self) -> String {
        "OpenAiResponsesInputTokensDetails".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiResponsesOutputTokensDetails")]
pub struct PyOpenAiResponsesOutputTokensDetails {
    inner: Arc<ProviderResponse>,
}
impl PyOpenAiResponsesOutputTokensDetails {
    fn d(&self) -> &OpenAiResponsesOutputTokensDetails {
        match self.inner.as_ref() {
            ProviderResponse::OpenAiResponses(r) => r
                .usage
                .as_ref()
                .expect("guarded")
                .output_tokens_details
                .as_ref()
                .expect("guarded"),
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiResponsesOutputTokensDetails {
    #[getter]
    fn reasoning_tokens(&self) -> u64 {
        self.d().reasoning_tokens
    }
    fn __repr__(&self) -> String {
        "OpenAiResponsesOutputTokensDetails".to_owned()
    }
}
