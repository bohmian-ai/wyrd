//! Private server-authorized artifact download orchestration.

use std::path::Path;

use wyrd_client::WyrdClient;
use wyrd_spec::ids::CardUid;
use wyrd_spec::registry::RelativeArtifactPath;
use wyrd_spec::storage::{DownloadInitRequest, DownloadInitResponse};
use wyrd_storage_client::WyrdStorageClient;

use crate::error::RegistryEngineError;

/// Initialize and execute one verified artifact download.
pub(crate) async fn download_artifact(
    client: &WyrdClient,
    storage: &WyrdStorageClient,
    card_uid: &CardUid,
    relative_path: &RelativeArtifactPath,
    dest: &Path,
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
    storage
        .download_verified(&response.plan, dest, &response.sha256, response.size_bytes)
        .await?;
    Ok(())
}
