//! Python-boundary errors for Wyrd card interfaces.

use serde_json::{Value, json};
use thiserror::Error;
use wyrd_spec::card::data::validate::DataCardError;
use wyrd_spec::card::model::validate::ModelCardError;
use wyrd_spec::error::WyrdError;

#[cfg(feature = "python")]
use pyo3::Python;
#[cfg(feature = "python")]
use pyo3::types::PyAnyMethods;

/// Result alias used by Python-boundary card interface methods.
pub type CardPyResult<T> = Result<T, WyrdPyError>;

/// String-backed Python-boundary error for card interface code.
///
/// This type never stores Python-owned errors. `PyO3` errors are converted into
/// owned strings at the boundary and then converted once into Python exceptions.
#[derive(Debug, Error)]
pub enum WyrdPyError {
    /// Public Wyrd error with a stable code and rich metadata.
    #[error(transparent)]
    Spec(#[from] WyrdError),
    /// Public Wyrd error retaining a safe textual Python cause.
    #[error("{error}")]
    SpecWithCause {
        /// Stable public error preserved across the boundary.
        error: WyrdError,
        /// Redacted source text attached as Python `__cause__`.
        cause: String,
    },
    /// Python boundary failure converted to an owned string.
    #[error("Python error: {0}")]
    Python(String),
    /// Python boundary failure retaining a safe textual cause for exception chaining.
    #[error("Python error: {message}")]
    PythonWithCause {
        /// Stable boundary message shown to the caller.
        message: String,
        /// Redacted source text attached as Python `__cause__`.
        cause: String,
    },
    /// Python object downcast failure converted to an owned string.
    #[error("Failed to downcast Python object: {0}")]
    Downcast(String),
    /// JSON serialization or parsing failure.
    #[error("JSON error: {0}")]
    Json(String),
    /// Local filesystem IO failure.
    #[error("IO error: {0}")]
    Io(String),
    /// Internal interface failure.
    #[error("Internal error: {0}")]
    Internal(String),
}

impl WyrdPyError {
    /// Build a `DataCard` validation error.
    pub fn validation(message: impl Into<String>) -> Self {
        Self::validation_with_details(message, Value::Null)
    }

    /// Build a `DataCard` validation error with structured details.
    pub fn validation_with_details(message: impl Into<String>, details: Value) -> Self {
        WyrdError::DataValidation {
            message: message.into(),
            details,
        }
        .into()
    }

    /// Build a `DataCard` unknown-data-type error.
    pub fn unknown_data_type(message: impl Into<String>) -> Self {
        WyrdError::DataUnknownDataType {
            message: message.into(),
            details: Value::Null,
        }
        .into()
    }

    /// Build a `DataCard` split-rule validation error.
    pub fn invalid_split_rule(message: impl Into<String>, details: Value) -> Self {
        WyrdError::DataInvalidSplitRule {
            message: message.into(),
            details,
        }
        .into()
    }

    /// Build a `DataCard` target-column validation error.
    pub fn target_column_unknown(column: impl Into<String>) -> Self {
        let column = column.into();
        WyrdError::DataTargetColumnUnknown {
            message: format!("target column not present in schema: {column}"),
            details: json!({ "column": column }),
        }
        .into()
    }

    /// Build a `DataCard` invalid-interface-option error.
    pub fn invalid_interface_option<I, S>(
        field: impl Into<String>,
        got: impl Into<String>,
        accepted: I,
    ) -> Self
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
        .into()
    }

    /// Build an error for missing interface metadata.
    pub fn interface_metadata_required(message: impl Into<String>) -> Self {
        WyrdError::DataInterfaceMetadataRequired {
            message: message.into(),
            details: Value::Null,
        }
        .into()
    }

    /// Build an error for a missing live data source.
    pub fn missing_data_source(message: impl Into<String>) -> Self {
        Self::validation(message)
    }

    /// Build a `ModelCard` missing-signature error.
    pub fn missing_signature(message: impl Into<String>) -> Self {
        WyrdError::ModelMissingSignature {
            message: message.into(),
            details: Value::Null,
        }
        .into()
    }

    /// Build a `ModelCard` validation error.
    pub fn model_validation(message: impl Into<String>) -> Self {
        Self::model_validation_with_details(message, Value::Null)
    }

