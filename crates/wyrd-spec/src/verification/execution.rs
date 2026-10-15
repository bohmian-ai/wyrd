//! Generic Verifier execution wire contracts.
//!
//! One request shape serves `POST /v1/verification/execute` (inline, `200`
//! with a [`VerificationResult`]) and `POST /v1/verification/enqueue` (`202`
//! with an [`EnqueueVerificationResponse`]). Files are declared by digest
//! through `POST /v1/verification/evidence` before either call, and
//! `GET /v1/verification/executions/{execution_id}` reports
//! [`VerificationExecutionState`]. Execution status stays separate from the
//! verdict: a failed requirement is a normal result, never a transport error.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use skald_spec::MediaFormat;

use super::{
    DriftReport, DriftWindow, VerificationError, VerificationExecutionStatus, VerificationVerdict,
};
use crate::auth::Sha256Hex;
use crate::card::agent::{BuiltinTool, ExecutionRoute};
use crate::card::blocks::{Check, CodeRuntime};
use crate::error::WyrdError;
use crate::ids::{CardUid, ExecutionId, SlotName};
use crate::reference::CardRef;
use crate::storage::upload::UploadPlan;
use crate::vala::eval::ids::TaskId;
use crate::vala::ids::{RunId, TraceId};

/// `POST /v1/verification/execute` and `POST /v1/verification/enqueue`
/// request body: the exact Verifier and subject plus one declaration per
/// named input slot. Tenant and principal come from authentication, never
/// from the body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VerificationRequest {
    /// Exact Verifier Card version to execute.
    pub verifier_card_uid: CardUid,
    /// Exact Card the judgment is about.
    pub card_uid: CardUid,
    /// The caller's application Run, when the execution belongs to one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
    /// Input declarations keyed by the Verifier's slot names.
    #[serde(default)]
    pub inputs: BTreeMap<SlotName, InputDeclaration>,
    /// Whole execution-route replacements keyed by Agent block ID.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub execution_route_overrides: BTreeMap<TaskId, ExecutionRoute>,
}

/// What a caller supplies for one input slot. Execution results echo the
/// pinned declarations as the original evidence identities.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum InputDeclaration {
    /// Bounded inline JSON for a `json` slot.
    Json {
        /// The supplied value.
        value: Value,
    },
    /// A file uploaded by digest through the evidence plan.
    File {
        /// SHA-256 of the file bytes.
        sha256: Sha256Hex,
        /// File length in bytes.
        size_bytes: u64,
        /// Declared canonical format.
        format: MediaFormat,
        /// Original filename, for display only.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filename: Option<String>,
    },
    /// A registered Artifact Card's file.
    Artifact {
        /// Exact Artifact Card reference.
        artifact: CardRef,
        /// File path inside the Artifact; required when it holds several.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<String>,
    },
    /// A recorded trace for a `trace` slot.
    Trace {
        /// The trace to judge.
        trace_id: TraceId,
    },
}

/// One file the caller intends to reference by digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvidenceFile {
    /// SHA-256 of the file bytes.
    pub sha256: Sha256Hex,
    /// File length in bytes.
    pub size_bytes: u64,
}

/// `POST /v1/verification/evidence` request body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvidencePlanRequest {
    /// Files the caller is about to declare.
    pub files: Vec<EvidenceFile>,
}

/// `POST /v1/verification/evidence` response: upload plans only for files
/// the server does not already hold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EvidencePlanResponse {
    /// One plan per missing file; present files are omitted.
    pub uploads: Vec<EvidenceUpload>,
}

/// How to upload one missing file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EvidenceUpload {
    /// Digest of the file this plan uploads.
    pub sha256: Sha256Hex,
    /// The upload plan.
    pub plan: UploadPlan,
}

/// `202 Accepted` body of `POST /v1/verification/enqueue`. It confirms
/// staging only; replay or conflict is decided when the queue accepts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EnqueueVerificationResponse {
    /// Identity to poll.
    pub execution_id: ExecutionId,
}

