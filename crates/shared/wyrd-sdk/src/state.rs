//! Local loading and indexing of complete hydrated Card bundles.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use base64::Engine;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use wyrd_registry::{
    HydratedArtifactManifest, HydratedBundleManifest, HydratedCardManifest, HydrationMode,
};
use wyrd_spec::envelope::{Card, Relationships};
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::CardRef;

/// One Card projected into a local `WyrdState`.
#[derive(Debug, Clone)]
pub struct StateCard {
    /// Exact server-resolved Card reference.
    pub card_ref: CardRef,
    /// All persisted aliases for this Card.
    pub aliases: Vec<String>,
}

/// A complete, local, non-executing Card graph.
#[derive(Debug, Clone)]
pub struct WyrdState {
    root: CardRef,
    cards: BTreeMap<String, StateCard>,
}

impl WyrdState {
    /// Load a complete hydrated bundle from disk without contacting a server.
    ///
    /// The loader validates the bundle manifest, every Card envelope, the
    /// relationship closure, alias uniqueness, artifact inventories, and all
    /// complete-hydration payload paths. Artifact bytes are streamed only for
    /// integrity checks; model and dataset payloads are never deserialized.
    ///
    /// # Errors
    /// Returns a structured Wyrd error when the bundle is incomplete, malformed,
    /// inconsistent, or contains a path outside its bundle directory.
    pub fn from_path(path: &Path) -> Result<Self, WyrdError> {
        let manifest: HydratedBundleManifest = read_yaml(&path.join("metadata.yaml"))?;
        validate_manifest_header(&manifest, path)?;

        let mut cards_by_ref = BTreeMap::new();
        let mut aliases = BTreeMap::new();
        for card_manifest in &manifest.cards {
            let card = load_card(path, card_manifest)?;
            let card_ref = exact_card_ref(&card, path)?;
            if card_ref != card_manifest.card_ref {
                return Err(bundle_error(
                    "hydrated Card envelope does not match its manifest CardRef",
                    path,
                ));
            }
            validate_artifacts(path, card_manifest)?;
            let key = card_ref.to_string();
            if cards_by_ref.insert(key.clone(), card_manifest).is_some() {
                return Err(bundle_error(
                    "hydrated bundle contains a duplicate CardRef",
                    path,
                ));
            }
            for alias in &card_manifest.aliases {
                validate_alias(alias, path)?;
                if aliases.insert(alias.clone(), key.clone()).is_some() {
                    return Err(bundle_error(
                        "hydrated bundle contains a conflicting alias",
                        path,
                    ));
                }
                validate_alias_record(path, alias, &card_ref, &card_manifest.card_path)?;
            }
        }

        validate_root(&manifest, &cards_by_ref, path)?;
        validate_closure(path, &manifest.cards, &cards_by_ref, &aliases)?;

        let mut indexed = BTreeMap::new();
        for card_manifest in &manifest.cards {
            let state_card = StateCard {
                card_ref: card_manifest.card_ref.clone(),
                aliases: card_manifest.aliases.clone(),
            };
            for alias in &card_manifest.aliases {
                indexed.insert(alias.clone(), state_card.clone());
            }
        }

        Ok(Self {
            root: manifest.root,
            cards: indexed,
        })
    }

    /// Return the exact root Card reference.
    #[must_use]
    pub fn root(&self) -> &CardRef {
        &self.root
    }

    /// Return all persisted aliases in stable order.
    pub fn aliases(&self) -> impl Iterator<Item = &str> {
        self.cards.keys().map(String::as_str)
    }

    /// Resolve one persisted alias to its exact Card reference.
    #[must_use]
    pub fn get(&self, alias: &str) -> Option<&StateCard> {
        self.cards.get(alias)
    }
}

fn validate_manifest_header(
    manifest: &HydratedBundleManifest,
    path: &Path,
) -> Result<(), WyrdError> {
    if manifest.api_version != "wyrd/hydrated-bundle/v1" {
        return Err(bundle_error("unsupported hydrated bundle apiVersion", path));
    }
    if manifest.hydration == HydrationMode::MetadataOnly {
        return Err(unhydrated_error(path));
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
        return Err(bundle_error(
            "hydrated bundle manifest counts are inconsistent",
            path,
        ));
    }
    Ok(())
}

