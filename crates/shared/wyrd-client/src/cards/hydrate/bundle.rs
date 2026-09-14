//! Staged bundle materialization for a resolved Card graph.

use std::{fs, path::Path, sync::Arc};

use serde::Serialize;
use wyrd_spec::{error::WyrdError, reference::CardRef};

use crate::cards::{
    RegistryContext, download::download_artifact, download_progress::DownloadProgressDisplay,
    error::RegistryEngineError,
};

use super::{
    HydratedArtifactManifest, HydratedBundleManifest, HydratedCardManifest, HydrationMode,
    graph::{ResolvedCard, ResolvedGraph, graph_error, validate_alias},
};

/// Counts produced while materializing one staged bundle.
pub(super) struct BundleStats {
    /// Number of artifact inventory entries represented in the bundle.
    pub(super) artifact_count: usize,
    /// Number of artifact payloads downloaded and verified.
    pub(super) downloaded_artifact_count: usize,
}

/// Paths for the stable documents belonging to one hydrated Card.
struct CardDocumentPaths<'a> {
    /// Card envelope path.
    card: &'a Path,
    /// Relationship projection path.
    relationships: &'a Path,
}

/// Materializes one resolved graph below an isolated staging directory.
pub(super) struct HydrationBundleWriter<'a> {
    /// Shared authenticated clients used for artifact downloads.
    context: &'a RegistryContext,
    /// Isolated directory that receives all bundle writes.
    staging: &'a Path,
    /// Requested artifact hydration depth.
    mode: HydrationMode,
}

impl<'a> HydrationBundleWriter<'a> {
    /// Creates a writer for one staged graph bundle.
    pub(super) fn new(
        context: &'a RegistryContext,
        staging: &'a Path,
        mode: HydrationMode,
    ) -> Self {
        Self {
            context,
            staging,
            mode,
        }
    }

    /// Writes every Card projection and the root metadata manifest.
    ///
    /// Local serialization and filesystem writes are synchronous. Complete hydration awaits each
    /// authorized artifact transfer before recording its local payload path.
    ///
    /// # Errors
    ///
    /// Returns an error when a path or alias is unsafe, local materialization fails, an
    /// artifact-bearing Card lacks a UID, serialization fails, or an artifact transfer fails.
    ///
    /// # Cancellation
    ///
    /// Cancellation during artifact transfer can leave partial files inside
    /// staging. The owning hydrator removes its unpublished staging bundle.
    pub(super) async fn write(&self, graph: &ResolvedGraph) -> Result<BundleStats, WyrdError> {
        let display = matches!(self.mode, HydrationMode::Complete)
            .then(|| Arc::new(DownloadProgressDisplay::stdout()));
        let result = async {
            let mut manifests = Vec::with_capacity(graph.cards.len());
            let mut downloaded_artifact_count = 0;
            for card in &graph.cards {
                let (manifest, downloaded) = self.write_card(card, display.as_ref()).await?;
                manifests.push(manifest);
                downloaded_artifact_count += downloaded;
            }
            let artifact_count = manifests.iter().map(|card| card.artifacts.len()).sum();
            let manifest = HydratedBundleManifest {
                api_version: "wyrd/hydrated-bundle/v1".to_owned(),
                hydration: self.mode,
                root: graph.root.clone(),
                card_count: manifests.len(),
                artifact_count,
                downloaded_artifact_count,
                cards: manifests,
            };
            write_yaml(&self.staging.join("metadata.yaml"), &manifest)?;
            Ok(BundleStats {
                artifact_count,
                downloaded_artifact_count,
            })
        }
        .await;
        if let Some(display) = display {
            display.clear();
        }
        result
    }

