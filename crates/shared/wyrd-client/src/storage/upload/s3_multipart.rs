//! S3 multipart upload.
//!
//! Each part PUT is bounded-retryable: transport failures and 5xx statuses
//! retry up to three attempts with exponential backoff, and a 403 response
//! (interpreted as presigned-URL expiry) triggers a fresh
//! [`PartUrlMinter`][crate::storage::upload::PartUrlMinter] call before the next
//! attempt. The completion payload records exactly one `(part_number, etag)`
//! entry per part in ascending order — retries never duplicate a part in the
//! completion list and never invert order (V-003).

use std::time::Duration;

use crate::WyrdClient;
use bytes::Bytes;
use wyrd_spec::storage::{
    HeaderPair, S3CompletedPart, S3MultipartComplete, UploadCompleteRequest, UploadPlan,
};

use super::{UploadHooks, UploadOutcome, checked_size, report};
use crate::storage::error::{StorageClientError, map_backend_response};
use crate::storage::upload::reader::SourceReader;

/// Total PUT attempts per part (first try plus retries) before a transport
/// failure, 5xx, or 403 is returned to the caller.
const MAX_PART_ATTEMPTS: u32 = 3;

/// Executes S3 multipart upload with retry and progress tracking.
///
/// Each part is retried up to 3 times on transport failure or 5xx. A 403
/// (expired presigned URL) triggers a fresh URL mint via
/// `hooks.part_url_minter`. Returns a completion payload with one
/// `(part_number, etag)` entry per part in ascending order—retries never
/// duplicate parts (V-003).
///
/// # Errors
///
/// Returns `PlanMismatch` for a non-multipart plan, `PlanInvalid` when part
/// sizing does not fit this platform, a source error from the reader,
/// `SizeMismatch` when the source length differs from the plan, and the
/// part-URL mint, transport, or backend status error left after retries.
///
/// # Cancellation
///
/// Cancelling can leave earlier parts uploaded to an incomplete multipart
/// upload; nothing is completed on the server by this function.
pub(crate) async fn upload(
    client: &WyrdClient,
    plan: &UploadPlan,
    mut reader: SourceReader,
    total: Option<u64>,
    hooks: &mut UploadHooks<'_>,
) -> Result<UploadOutcome, StorageClientError> {
    let UploadPlan::S3Multipart {
        part_count,
        part_size_bytes,
        required_headers,
        ..
    } = plan
    else {
        return Err(StorageClientError::PlanMismatch {
            expected: "S3Multipart",
            actual: super::plan_variant(plan),
        });
    };
    let part_size = checked_size(*part_size_bytes, "S3 part size")?;
    let capacity = usize::try_from(*part_count)
        .map_err(|_| StorageClientError::PlanInvalid("S3 part count exceeds usize"))?;
    let mut parts = Vec::with_capacity(capacity);
    let mut uploaded = 0u64;
    for part_number in 1..=*part_count {
        let bytes =
            reader
                .next_chunk(part_size)
                .await?
                .ok_or(StorageClientError::SizeMismatch {
                    expected: total.unwrap_or(*part_count as u64 * *part_size_bytes),
                    actual: uploaded,
                })?;
        let e_tag =
            send_part_with_retry(client, hooks, part_number, bytes.clone(), required_headers)
                .await?;
        uploaded += bytes.len() as u64;
        report(hooks.progress.as_ref(), uploaded, total);
        parts.push(S3CompletedPart { part_number, e_tag });
    }
    if reader.has_more().await? {
        return Err(StorageClientError::SizeMismatch {
            expected: total.unwrap_or(*part_count as u64 * *part_size_bytes),
            actual: uploaded + 1,
        });
    }
    if let Some(expected) = total
        && expected != uploaded
    {
        return Err(StorageClientError::SizeMismatch {
            expected,
            actual: uploaded,
        });
    }
    Ok(UploadOutcome::NeedsServerComplete(
        UploadCompleteRequest::S3Multipart(S3MultipartComplete { parts }),
    ))
}

/// PUT one part with bounded retry and presigned-URL remint on 403 expiry.
///
/// # Errors
///
/// Returns the part-URL mint error, or the transport or backend status error
/// once the retry budget is exhausted or the status is not retryable.
///
/// # Cancellation
///
/// Cancelling mid-request leaves the part's upload state unknown; a later
/// attempt re-sends the same part number.
async fn send_part_with_retry(
    client: &WyrdClient,
    hooks: &mut UploadHooks<'_>,
    part_number: u32,
    bytes: Bytes,
    required_headers: &[HeaderPair],
) -> Result<String, StorageClientError> {
    let mut attempt = 0u32;
    loop {
        let url = mint_part_url(hooks, part_number).await?;
        let mut headers: Vec<(&str, &str)> = required_headers
            .iter()
            .map(|h| (h.name.as_str(), h.value.as_str()))
            .collect();
        headers.push(("Idempotency-Key", hooks.idempotency_key));

        let outcome = client
            .request_external_stream(
                reqwest::Method::PUT,
                &url,
                Some(reqwest::Body::from(bytes.clone())),
                &headers,
            )
            .await;

        match outcome {
            Err(_err) => {
                if attempt + 1 < MAX_PART_ATTEMPTS {
                    sleep_backoff(attempt).await;
                    attempt += 1;
                    continue;
                }
                return Err(StorageClientError::Transport {
                    operation: "upload",
                });
            }
            Ok(response) => {
                let status = response.status().as_u16();
                if response.status().is_success() {
                    let e_tag = response
                        .headers()
                        .get("etag")
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_owned)
                        .ok_or(StorageClientError::MissingETag { part_number })?;
                    return Ok(e_tag);
                }
                let retryable = status == 403 || (500..600).contains(&status);
                if retryable && attempt + 1 < MAX_PART_ATTEMPTS {
                    sleep_backoff(attempt).await;
                    attempt += 1;
                    continue;
                }
                return Err(map_backend_response(response).await);
            }
        }
    }
}

/// Obtain a fresh presigned PUT URL for `part_number` from the upload hooks.
///
/// Called before every part attempt so a retry after a 403 always uses a
/// newly minted URL.
///
/// # Errors
///
/// Returns `MissingHook("part_url_minter")` when the engine supplied no
/// minter, or the minter's own error (typically a server request failure).
async fn mint_part_url(
    hooks: &mut UploadHooks<'_>,
    part_number: u32,
) -> Result<String, StorageClientError> {
    let minter = hooks
        .part_url_minter
        .as_mut()
        .ok_or(StorageClientError::MissingHook("part_url_minter"))?;
    minter(part_number).await
}

/// Sleep before the next part attempt: 50 ms after the first failure, 250 ms
/// after the second, and 500 ms for any later attempt.
///
/// Delays are deliberately short so retry tests do not stall; cancellation
/// simply cancels the timer.
async fn sleep_backoff(attempt: u32) {
    /// Per-attempt backoff in milliseconds; attempts past the table use 500 ms.
    // Attempt 0 → 50 ms, 1 → 250 ms. Values kept small so tests do not stall.
    const DELAY_MS: &[u64] = &[50, 250];
    let ms = DELAY_MS.get(attempt as usize).copied().unwrap_or(500);
    tokio::time::sleep(Duration::from_millis(ms)).await;
}
