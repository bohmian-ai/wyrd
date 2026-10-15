//! Thin napi projection of the Rust-owned Card registry and offline `WyrdState`.
//!
//! Every operation delegates to `wyrd_client`; this module only parses Node
//! strings into Wyrd types and projects results through
//! [`NativeLifecycleResult`], so failures keep their catalog metadata.

use std::path::Path;
use std::result::Result as StdResult;

use napi::Env;
use napi::Result;
use napi::bindgen_prelude::PromiseRaw;
use napi_derive::napi;
use wyrd_client::cards::{CardKind, CardSelector, Cards, HydrationMode, ListCardsRequest};
use wyrd_client::observe::Run;
use wyrd_client::state::{TelemetryFlushFailure, WyrdState};
use wyrd_spec::envelope::Card;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, SpaceName};
use wyrd_spec::reference::{CardRef, CardRefParseError};
use wyrd_spec::{ExecutionRoute, Prompt, ResponseType};

use crate::client::{NativeWyrdClient, NativeWyrdClientResult};
use crate::verification::{NativeSubmission, NativeVerifierInput};
use crate::{NativeLifecycleResult, NativeTableConfig, NativeWyrdError};

/// Tenant-scoped Card registry handle over the shared `wyrd_client` Cards.
#[napi]
pub struct NativeCards {
    /// Shared registry handle owning transport, authentication, and storage.
    cards: Cards,
}

/// Closed result of connecting one Card registry handle: the handle or a
/// catalog error.
#[napi(object, object_from_js = false)]
pub struct NativeCardsConnection {
    /// Connected handle when the connection succeeded.
    pub handle: Option<NativeCards>,
    /// Catalog failure otherwise.
    pub error: Option<NativeWyrdError>,
}

#[napi]
impl NativeWyrdClient {
    /// Connects one Card registry handle that calls the server as this client.
    ///
    /// Completes any credential exchange this client requires before
    /// resolving, as `Cards::connect_with` does; the public TypeScript
    /// `Cards.connect` passes the caller's client or the ambient one. A refused
    /// or unreachable exchange is returned as catalog metadata.
    ///
    /// # Errors
    ///
    /// Never returns a napi error; failures are carried in the result.
    #[napi]
    pub fn cards<'env>(
        &self,
        env: &'env Env,
    ) -> napi::Result<PromiseRaw<'env, NativeCardsConnection>> {
        let client = self.client.clone();
        crate::trace::spawn(env, async move {
            Ok(match Cards::connect_with(&client).await {
                Ok(cards) => NativeCardsConnection {
                    handle: Some(NativeCards { cards }),
                    error: None,
                },
                Err(error) => NativeCardsConnection {
                    handle: None,
                    error: Some(NativeWyrdError::from_wyrd(&error)),
                },
            })
        })
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
            state: WyrdState::from_path_with(path, self.client.clone()),
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
    pub fn register_from_path<'env>(
        &self,
        env: &'env Env,
        path: String,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let cards = self.cards.clone();
        crate::trace::spawn(env, async move {
            NativeLifecycleResult::outcome(
                Box::pin(cards.register_from_path(Path::new(&path))).await,
            )
        })
    }

    /// Fetches one Card envelope by exact reference.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the envelope cannot be serialized; an
    /// invalid reference or registry failure is returned in the result.
    #[napi]
    pub fn get<'env>(
        &self,
        env: &'env Env,
        card_ref: String,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let cards = self.cards.clone();
        crate::trace::spawn(env, async move {
            let result = match parse_card_ref(&card_ref) {
                Ok(card_ref) => cards.get(CardSelector::exact(card_ref)).await,
                Err(error) => Err(error),
            };
            NativeLifecycleResult::outcome(result)
        })
    }

    /// Lists metadata-only Card summaries for one serialized list request.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the page cannot be serialized; a
    /// malformed request or registry failure is returned in the result.
    #[napi]
    pub fn list<'env>(
        &self,
        env: &'env Env,
        request_json: String,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let cards = self.cards.clone();
        crate::trace::spawn(env, async move {
            let result = match serde_json::from_str::<ListCardsRequest>(&request_json) {
                Ok(request) => cards.list(request).await,
                Err(error) => Err(WyrdError::Validation {
                    message: error.to_string(),
                    details: serde_json::json!({ "field": "request" }),
                }),
            };
            NativeLifecycleResult::outcome(result)
        })
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
    pub fn resolve_latest<'env>(
        &self,
        env: &'env Env,
        kind: String,
        space: String,
        name: String,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let cards = self.cards.clone();
        crate::trace::spawn(env, async move {
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
                (Ok(kind), Ok(space), Ok(name)) => cards.resolve_latest(kind, space, name).await,
                (Err(error), ..) | (_, Err(error), _) | (.., Err(error)) => Err(error),
            };
            NativeLifecycleResult::outcome(result)
        })
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
    pub fn hydrate<'env>(
        &self,
        env: &'env Env,
        card_ref: String,
        destination: String,
        metadata_only: bool,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let cards = self.cards.clone();
        crate::trace::spawn(env, async move {
            let mode = if metadata_only {
                HydrationMode::MetadataOnly
            } else {
                HydrationMode::Complete
            };
            let result = match parse_card_ref(&card_ref) {
                Ok(card_ref) => {
                    Box::pin(cards.hydrate(CardSelector::exact(card_ref), &destination, mode)).await
                }
                Err(error) => Err(error),
            };
            NativeLifecycleResult::outcome(result)
        })
    }

    /// Soft-deletes one Card by exact reference.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the result cannot be projected; an
    /// invalid reference or registry failure is returned in the result.
    #[napi]
    pub fn delete<'env>(
        &self,
        env: &'env Env,
        card_ref: String,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let cards = self.cards.clone();
        crate::trace::spawn(env, async move {
            let result = match parse_card_ref(&card_ref) {
                Ok(card_ref) => cards.delete(CardSelector::exact(card_ref)).await,
                Err(error) => Err(error),
            };
            NativeLifecycleResult::outcome(result)
        })
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

