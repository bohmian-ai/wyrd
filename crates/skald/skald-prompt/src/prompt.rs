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
        prompt.inner.normalize_media_placeholders_mut()?;
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
        prompt.inner.normalize_media_placeholders_mut()?;
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
        prompt.inner.normalize_media_placeholders_mut()?;
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
    crate::coerce::{provider_name_from_py, provider_name_from_str, strings_from_py},
    crate::media::PyMediaRef,
    crate::response_format::ResponseFormat,
    crate::settings::{
        PyAnthropicSettings, PyGoogleGenerateSettings, PyOpenAiChatSettings,
        PyOpenAiResponsesSettings, resolve_anthropic_settings, resolve_google_settings,
        resolve_openai_chat_settings, resolve_openai_responses_settings,
    },
    pyo3::{
        IntoPyObjectExt,
        prelude::*,
        types::{PyAny, PyBytes, PyDict, PyList, PyString, PyTuple},
    },
    serde::de::DeserializeOwned,
    wyrd_interfaces::error::CardPyResult,
};

#[cfg(feature = "python")]
#[pyo3::pymethods]
impl Prompt {
    /// Build a vendor-native prompt from provider and message inputs.
    #[new]
    #[pyo3(signature = (messages, model, *, provider, system=None, response_format=None, operation=None, cache=None, model_settings=None, variables=None, version=None))]
    #[allow(clippy::too_many_arguments)]
    pub fn __new__(
        messages: &Bound<'_, PyAny>,
        model: String,
        provider: &Bound<'_, PyAny>,
        system: Option<String>,
        response_format: Option<&Bound<'_, PyAny>>,
        operation: Option<&str>,
        cache: Option<&Bound<'_, PyAny>>,
        model_settings: Option<&Bound<'_, PyAny>>,
        variables: Option<Vec<String>>,
        version: Option<String>,
    ) -> CardPyResult<Self> {
        let provider = provider_name_from_py(provider)?;
        let response_format = response_format_from_py(response_format)?;
        let auto_variables = variables.is_none();
        let variables = variables.unwrap_or_default();
        let mut prompt = match provider {
            skald_spec::ProviderName::OpenAi
                if operation
                    .is_some_and(|operation| operation.eq_ignore_ascii_case("responses")) =>
            {
                crate::builder::openai_responses(
                    model,
                    OpenAiResponsesOptions {
                        instructions: system,
                        response_format,
                        settings: resolve_openai_responses_settings(model_settings)?,
                        variables,
                        version,
                        ..OpenAiResponsesOptions::default()
                    },
                )?
            }
            skald_spec::ProviderName::OpenAi => crate::builder::openai_chat(
                model,
                OpenAiChatOptions {
                    system,
                    response_format,
                    prompt_cache_key: cache_prompt_key(cache)?,
                    settings: resolve_openai_chat_settings(model_settings)?,
                    variables,
                    version,
                    ..OpenAiChatOptions::default()
                },
            )?,
            skald_spec::ProviderName::Anthropic => crate::builder::anthropic(
                model,
                AnthropicOptions {
                    system,
                    response_format,
                    settings: resolve_anthropic_settings(model_settings)?,
                    variables,
                    version,
                    ..AnthropicOptions::default()
                },
            )?,
            skald_spec::ProviderName::Google => {
                let options = GeminiOptions {
                    system,
                    response_format,
                    settings: resolve_google_settings(model_settings)?,
                    variables,
                    version,
                    ..GeminiOptions::default()
                };
                crate::builder::gemini(model, options)?
            }
            skald_spec::ProviderName::Vertex => {
                let options = GeminiOptions {
                    system,
                    response_format,
                    settings: resolve_google_settings(model_settings)?,
                    variables,
                    version,
                    ..GeminiOptions::default()
                };
                crate::builder::vertex(model, options)?
            }
            skald_spec::ProviderName::Custom(value) => {
                return Err(PromptBuilderError::InvalidProvider(value).into());
            }
        };
        prompt = append_py_messages(prompt, messages)?;
        if auto_variables {
            prompt.inner.variables = extract_prompt_variables(&prompt)?;
        }
        Ok(prompt)
    }

