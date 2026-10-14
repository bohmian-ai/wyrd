//! Session memory trait and no-op default for agent runs.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use skald_spec::wire::openai_chat::OpenAiMessageContent;
use skald_spec::wire::openai_responses::{OpenAiResponseContentPart, OpenAiResponseItem};
use skald_spec::{
    AnthropicContentBlock, AnthropicMessage, GoogleContent, GooglePart, MessageNum,
    OpenAiChatMessage, ProviderName, ProviderRequest,
};

use crate::conversation::ConversationTurn;
use crate::error::{AgentError, AgentResult};

/// Durable identifier for a session memory stream.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

impl SessionId {
    /// Creates a session id from a string-like value.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Borrows the raw session id string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Role of one session turn.
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(module = "wyrd.session", name = "Role", eq, eq_int, from_py_object)
)]
#[cfg_attr(
    feature = "python",
    expect(
        clippy::unsafe_derive_deserialize,
        reason = "the only unsafe is PyO3's generated glue, which deserialization never reaches"
    )
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// System instruction turn.
    System,
    /// User input turn.
    User,
    /// Assistant output turn.
    Assistant,
    /// Tool output turn.
    Tool,
}

/// Persistable text-oriented session turn.
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(module = "wyrd.session", name = "SessionTurn", skip_from_py_object)
)]
#[cfg_attr(
    feature = "python",
    expect(
        clippy::unsafe_derive_deserialize,
        reason = "the only unsafe is PyO3's generated glue, which deserialization never reaches"
    )
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionTurn {
    /// Role for this turn.
    pub role: Role,
    /// Text content for this turn.
    pub content: String,
    /// Tool call id for `Role::Tool` turns.
    pub call_id: Option<String>,
}

/// Session memory backend contract for agent runs.
#[async_trait]
pub trait SessionMemory: Send + Sync {
    /// Fetches recent turns once at run start to seed the conversation.
    async fn recent(
        &self,
        session_id: &SessionId,
        limit: usize,
    ) -> Result<Vec<SessionTurn>, SessionError>;

    /// Appends a successful turn to the session stream.
    async fn append(&self, session_id: &SessionId, turn: SessionTurn) -> Result<(), SessionError>;
}

/// Default session backend that drops all reads and writes.
pub struct NoSession;

#[async_trait]
impl SessionMemory for NoSession {
    async fn recent(
        &self,
        _session_id: &SessionId,
        _limit: usize,
    ) -> Result<Vec<SessionTurn>, SessionError> {
        Ok(Vec::new())
    }

    async fn append(
        &self,
        _session_id: &SessionId,
        _turn: SessionTurn,
    ) -> Result<(), SessionError> {
        Ok(())
    }
}

/// Session memory backend failures.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// Recent-turn lookup failed.
    #[error("session recent fetch failed: {0}")]
    RecentFailed(String),
    /// Turn append failed.
    #[error("session append failed: {0}")]
    AppendFailed(String),
}

