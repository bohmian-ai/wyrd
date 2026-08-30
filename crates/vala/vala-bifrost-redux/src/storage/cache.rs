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

//! Concurrency is single-flight: the first caller for a key owns one retained
//! loader task and every later caller joins it, so N concurrent openings of one
//! object cost one decode. The state mutex is never held across `.await`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::future::BoxFuture;
use lru::LruCache;
use parquet::file::metadata::ParquetMetaData;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use wyrd_spec::ids::DataTenantId;

use crate::storage::error::BifrostStorageError;
use crate::storage::telemetry::{
    BifrostStorageTelemetry, CacheEffect, CacheEffectReason, MetadataLoadOutcome, StorageLifecycle,
};

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

/// One retained successful decode and the bytes it is charged.
#[derive(Debug)]
struct CachedMetadata {
    /// Immutable decoded metadata shared with every caller.
    metadata: Arc<ParquetMetaData>,
    /// Charged weight: decoded footprint plus the key's owned bytes.
    weight: u64,
}

/// One load with an owner and zero or more joined waiters.
#[derive(Debug)]
struct InFlight {
    /// Publishes the single terminal result to the owner and every waiter.
    ///
    /// `watch` rather than a broadcast channel because the value is retained:
    /// a waiter that subscribes after the publish still observes the terminal
    /// result instead of missing it or observing lag.
    publisher: watch::Sender<Option<MetadataLoadResult>>,
    /// Cancels this loader's own work without affecting any other key.
    cancel: CancellationToken,
    /// Retained loader task, joined or aborted at owner close.
    task: JoinHandle<()>,
}

/// Everything the cache mutates under one narrow lock.
///
/// The lock is never held across `.await`: a lookup takes it, decides, and
/// releases it before waiting on the load it just registered.
#[derive(Debug)]
struct CacheState {
    /// Successful entries in least-recently-used order.
    ///
    /// Holds no errors and no in-flight state, so its length and charged bytes
    /// are exactly the resident entries a snapshot reports.
    entries: LruCache<HotMetadataKey, CachedMetadata>,
    /// Charged bytes currently resident in `entries`.
    resident_bytes: u64,
    /// Loads with an owner that has not yet published a terminal result.
    inflight: HashMap<HotMetadataKey, InFlight>,
    /// Whether the owner still admits new loads.
    lifecycle: StorageLifecycle,
}

impl CacheState {
    /// Returns the resident entry count and charged bytes as one pair.
    fn resident_totals(&self) -> (u64, u64) {
        (
            u64::try_from(self.entries.len()).unwrap_or(u64::MAX),
            self.resident_bytes,
        )
    }
}

/// What one lookup registration decided for its caller.
enum Registration {
    /// A resident entry satisfied the lookup with no load at all.
    Resident(Arc<ParquetMetaData>),
    /// This caller installed and owns the load.
    Owner(watch::Receiver<Option<MetadataLoadResult>>),
    /// This caller joined an existing owner's load.
    Joined(watch::Receiver<Option<MetadataLoadResult>>),
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
                entries: LruCache::unbounded(),
                resident_bytes: 0,
                inflight: HashMap::new(),
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
    /// Cancelling one waiter detaches only that waiter — the owner's load
    /// continues for everyone else — while owner shutdown cancels the load
    /// itself and publishes one closed terminal to all of them.
    ///
    /// # Errors
    /// Returns the loader's closed failure, [`BifrostStorageError::Cancelled`]
    /// when this caller's token fires, [`BifrostStorageError::Deadline`] when
    /// its deadline elapses first, or [`BifrostStorageError::Closed`] when the
    /// owner no longer admits loads.
    ///
    /// # Panics
    /// Does not panic. A poisoned state lock is surfaced as
    /// [`BifrostStorageError::Closed`] rather than unwound, because a poisoned
    /// cache cannot make a safe retention decision.
    pub(crate) async fn get_or_load(
        self: &Arc<Self>,
        key: HotMetadataKey,
        load: MetadataLoadFuture,
        deadline: Instant,
        cancel: CancellationToken,
    ) -> MetadataLoadResult {
        let started = Instant::now();
        match self.register(&key, load)? {
            Registration::Resident(metadata) => Ok(metadata),
            Registration::Owner(mut receiver) => {
                let result = Self::await_publication(&mut receiver, deadline, &cancel).await;
                self.telemetry
                    .record_waiter_settled(Self::wait_outcome(&result), started.elapsed());
                result
            }
            Registration::Joined(mut receiver) => {
                self.telemetry.record_waiter_joined();
                let result = Self::await_publication(&mut receiver, deadline, &cancel).await;
                self.telemetry
                    .record_waiter_settled(Self::wait_outcome(&result), started.elapsed());
                result
            }
        }
    }

