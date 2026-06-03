//! Agent-as-tool adapter for inline sub-agent delegation.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use skald_runtime::ProviderRegistry;
use skald_tool::{AgentTool, ToolError};

use crate::agent::Agent;
use crate::delegation::{
    DELEGATION_DEPTH_CAP, WYRD_AGENT_DELEGATION_CHAIN, WYRD_AGENT_DELEGATION_DEPTH,
};
use crate::error::AgentError;

/// Tool adapter that delegates one invocation to a sub-agent run.
///
/// The adapter stores its tool metadata directly and implements
/// [`AgentTool`] without wrapping a `ToolDef`.
pub struct AgentDelegateTool {
    agent: Arc<Agent>,
    providers: Arc<ProviderRegistry>,
    name: String,
    description: String,
    input_schema: Value,
    output_schema: Value,
}

impl AgentDelegateTool {
    /// Constructs a typed delegate tool for `agent`.
    pub fn new(agent: Arc<Agent>, providers: Arc<ProviderRegistry>) -> Self {
        let name = agent.id.clone();
        let description = format!("Delegate to agent {name}");
        let input_schema = json!({
            "type": "object",
            "properties": {
                "input": {
                    "type": "string"
                }
            },
            "required": ["input"]
        });
        let output_schema = sub_agent_output_schema(&agent).unwrap_or_else(|| {
            json!({
                "type": "string"
            })
        });

        Self {
            agent,
            providers,
            name,
            description,
            input_schema,
            output_schema,
        }
    }

    /// Overrides the parent-facing tool description before trait erasure.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Erases this adapter to an executable agent tool.
    pub fn into_tool(self) -> Arc<dyn AgentTool> {
        Arc::new(self)
    }

    /// Constructs and erases a delegate tool with the default description.
    pub fn from_agent(agent: Arc<Agent>, providers: Arc<ProviderRegistry>) -> Arc<dyn AgentTool> {
        Self::new(agent, providers).into_tool()
    }
}

#[async_trait]
impl AgentTool for AgentDelegateTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn input_schema(&self) -> Value {
        self.input_schema.clone()
    }

    fn output_schema(&self) -> Value {
        self.output_schema.clone()
    }

    async fn invoke(&self, args: Value) -> Result<Value, ToolError> {
        let depth = WYRD_AGENT_DELEGATION_DEPTH
            .try_with(|depth| *depth)
            .unwrap_or(0);
        let mut chain = WYRD_AGENT_DELEGATION_CHAIN
            .try_with(|chain| chain.borrow().clone())
            .unwrap_or_default();

        if depth + 1 > DELEGATION_DEPTH_CAP {
            chain.push(self.agent.id.clone());
            let agent_err = AgentError::DelegationDepthExceeded { chain };
            let detail = agent_err.to_string();
            return Err(ToolError::Invocation {
                detail,
                cause: Some(Box::new(agent_err)),
            });
        }

        let input_str = args
            .get("input")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                ToolError::InvalidInput(
                    "missing 'input' field (AgentDelegateTool requires string input)".to_owned(),
                )
            })?;

        chain.push(self.agent.id.clone());
        let result = WYRD_AGENT_DELEGATION_DEPTH
            .scope(depth + 1, async {
                WYRD_AGENT_DELEGATION_CHAIN
                    .scope(std::cell::RefCell::new(chain), async {
                        self.agent.run(&self.providers, None, &input_str).await
                    })
                    .await
            })
            .await;

        match result {
            Ok(run) => Ok(Value::String(run.output)),
            Err(error) => Err(map_agent_error(error)),
        }
    }
}

fn map_agent_error(error: AgentError) -> ToolError {
    let detail = error.to_string();
    ToolError::Invocation {
        detail,
        cause: Some(Box::new(error)),
    }
}

fn sub_agent_output_schema(_agent: &Agent) -> Option<Value> {
    None
}