/// The catalog refusal `WyrdState.startTelemetry` throws when a global tracer
/// provider is already registered, so its title and remediation come from the
/// one Rust catalog.
#[napi]
pub fn telemetry_provider_exists() -> NativeWyrdError {
    NativeWyrdError::from_wyrd(&WyrdError::SdkTelemetryProviderExists {
        message: "an OpenTelemetry tracer provider is already installed".to_owned(),
        details: serde_json::json!({}),
    })
}

/// The catalog error `WyrdState.flush` and `WyrdState.shutdown` throw when the
/// tracer provider `startTelemetry` installed did not flush, built by the
/// shared [`TelemetryFlushFailure`] so Python and TypeScript report the same
/// code and details. `cause` is payload-free; `native_code` is the stable code
/// of a native drain that also failed.
#[napi]
pub fn telemetry_flush_error(
    timed_out: bool,
    cause: String,
    native_code: Option<String>,
) -> NativeWyrdError {
    let failure = if timed_out {
        TelemetryFlushFailure::timeout(cause)
    } else {
        TelemetryFlushFailure::failed(cause)
    };
    NativeWyrdError::from_wyrd(&failure.into_error(native_code.as_deref()))
}

#[napi]
impl NativeWyrdState {
    /// The client this state's server calls run as, resolving the ambient one
    /// once on first use, so a telemetry exporter authenticates as the state.
    #[napi]
    pub fn client(&self) -> NativeWyrdClientResult {
        match &self.state {
            Ok(state) => NativeWyrdClientResult::from_outcome(state.client().cloned()),
            Err(error) => NativeWyrdClientResult {
                client: None,
                error: Some(NativeWyrdError::from_wyrd(error)),
            },
        }
    }

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

    /// Returns the stored envelope of the Agent Card an alias names, with
    /// the exact Prompt reference it pins and that Prompt's stored envelope
    /// and derived metadata.
    ///
    /// The Prompt is the one the shared state's hydrated Agent resolves, so
    /// the wrapper navigates without resolving the relationship itself.
    ///
    /// # Errors
    ///
    /// As [`NativeWyrdState::verifier`].
    #[napi]
    pub fn agent(&self, alias: String) -> Result<NativeLifecycleResult> {
        self.read(|state| NativeLifecycleResult::outcome(hydrated_agent(state, &alias)))
    }