    /// Writes one Card's documents, artifacts, aliases, and manifest projection.
    ///
    /// # Errors
    ///
    /// Returns an error when the Card has no alias, a local path or write is invalid, the
    /// workflow display invariant is missing, or an artifact download fails.
    ///
    /// # Cancellation
    ///
    /// Cancellation during artifact transfer leaves only unpublished staging
    /// content for the owning hydrator to remove.
    async fn write_card(
        &self,
        card: &ResolvedCard,
        display: Option<&Arc<DownloadProgressDisplay>>,
    ) -> Result<(HydratedCardManifest, usize), WyrdError> {
        let canonical_alias = card
            .aliases
            .iter()
            .next()
            .ok_or_else(|| graph_error("hydrated Card has no bundle alias", &card.card_ref))?;
        validate_alias(canonical_alias)?;
        let card_dir = self.staging.join("cards").join(canonical_alias);
        let card_path = card_dir.join("card.yaml");
        let relationships_path = card_dir.join("relationships.yaml");
        let inventory_path = card_dir.join("artifacts.yaml");
        self.write_card_documents(
            card,
            CardDocumentPaths {
                card: &card_path,
                relationships: &relationships_path,
            },
        )?;

        let (artifacts, downloaded) = self.materialize_artifacts(card, &card_dir, display).await?;
        write_yaml(&inventory_path, &artifacts)?;
        let aliases = self.write_aliases(card, &card_path)?;
        Ok((
            HydratedCardManifest {
                aliases,
                card_ref: card.card_ref.clone(),
                card_path: relative_path(self.staging, &card_path)?,
                relationships_path: relative_path(self.staging, &relationships_path)?,
                artifact_inventory_path: relative_path(self.staging, &inventory_path)?,
                artifacts,
            },
            downloaded,
        ))
    }

    /// Writes the Card envelope and relationship projection.
    ///
    /// The inventory path is carried with the document set so callers construct all stable Card
    /// paths together; inventory content is written after artifact materialization.
    ///
    /// # Errors
    ///
    /// Returns an error when either YAML document cannot be serialized or written.
    fn write_card_documents(
        &self,
        card: &ResolvedCard,
        paths: CardDocumentPaths<'_>,
    ) -> Result<(), WyrdError> {
        write_yaml(paths.card, &card.card)?;
        write_yaml(paths.relationships, &card.card.relationships)?;
        Ok(())
    }

    /// Projects artifact inventory and optionally downloads each payload.
    ///
    /// # Errors
    ///
    /// Returns an error for unsafe paths, missing Card UIDs, a missing complete-hydration
    /// display, local directory failures, or failed and unverifiable artifact transfers.
    ///
    /// # Cancellation
    ///
    /// Cancellation can leave the current artifact partially written inside
    /// the unpublished staging bundle.
    async fn materialize_artifacts(
        &self,
        card: &ResolvedCard,
        card_dir: &Path,
        display: Option<&Arc<DownloadProgressDisplay>>,
    ) -> Result<(Vec<HydratedArtifactManifest>, usize), WyrdError> {
        let mut artifacts = Vec::with_capacity(card.inventory.artifacts.len());
        let mut downloaded = 0;
        for entry in &card.inventory.artifacts {
            validate_artifact_path(entry.relative_path.as_str(), &card.card_ref)?;
            let local_path = if matches!(self.mode, HydrationMode::Complete) {
                let display = display.ok_or_else(|| {
                    graph_error(
                        "complete hydration is missing its download display",
                        &card.card_ref,
                    )
                })?;
                let path = card_dir
                    .join("artifacts")
                    .join(entry.relative_path.as_str());
                if !path.starts_with(self.staging) {
                    return Err(graph_error(
                        "artifact path escaped hydration staging",
                        &card.card_ref,
                    ));
                }
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)
                        .map_err(RegistryEngineError::from)
                        .map_err(WyrdError::from)?;
                }
                let uid = card.card_ref.uid.as_ref().ok_or_else(|| {
                    graph_error("artifact-bearing Card is missing its UID", &card.card_ref)
                })?;
                download_artifact(
                    &self.context.engine.client,
                    &self.context.engine.storage,
                    uid,
                    &entry.relative_path,
                    &path,
                    Arc::clone(display),
                )
                .await
                .map_err(WyrdError::from)?;
                downloaded += 1;
                Some(relative_path(self.staging, &path)?)
            } else {
                None
            };
            artifacts.push(HydratedArtifactManifest {
                relative_path: entry.relative_path.as_str().to_owned(),
                sha256: entry.sha256.clone(),
                size_bytes: entry.size_bytes,
                content_type: entry.content_type.clone(),
                local_path,
            });
        }
        Ok((artifacts, downloaded))
    }

    /// Writes every alias record for one Card and returns stable alias ordering.
    ///
    /// # Errors
    ///
    /// Returns an error when an alias record path escapes staging or its YAML cannot be written.
    fn write_aliases(
        &self,
        card: &ResolvedCard,
        card_path: &Path,
    ) -> Result<Vec<String>, WyrdError> {
        let aliases = card.aliases.iter().cloned().collect::<Vec<_>>();
        for alias in &aliases {
            let alias_path = self.staging.join("aliases").join(format!("{alias}.yaml"));
            let alias_record = serde_json::json!({
                "alias": alias,
                "card_ref": card.card_ref,
                "card_path": relative_path(self.staging, card_path)?,
            });
            write_yaml(&alias_path, &alias_record)?;
        }
        Ok(aliases)
    }
}

