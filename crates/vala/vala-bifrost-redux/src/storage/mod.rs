//! The node's one Bifrost storage owner and its decoded-metadata cache.

pub(crate) mod cache;
mod error;
mod policy;
pub(crate) mod telemetry;

use std::ops::Range;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use futures_util::FutureExt as _;
use parquet::arrow::async_reader::AsyncFileReader;
use parquet::file::metadata::ParquetMetaData;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use wyrd_storage::handle::StorageHandle;

use crate::resources::OracleMetadataResources;
pub use crate::storage::cache::{HotMetadataKey, RetainedMetadata};
pub use crate::storage::error::BifrostStorageError;
pub use crate::storage::policy::{BifrostStorageConfig, BifrostStoragePolicy};
pub use crate::storage::telemetry::{
    CacheEffect, CacheEffectReason, MetadataCacheSnapshot, MetadataLoadOutcome, StorageLifecycle,
    StorageOperation, StorageRequestOutcome, TelemetryTransition,
};

use crate::storage::cache::ParquetMetadataCache;
use crate::storage::telemetry::BifrostStorageTelemetry;

/// Fixed detail published when an admitted attempt panicked.
///
/// Deliberately says nothing about what panicked: a panic payload is arbitrary
/// text produced anywhere beneath the backend client, and the one thing the
/// owner can state truthfully is that the attempt did not reach a terminal
/// result of its own.
const PANICKED_ATTEMPT: &str = "a governed storage attempt panicked";

/// One admitted logical request, settled exactly once.
///
/// Held by [`BifrostStorage::run_read`] and [`BifrostStorage::run_once`] for
/// the whole logical operation, not per attempt, so a retried read is one start
/// and one terminal. Settlement is checked rather than clamped: a duplicate or
/// missing settlement raises a bounded anomaly, because a live count silently
/// reconciled to zero is exactly how a broken owner reports a clean drain.
struct StorageRequestGuard<'telemetry> {
    /// The owner's one telemetry sink.
    telemetry: &'telemetry BifrostStorageTelemetry,
    /// Which governed operation this request performs.
    operation: StorageOperation,
    /// When the logical request was admitted, for the terminal histogram.
    started: Instant,
    /// Whether a terminal outcome has already been published.
    settled: bool,
}

impl<'telemetry> StorageRequestGuard<'telemetry> {
    /// Admits one logical request and raises the active-request count.
    fn admit(
        telemetry: &'telemetry BifrostStorageTelemetry,
        operation: StorageOperation,
    ) -> Self {
        telemetry.record_request_start(operation);
        Self {
            telemetry,
            operation,
            started: Instant::now(),
            settled: false,
        }
    }

    /// Publishes this request's one terminal outcome.
    ///
    /// A second call publishes nothing and records an anomaly instead, so two
    /// paths cannot disagree about how one request ended.
    fn settle(&mut self, outcome: StorageRequestOutcome) {
        if self.settled {
            self.telemetry
                .record_transition_anomaly(TelemetryTransition::RequestTerminal);
            return;
        }
        self.settled = true;
        self.telemetry
            .record_request_terminal(self.operation, outcome, self.started.elapsed());
    }
}

impl Drop for StorageRequestGuard<'_> {
    /// Records an anomaly for a request that left without a terminal outcome.
    ///
    /// Unreachable on a correct owner: every governed path settles before
    /// returning. Recording it keeps an unsettled request visible in the totals
    /// rather than leaving the active-request count permanently raised with no
    /// explanation.
    fn drop(&mut self) {
        if !self.settled {
            self.telemetry
                .record_transition_anomaly(TelemetryTransition::RequestTerminal);
            self.telemetry.record_request_terminal(
                self.operation,
                StorageRequestOutcome::Backend,
                self.started.elapsed(),
            );
        }
    }
}

/// One attempt's failure and whether the owner may admit another.
///
/// `retryable` is not derived from the error alone: a panicked attempt maps to
/// [`BifrostStorageError::Backend`], which is retryable as a class, but a
/// panic may have left a durable effect half-applied and must never be
/// replayed behind the caller's back.
struct AttemptFailure {
    /// The closed failure this attempt reached.
    error: BifrostStorageError,
    /// Whether another attempt of the same operation is permitted.
    retryable: bool,
}

impl AttemptFailure {
    /// Builds the failure for an attempt the owner refused or stopped.
    const fn terminal(error: BifrostStorageError) -> Self {
        Self {
            error,
            retryable: false,
        }
    }

    /// Builds the failure for a backend result, retryable by its own class.
    fn backend(error: &opendal::Error) -> Self {
        let error = BifrostStorageError::from_opendal(error);
        Self {
            retryable: error.is_retryable(),
            error,
        }
    }

    /// Builds the one-attempt failure for a panicked operation future.
    fn panicked() -> Self {
        Self::terminal(BifrostStorageError::Backend {
            detail: PANICKED_ATTEMPT.to_owned(),
        })
    }
}

