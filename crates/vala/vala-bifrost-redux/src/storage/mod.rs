//! The node's one Bifrost storage owner.

mod error;
mod policy;

use std::sync::Arc;

use datafusion::execution::cache::cache_manager::FileMetadataCache;
use datafusion::execution::cache::default_cache::DefaultCache;
use object_store::ObjectStore;
use object_store::limit::LimitStore;
use object_store::local::LocalFileSystem;
use wyrd_storage::BackendConfig;

use crate::resources::{BifrostResourceError, MetadataReservation, OracleMetadataResources};

pub use crate::storage::error::BifrostStorageError;
pub(crate) use crate::storage::error::error_chain_contains_not_found;
pub use crate::storage::policy::{BifrostStorageConfig, BifrostStoragePolicy};

/// The node's one Bifrost object store and Parquet footer cache.
///
/// Every Bifrost role reads and writes through [`Self::objects`]: Scribe
/// uploads, Oracle's `DataFusion` scans, Forge rewrites and cleanup, and the
/// Iceberg catalog's `FileIO`. The store is `object_store`'s own client for the
/// configured backend behind one node-wide [`LimitStore`]; timeouts and
/// retries are the cloud clients' own. `wyrd-storage` keeps its separate
/// client for card and artifact storage.
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
}

impl BifrostStorage {
    /// Builds the node's store for `backend` under one validated policy.
    ///
    /// A local root is created when absent and served by
    /// [`LocalFileSystem`], which has no timeout or retry. A cloud backend
    /// uses `object_store`'s client for it, configured from the environment
    /// for credentials, with the policy's request timeout and retry bounds and
    /// `wyrd-storage`'s idle-connection lifetime for that backend. An endpoint
    /// selects an S3-compatible service or an emulator.
    ///
    /// # Errors
    /// Returns [`BifrostStorageError::InvalidConfiguration`] when the local
    /// root cannot be created, a cloud backend is selected in a build without
    /// the `cloud` feature, or `object_store` rejects the backend settings.
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
        let built = |store: object_store::Result<Arc<dyn ObjectStore>>| {
            store.map_err(|error| invalid(&error))
        };
        match backend {
            BackendConfig::Local { root } => {
                std::fs::create_dir_all(root).map_err(|error| invalid(&error))?;
                Ok(Arc::new(
                    LocalFileSystem::new_with_prefix(root).map_err(|error| invalid(&error))?,
                ))
            }
            #[cfg(feature = "cloud")]
            BackendConfig::S3(config) => built(cloud::s3(config, backend, policy)),
            #[cfg(feature = "cloud")]
            BackendConfig::Gcs(config) => built(cloud::gcs(config, backend, policy)),
            #[cfg(feature = "cloud")]
            BackendConfig::Azure(config) => built(cloud::azure(config, backend, policy)),
            #[cfg(not(feature = "cloud"))]
            BackendConfig::S3(_) | BackendConfig::Gcs(_) | BackendConfig::Azure(_) => {
                let _ = policy;
                Err(invalid(
                    &"cloud storage backends are not compiled in this build; rebuild with the `cloud` feature",
                ))
            }
        }
    }

    /// Gives this owner `DataFusion`'s node-wide Parquet footer cache.
    ///
    /// Called once at composition on an Oracle-serving node. The cache's
    /// whole byte limit, `metadata_cache_bytes`, is charged to the Oracle
    /// memory root up front and held for the owner's life, because
    /// `DataFusion`'s cache reports no per-entry charge. A zero budget leaves
    /// the owner without a cache and charges nothing.
    ///
    /// # Errors
    /// Returns the [`BifrostResourceError`] of a refused root charge, or
    /// [`BifrostResourceError::Unavailable`] when the budget does not fit this
    /// platform's `usize`.
    pub fn with_metadata_cache(
        mut self,
        resources: &OracleMetadataResources,
    ) -> Result<Self, BifrostResourceError> {
        if self.policy.metadata_cache_bytes() == 0 {
            return Ok(self);
        }
        let bytes = usize::try_from(self.policy.metadata_cache_bytes()).map_err(|_| {
            BifrostResourceError::Unavailable {
                detail: "the metadata cache budget does not fit this platform".to_owned(),
            }
        })?;
        self.metadata_charge = Some(resources.try_reserve_metadata(bytes)?);
        self.metadata_cache = Some(Arc::new(DefaultCache::new(bytes)));
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
    /// Oracle-serving fixture, the footer cache charged to a 1 GiB test Oracle
    /// composition.
    ///
    /// # Panics
    /// Panics when the root, the default storage policy, or the test
    /// composition's footer-cache charge is invalid, which would mean the
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
                .with_metadata_cache(&oracle.metadata())
                .expect("the test Oracle composition covers the footer cache"),
        )
    }
}