/// Convert a session turn to a conversation turn in the native dialect of the
/// agent's `request` for `Role::Assistant`: an OpenAI Responses request gets
/// a Responses output message, other requests their provider's message shape.
///
/// Use this in preference to the `From` impl inside `seed_session_recent`.
/// The `From` impl falls back to the OpenAI Chat wire format for
/// `Role::Assistant` and is only safe for OpenAI Chat agents.
///
/// # Errors
///
/// Returns [`AgentError::LoopMessageType`] when `request` is a Jev question
/// request: Jev is not a chat dialect, so no session turn has a Jev shape and
/// none is ever re-encoded as another dialect.
pub(crate) fn session_turn_to_conversation_turn(
    turn: SessionTurn,
    request: &ProviderRequest,
) -> AgentResult<ConversationTurn> {
    if matches!(request, ProviderRequest::JevSystemOne(_)) {
        return Err(jev_is_not_a_chat_dialect());
    }
    Ok(match turn.role {
        Role::System => ConversationTurn::System {
            content: turn.content,
        },
        Role::User => ConversationTurn::User {
            content: turn.content,
        },
        Role::Assistant if matches!(request, ProviderRequest::OpenAiResponses(_)) => {
            ConversationTurn::Assistant {
                message: MessageNum::OpenAiResponses(vec![OpenAiResponseItem::Message {
                    role: "assistant".to_owned(),
                    content: vec![OpenAiResponseContentPart::OutputText { text: turn.content }],
                }]),
            }
        }
        Role::Assistant => {
            let message = match request.provider() {
                ProviderName::Anthropic => MessageNum::Anthropic(AnthropicMessage {
                    role: "assistant".to_owned(),
                    content: vec![AnthropicContentBlock::Text {
                        text: turn.content,
                        cache_control: None,
                        citations: None,
                    }],
                }),
                ProviderName::Google | ProviderName::Vertex => MessageNum::Gemini(GoogleContent {
                    role: "model".to_owned(),
                    parts: vec![GooglePart::Text { text: turn.content }],
                }),
                ProviderName::Jev => return Err(jev_is_not_a_chat_dialect()),
                ProviderName::OpenAi | ProviderName::Custom(_) => {
                    MessageNum::OpenAi(Box::new(OpenAiChatMessage {
                        role: "assistant".to_owned(),
                        content: Some(OpenAiMessageContent::Text(turn.content)),
                        ..Default::default()
                    }))
                }
            };
            ConversationTurn::Assistant { message }
        }
        Role::Tool => ConversationTurn::ToolResult {
            call_id: turn.call_id.unwrap_or_default(),
            ok: true,
            content: serde_json::Value::String(turn.content),
        },
    })
}

/// Refusal of a Jev request in the Agent loop: Jev answers keyed questions
/// and has no conversation turns, so it is never treated as `OpenAI` chat.
fn jev_is_not_a_chat_dialect() -> AgentError {
    AgentError::LoopMessageType {
        provider: ProviderName::Jev,
        detail: "jev is not a chat dialect; a Jev request never enters the Agent loop".to_owned(),
    }
}

impl From<SessionTurn> for ConversationTurn {
    /// Converts a session turn to a conversation turn.
    ///
    /// For `Role::Assistant` this always produces `MessageNum::OpenAi`. Use
    /// `session_turn_to_conversation_turn` when the provider is known to get
    /// the correct wire format for Anthropic or Gemini agents.
    fn from(turn: SessionTurn) -> Self {
        match turn.role {
            Role::System => Self::System {
                content: turn.content,
            },
            Role::User => Self::User {
                content: turn.content,
            },
            Role::Assistant => Self::Assistant {
                message: MessageNum::OpenAi(Box::new(OpenAiChatMessage {
                    role: "assistant".to_owned(),
                    content: Some(OpenAiMessageContent::Text(turn.content)),
                    ..Default::default()
                })),
            },
            Role::Tool => Self::ToolResult {
                call_id: turn.call_id.unwrap_or_default(),
                ok: true,
                content: serde_json::Value::String(turn.content),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use skald_spec::{ProviderName, ProviderRequest};

    use super::{Role, SessionTurn, session_turn_to_conversation_turn};
    use crate::error::AgentError;

    /// Every session turn of a Jev request is refused with a typed loop
    /// error naming Jev, never re-encoded as an `OpenAI` chat message.
    ///
    /// # Panics
    ///
    /// Panics when a turn converts or the error is not the Jev refusal.
    #[test]
    fn jev_session_turns_are_refused_as_not_a_chat_dialect() {
        let request = ProviderRequest::JevSystemOne(
            serde_json::from_value(json!({
                "model": "jev-latest",
                "state": "s",
                "questions": {"q": {"type": "noul", "instructions": "x"}}
            }))
            .expect("typed Jev request"),
        );
        for role in [Role::System, Role::User, Role::Assistant, Role::Tool] {
            let turn = SessionTurn {
                role,
                content: "hi".to_owned(),
                call_id: None,
            };
            let error = session_turn_to_conversation_turn(turn, &request)
                .expect_err("a Jev request has no chat turns");
            assert!(
                matches!(
                    &error,
                    AgentError::LoopMessageType { provider: ProviderName::Jev, detail }
                        if detail.contains("not a chat dialect")
                ),
                "{error}"
            );
        }
    }
}
