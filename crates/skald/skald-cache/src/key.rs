//! Cache key and value contracts for native provider requests.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use skald_spec::wire::anthropic_messages::{
    AnthropicContentBlock, AnthropicMessage, AnthropicMessagesRequest, AnthropicSystem,
    AnthropicSystemBlock,
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
            ProviderRequest::OpenAiChatCompletion(request) => {
                openai_cache_key(ProviderName::OpenAi, request)
            }
            ProviderRequest::OpenAiChatCompatible { provider, request } => {
                openai_cache_key(provider.clone(), request)
            }
            ProviderRequest::AnthropicMessage(request) => {
                if !anthropic_has_cache_control(request) {
                    return Ok(None);
                }

                // Hash only the static cache-controlled prefix (system blocks
                // + leading messages with cache_control) so that multi-turn
                // conversations with a stable system prompt and prefix produce
                // cache hits regardless of trailing message count.
                let prefix_bytes =
                    anthropic_cache_prefix_bytes(request).map_err(SkaldCacheError::serialize)?;
                Ok(Some(Self {
                    provider: ProviderName::Anthropic,
                    model: request.model.clone(),
                    request_prefix_hash: hash_bytes(&prefix_bytes),
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

fn openai_cache_key(
    provider: ProviderName,
    request: &skald_spec::OpenAiChatRequest,
) -> SkaldCacheResult<Option<CacheKey>> {
    Ok(request
        .settings
        .prompt_cache_key
        .as_deref()
        .map(|prompt_cache_key| CacheKey {
            provider,
            model: request.model.clone(),
            // OpenAI exposes the cache resource id directly. Keep that id
            // scoped by provider and model in the local key.
            request_prefix_hash: hash_bytes(prompt_cache_key.as_bytes()),
        }))
}

/// Serializes the cache-controlled prefix of an Anthropic request for hashing.
///
/// The prefix is the system (when any block carries cache_control) plus the
/// contiguous leading messages (from index 0) that have at least one content
/// block with cache_control. Hashing only this prefix means multi-turn
/// conversations produce cache hits as long as the prefix is stable.
fn anthropic_cache_prefix_bytes(
    request: &AnthropicMessagesRequest,
) -> Result<Vec<u8>, serde_json::Error> {
    let system_bytes: Vec<u8> = if request
        .system
        .as_ref()
        .is_some_and(system_has_cache_control)
    {
        serde_json::to_vec(&request.system)?
    } else {
        Vec::new()
    };

    let prefix_messages: Vec<&AnthropicMessage> = request
        .messages
        .iter()
        .take_while(|msg| msg.content.iter().any(content_has_cache_control))
        .collect();
    let messages_bytes = serde_json::to_vec(&prefix_messages)?;

    let mut bytes = system_bytes;
    bytes.extend_from_slice(&messages_bytes);
    Ok(bytes)
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

#[cfg(test)]
mod cache_key {
    use serde_json::json;

    use crate::{CacheKey, SkaldCacheError};
    use skald_spec::wire::anthropic_messages::AnthropicMessagesRequest;
    use skald_spec::wire::openai_chat::OpenAiChatRequest;
    use skald_spec::{ProviderName, ProviderRequest};

    fn openai_chat(prompt_cache_key: &str) -> ProviderRequest {
        ProviderRequest::OpenAiChatCompletion(
            serde_json::from_value::<OpenAiChatRequest>(json!({
                "model": "gpt-4o-mini",
                "messages": [{ "role": "user", "content": "cache this" }],
                "prompt_cache_key": prompt_cache_key
            }))
            .expect("valid OpenAI chat request"),
        )
    }

    fn anthropic_message(cache_control: bool) -> ProviderRequest {
        let content = if cache_control {
            json!([{
                "type": "text",
                "text": "cache this",
                "cache_control": { "type": "ephemeral" }
            }])
        } else {
            json!([{ "type": "text", "text": "do not cache" }])
        };

        ProviderRequest::AnthropicMessage(
            serde_json::from_value::<AnthropicMessagesRequest>(json!({
                "model": "claude-sonnet-4-5",
                "max_tokens": 256,
                "messages": [{ "role": "user", "content": content }]
            }))
            .expect("valid Anthropic messages request"),
        )
    }

    #[test]
    fn key_includes_content_hash_provider_scope() {
        let key = CacheKey::from_request(&openai_chat("tenant-a:prompt-1"))
            .expect("cache key derivation succeeds")
            .expect("prompt_cache_key creates a key");

        assert_eq!(key.provider, ProviderName::OpenAi);
        assert_eq!(key.model, "gpt-4o-mini");
        assert_eq!(key.request_prefix_hash.len(), 64);
    }

    #[test]
    fn key_equality_matches() {
        let left = CacheKey::from_request(&anthropic_message(true))
            .expect("cache key derivation succeeds")
            .expect("cache_control creates a key");
        let right = CacheKey::from_request(&anthropic_message(true))
            .expect("cache key derivation succeeds")
            .expect("cache_control creates a key");

        assert_eq!(left, right);
    }

    #[test]
    fn key_hash_distinct_for_different_scopes() {
        let openai = CacheKey::from_request(&openai_chat("shared-prefix"))
            .expect("cache key derivation succeeds")
            .expect("prompt_cache_key creates a key");
        let anthropic = CacheKey::from_request(&anthropic_message(true))
            .expect("cache key derivation succeeds")
            .expect("cache_control creates a key");

        assert_ne!(openai.provider, anthropic.provider);
        assert_ne!(openai, anthropic);
    }

    #[test]
    fn anthropic_without_cache_control_returns_none() {
        let key = CacheKey::from_request(&anthropic_message(false))
            .expect("cache key derivation succeeds");

        assert!(key.is_none());
    }

    #[test]
    fn raw_request_returns_opaque_request_error() {
        let body = serde_json::value::RawValue::from_string(r#"{"model":"x"}"#.to_owned())
            .expect("raw JSON is valid");
        let request = ProviderRequest::RawV1 {
            provider: ProviderName::Custom("local".to_owned()),
            body,
        };

        let error = CacheKey::from_request(&request).expect_err("raw request is opaque");

        assert_eq!(error, SkaldCacheError::OpaqueRequest);
        assert_eq!(error.code(), "SKALD_CACHE_400_OPAQUE_REQUEST");
    }
}