    /// Returns the stored envelope of the Prompt Card an alias names, with
    /// the metadata the shared state's hydrated Prompt derives from it.
    ///
    /// # Errors
    ///
    /// As [`NativeWyrdState::verifier`].
    #[napi]
    pub fn prompt(&self, alias: String) -> Result<NativeLifecycleResult> {
        self.read(|state| {
            NativeLifecycleResult::outcome(state.prompt(&alias).and_then(|prompt| {
                Ok(hydrated_prompt(
                    state.card(&alias)?,
                    &prompt.metadata.prompt,
                ))
            }))
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
    pub fn start_bifrost<'env>(
        &self,
        env: &'env Env,
        table: Option<NativeTableConfig>,
        client_byte_limit_bytes: Option<i64>,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let state = self.state.clone();
        crate::trace::spawn(env, async move {
            let state = match &state {
                Ok(state) => state,
                Err(error) => return Ok(NativeLifecycleResult::from_wyrd(error)),
            };
            let table = table.map(|table| table.parse()).transpose()?;
            NativeLifecycleResult::outcome(
                Box::pin(state.start_bifrost_with_config(
                    table,
                    crate::queue_config(client_byte_limit_bytes),
                ))
                .await,
            )
        })
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
    pub fn flush<'env>(
        &self,
        env: &'env Env,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let state = self.state.clone();
        crate::trace::spawn(env, async move {
            match &state {
                Ok(state) => NativeLifecycleResult::outcome(Box::pin(state.flush()).await),
                Err(error) => Ok(NativeLifecycleResult::from_wyrd(error)),
            }
        })
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
    pub fn shutdown<'env>(
        &self,
        env: &'env Env,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let state = self.state.clone();
        crate::trace::spawn(env, async move {
            match &state {
                Ok(state) => NativeLifecycleResult::outcome(Box::pin(state.shutdown()).await),
                Err(error) => Ok(NativeLifecycleResult::from_wyrd(error)),
            }
        })
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

/// Decodes an optional JSON argument named `field` from the TypeScript layer.
///
/// # Errors
///
/// Returns the stable `WYRD_SPEC_400_VALIDATION` error naming `field` when the
/// present value is not valid JSON of type `T`.
fn parse_optional_json<T: serde::de::DeserializeOwned>(
    field: &str,
    value: Option<&str>,
) -> StdResult<Option<T>, WyrdError> {
    value
        .map(serde_json::from_str)
        .transpose()
        .map_err(|error| WyrdError::Validation {
            message: format!("{field} is invalid: {error}"),
            details: serde_json::json!({ "field": field, "reason": error.to_string() }),
        })
}

/// Projects the Agent an alias names as its stored envelope, the exact
/// Prompt reference it pins, and that Prompt's [`hydrated_prompt`]
/// projection.
///
/// # Errors
///
/// Returns the unknown-alias, kind-mismatch, or invalid-bundle error the
/// shared state's Agent lookup reports, or an invalid-bundle error when the
/// resolved Prompt has no stored envelope.
fn hydrated_agent(state: &WyrdState, alias: &str) -> StdResult<serde_json::Value, WyrdError> {
    let agent = state.agent(alias)?;
    let prompt_key = agent.prompt_ref().to_string();
    let prompt = state
        .cards()
        .find_map(|(key, card)| (key == prompt_key).then_some(card))
        .ok_or_else(|| WyrdError::SdkInvalidStateBundle {
            message: "Agent prompt target has no stored envelope".to_owned(),
            details: serde_json::json!({ "alias": alias, "card_ref": agent.prompt_ref() }),
        })?;
    Ok(serde_json::json!({
        "card": state.card(alias)?,
        "prompt_ref": agent.prompt_ref(),
        "prompt": hydrated_prompt(prompt, &agent.prompt().metadata.prompt),
    }))
}

/// Projects a hydrated Prompt as its stored envelope plus the text variables,
/// media variables, and response schema and name the shared runtime Prompt
/// derives; a plain-text Prompt has a null schema and name.
///
/// The derived values are read, never recomputed, so TypeScript reports
/// exactly what the Rust and Python surfaces report.
fn hydrated_prompt(card: &Card, prompt: &Prompt) -> serde_json::Value {
    let (schema, name) = match &prompt.response_type {
        ResponseType::JsonSchema { name, schema } => (Some(schema), Some(name)),
        ResponseType::Text => (None, None),
    };
    serde_json::json!({
        "card": card,
        "variables": prompt.variables,
        "media_variables": prompt.media_variables,
        "response_schema": schema,
        "response_schema_name": name,
    })
}

/// Parses a Card reference passed as its text form or serialized JSON object.
///
/// # Errors
///
/// Returns the stable validation error when neither form parses.
pub(crate) fn parse_card_ref(value: &str) -> StdResult<CardRef, WyrdError> {
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
/// is a native class rather than a serializable value, so it carries either a
/// run handle or a catalog error.
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

    /// The UID of this view's subject Card, which `Run.scope` stamps on
    /// spans as `wyrd.card_uid`.
    #[napi(getter)]
    pub fn subject_uid(&self) -> String {
        self.run.subject_uid().as_str().to_owned()
    }

    /// An immutable sibling view scoped to a registered alias.
    #[napi]
    pub fn for_card(&self, alias: String) -> NativeRunOpen {
        NativeRunOpen::outcome(self.run.for_card(&alias))
    }

    /// Executes a bound Verifier now on named slot values and returns its
    /// result.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the outcome cannot be projected; input
    /// refusals, a Window Verifier, and the server's refusals are returned in
    /// [`NativeLifecycleResult`].
    #[napi]
    pub fn verify<'env>(
        &self,
        env: &'env Env,
        verifier: String,
        inputs: Vec<NativeVerifierInput>,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let run = self.run.clone();
        crate::trace::spawn(env, async move {
            let result = match crate::verification::inputs(inputs) {
                Ok(inputs) => Box::pin(run.observe().verify(&verifier, inputs)).await,
                Err(error) => Err(error),
            };
            NativeLifecycleResult::outcome(result)
        })
    }

    /// Submits named slot values: Window samples are recorded, Run evidence
    /// is queued for one execution.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the outcome cannot be projected; input
    /// refusals and the server's refusals are returned in [`NativeSubmission`].
    #[napi]
    pub fn submit<'env>(
        &self,
        env: &'env Env,
        verifier: String,
        inputs: Vec<NativeVerifierInput>,
    ) -> napi::Result<PromiseRaw<'env, NativeSubmission>> {
        let run = self.run.clone();
        crate::trace::spawn(env, async move {
            let result = match crate::verification::inputs(inputs) {
                Ok(inputs) => Box::pin(run.observe().submit(&verifier, inputs)).await,
                Err(error) => Err(error),
            };
            Ok(NativeSubmission::outcome(result))
        })
    }

    /// Invokes this view's tool-free Agent once with the string variables of
    /// `variables_json` and returns its final text.
    ///
    /// `route_json`, when given, is an execution route that replaces the
    /// Agent's declared route whole for this invocation; it is decoded into
    /// the Rust `ExecutionRoute` and passed to `Run::invoke_on`, which owns
    /// every routing decision. Omitted, the declared route is used.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the outcome cannot be projected; a
    /// non-Agent view, an Agent with tools, malformed variables or route, and
    /// the gateway's refusals are returned in [`NativeLifecycleResult`].
    #[napi]
    pub fn invoke<'env>(
        &self,
        env: &'env Env,
        variables_json: Option<String>,
        route_json: Option<String>,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let run = self.run.clone();
        crate::trace::spawn(env, async move {
            let parsed = parse_optional_json::<std::collections::BTreeMap<String, String>>(
                "variables",
                variables_json.as_deref(),
            )
            .and_then(|variables| {
                parse_optional_json::<ExecutionRoute>("route", route_json.as_deref())
                    .map(|route| (variables.unwrap_or_default(), route))
            });
            let (variables, route) = match parsed {
                Ok(parsed) => parsed,
                Err(error) => return Ok(NativeLifecycleResult::from_wyrd(&error)),
            };
            let pairs: Vec<(&str, &str)> = variables
                .iter()
                .map(|(name, value)| (name.as_str(), value.as_str()))
                .collect();
            let invoked = match &route {
                Some(route) => Box::pin(run.invoke_on(route, &pairs)).await,
                None => Box::pin(run.invoke(&pairs)).await,
            };
            NativeLifecycleResult::outcome(invoked)
        })
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
    pub fn record<'env>(
        &self,
        env: &'env Env,
        table: String,
        row_json: String,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let run = self.run.clone();
        crate::trace::spawn(env, async move {
            NativeLifecycleResult::outcome(
                Box::pin(run.observe().record_json(&table, &row_json)).await,
            )
        })
    }
}
