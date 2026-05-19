//! Stable Wyrd error hierarchy and error-code helpers.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Proc-macro re-export for Wyrd-coded error enums.
pub mod derive {
    pub use wyrd_error_derive::WyrdError;
}

/// Stable Wyrd error code registry for foundation errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    /// Request payload failed validation.
    WyrdSpec400Validation,
    /// Requested entity was not found.
    WyrdSpec404NotFound,
    /// Write was rejected because of a conflict.
    WyrdSpec409Conflict,
    /// Caller lacks permission.
    WyrdSpec403PermissionDenied,
    /// Unexpected internal failure.
    WyrdSpec500Internal,
    /// Upstream dependency failed.
    WyrdSpec502UpstreamFailure,
    /// Operation timed out.
    WyrdSpec504Timeout,
}

impl ErrorCode {
    /// Stable string code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WyrdSpec400Validation => "WYRD_SPEC_400_VALIDATION",
            Self::WyrdSpec404NotFound => "WYRD_SPEC_404_NOT_FOUND",
            Self::WyrdSpec409Conflict => "WYRD_SPEC_409_CONFLICT",
            Self::WyrdSpec403PermissionDenied => "WYRD_SPEC_403_PERMISSION_DENIED",
            Self::WyrdSpec500Internal => "WYRD_SPEC_500_INTERNAL",
            Self::WyrdSpec502UpstreamFailure => "WYRD_SPEC_502_UPSTREAM_FAILURE",
            Self::WyrdSpec504Timeout => "WYRD_SPEC_504_TIMEOUT",
        }
    }

    /// Suggested HTTP status.
    #[must_use]
    pub const fn status(self) -> u16 {
        match self {
            Self::WyrdSpec400Validation => 400,
            Self::WyrdSpec404NotFound => 404,
            Self::WyrdSpec409Conflict => 409,
            Self::WyrdSpec403PermissionDenied => 403,
            Self::WyrdSpec500Internal => 500,
            Self::WyrdSpec502UpstreamFailure => 502,
            Self::WyrdSpec504Timeout => 504,
        }
    }

    /// Operator-facing remediation hint.
    #[must_use]
    pub const fn remediation(self) -> &'static str {
        match self {
            Self::WyrdSpec400Validation => {
                "Check the submitted Card, Spec, Run, or helper value against the published schema."
            }
            Self::WyrdSpec404NotFound => "Check that the referenced Wyrd resource exists.",
            Self::WyrdSpec409Conflict => {
                "Refresh the resource and retry with the current version or idempotency key."
            }
            Self::WyrdSpec403PermissionDenied => {
                "Check the caller scopes, actor identity, and policy decision."
            }
            Self::WyrdSpec500Internal => "Retry later or inspect server logs using the request ID.",
            Self::WyrdSpec502UpstreamFailure => {
                "Check the upstream dependency health and retry policy."
            }
            Self::WyrdSpec504Timeout => "Retry with a longer timeout or reduce the request scope.",
        }
    }
}

/// Top-level Wyrd error value passed across crate and wire boundaries.
#[derive(Debug, Error, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WyrdError {
    /// Request payload failed validation.
    #[error("[WYRD_SPEC_400_VALIDATION] {message}")]
    Validation {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Requested entity was not found.
    #[error("[WYRD_SPEC_404_NOT_FOUND] {message}")]
    NotFound {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Write was rejected due to a duplicate key or concurrent change.
    #[error("[WYRD_SPEC_409_CONFLICT] {message}")]
    Conflict {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Caller is authenticated but lacks permission.
    #[error("[WYRD_SPEC_403_PERMISSION_DENIED] {message}")]
    PermissionDenied {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Unexpected internal failure.
    #[error("[WYRD_SPEC_500_INTERNAL] {message}")]
    Internal {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Upstream dependency failed.
    #[error("[WYRD_SPEC_502_UPSTREAM_FAILURE] {message}")]
    UpstreamFailure {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Operation exceeded its deadline.
    #[error("[WYRD_SPEC_504_TIMEOUT] {message}")]
    Timeout {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
}

impl WyrdError {
    /// Stable error code.
    #[must_use]
    pub fn code(&self) -> &str {
        self.error_code().as_str()
    }

    /// Suggested HTTP status.
    #[must_use]
    pub fn status(&self) -> u16 {
        self.error_code().status()
    }

    /// Stable error-code enum.
    #[must_use]
    pub const fn error_code(&self) -> ErrorCode {
        match self {
            Self::Validation { .. } => ErrorCode::WyrdSpec400Validation,
            Self::NotFound { .. } => ErrorCode::WyrdSpec404NotFound,
            Self::Conflict { .. } => ErrorCode::WyrdSpec409Conflict,
            Self::PermissionDenied { .. } => ErrorCode::WyrdSpec403PermissionDenied,
            Self::Internal { .. } => ErrorCode::WyrdSpec500Internal,
            Self::UpstreamFailure { .. } => ErrorCode::WyrdSpec502UpstreamFailure,
            Self::Timeout { .. } => ErrorCode::WyrdSpec504Timeout,
        }
    }

    /// Operator-facing remediation hint.
    #[must_use]
    pub fn remediation(&self) -> &'static str {
        self.error_code().remediation()
    }

    /// RFC 7807-style JSON problem payload.
    #[must_use]
    pub fn as_problem_json(&self) -> serde_json::Value {
        let (title, message, details) = match self {
            Self::Validation { message, details } => ("Validation failed", message, details),
            Self::NotFound { message, details } => ("Resource not found", message, details),
            Self::Conflict { message, details } => ("Conflict", message, details),
            Self::PermissionDenied { message, details } => ("Permission denied", message, details),
            Self::Internal { message, details } => ("Internal error", message, details),
            Self::UpstreamFailure { message, details } => {
                ("Upstream dependency failed", message, details)
            }
            Self::Timeout { message, details } => ("Timeout", message, details),
        };
        serde_json::json!({
            "type": format!("https://wyrd.dev/problems/{}", self.code()),
            "title": title,
            "status": self.status(),
            "code": self.code(),
            "message": message,
            "details": details,
            "remediation": self.remediation(),
        })
    }
}
