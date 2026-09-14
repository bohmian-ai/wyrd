//! Thin napi projection of the Rust-owned Card registry and offline `WyrdState`.
//!
//! Every operation delegates to `wyrd_client`; this module only parses Node
//! strings into Wyrd types and projects results through
//! [`NativeLifecycleResult`], so failures keep their catalog metadata.

use std::path::Path;

use napi_derive::napi;
use wyrd_client::cards::{CardGraphHydrator, CardSelector, Cards, HydrationMode, ListCardsRequest};
use wyrd_client::state::WyrdState;
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::{CardRef, CardRefParseError};

use crate::{NativeLifecycleResult, NativeWyrdError};

/// Tenant-scoped Card registry handle over the shared `wyrd_client` Cards.
#[napi]
pub struct NativeCards {
    /// Shared registry handle owning transport, authentication, and storage.
    cards: Cards,
}

/// Closed result of building one Card registry handle: a handle or a catalog error.
#[napi(object, object_from_js = false)]
pub struct NativeCardsConnection {
    /// Registry handle when construction succeeded.
    pub cards: Option<NativeCards>,
    /// Catalog failure when no credential resolves or the client cannot be built.
    pub error: Option<NativeWyrdError>,
}

/// Builds one Card registry handle without performing IO.
///
/// Omitted arguments resolve through the same shared client configuration
/// chain as `connectBifrost`, so both capabilities authenticate identically.
/// Credential and configuration failures are returned as catalog metadata.
#[napi]
pub fn connect_cards(server_url: Option<String>, credential: Option<String>) -> NativeCardsConnection {
    let client = wyrd_client::bifrost::client_from_options(
        server_url.as_deref(),
        credential.as_deref(),
        None,
    );
    drop(server_url);
    drop(credential);
    match client {
        Ok(client) => NativeCardsConnection {
            cards: Some(NativeCards {
                cards: Cards::with_client(client),
            }),
            error: None,
        },
        Err(error) => NativeCardsConnection {
            cards: None,
            error: Some(NativeWyrdError::from_wyrd(&WyrdError::from(&error))),
        },
    }
}

#[napi]
impl NativeCards {
    /// Loads one Card tree from disk and registers it as a composite.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the receipt cannot be serialized; load,
    /// validation, and registry failures are returned in the result.
    #[napi]
    pub async fn register_from_path(&self, path: String) -> napi::Result<NativeLifecycleResult> {
        NativeLifecycleResult::outcome(
            Box::pin(self.cards.register_from_path(Path::new(&path))).await,
        )
    }

    /// Fetches one Card envelope by exact reference.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the envelope cannot be serialized; an
    /// invalid reference or registry failure is returned in the result.
    #[napi]
    pub async fn get(&self, card_ref: String) -> napi::Result<NativeLifecycleResult> {
        let result = match parse_card_ref(&card_ref) {
            Ok(card_ref) => self.cards.get(CardSelector::exact(card_ref)).await,
            Err(error) => Err(error),
        };
        NativeLifecycleResult::outcome(result)
    }

    /// Lists metadata-only Card summaries for one serialized list request.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the page cannot be serialized; a
    /// malformed request or registry failure is returned in the result.
    #[napi]
    pub async fn list(&self, request_json: String) -> napi::Result<NativeLifecycleResult> {
        let result = match serde_json::from_str::<ListCardsRequest>(&request_json) {
            Ok(request) => self.cards.list(request).await,
            Err(error) => Err(WyrdError::Validation {
                message: error.to_string(),
                details: serde_json::json!({ "field": "request" }),
            }),
        };
        NativeLifecycleResult::outcome(result)
    }

    /// Hydrates one Card graph into a published local bundle.
    ///
    /// `metadata_only` skips artifact payload downloads. A failed hydration
    /// never publishes a partial bundle.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the summary cannot be serialized; graph,
    /// destination, and transfer failures are returned in the result.
    #[napi]
    pub async fn hydrate(
        &self,
        card_ref: String,
        destination: String,
        metadata_only: bool,
    ) -> napi::Result<NativeLifecycleResult> {
        let mode = if metadata_only {
            HydrationMode::MetadataOnly
        } else {
            HydrationMode::Complete
        };
        let result = match parse_card_ref(&card_ref) {
            Ok(card_ref) => {
                let hydrator = CardGraphHydrator::new(self.cards.registry_context());
                Box::pin(hydrator.hydrate(
                    &CardSelector::exact(card_ref),
                    Path::new(&destination),
                    mode,
                ))
                .await
            }
            Err(error) => Err(error),
        };
        NativeLifecycleResult::outcome(result)
    }