/// `object_store`'s cloud clients, built from `wyrd-storage`'s backend settings.
#[cfg(feature = "cloud")]
mod cloud {
    use std::sync::Arc;

    use object_store::aws::AmazonS3Builder;
    use object_store::azure::MicrosoftAzureBuilder;
    use object_store::gcp::GoogleCloudStorageBuilder;
    use object_store::{ClientOptions, ObjectStore, RetryConfig};
    use wyrd_storage::BackendConfig;
    use wyrd_storage::settings::{AzureConfig, GcsConfig, S3Config};

    use super::BifrostStoragePolicy;

    /// Returns the HTTP options and retry bounds every cloud client shares.
    ///
    /// The request timeout and retry bounds are the policy's; the idle-pool
    /// lifetime is `wyrd-storage`'s for this backend, and plain HTTP is allowed
    /// only for an `http://` endpoint (an emulator or local service).
    fn client(
        backend: &BackendConfig,
        endpoint: Option<&str>,
        policy: BifrostStoragePolicy,
    ) -> (ClientOptions, RetryConfig) {
        let options = ClientOptions::new()
            .with_timeout(policy.request_timeout())
            .with_pool_idle_timeout(wyrd_storage::factory::pool_idle_timeout(backend))
            .with_allow_http(endpoint.is_some_and(|endpoint| endpoint.starts_with("http://")));
        let retry = RetryConfig {
            max_retries: usize::try_from(policy.max_retries()).unwrap_or(usize::MAX),
            retry_timeout: policy.max_retry_elapsed(),
            ..RetryConfig::default()
        };
        (options, retry)
    }

    /// Builds the S3 client: virtual-hosted on AWS, path-style at an endpoint.
    ///
    /// # Errors
    /// Returns the `object_store` error for unusable settings or credentials.
    pub(super) fn s3(
        config: &S3Config,
        backend: &BackendConfig,
        policy: BifrostStoragePolicy,
    ) -> object_store::Result<Arc<dyn ObjectStore>> {
        let (options, retry) = client(backend, config.endpoint_url.as_deref(), policy);
        let mut builder = AmazonS3Builder::from_env()
            .with_bucket_name(&config.bucket)
            .with_client_options(options)
            .with_retry(retry);
        if let Some(region) = &config.region {
            builder = builder.with_region(region);
        }
        builder = match &config.endpoint_url {
            Some(endpoint) => builder.with_endpoint(endpoint),
            None => builder.with_virtual_hosted_style_request(true),
        };
        Ok(Arc::new(builder.build()?))
    }

    /// Builds the GCS client with `wyrd-storage`'s service-account cascade;
    /// an endpoint without a key is an anonymous emulator.
    ///
    /// # Errors
    /// Returns the `object_store` error for unusable settings or credentials.
    pub(super) fn gcs(
        config: &GcsConfig,
        backend: &BackendConfig,
        policy: BifrostStoragePolicy,
    ) -> object_store::Result<Arc<dyn ObjectStore>> {
        let (options, retry) = client(backend, config.endpoint_url.as_deref(), policy);
        let mut builder = GoogleCloudStorageBuilder::from_env()
            .with_bucket_name(&config.bucket)
            .with_client_options(options)
            .with_retry(retry);
        let key = wyrd_storage::factory::gcs::service_account_json();
        if let Some(key) = &key {
            builder = builder.with_service_account_key(key);
        }
        if let Some(endpoint) = &config.endpoint_url {
            builder = builder
                .with_base_url(endpoint)
                .with_skip_signature(key.is_none());
        }
        Ok(Arc::new(builder.build()?))
    }

    /// Builds the Azure Blob client; an endpoint names the account URL.
    ///
    /// # Errors
    /// Returns the `object_store` error for unusable settings or credentials.
    pub(super) fn azure(
        config: &AzureConfig,
        backend: &BackendConfig,
        policy: BifrostStoragePolicy,
    ) -> object_store::Result<Arc<dyn ObjectStore>> {
        let (options, retry) = client(backend, config.endpoint_url.as_deref(), policy);
        let mut builder = MicrosoftAzureBuilder::from_env()
            .with_account(&config.account)
            .with_container_name(&config.container)
            .with_client_options(options)
            .with_retry(retry);
        if let Some(endpoint) = &config.endpoint_url {
            builder = builder.with_endpoint(endpoint.clone());
        }
        Ok(Arc::new(builder.build()?))
    }
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
        assert!(matches!(
            BifrostStorageError::from_object_store(&missing),
            BifrostStorageError::NotFound { .. }
        ));
    }
}