    /// Build an `OpenAI` Chat prompt from native constructor arguments.
    #[staticmethod]
    #[pyo3(signature = (model, *, system=None, messages=None, response_format=None, cache=None, model_settings=None, variables=None, version=None))]
    #[allow(clippy::too_many_arguments)]
    pub fn openai_chat(
        model: String,
        system: Option<String>,
        messages: Option<&Bound<'_, PyAny>>,
        response_format: Option<&Bound<'_, PyAny>>,
        cache: Option<&Bound<'_, PyAny>>,
        model_settings: Option<&Bound<'_, PyAny>>,
        variables: Option<Vec<String>>,
        version: Option<String>,
    ) -> CardPyResult<Self> {
        let auto_variables = variables.is_none();
        let prompt = crate::builder::openai_chat(
            model,
            OpenAiChatOptions {
                system,
                messages: strings_from_py(messages)?,
                response_format: response_format_from_py(response_format)?,
                prompt_cache_key: cache_prompt_key(cache)?,
                settings: resolve_openai_chat_settings(model_settings)?,
                variables: variables.unwrap_or_default(),
                version,
            },
        )?;
        auto_assign_variables(prompt, auto_variables)
    }

    /// Build an `OpenAI` Responses prompt from native constructor arguments.
    #[staticmethod]
    #[pyo3(signature = (model, *, instructions=None, messages=None, response_format=None, model_settings=None, variables=None, version=None))]
    #[allow(clippy::too_many_arguments)]
    pub fn openai_responses(
        model: String,
        instructions: Option<String>,
        messages: Option<&Bound<'_, PyAny>>,
        response_format: Option<&Bound<'_, PyAny>>,
        model_settings: Option<&Bound<'_, PyAny>>,
        variables: Option<Vec<String>>,
        version: Option<String>,
    ) -> CardPyResult<Self> {
        let auto_variables = variables.is_none();
        let prompt = crate::builder::openai_responses(
            model,
            OpenAiResponsesOptions {
                instructions,
                messages: strings_from_py(messages)?,
                response_format: response_format_from_py(response_format)?,
                settings: resolve_openai_responses_settings(model_settings)?,
                variables: variables.unwrap_or_default(),
                version,
            },
        )?;
        auto_assign_variables(prompt, auto_variables)
    }

    /// Build an Anthropic Messages prompt from native constructor arguments.
    #[staticmethod]
    #[pyo3(signature = (model, *, system=None, messages=None, response_format=None, model_settings=None, variables=None, version=None))]
    #[allow(clippy::too_many_arguments)]
    pub fn anthropic(
        model: String,
        system: Option<String>,
        messages: Option<&Bound<'_, PyAny>>,
        response_format: Option<&Bound<'_, PyAny>>,
        model_settings: Option<&Bound<'_, PyAny>>,
        variables: Option<Vec<String>>,
        version: Option<String>,
    ) -> CardPyResult<Self> {
        let auto_variables = variables.is_none();
        let prompt = crate::builder::anthropic(
            model,
            AnthropicOptions {
                system,
                messages: strings_from_py(messages)?,
                response_format: response_format_from_py(response_format)?,
                settings: resolve_anthropic_settings(model_settings)?,
                variables: variables.unwrap_or_default(),
                version,
            },
        )?;
        auto_assign_variables(prompt, auto_variables)
    }

    /// Build a Google Gemini `GenerateContent` prompt from native constructor arguments.
    #[staticmethod]
    #[pyo3(signature = (model, *, system=None, messages=None, response_format=None, model_settings=None, variables=None, version=None))]
    #[allow(clippy::too_many_arguments)]
    pub fn gemini(
        model: String,
        system: Option<String>,
        messages: Option<&Bound<'_, PyAny>>,
        response_format: Option<&Bound<'_, PyAny>>,
        model_settings: Option<&Bound<'_, PyAny>>,
        variables: Option<Vec<String>>,
        version: Option<String>,
    ) -> CardPyResult<Self> {
        let auto_variables = variables.is_none();
        let options = GeminiOptions {
            system,
            messages: strings_from_py(messages)?,
            response_format: response_format_from_py(response_format)?,
            settings: resolve_google_settings(model_settings)?,
            variables: variables.unwrap_or_default(),
            version,
        };
        let prompt = crate::builder::gemini(model, options)?;
        auto_assign_variables(prompt, auto_variables)
    }

