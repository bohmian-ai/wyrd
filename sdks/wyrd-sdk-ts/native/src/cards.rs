//! Thin napi projection of the Rust-owned Card registry and offline `WyrdState`.
//!
//! Every operation delegates to `wyrd_client`; this module only parses Node
//! strings into Wyrd types and projects results through
//! [`NativeLifecycleResult`], so failures keep their catalog metadata.

use std::path::Path;
use std::result::Result as StdResult;

use napi::Result;
use napi_derive::napi;
use wyrd_client::cards::{CardGraphHydrator, CardSelector, Cards, HydrationMode, ListCardsRequest};
use wyrd_client::observe::eval::parse_session_id;
use wyrd_client::observe::{EvalObservationOptions, Run};
use wyrd_client::state::WyrdState;
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::{CardRef, CardRefParseError};

use crate::{NativeLifecycleResult, NativeTableConfig, NativeWyrdError};

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
pub fn connect_cards(
    server_url: Option<String>,
    credential: Option<String>,
) -> NativeCardsConnection {
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
    pub async fn register_from_path(&self, path: String) -> Result<NativeLifecycleResult> {
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
    pub async fn get(&self, card_ref: String) -> Result<NativeLifecycleResult> {
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
    pub async fn list(&self, request_json: String) -> Result<NativeLifecycleResult> {
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
    ) -> Result<NativeLifecycleResult> {
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
    pub async fn delete(&self, card_ref: String) -> Result<NativeLifecycleResult> {
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
    state: StdResult<WyrdState, WyrdError>,
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
    pub fn root_ref(&self) -> Result<NativeLifecycleResult> {
        self.read(|state| NativeLifecycleResult::outcome(Ok(state.root_ref())))
    }

    /// Returns every persisted alias in stable sorted order.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the aliases cannot be serialized.
    #[napi]
    pub fn aliases(&self) -> Result<NativeLifecycleResult> {
        self.read(|state| NativeLifecycleResult::outcome(Ok(state.aliases().collect::<Vec<_>>())))
    }

    /// Resolves an alias to its stored Card envelope.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the envelope cannot be serialized.
    #[napi]
    pub fn card(&self, alias: String) -> Result<NativeLifecycleResult> {
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
    pub fn card_ref(&self, alias: String) -> Result<NativeLifecycleResult> {
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
    pub fn artifacts(&self, alias: String) -> Result<NativeLifecycleResult> {
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

    /// Connects this state's one Bifrost writer and describes the fixed tables.
    ///
    /// The transport arguments are `connectBifrost`'s and resolve through the
    /// same chain when omitted. Startup describes both fixed observation tables
    /// before succeeding, so a run can never enqueue against a missing,
    /// unauthorized, or incompatible system table.
    ///
    /// # Errors
    ///
    /// Returns a napi error when `table` is not one serialized `TableConfig`;
    /// a second start, a closed state, and credential, dial, and fixed-table
    /// failures are returned in [`NativeLifecycleResult`].
    // justification: napi boundary; generated object and string arguments arrive owned
    #[allow(clippy::needless_pass_by_value)]
    #[napi]
    pub async fn start_bifrost(
        &self,
        table: Option<NativeTableConfig>,
        server_url: Option<String>,
        credential: Option<String>,
        grpc_url: Option<String>,
    ) -> Result<NativeLifecycleResult> {
        let state = match &self.state {
            Ok(state) => state,
            Err(error) => return Ok(NativeLifecycleResult::from_wyrd(error)),
        };
        let table = table.map(|table| table.parse()).transpose()?;
        let client = match wyrd_client::bifrost::client_from_options(
            server_url.as_deref(),
            credential.as_deref(),
            grpc_url.as_deref(),
        ) {
            Ok(client) => client,
            Err(error) => {
                return Ok(NativeLifecycleResult::from_wyrd(&WyrdError::from(&error)));
            }
        };
        NativeLifecycleResult::outcome(Box::pin(state.start_bifrost_with(&client, table)).await)
    }

    /// Opens one invocation over this state, targeting the root Service Card.
    ///
    /// Local only: no network IO, no server-side Run resource, and no Verifier
    /// execution.
    #[napi]
    pub fn run(&self) -> NativeRunOpen {
        NativeRunOpen::outcome(match &self.state {
            Ok(state) => Ok(state.run()),
            Err(error) => Err(error.clone()),
        })
    }

    /// Drains every producer of this state's writer without closing it.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the outcome cannot be projected; the
    /// lifecycle refusals and the first producer or sink failure are returned in
    /// [`NativeLifecycleResult`].
    #[napi]
    pub async fn flush(&self) -> Result<NativeLifecycleResult> {
        match &self.state {
            Ok(state) => NativeLifecycleResult::outcome(Box::pin(state.flush()).await),
            Err(error) => Ok(NativeLifecycleResult::from_wyrd(error)),
        }
    }

    /// Drains every producer of this state's writer and closes it to writes.
    ///
    /// Graceful shutdown is the durability barrier: queue admission is not a
    /// Scribe acknowledgement. After an ambiguous failure, retry `shutdown` on
    /// the same state rather than replacing the writer.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the outcome cannot be projected; the first
    /// producer or sink failure is returned in [`NativeLifecycleResult`].
    #[napi]
    pub async fn shutdown(&self) -> Result<NativeLifecycleResult> {
        match &self.state {
            Ok(state) => NativeLifecycleResult::outcome(Box::pin(state.shutdown()).await),
            Err(error) => Ok(NativeLifecycleResult::from_wyrd(error)),
        }
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
        project: impl FnOnce(&WyrdState) -> Result<NativeLifecycleResult>,
    ) -> Result<NativeLifecycleResult> {
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
fn parse_card_ref(value: &str) -> StdResult<CardRef, WyrdError> {
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

/// Closed result of opening one scoped run: a run handle or a catalog error.
///
/// Opening cannot be projected through [`NativeLifecycleResult`] because the run
/// is a native class rather than a serializable value, so it follows the same
/// handle-or-error shape as [`NativeCardsConnection`].
#[napi(object, object_from_js = false)]
pub struct NativeRunOpen {
    /// The scoped run when the bundle and alias resolved.
    pub run: Option<NativeRun>,
    /// The stable failure that rejected the bundle or the alias.
    pub error: Option<NativeWyrdError>,
}

impl NativeRunOpen {
    /// Project one native open outcome into the closed napi result.
    fn outcome(result: StdResult<Run, WyrdError>) -> Self {
        match result {
            Ok(run) => Self {
                run: Some(NativeRun { run }),
                error: None,
            },
            Err(error) => Self {
                run: None,
                error: Some(NativeWyrdError::from_wyrd(&error)),
            },
        }
    }
}

/// One invocation, or one Card-scoped view of it, over the shared `Run`.
///
/// Every emit delegates to `wyrd_client::observe`; this wrapper only carries
/// Node strings across the boundary so the three SDKs share one projection.
#[napi]
pub struct NativeRun {
    /// The native view whose subject and invocation every emit correlates to.
    run: Run,
}

#[napi]
impl NativeRun {
    /// The `UUIDv7` invocation identity this run and every view of it shares.
    #[napi(getter)]
    pub fn run_id(&self) -> String {
        self.run.run_id().as_str().to_owned()
    }

    /// The exact Card reference this view observes.
    #[napi(getter)]
    pub fn card_ref(&self) -> String {
        self.run.card_ref().to_string()
    }

    /// An immutable sibling view scoped to a registered alias.
    // justification: napi boundary; a JavaScript string is primitive and cannot
    // be passed by reference, so the generated binding requires an owned String
    #[allow(clippy::needless_pass_by_value)]
    #[napi]
    pub fn for_card(&self, alias: String) -> NativeRunOpen {
        NativeRunOpen::outcome(self.run.for_card(&alias))
    }

    /// Emits one Drift observation from its JSON feature object.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the outcome cannot be projected; a
    /// malformed feature map, the lifecycle refusals, and queue saturation are
    /// returned in [`NativeLifecycleResult`].
    // justification: napi boundary; JavaScript strings arrive owned
    #[allow(clippy::needless_pass_by_value)]
    #[napi]
    pub fn drift(
        &self,
        features_json: String,
        session_id: Option<String>,
    ) -> Result<NativeLifecycleResult> {
        let session = match parse_session_id(session_id.as_deref()) {
            Ok(session) => session,
            Err(error) => return Ok(NativeLifecycleResult::from_wyrd(&error)),
        };
        NativeLifecycleResult::outcome(self.run.observe().drift_json(&features_json, session))
    }

    /// Emits one Eval observation from its JSON context and options.
    ///
    /// `media_json` is one JSON array of media descriptors; `trace_id` and
    /// `span_id` are lower-case hex and fall back to the active span when both
    /// are omitted.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the outcome cannot be projected; malformed
    /// identifiers, a span without its trace, the lifecycle refusals, and queue
    /// saturation are returned in [`NativeLifecycleResult`].
    // justification: napi boundary; JavaScript strings arrive owned
    #[allow(clippy::needless_pass_by_value)]
    #[napi]
    pub fn eval(
        &self,
        context_json: String,
        session_id: Option<String>,
        media_json: Option<String>,
        trace_id: Option<String>,
        span_id: Option<String>,
    ) -> Result<NativeLifecycleResult> {
        let options = EvalObservationOptions::from_parts(
            session_id.as_deref(),
            media_json.as_deref(),
            trace_id.as_deref(),
            span_id.as_deref(),
        );
        let options = match options {
            Ok(options) => options,
            Err(error) => return Ok(NativeLifecycleResult::from_wyrd(&error)),
        };
        NativeLifecycleResult::outcome(self.run.observe().eval_json(&context_json, options))
    }

    /// Emits one row into a registered `vala.datasets` table.
    ///
    /// Asynchronous because the first call for a table describes it; later calls
    /// reuse the cached schema and producer.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the outcome cannot be projected; a
    /// reserved, unknown, or unauthorized table and the lifecycle refusals are
    /// returned in [`NativeLifecycleResult`].
    // justification: napi boundary; JavaScript strings arrive owned
    #[allow(clippy::needless_pass_by_value)]
    #[napi]
    pub async fn record(&self, table: String, row_json: String) -> Result<NativeLifecycleResult> {
        NativeLifecycleResult::outcome(
            Box::pin(self.run.observe().record_json(&table, &row_json)).await,
        )
    }
}
