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
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use skald_spec::{MediaFormat, NamedResponseFormat, SkaldError, check_output_schema};

use super::{
    DriftReport, DriftWindow, GroupSnapshot, OperatorDispatchState, VerificationError,
    VerificationExecutionStatus, VerificationVerdict,
};
use crate::auth::Sha256Hex;
use crate::card::agent::{BuiltinTool, ExecutionRoute};
use crate::card::verifier::{Check, CodeRuntime};
use crate::error::WyrdError;
use crate::ids::{CardUid, ExecutionId, GroupId, SlotName};
use crate::reference::CardRef;
use crate::storage::upload::UploadPlan;
use crate::vala::ids::{RunId, TraceId};
use crate::verification::TaskId;

/// `POST /v1/verification/execute` and `POST /v1/verification/enqueue`
/// request body: the exact Verifier and subject plus one declaration per
/// named input slot. Tenant and principal come from authentication, never
/// from the body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct VerificationRequest {
    /// Exact Verifier Card version to execute.
    pub verifier_card_uid: CardUid,
    /// Exact Card the judgment is about.
    pub card_uid: CardUid,
    /// The caller's application Run, when the execution belongs to one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
    /// Caller-chosen group this execution belongs to, scoped to the tenant
    /// and authenticated owner Service. Grouped Verifier bindings discover
    /// executions sharing it; omitting it keeps the invocation ungrouped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<GroupId>,
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
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
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
        #[cfg_attr(feature = "server", schema(value_type = String))]
        sha256: Sha256Hex,
        /// File length in bytes.
        size_bytes: u64,
        /// Declared canonical format.
        #[cfg_attr(feature = "server", schema(value_type = String))]
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
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct EvidenceFile {
    /// SHA-256 of the file bytes.
    #[cfg_attr(feature = "server", schema(value_type = String))]
    pub sha256: Sha256Hex,
    /// File length in bytes.
    pub size_bytes: u64,
}

/// `POST /v1/verification/evidence` request body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct EvidencePlanRequest {
    /// Files the caller is about to declare.
    pub files: Vec<EvidenceFile>,
}

/// `POST /v1/verification/evidence` response: upload plans only for files
/// the server does not already hold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct EvidencePlanResponse {
    /// One plan per missing file; present files are omitted.
    pub uploads: Vec<EvidenceUpload>,
}

/// How to upload one missing file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct EvidenceUpload {
    /// Digest of the file this plan uploads.
    #[cfg_attr(feature = "server", schema(value_type = String))]
    pub sha256: Sha256Hex,
    /// The upload plan.
    pub plan: UploadPlan,
}

/// `202 Accepted` body of `POST /v1/verification/enqueue`. It confirms
/// staging only; replay or conflict is decided when the queue accepts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[schemars(rename = "VerificationExecutionState")]
#[cfg_attr(feature = "server", schema(bound = ""))]
pub struct VerificationExecutionState<T = Map<String, Value>> {
    /// The execution.
    pub execution_id: ExecutionId,
    /// Lifecycle status, independent of the verdict.
    pub status: VerificationExecutionStatus,
    /// Terminal error of an `errored`, `cancelled`, or `timed_out` execution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<VerificationError>,
    /// The result once completed and visible.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(default)]
    #[cfg_attr(feature = "server", schema(value_type = Option<VerificationResult>))]
    pub result: Option<VerificationResult<T>>,
    /// Every Operator dispatch of a failed background binding execution or a
    /// changed group verdict, in identity order; empty otherwise. Delivery
    /// completes independently of the execution and never changes its verdict.
    #[serde(default)]
    pub dispatches: Vec<OperatorDispatchState>,
    /// Caller group the execution belongs to, or the group a group execution
    /// judges.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<GroupId>,
    /// Binding owner of a background execution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_card_uid: Option<CardUid>,
    /// The source snapshot a group execution judges.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<GroupSnapshot>,
}

/// The attributable result of one Verifier execution.
///
/// `T` is the type of each requirement's custom `data`: JSON objects on the
/// wire, or a caller's type after a typed SDK read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[schemars(rename = "VerificationResult")]
#[cfg_attr(feature = "server", schema(bound = ""))]
pub struct VerificationResult<T = Map<String, Value>> {
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
    #[cfg_attr(feature = "server", schema(value_type = VerificationOutcome))]
    pub outcome: VerificationOutcome<T>,
}

