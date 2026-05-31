//! Typed workflow context and persistence snapshot.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use skald_spec::MessageNum;

/// Workflow-level shared state passed through execution paths.
#[derive(Debug, Clone)]
pub struct Context {
    /// Per-task native message history, keyed by task id.
    pub task_messages: HashMap<String, Vec<MessageNum>>,
    /// Free-form mutable shared state.
    pub state: Value,
    /// Immutable global context shared with every task.
    pub global: Option<Arc<Value>>,
}

impl Default for Context {
    fn default() -> Self {
        Self {
            task_messages: HashMap::new(),
            state: Value::Null,
            global: None,
        }
    }
}

impl Context {
    /// Construct an empty context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct a context with an immutable global value preloaded.
    pub fn with_global(global: Value) -> Self {
        Self {
            global: Some(Arc::new(global)),
            ..Self::default()
        }
    }
}

/// Serialization wrapper used when persisting a context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextSnapshot {
    /// Mirrors `Context::task_messages`.
    pub task_messages: HashMap<String, Vec<MessageNum>>,
    /// Mirrors `Context::state`.
    pub state: Value,
    /// Mirrors `Context::global` without runtime sharing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub global: Option<Value>,
}

impl From<&Context> for ContextSnapshot {
    fn from(ctx: &Context) -> Self {
        Self {
            task_messages: ctx.task_messages.clone(),
            state: ctx.state.clone(),
            global: ctx.global.as_ref().map(|global| (**global).clone()),
        }
    }
}

impl From<ContextSnapshot> for Context {
    fn from(snapshot: ContextSnapshot) -> Self {
        Self {
            task_messages: snapshot.task_messages,
            state: snapshot.state,
            global: snapshot.global.map(Arc::new),
        }
    }
}
