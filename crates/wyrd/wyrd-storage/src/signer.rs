//! Closed backend signer dispatch.

use crate::azure::AzureSigner;
use crate::error::StorageError;
use crate::gcs::GcsSigner;
use crate::local::LocalSigner;
use crate::s3::S3Signer;
use crate::tenant_path::ValidatedPath;
use std::time::Duration;
use wyrd_spec::storage::{S3CompletedPart, StorageBackendKind, UploadPlan, WireProtocol};

/// Active signer for one configured backend.
#[derive(Debug, Clone)]
pub enum BackendSigner {
    /// Local filesystem signer.
    Local(LocalSigner),
    /// AWS S3 signer.
    S3(S3Signer),
    /// Google Cloud Storage signer.
    Gcs(GcsSigner),
    /// Azure Blob Storage signer.
    Azure(AzureSigner),
}

impl BackendSigner {
    /// Return the backend kind.
    #[must_use]
    pub fn kind(&self) -> StorageBackendKind {
        match self {
            Self::Local(_) => StorageBackendKind::Local,
            Self::S3(_) => StorageBackendKind::S3,
            Self::Gcs(_) => StorageBackendKind::Gcs,
            Self::Azure(_) => StorageBackendKind::Azure,
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
            Self::S3(signer) => signer.presign_single_put(path, size_bytes, ttl).await,
            Self::Gcs(signer) => signer.presign_single_put(path, size_bytes, ttl).await,
            Self::Azure(signer) => signer.presign_single_put(path, size_bytes, ttl).await,
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
            Self::S3(signer) => {
                signer
                    .init_multipart(path, part_count, part_size_bytes, ttl)
                    .await
            }
            Self::Gcs(signer) => {
                signer
                    .init_multipart(path, part_count, part_size_bytes, ttl)
                    .await
            }
            Self::Azure(signer) => {
                signer
                    .init_multipart(path, part_count, part_size_bytes, ttl)
                    .await
            }
        }
    }

    /// Presign one multipart part URL.
    ///
    /// # Errors
    /// Returns [`StorageError::BackendCapabilityMismatch`] for backends that do
    /// not use per-part presigning.
    pub async fn presign_part(
        &self,
        path: &ValidatedPath,
        backend_upload_id: &str,
        part_number: u32,
        ttl: Duration,
    ) -> Result<String, StorageError> {
        match self {
            Self::S3(signer) => {
                signer
                    .presign_part(path, backend_upload_id, part_number, ttl)
                    .await
            }
            Self::Local(_) | Self::Gcs(_) | Self::Azure(_) => {
                Err(StorageError::BackendCapabilityMismatch {
                    signer: self.kind(),
                    op: "presign_part",
                })
            }
        }
    }

    /// Complete a server-side finalization step.
    ///
    /// # Errors
    /// Returns a backend error when finalization fails or the payload does not
    /// match the active backend.
    pub async fn complete_server_side(
        &self,
        path: &ValidatedPath,
        backend_upload_id: Option<&str>,
        complete: CompletePayload,
    ) -> Result<(), StorageError> {
        match (self, complete) {
            (
                Self::S3(signer),
                CompletePayload::S3 {
                    parts,
                    expected_sha256,
                },
            ) => {
                let upload_id =
                    backend_upload_id.ok_or(StorageError::BackendCapabilityMismatch {
                        signer: StorageBackendKind::S3,
                        op: "complete_server_side",
                    })?;
                signer
                    .complete_multipart(path, upload_id, &parts, &expected_sha256)
                    .await
            }
            (Self::Azure(signer), CompletePayload::Azure { block_count }) => {
                signer.complete_blocklist_server(path, block_count).await
            }
            (Self::Local(signer), CompletePayload::Local) => {
                signer.finalize_temp_object(path).await
            }
            (Self::Gcs(_), _) => Err(StorageError::BackendCapabilityMismatch {
                signer: StorageBackendKind::Gcs,
                op: "complete_server_side",
            }),
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

    /// Verify SHA-256 for a stored object.
    ///
    /// # Errors
    /// Returns [`StorageError::Sha256Mismatch`] when verification fails, or a
    /// backend error when the object cannot be read.
    pub async fn verify_sha256(
        &self,
        path: &ValidatedPath,
        expected: &str,
        head_hint: &HeadInfo,
    ) -> Result<(), StorageError> {
        match self {
            Self::Local(signer) => signer.verify_sha256(path, expected, head_hint).await,
            Self::S3(signer) => signer.verify_sha256(path, expected, head_hint),
            Self::Gcs(signer) => signer.verify_sha256(path, expected, head_hint).await,
            Self::Azure(signer) => signer.verify_sha256(path, expected, head_hint).await,
        }
    }

    /// Abort a multipart upload.
    ///
    /// # Errors
    /// Returns a backend error when abort fails. GCS returns a capability
    /// mismatch because its resumable session URI is bearer-equivalent and is
    /// not persisted server-side.
    pub async fn abort_multipart(
        &self,
        path: &ValidatedPath,
        backend_upload_id: &str,
    ) -> Result<(), StorageError> {
        match self {
            Self::Local(_) => Ok(()),
            Self::S3(signer) => signer.abort_multipart(path, backend_upload_id).await,
            Self::Azure(signer) => signer.abort_multipart(path).await,
            Self::Gcs(_) => Err(StorageError::BackendCapabilityMismatch {
                signer: StorageBackendKind::Gcs,
                op: "abort_multipart",
            }),
        }
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
            Self::S3(signer) => signer.presign_get(path, ttl).await,
            Self::Gcs(signer) => signer.presign_get(path, ttl).await,
            Self::Azure(signer) => signer.presign_get(path, ttl).await,
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
            Self::S3(signer) => signer.head(path).await,
            Self::Gcs(signer) => signer.head(path).await,
            Self::Azure(signer) => signer.head(path).await,
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
            Self::S3(signer) => signer.remint_plan(path, input, ttl).await,
            Self::Gcs(signer) => signer.remint_plan(path, input, ttl).await,
            Self::Azure(signer) => signer.remint_plan(path, input, ttl).await,
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
    fn kind(&self) -> &'static str {
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
    /// Trusted backend-supplied base64 SHA-256 when available.
    pub sha256_b64: Option<String>,
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

/// Compare an expected SHA against an actual SHA.
pub(crate) fn check_sha(expected: &str, actual: String) -> Result<(), StorageError> {
    if expected == actual {
        Ok(())
    } else {
        Err(StorageError::Sha256Mismatch {
            expected: expected.to_owned(),
            actual,
        })
    }
}
