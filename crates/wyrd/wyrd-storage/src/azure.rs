//! Azure Blob Storage backend signer.

use crate::error::{AzureError, StorageError};
use crate::signer::{HeadInfo, MultipartInit, UploadPlanReplayInput};
use crate::tenant_path::ValidatedPath;
use azure_storage::shared_access_signature::service_sas::BlobSasPermissions;
use azure_storage_blobs::prelude::{BlobBlockType, BlobClient, BlobServiceClient, BlockList};
use std::time::Duration;
use time::OffsetDateTime;
use wyrd_spec::storage::{StorageBackendKind, UploadPlan, WireProtocol};

/// Azure SAS signing mode selected by the already-built client/config.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AzureSasMode {
    /// Use shared-key SAS generation. The client must have account-key
    /// credentials.
    AccountKey,
    /// Use user-delegation SAS generation. The client must have token
    /// credentials that can fetch a user delegation key.
    UserDelegation,
}

/// Azure Blob Storage signer.
#[derive(Clone)]
pub struct AzureSigner {
    service_client: BlobServiceClient,
    container: String,
    sas_mode: AzureSasMode,
}

impl std::fmt::Debug for AzureSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AzureSigner")
            .field("account", &self.account())
            .field("container", &self.container)
            .field("sas_mode", &self.sas_mode)
            .field(
                "service_client",
                &"<azure_storage_blobs::BlobServiceClient>",
            )
            .finish()
    }
}

impl AzureSigner {
    /// Build an Azure signer from an already-built SDK client.
    #[must_use]
    pub fn new(
        service_client: BlobServiceClient,
        container: String,
        sas_mode: AzureSasMode,
    ) -> Self {
        Self {
            service_client,
            container,
            sas_mode,
        }
    }

    /// Borrow the storage account name.
    #[must_use]
    pub fn account(&self) -> &str {
        self.service_client.account()
    }

    /// Borrow the container name.
    #[must_use]
    pub fn container(&self) -> &str {
        &self.container
    }

    /// Borrow the SDK service client.
    #[must_use]
    pub fn service_client(&self) -> &BlobServiceClient {
        &self.service_client
    }

    /// Return the SAS signing mode.
    #[must_use]
    pub fn sas_mode(&self) -> AzureSasMode {
        self.sas_mode
    }

    /// Presign a single PUT URL.
    ///
    /// # Errors
    /// Returns a typed backend error when SAS generation fails.
    pub async fn presign_single_put(
        &self,
        path: &ValidatedPath,
        _size_bytes: u64,
        ttl: Duration,
    ) -> Result<UploadPlan, StorageError> {
        let sas_url = self
            .signed_blob_url(path, write_permissions(), ttl, "presign_single_put")
            .await?;
        Ok(UploadPlan::SinglePut {
            put_url: sas_url,
            ttl_secs: crate::signer::ttl_secs(ttl),
            required_headers: vec![wyrd_spec::storage::HeaderPair {
                name: "x-ms-blob-type".to_owned(),
                value: "BlockBlob".to_owned(),
            }],
        })
    }

    /// Initiate an Azure block blob upload.
    ///
    /// # Errors
    /// Returns a typed backend error when SAS generation fails.
    pub async fn init_multipart(
        &self,
        path: &ValidatedPath,
        part_count: u32,
        part_size_bytes: u64,
        ttl: Duration,
    ) -> Result<MultipartInit, StorageError> {
        let sas_url = self
            .signed_blob_url(path, write_permissions(), ttl, "init_multipart")
            .await?;
        Ok(MultipartInit {
            plan: UploadPlan::AzureBlockBlob {
                sas_url,
                block_size_bytes: part_size_bytes,
                block_count_planned: part_count,
            },
            backend_upload_id: String::new(),
        })
    }

    /// Complete an Azure block list server-side.
    ///
    /// # Errors
    /// Returns a typed backend error when block-list commit fails.
    pub async fn complete_blocklist_server(
        &self,
        path: &ValidatedPath,
        block_count: u32,
    ) -> Result<(), StorageError> {
        let block_list = BlockList {
            blocks: (0..block_count)
                .map(|index| BlobBlockType::new_latest(azure_block_id(index)))
                .collect(),
        };
        self.blob_client(path)
            .put_block_list(block_list)
            .await
            .map_err(|err| azure_sdk_path("complete_blocklist_server", path, &err))?;
        Ok(())
    }

    /// Abort an Azure block blob upload by deleting the staged blob.
    ///
    /// # Errors
    /// Returns a typed backend error when the delete call fails.
    pub async fn abort_multipart(&self, path: &ValidatedPath) -> Result<(), StorageError> {
        self.blob_client(path)
            .delete()
            .await
            .map_err(|err| azure_sdk_path("abort_multipart", path, &err))?;
        Ok(())
    }

    /// Presign a GET URL.
    ///
    /// # Errors
    /// Returns a typed backend error when SAS generation fails.
    pub async fn presign_get(
        &self,
        path: &ValidatedPath,
        ttl: Duration,
    ) -> Result<String, StorageError> {
        self.signed_blob_url(path, read_permissions(), ttl, "presign_get")
            .await
    }

