//! The built-in conversion of a fixed native decision answer into an Agent
//! check's Verdict.
//!
//! [`VerdictTransformer`] reads the one answer its declared
//! [`VerdictTransform`] selects from the typed provider response the Agent
//! run already holds, applies the policy in memory, and returns the canonical
//! Verdict with the selected native answer as the check's actual value. An
//! answer outside the declared question's native contract is a check error,
//! never a guessed verdict.

use serde_json::{Value, json};
use skald_spec::wire::jev::{JevAnswer, JevQuestion};
use skald_spec::{ProviderRequest, ProviderResponse};
use wyrd_spec::card::verifier::transform::canonical;
use wyrd_spec::card::verifier::{
    ChoiceTransform, ProbabilityTransform, ScoreTransform, VerdictTransform,
};
use wyrd_spec::verification::VerificationVerdict;
use wyrd_spec::verification::execution::Verdict;

use super::BlockError;
use crate::operators::evaluate_operator;

/// Applies one Agent check's [`VerdictTransform`] to the decision request
/// the check's Prompt sent.
#[derive(Debug, Clone, Copy)]
pub struct VerdictTransformer<'a> {
    /// The declared policy.
    policy: &'a VerdictTransform,
    /// The bound request whose questions the response answers.
    request: &'a ProviderRequest,
}

/// A check error: the response is unusable for the declared policy.
fn unusable(reason: String) -> BlockError {
    BlockError::Terminal { reason }
}

impl<'a> VerdictTransformer<'a> {
    /// A transformer of `policy` over answers to `request`.
    #[must_use]
    pub const fn new(policy: &'a VerdictTransform, request: &'a ProviderRequest) -> Self {
        Self { policy, request }
    }

    /// The Verdict `response` yields under the policy, and the selected
    /// native answer as JSON for the check's actual value.
    ///
    /// The reason states the selected answer and the applied policy; the
    /// Verdict cites nothing, since the answer has no evidence locator.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for a response of another protocol
    /// than the request, a missing selected answer, an answer of another
    /// form than its question, an undeclared Choice label, a non-finite
    /// number, a Noul outside `0..1`, or a Score outside `0..N-1`.
    pub fn apply(&self, response: &ProviderResponse) -> Result<(Verdict, Value), BlockError> {
        let (ProviderRequest::JevSystemOne(request), ProviderResponse::JevSystemOne(response)) =
            (self.request, response)
        else {
            return Err(unusable(
                "verdict_transform reads only a fixed decision response to its own request"
                    .to_owned(),
            ));
        };
        let key = self.policy.question();
        let question = request
            .questions
            .get(key)
            .ok_or_else(|| unusable(format!("the Prompt declares no question '{key}'")))?;
        let answer = response.answers.get(key).ok_or_else(|| {
            unusable(format!(
                "the decision response has no answer to question '{key}'"
            ))
        })?;
        let (outcome, reason) = match (self.policy, question, answer) {
            (
                VerdictTransform::Choice(policy),
                JevQuestion::Choice { criteria, .. },
                JevAnswer::Choice {
                    choice,
                    confidence,
                    probabilities,
                },
            ) => {
                if !criteria.contains_key(choice) {
                    return Err(unusable(format!(
                        "the answer to question '{key}' selected an option the question does not declare"
                    )));
                }
                if !unit(*confidence) || !probabilities.values().all(|p| unit(*p)) {
                    return Err(unusable(format!(
                        "the answer to question '{key}' has a confidence or probability outside 0..1"
                    )));
                }
                choose(policy, key, choice, *confidence)?
            }
            (
                VerdictTransform::Probability(policy),
                JevQuestion::Noul { .. },
                JevAnswer::Noul { noul },
            ) => {
                if !unit(*noul) {
                    return Err(unusable(format!(
                        "the answer to question '{key}' has a probability outside 0..1"
                    )));
                }
                threshold(policy, key, *noul)
            }
            (
                VerdictTransform::Score(policy),
                JevQuestion::Score { criteria, .. },
                JevAnswer::Score { score, .. },
            ) => grade(policy, key, *score, criteria.len())?,
            _ => {
                return Err(unusable(format!(
                    "the answer to question '{key}' is not the {} answer its policy reads",
                    self.policy.form().as_str()
                )));
            }
        };
        let actual = serde_json::to_value(answer)
            .map_err(|_| unusable(format!("the answer to question '{key}' is not JSON")))?;
        Ok((
            Verdict {
                outcome,
                reason,
                citations: Vec::new(),
                data: None,
            },
            actual,
        ))
    }
}

