//! Local loading and indexing of complete hydrated Card bundles.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use base64::Engine;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use wyrd_registry::{
    HydratedArtifactManifest, HydratedBundleManifest, HydratedCardManifest, HydrationMode,
};
use wyrd_spec::api_version::ApiVersion;
use wyrd_spec::envelope::{Card, CardKind, Relationships};
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::{CardRef, scope_child_card_refs, unresolved_card_ref_paths};

/// One verified artifact payload in a local `WyrdState` bundle.
#[derive(Debug, Clone)]
pub struct HydratedArtifact {
    relative_path: String,
    local_path: PathBuf,
    sha256: String,
    size_bytes: u64,
    content_type: Option<String>,
}

impl HydratedArtifact {
    /// Return the server-owned relative artifact path.
    #[must_use]
    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }

    /// Return the absolute, bundle-confined local payload path.
    #[must_use]
    pub fn local_path(&self) -> &Path {
        &self.local_path
    }

    /// Return the base64-encoded SHA-256 digest.
    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    /// Return the verified payload byte size.
    #[must_use]
    pub fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    /// Return the optional artifact content type.
    #[must_use]
    pub fn content_type(&self) -> Option<&str> {
        self.content_type.as_deref()
    }
}

#[derive(Debug)]
struct StateGraph {
    root_key: String,
    root_ref: CardRef,
    cards_by_ref: BTreeMap<String, Card>,
    refs_by_key: BTreeMap<String, CardRef>,
    aliases: BTreeMap<String, String>,
    aliases_by_ref: BTreeMap<String, Vec<String>>,
    artifacts_by_ref: BTreeMap<String, Vec<HydratedArtifact>>,
    artifact_dirs_by_ref: BTreeMap<String, PathBuf>,
}

/// A complete, local, non-executing Card graph.
#[derive(Debug, Clone)]
pub struct WyrdState {
    graph: Arc<StateGraph>,
}

impl WyrdState {
    /// Load and validate a complete hydrated bundle without contacting Wyrd.
    ///
    /// # Errors
    /// Returns a stable SDK error when the bundle is incomplete, malformed,
    /// internally inconsistent, or contains an unconfined path.
    pub fn from_path(path: &Path) -> Result<Self, WyrdError> {
        let manifest = read_manifest(path)?;
        validate_manifest_header(&manifest, path)?;
        let graph = load_graph(path, manifest)?;
        Ok(Self {
            graph: Arc::new(graph),
        })
    }

    /// Return the exact root Card reference.
    #[must_use]
    pub fn root_ref(&self) -> &CardRef {
        &self.graph.root_ref
    }

    /// Return the stored root Service Card envelope.
    ///
    /// # Panics
    /// Panics only if the private graph invariant established by `from_path`
    /// is violated after construction.
    #[must_use]
    pub fn service(&self) -> &Card {
        self.graph
            .cards_by_ref
            .get(&self.graph.root_key)
            .expect("validated state graph root key must identify a Card")
    }

    /// Return all persisted aliases in stable sorted order.
    pub fn aliases(&self) -> impl Iterator<Item = &str> {
        self.graph.aliases.keys().map(String::as_str)
    }

    /// Resolve an alias to its stored complete Card envelope.
    pub fn card(&self, alias: &str) -> Result<&Card, WyrdError> {
        let key = resolve_key(&self.graph, alias)?;
        self.graph.cards_by_ref.get(key).ok_or_else(|| {
            state_bundle_error(
                "validated alias points to a missing Card",
                json!({ "alias": alias, "card_ref_key": key }),
            )
        })
    }

    /// Resolve an alias to its exact Card reference.
    pub fn card_ref(&self, alias: &str) -> Result<&CardRef, WyrdError> {
        let key = resolve_key(&self.graph, alias)?;
        self.graph.refs_by_key.get(key).ok_or_else(|| {
            state_bundle_error(
                "validated alias points to a missing Card reference",
                json!({ "alias": alias, "card_ref_key": key }),
            )
        })
    }

    /// Return all aliases registered for an exact Card reference.
    pub fn aliases_for(&self, card_ref: &CardRef) -> Result<&[String], WyrdError> {
        let key = card_ref.to_string();
        self.graph
            .aliases_by_ref
            .get(&key)
            .map(Vec::as_slice)
            .ok_or_else(|| {
                state_bundle_error(
                    "requested CardRef is not a member of this WyrdState graph",
                    json!({ "card_ref": card_ref }),
                )
            })
    }

    /// Return verified artifacts for an alias.
    pub fn artifacts(&self, alias: &str) -> Result<&[HydratedArtifact], WyrdError> {
        let key = resolve_key(&self.graph, alias)?;
        self.artifacts_by_key(key)
    }

    /// Return the confined artifact directory for an alias, when non-empty.
    pub fn artifact_dir(&self, alias: &str) -> Result<Option<&Path>, WyrdError> {
        let key = resolve_key(&self.graph, alias)?;
        self.artifact_dir_by_key(key)
    }

    /// Iterate over every loaded Card keyed by its canonical exact `CardRef`.
    #[expect(
        dead_code,
        reason = "crate-private iteration is consumed by the next SDK surface"
    )]
    pub(crate) fn cards(&self) -> impl Iterator<Item = (&str, &Card)> {
        self.graph
            .cards_by_ref
            .iter()
            .map(|(key, card)| (key.as_str(), card))
    }

    /// Iterate over loaded Cards of one kind keyed by exact `CardRef`.
    #[expect(
        dead_code,
        reason = "crate-private iteration is consumed by the next SDK surface"
    )]
    pub(crate) fn cards_of_kind(&self, kind: CardKind) -> impl Iterator<Item = (&str, &Card)> {
        self.graph
            .cards_by_ref
            .iter()
            .filter(move |(_, card)| card.kind == kind)
            .map(|(key, card)| (key.as_str(), card))
    }

    /// Resolve a canonical exact `CardRef` key to its stored reference.
    #[expect(
        dead_code,
        reason = "crate-private lookup is consumed by the next SDK surface"
    )]
    pub(crate) fn card_ref_by_key(&self, key: &str) -> Result<&CardRef, WyrdError> {
        self.graph.refs_by_key.get(key).ok_or_else(|| {
            state_bundle_error(
                "validated CardRef key points to a missing reference",
                json!({ "card_ref_key": key }),
            )
        })
    }

    /// Resolve a canonical exact `CardRef` key to its aliases.
    #[expect(
        dead_code,
        reason = "crate-private lookup is consumed by the next SDK surface"
    )]
    pub(crate) fn aliases_by_key(&self, key: &str) -> Result<&[String], WyrdError> {
        self.graph
            .aliases_by_ref
            .get(key)
            .map(Vec::as_slice)
            .ok_or_else(|| {
                state_bundle_error(
                    "validated CardRef key points to missing aliases",
                    json!({ "card_ref_key": key }),
                )
            })
    }

    /// Resolve a canonical exact `CardRef` key to its verified artifacts.
    pub(crate) fn artifacts_by_key(&self, key: &str) -> Result<&[HydratedArtifact], WyrdError> {
        self.graph
            .artifacts_by_ref
            .get(key)
            .map(Vec::as_slice)
            .ok_or_else(|| {
                state_bundle_error(
                    "validated CardRef key points to missing artifacts",
                    json!({ "card_ref_key": key }),
                )
            })
    }

    /// Resolve a canonical exact `CardRef` key to its artifact directory.
    pub(crate) fn artifact_dir_by_key(&self, key: &str) -> Result<Option<&Path>, WyrdError> {
        if !self.graph.refs_by_key.contains_key(key) {
            return Err(state_bundle_error(
                "validated CardRef key points to a missing reference",
                json!({ "card_ref_key": key }),
            ));
        }
        Ok(self
            .graph
            .artifact_dirs_by_ref
            .get(key)
            .map(PathBuf::as_path))
    }
}

