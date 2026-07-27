//! Client-side graph hydration and bundle publication.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use wyrd_spec::envelope::Card;
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::CardRef;
use wyrd_spec::registry::ArtifactInventoryResponse;

use crate::download::download_artifact;
use crate::error::RegistryEngineError;
use crate::handle::{CardSelector, Cards};
use crate::reads;

/// Hydration depth for a local Card bundle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HydrationMode {
    /// Write Card metadata and artifact inventories without payload bytes.
    #[serde(rename = "metadata")]
    MetadataOnly,
    /// Write Card metadata and verified artifact payload bytes.
    #[serde(rename = "complete")]
    Complete,
}

impl std::fmt::Display for HydrationMode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::MetadataOnly => "metadata",
            Self::Complete => "complete",
        })
    }
}

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

/// Stable metadata written to `metadata.yaml` at the bundle root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HydratedBundleManifest {
    /// Bundle format version.
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    /// Hydration mode used to build the bundle.
    pub hydration: HydrationMode,
    /// Exact resolved root reference.
    pub root: CardRef,
    /// Per-Card bundle entries.
    pub cards: Vec<HydratedCardManifest>,
    /// Number of unique Cards in the bundle.
    pub card_count: usize,
    /// Number of artifact inventory entries.
    pub artifact_count: usize,
    /// Number of downloaded artifact payloads.
    pub downloaded_artifact_count: usize,
}

/// One Card's stable bundle projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HydratedCardManifest {
    /// All user-facing aliases that point to this Card.
    pub aliases: Vec<String>,
    /// Exact resolved Card reference.
    pub card_ref: CardRef,
    /// Relative path to the Card envelope.
    pub card_path: String,
    /// Relative path to the relationship projection.
    pub relationships_path: String,
    /// Relative path to the artifact inventory.
    pub artifact_inventory_path: String,
    /// Artifact inventory and local payload paths.
    pub artifacts: Vec<HydratedArtifactManifest>,
}

/// One artifact inventory entry in a hydrated bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HydratedArtifactManifest {
    /// Relative server-owned artifact path.
    pub relative_path: String,
    /// Base64-encoded SHA-256 supplied by the server.
    pub sha256: String,
    /// Server-recorded byte length.
    pub size_bytes: i64,
    /// Optional MIME type.
    pub content_type: Option<String>,
    /// Relative local payload path when complete hydration downloaded it.
    pub local_path: Option<String>,
}

/// A Card, its server-owned artifact inventory, and safe local aliases.
#[derive(Debug)]
struct ResolvedCard {
    /// Exact server-resolved Card identity.
    card_ref: CardRef,
    /// Server-returned Card envelope to serialize into the bundle.
    card: Card,
    /// Server-owned artifact metadata used for manifest creation and downloads.
    inventory: ArtifactInventoryResponse,
    /// All validated aliases that point at this Card in the relationship graph.
    aliases: BTreeSet<String>,
}

/// Owns the local paths and rollback invariant for one bundle publication.
struct HydrationPublisher<'a> {
    /// Temporary directory containing the fully materialized bundle.
    staging: &'a Path,
    /// Final directory that should expose the new bundle after promotion.
    destination: &'a Path,
}

impl<'a> HydrationPublisher<'a> {
    /// Create a publisher for one staged bundle and its final destination.
    #[must_use]
    fn new(staging: &'a Path, destination: &'a Path) -> Self {
        Self {
            staging,
            destination,
        }
    }