    /// Build a `ModelCard` validation error with structured details.
    pub fn model_validation_with_details(message: impl Into<String>, details: Value) -> Self {
        WyrdError::ModelValidation {
            message: message.into(),
            details,
        }
        .into()
    }

    /// Build a `ModelCard` unknown-model-type error.
    pub fn unknown_model_type(module: impl Into<String>, type_name: impl Into<String>) -> Self {
        let module = module.into();
        let type_name = type_name.into();
        WyrdError::ModelUnknownModelType {
            message: format!("unsupported model object type: {module}.{type_name}"),
            details: json!({
                "module": module,
                "type_name": type_name,
            }),
        }
        .into()
    }

    /// Build a `ModelCard` serializer-unavailable error.
    pub fn serializer_unavailable(extra: &str) -> Self {
        WyrdError::ModelSerializerUnavailable {
            message: format!("required serializer unavailable: {extra}"),
            details: json!({ "extra": extra }),
        }
        .into()
    }

    /// Build an internal interface error.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }

    /// Build a boundary error that exposes a redacted Python cause through `__cause__`.
    pub fn python_with_cause(message: impl Into<String>, cause: impl Into<String>) -> Self {
        Self::PythonWithCause {
            message: message.into(),
            cause: cause.into(),
        }
    }

    /// Attach a redacted Python cause while preserving a stable Wyrd error.
    pub fn spec_with_cause(error: WyrdError, cause: impl Into<String>) -> Self {
        Self::SpecWithCause {
            error,
            cause: cause.into(),
        }
    }

    #[cfg(feature = "python")]
    fn into_wyrd_error(self) -> WyrdError {
        match self {
            Self::Spec(error) | Self::SpecWithCause { error, .. } => error,
            Self::Python(source) => internal_from_source("Python boundary failed", &source),
            Self::PythonWithCause { message, cause } => internal_from_source(&message, &cause),
            Self::Downcast(source) => {
                internal_from_source("Python object downcast failed", &source)
            }
            Self::Json(source) => internal_from_source("JSON conversion failed", &source),
            Self::Io(source) => internal_from_source("local IO failed", &source),
            Self::Internal(source) => WyrdError::Internal {
                message: "card interface failed internally".to_string(),
                details: json!({ "source": source }),
            },
        }
    }
}

impl From<DataCardError> for WyrdPyError {
    fn from(error: DataCardError) -> Self {
        WyrdError::from(error).into()
    }
}

impl From<ModelCardError> for WyrdPyError {
    fn from(error: ModelCardError) -> Self {
        WyrdError::from(error).into()
    }
}

impl From<serde_json::Error> for WyrdPyError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error.to_string())
    }
}

impl From<std::io::Error> for WyrdPyError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

#[cfg(feature = "python")]
impl<'a, 'py> From<pyo3::pyclass::PyClassGuardError<'a, 'py>> for WyrdPyError {
    fn from(error: pyo3::pyclass::PyClassGuardError<'a, 'py>) -> Self {
        Self::Python(error.to_string())
    }
}

#[cfg(feature = "python")]
impl<'a, 'py> From<pyo3::CastError<'a, 'py>> for WyrdPyError {
    fn from(error: pyo3::CastError<'a, 'py>) -> Self {
        Self::Downcast(error.to_string())
    }
}

#[cfg(feature = "python")]
impl From<pyo3::PyErr> for WyrdPyError {
    fn from(error: pyo3::PyErr) -> Self {
        Python::attach(|py| wyrd_py_error_from_py_err(py, &error))
    }
}

#[cfg(feature = "python")]
impl From<WyrdPyError> for pyo3::PyErr {
    fn from(error: WyrdPyError) -> Self {
        let cause = match &error {
            WyrdPyError::SpecWithCause { cause, .. }
            | WyrdPyError::PythonWithCause { cause, .. } => Some(cause.clone()),
            _ => None,
        };
        Python::attach(|py| {
            let converted = wyrd_utils::py::wyrd_error_to_py_err(error.into_wyrd_error());
            if let Some(cause) = cause {
                converted.set_cause(py, Some(pyo3::exceptions::PyRuntimeError::new_err(cause)));
            }
            converted
        })
    }
}

#[cfg(feature = "python")]
impl From<WyrdPyError> for wyrd_utils::py::WyrdPyError {
    /// Re-enter the shared boundary type through the stable catalog error.
    fn from(error: WyrdPyError) -> Self {
        Self::from(error.into_wyrd_error())
    }
}

