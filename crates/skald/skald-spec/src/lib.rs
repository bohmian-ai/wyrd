//! Skald-spec: the one native provider type set, shared end to end.
//!
//! Pure Rust. No IO. No async. No PyO3. No HTTP clients.

#![allow(clippy::module_name_repetitions)]

pub mod adapter;
pub mod convert;
pub mod error;
pub mod message;
pub mod prompt;
pub mod request;
pub mod response;
pub mod wire;

pub use adapter::{ResponseAdapter, ToolCallView, UsageView};
pub use convert::{MessageConversion, convert_message_dyn};
pub use error::{SkaldError, SkaldResult};
pub use message::MessageNum;
pub use prompt::{Prompt, ResponseType};
pub use request::{ProviderName, ProviderRequest};
pub use response::ProviderResponse;
pub use wire::anthropic_citation::AnthropicCitationV1;
pub use wire::anthropic_messages::{
    AnthropicContentBlock, AnthropicMessage, AnthropicMessagesRequest, AnthropicMessagesResponse,
    AnthropicStopReason, AnthropicStreamEvent, AnthropicUsage,
};
pub use wire::common::{FinishReason, TokenUsage};
pub use wire::google_embeddings::{
    GoogleBatchEmbedRequest, GoogleBatchEmbedResponse, GoogleEmbedContent, GoogleEmbedPart,
    GoogleEmbedRequest, GoogleEmbedding,
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
    OpenAiEmbeddingVector, OpenAiEmbeddingsInput, OpenAiEmbeddingsRequest, OpenAiEmbeddingsResponse,
};
pub use wire::openai_responses::{
    OpenAiResponseItem, OpenAiResponsesRequest, OpenAiResponsesResponse, OpenAiResponsesStreamEvent,
};
pub use wire::vertex_generate::VertexGenerateContentRequest;
pub use wire::vertex_predict::{VertexPredictRequest, VertexPredictResponse, VertexPrediction};
