//! Thin napi projection of the shared [`wyrd_client::Workflow`].
//!
//! Loading and running happen in Rust. This module only parses Node strings
//! and returns either a Workflow handle or a catalog error, so failures keep
//! their stable codes.

use std::result::Result as StdResult;

use napi::Result;
use napi_derive::napi;
use serde::Deserialize;
use serde_json::{Map, Value};
use wyrd_client::Workflow;
use wyrd_client::cards::{CardKind, CardSelector};
use wyrd_semver::VersionBlock;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, CardUid, SpaceName};

use crate::client::NativeWyrdClient;
use crate::{NativeLifecycleResult, NativeWyrdError};

/// Runnable Workflow loaded from a file or from the registry.
#[napi]
pub struct NativeWorkflow {
    /// Hydrated and validated shared Workflow.
    workflow: Workflow,
}

/// Closed result of loading one Workflow: a handle or a catalog error.
#[napi(object, object_from_js = false)]
pub struct NativeWorkflowLoad {
    /// Workflow when loading succeeded.
    pub workflow: Option<NativeWorkflow>,
    /// Catalog failure otherwise.
    pub error: Option<NativeWyrdError>,
}

impl NativeWorkflowLoad {
    /// Project one load outcome onto the closed result.
    pub(crate) fn from_outcome(outcome: StdResult<Workflow, WyrdError>) -> Self {
        match outcome {
            Ok(workflow) => Self {
                workflow: Some(NativeWorkflow { workflow }),
                error: None,
            },
            Err(error) => Self {
                workflow: None,
                error: Some(NativeWyrdError::from_wyrd(&error)),
            },
        }
    }
}

/// Registered-Workflow selector fields as JavaScript sends them.
///
/// Exactly one of two shapes is valid: `{ uid }` or `{ space, name, version }`.
/// Values stay raw strings here so each one is validated by its own
/// constructor and a failure names the field that was wrong.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkflowSelectorJson {
    /// Workflow UID.
    uid: Option<String>,
    /// Workflow space.
    space: Option<String>,
    /// Workflow name.
    name: Option<String>,
    /// Exact Workflow version.
    version: Option<String>,
}

/// Parse a JavaScript Workflow selector into a Cards selector.
///
/// # Errors
///
/// Returns `WYRD_WORKFLOW_400_INVALID_CARD_REF` with `details.field` set to
/// `selector` when the JSON is neither `{ uid }` nor
/// `{ space, name, version }`, including a mix of both, or to `uid`, `space`,
/// `name`, or `version` when that value is malformed.
pub(crate) fn parse_workflow_selector(selector_json: &str) -> StdResult<CardSelector, WyrdError> {
    let invalid = |field: &str, reason: String| WyrdError::WorkflowInvalidCardRef {
        message: format!("invalid Workflow {field}: {reason}"),
        details: serde_json::json!({ "field": field }),
    };
    let shape = |reason: String| {
        invalid(
            "selector",
            format!("selector must be {{ uid }} or {{ space, name, version }}: {reason}"),
        )
    };
    let fields: WorkflowSelectorJson =
        serde_json::from_str(selector_json).map_err(|error| shape(error.to_string()))?;
    match (fields.uid, fields.space, fields.name, fields.version) {
        (Some(uid), None, None, None) => Ok(CardSelector::uid(
            CardKind::Workflow,
            CardUid::new(uid).map_err(|error| invalid("uid", error.to_string()))?,
        )),
        (None, Some(space), Some(name), Some(version)) => Ok(CardSelector::named(
            CardKind::Workflow,
            SpaceName::new(space).map_err(|error| invalid("space", error.to_string()))?,
            CardName::new(name).map_err(|error| invalid("name", error.to_string()))?,
        )
        .with_version(
            VersionBlock::parse(version).map_err(|error| invalid("version", error.to_string()))?,
        )),
        _ => Err(shape("got another combination of fields".to_owned())),
    }
}

