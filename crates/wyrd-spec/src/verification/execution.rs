//! Generic Verifier execution wire contracts.
//!
//! One request shape serves `POST /v1/verification/execute` (inline, `200`
//! with a [`VerificationResult`]) and `POST /v1/verification/enqueue` (`202`
//! with an [`EnqueueVerificationResponse`]). Files are declared by digest
//! through `POST /v1/verification/evidence` before either call, and
//! `GET /v1/verification/executions/{execution_id}` reports
//! [`VerificationExecutionState`]. Execution status stays separate from the
//! verdict: a failed requirement is a normal result, never a transport error.
//! Every result read returns the compact [`VerificationResult`];
//! `GET /v1/verification/executions/{execution_id}/details` alone returns
//! the descriptive [`VerificationDetails`].

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

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
use crate::card::agent::{Authored, BuiltinTool, Compiled, ExecutionRoute, PromptForm};
use crate::card::verifier::{Check, CodeRuntime};
use crate::error::WyrdError;
use crate::ids::{CardUid, ExecutionId, GroupId, RepositoryId, SlotName};
use crate::reference::CardRef;
use crate::storage::ids::UploadId;
use crate::storage::upload::UploadPlan;
use crate::vala::ids::{RunId, TraceId};
use crate::verification::{PublicationName, TaskId};

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
    /// The invoking workload's deployment environment: an optional,
    /// non-empty correlation tag copied onto the accepted execution and its
    /// result. It never selects, filters, or changes the execution and is
    /// never permission.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deployment_environment: Option<String>,
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
/// the server does not already hold as completed File uploads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct EvidencePlanResponse {
    /// One upload per missing file; completed files are omitted.
    pub uploads: Vec<EvidenceUpload>,
}

/// How to upload one missing file through the shared storage lifecycle.
///
/// Transfer the bytes by `plan`, then finish with
/// `POST /v1/cards/upload/{upload_id}/complete`; multipart part URLs come
/// from `POST /v1/cards/upload/{upload_id}/part-url`. Completion succeeds
/// once the File's inventory record is acknowledged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct EvidenceUpload {
    /// Digest of the file this plan uploads.
    #[cfg_attr(feature = "server", schema(value_type = String))]
    pub sha256: Sha256Hex,
    /// Upload session to transfer into and complete.
    pub upload_id: UploadId,
    /// The upload plan.
    pub plan: UploadPlan,
}

/// `202 Accepted` body of `POST /v1/verification/enqueue`. It confirms
/// durable admission; a replay under the same key names the original
/// execution.
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
    /// The pull request source a `pull_request` binding execution was
    /// admitted for, frozen at admission.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull_request: Option<PullRequestActivation>,
}

/// One side of a pull request as an admission pinned it: the branch name
/// then and the exact commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct PullRequestRef {
    /// Branch name when the source was admitted.
    #[serde(rename = "ref")]
    pub ref_name: String,
    /// Exact commit SHA the execution reviews.
    pub sha: String,
}

/// The pull request source one queued execution was admitted for.
///
/// Every execution of one admission shares its `generation`. A newer
/// source of the same pull request supersedes the generation: its unfinished
/// executions are cancelled with code `pull_request_superseded`. A close or
/// draft conversion cancels them with `pull_request_closed` or
/// `pull_request_draft`, and losing the repository through removal,
/// suspension or disconnect with `repository_access_lost`. Settled history
/// keeps these pins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct PullRequestActivation {
    /// Provider web host, such as `github.com`.
    pub host: String,
    /// Stable provider id of the base repository.
    pub repository_id: RepositoryId,
    /// Pull request number in the base repository.
    pub number: u64,
    /// Base branch and commit.
    pub base: PullRequestRef,
    /// Head branch and commit.
    pub head: PullRequestRef,
    /// Stable provider id of the head repository; differs from
    /// `repository_id` for a fork and is absent once the fork is deleted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_repository_id: Option<RepositoryId>,
    /// Provider delivery that admitted the source, for matching the
    /// provider's delivery log.
    pub delivery_id: String,
    /// The admission every execution of this source shares.
    pub generation: uuid::Uuid,
}

/// The attributable decision of one Verifier execution.
///
/// The compact default every execution, status, and wait read returns: the
/// verdict and each requirement's judgment, without the check configuration,
/// provenance, steps, or evidence that [`VerificationDetails`] describes.
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
    /// The deployment environment tag the execution was invoked with, or
    /// absent when the invocation carried none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deployment_environment: Option<String>,
    /// Verdict and requirement judgments.
    #[cfg_attr(feature = "server", schema(value_type = VerificationOutcome))]
    pub outcome: VerificationOutcome<T>,
}

/// Overall verdict with its requirement judgments.
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
    /// Zero-based fanout item of the owning step's invocation this instance
    /// judged; absent without fanout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_index: Option<u32>,
    /// Zero-based repeat iteration of the owning step's invocation this
    /// instance judged; absent without repeat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iteration_index: Option<u32>,
    /// The requirement statement.
    pub statement: String,
    /// The outcome.
    pub outcome: RequirementOutcome,
    /// Why the outcome holds.
    pub reason: String,
    /// The compared actual value, when capture permits it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual: Option<Value>,
    /// Validated original-evidence citations: the original slot and locator,
    /// never a promise that the input body is echoed or fetchable.
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
}

