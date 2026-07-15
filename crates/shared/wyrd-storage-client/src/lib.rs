//! Client-side execution of server-minted artifact transfer plans.
//!
//! [`WyrdStorageClient`] composes over a shared [`WyrdClient`], which is the
//! sole source of truth for Wyrd auth and transport. LocalFs plans reuse
//! the authenticated transport; cloud plans (S3/GCS/Azure) go through the
//! same connection pool via
//! [`WyrdClient::request_external_stream`], without leaking Wyrd
//! credentials cross-origin.

#![deny(missing_docs)]

use std::path::Path;

use wyrd_client::WyrdClient;
use wyrd_spec::storage::{DownloadPlan, UploadPlan};

pub mod download;
pub mod error;
pub mod upload;

pub use download::DownloadOutcome;
pub use error::StorageClientError;
pub use upload::{
    ArtifactSource, FileSource, PartUrlFuture, PartUrlMinter, UploadHooks, UploadOutcome,
};

/// Storage transfer handle that dispatches every HTTP call through a shared
/// [`WyrdClient`].
///
/// Auth, retry policy, base URL resolution, and the reqwest connection pool
/// all come from the wrapped [`WyrdClient`]; this handle owns only the plan
/// dispatch logic.
#[derive(Clone)]
pub struct WyrdStorageClient {
    client: WyrdClient,
}

impl WyrdStorageClient {
    /// Build a storage client that shares the given [`WyrdClient`]'s auth and
    /// transport.
    #[must_use]
    pub fn new(client: &WyrdClient) -> Self {
        Self {
            client: client.clone(),
        }
    }

    /// Upload an artifact according to a server-minted plan.
    pub async fn upload<S: ArtifactSource>(
        &self,
        plan: &UploadPlan,
        source: S,
        hooks: UploadHooks<'_>,
    ) -> Result<UploadOutcome, StorageClientError> {
        upload::dispatch(&self.client, plan, source, hooks).await
    }

    /// Download an artifact according to a server-minted plan.
    pub async fn download(
        &self,
        plan: &DownloadPlan,
        dest: &Path,
    ) -> Result<DownloadOutcome, StorageClientError> {
        download::dispatch(&self.client, plan, dest).await
    }
}
