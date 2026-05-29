//! PromptCard envelope type.

use serde::{Deserialize, Serialize};

use crate::card::prompt::{ParameterName, validate};
use crate::error::WyrdError;

/// PromptCard spec body wrapping the native Skald prompt.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct PromptSpec {
    /// Native authored prompt. Provider-specific request shape lives here.
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
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            prompt: skald_spec::Prompt,
        }

        let raw = Raw::deserialize(deserializer)?;
        let spec = Self { prompt: raw.prompt };
        validate(&spec).map_err(serde::de::Error::custom)?;
        Ok(spec)
    }
}
