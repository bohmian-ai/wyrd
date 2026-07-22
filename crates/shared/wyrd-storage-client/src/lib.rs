//! Client-side execution of server-minted artifact transfer plans.
//!
//! [`WyrdStorageClient`] composes over a shared [`WyrdClient`], which is the
//! sole source of truth for Wyrd auth and transport. LocalFs plans reuse
//! the authenticated transport; cloud plans (S3/GCS/Azure) go through the
//! same connection pool via
//! [`WyrdClient::request_external_stream`], without leaking Wyrd
//! credentials cross-origin.

#![deny(missing_docs)]

use std::path::Path;

use wyrd_client::WyrdClient;
use wyrd_spec::storage::{DownloadPlan, UploadPlan};

pub mod download;
pub mod error;
pub mod upload;

pub use download::DownloadOutcome;
pub use error::StorageClientError;
pub use upload::{
    ArtifactSource, FileSource, PartUrlFuture, PartUrlMinter, UploadHooks, UploadOutcome,
};

/// Storage transfer handle that dispatches every HTTP call through a shared
/// [`WyrdClient`].
///
/// Auth, retry policy, base URL resolution, and the reqwest connection pool
/// all come from the wrapped [`WyrdClient`]; this handle owns only the plan
/// dispatch logic.
#[derive(Clone)]
pub struct WyrdStorageClient {
    client: WyrdClient,
}

impl WyrdStorageClient {
    /// Build a storage client that shares the given [`WyrdClient`]'s auth and
    /// transport.
    #[must_use]
    pub fn new(client: &WyrdClient) -> Self {
        Self {
            client: client.clone(),
        }
    }

    /// Upload an artifact according to a server-minted plan.
    ///
    /// # Arguments
    ///
    /// * `plan` - Server-generated upload plan containing storage backend
    ///   details, part sizing, and presigned URLs or authenticated endpoints
    /// * `source` - Artifact data source (file, bytes, or stream)
    /// * `hooks` - Progress callbacks and idempotency key
    ///
    /// # Returns
    ///
    /// * `UploadOutcome::Uploaded` - Single-PUT completed successfully
    /// * `UploadOutcome::NeedsServerComplete` - Multipart upload requires
    ///   server-side completion call
    ///
    /// # Errors
    ///
    /// * Transport failures, 5xx responses (retried up to 3 times for multipart)
    /// * Plan/source size mismatch
    /// * Backend-specific errors (S3/GCS/Azure)
    pub async fn upload<S: ArtifactSource>(
        &self,
        plan: &UploadPlan,
        source: S,
        hooks: UploadHooks<'_>,
    ) -> Result<UploadOutcome, StorageClientError> {
        upload::dispatch(&self.client, plan, source, hooks).await
    }

    /// Download an artifact according to a server-minted plan.
    ///
    /// # Arguments
    ///
    /// * `plan` - Server-generated download plan with presigned GET URL or
    ///   authenticated LocalFs endpoint
    /// * `dest` - Filesystem path where the downloaded artifact will be written
    ///
    /// # Returns
    ///
    /// `DownloadOutcome` with total bytes written to disk
    ///
    /// # Errors
    ///
    /// * Transport failures
    /// * Filesystem write errors
    /// * Non-success HTTP status from storage backend
    pub async fn download(
        &self,
        plan: &DownloadPlan,
        dest: &Path,
    ) -> Result<DownloadOutcome, StorageClientError> {
        download::dispatch(&self.client, plan, dest).await
    }

    /// Download an artifact and verify it against the server-declared digest
    /// and byte length.
    ///
    /// This is an internal-capability seam for registry loading. The public
    /// registry handle owns artifact selection; this client owns transfer and
    /// byte verification.
    ///
    /// # Errors
    /// Returns [`StorageClientError::VerifyFailed`] when the downloaded bytes
    /// do not match either declared value.
    pub async fn download_verified(
        &self,
        plan: &DownloadPlan,
        dest: &Path,
        expected_sha256: &str,
        expected_size_bytes: u64,
    ) -> Result<DownloadOutcome, StorageClientError> {
        download::dispatch_verified(
            &self.client,
            plan,
            dest,
            expected_sha256,
            expected_size_bytes,
        )
        .await
    }
}