/// `GET /v1/verification/executions/{execution_id}/details` response: what
/// one execution ran and how it judged.
///
/// The same identity and judgments as its [`VerificationResult`], plus each
/// requirement's exact authored check and provenance, every step, and the
/// pinned evidence declarations. `T` is the custom-data type, as in
/// [`VerificationResult`]. `F` is the inline Agent Prompt form: a read
/// keeps the [`Authored`] form and never compiles, renders, or executes a
/// Prompt; an engine records the [`Compiled`] form it ran.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(bound(
    serialize = "T: Serialize",
    deserialize = "T: serde::de::DeserializeOwned"
))]
#[schemars(rename = "VerificationDetails", bound = "T: JsonSchema, F: PromptForm")]
#[cfg_attr(feature = "server", schema(bound = ""))]
pub struct VerificationDetails<T = Map<String, Value>, F: PromptForm = Authored> {
    /// Server-issued execution identity.
    pub execution_id: ExecutionId,
    /// The caller's application Run, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
    /// The exact Verifier executed.
    pub verifier: CardRef,
    /// The exact subject judged.
    pub subject: CardRef,
    /// The deployment environment tag the execution was invoked with, or
    /// absent when the invocation carried none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deployment_environment: Option<String>,
    /// Verdict, requirement and step breakdown, and evidence.
    #[cfg_attr(feature = "server", schema(value_type = OutcomeDetails))]
    pub outcome: OutcomeDetails<T, F>,
}

/// Overall verdict with its full requirement and step breakdown and the
/// pinned evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(bound(
    serialize = "T: Serialize",
    deserialize = "T: serde::de::DeserializeOwned"
))]
#[schemars(rename = "OutcomeDetails", bound = "T: JsonSchema, F: PromptForm")]
#[cfg_attr(feature = "server", schema(bound = ""))]
pub struct OutcomeDetails<T = Map<String, Value>, F: PromptForm = Authored> {
    /// The `decide` rule applied to every applicable requirement.
    pub verdict: VerificationVerdict,
    /// Every requirement, step requirements included.
    #[cfg_attr(feature = "server", schema(value_type = Vec<RequirementDetails>))]
    pub requirements: Vec<RequirementDetails<T, F>>,
    /// Every step in plan order.
    pub steps: Vec<StepResult>,
    /// The pinned original evidence, by slot.
    pub evidence: BTreeMap<SlotName, InputDeclaration>,
}

/// The judgment of one requirement with the exact check that produced it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(bound(
    serialize = "T: Serialize",
    deserialize = "T: serde::de::DeserializeOwned"
))]
#[schemars(rename = "RequirementDetails", bound = "T: JsonSchema, F: PromptForm")]
#[cfg_attr(feature = "server", schema(bound = ""))]
pub struct RequirementDetails<T = Map<String, Value>, F: PromptForm = Authored> {
    /// The judgment, exactly as the compact result carries it.
    #[serde(flatten)]
    #[cfg_attr(feature = "server", schema(value_type = RequirementResult))]
    pub result: RequirementResult<T>,
    /// The exact check configuration that ran.
    #[schemars(with = "Check")]
    #[cfg_attr(feature = "server", schema(value_type = Check))]
    pub check: Check<F>,
    /// What executed the check, for Agent, code, and drift checks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<CallProvenance>,
}

/// The full outcome an engine produces and records: details holding each
/// check in the compiled form it ran.
pub type ExecutedOutcome = OutcomeDetails<Map<String, Value>, Compiled>;

/// One requirement of an [`ExecutedOutcome`].
pub type ExecutedRequirement = RequirementDetails<Map<String, Value>, Compiled>;

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

/// The committed settlement of one queued block invocation attempt.
///
/// A queued execution checkpoints each action and requirement it runs as
/// soon as the block settles, before the block's dependents run, so a
/// reclaimed execution restores the settlement instead of running the block
/// again. Skipped steps and requirements settled by a missing value perform
/// no IO and are never checkpointed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InvocationSettlement {
    /// A step action succeeded or failed after its attempts.
    Action {
        /// The reported step result, its output uncaptured.
        result: Box<StepResult>,
        /// Original slots the action read.
        sources: BTreeSet<SlotName>,
    },
    /// A requirement's check was judged.
    Requirement {
        /// The reported requirement with the exact check that judged it.
        result: Box<ExecutedRequirement>,
    },
}

/// The content kind one execution publication retains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum PublicationKind {
    /// One regular file.
    File,
    /// One directory tree.
    Directory,
    /// One bounded JSON value, `null` included.
    Json,
}

/// The producer invocation attempt that accepted a publication.
///
/// This is the host-assigned logical invocation identity: the step, its
/// zero-based fanout item and repeat iteration when those constructs exist,
/// and its one-based attempt. Authors never supply it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct Producer {
    /// The producing step.
    pub step_id: TaskId,
    /// Zero-based fanout item; `None` without fanout.
    pub item_index: Option<u32>,
    /// Zero-based repeat iteration; `None` without repeat.
    pub iteration_index: Option<u32>,
    /// One-based attempt.
    pub attempt: u32,
}

impl Producer {
    /// The one-based `attempt` of `step`, which neither fans out nor
    /// repeats.
    #[must_use]
    pub fn plain(step: &TaskId, attempt: u32) -> Self {
        Self {
            step_id: step.clone(),
            item_index: None,
            iteration_index: None,
            attempt,
        }
    }
}

