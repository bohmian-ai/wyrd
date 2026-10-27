//! The node's concurrent Parquet footer cache.

use std::time::Duration;

use datafusion::common::{HashMap, TableReference};
use datafusion::execution::cache::cache_manager::CachedFileMetadataEntry;
use datafusion::execution::cache::{Cache, CacheEntryInfo, CacheValue as _};
use object_store::path::Path;

/// `DataFusion`'s file metadata cache, served without a global lock.
///
/// `DataFusion`'s `DefaultCache` keeps its LRU behind one `Mutex`, and a hit
/// reorders the LRU, so every footer lookup of every concurrent query
/// serializes on that lock. This cache is `mini_moka`'s concurrent cache
/// instead: lookups read a sharded map and record recency in a buffer the
/// cache drains in batches. Entries are weighed by
/// [`CachedFileMetadataEntry::size`] against a byte limit fixed at
/// construction, because [`super::BifrostStorage::with_metadata_cache`]
/// charges that whole limit to the Oracle memory root up front.
pub(crate) struct FooterCache {
    /// Footers by object path, weighed in bytes.
    entries: mini_moka::sync::Cache<Path, CachedFileMetadataEntry>,
    /// The byte limit the cache was built with.
    limit: usize,
}

impl FooterCache {
    /// Builds an empty cache holding at most `limit` bytes of footers.
    ///
    /// A footer larger than `u32::MAX` bytes weighs `u32::MAX`, the most the
    /// cache can weigh one entry.
    pub(crate) fn new(limit: usize) -> Self {
        let entries = mini_moka::sync::Cache::builder()
            .max_capacity(u64::try_from(limit).unwrap_or(u64::MAX))
            .weigher(|_, entry: &CachedFileMetadataEntry| {
                u32::try_from(entry.size()).unwrap_or(u32::MAX)
            })
            .build();
        Self { entries, limit }
    }
}

impl Cache<Path, CachedFileMetadataEntry> for FooterCache {
    /// Returns the cached footer for `key`, recording the hit for eviction.
    ///
    /// Each lookup counts in `bifrost_footer_cache_hits_total` or
    /// `bifrost_footer_cache_misses_total`, so a scan that re-reads footers
    /// shows up.
    fn get(&self, key: &Path) -> Option<CachedFileMetadataEntry> {
        let entry = self.entries.get(key);
        let counter = if entry.is_some() {
            "bifrost_footer_cache_hits_total"
        } else {
            "bifrost_footer_cache_misses_total"
        };
        metrics::counter!(counter).increment(1);
        entry
    }

    /// Caches `value` under `key` and returns the footer it replaced.
    ///
    /// The read and the insert are separate operations, so a racing insert
    /// of the same key may be the one returned or the one replaced.
    fn put(&self, key: &Path, value: CachedFileMetadataEntry) -> Option<CachedFileMetadataEntry> {
        let previous = self.entries.get(key);
        self.entries.insert(key.clone(), value);
        previous
    }

    /// Evicts `key` and returns the footer it held.
    fn remove(&self, key: &Path) -> Option<CachedFileMetadataEntry> {
        let previous = self.entries.get(key);
        self.entries.invalidate(key);
        previous
    }

    /// Reports whether `key` is cached, without recording a hit.
    fn contains_key(&self, key: &Path) -> bool {
        self.entries.contains_key(key)
    }

    /// Returns the number of cached footers, which may lag recent inserts
    /// and evictions until the cache drains its pending work.
    fn len(&self) -> usize {
        usize::try_from(self.entries.entry_count()).unwrap_or(usize::MAX)
    }

    /// Evicts every footer.
    fn clear(&self) {
        self.entries.invalidate_all();
    }

    /// Names the cache in `DataFusion` diagnostics.
    fn name(&self) -> String {
        "BifrostFooterCache".to_owned()
    }

    /// Returns the byte limit the cache was built with.
    fn cache_limit(&self) -> usize {
        self.limit
    }

