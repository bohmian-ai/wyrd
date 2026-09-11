use std::sync::Arc;

use serde_json::{Value, json};
use skald_agent::{
    AgentContext, AgentError, AgentRun, CallbackOutcome, Conversation, ConversationTurn, Journal,
    JournalError, JournalEvent, NoSession, NoopJournal, Role, SessionError, SessionId,
    SessionMemory, SessionTurn,
};
use skald_spec::{
    MessageNum, OpenAiChatMessage, ProviderName, ProviderRequest, ProviderResponse,
    wire::openai_chat::OpenAiMessageContent,
};
use skald_tool::ToolError;
use wyrd_spec::error::WyrdError;

fn openai_assistant_message(content: &str) -> MessageNum {
    MessageNum::OpenAi(Box::new(OpenAiChatMessage {
        role: "assistant".to_owned(),
        content: Some(OpenAiMessageContent::Text(content.to_owned())),
        name: None,
        tool_calls: None,
        tool_call_id: None,
        refusal: None,
        annotations: Vec::new(),
        audio: None,
    }))
}

#[test]
fn conversation_serde_round_trip() {
    let mut conversation = Conversation::new();
    conversation.push(ConversationTurn::System {
        content: "system".to_owned(),
    });
    conversation.push(ConversationTurn::User {
        content: "user".to_owned(),
    });
    conversation.push(ConversationTurn::Assistant {
        message: openai_assistant_message("assistant"),
    });
    conversation.push(ConversationTurn::ToolResult {
        call_id: "call_1".to_owned(),
        ok: true,
        content: json!({ "ok": true }),
    });

    let json = serde_json::to_string(&conversation).expect("conversation serializes");
    let round_tripped: Conversation =
        serde_json::from_str(&json).expect("conversation deserializes");

    assert_eq!(round_tripped, conversation);
}

#[test]
fn conversation_turn_tag_is_lowercase_snake_case() {
    let turns = [
        (
            ConversationTurn::System {
                content: "system".to_owned(),
            },
            "system",
        ),
        (
            ConversationTurn::User {
                content: "user".to_owned(),
            },
            "user",
        ),
        (
            ConversationTurn::Assistant {
                message: openai_assistant_message("assistant"),
            },
            "assistant",
        ),
        (
            ConversationTurn::ToolResult {
                call_id: "call_1".to_owned(),
                ok: true,
                content: json!("tool"),
            },
            "tool_result",
        ),
    ];

    for (turn, expected) in turns {
        let value = serde_json::to_value(turn).expect("turn serializes");
        assert_eq!(value["type"], expected);
    }
}

#[test]
fn callback_outcome_construct_compiles_for_every_t() {
    let _compile_gate = || {
        let _: CallbackOutcome<String> = CallbackOutcome::Continue;
        let _: CallbackOutcome<AgentRun> = CallbackOutcome::Continue;
        let _: CallbackOutcome<ProviderRequest> = CallbackOutcome::Continue;
        let _: CallbackOutcome<ProviderResponse> = CallbackOutcome::Continue;
        let _: CallbackOutcome<Value> = CallbackOutcome::Continue;
        let _: CallbackOutcome<Result<Value, ToolError>> = CallbackOutcome::Continue;
    };
}

#[test]
fn agent_context_clone_is_cheap_arc() {
    let mut conversation = Conversation::new();
    conversation.push(ConversationTurn::User {
        content: "hi".to_owned(),
    });
    let context = AgentContext {
        agent_id: "agent".to_owned(),
        session_id: Some("session".to_owned()),
        iteration: 1,
        conversation: Arc::new(conversation),
    };

    let cloned = context.clone();

    assert!(Arc::ptr_eq(&context.conversation, &cloned.conversation));
}

#[test]
fn session_id_round_trip() {
    let session_id = SessionId::new("abc");
    let json = serde_json::to_string(&session_id).expect("session id serializes");
    let round_tripped: SessionId = serde_json::from_str(&json).expect("session id deserializes");

    assert_eq!(json, "\"abc\"");
    assert_eq!(round_tripped, session_id);
}

