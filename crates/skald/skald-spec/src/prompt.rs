use serde::{Deserialize, Serialize};

use crate::error::{SkaldError, SkaldResult};
use crate::request::ProviderRequest;

/// Authored native prompt request plus render metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Prompt {
    pub request: ProviderRequest,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default)]
    pub variables: Vec<String>,
    #[serde(default)]
    pub response_type: ResponseType,
}

/// Expected response shape for a rendered prompt.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ResponseType {
    #[default]
    Text,
    JsonSchema {
        name: String,
        schema: serde_json::Value,
    },
}

impl Prompt {
    /// Render declared `{{name}}` placeholders into the native request.
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
    let encoded = serde_json::to_string(value).map_err(SkaldError::serialize)?;
    Ok(encoded
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(&encoded)
        .to_string())
}