/// The node's one storage owner for every Bifrost role co-located on it.
///
/// Dedicated role pods each construct their own owner because they are separate
/// processes; an `All` or `Server` process constructs exactly one and shares it
/// across Scribe, Oracle, and Forge, so the coarse concurrency ceiling is a real
/// node-wide ceiling rather than one per role.
#[derive(Debug)]
pub struct BifrostStorage {
    /// The already-built backend client every operation runs against.
    ///
    /// Owned rather than rebuilt: the handle carries the process's one
    /// configured, credentialed, connection-pooled client, and constructing a
    /// second one here would give Bifrost a different backend identity from the
    /// rest of the server.
    handle: Arc<StorageHandle>,
    /// Validated policy applied to every operation.
    policy: BifrostStoragePolicy,
    /// The single production owner of cache and storage lifecycle signals.
    telemetry: Arc<BifrostStorageTelemetry>,
    /// Decoded-metadata cache, present only for an Oracle-serving composition
    /// with a nonzero budget.
    metadata_cache: Option<Arc<ParquetMetadataCache>>,
    /// Oracle memory root funding transient decodes and retained metadata.
    ///
    /// Absent for a composition that serves no Oracle role: such a node reads
    /// no hot footers, so it neither reserves metadata bytes nor holds a
    /// footer-planning slot.
    metadata_resources: Option<OracleMetadataResources>,
    /// Node-wide ceiling on concurrent backend requests.
    ///
    /// One ceiling for every role co-located in this process, so a node running
    /// Scribe, Oracle, and Forge together cannot issue three roles' worth of
    /// concurrent object-store work against a limit each of them believes it
    /// alone is spending.
    requests: Arc<Semaphore>,
    /// Triggered once when this owner begins shutting down.
    owner: CancellationToken,
    /// Deterministic attempt barrier installed by a test harness.
    ///
    /// Absent in a production build entirely: the field only exists when the
    /// test-support surface is compiled, so no shipped node carries a seam that
    /// can stall its object I/O.
    #[cfg(any(test, feature = "test-support"))]
    test_barrier: std::sync::Mutex<Option<Arc<StorageOperationBarrier>>>,
}

impl BifrostStorage {
    /// Builds the node's storage owner over one validated policy.
    ///
    /// `serves_oracle` gates the cache alone: a Scribe-only or Forge-only
    /// process still holds the owner and its policy but spends no
    /// metadata-cache budget, because neither reads a hot Parquet footer.
    #[must_use]
    pub fn new(
        handle: Arc<StorageHandle>,
        policy: BifrostStoragePolicy,
        metadata_resources: Option<OracleMetadataResources>,
    ) -> Self {
        let telemetry = Arc::new(BifrostStorageTelemetry::default());
        let metadata_cache = metadata_resources
            .clone()
            .filter(|_| policy.metadata_cache_bytes() > 0)
            .map(|resources| {
                Arc::new(ParquetMetadataCache::new(
                    policy.metadata_cache_bytes(),
                    Arc::clone(&telemetry),
                    resources,
                ))
            });
        let requests = Arc::new(Semaphore::new(policy.max_concurrent_requests()));
        Self {
            handle,
            policy,
            telemetry,
            metadata_cache,
            metadata_resources,
            requests,
            owner: CancellationToken::new(),
            #[cfg(any(test, feature = "test-support"))]
            test_barrier: std::sync::Mutex::new(None),
        }
    }

    /// Returns the validated policy this owner applies.
    #[must_use]
    pub const fn policy(&self) -> BifrostStoragePolicy {
        self.policy
    }

    /// Returns the backend client handle this owner runs every operation on.
    ///
    /// Exposed so Scribe and Forge can use this node's one configured client
    /// directly. They deliberately do not go through the owner's read path: a
    /// publication write is not idempotent and must never be retried behind
    /// their backs.
    #[must_use]
    pub fn handle(&self) -> &Arc<StorageHandle> {
        &self.handle
    }

    /// Returns the object-store operator backing this owner.
    ///
    /// This is the exact operator the rest of the server uses, which is what
    /// makes "one storage owner per node" true rather than merely stated.
    #[must_use]
    pub fn operator(&self) -> &opendal::Operator {
        self.handle.operator()
    }

    /// Returns whether this composition retains decoded metadata at all.
    #[must_use]
    pub const fn metadata_cache_enabled(&self) -> bool {
        self.metadata_cache.is_some()
    }

    /// Returns the latest instant a metadata load started now may still run.
    ///
    /// A caller that has no deadline of its own still must not hand
    /// [`Self::hot_metadata`] an unbounded one. This is the owner's own bound —
    /// the same figure `hot_metadata` would clamp any wider deadline to — so a
    /// caller without a budget inherits the process policy rather than
    /// inventing a constant.
    #[must_use]
    pub fn metadata_deadline(&self) -> Instant {
        Instant::now() + self.policy.max_retry_elapsed()
    }

    /// Returns decoded metadata for one immutable hot object.
    ///
    /// With a cache present the decode happens at most once per node per
    /// object, and concurrent callers join one loader. With no cache the same
    /// reader decodes directly and the decision is recorded as
    /// `bypass=disabled`, so a disabled deployment still publishes positive
    /// evidence of the path it took rather than silence. Both paths are bounded
    /// by the caller's deadline and token and by this owner's shutdown.
    ///
    /// `reader` is a factory rather than a reader because reading an immutable
    /// object is idempotent and therefore retryable, and a retried attempt
    /// needs a reader that has not already consumed part of the response.
    ///
    /// The caller must already have validated ticket, binding, full-schema
    /// fingerprint, and the signed descriptor: this operation returns bytes'
    /// structure, never authority.
    ///
    /// # Errors
    /// Returns the loader's closed [`BifrostStorageError`]:
    /// [`BifrostStorageError::RateLimited`] when node-wide admission is full,
    /// [`BifrostStorageError::Cancelled`] or [`BifrostStorageError::Deadline`]
    /// when this caller's bounds elapse, [`BifrostStorageError::Closed`] once
    /// the owner is shutting down, or the backend's own typed failure.
    pub async fn hot_metadata<R, F>(
        &self,
        key: HotMetadataKey,
        reader: F,
        deadline: Instant,
        cancel: CancellationToken,
    ) -> Result<RetainedMetadata, Arc<BifrostStorageError>>
    where
        F: Fn() -> R + Send + Sync + 'static,
        R: AsyncFileReader + Send + 'static,
    {
        let size = key.size_bytes();
        // One immutable bound for the whole operation, chosen before any work
        // starts: the caller's own deadline, or the point at which this owner
        // stops retrying a read, whichever comes first.
        let bound = deadline.min(Instant::now() + self.policy.max_retry_elapsed());
        let Some(cache) = self.metadata_cache.as_ref() else {
            self.telemetry
                .record_cache_effect(CacheEffect::Bypass, CacheEffectReason::Disabled);
            let load = self.governed_decode(reader, size);
            return Self::bounded(load, bound, &cancel, &self.owner)
                .await
                .map(RetainedMetadata::unreserved);
        };
        cache
            .get_or_load(
                key,
                self.governed_decode(reader, size).boxed(),
                bound,
                cancel,
            )
            .await
    }

