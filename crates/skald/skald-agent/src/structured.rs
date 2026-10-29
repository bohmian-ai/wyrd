//! Validation and repair feedback for structured-output Agent answers.

use std::fmt::Write as _;

use jsonschema::error::ValidationErrorKind;
use jsonschema::{ValidationError, Validator};
use serde_json::{Map, Value};

use crate::error::{AgentError, AgentResult};

/// Most violations one correction or error lists.
const MAX_VIOLATIONS: usize = 10;

/// Most UTF-8 bytes of one correction turn.
const MAX_CORRECTION_BYTES: usize = 16 * 1024;

/// Most UTF-8 bytes of one violation message inside a correction.
const MAX_MESSAGE_BYTES: usize = 400;

/// Most UTF-8 bytes of one schema fragment inside a correction, sized so
/// [`MAX_VIOLATIONS`] entries fit in [`MAX_CORRECTION_BYTES`].
const MAX_FRAGMENT_BYTES: usize = 900;

/// Opening of every correction turn.
const CORRECTION_OPENING: &str = "Your previous reply was not accepted because it does not satisfy the required JSON schema. Reply again with only a JSON object that satisfies the schema; do not repeat the rejected reply.";

/// The exact response schema an outgoing request sent, compiled once and
/// checked against every proposed final answer while that schema is sent.
pub(crate) struct StructuredOutput {
    /// The schema sent with the run's requests.
    schema: Value,
    /// Its compiled validator.
    validator: Validator,
}

/// Why a proposed final answer was refused.
pub(crate) struct Rejection {
    /// Bounded correction for the model, citing the reply and its schema.
    /// It is conversation content, never error or log content.
    pub(crate) correction: String,
    /// At most [`MAX_VIOLATIONS`] schema locations or fixed diagnostics,
    /// never reply values, reply keys, or parser text.
    pub(crate) violations: Vec<String>,
}

impl StructuredOutput {
    /// Compiles `schema`, the response schema an outgoing request sends.
    ///
    /// # Errors
    ///
    /// Returns [`AgentError::Prompt`] when `schema` does not compile, which a
    /// binding or `before_model` hook can cause.
    pub(crate) fn new(agent: &str, schema: &Value) -> AgentResult<Self> {
        let validator = jsonschema::validator_for(schema).map_err(|_| AgentError::Prompt {
            agent: agent.to_owned(),
            detail: "the response schema does not compile".to_owned(),
        })?;
        Ok(Self {
            schema: schema.clone(),
            validator,
        })
    }

    /// The schema this validator was compiled from.
    pub(crate) const fn schema(&self) -> &Value {
        &self.schema
    }

    /// Accepts `output` when it is a JSON object satisfying the schema.
    ///
    /// # Errors
    ///
    /// Returns a [`Rejection`] for malformed JSON, a non-object, or a schema
    /// violation.
    pub(crate) fn check(&self, output: &str) -> Result<Map<String, Value>, Rejection> {
        let value: Value = match serde_json::from_str(output.trim()) {
            Ok(value) => value,
            Err(error) => {
                return Err(self.whole(
                    &format!("The reply is not valid JSON ({error})."),
                    "reply is not valid JSON",
                ));
            }
        };
        if !value.is_object() {
            return Err(self.whole(
                "The reply is valid JSON but not a JSON object.",
                "reply is not a JSON object",
            ));
        }
        let errors: Vec<ValidationError<'_>> = self.validator.iter_errors(&value).collect();
        if errors.is_empty() {
            let Value::Object(map) = value else {
                unreachable!("checked to be an object");
            };
            return Ok(map);
        }
        Err(self.violations(&errors))
    }

    /// A rejection of the whole reply, showing the full schema.
    fn whole(&self, problem: &str, violation: &str) -> Rejection {
        let mut correction = format!("{CORRECTION_OPENING}\n\n{problem}\n\nRequired schema:\n");
        let budget = MAX_CORRECTION_BYTES.saturating_sub(correction.len());
        correction.push_str(&clip(&self.schema.to_string(), budget));
        Rejection {
            correction,
            violations: vec![violation.to_owned()],
        }
    }

