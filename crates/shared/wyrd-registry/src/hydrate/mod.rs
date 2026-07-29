//! Client-side Card graph hydration and bundle publication.

mod bundle;
mod graph;
mod workspace;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use wyrd_spec::{error::WyrdError, reference::CardRef};

pub use wyrd_spec::registry::{
    HydratedArtifactManifest, HydratedBundleManifest, HydratedCardManifest, HydrationMode,
};

use crate::{CardSelector, RegistryContext};

use self::{bundle::HydrationBundleWriter, graph::resolve_graph, workspace::HydrationWorkspace};

/// Machine-readable result of a published hydration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HydrationSummary {
    /// Exact resolved root reference.
    pub root: CardRef,
    /// Published bundle directory.
    pub destination: PathBuf,
    /// Hydration mode used to build the bundle.
    pub mode: HydrationMode,
    /// Number of unique Cards in the bundle.
    pub card_count: usize,
    /// Number of server-owned artifact inventory entries.
    pub artifact_count: usize,
    /// Number of artifact payloads downloaded and verified.
    pub downloaded_artifact_count: usize,
}

/// Hydrates a resolved Card graph into a locally published bundle.
///
/// The hydrator owns graph-oriented orchestration while sharing the authenticated registry
/// context used by other client capabilities. It does not add graph behavior to [`crate::Cards`].
#[derive(Clone)]
pub struct CardGraphHydrator {
    context: RegistryContext,
}

impl CardGraphHydrator {
    /// Creates a graph hydrator from an authenticated registry context.
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
    /// # Errors
    ///
    /// Returns an error when graph resolution fails, the destination is invalid, bundle content
    /// cannot be written, an artifact cannot be downloaded, or the staged bundle cannot be
    /// published safely.
    pub async fn hydrate(
        &self,
        selector: &CardSelector,
        destination: &Path,
        mode: HydrationMode,
    ) -> Result<HydrationSummary, WyrdError> {
        let graph = resolve_graph(&self.context.engine, selector).await?;
        let workspace = HydrationWorkspace::prepare(destination)?;
        let write_result = {
            let writer = HydrationBundleWriter::new(&self.context, workspace.staging(), mode);
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

        Ok(HydrationSummary {
            root: graph.root,
            destination: workspace.destination().to_path_buf(),
            card_count: graph.cards.len(),
            artifact_count: stats.artifact_count,
            downloaded_artifact_count: stats.downloaded_artifact_count,
            mode,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::HydrationMode;

    /// Hydration modes retain their stable bundle wire values.
    #[test]
    fn hydration_mode_uses_bundle_wire_values() {
        assert_eq!(
            serde_json::to_string(&HydrationMode::MetadataOnly).expect("metadata mode serializes"),
            "\"metadata\""
        );
        assert_eq!(
            serde_json::to_string(&HydrationMode::Complete).expect("complete mode serializes"),
            "\"complete\""
        );
    }
}
