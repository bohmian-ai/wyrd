//! Public `Cards` handle and client-side selector types.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::WyrdClient;
use futures_util::StreamExt;
use reqwest::Method;
use secrecy::SecretString;
use tempfile::TempDir;
use wyrd_loader::RegistrationInput;
use wyrd_semver::VersionBlock;
use wyrd_spec::envelope::{Card, CardKind};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, CardUid, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::registry::{GetCardResponse, ListCardsRequest, ListCardsResponse};
use wyrd_spec::storage::{DownloadInitRequest, DownloadInitResponse};

use crate::cards::RegistrationReceipt;
use crate::cards::config;
use crate::cards::download;
use crate::cards::download_progress::DownloadProgressDisplay;
use crate::cards::engine::{RegistryContext, RegistryEngine};
use crate::cards::error::RegistryEngineError;
use crate::cards::progress::RegistrationProgressSink;
use crate::cards::reads;

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
        version: Option<VersionBlock>,
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
        version: Option<VersionBlock>,
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
    pub fn with_version(mut self, version: VersionBlock) -> Self {
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
    /// Shared registry engine that owns the authenticated transport, tenant
    /// scope, and registration/load sagas; cloning `Cards` only bumps this
    /// `Arc`, so every clone drives the same engine state.
    pub(crate) engine: Arc<RegistryEngine>,
}

