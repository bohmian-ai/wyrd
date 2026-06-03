use serde::{Deserialize, Serialize};

use skald_tool::{ToolDef, ToolError};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
struct EchoInput {
    text: String,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
struct EchoOutput {
    echoed: String,
    version: String,
}

fn build_echo_tool(name: &str, version: &'static str) -> Arc<dyn skald_tool::AgentTool> {
    let name = name.to_owned();
    let version = version.to_owned();
    Arc::new(ToolDef::function(
        name,
        "echoes a provided text argument",
        move |input: EchoInput| -> Result<_, ToolError> {
            Ok(EchoOutput {
                echoed: input.text,
                version: version.clone(),
            })
        },
    ))
}

pub fn echo_tool(name: &str) -> Arc<dyn skald_tool::AgentTool> {
    build_echo_tool(name, "v1")
}
