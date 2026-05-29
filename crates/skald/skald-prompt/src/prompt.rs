//! Prompt wrapper and rendered request wrapper.

use skald_spec::ProviderRequest;
use skald_spec::wire::anthropic_messages::{
    AnthropicMessage, AnthropicSystem, AnthropicSystemBlock,
};
use skald_spec::wire::google_generate::{GoogleContent, GoogleFunctionResponse, GooglePart};
use skald_spec::wire::openai_chat::{OpenAiChatMessage, OpenAiMessageContent};
use skald_spec::wire::openai_responses::{OpenAiResponseContentPart, OpenAiResponseItem};

use crate::error::{PromptBuilderError, PromptBuilderResult};
use crate::messages::{anthropic_text_block, anthropic_tool_result_block, google_text_part};

/// Ergonomic builder wrapper around a native `skald_spec::Prompt`.
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(module = "wyrd.prompt", name = "Prompt", skip_from_py_object)
)]
#[derive(Debug, Clone, PartialEq)]
pub struct Prompt {
    pub(crate) inner: skald_spec::Prompt,
}

/// Opaque Python wrapper around a rendered native provider request.
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(module = "wyrd.prompt", name = "ProviderRequest", skip_from_py_object)
)]
#[derive(Debug, Clone, PartialEq)]
pub struct PyProviderRequest {
    inner: ProviderRequest,
}

impl Prompt {
    /// Borrow the wrapped native prompt.
    pub const fn native(&self) -> &skald_spec::Prompt {
        &self.inner
    }

    /// Consume the wrapper and return the native prompt.
    pub fn into_native(self) -> skald_spec::Prompt {
        self.inner
    }

    /// Wrap a native prompt.
    pub const fn from_native(inner: skald_spec::Prompt) -> Self {
        Self { inner }
    }

    /// Render declared prompt variables into a native provider request.
    pub fn render_native(&self, vars: &[(&str, &str)]) -> PromptBuilderResult<ProviderRequest> {
        Ok(self.inner.render(vars)?)
    }

    /// Return a copy with a native system message applied.
    pub fn with_system(&self, text: impl Into<String>) -> PromptBuilderResult<Self> {
        let mut prompt = self.clone();
        let text = text.into();
        match &mut prompt.inner.request {
            ProviderRequest::OpenAiChatCompletion(request) => {
                request.messages.insert(0, openai_message("system", text));
            }
            ProviderRequest::OpenAiResponses(request) => {
                request.instructions = Some(text);
            }
            ProviderRequest::AnthropicMessage(request) => {
                request.system = Some(AnthropicSystem::Blocks(vec![AnthropicSystemBlock::Text {
                    text,
                    cache_control: None,
                }]));
            }
            ProviderRequest::GeminiGenerateContent(request) => {
                request.system_instruction = Some(GoogleContent {
                    role: "user".to_owned(),
                    parts: vec![google_text_part(text)],
                });
            }
            ProviderRequest::Vertex(request) => {
                request.0.system_instruction = Some(GoogleContent {
                    role: "user".to_owned(),
                    parts: vec![google_text_part(text)],
                });
            }
            other => return unsupported_role(other, "system"),
        }
        Ok(prompt)
    }

    /// Return a copy with a native user message appended.
    pub fn with_user(&self, text: impl Into<String>) -> PromptBuilderResult<Self> {
        self.with_role_text("user", text)
    }

    /// Return a copy with a native assistant/model message appended.
    pub fn with_assistant(&self, text: impl Into<String>) -> PromptBuilderResult<Self> {
        self.with_role_text("assistant", text)
    }

    /// Return a copy with a native tool-result message appended.
    pub fn with_tool_result(
        &self,
        tool_use_id: impl Into<String>,
        content: impl Into<String>,
        is_error: bool,
    ) -> PromptBuilderResult<Self> {
        let mut prompt = self.clone();
        let tool_use_id = tool_use_id.into();
        let content = content.into();
        match &mut prompt.inner.request {
            ProviderRequest::OpenAiChatCompletion(request) => {
                let mut message = openai_message("tool", content);
                message.tool_call_id = Some(tool_use_id);
                request.messages.push(message);
            }
            ProviderRequest::OpenAiResponses(request) => {
                request.input.push(OpenAiResponseItem::FunctionCallOutput {
                    call_id: tool_use_id,
                    output: content,
                });
            }
            ProviderRequest::AnthropicMessage(request) => {
                request.messages.push(AnthropicMessage {
                    role: "user".to_owned(),
                    content: vec![anthropic_tool_result_block(tool_use_id, content, is_error)],
                });
            }
            ProviderRequest::GeminiGenerateContent(request) => {
                request
                    .contents
                    .push(google_tool_result(tool_use_id, &content));
            }
            ProviderRequest::Vertex(request) => {
                request
                    .0
                    .contents
                    .push(google_tool_result(tool_use_id, &content));
            }
            other => return unsupported_role(other, "tool_result"),
        }
        Ok(prompt)
    }

