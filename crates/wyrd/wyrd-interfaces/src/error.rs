use serde_json::{Value, json};
use wyrd_spec::error::WyrdError;

/// Build a `DataCard` validation error.
pub fn validation(message: impl Into<String>) -> WyrdError {
    validation_with_details(message, Value::Null)
}

/// Build a `DataCard` validation error with structured details.
pub fn validation_with_details(message: impl Into<String>, details: Value) -> WyrdError {
    WyrdError::DataValidation {
        message: message.into(),
        details,
    }
}

/// Build a `DataCard` unknown-data-type error.
pub fn unknown_data_type(message: impl Into<String>) -> WyrdError {
    WyrdError::DataUnknownDataType {
        message: message.into(),
        details: Value::Null,
    }
}

/// Build a `DataCard` split-rule validation error.
pub fn invalid_split_rule(message: impl Into<String>, details: Value) -> WyrdError {
    WyrdError::DataInvalidSplitRule {
        message: message.into(),
        details,
    }
}

/// Build a `DataCard` target-column validation error.
pub fn target_column_unknown(column: impl Into<String>) -> WyrdError {
    let column = column.into();
    WyrdError::DataTargetColumnUnknown {
        message: format!("target column not present in schema: {column}"),
        details: json!({ "column": column }),
    }
}

/// Build a `DataCard` invalid-interface-option error.
pub fn invalid_interface_option<I, S>(
    field: impl Into<String>,
    got: impl Into<String>,
    accepted: I,
) -> WyrdError
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let field = field.into();
    let got = got.into();
    let accepted = accepted
        .into_iter()
        .map(Into::into)
        .collect::<Vec<String>>();
    WyrdError::DataInvalidInterfaceOption {
        message: format!(
            "invalid {field} value {got:?}; expected one of: {}",
            accepted.join(", ")
        ),
        details: json!({
            "field": field,
            "got": got,
            "accepted": accepted,
        }),
    }
}

/// Build an error for missing interface metadata.
pub fn interface_metadata_required(message: impl Into<String>) -> WyrdError {
    WyrdError::DataInterfaceMetadataRequired {
        message: message.into(),
        details: Value::Null,
    }
}

/// Build an error for a missing live data source.
pub fn missing_data_source(message: impl Into<String>) -> WyrdError {
    validation(message)
}

/// Build a `ModelCard` missing-signature error.
pub fn missing_signature(message: impl Into<String>) -> WyrdError {
    WyrdError::ModelMissingSignature {
        message: message.into(),
        details: Value::Null,
    }
}

/// Build a `ModelCard` validation error.
pub fn model_validation(message: impl Into<String>) -> WyrdError {
    model_validation_with_details(message, Value::Null)
}

/// Build a `ModelCard` validation error with structured details.
pub fn model_validation_with_details(message: impl Into<String>, details: Value) -> WyrdError {
    WyrdError::ModelValidation {
        message: message.into(),
        details,
    }
}

/// Build a `ModelCard` unknown-model-type error.
pub fn unknown_model_type(module: impl Into<String>, type_name: impl Into<String>) -> WyrdError {
    let module = module.into();
    let type_name = type_name.into();
    WyrdError::ModelUnknownModelType {
        message: format!("unsupported model object type: {module}.{type_name}"),
        details: json!({
            "module": module,
            "type_name": type_name,
        }),
    }
}

/// Build a `ModelCard` serializer-unavailable error.
pub fn serializer_unavailable(extra: &str) -> WyrdError {
    WyrdError::ModelSerializerUnavailable {
        message: format!("required serializer unavailable: {extra}"),
        details: json!({ "extra": extra }),
    }
}

/// Build an internal boundary failure that records its originating source.
///
/// Every non-catalog failure inside a card interface — a Python call, a
/// downcast, a JSON conversion, or local IO — lands here so the shared adapter
/// still raises a catalog-backed exception rather than a bare Python error.
fn internal_source(message: &str, source: &impl std::fmt::Display) -> WyrdError {
    WyrdError::Internal {
        message: message.to_owned(),
        details: json!({ "source": source.to_string() }),
    }
}

/// Record a JSON conversion failure inside a card interface.
pub fn json_error(source: &impl std::fmt::Display) -> WyrdError {
    internal_source("JSON conversion failed", source)
}

/// Record a local filesystem failure inside a card interface.
pub fn io_error(source: &impl std::fmt::Display) -> WyrdError {
    internal_source("local IO failed", source)
}

/// Register interface exception types.
///
/// # Errors
/// Returns a Python error when the module rejects the exception registration.
#[cfg(feature = "python")]
pub fn register_exceptions(module: &pyo3::Bound<'_, pyo3::types::PyModule>) -> pyo3::PyResult<()> {
    wyrd_utils::py::register_wyrd_error_exception(module)
}

/// Project a spec-owned card validation failure onto the public catalog.
///
/// Card specs expose their own validation error types, so `map_err` uses this
/// helper to reach the catalog before the boundary adapter converts it.
pub fn card_error(error: impl Into<WyrdError>) -> WyrdError {
    error.into()
}
