//! Cache key and value contracts for native provider requests.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use skald_spec::wire::anthropic_messages::{
    AnthropicContentBlock, AnthropicMessagesRequest, AnthropicSystem, AnthropicSystemBlock,
};
use skald_spec::{ProviderName, ProviderRequest};

use crate::error::{SkaldCacheError, SkaldCacheResult};

/// Deterministic cache identity scoped by provider, model, and native content.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CacheKey {
    /// Provider scope prevents same-hash requests from colliding across APIs.
    pub provider: ProviderName,
    /// Native model identifier from the typed provider request.
    pub model: String,
    /// SHA-256 hex digest of the provider-native cache key material.
    pub request_prefix_hash: String,
}

/// Stored cache metadata returned by cache backends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CacheValue {
    /// Provider-owned id for the cached resource.
    pub provider_resource_id: String,
    /// Provider that owns the cached resource id.
    pub provider: ProviderName,
    /// Creation timestamp in caller-defined epoch seconds.
    pub created_at: u64,
    /// Cache value TTL in seconds.
    pub ttl_seconds: u64,
    /// Provider-specific metadata retained without interpretation.
    pub metadata: Map<String, Value>,
}

impl CacheValue {
    /// Creates a cache value with no provider-specific metadata.
    pub fn new(
        provider_resource_id: impl Into<String>,
        provider: ProviderName,
        created_at: u64,
        ttl_seconds: u64,
    ) -> Self {
        Self {
            provider_resource_id: provider_resource_id.into(),
            provider,
            created_at,
            ttl_seconds,
            metadata: Map::new(),
        }
    }
}

impl CacheKey {
    /// Derives a cache key from provider-native cache directives when present.
    pub fn from_request(request: &ProviderRequest) -> SkaldCacheResult<Option<Self>> {
        match request {
            ProviderRequest::OpenAiChatCompletion(request) => Ok(request
                .settings
                .prompt_cache_key
                .as_deref()
                .map(|prompt_cache_key| Self {
                    provider: ProviderName::OpenAi,
                    model: request.model.clone(),
                    // OpenAI exposes the cache resource id directly. Keep that
                    // id scoped by provider and model in the local key.
                    request_prefix_hash: hash_bytes(prompt_cache_key.as_bytes()),
                })),
            ProviderRequest::AnthropicMessage(request) => {
                if !anthropic_has_cache_control(request) {
                    return Ok(None);
                }

                // Anthropic cache directives live on native system, message,
                // and tool blocks. Hash the typed request after confirming at
                // least one native directive is present.
                let body = serde_json::to_vec(request).map_err(SkaldCacheError::serialize)?;
                Ok(Some(Self {
                    provider: ProviderName::Anthropic,
                    model: request.model.clone(),
                    request_prefix_hash: hash_bytes(&body),
                }))
            }
            ProviderRequest::RawV1 { .. } => Err(SkaldCacheError::OpaqueRequest),
            ProviderRequest::OpenAiResponses(_)
            | ProviderRequest::GeminiGenerateContent(_)
            | ProviderRequest::Vertex(_) => Ok(None),
            _ => Ok(None),
        }
    }
}

/// Finds any native Anthropic cache directive that makes this request cacheable.
fn anthropic_has_cache_control(request: &AnthropicMessagesRequest) -> bool {
    request
        .system
        .as_ref()
        .is_some_and(system_has_cache_control)
        || request
            .messages
            .iter()
            .flat_map(|message| message.content.iter())
            .any(content_has_cache_control)
        || request
            .tools
            .as_ref()
            .is_some_and(|tools| tools.iter().any(|tool| tool.cache_control.is_some()))
}

/// Checks cache directives on Anthropic system blocks.
fn system_has_cache_control(system: &AnthropicSystem) -> bool {
    match system {
        AnthropicSystem::Text(_) => false,
        AnthropicSystem::Blocks(blocks) => blocks.iter().any(|block| match block {
            AnthropicSystemBlock::Text { cache_control, .. } => cache_control.is_some(),
        }),
    }
}

/// Checks cache directives on Anthropic message content blocks.
fn content_has_cache_control(block: &AnthropicContentBlock) -> bool {
    match block {
        AnthropicContentBlock::Text { cache_control, .. }
        | AnthropicContentBlock::Image { cache_control, .. }
        | AnthropicContentBlock::Document { cache_control, .. }
        | AnthropicContentBlock::ToolResult { cache_control, .. } => cache_control.is_some(),
        AnthropicContentBlock::Thinking { .. }
        | AnthropicContentBlock::RedactedThinking { .. }
        | AnthropicContentBlock::ToolUse { .. } => false,
    }
}

fn hash_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
