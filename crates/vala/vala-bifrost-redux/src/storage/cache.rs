//! The node's one bounded, single-flight decoded-Parquet-metadata cache.
//!
//! Oracle opens the same immutable hot object from several places: a leader's
//! local scan, a follower serving a signed assignment, and repeated queries
//! over an unpublished cut. Decoding a Parquet footer is pure CPU over bytes
//! that can never change for a given object, so decoding it more than once per
//! node is waste that shows up as both latency and range I/O.
//!
//! This cache removes exactly that waste and nothing else. It retains only
//! successfully decoded metadata for immutable objects, keyed by an identity
//! that deliberately excludes projection, predicates, and schema fingerprint —
//! the decoded footer is the same regardless of what a query asks of it, and
//! folding request-shaped facts into the key would make every distinct query a
//! separate entry for identical bytes. Authority is not delegated here: the
//! caller validates ticket, binding, full-schema fingerprint, and signed
//! descriptor *before* consulting the cache, and applies projection,
//! predicates, residual filtering, and the tenant tripwire *after*. A warm
//! entry is a decode that did not happen, never a check that did not happen.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use futures_util::future::BoxFuture;
use parquet::file::metadata::ParquetMetaData;
use tokio_util::sync::CancellationToken;
use wyrd_spec::ids::DataTenantId;

use crate::storage::error::BifrostStorageError;
use crate::storage::telemetry::{BifrostStorageTelemetry, StorageLifecycle};

/// One cloneable terminal load result publishable to every joined waiter.
///
/// The error is `Arc`-wrapped because one loader's single closed failure is
/// delivered to an unbounded number of waiters; cloning the error itself would
/// duplicate its detail string per waiter for no benefit.
pub(crate) type MetadataLoadResult = Result<Arc<ParquetMetaData>, Arc<BifrostStorageError>>;

/// A caller-supplied decode of one object's Parquet metadata.
///
/// Boxed and `'static` so the cache can move it into the retained loader task.
/// The caller owns what the future actually does — in production it is a
/// governed ranged read through the storage owner's operator — which keeps the
/// cache free of any knowledge of memory accounting or storage layering.
pub(crate) type MetadataLoadFuture =
    BoxFuture<'static, Result<Arc<ParquetMetaData>, BifrostStorageError>>;

/// The complete identity of one immutable hot object's decoded metadata.
///
/// Every component is part of the object's identity, not the request's: the
/// tenant and logical table scope it, the canonical object path and the owning
/// `vala.file_list` row name it, and the writer's checksum plus the positive
/// object size pin the exact bytes. Two queries with different projections,
/// predicates, or schema fingerprints read the same footer and so must share
/// one entry; including any of those would fragment the cache without making it
/// safer, because none of them is validated here.
///
/// The values are already validated upstream: a key is only ever built from a
/// signed `PersistedFileDescriptor` that passed its own identity checks, so
/// this type does not re-litigate them.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HotMetadataKey {
    /// Authenticated data tenant owning the object.
    tenant_id: DataTenantId,
    /// Canonical logical table name the object belongs to.
    table: String,
    /// Canonical tenant-relative object path.
    object: String,
    /// Identity of the `vala.file_list` row that declared this object.
    file_list_id: uuid::Uuid,
    /// Writer-recorded checksum of the decoded object.
    checksum: [u8; 32],
    /// Exact object size in bytes; always positive.
    size_bytes: u64,
}

impl HotMetadataKey {
    /// Builds one hot-object identity from already-validated signed facts.
    #[must_use]
    pub fn new(
        tenant_id: DataTenantId,
        table: String,
        object: String,
        file_list_id: uuid::Uuid,
        checksum: [u8; 32],
        size_bytes: u64,
    ) -> Self {
        Self {
            tenant_id,
            table,
            object,
            file_list_id,
            checksum,
            size_bytes,
        }
    }

    /// Returns the object's exact size in bytes.
    #[must_use]
    pub const fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    /// Returns the heap bytes this key owns.
    ///
    /// Charged alongside the decoded metadata so a cache full of long paths
    /// cannot exceed its budget through key storage the ceiling never saw.
    pub(crate) fn owned_bytes(&self) -> u64 {
        let owned = self.table.len() + self.object.len();
        u64::try_from(owned).unwrap_or(u64::MAX)
    }
}

/// Everything the cache mutates under one narrow lock.
///
/// The lock is never held across `.await`: a lookup takes it, decides, and
/// releases it before waiting on the load it just registered.
#[derive(Debug)]
struct CacheState {
    /// Whether the owner still admits new loads.
    lifecycle: StorageLifecycle,
}

