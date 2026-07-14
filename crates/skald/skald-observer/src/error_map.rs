//! Map stable Skald error codes onto Wyrd-side classifications.

use wyrd_spec::error::WyrdError;

/// Local mirror for Wyrd agent/workflow error classifications.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WyrdErrorVariant {
    /// Agent tool was not found.
    AgentToolNotFound,
    /// Model requested a tool that is not attached to this agent.
    AgentToolNotInAgent,
    /// Agent and prompt providers differed.
    AgentProviderMismatch,
    /// Agent-as-tool delegation exceeded the configured nesting depth.
    AgentDelegationDepth,
    /// Tool arguments failed validation.
    AgentToolArgs,
    /// Prompt rendering or shaping failed.
    AgentPrompt,
    /// Structured response decoding failed.
    AgentStructuredDecode,
    /// Agent constructor or method argument was invalid.
    AgentInvalidArgument,
    /// Loop history included a message for another provider family.
    AgentLoopMessageType,
    /// Agent hit its iteration cap.
    AgentMaxIterations,
    /// User callback panicked.
    AgentCallbackPanic,
    /// Journal append failed.
    AgentJournalAppend,
    /// Provider call failed.
    AgentProvider,
    /// Agent run exceeded its configured timeout.
    AgentTimeout,
    /// Session memory failed to fetch recent turns.
    SessionRecentFailed,
    /// Session memory failed to append a turn.
    SessionAppendFailed,
    /// Workflow-layer error.
    Workflow(String),
}

