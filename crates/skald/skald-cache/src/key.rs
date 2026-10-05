//! Cache key and value contracts for native provider requests.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use skald_spec::ProviderName;

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