    fn with_role_text(&self, role: &str, text: impl Into<String>) -> PromptBuilderResult<Self> {
        let mut prompt = self.clone();
        let text = text.into();
        match &mut prompt.inner.request {
            ProviderRequest::OpenAiChatCompletion(request) => {
                request.messages.push(openai_message(role, text));
            }
            ProviderRequest::OpenAiResponses(request) => {
                request.input.push(OpenAiResponseItem::Message {
                    role: role.to_owned(),
                    content: vec![if role == "assistant" {
                        OpenAiResponseContentPart::OutputText { text }
                    } else {
                        OpenAiResponseContentPart::InputText { text }
                    }],
                });
            }
            ProviderRequest::AnthropicMessage(request) => {
                request.messages.push(AnthropicMessage {
                    role: if role == "assistant" {
                        "assistant".to_owned()
                    } else {
                        "user".to_owned()
                    },
                    content: vec![anthropic_text_block(text)],
                });
            }
            ProviderRequest::GeminiGenerateContent(request) => {
                request.contents.push(google_content(role, text));
            }
            ProviderRequest::Vertex(request) => {
                request.0.contents.push(google_content(role, text));
            }
            other => return unsupported_role(other, role),
        }
        Ok(prompt)
    }
}

impl PyProviderRequest {
    /// Wrap a native provider request for Python inspection.
    pub const fn from_native(inner: ProviderRequest) -> Self {
        Self { inner }
    }

    /// Borrow the wrapped provider request.
    pub const fn native(&self) -> &ProviderRequest {
        &self.inner
    }

    /// Consume the wrapper and return the native provider request.
    pub fn into_native(self) -> ProviderRequest {
        self.inner
    }
}

fn openai_message(role: &str, text: String) -> OpenAiChatMessage {
    OpenAiChatMessage {
        role: role.to_owned(),
        content: Some(OpenAiMessageContent::Text(text)),
        name: None,
        tool_calls: None,
        tool_call_id: None,
        refusal: None,
    }
}

fn google_content(role: &str, text: String) -> GoogleContent {
    GoogleContent {
        role: if role == "assistant" {
            "model".to_owned()
        } else {
            "user".to_owned()
        },
        parts: vec![google_text_part(text)],
    }
}

fn google_tool_result(name: String, content: &str) -> GoogleContent {
    GoogleContent {
        role: "function".to_owned(),
        parts: vec![GooglePart::FunctionResponse {
            function_response: GoogleFunctionResponse {
                name,
                response: serde_json::json!({ "content": content }),
            },
        }],
    }
}

fn unsupported_role(request: &ProviderRequest, role: &str) -> PromptBuilderResult<Prompt> {
    Err(PromptBuilderError::UnsupportedRole {
        provider: provider_name_to_string(&request.provider()),
        role: role.to_owned(),
    })
}

fn provider_name_to_string(provider: &skald_spec::ProviderName) -> String {
    match provider {
        skald_spec::ProviderName::OpenAi => "openai".to_owned(),
        skald_spec::ProviderName::Anthropic => "anthropic".to_owned(),
        skald_spec::ProviderName::Google => "google".to_owned(),
        skald_spec::ProviderName::Vertex => "vertex".to_owned(),
        skald_spec::ProviderName::Custom(value) => value.clone(),
    }
}

#[cfg(feature = "python")]
use {
    crate::builder::{AnthropicOptions, GeminiOptions, OpenAiChatOptions, OpenAiResponsesOptions},
    crate::coerce::{provider_name_from_py, strings_from_py},
    crate::response_format::ResponseFormat,
    pyo3::{
        prelude::*,
        types::{PyAny, PyBytes, PyDict},
    },
    serde::de::DeserializeOwned,
    wyrd_interfaces::error::CardPyResult,
};

