//! Cloud storage over `object_store` (S3, GCS, Azure).
//!
//! Compiled only under the `cloud` feature. [`CloudStore`] is the one store
//! construction every Wyrd storage consumer uses: the [`crate::StorageHandle`]
//! data plane, this signer, and Bifrost's analytical store. [`CloudSigner`]
//! derives every presigned URL and multipart control call from that store, so
//! one client owns credentials, addressing, and signing for each backend.

use std::sync::Arc;
use std::time::Duration;

use object_store::aws::{AmazonS3, AmazonS3Builder};
use object_store::azure::{MicrosoftAzure, MicrosoftAzureBuilder};
use object_store::gcp::{GoogleCloudStorage, GoogleCloudStorageBuilder};
use object_store::multipart::{MultipartStore, PartId};
use object_store::path::Path;
use object_store::signer::{SignedUrlOptions, Signer};
use object_store::{ClientOptions, ObjectStore, ObjectStoreExt as _, RetryConfig};
use reqwest::{Method, StatusCode, header};
use wyrd_spec::storage::{CompletedPart, HeaderPair, StorageBackendKind, UploadPlan, WireProtocol};

use crate::error::{AzureError, GcsError, S3Error, StorageError};
use crate::settings::{AzureConfig, GcsConfig, S3Config};
use crate::signer::{CompletePayload, HeadInfo, MultipartInit, UploadPlanReplayInput, ttl_secs};
use crate::tenant_path::ValidatedPath;

/// One configured cloud object store, kept concrete so signing and multipart
/// control stay available beside the [`ObjectStore`] data plane.
#[derive(Debug, Clone)]
pub enum CloudStore {
    /// AWS S3 or an S3-compatible service.
    S3(Arc<AmazonS3>),
    /// Google Cloud Storage over its XML API.
    Gcs(Arc<GoogleCloudStorage>),
    /// Azure Blob Storage.
    Azure(Arc<MicrosoftAzure>),
}

impl CloudStore {
    /// Build the S3 store from the environment's credential chain.
    ///
    /// `client` and `retry` are the consumer's transport policy. A configured
    /// endpoint addresses an S3-compatible service with path-style requests;
    /// AWS uses virtual-hosted requests.
    ///
    /// # Errors
    /// Returns [`StorageError::Backend`] when `object_store` rejects the
    /// configuration.
    pub fn s3(
        config: &S3Config,
        client: ClientOptions,
        retry: RetryConfig,
    ) -> Result<Self, StorageError> {
        let mut builder = AmazonS3Builder::from_env()
            .with_bucket_name(&config.bucket)
            .with_client_options(client)
            .with_retry(retry);
        if let Some(region) = &config.region {
            builder = builder.with_region(region);
        }
        builder = match &config.endpoint_url {
            Some(endpoint) => builder.with_endpoint(endpoint),
            None => builder.with_virtual_hosted_style_request(true),
        };
        builder
            .build()
            .map(|store| Self::S3(Arc::new(store)))
            .map_err(|error| build_failed(StorageBackendKind::S3, &error))
    }

    /// Build the GCS store from Wyrd's GCS credential variables, else the
    /// default credential chain.
    ///
    /// A configured endpoint names a GCS emulator, accepted only in builds
    /// with the `emulator` feature. Without a service-account key the
    /// emulator receives unsigned requests; presigning always needs a key.
    ///
    /// # Errors
    /// Returns [`StorageError::Backend`] when an endpoint is set without the
    /// `emulator` feature or `object_store` rejects the configuration.
    pub fn gcs(
        config: &GcsConfig,
        client: ClientOptions,
        retry: RetryConfig,
    ) -> Result<Self, StorageError> {
        let mut builder = GoogleCloudStorageBuilder::from_env()
            .with_bucket_name(&config.bucket)
            .with_client_options(client)
            .with_retry(retry);
        let key = crate::factory::service_account_json();
        if let Some(key) = &key {
            builder = builder.with_service_account_key(key);
        }
        if let Some(endpoint) = &config.endpoint_url {
            emulator_endpoint(StorageBackendKind::Gcs)?;
            builder = builder
                .with_base_url(endpoint)
                .with_skip_signature(key.is_none());
        }
        builder
            .build()
            .map(|store| Self::Gcs(Arc::new(store)))
            .map_err(|error| build_failed(StorageBackendKind::Gcs, &error))
    }

