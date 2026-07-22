//! Download plan dispatch and streaming response handling.

mod local_fs;
mod single_get;

use std::path::Path;

use wyrd_client::WyrdClient;
use wyrd_spec::storage::DownloadPlan;

use crate::error::StorageClientError;

/// Result of a download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadOutcome {
    /// Number of bytes written to the destination.
    pub bytes_written: u64,
}

pub(crate) async fn dispatch(
    client: &WyrdClient,
    plan: &DownloadPlan,
    dest: &Path,
) -> Result<DownloadOutcome, StorageClientError> {
    dispatch_with_verification(client, plan, dest, None).await
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DownloadVerification<'a> {
    /// Base64-encoded SHA-256 digest declared by the server.
    pub(crate) expected_sha256: &'a str,
    /// Byte length declared by the server.
    pub(crate) expected_size_bytes: u64,
}

/// Dispatch a transfer and verify it against the server declaration.
pub(crate) async fn dispatch_verified(
    client: &WyrdClient,
    plan: &DownloadPlan,
    dest: &Path,
    expected_sha256: &str,
    expected_size_bytes: u64,
) -> Result<DownloadOutcome, StorageClientError> {
    dispatch_with_verification(
        client,
        plan,
        dest,
        Some(DownloadVerification {
            expected_sha256,
            expected_size_bytes,
        }),
    )
    .await
}

async fn dispatch_with_verification(
    client: &WyrdClient,
    plan: &DownloadPlan,
    dest: &Path,
    verification: Option<DownloadVerification<'_>>,
) -> Result<DownloadOutcome, StorageClientError> {
    if plan.get_url.contains("/v1/cards/download/local/") {
        local_fs::download(client, plan, dest, verification).await
    } else {
        single_get::download(client, plan, dest, verification).await
    }
}
