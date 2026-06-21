//! Local filesystem backend factory.

use crate::error::StorageError;
use crate::local::LocalSigner;
use object_store::local::LocalFileSystem;
use opendal::services;
use std::path::{Path, PathBuf};

pub(crate) fn fs_service(root: &Path) -> services::Fs {
    services::Fs::default().root(&root.to_string_lossy())
}

/// Build the local signer.
///
/// # Errors
/// Returns an error when the root is invalid.
pub fn build_signer(root: PathBuf) -> Result<LocalSigner, StorageError> {
    LocalSigner::new(root)
}

/// Build the local object store.
///
/// # Errors
/// Returns an error when the root cannot be canonicalized.
pub fn build_object_store(root: &Path) -> Result<LocalFileSystem, StorageError> {
    LocalFileSystem::new_with_prefix(root).map_err(|source| StorageError::Backend {
        backend: wyrd_spec::storage::StorageBackendKind::Local,
        op: "build_object_store",
        message: source.to_string(),
    })
}
