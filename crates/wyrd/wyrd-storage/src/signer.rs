//! Closed backend signer dispatch.
//!
//! Cloud backends (S3, GCS, Azure) live behind a single `Cloud` variant that is
//! compiled only under the `cloud` feature; see [`crate::cloud`]. Builds without
//! `cloud` carry only the local filesystem signer and drop the cloud SDKs.

use crate::error::StorageError;
use crate::local::LocalSigner;
use crate::tenant_path::ValidatedPath;
use std::future::{Future, ready};
use std::pin::Pin;
use std::time::Duration;
use wyrd_spec::storage::{S3CompletedPart, StorageBackendKind, UploadPlan, WireProtocol};

/// The future every [`BackendSigner`] signing operation returns.
///
/// One boxed type lets each operation keep a single definition whatever
/// backends a build compiles in: the local signer resolves immediately and a
/// cloud signer awaits its SDK, and both arms coerce to this one type.
pub type SignerFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, StorageError>> + Send + 'a>>;

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
    /// Local signing does no IO and resolves immediately; cloud signing awaits
    /// the backend SDK.
    ///
    /// # Errors
    /// Returns an invalid URI error when a local path cannot be rendered as a
    /// file URL, or a backend error when cloud signing fails.
    pub fn presign_single_put<'a>(
        &'a self,
        path: &'a ValidatedPath,
        size_bytes: u64,
        ttl: Duration,
    ) -> SignerFuture<'a, UploadPlan> {
        match self {
            Self::Local(signer) => {
                Box::pin(ready(signer.presign_single_put(path, size_bytes, ttl)))
            }
            #[cfg(feature = "cloud")]
            Self::Cloud(cloud) => Box::pin(cloud.presign_single_put(path, size_bytes, ttl)),
        }
    }

    /// Initiate a multipart upload.
    ///
    /// A local upload is one file write, so the local signer resolves
    /// immediately with its single-PUT plan; cloud backends await the SDK.
    ///
    /// # Errors
    /// Returns an invalid URI error when a local path cannot be rendered as a
    /// file URL, or a backend error when cloud initialization fails.
    pub fn init_multipart<'a>(
        &'a self,
        path: &'a ValidatedPath,
        part_count: u32,
        part_size_bytes: u64,
        ttl: Duration,
    ) -> SignerFuture<'a, MultipartInit> {
        match self {
            Self::Local(signer) => Box::pin(ready(signer.init_multipart(
                path,
                part_count,
                part_size_bytes,
                ttl,
            ))),
            #[cfg(feature = "cloud")]
            Self::Cloud(cloud) => {
                Box::pin(cloud.init_multipart(path, part_count, part_size_bytes, ttl))
            }
        }
    }

    /// Presign one multipart part URL.
    ///
    /// # Errors
    /// Returns [`StorageError::BackendCapabilityMismatch`] for backends that do
    /// not use per-part presigning, including the local backend, or a backend
    /// error when cloud signing fails.
    pub fn presign_part<'a>(
        &'a self,
        path: &'a ValidatedPath,
        backend_upload_id: &'a str,
        part_number: u32,
        ttl: Duration,
    ) -> SignerFuture<'a, String> {
        match self {
            #[cfg(feature = "cloud")]
            Self::Cloud(cloud) => {
                Box::pin(cloud.presign_part(path, backend_upload_id, part_number, ttl))
            }
            Self::Local(_) => {
                // The local backend signs no parts, so the request is refused
                // whatever it names.
                let _ = (path, backend_upload_id, part_number, ttl);
                Box::pin(ready(Err(StorageError::BackendCapabilityMismatch {
                    signer: self.kind(),
                    op: "presign_part",
                })))
            }
        }
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

    /// Abort a multipart upload.
    ///
    /// Local uploads create no remote multipart session, so a local abort is a
    /// no-op that resolves immediately.
    ///
    /// # Errors
    /// Returns a backend error when a cloud abort fails. GCS returns a
    /// capability mismatch because its resumable session URI is
    /// bearer-equivalent and is not persisted server-side.
    pub fn abort_multipart<'a>(
        &'a self,
        path: &'a ValidatedPath,
        backend_upload_id: &'a str,
    ) -> SignerFuture<'a, ()> {
        match self {
            Self::Local(_) => {
                // No remote session exists to name.
                let _ = (path, backend_upload_id);
                Box::pin(ready(Ok(())))
            }
            #[cfg(feature = "cloud")]
            Self::Cloud(cloud) => Box::pin(cloud.abort_multipart(path, backend_upload_id)),
        }
    }

    /// Presign a GET URL.
    ///
    /// Local signing does no IO and resolves immediately; cloud signing awaits
    /// the backend SDK.
    ///
    /// # Errors
    /// Returns an invalid URI error when a local path cannot be rendered as a
    /// file URL, or a backend error when cloud signing fails.
    pub fn presign_get<'a>(
        &'a self,
        path: &'a ValidatedPath,
        ttl: Duration,
    ) -> SignerFuture<'a, String> {
        match self {
            Self::Local(signer) => Box::pin(ready(signer.presign_get(path, ttl))),
            #[cfg(feature = "cloud")]
            Self::Cloud(cloud) => Box::pin(cloud.presign_get(path, ttl)),
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
    /// Local signing does no IO and resolves immediately; cloud signing awaits
    /// the backend SDK.
    ///
    /// # Errors
    /// Returns a capability mismatch when the replay input does not match the
    /// active backend, an invalid URI error when a local path cannot be
    /// rendered as a file URL, or a backend error when cloud signing fails.
    pub fn remint_plan<'a>(
        &'a self,
        path: &'a ValidatedPath,
        input: &'a UploadPlanReplayInput,
        ttl: Duration,
    ) -> SignerFuture<'a, UploadPlan> {
        match self {
            Self::Local(signer) => Box::pin(ready(signer.remint_plan(path, input, ttl))),
            #[cfg(feature = "cloud")]
            Self::Cloud(cloud) => Box::pin(cloud.remint_plan(path, input, ttl)),
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
