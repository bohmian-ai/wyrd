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
    if plan.get_url.contains("/v1/cards/download/local/") {
        local_fs::download(client, plan, dest).await
    } else {
        single_get::download(client, plan, dest).await
    }
}
