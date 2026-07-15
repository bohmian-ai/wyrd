//! Google Cloud Storage resumable upload.

use wyrd_client::WyrdClient;
use wyrd_spec::storage::UploadPlan;

use super::{UploadHooks, UploadOutcome, checked_size, report};
use crate::error::{StorageClientError, map_backend_response};
use crate::upload::reader::SourceReader;

pub(crate) async fn upload(
    client: &WyrdClient,
    plan: &UploadPlan,
    mut reader: SourceReader,
    total: Option<u64>,
    hooks: &UploadHooks<'_>,
) -> Result<UploadOutcome, StorageClientError> {
    let UploadPlan::GcsResumable {
        session_uri,
        chunk_size_bytes,
    } = plan
    else {
        return Err(StorageClientError::PlanMismatch {
            expected: "GcsResumable",
            actual: super::plan_variant(plan),
        });
    };
    let chunk_size = checked_size(*chunk_size_bytes, "GCS chunk size")?;
    let mut offset = 0u64;
    while let Some(bytes) = reader.next_chunk(chunk_size).await? {
        // V-004: reject an oversize source before the next PUT crosses the
        // wire. A source that outgrows its declared size_hint would otherwise
        // succeed silently and violate the artifact-hash contract.
        if let Some(expected) = total
            && offset + bytes.len() as u64 > expected
        {
            return Err(StorageClientError::SizeMismatch {
                expected,
                actual: offset + bytes.len() as u64,
            });
        }
        let end = offset + bytes.len() as u64 - 1;
        let total_text = total.map_or_else(|| "*".to_owned(), |value| value.to_string());
        let content_range = format!("bytes {offset}-{end}/{total_text}");
        let headers: [(&str, &str); 2] = [
            ("Content-Range", content_range.as_str()),
            ("Idempotency-Key", hooks.idempotency_key),
        ];
        let response = client
            .request_external_stream(
                reqwest::Method::PUT,
                session_uri,
                Some(reqwest::Body::from(bytes)),
                &headers,
            )
            .await
            .map_err(|_| StorageClientError::Transport {
                operation: "upload",
            })?;
        if !(response.status().is_success() || response.status().as_u16() == 308) {
            return Err(map_backend_response(response).await);
        }
        offset = end + 1;
        report(hooks, offset, total);
    }
    // V-004: reject an undersize source. Some backends will commit a truncated
    // object silently; the artifact-hash / expected_size contract requires
    // exact-length transfer.
    if let Some(expected) = total
        && expected != offset
    {
        return Err(StorageClientError::SizeMismatch {
            expected,
            actual: offset,
        });
    }
    Ok(UploadOutcome::Uploaded)
}