    /// Builds the governed, retrying decode of one object's metadata.
    ///
    /// Owned by the storage owner rather than the cache because everything it
    /// bounds — the footer-planning slot, node-wide request admission, and the
    /// retry policy — belongs to the process, not to one cache entry. The cache
    /// decides *whether* a decode happens; this decides what a decode is
    /// allowed to spend.
    fn governed_decode<R, F>(
        &self,
        reader: F,
        size: u64,
    ) -> impl std::future::Future<Output = Result<Arc<ParquetMetaData>, BifrostStorageError>>
    + Send
    + 'static
    where
        F: Fn() -> R + Send + Sync + 'static,
        R: AsyncFileReader + Send + 'static,
    {
        let resources = self.metadata_resources.clone();
        let requests = Arc::clone(&self.requests);
        let policy = self.policy;
        async move {
            // The footer slot is the transient decode workspace, and taking it
            // first is what bounds how many decodes a node runs at once: the
            // Oracle root refuses the slot long before the process is out of
            // memory to decode into.
            let _slot = match resources.as_ref() {
                None => None,
                Some(resources) => Some(resources.try_acquire_footer_slot().map_err(|_| {
                    BifrostStorageError::RateLimited {
                        detail: "the Oracle memory root has no free footer slot".to_owned(),
                    }
                })?),
            };
            let mut attempt = 0_u32;
            loop {
                let permit = requests.clone().try_acquire_owned().map_err(|_| {
                    BifrostStorageError::RateLimited {
                        detail: "node-wide storage request admission is full".to_owned(),
                    }
                })?;
                let decoded = tokio::time::timeout(
                    policy.request_timeout(),
                    Self::decode_metadata(reader(), size),
                )
                .await
                .unwrap_or_else(|_| {
                    Err(BifrostStorageError::Timeout {
                        detail: "the object's footer did not decode within the request timeout"
                            .to_owned(),
                    })
                });
                drop(permit);
                let error = match decoded {
                    Ok(metadata) => return Ok(metadata),
                    Err(error) => error,
                };
                if attempt >= policy.max_retries() || !error.is_retryable() {
                    return Err(error);
                }
                attempt += 1;
                // Exponential, and deliberately not jittered here: the caller's
                // absolute bound already caps the total, and the retry count is
                // small enough that spreading a herd matters less than staying
                // predictable. Cancellation during the backoff is the caller's
                // own bound and is applied by `bounded`.
                tokio::time::sleep(Duration::from_millis(50) * 2_u32.pow(attempt.min(8))).await;
            }
        }
    }

    /// Bounds one operation by a deadline, a caller token, and owner shutdown.
    ///
    /// Used by the cache-disabled path, which has no loader task to supervise
    /// it. The enabled path gets the same bounds from the cache's own
    /// supervision, so this deliberately does not wrap it twice.
    async fn bounded<T>(
        operation: impl std::future::Future<Output = Result<T, BifrostStorageError>>,
        deadline: Instant,
        cancel: &CancellationToken,
        owner: &CancellationToken,
    ) -> Result<T, Arc<BifrostStorageError>> {
        tokio::select! {
            biased;
            () = owner.cancelled() => Err(Arc::new(BifrostStorageError::Closed)),
            () = cancel.cancelled() => Err(Arc::new(BifrostStorageError::Cancelled)),
            () = tokio::time::sleep_until(deadline.into()) => {
                Err(Arc::new(BifrostStorageError::Deadline))
            }
            completed = operation => completed.map_err(Arc::new),
        }
    }

    /// Decodes one object's Parquet metadata through the caller's reader.
    ///
    /// Kept as an associated function so it can be boxed into the cache's
    /// retained loader task without borrowing the owner.
    ///
    /// # Errors
    /// Returns [`BifrostStorageError::InvalidData`] when the object's footer
    /// does not decode. Range failures reach here already wrapped by the
    /// caller's reader.
    async fn decode_metadata<R>(
        mut reader: R,
        size: u64,
    ) -> Result<Arc<ParquetMetaData>, BifrostStorageError>
    where
        R: AsyncFileReader + Send + 'static,
    {
        parquet::file::metadata::ParquetMetaDataReader::new()
            .load_and_finish(&mut reader, size)
            .await
            .map(Arc::new)
            .map_err(|error| BifrostStorageError::from_parquet(&error))
    }

    /// Probes whether one object exists beneath this owner's backend.
    ///
    /// # Errors
    /// Returns the closed [`BifrostStorageError`] the governed read reached:
    /// [`BifrostStorageError::Closed`] when the owner is shutting down,
    /// [`BifrostStorageError::RateLimited`] when node-wide admission is full,
    /// [`BifrostStorageError::Timeout`] for one attempt's timeout,
    /// [`BifrostStorageError::Deadline`] when the fixed retry bound elapses, or
    /// the backend's own classified failure.
    pub async fn exists(&self, key: &str) -> Result<bool, BifrostStorageError> {
        self.run_read(StorageOperation::Exists, || self.operator().exists(key))
            .await
    }

    /// Returns one object's content length.
    ///
    /// # Errors
    /// Same closed vocabulary as [`Self::exists`].
    pub async fn stat(&self, key: &str) -> Result<u64, BifrostStorageError> {
        self.run_read(StorageOperation::Stat, || async {
            self.operator()
                .stat(key)
                .await
                .map(|metadata| metadata.content_length())
        })
        .await
    }

