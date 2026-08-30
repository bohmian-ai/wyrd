//! The node's one Bifrost storage owner and its decoded-metadata cache.

pub(crate) mod cache;
mod error;
mod policy;
pub(crate) mod telemetry;

use std::sync::Arc;
use std::time::Instant;

use parquet::arrow::async_reader::AsyncFileReader;
use parquet::file::metadata::ParquetMetaData;
use tokio_util::sync::CancellationToken;

pub use crate::storage::cache::HotMetadataKey;
pub use crate::storage::error::BifrostStorageError;
pub use crate::storage::policy::BifrostStoragePolicy;
pub use crate::storage::telemetry::{
    CacheEffect, CacheEffectReason, MetadataCacheSnapshot, MetadataLoadOutcome, StorageLifecycle,
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
}

impl BifrostStorage {
    /// Builds the node's storage owner over one validated policy.
    ///
    /// `serves_oracle` gates the cache alone: a Scribe-only or Forge-only
    /// process still holds the owner and its policy but spends no
    /// metadata-cache budget, because neither reads a hot Parquet footer.
    #[must_use]
    pub fn new(policy: BifrostStoragePolicy, serves_oracle: bool) -> Self {
        let telemetry = Arc::new(BifrostStorageTelemetry::default());
        let metadata_cache = (serves_oracle && policy.metadata_cache_bytes() > 0).then(|| {
            Arc::new(ParquetMetadataCache::new(
                policy.metadata_cache_bytes(),
                Arc::clone(&telemetry),
            ))
        });
        Self {
            policy,
            telemetry,
            metadata_cache,
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
    /// evidence of the path it took rather than silence.
    ///
    /// The caller must already have validated ticket, binding, full-schema
    /// fingerprint, and the signed descriptor: this operation returns bytes'
    /// structure, never authority.
    ///
    /// # Errors
    /// Returns the loader's closed [`BifrostStorageError`].
    pub async fn hot_metadata<R>(
        &self,
        key: HotMetadataKey,
        reader: R,
        deadline: Instant,
        cancel: CancellationToken,
    ) -> Result<Arc<ParquetMetaData>, Arc<BifrostStorageError>>
    where
        R: AsyncFileReader + Send + 'static,
    {
        let size = key.size_bytes();
        let Some(cache) = self.metadata_cache.as_ref() else {
            self.telemetry
                .record_cache_effect(CacheEffect::Bypass, CacheEffectReason::Disabled);
            return Self::decode_metadata(reader, size).await.map_err(Arc::new);
        };
        cache
            .get_or_load(
                key,
                Box::pin(Self::decode_metadata(reader, size)),
                deadline,
                cancel,
            )
            .await
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
        match self.metadata_cache.as_ref() {
            None => true,
            Some(cache) => cache.close(deadline).await,
        }
    }

    /// Closes cache admission immediately, aborting every retained loader.
    ///
    /// Idempotent, and safe after [`Self::close`].
    pub fn abort(&self) {
        if let Some(cache) = self.metadata_cache.as_ref() {
            cache.abort();
        }
    }
}
