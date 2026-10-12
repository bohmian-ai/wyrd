//! Storage handle shared by server-tier crates.

use crate::error::StorageError;
use crate::local::LocalSigner;
use crate::settings::{BackendConfig, StorageSettings};
use crate::signer::BackendSigner;
use crate::telemetry::{ActiveOperation, StorageOperation};
use crate::tenant_path::ValidatedPath;
use futures_util::TryStreamExt as _;
use object_store::path::Path;
use object_store::{ObjectStore, ObjectStoreExt as _};
use std::sync::Arc;
use std::time::Duration;
use wyrd_spec::storage::StorageBackendKind;

/// Error returned by [`StorageHandle::health_probe`].
#[derive(Debug, thiserror::Error)]
pub enum StorageHealthError {
    /// The storage backend probe timed out.
    #[error("storage health probe timed out after {ms}ms")]
    Timeout {
        /// Elapsed milliseconds before the probe was cancelled.
        ms: u64,
    },
    /// The storage backend returned an unexpected error.
    #[error("storage health probe failed")]
    Backend(#[source] object_store::Error),
    /// Local storage root is inaccessible.
    #[error("local storage root is inaccessible")]
    LocalRoot(#[source] std::io::Error),
}

/// Shared storage handle.
///
/// The handle carries the active backend signer and the `object_store` data
/// plane bound to the same backend. A cloud handle's data plane is the
/// signer's own store, so presigning, multipart control, and direct reads and
/// writes share one client.
#[derive(Clone)]
pub struct StorageHandle {
    /// Signer for client presigning, multipart control, and verification.
    signer: BackendSigner,
    /// Data plane for server-side reads, writes, listing, and deletes.
    objects: Arc<dyn ObjectStore>,
    /// Backend label recorded on every storage metric.
    metrics_backend: &'static str,
    /// Backend settings the handle was built from.
    backend_config: BackendConfig,
    /// Whether upload completion verifies server-side encryption.
    require_encryption: bool,
    /// Lifetime of presigned URLs.
    presign_ttl: Duration,
    /// Default multipart part size.
    default_part_size_bytes: u64,
    /// Size at or above which cloud uploads use multipart.
    multipart_threshold_bytes: u64,
}

impl std::fmt::Debug for StorageHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StorageHandle")
            .field("backend", &self.signer.kind())
            .field("objects", &self.objects)
            .field("require_encryption", &self.require_encryption)
            .field("presign_ttl", &self.presign_ttl)
            .field("default_part_size_bytes", &self.default_part_size_bytes)
            .field("multipart_threshold_bytes", &self.multipart_threshold_bytes)
            .finish_non_exhaustive()
    }
}

impl StorageHandle {
    /// Build a local storage handle from an already-constructed signer.
    ///
    /// Intended for tests and local-mode harnesses: `require_encryption` is
    /// `false` and every tuning value is the crate default. Production paths
    /// must use [`Self::from_settings`].
    ///
    /// # Panics
    ///
    /// Panics if the signer's root, which [`LocalSigner::new`] proved exists,
    /// can no longer be opened as a filesystem store.
    #[must_use]
    pub fn new(signer: LocalSigner) -> Self {
        let backend_config = BackendConfig::Local {
            root: signer.root().to_path_buf(),
        };
        let signer = BackendSigner::Local(signer);
        let objects = signer
            .objects()
            .expect("invariant: a LocalSigner root exists and opens as a filesystem store");
        Self::assemble(signer, objects, backend_config)
    }

    /// Assemble a handle from its substrate parts with default tuning and
    /// register its idle operation gauges.
    fn assemble(
        signer: BackendSigner,
        objects: Arc<dyn ObjectStore>,
        backend_config: BackendConfig,
    ) -> Self {
        let metrics_backend = crate::telemetry::backend_label(backend_config.kind());
        crate::telemetry::register(metrics_backend);
        Self {
            signer,
            objects,
            metrics_backend,
            backend_config,
            require_encryption: false,
            presign_ttl: Duration::from_secs(u64::from(crate::settings::DEFAULT_PRESIGN_TTL_SECS)),
            default_part_size_bytes: crate::settings::DEFAULT_PART_SIZE_BYTES,
            multipart_threshold_bytes: crate::plan::MULTIPART_THRESHOLD_BYTES,
        }
    }

