//! Thin napi projection of the Rust-owned Card registry and offline `WyrdState`.
//!
//! Every operation delegates to `wyrd_client`; this module only parses Node
//! strings into Wyrd types and projects results through
//! [`NativeLifecycleResult`], so failures keep their catalog metadata.

use std::path::Path;
use std::result::Result as StdResult;

use napi::Result;
use napi_derive::napi;
use wyrd_client::cards::{CardKind, CardSelector, Cards, HydrationMode, ListCardsRequest};
use wyrd_client::observe::eval::parse_session_id;
use wyrd_client::observe::{EvalObservationOptions, Run};
use wyrd_client::state::WyrdState;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, SpaceName};
use wyrd_spec::reference::{CardRef, CardRefParseError};

use crate::client::NativeWyrdClient;
use crate::workflow::{NativeWorkflowLoad, parse_workflow_selector};
use crate::{NativeLifecycleResult, NativeTableConfig, NativeWyrdError};

/// Tenant-scoped Card registry handle over the shared `wyrd_client` Cards.
#[napi]
pub struct NativeCards {
    /// Shared registry handle owning transport, authentication, and storage.
    cards: Cards,
}

#[napi]
impl NativeWyrdClient {
    /// Builds one Card registry handle that calls the server as this client.
    ///
    /// No IO happens here; the public TypeScript `Cards.connect` passes the
    /// caller's client or the ambient one.
    #[napi]
    pub fn cards(&self) -> NativeCards {
        NativeCards {
            cards: Cards::with_client(self.client.clone()),
        }
    }

    /// Loads and validates one hydrated bundle whose server calls run as this
    /// client.
    ///
    /// The state's identity is fixed here: Bifrost startup and every verify
    /// go through this client, and the ambient configuration is never read.
    ///
    /// # Arguments
    ///
    /// * `path` - The hydrated bundle directory.
    #[napi]
    pub fn open_wyrd_state(&self, path: String) -> NativeWyrdState {
        NativeWyrdState {
            state: WyrdState::from_path_with_client(path, self.client.clone()),
        }
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

    /// Resolves the latest Active version of one named Card to its exact
    /// reference.
    ///
    /// # Arguments
    ///
    /// * `kind` - The Card kind, such as `Model`.
    /// * `space` - The Card space.
    /// * `name` - The Card name.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the reference cannot be serialized; a
    /// malformed kind, space, or name (`WYRD_SPEC_400_VALIDATION`), no Active
    /// version, and registry failures are returned in the result.
    #[napi]
    pub async fn resolve_latest(
        &self,
        kind: String,
        space: String,
        name: String,
    ) -> Result<NativeLifecycleResult> {
        let invalid = |field: &str, error: &dyn std::fmt::Display| WyrdError::Validation {
            message: format!("{field} is invalid: {error}"),
            details: serde_json::json!({ "field": field }),
        };
        let parsed = (
            serde_json::from_value::<CardKind>(serde_json::Value::String(kind))
                .map_err(|error| invalid("kind", &error)),
            SpaceName::new(space).map_err(|error| invalid("space", &error)),
            CardName::new(name).map_err(|error| invalid("name", &error)),
        );
        let result = match parsed {
            (Ok(kind), Ok(space), Ok(name)) => self.cards.resolve_latest(kind, space, name).await,
            (Err(error), ..) | (_, Err(error), _) | (.., Err(error)) => Err(error),
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
                Box::pin(
                    self.cards
                        .hydrate(CardSelector::exact(card_ref), &destination, mode),
                )
                .await
            }
            Err(error) => Err(error),
        };
        NativeLifecycleResult::outcome(result)
    }

