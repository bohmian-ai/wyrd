//! Executable tool contract for the agent loop.
//!
//! [`skald_tool::ToolDef`] is the declaration shipped to providers;
//! [`AgentTool`] is the runnable the loop dispatches with provider-emitted args.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::value::RawValue;
use skald_tool::ToolDef;
use thiserror::Error;

/// Failures a tool implementation may raise during the loop.
#[derive(Debug, Error)]
pub enum AgentToolError {
    /// Tool args failed validation against the tool's input schema.
    #[error("tool '{tool}' arguments invalid: {detail}")]
    InvalidArgs {
        /// Tool that rejected the args.
        tool: String,
        /// Reason the args were rejected.
        detail: String,
    },
    /// Tool execution failed for an implementation-specific reason.
    #[error("tool '{tool}' execution failed: {detail}")]
    Execution {
        /// Tool that errored.
        tool: String,
        /// Implementation-supplied message.
        detail: String,
    },
}

/// Executable tool the agent loop dispatches.
#[async_trait]
pub trait AgentTool: Send + Sync {
    /// Stable name matching [`ToolDef::name`].
    fn name(&self) -> &str;

    /// Tool declaration that providers receive.
    fn def(&self) -> &ToolDef;

    /// Executes the tool with raw provider-emitted JSON arguments.
    async fn call(&self, args: &RawValue) -> Result<Box<RawValue>, AgentToolError>;
}

/// Concrete registry mirroring [`skald_runtime::ProviderRegistry`].
#[derive(Clone, Default)]
pub struct ToolRegistry {
    inner: HashMap<String, Arc<dyn AgentTool>>,
}

impl ToolRegistry {
    /// Creates an empty tool registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers or replaces a tool keyed by [`AgentTool::name`].
    pub fn register(&mut self, tool: Arc<dyn AgentTool>) {
        self.inner.insert(tool.name().to_owned(), tool);
    }

    /// Returns the registered tool for `name`, if any.
    pub fn get(&self, name: &str) -> Option<Arc<dyn AgentTool>> {
        self.inner.get(name).map(Arc::clone)
    }

    /// Returns the registered tool names.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.inner.keys().map(String::as_str)
    }

    /// Returns the number of registered tools.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns true when no tools are registered.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}