    /// Build a storage handle from boot settings.
    ///
    /// Builds the backend signer, runs the boot preflight, and binds the data
    /// plane to the signer's store.
    ///
    /// # Errors
    /// Returns a storage error when signer or store construction fails.
    pub async fn from_settings(settings: StorageSettings) -> Result<Arc<Self>, StorageError> {
        let signer = crate::factory::build_signer(&settings.backend).await?;
        let objects = signer.objects()?;
        Ok(Self::finish(settings, signer, objects).await)
    }

    /// Build a storage handle from boot settings over a supplied data plane.
    ///
    /// The signer, preflight, telemetry, and every tuning value are exactly
    /// the ones [`Self::from_settings`] applies; only server-side reads,
    /// writes, listing, and deletes go to `objects`. A test supplies a store
    /// that wraps the backend's to model behavior the local service does not
    /// exhibit, such as metadata that under-reports an object's size.
    ///
    /// # Errors
    /// Returns a storage error when signer construction fails.
    pub async fn from_settings_with_store(
        settings: StorageSettings,
        objects: Arc<dyn ObjectStore>,
    ) -> Result<Arc<Self>, StorageError> {
        let signer = crate::factory::build_signer(&settings.backend).await?;
        Ok(Self::finish(settings, signer, objects).await)
    }

    /// Run the boot preflight and apply the settings' tuning.
    async fn finish(
        settings: StorageSettings,
        signer: BackendSigner,
        objects: Arc<dyn ObjectStore>,
    ) -> Arc<Self> {
        crate::preflight::run(&signer).await;
        let handle = Self::assemble(signer, objects, settings.backend);
        Arc::new(Self {
            require_encryption: settings.require_encryption,
            presign_ttl: settings.presign_ttl,
            default_part_size_bytes: settings.part_size_bytes,
            multipart_threshold_bytes: settings.multipart_threshold_bytes,
            ..handle
        })
    }

    /// Return the configured backend kind.
    #[must_use]
    pub fn backend(&self) -> StorageBackendKind {
        self.signer.kind()
    }

    /// Borrow the configured backend settings.
    #[must_use]
    pub fn backend_config(&self) -> &BackendConfig {
        &self.backend_config
    }

    /// Borrow the active backend signer.
    #[must_use]
    pub fn signer(&self) -> &BackendSigner {
        &self.signer
    }

    /// Whether upload completion must verify server-side encryption markers.
    #[must_use]
    pub fn require_encryption(&self) -> bool {
        self.require_encryption
    }

    /// Presign TTL configured at boot.
    #[must_use]
    pub fn presign_ttl(&self) -> Duration {
        self.presign_ttl
    }

    /// Presign TTL in seconds.
    #[must_use]
    pub fn presign_ttl_secs(&self) -> u32 {
        crate::signer::ttl_secs(self.presign_ttl)
    }

    /// Default multipart part size in bytes.
    #[must_use]
    pub fn default_part_size_bytes(&self) -> u64 {
        self.default_part_size_bytes
    }

    /// Object size at or above which cloud backends switch to multipart upload.
    #[must_use]
    pub fn multipart_threshold_bytes(&self) -> u64 {
        self.multipart_threshold_bytes
    }

    /// Probe the storage backend for liveness.
    ///
    /// For the local backend, reads the metadata of the configured storage
    /// root. For cloud backends (S3, GCS, Azure), issues a `HEAD` of
    /// `_wyrd/health.sentinel`; not-found is healthy (the bucket answered and
    /// the sentinel is expected to be absent).
    ///
    /// # Errors
    ///
    /// Returns [`StorageHealthError`] when the backend returns an error that
    /// indicates inaccessibility.
    #[tracing::instrument(skip(self))]
    pub async fn health_probe(&self) -> Result<(), StorageHealthError> {
        if let BackendConfig::Local { root } = &self.backend_config {
            return tokio::fs::metadata(root)
                .await
                .map(|_| ())
                .map_err(StorageHealthError::LocalRoot);
        }
        let active = self.start(StorageOperation::Head);
        let result = match self
            .objects
            .head(&Path::from("_wyrd/health.sentinel"))
            .await
        {
            Ok(_) | Err(object_store::Error::NotFound { .. }) => Ok(()),
            Err(error) => Err(StorageHealthError::Backend(error)),
        };
        active.finish(result, 0)
    }

