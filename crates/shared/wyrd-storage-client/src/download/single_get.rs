//! Presigned and SAS download.

use std::path::Path;

use base64::Engine;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use wyrd_client::WyrdClient;
use wyrd_spec::storage::DownloadPlan;

use super::{DownloadOutcome, DownloadVerification};
use crate::error::StorageClientError;

/// Downloads an artifact via presigned or SAS GET URL.
///
/// External request with no Wyrd auth headers.
pub(crate) async fn download(
    client: &WyrdClient,
    plan: &DownloadPlan,
    dest: &Path,
    verification: Option<DownloadVerification<'_>>,
) -> Result<DownloadOutcome, StorageClientError> {
    let response = client
        .request_external_stream(reqwest::Method::GET, &plan.get_url, None, &[])
        .await
        .map_err(|_| StorageClientError::Transport {
            operation: "download",
        })?;
    write_response(response, dest, verification).await
}

/// Writes a successful HTTP response body to a file, tracking bytes written.
pub(crate) async fn write_response(
    response: reqwest::Response,
    dest: &Path,
    verification: Option<DownloadVerification<'_>>,
) -> Result<DownloadOutcome, StorageClientError> {
    if !response.status().is_success() {
        return Err(crate::error::map_backend_response(response).await);
    }
    let mut file = tokio::fs::File::create(dest).await?;
    let mut stream = response.bytes_stream();
    let mut bytes_written = 0u64;
    let mut digest = Sha256::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| StorageClientError::Transport {
            operation: "download",
        })?;
        tokio::io::AsyncWriteExt::write_all(&mut file, &chunk).await?;
        digest.update(&chunk);
        bytes_written += chunk.len() as u64;
    }
    if let Some(verification) = verification {
        let actual_sha256 = base64::engine::general_purpose::STANDARD.encode(digest.finalize());
        if bytes_written != verification.expected_size_bytes
            || actual_sha256 != verification.expected_sha256
        {
            return Err(StorageClientError::VerifyFailed {
                expected_sha256: verification.expected_sha256.to_owned(),
                actual_sha256,
                expected_size: verification.expected_size_bytes,
                actual_size: bytes_written,
            });
        }
    }
    Ok(DownloadOutcome { bytes_written })
}