/// Whether `value` is a finite number within `0..=1`.
fn unit(value: f64) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

/// The outcome of a Choice `label`: its mapped outcome, or the canonical
/// label itself without a map.
///
/// # Errors
/// Returns [`BlockError::Terminal`] for a label the map or the canonical
/// outcomes do not name.
fn choose(
    policy: &ChoiceTransform,
    key: &str,
    label: &str,
    confidence: f64,
) -> Result<(VerificationVerdict, String), BlockError> {
    let selected = format!("Question '{key}' selected '{label}' (confidence {confidence})");
    match &policy.outcomes {
        None => canonical(label)
            .map(|outcome| (outcome, format!("{selected}.")))
            .ok_or_else(|| {
                unusable(format!(
                    "the answer to question '{key}' is not passed, failed, or inconclusive"
                ))
            }),
        Some(outcomes) => outcomes
            .get(label)
            .map(|outcome| {
                (
                    *outcome,
                    format!("{selected}, which the policy maps to {}.", name(*outcome)),
                )
            })
            .ok_or_else(|| {
                unusable(format!(
                    "the answer to question '{key}' selected an option the policy does not map"
                ))
            }),
    }
}

/// The outcome of a Noul probability `noul` that the question holds.
fn threshold(policy: &ProbabilityTransform, key: &str, noul: f64) -> (VerificationVerdict, String) {
    let ProbabilityTransform {
        expected,
        pass_at,
        fail_at,
        ..
    } = *policy;
    let q = if expected { noul } else { 1.0 - noul };
    let (outcome, verdict) = if q >= pass_at {
        (
            VerificationVerdict::Passed,
            format!("at least pass_at {pass_at}"),
        )
    } else if q <= fail_at {
        (
            VerificationVerdict::Failed,
            format!("at most fail_at {fail_at}"),
        )
    } else {
        (
            VerificationVerdict::Inconclusive,
            format!("between fail_at {fail_at} and pass_at {pass_at}"),
        )
    };
    (
        outcome,
        format!(
            "Question '{key}' answered true with probability {noul}; the expected answer {expected} has probability {q}, {verdict}."
        ),
    )
}

/// The outcome of Score `score` on a scale of `levels` levels.
///
/// # Errors
/// Returns [`BlockError::Terminal`] for a non-finite score or one outside
/// `0..levels-1`.
fn grade(
    policy: &ScoreTransform,
    key: &str,
    score: f64,
    levels: usize,
) -> Result<(VerificationVerdict, String), BlockError> {
    let top = levels.saturating_sub(1) as f64;
    if !score.is_finite() || !(0.0..=top).contains(&score) {
        return Err(unusable(format!(
            "the answer to question '{key}' has a score outside its levels 0 to {top}"
        )));
    }
    let (observed, expected) = (json!(score), json!(policy.expected));
    let held = evaluate_operator(&observed, &policy.operator, &expected)
        .map_err(|_| {
            unusable(format!(
                "the score policy for question '{key}' cannot compare"
            ))
        })?
        .passed;
    let operator = policy.operator.discriminator();
    let (outcome, verb) = if held {
        (VerificationVerdict::Passed, "holds")
    } else {
        (VerificationVerdict::Failed, "does not hold")
    };
    Ok((
        outcome,
        format!(
            "Question '{key}' scored {score} on levels 0 to {top}; {operator} {} {verb}.",
            policy.expected
        ),
    ))
}

/// The wire name of `outcome`.
const fn name(outcome: VerificationVerdict) -> &'static str {
    match outcome {
        VerificationVerdict::Passed => "passed",
        VerificationVerdict::Failed => "failed",
        VerificationVerdict::Inconclusive => "inconclusive",
    }
}

#[cfg(test)]
mod tests {
    //! Every leaf policy's boundaries and every unusable answer.

    use std::collections::BTreeMap;

    use serde_json::json;
    use skald_spec::wire::jev::{JevRequest, JevResponse};
    use wyrd_spec::verification::ComparisonOperator;

    use super::*;

    /// The request asking `refund` (Choice), `billing` (Noul), and
    /// `compliance` (Score, three levels).
    fn request() -> ProviderRequest {
        ProviderRequest::JevSystemOne(
            serde_json::from_value::<JevRequest>(json!({
                "model": "jev-latest", "state": "s", "questions": {
                    "refund": {"type": "choice", "instructions": "x",
                               "criteria": {"compliant": "a", "violation": "b", "unknown": "c"}},
                    "canonical": {"type": "choice", "instructions": "x",
                                  "criteria": {"passed": "a", "failed": "b", "inconclusive": "c"}},
                    "billing": {"type": "noul", "instructions": "x"},
                    "compliance": {"type": "score", "instructions": "x",
                                   "criteria": ["low", "mid", "high"]},
                }
            }))
            .expect("request decodes"),
        )
    }