    /// A rejection listing the first schema violations with their rules and
    /// schema fragments, and saying how many it omitted.
    fn violations(&self, errors: &[ValidationError<'_>]) -> Rejection {
        let mut correction = format!("{CORRECTION_OPENING}\n\nViolations:\n");
        let mut violations = Vec::new();
        let mut listed = 0;
        for error in errors.iter().take(MAX_VIOLATIONS) {
            let entry = self.entry(listed + 1, error);
            // Leave room for the omission note.
            if correction.len() + entry.len() + 64 > MAX_CORRECTION_BYTES {
                break;
            }
            correction.push_str(&entry);
            violations.push(error.schema_path().to_string());
            listed += 1;
        }
        let omitted = errors.len() - listed;
        if omitted > 0 {
            let _ = write!(correction, "\n{omitted} further violations omitted.\n");
        }
        Rejection {
            correction,
            violations,
        }
    }

    /// One numbered violation: where, what rule, why, and the schema node
    /// declaring the rule, with each union alternative's first failure.
    fn entry(&self, number: usize, error: &ValidationError<'_>) -> String {
        let schema_path = error.schema_path().to_string();
        let mut entry = format!(
            "{number}. At `{}`: {} (rule `{}` at schema `{schema_path}`).\n",
            location(&error.instance_path().to_string()),
            clip(&error.to_string(), MAX_MESSAGE_BYTES),
            error.kind().keyword(),
        );
        if let ValidationErrorKind::AnyOf { context }
        | ValidationErrorKind::OneOfNotValid { context } = error.kind()
        {
            entry.push_str("   The value must match one of these shapes; each failed because:\n");
            for (index, failures) in context.iter().enumerate() {
                if let Some(first) = failures.first() {
                    let _ = writeln!(
                        entry,
                        "   - shape {}: {}",
                        index + 1,
                        clip(&first.to_string(), MAX_MESSAGE_BYTES)
                    );
                }
            }
        }
        let node = schema_path
            .rsplit_once('/')
            .and_then(|(parent, _)| self.schema.pointer(parent));
        if let Some(node) = node {
            let _ = writeln!(
                entry,
                "   Schema: {}",
                clip(&node.to_string(), MAX_FRAGMENT_BYTES)
            );
        }
        entry
    }
}

/// The instance location, with the root shown as `/`.
fn location(pointer: &str) -> &str {
    if pointer.is_empty() { "/" } else { pointer }
}

/// `text` in at most `budget` bytes, marking any clip.
fn clip(text: &str, budget: usize) -> String {
    if text.len() <= budget {
        return text.to_owned();
    }
    let note = format!(" … [clipped: {} bytes omitted]", text.len());
    let mut end = budget.saturating_sub(note.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{note}", &text[..end])
}

#[cfg(test)]
mod tests {
    //! Bounds and safety of structured-output rejections.

    use serde_json::json;

    use super::*;

    /// Malformed JSON, a non-object, and a violation are refused with fixed
    /// or schema-location violations, and a valid object is accepted.
    ///
    /// # Panics
    /// Panics when an answer is judged on the wrong side or a violation
    /// carries reply content.
    #[test]
    fn check_refuses_with_safe_violations() {
        let schema = json!({"type": "object", "properties": {"a": {"enum": ["x"]}}, "additionalProperties": false});
        let output = StructuredOutput::new("a", &schema).expect("compiles");
        assert_eq!(
            output.check(r#"{"a": "x"}"#).ok(),
            Some(json!({"a": "x"}).as_object().cloned().expect("object"))
        );
        for (reply, violation) in [
            ("{\"secret\":", "reply is not valid JSON"),
            ("[\"secret\"]", "reply is not a JSON object"),
            ("{\"a\": \"secret\"}", "/properties/a/enum"),
            ("{\"secret\": 1}", "/additionalProperties"),
        ] {
            let rejection = output.check(reply).expect_err("refused");
            assert_eq!(rejection.violations, vec![violation.to_owned()], "{reply}");
            assert!(rejection.correction.starts_with(CORRECTION_OPENING));
        }
    }

    /// A clip respects the budget and character boundaries.
    ///
    /// # Panics
    /// Panics when a clip exceeds its budget or lacks its note.
    #[test]
    fn clip_bounds_text() {
        let clipped = clip(&"é".repeat(1_000), 101);
        assert!(clipped.len() <= 101, "{}", clipped.len());
        assert!(clipped.contains("omitted"));
        assert_eq!(clip("1", 10), "1");
    }
}
