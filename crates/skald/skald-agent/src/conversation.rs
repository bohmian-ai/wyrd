//! In-run conversation accumulator for Skald agents.

use serde::{Deserialize, Serialize};
use skald_spec::MessageNum;

/// Provider-native record of turns observed during one agent run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Conversation {
    /// Ordered conversation turns.
    pub turns: Vec<ConversationTurn>,
}

impl Conversation {
    /// Creates an empty conversation.
    #[must_use]
    pub const fn new() -> Self {
        Self { turns: Vec::new() }
    }

    /// Appends one turn to the conversation.
    pub fn push(&mut self, turn: ConversationTurn) {
        self.turns.push(turn);
    }

    /// Returns the number of turns.
    #[must_use]
    pub fn len(&self) -> usize {
        self.turns.len()
    }

    /// Returns true when the conversation has no turns.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.turns.is_empty()
    }

    /// Iterates over turns in insertion order.
    pub fn iter(&self) -> std::slice::Iter<'_, ConversationTurn> {
        self.turns.iter()
    }
}

impl Default for Conversation {
    fn default() -> Self {
        Self::new()
    }
}

/// One provider-native turn in an agent conversation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConversationTurn {
    /// System prompt rendered from the agent prompt at run start.
    System {
        /// System text content.
        content: String,
    },
    /// User input passed to `Agent::run`.
    User {
        /// User text content.
        content: String,
    },
    /// Native model response, preserving the provider variant.
    Assistant {
        /// Provider-native assistant message.
        message: MessageNum,
    },
    /// Tool result keyed by the provider-emitted call id.
    ToolResult {
        /// Provider tool call id.
        call_id: String,
        /// Whether the tool call completed successfully.
        ok: bool,
        /// JSON result content.
        content: serde_json::Value,
    },
}
