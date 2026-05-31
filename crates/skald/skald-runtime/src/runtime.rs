//! Runtime configuration and native dispatch entry point.

use std::sync::Arc;
use std::time::Duration;

use skald_cache::{InMemoryCache, PromptCache};
use skald_providers::ProviderStream;
use skald_spec::{ProviderRequest, ProviderResponse};

use crate::dispatch::{dispatch, dispatch_stream};
use crate::error::SkaldRuntimeResult;
use crate::provider::ProviderRegistry;

/// Runtime defaults for provider dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    /// Provider request timeout used by default-built clients.
    pub default_timeout: Duration,
    /// In-memory prompt cache capacity used by [`SkaldRuntime::from_env`].
    pub cache_capacity: usize,
    /// In-memory prompt cache TTL used by [`SkaldRuntime::from_env`].
    pub cache_default_ttl: Duration,
    /// Default retry budget for provider clients that consume runtime config.
    pub max_retries: u32,
    /// Default model path component for Google and Vertex clients built from env.
    pub default_google_model: String,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            default_timeout: Duration::from_secs(120),
            cache_capacity: 1024,
            cache_default_ttl: Duration::from_secs(60 * 60),
            max_retries: 2,
            default_google_model: "gemini-2.5-flash".to_owned(),
        }
    }
}

/// Native Skald runtime over registered provider clients.
#[derive(Clone)]
pub struct SkaldRuntime {
    providers: ProviderRegistry,
    cache: Arc<dyn PromptCache + Send + Sync>,
    config: RuntimeConfig,
}

impl SkaldRuntime {
    /// Creates a runtime from an explicit registry, cache, and config.
    pub fn new(
        providers: ProviderRegistry,
        cache: Arc<dyn PromptCache + Send + Sync>,
        config: RuntimeConfig,
    ) -> Self {
        Self {
            providers,
            cache,
            config,
        }
    }

    /// Builds a runtime from environment-backed providers and an in-memory cache.
    pub fn from_env() -> Self {
        let config = RuntimeConfig::default();
        let providers = ProviderRegistry::from_env(&config.default_google_model);
        let cache: Arc<dyn PromptCache + Send + Sync> = Arc::new(InMemoryCache::new(
            config.cache_capacity,
            config.cache_default_ttl,
        ));
        Self::new(providers, cache, config)
    }

    /// Sends one native provider request and returns the native response.
    pub async fn send(&self, request: ProviderRequest) -> SkaldRuntimeResult<ProviderResponse> {
        dispatch(&self.providers, request).await
    }

    /// Sends one native streaming request and returns provider-native chunks.
    pub async fn stream(&self, request: ProviderRequest) -> SkaldRuntimeResult<ProviderStream> {
        dispatch_stream(&self.providers, request).await
    }

    /// Returns the runtime provider registry.
    pub const fn providers(&self) -> &ProviderRegistry {
        &self.providers
    }

    /// Returns the runtime prompt cache handle.
    pub fn cache(&self) -> &Arc<dyn PromptCache + Send + Sync> {
        &self.cache
    }

    /// Returns runtime configuration.
    pub const fn config(&self) -> &RuntimeConfig {
        &self.config
    }
}
