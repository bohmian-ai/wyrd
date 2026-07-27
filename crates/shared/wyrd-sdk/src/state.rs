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
use wyrd_spec::reference::{
    CardRef, registration_only_sibling_refs, scope_child_card_refs, unresolved_card_ref_paths,
};

/// One verified artifact payload in a local `WyrdState` bundle.
#[derive(Debug, Clone)]
pub struct HydratedArtifact {
    /// Server-declared relative path within the Card artifact directory.
    relative_path: String,
    /// Canonical absolute path to the verified local payload.
    local_path: PathBuf,
    /// Server-declared base64-encoded SHA-256 digest.
    sha256: String,
    /// Verified payload byte length.
    size_bytes: u64,
    /// Optional server-declared MIME type.
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

/// Immutable indexes for a validated hydrated Card bundle.
///
/// `HydratedStateIndex` owns the exact-reference, alias, and artifact maps
/// after `WyrdState` has completed filesystem loading and cross-card
/// validation. It indexes the Card relationships loaded from disk; it does
/// not perform the remote graph traversal owned by `wyrd-registry`.
#[derive(Debug)]
struct HydratedStateIndex {
    /// Canonical exact-reference key for the Service root.
    root_key: String,
    /// Exact root reference emitted by the hydration producer.
    root_ref: CardRef,
    /// Complete Card envelopes indexed by canonical exact-reference key.
    cards_by_ref: BTreeMap<String, Card>,
    /// Exact Card references indexed by their canonical key.
    refs_by_key: BTreeMap<String, CardRef>,
    /// Persisted aliases mapped to canonical exact-reference keys.
    aliases: BTreeMap<String, String>,
    /// Sorted aliases grouped by canonical exact-reference key.
    aliases_by_ref: BTreeMap<String, Vec<String>>,
    /// Verified artifact metadata grouped by canonical exact-reference key.
    artifacts_by_ref: BTreeMap<String, Vec<HydratedArtifact>>,
    /// Confined artifact directories grouped by canonical exact-reference key.
    artifact_dirs_by_ref: BTreeMap<String, PathBuf>,
}

impl HydratedStateIndex {
    /// Assemble validated Cards, references, aliases, and artifacts into one graph.
    ///
    /// The caller has already loaded every manifest entry and validated the
    /// graph closure. This method performs only deterministic map assembly and
    /// retains a final root-membership check before publishing the graph.
    ///
    /// # Errors
    ///
    /// Returns an invalid-state-bundle error when the validated root key is not
    /// present in the assembled Card map.
    fn assemble(
        root_ref: CardRef,
        loaded: Vec<LoadedManifestCard>,
        aliases: BTreeMap<String, String>,
    ) -> Result<Self, WyrdError> {
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
        Ok(Self {
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

    /// Resolve one persisted alias to its canonical exact-reference key.
    ///
    /// # Errors
    ///
    /// Returns `WYRD_SDK_404_UNKNOWN_ALIAS` when the alias is absent.
    fn resolve_key(&self, alias: &str) -> Result<&str, WyrdError> {
        self.aliases
            .get(alias)
            .map(String::as_str)
            .ok_or_else(|| WyrdError::SdkUnknownAlias {
                message: format!("WyrdState alias {alias:?} is not known"),
                details: json!({
                    "alias": alias,
                    "available_aliases": self.aliases.keys().cloned().collect::<Vec<_>>(),
                }),
            })
    }

    /// Resolve a canonical exact-reference key to its complete Card envelope.
    ///
    /// # Errors
    ///
    /// Returns an invalid-state-bundle error when the key is absent from the
    /// validated Card map.
    fn card_by_key(&self, key: &str) -> Result<&Card, WyrdError> {
        self.cards_by_ref.get(key).ok_or_else(|| {
            state_bundle_error(
                "validated CardRef key points to a missing Card",
                json!({ "card_ref_key": key }),
            )
        })
    }

    /// Resolve a canonical exact-reference key to its stored Card reference.
    ///
    /// # Errors
    ///
    /// Returns an invalid-state-bundle error when the key is absent from the
    /// validated reference map.
    fn card_ref_by_key(&self, key: &str) -> Result<&CardRef, WyrdError> {
        self.refs_by_key.get(key).ok_or_else(|| {
            state_bundle_error(
                "validated CardRef key points to a missing reference",
                json!({ "card_ref_key": key }),
            )
        })
    }

    /// Resolve a canonical exact-reference key to its sorted aliases.
    ///
    /// # Errors
    ///
    /// Returns an invalid-state-bundle error when the key is absent from the
    /// validated alias map.
    fn aliases_by_key(&self, key: &str) -> Result<&[String], WyrdError> {
        self.aliases_by_ref
            .get(key)
            .map(Vec::as_slice)
            .ok_or_else(|| {
                state_bundle_error(
                    "validated CardRef key points to missing aliases",
                    json!({ "card_ref_key": key }),
                )
            })
    }

    /// Resolve a canonical exact-reference key to its verified artifacts.
    ///
    /// # Errors
    ///
    /// Returns an invalid-state-bundle error when the key is absent from the
    /// validated artifact map.
    fn artifacts_by_key(&self, key: &str) -> Result<&[HydratedArtifact], WyrdError> {
        self.artifacts_by_ref
            .get(key)
            .map(Vec::as_slice)
            .ok_or_else(|| {
                state_bundle_error(
                    "validated CardRef key points to missing artifacts",
                    json!({ "card_ref_key": key }),
                )
            })
    }

    /// Resolve a canonical exact-reference key to its confined artifact directory.
    ///
    /// # Errors
    ///
    /// Returns an invalid-state-bundle error when the key is absent from the
    /// validated reference map.
    fn artifact_dir_by_key(&self, key: &str) -> Result<Option<&Path>, WyrdError> {
        if !self.refs_by_key.contains_key(key) {
            return Err(state_bundle_error(
                "validated CardRef key points to a missing reference",
                json!({ "card_ref_key": key }),
            ));
        }
        Ok(self.artifact_dirs_by_ref.get(key).map(PathBuf::as_path))
    }
}

/// A complete, local, non-executing Card graph.
#[derive(Debug, Clone)]
pub struct WyrdState {
    /// Shared immutable hydrated-state index safe to read across state clones.
    index: Arc<HydratedStateIndex>,
}

impl WyrdState {
    /// Load and validate a complete hydrated bundle without contacting Wyrd.
    ///
    /// # Errors
    ///
    /// Returns a stable SDK error when the bundle is incomplete, malformed,
    /// internally inconsistent, contains an unconfined path, or preserves a
    /// registration-only sibling reference.
    pub fn from_path(path: &Path) -> Result<Self, WyrdError> {
        let manifest = Self::read_manifest(path)?;
        Self::validate_manifest_header(&manifest, path)?;
        let index = Self::load_index(path, manifest)?;
        Ok(Self {
            index: Arc::new(index),
        })
    }

    /// Return the exact root Card reference.
    #[must_use]
    pub fn root_ref(&self) -> &CardRef {
        &self.index.root_ref
    }

    /// Return the stored root Service Card envelope.
    ///
    /// # Panics
    /// Panics only if the private graph invariant established by `from_path`
    /// is violated after construction.
    #[must_use]
    pub fn service(&self) -> &Card {
        self.index
            .card_by_key(&self.index.root_key)
            .expect("validated hydrated state index root key must identify a Card")
    }

    /// Return all persisted aliases in stable sorted order.
    pub fn aliases(&self) -> impl Iterator<Item = &str> {
        self.index.aliases.keys().map(String::as_str)
    }

    /// Resolve an alias to its stored complete Card envelope.
    ///
    /// # Errors
    ///
    /// Returns `WYRD_SDK_404_UNKNOWN_ALIAS` for an unknown alias or an
    /// invalid-state-bundle error if a validated alias index is inconsistent.
    pub fn card(&self, alias: &str) -> Result<&Card, WyrdError> {
        let key = self.index.resolve_key(alias)?;
        self.index.card_by_key(key).map_err(|_| {
            state_bundle_error(
                "validated alias points to a missing Card",
                json!({ "alias": alias, "card_ref_key": key }),
            )
        })
    }

    /// Resolve an alias to its exact Card reference.
    ///
    /// # Errors
    ///
    /// Returns `WYRD_SDK_404_UNKNOWN_ALIAS` for an unknown alias or an
    /// invalid-state-bundle error if a validated reference index is inconsistent.
    pub fn card_ref(&self, alias: &str) -> Result<&CardRef, WyrdError> {
        let key = self.index.resolve_key(alias)?;
        self.index.card_ref_by_key(key).map_err(|_| {
            state_bundle_error(
                "validated alias points to a missing Card reference",
                json!({ "alias": alias, "card_ref_key": key }),
            )
        })
    }

    /// Return all aliases registered for an exact Card reference.
    ///
    /// # Errors
    ///
    /// Returns an invalid-state-bundle error when the reference is not present
    /// in the validated graph indexes.
    pub fn aliases_for(&self, card_ref: &CardRef) -> Result<&[String], WyrdError> {
        let key = card_ref.to_string();
        self.index.aliases_by_key(&key).map_err(|_| {
            state_bundle_error(
                "requested CardRef is not a member of this WyrdState graph",
                json!({ "card_ref": card_ref }),
            )
        })
    }

    /// Return verified artifacts for an alias.
    ///
    /// # Errors
    ///
    /// Returns `WYRD_SDK_404_UNKNOWN_ALIAS` for an unknown alias or an
    /// invalid-state-bundle error if the artifact index is inconsistent.
    pub fn artifacts(&self, alias: &str) -> Result<&[HydratedArtifact], WyrdError> {
        let key = self.index.resolve_key(alias)?;
        self.artifacts_by_key(key)
    }

    /// Return the confined artifact directory for an alias, when non-empty.
    ///
    /// # Errors
    ///
    /// Returns `WYRD_SDK_404_UNKNOWN_ALIAS` for an unknown alias or an
    /// invalid-state-bundle error if the artifact index is inconsistent.
    pub fn artifact_dir(&self, alias: &str) -> Result<Option<&Path>, WyrdError> {
        let key = self.index.resolve_key(alias)?;
        self.artifact_dir_by_key(key)
    }

    /// Iterate over every loaded Card keyed by its canonical exact `CardRef`.
    #[expect(
        dead_code,
        reason = "crate-private iteration is consumed by the next SDK surface"
    )]
    pub(crate) fn cards(&self) -> impl Iterator<Item = (&str, &Card)> {
        self.index
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
        self.index
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
        self.index.card_ref_by_key(key)
    }

    /// Resolve a canonical exact `CardRef` key to its aliases.
    #[expect(
        dead_code,
        reason = "crate-private lookup is consumed by the next SDK surface"
    )]
    pub(crate) fn aliases_by_key(&self, key: &str) -> Result<&[String], WyrdError> {
        self.index.aliases_by_key(key)
    }