    /// Promote the staged bundle while restoring the previous bundle on failure.
    ///
    /// A new destination is renamed directly into place. When a destination
    /// already exists, it is first moved to a uniquely named backup, the new
    /// bundle is promoted, and the backup is removed only after promotion
    /// succeeds. A failed promotion attempts restoration before returning.
    ///
    /// # Errors
    /// Returns a registry error when promotion fails, restoration fails, or
    /// the filesystem cannot inspect or rename the involved paths.
    fn publish(&self) -> Result<(), WyrdError> {
        if fs::metadata(self.destination).is_err() {
            fs::rename(self.staging, self.destination)
                .map_err(RegistryEngineError::from)
                .map_err(WyrdError::from)?;
            return Ok(());
        }
        let parent = self.destination.parent().unwrap_or_else(|| Path::new("."));
        let backup = parent.join(format!(
            ".{}.previous-{}",
            self.destination
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("wyrd-state"),
            uuid::Uuid::now_v7()
        ));
        fs::rename(self.destination, &backup)
            .map_err(RegistryEngineError::from)
            .map_err(WyrdError::from)?;
        let promotion = {
            #[cfg(test)]
            if take_publish_fault(FAIL_PROMOTION) {
                Err(std::io::Error::other(
                    "injected hydration promotion failure",
                ))
            } else {
                fs::rename(self.staging, self.destination)
            }
            #[cfg(not(test))]
            fs::rename(self.staging, self.destination)
        };
        if let Err(error) = promotion {
            let publication_error = WyrdError::from(RegistryEngineError::from(error));
            let restoration = {
                #[cfg(test)]
                if take_publish_fault(FAIL_RESTORE) {
                    Err(std::io::Error::other(
                        "injected hydration restoration failure",
                    ))
                } else {
                    fs::rename(&backup, self.destination)
                }
                #[cfg(not(test))]
                fs::rename(&backup, self.destination)
            };
            if let Err(error) = restoration {
                let restoration_error = RegistryEngineError::from(error);
                tracing::error!(
                    error = %restoration_error,
                    destination = %self.destination.display(),
                    backup = %backup.display(),
                    "hydration publication failed and rollback failed"
                );
                return Err(WyrdError::Internal {
                    message: "hydration publication and rollback failed".to_owned(),
                    details: serde_json::json!({
                        "publication_error": publication_error.to_string(),
                        "rollback_error": restoration_error.to_string(),
                        "destination": self.destination,
                        "backup": backup,
                    }),
                });
            }
            return Err(publication_error);
        }
        let cleanup = {
            #[cfg(test)]
            if take_publish_fault(FAIL_CLEANUP) {
                Err(std::io::Error::other(
                    "injected hydration backup cleanup failure",
                ))
            } else {
                fs::remove_dir_all(&backup)
            }
            #[cfg(not(test))]
            fs::remove_dir_all(&backup)
        };
        if let Err(error) = cleanup {
            tracing::warn!(
                error = %error,
                backup = %backup.display(),
                destination = %self.destination.display(),
                "published hydration bundle but could not remove the previous bundle"
            );
        }
        Ok(())
    }
}

impl Cards {
    /// Register an authored YAML Card tree through the shared loader and
    /// registration saga.
    ///
    /// # Errors
    /// Returns a structured registry error when loading, validation,
    /// artifact upload, or server finalization fails.
    pub async fn register_from_path(
        &self,
        path: &Path,
    ) -> Result<crate::RegistrationReceipt, WyrdError> {
        let tree = wyrd_loader::load(path).map_err(|error| WyrdError::RegistryInvalidCardSpec {
            message: format!("card tree failed to load: {error}"),
            details: serde_json::json!({ "path": path, "error": error.to_string() }),
        })?;
        let input = wyrd_loader::build_registration_input(tree).map_err(|error| {
            WyrdError::RegistryInvalidCardSpec {
                message: format!("card tree failed validation: {error}"),
                details: serde_json::json!({ "path": path, "error": error.to_string() }),
            }
        })?;
        self.register(&input).await
    }

    /// Resolve and materialize a root Card and its complete reachable graph.
    ///
    /// The graph is resolved before any destination is touched. Output is
    /// written below a staging directory and published with one directory
    /// rename, so a failed read or transfer cannot expose a partial bundle.
    ///
    /// # Errors
    /// Returns a structured error for graph inconsistencies, unauthorized or
    /// missing reads, unsafe aliases/paths, transfer verification failures,
    /// or publication failures.
    ///
    /// # Cancellation
    /// Cancellation during a remote read or artifact transfer stops hydration
    /// after the completed stage. Returned errors clean up staging, but a task
    /// dropped while awaiting external IO may leave its staging directory for
    /// later local cleanup.
    pub async fn hydrate(
        &self,
        selector: CardSelector,
        destination: &Path,
        mode: HydrationMode,
    ) -> Result<HydrationSummary, WyrdError> {
        let resolved = self.resolve_graph(selector).await?;
        let root = resolved
            .first()
            .map(|card| card.card_ref.clone())
            .ok_or_else(|| WyrdError::RegistryInvalidCardSpec {
                message: "hydration graph did not contain a root Card".to_owned(),
                details: serde_json::json!({}),
            })?;
        let parent = destination.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)
            .map_err(RegistryEngineError::from)
            .map_err(WyrdError::from)?;
        match fs::metadata(destination) {
            Ok(metadata) if !metadata.is_dir() => {
                return Err(WyrdError::RegistryInvalidCardSpec {
                    message: "hydration destination is not a directory".to_owned(),
                    details: serde_json::json!({ "destination": destination }),
                });
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(WyrdError::from(RegistryEngineError::from(error)));
            }
        }

