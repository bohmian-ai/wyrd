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

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use skald_spec::MediaFormat;

use crate::card::agent::AgentSpec;
use crate::card::drift::DriftSpec;
use crate::card::prompt::ParameterName;
use crate::ids::FeatureName;
use crate::reference::{InlineableRef, Ref};
use crate::vala::eval::result::EvalContextCapture;
use crate::vala::eval::{ComparisonOperator, JsonPath, TaskId};

pub mod extract;
pub mod reference;
mod schema_path;
mod validate;

pub use reference::{PathSegment, Reference, ReferenceError, ReferenceRoot, VerdictField};
pub use validate::ResolvedAgent;

/// Most retries one step may declare after its first attempt.
pub const MAX_STEP_RETRIES: u32 = 5;

/// A subject-less Verifier: inputs, optional steps, requirements, decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VerifierSpec {
    /// Optional human-readable purpose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Named evidence slots, addressed as `inputs.<slot>`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<crate::ids::SlotName, InputSlot>,
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
    pub context_capture: Option<EvalContextCapture>,
}

/// One named evidence slot. Slots are required unless `required: false`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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
