//! TypeSafe Jev `POST /v1/systemone` wire types.
//!
//! Jev answers keyed questions about a `state` instead of generating
//! conversational text. A request carries the native model, the `state` text
//! to judge, and keyed `noul`, `choice`, or `score` questions; the answer
//! carries the resolved model, one typed [`JevAnswer`] per question key, and
//! token usage. [`JevRequest::validate`] enforces the documented question
//! bounds before any provider IO.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::wire::common::TokenUsage;

/// Most options one `choice` question may offer.
pub const JEV_MAX_CHOICE_OPTIONS: usize = 255;
/// Fewest levels one `score` question may define.
pub const JEV_MIN_SCORE_LEVELS: usize = 2;
/// Most levels one `score` question may define.
pub const JEV_MAX_SCORE_LEVELS: usize = 10;

/// `POST /v1/systemone` request body.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct JevRequest {
    /// Native Jev model, such as `jev-latest`.
    pub model: String,
    /// Text the questions are about, such as an agent reply.
    pub state: String,
    /// Questions keyed by the name their answer returns under.
    pub questions: BTreeMap<String, JevQuestion>,
}

/// One keyed Jev question, tagged by its answer form.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum JevQuestion {
    /// Yes/no question answered with a probability that it holds.
    Noul {
        /// What to decide about the state.
        instructions: String,
        /// Optional descriptions of the true and false outcomes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        criteria: Option<JevNoulCriteria>,
    },
    /// Pick exactly one of the keyed options.
    Choice {
        /// What to choose about the state.
        instructions: String,
        /// Option name to its description.
        criteria: BTreeMap<String, String>,
    },
    /// Grade on an ordered scale whose levels are indexed from zero.
    Score {
        /// What to grade about the state.
        instructions: String,
        /// Level descriptions, lowest first.
        criteria: Vec<String>,
    },
}

/// Outcome descriptions of a `noul` question.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct JevNoulCriteria {
    /// When the answer is true.
    #[serde(rename = "true")]
    pub when_true: String,
    /// When the answer is false.
    #[serde(rename = "false")]
    pub when_false: String,
}

/// `POST /v1/systemone` answer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct JevResponse {
    /// Exact model that answered, such as `jev-1.13.0`.
    pub model: String,
    /// One typed answer per question key.
    pub answers: BTreeMap<String, JevAnswer>,
    /// Token usage; absent when the provider omits it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<JevUsage>,
}

/// One keyed Jev answer, tagged by the form of the question it answers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JevAnswer {
    /// Answer to a `noul` question.
    Noul {
        /// Probability, from 0 to 1, that the question holds.
        noul: f64,
    },
    /// Answer to a `choice` question.
    Choice {
        /// Name of the chosen option.
        choice: String,
        /// Confidence, from 0 to 1, in the chosen option.
        confidence: f64,
        /// Option name to its probability.
        probabilities: BTreeMap<String, f64>,
    },
    /// Answer to a `score` question.
    Score {
        /// Expected level on the scale, normalized from 0 to 1.
        score: f64,
        /// Level index, as a decimal string, to its description.
        legend: BTreeMap<String, String>,
        /// Confidence, from 0 to 1, in the most probable level; absent when
        /// the provider omits it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        confidence: Option<f64>,
        /// Level index to its probability; absent when the provider omits it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        probabilities: Option<BTreeMap<String, f64>>,
    },
}

/// Token usage of a [`JevResponse`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct JevUsage {
    /// Tokens of the state and questions.
    pub input_tokens: u64,
    /// Tokens of the answers.
    pub output_tokens: u64,
}

impl From<JevUsage> for TokenUsage {
    /// Jev reports only input and output tokens; cache and reasoning counts
    /// are zero.
    fn from(usage: JevUsage) -> Self {
        Self {
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            ..Self::default()
        }
    }
}

/// A request that breaks a documented Jev bound.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{field}: {reason}")]
pub struct JevRequestError {
    /// Request path of the offending member, such as `questions.tone.criteria`.
    pub field: String,
    /// Stable human-readable reason.
    pub reason: &'static str,
}

