//! Session memory trait and no-op default for agent runs.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use skald_spec::wire::openai_chat::OpenAiMessageContent;
use skald_spec::{MessageNum, OpenAiChatMessage};

use crate::conversation::ConversationTurn;

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

impl From<SessionTurn> for ConversationTurn {
    fn from(turn: SessionTurn) -> Self {
        match turn.role {
            Role::System => Self::System {
                content: turn.content,
            },
            Role::User => Self::User {
                content: turn.content,
            },
            Role::Assistant => Self::Assistant {
                message: MessageNum::OpenAi(OpenAiChatMessage {
                    role: "assistant".to_owned(),
                    content: Some(OpenAiMessageContent::Text(turn.content)),
                    name: None,
                    tool_calls: None,
                    tool_call_id: None,
                    refusal: None,
                }),
            },
            Role::Tool => Self::ToolResult {
                call_id: turn.call_id.unwrap_or_default(),
                ok: true,
                content: serde_json::Value::String(turn.content),
            },
        }
    }
}