    /// Reads one object in full.
    ///
    /// # Errors
    /// Same closed vocabulary as [`Self::exists`].
    pub async fn read(&self, key: &str) -> Result<Bytes, BifrostStorageError> {
        self.run_read(StorageOperation::Read, || async {
            self.operator()
                .read(key)
                .await
                .map(|buffer| buffer.to_bytes())
        })
        .await
    }

    /// Reads one byte range of an object.
    ///
    /// # Errors
    /// Same closed vocabulary as [`Self::exists`].
    pub async fn read_range(
        &self,
        key: &str,
        range: Range<u64>,
    ) -> Result<Bytes, BifrostStorageError> {
        self.run_read(StorageOperation::ReadRange, || async {
            self.operator()
                .read_with(key)
                .range(range.clone())
                .await
                .map(|buffer| buffer.to_bytes())
        })
        .await
    }

    /// Lists the entries beneath one prefix.
    ///
    /// The complete listing is materialized before returning, so no request
    /// permit is held for a consumer-driven stream's lifetime.
    ///
    /// # Errors
    /// Same closed vocabulary as [`Self::exists`].
    pub async fn list(
        &self,
        prefix: &str,
        recursive: bool,
    ) -> Result<Vec<opendal::Entry>, BifrostStorageError> {
        self.run_read(StorageOperation::List, || async {
            self.operator().list_with(prefix).recursive(recursive).await
        })
        .await
    }

    /// Writes one object's complete bytes as a single, unrepeated effect.
    ///
    /// # Errors
    /// Returns [`BifrostStorageError::Closed`],
    /// [`BifrostStorageError::RateLimited`], [`BifrostStorageError::Timeout`],
    /// or the backend's classified failure. Never retried: a replayed
    /// publication is how a duplicate data file reaches a snapshot.
    pub async fn write_once(&self, key: &str, bytes: Bytes) -> Result<(), BifrostStorageError> {
        self.run_once(StorageOperation::Write, async {
            self.operator().write(key, bytes).await.map(|_| ())
        })
        .await
    }

    /// Opens one streaming writer as a single, unrepeated effect.
    ///
    /// # Errors
    /// Same closed vocabulary as [`Self::write_once`].
    pub async fn open_writer_once(
        &self,
        key: &str,
    ) -> Result<opendal::Writer, BifrostStorageError> {
        self.run_once(StorageOperation::OpenWriter, async {
            self.operator().writer(key).await
        })
        .await
    }

    /// Appends one buffer through an already-open writer, once.
    ///
    /// # Errors
    /// Same closed vocabulary as [`Self::write_once`].
    pub async fn writer_write_once(
        &self,
        writer: &mut opendal::Writer,
        bytes: Bytes,
    ) -> Result<(), BifrostStorageError> {
        self.run_once(StorageOperation::WriterWrite, writer.write(bytes))
            .await
    }

    /// Finalizes an already-open writer, once.
    ///
    /// # Errors
    /// Same closed vocabulary as [`Self::write_once`].
    pub async fn writer_close_once(
        &self,
        writer: &mut opendal::Writer,
    ) -> Result<(), BifrostStorageError> {
        self.run_once(StorageOperation::WriterClose, async {
            writer.close().await.map(|_| ())
        })
        .await
    }

    /// Deletes one object, once.
    ///
    /// # Errors
    /// Same closed vocabulary as [`Self::write_once`].
    pub async fn delete_once(&self, key: &str) -> Result<(), BifrostStorageError> {
        self.run_once(StorageOperation::Delete, self.operator().delete(key))
            .await
    }

    /// Deletes every object beneath one prefix, once.
    ///
    /// # Errors
    /// Same closed vocabulary as [`Self::write_once`].
    pub async fn delete_prefix_once(&self, key: &str) -> Result<(), BifrostStorageError> {
        self.run_once(StorageOperation::DeletePrefix, async {
            self.operator().delete_with(key).recursive(true).await
        })
        .await
    }

    /// Runs one idempotent read under the owner's complete governance.
    ///
    /// The absolute bound is fixed once, at entry, so a sequence of attempts
    /// can never extend its own budget. Each attempt takes its own request
    /// permit and races, in this precedence, owner cancellation, that fixed
    /// bound, the per-attempt request timeout, and the operation itself; the
    /// losing future is dropped before the method returns or retries, so no
    /// backend work and no permit survives a terminal result. Another attempt
    /// is admitted only while the failure's own class is retryable, the
    /// operation is idempotent, retries performed remain strictly below the
    /// policy's ceiling, the owner is open, and the fixed bound remains.
    ///
    /// The whole logical read publishes exactly one telemetry start and one
    /// terminal regardless of attempt count; every attempt after the first
    /// publishes one retry.
    ///
    /// # Errors
    /// Returns the closed [`BifrostStorageError`] of the last attempt, or
    /// [`BifrostStorageError::Closed`], [`BifrostStorageError::RateLimited`],
    /// or [`BifrostStorageError::Deadline`] when the owner refused, admission
    /// was full, or the fixed bound elapsed.
    async fn run_read<T, Fut>(
        &self,
        operation: StorageOperation,
        attempt: impl Fn() -> Fut,
    ) -> Result<T, BifrostStorageError>
    where
        Fut: std::future::Future<Output = Result<T, opendal::Error>>,
    {
        let mut guard = StorageRequestGuard::admit(&self.telemetry, operation);
        let result = self.read_attempts(operation, attempt).await;
        guard.settle(match &result {
            Ok(_) => StorageRequestOutcome::Success,
            Err(error) => error.request_outcome(),
        });
        result
    }

