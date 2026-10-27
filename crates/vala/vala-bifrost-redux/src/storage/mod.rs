//! The node's one Bifrost storage owner.

mod error;
mod footer_cache;
mod policy;
mod range_cache;

use std::sync::Arc;

use datafusion::execution::cache::cache_manager::FileMetadataCache;
use object_store::ObjectStore;
use object_store::limit::LimitStore;
use object_store::local::LocalFileSystem;
use wyrd_storage::BackendConfig;
#[cfg(feature = "cloud")]
use wyrd_storage::CloudStore;

use crate::resources::{BifrostResourceError, MetadataReservation, OracleMetadataResources};
use crate::storage::footer_cache::FooterCache;
pub(crate) use crate::storage::range_cache::RangeCache;

pub use crate::storage::error::BifrostStorageError;
pub(crate) use crate::storage::error::error_chain_contains_not_found;
pub use crate::storage::policy::{BifrostStorageConfig, BifrostStoragePolicy};

/// The node's one Bifrost object store and its Parquet footer and byte-range caches.
///
/// Every Bifrost role reads and writes through [`Self::objects`]: Scribe
/// uploads, Oracle's `DataFusion` scans, Forge rewrites and cleanup, and the
/// Iceberg catalog's `FileIO`. The store is `object_store`'s own client for the
/// configured backend behind one node-wide [`LimitStore`]; timeouts and
/// retries are the cloud clients' own. The client is built by
/// `wyrd_storage::CloudStore` constructors, the same ones its card and
/// artifact storage uses.
#[derive(Debug)]
pub struct BifrostStorage {
    /// The backend store every Bifrost operation runs against, rooted at the
    /// warehouse.
    objects: Arc<dyn ObjectStore>,
    /// The backend the store was built for; the Iceberg catalog derives its
    /// warehouse location from it.
    backend: BackendConfig,
    /// Validated policy the store was built with.
    policy: BifrostStoragePolicy,
    /// `DataFusion`'s node-wide Parquet footer cache, shared by every Oracle
    /// scan on this node; absent on a node that serves no Oracle or has a zero
    /// budget.
    metadata_cache: Option<Arc<FileMetadataCache>>,
    /// The whole cache budget, charged once to the Oracle memory root and held
    /// for the owner's life, so footers are never an ungoverned second pool.
    metadata_charge: Option<MetadataReservation>,
    /// The node-wide cache of Parquet byte ranges Oracle scans read from
    /// warehouse objects; absent on a node that serves no Oracle or has a zero
    /// budget.
    data_cache: Option<Arc<RangeCache>>,
    /// The whole `data_cache_bytes` budget, charged and held as
    /// `metadata_charge` is.
    data_charge: Option<MetadataReservation>,
}

impl BifrostStorage {
    /// Builds the node's store for `backend` under one validated policy.
    ///
    /// A local root is created when absent and served by
    /// [`LocalFileSystem`], which has no timeout or retry. A cloud backend
    /// uses `wyrd-storage`'s `object_store` client for it, configured from
    /// the environment for credentials, with the policy's request timeout and
    /// retry bounds and `wyrd-storage`'s idle-connection lifetime for that
    /// backend. An endpoint selects an S3-compatible service, or a GCS or
    /// Azure emulator in builds with `wyrd-storage`'s `emulator` feature.
    ///
    /// # Errors
    /// Returns [`BifrostStorageError::InvalidConfiguration`] when the local
    /// root cannot be created, a cloud backend is selected in a build without
    /// the `cloud` feature, or `wyrd-storage` rejects the backend settings.
    pub fn new(
        backend: &BackendConfig,
        policy: BifrostStoragePolicy,
    ) -> Result<Self, BifrostStorageError> {
        let store = Self::backend_store(backend, policy)?;
        Ok(Self {
            objects: Arc::new(LimitStore::new(store, policy.max_concurrent_requests())),
            backend: backend.clone(),
            policy,
            metadata_cache: None,
            metadata_charge: None,
            data_cache: None,
            data_charge: None,
        })
    }

