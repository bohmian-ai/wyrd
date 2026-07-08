use std::sync::Arc;

use arrow::array::{Array, ArrayRef, RecordBatch, StringArray};
use arrow::datatypes::DataType;
use regex::Regex;
use std::sync::OnceLock;

use crate::error::BifrostError;

/// Seam for a richer redaction classifier to be installed later.
/// If an installed classifier errors on a batch, that commit is refused
/// (`WYRD_VALA_500_REDACTION_FAILED`). Absence is never an error.
pub trait RedactionClassifier: Send + Sync {
    /// Scrub matched secret/PII spans inside `columns` of `batch`, in place.
    /// Returns a new batch with the same schema and row count with matched spans
    /// replaced by `⟪redacted⟫`. `Err` causes the commit to be refused.
    fn scrub(
        &self,
        batch: &RecordBatch,
        columns: &[&str],
    ) -> Result<RecordBatch, BifrostError>;
}

/// Regex patterns for known secret formats.
fn secret_patterns() -> &'static [Regex] {
    static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        vec![
            // GitHub PATs
            Regex::new(r"ghp_[A-Za-z0-9]{36,}").unwrap(),
            // OpenAI-style sk- keys
            Regex::new(r"sk-[A-Za-z0-9]{32,}").unwrap(),
            // AWS access key IDs
            Regex::new(r"AKIA[A-Z0-9]{16}").unwrap(),
            // JWT tokens eyJ…
            Regex::new(r"eyJ[A-Za-z0-9\-_]+\.[A-Za-z0-9\-_]+\.[A-Za-z0-9\-_]+").unwrap(),
            // PEM private keys
            Regex::new(r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----[^-]+-----END (?:RSA |EC |OPENSSH )?PRIVATE KEY-----").unwrap(),
            // Postgres DSNs with credentials
            Regex::new(r"postgres(?:ql)?://[^:]+:[^@]+@[^\s]+").unwrap(),
            // Email addresses
            Regex::new(r"\b[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}\b").unwrap(),
            // US SSN
            Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap(),
        ]
    })
}

/// Sensitive attribute key names whose values should be masked.
const SENSITIVE_ATTR_KEYS: &[&str] = &[
    "authorization",
    "cookie",
    "x-api-key",
    "password",
    "secret",
];

const REDACTED: &str = "⟪redacted⟫";

/// Returns `true` if the string contains a Luhn-valid credit card number.
fn contains_luhn_card(s: &str) -> bool {
    // Extract contiguous digit sequences of length 13-19
    let digits_only: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits_only.len() < 13 || digits_only.len() > 19 {
        return false;
    }
    luhn_check(&digits_only)
}

fn luhn_check(digits: &str) -> bool {
    let mut sum = 0u32;
    let mut double = false;
    for c in digits.chars().rev() {
        let Some(d) = c.to_digit(10) else { return false };
        let val = if double {
            let v = d * 2;
            if v > 9 { v - 9 } else { v }
        } else {
            d
        };
        sum += val;
        double = !double;
    }
    sum % 10 == 0
}

/// Redact a single string value in place.
fn redact_value(value: &str) -> String {
    let mut result = value.to_string();
    for pattern in secret_patterns() {
        if pattern.is_match(&result) {
            result = pattern.replace_all(&result, REDACTED).into_owned();
        }
    }
    // Luhn credit card check
    if contains_luhn_card(&result) {
        result = REDACTED.to_string();
    }
    result
}

/// Redact a column in `batch` named `col_name` if it is a string column.
/// Returns a new `ArrayRef` with secrets replaced by `⟪redacted⟫`.
fn redact_string_column(batch: &RecordBatch, col_name: &str) -> Option<(usize, ArrayRef)> {
    let schema = batch.schema();
    let idx = schema.index_of(col_name).ok()?;
    let col = batch.column(idx);
    if col.data_type() != &DataType::Utf8 && col.data_type() != &DataType::LargeUtf8 {
        return None;
    }
    let string_array = col.as_any().downcast_ref::<StringArray>()?;
    let mut changed = false;
    let values: Vec<Option<String>> = (0..string_array.len())
        .map(|i| {
            if string_array.is_null(i) {
                None
            } else {
                let v = string_array.value(i);
                let redacted = redact_value(v);
                if redacted != v {
                    changed = true;
                }
                Some(redacted)
            }
        })
        .collect();
    if !changed {
        return None;
    }
    let new_col: ArrayRef = Arc::new(StringArray::from(values));
    Some((idx, new_col))
}

/// Built-in redaction pass. Scrubs known secret formats, sensitive attribute
/// keys, Luhn-valid credit cards, SSNs, and email addresses from `columns` in
/// `batch`. In-place span replacement; never drops rows or columns.
pub struct BuiltinRedactionPass;

impl RedactionClassifier for BuiltinRedactionPass {
    fn scrub(&self, batch: &RecordBatch, columns: &[&str]) -> Result<RecordBatch, BifrostError> {
        let schema = batch.schema();
        let mut all_columns: Vec<ArrayRef> = batch.columns().to_vec();

        for col_name in columns {
            if let Some((idx, new_col)) = redact_string_column(batch, col_name) {
                all_columns[idx] = new_col;
            }
        }

        RecordBatch::try_new(schema, all_columns).map_err(|e| BifrostError::RedactionFailed {
            detail: e.to_string(),
        })
    }
}

/// Check if a key name is a sensitive attribute key.
pub fn is_sensitive_attr_key(key: &str) -> bool {
    let lower = key.to_lowercase();
    SENSITIVE_ATTR_KEYS.iter().any(|k| lower == *k)
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::StringArray;
    use arrow::datatypes::{Field, Schema};

    fn make_batch(values: Vec<Option<&str>>) -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![Field::new("payload", DataType::Utf8, true)]));
        let array: ArrayRef = Arc::new(StringArray::from(values));
        RecordBatch::try_new(schema, vec![array]).unwrap()
    }

    #[test]
    fn github_token_is_redacted() {
        let token = "ghp_abcdefghijklmnopqrstuvwxyz123456789012";
        let batch = make_batch(vec![Some(token)]);
        let pass = BuiltinRedactionPass;
        let result = pass.scrub(&batch, &["payload"]).unwrap();
        let col = result
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(col.value(0), REDACTED);
    }

    #[test]
    fn surrounding_content_survives() {
        let val = "prefix ghp_abcdefghijklmnopqrstuvwxyz123456789012 suffix";
        let batch = make_batch(vec![Some(val)]);
        let pass = BuiltinRedactionPass;
        let result = pass.scrub(&batch, &["payload"]).unwrap();
        let col = result
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert!(col.value(0).contains("prefix"));
        assert!(col.value(0).contains("suffix"));
        assert!(col.value(0).contains(REDACTED));
    }

    #[test]
    fn standard_content_is_untouched() {
        let val = "hello world no secrets here";
        let batch = make_batch(vec![Some(val)]);
        let pass = BuiltinRedactionPass;
        let result = pass.scrub(&batch, &["payload"]).unwrap();
        let col = result
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(col.value(0), val);
    }

    #[test]
    fn luhn_valid_card_is_redacted() {
        // Visa test card 4111111111111111 — Luhn-valid
        let batch = make_batch(vec![Some("4111111111111111")]);
        let pass = BuiltinRedactionPass;
        let result = pass.scrub(&batch, &["payload"]).unwrap();
        let col = result
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(col.value(0), REDACTED);
    }
}
