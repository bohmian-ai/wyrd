//! Public `Cards` handle and client-side selector types.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use secrecy::SecretString;
use tempfile::TempDir;
use wyrd_client::WyrdClient;
use wyrd_spec::envelope::{Card, CardKind};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, CardUid, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::registry::{ListCardsRequest, ListCardsResponse};

use crate::config;
use crate::download;
use crate::engine::RegistryEngine;
use crate::error::RegistryEngineError;
use crate::reads;

/// A read-time Card lookup request.
///
/// A named selector without a version means latest Active. A UID selector is
/// exact; optional identity fields are consistency assertions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardSelector {
    /// Select by kind, space, name, and optional exact version.
    Named {
        /// Card kind.
        kind: CardKind,
        /// Card space.
        space: SpaceName,
        /// Card name.
        name: CardName,
        /// Exact version, or `None` for latest Active reads.
        version: Option<wyrd_semver::VersionBlock>,
    },
    /// Select by an exact reference without changing its optional UID.
    Exact(CardRef),
    /// Select by exact UID with optional identity assertions.
    Uid {
        /// Card kind owning the UID.
        kind: CardKind,
        /// Exact Card UID.
        uid: CardUid,
        /// Optional asserted space.
        space: Option<SpaceName>,
        /// Optional asserted name.
        name: Option<CardName>,
        /// Optional asserted exact version.
        version: Option<wyrd_semver::VersionBlock>,
    },
}

impl CardSelector {
    /// Build a named selector whose read default is latest Active.
    #[must_use]
    pub fn named(kind: CardKind, space: SpaceName, name: CardName) -> Self {
        Self::Named {
            kind,
            space,
            name,
            version: None,
        }
    }

    /// Build an exact selector from a resolved Card reference.
    #[must_use]
    pub fn exact(card_ref: CardRef) -> Self {
        Self::Exact(card_ref)
    }

    /// Build an exact UID selector.
    #[must_use]
    pub fn uid(kind: CardKind, uid: CardUid) -> Self {
        Self::Uid {
            kind,
            uid,
            space: None,
            name: None,
            version: None,
        }
    }

    /// Add an exact version assertion to a selector.
    #[must_use]
    pub fn with_version(mut self, version: wyrd_semver::VersionBlock) -> Self {
        match &mut self {
            Self::Named {
                version: selected, ..
            } => *selected = Some(version),
            Self::Exact(CardRef {
                version: selected, ..
            }) => *selected = version,
            Self::Uid {
                version: selected, ..
            } => *selected = Some(version),
        }
        self
    }

    /// Add optional identity assertions to a UID selector.
    #[must_use]
    pub fn with_identity_assertions(
        mut self,
        space: Option<SpaceName>,
        name: Option<CardName>,
    ) -> Self {
        if let Self::Uid {
            space: selected_space,
            name: selected_name,
            ..
        } = &mut self
        {
            *selected_space = space;
            *selected_name = name;
        }
        self
    }
}

/// One loaded Card envelope and its managed artifact directory, when needed.
#[derive(Debug)]
pub struct LoadedCard {
    /// Server-hydrated Card envelope.
    pub card: Card,
    /// Managed temporary directory when the caller did not provide a path.
    pub temporary_artifact_directory: Option<TempDir>,
    /// Caller-provided artifact destination, when one was used.
    pub artifact_directory: Option<PathBuf>,
}

/// Cheap-to-clone, tenant-scoped Card registry handle.
#[derive(Clone)]
pub struct Cards {
    engine: Arc<RegistryEngine>,
}

impl Cards {
    /// Construct a registry handle from global client configuration with
    /// optional explicit server URL and API-key overrides.
    ///
    /// No network or token exchange occurs during construction.
    pub fn new(server_url: Option<&str>, api_key: Option<SecretString>) -> Result<Self, WyrdError> {
        let client = config::load(server_url, api_key).map_err(WyrdError::from)?;
        Ok(Self {
            engine: RegistryEngine::new(client),
        })
    }

