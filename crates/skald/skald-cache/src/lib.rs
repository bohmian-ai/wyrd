//! Native prompt cache primitives for Skald provider requests.
//!
//! This crate owns cache key derivation and local cache storage only. It has no
//! provider clients, async runtime, language binding surface, or Wyrd crate dependency.

#![allow(clippy::module_name_repetitions)]

pub mod error;
pub mod inmemory;
pub mod key;

pub use error::{SkaldCacheError, SkaldCacheResult};
pub use inmemory::{InMemoryCache, PromptCache};
pub use key::{CacheKey, CacheValue};