/// Overall verdict with its requirement and step breakdown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[schemars(rename = "VerificationOutcome")]
#[cfg_attr(feature = "server", schema(bound = ""))]
pub struct VerificationOutcome<T = Map<String, Value>> {
    /// The `decide` rule applied to every applicable requirement.
    pub verdict: VerificationVerdict,
    /// Every requirement, step requirements included.
    #[cfg_attr(feature = "server", schema(value_type = Vec<RequirementResult>))]
    pub requirements: Vec<RequirementResult<T>>,
    /// Every step in plan order.
    pub steps: Vec<StepResult>,
    /// The pinned original evidence, by slot.
    pub evidence: BTreeMap<SlotName, InputDeclaration>,
}

/// The judgment of one requirement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[schemars(rename = "RequirementResult")]
#[cfg_attr(feature = "server", schema(bound = ""))]
pub struct RequirementResult<T = Map<String, Value>> {
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
    /// Custom data an Agent or code check returned under its declared
    /// `verdict` format; absent when none was declared or the output was
    /// rejected.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(
        default,
        schema_with = "crate::schema::skipped_option::<Map<String, Value>>"
    )]
    #[cfg_attr(feature = "server", schema(value_type = Option<Object>))]
    pub data: Option<T>,
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
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum RequirementDetail {
    /// Full per-feature statistics of a drift check.
    Drift(DriftReport),
}

/// What executed one Agent, code, extract, or drift block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
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
        #[cfg_attr(feature = "server", schema(value_type = String))]
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
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ToolCallRecord {
    /// The built-in called.
    pub tool: BuiltinTool,
    /// Call arguments.
    pub arguments: Value,
    /// SHA-256 of the bounded output returned to the model; absent when the
    /// call returned none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "server", schema(value_type = Option<String>))]
    pub output_sha256: Option<Sha256Hex>,
    /// Bytes of output returned to the model; zero when it returned none.
    pub returned_bytes: u64,
    /// Error the tool returned, when it failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Most bytes of compact UTF-8 JSON one Verdict's custom `data` may take.
pub const MAX_VERDICT_DATA_BYTES: usize = 65_536;

/// Most nested containers one Verdict's custom `data` may hold; the root
/// object counts as one.
pub const MAX_VERDICT_DATA_DEPTH: usize = 16;

/// The name of the generated Verdict response format.
pub const VERDICT_FORMAT: &str = "verdict";

/// The `$id` that makes a declared `data` schema its own resource inside the
/// Verdict envelope, relative to the envelope's base: the data's `#` then
/// names the data schema, not the envelope.
const DATA_RESOURCE: &str = "verdict-data";

/// The closed schema every Agent and code check returns.
///
/// `data` is present exactly when the check declares a custom-data schema
/// through the `verdict` response format; the server validates the whole
/// response against that declaration before decoding it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct Verdict {
    /// The judgment.
    pub outcome: VerificationVerdict,
    /// Why it holds.
    pub reason: String,
    /// Original-evidence locations supporting it; a pass without a valid
    /// citation becomes inconclusive.
    pub citations: Vec<Citation>,
    /// Custom data the check's declared format requires.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "crate::schema::skipped_option::<Map<String, Value>>")]
    pub data: Option<Map<String, Value>>,
}

impl VerificationExecutionState {
    /// This state with its result's custom data decoded as `T`; a state
    /// without a result converts unchanged.
    ///
    /// # Errors
    /// Returns the [`VerificationResult::decode_data`] error.
    pub fn decode_data<T: DeserializeOwned>(
        self,
    ) -> Result<VerificationExecutionState<T>, WyrdError> {
        let Self {
            execution_id,
            status,
            error,
            result,
            dispatches,
            group_id,
            owner_card_uid,
            group,
        } = self;
        Ok(VerificationExecutionState {
            execution_id,
            status,
            error,
            result: result.map(VerificationResult::decode_data).transpose()?,
            dispatches,
            group_id,
            owner_card_uid,
            group,
        })
    }
}