fn read_manifest(bundle: &Path) -> Result<HydratedBundleManifest, WyrdError> {
    let metadata_path = bundle.join("metadata.yaml");
    let metadata = fs::metadata(&metadata_path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            unhydrated_error(&metadata_path, "hydrated bundle metadata.yaml is absent")
        } else {
            state_bundle_error(
                "hydrated bundle metadata.yaml cannot be read",
                json!({ "path": metadata_path, "source": error.to_string() }),
            )
        }
    })?;
    if !metadata.is_file() {
        return Err(unhydrated_error(
            &metadata_path,
            "hydrated bundle metadata.yaml is not a regular file",
        ));
    }
    read_yaml(&metadata_path)
}

fn validate_manifest_header(
    manifest: &HydratedBundleManifest,
    path: &Path,
) -> Result<(), WyrdError> {
    if manifest.api_version != "wyrd/hydrated-bundle/v1" {
        return Err(state_bundle_error(
            "unsupported hydrated bundle apiVersion",
            json!({ "path": path, "api_version": manifest.api_version }),
        ));
    }
    if manifest.hydration == HydrationMode::MetadataOnly {
        return Err(unhydrated_error(
            path,
            "hydrated bundle contains metadata only and cannot be used as runtime state",
        ));
    }
    if manifest.cards.is_empty()
        || manifest.card_count != manifest.cards.len()
        || manifest.artifact_count
            != manifest
                .cards
                .iter()
                .map(|card| card.artifacts.len())
                .sum::<usize>()
        || manifest.downloaded_artifact_count != manifest.artifact_count
    {
        return Err(state_bundle_error(
            "hydrated bundle manifest counts are inconsistent",
            json!({
                "path": path,
                "card_count": manifest.card_count,
                "actual_card_count": manifest.cards.len(),
                "artifact_count": manifest.artifact_count,
                "downloaded_artifact_count": manifest.downloaded_artifact_count,
            }),
        ));
    }
    if manifest.root.kind != CardKind::Service {
        return Err(state_bundle_error(
            "hydrated bundle root must be a Service Card",
            json!({ "root": manifest.root }),
        ));
    }
    if manifest.root.space.is_none() || manifest.root.uid.is_none() {
        return Err(state_bundle_error(
            "hydrated bundle root must have an exact space and UID",
            json!({ "root": manifest.root }),
        ));
    }
    Ok(())
}

fn load_graph(bundle: &Path, manifest: HydratedBundleManifest) -> Result<StateGraph, WyrdError> {
    let bundle = absolute_bundle(bundle)?;
    let mut loaded = Vec::with_capacity(manifest.cards.len());
    let mut known = BTreeSet::new();
    let mut aliases = BTreeMap::new();

    for entry in &manifest.cards {
        let item = load_manifest_card(&bundle, entry)?;
        if !known.insert(item.key.clone()) {
            return Err(invalid_duplicate_ref(&item.card_ref));
        }
        for alias in &item.aliases {
            if let Some(existing) = aliases.insert(alias.clone(), item.key.clone()) {
                return Err(conflicting_alias(alias, &existing, &item.key));
            }
        }
        loaded.push(item);
    }

    for item in &loaded {
        validate_spec_refs(&item.card, &known)?;
        validate_relationships(&item.card, &aliases, &known)?;
    }

    validate_root(&manifest.root, &loaded)?;
    assemble_graph(manifest.root, loaded, aliases)
}

fn load_manifest_card(
    bundle: &Path,
    entry: &HydratedCardManifest,
) -> Result<LoadedManifestCard, WyrdError> {
    let card_path = confined_existing_file(bundle, &entry.card_path)?;
    let relationships_path = confined_existing_file(bundle, &entry.relationships_path)?;
    let inventory_path = confined_existing_file(bundle, &entry.artifact_inventory_path)?;
    let card: Card = read_yaml(&card_path)?;
    if card.api_version.as_str() != ApiVersion::V1 {
        return Err(state_bundle_error(
            "hydrated Card does not use apiVersion wyrd/v1",
            json!({ "path": card_path, "api_version": card.api_version }),
        ));
    }
    let relationships: Relationships = read_yaml(&relationships_path)?;
    let inventory: Vec<HydratedArtifactManifest> = read_yaml(&inventory_path)?;
    if relationships != card.relationships {
        return Err(state_bundle_error(
            "hydrated relationship projection does not match its Card envelope",
            json!({ "card_path": card_path, "relationships_path": relationships_path }),
        ));
    }
    if inventory != entry.artifacts {
        return Err(state_bundle_error(
            "hydrated artifact inventory projection does not match its manifest",
            json!({ "card_path": card_path, "inventory_path": inventory_path }),
        ));
    }
    let card_ref = exact_card_ref(&card, &card_path)?;
    if card_ref != entry.card_ref {
        return Err(state_bundle_error(
            "hydrated Card envelope does not match its manifest CardRef",
            json!({ "card_path": card_path, "manifest_card_ref": entry.card_ref, "actual_card_ref": card_ref }),
        ));
    }
    if card_ref.uid.is_none() {
        return Err(state_bundle_error(
            "hydrated Card is missing its required UID",
            json!({ "card_ref": card_ref, "card_path": card_path }),
        ));
    }

    let mut artifacts = Vec::with_capacity(entry.artifacts.len());
    for artifact in &entry.artifacts {
        artifacts.push(validate_artifact(bundle, artifact)?);
    }
    let artifact_dir = if artifacts.is_empty() {
        None
    } else {
        let card_dir = card_path.parent().ok_or_else(|| {
            state_bundle_error(
                "hydrated Card path has no parent directory",
                json!({ "path": card_path }),
            )
        })?;
        let artifact_dir =
            confined_existing_dir(bundle, &relative_path(bundle, &card_dir.join("artifacts"))?)?;
        for artifact in &artifacts {
            let expected = artifact_dir.join(&artifact.relative_path);
            if artifact.local_path != expected {
                return Err(state_bundle_error(
                    "hydrated artifact payload is outside its Card artifact directory",
                    json!({ "relative_path": artifact.relative_path, "local_path": artifact.local_path, "artifact_dir": artifact_dir }),
                ));
            }
        }
        Some(artifact_dir)
    };

    for alias in &entry.aliases {
        validate_alias(alias)?;
        let alias_path = confined_existing_file(bundle, &format!("aliases/{alias}.yaml"))?;
        let record: AliasRecord = read_yaml(&alias_path)?;
        if record.alias != *alias
            || record.card_ref != card_ref
            || record.card_path != entry.card_path
        {
            return Err(state_bundle_error(
                "hydrated alias projection does not match its Card manifest",
                json!({ "alias": alias, "alias_path": alias_path, "card_ref": card_ref }),
            ));
        }
    }

    Ok(LoadedManifestCard {
        key: card_ref.to_string(),
        card_ref,
        card,
        aliases: entry.aliases.clone(),
        artifacts,
        artifact_dir,
    })
}

