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
use wyrd_spec::storage::{
    DownloadPlan, PartUrlResponse, UploadCompleteRequest, UploadCompleteResponse, UploadId,
    UploadPlan,
};

use crate::upload::{UploadHooks, UploadOutcome};

pub mod download;
pub mod error;
pub mod upload;

pub use download::DownloadOutcome;
pub use error::StorageClientError;
pub use upload::{ArtifactSource, FileSource};

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

    /// Upload one server-planned artifact and complete its server-owned upload
    /// record when the backend protocol requires a completion request.
    ///
    /// This is the only artifact-upload operation exposed to registry callers.
    /// Provider-specific dispatch, part URL minting, backend outcomes, and the
    /// server completion request remain inside this client.
    ///
    /// # Errors
    /// Returns a storage-client error when the plan is invalid, the source
    /// cannot be read, the backend transfer fails, or server completion fails.
    pub async fn upload_artifact<S: ArtifactSource>(
        &self,
        upload_id: &UploadId,
        plan: &UploadPlan,
        source: S,
        idempotency_key: &str,
    ) -> Result<(), StorageClientError> {
        let part_url_minter = upload::s3_part_url_minter(self, upload_id, plan);
        let outcome = self
            .upload(
                plan,
                source,
                upload::UploadHooks {
                    idempotency_key,
                    progress: None,
                    part_url_minter,
                },
            )
            .await?;
        if let Some(request) = outcome.into_server_complete() {
            self.complete(upload_id, &request, idempotency_key).await?;
        }
        Ok(())
    }

    /// Dispatch an artifact according to a server-minted plan.
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
    async fn upload<S: ArtifactSource>(
        &self,
        plan: &UploadPlan,
        source: S,
        hooks: UploadHooks<'_>,
    ) -> Result<UploadOutcome, StorageClientError> {
        upload::dispatch(&self.client, plan, source, hooks).await
    }

    /// Mint one S3 multipart part URL through the authenticated Wyrd server.
    ///
    /// The returned URL is provider-presigned and must be used without Wyrd
    /// credentials. The storage service remains responsible for tenant checks
    /// and validating the part against the durable upload row.
    ///
    /// # Errors
    /// Returns a structured server error when the upload is missing, expired,
    /// or is not an S3 multipart upload.
    async fn part_url(
        &self,
        upload_id: &UploadId,
        part_number: u32,
    ) -> Result<String, StorageClientError> {
        let path = format!("/v1/cards/upload/{upload_id}/part-url?part_number={part_number}");
        let response: PartUrlResponse = self
            .client
            .request_json(reqwest::Method::POST, &path, None::<&()>)
            .await
            .map_err(crate::error::from_authenticated)?;
        Ok(response.url)
    }

    /// Complete a server-owned upload after the backend accepted its bytes.
    ///
    /// S3 multipart and Azure block uploads require this call to commit their
    /// backend state. The server then verifies the stored object and advances
    /// the durable upload row.
    ///
    /// # Errors
    /// Returns a structured server error when completion fails validation,
    /// backend commit, object verification, or tenant authorization.
    async fn complete(
        &self,
        upload_id: &UploadId,
        request: &UploadCompleteRequest,
        idempotency_key: &str,
    ) -> Result<UploadCompleteResponse, StorageClientError> {
        let path = format!("/v1/cards/upload/{upload_id}/complete");
        self.client
            .submit_with_idempotency_key(reqwest::Method::POST, &path, request, idempotency_key)
            .await
            .map_err(crate::error::from_authenticated)
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