impl VerificationResult {
    /// This result with every present requirement `data` decoded as `T`.
    ///
    /// The SDKs' typed reads use it after the ordinary read: absent data
    /// stays absent, failed requirements decode too, and the stored result is
    /// never changed or re-executed. One mismatch fails the whole
    /// conversion, so no partially typed result is returned.
    ///
    /// # Errors
    /// Returns `WYRD_SDK_422_VERIFICATION_DATA_INVALID` naming the first
    /// requirement whose data does not decode, with the empty `path` of the
    /// data root and never the data itself: a decoder cannot tell a declared
    /// field name from a returned dictionary key, so no deeper path is safe.
    pub fn decode_data<T: DeserializeOwned>(self) -> Result<VerificationResult<T>, WyrdError> {
        let Self {
            execution_id,
            run_id,
            verifier,
            subject,
            outcome,
        } = self;
        let requirements = outcome
            .requirements
            .into_iter()
            .map(RequirementResult::decode_data)
            .collect::<Result<_, _>>()?;
        Ok(VerificationResult {
            execution_id,
            run_id,
            verifier,
            subject,
            outcome: VerificationOutcome {
                verdict: outcome.verdict,
                requirements,
                steps: outcome.steps,
                evidence: outcome.evidence,
            },
        })
    }
}

impl RequirementResult {
    /// This requirement with its present `data` decoded as `T`.
    ///
    /// # Errors
    /// Returns `WYRD_SDK_422_VERIFICATION_DATA_INVALID` with the requirement
    /// ID and the data root's empty path, never the data.
    fn decode_data<T: DeserializeOwned>(self) -> Result<RequirementResult<T>, WyrdError> {
        let data = match self.data {
            None => None,
            Some(data) => Some(serde_json::from_value(Value::Object(data)).map_err(|_| {
                WyrdError::SdkVerificationDataInvalid {
                    message: format!(
                        "requirement `{}` data does not match the requested type",
                        self.id.as_str()
                    ),
                    details: serde_json::json!({"requirement": self.id, "path": ""}),
                }
            })?),
        };
        Ok(RequirementResult {
            id: self.id,
            step: self.step,
            statement: self.statement,
            outcome: self.outcome,
            reason: self.reason,
            actual: self.actual,
            citations: self.citations,
            data,
            detail: self.detail,
            check: self.check,
            provenance: self.provenance,
        })
    }
}

impl Verdict {
    /// Resolves a `verdict` response format to the schema its responses
    /// must satisfy.
    ///
    /// The plain form is the generated closed Verdict without `data`; an
    /// expanded `data` schema becomes the required `data` member, embedded
    /// as its own JSON Schema 2020-12 resource under a private `$id` so its
    /// local `$ref`s resolve exactly as they do standalone. Data schemas are
    /// 2020-12 only and carry no root `$id`: an older draft or an authored
    /// identity could leave those references bound to the envelope. This is
    /// the resolver Prompt compilation and code-check validation share.
    ///
    /// # Errors
    ///
    /// Returns [`SkaldError::PromptInvalid`] at `response_format` for a name
    /// other than `verdict`; at `response_format.data.$schema` for a data
    /// schema declaring a draft other than 2020-12; at
    /// `response_format.data.$id` for one with a root `$id`; and at
    /// `response_format.data...` for one that is not a valid object JSON
    /// Schema.
    ///
    /// # Panics
    ///
    /// Panics if the generated Verdict schema has no `properties` object, an
    /// invariant of the derived schema.
    pub fn response_format(format: &NamedResponseFormat) -> Result<Map<String, Value>, SkaldError> {
        let invalid = |field: &str, message: &str| SkaldError::PromptInvalid {
            field: field.to_owned(),
            message: message.to_owned(),
        };
        if format.name() != VERDICT_FORMAT {
            return Err(invalid(
                "response_format",
                &format!(
                    "unknown response format `{}`; the named format is `verdict`",
                    format.name()
                ),
            ));
        }
        let Ok(Value::Object(mut schema)) =
            serde_json::to_value(crate::schema::schema_for::<Self>())
        else {
            unreachable!("a generated root schema serializes to a JSON object");
        };
        let required = |schema: &mut Map<String, Value>| {
            schema
                .get_mut("required")
                .and_then(Value::as_array_mut)
                .map(|required| required.retain(|name| name != "data"))
        };
        required(&mut schema);
        let properties = schema
            .get_mut("properties")
            .and_then(Value::as_object_mut)
            .expect("the generated Verdict schema declares properties");
        properties.remove("data");
        let Some(data) = format.data() else {
            return Ok(schema);
        };
        if let Some(dialect) = data.get("$schema")
            && !matches!(
                dialect.as_str(),
                Some(
                    "https://json-schema.org/draft/2020-12/schema"
                        | "https://json-schema.org/draft/2020-12/schema#"
                )
            )
        {
            return Err(invalid(
                "response_format.data.$schema",
                "a Verdict data schema is JSON Schema 2020-12; omit `$schema` or declare https://json-schema.org/draft/2020-12/schema",
            ));
        }
        if data.contains_key("$id") {
            return Err(invalid(
                "response_format.data.$id",
                "a Verdict data schema is embedded as its own resource; remove the root `$id`",
            ));
        }
        check_output_schema(&Value::Object(data.clone()), "response_format.data", true)?;
        let mut data = data.clone();
        data.insert("$id".to_owned(), Value::from(DATA_RESOURCE));
        schema["properties"]["data"] = Value::Object(data);
        schema
            .get_mut("required")
            .and_then(Value::as_array_mut)
            .expect("the generated Verdict schema requires fields")
            .push(Value::from("data"));
        Ok(schema)
    }

