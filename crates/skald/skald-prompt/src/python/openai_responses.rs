//! `OpenAI` Responses API Python wrappers (request + response sides). Uses the
//! `simple_responses_choice!` macro to generate the small tool-choice
//! wrappers — definition stays at the top so it precedes its callers.
//!
//! The `.expect("guarded")` calls and `unreachable!()` arms below all assert
//! invariants established by the constructors (variant + Some-ness of the
//! target field).

use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyModule;
use skald_spec::wire::openai_responses::{
    OpenAiReasoning, OpenAiReasoningSummary, OpenAiResponseContentPart, OpenAiResponseItem,
    OpenAiResponsesAllowedToolsChoice, OpenAiResponsesAllowedToolsKind,
    OpenAiResponsesAllowedToolsMode, OpenAiResponsesCustomToolChoice,
    OpenAiResponsesCustomToolFormat, OpenAiResponsesFunctionToolChoice,
    OpenAiResponsesFunctionToolKind, OpenAiResponsesGrammar, OpenAiResponsesGrammarSyntax,
    OpenAiResponsesHostedToolChoice, OpenAiResponsesHostedToolKind,
    OpenAiResponsesInputTokensDetails, OpenAiResponsesMcpToolChoice, OpenAiResponsesMcpToolKind,
    OpenAiResponsesOutputTokensDetails, OpenAiResponsesRequest, OpenAiResponsesSettings,
    OpenAiResponsesText, OpenAiResponsesTool, OpenAiResponsesToolChoice,
    OpenAiResponsesToolChoiceMode, OpenAiResponsesUsage, OpenAiTextResponseFormat,
};
use skald_spec::{ProviderRequest, ProviderResponse};
use wyrd_interfaces::error::CardPyResult;

use crate::prompt::wrong_variant;

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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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

pub(super) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyOpenAiResponsesRequest>()?;
    module.add_class::<PyOpenAiResponsesSettings>()?;
    module.add_class::<PyOpenAiResponsesText>()?;
    module.add_class::<PyOpenAiReasoning>()?;
    module.add_class::<PyOpenAiResponsesToolChoice>()?;
    module.add_class::<PyOpenAiResponsesAllowedToolsChoice>()?;
    module.add_class::<PyOpenAiResponsesHostedToolChoice>()?;
    module.add_class::<PyOpenAiResponsesFunctionToolChoice>()?;
    module.add_class::<PyOpenAiResponsesMcpToolChoice>()?;
    module.add_class::<PyOpenAiResponsesCustomToolChoice>()?;
    module.add_class::<PyOpenAiResponsesApplyPatchToolChoice>()?;
    module.add_class::<PyOpenAiResponsesShellToolChoice>()?;
    module.add_class::<PyOpenAiResponseItem>()?;
    module.add_class::<PyOpenAiResponseContentPart>()?;
    module.add_class::<PyOpenAiResponsesTool>()?;
    module.add_class::<PyOpenAiResponsesGrammar>()?;
    module.add_class::<PyOpenAiResponsesResponse>()?;
    module.add_class::<PyOpenAiResponsesUsage>()?;
    module.add_class::<PyOpenAiResponsesInputTokensDetails>()?;
    module.add_class::<PyOpenAiResponsesOutputTokensDetails>()?;
    Ok(())
}
