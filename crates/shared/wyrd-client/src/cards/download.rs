//! Private server-authorized artifact download orchestration.

use std::path::Path;
use std::sync::Arc;

use crate::WyrdClient;
use crate::storage::WyrdStorageClient;
use wyrd_spec::ids::CardUid;
use wyrd_spec::registry::RelativeArtifactPath;
use wyrd_spec::storage::{DownloadInitRequest, DownloadInitResponse};

use crate::cards::{download_progress::DownloadProgressDisplay, error::RegistryEngineError};

/// Initialize, render, and execute one verified artifact download.
///
/// # Errors
/// Returns the existing registry or storage error when initialization, transfer,
/// local writing, or verification fails. The display's bar is removed before
/// the error is returned.
///
/// # Cancellation
/// Cancellation can leave a partial destination file for the caller's staging
/// owner to discard.
pub(crate) async fn download_artifact(
    client: &WyrdClient,
    storage: &WyrdStorageClient,
    card_uid: &CardUid,
    relative_path: &RelativeArtifactPath,
    dest: &Path,
    display: Arc<DownloadProgressDisplay>,
) -> Result<(), RegistryEngineError> {
    let response: DownloadInitResponse = client
        .request_json(
            reqwest::Method::POST,
            "/v1/cards/download/init",
            Some(&DownloadInitRequest {
                card_uid: card_uid.clone(),
                relative_path: relative_path.as_str().to_owned(),
                ttl_secs: None,
            }),
        )
        .await?;
    display.start(relative_path, Some(response.size_bytes));
    let path = relative_path.clone();
    let progress_display = Arc::clone(&display);
    let result = storage
        .download_verified_with_progress(
            &response.plan,
            dest,
            &response.sha256,
            response.size_bytes,
            Arc::new(move |downloaded_bytes, total_bytes| {
                progress_display.set_position(&path, downloaded_bytes, total_bytes);
            }),
        )
        .await
        .map(|_| ())
        .map_err(RegistryEngineError::from);
    display.finish(relative_path, result.is_ok());
    result
}
