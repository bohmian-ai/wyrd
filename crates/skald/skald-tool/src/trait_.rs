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

    /// Invokes the tool for the provider tool call `call_id`.
    ///
    /// The Agent loop calls this, so a tool whose effect must stay
    /// idempotent across a repeated provider call can key it by the call's
    /// identity. Every other tool ignores it and runs [`Self::invoke`].
    ///
    /// # Errors
    /// Returns the tool's [`ToolError`].
    async fn invoke_call(&self, call_id: &str, args: Value) -> Result<Value, ToolError> {
        let _ = call_id;
        self.invoke(args).await
    }
}
