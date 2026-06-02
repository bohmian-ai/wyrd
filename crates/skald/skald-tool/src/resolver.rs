use std::sync::Arc;

use crate::registry::ToolRegistry;
use crate::toolerror::ToolError;
use crate::trait_::AgentTool;

/// Resolver abstraction for looking up executable tools by name.
pub trait ToolResolver: Send + Sync {
    fn resolve(&self, name: &str) -> Result<Arc<dyn AgentTool>, ToolError>;
}

impl ToolResolver for ToolRegistry {
    fn resolve(&self, name: &str) -> Result<Arc<dyn AgentTool>, ToolError> {
        ToolRegistry::resolve(self, name)
    }
}

impl<T: ToolResolver + ?Sized> ToolResolver for &T {
    fn resolve(&self, name: &str) -> Result<Arc<dyn AgentTool>, ToolError> {
        (**self).resolve(name)
    }
}