/// One regular file a file or directory publication retains.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PublicationFile {
    /// Path relative to the publication root; a file publication's one entry
    /// is its basename.
    pub path: String,
    /// SHA-256 of the file's bytes, the verification File that holds them.
    pub sha256: Sha256Hex,
    /// File size in bytes.
    pub size_bytes: u64,
}

/// The immutable manifest of a file or directory publication.
///
/// Every byte it names was completed as a verification File before the
/// manifest committed, so a manifest never names partial content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PublicationManifest {
    /// Every regular file, sorted by path.
    pub files: Vec<PublicationFile>,
    /// Every directory below a directory publication's root, empty ones
    /// included, sorted; empty for a file.
    pub directories: Vec<String>,
}

/// The content one publication retains: a manifest of completed Files for
/// a file or directory, or a bounded JSON value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PublicationContent {
    /// One regular file; its manifest holds exactly one entry.
    File {
        /// The retained file.
        manifest: PublicationManifest,
    },
    /// One directory tree.
    Directory {
        /// The retained tree.
        manifest: PublicationManifest,
    },
    /// One JSON value, `null` included.
    Json {
        /// The retained value.
        value: Value,
    },
}

impl PublicationContent {
    /// The kind of content retained.
    #[must_use]
    pub const fn kind(&self) -> PublicationKind {
        match self {
            Self::File { .. } => PublicationKind::File,
            Self::Directory { .. } => PublicationKind::Directory,
            Self::Json { .. } => PublicationKind::Json,
        }
    }
}

/// One accepted, immutable execution publication.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct Publication {
    /// The logical name, unique within its producer attempt.
    pub name: PublicationName,
    /// The invocation attempt that accepted it.
    pub producer: Producer,
    /// The retained content.
    #[serde(flatten)]
    pub content: PublicationContent,
}

/// The `manifest.json` of a mounted `steps.<id>.publications` collection.
///
/// It reports the producer step's terminal status, every invocation attempt
/// with its status, and every admitted publication with the path its content
/// is mounted at beneath the collection root, so a consumer reads failed or
/// skipped producers honestly instead of inferring them from absent files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PublicationCollection {
    /// The producer step's terminal status.
    pub status: StepStatus,
    /// Why the step failed or was skipped; `null` when it succeeded.
    pub reason: Option<String>,
    /// The step's speculative disposition.
    pub speculation: Speculation,
    /// Every invocation attempt, ordered by item, iteration, then attempt;
    /// empty when the step never ran.
    pub invocations: Vec<ProducerInvocation>,
    /// Every admitted publication, grouped like `invocations` and in
    /// acceptance order within an attempt.
    pub entries: Vec<CollectionEntry>,
}

/// A step's speculative disposition, as a collection manifest reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum Speculation {
    /// The step is not speculative.
    None,
    /// The step ran before its condition resolved; its contribution is
    /// withheld until the condition settles.
    Pending,
    /// The condition held; the contribution is admitted.
    Admitted,
    /// The condition was false; unfinished work was cancelled and the
    /// contribution is excluded from ordinary consumers.
    Discarded,
    /// The condition could not be evaluated; nothing is admitted.
    Blocked,
}

impl Speculation {
    /// Whether ordinary consumers may read the step's contribution.
    #[must_use]
    pub const fn admits(self) -> bool {
        matches!(self, Self::None | Self::Admitted)
    }
}

/// One invocation attempt of a collection's producer step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ProducerInvocation {
    /// Zero-based fanout item; `null` without fanout.
    pub item_index: Option<u32>,
    /// Zero-based repeat iteration; `null` without repeat.
    pub iteration_index: Option<u32>,
    /// One-based attempt.
    pub attempt: u32,
    /// How the attempt ended: a retried attempt failed.
    pub status: StepStatus,
    /// Why the attempt failed; `null` when it succeeded.
    pub reason: Option<String>,
}

/// One admitted publication of a mounted collection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CollectionEntry {
    /// The logical name.
    pub name: PublicationName,
    /// Whether it mounts as a file, a directory, or a JSON file.
    pub kind: PublicationKind,
    /// The mounted file, JSON file, or directory root, relative to the
    /// collection root and always beneath `content/`. Its segments are
    /// opaque: read them here rather than constructing them.
    pub path: String,
    /// Zero-based fanout item of the producer; `null` without fanout.
    pub item_index: Option<u32>,
    /// Zero-based repeat iteration of the producer; `null` without repeat.
    pub iteration_index: Option<u32>,
    /// One-based producer attempt.
    pub attempt: u32,
}

/// Page size an execution listing uses when `limit` is omitted.
pub const EXECUTION_PAGE_DEFAULT: u32 = 50;

/// Most entries one execution listing page returns.
pub const EXECUTION_PAGE_MAX: u32 = 200;

/// Query of the paginated execution listings,
/// `GET /v1/verification/executions/{execution_id}/invocations` and
/// `/publications`.
///
/// The first page fixes the listing's cut: every page of one listing,
/// followed through `next_cursor`, reads exactly the entries committed
/// before the first page was read, in commit order, with no gap or
/// duplicate. Entries committed later appear when a new listing is started.
/// A cursor binds its tenant, execution, and listing and confers no
/// authority, which every page re-checks.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::IntoParams, utoipa::ToSchema))]
#[cfg_attr(feature = "server", into_params(parameter_in = Query))]
#[serde(deny_unknown_fields)]
pub struct ExecutionPageQuery {
    /// Page size, 1–200; 50 when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 200))]
    pub limit: Option<u32>,
    /// Opaque continuation from the previous page's `next_cursor`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1, max = 2048))]
    pub cursor: Option<String>,
}