fn validate_spec_refs(card: &Card, known: &BTreeSet<String>) -> Result<(), WyrdError> {
    if let Some(path) = unresolved_card_ref_paths(&card.spec).first() {
        return Err(state_bundle_error(
            "hydrated Card contains an unresolved path reference",
            json!({ "card_ref": card_ref_for_error(card), "path": path }),
        ));
    }
    for card_ref in scope_child_card_refs(&card.spec) {
        if card_ref.uid.is_none() {
            return Err(state_bundle_error(
                "hydrated Card reference is missing its required UID",
                json!({ "card_ref": card_ref, "owner": card_ref_for_error(card) }),
            ));
        }
        let key = card_ref.to_string();
        if !known.contains(&key) {
            return Err(state_bundle_error(
                "hydrated Card spec reference is absent from the graph",
                json!({ "card_ref": card_ref, "owner": card_ref_for_error(card) }),
            ));
        }
    }
    Ok(())
}

fn validate_relationships(
    card: &Card,
    aliases: &BTreeMap<String, String>,
    known: &BTreeSet<String>,
) -> Result<(), WyrdError> {
    for relationship in &card.relationships.outbound_refs {
        let card_ref = &relationship.card_ref;
        if card_ref.uid.is_none() {
            return Err(state_bundle_error(
                "hydrated relationship is missing its required UID",
                json!({ "owner": card_ref_for_error(card), "card_ref": card_ref }),
            ));
        }
        let key = card_ref.to_string();
        if !known.contains(&key) {
            return Err(state_bundle_error(
                "hydrated relationship target is absent from the graph",
                json!({ "owner": card_ref_for_error(card), "card_ref": card_ref }),
            ));
        }
        if let Some(alias) = &relationship.alias
            && aliases.get(alias) != Some(&key)
        {
            return Err(state_bundle_error(
                "hydrated relationship alias does not resolve to its CardRef",
                json!({ "owner": card_ref_for_error(card), "alias": alias, "card_ref": card_ref }),
            ));
        }
    }
    Ok(())
}

fn validate_artifact(
    bundle: &Path,
    entry: &HydratedArtifactManifest,
) -> Result<HydratedArtifact, WyrdError> {
    validate_artifact_path(&entry.relative_path)?;
    let local_path = entry.local_path.as_deref().ok_or_else(|| {
        unhydrated_error(
            bundle,
            "complete hydration is missing an artifact payload path",
        )
    })?;
    let payload = confined_existing_file(bundle, local_path)?;
    let metadata = fs::metadata(&payload).map_err(|error| {
        state_bundle_error(
            "hydrated artifact payload cannot be read",
            json!({ "path": payload, "source": error.to_string() }),
        )
    })?;
    let expected_size = u64::try_from(entry.size_bytes).map_err(|_| {
        state_bundle_error(
            "hydrated artifact inventory has a negative size",
            json!({ "path": payload, "size_bytes": entry.size_bytes }),
        )
    })?;
    if metadata.len() != expected_size {
        return Err(state_bundle_error(
            "hydrated artifact payload size does not match its inventory",
            json!({ "path": payload, "expected_size": expected_size, "actual_size": metadata.len() }),
        ));
    }
    let digest = sha256_base64(&payload)?;
    if digest != entry.sha256 {
        return Err(state_bundle_error(
            "hydrated artifact payload digest does not match its inventory",
            json!({ "path": payload, "expected_sha256": entry.sha256, "actual_sha256": digest }),
        ));
    }
    Ok(HydratedArtifact {
        relative_path: entry.relative_path.clone(),
        local_path: payload,
        sha256: entry.sha256.clone(),
        size_bytes: expected_size,
        content_type: entry.content_type.clone(),
    })
}

fn resolve_key<'a>(graph: &'a StateGraph, alias: &str) -> Result<&'a str, WyrdError> {
    graph
        .aliases
        .get(alias)
        .map(String::as_str)
        .ok_or_else(|| WyrdError::SdkUnknownAlias {
            message: format!("WyrdState alias {alias:?} is not known"),
            details: json!({
                "alias": alias,
                "available_aliases": graph.aliases.keys().cloned().collect::<Vec<_>>(),
            }),
        })
}

fn state_bundle_error(message: impl Into<String>, details: Value) -> WyrdError {
    WyrdError::SdkInvalidStateBundle {
        message: message.into(),
        details,
    }
}

fn validate_root(root: &CardRef, loaded: &[LoadedManifestCard]) -> Result<(), WyrdError> {
    let Some(item) = loaded.iter().find(|item| item.key == root.to_string()) else {
        return Err(state_bundle_error(
            "hydrated bundle root is not present in its Card set",
            json!({ "root": root }),
        ));
    };
    if item.card_ref != *root || item.card.kind != CardKind::Service {
        return Err(state_bundle_error(
            "hydrated bundle root identity or kind does not match its manifest",
            json!({ "root": root, "actual": item.card_ref, "actual_kind": item.card.kind }),
        ));
    }
    Ok(())
}

fn assemble_graph(
    root_ref: CardRef,
    loaded: Vec<LoadedManifestCard>,
    aliases: BTreeMap<String, String>,
) -> Result<StateGraph, WyrdError> {
    let root_key = root_ref.to_string();
    let mut cards_by_ref = BTreeMap::new();
    let mut refs_by_key = BTreeMap::new();
    let mut aliases_by_ref = BTreeMap::<String, Vec<String>>::new();
    let mut artifacts_by_ref = BTreeMap::new();
    let mut artifact_dirs_by_ref = BTreeMap::new();

    for item in loaded {
        let key = item.key;
        refs_by_key.insert(key.clone(), item.card_ref);
        cards_by_ref.insert(key.clone(), item.card);
        let mut item_aliases = item.aliases;
        item_aliases.sort();
        aliases_by_ref.insert(key.clone(), item_aliases);
        artifacts_by_ref.insert(key.clone(), item.artifacts);
        if let Some(artifact_dir) = item.artifact_dir {
            artifact_dirs_by_ref.insert(key, artifact_dir);
        }
    }
    if !cards_by_ref.contains_key(&root_key) {
        return Err(state_bundle_error(
            "validated root key points to a missing Card",
            json!({ "root": root_ref }),
        ));
    }
    Ok(StateGraph {
        root_key,
        root_ref,
        cards_by_ref,
        refs_by_key,
        aliases,
        aliases_by_ref,
        artifacts_by_ref,
        artifact_dirs_by_ref,
    })
}