    /// A response holding `answers`.
    fn response(answers: serde_json::Value) -> ProviderResponse {
        let response: JevResponse =
            serde_json::from_value(json!({"model": "jev-1", "answers": answers}))
                .expect("response decodes");
        ProviderResponse::JevSystemOne(response)
    }

    /// Apply `policy` to `answers`.
    fn apply(
        policy: &VerdictTransform,
        answers: serde_json::Value,
    ) -> Result<(Verdict, Value), BlockError> {
        let request = request();
        VerdictTransformer::new(policy, &request).apply(&response(answers))
    }

    /// The outcome `policy` gives `answers`.
    ///
    /// # Panics
    /// Panics when the answer is unusable.
    fn outcome(policy: &VerdictTransform, answers: serde_json::Value) -> VerificationVerdict {
        apply(policy, answers).expect("usable answer").0.outcome
    }

    /// A Choice answer selecting `choice` with confidence 0.6.
    fn choice(key: &str, choice: &str) -> serde_json::Value {
        json!({key: {"type": "choice", "choice": choice, "confidence": 0.6,
                     "probabilities": {choice: 0.6}}})
    }

    /// Canonical labels copy through; a map assigns every custom label its
    /// outcome; confidence never changes the mapping; the answer is the
    /// actual value and the reason names answer and mapping.
    ///
    /// # Panics
    /// Panics when a label maps to another outcome.
    #[test]
    fn choice_maps_labels_and_ignores_confidence() {
        let canonical = VerdictTransform::Choice(ChoiceTransform {
            question: "canonical".into(),
            outcomes: None,
        });
        for (label, expected) in [
            ("passed", VerificationVerdict::Passed),
            ("failed", VerificationVerdict::Failed),
            ("inconclusive", VerificationVerdict::Inconclusive),
        ] {
            assert_eq!(outcome(&canonical, choice("canonical", label)), expected);
        }
        let mapped = VerdictTransform::Choice(ChoiceTransform {
            question: "refund".into(),
            outcomes: Some(BTreeMap::from([
                ("compliant".into(), VerificationVerdict::Passed),
                ("violation".into(), VerificationVerdict::Failed),
                ("unknown".into(), VerificationVerdict::Inconclusive),
            ])),
        });
        let (verdict, actual) = apply(&mapped, choice("refund", "violation")).expect("usable");
        assert_eq!(verdict.outcome, VerificationVerdict::Failed);
        assert_eq!(
            verdict.reason,
            "Question 'refund' selected 'violation' (confidence 0.6), which the policy maps to failed."
        );
        assert!(verdict.citations.is_empty());
        assert_eq!(actual, choice("refund", "violation")["refund"]);
        assert_eq!(
            outcome(&mapped, choice("refund", "unknown")),
            VerificationVerdict::Inconclusive
        );
    }

    /// The probability policy: `q >= pass_at` passes and `q <= fail_at`
    /// fails, both inclusive, values between are inconclusive, and
    /// `expected: false` reads `1 - p`.
    ///
    /// # Panics
    /// Panics when a boundary is judged otherwise.
    #[test]
    fn probability_thresholds_are_inclusive_for_both_polarities() {
        let policy = |expected| {
            VerdictTransform::Probability(ProbabilityTransform {
                question: "billing".into(),
                expected,
                pass_at: 0.75,
                fail_at: 0.25,
            })
        };
        let noul = |p: f64| json!({"billing": {"type": "noul", "noul": p}});
        for (expected, p, judged) in [
            (true, 0.75, VerificationVerdict::Passed),
            (true, 1.0, VerificationVerdict::Passed),
            (true, 0.25, VerificationVerdict::Failed),
            (true, 0.0, VerificationVerdict::Failed),
            (true, 0.5, VerificationVerdict::Inconclusive),
            (true, 0.74, VerificationVerdict::Inconclusive),
            (true, 0.26, VerificationVerdict::Inconclusive),
            (false, 0.25, VerificationVerdict::Passed),
            (false, 0.75, VerificationVerdict::Failed),
            (false, 0.5, VerificationVerdict::Inconclusive),
        ] {
            assert_eq!(
                outcome(&policy(expected), noul(p)),
                judged,
                "{expected} {p}"
            );
        }
        let strict = VerdictTransform::Probability(ProbabilityTransform {
            question: "billing".into(),
            expected: false,
            pass_at: 0.9,
            fail_at: 0.1,
        });
        assert_eq!(outcome(&strict, noul(0.02)), VerificationVerdict::Passed);
        assert_eq!(outcome(&strict, noul(0.98)), VerificationVerdict::Failed);
        let (verdict, actual) = apply(&strict, noul(0.5)).expect("usable");
        assert_eq!(verdict.outcome, VerificationVerdict::Inconclusive);
        assert_eq!(
            verdict.reason,
            "Question 'billing' answered true with probability 0.5; the expected answer false has probability 0.5, between fail_at 0.1 and pass_at 0.9."
        );
        assert_eq!(actual, json!({"type": "noul", "noul": 0.5}));
    }

