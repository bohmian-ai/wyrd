//! Authenticated LocalFs download.

use std::path::Path;

use wyrd_client::WyrdClient;
use wyrd_spec::storage::DownloadPlan;

use super::{DownloadOutcome, DownloadVerification};
use crate::download::single_get::write_response;
use crate::{
    DownloadProgressSink,
    error::{StorageClientError, from_authenticated},
};

/// Downloads an unverified artifact from LocalFs storage via authenticated Wyrd client.
///
/// Uses the Wyrd client's auth token; does not require presigned URLs.
///
/// # Errors
/// Returns authenticated transport, backend, or local filesystem errors.
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
        .request_raw(reqwest::Method::GET, &plan.get_url)
        .await
        .map_err(from_authenticated)?;
    write_response(response, dest, None, None).await
}

/// Downloads and verifies an artifact through the authenticated LocalFs route.
///
/// # Errors
/// Returns authenticated transport, filesystem, or verification errors. The
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
        .request_raw(reqwest::Method::GET, &plan.get_url)
        .await
        .map_err(from_authenticated)?;
    write_response(response, dest, Some(verification), Some(progress)).await
}
