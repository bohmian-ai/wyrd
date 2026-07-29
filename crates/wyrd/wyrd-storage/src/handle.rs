//! Storage handle shared by server-tier crates.

use crate::error::StorageError;
use crate::settings::{BackendConfig, StorageSettings};
use crate::signer::BackendSigner;
use crate::tenant_path::ValidatedPath;
use opendal::{EntryMode, ErrorKind, Operator};
use std::sync::{Arc, OnceLock};
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
    Backend(#[source] opendal::Error),
    /// Local storage root is inaccessible.
    #[error("local storage root is inaccessible")]
    LocalRoot(#[source] std::io::Error),
}

/// Shared storage handle.
///
/// The handle carries the active backend signer and the opendal `Operator`
/// used for backend health probing.
#[derive(Clone)]
pub struct StorageHandle {
    signer: BackendSigner,
    operator: Operator,
    backend_config: BackendConfig,
    require_encryption: bool,
    presign_ttl: Duration,
    default_part_size_bytes: u64,
    multipart_threshold_bytes: u64,
    public_base_url: Option<String>,
    public_base_url_override: Arc<OnceLock<String>>,
}

impl std::fmt::Debug for StorageHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StorageHandle")
            .field("backend", &self.signer.kind())
            .field("operator", &"<opendal::Operator>")
            .field("require_encryption", &self.require_encryption)
            .field("presign_ttl", &self.presign_ttl)
            .field("default_part_size_bytes", &self.default_part_size_bytes)
            .field("multipart_threshold_bytes", &self.multipart_threshold_bytes)
            .finish_non_exhaustive()
    }
}

impl StorageHandle {
    /// Build a storage handle from an already-constructed signer.
    ///
    /// Intended for tests and local-mode harnesses. The resulting
    /// [`BackendConfig`] is synthesized from the signer alone, so
    /// optional fields (`region`, `endpoint_url`, `force_path_style`,
    /// `public_base_url`) are defaulted, `require_encryption` is `false`,
    /// `presign_ttl` is the crate default, and `default_part_size_bytes`
    /// is the crate default. Production paths must use [`Self::from_settings`].
    ///
    /// # Panics
    ///
    /// Panics if opendal operator construction fails for the synthesized config.
    /// Safe under opendal 0.57 because operator construction is lazy (no network
    /// calls or credential loading at build time). Production paths must use
    /// [`Self::from_settings`].
    #[must_use]
    pub fn new(signer: BackendSigner) -> Self {
        let backend_config = match &signer {
            BackendSigner::Local(local) => BackendConfig::Local {
                root: local.root().to_path_buf(),
            },
            #[cfg(feature = "cloud")]
            BackendSigner::Cloud(cloud) => cloud.backend_config(),
        };
        let operator = crate::factory::build_operator(&backend_config)
            .expect("operator construction is infallible for the test/local-harness constructor");
        Self::assemble(signer, operator, backend_config)
    }

    /// Assemble a handle from its substrate parts with default tuning.
    fn assemble(signer: BackendSigner, operator: Operator, backend_config: BackendConfig) -> Self {
        Self {
            signer,
            operator,
            backend_config,
            require_encryption: false,
            presign_ttl: Duration::from_secs(u64::from(crate::settings::DEFAULT_PRESIGN_TTL_SECS)),
            default_part_size_bytes: crate::settings::DEFAULT_PART_SIZE_BYTES,
            multipart_threshold_bytes: crate::plan::MULTIPART_THRESHOLD_BYTES,
            public_base_url: None,
            public_base_url_override: Arc::new(OnceLock::new()),
        }
    }

    /// Build a handle from a signer and an explicit backend config, skipping the
    /// boot health probe.
    ///
    /// The operator is built from `backend_config` so any custom endpoint (for
    /// example a local emulator) is honored. Intended for tests and emulator
    /// harnesses that need a working data-plane operator without ambient cloud
    /// credentials. Production paths must use [`Self::from_settings`].
    ///
    /// # Errors
    /// Returns a storage error when operator construction fails.
    #[cfg(any(test, feature = "emulator"))]
    pub fn for_testing(
        signer: BackendSigner,
        backend_config: BackendConfig,
    ) -> Result<Self, StorageError> {
        let operator = crate::factory::build_operator(&backend_config)?;
        Ok(Self::assemble(signer, operator, backend_config))
    }

    /// Build an emulator handle with explicit backend settings and multipart
    /// tuning. This keeps emulator endpoints available to the server's OLAP
    /// catalog while allowing small journey fixtures to exercise multipart
    /// transfer paths.
    #[cfg(any(test, feature = "emulator"))]
    pub fn for_testing_with_multipart_threshold(
        signer: BackendSigner,
        backend_config: BackendConfig,
        multipart_threshold_bytes: u64,
    ) -> Result<Self, StorageError> {
        let mut handle = Self::for_testing(signer, backend_config)?;
        handle.multipart_threshold_bytes = multipart_threshold_bytes;
        Ok(handle)
    }

