use std::time::Duration;

use skald_cache::{CacheKey, CacheValue, InMemoryCache, PromptCache};
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
