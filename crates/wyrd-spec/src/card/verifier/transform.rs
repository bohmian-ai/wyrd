//! Declarative policies converting a fixed native decision answer into a
//! canonical Verdict.
//!
//! A [`VerdictTransform`] belongs to an Agent check ([`AgentCheck`]) and is
//! part of the versioned Verifier. Each leaf names one explicit question key
//! of the check Agent's Prompt; Vala dispatches on the provider's typed native
//! response, never on map order or caller-supplied JSON paths. Self-contained
//! field rules run through [`VerdictTransform::validate`]; rules that need the
//! resolved Prompt's questions run through [`VerdictTransform::validate_for`].

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value, json};
use skald_spec::ProviderRequest;
use skald_spec::wire::jev::JevQuestion;

use super::AgentCall;
use crate::card::agent::{Compiled, PromptForm};
use crate::error::WyrdError;
use crate::verification::{ComparisonOperator, VerificationVerdict};

/// An Agent in requirement-check position: an [`AgentCall`] plus the
/// optional policy that converts a fixed native decision answer into the
/// Verdict.
///
/// The wire form is the call's fields beside an optional
/// `verdict_transform`, so a check keeps the `kind: agent` shape; the call
/// keeps its own unknown-field refusal. Without a transform the check Agent's
/// Prompt must produce the canonical Verdict directly.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "server", schema(bound = ""))]
#[serde(bound = "")]
#[schemars(rename = "AgentCheck", bound = "F: PromptForm")]
pub struct AgentCheck<F: PromptForm = Compiled> {
    /// The Agent invocation.
    #[serde(flatten)]
    #[cfg_attr(feature = "server", schema(value_type = AgentCall))]
    pub call: AgentCall<F>,
    /// The policy converting the Agent's native decision answer into the
    /// Verdict; omitted, the Prompt must produce the Verdict itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict_transform: Option<VerdictTransform>,
}

impl<F: PromptForm> From<AgentCall<F>> for AgentCheck<F> {
    /// A direct-Verdict check of `call`, with no transform.
    fn from(call: AgentCall<F>) -> Self {
        Self {
            call,
            verdict_transform: None,
        }
    }
}

impl<'de, F: PromptForm> Deserialize<'de> for AgentCheck<F> {
    /// Takes `verdict_transform` out, then decodes the rest as an
    /// [`AgentCall`], as [`super::CodeCheck`] decodes its call. A `null`
    /// policy is the omitted one, as the optional schema allows.
    ///
    /// # Errors
    /// Returns the deserializer's error for a non-object, a malformed
    /// `verdict_transform`, or anything the Agent call refuses.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut object = Map::<String, Value>::deserialize(deserializer)?;
        let verdict_transform = object
            .remove("verdict_transform")
            .filter(|policy| !policy.is_null())
            .map(serde_json::from_value)
            .transpose()
            .map_err(serde::de::Error::custom)?;
        let call =
            AgentCall::<F>::deserialize(Value::Object(object)).map_err(serde::de::Error::custom)?;
        Ok(Self {
            call,
            verdict_transform,
        })
    }
}

/// The closed set of policies converting one native decision answer into a
/// Verdict.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VerdictTransform {
    /// Map the selected Choice label to an outcome.
    Choice(ChoiceTransform),
    /// Threshold a Noul probability.
    Probability(ProbabilityTransform),
    /// Compare a Score on its authored level scale.
    Score(ScoreTransform),
}

/// Map a Choice question's selected label to a canonical outcome.
///
/// Without `outcomes` the question's labels must be `passed`, `failed`,
/// and `inconclusive` themselves, and the selected label is the outcome.
/// Confidence is evidence and never changes the mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct ChoiceTransform {
    /// The Choice question key.
    pub question: String,
    /// Every declared option label to its outcome.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "server", schema(value_type = Option<BTreeMap<String, String>>))]
    pub outcomes: Option<BTreeMap<String, VerificationVerdict>>,
}

/// Threshold a Noul question's probability `p`.
///
/// With `q = p` when `expected` is true and `q = 1 - p` otherwise, the
/// check passes when `q >= pass_at`, fails when `q <= fail_at`, and is
/// inconclusive between them. Bounds are finite with
/// `0 <= fail_at < pass_at <= 1`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct ProbabilityTransform {
    /// The Noul question key.
    pub question: String,
    /// The answer that passes.
    pub expected: bool,
    /// The least polarity-adjusted probability that passes.
    pub pass_at: f64,
    /// The greatest polarity-adjusted probability that fails.
    pub fail_at: f64,
}

