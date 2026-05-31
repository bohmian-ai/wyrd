//! Synchronous in-memory cache backend backed by mini-moka.

use std::time::{Duration, Instant};

use mini_moka::sync::Cache;

use crate::{CacheKey, CacheValue};

/// Minimal synchronous cache API for prompt cache implementations.
pub trait PromptCache {
    /// Returns a value for `key` when present and not expired.
    fn get(&self, key: &CacheKey) -> Option<CacheValue>;

    /// Stores `value` for `key` with a per-entry TTL.
    fn put(&self, key: CacheKey, value: CacheValue, ttl: Duration);

    /// Removes a key when present.
    fn invalidate(&self, key: &CacheKey);
}

/// Capacity-limited in-memory prompt cache with per-entry TTL and TinyLFU eviction.
///
/// Per-entry TTL is tracked via an expiry `Instant` stored alongside each value.
/// mini-moka manages capacity eviction; this wrapper manages TTL expiry on access.
#[derive(Clone)]
pub struct InMemoryCache {
    inner: Cache<CacheKey, (CacheValue, Instant)>,
    default_ttl: Duration,
}

impl std::fmt::Debug for InMemoryCache {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("InMemoryCache")
            .field("capacity", &self.inner.policy().max_capacity())
            .field("default_ttl", &self.default_ttl)
            .finish()
    }
}

impl InMemoryCache {
    /// Creates a bounded in-memory cache with the given capacity and default TTL.
    pub fn new(capacity: usize, default_ttl: Duration) -> Self {
        Self {
            inner: Cache::builder().max_capacity(capacity as u64).build(),
            default_ttl,
        }
    }

    /// Stores a value using the cache's default TTL.
    pub fn put_default(&self, key: CacheKey, value: CacheValue) {
        self.put(key, value, self.default_ttl);
    }

    /// Returns the approximate number of live entries in the cache.
    pub fn len(&self) -> usize {
        self.inner.entry_count() as usize
    }

    /// Returns whether the cache has no entries.
    pub fn is_empty(&self) -> bool {
        self.inner.entry_count() == 0
    }
}

impl PromptCache for InMemoryCache {
    fn get(&self, key: &CacheKey) -> Option<CacheValue> {
        let (value, expiry) = self.inner.get(key)?;
        if Instant::now() >= expiry {
            // TTL elapsed — remove the entry and report a miss.
            self.inner.invalidate(key);
            return None;
        }
        Some(value)
    }

    fn put(&self, key: CacheKey, value: CacheValue, ttl: Duration) {
        self.inner.insert(key, (value, Instant::now() + ttl));
    }

    fn invalidate(&self, key: &CacheKey) {
        self.inner.invalidate(key);
    }
}
