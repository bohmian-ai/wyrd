use serde::{Deserialize, Serialize};

use crate::error::{SkaldError, SkaldResult};
use crate::request::ProviderRequest;

/// Authored native prompt request plus render metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Prompt {
    /// Native provider-shaped request authored with `{{variable}}` placeholders.
    pub request: ProviderRequest,
    /// Model identifier used for indexing and selection.
    pub model: String,
    /// Optional author-assigned prompt version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Declared placeholder names expected by `render`.
    #[serde(default)]
    pub variables: Vec<String>,
    /// Expected response shape for runtime validation.
    #[serde(default)]
    pub response_type: ResponseType,
}

/// Expected response shape for a rendered prompt.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ResponseType {
    /// Plain text response.
    #[default]
    Text,
    /// JSON response validated against a named schema by the runtime.
    JsonSchema {
        /// Schema name used in diagnostics and provider-native structured output fields.
        name: String,
        /// JSON Schema object.
        schema: serde_json::Value,
    },
}

impl Prompt {
    /// Render declared `{{name}}` placeholders into the native request.
    ///
    /// Values are escaped as JSON string content before substitution, so a
    /// variable value cannot inject new fields into the serialized provider
    /// request. The returned request remains in the same provider-native shape.
    pub fn render(&self, vars: &[(&str, &str)]) -> SkaldResult<ProviderRequest> {
        let mut json = serde_json::to_string(&self.request).map_err(SkaldError::serialize)?;

        for name in &self.variables {
            let value = vars
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| *value)
                .ok_or_else(|| SkaldError::missing_variable(name))?;
            json = json.replace(
                &format!("{{{{{name}}}}}"),
                &escape_json_string_content(value)?,
            );
        }

        serde_json::from_str(&json).map_err(SkaldError::deserialize)
    }
}

fn escape_json_string_content(value: &str) -> SkaldResult<String> {
    // `Prompt::render` substitutes inside a JSON string that already has
    // surrounding quotes from `serde_json::to_string(&request)`. Encode as a
    // JSON string, then remove only those wrapper quotes so escape sequences
    // like `\"` and `\\` remain intact in the provider request JSON.
    let encoded = serde_json::to_string(value).map_err(SkaldError::serialize)?;
    Ok(encoded
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(&encoded)
        .to_string())
}