/// Compare a Score question's answer with `expected` by a scalar numeric
/// [`ComparisonOperator`]: passed when the comparison holds, failed when it
/// does not.
///
/// Scores are level indices `0..N-1` of the question's `N` ordered levels,
/// possibly fractional, never normalized.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct ScoreTransform {
    /// The Score question key.
    pub question: String,
    /// The scalar numeric comparison: `equals`, `not_equals`,
    /// `greater_than`, `greater_than_or_equals`, `less_than`,
    /// `less_than_or_equals`, or `approximately_equals`.
    #[cfg_attr(feature = "server", schema(value_type = Value))]
    pub operator: ComparisonOperator,
    /// The level index compared against.
    pub expected: f64,
}

/// The native answer form a transform selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestionForm {
    /// A yes/no probability.
    Noul,
    /// One of named options.
    Choice,
    /// A level on an ordered scale.
    Score,
}

impl QuestionForm {
    /// The wire name of the form.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Noul => "noul",
            Self::Choice => "choice",
            Self::Score => "score",
        }
    }
}

/// One declared decision question, as a transform needs it: its form and,
/// for Choice, its option labels, for Score, its level count.
#[derive(Debug, Clone, Copy)]
pub struct DecisionQuestionView<'a> {
    /// The answer form.
    pub form: QuestionForm,
    /// Choice option labels, in their declared order.
    pub labels: Option<&'a BTreeMap<String, String>>,
    /// Score level count.
    pub levels: Option<usize>,
}

impl<'a> DecisionQuestionView<'a> {
    /// The view of one Jev question.
    #[must_use]
    pub fn jev(question: &'a JevQuestion) -> Self {
        match question {
            JevQuestion::Noul { .. } => Self {
                form: QuestionForm::Noul,
                labels: None,
                levels: None,
            },
            JevQuestion::Choice { criteria, .. } => Self {
                form: QuestionForm::Choice,
                labels: Some(criteria),
                levels: None,
            },
            JevQuestion::Score { criteria, .. } => Self {
                form: QuestionForm::Score,
                labels: None,
                levels: Some(criteria.len()),
            },
        }
    }
}

/// The keyed questions of a fixed decision `request`, or `None` for a
/// request no transform can read.
#[must_use]
pub fn decision_questions(
    request: &ProviderRequest,
) -> Option<BTreeMap<&str, DecisionQuestionView<'_>>> {
    match request {
        ProviderRequest::JevSystemOne(jev) => Some(
            jev.questions
                .iter()
                .map(|(key, question)| (key.as_str(), DecisionQuestionView::jev(question)))
                .collect(),
        ),
        _ => None,
    }
}

/// The structured refusal of one transform field.
fn refuse(rule: &str, path: &str, message: impl std::fmt::Display) -> WyrdError {
    WyrdError::Validation {
        message: format!("{path}: {message}"),
        details: json!({ "rule": rule, "path": path }),
    }
}

impl VerdictTransform {
    /// The question key this policy selects.
    #[must_use]
    pub fn question(&self) -> &str {
        match self {
            Self::Choice(choice) => &choice.question,
            Self::Probability(probability) => &probability.question,
            Self::Score(score) => &score.question,
        }
    }

    /// The question form this policy reads.
    #[must_use]
    pub const fn form(&self) -> QuestionForm {
        match self {
            Self::Choice(_) => QuestionForm::Choice,
            Self::Probability(_) => QuestionForm::Noul,
            Self::Score(_) => QuestionForm::Score,
        }
    }

    /// `value` as a policy authored at `path`: decoded, then checked against
    /// the rules it alone decides. SDKs run this where a policy is written,
    /// so a refusal carries the same catalog error as registration.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] with rule `verdict_transform`: at
    /// `path` for a value that is not a policy's JSON form, else as
    /// [`Self::validate`].
    pub fn authored(path: &str, value: Value) -> Result<Self, WyrdError> {
        let policy: Self = serde_json::from_value(value)
            .map_err(|error| refuse("verdict_transform", path, error))?;
        policy.validate(path)?;
        Ok(policy)
    }

