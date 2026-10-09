//! Card and Verification control-plane capabilities over MCP.
//!
//! Typed projections of the same server operations HTTP serves:
//! `cards.get` reads a Card (including `card.status.verification`), and the
//! four `verification.*` tools read binding and run status, request a manual
//! Drift run, and judge supplied input directly. Authorization, audit, tenancy, idempotency, and error
//! mapping stay with [`VerificationControl`] and the Cards read path, so this
//! module only parses arguments and projects answers. Queued verdicts are
//! read with the existing `bifrost.query` tool by `result_id`; a direct
//! execution returns its verdict.

use rmcp::model::{CallToolResult, Tool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Value as JsonValue};
use wyrd_runtime::permission::{Action, Resource};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{BindingId, CardUid, IdempotencyKey, VerificationRunId};
use wyrd_spec::registry::GetCardResponse;
use wyrd_spec::verification::{
    ExecuteVerificationRequest, Judgment, StartVerificationRunResponse, VerificationBindingStatus,
    VerificationRunInput, VerificationRunStatus, VerificationRunTarget,
};

use super::principals::{parse_args, tool};
use super::{WyrdMcpHandler, structured};
use crate::components::auth::Caller;
use crate::components::cards::routes::get_card_for;
use crate::components::verification::service::{VerificationControl, decode_start_request};

/// Wire name of the Card read.
pub(super) const CARDS_GET: &str = "cards.get";

/// Wire name of the binding status read.
pub(super) const GET_BINDING: &str = "verification.get_binding";

/// Wire name of the manual run request.
pub(super) const START_RUN: &str = "verification.start_run";

/// Wire name of the run status read.
pub(super) const GET_RUN: &str = "verification.get_run";

/// Wire name of the direct execution.
pub(super) const EXECUTE: &str = "verification.execute";

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

/// The write tools shown only to a caller holding any `verifier:run` grant.
///
/// A planning convenience, not the boundary: [`VerificationControl`]
/// authorizes `verifier:run` on the exact selected Verifier and audits that
/// decision when the tool is named directly.
#[must_use]
pub(super) fn write_descriptors() -> Vec<Tool> {
    vec![
        tool::<StartRunArgs, StartVerificationRunResponse>(
            START_RUN,
            "Start a manual verification run",
            "Durably enqueue one manual Drift run over a bounded UTC window [start, end) for a \
             binding or a direct Verifier/subject pair, and return its run_id without waiting \
             for scoring. Poll verification.get_run. Requires verifier:run on the selected \
             Verifier and, for a Card-bound caller, Card scope over the subject.",
            false,
        ),
        tool::<ExecuteVerificationRequest, Judgment>(
            EXECUTE,
            "Execute a Verifier on supplied input",
            "Judge supplied drift_samples, one eval_record, or one task_context with one exact \
             Verifier about one subject Card and return the verdict and detail (plus counts for \
             Drift and Eval) in this response, within a \
             60-second deadline. Nothing is enqueued, published, or dispatched; a failed verdict \
             is a successful result. PSI/SPC Verifiers need a ready fitted baseline; Eval trace \
             and agent assertions are unsupported. Requires verifier:run on the Verifier.",
            false,
        ),
    ]
}

/// Whether this caller's token carries any `verifier:run` grant.
///
/// Coarse admission only: a grant of one Verifier UID shows the tools, and
/// the service still authorizes the exact Verifier each call selects.
pub(super) fn may_start_runs(caller: &Caller) -> bool {
    caller
        .principal
        .effective_permissions
        .covers_operation(&Resource::Verifier, &Action::Run)
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

    /// Judge supplied input with one exact Verifier.
    ///
    /// Decodes the arguments exactly as the HTTP body.
    ///
    /// # Errors
    /// Returns [`WyrdError::VerificationInputInvalid`] for absent or malformed
    /// arguments, and every error of [`VerificationControl::execute`].
    pub(super) async fn mcp_execute(
        &self,
        caller: Caller,
        arguments: Option<JsonMap<String, JsonValue>>,
    ) -> Result<CallToolResult, WyrdError> {
        let request =
            ExecuteVerificationRequest::decode(JsonValue::Object(arguments.unwrap_or_default()))?;
        structured(
            &VerificationControl::new(&self.state)
                .execute(&caller, &request, None)
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
