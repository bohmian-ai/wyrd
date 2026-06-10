//! Payload redaction for observations emitted to Vala.

use serde_json::value::RawValue;
use serde_json::{Map, Value};

/// Placeholder substituted for blocked values.
pub const REDACTED_PLACEHOLDER: &str = "<REDACTED>";

/// Redaction policy for tool arguments and provider content.
///
/// Redaction operates on JSON field names only. Values that contain secret
/// patterns (e.g. an API key echoed back in a response body) are not redacted
/// unless the enclosing field name is in the blocklist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedactionPolicy {
    /// Case-insensitive tool argument field names to replace.
    pub blocked_arg_fields: Vec<String>,
    /// Maximum text length to emit. Zero disables truncation.
    pub max_content_chars: usize,
    /// Whether response text should be stripped entirely.
    pub strip_response_text: bool,
}

impl RedactionPolicy {
    /// Conservative default policy used until tenant policy is loaded.
    #[must_use]
    pub fn strict() -> Self {
        Self {
            blocked_arg_fields: vec![
                "password".to_owned(),
                "api_key".to_owned(),
                "secret".to_owned(),
                "token".to_owned(),
                "authorization".to_owned(),
            ],
            max_content_chars: 2_000,
            strip_response_text: false,
        }
    }

    /// Apply recursive field-name redaction to a raw tool argument payload.
    #[must_use]
    pub fn redact_tool_args(&self, _tool: &str, args: &RawValue) -> Box<RawValue> {
        let Ok(mut value) = serde_json::from_str::<Value>(args.get()) else {
            return placeholder_raw();
        };
        let blocked_lower: Vec<String> = self
            .blocked_arg_fields
            .iter()
            .map(|field| field.to_lowercase())
            .collect();
        redact_in_place(&mut value, &blocked_lower);
        RawValue::from_string(value.to_string()).unwrap_or_else(|_| placeholder_raw())
    }

    /// Apply recursive field-name redaction to a `serde_json::Value` payload.
    #[must_use]
    pub fn redact_value(&self, mut args: Value) -> Value {
        let blocked_lower: Vec<String> = self
            .blocked_arg_fields
            .iter()
            .map(|field| field.to_lowercase())
            .collect();
        redact_in_place(&mut args, &blocked_lower);
        args
    }

    /// Redact or truncate provider response text before observation.
    #[must_use]
    pub fn redact_response_text(&self, text: &str) -> String {
        if self.strip_response_text {
            return REDACTED_PLACEHOLDER.to_owned();
        }
        if self.max_content_chars == 0 {
            return text.to_owned();
        }
        let mut chars = text.chars();
        let truncated: String = chars.by_ref().take(self.max_content_chars).collect();
        if chars.next().is_some() {
            format!("{truncated}... [truncated]")
        } else {
            truncated
        }
    }
}

impl Default for RedactionPolicy {
    fn default() -> Self {
        Self::strict()
    }
}

fn placeholder_raw() -> Box<RawValue> {
    match RawValue::from_string(format!("\"{REDACTED_PLACEHOLDER}\"")) {
        Ok(raw) => raw,
        Err(_) => unreachable!("a JSON string literal is always a valid RawValue"),
    }
}

fn redact_in_place(value: &mut Value, blocked_lower: &[String]) {
    match value {
        Value::Object(map) => redact_object(map, blocked_lower),
        Value::Array(values) => {
            for value in values {
                redact_in_place(value, blocked_lower);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn redact_object(map: &mut Map<String, Value>, blocked_lower: &[String]) {
    for (key, value) in map.iter_mut() {
        if blocked_lower
            .iter()
            .any(|blocked| blocked == &key.to_lowercase())
        {
            *value = Value::String(REDACTED_PLACEHOLDER.to_owned());
        } else {
            redact_in_place(value, blocked_lower);
        }
    }
}
