//! Shared identity parsers for card holders.

use serde_json::json;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, CardUid, SpaceName};
use wyrd_spec::version::VersionBlock;

pub(crate) fn card_name(field: &str, value: &str) -> Result<CardName, WyrdError> {
    CardName::new(value).map_err(|error| invalid_identity(field, value, error))
}

pub(crate) fn version_block(value: &str) -> Result<VersionBlock, WyrdError> {
    VersionBlock::parse(value).map_err(|error| {
        validation_error(
            format!("invalid card version: {value}"),
            json!({
                "field": "version",
                "value": value,
                "source": error.to_string(),
            }),
        )
    })
}

pub(crate) fn space_name(value: &str) -> Result<SpaceName, WyrdError> {
    if value.is_empty() {
        return Err(validation_error(
            "card space is required and cannot be empty",
            json!({ "field": "space" }),
        ));
    }
    SpaceName::new(value).map_err(|error| invalid_identity("space", value, error))
}

pub(crate) fn optional_card_uid(value: &str) -> Result<Option<CardUid>, WyrdError> {
    if value.is_empty() {
        Ok(None)
    } else {
        CardUid::new(value)
            .map(Some)
            .map_err(|error| invalid_identity("uid", value, error))
    }
}

pub(crate) fn invalid_identity(
    field: &str,
    value: &str,
    error: impl std::fmt::Display,
) -> WyrdError {
    validation_error(
        format!("invalid card {field}: {value}"),
        json!({
            "field": field,
            "value": value,
            "source": error.to_string(),
        }),
    )
}

pub(crate) fn validation_error(
    message: impl Into<String>,
    details: serde_json::Value,
) -> WyrdError {
    WyrdError::Validation {
        message: message.into(),
        details,
    }
}
