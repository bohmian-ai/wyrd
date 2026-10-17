//! Verifier building blocks: named inputs, optional preparation steps,
//! requirements, and one decision rule.
//!
//! A [`VerifierSpec`] declares the evidence it needs as named input slots,
//! optionally prepares that evidence with `extract`, `agent`, or `code`
//! steps, and judges each declared requirement with one check. Step and
//! requirement IDs share one namespace across the whole Verifier and are
//! used unchanged in references, results, route overrides, and fitted drift
//! state. These values are declarative: resolution, execution, and storage
//! belong to their runtime owners.
//!
//! Subjects attach a Verifier through a [`VerificationBinding`] in their own
//! `verified_by` list; the binding is part of the containing Card version and
//! is never itself a Card.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use skald_spec::MediaFormat;

use crate::card::agent::AgentSpec;
use crate::card::drift::DriftSpec;
use crate::card::operator::OperatorSpec;
use crate::card::prompt::ParameterName;
use crate::card::trigger::TriggerSpec;
use crate::ids::{BindingId, FeatureName, SlotName};
use crate::reference::{CardRef, InlineableRef, Ref};
use crate::verification::ContextCapture;
use crate::verification::VerificationError;
use crate::verification::{ComparisonOperator, JsonPath, TaskId};

pub mod extract;
pub mod reference;
mod schema_path;
mod validate;

pub use reference::{
    PathSegment, Reference, ReferenceError, ReferenceRoot, SourceField, SourceReference,
    SourceText, VerdictField,
};
pub use validate::ResolvedAgent;

/// Most retries one step may declare after its first attempt.
pub const MAX_STEP_RETRIES: u32 = 5;

/// A subject-less Verifier: inputs, optional steps, requirements, decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct VerifierSpec {
    /// Optional human-readable purpose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Named evidence slots, addressed as `inputs.<slot>`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<SlotName, InputSlot>,
    /// Preparation steps; independent steps may run concurrently.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<Step>,
    /// Whole-Verifier requirements, judged after every step finishes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<Requirement>,
    /// How requirement outcomes combine into the verdict.
    #[serde(default, skip_serializing_if = "Decide::is_all")]
    pub decide: Decide,
    /// How permitted `actual` values are stored; absent means full capture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "server", schema(value_type = Option<Object>))]
    pub context_capture: Option<ContextCapture>,
}

impl VerifierSpec {
    /// Returns every Agent call with its authored check or action path, in
    /// step order and then Verifier-requirement order.
    ///
    /// Paths name the call itself, such as `spec.steps[0].action` or
    /// `spec.requirements[1].check`, so callers can append the field they
    /// report on.
    #[must_use]
    pub fn agent_calls(&self) -> Vec<(String, &AgentCall)> {
        /// Push each Agent check under `prefix`.
        fn checks<'a>(
            requirements: &'a [Requirement],
            prefix: &str,
            calls: &mut Vec<(String, &'a AgentCall)>,
        ) {
            for (i, requirement) in requirements.iter().enumerate() {
                if let Check::Agent(call) = &requirement.check {
                    calls.push((format!("{prefix}[{i}].check"), call));
                }
            }
        }
        let mut calls = Vec::new();
        for (i, step) in self.steps.iter().enumerate() {
            if let Action::Agent(call) = &step.action {
                calls.push((format!("spec.steps[{i}].action"), call));
            }
            checks(
                &step.requirements,
                &format!("spec.steps[{i}].requirements"),
                &mut calls,
            );
        }
        checks(&self.requirements, "spec.requirements", &mut calls);
        calls
    }

    /// Returns every requirement, step requirements first in step order,
    /// then the whole-Verifier requirements.
    pub fn all_requirements(&self) -> impl Iterator<Item = &Requirement> {
        self.steps
            .iter()
            .flat_map(|step| &step.requirements)
            .chain(&self.requirements)
    }

    /// Returns every drift check with its requirement ID; the ID keys the
    /// check's fitted baseline.
    pub fn drift_checks(&self) -> impl Iterator<Item = (&TaskId, &DriftCheck)> {
        self.all_requirements()
            .filter_map(|requirement| match &requirement.check {
                Check::Drift(drift) => Some((&requirement.id, drift)),
                _ => None,
            })
    }
}