    /// Drives one read's bounded attempt sequence without owning its telemetry.
    ///
    /// Split from [`Self::run_read`] so the one logical start and terminal are
    /// published around the whole sequence rather than once per attempt.
    ///
    /// # Errors
    /// Returns the same closed vocabulary as [`Self::run_read`].
    async fn read_attempts<T, Fut>(
        &self,
        operation: StorageOperation,
        attempt: impl Fn() -> Fut,
    ) -> Result<T, BifrostStorageError>
    where
        Fut: std::future::Future<Output = Result<T, opendal::Error>>,
    {
        let bound = Instant::now() + self.policy.max_retry_elapsed();
        let mut retries_performed = 0_u32;
        loop {
            if self.owner.is_cancelled() {
                return Err(BifrostStorageError::Closed);
            }
            if Instant::now() >= bound {
                return Err(BifrostStorageError::Deadline);
            }
            let failure = match self.attempt_once(operation, Some(bound), &attempt).await {
                Ok(value) => return Ok(value),
                Err(failure) => failure,
            };
            if !failure.retryable
                || !operation.is_retryable()
                || retries_performed >= self.policy.max_retries()
            {
                return Err(failure.error);
            }
            let backoff =
                Duration::from_millis(50) * 2_u32.pow(retries_performed.saturating_add(1).min(8));
            tokio::select! {
                biased;
                () = self.owner.cancelled() => return Err(BifrostStorageError::Closed),
                () = tokio::time::sleep_until(bound.into()) => {
                    return Err(BifrostStorageError::Deadline);
                }
                () = tokio::time::sleep(backoff) => {}
            }
            retries_performed = retries_performed.saturating_add(1);
            self.telemetry.record_request_retry(operation);
        }
    }

    /// Runs one non-idempotent effect under the owner's governance, once.
    ///
    /// Same admission, cancellation, and per-attempt timeout as a read, and
    /// deliberately no absolute retry bound and no second attempt: the owner
    /// cannot distinguish an effect that never landed from one whose
    /// acknowledgement was lost, so replaying it is how a duplicate object or a
    /// second delete happens.
    ///
    /// # Errors
    /// Returns [`BifrostStorageError::Closed`],
    /// [`BifrostStorageError::RateLimited`], [`BifrostStorageError::Timeout`],
    /// or the backend's classified failure.
    async fn run_once<T, Fut>(
        &self,
        operation: StorageOperation,
        attempt: Fut,
    ) -> Result<T, BifrostStorageError>
    where
        Fut: std::future::Future<Output = Result<T, opendal::Error>>,
    {
        let mut guard = StorageRequestGuard::admit(&self.telemetry, operation);
        let result = if self.owner.is_cancelled() {
            Err(BifrostStorageError::Closed)
        } else {
            let mut attempt = Some(attempt);
            self.attempt_once(operation, None, || {
                attempt
                    .take()
                    .expect("run_once admits exactly one attempt of its operation")
            })
            .await
            .map_err(|failure| failure.error)
        };
        guard.settle(match &result {
            Ok(_) => StorageRequestOutcome::Success,
            Err(error) => error.request_outcome(),
        });
        result
    }

    /// Admits and races exactly one attempt of a governed operation.
    ///
    /// The race is biased in the fixed precedence order — owner cancellation,
    /// the absolute read bound when one applies, the per-attempt request
    /// timeout, then completion — so the absolute bound terminates an in-flight
    /// attempt even when its own timeout has not elapsed, and a request timeout
    /// wins only when it truly occurs first. A panic beneath the backend client
    /// is caught here and reported as a one-attempt backend failure, because a
    /// panicked effect may already have been durable.
    ///
    /// # Errors
    /// Returns [`AttemptFailure`] carrying the closed error and whether the
    /// owner may admit another attempt.
    async fn attempt_once<T, Fut>(
        &self,
        operation: StorageOperation,
        bound: Option<Instant>,
        attempt: impl FnOnce() -> Fut,
    ) -> Result<T, AttemptFailure>
    where
        Fut: std::future::Future<Output = Result<T, opendal::Error>>,
    {
        let permit = self
            .requests
            .clone()
            .try_acquire_owned()
            .map_err(|_| {
                AttemptFailure::terminal(BifrostStorageError::RateLimited {
                    detail: "node-wide storage request admission is full".to_owned(),
                })
            })?;
        let deadline = bound.unwrap_or_else(|| Instant::now() + self.policy.max_retry_elapsed());
        tracing::trace!(
            operation = operation.as_str(),
            "Bifrost storage admitted one governed attempt"
        );
        let raced = tokio::select! {
            biased;
            () = self.owner.cancelled() => Err(AttemptFailure::terminal(BifrostStorageError::Closed)),
            () = tokio::time::sleep_until(deadline.into()), if bound.is_some() => {
                Err(AttemptFailure::terminal(BifrostStorageError::Deadline))
            }
            () = tokio::time::sleep(self.policy.request_timeout()) => {
                Err(AttemptFailure {
                    error: BifrostStorageError::Timeout {
                        detail: "the backend did not answer within the request timeout".to_owned(),
                    },
                    retryable: true,
                })
            }
            outcome = AssertUnwindSafe(async {
                #[cfg(any(test, feature = "test-support"))]
                self.pause_for_test(operation).await;
                attempt().await
            })
            .catch_unwind() => match outcome {
                Ok(Ok(value)) => Ok(value),
                Ok(Err(error)) => Err(AttemptFailure::backend(&error)),
                Err(_) => Err(AttemptFailure::panicked()),
            },
        };
        drop(permit);
        raced
    }

    /// Pauses one admitted attempt at an installed deterministic barrier.
    ///
    /// Test-support only, and deliberately incapable of producing a result: it
    /// may only delay or release the real operation, so every byte, cache
    /// effect, retry, lifecycle transition, and terminal outcome a test then
    /// observes came from the production path.
    #[cfg(any(test, feature = "test-support"))]
    async fn pause_for_test(&self, operation: StorageOperation) {
        let barrier = self
            .test_barrier
            .lock()
            .ok()
            .and_then(|barrier| barrier.clone());
        if let Some(barrier) = barrier {
            barrier.pause(operation).await;
        }
    }

