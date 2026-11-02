//! The node's Parquet byte-range cache.

use std::ops::Range;

use bytes::Bytes;
use object_store::path::Path;

/// Bytes Oracle scans read from warehouse Parquet objects, served again
/// without a store read.
///
/// Warehouse objects are written once under unique names and never
/// rewritten, so a range read from one stays correct for as long as it is
/// cached. Each entry is one exact range a scan asked for: repeated queries
/// read the same column chunks and pages, so the same ranges recur. Entries
/// are weighed by their bytes against a byte limit fixed at construction,
/// because [`super::BifrostStorage::with_oracle_caches`] charges that whole
/// limit to the Oracle memory root up front. Lookups read a sharded map, so
/// concurrent scans never serialize on the cache.
pub(crate) struct RangeCache {
    /// Ranges by object path and byte range, weighed in bytes.
    entries: mini_moka::sync::Cache<(Path, Range<u64>), Bytes>,
}

impl RangeCache {
    /// Builds an empty cache holding at most `limit` bytes of ranges.
    ///
    /// A range larger than `u32::MAX` bytes weighs `u32::MAX`, the most the
    /// cache can weigh one entry.
    pub(crate) fn new(limit: usize) -> Self {
        let entries = mini_moka::sync::Cache::builder()
            .max_capacity(u64::try_from(limit).unwrap_or(u64::MAX))
            .weigher(|_, bytes: &Bytes| u32::try_from(bytes.len()).unwrap_or(u32::MAX))
            .build();
        Self { entries }
    }

    /// Returns the cached bytes of `range` in `path`.
    ///
    /// Each lookup counts in `bifrost_range_cache_hits_total` or
    /// `bifrost_range_cache_misses_total`.
    pub(crate) fn get(&self, path: &Path, range: &Range<u64>) -> Option<Bytes> {
        let bytes = self.entries.get(&(path.clone(), range.clone()));
        let counter = if bytes.is_some() {
            "bifrost_range_cache_hits_total"
        } else {
            "bifrost_range_cache_misses_total"
        };
        metrics::counter!(counter).increment(1);
        bytes
    }

    /// Caches a copy of `bytes` as `range` of `path`.
    ///
    /// The copy is what makes an entry's weight its true size: a range a
    /// store returns may be a slice of a larger coalesced read, which the
    /// slice would otherwise keep alive.
    pub(crate) fn put(&self, path: &Path, range: Range<u64>, bytes: &[u8]) {
        self.entries
            .insert((path.clone(), range), Bytes::copy_from_slice(bytes));
    }
}

impl std::fmt::Debug for RangeCache {
    /// Shows the cache's size, never its bytes.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RangeCache")
            .field("entries", &self.entries.entry_count())
            .field("bytes", &self.entries.weighted_size())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Proves a cached range is served back as written and other ranges or
    /// objects miss.
    ///
    /// # Panics
    ///
    /// Panics when a cached range is not returned or a different key hits.
    #[test]
    fn range_cache_serves_exactly_the_cached_range() {
        let cache = RangeCache::new(1 << 20);
        let path = Path::from("tenant/spans/a.parquet");
        cache.put(&path, 4..8, b"abcd");
        assert_eq!(cache.get(&path, &(4..8)).as_deref(), Some(&b"abcd"[..]));
        assert_eq!(cache.get(&path, &(4..9)), None);
        assert_eq!(
            cache.get(&Path::from("tenant/spans/b.parquet"), &(4..8)),
            None
        );
    }

    /// Proves the cache holds no more than its byte limit.
    ///
    /// # Panics
    ///
    /// Panics when the cached bytes exceed the limit after eviction runs.
    #[test]
    fn range_cache_evicts_past_its_byte_limit() {
        use mini_moka::sync::ConcurrentCacheExt as _;

        let cache = RangeCache::new(1_024);
        let path = Path::from("tenant/spans/a.parquet");
        for start in 0..8_u64 {
            cache.put(&path, start * 512..(start + 1) * 512, &[0; 512]);
        }
        cache.entries.sync();
        assert!(cache.entries.weighted_size() <= 1_024);
    }
}