impl JevRequestError {
    /// Builds the rejection of `field`.
    fn new(field: impl Into<String>, reason: &'static str) -> Self {
        Self {
            field: field.into(),
            reason,
        }
    }
}

impl JevRequest {
    /// Checks the documented Jev bounds that decoding cannot express.
    ///
    /// The model must be non-empty; at least one question must be present, every key and
    /// instruction non-empty, a `choice` must offer 1 to
    /// [`JEV_MAX_CHOICE_OPTIONS`] named options, and a `score` must define
    /// [`JEV_MIN_SCORE_LEVELS`] to [`JEV_MAX_SCORE_LEVELS`] levels.
    ///
    /// # Errors
    ///
    /// Returns [`JevRequestError`] naming the first member that breaks a bound.
    pub fn validate(&self) -> Result<(), JevRequestError> {
        if self.model.trim().is_empty() {
            return Err(JevRequestError::new("model", "must be non-empty"));
        }
        if self.questions.is_empty() {
            return Err(JevRequestError::new(
                "questions",
                "must hold at least one question",
            ));
        }
        for (key, question) in &self.questions {
            if key.is_empty() {
                return Err(JevRequestError::new("questions", "keys must be non-empty"));
            }
            question.validate(&format!("questions.{key}"))?;
        }
        Ok(())
    }
}