/// `GET /v1/verification/executions/{execution_id}` response.
///
/// A conflicting idempotent replay settles as `errored` with the
/// idempotency-conflict code and never carries a result. `completed` may
/// briefly lack a visible result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VerificationExecutionState {
    /// The execution.
    pub execution_id: ExecutionId,
    /// Lifecycle status, independent of the verdict.
    pub status: VerificationExecutionStatus,
    /// Terminal error of an `errored`, `cancelled`, or `timed_out` execution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<VerificationError>,
    /// The result once completed and visible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<VerificationResult>,
}

/// The attributable result of one Verifier execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VerificationResult {
    /// Server-issued execution identity.
    pub execution_id: ExecutionId,
    /// The caller's application Run, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
    /// The exact Verifier executed.
    pub verifier: CardRef,
    /// The exact subject judged.
    pub subject: CardRef,
    /// Verdict, breakdown, and evidence.
    pub outcome: VerificationOutcome,
}

/// Overall verdict with its requirement and step breakdown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VerificationOutcome {
    /// The `decide` rule applied to every applicable requirement.
    pub verdict: VerificationVerdict,
    /// Every requirement, step requirements included.
    pub requirements: Vec<RequirementResult>,
    /// Every step in plan order.
    pub steps: Vec<StepResult>,
    /// The pinned original evidence, by slot.
    pub evidence: BTreeMap<SlotName, InputDeclaration>,
}

/// The judgment of one requirement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RequirementResult {
    /// Requirement ID.
    pub id: TaskId,
    /// Owning step of a step requirement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<TaskId>,
    /// The requirement statement.
    pub statement: String,
    /// The outcome.
    pub outcome: RequirementOutcome,
    /// Why the outcome holds.
    pub reason: String,
    /// The compared actual value, when capture permits it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual: Option<Value>,
    /// Validated original-evidence citations.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub citations: Vec<Citation>,
    /// Check-specific detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<RequirementDetail>,
    /// The exact check configuration that ran.
    pub check: Check,
    /// What executed the check, for Agent, code, and drift checks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<CallProvenance>,
}

/// One requirement's outcome. Only `not_applicable` leaves the `decide`
/// denominator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RequirementOutcome {
    /// The requirement held.
    Passed,
    /// The requirement was violated.
    Failed,
    /// No judgment could be made; never a pass.
    Inconclusive,
    /// The requirement did not apply because its step was intentionally
    /// skipped.
    NotApplicable,
}

/// Check-specific requirement detail.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum RequirementDetail {
    /// Full per-feature statistics of a drift check.
    Drift(DriftReport),
}

/// What executed one Agent, code, extract, or drift block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CallProvenance {
    /// An Agent call.
    Agent {
        /// Exact Agent Card; absent when declared inline in the Verifier.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent: Option<CardRef>,
        /// Exact Prompt Card; absent when declared inline.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prompt: Option<CardRef>,
        /// The effective route after overrides.
        route: ExecutionRoute,
    },
    /// A sandboxed code call.
    Code {
        /// SHA-256 of the executed source.
        sha256: Sha256Hex,
        /// The runtime.
        runtime: CodeRuntime,
    },
    /// An extraction.
    Extract {
        /// Extractor version that produced the output.
        version: String,
    },
    /// A drift check.
    Drift {
        /// The scored sample window, for scheduled evidence.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        window: Option<DriftWindow>,
    },
}

/// The execution record of one step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StepResult {
    /// Step ID.
    pub id: TaskId,
    /// Final status, as `steps.<id>.status` reads it.
    pub status: StepStatus,
    /// The action output, when capture permits it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<Value>,
    /// Attempts made, retries included; zero when skipped.
    pub attempts: u32,
    /// Wall-clock duration across attempts.
    pub duration_ms: u64,
    /// Why the step failed or was skipped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// What executed the action.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<CallProvenance>,
    /// Tools an Agent action called, in call order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCallRecord>,
}

