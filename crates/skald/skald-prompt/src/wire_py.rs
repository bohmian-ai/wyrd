//! Typed Python pyclass mirrors for every in-scope provider wire struct.
//!
//! One clone per boundary crossing (callback/observer receives `PyProviderRequest` /
//! `PyProviderResponse`). Every nested field access after that is zero-copy via the `Arc`.
// PyO3 `__repr__` methods must take `&self` even when they return a static string.
#![allow(clippy::unused_self)]
// PyO3 bridge code uses match-let patterns that predate `let…else` and are clearer inline.
#![allow(clippy::manual_let_else)]
// Large bridge file: exhaustive single-remaining-variant matches are overly verbose.
#![allow(clippy::match_wildcard_for_single_variants)]
// `|v| v.len()` is clearer than `Vec::len` in closure context here.
#![allow(clippy::redundant_closure_for_method_calls)]
// CardPyResult return types are intentionally uniform even when infallible.
#![allow(clippy::unnecessary_wraps)]
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
    OpenAiCustomToolFormat, OpenAiFilePart, OpenAiFunctionChoice, OpenAiGrammar,
    OpenAiGrammarSyntax, OpenAiImageUrl, OpenAiInputAudio, OpenAiMessageAnnotation,
    OpenAiMessageAudio, OpenAiMessageContent, OpenAiNamedCustomToolChoice,
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
    OpenAiResponsesAllowedToolsMode, OpenAiResponsesCustomToolChoice,
    OpenAiResponsesFunctionToolChoice, OpenAiResponsesFunctionToolKind, OpenAiResponsesGrammar,
    OpenAiResponsesGrammarSyntax, OpenAiResponsesHostedToolChoice, OpenAiResponsesHostedToolKind,
    OpenAiResponsesInputTokensDetails, OpenAiResponsesMcpToolChoice, OpenAiResponsesMcpToolKind,
    OpenAiResponsesOutputTokensDetails, OpenAiResponsesRequest, OpenAiResponsesSettings,
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
        self.req()
            .messages
            .iter()
            .map(|m| PyOpenAiChatMessage {
                inner: Arc::new(m.clone()),
            })
            .collect()
    }
    #[getter]
    fn response_format(&self) -> Option<PyOpenAiResponseFormat> {
        self.req()
            .response_format
            .as_ref()
            .map(|f| PyOpenAiResponseFormat {
                inner: Arc::new(f.clone()),
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
            .map(|s| PyOpenAiStreamOptions {
                inner: Arc::new(s.clone()),
            })
    }
    #[getter]
    fn tools(&self) -> Vec<PyOpenAiTool> {
        self.req()
            .tools
            .as_ref()
            .map(|ts| {
                ts.iter()
                    .map(|t| PyOpenAiTool {
                        inner: Arc::new(t.clone()),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    #[getter]
    fn tool_choice(&self) -> Option<PyOpenAiChatToolChoice> {
        self.req()
            .tool_choice
            .as_ref()
            .map(|c| PyOpenAiChatToolChoice {
                inner: Arc::new(c.clone()),
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
        self.s().stop.as_ref().map(|stop| PyOpenAiStop {
            inner: Arc::new(stop.clone()),
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
        self.s().audio.as_ref().map(|audio| PyOpenAiChatAudio {
            inner: Arc::new(audio.clone()),
        })
    }
    #[getter]
    fn prediction(&self) -> Option<PyOpenAiPredictionContent> {
        self.s()
            .prediction
            .as_ref()
            .map(|p| PyOpenAiPredictionContent {
                inner: Arc::new(p.clone()),
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
    inner: Arc<OpenAiStop>,
}
impl PyOpenAiStop {
    fn s(&self) -> &OpenAiStop {
        &self.inner
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
    inner: Arc<OpenAiChatAudio>,
}
impl PyOpenAiChatAudio {
    fn a(&self) -> &OpenAiChatAudio {
        &self.inner
    }
}
#[pymethods]
impl PyOpenAiChatAudio {
    #[getter]
    fn voice(&self) -> PyOpenAiVoice {
        PyOpenAiVoice {
            inner: Arc::new(self.inner.voice.clone()),
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
    inner: Arc<OpenAiVoice>,
}
impl PyOpenAiVoice {
    fn v(&self) -> &OpenAiVoice {
        &self.inner
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
    inner: Arc<OpenAiPredictionContent>,
}
impl PyOpenAiPredictionContent {
    fn p(&self) -> &OpenAiPredictionContent {
        &self.inner
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
            inner: Arc::new(self.inner.content.clone()),
        }
    }
    fn __repr__(&self) -> String {
        "OpenAiPredictionContent".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiPredictionPayload")]
pub struct PyOpenAiPredictionPayload {
    inner: Arc<OpenAiPredictionPayload>,
}
impl PyOpenAiPredictionPayload {
    fn p(&self) -> &OpenAiPredictionPayload {
        &self.inner
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
            OpenAiPredictionPayload::Parts(ps) => Ok(ps
                .iter()
                .map(|p| PyOpenAiPredictionContentPart {
                    inner: Arc::new(p.clone()),
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
    inner: Arc<OpenAiPredictionContentPart>,
}
impl PyOpenAiPredictionContentPart {
    fn p(&self) -> &OpenAiPredictionContentPart {
        &self.inner
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
    inner: Arc<OpenAiStreamOptions>,
}
impl PyOpenAiStreamOptions {
    fn s(&self) -> &OpenAiStreamOptions {
        &self.inner
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
    inner: Arc<OpenAiResponseFormat>,
}
impl PyOpenAiResponseFormat {
    fn f(&self) -> &OpenAiResponseFormat {
        &self.inner
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
            OpenAiResponseFormat::JsonSchema { json_schema } => Ok(PyOpenAiJsonSchema {
                inner: Arc::new(json_schema.clone()),
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
    inner: Arc<skald_spec::wire::openai_chat::OpenAiJsonSchema>,
}
impl PyOpenAiJsonSchema {
    fn s(&self) -> &skald_spec::wire::openai_chat::OpenAiJsonSchema {
        &self.inner
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
    inner: Arc<OpenAiTool>,
}
impl PyOpenAiTool {
    fn t(&self) -> &OpenAiTool {
        &self.inner
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
            OpenAiTool::Function { function } => Ok(PyOpenAiFunction {
                inner: Arc::new(function.clone()),
            }),
            _ => Err(wrong_variant("function", self.kind()).into()),
        }
    }
    fn as_custom_tool(&self) -> CardPyResult<PyOpenAiCustomTool> {
        match self.t() {
            OpenAiTool::Custom { custom } => Ok(PyOpenAiCustomTool {
                inner: Arc::new(custom.clone()),
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
    inner: Arc<skald_spec::wire::openai_chat::OpenAiFunction>,
}
impl PyOpenAiFunction {
    fn f(&self) -> &skald_spec::wire::openai_chat::OpenAiFunction {
        &self.inner
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
    inner: Arc<OpenAiCustomTool>,
}
impl PyOpenAiCustomTool {
    fn c(&self) -> &OpenAiCustomTool {
        &self.inner
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
        self.c().format.as_ref().map(|f| PyOpenAiCustomToolFormat {
            inner: Arc::new(f.clone()),
        })
    }
    fn __repr__(&self) -> String {
        format!("OpenAiCustomTool(name={:?})", self.c().name)
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiCustomToolFormat")]
pub struct PyOpenAiCustomToolFormat {
    inner: Arc<OpenAiCustomToolFormat>,
}
impl PyOpenAiCustomToolFormat {
    fn f(&self) -> &OpenAiCustomToolFormat {
        &self.inner
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
            OpenAiCustomToolFormat::Grammar { grammar } => Ok(PyOpenAiGrammar {
                inner: Arc::new(grammar.clone()),
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
    inner: Arc<OpenAiGrammar>,
}
impl PyOpenAiGrammar {
    fn g(&self) -> &OpenAiGrammar {
        &self.inner
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
    inner: Arc<OpenAiChatToolChoice>,
}
impl PyOpenAiChatToolChoice {
    fn c(&self) -> &OpenAiChatToolChoice {
        &self.inner
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
            OpenAiChatToolChoice::Allowed(a) => Ok(PyOpenAiAllowedToolsChoice {
                inner: Arc::new(a.clone()),
            }),
            _ => Err(wrong_variant("allowed", self.kind()).into()),
        }
    }
    fn as_function_choice(&self) -> CardPyResult<PyOpenAiNamedFunctionToolChoice> {
        match self.c() {
            OpenAiChatToolChoice::Function(f) => Ok(PyOpenAiNamedFunctionToolChoice {
                inner: Arc::new(f.clone()),
            }),
            _ => Err(wrong_variant("function", self.kind()).into()),
        }
    }
    fn as_custom_choice(&self) -> CardPyResult<PyOpenAiNamedCustomToolChoice> {
        match self.c() {
            OpenAiChatToolChoice::Custom(c) => Ok(PyOpenAiNamedCustomToolChoice {
                inner: Arc::new(c.clone()),
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
    inner: Arc<OpenAiAllowedToolsChoice>,
}
impl PyOpenAiAllowedToolsChoice {
    fn a(&self) -> &OpenAiAllowedToolsChoice {
        &self.inner
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
            inner: Arc::new(self.inner.allowed_tools.clone()),
        }
    }
    fn __repr__(&self) -> String {
        "OpenAiAllowedToolsChoice".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiAllowedTools")]
pub struct PyOpenAiAllowedTools {
    inner: Arc<OpenAiAllowedTools>,
}
impl PyOpenAiAllowedTools {
    fn a(&self) -> &OpenAiAllowedTools {
        &self.inner
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
    inner: Arc<OpenAiNamedFunctionToolChoice>,
}
impl PyOpenAiNamedFunctionToolChoice {
    fn n(&self) -> &OpenAiNamedFunctionToolChoice {
        &self.inner
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
            inner: Arc::new(self.inner.function.clone()),
        }
    }
    fn __repr__(&self) -> String {
        "OpenAiNamedFunctionToolChoice".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiFunctionChoice")]
pub struct PyOpenAiFunctionChoice {
    inner: Arc<OpenAiFunctionChoice>,
}
impl PyOpenAiFunctionChoice {
    fn f(&self) -> &OpenAiFunctionChoice {
        &self.inner
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
    inner: Arc<OpenAiNamedCustomToolChoice>,
}
impl PyOpenAiNamedCustomToolChoice {
    fn n(&self) -> &OpenAiNamedCustomToolChoice {
        &self.inner
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
            inner: Arc::new(self.inner.custom.clone()),
        }
    }
    fn __repr__(&self) -> String {
        "OpenAiNamedCustomToolChoice".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiCustomChoice")]
pub struct PyOpenAiCustomChoice {
    inner: Arc<OpenAiCustomChoice>,
}
impl PyOpenAiCustomChoice {
    fn c(&self) -> &OpenAiCustomChoice {
        &self.inner
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
    pub(crate) inner: Arc<OpenAiChatMessage>,
}
impl PyOpenAiChatMessage {
    fn m(&self) -> &OpenAiChatMessage {
        &self.inner
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
        self.m().content.as_ref().map(|c| PyOpenAiMessageContent {
            inner: Arc::new(c.clone()),
        })
    }
    #[getter]
    fn name(&self) -> Option<&str> {
        self.m().name.as_deref()
    }
    #[getter]
    fn tool_calls(&self) -> Option<Vec<PyOpenAiToolCall>> {
        self.m().tool_calls.as_ref().map(|tc| {
            tc.iter()
                .map(|c| PyOpenAiToolCall {
                    inner: Arc::new(c.clone()),
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
        self.m()
            .annotations
            .iter()
            .map(|a| PyOpenAiMessageAnnotation {
                inner: Arc::new(a.clone()),
            })
            .collect()
    }
    #[getter]
    fn audio(&self) -> Option<PyOpenAiMessageAudio> {
        self.m().audio.as_ref().map(|a| PyOpenAiMessageAudio {
            inner: Arc::new(a.clone()),
        })
    }
    fn __repr__(&self) -> String {
        format!("OpenAiChatMessage(role={:?})", self.m().role)
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiMessageContent")]
pub struct PyOpenAiMessageContent {
    inner: Arc<OpenAiMessageContent>,
}
impl PyOpenAiMessageContent {
    fn c(&self) -> &OpenAiMessageContent {
        &self.inner
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
            OpenAiMessageContent::Parts(_) => Err(wrong_variant("text", self.kind()).into()),
        }
    }
    fn as_parts(&self) -> CardPyResult<Vec<PyOpenAiContentPart>> {
        match self.c() {
            OpenAiMessageContent::Parts(ps) => Ok(ps
                .iter()
                .map(|p| PyOpenAiContentPart {
                    inner: Arc::new(p.clone()),
                })
                .collect()),
            OpenAiMessageContent::Text(_) => Err(wrong_variant("parts", self.kind()).into()),
        }
    }
    fn __repr__(&self) -> String {
        format!("OpenAiMessageContent(kind={:?})", self.kind())
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiContentPart")]
pub struct PyOpenAiContentPart {
    inner: Arc<OpenAiContentPart>,
}
impl PyOpenAiContentPart {
    fn p(&self) -> &OpenAiContentPart {
        &self.inner
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
            OpenAiContentPart::ImageUrl { image_url } => Ok(PyOpenAiImageUrl {
                inner: Arc::new(image_url.clone()),
            }),
            _ => Err(wrong_variant("image_url", self.kind()).into()),
        }
    }
    fn as_input_audio(&self) -> CardPyResult<PyOpenAiInputAudio> {
        match self.p() {
            OpenAiContentPart::InputAudio { input_audio } => Ok(PyOpenAiInputAudio {
                inner: Arc::new(input_audio.clone()),
            }),
            _ => Err(wrong_variant("input_audio", self.kind()).into()),
        }
    }
    fn as_file(&self) -> CardPyResult<PyOpenAiFilePart> {
        match self.p() {
            OpenAiContentPart::File { file } => Ok(PyOpenAiFilePart {
                inner: Arc::new(file.clone()),
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
    inner: Arc<OpenAiImageUrl>,
}
impl PyOpenAiImageUrl {
    fn i(&self) -> &OpenAiImageUrl {
        &self.inner
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
    inner: Arc<OpenAiInputAudio>,
}
impl PyOpenAiInputAudio {
    fn a(&self) -> &OpenAiInputAudio {
        &self.inner
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
    inner: Arc<OpenAiFilePart>,
}
impl PyOpenAiFilePart {
    fn f(&self) -> &OpenAiFilePart {
        &self.inner
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
    inner: Arc<OpenAiToolCall>,
}
impl PyOpenAiToolCall {
    fn c(&self) -> &OpenAiToolCall {
        &self.inner
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
            inner: Arc::new(self.inner.function.clone()),
        }
    }
    fn __repr__(&self) -> String {
        format!("OpenAiToolCall(id={:?})", self.c().id)
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiToolFunctionCall")]
pub struct PyOpenAiToolFunctionCall {
    inner: Arc<OpenAiToolFunctionCall>,
}
impl PyOpenAiToolFunctionCall {
    fn f(&self) -> &OpenAiToolFunctionCall {
        &self.inner
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
    inner: Arc<OpenAiMessageAnnotation>,
}
impl PyOpenAiMessageAnnotation {
    fn a(&self) -> &OpenAiMessageAnnotation {
        &self.inner
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
            inner: Arc::new(self.inner.url_citation.clone()),
        }
    }
    fn __repr__(&self) -> String {
        format!("OpenAiMessageAnnotation(kind={:?})", self.a().kind)
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiUrlCitation")]
pub struct PyOpenAiUrlCitation {
    inner: Arc<OpenAiUrlCitation>,
}
impl PyOpenAiUrlCitation {
    fn u(&self) -> &OpenAiUrlCitation {
        &self.inner
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
    inner: Arc<OpenAiMessageAudio>,
}
impl PyOpenAiMessageAudio {
    fn a(&self) -> &OpenAiMessageAudio {
        &self.inner
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
        self.resp().usage.as_ref().map(|u| PyOpenAiUsage {
            inner: Arc::new(u.clone()),
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
            inner: Arc::new(self.c().message.clone()),
        }
    }
    #[getter]
    fn logprobs(&self) -> Option<PyOpenAiChatLogprobs> {
        self.c().logprobs.as_ref().map(|l| PyOpenAiChatLogprobs {
            inner: Arc::new(l.clone()),
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
    inner: Arc<OpenAiChatLogprobs>,
}
impl PyOpenAiChatLogprobs {
    fn l(&self) -> &OpenAiChatLogprobs {
        &self.inner
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
    inner: Arc<OpenAiUsage>,
}
impl PyOpenAiUsage {
    fn u(&self) -> &OpenAiUsage {
        &self.inner
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
            .map(|d| PyOpenAiPromptTokensDetails {
                inner: Arc::new(d.clone()),
            })
    }
    #[getter]
    fn completion_tokens_details(&self) -> Option<PyOpenAiCompletionTokensDetails> {
        self.u()
            .completion_tokens_details
            .as_ref()
            .map(|d| PyOpenAiCompletionTokensDetails {
                inner: Arc::new(d.clone()),
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
    inner: Arc<OpenAiPromptTokensDetails>,
}
impl PyOpenAiPromptTokensDetails {
    fn d(&self) -> &OpenAiPromptTokensDetails {
        &self.inner
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
    inner: Arc<OpenAiCompletionTokensDetails>,
}
impl PyOpenAiCompletionTokensDetails {
    fn d(&self) -> &OpenAiCompletionTokensDetails {
        &self.inner
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
        self.req().system.as_ref().map(|s| PyAnthropicSystem {
            inner: Arc::new(s.clone()),
        })
    }
    #[getter]
    fn stream(&self) -> Option<bool> {
        self.req().stream
    }
    #[getter]
    fn tools(&self) -> Vec<PyAnthropicTool> {
        self.req()
            .tools
            .as_ref()
            .map(|ts| {
                ts.iter()
                    .map(|t| PyAnthropicTool {
                        inner: Arc::new(t.clone()),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    #[getter]
    fn output_config(&self) -> Option<PyAnthropicOutputConfig> {
        self.req()
            .output_config
            .as_ref()
            .map(|c| PyAnthropicOutputConfig {
                inner: Arc::new(c.clone()),
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
            .map(|t| PyAnthropicThinkingConfig {
                inner: Arc::new(t.clone()),
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
    inner: Arc<AnthropicSystem>,
}
impl PyAnthropicSystem {
    fn s(&self) -> &AnthropicSystem {
        &self.inner
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
            AnthropicSystem::Blocks(bs) => Ok(bs
                .iter()
                .map(|b| PyAnthropicSystemBlock {
                    inner: Arc::new(b.clone()),
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
    inner: Arc<AnthropicSystemBlock>,
}
impl PyAnthropicSystemBlock {
    fn b(&self) -> &AnthropicSystemBlock {
        &self.inner
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
                cache_control.as_ref().map(|cc| PyAnthropicCacheControl {
                    inner: Arc::new(cc.clone()),
                })
            }
        }
    }
    fn __repr__(&self) -> String {
        "AnthropicSystemBlock".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "AnthropicCacheControl")]
pub struct PyAnthropicCacheControl {
    inner: Arc<AnthropicCacheControl>,
}
impl PyAnthropicCacheControl {
    fn c(&self) -> &AnthropicCacheControl {
        &self.inner
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
    inner: Arc<AnthropicThinkingConfig>,
}
impl PyAnthropicThinkingConfig {
    fn t(&self) -> &AnthropicThinkingConfig {
        &self.inner
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
            AnthropicToolResultContent::Blocks(_) => Err(wrong_variant("text", self.kind()).into()),
        }
    }
    fn __repr__(&self) -> String {
        format!("AnthropicToolResultContent(kind={:?})", self.kind())
    }
}

#[pyclass(module = "wyrd.prompt", name = "AnthropicTool")]
pub struct PyAnthropicTool {
    inner: Arc<AnthropicTool>,
}
impl PyAnthropicTool {
    fn t(&self) -> &AnthropicTool {
        &self.inner
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
    inner: Arc<AnthropicOutputConfig>,
}
impl PyAnthropicOutputConfig {
    fn c(&self) -> &AnthropicOutputConfig {
        &self.inner
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
        google_request(&self.inner)
            .contents
            .iter()
            .map(|c| PyGoogleContent {
                inner: Arc::new(c.clone()),
            })
            .collect()
    }
    #[getter]
    fn system_instruction(&self) -> Option<PyGoogleContent> {
        google_request(&self.inner)
            .system_instruction
            .as_ref()
            .map(|c| PyGoogleContent {
                inner: Arc::new(c.clone()),
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
        google_request(&self.inner)
            .contents
            .iter()
            .map(|c| PyGoogleContent {
                inner: Arc::new(c.clone()),
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
    inner: Arc<GoogleContent>,
}
impl PyGoogleContent {
    fn c(&self) -> &GoogleContent {
        &self.inner
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
        self.c()
            .parts
            .iter()
            .map(|p| PyGooglePart {
                inner: Arc::new(p.clone()),
            })
            .collect()
    }
    fn __repr__(&self) -> String {
        format!("GoogleContent(role={:?})", self.c().role)
    }
}

#[pyclass(module = "wyrd.prompt", name = "GooglePart")]
pub struct PyGooglePart {
    inner: Arc<GooglePart>,
}
impl PyGooglePart {
    fn p(&self) -> &GooglePart {
        &self.inner
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
            GooglePart::InlineData { inline_data } => Ok(PyGoogleInlineData {
                inner: Arc::new(inline_data.clone()),
            }),
            _ => Err(wrong_variant("inline_data", self.kind()).into()),
        }
    }
    fn as_file_data(&self) -> CardPyResult<PyGoogleFileData> {
        match self.p() {
            GooglePart::FileData { file_data } => Ok(PyGoogleFileData {
                inner: Arc::new(file_data.clone()),
            }),
            _ => Err(wrong_variant("file_data", self.kind()).into()),
        }
    }
    fn as_function_call(&self) -> CardPyResult<PyGoogleFunctionCall> {
        match self.p() {
            GooglePart::FunctionCall { function_call } => Ok(PyGoogleFunctionCall {
                inner: Arc::new(function_call.clone()),
            }),
            _ => Err(wrong_variant("function_call", self.kind()).into()),
        }
    }
    fn as_function_response(&self) -> CardPyResult<PyGoogleFunctionResponse> {
        match self.p() {
            GooglePart::FunctionResponse { function_response } => Ok(PyGoogleFunctionResponse {
                inner: Arc::new(function_response.clone()),
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
    inner: Arc<GoogleInlineData>,
}
impl PyGoogleInlineData {
    fn d(&self) -> &GoogleInlineData {
        &self.inner
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
    inner: Arc<GoogleFileData>,
}
impl PyGoogleFileData {
    fn d(&self) -> &GoogleFileData {
        &self.inner
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
    inner: Arc<GoogleFunctionCall>,
}
impl PyGoogleFunctionCall {
    fn f(&self) -> &GoogleFunctionCall {
        &self.inner
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
    inner: Arc<GoogleFunctionResponse>,
}
impl PyGoogleFunctionResponse {
    fn f(&self) -> &GoogleFunctionResponse {
        &self.inner
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
            .map(|t| PyGoogleThinkingConfig {
                inner: Arc::new(t.clone()),
            })
    }
    fn __repr__(&self) -> String {
        "GoogleGenerationConfig".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "GoogleThinkingConfig")]
pub struct PyGoogleThinkingConfig {
    inner: Arc<GoogleThinkingConfig>,
}
impl PyGoogleThinkingConfig {
    fn t(&self) -> &GoogleThinkingConfig {
        &self.inner
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
    inner: Arc<GoogleSafetySetting>,
}
impl PyGoogleSafetySetting {
    fn s(&self) -> &GoogleSafetySetting {
        &self.inner
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
    inner: Arc<GoogleTool>,
}
impl PyGoogleTool {
    fn t(&self) -> &GoogleTool {
        &self.inner
    }
}
#[pymethods]
impl PyGoogleTool {
    #[getter]
    fn function_declarations(&self) -> Vec<PyGoogleFunctionDeclaration> {
        self.t()
            .function_declarations
            .as_ref()
            .map(|ds| {
                ds.iter()
                    .map(|d| PyGoogleFunctionDeclaration {
                        inner: Arc::new(d.clone()),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    fn __repr__(&self) -> String {
        "GoogleTool".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "GoogleFunctionDeclaration")]
pub struct PyGoogleFunctionDeclaration {
    inner: Arc<GoogleFunctionDeclaration>,
}
impl PyGoogleFunctionDeclaration {
    fn f(&self) -> &GoogleFunctionDeclaration {
        &self.inner
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
    inner: Arc<GoogleToolConfig>,
}
impl PyGoogleToolConfig {
    #[allow(dead_code)]
    fn c(&self) -> &GoogleToolConfig {
        &self.inner
    }
}
#[pymethods]
impl PyGoogleToolConfig {
    #[getter]
    fn function_calling_config(&self) -> PyGoogleFunctionCallingConfig {
        PyGoogleFunctionCallingConfig {
            inner: Arc::new(self.inner.function_calling_config.clone()),
        }
    }
    fn __repr__(&self) -> String {
        "GoogleToolConfig".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "GoogleFunctionCallingConfig")]
pub struct PyGoogleFunctionCallingConfig {
    inner: Arc<GoogleFunctionCallingConfig>,
}
impl PyGoogleFunctionCallingConfig {
    fn c(&self) -> &GoogleFunctionCallingConfig {
        &self.inner
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
        google_response(&self.inner)
            .candidates
            .iter()
            .map(|c| PyGoogleCandidate {
                inner: Arc::new(c.clone()),
            })
            .collect()
    }
    #[getter]
    fn usage_metadata(&self) -> Option<PyGoogleUsageMetadata> {
        google_response(&self.inner)
            .usage_metadata
            .as_ref()
            .map(|u| PyGoogleUsageMetadata {
                inner: Arc::new(u.clone()),
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
        google_response(&self.inner)
            .candidates
            .iter()
            .map(|c| PyGoogleCandidate {
                inner: Arc::new(c.clone()),
            })
            .collect()
    }
    #[getter]
    fn usage_metadata(&self) -> Option<PyGoogleUsageMetadata> {
        google_response(&self.inner)
            .usage_metadata
            .as_ref()
            .map(|u| PyGoogleUsageMetadata {
                inner: Arc::new(u.clone()),
            })
    }
    fn __repr__(&self) -> String {
        "VertexResponse".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "GoogleCandidate")]
pub struct PyGoogleCandidate {
    inner: Arc<GoogleCandidate>,
}
impl PyGoogleCandidate {
    fn c(&self) -> &GoogleCandidate {
        &self.inner
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
        self.c()
            .safety_ratings
            .iter()
            .map(|r| PyGoogleSafetyRating {
                inner: Arc::new(r.clone()),
            })
            .collect()
    }
    fn __repr__(&self) -> String {
        format!("GoogleCandidate(finish_reason={:?})", self.finish_reason())
    }
}

#[pyclass(module = "wyrd.prompt", name = "GoogleUsageMetadata")]
pub struct PyGoogleUsageMetadata {
    inner: Arc<GoogleUsageMetadata>,
}
impl PyGoogleUsageMetadata {
    fn u(&self) -> &GoogleUsageMetadata {
        &self.inner
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
    inner: Arc<GoogleSafetyRating>,
}
impl PyGoogleSafetyRating {
    fn r(&self) -> &GoogleSafetyRating {
        &self.inner
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
        self.req().text.as_ref().map(|t| PyOpenAiResponsesText {
            inner: Arc::new(t.clone()),
        })
    }
    #[getter]
    fn tools(&self) -> Vec<PyOpenAiResponsesTool> {
        self.req()
            .tools
            .as_ref()
            .map(|ts| {
                ts.iter()
                    .map(|t| PyOpenAiResponsesTool {
                        inner: Arc::new(t.clone()),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    #[getter]
    fn tool_choice(&self) -> Option<PyOpenAiResponsesToolChoice> {
        self.req()
            .tool_choice
            .as_ref()
            .map(|c| PyOpenAiResponsesToolChoice {
                inner: Arc::new(c.clone()),
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
        self.s().reasoning.as_ref().map(|r| PyOpenAiReasoning {
            inner: Arc::new(r.clone()),
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
    inner: Arc<OpenAiResponsesText>,
}
impl PyOpenAiResponsesText {
    fn t(&self) -> &OpenAiResponsesText {
        &self.inner
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
    inner: Arc<OpenAiReasoning>,
}
impl PyOpenAiReasoning {
    fn r(&self) -> &OpenAiReasoning {
        &self.inner
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
    inner: Arc<OpenAiResponsesToolChoice>,
}
impl PyOpenAiResponsesToolChoice {
    fn c(&self) -> &OpenAiResponsesToolChoice {
        &self.inner
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
            OpenAiResponsesToolChoice::Hosted(c) => Ok(PyOpenAiResponsesHostedToolChoice {
                inner: Arc::new(c.clone()),
            }),
            _ => Err(wrong_variant("hosted", self.kind()).into()),
        }
    }
    fn as_function_choice(&self) -> CardPyResult<PyOpenAiResponsesFunctionToolChoice> {
        match self.c() {
            OpenAiResponsesToolChoice::Function(c) => Ok(PyOpenAiResponsesFunctionToolChoice {
                inner: Arc::new(c.clone()),
            }),
            _ => Err(wrong_variant("function", self.kind()).into()),
        }
    }
    fn as_allowed(&self) -> CardPyResult<PyOpenAiResponsesAllowedToolsChoice> {
        match self.c() {
            OpenAiResponsesToolChoice::Allowed(c) => Ok(PyOpenAiResponsesAllowedToolsChoice {
                inner: Arc::new(c.clone()),
            }),
            _ => Err(wrong_variant("allowed", self.kind()).into()),
        }
    }
    fn as_mcp(&self) -> CardPyResult<PyOpenAiResponsesMcpToolChoice> {
        match self.c() {
            OpenAiResponsesToolChoice::Mcp(c) => Ok(PyOpenAiResponsesMcpToolChoice {
                inner: Arc::new(c.clone()),
            }),
            _ => Err(wrong_variant("mcp", self.kind()).into()),
        }
    }
    fn as_custom_choice(&self) -> CardPyResult<PyOpenAiResponsesCustomToolChoice> {
        match self.c() {
            OpenAiResponsesToolChoice::Custom(c) => Ok(PyOpenAiResponsesCustomToolChoice {
                inner: Arc::new(c.clone()),
            }),
            _ => Err(wrong_variant("custom", self.kind()).into()),
        }
    }
    fn as_apply_patch(&self) -> CardPyResult<PyOpenAiResponsesApplyPatchToolChoice> {
        match self.c() {
            OpenAiResponsesToolChoice::ApplyPatch(_) => {
                Ok(PyOpenAiResponsesApplyPatchToolChoice {})
            }
            _ => Err(wrong_variant("apply_patch", self.kind()).into()),
        }
    }
    fn as_shell(&self) -> CardPyResult<PyOpenAiResponsesShellToolChoice> {
        match self.c() {
            OpenAiResponsesToolChoice::Shell(_) => Ok(PyOpenAiResponsesShellToolChoice {}),
            _ => Err(wrong_variant("shell", self.kind()).into()),
        }
    }
    fn __repr__(&self) -> String {
        format!("OpenAiResponsesToolChoice(kind={:?})", self.kind())
    }
}

macro_rules! simple_responses_choice {
    ($name:ident, $py_name:literal, $inner_ty:ident) => {
        #[pyclass(module = "wyrd.prompt", name = $py_name)]
        pub struct $name {
            inner: Arc<$inner_ty>,
        }
        impl $name {
            fn c(&self) -> &$inner_ty {
                &self.inner
            }
        }
    };
}

simple_responses_choice!(
    PyOpenAiResponsesAllowedToolsChoice,
    "OpenAiResponsesAllowedToolsChoice",
    OpenAiResponsesAllowedToolsChoice
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
    OpenAiResponsesHostedToolChoice
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
    OpenAiResponsesFunctionToolChoice
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
    OpenAiResponsesMcpToolChoice
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
    OpenAiResponsesCustomToolChoice
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
pub struct PyOpenAiResponsesApplyPatchToolChoice {}
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
pub struct PyOpenAiResponsesShellToolChoice {}
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
    inner: Arc<OpenAiResponsesTool>,
}
impl PyOpenAiResponsesTool {
    fn t(&self) -> &OpenAiResponsesTool {
        &self.inner
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
    inner: Arc<OpenAiResponsesGrammar>,
}
impl PyOpenAiResponsesGrammar {
    fn g(&self) -> &OpenAiResponsesGrammar {
        &self.inner
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
        self.resp().usage.as_ref().map(|u| PyOpenAiResponsesUsage {
            inner: Arc::new(u.clone()),
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
    inner: Arc<OpenAiResponsesUsage>,
}
impl PyOpenAiResponsesUsage {
    fn u(&self) -> &OpenAiResponsesUsage {
        &self.inner
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
            .map(|d| PyOpenAiResponsesInputTokensDetails {
                inner: Arc::new(d.clone()),
            })
    }
    #[getter]
    fn output_tokens_details(&self) -> Option<PyOpenAiResponsesOutputTokensDetails> {
        self.u()
            .output_tokens_details
            .as_ref()
            .map(|d| PyOpenAiResponsesOutputTokensDetails {
                inner: Arc::new(d.clone()),
            })
    }
    fn __repr__(&self) -> String {
        "OpenAiResponsesUsage".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiResponsesInputTokensDetails")]
pub struct PyOpenAiResponsesInputTokensDetails {
    inner: Arc<OpenAiResponsesInputTokensDetails>,
}
impl PyOpenAiResponsesInputTokensDetails {
    fn d(&self) -> &OpenAiResponsesInputTokensDetails {
        &self.inner
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
    inner: Arc<OpenAiResponsesOutputTokensDetails>,
}
impl PyOpenAiResponsesOutputTokensDetails {
    fn d(&self) -> &OpenAiResponsesOutputTokensDetails {
        &self.inner
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