    /// Checks the rules this value alone decides, reporting fields below
    /// `path`, such as `requirements[0].check.verdict_transform`.
    ///
    /// The question key is non-empty; Choice outcome labels are non-empty;
    /// probability bounds are finite with `0 <= fail_at < pass_at <= 1`; a
    /// Score comparison is a scalar numeric operator with a finite
    /// `expected`.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] with rule `verdict_transform` and
    /// the offending field's `path`.
    pub fn validate(&self, path: &str) -> Result<(), WyrdError> {
        let rule = "verdict_transform";
        if self.question().trim().is_empty() {
            return Err(refuse(
                rule,
                &format!("{path}.question"),
                "name the question key this policy selects",
            ));
        }
        match self {
            Self::Choice(choice) => {
                if choice
                    .outcomes
                    .as_ref()
                    .is_some_and(|outcomes| outcomes.keys().any(String::is_empty))
                {
                    return Err(refuse(
                        rule,
                        &format!("{path}.outcomes"),
                        "outcome labels must be the question's non-empty option names",
                    ));
                }
                Ok(())
            }
            Self::Probability(probability) => {
                let ProbabilityTransform {
                    pass_at, fail_at, ..
                } = *probability;
                if !(pass_at.is_finite() && fail_at.is_finite()) {
                    return Err(refuse(
                        rule,
                        &format!("{path}.pass_at"),
                        "pass_at and fail_at must be finite numbers",
                    ));
                }
                if !(0.0..=1.0).contains(&fail_at) || !(0.0..=1.0).contains(&pass_at) {
                    return Err(refuse(
                        rule,
                        &format!("{path}.fail_at"),
                        "pass_at and fail_at must lie between 0 and 1",
                    ));
                }
                if fail_at >= pass_at {
                    return Err(refuse(
                        rule,
                        &format!("{path}.fail_at"),
                        "fail_at must be below pass_at; set 0 <= fail_at < pass_at <= 1",
                    ));
                }
                Ok(())
            }
            Self::Score(score) => {
                if !matches!(
                    score.operator,
                    ComparisonOperator::Equals
                        | ComparisonOperator::NotEquals
                        | ComparisonOperator::GreaterThan
                        | ComparisonOperator::GreaterThanOrEquals
                        | ComparisonOperator::LessThan
                        | ComparisonOperator::LessThanOrEquals
                        | ComparisonOperator::ApproximatelyEquals { .. }
                ) {
                    return Err(refuse(
                        rule,
                        &format!("{path}.operator"),
                        "a score compares with equals, not_equals, greater_than, greater_than_or_equals, less_than, less_than_or_equals, or approximately_equals",
                    ));
                }
                score
                    .operator
                    .validate()
                    .map_err(|error| refuse(rule, &format!("{path}.operator"), error))?;
                if !score.expected.is_finite() {
                    return Err(refuse(
                        rule,
                        &format!("{path}.expected"),
                        "expected must be a finite level index",
                    ));
                }
                Ok(())
            }
        }
    }