/// Final status of one step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    /// The action produced output.
    Succeeded,
    /// The action failed or timed out after its retries.
    Failed,
    /// The step did not run: its `when` was false or a dependency did not
    /// succeed.
    Skipped,
}

/// One tool call an Agent action made.
///
/// Records what the model received, never the raw tool output: the digest
/// and size describe the bounded output actually returned. A call that
/// returned no output, such as one that failed, has no digest and zero
/// returned bytes rather than a digest of nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ToolCallRecord {
    /// The built-in called.
    pub tool: BuiltinTool,
    /// Call arguments.
    pub arguments: Value,
    /// SHA-256 of the bounded output returned to the model; absent when the
    /// call returned none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_sha256: Option<Sha256Hex>,
    /// Bytes of output returned to the model; zero when it returned none.
    pub returned_bytes: u64,
    /// Error the tool returned, when it failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The closed schema every Agent and code check returns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Verdict {
    /// The judgment.
    pub outcome: VerificationVerdict,
    /// Why it holds.
    pub reason: String,
    /// Original-evidence locations supporting it; a pass without a valid
    /// citation becomes inconclusive.
    pub citations: Vec<Citation>,
}

/// A location in an original input slot, never in scratch space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Citation {
    /// The original input slot.
    pub slot: SlotName,
    /// Where in that slot.
    pub locator: Locator,
}

/// A typed location, serialized as a one-key object such as `{"page": 7}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Locator {
    /// One-based PDF page.
    Page(u32),
    /// One-based PPTX slide.
    Slide(u32),
    /// One-based inclusive text line range.
    Lines {
        /// First line.
        start: u32,
        /// Last line.
        end: u32,
    },
    /// RFC 6901 pointer into JSON input or canonical extracted JSON.
    JsonPointer(String),
    /// A rectangle on a one-based page, in page coordinates.
    Region {
        /// Page.
        page: u32,
        /// Left edge.
        x: f32,
        /// Top edge.
        y: f32,
        /// Width.
        width: f32,
        /// Height.
        height: f32,
    },
}

