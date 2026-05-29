//! Pure PromptCard serde codecs.

use crate::card::prompt::PromptSpec;
use crate::card::prompt::validate::PromptError;
use crate::envelope::Card;
use crate::error::WyrdError;

/// Supported pure PromptCard byte formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum CardLoadFormat {
    /// JSON card or spec bytes.
    Json,
    /// YAML card or spec bytes.
    Yaml,
}

impl CardLoadFormat {
    /// Detects the card format from a path extension without filesystem IO.
    pub fn from_extension(ext: Option<&str>) -> Result<Self, WyrdError> {
        match ext.map(str::to_ascii_lowercase).as_deref() {
            Some("json") => Ok(Self::Json),
            Some("yaml" | "yml") => Ok(Self::Yaml),
            other => Err(PromptError::LoaderBadExtension {
                extension: other.map(ToOwned::to_owned),
            }
            .into()),
        }
    }
}

/// Parses a full Prompt Card envelope from bytes.
pub fn parse_card_bytes(format: CardLoadFormat, bytes: &[u8]) -> Result<Card, WyrdError> {
    match format {
        CardLoadFormat::Json => {
            serde_json::from_slice(bytes).map_err(|error| WyrdError::Validation {
                message: "failed to parse prompt card JSON".to_owned(),
                details: serde_json::json!({ "source": error.to_string() }),
            })
        }
        CardLoadFormat::Yaml => {
            serde_yaml::from_slice(bytes).map_err(|error| WyrdError::Validation {
                message: "failed to parse prompt card YAML".to_owned(),
                details: serde_json::json!({ "source": error.to_string() }),
            })
        }
    }
}

/// Serializes a full Prompt Card envelope to bytes.
pub fn serialize_card(format: CardLoadFormat, card: &Card) -> Result<Vec<u8>, WyrdError> {
    match format {
        CardLoadFormat::Json => {
            serde_json::to_vec_pretty(card).map_err(|error| WyrdError::Internal {
                message: "failed to serialize prompt card JSON".to_owned(),
                details: serde_json::json!({ "source": error.to_string() }),
            })
        }
        CardLoadFormat::Yaml => {
            serde_yaml::to_string(card)
                .map(String::into_bytes)
                .map_err(|error| WyrdError::Internal {
                    message: "failed to serialize prompt card YAML".to_owned(),
                    details: serde_json::json!({ "source": error.to_string() }),
                })
        }
    }
}

/// Serializes a bare PromptSpec body to bytes.
pub fn serialize_spec_bytes(
    format: CardLoadFormat,
    spec: &PromptSpec,
) -> Result<Vec<u8>, WyrdError> {
    match format {
        CardLoadFormat::Json => {
            serde_json::to_vec_pretty(spec).map_err(|error| WyrdError::Internal {
                message: "failed to serialize prompt spec JSON".to_owned(),
                details: serde_json::json!({ "source": error.to_string() }),
            })
        }
        CardLoadFormat::Yaml => {
            serde_yaml::to_string(spec)
                .map(String::into_bytes)
                .map_err(|error| WyrdError::Internal {
                    message: "failed to serialize prompt spec YAML".to_owned(),
                    details: serde_json::json!({ "source": error.to_string() }),
                })
        }
    }
}

/// Parses a bare PromptSpec body from bytes.
pub fn parse_spec_bytes(format: CardLoadFormat, bytes: &[u8]) -> Result<PromptSpec, WyrdError> {
    match format {
        CardLoadFormat::Json => {
            serde_json::from_slice(bytes).map_err(|error| WyrdError::Validation {
                message: "failed to parse prompt spec JSON".to_owned(),
                details: serde_json::json!({ "source": error.to_string() }),
            })
        }
        CardLoadFormat::Yaml => {
            serde_yaml::from_slice(bytes).map_err(|error| WyrdError::Validation {
                message: "failed to parse prompt spec YAML".to_owned(),
                details: serde_json::json!({ "source": error.to_string() }),
            })
        }
    }
}