impl WyrdErrorVariant {
    /// Stable Wyrd-side code recorded into Vala.
    #[must_use]
    pub fn code(&self) -> String {
        match self {
            Self::AgentToolNotFound => "WYRD_AGENT_404_TOOL".to_owned(),
            Self::AgentToolNotInAgent => "WYRD_AGENT_404_TOOL_NOT_IN_AGENT".to_owned(),
            Self::AgentProviderMismatch => "WYRD_AGENT_409_PROVIDER_MISMATCH".to_owned(),
            Self::AgentDelegationDepth => "WYRD_AGENT_412_DELEGATION_DEPTH".to_owned(),
            Self::AgentToolArgs => "WYRD_AGENT_422_TOOL_ARGS".to_owned(),
            Self::AgentPrompt => "WYRD_AGENT_422_PROMPT".to_owned(),
            Self::AgentStructuredDecode => "WYRD_AGENT_422_STRUCTURED_DECODE".to_owned(),
            Self::AgentInvalidArgument => "WYRD_AGENT_422_INVALID_ARGUMENT".to_owned(),
            Self::AgentLoopMessageType => WyrdError::AgentLoopMessageType {
                message: String::new(),
                details: serde_json::Value::Null,
            }
            .code()
            .to_owned(),
            Self::AgentMaxIterations => "WYRD_AGENT_500_MAX_ITERATIONS".to_owned(),
            Self::AgentCallbackPanic => "WYRD_AGENT_500_CALLBACK_PANIC".to_owned(),
            Self::AgentJournalAppend => "WYRD_AGENT_500_JOURNAL".to_owned(),
            Self::AgentProvider => "WYRD_AGENT_502_PROVIDER".to_owned(),
            Self::AgentTimeout => "WYRD_AGENT_504_TIMEOUT".to_owned(),
            Self::SessionRecentFailed => "WYRD_SESSION_500_RECENT".to_owned(),
            Self::SessionAppendFailed => "WYRD_SESSION_500_APPEND".to_owned(),
            Self::Workflow(code) if code.starts_with("WYRD_") => code.clone(),
            Self::Workflow(code) => code
                .strip_prefix("SKALD_")
                .map_or_else(|| format!("WYRD_{code}"), |suffix| format!("WYRD_{suffix}")),
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
        "SKALD_AGENT_404_TOOL" => Some(WyrdErrorVariant::AgentToolNotFound),
        "SKALD_AGENT_404_TOOL_NOT_IN_AGENT" => Some(WyrdErrorVariant::AgentToolNotInAgent),
        "SKALD_AGENT_409_PROVIDER_MISMATCH" => Some(WyrdErrorVariant::AgentProviderMismatch),
        "SKALD_AGENT_412_DELEGATION_DEPTH" => Some(WyrdErrorVariant::AgentDelegationDepth),
        "SKALD_AGENT_422_TOOL_ARGS" => Some(WyrdErrorVariant::AgentToolArgs),
        "SKALD_AGENT_422_PROMPT" => Some(WyrdErrorVariant::AgentPrompt),
        "SKALD_AGENT_422_STRUCTURED_DECODE" => Some(WyrdErrorVariant::AgentStructuredDecode),
        "SKALD_AGENT_422_INVALID_ARGUMENT" => Some(WyrdErrorVariant::AgentInvalidArgument),
        "SKALD_AGENT_422_LOOP_MESSAGE_TYPE" => Some(WyrdErrorVariant::AgentLoopMessageType),
        "SKALD_AGENT_500_CALLBACK_PANIC" => Some(WyrdErrorVariant::AgentCallbackPanic),
        "SKALD_AGENT_500_JOURNAL" => Some(WyrdErrorVariant::AgentJournalAppend),
        "SKALD_AGENT_500_MAX_ITERATIONS" => Some(WyrdErrorVariant::AgentMaxIterations),
        "SKALD_AGENT_502_PROVIDER" => Some(WyrdErrorVariant::AgentProvider),
        "SKALD_AGENT_504_TIMEOUT" => Some(WyrdErrorVariant::AgentTimeout),
        "SKALD_SESSION_500_RECENT" => Some(WyrdErrorVariant::SessionRecentFailed),
        "SKALD_SESSION_500_APPEND" => Some(WyrdErrorVariant::SessionAppendFailed),
        "SKALD_WORKFLOW_404_TASK"
        | "SKALD_WORKFLOW_404_AGENT"
        | "WYRD_WORKFLOW_422_MISSING_DEPENDENCY"
        | "WYRD_WORKFLOW_422_DUPLICATE_STEP_ID"
        | "WYRD_WORKFLOW_422_CYCLE"
        | "SKALD_WORKFLOW_422_OUTPUT_SCHEMA"
        | "SKALD_WORKFLOW_500_MAX_RETRIES"
        | "SKALD_WORKFLOW_500_AGENT_RESPONSE_MISSING"
        | "SKALD_WORKFLOW_500_INTERNAL"
        | "SKALD_WORKFLOW_500_STALLED"
        | "SKALD_WORKFLOW_500_LOCK"
        | "SKALD_WORKFLOW_501_UNSUPPORTED_HANDOFF"
        | "WYRD_WORKFLOW_422_MISSING_PARAMETER" => {
            Some(WyrdErrorVariant::Workflow(code.to_owned()))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{WyrdErrorVariant, map_skald_code};
    use wyrd_spec::error::WyrdError;

    const AGENT_CODES: &[(&str, WyrdErrorVariant, &str)] = &[
        (
            "SKALD_AGENT_404_TOOL",
            WyrdErrorVariant::AgentToolNotFound,
            "WYRD_AGENT_404_TOOL",
        ),
        (
            "SKALD_AGENT_404_TOOL_NOT_IN_AGENT",
            WyrdErrorVariant::AgentToolNotInAgent,
            "WYRD_AGENT_404_TOOL_NOT_IN_AGENT",
        ),
        (
            "SKALD_AGENT_409_PROVIDER_MISMATCH",
            WyrdErrorVariant::AgentProviderMismatch,
            "WYRD_AGENT_409_PROVIDER_MISMATCH",
        ),
        (
            "SKALD_AGENT_412_DELEGATION_DEPTH",
            WyrdErrorVariant::AgentDelegationDepth,
            "WYRD_AGENT_412_DELEGATION_DEPTH",
        ),
        (
            "SKALD_AGENT_422_TOOL_ARGS",
            WyrdErrorVariant::AgentToolArgs,
            "WYRD_AGENT_422_TOOL_ARGS",
        ),
        (
            "SKALD_AGENT_422_PROMPT",
            WyrdErrorVariant::AgentPrompt,
            "WYRD_AGENT_422_PROMPT",
        ),
        (
            "SKALD_AGENT_422_STRUCTURED_DECODE",
            WyrdErrorVariant::AgentStructuredDecode,
            "WYRD_AGENT_422_STRUCTURED_DECODE",
        ),
        (
            "SKALD_AGENT_422_INVALID_ARGUMENT",
            WyrdErrorVariant::AgentInvalidArgument,
            "WYRD_AGENT_422_INVALID_ARGUMENT",
        ),
        (
            "SKALD_AGENT_422_LOOP_MESSAGE_TYPE",
            WyrdErrorVariant::AgentLoopMessageType,
            "WYRD_AGENT_422_LOOP_MESSAGE_TYPE",
        ),
        (
            "SKALD_AGENT_500_CALLBACK_PANIC",
            WyrdErrorVariant::AgentCallbackPanic,
            "WYRD_AGENT_500_CALLBACK_PANIC",
        ),
        (
            "SKALD_AGENT_500_JOURNAL",
            WyrdErrorVariant::AgentJournalAppend,
            "WYRD_AGENT_500_JOURNAL",
        ),
        (
            "SKALD_AGENT_500_MAX_ITERATIONS",
            WyrdErrorVariant::AgentMaxIterations,
            "WYRD_AGENT_500_MAX_ITERATIONS",
        ),
        (
            "SKALD_AGENT_502_PROVIDER",
            WyrdErrorVariant::AgentProvider,
            "WYRD_AGENT_502_PROVIDER",
        ),
        (
            "SKALD_AGENT_504_TIMEOUT",
            WyrdErrorVariant::AgentTimeout,
            "WYRD_AGENT_504_TIMEOUT",
        ),
    ];

    const SESSION_CODES: &[(&str, WyrdErrorVariant, &str)] = &[
        (
            "SKALD_SESSION_500_RECENT",
            WyrdErrorVariant::SessionRecentFailed,
            "WYRD_SESSION_500_RECENT",
        ),
        (
            "SKALD_SESSION_500_APPEND",
            WyrdErrorVariant::SessionAppendFailed,
            "WYRD_SESSION_500_APPEND",
        ),
    ];

    const WORKFLOW_CODES: &[&str] = &[
        "SKALD_WORKFLOW_404_TASK",
        "SKALD_WORKFLOW_404_AGENT",
        "WYRD_WORKFLOW_422_DUPLICATE_STEP_ID",
        "WYRD_WORKFLOW_422_MISSING_DEPENDENCY",
        "WYRD_WORKFLOW_422_CYCLE",
        "SKALD_WORKFLOW_422_OUTPUT_SCHEMA",
        "SKALD_WORKFLOW_500_AGENT_RESPONSE_MISSING",
        "SKALD_WORKFLOW_500_INTERNAL",
        "SKALD_WORKFLOW_500_LOCK",
        "SKALD_WORKFLOW_500_MAX_RETRIES",
        "SKALD_WORKFLOW_500_STALLED",
        "SKALD_WORKFLOW_501_UNSUPPORTED_HANDOFF",
        "WYRD_WORKFLOW_422_MISSING_PARAMETER",
    ];

    #[test]
    fn every_known_agent_code_maps_to_some() {
        for (skald_code, expected, _) in AGENT_CODES {
            assert_eq!(map_skald_code(skald_code), Some(expected.clone()));
        }
    }

    #[test]
    fn loop_message_type_maps_to_agent_loop_message_type_variant() {
        assert_eq!(
            map_skald_code("SKALD_AGENT_422_LOOP_MESSAGE_TYPE"),
            Some(WyrdErrorVariant::AgentLoopMessageType)
        );
    }

    #[test]
    fn every_known_session_code_maps_to_some() {
        for (skald_code, expected, _) in SESSION_CODES {
            assert_eq!(map_skald_code(skald_code), Some(expected.clone()));
        }
    }

    #[test]
    fn every_known_workflow_code_maps_to_workflow_variant() {
        for code in WORKFLOW_CODES {
            assert_eq!(
                map_skald_code(code),
                Some(WyrdErrorVariant::Workflow((*code).to_owned()))
            );
        }
    }

    #[test]
    fn unknown_skald_code_returns_none() {
        assert_eq!(map_skald_code("SKALD_AGENT_999_UNKNOWN"), None);
    }

    #[test]
    fn unknown_prefix_returns_none() {
        assert_eq!(map_skald_code("WYRD_PROMPT_404"), None);
    }

    #[test]
    fn variant_code_strings_match_wyrd_prefix() {
        for (_, variant, expected_code) in AGENT_CODES.iter().chain(SESSION_CODES.iter()) {
            let code = variant.code();
            assert_eq!(code, *expected_code);
            assert!(code.starts_with("WYRD_"));
        }

        for skald_code in WORKFLOW_CODES {
            let variant = WyrdErrorVariant::Workflow((*skald_code).to_owned());
            let code = variant.code();
            assert_eq!(code, skald_code.replacen("SKALD_", "WYRD_", 1));
            assert!(code.starts_with("WYRD_"));
        }
    }

    #[test]
    fn agent_loop_message_type_variant_has_correct_code() {
        assert_eq!(
            WyrdErrorVariant::AgentLoopMessageType.code(),
            "WYRD_AGENT_422_LOOP_MESSAGE_TYPE"
        );
    }

    #[test]
    fn public_agent_loop_message_type_catalog_code_matches() {
        let error = WyrdError::AgentLoopMessageType {
            message: "loop history contains a provider mismatch".to_owned(),
            details: serde_json::json!({ "provider": "OpenAi" }),
        };

        assert_eq!(error.code(), "WYRD_AGENT_422_LOOP_MESSAGE_TYPE");
        assert_eq!(error.status(), 422);
        assert_eq!(error.title(), "Loop message type mismatch");
    }
}
