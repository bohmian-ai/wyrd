use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{SkaldError, SkaldResult};
use crate::request::ProviderRequest;

/// Authored native prompt request plus render metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
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
    /// Return a copy with the supplied `{{name}}` placeholders bound.
    ///
    /// This is the reusable primitive behind higher-level prompt binding. It performs
    /// replacement inside JSON string leaves after serializing the native
    /// request, then deserializes back into the same provider-native enum.
    pub fn bind(&self, vars: &[(&str, &str)]) -> SkaldResult<Self> {
        let mut prompt = self.clone();
        prompt.bind_mut(vars)?;
        Ok(prompt)
    }

    /// Bind supplied `{{name}}` placeholders into this prompt in place.
    ///
    /// Unlike `render`, this may bind only a subset of declared variables. Any
    /// names still present in `variables` remain required for a later render.
    pub fn bind_mut(&mut self, vars: &[(&str, &str)]) -> SkaldResult<()> {
        let mut request = serde_json::to_value(&self.request).map_err(SkaldError::serialize)?;
        replace_string_leaves(&mut request, vars);
        self.request = serde_json::from_value(request).map_err(SkaldError::deserialize)?;
        self.variables
            .retain(|name| !vars.iter().any(|(key, _)| key == name));
        Ok(())
    }

    /// Render declared `{{name}}` placeholders into the native request.
    ///
    /// Values are escaped as JSON string content before substitution, so a
    /// variable value cannot inject new fields into the serialized provider
    /// request. The returned request remains in the same provider-native shape.
    pub fn render(&self, vars: &[(&str, &str)]) -> SkaldResult<ProviderRequest> {
        for name in &self.variables {
            vars.iter()
                .find(|(key, _)| key == name)
                .ok_or_else(|| SkaldError::missing_variable(name))?;
        }

        Ok(self.bind(vars)?.request)
    }
}

fn replace_string_leaves(value: &mut Value, vars: &[(&str, &str)]) {
    match value {
        Value::String(text) => {
            for (name, replacement) in vars {
                *text = text.replace(&format!("{{{{{name}}}}}"), replacement);
            }
        }
        Value::Array(values) => {
            for value in values {
                replace_string_leaves(value, vars);
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                replace_string_leaves(value, vars);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}
