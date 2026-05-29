//! Synchronous in-memory cache backend for native Skald cache keys.

use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::{CacheKey, CacheValue};

/// Minimal synchronous cache API for prompt cache implementations.
pub trait PromptCache {
    /// Returns a value for `key`, refreshing its LRU position when present.
    fn get(&self, key: &CacheKey) -> Option<CacheValue>;

    /// Stores `value` for `key` with a per-entry TTL.
    fn put(&self, key: CacheKey, value: CacheValue, ttl: Duration);

    /// Removes a key when present.
    fn invalidate(&self, key: &CacheKey);
}

/// Capacity-limited in-memory prompt cache with TTL and LRU eviction.
#[derive(Debug)]
pub struct InMemoryCache {
    capacity: usize,
    default_ttl: Duration,
    inner: Mutex<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    entries: HashMap<CacheKey, Entry>,
    order: VecDeque<CacheKey>,
}

#[derive(Debug, Clone)]
struct Entry {
    value: CacheValue,
    inserted_at: Instant,
    ttl: Duration,
}

impl InMemoryCache {
    /// Creates a bounded in-memory cache.
    pub fn new(capacity: usize, default_ttl: Duration) -> Self {
        Self {
            capacity,
            default_ttl,
            inner: Mutex::new(Inner::default()),
        }
    }

    /// Stores a value using the cache's default TTL.
    pub fn put_default(&self, key: CacheKey, value: CacheValue) {
        self.put(key, value, self.default_ttl);
    }

    /// Returns the current number of live entries after removing expired ones.
    pub fn len(&self) -> usize {
        let mut inner = self.lock_inner();
        inner.remove_expired();
        inner.entries.len()
    }

    /// Returns whether the cache currently has no live entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lock_inner(&self) -> MutexGuard<'_, Inner> {
        match self.inner.lock() {
            Ok(inner) => inner,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

impl PromptCache for InMemoryCache {
    fn get(&self, key: &CacheKey) -> Option<CacheValue> {
        let mut inner = self.lock_inner();
        inner.remove_expired();

        let value = inner.entries.get(key).map(|entry| entry.value.clone())?;
        inner.touch(key);
        Some(value)
    }

    fn put(&self, key: CacheKey, value: CacheValue, ttl: Duration) {
        if self.capacity == 0 {
            return;
        }

        let mut inner = self.lock_inner();
        inner.remove_expired();
        inner.entries.insert(
            key.clone(),
            Entry {
                value,
                inserted_at: Instant::now(),
                ttl,
            },
        );
        inner.touch(&key);
        inner.evict_to_capacity(self.capacity);
    }

    fn invalidate(&self, key: &CacheKey) {
        let mut inner = self.lock_inner();
        inner.entries.remove(key);
        inner.order.retain(|candidate| candidate != key);
    }
}

impl Inner {
    fn touch(&mut self, key: &CacheKey) {
        self.order.retain(|candidate| candidate != key);
        self.order.push_back(key.clone());
    }

    fn remove_expired(&mut self) {
        let now = Instant::now();
        self.entries
            .retain(|_, entry| now.duration_since(entry.inserted_at) < entry.ttl);
        self.order
            .retain(|candidate| self.entries.contains_key(candidate));
    }

    fn evict_to_capacity(&mut self, capacity: usize) {
        while self.entries.len() > capacity {
            if let Some(key) = self.order.pop_front() {
                self.entries.remove(&key);
            } else {
                break;
            }
        }
    }
}