    /// Read an object's bytes directly from the backend.
    ///
    /// Server-side data-plane access keyed by a tenant-validated path. For the
    /// client byte-transfer path use the presign methods on [`Self::signer`].
    ///
    /// # Errors
    /// Returns [`StorageError::ObjectNotFound`] when the object is absent, or
    /// [`StorageError::Backend`] for any other backend failure.
    #[tracing::instrument(skip(self), fields(backend = ?self.backend()))]
    pub async fn get_object(&self, path: &ValidatedPath) -> Result<Vec<u8>, StorageError> {
        let active = self.start(StorageOperation::Get);
        let result = async { self.objects.get(&path.location()).await?.bytes().await }
            .await
            .map(Vec::from)
            .map_err(|error| self.store_error(&error, "get_object", path));
        let bytes = result.as_ref().map_or(0, |body| body.len() as u64);
        active.finish(result, bytes)
    }

    /// Read an object's body, holding at most `limit + 1` bytes of it.
    ///
    /// The body streams from the backend chunk by chunk and the read stops as
    /// soon as `limit + 1` bytes are held. No stored metadata sizes the read,
    /// so a caller whose earlier [`Self::object_len`] check saw a stale or
    /// replaced object still learns the truth: the returned buffer is longer
    /// than `limit` exactly when the body is. Callers reject that overflow;
    /// this method only bounds the allocation. Stopping early drops the rest
    /// of the body stream. The object key is not recorded on the tracing span.
    ///
    /// # Errors
    /// Returns [`StorageError::ObjectNotFound`] when the object is absent, or
    /// [`StorageError::Backend`] for any other backend failure while opening
    /// or reading the body.
    #[tracing::instrument(skip(self, path), fields(backend = ?self.backend()))]
    pub async fn get_object_bounded(
        &self,
        path: &ValidatedPath,
        limit: u64,
    ) -> Result<Vec<u8>, StorageError> {
        let held = usize::try_from(limit)
            .unwrap_or(usize::MAX)
            .saturating_add(1);
        let active = self.start(StorageOperation::Get);
        let result = async {
            let mut stream = self.objects.get(&path.location()).await?.into_stream();
            let mut body = Vec::new();
            while body.len() < held
                && let Some(chunk) = stream.try_next().await?
            {
                let take = chunk.len().min(held - body.len());
                body.extend_from_slice(&chunk[..take]);
            }
            Ok(body)
        }
        .await
        .map_err(|error| self.store_error(&error, "get_object_bounded", path));
        let bytes = result.as_ref().map_or(0, |body| body.len() as u64);
        active.finish(result, bytes)
    }

    /// Size in bytes of an object, read from its metadata without its body.
    ///
    /// Lets a server-side reader bound an object before it loads it.
    ///
    /// # Errors
    /// Returns [`StorageError::ObjectNotFound`] when the object is absent, or
    /// [`StorageError::Backend`] for any other backend failure.
    #[tracing::instrument(skip(self), fields(backend = ?self.backend()))]
    pub async fn object_len(&self, path: &ValidatedPath) -> Result<u64, StorageError> {
        let active = self.start(StorageOperation::Head);
        let result = self
            .objects
            .head(&path.location())
            .await
            .map(|meta| meta.size)
            .map_err(|error| self.store_error(&error, "object_len", path));
        active.finish(result, 0)
    }

    /// Write an object's bytes directly to the backend.
    ///
    /// The local backend stages and renames the file through
    /// [`LocalSigner::write_atomically`]; cloud backends issue one `PUT`,
    /// which never retries.
    ///
    /// # Errors
    /// Returns [`StorageError::Backend`] when the backend write fails.
    #[tracing::instrument(skip(self, bytes), fields(backend = ?self.backend(), len = bytes.len()))]
    pub async fn put_object(
        &self,
        path: &ValidatedPath,
        bytes: Vec<u8>,
    ) -> Result<(), StorageError> {
        let len = bytes.len() as u64;
        let active = self.start(StorageOperation::Put);
        let result = match &self.signer {
            BackendSigner::Local(local) => {
                local
                    .write_atomically(std::path::Path::new(&path.full), &bytes)
                    .await
            }
            #[cfg(feature = "cloud")]
            BackendSigner::Cloud(_) => self
                .objects
                .put(&path.location(), object_store::PutPayload::from(bytes))
                .await
                .map(|_| ())
                .map_err(|error| self.store_error(&error, "put_object", path)),
        };
        active.finish(result, len)
    }

