//! AWS S3 backend signer shell.

use crate::error::{S3Error, StorageError};
use crate::signer::{HeadInfo, MultipartInit, UploadPlanReplayInput, ttl_secs};
use crate::tenant_path::ValidatedPath;
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
use std::time::Duration;
use wyrd_spec::storage::{
    HeaderPair, S3CompletedPart, StorageBackendKind, UploadPlan, WireProtocol,
};

/// AWS S3 signer.
#[derive(Clone)]
pub struct S3Signer {
    client: aws_sdk_s3::Client,
    bucket: String,
}

impl std::fmt::Debug for S3Signer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3Signer")
            .field("bucket", &self.bucket)
            .field("client", &"<aws_sdk_s3::Client>")
            .finish()
    }
}

impl S3Signer {
    /// Build an S3 signer from an already-built SDK client.
    #[must_use]
    pub fn new(client: aws_sdk_s3::Client, bucket: String) -> Self {
        Self { client, bucket }
    }

    /// Borrow the bucket name.
    #[must_use]
    pub fn bucket(&self) -> &str {
        &self.bucket
    }

    /// Borrow the SDK client.
    #[must_use]
    pub fn client(&self) -> &aws_sdk_s3::Client {
        &self.client
    }

    /// Presign a single PUT URL.
    ///
    /// # Errors
    /// Returns a typed backend error when presigning fails.
    pub async fn presign_single_put(
        &self,
        path: &ValidatedPath,
        _size_bytes: u64,
        ttl: Duration,
    ) -> Result<UploadPlan, StorageError> {
        let presigned = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(path.full.as_str())
            .presigned(presign_config(ttl)?)
            .await
            .map_err(|err| s3_presign("presign_single_put", err))?;

        Ok(UploadPlan::SinglePut {
            put_url: presigned.uri().to_owned(),
            ttl_secs: ttl_secs(ttl),
            required_headers: headers_from_presigned(presigned.headers()),
        })
    }

    /// Initiate S3 multipart upload.
    ///
    /// Requests full-object SHA-256 so S3 computes and stores the checksum
    /// server-side rather than requiring per-part client hashing.
    ///
    /// # Errors
    /// Returns a typed backend error when the SDK call fails or the response
    /// omits the backend upload id.
    pub async fn init_multipart(
        &self,
        path: &ValidatedPath,
        part_count: u32,
        part_size_bytes: u64,
        ttl: Duration,
    ) -> Result<MultipartInit, StorageError> {
        let output = self
            .client
            .create_multipart_upload()
            .bucket(&self.bucket)
            .key(path.full.as_str())
            .send()
            .await
            .map_err(|err| s3_sdk("init_multipart", err))?;
        let backend_upload_id = output
            .upload_id()
            .ok_or_else(|| StorageError::S3(Box::new(S3Error::MissingUploadId)))?
            .to_owned();

        Ok(MultipartInit {
            plan: UploadPlan::S3Multipart {
                part_count,
                part_size_bytes,
                part_url_ttl_secs: ttl_secs(ttl),
                required_headers: Vec::new(),
            },
            backend_upload_id,
        })
    }

    /// Presign one S3 multipart part.
    ///
    /// # Errors
    /// Returns a typed backend error when presigning fails or the part number
    /// does not fit the S3 SDK request shape.
    pub async fn presign_part(
        &self,
        path: &ValidatedPath,
        backend_upload_id: &str,
        part_number: u32,
        ttl: Duration,
    ) -> Result<String, StorageError> {
        let part_number = i32::try_from(part_number).map_err(|_| StorageError::Backend {
            backend: StorageBackendKind::S3,
            op: "presign_part",
            message: "part number exceeds S3 signed integer range".to_owned(),
        })?;
        let presigned = self
            .client
            .upload_part()
            .bucket(&self.bucket)
            .key(path.full.as_str())
            .upload_id(backend_upload_id)
            .part_number(part_number)
            .presigned(presign_config(ttl)?)
            .await
            .map_err(|err| s3_presign("presign_part", err))?;
        Ok(presigned.uri().to_owned())
    }

