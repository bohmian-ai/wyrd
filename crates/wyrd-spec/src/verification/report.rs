//! The optional standard code review report.
//!
//! A Prompt declaring `response_format: code_review` returns a
//! [`CodeReviewReport`] as its step's ordinary output; normal code may return
//! the same envelope. The report is review content, never a verdict: authored
//! requirements judge it like any other step output. After an execution
//! settles, [`select_report`] finds the one envelope its accepted successful
//! step outputs hold, so provider delivery can render it. Nothing here
//! performs IO.

use std::collections::BTreeSet;
use std::num::NonZeroU32;
use std::sync::LazyLock;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};
use skald_spec::{NamedResponseFormat, SkaldError};

use super::TaskId;
use super::execution::{StepResult, StepStatus, VERDICT_FORMAT, Verdict};
use crate::card::verifier::VerifierSpec;

/// The name of the standard code review response format and of its
/// envelope's `format` member.
pub const CODE_REVIEW_FORMAT: &str = "code_review";

/// The one supported report `version`.
pub const CODE_REVIEW_VERSION: u32 = 1;

/// The standard code review report: one review's summary, how much of its
/// scope it covered, and its findings.
///
/// Every member is present; an unlocated finding has empty location arrays
/// and an unavailable remediation is `null`. Severity and confidence describe
/// each finding and never decide a verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CodeReviewReport {
    /// Always `code_review`.
    pub format: CodeReviewFormat,
    /// Always `1`.
    pub version: ReportVersion,
    /// The review's overall summary.
    pub summary: String,
    /// What the review covered.
    pub coverage: Coverage,
    /// The findings, each with a report-unique `id`.
    pub findings: Vec<Finding>,
}

/// The report envelope discriminator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CodeReviewFormat {
    /// `code_review`.
    CodeReview,
}

/// The report version, always [`CODE_REVIEW_VERSION`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReportVersion;

impl Serialize for ReportVersion {
    /// Serializes as the integer `1`.
    ///
    /// # Errors
    /// Returns the serializer's error.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(CODE_REVIEW_VERSION)
    }
}

impl<'de> Deserialize<'de> for ReportVersion {
    /// Accepts exactly the integer `1`.
    ///
    /// # Errors
    /// Returns the deserializer's error for any other value.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match u64::deserialize(deserializer)? {
            1 => Ok(Self),
            other => Err(serde::de::Error::custom(format!(
                "unsupported code_review version {other}; the supported version is 1"
            ))),
        }
    }
}

impl JsonSchema for ReportVersion {
    /// The schema's name in generated definitions.
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ReportVersion".into()
    }

    /// An integer that can only be [`CODE_REVIEW_VERSION`].
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "integer", "enum": [CODE_REVIEW_VERSION] })
    }
}

/// How much of its scope a review covered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Coverage {
    /// Whether the review covered everything it was asked to.
    pub complete: bool,
    /// What it could not cover, and why.
    pub limitations: Vec<String>,
}

/// One review finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    /// Unique within the report; kept unchanged through aggregation and
    /// every delivery retry.
    pub id: String,
    /// Impact if the finding holds.
    pub severity: Severity,
    /// How well the evidence supports it.
    pub confidence: Confidence,
    /// One-line title.
    pub title: String,
    /// The explanation and its impact.
    pub explanation: String,
    /// Where the problem is; empty when it has no source location.
    pub locations: Vec<SourceAnchor>,
    /// Supporting source; empty when none.
    pub evidence: Vec<SourceAnchor>,
    /// How to fix it, or `null` when the review offers no remediation.
    #[serde(deserialize_with = "present")]
    #[schemars(required, schema_with = "nullable::<Remediation>")]
    pub remediation: Option<Remediation>,
}

/// A finding's impact. It describes the finding, not the verdict.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Catastrophic impact.
    Critical,
    /// Serious failure.
    High,
    /// Bounded material failure.
    Medium,
    /// Limited actionable impact.
    Low,
}

/// How well the evidence supports a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// Directly supported.
    High,
    /// Credible but not fully confirmed.
    Medium,
    /// Tentative.
    Low,
}

/// How to fix a finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Remediation {
    /// What to change and why.
    pub explanation: String,
    /// A replacement of one exact source range, or `null`.
    #[serde(deserialize_with = "present")]
    #[schemars(required, schema_with = "nullable::<SuggestedEdit>")]
    pub suggested_edit: Option<SuggestedEdit>,
}

/// A suggested replacement of exactly one retained source range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SuggestedEdit {
    /// The range the replacement replaces.
    pub location: SourceAnchor,
    /// The complete replacement text of that range.
    pub replacement: String,
}

