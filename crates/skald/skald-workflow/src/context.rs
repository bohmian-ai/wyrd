//! Typed workflow context and persistence snapshot.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use skald_spec::MessageNum;

/// Workflow-level shared state passed through execution paths.
#[derive(Debug, Clone)]
pub struct Context {
    /// Per-task native message history, keyed by task id.
    pub task_messages: HashMap<String, Vec<MessageNum>>,
    /// Workflow-level input available to every step render.
    pub input: Map<String, Value>,
    /// Cross-step parameters extracted from structured step outputs.
    pub parameters: Map<String, Value>,
    /// Free-form mutable shared state.
    pub state: Value,
    /// Immutable global context shared with every task.
    pub global: Option<Arc<Value>>,
}

impl Default for Context {
    fn default() -> Self {
        Self {
            task_messages: HashMap::new(),
            input: Map::new(),
            parameters: Map::new(),
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

    /// Records the messages a completed task contributes to successor inputs.
    pub fn record_task_messages(&mut self, task_id: impl Into<String>, msgs: Vec<MessageNum>) {
        self.task_messages.insert(task_id.into(), msgs);
    }

    /// Returns the messages recorded for an upstream task, when present.
    pub fn task_messages_for(&self, task_id: &str) -> Option<&[MessageNum]> {
        self.task_messages.get(task_id).map(Vec::as_slice)
    }

    /// Insert top-level structured-output keys into the workflow parameter map.
    pub fn ingest_structured_output(&mut self, output: &Map<String, Value>) {
        for (key, value) in output {
            self.parameters.insert(key.clone(), value.clone());
        }
    }

    /// Resolve a template variable against structured parameters, then input.
    pub fn lookup_variable(&self, name: &str) -> Option<&Value> {
        self.parameters.get(name).or_else(|| self.input.get(name))
    }
}

/// Serialization wrapper used when persisting a context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextSnapshot {
    /// Mirrors `Context::task_messages`.
    pub task_messages: HashMap<String, Vec<MessageNum>>,
    /// Mirrors `Context::input`.
    #[serde(default)]
    pub input: Map<String, Value>,
    /// Mirrors `Context::parameters`.
    #[serde(default)]
    pub parameters: Map<String, Value>,
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
            input: ctx.input.clone(),
            parameters: ctx.parameters.clone(),
            state: ctx.state.clone(),
            global: ctx.global.as_ref().map(|global| (**global).clone()),
        }
    }
}

impl From<ContextSnapshot> for Context {
    fn from(snapshot: ContextSnapshot) -> Self {
        Self {
            task_messages: snapshot.task_messages,
            input: snapshot.input,
            parameters: snapshot.parameters,
            state: snapshot.state,
            global: snapshot.global.map(Arc::new),
        }
    }
}