    /// Soft-deletes one Card by exact reference.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the result cannot be projected; an
    /// invalid reference or registry failure is returned in the result.
    #[napi]
    pub async fn delete(&self, card_ref: String) -> napi::Result<NativeLifecycleResult> {
        let result = match parse_card_ref(&card_ref) {
            Ok(card_ref) => self.cards.delete(CardSelector::exact(card_ref)).await,
            Err(error) => Err(error),
        };
        NativeLifecycleResult::outcome(result)
    }
}

/// Offline hydrated-bundle view over the shared `wyrd_client` `WyrdState`.
///
/// The open result is retained so an invalid bundle surfaces its catalog error
/// on the first read instead of as an untyped constructor failure.
#[napi]
pub struct NativeWyrdState {
    /// Validated state, or the stable error that rejected the bundle.
    state: Result<WyrdState, WyrdError>,
}

/// Loads and validates one hydrated bundle without contacting Wyrd.
#[napi]
pub fn open_wyrd_state(path: String) -> NativeWyrdState {
    let state = WyrdState::from_path(Path::new(&path));
    drop(path);
    NativeWyrdState { state }
}

#[napi]
impl NativeWyrdState {
    /// Returns the exact root Card reference.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the reference cannot be serialized.
    #[napi]
    pub fn root_ref(&self) -> napi::Result<NativeLifecycleResult> {
        self.read(|state| NativeLifecycleResult::outcome(Ok(state.root_ref())))
    }

    /// Returns every persisted alias in stable sorted order.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the aliases cannot be serialized.
    #[napi]
    pub fn aliases(&self) -> napi::Result<NativeLifecycleResult> {
        self.read(|state| NativeLifecycleResult::outcome(Ok(state.aliases().collect::<Vec<_>>())))
    }

    /// Resolves an alias to its stored Card envelope.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the envelope cannot be serialized.
    #[napi]
    pub fn card(&self, alias: String) -> napi::Result<NativeLifecycleResult> {
        let result = self.read(|state| NativeLifecycleResult::outcome(state.card(&alias)));
        drop(alias);
        result
    }

    /// Resolves an alias to its exact Card reference.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the reference cannot be serialized.
    #[napi]
    pub fn card_ref(&self, alias: String) -> napi::Result<NativeLifecycleResult> {
        let result = self.read(|state| NativeLifecycleResult::outcome(state.card_ref(&alias)));
        drop(alias);
        result
    }

    /// Returns the verified local artifacts for an alias.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the artifact list cannot be serialized.
    #[napi]
    pub fn artifacts(&self, alias: String) -> napi::Result<NativeLifecycleResult> {
        let result = self.read(|state| {
            NativeLifecycleResult::outcome(state.artifacts(&alias).map(|artifacts| {
                artifacts
                    .iter()
                    .map(|artifact| {
                        serde_json::json!({
                            "relative_path": artifact.relative_path(),
                            "local_path": artifact.local_path(),
                            "sha256": artifact.sha256(),
                            "size_bytes": artifact.size_bytes(),
                            "content_type": artifact.content_type(),
                        })
                    })
                    .collect::<Vec<_>>()
            }))
        });
        drop(alias);
        result
    }
}

impl NativeWyrdState {
    /// Runs one projection against the validated state, or returns the stored
    /// open failure unchanged.
    ///
    /// # Errors
    ///
    /// Returns the projection's napi error.
    fn read(
        &self,
        project: impl FnOnce(&WyrdState) -> napi::Result<NativeLifecycleResult>,
    ) -> napi::Result<NativeLifecycleResult> {
        match &self.state {
            Ok(state) => project(state),
            Err(error) => Ok(NativeLifecycleResult::from_wyrd(error)),
        }
    }
}

/// Parses a Card reference passed as its text form or serialized JSON object.
///
/// # Errors
///
/// Returns the stable validation error when neither form parses.
fn parse_card_ref(value: &str) -> Result<CardRef, WyrdError> {
    let parsed = if value.starts_with('{') {
        serde_json::from_str(value).map_err(|error| error.to_string())
    } else {
        value
            .parse()
            .map_err(|error: CardRefParseError| error.to_string())
    };
    parsed.map_err(|message| WyrdError::Validation {
        message,
        details: serde_json::json!({ "field": "card_ref" }),
    })
}
