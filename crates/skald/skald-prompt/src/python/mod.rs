//! Typed Python pyclass mirrors for every in-scope provider wire struct.
//!
//! One clone per boundary crossing: a callback or observer receives a
//! `PyProviderRequest` / `PyProviderResponse` holding the request or response
//! behind an `Arc`. Every nested field access from there is zero-copy — each
//! wrapper carries the same `Arc` plus an index (or other locator) into the
//! variant's field. Variant projection is asserted with `.expect("guarded")`
//! and `unreachable!()` arms, both naming the construction-time invariant
//! that makes the projection safe.
//!
//! The split below is by provider, with OpenAI Chat further split into its
//! request, message (shared between request and response), and response
//! sub-files. Each sub-module owns a `register` helper that adds its
//! `#[pyclass]` types to the parent Python module.

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

use pyo3::prelude::*;
use pyo3::types::PyModule;

pub mod anthropic;
pub mod google;
pub mod openai_chat_messages;
pub mod openai_chat_request;
pub mod openai_chat_response;
pub mod openai_responses;
pub mod response;
mod shared;

pub use anthropic::{
    PyAnthropicCacheControl, PyAnthropicCitationV1, PyAnthropicContentBlock,
    PyAnthropicDocumentSource, PyAnthropicImageSource, PyAnthropicMessage,
    PyAnthropicMessagesRequest, PyAnthropicMessagesResponse, PyAnthropicMessagesSettings,
    PyAnthropicOutputConfig, PyAnthropicSystem, PyAnthropicSystemBlock, PyAnthropicThinkingConfig,
    PyAnthropicTool, PyAnthropicToolResultContent, PyAnthropicUsage,
};
pub use google::{
    PyGeminiRequest, PyGeminiResponse, PyGoogleCandidate, PyGoogleCodeExecutionResult,
    PyGoogleContent, PyGoogleExecutableCode, PyGoogleFileData, PyGoogleFunctionCall,
    PyGoogleFunctionCallingConfig, PyGoogleFunctionDeclaration, PyGoogleFunctionResponse,
    PyGoogleGenerationConfig, PyGoogleInlineData, PyGooglePart, PyGoogleSafetyRating,
    PyGoogleSafetySetting, PyGoogleThinkingConfig, PyGoogleTool, PyGoogleToolConfig,
    PyGoogleUsageMetadata, PyVertexRequest, PyVertexResponse,
};
pub use openai_chat_messages::{
    PyOpenAiChatMessage, PyOpenAiContentPart, PyOpenAiFilePart, PyOpenAiImageUrl,
    PyOpenAiInputAudio, PyOpenAiMessageAnnotation, PyOpenAiMessageAudio, PyOpenAiMessageContent,
    PyOpenAiToolCall, PyOpenAiToolFunctionCall, PyOpenAiUrlCitation,
};
pub use openai_chat_request::{
    PyOpenAiAllowedTools, PyOpenAiAllowedToolsChoice, PyOpenAiChatAudio, PyOpenAiChatRequest,
    PyOpenAiChatSettings, PyOpenAiChatToolChoice, PyOpenAiCustomChoice, PyOpenAiCustomTool,
    PyOpenAiCustomToolFormat, PyOpenAiFunction, PyOpenAiFunctionChoice, PyOpenAiGrammar,
    PyOpenAiJsonSchema, PyOpenAiNamedCustomToolChoice, PyOpenAiNamedFunctionToolChoice,
    PyOpenAiPredictionContent, PyOpenAiPredictionContentPart, PyOpenAiPredictionPayload,
    PyOpenAiResponseFormat, PyOpenAiStop, PyOpenAiStreamOptions, PyOpenAiTool, PyOpenAiVoice,
};
pub use openai_chat_response::{
    PyOpenAiChatChoice, PyOpenAiChatLogprobs, PyOpenAiChatResponse,
    PyOpenAiCompletionTokensDetails, PyOpenAiPromptTokensDetails, PyOpenAiUsage,
};
pub use openai_responses::{
    PyOpenAiReasoning, PyOpenAiResponseContentPart, PyOpenAiResponseItem,
    PyOpenAiResponsesAllowedToolsChoice, PyOpenAiResponsesApplyPatchToolChoice,
    PyOpenAiResponsesCustomToolChoice, PyOpenAiResponsesFunctionToolChoice,
    PyOpenAiResponsesGrammar, PyOpenAiResponsesHostedToolChoice,
    PyOpenAiResponsesInputTokensDetails, PyOpenAiResponsesMcpToolChoice,
    PyOpenAiResponsesOutputTokensDetails, PyOpenAiResponsesRequest, PyOpenAiResponsesResponse,
    PyOpenAiResponsesSettings, PyOpenAiResponsesShellToolChoice, PyOpenAiResponsesText,
    PyOpenAiResponsesTool, PyOpenAiResponsesToolChoice, PyOpenAiResponsesUsage,
};
pub use response::PyProviderResponse;

pub fn register_prompt_provider_classes(module: &Bound<'_, PyModule>) -> PyResult<()> {
    response::register(module)?;
    openai_chat_request::register(module)?;
    openai_chat_messages::register(module)?;
    openai_chat_response::register(module)?;
    anthropic::register(module)?;
    google::register(module)?;
    openai_responses::register(module)?;
    Ok(())
}