/// An inclusive line range of one file at one commit.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceAnchor {
    /// Canonical repository-relative path.
    pub path: String,
    /// The commit whose content the lines index.
    pub commit: String,
    /// First line, one-based.
    #[schemars(with = "u32", range(min = 1))]
    pub start_line: NonZeroU32,
    /// Last line, inclusive, at least `start_line`.
    #[schemars(with = "u32", range(min = 1))]
    pub end_line: NonZeroU32,
}

/// Decodes a member that must be present, `null` included.
///
/// # Errors
/// Returns the deserializer's error; a missing member is refused because a
/// `deserialize_with` field has no implicit default.
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(deserializer)
}

/// Schema of a required member holding a `T` or `null`.
fn nullable<T: JsonSchema>(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    let inner = generator.subschema_for::<T>();
    schemars::json_schema!({ "anyOf": [inner, { "type": "null" }] })
}

/// Why a value is not a usable standard code review report.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReportProblem {
    /// The envelope does not decode as the supported report shape.
    #[error("the code_review report is malformed: {0}")]
    Malformed(String),
    /// Two findings share an id.
    #[error("finding id `{0}` is not unique in the report")]
    DuplicateFinding(String),
    /// A finding's id is empty.
    #[error("a finding has an empty id")]
    EmptyFindingId,
    /// An anchor's path is not a canonical repository-relative path.
    #[error("finding `{finding}` names a non-canonical path `{path}`")]
    InvalidPath {
        /// The finding.
        finding: String,
        /// The path as authored.
        path: String,
    },
    /// An anchor's commit is not a full lowercase hexadecimal object id.
    #[error("finding `{finding}` names an invalid commit `{commit}`")]
    InvalidCommit {
        /// The finding.
        finding: String,
        /// The commit as authored.
        commit: String,
    },
    /// An anchor ends before it starts.
    #[error("finding `{finding}` has a line range ending before it starts")]
    InvertedRange {
        /// The finding.
        finding: String,
    },
}

impl ReportProblem {
    /// The stable machine-readable reason of this problem.
    #[must_use]
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::Malformed(_) => "report_malformed",
            Self::DuplicateFinding(_) => "finding_id_duplicate",
            Self::EmptyFindingId => "finding_id_empty",
            Self::InvalidPath { .. } => "anchor_path_invalid",
            Self::InvalidCommit { .. } => "anchor_commit_invalid",
            Self::InvertedRange { .. } => "anchor_range_invalid",
        }
    }
}

/// The generated report schema, projected onto the portable structured
/// output subset. Generated once: it depends only on the report types.
static REPORT_SCHEMA: LazyLock<Map<String, Value>> = LazyLock::new(|| {
    let Ok(mut generated) = serde_json::to_value(crate::schema::schema_for::<CodeReviewReport>())
    else {
        unreachable!("a generated root schema serializes to a JSON object");
    };
    super::execution::portable(&mut generated);
    let Value::Object(schema) = generated else {
        unreachable!("a generated root schema serializes to a JSON object");
    };
    schema
});

impl CodeReviewReport {
    /// The closed JSON Schema every report satisfies, as the `code_review`
    /// response format resolves it.
    #[must_use]
    pub fn schema() -> &'static Map<String, Value> {
        &REPORT_SCHEMA
    }

    /// Recognizes `value` as a report envelope: `None` unless it is an
    /// object whose `format` is `code_review`, else the decoded and
    /// validated report or why it is unusable.
    ///
    /// # Errors
    /// Returns the [`ReportProblem`] of an envelope that does not decode or
    /// [`validate`](Self::validate).
    pub fn envelope(value: &Value) -> Option<Result<Self, ReportProblem>> {
        if value.get("format").and_then(Value::as_str) != Some(CODE_REVIEW_FORMAT) {
            return None;
        }
        Some(
            Self::deserialize(value)
                .map_err(|error| ReportProblem::Malformed(error.to_string()))
                .and_then(|report| report.validate().map(|()| report)),
        )
    }

    /// Checks what the schema cannot state: finding ids are nonempty and
    /// unique, and every anchor names a canonical path, a full commit id and
    /// an ordered range. Whether an anchor exists in retained source is the
    /// renderer's check.
    ///
    /// # Errors
    /// Returns the first [`ReportProblem`] in finding order.
    pub fn validate(&self) -> Result<(), ReportProblem> {
        let mut ids = BTreeSet::new();
        for finding in &self.findings {
            if finding.id.is_empty() {
                return Err(ReportProblem::EmptyFindingId);
            }
            if !ids.insert(finding.id.as_str()) {
                return Err(ReportProblem::DuplicateFinding(finding.id.clone()));
            }
            for anchor in finding.anchors() {
                anchor.validate(&finding.id)?;
            }
        }
        Ok(())
    }
}