    /// Records the decision for one lookup and installs a loader when needed.
    ///
    /// Everything that touches shared state happens here, under one
    /// acquisition, so a concurrent second caller either sees the resident
    /// entry or the in-flight registration the first caller just installed —
    /// never a window where both start a load.
    ///
    /// # Errors
    /// Returns [`BifrostStorageError::Closed`] when the owner is closing,
    /// closed, or its state lock is poisoned.
    fn register(
        self: &Arc<Self>,
        key: &HotMetadataKey,
        load: MetadataLoadFuture,
    ) -> Result<Registration, Arc<BifrostStorageError>> {
        let Ok(mut state) = self.state.lock() else {
            return Err(Arc::new(BifrostStorageError::Closed));
        };
        if state.lifecycle != StorageLifecycle::Open {
            drop(state);
            self.telemetry
                .record_cache_effect(CacheEffect::Bypass, CacheEffectReason::Closing);
            return Err(Arc::new(BifrostStorageError::Closed));
        }
        if let Some(entry) = state.entries.get(key) {
            let metadata = Arc::clone(&entry.metadata);
            drop(state);
            self.telemetry
                .record_cache_effect(CacheEffect::Hit, CacheEffectReason::None);
            return Ok(Registration::Resident(metadata));
        }
        if let Some(inflight) = state.inflight.get(key) {
            let receiver = inflight.publisher.subscribe();
            drop(state);
            self.telemetry
                .record_cache_effect(CacheEffect::Join, CacheEffectReason::None);
            return Ok(Registration::Joined(receiver));
        }
        let (publisher, receiver) = watch::channel(None);
        let cancel = CancellationToken::new();
        let task = self.spawn_loader(key.clone(), load, publisher.clone(), cancel.clone());
        state.inflight.insert(
            key.clone(),
            InFlight {
                publisher,
                cancel,
                task,
            },
        );
        drop(state);
        self.telemetry
            .record_cache_effect(CacheEffect::Miss, CacheEffectReason::None);
        self.telemetry.record_load_start();
        Ok(Registration::Owner(receiver))
    }

    /// Spawns the one retained loader task for `key`.
    ///
    /// The task, not the calling future, owns the load: that is what lets a
    /// cancelled caller detach without cancelling the work every other waiter
    /// is depending on, and what gives close something concrete to join or
    /// abort.
    fn spawn_loader(
        self: &Arc<Self>,
        key: HotMetadataKey,
        load: MetadataLoadFuture,
        publisher: watch::Sender<Option<MetadataLoadResult>>,
        cancel: CancellationToken,
    ) -> JoinHandle<()> {
        let cache = Arc::clone(self);
        tokio::spawn(async move {
            let started = Instant::now();
            let result = tokio::select! {
                biased;
                () = cancel.cancelled() => Err(Arc::new(BifrostStorageError::Closed)),
                loaded = load => loaded.map_err(Arc::new),
            };
            cache.settle(&key, &result, started.elapsed());
            // A publish failure means every receiver was already dropped, which
            // is the ordinary outcome when the last caller cancelled. The
            // terminal accounting above has already happened, so there is
            // nothing further to do.
            let _ = publisher.send(Some(result));
        })
    }

    /// Applies one terminal load result to the resident state and telemetry.
    ///
    /// Removes the in-flight registration first so a caller arriving after the
    /// publish sees either the newly resident entry or a fresh miss, and never
    /// joins a load that has already finished.
    fn settle(&self, key: &HotMetadataKey, result: &MetadataLoadResult, elapsed: Duration) {
        let outcome = match result {
            Ok(_) => MetadataLoadOutcome::Success,
            Err(error) if matches!(**error, BifrostStorageError::Deadline) => {
                MetadataLoadOutcome::Deadline
            }
            Err(error)
                if matches!(
                    **error,
                    BifrostStorageError::Cancelled | BifrostStorageError::Closed
                ) =>
            {
                MetadataLoadOutcome::Cancelled
            }
            Err(_) => MetadataLoadOutcome::Failed,
        };
        let retention = self.retain(key, result.as_ref().ok());
        self.telemetry.record_load_terminal(outcome, elapsed);
        if let Some(reason) = retention {
            self.telemetry
                .record_cache_effect(CacheEffect::Bypass, reason);
        }
    }

