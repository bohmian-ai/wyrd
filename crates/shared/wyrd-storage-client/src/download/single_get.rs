//! Presigned and SAS download.

use std::path::Path;

use futures_util::StreamExt;
use wyrd_client::WyrdClient;
use wyrd_spec::storage::DownloadPlan;

use super::DownloadOutcome;
use crate::error::StorageClientError;

/// Downloads an artifact via presigned or SAS GET URL.
///
/// External request with no Wyrd auth headers.
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
    write_response(response, dest).await
}

/// Writes a successful HTTP response body to a file, tracking bytes written.
pub(crate) async fn write_response(
    response: reqwest::Response,
    dest: &Path,
) -> Result<DownloadOutcome, StorageClientError> {
    if !response.status().is_success() {
        return Err(crate::error::map_backend_response(response).await);
    }
    let mut file = tokio::fs::File::create(dest).await?;
    let mut stream = response.bytes_stream();
    let mut bytes_written = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| StorageClientError::Transport {
            operation: "download",
        })?;
        tokio::io::AsyncWriteExt::write_all(&mut file, &chunk).await?;
        bytes_written += chunk.len() as u64;
    }
    Ok(DownloadOutcome { bytes_written })
}