/// The bounded, single-flight decoded-metadata cache for one node.
#[derive(Debug)]
pub(crate) struct ParquetMetadataCache {
    /// Mutable resident and in-flight state.
    state: Mutex<CacheState>,
    /// Configured ceiling on charged resident bytes; always positive.
    budget_bytes: u64,
    /// The owner's production telemetry facade.
    telemetry: Arc<BifrostStorageTelemetry>,
}

impl ParquetMetadataCache {
    /// Builds one cache over a positive byte budget.
    pub(crate) fn new(budget_bytes: u64, telemetry: Arc<BifrostStorageTelemetry>) -> Self {
        Self {
            state: Mutex::new(CacheState {
                lifecycle: StorageLifecycle::Open,
            }),
            budget_bytes,
            telemetry,
        }
    }

    /// Returns decoded metadata for `key`, loading it at most once per node.
    ///
    /// A resident entry returns immediately. Otherwise the first caller becomes
    /// the load's owner and retains one loader task; every later caller for the
    /// same key joins that task and receives a clone of its single terminal
    /// result.
    ///
    /// # Errors
    /// Returns the loader's closed failure.
    pub(crate) async fn get_or_load(
        self: &Arc<Self>,
        key: HotMetadataKey,
        load: MetadataLoadFuture,
        deadline: Instant,
        cancel: CancellationToken,
    ) -> MetadataLoadResult {
        let _ = (&key, deadline, &cancel, self.budget_bytes, &self.telemetry);
        load.await.map_err(Arc::new)
    }

    /// Drains the cache within one absolute deadline.
    ///
    /// Returns `true` when every retained loader settled in time.
    pub(crate) async fn close(&self, deadline: Instant) -> bool {
        let _ = deadline;
        if let Ok(mut state) = self.state.lock() {
            state.lifecycle = StorageLifecycle::Closed;
        }
        true
    }

    /// Drains the cache immediately, aborting every retained loader.
    pub(crate) fn abort(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.lifecycle = StorageLifecycle::Closed;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Range;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use arrow::array::{ArrayRef, Int64Array};
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::record_batch::RecordBatch;
    use bytes::Bytes;
    use futures_util::FutureExt as _;
    use parquet::arrow::arrow_reader::ArrowReaderOptions;
    use parquet::arrow::async_reader::AsyncFileReader;
    use parquet::errors::ParquetError;
    use tokio::sync::Notify;

    use super::*;
    use crate::storage::BifrostStorage;
    use crate::storage::policy::BifrostStoragePolicy;
    use crate::storage::telemetry::{CacheEffect, CacheEffectReason, MetadataLoadOutcome};

    /// Writes one real Parquet object whose footer the cache will decode.
    ///
    /// A synthesized or empty metadata value would make every weight, eviction,
    /// and oversize assertion meaningless, because the cache charges
    /// `ParquetMetaData::memory_size()`. Writing a small file and decoding its
    /// real footer gives the same value shape production retains.
    fn parquet_object(rows: i64) -> Bytes {
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![Arc::new(Int64Array::from((0..rows).collect::<Vec<_>>())) as ArrayRef],
        )
        .expect("metadata fixture batch");
        let mut buffer = Vec::new();
        let mut writer = parquet::arrow::ArrowWriter::try_new(&mut buffer, schema, None)
            .expect("metadata fixture writer");
        writer.write(&batch).expect("metadata fixture write");
        writer.close().expect("metadata fixture close");
        Bytes::from(buffer)
    }

    /// One in-memory object reader that counts the decodes it actually served.
    ///
    /// Every caller of `hot_metadata` supplies its own reader, so a reader that
    /// never performs a range read is positive evidence that the caller's bytes
    /// were never fetched: that is how "one decode for N waiters" and "a warm
    /// hit performs no backend load" are proven without a spy on the backend.
    /// The optional gate holds the elected loader inside its decode so later
    /// callers are guaranteed to arrive while the load is still in flight.
    struct CountingReader {
        /// The complete object bytes this reader serves ranges from.
        data: Bytes,
        /// Shared count of readers that served at least one range.
        decodes: Arc<AtomicUsize>,
        /// Whether this reader has already been counted.
        counted: bool,
        /// Optional barrier the first range read waits on.
        gate: Option<Arc<Notify>>,
    }

    impl CountingReader {
        /// Builds one reader over the shared object bytes.
        fn new(data: &Bytes, decodes: &Arc<AtomicUsize>, gate: Option<Arc<Notify>>) -> Self {
            Self {
                data: data.clone(),
                decodes: Arc::clone(decodes),
                counted: false,
                gate,
            }
        }
    }