    /// Checks this policy against the resolved Prompt `request` it judges:
    /// the request is a fixed decision protocol, the selected question
    /// exists with this policy's form, and Choice labels are canonical or
    /// every one is mapped.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] with rule
    /// `verdict_transform_prompt` for a request no transform reads, and
    /// `verdict_transform` naming the offending field otherwise.
    pub fn validate_for(&self, request: &ProviderRequest, path: &str) -> Result<(), WyrdError> {
        let Some(questions) = decision_questions(request) else {
            return Err(refuse(
                "verdict_transform_prompt",
                path,
                "verdict_transform reads a fixed decision answer; give the Agent a Jev system_one Prompt, or remove verdict_transform and declare `response_format: verdict`",
            ));
        };
        let rule = "verdict_transform";
        let key = self.question();
        let Some(question) = questions.get(key) else {
            let known: Vec<&str> = questions.keys().copied().collect();
            return Err(refuse(
                rule,
                &format!("{path}.question"),
                format!(
                    "the Prompt declares no question {key:?}; select one of {}",
                    known.join(", ")
                ),
            ));
        };
        if question.form != self.form() {
            return Err(refuse(
                rule,
                &format!("{path}.kind"),
                format!(
                    "question {key:?} is a {} question, which a {} policy cannot read; use the {} policy",
                    question.form.as_str(),
                    self.kind(),
                    policy_for(question.form),
                ),
            ));
        }
        if let (Self::Choice(choice), Some(labels)) = (self, question.labels) {
            match &choice.outcomes {
                None => {
                    if let Some(label) = labels.keys().find(|label| canonical(label).is_none()) {
                        return Err(refuse(
                            rule,
                            &format!("{path}.outcomes"),
                            format!(
                                "option {label:?} is not passed, failed, or inconclusive; map every option under outcomes"
                            ),
                        ));
                    }
                }
                Some(outcomes) => {
                    if let Some(label) = labels.keys().find(|label| !outcomes.contains_key(*label))
                    {
                        return Err(refuse(
                            rule,
                            &format!("{path}.outcomes"),
                            format!("option {label:?} has no outcome; map every declared option"),
                        ));
                    }
                    if let Some(label) = outcomes.keys().find(|label| !labels.contains_key(*label))
                    {
                        return Err(refuse(
                            rule,
                            &format!("{path}.outcomes.{label}"),
                            format!("question {key:?} declares no option {label:?}"),
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// The wire `kind` of this policy.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Choice(_) => "choice",
            Self::Probability(_) => "probability",
            Self::Score(_) => "score",
        }
    }
}

/// The policy kind that reads `form`.
const fn policy_for(form: QuestionForm) -> &'static str {
    match form {
        QuestionForm::Noul => "probability",
        QuestionForm::Choice => "choice",
        QuestionForm::Score => "score",
    }
}

/// The outcome a canonical Choice label names, if it is one.
#[must_use]
pub fn canonical(label: &str) -> Option<VerificationVerdict> {
    match label {
        "passed" => Some(VerificationVerdict::Passed),
        "failed" => Some(VerificationVerdict::Failed),
        "inconclusive" => Some(VerificationVerdict::Inconclusive),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use crate::card::verifier::VerifierSpec;

    /// The `kind` constant of each `oneOf` variant of `definition`.
    fn variant_kinds(definition: &Value) -> Vec<&str> {
        definition["oneOf"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|variant| variant.pointer("/properties/kind/const")?.as_str())
            .collect()
    }

    /// The Verifier schema that SDK projections and declaration clients read
    /// defines `VerdictTransform` as exactly the `choice`, `probability`
    /// and `score` union, and only `AgentCheck` carries `verdict_transform`:
    /// no action, step or other check does.
    ///
    /// # Panics
    /// Panics when the schema does not encode, the union differs, or another
    /// definition carries the field.
    #[test]
    fn verifier_schema_offers_verdict_transform_only_on_agent_checks() {
        let schema = serde_json::to_value(crate::schema::schema_for::<VerifierSpec>())
            .expect("the schema encodes");
        let defs = schema["$defs"]
            .as_object()
            .expect("the schema has definitions");

        assert_eq!(
            variant_kinds(&defs["VerdictTransform"]),
            ["choice", "probability", "score"]
        );

        let mut holders = Vec::new();
        for (name, definition) in defs {
            if definition
                .pointer("/properties/verdict_transform")
                .is_some()
            {
                holders.push(name.clone());
            }
            for variant in definition["oneOf"].as_array().into_iter().flatten() {
                if variant.pointer("/properties/verdict_transform").is_some() {
                    let kind = variant
                        .pointer("/properties/kind/const")
                        .and_then(Value::as_str);
                    holders.push(format!("{name}.{}", kind.unwrap_or("?")));
                }
            }
        }
        assert_eq!(holders, ["AgentCheck"]);
    }

    /// The optional policy's omitted and `null` forms both decode to the
    /// same direct-Verdict check, as its schema allows; a malformed non-null
    /// policy is still refused.
    ///
    /// # Panics
    /// Panics when the null form is refused or decodes otherwise, or a
    /// malformed policy decodes.
    #[test]
    fn agent_check_reads_a_null_policy_as_omitted() {
        let call = serde_json::json!({
            "agent": {"kind": "Agent", "name": "judge", "version": "1.0.0"}
        });
        let mut null = call.clone();
        null["verdict_transform"] = Value::Null;
        let omitted: super::AgentCheck =
            serde_json::from_value(call.clone()).expect("an omitted policy decodes");
        let explicit: super::AgentCheck =
            serde_json::from_value(null).expect("a null policy decodes");
        assert_eq!(explicit, omitted);
        assert_eq!(explicit.verdict_transform, None);

        let mut malformed = call;
        malformed["verdict_transform"] = serde_json::json!({"kind": "choice"});
        assert!(serde_json::from_value::<super::AgentCheck>(malformed).is_err());
    }
}