        let staging = parent.join(format!(
            ".{}.staging-{}",
            destination
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("wyrd-state"),
            uuid::Uuid::now_v7()
        ));
        fs::create_dir(&staging)
            .map_err(RegistryEngineError::from)
            .map_err(WyrdError::from)?;
        let result = self.write_bundle(&staging, &resolved, mode, &root).await;
        if let Err(error) = result {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
        if let Err(error) = HydrationPublisher::new(&staging, destination).publish() {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }

        let artifact_count = resolved
            .iter()
            .map(|card| card.inventory.artifacts.len())
            .sum();
        Ok(HydrationSummary {
            root,
            destination: destination.to_path_buf(),
            mode,
            card_count: resolved.len(),
            artifact_count,
            downloaded_artifact_count: if matches!(mode, HydrationMode::Complete) {
                artifact_count
            } else {
                0
            },
        })
    }
}

impl Cards {
    /// Resolve the selected Card and every reachable relationship.
    ///
    /// A versionless named selector is resolved to an exact reference first.
    /// The traversal then reads each exact Card and its artifact inventory,
    /// rejects cycles and unsafe aliases, and returns cards in deterministic
    /// reference order without touching the local destination.
    ///
    /// # Errors
    /// Returns a registry error when a Card or inventory read fails, a response
    /// does not match its requested reference, a relationship is cyclic or
    /// untyped, or an alias/path invariant is violated.
    ///
    /// # Cancellation
    /// Cancellation stops the current remote read. No local bundle is created
    /// by this stage, so no local cleanup is required.
    async fn resolve_graph(&self, selector: CardSelector) -> Result<Vec<ResolvedCard>, WyrdError> {
        let root = match &selector {
            CardSelector::Named { version: None, .. } => {
                let response = reads::get_response(&self.engine.client, &selector)
                    .await
                    .map_err(WyrdError::from)?;
                reads::card_ref_from_card(&response.card)
                    .map_err(WyrdError::from)
                    .map(CardSelector::exact)?
            }
            _ => selector,
        };
        let root_response = reads::get_response(&self.engine.client, &root)
            .await
            .map_err(WyrdError::from)?;
        let root_ref = reads::card_ref_from_card(&root_response.card).map_err(WyrdError::from)?;
        /// Depth-first traversal work item for one relationship edge.
        enum Visit {
            /// Read and enter a Card, retaining its alias and root marker.
            Enter(CardRef, String, bool),
            /// Mark a Card as fully traversed after its children complete.
            Exit(String),
        }
        /// Resolution state used to detect cycles during graph traversal.
        #[derive(Clone, Copy, PartialEq, Eq)]
        enum VisitState {
            /// The Card is on the active traversal stack.
            Visiting,
            /// The Card and all reachable children have been materialized.
            Resolved,
        }
        let mut stack = vec![Visit::Enter(root_ref, String::from("root"), true)];
        let mut states = BTreeMap::<String, VisitState>::new();
        let mut aliases = BTreeMap::<String, String>::new();
        let mut nodes = BTreeMap::<String, ResolvedCard>::new();
        let mut root_response = Some(root_response);

        while let Some(item) = stack.pop() {
            let (card_ref, alias, is_root) = match item {
                Visit::Exit(key) => {
                    states.insert(key, VisitState::Resolved);
                    continue;
                }
                Visit::Enter(card_ref, alias, is_root) => (card_ref, alias, is_root),
            };
            let key = card_ref.to_string();
            register_alias(&mut aliases, &alias, &key)?;
            match states.get(&key) {
                Some(VisitState::Visiting) => {
                    return Err(graph_error(
                        "card relationship graph contains a cycle",
                        &card_ref,
                    ));
                }
                Some(VisitState::Resolved) => {
                    if let Some(node) = nodes.get_mut(&key) {
                        node.aliases.insert(alias);
                    }
                    continue;
                }
                None => {
                    states.insert(key.clone(), VisitState::Visiting);
                }
            }
            let response = if is_root {
                root_response.take().ok_or_else(|| WyrdError::Internal {
                    message: "hydration root response was consumed more than once".to_owned(),
                    details: serde_json::json!({ "card_ref": card_ref }),
                })?
            } else {
                reads::get_response(&self.engine.client, &CardSelector::exact(card_ref.clone()))
                    .await
                    .map_err(WyrdError::from)?
            };
            let resolved = reads::card_ref_from_card(&response.card).map_err(WyrdError::from)?;
            if resolved != card_ref {
                return Err(graph_error(
                    "related Card response did not match its exact reference",
                    &card_ref,
                ));
            }
            let uid = card_ref.uid.clone().ok_or_else(|| {
                graph_error(
                    "hydration requires UID-bearing relationship references",
                    &card_ref,
                )
            })?;
            let inventory = crate::reads::list_artifacts(&self.engine.client, &uid)
                .await
                .map_err(WyrdError::from)?;
            let relationships = response.card.relationships.outbound_refs.clone();
            if !response.card.relationships.outbound.is_empty() && relationships.is_empty() {
                return Err(graph_error(
                    "server returned untyped outbound relationships; graph closure is unsafe",
                    &card_ref,
                ));
            }
            let node = ResolvedCard {
                card_ref: card_ref.clone(),
                card: response.card,
                inventory,
                aliases: BTreeSet::from([alias]),
            };
            stack.push(Visit::Exit(key.clone()));
            for relationship in relationships.iter().rev() {
                let child = relationship.card_ref.clone();
                let child_alias = relationship
                    .alias
                    .clone()
                    .unwrap_or_else(|| default_alias(&child));
                validate_alias(&child_alias)?;
                let child_key = child.to_string();
                register_alias(&mut aliases, &child_alias, &child_key)?;
                match states.get(&child_key) {
                    Some(VisitState::Visiting) => {
                        return Err(graph_error(
                            "card relationship graph contains a cycle",
                            &child,
                        ));
                    }
                    Some(VisitState::Resolved) => {
                        if let Some(existing) = nodes.get_mut(&child_key) {
                            existing.aliases.insert(child_alias);
                        }
                    }
                    None => stack.push(Visit::Enter(child, child_alias, false)),
                }
            }
            nodes.insert(key, node);
        }
        Ok(nodes.into_values().collect())
    }
}