    /// Resolve a canonical exact `CardRef` key to its verified artifacts.
    ///
    /// # Errors
    ///
    /// Returns an invalid-state-bundle error when the key is absent from the
    /// validated artifact index.
    pub(crate) fn artifacts_by_key(&self, key: &str) -> Result<&[HydratedArtifact], WyrdError> {
        self.index.artifacts_by_key(key)
    }

    /// Resolve a canonical exact `CardRef` key to its artifact directory.
    ///
    /// # Errors
    ///
    /// Returns an invalid-state-bundle error when the key is absent from the
    /// validated graph indexes.
    pub(crate) fn artifact_dir_by_key(&self, key: &str) -> Result<Option<&Path>, WyrdError> {
        self.index.artifact_dir_by_key(key)
    }
}

impl WyrdState {
    /// Read the bundle manifest from the fixed metadata projection.
    ///
    /// # Errors
    ///
    /// Returns `WYRD_SDK_400_UNHYDRATED_ARTIFACT` when metadata is absent or
    /// not a regular file, and an invalid-state-bundle error when YAML cannot
    /// be read or decoded.
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

    /// Validate bundle-level hydration mode, counts, and Service root identity.
    ///
    /// # Errors
    ///
    /// Returns an invalid-state-bundle error for unsupported headers, count
    /// mismatches, non-Service roots, or roots without exact identity.
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

