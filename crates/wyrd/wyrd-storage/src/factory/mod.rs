//! Backend SDK and object-store construction.

pub mod azure;
pub mod gcs;
pub mod local;
pub mod s3;

use crate::error::StorageError;
use crate::settings::BackendConfig;
use crate::signer::BackendSigner;
use object_store::ObjectStore;
use std::sync::Arc;

/// Build the active backend signer.
///
/// # Errors
/// Returns a storage error when SDK construction or boot probing fails.
pub async fn build_signer(backend: &BackendConfig) -> Result<BackendSigner, StorageError> {
    match backend {
        BackendConfig::Local { root } => {
            Ok(BackendSigner::Local(local::build_signer(root.clone())?))
        }
        BackendConfig::S3(config) => Ok(BackendSigner::S3(s3::build_signer(config).await?)),
        BackendConfig::Gcs(config) => Ok(BackendSigner::Gcs(gcs::build_signer(config).await?)),
        BackendConfig::Azure(config) => {
            Ok(BackendSigner::Azure(azure::build_signer(config).await?))
        }
    }
}

/// Build the single process-level object store for the selected backend.
///
/// # Errors
/// Returns a storage error when object-store construction fails.
pub fn build_object_store(backend: &BackendConfig) -> Result<Arc<dyn ObjectStore>, StorageError> {
    let store: Arc<dyn ObjectStore> = match backend {
        BackendConfig::Local { root } => Arc::new(local::build_object_store(root)?),
        BackendConfig::S3(config) => Arc::new(s3::build_object_store(config)?),
        BackendConfig::Gcs(config) => Arc::new(gcs::build_object_store(config)?),
        BackendConfig::Azure(config) => Arc::new(azure::build_object_store(config)?),
    };
    Ok(store)
}
