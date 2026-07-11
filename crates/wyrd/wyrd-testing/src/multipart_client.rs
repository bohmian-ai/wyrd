//! Shared client-side upload-loop driver for storage tests.
//!
//! The `MultipartClient` projects an [`UploadPlan`] onto the real wire calls
//! an SDK client must issue: PUTs to presigned URLs, chunked PUTs against a
//! GCS resumable session URI, staged block PUTs to an Azure SAS URL. No bytes
//! flow through the Wyrd server on these paths — the helper drives reqwest
//! against the storage backend directly. The server only mints URLs and
//! finalizes upload state.
//!
//! Used by both:
//! - signer-layer integration tests in `wyrd-storage/tests/integration_*.rs`
//! - server HTTP e2e tests in `wyrd-server/tests/storage_e2e.rs`
//!
//! Having a single canonical implementation prevents the two test tiers from
//! drifting in how they upload bytes.

use std::future::Future;

use base64::Engine;
use thiserror::Error;
use wyrd_spec::storage::{S3CompletedPart, UploadPlan};

/// Multipart-client errors surfaced to test code.
#[derive(Debug, Error)]
pub enum MultipartClientError {
    /// The plan variant did not match the method that was called.
    #[error("plan variant mismatch: expected {expected}, got {got}")]
    PlanMismatch {
        /// Variant the caller wanted.
        expected: &'static str,
        /// Variant the caller passed.
        got: &'static str,
    },
    /// The backend returned a non-success HTTP status.
    #[error("upload PUT to {url} returned status {status}: {body}")]
    HttpStatus {
        /// URL the PUT targeted.
        url: String,
        /// HTTP status code.
        status: u16,
        /// Response body (truncated for readability).
        body: String,
    },
    /// reqwest error sending or receiving.
    #[error("transport error: {0}")]
    Transport(#[from] reqwest::Error),
    /// S3 returned no ETag on a part PUT.
    #[error("S3 part {part_number} response missing ETag header")]
    MissingS3ETag {
        /// One-based part number.
        part_number: u32,
    },
    /// User-supplied callback returned an error.
    #[error("URL minter callback failed: {0}")]
    UrlMinter(String),
}

/// Drives client uploads against presigned URLs, GCS session URIs, and Azure SAS URLs.
#[derive(Clone, Debug, Default)]
pub struct MultipartClient {
    http: reqwest::Client,
}

impl MultipartClient {
    /// Create a new client with a default reqwest client.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Borrow the inner reqwest client (for tests that need to issue raw GETs).
    #[must_use]
    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    /// PUT bytes to a presigned single-PUT URL.
    ///
    /// # Errors
    /// Returns an error on plan mismatch, transport failure, or non-success HTTP status.
    pub async fn single_put(
        &self,
        plan: &UploadPlan,
        bytes: Vec<u8>,
    ) -> Result<(), MultipartClientError> {
        let UploadPlan::SinglePut {
            put_url,
            required_headers,
            ..
        } = plan
        else {
            return Err(MultipartClientError::PlanMismatch {
                expected: "SinglePut",
                got: plan_variant(plan),
            });
        };
        let mut request = self.http.put(put_url);
        for header in required_headers {
            request = request.header(header.name.as_str(), header.value.as_str());
        }
        let response = request.body(bytes).send().await?;
        ensure_success(put_url, response).await?;
        Ok(())
    }

    /// Drive an S3 multipart upload to completion.
    ///
    /// The caller supplies a `url_minter` async closure that returns a presigned
    /// PUT URL for the given one-based part number. This indirection lets the
    /// signer-layer tests wrap `S3Signer::presign_part` and the server-layer
    /// tests hit `/v1/cards/upload/{id}/parts/{n}/url` with the same driver.
    ///
    /// # Errors
    /// Returns an error on plan mismatch, transport failure, missing ETag,
    /// non-success HTTP status, or URL-minter failure.
    pub async fn s3_multipart<F, Fut, E>(
        &self,
        plan: &UploadPlan,
        url_minter: F,
        part_bytes: impl Fn(u32) -> Vec<u8>,
    ) -> Result<Vec<S3CompletedPart>, MultipartClientError>
    where
        F: Fn(u32) -> Fut,
        Fut: Future<Output = Result<String, E>>,
        E: std::fmt::Display,
    {
        let UploadPlan::S3Multipart { part_count, .. } = plan else {
            return Err(MultipartClientError::PlanMismatch {
                expected: "S3Multipart",
                got: plan_variant(plan),
            });
        };
        let mut parts = Vec::with_capacity(*part_count as usize);
        for part_number in 1..=*part_count {
            let url = url_minter(part_number)
                .await
                .map_err(|err| MultipartClientError::UrlMinter(err.to_string()))?;
            let response = self
                .http
                .put(&url)
                .body(part_bytes(part_number))
                .send()
                .await?;
            let e_tag = response
                .headers()
                .get("etag")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
                .ok_or(MultipartClientError::MissingS3ETag { part_number })?;
            ensure_success(&url, response).await?;
            parts.push(S3CompletedPart { part_number, e_tag });
        }
        Ok(parts)
    }

