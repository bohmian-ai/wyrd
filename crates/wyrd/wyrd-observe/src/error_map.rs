//! Map stable Skald error codes onto Wyrd-side classifications.

/// Local mirror for Wyrd agent/workflow error classifications.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WyrdErrorVariant {
    /// Agent provider was not found.
    AgentProviderNotFound,
    /// Agent tool was not found.
    AgentToolNotFound,
    /// Agent and prompt providers differed.
    AgentProviderMismatch,
    /// Tool arguments failed validation.
    AgentToolArgs,
    /// System prompt could not be represented for the provider.
    AgentSystemPrompt,
    /// Prompt rendering or shaping failed.
    AgentPrompt,
    /// Agent hit its iteration cap.
    AgentMaxIterations,
    /// Provider call failed.
    AgentProvider,
    /// Workflow-layer error.
    Workflow(String),
}

impl WyrdErrorVariant {
    /// Stable Wyrd-side code recorded into Vala.
    #[must_use]
    pub fn code(&self) -> String {
        match self {
            Self::AgentProviderNotFound => "WYRD_AGENT_404_PROVIDER".to_owned(),
            Self::AgentToolNotFound => "WYRD_AGENT_404_TOOL".to_owned(),
            Self::AgentProviderMismatch => "WYRD_AGENT_409_PROVIDER_MISMATCH".to_owned(),
            Self::AgentToolArgs => "WYRD_AGENT_422_TOOL_ARGS".to_owned(),
            Self::AgentSystemPrompt => "WYRD_AGENT_422_SYSTEM_PROMPT".to_owned(),
            Self::AgentPrompt => "WYRD_AGENT_422_PROMPT".to_owned(),
            Self::AgentMaxIterations => "WYRD_AGENT_500_MAX_ITERATIONS".to_owned(),
            Self::AgentProvider => "WYRD_AGENT_502_PROVIDER".to_owned(),
            Self::Workflow(code) => format!("WYRD_{code}"),
        }
    }
}

/// Look up the Wyrd-side classification for a stable Skald code.
///
/// This local [`WyrdErrorVariant`] mirror disappears when the future
/// `AgentCard` and `WorkflowCard` phase adds canonical `#[wyrd_error(...)]`
/// metadata to Wyrd's public error catalog. Until then, every known
/// `SKALD_AGENT_*` and `SKALD_WORKFLOW_*` code is enumerated here for Vala
/// observation records.
#[must_use]
pub fn map_skald_code(code: &str) -> Option<WyrdErrorVariant> {
    match code {
        "SKALD_AGENT_404_PROVIDER" => Some(WyrdErrorVariant::AgentProviderNotFound),
        "SKALD_AGENT_404_TOOL" => Some(WyrdErrorVariant::AgentToolNotFound),
        "SKALD_AGENT_409_PROVIDER_MISMATCH" => Some(WyrdErrorVariant::AgentProviderMismatch),
        "SKALD_AGENT_422_TOOL_ARGS" => Some(WyrdErrorVariant::AgentToolArgs),
        "SKALD_AGENT_422_SYSTEM_PROMPT" => Some(WyrdErrorVariant::AgentSystemPrompt),
        "SKALD_AGENT_422_PROMPT" => Some(WyrdErrorVariant::AgentPrompt),
        "SKALD_AGENT_500_MAX_ITERATIONS" => Some(WyrdErrorVariant::AgentMaxIterations),
        "SKALD_AGENT_502_PROVIDER" => Some(WyrdErrorVariant::AgentProvider),
        "SKALD_WORKFLOW_404_TASK"
        | "SKALD_WORKFLOW_404_AGENT"
        | "SKALD_WORKFLOW_422_DEP_MISSING"
        | "SKALD_WORKFLOW_409_TASK_EXISTS"
        | "SKALD_WORKFLOW_422_SELF_DEP"
        | "SKALD_WORKFLOW_422_CYCLE"
        | "SKALD_WORKFLOW_422_OUTPUT_SCHEMA"
        | "SKALD_WORKFLOW_500_MAX_RETRIES"
        | "SKALD_WORKFLOW_500_STALLED"
        | "SKALD_WORKFLOW_500_LOCK"
        | "SKALD_WORKFLOW_501_UNSUPPORTED_HANDOFF" => {
            Some(WyrdErrorVariant::Workflow(code.to_owned()))
        }
        _ => None,
    }
}
