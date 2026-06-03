//! Callback type aliases and context snapshots for agent runs.

use std::any::Any;
use std::sync::Arc;

use skald_spec::{ProviderRequest, ProviderResponse};
use skald_tool::{AgentTool, ToolError};

use crate::conversation::Conversation;
use crate::error::{AgentError, AgentResult};

/// Outcome a callback can return to control its associated operation.
pub enum CallbackOutcome<T> {
    /// Let the original operation run.
    Continue,
    /// Skip the original operation.
    Skip,
    /// Replace the value the original operation would have returned.
    ReplaceWith(T),
}

/// Result of applying a callback chain in registration order.
pub(crate) enum ChainResult<T> {
    /// A callback skipped the current operation.
    Skip,
    /// Effective current value after all callbacks continue or replace.
    Replaced(T),
}

/// Snapshot passed to every agent callback.
#[derive(Clone)]
pub struct AgentContext {
    /// Stable agent id.
    pub agent_id: String,
    /// Optional session id for the current run.
    pub session_id: Option<String>,
    /// Current loop iteration.
    pub iteration: u32,
    /// Read-only conversation snapshot for the callback fire point.
    pub conversation: Arc<Conversation>,
}

/// Callback fired before the agent run begins.
pub type BeforeAgentFn = Arc<dyn Fn(&AgentContext, &str) -> CallbackOutcome<String> + Send + Sync>;

/// Callback fired after the agent run completes.
pub type AfterAgentFn = Arc<
    dyn Fn(&AgentContext, &crate::run::AgentRun) -> CallbackOutcome<crate::run::AgentRun>
        + Send
        + Sync,
>;

/// Callback fired before one provider request is sent.
pub type BeforeModelFn =
    Arc<dyn Fn(&AgentContext, &ProviderRequest) -> CallbackOutcome<ProviderRequest> + Send + Sync>;

/// Callback fired after one provider response is received.
pub type AfterModelFn = Arc<
    dyn Fn(&AgentContext, &ProviderResponse) -> CallbackOutcome<ProviderResponse> + Send + Sync,
>;

/// Callback fired before one tool invocation.
pub type BeforeToolFn = Arc<
    dyn Fn(&AgentContext, &dyn AgentTool, &serde_json::Value) -> CallbackOutcome<serde_json::Value>
        + Send
        + Sync,
>;

/// Callback fired after one tool invocation.
pub type AfterToolFn = Arc<
    dyn Fn(
            &AgentContext,
            &dyn AgentTool,
            &Result<serde_json::Value, skald_tool::ToolError>,
        ) -> CallbackOutcome<Result<serde_json::Value, skald_tool::ToolError>>
        + Send
        + Sync,
>;

pub(crate) fn apply_chain_with_panic_catch<T, F>(
    chain: &[Arc<F>],
    ctx: &AgentContext,
    value: T,
    hook: &'static str,
) -> AgentResult<ChainResult<T>>
where
    F: Fn(&AgentContext, &T) -> CallbackOutcome<T> + ?Sized,
{
    let mut current = value;
    for callback in chain {
        let outcome =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(ctx, &current)));
        match outcome {
            Ok(CallbackOutcome::Continue) => {}
            Ok(CallbackOutcome::Skip) => return Ok(ChainResult::Skip),
            Ok(CallbackOutcome::ReplaceWith(replacement)) => current = replacement,
            Err(payload) => return Err(callback_panic(hook, payload)),
        }
    }
    Ok(ChainResult::Replaced(current))
}

pub(crate) fn apply_before_agent_chain_with_panic_catch(
    chain: &[BeforeAgentFn],
    ctx: &AgentContext,
    value: String,
    hook: &'static str,
) -> AgentResult<ChainResult<String>> {
    let mut current = value;
    for callback in chain {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            callback(ctx, current.as_str())
        }));
        match outcome {
            Ok(CallbackOutcome::Continue) => {}
            Ok(CallbackOutcome::Skip) => return Ok(ChainResult::Skip),
            Ok(CallbackOutcome::ReplaceWith(replacement)) => current = replacement,
            Err(payload) => return Err(callback_panic(hook, payload)),
        }
    }
    Ok(ChainResult::Replaced(current))
}

pub(crate) fn apply_chain_with_panic_catch_tool(
    chain: &[BeforeToolFn],
    ctx: &AgentContext,
    tool: &dyn AgentTool,
    args: serde_json::Value,
    hook: &'static str,
) -> AgentResult<ChainResult<serde_json::Value>> {
    let mut current = args;
    for callback in chain {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            callback(ctx, tool, &current)
        }));
        match outcome {
            Ok(CallbackOutcome::Continue) => {}
            Ok(CallbackOutcome::Skip) => return Ok(ChainResult::Skip),
            Ok(CallbackOutcome::ReplaceWith(replacement)) => current = replacement,
            Err(payload) => return Err(callback_panic(hook, payload)),
        }
    }
    Ok(ChainResult::Replaced(current))
}

pub(crate) fn apply_chain_with_panic_catch_tool_result(
    chain: &[AfterToolFn],
    ctx: &AgentContext,
    tool: &dyn AgentTool,
    result: Result<serde_json::Value, ToolError>,
    hook: &'static str,
) -> AgentResult<ChainResult<Result<serde_json::Value, ToolError>>> {
    let mut current = result;
    for callback in chain {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            callback(ctx, tool, &current)
        }));
        match outcome {
            Ok(CallbackOutcome::Continue) => {}
            Ok(CallbackOutcome::Skip) => return Ok(ChainResult::Skip),
            Ok(CallbackOutcome::ReplaceWith(replacement)) => current = replacement,
            Err(payload) => return Err(callback_panic(hook, payload)),
        }
    }
    Ok(ChainResult::Replaced(current))
}

fn callback_panic(hook: &'static str, payload: Box<dyn Any + Send>) -> AgentError {
    AgentError::CallbackPanic {
        hook: hook.to_owned(),
        payload: extract_panic_message(payload),
    }
}

fn extract_panic_message(payload: Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else if let Some(message) = payload.downcast_ref::<&'static str>() {
        (*message).to_owned()
    } else {
        "<non-string panic payload>".to_owned()
    }
}
