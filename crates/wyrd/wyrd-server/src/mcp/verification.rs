//! Verification control-plane capabilities over MCP.
//!
//! Typed projections of the same server operations HTTP serves: the
//! `verification.*` tools read binding and execution status, plan evidence
//! uploads, verify now, and enqueue a background execution. Authorization,
//! audit, tenancy, idempotency, and error mapping stay with
//! [`VerificationControl`], so this module only parses arguments and
//! projects answers.

use rmcp::model::{CallToolResult, Tool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Value as JsonValue};
use wyrd_runtime::permission::{Action, Resource};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{BindingId, ExecutionId, IdempotencyKey};
use wyrd_spec::verification::VerificationBindingStatus;
use wyrd_spec::verification::execution::{
    EnqueueVerificationResponse, EvidencePlanRequest, EvidencePlanResponse,
    VerificationExecutionState, VerificationRequest, VerificationResult,
};

use super::principals::{parse_args, tool};
use super::{WyrdMcpHandler, structured};
use crate::components::auth::Caller;
use crate::components::verification::service::VerificationControl;

/// Wire name of the binding status read.
pub(super) const GET_BINDING: &str = "verification.get_binding";

/// Wire name of the execution status read.
pub(super) const GET_EXECUTION: &str = "verification.get_execution";

/// Wire name of the evidence upload plan.
pub(super) const EVIDENCE: &str = "verification.evidence";

/// Wire name of the direct execution.
pub(super) const EXECUTE: &str = "verification.execute";

/// Wire name of the background submission.
pub(super) const ENQUEUE: &str = "verification.enqueue";

/// Arguments of `verification.get_binding`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GetBindingArgs {
    /// Binding ID from the owner Card's `card.status.verification.binding_ids`.
    binding_id: BindingId,
}

/// Arguments of `verification.get_execution`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GetExecutionArgs {
    /// Execution ID returned by `verification.enqueue`.
    execution_id: ExecutionId,
}

/// Arguments of `verification.enqueue`: the HTTP body plus the optional
/// `Idempotency-Key` header value.
///
/// Unknown members are refused by the flattened request.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct EnqueueArgs {
    /// The exact Verifier, subject, optional Run, and input declarations.
    #[serde(flatten)]
    request: VerificationRequest,
    /// Optional retry key; a retry with the same key and request returns the
    /// same `execution_id`.
    #[serde(default)]
    idempotency_key: Option<String>,
}

/// The read tools every authenticated caller may be shown.
#[must_use]
pub(super) fn descriptors_unscoped() -> Vec<Tool> {
    vec![
        tool::<GetBindingArgs, VerificationBindingStatus>(
            GET_BINDING,
            "Read a verification binding",
            "Read one verification binding's exact owner, subject, and Verifier identities, \
             whether its owner is active, Verifier readiness, next scheduled execution, last \
             activation, and latest execution. Requires cards:read.",
            true,
        ),
        tool::<GetExecutionArgs, VerificationExecutionState>(
            GET_EXECUTION,
            "Read a verification execution",
            "Read one Verifier execution's status, terminal error, Operator dispatch statuses, \
             and its result once published. A just-enqueued execution may briefly be not found, \
             and a completed one may briefly lack its result; poll again. Requires cards:read.",
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
        tool::<EvidencePlanRequest, EvidencePlanResponse>(
            EVIDENCE,
            "Plan evidence uploads",
            "Declare files by SHA-256 and length before verifying them and receive one direct \
             upload plan per file the server does not already hold. Requires verifier:run.",
            false,
        ),
        tool::<VerificationRequest, VerificationResult>(
            EXECUTE,
            "Verify now",
            "Execute one exact Verifier about one subject Card over named input declarations \
             (inline JSON, uploaded files by digest, Artifact files, or a trace) and return the \
             verdict with its requirement and step breakdown within a 60-second deadline. A \
             failed verdict is a successful result; no Operator fires. A drift Verifier is \
             refused: it judges scheduled windows. Requires verifier:run on the Verifier.",
            false,
        ),
        tool::<EnqueueArgs, EnqueueVerificationResponse>(
            ENQUEUE,
            "Enqueue a verification",
            "Stage one background execution of an exact Verifier over the same input \
             declarations as verification.execute and return its execution_id without waiting. \
             Poll verification.get_execution. Requires verifier:run on the Verifier.",
            false,
        ),
    ]
}

/// Whether this caller's token carries any `verifier:run` grant.
///
/// Coarse admission only: a grant of one Verifier UID shows the tools, and
/// the service still authorizes the exact Verifier each call selects.
pub(super) fn may_run_verifiers(caller: &Caller) -> bool {
    caller
        .principal
        .effective_permissions
        .covers_operation(&Resource::Verifier, &Action::Run)
}

impl WyrdMcpHandler {
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

    /// Read one verification execution's status.
    ///
    /// # Errors
    /// Returns a Wyrd error when the arguments are invalid, the caller lacks
    /// `cards:read`, the execution is not visible, or a read fails.
    pub(super) async fn mcp_get_execution(
        &self,
        caller: Caller,
        arguments: Option<JsonMap<String, JsonValue>>,
    ) -> Result<CallToolResult, WyrdError> {
        let args: GetExecutionArgs = parse_args(arguments, GET_EXECUTION)?;
        structured(
            &VerificationControl::new(&self.state)
                .get_execution(&caller, args.execution_id)
                .await?,
        )
    }

    /// Run one verification write tool: `evidence`, `execute`, or `enqueue`.
    ///
    /// Decodes the arguments exactly as the HTTP body; `enqueue` also takes
    /// the optional `idempotency_key`, validated with the shared key
    /// contract.
    ///
    /// # Errors
    /// Returns [`WyrdError::VerificationInputInvalid`] for malformed
    /// arguments, [`WyrdError::Validation`] for an invalid key, and every
    /// error of the named [`VerificationControl`] operation.
    pub(super) async fn mcp_verification_write(
        &self,
        name: &str,
        caller: Caller,
        arguments: Option<JsonMap<String, JsonValue>>,
    ) -> Result<CallToolResult, WyrdError> {
        let control = VerificationControl::new(&self.state);
        let arguments = JsonValue::Object(arguments.unwrap_or_default());
        let invalid = |error: serde_json::Error| WyrdError::VerificationInputInvalid {
            message: format!("{name} arguments are invalid: {error}"),
            details: serde_json::json!({ "tool": name }),
        };
        match name {
            EVIDENCE => structured(
                &control
                    .evidence(
                        &caller,
                        &serde_json::from_value(arguments).map_err(invalid)?,
                    )
                    .await?,
            ),
            EXECUTE => structured(
                &control
                    .execute(
                        &caller,
                        &serde_json::from_value(arguments).map_err(invalid)?,
                        None,
                    )
                    .await?,
            ),
            _ => {
                let args: EnqueueArgs = serde_json::from_value(arguments).map_err(invalid)?;
                let key = args
                    .idempotency_key
                    .map(IdempotencyKey::new)
                    .transpose()
                    .map_err(|error| WyrdError::Validation {
                        message: format!("idempotency_key is invalid: {error}"),
                        details: serde_json::json!({ "tool": ENQUEUE }),
                    })?;
                structured(&control.enqueue(&caller, args.request, key.as_ref())?)
            }
        }
    }
}
