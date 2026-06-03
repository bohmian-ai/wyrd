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
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NameTaken { .. } => "SKALD_TOOL_409_NAME_TAKEN",
            Self::NotRegistered { .. } => "SKALD_TOOL_404_NOT_REGISTERED",
            Self::InvalidInput(_) => "SKALD_TOOL_422_INPUT",
            Self::Invocation { .. } => "SKALD_TOOL_500_CALL",
            Self::OutputSerialization(_) => "SKALD_TOOL_500_OUTPUT",
        }
    }

    /// Suggested HTTP status for this failure.
    pub const fn status(&self) -> u16 {
        match self {
            Self::NameTaken { .. } => 409,
            Self::NotRegistered { .. } => 404,
            Self::InvalidInput(_) => 422,
            Self::Invocation { .. } | Self::OutputSerialization(_) => 500,
        }
    }

    /// Stable problem-title text.
    pub const fn title(&self) -> &'static str {
        match self {
            Self::NameTaken { .. } => "Tool name already registered",
            Self::NotRegistered { .. } => "Tool name not registered",
            Self::InvalidInput(_) => {
                "Tool invocation input did not match the declared input schema"
            }
            Self::Invocation { .. } => "Tool invocation failed during execution",
            Self::OutputSerialization(_) => "Tool output could not be serialized to JSON",
        }
    }

    /// Operator-facing remediation hint.
    pub const fn remediation(&self) -> &'static str {
        match self {
            Self::NameTaken { .. } => {
                "Use ToolRegistry::register_force to overwrite or pick a unique tool name."
            }
            Self::NotRegistered { .. } => {
                "Register the tool (Tool::function + registry.register) before loading the agent, or call default_registry() before resolving agent tools."
            }
            Self::InvalidInput(_) => {
                "Adjust the tool call arguments to match the schema returned by tool.input_schema()."
            }
            Self::Invocation { .. } => {
                "Check the tool's underlying error (cause); fix the tool implementation or its inputs."
            }
            Self::OutputSerialization(_) => {
                "Ensure the Out type implements Serialize and produces a JSON-compatible value."
            }
        }
    }
}

/// Result alias for executable tool operations.
pub type ToolResult<T> = Result<T, ToolError>;
