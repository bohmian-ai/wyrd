//! Client-side Card graph pull and bundle publication.

mod bundle;
mod graph;
mod workspace;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use wyrd_spec::{error::WyrdError, reference::CardRef};

pub use wyrd_spec::registry::{
    BundleArtifactManifest, BundleCardManifest, BundleManifest, BundleMode,
};

use crate::cards::{CardSelector, RegistryContext};

use self::{bundle::BundleWriter, workspace::BundleWorkspace};

/// Machine-readable result of a published pull.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullSummary {
    /// Exact resolved root reference.
    pub root: CardRef,
    /// Published bundle directory.
    pub destination: PathBuf,
    /// Bundle mode used to build the bundle.
    pub mode: BundleMode,
    /// Number of unique Cards in the bundle.
    pub card_count: usize,
    /// Number of server-owned artifact inventory entries.
    pub artifact_count: usize,
    /// Number of artifact payloads downloaded and verified.
    pub downloaded_artifact_count: usize,
}

/// Pulls a resolved Card graph into a locally published bundle.
///
/// The puller owns graph-oriented orchestration while sharing the authenticated registry
/// context used by other client capabilities. It does not add graph behavior to [`crate::cards::Cards`].
#[derive(Clone)]
pub struct CardGraphPuller {
    /// Authenticated, tenant-scoped registry context shared with other client
    /// capabilities; every resolve and artifact download issued while
    /// pulling a graph goes through it.
    context: RegistryContext,
}

impl CardGraphPuller {
    /// Creates a graph puller from an authenticated registry context.
    ///
    /// # Arguments
    /// * `context` - The authenticated registry context every read goes through.
    #[must_use]
    pub fn new(context: RegistryContext) -> Self {
        Self { context }
    }

    /// Resolves a Card graph, writes it into staging, and atomically publishes the bundle.
    ///
    /// Graph and artifact reads are asynchronous registry operations. Local YAML and filesystem
    /// writes remain synchronous. If writing fails, staging is removed best-effort. Publishing
    /// replaces an existing destination through a rollback-capable workspace operation.
    ///
    /// Cancellation can leave the operation's staging directory behind because cancellation
    /// prevents the explicit cleanup path from running.
    ///
    /// # Arguments
    /// * `selector` - The root Card.
    /// * `destination` - The bundle directory to publish.
    /// * `mode` - Whether artifacts are downloaded and verified or only metadata is written.
    ///
    /// # Errors
    ///
    /// Returns an error when graph resolution fails, the destination is invalid, bundle content
    /// cannot be written, an artifact cannot be downloaded, or the staged bundle cannot be
    /// published safely.
    pub async fn pull(
        &self,
        selector: &CardSelector,
        destination: &Path,
        mode: BundleMode,
    ) -> Result<PullSummary, WyrdError> {
        let graph = self.resolve_graph(selector).await?;
        let workspace = BundleWorkspace::prepare(destination)?;
        let write_result = {
            let writer = BundleWriter::new(&self.context, workspace.staging(), mode);
            writer.write(&graph).await
        };
        let stats = match write_result {
            Ok(stats) => stats,
            Err(error) => {
                workspace.abort();
                return Err(error);
            }
        };

        if let Err(error) = workspace.publish() {
            workspace.abort();
            return Err(error);
        }

        Ok(PullSummary {
            root: graph.root,
            destination: workspace.destination().to_path_buf(),
            card_count: graph.cards.len(),
            artifact_count: stats.artifact_count,
            downloaded_artifact_count: stats.downloaded_artifact_count,
            mode,
        })
    }
}

/// Pull entry points.
#[cfg(test)]
mod tests {
    use super::BundleMode;

    /// Bundle modes retain their stable bundle wire values.
    #[test]
    fn bundle_mode_uses_bundle_wire_values() {
        assert_eq!(
            serde_json::to_string(&BundleMode::MetadataOnly).expect("metadata mode serializes"),
            "\"metadata\""
        );
        assert_eq!(
            serde_json::to_string(&BundleMode::Complete).expect("complete mode serializes"),
            "\"complete\""
        );
    }
}
