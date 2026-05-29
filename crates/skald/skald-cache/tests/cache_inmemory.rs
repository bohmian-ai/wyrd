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

    assert_eq!(cache.get(&key), None);
    assert!(cache.is_empty());
}

#[test]
fn capacity_eviction_lru() {
    let cache = InMemoryCache::new(2, Duration::from_secs(60));
    let first = key("gpt-4o-mini", "first");
    let second = key("gpt-4o-mini", "second");
    let third = key("gpt-4o-mini", "third");

    cache.put_default(first.clone(), value("cached-first"));
    cache.put_default(second.clone(), value("cached-second"));
    assert!(cache.get(&first).is_some());
    cache.put_default(third.clone(), value("cached-third"));

    assert!(cache.get(&first).is_some());
    assert_eq!(cache.get(&second), None);
    assert!(cache.get(&third).is_some());
}
