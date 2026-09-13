//! Presigned and SAS download.

use std::path::Path;

use crate::WyrdClient;
use base64::Engine;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use wyrd_spec::storage::DownloadPlan;

use super::{DownloadOutcome, DownloadVerification};
use crate::storage::{DownloadProgressSink, error::StorageClientError};

/// Downloads an artifact via presigned or SAS GET URL.
///
/// External request with no Wyrd auth headers.
///
/// # Errors
/// Returns transport, backend, or local filesystem errors.
///
/// # Cancellation
/// Cancellation can leave a partial destination file for the caller to
/// discard.
pub(crate) async fn download(
    client: &WyrdClient,
    plan: &DownloadPlan,
    dest: &Path,
) -> Result<DownloadOutcome, StorageClientError> {
    let response = client
        .request_external_stream(reqwest::Method::GET, &plan.get_url, None, &[])
        .await
        .map_err(|_| StorageClientError::Transport {
            operation: "download",
        })?;
    write_response(response, dest, None, None).await
}

/// Downloads and verifies an artifact via a presigned or SAS GET URL.
///
/// # Errors
/// Returns a transport, backend, filesystem, or verification error. The
/// progress sink observes only chunks successfully written before a failure.
///
/// # Cancellation
/// Cancellation can leave a partial destination after reporting its
/// successfully written bytes.
pub(crate) async fn download_verified(
    client: &WyrdClient,
    plan: &DownloadPlan,
    dest: &Path,
    verification: DownloadVerification<'_>,
    progress: &DownloadProgressSink,
) -> Result<DownloadOutcome, StorageClientError> {
    let response = client
        .request_external_stream(reqwest::Method::GET, &plan.get_url, None, &[])
        .await
        .map_err(|_| StorageClientError::Transport {
            operation: "download",
        })?;
    write_response(response, dest, Some(verification), Some(progress)).await
}

/// Writes a successful HTTP response body to a file, tracking bytes written.
///
/// # Errors
/// Returns a backend, transport, filesystem, or verification error. When a
/// sink is present, it reports only chunks written successfully before an error.
///
/// # Cancellation
/// Cancellation can leave the destination partially written after reporting
/// its committed bytes.
pub(crate) async fn write_response(
    response: reqwest::Response,
    dest: &Path,
    verification: Option<DownloadVerification<'_>>,
    progress: Option<&DownloadProgressSink>,
) -> Result<DownloadOutcome, StorageClientError> {
    if !response.status().is_success() {
        return Err(crate::storage::error::map_backend_response(response).await);
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
        if let Some(progress) = progress {
            progress(
                bytes_written,
                verification.map(|value| value.expected_size_bytes),
            );
        }
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