impl Finding {
    /// Every anchor the finding names: locations, evidence, then its
    /// suggested edit's range.
    pub fn anchors(&self) -> impl Iterator<Item = &SourceAnchor> {
        self.locations.iter().chain(&self.evidence).chain(
            self.remediation
                .as_ref()
                .and_then(|remediation| remediation.suggested_edit.as_ref())
                .map(|edit| &edit.location),
        )
    }
}

impl SourceAnchor {
    /// Checks the anchor's own shape for `finding`.
    ///
    /// # Errors
    /// Returns [`ReportProblem::InvalidPath`] for an empty, absolute,
    /// backslashed or dot-segment path, [`ReportProblem::InvalidCommit`]
    /// unless the commit is 40 or 64 lowercase hexadecimal digits, and
    /// [`ReportProblem::InvertedRange`] when it ends before it starts.
    pub fn validate(&self, finding: &str) -> Result<(), ReportProblem> {
        let canonical = !self.path.is_empty()
            && !self.path.contains('\\')
            && self
                .path
                .split('/')
                .all(|segment| !matches!(segment, "" | "." | ".."));
        if !canonical {
            return Err(ReportProblem::InvalidPath {
                finding: finding.to_owned(),
                path: self.path.clone(),
            });
        }
        let commit = matches!(self.commit.len(), 40 | 64)
            && self
                .commit
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        if !commit {
            return Err(ReportProblem::InvalidCommit {
                finding: finding.to_owned(),
                commit: self.commit.clone(),
            });
        }
        if self.end_line < self.start_line {
            return Err(ReportProblem::InvertedRange {
                finding: finding.to_owned(),
            });
        }
        Ok(())
    }
}

/// Resolves a Prompt's named `response_format` to the schema its responses
/// must satisfy: `verdict` through [`Verdict::response_format`] and
/// `code_review` to [`CodeReviewReport::schema`].
///
/// # Errors
/// Returns [`SkaldError::PromptInvalid`] at `response_format` for any other
/// name, at `response_format.data` for a `code_review` declaring `data`, and
/// every [`Verdict::response_format`] refusal.
pub fn resolve_response_format(
    format: &NamedResponseFormat,
) -> Result<Map<String, Value>, SkaldError> {
    match format.name() {
        VERDICT_FORMAT => Verdict::response_format(format),
        CODE_REVIEW_FORMAT if format.data().is_some() => Err(SkaldError::PromptInvalid {
            field: "response_format.data".to_owned(),
            message: "the code_review format has a fixed schema and declares no data".to_owned(),
        }),
        CODE_REVIEW_FORMAT => Ok(CodeReviewReport::schema().clone()),
        other => Err(SkaldError::PromptInvalid {
            field: "response_format".to_owned(),
            message: format!(
                "unknown response format `{other}`; the named formats are `verdict` and `code_review`"
            ),
        }),
    }
}

/// The step output, or fanout item of one, that held a report envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReportSource {
    /// The step.
    pub step: TaskId,
    /// The zero-based fanout item; absent for a step that does not fan out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<u32>,
}

/// The report one settled execution's outputs hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportSelection {
    /// No accepted successful output is a report envelope.
    None,
    /// Exactly one is, and it is valid.
    One {
        /// Where it came from.
        source: ReportSource,
        /// The report.
        report: Box<CodeReviewReport>,
    },
    /// Exactly one is, and it is not a usable report.
    Invalid {
        /// Where it came from.
        source: ReportSource,
        /// Why it is unusable.
        problem: ReportProblem,
    },
    /// More than one is; none is chosen.
    Ambiguous {
        /// Every output holding an envelope, in step order.
        sources: Vec<ReportSource>,
    },
}

