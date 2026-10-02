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
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, CardUid, SpaceName};
use wyrd_spec::reference::CardRef;

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
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkflowSelectorJson {
    /// Workflow UID.
    uid: Option<CardUid>,
    /// Workflow space.
    space: Option<SpaceName>,
    /// Workflow name.
    name: Option<CardName>,
    /// Exact Workflow version.
    version: Option<String>,
}

/// Parse a JavaScript Workflow selector into a Cards selector.
///
/// # Errors
///
/// Returns `WYRD_WORKFLOW_400_INVALID_CARD_REF` when the JSON is neither
/// `{ uid }` nor `{ space, name, version }`, including a mix of both, or a
/// field is invalid.
pub(crate) fn parse_workflow_selector(selector_json: &str) -> StdResult<CardSelector, WyrdError> {
    let invalid = |reason: &str| WyrdError::WorkflowInvalidCardRef {
        message: format!(
            "Workflow selector must be {{ uid }} or {{ space, name, version }}: {reason}"
        ),
        details: serde_json::json!({ "field": "selector" }),
    };
    let fields: WorkflowSelectorJson =
        serde_json::from_str(selector_json).map_err(|error| invalid(&error.to_string()))?;
    match (fields.uid, fields.space, fields.name, fields.version) {
        (Some(uid), None, None, None) => Ok(CardSelector::uid(CardKind::Workflow, uid)),
        (None, Some(space), Some(name), Some(version)) => {
            let card_ref: CardRef = serde_json::from_value(serde_json::json!({
                "kind": "Workflow",
                "space": space,
                "name": name,
                "version": version,
            }))
            .map_err(|error| invalid(&error.to_string()))?;
            Ok(CardSelector::exact(card_ref))
        }
        _ => Err(invalid("got another combination of fields")),
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

#[napi]
impl NativeWorkflow {
    /// Returns the step IDs in declaration order.
    #[napi]
    pub fn step_ids(&self) -> Vec<String> {
        self.workflow.as_skald().step_ids()
    }

    /// Runs this Workflow with the given JSON object of declared inputs.
    ///
    /// Validation, input, and route errors are returned before any step is
    /// dispatched; step failures are recorded in the returned run.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the run cannot be serialized; a
    /// malformed input or pre-dispatch refusal is returned in the result.
    #[napi]
    pub async fn run(&self, input_json: Option<String>) -> Result<NativeLifecycleResult> {
        let input = match input_json
            .as_deref()
            .map(serde_json::from_str::<Map<String, Value>>)
        {
            None => Map::new(),
            Some(Ok(input)) => input,
            Some(Err(error)) => {
                return Ok(NativeLifecycleResult::from_wyrd(&WyrdError::Validation {
                    message: format!("Workflow input must be a JSON object: {error}"),
                    details: serde_json::json!({ "field": "input" }),
                }));
            }
        };
        let run = self.workflow.run(input).await.map_err(WyrdError::from);
        NativeLifecycleResult::outcome(run)
    }
}