    /// Keeps the construction limit: it is already charged to the Oracle
    /// memory root, and no Wyrd path hands this cache to a `DataFusion`
    /// `CacheManager` that would resize it.
    fn update_cache_limit(&self, limit: usize) {
        if limit != self.limit {
            tracing::warn!(
                limit,
                kept = self.limit,
                "footer cache keeps its charged byte limit"
            );
        }
    }

    /// Footers never expire on their own; a changed file fails the reader's
    /// [`CachedFileMetadataEntry::is_valid_for`] check instead.
    fn cache_ttl(&self) -> Option<Duration> {
        None
    }

    /// Ignored: footers never expire on their own.
    fn update_cache_ttl(&self, _ttl: Option<Duration>) {}

    /// Does nothing: footers are keyed by object path, which names no table.
    ///
    /// # Errors
    ///
    /// Never fails.
    fn drop_table_entries(&self, _table_ref: &TableReference) -> datafusion::common::Result<()> {
        Ok(())
    }

    /// Lists every cached footer with its weight; hits are counted per cache
    /// in `bifrost_footer_cache_hits_total`, not per entry, so each entry
    /// reports zero.
    fn list_entries(&self) -> HashMap<Path, CacheEntryInfo<CachedFileMetadataEntry>> {
        self.entries
            .iter()
            .map(|entry| {
                let value = entry.value().clone();
                let size_bytes = value.size();
                let info = CacheEntryInfo {
                    value,
                    size_bytes,
                    hits: 0,
                    expires: None,
                };
                (entry.key().clone(), info)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::any::Any;
    use std::sync::Arc;

    use datafusion::execution::cache::cache_manager::FileMetadata;
    use object_store::ObjectMeta;

    use super::*;

    /// A footer that reports a fixed weight.
    struct Weighed(usize);

    impl FileMetadata for Weighed {
        /// Exposes the footer for downcasting, as `DataFusion` requires.
        fn as_any(&self) -> &dyn Any {
            self
        }

        /// Reports the fixed weight.
        fn memory_size(&self) -> usize {
            self.0
        }

        /// Reports nothing extra.
        fn extra_info(&self) -> HashMap<String, String> {
            HashMap::new()
        }
    }

    /// Builds a cache entry for `path` that weighs `bytes`.
    fn entry(path: &Path, bytes: usize) -> CachedFileMetadataEntry {
        let meta = ObjectMeta {
            location: path.clone(),
            last_modified: chrono::DateTime::UNIX_EPOCH,
            size: 1,
            e_tag: None,
            version: None,
        };
        CachedFileMetadataEntry::new(meta, Arc::new(Weighed(bytes)))
    }

    /// The cache returns what it stored, returns the replaced footer on a
    /// second put, and forgets a removed footer.
    ///
    /// # Panics
    /// Panics when a lookup, replacement, or removal is wrong.
    #[test]
    fn footer_cache_round_trips() {
        let cache = FooterCache::new(100);
        let one = Path::from("one.parquet");
        assert!(cache.put(&one, entry(&one, 10)).is_none());
        assert_eq!(cache.get(&one).map(|e| e.size()), Some(10));
        assert_eq!(cache.put(&one, entry(&one, 20)).map(|e| e.size()), Some(10));
        assert_eq!(cache.remove(&one).map(|e| e.size()), Some(20));
        assert!(!cache.contains_key(&one));
    }

    /// Every lookup counts as a hit or a miss.
    ///
    /// # Panics
    /// Panics when a lookup is not counted in its counter.
    #[test]
    fn footer_cache_counts_hits_and_misses() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        metrics::with_local_recorder(&recorder, || {
            let cache = FooterCache::new(100);
            let one = Path::from("one.parquet");
            assert!(cache.get(&one).is_none());
            cache.put(&one, entry(&one, 10));
            assert!(cache.get(&one).is_some());
            assert!(cache.get(&one).is_some());
        });
        let counters = recorder.snapshot().counters;

        assert_eq!(counters.get("bifrost_footer_cache_hits_total"), Some(&2));
        assert_eq!(counters.get("bifrost_footer_cache_misses_total"), Some(&1));
    }
}
