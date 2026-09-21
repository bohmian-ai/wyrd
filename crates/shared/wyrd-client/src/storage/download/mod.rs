//! Download plan dispatch and streaming response handling.

mod local_fs;
mod single_get;

use std::path::Path;

use crate::WyrdClient;
use wyrd_spec::storage::DownloadPlan;

use crate::storage::{DownloadProgressSink, error::StorageClientError};

/// Result of a download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadOutcome {
    /// Number of bytes written to the destination.
    pub bytes_written: u64,
}

/// Dispatches an unverified transfer through its plan-selected backend.
///
/// # Errors
/// Returns transfer, backend, or filesystem errors from the selected download
/// path.
///
/// # Cancellation
/// Cancellation can leave a partial destination file for the caller to
/// discard.
pub(crate) async fn dispatch(
    client: &WyrdClient,
    plan: &DownloadPlan,
    dest: &Path,
) -> Result<DownloadOutcome, StorageClientError> {
    if plan.get_url.contains("/v1/cards/download/local?") {
        local_fs::download(client, plan, dest).await
    } else {
        single_get::download(client, plan, dest).await
    }
}

/// Server-declared integrity expectations for one downloaded artifact.
///
/// Built from the download plan by [`dispatch_verified`] and passed to the
/// backend-specific download paths, which fail the transfer when the written
/// bytes do not match the declared size and digest.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DownloadVerification<'a> {
    /// Base64-encoded SHA-256 digest declared by the server.
    pub(crate) expected_sha256: &'a str,
    /// Byte length declared by the server.
    pub(crate) expected_size_bytes: u64,
}

/// Dispatch a transfer, report successful writes, and verify the server declaration.
///
/// # Errors
/// Returns transfer, filesystem, or verification errors.
///
/// # Cancellation
/// Cancellation can leave the destination partially written after reporting
/// its committed bytes.
pub(crate) async fn dispatch_verified(
    client: &WyrdClient,
    plan: &DownloadPlan,
    dest: &Path,
    expected_sha256: &str,
    expected_size_bytes: u64,
    progress: DownloadProgressSink,
) -> Result<DownloadOutcome, StorageClientError> {
    let verification = DownloadVerification {
        expected_sha256,
        expected_size_bytes,
    };
    if plan.get_url.contains("/v1/cards/download/local?") {
        local_fs::download_verified(client, plan, dest, verification, &progress).await
    } else {
        single_get::download_verified(client, plan, dest, verification, &progress).await
    }
}