/// One named evidence slot. Slots are required unless `required: false`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum InputSlot {
    /// A JSON value, optionally constrained by JSON Schema.
    Json {
        /// JSON Schema the value must satisfy.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        schema: Option<Value>,
        /// Whether a caller must supply the slot.
        #[serde(default = "required_default", skip_serializing_if = "is_true")]
        required: bool,
    },
    /// A pinned file in one of the accepted formats.
    Media {
        /// Accepted formats.
        #[cfg_attr(feature = "server", schema(value_type = Vec<String>))]
        formats: Vec<MediaFormat>,
        /// Lower size bound than the deployment limit.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_bytes: Option<u64>,
        /// Whether a caller must supply the slot.
        #[serde(default = "required_default", skip_serializing_if = "is_true")]
        required: bool,
    },
    /// A trace resolved through tenant-scoped trace reads.
    Trace {
        /// Whether a caller must supply the slot.
        #[serde(default = "required_default", skip_serializing_if = "is_true")]
        required: bool,
    },
    /// Feature measurements or metric samples recorded for a scheduled window.
    Samples {
        /// Measurement names, including metric names.
        features: Vec<FeatureName>,
        /// Whether the schedule binding must fill the slot.
        #[serde(default = "required_default", skip_serializing_if = "is_true")]
        required: bool,
    },
    /// A group Verifier's input authored as a source reference string,
    /// `verifiers.<name>.inputs.<slot>` or
    /// `verifiers.<name>.requirements.<requirement>.outcome`, resolved at
    /// group execution to one ordered value per source execution.
    #[serde(untagged)]
    Source(SourceText),
}

impl InputSlot {
    /// Returns whether a value must be supplied for this slot.
    #[must_use]
    pub fn required(&self) -> bool {
        match self {
            Self::Json { required, .. }
            | Self::Media { required, .. }
            | Self::Trace { required }
            | Self::Samples { required, .. } => *required,
            Self::Source(_) => true,
        }
    }
}

/// Serde default making slots required unless declared otherwise.
fn required_default() -> bool {
    true
}

/// Serde skip predicate omitting the default `required: true`.
#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde skip_serializing_if passes the field by reference"
)]
fn is_true(value: &bool) -> bool {
    *value
}

/// One preparation step: an action plus the requirements judged on it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct Step {
    /// Verifier-unique block ID.
    pub id: TaskId,
    /// The data-producing action.
    pub action: Action,
    /// Ordering-only dependencies beyond those implied by references.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<TaskId>,
    /// Gate: the step runs only when the condition holds; otherwise its
    /// requirements are not applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<Condition>,
    /// Requirements judged after the action, before dependents start.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<Requirement>,
    /// Per-attempt timeout in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u32>,
    /// Retries after the first failed attempt, at most [`MAX_STEP_RETRIES`].
    #[serde(default, skip_serializing_if = "is_zero")]
    pub retries: u32,
}

/// Serde skip predicate omitting zero retries.
#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde skip_serializing_if passes the field by reference"
)]
fn is_zero(value: &u32) -> bool {
    *value == 0
}

/// A data-producing step action. Actions read and compute only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    /// Extract text and structure from one media slot.
    Extract {
        /// The media slot, as `inputs.<slot>`.
        from: Reference,
    },
    /// Invoke an Agent; its output follows the Prompt's response schema.
    Agent(AgentCall),
    /// Run sandboxed code returning JSON data.
    Code(CodeCall),
}

/// One Agent invocation in a step or check position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct AgentCall {
    /// The Agent, inline, file-authored, or registered.
    pub agent: InlineableRef<AgentSpec>,
    /// Injected values keyed by the Prompt's text and media placeholders.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<ParameterName, Reference>,
    /// Files under `/inputs/...` for the Agent's tools; step position only.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub workspace: BTreeMap<String, Reference>,
}

