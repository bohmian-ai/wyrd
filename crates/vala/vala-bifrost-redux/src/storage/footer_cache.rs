//! The node's concurrent Parquet footer cache.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use datafusion::common::{HashMap, TableReference};
use datafusion::datasource::physical_plan::parquet::metadata::CachedParquetMetaData;
use datafusion::execution::cache::cache_manager::{CachedFileMetadataEntry, FileMetadata};
use datafusion::execution::cache::{Cache, CacheEntryInfo, CacheValue as _};
use datafusion::parquet::file::metadata::ParquetMetaData;
use object_store::path::Path;

use crate::resources::{MetadataReservation, OracleMetadataResources};

/// `DataFusion`'s file metadata cache, served without a global lock.
///
/// `DataFusion`'s `DefaultCache` keeps its LRU behind one `Mutex`, and a hit
/// reorders the LRU, so every footer lookup of every concurrent query
/// serializes on that lock. This cache is `mini_moka`'s concurrent cache
/// instead: lookups read a sharded map and record recency in a buffer the
/// cache drains in batches. Entries are weighed by
/// [`CachedFileMetadataEntry::size`] against a byte limit, a ceiling rather
/// than a reservation.
///
/// Each cached footer is charged to the Oracle memory root at its size and
/// stays charged until no one holds it. `DataFusion` copies the decoded
/// `ParquetMetaData` handle out of an entry for the life of a scan, so the
/// charge is tied to that handle rather than to the cache entry: an evicted
/// footer a scan still reads stays charged until the scan lets it go.
pub(crate) struct FooterCache {
    /// Footers by object path, weighed in bytes.
    entries: mini_moka::sync::Cache<Path, CachedFileMetadataEntry>,
    /// The byte limit the cache was built with.
    limit: usize,
    /// The Oracle memory root every cached footer is charged to.
    resources: OracleMetadataResources,
    /// Every footer this cache charged and the handle that keeps it alive.
    charged: Mutex<ChargedFooters>,
}

/// The charged footers and when they are next swept for released ones.
struct ChargedFooters {
    /// Footers in charge order.
    footers: Vec<ChargedFooter>,
    /// The list length that triggers the next sweep.
    sweep_at: usize,
}

/// The list length of the first amortized sweep.
const FIRST_SWEEP: usize = 64;

/// One cached footer's charge and the handle whose holders own its bytes.
struct ChargedFooter {
    /// The handle scans and the cache share.
    held: FooterHandle,
    /// Ownership of the footer's bytes on the Oracle memory root.
    _reservation: MetadataReservation,
}

/// The shared handle of a cached footer's decoded bytes.
enum FooterHandle {
    /// A Parquet footer, held by scans through its `ParquetMetaData`.
    Parquet(Arc<ParquetMetaData>),
    /// Any other metadata, held through its cache value.
    Other(Arc<dyn FileMetadata>),
}

impl FooterHandle {
    /// The handle of `entry`'s decoded bytes.
    fn of(entry: &CachedFileMetadataEntry) -> Self {
        entry
            .file_metadata
            .as_any()
            .downcast_ref::<CachedParquetMetaData>()
            .map_or_else(
                || Self::Other(Arc::clone(&entry.file_metadata)),
                |parquet| Self::Parquet(Arc::clone(parquet.parquet_metadata())),
            )
    }

    /// Whether only the charged list still holds the bytes.
    fn released(&self) -> bool {
        match self {
            Self::Parquet(metadata) => Arc::strong_count(metadata) == 1,
            Self::Other(metadata) => Arc::strong_count(metadata) == 1,
        }
    }
}

impl FooterCache {
    /// Builds an empty cache holding at most `limit` bytes of footers, each
    /// charged to `resources` while anyone holds it.
    ///
    /// A footer larger than `u32::MAX` bytes weighs `u32::MAX`, the most the
    /// cache can weigh one entry.
    pub(crate) fn new(limit: usize, resources: OracleMetadataResources) -> Self {
        let entries = mini_moka::sync::Cache::builder()
            .max_capacity(u64::try_from(limit).unwrap_or(u64::MAX))
            .weigher(|_, entry: &CachedFileMetadataEntry| {
                u32::try_from(entry.size()).unwrap_or(u32::MAX)
            })
            .build();
        Self {
            entries,
            limit,
            resources,
            charged: Mutex::new(ChargedFooters {
                footers: Vec::new(),
                sweep_at: FIRST_SWEEP,
            }),
        }
    }

    /// Returns the charge of every footer no one holds any more.
    fn release_unheld(&self) {
        if let Ok(mut charged) = self.charged.lock() {
            self.sweep(&mut charged);
        }
    }