    /// Build a storage handle from a signer with an explicit multipart
    /// threshold.
    ///
    /// Test/local-harness constructor used to drive server-tier multipart
    /// uploads against emulator signers (GCS, Azure) whose backend config
    /// carries no emulator endpoint. Behaves like [`Self::new`] but overrides
    /// the multipart threshold so a small payload triggers a real multipart
    /// upload. The synthesized operator is never exercised because the
    /// in-process test server skips the boot health probe.
    ///
    /// # Panics
    ///
    /// Panics if operator construction fails for the synthesized config.
    #[cfg(any(test, feature = "emulator"))]
    #[must_use]
    pub fn from_signer(signer: BackendSigner, multipart_threshold_bytes: u64) -> Self {
        Self {
            multipart_threshold_bytes,
            ..Self::new(signer)
        }
    }

    /// Build a storage handle from boot settings.
    ///
    /// # Errors
    /// Returns a storage error when signer or operator construction fails.
    pub async fn from_settings(settings: StorageSettings) -> Result<Arc<Self>, StorageError> {
        let signer = crate::factory::build_signer(&settings.backend).await?;
        let operator = crate::factory::build_operator(&settings.backend)?;
        crate::preflight::run(&signer).await;

        Ok(Arc::new(Self {
            signer,
            operator,
            backend_config: settings.backend,
            require_encryption: settings.require_encryption,
            presign_ttl: settings.presign_ttl,
            default_part_size_bytes: settings.part_size_bytes,
            multipart_threshold_bytes: settings.multipart_threshold_bytes,
            public_base_url: settings.public_base_url,
            public_base_url_override: Arc::new(OnceLock::new()),
        }))
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

    /// Public base URL configured for local-mode routes.
    #[must_use]
    pub fn public_base_url(&self) -> Option<&str> {
        self.public_base_url_override
            .get()
            .map(String::as_str)
            .or(self.public_base_url.as_deref())
    }

    /// Bind the public base URL once the embedding server has selected its
    /// listener address.
    ///
    /// This is used by bound test servers whose ephemeral port is not known
    /// when the storage handle is first assembled. Production callers should
    /// configure `public_base_url` in [`StorageSettings`].
    ///
    /// Repeating the same assignment is harmless. A different assignment is
    /// rejected because local download plans must retain one public authority
    /// for the lifetime of this handle.
    ///
    /// # Errors
    /// Returns [`StorageError::PublicBaseUrlConflict`] when another URL was
    /// previously bound.
    pub fn set_public_base_url(&self, base_url: String) -> Result<(), StorageError> {
        let existing = self
            .public_base_url_override
            .get_or_init(|| base_url.clone());
        if existing == &base_url {
            Ok(())
        } else {
            Err(StorageError::PublicBaseUrlConflict {
                existing: existing.clone(),
                requested: base_url,
            })
        }
    }

    /// Probe the storage backend for liveness.
    ///
    /// For the local backend, attempts a `stat` of the configured storage root
    /// directory. For cloud backends (S3, GCS, Azure), issues a `stat` against
    /// `_wyrd/health.sentinel`; `NotFound` is treated as healthy (bucket
    /// accessible, sentinel absent is expected).
    ///
    /// # Errors
    ///
    /// Returns [`StorageHealthError`] when the probe times out or the backend
    /// returns an error that indicates inaccessibility.
    #[tracing::instrument(skip(self))]
    pub async fn health_probe(&self) -> Result<(), StorageHealthError> {
        if let BackendConfig::Local { root } = &self.backend_config {
            tokio::fs::metadata(root)
                .await
                .map(|_| ())
                .map_err(StorageHealthError::LocalRoot)
        } else {
            match self.operator.stat("_wyrd/health.sentinel").await {
                Ok(_) => Ok(()),
                Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
                Err(e) => Err(StorageHealthError::Backend(e)),
            }
        }
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
        let buf = self
            .operator
            .read(&path.full)
            .await
            .map_err(|e| self.map_operator_error(&e, "get_object", &path.full))?;
        Ok(buf.to_vec())
    }

    /// Write an object's bytes directly to the backend.
    ///
    /// # Errors
    /// Returns [`StorageError::Backend`] when the backend write fails.
    #[tracing::instrument(skip(self, bytes), fields(backend = ?self.backend(), len = bytes.len()))]
    pub async fn put_object(
        &self,
        path: &ValidatedPath,
        bytes: Vec<u8>,
    ) -> Result<(), StorageError> {
        #[cfg(not(feature = "cloud"))]
        {
            let BackendSigner::Local(local) = &self.signer;
            return local
                .write_atomically(std::path::Path::new(&path.full), &bytes)
                .await;
        }
        #[cfg(feature = "cloud")]
        if let BackendSigner::Local(local) = &self.signer {
            return local
                .write_atomically(std::path::Path::new(&path.full), &bytes)
                .await;
        }
        #[cfg(feature = "cloud")]
        self.operator
            .write(&path.full, bytes)
            .await
            .map_err(|e| self.map_operator_error(&e, "put_object", &path.full))?;
        #[cfg(feature = "cloud")]
        Ok(())
    }

    /// List object keys beneath a tenant-validated path.
    ///
    /// Returns the full backend keys of every file under `path` (recursive).
    ///
    /// # Errors
    /// Returns [`StorageError::Backend`] when the backend list fails.
    #[tracing::instrument(skip(self), fields(backend = ?self.backend()))]
    pub async fn list_objects(&self, path: &ValidatedPath) -> Result<Vec<String>, StorageError> {
        // Trailing slash scopes listing to objects strictly under this prefix;
        // opendal list_with without it also matches the prefix key itself.
        let prefix = format!("{}/", path.full);
        let entries = self
            .operator
            .list_with(&prefix)
            .recursive(true)
            .await
            .map_err(|e| self.map_operator_error(&e, "list_objects", &path.full))?;
        Ok(entries
            .into_iter()
            .filter(|entry| entry.metadata().mode() == EntryMode::FILE)
            .map(|entry| entry.path().to_owned())
            .collect())
    }

    /// Delete an object directly from the backend.
    ///
    /// # Errors
    /// Returns [`StorageError::Backend`] when the backend delete fails.
    #[tracing::instrument(skip(self), fields(backend = ?self.backend()))]
    pub async fn delete_object(&self, path: &ValidatedPath) -> Result<(), StorageError> {
        self.operator
            .delete(&path.full)
            .await
            .map_err(|e| self.map_operator_error(&e, "delete_object", &path.full))?;
        Ok(())
    }

    /// Map an opendal data-plane error onto the storage error catalog.
    fn map_operator_error(
        &self,
        error: &opendal::Error,
        op: &'static str,
        storage_path: &str,
    ) -> StorageError {
        if error.kind() == ErrorKind::NotFound {
            StorageError::ObjectNotFound {
                storage_path: storage_path.to_owned(),
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

    fn local_handle(root: &std::path::Path) -> StorageHandle {
        let signer =
            BackendSigner::Local(LocalSigner::new(root.to_path_buf()).expect("local signer"));
        StorageHandle::new(signer)
    }

    #[cfg(feature = "emulator")]
    #[test]
    fn from_signer_builds_cloud_handles_without_probing() {
        let gcs = crate::factory::gcs::build_emulator_signer(
            "wyrd-storage-test",
            "http://localhost:4443",
        )
        .expect("gcs emulator signer");
        let handle = StorageHandle::from_signer(
            BackendSigner::Cloud(Box::new(crate::cloud::CloudSigner::Gcs(gcs))),
            8 * 1024 * 1024,
        );
        assert_eq!(handle.backend(), StorageBackendKind::Gcs);
        assert_eq!(handle.multipart_threshold_bytes(), 8 * 1024 * 1024);

        let azure = crate::factory::azure::build_emulator_signer(
            "wyrd-storage-test",
            "http://127.0.0.1:10000",
        )
        .expect("azure emulator signer");
        let handle = StorageHandle::from_signer(
            BackendSigner::Cloud(Box::new(crate::cloud::CloudSigner::Azure(azure))),
            8 * 1024 * 1024,
        );
        assert_eq!(handle.backend(), StorageBackendKind::Azure);
        assert_eq!(handle.multipart_threshold_bytes(), 8 * 1024 * 1024);
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
}

#[cfg(test)]
mod from_settings_tests {
    use crate::{BackendSigner, StorageHandle};
    use std::sync::Arc;

    const ENV_KEYS: &[&str] = &[
        "WYRD_STORAGE_BACKEND",
        "WYRD_STORAGE_REQUIRE_ENCRYPTION",
        "WYRD_STORAGE_PRESIGN_TTL_SECS",
        "WYRD_STORAGE_PART_SIZE_BYTES",
        "WYRD_STORAGE_LOCAL_ROOT",
        "WYRD_PUBLIC_BASE_URL",
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
        assert_eq!(handle.public_base_url(), Some("https://wyrd.test"));
    }

    async fn local_handle() -> (Arc<StorageHandle>, tempfile::TempDir) {
        let root = tempfile::tempdir().expect("temp dir");
        let vars = vec![
            ("WYRD_STORAGE_BACKEND", Some("local".to_owned())),
            (
                "WYRD_STORAGE_LOCAL_ROOT",
                Some(root.path().display().to_string()),
            ),
            ("WYRD_PUBLIC_BASE_URL", Some("https://wyrd.test".to_owned())),
        ];
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
