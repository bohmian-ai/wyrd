use async_trait::async_trait;
use serde_json::Value;

use crate::toolerror::ToolError;

/// Executable JSON-in / JSON-out tool contract used by Skald agents.
#[async_trait]
pub trait AgentTool: Send + Sync + 'static {
    /// Stable tool name used in provider-native tool calls.
    fn name(&self) -> &str;

    /// Human-readable tool description sent to providers.
    fn description(&self) -> &str;

    /// JSON Schema for arguments accepted by this tool.
    fn input_schema(&self) -> Value;

    /// JSON Schema for values returned by this tool.
    fn output_schema(&self) -> Value;

    /// Invokes the tool with provider-emitted JSON arguments.
    async fn invoke(&self, args: Value) -> Result<Value, ToolError>;
}