    /// Build the Azure store; tokens sign with user-delegation keys and
    /// account keys sign directly.
    ///
    /// A configured endpoint names an Azure emulator, accepted only in builds
    /// with the `emulator` feature.
    ///
    /// # Errors
    /// Returns [`StorageError::Backend`] when an endpoint is set without the
    /// `emulator` feature or `object_store` rejects the configuration.
    pub fn azure(
        config: &AzureConfig,
        client: ClientOptions,
        retry: RetryConfig,
    ) -> Result<Self, StorageError> {
        let mut builder = MicrosoftAzureBuilder::from_env()
            .with_account(&config.account)
            .with_container_name(&config.container)
            .with_client_options(client)
            .with_retry(retry);
        if let Some(endpoint) = &config.endpoint_url {
            emulator_endpoint(StorageBackendKind::Azure)?;
            builder = builder.with_endpoint(endpoint.clone());
        }
        builder
            .build()
            .map(|store| Self::Azure(Arc::new(store)))
            .map_err(|error| build_failed(StorageBackendKind::Azure, &error))
    }

    /// Return the backend kind.
    #[must_use]
    pub fn kind(&self) -> StorageBackendKind {
        match self {
            Self::S3(_) => StorageBackendKind::S3,
            Self::Gcs(_) => StorageBackendKind::Gcs,
            Self::Azure(_) => StorageBackendKind::Azure,
        }
    }

    /// Return the store as the shared [`ObjectStore`] data plane.
    #[must_use]
    pub fn objects(&self) -> Arc<dyn ObjectStore> {
        match self {
            Self::S3(store) => Arc::clone(store) as Arc<dyn ObjectStore>,
            Self::Gcs(store) => Arc::clone(store) as Arc<dyn ObjectStore>,
            Self::Azure(store) => Arc::clone(store) as Arc<dyn ObjectStore>,
        }
    }

    /// Borrow the store as a URL signer.
    fn signer(&self) -> &dyn Signer {
        match self {
            Self::S3(store) => store.as_ref(),
            Self::Gcs(store) => store.as_ref(),
            Self::Azure(store) => store.as_ref(),
        }
    }

    /// Borrow the store's multipart control.
    fn multipart(&self) -> &dyn MultipartStore {
        match self {
            Self::S3(store) => store.as_ref(),
            Self::Gcs(store) => store.as_ref(),
            Self::Azure(store) => store.as_ref(),
        }
    }
}

/// Accept a GCS or Azure endpoint override only in an `emulator` build.
///
/// Production GCS and Azure address only the public provider, so honoring an
/// endpoint there would point one backend at an unknown service.
///
/// # Errors
/// Returns [`StorageError::Backend`] in a build without `emulator`.
fn emulator_endpoint(backend: StorageBackendKind) -> Result<(), StorageError> {
    if cfg!(feature = "emulator") {
        Ok(())
    } else {
        Err(StorageError::Backend {
            backend,
            op: "build_store",
            message: "WYRD_STORAGE_ENDPOINT_URL is supported for this backend only in builds with the `emulator` feature".to_owned(),
        })
    }
}

/// Report a store configuration `object_store` refused.
fn build_failed(backend: StorageBackendKind, error: &object_store::Error) -> StorageError {
    StorageError::Backend {
        backend,
        op: "build_store",
        message: error.to_string(),
    }
}

/// Presigning and multipart control for one cloud backend.
///
/// Every URL is signed by the backend's [`CloudStore`]; verification `HEAD`s
/// and the S3 lifecycle preflight are sent over `http` to URLs that store
/// signed, because `object_store` exposes neither the encryption headers nor
/// the bucket lifecycle.
#[derive(Debug, Clone)]
pub struct CloudSigner {
    /// The backend store that signs and controls uploads.
    store: CloudStore,
    /// Client for requests to presigned URLs.
    http: reqwest::Client,
}

impl CloudSigner {
    /// Wrap a cloud store, sending presigned requests through `http`.
    #[must_use]
    pub fn new(store: CloudStore, http: reqwest::Client) -> Self {
        Self { store, http }
    }

    /// Return the backend kind.
    pub(crate) fn kind(&self) -> StorageBackendKind {
        self.store.kind()
    }

    /// Return the signing store as the shared data plane.
    pub(crate) fn objects(&self) -> Arc<dyn ObjectStore> {
        self.store.objects()
    }

