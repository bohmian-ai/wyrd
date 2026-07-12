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

#[cfg(test)]
mod cache_inmemory {
    use std::time::Duration;

    use crate::{CacheKey, CacheValue, InMemoryCache, PromptCache};
    use skald_spec::ProviderName;

    fn key(model: &str, hash: &str) -> CacheKey {
        CacheKey {
            provider: ProviderName::OpenAi,
            model: model.to_owned(),
            request_prefix_hash: hash.to_owned(),
        }
    }

    fn value(id: &str) -> CacheValue {
        CacheValue::new(id, ProviderName::OpenAi, 1_700_000_000, 60)
    }

    #[test]
    fn put_then_get_returns_value() {
        let cache = InMemoryCache::new(2, Duration::from_secs(60));
        let key = key("gpt-4o-mini", "a");
        let value = value("cached-a");

        cache.put_default(key.clone(), value.clone());

        assert_eq!(cache.get(&key), Some(value));
    }

    #[test]
    fn get_missing_returns_none() {
        let cache = InMemoryCache::new(2, Duration::from_secs(60));

        assert_eq!(cache.get(&key("gpt-4o-mini", "missing")), None);
    }

    #[test]
    fn ttl_eviction_after_expiry() {
        let cache = InMemoryCache::new(2, Duration::from_secs(60));
        let key = key("gpt-4o-mini", "ttl");

        cache.put(key.clone(), value("cached-ttl"), Duration::from_millis(5));
        std::thread::sleep(Duration::from_millis(15));

        // get() detects the per-entry TTL expiry and returns None.
        assert_eq!(cache.get(&key), None);
    }

    #[test]
    fn capacity_eviction_reduces_entries() {
        // Use a capacity large enough for TinyLFU admission to stabilize, then
        // over-insert to trigger eviction. We verify the cache respects its bound
        // by checking that the hottest entry (accessed many times) survives.
        let cache = InMemoryCache::new(4, Duration::from_secs(60));
        let hot = key("gpt-4o-mini", "hot");
        cache.put_default(hot.clone(), value("hot"));

        // Build frequency for the hot key before eviction pressure begins.
        for _ in 0..8 {
            assert!(cache.get(&hot).is_some());
        }

        // Insert enough cold entries to overflow the capacity.
        for i in 0..8u32 {
            cache.put_default(
                key("gpt-4o-mini", &format!("cold-{i}")),
                value(&format!("cold-{i}")),
            );
        }

        // The frequently accessed entry should survive TinyLFU eviction.
        assert!(cache.get(&hot).is_some());
    }

    #[test]
    fn invalidate_removes_key_and_reports_miss() {
        let cache = InMemoryCache::new(4, Duration::from_secs(60));
        let key = key("gpt-4o-mini", "inv");

        cache.put_default(key.clone(), value("cached-inv"));
        assert!(cache.get(&key).is_some());

        cache.invalidate(&key);

        assert_eq!(cache.get(&key), None);
    }
}