/// Serializes one bounded hydration document and writes it below staging.
///
/// # Errors
///
/// Returns an error when the path has no parent, YAML serialization fails, or a local
/// directory/file operation fails.
fn write_yaml<T: Serialize>(path: &Path, value: &T) -> Result<(), WyrdError> {
    let parent = path.parent().ok_or_else(|| WyrdError::Internal {
        message: "hydration output path has no parent".to_owned(),
        details: serde_json::json!({ "path": path }),
    })?;
    fs::create_dir_all(parent)
        .map_err(RegistryEngineError::from)
        .map_err(WyrdError::from)?;
    let yaml = serde_yaml::to_string(value).map_err(|error| WyrdError::Internal {
        message: "failed to serialize hydrated bundle YAML".to_owned(),
        details: serde_json::json!({ "path": path, "error": error.to_string() }),
    })?;
    fs::write(path, yaml)
        .map_err(RegistryEngineError::from)
        .map_err(WyrdError::from)
}

/// Converts one path below the bundle root into a portable slash-separated path.
///
/// # Errors
///
/// Returns an error when the path does not remain below the bundle root.
fn relative_path(root: &Path, path: &Path) -> Result<String, WyrdError> {
    path.strip_prefix(root)
        .map_err(|_| WyrdError::RegistryInvalidCardSpec {
            message: "hydrated path escaped bundle destination".to_owned(),
            details: serde_json::json!({ "path": path, "root": root }),
        })
        .map(|path| path.to_string_lossy().replace('\\', "/"))
}

/// Validates an artifact path as a portable relative path confined to one Card directory.
///
/// # Errors
///
/// Returns an error when the path is empty, absolute, traverses directories, or contains
/// unsupported path characters.
fn validate_artifact_path(path: &str, card_ref: &CardRef) -> Result<(), WyrdError> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path.split('/').any(|segment| {
            segment.is_empty()
                || segment == "."
                || segment == ".."
                || segment.bytes().any(
                    |byte| !matches!(byte, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'.' | b'_' | b'-'),
                )
        })
    {
        return Err(graph_error(
            "artifact path is not confined to the hydrated bundle",
            card_ref,
        ));
    }
    Ok(())
}

/// Bundle hydration.
#[cfg(test)]
mod tests {
    use wyrd_semver::VersionBlock;
    use wyrd_spec::{
        envelope::CardKind,
        ids::{CardName, SpaceName},
        reference::CardRef,
    };

    use super::validate_artifact_path;

    /// Artifact inventory paths remain relative even when payloads are not downloaded.
    #[test]
    fn artifact_paths_are_validated_in_metadata_mode() {
        let card_ref = CardRef {
            kind: CardKind::Prompt,
            name: CardName::new("welcome").expect("test name is valid"),
            version: VersionBlock::parse("1.0.0").expect("test version is valid"),
            space: Some(SpaceName::new("default").expect("test space is valid")),
            uid: None,
        };
        assert!(validate_artifact_path("nested/file.bin", &card_ref).is_ok());
        assert!(validate_artifact_path("../escape", &card_ref).is_err());
        assert!(validate_artifact_path("nested/file name.bin", &card_ref).is_err());
    }
}
