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
//!
//! Concurrency is single-flight: the first caller for a key elects one retained
//! loader task and every later caller joins it, so N concurrent openings of one
//! object cost one decode. The state mutex is never held across `.await`.
//!
//! One owner supervises every load. The elected loader task is the only writer
//! of a terminal result: it races the election-fixed deadline and the
//! cache-owned token against the caller's decode, catches a panic in that
//! decode, retires its own in-flight entry, and publishes exactly one result to
//! everyone joined to it. Neither a waiter nor close ever publishes, so no two
//! paths can disagree about how one load ended.

use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::FutureExt as _;
use futures_util::future::BoxFuture;
use lru::LruCache;
use parquet::file::metadata::ParquetMetaData;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use wyrd_spec::ids::DataTenantId;

use crate::resources::{MetadataReservation, OracleMetadataResources};
use crate::storage::error::BifrostStorageError;
use crate::storage::telemetry::{
    BifrostStorageTelemetry, CacheEffect, CacheEffectReason, MetadataLoadOutcome, StorageLifecycle,
};

/// One cloneable terminal load result publishable to every joined waiter.
///
/// The error is `Arc`-wrapped because one loader's single closed failure is
/// delivered to an unbounded number of waiters; cloning the error itself would
/// duplicate its detail string per waiter for no benefit.
pub(crate) type MetadataLoadResult = Result<RetainedMetadata, Arc<BifrostStorageError>>;

/// Decoded metadata together with the root memory ownership that funds it.
///
/// Returned to every caller rather than a bare `Arc<ParquetMetaData>` so that
/// holding the metadata and owning its bytes are the same act. A borrower that
/// outlives the cache entry keeps the reservation alive, so eviction can free
/// the node's own reference without telling the Oracle memory root that bytes
/// a running query is still reading have been returned.
#[derive(Debug, Clone)]
pub struct RetainedMetadata {
    /// Immutable decoded metadata shared with every caller.
    metadata: Arc<ParquetMetaData>,
    /// Shared root-memory ownership, absent when the root declined to fund it.
    ///
    /// Absent metadata is served but never retained: the node hands the decode
    /// to the caller that paid for it and keeps nothing it cannot account for.
    reservation: Option<Arc<MetadataReservation>>,
}

impl RetainedMetadata {
    /// Wraps a decode that no cache reservation funds.
    ///
    /// The shape a disabled composition returns: the decode belongs to the
    /// query that asked for it and is accounted there, and the node retains
    /// nothing, so there is no shared ownership to carry.
    pub(crate) const fn unreserved(metadata: Arc<ParquetMetaData>) -> Self {
        Self {
            metadata,
            reservation: None,
        }
    }

    /// Returns the decoded metadata this borrower holds.
    #[must_use]
    pub fn metadata(&self) -> &Arc<ParquetMetaData> {
        &self.metadata
    }

    /// Returns whether the Oracle root funds retaining these bytes.
    #[must_use]
    pub const fn is_funded(&self) -> bool {
        self.reservation.is_some()
    }
}

impl std::ops::Deref for RetainedMetadata {
    type Target = ParquetMetaData;

    /// Borrows the decoded metadata, so a caller reads it without unwrapping
    /// the ownership it happens to travel with.
    fn deref(&self) -> &Self::Target {
        &self.metadata
    }
}

/// A caller-supplied decode of one object's Parquet metadata.
///
/// Boxed and `'static` so the cache can move it into the retained loader task.
/// The caller owns what the future actually does — in production it is a
/// governed ranged read through the storage owner's operator — which keeps the
/// cache free of any knowledge of memory accounting or storage layering.
pub(crate) type MetadataLoadFuture =
    BoxFuture<'static, Result<Arc<ParquetMetaData>, BifrostStorageError>>;

/// Why a cache-owned loader token was triggered.
///
/// Retained as an atomic beside the token because the token itself carries no
/// reason, and the terminal a loader publishes must name the real cause rather
/// than a single generic one: an operator distinguishing "every caller went
/// away" from "the node is shutting down" is reading exactly this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CancelCause {
    /// The token has not been triggered.
    None,
    /// Every waiter detached while the owner remained open.
    Abandoned,
    /// The owner is closing and admits no further work.
    Closing,
}

impl CancelCause {
    /// Returns the stable atomic encoding of this cause.
    const fn code(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Abandoned => 1,
            Self::Closing => 2,
        }
    }

    /// Recovers one cause from its atomic encoding.
    ///
    /// An unrecognized code decodes to [`Self::Closing`], the most
    /// conservative reading: it never reports work as merely abandoned when the
    /// owner may in fact be shutting down.
    const fn from_code(code: u8) -> Self {
        match code {
            0 => Self::None,
            1 => Self::Abandoned,
            _ => Self::Closing,
        }
    }

    /// Returns the terminal error this cause publishes.
    fn terminal(self) -> BifrostStorageError {
        match self {
            Self::Abandoned => BifrostStorageError::Cancelled,
            Self::None | Self::Closing => BifrostStorageError::Closed,
        }
    }
}

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
    /// Decoded metadata and the shared reservation funding it.
    metadata: RetainedMetadata,
    /// Charged weight: decoded footprint plus the key's owned bytes.
    ///
    /// The same figure the reservation owns, kept here so the byte ceiling can
    /// be enforced without reaching into the root ledger on every lookup.
    weight: u64,
}

/// One elected load with its fixed bound, its own token, and its live waiters.
#[derive(Debug)]
struct InFlight {
    /// Absolute deadline chosen at election and never revised.
    ///
    /// Fixed because the load is shared: letting a later joiner widen it would
    /// let one query extend work every other joiner is waiting on, and letting
    /// one shorten it would let a caller with a tight budget end work the
    /// others still need. Each waiter still detaches on its own earlier
    /// deadline or token without touching this one.
    deadline: Instant,
    /// Cache-owned token for this key's loader alone.
    cancel: CancellationToken,
    /// Why [`Self::cancel`] was triggered, as a [`CancelCause`] code.
    cause: Arc<AtomicU8>,
    /// Callers currently depending on this load, including the elector.
    ///
    /// Reaching zero while the load is still running means nobody wants the
    /// result any more, which is the only condition under which one waiter's
    /// departure may cancel shared work.
    waiters: u64,
    /// Publishes the single terminal result to everyone joined.
    ///
    /// `watch` rather than a broadcast channel because the value is retained:
    /// a waiter that subscribes after the publish still observes the terminal
    /// result instead of missing it or observing lag.
    publisher: watch::Sender<Option<MetadataLoadResult>>,
    /// Retained loader task, awaited and if necessary aborted at close.
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
    Resident(RetainedMetadata),
    /// This caller is joined to a load, under the election-fixed deadline.
    ///
    /// The elector and a later joiner are deliberately not distinguished here:
    /// both wait the same way, on the same publication, under the same shared
    /// bound, and both are counted the same way by the waiter guard.
    Joined {
        /// Receives the single terminal result.
        receiver: watch::Receiver<Option<MetadataLoadResult>>,
        /// The shared load's fixed absolute bound.
        deadline: Instant,
    },
}

/// Live membership of one shared load, settled exactly once.
///
/// Holding a guard is what makes a caller a waiter. Settling it on return, on
/// an early error, and on drop — including the drop of a cancelled future — is
/// what keeps the waiter gauge honest, and it is what lets the cache notice
/// that the last interested caller has gone so it can stop work nobody wants.
struct WaiterGuard {
    /// The cache whose accounting this guard settles.
    cache: Arc<ParquetMetadataCache>,
    /// The load this guard is joined to.
    key: HotMetadataKey,
    /// When this caller joined, for the wait-latency histogram.
    joined: Instant,
    /// Whether [`Self::settle`] already reconciled this membership.
    settled: bool,
}

