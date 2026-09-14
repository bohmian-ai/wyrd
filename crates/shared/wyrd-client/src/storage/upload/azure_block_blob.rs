//! Azure block blob upload.

use crate::WyrdClient;
use base64::Engine;
use wyrd_spec::storage::{AzureBlockBlobComplete, UploadCompleteRequest, UploadPlan};

use super::{UploadHooks, UploadOutcome, checked_size, report};
use crate::storage::error::{StorageClientError, map_backend_response};
use crate::storage::upload::reader::SourceReader;

/// Upload a sized source to Azure as fixed-size staged blocks through the
/// plan's SAS URL, reporting progress after each block.
///
/// Block ids are zero-padded sequence numbers. The source must be exactly the
/// declared size and yield exactly `block_count_planned` blocks. Azure does
/// not commit staged blocks itself, so success returns a completion request
/// carrying the block count for the server to issue Put Block List.
///
/// # Errors
///
/// Returns `PlanMismatch` for a non-Azure plan, `PlanInvalid` when the source
/// size is unknown or the block geometry does not fit `usize`, `SizeMismatch`
/// when the source size disagrees with the planned block count or the bytes
/// actually read, `Transport` for request failures, source read errors, and
/// mapped backend errors for non-success responses.
///
/// # Cancellation
///
/// Dropping the future leaves already-staged blocks uncommitted in Azure;
/// nothing is visible until the server completes the upload.
pub(crate) async fn upload(
    client: &WyrdClient,
    plan: &UploadPlan,
    mut reader: SourceReader,
    total: Option<u64>,
    hooks: &UploadHooks<'_>,
) -> Result<UploadOutcome, StorageClientError> {
    let UploadPlan::AzureBlockBlob {
        sas_url,
        block_size_bytes,
        block_count_planned,
    } = plan
    else {
        return Err(StorageClientError::PlanMismatch {
            expected: "AzureBlockBlob",
            actual: super::plan_variant(plan),
        });
    };
    let block_size = checked_size(*block_size_bytes, "Azure block size")?;
    let expected = usize::try_from(*block_count_planned)
        .map_err(|_| StorageClientError::PlanInvalid("Azure block count exceeds usize"))?;
    let Some(size) = total else {
        return Err(StorageClientError::PlanInvalid(
            "Azure upload requires a known source size",
        ));
    };
    let actual = size
        .checked_add(*block_size_bytes - 1)
        .ok_or(StorageClientError::PlanInvalid(
            "Azure block count overflow",
        ))?
        / *block_size_bytes;
    if usize::try_from(actual)
        .map_err(|_| StorageClientError::PlanInvalid("Azure block count exceeds usize"))?
        != expected
    {
        return Err(StorageClientError::SizeMismatch {
            expected: *block_count_planned as u64,
            actual,
        });
    }
    let mut count = 0u32;
    let mut uploaded = 0u64;
    while let Some(bytes) = reader.next_chunk(block_size).await? {
        // V-004: an over-size source would spill into a would-be extra block
        // and violate the artifact-hash contract. Reject before we PUT.
        if uploaded + bytes.len() as u64 > size {
            return Err(StorageClientError::SizeMismatch {
                expected: size,
                actual: uploaded + bytes.len() as u64,
            });
        }
        let block_id =
            base64::engine::general_purpose::STANDARD.encode(format!("{count:06}").as_bytes());
        let url = format!("{sas_url}&comp=block&blockid={block_id}");
        let headers = [("Idempotency-Key", hooks.idempotency_key)];
        let response = client
            .request_external_stream(
                reqwest::Method::PUT,
                &url,
                Some(reqwest::Body::from(bytes.clone())),
                &headers,
            )
            .await
            .map_err(|_| StorageClientError::Transport {
                operation: "upload",
            })?;
        if !response.status().is_success() {
            return Err(map_backend_response(response).await);
        }
        uploaded += bytes.len() as u64;
        count += 1;
        report(hooks.progress.as_ref(), uploaded, total);
    }
    // V-004: reject an under-size source. Azure's Put Block List will happily
    // commit the shorter blob and downstream verification would fail with a
    // less actionable error later.
    if uploaded != size {
        return Err(StorageClientError::SizeMismatch {
            expected: size,
            actual: uploaded,
        });
    }
    Ok(UploadOutcome::NeedsServerComplete(
        UploadCompleteRequest::AzureBlockBlob(AzureBlockBlobComplete { block_count: count }),
    ))
}