    /// Refuses custom `data` beyond the fixed size and depth bounds.
    ///
    /// Size is the compact UTF-8 JSON encoding of `data` alone, with only
    /// the escapes JSON requires; depth counts every object and array, the
    /// root included. The message names the violated limit, never the data.
    ///
    /// # Errors
    ///
    /// Returns the violated bound as a message when `data` exceeds
    /// [`MAX_VERDICT_DATA_BYTES`] or [`MAX_VERDICT_DATA_DEPTH`].
    pub fn check_data_bounds(&self) -> Result<(), String> {
        /// Containers in the deepest path through `value`.
        fn depth(value: &Value) -> usize {
            match value {
                Value::Object(map) => 1 + map.values().map(depth).max().unwrap_or(0),
                Value::Array(items) => 1 + items.iter().map(depth).max().unwrap_or(0),
                _ => 0,
            }
        }
        let Some(data) = &self.data else {
            return Ok(());
        };
        let bytes = serde_json::to_vec(data).map_or(usize::MAX, |encoded| encoded.len());
        if bytes > MAX_VERDICT_DATA_BYTES {
            return Err(format!(
                "verdict data is {bytes} bytes; the limit is {MAX_VERDICT_DATA_BYTES} bytes of compact JSON"
            ));
        }
        let nested = 1 + data.values().map(depth).max().unwrap_or(0);
        if nested > MAX_VERDICT_DATA_DEPTH {
            return Err(format!(
                "verdict data nests {nested} containers; the limit is {MAX_VERDICT_DATA_DEPTH}"
            ));
        }
        Ok(())
    }
}

/// A location in an original input slot, never in scratch space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct Citation {
    /// The original input slot.
    pub slot: SlotName,
    /// Where in that slot.
    pub locator: Locator,
}