#[test]
fn session_turn_round_trip_all_roles() {
    let turns = [
        SessionTurn {
            role: Role::System,
            content: "system".to_owned(),
            call_id: None,
        },
        SessionTurn {
            role: Role::User,
            content: "user".to_owned(),
            call_id: None,
        },
        SessionTurn {
            role: Role::Assistant,
            content: "assistant".to_owned(),
            call_id: None,
        },
        SessionTurn {
            role: Role::Tool,
            content: "tool".to_owned(),
            call_id: Some("call_1".to_owned()),
        },
        SessionTurn {
            role: Role::Tool,
            content: "tool".to_owned(),
            call_id: None,
        },
    ];

    for turn in turns {
        let json = serde_json::to_string(&turn).expect("session turn serializes");
        let round_tripped: SessionTurn =
            serde_json::from_str(&json).expect("session turn deserializes");
        assert_eq!(round_tripped, turn);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn no_session_recent_returns_empty() {
    let turns = NoSession
        .recent(&SessionId::new("x"), 50)
        .await
        .expect("no session recent succeeds");

    assert!(turns.is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn no_session_append_returns_ok() {
    let turn = SessionTurn {
        role: Role::User,
        content: "hi".to_owned(),
        call_id: None,
    };

    assert!(NoSession.append(&SessionId::new("x"), turn).await.is_ok());
}

#[test]
fn journal_event_serde_round_trip_all_variants() {
    let events = [
        JournalEvent::AgentStart {
            agent_id: "agent".to_owned(),
            input: "input".to_owned(),
            session_id: Some("session".to_owned()),
        },
        JournalEvent::Iteration { index: 0 },
        JournalEvent::ModelCall {
            iteration: 1,
            provider: "openai".to_owned(),
            model: "gpt-4o".to_owned(),
        },
        JournalEvent::ModelResult {
            iteration: 1,
            finish_reason: "stop".to_owned(),
            synthetic: false,
        },
        JournalEvent::ToolCall {
            iteration: 1,
            call_id: "call_1".to_owned(),
            tool_name: "lookup".to_owned(),
            args: json!({ "q": "x" }),
        },
        JournalEvent::ToolResult {
            iteration: 1,
            call_id: "call_1".to_owned(),
            ok: true,
            output: json!({ "answer": "x" }),
        },
        JournalEvent::AgentFinish {
            agent_id: "agent".to_owned(),
            finish_reason: "stop".to_owned(),
            iterations: 1,
        },
        JournalEvent::AgentError {
            agent_id: "agent".to_owned(),
            code: "SKALD_AGENT_500_TEST".to_owned(),
            message: "failed".to_owned(),
        },
    ];

    for event in events {
        let json = serde_json::to_string(&event).expect("journal event serializes");
        let round_tripped: JournalEvent =
            serde_json::from_str(&json).expect("journal event deserializes");
        assert_eq!(round_tripped, event);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn noop_journal_append_returns_ok() {
    assert!(
        NoopJournal
            .append(JournalEvent::Iteration { index: 0 })
            .await
            .is_ok()
    );
}

#[test]
fn session_turn_into_conversation_turn_maps_roles() {
    let user = ConversationTurn::from(SessionTurn {
        role: Role::User,
        content: "hi".to_owned(),
        call_id: None,
    });
    assert_eq!(
        user,
        ConversationTurn::User {
            content: "hi".to_owned()
        }
    );

    let system = ConversationTurn::from(SessionTurn {
        role: Role::System,
        content: "system".to_owned(),
        call_id: None,
    });
    assert_eq!(
        system,
        ConversationTurn::System {
            content: "system".to_owned()
        }
    );

    let tool = ConversationTurn::from(SessionTurn {
        role: Role::Tool,
        content: "tool".to_owned(),
        call_id: Some("abc".to_owned()),
    });
    assert_eq!(
        tool,
        ConversationTurn::ToolResult {
            call_id: "abc".to_owned(),
            ok: true,
            content: Value::String("tool".to_owned()),
        }
    );

    let assistant = ConversationTurn::from(SessionTurn {
        role: Role::Assistant,
        content: "assistant".to_owned(),
        call_id: None,
    });
    assert!(matches!(assistant, ConversationTurn::Assistant { .. }));
}

#[test]
fn error_codes_map_to_wyrd_error() {
    let delegation = AgentError::DelegationDepthExceeded {
        chain: vec!["a".to_owned()],
    };
    assert_eq!(delegation.code(), "SKALD_AGENT_412_DELEGATION_DEPTH");
    let projected = WyrdError::from(&delegation);
    assert_eq!(projected.code(), "WYRD_AGENT_412_DELEGATION_DEPTH");
    assert_eq!(projected.status(), 412);
    assert_eq!(projected.title(), "Agent delegation depth exceeded");
    assert_eq!(
        projected.remediation(),
        "Reduce nested agent-as-tool calls or restructure the workflow so the delegation chain stays within the cap."
    );

    let callback = AgentError::CallbackPanic {
        hook: "before_model".to_owned(),
        payload: "boom".to_owned(),
    };
    assert_eq!(callback.code(), "SKALD_AGENT_500_CALLBACK_PANIC");

    let recent = AgentError::SessionRecentFailed {
        session_id: "x".to_owned(),
        source: SessionError::RecentFailed("e".to_owned()),
    };
    assert_eq!(recent.code(), "SKALD_SESSION_500_RECENT");

    let append = AgentError::SessionAppendFailed {
        session_id: "x".to_owned(),
        source: SessionError::AppendFailed("e".to_owned()),
    };
    assert_eq!(append.code(), "SKALD_SESSION_500_APPEND");

    let journal = AgentError::JournalAppendFailed {
        source: JournalError::AppendFailed("e".to_owned()),
    };
    assert_eq!(journal.code(), "SKALD_AGENT_500_JOURNAL");
}

#[test]
fn session_turn_tool_role_none_call_id_produces_empty_string() {
    let turn = ConversationTurn::from(SessionTurn {
        role: Role::Tool,
        content: "result".to_owned(),
        call_id: None,
    });
    match turn {
        ConversationTurn::ToolResult { call_id, .. } => {
            assert!(
                call_id.is_empty(),
                "None call_id maps to empty string; providers that validate tool call ids will \
                 receive a malformed message — callers must ensure call_id is always Some"
            );
        }
        other => panic!("expected ToolResult, got {other:?}"),
    }
}

/// Every agent failure must project onto a distinct, non-degraded catalog
/// variant: every public Wyrd boundary reads its metadata from that
/// projection alone.
#[test]
fn agent_errors_project_onto_the_catalog() {
    let cases: Vec<(AgentError, &str)> = vec![
        (
            AgentError::ToolNotFound {
                name: "echo".to_owned(),
            },
            "WYRD_AGENT_404_TOOL",
        ),
        (
            AgentError::ToolNotInAgent {
                name: "echo".to_owned(),
            },
            "WYRD_AGENT_404_TOOL_NOT_IN_AGENT",
        ),
        (
            AgentError::InvalidToolArgs {
                tool: "echo".to_owned(),
                detail: "bad".to_owned(),
            },
            "WYRD_AGENT_422_TOOL_ARGS",
        ),
        (
            AgentError::max_iterations("a", 3),
            "WYRD_AGENT_500_MAX_ITERATIONS",
        ),
        (
            AgentError::Provider(skald_runtime::SkaldRuntimeError::provider_not_registered(
                ProviderName::OpenAi,
            )),
            "WYRD_AGENT_502_PROVIDER",
        ),
        (
            AgentError::Tool(skald_tool::SkaldToolError::invalid_schema(None, "bad")),
            "WYRD_TOOL_400_INVALID_SCHEMA",
        ),
        (
            AgentError::Prompt {
                agent: "a".to_owned(),
                detail: "bad".to_owned(),
            },
            "WYRD_AGENT_422_PROMPT",
        ),
        (
            AgentError::LoopMessageType {
                provider: ProviderName::OpenAi,
                detail: "bad".to_owned(),
            },
            "WYRD_AGENT_422_LOOP_MESSAGE_TYPE",
        ),
        (
            AgentError::ProviderMismatch {
                agent: "a".to_owned(),
                agent_provider: ProviderName::OpenAi,
                prompt_provider: ProviderName::Anthropic,
            },
            "WYRD_AGENT_409_PROVIDER_MISMATCH",
        ),
        (
            AgentError::DelegationDepthExceeded { chain: Vec::new() },
            "WYRD_AGENT_412_DELEGATION_DEPTH",
        ),
        (
            AgentError::CallbackPanic {
                hook: "before_model".to_owned(),
                payload: "boom".to_owned(),
            },
            "WYRD_AGENT_500_CALLBACK_PANIC",
        ),
        (
            AgentError::SessionRecentFailed {
                session_id: "x".to_owned(),
                source: SessionError::RecentFailed("e".to_owned()),
            },
            "WYRD_SESSION_500_RECENT",
        ),
        (
            AgentError::SessionAppendFailed {
                session_id: "x".to_owned(),
                source: SessionError::AppendFailed("e".to_owned()),
            },
            "WYRD_SESSION_500_APPEND",
        ),
        (
            AgentError::JournalAppendFailed {
                source: JournalError::AppendFailed("e".to_owned()),
            },
            "WYRD_AGENT_500_JOURNAL",
        ),
        (
            AgentError::Timeout {
                duration: std::time::Duration::from_millis(250),
            },
            "WYRD_AGENT_504_TIMEOUT",
        ),
        (
            AgentError::StructuredOutputDecode {
                agent: "a".to_owned(),
                detail: "bad".to_owned(),
            },
            "WYRD_AGENT_422_STRUCTURED_DECODE",
        ),
        (
            AgentError::InvalidArgument {
                name: "output_type".to_owned(),
                detail: "bad".to_owned(),
            },
            "WYRD_AGENT_422_INVALID_ARGUMENT",
        ),
    ];

    for (error, expected) in cases {
        let message = error.to_string();
        let projected = WyrdError::from(&error);
        assert_eq!(projected.code(), expected, "for {error:?}");
        assert_eq!(
            projected.as_problem_json()["detail"],
            serde_json::Value::String(message),
            "projection must preserve the agent message for {error:?}"
        );
    }
}
