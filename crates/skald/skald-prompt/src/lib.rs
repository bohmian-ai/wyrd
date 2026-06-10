//! Client-safe Prompt authoring builder for native Skald requests.
//!
//! This crate owns the Python-facing `Prompt` builder. The stored value is
//! always [`skald_spec::Prompt`]; helpers only make it easier to author the
//! provider-native wire structs.

#![deny(missing_docs)]

/// Provider-shaped Prompt constructors.
pub mod builder;
/// Python and JSON coercion helpers used by builder entry points.
pub mod coerce;
/// Prompt builder errors.
pub mod error;
/// Filesystem loader/dumper for the Prompt builder.
pub mod loader;
/// Media reference helpers for prompt media binding.
pub mod media;
/// Native message and content helper functions.
pub mod messages;
/// Prompt wrapper and rendered request wrapper.
pub mod prompt;
/// Response format helpers for provider-native structured output fields.
pub mod response_format;
/// Provider-native generation settings wrappers.
pub mod settings;
/// Typed Python pyclass mirrors for every in-scope provider wire struct.
#[cfg(feature = "python")]
pub mod wire_py;

pub use builder::{
    AnthropicOptions, GeminiOptions, OpenAiChatOptions, OpenAiResponsesOptions, VertexOptions,
    anthropic, gemini, openai_chat, openai_responses, raw, vertex,
};
pub use error::{PromptBuilderError, PromptBuilderResult};
pub use media::{MAX_MEDIA_FILE_BYTES, PyMediaRef, document_path, image_path};
pub use prompt::{Prompt, PyProviderRequest};
pub use response_format::{ResponseFormat, ResponseFormatKind};
pub use settings::{
    PyAnthropicSettings, PyGoogleGenerateSettings, PyOpenAiChatSettings, PyOpenAiResponsesSettings,
};
#[cfg(feature = "python")]
pub use settings::{apply_model_settings, model_settings_py, prompt_py};

#[cfg(feature = "python")]
use pyo3::{prelude::*, types::PyModule};

