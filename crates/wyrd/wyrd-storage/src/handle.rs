//! Storage handle shared by server-tier crates.

use crate::error::StorageError;
use crate::settings::{BackendConfig, StorageSettings};
use crate::signer::BackendSigner;
use object_store::ObjectStore;
use std::sync::Arc;
use std::time::Duration;
use wyrd_spec::storage::StorageBackendKind;

/// Shared storage handle.
///
/// The handle carries both the active backend signer and the single
/// process-wide object-store substrate used by server-side readers.
#[derive(Clone)]
pub struct StorageHandle {
    signer: BackendSigner,
    object_store: Arc<dyn ObjectStore>,
    backend_config: BackendConfig,
    require_encryption: bool,
    presign_ttl: Duration,
    default_part_size_bytes: u64,
    public_base_url: Option<String>,
}

impl std::fmt::Debug for StorageHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StorageHandle")
            .field("backend", &self.signer.kind())
            .field("object_store", &"<dyn ObjectStore>")
            .field("require_encryption", &self.require_encryption)
            .field("presign_ttl", &self.presign_ttl)
            .field("default_part_size_bytes", &self.default_part_size_bytes)
            .finish_non_exhaustive()
    }
}

impl StorageHandle {
    /// Build a storage handle from already-constructed components.
    #[must_use]
    pub fn new(signer: BackendSigner, object_store: Arc<dyn ObjectStore>) -> Self {
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
        Self {
            signer,
            object_store,
            backend_config,
            require_encryption: false,
            presign_ttl: Duration::from_secs(u64::from(crate::settings::DEFAULT_PRESIGN_TTL_SECS)),
            default_part_size_bytes: crate::settings::DEFAULT_PART_SIZE_BYTES,
            public_base_url: None,
        }
    }

    /// Build a storage handle from boot settings.
    ///
    /// # Errors
    /// Returns a storage error when signer or object-store construction fails.
    pub async fn from_settings(settings: StorageSettings) -> Result<Arc<Self>, StorageError> {
        let signer = crate::factory::build_signer(&settings.backend).await?;
        let object_store = crate::factory::build_object_store(&settings.backend)?;
        crate::preflight::run(&signer).await;

        Ok(Arc::new(Self {
            signer,
            object_store,
            backend_config: settings.backend,
            require_encryption: settings.require_encryption,
            presign_ttl: settings.presign_ttl,
            default_part_size_bytes: settings.part_size_bytes,
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

    /// Clone the shared object-store substrate.
    #[must_use]
    pub fn object_store(&self) -> Arc<dyn ObjectStore> {
        Arc::clone(&self.object_store)
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

    /// Public base URL configured for local-mode routes.
    #[must_use]
    pub fn public_base_url(&self) -> Option<&str> {
        self.public_base_url.as_deref()
    }
}