    /// Load all manifest entries, validate graph closure, and publish a graph.
    ///
    /// Loading is deliberately two-pass: all exact Card identities and aliases
    /// are collected before spec and relationship references are checked. This
    /// allows forward references while keeping the final `HydratedStateIndex` immutable.
    ///
    /// # Errors
    ///
    /// Returns an invalid-state-bundle or unhydrated-artifact error when any
    /// Card projection, reference, relationship, artifact, or root is invalid.
    fn load_index(
        bundle: &Path,
        manifest: HydratedBundleManifest,
    ) -> Result<HydratedStateIndex, WyrdError> {
        let bundle = absolute_bundle(bundle)?;
        let mut loaded = Vec::with_capacity(manifest.cards.len());
        let mut known = BTreeSet::new();
        let mut aliases = BTreeMap::new();

        for entry in &manifest.cards {
            let item = Self::load_manifest_card(&bundle, entry)?;
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
            Self::validate_spec_refs(&item.card, &known)?;
            Self::validate_relationships(&item.card, &aliases, &known)?;
        }

        Self::validate_root(&manifest.root, &loaded)?;
        HydratedStateIndex::assemble(manifest.root, loaded, aliases)
    }

    /// Load and verify one Card, relationship, alias, and artifact projection.
    ///
    /// # Errors
    ///
    /// Returns an invalid-state-bundle or unhydrated-artifact error when a
    /// projection is missing, mismatched, malformed, unconfined, or unverifiable.
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
            artifacts.push(Self::validate_artifact(bundle, artifact)?);
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
            let artifact_dir = confined_existing_dir(
                bundle,
                &relative_path(bundle, &card_dir.join("artifacts"))?,
            )?;
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

