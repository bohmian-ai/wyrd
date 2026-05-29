//! Prompt parameter names and placeholder extraction.

use std::collections::HashSet;
use std::fmt;
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::card::prompt::PromptSpec;
use crate::card::prompt::validate::PromptError;
use crate::error::WyrdError;

/// Valid prompt parameter name matching `^[a-zA-Z_][a-zA-Z0-9_]*$`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct ParameterName(String);

impl ParameterName {
    /// Creates a parameter name after validating the Wyrd prompt grammar.
    pub fn new(value: impl Into<String>) -> Result<Self, WyrdError> {
        let value = value.into();
        if !is_valid_parameter_name(&value) {
            return Err(PromptError::InvalidVariableName { name: value }.into());
        }
        Ok(Self(value))
    }

    /// Returns the validated name as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ParameterName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Returns true when `value` is a valid prompt parameter name.
pub fn is_valid_parameter_name(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// Returns the compiled `{{name}}` placeholder regex used by PromptCard validation.
pub fn placeholder_regex() -> &'static Regex {
    static PLACEHOLDER_RE: OnceLock<Regex> = OnceLock::new();
    PLACEHOLDER_RE.get_or_init(|| {
        Regex::new(r"\{\{([a-zA-Z_][a-zA-Z0-9_]*)\}\}")
            .expect("PromptCard placeholder regex is static and valid")
    })
}

/// Extracts declared-style `{{name}}` placeholders from the native request JSON.
///
/// Results are deterministic: first-seen order is preserved and repeated names
/// appear once.
pub fn extract_placeholders(spec: &PromptSpec) -> Result<Vec<String>, WyrdError> {
    let json = serde_json::to_string(&spec.prompt.request).map_err(|error| {
        WyrdError::from(PromptError::SerializeRequest {
            message: error.to_string(),
        })
    })?;
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for capture in placeholder_regex().captures_iter(&json) {
        let name = capture[1].to_owned();
        if seen.insert(name.clone()) {
            out.push(name);
        }
    }
    Ok(out)
}