impl JevQuestion {
    /// Checks this question's bounds, naming members below `field`.
    ///
    /// # Errors
    ///
    /// Returns [`JevRequestError`] for empty instructions, an empty or
    /// oversized option set, an empty option name, or a level count outside
    /// the score bounds.
    fn validate(&self, field: &str) -> Result<(), JevRequestError> {
        let instructions = match self {
            Self::Noul { instructions, .. }
            | Self::Choice { instructions, .. }
            | Self::Score { instructions, .. } => instructions,
        };
        if instructions.trim().is_empty() {
            return Err(JevRequestError::new(
                format!("{field}.instructions"),
                "must be non-empty",
            ));
        }
        match self {
            Self::Noul { .. } => Ok(()),
            Self::Choice { criteria, .. } => {
                if criteria.is_empty() || criteria.len() > JEV_MAX_CHOICE_OPTIONS {
                    Err(JevRequestError::new(
                        format!("{field}.criteria"),
                        "must offer 1 to 255 options",
                    ))
                } else if criteria.keys().any(String::is_empty) {
                    Err(JevRequestError::new(
                        format!("{field}.criteria"),
                        "option names must be non-empty",
                    ))
                } else {
                    Ok(())
                }
            }
            Self::Score { criteria, .. } => {
                if (JEV_MIN_SCORE_LEVELS..=JEV_MAX_SCORE_LEVELS).contains(&criteria.len()) {
                    Ok(())
                } else {
                    Err(JevRequestError::new(
                        format!("{field}.criteria"),
                        "must define 2 to 10 levels",
                    ))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Map, Value, json};

    /// Decodes and validates a request body.
    fn checked(body: Value) -> Result<JevRequest, String> {
        let request: JevRequest = serde_json::from_value(body).map_err(|e| e.to_string())?;
        request.validate().map_err(|e| e.to_string())?;
        Ok(request)
    }

    /// Every documented question form decodes, validates, and re-encodes to
    /// the same body.
    #[test]
    fn documented_forms_round_trip() {
        let body = json!({
            "model": "jev-latest",
            "state": "Thanks, \"happy\" to help!\nAnything else?",
            "questions": {
                "helpful": {"type": "noul", "instructions": "Is it helpful?",
                            "criteria": {"true": "helps", "false": "does not help"}},
                "plain": {"type": "noul", "instructions": "Is it plain?"},
                "tone": {"type": "choice", "instructions": "Tone?",
                         "criteria": {"polite": "courteous", "rude": "curt"}},
                "quality": {"type": "score", "instructions": "Quality?",
                            "criteria": ["poor", "ok", "great"]},
            }
        });
        let request = checked(body.clone()).expect("valid");
        assert_eq!(serde_json::to_value(&request).expect("encodes"), body);
    }

    /// Bad forms and broken bounds fail with the offending member named.
    #[test]
    fn bad_forms_and_bounds_are_rejected() {
        let with =
            |questions: Value| json!({"model": "jev-latest", "state": "s", "questions": questions});
        let options: Map<String, Value> = (0..256).map(|i| (format!("o{i}"), json!("d"))).collect();
        for (body, needle) in [
            (
                with(json!({"q": {"type": "essay", "instructions": "x"}})),
                "unknown variant",
            ),
            (
                with(json!({"q": {"type": "noul", "instructions": "x", "extra": 1}})),
                "unknown field",
            ),
            (
                with(json!({"q": {"type": "choice", "instructions": "x"}})),
                "missing field",
            ),
            (with(json!({})), "questions: must hold"),
            (
                with(json!({"q": {"type": "noul", "instructions": " "}})),
                "questions.q.instructions",
            ),
            (
                with(json!({"q": {"type": "choice", "instructions": "x", "criteria": {}}})),
                "1 to 255",
            ),
            (
                with(json!({"q": {"type": "choice", "instructions": "x", "criteria": options}})),
                "1 to 255",
            ),
            (
                with(json!({"q": {"type": "score", "instructions": "x", "criteria": ["one"]}})),
                "2 to 10",
            ),
            (
                with(
                    json!({"q": {"type": "score", "instructions": "x", "criteria": vec!["l"; 11]}}),
                ),
                "2 to 10",
            ),
            (
                json!({"model": "jev-latest", "state": 5, "questions": {"q": {"type": "noul", "instructions": "x"}}}),
                "invalid type",
            ),
            (
                json!({"model": "", "state": "s", "questions": {"q": {"type": "noul", "instructions": "x"}}}),
                "model",
            ),
            (
                json!({"model": "m", "state": "s", "questions": {}, "stream": true}),
                "unknown field",
            ),
        ] {
            let error = checked(body).expect_err(needle);
            assert!(error.contains(needle), "{error} should mention {needle}");
        }
    }

    /// Every documented answer form decodes typed, as the live provider
    /// sends it, and re-encodes to the same body; omitted usage stays absent.
    #[test]
    fn documented_answers_decode_typed() {
        let body = json!({
            "model": "jev-1.13.0",
            "answers": {
                "helpful": {"type": "noul", "noul": 0.93},
                "tone": {"type": "choice", "choice": "polite", "confidence": 0.76,
                         "probabilities": {"polite": 0.88, "rude": 0.12}},
                "quality": {"type": "score", "score": 0.19, "confidence": 0.71,
                            "legend": {"0": "poor", "1": "ok", "2": "great"},
                            "probabilities": {"0": 0.83, "1": 0.15, "2": 0.02}}
            },
            "usage": {"input_tokens": 378, "output_tokens": 65}
        });
        let response: JevResponse = serde_json::from_value(body.clone()).expect("decodes");
        assert_eq!(response.answers["helpful"], JevAnswer::Noul { noul: 0.93 });
        assert!(matches!(
            &response.answers["tone"],
            JevAnswer::Choice { choice, .. } if choice == "polite"
        ));
        assert!(matches!(
            &response.answers["quality"],
            JevAnswer::Score { legend, .. } if legend["2"] == "great"
        ));
        assert_eq!(
            TokenUsage::from(response.usage.expect("usage")).output_tokens,
            65
        );
        assert_eq!(serde_json::to_value(&response).expect("encodes"), body);
        let bare: JevResponse =
            serde_json::from_value(json!({"model": "m", "answers": {}})).expect("decodes");
        assert_eq!(bare.usage, None);
        assert!(
            serde_json::from_value::<JevResponse>(json!({"model": "m",
                "answers": {"q": {"type": "essay"}}}))
            .is_err(),
            "an unknown answer form is not a Jev answer"
        );
    }
}
