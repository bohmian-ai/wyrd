//! YAML serialization helpers.

use crate::envelope::Card;
use crate::error::WyrdError;

/// Serialize a Card to YAML.
///
/// # Errors
/// Returns a Wyrd error when serialization fails.
pub fn to_string(card: &Card) -> Result<String, WyrdError> {
    serde_yaml::to_string(card).map_err(|source| WyrdError::Internal {
        code: "WYRD_SPEC_500_YAML_SERIALIZE".to_string(),
        status: 500,
        message: "failed to serialize card to YAML".to_string(),
        details: serde_json::json!({ "source": source.to_string() }),
    })
}

/// Parse a Card from YAML.
///
/// # Errors
/// Returns a Wyrd error when parsing fails.
pub fn from_str(input: &str) -> Result<Card, WyrdError> {
    serde_yaml::from_str(input).map_err(|source| WyrdError::Validation {
        code: "WYRD_SPEC_400_YAML_PARSE".to_string(),
        status: 400,
        message: "failed to parse card YAML".to_string(),
        details: serde_json::json!({ "source": source.to_string() }),
    })
}