impl Cards {
    /// Construct a registry handle from global client configuration with
    /// optional explicit server URL and API-key overrides.
    ///
    /// No network or token exchange occurs during construction.
    ///
    /// # Errors
    /// Returns a Wyrd error when the local configuration or credential override
    /// cannot be loaded.
    pub fn new(
        server_url: Option<&str>,
        credential: Option<SecretString>,
    ) -> Result<Self, WyrdError> {
        let client = config::load(server_url, credential).map_err(WyrdError::from)?;
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

    /// Download and verify every server-declared artifact for one Card.
    ///
    /// The caller owns the destination and its lifetime. This method owns only
    /// registry selection, server download-plan creation, bounded transfer, and
    /// integrity verification; no Card receives a registry or storage handle.
    ///
    /// # Errors
    /// Returns a Wyrd error when inventory lookup, destination preparation,
    /// transfer, or integrity verification fails.
    pub async fn download_artifacts_to(
        &self,
        card_uid: &CardUid,
        destination: &Path,
    ) -> Result<(), WyrdError> {
        ArtifactMaterializer::new(self, card_uid, destination)
            .materialize()
            .await
    }
    /// Clone the authenticated context for another focused registry capability.
    ///
    /// The returned context shares this handle's transport, authentication
    /// cache, and storage client without exposing their implementation types.
    #[must_use]
    pub fn registry_context(&self) -> RegistryContext {
        RegistryContext::new(Arc::clone(&self.engine))
    }

    /// Loads one Card document from disk and registers it through the authenticated registry.
    ///
    /// The loader resolves the document and its local source context before this method
    /// delegates the durable write to [`Self::register`].
    ///
    /// # Errors
    ///
    /// Returns an error when the document cannot be loaded or converted into a registration
    /// request, or when the registry rejects the registration.
    pub async fn register_from_path(
        &self,
        path: &Path,
    ) -> Result<RegistrationReceipt, WyrdError> {
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

    /// Register a loader-produced composite input and drive its private
    /// artifact transfer and server-owned completion lifecycle.
    ///
    /// Native language card holders lower into this input at their language
    /// adapter boundary. Local source paths are consumed only by the private
    /// saga and never enter the request body.
    ///
    /// # Errors
    /// Returns a Wyrd error when validation, registration, artifact upload, or
    /// server-owned completion fails.
    pub async fn register(
        &self,
        input: &RegistrationInput,
    ) -> Result<RegistrationReceipt, WyrdError> {
        crate::cards::saga::register(&self.engine, input, None)
            .await
            .map_err(Into::into)
    }

    /// Register a loader-produced composite input and forward typed progress
    /// events to the caller while the saga runs.
    ///
    /// The callback may be invoked concurrently for up to four active
    /// artifact transfers. Callers that update shared state must provide their
    /// own synchronization; the callback itself must be safe to call from
    /// multiple asynchronous tasks.
    ///
    /// # Errors
    /// Returns a Wyrd error when validation, registration, artifact upload, or
    /// server-owned completion fails.
    pub async fn register_with_progress(
        &self,
        input: &RegistrationInput,
        progress: RegistrationProgressSink,
    ) -> Result<RegistrationReceipt, WyrdError> {
        crate::cards::saga::register(&self.engine, input, Some(progress))
            .await
            .map_err(Into::into)
    }

    /// Fetch one Card envelope using exact or latest selector semantics.
    ///
    /// # Errors
    /// Returns a Wyrd error when the selector is invalid, the Card is absent,
    /// or the server request fails.
    pub async fn get(&self, selector: CardSelector) -> Result<Card, WyrdError> {
        reads::get(&self.engine.client, &selector)
            .await
            .map_err(Into::into)
    }

    /// Fetch one Card response with its server-derived timestamps.
    ///
    /// # Errors
    /// Returns a Wyrd error when the selector is invalid, the Card is absent,
    /// or the server request fails.
    pub async fn get_response(
        &self,
        selector: CardSelector,
    ) -> Result<GetCardResponse, WyrdError> {
        reads::get_response(&self.engine.client, &selector)
            .await
            .map_err(Into::into)
    }

    /// List metadata-only Card summaries through the typed server query.
    ///
    /// # Errors
    /// Returns a Wyrd error when the request is invalid or the server query
    /// fails.
    pub async fn list(&self, request: ListCardsRequest) -> Result<ListCardsResponse, WyrdError> {
        reads::list(&self.engine.client, request)
            .await
            .map_err(Into::into)
    }

    /// Plan one authorized artifact download through the typed storage
    /// contract.
    ///
    /// # Errors
    /// Returns a Wyrd error when the Card or artifact is absent, unauthorized,
    /// or the server cannot mint a download plan.
    pub async fn download_init(
        &self,
        request: DownloadInitRequest,
    ) -> Result<DownloadInitResponse, WyrdError> {
        self.engine
            .client
            .request_json(Method::POST, "/v1/cards/download/init", Some(&request))
            .await
    }

    /// Soft-delete one Card. Named selectors must include an exact version.
    ///
    /// # Errors
    /// Returns a Wyrd error when the selector is not exact, the Card is absent,
    /// or the server mutation fails.
    pub async fn delete(&self, selector: CardSelector) -> Result<(), WyrdError> {
        crate::cards::reads::delete(&self.engine.client, selector)
            .await
            .map_err(Into::into)
    }

    /// Resolve the latest Active version to an exact `CardRef`.
    ///
    /// # Errors
    /// Returns a Wyrd error when no Active version matches or the server query
    /// fails.
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
    ///
    /// # Errors
    /// Returns a Wyrd error when the Card, artifact inventory, destination, or
    /// artifact transfer cannot be loaded.
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
        let temporary_directory = if destination.is_none() {
            Some(
                tempfile::Builder::new()
                    .prefix("wyrd-card-")
                    .tempdir()
                    .map_err(RegistryEngineError::from)
                    .map_err(WyrdError::from)?,
            )
        } else {
            None
        };
        let artifact_path = destination
            .or_else(|| temporary_directory.as_ref().map(TempDir::path))
            .ok_or_else(|| WyrdError::RegistryInvalidCardSpec {
                message: "artifact load did not produce a destination".to_owned(),
                details: serde_json::json!({}),
            })?;
        self.download_artifacts_to(&card_uid, artifact_path).await?;

        Ok(LoadedCard {
            card,
            temporary_artifact_directory: temporary_directory,
            artifact_directory: destination.map(Path::to_path_buf),
        })
    }
}

/// Builds one complete verified artifact publication before exposing it.
///
/// The materializer retains the authenticated registry engine, exact card UID,
/// and caller-selected destination for one operation. It writes only beneath a
/// random sibling staging directory and renames that complete directory into
/// place after every transfer verifies, so failed transfers cannot alter a
/// prior destination.
struct ArtifactMaterializer<'a> {
    /// Authenticated registry transport and storage client.
    engine: &'a Arc<RegistryEngine>,
    /// Exact server-assigned owner of the inventory.
    card_uid: &'a CardUid,
    /// Final caller-visible artifact root.
    destination: &'a Path,
}

