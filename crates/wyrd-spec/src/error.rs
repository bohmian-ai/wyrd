//! Stable Wyrd error hierarchy and error-code helpers.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::error::derive::WyrdError as WyrdErrorMeta;

/// Proc-macro re-export for Wyrd-coded error enums.
pub mod derive {
    pub use wyrd_error_derive::WyrdError;
}

/// Top-level Wyrd error value passed across crate and wire boundaries.
#[derive(Debug, Error, Serialize, Deserialize, schemars::JsonSchema, WyrdErrorMeta)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WyrdError {
    /// Request payload failed validation.
    #[error("[WYRD_SPEC_400_VALIDATION] {message}")]
    #[wyrd_error(
        code = "WYRD_SPEC_400_VALIDATION",
        status = 400,
        title = "Validation failed",
        remediation = "Check the submitted Wyrd request fields against the published schema and retry."
    )]
    Validation {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Requested entity was not found.
    #[error("[WYRD_SPEC_404_NOT_FOUND] {message}")]
    #[wyrd_error(
        code = "WYRD_SPEC_404_NOT_FOUND",
        status = 404,
        title = "Resource not found",
        remediation = "Check that the referenced Wyrd resource exists."
    )]
    NotFound {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Write was rejected due to a duplicate key or concurrent change.
    #[error("[WYRD_SPEC_409_CONFLICT] {message}")]
    #[wyrd_error(
        code = "WYRD_SPEC_409_CONFLICT",
        status = 409,
        title = "Conflict",
        remediation = "Refresh the resource and retry with the current version or idempotency key."
    )]
    Conflict {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Caller is authenticated but lacks permission.
    #[error("[WYRD_SPEC_403_PERMISSION_DENIED] {message}")]
    #[wyrd_error(
        code = "WYRD_SPEC_403_PERMISSION_DENIED",
        status = 403,
        title = "Permission denied",
        remediation = "Check the caller scopes, actor identity, and policy decision."
    )]
    PermissionDenied {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Unexpected internal failure.
    #[error("[WYRD_SPEC_500_INTERNAL] {message}")]
    #[wyrd_error(
        code = "WYRD_SPEC_500_INTERNAL",
        status = 500,
        title = "Internal error",
        remediation = "Retry later or inspect server logs using the request ID."
    )]
    Internal {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Upstream dependency failed.
    #[error("[WYRD_SPEC_502_UPSTREAM_FAILURE] {message}")]
    #[wyrd_error(
        code = "WYRD_SPEC_502_UPSTREAM_FAILURE",
        status = 502,
        title = "Upstream dependency failed",
        remediation = "Check the upstream dependency health and retry policy."
    )]
    UpstreamFailure {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Operation exceeded its deadline.
    #[error("[WYRD_SPEC_504_TIMEOUT] {message}")]
    #[wyrd_error(
        code = "WYRD_SPEC_504_TIMEOUT",
        status = 504,
        title = "Timeout",
        remediation = "Retry with a longer timeout or reduce the request scope."
    )]
    Timeout {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// DataCard validation failed.
    #[error("[WYRD_DATA_400_VALIDATION] {message}")]
    #[wyrd_error(
        code = "WYRD_DATA_400_VALIDATION",
        status = 400,
        title = "DataCard validation failed",
        remediation = "Fix the DataCard schema, artifact, stats, or interface metadata."
    )]
    DataValidation {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Python data input could not be classified as a supported DataCard source.
    #[error("[WYRD_DATA_400_UNKNOWN_DATA_TYPE] {message}")]
    #[wyrd_error(
        code = "WYRD_DATA_400_UNKNOWN_DATA_TYPE",
        status = 400,
        title = "Unknown DataCard data type",
        remediation = "Pass a supported data object, path, or explicit DataInterface."
    )]
    DataUnknownDataType {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// A DataCard split declaration is invalid.
    #[error("[WYRD_DATA_400_INVALID_SPLIT_RULE] {message}")]
    #[wyrd_error(
        code = "WYRD_DATA_400_INVALID_SPLIT_RULE",
        status = 400,
        title = "Invalid DataCard split rule",
        remediation = "Use a supported split operator and reference schema columns that exist."
    )]
    DataInvalidSplitRule {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// A DataCard target column is absent from the declared schema.
    #[error("[WYRD_DATA_400_TARGET_COLUMN_UNKNOWN] {message}")]
    #[wyrd_error(
        code = "WYRD_DATA_400_TARGET_COLUMN_UNKNOWN",
        status = 400,
        title = "Unknown DataCard target column",
        remediation = "Declare target columns that exist in the DataCard schema."
    )]
    DataTargetColumnUnknown {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// A Python DataCard interface option was invalid.
    #[error("[WYRD_DATA_400_INVALID_INTERFACE_OPTION] {message}")]
    #[wyrd_error(
        code = "WYRD_DATA_400_INVALID_INTERFACE_OPTION",
        status = 400,
        title = "Invalid DataCard interface option",
        remediation = "Use one of the supported interface option values."
    )]
    DataInvalidInterfaceOption {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Interface metadata was required but could not be inferred.
    #[error("[WYRD_DATA_400_INTERFACE_METADATA_REQUIRED] {message}")]
    #[wyrd_error(
        code = "WYRD_DATA_400_INTERFACE_METADATA_REQUIRED",
        status = 400,
        title = "DataCard interface metadata required",
        remediation = "Pass an explicit DataInterface with enough metadata to describe the data source."
    )]
    DataInterfaceMetadataRequired {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
}

impl WyrdError {
    /// RFC 9457 JSON problem payload.
    #[must_use]
    pub fn as_problem_json(&self) -> serde_json::Value {
        let (message, details) = self.message_details();
        serde_json::json!({
            "type": format!("https://wyrd.dev/problems/{}", self.code()),
            "title": self.title(),
            "status": self.status(),
            "detail": message,
            "code": self.code(),
            "details": details,
            "remediation": self.remediation(),
        })
    }

    fn message_details(&self) -> (&str, &serde_json::Value) {
        match self {
            Self::Validation { message, details }
            | Self::NotFound { message, details }
            | Self::Conflict { message, details }
            | Self::PermissionDenied { message, details }
            | Self::Internal { message, details }
            | Self::UpstreamFailure { message, details }
            | Self::Timeout { message, details }
            | Self::DataValidation { message, details }
            | Self::DataUnknownDataType { message, details }
            | Self::DataInvalidSplitRule { message, details }
            | Self::DataTargetColumnUnknown { message, details }
            | Self::DataInvalidInterfaceOption { message, details }
            | Self::DataInterfaceMetadataRequired { message, details } => (message, details),
        }
    }
}
