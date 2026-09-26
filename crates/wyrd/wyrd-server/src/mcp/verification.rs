//! Card and Verification control-plane capabilities over MCP.
//!
//! Typed projections of the same server operations HTTP serves:
//! `cards.get` reads a Card (including `card.status.verification`), and the
//! three `verification.*` tools read binding and run status and request a
//! manual Drift run. Authorization, audit, tenancy, idempotency, and error
//! mapping stay with [`VerificationControl`] and the Cards read path, so this
//! module only parses arguments and projects answers. Verdicts are read with
//! the existing `bifrost.query` tool by `result_id`.

use rmcp::model::{CallToolResult, Tool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Value as JsonValue};
use wyrd_runtime::Permission;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{BindingId, CardUid, IdempotencyKey, VerificationRunId};
use wyrd_spec::registry::GetCardResponse;
use wyrd_spec::verification::{
    StartVerificationRunResponse, VerificationBindingStatus, VerificationRunInput,
    VerificationRunStatus, VerificationRunTarget,
};

use super::WyrdMcpHandler;
use super::principals::{parse_args, tool};
use crate::components::auth::Caller;
use crate::components::cards::routes::get_card_for;
use crate::components::verification::service::{VerificationControl, decode_start_request};
use crate::http::error::internal_failure;

/// Wire name of the Card read.
pub(super) const CARDS_GET: &str = "cards.get";

/// Wire name of the binding status read.
pub(super) const GET_BINDING: &str = "verification.get_binding";

/// Wire name of the manual run request.
pub(super) const START_RUN: &str = "verification.start_run";

/// Wire name of the run status read.
pub(super) const GET_RUN: &str = "verification.get_run";

/// Arguments of `cards.get`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GetCardArgs {
    /// Card kind namespace of the UID.
    kind: CardKind,
    /// Server-minted Card UID.
    card_uid: CardUid,
}

/// Arguments of `verification.get_binding`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GetBindingArgs {
    /// Binding ID from the owner Card's `card.status.verification.binding_ids`.
    binding_id: BindingId,
}

/// Arguments of `verification.get_run`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GetRunArgs {
    /// Run ID returned by `verification.start_run`.
    run_id: VerificationRunId,
}

/// Advertised arguments of `verification.start_run`.
///
/// The HTTP request body plus the optional `Idempotency-Key` header value.
/// Only its schema is used: the arguments are decoded through the same
/// request decoder HTTP uses, so both refuse a malformed request identically.
#[derive(Debug, Serialize, JsonSchema)]
struct StartRunArgs {
    /// Binding or direct Verifier target.
    target: VerificationRunTarget,
    /// Drift window to analyze.
    input: VerificationRunInput,
    /// Optional retry key; a retry with the same key and request returns the
    /// same `run_id`.
    idempotency_key: Option<String>,
}

/// The read tools every authenticated caller may be shown.
#[must_use]
pub(super) fn descriptors_unscoped() -> Vec<Tool> {
    vec![
        tool::<GetCardArgs, GetCardResponse>(
            CARDS_GET,
            "Read a Card",
            "Read one Card in this caller's tenant by kind and Card UID. \
             card.status.verification carries a Verifier's PSI/SPC baseline status and an owner \
             Card's derived binding_ids. Requires cards:read.",
            true,
        ),
        tool::<GetBindingArgs, VerificationBindingStatus>(
            GET_BINDING,
            "Read a verification binding",
            "Read one verification binding's exact owner, subject, and Verifier identities, \
             whether its owner is active, Verifier readiness, next scheduled run, last \
             activation, and last run. Requires cards:read.",
            true,
        ),
        tool::<GetRunArgs, VerificationRunStatus>(
            GET_RUN,
            "Read a verification run",
            "Read one Verifier run's execution status, manual requester, result_id, execution \
             error, and Operator dispatch statuses. The verdict is read from Bifrost with \
             bifrost.query by result_id. Requires cards:read.",
            true,
        ),
    ]
}

/// The write tools shown only to a caller holding `evals:run`.
///
/// A planning convenience, not the boundary: [`VerificationControl`]
/// authorizes and audits the same permission when the tool is named directly.
#[must_use]
pub(super) fn write_descriptors() -> Vec<Tool> {
    vec![tool::<StartRunArgs, StartVerificationRunResponse>(
        START_RUN,
        "Start a manual verification run",
        "Durably enqueue one manual Drift run over a bounded UTC window [start, end) for a \
         binding or a direct Verifier/subject pair, and return its run_id without waiting for \
         scoring. Poll verification.get_run. Requires evals:run and, for a Card-bound caller, \
         Card scope over the subject.",
        false,
    )]
}