/// A typed location, serialized as a one-key object such as `{"page": 7}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
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
                "deck": {"kind": "artifact", "artifact": {"kind": "Artifact", "name": "decks", "version": "1.0.0"}, "path": "q3.pptx"},
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
            "verifier": {"kind": "Verifier", "name": "msa-policy", "version": "1.0.0"},
            "subject": {"kind": "Agent", "name": "drafter", "version": "1.0.0"},
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

    /// Typed decoding converts every present requirement `data`, leaves
    /// absent data absent, and refuses a mismatch with the requirement and
    /// field path but never the payload, returning no partial result.
    ///
    /// # Panics
    /// Panics when a conversion succeeds or fails on the wrong side.
    #[test]
    fn typed_data_decodes_whole_results() {
        /// The caller's model of a scoring check's custom data, the type
        /// the typed read decodes each requirement's `data` into.
        #[derive(Debug, PartialEq, Deserialize)]
        struct Judge {
            /// The judge's score; a string here is the mismatch under test.
            score: f64,
        }
        let requirement = |id: &str, data: Value| {
            let mut requirement = json!({
                "id": id, "statement": "s", "outcome": "failed", "reason": "r",
                "check": {"kind": "assert", "path": "inputs.answer", "operator": "is_not_null"}
            });
            if !data.is_null() {
                requirement["data"] = data;
            }
            requirement
        };
        let result = |requirements: Vec<Value>| -> VerificationResult {
            serde_json::from_value(json!({
                "execution_id": ExecutionId::new_v7(),
                "verifier": {"kind": "Verifier", "name": "judge", "version": "1.0.0"},
                "subject": {"kind": "Agent", "name": "drafter", "version": "1.0.0"},
                "outcome": {"verdict": "failed", "requirements": requirements, "steps": [], "evidence": {}}
            }))
            .expect("result")
        };
        let typed = result(vec![
            requirement("scored", json!({"score": 0.25})),
            requirement("plain", Value::Null),
        ])
        .decode_data::<Judge>()
        .expect("typed");
        assert_eq!(
            typed.outcome.requirements[0].data,
            Some(Judge { score: 0.25 })
        );
        assert_eq!(typed.outcome.requirements[1].data, None);
        let error = result(vec![
            requirement("scored", json!({"score": 0.25})),
            requirement("secretive", json!({"score": "hunter2"})),
        ])
        .decode_data::<Judge>()
        .expect_err("mismatch");
        assert_eq!(error.code(), "WYRD_SDK_422_VERIFICATION_DATA_INVALID");
        assert_eq!(
            error.problem().details,
            json!({"requirement": "secretive", "path": ""})
        );
        assert!(!error.to_string().contains("hunter2"), "{error}");
        let error = result(vec![requirement(
            "keyed",
            json!({"outer": {"customer-secret@example.test": "wrong"}}),
        )])
        .decode_data::<std::collections::BTreeMap<String, std::collections::BTreeMap<String, u8>>>()
        .expect_err("mismatch under a returned key");
        let problem = serde_json::to_string(&error.problem()).expect("problem");
        assert!(!problem.contains("customer-secret"), "{problem}");
        assert!(!error.to_string().contains("customer-secret"), "{error}");
    }

    /// Custom Verdict data obeys the inclusive size and depth bounds: exact
    /// boundaries pass and one more byte or container fails, counting UTF-8
    /// bytes, required JSON escapes, and wide arrays, with messages naming
    /// the limit and never the data.
    ///
    /// # Panics
    /// Panics when a boundary value is judged on the wrong side.
    #[test]
    fn custom_verdict_data_obeys_limits() {
        let verdict = |data: Value| Verdict {
            outcome: VerificationVerdict::Passed,
            reason: "ok".to_owned(),
            citations: Vec::new(),
            data: Some(data.as_object().expect("object data").clone()),
        };
        let text = |unit: &str, count: usize, tail: &str| json!({"s": format!("{}{tail}", unit.repeat(count))});
        let wide = |last: u32| {
            let mut items = vec![json!(0); 32_763];
            items.push(json!(last));
            json!({"a": items})
        };
        let nested = |containers: usize| {
            let mut value = json!(1);
            for level in 1..containers {
                value = if level % 2 == 0 {
                    json!({"o": value})
                } else {
                    json!([value])
                };
            }
            json!({"r": value})
        };
        for (data, fits) in [
            (text("x", 65_528, ""), true),
            (text("x", 65_529, ""), false),
            (text("é", 32_764, ""), true),
            (text("é", 32_764, "x"), false),
            (text("\n", 32_764, ""), true),
            (text("\n", 32_764, "x"), false),
            (text("\u{1}", 10_921, "xx"), true),
            (text("\u{1}", 10_921, "xxx"), false),
            (wide(9), true),
            (wide(10), true),
            (wide(100), false),
            (nested(16), true),
            (nested(17), false),
        ] {
            let bytes = serde_json::to_vec(&data).expect("json").len();
            match verdict(data).check_data_bounds() {
                Ok(()) => assert!(fits, "{bytes} bytes must fail"),
                Err(message) => {
                    assert!(!fits, "{bytes} bytes must pass: {message}");
                    assert!(
                        message.contains("65536") || message.contains("16"),
                        "{message}"
                    );
                    assert!(
                        !message.contains("xxx") && !message.contains('é'),
                        "{message}"
                    );
                }
            }
        }
        assert_eq!(
            serde_json::to_vec(&wide(10)).expect("json").len(),
            MAX_VERDICT_DATA_BYTES
        );
        let plain = Verdict {
            data: None,
            ..verdict(json!({}))
        };
        assert_eq!(plain.check_data_bounds(), Ok(()));
        assert!(
            serde_json::to_value(&plain)
                .expect("json")
                .get("data")
                .is_none(),
            "absent data is omitted"
        );
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