#[cfg(feature = "python")]
#[pyo3::pymethods]
impl Prompt {
    /// Build an `OpenAI` Chat prompt from native constructor arguments.
    #[staticmethod]
    #[pyo3(signature = (model, *, system=None, messages=None, response_format=None, temperature=None, top_p=None, max_tokens=None, variables=None, version=None))]
    #[allow(clippy::too_many_arguments)]
    pub fn openai_chat(
        model: String,
        system: Option<String>,
        messages: Option<&Bound<'_, PyAny>>,
        response_format: Option<&Bound<'_, PyAny>>,
        temperature: Option<f32>,
        top_p: Option<f32>,
        max_tokens: Option<u32>,
        variables: Option<Vec<String>>,
        version: Option<String>,
    ) -> CardPyResult<Self> {
        Ok(crate::builder::openai_chat(
            model,
            OpenAiChatOptions {
                system,
                messages: strings_from_py(messages)?,
                response_format: response_format_from_py(response_format)?,
                temperature,
                top_p,
                max_tokens,
                variables: variables.unwrap_or_default(),
                version,
                ..OpenAiChatOptions::default()
            },
        )?)
    }

    /// Build an `OpenAI` Responses prompt from native constructor arguments.
    #[staticmethod]
    #[pyo3(signature = (model, *, instructions=None, messages=None, response_format=None, temperature=None, top_p=None, max_output_tokens=None, variables=None, version=None))]
    #[allow(clippy::too_many_arguments)]
    pub fn openai_responses(
        model: String,
        instructions: Option<String>,
        messages: Option<&Bound<'_, PyAny>>,
        response_format: Option<&Bound<'_, PyAny>>,
        temperature: Option<f32>,
        top_p: Option<f32>,
        max_output_tokens: Option<u32>,
        variables: Option<Vec<String>>,
        version: Option<String>,
    ) -> CardPyResult<Self> {
        Ok(crate::builder::openai_responses(
            model,
            OpenAiResponsesOptions {
                instructions,
                messages: strings_from_py(messages)?,
                response_format: response_format_from_py(response_format)?,
                temperature,
                top_p,
                max_output_tokens,
                variables: variables.unwrap_or_default(),
                version,
                ..OpenAiResponsesOptions::default()
            },
        )?)
    }

    /// Build an Anthropic Messages prompt from native constructor arguments.
    #[staticmethod]
    #[pyo3(signature = (model, *, system=None, messages=None, max_tokens=1024, response_format=None, temperature=None, top_p=None, top_k=None, variables=None, version=None))]
    #[allow(clippy::too_many_arguments)]
    pub fn anthropic(
        model: String,
        system: Option<String>,
        messages: Option<&Bound<'_, PyAny>>,
        max_tokens: u32,
        response_format: Option<&Bound<'_, PyAny>>,
        temperature: Option<f32>,
        top_p: Option<f32>,
        top_k: Option<u32>,
        variables: Option<Vec<String>>,
        version: Option<String>,
    ) -> CardPyResult<Self> {
        Ok(crate::builder::anthropic(
            model,
            AnthropicOptions {
                system,
                messages: strings_from_py(messages)?,
                max_tokens,
                response_format: response_format_from_py(response_format)?,
                temperature,
                top_p,
                top_k,
                variables: variables.unwrap_or_default(),
                version,
                ..AnthropicOptions::default()
            },
        )?)
    }

    /// Build a Google Gemini `GenerateContent` prompt from native constructor arguments.
    #[staticmethod]
    #[pyo3(signature = (model, *, system=None, messages=None, response_format=None, temperature=None, top_p=None, top_k=None, max_output_tokens=None, variables=None, version=None))]
    #[allow(clippy::too_many_arguments)]
    pub fn gemini(
        model: String,
        system: Option<String>,
        messages: Option<&Bound<'_, PyAny>>,
        response_format: Option<&Bound<'_, PyAny>>,
        temperature: Option<f32>,
        top_p: Option<f32>,
        top_k: Option<u32>,
        max_output_tokens: Option<u32>,
        variables: Option<Vec<String>>,
        version: Option<String>,
    ) -> CardPyResult<Self> {
        let options = GeminiOptions {
            system,
            messages: strings_from_py(messages)?,
            response_format: response_format_from_py(response_format)?,
            temperature,
            top_p,
            top_k,
            max_output_tokens,
            variables: variables.unwrap_or_default(),
            version,
        };
        Ok(crate::builder::gemini(model, options)?)
    }