/// Load an authored Workflow file and the Cards it references.
///
/// Delegates to the shared [`Workflow::from_path`]: local files load without
/// a server, and registry refs are read through the ambient client
/// configuration (`WYRD_SERVER_URL`, `WYRD_API_KEY`). Never rejects: a load
/// failure is returned in [`NativeWorkflowLoad::error`] with its catalog
/// code, and the public TypeScript `Workflow.fromPath` throws it as a
/// `WyrdError`.
///
/// Loading only reads files and Cards. If the Node promise is abandoned,
/// completed reads may already have happened, but no partial Workflow is
/// returned and nothing durable is written.
#[napi]
pub async fn load_workflow_from_path(path: String) -> NativeWorkflowLoad {
    NativeWorkflowLoad::from_outcome(Workflow::from_path(path).await)
}

/// Parse a Workflow from its Card envelope YAML text, resolving inline
/// Agents and Prompts eagerly.
///
/// Delegates to the shared [`Workflow::from_yaml`]: no file is read and no
/// client is built, so a Workflow that refs a registered Card is loaded with
/// [`load_workflow_from_path`] instead. Never throws: a parse or validation
/// failure is returned in [`NativeWorkflowLoad::error`] with its catalog code.
///
/// # Arguments
///
/// * `yaml` - The Workflow Card envelope text.
#[napi]
pub fn workflow_from_yaml(yaml: String) -> NativeWorkflowLoad {
    NativeWorkflowLoad::from_outcome(Workflow::from_yaml(&yaml))
}

#[napi]
impl NativeWyrdClient {
    /// Load an authored Workflow file and its Cards as this client.
    ///
    /// Delegates to the shared [`Workflow::from_path_with_client`]: registry
    /// refs are read, and gateway steps later run, as this client rather than
    /// the ambient configuration. Never rejects: a load failure is returned
    /// in [`NativeWorkflowLoad::error`] with its catalog code. Cancellation
    /// behaves as for [`load_workflow_from_path`].
    #[napi]
    pub async fn load_workflow_from_path(&self, path: String) -> NativeWorkflowLoad {
        NativeWorkflowLoad::from_outcome(
            Workflow::from_path_with_client(path, self.client.clone()).await,
        )
    }
}

#[napi]
impl NativeWorkflow {
    /// Returns the Workflow Card name, when the envelope declares one.
    #[napi(getter)]
    pub fn name(&self) -> Option<String> {
        self.workflow.name().map(str::to_owned)
    }

    /// Returns the Workflow Card version, when the envelope declares one.
    #[napi(getter)]
    pub fn version(&self) -> Option<String> {
        self.workflow.version().map(str::to_owned)
    }

    /// Returns the Workflow Card space, when the envelope declares one.
    #[napi(getter)]
    pub fn space(&self) -> Option<String> {
        self.workflow.space().map(str::to_owned)
    }

    /// Returns the step IDs in declaration order.
    #[napi]
    pub fn steps(&self) -> Vec<String> {
        self.workflow.as_skald().step_ids()
    }

    /// Runs this Workflow with its JSON-encoded input.
    ///
    /// A JSON string is shorthand for the declared string input named
    /// `input`, a JSON object supplies declared inputs by name, and an
    /// omitted input uses the defaults. Validation, input, and route errors
    /// are returned before any step is dispatched; step failures are recorded
    /// in the returned run.
    ///
    /// # Arguments
    ///
    /// * `input_json` - The JSON string or object input, or `None`.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the run cannot be serialized; a
    /// malformed input or pre-dispatch refusal is returned in the result.
    #[napi]
    pub async fn run(&self, input_json: Option<String>) -> Result<NativeLifecycleResult> {
        let input = input_json
            .as_deref()
            .map_or(Ok(Value::Object(Map::new())), serde_json::from_str);
        let run = match input {
            Ok(Value::String(text)) => self.workflow.run(text).await,
            Ok(Value::Object(input)) => self.workflow.run(input).await,
            _ => {
                return Ok(NativeLifecycleResult::from_wyrd(&WyrdError::Validation {
                    message: "Workflow input must be a string or a JSON object".to_owned(),
                    details: serde_json::json!({ "field": "input" }),
                }));
            }
        };
        NativeLifecycleResult::outcome(run.map_err(WyrdError::from))
    }
}