    /// List object keys beneath a tenant-validated path.
    ///
    /// Returns the full backend keys of every object strictly under `path`
    /// (recursive).
    ///
    /// # Errors
    /// Returns [`StorageError::Backend`] when the backend list fails.
    #[tracing::instrument(skip(self), fields(backend = ?self.backend()))]
    pub async fn list_objects(&self, path: &ValidatedPath) -> Result<Vec<String>, StorageError> {
        let active = self.start(StorageOperation::List);
        let result = self
            .objects
            .list(Some(&path.location()))
            .map_ok(|meta| meta.location.to_string())
            .try_collect()
            .await
            .map_err(|error| self.store_error(&error, "list_objects", path));
        active.finish(result, 0)
    }

    /// Delete an object directly from the backend.
    ///
    /// Deleting an absent object succeeds, so a repeated delete is idempotent.
    ///
    /// # Errors
    /// Returns [`StorageError::Backend`] when the backend delete fails.
    #[tracing::instrument(skip(self), fields(backend = ?self.backend()))]
    pub async fn delete_object(&self, path: &ValidatedPath) -> Result<(), StorageError> {
        let active = self.start(StorageOperation::Delete);
        let result = match self.objects.delete(&path.location()).await {
            Ok(()) | Err(object_store::Error::NotFound { .. }) => Ok(()),
            Err(error) => Err(self.store_error(&error, "delete_object", path)),
        };
        active.finish(result, 0)
    }

    /// Start one metered data-plane operation.
    fn start(&self, operation: StorageOperation) -> ActiveOperation {
        ActiveOperation::start(self.metrics_backend, operation)
    }