    /// Applies the cache's pending evictions, so evicted entries let go of
    /// their footers, then drops the charge of every footer no one holds.
    ///
    /// Puts sweep only when the list has doubled since the last sweep, or
    /// before refusing a charge, so a put costs amortized constant time and
    /// lookups never take this lock. A released footer stays charged until
    /// the next sweep, which errs toward refusing growth, never toward a
    /// double promise.
    fn sweep(&self, charged: &mut ChargedFooters) {
        mini_moka::sync::ConcurrentCacheExt::sync(&self.entries);
        charged.footers.retain(|footer| !footer.held.released());
        charged.sweep_at = (charged.footers.len() * 2).max(FIRST_SWEEP);
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

    /// Caches `value` under `key`, charged at its size, and returns the
    /// footer it replaced.
    ///
    /// When the Oracle memory root cannot cover the footer even after the
    /// cache returns the charge of footers no one holds, it is not cached, so
    /// cache growth never displaces active work; the refusal counts in
    /// `bifrost_footer_cache_refusals_total`. The read and the insert are
    /// separate operations, so a racing insert of the same key may be the one
    /// returned or the one replaced.
    fn put(&self, key: &Path, value: CachedFileMetadataEntry) -> Option<CachedFileMetadataEntry> {
        let size = value.size();
        let reservation = self.resources.try_reserve_metadata(size).or_else(|_| {
            self.release_unheld();
            self.resources.try_reserve_metadata(size)
        });
        let Ok(reservation) = reservation else {
            metrics::counter!("bifrost_footer_cache_refusals_total").increment(1);
            return None;
        };
        let Ok(mut charged) = self.charged.lock() else {
            return None;
        };
        if charged.footers.len() >= charged.sweep_at {
            self.sweep(&mut charged);
        }
        charged.footers.push(ChargedFooter {
            held: FooterHandle::of(&value),
            _reservation: reservation,
        });
        drop(charged);
        let previous = self.entries.get(key);
        self.entries.insert(key.clone(), value);
        previous
    }

    /// Evicts `key` and returns the footer it held; its bytes stay charged
    /// while the returned footer or a scan still holds them, and return at
    /// the next sweep after that.
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

    /// Evicts every footer, returning the charge of each no one holds.
    fn clear(&self) {
        self.entries.invalidate_all();
        self.release_unheld();
    }

    /// Names the cache in `DataFusion` diagnostics.
    fn name(&self) -> String {
        "BifrostFooterCache".to_owned()
    }

    /// Returns the byte limit the cache was built with.
    fn cache_limit(&self) -> usize {
        self.limit
    }

    /// Keeps the construction limit, the configured ceiling: no Wyrd path
    /// hands this cache to a `DataFusion` `CacheManager` that would resize it.
    fn update_cache_limit(&self, limit: usize) {
        if limit != self.limit {
            tracing::warn!(
                limit,
                kept = self.limit,
                "footer cache keeps its configured byte limit"
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
        let cache = FooterCache::new(
            100,
            crate::resources::OracleResources::for_test().metadata(),
        );
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
            let cache = FooterCache::new(
                100,
                crate::resources::OracleResources::for_test().metadata(),
            );
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

    /// A decoded Parquet footer a scan holds stays charged after eviction
    /// until the scan drops it, and a footer the root cannot cover is not
    /// cached.
    ///
    /// # Panics
    /// Panics when eviction returns held bytes, the last holder keeps them,
    /// or an uncovered footer is cached.
    #[test]
    fn footer_cache_charges_held_parquet_footers_until_released() {
        use datafusion::parquet::arrow::ArrowWriter;
        use datafusion::parquet::file::metadata::ParquetMetaDataReader;

        let schema = Arc::new(arrow::datatypes::Schema::new(vec![
            arrow::datatypes::Field::new("v", arrow::datatypes::DataType::Int64, false),
        ]));
        let batch = arrow::record_batch::RecordBatch::try_new(
            Arc::clone(&schema),
            vec![Arc::new(arrow::array::Int64Array::from(vec![1, 2, 3]))],
        )
        .expect("batch");
        let mut file = Vec::new();
        let mut writer = ArrowWriter::try_new(&mut file, schema, None).expect("writer");
        writer.write(&batch).expect("write");
        writer.close().expect("close");
        let decoded = Arc::new(
            ParquetMetaDataReader::new()
                .parse_and_finish(&bytes::Bytes::from(file))
                .expect("footer"),
        );

        let oracle = crate::resources::OracleResources::for_test();
        let used = || {
            oracle
                .snapshot()
                .expect("snapshot")
                .oracle_memory_used_bytes
        };
        let idle = used();
        let cache = FooterCache::new(1 << 20, oracle.metadata());
        let path = Path::from("tenant/spans/a.parquet");
        let mut parquet = entry(&path, 0);
        parquet.file_metadata = Arc::new(CachedParquetMetaData::new(decoded));
        let weight = parquet.size();
        assert!(weight > 0);
        cache.put(&path, parquet);
        assert_eq!(used(), idle + weight);

        let scan = cache
            .get(&path)
            .and_then(|entry| {
                entry
                    .file_metadata
                    .as_any()
                    .downcast_ref::<CachedParquetMetaData>()
                    .map(|cached| Arc::clone(cached.parquet_metadata()))
            })
            .expect("a scan holds the decoded footer");
        drop(cache.remove(&path));
        assert_eq!(used(), idle + weight, "a held evicted footer stays charged");
        drop(scan);
        cache.clear();
        assert_eq!(used(), idle, "the last holder returns the bytes");

        assert!(cache.put(&path, entry(&path, 2 << 30)).is_none());
        assert!(
            !cache.contains_key(&path),
            "an uncovered footer is not cached"
        );
        assert_eq!(used(), idle);
    }
}
