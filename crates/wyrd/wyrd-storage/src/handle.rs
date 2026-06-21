//! Storage handle shared by server-tier crates.

use crate::error::StorageError;
use crate::settings::{BackendConfig, StorageSettings};
use crate::signer::BackendSigner;
use opendal::{ErrorKind, Operator};
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
    /// Panics if opendal operator construction fails for the synthesized local
    /// config. All documented call sites pass a `Local` signer; fs-operator
    /// construction is infallible.
    #[must_use]
    pub fn new(signer: BackendSigner) -> Self {
        let backend_config = match &signer {
            BackendSigner::Local(local) => BackendConfig::Local {
                root: local.root().to_path_buf(),
            },
            BackendSigner::S3(s3) => BackendConfig::S3(crate::settings::S3Config {
                bucket: s3.bucket().to_owned(),
                region: None,
                endpoint_url: None,
                force_path_style: false,
            }),
            BackendSigner::Gcs(gcs) => BackendConfig::Gcs(crate::settings::GcsConfig {
                bucket: gcs.bucket().to_owned(),
            }),
            BackendSigner::Azure(azure) => BackendConfig::Azure(crate::settings::AzureConfig {
                account: azure.account().to_owned(),
                container: azure.container().to_owned(),
            }),
        };
        let operator = crate::factory::build_operator(&backend_config)
            .expect("operator construction is infallible for the test/local-harness constructor");
        Self {
            signer,
            operator,
            backend_config,
            require_encryption: false,
            presign_ttl: Duration::from_secs(u64::from(crate::settings::DEFAULT_PRESIGN_TTL_SECS)),
            default_part_size_bytes: crate::settings::DEFAULT_PART_SIZE_BYTES,
            multipart_threshold_bytes: crate::plan::MULTIPART_THRESHOLD_BYTES,
            public_base_url: None,
        }
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
        self.public_base_url.as_deref()
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local::LocalSigner;
    use tempfile::TempDir;

    fn local_handle(root: &std::path::Path) -> StorageHandle {
        let signer = BackendSigner::Local(LocalSigner::new(root.to_path_buf()).unwrap());
        StorageHandle::new(signer)
    }

    #[test]
    fn from_signer_builds_cloud_handles_without_probing() {
        let gcs = crate::factory::gcs::build_emulator_signer("wyrd-storage-test", "http://localhost:4443")
            .expect("gcs emulator signer");
        let handle = StorageHandle::from_signer(BackendSigner::Gcs(gcs), 8 * 1024 * 1024);
        assert_eq!(handle.backend(), StorageBackendKind::Gcs);
        assert_eq!(handle.multipart_threshold_bytes(), 8 * 1024 * 1024);

        let azure =
            crate::factory::azure::build_emulator_signer("wyrd-storage-test", "http://127.0.0.1:10000")
                .expect("azure emulator signer");
        let handle = StorageHandle::from_signer(BackendSigner::Azure(azure), 8 * 1024 * 1024);
        assert_eq!(handle.backend(), StorageBackendKind::Azure);
        assert_eq!(handle.multipart_threshold_bytes(), 8 * 1024 * 1024);
    }

    #[tokio::test]
    async fn health_probe_local_root_exists() {
        let dir = TempDir::new().unwrap();
        let handle = local_handle(dir.path());
        assert!(handle.health_probe().await.is_ok());
    }

    #[tokio::test]
    async fn health_probe_local_missing_root() {
        let dir = TempDir::new().unwrap();
        let handle = local_handle(dir.path());
        // Remove the directory after creating the handle to simulate inaccessible root.
        std::fs::remove_dir_all(dir.path()).unwrap();
        let result = handle.health_probe().await;
        assert!(
            matches!(result, Err(StorageHealthError::LocalRoot(_))),
            "expected LocalRoot error, got {result:?}"
        );
    }
}
