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

/// Returns the compiled text placeholder regex used by PromptCard validation.
///
/// Accepts both `${name}` and `{{name}}`. The `${media:name}` form remains
/// reserved for media binding and is not matched here.
pub fn text_placeholder_regex() -> &'static Regex {
    static PLACEHOLDER_RE: OnceLock<Regex> = OnceLock::new();
    PLACEHOLDER_RE.get_or_init(|| {
        match Regex::new(r"\$\{([a-zA-Z_][a-zA-Z0-9_]*)\}|\{\{([a-zA-Z_][a-zA-Z0-9_]*)\}\}") {
            Ok(regex) => regex,
            Err(error) => panic!("PromptCard placeholder regex is static and valid: {error}"),
        }
    })
}

/// Returns the compiled `${media:name}` placeholder regex used by PromptCard validation.
pub fn media_placeholder_regex() -> &'static Regex {
    static PLACEHOLDER_RE: OnceLock<Regex> = OnceLock::new();
    PLACEHOLDER_RE.get_or_init(
        || match Regex::new(r"\$\{media:([a-zA-Z_][a-zA-Z0-9_]*)\}") {
            Ok(regex) => regex,
            Err(error) => panic!("PromptCard media placeholder regex is static and valid: {error}"),
        },
    )
}

/// Extracts declared-style text placeholders from the native request JSON.
///
/// Results are deterministic: first-seen order is preserved and repeated names
/// appear once.
pub fn extract_text_placeholders(spec: &PromptSpec) -> Result<Vec<String>, WyrdError> {
    extract_with_regex(spec, text_placeholder_regex())
}

/// Extracts `${media:name}` placeholders from the native request JSON.
///
/// Results are deterministic: first-seen order is preserved and repeated names
/// appear once.
pub fn extract_media_placeholders(spec: &PromptSpec) -> Result<Vec<String>, WyrdError> {
    extract_with_regex(spec, media_placeholder_regex())
}

fn extract_with_regex(spec: &PromptSpec, regex: &Regex) -> Result<Vec<String>, WyrdError> {
    let json = serde_json::to_string(&spec.prompt.request).map_err(|error| {
        WyrdError::from(PromptError::SerializeRequest {
            message: error.to_string(),
        })
    })?;
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for capture in regex.captures_iter(&json) {
        let Some(name_match) = capture.get(1).or_else(|| capture.get(2)) else {
            continue;
        };
        let name = name_match.as_str().to_owned();
        if seen.insert(name.clone()) {
            out.push(name);
        }
    }
    Ok(out)
}