impl WaiterGuard {
    /// Settles this membership with the outcome the caller actually observed.
    fn settle(mut self, outcome: MetadataLoadOutcome) {
        self.settled = true;
        self.cache.release_waiter(&self.key, outcome, self.joined);
    }
}

impl Drop for WaiterGuard {
    /// Settles an abandoned membership as cancelled.
    ///
    /// Reached when the caller's future is dropped mid-wait, which is the
    /// ordinary shape of a cancelled query: nothing returned a result, so the
    /// only truthful outcome is that this caller stopped waiting.
    fn drop(&mut self) {
        if !self.settled {
            self.cache
                .release_waiter(&self.key, MetadataLoadOutcome::Cancelled, self.joined);
        }
    }
}

/// The bounded, single-flight decoded-metadata cache for one node.
#[derive(Debug)]
pub(crate) struct ParquetMetadataCache {
    /// Mutable resident and in-flight state.
    state: Mutex<CacheState>,
    /// Configured ceiling on charged resident bytes; always positive.
    ///
    /// A local ceiling on top of the root, not instead of it: the root bounds
    /// what the node may own at all, and this bounds how much of that the cache
    /// is willing to spend on retained footers.
    budget_bytes: u64,
    /// The Oracle memory root every retained byte is charged against.
    resources: OracleMetadataResources,
    /// Serializes close and shares its single completion with every caller.
    ///
    /// `Some(clean)` once a close has finished, so a repeated or concurrent
    /// caller observes that same terminal lifecycle rather than running a
    /// second close over state the first one already reconciled.
    closing: tokio::sync::Mutex<Option<bool>>,
    /// The owner's production telemetry facade.
    telemetry: Arc<BifrostStorageTelemetry>,
}

impl ParquetMetadataCache {
    /// Builds one cache over a positive byte budget.
    pub(crate) fn new(
        budget_bytes: u64,
        telemetry: Arc<BifrostStorageTelemetry>,
        resources: OracleMetadataResources,
    ) -> Self {
        Self {
            state: Mutex::new(CacheState {
                entries: LruCache::unbounded(),
                resident_bytes: 0,
                inflight: HashMap::new(),
                lifecycle: StorageLifecycle::Open,
            }),
            budget_bytes,
            resources,
            closing: tokio::sync::Mutex::new(None),
            telemetry,
        }
    }

