//! Native prompt cache primitives for Skald provider requests.
//!
//! This crate owns cache key and value contracts and local cache storage only. It has no
//! provider clients, async runtime, language binding surface, or Wyrd crate dependency.

pub mod inmemory;
pub mod key;

pub use inmemory::{InMemoryCache, PromptCache};
pub use key::{CacheKey, CacheValue};