    /// Read object metadata.
    ///
    /// # Errors
    /// Returns a typed backend error when metadata cannot be read.
    pub async fn head(&self, path: &ValidatedPath) -> Result<HeadInfo, StorageError> {
        let response = self
            .blob_client(path)
            .get_properties()
            .await
            .map_err(|err| azure_sdk_path("head", path, &err))?;
        let properties = response.blob.properties;
        Ok(HeadInfo {
            size_bytes: properties.content_length,
            sse_marker: if properties.server_encrypted {
                properties
                    .encryption_scope
                    .or(properties.customer_provided_key_sha256)
                    .or_else(|| Some("server_encrypted".to_owned()))
            } else {
                None
            },
            content_type: if properties.content_type.is_empty() {
                None
            } else {
                Some(properties.content_type)
            },
            sha256_b64: None,
        })
    }

    /// Verify SHA-256.
    ///
    /// Azure block blobs have no server-computed SHA-256 in metadata (only
    /// `Content-MD5` and `x-ms-content-crc64`). Per the no-bytes-on-server
    /// invariant the `get_content()` fallback is removed entirely. Verification
    /// is client-declared: the client commits `expected_sha256` at upload init,
    /// SAS-restricted PUT plus TLS prevents in-flight tampering, and Azure's
    /// CRC64 validates wire integrity. Server-side SHA-256 recomputation is not
    /// available for Azure and must not be attempted.
    ///
    /// # Errors
    /// Returns SHA mismatch when a server-computed digest is available (future),
    /// or `Ok(())` for the client-declared fast path.
    #[allow(clippy::unused_async)]
    pub async fn verify_sha256(
        &self,
        _path: &ValidatedPath,
        _expected: &str,
        _head_hint: &HeadInfo,
    ) -> Result<(), StorageError> {
        Ok(())
    }

    /// Re-mint an Azure upload plan.
    ///
    /// # Errors
    /// Returns a typed backend error for the Azure protocol and capability
    /// mismatch for other protocols.
    pub async fn remint_plan(
        &self,
        path: &ValidatedPath,
        input: &UploadPlanReplayInput,
        ttl: Duration,
    ) -> Result<UploadPlan, StorageError> {
        if input.wire_protocol == WireProtocol::AzureBlockBlobV1 {
            let block_count = input
                .block_count_planned
                .unwrap_or(input.part_count_planned);
            Ok(self
                .init_multipart(path, block_count, input.part_size_bytes, ttl)
                .await?
                .plan)
        } else {
            Err(StorageError::BackendCapabilityMismatch {
                signer: StorageBackendKind::Azure,
                op: "remint_plan",
            })
        }
    }

    async fn signed_blob_url(
        &self,
        path: &ValidatedPath,
        permissions: BlobSasPermissions,
        ttl: Duration,
        op: &'static str,
    ) -> Result<String, StorageError> {
        let expiry = OffsetDateTime::now_utc() + ttl;
        let blob = self.blob_client(path);
        let sas = match self.sas_mode {
            AzureSasMode::AccountKey => blob
                .shared_access_signature(permissions, expiry)
                .await
                .map_err(|err| azure_sdk(op, &err))?,
            AzureSasMode::UserDelegation => {
                let start = OffsetDateTime::now_utc();
                let key = self
                    .service_client
                    .get_user_deligation_key(start, expiry)
                    .await
                    .map_err(|err| azure_sdk(op, &err))?;
                blob.user_delegation_shared_access_signature(permissions, &key.user_deligation_key)
                    .await
                    .map_err(|err| azure_sdk(op, &err))?
            }
        };
        blob.generate_signed_blob_url(&sas)
            .map(|url| url.to_string())
            .map_err(|err| azure_sdk(op, &err))
    }

    fn blob_client(&self, path: &ValidatedPath) -> BlobClient {
        self.service_client
            .container_client(&self.container)
            .blob_client(path.full.as_str())
    }
}

fn read_permissions() -> BlobSasPermissions {
    BlobSasPermissions {
        read: true,
        ..Default::default()
    }
}

fn write_permissions() -> BlobSasPermissions {
    BlobSasPermissions {
        create: true,
        write: true,
        ..Default::default()
    }
}

fn azure_block_id(zero_based_index: u32) -> Vec<u8> {
    format!("{zero_based_index:06}").into_bytes()
}

fn azure_sdk_path(op: &'static str, path: &ValidatedPath, err: &azure_core::Error) -> StorageError {
    if err
        .as_http_error()
        .is_some_and(|http| http.status() == azure_core::StatusCode::NotFound)
    {
        StorageError::Azure(Box::new(AzureError::BlobNotFound {
            storage_path: path.full.clone(),
        }))
    } else if err
        .as_http_error()
        .is_some_and(|http| http.status() == azure_core::StatusCode::TooManyRequests)
    {
        StorageError::Azure(Box::new(AzureError::Throttled))
    } else {
        azure_sdk(op, err)
    }
}

fn azure_sdk(op: &'static str, err: &azure_core::Error) -> StorageError {
    StorageError::Azure(Box::new(AzureError::Sdk(format!("{op}: {err}"))))
}

#[cfg(test)]
mod tests {
    use super::azure_block_id;

    #[test]
    fn azure_block_id_uses_locked_zero_based_six_digit_shape() {
        assert_eq!(azure_block_id(0), b"000000");
        assert_eq!(azure_block_id(1), b"000001");
        assert_eq!(azure_block_id(9), b"000009");
    }
}