fn load_card(path: &Path, manifest: &HydratedCardManifest) -> Result<Card, WyrdError> {
    let card_path = confined_path(path, &manifest.card_path)?;
    let relationships_path = confined_path(path, &manifest.relationships_path)?;
    let inventory_path = confined_path(path, &manifest.artifact_inventory_path)?;
    let card: Card = read_yaml(&card_path)?;
    let relationships: Relationships = read_yaml(&relationships_path)?;
    let inventory: Vec<HydratedArtifactManifest> = read_yaml(&inventory_path)?;
    if inventory != manifest.artifacts || relationships != card.relationships {
        return Err(bundle_error(
            "hydrated projections do not match their Card envelope",
            path,
        ));
    }
    Ok(card)
}

fn validate_artifacts(path: &Path, manifest: &HydratedCardManifest) -> Result<(), WyrdError> {
    for artifact in &manifest.artifacts {
        let Some(local_path) = &artifact.local_path else {
            return Err(bundle_error(
                "complete hydration is missing an artifact payload path",
                path,
            ));
        };
        let payload = confined_path(path, local_path)?;
        let metadata = fs::metadata(&payload)
            .map_err(|_| bundle_error("hydrated artifact payload is missing", path))?;
        let expected_size = u64::try_from(artifact.size_bytes)
            .map_err(|_| bundle_error("hydrated artifact inventory has a negative size", path))?;
        if !metadata.is_file() || metadata.len() != expected_size {
            return Err(bundle_error(
                "hydrated artifact payload size does not match its inventory",
                path,
            ));
        }
        if sha256_base64(&payload)? != artifact.sha256 {
            return Err(bundle_error(
                "hydrated artifact payload digest does not match its inventory",
                path,
            ));
        }
    }
    Ok(())
}

fn validate_root(
    manifest: &HydratedBundleManifest,
    cards: &BTreeMap<String, &HydratedCardManifest>,
    path: &Path,
) -> Result<(), WyrdError> {
    if !cards.contains_key(&manifest.root.to_string()) {
        return Err(bundle_error(
            "hydrated bundle root is not present in its Card set",
            path,
        ));
    }
    Ok(())
}

fn validate_closure(
    path: &Path,
    manifests: &[HydratedCardManifest],
    cards: &BTreeMap<String, &HydratedCardManifest>,
    aliases: &BTreeMap<String, String>,
) -> Result<(), WyrdError> {
    for manifest in manifests {
        let relationships: Relationships =
            read_yaml(&confined_path(path, &manifest.relationships_path)?)?;
        for relationship in relationships.outbound_refs {
            let key = relationship.card_ref.to_string();
            if !cards.contains_key(&key) {
                return Err(bundle_error(
                    "hydrated bundle relationship closure is incomplete",
                    path,
                ));
            }
            if let Some(alias) = relationship.alias
                && aliases.get(&alias) != Some(&key)
            {
                return Err(bundle_error(
                    "hydrated relationship alias does not resolve to its CardRef",
                    path,
                ));
            }
        }
    }
    Ok(())
}

fn validate_alias_record(
    path: &Path,
    alias: &str,
    card_ref: &CardRef,
    card_path: &str,
) -> Result<(), WyrdError> {
    let record_path = path.join("aliases").join(format!("{alias}.yaml"));
    let record: AliasRecord = read_yaml(&record_path)?;
    if record.alias != alias || record.card_ref != *card_ref || record.card_path != card_path {
        return Err(bundle_error(
            "hydrated alias record does not match its Card manifest",
            path,
        ));
    }
    Ok(())
}

