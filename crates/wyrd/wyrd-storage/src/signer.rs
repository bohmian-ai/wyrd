//! Closed backend signer dispatch.
//!
//! Cloud backends (S3, GCS, Azure) live behind a single `Cloud` variant that is
//! compiled only under the `cloud` feature; see [`crate::cloud`]. Builds without
//! `cloud` carry only the local filesystem signer and drop the cloud SDKs.

use crate::error::StorageError;
use crate::local::LocalSigner;
use crate::tenant_path::ValidatedPath;
use std::time::Duration;
use wyrd_spec::storage::{S3CompletedPart, StorageBackendKind, UploadPlan, WireProtocol};

/// Active signer for one configured backend.
#[derive(Debug, Clone)]
pub enum BackendSigner {
    /// Local filesystem signer.
    Local(LocalSigner),
    /// Cloud backend signer (S3, GCS, or Azure).
    #[cfg(feature = "cloud")]
    Cloud(Box<crate::cloud::CloudSigner>),
}

impl BackendSigner {
    /// Return the backend kind.
    #[must_use]
    pub fn kind(&self) -> StorageBackendKind {
        match self {
            Self::Local(_) => StorageBackendKind::Local,
            #[cfg(feature = "cloud")]
            Self::Cloud(cloud) => cloud.kind(),
        }
    }

    /// Presign or describe a single PUT upload.
    ///
    /// # Errors
    /// Returns a backend error when signing fails or the operation is not yet
    /// implemented for the active backend.
    pub async fn presign_single_put(
        &self,
        path: &ValidatedPath,
        size_bytes: u64,
        ttl: Duration,
    ) -> Result<UploadPlan, StorageError> {
        match self {
            Self::Local(signer) => signer.presign_single_put(path, size_bytes, ttl).await,
            #[cfg(feature = "cloud")]
            Self::Cloud(cloud) => cloud.presign_single_put(path, size_bytes, ttl).await,
        }
    }

    /// Initiate a multipart upload.
    ///
    /// # Errors
    /// Returns a backend error when initialization fails or is not implemented
    /// for the active backend.
    pub async fn init_multipart(
        &self,
        path: &ValidatedPath,
        part_count: u32,
        part_size_bytes: u64,
        ttl: Duration,
    ) -> Result<MultipartInit, StorageError> {
        match self {
            Self::Local(signer) => {
                signer
                    .init_multipart(path, part_count, part_size_bytes, ttl)
                    .await
            }
            #[cfg(feature = "cloud")]
            Self::Cloud(cloud) => {
                cloud
                    .init_multipart(path, part_count, part_size_bytes, ttl)
                    .await
            }
        }
    }

    /// Presign one multipart part URL in a cloud-enabled build.
    ///
    /// # Errors
    /// Returns [`StorageError::BackendCapabilityMismatch`] for backends that do
    /// not use per-part presigning, or a backend error when cloud signing fails.
    #[cfg(feature = "cloud")]
    pub async fn presign_part(
        &self,
        path: &ValidatedPath,
        backend_upload_id: &str,
        part_number: u32,
        ttl: Duration,
    ) -> Result<String, StorageError> {
        #[cfg(not(feature = "cloud"))]
        std::future::ready(()).await;
        match self {
            #[cfg(feature = "cloud")]
            Self::Cloud(cloud) => {
                cloud
                    .presign_part(path, backend_upload_id, part_number, ttl)
                    .await
            }
            Self::Local(_) => Err(StorageError::BackendCapabilityMismatch {
                signer: self.kind(),
                op: "presign_part",
            }),
        }
    }

    /// Presign one multipart part URL in a build without cloud support.
    ///
    /// No local backend uses per-part presigning, so this returns the typed
    /// capability error in an immediately-ready future while preserving the
    /// awaitable signer API.
    ///
    /// # Errors
    /// Always returns [`StorageError::BackendCapabilityMismatch`].
    #[cfg(not(feature = "cloud"))]
    pub fn presign_part(
        &self,
        _path: &ValidatedPath,
        _backend_upload_id: &str,
        _part_number: u32,
        _ttl: Duration,
    ) -> std::future::Ready<Result<String, StorageError>> {
        std::future::ready(Err(StorageError::BackendCapabilityMismatch {
            signer: self.kind(),
            op: "presign_part",
        }))
    }

    /// Complete a server-side finalization step.
    ///
    /// # Errors
    /// Returns a backend error when finalization fails or the payload does not
    /// match the active backend.
    #[cfg_attr(not(feature = "cloud"), allow(unused_variables))]
    pub async fn complete_server_side(
        &self,
        path: &ValidatedPath,
        backend_upload_id: Option<&str>,
        complete: CompletePayload,
    ) -> Result<(), StorageError> {
        match (self, complete) {
            (Self::Local(signer), CompletePayload::Local) => {
                signer.finalize_temp_object(path).await
            }
            #[cfg(feature = "cloud")]
            (Self::Cloud(cloud), payload) => {
                cloud
                    .complete_server_side(path, backend_upload_id, payload)
                    .await
            }
            (signer, payload) => {
                tracing::error!(
                    backend = %signer.kind(),
                    payload = payload.kind(),
                    "storage complete_server_side capability mismatch"
                );
                Err(StorageError::BackendCapabilityMismatch {
                    signer: signer.kind(),
                    op: "complete_server_side",
                })
            }
        }
    }