    /// Builds the unlimited backend store for `backend`.
    ///
    /// # Errors
    /// Same conditions as [`Self::new`].
    fn backend_store(
        backend: &BackendConfig,
        policy: BifrostStoragePolicy,
    ) -> Result<Arc<dyn ObjectStore>, BifrostStorageError> {
        let invalid = |error: &dyn std::fmt::Display| BifrostStorageError::InvalidConfiguration {
            detail: format!("the Bifrost object store cannot be built: {error}"),
        };
        #[cfg(feature = "cloud")]
        let built = |store: Result<CloudStore, wyrd_storage::StorageError>| {
            store
                .map(|store| store.objects())
                .map_err(|error| invalid(&error))
        };
        match backend {
            BackendConfig::Local { root } => {
                std::fs::create_dir_all(root).map_err(|error| invalid(&error))?;
                Ok(Arc::new(
                    LocalFileSystem::new_with_prefix(root).map_err(|error| invalid(&error))?,
                ))
            }
            #[cfg(feature = "cloud")]
            BackendConfig::S3(config) => {
                let (options, retry) = cloud_transport(backend, policy);
                built(CloudStore::s3(config, options, retry))
            }
            #[cfg(feature = "cloud")]
            BackendConfig::Gcs(config) => {
                let (options, retry) = cloud_transport(backend, policy);
                built(CloudStore::gcs(config, options, retry))
            }
            #[cfg(feature = "cloud")]
            BackendConfig::Azure(config) => {
                let (options, retry) = cloud_transport(backend, policy);
                built(CloudStore::azure(config, options, retry))
            }
            #[cfg(not(feature = "cloud"))]
            BackendConfig::S3(_) | BackendConfig::Gcs(_) | BackendConfig::Azure(_) => {
                let _ = policy;
                Err(invalid(
                    &"cloud storage backends are not compiled in this build; rebuild with the `cloud` feature",
                ))
            }
        }
    }

    /// Gives this owner its node-wide Parquet footer cache, a [`FooterCache`],
    /// and its byte-range cache, a [`RangeCache`].
    ///
    /// Called once at composition on an Oracle-serving node. Each cache's
    /// whole byte limit, `metadata_cache_bytes` and `data_cache_bytes`, is
    /// charged to the Oracle memory root up front and held for the owner's
    /// life, because neither cache reports a per-entry charge. A zero budget
    /// leaves the owner without that cache and charges nothing for it.
    ///
    /// # Errors
    /// Returns the [`BifrostResourceError`] of a refused root charge, or
    /// [`BifrostResourceError::Unavailable`] when a budget does not fit this
    /// platform's `usize`.
    pub fn with_oracle_caches(
        mut self,
        resources: &OracleMetadataResources,
    ) -> Result<Self, BifrostResourceError> {
        let budget = |bytes: u64| {
            usize::try_from(bytes).map_err(|_| BifrostResourceError::Unavailable {
                detail: "an Oracle cache budget does not fit this platform".to_owned(),
            })
        };
        let footers = budget(self.policy.metadata_cache_bytes())?;
        let ranges = budget(self.policy.data_cache_bytes())?;
        if footers > 0 {
            self.metadata_charge = Some(resources.try_reserve_metadata(footers)?);
            self.metadata_cache = Some(Arc::new(FooterCache::new(footers)));
        }
        if ranges > 0 {
            self.data_charge = Some(resources.try_reserve_metadata(ranges)?);
            self.data_cache = Some(Arc::new(RangeCache::new(ranges)));
        }
        Ok(self)
    }

    /// Returns the store every Bifrost read and write goes through.
    #[must_use]
    pub fn objects(&self) -> &Arc<dyn ObjectStore> {
        &self.objects
    }

    /// Returns the backend this store was built for.
    #[must_use]
    pub const fn backend(&self) -> &BackendConfig {
        &self.backend
    }

    /// Returns the node-wide Parquet footer cache, when this node has one.
    #[must_use]
    pub fn metadata_cache(&self) -> Option<&Arc<FileMetadataCache>> {
        self.metadata_cache.as_ref()
    }

    /// Returns the node-wide Parquet byte-range cache, when this node has one.
    #[must_use]
    pub(crate) fn data_cache(&self) -> Option<&Arc<RangeCache>> {
        self.data_cache.as_ref()
    }

