use serde::{Deserialize, Serialize};

use crate::card::prompt::{ParameterName, validate};
use crate::error::WyrdError;

/// PromptCard spec body wrapping the native Skald prompt.
///
/// Serializes with the `prompt` fields flattened directly into the spec body
/// (no `prompt:` nesting key on the wire). Accepts two input forms:
///
/// - **Declarative** (`provider` present, `request` absent): a `PromptDraft`
///   that is compiled into the native provider request.
/// - **Native** (`request` present): the stored native `Prompt` shape, as
///   produced by `save()`.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PromptSpec {
    /// Native authored prompt. Provider-specific request shape lives here.
    #[serde(flatten)]
    #[cfg_attr(feature = "server", schema(value_type = serde_json::Value))]
    pub prompt: skald_spec::Prompt,
}

impl PromptSpec {
    /// Builds a prompt spec and runs the six PromptCard envelope invariants.
    pub fn new(prompt: skald_spec::Prompt) -> Result<Self, WyrdError> {
        let spec = Self { prompt };
        validate(&spec)?;
        Ok(spec)
    }

    /// Returns declared variables as validated parameter names.
    pub fn parameters(&self) -> Vec<ParameterName> {
        self.prompt
            .variables
            .iter()
            .filter_map(|name| ParameterName::new(name.clone()).ok())
            .collect()
    }

    /// Computes `sha256:<hex64>` over native prompt JSON without `prompt.version`.
    pub fn content_hash(&self) -> String {
        crate::card::prompt::content_hash(self)
    }

    /// Returns true when the native prompt declares no variables.
    pub fn is_fully_bound(&self) -> bool {
        self.prompt.variables.is_empty()
    }
}

impl<'de> Deserialize<'de> for PromptSpec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;

        let prompt = if value.get("provider").is_some() && value.get("request").is_none() {
            // Declarative path: compile via PromptDraft.
            let draft: skald_spec::PromptDraft =
                serde_json::from_value(value).map_err(serde::de::Error::custom)?;
            draft.compile().map_err(serde::de::Error::custom)?
        } else {
            // Native path: deserialize the stored Prompt shape directly.
            serde_json::from_value(value).map_err(serde::de::Error::custom)?
        };

        let spec = Self { prompt };
        validate(&spec).map_err(serde::de::Error::custom)?;
        Ok(spec)
    }
}
