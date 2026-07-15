//! Single PUT upload.

use wyrd_client::WyrdClient;
use wyrd_spec::storage::UploadPlan;

use super::{UploadHooks, UploadOutcome};
use crate::error::StorageClientError;
use crate::upload::reader::SourceReader;

/// Executes a single-PUT upload via presigned URL.
///
/// Streams the source body directly to the storage backend with the plan's
/// required headers plus the idempotency key. No retry—transport failures
/// propagate immediately.
pub(crate) async fn upload(
    client: &WyrdClient,
    plan: &UploadPlan,
    reader: SourceReader,
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

    let response = client
        .request_external_stream(
            reqwest::Method::PUT,
            put_url,
            Some(reqwest::Body::wrap_stream(reader.stream())),
            &headers,
        )
        .await
        .map_err(|_| StorageClientError::Transport {
            operation: "upload",
        })?;
    if !response.status().is_success() {
        return Err(crate::error::map_backend_response(response).await);
    }
    Ok(UploadOutcome::Uploaded)
}