    /// Retires the in-flight registration and retains an eligible success.
    ///
    /// Returns the bypass reason when the metadata could not be retained, so
    /// the caller can record that decision outside the lock. Only a successful
    /// decode that fits the whole budget is retained; an oversized entry is
    /// still returned to its callers, it is simply not kept.
    fn retain(
        &self,
        key: &HotMetadataKey,
        metadata: Option<&Arc<ParquetMetaData>>,
    ) -> Option<CacheEffectReason> {
        let Ok(mut state) = self.state.lock() else {
            return None;
        };
        if let Some(inflight) = state.inflight.remove(key) {
            // The task is finishing right now; dropping the handle detaches it
            // rather than cancelling it, which is correct — the publish below
            // is the last thing it does.
            drop(inflight);
        }
        let Some(metadata) = metadata else {
            let (entries, bytes) = state.resident_totals();
            drop(state);
            self.telemetry.record_resident(entries, bytes);
            return None;
        };
        if state.lifecycle != StorageLifecycle::Open {
            drop(state);
            return Some(CacheEffectReason::Closing);
        }
        let weight = u64::try_from(metadata.memory_size())
            .unwrap_or(u64::MAX)
            .saturating_add(key.owned_bytes());
        if weight > self.budget_bytes {
            drop(state);
            return Some(CacheEffectReason::Oversized);
        }
        let mut evicted = 0_u32;
        while state.resident_bytes.saturating_add(weight) > self.budget_bytes {
            let Some((_, removed)) = state.entries.pop_lru() else {
                break;
            };
            state.resident_bytes = state.resident_bytes.saturating_sub(removed.weight);
            evicted = evicted.saturating_add(1);
        }
        state.entries.put(
            key.clone(),
            CachedMetadata {
                metadata: Arc::clone(metadata),
                weight,
            },
        );
        state.resident_bytes = state.resident_bytes.saturating_add(weight);
        let (entries, bytes) = state.resident_totals();
        drop(state);
        for _ in 0..evicted {
            self.telemetry
                .record_cache_effect(CacheEffect::Evict, CacheEffectReason::None);
        }
        self.telemetry.record_resident(entries, bytes);
        None
    }

    /// Awaits one publication under this caller's own cancellation and deadline.
    ///
    /// Neither the token nor the deadline touches the load: they end this
    /// caller's interest in it, which is what makes a cancelled query cheap for
    /// everyone still waiting on the same object.
    async fn await_publication(
        receiver: &mut watch::Receiver<Option<MetadataLoadResult>>,
        deadline: Instant,
        cancel: &CancellationToken,
    ) -> MetadataLoadResult {
        loop {
            if let Some(result) = receiver.borrow_and_update().clone() {
                return result;
            }
            let changed = receiver.changed();
            tokio::select! {
                biased;
                () = cancel.cancelled() => return Err(Arc::new(BifrostStorageError::Cancelled)),
                () = tokio::time::sleep_until(deadline.into()) => {
                    return Err(Arc::new(BifrostStorageError::Deadline));
                }
                changed = changed => {
                    if changed.is_err() {
                        // The loader dropped its publisher without sending,
                        // which only happens if the task itself was aborted.
                        return Err(Arc::new(BifrostStorageError::Closed));
                    }
                }
            }
        }
    }

    /// Projects one awaited result into its waiter-latency outcome.
    fn wait_outcome(result: &MetadataLoadResult) -> MetadataLoadOutcome {
        match result {
            Ok(_) => MetadataLoadOutcome::Success,
            Err(error) => match **error {
                BifrostStorageError::Cancelled | BifrostStorageError::Closed => {
                    MetadataLoadOutcome::Cancelled
                }
                BifrostStorageError::Deadline => MetadataLoadOutcome::Deadline,
                _ => MetadataLoadOutcome::Failed,
            },
        }
    }

