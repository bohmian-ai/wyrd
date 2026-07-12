//! Cloud backend signer dispatch (S3, GCS, Azure).
//!
//! Compiled only under the `cloud` feature. All cloud-SDK-dependent match logic
//! is centralized here so the rest of the crate stays cloud-free and the cloud
//! SDKs (with their heavy dependency trees) drop out of any build that does not
//! enable `cloud`. [`crate::signer::BackendSigner`] holds a single `Cloud`
//! variant wrapping [`CloudSigner`]; the fail-closed boundary for cloud-disabled
//! builds lives in [`crate::factory`].

use crate::azure::AzureSigner;
use crate::error::StorageError;
use crate::gcs::GcsSigner;
use crate::s3::S3Signer;
use crate::settings::{AzureConfig, BackendConfig, GcsConfig, S3Config};
use crate::signer::{CompletePayload, HeadInfo, MultipartInit, UploadPlanReplayInput};
use crate::tenant_path::ValidatedPath;
use std::time::Duration;
use wyrd_spec::storage::{StorageBackendKind, UploadPlan};

/// Active signer for one configured cloud backend.
#[derive(Debug, Clone)]
pub enum CloudSigner {
    /// AWS S3 signer.
    S3(S3Signer),
    /// Google Cloud Storage signer.
    Gcs(GcsSigner),
    /// Azure Blob Storage signer.
    Azure(AzureSigner),
}

impl CloudSigner {
    /// Return the backend kind.
    pub(crate) fn kind(&self) -> StorageBackendKind {
        match self {
            Self::S3(_) => StorageBackendKind::S3,
            Self::Gcs(_) => StorageBackendKind::Gcs,
            Self::Azure(_) => StorageBackendKind::Azure,
        }
    }

    /// Synthesize a [`BackendConfig`] from the signer alone (test/local-harness
    /// reconstruction). Optional fields are defaulted; production paths use
    /// `StorageHandle::from_settings`.
    pub(crate) fn backend_config(&self) -> BackendConfig {
        match self {
            Self::S3(s3) => BackendConfig::S3(S3Config {
                bucket: s3.bucket().to_owned(),
                region: None,
                endpoint_url: None,
                force_path_style: false,
            }),
            Self::Gcs(gcs) => BackendConfig::Gcs(GcsConfig {
                bucket: gcs.bucket().to_owned(),
                endpoint_url: None,
            }),
            Self::Azure(azure) => BackendConfig::Azure(AzureConfig {
                account: azure.account().to_owned(),
                container: azure.container().to_owned(),
                endpoint_url: None,
            }),
        }
    }

    pub(crate) async fn presign_single_put(
        &self,
        path: &ValidatedPath,
        size_bytes: u64,
        ttl: Duration,
    ) -> Result<UploadPlan, StorageError> {
        match self {
            Self::S3(signer) => signer.presign_single_put(path, size_bytes, ttl).await,
            Self::Gcs(signer) => signer.presign_single_put(path, size_bytes, ttl).await,
            Self::Azure(signer) => signer.presign_single_put(path, size_bytes, ttl).await,
        }
    }

    pub(crate) async fn init_multipart(
        &self,
        path: &ValidatedPath,
        part_count: u32,
        part_size_bytes: u64,
        ttl: Duration,
    ) -> Result<MultipartInit, StorageError> {
        match self {
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

    pub(crate) async fn presign_part(
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
            Self::Gcs(_) | Self::Azure(_) => Err(StorageError::BackendCapabilityMismatch {
                signer: self.kind(),
                op: "presign_part",
            }),
        }
    }

    pub(crate) async fn complete_server_side(
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

    pub(crate) async fn abort_multipart(
        &self,
        path: &ValidatedPath,
        backend_upload_id: &str,
    ) -> Result<(), StorageError> {
        match self {
            Self::S3(signer) => signer.abort_multipart(path, backend_upload_id).await,
            Self::Azure(signer) => signer.abort_multipart(path).await,
            Self::Gcs(_) => Err(StorageError::BackendCapabilityMismatch {
                signer: StorageBackendKind::Gcs,
                op: "abort_multipart",
            }),
        }
    }

    pub(crate) async fn presign_get(
        &self,
        path: &ValidatedPath,
        ttl: Duration,
    ) -> Result<String, StorageError> {
        match self {
            Self::S3(signer) => signer.presign_get(path, ttl).await,
            Self::Gcs(signer) => signer.presign_get(path, ttl).await,
            Self::Azure(signer) => signer.presign_get(path, ttl).await,
        }
    }

    pub(crate) async fn head_for_verification(
        &self,
        path: &ValidatedPath,
    ) -> Result<HeadInfo, StorageError> {
        match self {
            Self::S3(signer) => signer.head(path).await,
            Self::Gcs(signer) => signer.head(path).await,
            Self::Azure(signer) => signer.head(path).await,
        }
    }

    pub(crate) async fn remint_plan(
        &self,
        path: &ValidatedPath,
        input: &UploadPlanReplayInput,
        ttl: Duration,
    ) -> Result<UploadPlan, StorageError> {
        match self {
            Self::S3(signer) => signer.remint_plan(path, input, ttl).await,
            Self::Gcs(signer) => signer.remint_plan(path, input, ttl).await,
            Self::Azure(signer) => signer.remint_plan(path, input, ttl).await,
        }
    }

    /// Run non-fatal boot preflight checks for the active cloud backend.
    pub(crate) async fn preflight(&self) {
        if let Self::S3(s3) = self {
            check_s3_lifecycle(s3).await;
        }
    }
}

/// Warn if an S3 bucket lacks an incomplete multipart upload lifecycle rule.
async fn check_s3_lifecycle(s3: &S3Signer) {
    let response = s3
        .client()
        .get_bucket_lifecycle_configuration()
        .bucket(s3.bucket())
        .send()
        .await;

    match response {
        Ok(output) => {
            let has_abort_rule = output.rules().iter().any(|rule| {
                rule.abort_incomplete_multipart_upload()
                    .and_then(
                        aws_sdk_s3::types::AbortIncompleteMultipartUpload::days_after_initiation,
                    )
                    .is_some()
            });
            if !has_abort_rule {
                tracing::warn!(
                    bucket = %s3.bucket(),
                    rule_id = "wyrd-abort-incomplete-multipart",
                    lifecycle_rule_missing = true,
                    remediation = "Add an AbortIncompleteMultipartUpload lifecycle rule with DaysAfterInitiation: 7 or shorter.",
                    "S3 bucket lifecycle missing AbortIncompleteMultipartUpload rule"
                );
            }
        }
        Err(source) => {
            tracing::warn!(
                bucket = %s3.bucket(),
                error = ?source,
                lifecycle_rule_missing = true,
                "S3 lifecycle preflight could not fetch bucket lifecycle configuration"
            );
        }
    }
}