fn register_alias(
    aliases: &mut BTreeMap<String, String>,
    alias: &str,
    card_key: &str,
) -> Result<(), WyrdError> {
    validate_alias(alias)?;
    if let Some(existing) = aliases.get(alias) {
        if existing != card_key {
            return Err(WyrdError::RegistryInvalidCardSpec {
                message: "relationship aliases identify conflicting Cards".to_owned(),
                details: serde_json::json!({ "alias": alias }),
            });
        }
    } else {
        aliases.insert(alias.to_owned(), card_key.to_owned());
    }
    Ok(())
}

fn validate_alias(alias: &str) -> Result<(), WyrdError> {
    if alias.is_empty()
        || alias == "."
        || alias == ".."
        || alias.contains('/')
        || alias.contains('\\')
        || alias.bytes().any(
            |byte| !matches!(byte, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'.' | b'_' | b'-'),
        )
    {
        return Err(WyrdError::RegistryInvalidCardSpec {
            message: "relationship alias is not a safe bundle path".to_owned(),
            details: serde_json::json!({ "alias": alias }),
        });
    }
    Ok(())
}

fn default_alias(card_ref: &CardRef) -> String {
    format!(
        "{}-{}-{}-{}",
        card_ref
            .space
            .as_ref()
            .map_or("default", |space| space.as_str()),
        card_ref.kind.wire_name(),
        card_ref.name,
        card_ref.version
    )
}

fn graph_error(message: &str, card_ref: &CardRef) -> WyrdError {
    WyrdError::RegistryInvalidCardSpec {
        message: message.to_owned(),
        details: serde_json::json!({ "card_ref": card_ref }),
    }
}