    /// Drive a GCS resumable upload to completion by PUTing chunks with `Content-Range`.
    ///
    /// `chunks` must concatenate to the full object. The final chunk's response
    /// status must be 200 or 201; intermediate chunks return 308 to signal more.
    ///
    /// # Errors
    /// Returns an error on plan mismatch, transport failure, or non-success HTTP status.
    pub async fn gcs_resumable(
        &self,
        plan: &UploadPlan,
        chunks: Vec<Vec<u8>>,
    ) -> Result<(), MultipartClientError> {
        let UploadPlan::GcsResumable { session_uri, .. } = plan else {
            return Err(MultipartClientError::PlanMismatch {
                expected: "GcsResumable",
                got: plan_variant(plan),
            });
        };
        let total: u64 = chunks.iter().map(|c| c.len() as u64).sum();
        let mut offset: u64 = 0;
        let last_index = chunks.len().saturating_sub(1);
        for (index, chunk) in chunks.into_iter().enumerate() {
            let len = chunk.len() as u64;
            if len == 0 {
                continue;
            }
            let end_inclusive = offset + len - 1;
            let content_range = format!("bytes {offset}-{end_inclusive}/{total}");
            let response = self
                .http
                .put(session_uri)
                .header("Content-Type", "application/octet-stream")
                .header("Content-Range", content_range)
                .body(chunk)
                .send()
                .await?;
            let status = response.status().as_u16();
            if index == last_index {
                if !response.status().is_success() {
                    return Err(MultipartClientError::HttpStatus {
                        url: session_uri.clone(),
                        status,
                        body: truncate_body(response.text().await?),
                    });
                }
            } else if !(status == 308 || response.status().is_success()) {
                return Err(MultipartClientError::HttpStatus {
                    url: session_uri.clone(),
                    status,
                    body: truncate_body(response.text().await?),
                });
            }
            offset += len;
        }
        Ok(())
    }

    /// Stage Azure block-blob blocks against a SAS URL.
    ///
    /// Each block is PUT with `comp=block&blockid={base64}` where the block id
    /// is the same zero-padded 6-digit ASCII form as
    /// `wyrd_storage::azure::azure_block_id` (kept in lockstep to avoid commit
    /// drift; the signer's `complete_blocklist_server` regenerates the same ids
    /// from the block count).
    ///
    /// Returns the number of blocks staged so the caller can pass it to
    /// `complete_blocklist_server` (signer tier) or the HTTP complete handler
    /// (server tier).
    ///
    /// # Errors
    /// Returns an error on plan mismatch, transport failure, or non-success HTTP status.
    pub async fn azure_block_blob(
        &self,
        plan: &UploadPlan,
        blocks: Vec<Vec<u8>>,
    ) -> Result<u32, MultipartClientError> {
        let UploadPlan::AzureBlockBlob { sas_url, .. } = plan else {
            return Err(MultipartClientError::PlanMismatch {
                expected: "AzureBlockBlob",
                got: plan_variant(plan),
            });
        };
        let count = u32::try_from(blocks.len()).map_err(|_| {
            MultipartClientError::UrlMinter(
                "azure_block_blob: more than u32::MAX blocks requested".to_owned(),
            )
        })?;
        for (index, block) in blocks.into_iter().enumerate() {
            let block_id_b64 =
                base64::engine::general_purpose::STANDARD.encode(format!("{index:06}").as_bytes());
            let block_url = format!("{sas_url}&comp=block&blockid={block_id_b64}");
            let response = self.http.put(&block_url).body(block).send().await?;
            ensure_success(&block_url, response).await?;
        }
        Ok(count)
    }

    /// GET an object via a presigned/SAS URL and return its body bytes.
    ///
    /// # Errors
    /// Returns an error on transport failure or non-success HTTP status.
    pub async fn download(&self, get_url: &str) -> Result<Vec<u8>, MultipartClientError> {
        let response = self.http.get(get_url).send().await?;
        if !response.status().is_success() {
            let status = response.status().as_u16();
            return Err(MultipartClientError::HttpStatus {
                url: get_url.to_owned(),
                status,
                body: truncate_body(response.text().await?),
            });
        }
        Ok(response.bytes().await?.to_vec())
    }
}

async fn ensure_success(
    url: &str,
    response: reqwest::Response,
) -> Result<(), MultipartClientError> {
    if response.status().is_success() {
        return Ok(());
    }
    let status = response.status().as_u16();
    let body = truncate_body(response.text().await?);
    Err(MultipartClientError::HttpStatus {
        url: url.to_owned(),
        status,
        body,
    })
}

fn truncate_body(body: String) -> String {
    if body.len() > 512 {
        format!("{}…(truncated {} bytes)", &body[..512], body.len() - 512)
    } else {
        body
    }
}

fn plan_variant(plan: &UploadPlan) -> &'static str {
    match plan {
        UploadPlan::SinglePut { .. } => "SinglePut",
        UploadPlan::LocalFs { .. } => "LocalFs",
        UploadPlan::S3Multipart { .. } => "S3Multipart",
        UploadPlan::GcsResumable { .. } => "GcsResumable",
        UploadPlan::AzureBlockBlob { .. } => "AzureBlockBlob",
    }
}