/// One sandboxed code invocation in a step or check position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct CodeCall {
    /// Execution runtime.
    #[serde(default, skip_serializing_if = "CodeRuntime::is_default")]
    pub runtime: CodeRuntime,
    /// Immutable source.
    pub source: CodeSource,
    /// Function called with the bound inputs as keyword arguments.
    pub entrypoint: String,
    /// Values passed to the entrypoint.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<ParameterName, Reference>,
}

/// A code execution runtime.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum CodeRuntime {
    /// Sandboxed Python; the only supported runtime.
    #[default]
    Python,
}

impl CodeRuntime {
    /// Serde skip predicate omitting the default runtime.
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Immutable code source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum CodeSource {
    /// Source text embedded in the Verifier version.
    Inline(String),
    /// A file inside an exact Artifact Card version.
    Artifact {
        /// The Artifact Card.
        card: Ref,
        /// File path inside the Artifact, when it holds more than one file.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<String>,
    },
}

/// One declared requirement, producing exactly one verdict.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    /// Verifier-unique block ID.
    pub id: TaskId,
    /// The claim the check judges, readable as `requirement.statement`.
    pub statement: String,
    /// The verdict-producing check.
    pub check: Check,
}

/// A verdict-producing requirement check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Check {
    /// Typed comparison over one value (`path`) or every wildcard match
    /// (`each`); exactly one is set.
    Assert {
        /// One value.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<Reference>,
        /// Every value selected through `[*]`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        each: Option<Reference>,
        /// Comparison operator.
        operator: ComparisonOperator,
        /// Right-hand side of the comparison.
        #[serde(default, skip_serializing_if = "Value::is_null")]
        expected: Value,
    },
    /// Trace assertion over a trace slot.
    Trace(TraceCheck),
    /// A tool-less Agent returning the Verdict schema.
    Agent(AgentCall),
    /// Sandboxed code returning the Verdict schema.
    Code(CodeCall),
    /// Statistical drift over a samples slot.
    Drift(DriftCheck),
}

/// The evidence mode a check needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckEvidence {
    /// Evidence supplied with one execution request.
    Run,
    /// Samples recorded for a scheduled window.
    Window,
}

impl Check {
    /// Returns the evidence mode this check kind needs.
    #[must_use]
    pub fn evidence(&self) -> CheckEvidence {
        match self {
            Self::Drift(_) => CheckEvidence::Window,
            Self::Assert { .. } | Self::Trace(_) | Self::Agent(_) | Self::Code(_) => {
                CheckEvidence::Run
            }
        }
    }
}

/// Trace assertion over a declared trace slot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct TraceCheck {
    /// The trace slot, as `inputs.<slot>`.
    pub on: Reference,
    /// JSONPath into the assembled trace document.
    pub span_selector: JsonPath,
    /// Comparison operator.
    pub operator: ComparisonOperator,
    /// Right-hand side of the comparison.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected: Value,
}

/// Drift check: an existing [`DriftSpec`] over one samples slot.
///
/// The wire form flattens the drift fields beside `on`; the drift fields
/// keep their own unknown-field refusal.
#[derive(Debug, Clone, PartialEq, JsonSchema)]
pub struct DriftCheck {
    /// The samples slot, as `inputs.<slot>`.
    pub on: Reference,
    /// Method, signal, condition, and profile.
    #[schemars(flatten)]
    pub spec: DriftSpec,
}

#[cfg(feature = "server")]
impl utoipa::PartialSchema for DriftCheck {
    /// Describes the flattened wire form: every [`DriftSpec`] field plus the
    /// required `on` reference.
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        use utoipa::openapi::schema::{AllOfBuilder, ObjectBuilder};
        AllOfBuilder::new()
            .item(utoipa::openapi::Ref::from_schema_name("DriftSpec"))
            .item(
                ObjectBuilder::new()
                    .property("on", <Reference as utoipa::PartialSchema>::schema())
                    .required("on"),
            )
            .into()
    }
}