    /// Map an `object_store` data-plane error onto the storage error catalog.
    fn store_error(
        &self,
        error: &object_store::Error,
        op: &'static str,
        path: &ValidatedPath,
    ) -> StorageError {
        if matches!(error, object_store::Error::NotFound { .. }) {
            StorageError::ObjectNotFound {
                storage_path: path.full.clone(),
            }
        } else {
            StorageError::Backend {
                backend: self.signer.kind(),
                op,
                message: error.to_string(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local::LocalSigner;
    use tempfile::TempDir;

    /// Build a local handle over `root`.
    fn local_handle(root: &std::path::Path) -> StorageHandle {
        StorageHandle::new(LocalSigner::new(root.to_path_buf()).expect("local signer"))
    }

    #[tokio::test]
    async fn health_probe_local_root_exists() {
        let dir = TempDir::new().expect("tempdir");
        let handle = local_handle(dir.path());
        assert!(handle.health_probe().await.is_ok());
    }

    #[tokio::test]
    async fn health_probe_local_missing_root() {
        let dir = TempDir::new().expect("tempdir");
        let handle = local_handle(dir.path());
        // Remove the directory after creating the handle to simulate inaccessible root.
        std::fs::remove_dir_all(dir.path()).expect("remove tempdir");
        let result = handle.health_probe().await;
        assert!(
            matches!(result, Err(StorageHealthError::LocalRoot(_))),
            "expected LocalRoot error, got {result:?}"
        );
    }

    /// Verify local direct writes create nested parents before staging bytes.
    #[tokio::test]
    async fn put_object_local_creates_nested_parent() {
        let dir = TempDir::new().expect("tempdir");
        let handle = local_handle(dir.path());
        let tenant = wyrd_spec::DataTenantId::new_v7();
        let card = uuid::Uuid::now_v7().to_string();
        let full = crate::tenant_path::build(tenant, &card, "blob/spec.json");
        let path =
            crate::tenant_path::validate(&full, tenant).expect("nested object path is valid");

        handle
            .put_object(&path, b"card".to_vec())
            .await
            .expect("nested local write succeeds");

        assert_eq!(
            std::fs::read(dir.path().join(full)).expect("written object is readable"),
            b"card"
        );
    }

    /// A bounded read returns a body within the limit whole, stops one byte
    /// past the limit for a longer body, and reports an absent object.
    #[tokio::test]
    async fn get_object_bounded_holds_at_most_one_byte_past_the_limit() {
        let dir = TempDir::new().expect("tempdir");
        let handle = local_handle(dir.path());
        let tenant = wyrd_spec::DataTenantId::new_v7();
        let card = uuid::Uuid::now_v7().to_string();
        let path = |name: &str| {
            crate::tenant_path::validate(&crate::tenant_path::build(tenant, &card, name), tenant)
                .expect("object path is valid")
        };
        handle
            .put_object(&path("body"), b"0123456789".to_vec())
            .await
            .expect("the object writes");

        let whole = handle.get_object_bounded(&path("body"), 10).await;
        let over = handle.get_object_bounded(&path("body"), 4).await;
        let absent = handle.get_object_bounded(&path("absent"), 4).await;

        assert_eq!(whole.expect("the body reads"), b"0123456789");
        assert_eq!(over.expect("the prefix reads"), b"01234");
        assert!(
            matches!(absent, Err(StorageError::ObjectNotFound { .. })),
            "expected ObjectNotFound, got {absent:?}"
        );
    }
    /// Every data-plane call records one success with its bytes, and no
    /// operation stays active afterwards.
    #[tokio::test(flavor = "current_thread")]
    async fn data_plane_calls_emit_exact_operation_metrics() {
        let recorder = crate::telemetry::tests::TestRecorder::default();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let dir = TempDir::new().expect("tempdir");
        let handle = local_handle(dir.path());
        let tenant = wyrd_spec::DataTenantId::new_v7();
        let card = uuid::Uuid::now_v7().to_string();
        let path = crate::tenant_path::validate(
            &crate::tenant_path::build(tenant, &card, "dir/object"),
            tenant,
        )
        .expect("object path is valid");
        let prefix =
            crate::tenant_path::validate(&crate::tenant_path::build(tenant, &card, "dir"), tenant)
                .expect("prefix path is valid");

        handle.put_object(&path, vec![1_u8; 3]).await.expect("put");
        assert_eq!(handle.get_object(&path).await.expect("get").len(), 3);
        assert_eq!(handle.object_len(&path).await.expect("head"), 3);
        assert_eq!(
            handle.list_objects(&prefix).await.expect("list"),
            vec![path.full.clone()]
        );
        handle.delete_object(&path).await.expect("delete");

        for (direction, operation) in [("write", "put"), ("read", "get")] {
            assert_eq!(
                recorder.counter(&format!(
                    "wyrd_storage_bytes_total{{backend=local,direction={direction},operation={operation}}}"
                )),
                3
            );
        }
        for operation in ["put", "get", "head", "list", "delete"] {
            assert_eq!(
                recorder.counter(&format!(
                    "wyrd_storage_operations_total{{backend=local,operation={operation},outcome=success}}"
                )),
                1
            );
            assert!(
                recorder
                    .gauge(&format!(
                        "wyrd_storage_operations_active{{backend=local,operation={operation}}}"
                    ))
                    .abs()
                    <= f64::EPSILON
            );
        }
    }
}

#[cfg(test)]
mod from_settings_tests {
    use crate::{BackendSigner, StorageHandle};
    use std::sync::Arc;

    /// Storage variables cleared unless the test provides them.
    const ENV_KEYS: &[&str] = &[
        "WYRD_STORAGE_URL",
        "WYRD_STORAGE_ENDPOINT_URL",
        "WYRD_STORAGE_REQUIRE_ENCRYPTION",
        "WYRD_STORAGE_PRESIGN_TTL_SECS",
        "WYRD_STORAGE_PART_SIZE_BYTES",
        "WYRD_STORAGE_MULTIPART_THRESHOLD_BYTES",
    ];

    #[tokio::test]
    async fn builds_local_handle_from_settings() {
        let (handle, _root) = Box::pin(local_handle()).await;

        assert!(matches!(handle.signer(), BackendSigner::Local(_)));
        assert_eq!(
            handle.backend(),
            wyrd_spec::storage::StorageBackendKind::Local
        );
        assert_eq!(handle.presign_ttl_secs(), 600);
        assert_eq!(handle.default_part_size_bytes(), 16 * 1024 * 1024);
    }

    /// Boot a local handle from a `file:` storage URL over a fresh temp root.
    async fn local_handle() -> (Arc<StorageHandle>, tempfile::TempDir) {
        let root = tempfile::tempdir().expect("temp dir");
        let vars = vec![(
            "WYRD_STORAGE_URL",
            Some(
                url::Url::from_file_path(root.path())
                    .expect("temp dir is absolute")
                    .to_string(),
            ),
        )];
        let provided = vars.iter().map(|(key, _)| *key).collect::<Vec<_>>();
        let mut all = ENV_KEYS
            .iter()
            .filter(|key| !provided.contains(key))
            .map(|key| (*key, None))
            .collect::<Vec<(&'static str, Option<String>)>>();
        all.extend(vars);

        let handle = Box::pin(temp_env::async_with_vars(all, async {
            let settings = crate::settings::from_env().expect("settings parse");
            StorageHandle::from_settings(settings)
                .await
                .expect("local handle")
        }))
        .await;

        (handle, root)
    }
}
