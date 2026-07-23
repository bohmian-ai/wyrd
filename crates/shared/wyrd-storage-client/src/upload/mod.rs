//! Upload plans, transfer hooks, and protocol dispatch.

mod azure_block_blob;
mod gcs_resumable;
mod local_fs;
mod reader;
mod s3_multipart;
mod single_put;

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use bytes::Bytes;
use futures_util::stream::{BoxStream, StreamExt};
use tokio_util::io::ReaderStream;
use wyrd_client::WyrdClient;
use wyrd_spec::storage::{UploadId, UploadPlan};

use crate::error::StorageClientError;
use reader::SourceReader;

#[cfg(test)]
mod tests;

/// A bounded asynchronous source of artifact chunks.
pub trait ArtifactSource: Send {
    /// Return a known source size when available.
    fn size_hint(&self) -> Option<u64>;
    /// Consume the source as a chunk stream.
    fn into_stream(self) -> BoxStream<'static, Result<Bytes, StorageClientError>>;
}

/// Progress emitted while one artifact source is consumed by a provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadProgress {
    /// Stable server-owned identity for the upload operation.
    pub upload_id: UploadId,
    /// Bytes consumed by the provider transfer so far.
    pub uploaded_bytes: u64,
    /// Expected source size when the source exposes one.
    pub total_bytes: Option<u64>,
}

/// Thread-safe callback for caller-owned upload progress handling.
pub type UploadProgressSink = Arc<dyn Fn(UploadProgress) + Send + Sync + 'static>;

impl ArtifactSource for Vec<u8> {
    fn size_hint(&self) -> Option<u64> {
        Some(self.len() as u64)
    }

    fn into_stream(self) -> BoxStream<'static, Result<Bytes, StorageClientError>> {
        futures_util::stream::once(async move { Ok(Bytes::from(self)) }).boxed()
    }
}

impl ArtifactSource for Bytes {
    fn size_hint(&self) -> Option<u64> {
        Some(self.len() as u64)
    }

    fn into_stream(self) -> BoxStream<'static, Result<Bytes, StorageClientError>> {
        futures_util::stream::once(async move { Ok(self) }).boxed()
    }
}

/// A filesystem-backed artifact source.
#[derive(Debug, Clone)]
pub struct FileSource {
    path: PathBuf,
    size: Option<u64>,
}

impl FileSource {
    /// Create a source by path. The file is opened when the upload starts.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            size: None,
        }
    }

    /// Open a source and capture its size without reading its contents.
    pub async fn open(path: impl Into<PathBuf>) -> Result<Self, StorageClientError> {
        let path = path.into();
        let size = tokio::fs::metadata(&path).await?.len();
        Ok(Self {
            path,
            size: Some(size),
        })
    }
}

impl ArtifactSource for FileSource {
    fn size_hint(&self) -> Option<u64> {
        self.size
    }

    fn into_stream(self) -> BoxStream<'static, Result<Bytes, StorageClientError>> {
        async_stream::try_stream! {
            let file = tokio::fs::File::open(self.path).await?;
            let mut reader = ReaderStream::new(file);
            while let Some(chunk) = reader.next().await {
                yield chunk.map_err(StorageClientError::Io)?;
            }
        }
        .boxed()
    }
}

/// Future returned by an on-demand S3 part URL minter.
pub(crate) type PartUrlFuture<'a> =
    Pin<Box<dyn Future<Output = Result<String, StorageClientError>> + Send + 'a>>;

/// On-demand S3 part URL callback.
pub(crate) type PartUrlMinter<'a> = Box<dyn FnMut(u32) -> PartUrlFuture<'a> + Send + 'a>;

/// Owned callback used by provider streams and chunk loops.
pub(crate) type ProgressCallback = Arc<dyn Fn(u64, Option<u64>) + Send + Sync + 'static>;

/// Hooks supplied by the engine around a byte transfer.
pub(crate) struct UploadHooks<'a> {
    /// Stable key to replay on direct backend requests.
    pub idempotency_key: &'a str,
    /// Optional `(uploaded, total)` callback.
    pub progress: Option<ProgressCallback>,
    /// On-demand S3 part URL minting callback.
    pub part_url_minter: Option<PartUrlMinter<'a>>,
}

