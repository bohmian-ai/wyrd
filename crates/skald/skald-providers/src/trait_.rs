//! Async provider trait and native stream carrier.

use async_trait::async_trait;
use skald_spec::{
    AnthropicStreamEvent, OpenAiChatStreamChunk, OpenAiResponsesStreamEvent, ProviderRequest,
    ProviderResponse,
};

use crate::error::ProviderResult;

/// Native provider stream output variants.
#[derive(Debug, Clone, PartialEq)]
pub enum ProviderStream {
    /// OpenAI Chat Completions stream chunks.
    OpenAiChat(Vec<OpenAiChatStreamChunk>),
    /// OpenAI Responses stream events.
    OpenAiResponses(Vec<OpenAiResponsesStreamEvent>),
    /// Anthropic Messages stream events.
    Anthropic(Vec<AnthropicStreamEvent>),
    /// Google or Vertex JSONL GenerateContent chunks.
    Google(Vec<skald_spec::GoogleGenerateContentResponse>),
}

/// Async client surface over native provider request and response shapes.
#[async_trait]
pub trait ProviderClient: Send + Sync {
    /// Sends one native provider request and returns the native response.
    async fn send(&self, request: ProviderRequest) -> ProviderResult<ProviderResponse>;

    /// Sends one streaming request and decodes provider-native stream chunks.
    async fn stream(&self, request: ProviderRequest) -> ProviderResult<ProviderStream>;
}