    /// Build a Vertex `GenerateContent` prompt from native constructor arguments.
    #[staticmethod]
    #[pyo3(signature = (model, *, system=None, messages=None, response_format=None, model_settings=None, variables=None, version=None))]
    #[allow(clippy::too_many_arguments)]
    pub fn vertex(
        model: String,
        system: Option<String>,
        messages: Option<&Bound<'_, PyAny>>,
        response_format: Option<&Bound<'_, PyAny>>,
        model_settings: Option<&Bound<'_, PyAny>>,
        variables: Option<Vec<String>>,
        version: Option<String>,
    ) -> CardPyResult<Self> {
        let auto_variables = variables.is_none();
        let options = GeminiOptions {
            system,
            messages: strings_from_py(messages)?,
            response_format: response_format_from_py(response_format)?,
            settings: resolve_google_settings(model_settings)?,
            variables: variables.unwrap_or_default(),
            version,
        };
        let prompt = crate::builder::vertex(model, options)?;
        auto_assign_variables(prompt, auto_variables)
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

    /// Return a provider-native image URL part for the current prompt provider.
    #[staticmethod]
    #[pyo3(signature = (url, *, detail=None, provider="openai"))]
    pub fn image_url(
        py: Python<'_>,
        url: String,
        detail: Option<String>,
        provider: &str,
    ) -> CardPyResult<Py<PyAny>> {
        match provider_name_from_str(provider) {
            skald_spec::ProviderName::OpenAi => {
                py_value(py, &crate::messages::openai_image_url_part(url, detail))
            }
            skald_spec::ProviderName::Anthropic => {
                py_value(py, &crate::messages::anthropic_image_url_block(url))
            }
            other => Err(PromptBuilderError::UnsupportedRole {
                provider: provider_name_to_string(&other),
                role: "image_url".to_owned(),
            }
            .into()),
        }
    }

    /// Return an Anthropic image base64 block as a native JSON shape.
    #[staticmethod]
    pub fn image_base64(
        py: Python<'_>,
        media_type: String,
        data: String,
    ) -> CardPyResult<Py<PyAny>> {
        py_value(
            py,
            &crate::messages::anthropic_image_base64_block(media_type, data),
        )
    }

    /// Return a Google file URI part as a native JSON shape.
    #[staticmethod]
    pub fn file_uri(
        py: Python<'_>,
        mime_type: String,
        file_uri: String,
    ) -> CardPyResult<Py<PyAny>> {
        py_value(
            py,
            &crate::messages::google_file_data_part(mime_type, file_uri),
        )
    }

    /// Return an `OpenAI` file-id content part as a native JSON shape.
    #[staticmethod]
    pub fn file_id(py: Python<'_>, file_id: String) -> CardPyResult<Py<PyAny>> {
        py_value(py, &crate::messages::openai_file_id_part(file_id))
    }

    /// Return an Anthropic text document block as a native JSON shape.
    #[staticmethod]
    #[pyo3(signature = (media_type, data, title=None))]
    pub fn document_text(
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

    /// Render declared variables and return an opaque provider request.
    #[pyo3(signature = (**kwargs))]
    pub fn render(&self, kwargs: Option<&Bound<'_, PyDict>>) -> CardPyResult<PyProviderRequest> {
        let owned = binding_pairs(None, None, kwargs)?;
        let borrowed = borrowed_pairs(&owned);
        Ok(PyProviderRequest::from_native(
            self.render_native(&borrowed)?,
        ))
    }

    /// Return a copy with one or more prompt variables bound.
    #[pyo3(signature = (name=None, value=None, **kwargs))]
    pub fn bind(
        &self,
        name: Option<&str>,
        value: Option<&Bound<'_, PyAny>>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<Self> {
        let owned = binding_pairs(name, value, kwargs)?;
        require_binding_args(&owned)?;
        let borrowed = borrowed_pairs(&owned);
        Ok(Self::from_native(
            self.inner
                .bind(&borrowed)
                .map_err(PromptBuilderError::from)?,
        ))
    }

    /// Bind one or more prompt variables in place.
    #[pyo3(signature = (name=None, value=None, **kwargs))]
    pub fn bind_mut(
        &mut self,
        name: Option<&str>,
        value: Option<&Bound<'_, PyAny>>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<()> {
        let owned = binding_pairs(name, value, kwargs)?;
        require_binding_args(&owned)?;
        let borrowed = borrowed_pairs(&owned);
        Ok(self
            .inner
            .bind_mut(&borrowed)
            .map_err(PromptBuilderError::from)?)
    }

    /// Return a copy with a media placeholder bound to a provider-native value.
    #[allow(clippy::needless_pass_by_value)]
    pub fn bind_media(&self, name: &str, media: PyRef<'_, PyMediaRef>) -> CardPyResult<Self> {
        Ok(Self::from_native(
            self.inner
                .bind_media(name, media.native())
                .map_err(PromptBuilderError::from)?,
        ))
    }

    /// Bind a media placeholder in place.
    #[allow(clippy::needless_pass_by_value)]
    pub fn bind_media_mut(&mut self, name: &str, media: PyRef<'_, PyMediaRef>) -> CardPyResult<()> {
        Ok(self
            .inner
            .bind_media_mut(name, media.native())
            .map_err(PromptBuilderError::from)?)
    }

    /// Return the provider name for the current native request variant.
    #[getter]
    pub fn provider(&self) -> String {
        provider_name_to_string(&self.inner.request.provider())
    }

    /// Return the native provider request wrapper.
    #[getter]
    pub fn request(&self) -> PyProviderRequest {
        PyProviderRequest::from_native(self.inner.request.clone())
    }

    /// Return typed provider generation settings, or `None` for raw prompts.
    #[getter]
    pub fn model_settings(&self, py: Python<'_>) -> CardPyResult<Option<Py<PyAny>>> {
        Ok(match self.inner.settings_ref() {
            Some(skald_spec::ProviderSettingsRef::OpenAiChat(settings)) => {
                Some(PyOpenAiChatSettings::from_native(settings.clone()).into_py_any(py)?)
            }
            Some(skald_spec::ProviderSettingsRef::OpenAiResponses(settings)) => {
                Some(PyOpenAiResponsesSettings::from_native(settings.clone()).into_py_any(py)?)
            }
            Some(skald_spec::ProviderSettingsRef::Anthropic(settings)) => {
                Some(PyAnthropicSettings::from_native(settings.clone()).into_py_any(py)?)
            }
            Some(skald_spec::ProviderSettingsRef::Google(settings)) => {
                Some(PyGoogleGenerateSettings::from_native(settings.clone()).into_py_any(py)?)
            }
            None => None,
        })
    }

    /// Return native request messages or content turns as Python objects.
    #[getter]
    pub fn messages(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        wyrd_utils::py::json_to_pyobject(py, &request_messages_value(&self.inner.request))
            .map_err(Into::into)
    }

    /// Return the last native request message or content turn.
    #[getter]
    pub fn message(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        let messages = request_messages_value(&self.inner.request);
        let value = messages
            .as_array()
            .and_then(|values| values.last())
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        wyrd_utils::py::json_to_pyobject(py, &value).map_err(Into::into)
    }

    /// Return native system instructions when the provider has that field.
    #[getter]
    pub fn system_messages(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        wyrd_utils::py::json_to_pyobject(py, &request_system_value(&self.inner.request))
            .map_err(Into::into)
    }

    /// Replace the native provider request from a wrapper or JSON-like object.
    #[setter]
    pub fn set_request(&mut self, value: &Bound<'_, PyAny>) -> CardPyResult<()> {
        self.inner.request = provider_request_from_py(value)?;
        if let Some(model) = request_model(&self.inner.request) {
            self.inner.model = model.to_owned();
        }
        Ok(())
    }

    /// Return the native model string.
    #[getter]
    pub fn model(&self) -> &str {
        &self.inner.model
    }

    /// Set the prompt model string and any native request model field.
    #[setter]
    pub fn set_model(&mut self, model: String) -> CardPyResult<()> {
        crate::coerce::checked_model(model.as_str())?;
        self.inner.model.clone_from(&model);
        set_request_model(&mut self.inner.request, model);
        Ok(())
    }

    /// Return the prompt version when present.
    #[getter]
    pub fn version(&self) -> Option<&str> {
        self.inner.version.as_deref()
    }

    /// Set the optional prompt version.
    #[setter]
    pub fn set_version(&mut self, version: Option<String>) {
        self.inner.version = version;
    }

    /// Return declared render variables.
    #[getter]
    pub fn variables(&self) -> Vec<String> {
        self.inner.variables.clone()
    }

    /// Return declared media variables.
    #[getter]
    pub fn media_variables(&self) -> Vec<String> {
        self.inner.media_variables.clone()
    }

    /// Set declared render variables.
    #[setter]
    pub fn set_variables(&mut self, variables: Vec<String>) {
        self.inner.variables = variables;
    }

    /// Return native prompt JSON as a Python dictionary.
    pub fn model_dump(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        py_value(py, &self.inner)
    }

    /// Return this prompt as a JSON string.
    pub fn model_dump_json(&self) -> CardPyResult<String> {
        Ok(serde_json::to_string(&self.inner)?)
    }

    /// Build a prompt from serialized native prompt JSON.
    #[staticmethod]
    pub fn model_validate_json(data: &str) -> CardPyResult<Self> {
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

    /// Return a pretty JSON string for interactive inspection.
    pub fn __str__(&self) -> String {
        wyrd_utils::json::pretty_json_string(&self.inner)
    }
}

#[cfg(feature = "python")]
#[pyo3::pymethods]
impl PyProviderRequest {
    /// Return the native provider request as a Python dictionary.
    pub fn model_dump(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        py_value(py, &self.inner)
    }

    /// Return the native provider request as a JSON string.
    pub fn model_dump_json(&self) -> CardPyResult<String> {
        Ok(serde_json::to_string(&self.inner)?)
    }

    /// Return the provider name for the rendered request.
    #[getter]
    pub fn provider(&self) -> String {
        provider_name_to_string(&self.inner.provider())
    }

    /// Return native request messages or content turns as Python objects.
    #[getter]
    pub fn messages(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        wyrd_utils::py::json_to_pyobject(py, &request_messages_value(&self.inner))
            .map_err(Into::into)
    }

    /// Return the last native request message or content turn.
    #[getter]
    pub fn message(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        let messages = request_messages_value(&self.inner);
        let value = messages
            .as_array()
            .and_then(|values| values.last())
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        wyrd_utils::py::json_to_pyobject(py, &value).map_err(Into::into)
    }

    /// Return native system instructions when the provider has that field.
    #[getter]
    pub fn system(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        wyrd_utils::py::json_to_pyobject(py, &request_system_value(&self.inner)).map_err(Into::into)
    }

    /// Return a concise Python representation.
    pub fn __repr__(&self) -> String {
        format!("ProviderRequest(provider={:?})", self.provider())
    }

    /// Return a pretty JSON string for interactive inspection.
    pub fn __str__(&self) -> String {
        wyrd_utils::json::pretty_json_string(&self.inner)
    }
}

#[cfg(feature = "python")]
fn auto_assign_variables(mut prompt: Prompt, auto_variables: bool) -> CardPyResult<Prompt> {
    if auto_variables {
        prompt.inner.variables = extract_prompt_variables(&prompt)?;
    }
    Ok(prompt)
}

#[cfg(feature = "python")]
fn extract_prompt_variables(prompt: &Prompt) -> CardPyResult<Vec<String>> {
    let spec = wyrd_spec::PromptSpec {
        prompt: prompt.inner.clone(),
    };
    Ok(wyrd_spec::extract_placeholders(&spec)?)
}

#[cfg(feature = "python")]
fn append_py_messages(mut prompt: Prompt, messages: &Bound<'_, PyAny>) -> CardPyResult<Prompt> {
    if messages.is_instance_of::<PyString>() {
        return Ok(prompt.with_user(messages.extract::<String>()?)?);
    }
    if let Ok(list) = messages.cast::<PyList>() {
        for item in list.iter() {
            prompt = append_py_content(&prompt, "user", &item)?;
        }
        return Ok(prompt);
    }
    if let Ok(tuple) = messages.cast::<PyTuple>() {
        for item in tuple.iter() {
            prompt = append_py_content(&prompt, "user", &item)?;
        }
        return Ok(prompt);
    }
    append_py_content(&prompt, "user", messages)
}

#[cfg(feature = "python")]
fn binding_pairs(
    name: Option<&str>,
    value: Option<&Bound<'_, PyAny>>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> CardPyResult<Vec<(String, String)>> {
    let mut owned = Vec::new();
    if let (Some(name), Some(value)) = (name, value) {
        owned.push((name.to_owned(), value.str()?.extract::<String>()?));
    }
    if let Some(kwargs) = kwargs {
        for (key, value) in kwargs.iter() {
            owned.push((key.extract::<String>()?, value.str()?.extract::<String>()?));
        }
    }
    Ok(owned)
}

#[cfg(feature = "python")]
fn borrowed_pairs(owned: &[(String, String)]) -> Vec<(&str, &str)> {
    owned
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect()
}

#[cfg(feature = "python")]
fn require_binding_args(owned: &[(String, String)]) -> CardPyResult<()> {
    if owned.is_empty() {
        return Err(PromptBuilderError::Validation(
            "must provide either (name, value) or keyword arguments for binding".to_owned(),
        )
        .into());
    }
    Ok(())
}

#[cfg(feature = "python")]
fn provider_request_from_py(value: &Bound<'_, PyAny>) -> CardPyResult<ProviderRequest> {
    if let Ok(request) = value.extract::<PyRef<'_, PyProviderRequest>>() {
        return Ok(request.inner.clone());
    }
    Ok(serde_json::from_value(wyrd_utils::py::pyobject_to_json(
        value,
    )?)?)
}

#[cfg(feature = "python")]
fn request_model(request: &ProviderRequest) -> Option<&str> {
    match request {
        ProviderRequest::OpenAiChatCompletion(request) => Some(&request.model),
        ProviderRequest::OpenAiResponses(request) => Some(&request.model),
        ProviderRequest::OpenAiEmbeddings(request) => Some(&request.model),
        ProviderRequest::AnthropicMessage(request) => Some(&request.model),
        ProviderRequest::GoogleBatchEmbed(request) => request
            .requests
            .first()
            .map(|request| request.model.as_str()),
        _ => None,
    }
}

#[cfg(feature = "python")]
fn set_request_model(request: &mut ProviderRequest, model: String) {
    match request {
        ProviderRequest::OpenAiChatCompletion(request) => request.model = model,
        ProviderRequest::OpenAiResponses(request) => request.model = model,
        ProviderRequest::OpenAiEmbeddings(request) => request.model = model,
        ProviderRequest::AnthropicMessage(request) => request.model = model,
        ProviderRequest::GoogleBatchEmbed(request) => {
            for request in &mut request.requests {
                request.model.clone_from(&model);
            }
        }
        _ => {}
    }
}

#[cfg(feature = "python")]
fn request_messages_value(request: &ProviderRequest) -> serde_json::Value {
    match request {
        ProviderRequest::OpenAiChatCompletion(request) => serde_json::to_value(&request.messages),
        ProviderRequest::OpenAiResponses(request) => serde_json::to_value(&request.input),
        ProviderRequest::AnthropicMessage(request) => serde_json::to_value(&request.messages),
        ProviderRequest::GeminiGenerateContent(request) => serde_json::to_value(&request.contents),
        ProviderRequest::Vertex(request) => serde_json::to_value(&request.0.contents),
        _ => Ok(serde_json::Value::Array(Vec::new())),
    }
    .unwrap_or(serde_json::Value::Null)
}

#[cfg(feature = "python")]
fn request_system_value(request: &ProviderRequest) -> serde_json::Value {
    match request {
        ProviderRequest::OpenAiChatCompletion(request) => serde_json::to_value(
            request
                .messages
                .iter()
                .filter(|message| message.role == "system")
                .collect::<Vec<_>>(),
        ),
        ProviderRequest::OpenAiResponses(request) => serde_json::to_value(&request.instructions),
        ProviderRequest::AnthropicMessage(request) => serde_json::to_value(&request.system),
        ProviderRequest::GeminiGenerateContent(request) => {
            serde_json::to_value(&request.system_instruction)
        }
        ProviderRequest::Vertex(request) => serde_json::to_value(&request.0.system_instruction),
        _ => Ok(serde_json::Value::Null),
    }
    .unwrap_or(serde_json::Value::Null)
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
fn cache_prompt_key(value: Option<&Bound<'_, PyAny>>) -> CardPyResult<Option<String>> {
    let Some(value) = value.filter(|value| !value.is_none()) else {
        return Ok(None);
    };
    if let Ok(text) = value.extract::<String>() {
        return Ok(Some(text));
    }
    if let Ok(dict) = value.cast::<PyDict>() {
        if let Ok(Some(item)) = dict.get_item("prompt_cache_key") {
            return Ok(Some(item.extract::<String>()?));
        }
        if let Ok(Some(item)) = dict.get_item("key") {
            return Ok(Some(item.extract::<String>()?));
        }
    }
    Ok(Some(value.str()?.extract::<String>()?))
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
    out.inner
        .normalize_media_placeholders_mut()
        .map_err(PromptBuilderError::from)?;
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