/// How one invocation attempt stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum InvocationProgress {
    /// Dispatched and not settled: still running, or abandoned when its
    /// execution is terminal.
    Started,
    /// A step action settled.
    Action {
        /// How the action ended.
        status: StepStatus,
    },
    /// A requirement was judged.
    Requirement {
        /// The judgment.
        outcome: RequirementOutcome,
    },
}

/// One checkpointed invocation attempt of a queued execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ExecutionInvocation {
    /// The step or requirement, item, iteration, and attempt.
    pub producer: Producer,
    /// Whether the attempt is running or how it settled.
    #[serde(flatten)]
    pub progress: InvocationProgress,
    /// Why a settled attempt failed, was skipped, or was judged as it was;
    /// `null` otherwise.
    pub reason: Option<String>,
    /// The attempt's speculative disposition.
    pub speculation: Speculation,
}

/// How far one execution's history has been projected into
/// `bifrost.verification.execution_events`.
///
/// Projection lag delays history only: invocation progress and publications
/// read from the execution's records whatever this reports.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct HistoryProjection {
    /// Events Scribe acknowledged.
    pub acknowledged: u64,
    /// Events not yet submitted.
    pub pending: u64,
    /// Events submitted without an acknowledgement, to be resubmitted under
    /// the same batch identity.
    pub failed: u64,
}

/// `GET /v1/verification/executions/{execution_id}/invocations` response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct InvocationPage {
    /// The execution.
    pub execution_id: ExecutionId,
    /// Its current lifecycle status.
    pub status: VerificationExecutionStatus,
    /// The page's invocation attempts, in commit order.
    pub items: Vec<ExecutionInvocation>,
    /// Continuation of this listing; `null` on its last page.
    pub next_cursor: Option<String>,
    /// The execution's history projection position.
    pub history: HistoryProjection,
}

/// One accepted publication as a listing names it, without its content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PublicationSummary {
    /// The logical name, unique within its producer attempt.
    pub name: PublicationName,
    /// The content kind retained.
    pub kind: PublicationKind,
    /// The invocation attempt that accepted it.
    pub producer: Producer,
    /// Its producer's speculative disposition.
    pub speculation: Speculation,
}

/// `GET /v1/verification/executions/{execution_id}/publications` response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PublicationPage {
    /// The execution.
    pub execution_id: ExecutionId,
    /// The page's publications, in acceptance order.
    pub items: Vec<PublicationSummary>,
    /// Continuation of this listing; `null` on its last page.
    pub next_cursor: Option<String>,
}

/// Producer selection of one publication read or download, the query of
/// `GET` and `POST /v1/verification/executions/{execution_id}/publications/{step_id}/{name}[/download]`.
///
/// It names the exact producer attempt a listing reports, never the latest
/// attempt.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::IntoParams, utoipa::ToSchema))]
#[cfg_attr(feature = "server", into_params(parameter_in = Query))]
#[serde(deny_unknown_fields)]
pub struct PublicationSelector {
    /// One-based producer attempt.
    #[schemars(range(min = 1))]
    pub attempt: u32,
    /// Zero-based fanout item; omitted without fanout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_index: Option<u32>,
    /// Zero-based repeat iteration; omitted without repeat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iteration_index: Option<u32>,
    /// For a directory download, the regular file's path relative to the
    /// publication root, as its manifest lists it; omitted otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// `POST /v1/verification/executions/{execution_id}/cancel` response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CancelExecutionResponse {
    /// The execution.
    pub execution_id: ExecutionId,
    /// Its status after the cancellation fence: `cancelled` for work that
    /// was unfinished, otherwise the terminal status it already had.
    pub status: VerificationExecutionStatus,
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
            pull_request,
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
            pull_request,
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
            deployment_environment,
            outcome,
        } = self;
        Ok(VerificationResult {
            execution_id,
            run_id,
            verifier,
            subject,
            deployment_environment,
            outcome: VerificationOutcome {
                verdict: outcome.verdict,
                requirements: outcome
                    .requirements
                    .into_iter()
                    .map(RequirementResult::decode_data)
                    .collect::<Result<_, _>>()?,
            },
        })
    }
}

impl VerificationDetails {
    /// These details with every present requirement `data` decoded as `T`,
    /// with the [`VerificationResult::decode_data`] semantics.
    ///
    /// # Errors
    /// Returns the [`VerificationResult::decode_data`] error.
    pub fn decode_data<T: DeserializeOwned>(self) -> Result<VerificationDetails<T>, WyrdError> {
        let Self {
            execution_id,
            run_id,
            verifier,
            subject,
            deployment_environment,
            outcome,
        } = self;
        Ok(VerificationDetails {
            execution_id,
            run_id,
            verifier,
            subject,
            deployment_environment,
            outcome: OutcomeDetails {
                verdict: outcome.verdict,
                requirements: outcome
                    .requirements
                    .into_iter()
                    .map(|requirement| {
                        Ok(RequirementDetails {
                            result: requirement.result.decode_data()?,
                            check: requirement.check,
                            provenance: requirement.provenance,
                        })
                    })
                    .collect::<Result<_, WyrdError>>()?,
                steps: outcome.steps,
                evidence: outcome.evidence,
            },
        })
    }
}

