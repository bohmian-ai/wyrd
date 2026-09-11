use wyrd_spec::error::WyrdError;

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

impl From<ToolError> for WyrdError {
    /// Project an owned tool failure onto the derive-backed Wyrd catalog.
    fn from(error: ToolError) -> Self {
        Self::from(&error)
    }
}

impl From<&ToolError> for WyrdError {
    /// Project a tool failure onto the derive-backed Wyrd catalog.
    ///
    /// The catalog owns every public metadata field, so this projection only
    /// chooses the variant and supplies the message plus structured details.
    /// A [`ToolError::StructuredInvocation`] already carries a stable code from
    /// its owner, so it is reconstructed from that code where the catalog knows
    /// it and otherwise preserved under `details.original_code`.
    fn from(error: &ToolError) -> Self {
        let message = error.to_string();
        match error {
            ToolError::InvalidInput(detail) => Self::ToolInvalidInput {
                message,
                details: serde_json::json!({ "reason": detail }),
            },
            ToolError::Invocation { detail, .. } => Self::ToolInvocationFailed {
                message,
                details: serde_json::json!({ "reason": detail }),
            },
            ToolError::StructuredInvocation(structured) => {
                let details = structured
                    .safe_details
                    .clone()
                    .unwrap_or_else(|| serde_json::json!({}));
                Self::from_code(&structured.code, structured.detail.clone(), details.clone())
                    .unwrap_or(Self::ToolInvocationFailed {
                        message,
                        details: serde_json::json!({
                            "original_code": structured.code,
                            "original_details": details,
                        }),
                    })
            }
            ToolError::OutputSerialization(detail) => Self::ToolOutputSerialization {
                message,
                details: serde_json::json!({ "reason": detail }),
            },
            ToolError::NameTaken { name } => Self::ToolNameTaken {
                message,
                details: serde_json::json!({ "tool": name }),
            },
            ToolError::NotRegistered { name, available } => Self::ToolNotRegistered {
                message,
                details: serde_json::json!({ "tool": name, "available": available }),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{StructuredInvocationError, ToolError, WyrdError};

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
        assert_eq!(error.detail(), Some("missing query permission"));
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
        let projected = WyrdError::from(&error);
        assert_eq!(projected.code(), "WYRD_TOOL_500_CALL");
        assert_eq!(projected.status(), 500);
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
    }

    /// Every tool failure reaches a catalog variant, and a structured failure
    /// whose code the catalog does not know keeps that code in `details`
    /// instead of losing it.
    #[test]
    fn tool_errors_project_onto_the_catalog() {
        let cases: Vec<(ToolError, &str)> = vec![
            (
                ToolError::InvalidInput("bad".to_owned()),
                "WYRD_TOOL_422_INPUT",
            ),
            (
                ToolError::Invocation {
                    detail: "boom".to_owned(),
                    cause: None,
                },
                "WYRD_TOOL_500_CALL",
            ),
            (
                ToolError::OutputSerialization("bad".to_owned()),
                "WYRD_TOOL_500_OUTPUT",
            ),
            (
                ToolError::NameTaken {
                    name: "echo".to_owned(),
                },
                "WYRD_TOOL_409_NAME_TAKEN",
            ),
            (
                ToolError::NotRegistered {
                    name: "echo".to_owned(),
                    available: Vec::new(),
                },
                "WYRD_TOOL_404_NOT_REGISTERED",
            ),
            (
                ToolError::StructuredInvocation(Box::new(StructuredInvocationError {
                    code: "WYRD_AGENT_412_DELEGATION_DEPTH".to_owned(),
                    status: 412,
                    title: "Agent delegation depth exceeded".to_owned(),
                    detail: "too deep".to_owned(),
                    remediation: "reduce nesting".to_owned(),
                    safe_details: None,
                })),
                "WYRD_AGENT_412_DELEGATION_DEPTH",
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(WyrdError::from(&error).code(), expected, "for {error:?}");
        }

        let unknown = ToolError::StructuredInvocation(Box::new(StructuredInvocationError {
            code: "WYRD_TEST_403_DENIED".to_owned(),
            status: 403,
            title: "Permission denied".to_owned(),
            detail: "missing query permission".to_owned(),
            remediation: "request the query role".to_owned(),
            safe_details: None,
        }));
        let projected = WyrdError::from(&unknown);
        assert_eq!(projected.code(), "WYRD_TOOL_500_CALL");
        assert_eq!(
            projected.as_problem_json()["details"]["original_code"],
            serde_json::Value::String("WYRD_TEST_403_DENIED".to_owned())
        );
    }
}
