//! Google Gemini / Vertex Generate-Content Python wrappers. Gemini and
//! Vertex share every nested type; the `google_request` and
//! `google_response` helpers projet from either provider variant. The
//! `.expect("guarded")` calls and `unreachable!()` arms assert that an
//! upstream constructor has already proven the variant.

use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyModule;
use skald_spec::wire::google_generate::{
    GoogleCandidate, GoogleCodeExecutionResult, GoogleContent, GoogleExecutableCode,
    GoogleFileData, GoogleFunctionCall, GoogleFunctionCallingConfig, GoogleFunctionDeclaration,
    GoogleFunctionResponse, GoogleGenerateContentRequest, GoogleGenerateContentResponse,
    GoogleGenerationConfig, GoogleInlineData, GooglePart, GoogleSafetyRating, GoogleSafetySetting,
    GoogleThinkingConfig, GoogleTool, GoogleToolConfig, GoogleUsageMetadata,
};
use skald_spec::{ProviderRequest, ProviderResponse};
use wyrd_utils::py::WyrdPyResult;

use crate::prompt::wrong_variant;

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
    fn as_text(&self) -> WyrdPyResult<String> {
        match self.p() {
            GooglePart::Text { text } => Ok(text.clone()),
            _ => Err(wrong_variant("text", self.kind()).into()),
        }
    }
    fn as_inline_data(&self) -> WyrdPyResult<PyGoogleInlineData> {
        match self.p() {
            GooglePart::InlineData { .. } => Ok(PyGoogleInlineData {
                inner: Arc::clone(&self.inner),
                content_index: self.content_index,
                part_index: self.part_index,
            }),
            _ => Err(wrong_variant("inline_data", self.kind()).into()),
        }
    }
    fn as_file_data(&self) -> WyrdPyResult<PyGoogleFileData> {
        match self.p() {
            GooglePart::FileData { .. } => Ok(PyGoogleFileData {
                inner: Arc::clone(&self.inner),
                content_index: self.content_index,
                part_index: self.part_index,
            }),
            _ => Err(wrong_variant("file_data", self.kind()).into()),
        }
    }
    fn as_function_call(&self) -> WyrdPyResult<PyGoogleFunctionCall> {
        match self.p() {
            GooglePart::FunctionCall { .. } => Ok(PyGoogleFunctionCall {
                inner: Arc::clone(&self.inner),
                content_index: self.content_index,
                part_index: self.part_index,
            }),
            _ => Err(wrong_variant("function_call", self.kind()).into()),
        }
    }
    fn as_function_response(&self) -> WyrdPyResult<PyGoogleFunctionResponse> {
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
    fn args(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
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
    fn response(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
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
    fn parameters(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
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
    #[allow(dead_code)]
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

pub(super) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyGeminiRequest>()?;
    module.add_class::<PyGoogleContent>()?;
    module.add_class::<PyGooglePart>()?;
    module.add_class::<PyGoogleInlineData>()?;
    module.add_class::<PyGoogleFileData>()?;
    module.add_class::<PyGoogleFunctionCall>()?;
    module.add_class::<PyGoogleFunctionResponse>()?;
    module.add_class::<PyGoogleExecutableCode>()?;
    module.add_class::<PyGoogleCodeExecutionResult>()?;
    module.add_class::<PyGoogleGenerationConfig>()?;
    module.add_class::<PyGoogleThinkingConfig>()?;
    module.add_class::<PyGoogleSafetySetting>()?;
    module.add_class::<PyGoogleTool>()?;
    module.add_class::<PyGoogleFunctionDeclaration>()?;
    module.add_class::<PyGoogleToolConfig>()?;
    module.add_class::<PyGoogleFunctionCallingConfig>()?;
    module.add_class::<PyGeminiResponse>()?;
    module.add_class::<PyGoogleCandidate>()?;
    module.add_class::<PyGoogleUsageMetadata>()?;
    module.add_class::<PyGoogleSafetyRating>()?;
    module.add_class::<PyVertexRequest>()?;
    module.add_class::<PyVertexResponse>()?;
    Ok(())
}
