//! Single PUT upload.

use crate::WyrdClient;
use futures_util::StreamExt;
use wyrd_spec::storage::UploadPlan;

use super::{UploadHooks, UploadOutcome, report};
use crate::storage::error::StorageClientError;
use crate::storage::upload::reader::SourceReader;

/// Executes a single-PUT upload via presigned URL.
///
/// Streams the source body directly to the storage backend with the plan's
/// required headers plus the idempotency key. No retry—transport failures
/// propagate immediately.
pub(crate) async fn upload(
    client: &WyrdClient,
    plan: &UploadPlan,
    reader: SourceReader,
    total: Option<u64>,
    hooks: &UploadHooks<'_>,
) -> Result<UploadOutcome, StorageClientError> {
    let UploadPlan::SinglePut {
        put_url,
        required_headers,
        ..
    } = plan
    else {
        return Err(StorageClientError::PlanMismatch {
            expected: "SinglePut",
            actual: super::plan_variant(plan),
        });
    };
    let mut headers: Vec<(&str, &str)> = required_headers
        .iter()
        .map(|h| (h.name.as_str(), h.value.as_str()))
        .collect();
    headers.push(("Idempotency-Key", hooks.idempotency_key));

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
    let response = client
        .request_external_stream(reqwest::Method::PUT, put_url, Some(body), &headers)
        .await
        .map_err(|_| StorageClientError::Transport {
            operation: "upload",
        })?;
    if !response.status().is_success() {
        return Err(crate::storage::error::map_backend_response(response).await);
    }
    Ok(UploadOutcome::Uploaded)
}
