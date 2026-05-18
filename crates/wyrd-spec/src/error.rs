//! Stable Wyrd error hierarchy and error-code helpers.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Proc-macro re-export for Wyrd-coded error enums.
pub mod derive {
    pub use wyrd_error_derive::WyrdError;
}

/// Top-level Wyrd error value passed across crate and wire boundaries.
#[derive(Debug, Error, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WyrdError {
    /// Request payload failed validation.
    #[error("[{}] {message}", code)]
    Validation {
        /// Stable error code.
        code: String,
        /// Suggested HTTP status.
        status: u16,
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Requested entity was not found.
    #[error("[{}] {message}", code)]
    NotFound {
        /// Stable error code.
        code: String,
        /// Suggested HTTP status.
        status: u16,
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Write was rejected due to a duplicate key or concurrent change.
    #[error("[{}] {message}", code)]
    Conflict {
        /// Stable error code.
        code: String,
        /// Suggested HTTP status.
        status: u16,
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Caller is authenticated but lacks permission.
    #[error("[{}] {message}", code)]
    PermissionDenied {
        /// Stable error code.
        code: String,
        /// Suggested HTTP status.
        status: u16,
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Unexpected internal failure.
    #[error("[{}] {message}", code)]
    Internal {
        /// Stable error code.
        code: String,
        /// Suggested HTTP status.
        status: u16,
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Upstream dependency failed.
    #[error("[{}] {message}", code)]
    UpstreamFailure {
        /// Stable error code.
        code: String,
        /// Suggested HTTP status.
        status: u16,
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Operation exceeded its deadline.
    #[error("[{}] {message}", code)]
    Timeout {
        /// Stable error code.
        code: String,
        /// Suggested HTTP status.
        status: u16,
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
        match self {
            Self::Validation { code, .. }
            | Self::NotFound { code, .. }
            | Self::Conflict { code, .. }
            | Self::PermissionDenied { code, .. }
            | Self::Internal { code, .. }
            | Self::UpstreamFailure { code, .. }
            | Self::Timeout { code, .. } => code,
        }
    }

    /// Suggested HTTP status.
    #[must_use]
    pub fn status(&self) -> u16 {
        match self {
            Self::Validation { status, .. }
            | Self::NotFound { status, .. }
            | Self::Conflict { status, .. }
            | Self::PermissionDenied { status, .. }
            | Self::Internal { status, .. }
            | Self::UpstreamFailure { status, .. }
            | Self::Timeout { status, .. } => *status,
        }
    }
}
