//! Provider dispatch helpers.

use std::sync::atomic::{AtomicU64, Ordering};

use skald_providers::ProviderStream;
use skald_spec::{ProviderRequest, ProviderResponse};
use tracing::{Span, instrument};

use crate::error::{SkaldRuntimeError, SkaldRuntimeResult};
use crate::provider::ProviderRegistry;

static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Routes a native request to its registered provider and returns a native response.
#[instrument(skip_all, fields(
    provider = tracing::field::Empty,
    model = tracing::field::Empty,
    request_id = tracing::field::Empty,
    usage_input_tokens = tracing::field::Empty,
    usage_output_tokens = tracing::field::Empty,
))]
pub async fn dispatch(
    providers: &ProviderRegistry,
    request: ProviderRequest,
) -> SkaldRuntimeResult<ProviderResponse> {
    let provider = request.provider();
    let request_id = REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    Span::current().record("provider", tracing::field::debug(&provider));
    if let Some(model) = request_model(&request) {
        Span::current().record("model", tracing::field::display(model));
    }
    Span::current().record("request_id", request_id);
    let client = providers
        .get(&provider)
        .ok_or_else(|| SkaldRuntimeError::provider_not_registered(provider.clone()))?;
    let response = client
        .send(request)
        .await
        .map_err(|source| SkaldRuntimeError::from_provider(provider, source))?;
    if let Some(usage) = response.adapter().usage() {
        Span::current().record("usage_input_tokens", usage.usage.input_tokens);
        Span::current().record("usage_output_tokens", usage.usage.output_tokens);
    }
    Ok(response)
}

/// Routes a native streaming request to its registered provider.
#[instrument(skip_all, fields(
    provider = tracing::field::Empty,
    model = tracing::field::Empty,
    request_id = tracing::field::Empty,
))]
pub async fn dispatch_stream(
    providers: &ProviderRegistry,
    request: ProviderRequest,
) -> SkaldRuntimeResult<ProviderStream> {
    let provider = request.provider();
    let request_id = REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    Span::current().record("provider", tracing::field::debug(&provider));
    if let Some(model) = request_model(&request) {
        Span::current().record("model", tracing::field::display(model));
    }
    Span::current().record("request_id", request_id);
    let client = providers
        .get(&provider)
        .ok_or_else(|| SkaldRuntimeError::provider_not_registered(provider.clone()))?;
    client
        .stream(request)
        .await
        .map_err(|source| SkaldRuntimeError::from_provider(provider, source))
}

fn request_model(request: &ProviderRequest) -> Option<&str> {
    match request {
        ProviderRequest::OpenAiChatCompletion(request)
        | ProviderRequest::OpenAiChatCompatible { request, .. } => Some(&request.model),
        ProviderRequest::OpenAiResponses(request) => Some(&request.model),
        ProviderRequest::OpenAiEmbeddings(request) => Some(&request.model),
        ProviderRequest::AnthropicMessage(request) => Some(&request.model),
        ProviderRequest::GeminiGenerateContent(_)
        | ProviderRequest::GoogleBatchEmbed(_)
        | ProviderRequest::Vertex(_)
        | ProviderRequest::VertexPredict(_)
        | ProviderRequest::RawV1 { .. } => None,
        // ProviderRequest is #[non_exhaustive]. Any new variant that carries a
        // model field must be added above; this arm exists only for forward
        // compatibility and will silently omit model telemetry for unknown variants.
        _ => None,
    }
}
