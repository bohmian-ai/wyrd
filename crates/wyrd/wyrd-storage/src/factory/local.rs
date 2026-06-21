//! Local filesystem backend factory.

use crate::error::StorageError;
use crate::local::LocalSigner;
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