    /// Returns decoded metadata for `key`, loading it at most once per node.
    ///
    /// A resident entry returns immediately. Otherwise the first caller elects
    /// one retained loader task under an immutable deadline, and every later
    /// caller for the same key joins that task and receives a clone of its
    /// single terminal result.
    ///
    /// This caller's own `deadline` and `cancel` bound only this caller: they
    /// end its interest in the load, which is what makes a cancelled query
    /// cheap for everyone still waiting on the same object. Shared work stops
    /// only when the last waiter leaves or the owner closes.
    ///
    /// # Errors
    /// Returns the loader's closed failure, [`BifrostStorageError::Cancelled`]
    /// when this caller's token fires or the load was abandoned by all of its
    /// waiters, [`BifrostStorageError::Deadline`] when a deadline elapses
    /// first, or [`BifrostStorageError::Closed`] when the owner no longer
    /// admits loads.
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
        let (mut receiver, shared_deadline) = match self.register(&key, load, deadline)? {
            Registration::Resident(metadata) => return Ok(metadata),
            Registration::Joined { receiver, deadline } => (receiver, deadline),
        };
        let guard = WaiterGuard {
            cache: Arc::clone(self),
            key,
            joined: Instant::now(),
            settled: false,
        };
        // The caller waits under whichever bound expires first. Its own
        // deadline only ever detaches it; the shared one is what the loader
        // itself is racing, and is repeated here so a waiter is never left
        // holding a membership past the load's own bound.
        let bound = deadline.min(shared_deadline);
        let result = Self::await_publication(&mut receiver, bound, &cancel).await;
        guard.settle(Self::wait_outcome(&result));
        result
    }

    /// Records the decision for one lookup and elects a loader when needed.
    ///
    /// Everything that touches shared state happens here, under one
    /// acquisition, so a concurrent second caller either sees the resident
    /// entry or the in-flight registration the first caller just installed —
    /// never a window where both start a load. The waiter count is raised here
    /// too, before the lock is released, so a load can never look abandoned
    /// between its election and its elector's first await.
    ///
    /// # Errors
    /// Returns [`BifrostStorageError::Closed`] when the owner is closing,
    /// closed, or its state lock is poisoned.
    fn register(
        self: &Arc<Self>,
        key: &HotMetadataKey,
        load: MetadataLoadFuture,
        requested_deadline: Instant,
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
            let metadata = entry.metadata.clone();
            drop(state);
            self.telemetry
                .record_cache_effect(CacheEffect::Hit, CacheEffectReason::None);
            return Ok(Registration::Resident(metadata));
        }
        if let Some(inflight) = state.inflight.get_mut(key) {
            let receiver = inflight.publisher.subscribe();
            let deadline = inflight.deadline;
            inflight.waiters = inflight.waiters.saturating_add(1);
            drop(state);
            self.telemetry
                .record_cache_effect(CacheEffect::Join, CacheEffectReason::None);
            self.telemetry.record_waiter_joined();
            return Ok(Registration::Joined { receiver, deadline });
        }
        let (publisher, receiver) = watch::channel(None);
        let cancel = CancellationToken::new();
        let cause = Arc::new(AtomicU8::new(CancelCause::None.code()));
        // Recorded before the task can be scheduled, so a load that settles
        // immediately can never publish a terminal the totals have no start
        // for.
        self.telemetry
            .record_cache_effect(CacheEffect::Miss, CacheEffectReason::None);
        self.telemetry.record_load_start();
        self.telemetry.record_waiter_joined();
        let task = self.spawn_loader(
            key.clone(),
            load,
            publisher.clone(),
            cancel.clone(),
            Arc::clone(&cause),
            requested_deadline,
        );
        state.inflight.insert(
            key.clone(),
            InFlight {
                deadline: requested_deadline,
                cancel,
                cause,
                waiters: 1,
                publisher,
                task,
            },
        );
        drop(state);
        Ok(Registration::Joined {
            receiver,
            deadline: requested_deadline,
        })
    }

    /// Spawns the one retained, supervised loader task for `key`.
    ///
    /// The task, not any calling future, owns the load and its terminal. That
    /// is what lets a cancelled caller detach without ending work every other
    /// waiter depends on, what gives close something concrete to await, and
    /// what guarantees the key is retired and the result published exactly
    /// once — including when the caller's decode panics, which is caught here
    /// rather than left to poison the key.
    fn spawn_loader(
        self: &Arc<Self>,
        key: HotMetadataKey,
        load: MetadataLoadFuture,
        publisher: watch::Sender<Option<MetadataLoadResult>>,
        cancel: CancellationToken,
        cause: Arc<AtomicU8>,
        deadline: Instant,
    ) -> JoinHandle<()> {
        let cache = Arc::clone(self);
        tokio::spawn(async move {
            let started = Instant::now();
            let decoded: Result<Arc<ParquetMetaData>, Arc<BifrostStorageError>> = tokio::select! {
                biased;
                () = cancel.cancelled() => Err(Arc::new(
                    CancelCause::from_code(cause.load(Ordering::Acquire)).terminal(),
                )),
                () = tokio::time::sleep_until(deadline.into()) => {
                    Err(Arc::new(BifrostStorageError::Deadline))
                }
                loaded = AssertUnwindSafe(load).catch_unwind() => match loaded {
                    Ok(loaded) => loaded.map_err(Arc::new),
                    // A panicking decode is undecodable bytes as far as every
                    // waiter is concerned, and turning it into a terminal here
                    // is what stops one bad object from leaving its key
                    // permanently in flight.
                    Err(_) => Err(Arc::new(BifrostStorageError::InvalidData {
                        detail: "parquet metadata decode panicked".to_owned(),
                    })),
                },
            };
            let result = decoded.map(|metadata| cache.fund(&key, metadata));
            cache.settle(&key, &result, started.elapsed());
            // A publish failure means every receiver was already dropped, which
            // is the ordinary outcome when the last caller detached. The
            // terminal accounting above has already happened, so there is
            // nothing further to do.
            let _ = publisher.send(Some(result));
        })
    }

    /// Settles one waiter's membership and stops abandoned work.
    ///
    /// Called exactly once per admitted waiter, from the guard's normal path or
    /// its drop. When the departing waiter was the last one and the load is
    /// still running under an open owner, its token is triggered with
    /// [`CancelCause::Abandoned`]: the loader then publishes `Cancelled`,
    /// retires its own key, and a later request is free to elect a fresh load.
    fn release_waiter(&self, key: &HotMetadataKey, outcome: MetadataLoadOutcome, joined: Instant) {
        let abandoned = {
            let Ok(mut state) = self.state.lock() else {
                return;
            };
            let open = state.lifecycle == StorageLifecycle::Open;
            state.inflight.get_mut(key).and_then(|inflight| {
                inflight.waiters = inflight.waiters.saturating_sub(1);
                (open && inflight.waiters == 0).then(|| {
                    inflight
                        .cause
                        .store(CancelCause::Abandoned.code(), Ordering::Release);
                    inflight.cancel.clone()
                })
            })
        };
        if let Some(cancel) = abandoned {
            cancel.cancel();
        }
        self.telemetry
            .record_waiter_settled(outcome, joined.elapsed());
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
        metadata: Option<&RetainedMetadata>,
    ) -> Option<CacheEffectReason> {
        let Ok(mut state) = self.state.lock() else {
            return None;
        };
        if let Some(inflight) = state.inflight.remove(key) {
            // The task is finishing right now; dropping the handle detaches it
            // rather than cancelling it, which is correct — the publish that
            // follows this call is the last thing it does.
            drop(inflight);
        }
        let Some(metadata) = metadata else {
            let (entries, bytes) = state.resident_totals();
            drop(state);
            self.telemetry.record_resident(entries, bytes);
            return None;
        };
        if !metadata.is_funded() {
            drop(state);
            // The unfunded decision was already published when the root
            // declined it; retention simply does not happen.
            return None;
        }
        if state.lifecycle != StorageLifecycle::Open {
            drop(state);
            return Some(CacheEffectReason::Closing);
        }
        let weight = Self::weight_of(key, metadata);
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
                metadata: metadata.clone(),
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

    /// Returns the exact bytes retaining this decode would own.
    ///
    /// The key's own heap is included because the cache stores it beside the
    /// metadata; charging only the footer would let a table full of long object
    /// paths grow past both the local ceiling and the root's view of it.
    fn weight_of(key: &HotMetadataKey, metadata: &RetainedMetadata) -> u64 {
        u64::try_from(metadata.memory_size())
            .unwrap_or(u64::MAX)
            .saturating_add(key.owned_bytes())
    }

    /// Charges one successful decode to the Oracle root before it is shared.
    ///
    /// Reserving here, in the loader, is what makes the reservation shared: the
    /// single result published to every joined caller carries the one
    /// reservation, so N borrowers of one decode own one charge rather than
    /// none or N. A root that declines still returns the metadata — the
    /// elected query already paid for that decode and is entitled to it — but
    /// the node keeps nothing it could not account for.
    fn fund(&self, key: &HotMetadataKey, metadata: Arc<ParquetMetaData>) -> RetainedMetadata {
        let unfunded = RetainedMetadata {
            metadata,
            reservation: None,
        };
        let bytes = usize::try_from(Self::weight_of(key, &unfunded)).unwrap_or(usize::MAX);
        if let Ok(reservation) = self.resources.try_reserve_metadata(bytes) {
            RetainedMetadata {
                metadata: unfunded.metadata,
                reservation: Some(Arc::new(reservation)),
            }
        } else {
            self.telemetry
                .record_cache_effect(CacheEffect::Bypass, CacheEffectReason::Unfunded);
            unfunded
        }
    }

    /// Awaits one publication under this caller's own bound and token.
    ///
    /// Neither the token nor the bound touches the load itself: they end this
    /// caller's interest in it, and the cache decides separately whether the
    /// work still has anyone waiting for it.
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

    /// Stops admitting loads, triggers every loader token, and takes the tasks.
    ///
    /// Each task is paired with whether it had already settled when close
    /// began, sampled before `Closing` is published, so a loader that finishes
    /// only after close started can never be mistaken for one that settled in
    /// time.
    ///
    /// The tokens are triggered with [`CancelCause::Closing`] so each loader
    /// publishes `Closed` itself. Close deliberately publishes nothing: two
    /// writers of one terminal could disagree about how a load ended, and the
    /// loader is the one that actually knows.
    fn begin_close(&self) -> Vec<(JoinHandle<()>, bool)> {
        let Ok(mut state) = self.state.lock() else {
            return Vec::new();
        };
        state.lifecycle = StorageLifecycle::Closing;
        let inflight = std::mem::take(&mut state.inflight);
        drop(state);
        let tasks: Vec<_> = inflight
            .into_values()
            .map(|entry| {
                let settled = entry.task.is_finished();
                (entry, settled)
            })
            .collect();
        self.telemetry.record_lifecycle(StorageLifecycle::Closing);
        tasks
            .into_iter()
            .map(|(entry, settled)| {
                entry
                    .cause
                    .store(CancelCause::Closing.code(), Ordering::Release);
                entry.cancel.cancel();
                (entry.task, settled)
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
    /// Returns `true` when every retained loader settled on its own before the
    /// deadline. A loader that did not is aborted *and awaited*, so `Closed` is
    /// never observable while a task is still running: the difference between
    /// clean and unclean is whether shutdown had to force the issue, not
    /// whether anything is still outstanding afterwards.
    ///
    /// Close is serialized behind one shared completion. Concurrent callers
    /// wait for the same close, and a repeated caller observes that same
    /// terminal lifecycle instead of running a second close over state the
    /// first one already reconciled.
    pub(crate) async fn close(&self, deadline: Instant) -> bool {
        let mut closing = self.closing.lock().await;
        if let Some(clean) = *closing {
            return clean;
        }
        // With no budget left there is nothing to wait for: a loader still
        // running when close began is forced, however soon it finishes after.
        let no_budget = deadline <= Instant::now();
        let mut clean = true;
        for (mut task, settled) in self.begin_close() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let forced = if no_budget {
                !settled
            } else {
                tokio::time::timeout(remaining, &mut task).await.is_err()
            };
            if forced {
                // The loader did not settle within the shutdown budget. Abort
                // it and await the abort, so the owner never reports `Closed`
                // while a task is still touching its state.
                clean = false;
                task.abort();
                if task.await.is_err() {
                    // The loader was cancelled before it could publish its own
                    // terminal, so close reconciles that one load itself.
                    // Without this the in-flight gauge would keep a start no
                    // terminal will ever settle, and a forced shutdown would
                    // look like a leak forever.
                    self.telemetry
                        .record_load_terminal(MetadataLoadOutcome::Cancelled, Duration::ZERO);
                }
            }
        }
        self.finish_close();
        *closing = Some(clean);
        clean
    }

    /// Drains the cache immediately, aborting and awaiting every loader.
    ///
    /// Idempotent, and safe to call after [`Self::close`]: both share the same
    /// single completion, so the second caller observes the first one's result
    /// rather than reopening a settled lifecycle.
    pub(crate) async fn abort(&self) {
        self.close(Instant::now()).await;
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
    use crate::storage::telemetry::{
        CacheEffect, CacheEffectReason, MetadataCacheSnapshot, MetadataLoadOutcome,
    };

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

    /// Builds the Oracle role capability these tests charge retained bytes to.
    ///
    /// # Panics
    /// Panics when the fixture observation does not admit an Oracle role.
    fn oracle_role(memory_bytes: usize) -> crate::resources::OracleResources {
        crate::resources::BifrostRuntimeResources::composed_for_test(
            memory_bytes,
            1024 * 1024 * 1024,
            [crate::resources::BifrostRole::Oracle],
        )
        .oracle()
        .expect("the fixture observation admits Oracle")
    }

    /// Builds the Oracle memory root these tests charge retained bytes to.
    ///
    /// A real governor rather than a stub: the reservation coupling this suite
    /// asserts is only meaningful against the ledger production uses, and a
    /// fake root would make "the bytes stayed charged" a statement about the
    /// fake.
    ///
    /// # Panics
    /// Panics when the fixture observation does not admit an Oracle role.
    fn oracle_metadata_resources(memory_bytes: usize) -> OracleMetadataResources {
        oracle_role(memory_bytes).metadata()
    }

    /// Builds a local-backed storage handle for the fixture owner.
    ///
    /// The suite never reads through the backend — every decode is driven by an
    /// in-memory reader — but the owner holds a real handle in production, so
    /// the fixture holds one too rather than making the field optional purely
    /// for tests.
    ///
    /// # Panics
    /// Panics when the temporary root or signer cannot be created.
    fn local_handle() -> Arc<wyrd_storage::handle::StorageHandle> {
        let root = tempfile::tempdir().expect("fixture storage root");
        let signer = wyrd_storage::signer::BackendSigner::Local(
            wyrd_storage::local::LocalSigner::new(root.keep()).expect("fixture local signer"),
        );
        Arc::new(wyrd_storage::handle::StorageHandle::new(signer))
    }

    /// Builds an Oracle-serving owner with an explicit metadata-cache budget.
    fn storage_with_cache(cache_bytes: u64) -> BifrostStorage {
        BifrostStorage::new(
            local_handle(),
            BifrostStoragePolicy::resolve(
                crate::storage::policy::BifrostStorageConfig {
                    metadata_cache_bytes: Some(cache_bytes),
                    ..crate::storage::policy::BifrostStorageConfig::default()
                },
                2 * 1024 * 1024 * 1024,
                true,
            )
            .expect("the fixture storage policy is valid"),
            Some(oracle_metadata_resources(2 * 1024 * 1024 * 1024)),
        )
    }

    /// One node decodes one immutable object at most once, serves a repeat
    /// request with no backend load at all, keys on the object's identity
    /// alone, and records an explicit disabled bypass that retains nothing.
    ///
    /// These properties are asserted together because they are one invariant
    /// seen from several sides: the entry the cache decides to create, the load
    /// it declines to repeat, the identity it creates entries under, and the
    /// path a disabled composition takes. Splitting them would let a key that
    /// fragments per request still pass a single-flight test.
    ///
    /// # Panics
    /// Panics when a waiter task, decode, or reconciliation assertion fails.
    #[tokio::test]
    async fn metadata_cache_reconciles_single_flight_identity_and_bypass() {
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
            let object = object.clone();
            let decodes = Arc::clone(&decodes);
            let gate = (index == 0).then(|| Arc::clone(&gate));
            waiters.push(tokio::spawn(async move {
                storage
                    .hot_metadata(
                        waiter_key,
                        move || CountingReader::new(&object, &decodes, gate.clone()),
                        deadline,
                        CancellationToken::new(),
                    )
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
                Arc::ptr_eq(observed.metadata(), settled[0].metadata()),
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
                {
                    let object = object.clone();
                    let decodes = Arc::clone(&decodes);
                    move || CountingReader::new(&object, &decodes, None)
                },
                deadline,
                CancellationToken::new(),
            )
            .await
            .expect("the warm entry is served");
        assert!(Arc::ptr_eq(warm.metadata(), settled[0].metadata()));
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
                {
                    let object = object.clone();
                    let decodes = Arc::clone(&decodes);
                    move || CountingReader::new(&object, &decodes, None)
                },
                deadline,
                CancellationToken::new(),
            )
            .await
            .expect("the rewritten object decodes");
        assert!(!Arc::ptr_eq(rewritten.metadata(), settled[0].metadata()));
        assert_eq!(decodes.load(Ordering::SeqCst), 2, "checksum is identity");
        assert_eq!(
            storage.telemetry_snapshot().effect(CacheEffect::Miss),
            2,
            "a changed checksum is a new object, not a hit"
        );
    }

    /// A composition with no metadata budget still serves every read, says so,
    /// and retains nothing.
    ///
    /// Separate from the caching scenario because it is a different
    /// composition: the point is that turning the cache off is a declared,
    /// observable path rather than silence, so an operator reading telemetry
    /// can tell "no cache" apart from "cache never consulted".
    ///
    /// # Panics
    /// Panics when a decode or bypass assertion fails.
    #[tokio::test]
    async fn a_composition_with_no_budget_bypasses_and_retains_nothing() {
        let object = parquet_object(64);
        let size = u64::try_from(object.len()).expect("fixture object fits u64");
        let tenant = DataTenantId::new_v7();
        let deadline = Instant::now() + Duration::from_secs(30);
        let key = test_key(tenant, "a.parquet", 0x11, size);
        let disabled = storage_with_cache(0);
        assert!(!disabled.metadata_cache_enabled());
        let disabled_decodes = Arc::new(AtomicUsize::new(0));
        for _ in 0..2 {
            disabled
                .hot_metadata(
                    key.clone(),
                    {
                        let object = object.clone();
                        let disabled_decodes = Arc::clone(&disabled_decodes);
                        move || CountingReader::new(&object, &disabled_decodes, None)
                    },
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
    }

    /// The cache stays inside its byte ceiling by giving up its least recently
    /// used entry, and refuses to keep what the whole ceiling cannot hold.
    ///
    /// Separate from the single-flight scenario because it is a different
    /// decision made against a different cache: this one is deliberately sized
    /// to exactly two entries so eviction order is observable rather than
    /// incidental.
    ///
    /// # Panics
    /// Panics when a decode or eviction assertion fails.
    #[tokio::test]
    async fn the_cache_evicts_in_least_recently_used_order_and_refuses_oversize() {
        let object = parquet_object(64);
        let size = u64::try_from(object.len()).expect("fixture object fits u64");
        let tenant = DataTenantId::new_v7();
        let metadata = decode_fixture(&object).await;
        let deadline = Instant::now() + Duration::from_hours(1);
        // Eviction is access-ordered, not insertion-ordered. Every fixture key
        // below has the same table and the same object-name length, so all
        // three entries weigh exactly the same and the budget is an exact
        // multiple of that weight rather than an estimate.
        let evicting_key = |checksum: u8, object: &str| test_key(tenant, object, checksum, size);
        let weight = u64::try_from(metadata.memory_size()).expect("footprint fits u64")
            + evicting_key(0x31, "aaa.parquet").owned_bytes();
        let telemetry = Arc::new(BifrostStorageTelemetry::default());
        let cache = Arc::new(ParquetMetadataCache::new(
            2 * weight,
            Arc::clone(&telemetry),
            oracle_metadata_resources(2 * 1024 * 1024 * 1024),
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
        let tiny = Arc::new(ParquetMetadataCache::new(
            1,
            Arc::clone(&tiny_telemetry),
            oracle_metadata_resources(2 * 1024 * 1024 * 1024),
        ));
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
        assert!(Arc::ptr_eq(bypassed.metadata(), &metadata));
        let snapshot = tiny_telemetry.snapshot();
        assert_eq!(snapshot.effect(CacheEffect::Bypass), 1);
        assert_eq!(snapshot.reason(CacheEffectReason::Oversized), 1);
        assert_eq!(snapshot.resident_entries(), 0);
        assert_eq!(snapshot.resident_bytes(), 0);
    }

    /// Decodes the fixture object's real footer for use as a load result.
    ///
    /// # Panics
    /// Panics when the fixture object does not decode.
    async fn decode_fixture(object: &Bytes) -> Arc<ParquetMetaData> {
        let size = u64::try_from(object.len()).expect("fixture object fits u64");
        let mut reader = CountingReader::new(object, &Arc::new(AtomicUsize::new(0)), None);
        parquet::file::metadata::ParquetMetaDataReader::new()
            .load_and_finish(&mut reader, size)
            .await
            .map(Arc::new)
            .expect("fixture footer decodes")
    }

    /// Waits until the owner's totals satisfy `reached`.
    ///
    /// A barrier, not a timed assumption: no assertion below depends on how
    /// long this takes, only on the state it waits for, so a slow machine
    /// makes the test slower rather than flaky. It yields first and only backs
    /// off when yielding is not enough, which is the case when the task it is
    /// waiting on runs on another worker thread. Bounded so a design
    /// regression fails the test instead of hanging it.
    ///
    /// # Panics
    /// Panics when the condition is not reached within the yield budget.
    async fn settled_to(
        telemetry: &Arc<BifrostStorageTelemetry>,
        reached: impl Fn(&MetadataCacheSnapshot) -> bool,
    ) {
        for attempt in 0..2_000_u32 {
            if reached(&telemetry.snapshot()) {
                return;
            }
            if attempt % 32 == 31 {
                tokio::time::sleep(Duration::from_millis(1)).await;
            } else {
                tokio::task::yield_now().await;
            }
        }
        panic!(
            "the owner never reached the awaited state: {:?}",
            telemetry.snapshot()
        );
    }

    /// Builds a load future that fails the test if it is ever polled.
    ///
    /// A joiner supplies one of these, so the assertion that N callers cost one
    /// decode is proven by the joiners' own work never starting, not only by a
    /// count the cache itself reports.
    fn unused_load() -> MetadataLoadFuture {
        async { panic!("a joined caller's load future must never be polled") }.boxed()
    }

    /// The elected loader task owns the terminal for every caller joined to it:
    /// one failure settles all three exactly once and retires their key.
    ///
    /// The fan-out is the proof: a per-caller loader would satisfy a
    /// single-caller test while still letting three callers disagree about how
    /// one load ended, and the joiners' own load futures never running is what
    /// shows the work was genuinely shared rather than merely counted as
    /// shared.
    ///
    /// # Panics
    /// Panics when a waiter task, terminal, or reconciliation assertion fails.
    #[tokio::test]
    async fn one_supervised_terminal_settles_every_caller_and_frees_the_key() {
        let tenant = DataTenantId::new_v7();
        let telemetry = Arc::new(BifrostStorageTelemetry::default());
        let cache = Arc::new(ParquetMetadataCache::new(
            64 * 1024 * 1024,
            Arc::clone(&telemetry),
            oracle_metadata_resources(2 * 1024 * 1024 * 1024),
        ));
        let far = Instant::now() + Duration::from_hours(1);

        let failing = test_key(tenant, "failing.parquet", 0x11, 1);
        let gate = Arc::new(Notify::new());
        let elector = tokio::spawn({
            let cache = Arc::clone(&cache);
            let key = failing.clone();
            let gate = Arc::clone(&gate);
            async move {
                cache
                    .get_or_load(
                        key,
                        async move {
                            gate.notified().await;
                            Err(BifrostStorageError::NotFound {
                                detail: "the fixture object is absent".to_owned(),
                            })
                        }
                        .boxed(),
                        far,
                        CancellationToken::new(),
                    )
                    .await
            }
        });
        settled_to(&telemetry, |snapshot| snapshot.load_starts() == 1).await;
        let joiners = (0..2)
            .map(|_| {
                tokio::spawn({
                    let cache = Arc::clone(&cache);
                    let key = failing.clone();
                    async move {
                        cache
                            .get_or_load(key, unused_load(), far, CancellationToken::new())
                            .await
                    }
                })
            })
            .collect::<Vec<_>>();
        settled_to(&telemetry, |snapshot| snapshot.waiters() == 3).await;
        gate.notify_waiters();

        let elected = elector.await.expect("the elector task completes");
        assert!(matches!(
            elected.expect_err("the elected load fails"),
            ref error if matches!(**error, BifrostStorageError::NotFound { .. })
        ));
        for joiner in joiners {
            let joined = joiner.await.expect("a joined task completes");
            assert!(matches!(
                joined.expect_err("a joined caller observes the same failure"),
                ref error if matches!(**error, BifrostStorageError::NotFound { .. })
            ));
        }
        let snapshot = telemetry.snapshot();
        assert_eq!(snapshot.effect(CacheEffect::Miss), 1);
        assert_eq!(snapshot.effect(CacheEffect::Join), 2);
        assert_eq!(snapshot.load_starts(), 1);
        assert_eq!(snapshot.load_terminal(MetadataLoadOutcome::Failed), 1);
        assert_eq!(snapshot.inflight_loads(), 0);
        assert_eq!(snapshot.waiters(), 0);

        assert_eq!(snapshot.anomalies(), 0);
        assert!(cache.close(far).await);
    }

    /// A key whose load ended badly is retired rather than poisoned, and a
    /// panicking decode settles its callers like any other terminal.
    ///
    /// Separate from the fan-out scenario because it asserts what happens
    /// *after* a terminal: the failure a node saw once must not be the failure
    /// it keeps returning, and a decode that panicked must not leave its key
    /// permanently in flight.
    ///
    /// # Panics
    /// Panics when a decode, terminal, or reconciliation assertion fails.
    #[tokio::test]
    async fn a_badly_ended_load_frees_its_key_for_a_later_caller() {
        let object = parquet_object(32);
        let decoded = decode_fixture(&object).await;
        let tenant = DataTenantId::new_v7();
        let telemetry = Arc::new(BifrostStorageTelemetry::default());
        let cache = Arc::new(ParquetMetadataCache::new(
            64 * 1024 * 1024,
            Arc::clone(&telemetry),
            oracle_metadata_resources(2 * 1024 * 1024 * 1024),
        ));
        let far = Instant::now() + Duration::from_hours(1);
        let failing = test_key(tenant, "failing.parquet", 0x11, 1);
        let observed = cache
            .get_or_load(
                failing.clone(),
                async {
                    Err(BifrostStorageError::NotFound {
                        detail: "the fixture object is absent".to_owned(),
                    })
                }
                .boxed(),
                far,
                CancellationToken::new(),
            )
            .await
            .expect_err("the first load fails");
        assert!(matches!(*observed, BifrostStorageError::NotFound { .. }));

        // The failed key is retired, not poisoned: a later caller elects a
        // fresh load and its success is retained.
        let retried = cache
            .get_or_load(
                failing.clone(),
                {
                    let decoded = Arc::clone(&decoded);
                    async move { Ok(decoded) }.boxed()
                },
                far,
                CancellationToken::new(),
            )
            .await
            .expect("the retried load succeeds");
        assert_eq!(retried.memory_size(), decoded.memory_size());
        assert_eq!(telemetry.snapshot().effect(CacheEffect::Miss), 2);
        assert_eq!(telemetry.snapshot().resident_entries(), 1);

        // A panicking decode is a terminal like any other: it settles every
        // caller and releases the key rather than stranding it in flight.
        let panicking = test_key(tenant, "panicking.parquet", 0x22, 1);
        let observed = cache
            .get_or_load(
                panicking.clone(),
                async { panic!("the fixture decode panics") }.boxed(),
                far,
                CancellationToken::new(),
            )
            .await
            .expect_err("a panicking decode fails its caller");
        assert!(matches!(*observed, BifrostStorageError::InvalidData { .. }));
        let snapshot = telemetry.snapshot();
        assert_eq!(snapshot.load_terminal(MetadataLoadOutcome::Failed), 2);
        assert_eq!(snapshot.load_starts(), snapshot.load_terminals());
        assert_eq!(snapshot.inflight_loads(), 0);
        assert_eq!(snapshot.waiters(), 0);
        assert_eq!(snapshot.anomalies(), 0);
        assert!(cache.close(far).await);
    }

    /// One caller's cancellation or deadline never ends work another caller is
    /// still waiting on, the shared deadline is fixed at election, and the last
    /// waiter's departure stops abandoned work with a truthful cause.
    ///
    /// Asserted together because they are the two halves of one bound: a
    /// per-caller bound that only detaches, and a shared bound that actually
    /// ends the load. Testing either alone would accept an implementation where
    /// the first cancelled query kills a load six others still need, or where a
    /// long-deadline joiner keeps abandoned work alive.
    ///
    /// # Panics
    /// Panics when a waiter task, terminal cause, or reconciliation assertion
    /// fails.
    #[tokio::test]
    async fn a_shared_load_outlives_one_caller_and_ends_when_all_of_them_leave() {
        let tenant = DataTenantId::new_v7();
        let telemetry = Arc::new(BifrostStorageTelemetry::default());
        let cache = Arc::new(ParquetMetadataCache::new(
            64 * 1024 * 1024,
            Arc::clone(&telemetry),
            oracle_metadata_resources(2 * 1024 * 1024 * 1024),
        ));
        let far = Instant::now() + Duration::from_hours(1);

        let abandoned = test_key(tenant, "abandoned.parquet", 0x31, 1);
        let cancel = CancellationToken::new();
        let elector = tokio::spawn({
            let cache = Arc::clone(&cache);
            let key = abandoned.clone();
            let cancel = cancel.clone();
            async move {
                cache
                    .get_or_load(key, std::future::pending().boxed(), far, cancel)
                    .await
            }
        });
        settled_to(&telemetry, |snapshot| snapshot.load_starts() == 1).await;
        let joiner = tokio::spawn({
            let cache = Arc::clone(&cache);
            let key = abandoned.clone();
            async move {
                cache
                    .get_or_load(key, unused_load(), far, CancellationToken::new())
                    .await
            }
        });
        settled_to(&telemetry, |snapshot| snapshot.waiters() == 2).await;

        // The elector detaches on its own token. The load keeps running because
        // the joiner still wants the result.
        cancel.cancel();
        let elected = elector.await.expect("the elector task completes");
        assert!(matches!(
            *elected.expect_err("the cancelled elector fails"),
            BifrostStorageError::Cancelled
        ));
        settled_to(&telemetry, |snapshot| snapshot.waiters() == 1).await;
        assert_eq!(telemetry.snapshot().inflight_loads(), 1);

        // The last waiter's departure is what ends work nobody wants, and the
        // published cause says so.
        joiner.abort();
        settled_to(&telemetry, |snapshot| snapshot.inflight_loads() == 0).await;
        let snapshot = telemetry.snapshot();
        assert_eq!(snapshot.load_terminal(MetadataLoadOutcome::Cancelled), 1);
        assert_eq!(snapshot.waiters(), 0);
        assert_eq!(snapshot.resident_entries(), 0);

        // A later joiner accepts the election-fixed deadline and cannot widen
        // it: both callers observe the same deadline terminal at the elected
        // bound, not at the joiner's own far later one. The elected bound is
        // short but not already spent, because the joiner has to reach an
        // open load for the claim to mean anything — an already-elapsed bound
        // would let the elector settle and free the key first, and the joiner
        // would then elect a load of its own instead of accepting this one's.
        let bounded = test_key(tenant, "bounded.parquet", 0x32, 1);
        let short = Instant::now() + Duration::from_millis(500);
        let elector = tokio::spawn({
            let cache = Arc::clone(&cache);
            let key = bounded.clone();
            async move {
                cache
                    .get_or_load(
                        key,
                        std::future::pending().boxed(),
                        short,
                        CancellationToken::new(),
                    )
                    .await
            }
        });
        settled_to(&telemetry, |snapshot| snapshot.load_starts() == 2).await;
        let joiner = tokio::spawn({
            let cache = Arc::clone(&cache);
            let key = bounded.clone();
            async move {
                cache
                    .get_or_load(key, unused_load(), far, CancellationToken::new())
                    .await
            }
        });
        settled_to(&telemetry, |snapshot| snapshot.waiters() == 2).await;
        for task in [elector, joiner] {
            let observed = task.await.expect("a bounded task completes");
            assert!(matches!(
                *observed.expect_err("the bounded load fails"),
                BifrostStorageError::Deadline
            ));
        }
        let snapshot = telemetry.snapshot();
        assert_eq!(snapshot.load_terminal(MetadataLoadOutcome::Deadline), 1);
        assert_eq!(snapshot.load_starts(), snapshot.load_terminals());
        assert!(snapshot.is_quiescent());
        assert!(cache.close(far).await);
    }

    /// Close is serialized, repeatable, and never reports a settled owner while
    /// a loader task is still running.
    ///
    /// Asserted as one scenario because "closed" is a single claim: a graceful
    /// close that settles its loaders is clean, a close whose budget elapsed is
    /// unclean but still leaves nothing running, and every concurrent or later
    /// caller observes that same one outcome rather than re-closing state the
    /// first close already reconciled.
    ///
    /// # Panics
    /// Panics when a close, terminal, or reconciliation assertion fails.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn close_settles_every_loader_once_and_shares_that_one_outcome() {
        let tenant = DataTenantId::new_v7();
        let telemetry = Arc::new(BifrostStorageTelemetry::default());
        let cache = Arc::new(ParquetMetadataCache::new(
            64 * 1024 * 1024,
            Arc::clone(&telemetry),
            oracle_metadata_resources(2 * 1024 * 1024 * 1024),
        ));
        let far = Instant::now() + Duration::from_hours(1);

        let graceful = test_key(tenant, "graceful.parquet", 0x41, 1);
        let waiter = tokio::spawn({
            let cache = Arc::clone(&cache);
            let key = graceful.clone();
            async move {
                cache
                    .get_or_load(
                        key,
                        std::future::pending().boxed(),
                        far,
                        CancellationToken::new(),
                    )
                    .await
            }
        });
        settled_to(&telemetry, |snapshot| snapshot.load_starts() == 1).await;

        // Two closers race one close. Both observe the same completion, and the
        // loader settles itself inside the budget, so the close is clean.
        let concurrent = tokio::spawn({
            let cache = Arc::clone(&cache);
            async move { cache.close(far).await }
        });
        assert!(cache.close(far).await);
        assert!(concurrent.await.expect("the concurrent closer completes"));
        assert!(
            cache.close(Instant::now()).await,
            "a repeated close observes the first close's outcome"
        );

        let observed = waiter.await.expect("the waiter task completes");
        assert!(matches!(
            *observed.expect_err("a load outstanding at close fails"),
            BifrostStorageError::Closed
        ));
        let snapshot = telemetry.snapshot();
        assert_eq!(snapshot.load_terminal(MetadataLoadOutcome::Cancelled), 1);
        assert_eq!(snapshot.load_starts(), snapshot.load_terminals());
        assert_eq!(snapshot.lifecycle(), StorageLifecycle::Closed);
        assert!(snapshot.is_quiescent());

        // Admission is closed for good: a later caller is refused, and the
        // refusal is published rather than silently treated as a miss.
        let refused = cache
            .get_or_load(graceful, unused_load(), far, CancellationToken::new())
            .await
            .expect_err("a request after close is refused");
        assert!(matches!(*refused, BifrostStorageError::Closed));
        assert_eq!(
            telemetry.snapshot().reason(CacheEffectReason::Closing),
            1,
            "the refusal is recorded, not silent"
        );
    }

    /// A close whose budget has already elapsed reports unclean and still
    /// leaves nothing running.
    ///
    /// A decode is CPU-bound, so a loader can be somewhere its cancellation
    /// token cannot reach it. Shutdown must still terminate: the owner aborts
    /// such a loader, awaits the abort, and reconciles the load it could not
    /// wait out, so `Closed` never means "probably finished".
    ///
    /// # Panics
    /// Panics when the forced close, its terminal, or reconciliation fails.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_close_with_no_budget_left_aborts_and_awaits_its_loaders() {
        /// How long close must stay outstanding to prove it gave up on the
        /// blocked loader rather than observing a finished one. Any value
        /// exceeds the handful of instructions between recording `Closing` and
        /// polling the loader; it is a lower bound on close's progress, never a
        /// wait for it, because a blocked worker makes early completion
        /// impossible rather than merely unlikely.
        const FORCED_CLOSE_EVIDENCE: Duration = Duration::from_millis(500);

        let tenant = DataTenantId::new_v7();
        let far = Instant::now() + Duration::from_hours(1);
        let forced_telemetry = Arc::new(BifrostStorageTelemetry::default());
        let forced = Arc::new(ParquetMetadataCache::new(
            64 * 1024 * 1024,
            Arc::clone(&forced_telemetry),
            oracle_metadata_resources(2 * 1024 * 1024 * 1024),
        ));
        let (release, blocked) = std::sync::mpsc::channel::<()>();
        let stuck = tokio::spawn({
            let cache = Arc::clone(&forced);
            let key = test_key(tenant, "stuck.parquet", 0x42, 1);
            async move {
                cache
                    .get_or_load(
                        key,
                        async move {
                            // Blocks its worker rather than awaiting, exactly
                            // as a synchronous footer decode does.
                            let _ = blocked.recv();
                            Err(BifrostStorageError::Cancelled)
                        }
                        .boxed(),
                        far,
                        CancellationToken::new(),
                    )
                    .await
            }
        });
        settled_to(&forced_telemetry, |snapshot| snapshot.load_starts() == 1).await;
        let mut closing = tokio::spawn({
            let cache = Arc::clone(&forced);
            async move { cache.close(Instant::now()).await }
        });
        // The release must land after close has polled this loader and given
        // up on it, or close observes an already-finished load and correctly
        // reports clean. `Closing` is recorded by `begin_close`, before that
        // poll, so waiting on the lifecycle races the very thing under test.
        // A close that cannot complete is the evidence instead: the loader
        // holds its worker in a synchronous `recv`, so close is necessarily
        // parked in the abort-await it performs after giving up, and only the
        // release below can let it finish.
        assert!(
            tokio::time::timeout(FORCED_CLOSE_EVIDENCE, &mut closing)
                .await
                .is_err(),
            "a close cannot settle while a loader still holds its worker"
        );
        let _ = release.send(());
        assert!(
            !closing.await.expect("the forced closer completes"),
            "a close with no budget left cannot settle its loaders gracefully"
        );
        let observed = stuck.await.expect("the stuck waiter completes");
        assert!(
            observed.is_err(),
            "a load the owner had to force never returns metadata"
        );
        let snapshot = forced_telemetry.snapshot();
        assert_eq!(
            snapshot.load_starts(),
            snapshot.load_terminals(),
            "a forced close reconciles the loads it could not wait out"
        );
        assert!(snapshot.is_quiescent());
    }

    /// Retained metadata is owned against the Oracle memory root for as long as
    /// anything holds it, an unfundable decode is served but never kept, and a
    /// drained owner returns every byte.
    ///
    /// Asserted as one scenario because the three are one property: the cache
    /// never owns bytes the root does not know about, and never tells the root
    /// bytes are free while a query is still reading them. An eviction test
    /// alone would pass an implementation that releases the charge the moment
    /// the entry leaves the map, which is exactly the double-spend this
    /// prevents.
    ///
    /// # Panics
    /// Panics when a decode, reservation, or root assertion fails.
    #[tokio::test]
    async fn retained_metadata_stays_charged_to_the_root_until_its_last_borrower() {
        let object = parquet_object(64);
        let decoded = decode_fixture(&object).await;
        let tenant = DataTenantId::new_v7();
        let oracle = oracle_role(2 * 1024 * 1024 * 1024);
        let baseline = oracle
            .snapshot()
            .expect("baseline snapshot")
            .oracle_memory_used_bytes;
        let telemetry = Arc::new(BifrostStorageTelemetry::default());
        let key_a = test_key(tenant, "aaa.parquet", 0x51, 1);
        let key_b = test_key(tenant, "bbb.parquet", 0x52, 1);
        let weight =
            u64::try_from(decoded.memory_size()).expect("footprint fits u64") + key_a.owned_bytes();
        // Exactly one entry fits, so retaining the second must evict the first.
        let cache = Arc::new(ParquetMetadataCache::new(
            weight,
            Arc::clone(&telemetry),
            oracle.metadata(),
        ));
        let far = Instant::now() + Duration::from_hours(1);
        let load = |metadata: &Arc<ParquetMetaData>| {
            let metadata = Arc::clone(metadata);
            async move { Ok(metadata) }.boxed()
        };

        let borrower = cache
            .get_or_load(key_a.clone(), load(&decoded), far, CancellationToken::new())
            .await
            .expect("the first decode is retained");
        assert!(borrower.is_funded());
        let charged = |oracle: &crate::resources::OracleResources| {
            oracle
                .snapshot()
                .expect("root snapshot")
                .oracle_memory_used_bytes
                - baseline
        };
        assert_eq!(charged(&oracle), usize::try_from(weight).expect("fits"));

        // Evicting the entry does not free bytes the borrower is still reading.
        let resident = cache
            .get_or_load(key_b.clone(), load(&decoded), far, CancellationToken::new())
            .await
            .expect("the second decode is retained");
        assert_eq!(telemetry.snapshot().effect(CacheEffect::Evict), 1);
        assert_eq!(telemetry.snapshot().resident_entries(), 1);
        assert_eq!(
            charged(&oracle),
            2 * usize::try_from(weight).expect("fits"),
            "an evicted entry a borrower still holds stays charged"
        );
        drop(borrower);
        assert_eq!(
            charged(&oracle),
            usize::try_from(weight).expect("fits"),
            "the last borrower's release is what returns the bytes"
        );

        // A root with nothing left to give still serves the decode; it simply
        // is not kept, and the decision is published rather than silent.
        let exact = usize::try_from(weight).expect("fits");
        let mut held = Vec::new();
        // Coarse first, then exact, so the root is drained past the point where
        // even one more entry's worth of bytes is available.
        for bytes in [crate::resources::ORACLE_METADATA_MEMORY_BYTES, exact] {
            while let Ok(reservation) = oracle.metadata().try_reserve_metadata(bytes) {
                held.push(reservation);
            }
        }
        let key_c = test_key(tenant, "ccc.parquet", 0x53, 1);
        let unfunded = cache
            .get_or_load(key_c, load(&decoded), far, CancellationToken::new())
            .await
            .expect("an unfundable decode still reaches its caller");
        assert!(!unfunded.is_funded());
        let snapshot = telemetry.snapshot();
        assert_eq!(snapshot.reason(CacheEffectReason::Unfunded), 1);
        assert_eq!(
            snapshot.resident_entries(),
            1,
            "the node keeps only what the root funded"
        );
        drop(held);
        drop(unfunded);

        assert!(cache.close(far).await);
        drop(resident);
        assert_eq!(
            charged(&oracle),
            0,
            "a drained owner returns every metadata byte it owned"
        );
        assert!(telemetry.snapshot().is_quiescent());
    }

    /// One reader that fails its first `failures` range reads from beneath
    /// Parquet, then serves the real object.
    ///
    /// Models a transient object-store range failure, which is the only class
    /// the owner is allowed to retry: an unreadable range may succeed on the
    /// next attempt, whereas a footer that does not parse never will.
    struct FlakyReader {
        /// Complete object bytes served once the injected failures are spent.
        data: Bytes,
        /// Shared count of attempts across every reader this factory built.
        attempts: Arc<AtomicUsize>,
        /// How many attempts fail before the object is served.
        failures: usize,
        /// Whether this reader has already consumed an attempt.
        counted: bool,
    }

    impl AsyncFileReader for FlakyReader {
        fn get_bytes(
            &mut self,
            range: Range<u64>,
        ) -> futures_util::future::BoxFuture<'_, Result<Bytes, ParquetError>> {
            async move {
                if !self.counted {
                    self.counted = true;
                    let attempt = self.attempts.fetch_add(1, Ordering::SeqCst);
                    if attempt < self.failures {
                        return Err(ParquetError::External(
                            "the fixture object store dropped the range".into(),
                        ));
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

    /// A read whose range failed transiently is retried and succeeds; a footer
    /// that cannot decode is returned on its first attempt.
    ///
    /// Asserted together because retry policy is only correct as a pair. Retry
    /// everything and a corrupt object costs every attempt and every backoff
    /// before failing anyway; retry nothing and one dropped connection fails a
    /// query that would have succeeded immediately.
    ///
    /// # Panics
    /// Panics when a decode, attempt count, or error class assertion fails.
    #[tokio::test]
    async fn a_transient_range_failure_is_retried_and_a_corrupt_footer_is_not() {
        let object = parquet_object(32);
        let size = u64::try_from(object.len()).expect("fixture object fits u64");
        let tenant = DataTenantId::new_v7();
        let storage = storage_with_cache(1 << 20);
        let far = Instant::now() + Duration::from_hours(1);
        let attempts = Arc::new(AtomicUsize::new(0));

        let retried = storage
            .hot_metadata(
                test_key(tenant, "flaky.parquet", 0x61, size),
                {
                    let object = object.clone();
                    let attempts = Arc::clone(&attempts);
                    move || FlakyReader {
                        data: object.clone(),
                        attempts: Arc::clone(&attempts),
                        failures: 2,
                        counted: false,
                    }
                },
                far,
                CancellationToken::new(),
            )
            .await
            .expect("a transient range failure does not fail the read");
        assert!(retried.is_funded());
        assert_eq!(
            attempts.load(Ordering::SeqCst),
            3,
            "the read is retried exactly as far as the policy allows"
        );

        let corrupt = Bytes::from_static(b"not a parquet footer at all");
        let corrupt_attempts = Arc::new(AtomicUsize::new(0));
        let failure = storage
            .hot_metadata(
                test_key(
                    tenant,
                    "corrupt.parquet",
                    0x62,
                    u64::try_from(corrupt.len()).expect("fixture fits u64"),
                ),
                {
                    let corrupt = corrupt.clone();
                    let corrupt_attempts = Arc::clone(&corrupt_attempts);
                    move || FlakyReader {
                        data: corrupt.clone(),
                        attempts: Arc::clone(&corrupt_attempts),
                        failures: 0,
                        counted: false,
                    }
                },
                far,
                CancellationToken::new(),
            )
            .await
            .expect_err("a footer that cannot decode fails the read");
        assert!(matches!(*failure, BifrostStorageError::InvalidData { .. }));
        assert_eq!(
            corrupt_attempts.load(Ordering::SeqCst),
            1,
            "an undecodable footer is never retried"
        );
        assert!(storage.close(far).await);
    }

    /// A composition with no cache still honours its caller's cancellation.
    ///
    /// The disabled path has no loader task supervising it, so without an
    /// explicit bound a cancelled query would keep a decode running against a
    /// backend nobody is waiting on.
    ///
    /// # Panics
    /// Panics when the cancelled read does not fail as cancelled.
    #[tokio::test]
    async fn a_disabled_composition_still_bounds_its_caller() {
        let object = parquet_object(32);
        let size = u64::try_from(object.len()).expect("fixture object fits u64");
        let tenant = DataTenantId::new_v7();
        let storage = storage_with_cache(0);
        assert!(!storage.metadata_cache_enabled());
        let cancel = CancellationToken::new();
        cancel.cancel();
        let decodes = Arc::new(AtomicUsize::new(0));
        let observed = storage
            .hot_metadata(
                test_key(tenant, "cancelled.parquet", 0x71, size),
                {
                    let object = object.clone();
                    let decodes = Arc::clone(&decodes);
                    move || CountingReader::new(&object, &decodes, None)
                },
                Instant::now() + Duration::from_hours(1),
                cancel,
            )
            .await
            .expect_err("a cancelled caller receives no metadata");
        assert!(matches!(*observed, BifrostStorageError::Cancelled));
    }
}