impl<T, F: PromptForm> OutcomeDetails<T, F> {
    /// The compact decision of this outcome: its verdict and each
    /// requirement's judgment, without checks, provenance, steps, or
    /// evidence.
    #[must_use]
    pub fn compact(self) -> VerificationOutcome<T> {
        VerificationOutcome {
            verdict: self.verdict,
            requirements: self
                .requirements
                .into_iter()
                .map(|requirement| requirement.result)
                .collect(),
        }
    }
}

impl<T, F: PromptForm> VerificationDetails<T, F> {
    /// The compact [`VerificationResult`] of these details: the same
    /// identity and judgments.
    #[must_use]
    pub fn compact(self) -> VerificationResult<T> {
        VerificationResult {
            execution_id: self.execution_id,
            run_id: self.run_id,
            verifier: self.verifier,
            subject: self.subject,
            deployment_environment: self.deployment_environment,
            outcome: self.outcome.compact(),
        }
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
            item_index: self.item_index,
            iteration_index: self.iteration_index,
            statement: self.statement,
            outcome: self.outcome,
            reason: self.reason,
            actual: self.actual,
            citations: self.citations,
            data,
            detail: self.detail,
        })
    }
}

/// The generated closed Verdict schema without `data`, the plain `verdict`
/// format every expanded one extends. Generated once: it depends only on the
/// Verdict type, and every compiled Prompt and decoded inline Agent check
/// resolves it.
///
/// # Panics
///
/// Panics on first use if the generated schema is not an object declaring
/// `properties`, an invariant of the derived schema.
static PLAIN_VERDICT: LazyLock<Map<String, Value>> = LazyLock::new(|| {
    let Ok(mut generated) = serde_json::to_value(crate::schema::schema_for::<Verdict>()) else {
        unreachable!("a generated root schema serializes to a JSON object");
    };
    portable(&mut generated);
    let Value::Object(mut schema) = generated else {
        unreachable!("a generated root schema serializes to a JSON object");
    };
    if let Some(required) = schema.get_mut("required").and_then(Value::as_array_mut) {
        required.retain(|name| name != "data");
    }
    schema
        .get_mut("properties")
        .and_then(Value::as_object_mut)
        .expect("the generated Verdict schema declares properties")
        .remove("data");
    schema
});

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
        let mut schema = PLAIN_VERDICT.clone();
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

    /// Narrows a resolved Verdict schema's citation slots to `slots`, the
    /// bare slot names one check invocation can reach.
    ///
    /// Hosts apply it to a per-call copy before the model runs, so the
    /// provider request and local validation admit the same slots and a
    /// wrong slot is repaired rather than judged. An empty set, or a schema
    /// without the generated `SlotName` definition, is left unchanged;
    /// final evidence validation still decides every citation.
    pub fn restrict_slots(schema: &mut Value, slots: &BTreeSet<SlotName>) {
        if slots.is_empty() {
            return;
        }
        if let Some(slot) = schema.pointer_mut("/$defs/SlotName") {
            *slot = serde_json::json!({
                "type": "string",
                "enum": slots.iter().map(SlotName::as_str).collect::<Vec<_>>(),
            });
        }
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

/// Projects a generated schema onto the subset OpenAI strict mode,
/// Anthropic, and Gemini all accept, without changing what it admits beyond
/// numeric ranges.
///
/// `oneOf` becomes `anyOf`, which is equivalent for the disjoint closed
/// alternatives the generator emits, and an alternation of `const` values
/// becomes one `enum`. Siblings of `$ref`, `format`, and `minimum` are
/// dropped; typed decoding still enforces the numeric ranges they stated.
/// Keys of `properties` and `$defs` are names, never keywords. It runs on
/// the generated envelope only, before any authored `data` schema is
/// embedded.
fn portable(schema: &mut Value) {
    let Value::Object(node) = schema else {
        return;
    };
    if node.contains_key("$ref") {
        node.retain(|key, _| key == "$ref");
        return;
    }
    node.remove("format");
    node.remove("minimum");
    if let Some(Value::Array(alternatives)) = node.remove("oneOf") {
        let constants: Option<Vec<Value>> = alternatives
            .iter()
            .map(|alternative| alternative.get("const").cloned())
            .collect();
        let kind = alternatives
            .first()
            .and_then(|first| first.get("type"))
            .cloned();
        match constants {
            Some(values)
                if alternatives
                    .iter()
                    .all(|alternative| alternative.get("type") == kind.as_ref()) =>
            {
                if let Some(kind) = kind {
                    node.insert("type".to_owned(), kind);
                }
                node.insert("enum".to_owned(), Value::Array(values));
            }
            _ => {
                node.insert("anyOf".to_owned(), Value::Array(alternatives));
            }
        }
    }
    for (keyword, value) in node.iter_mut() {
        match (keyword.as_str(), value) {
            ("properties" | "$defs", Value::Object(named)) => named.values_mut().for_each(portable),
            (_, Value::Array(items)) => items.iter_mut().for_each(portable),
            (_, value) => portable(value),
        }
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

    /// Details with every locator kind, drift detail, provenance, tool calls
    /// with and without returned output, and a skipped step keep their
    /// documented wire form; records without a returned size, naming a
    /// non-built-in tool, or with a non-canonical digest are refused.
    ///
    /// # Panics
    /// Panics when the documented details do not round-trip.
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
        let parsed: VerificationDetails =
            serde_json::from_value(result.clone()).expect("documented details");
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

    /// The full stored record of an execution judging every outcome kind:
    /// failed custom data, drift detail, citations, an inline Agent check
    /// whose named `response_format` only a compiler resolves, steps, and
    /// evidence.
    fn recorded(prompt: &Value) -> Value {
        json!({
            "execution_id": "0195a4b8-3c1d-7e00-8000-000000000001",
            "run_id": "run-1",
            "verifier": {"kind": "Verifier", "name": "judge", "version": "1.0.0"},
            "subject": {"kind": "Agent", "name": "drafter", "version": "1.0.0"},
            "deployment_environment": "prod",
            "outcome": {
                "verdict": "failed",
                "requirements": [{
                    "id": "grounded", "statement": "Cites the policy", "outcome": "failed",
                    "reason": "no citation", "actual": {"score": 0.25},
                    "citations": [{"slot": "policy", "locator": {"page": 2}}],
                    "data": {"score": 0.25},
                    "check": {"kind": "agent", "agent": {
                        "prompt": prompt,
                        "execution_route": {"kind": "wyrd_gateway", "deployment": "judge"}
                    }, "inputs": {"answer": "inputs.answer"}},
                    "provenance": {"kind": "agent", "route": {"kind": "wyrd_gateway", "deployment": "judge"}}
                }, {
                    "id": "present", "statement": "Has an answer", "outcome": "passed",
                    "reason": "present",
                    "check": {"kind": "assert", "path": "inputs.answer", "operator": "is_not_null"}
                }, {
                    "id": "named", "statement": "Judged by the registered Agent",
                    "outcome": "inconclusive", "reason": "provider refused",
                    "check": {"kind": "agent", "agent": {"kind": "Agent", "name": "judge", "version": "1.0.0"}}
                }, {
                    "id": "stable", "step": "structure", "statement": "Inputs are stable",
                    "outcome": "not_applicable", "reason": "step skipped",
                    "detail": {"drift": {"method": "Psi", "features": {}, "verdict": "NoDrift"}},
                    "check": {"kind": "assert", "path": "inputs.answer", "operator": "is_not_null"}
                }],
                "steps": [{"id": "structure", "status": "skipped", "attempts": 0, "duration_ms": 0, "reason": "when was false"}],
                "evidence": {"answer": {"kind": "json", "value": {"text": "yes"}}}
            }
        })
    }

    /// The portable inline Prompt [`recorded`] judges with, declaring custom
    /// data through the named `verdict` format.
    fn portable_prompt() -> Value {
        json!({
            "provider": "openai", "model": "gpt-4o",
            "messages": [{"role": "user", "content": "{{answer}}"}],
            "response_format": {"kind": "verdict", "data": {
                "type": "object", "properties": {"score": {"type": "number"}},
                "required": ["score"], "additionalProperties": false
            }}
        })
    }

    /// The compact result keeps identity and every judgment field, custom
    /// data and drift detail included, for each outcome kind, and omits the
    /// check, provenance, steps, and evidence as absent keys rather than
    /// nulls. Projecting stored details and decoding the stored record
    /// directly agree; typed data decodes failed requirements and refuses a
    /// mismatch without changing the decision. The published schema
    /// declares no configuration field.
    ///
    /// # Panics
    /// Panics when a retained field is lost, an omitted field is present, or
    /// the projections disagree.
    #[test]
    fn compact_results_preserve_judgments_without_configuration() {
        /// The caller's model of the judge's custom data.
        #[derive(Debug, PartialEq, Deserialize)]
        struct Judge {
            /// The judge's score.
            score: f64,
        }

        let record = recorded(&portable_prompt());
        let mut expected = record.clone();
        let outcome = expected["outcome"].as_object_mut().expect("outcome");
        outcome.remove("steps");
        outcome.remove("evidence");
        for requirement in outcome["requirements"]
            .as_array_mut()
            .expect("requirements")
        {
            let fields = requirement.as_object_mut().expect("requirement");
            fields.remove("check");
            fields.remove("provenance");
        }
        let projected = serde_json::from_value::<VerificationDetails>(record.clone())
            .expect("details")
            .compact();
        assert_eq!(
            serde_json::to_value(&projected).expect("serialize"),
            expected
        );
        let direct: VerificationResult =
            serde_json::from_value(record).expect("a compact read of the full record");
        assert_eq!(direct, projected, "both projections agree");
        let outcomes: Vec<_> = projected
            .outcome
            .requirements
            .iter()
            .map(|requirement| requirement.outcome)
            .collect();
        assert_eq!(
            outcomes,
            [
                RequirementOutcome::Failed,
                RequirementOutcome::Passed,
                RequirementOutcome::Inconclusive,
                RequirementOutcome::NotApplicable,
            ]
        );

        let typed = projected.clone().decode_data::<Judge>().expect("typed");
        assert_eq!(
            typed.outcome.requirements[0].data,
            Some(Judge { score: 0.25 })
        );
        assert_eq!(typed.outcome.verdict, projected.outcome.verdict);
        assert!(projected.decode_data::<BTreeMap<String, String>>().is_err());

        let schema = serde_json::to_value(crate::schema::schema_for::<VerificationResult>())
            .expect("schema");
        let definitions = &schema["$defs"];
        let names = |definition: &str| -> BTreeSet<String> {
            definitions[definition]["properties"]
                .as_object()
                .expect(definition)
                .keys()
                .cloned()
                .collect()
        };
        assert_eq!(
            names("VerificationOutcome"),
            BTreeSet::from(["verdict".to_owned(), "requirements".to_owned()])
        );
        assert!(names("RequirementResult").is_disjoint(&BTreeSet::from([
            "check".to_owned(),
            "provenance".to_owned()
        ])));
    }

    /// Details keep each inline Agent's authored Prompt, portable and
    /// native, and a registered Agent reference exactly as recorded, and
    /// agree with the compact result. Decoding never compiles: a Prompt
    /// naming a format no compiler resolves still describes what was
    /// recorded, while the executable form refuses it. Structural integrity
    /// is still checked.
    ///
    /// # Panics
    /// Panics when the details change a recorded field, compile a Prompt, or
    /// admit a malformed record.
    #[test]
    fn execution_details_preserve_authored_checks_without_compilation() {
        let native = json!({
            "provider": "anthropic", "model": "claude", "format": "messages",
            "body": {"max_tokens": 64, "messages": [{"role": "user", "content": [{"type": "text", "text": "{{answer}}"}]}]},
            "response_format": "verdict"
        });
        let unresolvable = {
            let mut prompt = portable_prompt();
            prompt["response_format"] = json!("not-a-format");
            prompt
        };
        for prompt in [portable_prompt(), native, unresolvable.clone()] {
            let record = recorded(&prompt);
            let details: VerificationDetails =
                serde_json::from_value(record.clone()).expect("details decode as recorded");
            assert_eq!(serde_json::to_value(&details).expect("serialize"), record);
            let Check::Agent(call) = &details.outcome.requirements[0].check else {
                panic!("an Agent check stays an Agent check");
            };
            assert_eq!(
                call.agent
                    .as_inline()
                    .map(|agent| agent.prompt.as_inline().is_some()),
                Some(true),
                "the inline Agent keeps its inline authored Prompt"
            );
            assert_eq!(
                details.outcome.requirements[2].check,
                serde_json::from_value(record["outcome"]["requirements"][2]["check"].clone())
                    .expect("registered Agent reference")
            );
            let compact: VerificationResult = serde_json::from_value(record).expect("compact");
            assert_eq!(details.compact(), compact, "identity and judgments agree");
        }
        assert!(
            serde_json::from_value::<
                VerificationDetails<Map<String, Value>, crate::card::agent::Compiled>,
            >(recorded(&unresolvable))
            .is_err(),
            "the executable form compiles, and refuses the unresolvable format"
        );
        let mut malformed = recorded(&portable_prompt());
        malformed["outcome"]["requirements"][1]["check"]["extra"] = json!(true);
        assert!(serde_json::from_value::<VerificationDetails>(malformed).is_err());
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

    /// The plain Verdict schema is the portable structural target once
    /// titles and descriptions are removed: an outcome enum, closed citation
    /// objects, and five disjoint closed locator shapes under `anyOf`, with
    /// no `oneOf`, `const`, numeric annotations, or `$ref` siblings. The
    /// schema accepts every locator shape and refuses wrong outcomes,
    /// missing or extra fields, and several locator keys, while values the
    /// portable schema admits but `u32` cannot hold still fail decoding.
    ///
    /// # Panics
    /// Panics when the schema differs from the target or a value is judged
    /// on the wrong side.
    #[test]
    fn plain_verdict_schema_is_portable() {
        /// Refuses a non-portable keyword or a `$ref` sibling anywhere.
        ///
        /// # Panics
        /// Panics at the first node holding `oneOf`, `const`, `minimum`,
        /// `format`, or a `$ref` with siblings.
        fn portable(value: &Value) {
            match value {
                Value::Object(map) => {
                    assert!(!map.contains_key("$ref") || map.len() == 1, "{map:?}");
                    for keyword in ["oneOf", "const", "minimum", "format"] {
                        assert!(!map.contains_key(keyword), "{keyword} in {map:?}");
                    }
                    map.values().for_each(portable);
                }
                Value::Array(items) => items.iter().for_each(portable),
                _ => {}
            }
        }
        /// The schema with every `title` and `description` removed.
        fn structure(value: &Value) -> Value {
            match value {
                Value::Object(map) => Value::Object(
                    map.iter()
                        .filter(|(key, _)| !matches!(key.as_str(), "title" | "description"))
                        .map(|(key, value)| (key.clone(), structure(value)))
                        .collect(),
                ),
                Value::Array(items) => Value::Array(items.iter().map(structure).collect()),
                other => other.clone(),
            }
        }
        let schema = Value::Object(
            Verdict::response_format(&NamedResponseFormat::Name(VERDICT_FORMAT.to_owned()))
                .expect("resolves"),
        );
        let integer = || json!({"type": "integer"});
        let number = || json!({"type": "number"});
        let closed = |key: &str, value: Value| json!({"type": "object", "properties": {key: value}, "required": [key], "additionalProperties": false});
        let target = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "properties": {
                "outcome": {"$ref": "#/$defs/VerificationVerdict"},
                "reason": {"type": "string"},
                "citations": {"type": "array", "items": {"$ref": "#/$defs/Citation"}}
            },
            "additionalProperties": false,
            "required": ["outcome", "reason", "citations"],
            "$defs": {
                "VerificationVerdict": {"type": "string", "enum": ["passed", "failed", "inconclusive"]},
                "Citation": {
                    "type": "object",
                    "properties": {
                        "slot": {"$ref": "#/$defs/SlotName"},
                        "locator": {"$ref": "#/$defs/Locator"}
                    },
                    "additionalProperties": false,
                    "required": ["slot", "locator"]
                },
                "SlotName": {"type": "string"},
                "Locator": {"anyOf": [
                    closed("page", integer()),
                    closed("slide", integer()),
                    closed("lines", json!({
                        "type": "object",
                        "properties": {"start": integer(), "end": integer()},
                        "required": ["start", "end"],
                        "additionalProperties": false
                    })),
                    closed("json_pointer", json!({"type": "string"})),
                    closed("region", json!({
                        "type": "object",
                        "properties": {
                            "page": integer(), "x": number(), "y": number(),
                            "width": number(), "height": number()
                        },
                        "required": ["page", "x", "y", "width", "height"],
                        "additionalProperties": false
                    })),
                ]}
            }
        });
        assert_eq!(structure(&schema), target, "{schema:#}");
        portable(&schema);
        let validator = jsonschema::validator_for(&schema).expect("valid schema");
        let verdict = |outcome: Value, locator: Value| json!({"outcome": outcome, "reason": "r", "citations": [{"slot": "order", "locator": locator}]});
        for locator in [
            json!({"page": 1}),
            json!({"slide": 2}),
            json!({"lines": {"start": 1, "end": 3}}),
            json!({"json_pointer": "/a"}),
            json!({"region": {"page": 1, "x": 0.5, "y": 1, "width": 2, "height": 3}}),
        ] {
            let value = verdict(json!("failed"), locator);
            assert!(validator.is_valid(&value), "{value}");
            serde_json::from_value::<Verdict>(value.clone())
                .unwrap_or_else(|error| panic!("{value}: {error}"));
        }
        for value in [
            verdict(json!("FAIL"), json!({"page": 1})),
            verdict(json!("passed"), json!({"page": 1, "slide": 2})),
            verdict(json!("passed"), json!({"lines": {"start": 1}})),
            verdict(json!("passed"), json!({"page": 1, "extra": 1})),
            json!({"outcome": "passed", "reason": "r"}),
            json!({"outcome": "passed", "reason": "r", "citations": [], "extra": 1}),
            json!({"outcome": "passed", "reason": "r", "citations": [{"slot": "order"}]}),
        ] {
            assert!(!validator.is_valid(&value), "{value}");
        }
        for page in [json!(-1), json!(4_294_967_296_u64)] {
            let value = verdict(json!("passed"), json!({"page": page}));
            assert!(validator.is_valid(&value), "{value}");
            assert!(
                serde_json::from_value::<Verdict>(value.clone()).is_err(),
                "{value}"
            );
        }
    }

    /// An expanded format embeds the authored `data` schema with only the
    /// private `$id` added: its `oneOf`, `const`, numeric and `format`
    /// keywords, optional fields, and local references keep their meaning
    /// while the envelope around it is portable.
    ///
    /// # Panics
    /// Panics when an authored keyword changes or the data is not required.
    #[test]
    fn expanded_verdict_keeps_authored_data_schema() {
        let data = json!({
            "type": "object",
            "properties": {
                "score": {"type": "integer", "minimum": 1, "format": "int32", "description": "kept"},
                "kind": {"oneOf": [{"const": "a"}, {"const": "b"}]},
                "item": {"$ref": "#/$defs/item", "description": "kept"}
            },
            "required": ["score"],
            "$defs": {"item": {"type": "string", "minLength": 2}}
        });
        let format: NamedResponseFormat =
            serde_json::from_value(json!({"kind": "verdict", "data": data})).expect("format");
        let schema = Value::Object(Verdict::response_format(&format).expect("resolves"));
        let mut embedded = data.clone();
        embedded["$id"] = json!(DATA_RESOURCE);
        assert_eq!(schema["properties"]["data"], embedded);
        assert!(
            schema["required"]
                .as_array()
                .expect("required")
                .contains(&json!("data"))
        );
        assert!(schema["$defs"]["Locator"].get("anyOf").is_some());
        let validator = jsonschema::validator_for(&schema).expect("valid schema");
        let verdict = |data: Value| json!({"outcome": "passed", "reason": "r", "citations": [], "data": data});
        assert!(validator.is_valid(&verdict(json!({"score": 1, "kind": "a", "item": "ok"}))));
        for data in [
            json!({"score": 0}),
            json!({"score": 1, "kind": "c"}),
            json!({"score": 1, "item": "x"}),
            json!({}),
        ] {
            assert!(!validator.is_valid(&verdict(data.clone())), "{data}");
        }
    }

    /// Restriction admits exactly the reachable bare slots, and an empty set
    /// leaves the schema unchanged.
    ///
    /// # Panics
    /// Panics when a slot outside the set, or a prefixed slot, is admitted.
    #[test]
    fn restrict_slots_admits_only_reachable_slots() {
        let plain = Value::Object(
            Verdict::response_format(&NamedResponseFormat::Name(VERDICT_FORMAT.to_owned()))
                .expect("resolves"),
        );
        let mut unchanged = plain.clone();
        Verdict::restrict_slots(&mut unchanged, &BTreeSet::new());
        assert_eq!(unchanged, plain);
        let mut schema = plain;
        let slots = ["order", "receipt"].map(|slot| SlotName::new(slot).expect(slot));
        Verdict::restrict_slots(&mut schema, &slots.into_iter().collect());
        let slot = jsonschema::validator_for(&schema["$defs"]["SlotName"]).expect("valid");
        assert!(slot.is_valid(&json!("order")) && slot.is_valid(&json!("receipt")));
        for wrong in ["inputs.order", "other", ""] {
            assert!(!slot.is_valid(&json!(wrong)), "{wrong}");
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