/// Result of a completed transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UploadOutcome {
    /// The backend has accepted the complete object.
    Uploaded,
    /// The server must commit the backend multipart state.
    NeedsServerComplete(wyrd_spec::storage::UploadCompleteRequest),
}

impl UploadOutcome {
    /// Return the server completion body, if the protocol needs one.
    #[must_use]
    pub fn into_server_complete(self) -> Option<wyrd_spec::storage::UploadCompleteRequest> {
        match self {
            Self::Uploaded => None,
            Self::NeedsServerComplete(request) => Some(request),
        }
    }
}

pub(crate) async fn dispatch<S: ArtifactSource>(
    client: &WyrdClient,
    plan: &UploadPlan,
    source: S,
    mut hooks: UploadHooks<'_>,
) -> Result<UploadOutcome, StorageClientError> {
    validate_plan(plan)?;
    let total = source.size_hint();
    let reader = SourceReader::new(source.into_stream());
    match plan {
        UploadPlan::LocalFs { .. } => local_fs::upload(client, plan, reader, total, &hooks).await,
        UploadPlan::SinglePut { .. } => {
            single_put::upload(client, plan, reader, total, &hooks).await
        }
        UploadPlan::S3Multipart { .. } => {
            s3_multipart::upload(client, plan, reader, total, &mut hooks).await
        }
        UploadPlan::GcsResumable { .. } => {
            gcs_resumable::upload(client, plan, reader, total, &hooks).await
        }
        UploadPlan::AzureBlockBlob { .. } => {
            azure_block_blob::upload(client, plan, reader, total, &hooks).await
        }
    }
}

fn validate_plan(plan: &UploadPlan) -> Result<(), StorageClientError> {
    match plan {
        UploadPlan::S3Multipart {
            part_count,
            part_size_bytes,
            ..
        } if *part_count == 0 || *part_size_bytes == 0 => Err(StorageClientError::PlanInvalid(
            "S3 part count and size must be positive",
        )),
        UploadPlan::GcsResumable {
            chunk_size_bytes, ..
        } if *chunk_size_bytes == 0 => Err(StorageClientError::PlanInvalid(
            "GCS chunk size must be positive",
        )),
        UploadPlan::AzureBlockBlob {
            block_size_bytes,
            block_count_planned,
            ..
        } if *block_size_bytes == 0 || *block_count_planned == 0 => Err(
            StorageClientError::PlanInvalid("Azure block count and size must be positive"),
        ),
        _ => Ok(()),
    }
}

pub(crate) fn checked_size(value: u64, name: &'static str) -> Result<usize, StorageClientError> {
    let size = usize::try_from(value).map_err(|_| StorageClientError::PlanInvalid(name))?;
    if size > isize::MAX as usize {
        return Err(StorageClientError::PlanInvalid(name));
    }
    Ok(size)
}

pub(crate) fn report(progress: Option<&ProgressCallback>, uploaded: u64, total: Option<u64>) {
    if let Some(progress) = progress {
        progress(uploaded, total);
    }
}

/// Build the authenticated S3 part-URL callback for one server-owned upload.
pub(crate) fn s3_part_url_minter<'a>(
    storage: &'a crate::WyrdStorageClient,
    upload_id: &'a UploadId,
    plan: &UploadPlan,
) -> Option<PartUrlMinter<'a>> {
    if !matches!(plan, UploadPlan::S3Multipart { .. }) {
        return None;
    }
    let storage = storage.clone();
    let upload_id = upload_id.clone();
    Some(Box::new(move |part_number| {
        let storage = storage.clone();
        let upload_id = upload_id.clone();
        Box::pin(async move { storage.part_url(&upload_id, part_number).await })
    }))
}

pub(crate) fn plan_variant(plan: &UploadPlan) -> &'static str {
    match plan {
        UploadPlan::LocalFs { .. } => "LocalFs",
        UploadPlan::SinglePut { .. } => "SinglePut",
        UploadPlan::S3Multipart { .. } => "S3Multipart",
        UploadPlan::GcsResumable { .. } => "GcsResumable",
        UploadPlan::AzureBlockBlob { .. } => "AzureBlockBlob",
    }
}