    /// Build a Vertex `GenerateContent` prompt from native constructor arguments.
    #[staticmethod]
    #[pyo3(signature = (model, *, system=None, messages=None, response_format=None, temperature=None, top_p=None, top_k=None, max_output_tokens=None, variables=None, version=None))]
    #[allow(clippy::too_many_arguments)]
    pub fn vertex(
        model: String,
        system: Option<String>,
        messages: Option<&Bound<'_, PyAny>>,
        response_format: Option<&Bound<'_, PyAny>>,
        temperature: Option<f32>,
        top_p: Option<f32>,
        top_k: Option<u32>,
        max_output_tokens: Option<u32>,
        variables: Option<Vec<String>>,
        version: Option<String>,
    ) -> CardPyResult<Self> {
        let options = GeminiOptions {
            system,
            messages: strings_from_py(messages)?,
            response_format: response_format_from_py(response_format)?,
            temperature,
            top_p,
            top_k,
            max_output_tokens,
            variables: variables.unwrap_or_default(),
            version,
        };
        Ok(crate::builder::vertex(model, options)?)
    }

    /// Build a raw JSON passthrough prompt with a required provider target.
    #[staticmethod]
    pub fn raw(
        provider: &Bound<'_, PyAny>,
        model: String,
        body: &Bound<'_, PyBytes>,
    ) -> CardPyResult<Self> {
        Ok(crate::builder::raw(
            provider_name_from_py(provider)?,
            model,
            body.as_bytes(),
        )?)
    }

    /// Return a copy with a system message applied.
    pub fn system(&self, text: String) -> CardPyResult<Self> {
        Ok(self.with_system(text)?)
    }

    /// Return a copy with a user message appended.
    pub fn user(&self, content: &Bound<'_, PyAny>) -> CardPyResult<Self> {
        append_py_content(self, "user", content)
    }

    /// Return a copy with an assistant message appended.
    pub fn assistant(&self, content: &Bound<'_, PyAny>) -> CardPyResult<Self> {
        append_py_content(self, "assistant", content)
    }

    /// Return a copy with a tool result appended.
    #[pyo3(signature = (tool_use_id, content, is_error=false))]
    pub fn tool_result(
        &self,
        tool_use_id: String,
        content: String,
        is_error: bool,
    ) -> CardPyResult<Self> {
        Ok(self.with_tool_result(tool_use_id, content, is_error)?)
    }

    /// Return an `OpenAI` Chat image URL content part as a native JSON shape.
    #[staticmethod]
    #[pyo3(signature = (url, detail=None))]
    pub fn openai_image_url(
        py: Python<'_>,
        url: String,
        detail: Option<String>,
    ) -> CardPyResult<Py<PyAny>> {
        py_value(py, &crate::messages::openai_image_url_part(url, detail))
    }

    /// Return an `OpenAI` Chat audio content part as a native JSON shape.
    #[staticmethod]
    pub fn openai_audio(py: Python<'_>, data: String, format: String) -> CardPyResult<Py<PyAny>> {
        py_value(py, &crate::messages::openai_audio_part(data, format))
    }

    /// Return an `OpenAI` Chat file-id content part as a native JSON shape.
    #[staticmethod]
    pub fn openai_file_id(py: Python<'_>, file_id: String) -> CardPyResult<Py<PyAny>> {
        py_value(py, &crate::messages::openai_file_id_part(file_id))
    }

    /// Return an `OpenAI` Chat inline file content part as a native JSON shape.
    #[staticmethod]
    #[pyo3(signature = (file_data, filename=None))]
    pub fn openai_file_data(
        py: Python<'_>,
        file_data: String,
        filename: Option<String>,
    ) -> CardPyResult<Py<PyAny>> {
        py_value(
            py,
            &crate::messages::openai_file_data_part(file_data, filename),
        )
    }

    /// Return an Anthropic image URL content block as a native JSON shape.
    #[staticmethod]
    pub fn anthropic_image_url(py: Python<'_>, url: String) -> CardPyResult<Py<PyAny>> {
        py_value(py, &crate::messages::anthropic_image_url_block(url))
    }

