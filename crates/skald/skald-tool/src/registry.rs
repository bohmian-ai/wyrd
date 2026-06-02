//! Tool registry and process-wide default registry for Skald tools.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

// parking_lot is intentionally not added here to avoid a new dependency.
// std::sync::RwLock is acceptable because registry access is infrequent:
// writes occur at startup/import time; reads occur only during agent loading.
use crate::toolerror::ToolError;
use crate::trait_::AgentTool;

/// In-memory registry of executable tools keyed by tool name.
pub struct ToolRegistry {
    inner: RwLock<HashMap<String, Arc<dyn AgentTool>>>,
}

impl ToolRegistry {
    /// Creates a new, empty tool registry.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
        }
    }

    /// Register a tool name uniquely.
    ///
    /// Returns `SKALD_TOOL_409_NAME_TAKEN` if a tool with the same name
    /// is already registered.
    pub fn register(&self, tool: Arc<dyn AgentTool>) -> Result<(), ToolError> {
        let name = tool.name().to_string();
        let mut guard = self.inner.write().expect("ToolRegistry lock poisoned");

        if guard.contains_key(&name) {
            return Err(ToolError::NameTaken { name });
        }

        guard.insert(name, tool);
        Ok(())
    }

    /// Register a tool name, replacing any existing registration.
    ///
    /// This is intended for fixture and test code.
    pub fn register_force(&self, tool: Arc<dyn AgentTool>) {
        let name = tool.name().to_string();
        let mut guard = self.inner.write().expect("ToolRegistry lock poisoned");
        guard.insert(name, tool);
    }

    /// Resolve a tool by name.
    ///
    /// Returns `SKALD_TOOL_404_NOT_REGISTERED` when missing, with sorted names.
    pub fn resolve(&self, name: &str) -> Result<Arc<dyn AgentTool>, ToolError> {
        let guard = self.inner.read().expect("ToolRegistry lock poisoned");
        if let Some(tool) = guard.get(name) {
            return Ok(Arc::clone(tool));
        }

        let mut available: Vec<String> = guard.keys().cloned().collect();
        drop(guard);
        available.sort();
        Err(ToolError::NotRegistered {
            name: name.to_string(),
            available,
        })
    }

    /// List all registered tool names in sorted order.
    pub fn names(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .inner
            .read()
            .expect("ToolRegistry lock poisoned")
            .keys()
            .cloned()
            .collect();
        out.sort();
        out
    }

    /// Clears all registrations.
    ///
    /// Existing resolved `Arc<dyn AgentTool>` values remain valid.
    pub fn clear(&self) {
        let mut guard = self.inner.write().expect("ToolRegistry lock poisoned");
        guard.clear();
    }
}

/// Process-global tool registry used by default resolution paths.
pub fn default_registry() -> &'static ToolRegistry {
    static REGISTRY: OnceLock<ToolRegistry> = OnceLock::new();
    REGISTRY.get_or_init(ToolRegistry::new)
}