    /// Prove the credential chain can reach the bucket or container by
    /// listing its root once.
    ///
    /// # Errors
    /// Returns [`StorageError::CredentialChain`] when the listing fails.
    pub(crate) async fn probe(&self) -> Result<(), StorageError> {
        let backend = self.kind();
        self.store
            .objects()
            .list_with_delimiter(None)
            .await
            .map(|_| ())
            .map_err(|error| {
                tracing::error!(
                    %backend,
                    error = %error,
                    "storage boot probe failed; check credentials and bucket permissions"
                );
                StorageError::CredentialChain(backend_label(backend))
            })
    }

    /// Presign one single-PUT upload; Azure also requires the block-blob type
    /// header.
    ///
    /// # Errors
    /// Returns a typed backend error when signing fails.
    pub(crate) async fn presign_single_put(
        &self,
        path: &ValidatedPath,
        ttl: Duration,
    ) -> Result<UploadPlan, StorageError> {
        let put_url = self
            .sign(Method::PUT, path, ttl, "presign_single_put")
            .await?;
        let required_headers = match self.store {
            CloudStore::Azure(_) => vec![HeaderPair {
                name: "x-ms-blob-type".to_owned(),
                value: "BlockBlob".to_owned(),
            }],
            CloudStore::S3(_) | CloudStore::Gcs(_) => Vec::new(),
        };
        Ok(UploadPlan::SinglePut {
            put_url,
            ttl_secs: ttl_secs(ttl),
            required_headers,
        })
    }

    /// Start a multipart upload.
    ///
    /// S3 and GCS create a backend upload whose id the server persists and
    /// later signs parts, completes, or aborts with. Azure returns a write SAS
    /// for client-staged blocks the server commits.
    ///
    /// # Errors
    /// Returns a typed backend error when the upload cannot be created or the
    /// SAS cannot be signed.
    pub(crate) async fn init_multipart(
        &self,
        path: &ValidatedPath,
        part_count: u32,
        part_size_bytes: u64,
        ttl: Duration,
    ) -> Result<MultipartInit, StorageError> {
        if let CloudStore::Azure(_) = self.store {
            let sas_url = self.sign(Method::PUT, path, ttl, "init_multipart").await?;
            return Ok(MultipartInit {
                plan: UploadPlan::AzureBlockBlob {
                    sas_url,
                    block_size_bytes: part_size_bytes,
                    block_count_planned: part_count,
                },
                backend_upload_id: String::new(),
            });
        }
        let backend_upload_id = self
            .store
            .multipart()
            .create_multipart(&path.location())
            .await
            .map_err(|error| self.failed("init_multipart", path, &error))?;
        Ok(MultipartInit {
            plan: multipart_plan(part_count, part_size_bytes, ttl),
            backend_upload_id,
        })
    }

    /// Presign one part `PUT` of an S3 or GCS multipart upload.
    ///
    /// # Errors
    /// Returns [`StorageError::BackendCapabilityMismatch`] for Azure, or a
    /// typed backend error when signing fails.
    pub(crate) async fn presign_part(
        &self,
        path: &ValidatedPath,
        backend_upload_id: &str,
        part_number: u32,
        ttl: Duration,
    ) -> Result<String, StorageError> {
        if let CloudStore::Azure(_) = self.store {
            return Err(self.mismatch("presign_part"));
        }
        let options = SignedUrlOptions::new().with_query([
            ("partNumber", part_number.to_string()),
            ("uploadId", backend_upload_id.to_owned()),
        ]);
        self.store
            .signer()
            .signed_url_opts(Method::PUT, &path.location(), ttl, &options)
            .await
            .map(String::from)
            .map_err(|error| self.failed("presign_part", path, &error))
    }

