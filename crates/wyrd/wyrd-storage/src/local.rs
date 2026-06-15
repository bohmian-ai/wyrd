//! Local filesystem backend signer.

use crate::error::{LocalError, StorageError};
use crate::sha::stream_sha256;
use crate::signer::{HeadInfo, MultipartInit, UploadPlanReplayInput, check_sha, ttl_secs};
use crate::tenant_path::ValidatedPath;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use url::Url;
use wyrd_spec::storage::{HeaderPair, UploadPlan, WireProtocol};

/// Local filesystem signer.
#[derive(Debug, Clone)]
pub struct LocalSigner {
    root: PathBuf,
}

impl LocalSigner {
    /// Build a local signer from an absolute root path.
    ///
    /// # Errors
    /// Returns an error when the root is not absolute or does not exist.
    pub fn new(root: PathBuf) -> Result<Self, StorageError> {
        if !root.is_absolute() {
            return Err(StorageError::Local(LocalError::NonAbsoluteRoot(
                root.display().to_string(),
            )));
        }
        if !root.exists() {
            return Err(StorageError::Local(LocalError::RootNotFound(
                root.display().to_string(),
            )));
        }
        Ok(Self { root })
    }

    /// Return the configured root path.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Ensure a directory exists under the local root.
    ///
    /// # Errors
    /// Returns an IO error if directory creation fails.
    pub async fn mkdir_p(&self, relative: &Path) -> Result<(), StorageError> {
        fs::create_dir_all(self.root.join(relative)).await?;
        Ok(())
    }

    /// Write bytes atomically under the local root.
    ///
    /// # Errors
    /// Returns an IO error if writing, syncing, or renaming fails.
    pub async fn write_atomically(
        &self,
        relative: &Path,
        bytes: &[u8],
    ) -> Result<(), StorageError> {
        let target = self.root.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).await?;
        }
        let temp = target.with_extension("tmp");
        let mut file = fs::File::create(&temp).await?;
        file.write_all(bytes).await?;
        file.sync_all().await?;
        drop(file);
        fs::rename(temp, target).await?;
        Ok(())
    }

    /// Copy a stored object into a destination file.
    ///
    /// # Errors
    /// Returns an IO error if reading or writing fails.
    pub async fn read_into(&self, path: &ValidatedPath, dest: &Path) -> Result<(), StorageError> {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::copy(self.object_path(path), dest)
            .await
            .map_err(|err| LocalError::from_io(err, &path.full))?;
        Ok(())
    }

    /// Return a local single PUT plan.
    ///
    /// # Errors
    /// Returns an invalid URI error if the local file path cannot be rendered
    /// as a file URL.
    pub async fn presign_single_put(
        &self,
        path: &ValidatedPath,
        _size_bytes: u64,
        ttl: Duration,
    ) -> Result<UploadPlan, StorageError> {
        Ok(UploadPlan::SinglePut {
            put_url: self.file_url(path)?,
            ttl_secs: ttl_secs(ttl),
            required_headers: Vec::<HeaderPair>::new(),
        })
    }

    /// Return the local single PUT plan for multipart init.
    ///
    /// # Errors
    /// Returns an invalid URI error if the local file path cannot be rendered
    /// as a file URL.
    pub async fn init_multipart(
        &self,
        path: &ValidatedPath,
        _part_count: u32,
        _part_size_bytes: u64,
        ttl: Duration,
    ) -> Result<MultipartInit, StorageError> {
        Ok(MultipartInit {
            plan: self.presign_single_put(path, 0, ttl).await?,
            backend_upload_id: String::new(),
        })
    }

    /// Finalize a local temporary object.
    ///
    /// # Errors
    /// Returns an IO error if the parent directory cannot be synced.
    pub async fn finalize_temp_object(&self, path: &ValidatedPath) -> Result<(), StorageError> {
        if let Some(parent) = self.object_path(path).parent() {
            let dir = fs::File::open(parent).await?;
            dir.sync_all().await?;
        }
        Ok(())
    }

    /// Return a local GET URL.
    ///
    /// # Errors
    /// Returns an invalid URI error if the local file path cannot be rendered
    /// as a file URL.
    pub async fn presign_get(
        &self,
        path: &ValidatedPath,
        _ttl: Duration,
    ) -> Result<String, StorageError> {
        self.file_url(path)
    }

    /// Read local object metadata.
    ///
    /// # Errors
    /// Returns a local error if metadata cannot be read.
    pub async fn head(&self, path: &ValidatedPath) -> Result<HeadInfo, StorageError> {
        let metadata = fs::metadata(self.object_path(path))
            .await
            .map_err(|err| LocalError::from_io(err, &path.full))?;
        Ok(HeadInfo {
            size_bytes: metadata.len(),
            sse_marker: Some("none".to_owned()),
            content_type: None,
            sha256_b64: None,
        })
    }

    /// Verify SHA-256 by streaming the local object.
    ///
    /// # Errors
    /// Returns a local IO error or SHA mismatch.
    pub async fn verify_sha256(
        &self,
        path: &ValidatedPath,
        expected: &str,
        _head_hint: &HeadInfo,
    ) -> Result<(), StorageError> {
        let mut file = fs::File::open(self.object_path(path))
            .await
            .map_err(|err| LocalError::from_io(err, &path.full))?;
        let actual = stream_sha256(&mut file).await?;
        check_sha(expected, actual)
    }

    /// Re-mint a local upload plan.
    ///
    /// # Errors
    /// Returns a capability mismatch if the replay protocol is not local.
    pub async fn remint_plan(
        &self,
        path: &ValidatedPath,
        input: &UploadPlanReplayInput,
        ttl: Duration,
    ) -> Result<UploadPlan, StorageError> {
        if input.wire_protocol != WireProtocol::LocalFsV1 {
            return Err(StorageError::BackendCapabilityMismatch {
                signer: wyrd_spec::storage::StorageBackendKind::Local,
                op: "remint_plan",
            });
        }
        self.presign_single_put(path, input.part_size_bytes, ttl)
            .await
    }

    fn object_path(&self, path: &ValidatedPath) -> PathBuf {
        self.root.join(&path.full)
    }

    fn file_url(&self, path: &ValidatedPath) -> Result<String, StorageError> {
        Url::from_file_path(self.object_path(path))
            .map(|url| url.to_string())
            .map_err(|()| StorageError::InvalidUri(self.object_path(path).display().to_string()))
    }
}