fn exact_card_ref(card: &Card, path: &Path) -> Result<CardRef, WyrdError> {
    let version = card.metadata.resolved_pin().cloned().ok_or_else(|| {
        state_bundle_error(
            "hydrated Card is missing an exact version pin",
            json!({ "path": path }),
        )
    })?;
    let space = card.metadata.space.clone().ok_or_else(|| {
        state_bundle_error(
            "hydrated Card is missing a resolved space",
            json!({ "path": path }),
        )
    })?;
    let uid = card.metadata.uid.clone().ok_or_else(|| {
        state_bundle_error(
            "hydrated Card is missing its required UID",
            json!({ "path": path }),
        )
    })?;
    Ok(CardRef {
        kind: card.kind.clone(),
        name: card.metadata.name.clone(),
        version,
        space: Some(space),
        uid: Some(uid),
    })
}

fn absolute_bundle(bundle: &Path) -> Result<PathBuf, WyrdError> {
    let metadata = fs::symlink_metadata(bundle).map_err(|error| {
        state_bundle_error(
            "hydrated bundle directory cannot be read",
            json!({ "path": bundle, "source": error.to_string() }),
        )
    })?;
    if !metadata.is_dir() {
        return Err(state_bundle_error(
            "hydrated bundle path is not a directory",
            json!({ "path": bundle }),
        ));
    }
    fs::canonicalize(bundle).map_err(|error| {
        state_bundle_error(
            "hydrated bundle directory cannot be canonicalized",
            json!({ "path": bundle, "source": error.to_string() }),
        )
    })
}

fn confined_path(root: &Path, relative: &str) -> Result<PathBuf, WyrdError> {
    if relative.is_empty() {
        return Err(state_bundle_error(
            "hydrated bundle path is empty",
            json!({ "path": relative }),
        ));
    }
    let path = Path::new(relative);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(state_bundle_error(
            "hydrated bundle path escapes its root",
            json!({ "path": relative, "root": root }),
        ));
    }
    Ok(root.join(path))
}

fn confined_existing_file(root: &Path, relative: &str) -> Result<PathBuf, WyrdError> {
    let root = absolute_bundle(root)?;
    let path = confined_path(&root, relative)?;
    let metadata = fs::symlink_metadata(&path).map_err(|error| {
        state_bundle_error(
            "hydrated bundle file cannot be read",
            json!({ "path": path, "source": error.to_string() }),
        )
    })?;
    if !metadata.is_file() {
        return Err(state_bundle_error(
            "hydrated bundle projection is not a regular file",
            json!({ "path": path }),
        ));
    }
    let canonical = fs::canonicalize(&path).map_err(|error| {
        state_bundle_error(
            "hydrated bundle file cannot be canonicalized",
            json!({ "path": path, "source": error.to_string() }),
        )
    })?;
    if !canonical.starts_with(&root) {
        return Err(state_bundle_error(
            "hydrated bundle path escapes its root",
            json!({ "path": path, "root": root }),
        ));
    }
    Ok(canonical)
}

fn confined_existing_dir(root: &Path, relative: &str) -> Result<PathBuf, WyrdError> {
    let root = absolute_bundle(root)?;
    let path = confined_path(&root, relative)?;
    let metadata = fs::symlink_metadata(&path).map_err(|error| {
        state_bundle_error(
            "hydrated artifact directory cannot be read",
            json!({ "path": path, "source": error.to_string() }),
        )
    })?;
    if !metadata.is_dir() {
        return Err(state_bundle_error(
            "hydrated artifact directory is not a directory",
            json!({ "path": path }),
        ));
    }
    let canonical = fs::canonicalize(&path).map_err(|error| {
        state_bundle_error(
            "hydrated artifact directory cannot be canonicalized",
            json!({ "path": path, "source": error.to_string() }),
        )
    })?;
    if !canonical.starts_with(&root) {
        return Err(state_bundle_error(
            "hydrated artifact directory escapes its root",
            json!({ "path": path, "root": root }),
        ));
    }
    Ok(canonical)
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
        return Err(state_bundle_error(
            "hydrated bundle contains an unsafe alias",
            json!({ "alias": alias }),
        ));
    }
    Ok(())
}

fn validate_artifact_path(path: &str) -> Result<(), WyrdError> {
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
        return Err(state_bundle_error(
            "artifact path is not confined to the hydrated bundle",
            json!({ "relative_path": path }),
        ));
    }
    Ok(())
}