#[cfg(feature = "server")]
impl utoipa::ToSchema for DriftCheck {
    /// Registers the referenced [`DriftSpec`] component and its closure.
    fn schemas(
        schemas: &mut Vec<(
            String,
            utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
        )>,
    ) {
        schemas.push((
            <DriftSpec as utoipa::ToSchema>::name().into(),
            <DriftSpec as utoipa::PartialSchema>::schema(),
        ));
        <DriftSpec as utoipa::ToSchema>::schemas(schemas);
    }
}

impl Serialize for DriftCheck {
    /// Writes the drift spec's fields with `on` beside them as its reference
    /// text, the flattened wire form authors write.
    ///
    /// # Errors
    /// Returns the serializer's error, or a custom error when the drift spec
    /// does not serialize to JSON.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut value = serde_json::to_value(&self.spec).map_err(serde::ser::Error::custom)?;
        if let Value::Object(fields) = &mut value {
            fields.insert("on".to_owned(), Value::String(self.on.to_string()));
        }
        value.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DriftCheck {
    /// Splits the required `on` reference out of the flattened object and
    /// decodes the remaining fields as a [`DriftSpec`], which keeps its own
    /// unknown-field refusal.
    ///
    /// # Errors
    /// Returns a missing-field error when `on` is absent, and a custom error
    /// when `on` is not a valid reference or the remaining fields are not a
    /// valid drift spec.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut value = Value::deserialize(deserializer)?;
        let on = value
            .as_object_mut()
            .and_then(|fields| fields.remove("on"))
            .ok_or_else(|| D::Error::missing_field("on"))?;
        Ok(Self {
            on: Reference::deserialize(on).map_err(D::Error::custom)?,
            spec: DriftSpec::deserialize(value).map_err(D::Error::custom)?,
        })
    }
}

/// Step gate comparing one referenced value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct Condition {
    /// The compared value.
    pub path: Reference,
    /// Comparison operator.
    pub operator: ComparisonOperator,
    /// Right-hand side of the comparison.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected: Value,
}

/// How requirement outcomes combine. Not-applicable requirements are
/// excluded; no applicable requirement is inconclusive.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Decide {
    /// Any failure fails; otherwise any inconclusive is inconclusive.
    #[default]
    All,
    /// Passed share of applicable requirements, counting inconclusive ones,
    /// must reach `threshold` in `(0, 1]`.
    PassRate {
        /// Minimum passed share.
        threshold: f64,
    },
}

impl Decide {
    /// Serde skip predicate omitting the default rule.
    #[must_use]
    pub fn is_all(&self) -> bool {
        matches!(self, Self::All)
    }
}

/// One scheduled-evidence source selector in a binding's `inputs` map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ScheduledInput {
    /// Recorded feature measurements.
    Features(Vec<FeatureName>),
    /// One recorded metric.
    Metric(FeatureName),
}

impl ScheduledInput {
    /// Returns the measurement names the selector reads.
    #[must_use]
    pub fn names(&self) -> &[FeatureName] {
        match self {
            Self::Features(names) => names,
            Self::Metric(name) => std::slice::from_ref(name),
        }
    }
}

/// Reserved subject-occurrence key of a Service-level or standalone-Agent
/// verification binding.
///
/// A binding's natural key names its subject occurrence: a Service component
/// binding uses the component alias, and a binding on the owner itself uses
/// this value. Composition validation refuses it as a component alias, so the
/// owner and component domains of the key never collide and the key is never
/// null.
pub const OWNER_OCCURRENCE_KEY: &str = "$owner";

/// Server-derived verification state projected onto `card.status`.
///
/// A Service or standalone Agent owning `verified_by` bindings exposes the
/// stable identity of each projected binding here, so a caller can address a
/// binding without a separate listing resource. The field is read-only: it is
/// derived from the tenant's binding projection on every Card read and is
/// never persisted as Card status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct VerificationStatus {
    /// Stable identities of this owner Card version's projected bindings,
    /// ordered by identity so reordering authored bindings changes nothing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub binding_ids: Vec<BindingId>,
    /// Fitted-baseline readiness of a PSI or SPC Drift Verifier Card version.
    ///
    /// Absent on every other Card, including a Custom Drift Verifier, which
    /// scores its authored scalar baseline and is ready without fitting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline: Option<DriftBaselineStatus>,
}

