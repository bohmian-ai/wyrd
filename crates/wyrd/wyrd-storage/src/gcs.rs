//! Google Cloud Storage backend signer.

use crate::error::{GcsError, StorageError};
use crate::signer::{HeadInfo, MultipartInit, UploadPlanReplayInput, ttl_secs};
use crate::tenant_path::ValidatedPath;
use gcloud_storage::client::Client;
use gcloud_storage::http::objects::Object;
use gcloud_storage::http::objects::get::GetObjectRequest;
use gcloud_storage::http::objects::upload::{UploadObjectRequest, UploadType};
use gcloud_storage::sign::{SignedURLMethod, SignedURLOptions};
use std::time::Duration;
use wyrd_spec::storage::{StorageBackendKind, UploadPlan, WireProtocol};

/// Google Cloud Storage signer.
#[derive(Clone)]
pub struct GcsSigner {
    client: Client,
    bucket: String,
    emulator_endpoint: Option<String>,
}

impl std::fmt::Debug for GcsSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GcsSigner")
            .field("bucket", &self.bucket)
            .field("emulator_endpoint", &self.emulator_endpoint)
            .field("client", &"<gcloud_storage::client::Client>")
            .finish()
    }
}

impl GcsSigner {
    /// Build a GCS signer from an already-built SDK client.
    #[must_use]
    pub fn new(client: Client, bucket: String) -> Self {
        Self {
            client,
            bucket,
            emulator_endpoint: None,
        }
    }

    /// Build a signer for the fake-GCS emulator's unsigned media endpoint.
    #[cfg(any(test, feature = "emulator"))]
    #[must_use]
    pub fn new_emulator(client: Client, bucket: String, endpoint: &str) -> Self {
        Self {
            client,
            bucket,
            emulator_endpoint: Some(endpoint.trim_end_matches('/').to_owned()),
        }
    }

    /// Borrow the bucket name.
    #[must_use]
    pub fn bucket(&self) -> &str {
        &self.bucket
    }

    /// Borrow the SDK client.
    #[must_use]
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Presign a single PUT URL.
    ///
    /// # Errors
    /// Returns a typed backend error when the configured GCS signer cannot
    /// produce a signed URL.
    pub async fn presign_single_put(
        &self,
        path: &ValidatedPath,
        _size_bytes: u64,
        ttl: Duration,
    ) -> Result<UploadPlan, StorageError> {
        let put_url = self
            .client
            .signed_url(
                &self.bucket,
                path.full.as_str(),
                None,
                None,
                SignedURLOptions {
                    method: SignedURLMethod::PUT,
                    expires: ttl,
                    ..Default::default()
                },
            )
            .await
            .map_err(|err| gcs_sdk("presign_single_put", err))?;

        Ok(UploadPlan::SinglePut {
            put_url,
            ttl_secs: ttl_secs(ttl),
            required_headers: Vec::new(),
        })
    }

    /// Initiate a GCS resumable upload.
    ///
    /// # Errors
    /// Returns a typed backend error when GCS does not create a session URI.
    pub async fn init_multipart(
        &self,
        path: &ValidatedPath,
        _part_count: u32,
        part_size_bytes: u64,
        _ttl: Duration,
    ) -> Result<MultipartInit, StorageError> {
        let upload = self
            .client
            .prepare_resumable_upload(
                &UploadObjectRequest {
                    bucket: self.bucket.clone(),
                    ..Default::default()
                },
                &UploadType::Multipart(Box::new(Object {
                    name: path.full.clone(),
                    ..Default::default()
                })),
            )
            .await
            .map_err(|err| gcs_http("init_multipart", &err))?;
        let session_uri = upload.url().to_owned();
        if session_uri.is_empty() {
            return Err(StorageError::Gcs(Box::new(GcsError::MissingSessionUrl)));
        }

        Ok(MultipartInit {
            plan: UploadPlan::GcsResumable {
                session_uri,
                chunk_size_bytes: part_size_bytes,
            },
            backend_upload_id: String::new(),
        })
    }

    /// Presign a GET URL.
    ///
    /// # Errors
    /// Returns a typed backend error when the configured GCS signer cannot
    /// produce a signed URL.
    pub async fn presign_get(
        &self,
        path: &ValidatedPath,
        ttl: Duration,
    ) -> Result<String, StorageError> {
        if let Some(endpoint) = &self.emulator_endpoint {
            return Ok(format!(
                "{endpoint}/storage/v1/b/{}/o/{}?alt=media",
                self.bucket,
                path.full.replace('/', "%2F")
            ));
        }
        self.client
            .signed_url(
                &self.bucket,
                path.full.as_str(),
                None,
                None,
                SignedURLOptions {
                    method: SignedURLMethod::GET,
                    expires: ttl,
                    ..Default::default()
                },
            )
            .await
            .map_err(|err| gcs_sdk("presign_get", err))
    }

    /// Read object metadata.
    ///
    /// # Errors
    /// Returns a typed backend error when metadata cannot be read.
    pub async fn head(&self, path: &ValidatedPath) -> Result<HeadInfo, StorageError> {
        let object = self
            .client
            .get_object(&self.get_request(path))
            .await
            .map_err(|err| gcs_http_path("head", path, err))?;
        let size_bytes = u64::try_from(object.size).map_err(|_| StorageError::Backend {
            backend: StorageBackendKind::Gcs,
            op: "head",
            message: "GCS returned a negative object size".to_owned(),
        })?;
        Ok(HeadInfo {
            size_bytes,
            sse_marker: object.kms_key_name,
            content_type: object.content_type,
        })
    }

    /// Re-mint a GCS upload plan.
    ///
    /// # Errors
    /// Returns a typed backend error for the GCS protocol and capability
    /// mismatch for other protocols.
    pub async fn remint_plan(
        &self,
        path: &ValidatedPath,
        input: &UploadPlanReplayInput,
        ttl: Duration,
    ) -> Result<UploadPlan, StorageError> {
        if input.wire_protocol == WireProtocol::GcsResumableV1 {
            Ok(self
                .init_multipart(path, input.part_count_planned, input.part_size_bytes, ttl)
                .await?
                .plan)
        } else {
            Err(StorageError::BackendCapabilityMismatch {
                signer: StorageBackendKind::Gcs,
                op: "remint_plan",
            })
        }
    }

    fn get_request(&self, path: &ValidatedPath) -> GetObjectRequest {
        GetObjectRequest {
            bucket: self.bucket.clone(),
            object: path.full.clone(),
            ..Default::default()
        }
    }
}

fn gcs_http_path(
    op: &'static str,
    path: &ValidatedPath,
    err: gcloud_storage::http::Error,
) -> StorageError {
    match err {
        gcloud_storage::http::Error::Response(response) if response.code == 404 => {
            StorageError::Gcs(Box::new(GcsError::NotFound {
                storage_path: path.full.clone(),
            }))
        }
        gcloud_storage::http::Error::Response(response) if response.code == 429 => {
            StorageError::Gcs(Box::new(GcsError::Throttled))
        }
        other => gcs_http(op, &other),
    }
}

fn gcs_http(op: &'static str, err: &gcloud_storage::http::Error) -> StorageError {
    StorageError::Gcs(Box::new(GcsError::Sdk(format!("{op}: {err}"))))
}

fn gcs_sdk(op: &'static str, err: impl std::fmt::Display) -> StorageError {
    StorageError::Gcs(Box::new(GcsError::Sdk(format!("{op}: {err}"))))
}