/// Finds the report among `steps`, the settled final step results of one
/// execution of `spec`.
///
/// Each succeeded step's final output is a candidate; a fanout step's
/// output is its retained items, each its own candidate. A failed step, a
/// skipped one (a discarded speculative step included) and a repeat's
/// earlier iterations hold no output, so they never count. An envelope a
/// later consolidator consumed still counts.
#[must_use]
pub fn select_report(spec: &VerifierSpec, steps: &[StepResult]) -> ReportSelection {
    let mut found = Vec::new();
    for step in steps {
        let Some(output) = step
            .output
            .as_ref()
            .filter(|_| step.status == StepStatus::Succeeded)
        else {
            continue;
        };
        let fanout = spec
            .steps
            .iter()
            .any(|declared| declared.id == step.id && declared.foreach.is_some());
        match (fanout, output) {
            (true, Value::Array(items)) => {
                for (index, item) in items.iter().enumerate() {
                    if let Some(decoded) = CodeReviewReport::envelope(item) {
                        let source = ReportSource {
                            step: step.id.clone(),
                            item: u32::try_from(index).ok(),
                        };
                        found.push((source, decoded));
                    }
                }
            }
            _ => {
                if let Some(decoded) = CodeReviewReport::envelope(output) {
                    let source = ReportSource {
                        step: step.id.clone(),
                        item: None,
                    };
                    found.push((source, decoded));
                }
            }
        }
    }
    if found.len() > 1 {
        return ReportSelection::Ambiguous {
            sources: found.into_iter().map(|(source, _)| source).collect(),
        };
    }
    match found.pop() {
        None => ReportSelection::None,
        Some((source, Ok(report))) => ReportSelection::One {
            source,
            report: Box::new(report),
        },
        Some((source, Err(problem))) => ReportSelection::Invalid { source, problem },
    }
}

