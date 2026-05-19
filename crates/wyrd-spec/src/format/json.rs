//! JSON serialization helpers.

use crate::envelope::Card;
use crate::error::WyrdError;

/// Serialize a Card to pretty JSON.
///
/// # Errors
/// Returns a Wyrd error when serialization fails.
pub fn to_string_pretty(card: &Card) -> Result<String, WyrdError> {
    serde_json::to_string_pretty(card).map_err(|source| WyrdError::Internal {
        message: "failed to serialize card to JSON".to_string(),
        details: serde_json::json!({ "source": source.to_string() }),
    })
}

/// Parse a Card from JSON.
///
/// # Errors
/// Returns a Wyrd error when parsing fails.
pub fn from_str(input: &str) -> Result<Card, WyrdError> {
    serde_json::from_str(input).map_err(|source| WyrdError::Validation {
        message: "failed to parse card JSON".to_string(),
        details: serde_json::json!({ "source": source.to_string() }),
    })
}
