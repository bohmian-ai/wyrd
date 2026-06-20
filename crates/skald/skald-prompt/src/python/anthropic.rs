//! Anthropic Messages API Python wrappers (request + response sides).
//!
//! Same shape as the `OpenAI` wrappers: an `Arc<ProviderRequest>` or
//! `Arc<ProviderResponse>` plus a locator into the targeted field.
//! Construction guarantees the variant; `.expect("guarded")` and
//! `unreachable!()` arms below assert that guarantee at projection time.

use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyModule;
use skald_spec::wire::anthropic_citation::AnthropicCitationV1;
use skald_spec::wire::anthropic_messages::{
    AnthropicCacheControl, AnthropicContentBlock, AnthropicDocumentSource, AnthropicImageSource,
    AnthropicMessage, AnthropicMessagesRequest, AnthropicMessagesResponse,
    AnthropicMessagesSettings, AnthropicOutputConfig, AnthropicOutputFormat, AnthropicStopReason,
    AnthropicSystem, AnthropicSystemBlock, AnthropicThinkingConfig, AnthropicTool,
    AnthropicToolResultContent, AnthropicUsage,
};
use skald_spec::{ProviderRequest, ProviderResponse};
use wyrd_interfaces::error::CardPyResult;

use crate::prompt::wrong_variant;

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
    #[allow(dead_code)]
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
                    AnthropicSystem::Text(_) => unreachable!(),
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
            AnthropicToolResultContent::Blocks(_) => Err(wrong_variant("text", self.kind()).into()),
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

pub(super) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyAnthropicMessagesRequest>()?;
    module.add_class::<PyAnthropicMessagesSettings>()?;
    module.add_class::<PyAnthropicSystem>()?;
    module.add_class::<PyAnthropicSystemBlock>()?;
    module.add_class::<PyAnthropicCacheControl>()?;
    module.add_class::<PyAnthropicThinkingConfig>()?;
    module.add_class::<PyAnthropicMessage>()?;
    module.add_class::<PyAnthropicContentBlock>()?;
    module.add_class::<PyAnthropicImageSource>()?;
    module.add_class::<PyAnthropicDocumentSource>()?;
    module.add_class::<PyAnthropicToolResultContent>()?;
    module.add_class::<PyAnthropicTool>()?;
    module.add_class::<PyAnthropicOutputConfig>()?;
    module.add_class::<PyAnthropicCitationV1>()?;
    module.add_class::<PyAnthropicMessagesResponse>()?;
    module.add_class::<PyAnthropicUsage>()?;
    Ok(())
}
