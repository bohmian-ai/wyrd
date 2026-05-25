//! Python-boundary errors for Wyrd card interfaces.

use serde_json::{Value, json};
use thiserror::Error;
use wyrd_spec::card::data::validate::DataCardError;
use wyrd_spec::card::model::validate::ModelCardError;
use wyrd_spec::error::WyrdError;

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
    /// Python boundary failure converted to an owned string.
    #[error("Python error: {0}")]
    Python(String),
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

    /// Build an internal interface error.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }

    #[cfg(feature = "python")]
    fn into_wyrd_error(self) -> WyrdError {
        match self {
            Self::Spec(error) => error,
            Self::Python(source) => {
                data_validation_from_source("DataCard Python boundary failed", &source)
            }
            Self::Downcast(source) => {
                data_validation_from_source("DataCard Python object downcast failed", &source)
            }
            Self::Json(source) => {
                data_validation_from_source("DataCard JSON conversion failed", &source)
            }
            Self::Io(source) => data_validation_from_source("DataCard local IO failed", &source),
            Self::Internal(source) => WyrdError::Internal {
                message: "DataCard interface failed internally".to_string(),
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
        Self::Python(error.to_string())
    }
}

#[cfg(feature = "python")]
impl From<WyrdPyError> for pyo3::PyErr {
    fn from(error: WyrdPyError) -> Self {
        wyrd_utils::py::wyrd_error_to_py_err(error.into_wyrd_error())
    }
}

/// Register interface exception types.
#[cfg(feature = "python")]
pub fn register_exceptions(module: &pyo3::Bound<'_, pyo3::types::PyModule>) -> pyo3::PyResult<()> {
    wyrd_utils::py::register_wyrd_error_exception(module)
}

#[cfg(feature = "python")]
fn data_validation_from_source(message: &str, source: &str) -> WyrdError {
    WyrdError::DataValidation {
        message: message.to_string(),
        details: json!({ "source": source }),
    }
}