impl VerificationStatus {
    /// Build the status a Card read serves, or `None` when it has nothing to report.
    #[must_use]
    pub fn derived(
        binding_ids: Vec<BindingId>,
        baseline: Option<DriftBaselineStatus>,
    ) -> Option<Self> {
        (!binding_ids.is_empty() || baseline.is_some()).then_some(Self {
            binding_ids,
            baseline,
        })
    }
}

/// Lifecycle of one PSI or SPC Verifier's server-fitted baseline.
///
/// Registration creates it `pending`; the fitter claims it `building` and
/// settles it `ready` with the persisted fitted profile or `failed` with a
/// structured error. A failed fit is retried through the same record while
/// attempts remain, so `failed` is visible without being necessarily final.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum DriftBaselineState {
    /// Registered and waiting for the fitter.
    Pending,
    /// Claimed by a fitter that is reading and fitting the baseline Data.
    Building,
    /// Fitted and persisted; runs may start.
    Ready,
    /// The last fit attempt failed; see the structured error.
    Failed,
}

impl DriftBaselineState {
    /// Stored and wire value of this state.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Building => "building",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }

    /// Decode a stored state value, or `None` outside the closed set.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "building" => Some(Self::Building),
            "ready" => Some(Self::Ready),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

/// Server-derived fitted-baseline status of a PSI or SPC Drift Verifier.
///
/// Served on `card.status.verification.baseline`; it never mutates the
/// authored spec.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct DriftBaselineStatus {
    /// Current fit lifecycle state.
    pub state: DriftBaselineState,
    /// Exact baseline Data Card the profile is fitted from, UID-pinned.
    pub data: CardRef,
    /// Structured error of the last failed fit attempt, when one failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<VerificationError>,
}

/// One inline declaration attaching a Verifier to its containing subject.
///
/// Authored on a Service, a Service component occurrence, or a standalone
/// Agent. Registration resolves and UID-pins the Verifier, the effective
/// Trigger, and every effective Operator before the owning Card persists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct VerificationBinding {
    /// The exact Verifier Card this subject is verified by.
    pub verifier: Ref,
    /// When the bound Verifier runs, inline or as a referenced Trigger Card.
    ///
    /// Absent means explicit-only: the binding attaches the Verifier to its
    /// subject for direct invocation and nothing activates it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runs_on: Option<InlineableRef<TriggerSpec>>,
    /// Operators dispatched independently after a `failed` verdict.
    ///
    /// Empty means a failed result stays durable with no reaction. A
    /// `group_end` binding refuses this list; it reacts through
    /// [`Self::on_verdict_change`] instead.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on_failure: Vec<InlineableRef<OperatorSpec>>,
    /// Operators dispatched independently when a settled group judgment's
    /// verdict differs from the preceding settled judgment of the same
    /// binding and group.
    ///
    /// Valid only on a `group_end` binding. The first judgment of a group is
    /// the baseline and dispatches nothing; an unchanged verdict dispatches
    /// nothing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on_verdict_change: Vec<InlineableRef<OperatorSpec>>,
    /// Scheduled evidence: the recorded measurements filling each samples
    /// slot of the Verifier within the subject's schedule window.
    ///
    /// Checked against the Verifier with [`VerifierSpec::validate_schedule`];
    /// names are never inferred.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<SlotName, ScheduledInput>,
}

impl VerificationBinding {
    /// Each authored Operator list beside its field name, in declaration
    /// order, so every validator and reference walker covers both reactions.
    #[must_use]
    pub fn operator_lists(&self) -> [(&'static str, &[InlineableRef<OperatorSpec>]); 2] {
        [
            ("on_failure", &self.on_failure),
            ("on_verdict_change", &self.on_verdict_change),
        ]
    }

    /// Each authored Operator list beside its field name, mutably, for
    /// reference pinning.
    pub fn operator_lists_mut(
        &mut self,
    ) -> [(&'static str, &mut Vec<InlineableRef<OperatorSpec>>); 2] {
        [
            ("on_failure", &mut self.on_failure),
            ("on_verdict_change", &mut self.on_verdict_change),
        ]
    }
}
