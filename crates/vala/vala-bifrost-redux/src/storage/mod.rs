//! The node's one Bifrost storage owner and its decoded-metadata cache.

pub(crate) mod cache;
mod error;
mod policy;
pub(crate) mod telemetry;

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::FutureExt as _;
use parquet::arrow::async_reader::AsyncFileReader;
use parquet::file::metadata::ParquetMetaData;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::resources::OracleMetadataResources;
pub use crate::storage::cache::{HotMetadataKey, RetainedMetadata};
pub use crate::storage::error::BifrostStorageError;
pub use crate::storage::policy::{BifrostStorageConfig, BifrostStoragePolicy};
pub use crate::storage::telemetry::{
    CacheEffect, CacheEffectReason, MetadataCacheSnapshot, MetadataLoadOutcome, StorageLifecycle,
    TelemetryTransition,
};

use crate::storage::cache::ParquetMetadataCache;
use crate::storage::telemetry::BifrostStorageTelemetry;

/// The node's one storage owner for every Bifrost role co-located on it.
///
/// Dedicated role pods each construct their own owner because they are separate
/// processes; an `All` or `Server` process constructs exactly one and shares it
/// across Scribe, Oracle, and Forge, so the coarse concurrency ceiling is a real
/// node-wide ceiling rather than one per role.
#[derive(Debug)]
pub struct BifrostStorage {
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
}

impl BifrostStorage {
    /// Builds the node's storage owner over one validated policy.
    ///
    /// `serves_oracle` gates the cache alone: a Scribe-only or Forge-only
    /// process still holds the owner and its policy but spends no
    /// metadata-cache budget, because neither reads a hot Parquet footer.
    #[must_use]
    pub fn new(
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
            policy,
            telemetry,
            metadata_cache,
            metadata_resources,
            requests,
            owner: CancellationToken::new(),
        }
    }

    /// Returns the validated policy this owner applies.
    #[must_use]
    pub const fn policy(&self) -> BifrostStoragePolicy {
        self.policy
    }

    /// Returns whether this composition retains decoded metadata at all.
    #[must_use]
    pub const fn metadata_cache_enabled(&self) -> bool {
        self.metadata_cache.is_some()
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
    pub async fn close(&self, deadline: Instant) -> bool {
        self.owner.cancel();
        match self.metadata_cache.as_ref() {
            None => true,
            Some(cache) => cache.close(deadline).await,
        }
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
        if let Some(cache) = self.metadata_cache.as_ref() {
            cache.abort().await;
        }
    }
}
