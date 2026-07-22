//! Authenticated LocalFs download.

use std::path::Path;

use wyrd_client::WyrdClient;
use wyrd_spec::storage::DownloadPlan;

use super::{DownloadOutcome, DownloadVerification};
use crate::download::single_get::write_response;
use crate::error::{StorageClientError, from_authenticated};

/// Downloads an artifact from LocalFs storage via authenticated Wyrd client.
///
/// Uses the Wyrd client's auth token; does not require presigned URLs.
pub(crate) async fn download(
    client: &WyrdClient,
    plan: &DownloadPlan,
    dest: &Path,
    verification: Option<DownloadVerification<'_>>,
) -> Result<DownloadOutcome, StorageClientError> {
    let response = client
        .request_raw(reqwest::Method::GET, &plan.get_url)
        .await
        .map_err(from_authenticated)?;
    write_response(response, dest, verification).await
}