    /// A Score compares on its level indices, fractional values included.
    ///
    /// # Panics
    /// Panics when a comparison is judged otherwise.
    #[test]
    fn score_compares_level_indices() {
        let policy = |operator, expected| {
            VerdictTransform::Score(ScoreTransform {
                question: "compliance".into(),
                operator,
                expected,
            })
        };
        let score =
            |s: f64| json!({"compliance": {"type": "score", "score": s, "legend": {"0": "low"}}});
        let at_least_two = policy(ComparisonOperator::GreaterThanOrEquals, 2.0);
        assert_eq!(
            outcome(&at_least_two, score(2.0)),
            VerificationVerdict::Passed
        );
        assert_eq!(
            outcome(&at_least_two, score(1.8)),
            VerificationVerdict::Failed
        );
        assert_eq!(
            apply(&at_least_two, score(1.8)).expect("usable").0.reason,
            "Question 'compliance' scored 1.8 on levels 0 to 2; greater_than_or_equals 2 does not hold."
        );
        assert_eq!(
            outcome(&policy(ComparisonOperator::LessThan, 1.0), score(0.0)),
            VerificationVerdict::Passed
        );
        assert_eq!(
            outcome(
                &policy(
                    ComparisonOperator::ApproximatelyEquals { tolerance: 0.25 },
                    1.0
                ),
                score(1.2)
            ),
            VerificationVerdict::Passed
        );
    }

    /// Missing answers, wrong tags, undeclared or unmapped labels, and
    /// out-of-range or non-finite numbers are check errors.
    ///
    /// # Panics
    /// Panics when an unusable answer yields a verdict.
    #[test]
    fn unusable_answers_are_check_errors() {
        let canonical = VerdictTransform::Choice(ChoiceTransform {
            question: "canonical".into(),
            outcomes: None,
        });
        let billing = VerdictTransform::Probability(ProbabilityTransform {
            question: "billing".into(),
            expected: true,
            pass_at: 0.9,
            fail_at: 0.1,
        });
        let compliance = VerdictTransform::Score(ScoreTransform {
            question: "compliance".into(),
            operator: ComparisonOperator::GreaterThanOrEquals,
            expected: 1.0,
        });
        for (policy, answers) in [
            (&canonical, json!({})),
            (
                &canonical,
                json!({"canonical": {"type": "noul", "noul": 0.5}}),
            ),
            (&canonical, choice("canonical", "maybe")),
            (
                &canonical,
                json!({"canonical": {"type": "choice", "choice": "passed", "confidence": 1.5,
                                     "probabilities": {"passed": 1.5}}}),
            ),
            (&billing, json!({"billing": {"type": "noul", "noul": 1.01}})),
            (
                &billing,
                json!({"billing": {"type": "noul", "noul": -0.01}}),
            ),
            (
                &compliance,
                json!({"compliance": {"type": "score", "score": 2.5, "legend": {}}}),
            ),
            (
                &compliance,
                json!({"compliance": {"type": "score", "score": -1, "legend": {}}}),
            ),
        ] {
            assert!(
                matches!(
                    apply(policy, answers.clone()),
                    Err(BlockError::Terminal { .. })
                ),
                "{answers}"
            );
        }
        let request = request();
        for value in [f64::NAN, f64::INFINITY] {
            let response = ProviderResponse::JevSystemOne(JevResponse {
                model: "jev-1".into(),
                answers: BTreeMap::from([("billing".into(), JevAnswer::Noul { noul: value })]),
                usage: None,
            });
            assert!(matches!(
                VerdictTransformer::new(&billing, &request).apply(&response),
                Err(BlockError::Terminal { .. })
            ));
        }
    }
}