    /// Return an Anthropic base64 image content block as a native JSON shape.
    #[staticmethod]
    pub fn anthropic_image_base64(
        py: Python<'_>,
        media_type: String,
        data: String,
    ) -> CardPyResult<Py<PyAny>> {
        py_value(
            py,
            &crate::messages::anthropic_image_base64_block(media_type, data),
        )
    }

    /// Return an Anthropic image file-id content block as a native JSON shape.
    #[staticmethod]
    pub fn anthropic_image_file_id(py: Python<'_>, file_id: String) -> CardPyResult<Py<PyAny>> {
        py_value(py, &crate::messages::anthropic_image_file_id_block(file_id))
    }

    /// Return an Anthropic text document content block as a native JSON shape.
    #[staticmethod]
    #[pyo3(signature = (media_type, data, title=None))]
    pub fn anthropic_document_text(
        py: Python<'_>,
        media_type: String,
        data: String,
        title: Option<String>,
    ) -> CardPyResult<Py<PyAny>> {
        py_value(
            py,
            &crate::messages::anthropic_document_text_block(media_type, data, title),
        )
    }

    /// Return an Anthropic document file-id content block as a native JSON shape.
    #[staticmethod]
    #[pyo3(signature = (file_id, title=None))]
    pub fn anthropic_document_file_id(
        py: Python<'_>,
        file_id: String,
        title: Option<String>,
    ) -> CardPyResult<Py<PyAny>> {
        py_value(
            py,
            &crate::messages::anthropic_document_file_id_block(file_id, title),
        )
    }

    /// Return a Google inline-data part as a native JSON shape.
    #[staticmethod]
    pub fn google_inline_data(
        py: Python<'_>,
        mime_type: String,
        data: String,
    ) -> CardPyResult<Py<PyAny>> {
        py_value(
            py,
            &crate::messages::google_inline_data_part(mime_type, data),
        )
    }

    /// Return a Google file-data part as a native JSON shape.
    #[staticmethod]
    pub fn google_file_data(
        py: Python<'_>,
        mime_type: String,
        file_uri: String,
    ) -> CardPyResult<Py<PyAny>> {
        py_value(
            py,
            &crate::messages::google_file_data_part(mime_type, file_uri),
        )
    }

    /// Render declared variables and return an opaque provider request.
    #[pyo3(signature = (**kwargs))]
    pub fn render(&self, kwargs: Option<&Bound<'_, PyDict>>) -> CardPyResult<PyProviderRequest> {
        let mut owned = Vec::new();
        if let Some(kwargs) = kwargs {
            for (key, value) in kwargs.iter() {
                owned.push((key.extract::<String>()?, value.str()?.extract::<String>()?));
            }
        }
        let borrowed = owned
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect::<Vec<_>>();
        Ok(PyProviderRequest::from_native(
            self.render_native(&borrowed)?,
        ))
    }

    /// Return the provider name for the current native request variant.
    #[getter]
    pub fn provider(&self) -> String {
        provider_name_to_string(&self.inner.request.provider())
    }

    /// Return the native model string.
    #[getter]
    pub fn model(&self) -> &str {
        &self.inner.model
    }

    /// Return the prompt version when present.
    #[getter]
    pub fn version(&self) -> Option<&str> {
        self.inner.version.as_deref()
    }

    /// Return declared render variables.
    #[getter]
    pub fn variables(&self) -> Vec<String> {
        self.inner.variables.clone()
    }

    /// Serialize the native prompt as JSON.
    pub fn to_json(&self) -> CardPyResult<String> {
        Ok(serde_json::to_string(&self.inner)?)
    }

    /// Deserialize a native prompt from JSON.
    #[staticmethod]
    pub fn from_json(data: &str) -> CardPyResult<Self> {
        Ok(Self::from_native(serde_json::from_str(data)?))
    }

    /// Load a bare prompt spec from JSON or YAML.
    #[staticmethod]
    pub fn load(path: std::path::PathBuf) -> CardPyResult<Self> {
        Ok(crate::loader::load_prompt(path)?)
    }

    /// Dump a bare prompt spec to JSON or YAML.
    pub fn dump(&self, path: std::path::PathBuf) -> CardPyResult<()> {
        Ok(crate::loader::dump_prompt(self, path)?)
    }

    /// Return a concise Python representation.
    pub fn __repr__(&self) -> String {
        format!(
            "Prompt(provider={:?}, model={:?})",
            self.provider(),
            self.inner.model
        )
    }
}

