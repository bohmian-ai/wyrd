//! Skald-spec: the one native provider type set, shared end to end.
//!
//! Pure Rust. No IO. No async. No PyO3. No HTTP clients.

#![allow(clippy::module_name_repetitions)]

pub mod message;
pub mod request;
pub mod response;
pub mod wire;

pub use message::MessageNum;
pub use request::{ProviderName, ProviderRequest};
pub use response::ProviderResponse;
pub use wire::anthropic_citation::AnthropicCitationV1;
pub use wire::anthropic_messages::{
    AnthropicContentBlock, AnthropicMessage, AnthropicMessagesRequest, AnthropicMessagesResponse,
    AnthropicStopReason, AnthropicStreamEvent, AnthropicUsage,
};
pub use wire::common::{FinishReason, TokenUsage};
pub use wire::google_embeddings::{
    GoogleBatchEmbedRequest, GoogleBatchEmbedResponse, GoogleEmbedding,
};
pub use wire::google_generate::{
    GoogleCandidate, GoogleContent, GoogleFinishReason, GoogleGenerateContentRequest,
    GoogleGenerateContentResponse, GooglePart, GoogleSafetyRating, GoogleUsageMetadata,
};
pub use wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatChoiceDelta, OpenAiChatLogprobs, OpenAiChatMessage,
    OpenAiChatRequest, OpenAiChatResponse, OpenAiChatStreamChunk, OpenAiToolCall, OpenAiUsage,
};
pub use wire::openai_embeddings::{
    OpenAiEmbeddingVector, OpenAiEmbeddingsRequest, OpenAiEmbeddingsResponse,
};
pub use wire::openai_responses::{
    OpenAiResponseItem, OpenAiResponsesRequest, OpenAiResponsesResponse, OpenAiResponsesStreamEvent,
};
pub use wire::vertex_generate::VertexGenerateContentRequest;
pub use wire::vertex_predict::{VertexPredictRequest, VertexPredictResponse, VertexPrediction};
