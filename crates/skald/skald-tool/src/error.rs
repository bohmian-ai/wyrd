//! Error types for Skald tool declarations.

/// Result alias for fallible skald-tool operations.
pub type SkaldToolResult<T> = Result<T, SkaldToolError>;

/// Tool declaration failures with stable machine-readable codes.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SkaldToolError {
    /// The tool name, description, or JSON schema is invalid.
    #[error("invalid tool schema for {tool:?}: {message}")]
    InvalidSchema {
        /// Tool name when the failure can be attributed to one declaration.
        tool: Option<String>,
        /// Human-readable validation detail.
        message: String,
    },
}

impl SkaldToolError {
    /// Creates an invalid-schema error for a specific tool declaration.
    pub fn invalid_schema(tool: Option<String>, message: impl Into<String>) -> Self {
        Self::InvalidSchema {
            tool,
            message: message.into(),
        }
    }

    /// Returns the stable Wyrd error code for this tool failure.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidSchema { .. } => "SKALD_TOOL_400_INVALID_SCHEMA",
        }
    }
}
