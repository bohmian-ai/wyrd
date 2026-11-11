//! Provider dispatch helpers.

use std::sync::atomic::{AtomicU64, Ordering};

use skald_providers::ProviderStream;
use skald_spec::{ProviderName, ProviderRequest, ProviderResponse};
use tracing::{Instrument as _, Span, info_span};

use crate::error::{SkaldRuntimeError, SkaldRuntimeResult};
use crate::provider::ProviderRegistry;

static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Routes a native request to the client registered under `provider` and
/// returns a native response.
///
/// `provider` is the dispatch target, normally `Prompt::provider`; it may name
/// a custom OpenAI-compatible client for an OpenAI Chat request.
///
/// # Errors
///
/// Returns [`SkaldRuntimeError::ProviderNotRegistered`] when no client is
/// registered under `provider`, or the client's provider failure.
pub async fn dispatch(
    providers: &ProviderRegistry,
    provider: ProviderName,
    request: ProviderRequest,
) -> SkaldRuntimeResult<ProviderResponse> {
    let span = dispatch_span(&provider, &request);
    async move {
        let client = providers
            .get(&provider)
            .ok_or_else(|| SkaldRuntimeError::provider_not_registered(provider.clone()))?;
        let response = client
            .send(request)
            .await
            .map_err(|source| SkaldRuntimeError::from_provider(provider, source))?;
        if let Some(usage) = response.adapter().usage() {
            let span = Span::current();
            span.record("usage_input_tokens", usage.usage.input_tokens);
            span.record("usage_output_tokens", usage.usage.output_tokens);
        }
        Ok(response)
    }
    .instrument(span)
    .await
}

/// Routes a native streaming request to the client registered under `provider`.
///
/// # Errors
///
/// Returns [`SkaldRuntimeError::ProviderNotRegistered`] when no client is
/// registered under `provider`, or the client's provider failure.
pub async fn dispatch_stream(
    providers: &ProviderRegistry,
    provider: ProviderName,
    request: ProviderRequest,
) -> SkaldRuntimeResult<ProviderStream> {
    let span = dispatch_span(&provider, &request);
    async move {
        let client = providers
            .get(&provider)
            .ok_or_else(|| SkaldRuntimeError::provider_not_registered(provider.clone()))?;
        client
            .stream(request)
            .await
            .map_err(|source| SkaldRuntimeError::from_provider(provider, source))
    }
    .instrument(span)
    .await
}

/// Opens the `dispatch` span of one send of `request` to `provider`, with
/// the provider, model, and a process-unique request id set at creation so
/// no field is recorded later; usage tokens stay empty until a response
/// reports them.
fn dispatch_span(provider: &ProviderName, request: &ProviderRequest) -> Span {
    info_span!(
        "dispatch",
        provider = ?provider,
        model = request_model(request),
        request_id = REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed),
        usage_input_tokens = tracing::field::Empty,
        usage_output_tokens = tracing::field::Empty,
    )
}

fn request_model(request: &ProviderRequest) -> Option<&str> {
    match request {
        ProviderRequest::OpenAiChatCompletion(request) => Some(&request.model),
        ProviderRequest::OpenAiResponses(request) => Some(&request.model),
        ProviderRequest::OpenAiEmbeddings(request) => Some(&request.model),
        ProviderRequest::AnthropicMessage(request) => Some(&request.model),
        // ProviderRequest is #[non_exhaustive]. Any new variant that carries a
        // model field must be added above; this arm exists only for forward
        // compatibility and will silently omit model telemetry for unknown variants.
        _ => None,
    }
}