/// Runs the staged download and atomic publication workflow.
impl<'a> ArtifactMaterializer<'a> {
    /// Create a materializer for one Card inventory and destination.
    #[must_use]
    fn new(cards: &'a Cards, card_uid: &'a CardUid, destination: &'a Path) -> Self {
        Self {
            engine: &cards.engine,
            card_uid,
            destination,
        }
    }

    /// Fetch, verify, and atomically publish the complete artifact inventory.
    ///
    /// # Errors
    /// Returns a Wyrd error when inventory planning, staging creation, any
    /// transfer, digest verification, or final publication fails. On transfer
    /// failure the staging directory is dropped and the destination is not
    /// modified. The workflow clears its stdout transfer display before either
    /// a result or error becomes visible to its caller.
    ///
    /// # Cancellation
    /// Cancellation drops the unpublished staging directory and may remove the
    /// active display without completing its bars. The prior destination
    /// remains unchanged.
    async fn materialize(&self) -> Result<(), WyrdError> {
        let inventory = reads::list_artifacts(&self.engine.client, self.card_uid)
            .await
            .map_err(WyrdError::from)?;
        let parent =
            self.destination
                .parent()
                .ok_or_else(|| WyrdError::RegistryInvalidCardSpec {
                    message: "artifact destination must have a parent directory".to_owned(),
                    details: serde_json::json!({ "destination": self.destination }),
                })?;
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(RegistryEngineError::from)
            .map_err(WyrdError::from)?;
        let staging = tempfile::Builder::new()
            .prefix(".wyrd-artifacts-")
            .tempdir_in(parent)
            .map_err(RegistryEngineError::from)
            .map_err(WyrdError::from)?;
        let staging_root = staging.path().join("publication");
        tokio::fs::create_dir(&staging_root)
            .await
            .map_err(RegistryEngineError::from)
            .map_err(WyrdError::from)?;
        let display = Arc::new(DownloadProgressDisplay::stdout());
        let downloads =
            futures_util::stream::iter(inventory.artifacts.into_iter().map(|artifact| {
                let engine = Arc::clone(self.engine);
                let card_uid = self.card_uid.clone();
                let staging_root = staging_root.clone();
                let display = Arc::clone(&display);
                async move {
                    let path = staging_root.join(artifact.relative_path.as_str());
                    if let Some(parent) = path.parent() {
                        tokio::fs::create_dir_all(parent)
                            .await
                            .map_err(RegistryEngineError::from)?;
                    }
                    download::download_artifact(
                        &engine.client,
                        &engine.storage,
                        &card_uid,
                        &artifact.relative_path,
                        &path,
                        display,
                    )
                    .await
                }
            }))
            .buffer_unordered(4)
            .collect::<Vec<_>>()
            .await;
        let result = match downloads.into_iter().find_map(Result::err) {
            Some(error) => Err(WyrdError::from(error)),
            None => self.publish(&staging_root, staging.path()).await,
        };
        display.clear();
        result
    }