impl Cards {
    /// Materialize resolved Cards, inventories, aliases, and optional artifacts.
    ///
    /// Local manifest and directory writes are synchronous and bounded to the
    /// staged bundle. Complete hydration additionally awaits each authorized,
    /// verified artifact transfer before recording its local path in the
    /// manifest.
    ///
    /// # Errors
    /// Returns a registry error when a path or alias is unsafe, local bundle
    /// materialization fails, an artifact lacks a UID, serialization fails, or
    /// a remote artifact transfer cannot be verified.
    ///
    /// # Cancellation
    /// Cancellation during an artifact transfer can leave partial files in the
    /// staging directory. The owning `hydrate` workflow removes staging when
    /// an awaited operation returns an error; a dropped task may require later
    /// local cleanup.
    async fn write_bundle(
        &self,
        staging: &Path,
        cards: &[ResolvedCard],
        mode: HydrationMode,
        root: &CardRef,
    ) -> Result<(), WyrdError> {
        let mut manifests = Vec::with_capacity(cards.len());
        let mut downloaded = 0;
        for card in cards {
            let canonical_alias =
                card.aliases.iter().next().ok_or_else(|| {
                    graph_error("hydrated Card has no bundle alias", &card.card_ref)
                })?;
            validate_alias(canonical_alias)?;
            let card_dir = staging.join("cards").join(canonical_alias);
            fs::create_dir_all(&card_dir)
                .map_err(RegistryEngineError::from)
                .map_err(WyrdError::from)?;
            let card_path = card_dir.join("card.yaml");
            let relationships_path = card_dir.join("relationships.yaml");
            let inventory_path = card_dir.join("artifacts.yaml");
            write_yaml(&card_path, &card.card)?;
            write_yaml(&relationships_path, &card.card.relationships)?;
            let mut artifacts = Vec::with_capacity(card.inventory.artifacts.len());
            for entry in &card.inventory.artifacts {
                validate_artifact_path(entry.relative_path.as_str(), &card.card_ref)?;
                let local_path = if matches!(mode, HydrationMode::Complete) {
                    let path = card_dir
                        .join("artifacts")
                        .join(entry.relative_path.as_str());
                    if !path.starts_with(staging) {
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
                    download_artifact(
                        &self.engine.client,
                        &self.engine.storage,
                        card.card_ref.uid.as_ref().ok_or_else(|| {
                            graph_error("artifact-bearing Card is missing its UID", &card.card_ref)
                        })?,
                        &entry.relative_path,
                        &path,
                    )
                    .await
                    .map_err(WyrdError::from)?;
                    downloaded += 1;
                    Some(relative_path(staging, &path)?)
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
            write_yaml(&inventory_path, &artifacts)?;
            let aliases = card.aliases.iter().cloned().collect::<Vec<_>>();
            for alias in &aliases {
                let alias_path = staging.join("aliases").join(format!("{alias}.yaml"));
                let alias_record = serde_json::json!({
                    "alias": alias,
                    "card_ref": card.card_ref,
                    "card_path": relative_path(staging, &card_path)?,
                });
                write_yaml(&alias_path, &alias_record)?;
            }
            manifests.push(HydratedCardManifest {
                aliases,
                card_ref: card.card_ref.clone(),
                card_path: relative_path(staging, &card_path)?,
                relationships_path: relative_path(staging, &relationships_path)?,
                artifact_inventory_path: relative_path(staging, &inventory_path)?,
                artifacts,
            });
        }
        let artifact_count = manifests.iter().map(|card| card.artifacts.len()).sum();
        let manifest = HydratedBundleManifest {
            api_version: "wyrd/hydrated-bundle/v1".to_owned(),
            hydration: mode,
            root: root.clone(),
            card_count: manifests.len(),
            artifact_count,
            downloaded_artifact_count: downloaded,
            cards: manifests,
        };
        write_yaml(&staging.join("metadata.yaml"), &manifest)
    }
}

/// Serialize one bounded hydration manifest and write it below the staging tree.
///
/// This helper is synchronous because it only performs local YAML serialization
/// and small manifest/file writes. Network and storage transfers remain in the
/// async bundle workflow that calls it.
///
/// # Errors
/// Returns a registry error when the path has no parent, YAML serialization
/// fails, or the local directory/file operation fails.
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

fn relative_path(root: &Path, path: &Path) -> Result<String, WyrdError> {
    path.strip_prefix(root)
        .map_err(|_| WyrdError::RegistryInvalidCardSpec {
            message: "hydrated path escaped bundle destination".to_owned(),
            details: serde_json::json!({ "path": path, "root": root }),
        })
        .map(|path| path.to_string_lossy().replace('\\', "/"))
}

#[cfg(test)]
const FAIL_PROMOTION: u8 = 1;
#[cfg(test)]
const FAIL_RESTORE: u8 = 2;
#[cfg(test)]
const FAIL_CLEANUP: u8 = 4;
#[cfg(test)]
static PUBLISH_FAULTS: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

#[cfg(test)]
fn take_publish_fault(fault: u8) -> bool {
    use std::sync::atomic::Ordering;

    PUBLISH_FAULTS.fetch_and(!fault, Ordering::SeqCst) & fault != 0
}

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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use tempfile::TempDir;

    use super::{
        FAIL_CLEANUP, FAIL_PROMOTION, FAIL_RESTORE, HydrationMode, HydrationPublisher,
        PUBLISH_FAULTS, default_alias, register_alias, validate_alias, validate_artifact_path,
    };
    use wyrd_semver::VersionBlock;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;

    /// Configure one-shot publication faults for rollback tests.
    fn inject_publish_faults(faults: u8) {
        PUBLISH_FAULTS.store(faults, std::sync::atomic::Ordering::SeqCst);
    }

    /// Write a marker file used to distinguish old and newly published bundles.
    fn write_bundle_marker(directory: &std::path::Path, marker: &str) {
        std::fs::create_dir_all(directory).expect("bundle directory creates");
        std::fs::write(directory.join("marker"), marker).expect("bundle marker writes");
    }

    #[test]
    fn aliases_are_path_safe() {
        assert!(validate_alias("agent_one").is_ok());
        assert!(validate_alias("../escape").is_err());
        assert!(validate_alias("nested/name").is_err());
    }

    #[test]
    fn default_alias_contains_exact_identity_without_uid() {
        let card_ref = CardRef {
            kind: CardKind::Prompt,
            name: CardName::new("welcome").expect("test name is valid"),
            version: VersionBlock::parse("1.0.0").expect("test version is valid"),
            space: Some(SpaceName::new("default").expect("test space is valid")),
            uid: None,
        };
        assert_eq!(default_alias(&card_ref), "default-Prompt-welcome-1.0.0");
    }

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

    #[test]
    fn aliases_cannot_point_at_two_exact_cards() {
        let mut aliases = BTreeMap::new();
        register_alias(&mut aliases, "shared", "first").expect("first alias registers");
        assert!(register_alias(&mut aliases, "shared", "second").is_err());
        register_alias(&mut aliases, "shared", "first").expect("same alias remains valid");
    }

    /// Replacing a bundle with missing staging returns an error and preserves the old bundle.
    #[test]
    fn failed_replacement_restores_the_previous_bundle() {
        let temp = TempDir::new().expect("tempdir creates");
        let destination = temp.path().join("bundle");
        let staging = temp.path().join("missing-staging");
        write_bundle_marker(&destination, "previous");

        let error = HydrationPublisher::new(&staging, &destination)
            .publish()
            .expect_err("missing staging must fail replacement");

        assert!(
            error
                .to_string()
                .contains("local artifact materialization failed")
        );
        assert_eq!(
            std::fs::read_to_string(destination.join("marker")).expect("previous bundle reads"),
            "previous"
        );
    }

    /// Promotion and restoration failures surface as an unrecoverable publication error.
    #[test]
    fn rollback_failure_is_reported_as_unrecoverable() {
        let temp = TempDir::new().expect("tempdir creates");
        let destination = temp.path().join("bundle");
        let staging = temp.path().join("staging");
        write_bundle_marker(&destination, "previous");
        write_bundle_marker(&staging, "new");
        inject_publish_faults(FAIL_PROMOTION | FAIL_RESTORE);

        let error = HydrationPublisher::new(&staging, &destination)
            .publish()
            .expect_err("injected promotion must fail");

        assert!(
            error
                .to_string()
                .contains("hydration publication and rollback failed")
        );
        assert!(!destination.exists());
        assert!(
            temp.path()
                .read_dir()
                .expect("publication parent reads")
                .flatten()
                .any(|entry| entry.file_name().to_string_lossy().contains(".previous-"))
        );
    }

    /// A cleanup failure does not hide a bundle that was already published.
    #[test]
    fn cleanup_failure_does_not_hide_a_published_bundle() {
        let temp = TempDir::new().expect("tempdir creates");
        let destination = temp.path().join("bundle");
        let staging = temp.path().join("staging");
        write_bundle_marker(&destination, "previous");
        write_bundle_marker(&staging, "new");
        inject_publish_faults(FAIL_CLEANUP);

        HydrationPublisher::new(&staging, &destination)
            .publish()
            .expect("cleanup failure is best effort");

        assert_eq!(
            std::fs::read_to_string(destination.join("marker")).expect("new bundle reads"),
            "new"
        );
    }
}