    /// Stops admitting loads and cancels every retained loader.
    ///
    /// Returns the retained task handles so the caller can join them under one
    /// absolute deadline without holding the state lock across an await.
    fn begin_close(&self) -> Vec<JoinHandle<()>> {
        let Ok(mut state) = self.state.lock() else {
            return Vec::new();
        };
        state.lifecycle = StorageLifecycle::Closing;
        let inflight = std::mem::take(&mut state.inflight);
        drop(state);
        self.telemetry.record_lifecycle(StorageLifecycle::Closing);
        inflight
            .into_values()
            .map(|entry| {
                entry.cancel.cancel();
                // Waking every waiter here rather than relying on the loader's
                // own publish is what bounds shutdown: a loader blocked in a
                // backend call may not observe its token until its request
                // timeout, and no waiter should have to wait that long.
                let _ = entry
                    .publisher
                    .send(Some(Err(Arc::new(BifrostStorageError::Closed))));
                entry.task
            })
            .collect()
    }

    /// Clears every resident entry and marks the owner settled.
    fn finish_close(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.entries.clear();
            state.resident_bytes = 0;
            state.lifecycle = StorageLifecycle::Closed;
        }
        self.telemetry.record_resident(0, 0);
        self.telemetry.record_lifecycle(StorageLifecycle::Closed);
    }

    /// Drains the cache within one absolute deadline.
    ///
    /// Returns `true` when every retained loader settled in time. Idempotent:
    /// a second call finds no loaders and no entries and reports clean.
    pub(crate) async fn close(&self, deadline: Instant) -> bool {
        let tasks = self.begin_close();
        let mut clean = true;
        for task in tasks {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if tokio::time::timeout(remaining, task).await.is_err() {
                clean = false;
            }
        }
        self.finish_close();
        clean
    }

    /// Drains the cache immediately, aborting every retained loader.
    ///
    /// Idempotent, and safe to call after [`Self::close`].
    pub(crate) fn abort(&self) {
        for task in self.begin_close() {
            task.abort();
        }
        self.finish_close();
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

        // Eviction is access-ordered, not insertion-ordered. Every fixture key
        // below has the same table and the same object-name length, so all
        // three entries weigh exactly the same and the budget is an exact
        // multiple of that weight rather than an estimate.
        let metadata = Arc::clone(&settled[0]);
        let evicting_key = |checksum: u8, object: &str| test_key(tenant, object, checksum, size);
        let weight = u64::try_from(metadata.memory_size()).expect("footprint fits u64")
            + evicting_key(0x31, "aaa.parquet").owned_bytes();
        let telemetry = Arc::new(BifrostStorageTelemetry::default());
        let cache = Arc::new(ParquetMetadataCache::new(
            2 * weight,
            Arc::clone(&telemetry),
        ));
        let resident = |cache: &Arc<ParquetMetadataCache>, key: HotMetadataKey| {
            let loaded = Arc::clone(&metadata);
            let cache = Arc::clone(cache);
            async move {
                cache
                    .get_or_load(
                        key,
                        Box::pin(async move { Ok(loaded) }),
                        deadline,
                        CancellationToken::new(),
                    )
                    .await
                    .expect("the fixture load always succeeds")
            }
        };
        let key_a = evicting_key(0x31, "aaa.parquet");
        let key_b = evicting_key(0x32, "bbb.parquet");
        let key_c = evicting_key(0x33, "ccc.parquet");
        resident(&cache, key_a.clone()).await;
        resident(&cache, key_b.clone()).await;
        let snapshot = telemetry.snapshot();
        assert_eq!(snapshot.resident_entries(), 2, "the budget holds both");
        assert_eq!(snapshot.resident_bytes(), 2 * weight);
        assert_eq!(snapshot.effect(CacheEffect::Evict), 0);

        // Touch A so B, not A, is least recently used.
        resident(&cache, key_a.clone()).await;
        assert_eq!(telemetry.snapshot().effect(CacheEffect::Hit), 1);

        resident(&cache, key_c).await;
        let snapshot = telemetry.snapshot();
        assert_eq!(
            snapshot.effect(CacheEffect::Evict),
            1,
            "admitting a third entry gives up exactly one"
        );
        assert_eq!(snapshot.resident_entries(), 2);
        assert_eq!(snapshot.resident_bytes(), 2 * weight);

        // A was touched most recently, so A survived and B is the one gone.
        resident(&cache, key_a).await;
        assert_eq!(
            telemetry.snapshot().effect(CacheEffect::Hit),
            2,
            "the recently used entry survived eviction"
        );
        resident(&cache, key_b).await;
        assert_eq!(
            telemetry.snapshot().effect(CacheEffect::Hit),
            2,
            "the least recently used entry was the one evicted"
        );

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