    /// Replace the destination only after the complete staging tree verifies.
    ///
    /// The old destination is first renamed beneath the private staging root.
    /// If publishing the new tree fails, that old tree is restored before the
    /// error is returned. Dropping the staging guard removes the superseded
    /// tree only after the new publication is visible.
    ///
    /// # Errors
    /// Returns a Wyrd error if either rename fails. A failed publish attempts
    /// to restore the prior destination; restoration failure is reported with
    /// the original publish failure retained in diagnostics.
    async fn publish(&self, staging_root: &Path, staging_parent: &Path) -> Result<(), WyrdError> {
        let previous = staging_parent.join("previous");
        let had_previous = self.destination.exists();
        if had_previous {
            tokio::fs::rename(self.destination, &previous)
                .await
                .map_err(RegistryEngineError::from)
                .map_err(WyrdError::from)?;
        }
        if let Err(error) = tokio::fs::rename(staging_root, self.destination).await {
            if had_previous {
                let _ = tokio::fs::rename(&previous, self.destination).await;
            }
            return Err(RegistryEngineError::from(error).into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use base64::Engine;
    use secrecy::SecretString;
    use sha2::{Digest, Sha256};
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{CardSelector, Cards};
    use wyrd_semver::VersionBlock;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, CardUid, SpaceName};

    /// Preserve an existing publication and remove private staging when a later
    /// artifact fails integrity verification.
    #[tokio::test]
    async fn bad_second_digest_preserves_destination_and_removes_staging() {
        let server = MockServer::start().await;
        let card_uid =
            CardUid::new("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00").expect("test uid is valid");
        let first = b"verified-first";
        let second = b"corrupt-second";
        let first_digest = base64::engine::general_purpose::STANDARD.encode(Sha256::digest(first));
        let second_digest =
            base64::engine::general_purpose::STANDARD.encode(Sha256::digest(second));
        Mock::given(method("POST"))
            .and(path("/auth/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "test-access-token",
                "refresh_token": "unused-refresh-token",
                "token_type": "Bearer",
                "expires_at": chrono::Utc::now() + chrono::Duration::hours(1)
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/v1/cards/{card_uid}/artifacts")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "artifacts": [
                    {
                        "relative_path": "first.bin",
                        "sha256": first_digest.clone(),
                        "size_bytes": first.len(),
                        "content_type": null
                    },
                    {
                        "relative_path": "nested/second.bin",
                        "sha256": second_digest.clone(),
                        "size_bytes": second.len(),
                        "content_type": null
                    }
                ]
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/cards/download/init"))
            .and(body_json(serde_json::json!({
                "card_uid": card_uid.clone(),
                "relative_path": "first.bin",
                "ttl_secs": null
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "plan": {
                    "get_url": format!("{}/download/first", server.uri()),
                    "ttl_secs": 0
                },
                "size_bytes": first.len(),
                "sha256": first_digest.clone()
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/cards/download/init"))
            .and(body_json(serde_json::json!({
                "card_uid": card_uid.clone(),
                "relative_path": "nested/second.bin",
                "ttl_secs": null
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "plan": {
                    "get_url": format!("{}/download/second", server.uri()),
                    "ttl_secs": 0
                },
                "size_bytes": second.len(),
                "sha256": first_digest.clone()
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/download/first"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(first))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/download/second"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(second))
            .mount(&server)
            .await;

        let parent = tempfile::tempdir().expect("test parent is created");
        let destination = parent.path().join("artifacts");
        std::fs::create_dir(&destination).expect("existing destination is created");
        std::fs::write(destination.join("existing.txt"), b"keep")
            .expect("existing publication is written");
        let cards = Cards::new(
            Some(&server.uri()),
            Some(SecretString::from("test-api-key")),
        )
        .expect("test cards handle is configured");

        let error = cards
            .download_artifacts_to(&card_uid, &destination)
            .await
            .expect_err("bad second digest must reject the publication");

        assert!(matches!(
            error,
            wyrd_spec::error::WyrdError::RegistryArtifactVerifyFailed { .. }
        ));
        assert_eq!(
            std::fs::read(destination.join("existing.txt"))
                .expect("existing publication remains readable"),
            b"keep"
        );
        let staging_remains = std::fs::read_dir(parent.path())
            .expect("test parent is readable")
            .filter_map(Result::ok)
            .any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".wyrd-artifacts-")
            });
        assert!(!staging_remains);
    }

    /// Keep latest and exact selector construction semantically distinct.
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