/// Whether this caller's token carries `evals:run`.
pub(super) fn may_start_runs(caller: &Caller) -> bool {
    caller
        .principal
        .effective_permissions
        .contains(&Permission::eval_run())
}

/// Project one typed answer as a structured tool result.
///
/// # Errors
/// Returns an internal error when the answer cannot be serialized.
fn structured<T: Serialize>(value: &T) -> Result<CallToolResult, WyrdError> {
    Ok(CallToolResult::structured(
        serde_json::to_value(value).map_err(|error| {
            internal_failure("verification answer could not be projected", &error)
        })?,
    ))
}

impl WyrdMcpHandler {
    /// Read one Card by kind and UID.
    ///
    /// # Errors
    /// Returns a Wyrd error when the arguments are invalid, the caller lacks
    /// `cards:read`, the Card is absent, or the read fails.
    pub(super) async fn mcp_get_card(
        &self,
        caller: Caller,
        arguments: Option<JsonMap<String, JsonValue>>,
    ) -> Result<CallToolResult, WyrdError> {
        let args: GetCardArgs = parse_args(arguments, CARDS_GET)?;
        structured(&get_card_for(&self.state, &caller, &args.kind, &args.card_uid).await?)
    }

    /// Read one verification binding's status.
    ///
    /// # Errors
    /// Returns a Wyrd error when the arguments are invalid, the caller lacks
    /// `cards:read`, the binding is absent, or the read fails.
    pub(super) async fn mcp_get_binding(
        &self,
        caller: Caller,
        arguments: Option<JsonMap<String, JsonValue>>,
    ) -> Result<CallToolResult, WyrdError> {
        let args: GetBindingArgs = parse_args(arguments, GET_BINDING)?;
        structured(
            &VerificationControl::new(&self.state)
                .get_binding(&caller, args.binding_id)
                .await?,
        )
    }

    /// Read one verification run's status.
    ///
    /// # Errors
    /// Returns a Wyrd error when the arguments are invalid, the caller lacks
    /// `cards:read`, the run is absent, or the read fails.
    pub(super) async fn mcp_get_run(
        &self,
        caller: Caller,
        arguments: Option<JsonMap<String, JsonValue>>,
    ) -> Result<CallToolResult, WyrdError> {
        let args: GetRunArgs = parse_args(arguments, GET_RUN)?;
        structured(
            &VerificationControl::new(&self.state)
                .get_run(&caller, args.run_id)
                .await?,
        )
    }

    /// Durably enqueue one manual Drift run.
    ///
    /// Removes the optional `idempotency_key` argument, validates it with the
    /// shared key contract, and decodes the rest exactly as the HTTP body.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] for absent arguments or an invalid
    /// key, and every error of [`VerificationControl::start_run`].
    pub(super) async fn mcp_start_run(
        &self,
        caller: Caller,
        arguments: Option<JsonMap<String, JsonValue>>,
    ) -> Result<CallToolResult, WyrdError> {
        let mut arguments = arguments.ok_or_else(|| WyrdError::Validation {
            message: format!("{START_RUN} requires arguments"),
            details: serde_json::json!({ "tool": START_RUN }),
        })?;
        let key =
            match arguments.remove("idempotency_key") {
                None | Some(JsonValue::Null) => None,
                Some(JsonValue::String(key)) => Some(IdempotencyKey::new(key).map_err(
                    |error| WyrdError::Validation {
                        message: format!("idempotency_key is invalid: {error}"),
                        details: serde_json::json!({ "tool": START_RUN }),
                    },
                )?),
                Some(_) => {
                    return Err(WyrdError::Validation {
                        message: "idempotency_key must be a string".to_owned(),
                        details: serde_json::json!({ "tool": START_RUN }),
                    });
                }
            };
        let request = decode_start_request(JsonValue::Object(arguments))?;
        let run_id = VerificationControl::new(&self.state)
            .start_run(&caller, &request, key.as_ref())
            .await?;
        structured(&StartVerificationRunResponse { run_id })
    }
}
