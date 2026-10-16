use serde::{Deserialize, Serialize};

use crate::card::prompt::{ParameterName, validate};
use crate::error::WyrdError;

/// PromptCard spec body: the authored Prompt, portable or native.
///
/// The wire form is [`skald_spec::AuthoredPrompt`] directly in the spec body;
/// decoding compiles it into the runtime Prompt, and serializing writes the
/// authored form back unchanged.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PromptSpec {
    /// Runtime Prompt compiled from, and serialized as, the authored form.
    #[serde(flatten)]
    #[cfg_attr(feature = "server", schema(value_type = serde_json::Value))]
    pub prompt: skald_spec::Prompt,
}

impl PromptSpec {
    /// Builds a prompt spec from the Prompt's authored form.
    ///
    /// The Prompt is recompiled from [`skald_spec::Prompt::authored`], so the
    /// spec always holds exactly what its serialized form decodes to.
    ///
    /// # Errors
    ///
    /// Returns the authoring error when the Prompt has no authorable form or
    /// that form does not compile, and the parameter-name error for a
    /// placeholder outside the grammar.
    pub fn new(prompt: skald_spec::Prompt) -> Result<Self, WyrdError> {
        let prompt = prompt.authored()?.compile()?;
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

    /// Computes `sha256:<hex64>` over the authored Prompt JSON.
    pub fn content_hash(&self) -> String {
        crate::card::prompt::content_hash(self)
    }

    /// Returns true when the prompt references no text variables.
    pub fn is_fully_bound(&self) -> bool {
        self.prompt.variables.is_empty()
    }
}

impl<'de> Deserialize<'de> for PromptSpec {
    /// Decodes the authored Prompt form, which compiles it, then applies the
    /// Prompt Card validation `PromptSpec::new` applies.
    ///
    /// # Errors
    /// Returns the deserializer's error when the authored form does not decode
    /// or compile, or a placeholder is outside the parameter-name grammar.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let spec = Self {
            prompt: skald_spec::Prompt::deserialize(deserializer)?,
        };
        validate(&spec).map_err(serde::de::Error::custom)?;
        Ok(spec)
    }
}