fn validate_alias(alias: &str, path: &Path) -> Result<(), WyrdError> {
    if alias.is_empty()
        || alias == "."
        || alias == ".."
        || alias.contains('/')
        || alias.contains('\\')
        || alias.bytes().any(
            |byte| !matches!(byte, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'.' | b'_' | b'-'),
        )
    {
        return Err(bundle_error(
            "hydrated bundle contains an unsafe alias",
            path,
        ));
    }
    Ok(())
}

fn confined_path(root: &Path, relative: &str) -> Result<PathBuf, WyrdError> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(bundle_error("hydrated bundle path escapes its root", root));
    }
    Ok(root.join(path))
}

fn exact_card_ref(card: &Card, path: &Path) -> Result<CardRef, WyrdError> {
    let version = card
        .metadata
        .resolved_pin()
        .cloned()
        .ok_or_else(|| bundle_error("hydrated Card is missing an exact version pin", path))?;
    let space = card
        .metadata
        .space
        .clone()
        .ok_or_else(|| bundle_error("hydrated Card is missing a resolved space", path))?;
    Ok(CardRef {
        kind: card.kind.clone(),
        name: card.metadata.name.clone(),
        version,
        space: Some(space),
        uid: card.metadata.uid.clone(),
    })
}

fn sha256_base64(path: &Path) -> Result<String, WyrdError> {
    let mut file =
        File::open(path).map_err(|_| bundle_error("hydrated artifact cannot be opened", path))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 128 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| bundle_error("hydrated artifact cannot be read", path))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(base64::engine::general_purpose::STANDARD.encode(hasher.finalize()))
}

fn read_yaml<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, WyrdError> {
    let contents =
        fs::read(path).map_err(|_| bundle_error("hydrated bundle file cannot be read", path))?;
    serde_yaml::from_slice(&contents)
        .map_err(|_| bundle_error("hydrated bundle YAML is invalid", path))
}

fn bundle_error(message: &str, path: &Path) -> WyrdError {
    WyrdError::RegistryInvalidCardSpec {
        message: message.to_owned(),
        details: serde_json::json!({ "path": path }),
    }
}

fn unhydrated_error(path: &Path) -> WyrdError {
    WyrdError::SdkUnhydratedArtifact {
        message: "hydrated bundle contains metadata only and cannot be used as runtime state"
            .to_owned(),
        details: serde_json::json!({ "path": path }),
    }
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
    use super::WyrdState;
    use std::fs;
    use std::path::PathBuf;

    use wyrd_registry::{HydratedBundleManifest, HydrationMode};
    use wyrd_semver::VersionBlock;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;

    #[test]
    fn missing_bundle_manifest_is_rejected_without_network_state() {
        let error = WyrdState::from_path(std::path::Path::new("missing-wyrd-state"))
            .expect_err("missing local bundle must fail");
        assert_eq!(error.code(), "WYRD_REGISTRY_400_INVALID_CARD_SPEC");
    }

    #[test]
    fn metadata_only_bundle_returns_stable_unhydrated_error() {
        let root = CardRef {
            kind: CardKind::Data,
            name: CardName::new("dataset").expect("test name is valid"),
            version: VersionBlock::parse("1.0.0").expect("test version is valid"),
            space: Some(SpaceName::new("default").expect("test space is valid")),
            uid: None,
        };
        let manifest = HydratedBundleManifest {
            api_version: "wyrd/hydrated-bundle/v1".to_owned(),
            hydration: HydrationMode::MetadataOnly,
            root,
            cards: Vec::new(),
            card_count: 0,
            artifact_count: 0,
            downloaded_artifact_count: 0,
        };
        let path = PathBuf::from(format!("target/wyrd-sdk-state-test-{}", std::process::id()));
        fs::create_dir_all(&path).expect("test directory is writable");
        fs::write(
            path.join("metadata.yaml"),
            serde_yaml::to_string(&manifest).expect("manifest serializes"),
        )
        .expect("manifest is writable");

        let error = WyrdState::from_path(&path).expect_err("metadata-only state must fail");
        assert_eq!(error.code(), "WYRD_SDK_400_UNHYDRATED_ARTIFACT");
        fs::remove_dir_all(path).expect("test directory is removable");
    }
}