fn sha256_base64(path: &Path) -> Result<String, WyrdError> {
    let mut file = File::open(path).map_err(|error| {
        state_bundle_error(
            "hydrated artifact cannot be opened",
            json!({ "path": path, "source": error.to_string() }),
        )
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 128 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| {
            state_bundle_error(
                "hydrated artifact cannot be read",
                json!({ "path": path, "source": error.to_string() }),
            )
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(base64::engine::general_purpose::STANDARD.encode(hasher.finalize()))
}

fn read_yaml<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, WyrdError> {
    let contents = fs::read(path).map_err(|error| {
        state_bundle_error(
            "hydrated bundle file cannot be read",
            json!({ "path": path, "source": error.to_string() }),
        )
    })?;
    serde_yaml::from_slice(&contents).map_err(|error| {
        state_bundle_error(
            "hydrated bundle YAML is invalid",
            json!({ "path": path, "source": error.to_string() }),
        )
    })
}

fn relative_path(root: &Path, path: &Path) -> Result<String, WyrdError> {
    path.strip_prefix(root)
        .map_err(|_| {
            state_bundle_error(
                "hydrated path is outside its bundle",
                json!({ "path": path, "root": root }),
            )
        })
        .and_then(|relative| {
            relative.to_str().map(str::to_owned).ok_or_else(|| {
                state_bundle_error("hydrated path is not valid UTF-8", json!({ "path": path }))
            })
        })
}

fn card_ref_for_error(card: &Card) -> Value {
    json!({ "kind": card.kind, "name": card.metadata.name, "version": card.metadata.version, "space": card.metadata.space, "uid": card.metadata.uid })
}

fn invalid_duplicate_ref(card_ref: &CardRef) -> WyrdError {
    state_bundle_error(
        "hydrated bundle contains a duplicate CardRef",
        json!({ "card_ref": card_ref }),
    )
}

fn conflicting_alias(alias: &str, existing: &str, incoming: &str) -> WyrdError {
    state_bundle_error(
        "hydrated bundle contains a conflicting alias",
        json!({ "alias": alias, "existing_card_ref_key": existing, "incoming_card_ref_key": incoming }),
    )
}

fn unhydrated_error(path: &Path, message: &str) -> WyrdError {
    WyrdError::SdkUnhydratedArtifact {
        message: message.to_owned(),
        details: json!({ "path": path }),
    }
}

#[derive(Debug)]
struct LoadedManifestCard {
    key: String,
    card_ref: CardRef,
    card: Card,
    aliases: Vec<String>,
    artifacts: Vec<HydratedArtifact>,
    artifact_dir: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AliasRecord {
    alias: String,
    card_ref: CardRef,
    card_path: String,
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};

    use base64::Engine;
    use serde::Serialize;
    use serde_json::Value;
    use sha2::{Digest, Sha256};
    use tempfile::{TempDir, tempdir};
    use wyrd_registry::{
        HydratedArtifactManifest, HydratedBundleManifest, HydratedCardManifest, HydrationMode,
    };
    use wyrd_semver::{VersionBlock, VersionSpec};
    use wyrd_spec::api_version::ApiVersion;
    use wyrd_spec::card::field::FieldSpec;
    use wyrd_spec::card::model::{CustomMeta, ModelInterface, ModelSignature, ModelSpec, TaskType};
    use wyrd_spec::card::service::{ServiceComponent, ServiceSpec};
    use wyrd_spec::envelope::{Card, CardKind, Metadata, Relationships, Spec};
    use wyrd_spec::ids::{CardName, CardUid, SpaceName};
    use wyrd_spec::reference::{CardRef, Ref};

    use super::WyrdState;

    #[derive(Clone)]
    struct TestArtifact {
        relative_path: String,
        bytes: Vec<u8>,
        content_type: Option<String>,
    }

    struct TestBundle {
        root: TempDir,
        manifest: HydratedBundleManifest,
        cards: RefCell<BTreeMap<String, Card>>,
        artifact_bytes: RefCell<BTreeMap<(String, String), Vec<u8>>>,
    }

    impl TestBundle {
        fn complete_service() -> Self {
            let root_ref = test_ref(CardKind::Service, "service", 1);
            let model_ref = test_ref(CardKind::Model, "model", 2);
            let backup_ref = test_ref(CardKind::Model, "backup", 3);
            let relationships = Relationships {
                outbound: Vec::new(),
                outbound_refs: vec![
                    wyrd_spec::envelope::CardRelationship {
                        card_ref: model_ref.clone(),
                        alias: Some("model".to_owned()),
                    },
                    wyrd_spec::envelope::CardRelationship {
                        card_ref: backup_ref.clone(),
                        alias: Some("backup".to_owned()),
                    },
                ],
                inbound: Vec::new(),
                inbound_refs: Vec::new(),
            };
            let service_spec = ServiceSpec {
                components: vec![
                    ServiceComponent {
                        alias: "model".to_owned(),
                        card_ref: Ref::Ref(model_ref.clone()),
                        source: None,
                        config: BTreeMap::new(),
                        credential_refs: Vec::new(),
                    },
                    ServiceComponent {
                        alias: "backup".to_owned(),
                        card_ref: Ref::Ref(backup_ref.clone()),
                        source: None,
                        config: BTreeMap::new(),
                        credential_refs: Vec::new(),
                    },
                ],
                ..ServiceSpec::default()
            };
            let mut bundle = Self {
                root: tempdir().expect("test bundle tempdir is available"),
                manifest: HydratedBundleManifest {
                    api_version: "wyrd/hydrated-bundle/v1".to_owned(),
                    hydration: HydrationMode::Complete,
                    root: root_ref.clone(),
                    cards: Vec::new(),
                    card_count: 0,
                    artifact_count: 0,
                    downloaded_artifact_count: 0,
                },
                cards: RefCell::new(BTreeMap::new()),
                artifact_bytes: RefCell::new(BTreeMap::new()),
            };
            bundle.add_card(
                "root",
                card(
                    &root_ref,
                    CardKind::Service,
                    Spec::Service(service_spec),
                    relationships,
                ),
                &[],
            );
            bundle.add_card(
                "model",
                card(
                    &model_ref,
                    CardKind::Model,
                    Spec::Model(model_spec()),
                    Relationships::default(),
                ),
                &[],
            );
            bundle.add_card(
                "backup",
                card(
                    &backup_ref,
                    CardKind::Model,
                    Spec::Model(model_spec()),
                    Relationships::default(),
                ),
                &[],
            );
            bundle.manifest.card_count = bundle.manifest.cards.len();
            bundle.manifest.artifact_count = 0;
            bundle.manifest.downloaded_artifact_count = 0;
            bundle.write();
            bundle
        }

        fn with_artifact() -> Self {
            let mut bundle = Self::complete_service();
            let model_ref = test_ref(CardKind::Model, "model", 2);
            let artifact = TestArtifact {
                relative_path: "weights.bin".to_owned(),
                bytes: b"weights".to_vec(),
                content_type: Some("application/octet-stream".to_owned()),
            };
            let entry = bundle
                .manifest
                .cards
                .iter_mut()
                .find(|entry| entry.card_ref == model_ref)
                .expect("model fixture exists");
            bundle.artifact_bytes.borrow_mut().insert(
                (model_ref.to_string(), artifact.relative_path.clone()),
                artifact.bytes.clone(),
            );
            entry.artifacts.push(HydratedArtifactManifest {
                relative_path: artifact.relative_path.clone(),
                sha256: digest(&artifact.bytes),
                size_bytes: i64::try_from(artifact.bytes.len()).expect("fixture size fits"),
                content_type: artifact.content_type,
                local_path: Some("cards/model/artifacts/weights.bin".to_owned()),
            });
            bundle.manifest.card_count = bundle.manifest.cards.len();
            bundle.manifest.artifact_count = 1;
            bundle.manifest.downloaded_artifact_count = 1;
            bundle.write();
            bundle
        }

        fn add_card(&mut self, alias: &str, card: Card, artifacts: &[TestArtifact]) -> CardRef {
            let card_ref = card_ref_from_card(&card);
            let key = card_ref.to_string();
            self.cards.borrow_mut().insert(key.clone(), card);
            let entries = artifacts
                .iter()
                .map(|artifact| {
                    self.artifact_bytes.borrow_mut().insert(
                        (key.clone(), artifact.relative_path.clone()),
                        artifact.bytes.clone(),
                    );
                    HydratedArtifactManifest {
                        relative_path: artifact.relative_path.clone(),
                        sha256: digest(&artifact.bytes),
                        size_bytes: i64::try_from(artifact.bytes.len()).expect("fixture size fits"),
                        content_type: artifact.content_type.clone(),
                        local_path: Some(format!(
                            "cards/{alias}/artifacts/{}",
                            artifact.relative_path
                        )),
                    }
                })
                .collect::<Vec<_>>();
            self.manifest.cards.push(HydratedCardManifest {
                aliases: vec![alias.to_owned()],
                card_ref: card_ref.clone(),
                card_path: format!("cards/{alias}/card.yaml"),
                relationships_path: format!("cards/{alias}/relationships.yaml"),
                artifact_inventory_path: format!("cards/{alias}/artifacts.yaml"),
                artifacts: entries,
            });
            card_ref
        }

        fn add_alias(&mut self, alias: &str, target: &CardRef) {
            let entry = self
                .manifest
                .cards
                .iter_mut()
                .find(|entry| entry.card_ref == *target)
                .expect("fixture target exists");
            entry.aliases.push(alias.to_owned());
        }

        fn write(&self) {
            fs::create_dir_all(self.root.path().join("aliases"))
                .expect("fixture aliases dir writes");
            for entry in &self.manifest.cards {
                let card = self
                    .cards
                    .borrow()
                    .get(&entry.card_ref.to_string())
                    .cloned()
                    .expect("fixture Card exists");
                write_yaml(&self.root.path().join(&entry.card_path), &card);
                write_yaml(
                    &self.root.path().join(&entry.relationships_path),
                    &card.relationships,
                );
                write_yaml(
                    &self.root.path().join(&entry.artifact_inventory_path),
                    &entry.artifacts,
                );
                for artifact in &entry.artifacts {
                    let bytes = self
                        .artifact_bytes
                        .borrow()
                        .get(&(entry.card_ref.to_string(), artifact.relative_path.clone()))
                        .cloned()
                        .expect("fixture artifact bytes exist");
                    let path = self.root.path().join(
                        artifact
                            .local_path
                            .as_ref()
                            .expect("complete fixture artifact path"),
                    );
                    fs::create_dir_all(path.parent().expect("fixture artifact parent writes"))
                        .expect("fixture artifact parent exists");
                    fs::write(path, bytes).expect("fixture artifact writes");
                }
                for alias in &entry.aliases {
                    write_yaml(
                        &self
                            .root
                            .path()
                            .join("aliases")
                            .join(format!("{alias}.yaml")),
                        &serde_json::json!({ "alias": alias, "card_ref": entry.card_ref, "card_path": entry.card_path }),
                    );
                }
            }
            let mut manifest = self.manifest.clone();
            manifest.card_count = manifest.cards.len();
            manifest.artifact_count = manifest
                .cards
                .iter()
                .map(|entry| entry.artifacts.len())
                .sum();
            manifest.downloaded_artifact_count = manifest.artifact_count;
            write_yaml(&self.root.path().join("metadata.yaml"), &manifest);
        }

        fn path(&self) -> &Path {
            self.root.path()
        }

        fn rewrite_manifest(&mut self, f: impl FnOnce(&mut HydratedBundleManifest)) {
            f(&mut self.manifest);
            write_yaml(&self.root.path().join("metadata.yaml"), &self.manifest);
            for entry in &self.manifest.cards {
                write_yaml(
                    &self.root.path().join(&entry.artifact_inventory_path),
                    &entry.artifacts,
                );
            }
        }

        fn rewrite_card(&self, card_ref: &CardRef, f: impl FnOnce(&mut Card)) {
            let mut cards = self.cards.borrow_mut();
            let card = cards
                .get_mut(&card_ref.to_string())
                .expect("fixture Card exists");
            f(card);
            let entry = self
                .manifest
                .cards
                .iter()
                .find(|entry| entry.card_ref == *card_ref)
                .expect("fixture manifest Card exists");
            write_yaml(&self.root.path().join(&entry.card_path), card);
        }

        fn rewrite_artifact(&self, card_ref: &CardRef, relative: &str, bytes: &[u8]) {
            let entry = self
                .manifest
                .cards
                .iter()
                .find(|entry| entry.card_ref == *card_ref)
                .expect("fixture artifact Card exists");
            let artifact = entry
                .artifacts
                .iter()
                .find(|artifact| artifact.relative_path == relative)
                .expect("fixture artifact exists");
            fs::write(
                self.root
                    .path()
                    .join(artifact.local_path.as_ref().expect("fixture artifact path")),
                bytes,
            )
            .expect("fixture artifact rewrite");
        }
    }

    fn card(card_ref: &CardRef, kind: CardKind, spec: Spec, relationships: Relationships) -> Card {
        Card {
            api_version: ApiVersion::v1(),
            kind,
            metadata: Metadata {
                name: card_ref.name.clone(),
                version: Some(VersionSpec::Pin(card_ref.version.clone())),
                bump: None,
                space: card_ref.space.clone(),
                uid: card_ref.uid.clone(),
                labels: wyrd_spec::metadata::Labels::default(),
                annotations: wyrd_spec::metadata::Annotations::default(),
                spec_hash: None,
                artifact_hash: None,
                origin: None,
            },
            spec,
            relationships,
            status: None,
        }
    }

    fn model_spec() -> ModelSpec {
        ModelSpec {
            interface: ModelInterface::Custom(CustomMeta {
                framework_version: "1".to_owned(),
                model_subtype: None,
                loader_module: "fixture".to_owned(),
                loader_class: "Model".to_owned(),
                extra: BTreeMap::new(),
            }),
            task_type: TaskType::Other,
            signature: ModelSignature::new(
                vec![FieldSpec::new(
                    wyrd_spec::ids::ColumnName::new("input").expect("fixture field is valid"),
                    "float64",
                )],
                vec![FieldSpec::new(
                    wyrd_spec::ids::ColumnName::new("output").expect("fixture field is valid"),
                    "float64",
                )],
            ),
            sample_input: None,
            card_refs: Vec::new(),
            publishes_to: Vec::new(),
        }
    }

    fn card_ref_from_card(card: &Card) -> CardRef {
        CardRef {
            kind: card.kind.clone(),
            name: card.metadata.name.clone(),
            version: card
                .metadata
                .resolved_pin()
                .cloned()
                .expect("fixture version is pinned"),
            space: card.metadata.space.clone(),
            uid: card.metadata.uid.clone(),
        }
    }

    fn test_ref(kind: CardKind, name: &str, uid_suffix: u8) -> CardRef {
        let uid = format!("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b{uid_suffix:02x}");
        CardRef {
            kind,
            name: CardName::new(name).expect("fixture name is valid"),
            version: VersionBlock::parse("1.0.0").expect("fixture version is valid"),
            space: Some(SpaceName::new("default").expect("fixture space is valid")),
            uid: Some(CardUid::new(uid).expect("fixture UID is valid")),
        }
    }

    fn digest(bytes: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(Sha256::digest(bytes))
    }

    fn write_yaml<T: Serialize>(path: &Path, value: &T) {
        fs::create_dir_all(path.parent().expect("fixture YAML path has parent"))
            .expect("fixture YAML parent exists");
        fs::write(
            path,
            serde_yaml::to_string(value).expect("fixture YAML serializes"),
        )
        .expect("fixture YAML writes");
    }

    fn problem_details(error: &wyrd_spec::error::WyrdError) -> Value {
        error.as_problem_json()["details"].clone()
    }

    fn assert_error(
        result: Result<WyrdState, wyrd_spec::error::WyrdError>,
        code: &str,
        field: &str,
    ) {
        let error = result.expect_err("bundle fixture must be rejected");
        assert_eq!(error.code(), code);
        assert!(
            !problem_details(&error)[field].is_null(),
            "missing detail field {field}: {}",
            problem_details(&error)
        );
    }

    #[test]
    fn loads_complete_service_graph_without_network() {
        let bundle = TestBundle::complete_service();
        let state = WyrdState::from_path(bundle.path()).expect("complete graph loads");
        assert_eq!(state.root_ref().kind, CardKind::Service);
        assert_eq!(state.service().kind, CardKind::Service);
        assert_eq!(
            state.card("model").expect("model alias resolves").kind,
            CardKind::Model
        );
    }

    #[test]
    fn aliases_for_same_card_resolve_same_address() {
        let mut bundle = TestBundle::complete_service();
        let target = test_ref(CardKind::Model, "model", 2);
        bundle.add_alias("model-alt", &target);
        bundle.write();
        let state = WyrdState::from_path(bundle.path()).expect("complete graph loads");
        assert!(std::ptr::eq(
            state.card("model").expect("model resolves"),
            state.card("model-alt").expect("alias resolves")
        ));
    }

    #[test]
    fn aliases_are_stably_sorted() {
        let mut bundle = TestBundle::complete_service();
        bundle.add_alias("aaa", &test_ref(CardKind::Model, "model", 2));
        bundle.add_alias("zzz", &test_ref(CardKind::Model, "model", 2));
        bundle.write();
        let state = WyrdState::from_path(bundle.path()).expect("complete graph loads");
        assert_eq!(
            state.aliases().collect::<Vec<_>>(),
            vec!["aaa", "backup", "model", "root", "zzz"]
        );
    }

    #[test]
    fn rejects_missing_manifest_as_unhydrated() {
        let bundle = tempdir().expect("test tempdir is available");
        assert_error(
            WyrdState::from_path(bundle.path()),
            "WYRD_SDK_400_UNHYDRATED_ARTIFACT",
            "path",
        );
    }

    #[test]
    fn rejects_metadata_only_bundle_as_unhydrated() {
        let mut bundle = TestBundle::complete_service();
        bundle.rewrite_manifest(|manifest| manifest.hydration = HydrationMode::MetadataOnly);
        assert_error(
            WyrdState::from_path(bundle.path()),
            "WYRD_SDK_400_UNHYDRATED_ARTIFACT",
            "path",
        );
    }

    #[test]
    fn rejects_inconsistent_manifest_counts() {
        let mut bundle = TestBundle::complete_service();
        bundle.rewrite_manifest(|manifest| manifest.card_count += 1);
        assert_error(
            WyrdState::from_path(bundle.path()),
            "WYRD_SDK_400_INVALID_STATE_BUNDLE",
            "card_count",
        );
    }

    #[test]
    fn rejects_non_service_root() {
        let mut bundle = TestBundle::complete_service();
        bundle.rewrite_manifest(|manifest| manifest.root.kind = CardKind::Model);
        assert_error(
            WyrdState::from_path(bundle.path()),
            "WYRD_SDK_400_INVALID_STATE_BUNDLE",
            "root",
        );
    }

    #[test]
    fn rejects_root_manifest_identity_mismatch() {
        let mut bundle = TestBundle::complete_service();
        bundle.rewrite_manifest(|manifest| {
            manifest.root.name = CardName::new("other").expect("fixture name is valid");
        });
        assert_error(
            WyrdState::from_path(bundle.path()),
            "WYRD_SDK_400_INVALID_STATE_BUNDLE",
            "root",
        );
    }

    #[test]
    fn rejects_root_without_uid() {
        let mut bundle = TestBundle::complete_service();
        bundle.rewrite_manifest(|manifest| manifest.root.uid = None);
        assert_error(
            WyrdState::from_path(bundle.path()),
            "WYRD_SDK_400_INVALID_STATE_BUNDLE",
            "root",
        );
    }

    #[test]
    fn rejects_child_without_uid() {
        let bundle = TestBundle::complete_service();
        let child = test_ref(CardKind::Model, "model", 2);
        bundle.rewrite_card(&child, |card| card.metadata.uid = None);
        assert_error(
            WyrdState::from_path(bundle.path()),
            "WYRD_SDK_400_INVALID_STATE_BUNDLE",
            "path",
        );
    }

    #[test]
    fn rejects_manifest_card_identity_mismatch() {
        let mut bundle = TestBundle::complete_service();
        bundle.rewrite_manifest(|manifest| {
            manifest.cards[1].card_ref.name =
                CardName::new("other").expect("fixture name is valid");
        });
        assert_error(
            WyrdState::from_path(bundle.path()),
            "WYRD_SDK_400_INVALID_STATE_BUNDLE",
            "manifest_card_ref",
        );
    }

    #[test]
    fn rejects_duplicate_exact_card_ref() {
        let mut bundle = TestBundle::complete_service();
        let model = test_ref(CardKind::Model, "model", 2);
        let duplicate = bundle
            .cards
            .borrow()
            .get(&model.to_string())
            .cloned()
            .expect("fixture model exists");
        bundle.cards.borrow_mut().insert(
            test_ref(CardKind::Model, "backup", 3).to_string(),
            duplicate,
        );
        bundle.manifest.cards[2].card_ref = model.clone();
        bundle.manifest.cards[2].aliases = vec!["backup".to_owned()];
        bundle.write();
        assert_error(
            WyrdState::from_path(bundle.path()),
            "WYRD_SDK_400_INVALID_STATE_BUNDLE",
            "card_ref",
        );
    }

    #[test]
    fn rejects_conflicting_alias() {
        let mut bundle = TestBundle::complete_service();
        bundle.add_alias("model", &test_ref(CardKind::Model, "backup", 3));
        bundle.write();
        assert_error(
            WyrdState::from_path(bundle.path()),
            "WYRD_SDK_400_INVALID_STATE_BUNDLE",
            "alias",
        );
    }

    #[test]
    fn rejects_alias_projection_mismatch() {
        let bundle = TestBundle::complete_service();
        write_yaml(
            &bundle.path().join("aliases/model.yaml"),
            &serde_json::json!({ "alias": "model", "card_ref": test_ref(CardKind::Model, "backup", 3), "card_path": "cards/model/card.yaml" }),
        );
        assert_error(
            WyrdState::from_path(bundle.path()),
            "WYRD_SDK_400_INVALID_STATE_BUNDLE",
            "alias",
        );
    }

    #[test]
    fn rejects_relationship_projection_mismatch() {
        let bundle = TestBundle::complete_service();
        write_yaml(
            &bundle.path().join("cards/root/relationships.yaml"),
            &Relationships::default(),
        );
        assert_error(
            WyrdState::from_path(bundle.path()),
            "WYRD_SDK_400_INVALID_STATE_BUNDLE",
            "relationships_path",
        );
    }

    #[test]
    fn rejects_artifact_inventory_projection_mismatch() {
        let bundle = TestBundle::with_artifact();
        write_yaml(
            &bundle.path().join("cards/model/artifacts.yaml"),
            &Vec::<HydratedArtifactManifest>::new(),
        );
        assert_error(
            WyrdState::from_path(bundle.path()),
            "WYRD_SDK_400_INVALID_STATE_BUNDLE",
            "inventory_path",
        );
    }

    #[test]
    fn rejects_missing_relationship_target() {
        let bundle = TestBundle::complete_service();
        let child = test_ref(CardKind::Model, "model", 2);
        bundle.rewrite_card(&test_ref(CardKind::Service, "service", 1), |card| {
            card.relationships.outbound_refs[0].card_ref = CardRef {
                name: CardName::new("missing").expect("fixture name is valid"),
                ..child
            };
        });
        let root_card = bundle
            .cards
            .borrow()
            .get(&test_ref(CardKind::Service, "service", 1).to_string())
            .cloned()
            .expect("fixture root exists");
        write_yaml(
            &bundle.path().join("cards/root/relationships.yaml"),
            &root_card.relationships,
        );
        assert_error(
            WyrdState::from_path(bundle.path()),
            "WYRD_SDK_400_INVALID_STATE_BUNDLE",
            "card_ref",
        );
    }

    #[test]
    fn rejects_unresolved_nested_path_reference() {
        let bundle = TestBundle::complete_service();
        bundle.rewrite_card(&test_ref(CardKind::Service, "service", 1), |card| {
            if let Spec::Service(spec) = &mut card.spec {
                spec.components[0].card_ref = Ref::Path(PathBuf::from("nested/model.yaml"));
            }
        });
        assert_error(
            WyrdState::from_path(bundle.path()),
            "WYRD_SDK_400_INVALID_STATE_BUNDLE",
            "path",
        );
    }

    #[test]
    fn rejects_spec_ref_absent_from_graph() {
        let bundle = TestBundle::complete_service();
        bundle.rewrite_card(&test_ref(CardKind::Service, "service", 1), |card| {
            if let Spec::Service(spec) = &mut card.spec {
                spec.components[0].card_ref = Ref::Ref(test_ref(CardKind::Model, "missing", 4));
            }
        });
        assert_error(
            WyrdState::from_path(bundle.path()),
            "WYRD_SDK_400_INVALID_STATE_BUNDLE",
            "owner",
        );
    }

    #[test]
    fn rejects_escaping_manifest_path() {
        let mut bundle = TestBundle::complete_service();
        bundle.rewrite_manifest(|manifest| manifest.cards[0].card_path = "../card.yaml".to_owned());
        assert_error(
            WyrdState::from_path(bundle.path()),
            "WYRD_SDK_400_INVALID_STATE_BUNDLE",
            "path",
        );
    }

    #[test]
    fn rejects_unsafe_alias() {
        let mut bundle = TestBundle::complete_service();
        bundle
            .rewrite_manifest(|manifest| manifest.cards[1].aliases = vec!["../escape".to_owned()]);
        assert_error(
            WyrdState::from_path(bundle.path()),
            "WYRD_SDK_400_INVALID_STATE_BUNDLE",
            "alias",
        );
    }

    #[test]
    fn rejects_missing_artifact_payload() {
        let bundle = TestBundle::with_artifact();
        fs::remove_file(bundle.path().join("cards/model/artifacts/weights.bin"))
            .expect("fixture payload exists");
        assert_error(
            WyrdState::from_path(bundle.path()),
            "WYRD_SDK_400_INVALID_STATE_BUNDLE",
            "path",
        );
    }

    #[test]
    fn rejects_non_file_artifact_payload() {
        let bundle = TestBundle::with_artifact();
        fs::remove_file(bundle.path().join("cards/model/artifacts/weights.bin"))
            .expect("fixture payload exists");
        fs::create_dir(bundle.path().join("cards/model/artifacts/weights.bin"))
            .expect("fixture directory creates");
        assert_error(
            WyrdState::from_path(bundle.path()),
            "WYRD_SDK_400_INVALID_STATE_BUNDLE",
            "path",
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_artifact_escape() {
        let bundle = TestBundle::with_artifact();
        let payload = bundle.path().join("cards/model/artifacts/weights.bin");
        fs::remove_file(&payload).expect("fixture payload exists");
        std::os::unix::fs::symlink("/etc/hosts", payload).expect("fixture symlink creates");
        assert_error(
            WyrdState::from_path(bundle.path()),
            "WYRD_SDK_400_INVALID_STATE_BUNDLE",
            "path",
        );
    }

    #[cfg(not(unix))]
    #[test]
    fn rejects_symlinked_artifact_escape() {}

    #[test]
    fn rejects_artifact_size_mismatch() {
        let mut bundle = TestBundle::with_artifact();
        bundle.rewrite_manifest(|manifest| manifest.cards[1].artifacts[0].size_bytes += 1);
        assert_error(
            WyrdState::from_path(bundle.path()),
            "WYRD_SDK_400_INVALID_STATE_BUNDLE",
            "expected_size",
        );
    }

    #[test]
    fn rejects_artifact_digest_mismatch() {
        let bundle = TestBundle::with_artifact();
        bundle.rewrite_artifact(
            &test_ref(CardKind::Model, "model", 2),
            "weights.bin",
            b"corrupt",
        );
        assert_error(
            WyrdState::from_path(bundle.path()),
            "WYRD_SDK_400_INVALID_STATE_BUNDLE",
            "expected_sha256",
        );
    }

    #[test]
    fn unknown_alias_lists_available_aliases() {
        let bundle = TestBundle::complete_service();
        let state = WyrdState::from_path(bundle.path()).expect("complete graph loads");
        let error = state.card("missing").expect_err("unknown alias rejects");
        assert_eq!(error.code(), "WYRD_SDK_404_UNKNOWN_ALIAS");
        assert_eq!(
            problem_details(&error)["available_aliases"],
            serde_json::json!(["backup", "model", "root"])
        );
    }

    #[test]
    fn artifact_paths_are_absolute_confined_and_verified() {
        let bundle = TestBundle::with_artifact();
        let state = WyrdState::from_path(bundle.path()).expect("complete artifact graph loads");
        let artifact = &state.artifacts("model").expect("model artifacts resolve")[0];
        assert!(artifact.local_path().is_absolute());
        assert!(
            artifact
                .local_path()
                .starts_with(fs::canonicalize(bundle.path()).expect("fixture root canonicalizes"))
        );
        assert_eq!(artifact.size_bytes(), 7);
        assert_eq!(artifact.content_type(), Some("application/octet-stream"));
        assert_eq!(
            state
                .artifact_dir("model")
                .expect("artifact dir resolves")
                .expect("artifact dir exists"),
            artifact
                .local_path()
                .parent()
                .expect("artifact parent exists")
        );
    }
}