impl From<&WyrdError> for VerificationError {
    /// Record a refusal as the secret-free stable code and message an
    /// execution status carries, such as a delayed idempotency conflict.
    fn from(error: &WyrdError) -> Self {
        Self {
            code: error.code().to_owned(),
            message: error.message_details().0.into_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    //! Wire round trips and refusals for the Verifier execution contracts.

    use serde_json::json;

    use super::*;

    /// Every input kind and route override round-trips, and malformed
    /// declarations are refused.
    ///
    /// # Panics
    /// Panics when a request does not round-trip or a malformed one parses.
    #[test]
    fn verification_request_round_trips_every_input_kind() {
        let digest = "a".repeat(64);
        let uid = || CardUid::from_uuid(uuid::Uuid::now_v7()).expect("v7 uid");
        let request = json!({
            "verifier_card_uid": uid(),
            "card_uid": uid(),
            "inputs": {
                "answer": {"kind": "json", "value": {"text": "yes"}},
                "policy": {"kind": "file", "sha256": digest, "size_bytes": 10, "format": "pdf", "filename": "p.pdf"},
                "deck": {"kind": "artifact", "artifact": {"kind": "Artifact", "space": "legal", "name": "decks", "version": "1.0.0"}, "path": "q3.pptx"},
                "trace": {"kind": "trace", "trace_id": "0af7651916cd43dd8448eb211c80319c"}
            },
            "execution_route_overrides": {"judge": {"kind": "wyrd_gateway", "deployment": "eu-west"}}
        });
        let parsed: VerificationRequest =
            serde_json::from_value(request.clone()).expect("documented request");
        assert_eq!(serde_json::to_value(&parsed).expect("serialize"), request);
        for invalid in [
            json!({"kind": "file", "sha256": "A".repeat(64), "size_bytes": 1, "format": "pdf"}),
            json!({"kind": "file", "sha256": digest, "size_bytes": 1, "format": "exe"}),
            json!({"kind": "json", "value": 1, "extra": true}),
        ] {
            assert!(
                serde_json::from_value::<InputDeclaration>(invalid.clone()).is_err(),
                "{invalid}"
            );
        }
    }

    /// A result with every locator kind, drift detail, provenance, tool calls
    /// with and without returned output, and a skipped step keeps its
    /// documented wire form; records without a returned size, naming a
    /// non-built-in tool, or with a non-canonical digest are refused.
    ///
    /// # Panics
    /// Panics when the documented result does not round-trip.
    #[test]
    fn verification_result_round_trips_documented_outcome() {
        let result = json!({
            "execution_id": ExecutionId::new_v7(),
            "verifier": {"kind": "Verifier", "space": "legal", "name": "msa-policy", "version": "1.0.0"},
            "subject": {"kind": "Agent", "space": "legal", "name": "drafter", "version": "1.0.0"},
            "outcome": {
                "verdict": "failed",
                "requirements": [{
                    "id": "liability-cap", "statement": "Cap at 12 months",
                    "outcome": "failed", "reason": "cap is 24 months",
                    "citations": [
                        {"slot": "contract", "locator": {"page": 7}},
                        {"slot": "deck", "locator": {"slide": 2}},
                        {"slot": "notes", "locator": {"lines": {"start": 1, "end": 3}}},
                        {"slot": "workbook", "locator": {"json_pointer": "/sheets/0/cells/1/value"}},
                        {"slot": "contract", "locator": {"region": {"page": 7, "x": 0.5, "y": 1.0, "width": 2.0, "height": 0.25}}}
                    ],
                    "check": {"kind": "assert", "path": "inputs.contract", "operator": "is_not_null"},
                    "provenance": {"kind": "agent", "route": {"kind": "wyrd_gateway", "deployment": "eu-west"}}
                }, {
                    "id": "stable", "statement": "Inputs are stable",
                    "outcome": "not_applicable", "reason": "step skipped",
                    "detail": {"drift": {"method": "Psi", "features": {}, "verdict": "NoDrift"}},
                    "check": {"kind": "assert", "path": "inputs.contract", "operator": "is_not_null"},
                    "step": "structure"
                }],
                "steps": [{
                    "id": "structure", "status": "skipped", "attempts": 0, "duration_ms": 0,
                    "reason": "when was false",
                    "tool_calls": [
                        {"tool": "fs.read", "arguments": {"path": "/inputs/a.pdf"}, "output_sha256": "c".repeat(64), "returned_bytes": 512},
                        {"tool": "fs.search", "arguments": {"query": "cap"}, "returned_bytes": 0, "error": "workspace read timed out"}
                    ]
                }],
                "evidence": {"contract": {"kind": "file", "sha256": "b".repeat(64), "size_bytes": 4, "format": "pdf"}}
            }
        });
        let parsed: VerificationResult =
            serde_json::from_value(result.clone()).expect("documented result");
        assert_eq!(serde_json::to_value(&parsed).expect("serialize"), result);
        assert!(
            serde_json::from_value::<Verdict>(
                json!({"outcome": "passed", "reason": "ok", "citations": [], "score": 1})
            )
            .is_err()
        );
        for record in [
            json!({"tool": "fs.read", "arguments": {}}),
            json!({"tool": "http.fetch", "arguments": {}, "returned_bytes": 0}),
            json!({"tool": "fs.read", "arguments": {}, "output_sha256": "C".repeat(64), "returned_bytes": 1}),
        ] {
            assert!(
                serde_json::from_value::<ToolCallRecord>(record.clone()).is_err(),
                "{record}"
            );
        }
    }

    /// A delayed idempotency conflict is reported with the existing stable
    /// code on the execution status.
    ///
    /// # Panics
    /// Panics when the code or message is not carried over.
    #[test]
    fn verification_error_carries_idempotency_conflict() {
        let error = VerificationError::from(&WyrdError::RegistryIdempotencyConflict {
            message: "key reused with different content".to_owned(),
            details: json!({}),
        });
        assert_eq!(error.code, "WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT");
        assert_eq!(error.message, "key reused with different content");
    }
}