#[cfg(feature = "python")]
#[pyo3::pymethods]
impl PyProviderRequest {
    /// Serialize the native provider request as JSON.
    pub fn to_json(&self) -> CardPyResult<String> {
        Ok(serde_json::to_string(&self.inner)?)
    }

    /// Return the provider name for the rendered request.
    #[getter]
    pub fn provider(&self) -> String {
        provider_name_to_string(&self.inner.provider())
    }

    /// Return a concise Python representation.
    pub fn __repr__(&self) -> String {
        format!("ProviderRequest(provider={:?})", self.provider())
    }
}

#[cfg(feature = "python")]
fn response_format_from_py(
    value: Option<&Bound<'_, PyAny>>,
) -> CardPyResult<Option<ResponseFormat>> {
    let Some(value) = value.filter(|value| !value.is_none()) else {
        return Ok(None);
    };

    if let Ok(format) = value.extract::<PyRef<'_, ResponseFormat>>() {
        return Ok(Some(format.clone()));
    }

    let schema = crate::coerce::schema_from_py(value)?;
    Ok(Some(ResponseFormat::json_schema("response", schema)?))
}

#[cfg(feature = "python")]
fn append_py_content(
    prompt: &Prompt,
    role: &str,
    content: &Bound<'_, PyAny>,
) -> CardPyResult<Prompt> {
    if let Ok(text) = content.extract::<String>() {
        return match role {
            "assistant" => Ok(prompt.with_assistant(text)?),
            _ => Ok(prompt.with_user(text)?),
        };
    }

    let value = wyrd_utils::py::pyobject_to_json(content)?;
    append_native_json_content(prompt, role, value)
}

#[cfg(feature = "python")]
fn append_native_json_content(
    prompt: &Prompt,
    role: &str,
    value: serde_json::Value,
) -> CardPyResult<Prompt> {
    let mut out = prompt.clone();
    match &mut out.inner.request {
        ProviderRequest::OpenAiChatCompletion(request) => {
            let parts = json_parts::<skald_spec::wire::openai_chat::OpenAiContentPart>(value)?;
            request.messages.push(OpenAiChatMessage {
                role: role.to_owned(),
                content: Some(OpenAiMessageContent::Parts(parts)),
                name: None,
                tool_calls: None,
                tool_call_id: None,
                refusal: None,
            });
        }
        ProviderRequest::OpenAiResponses(request) => {
            let parts = json_parts::<OpenAiResponseContentPart>(value)?;
            request.input.push(OpenAiResponseItem::Message {
                role: role.to_owned(),
                content: parts,
            });
        }
        ProviderRequest::AnthropicMessage(request) => {
            let blocks =
                json_parts::<skald_spec::wire::anthropic_messages::AnthropicContentBlock>(value)?;
            request.messages.push(AnthropicMessage {
                role: if role == "assistant" {
                    "assistant".to_owned()
                } else {
                    "user".to_owned()
                },
                content: blocks,
            });
        }
        ProviderRequest::GeminiGenerateContent(request) => {
            request.contents.push(GoogleContent {
                role: if role == "assistant" {
                    "model".to_owned()
                } else {
                    "user".to_owned()
                },
                parts: json_parts::<GooglePart>(value)?,
            });
        }
        ProviderRequest::Vertex(request) => {
            request.0.contents.push(GoogleContent {
                role: if role == "assistant" {
                    "model".to_owned()
                } else {
                    "user".to_owned()
                },
                parts: json_parts::<GooglePart>(value)?,
            });
        }
        other => {
            return Err(PromptBuilderError::UnsupportedRole {
                provider: provider_name_to_string(&other.provider()),
                role: role.to_owned(),
            }
            .into());
        }
    }
    Ok(out)
}

#[cfg(feature = "python")]
fn json_parts<T>(value: serde_json::Value) -> CardPyResult<Vec<T>>
where
    T: DeserializeOwned,
{
    if value.is_array() {
        Ok(serde_json::from_value(value)?)
    } else {
        Ok(vec![serde_json::from_value(value)?])
    }
}

#[cfg(feature = "python")]
fn py_value<T>(py: Python<'_>, value: &T) -> CardPyResult<Py<PyAny>>
where
    T: serde::Serialize,
{
    Ok(wyrd_utils::py::json_to_pyobject(
        py,
        &serde_json::to_value(value)?,
    )?)
}