    /// Construct a handle around an already assembled client.
    ///
    /// This advanced seam is intended for tests and embedding. It bypasses
    /// profile resolution while retaining the same shared transport stack.
    #[must_use]
    pub fn with_client(client: WyrdClient) -> Self {
        Self {
            engine: RegistryEngine::new(client),
        }
    }

    /// Fetch one Card envelope using exact or latest selector semantics.
    pub async fn get(&self, selector: CardSelector) -> Result<Card, WyrdError> {
        reads::get(&self.engine.client, &selector)
            .await
            .map_err(Into::into)
    }

    /// List metadata-only Card summaries through the typed server query.
    pub async fn list(&self, request: ListCardsRequest) -> Result<ListCardsResponse, WyrdError> {
        reads::list(&self.engine.client, request)
            .await
            .map_err(Into::into)
    }

    /// Soft-delete one Card. Named selectors must include an exact version.
    pub async fn delete(&self, selector: CardSelector) -> Result<(), WyrdError> {
        crate::reads::delete(&self.engine.client, selector)
            .await
            .map_err(Into::into)
    }

    /// Resolve the latest Active version to an exact `CardRef`.
    pub async fn resolve_latest(
        &self,
        kind: CardKind,
        space: SpaceName,
        name: CardName,
    ) -> Result<CardRef, WyrdError> {
        reads::resolve_latest(&self.engine.client, kind, space, name)
            .await
            .map_err(Into::into)
    }

    /// Resolve one Card and materialize its server-owned artifacts.
    ///
    /// The returned envelope remains server-derived. A later language adapter
    /// can lower it into a native holder without introducing another network
    /// or storage implementation.
    pub async fn load(
        &self,
        selector: CardSelector,
        destination: Option<&Path>,
    ) -> Result<LoadedCard, WyrdError> {
        let card = self.get(selector).await?;
        let card_uid =
            card.metadata
                .uid
                .clone()
                .ok_or_else(|| WyrdError::RegistryInvalidCardSpec {
                    message: "server response did not contain a card uid".to_owned(),
                    details: serde_json::json!({}),
                })?;
        let inventory = reads::list_artifacts(&self.engine.client, &card_uid)
            .await
            .map_err(WyrdError::from)?;

        let (root, temporary_artifact_directory, artifact_directory) = match destination {
            Some(path) => {
                tokio::fs::create_dir_all(path)
                    .await
                    .map_err(RegistryEngineError::from)
                    .map_err(WyrdError::from)?;
                (path.to_path_buf(), None, Some(path.to_path_buf()))
            }
            None => {
                let temporary = tempfile::Builder::new()
                    .prefix("wyrd-card-")
                    .tempdir()
                    .map_err(RegistryEngineError::from)
                    .map_err(WyrdError::from)?;
                let root = temporary.path().to_path_buf();
                (root, Some(temporary), None)
            }
        };

        for artifact in inventory.artifacts {
            let path = root.join(artifact.relative_path.as_str());
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(RegistryEngineError::from)
                    .map_err(WyrdError::from)?;
            }
            download::download_artifact(
                &self.engine.client,
                &self.engine.storage,
                &card_uid,
                &artifact.relative_path,
                &path,
            )
            .await
            .map_err(WyrdError::from)?;
        }

        Ok(LoadedCard {
            card,
            temporary_artifact_directory,
            artifact_directory,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::CardSelector;
    use wyrd_semver::VersionBlock;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, CardUid, SpaceName};

    #[test]
    fn selector_constructors_keep_latest_and_exact_distinct() {
        let kind = CardKind::Prompt;
        let space = SpaceName::new("prod").expect("test space is valid");
        let name = CardName::new("prompt").expect("test name is valid");
        let latest = CardSelector::named(kind.clone(), space.clone(), name.clone());
        assert!(matches!(latest, CardSelector::Named { version: None, .. }));
        let exact =
            latest.with_version(VersionBlock::parse("1.0.0").expect("test version is valid"));
        assert!(matches!(
            exact,
            CardSelector::Named {
                version: Some(_),
                ..
            }
        ));
        let uid = CardUid::new("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00").expect("test uid is valid");
        assert!(matches!(
            CardSelector::uid(kind, uid),
            CardSelector::Uid { .. }
        ));
    }
}
