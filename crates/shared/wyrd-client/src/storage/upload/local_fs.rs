//! Authenticated LocalFs upload.

use crate::WyrdClient;
use futures_util::StreamExt;
use wyrd_spec::storage::UploadPlan;

use super::{UploadHooks, UploadOutcome, report};
use crate::storage::error::{StorageClientError, from_authenticated};
use crate::storage::upload::reader::SourceReader;

/// Stream a source as one authenticated PUT to the Wyrd server's local
/// filesystem storage endpoint.
///
/// The request goes through the client's authenticated transport rather than
/// an external presigned URL. Progress is reported as each streamed chunk is
/// handed to the request body. The server stores the object directly, so
/// success returns [`UploadOutcome::Uploaded`].
///
/// # Errors
///
/// Returns `PlanMismatch` for a non-LocalFs plan and the mapped authenticated
/// transport error when the PUT fails, including source stream errors
/// surfaced through the request body.
///
/// # Cancellation
///
/// Dropping the future aborts the streamed body; the server may discard or
/// retain a partial write depending on how much it received.
pub(crate) async fn upload(
    client: &WyrdClient,
    plan: &UploadPlan,
    reader: SourceReader,
    total: Option<u64>,
    hooks: &UploadHooks<'_>,
) -> Result<UploadOutcome, StorageClientError> {
    let UploadPlan::LocalFs { put_url, .. } = plan else {
        return Err(StorageClientError::PlanMismatch {
            expected: "LocalFs",
            actual: super::plan_variant(plan),
        });
    };
    let body = if let Some(progress) = hooks.progress.clone() {
        let mut uploaded = 0_u64;
        let stream = reader.stream().map(move |chunk| {
            if let Ok(bytes) = &chunk {
                uploaded += bytes.len() as u64;
                report(Some(&progress), uploaded, total);
            }
            chunk
        });
        reqwest::Body::wrap_stream(stream)
    } else {
        reqwest::Body::wrap_stream(reader.stream())
    };
    client
        .request_stream(reqwest::Method::PUT, put_url, body)
        .await
        .map_err(from_authenticated)?;
    Ok(UploadOutcome::Uploaded)
}