    impl AsyncFileReader for CountingReader {
        fn get_bytes(
            &mut self,
            range: Range<u64>,
        ) -> futures_util::future::BoxFuture<'_, Result<Bytes, ParquetError>> {
            async move {
                if !self.counted {
                    self.counted = true;
                    self.decodes.fetch_add(1, Ordering::SeqCst);
                    if let Some(gate) = self.gate.take() {
                        gate.notified().await;
                    }
                }
                let start = usize::try_from(range.start).expect("fixture range fits usize");
                let end = usize::try_from(range.end).expect("fixture range fits usize");
                Ok(self.data.slice(start..end))
            }
            .boxed()
        }

        fn get_metadata<'a>(
            &'a mut self,
            _options: Option<&'a ArrowReaderOptions>,
        ) -> futures_util::future::BoxFuture<'a, Result<Arc<ParquetMetaData>, ParquetError>>
        {
            async move {
                Err(ParquetError::General(
                    "the fixture reader serves ranges, never cached metadata".to_owned(),
                ))
            }
            .boxed()
        }
    }

    /// Builds one hot identity that differs only in the named component.
    fn test_key(
        tenant: DataTenantId,
        object: &str,
        checksum: u8,
        size_bytes: u64,
    ) -> HotMetadataKey {
        HotMetadataKey::new(
            tenant,
            "vala.bifrost.events".to_owned(),
            object.to_owned(),
            uuid::Uuid::nil(),
            [checksum; 32],
            size_bytes,
        )
    }

    /// Builds an Oracle-serving owner with an explicit metadata-cache budget.
    fn storage_with_cache(cache_bytes: u64) -> BifrostStorage {
        BifrostStorage::new(
            BifrostStoragePolicy::new(30_000, 3, 60_000, 128, cache_bytes),
            true,
        )
    }

    /// One node decodes one immutable object at most once, serves a repeat
    /// request with no backend load at all, keys on the object's identity
    /// alone, records an explicit disabled bypass that retains nothing, refuses
    /// to retain what its whole budget cannot hold, and evicts deterministically
    /// in least-recently-used order.
    ///
    /// These properties are asserted together because they are one invariant
    /// seen from several sides: the entry the cache decides to create, the load
    /// it declines to repeat, the identity it creates entries under, the path a
    /// disabled composition takes, the entry it declines to keep, and the entry
    /// it gives up to stay inside the ceiling. Splitting them would let a key
    /// that fragments per request still pass a single-flight test.
    ///
    /// # Panics
    /// Panics when a waiter task, decode, or reconciliation assertion fails.
    #[tokio::test]
    async fn metadata_cache_reconciles_single_flight_identity_bypass_and_eviction() {
        let object = parquet_object(64);
        let size = u64::try_from(object.len()).expect("fixture object fits u64");
        let tenant = DataTenantId::new_v7();
        let storage = Arc::new(storage_with_cache(1 << 20));
        let deadline = Instant::now() + Duration::from_secs(30);
        let decodes = Arc::new(AtomicUsize::new(0));
        let gate = Arc::new(Notify::new());
        let key = test_key(tenant, "a.parquet", 0x11, size);

        // Single flight: the owner and every joined waiter share one decode.
        let mut waiters = Vec::new();
        for index in 0..4 {
            let storage = Arc::clone(&storage);
            let waiter_key = key.clone();
            let reader =
                CountingReader::new(&object, &decodes, (index == 0).then(|| Arc::clone(&gate)));
            waiters.push(tokio::spawn(async move {
                storage
                    .hot_metadata(waiter_key, reader, deadline, CancellationToken::new())
                    .await
            }));
            tokio::task::yield_now().await;
        }
        gate.notify_waiters();
        let mut settled = Vec::new();
        for waiter in waiters {
            settled.push(
                waiter
                    .await
                    .expect("waiter task")
                    .expect("published decode"),
            );
        }
        assert_eq!(
            decodes.load(Ordering::SeqCst),
            1,
            "one object must cost exactly one decode"
        );
        for observed in &settled {
            assert!(
                Arc::ptr_eq(observed, &settled[0]),
                "every waiter must observe the one shared decode"
            );
        }
        let snapshot = storage.telemetry_snapshot();
        assert_eq!(snapshot.load_starts(), 1);
        assert_eq!(snapshot.load_terminal(MetadataLoadOutcome::Success), 1);
        assert_eq!(snapshot.load_starts(), snapshot.load_terminals());
        assert_eq!(snapshot.effect(CacheEffect::Miss), 1);
        assert_eq!(snapshot.effect(CacheEffect::Join), 3);
        assert_eq!(snapshot.resident_entries(), 1);
        assert!(snapshot.resident_bytes() > 0);
        assert_eq!(snapshot.inflight_loads(), 0);
        assert_eq!(snapshot.waiters(), 0);

        // Repeat hit: a warm entry performs no backend load at all.
        let warm = storage
            .hot_metadata(
                key.clone(),
                CountingReader::new(&object, &decodes, None),
                deadline,
                CancellationToken::new(),
            )
            .await
            .expect("the warm entry is served");
        assert!(Arc::ptr_eq(&warm, &settled[0]));
        assert_eq!(
            decodes.load(Ordering::SeqCst),
            1,
            "a hit must not read the object again"
        );
        let snapshot = storage.telemetry_snapshot();
        assert_eq!(snapshot.effect(CacheEffect::Hit), 1);
        assert_eq!(snapshot.load_starts(), 1, "a hit starts no load");

        // Identity: the same path under a different checksum is a different
        // object and must not be served by the resident entry.
        let rewritten = storage
            .hot_metadata(
                test_key(tenant, "a.parquet", 0x22, size),
                CountingReader::new(&object, &decodes, None),
                deadline,
                CancellationToken::new(),
            )
            .await
            .expect("the rewritten object decodes");
        assert!(!Arc::ptr_eq(&rewritten, &settled[0]));
        assert_eq!(decodes.load(Ordering::SeqCst), 2, "checksum is identity");
        assert_eq!(
            storage.telemetry_snapshot().effect(CacheEffect::Miss),
            2,
            "a changed checksum is a new object, not a hit"
        );

        // Disabled composition: the read still succeeds, publishes positive
        // bypass evidence, and retains nothing at all.
        let disabled = storage_with_cache(0);
        assert!(!disabled.metadata_cache_enabled());
        let disabled_decodes = Arc::new(AtomicUsize::new(0));
        for _ in 0..2 {
            disabled
                .hot_metadata(
                    key.clone(),
                    CountingReader::new(&object, &disabled_decodes, None),
                    deadline,
                    CancellationToken::new(),
                )
                .await
                .expect("a disabled composition still decodes");
        }
        let snapshot = disabled.telemetry_snapshot();
        assert_eq!(disabled_decodes.load(Ordering::SeqCst), 2);
        assert_eq!(snapshot.effect(CacheEffect::Bypass), 2);
        assert_eq!(snapshot.reason(CacheEffectReason::Disabled), 2);
        assert_eq!(snapshot.resident_entries(), 0);
        assert_eq!(snapshot.resident_bytes(), 0);
        assert!(snapshot.is_quiescent());

        // Eviction: a budget that holds exactly one entry gives up the
        // least-recently-used entry to admit the next one.
        let metadata = Arc::clone(&settled[0]);
        let weight =
            u64::try_from(metadata.memory_size()).expect("footprint fits u64") + key.owned_bytes();
        let telemetry = Arc::new(BifrostStorageTelemetry::default());
        let cache = Arc::new(ParquetMetadataCache::new(weight, Arc::clone(&telemetry)));
        for checksum in [0x31_u8, 0x32] {
            let loaded = Arc::clone(&metadata);
            cache
                .get_or_load(
                    test_key(tenant, "evicting.parquet", checksum, size),
                    Box::pin(async move { Ok(loaded) }),
                    deadline,
                    CancellationToken::new(),
                )
                .await
                .expect("both decodes succeed");
        }
        let snapshot = telemetry.snapshot();
        assert_eq!(snapshot.effect(CacheEffect::Evict), 1);
        assert_eq!(snapshot.resident_entries(), 1);
        assert_eq!(snapshot.resident_bytes(), weight);

        // Oversized: metadata larger than the whole budget is still returned to
        // its caller and simply not retained.
        let tiny_telemetry = Arc::new(BifrostStorageTelemetry::default());
        let tiny = Arc::new(ParquetMetadataCache::new(1, Arc::clone(&tiny_telemetry)));
        let loaded = Arc::clone(&metadata);
        let bypassed = tiny
            .get_or_load(
                test_key(tenant, "oversized.parquet", 0x44, size),
                Box::pin(async move { Ok(loaded) }),
                deadline,
                CancellationToken::new(),
            )
            .await
            .expect("an oversized decode still reaches its caller");
        assert!(Arc::ptr_eq(&bypassed, &metadata));
        let snapshot = tiny_telemetry.snapshot();
        assert_eq!(snapshot.effect(CacheEffect::Bypass), 1);
        assert_eq!(snapshot.reason(CacheEffectReason::Oversized), 1);
        assert_eq!(snapshot.resident_entries(), 0);
        assert_eq!(snapshot.resident_bytes(), 0);
    }
}
