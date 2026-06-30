//! One evaluation result attached to a GenAI span.
//!
//! OTel GenAI semantic convention: `gen_ai.evaluation.result` event.
//!
//! Producers emit one [`SpanEvent`](crate::vala::trace::SpanEvent) per
//! evaluation, with the event name `gen_ai.evaluation.result` and attributes:
//!
//! - `gen_ai.evaluation.name` - required
//! - `gen_ai.evaluation.score.label` - optional categorical label
//! - `gen_ai.evaluation.score.value` - optional numeric score
//! - `gen_ai.evaluation.explanation` - optional human-readable explanation
//! - `gen_ai.response.id` - optional link to the model response that was
//!   evaluated
//!
//! See the constants in [`crate::vala::trace::attributes`]:
//! [`GEN_AI_EVALUATION_EVENT`](crate::vala::trace::attributes::GEN_AI_EVALUATION_EVENT),
//! [`GEN_AI_EVALUATION_NAME`](crate::vala::trace::attributes::GEN_AI_EVALUATION_NAME),
//! [`GEN_AI_EVALUATION_SCORE_LABEL`](crate::vala::trace::attributes::GEN_AI_EVALUATION_SCORE_LABEL),
//! [`GEN_AI_EVALUATION_SCORE_VALUE`](crate::vala::trace::attributes::GEN_AI_EVALUATION_SCORE_VALUE),
//! [`GEN_AI_EVALUATION_EXPLANATION`](crate::vala::trace::attributes::GEN_AI_EVALUATION_EXPLANATION).
//!
//! Extraction from `SpanEvent` to `GenAiEvalResult` happens in the GenAI
//! projector. Wyrd-side, that is `vala-bifrost` or the OTLP receiver
//! (`vala-ingest`). This crate just publishes the shape.

use serde::{Deserialize, Serialize};

use crate::error::WyrdError;

/// One evaluation result sourced from a `gen_ai.evaluation.result` event on
/// the parent GenAI span.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GenAiEvalResult {
    /// `gen_ai.evaluation.name` - required free-form identifier of the
    /// evaluation, such as `"factuality"`, `"safety"`, or `"toxicity"`.
    pub name: String,
    /// `gen_ai.evaluation.score.label` - optional categorical label, such as
    /// `"pass"`, `"fail"`, or `"borderline"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_label: Option<String>,
    /// `gen_ai.evaluation.score.value` - optional finite numeric score.
    ///
    /// NaN and infinite values are rejected by [`GenAiEvalResult::validate`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_value: Option<f64>,
    /// `gen_ai.evaluation.explanation` - optional human-readable explanation.
    ///
    /// The explanation is length-bounded at 4096 characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
    /// `gen_ai.response.id` - optional link to the model response that was
    /// evaluated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_id: Option<String>,
}

impl GenAiEvalResult {
    /// Validate the evaluation-result invariants.
    ///
    /// Enforces:
    ///
    /// - `name` non-empty and <= 256 characters
    /// - `score_value` finite when present
    /// - `score_label` <= 64 characters when present
    /// - `explanation` <= 4096 characters when present
    /// - `response_id` <= 128 characters when present
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when any invariant fails.
    pub fn validate(&self) -> Result<(), WyrdError> {
        if self.name.is_empty() {
            return Err(WyrdError::Validation {
                message: "gen_ai_eval_result.name must be non-empty".to_string(),
                details: serde_json::Value::Null,
            });
        }
        if self.name.len() > 256 {
            return Err(WyrdError::Validation {
                message: format!(
                    "gen_ai_eval_result.name length must be <= 256, got {}",
                    self.name.len()
                ),
                details: serde_json::Value::Null,
            });
        }
        if let Some(value) = self.score_value
            && !value.is_finite()
        {
            return Err(WyrdError::Validation {
                message: format!("gen_ai_eval_result.score_value must be finite, got {value}"),
                details: serde_json::Value::Null,
            });
        }
        if let Some(label) = &self.score_label
            && label.len() > 64
        {
            return Err(WyrdError::Validation {
                message: format!(
                    "gen_ai_eval_result.score_label length must be <= 64, got {}",
                    label.len()
                ),
                details: serde_json::Value::Null,
            });
        }
        if let Some(explanation) = &self.explanation
            && explanation.len() > 4096
        {
            return Err(WyrdError::Validation {
                message: format!(
                    "gen_ai_eval_result.explanation length must be <= 4096, got {}",
                    explanation.len()
                ),
                details: serde_json::Value::Null,
            });
        }
        if let Some(response_id) = &self.response_id
            && response_id.len() > 128
        {
            return Err(WyrdError::Validation {
                message: format!(
                    "gen_ai_eval_result.response_id length must be <= 128, got {}",
                    response_id.len()
                ),
                details: serde_json::Value::Null,
            });
        }
        Ok(())
    }
}