    /// Loads one registered Workflow and its locked Agents and Prompts.
    ///
    /// `selector_json` is `{ "space", "name", "version" }` or `{ "uid" }`; any
    /// other shape, including a mix of both, is refused with
    /// `WYRD_WORKFLOW_400_INVALID_CARD_REF` before any read. A valid selector delegates
    /// to the shared [`wyrd_client::WorkflowCards::load`], which reads every
    /// Card at its locked version.
    ///
    /// Never rejects: a failure is returned in [`NativeWorkflowLoad::error`]
    /// with its catalog code, and the public TypeScript `cards.workflow.load`
    /// throws it as a `WyrdError`. Loading only reads Cards. If the Node
    /// promise is abandoned, completed reads may already have happened, but
    /// no partial Workflow is returned and nothing durable is written.
    #[napi]
    pub async fn load_workflow(&self, selector_json: String) -> NativeWorkflowLoad {
        let outcome = match parse_workflow_selector(&selector_json) {
            Ok(selector) => self.cards.workflow().load(&selector).await,
            Err(error) => Err(error),
        };
        NativeWorkflowLoad::from_outcome(outcome)
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
///
/// The state resolves the ambient client once, at its first server call, and
/// keeps it; [`NativeWyrdClient::open_wyrd_state`] fixes the client instead.
#[napi]
pub fn open_wyrd_state(path: String) -> NativeWyrdState {
    NativeWyrdState {
        state: WyrdState::from_path(path),
    }
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

    /// Returns the stored envelope of the root Card the bundle was hydrated
    /// from.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the envelope cannot be serialized.
    #[napi]
    pub fn service(&self) -> Result<NativeLifecycleResult> {
        self.read(|state| NativeLifecycleResult::outcome(Ok(state.service())))
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

    /// Returns the stored envelope of the Verifier Card an alias names.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the envelope cannot be serialized; an
    /// unknown alias and another kind are returned in the result.
    #[napi]
    pub fn verifier(&self, alias: String) -> Result<NativeLifecycleResult> {
        self.read(|state| {
            NativeLifecycleResult::outcome(state.verifier(&alias).and_then(|_| state.card(&alias)))
        })
    }

    /// Returns the stored envelope of the Workflow Card an alias names.
    ///
    /// # Errors
    ///
    /// As [`NativeWyrdState::verifier`].
    #[napi]
    pub fn workflow(&self, alias: String) -> Result<NativeLifecycleResult> {
        self.read(|state| {
            NativeLifecycleResult::outcome(state.workflow(&alias).and_then(|_| state.card(&alias)))
        })
    }

    /// Returns the stored envelope of the Agent Card an alias names.
    ///
    /// # Errors
    ///
    /// As [`NativeWyrdState::verifier`].
    #[napi]
    pub fn agent(&self, alias: String) -> Result<NativeLifecycleResult> {
        self.read(|state| {
            NativeLifecycleResult::outcome(state.agent(&alias).and_then(|_| state.card(&alias)))
        })
    }

    /// Returns the stored envelope of the Prompt Card an alias names.
    ///
    /// # Errors
    ///
    /// As [`NativeWyrdState::verifier`].
    #[napi]
    pub fn prompt(&self, alias: String) -> Result<NativeLifecycleResult> {
        self.read(|state| {
            NativeLifecycleResult::outcome(state.prompt(&alias).and_then(|_| state.card(&alias)))
        })
    }

    /// Returns the stored envelope of the Model Card an alias names.
    ///
    /// # Errors
    ///
    /// As [`NativeWyrdState::verifier`].
    #[napi]
    pub fn model(&self, alias: String) -> Result<NativeLifecycleResult> {
        self.read(|state| {
            NativeLifecycleResult::outcome(state.model(&alias).and_then(|_| state.card(&alias)))
        })
    }

    /// Returns the stored envelope of the Data Card an alias names.
    ///
    /// # Errors
    ///
    /// As [`NativeWyrdState::verifier`].
    #[napi]
    pub fn data(&self, alias: String) -> Result<NativeLifecycleResult> {
        self.read(|state| {
            NativeLifecycleResult::outcome(state.data(&alias).and_then(|_| state.card(&alias)))
        })
    }

    /// Connects this state's one Bifrost writer and describes the fixed tables.
    ///
    /// Runs as the state's client: the one it was opened with, else the
    /// ambient client it resolves once. Startup describes both fixed
    /// observation tables before succeeding, so a run can never enqueue
    /// against a missing, unauthorized, or incompatible system table.
    ///
    /// # Arguments
    ///
    /// * `table` - The serialized active write table, or `None` for none.
    /// * `client_byte_limit_bytes` - The handle-wide ingestion byte budget, or
    ///   `None` for the 256 MiB default.
    ///
    /// # Errors
    ///
    /// Returns a napi error when `table` is not one serialized `TableConfig`;
    /// a second start, a closed state, and credential, byte-budget, dial, and
    /// fixed-table failures are returned in [`NativeLifecycleResult`].
    #[napi]
    pub async fn start_bifrost(
        &self,
        table: Option<NativeTableConfig>,
        client_byte_limit_bytes: Option<i64>,
    ) -> Result<NativeLifecycleResult> {
        let state = match &self.state {
            Ok(state) => state,
            Err(error) => return Ok(NativeLifecycleResult::from_wyrd(error)),
        };
        let table = table.map(|table| table.parse()).transpose()?;
        NativeLifecycleResult::outcome(
            Box::pin(
                state
                    .start_bifrost_with_config(table, crate::queue_config(client_byte_limit_bytes)),
            )
            .await,
        )
    }

    /// Opens one invocation over this state, targeting `alias` or the root Service.
    ///
    /// Local only: no network IO, no server-side Run resource, and no Verifier
    /// execution. An `alias` resolves in the hydrated graph before the run
    /// mints its `run_id`; an unknown alias is returned as the outcome's
    /// `WYRD_SDK_404_UNKNOWN_ALIAS` error and nothing is opened.
    #[napi]
    pub fn run(&self, alias: Option<String>) -> NativeRunOpen {
        NativeRunOpen::outcome(match (&self.state, alias) {
            (Ok(state), Some(alias)) => state.run_for_card(&alias),
            (Ok(state), None) => Ok(state.run()),
            (Err(error), _) => Err(error.clone()),
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
/// handle-or-error shape as [`NativeWorkflowLoad`].
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

    /// The alias this view was opened with.
    #[napi(getter)]
    pub fn alias(&self) -> String {
        self.run.alias().to_owned()
    }

    /// An immutable sibling view scoped to a registered alias.
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

    /// Judges this view's subject with a bound Verifier from JSON input.
    ///
    /// `input_json` is one Eval context object or one array of Drift feature
    /// rows; `media_json` is one JSON array of media descriptors. One server
    /// call, never replayed; nothing is observed, recorded, or dispatched.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the outcome cannot be projected; an
    /// unbound Verifier, input of the wrong shape, malformed media, and the
    /// server's verification refusals are returned in
    /// [`NativeLifecycleResult`].
    #[napi]
    pub async fn verify(
        &self,
        verifier: String,
        input_json: String,
        media_json: Option<String>,
    ) -> Result<NativeLifecycleResult> {
        let media = match media_json.as_deref().map(serde_json::from_str).transpose() {
            Ok(media) => media.unwrap_or_default(),
            Err(error) => {
                return Ok(NativeLifecycleResult::from_wyrd(&WyrdError::Validation {
                    message: format!("media is invalid: {error}"),
                    details: serde_json::json!({ "field": "media", "reason": error.to_string() }),
                }));
            }
        };
        NativeLifecycleResult::outcome(
            Box::pin(
                self.run
                    .observe()
                    .verify_json(&verifier, &input_json, media),
            )
            .await,
        )
    }

    /// Invokes this view's tool-free Agent once through the Wyrd gateway with
    /// the string variables of `variables_json` and returns its final text.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the outcome cannot be projected; a
    /// non-Agent view, an Agent with tools, malformed variables, and the
    /// gateway's refusals are returned in [`NativeLifecycleResult`].
    #[napi]
    pub async fn invoke(&self, variables_json: Option<String>) -> Result<NativeLifecycleResult> {
        let variables: std::collections::BTreeMap<String, String> = match variables_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
        {
            Ok(variables) => variables.unwrap_or_default(),
            Err(error) => {
                return Ok(NativeLifecycleResult::from_wyrd(&WyrdError::Validation {
                    message: format!("variables are invalid: {error}"),
                    details: serde_json::json!({
                        "field": "variables",
                        "reason": error.to_string(),
                    }),
                }));
            }
        };
        let pairs: Vec<(&str, &str)> = variables
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        NativeLifecycleResult::outcome(Box::pin(self.run.invoke(&pairs)).await)
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
    #[napi]
    pub async fn record(&self, table: String, row_json: String) -> Result<NativeLifecycleResult> {
        NativeLifecycleResult::outcome(
            Box::pin(self.run.observe().record_json(&table, &row_json)).await,
        )
    }
}
