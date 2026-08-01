/// Owned metadata for a structured invocation failure.
#[derive(Debug, thiserror::Error)]
#[error("{detail}")]
pub struct StructuredInvocationError {
    /// Stable machine-readable failure code.
    pub code: String,
    /// HTTP-equivalent status for the failure.
    pub status: u16,
    /// Stable problem title.
    pub title: String,
    /// Human-readable failure detail.
    pub detail: String,
    /// Operator-facing remediation guidance.
    pub remediation: String,
    /// Scrubbed structured context safe to expose to an agent.
    pub safe_details: Option<serde_json::Value>,
}

/// Failures raised by executable Skald tools.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    /// Provider-emitted JSON did not deserialize into the tool's input type.
    #[error("tool invocation input did not match the declared input schema: {0}")]
    InvalidInput(String),

    /// The underlying tool implementation failed.
    #[error("tool invocation failed during execution: {detail}")]
    Invocation {
        /// Human-readable failure detail.
        detail: String,
        /// Optional source error from the tool implementation.
        #[source]
        cause: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// An invocation failed with stable structured metadata from its owner.
    #[error("tool invocation failed with structured metadata: {0}")]
    StructuredInvocation(#[source] Box<StructuredInvocationError>),

    /// Tool output failed JSON serialization.
    #[error("tool output could not be serialized to JSON: {0}")]
    OutputSerialization(String),

    /// Tool name is already registered.
    #[error("tool name `{name}` is already registered")]
    NameTaken {
        /// Name that collided.
        name: String,
    },

    /// Tool lookup failed for an unknown name.
    #[error("tool name `{name}` is not registered (available: {available:?})")]
    NotRegistered {
        /// Name requested by caller.
        name: String,
        /// Registered names, sorted lexicographically.
        available: Vec<String>,
    },
}

impl ToolError {
    /// Stable machine-readable code for this tool invocation failure.
    pub fn code(&self) -> String {
        match self {
            Self::NameTaken { .. } => "SKALD_TOOL_409_NAME_TAKEN".to_owned(),
            Self::NotRegistered { .. } => "SKALD_TOOL_404_NOT_REGISTERED".to_owned(),
            Self::InvalidInput(_) => "SKALD_TOOL_422_INPUT".to_owned(),
            Self::Invocation { .. } => "SKALD_TOOL_500_CALL".to_owned(),
            Self::StructuredInvocation(error) => error.code.clone(),
            Self::OutputSerialization(_) => "SKALD_TOOL_500_OUTPUT".to_owned(),
        }
    }

    /// Suggested HTTP status for this failure.
    pub fn status(&self) -> u16 {
        match self {
            Self::NameTaken { .. } => 409,
            Self::NotRegistered { .. } => 404,
            Self::InvalidInput(_) => 422,
            Self::StructuredInvocation(error) => error.status,
            Self::Invocation { .. } | Self::OutputSerialization(_) => 500,
        }
    }

    /// Stable problem-title text.
    pub fn title(&self) -> String {
        match self {
            Self::NameTaken { .. } => "Tool name already registered".to_owned(),
            Self::NotRegistered { .. } => "Tool name not registered".to_owned(),
            Self::InvalidInput(_) => {
                "Tool invocation input did not match the declared input schema".to_owned()
            }
            Self::Invocation { .. } => "Tool invocation failed during execution".to_owned(),
            Self::StructuredInvocation(error) => error.title.clone(),
            Self::OutputSerialization(_) => {
                "Tool output could not be serialized to JSON".to_owned()
            }
        }
    }

    /// Operator-facing remediation hint.
    pub fn remediation(&self) -> String {
        match self {
            Self::NameTaken { .. } => {
                "Pick a unique tool name or clear the registry before registering the replacement.".to_owned()
            }
            Self::NotRegistered { .. } => {
                "Register the tool (Tool::function + registry.register) before loading the agent, or call default_registry() before resolving agent tools.".to_owned()
            }
            Self::InvalidInput(_) => {
                "Adjust the tool call arguments to match the schema returned by tool.input_schema().".to_owned()
            }
            Self::Invocation { .. } => {
                "Check the tool's underlying error (cause); fix the tool implementation or its inputs.".to_owned()
            }
            Self::StructuredInvocation(error) => error.remediation.clone(),
            Self::OutputSerialization(_) => {
                "Ensure the Out type implements Serialize and produces a JSON-compatible value.".to_owned()
            }
        }
    }

    /// Returns structured invocation detail when this failure carries it.
    #[must_use]
    pub fn detail(&self) -> Option<&str> {
        match self {
            Self::StructuredInvocation(error) => Some(&error.detail),
            Self::Invocation { detail, .. } => Some(detail),
            _ => None,
        }
    }

    /// Returns scrubbed structured detail when this failure carries it.
    #[must_use]
    pub fn safe_details(&self) -> Option<&serde_json::Value> {
        match self {
            Self::StructuredInvocation(error) => error.safe_details.as_ref(),
            _ => None,
        }
    }
}

/// Result alias for executable tool operations.
pub type ToolResult<T> = Result<T, ToolError>;

#[cfg(test)]
mod tests {
    use super::{StructuredInvocationError, ToolError};

    /// Structured invocation metadata remains available without parsing text.
    #[test]
    fn structured_invocation_accessors_preserve_owned_fields() {
        let error = ToolError::StructuredInvocation(Box::new(StructuredInvocationError {
            code: "WYRD_TEST_403_DENIED".to_owned(),
            status: 403,
            title: "Permission denied".to_owned(),
            detail: "missing query permission".to_owned(),
            remediation: "request the query role".to_owned(),
            safe_details: Some(serde_json::json!({"permission": "bifrost_query:read"})),
        }));
        assert_eq!(error.code(), "WYRD_TEST_403_DENIED");
        assert_eq!(error.status(), 403);
        assert_eq!(error.title(), "Permission denied");
        assert_eq!(error.detail(), Some("missing query permission"));
        assert_eq!(error.remediation(), "request the query role");
        let expected = serde_json::json!({"permission": "bifrost_query:read"});
        assert_eq!(error.safe_details(), Some(&expected));
    }

    /// Existing generic tool errors retain their catalog codes.
    #[test]
    fn generic_invocation_code_is_unchanged() {
        let error = ToolError::Invocation {
            detail: "boom".to_owned(),
            cause: None,
        };
        assert_eq!(error.code(), "SKALD_TOOL_500_CALL");
        assert_eq!(error.status(), 500);
    }

    /// Structured failures preserve their typed source for error-chain consumers.
    #[test]
    fn structured_invocation_exposes_source_chain_and_display() {
        let error = ToolError::StructuredInvocation(Box::new(StructuredInvocationError {
            code: "WYRD_TEST_500_STRUCTURED".to_owned(),
            status: 500,
            title: "Structured failure".to_owned(),
            detail: "terminal detail".to_owned(),
            remediation: "inspect the terminal".to_owned(),
            safe_details: None,
        }));
        let source = std::error::Error::source(&error).expect("structured source is retained");
        assert_eq!(source.to_string(), "terminal detail");
        assert!(error.to_string().contains("terminal detail"));
        assert_eq!(error.code(), "WYRD_TEST_500_STRUCTURED");
        assert_eq!(error.status(), 500);
    }
}