    /// Commit an upload server-side: S3 and GCS complete with the client's
    /// part ETags in part order, and Azure commits its zero-based block ids.
    ///
    /// # Errors
    /// Returns [`StorageError::BackendCapabilityMismatch`] when the payload
    /// does not match the backend or an S3/GCS upload id is missing, or a
    /// typed backend error when completion fails.
    pub(crate) async fn complete_server_side(
        &self,
        path: &ValidatedPath,
        backend_upload_id: Option<&str>,
        complete: CompletePayload,
    ) -> Result<(), StorageError> {
        let (upload_id, parts) = match (&self.store, complete) {
            (CloudStore::S3(_) | CloudStore::Gcs(_), CompletePayload::Multipart { parts }) => (
                backend_upload_id.ok_or_else(|| self.mismatch("complete_server_side"))?,
                parts.into_iter().map(part_id).collect(),
            ),
            (CloudStore::Azure(_), CompletePayload::Azure { block_count }) => (
                "",
                (0..block_count)
                    .map(|index| PartId {
                        content_id: azure_block_id(index),
                    })
                    .collect(),
            ),
            (_, payload) => {
                tracing::error!(
                    backend = %self.kind(),
                    payload = payload.kind(),
                    "storage complete_server_side capability mismatch"
                );
                return Err(self.mismatch("complete_server_side"));
            }
        };
        self.store
            .multipart()
            .complete_multipart(&path.location(), &upload_id.to_owned(), parts)
            .await
            .map(|_| ())
            .map_err(|error| self.failed("complete_server_side", path, &error))
    }

    /// Abort an upload: S3 and GCS abort the backend upload, and Azure
    /// deletes the blob, which fails when no committed blob exists.
    ///
    /// # Errors
    /// Returns a typed backend error when the abort or delete fails.
    pub(crate) async fn abort_multipart(
        &self,
        path: &ValidatedPath,
        backend_upload_id: &str,
    ) -> Result<(), StorageError> {
        let location = path.location();
        let result = match self.store {
            CloudStore::Azure(ref store) => store.delete(&location).await,
            CloudStore::S3(_) | CloudStore::Gcs(_) => {
                self.store
                    .multipart()
                    .abort_multipart(&location, &backend_upload_id.to_owned())
                    .await
            }
        };
        result.map_err(|error| self.failed("abort_multipart", path, &error))
    }

    /// Presign one download.
    ///
    /// # Errors
    /// Returns a typed backend error when signing fails.
    pub(crate) async fn presign_get(
        &self,
        path: &ValidatedPath,
        ttl: Duration,
    ) -> Result<String, StorageError> {
        self.sign(Method::GET, path, ttl, "presign_get").await
    }

