//! Provider dispatch helpers.
//!
//! Dispatch owns the one OpenTelemetry GenAI inference span of every model
//! call, named after the call's operation (`chat`, `generate_content`, or
//! `embeddings`).

use skald_providers::ProviderStream;
use skald_spec::{ProviderName, ProviderRequest, ProviderResponse};
use tracing::{Instrument as _, Span, field, info_span};

use crate::error::{SkaldRuntimeError, SkaldRuntimeResult};
use crate::provider::ProviderRegistry;

/// Routes a native request to the client registered under `provider` and
/// returns a native response.
///
/// `provider` is the dispatch target, normally `Prompt::provider`; it may name
/// a custom OpenAI-compatible client for an OpenAI Chat request. The call
/// runs inside its inference span (see [`inference_span`]), which records the
/// response's token usage or the failure's stable code.
///
/// # Errors
///
/// Returns [`SkaldRuntimeError::ProviderNotRegistered`] when no client is
/// registered under `provider`, or the client's provider failure.
pub async fn dispatch(
    providers: &ProviderRegistry,
    provider: ProviderName,
    request: &ProviderRequest,
    model: Option<&str>,
) -> SkaldRuntimeResult<ProviderResponse> {
    let span = inference_span(&provider, request, model);
    async move {
        let client = providers
            .get(&provider)
            .ok_or_else(|| SkaldRuntimeError::provider_not_registered(provider.clone()))?;
        client
            .send(request)
            .await
            .map_err(|source| SkaldRuntimeError::from_provider(provider, source))
    }
    .instrument(span.clone())
    .await
    .inspect(|response| {
        // OpenTelemetry has no unsigned integer: a `u64` would export as a
        // string, which Bifrost rejects for these integer GenAI attributes.
        if let Some(usage) = response.adapter().usage() {
            let tokens = |count: u64| i64::try_from(count).unwrap_or(i64::MAX);
            span.record(
                "gen_ai.usage.input_tokens",
                tokens(usage.usage.input_tokens),
            );
            span.record(
                "gen_ai.usage.output_tokens",
                tokens(usage.usage.output_tokens),
            );
        }
    })
    .inspect_err(|error| record_error(&span, error.code()))
}

/// Routes a native streaming request to the client registered under
/// `provider`, inside its inference span.
///
/// # Errors
///
/// Returns [`SkaldRuntimeError::ProviderNotRegistered`] when no client is
/// registered under `provider`, or the client's provider failure.
pub async fn dispatch_stream(
    providers: &ProviderRegistry,
    provider: ProviderName,
    request: &ProviderRequest,
    model: Option<&str>,
) -> SkaldRuntimeResult<ProviderStream> {
    let span = inference_span(&provider, request, model);
    async move {
        let client = providers
            .get(&provider)
            .ok_or_else(|| SkaldRuntimeError::provider_not_registered(provider.clone()))?;
        client
            .stream(request)
            .await
            .map_err(|source| SkaldRuntimeError::from_provider(provider, source))
    }
    .instrument(span.clone())
    .await
    .inspect_err(|error| record_error(&span, error.code()))
}

/// OpenTelemetry GenAI `gen_ai.provider.name` value for `provider`.
///
/// Uses the semantic-convention well-known values; a custom provider keeps
/// its own name.
#[must_use]
pub fn genai_provider_name(provider: &ProviderName) -> &str {
    match provider {
        ProviderName::OpenAi => "openai",
        ProviderName::Anthropic => "anthropic",
        ProviderName::Google => "gcp.gemini",
        ProviderName::Vertex => "gcp.vertex_ai",
        ProviderName::Jev => "jev",
        ProviderName::Custom(name) => name,
    }
}

/// Mark `span` failed with the stable error `code` as its `error.type`.
pub fn record_error(span: &Span, code: &str) {
    span.record("error.type", code);
    span.record("otel.status_code", "ERROR");
}

/// GenAI operation one model call performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operation {
    /// A chat API: OpenAI Chat Completions or Responses, Anthropic Messages.
    Chat,
    /// A content generation API: Google GenerateContent, Jev `systemone`, or
    /// a raw body no typed request models.
    GenerateContent,
    /// An embeddings API: OpenAI, Google batch, or Vertex predict embeddings.
    Embeddings,
}

impl Operation {
    /// The operation `request` performs.
    fn of(request: &ProviderRequest) -> Self {
        match request {
            ProviderRequest::OpenAiChatCompletion(_)
            | ProviderRequest::OpenAiResponses(_)
            | ProviderRequest::AnthropicMessage(_) => Self::Chat,
            ProviderRequest::OpenAiEmbeddings(_)
            | ProviderRequest::GoogleBatchEmbed(_)
            | ProviderRequest::VertexPredict(_) => Self::Embeddings,
            _ => Self::GenerateContent,
        }
    }
}

/// Open the GenAI inference span of one call of `request` to `provider`.
///
/// The span is named after the call's operation, the same value as its
/// `gen_ai.operation.name`, so every name is static and the tenant catalog
/// lists it. It records the semantic provider name and the model the
/// request is made to: the request's own model, which reflects any
/// `before_model` replacement, or `model` for wire shapes that carry none
/// (Gemini, Vertex). Token usage and `error.type` stay empty until the call
/// ends. No payload is recorded.
fn inference_span(provider: &ProviderName, request: &ProviderRequest, model: Option<&str>) -> Span {
    macro_rules! span {
        ($operation:literal) => {
            info_span!(
                $operation,
                gen_ai.operation.name = $operation,
                gen_ai.provider.name = genai_provider_name(provider),
                gen_ai.request.model = request_model(request).or(model),
                gen_ai.usage.input_tokens = field::Empty,
                gen_ai.usage.output_tokens = field::Empty,
                error.r#type = field::Empty,
                otel.status_code = field::Empty,
            )
        };
    }
    match Operation::of(request) {
        Operation::Chat => span!("chat"),
        Operation::GenerateContent => span!("generate_content"),
        Operation::Embeddings => span!("embeddings"),
    }
}

/// The model `request` names, when its wire shape carries one.
fn request_model(request: &ProviderRequest) -> Option<&str> {
    match request {
        ProviderRequest::OpenAiChatCompletion(request) => Some(&request.model),
        ProviderRequest::OpenAiResponses(request) => Some(&request.model),
        ProviderRequest::OpenAiEmbeddings(request) => Some(&request.model),
        ProviderRequest::AnthropicMessage(request) => Some(&request.model),
        ProviderRequest::JevSystemOne(request) => Some(&request.model),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// Chat APIs are `chat`, GenerateContent and Jev are `generate_content`,
    /// and embedding APIs are `embeddings`.
    ///
    /// # Panics
    /// Panics when a request kind maps to another operation.
    #[test]
    fn requests_map_to_their_genai_operation() {
        let request = |kind: &str, body: serde_json::Value| -> ProviderRequest {
            serde_json::from_value(json!({"provider": kind, "body": body}))
                .expect("request decodes")
        };
        let messages = json!([{"role": "user", "content": "hi"}]);
        for (request, operation) in [
            (
                request(
                    "open_ai_chat_completion",
                    json!({"model": "m", "messages": messages}),
                ),
                Operation::Chat,
            ),
            (
                request(
                    "gemini_generate_content",
                    json!({"contents": [{"role": "user", "parts": [{"text": "hi"}]}]}),
                ),
                Operation::GenerateContent,
            ),
            (
                request("open_ai_embeddings", json!({"model": "m", "input": "hi"})),
                Operation::Embeddings,
            ),
        ] {
            assert_eq!(Operation::of(&request), operation, "{request:?}");
        }
    }
}