    /// Abort a multipart upload in a cloud-enabled build.
    ///
    /// # Errors
    /// Returns a backend error when abort fails. GCS returns a capability
    /// mismatch because its resumable session URI is bearer-equivalent and is
    /// not persisted server-side.
    #[cfg(feature = "cloud")]
    pub async fn abort_multipart(
        &self,
        path: &ValidatedPath,
        backend_upload_id: &str,
    ) -> Result<(), StorageError> {
        #[cfg(not(feature = "cloud"))]
        std::future::ready(()).await;
        match self {
            Self::Local(_) => Ok(()),
            #[cfg(feature = "cloud")]
            Self::Cloud(cloud) => cloud.abort_multipart(path, backend_upload_id).await,
        }
    }

    /// Abort a multipart upload in a build without cloud support.
    ///
    /// Local uploads do not create remote multipart sessions, so abort is a
    /// deterministic no-op returned through an immediately-ready future.
    ///
    /// # Errors
    /// This operation does not fail for the local backend.
    #[cfg(not(feature = "cloud"))]
    pub fn abort_multipart(
        &self,
        _path: &ValidatedPath,
        _backend_upload_id: &str,
    ) -> std::future::Ready<Result<(), StorageError>> {
        std::future::ready(Ok(()))
    }

    /// Presign a GET URL.
    ///
    /// # Errors
    /// Returns a backend error when signing fails.
    pub async fn presign_get(
        &self,
        path: &ValidatedPath,
        ttl: Duration,
    ) -> Result<String, StorageError> {
        match self {
            Self::Local(signer) => signer.presign_get(path, ttl).await,
            #[cfg(feature = "cloud")]
            Self::Cloud(cloud) => cloud.presign_get(path, ttl).await,
        }
    }

    /// Fetch object metadata for completion verification.
    ///
    /// # Errors
    /// Returns a backend error when metadata cannot be read.
    pub async fn head_for_verification(
        &self,
        path: &ValidatedPath,
    ) -> Result<HeadInfo, StorageError> {
        match self {
            Self::Local(signer) => signer.head(path).await,
            #[cfg(feature = "cloud")]
            Self::Cloud(cloud) => cloud.head_for_verification(path).await,
        }
    }

    /// Re-mint an upload plan from non-bearer replay input.
    ///
    /// # Errors
    /// Returns a backend error when the replay input does not match the active
    /// backend or signing fails.
    pub async fn remint_plan(
        &self,
        path: &ValidatedPath,
        input: &UploadPlanReplayInput,
        ttl: Duration,
    ) -> Result<UploadPlan, StorageError> {
        match self {
            Self::Local(signer) => signer.remint_plan(path, input, ttl).await,
            #[cfg(feature = "cloud")]
            Self::Cloud(cloud) => cloud.remint_plan(path, input, ttl).await,
        }
    }
}

/// Multipart initialization result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultipartInit {
    /// Upload plan returned to the client.
    pub plan: UploadPlan,
    /// Non-bearer backend upload id persisted by the server when available.
    pub backend_upload_id: String,
}

/// Server-side completion payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletePayload {
    /// Cloud single PUT, server-verified only.
    SinglePut,
    /// S3 multipart completion payload.
    S3 {
        /// Completed S3 parts.
        parts: Vec<S3CompletedPart>,
        /// Client-declared base64 SHA-256 passed to S3 for server-side verification.
        expected_sha256: String,
    },
    /// Azure block blob completion payload.
    Azure {
        /// Planned block count reconstructed by the server.
        block_count: u32,
    },
    /// Local filesystem finalize.
    Local,
}

impl CompletePayload {
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::SinglePut => "single_put",
            Self::S3 { .. } => "s3",
            Self::Azure { .. } => "azure",
            Self::Local => "local",
        }
    }
}

/// Metadata used for upload completion verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadInfo {
    /// Stored object size in bytes.
    pub size_bytes: u64,
    /// Backend-specific encryption marker.
    pub sse_marker: Option<String>,
    /// Stored content type when known.
    pub content_type: Option<String>,
}

/// Non-bearer data needed to re-mint an upload plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadPlanReplayInput {
    /// Original wire protocol.
    pub wire_protocol: WireProtocol,
    /// Non-bearer backend upload id, if any.
    pub backend_upload_id: Option<String>,
    /// Planned part count.
    pub part_count_planned: u32,
    /// Planned part size.
    pub part_size_bytes: u64,
    /// Planned Azure block count.
    pub block_count_planned: Option<u32>,
}

/// Convert a duration to a saturated `u32` seconds value.
#[must_use]
pub(crate) fn ttl_secs(ttl: Duration) -> u32 {
    u32::try_from(ttl.as_secs()).unwrap_or(u32::MAX)
}