/// Unit proofs of the report contract, its schema and report selection.
#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// A valid report with one located finding carrying a suggestion and
    /// one unlocated finding.
    fn report() -> Value {
        let anchor = json!({
            "path": "src/lib.rs", "commit": "a".repeat(40), "start_line": 3, "end_line": 4
        });
        json!({
            "format": "code_review", "version": 1, "summary": "Two findings.",
            "coverage": {"complete": true, "limitations": []},
            "findings": [
                {"id": "F-2", "severity": "low", "confidence": "medium", "title": "Naming",
                 "explanation": "Unclear.", "locations": [], "evidence": [], "remediation": null},
                {"id": "F-1", "severity": "critical", "confidence": "high", "title": "Overflow",
                 "explanation": "Unchecked add.", "locations": [anchor], "evidence": [anchor],
                 "remediation": {"explanation": "Use checked_add.",
                                 "suggested_edit": {"location": anchor, "replacement": "x\n"}}}
            ]
        })
    }

    /// The generated schema is closed, requires every member including the
    /// nullable ones, pins format and version, and admits exactly what
    /// decoding admits.
    ///
    /// # Panics
    /// Panics when the schema is invalid or admits or refuses a value decoding does not.
    #[test]
    fn the_schema_is_closed_and_requires_every_member() {
        let schema = Value::Object(CodeReviewReport::schema().clone());
        let validator = jsonschema::validator_for(&schema).expect("valid schema");
        assert!(validator.is_valid(&report()));
        assert_eq!(schema["additionalProperties"], json!(false));
        let finding = &schema["$defs"]["Finding"];
        let required: BTreeSet<&str> = finding["required"]
            .as_array()
            .expect("required")
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert!(required.contains("remediation"), "{finding}");
        for broken in [
            json!({"version": 2}),
            json!({"format": "other"}),
            json!({"extra": true}),
        ] {
            let mut value = report();
            value
                .as_object_mut()
                .expect("object")
                .extend(broken.as_object().expect("object").clone());
            assert!(!validator.is_valid(&value), "{broken}");
            assert!(CodeReviewReport::deserialize(&value).is_err(), "{broken}");
        }
        let mut missing = report();
        missing["findings"][0]
            .as_object_mut()
            .expect("finding")
            .remove("remediation");
        assert!(!validator.is_valid(&missing));
        assert!(CodeReviewReport::deserialize(&missing).is_err());
    }

    /// `code_review` resolves the report schema, refuses `data`, and
    /// `verdict` still resolves the Verdict.
    ///
    /// # Panics
    /// Panics when a named format does not resolve to its schema.
    #[test]
    fn the_named_resolver_serves_both_formats() {
        let named = |name: &str| NamedResponseFormat::Name(name.to_owned());
        assert_eq!(
            resolve_response_format(&named("code_review")).expect("resolves"),
            *CodeReviewReport::schema()
        );
        assert_eq!(
            resolve_response_format(&named("verdict")).expect("resolves"),
            Verdict::response_format(&named("verdict")).expect("resolves")
        );
        let with_data = NamedResponseFormat::Expanded {
            kind: "code_review".to_owned(),
            data: Some(Map::new()),
        };
        assert!(matches!(
            resolve_response_format(&with_data),
            Err(SkaldError::PromptInvalid { field, .. }) if field == "response_format.data"
        ));
        assert!(resolve_response_format(&named("summary")).is_err());
    }

    /// Envelope recognition ignores other outputs, decodes valid reports and
    /// names what makes an envelope unusable.
    ///
    /// # Panics
    /// Panics when an envelope is misrecognized or a problem is misreported.
    #[test]
    fn envelopes_are_recognized_and_validated() {
        assert!(CodeReviewReport::envelope(&json!({"findings": []})).is_none());
        assert!(CodeReviewReport::envelope(&json!([report()])).is_none());
        let decoded = CodeReviewReport::envelope(&report()).expect("envelope");
        assert_eq!(decoded.expect("valid").findings.len(), 2);
        let cases = [
            ("/findings/1/id", json!("F-2"), "finding_id_duplicate"),
            ("/findings/1/id", json!(""), "finding_id_empty"),
            (
                "/findings/1/locations/0/path",
                json!("../x"),
                "anchor_path_invalid",
            ),
            (
                "/findings/1/locations/0/path",
                json!("/abs"),
                "anchor_path_invalid",
            ),
            (
                "/findings/1/evidence/0/commit",
                json!("HEAD"),
                "anchor_commit_invalid",
            ),
            (
                "/findings/1/locations/0/start_line",
                json!(9),
                "anchor_range_invalid",
            ),
            (
                "/findings/1/locations/0/start_line",
                json!(0),
                "report_malformed",
            ),
            ("/version", json!(2), "report_malformed"),
        ];
        for (pointer, value, reason) in cases {
            let mut broken = report();
            *broken.pointer_mut(pointer).expect("pointer") = value;
            let problem = CodeReviewReport::envelope(&broken)
                .expect("envelope")
                .expect_err(pointer);
            assert_eq!(problem.reason(), reason, "{pointer}");
        }
    }

    /// A Verifier with plain steps `a`, `b`, `c` and fanout step `fan`.
    ///
    /// # Panics
    /// Panics when the authored Verifier does not deserialize.
    fn spec() -> VerifierSpec {
        let code = |id: &str| {
            json!({"id": id, "action": {"kind": "code", "entrypoint": "f",
                   "source": {"inline": "def f(): pass"}}})
        };
        let mut fan = code("fan");
        fan["foreach"] = json!({"count": 2});
        serde_json::from_value(json!({
            "steps": [code("a"), code("b"), code("c"), fan],
            "requirements": [{"id": "r", "statement": "s",
                              "check": {"kind": "assert", "path": "steps.a.status",
                                        "operator": "is_not_null"}}]
        }))
        .expect("authored Verifier deserializes")
    }

    /// A settled step result.
    ///
    /// # Panics
    /// Panics when `id` is not a valid step id.
    fn step(id: &str, status: StepStatus, output: Option<Value>) -> StepResult {
        StepResult {
            id: TaskId::new(id).expect("task id"),
            status,
            output,
            attempts: 1,
            duration_ms: 0,
            reason: None,
            provenance: None,
            tool_calls: Vec::new(),
        }
    }

    /// Only succeeded outputs count; each fanout item is its own candidate;
    /// an envelope a later step consumed still counts; one malformed
    /// envelope is invalid, never ignored.
    ///
    /// # Panics
    /// Panics when a selection differs from the one expected.
    #[test]
    fn selection_counts_each_retained_final_output() {
        let spec = spec();
        let ok = StepStatus::Succeeded;
        let none = select_report(&spec, &[step("a", ok, Some(json!({"n": 1})))]);
        assert_eq!(none, ReportSelection::None);
        let ignored = [
            step("a", StepStatus::Failed, None),
            step("b", StepStatus::Skipped, None),
            step("c", ok, Some(report())),
        ];
        assert!(matches!(
            select_report(&spec, &ignored),
            ReportSelection::One { source, .. } if source.step.as_str() == "c" && source.item.is_none()
        ));
        let item = [step("fan", ok, Some(json!([{"n": 1}, report()])))];
        assert!(matches!(
            select_report(&spec, &item),
            ReportSelection::One { source, .. } if source.item == Some(1)
        ));
        let consumed = [step("a", ok, Some(report())), step("b", ok, Some(report()))];
        assert!(matches!(
            select_report(&spec, &consumed),
            ReportSelection::Ambiguous { sources } if sources.len() == 2
        ));
        let both_items = [step("fan", ok, Some(json!([report(), report()])))];
        assert!(matches!(
            select_report(&spec, &both_items),
            ReportSelection::Ambiguous { sources } if sources.len() == 2
        ));
        let mut broken = report();
        broken["version"] = json!(2);
        assert!(matches!(
            select_report(&spec, &[step("a", ok, Some(broken))]),
            ReportSelection::Invalid {
                problem: ReportProblem::Malformed(_),
                ..
            }
        ));
    }
}
