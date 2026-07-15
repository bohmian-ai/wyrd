//! Authenticated LocalFs upload.

use wyrd_client::WyrdClient;
use wyrd_spec::storage::UploadPlan;

use super::UploadOutcome;
use crate::error::{StorageClientError, from_authenticated};
use crate::upload::reader::SourceReader;

pub(crate) async fn upload(
    client: &WyrdClient,
    plan: &UploadPlan,
    reader: SourceReader,
) -> Result<UploadOutcome, StorageClientError> {
    let UploadPlan::LocalFs { put_url, .. } = plan else {
        return Err(StorageClientError::PlanMismatch {
            expected: "LocalFs",
            actual: super::plan_variant(plan),
        });
    };
    client
        .http()
        .request_stream(
            reqwest::Method::PUT,
            put_url,
            reqwest::Body::wrap_stream(reader.stream()),
        )
        .await
        .map_err(from_authenticated)?;
    Ok(UploadOutcome::Uploaded)
}