    /// Returns the validated policy this owner applies.
    #[must_use]
    pub const fn policy(&self) -> BifrostStoragePolicy {
        self.policy
    }

    /// Replaces this owner's store with `wrap` applied to it, for a fixture.
    ///
    /// A test harness wraps the production store to observe or hold the calls
    /// every Bifrost role makes through [`Self::objects`].
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn with_objects_for_test(
        mut self,
        wrap: impl FnOnce(Arc<dyn ObjectStore>) -> Arc<dyn ObjectStore>,
    ) -> Self {
        self.objects = wrap(self.objects);
        self
    }

    /// Builds an owner over one local root for a fixture.
    ///
    /// Composed the way production composes: a local store, the default
    /// policy resolved for the fixture's Oracle-serving decision, and, on an
    /// Oracle-serving fixture, the footer and byte-range caches charged to a
    /// 1 GiB test Oracle composition.
    ///
    /// # Panics
    /// Panics when the root, the default storage policy, or the test
    /// composition's cache charges are invalid, which would mean the
    /// fixture itself is unusable.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn for_test(root: &std::path::Path, serves_oracle: bool) -> Arc<Self> {
        let owner = Self::new(
            &BackendConfig::Local {
                root: root.to_path_buf(),
            },
            BifrostStoragePolicy::resolve(
                BifrostStorageConfig::default(),
                2 * 1024 * 1024 * 1024,
                serves_oracle,
            )
            .expect("the fixture storage policy is valid"),
        )
        .expect("the fixture local store is valid");
        if !serves_oracle {
            return Arc::new(owner);
        }
        let oracle = crate::resources::BifrostRuntimeResources::composed_for_test(
            1 << 30,
            1 << 30,
            [crate::resources::BifrostRole::Oracle],
        )
        .oracle()
        .expect("the composed test roles include Oracle");
        Arc::new(
            owner
                .with_oracle_caches(&oracle.metadata())
                .expect("the test Oracle composition covers the Oracle caches"),
        )
    }
}

/// Returns the HTTP options and retry bounds every Bifrost cloud client uses.
///
/// The request timeout and retry bounds are the policy's; the idle-pool
/// lifetime and plain-HTTP allowance are `wyrd-storage`'s for this backend.
#[cfg(feature = "cloud")]
fn cloud_transport(
    backend: &BackendConfig,
    policy: BifrostStoragePolicy,
) -> (object_store::ClientOptions, object_store::RetryConfig) {
    let options =
        wyrd_storage::factory::client_options(backend).with_timeout(policy.request_timeout());
    let retry = object_store::RetryConfig {
        max_retries: usize::try_from(policy.max_retries()).unwrap_or(usize::MAX),
        retry_timeout: policy.max_retry_elapsed(),
        ..object_store::RetryConfig::default()
    };
    (options, retry)
}

#[cfg(test)]
mod tests {
    use object_store::ObjectStoreExt as _;
    use object_store::path::Path as ObjectPath;

    use super::*;

    /// A local owner reads and writes beneath its root, and a missing object
    /// is `object_store`'s `NotFound`, which the stale-object rule recognizes.
    ///
    /// # Panics
    /// Panics when the round trip or the not-found classification fails.
    #[tokio::test]
    async fn local_store_round_trips_and_reports_missing_objects() {
        let root = tempfile::tempdir().expect("warehouse root");
        let storage = BifrostStorage::for_test(&root.path().join("warehouse"), false);
        let path = ObjectPath::from("data/one.parquet");
        storage
            .objects()
            .put(&path, bytes::Bytes::from_static(b"abc").into())
            .await
            .expect("put");
        let read = storage
            .objects()
            .get(&path)
            .await
            .expect("get")
            .bytes()
            .await
            .expect("bytes");
        assert_eq!(read.as_ref(), b"abc");
        assert!(root.path().join("warehouse/data/one.parquet").exists());
        let missing = storage
            .objects()
            .head(&ObjectPath::from("data/absent.parquet"))
            .await
            .expect_err("absent");
        assert!(error_chain_contains_not_found(&missing));
    }
}