    /// Installs the deterministic attempt barrier this owner honors.
    ///
    /// Replaces any previously installed barrier. Test-support only.
    #[cfg(any(test, feature = "test-support"))]
    pub fn install_operation_barrier_for_test(&self, barrier: Arc<StorageOperationBarrier>) {
        if let Ok(mut installed) = self.test_barrier.lock() {
            *installed = Some(barrier);
        }
    }

    /// Returns request permits this owner currently has free.
    ///
    /// Test-support only: it exists so a test can prove a terminal attempt
    /// released its admission rather than leaking it into the node's ceiling.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn available_request_permits(&self) -> usize {
        self.requests.available_permits()
    }

    /// Returns the retained reconciliation snapshot.
    ///
    /// Test-support only: production observation is the emitted metrics and
    /// events, and exposing the totals to production callers would invite a
    /// second source of truth.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn telemetry_snapshot(&self) -> MetadataCacheSnapshot {
        self.telemetry.snapshot()
    }

    /// Closes cache admission and settles every retained loader by `deadline`.
    ///
    /// Called only after public and query admission are closed and query owners
    /// have drained, so a load still outstanding here belongs to work that was
    /// already cancelled. Idempotent.
    ///
    /// The lifecycle transitions are published here rather than by the cache so
    /// a composition with no cache — a Scribe-only node, or an Oracle node
    /// booted with an explicit zero budget — still reports `Closed` on a clean
    /// teardown instead of remaining indistinguishable from a node that never
    /// shut down.
    pub async fn close(&self, deadline: Instant) -> bool {
        self.owner.cancel();
        self.telemetry.record_lifecycle(StorageLifecycle::Closing);
        let settled = match self.metadata_cache.as_ref() {
            None => true,
            Some(cache) => cache.close(deadline).await,
        };
        if settled {
            self.telemetry.record_lifecycle(StorageLifecycle::Closed);
        }
        settled
    }

    /// Closes cache admission immediately, aborting every retained loader.
    ///
    /// Idempotent, and safe after [`Self::close`]: both share the cache's one
    /// close completion, so a second caller observes the first one's outcome.
    ///
    /// Awaits every aborted loader, so once this returns no retained task is
    /// still running against the owner's state.
    pub async fn abort(&self) {
        self.owner.cancel();
        self.telemetry.record_lifecycle(StorageLifecycle::Closing);
        if let Some(cache) = self.metadata_cache.as_ref() {
            cache.abort().await;
        }
        self.telemetry.record_lifecycle(StorageLifecycle::Closed);
    }
}

/// A deterministic pause installed immediately before one governed operation
/// reaches `OpenDAL`.
///
/// Exists so a journey can hold a real backend request open long enough to
/// cancel the server underneath it. It carries no result of its own: the only
/// thing it can do is delay and release the production operation, so the bytes,
/// cache effects, retries, lifecycle transitions, and terminal outcomes a test
/// observes afterwards are the ones the production path produced.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug)]
pub struct StorageOperationBarrier {
    /// The one operation this barrier stalls; every other passes untouched.
    operation: StorageOperation,
    /// Fires once an admitted attempt of that operation has reached the pause.
    reached: CancellationToken,
    /// Fires when the test releases every stalled attempt.
    released: CancellationToken,
}

#[cfg(any(test, feature = "test-support"))]
impl StorageOperationBarrier {
    /// Builds a barrier that stalls exactly one governed operation.
    #[must_use]
    pub fn new(operation: StorageOperation) -> Arc<Self> {
        Arc::new(Self {
            operation,
            reached: CancellationToken::new(),
            released: CancellationToken::new(),
        })
    }

    /// Waits until one admitted attempt of the stalled operation has arrived.
    pub async fn wait_until_reached(&self) {
        self.reached.cancelled().await;
    }

    /// Reports whether an admitted attempt has already reached the barrier.
    #[must_use]
    pub fn was_reached(&self) -> bool {
        self.reached.is_cancelled()
    }

    /// Releases every stalled attempt and stops stalling future ones.
    pub fn release(&self) {
        self.released.cancel();
    }

    /// Stalls one admitted attempt when it matches this barrier's operation.
    ///
    /// Returns immediately for every other operation and once released, so a
    /// barrier installed for a ranged read never delays an unrelated write.
    async fn pause(&self, operation: StorageOperation) {
        if operation != self.operation {
            return;
        }
        self.reached.cancel();
        self.released.cancelled().await;
    }
}

#[cfg(test)]
mod governed_request_tests {
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use super::*;

    /// Marks its flag when dropped, proving a losing attempt future was released.
    struct DropSentinel(Arc<AtomicBool>);

    impl Drop for DropSentinel {
        /// Records that the attempt future this sentinel lives in was dropped.
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    /// Builds one storage owner over a real local backend at `root`.
    ///
    /// A real handle rather than a stub: the governed workflows are only
    /// meaningful against the operator a node actually holds, and the injected
    /// failures below are supplied by the attempt closure rather than by a fake
    /// backend, so nothing about admission, retry, or settlement is simulated.
    ///
    /// # Panics
    /// Panics when the local signer or the supplied policy is invalid.
    fn owner(root: &Path, config: BifrostStorageConfig) -> Arc<BifrostStorage> {
        let signer = wyrd_storage::signer::BackendSigner::Local(
            wyrd_storage::local::LocalSigner::new(root.to_path_buf()).expect("local signer"),
        );
        Arc::new(BifrostStorage::new(
            Arc::new(wyrd_storage::handle::StorageHandle::new(signer)),
            BifrostStoragePolicy::resolve(config, u64::from(u32::MAX), false)
                .expect("the supplied storage policy is valid"),
            None,
        ))
    }