#[cfg(feature = "python")]
impl From<wyrd_utils::py::WyrdPyError> for WyrdPyError {
    /// Adopt a shared-boundary failure without losing its catalog identity.
    fn from(error: wyrd_utils::py::WyrdPyError) -> Self {
        Self::Spec(error.into_wyrd_error())
    }
}

/// Register interface exception types.
#[cfg(feature = "python")]
pub fn register_exceptions(module: &pyo3::Bound<'_, pyo3::types::PyModule>) -> pyo3::PyResult<()> {
    wyrd_utils::py::register_wyrd_error_exception(module)
}

#[cfg(feature = "python")]
fn internal_from_source(message: &str, source: &str) -> WyrdError {
    WyrdError::Internal {
        message: message.to_string(),
        details: json!({ "source": source }),
    }
}

#[cfg(feature = "python")]
fn wyrd_py_error_from_py_err(py: Python<'_>, error: &pyo3::PyErr) -> WyrdPyError {
    if !error.is_instance_of::<wyrd_utils::py::WyrdError>(py) {
        return WyrdPyError::Python(error.to_string());
    }

    let value = error.value(py);
    let code = py_error_attr_string(value, "code");
    let message = py_error_attr_string(value, "message").unwrap_or_else(|| error.to_string());
    let details = value
        .getattr("details")
        .ok()
        .and_then(|details| wyrd_utils::py::pyobject_to_json(&details).ok())
        .unwrap_or(Value::Null);

    match code.as_deref() {
        Some("WYRD_DATA_400_VALIDATION") => WyrdError::DataValidation { message, details }.into(),
        Some("WYRD_MODEL_400_VALIDATION") => WyrdError::ModelValidation { message, details }.into(),
        Some("WYRD_MODEL_400_UNKNOWN_MODEL_TYPE") => {
            WyrdError::ModelUnknownModelType { message, details }.into()
        }
        Some("WYRD_MODEL_400_MISSING_SIGNATURE") => {
            WyrdError::ModelMissingSignature { message, details }.into()
        }
        Some("WYRD_MODEL_400_DTYPE_NORMALIZE_FAILED") => {
            WyrdError::ModelDtypeNormalizeFailed { message, details }.into()
        }
        Some("WYRD_MODEL_400_SHAPE_INVALID") => {
            WyrdError::ModelShapeInvalid { message, details }.into()
        }
        Some("WYRD_MODEL_400_HF_REVISION_INVALID") => {
            WyrdError::ModelHfRevisionInvalid { message, details }.into()
        }
        Some("WYRD_MODEL_400_HF_TASK_MISSING") => {
            WyrdError::ModelHfTaskMissing { message, details }.into()
        }
        Some("WYRD_MODEL_400_CUSTOM_LOADER_INVALID") => {
            WyrdError::ModelCustomLoaderInvalid { message, details }.into()
        }
        Some("WYRD_MODEL_501_SERIALIZER_UNAVAILABLE") => {
            WyrdError::ModelSerializerUnavailable { message, details }.into()
        }
        Some("WYRD_PROMPT_422_INVALID_OUTPUT_SCHEMA") => {
            WyrdError::PromptInvalidOutputSchema { message, details }.into()
        }
        Some("WYRD_PROMPT_422_PYDANTIC_REQUIRED") => {
            WyrdError::PromptPydanticRequired { message, details }.into()
        }
        _ => WyrdPyError::Python(error.to_string()),
    }
}

#[cfg(feature = "python")]
fn py_error_attr_string(value: &pyo3::Bound<'_, pyo3::types::PyAny>, name: &str) -> Option<String> {
    value.getattr(name).ok()?.extract::<String>().ok()
}

// The free-function catalog constructors below are the REQ-024 projection
// consumed by every interface module in this crate. They return the shared
// `WyrdError` catalog directly; the `WyrdPyError` enum above remains the
// current Card-holder boundary type until the SDK convergence retires it.
mod catalog {
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

    /// Project a spec-owned card validation failure onto the public catalog.
    ///
    /// Card specs expose their own validation error types, so `map_err` uses this
    /// helper to reach the catalog before the boundary adapter converts it.
    pub fn card_error(error: impl Into<WyrdError>) -> WyrdError {
        error.into()
    }

}

pub use catalog::*;