    /// Read size, content type, and encryption marker with one `HEAD` to a
    /// presigned URL.
    ///
    /// Azure SAS tokens grant read for `GET` and `HEAD` alike, so Azure signs
    /// a `GET`; S3 and GCS sign the `HEAD` itself.
    ///
    /// # Errors
    /// Returns the backend's typed not-found or throttled error, or a typed
    /// backend error for any other failure.
    pub(crate) async fn head_for_verification(
        &self,
        path: &ValidatedPath,
    ) -> Result<HeadInfo, StorageError> {
        let signed = match self.store {
            CloudStore::Azure(_) => Method::GET,
            CloudStore::S3(_) | CloudStore::Gcs(_) => Method::HEAD,
        };
        let url = self.sign(signed, path, HEAD_URL_TTL, "head").await?;
        let response = self
            .http
            .head(url)
            .send()
            .await
            .map_err(|error| self.http_failed("head", &error.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(self.status_failed("head", path, status));
        }
        let headers = response.headers();
        let text = |name: &str| {
            headers
                .get(name)
                .and_then(|value| value.to_str().ok())
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        };
        let size_bytes = text(header::CONTENT_LENGTH.as_str())
            .and_then(|value| value.parse().ok())
            .ok_or_else(|| self.http_failed("head", "response has no content length"))?;
        let sse_marker = match self.store {
            CloudStore::S3(_) => text("x-amz-server-side-encryption"),
            CloudStore::Gcs(_) => text("x-goog-encryption-kms-key-name"),
            CloudStore::Azure(_) => (text("x-ms-server-encrypted").as_deref() == Some("true"))
                .then(|| {
                    text("x-ms-encryption-scope")
                        .or_else(|| text("x-ms-encryption-key-sha256"))
                        .unwrap_or_else(|| "server_encrypted".to_owned())
                }),
        };
        Ok(HeadInfo {
            size_bytes,
            sse_marker,
            content_type: text(header::CONTENT_TYPE.as_str()),
        })
    }

    /// Re-mint the plan a replayed upload init returns.
    ///
    /// A single PUT is re-signed; an S3 or GCS multipart plan is rebuilt from
    /// the persisted geometry because its upload id stays server-side; an
    /// Azure block plan is re-signed.
    ///
    /// # Errors
    /// Returns [`StorageError::BackendCapabilityMismatch`] when the protocol
    /// does not belong to this backend, or a typed backend error when signing
    /// fails.
    pub(crate) async fn remint_plan(
        &self,
        path: &ValidatedPath,
        input: &UploadPlanReplayInput,
        ttl: Duration,
    ) -> Result<UploadPlan, StorageError> {
        match (&self.store, input.wire_protocol) {
            (_, WireProtocol::SinglePutV1) => self.presign_single_put(path, ttl).await,
            (CloudStore::S3(_) | CloudStore::Gcs(_), WireProtocol::MultipartV1) => Ok(
                multipart_plan(input.part_count_planned, input.part_size_bytes, ttl),
            ),
            (CloudStore::Azure(_), WireProtocol::AzureBlockBlobV1) => {
                let block_count = input
                    .block_count_planned
                    .unwrap_or(input.part_count_planned);
                Ok(self
                    .init_multipart(path, block_count, input.part_size_bytes, ttl)
                    .await?
                    .plan)
            }
            _ => Err(self.mismatch("remint_plan")),
        }
    }

    /// Run non-fatal boot preflight checks: warn when an S3 bucket lacks an
    /// `AbortIncompleteMultipartUpload` lifecycle rule.
    pub(crate) async fn preflight(&self) {
        let CloudStore::S3(store) = &self.store else {
            return;
        };
        let options = SignedUrlOptions::new().with_query([("lifecycle", "")]);
        let url = match store
            .signed_url_opts(Method::GET, &Path::from(""), HEAD_URL_TTL, &options)
            .await
        {
            Ok(url) => url,
            Err(error) => {
                tracing::warn!(error = %error, lifecycle_rule_missing = true, "S3 lifecycle preflight could not sign the lifecycle request");
                return;
            }
        };
        let body = match self.http.get(url).send().await {
            Ok(response) if response.status() == StatusCode::NOT_FOUND => String::new(),
            Ok(response) if response.status().is_success() => {
                response.text().await.unwrap_or_default()
            }
            Ok(response) => {
                tracing::warn!(status = %response.status(), lifecycle_rule_missing = true, "S3 lifecycle preflight could not fetch bucket lifecycle configuration");
                return;
            }
            Err(error) => {
                tracing::warn!(error = %error, lifecycle_rule_missing = true, "S3 lifecycle preflight could not fetch bucket lifecycle configuration");
                return;
            }
        };
        if !has_abort_rule(&body) {
            tracing::warn!(
                rule_id = "wyrd-abort-incomplete-multipart",
                lifecycle_rule_missing = true,
                remediation = "Add an AbortIncompleteMultipartUpload lifecycle rule with DaysAfterInitiation: 7 or shorter.",
                "S3 bucket lifecycle missing AbortIncompleteMultipartUpload rule"
            );
        }
    }

    /// Sign `method` for one object.
    ///
    /// # Errors
    /// Returns a typed backend error when signing fails.
    async fn sign(
        &self,
        method: Method,
        path: &ValidatedPath,
        ttl: Duration,
        op: &'static str,
    ) -> Result<String, StorageError> {
        self.store
            .signer()
            .signed_url(method, &path.location(), ttl)
            .await
            .map(String::from)
            .map_err(|error| self.failed(op, path, &error))
    }

    /// Map an `object_store` failure to the backend's typed error.
    fn failed(
        &self,
        op: &'static str,
        path: &ValidatedPath,
        error: &object_store::Error,
    ) -> StorageError {
        if matches!(error, object_store::Error::NotFound { .. }) {
            self.not_found(path)
        } else {
            self.sdk(format!("{op}: {error}"))
        }
    }

    /// Map an unsuccessful presigned-request status to the backend's typed
    /// error.
    fn status_failed(
        &self,
        op: &'static str,
        path: &ValidatedPath,
        status: StatusCode,
    ) -> StorageError {
        match status {
            StatusCode::NOT_FOUND => self.not_found(path),
            StatusCode::TOO_MANY_REQUESTS | StatusCode::SERVICE_UNAVAILABLE => self.throttled(),
            other => self.sdk(format!("{op}: HTTP {other}")),
        }
    }

    /// Report a presigned request that failed in transport or returned an
    /// unusable response.
    fn http_failed(&self, op: &'static str, detail: &str) -> StorageError {
        self.sdk(format!("{op}: {detail}"))
    }

    /// The backend's typed not-found error for `path`.
    fn not_found(&self, path: &ValidatedPath) -> StorageError {
        let storage_path = path.full.clone();
        match self.store {
            CloudStore::S3(_) => StorageError::S3(Box::new(S3Error::NoSuchKey { storage_path })),
            CloudStore::Gcs(_) => StorageError::Gcs(Box::new(GcsError::NotFound { storage_path })),
            CloudStore::Azure(_) => {
                StorageError::Azure(Box::new(AzureError::BlobNotFound { storage_path }))
            }
        }
    }

    /// The backend's typed throttling error.
    fn throttled(&self) -> StorageError {
        match self.store {
            CloudStore::S3(_) => StorageError::S3(Box::new(S3Error::Throttled)),
            CloudStore::Gcs(_) => StorageError::Gcs(Box::new(GcsError::Throttled)),
            CloudStore::Azure(_) => StorageError::Azure(Box::new(AzureError::Throttled)),
        }
    }

    /// The backend's typed client failure carrying `message`.
    fn sdk(&self, message: String) -> StorageError {
        match self.store {
            CloudStore::S3(_) => StorageError::S3(Box::new(S3Error::Sdk(message))),
            CloudStore::Gcs(_) => StorageError::Gcs(Box::new(GcsError::Sdk(message))),
            CloudStore::Azure(_) => StorageError::Azure(Box::new(AzureError::Sdk(message))),
        }
    }

    /// The typed refusal of an operation this backend does not perform.
    fn mismatch(&self, op: &'static str) -> StorageError {
        StorageError::BackendCapabilityMismatch {
            signer: self.kind(),
            op,
        }
    }
}

/// Lifetime of the URLs the signer itself requests immediately.
const HEAD_URL_TTL: Duration = Duration::from_mins(1);

/// The multipart plan returned for a new or replayed S3 or GCS upload.
fn multipart_plan(part_count: u32, part_size_bytes: u64, ttl: Duration) -> UploadPlan {
    UploadPlan::Multipart {
        part_count,
        part_size_bytes,
        part_url_ttl_secs: ttl_secs(ttl),
        required_headers: Vec::new(),
    }
}

/// The completion part for one client-reported ETag.
///
/// `object_store` numbers parts by position, so the client's parts arrive in
/// part order; the backend rejects any ETag that does not match its number.
fn part_id(part: CompletedPart) -> PartId {
    PartId {
        content_id: part.e_tag,
    }
}

/// The Azure block id for a zero-based block index: six zero-padded digits,
/// matching the ids clients stage.
fn azure_block_id(zero_based_index: u32) -> String {
    format!("{zero_based_index:06}")
}

/// Whether an S3 lifecycle configuration document sets an
/// `AbortIncompleteMultipartUpload` rule with a day count.
fn has_abort_rule(lifecycle_xml: &str) -> bool {
    lifecycle_xml.contains("<AbortIncompleteMultipartUpload>")
        && lifecycle_xml.contains("<DaysAfterInitiation>")
}

/// The credential-chain label of a cloud backend.
fn backend_label(backend: StorageBackendKind) -> &'static str {
    match backend {
        StorageBackendKind::S3 => "s3",
        StorageBackendKind::Gcs => "gcs",
        StorageBackendKind::Azure => "azure",
        StorageBackendKind::Local => "local",
    }
}

#[cfg(test)]
mod tests {
    use super::{azure_block_id, has_abort_rule};

    /// Azure block ids keep the locked zero-based six-digit shape clients stage.
    #[test]
    fn azure_block_id_uses_locked_zero_based_six_digit_shape() {
        assert_eq!(azure_block_id(0), "000000");
        assert_eq!(azure_block_id(1), "000001");
        assert_eq!(azure_block_id(9), "000009");
    }

    /// Only a lifecycle document with an abort rule and a day count passes.
    #[test]
    fn abort_rule_requires_the_rule_and_its_days() {
        assert!(has_abort_rule(
            "<Rule><AbortIncompleteMultipartUpload><DaysAfterInitiation>7</DaysAfterInitiation></AbortIncompleteMultipartUpload></Rule>"
        ));
        assert!(!has_abort_rule(
            "<Rule><Expiration><Days>7</Days></Expiration></Rule>"
        ));
        assert!(!has_abort_rule(""));
    }
}