/// Register prompt Python classes on a `wyrd.prompt` module.
#[cfg(feature = "python")]
#[allow(clippy::too_many_lines)]
pub fn register_prompt(module: &Bound<'_, PyModule>) -> PyResult<()> {
    wyrd_interfaces::error::register_exceptions(module)?;
    module.add_class::<Prompt>()?;
    module.add_class::<PyProviderRequest>()?;
    module.add_class::<wire_py::PyProviderResponse>()?;
    module.add_class::<PyMediaRef>()?;
    module.add_class::<ResponseFormat>()?;
    module.add_class::<PyOpenAiChatSettings>()?;
    module.add_class::<PyOpenAiResponsesSettings>()?;
    module.add_class::<PyAnthropicSettings>()?;
    module.add_class::<PyGoogleGenerateSettings>()?;
    // OpenAI Chat request pyclasses
    module.add_class::<wire_py::PyOpenAiChatRequest>()?;
    module.add_class::<wire_py::PyOpenAiChatSettings>()?;
    module.add_class::<wire_py::PyOpenAiStop>()?;
    module.add_class::<wire_py::PyOpenAiChatAudio>()?;
    module.add_class::<wire_py::PyOpenAiVoice>()?;
    module.add_class::<wire_py::PyOpenAiPredictionContent>()?;
    module.add_class::<wire_py::PyOpenAiPredictionPayload>()?;
    module.add_class::<wire_py::PyOpenAiPredictionContentPart>()?;
    module.add_class::<wire_py::PyOpenAiStreamOptions>()?;
    module.add_class::<wire_py::PyOpenAiResponseFormat>()?;
    module.add_class::<wire_py::PyOpenAiJsonSchema>()?;
    module.add_class::<wire_py::PyOpenAiTool>()?;
    module.add_class::<wire_py::PyOpenAiFunction>()?;
    module.add_class::<wire_py::PyOpenAiCustomTool>()?;
    module.add_class::<wire_py::PyOpenAiCustomToolFormat>()?;
    module.add_class::<wire_py::PyOpenAiGrammar>()?;
    module.add_class::<wire_py::PyOpenAiChatToolChoice>()?;
    module.add_class::<wire_py::PyOpenAiAllowedToolsChoice>()?;
    module.add_class::<wire_py::PyOpenAiAllowedTools>()?;
    module.add_class::<wire_py::PyOpenAiNamedFunctionToolChoice>()?;
    module.add_class::<wire_py::PyOpenAiFunctionChoice>()?;
    module.add_class::<wire_py::PyOpenAiNamedCustomToolChoice>()?;
    module.add_class::<wire_py::PyOpenAiCustomChoice>()?;
    // OpenAI Chat shared message pyclasses
    module.add_class::<wire_py::PyOpenAiChatMessage>()?;
    module.add_class::<wire_py::PyOpenAiMessageContent>()?;
    module.add_class::<wire_py::PyOpenAiContentPart>()?;
    module.add_class::<wire_py::PyOpenAiImageUrl>()?;
    module.add_class::<wire_py::PyOpenAiInputAudio>()?;
    module.add_class::<wire_py::PyOpenAiFilePart>()?;
    module.add_class::<wire_py::PyOpenAiToolCall>()?;
    module.add_class::<wire_py::PyOpenAiToolFunctionCall>()?;
    module.add_class::<wire_py::PyOpenAiMessageAnnotation>()?;
    module.add_class::<wire_py::PyOpenAiUrlCitation>()?;
    module.add_class::<wire_py::PyOpenAiMessageAudio>()?;
    // OpenAI Chat response pyclasses
    module.add_class::<wire_py::PyOpenAiChatResponse>()?;
    module.add_class::<wire_py::PyOpenAiChatChoice>()?;
    module.add_class::<wire_py::PyOpenAiChatLogprobs>()?;
    module.add_class::<wire_py::PyOpenAiUsage>()?;
    module.add_class::<wire_py::PyOpenAiPromptTokensDetails>()?;
    module.add_class::<wire_py::PyOpenAiCompletionTokensDetails>()?;
    // Anthropic pyclasses
    module.add_class::<wire_py::PyAnthropicMessagesRequest>()?;
    module.add_class::<wire_py::PyAnthropicMessagesSettings>()?;
    module.add_class::<wire_py::PyAnthropicSystem>()?;
    module.add_class::<wire_py::PyAnthropicSystemBlock>()?;
    module.add_class::<wire_py::PyAnthropicCacheControl>()?;
    module.add_class::<wire_py::PyAnthropicThinkingConfig>()?;
    module.add_class::<wire_py::PyAnthropicMessage>()?;
    module.add_class::<wire_py::PyAnthropicContentBlock>()?;
    module.add_class::<wire_py::PyAnthropicImageSource>()?;
    module.add_class::<wire_py::PyAnthropicDocumentSource>()?;
    module.add_class::<wire_py::PyAnthropicToolResultContent>()?;
    module.add_class::<wire_py::PyAnthropicTool>()?;
    module.add_class::<wire_py::PyAnthropicOutputConfig>()?;
    module.add_class::<wire_py::PyAnthropicCitationV1>()?;
    module.add_class::<wire_py::PyAnthropicMessagesResponse>()?;
    module.add_class::<wire_py::PyAnthropicUsage>()?;
    // Google Gemini pyclasses
    module.add_class::<wire_py::PyGeminiRequest>()?;
    module.add_class::<wire_py::PyGoogleContent>()?;
    module.add_class::<wire_py::PyGooglePart>()?;
    module.add_class::<wire_py::PyGoogleInlineData>()?;
    module.add_class::<wire_py::PyGoogleFileData>()?;
    module.add_class::<wire_py::PyGoogleFunctionCall>()?;
    module.add_class::<wire_py::PyGoogleFunctionResponse>()?;
    module.add_class::<wire_py::PyGoogleExecutableCode>()?;
    module.add_class::<wire_py::PyGoogleCodeExecutionResult>()?;
    module.add_class::<wire_py::PyGoogleGenerationConfig>()?;
    module.add_class::<wire_py::PyGoogleThinkingConfig>()?;
    module.add_class::<wire_py::PyGoogleSafetySetting>()?;
    module.add_class::<wire_py::PyGoogleTool>()?;
    module.add_class::<wire_py::PyGoogleFunctionDeclaration>()?;
    module.add_class::<wire_py::PyGoogleToolConfig>()?;
    module.add_class::<wire_py::PyGoogleFunctionCallingConfig>()?;
    module.add_class::<wire_py::PyGeminiResponse>()?;
    module.add_class::<wire_py::PyGoogleCandidate>()?;
    module.add_class::<wire_py::PyGoogleUsageMetadata>()?;
    module.add_class::<wire_py::PyGoogleSafetyRating>()?;
    // Vertex pyclasses
    module.add_class::<wire_py::PyVertexRequest>()?;
    module.add_class::<wire_py::PyVertexResponse>()?;
    // OpenAI Responses pyclasses
    module.add_class::<wire_py::PyOpenAiResponsesRequest>()?;
    module.add_class::<wire_py::PyOpenAiResponsesSettings>()?;
    module.add_class::<wire_py::PyOpenAiResponsesText>()?;
    module.add_class::<wire_py::PyOpenAiReasoning>()?;
    module.add_class::<wire_py::PyOpenAiResponsesToolChoice>()?;
    module.add_class::<wire_py::PyOpenAiResponsesAllowedToolsChoice>()?;
    module.add_class::<wire_py::PyOpenAiResponsesHostedToolChoice>()?;
    module.add_class::<wire_py::PyOpenAiResponsesFunctionToolChoice>()?;
    module.add_class::<wire_py::PyOpenAiResponsesMcpToolChoice>()?;
    module.add_class::<wire_py::PyOpenAiResponsesCustomToolChoice>()?;
    module.add_class::<wire_py::PyOpenAiResponsesApplyPatchToolChoice>()?;
    module.add_class::<wire_py::PyOpenAiResponsesShellToolChoice>()?;
    module.add_class::<wire_py::PyOpenAiResponseItem>()?;
    module.add_class::<wire_py::PyOpenAiResponseContentPart>()?;
    module.add_class::<wire_py::PyOpenAiResponsesTool>()?;
    module.add_class::<wire_py::PyOpenAiResponsesGrammar>()?;
    module.add_class::<wire_py::PyOpenAiResponsesResponse>()?;
    module.add_class::<wire_py::PyOpenAiResponsesUsage>()?;
    module.add_class::<wire_py::PyOpenAiResponsesInputTokensDetails>()?;
    module.add_class::<wire_py::PyOpenAiResponsesOutputTokensDetails>()?;
    Ok(())
}