    /// Validate durable, nested, and registration-only references in one Card spec.
    ///
    /// Validation precedence is fixed: unresolved paths, registration-only
    /// siblings, missing UIDs, then missing graph targets. Sibling detection is
    /// typed and includes nested inline Agent and Prompt bodies.
    ///
    /// # Errors
    ///
    /// Returns an invalid-state-bundle error describing the first reference
    /// violation in the prescribed precedence order.
    fn validate_spec_refs(card: &Card, known: &BTreeSet<String>) -> Result<(), WyrdError> {
        if let Some(path) = unresolved_card_ref_paths(&card.spec).first() {
            return Err(state_bundle_error(
                "hydrated Card contains an unresolved path reference",
                json!({ "card_ref": card_ref_for_error(card), "path": path }),
            ));
        }
        if let Some(sibling) = registration_only_sibling_refs(&card.spec).first() {
            return Err(state_bundle_error(
                "hydrated Card contains a registration-only sibling reference",
                json!({
                    "owner": card_ref_for_error(card),
                    "card_ref": sibling,
                    "reference_form": "sibling",
                }),
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

    /// Validate every server-derived outbound relationship against graph indexes.
    ///
    /// # Errors
    ///
    /// Returns an invalid-state-bundle error when a relationship lacks a UID,
    /// points outside the graph, or disagrees with its alias projection.
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

    /// Verify one artifact's confined path, size, digest, and metadata.
    ///
    /// # Errors
    ///
    /// Returns an unhydrated-artifact error when complete payload metadata is
    /// absent, or an invalid-state-bundle error when path, filesystem, size,
    /// or digest validation fails.
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
}

/// Build a structured invalid-state-bundle error for local hydration failures.
///
/// The helper is stateless because its result depends only on the supplied
/// message and detail payload; the owning validation stage remains on
/// `WyrdState`.
fn state_bundle_error(message: impl Into<String>, details: Value) -> WyrdError {
    WyrdError::SdkInvalidStateBundle {
        message: message.into(),
        details,
    }
}

impl WyrdState {
    /// Confirm that the manifest root is an exact, loaded Service Card.
    ///
    /// # Errors
    ///
    /// Returns an invalid-state-bundle error when the root is absent, has a
    /// mismatched identity, or is not a Service Card.
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
}

/// Convert a hydrated Card envelope into its exact UID-bearing reference.
///
/// # Errors
///
/// Returns an invalid-state-bundle error when the Card lacks a pinned version,
/// resolved space, or required UID.
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

/// Canonicalize and verify the root directory used for local hydration.
///
/// # Errors
///
/// Returns an invalid-state-bundle error when the path cannot be inspected,
/// is not a directory, or cannot be canonicalized.
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

/// Join one manifest-relative path to a bundle without permitting traversal.
///
/// # Errors
///
/// Returns an invalid-state-bundle error when the relative path is empty,
/// absolute, or contains parent/root/prefix components.
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

/// Resolve a manifest file and verify its canonical target stays in the bundle.
///
/// # Errors
///
/// Returns an invalid-state-bundle error when the path is missing, not a regular
/// file, cannot be canonicalized, or escapes the bundle through a symlink.
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

/// Resolve an artifact directory and verify its canonical target stays in the bundle.
///
/// # Errors
///
/// Returns an invalid-state-bundle error when the path is missing, not a
/// directory, cannot be canonicalized, or escapes the bundle through a symlink.
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

/// Validate the filename-safe alias syntax used by hydration projections.
///
/// # Errors
///
/// Returns an invalid-state-bundle error when the alias is empty, traversal-like,
/// contains a separator, or contains a character outside the safe ASCII set.
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

/// Validate the slash-delimited relative path recorded for one artifact.
///
/// # Errors
///
/// Returns an invalid-state-bundle error when the path is empty, absolute,
/// contains separators or traversal segments, or contains unsafe characters.
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

/// Hash a verified local artifact payload using the bundle's digest encoding.
///
/// # Errors
///
/// Returns an invalid-state-bundle error when the payload cannot be opened or
/// read during streaming hashing.
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

/// Decode one hydrated YAML projection into its typed Rust contract.
///
/// # Errors
///
/// Returns an invalid-state-bundle error when the projection cannot be read or
/// does not deserialize as the requested type.
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

/// Return a UTF-8 manifest-relative path after confinement has been established.
///
/// # Errors
///
/// Returns an invalid-state-bundle error when the path is outside the bundle or
/// cannot be represented as UTF-8.
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

/// Project Card identity into the structured details of a state error.
#[must_use]
fn card_ref_for_error(card: &Card) -> Value {
    json!({ "kind": card.kind, "name": card.metadata.name, "version": card.metadata.version, "space": card.metadata.space, "uid": card.metadata.uid })
}

/// Construct the stable error for a duplicate exact Card reference.
#[must_use]
fn invalid_duplicate_ref(card_ref: &CardRef) -> WyrdError {
    state_bundle_error(
        "hydrated bundle contains a duplicate CardRef",
        json!({ "card_ref": card_ref }),
    )
}

/// Construct the stable error for two Cards claiming one alias.
#[must_use]
fn conflicting_alias(alias: &str, existing: &str, incoming: &str) -> WyrdError {
    state_bundle_error(
        "hydrated bundle contains a conflicting alias",
        json!({ "alias": alias, "existing_card_ref_key": existing, "incoming_card_ref_key": incoming }),
    )
}

/// Construct the stable error for a metadata-only or incomplete bundle.
#[must_use]
fn unhydrated_error(path: &Path, message: &str) -> WyrdError {
    WyrdError::SdkUnhydratedArtifact {
        message: message.to_owned(),
        details: json!({ "path": path }),
    }
}

#[derive(Debug)]
struct LoadedManifestCard {
    /// Canonical exact-reference key used by every graph index.
    key: String,
    /// Exact identity read from the Card envelope and manifest.
    card_ref: CardRef,
    /// Complete Card envelope loaded from disk.
    card: Card,
    /// Manifest aliases validated against their projection files.
    aliases: Vec<String>,
    /// Verified artifacts owned by this Card.
    artifacts: Vec<HydratedArtifact>,
    /// Canonical artifact directory when the Card has artifacts.
    artifact_dir: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AliasRecord {
    /// Alias represented by this projection filename.
    alias: String,
    /// Exact Card reference addressed by the alias.
    card_ref: CardRef,
    /// Relative Card projection path paired with the alias.
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
    use wyrd_spec::card::agent::{AgentRunConfigSpec, AgentSpec};
    use wyrd_spec::card::field::FieldSpec;
    use wyrd_spec::card::model::{CustomMeta, ModelInterface, ModelSignature, ModelSpec, TaskType};
    use wyrd_spec::card::service::{ServiceComponent, ServiceSpec};
    use wyrd_spec::card::workflow::{WorkflowAction, WorkflowSpec, WorkflowStep};
    use wyrd_spec::envelope::{Card, CardKind, Metadata, Relationships, Spec};
    use wyrd_spec::ids::{CardName, CardUid, SpaceName};
    use wyrd_spec::reference::{CardRef, InlineableRef, Ref};

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

        /// Add a Workflow whose inline Agent prompt uses the supplied reference form.
        fn with_nested_prompt(&mut self, prompt_ref: CardRef, sibling: bool) -> CardRef {
            let workflow_ref = test_ref(CardKind::Workflow, "workflow", 4);
            let workflow = WorkflowSpec {
                steps: vec![WorkflowStep {
                    id: "agent".to_owned(),
                    action: WorkflowAction::Agent(InlineableRef::Inline(Box::new(AgentSpec {
                        prompt: if sibling {
                            InlineableRef::Sibling {
                                sibling: prompt_ref,
                            }
                        } else {
                            InlineableRef::Ref(prompt_ref)
                        },
                        tool_names: Vec::new(),
                        run_config: AgentRunConfigSpec::default(),
                        publishes_to: Vec::new(),
                    }))),
                    depends_on: Vec::new(),
                    inputs: BTreeMap::new(),
                    condition: None,
                    timeout_seconds: None,
                    retry: None,
                    display: BTreeMap::new(),
                }],
                ..WorkflowSpec::default()
            };
            self.add_card(
                "workflow",
                card(
                    &workflow_ref,
                    CardKind::Workflow,
                    Spec::Workflow(workflow),
                    Relationships::default(),
                ),
                &[],
            );
            let root_ref = test_ref(CardKind::Service, "service", 1);
            self.rewrite_card(&root_ref, |card| {
                if let Spec::Service(spec) = &mut card.spec {
                    spec.components.push(ServiceComponent {
                        alias: "workflow".to_owned(),
                        card_ref: Ref::Ref(workflow_ref.clone()),
                        source: None,
                        config: BTreeMap::new(),
                        credential_refs: Vec::new(),
                    });
                }
            });
            self.write();
            workflow_ref
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

    /// Assert the stable error shape used when durable state contains a sibling.
    fn assert_sibling_error(
        result: Result<WyrdState, wyrd_spec::error::WyrdError>,
        expected_ref: &CardRef,
    ) {
        let error = result.expect_err("sibling reference must be rejected");
        assert_eq!(error.code(), "WYRD_SDK_400_INVALID_STATE_BUNDLE");
        let details = problem_details(&error);
        assert!(!details["owner"].is_null(), "owner detail is required");
        assert_eq!(details["card_ref"], serde_json::json!(expected_ref));
        assert_eq!(details["reference_form"], "sibling");
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

    /// Reject a registration-only sibling in a top-level Service component.
    #[test]
    fn rejects_top_level_sibling_reference() {
        let bundle = TestBundle::complete_service();
        let sibling = test_ref(CardKind::Model, "model", 2);
        bundle.rewrite_card(&test_ref(CardKind::Service, "service", 1), |card| {
            if let Spec::Service(spec) = &mut card.spec {
                spec.components[0].card_ref = Ref::Sibling {
                    sibling: sibling.clone(),
                };
            }
        });

        assert_sibling_error(WyrdState::from_path(bundle.path()), &sibling);
    }

    /// Reject a sibling in the prompt slot of a nested inline Agent.
    #[test]
    fn rejects_nested_inline_agent_prompt_sibling_reference() {
        let sibling = test_ref(CardKind::Model, "model", 2);
        let mut bundle = TestBundle::complete_service();
        bundle.with_nested_prompt(sibling.clone(), true);

        assert_sibling_error(WyrdState::from_path(bundle.path()), &sibling);
    }

    /// Load the nested inline Agent when its prompt uses a durable reference.
    #[test]
    fn loads_nested_inline_agent_prompt_durable_reference() {
        let target = test_ref(CardKind::Model, "model", 2);
        let mut bundle = TestBundle::complete_service();
        let workflow_ref = bundle.with_nested_prompt(target, false);
        let state = WyrdState::from_path(bundle.path()).expect("durable nested ref loads");

        assert_eq!(
            state.card_ref("workflow").expect("workflow resolves"),
            &workflow_ref
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