    /// Returns one injected retryable backend failure.
    fn transient() -> opendal::Error {
        opendal::Error::new(opendal::ErrorKind::Unexpected, "injected transient failure")
    }

    /// A governed read spends exactly the attempts its owner's policy allows,
    /// stops at the one absolute bound fixed when it began, and leaves nothing
    /// behind when that bound wins.
    ///
    /// The bound is the load-bearing part. A retry sequence that re-derived its
    /// deadline from each attempt could extend itself indefinitely while every
    /// individual attempt still looked bounded, so the test stalls the last
    /// admitted attempt past the elapsed ceiling and requires `Deadline` rather
    /// than the later per-attempt timeout, the losing future to be dropped, the
    /// request permit to be released, and no further backend call.
    ///
    /// # Panics
    /// Panics when the attempt count, retry count, terminal error, or released
    /// state does not match.
    #[tokio::test]
    async fn iceberg_read_retry_is_bounded_by_owner_policy() {
        let root = tempfile::tempdir().expect("warehouse root");
        let storage = owner(
            root.path(),
            BifrostStorageConfig {
                request_timeout_ms: Some(1_000),
                max_retries: Some(2),
                max_retry_elapsed_ms: Some(1_000),
                ..BifrostStorageConfig::default()
            },
        );
        let attempts = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let error = storage
            .run_read(StorageOperation::Read, || {
                let attempts = Arc::clone(&attempts);
                let dropped = Arc::clone(&dropped);
                async move {
                    let attempt = attempts.fetch_add(1, Ordering::SeqCst) + 1;
                    if attempt < 3 {
                        return Err::<Bytes, _>(transient());
                    }
                    let _sentinel = DropSentinel(dropped);
                    tokio::time::sleep(Duration::from_secs(600)).await;
                    Err(transient())
                }
            })
            .await
            .expect_err("the fixed absolute bound terminates the sequence");

        assert_eq!(error, BifrostStorageError::Deadline);
        assert_eq!(
            attempts.load(Ordering::SeqCst),
            3,
            "one initial attempt plus exactly max_retries retries, and no fourth"
        );
        assert!(
            dropped.load(Ordering::SeqCst),
            "the losing attempt future must be dropped when the bound wins"
        );
        let snapshot = storage.telemetry_snapshot();
        assert_eq!(snapshot.request_retries(), 2);
        assert_eq!(snapshot.request_starts(), 1, "a retried read is one request");
        assert_eq!(
            snapshot.request_terminal(StorageRequestOutcome::Deadline),
            1
        );
        assert_eq!(snapshot.active_requests(), 0);
        assert_eq!(snapshot.anomalies(), 0);
        assert_eq!(
            storage.available_request_permits(),
            storage.policy().max_concurrent_requests(),
            "the terminal attempt must release its admission"
        );

        let timeout_root = tempfile::tempdir().expect("warehouse root");
        let bounded = owner(
            timeout_root.path(),
            BifrostStorageConfig {
                request_timeout_ms: Some(100),
                max_retries: Some(0),
                max_retry_elapsed_ms: Some(1_000),
                ..BifrostStorageConfig::default()
            },
        );
        let attempts = Arc::new(AtomicUsize::new(0));
        let error = bounded
            .run_read(StorageOperation::Read, || {
                let attempts = Arc::clone(&attempts);
                async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_secs(600)).await;
                    Err::<Bytes, _>(transient())
                }
            })
            .await
            .expect_err("the per-attempt timeout terminates the only attempt");
        assert!(
            matches!(error, BifrostStorageError::Timeout { .. }),
            "a request timeout that occurs first must not be reported as the elapsed bound"
        );
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        assert_eq!(bounded.telemetry_snapshot().request_retries(), 0);
    }

    /// Every non-idempotent effect is admitted exactly once, whatever it returns.
    ///
    /// A transparently replayed write, writer append, close, or delete is how a
    /// duplicate data file reaches a snapshot or a live object disappears, so
    /// the owner must refuse a second attempt even for a failure class it would
    /// happily retry on a read. The injected failure is deliberately the
    /// retryable one.
    ///
    /// # Panics
    /// Panics when an effect is attempted more than once or an effect operation
    /// declares itself retryable.
    #[tokio::test]
    async fn iceberg_mutations_are_attempted_once() {
        let root = tempfile::tempdir().expect("warehouse root");
        let storage = owner(
            root.path(),
            BifrostStorageConfig {
                max_retries: Some(5),
                ..BifrostStorageConfig::default()
            },
        );
        for effect in [
            StorageOperation::Write,
            StorageOperation::OpenWriter,
            StorageOperation::WriterWrite,
            StorageOperation::WriterClose,
            StorageOperation::Delete,
            StorageOperation::DeletePrefix,
        ] {
            assert!(
                !effect.is_retryable(),
                "{} must never be transparently replayed",
                effect.as_str()
            );
            let attempts = Arc::new(AtomicUsize::new(0));
            let counted = Arc::clone(&attempts);
            let error = storage
                .run_once(effect, async move {
                    counted.fetch_add(1, Ordering::SeqCst);
                    Err::<(), _>(transient())
                })
                .await
                .expect_err("the injected effect fails");
            assert!(matches!(error, BifrostStorageError::Backend { .. }));
            assert_eq!(
                attempts.load(Ordering::SeqCst),
                1,
                "{} was attempted more than once",
                effect.as_str()
            );
        }
        let snapshot = storage.telemetry_snapshot();
        assert_eq!(snapshot.request_starts(), 6);
        assert_eq!(snapshot.request_terminals(), 6);
        assert_eq!(snapshot.request_retries(), 0);
        assert_eq!(snapshot.active_requests(), 0);
        assert_eq!(snapshot.anomalies(), 0);
    }

    /// Every terminal a governed request can reach settles its own accounting.
    ///
    /// Asserted as one matrix because the property under test is not any single
    /// outcome but the invariant across all of them: one logical request
    /// publishes one start and one terminal, active work returns to zero, and
    /// no settlement is unmatched. An owner that leaked exactly one path would
    /// still look correct from every other path's side.
    ///
    /// # Panics
    /// Panics when an expected terminal is not published or the totals do not
    /// reconcile after a case.
    #[tokio::test]
    async fn storage_request_telemetry_reconciles_terminal_matrix() {
        let root = tempfile::tempdir().expect("warehouse root");

        let storage = owner(root.path(), BifrostStorageConfig::default());
        storage
            .run_read(StorageOperation::Exists, || async { Ok(true) })
            .await
            .expect("a successful read returns its value");
        assert_reconciled(&storage, StorageRequestOutcome::Success, 1);

        let attempts = Arc::new(AtomicUsize::new(0));
        storage
            .run_read(StorageOperation::Read, || {
                let attempts = Arc::clone(&attempts);
                async move {
                    if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                        return Err(transient());
                    }
                    Ok(Bytes::from_static(b"rows"))
                }
            })
            .await
            .expect("a retried read still succeeds");
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        assert_eq!(storage.telemetry_snapshot().request_retries(), 1);
        assert_reconciled(&storage, StorageRequestOutcome::Success, 2);

        let error = storage
            .run_once(StorageOperation::Delete, async {
                Err::<(), _>(opendal::Error::new(
                    opendal::ErrorKind::NotFound,
                    "injected missing object",
                ))
            })
            .await
            .expect_err("a missing object fails");
        assert!(matches!(error, BifrostStorageError::NotFound { .. }));
        assert_reconciled(&storage, StorageRequestOutcome::NotFound, 1);

        let panicking = storage
            .run_once(StorageOperation::Write, async {
                panic!("an injected panic beneath the backend client");
                #[allow(unreachable_code)]
                Ok::<(), opendal::Error>(())
            })
            .await
            .expect_err("a panicked attempt fails rather than unwinding the owner");
        assert_eq!(
            panicking,
            BifrostStorageError::Backend {
                detail: PANICKED_ATTEMPT.to_owned(),
            }
        );
        assert_reconciled(&storage, StorageRequestOutcome::Backend, 1);

        let timeout_root = tempfile::tempdir().expect("warehouse root");
        let timing = owner(
            timeout_root.path(),
            BifrostStorageConfig {
                request_timeout_ms: Some(100),
                max_retries: Some(0),
                max_retry_elapsed_ms: Some(1_000),
                max_concurrent_requests: Some(1),
                ..BifrostStorageConfig::default()
            },
        );
        let error = timing
            .run_read(StorageOperation::Read, || async {
                tokio::time::sleep(Duration::from_secs(600)).await;
                Ok::<Bytes, opendal::Error>(Bytes::new())
            })
            .await
            .expect_err("a stalled attempt times out");
        assert!(matches!(error, BifrostStorageError::Timeout { .. }));
        assert_reconciled(&timing, StorageRequestOutcome::Timeout, 1);

        let barrier = StorageOperationBarrier::new(StorageOperation::Exists);
        timing.install_operation_barrier_for_test(Arc::clone(&barrier));
        let stalled = tokio::spawn({
            let timing = Arc::clone(&timing);
            async move {
                timing
                    .run_read(StorageOperation::Exists, || async { Ok(true) })
                    .await
            }
        });
        barrier.wait_until_reached().await;
        let refused = timing
            .run_read(StorageOperation::Stat, || async { Ok(0_u64) })
            .await
            .expect_err("the node's one request slot is already spent");
        assert!(matches!(refused, BifrostStorageError::RateLimited { .. }));
        let refused_snapshot = timing.telemetry_snapshot();
        assert_eq!(
            refused_snapshot.request_terminal(StorageRequestOutcome::RateLimited),
            1
        );
        assert_eq!(
            refused_snapshot.active_requests(),
            1,
            "only the stalled read remains admitted while the refusal settles"
        );
        assert_eq!(refused_snapshot.anomalies(), 0);

        timing.abort().await;
        assert_eq!(
            stalled
                .await
                .expect("the stalled read joins")
                .expect_err("owner cancellation terminates the stalled attempt"),
            BifrostStorageError::Closed,
            "an admitted attempt must observe owner cancellation"
        );
        barrier.release();
        assert_eq!(
            timing
                .run_read(StorageOperation::Read, || async {
                    Ok::<Bytes, opendal::Error>(Bytes::new())
                })
                .await
                .expect_err("a closed owner admits nothing"),
            BifrostStorageError::Closed
        );
        let snapshot = timing.telemetry_snapshot();
        assert_eq!(snapshot.request_terminal(StorageRequestOutcome::Closed), 2);
        assert_eq!(snapshot.request_starts(), snapshot.request_terminals());
        assert_eq!(snapshot.active_requests(), 0);
        assert_eq!(snapshot.anomalies(), 0);
    }

    /// Asserts one owner published the expected terminal and reconciles.
    ///
    /// # Panics
    /// Panics when the terminal count differs, starts and terminals disagree,
    /// active work is nonzero, or any settlement was unmatched.
    fn assert_reconciled(
        storage: &BifrostStorage,
        outcome: StorageRequestOutcome,
        expected: u64,
    ) {
        let snapshot = storage.telemetry_snapshot();
        assert_eq!(
            snapshot.request_terminal(outcome),
            expected,
            "{} terminals",
            outcome.as_str()
        );
        assert_eq!(snapshot.request_starts(), snapshot.request_terminals());
        assert_eq!(snapshot.active_requests(), 0);
        assert_eq!(snapshot.anomalies(), 0);
    }
}
