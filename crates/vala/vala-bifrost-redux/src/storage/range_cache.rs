//! The node's Parquet byte-range cache.

use std::ops::Range;

use bytes::Bytes;
use object_store::path::Path;

use crate::resources::{MetadataReservation, OracleMetadataResources};

/// Bytes Oracle scans read from warehouse Parquet objects, served again
/// without a store read.
///
/// Warehouse objects are written once under unique names and never
/// rewritten, so a range read from one stays correct for as long as it is
/// cached. Each entry is one exact range a scan asked for: repeated queries
/// read the same column chunks and pages, so the same ranges recur. Entries
/// are weighed by their bytes against a byte limit, a ceiling rather than a
/// reservation. Each entry's allocation carries its own charge on the Oracle
/// memory root, so the cache withholds nothing while empty and an evicted
/// range a scan still holds stays charged until its last reference drops.
/// Lookups read a sharded map, so concurrent scans never serialize on the
/// cache.
pub(crate) struct RangeCache {
    /// Ranges by object path and byte range, weighed in bytes.
    entries: mini_moka::sync::Cache<(Path, Range<u64>), Bytes>,
    /// The Oracle memory root every cached allocation is charged to.
    resources: OracleMetadataResources,
}

/// One cached range's allocation and the charge it owns.
struct ChargedRange {
    /// The copied range.
    bytes: Vec<u8>,
    /// Ownership of `bytes` on the Oracle memory root.
    _reservation: MetadataReservation,
}

impl AsRef<[u8]> for ChargedRange {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl RangeCache {
    /// Builds an empty cache holding at most `limit` bytes of ranges, each
    /// charged to `resources` while it is retained.
    ///
    /// A range larger than `u32::MAX` bytes weighs `u32::MAX`, the most the
    /// cache can weigh one entry.
    pub(crate) fn new(limit: usize, resources: OracleMetadataResources) -> Self {
        let entries = mini_moka::sync::Cache::builder()
            .max_capacity(u64::try_from(limit).unwrap_or(u64::MAX))
            .weigher(|_, bytes: &Bytes| u32::try_from(bytes.len()).unwrap_or(u32::MAX))
            .build();
        Self { entries, resources }
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

    /// Caches a charged copy of `bytes` as `range` of `path`.
    ///
    /// The copy is what makes an entry's weight and charge its true size: a
    /// range a store returns may be a slice of a larger coalesced read, which
    /// the slice would otherwise keep alive. When the Oracle memory root
    /// cannot cover the copy, the range is not cached, so cache growth never
    /// displaces active work; the refusal counts in
    /// `bifrost_range_cache_refusals_total`.
    pub(crate) fn put(&self, path: &Path, range: Range<u64>, bytes: &[u8]) {
        let Ok(reservation) = self.resources.try_reserve_metadata(bytes.len()) else {
            metrics::counter!("bifrost_range_cache_refusals_total").increment(1);
            return;
        };
        let charged = ChargedRange {
            bytes: bytes.to_vec(),
            _reservation: reservation,
        };
        self.entries
            .insert((path.clone(), range), Bytes::from_owner(charged));
    }
}

impl std::fmt::Debug for RangeCache {
    /// Shows the cache's size, never its bytes.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RangeCache")
            .field("entries", &self.entries.entry_count())
            .field("bytes", &self.entries.weighted_size())
            .finish_non_exhaustive()
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
        let cache = RangeCache::new(
            1 << 20,
            crate::resources::OracleResources::for_test().metadata(),
        );
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

        let cache = RangeCache::new(
            1_024,
            crate::resources::OracleResources::for_test().metadata(),
        );
        let path = Path::from("tenant/spans/a.parquet");
        for start in 0..8_u64 {
            cache.put(&path, start * 512..(start + 1) * 512, &[0; 512]);
        }
        cache.entries.sync();
        assert!(cache.entries.weighted_size() <= 1_024);
    }

    /// An evicted range a scan still holds stays charged until its last
    /// reference drops, and a range the root cannot cover is not cached.
    ///
    /// # Panics
    ///
    /// Panics when eviction returns referenced bytes, the last reference
    /// keeps them, or an uncovered range is cached.
    #[test]
    fn range_cache_charges_retained_bytes_until_released() {
        use mini_moka::sync::ConcurrentCacheExt as _;

        let oracle = crate::resources::OracleResources::for_test();
        let used = || {
            oracle
                .snapshot()
                .expect("snapshot")
                .oracle_memory_used_bytes
        };
        let idle = used();
        let cache = RangeCache::new(1 << 20, oracle.metadata());
        let path = Path::from("tenant/spans/a.parquet");
        cache.put(&path, 0..4_096, &[1; 4_096]);
        assert_eq!(used(), idle + 4_096);
        let held = cache.get(&path, &(0..4_096)).expect("the range is cached");
        cache.entries.invalidate(&(path.clone(), 0..4_096));
        cache.entries.sync();
        assert_eq!(used(), idle + 4_096, "a held evicted range stays charged");
        drop(held);
        assert_eq!(used(), idle, "the last reference returns the bytes");

        cache.put(&path, 0..2, &vec![0; 2 << 30]);
        assert!(
            cache.get(&path, &(0..2)).is_none(),
            "an uncovered range is not cached"
        );
        assert_eq!(used(), idle);
    }
}