    /// Complete an S3 multipart upload.
    ///
    /// The storage layer does not enforce checksum policy. `expected_sha256` is
    /// retained on the call signature so the rest of the system continues to
    /// thread it through (request → DB), but it is not pushed into the S3 API:
    /// the upload was initiated without a server-side checksum algorithm, so the
    /// matching `x-amz-checksum-sha256` on complete would be rejected. Any
    /// integrity verification a caller wants belongs above the storage layer.
    ///
    /// # Errors
    /// Returns a typed backend error when completion fails or a part number is
    /// outside the S3 SDK request range.
    pub async fn complete_multipart(
        &self,
        path: &ValidatedPath,
        backend_upload_id: &str,
        parts: &[S3CompletedPart],
        _expected_sha256: &str,
    ) -> Result<(), StorageError> {
        let completed_parts = parts
            .iter()
            .map(|part| {
                let part_number =
                    i32::try_from(part.part_number).map_err(|_| StorageError::Backend {
                        backend: StorageBackendKind::S3,
                        op: "complete_multipart",
                        message: "part number exceeds S3 signed integer range".to_owned(),
                    })?;
                Ok(CompletedPart::builder()
                    .part_number(part_number)
                    .e_tag(part.e_tag.clone())
                    .build())
            })
            .collect::<Result<Vec<_>, StorageError>>()?;
        let completed = CompletedMultipartUpload::builder()
            .set_parts(Some(completed_parts))
            .build();

        self.client
            .complete_multipart_upload()
            .bucket(&self.bucket)
            .key(path.full.as_str())
            .upload_id(backend_upload_id)
            .multipart_upload(completed)
            .send()
            .await
            .map_err(|err| s3_sdk("complete_multipart", err))?;
        Ok(())
    }

    /// Abort an S3 multipart upload.
    ///
    /// # Errors
    /// Returns a typed backend error when the abort request fails.
    pub async fn abort_multipart(
        &self,
        path: &ValidatedPath,
        backend_upload_id: &str,
    ) -> Result<(), StorageError> {
        self.client
            .abort_multipart_upload()
            .bucket(&self.bucket)
            .key(path.full.as_str())
            .upload_id(backend_upload_id)
            .send()
            .await
            .map_err(|err| s3_sdk("abort_multipart", err))?;
        Ok(())
    }

    /// Presign a GET URL.
    ///
    /// # Errors
    /// Returns a typed backend error when presigning fails.
    pub async fn presign_get(
        &self,
        path: &ValidatedPath,
        ttl: Duration,
    ) -> Result<String, StorageError> {
        let presigned = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(path.full.as_str())
            .presigned(presign_config(ttl)?)
            .await
            .map_err(|err| s3_presign("presign_get", err))?;
        Ok(presigned.uri().to_owned())
    }

    /// Read S3 object metadata.
    ///
    /// # Errors
    /// Returns a typed backend error when the SDK call fails or the object size
    /// is invalid.
    pub async fn head(&self, path: &ValidatedPath) -> Result<HeadInfo, StorageError> {
        let output = self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(path.full.as_str())
            .send()
            .await
            .map_err(|err| s3_sdk("head", err))?;
        let size_bytes = match output.content_length() {
            Some(size) => u64::try_from(size).map_err(|_| StorageError::Backend {
                backend: StorageBackendKind::S3,
                op: "head",
                message: "S3 returned a negative content length".to_owned(),
            })?,
            None => 0,
        };
        Ok(HeadInfo {
            size_bytes,
            content_type: output.content_type().map(ToOwned::to_owned),
            sse_marker: output
                .server_side_encryption()
                .map(|value| value.as_str().to_owned()),
        })
    }

    /// Re-mint an S3 upload plan from non-bearer state.
    ///
    /// # Errors
    /// Returns a backend error when presigning fails or a capability mismatch
    /// for non-S3 protocols.
    pub async fn remint_plan(
        &self,
        path: &ValidatedPath,
        input: &UploadPlanReplayInput,
        ttl: Duration,
    ) -> Result<UploadPlan, StorageError> {
        match input.wire_protocol {
            WireProtocol::S3MultipartV1 => Ok(UploadPlan::S3Multipart {
                part_count: input.part_count_planned,
                part_size_bytes: input.part_size_bytes,
                part_url_ttl_secs: ttl_secs(ttl),
                required_headers: Vec::<HeaderPair>::new(),
            }),
            WireProtocol::SinglePutV1 => self.presign_single_put(path, 0, ttl).await,
            _ => Err(StorageError::BackendCapabilityMismatch {
                signer: StorageBackendKind::S3,
                op: "remint_plan",
            }),
        }
    }
}

fn presign_config(ttl: Duration) -> Result<PresigningConfig, StorageError> {
    PresigningConfig::expires_in(ttl)
        .map_err(|err| StorageError::S3(Box::new(S3Error::Presign(format!("invalid ttl: {err}")))))
}

fn headers_from_presigned<'a>(
    headers: impl Iterator<Item = (&'a str, &'a str)>,
) -> Vec<HeaderPair> {
    headers
        .map(|(name, value)| HeaderPair {
            name: name.to_owned(),
            value: value.to_owned(),
        })
        .collect()
}

fn s3_presign(op: &'static str, err: impl std::fmt::Display) -> StorageError {
    StorageError::S3(Box::new(S3Error::Presign(format!("{op}: {err}"))))
}

fn s3_sdk(op: &'static str, err: impl std::fmt::Display) -> StorageError {
    StorageError::S3(Box::new(S3Error::Sdk(format!("{op}: {err}"))))
}
